// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE file in the project root for license information.
// ---------------------------------------------------------------------------------------------

use super::MAX_EXTERNAL_EVENTS;
use crate::error::{CoreError, LimitKind};
use crate::policy::{CHECKPOINT_EVENTS, Operation};
use std::cell::{Cell, RefCell};
use syntect::parsing::{ExecutionMonitor, Stage, StopReason};

/// A stack-only reborrow of the caller's one independent highlight Operation.
/// No callback holds this borrow on return; no threads/FFI/client code can reenter it.
pub(super) struct Work<'a> {
    op: RefCell<&'a mut Operation>,
    events: Cell<u64>,
}

impl<'a> Work<'a> {
    pub fn new(op: &'a mut Operation) -> Self {
        Self {
            op: RefCell::new(op),
            events: Cell::new(0),
        }
    }
    pub fn events(&self) -> u64 {
        self.events.get()
    }
    pub fn with_op<T>(
        &self,
        action: impl FnOnce(&mut Operation) -> Result<T, CoreError>,
    ) -> Result<T, CoreError> {
        let mut op = self
            .op
            .try_borrow_mut()
            .map_err(|_| CoreError::InternalInvariant)?;
        action(&mut op)
    }
    pub fn fault(&self) -> Option<CoreError> {
        self.op.borrow().first_fault()
    }
    pub fn fail(&self, error: CoreError) -> CoreError {
        self.with_op(|op| Ok(op.fail(error)))
            .unwrap_or(CoreError::InternalInvariant)
    }
    pub fn checkpoint(&self, units: u32) -> Result<(), CoreError> {
        self.with_op(|op| self.charge(op, units))
    }
    pub fn charge(&self, op: &mut Operation, units: u32) -> Result<(), CoreError> {
        op.checkpoint_now()?;
        if units != 0 {
            let requested = self
                .events
                .get()
                .checked_add(u64::from(units))
                .ok_or_else(|| op.fail(CoreError::ArithmeticOverflow))?;
            op.check_limit(LimitKind::Work, requested, MAX_EXTERNAL_EVENTS)?;
            // This counter is functional admission only. The original Operation owns every fee.
            let mut remaining = u64::from(units);
            while remaining != 0 {
                let chunk = remaining.min(CHECKPOINT_EVENTS);
                op.external_events(chunk)?;
                self.events.set(self.events.get() + chunk);
                remaining -= chunk;
            }
        }
        Ok(())
    }
}

pub(super) struct Monitor<'a, 'op>(pub &'a Work<'op>);
impl ExecutionMonitor for Monitor<'_, '_> {
    fn checkpoint(&mut self, _: Stage, units: u32) -> Result<(), StopReason> {
        self.0.checkpoint(units).map_err(|error| match error {
            CoreError::Cancelled => StopReason::Cancelled,
            CoreError::DeadlineExceeded { .. } | CoreError::ClockFailure => {
                StopReason::DeadlineExceeded
            }
            CoreError::ResourceLimit {
                kind: LimitKind::Work,
                ..
            } => StopReason::WorkLimitExceeded,
            _ => StopReason::ResourceLimitExceeded,
        })
    }
}
