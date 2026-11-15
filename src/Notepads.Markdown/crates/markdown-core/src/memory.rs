// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE file in the project root for license information.
// ---------------------------------------------------------------------------------------------

//! Safe admitted containers. No unchecked grow, clone, or owned-buffer escape.

use crate::error::CoreError;
#[cfg(test)]
use crate::error::LimitKind;
#[cfg(test)]
use crate::policy::CHECKPOINT_BYTES;
use crate::policy::{Lease, Operation};

mod build;
#[cfg(test)]
pub(crate) use build::BuildGrowingText;
pub(crate) use build::{BuildGrowingVec, BuildString, BuildVec, FrozenRows, FrozenText};
mod build_sort;
mod frozen_paged;
pub(crate) use frozen_paged::{BuildFrozenPagedRead, FrozenPagedRows};
mod owner_vec;
pub(crate) use build_sort::{BuildSortRows, sort_build_rows_by};
pub(crate) use owner_vec::BuildOwnerVec;
#[cfg(test)]
mod build_tests;
mod build_window;
pub(crate) use build_window::{
    BuildKernel, BuildRecord, BuildStep, BuildValue, BuildWindow, run_window,
};
mod build_views;
#[cfg(test)]
pub(crate) use build_views::BuildExactRead;
pub(crate) use build_views::BuildTextChunk;

mod paged;
#[cfg(test)]
pub(crate) use paged::PagedRows;
pub(crate) use paged::build::BuildRows;
mod exact;
#[cfg(test)]
pub(crate) use exact::ExactRows;
pub(crate) use exact::build::BuildExactRows;
#[cfg(test)]
pub(crate) use exact::build::BuildExactText;
#[cfg(test)]
mod task;
#[cfg(test)]
pub(crate) use task::{TaskAbandon, TaskKernel, TaskStep, TaskValue, TaskWindow, run_task};
#[cfg(test)]
mod task_exact_tests;
#[cfg(test)]
mod task_rows;
#[cfg(test)]
mod task_source;
#[cfg(test)]
mod task_tests;

pub(crate) struct ChargedString {
    // Rust drops fields in declaration order: allocation before reservation.
    text: String,
    _lease: Lease,
}

impl ChargedString {
    pub(crate) fn source(capacity: usize, op: &mut Operation) -> Result<Self, CoreError> {
        Self::new(capacity, op, true)
    }

    #[cfg(test)]
    pub(crate) fn auxiliary(capacity: usize, op: &mut Operation) -> Result<Self, CoreError> {
        Self::new(capacity, op, false)
    }

    fn new(capacity: usize, op: &mut Operation, source: bool) -> Result<Self, CoreError> {
        op.events(1)?;
        let mut lease = if source {
            op.budget().reserve_source(capacity as u64)
        } else {
            op.budget().reserve_auxiliary(capacity as u64)
        }
        .map_err(|error| op.fail(error))?;
        let mut text = String::new();
        if capacity != 0 {
            op.checkpoint_now()?;
        }
        text.try_reserve_exact(capacity)
            .map_err(|_| op.fail(CoreError::OutOfMemory))?;
        if capacity != 0 {
            op.checkpoint_now()?;
        }
        // On error the later-declared text frees before the earlier lease.
        lease
            .admit_actual(text.capacity() as u64)
            .map_err(|error| op.fail(error))?;
        op.checkpoint()?;
        Ok(Self {
            text,
            _lease: lease,
        })
    }

    pub(crate) fn as_str(&self) -> &str {
        &self.text
    }

    pub(crate) fn append(&mut self, text: &str, op: &mut Operation) -> Result<(), CoreError> {
        op.copy(text.len() as u64)?;
        if text.len() > self.text.capacity() - self.text.len() {
            return Err(op.fail(CoreError::InternalInvariant));
        }
        self.text.push_str(text);
        Ok(())
    }
}

#[cfg(test)]
pub(crate) struct ChargedVec<T> {
    values: Vec<T>,
    _lease: Lease,
}

