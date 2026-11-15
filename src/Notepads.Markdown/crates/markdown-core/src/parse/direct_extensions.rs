// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE file in the project root for license information.
// ---------------------------------------------------------------------------------------------

//! G3 lowering consumes branded native facts after AST retirement. Structure
//! projection has one decision function shared by measurement and emission.

use super::*;
use crate::memory::BuildValue;

#[derive(Clone, Copy)]
pub(super) struct TaskBinding {
    pub object: b::ObjectRef,
    pub node: b::NodeRef,
    pub item: b::NodeRef,
    pub checked: bool,
    pub marker: b::SourceBlockRef,
    pub spacer: b::SourceBlockRef,
    pub action: Option<TaskAction>,
    pub claim: b::AuthorityRef,
    pub sources: b::SourceBlockRef,
}
impl BuildValue for TaskBinding {}

#[derive(Clone, Copy)]
pub(super) enum ObjectBinding {
    Image(metadata::ImageBinding),
    Task(TaskBinding),
    Derived(metadata::DerivedBinding),
}
impl BuildValue for ObjectBinding {}

impl ObjectBinding {
    pub(super) fn object(self) -> b::ObjectRef {
        match self {
            Self::Image(v) => v.object,
            Self::Task(v) => v.object,
            Self::Derived(v) => v.object,
        }
    }
    pub(super) fn node(self) -> b::NodeRef {
        match self {
            Self::Image(v) => v.node,
            Self::Task(v) => v.node,
            Self::Derived(v) => v.node,
        }
    }
    pub(super) fn claim(self) -> b::AuthorityRef {
        match self {
            Self::Image(v) => v.claim,
            Self::Task(v) => v.claim,
            Self::Derived(v) => v.claim,
        }
    }
    pub(super) fn sources(self) -> b::SourceBlockRef {
        match self {
            Self::Image(v) => v.sources,
            Self::Task(v) => v.sources,
            Self::Derived(v) => v.sources,
        }
    }
    pub(super) fn draft(self, atom: b::AtomRef) -> b::DraftObjectRow {
        match self {
            Self::Derived(v) => b::DraftObjectRow::Derived {
                node: v.node,
                atom,
                kind: v.kind,
                input: v.input,
                export_sources: v.sources,
            },
            Self::Image(v) => b::DraftObjectRow::Image {
                node: v.node,
                atom,
                alt: v.alt,
                destination: v.destination,
                title: v.title,
                reference: v.reference,
                wiki_size: v.wiki_size,
            },
            Self::Task(v) => b::DraftObjectRow::Task {
                node: v.node,
                atom,
                item: v.item,
                checked: v.checked,
                marker: v.marker,
                spacer: v.spacer,
                action: v.action,
            },
        }
    }
}

fn alignment(value: native::g3::Alignment) -> Alignment {
    match value {
        native::g3::Alignment::None => Alignment::None,
        native::g3::Alignment::Left => Alignment::Left,
        native::g3::Alignment::Center => Alignment::Center,
        native::g3::Alignment::Right => Alignment::Right,
    }
}

/// Resolve only the accepted sparse association for this exact cell-buffer
/// subspan. The stage belongs to the original Cell even when its derived text
/// later becomes a Text/Code/Link field or a separately owned payload.
pub(super) fn pipe_authority<'view, 'input, 'id>(
    native: &Native<'_, 'view, 'input, '_, 'id>,
    graph: &mut graph::Graph<'view, 'input, 'id>,
    builder: &mut b::DirectBuilder<'id>,
    source: n::SourceToken<'view, 'input>,
    r_start: usize,
    r_end: usize,
    work: &mut OwnedWork<'id, '_>,
) -> Result<b::AuthorityRef, CoreError> {
    let accepted = native.read(work, |access| {
        access.table_pipe_claim(source, r_start, r_end)
    })?;
    let host = accepted.host.ok_or(CoreError::InvalidProvenance)?;
    if accepted.kind != n::ClaimKind::Table
        || host.role != n::HostRole::Block
        || host.phase != n::NativePhase::Block
        || host.payload_role.is_some()
        || host.consumed_end.checked_sub(host.consumed_start) != Some(2)
    {
        return Err(work.fail(CoreError::InvalidProvenance));
    }
    let index = graph
        .member_for_lineage(accepted.owner, native, work)?
        .ok_or(CoreError::InvalidProvenance)?;
    let owner = graph.nodes.get(index, work)?.node;
    if builder.graph_node(owner, work)?.kind != NodeKind::TableCell {
        return Err(work.fail(CoreError::InvalidProvenance));
    }
    let (domains, actual) = graph.host_sources(native, host.source, builder, work)?;
    if actual.utf8_len != host.utf8_len || host.consumed_end > host.utf8_len {
        return Err(work.fail(CoreError::InvalidProvenance));
    }
    let sources = native.sources(accepted.source, builder, work)?.0;
    if builder.source_len(sources, work)? != 1 || builder.source_at(sources, 0, work)?.len() != 2 {
        return Err(work.fail(CoreError::InvalidProvenance));
    }
    builder.write_extra_authority(
        b::DraftAuthoritySpec {
            owner,
            host: b::DraftHostRow {
                owner,
                role: HostRole::Block,
                utf8_len: host.utf8_len as u64,
                domains,
            },
            phase: NativePhase::Block,
            kind: ClaimKind::Table,
            consumed: LocalUtf8Span::new(host.consumed_start as u64, host.consumed_end as u64)?,
            sources,
        },
        work,
    )
}

