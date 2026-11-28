// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE file in the project root for license information.
// ---------------------------------------------------------------------------------------------

//! SVG work funding from the existing image control and captured model root.
//! This module records requested capacities; allocation owners and the FFI
//! ticket arena prove physical allocation/free separately. It owns no payload.

use super::{ImageResourcePolicy, RequestKey, RequestPermit, ResourceError, Shared};
use crate::error::{CoreError, LimitKind};
use crate::policy::{BoundaryReservation, CancellationToken, Operation, SourceBudget};
use std::sync::Arc;
use std::sync::atomic::Ordering;

mod retained;
pub use retained::{
    CoreRetainedFunding, RetainedBootstrap, SvgRetainedRetirement, SvgRetainedStats,
};

pub const MAX_SVG_JOB_BYTES: u64 = 128 * 1024 * 1024;
pub const MAX_SVG_CONTROL_BYTES: u64 = 128 * 1024 * 1024;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SvgWorkError {
    Source(CoreError),
    Resource(ResourceError),
    InvalidState,
    InvalidSize,
    ArithmeticOverflow,
    JobLimit { requested: u64, limit: u64 },
    ControlLimit { requested: u64, limit: u64 },
}

impl From<CoreError> for SvgWorkError {
    fn from(value: CoreError) -> Self {
        Self::Source(value)
    }
}

impl From<SvgWorkError> for CoreError {
    fn from(value: SvgWorkError) -> Self {
        match value {
            SvgWorkError::Source(error) => error,
            SvgWorkError::Resource(ResourceError::StaleResult) => Self::StaleResult,
            SvgWorkError::Resource(ResourceError::UnknownObject) => Self::UnknownId,
            SvgWorkError::Resource(ResourceError::ArithmeticOverflow)
            | SvgWorkError::ArithmeticOverflow => Self::ArithmeticOverflow,
            SvgWorkError::Resource(ResourceError::OutOfMemory) => Self::OutOfMemory,
            SvgWorkError::InvalidSize => Self::InvalidRange,
            SvgWorkError::JobLimit { requested, limit }
            | SvgWorkError::ControlLimit { requested, limit } => Self::ResourceLimit {
                kind: LimitKind::AuxiliaryCapacity,
                requested,
                limit,
            },
            SvgWorkError::Resource(_) | SvgWorkError::InvalidState => Self::InvalidState,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct SvgWorkStats {
    /// Includes this account's separately funded header and its aggregate work.
    pub job_bytes: u64,
    pub job_peak_bytes: u64,
    pub control_bytes: u64,
    /// Header is already funded by the outer Owned allocation's root lease.
    pub root_work_bytes: u64,
    pub header_bytes: u64,
    pub closed: bool,
}

/// Policy-only charge for the actual outer account allocation. Keep on the
/// bootstrap stack until commit, then move out before freeing that allocation.
/// Drop it only after the corresponding allocation was physically freed.
#[must_use]
pub struct SvgHeaderCharge {
    shared: Arc<Shared>,
    bytes: u64,
}

impl Drop for SvgHeaderCharge {
    fn drop(&mut self) {
        let mut state = self.shared.lock();
        state.svg_work_live = state
            .svg_work_live
            .checked_sub(self.bytes)
            .expect("live SVG header policy charge");
    }
}

/// Allocation-free provisional creation. Declare the outer allocation Slot
/// after this bootstrap, so any Slot failure frees its bytes before this guard.
#[must_use]
pub struct SvgWorkBootstrap<'a> {
    request: &'a RequestPermit,
    account: Option<SvgWorkAccount>,
    header: Option<SvgHeaderCharge>,
    committed: bool,
}

impl SvgWorkBootstrap<'_> {
    /// Infallible final step after the outer allocation Slot is ready.
    pub fn commit(mut self) -> (SvgWorkAccount, SvgHeaderCharge) {
        let account = self.account.take().expect("complete SVG bootstrap");
        let header = self.header.take().expect("funded SVG bootstrap header");
        self.request.svg_account.store(2, Ordering::Release);
        self.committed = true;
        (account, header)
    }
}

impl Drop for SvgWorkBootstrap<'_> {
    fn drop(&mut self) {
        if !self.committed {
            self.request.svg_account.store(0, Ordering::Release);
        }
    }
}

