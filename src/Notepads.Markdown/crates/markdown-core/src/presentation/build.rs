// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE file in the project root for license information.
// ---------------------------------------------------------------------------------------------

use super::storage::{Blob, OwnedRows, Store};
use super::*;
use crate::code_fence::FenceInfo;
use crate::identity::LogicalSpan;
use crate::ir::{
    AtomContent, AtomKind, ClaimKind, DetailRow, FlowId, FlowRow, FlowScope, ListMarker, NodeId,
    NodeKind, NodeRow, ObjectId, ObjectRow, PayloadId, Rows,
};
use crate::layout::{BlockKind, BlockRequest, MAX_MEASUREMENT_BATCH};

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
struct Counts {
    blocks: usize,
    texts: usize,
    runs: usize,
    script_runs: usize,
    script_scopes: usize,
    ruby_units: usize,
    cells: usize,
    decorations: usize,
    images: usize,
    derived: usize,
    links: usize,
    blob: usize,
}

struct Tables {
    blocks: Store<Block>,
    texts: Store<Text>,
    runs: Store<Run>,
    script_runs: Store<ScriptRun>,
    script_scopes: Store<ScriptScope>,
    ruby_units: Store<RubyUnit>,
    cells: Store<Cell>,
    decorations: Store<Decoration>,
    images: Store<Image>,
    derived: Store<DerivedObject>,
    links: Store<Link>,
    blob: Blob,
}

pub(super) struct Buffers {
    pub blocks: OwnedRows<Block>,
    pub texts: OwnedRows<Text>,
    pub runs: OwnedRows<Run>,
    pub script_runs: OwnedRows<ScriptRun>,
    pub script_scopes: OwnedRows<ScriptScope>,
    pub ruby_units: OwnedRows<RubyUnit>,
    pub cells: OwnedRows<Cell>,
    pub decorations: OwnedRows<Decoration>,
    pub images: OwnedRows<Image>,
    pub derived: OwnedRows<DerivedObject>,
    pub links: OwnedRows<Link>,
    pub blob: OwnedRows<u8>,
}

impl Tables {
    fn counting(limit: u64) -> Self {
        Self {
            blocks: Store::count(limit),
            texts: Store::count(limit),
            runs: Store::count(limit),
            script_runs: Store::count(limit),
            script_scopes: Store::count(limit),
            ruby_units: Store::count(limit),
            cells: Store::count(limit),
            decorations: Store::count(limit),
            images: Store::count(limit),
            derived: Store::count(limit),
            links: Store::count(limit),
            blob: Blob::count(),
        }
    }

    fn counts(&self) -> Counts {
        Counts {
            blocks: self.blocks.len,
            texts: self.texts.len,
            runs: self.runs.len,
            script_runs: self.script_runs.len,
            script_scopes: self.script_scopes.len,
            ruby_units: self.ruby_units.len,
            cells: self.cells.len,
            decorations: self.decorations.len,
            images: self.images.len,
            derived: self.derived.len,
            links: self.links.len,
            blob: self.blob.len,
        }
    }

    fn allocated(counts: Counts, limit: u64, op: &mut Operation) -> Result<Self, CoreError> {
        Ok(Self {
            blocks: Store::allocated(counts.blocks, limit, op)?,
            texts: Store::allocated(counts.texts, limit, op)?,
            runs: Store::allocated(counts.runs, limit, op)?,
            script_runs: Store::allocated(counts.script_runs, limit, op)?,
            script_scopes: Store::allocated(counts.script_scopes, limit, op)?,
            ruby_units: Store::allocated(counts.ruby_units, limit, op)?,
            cells: Store::allocated(counts.cells, limit, op)?,
            decorations: Store::allocated(counts.decorations, limit, op)?,
            images: Store::allocated(counts.images, limit, op)?,
            derived: Store::allocated(counts.derived, limit, op)?,
            links: Store::allocated(counts.links, limit, op)?,
            blob: Blob::allocated(counts.blob, op)?,
        })
    }

    fn finish(self, op: &mut Operation) -> Result<Buffers, CoreError> {
        Ok(Buffers {
            blocks: self.blocks.finish(op)?,
            texts: self.texts.finish(op)?,
            runs: self.runs.finish(op)?,
            script_runs: self.script_runs.finish(op)?,
            script_scopes: self.script_scopes.finish(op)?,
            ruby_units: self.ruby_units.finish(op)?,
            cells: self.cells.finish(op)?,
            decorations: self.decorations.finish(op)?,
            images: self.images.finish(op)?,
            derived: self.derived.finish(op)?,
            links: self.links.finish(op)?,
            blob: self.blob.finish(op)?,
        })
    }