fn attribute<'id>(
    builder: &mut b::DirectBuilder<'id>,
    owner: b::NodeRef,
    kind: AttributeKind,
    claim: b::AuthorityRef,
    sources: b::SourceBlockRef,
    work: &mut OwnedWork<'id, '_>,
) -> Result<(), CoreError> {
    builder.attribute(
        b::DraftAttributeRow {
            owner,
            kind,
            claim,
            sources,
        },
        work,
    )
}

fn action<'view, 'input, 'id>(
    native: &Native<'_, 'view, 'input, '_, 'id>,
    builder: &b::DirectBuilder<'id>,
    marker: b::SourceBlockRef,
    work: &mut OwnedWork<'id, '_>,
) -> Result<Option<TaskAction>, CoreError> {
    if builder.source_len(marker, work)? != 1 {
        return Ok(None);
    }
    let source = builder.source_at(marker, 0, work)?;
    if source.len() != 3 {
        return Ok(None);
    }
    let start = usize::try_from(source.start()).map_err(|_| CoreError::ArithmeticOverflow)?;
    let end = usize::try_from(source.end()).map_err(|_| CoreError::ArithmeticOverflow)?;
    let raw = native.source.input().source().text().as_bytes();
    let bytes = raw.get(start..end).ok_or(CoreError::InvalidProvenance)?;
    work.scan(3)?;
    if !matches!(bytes, b"[ ]" | b"[x]" | b"[X]") {
        return Ok(None);
    }
    Ok(Some(TaskAction {
        source,
        expected: [bytes[0], bytes[1], bytes[2]],
    }))
}

