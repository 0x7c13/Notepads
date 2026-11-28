// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE file in the project root for license information.
// ---------------------------------------------------------------------------------------------
//! Allocation-free borrowed Copy algorithms. An owner wrapper must record an
//! error immediately on return, before dropping its paid buffers. There are
//! no owned temporaries in these algorithms whose cleanup can replace it.

use crate::storage::{copy_window, copy_work};
use crate::{Error, FailureKind, LimitKind, WorkControl};
use core::{cmp::Ordering, ops::Range};

fn invalid_range() -> Error {
    Error::new(FailureKind::Invariant, LimitKind::Capacity)
}

/// Preserve memmove overlap semantics with a check before each <=256-record,
/// <=64KiB window. Error leaves valid Copy values and may have moved a prefix
/// (or suffix for rightward overlap); it does not promise a rollback.
pub fn copy_within_with_control<T: Copy, C: WorkControl<Error = Error>>(
    values: &mut [T],
    source: Range<usize>,
    destination: usize,
    control: &mut C,
) -> Result<(), Error> {
    control.checkpoint(0)?;
    let window = copy_window::<T>()?;
    if source.start > source.end || source.end > values.len() {
        return Err(invalid_range());
    }
    let length = source.end - source.start;
    if destination
        .checked_add(length)
        .is_none_or(|end| end > values.len())
    {
        return Err(invalid_range());
    }
    if destination == source.start {
        return control.checkpoint(0);
    }
    if destination > source.start && destination < source.end {
        let mut remaining = length;
        while remaining != 0 {
            let count = window.min(remaining);
            let offset = remaining - count;
            control.checkpoint(copy_work::<T>(count))?;
            values.copy_within(
                source.start + offset..source.start + remaining,
                destination + offset,
            );
            remaining = offset;
        }
    } else {
        let mut copied = 0;
        while copied < length {
            let count = window.min(length - copied);
            control.checkpoint(copy_work::<T>(count))?;
            values.copy_within(
                source.start + copied..source.start + copied + count,
                destination + copied,
            );
            copied += count;
        }
    }
    control.checkpoint(0)
}

/// Stable merge sort into caller-owned initialized scratch. Equal keys select
/// the left record. The comparator receives this same control so nonconstant
/// key comparisons can check their own inner work without a second account.
/// An error can leave a partially copied valid array; success alone guarantees
/// a sorted permutation. Scratch must contain at least values.len() records.
/// The comparator is borrowed: its captured owners belong to the caller, which
/// must record an error before retiring them and check the slot afterwards.
pub fn stable_sort_by_with_control<T: Copy, C: WorkControl<Error = Error>>(
    values: &mut [T],
    scratch: &mut [T],
    control: &mut C,
    compare: &mut impl FnMut(&T, &T, &mut C) -> Result<Ordering, Error>,
) -> Result<(), Error> {
    control.checkpoint(0)?;
    let window = copy_window::<T>()?;
    let length = values.len();
    if scratch.len() < length {
        return Err(invalid_range());
    }
    let mut width = 1usize;
    while width < length {
        control.checkpoint(1)?;
        let mut start = 0;
        while start < length {
            control.checkpoint(1)?;
            let middle = start.saturating_add(width).min(length);
            let end = middle.saturating_add(width).min(length);
            let (mut left, mut right, mut write) = (start, middle, start);
            while left < middle && right < end {
                control.checkpoint(1)?;
                let ordering = compare(&values[left], &values[right], control)?;
                control.checkpoint(0)?;
                let selected = if ordering != Ordering::Greater {
                    let index = left;
                    left += 1;
                    index
                } else {
                    let index = right;
                    right += 1;
                    index
                };
                control.checkpoint(copy_work::<T>(1))?;
                scratch[write] = values[selected];
                write += 1;
            }
            for (mut input, finish) in [(left, middle), (right, end)] {
                while input < finish {
                    let count = window.min(finish - input);
                    control.checkpoint(copy_work::<T>(count))?;
                    scratch[write..write + count].copy_from_slice(&values[input..input + count]);
                    input += count;
                    write += count;
                }
            }
            start = end;
        }
        for (target, source) in values
            .chunks_mut(window)
            .zip(scratch[..length].chunks(window))
        {
            control.checkpoint(copy_work::<T>(target.len()))?;
            target.copy_from_slice(source);
        }
        width = width.saturating_mul(2);
    }
    control.checkpoint(0)
}
