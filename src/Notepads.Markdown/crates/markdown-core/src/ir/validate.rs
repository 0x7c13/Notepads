// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE file in the project root for license information.
// ---------------------------------------------------------------------------------------------

//! Full graph, authority-domain, partition and transform validation for one immutable draft.

use super::storage::owned_row;
use super::validation_output::{Checkpoint, DraftFacts, SPARSE_BYTES, VerifiedFacts};
use super::*;
use crate::error::LimitKind;
use crate::identity::ModelIdentity;
use crate::memory::{BuildGrowingVec, BuildRows, BuildValue};
use crate::policy::{CHECKPOINT_BYTES, OwnedWork, ScalarBatchEnd, ScalarChargeOrder};
use crate::source::SourceSnapshot;
use std::ops::ControlFlow;

mod derived;
mod task;
mod toc;

#[derive(Clone, Copy)]
struct Frame {
    node: NodeId,
    depth: u64,
}
#[derive(Clone, Copy)]
struct Posting {
    span: SourceSpan,
}
#[derive(Clone, Copy)]
struct TargetIndex {
    positives: Rows<AtomId>,
}

#[derive(Clone, Copy)]
struct TargetSpec {
    domains: Rows<SourceSpan>,
    atoms: Rows<AtomRow>,
    length: u64,
    flow: Option<FlowId>,
    payload: Option<PayloadRow>,
    reordered_footnotes: bool,
}

fn require(value: bool) -> Result<(), CoreError> {
    if value {
        Ok(())
    } else {
        Err(CoreError::InvalidProvenance)
    }
}

pub(crate) fn validate_owned<'id>(
    ir: &CompactIr,
    source: &SourceSnapshot,
    op: &mut OwnedWork<'id, '_>,
) -> Result<(), CoreError> {
    let result = inner(ir, source, op, false);
    result.map(|_| ()).map_err(|error| op.fail(error))
}

// Only this module can construct the completed-validation witness.
pub(super) struct ValidationSeal(());

pub(crate) fn validate_for_index_owned<'id, 'ir, 'src, 'call, 'op>(
    ir: &'ir CompactIr,
    source: &'src SourceSnapshot,
    identity: ModelIdentity,
    op: &'call mut OwnedWork<'id, 'op>,
) -> Result<VerifiedFacts<'ir, 'src, 'call, 'id, 'op>, CoreError> {
    op.checkpoint_now()?;
    if !op.uses_verified_index() {
        return Err(op.fail(CoreError::UnsupportedConfiguration));
    }
    op.ensure_budget(&ir.budget)?;
    op.ensure_budget(source.budget())?;
    let expected = identity.source();
    let actual = source.key();
    if expected.document() != actual.document() {
        return Err(op.fail(CoreError::WrongDocument));
    }
    if expected.revision() != actual.revision() {
        return Err(op.fail(CoreError::WrongRevision {
            expected: expected.revision(),
            actual: actual.revision(),
        }));
    }
    let result = inner(ir, source, op, true);
    let (main, facts) = result.map_err(|error| op.fail(error))?;
    let facts = facts.ok_or_else(|| op.fail(CoreError::InternalInvariant))?;
    op.checkpoint_now()?;
    Ok(facts.seal(ir, source, identity, main, op, ValidationSeal(())))
}

fn inner<'id>(
    ir: &CompactIr,
    source: &SourceSnapshot,
    op: &mut OwnedWork<'id, '_>,
    collect: bool,
) -> Result<(FlowId, Option<DraftFacts<'id>>), CoreError> {
    op.check()?;
    op.ensure_budget(source.budget())?;
    let limits = op
        .model_limits()
        .ok_or(CoreError::UnsupportedConfiguration)?;
    require(!ir.flows().is_empty() && !ir.nodes().is_empty())?;
    if op.uses_task_access() {
        task::check_spans(
            ir,
            source,
            Rows::new(0, ir.sources().len())?,
            task::SpanPolicy::All,
            op,
        )?;
    } else {
        for span in ir.sources() {
            op.events(1)?;
            source.validate_span(*span)?;
        }
    }
    // The pool owns its segments; each referenced slice checks its physical bounds
    // and scalar boundaries. Value/proof validation remains independent below.
    let (main, reordered_footnotes, table_of_contents) = graph(ir, source, op)?;
    claims(ir, source, op)?;
    let mut facts = if collect {
        Some(DraftFacts::new(ir.atoms_len(), op)?)
    } else {
        None
    };
    let mut atom_owners = zeros(ir.atoms_len(), op)?;
    let mut positives =
        BuildGrowingVec::<AtomId>::new(limits.index_records, LimitKind::IndexRecords, op)?;
    let mut targets =
        BuildGrowingVec::<TargetIndex>::new(limits.model_records, LimitKind::ModelRecords, op)?;
    let mut html_delimiters = 0_usize;
    for (index, flow) in ir.flows().iter().enumerate() {
        op.events(1)?;
        let id = FlowId::from_index(index)?;
        let start = positives.len();
        let endpoints = target(
            ir,
            source,
            TargetSpec {
                domains: flow.domains,
                atoms: flow.atoms,
                length: flow.logical_utf16,
                flow: Some(id),
                payload: None,
                reordered_footnotes: reordered_footnotes && id == main,
            },
            &mut atom_owners,
            &mut positives,
            facts.as_mut(),
            op,
        )?;
        html_delimiters = html_delimiters
            .checked_add(endpoints)
            .ok_or(CoreError::ArithmeticOverflow)?;
        targets.push(
            TargetIndex {
                positives: Rows::new(start, positives.len() - start)?,
            },
            op,
        )?;
    }
    for payload in ir.payloads() {
        op.events(1)?;
        let start = positives.len();
        target(
            ir,
            source,
            TargetSpec {
                domains: payload.domains,
                atoms: payload.atoms,
                length: payload.utf8_len,
                flow: None,
                payload: Some(*payload),
                reordered_footnotes: false,
            },
            &mut atom_owners,
            &mut positives,
            facts.as_mut(),
            op,
        )?;
        targets.push(
            TargetIndex {
                positives: Rows::new(start, positives.len() - start)?,
            },
            op,
        )?;
        payload_value(ir, source, *payload, op)?;
    }
    check_flags(&atom_owners, 1, op)?;
    drop(atom_owners);
    for (index, node) in ir.nodes().iter().enumerate() {
        op.events(1)?;
        let flow = ir
            .flows()
            .get(node.flow.index())
            .ok_or(CoreError::InvalidProvenance)?;
        require(node.logical.end() <= flow.logical_utf16)?;
        let positive = owned_row(&targets, node.flow.index(), op)?.positives;
        positive
            .indices(positives.len())
            .map_err(|error| op.fail(error))?;
        let first = logical_boundary(
            ir,
            source,
            positive,
            &positives,
            node.logical.start(),
            facts.as_ref(),
            op,
        )?;
        // The flow partition makes the full, nonempty endpoint spans
        // unique. This count proves that each leaf has its own proof,
        // even if a generic hidden atom otherwise covers its source.
        let delimiters = if inline_html_code_detail(ir, *node).is_some() {
            2
        } else if is_html_emphasis_leaf(*node) {
            1
        } else {
            0
        };
        html_delimiters = html_delimiters
            .checked_sub(delimiters)
            .ok_or(CoreError::InvalidProvenance)?;
        if inline_html_break_detail(ir, *node).is_some() {
            let atom = first.ok_or(CoreError::InvalidProvenance)?;
            require(
                atom.owner == NodeId::from_index(index)?
                    && atom.kind == AtomKind::Transform
                    && atom.target.start == node.logical.start()
                    && atom.target.end == node.logical.end(),
            )?;
        }
        logical_boundary(
            ir,
            source,
            positive,
            &positives,
            node.logical.end(),
            facts.as_ref(),
            op,
        )?;
    }
    require(html_delimiters == 0)?;
    if table_of_contents {
        toc::verify(ir, source, main, &targets, &positives, op)?;
    }
    drop(targets);
    drop(positives);
    details(ir, source, op)?;
    objects(ir, source, op)?;
    references(ir, source, op)?;
    payloads(ir, op)?;
    proofs(ir, op)?;
    if let Some(facts) = &facts {
        facts.complete(op)?;
    }
    op.check()?;
    Ok((main, facts))
}

fn zeros<'id>(
    count: usize,
    op: &mut OwnedWork<'id, '_>,
) -> Result<BuildGrowingVec<'id, u8>, CoreError> {
    let maximum = op
        .model_limits()
        .ok_or(CoreError::UnsupportedConfiguration)?
        .index_records;
    let mut values = BuildGrowingVec::new(maximum, LimitKind::IndexRecords, op)?;
    for _ in 0..count {
        values.push(0, op)?;
    }
    Ok(values)
}

fn graph<'id>(
    ir: &CompactIr,
    source: &SourceSnapshot,
    op: &mut OwnedWork<'id, '_>,
) -> Result<(FlowId, bool, bool), CoreError> {
    let limits = op
        .model_limits()
        .ok_or(CoreError::UnsupportedConfiguration)?;
    let mut seen = zeros(ir.nodes().len(), op)?;
    let mut stack =
        BuildGrowingVec::<Frame>::new(limits.index_records, LimitKind::IndexRecords, op)?;
    let mut main_count = 0;
    let mut main = None;
    let mut reordered_footnotes = false;
    let mut table_of_contents = false;
    for (flow_index, flow) in ir.flows().iter().enumerate() {
        op.events(1)?;
        let flow_id = FlowId::from_index(flow_index)?;
        let root = *ir
            .nodes()
            .get(flow.root.index())
            .ok_or(CoreError::InvalidProvenance)?;
        let domains = domains(ir, source, flow.domains, op)?;
        require(
            root.flow == flow_id
                && root.parent.is_none()
                && root.next_sibling.is_none()
                && root.logical.start() == 0
                && root.logical.end() == flow.logical_utf16,
        )?;
        match flow.scope {
            FlowScope::Main => {
                main_count += 1;
                main = Some(flow_id);
                require(
                    root.kind == NodeKind::Document
                        && domains.len() == 1
                        && domains[0] == SourceSpan::new(0, source.text().len() as u64)?
                        && root.source_envelope == domains[0],
                )?;
            }
            FlowScope::ImageAlt { object } => {
                require(root.kind == NodeKind::AltRoot)?;
                let image = ir
                    .objects()
                    .get(object.index())
                    .ok_or(CoreError::InvalidProvenance)?;
                require(matches!(image, ObjectRow::Image { alt, .. } if *alt == flow_id))?;
                require(
                    root.source_envelope.start() == domains[0].start()
                        && root.source_envelope.end() == domains[domains.len() - 1].end(),
                )?;
                if domains[0].is_empty() {
                    require(flow.atoms.is_empty() && flow.logical_utf16 == 0)?;
                }
            }
        }
        require(owned_row(&seen, flow.root.index(), op)? == 0)?;
        seen.replace(flow.root.index(), 1, op)?;
        put_frame(
            &mut stack,
            0,
            Frame {
                node: flow.root,
                depth: 1,
            },
            op,
        )?;
        let mut top = 1;
        while top > 0 {
            top -= 1;
            let frame = owned_row(&stack, top, op)?;
            op.check_limit(LimitKind::Depth, frame.depth, limits.depth)?;
            let node = *ir
                .nodes()
                .get(frame.node.index())
                .ok_or(CoreError::InvalidProvenance)?;
            require(node.flow == flow_id && owned_row(&seen, frame.node.index(), op)? == 1)?;
            seen.replace(frame.node.index(), 2, op)?;
            source.validate_span(node.source_envelope)?;
            if let Some(detail) = node.detail {
                let _ = ir
                    .details()
                    .get(detail.index())
                    .ok_or(CoreError::InvalidProvenance)?;
            }
            require(
                node.logical.start() <= node.logical.end()
                    && node.logical.end() <= flow.logical_utf16,
            )?;
            if node.kind == NodeKind::FootnoteFooter {
                require(node.source_envelope.is_empty() && node.first_child.is_some())?;
            }
            if node.kind == NodeKind::TableOfContents {
                require(flow.scope == FlowScope::Main && !node.source_envelope.is_empty())?;
                table_of_contents = true;
            }
            description_shape(ir, node, op)?;
            let mut child = node.first_child;
            let mut previous_end = None;
            let mut logical_end = node.logical.start();
            let mut in_footnotes = false;
            while let Some(id) = child {
                op.events(1)?;
                let row = *ir
                    .nodes()
                    .get(id.index())
                    .ok_or(CoreError::InvalidProvenance)?;
                require(
                    row.parent == Some(frame.node)
                        && row.flow == flow_id
                        && owned_row(&seen, id.index(), op)? == 0
                        && node.source_envelope.start() <= row.source_envelope.start()
                        && row.source_envelope.end() <= node.source_envelope.end()
                        && node.logical.start() <= row.logical.start()
                        && row.logical.end() <= node.logical.end(),
                )?;
                if row.kind == NodeKind::FootnoteFooter {
                    require(
                        node.kind == NodeKind::FootnoteDefinition
                            && row.next_sibling.is_none()
                            && row.source_envelope.end() == node.source_envelope.end(),
                    )?;
                }
                if node.kind == NodeKind::DescriptionList {
                    require(row.kind == NodeKind::DescriptionItem)?;
                }
                if node.kind == NodeKind::DescriptionDetails {
                    require(matches!(
                        row.kind,
                        NodeKind::Paragraph
                            | NodeKind::Heading
                            | NodeKind::BlockQuote
                            | NodeKind::List
                            | NodeKind::DescriptionList
                            | NodeKind::CodeBlock
                            | NodeKind::HtmlBlock
                            | NodeKind::HtmlComment
                            | NodeKind::ThematicBreak
                            | NodeKind::Table
                            | NodeKind::Diagram
                            | NodeKind::MathBlock
                            | NodeKind::TableOfContents
                    ))?;
                }
                require(
                    (row.kind == NodeKind::FootnoteBacklink)
                        == (node.kind == NodeKind::FootnoteFooter),
                )?;
                require(
                    (row.kind == NodeKind::TableOfContentsEntry)
                        == (node.kind == NodeKind::TableOfContents),
                )?;
                if row.kind == NodeKind::FootnoteDefinition {
                    require(
                        frame.node == flow.root
                            && matches!(flow.scope, FlowScope::Main)
                            && logical_end <= row.logical.start(),
                    )?;
                    in_footnotes = true;
                    reordered_footnotes = true;
                } else {
                    require(!in_footnotes)?;
                    if let Some(end) = previous_end {
                        require(end <= row.source_envelope.start())?;
                    }
                    previous_end = Some(row.source_envelope.end());
                }
                logical_end = logical_end.max(row.logical.end());
                seen.replace(id.index(), 1, op)?;
                put_frame(
                    &mut stack,
                    top,
                    Frame {
                        node: id,
                        depth: frame
                            .depth
                            .checked_add(1)
                            .ok_or(CoreError::ArithmeticOverflow)?,
                    },
                    op,
                )?;
                top += 1;
                child = row.next_sibling;
            }
        }
    }
    require(main_count == 1)?;
    check_flags(&seen, 2, op)?;
    // Every Alt has exactly one Image owner. Following the functional parent chain
    // detects self/cycles without recursion or a second ownership graph.
    for index in 0..ir.flows().len() {
        let mut cursor = FlowId::from_index(index)?;
        let mut depth = 0_u64;
        loop {
            op.events(1)?;
            depth = depth.checked_add(1).ok_or(CoreError::ArithmeticOverflow)?;
            op.check_limit(LimitKind::Depth, depth, limits.depth)?;
            let flow = ir
                .flows()
                .get(cursor.index())
                .ok_or(CoreError::InvalidProvenance)?;
            match flow.scope {
                FlowScope::Main => break,
                FlowScope::ImageAlt { object } => match *ir
                    .objects()
                    .get(object.index())
                    .ok_or(CoreError::InvalidProvenance)?
                {
                    ObjectRow::Image { node, alt, .. } => {
                        require(alt == cursor)?;
                        cursor = ir
                            .nodes()
                            .get(node.index())
                            .ok_or(CoreError::InvalidProvenance)?
                            .flow;
                    }
                    _ => return Err(CoreError::InvalidProvenance),
                },
            }
            require(cursor.index() != index && depth <= ir.flows().len() as u64)?;
        }
    }
    Ok((
        main.ok_or(CoreError::InvalidProvenance)?,
        reordered_footnotes,
        table_of_contents,
    ))
}

