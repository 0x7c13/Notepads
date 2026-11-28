// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE file in the project root for license information.
// ---------------------------------------------------------------------------------------------

//! Exclusive, uninterrupted construction of one actual contiguous atom block.

use super::*;

#[cfg(test)]
#[path = "target_task_tests.rs"]
mod task_tests;
#[cfg(test)]
#[path = "target_tests.rs"]
mod tests;

/// A completed block can only be produced by a writer that consumed every ticket.
#[derive(Debug, Eq, PartialEq)]
pub(crate) struct WrittenAtomBlock {
    reference: DraftRef<Rows<AtomRow>>,
}

#[derive(Debug, Eq, PartialEq)]
pub(crate) enum DraftAtomBlock {
    Written(WrittenAtomBlock),
    #[cfg(test)]
    Reserved(DraftRef<Rows<AtomRow>>),
}

impl From<WrittenAtomBlock> for DraftAtomBlock {
    fn from(value: WrittenAtomBlock) -> Self {
        Self::Written(value)
    }
}

#[cfg(test)]
impl From<DraftRef<Rows<AtomRow>>> for DraftAtomBlock {
    fn from(value: DraftRef<Rows<AtomRow>>) -> Self {
        Self::Reserved(value)
    }
}

impl Lower for DraftAtomBlock {
    type Value = Rows<AtomRow>;

    fn lower<'id>(
        self,
        builder: &ExactBuilder<'id>,
        op: &mut OwnedWork<'id, '_>,
    ) -> Result<Self::Value, CoreError> {
        match self {
            Self::Written(block) => {
                let rows = builder.strip(block.reference, op)?;
                // The non-copy witness owns the completed chain; no arbitrary
                // numeric range can reach this branch, including an empty range.
                rows.indices(builder.ir.packets.len())
                    .map_err(|error| op.fail(error))?;
                Ok(rows)
            }
            #[cfg(test)]
            Self::Reserved(reference) => builder.atom_block_ref(reference, op),
        }
    }
}

pub(crate) struct TargetWriter<'builder, 'operation, 'id, 'op> {
    builder: &'builder mut ExactBuilder<'id>,
    op: &'operation mut OwnedWork<'id, 'op>,
    start: usize,
    expected: usize,
    committed: usize,
    pending: Option<AtomId>,
    closed: bool,
}

impl<'id> ExactBuilder<'id> {
    pub(crate) fn begin_target<'builder, 'operation, 'op>(
        &'builder mut self,
        expected_len: usize,
        op: &'operation mut OwnedWork<'id, 'op>,
    ) -> Result<TargetWriter<'builder, 'operation, 'id, 'op>, CoreError> {
        self.ensure(op)?;
        let start = self.ir.packets.len();
        let result = (|| {
            op.events(1)?;
            let end = start
                .checked_add(expected_len)
                .ok_or_else(|| op.fail(CoreError::ArithmeticOverflow))?;
            if end > self.counts.atoms {
                return Err(op.fail(CoreError::InvalidProvenance));
            }
            Ok(())
        })();
        result.map_err(|error| op.fail(error))?;
        Ok(TargetWriter {
            builder: self,
            op,
            start,
            expected: expected_len,
            committed: 0,
            pending: None,
            closed: false,
        })
    }
}

