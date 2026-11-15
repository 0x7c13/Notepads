// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE file in the project root for license information.
// ---------------------------------------------------------------------------------------------

//! Existing flat allocations under a scoped task; allocation stays with drivers.

use super::{GrowingVec, TaskAbandon, TaskStep, TaskValue, TaskWindow};
use crate::error::CoreError;
use std::cell::Cell;
use std::marker::PhantomData;

pub(crate) struct TaskGrowingWrite<'scope, T: TaskValue> {
    values: &'scope mut [T],
    fault: &'scope Cell<Option<CoreError>>,
    scope: PhantomData<fn(&'scope ()) -> &'scope ()>,
}

impl<'scope> TaskWindow<'scope, '_> {
    pub(crate) fn write_growing<T: TaskValue>(
        &mut self,
        rows: &'scope mut GrowingVec<T>,
        owner_fault: &'scope Cell<Option<CoreError>>,
        abandon: TaskAbandon,
    ) -> Result<TaskStep<TaskGrowingWrite<'scope, T>>, CoreError> {
        self.value::<T>()?;
        if !self.binding::<GrowingVec<T>>()? {
            return Ok(TaskStep::Full);
        }
        self.parts().ensure_auxiliary(rows._lease.as_ref())?;
        if matches!(self.owner(owner_fault, abandon)?, TaskStep::Full) {
            return Ok(TaskStep::Full);
        }
        Ok(TaskStep::Ready(TaskGrowingWrite {
            values: &mut rows.values,
            fault: owner_fault,
            scope: PhantomData,
        }))
    }
}

fn row_bytes<T: TaskValue>() -> u64 {
    (4 * size_of::<T>() + 2 * size_of::<TaskGrowingWrite<'_, T>>()) as u64
}

impl<'scope, T: TaskValue> TaskGrowingWrite<'scope, T> {
    pub(crate) fn get(
        &self,
        index: usize,
        window: &mut TaskWindow<'scope, '_>,
    ) -> Result<TaskStep<Option<T>>, CoreError> {
        if !window.action(row_bytes::<T>())? {
            return Ok(TaskStep::Full);
        }
        Ok(TaskStep::Ready(self.values.get(index).copied()))
    }

    pub(crate) fn replace(
        &mut self,
        index: usize,
        value: T,
        window: &mut TaskWindow<'scope, '_>,
    ) -> Result<TaskStep<()>, CoreError> {
        if !window.action(row_bytes::<T>())? {
            return Ok(TaskStep::Full);
        }
        let result = (|| {
            window.parts().event()?;
            window.parts().copy(size_of::<T>() as u64)?;
            let slot = self
                .values
                .get_mut(index)
                .ok_or_else(|| window.fail(CoreError::InternalInvariant))?;
            *slot = value;
            Ok(TaskStep::Ready(()))
        })();
        if let Err(error) = result
            && self.fault.get().is_none()
        {
            self.fault.set(Some(error));
        }
        result
    }
}
