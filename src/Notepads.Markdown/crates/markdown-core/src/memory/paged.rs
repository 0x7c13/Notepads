// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE file in the project root for license information.
// ---------------------------------------------------------------------------------------------

//! Fallible pages with stable logical indexes and explicit last-use retirement.

pub(super) mod build;

#[cfg(test)]
use super::ChargedVec;
#[cfg(test)]
use super::{TaskAbandon, TaskStep, TaskValue, TaskWindow};
#[cfg(test)]
use crate::error::{CoreError, LimitKind};
#[cfg(test)]
use crate::policy::{CHECKPOINT_BYTES, Lease, MAX_METADATA_VIEW_BYTES, Operation, SourceBudget};
#[cfg(test)]
use std::cell::Cell;
#[cfg(test)]
use std::marker::PhantomData;

#[cfg(test)]
struct Owners<T> {
    // Allocation and owned pages are dropped before their descriptor lease.
    values: Vec<Option<ChargedVec<T>>>,
    _lease: Option<Lease>,
}

#[cfg(test)]
pub(crate) struct TaskPagedRead<'scope, T: TaskValue> {
    rows: &'scope PagedRows<T>,
    page: Option<(usize, &'scope [T])>,
    scope: PhantomData<fn(&'scope ()) -> &'scope ()>,
}

#[cfg(test)]
pub(crate) struct TaskPagedWrite<'scope, T: TaskValue> {
    owners: &'scope mut Owners<T>,
    fault: &'scope Cell<Option<CoreError>>,
    len: usize,
    width: usize,
    scope: PhantomData<fn(&'scope ()) -> &'scope ()>,
}

#[cfg(test)]
impl<'scope> TaskWindow<'scope, '_> {
    pub(crate) fn read_rows<T: TaskValue>(
        &mut self,
        rows: &'scope PagedRows<T>,
    ) -> Result<TaskStep<TaskPagedRead<'scope, T>>, CoreError> {
        self.value::<T>()?;
        if !self.binding::<PagedRows<T>>()? {
            return Ok(TaskStep::Full);
        }
        let result = (|| {
            if let Some(error) = rows.fault.get() {
                return Err(self.fail(error));
            }
            self.parts().ensure_budget(&rows.budget)
        })();
        rows.latch(result)?;
        Ok(TaskStep::Ready(TaskPagedRead {
            rows,
            page: None,
            scope: PhantomData,
        }))
    }

    pub(crate) fn write_rows<T: TaskValue>(
        &mut self,
        rows: &'scope mut PagedRows<T>,
    ) -> Result<TaskStep<TaskPagedWrite<'scope, T>>, CoreError> {
        self.value::<T>()?;
        if !self.binding::<PagedRows<T>>()? {
            return Ok(TaskStep::Full);
        }
        let result = match rows.fault.get() {
            Some(error) => Err(self.fail(error)),
            None => self.parts().ensure_budget(&rows.budget),
        };
        rows.latch(result)?;
        if matches!(
            self.owner(&rows.fault, TaskAbandon::InternalInvariant)?,
            TaskStep::Full
        ) {
            return Ok(TaskStep::Full);
        }
        Ok(TaskStep::Ready(TaskPagedWrite {
            owners: &mut rows.owners,
            fault: &rows.fault,
            len: rows.len,
            width: rows.width,
            scope: PhantomData,
        }))
    }
}

#[cfg(test)]
fn task_row_bytes<T: TaskValue>() -> u64 {
    // Include owned result transport, the actual page descriptor and the
    // fixed view/cache headers. No payload scan is hidden in these headers.
    (4 * size_of::<T>()
        + 2 * size_of::<Option<ChargedVec<T>>>()
        + 2 * size_of::<TaskPagedRead<'_, T>>().max(size_of::<TaskPagedWrite<'_, T>>())) as u64
}

