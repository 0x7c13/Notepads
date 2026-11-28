// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE file in the project root for license information.
// ---------------------------------------------------------------------------------------------

//! Response storage admits old plus new capacity and sorting scratch together.

use crate::error::{CoreError, LimitKind};
use crate::policy::{CHECKPOINT_BYTES, Lease, Operation};
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

pub(super) struct QueryScope {
    live_capacity: AtomicU64,
    live_rows: AtomicU64,
    max_capacity: u64,
    max_rows: u64,
    _lease: Lease,
}

impl QueryScope {
    pub(super) fn new(op: &mut Operation) -> Result<Arc<Self>, CoreError> {
        op.events(1)?;
        let limits = op
            .budget()
            .model_limits()
            .copied()
            .ok_or_else(|| op.fail(CoreError::UnsupportedConfiguration))?;
        let bytes = (size_of::<Self>() + 2 * size_of::<usize>()) as u64;
        op.check_limit(LimitKind::QueryCapacity, bytes, limits.query_capacity)?;
        let lease = op
            .budget()
            .reserve_auxiliary(bytes)
            .map_err(|e| op.fail(e))?;
        Ok(Arc::new(Self {
            live_capacity: AtomicU64::new(bytes),
            live_rows: AtomicU64::new(0),
            max_capacity: limits.query_capacity,
            max_rows: limits.query_records,
            _lease: lease,
        }))
    }

    fn capacity(
        self: &Arc<Self>,
        amount: u64,
        op: &mut Operation,
    ) -> Result<QueryLease, CoreError> {
        let live = self.live_capacity.load(Ordering::Relaxed);
        let next = live
            .checked_add(amount)
            .ok_or_else(|| op.fail(CoreError::ArithmeticOverflow))?;
        op.check_limit(LimitKind::QueryCapacity, next, self.max_capacity)?;
        self.live_capacity.store(next, Ordering::Relaxed);
        Ok(QueryLease {
            scope: self.clone(),
            amount,
        })
    }

    fn row(&self, op: &mut Operation) -> Result<(), CoreError> {
        let next = self
            .live_rows
            .load(Ordering::Relaxed)
            .checked_add(1)
            .ok_or_else(|| op.fail(CoreError::ArithmeticOverflow))?;
        op.check_limit(LimitKind::QueryRecords, next, self.max_rows)?;
        self.live_rows.store(next, Ordering::Relaxed);
        Ok(())
    }
}

struct QueryLease {
    scope: Arc<QueryScope>,
    amount: u64,
}

impl Drop for QueryLease {
    fn drop(&mut self) {
        self.scope
            .live_capacity
            .fetch_sub(self.amount, Ordering::Relaxed);
    }
}

pub(super) struct QueryVec<T: Copy> {
    values: Vec<T>,
    auxiliary: Option<Lease>,
    capacity: Option<QueryLease>,
    scope: Arc<QueryScope>,
}

