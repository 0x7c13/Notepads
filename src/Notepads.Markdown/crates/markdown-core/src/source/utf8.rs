// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE file in the project root for license information.
// ---------------------------------------------------------------------------------------------

use crate::error::CoreError;
use crate::memory::ChargedString;
use crate::policy::{CHECKPOINT_BYTES, Operation, OwnedWork};

/// Validate bounded blocks, retaining an incomplete suffix by restarting at its first byte.
/// At most three bytes are scanned again; valid-prefix revalidation is also charged as scanning.
pub(super) fn visit(
    bytes: &[u8],
    edit_index: Option<u64>,
    op: &mut Operation,
    mut consume: impl FnMut(&str, &mut Operation) -> Result<(), CoreError>,
) -> Result<(), CoreError> {
    op.checkpoint()?;
    let mut cursor = 0;
    while cursor < bytes.len() {
        let end = cursor + (bytes.len() - cursor).min(CHECKPOINT_BYTES as usize);
        let chunk = &bytes[cursor..end];
        op.scan(chunk.len() as u64)?;
        match std::str::from_utf8(chunk) {
            Ok(text) => {
                consume(text, op)?;
                cursor = end;
            }
            Err(error) if error.error_len().is_none() && end < bytes.len() => {
                let valid = error.valid_up_to();
                if valid == 0 || chunk.len() - valid > 3 {
                    return Err(op.fail(CoreError::InternalInvariant));
                }
                op.scan(valid as u64)?;
                let text = std::str::from_utf8(&chunk[..valid])
                    .map_err(|_| op.fail(CoreError::InternalInvariant))?;
                consume(text, op)?;
                cursor += valid;
            }
            Err(error) => {
                let offset = u64::try_from(cursor + error.valid_up_to())
                    .map_err(|_| op.fail(CoreError::ArithmeticOverflow))?;
                return Err(op.fail(match edit_index {
                    Some(edit_index) => CoreError::InvalidEditUtf8 { edit_index, offset },
                    None => CoreError::InvalidUtf8 { offset },
                }));
            }
        }
    }
    op.checkpoint()
}

/// Choose a scalar-aligned block without exceeding the byte checkpoint bound.
/// Boundary probes read at most three continuation bytes and are counted separately.
pub(super) fn end(text: &str, cursor: usize, op: &mut Operation) -> Result<usize, CoreError> {
    end_with(text, cursor, |bytes| op.scan(bytes))
}

pub(super) fn end_owned(
    text: &str,
    cursor: usize,
    work: &mut OwnedWork<'_, '_>,
) -> Result<usize, CoreError> {
    end_with(text, cursor, |bytes| work.scan(bytes))
}

fn end_with(
    text: &str,
    cursor: usize,
    mut scan: impl FnMut(u64) -> Result<(), CoreError>,
) -> Result<usize, CoreError> {
    let mut end = cursor + (text.len() - cursor).min(CHECKPOINT_BYTES as usize);
    while end < text.len() {
        scan(1)?;
        if text.is_char_boundary(end) {
            break;
        }
        end -= 1;
    }
    Ok(end)
}

pub(super) fn copy(
    text: &str,
    output: &mut ChargedString,
    op: &mut Operation,
) -> Result<(), CoreError> {
    let mut cursor = 0;
    while cursor < text.len() {
        let end = end(text, cursor, op)?;
        output.append(&text[cursor..end], op)?;
        cursor = end;
    }
    op.checkpoint()
}
