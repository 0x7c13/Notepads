// Notepads Markdown modifications Copyright (c) 2026, Jiaqi (0x7c13) Liu.
// Original third-party licenses remain applicable; see the crate COPYING file.

macro_rules! native_some { ($label:lifetime, $value:expr) => { match $value { Some(value) => value, None => break $label None } }; }
use crate::native::{
    AcceptedFact, AppendKind, BufferId, Context, Host, NativeKind, NativeViolation, PSpan, RSpan,
    SyntaxRole, g3,
};
use std::borrow::Cow;
use std::cmp::min;
use std::mem;

use crate::matchers::backslash_matcher;
use crate::nodes::{Ast, BlockLines, LineColumn, Node, NodeTable, NodeValue, TableAlignment};
use crate::parser::Parser;
use crate::scanners;
use crate::strings::{count_newlines, is_line_end_char, newlines_of, trim_cow};

// Limit to prevent a malicious input from causing a denial of service.
// See get_num_autocompleted_cells.
const MAX_AUTOCOMPLETED_CELLS: usize = 500_000;

pub(super) struct TableBlockResult<'a> {
    pub(super) container: Node<'a>,
    pub(super) replace: bool,
    pub(super) mark_visited: bool,
    pub(super) split: bool,
}

impl<'a> TableBlockResult<'a> {
    pub(super) fn from_legacy((container, replace, mark_visited): (Node<'a>, bool, bool)) -> Self {
        Self {
            container,
            replace,
            mark_visited,
            split: false,
        }
    }
}

pub(super) fn try_opening_block<'a, H: Host>(
    parser: &mut Parser<'a, '_, '_, '_, '_, '_, H>,
    container: Node<'a>,
    line: &str,
) -> Result<Option<TableBlockResult<'a>>, H::Error> {
    if parser.ctx.capture {
        return captured_block(parser, container, line);
    }
    let result = 'native: {
        let aligns = match &container.data().value {
            NodeValue::Paragraph => None,
            NodeValue::Table(nt) => Some(nt.alignments.clone()),
            _ => break 'native None,
        };

        match aligns {
            None => try_opening_header(parser, container, line)?,
            Some(ref aligns) => try_opening_row(parser, container, aligns, line)?,
        }
    };
    Ok(result.map(TableBlockResult::from_legacy))
}

fn try_opening_header<'a, H: Host>(
    parser: &mut Parser<'a, '_, '_, '_, '_, '_, H>,
    container: Node<'a>,
    line: &str,
) -> Result<Option<(Node<'a>, bool, bool)>, H::Error> {
    let result = 'native: {
        if container.data().table_visited {
            break 'native Some((container, false, false));
        }

        if scanners::table_start(&line[parser.first_nonspace..]).is_none() {
            break 'native Some((container, false, false));
        }

        let spoiler = parser.options.extension.spoiler;

        let delimiter_row = match row(&line[parser.first_nonspace..], spoiler) {
            Some(delimiter_row) => delimiter_row,
            None => break 'native Some((container, false, true)),
        };

        let mut container_content = mem::take(&mut container.data_mut().lines_mut().content);
        let mut header_row = match row(&container_content, spoiler) {
            Some(header_row) => header_row,
            None => {
                container.data_mut().lines_mut().content = container_content;
                break 'native Some((container, false, true));
            }
        };

        if header_row.cells.len() != delimiter_row.cells.len() {
            container.data_mut().lines_mut().content = container_content;
            break 'native Some((container, false, true));
        }

        if header_row.paragraph_offset > 0 {
            try_inserting_table_header_paragraph(
                parser,
                container,
                &container_content,
                header_row.paragraph_offset,
                header_row.leading_indent,
            )?;
        }

        let mut alignments = vec![];
        for cell in delimiter_row.cells {
            let cell_content = cell.content.as_bytes();
            let left = !cell_content.is_empty() && cell_content[0] == b':';
            let right = !cell_content.is_empty() && cell_content[cell_content.len() - 1] == b':';
            parser.ctx.push(
                &mut alignments,
                if left && right {
                    TableAlignment::Center
                } else if left {
                    TableAlignment::Left
                } else if right {
                    TableAlignment::Right
                } else {
                    TableAlignment::None
                },
            )?;
        }

        let start = container.data().sourcepos.start;
        let table_value = parser.ctx.boxed(NodeTable {
            alignments,
            num_columns: header_row.cells.len(),
            num_rows: 0,
            num_nonempty_cells: 0,
        })?;
        let child = Ast::new(NodeValue::Table(table_value), start);
        let table = parser.arena.try_alloc(child.into(), parser.ctx)?;
        container.append(table);

        let header = parser.add_child(table, NodeValue::TableRow(true), start.column)?;
        {
            let header_ast = &mut header.data_mut();
            header_ast.sourcepos.start.line = start.line;
            header_ast.sourcepos.end = start.column_add(
                (container_content.len()
                    - newlines_of(&container_content)
                    - 1
                    - header_row.paragraph_offset
                    - header_row.leading_indent) as isize,
            );
        }

        let mut i = 0;

        while i < header_row.cells.len() {
            let cell = &mut header_row.cells[i];
            let ast_cell = parser.add_child(
                header,
                NodeValue::TableCell,
                start.column + cell.start_offset
                    - header_row.paragraph_offset
                    - header_row.leading_indent,
            )?;
            let ast = &mut ast_cell.data_mut();
            ast.sourcepos.start.line = start.line;
            ast.sourcepos.end = start.column_add(
                (cell.end_offset - header_row.paragraph_offset - header_row.leading_indent)
                    as isize,
            );
            let lines = ast.lines_mut();
            let content = parser.ctx.string(&cell.content)?;
            let previous = mem::replace(&mut lines.content, content);
            parser.ctx.discard_string(previous)?;
            parser.ctx.small_push(
                &mut lines.offsets,
                start.column + cell.start_offset - 1 + cell.internal_offset
                    - header_row.paragraph_offset
                    - header_row.leading_indent,
            )?;

            i += 1;
        }

        container_content.clear();
        let previous = mem::replace(&mut parser.working_content, container_content);
        parser.ctx.discard_string(previous)?;

        let offset = line.len() - newlines_of(line) - parser.offset;
        parser.advance_offset(line, offset, false)?;

        adjust_table_counters(
            table,
            i,
            (parser.line_number, parser.curline_end_col).into(),
        );

        Some((table, true, false))
    };
    Ok(result)
}

