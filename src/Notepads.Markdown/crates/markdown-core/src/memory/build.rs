// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE file in the project root for license information.
// ---------------------------------------------------------------------------------------------

//! Actual construction allocations, bound to the one original work capability.

use crate::error::{CoreError, LimitKind};
use crate::policy::{CHECKPOINT_BYTES, ConstructionId, Lease, OwnedWork};
use std::marker::PhantomData;

/// Frozen allocations have no mutable or raw-owner export. Buffers drop first.
pub(crate) struct FrozenRows<T> {
    pub(super) values: Vec<T>,
    pub(super) _lease: Option<Lease>,
}
impl<T> FrozenRows<T> {
    pub(crate) fn len(&self) -> usize {
        self.values.len()
    }
    pub(crate) fn as_slice(&self) -> &[T] {
        &self.values
    }
}

pub(crate) struct FrozenText {
    pub(super) text: String,
    pub(super) _lease: Option<Lease>,
}
impl FrozenText {
    pub(crate) fn as_str(&self) -> &str {
        &self.text
    }
}

pub(crate) struct BuildString<'id> {
    pub(in crate::memory) text: String,
    lease: Lease,
    id: ConstructionId<'id>,
}
impl<'id> BuildString<'id> {
    pub(crate) fn auxiliary(
        capacity: usize,
        work: &mut OwnedWork<'id, '_>,
    ) -> Result<Self, CoreError> {
        work.events(1)?;
        let mut lease = work.reserve_auxiliary(capacity as u64)?;
        let mut text = String::new();
        if capacity != 0 {
            work.checkpoint_now()?;
        }
        text.try_reserve_exact(capacity)
            .map_err(|_| work.fail(CoreError::OutOfMemory))?;
        if capacity != 0 {
            work.checkpoint_now()?;
        }
        lease
            .admit_actual(text.capacity() as u64)
            .map_err(|error| work.fail(error))?;
        work.check()?;
        Ok(Self {
            text,
            lease,
            id: PhantomData,
        })
    }
    pub(in crate::memory) fn as_str(&self) -> &str {
        &self.text
    }
    pub(crate) fn len(&self) -> usize {
        self.text.len()
    }
    pub(crate) fn append(
        &mut self,
        text: &str,
        work: &mut OwnedWork<'id, '_>,
    ) -> Result<(), CoreError> {
        work.copy(text.len() as u64)?;
        if text.len() > self.text.capacity() - self.text.len() {
            return Err(work.fail(CoreError::InternalInvariant));
        }
        self.text.push_str(text);
        Ok(())
    }
    pub(crate) fn freeze(self, work: &mut OwnedWork<'id, '_>) -> Result<FrozenText, CoreError> {
        work.check()?;
        Ok(FrozenText {
            text: self.text,
            _lease: Some(self.lease),
        })
    }
}

pub(crate) struct BuildVec<'id, T> {
    pub(super) values: Vec<T>,
    lease: Lease,
    id: ConstructionId<'id>,
}
impl<'id, T> BuildVec<'id, T> {
    pub(crate) fn new(capacity: usize, work: &mut OwnedWork<'id, '_>) -> Result<Self, CoreError> {
        work.events(1)?;
        let size = size_of::<T>();
        if size == 0 {
            return Err(work.fail(CoreError::InternalInvariant));
        }
        let bytes = capacity
            .checked_mul(size)
            .ok_or_else(|| work.fail(CoreError::ArithmeticOverflow))?;
        let mut lease = work.reserve_auxiliary(bytes as u64)?;
        let mut values = Vec::new();
        if capacity != 0 {
            work.checkpoint_now()?;
        }
        values
            .try_reserve_exact(capacity)
            .map_err(|_| work.fail(CoreError::OutOfMemory))?;
        if capacity != 0 {
            work.checkpoint_now()?;
        }
        let actual = values
            .capacity()
            .checked_mul(size)
            .ok_or_else(|| work.fail(CoreError::ArithmeticOverflow))?;
        lease
            .admit_actual(actual as u64)
            .map_err(|error| work.fail(error))?;
        work.check()?;
        Ok(Self {
            values,
            lease,
            id: PhantomData,
        })
    }
    pub(crate) fn push(
        &mut self,
        value: T,
        work: &mut OwnedWork<'id, '_>,
    ) -> Result<(), CoreError> {
        work.events(1)?;
        if self.values.len() == self.values.capacity() {
            return Err(work.fail(CoreError::InternalInvariant));
        }
        self.values.push(value);
        Ok(())
    }
    pub(in crate::memory) fn as_slice(&self) -> &[T] {
        &self.values
    }
    pub(crate) fn len(&self) -> usize {
        self.values.len()
    }
    pub(crate) fn freeze(self, work: &mut OwnedWork<'id, '_>) -> Result<FrozenRows<T>, CoreError> {
        work.check()?;
        Ok(FrozenRows {
            values: self.values,
            _lease: Some(self.lease),
        })
    }
}

