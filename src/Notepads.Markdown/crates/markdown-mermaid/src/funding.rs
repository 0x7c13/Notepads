// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE file in the project root for license information.
// ---------------------------------------------------------------------------------------------

//! One real Core job grant under every controlled flowchart allocation.
//! The trusted Windows owner keeps the job until its actual handle is signaled.
//! These values count requested Layout capacity, not platform/driver RSS.

use crate::{DiagramError, canonical::CanonicalError, native_common};
use merman_core::OperationControl;
use markdown_memory::{AllocationAuthority, Family, MemoryError, MemoryScope};
use markdown_core::{
    model::AcceptedModel,
    resources::derived::{ControlPolicy, JobGrant, RetainedGrant},
};
use std::{
    alloc::Layout,
    sync::{Arc, Mutex, MutexGuard},
};

const FAMILIES: usize = 9;
pub const MAX_STACK_RESERVATION: u64 = 8 * 1024 * 1024;

#[derive(Clone, Copy, Debug, Default)]
pub struct FundingUsage {
    pub live_rust_bytes: u64,
    pub peak_requested_bytes: u64,
    pub native_charged_bytes: u64,
    pub live_by_family: [u64; FAMILIES],
    pub allocation_attempts: u64,
    pub work_units: u64,
}

struct Ledger {
    usage: FundingUsage,
    grant: Option<JobGrant>,
    first: Option<MemoryError>,
    finalized: bool,
}

pub struct JobMemory {
    ledger: Mutex<Ledger>,
    // MUST be the original unscoped/unregioned control. A scoped control here
    // would produce Authority -> Control -> Scope/Region -> Authority cycles.
    control: OperationControl,
    paid_metadata: u64,
}

impl JobMemory {
    /// Caller already owns real queued metadata for the small Arc/control
    /// initialization blocks. Full 64MiB admission happens before Arc creation.
    pub fn activate(
        model: &AcceptedModel,
        policy: &ControlPolicy,
        paid_metadata: u64,
        stack_reservation: u64,
        unscoped_control: OperationControl,
    ) -> Result<Arc<Self>, DiagramError> {
        let header_upper =
            (size_of::<Self>() + 2 * size_of::<usize>() + 2 * align_of::<Self>()) as u64;
        if paid_metadata < header_upper
            || paid_metadata > 16 * 1024
            || stack_reservation == 0
            || stack_reservation > MAX_STACK_RESERVATION
            || unscoped_control.memory_scope().is_some()
            || unscoped_control.memory_region().is_some()
        {
            return Err(DiagramError::InvalidFunding);
        }
        unscoped_control
            .checkpoint()
            .map_err(|_| native_common::canonical(CanonicalError::Cancelled, &unscoped_control))?;
        let grant = policy.acquire_job(model).map_err(DiagramError::Funding)?;
        let initial = paid_metadata
            .checked_add(stack_reservation)
            .ok_or(DiagramError::InvalidFunding)?;
        if initial >= grant.reserved_bytes() {
            return Err(DiagramError::InvalidFunding);
        }
        Ok(Arc::new(Self {
            ledger: Mutex::new(Ledger {
                usage: FundingUsage {
                    native_charged_bytes: initial,
                    peak_requested_bytes: initial,
                    ..FundingUsage::default()
                },
                grant: Some(grant),
                first: None,
                finalized: false,
            }),
            control: unscoped_control,
            paid_metadata,
        }))
    }

    fn lock(&self) -> MutexGuard<'_, Ledger> {
        self.ledger
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
    }

    fn fail(&self, ledger: &mut Ledger, error: MemoryError) -> MemoryError {
        if ledger.first.is_none() {
            ledger.first = Some(error);
        }
        self.control.cancel();
        ledger.first.unwrap_or(error)
    }

    pub fn scope(self: &Arc<Self>) -> MemoryScope {
        MemoryScope::new(self.clone())
    }
    pub fn usage(&self) -> FundingUsage {
        self.lock().usage
    }
    pub fn failure(&self) -> Option<MemoryError> {
        self.lock().first
    }
    /// The narrow allocation interface carries only Cancelled. Recover its
    /// original reason from this authority's unscoped control before publishing
    /// a whole-job error. A recorded funded failure keeps its original priority.
    pub fn resolve_error(&self, error: DiagramError) -> DiagramError {
        let error = self.failure().map_or(error, DiagramError::Memory);
        match error {
            DiagramError::Cancelled | DiagramError::Memory(MemoryError::Cancelled) => {
                native_common::canonical(CanonicalError::Cancelled, &self.control)
            }
            _ => error,
        }
    }
    pub fn cancel(&self) {
        self.control.cancel();
    }

    /// Cumulative requested native blocks include failed allocation attempts
    /// and construction copies; old blocks cannot be silently reused as credit.
    /// The actual native allocator runs only after this call succeeds.
    pub fn charge_native(&self, bytes: u64) -> Result<(), DiagramError> {
        self.control
            .checkpoint()
            .map_err(|_| self.resolve_error(DiagramError::Cancelled))?;
        let mut ledger = self.lock();
        if let Some(error) = ledger.first {
            return Err(DiagramError::Memory(error));
        }
        let grant = ledger.grant.as_ref().ok_or(DiagramError::InvalidFunding)?;
        if ledger.finalized {
            return Err(DiagramError::InvalidFunding);
        }
        let next = ledger
            .usage
            .native_charged_bytes
            .checked_add(bytes)
            .ok_or(DiagramError::InvalidFunding)?;
        let total = next
            .checked_add(ledger.usage.live_rust_bytes)
            .ok_or(DiagramError::InvalidFunding)?;
        if total > grant.reserved_bytes() {
            return Err(DiagramError::Memory(
                self.fail(&mut ledger, MemoryError::BudgetLimited),
            ));
        }
        ledger.usage.native_charged_bytes = next;
        ledger.usage.peak_requested_bytes = ledger.usage.peak_requested_bytes.max(total);
        Ok(())
    }

    /// Private host protocol requirement (enforced at the native ABI boundary):
    /// Trusted native caller has observed the real worker handle signaled and
    /// destroyed every Rust payload, producer, scoped control and region.
    /// Still-live native buffers and their headers remain externally owned.
    /// A nonzero live count refuses shrink, including late COM callback owners.
    pub fn finalize_after_thread_exit(
        &self,
        actual_native: u64,
    ) -> Result<RetainedGrant, DiagramError> {
        self.control
            .checkpoint()
            .map_err(|_| self.resolve_error(DiagramError::Cancelled))?;
        let mut ledger = self.lock();
        if ledger.finalized
            || ledger.usage.live_rust_bytes != 0
            || actual_native < self.paid_metadata
            || actual_native > ledger.usage.native_charged_bytes
        {
            return Err(DiagramError::InvalidFunding);
        }
        if let Some(error) = ledger.first {
            return Err(DiagramError::Memory(error));
        }
        let retained = ledger
            .grant
            .as_mut()
            .ok_or(DiagramError::InvalidFunding)?
            .retain_result(actual_native)
            .map_err(DiagramError::Funding)?;
        // This is deliberately the first actual worker-permit release.
        drop(ledger.grant.take());
        ledger.finalized = true;
        Ok(retained)
    }

    /// Same private post-signaled protocol as finalization, for a terminal
    /// failure with no native payload or external authority left. Queued
    /// metadata remains owned by the enclosing context. In particular a late
    /// COM callback carrying a receipt prevents returning the worker grant.
    pub fn retire_after_thread_exit(&self) -> Result<(), DiagramError> {
        let mut ledger = self.lock();
        if ledger.finalized || ledger.usage.live_rust_bytes != 0 || ledger.grant.is_none() {
            return Err(DiagramError::InvalidFunding);
        }
        drop(ledger.grant.take());
        ledger.finalized = true;
        Ok(())
    }
}

