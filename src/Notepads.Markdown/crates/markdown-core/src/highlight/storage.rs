// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE file in the project root for license information.
// ---------------------------------------------------------------------------------------------

//! The existing fixed OwnedRows pattern, with bounded replacement growth. Buffers drop first.

use crate::error::CoreError;
use crate::policy::{BoundaryReservation, CHECKPOINT_BYTES, Operation};

pub(super) struct OwnedRows<T: Copy> {
    pub values: Vec<T>,
    _reservation: BoundaryReservation,
}

impl<T: Copy> OwnedRows<T> {
    pub fn new(capacity: usize, op: &mut Operation) -> Result<Self, CoreError> {
        let bytes = capacity
            .checked_mul(size_of::<T>())
            .ok_or_else(|| op.fail(CoreError::ArithmeticOverflow))?;
        let mut reservation = op.reserve_boundary(bytes as u64)?;
        let mut values = Vec::new();
        values
            .try_reserve_exact(capacity)
            .map_err(|_| op.fail(CoreError::OutOfMemory))?;
        let actual = values
            .capacity()
            .checked_mul(size_of::<T>())
            .ok_or_else(|| op.fail(CoreError::ArithmeticOverflow))?;
        reservation.admit_actual(actual as u64, op)?;
        Ok(Self {
            values,
            _reservation: reservation,
        })
    }

    pub fn push(&mut self, value: T, op: &mut Operation) -> Result<(), CoreError> {
        if self.values.len() == self.values.capacity() {
            return Err(op.fail(CoreError::InternalInvariant));
        }
        op.external_write(size_of::<T>() as u64)?;
        self.values.push(value);
        Ok(())
    }
}

pub(super) struct GrowingRows<T: Copy> {
    data: Option<OwnedRows<T>>,
    max: usize,
}

impl<T: Copy> GrowingRows<T> {
    pub fn new(max: usize) -> Self {
        Self { data: None, max }
    }
    pub fn values(&self) -> &[T] {
        self.data
            .as_ref()
            .map_or(&[], |data| data.values.as_slice())
    }
    pub fn len(&self) -> usize {
        self.values().len()
    }

    pub fn last_mut(&mut self) -> Option<&mut T> {
        self.data.as_mut()?.values.last_mut()
    }

    /// Rollback is a discard, not new allocation/work; it remains available after a terminal fault.
    pub fn truncate(&mut self, len: usize) {
        if let Some(data) = &mut self.data {
            data.values.truncate(len);
        }
    }

    /// False is the nonfatal functional span-count limit; capacity faults stay on the one op.
    pub fn push(&mut self, value: T, op: &mut Operation) -> Result<bool, CoreError> {
        if self.len() == self.max {
            return Ok(false);
        }
        if self
            .data
            .as_ref()
            .is_none_or(|data| data.values.len() == data.values.capacity())
        {
            let capacity = self
                .len()
                .checked_mul(2)
                .ok_or_else(|| op.fail(CoreError::ArithmeticOverflow))?
                .max(64)
                .min(self.max);
            let mut next = OwnedRows::new(capacity, op)?;
            for chunk in self
                .values()
                .chunks((CHECKPOINT_BYTES as usize / size_of::<T>()).max(1))
            {
                op.checkpoint_now()?;
                op.external_write(size_of_val(chunk) as u64)?;
                next.values.extend_from_slice(chunk);
            }
            op.checkpoint_now()?;
            // Old and new allocations/reservations overlap until this replace and actual drop.
            self.data = Some(next);
        }
        self.data
            .as_mut()
            .ok_or_else(|| op.fail(CoreError::InternalInvariant))?
            .push(value, op)?;
        Ok(true)
    }
}

pub(super) struct OwnedText {
    pub text: String,
    _reservation: BoundaryReservation,
}

impl OwnedText {
    pub fn new(capacity: usize, op: &mut Operation) -> Result<Self, CoreError> {
        let mut reservation = op.reserve_boundary(capacity as u64)?;
        let mut text = String::new();
        text.try_reserve_exact(capacity)
            .map_err(|_| op.fail(CoreError::OutOfMemory))?;
        reservation.admit_actual(text.capacity() as u64, op)?;
        Ok(Self {
            text,
            _reservation: reservation,
        })
    }

    pub fn append(&mut self, value: &str, op: &mut Operation) -> Result<(), CoreError> {
        if value.len() > self.text.capacity() - self.text.len() {
            return Err(op.fail(CoreError::InternalInvariant));
        }
        let mut remaining = value;
        while !remaining.is_empty() {
            op.checkpoint_now()?;
            let mut end = remaining.len().min(CHECKPOINT_BYTES as usize);
            while !remaining.is_char_boundary(end) {
                end -= 1;
            }
            let (chunk, rest) = remaining.split_at(end);
            op.external_write(chunk.len() as u64)?;
            self.text.push_str(chunk);
            remaining = rest;
        }
        Ok(())
    }
}
