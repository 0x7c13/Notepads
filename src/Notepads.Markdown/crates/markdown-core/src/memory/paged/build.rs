// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE file in the project root for license information.
// ---------------------------------------------------------------------------------------------

//! Actual pages bound to one construction; no table budget, fault or recovery API.

use crate::error::{CoreError, LimitKind};
use crate::memory::{BuildVec, FrozenPagedRows};
use crate::policy::{CHECKPOINT_BYTES, ConstructionId, Lease, MAX_METADATA_VIEW_BYTES, OwnedWork};
use std::marker::PhantomData;

struct Owners<'id, T> {
    values: Vec<Option<BuildVec<'id, T>>>,
    _lease: Option<Lease>,
}
pub(crate) struct BuildRows<'id, T: Copy> {
    owners: Owners<'id, T>,
    len: usize,
    max_len: usize,
    width: usize,
    kind: LimitKind,
    id: ConstructionId<'id>,
}
impl<'id, T: Copy> BuildRows<'id, T> {
    pub(crate) fn new(
        max_len: u64,
        kind: LimitKind,
        work: &mut OwnedWork<'id, '_>,
    ) -> Result<Self, CoreError> {
        work.events(1)?;
        let size = size_of::<T>();
        if size == 0 || size as u64 > CHECKPOINT_BYTES {
            return Err(work.fail(CoreError::InternalInvariant));
        }
        let max_len =
            usize::try_from(max_len).map_err(|_| work.fail(CoreError::ArithmeticOverflow))?;
        max_len
            .checked_mul(size)
            .ok_or_else(|| work.fail(CoreError::ArithmeticOverflow))?;
        Ok(Self {
            owners: Owners {
                values: Vec::new(),
                _lease: None,
            },
            len: 0,
            max_len,
            width: (16384 / size).clamp(1, 1024),
            kind,
            id: PhantomData,
        })
    }
    pub(crate) fn len(&self) -> usize {
        self.len
    }
    /// Test observation only: one physical row, without granting production access.
    #[cfg(test)]
    pub(crate) fn raw_value_for_test(&self, index: usize) -> Option<T> {
        if index >= self.len {
            return None;
        }
        self.owners
            .values
            .get(index / self.width)
            .and_then(Option::as_ref)
            .and_then(|page| page.as_slice().get(index % self.width))
            .copied()
    }
    #[cfg(test)]
    pub(crate) fn page_count(&self) -> usize {
        self.owners.values.len()
    }
    #[cfg(test)]
    pub(crate) fn address_for_test(&self, index: usize) -> Option<usize> {
        if index >= self.len {
            return None;
        }
        self.owners
            .values
            .get(index / self.width)
            .and_then(Option::as_ref)
            .and_then(|page| page.as_slice().get(index % self.width))
            .map(|value| std::ptr::from_ref(value) as usize)
    }
    /// Transfer actual row allocations and leases; only the owner directory is
    /// rebuilt. A retired page makes this construction permanently unfreezable.
    pub(crate) fn freeze(
        mut self,
        work: &mut OwnedWork<'id, '_>,
    ) -> Result<FrozenPagedRows<T>, CoreError> {
        work.check()?;
        if self.owners.values.len() != self.len.div_ceil(self.width) {
            return Err(work.fail(CoreError::InternalInvariant));
        }
        let mut pages = BuildVec::new(self.owners.values.len(), work)?;
        for (index, owner) in self.owners.values.iter_mut().enumerate() {
            work.events(1)?;
            work.scan(size_of::<Option<BuildVec<'id, T>>>() as u64)?;
            let page = owner
                .take()
                .ok_or_else(|| work.fail(CoreError::InternalInvariant))?;
            if page.len() != self.width.min(self.len - index * self.width) {
                return Err(work.fail(CoreError::InternalInvariant));
            }
            work.copy(size_of::<crate::memory::FrozenRows<T>>() as u64)?;
            pages.push(page.freeze(work)?, work)?;
        }
        let len = self.len;
        let width = self.width;
        // Old descriptors release before their lease. The original row buffers
        // now belong to `pages`, including on a later checkpoint failure.
        drop(self);
        work.checkpoint_now()?;
        Ok(FrozenPagedRows {
            pages: pages.freeze(work)?,
            len,
            width,
        })
    }
    pub(crate) fn get(&self, index: usize, work: &mut OwnedWork<'id, '_>) -> Result<T, CoreError> {
        work.check()?;
        work.events(1)?;
        if index >= self.len {
            return Err(work.fail(CoreError::InternalInvariant));
        }
        let page = self
            .owners
            .values
            .get(index / self.width)
            .and_then(Option::as_ref)
            .ok_or_else(|| work.fail(CoreError::InternalInvariant))?;
        let bytes = size_of::<T>() as u64;
        let read = || {
            page.as_slice()
                .get(index % self.width)
                .copied()
                .ok_or(CoreError::InternalInvariant)
        };
        if bytes <= MAX_METADATA_VIEW_BYTES {
            work.read_metadata(bytes, read)
        } else {
            work.scan(bytes)?;
            work.copy(bytes)?;
            read().map_err(|error| work.fail(error))
        }
    }
    pub(crate) fn push(
        &mut self,
        value: T,
        work: &mut OwnedWork<'id, '_>,
    ) -> Result<usize, CoreError> {
        work.check()?;
        work.events(1)?;
        let required = self
            .len
            .checked_add(1)
            .ok_or_else(|| work.fail(CoreError::ArithmeticOverflow))?;
        work.check_limit(self.kind, required as u64, self.max_len as u64)?;
        if self.len.is_multiple_of(self.width) {
            self.grow_owners(work)?;
            let page = BuildVec::new(self.width.min(self.max_len - self.len), work)?;
            work.copy(size_of::<Option<BuildVec<'id, T>>>() as u64)?;
            self.owners.values.push(Some(page));
        }
        let page = self
            .owners
            .values
            .get_mut(self.len / self.width)
            .and_then(Option::as_mut)
            .ok_or_else(|| work.fail(CoreError::InternalInvariant))?;
        work.copy(size_of::<T>() as u64)?;
        page.push(value, work)?;
        let index = self.len;
        self.len = required;
        Ok(index)
    }
    pub(crate) fn replace(
        &mut self,
        index: usize,
        value: T,
        work: &mut OwnedWork<'id, '_>,
    ) -> Result<(), CoreError> {
        work.check()?;
        work.events(1)?;
        if index >= self.len {
            return Err(work.fail(CoreError::InternalInvariant));
        }
        let slot = self
            .owners
            .values
            .get_mut(index / self.width)
            .and_then(Option::as_mut)
            .and_then(|page| page.values.get_mut(index % self.width))
            .ok_or_else(|| work.fail(CoreError::InternalInvariant))?;
        work.copy(size_of::<T>() as u64)?;
        *slot = value;
        Ok(())
    }
    #[cfg(test)]
    pub(crate) fn retire_page(
        &mut self,
        index: usize,
        work: &mut OwnedWork<'id, '_>,
    ) -> Result<(), CoreError> {
        work.check()?;
        work.events(1)?;
        let slot = self
            .owners
            .values
            .get_mut(index)
            .ok_or_else(|| work.fail(CoreError::InternalInvariant))?;
        if slot.is_none() {
            return Err(work.fail(CoreError::InternalInvariant));
        }
        work.copy(size_of::<Option<BuildVec<'id, T>>>() as u64)?;
        drop(slot.take());
        Ok(())
    }
    fn grow_owners(&mut self, work: &mut OwnedWork<'id, '_>) -> Result<(), CoreError> {
        if self.owners.values.len() < self.owners.values.capacity() {
            return Ok(());
        }
        let capacity = self
            .owners
            .values
            .capacity()
            .checked_mul(2)
            .ok_or_else(|| work.fail(CoreError::ArithmeticOverflow))?
            .max(1)
            .min(self.max_len.div_ceil(self.width));
        let bytes = capacity
            .checked_mul(size_of::<Option<BuildVec<'id, T>>>())
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
            .checked_mul(size_of::<Option<BuildVec<'id, T>>>())
            .ok_or_else(|| work.fail(CoreError::ArithmeticOverflow))?;
        lease
            .admit_actual(actual as u64)
            .map_err(|error| work.fail(error))?;
        let mut replacement = Owners {
            values,
            _lease: Some(lease),
        };
        // A failing move leaves two real owners of disjoint pages. Both destroy
        // their pages/descriptor allocations before their respective leases.
        for owner in &mut self.owners.values {
            work.events(1)?;
            work.copy(size_of::<Option<BuildVec<'id, T>>>() as u64)?;
            replacement.values.push(owner.take());
        }
        work.checkpoint_now()?;
        drop(std::mem::replace(&mut self.owners, replacement));
        work.checkpoint_now()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::policy::{CancellationToken, SourceBudget, SourceLimits, construction_scope};

    fn prepare<'id>(work: &mut OwnedWork<'id, '_>) -> BuildRows<'id, [u8; 512]> {
        let mut rows = BuildRows::new(256, LimitKind::IndexRecords, work).unwrap();
        for _ in 0..128 {
            rows.push([1_u8; 512], work).unwrap();
        }
        assert_eq!(rows.owners.values.len(), 4);
        assert_eq!(rows.owners.values.capacity(), 4);
        rows
    }

    #[test]
    fn freezing_keeps_real_page_buffers_and_accounts_both_directories() {
        let budget = SourceBudget::new(SourceLimits::default()).unwrap();
        let mut op = budget.begin(CancellationToken::new());
        construction_scope(&mut op, |mut work| {
            let mut rows = prepare(&mut work);
            for index in 0..rows.len() {
                rows.replace(index, [index as u8; 512], &mut work).unwrap();
            }
            let pointers: Vec<_> = rows
                .owners
                .values
                .iter()
                .map(|page| page.as_ref().unwrap().as_slice().as_ptr())
                .collect();
            let retained = budget.usage().auxiliary_capacity;
            let old_directory = (rows.owners.values.capacity()
                * size_of::<Option<BuildVec<'_, [u8; 512]>>>())
                as u64;
            let new_directory = (4 * size_of::<crate::memory::FrozenRows<[u8; 512]>>()) as u64;
            let frozen = rows.freeze(&mut work).unwrap();
            assert_eq!(frozen.len(), 128);
            for (index, pointer) in pointers.into_iter().enumerate() {
                assert_eq!(frozen.get(index * 32).unwrap() as *const _, pointer);
            }
            assert_eq!(
                budget.usage().auxiliary_capacity,
                retained - old_directory + new_directory
            );
            assert!(budget.usage().auxiliary_capacity_peak >= retained + new_directory);
            assert_eq!(frozen.iter().count(), 128);
            assert_eq!(frozen.iter().rev().count(), 128);
            assert!(
                frozen
                    .iter()
                    .enumerate()
                    .all(|(index, value)| *value == [index as u8; 512])
            );
            let range = frozen.range(31, 34).unwrap();
            assert_eq!(range.len(), 34);
            assert_eq!(range.get(0), frozen.get(31));
            assert_eq!(range.get(33), frozen.get(64));
            assert!(range.get(34).is_none());
            assert_eq!(range.iter().count(), 34);
            assert!(frozen.range(128, 0).is_some());
            assert!(frozen.range(128, 1).is_none());
            assert!(frozen.range(usize::MAX, 2).is_none());
            assert!(frozen.get(128).is_none());
            drop(frozen);
            assert_eq!(budget.usage().auxiliary_capacity, 0);
            work.finish().unwrap();
        });
    }

    #[test]
    fn freezing_retired_pages_rejects_and_releases_all_actual_storage() {
        let budget = SourceBudget::new(SourceLimits::default()).unwrap();
        let mut op = budget.begin(CancellationToken::new());
        construction_scope(&mut op, |mut work| {
            let mut rows = prepare(&mut work);
            rows.retire_page(2, &mut work).unwrap();
            assert!(matches!(
                rows.freeze(&mut work),
                Err(CoreError::InternalInvariant)
            ));
            assert_eq!(budget.usage().auxiliary_capacity, 0);
            assert_eq!(work.finish(), Err(CoreError::InternalInvariant));
        });
    }

    #[test]
    fn freeze_deadline_at_each_real_clock_read_including_after_move_releases_every_page() {
        let budget = SourceBudget::task_fixture();
        let mut op = budget.begin(CancellationToken::new());
        op.set_clock_samples_for_test(&[100_000_000]);
        let reads = construction_scope(&mut op, |mut work| {
            let rows = prepare(&mut work);
            work.set_clock_samples_for_test(&[100_000_000]);
            drop(rows.freeze(&mut work).unwrap());
            let reads = work.clock_read_count_for_test();
            work.finish().unwrap();
            reads
        });
        // The last forced read is after the old directory was dropped and all
        // real page ownership was transferred, before any success can escape.
        assert!(reads >= 3);
        for at in 1..=reads {
            let budget = SourceBudget::task_fixture();
            let mut op = budget.begin(CancellationToken::new());
            op.set_clock_samples_for_test(&[100_000_000]);
            construction_scope(&mut op, |mut work| {
                let rows = prepare(&mut work);
                let mut samples = vec![100_000_000; at];
                samples[at - 1] = 500_000_000;
                work.set_clock_samples_for_test(&samples);
                let error = rows.freeze(&mut work).err().unwrap();
                assert_eq!(
                    error,
                    CoreError::DeadlineExceeded {
                        elapsed_ns: 500_000_000,
                        limit_ns: 500_000_000
                    }
                );
                assert_eq!(budget.usage().auxiliary_capacity, 0);
                assert_eq!(work.finish(), Err(error));
            });
        }
    }

    #[test]
    fn faults_through_frozen_directory_transfer_release_original_and_moved_pages() {
        let budget = SourceBudget::new(SourceLimits::default()).unwrap();
        let mut op = budget.begin(CancellationToken::new());
        let (before, after) = construction_scope(&mut op, |mut work| {
            let rows = prepare(&mut work);
            let before = work.stats().unwrap().charged_units();
            drop(rows.freeze(&mut work).unwrap());
            let after = work.stats().unwrap().charged_units();
            work.finish().unwrap();
            (before, after)
        });
        // Every charged position in the small owner transfer is a real failure
        // point, including partial moves. No row buffer is duplicated.
        for limit in before..after {
            let budget = SourceBudget::new(SourceLimits {
                max_work: limit,
                ..SourceLimits::default()
            })
            .unwrap();
            let mut op = budget.begin(CancellationToken::new());
            construction_scope(&mut op, |mut work| {
                let rows = prepare(&mut work);
                let error = rows.freeze(&mut work).err().unwrap();
                assert!(matches!(
                    error,
                    CoreError::ResourceLimit {
                        kind: LimitKind::Work,
                        ..
                    }
                ));
                assert_eq!(budget.usage().auxiliary_capacity, 0);
                assert_eq!(work.check(), Err(error));
                assert_eq!(work.finish(), Err(error));
            });
        }
    }

    #[test]
    fn partial_descriptor_move_destroys_disjoint_pages_and_releases_both_leases() {
        // Determine preparation's exact deterministic charge with the existing
        // WorkStats. This is a fault-injection unit test, not a timing observer.
        let baseline = SourceBudget::new(SourceLimits::default()).unwrap();
        let mut op = baseline.begin(CancellationToken::new());
        construction_scope(&mut op, |mut work| {
            drop(prepare(&mut work));
            work.finish().unwrap();
        });
        let before_move = op.stats().unwrap().charged_units();
        let descriptor = size_of::<Option<BuildVec<'_, [u8; 512]>>>() as u64;
        let budget = SourceBudget::new(SourceLimits {
            max_work: before_move + 1 + descriptor,
            ..SourceLimits::default()
        })
        .unwrap();
        let mut op = budget.begin(CancellationToken::new());
        construction_scope(&mut op, |mut work| {
            let mut rows = prepare(&mut work);
            let retained = budget.usage().auxiliary_capacity;
            let error = rows.grow_owners(&mut work).unwrap_err();
            assert!(matches!(
                error,
                CoreError::ResourceLimit {
                    kind: LimitKind::Work,
                    ..
                }
            ));
            // One actual page moved before the next event failed. The failed
            // replacement has dropped that page, while the remaining three
            // still belong to the original table. No reassembly is permitted.
            assert!(rows.owners.values[0].is_none());
            assert!(rows.owners.values[1..].iter().all(Option::is_some));
            assert_eq!(budget.usage().auxiliary_capacity, retained - 16384);
            assert!(budget.usage().auxiliary_capacity_peak >= retained + 8 * descriptor);
            assert_eq!(rows.get(32, &mut work), Err(error));
            drop(rows);
            assert_eq!(budget.usage().auxiliary_capacity, 0);
            assert_eq!(work.finish(), Err(error));
        });
    }
}
