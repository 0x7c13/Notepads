// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE file in the project root for license information.
// ---------------------------------------------------------------------------------------------

//! One invariant construction identity and the original operation's only work authority.

use super::{
    CHECKPOINT_BYTES, CHECKPOINT_EVENTS, CoreError, Lease, LimitKind, MAX_METADATA_VIEW_BYTES,
    ModelLimits, Operation, ScalarBatchEnd, ScalarChargeOrder, SourceBudget, SourceLimits,
    WorkKind,
};
use std::marker::PhantomData;
use std::ops::ControlFlow;
use std::str::Chars;
use std::sync::Arc;

pub(crate) type ConstructionId<'id> = PhantomData<fn(&'id ()) -> &'id ()>;

pub(crate) struct OwnedWork<'id, 'op> {
    op: &'op mut Operation,
    _id: ConstructionId<'id>,
    completed: bool,
}

/// Production callers are audited: complete model construction and immutable
/// ParseInput's standalone preparation. No caller receives the original op.
pub(crate) fn construction_scope<'op, R>(
    op: &'op mut Operation,
    body: impl for<'id> FnOnce(OwnedWork<'id, 'op>) -> R,
) -> R {
    body(OwnedWork {
        op,
        _id: PhantomData,
        completed: false,
    })
}

impl<'id, 'op> OwnedWork<'id, 'op> {
    pub(crate) fn check(&mut self) -> Result<(), CoreError> {
        self.op.checkpoint()
    }

    pub(crate) fn checkpoint_now(&mut self) -> Result<(), CoreError> {
        self.op.checkpoint_now()
    }

    pub(crate) fn first_fault(&self) -> Option<CoreError> {
        self.op.first_fault()
    }

    #[cfg(test)]
    pub(crate) fn clock_read_count_for_test(&self) -> usize {
        self.op.clock_read_count_for_test()
    }

    #[cfg(test)]
    pub(crate) fn set_clock_samples_for_test(&mut self, samples: &[u64]) {
        self.op.set_clock_samples_for_test(samples);
    }

    #[cfg(test)]
    pub(crate) fn stats(&self) -> Option<super::WorkStats> {
        self.op.stats()
    }

    pub(crate) fn fail(&mut self, error: CoreError) -> CoreError {
        self.op.fail(error)
    }

    pub(crate) fn ensure_budget(&mut self, budget: &SourceBudget) -> Result<(), CoreError> {
        self.op.ensure_budget(budget)
    }

    pub(crate) fn limits(&self) -> SourceLimits {
        *self.op.budget().limits()
    }

    pub(crate) fn require_model_limits(&mut self) -> Result<ModelLimits, CoreError> {
        self.model_limits()
            .ok_or_else(|| self.fail(CoreError::UnsupportedConfiguration))
    }

    pub(crate) fn model_limits(&self) -> Option<ModelLimits> {
        self.op.budget().model_limits().copied()
    }

    /// Only final immutable IR/index owners retain the root for later,
    /// separately admitted queries. This never grants construction identity.
    pub(crate) fn frozen_budget(&self) -> SourceBudget {
        self.op.budget().clone()
    }

    pub(crate) fn uses_verified_index(&self) -> bool {
        self.op.uses_verified_index()
    }

    pub(crate) fn uses_task_access(&self) -> bool {
        self.op.uses_task_access()
    }

    pub(crate) fn scan(&mut self, bytes: u64) -> Result<(), CoreError> {
        self.op.scan(bytes)
    }

    pub(crate) fn scan_chunked(&mut self, bytes: u64) -> Result<(), CoreError> {
        self.op.scan_chunked(bytes)
    }

    pub(crate) fn copy(&mut self, bytes: u64) -> Result<(), CoreError> {
        self.op.copy(bytes)
    }

    pub(crate) fn events(&mut self, count: u64) -> Result<(), CoreError> {
        self.op.events(count)
    }

    pub(crate) fn view_scan(&mut self, bytes: u64) -> Result<(), CoreError> {
        self.op.view_scan(bytes)
    }

    pub(crate) fn view_copy(&mut self, bytes: u64) -> Result<(), CoreError> {
        self.op.view_copy(bytes)
    }

    /// One audited fixed owned access. The caller supplies no nested work,
    /// allocation, borrowed output or application callback.
    pub(crate) fn read_metadata<T: Copy>(
        &mut self,
        bytes: u64,
        read: impl FnOnce() -> Result<T, CoreError>,
    ) -> Result<T, CoreError> {
        self.op.read_metadata(bytes, read)
    }

    pub(crate) fn visit_scalar_batch(
        &mut self,
        input: &mut Chars<'_>,
        order: ScalarChargeOrder,
        visit: impl FnMut(char) -> Result<ControlFlow<()>, CoreError>,
    ) -> Result<ScalarBatchEnd, CoreError> {
        self.op.visit_scalar_batch(input, order, visit)
    }

    pub(crate) fn admit_model(&mut self, kind: LimitKind, amount: u64) -> Result<(), CoreError> {
        self.op.admit_model(kind, amount)
    }

    pub(crate) fn check_limit(
        &mut self,
        kind: LimitKind,
        requested: u64,
        limit: u64,
    ) -> Result<(), CoreError> {
        self.op.check_limit(kind, requested, limit)
    }

