// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE file in the project root for license information.
// ---------------------------------------------------------------------------------------------

//! Immutable outline ABI1. One exclusive handle preserves its original operation.

use super::*;
use markdown_core::outline::Outline;
use std::ffi::c_void;

pub const OUTLINE_ABI_VERSION: u32 = 1;

#[repr(C, align(8))]
#[derive(Clone, Copy, Debug, Default)]
pub struct OutlineBlob {
    pub start: u64,
    pub length: u64,
}

#[repr(C, align(8))]
#[derive(Clone, Copy, Debug, Default)]
pub struct OutlineHeading {
    pub node: u32,
    pub level: u32,
    pub logical_start: u64,
    pub logical_end: u64,
    pub title: OutlineBlob,
    pub anchor: OutlineBlob,
}

#[repr(C, align(8))]
#[derive(Clone, Copy, Debug, Default)]
pub struct OutlineAnchor {
    pub position: u64,
    pub legacy: u32,
    pub node: u32,
    pub name: OutlineBlob,
}

#[repr(C, align(8))]
#[derive(Clone, Copy, Debug, Default)]
pub struct OutlineSummary {
    pub header: WireHeader,
    pub model: WireIdentity,
    pub headings: u64,
    pub blob_bytes: u64,
    pub anchors: u64,
}

pub struct CoreOutline {
    // Payload/model retire before the original operation and its root token.
    outline: Outline,
    op: Operation,
    poisoned: bool,
}

const _: () = {
    assert!(size_of::<OutlineBlob>() == 16 && align_of::<OutlineBlob>() == 8);
    assert!(size_of::<OutlineHeading>() == 56 && align_of::<OutlineHeading>() == 8);
    assert!(size_of::<OutlineAnchor>() == 32 && align_of::<OutlineAnchor>() == 8);
    assert!(size_of::<OutlineSummary>() == 72 && align_of::<OutlineSummary>() == 8);
};

/// # Safety
/// A readable initialized prefix, aligned for the complete summary.
unsafe fn summary_header(pointer: *const OutlineSummary) -> Result<(), i32> {
    if pointer.is_null() || !(pointer as usize).is_multiple_of(align_of::<OutlineSummary>()) {
        return Err(STATUS_INVALID_ARGUMENT);
    }
    // SAFETY: only the readable prefix is touched until version and size match.
    let prefix = unsafe { pointer.cast::<WireHeader>().read() };
    if prefix.abi_version != OUTLINE_ABI_VERSION
        || prefix.struct_size as usize != size_of::<OutlineSummary>()
    {
        return Err(STATUS_ABI_MISMATCH);
    }
    Ok(())
}

/// # Safety
/// Exclusive live handle, initialized independent error prefix and owned outputs.
unsafe fn outline_action(
    handle: *mut Owned<CoreOutline>,
    error: *mut WireError,
    work: impl FnOnce(&mut CoreOutline) -> Result<(), CoreError>,
) -> i32 {
    // SAFETY: handle remains live/exclusive and action checks the error prefix.
    let code = unsafe {
        action(error, || {
            let state = &mut (*handle).value;
            if state.poisoned {
                return Err(CoreError::InvalidState);
            }
            state.op.boundary_work().checkpoint_now()?;
            let result = work(state);
            match result {
                Ok(()) => state.op.boundary_work().checkpoint_now(),
                Err(fault) => Err(state.op.boundary_work().boundary_failure(fault)),
            }
        })
    };
    if code == STATUS_PANIC {
        // SAFETY: the same exclusive handle is still live after containment.
        unsafe { (*handle).value.poisoned = true };
    }
    code
}

#[unsafe(no_mangle)]
pub extern "C" fn notepads_markdown_outline_abi_version() -> u32 {
    OUTLINE_ABI_VERSION
}

/// # Safety
/// Live shared model/cancel, independent writable output and initialized error.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn notepads_markdown_outline_create(
    model: *const Owned<CoreModel>,
    cancel: *const Owned<CoreCancel>,
    output: *mut *mut Owned<CoreOutline>,
    error: *mut WireError,
) -> i32 {
    handle_output!(output);
    preflight!(valid_handle(model) && valid_handle(cancel));
    // SAFETY: shared input handles and distinct output storage live throughout.
    unsafe {
        action(error, || {
            let model = (*model).value.model.clone();
            let cancel = &(*cancel).value;
            if !cancel.budget.same_root(model.budget()) {
                return Err(CoreError::WrongBudget);
            }
            let mut op = cancel.budget.begin(cancel.token.clone());
            let slot = Slot::new(op.boundary_work())?;
            let outline = Outline::build(model, &mut op)?;
            op.boundary_work()
                .external_write(size_of::<usize>() as u64)?;
            op.boundary_work().checkpoint_now()?;
            output.write(slot.commit(CoreOutline {
                outline,
                op,
                poisoned: false,
            }));
            Ok(())
        })
    }
}

