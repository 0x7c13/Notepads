// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE file in the project root for license information.
// ---------------------------------------------------------------------------------------------

//! Derived-object descriptors share the current immutable preview batch blob.
//! This additive ABI never changes the original Preview ABI2 row layouts.

use super::*;
use markdown_core::ir::{DerivedKind, MathSyntax};

pub const DERIVED_ABI_VERSION: u32 = 1;

#[repr(C, align(8))]
#[derive(Clone, Copy, Debug, Default)]
pub struct DerivedSummary {
    pub header: WireHeader,
    pub key: PreviewKey,
    pub start: u64,
    pub rows: u64,
    pub blob_bytes: u64,
}

#[repr(C, align(8))]
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct DerivedRow {
    pub object: u32,
    pub node: u32,
    pub text: u32,
    pub offset_utf16: u32,
    pub kind: u32,
    pub syntax: u32,
    pub input: PreviewBlob,
    pub export: PreviewBlob,
    pub accessible_title: PreviewBlob,
    pub accessible_description: PreviewBlob,
}

const _: () = {
    assert!(size_of::<DerivedSummary>() == 128);
    assert!(size_of::<DerivedRow>() == 88);
    assert!(std::mem::offset_of!(DerivedRow, input) == 24);
};

#[unsafe(no_mangle)]
pub extern "C" fn notepads_markdown_derived_abi_version() -> u32 {
    DERIVED_ABI_VERSION
}

/// # Safety
/// Exclusive live preview layout, independent valid writable summary/error PODs.
/// Only the initialized eight-byte summary prefix is read before size/version acceptance.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn notepads_markdown_derived_summary(
    handle: *mut Owned<PreviewLayout>,
    output: *mut DerivedSummary,
    error: *mut WireError,
) -> i32 {
    preflight!(
        valid_handle(handle)
            && !output.is_null()
            && (output as usize).is_multiple_of(align_of::<DerivedSummary>())
    );
    // SAFETY: the caller supplies the initialized readable common prefix.
    let supplied = unsafe { output.cast::<WireHeader>().read() };
    if supplied.abi_version != DERIVED_ABI_VERSION
        || supplied.struct_size != size_of::<DerivedSummary>() as u32
    {
        return STATUS_ABI_MISMATCH;
    }
    // SAFETY: the accepted prefix guarantees full nonoverlapping writable storage.
    unsafe {
        layout_action(handle, error, |layout, op| {
            let batch = layout.batch.as_ref().ok_or(CoreError::InvalidState)?;
            op.external_write(size_of::<DerivedSummary>() as u64)?;
            op.boundary_work().checkpoint_now()?;
            output.write(DerivedSummary {
                header: WireHeader {
                    abi_version: DERIVED_ABI_VERSION,
                    struct_size: size_of::<DerivedSummary>() as u32,
                },
                key: to_wire_key(batch.key()),
                start: batch.start() as u64,
                rows: batch.derived().len() as u64,
                blob_bytes: batch.blob().len() as u64,
            });
            Ok(())
        })
    }
}

/// # Safety
/// Same exclusive batch as summary; no intervening measure/present/submit calls.
/// Buffer has count aligned rows and does not overlap layout/result/error storage.
/// On failure discard the entire buffer batch; the Core read-result success POD is unchanged.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn notepads_markdown_derived_read(
    handle: *mut Owned<PreviewLayout>,
    start: u64,
    count: u64,
    row_size: u32,
    buffer: *mut DerivedRow,
    output: *mut WireReadResult,
    error: *mut WireError,
) -> i32 {
    preflight!(
        valid_handle(handle)
            && !buffer.is_null()
            && count <= 256
            && row_size as usize == size_of::<DerivedRow>()
            && (buffer as usize).is_multiple_of(align_of::<DerivedRow>())
    );
    // SAFETY: caller supplies an initialized Core ABI3 result prefix.
    if let Err(code) = unsafe { header(output) } {
        return code;
    }
    // SAFETY: all pointer storage satisfies the documented exclusive ABI borrow.
    unsafe {
        layout_action(handle, error, |layout, op| {
            let batch = layout.batch.as_ref().ok_or(CoreError::InvalidState)?;
            let start = usize::try_from(start).map_err(|_| CoreError::ArithmeticOverflow)?;
            let count = count as usize;
            let end = start
                .checked_add(count)
                .ok_or(CoreError::ArithmeticOverflow)?;
            if end > batch.derived().len() {
                return Err(CoreError::InvalidRange);
            }
            write_rows(buffer.cast(), count, op, |index| {
                let row = batch.derived()[start + index];
                let (kind, syntax) = match row.kind {
                    DerivedKind::Math(syntax) => (
                        1,
                        match syntax {
                            MathSyntax::DollarInline => 1,
                            MathSyntax::DollarDisplay => 2,
                            MathSyntax::LatexInline => 3,
                            MathSyntax::LatexDisplay => 4,
                            MathSyntax::CodeInline => 5,
                            MathSyntax::FencedDisplay => 6,
                        },
                    ),
                    DerivedKind::Diagram => (2, 0),
                    DerivedKind::NoteEmbed => (3, 0),
                };
                Ok(DerivedRow {
                    object: row.object.value(),
                    node: row.node.value(),
                    text: row.text,
                    offset_utf16: row.offset_utf16,
                    kind,
                    syntax,
                    input: blob(row.input),
                    export: blob(row.export),
                    accessible_title: blob(row.accessible_title),
                    accessible_description: blob(row.accessible_description),
                })
            })?;
            op.external_write(size_of::<WireReadResult>() as u64)?;
            op.boundary_work().checkpoint_now()?;
            output.write(WireReadResult {
                header: WireHeader::of::<WireReadResult>(),
                written: count as u64,
                next: end as u64,
                done: u32::from(end == batch.derived().len()),
                reserved: 0,
            });
            Ok(())
        })
    }
}
