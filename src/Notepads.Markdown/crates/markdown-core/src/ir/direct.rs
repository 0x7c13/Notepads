// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE file in the project root for license information.
// ---------------------------------------------------------------------------------------------

//! Direct final-page construction. Scoped identities never grant provenance.
use super::direct_rows::SlotRows;
use super::node_rows::NodeStorage;
use super::segmented_text::SegmentBuilder;
pub(crate) use super::segmented_text::read_chunk as owned_text_chunk;
use super::*;
use crate::error::LimitKind;
use crate::memory::{BuildGrowingVec, BuildRows, BuildTextChunk};
use crate::policy::OwnedWork;
use std::sync::atomic::{AtomicU64, Ordering};
mod drafts;
mod rows;
mod target;
pub(crate) use drafts::*;
use rows::Lower;
pub(crate) use target::{ClosedTarget, TargetWriter, WrittenAtomBlock};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct BuilderKey(u64);
static NEXT_BUILDER: AtomicU64 = AtomicU64::new(0);
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct DraftRef<T: Copy> {
    key: BuilderKey,
    value: T,
}
impl<T: crate::memory::BuildValue> crate::memory::BuildValue for DraftRef<T> {}
pub(crate) type NodeRef = DraftRef<NodeId>;
pub(crate) type FlowRef = DraftRef<FlowId>;
pub(crate) type DetailRef = DraftRef<DetailId>;
pub(crate) type ObjectRef = DraftRef<ObjectId>;
pub(crate) type PayloadRef = DraftRef<PayloadId>;
pub(crate) type DefinitionRef = DraftRef<DefinitionId>;
pub(crate) type ReferenceUseRef = DraftRef<ReferenceUseId>;
pub(crate) type ProofRef = DraftRef<ProofId>;
pub(crate) type SourceBlockRef = DraftRef<Rows<SourceSpan>>;
pub(crate) type TextRef = DraftRef<TextSlice>;
/// Identity of this existing builder, without granting any row or value access.
pub(crate) type BuilderBinding = DraftRef<()>;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct BornAtom {
    key: BuilderKey,
    target: u64,
    index: usize,
}
impl crate::memory::BuildValue for BornAtom {}
#[cfg(test)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct FinalAtomRef(DraftRef<AtomId>);
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum AtomRef {
    Born(BornAtom),
    #[cfg(test)]
    Final(FinalAtomRef),
}
impl From<BornAtom> for AtomRef {
    fn from(value: BornAtom) -> Self {
        Self::Born(value)
    }
}
#[cfg(test)]
impl From<FinalAtomRef> for AtomRef {
    fn from(value: FinalAtomRef) -> Self {
        Self::Final(value)
    }
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum AuthorityRef {
    Born(BornAtom),
    Final(DraftRef<ClaimId>),
}
impl BornAtom {
    pub(crate) fn primary(self) -> AuthorityRef {
        AuthorityRef::Born(self)
    }
}
#[derive(Debug, Eq, PartialEq)]
pub(crate) enum DraftAtomBlock {
    Written(WrittenAtomBlock),
}
impl From<WrittenAtomBlock> for DraftAtomBlock {
    fn from(value: WrittenAtomBlock) -> Self {
        Self::Written(value)
    }
}
impl Lower for DraftAtomBlock {
    type Value = Rows<AtomRow>;
    fn lower<'id>(
        self,
        b: &DirectBuilder<'id>,
        w: &mut OwnedWork<'id, '_>,
    ) -> Result<Self::Value, CoreError> {
        let Self::Written(value) = self;
        let rows = b.strip(value.reference, w)?;
        rows.indices(b.completed_atoms).map_err(|e| w.fail(e))?;
        Ok(rows)
    }
}

#[derive(Clone, Copy)]
enum FixupSite {
    Object(usize),
    Proof(usize),
    Stage(usize),
    Detail(usize),
    Definition(usize),
    Reference(usize),
    Attribute(usize),
}
impl crate::memory::BuildValue for FixupSite {}

pub(crate) trait TableId: crate::memory::BuildValue {
    fn index(self) -> usize;
    fn len(b: &DirectBuilder<'_>) -> usize;
    fn written<'id>(
        b: &DirectBuilder<'id>,
        index: usize,
        w: &mut OwnedWork<'id, '_>,
    ) -> Result<bool, CoreError>;
}

