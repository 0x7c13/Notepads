// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE file in the project root for license information.
// ---------------------------------------------------------------------------------------------

//! Views of a real exact allocation. Lazy allocation remains outside a window.

use super::ExactRows;
use crate::error::CoreError;
use crate::memory::{TaskAbandon, TaskStep, TaskValue, TaskWindow};
use std::cell::Cell;
use std::marker::PhantomData;

pub(crate) struct TaskExactWrite<'scope, T: TaskValue> {
    values: Option<&'scope mut Vec<T>>,
    count: usize,
    fault: &'scope Cell<Option<CoreError>>,
    scope: PhantomData<fn(&'scope ()) -> &'scope ()>,
}

impl<'scope> TaskWindow<'scope, '_> {
    pub(crate) fn write_exact<T: TaskValue>(
        &mut self,
        rows: &'scope mut ExactRows<T>,
        owner_fault: &'scope Cell<Option<CoreError>>,
        abandon: TaskAbandon,
    ) -> Result<TaskStep<TaskExactWrite<'scope, T>>, CoreError> {
        self.value::<T>()?;
        if !self.binding::<ExactRows<T>>()? {
            return Ok(TaskStep::Full);
        }
        self.parts().ensure_budget(&rows.budget)?;
        if matches!(self.owner(owner_fault, abandon)?, TaskStep::Full) {
            return Ok(TaskStep::Full);
        }
        if rows.data.is_none() && rows.count != 0 {
            // No original push fee or logical write has occurred. The driver
            // must use its whole original lazy-allocation path before retrying.
            return Ok(TaskStep::NeedCapacity(rows.count));
        }
        Ok(TaskStep::Ready(TaskExactWrite {
            values: rows.data.as_mut().map(|data| &mut data.values),
            count: rows.count,
            fault: owner_fault,
            scope: PhantomData,
        }))
    }
}

fn row_bytes<T: TaskValue>() -> u64 {
    (4 * size_of::<T>() + 2 * size_of::<TaskExactWrite<'_, T>>()) as u64
}

impl<'scope, T: TaskValue> TaskExactWrite<'scope, T> {
    /// One indivisible append and 1 -> 2 state transition. Every fallible
    /// validation and original fee precedes both physical writes.
    pub(crate) fn push_and_mark(
        &mut self,
        row: T,
        states: &mut TaskExactWrite<'scope, u8>,
        index: usize,
        window: &mut TaskWindow<'scope, '_>,
    ) -> Result<TaskStep<()>, CoreError> {
        if !window.action(row_bytes::<T>() + row_bytes::<u8>())? {
            return Ok(TaskStep::Full);
        }
        let result = (|| {
            window.parts().event()?;
            let len = self.values.as_ref().map_or(0, |values| values.len());
            if len >= self.count || len != index {
                return Err(window.fail(CoreError::InvalidProvenance));
            }
            let values = self
                .values
                .as_mut()
                .ok_or_else(|| window.fail(CoreError::InternalInvariant))?;
            window.parts().copy(size_of::<T>() as u64)?;
            window.parts().event()?;
            if values.len() == values.capacity() {
                return Err(window.fail(CoreError::InternalInvariant));
            }
            window.parts().event()?;
            let slot = states
                .values
                .as_mut()
                .and_then(|values| values.get_mut(index))
                .ok_or_else(|| window.fail(CoreError::InvalidProvenance))?;
            if *slot != 1 {
                return Err(window.fail(CoreError::InvalidProvenance));
            }
            window.parts().copy(1)?;
            values.push(row);
            *slot = 2;
            Ok(TaskStep::Ready(()))
        })();
        let result = states.latch(result);
        self.latch(result)
    }

    fn latch<R>(&self, result: Result<R, CoreError>) -> Result<R, CoreError> {
        if let Err(error) = result
            && self.fault.get().is_none()
        {
            self.fault.set(Some(error));
        }
        result
    }
}