impl ImageResourcePolicy {
    /// Same-control SVG requested-live capacity, including retired-source work
    /// and separately funded account headers. The existing image ABI is unchanged.
    pub fn svg_work_bytes(&self) -> u64 {
        self.shared.lock().svg_work_live
    }
}

impl RequestPermit {
    /// The caller validates model/cancel same-root and source identity before
    /// passing that model's budget. This captures it permanently; subsequent
    /// work calls cannot substitute a policy-owned or foreign model root.
    pub fn begin_svg_work(
        &self,
        model_budget: &SourceBudget,
        cancel: CancellationToken,
        header_bytes: u64,
    ) -> Result<SvgWorkBootstrap<'_>, SvgWorkError> {
        if header_bytes == 0 {
            return Err(SvgWorkError::InvalidSize);
        }
        if header_bytes > MAX_SVG_JOB_BYTES {
            return Err(SvgWorkError::JobLimit {
                requested: header_bytes,
                limit: MAX_SVG_JOB_BYTES,
            });
        }
        self.svg_account
            .compare_exchange(0, 1, Ordering::AcqRel, Ordering::Acquire)
            .map_err(|_| SvgWorkError::InvalidState)?;
        let mut bootstrap = SvgWorkBootstrap {
            request: self,
            account: None,
            header: None,
            committed: false,
        };
        let mut op = model_budget.begin(cancel.clone());
        op.boundary_work().checkpoint_now()?;
        let grant = model_budget.reserve_resource_capacity(0)?;
        {
            let mut state = self.shared.lock();
            state.pending(self.key).map_err(SvgWorkError::Resource)?;
            let next = state
                .svg_work_live
                .checked_add(header_bytes)
                .ok_or(SvgWorkError::ArithmeticOverflow)?;
            check_control(next)?;
            state.svg_work_live = next;
        }
        bootstrap.header = Some(SvgHeaderCharge {
            shared: self.shared.clone(),
            bytes: header_bytes,
        });
        bootstrap.account = Some(SvgWorkAccount {
            shared: self.shared.clone(),
            key: self.key,
            budget: model_budget.clone(),
            cancel,
            grant: Some(grant),
            header_bytes,
            job_bytes: header_bytes,
            retained_borrow_bytes: 0,
            peak_bytes: header_bytes,
            closed: false,
        });
        op.boundary_work().checkpoint_now()?;
        Ok(bootstrap)
    }
}

/// Exclusive work funding, shared only through the existing control ledger.
/// FFI validates ticket identity/state and exact free before reducing this sum.
#[must_use]
pub struct SvgWorkAccount {
    shared: Arc<Shared>,
    key: RequestKey,
    budget: SourceBudget,
    cancel: CancellationToken,
    grant: Option<BoundaryReservation>,
    header_bytes: u64,
    job_bytes: u64,
    retained_borrow_bytes: u64,
    peak_bytes: u64,
    closed: bool,
}

impl SvgWorkAccount {
    /// Immutable original image request identity captured by this account.
    pub fn request_key(&self) -> RequestKey {
        self.key
    }
    /// One persistent engine operation shares the captured model's original
    /// cancellation token, work policy and elapsed protection window. Callers
    /// retain it across checkpoints instead of restarting its counters.
    pub fn begin_engine_operation(&self) -> Operation {
        self.budget.begin(self.cancel.clone())
    }

    pub fn reserve_work_bytes(&mut self, bytes: u64) -> Result<(), SvgWorkError> {
        if bytes == 0 {
            return Err(SvgWorkError::InvalidSize);
        }
        self.grow(bytes, true)
    }

    /// Reconcile one existing allocation. Shrinking only returns unused
    /// prepayment; caller must not shrink below its still-live physical bytes.
    /// Growth may fail after source retirement/cancellation. Failure leaves the
    /// previous amount funded, so the caller must free then release that amount.
    pub fn reconcile_work_bytes(&mut self, previous: u64, actual: u64) -> Result<(), SvgWorkError> {
        if previous == 0 || actual == 0 || previous > self.work_bytes() {
            return Err(SvgWorkError::InvalidSize);
        }
        if actual > previous {
            self.grow(actual - previous, false)
        } else {
            self.release_freed_work_bytes(previous - actual)
        }
    }

    fn grow(&mut self, bytes: u64, require_current: bool) -> Result<(), SvgWorkError> {
        self.grow_with(bytes, require_current, |grant, total, op| {
            grant.admit_actual(total, op)
        })
    }

