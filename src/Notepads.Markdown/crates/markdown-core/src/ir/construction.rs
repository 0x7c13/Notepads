// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE file in the project root for license information.
// ---------------------------------------------------------------------------------------------

//! Exact unpublished construction. Scoped handles never authorize provenance.

use super::*;
use crate::error::LimitKind;
use crate::memory::{BuildExactRows, BuildExactText};
use crate::policy::{ConstructionId, OwnedWork};
use std::num::NonZeroU64;
use std::sync::atomic::{AtomicU64, Ordering};

mod drafts;
mod rows;
#[cfg(test)]
mod slots;
mod target;
mod task;
#[cfg(test)]
mod tests;
pub(crate) use drafts::*;
use rows::Lower;
#[cfg(test)]
pub(crate) use slots::SlotOwner;
pub(crate) use target::DraftAtomBlock;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct BuilderKey(NonZeroU64);
static NEXT_BUILDER: AtomicU64 = AtomicU64::new(0);

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct DraftRef<T: Copy> {
    key: BuilderKey,
    value: T,
}
impl<T: crate::memory::BuildValue> crate::memory::BuildValue for DraftRef<T> {}
impl<T: Copy> DraftRef<T> {
    /// Local identity is readable, but cannot be turned into a scoped handle.
    #[cfg(test)]
    pub(crate) fn local(self) -> T {
        self.value
    }
}
pub(crate) type NodeRef = DraftRef<NodeId>;
pub(crate) type FlowRef = DraftRef<FlowId>;
pub(crate) type AtomRef = DraftRef<AtomId>;
pub(crate) type DetailRef = DraftRef<DetailId>;
pub(crate) type ObjectRef = DraftRef<ObjectId>;
pub(crate) type ProofRef = DraftRef<ProofId>;
pub(crate) type AuthorityRef = DraftRef<ClaimId>;
pub(crate) type SourceBlockRef = DraftRef<Rows<SourceSpan>>;
pub(crate) type TextRef = DraftRef<TextSlice>;

pub(crate) struct ReservedAtom {
    reference: AtomRef,
}
impl ReservedAtom {
    pub(crate) fn atom_ref(&self) -> AtomRef {
        self.reference
    }
    pub(crate) fn primary_ref(&self) -> AuthorityRef {
        DraftRef {
            key: self.reference.key,
            value: ClaimId::primary(self.reference.value),
        }
    }
}

#[derive(Clone, Copy, Debug, Default)]
pub(crate) struct ExactCounts {
    pub nodes: usize,
    pub flows: usize,
    pub atoms: usize,
    pub extra_authorities: usize,
    pub details: usize,
    pub objects: usize,
    pub payloads: usize,
    pub definitions: usize,
    pub references: usize,
    pub proofs: usize,
    pub primitives: usize,
    pub stages: usize,
    pub attributes: usize,
    pub sources: usize,
    pub alignments: usize,
    pub text_bytes: usize,
}

#[derive(Clone, Copy)]
pub(crate) enum Table {
    Nodes,
    Flows,
    Atoms,
    Details,
    Objects,
    Payloads,
    Definitions,
    References,
    Proofs,
}
struct State<'id> {
    bits: BuildExactRows<'id, u8>,
    count: usize,
}
pub(crate) trait TableId: crate::memory::BuildValue {
    const TABLE: Table;
    fn index(self) -> usize;
    fn at(index: usize) -> Result<Self, CoreError>;
}
macro_rules! ids {
    ($($id:ty => $table:ident),+ $(,)?) => {$(
        impl TableId for $id {
            const TABLE: Table = Table::$table;
            fn index(self) -> usize { self.index() }
            fn at(index: usize) -> Result<Self, CoreError> { Self::from_index(index) }
        }
    )+};
}
ids!(NodeId=>Nodes, FlowId=>Flows, AtomId=>Atoms, DetailId=>Details,
    ObjectId=>Objects, PayloadId=>Payloads, DefinitionId=>Definitions,
    ReferenceUseId=>References, ProofId=>Proofs);

pub(crate) struct ExactBuilder<'id> {
    key: BuilderKey,
    ir: DraftIr<'id>,
    states: [State<'id>; 9],
    counts: ExactCounts,
    bulk_reserved: [bool; 9],
}