struct DirectDraft<'id> {
    nodes: NodeStorage<'id>,
    flows: SlotRows<'id, FlowRow>,
    packets: SlotRows<'id, AtomPacket>,
    extra_authorities: SlotRows<'id, ExtraAuthority>,
    details: SlotRows<'id, DetailRow>,
    objects: SlotRows<'id, ObjectRow>,
    payloads: SlotRows<'id, PayloadRow>,
    definitions: SlotRows<'id, DefinitionRow>,
    references: SlotRows<'id, ReferenceUseRow>,
    proofs: SlotRows<'id, ProofRow>,
    primitives: SlotRows<'id, PrimitiveRow>,
    stages: SlotRows<'id, StageRow>,
    attributes: SlotRows<'id, AttributeRow>,
    sources: SlotRows<'id, SourceSpan>,
    alignments: SlotRows<'id, Alignment>,
    text: SegmentBuilder<'id>,
}
pub(crate) struct DirectBuilder<'id> {
    key: BuilderKey,
    ir: DirectDraft<'id>,
    next_target: u64,
    active_target: Option<(u64, usize)>,
    completed_atoms: usize,
    fixups: BuildRows<'id, FixupSite>,
}
impl<'id> DirectBuilder<'id> {
    pub(crate) fn new(w: &mut OwnedWork<'id, '_>) -> Result<Self, CoreError> {
        w.events(1)?;
        let limits = w
            .model_limits()
            .ok_or_else(|| w.fail(CoreError::UnsupportedConfiguration))?;
        w.scan(8)?;
        w.copy(8)?;
        let key = NEXT_BUILDER
            .try_update(Ordering::Relaxed, Ordering::Relaxed, |n| n.checked_add(1))
            .map_err(|_| w.fail(CoreError::ArithmeticOverflow))?
            + 1;
        Ok(Self {
            key: BuilderKey(key),
            ir: DirectDraft {
                nodes: NodeStorage::new(limits.model_records.min(u32::MAX as u64 - 1), w)?,
                flows: SlotRows::new(
                    limits.model_records.min(u32::MAX as u64 - 1),
                    LimitKind::ModelRecords,
                    w,
                )?,
                packets: SlotRows::new(
                    limits.model_records.min(u32::MAX as u64 - 1),
                    LimitKind::ModelRecords,
                    w,
                )?,
                extra_authorities: SlotRows::new(
                    limits.model_records.min(u32::MAX as u64 - 1),
                    LimitKind::ModelRecords,
                    w,
                )?,
                details: SlotRows::new(
                    limits.model_records.min(u32::MAX as u64 - 1),
                    LimitKind::ModelRecords,
                    w,
                )?,
                objects: SlotRows::new(
                    limits.model_records.min(u32::MAX as u64 - 1),
                    LimitKind::ModelRecords,
                    w,
                )?,
                payloads: SlotRows::new(
                    limits.model_records.min(u32::MAX as u64 - 1),
                    LimitKind::ModelRecords,
                    w,
                )?,
                definitions: SlotRows::new(
                    limits.model_records.min(u32::MAX as u64 - 1),
                    LimitKind::ModelRecords,
                    w,
                )?,
                references: SlotRows::new(
                    limits.model_records.min(u32::MAX as u64 - 1),
                    LimitKind::ModelRecords,
                    w,
                )?,
                proofs: SlotRows::new(
                    limits.model_records.min(u32::MAX as u64 - 1),
                    LimitKind::ModelRecords,
                    w,
                )?,
                primitives: SlotRows::new(
                    limits.proof_records.min(u32::MAX as u64 - 1),
                    LimitKind::ProofRecords,
                    w,
                )?,
                stages: SlotRows::new(
                    limits.proof_records.min(u32::MAX as u64 - 1),
                    LimitKind::ProofRecords,
                    w,
                )?,
                attributes: SlotRows::new(
                    limits.model_records.min(u32::MAX as u64 - 1),
                    LimitKind::ModelRecords,
                    w,
                )?,
                sources: SlotRows::new(
                    limits.source_pieces.min(u32::MAX as u64 - 1),
                    LimitKind::SourcePieces,
                    w,
                )?,
                alignments: SlotRows::new(
                    limits.model_records.min(u32::MAX as u64 - 1),
                    LimitKind::ModelRecords,
                    w,
                )?,
                text: SegmentBuilder::new(w)?,
            },
            next_target: 0,
            active_target: None,
            completed_atoms: 0,
            fixups: BuildRows::new(
                limits.model_records.saturating_add(limits.proof_records),
                LimitKind::AuxiliaryCapacity,
                w,
            )?,
        })
    }
    fn scoped<T: Copy>(&self, value: T) -> DraftRef<T> {
        DraftRef {
            key: self.key,
            value,
        }
    }
    pub(crate) fn binding(&self, w: &mut OwnedWork<'id, '_>) -> Result<BuilderBinding, CoreError> {
        w.events(1)?;
        w.view_copy(size_of::<BuilderBinding>() as u64)?;
        Ok(self.scoped(()))
    }
    pub(crate) fn check_binding(
        &self,
        binding: BuilderBinding,
        w: &mut OwnedWork<'id, '_>,
    ) -> Result<(), CoreError> {
        self.strip(binding, w)
    }
    fn strip<T: Copy>(
        &self,
        value: DraftRef<T>,
        w: &mut OwnedWork<'id, '_>,
    ) -> Result<T, CoreError> {
        w.events(1)?;
        w.scan(size_of::<BuilderKey>() as u64)?;
        if value.key != self.key {
            return Err(w.fail(CoreError::InvalidProvenance));
        }
        Ok(value.value)
    }
    fn reference<T: TableId>(
        &self,
        value: DraftRef<T>,
        written: bool,
        w: &mut OwnedWork<'id, '_>,
    ) -> Result<T, CoreError> {
        let id = self.strip(value, w)?;
        if id.index() >= T::len(self) || (written && !T::written(self, id.index(), w)?) {
            return Err(w.fail(CoreError::InvalidProvenance));
        }
        Ok(id)
    }
    /// Check the actual issuer and existing row, without granting written access.
    pub(crate) fn check_reference<T: TableId>(
        &self,
        reference: DraftRef<T>,
        w: &mut OwnedWork<'id, '_>,
    ) -> Result<(), CoreError> {
        self.reference(reference, false, w).map(|_| ())
    }
    fn optional<T: TableId>(
        &self,
        value: Option<DraftRef<T>>,
        w: &mut OwnedWork<'id, '_>,
    ) -> Result<Option<T>, CoreError> {
        value.map(|v| self.reference(v, false, w)).transpose()
    }
    fn born(&self, value: BornAtom, w: &mut OwnedWork<'id, '_>) -> Result<AtomId, CoreError> {
        w.events(1)?;
        if value.key != self.key
            || self
                .active_target
                .is_none_or(|(target, start)| target != value.target || value.index < start)
            || value.index >= self.ir.packets.len()
        {
            return Err(w.fail(CoreError::InvalidProvenance));
        }
        AtomId::from_index(value.index).map_err(|e| w.fail(e))
    }
    fn atom_ref(&self, value: AtomRef, w: &mut OwnedWork<'id, '_>) -> Result<AtomId, CoreError> {
        match value {
            AtomRef::Born(value) => self.born(value, w),
            #[cfg(test)]
            AtomRef::Final(value) => {
                let id = self.strip(value.0, w)?;
                if id.index() >= self.completed_atoms {
                    return Err(w.fail(CoreError::InvalidProvenance));
                }
                Ok(id)
            }
        }
    }
    fn authority_ref(
        &self,
        value: AuthorityRef,
        written: bool,
        w: &mut OwnedWork<'id, '_>,
    ) -> Result<ClaimId, CoreError> {
        let id = match value {
            AuthorityRef::Born(value) => ClaimId::primary(self.born(value, w)?),
            AuthorityRef::Final(value) => self.strip(value, w)?,
        };
        match id.storage() {
            types::ClaimStorage::Extra(id) => {
                self.ir.extra_authorities.get(id.index(), w)?;
            }
            types::ClaimStorage::Primary(atom) => {
                if matches!(value, AuthorityRef::Final(_)) && atom.index() >= self.completed_atoms {
                    return Err(w.fail(CoreError::InvalidProvenance));
                }
                if written
                    && !matches!(
                        self.ir.packets.get(atom.index(), w)?.authority,
                        AuthorityBinding::Owned(_)
                    )
                {
                    return Err(w.fail(CoreError::InvalidProvenance));
                }
            }
        }
        Ok(id)
    }
    fn source_ref(
        &self,
        value: SourceBlockRef,
        w: &mut OwnedWork<'id, '_>,
    ) -> Result<Rows<SourceSpan>, CoreError> {
        let rows = self.strip(value, w)?;
        rows.indices(self.ir.sources.len()).map_err(|e| w.fail(e))?;
        Ok(rows)
    }
    fn text_ref(&self, value: TextRef, w: &mut OwnedWork<'id, '_>) -> Result<TextSlice, CoreError> {
        let text = self.strip(value, w)?;
        self.ir.text.validate(text, w)?;
        Ok(text)
    }
    fn record_fixup(
        &mut self,
        site: FixupSite,
        w: &mut OwnedWork<'id, '_>,
    ) -> Result<(), CoreError> {
        if self.active_target.is_some() {
            self.fixups.push(site, w)?;
        }
        Ok(())
    }
    pub(crate) fn source_parts(
        &mut self,
        rows: &[SourceSpan],
        w: &mut OwnedWork<'id, '_>,
    ) -> Result<SourceBlockRef, CoreError> {
        let start = self.ir.sources.len();
        for row in rows {
            w.admit_model(LimitKind::SourcePieces, 1)?;
            self.ir.sources.push(*row, w)?;
        }
        let value = Rows::new(start, rows.len()).map_err(|e| w.fail(e))?;
        Ok(self.scoped(value))
    }
    pub(crate) fn source_len(
        &self,
        block: SourceBlockRef,
        work: &mut OwnedWork<'id, '_>,
    ) -> Result<usize, CoreError> {
        Ok(self.source_ref(block, work)?.len())
    }
    pub(crate) fn source_at(
        &self,
        block: SourceBlockRef,
        index: usize,
        w: &mut OwnedWork<'id, '_>,
    ) -> Result<SourceSpan, CoreError> {
        let rows = self.source_ref(block, w)?;
        if index >= rows.len() {
            return Err(w.fail(CoreError::InvalidProvenance));
        }
        self.ir.sources.get(rows.start() + index, w)
    }
    pub(crate) fn text_from_chunks(
        &mut self,
        len: usize,
        w: &mut OwnedWork<'id, '_>,
        chunk: impl FnMut(usize, &mut OwnedWork<'id, '_>) -> Result<BuildTextChunk, CoreError>,
    ) -> Result<TextRef, CoreError> {
        let value = self.ir.text.append_chunks(len, w, chunk)?;
        Ok(self.scoped(value))
    }
    pub(crate) fn text(
        &mut self,
        value: &str,
        w: &mut OwnedWork<'id, '_>,
    ) -> Result<TextRef, CoreError> {
        self.text_from_chunks(value.len(), w, |offset, w| {
            super::segmented_text::read_chunk(value, offset, value.len(), w)
        })
    }
    pub(crate) fn text_chunk(
        &self,
        value: TextRef,
        offset: usize,
        w: &mut OwnedWork<'id, '_>,
    ) -> Result<BuildTextChunk, CoreError> {
        self.ir.text.chunk(self.strip(value, w)?, offset, w)
    }
    pub(crate) fn text_subslice(
        &self,
        value: TextRef,
        start: usize,
        len: usize,
        w: &mut OwnedWork<'id, '_>,
    ) -> Result<TextRef, CoreError> {
        let value = self.text_ref(value, w)?;
        if start.checked_add(len).is_none_or(|end| end > value.len()) {
            return Err(w.fail(CoreError::InvalidProvenance));
        }
        let sub = TextSlice {
            start: u32::try_from(value.start() + start)
                .map_err(|_| w.fail(CoreError::ArithmeticOverflow))?,
            len: u32::try_from(len).map_err(|_| w.fail(CoreError::ArithmeticOverflow))?,
        };
        self.ir.text.validate(sub, w)?;
        Ok(self.scoped(sub))
    }
    pub(crate) fn write_extra_authority(
        &mut self,
        value: DraftAuthoritySpec,
        w: &mut OwnedWork<'id, '_>,
    ) -> Result<AuthorityRef, CoreError> {
        let value = value.lower(self, w).map_err(|e| w.fail(e))?;
        w.admit_model(LimitKind::ModelRecords, 1)?;
        let index = self
            .ir
            .extra_authorities
            .push(ExtraAuthority { value }, w)?;
        Ok(AuthorityRef::Final(self.scoped(ClaimId::extra(
            ExtraAuthorityId::from_index(index).map_err(|e| w.fail(e))?,
        ))))
    }
    pub(crate) fn finish(self, w: &mut OwnedWork<'id, '_>) -> Result<CompactIr, CoreError> {
        w.check()?;
        if self.active_target.is_some() || self.completed_atoms != self.ir.packets.len() {
            return Err(w.fail(CoreError::InvalidProvenance));
        }
        Ok(CompactIr {
            budget: w.frozen_budget(),
            nodes: ModelRows::completed_nodes(self.ir.nodes.freeze(w)?),
            flows: self.ir.flows.freeze(w)?,
            packets: self.ir.packets.freeze(w)?,
            extra_authorities: self.ir.extra_authorities.freeze(w)?,
            details: self.ir.details.freeze(w)?,
            objects: self.ir.objects.freeze(w)?,
            payloads: self.ir.payloads.freeze(w)?,
            definitions: self.ir.definitions.freeze(w)?,
            references: self.ir.references.freeze(w)?,
            proofs: self.ir.proofs.freeze(w)?,
            primitives: self.ir.primitives.freeze(w)?,
            stages: self.ir.stages.freeze(w)?,
            attributes: self.ir.attributes.freeze(w)?,
            sources: self.ir.sources.freeze(w)?,
            alignments: self.ir.alignments.freeze(w)?,
            text: ModelText::segmented(self.ir.text.freeze(w)?),
        })
    }
}
impl TableId for NodeId {
    fn index(self) -> usize {
        self.index()
    }
    fn len(b: &DirectBuilder<'_>) -> usize {
        b.ir.nodes.len()
    }
    fn written<'id>(
        b: &DirectBuilder<'id>,
        index: usize,
        w: &mut OwnedWork<'id, '_>,
    ) -> Result<bool, CoreError> {
        b.ir.nodes.is_written(index, w)
    }
}
impl<'id> DirectBuilder<'id> {
    pub(crate) fn reserve_node(
        &mut self,
        seed: DraftNodeSeed,
        w: &mut OwnedWork<'id, '_>,
    ) -> Result<DraftRef<NodeId>, CoreError> {
        let row = seed.lower(self, w).map_err(|e| w.fail(e))?;
        w.admit_model(LimitKind::ModelRecords, 1)?;
        let index = self.ir.nodes.reserve(row, seed.initial.is_some(), w)?;
        Ok(self.scoped(NodeId::from_index(index).map_err(|e| w.fail(e))?))
    }
    pub(crate) fn graph_node(
        &self,
        reference: NodeRef,
        w: &mut OwnedWork<'id, '_>,
    ) -> Result<GraphNode, CoreError> {
        let id = self.reference(reference, false, w)?;
        let row = self.ir.nodes.graph_row(id.index(), w)?;
        Ok(GraphNode {
            index: id.index(),
            parent: row.parent.map(NodeId::index),
            flow: row.flow.index(),
            kind: row.kind,
            detail: row.detail.map(|detail| self.scoped(detail)),
        })
    }
    pub(crate) fn node_initial(
        &self,
        reference: NodeRef,
        w: &mut OwnedWork<'id, '_>,
    ) -> Result<Option<SourceSpan>, CoreError> {
        let id = self.reference(reference, false, w)?;
        self.ir.nodes.initial(id.index(), w)
    }
    pub(crate) fn set_node_initial(
        &mut self,
        reference: NodeRef,
        initial: Option<SourceSpan>,
        w: &mut OwnedWork<'id, '_>,
    ) -> Result<(), CoreError> {
        let id = self.reference(reference, false, w)?;
        self.ir.nodes.set_initial(id.index(), initial, w)
    }
    pub(crate) fn complete_nodes(&mut self, w: &mut OwnedWork<'id, '_>) -> Result<(), CoreError> {
        self.ir.nodes.complete(w)
    }
    pub(crate) fn write_node(
        &mut self,
        reference: DraftRef<NodeId>,
        row: DraftNodeRow,
        w: &mut OwnedWork<'id, '_>,
    ) -> Result<DraftRef<NodeId>, CoreError> {
        let id = self.reference(reference, false, w)?;
        let row = row.lower(self, w).map_err(|e| w.fail(e))?;
        self.ir.nodes.write(id.index(), row, w)?;
        Ok(reference)
    }
}
impl TableId for FlowId {
    fn index(self) -> usize {
        self.index()
    }
    fn len(b: &DirectBuilder<'_>) -> usize {
        b.ir.flows.len()
    }
    fn written<'id>(
        b: &DirectBuilder<'id>,
        index: usize,
        w: &mut OwnedWork<'id, '_>,
    ) -> Result<bool, CoreError> {
        b.ir.flows.is_written(index, w)
    }
}
impl<'id> DirectBuilder<'id> {
    pub(crate) fn reserve_flow(
        &mut self,
        w: &mut OwnedWork<'id, '_>,
    ) -> Result<DraftRef<FlowId>, CoreError> {
        w.admit_model(LimitKind::ModelRecords, 1)?;
        let index = self.ir.flows.reserve(w)?;
        Ok(self.scoped(FlowId::from_index(index).map_err(|e| w.fail(e))?))
    }
    pub(crate) fn write_flow(
        &mut self,
        reference: DraftRef<FlowId>,
        row: DraftFlowRow,
        w: &mut OwnedWork<'id, '_>,
    ) -> Result<DraftRef<FlowId>, CoreError> {
        let id = self.reference(reference, false, w)?;
        let row = row.lower(self, w).map_err(|e| w.fail(e))?;
        self.ir.flows.write(id.index(), row, w)?;
        Ok(reference)
    }
}
impl TableId for DetailId {
    fn index(self) -> usize {
        self.index()
    }
    fn len(b: &DirectBuilder<'_>) -> usize {
        b.ir.details.len()
    }
    fn written<'id>(
        b: &DirectBuilder<'id>,
        index: usize,
        w: &mut OwnedWork<'id, '_>,
    ) -> Result<bool, CoreError> {
        b.ir.details.is_written(index, w)
    }
}
impl<'id> DirectBuilder<'id> {
    pub(crate) fn reserve_detail(
        &mut self,
        w: &mut OwnedWork<'id, '_>,
    ) -> Result<DraftRef<DetailId>, CoreError> {
        w.admit_model(LimitKind::ModelRecords, 1)?;
        let index = self.ir.details.reserve(w)?;
        Ok(self.scoped(DetailId::from_index(index).map_err(|e| w.fail(e))?))
    }
    pub(crate) fn write_detail(
        &mut self,
        reference: DraftRef<DetailId>,
        row: DraftDetailRow,
        w: &mut OwnedWork<'id, '_>,
    ) -> Result<DraftRef<DetailId>, CoreError> {
        let id = self.reference(reference, false, w)?;
        let row = row.lower(self, w).map_err(|e| w.fail(e))?;
        self.ir.details.write(id.index(), row, w)?;
        self.record_fixup(FixupSite::Detail(id.index()), w)?;
        Ok(reference)
    }
}
impl TableId for ObjectId {
    fn index(self) -> usize {
        self.index()
    }
    fn len(b: &DirectBuilder<'_>) -> usize {
        b.ir.objects.len()
    }
    fn written<'id>(
        b: &DirectBuilder<'id>,
        index: usize,
        w: &mut OwnedWork<'id, '_>,
    ) -> Result<bool, CoreError> {
        b.ir.objects.is_written(index, w)
    }
}
impl<'id> DirectBuilder<'id> {
    pub(crate) fn reserve_object(
        &mut self,
        w: &mut OwnedWork<'id, '_>,
    ) -> Result<DraftRef<ObjectId>, CoreError> {
        w.admit_model(LimitKind::ModelRecords, 1)?;
        let index = self.ir.objects.reserve(w)?;
        Ok(self.scoped(ObjectId::from_index(index).map_err(|e| w.fail(e))?))
    }
    pub(crate) fn write_object(
        &mut self,
        reference: DraftRef<ObjectId>,
        row: DraftObjectRow,
        w: &mut OwnedWork<'id, '_>,
    ) -> Result<DraftRef<ObjectId>, CoreError> {
        let id = self.reference(reference, false, w)?;
        let row = row.lower(self, w).map_err(|e| w.fail(e))?;
        self.ir.objects.write(id.index(), row, w)?;
        self.record_fixup(FixupSite::Object(id.index()), w)?;
        Ok(reference)
    }
}
impl TableId for PayloadId {
    fn index(self) -> usize {
        self.index()
    }
    fn len(b: &DirectBuilder<'_>) -> usize {
        b.ir.payloads.len()
    }
    fn written<'id>(
        b: &DirectBuilder<'id>,
        index: usize,
        w: &mut OwnedWork<'id, '_>,
    ) -> Result<bool, CoreError> {
        b.ir.payloads.is_written(index, w)
    }
}
impl<'id> DirectBuilder<'id> {
    pub(crate) fn reserve_payload(
        &mut self,
        w: &mut OwnedWork<'id, '_>,
    ) -> Result<DraftRef<PayloadId>, CoreError> {
        w.admit_model(LimitKind::ModelRecords, 1)?;
        let index = self.ir.payloads.reserve(w)?;
        Ok(self.scoped(PayloadId::from_index(index).map_err(|e| w.fail(e))?))
    }
    pub(crate) fn write_payload(
        &mut self,
        reference: DraftRef<PayloadId>,
        row: DraftPayloadRow,
        w: &mut OwnedWork<'id, '_>,
    ) -> Result<DraftRef<PayloadId>, CoreError> {
        let id = self.reference(reference, false, w)?;
        let row = row.lower(self, w).map_err(|e| w.fail(e))?;
        self.ir.payloads.write(id.index(), row, w)?;
        Ok(reference)
    }
}
impl TableId for DefinitionId {
    fn index(self) -> usize {
        self.index()
    }
    fn len(b: &DirectBuilder<'_>) -> usize {
        b.ir.definitions.len()
    }
    fn written<'id>(
        b: &DirectBuilder<'id>,
        index: usize,
        w: &mut OwnedWork<'id, '_>,
    ) -> Result<bool, CoreError> {
        b.ir.definitions.is_written(index, w)
    }
}
impl<'id> DirectBuilder<'id> {
    pub(crate) fn reserve_definition(
        &mut self,
        w: &mut OwnedWork<'id, '_>,
    ) -> Result<DraftRef<DefinitionId>, CoreError> {
        w.admit_model(LimitKind::ModelRecords, 1)?;
        let index = self.ir.definitions.reserve(w)?;
        Ok(self.scoped(DefinitionId::from_index(index).map_err(|e| w.fail(e))?))
    }
    pub(crate) fn write_definition(
        &mut self,
        reference: DraftRef<DefinitionId>,
        row: DraftDefinitionRow,
        w: &mut OwnedWork<'id, '_>,
    ) -> Result<DraftRef<DefinitionId>, CoreError> {
        let id = self.reference(reference, false, w)?;
        let row = row.lower(self, w).map_err(|e| w.fail(e))?;
        self.ir.definitions.write(id.index(), row, w)?;
        self.record_fixup(FixupSite::Definition(id.index()), w)?;
        Ok(reference)
    }
}
impl TableId for ReferenceUseId {
    fn index(self) -> usize {
        self.index()
    }
    fn len(b: &DirectBuilder<'_>) -> usize {
        b.ir.references.len()
    }
    fn written<'id>(
        b: &DirectBuilder<'id>,
        index: usize,
        w: &mut OwnedWork<'id, '_>,
    ) -> Result<bool, CoreError> {
        b.ir.references.is_written(index, w)
    }
}
impl<'id> DirectBuilder<'id> {
    pub(crate) fn reserve_reference(
        &mut self,
        w: &mut OwnedWork<'id, '_>,
    ) -> Result<DraftRef<ReferenceUseId>, CoreError> {
        w.admit_model(LimitKind::ModelRecords, 1)?;
        let index = self.ir.references.reserve(w)?;
        Ok(self.scoped(ReferenceUseId::from_index(index).map_err(|e| w.fail(e))?))
    }
    pub(crate) fn write_reference(
        &mut self,
        reference: DraftRef<ReferenceUseId>,
        row: DraftReferenceUseRow,
        w: &mut OwnedWork<'id, '_>,
    ) -> Result<DraftRef<ReferenceUseId>, CoreError> {
        let id = self.reference(reference, false, w)?;
        let row = row.lower(self, w).map_err(|e| w.fail(e))?;
        self.ir.references.write(id.index(), row, w)?;
        self.record_fixup(FixupSite::Reference(id.index()), w)?;
        Ok(reference)
    }
}
impl TableId for ProofId {
    fn index(self) -> usize {
        self.index()
    }
    fn len(b: &DirectBuilder<'_>) -> usize {
        b.ir.proofs.len()
    }
    fn written<'id>(
        b: &DirectBuilder<'id>,
        index: usize,
        w: &mut OwnedWork<'id, '_>,
    ) -> Result<bool, CoreError> {
        b.ir.proofs.is_written(index, w)
    }
}
impl<'id> DirectBuilder<'id> {
    pub(crate) fn reserve_proof(
        &mut self,
        w: &mut OwnedWork<'id, '_>,
    ) -> Result<DraftRef<ProofId>, CoreError> {
        w.admit_model(LimitKind::ModelRecords, 1)?;
        let index = self.ir.proofs.reserve(w)?;
        Ok(self.scoped(ProofId::from_index(index).map_err(|e| w.fail(e))?))
    }
    pub(crate) fn write_proof(
        &mut self,
        reference: DraftRef<ProofId>,
        row: DraftProofRow,
        w: &mut OwnedWork<'id, '_>,
    ) -> Result<DraftRef<ProofId>, CoreError> {
        let id = self.reference(reference, false, w)?;
        let row = row.lower(self, w).map_err(|e| w.fail(e))?;
        self.ir.proofs.write(id.index(), row, w)?;
        self.record_fixup(FixupSite::Proof(id.index()), w)?;
        Ok(reference)
    }
}
impl<'id> DirectBuilder<'id> {
    fn primitive_ref(
        &self,
        reference: DraftRef<Rows<PrimitiveRow>>,
        w: &mut OwnedWork<'id, '_>,
    ) -> Result<Rows<PrimitiveRow>, CoreError> {
        let rows = self.strip(reference, w)?;
        rows.indices(self.ir.primitives.len())
            .map_err(|e| w.fail(e))?;
        Ok(rows)
    }
    pub(crate) fn primitive_parts(
        &mut self,
        rows: &[PrimitiveRow],
        w: &mut OwnedWork<'id, '_>,
    ) -> Result<DraftRef<Rows<PrimitiveRow>>, CoreError> {
        let start = self.ir.primitives.len();
        for row in rows {
            w.admit_model(LimitKind::ProofRecords, 1)?;
            let row = *row;
            let _index = self.ir.primitives.push(row, w)?;
        }
        Ok(self.scoped(Rows::new(start, rows.len()).map_err(|e| w.fail(e))?))
    }
}
impl<'id> DirectBuilder<'id> {
    fn stage_ref(
        &self,
        reference: DraftRef<Rows<StageRow>>,
        w: &mut OwnedWork<'id, '_>,
    ) -> Result<Rows<StageRow>, CoreError> {
        let rows = self.strip(reference, w)?;
        rows.indices(self.ir.stages.len()).map_err(|e| w.fail(e))?;
        Ok(rows)
    }
    pub(crate) fn stage_parts(
        &mut self,
        rows: &[DraftStageRow],
        w: &mut OwnedWork<'id, '_>,
    ) -> Result<DraftRef<Rows<StageRow>>, CoreError> {
        let start = self.ir.stages.len();
        for row in rows {
            w.admit_model(LimitKind::ProofRecords, 1)?;
            let row = (*row).lower(self, w).map_err(|e| w.fail(e))?;
            let _index = self.ir.stages.push(row, w)?;
            self.record_fixup(FixupSite::Stage(_index), w)?;
        }
        Ok(self.scoped(Rows::new(start, rows.len()).map_err(|e| w.fail(e))?))
    }
}
impl<'id> DirectBuilder<'id> {
    pub(crate) fn alignment_parts(
        &mut self,
        rows: &[Alignment],
        work: &mut OwnedWork<'id, '_>,
    ) -> Result<DraftRef<Rows<Alignment>>, CoreError> {
        let start = self.ir.alignments.len();
        for row in rows {
            work.admit_model(LimitKind::ModelRecords, 1)?;
            self.ir.alignments.push(*row, work)?;
        }
        self.written_alignment_block(start, rows.len(), work)
    }