pub(crate) struct BuildGrowingVec<'id, T: Copy> {
    pub(super) values: Vec<T>,
    lease: Option<Lease>,
    max_len: usize,
    kind: LimitKind,
    id: ConstructionId<'id>,
}
impl<'id, T: Copy> BuildGrowingVec<'id, T> {
    #[cfg(test)]
    pub(crate) fn storage_identity_for_test(&self) -> (usize, usize) {
        (self.values.as_ptr() as usize, self.len())
    }
    #[cfg(test)]
    pub(crate) fn raw_value_for_test(&self, index: usize) -> Option<T> {
        self.values.get(index).copied()
    }
    pub(crate) fn new(
        max_len: u64,
        kind: LimitKind,
        work: &mut OwnedWork<'id, '_>,
    ) -> Result<Self, CoreError> {
        work.check()?;
        let size = size_of::<T>();
        if size == 0 || size as u64 > CHECKPOINT_BYTES {
            return Err(work.fail(CoreError::InternalInvariant));
        }
        let max_len =
            usize::try_from(max_len).map_err(|_| work.fail(CoreError::ArithmeticOverflow))?;
        max_len
            .checked_mul(size)
            .ok_or_else(|| work.fail(CoreError::ArithmeticOverflow))?;
        Ok(Self {
            values: Vec::new(),
            lease: None,
            max_len,
            kind,
            id: PhantomData,
        })
    }
    pub(crate) fn len(&self) -> usize {
        self.values.len()
    }
    pub(in crate::memory) fn as_slice(&self) -> &[T] {
        &self.values
    }
    pub(crate) fn push(
        &mut self,
        value: T,
        work: &mut OwnedWork<'id, '_>,
    ) -> Result<(), CoreError> {
        self.extend_from_slice(std::slice::from_ref(&value), work)
    }
    pub(crate) fn replace(
        &mut self,
        index: usize,
        value: T,
        work: &mut OwnedWork<'id, '_>,
    ) -> Result<(), CoreError> {
        work.events(1)?;
        work.copy(size_of::<T>() as u64)?;
        let slot = self
            .values
            .get_mut(index)
            .ok_or_else(|| work.fail(CoreError::InternalInvariant))?;
        *slot = value;
        Ok(())
    }
    pub(crate) fn extend_from_slice(
        &mut self,
        values: &[T],
        work: &mut OwnedWork<'id, '_>,
    ) -> Result<(), CoreError> {
        work.check()?;
        let required = self
            .values
            .len()
            .checked_add(values.len())
            .ok_or_else(|| work.fail(CoreError::ArithmeticOverflow))?;
        work.check_limit(self.kind, required as u64, self.max_len as u64)?;
        if required > self.values.capacity() {
            self.grow(required, work)?;
        }
        for chunk in values.chunks(CHECKPOINT_BYTES as usize / size_of::<T>()) {
            work.events(1)?;
            work.copy(size_of_val(chunk) as u64)?;
            self.values.extend_from_slice(chunk);
        }
        Ok(())
    }
    pub(crate) fn extend_from_rows(
        &mut self,
        source: &BuildGrowingVec<'id, T>,
        start: usize,
        end: usize,
        work: &mut OwnedWork<'id, '_>,
    ) -> Result<(), CoreError> {
        work.check()?;
        let values = source
            .values
            .get(start..end)
            .ok_or_else(|| work.fail(CoreError::InvalidRange))?;
        self.extend_from_slice(values, work)
    }
    fn grow(&mut self, required: usize, work: &mut OwnedWork<'id, '_>) -> Result<(), CoreError> {
        work.events(1)?;
        let capacity = self
            .values
            .capacity()
            .checked_mul(2)
            .ok_or_else(|| work.fail(CoreError::ArithmeticOverflow))?
            .max(1)
            .max(required)
            .min(self.max_len);
        let bytes = capacity
            .checked_mul(size_of::<T>())
            .ok_or_else(|| work.fail(CoreError::ArithmeticOverflow))?;
        let mut lease = work.reserve_auxiliary(bytes as u64)?;
        // New allocation and old allocation retain distinct leases throughout copying.
        let mut values = Vec::new();
        work.checkpoint_now()?;
        values
            .try_reserve_exact(capacity)
            .map_err(|_| work.fail(CoreError::OutOfMemory))?;
        work.checkpoint_now()?;
        let actual = values
            .capacity()
            .checked_mul(size_of::<T>())
            .ok_or_else(|| work.fail(CoreError::ArithmeticOverflow))?;
        lease
            .admit_actual(actual as u64)
            .map_err(|error| work.fail(error))?;
        for chunk in self
            .values
            .chunks(CHECKPOINT_BYTES as usize / size_of::<T>())
        {
            work.events(1)?;
            work.copy(size_of_val(chunk) as u64)?;
            values.extend_from_slice(chunk);
        }
        work.checkpoint_now()?;
        let replacement = Self {
            values,
            lease: Some(lease),
            max_len: self.max_len,
            kind: self.kind,
            id: PhantomData,
        };
        drop(std::mem::replace(self, replacement));
        work.checkpoint_now()
    }
    pub(crate) fn freeze(self, work: &mut OwnedWork<'id, '_>) -> Result<FrozenRows<T>, CoreError> {
        work.check()?;
        Ok(FrozenRows {
            values: self.values,
            _lease: self.lease,
        })
    }
}