    fn bytes(&mut self, value: &str, op: &mut Operation) -> Result<BlobSlice, CoreError> {
        let start = self.blob.len;
        self.blob.append(value, op)?;
        blob_range(start, self.blob.len, op)
    }

    fn payload(
        &mut self,
        model: &AcceptedModel,
        id: Option<PayloadId>,
        op: &mut Operation,
    ) -> Result<BlobSlice, CoreError> {
        let Some(id) = id else {
            return Ok(BlobSlice::default());
        };
        self.bytes(payload_text(model, id, op)?, op)
    }

    fn code_highlight_lines(
        &mut self,
        metadata: &FenceInfo<'_>,
        op: &mut Operation,
    ) -> Result<BlobSlice, CoreError> {
        let start = self.blob.len;
        for highlight in metadata.highlights() {
            op.events(1)?;
            self.blob
                .append_bytes(&highlight.start().to_le_bytes(), op)?;
            self.blob.append_bytes(&highlight.end().to_le_bytes(), op)?;
        }
        if start == self.blob.len {
            return Ok(BlobSlice::default());
        }
        blob_range(start, self.blob.len, op)
    }

    fn wiki_image_destination(
        &mut self,
        model: &AcceptedModel,
        id: PayloadId,
        op: &mut Operation,
    ) -> Result<BlobSlice, CoreError> {
        let target = payload_text(model, id, op)?;
        op.scan(target.len() as u64)?;
        let start = self.blob.len;
        let mut parts = target.split('%');
        self.blob.append(parts.next().unwrap_or_default(), op)?;
        for part in parts {
            self.blob.append("%25", op)?;
            self.blob.append(part, op)?;
        }
        blob_range(start, self.blob.len, op)
    }

    fn flow_text(
        &mut self,
        model: &AcceptedModel,
        flow: FlowId,
        logical: LogicalSpan,
        op: &mut Operation,
    ) -> Result<BlobSlice, CoreError> {
        let start = self.blob.len;
        crate::reader::text::visit(model, flow, logical, op, |value, op| {
            self.blob.append(value, op)
        })?;
        blob_range(start, self.blob.len, op)
    }

    fn image_alt_text(
        &mut self,
        model: &AcceptedModel,
        root: FlowId,
        op: &mut Operation,
    ) -> Result<BlobSlice, CoreError> {
        let start = self.blob.len;
        let mut flow = root;
        let mut row = alt_flow(model, flow, op)?;
        let mut next = row.atoms.start();
        loop {
            if next == row.atoms.start() + row.atoms.len() {
                if flow == root {
                    return blob_range(start, self.blob.len, op);
                }
                // Each alt flow has its own coordinates. The parent image's next
                // atom lets this walk resume with constant stack space.
                let FlowScope::ImageAlt { object } = row.scope else {
                    return Err(op.fail(CoreError::InvalidProvenance));
                };
                let ObjectRow::Image { node, atom, .. } = object_row(model, object, op)? else {
                    return Err(op.fail(CoreError::InvalidProvenance));
                };
                flow = read_node(model, node, op)?.flow;
                row = alt_flow(model, flow, op)?;
                next = atom.index() + 1;
                continue;
            }
            let atom = model.ir().atom_at(next, op)?;
            next += 1;
            if atom.kind == AtomKind::Hidden || atom.target.start == atom.target.end {
                continue;
            }
            if let AtomContent::Object(object) = atom.content {
                match object_row(model, object, op)? {
                    ObjectRow::Image { alt, .. } => {
                        flow = alt;
                        row = alt_flow(model, flow, op)?;
                        next = row.atoms.start();
                        continue;
                    }
                    ObjectRow::Derived { export_sources, .. } => {
                        self.raw_export(model, export_sources, op)?;
                        continue;
                    }
                    ObjectRow::Task { .. } => {}
                }
            }
            let part =
                LogicalSpan::new(atom.target.start, atom.target.end).map_err(|e| op.fail(e))?;
            self.blob
                .append(crate::reader::text::atom_part(model, atom, part, op)?, op)?;
        }
    }

    fn raw_export(
        &mut self,
        model: &AcceptedModel,
        sources: Rows<crate::identity::SourceSpan>,
        op: &mut Operation,
    ) -> Result<BlobSlice, CoreError> {
        let start = self.blob.len;
        for span in sources.slice(model.ir().sources())? {
            op.events(1)?;
            model
                .source()
                .validate_span(*span)
                .map_err(|e| op.fail(e))?;
            let begin = usize::try_from(span.start())
                .map_err(|_| op.fail(CoreError::ArithmeticOverflow))?;
            let end =
                usize::try_from(span.end()).map_err(|_| op.fail(CoreError::ArithmeticOverflow))?;
            let text = model
                .source()
                .text()
                .get(begin..end)
                .ok_or_else(|| op.fail(CoreError::InvalidProvenance))?;
            self.blob.append(text, op)?;
        }
        blob_range(start, self.blob.len, op)
    }
}

