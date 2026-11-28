// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE file in the project root for license information.
// ---------------------------------------------------------------------------------------------

//! Object syntax and raw export are checked independently of engine rendering.

use super::*;

#[allow(clippy::too_many_arguments)]
pub(super) fn validate<'id>(
    ir: &CompactIr,
    source: &SourceSnapshot,
    node: NodeId,
    atom: AtomRow,
    kind: DerivedKind,
    input: PayloadId,
    export_sources: Rows<SourceSpan>,
    op: &mut OwnedWork<'id, '_>,
) -> Result<(), CoreError> {
    let fence = match kind {
        DerivedKind::Diagram => Some((NodeKind::Diagram, "mermaid")),
        DerivedKind::Math(MathSyntax::FencedDisplay) => Some((NodeKind::MathBlock, "math")),
        _ => None,
    };
    if let Some(fence) = fence {
        return validate_fence(ir, source, node, atom, input, export_sources, fence, op);
    }
    if kind == DerivedKind::NoteEmbed {
        let row = ir.nodes()[node.index()];
        let claim = ir.owned_read_authority(atom.claim, op)?;
        require(
            row.kind == NodeKind::NoteEmbed
                && row.first_child.is_none()
                && claim.owner == node
                && claim.kind == ClaimKind::NoteEmbed,
        )?;
        let syntax = export_sources.slice(ir.sources())?;
        require(
            syntax == atom.sources.slice(ir.sources())?
                && syntax == claim.sources.slice(ir.sources())?,
        )?;
        payload_owner(
            ir,
            input,
            PayloadOwner::Node(node),
            PayloadRole::Destination,
        )?;
        let target = ir.text_owned(ir.payloads()[input.index()].value, op)?;
        op.scan(target.len() as u64)?;
        require(
            comrak::native::wiki_note_target(target)
                && wiki_alias(ir, source, input, export_sources, "![[", op)?.is_none(),
        )?;
        return Ok(());
    }
    let DerivedKind::Math(syntax) = kind else {
        return Err(CoreError::UnsupportedConfiguration);
    };
    require(ir.nodes()[node.index()].kind == NodeKind::Math)?;
    let claim = ir.owned_read_authority(atom.claim, op)?;
    require(
        claim.owner == node
            && claim.kind == ClaimKind::Math(syntax)
            && claim.phase == NativePhase::Inline
            && claim.host.role == HostRole::MathInput,
    )?;
    let exported = parts(ir, source, export_sources, op)?;
    let syntax_parts = parts(ir, source, atom.sources, op)?;
    let accepted = parts(ir, source, claim.sources, op)?;
    require(exported.len() == syntax_parts.len() && exported.len() == accepted.len())?;
    let mut total = 0_u64;
    for (index, span) in exported.iter().enumerate() {
        op.events(1)?;
        require(*span == syntax_parts[index] && *span == accepted[index])?;
        total = total
            .checked_add(span.len())
            .ok_or(CoreError::ArithmeticOverflow)?;
    }
    let first = *exported.first().ok_or(CoreError::InvalidProvenance)?;
    let last = *exported.last().ok_or(CoreError::InvalidProvenance)?;
    let (opening, closing): (&[u8], &[u8]) = match syntax {
        MathSyntax::DollarInline => (b"$", b"$"),
        MathSyntax::DollarDisplay => (b"$$", b"$$"),
        MathSyntax::LatexInline => (b"\\(", b"\\)"),
        MathSyntax::LatexDisplay => (b"\\[", b"\\]"),
        MathSyntax::CodeInline => (b"$`", b"`$"),
        MathSyntax::FencedDisplay => return Err(CoreError::InvalidProvenance),
    };
    let first_bytes = source_bytes(source, first)?;
    let last_bytes = source_bytes(source, last)?;
    op.scan((opening.len() + closing.len()) as u64)?;
    require(first_bytes.starts_with(opening) && last_bytes.ends_with(closing))?;
    require(total > (opening.len() + closing.len()) as u64)?;
    if syntax == MathSyntax::DollarInline {
        require(!first_bytes.starts_with(b"$$"))?;
    }
    payload_owner(ir, input, PayloadOwner::Node(node), PayloadRole::MathInput)?;
    let payload = ir.payloads()[input.index()];
    let payload_parts = parts(ir, source, payload.domains, op)?;
    let content_start = first
        .start()
        .checked_add(opening.len() as u64)
        .ok_or(CoreError::ArithmeticOverflow)?;
    let content_end = last
        .end()
        .checked_sub(closing.len() as u64)
        .ok_or(CoreError::InvalidProvenance)?;
    let mut content_bytes = 0_u64;
    for span in payload_parts {
        op.events(1)?;
        require(span.start() >= content_start && span.end() <= content_end)?;
        require(super::super::replay::covered(*span, exported, op)?)?;
        content_bytes = content_bytes
            .checked_add(span.len())
            .ok_or(CoreError::ArithmeticOverflow)?;
    }
    require(content_bytes == total - (opening.len() + closing.len()) as u64)?;
    for index in payload.atoms.indices(ir.atoms_len())? {
        let input_atom = ir.owned_read_atom_at(index, op)?;
        let input_claim = ir.owned_read_authority(input_atom.claim, op)?;
        require(
            input_atom.owner == node
                && input_claim.owner == node
                && input_claim.kind == ClaimKind::Math(syntax)
                && input_claim.phase == NativePhase::Inline
                && input_claim.host.role == HostRole::MathInput,
        )?;
        if let Some(proof) = input_atom.proof {
            let stages = ir.proofs()[proof.index()].stages.slice(ir.stages())?;
            for stage in stages {
                op.events(1)?;
                let stage_claim = ir.owned_read_authority(stage.accepted_by, op)?;
                if matches!(
                    stage.kind,
                    StageKind::CodeLineEndingToSpace | StageKind::CodeEdgeTrim
                ) {
                    require(
                        matches!(syntax, MathSyntax::DollarInline | MathSyntax::CodeInline)
                            && stage_claim.owner == node
                            && stage_claim.kind == ClaimKind::Math(syntax),
                    )?;
                }
            }
        }
    }
    Ok(())
}

