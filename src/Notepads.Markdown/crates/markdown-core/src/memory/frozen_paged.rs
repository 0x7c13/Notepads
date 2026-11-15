// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE file in the project root for license information.
// ---------------------------------------------------------------------------------------------

//! Immutable physical pages. Freezing never copies the row buffers.

use super::{BuildRecord, BuildStep, BuildValue, BuildWindow, FrozenRows};
use crate::error::CoreError;
use crate::policy::ConstructionId;
use std::marker::PhantomData;

pub(crate) struct FrozenPagedRows<T> {
    pub(in crate::memory) pages: FrozenRows<FrozenRows<T>>,
    pub(in crate::memory) len: usize,
    pub(in crate::memory) width: usize,
}

impl<T> FrozenPagedRows<T> {
    pub(crate) fn len(&self) -> usize {
        self.len
    }

    pub(crate) fn get(&self, index: usize) -> Option<&T> {
        if index >= self.len {
            return None;
        }
        self.pages
            .as_slice()
            .get(index / self.width)?
            .as_slice()
            .get(index % self.width)
    }

    #[cfg(test)]
    pub(crate) fn iter(&self) -> impl ExactSizeIterator<Item = &T> + DoubleEndedIterator {
        (0..self.len)
            .map(|index| &self.pages.as_slice()[index / self.width].as_slice()[index % self.width])
    }

    #[cfg(test)]
    pub(crate) fn range(&self, start: usize, len: usize) -> Option<PagedRange<'_, T>> {
        let end = start.checked_add(len)?;
        (end <= self.len).then_some(PagedRange {
            rows: self,
            start,
            end,
        })
    }
}

#[cfg(test)]
pub(crate) struct PagedRange<'a, T> {
    rows: &'a FrozenPagedRows<T>,
    start: usize,
    end: usize,
}

#[cfg(test)]
impl<'a, T> PagedRange<'a, T> {
    pub(crate) fn len(&self) -> usize {
        self.end - self.start
    }

    pub(crate) fn get(&self, index: usize) -> Option<&'a T> {
        if index >= self.len() {
            return None;
        }
        self.rows.get(self.start + index)
    }

    pub(crate) fn iter(&self) -> impl ExactSizeIterator<Item = &'a T> + DoubleEndedIterator {
        (self.start..self.end).map(|index| {
            &self.rows.pages.as_slice()[index / self.rows.width].as_slice()[index % self.rows.width]
        })
    }
}

pub(crate) struct BuildFrozenPagedRead<'scope, 'id, T: BuildValue> {
    rows: &'scope FrozenPagedRows<T>,
    scope: ConstructionId<'scope>,
    id: ConstructionId<'id>,
}

impl<'scope, 'call, 'id, 'op> BuildWindow<'scope, 'call, 'id, 'op> {
    pub(crate) fn read_frozen_paged<T: BuildValue>(
        &mut self,
        rows: &'scope FrozenPagedRows<T>,
    ) -> Result<BuildStep<BuildFrozenPagedRead<'scope, 'id, T>>, CoreError> {
        self.value::<T>()?;
        if !self.binding::<FrozenPagedRows<T>>()? {
            return Ok(BuildStep::Full);
        }
        Ok(BuildStep::Ready(BuildFrozenPagedRead {
            rows,
            scope: PhantomData,
            id: PhantomData,
        }))
    }
}

impl<'scope, 'id, T: BuildValue> BuildFrozenPagedRead<'scope, 'id, T> {
    pub(crate) fn get(
        &self,
        index: usize,
        window: &mut BuildWindow<'scope, '_, 'id, '_>,
    ) -> Result<BuildStep<Option<T>>, CoreError> {
        if !window.action(
            (4 * size_of::<T>() + 2 * size_of::<Self>() + 2 * size_of::<FrozenRows<T>>()) as u64,
        )? {
            return Ok(BuildStep::Full);
        }
        // Same contract as a contiguous frozen view: the semantic caller owns
        // its scan/copy fee; the physical view bounds all descriptor accesses.
        Ok(BuildStep::Ready(self.rows.get(index).copied()))
    }
}