pub(super) fn build(
    model: &AcceptedModel,
    batch: &MeasurementBatch<'_>,
    op: &mut Operation,
) -> Result<Buffers, CoreError> {
    op.ensure_budget(model.budget())?;
    op.checkpoint_now()?;
    if batch.key.model() != model.identity() {
        return Err(op.fail(CoreError::StaleResult));
    }
    if batch.requests.len() > MAX_MEASUREMENT_BATCH {
        return Err(op.fail(CoreError::InvalidRange));
    }
    batch
        .start
        .checked_add(batch.requests.len())
        .ok_or_else(|| op.fail(CoreError::ArithmeticOverflow))?;
    let limit = model
        .budget()
        .model_limits()
        .ok_or_else(|| op.fail(CoreError::UnsupportedConfiguration))?
        .model_records;
    let mut counting = Tables::counting(limit);
    project(model, batch, &mut counting, op)?;
    let counts = counting.counts();
    let mut writing = Tables::allocated(counts, limit, op)?;
    project(model, batch, &mut writing, op)?;
    if writing.counts() != counts {
        return Err(op.fail(CoreError::InternalInvariant));
    }
    let result = writing.finish(op)?;
    op.checkpoint_now()?;
    Ok(result)
}

fn project(
    model: &AcceptedModel,
    batch: &MeasurementBatch<'_>,
    out: &mut Tables,
    op: &mut Operation,
) -> Result<(), CoreError> {
    for request in batch.requests {
        let node = read_node(model, request.node, op)?;
        let expected = match node.kind {
            NodeKind::Paragraph
            | NodeKind::Diagram
            | NodeKind::MathBlock
            | NodeKind::FootnoteDefinition
            | NodeKind::FootnoteFooter
            | NodeKind::TableOfContentsEntry => BlockKind::Paragraph,
            NodeKind::Heading => BlockKind::Heading,
            NodeKind::CodeBlock => BlockKind::CodeBlock,
            NodeKind::HtmlBlock => BlockKind::HtmlBlock,
            NodeKind::ThematicBreak => BlockKind::ThematicBreak,
            NodeKind::Table => BlockKind::Table,
            NodeKind::ListItem => BlockKind::EmptyListItem,
            _ => return Err(op.fail(CoreError::InvalidProvenance)),
        };
        let expected_span = if node.kind == NodeKind::FootnoteDefinition {
            crate::layout::footnote_label_span(model, node, op)?
        } else {
            node.logical
        };
        if request.kind != expected
            || request.flow != model.main_flow_id()
            || node.flow != request.flow
            || request.logical.start() < node.logical.start()
            || request.logical.end() > node.logical.end()
            || (expected != BlockKind::EmptyListItem && request.logical != expected_span)
            || !request.x.is_finite()
            || request.x < 0.0
            || !request.available_width.is_finite()
            || request.available_width <= 0.0
        {
            return Err(op.fail(CoreError::InvalidProvenance));
        }
        let texts = out.texts.len;
        let cells = out.cells.len;
        let decorations = out.decorations.len;
        decoration(model, *request, out, op)?;
        let mut heading_level = 0;
        let mut columns = 0;
        let mut row_count = 0;
        let mut code_info = BlobSlice::default();
        let mut code_first_line_number = 0;
        let mut code_highlight_lines = BlobSlice::default();
        match expected {
            BlockKind::Table => {
                let DetailRow::Table {
                    columns: count,
                    alignments,
                    ..
                } = detail(model, node, op)?
                else {
                    return Err(op.fail(CoreError::InvalidProvenance));
                };
                columns = count;
                if columns == 0 {
                    return Err(op.fail(CoreError::InvalidProvenance));
                }
                siblings(model, node.first_child, op, |row_id, row_node, op| {
                    let DetailRow::Row { index, header, .. } = detail(model, row_node, op)? else {
                        return Err(op.fail(CoreError::InvalidProvenance));
                    };
                    if row_node.kind != NodeKind::TableRow || index != row_count {
                        return Err(op.fail(CoreError::InvalidProvenance));
                    }
                    row_count = row_count
                        .checked_add(1)
                        .ok_or_else(|| op.fail(CoreError::ArithmeticOverflow))?;
                    siblings(model, row_node.first_child, op, |cell_id, cell_node, op| {
                        let DetailRow::Cell {
                            column, missing, ..
                        } = detail(model, cell_node, op)?
                        else {
                            return Err(op.fail(CoreError::InvalidProvenance));
                        };
                        if cell_node.kind != NodeKind::TableCell || column >= columns {
                            return Err(op.fail(CoreError::InvalidProvenance));
                        }
                        let alignment = *alignments
                            .slice(model.ir().alignments())
                            .map_err(|e| op.fail(e))?
                            .get(column as usize)
                            .ok_or_else(|| op.fail(CoreError::InvalidProvenance))?;
                        let html = model.html_table_cell(request.node, cell_id, op)?;
                        let text = if html.is_some_and(|cell| cell.flags & HTML_CELL_COVERED != 0) {
                            if !cell_node.logical.is_empty() || cell_node.first_child.is_some() {
                                return Err(op.fail(CoreError::InvalidProvenance));
                            }
                            empty_text(cell_id, cell_node.logical, out, op)?
                        } else {
                            write_text(model, cell_id, cell_node.logical, batch.scope, out, op)?
                        };
                        out.cells.push(
                            Cell {
                                node: cell_id,
                                row_node: row_id,
                                row: index,
                                column,
                                header: if let Some(html) = html {
                                    html.header
                                } else if model.is_front_matter_table(request.node) {
                                    column == 0
                                } else {
                                    header
                                },
                                missing,
                                alignment: html.map_or(alignment, |cell| cell.alignment),
                                text,
                                colspan: html.map_or(1, |cell| cell.colspan),
                                rowspan: html.map_or(1, |cell| cell.rowspan),
                                html_flags: html.map_or(0, |cell| cell.flags),
                            },
                            op,
                        )?;
                        Ok(())
                    })
                })?;
            }
            BlockKind::EmptyListItem | BlockKind::ThematicBreak => {}
            _ => {
                if expected == BlockKind::Heading {
                    let DetailRow::Heading { level, .. } = detail(model, node, op)? else {
                        return Err(op.fail(CoreError::InvalidProvenance));
                    };
                    heading_level = u32::from(level);
                }
                if expected == BlockKind::CodeBlock {
                    let DetailRow::Code { info, .. } = detail(model, node, op)? else {
                        return Err(op.fail(CoreError::InvalidProvenance));
                    };
                    if let Some(info) = info {
                        let raw = payload_text(model, info, op)?;
                        code_info = out.bytes(raw, op)?;
                        op.scan_chunked(raw.len() as u64)?;
                        let metadata = FenceInfo::parse(raw);
                        code_first_line_number = metadata.first_line_number.unwrap_or_default();
                        code_highlight_lines = out.code_highlight_lines(&metadata, op)?;
                    }
                }
                write_text(model, request.node, request.logical, batch.scope, out, op)?;
            }
        }
        out.blocks.push(
            Block {
                request: *request,
                heading_level,
                columns,
                row_count,
                code_info,
                code_first_line_number,
                code_highlight_lines,
                texts: rows(texts, out.texts.len, op)?,
                cells: rows(cells, out.cells.len, op)?,
                decorations: rows(decorations, out.decorations.len, op)?,
            },
            op,
        )?;
    }
    Ok(())
}