fn try_opening_row<'a, H: Host>(
    parser: &mut Parser<'a, '_, '_, '_, '_, '_, H>,
    container: Node<'a>,
    alignments: &[TableAlignment],
    line: &str,
) -> Result<Option<(Node<'a>, bool, bool)>, H::Error> {
    let result = 'native: {
        if parser.blank {
            break 'native None;
        }

        if get_num_autocompleted_cells(container) > MAX_AUTOCOMPLETED_CELLS {
            break 'native None;
        }

        let start_column = parser.first_nonspace + 1;
        let spoiler = parser.options.extension.spoiler;
        let mut this_row = native_some!('native, row(&line[parser.first_nonspace..], spoiler));

        let new_row = parser.add_child(container, NodeValue::TableRow(false), start_column)?;
        new_row.data_mut().sourcepos.end.column = parser.curline_end_col;

        let parsed_cells = min(alignments.len(), this_row.cells.len());
        let mut last_column = start_column;

        for i in 0..parsed_cells {
            let cell = &mut this_row.cells[i];
            let cell_node = parser.add_child(
                new_row,
                NodeValue::TableCell,
                start_column + cell.start_offset,
            )?;
            let cell_ast = &mut cell_node.data_mut();
            cell_ast.sourcepos.end.column = start_column + cell.end_offset;
            let lines = cell_ast.lines_mut();
            let content = parser.ctx.string(&cell.content)?;
            let previous = mem::replace(&mut lines.content, content);
            parser.ctx.discard_string(previous)?;
            parser.ctx.small_push(
                &mut lines.offsets,
                start_column + cell.start_offset - 1 + cell.internal_offset,
            )?;

            last_column = cell_ast.sourcepos.end.column;
        }

        for _ in parsed_cells..alignments.len() {
            let cell_node = parser.add_child(new_row, NodeValue::TableCell, last_column + 1)?;
            // for autocompleted (empty) cells, set end column equal to start
            let cell_ast = &mut cell_node.data_mut();
            cell_ast.sourcepos.end.column = last_column + 1;
        }

        let offset = line.len() - parser.offset - newlines_of(line);
        parser.advance_offset(line, offset, false)?;

        adjust_table_counters(
            container,
            parsed_cells,
            (parser.line_number, parser.curline_end_col).into(),
        );

        Some((new_row, false, false))
    };
    Ok(result)
}

struct Row<'t> {
    paragraph_offset: usize,
    leading_indent: usize,
    cells: Vec<Cell<'t>>,
}

struct Cell<'t> {
    start_offset: usize,
    end_offset: usize,
    internal_offset: usize,
    content: Cow<'t, str>,
}

fn row(string: &str, spoiler: bool) -> Option<Row<'_>> {
    let bytes = string.as_bytes();
    let len = string.len();
    let mut cells: Vec<Cell> = vec![];

    let mut leading_indent = bytes
        .iter()
        .take_while(|&&b| b == b' ' || b == b'\t')
        .count();
    let mut offset =
        leading_indent + scanners::table_cell_end(&string[leading_indent..]).unwrap_or(0);

    let mut paragraph_offset = 0;

    while offset < len {
        let cell_matched = scanners::table_cell(&string[offset..], spoiler).unwrap_or(0);
        let pipe_matched = scanners::table_cell_end(&string[offset + cell_matched..]).unwrap_or(0);

        if cell_matched > 0 || pipe_matched > 0 {
            let mut cell = unescape_pipes(&string[offset..offset + cell_matched]);
            trim_cow(&mut cell);

            let mut start_offset = offset;
            let mut internal_offset = 0;

            while start_offset > paragraph_offset + leading_indent
                && bytes[start_offset - 1] != b'|'
            {
                start_offset -= 1;
                internal_offset += 1;
            }

            if cells.len() == u16::MAX as usize {
                return None;
            }

            cells.push(Cell {
                start_offset,
                end_offset: offset + cell_matched - 1,
                internal_offset,
                content: cell,
            });
        }

        offset += cell_matched + pipe_matched;

        if pipe_matched == 0 {
            let row_end_offset = scanners::table_row_end(&string[offset..]).unwrap_or(0);
            offset += row_end_offset;

            if row_end_offset == 0 || offset == len {
                break;
            }

            paragraph_offset = offset;
            cells.clear();
            leading_indent = bytes[offset..]
                .iter()
                .take_while(|&&b| b == b' ' || b == b'\t')
                .count();
            offset += leading_indent;
            offset += scanners::table_cell_end(&string[offset..]).unwrap_or(0);
        }
    }

    if offset != len || cells.is_empty() {
        None
    } else {
        Some(Row {
            paragraph_offset,
            leading_indent,
            cells,
        })
    }
}

fn try_inserting_table_header_paragraph<'a, H: Host>(
    parser: &mut Parser<'a, '_, '_, '_, '_, '_, H>,
    container: Node<'a>,
    container_content: &str,
    paragraph_offset: usize,
    leading_indent: usize,
) -> Result<(), H::Error> {
    'native: {
        if container
            .parent()
            .is_some_and(|p| !p.can_contain_type(&NodeValue::Paragraph))
        {
            break 'native;
        }

        let preface = &container_content[..paragraph_offset];
        let mut paragraph_content = unescape_pipes(preface);
        let (newlines, _since_newline) = count_newlines(&paragraph_content);
        trim_cow(&mut paragraph_content);
        let paragraph_content = parser.ctx.string(&paragraph_content)?;

        let container_ast = &mut *container.data_mut();
        let start = container_ast.sourcepos.start;

        let mut paragraph = Ast::new(NodeValue::Paragraph, start);
        paragraph.sourcepos.end.line = start.line + newlines - 1;

        let container_offsets = &container_ast.lines.as_ref().unwrap().offsets;
        let mut offsets = smallvec::SmallVec::new();
        for &offset in &container_offsets[..newlines] {
            parser.ctx.small_push(&mut offsets, offset)?;
        }

        let last_line_offset = *offsets.last().unwrap_or(&0);
        paragraph.sourcepos.end.column = last_line_offset
            + preface
                .as_bytes()
                .iter()
                .rev()
                .skip(newlines_of(preface))
                .take_while(|&&c| !is_line_end_char(c))
                .count();

        container_ast.sourcepos.start.line += newlines;
        container_ast.sourcepos.start.column = container_offsets[newlines] + leading_indent + 1;

        paragraph.lines = Some(parser.ctx.boxed(BlockLines {
            opened_line: 0,
            eof_anchor: None,
            buffer: None,
            base: 0,
            content: paragraph_content,
            offsets,
        })?);
        let node = parser.arena.try_alloc(paragraph.into(), parser.ctx)?;
        container.insert_before(node);
    };
    Ok(())
}

