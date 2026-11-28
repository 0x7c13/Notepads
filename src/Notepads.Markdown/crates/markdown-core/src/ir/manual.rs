// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE file in the project root for license information.
// ---------------------------------------------------------------------------------------------

//! Test-only hand-written unaccepted drafts. Never used by the native producer.

use super::*;
use crate::error::LimitKind;
use crate::memory::{GrowingText, GrowingVec};

/// Independent mutable test fixture, never an accepted or production IR.
pub(crate) struct ManualIr {
    pub(crate) budget: SourceBudget,
    pub(crate) nodes: GrowingVec<NodeRow>,
    pub(crate) flows: GrowingVec<FlowRow>,
    pub(crate) packets: GrowingVec<AtomPacket>,
    pub(crate) extra_authorities: GrowingVec<ExtraAuthority>,
    pub(crate) details: GrowingVec<DetailRow>,
    pub(crate) objects: GrowingVec<ObjectRow>,
    pub(crate) payloads: GrowingVec<PayloadRow>,
    pub(crate) definitions: GrowingVec<DefinitionRow>,
    pub(crate) references: GrowingVec<ReferenceUseRow>,
    pub(crate) proofs: GrowingVec<ProofRow>,
    pub(crate) primitives: GrowingVec<PrimitiveRow>,
    pub(crate) stages: GrowingVec<StageRow>,
    pub(crate) attributes: GrowingVec<AttributeRow>,
    pub(crate) sources: GrowingVec<SourceSpan>,
    pub(crate) alignments: GrowingVec<Alignment>,
    pub(crate) text: GrowingText,
}
impl ManualIr {
    pub(crate) fn nodes(&self) -> &[NodeRow] {
        self.nodes.as_slice()
    }
    pub(crate) fn flows(&self) -> &[FlowRow] {
        self.flows.as_slice()
    }
    pub(crate) fn details(&self) -> &[DetailRow] {
        self.details.as_slice()
    }
    pub(crate) fn objects(&self) -> &[ObjectRow] {
        self.objects.as_slice()
    }
    pub(crate) fn payloads(&self) -> &[PayloadRow] {
        self.payloads.as_slice()
    }
    pub(crate) fn definitions(&self) -> &[DefinitionRow] {
        self.definitions.as_slice()
    }
    pub(crate) fn references(&self) -> &[ReferenceUseRow] {
        self.references.as_slice()
    }
    pub(crate) fn proofs(&self) -> &[ProofRow] {
        self.proofs.as_slice()
    }
    pub(crate) fn primitives(&self) -> &[PrimitiveRow] {
        self.primitives.as_slice()
    }
    pub(crate) fn stages(&self) -> &[StageRow] {
        self.stages.as_slice()
    }
    pub(crate) fn sources(&self) -> &[SourceSpan] {
        self.sources.as_slice()
    }
    pub(crate) fn atoms_len(&self) -> usize {
        self.packets.len()
    }
    pub(crate) fn text(&self, value: TextSlice) -> Result<&str, CoreError> {
        self.text
            .as_str()
            .get(
                value.start()
                    ..value
                        .start()
                        .checked_add(value.len())
                        .ok_or(CoreError::ArithmeticOverflow)?,
            )
            .ok_or(CoreError::InvalidProvenance)
    }
    pub(crate) fn snapshot(&self, op: &mut Operation) -> Result<CompactIr, CoreError> {
        op.ensure_budget(&self.budget)?;
        crate::policy::construction_scope(op, |mut work| {
            fn rows<'id, T: Copy>(
                values: &[T],
                work: &mut OwnedWork<'id, '_>,
            ) -> Result<ModelRows<T>, CoreError> {
                let mut copy = super::direct_rows::SlotRows::new(
                    values.len() as u64,
                    LimitKind::ModelRecords,
                    work,
                )?;
                for value in values {
                    copy.push(*value, work)?;
                }
                copy.freeze(work)
            }
            let mut text = crate::memory::BuildExactText::new(self.text.len(), &mut work)?;
            text.append(self.text.as_str(), &mut work)?;
            // Copy raw bad edges and rows verbatim; do not call ExactBuilder or validator.
            let ir = CompactIr {
                budget: work.frozen_budget(),
                nodes: rows(self.nodes.as_slice(), &mut work)?,
                flows: rows(self.flows.as_slice(), &mut work)?,
                packets: rows(self.packets.as_slice(), &mut work)?,
                extra_authorities: rows(self.extra_authorities.as_slice(), &mut work)?,
                details: rows(self.details.as_slice(), &mut work)?,
                objects: rows(self.objects.as_slice(), &mut work)?,
                payloads: rows(self.payloads.as_slice(), &mut work)?,
                definitions: rows(self.definitions.as_slice(), &mut work)?,
                references: rows(self.references.as_slice(), &mut work)?,
                proofs: rows(self.proofs.as_slice(), &mut work)?,
                primitives: rows(self.primitives.as_slice(), &mut work)?,
                stages: rows(self.stages.as_slice(), &mut work)?,
                attributes: rows(self.attributes.as_slice(), &mut work)?,
                sources: rows(self.sources.as_slice(), &mut work)?,
                alignments: rows(self.alignments.as_slice(), &mut work)?,
                text: ModelText::segmented(super::segmented_text::SegmentedText::from_frozen(
                    text.freeze(&mut work)?,
                    &mut work,
                )?),
            };
            work.finish()?;
            Ok(ir)
        })
    }
}

