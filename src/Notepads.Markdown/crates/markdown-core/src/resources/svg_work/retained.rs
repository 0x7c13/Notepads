// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE file in the project root for license information.
// ---------------------------------------------------------------------------------------------

//! Safe funding only: the FFI canonical owner proves physical state and free.

use super::{
    BoundaryReservation, MAX_SVG_JOB_BYTES, Operation, RequestKey, Shared, SourceBudget,
    SvgHeaderCharge, SvgWorkAccount, SvgWorkError, check_control,
};
use crate::resources::ByteReservation;
use std::sync::Arc;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct SvgRetainedStats {
    pub native_bytes: u64,
    pub preview_bytes: u64,
    pub header_bytes: u64,
    pub control_bytes: u64,
    pub root_live_bytes: u64,
    pub physical_live: bool,
}

/// No Native pointer, job, request permit, or per-borrow allocation is retained.
/// P+L's charge and the image clone leave this owner only after real Native free
/// and real preview-record deallocation. H's separate guard belongs to the FFI
/// Owned allocation and must outlive that allocation's physical deallocation.
#[must_use]
pub struct CoreRetainedFunding {
    shared: Arc<Shared>,
    key: RequestKey,
    budget: SourceBudget,
    image: Option<ByteReservation>,
    physical: Option<SvgHeaderCharge>,
    native_bytes: u64,
    preview_bytes: u64,
    header_bytes: u64,
    total_bytes: u64,
}

impl CoreRetainedFunding {
    pub fn budget(&self) -> &SourceBudget {
        &self.budget
    }

    pub fn key(&self) -> RequestKey {
        self.key
    }

    pub fn image(&self) -> Option<&ByteReservation> {
        self.image.as_ref()
    }

    pub fn total_bytes(&self) -> u64 {
        self.total_bytes
    }

    pub fn belongs_to(&self, reservation: &BoundaryReservation) -> bool {
        reservation.belongs_to(&self.budget)
    }

    pub fn matches_image(&self, image: &ByteReservation) -> bool {
        self.image
            .as_ref()
            .is_some_and(|held| Arc::ptr_eq(&held.owner, &image.owner))
    }

    pub fn stats(&self) -> SvgRetainedStats {
        SvgRetainedStats {
            native_bytes: self.native_bytes,
            preview_bytes: self.preview_bytes,
            header_bytes: self.header_bytes,
            control_bytes: self.shared.lock().svg_work_live,
            root_live_bytes: self.budget.usage().auxiliary_capacity,
            physical_live: self.physical.is_some(),
        }
    }

    /// Canonical physical-event acknowledgement, not a free operation. FFI must
    /// validate state before free, then call exactly once, after successful
    /// HeapFree and preview-record deallocation (or never-allocated rollback).
    /// Move this guard out of the record lock and drop it after unlocking: image
    /// retirement may reenter the same policy. No cancellation/current checks.
    pub fn take_physical_retirement(&mut self) -> SvgRetainedRetirement {
        SvgRetainedRetirement {
            _image: self.image.take(),
            _physical: self.physical.take().expect("canonical live retained owner"),
        }
    }
}

impl Drop for CoreRetainedFunding {
    fn drop(&mut self) {
        // Dropping a record alone cannot prove its external physical owner died.
        // Preserve live image/control charges on misuse rather than invent free.
        if let Some(physical) = self.physical.take() {
            std::mem::forget(physical);
            if let Some(image) = self.image.take() {
                std::mem::forget(image);
            }
        }
    }
}

/// Drop outside every FFI record/policy lock, after P and L's physical events.
#[must_use]
pub struct SvgRetainedRetirement {
    _image: Option<ByteReservation>,
    _physical: SvgHeaderCharge,
}

/// Declare this before both FFI Slots. Failed Slot allocations then physically
/// deallocate before this guard rolls back control and the creator's job claim.
#[must_use]
pub struct RetainedBootstrap<'a> {
    account: &'a mut SvgWorkAccount,
    funding: Option<CoreRetainedFunding>,
    header: Option<SvgHeaderCharge>,
    previous_peak: u64,
    bytes: u64,
    committed: bool,
}