#[cfg(test)]
impl<T> ChargedVec<T> {
    pub(crate) fn new(capacity: usize, op: &mut Operation) -> Result<Self, CoreError> {
        op.events(1)?;
        let size = std::mem::size_of::<T>();
        if size == 0 {
            return Err(op.fail(CoreError::InternalInvariant));
        }
        let bytes = capacity
            .checked_mul(size)
            .ok_or_else(|| op.fail(CoreError::ArithmeticOverflow))?;
        let mut lease = op
            .budget()
            .reserve_auxiliary(bytes as u64)
            .map_err(|error| op.fail(error))?;
        let mut values = Vec::new();
        if capacity != 0 {
            op.checkpoint_now()?;
        }
        values
            .try_reserve_exact(capacity)
            .map_err(|_| op.fail(CoreError::OutOfMemory))?;
        if capacity != 0 {
            op.checkpoint_now()?;
        }
        let actual = values
            .capacity()
            .checked_mul(size)
            .ok_or_else(|| op.fail(CoreError::ArithmeticOverflow))?;
        lease
            .admit_actual(actual as u64)
            .map_err(|error| op.fail(error))?;
        op.checkpoint()?;
        Ok(Self {
            values,
            _lease: lease,
        })
    }

    pub(crate) fn push(&mut self, value: T, op: &mut Operation) -> Result<(), CoreError> {
        op.events(1)?;
        if self.values.len() == self.values.capacity() {
            return Err(op.fail(CoreError::InternalInvariant));
        }
        self.values.push(value);
        Ok(())
    }

    pub(crate) fn as_slice(&self) -> &[T] {
        &self.values
    }
}

/// Unknown-size flat builders: the complete new allocation is admitted while
/// the old allocation remains live. Growth never invokes an opaque realloc.
#[cfg(test)]
pub(crate) struct GrowingVec<T: Copy> {
    values: Vec<T>,
    _lease: Option<Lease>,
    max_len: usize,
    kind: LimitKind,
}

/// A single always-valid UTF8 pool. Borrowing text never revalidates a full run.
#[cfg(test)]
pub(crate) struct GrowingText {
    text: ChargedString,
    max_len: usize,
}

#[cfg(test)]
impl GrowingText {
    pub(crate) fn new(max_len: u64, op: &mut Operation) -> Result<Self, CoreError> {
        Ok(Self {
            text: ChargedString::auxiliary(0, op)?,
            max_len: usize::try_from(max_len)
                .map_err(|_| op.fail(CoreError::ArithmeticOverflow))?,
        })
    }

    pub(crate) fn len(&self) -> usize {
        self.text.as_str().len()
    }
    pub(crate) fn as_str(&self) -> &str {
        self.text.as_str()
    }

    pub(crate) fn append(&mut self, value: &str, op: &mut Operation) -> Result<(), CoreError> {
        op.checkpoint()?;
        let required = self
            .len()
            .checked_add(value.len())
            .ok_or_else(|| op.fail(CoreError::ArithmeticOverflow))?;
        op.check_limit(
            LimitKind::DecodedBytes,
            required as u64,
            self.max_len as u64,
        )?;
        if required > self.text.text.capacity() {
            let capacity = self
                .text
                .text
                .capacity()
                .checked_mul(2)
                .ok_or_else(|| op.fail(CoreError::ArithmeticOverflow))?
                .max(1)
                .max(required)
                .min(self.max_len);
            let mut replacement = ChargedString::auxiliary(capacity, op)?;
            Self::append_chunks(&mut replacement, self.text.as_str(), op)?;
            op.checkpoint_now()?;
            drop(std::mem::replace(&mut self.text, replacement));
            op.checkpoint_now()?;
        }
        Self::append_chunks(&mut self.text, value, op)
    }

