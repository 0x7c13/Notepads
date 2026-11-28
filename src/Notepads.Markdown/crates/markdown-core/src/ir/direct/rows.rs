// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE file in the project root for license information.
// ---------------------------------------------------------------------------------------------

//! Every construction field is checked before its scope is stripped.

use super::*;
pub(super) trait Lower {
    type Value;
    fn lower<'id>(
        self,
        b: &DirectBuilder<'id>,
        op: &mut OwnedWork<'id, '_>,
    ) -> Result<Self::Value, CoreError>;
}

impl Lower for DraftNodeSeed {
    type Value = NodeRow;
    fn lower<'id>(
        self,
        b: &DirectBuilder<'id>,
        op: &mut OwnedWork<'id, '_>,
    ) -> Result<NodeRow, CoreError> {
        op.view_scan(size_of::<Self>() as u64)?;
        Ok(NodeRow {
            parent: b.optional(self.parent, op)?,
            first_child: None,
            next_sibling: None,
            flow: b.reference(self.flow, false, op)?,
            kind: self.kind,
            detail: b.optional(self.detail, op)?,
            source_envelope: self.initial.unwrap_or(SourceSpan::new(0, 0)?),
            logical: crate::identity::LogicalSpan::new(0, 0)?,
        })
    }
}

impl Lower for DraftNodeRow {
    type Value = NodeRow;
    fn lower<'id>(
        self,
        b: &DirectBuilder<'id>,
        op: &mut OwnedWork<'id, '_>,
    ) -> Result<NodeRow, CoreError> {
        op.view_scan(size_of::<Self>() as u64)?;
        Ok(NodeRow {
            parent: b.optional(self.parent, op)?,
            first_child: b.optional(self.first_child, op)?,
            next_sibling: b.optional(self.next_sibling, op)?,
            flow: b.reference(self.flow, false, op)?,
            kind: self.kind,
            detail: b.optional(self.detail, op)?,
            source_envelope: self.source_envelope,
            logical: self.logical,
        })
    }
}

impl Lower for DraftFlowScope {
    type Value = FlowScope;
    fn lower<'id>(
        self,
        b: &DirectBuilder<'id>,
        op: &mut OwnedWork<'id, '_>,
    ) -> Result<FlowScope, CoreError> {
        op.view_scan(size_of::<Self>() as u64)?;
        Ok(match self {
            Self::Main => FlowScope::Main,
            Self::ImageAlt { object } => FlowScope::ImageAlt {
                object: b.reference(object, false, op)?,
            },
        })
    }
}

impl Lower for DraftFlowRow {
    type Value = FlowRow;
    fn lower<'id>(
        self,
        b: &DirectBuilder<'id>,
        op: &mut OwnedWork<'id, '_>,
    ) -> Result<FlowRow, CoreError> {
        op.view_scan((size_of::<Self>() - size_of::<DraftFlowScope>()) as u64)?;
        Ok(FlowRow {
            scope: self.scope.lower(b, op)?,
            root: b.reference(self.root, false, op)?,
            domains: b.source_ref(self.domains, op)?,
            atoms: self.atoms.lower(b, op)?,
            logical_utf16: self.logical_utf16,
        })
    }
}

impl Lower for DraftAtomContent {
    type Value = AtomContent;
    fn lower<'id>(
        self,
        b: &DirectBuilder<'id>,
        op: &mut OwnedWork<'id, '_>,
    ) -> Result<AtomContent, CoreError> {
        op.view_scan(size_of::<Self>() as u64)?;
        Ok(match self {
            Self::Empty => AtomContent::Empty,
            Self::Pool(value) => AtomContent::Pool(b.text_ref(value, op)?),
            Self::Object(value) => AtomContent::Object(b.reference(value, false, op)?),
        })
    }
}

impl Lower for DraftAtomFields {
    type Value = AtomFields;
    fn lower<'id>(
        self,
        b: &DirectBuilder<'id>,
        op: &mut OwnedWork<'id, '_>,
    ) -> Result<AtomFields, CoreError> {
        op.view_scan((size_of::<Self>() - size_of::<DraftAtomContent>()) as u64)?;
        Ok(AtomFields {
            owner: b.reference(self.owner, false, op)?,
            kind: self.kind,
            target: self.target,
            sources: b.source_ref(self.sources, op)?,
            proof: b.optional(self.proof, op)?,
            content: self.content.lower(b, op)?,
        })
    }
}

