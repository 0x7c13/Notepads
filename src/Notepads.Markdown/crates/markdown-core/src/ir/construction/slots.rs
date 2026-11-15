// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE file in the project root for license information.
// ---------------------------------------------------------------------------------------------

use super::*;
use crate::policy::Lease;

enum Slot<T> {
    Empty,
    Stored(T),
    Taken,
}
pub(crate) struct SlotOwner<'id, T> {
    values: Vec<Slot<T>>,
    _lease: Lease,
    key: BuilderKey,
    _id: ConstructionId<'id>,
}
impl<'id> SlotOwner<'id, ReservedAtom> {
    pub(super) fn new(
        key: BuilderKey,
        count: usize,
        op: &mut OwnedWork<'id, '_>,
    ) -> Result<Self, CoreError> {
        op.check()?;
        op.events(1)?;
        let bytes = count
            .checked_mul(size_of::<Slot<ReservedAtom>>())
            .ok_or_else(|| op.fail(CoreError::ArithmeticOverflow))?;
        let mut lease = op.reserve_auxiliary(bytes as u64).map_err(|e| op.fail(e))?;
        let mut values = Vec::new();
        values
            .try_reserve_exact(count)
            .map_err(|_| op.fail(CoreError::OutOfMemory))?;
        let actual = values
            .capacity()
            .checked_mul(size_of::<Slot<ReservedAtom>>())
            .ok_or_else(|| op.fail(CoreError::ArithmeticOverflow))?;
        lease.admit_actual(actual as u64).map_err(|e| op.fail(e))?;
        op.check()?;
        for _ in 0..count {
            op.events(1)?;
            op.copy(size_of::<Slot<ReservedAtom>>() as u64)?;
            values.push(Slot::Empty);
        }
        Ok(Self {
            values,
            _lease: lease,
            key,
            _id: std::marker::PhantomData,
        })
    }
    fn ensure(&self, op: &mut OwnedWork<'id, '_>) -> Result<(), CoreError> {
        op.check()
    }
    pub(crate) fn store(
        &mut self,
        index: usize,
        ticket: ReservedAtom,
        op: &mut OwnedWork<'id, '_>,
    ) -> Result<(), CoreError> {
        let result = (|| {
            self.ensure(op)?;
            op.events(1)?;
            if ticket.reference.key != self.key || ticket.reference.value.index() != index {
                return Err(op.fail(CoreError::InvalidProvenance));
            }
            let slot = self
                .values
                .get_mut(index)
                .ok_or_else(|| op.fail(CoreError::InvalidProvenance))?;
            if !matches!(slot, Slot::Empty) {
                return Err(op.fail(CoreError::InvalidProvenance));
            }
            op.copy(size_of::<Slot<ReservedAtom>>() as u64)?;
            *slot = Slot::Stored(ticket);
            Ok(())
        })();
        result.map_err(|error| op.fail(error))
    }
    pub(crate) fn take(
        &mut self,
        index: usize,
        op: &mut OwnedWork<'id, '_>,
    ) -> Result<ReservedAtom, CoreError> {
        let result = (|| {
            self.ensure(op)?;
            op.events(1)?;
            let slot = self
                .values
                .get_mut(index)
                .ok_or_else(|| op.fail(CoreError::InvalidProvenance))?;
            if !matches!(slot, Slot::Stored(_)) {
                return Err(op.fail(CoreError::InvalidProvenance));
            }
            op.copy(size_of::<Slot<ReservedAtom>>() as u64)?;
            match std::mem::replace(slot, Slot::Taken) {
                Slot::Stored(ticket) => Ok(ticket),
                _ => Err(op.fail(CoreError::InternalInvariant)),
            }
        })();
        result.map_err(|error| op.fail(error))
    }
}