#[cfg(test)]
impl<'scope, T: TaskValue> TaskPagedRead<'scope, T> {
    pub(crate) fn get(
        &mut self,
        index: usize,
        window: &mut TaskWindow<'scope, '_>,
    ) -> Result<TaskStep<T>, CoreError> {
        if !window.action(task_row_bytes::<T>())? {
            return Ok(TaskStep::Full);
        }
        let result = (|| {
            window.parts().event()?;
            if index >= self.rows.len {
                return Err(window.fail(CoreError::InternalInvariant));
            }
            let page_index = index / self.rows.width;
            if !matches!(self.page, Some((at, _)) if at == page_index) {
                let page = self
                    .rows
                    .owners
                    .values
                    .get(page_index)
                    .and_then(Option::as_ref)
                    .ok_or_else(|| window.fail(CoreError::InternalInvariant))?;
                self.page = Some((page_index, page.as_slice()));
            }
            window.parts().view_read(size_of::<T>() as u64)?;
            let page = self
                .page
                .ok_or_else(|| window.fail(CoreError::InternalInvariant))?
                .1;
            page.get(index % self.rows.width)
                .copied()
                .map(TaskStep::Ready)
                .ok_or_else(|| window.fail(CoreError::InternalInvariant))
        })();
        self.rows.latch(result)
    }
}

