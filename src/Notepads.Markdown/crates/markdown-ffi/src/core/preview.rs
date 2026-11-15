// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE file in the project root for license information.
// ---------------------------------------------------------------------------------------------

//! Private preview ABI3. Every mutable layout has one original cancellation flag.

use super::*;
use markdown_core::identity::LogicalSpan;
use markdown_core::ir::{Alignment, Rows};
use markdown_core::layout::{
    BlockKind, Disclosure, DisclosureOverride, DocumentLayout, LayoutDraft, LayoutKey, LayoutStyle,
    MeasuredBlock, MeasurementBatch, Viewport,
};
use markdown_core::presentation::{self, PresentationBatch};

mod wire;
pub use wire::*;
mod derived;
pub use derived::*;
mod containers;
pub use containers::*;

enum LayoutState {
    Draft(LayoutDraft),
    Complete(DocumentLayout),
    Empty,
}

pub struct PreviewLayout {
    // Projection and geometry storage retire before the final model/control pin.
    batch: Option<PresentationBatch>,
    presenting_prefix: bool,
    semantic_only: bool,
    scope: Option<LogicalSpan>,
    state: LayoutState,
    model: AcceptedModel,
    cancel: CancellationToken,
    key: LayoutKey,
    fault: Option<CoreError>,
}

pub struct PreviewAllocation {
    pub(super) reservation: BoundaryReservation,
    pub(super) admitted: bool,
    pub(super) svg_attribution: u64,
}

impl PreviewLayout {
    fn operation(&self) -> Result<Operation, CoreError> {
        if let Some(error) = self.fault {
            return Err(error);
        }
        let mut op = self.model.budget().begin(self.cancel.clone());
        op.boundary_work().checkpoint_now()?;
        if let LayoutState::Complete(layout) = &self.state {
            layout.check_install(self.key, &mut op)?;
        }
        Ok(op)
    }

    fn latch(&mut self, error: CoreError) -> CoreError {
        *self.fault.get_or_insert(error)
    }

    fn visibility(&self) -> Result<(&[Disclosure], &[LogicalSpan]), CoreError> {
        match &self.state {
            LayoutState::Draft(draft) => Ok((draft.disclosures(), draft.hidden_ranges())),
            LayoutState::Complete(layout) => Ok((layout.disclosures(), layout.hidden_ranges())),
            LayoutState::Empty => Err(CoreError::InvalidState),
        }
    }

    fn summary(&self, start: usize) -> PreviewSummary {
        let mut result = PreviewSummary {
            header: prefix::<PreviewSummary>(),
            key: to_wire_key(self.key),
            start: start as u64,
            ..Default::default()
        };
        if let Some(batch) = &self.batch {
            result.start = batch.start() as u64;
            result.blocks = batch.blocks().len() as u64;
            result.texts = batch.texts().len() as u64;
            result.runs = batch.runs().len() as u64;
            result.cells = batch.cells().len() as u64;
            result.decorations = batch.decorations().len() as u64;
            result.images = batch.images().len() as u64;
            result.links = batch.links().len() as u64;
            result.blob_bytes = batch.blob().len() as u64;
            result.ruby_units = batch.ruby_units().len() as u64;
        }
        result
    }
}

fn from_wire_key(value: PreviewKey) -> Result<LayoutKey, CoreError> {
    LayoutKey::new(
        identity(value.model)?,
        value.layout_generation,
        value.theme_generation,
        value.measurement_generation,
        value.width,
        LayoutStyle::markdown(value.indent, value.gap)?,
    )
}

fn to_wire_key(value: LayoutKey) -> PreviewKey {
    PreviewKey {
        header: prefix::<PreviewKey>(),
        model: wire_identity(value.model()),
        layout_generation: value.layout_generation(),
        theme_generation: value.theme_generation(),
        measurement_generation: value.measurement_generation(),
        width: value.document_width(),
        indent: value.style().indent(),
        gap: value.style().block_gap(),
    }
}

unsafe fn preview_header<T>(pointer: *const T) -> Result<(), i32> {
    if pointer.is_null() || !(pointer as usize).is_multiple_of(align_of::<T>()) {
        return Err(STATUS_INVALID_ARGUMENT);
    }
    // SAFETY: ABI caller supplies a readable aligned 8-byte prefix.
    let value = unsafe { pointer.cast::<WireHeader>().read() };
    if value.abi_version != PREVIEW_ABI_VERSION || value.struct_size as usize != size_of::<T>() {
        return Err(STATUS_ABI_MISMATCH);
    }
    Ok(())
}

unsafe fn layout_action(
    handle: *mut Owned<PreviewLayout>,
    error: *mut WireError,
    work: impl FnOnce(&mut PreviewLayout, &mut Operation) -> Result<(), CoreError>,
) -> i32 {
    // SAFETY: caller holds the sole layout borrow; action validates error before dereference.
    let code = unsafe {
        action(error, || {
            let layout = &mut (*handle).value;
            let mut op = layout.operation().map_err(|e| layout.latch(e))?;
            work(layout, &mut op).map_err(|e| layout.latch(op.boundary_failure(e)))
        })
    };
    if code == STATUS_PANIC {
        // SAFETY: the exclusive handle remains live after panic containment.
        unsafe {
            (*handle).value.latch(CoreError::InvalidState);
        }
    }
    code
}