fn empty_text(
    node: NodeId,
    logical: LogicalSpan,
    out: &mut Tables,
    op: &mut Operation,
) -> Result<u32, CoreError> {
    let value = out.bytes("", op)?;
    let runs = rows(out.runs.len, out.runs.len, op)?;
    let images = rows(out.images.len, out.images.len, op)?;
    let derived = rows(out.derived.len, out.derived.len, op)?;
    out.texts.push(
        Text {
            node,
            logical,
            utf16_len: 0,
            value,
            runs,
            images,
            derived,
        },
        op,
    )
}

fn write_text(
    model: &AcceptedModel,
    node: NodeId,
    logical: LogicalSpan,
    scope: Option<LogicalSpan>,
    out: &mut Tables,
    op: &mut Operation,
) -> Result<u32, CoreError> {
    let text_index = index(out.texts.len, op)?;
    let row = read_node(model, node, op)?;
    let mut base_flags = 0;
    if row.kind == NodeKind::Paragraph
        && let Some(parent) = row.parent
    {
        let container = read_node(model, parent, op)?;
        if container.kind == NodeKind::DescriptionTerm
            || (container.kind == NodeKind::BlockQuote
                && container.detail.is_some()
                && matches!(detail(model, container, op)?, DetailRow::Callout { title, .. } if title == node))
        {
            base_flags = RUN_BOLD;
        }
    }
    for scope in model.inline_script_scopes(node, op)? {
        let font_flags = if scope.font_flags & SCRIPT_SHAPE_ONLY != 0 {
            SCRIPT_SHAPE_ONLY
        } else {
            let (ancestor_flags, _) = style(model, scope.parent, node, op)?;
            ((ancestor_flags | base_flags) & SCRIPT_FONT_MASK)
                | (scope.font_flags & SCRIPT_SCOPE_MASK)
        };
        op.events(1)?;
        if scope.start < logical.start() || scope.end > logical.end() || scope.start > scope.end {
            return Err(op.fail(CoreError::InvalidProvenance));
        }
        out.script_scopes.push(
            ScriptScope {
                text: text_index,
                start: u32::try_from(scope.start - logical.start())
                    .map_err(|_| op.fail(CoreError::ArithmeticOverflow))?,
                end: u32::try_from(scope.end - logical.start())
                    .map_err(|_| op.fail(CoreError::ArithmeticOverflow))?,
                font_flags,
                style: scope.style,
            },
            op,
        )?;
    }
    let utf16_len =
        u32::try_from(logical.len()).map_err(|_| op.fail(CoreError::ArithmeticOverflow))?;
    for unit in model.html_ruby(logical) {
        op.events(1)?;
        if !(logical.start() <= unit.start
            && unit.start < unit.base_end
            && unit.base_end < unit.end
            && unit.end <= logical.end())
        {
            return Err(op.fail(CoreError::InvalidProvenance));
        }
        let relative = |position| (position - logical.start()) as u32;
        out.ruby_units.push(
            RubyUnit {
                text: text_index,
                start: relative(unit.start),
                base_end: relative(unit.base_end),
                end: relative(unit.end),
            },
            op,
        )?;
    }
    let value = out.flow_text(model, model.main_flow_id(), logical, op)?;
    let runs = out.runs.len;
    let images = out.images.len;
    let derived = out.derived.len;
    let mut last_link: Option<(NodeId, Option<u32>)> = None;
    crate::reader::text::atoms(
        model,
        model.main_flow_id(),
        logical,
        op,
        |atom, part, op| {
            let (flags, link_node) = style(model, atom.owner, node, op)?;
            let flags = flags | base_flags | model.inline_emphasis_flags(part.start(), op)?;
            let script_style = model.inline_script_style(part.start(), op)?;
            let link = if let Some(link_node) = link_node {
                if let Some((node, index)) = last_link
                    && node == link_node
                {
                    index
                } else {
                    let row = read_node(model, link_node, op)?;
                    let row_detail = detail(model, row, op)?;
                    let wiki = if let DetailRow::Link { claim, .. } = row_detail {
                        model.ir().authority(claim, op)?.kind == ClaimKind::WikiLink
                    } else {
                        false
                    };
                    let citation = matches!(row_detail, DetailRow::Link { .. })
                        && model.citation_region(row.source_envelope, op)?;
                    let (destination, title, local_position, local_heading) = match row_detail {
                        DetailRow::Link {
                            destination, title, ..
                        } => {
                            let local = if citation {
                                model.citation_destination(
                                    payload_text(model, destination, op)?,
                                    op,
                                )?
                            } else {
                                None
                            };
                            if let Some(position) = local {
                                let title = out.flow_text(model, row.flow, row.logical, op)?;
                                (BlobSlice::default(), title, Some(position), None)
                            } else {
                                (
                                    out.payload(model, Some(destination), op)?,
                                    out.payload(model, title, op)?,
                                    None,
                                    None,
                                )
                            }
                        }
                        DetailRow::FootnoteReference { definition, .. } => {
                            let target = read_node(model, definition, op)?;
                            let start = out.blob.len;
                            out.blob.append("Footnote ", op)?;
                            out.flow_text(model, row.flow, row.logical, op)?;
                            let title = blob_range(start, out.blob.len, op)?;
                            (
                                BlobSlice::default(),
                                title,
                                Some(target.logical.start()),
                                None,
                            )
                        }
                        DetailRow::FootnoteBacklink {
                            reference,
                            ordinal,
                            occurrence,
                            ..
                        } => {
                            let target = read_node(model, reference, op)?;
                            let mut bytes = [0_u8; 64];
                            let len = {
                                use std::io::Write as _;
                                let mut title = std::io::Cursor::new(bytes.as_mut_slice());
                                write!(
                                    title,
                                    "Back to reference {occurrence} of footnote {ordinal}"
                                )
                                .map_err(|_| op.fail(CoreError::InternalInvariant))?;
                                title.position() as usize
                            };
                            let title = std::str::from_utf8(&bytes[..len])
                                .map_err(|_| op.fail(CoreError::InternalInvariant))?;
                            (
                                BlobSlice::default(),
                                out.bytes(title, op)?,
                                Some(target.logical.start()),
                                None,
                            )
                        }
                        DetailRow::TableOfContentsEntry { heading, .. } => {
                            (BlobSlice::default(), value, None, Some(heading))
                        }
                        _ => return Err(op.fail(CoreError::InvalidProvenance)),
                    };
                    let index = if citation
                        && scope.is_some_and(|span| {
                            local_position.is_none_or(|position| {
                                position < span.start() || position >= span.end()
                            })
                        }) {
                        None
                    } else {
                        Some(out.links.push(
                            Link {
                                node: link_node,
                                local_position,
                                local_heading,
                                wiki,
                                destination,
                                title,
                            },
                            op,
                        )?)
                    };
                    last_link = Some((link_node, index));
                    index
                }
            } else {
                None
            };
            let mut write_run = |start, end, flags| {
                let run = out.runs.push(
                    Run {
                        start: u32::try_from(start - logical.start())
                            .map_err(|_| op.fail(CoreError::ArithmeticOverflow))?,
                        end: u32::try_from(end - logical.start())
                            .map_err(|_| op.fail(CoreError::ArithmeticOverflow))?,
                        flags,
                        link,
                    },
                    op,
                )?;
                if script_style != ScriptStyle::default() {
                    out.script_runs.push(
                        ScriptRun {
                            run,
                            style: script_style,
                        },
                        op,
                    )?;
                }
                Ok::<(), CoreError>(())
            };
            let mut start = part.start();
            for (range, html_flags) in model.html_styles(part) {
                let styled_start = range.start.max(start);
                let styled_end = range.end.min(part.end());
                if start < styled_start {
                    write_run(start, styled_start, flags)?;
                }
                write_run(styled_start, styled_end, flags | html_flags)?;
                start = styled_end;
            }
            if start < part.end() {
                write_run(start, part.end(), flags)?;
            }
            if atom.kind == AtomKind::Object {
                let AtomContent::Object(object) = atom.content else {
                    return Err(op.fail(CoreError::InvalidProvenance));
                };
                if let ObjectRow::Derived {
                    node,
                    kind,
                    input,
                    export_sources,
                    ..
                } = object_row(model, object, op)?
                {
                    let payload = payload_text(model, input, op)?;
                    let input = out.bytes(payload, op)?;
                    let (accessible_title, accessible_description) = if kind
                        == crate::ir::DerivedKind::Diagram
                    {
                        let metadata = crate::diagram_accessibility::inspect(payload, |bytes| {
                            op.scan(bytes as u64)
                        })?;
                        if metadata.is_limited() {
                            (BlobSlice::default(), BlobSlice::default())
                        } else {
                            let here = input.start as usize;
                            let mut field = |range: Option<std::ops::Range<usize>>| {
                                range.map_or(Ok(BlobSlice::default()), |range| {
                                    blob_range(here + range.start, here + range.end, op)
                                })
                            };
                            (field(metadata.title())?, field(metadata.description())?)
                        }
                    } else {
                        (BlobSlice::default(), BlobSlice::default())
                    };
                    let export = out.raw_export(model, export_sources, op)?;
                    out.derived.push(
                        DerivedObject {
                            object,
                            node,
                            text: text_index,
                            offset_utf16: u32::try_from(part.start() - logical.start())
                                .map_err(|_| op.fail(CoreError::ArithmeticOverflow))?,
                            kind,
                            input,
                            export,
                            accessible_title,
                            accessible_description,
                        },
                        op,
                    )?;
                    return Ok(());
                }
                let ObjectRow::Image {
                    node,
                    alt,
                    destination,
                    title,
                    wiki_size,
                    ..
                } = object_row(model, object, op)?
                else {
                    return Err(op.fail(CoreError::InvalidProvenance));
                };
                let destination = if wiki_size.is_some() {
                    out.wiki_image_destination(model, destination, op)?
                } else {
                    out.payload(model, Some(destination), op)?
                };
                let title = out.payload(model, title, op)?;
                let alt = out.image_alt_text(model, alt, op)?;
                let [light, dark] = model.html_picture_sources(node);
                let light_source = out.bytes(light.unwrap_or_default(), op)?;
                let dark_source = out.bytes(dark.unwrap_or_default(), op)?;
                out.images.push(
                    Image {
                        object,
                        node,
                        text: text_index,
                        offset_utf16: u32::try_from(part.start() - logical.start())
                            .map_err(|_| op.fail(CoreError::ArithmeticOverflow))?,
                        destination,
                        title,
                        alt,
                        light_source,
                        dark_source,
                        wiki_size,
                    },
                    op,
                )?;
            }
            Ok(())
        },
    )?;
    out.texts.push(
        Text {
            node,
            logical,
            utf16_len,
            value,
            runs: rows(runs, out.runs.len, op)?,
            images: rows(images, out.images.len, op)?,
            derived: rows(derived, out.derived.len, op)?,
        },
        op,
    )
}