fn description_shape(
    ir: &CompactIr,
    node: NodeRow,
    op: &mut OwnedWork<'_, '_>,
) -> Result<(), CoreError> {
    let read = |id: Option<NodeId>, op: &mut OwnedWork<'_, '_>| -> Result<NodeRow, CoreError> {
        op.scan(size_of::<NodeRow>() as u64)?;
        id.and_then(|id| ir.nodes().get(id.index()).copied())
            .ok_or(CoreError::InvalidProvenance)
    };
    match node.kind {
        NodeKind::DescriptionList => {
            let item = read(node.first_child, op)?;
            require(
                item.kind == NodeKind::DescriptionItem
                    && read(item.first_child, op)?.kind == NodeKind::DescriptionTerm,
            )?;
        }
        NodeKind::DescriptionItem => {
            require(read(node.parent, op)?.kind == NodeKind::DescriptionList)?;
            let first = read(node.first_child, op)?;
            let details = if first.kind == NodeKind::DescriptionTerm {
                read(first.next_sibling, op)?
            } else {
                first
            };
            require(
                details.kind == NodeKind::DescriptionDetails && details.next_sibling.is_none(),
            )?;
        }
        NodeKind::DescriptionTerm => {
            let paragraph = read(node.first_child, op)?;
            require(
                read(node.parent, op)?.kind == NodeKind::DescriptionItem
                    && paragraph.kind == NodeKind::Paragraph
                    && paragraph.next_sibling.is_none(),
            )?;
        }
        NodeKind::DescriptionDetails => {
            require(read(node.parent, op)?.kind == NodeKind::DescriptionItem)?
        }
        _ => {}
    }
    Ok(())
}

fn put_frame<'id>(
    stack: &mut BuildGrowingVec<'id, Frame>,
    index: usize,
    value: Frame,
    op: &mut OwnedWork<'id, '_>,
) -> Result<(), CoreError> {
    if index == stack.len() {
        stack.push(value, op)
    } else {
        stack.replace(index, value, op)
    }
}

fn domains<'id, 'a>(
    ir: &'a CompactIr,
    source: &SourceSnapshot,
    range: Rows<SourceSpan>,
    op: &mut OwnedWork<'id, '_>,
) -> Result<crate::ir::RowView<'a, SourceSpan>, CoreError> {
    let spans = range.slice(ir.sources())?;
    require(!spans.is_empty())?;
    if op.uses_task_access() {
        task::check_spans(ir, source, range, task::SpanPolicy::Domains, op)?;
        return Ok(spans);
    }
    let mut previous = None;
    for span in spans {
        op.events(1)?;
        source.validate_span(*span)?;
        require(!span.is_empty() || spans.len() == 1)?;
        if let Some(end) = previous {
            require(end < span.start())?;
        }
        previous = Some(span.end());
    }
    Ok(spans)
}

fn parts<'id, 'a>(
    ir: &'a CompactIr,
    source: &SourceSnapshot,
    range: Rows<SourceSpan>,
    op: &mut OwnedWork<'id, '_>,
) -> Result<crate::ir::RowView<'a, SourceSpan>, CoreError> {
    let spans = range.slice(ir.sources())?;
    if op.uses_task_access() {
        task::check_spans(ir, source, range, task::SpanPolicy::Parts, op)?;
        return Ok(spans);
    }
    let mut previous = None;
    for span in spans {
        op.events(1)?;
        source.validate_span(*span)?;
        if let Some(end) = previous {
            require(end <= span.start())?;
        }
        previous = Some(span.end());
    }
    Ok(spans)
}

fn related<'id>(
    ir: &CompactIr,
    left: NodeId,
    right: NodeId,
    op: &mut OwnedWork<'id, '_>,
) -> Result<bool, CoreError> {
    let a = *ir
        .nodes()
        .get(left.index())
        .ok_or(CoreError::InvalidProvenance)?;
    let b = *ir
        .nodes()
        .get(right.index())
        .ok_or(CoreError::InvalidProvenance)?;
    if a.flow != b.flow {
        return Ok(false);
    }
    for start in [left, right] {
        let wanted = if start == left { right } else { left };
        let mut cursor = Some(start);
        let mut steps = 0;
        while let Some(node) = cursor {
            op.events(1)?;
            if node == wanted {
                return Ok(true);
            }
            cursor = ir
                .nodes()
                .get(node.index())
                .ok_or(CoreError::InvalidProvenance)?
                .parent;
            steps += 1;
            require(steps <= ir.nodes().len())?;
        }
    }
    Ok(false)
}

fn claims<'id>(
    ir: &CompactIr,
    source: &SourceSnapshot,
    op: &mut OwnedWork<'id, '_>,
) -> Result<(), CoreError> {
    let total = ir
        .atoms_len()
        .checked_add(ir.extra_authorities_len())
        .ok_or(CoreError::ArithmeticOverflow)?;
    for index in 0..total {
        let id = if index < ir.atoms_len() {
            op.scan(types::AUTHORITY_SELECTOR_BYTES)?;
            match &ir.packets.view()[index].authority {
                AuthorityBinding::Owned(_) => ClaimId::primary(AtomId::from_index(index)?),
                AuthorityBinding::Shared(id) => {
                    op.scan(size_of::<ClaimId>() as u64)?;
                    ir.owned_read_authority(*id, op)?;
                    continue;
                }
            }
        } else {
            ClaimId::extra(ExtraAuthorityId::from_index(index - ir.atoms_len())?)
        };
        let claim = ir.owned_read_authority(id, op)?;
        let host = claim.host;
        op.events(1)?;
        let _ = ir
            .nodes()
            .get(host.owner.index())
            .ok_or(CoreError::InvalidProvenance)?;
        let _ = domains(ir, source, host.domains, op)?;
        require(
            claim.consumed.end() <= host.utf8_len && related(ir, claim.owner, host.owner, op)?,
        )?;
        let accepted = parts(ir, source, claim.sources, op)?;
        require(!accepted.is_empty())?;
        let scope = host.domains.slice(ir.sources())?;
        for span in accepted {
            require(super::replay::covered(*span, scope, op)?)?;
        }
        let valid_role = match claim.phase {
            NativePhase::Block => {
                matches!(host.role, HostRole::Block | HostRole::Structure)
                    || host.role == HostRole::FenceInput
                        && matches!(
                            ir.nodes()[claim.owner.index()].kind,
                            NodeKind::Diagram | NodeKind::MathBlock
                        )
            }
            NativePhase::Inline => {
                host.role == HostRole::Inline
                    || host.role == HostRole::MathInput && matches!(claim.kind, ClaimKind::Math(_))
            }
            NativePhase::Metadata => matches!(
                host.role,
                HostRole::Destination
                    | HostRole::Title
                    | HostRole::FenceInfo
                    | HostRole::ReferenceLabel
            ),
            NativePhase::Structure => host.role == HostRole::Structure,
        };
        require(valid_role)?;
        match claim.kind {
            ClaimKind::WikiLink => {
                require(
                    claim.owner == host.owner
                        && claim.phase == NativePhase::Inline
                        && host.role == HostRole::Inline
                        && ir.nodes()[claim.owner.index()].kind == NodeKind::Link,
                )?;
                wiki_body(source, accepted, "[[", op)?;
            }
            ClaimKind::NoteEmbed => {
                require(
                    claim.owner == host.owner
                        && claim.phase == NativePhase::Inline
                        && host.role == HostRole::Inline
                        && ir.nodes()[claim.owner.index()].kind == NodeKind::NoteEmbed,
                )?;
                wiki_body(source, accepted, "![[", op)?;
            }
            ClaimKind::TableOfContentsMarker => {
                require(
                    claim.owner == host.owner
                        && claim.phase == NativePhase::Block
                        && accepted.len() == 1
                        && ir.nodes()[claim.owner.index()].kind == NodeKind::TableOfContents,
                )?;
                let raw = source_bytes(source, accepted[0])?;
                require(matches!(raw.len(), 5 | 9))?;
                op.scan(raw.len() as u64)?;
                require(
                    raw.eq_ignore_ascii_case(b"[TOC]") || raw.eq_ignore_ascii_case(b"[[_TOC_]]"),
                )?;
            }
            ClaimKind::EmojiShortcode => {
                require(
                    claim.phase == NativePhase::Inline
                        && accepted.len() == 1
                        && ir.nodes()[claim.owner.index()].kind == NodeKind::Text,
                )?;
            }
            ClaimKind::Delimiter if ir.nodes()[claim.owner.index()].kind == NodeKind::Highlight => {
                require(claim.phase == NativePhase::Inline && accepted.len() == 1)?;
                op.scan(2)?;
                require(source_bytes(source, accepted[0])? == b"==")?;
            }
            ClaimKind::Delimiter
                if matches!(
                    ir.nodes()[claim.owner.index()].kind,
                    NodeKind::Superscript | NodeKind::Subscript
                ) =>
            {
                require(claim.phase == NativePhase::Inline && accepted.len() == 1)?;
                op.scan(2)?;
                let raw = source_bytes(source, accepted[0])?;
                require(match ir.nodes()[claim.owner.index()].kind {
                    NodeKind::Superscript => matches!(raw, b"^" | b"^^"),
                    NodeKind::Subscript => raw == b"~",
                    _ => false,
                })?;
            }
            ClaimKind::DescriptionMarker => {
                require(
                    claim.owner == host.owner
                        && claim.phase == NativePhase::Block
                        && ir.nodes()[claim.owner.index()].kind == NodeKind::DescriptionItem
                        && accepted.len() == 1,
                )?;
                let span = accepted[0];
                op.scan(2)?;
                require(
                    matches!(source_bytes(source, span)?, b":" | b"~")
                        && matches!(
                            source.text().as_bytes().get(span.end() as usize),
                            Some(b' ' | b'\t')
                        ),
                )?;
            }
            ClaimKind::HeadingIdentifier => {
                require(
                    claim.owner == host.owner
                        && ir.nodes()[claim.owner.index()].kind == NodeKind::Heading,
                )?;
                let start = accepted[0].start();
                let mut end = start;
                for span in accepted {
                    require(span.start() == end)?;
                    end = span.end();
                }
                let span = SourceSpan::new(start, end)?;
                let raw = source_bytes(source, span)?;
                for chunk in raw.chunks(CHECKPOINT_BYTES as usize) {
                    op.scan(chunk.len() as u64)?;
                }
                let value = std::str::from_utf8(raw).map_err(|_| CoreError::InvalidProvenance)?;
                require(comrak::native::heading_identifier(value).is_some())?;
            }
            ClaimKind::CalloutMarker => {
                require(
                    claim.owner == host.owner
                        && claim.phase == NativePhase::Block
                        && ir.nodes()[claim.owner.index()].kind == NodeKind::BlockQuote
                        && accepted.len() == 1,
                )?;
                let raw = source_bytes(source, accepted[0])?;
                require(raw.len() <= 20)?;
                op.scan(raw.len() as u64)?;
                require(comrak::native::callout_marker(raw).is_some())?;
            }
            ClaimKind::FootnoteMarker | ClaimKind::FootnoteReference => {
                let definition = claim.kind == ClaimKind::FootnoteMarker;
                let expected_kind = if definition {
                    NodeKind::FootnoteDefinition
                } else {
                    NodeKind::FootnoteReference
                };
                require(
                    ir.nodes()[claim.owner.index()].kind == expected_kind
                        && claim.owner == host.owner
                        && claim.phase
                            == if definition {
                                NativePhase::Block
                            } else {
                                NativePhase::Inline
                            },
                )?;
                let start = accepted[0].start();
                let mut end = start;
                for span in accepted {
                    require(span.start() == end)?;
                    end = span.end();
                }
                let bytes = source_bytes(source, SourceSpan::new(start, end)?)?;
                let suffix: &[u8] = if definition { b"]:" } else { b"]" };
                let label = bytes
                    .strip_prefix(b"[^")
                    .and_then(|body| body.strip_suffix(suffix))
                    .filter(|body| !body.is_empty())
                    .ok_or(CoreError::InvalidProvenance)?;
                for chunk in label.chunks(CHECKPOINT_BYTES as usize) {
                    op.scan(chunk.len() as u64)?;
                    // The native input normalizes a source NUL to U+FFFD.
                    require(
                        chunk
                            .iter()
                            .all(|&byte| byte == 0 || !comrak::native::footnote_label_break(byte)),
                    )?;
                }
            }
            ClaimKind::InlineFootnoteOpening
            | ClaimKind::InlineFootnoteBody
            | ClaimKind::InlineFootnoteClosing => {
                let (owner, delimiter): (NodeKind, Option<&[u8]>) = match claim.kind {
                    ClaimKind::InlineFootnoteOpening => (NodeKind::FootnoteReference, Some(b"^[")),
                    ClaimKind::InlineFootnoteClosing => (NodeKind::FootnoteReference, Some(b"]")),
                    _ => (NodeKind::FootnoteDefinition, None),
                };
                require(
                    ir.nodes()[claim.owner.index()].kind == owner
                        && claim.phase == NativePhase::Inline
                        && claim.owner == host.owner
                        && claim.consumed.start() < claim.consumed.end(),
                )?;
                if let Some(delimiter) = delimiter {
                    op.scan(delimiter.len() as u64)?;
                    require(
                        accepted.len() == 1
                            && claim.consumed.end() - claim.consumed.start()
                                == delimiter.len() as u64
                            && source_bytes(source, accepted[0])? == delimiter,
                    )?;
                }
            }
            ClaimKind::Math(_) => require(
                claim.phase == NativePhase::Inline
                    && host.role == HostRole::MathInput
                    && claim.owner == host.owner
                    && ir.nodes()[claim.owner.index()].kind == NodeKind::Math,
            )?,
            ClaimKind::Entity | ClaimKind::Escape => require(matches!(
                claim.phase,
                NativePhase::Inline | NativePhase::Metadata
            ))?,
            ClaimKind::Break
            | ClaimKind::Link
            | ClaimKind::Image
            | ClaimKind::Code
            | ClaimKind::ReferenceUse => require(
                claim.phase == NativePhase::Inline
                    || claim.kind == ClaimKind::Code && claim.phase == NativePhase::Block,
            )?,
            ClaimKind::ContainerPrefix
            | ClaimKind::ReferenceDefinition
            | ClaimKind::LeadingBom
            | ClaimKind::Table
            | ClaimKind::Task => {
                require(claim.phase == NativePhase::Block || claim.phase == NativePhase::Structure)?
            }
            ClaimKind::Payload => require(claim.phase == NativePhase::Metadata)?,
            _ => {}
        }
        if claim.kind == ClaimKind::LeadingBom {
            op.scan(3)?;
            require(
                accepted.len() == 1
                    && accepted[0] == SourceSpan::new(0, 3)?
                    && source.text().as_bytes().get(..3) == Some("\u{feff}".as_bytes()),
            )?;
        }
    }
    for attribute in ir.attributes() {
        op.events(1)?;
        let claim = ir.owned_read_authority(attribute.claim, op)?;
        let _ = ir
            .nodes()
            .get(attribute.owner.index())
            .ok_or(CoreError::InvalidProvenance)?;
        require(claim.owner == attribute.owner)?;
        let spans = parts(ir, source, attribute.sources, op)?;
        require(!spans.is_empty())?;
        for span in spans {
            require(super::replay::covered(
                *span,
                claim.sources.slice(ir.sources())?,
                op,
            )?)?;
        }
    }
    Ok(())
}