#[unsafe(no_mangle)]
pub extern "C" fn notepads_markdown_preview_abi_version() -> u32 {
    PREVIEW_ABI_VERSION
}

/// # Safety
/// Writable independent handle/error storage satisfying the ABI3 error prefix.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn notepads_markdown_preview_cancel_create(
    output: *mut *mut Owned<CoreCancel>,
    error: *mut WireError,
) -> i32 {
    handle_output!(output);
    // SAFETY: checked output storage; the new flag owns its separate root.
    unsafe {
        action(error, || {
            output.write(new_control_cancel(SourceBudget::preview())?);
            Ok(())
        })
    }
}

/// # Safety
/// Live shared model/cancel, readable key prefix/full POD and independent outputs.
/// Nonempty overrides must be aligned and readable for count rows for the duration of the call.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn notepads_markdown_preview_layout_create(
    model: *const Owned<CoreModel>,
    cancel: *const Owned<CoreCancel>,
    key: *const PreviewKey,
    overrides: *const DisclosureOverride,
    override_count: u32,
    purpose: u32,
    section_heading: u32,
    output: *mut *mut Owned<PreviewLayout>,
    error: *mut WireError,
) -> i32 {
    handle_output!(output);
    preflight!(
        valid_handle(model)
            && valid_handle(cancel)
            && (override_count == 0
                || (!overrides.is_null()
                    && (overrides as usize).is_multiple_of(align_of::<DisclosureOverride>())))
    );
    if let Err(code) = unsafe { preview_header(key) } {
        return code;
    }
    // SAFETY: all shared handles and the accepted full key are live during this call.
    unsafe {
        action(error, || {
            let model = (*model).value.model.clone();
            let cancel = &(*cancel).value;
            if !cancel.budget.same_root(model.budget()) {
                return Err(CoreError::WrongBudget);
            }
            let key = from_wire_key(key.read())?;
            let mut op = cancel.budget.begin(cancel.token.clone());
            let override_count = override_count as usize;
            if override_count > model.disclosure_regions().len()
                || override_count > isize::MAX as usize / size_of::<DisclosureOverride>()
                || purpose > PREVIEW_LAYOUT_SEMANTICS
                || (purpose != PREVIEW_LAYOUT_VISIBLE && override_count != 0)
            {
                return Err(CoreError::InvalidRange);
            }
            let overrides = if override_count == 0 {
                &[]
            } else {
                std::slice::from_raw_parts(overrides, override_count)
            };
            let slot = Slot::new(op.boundary_work())?;
            let draft = if section_heading != 0 {
                LayoutDraft::section(
                    model.clone(),
                    key,
                    section_heading,
                    (purpose == PREVIEW_LAYOUT_VISIBLE).then_some(overrides),
                    &mut op,
                )?
            } else if purpose == PREVIEW_LAYOUT_VISIBLE {
                LayoutDraft::with_disclosures(model.clone(), key, overrides, &mut op)?
            } else {
                LayoutDraft::unfiltered_projection(model.clone(), key, &mut op)?
            };
            op.boundary_work().checkpoint_now()?;
            output.write(slot.commit(PreviewLayout {
                batch: None,
                presenting_prefix: false,
                semantic_only: purpose == PREVIEW_LAYOUT_SEMANTICS,
                scope: draft.scope(),
                state: LayoutState::Draft(draft),
                model,
                cancel: cancel.token.clone(),
                key,
                fault: None,
            }));
            Ok(())
        })
    }
}

/// # Safety
/// Exclusive live layout and independent initialized visibility/error prefixes.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn notepads_markdown_preview_visibility(
    handle: *mut Owned<PreviewLayout>,
    output: *mut PreviewVisibility,
    error: *mut WireError,
) -> i32 {
    preflight!(valid_handle(handle));
    // SAFETY: the supplied output starts with an initialized writable prefix.
    if let Err(code) = unsafe { preview_header(output) } {
        return code;
    }
    // SAFETY: exclusive layout and checked output, with no aliasing.
    unsafe {
        layout_action(handle, error, |layout, op| {
            let (disclosures, hidden) = layout.visibility()?;
            let blocks = match &layout.state {
                LayoutState::Draft(draft) => draft.blocks_len(),
                LayoutState::Complete(complete) => complete.requests().len(),
                LayoutState::Empty => return Err(CoreError::InvalidState),
            };
            op.external_write(size_of::<PreviewVisibility>() as u64)?;
            op.boundary_work().checkpoint_now()?;
            output.write(PreviewVisibility {
                header: prefix::<PreviewVisibility>(),
                key: to_wire_key(layout.key),
                disclosures: disclosures.len() as u64,
                hidden_ranges: hidden.len() as u64,
                blocks: blocks as u64,
            });
            Ok(())
        })
    }
}

