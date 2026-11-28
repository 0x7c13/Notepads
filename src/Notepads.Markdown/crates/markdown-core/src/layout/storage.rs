// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE file in the project root for license information.
// ---------------------------------------------------------------------------------------------

use crate::error::CoreError;
use crate::policy::{BoundaryReservation, Operation};

/// Fixed capacity: construction admits both directories before any measurement.
pub(super) struct LayoutRows<T: Copy> {
    pub values: Vec<T>,
    // Declaration order frees actual storage before its original reservation.
    _reservation: BoundaryReservation,
}

impl<T: Copy> LayoutRows<T> {
    pub fn new(capacity: usize, op: &mut Operation) -> Result<Self, CoreError> {
        let bytes = capacity
            .checked_mul(size_of::<T>())
            .ok_or_else(|| op.fail(CoreError::ArithmeticOverflow))?;
        let mut reservation = op.reserve_boundary(bytes as u64)?;
        // Locals also unwind in the required order if allocation/admission fails.
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
        op.copy(size_of::<T>() as u64)?;
        self.values.push(value);
        Ok(())
    }
}