// Ordinary payload stages belong to their occurrence. A table pipe keeps the
// original block-phase Cell authority, even inside a URL/title. Only the nearest
// strict Cell ancestor can grant this exception. ImageAlt crosses only its
// validated unique Image object back to the owning flow; symmetric related()
// would also admit a descendant or a different containing context.
pub(super) fn payload_stage_owner<'id>(
    ir: &CompactIr,
    owner: NodeId,
    payload_owner: PayloadOwner,
    stage: StageKind,
    claim: AuthoritySpec,
    op: &mut OwnedWork<'id, '_>,
) -> Result<bool, CoreError> {
    if stage != StageKind::TableEscapedPipe {
        return Ok(claim.owner == owner && claim.host.owner == owner);
    }
    if payload_owner != PayloadOwner::Node(owner)
        || claim.host.owner != claim.owner
        || claim.kind != ClaimKind::Table
        || claim.phase != NativePhase::Block
        || claim.host.role != HostRole::Block
        || claim.consumed.end().checked_sub(claim.consumed.start()) != Some(2)
    {
        return Ok(false);
    }
    let sources = claim.sources.slice(ir.sources())?;
    if sources.len() != 1 || sources[0].len() != 2 {
        return Ok(false);
    }
    let mut cursor = owner;
    let mut steps = 0;
    loop {
        op.events(1)?;
        let current = ir
            .nodes()
            .get(cursor.index())
            .ok_or(CoreError::InvalidProvenance)?;
        let next = if let Some(parent) = current.parent {
            let parent_row = ir
                .nodes()
                .get(parent.index())
                .ok_or(CoreError::InvalidProvenance)?;
            if parent_row.flow != current.flow {
                return Ok(false);
            }
            parent
        } else {
            let scope = ir
                .flows()
                .get(current.flow.index())
                .ok_or(CoreError::InvalidProvenance)?;
            if scope.root != cursor || current.kind != NodeKind::AltRoot {
                return Ok(false);
            }
            let FlowScope::ImageAlt { object } = scope.scope else {
                return Ok(false);
            };
            let ObjectRow::Image { node, alt, .. } = *ir
                .objects()
                .get(object.index())
                .ok_or(CoreError::InvalidProvenance)?
            else {
                return Ok(false);
            };
            let image = ir
                .nodes()
                .get(node.index())
                .ok_or(CoreError::InvalidProvenance)?;
            if alt != current.flow || image.kind != NodeKind::Image || image.flow == current.flow {
                return Ok(false);
            }
            node
        };
        let ancestor = ir
            .nodes()
            .get(next.index())
            .ok_or(CoreError::InvalidProvenance)?;
        if ancestor.kind == NodeKind::TableCell {
            return Ok(next == claim.owner
                && ancestor.source_envelope.start() <= sources[0].start()
                && sources[0].end() <= ancestor.source_envelope.end());
        }
        cursor = next;
        steps += 1;
        require(steps <= ir.nodes().len())?;
    }
}

#[derive(Clone, Copy)]
struct EmphasisEndpoint {
    tag: HtmlEmphasis,
    parent: NodeId,
    host: HostRow,
    consumed: crate::identity::LocalUtf8Span,
}
impl BuildValue for EmphasisEndpoint {}

struct EmphasisPairs<'id> {
    open: BuildGrowingVec<'id, EmphasisEndpoint>,
    depth: usize,
    count: usize,
    last_source_end: u64,
}

fn is_html_emphasis_leaf(node: NodeRow) -> bool {
    node.kind == NodeKind::HtmlInline
        && node.logical.start() == node.logical.end()
        && !node.source_envelope.is_empty()
}

impl<'id> EmphasisPairs<'id> {
    fn new(op: &mut OwnedWork<'id, '_>) -> Result<Self, CoreError> {
        let max = op
            .model_limits()
            .ok_or(CoreError::UnsupportedConfiguration)?
            .model_records;
        Ok(Self {
            open: BuildGrowingVec::new(max, LimitKind::ModelRecords, op)?,
            depth: 0,
            count: 0,
            last_source_end: 0,
        })
    }

    fn endpoint(
        &mut self,
        ir: &CompactIr,
        atom: AtomRow,
        node: NodeRow,
        claim: AuthoritySpec,
        parameter: u32,
        output_cursor: u64,
        op: &mut OwnedWork<'id, '_>,
    ) -> Result<(), CoreError> {
        let tag = HtmlEmphasis::from_parameter(parameter).ok_or(CoreError::InvalidProvenance)?;
        let parent = node.parent.ok_or(CoreError::InvalidProvenance)?;
        let parent_kind = ir
            .nodes()
            .get(parent.index())
            .ok_or(CoreError::InvalidProvenance)?
            .kind;
        let detail = html_detail_claim(ir, node).ok_or(CoreError::InvalidProvenance)?;
        let detail = ir.owned_read_authority(detail, op)?;
        require(
            is_html_emphasis_leaf(node)
                && node.first_child.is_none()
                && matches!(
                    parent_kind,
                    NodeKind::AltRoot
                        | NodeKind::Paragraph
                        | NodeKind::Heading
                        | NodeKind::TableCell
                        | NodeKind::Emphasis
                        | NodeKind::Strong
                        | NodeKind::Link
                        | NodeKind::Strike
                        | NodeKind::Highlight
                        | NodeKind::Superscript
                        | NodeKind::Subscript
                )
                && atom.kind == AtomKind::Hidden
                && atom.target.start == node.logical.start()
                && atom.target.end == node.logical.end()
                && atom.target.start == output_cursor
                && detail.kind == ClaimKind::Html
                && detail.phase == NativePhase::Inline
                && claim.host == detail.host
                && claim.consumed == detail.consumed
                && atom.sources.len() == 1
                && atom.sources.slice(ir.sources())?[0] == node.source_envelope
                && atom.sources.slice(ir.sources())? == detail.sources.slice(ir.sources())?
                && self.last_source_end <= node.source_envelope.start(),
        )?;
        self.last_source_end = node.source_envelope.end();
        if tag.closing {
            self.depth = self
                .depth
                .checked_sub(1)
                .ok_or(CoreError::InvalidProvenance)?;
            let opening = owned_row(&self.open, self.depth, op)?;
            require(
                opening.tag.matches(tag)
                    && opening.parent == parent
                    && opening.host.domains == claim.host.domains
                    && opening.host.utf8_len == claim.host.utf8_len
                    && opening.host.role == claim.host.role
                    && opening.consumed.end() <= claim.consumed.start(),
            )?;
        } else {
            let endpoint = EmphasisEndpoint {
                tag,
                parent,
                host: claim.host,
                consumed: claim.consumed,
            };
            if self.depth == self.open.len() {
                self.open.push(endpoint, op)?;
            } else {
                self.open.replace(self.depth, endpoint, op)?;
            }
            self.depth += 1;
        }
        self.count += 1;
        Ok(())
    }
}

struct OpenHtmlCode {
    owner: NodeId,
    tag: HtmlCodeDelimiter,
    syntax: AuthoritySpec,
    next: u64,
    source_row: usize,
    source_next: u64,
}

impl OpenHtmlCode {
    fn consume_source(
        &mut self,
        ir: &CompactIr,
        span: SourceSpan,
        op: &mut OwnedWork<'_, '_>,
    ) -> Result<(), CoreError> {
        require(!span.is_empty() && span.start() == self.source_next)?;
        let syntax = self.syntax.sources.slice(ir.sources())?;
        let mut cursor = span.start();
        while cursor < span.end() {
            op.events(1)?;
            let row = syntax
                .get(self.source_row)
                .ok_or(CoreError::InvalidProvenance)?;
            require(self.source_next == cursor && cursor < row.end())?;
            cursor = row.end().min(span.end());
            self.source_next = cursor;
            if cursor == row.end() {
                self.source_row += 1;
                if let Some(next) = syntax.get(self.source_row) {
                    self.source_next = next.start();
                }
            }
        }
        Ok(())
    }

    fn body(
        &mut self,
        ir: &CompactIr,
        id: AtomId,
        atom: AtomRow,
        claim: AuthoritySpec,
        op: &mut OwnedWork<'_, '_>,
    ) -> Result<(), CoreError> {
        require(
            atom.owner == self.owner
                && atom.claim == ClaimId::primary(id)
                && claim.kind == ClaimKind::Code
                && claim.phase == NativePhase::Inline
                && claim.host == self.syntax.host
                && claim.consumed.start() == self.next
                && claim.consumed.end() > self.next
                && claim.consumed.end() < self.syntax.consumed.end()
                && matches!(atom.kind, AtomKind::Copy | AtomKind::Transform),
        )?;
        let sources = atom.sources.slice(ir.sources())?;
        for span in sources {
            self.consume_source(ir, *span, op)?;
        }
        let normalized_width = if atom.kind == AtomKind::Transform {
            let proof = ir
                .proofs()
                .get(atom.proof.ok_or(CoreError::InvalidProvenance)?.index())
                .ok_or(CoreError::InvalidProvenance)?;
            let primitives = proof.primitives.slice(ir.primitives())?;
            let stages = proof.stages.slice(ir.stages())?;
            require(
                primitives.len() == 1
                    && matches!(
                        primitives[0].kind,
                        PrimitiveKind::Copy
                            | PrimitiveKind::CrLf
                            | PrimitiveKind::Cr
                            | PrimitiveKind::Nul
                    )
                    && (stages.is_empty()
                        || stages.len() == 1
                            && matches!(
                                stages[0].kind,
                                StageKind::PartialTab | StageKind::TableEscapedPipe
                            )),
            )?;
            let AtomContent::Pool(value) = atom.content else {
                return Err(CoreError::InvalidProvenance);
            };
            super::replay::pool(ir, value, op)?.len() as u64
        } else {
            require(sources.len() == 1)?;
            sources[0].end() - sources[0].start()
        };
        require(claim.consumed.end() - self.next == normalized_width)?;
        self.next = claim.consumed.end();
        Ok(())
    }
}

fn require_full_html_code_sources(
    ir: &CompactIr,
    syntax: AuthoritySpec,
    envelope: SourceSpan,
    op: &mut OwnedWork<'_, '_>,
) -> Result<(), CoreError> {
    // The claim validator has already proved sorted, disjoint coverage within
    // the host. Equal byte counts therefore prove the complete intersection.
    let mut accepted = 0_u64;
    for span in syntax.sources.slice(ir.sources())? {
        op.events(1)?;
        accepted = accepted
            .checked_add(span.len())
            .ok_or(CoreError::ArithmeticOverflow)?;
    }
    let domains = syntax.host.domains.slice(ir.sources())?;
    let mut lo = 0;
    let mut hi = domains.len();
    while lo < hi {
        op.events(1)?;
        let mid = lo + (hi - lo) / 2;
        if domains[mid].end() <= envelope.start() {
            lo = mid + 1;
        } else {
            hi = mid;
        }
    }
    let mut expected = 0_u64;
    while let Some(domain) = domains.get(lo) {
        op.events(1)?;
        if domain.start() >= envelope.end() {
            break;
        }
        let start = domain.start().max(envelope.start());
        let end = domain.end().min(envelope.end());
        expected = expected
            .checked_add(end - start)
            .ok_or(CoreError::ArithmeticOverflow)?;
        lo += 1;
    }
    require(expected == accepted)
}

fn html_code_endpoint(
    ir: &CompactIr,
    atom: AtomRow,
    node: NodeRow,
    claim: AuthoritySpec,
    parameter: u32,
    cursor: u64,
    open: &mut Option<OpenHtmlCode>,
    op: &mut OwnedWork<'_, '_>,
) -> Result<SourceSpan, CoreError> {
    let tag = HtmlCodeDelimiter::from_parameter(parameter).ok_or(CoreError::InvalidProvenance)?;
    let detail = inline_html_code_detail(ir, node).ok_or(CoreError::InvalidProvenance)?;
    let syntax = ir.owned_read_authority(detail, op)?;
    let sources = atom.sources.slice(ir.sources())?;
    let syntax_sources = syntax.sources.slice(ir.sources())?;
    require(
        node.first_child.is_none()
            && atom.kind == AtomKind::Hidden
            && atom.target.start == cursor
            && atom.target.end == cursor
            && syntax.owner == atom.owner
            && syntax.host == claim.host
            && syntax.kind == ClaimKind::Html
            && syntax.phase == NativePhase::Inline
            && sources.len() == 1
            && !syntax_sources.is_empty()
            && syntax_sources[0].start() == node.source_envelope.start()
            && syntax_sources[syntax_sources.len() - 1].end() == node.source_envelope.end(),
    )?;
    if tag.closing {
        let mut start = open.take().ok_or(CoreError::InvalidProvenance)?;
        start.consume_source(ir, sources[0], op)?;
        require(
            start.owner == atom.owner
                && start.tag.matches(tag)
                && start.source_row == syntax_sources.len()
                && start.next == claim.consumed.start()
                && claim.consumed.end() == syntax.consumed.end()
                && sources[0].end() == node.source_envelope.end()
                && cursor == node.logical.end(),
        )?;
    } else {
        require(
            open.is_none()
                && claim.consumed.start() == syntax.consumed.start()
                && sources[0].start() == node.source_envelope.start()
                && cursor == node.logical.start(),
        )?;
        require_full_html_code_sources(ir, syntax, node.source_envelope, op)?;
        let mut start = OpenHtmlCode {
            owner: atom.owner,
            tag,
            syntax,
            next: claim.consumed.end(),
            source_row: 0,
            source_next: syntax_sources[0].start(),
        };
        start.consume_source(ir, sources[0], op)?;
        *open = Some(start);
    }
    Ok(sources[0])
}

fn footnote_ancestor(
    ir: &CompactIr,
    mut id: NodeId,
    op: &mut OwnedWork<'_, '_>,
) -> Result<Option<NodeId>, CoreError> {
    loop {
        op.events(1)?;
        let node = ir
            .nodes()
            .get(id.index())
            .ok_or(CoreError::InvalidProvenance)?;
        if node.kind == NodeKind::FootnoteDefinition {
            return Ok(Some(id));
        }
        let Some(parent) = node.parent else {
            return Ok(None);
        };
        id = parent;
    }
}

fn matches_ordinal(digits: &[u8], expected: u32) -> bool {
    digits
        .first()
        .is_some_and(|byte| (b'1'..=b'9').contains(byte))
        && digits.iter().all(u8::is_ascii_digit)
        && std::str::from_utf8(digits)
            .ok()
            .and_then(|value| value.parse::<u32>().ok())
            == Some(expected)
}