    pub(crate) fn alignments_len(&self) -> usize {
        self.ir.alignments.len()
    }

    pub(crate) fn written_alignment_block(
        &self,
        start: usize,
        count: usize,
        work: &mut OwnedWork<'id, '_>,
    ) -> Result<DraftRef<Rows<Alignment>>, CoreError> {
        work.check()?;
        let rows = Rows::new(start, count).map_err(|e| work.fail(e))?;
        rows.indices(self.ir.alignments.len())
            .map_err(|e| work.fail(e))?;
        Ok(self.scoped(rows))
    }

    fn alignment_ref(
        &self,
        reference: DraftRef<Rows<Alignment>>,
        w: &mut OwnedWork<'id, '_>,
    ) -> Result<Rows<Alignment>, CoreError> {
        let rows = self.strip(reference, w)?;
        rows.indices(self.ir.alignments.len())
            .map_err(|e| w.fail(e))?;
        Ok(rows)
    }
}
impl<'id> DirectBuilder<'id> {
    pub(crate) fn attribute(
        &mut self,
        row: DraftAttributeRow,
        w: &mut OwnedWork<'id, '_>,
    ) -> Result<(), CoreError> {
        let row = row.lower(self, w).map_err(|e| w.fail(e))?;
        w.admit_model(LimitKind::ModelRecords, 1)?;
        let index = self.ir.attributes.push(row, w)?;
        self.record_fixup(FixupSite::Attribute(index), w)
    }
}