fn unescape_pipes(string: &str) -> Cow<'_, str> {
    let bytes = string.as_bytes();
    let mut backslashes = backslash_matcher().iter(bytes);
    let mut v = String::new();
    let mut offset = 0;

    while let Some(index) = backslashes.next() {
        if bytes.get(index + 1) == Some(&b'|') {
            v.push_str(&string[offset..index]);
            offset = index + 1;
        }
        backslashes.advance_to(index + 2);
    }

    if offset == 0 {
        string.into()
    } else {
        v.push_str(&string[offset..]);
        v.into()
    }
}

// Increment the number of rows in the table. Also update num_nonempty_cells,
// which keeps track of the number of cells which were parsed from the
// input file. (If one of the rows is too short, then the trailing cells are
// autocompleted. Autocompleted cells are not counted in num_nonempty_cells.)
// The purpose of this is to prevent a malicious input from generating a very
// large number of autocompleted cells, which could cause a denial of service
// vulnerability.
fn adjust_table_counters(container: Node<'_>, i: usize, end: LineColumn) {
    let mut ast = container.data_mut();
    let NodeValue::Table(ref mut nt) = ast.value else {
        unreachable!();
    };
    nt.num_rows += 1;
    nt.num_nonempty_cells += i;
    ast.sourcepos.end = end;
}

// Calculate the number of autocompleted cells.
fn get_num_autocompleted_cells(container: Node<'_>) -> usize {
    return match container.data().value {
        NodeValue::Table(ref node_table) => {
            let num_cells = node_table.num_columns * node_table.num_rows;

            if num_cells < node_table.num_nonempty_cells {
                0
            } else {
                (node_table.num_columns * node_table.num_rows) - node_table.num_nonempty_cells
            }
        }
        _ => 0,
    };
}

// Captured G3 uses the same cell/escaped-punctuation rules as the legacy row
// scanner, but keeps raw offsets until acceptance and charges every scan.
#[derive(Clone, Copy)]
struct CellScan {
    raw_start: usize,
    raw_end: usize,
    trim_start: usize,
    trim_end: usize,
    alignment: Option<g3::Alignment>,
}

struct RowScan {
    cells: Vec<CellScan>,
    anchor: usize,
}

fn scan_row<H: Host>(ctx: &mut Context<'_, H>, raw: &str) -> Result<Option<RowScan>, H::Error> {
    let bytes = raw.as_bytes();
    let mut end = bytes.len();
    if end != 0 && bytes[end - 1] == b'\n' {
        ctx.scan_step()?;
        end -= 1;
    }
    if end != 0 && bytes[end - 1] == b'\r' {
        ctx.scan_step()?;
        end -= 1;
    }
    let physical_end = end;
    let mut start = 0;
    while start < end && matches!(bytes[start], b' ' | b'\t') {
        ctx.scan_step()?;
        start += 1;
    }
    while end > start && matches!(bytes[end - 1], b' ' | b'\t') {
        ctx.scan_step()?;
        end -= 1;
    }
    if start == end {
        return Ok(None);
    }
    let mut at = start;
    if bytes[at] == b'|' {
        ctx.scan_step()?;
        at += 1;
    }
    let mut cell_start = at;
    let mut cells = Vec::new();
    let mut anchor = physical_end;
    while at < end {
        ctx.scan_step()?;
        let byte = bytes[at];
        if matches!(byte, b'\r' | b'\n') {
            ctx.discard_vec(cells)?;
            return Ok(None); // A row probe must receive exactly one physical line.
        }
        if byte == b'\\' && at + 1 < end && bytes[at + 1].is_ascii_punctuation() {
            ctx.scan_step()?;
            at += 2;
            continue;
        }
        if byte == b'|' {
            push_cell(ctx, &mut cells, bytes, cell_start, at)?;
            at += 1;
            cell_start = at;
            if at == end {
                anchor = at - 1;
            }
        } else {
            at += 1;
        }
    }
    if cell_start < end {
        // Include physical trailing spaces in the final raw cell witness.
        push_cell(ctx, &mut cells, bytes, cell_start, physical_end)?;
    }
    if cells.is_empty() {
        ctx.discard_vec(cells)?;
        return Ok(None);
    }
    Ok(Some(RowScan { cells, anchor }))
}

fn push_cell<H: Host>(
    ctx: &mut Context<'_, H>,
    cells: &mut Vec<CellScan>,
    bytes: &[u8],
    raw_start: usize,
    raw_end: usize,
) -> Result<(), H::Error> {
    let mut trim_start = raw_start;
    let mut trim_end = raw_end;
    while trim_start < trim_end && matches!(bytes[trim_start], b' ' | b'\t') {
        ctx.scan_step()?;
        trim_start += 1;
    }
    while trim_end > trim_start && matches!(bytes[trim_end - 1], b' ' | b'\t') {
        ctx.scan_step()?;
        trim_end -= 1;
    }
    ctx.push(
        cells,
        CellScan {
            raw_start,
            raw_end,
            trim_start,
            trim_end,
            alignment: None,
        },
    )
}

fn scan_alignment<H: Host>(
    ctx: &mut Context<'_, H>,
    text: &str,
) -> Result<Option<g3::Alignment>, H::Error> {
    let bytes = text.as_bytes();
    if bytes.is_empty() {
        return Ok(None);
    }
    ctx.scan_step()?;
    let left = bytes[0] == b':';
    ctx.scan_step()?;
    let right = bytes[bytes.len() - 1] == b':';
    let start = usize::from(left);
    let end = bytes.len() - usize::from(right);
    if start >= end {
        return Ok(None);
    }
    for &byte in &bytes[start..end] {
        ctx.scan_step()?;
        if byte != b'-' {
            return Ok(None);
        }
    }
    Ok(Some(match (left, right) {
        (false, false) => g3::Alignment::None,
        (true, false) => g3::Alignment::Left,
        (false, true) => g3::Alignment::Right,
        (true, true) => g3::Alignment::Center,
    }))
}

