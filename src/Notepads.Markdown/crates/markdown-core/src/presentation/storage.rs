// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE file in the project root for license information.
// ---------------------------------------------------------------------------------------------

use crate::error::{CoreError, LimitKind};
use crate::policy::{BoundaryReservation, Operation};

pub(super) struct OwnedRows<T: Copy> {
    pub values: Vec<T>,
    _reservation: BoundaryReservation,
}

impl<T: Copy> OwnedRows<T> {
    fn new(capacity: usize, op: &mut Operation) -> Result<Self, CoreError> {
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
}

/// The counting pass holds no allocation. The writing pass has exact admitted capacity.
pub(super) struct Store<T: Copy> {
    pub len: usize,
    limit: u64,
    data: Option<OwnedRows<T>>,
}

impl<T: Copy> Store<T> {
    pub fn count(limit: u64) -> Self {
        Self {
            len: 0,
            limit,
            data: None,
        }
    }

    pub fn allocated(capacity: usize, limit: u64, op: &mut Operation) -> Result<Self, CoreError> {
        Ok(Self {
            len: 0,
            limit,
            data: Some(OwnedRows::new(capacity, op)?),
        })
    }

    pub fn push(&mut self, value: T, op: &mut Operation) -> Result<u32, CoreError> {
        let index = u32::try_from(self.len).map_err(|_| op.fail(CoreError::ArithmeticOverflow))?;
        let next = self
            .len
            .checked_add(1)
            .ok_or_else(|| op.fail(CoreError::ArithmeticOverflow))?;
        op.events(1)?;
        op.check_limit(LimitKind::ModelRecords, next as u64, self.limit)?;
        if let Some(data) = &mut self.data {
            if data.values.len() == data.values.capacity() {
                return Err(op.fail(CoreError::InternalInvariant));
            }
            op.copy(size_of::<T>() as u64)?;
            data.values.push(value);
        }
        self.len = next;
        Ok(index)
    }

    pub fn finish(self, op: &mut Operation) -> Result<OwnedRows<T>, CoreError> {
        self.data
            .ok_or_else(|| op.fail(CoreError::InternalInvariant))
    }
}

pub(super) struct Blob {
    pub len: usize,
    data: Option<OwnedRows<u8>>,
}

impl Blob {
    pub fn count() -> Self {
        Self { len: 0, data: None }
    }

    pub fn allocated(capacity: usize, op: &mut Operation) -> Result<Self, CoreError> {
        Ok(Self {
            len: 0,
            data: Some(OwnedRows::new(capacity, op)?),
        })
    }

    pub fn append(&mut self, value: &str, op: &mut Operation) -> Result<(), CoreError> {
        self.append_bytes(value.as_bytes(), op)
    }

    pub fn append_bytes(&mut self, value: &[u8], op: &mut Operation) -> Result<(), CoreError> {
        let next = self
            .len
            .checked_add(value.len())
            .ok_or_else(|| op.fail(CoreError::ArithmeticOverflow))?;
        u32::try_from(next).map_err(|_| op.fail(CoreError::ArithmeticOverflow))?;
        for chunk in value.chunks(65_536) {
            op.scan(chunk.len() as u64)?;
            if let Some(data) = &mut self.data {
                if chunk.len() > data.values.capacity() - data.values.len() {
                    return Err(op.fail(CoreError::InternalInvariant));
                }
                op.copy(chunk.len() as u64)?;
                data.values.extend_from_slice(chunk);
            }
        }
        self.len = next;
        Ok(())
    }

    pub fn finish(self, op: &mut Operation) -> Result<OwnedRows<u8>, CoreError> {
        self.data
            .ok_or_else(|| op.fail(CoreError::InternalInvariant))
    }
}