/// Stream immutable semantic input on a dedicated unfiltered projection. No
/// measurement, presentation prefix, or synthetic geometry is produced.
///
/// # Safety
/// Exclusive live layout and independent initialized summary/error prefixes.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn notepads_markdown_preview_semantic_batch(
    handle: *mut Owned<PreviewLayout>,
    start: u64,
    count: u32,
    output: *mut PreviewSummary,
    error: *mut WireError,
) -> i32 {
    preflight!(valid_handle(handle));
    // SAFETY: the supplied output starts with an initialized writable prefix.
    if let Err(code) = unsafe { preview_header(output) } {
        return code;
    }
    // SAFETY: exclusive layout and checked output, with no aliasing.
    unsafe {
        layout_action(handle, error, |layout, op| {
            let LayoutState::Draft(draft) = &layout.state else {
                return Err(CoreError::InvalidState);
            };
            if !layout.semantic_only {
                return Err(CoreError::InvalidState);
            }
            if !(1..=256).contains(&count) {
                return Err(CoreError::InvalidRange);
            }
            let start = usize::try_from(start).map_err(|_| CoreError::ArithmeticOverflow)?;
            let end = start
                .checked_add(count as usize)
                .ok_or(CoreError::ArithmeticOverflow)?;
            let requests = draft
                .requests()
                .get(start..end)
                .ok_or(CoreError::InvalidRange)?;
            let input = MeasurementBatch {
                key: layout.key,
                start,
                requests,
                scope: layout.scope,
            };
            layout.batch = None;
            layout.batch = Some(PresentationBatch::build(layout.model.clone(), &input, op)?);
            op.external_write(size_of::<PreviewSummary>() as u64)?;
            op.boundary_work().checkpoint_now()?;
            output.write(layout.summary(start));
            Ok(())
        })
    }
}

/// # Safety
/// Exclusive live layout, independent initialized preview summary/error prefixes.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn notepads_markdown_preview_measure_begin(
    handle: *mut Owned<PreviewLayout>,
    maximum: u32,
    output: *mut PreviewSummary,
    error: *mut WireError,
) -> i32 {
    preflight!(valid_handle(handle));
    if let Err(code) = unsafe { preview_header(output) } {
        return code;
    }
    // SAFETY: exclusive handle; summary is written only after the entire projection succeeds.
    unsafe {
        layout_action(handle, error, |layout, op| {
            if layout.semantic_only {
                return Err(CoreError::InvalidState);
            }
            if !(1..=256).contains(&maximum) {
                return Err(CoreError::InvalidRange);
            }
            let LayoutState::Draft(draft) = &mut layout.state else {
                return Err(CoreError::InvalidState);
            };
            let start = draft.completed_blocks();
            if std::mem::take(&mut layout.presenting_prefix) {
                layout.batch = None;
            }
            if layout.batch.is_none()
                && let Some(batch) = draft.measurement_batch(maximum as usize, op)?
            {
                layout.batch = Some(PresentationBatch::build(layout.model.clone(), &batch, op)?);
            }
            let value = layout.summary(start);
            op.external_write(size_of::<PreviewSummary>() as u64)?;
            op.boundary_work().checkpoint_now()?;
            output.write(value);
            Ok(())
        })
    }
}

fn rows<T: Copy>(value: Rows<T>) -> WireRows {
    WireRows {
        start: value.start() as u32,
        count: value.len() as u32,
    }
}
/// # Safety
/// Exclusive live layout and independent writable summary/error PODs. The summary
/// has the independently versioned script prefix; its key pins the current batch.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn notepads_markdown_preview_script_summary(
    handle: *mut Owned<PreviewLayout>,
    output: *mut PreviewScriptSummary,
    error: *mut WireError,
) -> i32 {
    preflight!(
        valid_handle(handle)
            && !output.is_null()
            && (output as usize).is_multiple_of(align_of::<PreviewScriptSummary>())
    );
    // SAFETY: the caller supplies an initialized readable eight-byte prefix.
    let supplied = unsafe { output.cast::<WireHeader>().read() };
    if supplied.abi_version != PREVIEW_SCRIPT_ABI_VERSION
        || supplied.struct_size != size_of::<PreviewScriptSummary>() as u32
    {
        return STATUS_ABI_MISMATCH;
    }
    // SAFETY: the accepted size covers the full independent writable POD.
    unsafe {
        layout_action(handle, error, |layout, op| {
            let batch = layout.batch.as_ref().ok_or(CoreError::InvalidState)?;
            op.external_write(size_of::<PreviewScriptSummary>() as u64)?;
            op.boundary_work().checkpoint_now()?;
            output.write(PreviewScriptSummary {
                header: WireHeader {
                    abi_version: PREVIEW_SCRIPT_ABI_VERSION,
                    struct_size: size_of::<PreviewScriptSummary>() as u32,
                },
                key: to_wire_key(batch.key()),
                start: batch.start() as u64,
                rows: batch.script_scopes().len() as u64,
            });
            Ok(())
        })
    }
}

fn blob(value: presentation::BlobSlice) -> PreviewBlob {
    PreviewBlob {
        start: value.start as u64,
        length: value.len as u64,
    }
}

