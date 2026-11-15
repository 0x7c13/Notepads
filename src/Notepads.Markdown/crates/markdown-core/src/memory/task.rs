// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE file in the project root for license information.
// ---------------------------------------------------------------------------------------------

//! Generative physical access windows; no arbitrary action or cost callback.

use crate::error::CoreError;
use crate::policy::{CHECKPOINT_BYTES, CHECKPOINT_EVENTS, Operation, TaskOperation};
use std::cell::Cell;
use std::marker::PhantomData;

const OWNER_SLOTS: usize = 32;
const MAX_VALUE_BYTES: usize = 512;
// Fixed entry/close/drop work includes the owner array and an owned result.
// These are protection headroom, not relaxed numerical resource quotas.
const TRAILER_BYTES: u64 = 8192;
const TRAILER_STEPS: u64 = 64;

/// Internal audited registry: flat owned values, no references, pointers,
/// callbacks or allocations. No blanket implementation for arbitrary Copy.
pub(crate) trait TaskValue: Copy {}

impl TaskValue for () {}
impl TaskValue for bool {}
impl TaskValue for u8 {}
impl TaskValue for u32 {}
impl TaskValue for u64 {}
impl TaskValue for usize {}
impl TaskValue for std::cmp::Ordering {}
impl TaskValue for crate::identity::SourceSpan {}
impl<T: TaskValue> TaskValue for Option<T> {}
impl<A: TaskValue, B: TaskValue> TaskValue for (A, B) {}

#[derive(Debug, Eq, PartialEq)]
pub(crate) enum TaskStep<T> {
    Ready(T),
    Full,
    NeedCapacity(usize),
}

/// Only audited in-crate algorithms implement this, with a flat owned output.
/// The fresh scope cannot be stored in the algorithm's continuation.
pub(crate) trait TaskKernel {
    type Output: TaskValue;

    fn run<'scope>(
        &'scope mut self,
        window: &mut TaskWindow<'scope, '_>,
    ) -> Result<TaskStep<Self::Output>, CoreError>;
}

#[derive(Clone, Copy)]
pub(crate) enum TaskAbandon {
    InternalInvariant,
    InvalidProvenance,
}

#[derive(Clone, Copy)]
struct Owner<'scope> {
    fault: &'scope Cell<Option<CoreError>>,
    abandon: TaskAbandon,
}

impl Owner<'_> {
    fn fail(self, error: CoreError) {
        if self.fault.get().is_none() {
            self.fault.set(Some(error));
        }
    }

    fn abandon(self) {
        self.fail(match self.abandon {
            TaskAbandon::InternalInvariant => CoreError::InternalInvariant,
            TaskAbandon::InvalidProvenance => CoreError::InvalidProvenance,
        });
    }
}