pub(super) fn matches_with_context<H: Host>(
    ctx: &mut Context<'_, H>,
    line: &str,
) -> Result<bool, H::Error> {
    let row = scan_row(ctx, line)?;
    if let Some(row) = row {
        ctx.discard_vec(row.cells)?;
        Ok(true)
    } else {
        Ok(false)
    }
}

fn add_offset<H: Host>(
    ctx: &mut Context<'_, H>,
    base: usize,
    offset: usize,
) -> Result<usize, H::Error> {
    base.checked_add(offset)
        .ok_or_else(|| ctx.host.reject(NativeViolation::ArithmeticOverflow))
}

fn raw_part<H: Host>(
    ctx: &mut Context<'_, H>,
    raw: RSpan,
    start: usize,
    end: usize,
) -> Result<RSpan, H::Error> {
    let start = add_offset(ctx, raw.start, start)?;
    let end = add_offset(ctx, raw.start, end)?;
    if start > end || end > raw.end {
        return Err(ctx.host.reject(NativeViolation::InvalidProvenance));
    }
    Ok(RSpan {
        buffer: raw.buffer,
        start,
        end,
    })
}

fn cut<H: Host>(
    ctx: &mut Context<'_, H>,
    raw: RSpan,
    start: usize,
    end: usize,
    reason: SyntaxRole,
) -> Result<(), H::Error> {
    if start != end {
        let span = raw_part(ctx, raw, start, end)?;
        ctx.fact(AcceptedFact::Cut { span, reason })?;
    }
    Ok(())
}

fn native_node<'a, H: Host>(
    parser: &mut Parser<'a, '_, '_, '_, '_, '_, H>,
    parent: Node<'a>,
    value: NodeValue,
    kind: NativeKind,
    start: LineColumn,
) -> Result<Node<'a>, H::Error> {
    let mut depth = 1_usize;
    for _ in parent.ancestors() {
        parser.ctx.event()?;
        depth = depth
            .checked_add(1)
            .ok_or_else(|| parser.ctx.host.reject(NativeViolation::ArithmeticOverflow))?;
        parser
            .ctx
            .host
            .admit(crate::native::NativeAdmission::Depth(depth as u64))?;
    }
    let parent_id = parent
        .data()
        .native_id
        .ok_or_else(|| parser.ctx.host.reject(NativeViolation::InvalidProvenance))?;
    let id = parser.ctx.node(Some(parent_id), kind)?;
    let mut ast = Ast::new(value, start);
    ast.native_id = Some(id);
    ast.open = false;
    let node = parser.arena.try_alloc(ast.into(), parser.ctx)?;
    parent.append(node);
    Ok(node)
}

fn line_buffer<H: Host>(
    ctx: &mut Context<'_, H>,
    owner: crate::native::NativeNodeId,
    line: &str,
    offset: usize,
) -> Result<RSpan, H::Error> {
    let text = line
        .get(offset..)
        .ok_or_else(|| ctx.host.reject(NativeViolation::InvalidProvenance))?;
    let start = add_offset(ctx, ctx.line_start, offset)?;
    let end = add_offset(ctx, start, text.len())?;
    let buffer = ctx.buffer(owner)?;
    let raw = RSpan {
        buffer,
        start: 0,
        end: text.len(),
    };
    ctx.fact(AcceptedFact::Append {
        span: raw,
        source: PSpan { start, end },
        kind: AppendKind::Copy,
    })?;
    ctx.fact(AcceptedFact::Seal {
        buffer,
        len: text.len(),
    })?;
    Ok(raw)
}

/// The actual immutable raw borrow and unique String are present here. The
/// Context transport methods receive only already accepted coordinates.
fn cell_content<H: Host>(
    ctx: &mut Context<'_, H>,
    text: &str,
    raw: RSpan,
    cell: CellScan,
    buffer: BufferId,
) -> Result<String, H::Error> {
    if raw.end.checked_sub(raw.start) != Some(text.len()) {
        return Err(ctx.host.reject(NativeViolation::InvalidProvenance));
    }
    let bytes = text.as_bytes();
    let mut result = String::new();
    let mut at = cell.trim_start;
    let mut copy_start = at;
    while at < cell.trim_end {
        ctx.scan_step()?;
        if bytes[at] == b'\\' && at + 1 < cell.trim_end {
            ctx.scan_step()?;
            if bytes[at + 1] == b'|' {
                copy_cell_part(ctx, &mut result, buffer, text, raw, copy_start, at)?;
                accept_table_pipe(ctx, &mut result, buffer, text, raw, at)?;
                at += 2;
                copy_start = at;
                continue;
            }
            if bytes[at + 1].is_ascii_punctuation() {
                at += 2;
                continue;
            }
        }
        at += 1;
    }
    copy_cell_part(
        ctx,
        &mut result,
        buffer,
        text,
        raw,
        copy_start,
        cell.trim_end,
    )?;
    if cell.trim_start == cell.trim_end {
        let source = raw_part(ctx, raw, cell.trim_start, cell.trim_start)?;
        ctx.transport_append_from_buffer(
            RSpan {
                buffer,
                start: 0,
                end: 0,
            },
            source,
        )?;
    }
    Ok(result)
}

