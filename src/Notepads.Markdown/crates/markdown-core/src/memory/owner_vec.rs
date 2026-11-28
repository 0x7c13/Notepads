// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE file in the project root for license information.
// ---------------------------------------------------------------------------------------------

//! Metered directory of non-Copy owners. Growth moves, never clones, owners.

use super::FrozenRows;
use crate::error::{CoreError, LimitKind};
use crate::policy::{CHECKPOINT_BYTES, ConstructionId, Lease, OwnedWork};
use std::marker::PhantomData;

pub(crate) struct BuildOwnerVec<'id, T> {
    values: Vec<T>,
    lease: Option<Lease>,
    max_len: usize,
    kind: LimitKind,
    id: ConstructionId<'id>,
}

struct OwnerReadCall<'call, 'id, 'op> {
    work: &'call mut OwnedWork<'id, 'op>,
    closed: bool,
}
impl OwnerReadCall<'_, '_, '_> {
    fn finish<R>(mut self, result: Result<R, CoreError>) -> Result<R, CoreError> {
        let result = result.map_err(|error| self.work.fail(error));
        let result = self.work.check().and(result);
        self.closed = true;
        result
    }
}
impl Drop for OwnerReadCall<'_, '_, '_> {
    fn drop(&mut self) {
        if !self.closed {
            self.work.fail(CoreError::InternalInvariant);
        }
    }
}

impl<'id, T> BuildOwnerVec<'id, T> {
    pub(crate) fn new(
        max_len: u64,
        kind: LimitKind,
        work: &mut OwnedWork<'id, '_>,
    ) -> Result<Self, CoreError> {
        work.check()?;
        if size_of::<T>() == 0 || size_of::<T>() as u64 > CHECKPOINT_BYTES {
            return Err(work.fail(CoreError::InternalInvariant));
        }
        let max_len =
            usize::try_from(max_len).map_err(|_| work.fail(CoreError::ArithmeticOverflow))?;
        max_len
            .checked_mul(size_of::<T>())
            .ok_or_else(|| work.fail(CoreError::ArithmeticOverflow))?;
        Ok(Self {
            values: Vec::new(),
            lease: None,
            max_len,
            kind,
            id: PhantomData,
        })
    }

    pub(crate) fn len(&self) -> usize {
        self.values.len()
    }

