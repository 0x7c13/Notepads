// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE file in the project root for license information.
// ---------------------------------------------------------------------------------------------

//! Reservations occupy the exact pages that become the immutable model.

use super::storage::{FinalSlot, ModelRows};
use crate::error::{CoreError, LimitKind};
use crate::memory::BuildRows;
use crate::policy::OwnedWork;

pub(super) struct SlotRows<'id, T: Copy> {
    rows: BuildRows<'id, FinalSlot<T>>,
    pending: usize,
}
impl<'id, T: Copy> SlotRows<'id, T> {
    pub(super) fn new(
        max: u64,
        kind: LimitKind,
        work: &mut OwnedWork<'id, '_>,
    ) -> Result<Self, CoreError> {
        Ok(Self {
            rows: BuildRows::new(max, kind, work)?,
            pending: 0,
        })
    }
    pub(super) fn len(&self) -> usize {
        self.rows.len()
    }
    pub(super) fn reserve(&mut self, work: &mut OwnedWork<'id, '_>) -> Result<usize, CoreError> {
        let pending = self
            .pending
            .checked_add(1)
            .ok_or_else(|| work.fail(CoreError::ArithmeticOverflow))?;
        let index = self.rows.push(FinalSlot::Reserved, work)?;
        self.pending = pending;
        Ok(index)
    }
    pub(super) fn push(
        &mut self,
        row: T,
        work: &mut OwnedWork<'id, '_>,
    ) -> Result<usize, CoreError> {
        self.rows.push(FinalSlot::Written(row), work)
    }
    pub(super) fn is_written(
        &self,
        index: usize,
        work: &mut OwnedWork<'id, '_>,
    ) -> Result<bool, CoreError> {
        if index >= self.len() {
            return Err(work.fail(CoreError::InvalidProvenance));
        }
        Ok(matches!(self.rows.get(index, work)?, FinalSlot::Written(_)))
    }
    pub(super) fn write(
        &mut self,
        index: usize,
        row: T,
        work: &mut OwnedWork<'id, '_>,
    ) -> Result<(), CoreError> {
        if self.is_written(index, work)? {
            return Err(work.fail(CoreError::InvalidProvenance));
        }
        self.rows.replace(index, FinalSlot::Written(row), work)?;
        self.pending -= 1;
        Ok(())
    }
    pub(super) fn get(&self, index: usize, work: &mut OwnedWork<'id, '_>) -> Result<T, CoreError> {
        if index >= self.len() {
            return Err(work.fail(CoreError::InvalidProvenance));
        }
        match self.rows.get(index, work)? {
            FinalSlot::Written(value) => Ok(value),
            FinalSlot::Reserved => Err(work.fail(CoreError::InvalidProvenance)),
        }
    }
    #[cfg(test)]
    pub(super) fn raw_value_for_test(&self, index: usize) -> Option<T> {
        match self.rows.raw_value_for_test(index)? {
            FinalSlot::Written(value) => Some(value),
            FinalSlot::Reserved => None,
        }
    }
    /// Used only for the unpublished target permutation and its reference fixup.
    pub(super) fn replace_written(
        &mut self,
        index: usize,
        row: T,
        work: &mut OwnedWork<'id, '_>,
    ) -> Result<(), CoreError> {
        if !self.is_written(index, work)? {
            return Err(work.fail(CoreError::InvalidProvenance));
        }
        self.rows.replace(index, FinalSlot::Written(row), work)
    }
    pub(super) fn freeze(self, work: &mut OwnedWork<'id, '_>) -> Result<ModelRows<T>, CoreError> {
        work.events(1)?;
        if self.pending != 0 {
            return Err(work.fail(CoreError::InvalidProvenance));
        }
        // The only mutations are reserve/write/push/permutation; pending is an
        // exact count, and every reservation is physically carried in its page.
        Ok(ModelRows::paged(self.rows.freeze(work)?))
    }
}