fn target<'id>(
    ir: &CompactIr,
    source: &SourceSnapshot,
    spec: TargetSpec,
    owners: &mut BuildGrowingVec<'id, u8>,
    positives: &mut BuildGrowingVec<'id, AtomId>,
    mut facts: Option<&mut DraftFacts<'id>>,
    op: &mut OwnedWork<'id, '_>,
) -> Result<usize, CoreError> {
    let TargetSpec {
        domains: domain_range,
        atoms: atom_range,
        length,
        flow,
        payload,
        reordered_footnotes,
    } = spec;
    let payload_role = payload.map(|p| p.role);
    let scope = domains(ir, source, domain_range, op)?;
    let atoms = atom_range.indices(ir.atoms_len())?;
    let maximum = op
        .model_limits()
        .ok_or(CoreError::UnsupportedConfiguration)?
        .source_pieces;
    let mut postings = BuildGrowingVec::<Posting>::new(maximum, LimitKind::SourcePieces, op)?;
    let mut output_cursor = 0_u64;
    let mut emphasis = EmphasisPairs::new(op)?;
    let mut html_code = None;
    let mut code_endpoints = 0_usize;
    let mut previous_positive_end = 0;
    let mut note = None;

    for (relative, position) in atoms.enumerate() {
        let atom = ir.owned_read_atom_at(position, op)?;
        op.events(1)?;
        let position = atom_range
            .start()
            .checked_add(relative)
            .ok_or(CoreError::ArithmeticOverflow)?;
        let id = AtomId::from_index(position)?;
        require(owned_row(owners, position, op)? == 0)?;
        owners.replace(position, 1, op)?;
        let node = *ir
            .nodes()
            .get(atom.owner.index())
            .ok_or(CoreError::InvalidProvenance)?;
        if let Some(flow) = flow {
            require(node.flow == flow)?;
        }
        let claim = ir.owned_read_authority(atom.claim, op)?;
        let first_stage = ir.first_stage(atom);
        if reordered_footnotes
            && (atom.target.start < atom.target.end
                || first_stage.is_some_and(|stage| {
                    matches!(
                        stage.kind,
                        StageKind::HtmlEmphasisDelimiter | StageKind::HtmlCodeDelimiter
                    )
                }))
        {
            let next_note = footnote_ancestor(ir, atom.owner, op)?;
            if note != next_note {
                require(emphasis.depth == 0 && html_code.is_none())?;
                previous_positive_end = 0;
                emphasis.last_source_end = 0;
                note = next_note;
            }
        }
        let html_detail = inline_html_break_detail(ir, node);
        let html_stage =
            first_stage.is_some_and(|stage| stage.kind == StageKind::HtmlBreakToLineFeed);
        if let Some(detail_claim) = html_detail {
            let authority = ir.owned_read_authority(detail_claim, op)?;
            require(
                html_stage
                    && flow.is_some()
                    && atom.kind == AtomKind::Transform
                    && atom.target.start == node.logical.start()
                    && atom.target.end == node.logical.end()
                    && claim.host == authority.host
                    && claim.consumed == authority.consumed
                    && claim.sources.len() == 1
                    && authority.sources.len() == 1
                    && claim.sources.slice(ir.sources())?[0] == node.source_envelope
                    && claim.sources.slice(ir.sources())?
                        == authority.sources.slice(ir.sources())?,
            )?;
        } else {
            require(!html_stage)?;
        }
        if let Some(stage) =
            first_stage.filter(|stage| stage.kind == StageKind::HtmlEmphasisDelimiter)
        {
            require(flow.is_some() && previous_positive_end <= node.source_envelope.start())?;
            emphasis.endpoint(ir, atom, node, claim, stage.parameter, output_cursor, op)?;
        }
        if let Some(stage) = first_stage.filter(|stage| stage.kind == StageKind::HtmlCodeDelimiter)
        {
            require(flow.is_some())?;
            let span = html_code_endpoint(
                ir,
                atom,
                node,
                claim,
                stage.parameter,
                output_cursor,
                &mut html_code,
                op,
            )?;
            require(
                previous_positive_end <= span.start() && emphasis.last_source_end <= span.start(),
            )?;
            emphasis.last_source_end = span.end();
            code_endpoints += 1;
        } else if atom.kind == AtomKind::Hidden
            && claim.kind == ClaimKind::Code
            && inline_html_code_detail(ir, node).is_some()
        {
            return Err(CoreError::InvalidProvenance);
        }
        if let Some(payload) = payload {
            let owner = match payload.owner {
                PayloadOwner::Node(owner) => owner,
                PayloadOwner::Definition(id) => {
                    let definition = ir
                        .definitions()
                        .get(id.index())
                        .ok_or(CoreError::InvalidProvenance)?;
                    ir.owned_read_authority(definition.claim, op)?.owner
                }
            };
            require(atom.owner == owner && claim.owner == owner)?;
            require(claim.host.owner == owner)?;
            if let Some(proof) = atom.proof {
                let proof = ir
                    .proofs()
                    .get(proof.index())
                    .ok_or(CoreError::InvalidProvenance)?;
                for stage in proof.stages.slice(ir.stages())? {
                    op.events(1)?;
                    let stage_claim = ir.owned_read_authority(stage.accepted_by, op)?;
                    require(payload_stage_owner(
                        ir,
                        owner,
                        payload.owner,
                        stage.kind,
                        stage_claim,
                        op,
                    )?)?;
                }
            }
        }
        require(
            related(ir, atom.owner, claim.owner, op)?
                && atom.target.start <= atom.target.end
                && atom.target.end <= length,
        )?;
        if flow.is_some() && atom.kind != AtomKind::Synthetic {
            require(
                node.logical.start() <= atom.target.start && atom.target.end <= node.logical.end(),
            )?;
        }
        let spans = parts(ir, source, atom.sources, op)?;
        require(!spans.is_empty())?;
        let mut all_positive = true;
        for span in spans {
            op.events(1)?;
            all_positive &= !span.is_empty();
        }
        let authority = claim.sources.slice(ir.sources())?;
        for span in spans {
            require(
                super::replay::covered(*span, scope, op)?
                    && super::replay::covered(*span, authority, op)?,
            )?;
            if !span.is_empty() {
                postings.push(Posting { span: *span }, op)?;
            }
            if let Some(facts) = facts.as_deref_mut() {
                facts.source(*span)?;
            }
        }
        match atom.kind {
            AtomKind::Copy => {
                require(
                    spans.len() == 1
                        && !spans[0].is_empty()
                        && atom.proof.is_none()
                        && atom.content == AtomContent::Empty,
                )?;
                let text = source_text(source, spans[0])?;
                let raw = text.as_bytes();
                require(
                    !(raw.first() == Some(&b'\n')
                        && spans[0].start() > 0
                        && source.text().as_bytes().get(spans[0].start() as usize - 1)
                            == Some(&b'\r')),
                )?;
                let units = text_units(text, true, flow.is_some(), id, facts.as_deref_mut(), op)?;
                require(atom.target.end - atom.target.start == units)?;
            }
            AtomKind::Transform => {
                require(all_positive && atom.target.start < atom.target.end)?;
                let value = match atom.content {
                    AtomContent::Pool(v) => v,
                    _ => return Err(CoreError::InvalidProvenance),
                };
                let units = text_units(
                    ir.text_owned(value, op)?,
                    false,
                    flow.is_some(),
                    id,
                    facts.as_deref_mut(),
                    op,
                )?;
                require(atom.target.end - atom.target.start == units)?;
                super::replay::replay(ir, source, id, payload_role, op)?;
            }
            AtomKind::Hidden => {
                require(
                    all_positive
                        && atom.target.start == atom.target.end
                        && atom.content == AtomContent::Empty
                        && !matches!(claim.kind, ClaimKind::ProjectionSeparator(_))
                        && (!matches!(claim.kind, ClaimKind::Entity | ClaimKind::Escape)
                            || atom.proof.is_some()),
                )?;
                if atom.proof.is_some() {
                    super::replay::replay(ir, source, id, payload_role, op)?;
                }
            }
            AtomKind::Synthetic => {
                require(
                    spans.len() == 1
                        && spans[0].is_empty()
                        && atom.proof.is_none()
                        && atom.target.start < atom.target.end,
                )?;
                let value = match atom.content {
                    AtomContent::Pool(v) => v,
                    _ => return Err(CoreError::InvalidProvenance),
                };
                let bytes = super::replay::pool(ir, value, op)?;
                let number =
                    claim.kind == ClaimKind::ProjectionSeparator(SyntheticKind::FootnoteNumber);
                let label =
                    claim.kind == ClaimKind::ProjectionSeparator(SyntheticKind::FootnoteLabel);
                let backlink =
                    claim.kind == ClaimKind::ProjectionSeparator(SyntheticKind::FootnoteBacklink);
                let callout =
                    claim.kind == ClaimKind::ProjectionSeparator(SyntheticKind::CalloutTitle);
                let toc = claim.kind
                    == ClaimKind::ProjectionSeparator(SyntheticKind::TableOfContentsLabel);
                require(
                    toc || bytes.len()
                        <= if backlink {
                            14
                        } else if label {
                            11
                        } else if number {
                            10
                        } else if callout {
                            9
                        } else {
                            8
                        },
                )?;
                op.scan_chunked(bytes.len() as u64)?;
                let valid = match claim.kind {
                    ClaimKind::ProjectionSeparator(
                        SyntheticKind::BlockLineFeed
                        | SyntheticKind::RowLineFeed
                        | SyntheticKind::CodeTerminalLineFeed,
                    ) => flow.is_some() && bytes == b"\n",
                    ClaimKind::ProjectionSeparator(SyntheticKind::CellTab) => {
                        flow.is_some() && bytes == b"\t"
                    }
                    ClaimKind::ProjectionSeparator(SyntheticKind::TaskLabelSpace) => {
                        flow.is_some() && bytes == b" "
                    }
                    ClaimKind::ProjectionSeparator(SyntheticKind::HttpScheme) => {
                        payload_role == Some(PayloadRole::Destination) && bytes == b"http://"
                    }
                    ClaimKind::ProjectionSeparator(SyntheticKind::MailtoScheme) => {
                        payload_role == Some(PayloadRole::Destination) && bytes == b"mailto:"
                    }
                    ClaimKind::ProjectionSeparator(
                        SyntheticKind::FootnoteNumber | SyntheticKind::FootnoteLabel,
                    ) => {
                        let digits = if label {
                            bytes.strip_suffix(b".").unwrap_or_default()
                        } else {
                            bytes
                        };
                        let expected = match node.detail.and_then(|id| ir.details().get(id.index()))
                        {
                            Some(DetailRow::FootnoteReference { ordinal, .. }) if number => {
                                Some(*ordinal)
                            }
                            Some(DetailRow::FootnoteDefinition { ordinal, .. }) if label => {
                                Some(*ordinal)
                            }
                            _ => None,
                        };
                        flow.is_some()
                            && expected.is_some_and(|number| matches_ordinal(digits, number))
                    }
                    ClaimKind::ProjectionSeparator(SyntheticKind::FootnoteBacklink) => {
                        let occurrence =
                            match node.detail.and_then(|id| ir.details().get(id.index())) {
                                Some(DetailRow::FootnoteBacklink { occurrence, .. }) => *occurrence,
                                _ => return Err(CoreError::InvalidProvenance),
                            };
                        flow.is_some()
                            && if occurrence == 1 {
                                bytes == "↩".as_bytes()
                            } else {
                                bytes
                                    .strip_prefix(" ↩".as_bytes())
                                    .is_some_and(|digits| matches_ordinal(digits, occurrence))
                            }
                    }
                    ClaimKind::ProjectionSeparator(SyntheticKind::TableOfContentsLabel) => {
                        flow.is_some()
                            && !bytes.is_empty()
                            && matches!(
                                node.detail.and_then(|id| ir.details().get(id.index())),
                                Some(DetailRow::TableOfContentsEntry { claim, .. })
                                    if *claim == atom.claim
                            )
                    }
                    ClaimKind::ProjectionSeparator(SyntheticKind::CalloutTitle) => {
                        let title = node
                            .parent
                            .and_then(|parent| ir.nodes().get(parent.index()))
                            .and_then(|quote| quote.detail)
                            .and_then(|detail| ir.details().get(detail.index()));
                        flow.is_some()
                            && matches!(title, Some(DetailRow::Callout { kind, title, .. })
                            if *title == atom.owner && bytes == kind.title().as_bytes())
                    }
                    _ => false,
                };
                require(valid)?;
                let units = if toc {
                    let text =
                        std::str::from_utf8(bytes).map_err(|_| CoreError::InvalidProvenance)?;
                    text.encode_utf16().count()
                } else if backlink {
                    bytes.len() - 2
                } else {
                    bytes.len()
                };
                require(atom.target.end - atom.target.start == units as u64)?;
                require(
                    claim.phase == NativePhase::Structure
                        || claim.phase == NativePhase::Metadata
                        || matches!(
                            claim.kind,
                            ClaimKind::ProjectionSeparator(SyntheticKind::CodeTerminalLineFeed)
                        ) && claim.phase == NativePhase::Block,
                )?;
                let context = match claim.kind {
                    ClaimKind::ProjectionSeparator(SyntheticKind::BlockLineFeed) => matches!(
                        node.kind,
                        NodeKind::Paragraph
                            | NodeKind::Heading
                            | NodeKind::CodeBlock
                            | NodeKind::Diagram
                            | NodeKind::MathBlock
                            | NodeKind::HtmlBlock
                            | NodeKind::Table
                            | NodeKind::TaskCheckbox
                            | NodeKind::FootnoteDefinition
                            | NodeKind::FootnoteFooter
                            | NodeKind::TableOfContentsEntry
                    ),
                    ClaimKind::ProjectionSeparator(SyntheticKind::CellTab) => {
                        node.kind == NodeKind::TableCell
                    }
                    ClaimKind::ProjectionSeparator(SyntheticKind::RowLineFeed) => {
                        node.kind == NodeKind::TableRow
                    }
                    ClaimKind::ProjectionSeparator(SyntheticKind::TaskLabelSpace) => matches!(
                        node.kind,
                        NodeKind::TaskCheckbox | NodeKind::Paragraph | NodeKind::ListItem
                    ),
                    ClaimKind::ProjectionSeparator(SyntheticKind::CodeTerminalLineFeed) => {
                        node.kind == NodeKind::CodeBlock
                    }
                    ClaimKind::ProjectionSeparator(SyntheticKind::FootnoteNumber) => {
                        node.kind == NodeKind::FootnoteReference
                    }
                    ClaimKind::ProjectionSeparator(SyntheticKind::FootnoteLabel) => {
                        node.kind == NodeKind::FootnoteDefinition
                    }
                    ClaimKind::ProjectionSeparator(SyntheticKind::FootnoteBacklink) => {
                        node.kind == NodeKind::FootnoteBacklink
                    }
                    ClaimKind::ProjectionSeparator(SyntheticKind::TableOfContentsLabel) => {
                        node.kind == NodeKind::TableOfContentsEntry
                    }
                    ClaimKind::ProjectionSeparator(SyntheticKind::CalloutTitle) => {
                        node.kind == NodeKind::Paragraph
                    }
                    ClaimKind::ProjectionSeparator(
                        SyntheticKind::HttpScheme | SyntheticKind::MailtoScheme,
                    ) => matches!(node.kind, NodeKind::Link | NodeKind::Image),
                    _ => false,
                };
                require(context)?;
                match claim.kind {
                    ClaimKind::ProjectionSeparator(
                        SyntheticKind::BlockLineFeed
                        | SyntheticKind::RowLineFeed
                        | SyntheticKind::CellTab,
                    ) => {
                        require(atom.target.end == node.logical.start())?;
                    }
                    ClaimKind::ProjectionSeparator(SyntheticKind::TaskLabelSpace)
                        if node.kind == NodeKind::TaskCheckbox =>
                    {
                        require(
                            atom.target.start == node.logical.end()
                                || atom.target.end == node.logical.end(),
                        )?;
                    }
                    ClaimKind::ProjectionSeparator(SyntheticKind::TaskLabelSpace)
                        if node.kind == NodeKind::Paragraph =>
                    {
                        require(atom.target.end == node.logical.start())?;
                    }
                    ClaimKind::ProjectionSeparator(SyntheticKind::CodeTerminalLineFeed) => {
                        require(
                            atom.target.end == node.logical.end()
                                && spans[0].start() == source.text().len() as u64,
                        )?;
                    }
                    ClaimKind::ProjectionSeparator(
                        SyntheticKind::FootnoteNumber
                        | SyntheticKind::FootnoteBacklink
                        | SyntheticKind::TableOfContentsLabel
                        | SyntheticKind::CalloutTitle,
                    ) => {
                        require(
                            atom.target.start == node.logical.start()
                                && atom.target.end == node.logical.end()
                                && spans[0].start() == node.source_envelope.start(),
                        )?;
                    }
                    ClaimKind::ProjectionSeparator(SyntheticKind::FootnoteLabel) => {
                        require(
                            atom.target.start == node.logical.start()
                                && atom.target.end <= node.logical.end()
                                && spans[0].start() == node.source_envelope.start(),
                        )?;
                    }
                    ClaimKind::ProjectionSeparator(
                        SyntheticKind::HttpScheme | SyntheticKind::MailtoScheme,
                    ) => {
                        require(atom.target.start == 0)?;
                    }
                    _ => {}
                }
            }
            AtomKind::Object => require(
                flow.is_some()
                    && all_positive
                    && atom.target.end - atom.target.start == 1
                    && atom.proof.is_none()
                    && matches!(atom.content, AtomContent::Object(_))
                    && (matches!(
                        claim.kind,
                        ClaimKind::Image
                            | ClaimKind::NoteEmbed
                            | ClaimKind::Task
                            | ClaimKind::Math(_)
                    ) || claim.kind == ClaimKind::Code
                        && matches!(node.kind, NodeKind::Diagram | NodeKind::MathBlock)),
            )?,
        }
        if let Some(facts) = facts.as_deref_mut() {
            facts.atom(atom.target.start < atom.target.end)?;
        }
        if atom.target.start < atom.target.end {
            if let Some(open) = &mut html_code {
                open.body(ir, id, atom, claim, op)?;
            } else {
                require(inline_html_code_detail(ir, node).is_none())?;
            }
            require(
                atom.target.start == output_cursor && emphasis.last_source_end <= spans[0].start(),
            )?;
            previous_positive_end = previous_positive_end.max(spans[spans.len() - 1].end());
            output_cursor = atom.target.end;
            positives.push(id, op)?;
        }
    }
    require(output_cursor == length && emphasis.depth == 0 && html_code.is_none())?;
    sort_rows_by_key(&mut postings, |p| (p.span.start(), p.span.end()), op)?;
    let mut posting = 0;
    for domain in scope {
        let mut cursor = domain.start();
        while cursor < domain.end() {
            op.events(1)?;
            let next = owned_row(&postings, posting, op)?.span;
            require(
                next.start() == cursor && next.end() <= domain.end() && next.start() < next.end(),
            )?;
            cursor = next.end();
            posting += 1;
        }
    }
    require(posting == postings.len())?;
    if let Some(facts) = facts {
        facts.target()?;
    }
    emphasis
        .count
        .checked_add(code_endpoints)
        .ok_or(CoreError::ArithmeticOverflow)
}