/// The scanner selects an escaped pair; this accepting operation checks the
/// borrowed bytes again before any write. Tests can inject a stale descriptor
/// with legal coordinates without weakening the source transport interface.
fn accept_table_pipe<H: Host>(
    ctx: &mut Context<'_, H>,
    result: &mut String,
    buffer: BufferId,
    text: &str,
    raw: RSpan,
    start: usize,
) -> Result<(), H::Error> {
    let end = add_offset(ctx, start, 2)?;
    ctx.scan(2)?;
    if raw.end.checked_sub(raw.start) != Some(text.len())
        || text.as_bytes().get(start..end) != Some(&[0x5c, 0x7c])
    {
        return Err(ctx.host.reject(NativeViolation::InvalidProvenance));
    }
    let source = raw_part(ctx, raw, start, end)?;
    let old_len = result.len();
    let expected_end = add_offset(ctx, old_len, 1)?;
    ctx.append(result, "|")?;
    if result.len() != expected_end || result.as_bytes()[old_len] != b'|' {
        return Err(ctx.host.reject(NativeViolation::InvalidProvenance));
    }
    ctx.transport_table_pipe(
        RSpan {
            buffer,
            start: old_len,
            end: expected_end,
        },
        source,
    )
}

fn copy_cell_part<H: Host>(
    ctx: &mut Context<'_, H>,
    result: &mut String,
    buffer: BufferId,
    text: &str,
    raw: RSpan,
    start: usize,
    end: usize,
) -> Result<(), H::Error> {
    if start == end {
        return Ok(());
    }
    let actual = text
        .get(start..end)
        .ok_or_else(|| ctx.host.reject(NativeViolation::InvalidProvenance))?;
    let source = raw_part(ctx, raw, start, end)?;
    let old_len = result.len();
    ctx.append(result, actual)?;
    let expected_end = add_offset(ctx, old_len, end - start)?;
    if result.len() != expected_end {
        return Err(ctx.host.reject(NativeViolation::InvalidProvenance));
    }
    ctx.transport_append_from_buffer(
        RSpan {
            buffer,
            start: old_len,
            end: expected_end,
        },
        source,
    )
}

fn emit_cells<'a, H: Host>(
    parser: &mut Parser<'a, '_, '_, '_, '_, '_, H>,
    row: Node<'a>,
    text: &str,
    raw: RSpan,
    scan: &RowScan,
    columns: usize,
    diagnostic_column: usize,
) -> Result<(), H::Error> {
    let row_id = row
        .data()
        .native_id
        .ok_or_else(|| parser.ctx.host.reject(NativeViolation::InvalidProvenance))?;
    let mut cursor = 0;
    for (column, cell) in scan.cells.iter().copied().enumerate() {
        parser.ctx.event()?;
        cut(
            parser.ctx,
            raw,
            cursor,
            cell.raw_start,
            SyntaxRole::TableSeparator,
        )?;
        if column >= columns {
            cut(
                parser.ctx,
                raw,
                cell.raw_start,
                cell.raw_end,
                SyntaxRole::TableExtraCell,
            )?;
        } else {
            cut(
                parser.ctx,
                raw,
                cell.raw_start,
                cell.trim_start,
                SyntaxRole::TableTrim,
            )?;
            cut(
                parser.ctx,
                raw,
                cell.trim_end,
                cell.raw_end,
                SyntaxRole::TableTrim,
            )?;
            emit_cell(
                parser,
                row,
                row_id,
                text,
                raw,
                cell,
                column,
                false,
                diagnostic_column,
            )?;
        }
        cursor = cell.raw_end;
    }
    cut(
        parser.ctx,
        raw,
        cursor,
        text.len(),
        SyntaxRole::TableSeparator,
    )?;
    for column in scan.cells.len()..columns {
        parser.ctx.event()?;
        let at = scan.anchor;
        emit_cell(
            parser,
            row,
            row_id,
            text,
            raw,
            CellScan {
                raw_start: at,
                raw_end: at,
                trim_start: at,
                trim_end: at,
                alignment: None,
            },
            column,
            true,
            diagnostic_column,
        )?;
    }
    Ok(())
}

#[allow(clippy::too_many_arguments)]
fn emit_cell<'a, H: Host>(
    parser: &mut Parser<'a, '_, '_, '_, '_, '_, H>,
    row: Node<'a>,
    row_id: crate::native::NativeNodeId,
    text: &str,
    raw: RSpan,
    cell: CellScan,
    column: usize,
    missing: bool,
    diagnostic_column: usize,
) -> Result<(), H::Error> {
    let start = row.data().sourcepos.start;
    let node = native_node(
        parser,
        row,
        NodeValue::TableCell,
        NativeKind::TableCell,
        start,
    )?;
    let node_id = node.data().native_id.expect("native cell");
    let buffer = parser.ctx.buffer(node_id)?;
    let content = cell_content(parser.ctx, text, raw, cell, buffer)?;
    let mut offsets = smallvec::SmallVec::new();
    let offset = add_offset(parser.ctx, diagnostic_column, cell.trim_start)?;
    parser.ctx.small_push(&mut offsets, offset)?;
    let lines = parser.ctx.boxed(BlockLines {
        buffer: Some(buffer),
        base: 0,
        opened_line: start.line,
        eof_anchor: None,
        content,
        offsets,
    })?;
    node.data_mut().lines = Some(lines);
    let source = raw_part(parser.ctx, raw, cell.raw_start, cell.raw_end)?;
    let column = u32::try_from(column)
        .map_err(|_| parser.ctx.host.reject(NativeViolation::ArithmeticOverflow))?;
    parser.ctx.fact(AcceptedFact::G3(g3::Fact::Cell {
        node: node_id,
        row: row_id,
        column,
        missing,
        raw: source,
        content: buffer,
    }))
}

fn captured_block<'a, H: Host>(
    parser: &mut Parser<'a, '_, '_, '_, '_, '_, H>,
    container: Node<'a>,
    line: &str,
) -> Result<Option<TableBlockResult<'a>>, H::Error> {
    let paragraph = matches!(container.data().value, NodeValue::Paragraph);
    if paragraph {
        captured_header(parser, container, line)
    } else if matches!(container.data().value, NodeValue::Table(_)) {
        Ok(captured_row(parser, container, line)?.map(TableBlockResult::from_legacy))
    } else {
        Ok(None)
    }
}