impl RetainedBootstrap<'_> {
    pub fn budget(&self) -> &SourceBudget {
        &self.account.budget
    }

    pub fn operation(&self) -> Operation {
        self.account.budget.begin(self.account.cancel.clone())
    }

    pub fn key(&self) -> RequestKey {
        self.account.key
    }

    /// Infallible transfer after both real Slots and the creator row are ready.
    /// Creator P+L+H is already in account.retained_borrow_bytes().
    pub fn commit(mut self) -> (CoreRetainedFunding, SvgHeaderCharge) {
        let funding = self.funding.take().expect("complete retained bootstrap");
        let header = self.header.take().expect("funded retained header");
        self.committed = true;
        (funding, header)
    }
}

impl Drop for RetainedBootstrap<'_> {
    fn drop(&mut self) {
        if !self.committed {
            self.account
                .return_retained(self.bytes)
                .expect("provisional creator borrow");
            self.account.peak_bytes = self.previous_peak;
            if let Some(mut funding) = self.funding.take() {
                drop(funding.take_physical_retirement());
            }
            // H drops after the FFI Slot's actual deallocation, per declaration
            // order. No root reservation was added to the work aggregate.
        }
    }
}

impl SvgWorkAccount {
    /// Read-only provenance check before FFI allocates or grows its ticket arena.
    pub fn retained_root_matches(&self, reservation: &BoundaryReservation) -> bool {
        reservation.belongs_to(&self.budget)
    }

    /// Pure rejection must precede any FFI arena growth. prepare_retained still
    /// repeats admission at commit time because source/other jobs can advance.
    pub fn validate_retained_prepare(
        &self,
        image: Option<&ByteReservation>,
        native_bytes: u64,
        preview_bytes: u64,
        header_bytes: u64,
    ) -> Result<(), SvgWorkError> {
        if native_bytes == 0 || preview_bytes == 0 || header_bytes == 0 {
            return Err(SvgWorkError::InvalidSize);
        }
        if image.is_some_and(|image| {
            !Arc::ptr_eq(&self.shared, &image.owner.shared)
                || self.key.epoch != image.owner.key.epoch
                || self.key.object != image.owner.key.object
        }) {
            return Err(SvgWorkError::InvalidState);
        }
        let bytes = native_bytes
            .checked_add(preview_bytes)
            .and_then(|bytes| bytes.checked_add(header_bytes))
            .ok_or(SvgWorkError::ArithmeticOverflow)?;
        let mut op = self.budget.begin(self.cancel.clone());
        self.check_retained_borrow(bytes, &mut op)?;
        let state = self.shared.lock();
        state.pending(self.key).map_err(SvgWorkError::Resource)?;
        check_control(
            state
                .svg_work_live
                .checked_add(bytes)
                .ok_or(SvgWorkError::ArithmeticOverflow)?,
        )
    }

    /// No counters change. FFI uses this before allocating an arena, then the
    /// actual borrow repeats admission to catch source/cancellation races.
    pub fn validate_retained_borrow(
        &self,
        funding: &CoreRetainedFunding,
    ) -> Result<(), SvgWorkError> {
        if funding.physical.is_none()
            || !Arc::ptr_eq(&self.shared, &funding.shared)
            || self.key.epoch != funding.key.epoch
            || self.key.object != funding.key.object
            || !self.budget.same_root(&funding.budget)
        {
            return Err(SvgWorkError::InvalidState);
        }
        let mut op = self.budget.begin(self.cancel.clone());
        self.check_retained_borrow(funding.total_bytes, &mut op)?;
        self.shared
            .lock()
            .pending(self.key)
            .map_err(SvgWorkError::Resource)?;
        Ok(())
    }