impl<'scope, 'id, T: BuildRecord> BuildFrozenPagedRead<'scope, 'id, T> {
    pub(crate) fn record(
        &self,
        index: usize,
        window: &mut BuildWindow<'scope, '_, 'id, '_>,
    ) -> Result<BuildStep<T::Output>, CoreError> {
        window.value::<T::Output>()?;
        if !window.action(
            (4 * size_of::<T>()
                + 4 * size_of::<T::Output>()
                + 2 * size_of::<Self>()
                + 2 * size_of::<FrozenRows<T>>()) as u64,
        )? {
            return Ok(BuildStep::Full);
        }
        window.parts().event()?;
        let row = self
            .rows
            .get(index)
            .ok_or_else(|| window.fail(CoreError::InvalidProvenance))?;
        row.project(index, window.parts())
            .map(BuildStep::Ready)
            .map_err(|error| window.fail(error))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::error::LimitKind;
    use crate::memory::{BuildKernel, BuildRows, run_window};
    use crate::policy::{CancellationToken, ConstructionCharges, SourceBudget, construction_scope};

    #[derive(Clone, Copy)]
    struct Record(u64);
    impl BuildValue for Record {}
    impl BuildRecord for Record {
        type Output = u64;
        fn project(
            &self,
            _: usize,
            work: &mut ConstructionCharges<'_, '_, '_>,
        ) -> Result<u64, CoreError> {
            work.scan(size_of::<Self>() as u64)?;
            work.copy(size_of::<u64>() as u64)?;
            Ok(self.0)
        }
    }

    struct ReadKernel<'a> {
        rows: &'a FrozenPagedRows<Record>,
        index: usize,
        project: bool,
    }
    impl<'id> BuildKernel<'id> for ReadKernel<'_> {
        type Output = Option<u64>;
        fn run<'scope>(
            &'scope mut self,
            window: &mut BuildWindow<'scope, '_, 'id, '_>,
        ) -> Result<BuildStep<Self::Output>, CoreError> {
            let view = match window.read_frozen_paged(self.rows)? {
                BuildStep::Ready(view) => view,
                BuildStep::Full => return Ok(BuildStep::Full),
                BuildStep::NeedCapacity(_) => return Err(window.fail(CoreError::InternalInvariant)),
            };
            if self.project {
                return Ok(match view.record(self.index, window)? {
                    BuildStep::Ready(value) => BuildStep::Ready(Some(value)),
                    BuildStep::Full => BuildStep::Full,
                    BuildStep::NeedCapacity(_) => {
                        return Err(window.fail(CoreError::InternalInvariant));
                    }
                });
            }
            Ok(match view.get(self.index, window)? {
                BuildStep::Ready(value) => BuildStep::Ready(value.map(|row| row.0)),
                BuildStep::Full => BuildStep::Full,
                BuildStep::NeedCapacity(_) => return Err(window.fail(CoreError::InternalInvariant)),
            })
        }
    }

    #[test]
    fn physical_frozen_window_crosses_pages_and_projects_the_actual_record() {
        let budget = SourceBudget::task_fixture();
        let mut op = budget.begin(CancellationToken::new());
        construction_scope(&mut op, |mut work| {
            let mut rows = BuildRows::new(1100, LimitKind::IndexRecords, &mut work).unwrap();
            for index in 0..1100 {
                rows.push(Record(index), &mut work).unwrap();
            }
            let frozen = rows.freeze(&mut work).unwrap();
            for project in [false, true] {
                for index in [0, 1023, 1024, 1099] {
                    assert_eq!(
                        run_window(
                            &mut ReadKernel {
                                rows: &frozen,
                                index,
                                project
                            },
                            &mut work
                        ),
                        Ok(BuildStep::Ready(Some(index as u64)))
                    );
                }
            }
            assert_eq!(
                run_window(
                    &mut ReadKernel {
                        rows: &frozen,
                        index: 1100,
                        project: false
                    },
                    &mut work
                ),
                Ok(BuildStep::Ready(None))
            );
            assert_eq!(
                run_window(
                    &mut ReadKernel {
                        rows: &frozen,
                        index: 1100,
                        project: true
                    },
                    &mut work
                ),
                Err(CoreError::InvalidProvenance)
            );
            assert_eq!(work.first_fault(), Some(CoreError::InvalidProvenance));
            drop(frozen);
            assert_eq!(budget.usage().auxiliary_capacity, 0);
            assert_eq!(work.finish(), Err(CoreError::InvalidProvenance));
        });
    }

    #[test]
    fn empty_frozen_table_has_valid_empty_range_and_no_row_allocation() {
        let budget = SourceBudget::task_fixture();
        let mut op = budget.begin(CancellationToken::new());
        construction_scope(&mut op, |mut work| {
            let rows = BuildRows::<u64>::new(0, LimitKind::IndexRecords, &mut work).unwrap();
            let frozen = rows.freeze(&mut work).unwrap();
            assert_eq!(frozen.len(), 0);
            assert!(frozen.get(0).is_none());
            assert_eq!(frozen.range(0, 0).unwrap().iter().count(), 0);
            assert!(frozen.range(0, 1).is_none());
            assert_eq!(budget.usage().auxiliary_capacity, 0);
            drop(frozen);
            work.finish().unwrap();
        });
    }
}
