// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE file in the project root for license information.
// ---------------------------------------------------------------------------------------------

//! Finite physical accesses borrowing only the original construction work.

use crate::error::CoreError;
use crate::policy::{
    CHECKPOINT_BYTES, CHECKPOINT_EVENTS, ConstructionCharges, ConstructionId, OwnedWork,
};
use std::marker::PhantomData;
#[cfg(test)]
use std::num::NonZeroU64;

const MAX_VALUE_BYTES: usize = 512;
const TRAILER_BYTES: u64 = 8192;
const TRAILER_STEPS: u64 = 64;

/// Audited flat, owned records; no blanket Copy implementation or borrowed output.
pub(crate) trait BuildValue: Copy {}
pub(crate) trait BuildRecord: BuildValue {
    type Output: BuildValue;
    fn project(
        &self,
        index: usize,
        work: &mut ConstructionCharges<'_, '_, '_>,
    ) -> Result<Self::Output, CoreError>;
}
impl BuildValue for () {}
impl BuildValue for bool {}
impl BuildValue for u8 {}
impl BuildValue for u16 {}
impl BuildValue for u32 {}
impl BuildValue for u64 {}
impl BuildValue for usize {}
impl BuildValue for std::cmp::Ordering {}
impl BuildValue for crate::identity::SourceSpan {}
impl<T: BuildValue> BuildValue for Option<T> {}
impl<A: BuildValue, B: BuildValue> BuildValue for (A, B) {}

#[derive(Debug, Eq, PartialEq)]
pub(crate) enum BuildStep<T> {
    Ready(T),
    Full,
    NeedCapacity(usize),
}
pub(crate) trait BuildKernel<'id> {
    type Output: BuildValue;
    fn run<'scope>(
        &'scope mut self,
        window: &mut BuildWindow<'scope, '_, 'id, '_>,
    ) -> Result<BuildStep<Self::Output>, CoreError>;
}

pub(crate) struct BuildWindow<'scope, 'call, 'id, 'op> {
    parts: ConstructionCharges<'call, 'id, 'op>,
    bytes: u64,
    steps: u64,
    closed: bool,
    scope: ConstructionId<'scope>,
}
const _: () = assert!(
    4 * size_of::<BuildWindow<'static, 'static, 'static, 'static>>() + 4 * MAX_VALUE_BYTES
        <= TRAILER_BYTES as usize
);

pub(crate) fn run_window<'id, K: BuildKernel<'id>>(
    kernel: &mut K,
    work: &mut OwnedWork<'id, '_>,
) -> Result<BuildStep<K::Output>, CoreError> {
    if size_of::<K::Output>() > MAX_VALUE_BYTES {
        return Err(work.fail(CoreError::InternalInvariant));
    }
    let result = {
        let parts = work.borrow_window_charges()?;
        let mut window = BuildWindow {
            parts,
            bytes: 0,
            steps: 0,
            closed: false,
            scope: PhantomData,
        };
        let result = kernel.run(&mut window);
        // Lock typed Err while the guard still owns the original first-fault.
        window.close(result)
    };
    let observed = work.checkpoint_now();
    result.and_then(|value| observed.map(|()| value))
}

impl<'scope, 'call, 'id, 'op> BuildWindow<'scope, 'call, 'id, 'op> {
    pub(crate) fn fail(&mut self, error: CoreError) -> CoreError {
        self.parts.fail(error)
    }
    #[cfg(test)]
    pub(crate) fn keyed_value<T: BuildValue>(
        &mut self,
        expected: NonZeroU64,
        actual: NonZeroU64,
        value: T,
    ) -> Result<BuildStep<T>, CoreError> {
        self.value::<T>()?;
        if !self.action((4 * size_of::<T>() + 4 * size_of::<NonZeroU64>()) as u64)? {
            return Ok(BuildStep::Full);
        }
        self.parts.event()?;
        self.parts.scan(size_of::<NonZeroU64>() as u64)?;
        if actual != expected {
            return Err(self.fail(CoreError::InvalidProvenance));
        }
        Ok(BuildStep::Ready(value))
    }
    pub(crate) fn advance(&mut self) -> Result<BuildStep<()>, CoreError> {
        Ok(if self.action(0)? {
            BuildStep::Ready(())
        } else {
            BuildStep::Full
        })
    }
    pub(crate) fn event(&mut self) -> Result<BuildStep<()>, CoreError> {
        if !self.action(0)? {
            return Ok(BuildStep::Full);
        }
        self.parts.event()?;
        Ok(BuildStep::Ready(()))
    }
    pub(crate) fn admit_record(
        &mut self,
        kind: crate::error::LimitKind,
        amount: u64,
    ) -> Result<BuildStep<()>, CoreError> {
        if !self.action((2 * size_of::<crate::policy::ModelLimits>() + 16) as u64)? {
            return Ok(BuildStep::Full);
        }
        self.parts.admit_model(kind, amount)?;
        Ok(BuildStep::Ready(()))
    }
    fn close<T>(
        mut self,
        result: Result<BuildStep<T>, CoreError>,
    ) -> Result<BuildStep<T>, CoreError> {
        let result = result.map_err(|error| self.fail(error));
        let result = self.parts.check().and(result);
        self.closed = true;
        result
    }
    pub(in crate::memory) fn action(&mut self, bytes: u64) -> Result<bool, CoreError> {
        self.parts.check()?;
        if bytes > CHECKPOINT_BYTES - TRAILER_BYTES {
            return Err(self.fail(CoreError::InternalInvariant));
        }
        if self.steps == CHECKPOINT_EVENTS - TRAILER_STEPS
            || bytes > CHECKPOINT_BYTES - TRAILER_BYTES - self.bytes
        {
            return Ok(false);
        }
        self.bytes += bytes;
        self.steps += 1;
        Ok(true)
    }
    pub(in crate::memory) fn binding<T>(&mut self) -> Result<bool, CoreError> {
        if size_of::<T>() > MAX_VALUE_BYTES {
            return Err(self.fail(CoreError::InternalInvariant));
        }
        self.action((2 * size_of::<T>()) as u64)
    }
    pub(in crate::memory) fn value<T: BuildValue>(&mut self) -> Result<(), CoreError> {
        if size_of::<T>() == 0 || size_of::<T>() > MAX_VALUE_BYTES {
            return Err(self.fail(CoreError::InternalInvariant));
        }
        self.parts.check()
    }
    pub(in crate::memory) fn parts(&mut self) -> &mut ConstructionCharges<'call, 'id, 'op> {
        &mut self.parts
    }
}
impl Drop for BuildWindow<'_, '_, '_, '_> {
    fn drop(&mut self) {
        if !self.closed {
            self.parts.fail(CoreError::InternalInvariant);
        }
    }
}