/// Invoke before the old metadata structural fallback. These nodes have actual
/// G3 raw witnesses and never need a manufactured diagnostic BlockStart.
#[allow(clippy::too_many_arguments)]
pub(super) fn emit<'view, 'input, 'id>(
    native: &Native<'_, 'view, 'input, '_, 'id>,
    graph: &mut graph::Graph<'view, 'input, 'id>,
    builder: &mut b::DirectBuilder<'id>,
    node: b::NodeRef,
    metadata: graph::Metadata,
    token: n::G3Token<'view, 'input>,
    work: &mut OwnedWork<'id, '_>,
) -> Result<Option<ObjectBinding>, CoreError> {
    let info = native.read(work, |access| access.g3_info(token))?;
    let source = match info {
        n::G3Info::Table { delimiter, .. } => delimiter,
        n::G3Info::Row { raw, .. } | n::G3Info::Cell { raw, .. } => raw,
        n::G3Info::Task { marker, .. } => marker,
    };
    let kind = if matches!(info, n::G3Info::Task { .. }) {
        ClaimKind::Task
    } else {
        ClaimKind::Table
    };
    let (claim, sources) = metadata::authority(
        native,
        graph,
        builder,
        node,
        (HostRole::Block, NativePhase::Block, kind),
        source,
        work,
    )?;
    let detail = match info {
        n::G3Info::Table { columns, .. } => {
            if columns == 0 {
                return Err(work.fail(CoreError::InvalidProvenance));
            }
            let alignment_start = builder.alignments_len();
            let witness_start = builder.sources_len();
            for column in 0..columns as usize {
                work.events(1)?;
                let value = native.read(work, |access| access.table_alignment(token, column))?;
                builder.alignment_parts(&[alignment(value.value)], work)?;
                let mut witness = None;
                native.visit_sources(value.delimiter_cell, work, |span, work| {
                    if witness.replace(span).is_some() || span.is_empty() {
                        return Err(work.fail(CoreError::InvalidProvenance));
                    }
                    Ok(())
                })?;
                builder.source_parts(&[witness.ok_or(CoreError::InvalidProvenance)?], work)?;
            }
            let witnesses = builder.written_source_block(witness_start, columns as usize, work)?;
            let alignments =
                builder.written_alignment_block(alignment_start, columns as usize, work)?;
            attribute(
                builder,
                node,
                AttributeKind::TableColumns,
                claim,
                sources,
                work,
            )?;
            attribute(
                builder,
                node,
                AttributeKind::TableAlignment,
                claim,
                witnesses,
                work,
            )?;
            b::DraftDetailRow::Table {
                columns,
                alignments,
                claim,
            }
        }
        n::G3Info::Row { index, header, .. } => {
            attribute(
                builder,
                node,
                AttributeKind::RowHeader,
                claim,
                sources,
                work,
            )?;
            b::DraftDetailRow::Row {
                index,
                header,
                claim,
            }
        }
        n::G3Info::Cell {
            column, missing, ..
        } => {
            attribute(
                builder,
                node,
                AttributeKind::CellMissing,
                claim,
                sources,
                work,
            )?;
            b::DraftDetailRow::Cell {
                column,
                missing,
                claim,
            }
        }
        n::G3Info::Task {
            checked,
            item,
            spacer,
            ..
        } => {
            let item_index = graph
                .member_for_lineage(item, native, work)?
                .ok_or(CoreError::InvalidProvenance)?;
            let item = graph.nodes.get(item_index, work)?.node;
            if builder.graph_node(item, work)?.kind != NodeKind::ListItem {
                return Err(work.fail(CoreError::InvalidProvenance));
            }
            let spacer = native.sources(spacer, builder, work)?.0;
            let action = action(native, builder, sources, work)?;
            return Ok(Some(ObjectBinding::Task(TaskBinding {
                object: metadata.object().ok_or(CoreError::InvalidProvenance)?,
                node,
                item,
                checked,
                marker: sources,
                spacer,
                action,
                claim,
                sources,
            })));
        }
    };
    builder.write_detail(
        metadata.detail().ok_or(CoreError::InvalidProvenance)?,
        detail,
        work,
    )?;
    Ok(None)
}

/// All ordinals and the task-label relation come from accepted native facts,
/// not from scanning text or inferring geometry. Caller checks the relation
/// against the actual parent/sibling graph and charges that access.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum Position {
    Ordinary,
    Cell(u32),
    Row(u32),
    TaskLabel,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum SeparatorOwner {
    Current,
    TaskCheckbox,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) struct Separator {
    pub kind: SyntheticKind,
    pub owner: SeparatorOwner,
}

impl Separator {
    pub(super) fn text(self) -> &'static str {
        match self.kind {
            SyntheticKind::CellTab => "\t",
            SyntheticKind::TaskLabelSpace => " ",
            _ => "\n",
        }
    }
}

pub(super) fn block(kind: NodeKind) -> bool {
    matches!(
        kind,
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
    )
}

pub(super) fn before(
    kind: NodeKind,
    position: Position,
    outer_blocks: bool,
    output: bool,
    previous_block: bool,
) -> Result<Option<Separator>, CoreError> {
    let structural = match position {
        Position::Cell(column) if kind == NodeKind::TableCell => {
            (column != 0).then_some(SyntheticKind::CellTab)
        }
        Position::Row(index) if kind == NodeKind::TableRow => {
            (index != 0).then_some(SyntheticKind::RowLineFeed)
        }
        Position::TaskLabel if kind == NodeKind::Paragraph => {
            return Ok(output.then_some(Separator {
                kind: SyntheticKind::TaskLabelSpace,
                owner: SeparatorOwner::TaskCheckbox,
            }));
        }
        Position::Ordinary => None,
        _ => return Err(CoreError::InvalidProvenance),
    };
    if let Some(kind) = structural {
        return Ok(Some(Separator {
            kind,
            owner: SeparatorOwner::Current,
        }));
    }
    Ok(
        (outer_blocks && output && previous_block && block(kind)).then_some(Separator {
            kind: SyntheticKind::BlockLineFeed,
            owner: SeparatorOwner::Current,
        }),
    )
}

/// A later empty cell/row still contributes its structural separator. This is
/// included in postorder output propagation before either projection consumer.
pub(super) fn structural_output(position: Position) -> bool {
    matches!(position, Position::Cell(column) if column != 0)
        || matches!(position, Position::Row(index) if index != 0)
}

pub(super) fn marks_block(
    kind: NodeKind,
    position: Position,
    outer_blocks: bool,
    output: bool,
) -> bool {
    outer_blocks && output && block(kind) && position != Position::TaskLabel
}