    fn append_chunks(
        target: &mut ChargedString,
        text: &str,
        op: &mut Operation,
    ) -> Result<(), CoreError> {
        let mut at = 0;
        while at < text.len() {
            op.events(1)?;
            let mut end = text.len().min(at.saturating_add(CHECKPOINT_BYTES as usize));
            while !text.is_char_boundary(end) {
                op.scan(1)?;
                end -= 1;
            }
            target.append(&text[at..end], op)?;
            at = end;
        }
        Ok(())
    }
}

#[cfg(test)]
impl<T: Copy> GrowingVec<T> {
    pub(crate) fn new(
        max_len: u64,
        kind: LimitKind,
        op: &mut Operation,
    ) -> Result<Self, CoreError> {
        op.checkpoint()?;
        let size = size_of::<T>();
        if size == 0 || size as u64 > CHECKPOINT_BYTES {
            return Err(op.fail(CoreError::InternalInvariant));
        }
        let max_len =
            usize::try_from(max_len).map_err(|_| op.fail(CoreError::ArithmeticOverflow))?;
        max_len
            .checked_mul(size)
            .ok_or_else(|| op.fail(CoreError::ArithmeticOverflow))?;
        Ok(Self {
            values: Vec::new(),
            _lease: None,
            max_len,
            kind,
        })
    }

    pub(crate) fn as_slice(&self) -> &[T] {
        &self.values
    }

    pub(crate) fn len(&self) -> usize {
        self.values.len()
    }

    pub(crate) fn push(&mut self, value: T, op: &mut Operation) -> Result<(), CoreError> {
        self.extend_from_slice(std::slice::from_ref(&value), op)
    }

    pub(crate) fn replace(
        &mut self,
        index: usize,
        value: T,
        op: &mut Operation,
    ) -> Result<(), CoreError> {
        op.events(1)?;
        op.copy(size_of::<T>() as u64)?;
        let slot = self
            .values
            .get_mut(index)
            .ok_or_else(|| op.fail(CoreError::InternalInvariant))?;
        *slot = value;
        Ok(())
    }

    pub(crate) fn extend_from_slice(
        &mut self,
        values: &[T],
        op: &mut Operation,
    ) -> Result<(), CoreError> {
        op.checkpoint()?;
        let required = self
            .values
            .len()
            .checked_add(values.len())
            .ok_or_else(|| op.fail(CoreError::ArithmeticOverflow))?;
        op.check_limit(self.kind, required as u64, self.max_len as u64)?;
        if required > self.values.capacity() {
            self.grow(required, op)?;
        }
        let size = size_of::<T>();
        let per_chunk = CHECKPOINT_BYTES as usize / size;
        for chunk in values.chunks(per_chunk) {
            op.events(1)?;
            op.copy(size_of_val(chunk) as u64)?;
            self.values.extend_from_slice(chunk);
        }
        Ok(())
    }

    fn grow(&mut self, required: usize, op: &mut Operation) -> Result<(), CoreError> {
        op.events(1)?;
        let capacity = self
            .values
            .capacity()
            .checked_mul(2)
            .ok_or_else(|| op.fail(CoreError::ArithmeticOverflow))?
            .max(1)
            .max(required)
            .min(self.max_len);
        let size = size_of::<T>();
        let bytes = capacity
            .checked_mul(size)
            .ok_or_else(|| op.fail(CoreError::ArithmeticOverflow))?;
        let mut lease = op
            .budget()
            .reserve_auxiliary(bytes as u64)
            .map_err(|error| op.fail(error))?;
        // Later declaration ensures values frees before lease on every error.
        let mut values = Vec::new();
        op.checkpoint_now()?;
        values
            .try_reserve_exact(capacity)
            .map_err(|_| op.fail(CoreError::OutOfMemory))?;
        op.checkpoint_now()?;
        let actual = values
            .capacity()
            .checked_mul(size)
            .ok_or_else(|| op.fail(CoreError::ArithmeticOverflow))?;
        lease
            .admit_actual(actual as u64)
            .map_err(|error| op.fail(error))?;
        for chunk in self.values.chunks(CHECKPOINT_BYTES as usize / size) {
            op.events(1)?;
            op.copy(size_of_val(chunk) as u64)?;
            values.extend_from_slice(chunk);
        }
        op.checkpoint_now()?;
        let replacement = Self {
            values,
            _lease: Some(lease),
            max_len: self.max_len,
            kind: self.kind,
        };
        // Field order releases the old buffer before its old reservation.
        drop(std::mem::replace(self, replacement));
        op.checkpoint_now()?;
        Ok(())
    }
}