#[cfg(test)]
impl<'scope, T: TaskValue> TaskPagedWrite<'scope, T> {
    pub(crate) fn replace(
        &mut self,
        index: usize,
        value: T,
        window: &mut TaskWindow<'scope, '_>,
    ) -> Result<TaskStep<()>, CoreError> {
        if !window.action(task_row_bytes::<T>())? {
            return Ok(TaskStep::Full);
        }
        let result = (|| {
            window.parts().event()?;
            if index >= self.len {
                return Err(window.fail(CoreError::InternalInvariant));
            }
            let slot = self
                .owners
                .values
                .get_mut(index / self.width)
                .and_then(Option::as_mut)
                .and_then(|page| page.values.get_mut(index % self.width))
                .ok_or_else(|| window.fail(CoreError::InternalInvariant))?;
            window.parts().copy(size_of::<T>() as u64)?;
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

/// Fixed metadata rows; source/text bytes belong in their separate byte owners.
#[cfg(test)]
pub(crate) struct PagedRows<T: Copy> {
    owners: Owners<T>,
    budget: SourceBudget,
    len: usize,
    max_len: usize,
    width: usize,
    kind: LimitKind,
    fault: Cell<Option<CoreError>>,
}

#[cfg(test)]
impl<T: Copy> PagedRows<T> {
    pub(crate) fn new(
        max_len: u64,
        kind: LimitKind,
        op: &mut Operation,
    ) -> Result<Self, CoreError> {
        op.events(1)?;
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
            owners: Owners {
                values: Vec::new(),
                _lease: None,
            },
            budget: op.budget().clone(),
            len: 0,
            max_len,
            width: (16384 / size).clamp(1, 1024),
            kind,
            fault: Cell::new(None),
        })
    }

    pub(crate) fn len(&self) -> usize {
        self.len
    }

    pub(crate) fn page_width(&self) -> usize {
        self.width
    }

    pub(crate) fn page_count(&self) -> usize {
        self.owners.values.len()
    }

    fn ensure(&self, op: &mut Operation) -> Result<(), CoreError> {
        if let Some(error) = self.fault.get() {
            return Err(op.fail(error));
        }
        op.ensure_budget(&self.budget)?;
        Ok(())
    }

    pub(crate) fn get(&self, index: usize, op: &mut Operation) -> Result<T, CoreError> {
        self.latch(self.get_inner(index, op))
    }

    fn get_inner(&self, index: usize, op: &mut Operation) -> Result<T, CoreError> {
        self.ensure(op)?;
        op.events(1)?;
        if index >= self.len {
            return Err(op.fail(CoreError::InternalInvariant));
        }
        let page = self.owners.values[index / self.width]
            .as_ref()
            .ok_or_else(|| op.fail(CoreError::InternalInvariant))?;
        let bytes = size_of::<T>() as u64;
        let read = || {
            page.as_slice()
                .get(index % self.width)
                .copied()
                .ok_or(CoreError::InternalInvariant)
        };
        if bytes <= MAX_METADATA_VIEW_BYTES {
            op.read_metadata(bytes, read)
        } else {
            op.scan(bytes)?;
            op.copy(bytes)?;
            read().map_err(|error| op.fail(error))
        }
    }

    pub(crate) fn push(&mut self, value: T, op: &mut Operation) -> Result<usize, CoreError> {
        let result = self.push_inner(value, op);
        self.latch(result)
    }

    fn push_inner(&mut self, value: T, op: &mut Operation) -> Result<usize, CoreError> {
        self.ensure(op)?;
        op.events(1)?;
        let required = self
            .len
            .checked_add(1)
            .ok_or_else(|| op.fail(CoreError::ArithmeticOverflow))?;
        op.check_limit(self.kind, required as u64, self.max_len as u64)?;
        if self.len.is_multiple_of(self.width) {
            self.grow_owners(op)?;
            let capacity = self.width.min(self.max_len - self.len);
            let page = ChargedVec::new(capacity, op)?;
            op.copy(size_of::<Option<ChargedVec<T>>>() as u64)?;
            self.owners.values.push(Some(page));
        }
        let page = self.owners.values[self.len / self.width]
            .as_mut()
            .ok_or_else(|| op.fail(CoreError::InternalInvariant))?;
        op.copy(size_of::<T>() as u64)?;
        page.push(value, op)?;
        let index = self.len;
        self.len = required;
        Ok(index)
    }

    pub(crate) fn replace(
        &mut self,
        index: usize,
        value: T,
        op: &mut Operation,
    ) -> Result<(), CoreError> {
        let result = self.replace_inner(index, value, op);
        self.latch(result)
    }

    fn replace_inner(
        &mut self,
        index: usize,
        value: T,
        op: &mut Operation,
    ) -> Result<(), CoreError> {
        self.ensure(op)?;
        op.events(1)?;
        if index >= self.len {
            return Err(op.fail(CoreError::InternalInvariant));
        }
        let page = self.owners.values[index / self.width]
            .as_mut()
            .ok_or_else(|| op.fail(CoreError::InternalInvariant))?;
        let slot = page
            .values
            .get_mut(index % self.width)
            .ok_or_else(|| op.fail(CoreError::InternalInvariant))?;
        op.copy(size_of::<T>() as u64)?;
        *slot = value;
        Ok(())
    }

    /// The caller must prove that no remaining consumer needs this page.
    /// Logical indexes are neither removed nor reused.
    pub(crate) fn retire_page(
        &mut self,
        page_index: usize,
        op: &mut Operation,
    ) -> Result<(), CoreError> {
        let result = (|| {
            self.ensure(op)?;
            op.events(1)?;
            let slot = self
                .owners
                .values
                .get_mut(page_index)
                .ok_or_else(|| op.fail(CoreError::InternalInvariant))?;
            if slot.is_none() {
                return Err(op.fail(CoreError::InternalInvariant));
            }
            op.copy(size_of::<Option<ChargedVec<T>>>() as u64)?;
            drop(slot.take());
            Ok(())
        })();
        self.latch(result)
    }

    fn latch<U>(&self, result: Result<U, CoreError>) -> Result<U, CoreError> {
        if let Err(error) = result
            && self.fault.get().is_none()
        {
            self.fault.set(Some(error));
        }
        result
    }

    fn grow_owners(&mut self, op: &mut Operation) -> Result<(), CoreError> {
        if self.owners.values.len() < self.owners.values.capacity() {
            return Ok(());
        }
        let max_pages = self.max_len.div_ceil(self.width);
        let capacity = self
            .owners
            .values
            .capacity()
            .checked_mul(2)
            .ok_or_else(|| op.fail(CoreError::ArithmeticOverflow))?
            .max(1)
            .min(max_pages);
        let bytes = capacity
            .checked_mul(size_of::<Option<ChargedVec<T>>>())
            .ok_or_else(|| op.fail(CoreError::ArithmeticOverflow))?;
        let mut lease = op
            .budget()
            .reserve_auxiliary(bytes as u64)
            .map_err(|error| op.fail(error))?;
        let mut values = Vec::new();
        op.checkpoint_now()?;
        values
            .try_reserve_exact(capacity)
            .map_err(|_| op.fail(CoreError::OutOfMemory))?;
        op.checkpoint_now()?;
        let actual = values
            .capacity()
            .checked_mul(size_of::<Option<ChargedVec<T>>>())
            .ok_or_else(|| op.fail(CoreError::ArithmeticOverflow))?;
        lease
            .admit_actual(actual as u64)
            .map_err(|error| op.fail(error))?;
        let mut replacement = Owners {
            values,
            _lease: Some(lease),
        };
        // Both descriptor arrays are admitted throughout the bounded moves.
        // Partial failure drops the moved owners and poisons this entire draft.
        for owner in &mut self.owners.values {
            op.events(1)?;
            op.copy(size_of::<Option<ChargedVec<T>>>() as u64)?;
            replacement.values.push(owner.take());
        }
        op.checkpoint_now()?;
        drop(std::mem::replace(&mut self.owners, replacement));
        op.checkpoint_now()?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::policy::{CancellationToken, SourceLimits};

    #[test]
    fn deadline_after_descriptor_allocation_releases_new_and_retained_pages() {
        let budget = SourceBudget::task_fixture();
        let mut preparation = budget.begin(CancellationToken::new());
        preparation.set_clock_samples_for_test(&[100_000_000]);
        let mut rows =
            PagedRows::<[u8; 16384]>::new(3, LimitKind::IndexRecords, &mut preparation).unwrap();
        rows.push([7; 16384], &mut preparation).unwrap();
        let retained = budget.usage().auxiliary_capacity;

        let mut op = budget.begin(CancellationToken::new());
        op.set_clock_samples_for_test(&[100_000_000, 500_000_000]);
        let deadline = CoreError::DeadlineExceeded {
            elapsed_ns: 500_000_000,
            limit_ns: 500_000_000,
        };
        assert_eq!(rows.push([8; 16384], &mut op), Err(deadline));
        assert_eq!(rows.len(), 1);
        assert_eq!(budget.usage().auxiliary_capacity, retained);
        assert!(budget.usage().auxiliary_capacity_peak > retained);
        let mut later = budget.begin(CancellationToken::new());
        later.set_clock_samples_for_test(&[100_000_000]);
        assert_eq!(rows.get(0, &mut later), Err(deadline));
        drop(rows);
        assert_eq!(budget.usage().auxiliary_capacity, 0);
    }

    #[test]
    fn pages_keep_indexes_after_retirement_and_clip_the_last_allocation() {
        let budget = SourceBudget::model_fixture();
        let mut op = budget.begin(CancellationToken::new());
        let mut rows = PagedRows::<u64>::new(1025, LimitKind::IndexRecords, &mut op).unwrap();
        assert_eq!(rows.page_width(), 1024);
        for i in 0..1025 {
            assert_eq!(rows.push(i, &mut op).unwrap(), i as usize);
        }
        assert_eq!(rows.page_count(), 2);
        rows.replace(1024, 9999, &mut op).unwrap();
        let before = budget.usage().auxiliary_capacity;
        rows.retire_page(0, &mut op).unwrap();
        assert_eq!(before - budget.usage().auxiliary_capacity, 8192);
        assert_eq!(rows.len(), 1025);
        assert_eq!(rows.get(1024, &mut op).unwrap(), 9999);
        assert_eq!(rows.get(0, &mut op), Err(CoreError::InternalInvariant));
        let mut later = budget.begin(CancellationToken::new());
        assert_eq!(
            rows.get(1024, &mut later),
            Err(CoreError::InternalInvariant)
        );
        drop(rows);
        assert_eq!(budget.usage().auxiliary_capacity, 0);
    }

    #[test]
    fn page_owner_growth_fault_latches_and_frees_every_moved_owner() {
        let budget = SourceBudget::new(SourceLimits {
            max_work: 20000,
            ..SourceLimits::default()
        })
        .unwrap();
        let mut op = budget.begin(CancellationToken::new());
        let mut rows = PagedRows::<[u8; 16384]>::new(3, LimitKind::IndexRecords, &mut op).unwrap();
        rows.push([0; 16384], &mut op).unwrap();
        let error = rows.push([1; 16384], &mut op).unwrap_err();
        assert!(matches!(
            error,
            CoreError::ResourceLimit {
                kind: LimitKind::Work,
                ..
            }
        ));
        assert_eq!(op.first_fault(), Some(error));
        let mut next = budget.begin(CancellationToken::new());
        assert_eq!(rows.get(0, &mut next), Err(error));
        let cancel = CancellationToken::new();
        cancel.cancel();
        let mut cancelled = budget.begin(cancel);
        assert_eq!(rows.get(0, &mut cancelled), Err(error));
        drop(rows);
        assert_eq!(budget.usage().auxiliary_capacity, 0);
    }

    #[test]
    fn oversized_rows_zero_rows_and_foreign_operations_are_rejected() {
        let budget = SourceBudget::model_fixture();
        let mut op = budget.begin(CancellationToken::new());
        assert!(matches!(
            PagedRows::<()>::new(1, LimitKind::IndexRecords, &mut op),
            Err(CoreError::InternalInvariant)
        ));
        let mut op = budget.begin(CancellationToken::new());
        assert!(matches!(
            PagedRows::<[u8; 65537]>::new(1, LimitKind::IndexRecords, &mut op),
            Err(CoreError::InternalInvariant)
        ));
        let mut op = budget.begin(CancellationToken::new());
        let mut rows = PagedRows::<[u8; 20000]>::new(1, LimitKind::IndexRecords, &mut op).unwrap();
        assert_eq!(rows.page_width(), 1);
        rows.push([0; 20000], &mut op).unwrap();
        let foreign = SourceBudget::model_fixture();
        let mut foreign_op = foreign.begin(CancellationToken::new());
        assert_eq!(rows.get(0, &mut foreign_op), Err(CoreError::WrongBudget));
        drop(rows);
        assert_eq!(budget.usage().auxiliary_capacity, 0);
        assert_eq!(foreign.usage().auxiliary_capacity, 0);
    }

    #[test]
    fn view_policy_only_discounts_bounded_metadata_getters() {
        let budget = SourceBudget::model_fixture();
        let mut op = budget.begin(CancellationToken::new());
        let mut rows = PagedRows::<u64>::new(1, LimitKind::IndexRecords, &mut op).unwrap();
        let before = op.stats().expect("statistics enabled");
        rows.push(42, &mut op).unwrap();
        let written = op.stats().expect("statistics enabled");
        assert_eq!(written.metadata_actions(), before.metadata_actions());
        assert!(written.copied_bytes() - before.copied_bytes() >= 8);
        assert_eq!(rows.get(0, &mut op).unwrap(), 42);
        let read = op.stats().expect("statistics enabled");
        assert_eq!(read.charged_units() - written.charged_units(), 3);
        assert_eq!(read.total() - written.total(), 17);
        assert_eq!(read.metadata_actions() - written.metadata_actions(), 2);
        let mut large = PagedRows::<[u8; 513]>::new(1, LimitKind::IndexRecords, &mut op).unwrap();
        large.push([7; 513], &mut op).unwrap();
        let before = op.stats().expect("statistics enabled");
        assert_eq!(large.get(0, &mut op).unwrap(), [7; 513]);
        let after = op.stats().expect("statistics enabled");
        assert_eq!(after.charged_units() - before.charged_units(), 1027);
        assert_eq!(after.total() - before.total(), 1027);
        assert_eq!(after.metadata_actions(), before.metadata_actions());
        drop((rows, large));
        assert_eq!(budget.usage().auxiliary_capacity, 0);
    }

    #[test]
    fn failing_second_view_charge_keeps_successful_getter_prefix() {
        let budget = SourceBudget::model_fixture();
        let mut preparation = budget.begin(CancellationToken::new());
        let mut rows = PagedRows::<u64>::new(1, LimitKind::IndexRecords, &mut preparation).unwrap();
        rows.push(42, &mut preparation).unwrap();
        let mut op = budget.begin(CancellationToken::new());
        // Legal edge state after earlier same-operation events.
        op.events(1).unwrap();
        while op.stats().expect("statistics enabled").charged_units() < budget.limits().max_work - 2
        {
            let count = (budget.limits().max_work
                - 2
                - op.stats().expect("statistics enabled").charged_units())
            .min(crate::policy::CHECKPOINT_BYTES);
            op.scan(count).unwrap();
        }
        assert!(matches!(
            rows.get(0, &mut op),
            Err(CoreError::ResourceLimit {
                kind: LimitKind::Work,
                ..
            })
        ));
        let failed = op.stats().expect("statistics enabled");
        assert_eq!(failed.charged_units(), budget.limits().max_work);
        assert_eq!(failed.metadata_actions(), 1);
        assert_eq!(failed.metadata_scanned_bytes(), 8);
        assert_eq!(failed.metadata_copied_bytes(), 0);
        let mut later = budget.begin(CancellationToken::new());
        assert!(matches!(
            rows.get(0, &mut later),
            Err(CoreError::ResourceLimit {
                kind: LimitKind::Work,
                ..
            })
        ));
        assert_eq!(later.stats().expect("statistics enabled").total(), 0);
        drop(rows);
        assert_eq!(budget.usage().auxiliary_capacity, 0);
    }
}