    fn grow_with(
        &mut self,
        bytes: u64,
        require_current: bool,
        admit: impl FnOnce(&mut BoundaryReservation, u64, &mut Operation) -> Result<(), CoreError>,
    ) -> Result<(), SvgWorkError> {
        if require_current && self.closed {
            return Err(SvgWorkError::InvalidState);
        }
        let job_next = self
            .job_bytes
            .checked_add(bytes)
            .ok_or(SvgWorkError::ArithmeticOverflow)?;
        if job_next > MAX_SVG_JOB_BYTES {
            return Err(SvgWorkError::JobLimit {
                requested: job_next,
                limit: MAX_SVG_JOB_BYTES,
            });
        }
        let mut op = self.budget.begin(self.cancel.clone());
        op.boundary_work().checkpoint_now()?;
        {
            let mut state = self.shared.lock();
            if require_current {
                state.pending(self.key).map_err(SvgWorkError::Resource)?;
            }
            let control_next = state
                .svg_work_live
                .checked_add(bytes)
                .ok_or(SvgWorkError::ArithmeticOverflow)?;
            check_control(control_next)?;
            state.svg_work_live = control_next;
        }
        let previous_root = self.work_bytes();
        let root_next = previous_root
            .checked_add(bytes)
            .expect("work is included in checked job total");
        let result = admit(
            self.grant.as_mut().expect("live SVG root grant"),
            root_next,
            &mut op,
        );
        if let Err(error) = result {
            // admit_actual may have grown the lease before a final checkpoint
            // failed. No payload was allocated yet: restore its actual amount.
            self.grant
                .as_mut()
                .expect("live SVG root grant")
                .shrink_to(previous_root)
                .expect("root admission never shrinks an existing grant");
            let mut state = self.shared.lock();
            state.svg_work_live = state
                .svg_work_live
                .checked_sub(bytes)
                .expect("provisional SVG work claim");
            return Err(error.into());
        }
        self.job_bytes = job_next;
        self.peak_bytes = self.peak_bytes.max(job_next);
        Ok(())
    }

    /// Accounting acknowledgment, not a free operation. The owner must have
    /// already freed the exact bytes (or never allocated unused prepayment).
    /// Cleanup remains available after close, source retirement and cancellation.
    pub fn release_freed_work_bytes(&mut self, bytes: u64) -> Result<(), SvgWorkError> {
        let previous = self.work_bytes();
        let next = previous
            .checked_sub(bytes)
            .ok_or(SvgWorkError::InvalidSize)?;
        self.grant
            .as_mut()
            .expect("live SVG root grant")
            .shrink_to(next)?;
        let mut state = self.shared.lock();
        state.svg_work_live = state
            .svg_work_live
            .checked_sub(bytes)
            .expect("live SVG work policy charge");
        self.job_bytes -= bytes;
        Ok(())
    }

    pub fn close(&mut self) {
        self.closed = true;
    }

    pub fn is_current(&self) -> bool {
        self.shared.lock().pending(self.key).is_ok()
    }

    pub fn stats(&self) -> SvgWorkStats {
        SvgWorkStats {
            job_bytes: self.job_bytes,
            job_peak_bytes: self.peak_bytes,
            control_bytes: self.shared.lock().svg_work_live,
            root_work_bytes: self.work_bytes(),
            header_bytes: self.header_bytes,
            closed: self.closed,
        }
    }

    fn work_bytes(&self) -> u64 {
        self.grant
            .as_ref()
            .expect("live SVG root grant")
            .reserved_bytes()
    }
}

impl Drop for SvgWorkAccount {
    fn drop(&mut self) {
        if let Some(grant) = self.grant.take()
            && grant.reserved_bytes() != 0
        {
            // Caller misuse cannot prove payload free. Preserve both live
            // ledgers and their canonical roots instead of inventing retirement.
            std::mem::forget(grant);
            std::mem::forget(self.shared.clone());
        }
    }
}

fn check_control(requested: u64) -> Result<(), SvgWorkError> {
    if requested > MAX_SVG_CONTROL_BYTES {
        Err(SvgWorkError::ControlLimit {
            requested,
            limit: MAX_SVG_CONTROL_BYTES,
        })
    } else {
        Ok(())
    }
}

#[cfg(test)]
mod tests;