/// Locate only the immediately preceding physical row. Failed probes never
/// scan the paragraph prefix or leave a paragraph-lifetime negative memo.
fn last_header<H: Host>(ctx: &mut Context<'_, H>, text: &str) -> Result<(usize, usize), H::Error> {
    let bytes = text.as_bytes();
    let mut end = bytes.len();
    if end > 0 && bytes[end - 1] == b'\n' {
        ctx.scan_step()?;
        end -= 1;
    }
    if end > 0 && bytes[end - 1] == b'\r' {
        ctx.scan_step()?;
        end -= 1;
    }
    let mut begin = end;
    while begin > 0 {
        ctx.scan_step()?;
        if matches!(bytes[begin - 1], b'\r' | b'\n') {
            break;
        }
        begin -= 1;
    }
    let mut pipe = begin;
    while pipe < end {
        ctx.scan_step()?;
        if !matches!(bytes[pipe], b' ' | b'\t') {
            break;
        }
        pipe += 1;
    }
    Ok((begin, pipe - begin))
}

struct AcceptedHeader<'a> {
    table: Node<'a>,
    text: String,
    raw: RSpan,
    start: LineColumn,
    offset: usize,
}

/// Commit the already-probed last row without changing the old paragraph's
/// accepted buffer coordinates or replaying previously consumed definitions.
fn split_header<'a, H: Host>(
    parser: &mut Parser<'a, '_, '_, '_, '_, '_, H>,
    paragraph: Node<'a>,
    value: NodeValue,
    begin: usize,
    pipe: usize,
) -> Result<AcceptedHeader<'a>, H::Error> {
    let parent = paragraph
        .parent()
        .ok_or_else(|| parser.ctx.host.reject(NativeViolation::InvalidProvenance))?;
    if !parser.current.same_node(paragraph)
        || !parent.last_child().is_some_and(|n| n.same_node(paragraph))
    {
        return Err(parser.ctx.host.reject(NativeViolation::InvalidProvenance));
    }
    let (prefix, header, original, first, start, offset, prefix_end, rows) = {
        let data = paragraph.data();
        let lines = data
            .lines
            .as_ref()
            .ok_or_else(|| parser.ctx.host.reject(NativeViolation::InvalidProvenance))?;
        let raw = RSpan {
            buffer: lines
                .buffer
                .ok_or_else(|| parser.ctx.host.reject(NativeViolation::InvalidProvenance))?,
            start: lines.base,
            end: add_offset(parser.ctx, lines.base, lines.content.len())?,
        };
        let prefix = lines
            .content
            .get(..begin)
            .ok_or_else(|| parser.ctx.host.reject(NativeViolation::InvalidProvenance))?;
        let header = lines
            .content
            .get(begin..)
            .ok_or_else(|| parser.ctx.host.reject(NativeViolation::InvalidProvenance))?;
        if header.as_bytes().get(pipe) != Some(&b'|') {
            return Err(parser.ctx.host.reject(NativeViolation::InvalidProvenance));
        }
        let mut rows = 0usize;
        let mut row_start = 0usize;
        let mut last_width = 0usize;
        for (at, byte) in prefix.bytes().enumerate() {
            parser.ctx.scan_step()?;
            if byte == b'\n' {
                last_width = at - row_start;
                row_start = at + 1;
                rows = add_offset(parser.ctx, rows, 1)?;
            }
        }
        let line_count = add_offset(parser.ctx, rows, 1)?;
        if rows == 0 || row_start != begin || lines.offsets.len() != line_count {
            return Err(parser.ctx.host.reject(NativeViolation::InvalidProvenance));
        }
        let line = add_offset(parser.ctx, lines.opened_line, rows)?;
        if line.checked_add(1) != Some(parser.line_number) {
            return Err(parser.ctx.host.reject(NativeViolation::InvalidProvenance));
        }
        let offset = lines.offsets[rows];
        let column = add_offset(parser.ctx, offset, pipe)?;
        let start = LineColumn {
            line,
            column: add_offset(parser.ctx, column, 1)?,
        };
        let prefix_end = LineColumn {
            line: line - 1,
            column: add_offset(parser.ctx, lines.offsets[rows - 1], last_width)?,
        };
        (
            parser.ctx.string(prefix)?,
            parser.ctx.string(header)?,
            raw,
            add_offset(parser.ctx, lines.base, begin)?,
            start,
            offset,
            prefix_end,
            rows,
        )
    };
    parser.ctx.fact(AcceptedFact::Seal {
        buffer: original.buffer,
        len: original.end,
    })?;
    let paragraph_id = paragraph
        .data()
        .native_id
        .ok_or_else(|| parser.ctx.host.reject(NativeViolation::InvalidProvenance))?;
    let prefix_buffer = parser.ctx.buffer(paragraph_id)?;
    parser.ctx.transport_append_from_buffer(
        RSpan {
            buffer: prefix_buffer,
            start: 0,
            end: prefix.len(),
        },
        RSpan {
            end: first,
            ..original
        },
    )?;
    let old = {
        let mut data = paragraph.data_mut();
        let lines = data.lines_mut();
        let old = mem::replace(&mut lines.content, prefix);
        lines.buffer = Some(prefix_buffer);
        lines.base = 0;
        lines.eof_anchor = None;
        lines.offsets.truncate(rows);
        old
    };
    let table = native_node(parser, parent, value, NativeKind::Table, start)?;
    {
        let mut data = table.data_mut();
        data.open = true;
        data.sourcepos.end = (parser.line_number, parser.curline_end_col).into();
    }
    let table_id = table
        .data()
        .native_id
        .ok_or_else(|| parser.ctx.host.reject(NativeViolation::InvalidProvenance))?;
    let buffer = parser.ctx.buffer(table_id)?;
    let raw = RSpan {
        buffer,
        start: 0,
        end: header.len(),
    };
    parser.ctx.transport_append_from_buffer(
        raw,
        RSpan {
            start: first,
            ..original
        },
    )?;
    parser.ctx.fact(AcceptedFact::Seal {
        buffer,
        len: header.len(),
    })?;
    let pipe_end = add_offset(parser.ctx, pipe, 1)?;
    let anchor = parser.ctx.accepted_copy_piece(RSpan {
        buffer,
        start: pipe,
        end: pipe_end,
    })?;
    parser.ctx.fact(AcceptedFact::BlockStart {
        node: table_id,
        at: anchor.start,
    })?;
    // Ordinary finalize owns any still-unconsumed definitions in the prefix.
    // Shared handle_table switches current only after this whole commit succeeds.
    parser.finalize(paragraph)?;
    paragraph.data_mut().sourcepos.end = prefix_end;
    parser.ctx.discard_string(old)?;
    Ok(AcceptedHeader {
        table,
        text: header,
        raw,
        start,
        offset,
    })
}