impl AllocationAuthority for JobMemory {
    fn reserve(&self, layout: Layout, family: Family) -> Result<(), MemoryError> {
        if self.control.checkpoint().is_err() {
            return Err(MemoryError::Cancelled);
        }
        let mut ledger = self.lock();
        if let Some(error) = ledger.first {
            return Err(error);
        }
        if ledger.finalized || ledger.grant.is_none() {
            return Err(self.fail(&mut ledger, MemoryError::InvalidState));
        }
        let bytes = u64::try_from(layout.size())
            .map_err(|_| self.fail(&mut ledger, MemoryError::ArithmeticOverflow))?;
        let live = ledger
            .usage
            .live_rust_bytes
            .checked_add(bytes)
            .ok_or_else(|| self.fail(&mut ledger, MemoryError::ArithmeticOverflow))?;
        let total = ledger
            .usage
            .native_charged_bytes
            .checked_add(live)
            .ok_or_else(|| self.fail(&mut ledger, MemoryError::ArithmeticOverflow))?;
        if total > ledger.grant.as_ref().map_or(0, JobGrant::reserved_bytes) {
            return Err(self.fail(&mut ledger, MemoryError::BudgetLimited));
        }
        let index = family as usize;
        let family_live = ledger.usage.live_by_family[index]
            .checked_add(bytes)
            .ok_or_else(|| self.fail(&mut ledger, MemoryError::ArithmeticOverflow))?;
        let attempts = ledger
            .usage
            .allocation_attempts
            .checked_add(1)
            .ok_or_else(|| self.fail(&mut ledger, MemoryError::ArithmeticOverflow))?;
        ledger.usage.live_rust_bytes = live;
        ledger.usage.live_by_family[index] = family_live;
        ledger.usage.allocation_attempts = attempts;
        ledger.usage.peak_requested_bytes = ledger.usage.peak_requested_bytes.max(total);
        Ok(())
    }

    fn release(&self, layout: Layout, family: Family) {
        let mut ledger = self.lock();
        let bytes = layout.size() as u64;
        let index = family as usize;
        if ledger.usage.live_rust_bytes < bytes || ledger.usage.live_by_family[index] < bytes {
            self.fail(&mut ledger, MemoryError::InvalidState);
            return;
        }
        ledger.usage.live_rust_bytes -= bytes;
        ledger.usage.live_by_family[index] -= bytes;
    }

    fn failure(&self) -> Option<MemoryError> {
        JobMemory::failure(self)
    }
    fn record_failure(&self, error: MemoryError) {
        self.fail(&mut self.lock(), error);
    }
    fn before_allocate(&self, _: Layout, _: Family) -> Result<(), MemoryError> {
        if self.control.checkpoint().is_err() {
            Err(MemoryError::Cancelled)
        } else {
            Ok(())
        }
    }
    fn charge_work(&self, units: usize) -> Result<(), MemoryError> {
        if self.control.checkpoint().is_err() {
            return Err(MemoryError::Cancelled);
        }
        let mut ledger = self.lock();
        if let Some(error) = ledger.first {
            return Err(error);
        }
        let next = ledger
            .usage
            .work_units
            .checked_add(units as u64)
            .ok_or_else(|| self.fail(&mut ledger, MemoryError::ArithmeticOverflow))?;
        if next > 1_000_000 {
            return Err(self.fail(&mut ledger, MemoryError::WorkLimited));
        }
        ledger.usage.work_units = next;
        Ok(())
    }
}

#[cfg(test)]
#[path = "funding_tests.rs"]
mod tests;