fn block_row(layout: &PreviewLayout, index: usize) -> Result<PreviewBlock, CoreError> {
    let batch = layout.batch.as_ref().ok_or(CoreError::InvalidState)?;
    let block = &batch.blocks()[index];
    let request = block.request;
    let placements = match &layout.state {
        LayoutState::Complete(complete) => Some(complete.placements()),
        LayoutState::Draft(draft) if layout.presenting_prefix => Some(draft.prefix_placements()),
        _ => None,
    };
    let (y, height) = if let Some(rows) = placements {
        let placement = rows
            .get(batch.start() + index)
            .ok_or(CoreError::InvalidRange)?;
        (placement.y, placement.height)
    } else {
        (0.0, 0.0)
    };
    Ok(PreviewBlock {
        node: request.node.value(),
        kind: match request.kind {
            BlockKind::Paragraph => 1,
            BlockKind::Heading => 2,
            BlockKind::CodeBlock => 3,
            BlockKind::HtmlBlock => 4,
            BlockKind::ThematicBreak => 5,
            BlockKind::Table => 6,
            BlockKind::EmptyListItem => 7,
        },
        logical_start: request.logical.start(),
        logical_end: request.logical.end(),
        x: request.x,
        width: request.available_width,
        y,
        height,
        quote_depth: request.context.quote_depth,
        item_depth: request.context.item_depth,
        heading_level: block.heading_level,
        columns: block.columns,
        row_count: block.row_count,
        alignment: layout.model.html_alignment(request.node),
        texts: rows(block.texts),
        cells: rows(block.cells),
        decorations: rows(block.decorations),
        code_info: blob(block.code_info),
        code_first_line_number: block.code_first_line_number,
        code_reserved: 0,
        code_highlight_lines: blob(block.code_highlight_lines),
    })
}

unsafe fn write_rows<T: Copy>(
    buffer: *mut std::ffi::c_void,
    count: usize,
    op: &mut Operation,
    mut value: impl FnMut(usize) -> Result<T, CoreError>,
) -> Result<(), CoreError> {
    if !(buffer as usize).is_multiple_of(align_of::<T>()) {
        return Err(CoreError::InvalidBoundary);
    }
    op.external_write((count * size_of::<T>()) as u64)?;
    for index in 0..count {
        op.external_events(1)?;
        let row = value(index)?;
        // SAFETY: caller supplies count writable rows; table size/alignment/range checked first.
        unsafe {
            buffer.cast::<T>().add(index).write(row);
        }
    }
    Ok(())
}