fn captured_header<'a, H: Host>(
    parser: &mut Parser<'a, '_, '_, '_, '_, '_, H>,
    container: Node<'a>,
    line: &str,
) -> Result<Option<TableBlockResult<'a>>, H::Error> {
    let delimiter_text = line
        .get(parser.first_nonspace..)
        .ok_or_else(|| parser.ctx.host.reject(NativeViolation::InvalidProvenance))?;
    let Some(mut delimiter) = scan_row(parser.ctx, delimiter_text)? else {
        return Ok(Some(TableBlockResult::from_legacy((
            container, false, false,
        ))));
    };
    for cell in &mut delimiter.cells {
        cell.alignment =
            scan_alignment(parser.ctx, &delimiter_text[cell.trim_start..cell.trim_end])?;
        if cell.alignment.is_none() {
            parser.ctx.discard_vec(delimiter.cells)?;
            return Ok(Some(TableBlockResult::from_legacy((
                container, false, false,
            ))));
        }
    }
    let (begin, pipe, header) = {
        let data = container.data();
        let lines = data
            .lines
            .as_ref()
            .ok_or_else(|| parser.ctx.host.reject(NativeViolation::InvalidProvenance))?;
        let (begin, pipe) = last_header(parser.ctx, &lines.content)?;
        if begin != 0 && lines.content.as_bytes().get(begin + pipe) != Some(&b'|') {
            parser.ctx.discard_vec(delimiter.cells)?;
            return Ok(Some(TableBlockResult::from_legacy((
                container, false, false,
            ))));
        }
        (begin, pipe, scan_row(parser.ctx, &lines.content[begin..])?)
    };
    let Some(header) = header else {
        parser.ctx.discard_vec(delimiter.cells)?;
        return Ok(Some(TableBlockResult::from_legacy((
            container, false, false,
        ))));
    };
    if header.cells.len() != delimiter.cells.len() {
        parser.ctx.discard_vec(header.cells)?;
        parser.ctx.discard_vec(delimiter.cells)?;
        return Ok(Some(TableBlockResult::from_legacy((
            container, false, false,
        ))));
    }
    let columns = u32::try_from(header.cells.len())
        .map_err(|_| parser.ctx.host.reject(NativeViolation::ArithmeticOverflow))?;
    // First mutation: all grammar probes above have completed without IDs,
    // accepted facts or changes to the original paragraph content.
    let mut alignments = Vec::new();
    for cell in &delimiter.cells {
        parser.ctx.event()?;
        parser.ctx.push(
            &mut alignments,
            match cell.alignment.expect("accepted alignment") {
                g3::Alignment::None => TableAlignment::None,
                g3::Alignment::Left => TableAlignment::Left,
                g3::Alignment::Right => TableAlignment::Right,
                g3::Alignment::Center => TableAlignment::Center,
            },
        )?;
    }
    let value = parser.ctx.boxed(NodeTable {
        alignments,
        num_columns: columns as usize,
        num_rows: 1,
        num_nonempty_cells: columns as usize,
    })?;
    let accepted = if begin != 0 {
        split_header(parser, container, NodeValue::Table(value), begin, pipe)?
    } else {
        let table_id = container
            .data()
            .native_id
            .ok_or_else(|| parser.ctx.host.reject(NativeViolation::InvalidProvenance))?;
        parser.ctx.fact(AcceptedFact::Retyped {
            node: table_id,
            kind: NativeKind::Table,
        })?;
        let (lines, start) = {
            let mut data = container.data_mut();
            let lines = data
                .lines
                .take()
                .ok_or_else(|| parser.ctx.host.reject(NativeViolation::InvalidProvenance))?;
            data.value = NodeValue::Table(value);
            data.sourcepos.end = (parser.line_number, parser.curline_end_col).into();
            (parser.ctx.unbox(lines)?, data.sourcepos.start)
        };
        let buffer = lines
            .buffer
            .ok_or_else(|| parser.ctx.host.reject(NativeViolation::InvalidProvenance))?;
        let end = add_offset(parser.ctx, lines.base, lines.content.len())?;
        let raw = RSpan {
            buffer,
            start: lines.base,
            end,
        };
        parser.ctx.fact(AcceptedFact::Seal { buffer, len: end })?;
        let offset = lines.offsets.first().copied().unwrap_or(0);
        parser.ctx.discard_small(lines.offsets)?;
        AcceptedHeader {
            table: container,
            text: lines.content,
            raw,
            start,
            offset,
        }
    };
    let AcceptedHeader {
        table,
        text: header_text,
        raw: header_raw,
        start,
        offset,
    } = accepted;
    let table_id = table
        .data()
        .native_id
        .ok_or_else(|| parser.ctx.host.reject(NativeViolation::InvalidProvenance))?;
    // The container prefix is already consumed. Only the remaining delimiter
    // indentation belongs to this Table; emit before advancing the line cursor.
    parser.block_whitespace(table, parser.offset, parser.first_nonspace)?;
    let delimiter_raw = line_buffer(parser.ctx, table_id, line, parser.first_nonspace)?;
    parser.ctx.fact(AcceptedFact::G3(g3::Fact::Table {
        node: table_id,
        columns,
        delimiter: delimiter_raw,
    }))?;
    for (column, cell) in delimiter.cells.iter().enumerate() {
        parser.ctx.event()?;
        let witness = raw_part(parser.ctx, delimiter_raw, cell.trim_start, cell.trim_end)?;
        parser.ctx.fact(AcceptedFact::G3(g3::Fact::Alignment {
            table: table_id,
            column: column as u32,
            value: cell.alignment.expect("accepted alignment"),
            delimiter_cell: witness,
        }))?;
    }
    cut(
        parser.ctx,
        delimiter_raw,
        0,
        delimiter_text.len(),
        SyntaxRole::TableDelimiter,
    )?;
    let row = native_node(
        parser,
        table,
        NodeValue::TableRow(true),
        NativeKind::TableRow,
        start,
    )?;
    let row_id = row.data().native_id.expect("native header row");
    parser.ctx.fact(AcceptedFact::G3(g3::Fact::Row {
        node: row_id,
        table: table_id,
        index: 0,
        header: true,
        raw: header_raw,
    }))?;
    emit_cells(
        parser,
        row,
        &header_text,
        header_raw,
        &header,
        columns as usize,
        offset,
    )?;
    parser.ctx.discard_string(header_text)?;
    parser.ctx.discard_vec(header.cells)?;
    parser.ctx.discard_vec(delimiter.cells)?;
    let advance = line
        .len()
        .checked_sub(newlines_of(line))
        .and_then(|n| n.checked_sub(parser.offset))
        .ok_or_else(|| parser.ctx.host.reject(NativeViolation::InvalidProvenance))?;
    parser.advance_offset(line, advance, false)?;
    Ok(Some(TableBlockResult {
        container: table,
        replace: false,
        mark_visited: false,
        split: begin != 0,
    }))
}