pub(crate) struct OwnerLease {
    _lease: Lease,
}

#[cfg(test)]
pub(crate) trait SortRows<T: Copy> {
    fn as_slice(&self) -> &[T];
    fn len(&self) -> usize {
        self.as_slice().len()
    }
    fn replace(&mut self, index: usize, value: T, op: &mut Operation) -> Result<(), CoreError>;
}
#[cfg(test)]
impl<T: Copy> SortRows<T> for GrowingVec<T> {
    fn as_slice(&self) -> &[T] {
        self.as_slice()
    }
    fn replace(&mut self, index: usize, value: T, op: &mut Operation) -> Result<(), CoreError> {
        self.replace(index, value, op)
    }
}
#[cfg(test)]
impl<T: Copy> SortRows<T> for ExactRows<T> {
    fn as_slice(&self) -> &[T] {
        self.as_slice()
    }
    fn replace(&mut self, index: usize, value: T, op: &mut Operation) -> Result<(), CoreError> {
        self.replace(index, value, op)
    }
}

/// Bounded stable merge of admitted rows. Ordered runs incur no scratch or rewrites.
/// A fault may leave this private draft partly sorted; callers must discard it.
#[cfg(test)]
pub(crate) fn sort_by<T: Copy>(
    values: &mut impl SortRows<T>,
    start: usize,
    mut compare: impl FnMut(T, T, &mut Operation) -> Result<std::cmp::Ordering, CoreError>,
    op: &mut Operation,
) -> Result<(), CoreError> {
    op.checkpoint()?;
    let end_of_rows = values.len();
    let length = end_of_rows
        .checked_sub(start)
        .ok_or_else(|| op.fail(CoreError::InvalidRange))?;
    let mut ordered = true;
    for pair in values.as_slice()[start..].windows(2) {
        op.events(1)?;
        if compare(pair[0], pair[1], op)?.is_gt() {
            ordered = false;
            break;
        }
    }
    if ordered {
        return op.checkpoint();
    }
    let mut width = 1_usize;
    while width < length {
        let mut start = start;
        while start < end_of_rows {
            op.events(1)?;
            let middle = start
                .checked_add(width)
                .ok_or_else(|| op.fail(CoreError::ArithmeticOverflow))?
                .min(end_of_rows);
            let end = middle
                .checked_add(width)
                .ok_or_else(|| op.fail(CoreError::ArithmeticOverflow))?
                .min(end_of_rows);
            if middle < end
                && compare(values.as_slice()[middle - 1], values.as_slice()[middle], op)?.is_gt()
            {
                let mut scratch = ChargedVec::new(end - start, op)?;
                let mut left = start;
                let mut right = middle;
                while left < middle || right < end {
                    op.events(1)?;
                    let take_left = left < middle
                        && (right == end
                            || !compare(values.as_slice()[left], values.as_slice()[right], op)?
                                .is_gt());
                    let index = if take_left {
                        let at = left;
                        left += 1;
                        at
                    } else {
                        let at = right;
                        right += 1;
                        at
                    };
                    op.copy(size_of::<T>() as u64)?;
                    scratch.push(values.as_slice()[index], op)?;
                }
                for (offset, row) in scratch.as_slice().iter().copied().enumerate() {
                    op.events(1)?;
                    values.replace(start + offset, row, op)?;
                }
            }
            start = end;
        }
        width = width
            .checked_mul(2)
            .ok_or_else(|| op.fail(CoreError::ArithmeticOverflow))?;
    }
    op.checkpoint()
}