pub(crate) struct IrBuilder {
    pub(crate) ir: ManualIr,
}

macro_rules! append {
    ($($method:ident, $table:ident, $row:ty, $id:ty, $limit:ident);+ $(;)?) => {$(
        pub(crate) fn $method(&mut self, row: $row, op: &mut Operation)
            -> Result<$id, CoreError> {
            op.admit_model(LimitKind::$limit, 1)?;
            let id = <$id>::from_index(self.ir.$table.len()).map_err(|e| op.fail(e))?;
            self.ir.$table.push(row, op)?;
            Ok(id)
        }
    )+};
}

impl IrBuilder {
    pub(crate) fn unbound_authority() -> ClaimId {
        ClaimId::extra(ExtraAuthorityId::from_index(0).unwrap())
    }
    pub(crate) fn new(op: &mut Operation) -> Result<Self, CoreError> {
        let limits = op
            .budget()
            .model_limits()
            .copied()
            .ok_or_else(|| op.fail(CoreError::UnsupportedConfiguration))?;
        macro_rules! table {
            ($count:expr,$kind:ident) => {
                GrowingVec::new($count, LimitKind::$kind, op)?
            };
        }
        let m = limits.model_records;
        let ir = ManualIr {
            budget: op.budget().clone(),
            nodes: table!(m, ModelRecords),
            flows: table!(m, ModelRecords),
            packets: table!(m, ModelRecords),
            extra_authorities: table!(m, ModelRecords),
            details: table!(m, ModelRecords),
            objects: table!(m, ModelRecords),
            payloads: table!(m, ModelRecords),
            definitions: table!(m, ModelRecords),
            references: table!(m, ModelRecords),
            proofs: table!(m, ModelRecords),
            attributes: table!(m, ModelRecords),
            primitives: table!(limits.proof_records, ProofRecords),
            stages: table!(limits.proof_records, ProofRecords),
            sources: table!(limits.source_pieces, SourcePieces),
            alignments: table!(m, ModelRecords),
            text: GrowingText::new(limits.decoded_bytes, op)?,
        };
        Ok(Self { ir })
    }

    append!(node, nodes, NodeRow, NodeId, ModelRecords;
        flow, flows, FlowRow, FlowId, ModelRecords;
        detail, details, DetailRow, DetailId, ModelRecords;
        object, objects, ObjectRow, ObjectId, ModelRecords;
        payload, payloads, PayloadRow, PayloadId, ModelRecords;
        definition, definitions, DefinitionRow, DefinitionId, ModelRecords;
        reference, references, ReferenceUseRow, ReferenceUseId, ModelRecords;
        proof, proofs, ProofRow, ProofId, ModelRecords;
        attribute, attributes, AttributeRow, AttributeId, ModelRecords);

    #[cfg(test)]
    pub(crate) fn source_parts(
        &mut self,
        sources: &[SourceSpan],
        op: &mut Operation,
    ) -> Result<Rows<SourceSpan>, CoreError> {
        op.admit_model(LimitKind::SourcePieces, sources.len() as u64)?;
        let range = Rows::new(self.ir.sources.len(), sources.len()).map_err(|e| op.fail(e))?;
        for row in sources {
            self.ir.sources.push(*row, op)?;
        }
        Ok(range)
    }

    pub(crate) fn primitive_parts(
        &mut self,
        rows: &[PrimitiveRow],
        op: &mut Operation,
    ) -> Result<Rows<PrimitiveRow>, CoreError> {
        op.admit_model(LimitKind::ProofRecords, rows.len() as u64)?;
        let range = Rows::new(self.ir.primitives.len(), rows.len()).map_err(|e| op.fail(e))?;
        for row in rows {
            self.ir.primitives.push(*row, op)?;
        }
        Ok(range)
    }