fn text_units<'id>(
    text: &str,
    copy: bool,
    flow: bool,
    id: AtomId,
    mut facts: Option<&mut DraftFacts<'id>>,
    op: &mut OwnedWork<'id, '_>,
) -> Result<u64, CoreError> {
    let collect = flow && facts.is_some();
    let begin = facts.as_deref().map_or(0, DraftFacts::checkpoint_len);
    let (mut units, mut byte, mut previous) = (0_u64, 0_u64, 0_u64);
    let mut scalars = text.chars();
    loop {
        match op.visit_scalar_batch(&mut scalars, ScalarChargeOrder::ScanThenEvent, |scalar| {
            if copy {
                require(scalar != '\r' && scalar != '\0')?;
            }
            units = units
                .checked_add(if flow {
                    scalar.len_utf16() as u64
                } else {
                    scalar.len_utf8() as u64
                })
                .ok_or(CoreError::ArithmeticOverflow)?;
            if collect {
                byte = byte
                    .checked_add(scalar.len_utf8() as u64)
                    .ok_or(CoreError::ArithmeticOverflow)?;
                if byte < text.len() as u64 && byte - previous >= SPARSE_BYTES {
                    return Ok(ControlFlow::Break(()));
                }
            }
            Ok(ControlFlow::Continue(()))
        })? {
            ScalarBatchEnd::Full => {}
            ScalarBatchEnd::Exhausted => break,
            ScalarBatchEnd::Break => {
                // The scalar visitor has completed its flag check before any allocation.
                facts
                    .as_deref_mut()
                    .ok_or_else(|| op.fail(CoreError::InternalInvariant))?
                    .checkpoint(Checkpoint { byte, utf16: units }, op)?;
                previous = byte;
            }
        }
    }
    if collect {
        facts
            .ok_or_else(|| op.fail(CoreError::InternalInvariant))?
            .atom_checkpoints(id, begin, op)?;
    }
    Ok(units)
}

fn payload_value<'id>(
    ir: &CompactIr,
    source: &SourceSnapshot,
    payload: PayloadRow,
    op: &mut OwnedWork<'id, '_>,
) -> Result<(), CoreError> {
    let value = super::replay::pool(ir, payload.value, op)?;
    require(value.len() as u64 == payload.utf8_len)?;
    let mut cursor = 0_usize;
    for index in payload.atoms.indices(ir.atoms_len())? {
        let atom = ir.owned_read_atom_at(index, op)?;
        op.events(1)?;
        let bytes = match atom.kind {
            AtomKind::Hidden => continue,
            AtomKind::Copy => source_bytes(source, atom.sources.slice(ir.sources())?[0])?,
            AtomKind::Transform | AtomKind::Synthetic => match atom.content {
                AtomContent::Pool(v) => super::replay::pool(ir, v, op)?,
                _ => return Err(CoreError::InvalidProvenance),
            },
            AtomKind::Object => return Err(CoreError::InvalidProvenance),
        };
        let end = cursor
            .checked_add(bytes.len())
            .ok_or(CoreError::ArithmeticOverflow)?;
        super::replay::equal_bytes(
            bytes,
            value.get(cursor..end).ok_or(CoreError::InvalidProvenance)?,
            op,
        )?;
        cursor = end;
    }
    require(cursor == value.len())
}

fn inline_html_break_detail(ir: &CompactIr, node: NodeRow) -> Option<ClaimId> {
    if node.kind != NodeKind::HardBreak {
        return None;
    }
    html_detail_claim(ir, node)
}

fn inline_html_code_detail(ir: &CompactIr, node: NodeRow) -> Option<ClaimId> {
    (node.kind == NodeKind::InlineCode)
        .then(|| html_detail_claim(ir, node))
        .flatten()
}

fn html_detail_claim(ir: &CompactIr, node: NodeRow) -> Option<ClaimId> {
    match ir.details().get(node.detail?.index())? {
        DetailRow::Html { claim } => Some(*claim),
        _ => None,
    }
}

fn positive_after<'id>(
    ir: &CompactIr,
    positive: Rows<AtomId>,
    positives: &BuildGrowingVec<'id, AtomId>,
    position: u64,
    op: &mut OwnedWork<'id, '_>,
) -> Result<Option<usize>, CoreError> {
    let mut left = 0;
    let mut right = positive.len();
    while left < right {
        op.events(1)?;
        let mid = left + (right - left) / 2;
        if ir.owned_atom_target_end(
            owned_row(positives, positive.start() + mid, op)?.index(),
            op,
        )? <= position
        {
            left = mid + 1;
        } else {
            right = mid;
        }
    }
    Ok((left < positive.len()).then_some(positive.start() + left))
}

fn logical_boundary<'id>(
    ir: &CompactIr,
    source: &SourceSnapshot,
    positive: Rows<AtomId>,
    positives: &BuildGrowingVec<'id, AtomId>,
    position: u64,
    facts: Option<&DraftFacts<'id>>,
    op: &mut OwnedWork<'id, '_>,
) -> Result<Option<AtomRow>, CoreError> {
    let Some(index) = positive_after(ir, positive, positives, position, op)? else {
        return Ok(None);
    };
    let id = owned_row(positives, index, op)?;
    let atom = ir.owned_read_atom_at(id.index(), op)?;
    if position == atom.target.start {
        return Ok(Some(atom));
    }
    require(atom.target.start < position && position < atom.target.end)?;
    let text = match atom.kind {
        AtomKind::Copy => source_text(source, atom.sources.slice(ir.sources())?[0])?,
        AtomKind::Transform | AtomKind::Synthetic => match atom.content {
            AtomContent::Pool(v) => ir.text_owned(v, op)?,
            _ => return Err(CoreError::InvalidProvenance),
        },
        _ => return Err(CoreError::InvalidProvenance),
    };
    let wanted = position - atom.target.start;
    if let Some(facts) = facts {
        facts.logical_boundary(id, text, wanted, op)?;
        return Ok(Some(atom));
    }
    let mut units = 0_u64;
    let mut scalars = text.chars();
    loop {
        match op.visit_scalar_batch(&mut scalars, ScalarChargeOrder::ScanThenEvent, |scalar| {
            units = units
                .checked_add(scalar.len_utf16() as u64)
                .ok_or(CoreError::ArithmeticOverflow)?;
            if units >= wanted {
                require(units == wanted)?;
                Ok(ControlFlow::Break(()))
            } else {
                Ok(ControlFlow::Continue(()))
            }
        })? {
            ScalarBatchEnd::Full => {}
            ScalarBatchEnd::Break => return Ok(Some(atom)),
            ScalarBatchEnd::Exhausted => return Err(CoreError::InvalidProvenance),
        }
    }
}

fn source_bytes(source: &SourceSnapshot, span: SourceSpan) -> Result<&[u8], CoreError> {
    Ok(source_text(source, span)?.as_bytes())
}

fn source_text(source: &SourceSnapshot, span: SourceSpan) -> Result<&str, CoreError> {
    source.validate_span(span)?;
    let start = usize::try_from(span.start()).map_err(|_| CoreError::ArithmeticOverflow)?;
    let end = usize::try_from(span.end()).map_err(|_| CoreError::ArithmeticOverflow)?;
    source
        .text()
        .get(start..end)
        .ok_or(CoreError::InvalidBoundary)
}

fn sort_rows_by_key<'id, T: Copy, K: Ord>(
    values: &mut BuildGrowingVec<'id, T>,
    key: impl Fn(T) -> K + Copy,
    op: &mut OwnedWork<'id, '_>,
) -> Result<(), CoreError> {
    crate::memory::sort_build_rows_by(values, 0, |a, b, _| Ok(key(a).cmp(&key(b))), op)
}
fn record_note_occurrence<'id>(
    counts: &mut Option<BuildRows<'id, u32>>,
    ordinal: u32,
    occurrence: u32,
    limit: u64,
    op: &mut OwnedWork<'id, '_>,
) -> Result<(), CoreError> {
    let index = ordinal.checked_sub(1).ok_or(CoreError::InvalidProvenance)? as usize;
    let counts = if let Some(counts) = counts {
        counts
    } else {
        counts.insert(BuildRows::new(limit, LimitKind::IndexRecords, op)?)
    };
    if index == counts.len() {
        require(occurrence == 1)?;
        counts.push(occurrence, op)?;
    } else {
        let expected = counts
            .get(index, op)?
            .checked_add(1)
            .ok_or(CoreError::ArithmeticOverflow)?;
        require(occurrence == expected)?;
        counts.replace(index, occurrence, op)?;
    }
    Ok(())
}

fn inline_note_sources(
    ir: &CompactIr,
    opening: AuthoritySpec,
    body: AuthoritySpec,
    closing: AuthoritySpec,
    envelope: SourceSpan,
    op: &mut OwnedWork<'_, '_>,
) -> Result<(), CoreError> {
    let first = opening.sources.slice(ir.sources())?;
    let last = closing.sources.slice(ir.sources())?;
    require(first.len() == 1 && last.len() == 1)?;
    let start = first[0].end();
    let end = last[0].start();
    require(envelope.start() <= start && start < end && end <= envelope.end())?;
    let domains = body.host.domains.slice(ir.sources())?;
    let accepted = body.sources.slice(ir.sources())?;
    let (mut lo, mut hi) = (0, domains.len());
    while lo < hi {
        op.events(1)?;
        let mid = lo + (hi - lo) / 2;
        if domains[mid].end() <= start {
            lo = mid + 1;
        } else {
            hi = mid;
        }
    }
    let mut count = 0;
    for index in lo..domains.len() {
        op.events(1)?;
        let span = domains[index];
        if span.start() >= end {
            break;
        }
        let expected = SourceSpan::new(span.start().max(start), span.end().min(end))?;
        require(accepted.get(count) == Some(&expected))?;
        count += 1;
    }
    require(count != 0 && count == accepted.len())
}

#[cfg(test)]
#[test]
fn inline_note_sources_reject_a_different_body_in_the_same_host() {
    use crate::config::{Extensions, ParseConfig, Profile};
    use crate::identity::SourceKey;
    use crate::model::AcceptedModel;
    use crate::policy::{CancellationToken, SourceBudget, construction_scope};
    use crate::source::SourceDocument;

    let budget = SourceBudget::preview();
    let mut op = budget.begin(CancellationToken::new());
    let original = SourceDocument::new(
        SourceKey::new([115; 16], 1).unwrap(),
        b"> a^[one\r\n> continued] b^[two]\r\n",
        &mut op,
    )
    .unwrap()
    .snapshot();
    let model = AcceptedModel::parse(
        original,
        ParseConfig::with_extensions(Profile::G3, Extensions::FOOTNOTES),
        1,
        &mut op,
    )
    .unwrap();
    let ir = model.ir();
    construction_scope(&mut op, |mut work| {
        let mut notes = ir.details().iter().filter_map(|detail| match *detail {
            DetailRow::FootnoteReference {
                definition,
                claim,
                closing: Some(closing),
                ..
            } => {
                let target = ir.nodes()[definition.index()];
                let DetailRow::FootnoteDefinition { claim: body, .. } =
                    ir.details()[target.detail.unwrap().index()]
                else {
                    unreachable!()
                };
                Some((claim, body, closing, target.source_envelope))
            }
            _ => None,
        });
        let (open, body, close, envelope) = notes.next().unwrap();
        let (_, other, _, _) = notes.next().unwrap();
        assert!(notes.next().is_none());
        let open = ir.owned_read_authority(open, &mut work)?;
        let mut body = ir.owned_read_authority(body, &mut work)?;
        let close = ir.owned_read_authority(close, &mut work)?;
        let other = ir.owned_read_authority(other, &mut work)?;
        assert_eq!(body.host.domains, other.host.domains);
        inline_note_sources(ir, open, body, close, envelope, &mut work)?;
        body.sources = other.sources;
        assert_eq!(
            inline_note_sources(ir, open, body, close, envelope, &mut work),
            Err(CoreError::InvalidProvenance)
        );
        work.finish()
    })
    .unwrap();
}