impl OwnerLease {
    pub(crate) fn new(op: &mut Operation) -> Result<Self, CoreError> {
        op.events(1)?;
        let lease = op
            .budget()
            .reserve_owner()
            .map_err(|error| op.fail(error))?;
        Ok(Self { _lease: lease })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::error::LimitKind;
    use crate::policy::{CancellationToken, SourceBudget, SourceLimits};

    #[test]
    fn deadline_after_string_and_vec_allocation_returns_their_reservations() {
        let budget = SourceBudget::task_fixture();
        let deadline = CoreError::DeadlineExceeded {
            elapsed_ns: 500_000_000,
            limit_ns: 500_000_000,
        };
        let mut op = budget.begin(CancellationToken::new());
        op.set_clock_samples_for_test(&[100_000_000, 500_000_000]);
        assert!(matches!(
            ChargedString::source(8, &mut op),
            Err(error) if error == deadline
        ));
        assert_eq!(op.first_fault(), Some(deadline));
        assert!(budget.usage().source_capacity_peak >= 8);
        assert_eq!(budget.usage().source_capacity, 0);

        let mut op = budget.begin(CancellationToken::new());
        op.set_clock_samples_for_test(&[100_000_000, 500_000_000]);
        assert!(matches!(
            ChargedVec::<u64>::new(2, &mut op),
            Err(error) if error == deadline
        ));
        assert_eq!(op.first_fault(), Some(deadline));
        assert!(budget.usage().auxiliary_capacity_peak >= 16);
        assert_eq!(budget.usage().auxiliary_capacity, 0);
    }

    #[test]
    fn deadline_after_growth_allocation_keeps_the_original_flat_buffer() {
        let budget = SourceBudget::task_fixture();
        let mut preparation = budget.begin(CancellationToken::new());
        preparation.set_clock_samples_for_test(&[100_000_000]);
        let mut rows = GrowingVec::new(4, LimitKind::IndexRecords, &mut preparation).unwrap();
        rows.push(7_u64, &mut preparation).unwrap();
        let retained = budget.usage().auxiliary_capacity;

        let mut op = budget.begin(CancellationToken::new());
        op.set_clock_samples_for_test(&[100_000_000, 500_000_000]);
        let deadline = CoreError::DeadlineExceeded {
            elapsed_ns: 500_000_000,
            limit_ns: 500_000_000,
        };
        assert_eq!(rows.push(8, &mut op), Err(deadline));
        assert_eq!(rows.as_slice(), &[7]);
        assert_eq!(budget.usage().auxiliary_capacity, retained);
        assert!(budget.usage().auxiliary_capacity_peak > retained);
        drop(rows);
        assert_eq!(budget.usage().auxiliary_capacity, 0);
    }

    #[test]
    fn shared_sort_cancels_inside_a_live_merge_and_releases_scratch() {
        let budget = SourceBudget::model_fixture();
        let token = CancellationToken::new();
        let mut op = budget.begin(token.clone());
        let mut rows = GrowingVec::new(128, LimitKind::IndexRecords, &mut op).unwrap();
        for value in (0..128_u32).rev() {
            rows.push(value, &mut op).unwrap();
        }
        let retained = budget.usage().auxiliary_capacity;
        let mut inside_merge = false;
        let error = sort_by(
            &mut rows,
            0,
            |a, b, _| {
                if budget.usage().auxiliary_capacity > retained {
                    inside_merge = true;
                    token.cancel();
                }
                Ok(a.cmp(&b))
            },
            &mut op,
        )
        .unwrap_err();
        assert!(inside_merge);
        assert_eq!(error, CoreError::Cancelled);
        assert_eq!(op.first_fault(), Some(error));
        assert_eq!(budget.usage().auxiliary_capacity, retained);
        drop(rows);
        assert_eq!(budget.usage().auxiliary_capacity, 0);
    }

    #[test]
    fn shared_sort_ordered_runs_need_no_scratch_and_bad_prefix_is_typed() {
        let budget = SourceBudget::model_fixture();
        let mut op = budget.begin(CancellationToken::new());
        let mut rows = GrowingVec::new(16, LimitKind::IndexRecords, &mut op).unwrap();
        rows.extend_from_slice(&[99_u32, 0, 1, 1, 2, 3], &mut op)
            .unwrap();
        let before = budget.usage();
        sort_by(&mut rows, 1, |a, b, _| Ok(a.cmp(&b)), &mut op).unwrap();
        assert_eq!(rows.as_slice(), &[99, 0, 1, 1, 2, 3]);
        assert_eq!(
            budget.usage().auxiliary_capacity_peak,
            before.auxiliary_capacity_peak
        );
        assert_eq!(
            sort_by(&mut rows, 7, |a, b, _| Ok(a.cmp(&b)), &mut op),
            Err(CoreError::InvalidRange)
        );
    }

    #[test]
    fn shared_sort_counts_both_scratch_writes_and_writeback() {
        let budget = SourceBudget::model_fixture();
        let mut op = budget.begin(CancellationToken::new());
        let mut rows = GrowingVec::new(2, LimitKind::IndexRecords, &mut op).unwrap();
        rows.extend_from_slice(&[2_u64, 1], &mut op).unwrap();
        let before = op.stats().expect("statistics enabled").copied_bytes();
        sort_by(&mut rows, 0, |a, b, _| Ok(a.cmp(&b)), &mut op).unwrap();
        assert_eq!(rows.as_slice(), &[1, 2]);
        assert_eq!(
            op.stats().expect("statistics enabled").copied_bytes() - before,
            32
        );
    }

    #[test]
    fn failed_capacity_and_layout_reservations_return_every_lease() {
        let budget = SourceBudget::new(SourceLimits {
            retained_source_capacity: 3,
            ..SourceLimits::default()
        })
        .unwrap();
        let mut op = budget.begin(CancellationToken::new());
        assert!(matches!(
            ChargedString::source(4, &mut op),
            Err(CoreError::ResourceLimit {
                kind: LimitKind::SourceCapacity,
                ..
            })
        ));
        assert_eq!(budget.usage().source_capacity, 0);
        let mut op = budget.begin(CancellationToken::new());
        assert!(matches!(
            ChargedVec::<u64>::new(usize::MAX, &mut op),
            Err(CoreError::ArithmeticOverflow)
        ));
        assert_eq!(budget.usage().auxiliary_capacity, 0);
    }

    #[test]
    fn fallible_allocation_error_rolls_back_preallocation_reservation() {
        let budget = SourceBudget::new(SourceLimits {
            retained_source_capacity: usize::MAX as u64,
            ..SourceLimits::default()
        })
        .unwrap();
        let mut op = budget.begin(CancellationToken::new());
        // Invalid Rust allocation layout; no giant physical allocation is attempted.
        assert!(matches!(
            ChargedString::source(usize::MAX, &mut op),
            Err(CoreError::OutOfMemory)
        ));
        assert_eq!(budget.usage().source_capacity, 0);
        assert_eq!(op.first_fault(), Some(CoreError::OutOfMemory));
    }

    #[test]
    fn cancellation_does_not_prevent_existing_owner_cleanup() {
        let budget = SourceBudget::source_fixture();
        let cancel = CancellationToken::new();
        let mut op = budget.begin(cancel.clone());
        let mut text = ChargedString::auxiliary(8, &mut op).unwrap();
        text.append("abc", &mut op).unwrap();
        cancel.cancel();
        assert_eq!(text.append("d", &mut op), Err(CoreError::Cancelled));
        assert_eq!(text.as_str(), "abc");
        assert_eq!(budget.usage().auxiliary_capacity, 8);
        drop(text);
        assert_eq!(budget.usage().auxiliary_capacity, 0);
    }

    #[test]
    fn containers_cannot_grow_past_their_admitted_capacity() {
        let budget = SourceBudget::source_fixture();
        let mut op = budget.begin(CancellationToken::new());
        let mut records = ChargedVec::<u64>::new(1, &mut op).unwrap();
        records.push(7, &mut op).unwrap();
        assert_eq!(records.push(8, &mut op), Err(CoreError::InternalInvariant));
        assert_eq!(records.as_slice(), &[7]);
        drop(records);
        assert_eq!(budget.usage().auxiliary_capacity, 0);
    }

    #[test]
    fn growing_buffers_admit_old_and_new_together_and_rollback_failure() {
        let budget = SourceBudget::new(SourceLimits {
            auxiliary_capacity: 32,
            ..SourceLimits::default()
        })
        .unwrap();
        let mut op = budget.begin(CancellationToken::new());
        let mut rows = GrowingVec::<u64>::new(8, LimitKind::ModelRecords, &mut op).unwrap();
        rows.push(1, &mut op).unwrap();
        rows.push(2, &mut op).unwrap();
        assert_eq!(budget.usage().auxiliary_capacity, 16);
        // Realloc's net delta would fit, but old16 + complete new32 does not.
        assert_eq!(
            rows.push(3, &mut op),
            Err(CoreError::ResourceLimit {
                kind: LimitKind::AuxiliaryCapacity,
                requested: 48,
                limit: 32,
            })
        );
        assert_eq!(rows.as_slice(), &[1, 2]);
        assert_eq!(budget.usage().auxiliary_capacity, 16);
        drop(rows);
        assert_eq!(budget.usage().auxiliary_capacity, 0);
    }

    #[test]
    fn growth_moves_are_bounded_metered_and_last_drop_releases_reservations() {
        let budget = SourceBudget::model_fixture();
        let mut op = budget.begin(CancellationToken::new());
        let mut rows = GrowingVec::<u64>::new(5, LimitKind::ModelRecords, &mut op).unwrap();
        for value in 0..5 {
            rows.push(value, &mut op).unwrap();
        }
        assert_eq!(rows.as_slice(), &[0, 1, 2, 3, 4]);
        // Writes40 + growth moves(8+16+32), including non-power-of-two cap5.
        assert_eq!(op.stats().expect("statistics enabled").copied_bytes(), 96);
        assert_eq!(budget.usage().auxiliary_capacity, 40);
        assert_eq!(budget.usage().auxiliary_capacity_peak, 72);
        assert_eq!(
            rows.push(5, &mut op),
            Err(CoreError::ResourceLimit {
                kind: LimitKind::ModelRecords,
                requested: 6,
                limit: 5,
            })
        );
        drop(rows);
        assert_eq!(budget.usage().auxiliary_capacity, 0);
    }

    #[test]
    fn a_fault_during_bounded_move_frees_new_storage_and_keeps_old_storage() {
        let budget = SourceBudget::new(SourceLimits {
            max_work: 40,
            ..SourceLimits::default()
        })
        .unwrap();
        let cancel = CancellationToken::new();
        let mut op = budget.begin(cancel.clone());
        let mut rows = GrowingVec::<u64>::new(4, LimitKind::ModelRecords, &mut op).unwrap();
        rows.push(1, &mut op).unwrap();
        rows.push(2, &mut op).unwrap();
        let error = rows.push(3, &mut op).unwrap_err();
        assert!(matches!(
            error,
            CoreError::ResourceLimit {
                kind: LimitKind::Work,
                ..
            }
        ));
        assert_eq!(rows.as_slice(), &[1, 2]);
        assert_eq!(budget.usage().auxiliary_capacity, 16);
        cancel.cancel();
        assert_eq!(op.checkpoint(), Err(error));
        drop(rows);
        assert_eq!(budget.usage().auxiliary_capacity, 0);
    }
}