/// A table whose actual slots have already made their unique 0 -> 1 transition.
/// The issuing scope is held once, rather than copied into a parallel row table.
pub(crate) struct ReservationTable<'id, T: Copy> {
    key: BuilderKey,
    count: usize,
    _type: std::marker::PhantomData<(T, ConstructionId<'id>)>,
}

impl<'id, T: TableId> ReservationTable<'id, T> {
    pub(crate) fn at(
        &self,
        index: usize,
        op: &mut OwnedWork<'id, '_>,
    ) -> Result<DraftRef<T>, CoreError> {
        op.check()?;
        op.events(1)?;
        if index >= self.count {
            return Err(op.fail(CoreError::InvalidProvenance));
        }
        Ok(DraftRef {
            key: self.key,
            value: T::at(index).map_err(|error| op.fail(error))?,
        })
    }
}

impl<'id> ExactBuilder<'id> {
    fn reserve_table<T: TableId>(
        &mut self,
        op: &mut OwnedWork<'id, '_>,
    ) -> Result<ReservationTable<'id, T>, CoreError> {
        self.transaction(op, |this, op| {
            let table = T::TABLE as usize;
            if this.bulk_reserved[table] {
                return Err(op.fail(CoreError::InvalidProvenance));
            }
            op.copy(1)?;
            this.bulk_reserved[table] = true;
            let count = this.states[T::TABLE as usize].count;
            for index in 0..count {
                // Keep actual admission, duplicate rejection and state writes.
                this.reserve::<T>(index, op)?;
            }
            Ok(ReservationTable {
                key: this.key,
                count,
                _type: std::marker::PhantomData,
            })
        })
    }
    pub(crate) fn written_primitive_block(
        &self,
        start: usize,
        len: usize,
        op: &mut OwnedWork<'id, '_>,
    ) -> Result<DraftRef<Rows<PrimitiveRow>>, CoreError> {
        let result = (|| {
            self.ensure(op)?;
            let reference = self.scoped(Rows::new(start, len).map_err(|e| op.fail(e))?);
            self.primitive_ref(reference, op)?;
            Ok(reference)
        })();
        result.map_err(|error| op.fail(error))
    }
    pub(crate) fn written_stage_block(
        &self,
        start: usize,
        len: usize,
        op: &mut OwnedWork<'id, '_>,
    ) -> Result<DraftRef<Rows<StageRow>>, CoreError> {
        let result = (|| {
            self.ensure(op)?;
            let reference = self.scoped(Rows::new(start, len).map_err(|e| op.fail(e))?);
            self.stage_ref(reference, op)?;
            Ok(reference)
        })();
        result.map_err(|error| op.fail(error))
    }
    pub(crate) fn new(counts: ExactCounts, op: &mut OwnedWork<'id, '_>) -> Result<Self, CoreError> {
        op.events(1)?;
        let limits = op
            .model_limits()
            .ok_or_else(|| op.fail(CoreError::UnsupportedConfiguration))?;
        let model_counts = [
            counts.nodes,
            counts.flows,
            counts.atoms,
            counts.extra_authorities,
            counts.details,
            counts.objects,
            counts.payloads,
            counts.definitions,
            counts.references,
            counts.proofs,
            counts.attributes,
            counts.alignments,
        ];
        let mut total = 0_u64;
        for count in model_counts {
            op.events(1)?;
            if count >= u32::MAX as usize {
                return Err(op.fail(CoreError::ArithmeticOverflow));
            }
            total = total
                .checked_add(count as u64)
                .ok_or_else(|| op.fail(CoreError::ArithmeticOverflow))?;
        }
        op.check_limit(LimitKind::ModelRecords, total, limits.model_records)?;
        op.check_limit(
            LimitKind::SourcePieces,
            counts.sources as u64,
            limits.source_pieces,
        )?;
        let proof_count = counts
            .primitives
            .checked_add(counts.stages)
            .ok_or_else(|| op.fail(CoreError::ArithmeticOverflow))?;
        op.check_limit(
            LimitKind::ProofRecords,
            proof_count as u64,
            limits.proof_records,
        )?;
        op.check_limit(
            LimitKind::DecodedBytes,
            counts.text_bytes as u64,
            limits.decoded_bytes,
        )?;
        for count in [
            counts.sources,
            counts.primitives,
            counts.stages,
            counts.text_bytes,
        ] {
            if count > u32::MAX as usize {
                return Err(op.fail(CoreError::ArithmeticOverflow));
            }
        }
        op.scan(size_of::<u64>() as u64)?;
        op.copy(size_of::<u64>() as u64)?;
        let old = NEXT_BUILDER
            .try_update(Ordering::Relaxed, Ordering::Relaxed, |value| {
                value.checked_add(1)
            })
            .map_err(|_| op.fail(CoreError::ArithmeticOverflow))?;
        let key = BuilderKey(
            NonZeroU64::new(old + 1).ok_or_else(|| op.fail(CoreError::InternalInvariant))?,
        );
        let ir = DraftIr {
            nodes: BuildExactRows::new(counts.nodes, op)?,
            flows: BuildExactRows::new(counts.flows, op)?,
            packets: BuildExactRows::new(counts.atoms, op)?,
            extra_authorities: BuildExactRows::new(counts.extra_authorities, op)?,
            details: BuildExactRows::new(counts.details, op)?,
            objects: BuildExactRows::new(counts.objects, op)?,
            payloads: BuildExactRows::new(counts.payloads, op)?,
            definitions: BuildExactRows::new(counts.definitions, op)?,
            references: BuildExactRows::new(counts.references, op)?,
            proofs: BuildExactRows::new(counts.proofs, op)?,
            primitives: BuildExactRows::new(counts.primitives, op)?,
            stages: BuildExactRows::new(counts.stages, op)?,
            attributes: BuildExactRows::new(counts.attributes, op)?,
            sources: BuildExactRows::new(counts.sources, op)?,
            alignments: BuildExactRows::new(counts.alignments, op)?,
            text: BuildExactText::new(counts.text_bytes, op)?,
        };
        let ns = [
            counts.nodes,
            counts.flows,
            counts.atoms,
            counts.details,
            counts.objects,
            counts.payloads,
            counts.definitions,
            counts.references,
            counts.proofs,
        ];
        // Empty ledgers allocate lazily when their first real reservation is issued.
        macro_rules! state {
            ($index:expr) => {
                State {
                    bits: BuildExactRows::new(ns[$index], op)?,
                    count: ns[$index],
                }
            };
        }
        let states = [
            state!(0),
            state!(1),
            state!(2),
            state!(3),
            state!(4),
            state!(5),
            state!(6),
            state!(7),
            state!(8),
        ];
        Ok(Self {
            key,
            ir,
            states,
            counts,
            bulk_reserved: [false; 9],
        })
    }

    fn ensure(&self, op: &mut OwnedWork<'id, '_>) -> Result<(), CoreError> {
        op.check()
    }
    fn transaction<T>(
        &mut self,
        op: &mut OwnedWork<'id, '_>,
        f: impl FnOnce(&mut Self, &mut OwnedWork<'id, '_>) -> Result<T, CoreError>,
    ) -> Result<T, CoreError> {
        let result = self.ensure(op).and_then(|()| f(self, op));
        result.map_err(|error| op.fail(error))
    }
    fn scoped<T: Copy>(&self, value: T) -> DraftRef<T> {
        DraftRef {
            key: self.key,
            value,
        }
    }
    fn strip<T: Copy>(
        &self,
        value: DraftRef<T>,
        op: &mut OwnedWork<'id, '_>,
    ) -> Result<T, CoreError> {
        op.events(1)?;
        op.scan(size_of::<BuilderKey>() as u64)?;
        if value.key != self.key {
            return Err(op.fail(CoreError::InvalidProvenance));
        }
        Ok(value.value)
    }
    fn reference<T: TableId>(
        &self,
        value: DraftRef<T>,
        written: bool,
        op: &mut OwnedWork<'id, '_>,
    ) -> Result<T, CoreError> {
        let id = self.strip(value, op)?;
        let state = &self.states[T::TABLE as usize];
        op.scan(1)?;
        let bit = state.bits.get(id.index(), op)?.unwrap_or(0);
        if bit < if written { 2 } else { 1 } {
            return Err(op.fail(CoreError::InvalidProvenance));
        }
        Ok(id)
    }
    fn optional<T: TableId>(
        &self,
        value: Option<DraftRef<T>>,
        op: &mut OwnedWork<'id, '_>,
    ) -> Result<Option<T>, CoreError> {
        value
            .map(|value| self.reference(value, false, op))
            .transpose()
    }
    fn reserve<T: TableId>(
        &mut self,
        index: usize,
        op: &mut OwnedWork<'id, '_>,
    ) -> Result<DraftRef<T>, CoreError> {
        // The first reservation retains the complete original lazy ledger
        // initialization. Only an existing allocation enters a task window.
        if op.uses_task_access() && self.states[T::TABLE as usize].bits.len() != 0 {
            return task::reserve(self, index, op);
        }
        let state = &mut self.states[T::TABLE as usize];
        if index >= state.count {
            return Err(op.fail(CoreError::InvalidProvenance));
        }
        if state.bits.len() == 0 {
            for _ in 0..state.count {
                state.bits.push(0, op)?;
            }
        }
        op.scan(1)?;
        if state
            .bits
            .get(index, op)?
            .ok_or_else(|| op.fail(CoreError::InvalidProvenance))?
            != 0
        {
            return Err(op.fail(CoreError::InvalidProvenance));
        }
        op.admit_model(LimitKind::ModelRecords, 1)?;
        state.bits.replace(index, 1, op)?;
        Ok(self.scoped(T::at(index).map_err(|e| op.fail(e))?))
    }
    fn complete_row<T: TableId>(
        &mut self,
        value: DraftRef<T>,
        op: &mut OwnedWork<'id, '_>,
    ) -> Result<(), CoreError> {
        let id = self.reference(value, false, op)?;
        let state = &mut self.states[T::TABLE as usize];
        if state
            .bits
            .get(id.index(), op)?
            .ok_or_else(|| op.fail(CoreError::InvalidProvenance))?
            != 1
        {
            return Err(op.fail(CoreError::InvalidProvenance));
        }
        state.bits.replace(id.index(), 2, op)
    }
    pub(crate) fn reserve_atom(
        &mut self,
        index: usize,
        op: &mut OwnedWork<'id, '_>,
    ) -> Result<ReservedAtom, CoreError> {
        self.transaction(op, |this, op| {
            Ok(ReservedAtom {
                reference: this.reserve(index, op)?,
            })
        })
    }
    #[cfg(test)]
    pub(crate) fn atom_slots(
        &self,
        op: &mut OwnedWork<'id, '_>,
    ) -> Result<SlotOwner<'id, ReservedAtom>, CoreError> {
        self.ensure(op)?;
        let result = SlotOwner::new(self.key, self.counts.atoms, op);
        result.map_err(|error| op.fail(error))
    }
    fn authority_ref(
        &self,
        value: AuthorityRef,
        terminal: bool,
        op: &mut OwnedWork<'id, '_>,
    ) -> Result<ClaimId, CoreError> {
        let id = self.strip(value, op)?;
        match id.storage() {
            types::ClaimStorage::Primary(atom) => {
                self.reference(self.scoped(atom), terminal, op)?;
                if terminal {
                    self.ir.authority(id, op)?;
                }
            }
            types::ClaimStorage::Extra(extra) => {
                if extra.index() >= self.ir.extra_authorities.len() {
                    return Err(op.fail(CoreError::InvalidProvenance));
                }
            }
        }
        Ok(id)
    }
    fn source_ref(
        &self,
        value: SourceBlockRef,
        op: &mut OwnedWork<'id, '_>,
    ) -> Result<Rows<SourceSpan>, CoreError> {
        let rows = self.strip(value, op)?;
        rows.indices(self.ir.sources.len())
            .map_err(|e| op.fail(e))?;
        Ok(rows)
    }
    fn text_ref(
        &self,
        value: TextRef,
        op: &mut OwnedWork<'id, '_>,
    ) -> Result<TextSlice, CoreError> {
        let text = self.strip(value, op)?;
        self.ir.text.validate_range(text.start(), text.len(), op)?;
        Ok(text)
    }
    #[cfg(test)]
    fn atom_block_ref(
        &self,
        value: DraftRef<Rows<AtomRow>>,
        op: &mut OwnedWork<'id, '_>,
    ) -> Result<Rows<AtomRow>, CoreError> {
        let rows = self.strip(value, op)?;
        let end = rows
            .start()
            .checked_add(rows.len())
            .ok_or_else(|| op.fail(CoreError::ArithmeticOverflow))?;
        if end > self.counts.atoms {
            return Err(op.fail(CoreError::InvalidProvenance));
        }
        for index in rows.start()..end {
            self.reference(
                self.scoped(AtomId::from_index(index).map_err(|e| op.fail(e))?),
                false,
                op,
            )?;
        }
        Ok(rows)
    }
    #[cfg(test)]
    pub(crate) fn reserved_atom_block(
        &self,
        start: usize,
        len: usize,
        op: &mut OwnedWork<'id, '_>,
    ) -> Result<DraftRef<Rows<AtomRow>>, CoreError> {
        self.ensure(op)?;
        let result = (|| {
            let value = self.scoped(Rows::new(start, len).map_err(|e| op.fail(e))?);
            self.atom_block_ref(value, op)?;
            Ok(value)
        })();
        result.map_err(|error| op.fail(error))
    }
    pub(crate) fn written_source_block(
        &self,
        start: usize,
        len: usize,
        op: &mut OwnedWork<'id, '_>,
    ) -> Result<SourceBlockRef, CoreError> {
        self.ensure(op)?;
        let result = (|| {
            let value = self.scoped(Rows::new(start, len).map_err(|e| op.fail(e))?);
            self.source_ref(value, op)?;
            Ok(value)
        })();
        result.map_err(|error| op.fail(error))
    }
    pub(crate) fn source_parts(
        &mut self,
        rows: &[SourceSpan],
        op: &mut OwnedWork<'id, '_>,
    ) -> Result<SourceBlockRef, CoreError> {
        self.transaction(op, |this, op| {
            let value = Rows::new(this.ir.sources.len(), rows.len()).map_err(|e| op.fail(e))?;
            op.admit_model(LimitKind::SourcePieces, rows.len() as u64)?;
            for row in rows {
                this.ir.sources.push(*row, op)?;
            }
            Ok(this.scoped(value))
        })
    }

    #[cfg(test)]
    pub(crate) fn text(
        &mut self,
        text: &str,
        op: &mut OwnedWork<'id, '_>,
    ) -> Result<TextRef, CoreError> {
        self.transaction(op, |this, op| {
            let value = TextSlice {
                start: u32::try_from(this.ir.text.len())
                    .map_err(|_| op.fail(CoreError::ArithmeticOverflow))?,
                len: u32::try_from(text.len())
                    .map_err(|_| op.fail(CoreError::ArithmeticOverflow))?,
            };
            this.ir.text.append(text, op)?;
            Ok(this.scoped(value))
        })
    }

    pub(crate) fn finish(self, op: &mut OwnedWork<'id, '_>) -> Result<CompactIr, CoreError> {
        self.ensure(op)?;
        for state in &self.states {
            if state.count == 0 {
                continue;
            }
            state.bits.complete(op)?;
            for index in 0..state.bits.len() {
                let bit = state
                    .bits
                    .get(index, op)?
                    .ok_or_else(|| op.fail(CoreError::InvalidProvenance))?;
                op.scan(1)?;
                if bit != 2 {
                    return Err(op.fail(CoreError::InvalidProvenance));
                }
            }
        }
        macro_rules! complete { ($($field:ident),+)=>{ $(self.ir.$field.complete(op)?;)+ }; }
        complete!(
            nodes,
            flows,
            packets,
            extra_authorities,
            details,
            objects,
            payloads,
            definitions,
            references,
            proofs,
            primitives,
            stages,
            attributes,
            sources,
            alignments,
            text
        );
        for index in 0..self.ir.packets.len() {
            let packet = self
                .ir
                .packets
                .get(index, op)?
                .ok_or_else(|| op.fail(CoreError::InvalidProvenance))?;
            op.events(1)?;
            op.scan(types::AUTHORITY_SELECTOR_BYTES)?;
            if let AuthorityBinding::Shared(id) = &packet.authority {
                op.scan(size_of::<ClaimId>() as u64)?;
                self.ir.authority(*id, op)?;
            }
        }
        op.check()?;
        self.ir.freeze(op)
    }
}
macro_rules! rows {
    ($($write:ident,$table:ident,$draft:ty,$id:ty);+ $(;)?) => {$(
        impl<'id> ExactBuilder<'id> {
            pub(crate) fn $write(&mut self,reference:DraftRef<$id>,row:$draft,op:&mut OwnedWork<'id, '_>)->Result<DraftRef<$id>,CoreError> {
                self.transaction(op,|this,op|{
                    let id=this.reference(reference,false,op)?;
                    if id.index()!=this.ir.$table.len() {return Err(op.fail(CoreError::InvalidProvenance));}
                    if op.uses_task_access() && this.ir.$table.has_allocation() {
                        let value=row.lower(this,op)?;
                        return task::write_row(
                            task::RowTarget {
                                rows: &mut this.ir.$table,
                                states: &mut this.states[<$id>::TABLE as usize].bits,
                                key: this.key,
                            },
                            reference,
                            value,
                            op,
                        );
                    }
                    let value=row.lower(this,op)?;
                    this.ir.$table.push(value,op)?;
                    this.complete_row(reference,op)?; Ok(reference)
                })
            }
        }
    )+};
}
macro_rules! single_reservations {
    ($($(#[$attribute:meta])* $name:ident => $id:ty),+ $(,)?) => {$(
        impl<'id> ExactBuilder<'id> {
            $(#[$attribute])*
            pub(crate) fn $name(&mut self,index:usize,op:&mut OwnedWork<'id, '_>)->Result<DraftRef<$id>,CoreError> {
                self.transaction(op,|this,op|this.reserve(index,op))
            }
        }
    )+};
}
macro_rules! tables {
    ($($name:ident => $id:ty),+ $(,)?) => {$(
        impl<'id> ExactBuilder<'id> {
            pub(crate) fn $name(
                &mut self,
                op: &mut OwnedWork<'id, '_>,
            ) -> Result<ReservationTable<'id, $id>, CoreError> {
                self.reserve_table::<$id>(op)
            }
        }
    )+};
}
tables!(
    reserve_nodes => NodeId,
    reserve_flows => FlowId,
    reserve_objects => ObjectId,
);
single_reservations!(
    #[cfg(test)] reserve_node => NodeId,
    #[cfg(test)] reserve_flow => FlowId,
    #[cfg(test)] reserve_object => ObjectId,
    reserve_proof => ProofId,
);
rows!(write_node,nodes,DraftNodeRow,NodeId;
    write_flow,flows,DraftFlowRow,FlowId;
    write_proof,proofs,DraftProofRow,ProofId);

impl<'id> ExactBuilder<'id> {
    fn primitive_ref(
        &self,
        value: DraftRef<Rows<PrimitiveRow>>,
        op: &mut OwnedWork<'id, '_>,
    ) -> Result<Rows<PrimitiveRow>, CoreError> {
        let rows = self.strip(value, op)?;
        rows.indices(self.ir.primitives.len())
            .map_err(|e| op.fail(e))?;
        Ok(rows)
    }
    fn stage_ref(
        &self,
        value: DraftRef<Rows<StageRow>>,
        op: &mut OwnedWork<'id, '_>,
    ) -> Result<Rows<StageRow>, CoreError> {
        let rows = self.strip(value, op)?;
        rows.indices(self.ir.stages.len()).map_err(|e| op.fail(e))?;
        Ok(rows)
    }

    pub(crate) fn primitive_parts(
        &mut self,
        values: &[PrimitiveRow],
        op: &mut OwnedWork<'id, '_>,
    ) -> Result<DraftRef<Rows<PrimitiveRow>>, CoreError> {
        self.transaction(op, |this, op| {
            let rows = Rows::new(this.ir.primitives.len(), values.len()).map_err(|e| op.fail(e))?;
            op.admit_model(LimitKind::ProofRecords, values.len() as u64)?;
            for value in values {
                this.ir.primitives.push(*value, op)?;
            }
            Ok(this.scoped(rows))
        })
    }
    pub(crate) fn stage_parts(
        &mut self,
        values: &[DraftStageRow],
        op: &mut OwnedWork<'id, '_>,
    ) -> Result<DraftRef<Rows<StageRow>>, CoreError> {
        self.transaction(op, |this, op| {
            let rows = Rows::new(this.ir.stages.len(), values.len()).map_err(|e| op.fail(e))?;
            op.admit_model(LimitKind::ProofRecords, values.len() as u64)?;
            for value in values {
                let row = value.lower(this, op)?;
                this.ir.stages.push(row, op)?;
            }
            Ok(this.scoped(rows))
        })
    }

    pub(crate) fn write_extra_authority(
        &mut self,
        value: DraftAuthoritySpec,
        op: &mut OwnedWork<'id, '_>,
    ) -> Result<AuthorityRef, CoreError> {
        self.transaction(op, |this, op| {
            let value = value.lower(this, op)?;
            let id = ExtraAuthorityId::from_index(this.ir.extra_authorities.len())
                .map_err(|e| op.fail(e))?;
            op.admit_model(LimitKind::ModelRecords, 1)?;
            this.ir
                .extra_authorities
                .push(ExtraAuthority { value }, op)?;
            Ok(this.scoped(ClaimId::extra(id)))
        })
    }
    #[cfg(test)]
    fn put_atom(
        &mut self,
        ticket: ReservedAtom,
        atom: AtomFields,
        authority: AuthorityBinding,
        op: &mut OwnedWork<'id, '_>,
    ) -> Result<AtomRef, CoreError> {
        let reference = ticket.reference;
        let id = self.reference(reference, false, op)?;
        if id.index() != self.ir.packets.len() {
            return Err(op.fail(CoreError::InvalidProvenance));
        }
        self.ir.packets.push(AtomPacket { atom, authority }, op)?;
        self.complete_row(reference, op)?;
        Ok(reference)
    }
    #[cfg(test)]
    pub(crate) fn write_owned_atom(
        &mut self,
        ticket: ReservedAtom,
        atom: DraftAtomFields,
        authority: DraftAuthoritySpec,
        op: &mut OwnedWork<'id, '_>,
    ) -> Result<AtomRef, CoreError> {
        self.transaction(op, |this, op| {
            this.reference(ticket.reference, false, op)?;
            let atom = atom.lower(this, op)?;
            let authority = authority.lower(this, op)?;
            if atom.owner != authority.owner || atom.owner != authority.host.owner {
                return Err(op.fail(CoreError::InvalidProvenance));
            }
            if atom.sources != authority.sources {
                let a = atom
                    .sources
                    .indices(this.ir.sources.len())
                    .map_err(|e| op.fail(e))?;
                let b = authority
                    .sources
                    .indices(this.ir.sources.len())
                    .map_err(|e| op.fail(e))?;
                if a.len() != b.len() {
                    return Err(op.fail(CoreError::InvalidProvenance));
                }
                for (a, b) in a.zip(b) {
                    op.events(1)?;
                    op.scan((2 * size_of::<SourceSpan>()) as u64)?;
                    if this.ir.sources.get(a, op)? != this.ir.sources.get(b, op)? {
                        return Err(op.fail(CoreError::InvalidProvenance));
                    }
                }
            }
            let primary = PrimaryAuthority {
                phase: authority.phase,
                kind: authority.kind,
                consumed: authority.consumed,
                host_role: authority.host.role,
                host_utf8_len: authority.host.utf8_len,
                host_domains: authority.host.domains,
            };
            this.put_atom(ticket, atom, AuthorityBinding::Owned(primary), op)
        })
    }
    #[cfg(test)]
    pub(crate) fn write_shared_atom(
        &mut self,
        ticket: ReservedAtom,
        atom: DraftAtomFields,
        authority: AuthorityRef,
        op: &mut OwnedWork<'id, '_>,
    ) -> Result<AtomRef, CoreError> {
        self.transaction(op, |this, op| {
            this.reference(ticket.reference, false, op)?;
            let atom = atom.lower(this, op)?;
            let authority = this.authority_ref(authority, true, op)?;
            this.put_atom(ticket, atom, AuthorityBinding::Shared(authority), op)
        })
    }

    #[cfg(test)]
    pub(crate) fn patch_flow_target(
        &mut self,
        flow: FlowRef,
        atoms: DraftRef<Rows<AtomRow>>,
        length: u64,
        op: &mut OwnedWork<'id, '_>,
    ) -> Result<(), CoreError> {
        self.transaction(op, |this, op| {
            let id = this.reference(flow, true, op)?;
            op.scan(size_of::<FlowRow>() as u64)?;
            op.copy(size_of::<FlowRow>() as u64)?;
            let mut row = this
                .ir
                .flows
                .get(id.index(), op)?
                .ok_or_else(|| op.fail(CoreError::InvalidProvenance))?;
            row.atoms = this.atom_block_ref(atoms, op)?;
            row.logical_utf16 = length;
            this.ir.flows.replace(id.index(), row, op)
        })
    }
}
