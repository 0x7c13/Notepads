// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE file in the project root for license information.
// ---------------------------------------------------------------------------------------------

//! Borrowed task state. Actual bounded actions belong to the storage layer.

use super::{CoreError, Lease, Operation, SourceBudget, WorkKind};
use std::sync::Arc;

pub(crate) struct TaskOperation<'op> {
    budget: &'op SourceBudget,
    fault: &'op mut Option<CoreError>,
}

impl Operation {
    pub(crate) fn borrow_task_parts(&mut self) -> Result<TaskOperation<'_>, CoreError> {
        self.checkpoint_now()?;
        if !self.uses_task_access() || self.stats.is_some() {
            return Err(self.fail(CoreError::UnsupportedConfiguration));
        }
        Ok(TaskOperation {
            budget: &self.budget,
            fault: &mut self.fault,
        })
    }
}

impl TaskOperation<'_> {
    pub(crate) fn check(&self) -> Result<(), CoreError> {
        self.fault.map_or(Ok(()), Err)
    }

    pub(crate) fn fail(&mut self, error: CoreError) -> CoreError {
        *self.fault.get_or_insert(error)
    }

    pub(crate) fn ensure_budget(&mut self, budget: &SourceBudget) -> Result<(), CoreError> {
        self.check()?;
        if !self.budget.same_root(budget) {
            return Err(self.fail(CoreError::WrongBudget));
        }
        Ok(())
    }

    pub(crate) fn ensure_auxiliary(&mut self, lease: Option<&Lease>) -> Result<(), CoreError> {
        self.check()?;
        if let Some(lease) = lease
            && !Arc::ptr_eq(&self.budget.inner.auxiliary, &lease.quota)
        {
            return Err(self.fail(CoreError::WrongBudget));
        }
        Ok(())
    }

    pub(crate) fn ensure_source(&mut self, lease: &Lease) -> Result<(), CoreError> {
        self.check()?;
        if !Arc::ptr_eq(&self.budget.inner.source, &lease.quota) {
            return Err(self.fail(CoreError::WrongBudget));
        }
        Ok(())
    }

    /// Task access has no cumulative diagnostic state. Preserve the individual fee's
    /// checked width, while the enclosing actual action schedules protection.
    pub(crate) fn event(&mut self) -> Result<(), CoreError> {
        self.fee(WorkKind::Events, 1)
    }

    pub(crate) fn copy(&mut self, count: u64) -> Result<(), CoreError> {
        self.fee(WorkKind::Copy, count)
    }

    pub(crate) fn view_read(&mut self, count: u64) -> Result<(), CoreError> {
        self.fee(WorkKind::ViewScan, count)?;
        self.fee(WorkKind::ViewCopy, count)
    }

    fn fee(&mut self, kind: WorkKind, count: u64) -> Result<(), CoreError> {
        self.check()?;
        let limit = match kind {
            WorkKind::Scan | WorkKind::Copy => super::CHECKPOINT_BYTES,
            WorkKind::Events => super::CHECKPOINT_EVENTS,
            WorkKind::ViewScan | WorkKind::ViewCopy => super::MAX_METADATA_VIEW_BYTES,
        };
        if count > limit {
            return Err(self.fail(CoreError::InternalInvariant));
        }
        Ok(())
    }
}