fn style(
    model: &AcceptedModel,
    owner: NodeId,
    text: NodeId,
    op: &mut Operation,
) -> Result<(u32, Option<NodeId>), CoreError> {
    let mut current = owner;
    let mut flags = 0;
    let mut link = None;
    let mut left = model.ir().nodes().len();
    loop {
        left = left
            .checked_sub(1)
            .ok_or_else(|| op.fail(CoreError::InvalidProvenance))?;
        let row = read_node(model, current, op)?;
        flags |= match row.kind {
            NodeKind::Strong | NodeKind::FootnoteDefinition => RUN_BOLD,
            NodeKind::Emphasis => RUN_ITALIC,
            NodeKind::Strike => RUN_STRIKE,
            NodeKind::Highlight => RUN_HIGHLIGHT,
            NodeKind::InlineCode | NodeKind::CodeBlock => RUN_CODE,
            NodeKind::HtmlInline | NodeKind::HtmlBlock => RUN_INERT_HTML,
            _ => 0,
        };
        if matches!(
            row.kind,
            NodeKind::Link
                | NodeKind::FootnoteReference
                | NodeKind::FootnoteBacklink
                | NodeKind::TableOfContentsEntry
        ) && link.is_none()
        {
            link = Some(current);
        }
        if current == text {
            return Ok((flags, link));
        }
        current = row
            .parent
            .ok_or_else(|| op.fail(CoreError::InvalidProvenance))?;
    }
}