/// # Safety
/// Exclusive layout; buffer has count*row_size writable aligned bytes, independent result/error.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn notepads_markdown_preview_read(
    handle: *mut Owned<PreviewLayout>,
    table: u32,
    start: u64,
    count: u64,
    row_size: u32,
    buffer: *mut std::ffi::c_void,
    output: *mut WireReadResult,
    error: *mut WireError,
) -> i32 {
    preflight!(valid_handle(handle) && !buffer.is_null());
    if let Err(code) = unsafe { header(output) } {
        return code;
    }
    // SAFETY: live exclusive handle and independent valid writable output/buffer storage.
    unsafe {
        layout_action(handle, error, |layout, op| {
            let start = usize::try_from(start).map_err(|_| CoreError::ArithmeticOverflow)?;
            let count = usize::try_from(count).map_err(|_| CoreError::ArithmeticOverflow)?;
            if matches!(table, PREVIEW_DISCLOSURES | PREVIEW_HIDDEN_RANGES) {
                let (disclosures, hidden) = layout.visibility()?;
                let (size, total) = if table == PREVIEW_DISCLOSURES {
                    (size_of::<PreviewDisclosure>(), disclosures.len())
                } else {
                    (size_of::<PreviewHiddenRange>(), hidden.len())
                };
                let end = start
                    .checked_add(count)
                    .ok_or(CoreError::ArithmeticOverflow)?;
                if row_size as usize != size || count > 256 || end > total {
                    return Err(CoreError::InvalidRange);
                }
                if table == PREVIEW_DISCLOSURES {
                    write_rows(buffer, count, op, |i| {
                        let row = disclosures[start + i];
                        Ok(PreviewDisclosure {
                            node: row.node.value(),
                            summary: row.summary.value(),
                            expanded: u32::from(row.expanded),
                            first_body_node: row.first_body_node,
                            next_node: row.next_node,
                            reserved: 0,
                            summary_start: row.summary_start,
                            body_start: row.body.start(),
                            body_end: row.body.end(),
                        })
                    })?;
                } else {
                    write_rows(buffer, count, op, |i| {
                        let range = hidden[start + i];
                        Ok(PreviewHiddenRange {
                            start: range.start(),
                            end: range.end(),
                        })
                    })?;
                }
                op.external_write(size_of::<WireReadResult>() as u64)?;
                op.boundary_work().checkpoint_now()?;
                output.write(WireReadResult {
                    header: WireHeader::of::<WireReadResult>(),
                    written: count as u64,
                    next: end as u64,
                    done: u32::from(end == total),
                    reserved: 0,
                });
                return Ok(());
            }
            let batch = layout.batch.as_ref().ok_or(CoreError::InvalidState)?;
            let (size, total, limit) = match table {
                1 => (size_of::<PreviewBlock>(), batch.blocks().len(), 256),
                2 => (size_of::<PreviewText>(), batch.texts().len(), 256),
                3 => (size_of::<PreviewRun>(), batch.runs().len(), 256),
                4 => (size_of::<PreviewCell>(), batch.cells().len(), 256),
                5 => (
                    size_of::<PreviewDecoration>(),
                    batch.decorations().len(),
                    256,
                ),
                6 => (size_of::<PreviewImage>(), batch.images().len(), 256),
                7 => (size_of::<PreviewLink>(), batch.links().len(), 256),
                8 => (1, batch.blob().len(), 65_536),
                10 => (
                    size_of::<PreviewScriptScope>(),
                    batch.script_scopes().len(),
                    256,
                ),
                11 => (size_of::<PreviewBlockGeometry>(), batch.blocks().len(), 256),
                PREVIEW_HTML_CELLS => (size_of::<PreviewHtmlCell>(), batch.cells().len(), 256),
                PREVIEW_RUBY_UNITS => (size_of::<PreviewRubyUnit>(), batch.ruby_units().len(), 256),
                _ => return Err(CoreError::UnknownId),
            };
            let end = start
                .checked_add(count)
                .ok_or(CoreError::ArithmeticOverflow)?;
            if row_size as usize != size || count > limit || end > total {
                return Err(CoreError::InvalidRange);
            }
            match table {
                1 => write_rows(buffer, count, op, |i| block_row(layout, start + i))?,
                11 => write_rows(buffer, count, op, |i| {
                    let request = batch.blocks()[start + i].request;
                    Ok(PreviewBlockGeometry {
                        node: request.node.value(),
                        reserved: 0,
                        marker_x: request.marker_x,
                    })
                })?,
                2 => write_rows(buffer, count, op, |i| {
                    let r = &batch.texts()[start + i];
                    Ok(PreviewText {
                        node: r.node.value(),
                        reserved: 0,
                        logical_start: r.logical.start(),
                        logical_end: r.logical.end(),
                        utf16_length: r.utf16_len as u64,
                        utf8: blob(r.value),
                        runs: rows(r.runs),
                        images: rows(r.images),
                    })
                })?,
                3 => {
                    let styles = batch.script_runs();
                    let (mut left, mut right) = (0, styles.len());
                    while left < right {
                        op.external_events(1)?;
                        let middle = left + (right - left) / 2;
                        if (styles[middle].run as usize) < start {
                            left = middle + 1;
                        } else {
                            right = middle;
                        }
                    }
                    let mut cursor = left;
                    write_rows(buffer, count, op, |i| {
                        let index = start + i;
                        let r = &batch.runs()[index];
                        let style = if cursor < styles.len() && styles[cursor].run as usize == index
                        {
                            let style = styles[cursor].style;
                            cursor += 1;
                            style
                        } else {
                            presentation::ScriptStyle::default()
                        };
                        Ok(PreviewRun {
                            start: r.start,
                            end: r.end,
                            flags: r.flags,
                            link: r.link.unwrap_or(u32::MAX),
                            size_scale: style.size_scale,
                            relative_em: style.relative_em,
                            absolute_dip: style.absolute_dip,
                        })
                    })?;
                }
                10 => write_rows(buffer, count, op, |i| {
                    let r = batch.script_scopes()[start + i];
                    Ok(PreviewScriptScope {
                        text: r.text,
                        start: r.start,
                        end: r.end,
                        font_flags: r.font_flags,
                        size_scale: r.style.size_scale,
                        relative_em: r.style.relative_em,
                        absolute_dip: r.style.absolute_dip,
                    })
                })?,
                4 => write_rows(buffer, count, op, |i| {
                    let r = &batch.cells()[start + i];
                    Ok(PreviewCell {
                        node: r.node.value(),
                        row_node: r.row_node.value(),
                        row: r.row,
                        column: r.column,
                        alignment: match r.alignment {
                            Alignment::None => 0,
                            Alignment::Left => 1,
                            Alignment::Center => 2,
                            Alignment::Right => 3,
                        },
                        header: u32::from(r.header),
                        missing: u32::from(r.missing),
                        text: r.text,
                    })
                })?,
                PREVIEW_RUBY_UNITS => write_rows(buffer, count, op, |i| {
                    let ruby = batch.ruby_units()[start + i];
                    Ok(PreviewRubyUnit {
                        text: ruby.text,
                        start: ruby.start,
                        base_end: ruby.base_end,
                        end: ruby.end,
                    })
                })?,
                PREVIEW_HTML_CELLS => write_rows(buffer, count, op, |i| {
                    let r = &batch.cells()[start + i];
                    Ok(PreviewHtmlCell {
                        node: r.node.value(),
                        colspan: r.colspan,
                        flags: r.html_flags,
                        reserved: 0,
                        rowspan: r.rowspan,
                        reserved_geometry: 0,
                    })
                })?,
                5 => write_rows(buffer, count, op, |i| {
                    let r = &batch.decorations()[start + i];
                    let (has_action, action_start, action_end, expected) =
                        r.action.map_or((0, 0, 0, 0), |a| {
                            (
                                1,
                                layout.model.original_source_byte(
                                    a.source.start(),
                                    markdown_core::mapping::Affinity::After,
                                ),
                                layout.model.original_source_byte(
                                    a.source.end(),
                                    markdown_core::mapping::Affinity::Before,
                                ),
                                u32::from_le_bytes([
                                    a.expected[0],
                                    a.expected[1],
                                    a.expected[2],
                                    0,
                                ]),
                            )
                        });
                    Ok(PreviewDecoration {
                        item: r.item.value(),
                        list: r.list.value(),
                        kind: match r.kind {
                            presentation::DecorationKind::Bullet => 1,
                            presentation::DecorationKind::Ordered => 2,
                            presentation::DecorationKind::Task => 3,
                            presentation::DecorationKind::StaticHtmlCheckbox => 4,
                        },
                        depth: r.depth,
                        ordinal: r.ordinal,
                        checked: u32::from(r.checked),
                        object: r.task.map_or(u32::MAX, |id| id.value()),
                        marker: r.marker as u32,
                        has_action,
                        action_start,
                        action_end,
                        expected,
                        reserved: 0,
                        logical_start: r.logical.map_or(u64::MAX, |span| span.start()),
                        logical_end: r.logical.map_or(u64::MAX, |span| span.end()),
                    })
                })?,
                6 => write_rows(buffer, count, op, |i| {
                    let r = &batch.images()[start + i];
                    let (width, height) = r
                        .wiki_size
                        .map(|(width, height)| (u32::from(width), u32::from(height)))
                        .unwrap_or_else(|| layout.model.html_image_size(r.node));
                    Ok(PreviewImage {
                        object: r.object.value(),
                        node: r.node.value(),
                        text: r.text,
                        offset_utf16: r.offset_utf16,
                        width,
                        height,
                        destination: blob(r.destination),
                        title: blob(r.title),
                        alt: blob(r.alt),
                        light_source: blob(r.light_source),
                        dark_source: blob(r.dark_source),
                    })
                })?,
                7 => write_rows(buffer, count, op, |i| {
                    let r = &batch.links()[start + i];
                    let local_position_plus_one = r.local_position.map_or(Ok(0), |position| {
                        u32::try_from(position)
                            .ok()
                            .and_then(|position| position.checked_add(1))
                            .ok_or(CoreError::ArithmeticOverflow)
                    })?;
                    Ok(PreviewLink {
                        node: r.node.value(),
                        local_position_plus_one,
                        local_heading_node: r.local_heading.map_or(0, |node| node.value()),
                        wiki: u32::from(r.wiki),
                        destination: blob(r.destination),
                        title: blob(r.title),
                    })
                })?,
                8 => {
                    op.external_write(count as u64)?;
                    // SAFETY: caller supplies count bytes, source is this immutable checked subrange.
                    ptr::copy_nonoverlapping(
                        batch.blob()[start..end].as_ptr(),
                        buffer.cast::<u8>(),
                        count,
                    );
                }
                _ => return Err(CoreError::UnknownId),
            }
            op.external_write(size_of::<WireReadResult>() as u64)?;
            op.boundary_work().checkpoint_now()?;
            output.write(WireReadResult {
                header: WireHeader::of::<WireReadResult>(),
                written: count as u64,
                next: end as u64,
                done: u32::from(end == total),
                reserved: 0,
            });
            Ok(())
        })
    }
}