impl<'id, 'op> TargetWriter<'_, '_, 'id, 'op> {
    fn transaction<T>(
        &mut self,
        action: impl FnOnce(&mut ExactBuilder<'id>, &mut OwnedWork<'id, '_>) -> Result<T, CoreError>,
    ) -> Result<T, CoreError> {
        self.builder.transaction(self.op, action)
    }

    fn reject<T>(&mut self) -> Result<T, CoreError> {
        let result = Err(self.op.fail(CoreError::InvalidProvenance));
        result.map_err(|error| self.op.fail(error))
    }

    pub(crate) fn reserve_next(&mut self) -> Result<ReservedAtom, CoreError> {
        self.builder.ensure(self.op)?;
        if self.pending.is_some() || self.committed >= self.expected {
            return self.reject();
        }
        let index = self.start + self.committed;
        let ticket = self.builder.reserve_atom(index, self.op)?;
        self.pending = Some(ticket.reference.value);
        Ok(ticket)
    }

    /// Only immutable construction access is exposed; the same OwnedWork stays
    /// exclusively borrowed until close/drop. Returned handles keep their scope.
    pub(crate) fn with_read<T>(
        &mut self,
        action: impl FnOnce(&ExactBuilder<'id>, &mut OwnedWork<'id, '_>) -> Result<T, CoreError>,
    ) -> Result<T, CoreError> {
        self.transaction(|builder, op| {
            let value = action(builder, op).map_err(|error| op.fail(error))?;
            op.check()?;
            Ok(value)
        })
    }

    fn commit(
        &mut self,
        ticket: ReservedAtom,
        lower: impl FnOnce(
            &ExactBuilder<'id>,
            &mut OwnedWork<'id, '_>,
        ) -> Result<(AtomFields, AuthorityBinding), CoreError>,
    ) -> Result<AtomRef, CoreError> {
        // A first packet can allocate after immutable lowering. Select that
        // entire original path before lowering; never retry a completed lower.
        if self.op.uses_task_access() && self.builder.ir.packets.has_allocation() {
            self.builder.ensure(self.op)?;
            let reference = ticket.reference;
            let next = self.start + self.committed;
            task::check_commit(self.builder, reference, self.pending, next, self.op)?;
            let lowered = lower(self.builder, self.op).map_err(|error| self.op.fail(error));
            let (atom, authority) = lowered.map_err(|error| self.op.fail(error))?;
            task::commit_packet(
                self.builder,
                &mut self.pending,
                &mut self.committed,
                next,
                AtomPacket { atom, authority },
                self.op,
            )?;
            return Ok(reference);
        }
        let pending = self.pending;
        let next = self.start + self.committed;
        let reference = ticket.reference;
        let result = self.transaction(|builder, op| {
            let id = builder.strip(reference, op)?;
            op.scan(1)?;
            let state = &builder.states[Table::Atoms as usize];
            let bit = state.bits.get(id.index(), op)?.unwrap_or(0);
            if pending != Some(id)
                || bit != 1
                || id.index() != next
                || next != builder.ir.packets.len()
            {
                return Err(op.fail(CoreError::InvalidProvenance));
            }
            // Lowering has immutable builder access. No atom reservation or row
            // can change between this check, the write and the 1 -> 2 transition.
            let (atom, authority) = lower(builder, op)?;
            builder
                .ir
                .packets
                .push(AtomPacket { atom, authority }, op)?;
            builder.states[Table::Atoms as usize]
                .bits
                .replace(id.index(), 2, op)?;
            Ok(reference)
        });
        if result.is_ok() {
            self.pending = None;
            self.committed += 1;
        }
        result
    }

    pub(crate) fn commit_owned(
        &mut self,
        ticket: ReservedAtom,
        value: DraftOwnedAtom,
    ) -> Result<AtomRef, CoreError> {
        self.commit(ticket, |builder, op| {
            let (atom, primary) = value.lower(builder, op)?;
            Ok((atom, AuthorityBinding::Owned(primary)))
        })
    }

    pub(crate) fn commit_shared(
        &mut self,
        ticket: ReservedAtom,
        value: DraftAtomFields,
        authority: AuthorityRef,
    ) -> Result<AtomRef, CoreError> {
        self.commit(ticket, |builder, op| {
            let atom = value.lower(builder, op)?;
            let authority = builder.authority_ref(authority, true, op)?;
            Ok((atom, AuthorityBinding::Shared(authority)))
        })
    }

    pub(crate) fn close(mut self) -> Result<WrittenAtomBlock, CoreError> {
        let start = self.start;
        let expected = self.expected;
        let committed = self.committed;
        let pending = self.pending;
        let result = self.transaction(|builder, op| {
            op.events(1)?;
            if pending.is_some()
                || committed != expected
                || builder.ir.packets.len() != start + expected
            {
                return Err(op.fail(CoreError::InvalidProvenance));
            }
            let rows = Rows::new(start, expected).map_err(|error| op.fail(error))?;
            Ok(WrittenAtomBlock {
                reference: builder.scoped(rows),
            })
        });
        if result.is_ok() {
            self.closed = true;
        }
        result
    }

    // Restricted auxiliary forwarding: none of these methods can reserve or
    // write another atom, start/close a target or finish the builder.
    pub(crate) fn reserve_proof(&mut self, index: usize) -> Result<ProofRef, CoreError> {
        self.builder.reserve_proof(index, self.op)
    }

    pub(crate) fn primitive_parts(
        &mut self,
        rows: &[PrimitiveRow],
    ) -> Result<DraftRef<Rows<PrimitiveRow>>, CoreError> {
        self.builder.primitive_parts(rows, self.op)
    }

    pub(crate) fn written_primitive_block(
        &mut self,
        start: usize,
        len: usize,
    ) -> Result<DraftRef<Rows<PrimitiveRow>>, CoreError> {
        self.builder.written_primitive_block(start, len, self.op)
    }

    pub(crate) fn write_extra_authority(
        &mut self,
        specification: DraftAuthoritySpec,
    ) -> Result<AuthorityRef, CoreError> {
        self.builder.write_extra_authority(specification, self.op)
    }

    pub(crate) fn stage_parts(
        &mut self,
        rows: &[DraftStageRow],
    ) -> Result<DraftRef<Rows<StageRow>>, CoreError> {
        self.builder.stage_parts(rows, self.op)
    }

    pub(crate) fn written_stage_block(
        &mut self,
        start: usize,
        len: usize,
    ) -> Result<DraftRef<Rows<StageRow>>, CoreError> {
        self.builder.written_stage_block(start, len, self.op)
    }

    pub(crate) fn write_proof(
        &mut self,
        reference: ProofRef,
        row: DraftProofRow,
    ) -> Result<ProofRef, CoreError> {
        self.builder.write_proof(reference, row, self.op)
    }
}

impl Drop for TargetWriter<'_, '_, '_, '_> {
    fn drop(&mut self) {
        if !self.closed {
            // No work or allocation during unwind/cancellation/abandonment.
            // A prior owner error is still returned by its first-fault latch.
            self.op.fail(CoreError::InvalidProvenance);
        }
    }
}

impl Lower for DraftOwnedAtom {
    type Value = (AtomFields, PrimaryAuthority);

    fn lower<'id>(
        self,
        builder: &ExactBuilder<'id>,
        op: &mut OwnedWork<'id, '_>,
    ) -> Result<Self::Value, CoreError> {
        op.view_scan((size_of::<Self>() - size_of::<DraftAtomContent>()) as u64)?;
        let atom = AtomFields {
            owner: builder.reference(self.owner, false, op)?,
            kind: self.kind,
            target: self.target,
            sources: builder.source_ref(self.sources, op)?,
            proof: builder.optional(self.proof, op)?,
            content: self.content.lower(builder, op)?,
        };
        let primary = PrimaryAuthority {
            phase: self.primary.phase,
            kind: self.primary.kind,
            consumed: self.primary.consumed,
            host_role: self.primary.host_role,
            host_utf8_len: self.primary.host_utf8_len,
            host_domains: builder.source_ref(self.primary.host_domains, op)?,
        };
        Ok((atom, primary))
    }
}