fn details<'id>(
    ir: &CompactIr,
    source: &SourceSnapshot,
    op: &mut OwnedWork<'id, '_>,
) -> Result<(), CoreError> {
    let mut used = zeros(ir.details().len(), op)?;
    let maximum = op
        .model_limits()
        .ok_or(CoreError::UnsupportedConfiguration)?
        .index_records;
    // One row per NodeId: immutable expected mask in the high half, observed
    // attributes in the low half. Both masks have the same lifetime and index.
    let mut attributes = BuildGrowingVec::<u32>::new(maximum, LimitKind::IndexRecords, op)?;
    let mut definition_ordinal = 0_u32;
    let mut footnote_occurrences = None;
    let mut backlink_occurrences = None;
    for (index, node) in ir.nodes().iter().enumerate() {
        op.events(1)?;
        let owner = NodeId::from_index(index)?;
        let mut expected = 0_u16;
        if let Some(id) = node.detail {
            let detail = *ir
                .details()
                .get(id.index())
                .ok_or(CoreError::InvalidProvenance)?;
            require(owned_row(&used, id.index(), op)? == 0)?;
            used.replace(id.index(), 1, op)?;
            let claim = match detail {
                DetailRow::Heading { level, form, claim } => {
                    require(
                        node.kind == NodeKind::Heading
                            && (1..=6).contains(&level)
                            && (form != HeadingForm::Setext || level <= 2),
                    )?;
                    expected =
                        attribute_mask(&[AttributeKind::HeadingLevel, AttributeKind::HeadingForm]);
                    claim
                }
                DetailRow::List {
                    marker,
                    start,
                    claim,
                    ..
                } => {
                    require(node.kind == NodeKind::List && marker_value(marker, start))?;
                    expected = attribute_mask(&[
                        AttributeKind::ListMarker,
                        AttributeKind::ListStart,
                        AttributeKind::ListTightness,
                    ]);
                    claim
                }
                DetailRow::Item {
                    marker,
                    value,
                    claim,
                    ..
                } => {
                    require(node.kind == NodeKind::ListItem && marker_value(marker, value))?;
                    expected = attribute_mask(&[AttributeKind::ItemMarker]);
                    claim
                }
                DetailRow::DescriptionItem { claim, .. } => {
                    let authority = ir.owned_read_authority(claim, op)?;
                    require(
                        node.kind == NodeKind::DescriptionItem
                            && authority.owner == owner
                            && authority.kind == ClaimKind::DescriptionMarker,
                    )?;
                    claim
                }
                DetailRow::Code { form, info, claim } => {
                    require(matches!(
                        node.kind,
                        NodeKind::CodeBlock | NodeKind::Diagram | NodeKind::MathBlock
                    ))?;
                    expected = attribute_mask(&[AttributeKind::CodeForm]);
                    if matches!(form, CodeForm::Fenced { .. }) {
                        expected |= attribute_mask(&[AttributeKind::CodeFence]);
                    }
                    if let CodeForm::Fenced { byte, length } = form {
                        require(matches!(byte, b'`' | b'~') && length >= 3)?;
                    }
                    if let Some(info) = info {
                        payload_owner(ir, info, PayloadOwner::Node(owner), PayloadRole::FenceInfo)?;
                        require(matches!(form, CodeForm::Fenced { .. }))?;
                    }
                    claim
                }
                DetailRow::Html { claim } => {
                    if node.kind == NodeKind::HtmlComment {
                        let authority = ir.owned_read_authority(claim, op)?;
                        let spans = authority.sources.slice(ir.sources())?;
                        let first = *spans.first().ok_or(CoreError::InvalidProvenance)?;
                        let last = *spans.last().ok_or(CoreError::InvalidProvenance)?;
                        op.scan(7)?;
                        require(
                            node.first_child.is_none()
                                && node.logical.is_empty()
                                && authority.kind == ClaimKind::Html
                                && authority.phase == NativePhase::Inline
                                && authority.host.role == HostRole::Inline
                                && authority.host.owner == owner
                                && first.start() == node.source_envelope.start()
                                && last.end() == node.source_envelope.end()
                                && source_text(source, first)?.starts_with("<!--")
                                && source_text(source, last)?.ends_with("-->"),
                        )?;
                    } else if matches!(node.kind, NodeKind::HardBreak | NodeKind::InlineCode) {
                        let authority = ir.owned_read_authority(claim, op)?;
                        require(
                            node.first_child.is_none()
                                && (node.kind != NodeKind::HardBreak
                                    || node.logical.end() - node.logical.start() == 1)
                                && authority.kind == ClaimKind::Html
                                && authority.phase == NativePhase::Inline
                                && authority.host.role == HostRole::Inline
                                && authority.host.owner == owner,
                        )?;
                    } else {
                        require(matches!(
                            node.kind,
                            NodeKind::HtmlBlock | NodeKind::HtmlInline
                        ))?;
                    }
                    claim
                }
                DetailRow::Table {
                    columns,
                    alignments,
                    claim,
                } => {
                    require(
                        node.kind == NodeKind::Table
                            && columns > 0
                            && alignments.slice(ir.alignments())?.len() == columns as usize,
                    )?;
                    expected = attribute_mask(&[
                        AttributeKind::TableColumns,
                        AttributeKind::TableAlignment,
                    ]);
                    claim
                }
                DetailRow::Row { claim, .. } => {
                    require(node.kind == NodeKind::TableRow)?;
                    require(
                        ir.nodes()[node.parent.ok_or(CoreError::InvalidProvenance)?.index()].kind
                            == NodeKind::Table,
                    )?;
                    expected = attribute_mask(&[AttributeKind::RowHeader]);
                    claim
                }
                DetailRow::Cell { claim, .. } => {
                    require(node.kind == NodeKind::TableCell)?;
                    require(
                        ir.nodes()[node.parent.ok_or(CoreError::InvalidProvenance)?.index()].kind
                            == NodeKind::TableRow,
                    )?;
                    expected = attribute_mask(&[AttributeKind::CellMissing]);
                    claim
                }
                DetailRow::Callout {
                    kind,
                    disclosure,
                    title,
                    claim,
                } => {
                    let header = ir
                        .nodes()
                        .get(title.index())
                        .ok_or(CoreError::InvalidProvenance)?;
                    let authority = ir.owned_read_authority(claim, op)?;
                    let marker = authority.sources.slice(ir.sources())?;
                    require(
                        node.kind == NodeKind::BlockQuote
                            && node.first_child == Some(title)
                            && header.kind == NodeKind::Paragraph
                            && header.flow == node.flow
                            && authority.kind == ClaimKind::CalloutMarker
                            && marker.len() == 1
                            && marker[0].end() <= header.source_envelope.start(),
                    )?;
                    let raw = source_bytes(source, marker[0])?;
                    require(raw.len() <= 20)?;
                    op.scan(raw.len() as u64)?;
                    require(comrak::native::callout_marker(raw) == Some((kind, disclosure)))?;
                    claim
                }
                DetailRow::FootnoteDefinition { ordinal, claim } => {
                    definition_ordinal = definition_ordinal
                        .checked_add(1)
                        .ok_or(CoreError::ArithmeticOverflow)?;
                    require(
                        node.kind == NodeKind::FootnoteDefinition
                            && ordinal == definition_ordinal
                            && matches!(
                                ir.owned_read_authority(claim, op)?.kind,
                                ClaimKind::FootnoteMarker | ClaimKind::InlineFootnoteBody
                            ),
                    )?;
                    claim
                }
                DetailRow::FootnoteReference {
                    definition,
                    ordinal,
                    occurrence,
                    claim,
                    closing,
                } => {
                    let target = ir
                        .nodes()
                        .get(definition.index())
                        .ok_or(CoreError::InvalidProvenance)?;
                    let target_detail = target.detail.ok_or(CoreError::InvalidProvenance)?;
                    let Some(DetailRow::FootnoteDefinition {
                        ordinal: number,
                        claim: target_claim,
                    }) = ir.details().get(target_detail.index())
                    else {
                        return Err(CoreError::InvalidProvenance);
                    };
                    require(
                        node.kind == NodeKind::FootnoteReference
                            && node.first_child.is_none()
                            && target.kind == NodeKind::FootnoteDefinition
                            && target.flow == node.flow
                            && *number == ordinal,
                    )?;
                    let opening = ir.owned_read_authority(claim, op)?;
                    let body = ir.owned_read_authority(*target_claim, op)?;
                    if let Some(closing_claim) = closing {
                        let closing = ir.owned_read_authority(closing_claim, op)?;
                        require(
                            occurrence == 1
                                && opening.kind == ClaimKind::InlineFootnoteOpening
                                && body.kind == ClaimKind::InlineFootnoteBody
                                && closing.kind == ClaimKind::InlineFootnoteClosing
                                && opening.owner == closing.owner
                                && body.owner == definition
                                && opening.host.domains == body.host.domains
                                && opening.host.domains == closing.host.domains
                                && opening.host.utf8_len == body.host.utf8_len
                                && opening.host.utf8_len == closing.host.utf8_len
                                && opening.consumed.end() == body.consumed.start()
                                && body.consumed.end() == closing.consumed.start(),
                        )?;
                        inline_note_sources(
                            ir,
                            opening,
                            body,
                            closing,
                            target.source_envelope,
                            op,
                        )?;
                    } else {
                        require(
                            opening.kind == ClaimKind::FootnoteReference
                                && body.kind == ClaimKind::FootnoteMarker,
                        )?;
                    }
                    record_note_occurrence(
                        &mut footnote_occurrences,
                        ordinal,
                        occurrence,
                        maximum,
                        op,
                    )?;
                    claim
                }
                DetailRow::FootnoteBacklink {
                    reference,
                    ordinal,
                    occurrence,
                    claim,
                } => {
                    let footer = ir
                        .nodes()
                        .get(node.parent.ok_or(CoreError::InvalidProvenance)?.index())
                        .ok_or(CoreError::InvalidProvenance)?;
                    let definition = footer.parent.ok_or(CoreError::InvalidProvenance)?;
                    let target = ir
                        .nodes()
                        .get(reference.index())
                        .ok_or(CoreError::InvalidProvenance)?;
                    let target_detail = target.detail.and_then(|id| ir.details().get(id.index()));
                    require(
                        node.kind == NodeKind::FootnoteBacklink
                            && node.first_child.is_none()
                            && footer.kind == NodeKind::FootnoteFooter
                            && node.source_envelope == footer.source_envelope
                            && target.kind == NodeKind::FootnoteReference
                            && target.flow == node.flow
                            && matches!(target_detail, Some(DetailRow::FootnoteReference { definition: note, ordinal: number, occurrence: instance, .. }) if *note == definition && *number == ordinal && *instance == occurrence)
                            && ir.owned_read_authority(claim, op)?.kind
                                == ClaimKind::ProjectionSeparator(SyntheticKind::FootnoteBacklink),
                    )?;
                    record_note_occurrence(
                        &mut backlink_occurrences,
                        ordinal,
                        occurrence,
                        maximum,
                        op,
                    )?;
                    claim
                }
                DetailRow::TableOfContentsEntry {
                    heading,
                    depth,
                    claim,
                } => {
                    let parent = node
                        .parent
                        .and_then(|id| ir.nodes().get(id.index()))
                        .ok_or(CoreError::InvalidProvenance)?;
                    let heading = ir
                        .nodes()
                        .get(heading.index())
                        .ok_or(CoreError::InvalidProvenance)?;
                    require(
                        node.kind == NodeKind::TableOfContentsEntry
                            && node.first_child.is_none()
                            && parent.kind == NodeKind::TableOfContents
                            && node.source_envelope.is_empty()
                            && node.source_envelope.end() == parent.source_envelope.end()
                            && heading.kind == NodeKind::Heading
                            && ir.flows()[node.flow.index()].scope == FlowScope::Main
                            && heading.flow == node.flow
                            && depth < 6
                            && ir.owned_read_authority(claim, op)?.kind
                                == ClaimKind::ProjectionSeparator(
                                    SyntheticKind::TableOfContentsLabel,
                                ),
                    )?;
                    claim
                }
                DetailRow::Link {
                    destination,
                    title,
                    reference,
                    claim,
                } => {
                    require(node.kind == NodeKind::Link)?;
                    payload_owner(
                        ir,
                        destination,
                        PayloadOwner::Node(owner),
                        PayloadRole::Destination,
                    )?;
                    let authority = ir.owned_read_authority(claim, op)?;
                    if authority.kind == ClaimKind::WikiLink {
                        require(title.is_none() && reference.is_none())?;
                        wiki_alias(ir, source, destination, authority.sources, "[[", op)?;
                    }
                    if let Some(title) = title {
                        payload_owner(ir, title, PayloadOwner::Node(owner), PayloadRole::Title)?;
                    }
                    if let Some(reference) = reference {
                        let r = ir
                            .references()
                            .get(reference.index())
                            .ok_or(CoreError::InvalidProvenance)?;
                        require(
                            r.node == owner && r.destination == destination && r.title == title,
                        )?;
                    }
                    claim
                }
            };
            require(ir.owned_read_authority(claim, op)?.owner == owner)?;
        } else {
            require(!matches!(
                node.kind,
                NodeKind::Heading
                    | NodeKind::List
                    | NodeKind::ListItem
                    | NodeKind::DescriptionItem
                    | NodeKind::CodeBlock
                    | NodeKind::HtmlBlock
                    | NodeKind::HtmlInline
                    | NodeKind::HtmlComment
                    | NodeKind::Table
                    | NodeKind::TableRow
                    | NodeKind::TableCell
                    | NodeKind::FootnoteDefinition
                    | NodeKind::FootnoteReference
                    | NodeKind::FootnoteBacklink
                    | NodeKind::TableOfContentsEntry
                    | NodeKind::Link
            ))?;
        }
        attributes.push(u32::from(expected) << 16, op)?;
        if node.kind == NodeKind::ListItem {
            let parent = ir
                .nodes()
                .get(node.parent.ok_or(CoreError::InvalidProvenance)?.index())
                .ok_or(CoreError::InvalidProvenance)?;
            require(parent.kind == NodeKind::List)?;
            let (marker, value) = match ir.details()[node.detail.unwrap().index()] {
                DetailRow::Item { marker, value, .. } => (marker, value),
                _ => return Err(CoreError::InvalidProvenance),
            };
            let (list_marker, list_start) =
                match ir.details()[parent.detail.ok_or(CoreError::InvalidProvenance)?.index()] {
                    DetailRow::List { marker, start, .. } => (marker, start),
                    _ => return Err(CoreError::InvalidProvenance),
                };
            require(marker == list_marker)?;
            if parent.first_child == Some(owner) {
                require(value == list_start)?;
            }
        }
        if node.kind == NodeKind::Table {
            table_children(ir, owner, op)?;
        }
        if node.kind == NodeKind::List {
            let mut item = node.first_child;
            let mut occurrence = 0_u32;
            while let Some(id) = item {
                op.events(1)?;
                let child = ir.nodes()[id.index()];
                require(child.kind == NodeKind::ListItem)?;
                require(
                    matches!(ir.details()[child.detail.ok_or(CoreError::InvalidProvenance)?.index()], DetailRow::Item { occurrence: actual, .. } if actual == occurrence),
                )?;
                occurrence = occurrence
                    .checked_add(1)
                    .ok_or(CoreError::ArithmeticOverflow)?;
                item = child.next_sibling;
            }
            require(occurrence > 0)?;
        }
    }
    require(
        footnote_occurrences.as_ref().map_or(0, BuildRows::len) == definition_ordinal as usize,
    )?;
    match (footnote_occurrences, backlink_occurrences) {
        (Some(references), Some(backlinks)) => {
            require(references.len() == backlinks.len())?;
            for index in 0..references.len() {
                require(references.get(index, op)? == backlinks.get(index, op)?)?;
            }
        }
        (None, None) => {}
        _ => return Err(CoreError::InvalidProvenance),
    }
    check_flags(&used, 1, op)?;
    for attribute in ir.attributes() {
        op.events(1)?;
        let node = ir.nodes()[attribute.owner.index()];
        let valid = match attribute.kind {
            AttributeKind::HeadingLevel | AttributeKind::HeadingForm => {
                node.kind == NodeKind::Heading
            }
            AttributeKind::ListMarker | AttributeKind::ListStart | AttributeKind::ListTightness => {
                node.kind == NodeKind::List
            }
            AttributeKind::ItemMarker => node.kind == NodeKind::ListItem,
            AttributeKind::CodeForm | AttributeKind::CodeFence => {
                matches!(
                    node.kind,
                    NodeKind::CodeBlock | NodeKind::Diagram | NodeKind::MathBlock
                )
            }
            AttributeKind::TableColumns | AttributeKind::TableAlignment => {
                node.kind == NodeKind::Table
            }
            AttributeKind::RowHeader => node.kind == NodeKind::TableRow,
            AttributeKind::CellMissing => node.kind == NodeKind::TableCell,
        };
        require(valid)?;
        let mask = attribute_mask(&[attribute.kind]);
        let previous = owned_row(&attributes, attribute.owner.index(), op)?;
        require((previous as u16) & mask == 0)?;
        attributes.replace(attribute.owner.index(), previous | u32::from(mask), op)?;
        let _ = parts(ir, source, attribute.sources, op)?;
        attribute_value(ir, source, *attribute, op)?;
    }
    for index in 0..attributes.len() {
        let masks = owned_row(&attributes, index, op)?;
        op.events(1)?;
        require((masks >> 16) as u16 == masks as u16)?;
    }
    Ok(())
}