#[allow(clippy::too_many_arguments)]
fn validate_fence<'id>(
    ir: &CompactIr,
    source: &SourceSnapshot,
    node: NodeId,
    atom: AtomRow,
    input: PayloadId,
    export_sources: Rows<SourceSpan>,
    fence: (NodeKind, &str),
    op: &mut OwnedWork<'id, '_>,
) -> Result<(), CoreError> {
    let (node_kind, language) = fence;
    let row = ir.nodes()[node.index()];
    require(row.kind == node_kind)?;
    let detail = row.detail.ok_or(CoreError::InvalidProvenance)?;
    let DetailRow::Code {
        form: CodeForm::Fenced { byte, length },
        info: Some(info),
        claim: detail_claim,
    } = ir.details()[detail.index()]
    else {
        return Err(CoreError::InvalidProvenance);
    };
    require(matches!(byte, b'`' | b'~') && length >= 3)?;
    payload_owner(ir, info, PayloadOwner::Node(node), PayloadRole::FenceInfo)?;
    let fence_info = ir.text_owned(ir.payloads()[info.index()].value, op)?;
    let token = fence_info.split_ascii_whitespace().next();
    require(token == Some(language))?;
    let claim = ir.owned_read_authority(atom.claim, op)?;
    let code_claim = ir.owned_read_authority(detail_claim, op)?;
    require(
        claim == code_claim
            && claim.owner == node
            && claim.kind == ClaimKind::Code
            && claim.phase == NativePhase::Block
            && claim.host.role == HostRole::Block,
    )?;
    let exported = parts(ir, source, export_sources, op)?;
    let syntax = parts(ir, source, atom.sources, op)?;
    let accepted = parts(ir, source, claim.sources, op)?;
    require(!exported.is_empty() && exported == syntax && exported == accepted)?;
    require(
        exported[0].start() == row.source_envelope.start()
            && exported[exported.len() - 1].end() == row.source_envelope.end(),
    )?;
    payload_owner(ir, input, PayloadOwner::Node(node), PayloadRole::FenceInput)?;
    let payload = ir.payloads()[input.index()];
    for span in parts(ir, source, payload.domains, op)? {
        require(super::super::replay::covered(*span, exported, op)?)?;
    }
    for at in payload.atoms.indices(ir.atoms_len())? {
        let atom = ir.owned_read_atom_at(at, op)?;
        let claim = ir.owned_read_authority(atom.claim, op)?;
        require(
            atom.owner == node
                && claim.owner == node
                && claim.phase == NativePhase::Block
                && claim.host.role == HostRole::FenceInput
                && claim.kind == ClaimKind::Code,
        )?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::{Extensions, ParseConfig, Profile};
    use crate::identity::SourceKey;
    use crate::model::AcceptedModel;
    use crate::policy::{CancellationToken, SourceBudget, construction_scope};
    use crate::source::SourceDocument;

    #[test]
    fn native_math_rejects_another_objects_input_export_and_a_forged_delimiter_mode() {
        let budget = SourceBudget::preview();
        let mut op = budget.begin(CancellationToken::new());
        let source = SourceDocument::new(
            SourceKey::new([135; 16], 1).unwrap(),
            b"$x$ \\(y\\)",
            &mut op,
        )
        .unwrap();
        let model = AcceptedModel::parse(
            source.snapshot(),
            ParseConfig::with_extensions(Profile::G3, Extensions::MATH),
            1,
            &mut op,
        )
        .unwrap();
        let ObjectRow::Derived {
            node,
            atom,
            kind,
            input,
            export_sources,
        } = model.ir().objects()[0]
        else {
            panic!()
        };
        let ObjectRow::Derived {
            input: other_input,
            export_sources: other_export,
            ..
        } = model.ir().objects()[1]
        else {
            panic!()
        };
        for (candidate_kind, candidate_input, candidate_export) in [
            (
                DerivedKind::Math(MathSyntax::LatexInline),
                input,
                export_sources,
            ),
            (kind, other_input, export_sources),
            (kind, input, other_export),
        ] {
            let mut op = budget.begin(CancellationToken::new());
            let result = construction_scope(&mut op, |mut work| {
                let atom = model.ir().owned_read_atom_at(atom.index(), &mut work)?;
                validate(
                    model.ir(),
                    model.source(),
                    node,
                    atom,
                    candidate_kind,
                    candidate_input,
                    candidate_export,
                    &mut work,
                )
                .map_err(|error| work.fail(error))?;
                work.finish()
            });
            assert_eq!(result, Err(CoreError::InvalidProvenance));
        }
    }
}