impl Lower for DraftDetailRow {
    type Value = DetailRow;
    fn lower<'id>(
        self,
        b: &DirectBuilder<'id>,
        op: &mut OwnedWork<'id, '_>,
    ) -> Result<DetailRow, CoreError> {
        op.view_scan(size_of::<Self>() as u64)?;
        Ok(match self {
            Self::Heading { level, form, claim } => DetailRow::Heading {
                level,
                form,
                claim: b.authority_ref(claim, false, op)?,
            },
            Self::List {
                marker,
                start,
                tight,
                claim,
            } => DetailRow::List {
                marker,
                start,
                tight,
                claim: b.authority_ref(claim, false, op)?,
            },
            Self::Item {
                occurrence,
                marker,
                value,
                claim,
            } => DetailRow::Item {
                occurrence,
                marker,
                value,
                claim: b.authority_ref(claim, false, op)?,
            },
            Self::DescriptionItem { tight, claim } => DetailRow::DescriptionItem {
                tight,
                claim: b.authority_ref(claim, false, op)?,
            },
            Self::Code { form, info, claim } => DetailRow::Code {
                form,
                info: b.optional(info, op)?,
                claim: b.authority_ref(claim, false, op)?,
            },
            Self::Html { claim } => DetailRow::Html {
                claim: b.authority_ref(claim, false, op)?,
            },
            Self::Table {
                columns,
                alignments,
                claim,
            } => DetailRow::Table {
                columns,
                alignments: b.alignment_ref(alignments, op)?,
                claim: b.authority_ref(claim, false, op)?,
            },
            Self::Row {
                index,
                header,
                claim,
            } => DetailRow::Row {
                index,
                header,
                claim: b.authority_ref(claim, false, op)?,
            },
            Self::Cell {
                column,
                missing,
                claim,
            } => DetailRow::Cell {
                column,
                missing,
                claim: b.authority_ref(claim, false, op)?,
            },
            Self::Link {
                destination,
                title,
                reference,
                claim,
            } => DetailRow::Link {
                destination: b.reference(destination, false, op)?,
                title: b.optional(title, op)?,
                reference: b.optional(reference, op)?,
                claim: b.authority_ref(claim, false, op)?,
            },
            Self::FootnoteDefinition { ordinal, claim } => DetailRow::FootnoteDefinition {
                ordinal,
                claim: b.authority_ref(claim, false, op)?,
            },
            Self::FootnoteReference {
                definition,
                ordinal,
                occurrence,
                claim,
                closing,
            } => DetailRow::FootnoteReference {
                definition: b.reference(definition, false, op)?,
                ordinal,
                occurrence,
                claim: b.authority_ref(claim, false, op)?,
                closing: closing
                    .map(|claim| b.authority_ref(claim, false, op))
                    .transpose()?,
            },
            Self::FootnoteBacklink {
                reference,
                ordinal,
                occurrence,
                claim,
            } => DetailRow::FootnoteBacklink {
                reference: b.reference(reference, false, op)?,
                ordinal,
                occurrence,
                claim: b.authority_ref(claim, false, op)?,
            },
            Self::TableOfContentsEntry {
                heading,
                depth,
                claim,
            } => DetailRow::TableOfContentsEntry {
                heading: b.reference(heading, false, op)?,
                depth,
                claim: b.authority_ref(claim, false, op)?,
            },
            Self::Callout {
                kind,
                disclosure,
                title,
                claim,
            } => DetailRow::Callout {
                kind,
                disclosure,
                title: b.reference(title, false, op)?,
                claim: b.authority_ref(claim, false, op)?,
            },
        })
    }
}

impl Lower for DraftObjectRow {
    type Value = ObjectRow;
    fn lower<'id>(
        self,
        b: &DirectBuilder<'id>,
        op: &mut OwnedWork<'id, '_>,
    ) -> Result<ObjectRow, CoreError> {
        op.view_scan(size_of::<Self>() as u64)?;
        Ok(match self {
            Self::Derived {
                node,
                atom,
                kind,
                input,
                export_sources,
            } => ObjectRow::Derived {
                node: b.reference(node, false, op)?,
                atom: b.atom_ref(atom, op)?,
                kind,
                input: b.reference(input, false, op)?,
                export_sources: b.source_ref(export_sources, op)?,
            },
            Self::Image {
                node,
                atom,
                alt,
                destination,
                title,
                reference,
                wiki_size,
            } => ObjectRow::Image {
                node: b.reference(node, false, op)?,
                atom: b.atom_ref(atom, op)?,
                alt: b.reference(alt, false, op)?,
                destination: b.reference(destination, false, op)?,
                title: b.optional(title, op)?,
                reference: b.optional(reference, op)?,
                wiki_size,
            },
            Self::Task {
                node,
                atom,
                item,
                checked,
                marker,
                spacer,
                action,
            } => ObjectRow::Task {
                node: b.reference(node, false, op)?,
                atom: b.atom_ref(atom, op)?,
                item: b.reference(item, false, op)?,
                checked,
                marker: b.source_ref(marker, op)?,
                spacer: b.source_ref(spacer, op)?,
                action,
            },
        })
    }
}

impl Lower for DraftPayloadOwner {
    type Value = PayloadOwner;
    fn lower<'id>(
        self,
        b: &DirectBuilder<'id>,
        op: &mut OwnedWork<'id, '_>,
    ) -> Result<PayloadOwner, CoreError> {
        op.view_scan(size_of::<Self>() as u64)?;
        Ok(match self {
            Self::Node(value) => PayloadOwner::Node(b.reference(value, false, op)?),
            Self::Definition(value) => PayloadOwner::Definition(b.reference(value, false, op)?),
        })
    }
}

