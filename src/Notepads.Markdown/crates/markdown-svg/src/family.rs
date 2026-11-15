// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE file in the project root for license information.
// ---------------------------------------------------------------------------------------------
//! Same-cascade typed family lists encoded into the existing private CSS field.
//! Source CSS decoding belongs to svgtypes. This module never reparses a name.
use crate::abi::{Failure, Result};
use crate::memory::{Context, failure_from_resource};
use markdown_svg_budget::BudgetVec;

const MAX_POOL_BYTES: usize = 1048576;
const HEX: &[u8; 16] = b"0123456789ABCDEF";

fn visit(
    callbacks: &Context,
    families: &[usvg::FontFamily],
    mut emit: impl FnMut(&[u8]) -> Result<()>,
) -> Result<()> {
    if families.is_empty() {
        return Err(callbacks.record_failure(Failure::Contract));
    }
    for (index, family) in families.iter().enumerate() {
        if index % 256 == 0 {
            callbacks.checkpoint((families.len() - index).min(256) as u64)?;
        }
        if index != 0 {
            emit(b", ")?;
        }
        let generic: &[u8] = match family {
            usvg::FontFamily::Serif => b"serif",
            usvg::FontFamily::SansSerif => b"sans-serif",
            usvg::FontFamily::Cursive => b"cursive",
            usvg::FontFamily::Fantasy => b"fantasy",
            usvg::FontFamily::Monospace => b"monospace",
            usvg::FontFamily::Named(name) => {
                if name.is_empty() || name.len() > 4096 {
                    return Err(callbacks.record_failure(Failure::Contract));
                }
                emit(b"\"")?;
                for chunk in name.as_bytes().chunks(4096) {
                    callbacks.checkpoint(chunk.len() as u64)?;
                    let mut start = 0;
                    for (at, &byte) in chunk.iter().enumerate() {
                        if byte == 0 {
                            return Err(callbacks.record_failure(Failure::Contract));
                        }
                        if byte == b'"' || byte == b'\\' || byte < 0x20 || byte == 0x7f {
                            emit(&chunk[start..at])?;
                            if byte == b'"' {
                                emit(b"\\\"")?;
                            } else if byte == b'\\' {
                                emit(b"\\\\")?;
                            } else {
                                let mut escaped = [b'0'; 8];
                                escaped[0] = b'\\';
                                escaped[5] = HEX[usize::from(byte >> 4)];
                                escaped[6] = HEX[usize::from(byte & 15)];
                                escaped[7] = b' ';
                                emit(&escaped)?;
                            }
                            start = at + 1;
                        }
                    }
                    emit(&chunk[start..])?;
                }
                emit(b"\"")?;
                continue;
            }
        };
        emit(generic)?;
    }
    Ok(())
}

/// Exact byte capacity, including quoting/escaping and list separators.
pub(crate) fn encoded_len(callbacks: &Context, families: &[usvg::FontFamily]) -> Result<usize> {
    let mut length = 0usize;
    visit(callbacks, families, |bytes| {
        length = length
            .checked_add(bytes.len())
            .filter(|&value| value <= MAX_POOL_BYTES)
            .ok_or_else(|| callbacks.record_failure(Failure::Contract))?;
        Ok(())
    })?;
    Ok(length)
}

/// Write directly into the draft's admitted pool; no temporary String or Vec.
pub(crate) fn append(
    callbacks: &Context,
    families: &[usvg::FontFamily],
    strings: &mut BudgetVec<u8>,
) -> Result<()> {
    visit(callbacks, families, |bytes| {
        if strings
            .len()
            .checked_add(bytes.len())
            .is_none_or(|value| value > MAX_POOL_BYTES)
        {
            return Err(callbacks.record_failure(Failure::Contract));
        }
        strings
            .try_extend_from_slice(bytes)
            .map_err(failure_from_resource)
    })
}