fn attribute_mask(kinds: &[AttributeKind]) -> u16 {
    kinds.iter().fold(0, |mask, kind| mask | (1 << *kind as u8))
}

fn attribute_value<'id>(
    ir: &CompactIr,
    source: &SourceSnapshot,
    attribute: AttributeRow,
    op: &mut OwnedWork<'id, '_>,
) -> Result<(), CoreError> {
    let node = ir.nodes()[attribute.owner.index()];
    let detail = ir.details()[node.detail.ok_or(CoreError::InvalidProvenance)?.index()];
    let spans = attribute.sources.slice(ir.sources())?;
    // These are exact syntax witnesses. Derived structural values retain their
    // native structural claim; replay does not run a second Markdown parser.
    match (attribute.kind, detail) {
        (
            AttributeKind::HeadingLevel | AttributeKind::HeadingForm,
            DetailRow::Heading { level, form, .. },
        ) => {
            let expected = match form {
                HeadingForm::Atx => b'#',
                HeadingForm::Setext if level == 1 => b'=',
                HeadingForm::Setext => b'-',
            };
            let mut count = 0_u64;
            for span in spans {
                for chunk in source_bytes(source, *span)?.chunks(CHECKPOINT_BYTES as usize) {
                    op.scan(chunk.len() as u64)?;
                    require(chunk.iter().all(|byte| *byte == expected))?;
                    count = count
                        .checked_add(chunk.len() as u64)
                        .ok_or(CoreError::ArithmeticOverflow)?;
                }
            }
            require(count > 0 && (form != HeadingForm::Atx || count == level as u64))?;
        }
        (
            AttributeKind::ListMarker | AttributeKind::ListStart,
            DetailRow::List {
                marker,
                start: value,
                ..
            },
        )
        | (AttributeKind::ItemMarker, DetailRow::Item { marker, value, .. }) => {
            let mut buffer = [0_u8; 10];
            let mut len = 0_usize;
            for span in spans {
                let raw = source_bytes(source, *span)?;
                let end = len
                    .checked_add(raw.len())
                    .ok_or(CoreError::ArithmeticOverflow)?;
                require(end <= buffer.len())?;
                op.scan(raw.len() as u64)?;
                buffer[len..end].copy_from_slice(raw);
                len = end;
            }
            let raw = &buffer[..len];
            match marker {
                ListMarker::Bullet(byte) => require(raw == [byte])?,
                ListMarker::Ordered { delimiter } => {
                    require((2..=10).contains(&raw.len()) && raw.last() == Some(&delimiter))?;
                    let mut number = 0_u64;
                    for byte in &raw[..raw.len() - 1] {
                        require(byte.is_ascii_digit())?;
                        number = number
                            .checked_mul(10)
                            .and_then(|n| n.checked_add((byte - b'0') as u64))
                            .ok_or(CoreError::ArithmeticOverflow)?;
                    }
                    require(number == value)?;
                }
            }
        }
        (
            AttributeKind::CodeFence,
            DetailRow::Code {
                form: CodeForm::Fenced { byte, length },
                ..
            },
        ) => {
            let mut count = 0_u64;
            for span in spans {
                for chunk in source_bytes(source, *span)?.chunks(CHECKPOINT_BYTES as usize) {
                    op.scan(chunk.len() as u64)?;
                    require(chunk.iter().all(|actual| *actual == byte))?;
                    count = count
                        .checked_add(chunk.len() as u64)
                        .ok_or(CoreError::ArithmeticOverflow)?;
                }
            }
            require(count == length as u64)?;
        }
        (
            AttributeKind::TableAlignment,
            DetailRow::Table {
                columns,
                alignments,
                ..
            },
        ) => {
            let values = alignments.slice(ir.alignments())?;
            require(spans.len() == columns as usize && spans.len() == values.len())?;
            // One accepted delimiter-cell witness per column, in native column
            // order. Only the recorded colon attribute is replayed here.
            for (span, alignment) in spans.iter().zip(values) {
                op.events(1)?;
                let raw = source_bytes(source, *span)?;
                let mut start = 0;
                let mut end = raw.len();
                while start < end {
                    op.scan(1)?;
                    if !matches!(raw[start], b' ' | b'\t') {
                        break;
                    }
                    start += 1;
                }
                while end > start {
                    op.scan(1)?;
                    if !matches!(raw[end - 1], b' ' | b'\t') {
                        break;
                    }
                    end -= 1;
                }
                require(start < end)?;
                op.scan(2)?;
                let left = raw[start] == b':';
                let right = raw[end - 1] == b':';
                if left {
                    start += 1;
                }
                if right {
                    end -= 1;
                }
                require(start < end)?;
                for chunk in raw[start..end].chunks(CHECKPOINT_BYTES as usize) {
                    op.scan(chunk.len() as u64)?;
                    require(chunk.iter().all(|byte| *byte == b'-'))?;
                }
                let actual = match (left, right) {
                    (false, false) => Alignment::None,
                    (true, false) => Alignment::Left,
                    (false, true) => Alignment::Right,
                    (true, true) => Alignment::Center,
                };
                require(*alignment == actual)?;
            }
        }
        _ => {
            let mut positive = false;
            for span in spans {
                op.events(1)?;
                positive |= !span.is_empty();
            }
            require(positive || matches!(attribute.kind, AttributeKind::CellMissing))?;
        }
    }
    Ok(())
}

fn marker_value(marker: ListMarker, value: u64) -> bool {
    match marker {
        ListMarker::Bullet(byte) => matches!(byte, b'-' | b'+' | b'*') && value == 0,
        ListMarker::Ordered { delimiter } => {
            matches!(delimiter, b'.' | b')') && value <= 999_999_999
        }
    }
}

fn table_children<'id>(
    ir: &CompactIr,
    table: NodeId,
    op: &mut OwnedWork<'id, '_>,
) -> Result<(), CoreError> {
    let node = ir.nodes()[table.index()];
    let columns = match ir.details()[node.detail.ok_or(CoreError::InvalidProvenance)?.index()] {
        DetailRow::Table { columns, .. } => columns,
        _ => return Err(CoreError::InvalidProvenance),
    };
    let mut row = node.first_child;
    let mut row_index = 0;
    while let Some(id) = row {
        op.events(1)?;
        let row_node = ir.nodes()[id.index()];
        require(row_node.kind == NodeKind::TableRow)?;
        match ir.details()[row_node.detail.ok_or(CoreError::InvalidProvenance)?.index()] {
            DetailRow::Row { index, header, .. } => {
                require(index == row_index && header == (row_index == 0))?
            }
            _ => return Err(CoreError::InvalidProvenance),
        }
        let mut cell = row_node.first_child;
        let mut column = 0;
        while let Some(id) = cell {
            op.events(1)?;
            let cell_node = ir.nodes()[id.index()];
            require(cell_node.kind == NodeKind::TableCell)?;
            match ir.details()[cell_node
                .detail
                .ok_or(CoreError::InvalidProvenance)?
                .index()]
            {
                DetailRow::Cell {
                    column: actual,
                    missing,
                    ..
                } => {
                    require(actual == column)?;
                    if missing {
                        require(
                            cell_node.source_envelope.is_empty()
                                && cell_node.logical.is_empty()
                                && cell_node.first_child.is_none(),
                        )?;
                    }
                }
                _ => return Err(CoreError::InvalidProvenance),
            }
            column += 1;
            cell = cell_node.next_sibling;
        }
        require(column == columns)?;
        row_index += 1;
        row = row_node.next_sibling;
    }
    require(row_index > 0)
}

fn payload_owner(
    ir: &CompactIr,
    id: PayloadId,
    owner: PayloadOwner,
    role: PayloadRole,
) -> Result<(), CoreError> {
    let row = ir
        .payloads()
        .get(id.index())
        .ok_or(CoreError::InvalidProvenance)?;
    require(row.owner == owner && row.role == role)
}

fn wiki_body<'a>(
    source: &'a SourceSnapshot,
    spans: RowView<'_, SourceSpan>,
    prefix: &str,
    op: &mut OwnedWork<'_, '_>,
) -> Result<(&'a str, u64), CoreError> {
    let first = spans.first().ok_or(CoreError::InvalidProvenance)?;
    let last = spans.last().ok_or(CoreError::InvalidProvenance)?;
    let raw = source_text(source, SourceSpan::new(first.start(), last.end())?)?;
    require((5..=4004).contains(&raw.len()))?;
    op.scan(raw.len() as u64)?;
    let body = raw
        .strip_prefix(prefix)
        .and_then(|raw| raw.strip_suffix("]]"))
        .ok_or(CoreError::InvalidProvenance)?;
    require(!body.contains(['\r', '\n']))?;
    Ok((body, first.start() + prefix.len() as u64))
}

fn wiki_alias<'a>(
    ir: &CompactIr,
    source: &'a SourceSnapshot,
    destination: PayloadId,
    syntax: Rows<SourceSpan>,
    prefix: &str,
    op: &mut OwnedWork<'_, '_>,
) -> Result<Option<&'a str>, CoreError> {
    let (body, start) = wiki_body(source, syntax.slice(ir.sources())?, prefix, op)?;
    let target = ir.payloads()[destination.index()]
        .domains
        .slice(ir.sources())?;
    let first = target.first().ok_or(CoreError::InvalidProvenance)?;
    let last = target.last().ok_or(CoreError::InvalidProvenance)?;
    let trimmed = body.trim_start_matches([' ', '\t']);
    require(
        first.start() == start + (body.len() - trimmed.len()) as u64 && last.end() > first.start(),
    )?;
    let end = last
        .end()
        .checked_sub(start)
        .ok_or(CoreError::InvalidProvenance)?;
    let rest = body
        .get(end as usize..)
        .ok_or(CoreError::InvalidProvenance)?
        .trim_start_matches([' ', '\t']);
    if rest.is_empty() {
        Ok(None)
    } else {
        let alias = rest
            .strip_prefix('|')
            .or_else(|| rest.strip_prefix("\\|"))
            .ok_or(CoreError::InvalidProvenance)?
            .trim_matches([' ', '\t']);
        require(!alias.is_empty())?;
        Ok(Some(alias))
    }
}

fn payloads<'id>(ir: &CompactIr, op: &mut OwnedWork<'id, '_>) -> Result<(), CoreError> {
    let mut owned = zeros(ir.payloads().len(), op)?;
    fn bind<'id>(
        ir: &CompactIr,
        owned: &mut BuildGrowingVec<'id, u8>,
        id: PayloadId,
        owner: PayloadOwner,
        role: PayloadRole,
        op: &mut OwnedWork<'id, '_>,
    ) -> Result<(), CoreError> {
        op.events(1)?;
        payload_owner(ir, id, owner, role)?;
        require(owned_row(owned, id.index(), op)? == 0)?;
        owned.replace(id.index(), 1, op)
    }
    for (index, node) in ir.nodes().iter().enumerate() {
        op.events(1)?;
        let owner = PayloadOwner::Node(NodeId::from_index(index)?);
        if let Some(detail) = node.detail {
            match ir.details()[detail.index()] {
                DetailRow::Code {
                    info: Some(info), ..
                } => bind(ir, &mut owned, info, owner, PayloadRole::FenceInfo, op)?,
                DetailRow::Link {
                    destination, title, ..
                } => {
                    bind(
                        ir,
                        &mut owned,
                        destination,
                        owner,
                        PayloadRole::Destination,
                        op,
                    )?;
                    if let Some(title) = title {
                        bind(ir, &mut owned, title, owner, PayloadRole::Title, op)?;
                    }
                }
                _ => {}
            }
        }
    }
    for object in ir.objects() {
        op.events(1)?;
        if let ObjectRow::Derived {
            node, input, kind, ..
        } = *object
        {
            let role = match kind {
                DerivedKind::Math(MathSyntax::FencedDisplay) | DerivedKind::Diagram => {
                    PayloadRole::FenceInput
                }
                DerivedKind::Math(_) => PayloadRole::MathInput,
                DerivedKind::NoteEmbed => PayloadRole::Destination,
            };
            bind(ir, &mut owned, input, PayloadOwner::Node(node), role, op)?;
        }
        if let ObjectRow::Image {
            node,
            destination,
            title,
            ..
        } = *object
        {
            let owner = PayloadOwner::Node(node);
            bind(
                ir,
                &mut owned,
                destination,
                owner,
                PayloadRole::Destination,
                op,
            )?;
            if let Some(title) = title {
                bind(ir, &mut owned, title, owner, PayloadRole::Title, op)?;
            }
        }
    }
    for (index, definition) in ir.definitions().iter().enumerate() {
        op.events(1)?;
        let owner = PayloadOwner::Definition(DefinitionId::from_index(index)?);
        bind(
            ir,
            &mut owned,
            definition.label,
            owner,
            PayloadRole::ReferenceLabel,
            op,
        )?;
        bind(
            ir,
            &mut owned,
            definition.destination,
            owner,
            PayloadRole::Destination,
            op,
        )?;
        if let Some(title) = definition.title {
            bind(ir, &mut owned, title, owner, PayloadRole::Title, op)?;
        }
    }
    for index in 0..owned.len() {
        let flag = owned_row(&owned, index, op)?;
        op.events(1)?;
        require(flag == 1)?;
    }
    Ok(())
}