fn decoration(
    model: &AcceptedModel,
    request: BlockRequest,
    out: &mut Tables,
    op: &mut Operation,
) -> Result<(), CoreError> {
    let mut current = Some(request.node);
    let mut left = model.ir().nodes().len();
    while let Some(item) = current {
        left = left
            .checked_sub(1)
            .ok_or_else(|| op.fail(CoreError::InvalidProvenance))?;
        let item_row = read_node(model, item, op)?;
        current = item_row.parent;
        if item_row.kind != NodeKind::ListItem {
            continue;
        }
        let first = first_body(model, item, op)?;
        if (request.kind == BlockKind::EmptyListItem && first.is_some())
            || (request.kind != BlockKind::EmptyListItem && first != Some(request.node))
        {
            return Ok(());
        }
        let list = item_row
            .parent
            .ok_or_else(|| op.fail(CoreError::InvalidProvenance))?;
        let list_row = read_node(model, list, op)?;
        let DetailRow::List { start, .. } = detail(model, list_row, op)? else {
            return Err(op.fail(CoreError::InvalidProvenance));
        };
        let DetailRow::Item {
            occurrence, marker, ..
        } = detail(model, item_row, op)?
        else {
            return Err(op.fail(CoreError::InvalidProvenance));
        };
        let (kind, marker, ordinal) = match marker {
            ListMarker::Bullet(byte) => (DecorationKind::Bullet, byte, 0),
            ListMarker::Ordered { delimiter } => (
                DecorationKind::Ordered,
                delimiter,
                start
                    .checked_add(u64::from(occurrence))
                    .ok_or_else(|| op.fail(CoreError::ArithmeticOverflow))?,
            ),
        };
        let mut decoration = Decoration {
            item,
            list,
            kind,
            depth: request.context.item_depth,
            ordinal,
            marker,
            checked: false,
            task: None,
            action: None,
            logical: None,
        };
        siblings(model, item_row.first_child, op, |child, row, op| {
            if row.kind == NodeKind::TaskCheckbox {
                crate::reader::text::atoms(model, row.flow, row.logical, op, |atom, _, op| {
                    let AtomContent::Object(object) = atom.content else {
                        return Err(op.fail(CoreError::InvalidProvenance));
                    };
                    let ObjectRow::Task {
                        node,
                        item: actual_item,
                        checked,
                        action,
                        ..
                    } = object_row(model, object, op)?
                    else {
                        return Err(op.fail(CoreError::InvalidProvenance));
                    };
                    if node != child
                        || actual_item != item
                        || decoration.task.is_some()
                        || atom.target.end.checked_sub(atom.target.start) != Some(1)
                    {
                        return Err(op.fail(CoreError::InvalidProvenance));
                    }
                    op.events(
                        u64::from(model.ir().objects().len().checked_ilog2().unwrap_or(0)) + 1,
                    )?;
                    let static_html = model.is_static_html_checkbox(object);
                    decoration.kind = if static_html {
                        DecorationKind::StaticHtmlCheckbox
                    } else {
                        DecorationKind::Task
                    };
                    decoration.task = Some(object);
                    decoration.checked = checked;
                    decoration.action = if static_html { None } else { action };
                    decoration.logical = Some(
                        LogicalSpan::new(atom.target.start, atom.target.end)
                            .map_err(|error| op.fail(error))?,
                    );
                    Ok(())
                })?;
            }
            Ok(())
        })?;
        out.decorations.push(decoration, op)?;
        return Ok(());
    }
    Ok(())
}

