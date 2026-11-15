// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE file in the project root for license information.
// ---------------------------------------------------------------------------------------------

//! Strict bounded HTML table projection. The accepted GFM parse remains the
//! sole authority for node/flow identities; HTML facts only refine presentation.

use super::HtmlProjection;
use super::fragment::{self, Anchor, AnchorKind, Fragment, Part};
use super::work::ProjectionWork;
use crate::error::CoreError;
use crate::ir::{Alignment, AtomContent, AtomKind, DetailRow, NodeId, NodeKind, ObjectRow};
use crate::model::AcceptedModel;
use crate::policy::{Lease, Operation};
use lol_html::{HtmlRewriter, MemorySettings, Settings, doc_comments, doc_text, element, end_tag};
use std::cell::RefCell;
use std::ops::Range;
use std::rc::Rc;

pub(crate) const HTML: u32 = 1;
pub(crate) const COVERED: u32 = 2;
pub(crate) const CAPTION: u32 = 4;
const MAX_COLUMNS: usize = 64;
const MAX_ROWS: usize = 256;
const MAX_SLOTS: usize = 4096;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct CellFacts {
    pub colspan: u32,
    pub rowspan: u32,
    pub flags: u32,
    pub header: bool,
    pub alignment: Alignment,
}

#[derive(Debug)]
pub(super) struct Cell {
    pub raw: Range<usize>,
    pub body: Range<usize>,
    pub source: Range<usize>,
    pub row: u32,
    pub column: u32,
    pub facts: CellFacts,
    pub expected: String,
    pub node: Option<NodeId>,
}

#[derive(Debug)]
pub(super) struct Table {
    pub projected: Range<usize>,
    pub source: Range<usize>,
    pub columns: u32,
    pub rows: Vec<Range<usize>>,
    pub cells: Vec<Cell>,
    pub node: Option<NodeId>,
}

impl Table {
    pub fn retained_bytes(&self) -> usize {
        self.rows.capacity() * size_of::<Range<usize>>()
            + self.cells.capacity() * size_of::<Cell>()
            + self
                .cells
                .iter()
                .map(|cell| cell.expected.capacity())
                .sum::<usize>()
    }

    pub fn offset(&mut self, original: usize, projected: usize) {
        self.projected = projected + self.projected.start..projected + self.projected.end;
        self.source = original + self.source.start..original + self.source.end;
        for row in &mut self.rows {
            *row = projected + row.start..projected + row.end;
        }
        for cell in &mut self.cells {
            cell.raw = projected + cell.raw.start..projected + cell.raw.end;
            cell.body = projected + cell.body.start..projected + cell.body.end;
            cell.source = original + cell.source.start..original + cell.source.end;
        }
    }
}

pub(super) struct Prepared {
    pub fragment: Fragment,
    pub storage: Lease,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum Kind {
    Table,
    Group,
    Row,
    Cell,
    Caption,
    Inline,
}

struct Frame {
    name: String,
    start: usize,
    kind: Kind,
}
struct Body {
    source: Range<usize>,
    body: Range<usize>,
    colspan: usize,
    rowspan: usize,
    header: bool,
    id: Option<String>,
}
struct Row {
    source: Range<usize>,
    cells: Vec<Body>,
    id: Option<String>,
    group_id: Option<String>,
    group: usize,
}
struct Parsed {
    source_len: usize,
    stack: Vec<Frame>,
    coverage: Vec<Range<usize>>,
    table: Option<Range<usize>>,
    table_id: Option<String>,
    group_id: Option<String>,
    row_group: usize,
    rows: Vec<Row>,
    caption: Option<Body>,
    groups: u8,
    started_rows: bool,
    closed: bool,
    fault: Option<CoreError>,
}

fn space(c: char) -> bool {
    matches!(c, ' ' | '\t' | '\r' | '\n' | '\x0c')
}

fn integer(value: &str) -> Option<usize> {
    let value = value.trim_matches(space);
    if value.is_empty() || !value.bytes().all(|byte| byte.is_ascii_digit()) {
        return None;
    }
    value.bytes().try_fold(0usize, |value, byte| {
        value.checked_mul(10)?.checked_add((byte - b'0') as usize)
    })
}

pub(super) fn candidate(source: &str) -> bool {
    let source = source.trim_start_matches(space).as_bytes();
    crate::ir::inline_tag_prefix(&source[..source.len().min(7)])
        .is_some_and(|(name, closing, _)| !closing && name.eq_ignore_ascii_case(b"table"))
}

impl Parsed {
    fn cover(&mut self, span: Range<usize>) -> lol_html::HandlerResult {
        if span.start > span.end || span.end > self.source_len {
            self.fault = Some(CoreError::InvalidProvenance);
            return Err("Invalid table token source".into());
        }
        if self.coverage.len() >= fragment::MAX_PARTS {
            return Err("HTML table token limit".into());
        }
        if !span.is_empty() {
            self.coverage.push(span);
        }
        Ok(())
    }