    pub(crate) fn reserve_auxiliary(&mut self, bytes: u64) -> Result<Lease, CoreError> {
        self.op.checkpoint()?;
        self.op
            .budget()
            .reserve_auxiliary(bytes)
            .map_err(|error| self.fail(error))
    }

    pub(crate) fn reserve_native(&mut self, bytes: u64) -> Result<NativeLease<'id>, CoreError> {
        self.checkpoint_now()?;
        Ok(NativeLease {
            lease: self.reserve_auxiliary(bytes)?,
            id: PhantomData,
        })
    }

    pub(crate) fn admit_native_actual(
        &mut self,
        lease: &mut NativeLease<'id>,
        bytes: u64,
    ) -> Result<(), CoreError> {
        self.checkpoint_now()?;
        if !Arc::ptr_eq(&lease.lease.quota, &self.op.budget.inner.auxiliary) {
            return Err(self.fail(CoreError::WrongBudget));
        }
        lease
            .lease
            .admit_actual(bytes)
            .map_err(|error| self.fail(error))?;
        self.checkpoint_now()
    }

    /// Only the actual physical-window driver borrows this authority. Its
    /// bounded actions and entry/exit checks are owned by the memory layer.
    pub(crate) fn borrow_window_charges(
        &mut self,
    ) -> Result<ConstructionCharges<'_, 'id, 'op>, CoreError> {
        self.checkpoint_now()?;
        let fast = self.op.uses_task_access() && self.op.stats.is_none();
        Ok(ConstructionCharges { work: self, fast })
    }

    /// Retire mutable construction before granting the immutable result.
    pub(crate) fn finish(mut self) -> Result<(), CoreError> {
        let result = self.checkpoint_now();
        self.completed = true;
        result
    }
}

impl Drop for OwnedWork<'_, '_> {
    fn drop(&mut self) {
        if !self.completed {
            self.op.fail(CoreError::InternalInvariant);
        }
    }
}

/// The actual nonclone native allocation reservation belongs to this exact
/// construction. Host must still free its allocation before dropping it.
pub(crate) struct NativeLease<'id> {
    lease: Lease,
    id: ConstructionId<'id>,
}

/// Borrow of the same work, not a second fault, budget, registration or task.
/// Callers must remain the audited finite physical-window driver and flat
/// record projections; allocation and arbitrary-op access are unavailable.
pub(crate) struct ConstructionCharges<'call, 'id, 'op> {
    work: &'call mut OwnedWork<'id, 'op>,
    fast: bool,
}

impl ConstructionCharges<'_, '_, '_> {
    pub(crate) fn check(&self) -> Result<(), CoreError> {
        self.work.op.fault.map_or(Ok(()), Err)
    }

    pub(crate) fn fail(&mut self, error: CoreError) -> CoreError {
        self.work.fail(error)
    }

    pub(crate) fn admit_model(&mut self, kind: LimitKind, amount: u64) -> Result<(), CoreError> {
        if !self.fast {
            return self.work.admit_model(kind, amount);
        }
        self.check()?;
        let model = self
            .work
            .model_limits()
            .ok_or_else(|| self.fail(CoreError::UnsupportedConfiguration))?;
        let (index, limit) = model
            .limit(kind)
            .ok_or_else(|| self.fail(CoreError::InternalInvariant))?;
        let requested = if kind == LimitKind::Depth {
            amount
        } else {
            self.work.op.admitted[index]
                .checked_add(amount)
                .ok_or_else(|| self.fail(CoreError::ArithmeticOverflow))?
        };
        if requested > limit {
            return Err(self.fail(CoreError::ResourceLimit {
                kind,
                requested,
                limit,
            }));
        }
        if kind != LimitKind::Depth {
            self.work.op.admitted[index] = requested;
        }
        Ok(())
    }

    pub(crate) fn event(&mut self) -> Result<(), CoreError> {
        self.fee(WorkKind::Events, 1)
    }

    pub(crate) fn scan(&mut self, count: u64) -> Result<(), CoreError> {
        self.fee(WorkKind::Scan, count)
    }

    pub(crate) fn copy(&mut self, count: u64) -> Result<(), CoreError> {
        self.fee(WorkKind::Copy, count)
    }

    pub(crate) fn view_scan(&mut self, count: u64) -> Result<(), CoreError> {
        self.fee(WorkKind::ViewScan, count)
    }

    pub(crate) fn view_copy(&mut self, count: u64) -> Result<(), CoreError> {
        self.fee(WorkKind::ViewCopy, count)
    }

    fn fee(&mut self, kind: WorkKind, count: u64) -> Result<(), CoreError> {
        if self.fast {
            self.check()?;
            let limit = match kind {
                WorkKind::Scan | WorkKind::Copy => CHECKPOINT_BYTES,
                WorkKind::Events => CHECKPOINT_EVENTS,
                WorkKind::ViewScan | WorkKind::ViewCopy => MAX_METADATA_VIEW_BYTES,
            };
            if count > limit {
                return Err(self.fail(CoreError::InternalInvariant));
            }
            return Ok(());
        }
        match kind {
            WorkKind::Scan => self.work.scan(count),
            WorkKind::Copy => self.work.copy(count),
            WorkKind::Events => self.work.events(count),
            WorkKind::ViewScan => self.work.view_scan(count),
            WorkKind::ViewCopy => self.work.view_copy(count),
        }
    }
}

#[cfg(test)]
mod tests;