/// # Safety
/// Exclusive layout, live full key/metric array of at most 256 aligned rows and valid error prefix.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn notepads_markdown_preview_submit(
    handle: *mut Owned<PreviewLayout>,
    key: *const PreviewKey,
    start: u64,
    metrics: *const PreviewMetric,
    count: u32,
    error: *mut WireError,
) -> i32 {
    preflight!(
        valid_handle(handle)
            && !metrics.is_null()
            && (metrics as usize).is_multiple_of(align_of::<PreviewMetric>())
    );
    if let Err(code) = unsafe { preview_header(key) } {
        return code;
    }
    // SAFETY: readable full key/metric array, exclusively borrowed layout.
    unsafe {
        layout_action(handle, error, |layout, op| {
            if layout.semantic_only {
                return Err(CoreError::InvalidState);
            }
            let key = from_wire_key(key.read())?;
            if key != layout.key {
                return Err(CoreError::StaleResult);
            }
            if layout.presenting_prefix {
                return Err(CoreError::InvalidState);
            }
            let batch = layout.batch.as_ref().ok_or(CoreError::InvalidState)?;
            if !(1..=256).contains(&count)
                || count as usize != batch.blocks().len()
                || start != batch.start() as u64
            {
                return Err(CoreError::InvalidRange);
            }
            let first = batch.blocks()[0].request.node;
            let mut measured = [MeasuredBlock {
                node: first,
                height: 0.0,
            }; 256];
            for (index, measured_block) in measured.iter_mut().enumerate().take(count as usize) {
                op.external_read(size_of::<PreviewMetric>() as u64)?;
                let value = metrics.add(index).read();
                let node = batch.blocks()[index].request.node;
                if value.reserved != 0 || value.node != node.value() {
                    return Err(CoreError::InvalidState);
                }
                *measured_block = MeasuredBlock {
                    node,
                    height: value.height,
                };
            }
            let LayoutState::Draft(draft) = &mut layout.state else {
                return Err(CoreError::InvalidState);
            };
            draft.submit_measurements(
                key,
                usize::try_from(start).map_err(|_| CoreError::ArithmeticOverflow)?,
                &measured[..count as usize],
                op,
            )?;
            layout.batch = None;
            Ok(())
        })
    }
}