    fn in_body(&self) -> bool {
        self.stack
            .iter()
            .any(|frame| matches!(frame.kind, Kind::Cell | Kind::Caption))
    }

    fn text(&mut self, span: Range<usize>, raw: &str) -> lol_html::HandlerResult {
        self.cover(span)?;
        if !self.in_body() && !raw.chars().all(space) {
            return Err("Text outside table cell".into());
        }
        Ok(())
    }

    fn open(&mut self, el: &lol_html::html_content::Element<'_, '_>) -> lol_html::HandlerResult {
        let span = el.source_location().bytes();
        self.cover(span.clone())?;
        if self.closed {
            return Err("Element after HTML table".into());
        }
        let name = el.tag_name();
        let parent = self.stack.last().map(|frame| frame.kind);
        let kind = if self.in_body() {
            if !fragment::safe_inline(&name) {
                return Err("Unsupported table inline content".into());
            }
            Kind::Inline
        } else {
            match name.as_str() {
                "table" if parent.is_none() && self.table.is_none() => {
                    self.table = Some(span.start..span.end);
                    self.table_id = fragment::anchor_attribute(el, "id");
                    Kind::Table
                }
                "caption"
                    if parent == Some(Kind::Table)
                        && self.caption.is_none()
                        && !self.started_rows =>
                {
                    self.caption = Some(Body {
                        source: span.clone(),
                        body: span.end..span.end,
                        colspan: 1,
                        rowspan: 1,
                        header: false,
                        id: fragment::anchor_attribute(el, "id"),
                    });
                    Kind::Caption
                }
                "thead" | "tbody" | "tfoot" if parent == Some(Kind::Table) => {
                    let flag = match name.as_str() {
                        "thead" => 1,
                        "tbody" => 2,
                        _ => 4,
                    };
                    if self.groups & 4 != 0
                        || (flag == 1 && (self.started_rows || self.groups != 0))
                    {
                        return Err("Reordered table group".into());
                    }
                    self.groups |= flag;
                    self.started_rows = true;
                    self.group_id = fragment::anchor_attribute(el, "id");
                    self.row_group = span.start + 1;
                    Kind::Group
                }
                "tr" if matches!(parent, Some(Kind::Table | Kind::Group)) => {
                    if parent == Some(Kind::Table) && self.groups & 4 != 0 {
                        return Err("Row after table footer".into());
                    }
                    if self.rows.len() >= MAX_ROWS {
                        return Err("HTML table row limit".into());
                    }
                    self.started_rows = true;
                    self.rows.push(Row {
                        source: span.clone(),
                        cells: Vec::new(),
                        id: fragment::anchor_attribute(el, "id"),
                        group_id: self.group_id.take(),
                        group: self.row_group,
                    });
                    Kind::Row
                }
                "td" | "th" if parent == Some(Kind::Row) => {
                    for key in ["colspan", "rowspan", "scope"] {
                        if el
                            .attributes()
                            .iter()
                            .filter(|attr| attr.name().eq_ignore_ascii_case(key))
                            .count()
                            > 1
                        {
                            return Err("Duplicate table attribute".into());
                        }
                    }
                    let attr = |key| {
                        el.get_attribute(key)
                            .map(|value| htmlize::unescape_attribute(value).into_owned())
                    };
                    let colspan = match attr("colspan") {
                        Some(value) => integer(&value).ok_or("Invalid colspan")?,
                        None => 1,
                    };
                    if !(1..=MAX_COLUMNS).contains(&colspan) {
                        return Err("HTML colspan limit".into());
                    }
                    let rowspan = match attr("rowspan") {
                        Some(value) => integer(&value).ok_or("Invalid rowspan")?,
                        None => 1,
                    };
                    if rowspan > MAX_ROWS {
                        return Err("HTML rowspan limit".into());
                    }
                    if let Some(value) = attr("scope")
                        && !["col", "row", "colgroup", "rowgroup"]
                            .iter()
                            .any(|key| value.trim_matches(space).eq_ignore_ascii_case(key))
                    {
                        return Err("Invalid cell scope".into());
                    }
                    let row = self.rows.last_mut().ok_or("Missing HTML table row")?;
                    if row.cells.len() >= MAX_COLUMNS {
                        return Err("HTML table cell limit".into());
                    }
                    row.cells.push(Body {
                        source: span.clone(),
                        body: span.end..span.end,
                        colspan,
                        rowspan,
                        header: name == "th",
                        id: fragment::anchor_attribute(el, "id"),
                    });
                    Kind::Cell
                }
                _ => return Err("Unsupported HTML table structure".into()),
            }
        };
        if el.can_have_content() {
            if self.stack.len() >= 32 {
                return Err("HTML table depth limit".into());
            }
            self.stack.push(Frame {
                name,
                start: span.start,
                kind,
            });
        } else if kind != Kind::Inline {
            return Err("Incomplete HTML table element".into());
        }
        Ok(())
    }