    /// P=Native complete requested Layout, L=Owned<PreviewAllocation>,
    /// H=Owned<Retained>. Metadata uses image=None; payloads retain one clone of
    /// their original image reservation. FFI checks pending/admitted state and
    /// exact B separately; image capacity must never be compared against P.
    pub fn prepare_retained(
        &mut self,
        image: Option<&ByteReservation>,
        native_bytes: u64,
        preview_bytes: u64,
        header_bytes: u64,
    ) -> Result<RetainedBootstrap<'_>, SvgWorkError> {
        if native_bytes == 0 || preview_bytes == 0 || header_bytes == 0 {
            return Err(SvgWorkError::InvalidSize);
        }
        if image.is_some_and(|image| {
            !Arc::ptr_eq(&self.shared, &image.owner.shared)
                || self.key.epoch != image.owner.key.epoch
                || self.key.object != image.owner.key.object
        }) {
            return Err(SvgWorkError::InvalidState);
        }
        let physical_bytes = native_bytes
            .checked_add(preview_bytes)
            .ok_or(SvgWorkError::ArithmeticOverflow)?;
        let bytes = physical_bytes
            .checked_add(header_bytes)
            .ok_or(SvgWorkError::ArithmeticOverflow)?;
        let mut op = self.budget.begin(self.cancel.clone());
        let job_next = self.check_retained_borrow(bytes, &mut op)?;
        {
            let mut state = self.shared.lock();
            state.pending(self.key).map_err(SvgWorkError::Resource)?;
            let control_next = state
                .svg_work_live
                .checked_add(bytes)
                .ok_or(SvgWorkError::ArithmeticOverflow)?;
            check_control(control_next)?;
            state.svg_work_live = control_next;
        }
        let funding = CoreRetainedFunding {
            shared: self.shared.clone(),
            key: self.key,
            budget: self.budget.clone(),
            image: image.cloned(),
            physical: Some(SvgHeaderCharge {
                shared: self.shared.clone(),
                bytes: physical_bytes,
            }),
            native_bytes,
            preview_bytes,
            header_bytes,
            total_bytes: bytes,
        };
        let header = SvgHeaderCharge {
            shared: self.shared.clone(),
            bytes: header_bytes,
        };
        let previous_peak = self.peak_bytes;
        self.job_bytes = job_next;
        self.retained_borrow_bytes += bytes; // Included in the checked job total.
        self.peak_bytes = self.peak_bytes.max(job_next);
        let bootstrap = RetainedBootstrap {
            account: self,
            funding: Some(funding),
            header: Some(header),
            previous_peak,
            bytes,
            committed: false,
        };
        op.boundary_work().checkpoint_now()?;
        Ok(bootstrap)
    }

    /// FFI first validates admitted/live canonical state and same-job row
    /// uniqueness. This only charges this job, never the control or model root.
    pub fn borrow_retained(&mut self, funding: &CoreRetainedFunding) -> Result<u64, SvgWorkError> {
        if funding.physical.is_none()
            || !Arc::ptr_eq(&self.shared, &funding.shared)
            || self.key.epoch != funding.key.epoch
            || self.key.object != funding.key.object
            || !self.budget.same_root(&funding.budget)
        {
            return Err(SvgWorkError::InvalidState);
        }
        let bytes = funding.total_bytes;
        let mut op = self.budget.begin(self.cancel.clone());
        let job_next = self.check_retained_borrow(bytes, &mut op)?;
        self.shared
            .lock()
            .pending(self.key)
            .map_err(SvgWorkError::Resource)?;
        op.boundary_work().checkpoint_now()?;
        self.job_bytes = job_next;
        self.retained_borrow_bytes += bytes;
        self.peak_bytes = self.peak_bytes.max(job_next);
        Ok(bytes)
    }

    fn check_retained_borrow(&self, bytes: u64, op: &mut Operation) -> Result<u64, SvgWorkError> {
        if self.closed {
            return Err(SvgWorkError::InvalidState);
        }
        op.boundary_work().checkpoint_now()?;
        let next = self
            .job_bytes
            .checked_add(bytes)
            .ok_or(SvgWorkError::ArithmeticOverflow)?;
        if next > MAX_SVG_JOB_BYTES {
            return Err(SvgWorkError::JobLimit {
                requested: next,
                limit: MAX_SVG_JOB_BYTES,
            });
        }
        Ok(next)
    }

    /// FFI validates a canonical borrow row first. Returning an existing borrow
    /// does not inspect cancellation, current source, close, or policy poison.
    pub fn return_retained(&mut self, bytes: u64) -> Result<(), SvgWorkError> {
        if bytes == 0 || bytes > self.retained_borrow_bytes {
            return Err(SvgWorkError::InvalidSize);
        }
        self.retained_borrow_bytes -= bytes;
        self.job_bytes -= bytes;
        Ok(())
    }

    pub fn retained_borrow_bytes(&self) -> u64 {
        self.retained_borrow_bytes
    }
}
