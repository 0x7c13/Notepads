// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE file in the project root for license information.
// ---------------------------------------------------------------------------------------------

//! Exact construction allocations; only consumption yields immutable storage.

use crate::error::CoreError;
#[cfg(test)]
use crate::memory::{BuildString, FrozenText};
use crate::memory::{BuildVec, FrozenRows};
use crate::policy::{CHECKPOINT_BYTES, ConstructionId, OwnedWork};
use std::marker::PhantomData;

pub(crate) struct BuildExactRows<'id, T: Copy> {
    pub(in crate::memory) data: Option<BuildVec<'id, T>>,
    pub(in crate::memory) count: usize,
    id: ConstructionId<'id>,
}
impl<'id, T: Copy> BuildExactRows<'id, T> {
    pub(crate) fn new(count: usize, work: &mut OwnedWork<'id, '_>) -> Result<Self, CoreError> {
        work.events(1)?;
        if size_of::<T>() == 0 || size_of::<T>() as u64 > CHECKPOINT_BYTES {
            return Err(work.fail(CoreError::InternalInvariant));
        }
        count
            .checked_mul(size_of::<T>())
            .ok_or_else(|| work.fail(CoreError::ArithmeticOverflow))?;
        Ok(Self {
            data: None,
            count,
            id: PhantomData,
        })
    }
    pub(crate) fn len(&self) -> usize {
        self.data.as_ref().map_or(0, BuildVec::len)
    }
    #[cfg(test)]
    pub(crate) fn storage_identity_for_test(&self) -> (usize, usize) {
        (self.as_slice().as_ptr() as usize, self.len())
    }
    #[cfg(test)]
    pub(crate) fn raw_value_for_test(&self, index: usize) -> Option<T> {
        self.as_slice().get(index).copied()
    }
    pub(in crate::memory) fn as_slice(&self) -> &[T] {
        self.data.as_ref().map_or(&[], BuildVec::as_slice)
    }
    pub(crate) fn get(
        &self,
        index: usize,
        work: &mut OwnedWork<'id, '_>,
    ) -> Result<Option<T>, CoreError> {
        crate::memory::build::read_value(self.as_slice(), index, work)
    }
    pub(crate) fn has_allocation(&self) -> bool {
        self.data.is_some()
    }
    pub(crate) fn push(&mut self, row: T, work: &mut OwnedWork<'id, '_>) -> Result<(), CoreError> {
        work.check()?;
        work.events(1)?;
        if self.len() >= self.count {
            return Err(work.fail(CoreError::InvalidProvenance));
        }
        if self.data.is_none() {
            self.data = Some(BuildVec::new(self.count, work)?);
        }
        work.copy(size_of::<T>() as u64)?;
        self.data
            .as_mut()
            .ok_or_else(|| work.fail(CoreError::InternalInvariant))?
            .push(row, work)
    }
    pub(crate) fn replace(
        &mut self,
        index: usize,
        row: T,
        work: &mut OwnedWork<'id, '_>,
    ) -> Result<(), CoreError> {
        work.check()?;
        work.events(1)?;
        let slot = self
            .data
            .as_mut()
            .and_then(|data| data.values.get_mut(index))
            .ok_or_else(|| work.fail(CoreError::InvalidProvenance))?;
        work.copy(size_of::<T>() as u64)?;
        *slot = row;
        Ok(())
    }
    pub(crate) fn complete(&self, work: &mut OwnedWork<'id, '_>) -> Result<(), CoreError> {
        work.check()?;
        work.events(1)?;
        if self.len() != self.count {
            return Err(work.fail(CoreError::InvalidProvenance));
        }
        Ok(())
    }
    pub(crate) fn freeze(self, work: &mut OwnedWork<'id, '_>) -> Result<FrozenRows<T>, CoreError> {
        self.complete(work)?;
        match self.data {
            Some(data) => data.freeze(work),
            None => Ok(FrozenRows {
                values: Vec::new(),
                _lease: None,
            }),
        }
    }
}

#[cfg(test)]
pub(crate) struct BuildExactText<'id> {
    pub(in crate::memory) data: Option<BuildString<'id>>,
    pub(in crate::memory) count: usize,
    id: ConstructionId<'id>,
}
#[cfg(test)]
impl<'id> BuildExactText<'id> {
    pub(crate) fn new(count: usize, work: &mut OwnedWork<'id, '_>) -> Result<Self, CoreError> {
        work.events(1)?;
        Ok(Self {
            data: None,
            count,
            id: PhantomData,
        })
    }
    pub(crate) fn len(&self) -> usize {
        self.as_str().len()
    }
    pub(in crate::memory) fn as_str(&self) -> &str {
        self.data.as_ref().map_or("", BuildString::as_str)
    }
    pub(crate) fn validate_range(
        &self,
        start: usize,
        len: usize,
        work: &mut OwnedWork<'id, '_>,
    ) -> Result<(), CoreError> {
        work.check()?;
        let end = start
            .checked_add(len)
            .ok_or_else(|| work.fail(CoreError::ArithmeticOverflow))?;
        // Fixed endpoint-only inspection; no borrowed construction text escapes.
        work.scan(2)?;
        if self.as_str().get(start..end).is_none() {
            return Err(work.fail(CoreError::InvalidBoundary));
        }
        Ok(())
    }
    pub(crate) fn append(
        &mut self,
        text: &str,
        work: &mut OwnedWork<'id, '_>,
    ) -> Result<(), CoreError> {
        work.check()?;
        let end = self
            .len()
            .checked_add(text.len())
            .ok_or_else(|| work.fail(CoreError::ArithmeticOverflow))?;
        if end > self.count {
            return Err(work.fail(CoreError::InvalidProvenance));
        }
        if self.data.is_none() && !text.is_empty() {
            self.data = Some(BuildString::auxiliary(self.count, work)?);
        }
        let mut start = 0;
        while start < text.len() {
            work.events(1)?;
            let mut end = text
                .len()
                .min(start.saturating_add(CHECKPOINT_BYTES as usize));
            while !text.is_char_boundary(end) {
                work.scan(1)?;
                end -= 1;
            }
            self.data
                .as_mut()
                .ok_or_else(|| work.fail(CoreError::InternalInvariant))?
                .append(&text[start..end], work)?;
            start = end;
        }
        Ok(())
    }
    pub(crate) fn complete(&self, work: &mut OwnedWork<'id, '_>) -> Result<(), CoreError> {
        work.check()?;
        work.events(1)?;
        if self.len() != self.count {
            return Err(work.fail(CoreError::InvalidProvenance));
        }
        Ok(())
    }
    pub(crate) fn freeze(self, work: &mut OwnedWork<'id, '_>) -> Result<FrozenText, CoreError> {
        self.complete(work)?;
        match self.data {
            Some(data) => data.freeze(work),
            None => Ok(FrozenText {
                text: String::new(),
                _lease: None,
            }),
        }
    }
}