fn objects<'id>(
    ir: &CompactIr,
    source: &SourceSnapshot,
    op: &mut OwnedWork<'id, '_>,
) -> Result<(), CoreError> {
    let mut claimed = zeros(ir.atoms_len(), op)?;
    let mut claimed_nodes = zeros(ir.nodes().len(), op)?;
    for (index, object) in ir.objects().iter().enumerate() {
        op.events(1)?;
        let id = ObjectId::from_index(index)?;
        let (node_id, atom_id) = match *object {
            ObjectRow::Image { node, atom, .. }
            | ObjectRow::Task { node, atom, .. }
            | ObjectRow::Derived { node, atom, .. } => (node, atom),
        };
        let node = *ir
            .nodes()
            .get(node_id.index())
            .ok_or(CoreError::InvalidProvenance)?;
        let atom = ir.owned_read_atom_at(atom_id.index(), op)?;
        require(
            atom.owner == node_id
                && atom.kind == AtomKind::Object
                && atom.content == AtomContent::Object(id)
                && owned_row(&claimed, atom_id.index(), op)? == 0,
        )?;
        claimed.replace(atom_id.index(), 1, op)?;
        require(owned_row(&claimed_nodes, node_id.index(), op)? == 0)?;
        claimed_nodes.replace(node_id.index(), 1, op)?;
        match *object {
            ObjectRow::Derived {
                kind,
                input,
                export_sources,
                ..
            } => {
                derived::validate(ir, source, node_id, atom, kind, input, export_sources, op)?;
            }
            ObjectRow::Image {
                alt,
                destination,
                title,
                reference,
                wiki_size,
                ..
            } => {
                require(
                    node.kind == NodeKind::Image
                        && ir
                            .flows()
                            .get(alt.index())
                            .is_some_and(|flow| flow.scope == FlowScope::ImageAlt { object: id }),
                )?;
                payload_owner(
                    ir,
                    destination,
                    PayloadOwner::Node(node_id),
                    PayloadRole::Destination,
                )?;
                if let Some(size) = wiki_size {
                    require(title.is_none() && reference.is_none())?;
                    let target = ir.text_owned(ir.payloads()[destination.index()].value, op)?;
                    op.scan(target.len() as u64)?;
                    require(comrak::native::wiki_image_target(target))?;
                    let alias = wiki_alias(ir, source, destination, atom.sources, "![[", op)?;
                    require(
                        alias
                            .and_then(comrak::native::wiki_image_dimensions)
                            .unwrap_or_default()
                            == size,
                    )?;
                }
                if let Some(title) = title {
                    payload_owner(ir, title, PayloadOwner::Node(node_id), PayloadRole::Title)?;
                }
                if let Some(reference) = reference {
                    let row = ir
                        .references()
                        .get(reference.index())
                        .ok_or(CoreError::InvalidProvenance)?;
                    require(
                        row.node == node_id && row.destination == destination && row.title == title,
                    )?;
                }
            }
            ObjectRow::Task {
                item,
                checked,
                marker,
                spacer,
                action,
                ..
            } => {
                require(
                    node.kind == NodeKind::TaskCheckbox
                        && ir.flows()[node.flow.index()].scope == FlowScope::Main,
                )?;
                let item_row = *ir
                    .nodes()
                    .get(item.index())
                    .ok_or(CoreError::InvalidProvenance)?;
                require(item_row.kind == NodeKind::ListItem && related(ir, item, node_id, op)?)?;
                let marker = parts(ir, source, marker, op)?;
                let spacer = parts(ir, source, spacer, op)?;
                let mut raw = [0_u8; 8];
                let mut len = 0_usize;
                for span in marker {
                    let bytes = source_bytes(source, *span)?;
                    require(bytes.len() <= raw.len())?;
                    op.scan(bytes.len() as u64)?;
                    let end = len
                        .checked_add(bytes.len())
                        .ok_or(CoreError::ArithmeticOverflow)?;
                    require(end <= raw.len())?;
                    raw[len..end].copy_from_slice(bytes);
                    len = end;
                }
                let raw = &raw[..len];
                require(raw.first() == Some(&b'[') && raw.last() == Some(&b']'))?;
                let middle = raw
                    .get(1..raw.len().saturating_sub(1))
                    .ok_or(CoreError::InvalidProvenance)?;
                require(
                    matches!(
                        middle,
                        b" " | b"\t" | b"\n" | b"\r" | b"\r\n" | b"\x0b" | b"\x0c" | b"x" | b"X"
                    ) && checked == matches!(middle, b"x" | b"X")
                        && !spacer.is_empty(),
                )?;
                let mut whitespace = false;
                for span in spacer {
                    let bytes = source_bytes(source, *span)?;
                    for chunk in bytes.chunks(CHECKPOINT_BYTES as usize) {
                        op.scan(chunk.len() as u64)?;
                        require(chunk.iter().all(|b| {
                            matches!(b, b' ' | b'\t' | b'\n' | b'\r' | b'\x0b' | b'\x0c')
                        }))?;
                        whitespace |= !chunk.is_empty();
                    }
                }
                require(whitespace)?;
                if let Some(action) = action {
                    op.scan(3)?;
                    require(
                        marker.len() == 1
                            && marker[0] == action.source
                            && action.source.len() == 3
                            && matches!(&action.expected, b"[ ]" | b"[x]" | b"[X]")
                            && source_bytes(source, action.source)? == action.expected,
                    )?;
                }
            }
        }
    }
    for index in 0..ir.atoms_len() {
        let atom = ir.owned_read_atom_at(index, op)?;
        op.events(1)?;
        require((atom.kind == AtomKind::Object) == (owned_row(&claimed, index, op)? == 1))?;
    }
    for (index, node) in ir.nodes().iter().enumerate() {
        op.events(1)?;
        require(
            matches!(
                node.kind,
                NodeKind::Image
                    | NodeKind::TaskCheckbox
                    | NodeKind::Math
                    | NodeKind::Diagram
                    | NodeKind::MathBlock
                    | NodeKind::NoteEmbed
            ) == (owned_row(&claimed_nodes, index, op)? == 1),
        )?;
    }
    Ok(())
}

fn references<'id>(
    ir: &CompactIr,
    source: &SourceSnapshot,
    op: &mut OwnedWork<'id, '_>,
) -> Result<(), CoreError> {
    let mut previous_definition_end = 0_u64;
    for (index, definition) in ir.definitions().iter().enumerate() {
        op.events(1)?;
        let id = DefinitionId::from_index(index)?;
        let claim = ir.owned_read_authority(definition.claim, op)?;
        let syntax = parts(ir, source, definition.syntax, op)?;
        require(
            claim.kind == ClaimKind::ReferenceDefinition
                && !syntax.is_empty()
                && previous_definition_end <= syntax[0].start(),
        )?;
        previous_definition_end = syntax[syntax.len() - 1].end();
        for span in syntax {
            require(super::replay::covered(
                *span,
                claim.sources.slice(ir.sources())?,
                op,
            )?)?;
        }
        payload_owner(
            ir,
            definition.label,
            PayloadOwner::Definition(id),
            PayloadRole::ReferenceLabel,
        )?;
        payload_owner(
            ir,
            definition.destination,
            PayloadOwner::Definition(id),
            PayloadRole::Destination,
        )?;
        if let Some(title) = definition.title {
            payload_owner(ir, title, PayloadOwner::Definition(id), PayloadRole::Title)?;
        }
        require(!super::replay::pool(ir, definition.key, op)?.is_empty())?;
        for payload in [
            Some(definition.label),
            Some(definition.destination),
            definition.title,
        ]
        .into_iter()
        .flatten()
        {
            op.events(1)?;
            for span in ir.payloads()[payload.index()].domains.slice(ir.sources())? {
                require(super::replay::covered(*span, syntax, op)?)?;
            }
        }
    }
    let maximum = op
        .model_limits()
        .ok_or(CoreError::UnsupportedConfiguration)?
        .index_records;
    // Check final winner facts against source-order occurrences of the same
    // accepted key. This does not normalize a label or re-run grammar lookup.
    let mut definitions =
        BuildGrowingVec::<DefinitionId>::new(maximum, LimitKind::IndexRecords, op)?;
    for index in 0..ir.definitions().len() {
        definitions.push(DefinitionId::from_index(index)?, op)?;
    }
    sort_rows_by(
        &mut definitions,
        |left, right, op| {
            let a = super::replay::pool(ir, ir.definitions()[left.index()].key, op)?;
            let b = super::replay::pool(ir, ir.definitions()[right.index()].key, op)?;
            let order = compare_bytes(a, b, op)?;
            Ok(order.then_with(|| left.cmp(&right)))
        },
        op,
    )?;
    let mut previous: Option<DefinitionId> = None;
    for index in 0..definitions.len() {
        let id = owned_row(&definitions, index, op)?;
        op.events(1)?;
        let definition = ir.definitions()[id.index()];
        let same = if let Some(previous) = previous {
            compare_bytes(
                super::replay::pool(ir, ir.definitions()[previous.index()].key, op)?,
                super::replay::pool(ir, definition.key, op)?,
                op,
            )? == std::cmp::Ordering::Equal
        } else {
            false
        };
        require(definition.winner == !same)?;
        previous = Some(id);
    }
    // Sort only charged indices, never duplicate keys or build another winner map.
    let mut uses = BuildGrowingVec::<u32>::new(maximum, LimitKind::IndexRecords, op)?;
    let mut used = zeros(ir.references().len(), op)?;
    for reference in ir.references() {
        op.events(1)?;
        let node = ir
            .nodes()
            .get(reference.node.index())
            .ok_or(CoreError::InvalidProvenance)?;
        require(matches!(node.kind, NodeKind::Link | NodeKind::Image))?;
        let definition = *ir
            .definitions()
            .get(reference.winner.index())
            .ok_or(CoreError::InvalidProvenance)?;
        let claim = ir.owned_read_authority(reference.claim, op)?;
        require(
            definition.winner
                && claim.kind == ClaimKind::ReferenceUse
                && claim.owner == reference.node,
        )?;
        super::replay::equal_bytes(
            super::replay::pool(ir, reference.key, op)?,
            super::replay::pool(ir, definition.key, op)?,
            op,
        )?;
        let syntax = parts(ir, source, reference.syntax, op)?;
        require(!syntax.is_empty())?;
        for span in syntax {
            require(super::replay::covered(
                *span,
                claim.sources.slice(ir.sources())?,
                op,
            )?)?;
        }
        payload_owner(
            ir,
            reference.destination,
            PayloadOwner::Node(reference.node),
            PayloadRole::Destination,
        )?;
        let dst = ir.payloads()[reference.destination.index()];
        let original = ir.payloads()[definition.destination.index()];
        super::replay::equal_bytes(
            super::replay::pool(ir, dst.value, op)?,
            super::replay::pool(ir, original.value, op)?,
            op,
        )?;
        equal_spans(
            dst.domains.slice(ir.sources())?,
            original.domains.slice(ir.sources())?,
            op,
        )?;
        require(reference.title.is_some() == definition.title.is_some())?;
        if let (Some(title), Some(original)) = (reference.title, definition.title) {
            payload_owner(
                ir,
                title,
                PayloadOwner::Node(reference.node),
                PayloadRole::Title,
            )?;
            super::replay::equal_bytes(
                super::replay::pool(ir, ir.payloads()[title.index()].value, op)?,
                super::replay::pool(ir, ir.payloads()[original.index()].value, op)?,
                op,
            )?;
            equal_spans(
                ir.payloads()[title.index()].domains.slice(ir.sources())?,
                ir.payloads()[original.index()]
                    .domains
                    .slice(ir.sources())?,
                op,
            )?;
        }
        uses.push(reference.node.value(), op)?;
    }
    sort_rows_by_key(&mut uses, |value| value, op)?;
    for index in 1..uses.len() {
        let pair = [
            owned_row(&uses, index - 1, op)?,
            owned_row(&uses, index, op)?,
        ];
        op.events(1)?;
        require(pair[0] != pair[1])?;
    }
    for node in ir.nodes() {
        op.events(1)?;
        if let Some(detail) = node.detail
            && let DetailRow::Link {
                reference: Some(id),
                ..
            } = ir.details()[detail.index()]
        {
            require(owned_row(&used, id.index(), op)? == 0)?;
            used.replace(id.index(), 1, op)?;
        }
    }
    for object in ir.objects() {
        op.events(1)?;
        if let ObjectRow::Image {
            reference: Some(id),
            ..
        } = *object
        {
            require(owned_row(&used, id.index(), op)? == 0)?;
            used.replace(id.index(), 1, op)?;
        }
    }
    for index in 0..used.len() {
        let flag = owned_row(&used, index, op)?;
        op.events(1)?;
        require(flag == 1)?;
    }
    Ok(())
}

fn equal_spans<'id>(
    left: crate::ir::RowView<'_, SourceSpan>,
    right: crate::ir::RowView<'_, SourceSpan>,
    op: &mut OwnedWork<'id, '_>,
) -> Result<(), CoreError> {
    require(left.len() == right.len())?;
    for (a, b) in left.iter().zip(right) {
        op.events(1)?;
        require(a == b)?;
    }
    Ok(())
}

fn compare_bytes<'id>(
    left: &[u8],
    right: &[u8],
    op: &mut OwnedWork<'id, '_>,
) -> Result<std::cmp::Ordering, CoreError> {
    if op.uses_task_access() {
        return task::compare_bytes(left, right, op);
    }
    for (a, b) in left.iter().zip(right) {
        op.scan(2)?;
        if a != b {
            return Ok(a.cmp(b));
        }
    }
    Ok(left.len().cmp(&right.len()))
}

fn sort_rows_by<'id, T: Copy>(
    values: &mut BuildGrowingVec<'id, T>,
    compare: impl Fn(T, T, &mut OwnedWork<'id, '_>) -> Result<std::cmp::Ordering, CoreError> + Copy,
    op: &mut OwnedWork<'id, '_>,
) -> Result<(), CoreError> {
    crate::memory::sort_build_rows_by(values, 0, compare, op)
}
fn proofs<'id>(ir: &CompactIr, op: &mut OwnedWork<'id, '_>) -> Result<(), CoreError> {
    let mut used = zeros(ir.proofs().len(), op)?;
    let mut primitives = zeros(ir.primitives().len(), op)?;
    let mut stages = zeros(ir.stages().len(), op)?;
    for index in 0..ir.atoms_len() {
        let atom = ir.owned_read_atom_at(index, op)?;
        op.events(1)?;
        if let Some(id) = atom.proof {
            let proof = *ir
                .proofs()
                .get(id.index())
                .ok_or(CoreError::InvalidProvenance)?;
            require(
                matches!(atom.kind, AtomKind::Transform | AtomKind::Hidden)
                    && proof.atom == AtomId::from_index(index)?
                    && owned_row(&used, id.index(), op)? == 0,
            )?;
            used.replace(id.index(), 1, op)?;
            mark(proof.primitives, ir.primitives(), &mut primitives, op)?;
            mark(proof.stages, ir.stages(), &mut stages, op)?;
        }
    }
    for flags in [&used, &primitives, &stages] {
        check_flags(flags, 1, op)?;
    }
    Ok(())
}

fn check_flags<'id>(
    rows: &BuildGrowingVec<'id, u8>,
    expected: u8,
    op: &mut OwnedWork<'id, '_>,
) -> Result<(), CoreError> {
    if op.uses_task_access() {
        return task::check_flags(rows, expected, op);
    }
    for index in 0..rows.len() {
        let flag = owned_row(rows, index, op)?;
        op.events(1)?;
        require(flag == expected)?;
    }
    Ok(())
}

fn mark<'id, T: Copy>(
    range: Rows<T>,
    rows: crate::ir::RowView<'_, T>,
    used: &mut BuildGrowingVec<'id, u8>,
    op: &mut OwnedWork<'id, '_>,
) -> Result<(), CoreError> {
    let _ = range.slice(rows)?;
    for index in range.start()..range.start() + range.len() {
        op.events(1)?;
        require(owned_row(used, index, op)? == 0)?;
        used.replace(index, 1, op)?;
    }
    Ok(())
}

#[cfg(test)]
pub(crate) fn validate(
    ir: &CompactIr,
    source: &SourceSnapshot,
    op: &mut crate::policy::Operation,
) -> Result<(), CoreError> {
    crate::policy::construction_scope(op, |mut work| {
        validate_owned(ir, source, &mut work)?;
        work.finish()
    })
}

// Scalar coordinates and local IDs; no borrowed or writable capability.
impl crate::memory::BuildValue for Frame {}
impl crate::memory::BuildValue for Posting {}
impl crate::memory::BuildValue for TargetIndex {}