fn first_body(
    model: &AcceptedModel,
    item: NodeId,
    op: &mut Operation,
) -> Result<Option<NodeId>, CoreError> {
    let mut current = read_node(model, item, op)?.first_child;
    let mut left = model.ir().nodes().len();
    while let Some(id) = current {
        left = left
            .checked_sub(1)
            .ok_or_else(|| op.fail(CoreError::InvalidProvenance))?;
        let row = read_node(model, id, op)?;
        if matches!(
            row.kind,
            NodeKind::Paragraph
                | NodeKind::Diagram
                | NodeKind::MathBlock
                | NodeKind::Heading
                | NodeKind::CodeBlock
                | NodeKind::HtmlBlock
                | NodeKind::ThematicBreak
                | NodeKind::Table
                | NodeKind::TableOfContentsEntry
        ) {
            return Ok(Some(id));
        }
        if row.kind != NodeKind::ListItem
            && let Some(child) = row.first_child
        {
            current = Some(child);
            continue;
        }
        let mut row = row;
        loop {
            op.events(1)?;
            if let Some(sibling) = row.next_sibling {
                current = Some(sibling);
                break;
            }
            let previous = row
                .parent
                .ok_or_else(|| op.fail(CoreError::InvalidProvenance))?;
            if previous == item {
                current = None;
                break;
            }
            row = read_node(model, previous, op)?;
        }
    }
    Ok(None)
}

