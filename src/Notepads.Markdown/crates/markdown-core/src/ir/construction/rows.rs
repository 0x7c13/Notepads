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
        b: &ExactBuilder<'id>,
        op: &mut OwnedWork<'id, '_>,
    ) -> Result<Self::Value, CoreError>;
}

impl Lower for DraftNodeRow {
    type Value = NodeRow;
    fn lower<'id>(
        self,
        b: &ExactBuilder<'id>,
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
        _b: &ExactBuilder<'id>,
        op: &mut OwnedWork<'id, '_>,
    ) -> Result<FlowScope, CoreError> {
        op.view_scan(size_of::<Self>() as u64)?;
        Ok(match self {
            Self::Main => FlowScope::Main,
        })
    }
}

impl Lower for DraftFlowRow {
    type Value = FlowRow;
    fn lower<'id>(
        self,
        b: &ExactBuilder<'id>,
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
        b: &ExactBuilder<'id>,
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
        b: &ExactBuilder<'id>,
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

impl Lower for DraftHostRow {
    type Value = HostRow;
    fn lower<'id>(
        self,
        b: &ExactBuilder<'id>,
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
        b: &ExactBuilder<'id>,
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
        b: &ExactBuilder<'id>,
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
        b: &ExactBuilder<'id>,
        op: &mut OwnedWork<'id, '_>,
    ) -> Result<ProofRow, CoreError> {
        op.view_scan(size_of::<Self>() as u64)?;
        Ok(ProofRow {
            atom: b.reference(self.atom, false, op)?,
            primitives: b.primitive_ref(self.primitives, op)?,
            stages: b.stage_ref(self.stages, op)?,
            value: b.text_ref(self.value, op)?,
        })
    }
}