    /// Only a short read of one actual owner; no directory, owner or buffer
    /// can escape through R. The callback receives the same work authority.
    pub(crate) fn with<R>(
        &self,
        index: usize,
        work: &mut OwnedWork<'id, '_>,
        body: impl for<'a> FnOnce(&'a T, &mut OwnedWork<'id, '_>) -> Result<R, CoreError>,
    ) -> Result<R, CoreError> {
        work.events(1)?;
        work.scan(size_of::<T>() as u64)?;
        let value = self
            .values
            .get(index)
            .ok_or_else(|| work.fail(CoreError::InvalidRange))?;
        let guard = OwnerReadCall {
            work,
            closed: false,
        };
        let result = body(value, guard.work);
        guard.finish(result)
    }

    pub(crate) fn push(
        &mut self,
        value: T,
        work: &mut OwnedWork<'id, '_>,
    ) -> Result<(), CoreError> {
        work.events(1)?;
        let required = self
            .len()
            .checked_add(1)
            .ok_or_else(|| work.fail(CoreError::ArithmeticOverflow))?;
        work.check_limit(self.kind, required as u64, self.max_len as u64)?;
        if required > self.values.capacity() {
            self.grow(work)?;
        }
        work.copy(size_of::<T>() as u64)?;
        self.values.push(value);
        Ok(())
    }

    fn grow(&mut self, work: &mut OwnedWork<'id, '_>) -> Result<(), CoreError> {
        let capacity = self
            .values
            .capacity()
            .checked_mul(2)
            .ok_or_else(|| work.fail(CoreError::ArithmeticOverflow))?
            .max(1)
            .min(self.max_len);
        let bytes = capacity
            .checked_mul(size_of::<T>())
            .ok_or_else(|| work.fail(CoreError::ArithmeticOverflow))?;
        let mut lease = work.reserve_auxiliary(bytes as u64)?;
        let mut values = Vec::new();
        work.checkpoint_now()?;
        values
            .try_reserve_exact(capacity)
            .map_err(|_| work.fail(CoreError::OutOfMemory))?;
        work.checkpoint_now()?;
        let actual = values
            .capacity()
            .checked_mul(size_of::<T>())
            .ok_or_else(|| work.fail(CoreError::ArithmeticOverflow))?;
        lease
            .admit_actual(actual as u64)
            .map_err(|error| work.fail(error))?;
        let mut replacement = Self {
            values,
            lease: Some(lease),
            max_len: self.max_len,
            kind: self.kind,
            id: PhantomData,
        };
        // A fault consumes the remaining drain and drops moved owners through
        // replacement. This private construction cannot resume after first-fault.
        for value in self.values.drain(..) {
            work.events(1)?;
            work.copy(size_of::<T>() as u64)?;
            replacement.values.push(value);
        }
        work.checkpoint_now()?;
        drop(std::mem::replace(self, replacement));
        work.checkpoint_now()
    }

    pub(crate) fn freeze(self, work: &mut OwnedWork<'id, '_>) -> Result<FrozenRows<T>, CoreError> {
        work.check()?;
        Ok(FrozenRows {
            values: self.values,
            _lease: self.lease,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::memory::BuildString;
    use crate::policy::{CancellationToken, SourceBudget, SourceLimits, construction_scope};

    #[test]
    fn noncopy_growth_and_freeze_transfer_each_buffer_once() {
        let budget = SourceBudget::new(SourceLimits::default()).unwrap();
        let mut op = budget.begin(CancellationToken::new());
        construction_scope(&mut op, |mut work| {
            let mut owners = BuildOwnerVec::new(3, LimitKind::IndexRecords, &mut work).unwrap();
            let mut pointers = Vec::new();
            for value in ["alpha", "你好", "omega"] {
                let mut text = BuildString::auxiliary(value.len(), &mut work).unwrap();
                text.append(value, &mut work).unwrap();
                pointers.push(text.as_str().as_ptr());
                owners
                    .push(text.freeze(&mut work).unwrap(), &mut work)
                    .unwrap();
            }
            assert_eq!(owners.len(), 3);
            assert_eq!(
                owners.with(1, &mut work, |owner, _| Ok(owner.as_str().len())),
                Ok(6)
            );
            let pointer = owners.values.as_ptr();
            let frozen = owners.freeze(&mut work).unwrap();
            assert_eq!(frozen.as_slice().as_ptr(), pointer);
            for (index, pointer) in pointers.into_iter().enumerate() {
                assert_eq!(frozen.as_slice()[index].as_str().as_ptr(), pointer);
            }
            drop(frozen);
            assert_eq!(budget.usage().auxiliary_capacity, 0);
            work.finish().unwrap();
        });
    }

    #[test]
    fn callback_raw_error_latches_before_return_and_cannot_resume() {
        let budget = SourceBudget::new(SourceLimits::default()).unwrap();
        let mut op = budget.begin(CancellationToken::new());
        construction_scope(&mut op, |mut work| {
            let mut owners = BuildOwnerVec::new(2, LimitKind::IndexRecords, &mut work).unwrap();
            owners
                .push(BuildString::auxiliary(8, &mut work).unwrap(), &mut work)
                .unwrap();
            assert_eq!(
                owners.with(0, &mut work, |_, _| Err::<(), _>(
                    CoreError::InvalidBoundary
                )),
                Err(CoreError::InvalidBoundary)
            );
            assert_eq!(
                owners.with(0, &mut work, |_, _| Ok(())),
                Err(CoreError::InvalidBoundary)
            );
            drop(owners);
            assert_eq!(budget.usage().auxiliary_capacity, 0);
            assert_eq!(work.finish(), Err(CoreError::InvalidBoundary));
        });
    }

    #[test]
    fn callback_cannot_hide_an_inner_fault_with_ok() {
        let budget = SourceBudget::new(SourceLimits::default()).unwrap();
        let mut op = budget.begin(CancellationToken::new());
        construction_scope(&mut op, |mut work| {
            let mut owners = BuildOwnerVec::new(1, LimitKind::IndexRecords, &mut work).unwrap();
            owners
                .push(BuildString::auxiliary(8, &mut work).unwrap(), &mut work)
                .unwrap();
            assert_eq!(
                owners.with(0, &mut work, |_, work| {
                    let _ = work.fail(CoreError::InvalidProvenance);
                    Ok(())
                }),
                Err(CoreError::InvalidProvenance)
            );
            drop(owners);
            assert_eq!(budget.usage().auxiliary_capacity, 0);
            assert_eq!(work.finish(), Err(CoreError::InvalidProvenance));
        });
    }

    #[test]
    fn callback_caught_unwind_cannot_resume_with_the_same_work() {
        let budget = SourceBudget::new(SourceLimits::default()).unwrap();
        let mut op = budget.begin(CancellationToken::new());
        construction_scope(&mut op, |mut work| {
            let mut owners = BuildOwnerVec::new(1, LimitKind::IndexRecords, &mut work).unwrap();
            owners
                .push(BuildString::auxiliary(8, &mut work).unwrap(), &mut work)
                .unwrap();
            let unwind = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                let _: Result<(), CoreError> =
                    owners.with(0, &mut work, |_, _| panic!("owner callback panic"));
            }));
            assert!(unwind.is_err());
            assert_eq!(work.first_fault(), Some(CoreError::InternalInvariant));
            assert_eq!(
                owners.with(0, &mut work, |_, _| Ok(())),
                Err(CoreError::InternalInvariant)
            );
            drop(owners);
            assert_eq!(budget.usage().auxiliary_capacity, 0);
            assert_eq!(work.finish(), Err(CoreError::InternalInvariant));
        });
    }

    #[test]
    fn every_partial_noncopy_move_releases_all_buffers_and_both_directories() {
        fn prepare<'id>(work: &mut OwnedWork<'id, '_>) -> BuildOwnerVec<'id, BuildString<'id>> {
            let mut owners = BuildOwnerVec::new(4, LimitKind::IndexRecords, work).unwrap();
            for _ in 0..2 {
                owners
                    .push(BuildString::auxiliary(16, work).unwrap(), work)
                    .unwrap();
            }
            owners
        }
        let budget = SourceBudget::new(SourceLimits::default()).unwrap();
        let mut op = budget.begin(CancellationToken::new());
        let (before, after) = construction_scope(&mut op, |mut work| {
            let mut owners = prepare(&mut work);
            let before = work.stats().unwrap().charged_units();
            owners.grow(&mut work).unwrap();
            let after = work.stats().unwrap().charged_units();
            drop(owners);
            work.finish().unwrap();
            (before, after)
        });
        for limit in before..after {
            let budget = SourceBudget::new(SourceLimits {
                max_work: limit,
                ..SourceLimits::default()
            })
            .unwrap();
            let mut op = budget.begin(CancellationToken::new());
            construction_scope(&mut op, |mut work| {
                let mut owners = prepare(&mut work);
                let error = owners.grow(&mut work).unwrap_err();
                assert!(matches!(
                    error,
                    CoreError::ResourceLimit {
                        kind: LimitKind::Work,
                        ..
                    }
                ));
                drop(owners);
                assert_eq!(budget.usage().auxiliary_capacity, 0);
                assert_eq!(work.finish(), Err(error));
            });
        }
    }
}