/// # Safety
/// Exclusive live layout and independent initialized preview extent/error PODs.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn notepads_markdown_preview_finish(
    handle: *mut Owned<PreviewLayout>,
    output: *mut PreviewExtent,
    error: *mut WireError,
) -> i32 {
    preflight!(valid_handle(handle));
    if let Err(code) = unsafe { preview_header(output) } {
        return code;
    }
    // SAFETY: exclusive handle; successful result is written only after complete acceptance.
    unsafe {
        layout_action(handle, error, |layout, op| {
            if layout.semantic_only {
                return Err(CoreError::InvalidState);
            }
            op.external_write(size_of::<PreviewExtent>() as u64)?;
            let LayoutState::Draft(draft) =
                std::mem::replace(&mut layout.state, LayoutState::Empty)
            else {
                return Err(CoreError::InvalidState);
            };
            let complete = draft.finish(op)?;
            let result = PreviewExtent {
                header: prefix::<PreviewExtent>(),
                key: to_wire_key(layout.key),
                blocks: complete.requests().len() as u64,
                height: complete.total_height(),
            };
            layout.batch = None;
            layout.state = LayoutState::Complete(complete);
            output.write(result);
            Ok(())
        })
    }
}

/// Opt into presentation of a measured draft prefix through the next measure
/// call. A zero block count means no intermediate viewport is available.
///
/// # Safety
/// Exclusive live layout; independent initialized extent and error PODs.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn notepads_markdown_preview_begin_prefix(
    handle: *mut Owned<PreviewLayout>,
    bottom: f64,
    output: *mut PreviewExtent,
    error: *mut WireError,
) -> i32 {
    preflight!(valid_handle(handle));
    if let Err(code) = unsafe { preview_header(output) } {
        return code;
    }
    // SAFETY: exclusive layout and checked, independent output prefixes.
    unsafe {
        layout_action(handle, error, |layout, op| {
            if layout.semantic_only {
                return Err(CoreError::InvalidState);
            }
            let LayoutState::Draft(draft) = &layout.state else {
                return Err(CoreError::InvalidState);
            };
            let covered = draft.covers_prefix(bottom, op)?;
            op.external_write(size_of::<PreviewExtent>() as u64)?;
            op.boundary_work().checkpoint_now()?;
            if layout.presenting_prefix {
                layout.batch = None;
            }
            layout.presenting_prefix = covered;
            output.write(PreviewExtent {
                header: prefix::<PreviewExtent>(),
                key: to_wire_key(layout.key),
                blocks: if covered {
                    draft.completed_blocks() as u64
                } else {
                    0
                },
                height: if covered { draft.prefix_height() } else { 0.0 },
            });
            Ok(())
        })
    }
}

/// # Safety
/// Exclusive layout, readable viewport prefix/full POD and independent writable range/error.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn notepads_markdown_preview_visible(
    handle: *mut Owned<PreviewLayout>,
    viewport: *const PreviewViewport,
    output: *mut PreviewRange,
    error: *mut WireError,
) -> i32 {
    preflight!(valid_handle(handle));
    if let Err(code) = unsafe { preview_header(viewport) } {
        return code;
    }
    // SAFETY: the accepted outer prefix guarantees the full viewport is readable;
    // the nested key still has its own independently versioned ABI prefix.
    if let Err(code) = unsafe { preview_header(ptr::addr_of!((*viewport).key)) } {
        return code;
    }
    if let Err(code) = unsafe { preview_header(output) } {
        return code;
    }
    // SAFETY: all matching PODs are fully readable/writable as documented.
    unsafe {
        layout_action(handle, error, |layout, op| {
            let viewport = viewport.read();
            if from_wire_key(viewport.key)? != layout.key {
                return Err(CoreError::StaleResult);
            }
            let viewport = Viewport {
                y: viewport.y,
                height: viewport.height,
                overscan: viewport.overscan,
            };
            let range = match &layout.state {
                LayoutState::Complete(complete) => complete.visible_blocks(viewport, op)?,
                LayoutState::Draft(draft) if layout.presenting_prefix => {
                    markdown_core::layout::visible_placements(
                        draft.prefix_placements(),
                        viewport,
                        op,
                    )?
                }
                _ => return Err(CoreError::InvalidState),
            };
            op.external_write(size_of::<PreviewRange>() as u64)?;
            op.boundary_work().checkpoint_now()?;
            output.write(PreviewRange {
                header: prefix::<PreviewRange>(),
                start: range.start as u64,
                count: range.len() as u64,
            });
            Ok(())
        })
    }
}

/// # Safety
/// Exclusive layout and independent initialized preview summary/error prefixes.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn notepads_markdown_preview_present(
    handle: *mut Owned<PreviewLayout>,
    start: u64,
    count: u32,
    output: *mut PreviewSummary,
    error: *mut WireError,
) -> i32 {
    preflight!(valid_handle(handle));
    if let Err(code) = unsafe { preview_header(output) } {
        return code;
    }
    // SAFETY: live exclusive layout and fully writable accepted summary POD.
    unsafe {
        layout_action(handle, error, |layout, op| {
            if count > 256 {
                return Err(CoreError::InvalidRange);
            }
            let start = usize::try_from(start).map_err(|_| CoreError::ArithmeticOverflow)?;
            let end = start
                .checked_add(count as usize)
                .ok_or(CoreError::ArithmeticOverflow)?;
            // No borrowed pointers escape read(). Retire the old projection before
            // allocating its replacement, including on a terminal build failure.
            layout.batch = None;
            let requests = match &layout.state {
                LayoutState::Complete(complete) => complete.requests(),
                LayoutState::Draft(draft) if layout.presenting_prefix => draft.prefix_requests(),
                _ => return Err(CoreError::InvalidState),
            };
            let requests = requests.get(start..end).ok_or(CoreError::InvalidRange)?;
            let batch = MeasurementBatch {
                key: layout.key,
                start,
                requests,
                scope: layout.scope,
            };
            let next = if count == 0 {
                None
            } else {
                Some(PresentationBatch::build(layout.model.clone(), &batch, op)?)
            };
            layout.batch = next;
            let summary = layout.summary(start);
            op.external_write(size_of::<PreviewSummary>() as u64)?;
            op.boundary_work().checkpoint_now()?;
            output.write(summary);
            Ok(())
        })
    }
}