impl<'id> DirectBuilder<'id> {
    pub(crate) fn read_node(
        &self,
        reference: DraftRef<NodeId>,
        w: &mut OwnedWork<'id, '_>,
    ) -> Result<NodeRow, CoreError> {
        let id = self.reference(reference, false, w)?;
        self.ir.nodes.get(id.index(), w)
    }
    #[cfg(test)]
    pub(crate) fn read_object(
        &self,
        reference: DraftRef<ObjectId>,
        w: &mut OwnedWork<'id, '_>,
    ) -> Result<ObjectRow, CoreError> {
        let id = self.reference(reference, true, w)?;
        self.ir.objects.get(id.index(), w)
    }
    pub(crate) fn read_detail(
        &self,
        reference: DraftRef<DetailId>,
        w: &mut OwnedWork<'id, '_>,
    ) -> Result<DetailRow, CoreError> {
        let id = self.reference(reference, true, w)?;
        self.ir.details.get(id.index(), w)
    }
    #[cfg(test)]
    pub(crate) fn read_definition(
        &self,
        reference: DraftRef<DefinitionId>,
        w: &mut OwnedWork<'id, '_>,
    ) -> Result<DefinitionRow, CoreError> {
        let id = self.reference(reference, true, w)?;
        self.ir.definitions.get(id.index(), w)
    }
    #[cfg(test)]
    pub(crate) fn read_reference(
        &self,
        reference: DraftRef<ReferenceUseId>,
        w: &mut OwnedWork<'id, '_>,
    ) -> Result<ReferenceUseRow, CoreError> {
        let id = self.reference(reference, true, w)?;
        self.ir.references.get(id.index(), w)
    }
    #[cfg(test)]
    pub(crate) fn read_proof(
        &self,
        reference: DraftRef<ProofId>,
        w: &mut OwnedWork<'id, '_>,
    ) -> Result<ProofRow, CoreError> {
        let id = self.reference(reference, true, w)?;
        self.ir.proofs.get(id.index(), w)
    }
    pub(crate) fn sources_len(&self) -> usize {
        self.ir.sources.len()
    }
    pub(crate) fn written_source_block(
        &self,
        start: usize,
        len: usize,
        w: &mut OwnedWork<'id, '_>,
    ) -> Result<DraftRef<Rows<SourceSpan>>, CoreError> {
        w.check()?;
        let rows = Rows::new(start, len).map_err(|e| w.fail(e))?;
        rows.indices(self.ir.sources.len()).map_err(|e| w.fail(e))?;
        Ok(self.scoped(rows))
    }
    pub(crate) fn primitives_len(&self) -> usize {
        self.ir.primitives.len()
    }
    pub(crate) fn written_primitive_block(
        &self,
        start: usize,
        len: usize,
        w: &mut OwnedWork<'id, '_>,
    ) -> Result<DraftRef<Rows<PrimitiveRow>>, CoreError> {
        w.check()?;
        let rows = Rows::new(start, len).map_err(|e| w.fail(e))?;
        rows.indices(self.ir.primitives.len())
            .map_err(|e| w.fail(e))?;
        Ok(self.scoped(rows))
    }
    pub(crate) fn stages_len(&self) -> usize {
        self.ir.stages.len()
    }
    pub(crate) fn written_stage_block(
        &self,
        start: usize,
        len: usize,
        w: &mut OwnedWork<'id, '_>,
    ) -> Result<DraftRef<Rows<StageRow>>, CoreError> {
        w.check()?;
        let rows = Rows::new(start, len).map_err(|e| w.fail(e))?;
        rows.indices(self.ir.stages.len()).map_err(|e| w.fail(e))?;
        Ok(self.scoped(rows))
    }
}

#[cfg(test)]
mod tests;