fn siblings(
    model: &AcceptedModel,
    first: Option<NodeId>,
    op: &mut Operation,
    mut emit: impl FnMut(NodeId, NodeRow, &mut Operation) -> Result<(), CoreError>,
) -> Result<(), CoreError> {
    let mut current = first;
    let mut left = model.ir().nodes().len();
    while let Some(id) = current {
        left = left
            .checked_sub(1)
            .ok_or_else(|| op.fail(CoreError::InvalidProvenance))?;
        let row = read_node(model, id, op)?;
        emit(id, row, op)?;
        current = row.next_sibling;
    }
    Ok(())
}

fn alt_flow(model: &AcceptedModel, flow: FlowId, op: &mut Operation) -> Result<FlowRow, CoreError> {
    op.events(1)?;
    op.scan(size_of::<FlowRow>() as u64)?;
    model
        .ir()
        .flows()
        .get(flow.index())
        .copied()
        .filter(|row| matches!(row.scope, FlowScope::ImageAlt { .. }))
        .ok_or_else(|| op.fail(CoreError::InvalidProvenance))
}

fn read_node(
    model: &AcceptedModel,
    node: NodeId,
    op: &mut Operation,
) -> Result<NodeRow, CoreError> {
    op.events(1)?;
    op.scan(size_of::<NodeRow>() as u64)?;
    model
        .ir()
        .nodes()
        .get(node.index())
        .copied()
        .ok_or_else(|| op.fail(CoreError::UnknownId))
}

fn detail(
    model: &AcceptedModel,
    node: NodeRow,
    op: &mut Operation,
) -> Result<DetailRow, CoreError> {
    op.events(1)?;
    let id = node
        .detail
        .ok_or_else(|| op.fail(CoreError::InvalidProvenance))?;
    model
        .ir()
        .details()
        .get(id.index())
        .copied()
        .ok_or_else(|| op.fail(CoreError::UnknownId))
}

fn object_row(
    model: &AcceptedModel,
    object: ObjectId,
    op: &mut Operation,
) -> Result<ObjectRow, CoreError> {
    op.events(1)?;
    model
        .ir()
        .objects()
        .get(object.index())
        .copied()
        .ok_or_else(|| op.fail(CoreError::UnknownId))
}

fn index(value: usize, op: &mut Operation) -> Result<u32, CoreError> {
    u32::try_from(value).map_err(|_| op.fail(CoreError::ArithmeticOverflow))
}

fn rows<T: Copy>(start: usize, end: usize, op: &mut Operation) -> Result<Rows<T>, CoreError> {
    Rows::new(
        start,
        end.checked_sub(start)
            .ok_or_else(|| op.fail(CoreError::InternalInvariant))?,
    )
    .map_err(|e| op.fail(e))
}

fn payload_text<'a>(
    model: &'a AcceptedModel,
    id: PayloadId,
    op: &mut Operation,
) -> Result<&'a str, CoreError> {
    op.events(1)?;
    let row = model
        .ir()
        .payloads()
        .get(id.index())
        .ok_or_else(|| op.fail(CoreError::UnknownId))?;
    model.ir().text_query(row.value, op)
}

fn blob_range(start: usize, end: usize, op: &mut Operation) -> Result<BlobSlice, CoreError> {
    Ok(BlobSlice {
        start: index(start, op)?,
        len: index(
            end.checked_sub(start)
                .ok_or_else(|| op.fail(CoreError::InternalInvariant))?,
            op,
        )?,
    })
}