#[cfg(test)]
pub(crate) struct BuildGrowingText<'id> {
    pub(in crate::memory) text: BuildString<'id>,
    pub(in crate::memory) max_len: usize,
}
#[cfg(test)]
impl<'id> BuildGrowingText<'id> {
    pub(crate) fn new(max_len: u64, work: &mut OwnedWork<'id, '_>) -> Result<Self, CoreError> {
        Ok(Self {
            text: BuildString::auxiliary(0, work)?,
            max_len: usize::try_from(max_len)
                .map_err(|_| work.fail(CoreError::ArithmeticOverflow))?,
        })
    }
    pub(crate) fn len(&self) -> usize {
        self.text.as_str().len()
    }
    pub(in crate::memory) fn as_str(&self) -> &str {
        self.text.as_str()
    }
    pub(crate) fn append(
        &mut self,
        value: &str,
        work: &mut OwnedWork<'id, '_>,
    ) -> Result<(), CoreError> {
        work.check()?;
        let required = self
            .len()
            .checked_add(value.len())
            .ok_or_else(|| work.fail(CoreError::ArithmeticOverflow))?;
        self.ensure_capacity(required, work)?;
        Self::append_chunks(&mut self.text, value, work)
    }
    pub(crate) fn ensure_capacity(
        &mut self,
        required: usize,
        work: &mut OwnedWork<'id, '_>,
    ) -> Result<(), CoreError> {
        work.check()?;
        work.check_limit(
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
                .ok_or_else(|| work.fail(CoreError::ArithmeticOverflow))?
                .max(1)
                .max(required)
                .min(self.max_len);
            let mut replacement = BuildString::auxiliary(capacity, work)?;
            Self::append_chunks(&mut replacement, self.text.as_str(), work)?;
            work.checkpoint_now()?;
            drop(std::mem::replace(&mut self.text, replacement));
            work.checkpoint_now()?;
        }
        Ok(())
    }
    fn append_chunks(
        target: &mut BuildString<'id>,
        text: &str,
        work: &mut OwnedWork<'id, '_>,
    ) -> Result<(), CoreError> {
        let mut at = 0;
        while at < text.len() {
            work.events(1)?;
            let mut end = text.len().min(at.saturating_add(CHECKPOINT_BYTES as usize));
            while !text.is_char_boundary(end) {
                work.scan(1)?;
                end -= 1;
            }
            target.append(&text[at..end], work)?;
            at = end;
        }
        Ok(())
    }
}

pub(in crate::memory) fn read_value<T: Copy>(
    values: &[T],
    index: usize,
    work: &mut OwnedWork<'_, '_>,
) -> Result<Option<T>, CoreError> {
    work.check()?;
    let bytes = size_of::<T>() as u64;
    if bytes <= crate::policy::MAX_METADATA_VIEW_BYTES {
        work.read_metadata(bytes, || Ok(values.get(index).copied()))
    } else {
        work.scan(bytes)?;
        work.copy(bytes)?;
        Ok(values.get(index).copied())
    }
}