    pub(crate) fn stage_parts(
        &mut self,
        rows: &[StageRow],
        op: &mut Operation,
    ) -> Result<Rows<StageRow>, CoreError> {
        op.admit_model(LimitKind::ProofRecords, rows.len() as u64)?;
        let range = Rows::new(self.ir.stages.len(), rows.len()).map_err(|e| op.fail(e))?;
        for row in rows {
            self.ir.stages.push(*row, op)?;
        }
        Ok(range)
    }

    #[cfg(test)]
    pub(crate) fn alignment_parts(
        &mut self,
        rows: &[Alignment],
        op: &mut Operation,
    ) -> Result<Rows<Alignment>, CoreError> {
        op.admit_model(LimitKind::ModelRecords, rows.len() as u64)?;
        let range = Rows::new(self.ir.alignments.len(), rows.len()).map_err(|e| op.fail(e))?;
        for row in rows {
            self.ir.alignments.push(*row, op)?;
        }
        Ok(range)
    }

    pub(crate) fn text(&mut self, text: &str, op: &mut Operation) -> Result<TextSlice, CoreError> {
        // The native output meter already charges native bytes; this separate
        // pool bounds retained model bytes and shares the same capacity root.
        let start = u32::try_from(self.ir.text.len())
            .map_err(|_| op.fail(CoreError::ArithmeticOverflow))?;
        let len = u32::try_from(text.len()).map_err(|_| op.fail(CoreError::ArithmeticOverflow))?;
        start
            .checked_add(len)
            .ok_or_else(|| op.fail(CoreError::ArithmeticOverflow))?;
        self.ir.text.append(text, op)?;
        Ok(TextSlice { start, len })
    }
}

impl IrBuilder {
    pub(crate) fn claim(
        &mut self,
        value: AuthoritySpec,
        op: &mut Operation,
    ) -> Result<ClaimId, CoreError> {
        let id = ExtraAuthorityId::from_index(self.ir.extra_authorities.len())
            .map_err(|e| op.fail(e))?;
        op.admit_model(LimitKind::ModelRecords, 1)?;
        self.ir
            .extra_authorities
            .push(ExtraAuthority { value }, op)?;
        Ok(ClaimId::extra(id))
    }
    pub(crate) fn atom(&mut self, row: AtomRow, op: &mut Operation) -> Result<AtomId, CoreError> {
        let id = AtomId::from_index(self.ir.packets.len()).map_err(|e| op.fail(e))?;
        op.admit_model(LimitKind::ModelRecords, 1)?;
        self.ir.packets.push(packet(row), op)?;
        Ok(id)
    }
}
fn packet(row: AtomRow) -> AtomPacket {
    AtomPacket {
        atom: AtomFields {
            owner: row.owner,
            kind: row.kind,
            target: row.target,
            sources: row.sources,
            proof: row.proof,
            content: row.content,
        },
        authority: AuthorityBinding::Shared(row.claim),
    }
}
impl ManualIr {
    pub(crate) fn test_atom(&self, index: usize) -> AtomRow {
        let row = self.packets.as_slice()[index];
        let claim = match row.authority {
            AuthorityBinding::Owned(_) => ClaimId::primary(AtomId::from_index(index).unwrap()),
            AuthorityBinding::Shared(id) => id,
        };
        AtomRow {
            owner: row.atom.owner,
            kind: row.atom.kind,
            target: row.atom.target,
            sources: row.atom.sources,
            claim,
            proof: row.atom.proof,
            content: row.atom.content,
        }
    }
    pub(crate) fn set_test_atom(
        &mut self,
        index: usize,
        row: AtomRow,
        op: &mut Operation,
    ) -> Result<(), CoreError> {
        self.packets.replace(index, packet(row), op)
    }
    pub(crate) fn test_authority_at(&self, index: usize) -> AuthoritySpec {
        self.extra_authorities.as_slice()[index].value
    }
    pub(crate) fn test_authority(&self, id: ClaimId) -> AuthoritySpec {
        let types::ClaimStorage::Extra(id) = id.storage() else {
            panic!("manual authority must be an extra")
        };
        self.test_authority_at(id.index())
    }
    pub(crate) fn set_test_authority(
        &mut self,
        id: ClaimId,
        value: AuthoritySpec,
        op: &mut Operation,
    ) -> Result<(), CoreError> {
        let types::ClaimStorage::Extra(id) = id.storage() else {
            return Err(op.fail(CoreError::InvalidProvenance));
        };
        self.extra_authorities
            .replace(id.index(), ExtraAuthority { value }, op)
    }
    pub(crate) fn set_test_authority_at(
        &mut self,
        index: usize,
        value: AuthoritySpec,
        op: &mut Operation,
    ) -> Result<(), CoreError> {
        self.extra_authorities
            .replace(index, ExtraAuthority { value }, op)
    }
}