/// # Safety
/// Exclusive live handle and writable nonoverlapping summary/error prefixes.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn notepads_markdown_outline_get_summary(
    handle: *mut Owned<CoreOutline>,
    output: *mut OutlineSummary,
    error: *mut WireError,
) -> i32 {
    preflight!(valid_handle(handle));
    // SAFETY: caller supplies a readable summary prefix.
    if let Err(code) = unsafe { summary_header(output) } {
        return code;
    }
    // SAFETY: matching prefixes imply full writable independent POD storage.
    unsafe {
        outline_action(handle, error, |state| {
            state
                .op
                .boundary_work()
                .external_read(size_of::<OutlineSummary>() as u64)?;
            state
                .op
                .boundary_work()
                .external_write(size_of::<OutlineSummary>() as u64)?;
            let result = OutlineSummary {
                header: WireHeader {
                    abi_version: OUTLINE_ABI_VERSION,
                    struct_size: size_of::<OutlineSummary>() as u32,
                },
                model: wire_identity(state.outline.model().identity()),
                headings: state.outline.headings().len() as u64,
                blob_bytes: state.outline.blob().len() as u64,
                anchors: state.outline.anchors().len() as u64,
            };
            output.write(result);
            Ok(())
        })
    }
}

/// # Safety
/// Exclusive live handle, exact aligned finite writable buffer, separate PODs.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn notepads_markdown_outline_read(
    handle: *mut Owned<CoreOutline>,
    table: u32,
    start: u64,
    count: u64,
    row_size: u32,
    buffer: *mut c_void,
    output: *mut WireReadResult,
    error: *mut WireError,
) -> i32 {
    preflight!(valid_handle(handle) && (count == 0 || !buffer.is_null()));
    // SAFETY: caller supplies an initialized independent core read-result prefix.
    if let Err(code) = unsafe { header(output) } {
        return code;
    }
    // SAFETY: all reads/writes stay within caller-guaranteed independent storage.
    unsafe {
        outline_action(handle, error, |state| {
            let (size, alignment, total, maximum) = match table {
                1 => (
                    size_of::<OutlineHeading>(),
                    align_of::<OutlineHeading>(),
                    state.outline.headings().len(),
                    256,
                ),
                2 => (1, 1, state.outline.blob().len(), 65_536),
                3 => (
                    size_of::<OutlineAnchor>(),
                    align_of::<OutlineAnchor>(),
                    state.outline.anchors().len(),
                    256,
                ),
                _ => return Err(CoreError::UnknownId),
            };
            let start = usize::try_from(start).map_err(|_| CoreError::ArithmeticOverflow)?;
            let count = usize::try_from(count).map_err(|_| CoreError::ArithmeticOverflow)?;
            let end = start
                .checked_add(count)
                .ok_or(CoreError::ArithmeticOverflow)?;
            if row_size as usize != size || count > maximum || end > total {
                return Err(CoreError::InvalidRange);
            }
            if !(buffer as usize).is_multiple_of(alignment) {
                return Err(CoreError::InvalidBoundary);
            }
            let bytes = count
                .checked_mul(size)
                .ok_or(CoreError::ArithmeticOverflow)? as u64;
            state.op.boundary_work().external_read(bytes)?;
            state.op.boundary_work().external_write(bytes)?;
            if table == 1 {
                for index in 0..count {
                    state.op.boundary_work().external_events(1)?;
                    let row = state.outline.headings()[start + index];
                    let result = OutlineHeading {
                        node: row.node.value(),
                        level: row.level,
                        logical_start: row.logical.start(),
                        logical_end: row.logical.end(),
                        title: OutlineBlob {
                            start: row.title.start as u64,
                            length: row.title.len as u64,
                        },
                        anchor: OutlineBlob {
                            start: row.anchor.start as u64,
                            length: row.anchor.len as u64,
                        },
                    };
                    buffer.cast::<OutlineHeading>().add(index).write(result);
                }
            } else if table == 3 {
                for index in 0..count {
                    state.op.boundary_work().external_events(1)?;
                    let row = state.outline.anchors()[start + index];
                    buffer
                        .cast::<OutlineAnchor>()
                        .add(index)
                        .write(OutlineAnchor {
                            position: row.position,
                            legacy: u32::from(row.legacy),
                            node: row.node.map_or(0, |node| node.value()),
                            name: OutlineBlob {
                                start: row.name.start as u64,
                                length: row.name.len as u64,
                            },
                        });
                }
            } else if count != 0 {
                ptr::copy_nonoverlapping(
                    state.outline.blob().as_ptr().add(start),
                    buffer.cast(),
                    count,
                );
            }
            state
                .op
                .boundary_work()
                .external_write(size_of::<WireReadResult>() as u64)?;
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
/// Unique live handle, or null; never concurrent with any other directory call.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn notepads_markdown_outline_destroy(handle: *mut Owned<CoreOutline>) -> i32 {
    if handle.is_null() {
        return 0;
    }
    preflight!(valid_handle(handle));
    contain_panic(|| {
        // SAFETY: caller transfers the unique live owned handle exactly once.
        unsafe { release(handle) };
        0
    })
}

#[cfg(test)]
mod tests;
