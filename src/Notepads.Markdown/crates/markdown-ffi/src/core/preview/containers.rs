// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE file in the project root for license information.
// ---------------------------------------------------------------------------------------------

//! Additive quote geometry: complete document coordinates, independent of the current batch.

use super::*;

#[repr(C, align(8))]
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct PreviewBlockGeometry {
    pub node: u32,
    pub reserved: u32,
    pub marker_x: f64,
}

#[repr(C, align(8))]
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct PreviewQuote {
    pub node: u32,
    pub depth: u32,
    pub callout: u32,
    pub reserved: u32,
    pub x: f64,
    pub width: f64,
    pub y: f64,
    pub height: f64,
    pub border_width: f64,
}

#[repr(C, align(8))]
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct PreviewQuoteSummary {
    pub header: WireHeader,
    pub key: PreviewKey,
    pub rows: u64,
}

fn quote_count(layout: &PreviewLayout, op: &mut Operation) -> Result<usize, CoreError> {
    match &layout.state {
        LayoutState::Complete(complete) => Ok(complete.quotes().len()),
        LayoutState::Draft(draft) if layout.presenting_prefix => draft.prefix_quote_count(op),
        _ => Err(CoreError::InvalidState),
    }
}

const _: () = {
    assert!(size_of::<PreviewBlockGeometry>() == 16);
    assert!(size_of::<PreviewQuote>() == 56);
    assert!(size_of::<PreviewQuoteSummary>() == 112);
};

/// # Safety
/// Exclusive live layout; independent initialized summary prefix and writable error POD.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn notepads_markdown_preview_quote_summary(
    handle: *mut Owned<PreviewLayout>,
    output: *mut PreviewQuoteSummary,
    error: *mut WireError,
) -> i32 {
    preflight!(
        valid_handle(handle)
            && !output.is_null()
            && (output as usize).is_multiple_of(align_of::<PreviewQuoteSummary>())
    );
    // SAFETY: the caller supplied a readable common header.
    if let Err(code) = unsafe { header(output) } {
        return code;
    }
    // SAFETY: accepted prefix, exclusive layout and independent valid writable PODs.
    unsafe {
        layout_action(handle, error, |layout, op| {
            let count = quote_count(layout, op)?;
            op.external_write(size_of::<PreviewQuoteSummary>() as u64)?;
            op.boundary_work().checkpoint_now()?;
            output.write(PreviewQuoteSummary {
                header: WireHeader::of::<PreviewQuoteSummary>(),
                key: to_wire_key(layout.key),
                rows: count as u64,
            });
            Ok(())
        })
    }
}

/// # Safety
/// Exclusive presenting layout, at most 256 writable aligned rows, independent Core result/error.
/// No pointers may overlap. Discard all buffer bytes if the call fails.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn notepads_markdown_preview_quote_read(
    handle: *mut Owned<PreviewLayout>,
    start: u64,
    count: u64,
    row_size: u32,
    buffer: *mut PreviewQuote,
    output: *mut WireReadResult,
    error: *mut WireError,
) -> i32 {
    preflight!(
        valid_handle(handle)
            && !buffer.is_null()
            && count <= 256
            && row_size as usize == size_of::<PreviewQuote>()
            && (buffer as usize).is_multiple_of(align_of::<PreviewQuote>())
    );
    // SAFETY: caller supplies an initialized Core result prefix.
    if let Err(code) = unsafe { header(output) } {
        return code;
    }
    // SAFETY: exclusive live layout, checked independent aligned storage and prefixes.
    unsafe {
        layout_action(handle, error, |layout, op| {
            let total = quote_count(layout, op)?;
            let start = usize::try_from(start).map_err(|_| CoreError::ArithmeticOverflow)?;
            let count = count as usize;
            let end = start
                .checked_add(count)
                .ok_or(CoreError::ArithmeticOverflow)?;
            if end > total {
                return Err(CoreError::InvalidRange);
            }
            if let LayoutState::Draft(draft) = &layout.state {
                draft.charge_prefix_quotes(count, op)?;
            }
            write_rows(buffer.cast(), count, op, |index| {
                let index = start + index;
                let row = match &layout.state {
                    LayoutState::Complete(complete) => complete.quotes().get(index).copied(),
                    LayoutState::Draft(draft) => draft.prefix_quote(index),
                    LayoutState::Empty => None,
                }
                .ok_or(CoreError::InternalInvariant)?;
                Ok(PreviewQuote {
                    node: row.node.value(),
                    depth: row.depth,
                    callout: row.callout.map_or(0, |kind| kind as u32),
                    reserved: 0,
                    x: row.x,
                    width: row.width,
                    y: row.y,
                    height: row.height,
                    border_width: row.border_width,
                })
            })?;
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