impl Lower for DraftPayloadRow {
    type Value = PayloadRow;
    fn lower<'id>(
        self,
        b: &DirectBuilder<'id>,
        op: &mut OwnedWork<'id, '_>,
    ) -> Result<PayloadRow, CoreError> {
        op.view_scan(size_of::<Self>() as u64)?;
        Ok(PayloadRow {
            owner: self.owner.lower(b, op)?,
            role: self.role,
            value: b.text_ref(self.value, op)?,
            domains: b.source_ref(self.domains, op)?,
            atoms: self.atoms.lower(b, op)?,
            utf8_len: self.utf8_len,
        })
    }
}

impl Lower for DraftDefinitionRow {
    type Value = DefinitionRow;
    fn lower<'id>(
        self,
        b: &DirectBuilder<'id>,
        op: &mut OwnedWork<'id, '_>,
    ) -> Result<DefinitionRow, CoreError> {
        op.view_scan(size_of::<Self>() as u64)?;
        Ok(DefinitionRow {
            syntax: b.source_ref(self.syntax, op)?,
            label: b.reference(self.label, false, op)?,
            destination: b.reference(self.destination, false, op)?,
            title: b.optional(self.title, op)?,
            key: b.text_ref(self.key, op)?,
            winner: self.winner,
            claim: b.authority_ref(self.claim, false, op)?,
        })
    }
}

impl Lower for DraftReferenceUseRow {
    type Value = ReferenceUseRow;
    fn lower<'id>(
        self,
        b: &DirectBuilder<'id>,
        op: &mut OwnedWork<'id, '_>,
    ) -> Result<ReferenceUseRow, CoreError> {
        op.view_scan(size_of::<Self>() as u64)?;
        Ok(ReferenceUseRow {
            node: b.reference(self.node, false, op)?,
            syntax: b.source_ref(self.syntax, op)?,
            winner: b.reference(self.winner, false, op)?,
            key: b.text_ref(self.key, op)?,
            destination: b.reference(self.destination, false, op)?,
            title: b.optional(self.title, op)?,
            claim: b.authority_ref(self.claim, false, op)?,
        })
    }
}

impl Lower for DraftHostRow {
    type Value = HostRow;
    fn lower<'id>(
        self,
        b: &DirectBuilder<'id>,
        op: &mut OwnedWork<'id, '_>,
    ) -> Result<HostRow, CoreError> {
        op.view_scan(size_of::<Self>() as u64)?;
        Ok(HostRow {
            owner: b.reference(self.owner, false, op)?,
            role: self.role,
            utf8_len: self.utf8_len,
            domains: b.source_ref(self.domains, op)?,
        })
    }
}

impl Lower for DraftAuthoritySpec {
    type Value = AuthoritySpec;
    fn lower<'id>(
        self,
        b: &DirectBuilder<'id>,
        op: &mut OwnedWork<'id, '_>,
    ) -> Result<AuthoritySpec, CoreError> {
        op.view_scan((size_of::<Self>() - size_of::<DraftHostRow>()) as u64)?;
        Ok(AuthoritySpec {
            owner: b.reference(self.owner, false, op)?,
            host: self.host.lower(b, op)?,
            phase: self.phase,
            kind: self.kind,
            consumed: self.consumed,
            sources: b.source_ref(self.sources, op)?,
        })
    }
}

impl Lower for DraftStageRow {
    type Value = StageRow;
    fn lower<'id>(
        self,
        b: &DirectBuilder<'id>,
        op: &mut OwnedWork<'id, '_>,
    ) -> Result<StageRow, CoreError> {
        op.view_scan(size_of::<Self>() as u64)?;
        Ok(StageRow {
            kind: self.kind,
            input: self.input,
            output: b.text_ref(self.output, op)?,
            parameter: self.parameter,
            accepted_by: b.authority_ref(self.accepted_by, false, op)?,
        })
    }
}

impl Lower for DraftProofRow {
    type Value = ProofRow;
    fn lower<'id>(
        self,
        b: &DirectBuilder<'id>,
        op: &mut OwnedWork<'id, '_>,
    ) -> Result<ProofRow, CoreError> {
        op.view_scan(size_of::<Self>() as u64)?;
        Ok(ProofRow {
            atom: b.atom_ref(self.atom, op)?,
            primitives: b.primitive_ref(self.primitives, op)?,
            stages: b.stage_ref(self.stages, op)?,
            value: b.text_ref(self.value, op)?,
        })
    }
}

impl Lower for DraftAttributeRow {
    type Value = AttributeRow;
    fn lower<'id>(
        self,
        b: &DirectBuilder<'id>,
        op: &mut OwnedWork<'id, '_>,
    ) -> Result<AttributeRow, CoreError> {
        op.view_scan(size_of::<Self>() as u64)?;
        Ok(AttributeRow {
            owner: b.reference(self.owner, false, op)?,
            kind: self.kind,
            claim: b.authority_ref(self.claim, false, op)?,
            sources: b.source_ref(self.sources, op)?,
        })
    }
}