/// # Safety
/// Exclusive layout, writable distinct allocation-handle output/error storage.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn notepads_markdown_preview_reserve(
    handle: *mut Owned<PreviewLayout>,
    bytes: u64,
    output: *mut *mut Owned<PreviewAllocation>,
    error: *mut WireError,
) -> i32 {
    handle_output!(output);
    preflight!(valid_handle(handle));
    // SAFETY: live exclusive handle and writable output validated before action.
    unsafe {
        layout_action(handle, error, |_, op| {
            usize::try_from(bytes).map_err(|_| CoreError::ArithmeticOverflow)?;
            let slot = Slot::new(op.boundary_work())?;
            let reservation = op.reserve_boundary(bytes)?;
            op.boundary_work().checkpoint_now()?;
            output.write(slot.commit(PreviewAllocation {
                reservation,
                admitted: false,
                svg_attribution: 0,
            }));
            Ok(())
        })
    }
}

/// # Safety
/// Exclusive layout/allocation handles sharing a budget root; independent error storage.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn notepads_markdown_preview_admit(
    handle: *mut Owned<PreviewLayout>,
    allocation: *mut Owned<PreviewAllocation>,
    bytes: u64,
    error: *mut WireError,
) -> i32 {
    preflight!(valid_handle(handle) && valid_handle(allocation));
    // SAFETY: caller keeps the independently owned reservation live and exclusive.
    unsafe {
        layout_action(handle, error, |_, op| {
            usize::try_from(bytes).map_err(|_| CoreError::ArithmeticOverflow)?;
            (*allocation).value.reservation.admit_actual(bytes, op)?;
            (*allocation).value.admitted = true;
            Ok(())
        })
    }
}

/// # Safety
/// Shared live model/cancel, independent exclusive output/error; caller keeps native
/// bytes alive until their separately owned reservation is destroyed after free.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn notepads_markdown_preview_model_reserve(
    model: *const Owned<CoreModel>,
    cancel: *const Owned<CoreCancel>,
    bytes: u64,
    output: *mut *mut Owned<PreviewAllocation>,
    error: *mut WireError,
) -> i32 {
    handle_output!(output);
    preflight!(valid_handle(model) && valid_handle(cancel));
    // SAFETY: shared model/cancel stay live; action validates writable error prefix.
    unsafe {
        action(error, || {
            let model = &(*model).value.model;
            let cancel = &(*cancel).value;
            if !cancel.budget.same_root(model.budget()) {
                return Err(CoreError::WrongBudget);
            }
            usize::try_from(bytes).map_err(|_| CoreError::ArithmeticOverflow)?;
            let mut op = cancel.budget.begin(cancel.token.clone());
            op.boundary_work().checkpoint_now()?;
            let slot = Slot::new(op.boundary_work())?;
            let reservation = op.reserve_boundary(bytes)?;
            op.boundary_work().checkpoint_now()?;
            output.write(slot.commit(PreviewAllocation {
                reservation,
                admitted: false,
                svg_attribution: 0,
            }));
            Ok(())
        })
    }
}

/// # Safety
/// Live shared model/cancel and exclusively borrowed allocation; no native bytes
/// are accessed here. Destroy the reservation only after the funded bytes are freed.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn notepads_markdown_preview_model_admit(
    model: *const Owned<CoreModel>,
    cancel: *const Owned<CoreCancel>,
    allocation: *mut Owned<PreviewAllocation>,
    capacity_bytes: u64,
    error: *mut WireError,
) -> i32 {
    preflight!(valid_handle(model) && valid_handle(cancel) && valid_handle(allocation));
    // SAFETY: immutable model/cancel and sole allocation borrow remain live.
    unsafe {
        action(error, || {
            let model = &(*model).value.model;
            let cancel = &(*cancel).value;
            if !cancel.budget.same_root(model.budget()) {
                return Err(CoreError::WrongBudget);
            }
            usize::try_from(capacity_bytes).map_err(|_| CoreError::ArithmeticOverflow)?;
            let mut op = cancel.budget.begin(cancel.token.clone());
            op.boundary_work().checkpoint_now()?;
            (*allocation)
                .value
                .reservation
                .admit_actual(capacity_bytes, &mut op)?;
            (*allocation).value.admitted = true;
            Ok(())
        })
    }
}

destroy!(notepads_markdown_preview_layout_destroy, PreviewLayout);
destroy!(
    notepads_markdown_preview_allocation_destroy,
    PreviewAllocation
);

#[cfg(test)]
mod tests;
