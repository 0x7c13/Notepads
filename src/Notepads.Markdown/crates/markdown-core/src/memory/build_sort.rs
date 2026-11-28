// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE file in the project root for license information.
// ---------------------------------------------------------------------------------------------

//! Stable admitted merge with one construction identity throughout.
use super::{BuildExactRows, BuildGrowingVec, BuildVec};
use crate::error::CoreError;
use crate::policy::OwnedWork;
use std::cmp::Ordering;

pub(crate) trait BuildSortRows<'id, T: Copy> {
    fn len(&self) -> usize;
    // Audited sole caller: this module, retaining the original sort fees.
    fn get_for_sort(&self, index: usize, work: &mut OwnedWork<'id, '_>) -> Result<T, CoreError>;
    fn replace(
        &mut self,
        index: usize,
        value: T,
        work: &mut OwnedWork<'id, '_>,
    ) -> Result<(), CoreError>;
}
macro_rules! rows {
    ($name:ident) => {
        impl<'id, T: Copy> BuildSortRows<'id, T> for $name<'id, T> {
            fn len(&self) -> usize {
                self.len()
            }
            fn get_for_sort(
                &self,
                index: usize,
                work: &mut OwnedWork<'id, '_>,
            ) -> Result<T, CoreError> {
                sort_value(self.as_slice(), index, work)
            }
            fn replace(
                &mut self,
                index: usize,
                value: T,
                work: &mut OwnedWork<'id, '_>,
            ) -> Result<(), CoreError> {
                self.replace(index, value, work)
            }
        }
    };
}
rows!(BuildGrowingVec);
rows!(BuildExactRows);
fn row<'id, T: Copy>(
    values: &impl BuildSortRows<'id, T>,
    index: usize,
    work: &mut OwnedWork<'id, '_>,
) -> Result<T, CoreError> {
    values.get_for_sort(index, work)
}

// Fixed owned access within the already bounded sort loop, with no exported
// slice or duplicate metadata fees on top of its original comparison/copy fees.
fn sort_value<T: Copy>(
    values: &[T],
    index: usize,
    work: &mut OwnedWork<'_, '_>,
) -> Result<T, CoreError> {
    work.check()?;
    if size_of::<T>() == 0 || size_of::<T>() as u64 > crate::policy::CHECKPOINT_BYTES {
        return Err(work.fail(CoreError::InternalInvariant));
    }
    values
        .get(index)
        .copied()
        .ok_or_else(|| work.fail(CoreError::InternalInvariant))
}

pub(crate) fn sort_build_rows_by<'id, T: Copy>(
    values: &mut impl BuildSortRows<'id, T>,
    start: usize,
    mut compare: impl FnMut(T, T, &mut OwnedWork<'id, '_>) -> Result<Ordering, CoreError>,
    work: &mut OwnedWork<'id, '_>,
) -> Result<(), CoreError> {
    work.check()?;
    let end_of_rows = values.len();
    let length = end_of_rows
        .checked_sub(start)
        .ok_or_else(|| work.fail(CoreError::InvalidRange))?;
    let mut ordered = true;
    for index in start.saturating_add(1)..end_of_rows {
        work.events(1)?;
        let left = row(values, index - 1, work)?;
        let right = row(values, index, work)?;
        if compare(left, right, work)
            .map_err(|error| work.fail(error))?
            .is_gt()
        {
            ordered = false;
            break;
        }
    }
    if ordered {
        return work.check();
    }
    let mut width = 1_usize;
    while width < length {
        let mut start = start;
        while start < end_of_rows {
            work.events(1)?;
            let middle = start
                .checked_add(width)
                .ok_or_else(|| work.fail(CoreError::ArithmeticOverflow))?
                .min(end_of_rows);
            let end = middle
                .checked_add(width)
                .ok_or_else(|| work.fail(CoreError::ArithmeticOverflow))?
                .min(end_of_rows);
            let merge = if middle < end {
                let left = row(values, middle - 1, work)?;
                let right = row(values, middle, work)?;
                compare(left, right, work)
                    .map_err(|error| work.fail(error))?
                    .is_gt()
            } else {
                false
            };
            if merge {
                // Exact scratch preserves the previous algorithm's allocation shape.
                let mut scratch = BuildVec::new(end - start, work)?;
                let mut left = start;
                let mut right = middle;
                while left < middle || right < end {
                    work.events(1)?;
                    let take_left = if left < middle {
                        if right == end {
                            true
                        } else {
                            let a = row(values, left, work)?;
                            let b = row(values, right, work)?;
                            !compare(a, b, work)
                                .map_err(|error| work.fail(error))?
                                .is_gt()
                        }
                    } else {
                        false
                    };
                    let index = if take_left {
                        let at = left;
                        left += 1;
                        at
                    } else {
                        let at = right;
                        right += 1;
                        at
                    };
                    work.copy(size_of::<T>() as u64)?;
                    let value = row(values, index, work)?;
                    scratch.push(value, work)?;
                }
                for offset in 0..scratch.len() {
                    work.events(1)?;
                    let value = sort_value(scratch.as_slice(), offset, work)?;
                    values.replace(start + offset, value, work)?;
                }
            }
            start = end;
        }
        width = width
            .checked_mul(2)
            .ok_or_else(|| work.fail(CoreError::ArithmeticOverflow))?;
    }
    work.check()
}