pub(crate) struct TaskWindow<'scope, 'op> {
    parts: TaskOperation<'op>,
    bytes: u64,
    steps: u64,
    owners: [Option<Owner<'scope>>; OWNER_SLOTS],
    owner_len: usize,
    closed: bool,
    // Invariance keeps access views from being moved to a different scope.
    scope: PhantomData<fn(&'scope ()) -> &'scope ()>,
}

// Reserve enough for the fixed window/result transports and all 32 owner
// slots at close/drop. Payload actions never spend this trailer headroom.
const _: () = assert!(
    4 * size_of::<TaskWindow<'static, 'static>>()
        + 2 * size_of::<[Option<Owner<'static>>; OWNER_SLOTS]>()
        + 4 * MAX_VALUE_BYTES
        <= TRAILER_BYTES as usize
);
const _: () = assert!(2 * OWNER_SLOTS <= TRAILER_STEPS as usize);

pub(crate) fn run_task<K: TaskKernel>(
    kernel: &mut K,
    op: &mut Operation,
) -> Result<TaskStep<K::Output>, CoreError> {
    if size_of::<K::Output>() > MAX_VALUE_BYTES {
        return Err(op.fail(CoreError::InternalInvariant));
    }
    let (result, owners) = {
        let parts = op.borrow_task_parts()?;
        let mut window = TaskWindow {
            parts,
            bytes: 0,
            steps: 0,
            owners: [None; OWNER_SLOTS],
            owner_len: 0,
            closed: false,
            scope: PhantomData,
        };
        let result = kernel.run(&mut window);
        window.close(result)
    };
    // Every access view and the borrowed task parts have ended before the
    // original operation observes cancellation/time or a driver can allocate.
    let observed = op.checkpoint_now();
    let result = result.and_then(|value| observed.map(|()| value));
    if let Err(error) = result {
        for owner in owners.into_iter().flatten() {
            owner.fail(error);
        }
    }
    result
}

impl<'scope, 'op> TaskWindow<'scope, 'op> {
    pub(crate) fn fail(&mut self, error: CoreError) -> CoreError {
        self.parts.fail(error)
    }

    /// Register the actual private owner, not a new shadow fault slot.
    pub(crate) fn owner(
        &mut self,
        fault: &'scope Cell<Option<CoreError>>,
        abandon: TaskAbandon,
    ) -> Result<TaskStep<()>, CoreError> {
        self.parts.check()?;
        let checks = self.owner_len + 1;
        let bytes = (2 * checks * size_of::<Option<Owner<'_>>>()) as u64
            + (2 * size_of::<Option<CoreError>>()) as u64;
        if !self.action_cost(bytes, checks as u64)? {
            return Ok(TaskStep::Full);
        }
        if let Some(error) = fault.get() {
            return Err(self.fail(error));
        }
        for owner in self.owners[..self.owner_len].iter().flatten() {
            if std::ptr::eq(owner.fault, fault) {
                return Ok(TaskStep::Ready(()));
            }
        }
        if self.owner_len == OWNER_SLOTS {
            return Err(self.fail(CoreError::InternalInvariant));
        }
        self.owners[self.owner_len] = Some(Owner { fault, abandon });
        self.owner_len += 1;
        Ok(TaskStep::Ready(()))
    }

    fn close<T>(
        mut self,
        result: Result<TaskStep<T>, CoreError>,
    ) -> (
        Result<TaskStep<T>, CoreError>,
        [Option<Owner<'scope>>; OWNER_SLOTS],
    ) {
        let result = result.map_err(|error| self.fail(error));
        let result = self.parts.check().and(result);
        self.closed = true;
        (result, self.owners)
    }

    // These methods are visible only inside memory and its children. Algorithm
    // authors get the concrete access methods, never a cost/permit interface.
    pub(super) fn action(&mut self, bytes: u64) -> Result<bool, CoreError> {
        self.action_cost(bytes, 1)
    }

    fn action_cost(&mut self, bytes: u64, steps: u64) -> Result<bool, CoreError> {
        self.parts.check()?;
        if bytes > CHECKPOINT_BYTES - TRAILER_BYTES || steps > CHECKPOINT_EVENTS - TRAILER_STEPS {
            return Err(self.fail(CoreError::InternalInvariant));
        }
        if steps > CHECKPOINT_EVENTS - TRAILER_STEPS - self.steps
            || bytes > CHECKPOINT_BYTES - TRAILER_BYTES - self.bytes
        {
            return Ok(false);
        }
        self.bytes += bytes;
        self.steps += steps;
        Ok(true)
    }

    pub(super) fn binding<T>(&mut self) -> Result<bool, CoreError> {
        // Actual fixed headers/root handles are bounded by their real layout.
        if size_of::<T>() > MAX_VALUE_BYTES {
            return Err(self.fail(CoreError::InternalInvariant));
        }
        self.action((2 * size_of::<T>()) as u64)
    }

    pub(super) fn value<T: TaskValue>(&mut self) -> Result<(), CoreError> {
        if size_of::<T>() == 0 || size_of::<T>() > MAX_VALUE_BYTES {
            return Err(self.fail(CoreError::InternalInvariant));
        }
        Ok(())
    }

    pub(super) fn parts(&mut self) -> &mut TaskOperation<'op> {
        &mut self.parts
    }
}

impl Drop for TaskWindow<'_, '_> {
    fn drop(&mut self) {
        if !self.closed {
            self.parts.fail(CoreError::InternalInvariant);
            for owner in self.owners.iter().flatten() {
                owner.abandon();
            }
        }
    }
}