impl<T: Copy> QueryVec<T> {
    pub(super) fn new(scope: &Arc<QueryScope>) -> Self {
        Self {
            values: Vec::new(),
            auxiliary: None,
            capacity: None,
            scope: scope.clone(),
        }
    }
    pub(super) fn len(&self) -> usize {
        self.values.len()
    }
    pub(super) fn as_slice(&self) -> &[T] {
        &self.values
    }
    pub(super) fn scope(&self) -> &Arc<QueryScope> {
        &self.scope
    }
    pub(super) fn push(&mut self, value: T, op: &mut Operation) -> Result<(), CoreError> {
        op.events(1)?;
        let required = self
            .values
            .len()
            .checked_add(1)
            .ok_or_else(|| op.fail(CoreError::ArithmeticOverflow))?;
        if required > self.values.capacity() {
            self.grow(required, op)?;
        }
        op.copy(size_of::<T>() as u64)?;
        self.scope.row(op)?;
        self.values.push(value);
        Ok(())
    }
    pub(super) fn replace(
        &mut self,
        index: usize,
        value: T,
        op: &mut Operation,
    ) -> Result<(), CoreError> {
        op.events(1)?;
        op.copy(size_of::<T>() as u64)?;
        *self
            .values
            .get_mut(index)
            .ok_or_else(|| op.fail(CoreError::InternalInvariant))? = value;
        Ok(())
    }
    pub(super) fn truncate(&mut self, len: usize) {
        debug_assert!(len <= self.len());
        self.scope
            .live_rows
            .fetch_sub((self.len() - len) as u64, Ordering::Relaxed);
        self.values.truncate(len);
    }
    fn grow(&mut self, required: usize, op: &mut Operation) -> Result<(), CoreError> {
        let size = size_of::<T>();
        if size == 0 || size as u64 > CHECKPOINT_BYTES {
            return Err(op.fail(CoreError::InternalInvariant));
        }
        let max = usize::try_from(self.scope.max_rows)
            .map_err(|_| op.fail(CoreError::ArithmeticOverflow))?;
        let count = self
            .values
            .capacity()
            .checked_mul(2)
            .ok_or_else(|| op.fail(CoreError::ArithmeticOverflow))?
            .max(1)
            .max(required)
            .min(max);
        if count < required {
            return Err(op.fail(CoreError::ResourceLimit {
                kind: LimitKind::QueryRecords,
                requested: required as u64,
                limit: self.scope.max_rows,
            }));
        }
        let bytes = count
            .checked_mul(size)
            .ok_or_else(|| op.fail(CoreError::ArithmeticOverflow))? as u64;
        let mut query_lease = self.scope.capacity(bytes, op)?;
        let mut auxiliary = op
            .budget()
            .reserve_auxiliary(bytes)
            .map_err(|e| op.fail(e))?;
        let mut values = Vec::new();
        values
            .try_reserve_exact(count)
            .map_err(|_| op.fail(CoreError::OutOfMemory))?;
        let actual = values
            .capacity()
            .checked_mul(size)
            .ok_or_else(|| op.fail(CoreError::ArithmeticOverflow))? as u64;
        if actual > bytes {
            let mut extra = self.scope.capacity(actual - bytes, op)?;
            query_lease.amount = actual;
            extra.amount = 0;
        }
        auxiliary.admit_actual(actual).map_err(|e| op.fail(e))?;
        for chunk in self.values.chunks(CHECKPOINT_BYTES as usize / size) {
            op.events(1)?;
            op.copy(std::mem::size_of_val(chunk) as u64)?;
            values.extend_from_slice(chunk);
        }
        op.checkpoint()?;
        let old_values = std::mem::replace(&mut self.values, values);
        let old_auxiliary = self.auxiliary.replace(auxiliary);
        let old_capacity = self.capacity.replace(query_lease);
        drop(old_values);
        drop(old_auxiliary);
        drop(old_capacity);
        Ok(())
    }
}

impl<T: Copy> Drop for QueryVec<T> {
    fn drop(&mut self) {
        self.scope
            .live_rows
            .fetch_sub(self.len() as u64, Ordering::Relaxed);
    }
}

/// Stable bottom-up merging: each comparison and move is cancellable and metered.
pub(super) fn sort<T: Copy>(
    rows: &mut QueryVec<T>,
    start: usize,
    compare: impl Fn(T, T) -> std::cmp::Ordering,
    op: &mut Operation,
) -> Result<(), CoreError> {
    let len = rows
        .len()
        .checked_sub(start)
        .ok_or_else(|| op.fail(CoreError::InternalInvariant))?;
    if len < 2 {
        return Ok(());
    }
    let mut ordered = true;
    for index in start + 1..rows.len() {
        op.events(1)?;
        if compare(rows.as_slice()[index - 1], rows.as_slice()[index])
            == std::cmp::Ordering::Greater
        {
            ordered = false;
            break;
        }
    }
    if ordered {
        return Ok(());
    }
    let mut scratch = QueryVec::new(&rows.scope);
    for index in start..rows.len() {
        scratch.push(rows.as_slice()[index], op)?;
    }
    let mut width = 1usize;
    while width < len {
        let mut base = 0usize;
        while base < len {
            let middle = base.saturating_add(width).min(len);
            let end = middle.saturating_add(width).min(len);
            let (mut left, mut right) = (base, middle);
            for out in base..end {
                op.events(1)?;
                let use_left = left < middle
                    && (right == end
                        || compare(
                            rows.as_slice()[start + left],
                            rows.as_slice()[start + right],
                        ) != std::cmp::Ordering::Greater);
                let index = if use_left {
                    let i = left;
                    left += 1;
                    i
                } else {
                    let i = right;
                    right += 1;
                    i
                };
                scratch.replace(out, rows.as_slice()[start + index], op)?;
            }
            base = end;
        }
        for i in 0..len {
            rows.replace(start + i, scratch.as_slice()[i], op)?;
        }
        width = width
            .checked_mul(2)
            .ok_or_else(|| op.fail(CoreError::ArithmeticOverflow))?;
    }
    Ok(())
}