    fn close(&mut self, start: usize, name: &str, span: Range<usize>) -> lol_html::HandlerResult {
        self.cover(span.clone())?;
        let frame = self.stack.pop().ok_or("Unmatched table close")?;
        if frame.start != start || !frame.name.eq_ignore_ascii_case(name) {
            return Err("Mismatched HTML table close".into());
        }
        match frame.kind {
            Kind::Table => {
                self.table.as_mut().unwrap().end = span.end;
                self.closed = true;
            }
            Kind::Row => {
                let row = self.rows.last_mut().ok_or("Missing row")?;
                row.source.end = span.end;
            }
            Kind::Cell => {
                let cell = self
                    .rows
                    .last_mut()
                    .and_then(|row| row.cells.last_mut())
                    .ok_or("Missing cell")?;
                cell.body.end = span.start;
                cell.source.end = span.end;
            }
            Kind::Caption => {
                let caption = self.caption.as_mut().ok_or("Missing caption")?;
                caption.body.end = span.start;
                caption.source.end = span.end;
            }
            Kind::Group => {
                self.group_id = None;
                self.row_group = span.end + 1;
            }
            _ => {}
        }
        Ok(())
    }
}

fn parse(source: &str) -> Result<Option<Parsed>, CoreError> {
    let state = Rc::new(RefCell::new(Parsed {
        source_len: source.len(),
        stack: Vec::new(),
        coverage: Vec::new(),
        table: None,
        table_id: None,
        group_id: None,
        row_group: 0,
        rows: Vec::new(),
        caption: None,
        groups: 0,
        started_rows: false,
        closed: false,
        fault: None,
    }));
    let text = state.clone();
    let comments = state.clone();
    let elements = state.clone();
    let settings = Settings::new()
        .with_memory_settings(MemorySettings::new().with_max_allowed_memory_usage(2 * 1024 * 1024))
        .append_document_content_handler(doc_text!(move |chunk| {
            let span = chunk.source_location().bytes();
            let mut state = text.borrow_mut();
            let Some(raw) = source.get(span.clone()) else {
                state.fault = Some(CoreError::InvalidProvenance);
                return Err("Invalid table text source".into());
            };
            state.text(span, raw)
        }))
        .append_document_content_handler(doc_comments!(move |comment| {
            let mut state = comments.borrow_mut();
            if state.stack.is_empty() {
                return Err("Comment outside single table".into());
            }
            state.cover(comment.source_location().bytes())
        }))
        .append_element_content_handler(element!("*", move |el| {
            if el.namespace_uri() != "http://www.w3.org/1999/xhtml" {
                return Err("Foreign table namespace".into());
            }
            elements.borrow_mut().open(el)?;
            if el.can_have_content() {
                let start = el.source_location().bytes().start;
                let name = el.tag_name();
                let elements = elements.clone();
                el.on_end_tag(end_tag!(move |end| {
                    if !end.name().eq_ignore_ascii_case(&name) {
                        return Err("Mismatched table token".into());
                    }
                    elements
                        .borrow_mut()
                        .close(start, &name, end.source_location().bytes())
                }))?;
            }
            Ok(())
        }));
    let mut parser = HtmlRewriter::new(settings, |_: &[u8]| {});
    let valid = match parser.write(source.as_bytes()) {
        Ok(()) => parser.end().is_ok(),
        Err(_) => {
            drop(parser);
            false
        }
    };
    if !valid {
        return match state.borrow().fault {
            Some(error) => Err(error),
            None => Ok(None),
        };
    }
    let state = Rc::try_unwrap(state)
        .map_err(|_| CoreError::InternalInvariant)?
        .into_inner();
    if let Some(error) = state.fault {
        return Err(error);
    }
    if !valid || !state.closed || !state.stack.is_empty() || state.rows.is_empty() {
        return Ok(None);
    }
    let mut at = 0usize;
    for span in &state.coverage {
        if source.get(span.clone()).is_none() {
            return Err(CoreError::InvalidProvenance);
        }
        if span.start < at
            || !source
                .get(at..span.start)
                .is_some_and(|gap| gap.chars().all(space))
        {
            return Ok(None);
        }
        at = span.end;
    }
    if !source.get(at..).is_some_and(|gap| gap.chars().all(space)) {
        return Ok(None);
    }
    Ok(Some(state))
}

fn emit(
    out: &mut Fragment,
    source: Range<usize>,
    value: &str,
    op: &mut impl ProjectionWork,
) -> Result<bool, CoreError> {
    op.events(1)?;
    if out.parts.len() >= fragment::MAX_PARTS
        || out
            .markdown
            .len()
            .checked_add(value.len())
            .is_none_or(|n| n > 6 * fragment::MAX_FRAGMENT_BYTES)
    {
        return Ok(false);
    }
    let start = out.markdown.len();
    out.markdown.push_str(value);
    out.parts.push(Part {
        source,
        output: start..out.markdown.len(),
    });
    Ok(true)
}

fn body(
    out: &mut Fragment,
    source: &str,
    span: Range<usize>,
    op: &mut impl ProjectionWork,
) -> Result<Option<(Range<usize>, String)>, CoreError> {
    op.scan(span.len() as u64)?;
    let raw = source
        .get(span.clone())
        .ok_or_else(|| op.fail(CoreError::InvalidProvenance))?;
    let inline = fragment::lower_inline(raw);
    op.checkpoint_now()?;
    let Some(inline) = inline else {
        return Ok(None);
    };
    let start = out.markdown.len();
    for part in inline.parts {
        if !emit(
            out,
            span.start + part.source.start..span.start + part.source.end,
            &inline.markdown[part.output],
            op,
        )? {
            return Ok(None);
        }
    }
    let end = out.markdown.len();
    out.ruby.extend(
        inline
            .ruby
            .into_iter()
            .map(|unit| unit.offset(start as u64)),
    );
    out.styles.extend(
        inline
            .styles
            .into_iter()
            .map(|(range, flags)| (start as u64 + range.start..start as u64 + range.end, flags)),
    );
    out.images.extend(
        inline
            .images
            .into_iter()
            .map(|(range, size)| (start + range.start..start + range.end, size)),
    );
    out.pictures
        .extend(inline.pictures.into_iter().map(|mut picture| {
            picture.offset(span.start, start);
            picture
        }));
    out.anchors
        .extend(inline.anchors.into_iter().map(|mut anchor| {
            anchor.offset += start;
            anchor
        }));
    out.destinations
        .extend(inline.destinations.into_iter().map(|mut item| {
            item.link_start += start;
            item.source = span.start + item.source.start..span.start + item.source.end;
            item.output = start + item.output.start..start + item.output.end;
            item
        }));
    Ok(Some((start..end, inline.inline_text)))
}

#[derive(Clone, Copy, Eq, PartialEq)]
enum Slot {
    Vacant,
    Owner { colspan: usize, rowspan: usize },
    Covered,
}

struct Topology {
    columns: usize,
    slots: Vec<Slot>,
    _storage: Lease,
}

fn topology(
    rows: &[Row],
    caption: bool,
    op: &mut impl ProjectionWork,
) -> Result<Option<Topology>, CoreError> {
    let total_rows = rows.len() + usize::from(caption);
    if rows.is_empty() || total_rows > MAX_ROWS {
        return Ok(None);
    }
    let mut occupied_until = [0; MAX_COLUMNS];
    let mut layout: Option<Topology> = None;
    let mut group_end = 0;
    for (index, row) in rows.iter().enumerate() {
        op.checkpoint_now()?;
        if index == group_end {
            while group_end < rows.len() && rows[group_end].group == row.group {
                op.events(1)?;
                group_end += 1;
            }
        }
        let mut slots = [Slot::Vacant; MAX_COLUMNS];
        op.events(MAX_COLUMNS as u64)?;
        for (slot, end) in slots.iter_mut().zip(occupied_until) {
            if end > index {
                *slot = Slot::Covered;
            }
        }
        let mut column = 0;
        for cell in &row.cells {
            op.events(cell.colspan as u64)?;
            while column < MAX_COLUMNS && slots[column] != Slot::Vacant {
                column += 1;
            }
            let rowspan = if cell.rowspan == 0 {
                group_end - index
            } else {
                cell.rowspan
            };
            let row_end = index + rowspan;
            let column_end = column + cell.colspan;
            if row_end > group_end
                || column_end > MAX_COLUMNS
                || slots[column..column_end]
                    .iter()
                    .any(|slot| *slot != Slot::Vacant)
            {
                return Ok(None);
            }
            slots[column] = Slot::Owner {
                colspan: cell.colspan,
                rowspan,
            };
            slots[column + 1..column_end].fill(Slot::Covered);
            occupied_until[column..column_end].fill(row_end);
            column = column_end;
        }
        let Some(last) = slots.iter().rposition(|slot| *slot != Slot::Vacant) else {
            return Ok(None);
        };
        let width = last + 1;
        if slots[..width].contains(&Slot::Vacant) {
            return Ok(None);
        }
        if layout.is_none() {
            if width * total_rows > MAX_SLOTS {
                return Ok(None);
            }
            let count = width * rows.len();
            let Ok(storage) = op.reserve_optional((count * size_of::<Slot>()) as u64) else {
                return Ok(None);
            };
            layout = Some(Topology {
                columns: width,
                slots: Vec::with_capacity(count),
                _storage: storage,
            });
        }
        let built = layout.as_mut().unwrap();
        if built.columns != width {
            return Ok(None);
        }
        op.copy((width * size_of::<Slot>()) as u64)?;
        built.slots.extend_from_slice(&slots[..width]);
    }
    Ok(layout)
}

fn emit_covered_cell(
    out: &mut Fragment,
    table: &mut Table,
    row: usize,
    column: usize,
    caption: bool,
    boundary: usize,
    op: &mut impl ProjectionWork,
) -> Result<bool, CoreError> {
    let at = out.markdown.len();
    if !emit(out, boundary..boundary, " ", op)? {
        return Ok(false);
    }
    table.cells.push(Cell {
        raw: at..at + 1,
        body: at..at,
        source: boundary..boundary,
        row: row as u32,
        column: column as u32,
        facts: CellFacts {
            colspan: 0,
            rowspan: 0,
            flags: HTML | COVERED | if caption { CAPTION } else { 0 },
            header: false,
            alignment: Alignment::None,
        },
        expected: String::new(),
        node: None,
    });
    emit(out, boundary..boundary, "|", op)
}

pub(super) fn lower(
    source: &str,
    op: &mut impl ProjectionWork,
) -> Result<Option<Prepared>, CoreError> {
    op.checkpoint_now()?;
    if source.len() > fragment::MAX_FRAGMENT_BYTES {
        return Ok(None);
    }
    let parsed = parse(source).map_err(|error| op.fail(error))?;
    op.checkpoint_now()?;
    let Some(parsed) = parsed else {
        return Ok(None);
    };
    super::charge_events(op, parsed.coverage.len() as u64)?;
    let Some(topology) = topology(&parsed.rows, parsed.caption.is_some(), op)? else {
        return Ok(None);
    };
    let columns = topology.columns;
    let rows = parsed.rows.len() + usize::from(parsed.caption.is_some());
    let Some(slots) = rows
        .checked_mul(columns)
        .filter(|&n| rows <= MAX_ROWS && n <= MAX_SLOTS)
    else {
        return Ok(None);
    };
    // Colspan amplification is separately charged before rectangular slots,
    // generated syntax Parts and row directories can be materialized.
    let bytes = slots
        .checked_mul(size_of::<Cell>() + 4 * size_of::<Part>() + 8)
        .and_then(|n| {
            n.checked_add(
                rows * (size_of::<Range<usize>>() + size_of::<Row>()) + size_of::<Table>(),
            )
        })
        .ok_or_else(|| op.fail(CoreError::ArithmeticOverflow))?;
    let Ok(storage) = op.reserve_optional(bytes as u64) else {
        return Ok(None);
    };
    let mut out = Fragment::default();
    emit_anchor(&mut out, parsed.table_id, AnchorKind::Table);
    let mut table = Table {
        projected: 0..0,
        source: parsed.table.unwrap(),
        columns: columns as u32,
        rows: Vec::with_capacity(rows),
        cells: Vec::with_capacity(slots),
        node: None,
    };
    let mut all_rows = Vec::with_capacity(rows);
    let has_caption = parsed.caption.is_some();
    if let Some(caption) = parsed.caption {
        all_rows.push(Row {
            source: caption.source.clone(),
            cells: vec![Body {
                colspan: columns,
                ..caption
            }],
            id: None,
            group_id: None,
            group: 0,
        });
    }
    all_rows.extend(parsed.rows);
    for (
        row_index,
        Row {
            source: row_source,
            cells,
            id,
            group_id,
            group: _,
        },
    ) in all_rows.into_iter().enumerate()
    {
        op.checkpoint_now()?;
        let caption = has_caption && row_index == 0;
        let row_start = out.markdown.len();
        emit_anchor(&mut out, group_id, AnchorKind::Text);
        emit_anchor(&mut out, id, AnchorKind::Text);
        if !emit(&mut out, row_source.start..row_source.start, "|", op)? {
            return Ok(None);
        }
        let mut caption_slots = [Slot::Covered; MAX_COLUMNS];
        caption_slots[0] = Slot::Owner {
            colspan: columns,
            rowspan: 1,
        };
        let slots = if caption {
            &caption_slots[..columns]
        } else {
            let start = (row_index - usize::from(has_caption)) * columns;
            &topology.slots[start..start + columns]
        };
        let mut boundary = row_source.start;
        let mut cells = cells.into_iter();
        for (column, slot) in slots.iter().enumerate() {
            let Slot::Owner { colspan, rowspan } = *slot else {
                if *slot != Slot::Covered
                    || !emit_covered_cell(
                        &mut out, &mut table, row_index, column, caption, boundary, op,
                    )?
                {
                    return Ok(None);
                }
                continue;
            };
            let cell = cells
                .next()
                .ok_or_else(|| op.fail(CoreError::InvalidProvenance))?;
            emit_anchor(&mut out, cell.id, AnchorKind::Text);
            let raw_start = out.markdown.len();
            let Some((content, expected)) = body(&mut out, source, cell.body.clone(), op)? else {
                return Ok(None);
            };
            if content.is_empty() && !emit(&mut out, cell.body.start..cell.body.start, " ", op)? {
                return Ok(None);
            }
            let raw_end = out.markdown.len();
            table.cells.push(Cell {
                raw: raw_start..raw_end,
                body: content,
                source: cell.source.clone(),
                row: row_index as u32,
                column: column as u32,
                facts: CellFacts {
                    colspan: colspan as u32,
                    rowspan: rowspan as u32,
                    flags: HTML | if caption { CAPTION } else { 0 },
                    header: cell.header,
                    alignment: if caption {
                        Alignment::Center
                    } else {
                        Alignment::None
                    },
                },
                expected,
                node: None,
            });
            if !emit(&mut out, cell.source.end..cell.source.end, "|", op)? {
                return Ok(None);
            }
            boundary = cell.source.end;
        }
        if cells.next().is_some() {
            return Err(op.fail(CoreError::InvalidProvenance));
        }
        if !emit(&mut out, row_source.end..row_source.end, "\n", op)? {
            return Ok(None);
        }
        // Native GFM Row.raw includes its terminating LF. Keep that source
        // authority inside this row, while cell raw/body bounds stay tight.
        table.rows.push(row_start..out.markdown.len());
        if row_index == 0 {
            let delimiter = format!("|{}\n", "---|".repeat(columns));
            if !emit(&mut out, row_source.end..row_source.end, &delimiter, op)? {
                return Ok(None);
            }
        }
    }
    table.projected.end = out.markdown.len();
    if !emit(&mut out, table.source.end..table.source.end, "\n", op)? {
        return Ok(None);
    }
    out.html_table = Some(table);
    Ok(Some(Prepared {
        fragment: out,
        storage,
    }))
}

fn emit_anchor(out: &mut Fragment, name: Option<String>, kind: AnchorKind) {
    if let Some(name) = name {
        out.anchors.push(Anchor {
            name,
            offset: out.markdown.len(),
            legacy: false,
            kind,
        });
    }
}

fn within(inner: crate::identity::SourceSpan, outer: &Range<usize>) -> bool {
    outer.start as u64 <= inner.start() && inner.end() <= outer.end as u64
}

fn text_closure(
    model: &AcceptedModel,
    node: NodeId,
    cell: &Cell,
    op: &mut Operation,
) -> Result<(), CoreError> {
    let ir = model.ir();
    let row = ir.nodes()[node.index()];
    if row.logical.len() != cell.expected.encode_utf16().count() as u64 {
        return Err(op.fail(CoreError::InvalidProvenance));
    }
    let mut at = 0usize;
    crate::reader::text::atoms(
        model,
        model.main_flow_id(),
        row.logical,
        op,
        |atom, part, op| {
            if atom.kind == AtomKind::Synthetic {
                return Err(op.fail(CoreError::InvalidProvenance));
            }
            for span in atom
                .sources
                .slice(ir.sources())
                .map_err(|error| op.fail(error))?
            {
                if !within(*span, &cell.body) {
                    return Err(op.fail(CoreError::InvalidProvenance));
                }
            }
            if let AtomContent::Object(object) = atom.content {
                let ObjectRow::Image {
                    alt,
                    destination,
                    title,
                    ..
                } = ir.objects()[object.index()]
                else {
                    return Err(op.fail(CoreError::InvalidProvenance));
                };
                for span in ir.flows()[alt.index()]
                    .domains
                    .slice(ir.sources())
                    .map_err(|error| op.fail(error))?
                {
                    if !within(*span, &cell.body) {
                        return Err(op.fail(CoreError::InvalidProvenance));
                    }
                }
                for payload in [Some(destination), title].into_iter().flatten() {
                    for span in ir.payloads()[payload.index()]
                        .domains
                        .slice(ir.sources())
                        .map_err(|error| op.fail(error))?
                    {
                        if !within(*span, &cell.body) {
                            return Err(op.fail(CoreError::InvalidProvenance));
                        }
                    }
                }
            }
            let value = crate::reader::text::atom_part(model, atom, part, op)?;
            // A bounded original fragment can expand into one longer GFM
            // atom (for example raw NUL becomes three-byte U+FFFD). Charge
            // every byte in policy-sized blocks; keep the UTF-8 text and
            // the following provenance comparison as one complete slice.
            for block in value
                .as_bytes()
                .chunks(crate::policy::CHECKPOINT_BYTES as usize)
            {
                op.scan(block.len() as u64)?;
            }
            let end = at
                .checked_add(value.len())
                .ok_or_else(|| op.fail(CoreError::ArithmeticOverflow))?;
            if cell.expected.get(at..end) != Some(value) {
                return Err(op.fail(CoreError::InvalidProvenance));
            }
            at = end;
            Ok(())
        },
    )?;
    if at != cell.expected.len() {
        return Err(op.fail(CoreError::InvalidProvenance));
    }
    // Every descendant and link payload must stay in this generated body.
    let mut current = row.first_child;
    let mut left = ir.nodes().len();
    while let Some(child) = current {
        op.events(1)?;
        left = left
            .checked_sub(1)
            .ok_or_else(|| op.fail(CoreError::InvalidProvenance))?;
        let child_row = ir.nodes()[child.index()];
        if child_row.flow != model.main_flow_id() || !within(child_row.source_envelope, &cell.body)
        {
            return Err(op.fail(CoreError::InvalidProvenance));
        }
        if let Some(detail) = child_row.detail
            && let DetailRow::Link {
                destination, title, ..
            } = ir.details()[detail.index()]
        {
            for payload in [Some(destination), title].into_iter().flatten() {
                for span in ir.payloads()[payload.index()]
                    .domains
                    .slice(ir.sources())
                    .map_err(|error| op.fail(error))?
                {
                    if !within(*span, &cell.body) {
                        return Err(op.fail(CoreError::InvalidProvenance));
                    }
                }
            }
        }
        if let Some(first) = child_row.first_child {
            current = Some(first);
            continue;
        }
        let mut walked = child;
        loop {
            let walked_row = ir.nodes()[walked.index()];
            if let Some(next) = walked_row.next_sibling {
                current = Some(next);
                break;
            }
            let parent = walked_row
                .parent
                .ok_or_else(|| op.fail(CoreError::InvalidProvenance))?;
            if parent == node {
                current = None;
                break;
            }
            walked = parent;
        }
    }
    Ok(())
}

impl HtmlProjection {
    pub fn bind_tables(
        &mut self,
        model: &AcceptedModel,
        op: &mut Operation,
    ) -> Result<(), CoreError> {
        op.checkpoint_now()?;
        if self.tables.is_empty() {
            return Ok(());
        }
        if model.source().key() != self.original.key() {
            return Err(op.fail(CoreError::InvalidProvenance));
        }
        let ir = model.ir();
        let root = ir.flows()[model.main_flow_id().index()].root;
        let mut previous = 0;
        for table in &self.tables {
            op.events(1)?;
            if table.node.is_some()
                || table.projected.start < previous
                || table.projected.is_empty()
                || model.source().text().get(table.projected.clone()).is_none()
                || self.original.text().get(table.source.clone()).is_none()
                || table.columns == 0
                || table.columns as usize > MAX_COLUMNS
                || table.rows.is_empty()
                || table.rows.len() > MAX_ROWS
                || table.cells.len() != table.rows.len() * table.columns as usize
                || table.cells.len() > MAX_SLOTS
            {
                return Err(op.fail(CoreError::InvalidProvenance));
            }
            previous = table.projected.end;
        }
        for (index, node) in ir.nodes().iter().enumerate() {
            op.events(1)?;
            if node.kind != NodeKind::Table {
                continue;
            }
            op.events(u64::from(self.tables.len().checked_ilog2().unwrap_or(0)) + 1)?;
            let Ok(found) = self
                .tables
                .binary_search_by_key(&(node.source_envelope.start() as usize), |table| {
                    table.projected.start
                })
            else {
                continue;
            };
            let table = &mut self.tables[found];
            let table_id = NodeId::from_index(index).map_err(|error| op.fail(error))?;
            if table.node.is_some()
                || node.parent != Some(root)
                || node.flow != model.main_flow_id()
                || !within(node.source_envelope, &table.projected)
                || node.source_envelope.end() < table.rows.last().unwrap().end as u64
            {
                return Err(op.fail(CoreError::InvalidProvenance));
            }
            let DetailRow::Table { columns, .. } = ir.details()[node
                .detail
                .ok_or_else(|| op.fail(CoreError::InvalidProvenance))?
                .index()]
            else {
                return Err(op.fail(CoreError::InvalidProvenance));
            };
            if columns != table.columns {
                return Err(op.fail(CoreError::InvalidProvenance));
            }
            let mut row_id = node.first_child;
            let mut occupied_until = [0; MAX_COLUMNS];
            for row_index in 0..table.rows.len() {
                op.checkpoint_now()?;
                let id = row_id.ok_or_else(|| op.fail(CoreError::InvalidProvenance))?;
                let row = ir.nodes()[id.index()];
                if row.kind != NodeKind::TableRow
                    || row.parent != Some(table_id)
                    || row.flow != model.main_flow_id()
                    || !within(row.source_envelope, &table.rows[row_index])
                {
                    return Err(op.fail(CoreError::InvalidProvenance));
                }
                let DetailRow::Row { index, header, .. } = ir.details()[row
                    .detail
                    .ok_or_else(|| op.fail(CoreError::InvalidProvenance))?
                    .index()]
                else {
                    return Err(op.fail(CoreError::InvalidProvenance));
                };
                if index as usize != row_index || header != (row_index == 0) {
                    return Err(op.fail(CoreError::InvalidProvenance));
                }
                let caption = table.cells[row_index * columns as usize].facts.flags & CAPTION != 0;
                if caption && row_index != 0 {
                    return Err(op.fail(CoreError::InvalidProvenance));
                }
                let mut actual_cell = row.first_child;
                for column in 0..columns as usize {
                    op.events(1)?;
                    let cell = &mut table.cells[row_index * columns as usize + column];
                    let cell_id =
                        actual_cell.ok_or_else(|| op.fail(CoreError::InvalidProvenance))?;
                    let actual = ir.nodes()[cell_id.index()];
                    if cell.node.is_some()
                        || cell.row as usize != row_index
                        || cell.column as usize != column
                        || cell.facts.flags & HTML == 0
                        || cell.facts.flags & !(HTML | COVERED | CAPTION) != 0
                        || (cell.facts.flags & CAPTION != 0) != caption
                        || actual.kind != NodeKind::TableCell
                        || actual.parent != Some(id)
                        || actual.flow != model.main_flow_id()
                        || !within(actual.source_envelope, &cell.raw)
                        || cell.raw.start < table.rows[row_index].start
                        || cell.raw.end > table.rows[row_index].end
                        || cell.body.start < cell.raw.start
                        || cell.body.end > cell.raw.end
                        || cell.source.start < table.source.start
                        || cell.source.end > table.source.end
                        || self.original.text().get(cell.source.clone()).is_none()
                    {
                        return Err(op.fail(CoreError::InvalidProvenance));
                    }
                    let DetailRow::Cell {
                        column: actual_column,
                        missing,
                        ..
                    } = ir.details()[actual
                        .detail
                        .ok_or_else(|| op.fail(CoreError::InvalidProvenance))?
                        .index()]
                    else {
                        return Err(op.fail(CoreError::InvalidProvenance));
                    };
                    if actual_column as usize != column || missing {
                        return Err(op.fail(CoreError::InvalidProvenance));
                    }
                    if cell.facts.flags & COVERED != 0 {
                        if occupied_until[column] <= row_index
                            || cell.facts.colspan != 0
                            || cell.facts.rowspan != 0
                            || cell.facts.header
                            || cell.facts.alignment != Alignment::None
                            || !cell.expected.is_empty()
                            || !cell.source.is_empty()
                            || !actual.logical.is_empty()
                            || actual.first_child.is_some()
                        {
                            return Err(op.fail(CoreError::InvalidProvenance));
                        }
                    } else {
                        if cell.source.is_empty()
                            || cell.facts.colspan == 0
                            || cell.facts.colspan as usize > columns as usize - column
                            || cell.facts.rowspan == 0
                            || cell.facts.rowspan as usize > table.rows.len() - row_index
                            || (caption
                                && (column != 0
                                    || cell.facts.colspan != columns
                                    || cell.facts.rowspan != 1
                                    || cell.facts.header
                                    || cell.facts.alignment != Alignment::Center))
                        {
                            return Err(op.fail(CoreError::InvalidProvenance));
                        }
                        let width = cell.facts.colspan as usize;
                        op.events(width as u64)?;
                        let locations = &mut occupied_until[column..column + width];
                        if locations.iter().any(|end| *end > row_index) {
                            return Err(op.fail(CoreError::InvalidProvenance));
                        }
                        locations.fill(row_index + cell.facts.rowspan as usize);
                    }
                    text_closure(model, cell_id, cell, op)?;
                    cell.node = Some(cell_id);
                    actual_cell = actual.next_sibling;
                }
                if actual_cell.is_some() {
                    return Err(op.fail(CoreError::InvalidProvenance));
                }
                row_id = row.next_sibling;
            }
            if row_id.is_some() {
                return Err(op.fail(CoreError::InvalidProvenance));
            }
            table.node = Some(table_id);
        }
        for table in &mut self.tables {
            if table.node.is_none() || table.cells.iter().any(|cell| cell.node.is_none()) {
                return Err(op.fail(CoreError::InvalidProvenance));
            }
            super::charge_events(
                op,
                table.cells.len() as u64
                    * (u64::from(table.cells.len().checked_ilog2().unwrap_or(0)) + 1),
            )?;
            table.cells.sort_unstable_by_key(|cell| cell.node);
        }
        super::charge_events(
            op,
            self.tables.len() as u64
                * (u64::from(self.tables.len().checked_ilog2().unwrap_or(0)) + 1),
        )?;
        self.tables.sort_unstable_by_key(|table| table.node);
        op.checkpoint_now()?;
        Ok(())
    }

    pub fn table_cell(
        &self,
        table: NodeId,
        cell: NodeId,
        op: &mut Operation,
    ) -> Result<Option<CellFacts>, CoreError> {
        if self.tables.is_empty() {
            return Ok(None);
        }
        op.events(u64::from(self.tables.len().checked_ilog2().unwrap_or(0)) + 1)?;
        let Ok(index) = self
            .tables
            .binary_search_by_key(&Some(table), |table| table.node)
        else {
            return Ok(None);
        };
        let table = &self.tables[index];
        op.events(u64::from(table.cells.len().checked_ilog2().unwrap_or(0)) + 1)?;
        let index = table
            .cells
            .binary_search_by_key(&Some(cell), |cell| cell.node)
            .map_err(|_| op.fail(CoreError::InvalidProvenance))?;
        Ok(Some(table.cells[index].facts))
    }
}