fn captured_row<'a, H: Host>(
    parser: &mut Parser<'a, '_, '_, '_, '_, '_, H>,
    table: Node<'a>,
    line: &str,
) -> Result<Option<(Node<'a>, bool, bool)>, H::Error> {
    if parser.blank {
        return Ok(None);
    }
    let text = line
        .get(parser.first_nonspace..)
        .ok_or_else(|| parser.ctx.host.reject(NativeViolation::InvalidProvenance))?;
    let Some(scan) = scan_row(parser.ctx, text)? else {
        return Ok(None);
    };
    let (columns, index) = match table.data().value {
        NodeValue::Table(ref value) => (value.num_columns, value.num_rows),
        _ => return Err(parser.ctx.host.reject(NativeViolation::InvalidProvenance)),
    };
    let next_rows = index
        .checked_add(1)
        .ok_or_else(|| parser.ctx.host.reject(NativeViolation::ArithmeticOverflow))?;
    columns
        .checked_mul(next_rows)
        .ok_or_else(|| parser.ctx.host.reject(NativeViolation::ArithmeticOverflow))?;
    let index = u32::try_from(index)
        .map_err(|_| parser.ctx.host.reject(NativeViolation::ArithmeticOverflow))?;
    let start = LineColumn {
        line: parser.line_number,
        column: parser.first_nonspace + 1,
    };
    let row = native_node(
        parser,
        table,
        NodeValue::TableRow(false),
        NativeKind::TableRow,
        start,
    )?;
    // This row becomes the parser's current block and is finalized through the
    // ordinary next-line/EOF lifecycle. Header rows and cells never do.
    row.data_mut().open = true;
    let row_id = row.data().native_id.expect("native row");
    let table_id = table.data().native_id.expect("native table");
    // Only the accepted row consumes this indentation; container prefixes
    // already end at offset and must retain their original authority.
    parser.block_whitespace(row, parser.offset, parser.first_nonspace)?;
    let raw = line_buffer(parser.ctx, row_id, line, parser.first_nonspace)?;
    parser.ctx.fact(AcceptedFact::G3(g3::Fact::Row {
        node: row_id,
        table: table_id,
        index,
        header: false,
        raw,
    }))?;
    emit_cells(
        parser,
        row,
        text,
        raw,
        &scan,
        columns,
        parser.first_nonspace,
    )?;
    let actual_cells = columns.min(scan.cells.len());
    parser.ctx.discard_vec(scan.cells)?;
    let mut data = table.data_mut();
    if let NodeValue::Table(ref mut value) = data.value {
        value.num_rows = next_rows;
        value.num_nonempty_cells = value
            .num_nonempty_cells
            .checked_add(actual_cells)
            .ok_or_else(|| parser.ctx.host.reject(NativeViolation::ArithmeticOverflow))?;
    }
    data.sourcepos.end = (parser.line_number, parser.curline_end_col).into();
    drop(data);
    row.data_mut().sourcepos.end = (parser.line_number, parser.curline_end_col).into();
    let advance = line
        .len()
        .checked_sub(newlines_of(line))
        .and_then(|n| n.checked_sub(parser.offset))
        .ok_or_else(|| parser.ctx.host.reject(NativeViolation::InvalidProvenance))?;
    parser.advance_offset(line, advance, false)?;
    Ok(Some((row, false, false)))
}

pub fn matches(line: &str, spoiler: bool) -> bool {
    row(line, spoiler).is_some()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn g3_pipe_acceptance_checks_borrowed_bytes_before_append_or_claim() {
        use crate::native::g3::tests::{Error, TestHost};
        use std::sync::atomic::Ordering;
        let mut host = TestHost::new();
        let live = host.live.clone();
        let mut ctx = Context::new(&mut host, 34);
        let mut output = ctx.string("a").unwrap();
        let result = accept_table_pipe(
            &mut ctx,
            &mut output,
            BufferId::from_raw(2).unwrap(),
            "a!|b",
            RSpan {
                buffer: BufferId::from_raw(1).unwrap(),
                start: 11,
                end: 15,
            },
            1,
        );
        assert_eq!(
            result,
            Err(Error::Violation(NativeViolation::InvalidProvenance))
        );
        assert_eq!(output, "a");
        drop(output);
        drop(ctx);
        assert_eq!(host.facts, 0);
        assert_eq!(
            host.first,
            Some(Error::Violation(NativeViolation::InvalidProvenance))
        );
        assert_eq!(live.load(Ordering::SeqCst), 0);
    }

    #[test]
    fn pipe_unescaping_preserves_backslash_pairs_and_utf8() {
        for count in 0..=6 {
            let input = format!("{}{}|雪", "語".repeat(32), "\\".repeat(count));
            let output = unescape_pipes(&input);
            let expected = format!("{}{}|雪", "語".repeat(32), "\\".repeat(count - count % 2));
            assert_eq!(output, expected);
            if count % 2 == 0 {
                assert!(matches!(output, Cow::Borrowed(_)));
            } else {
                assert!(matches!(output, Cow::Owned(_)));
            }
        }
    }

    #[test]
    fn pipe_unescaping_borrows_unchanged_input() {
        for input in ["", "雪", "plain | text", r"a\\|b", r"a\雪|b", r"\"] {
            let output = unescape_pipes(input);
            assert_eq!(output, input);
            match output {
                Cow::Borrowed(_) => (),
                Cow::Owned(_) => panic!("unchanged input should be borrowed"),
            }
        }
    }
}
