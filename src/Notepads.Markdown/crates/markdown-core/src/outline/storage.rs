// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE file in the project root for license information.
// ---------------------------------------------------------------------------------------------

use crate::error::CoreError;
use crate::policy::{BoundaryReservation, CHECKPOINT_BYTES, CHECKPOINT_EVENTS, Operation};

/// The allocation retires before its request-capacity reservation.
pub(super) struct PaidVec<T> {
    pub values: Vec<T>,
    _reservation: BoundaryReservation,
}

impl<T> PaidVec<T> {
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
        op.events(1)?;
        if self.values.len() == self.values.capacity() {
            return Err(op.fail(CoreError::InternalInvariant));
        }
        copy(size_of::<T>() as u64, op)?;
        self.values.push(value);
        Ok(())
    }
}

impl PaidVec<u8> {
    pub fn append(&mut self, value: &[u8], op: &mut Operation) -> Result<(), CoreError> {
        if value.len() > self.values.capacity() - self.values.len() {
            return Err(op.fail(CoreError::InternalInvariant));
        }
        for chunk in value.chunks(CHECKPOINT_BYTES as usize) {
            op.scan(chunk.len() as u64)?;
            op.copy(chunk.len() as u64)?;
            self.values.extend_from_slice(chunk);
        }
        Ok(())
    }
}

pub(super) fn scan(mut bytes: u64, op: &mut Operation) -> Result<(), CoreError> {
    while bytes != 0 {
        let count = bytes.min(CHECKPOINT_BYTES);
        op.scan(count)?;
        bytes -= count;
    }
    Ok(())
}

pub(super) fn copy(mut bytes: u64, op: &mut Operation) -> Result<(), CoreError> {
    while bytes != 0 {
        let count = bytes.min(CHECKPOINT_BYTES);
        op.copy(count)?;
        bytes -= count;
    }
    Ok(())
}

pub(super) fn events(mut count: u64, op: &mut Operation) -> Result<(), CoreError> {
    while count != 0 {
        let part = count.min(CHECKPOINT_EVENTS);
        op.events(part)?;
        count -= part;
    }
    Ok(())
}
