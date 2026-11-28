// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE file in the project root for license information.
// ---------------------------------------------------------------------------------------------

//! Strict, bounded stack conversion; no replacement characters or whole-input temporary.

use crate::error::CoreError;
use crate::policy::{CHECKPOINT_EVENTS, Operation};

pub(super) fn visit(
    input: &[u16],
    op: &mut Operation,
    mut consume: impl FnMut(&str, &mut Operation) -> Result<(), CoreError>,
) -> Result<(), CoreError> {
    op.checkpoint_now()?;
    let mut high = None;
    for (block, units) in input.chunks(CHECKPOINT_EVENTS as usize).enumerate() {
        op.scan(std::mem::size_of_val(units) as u64)?;
        op.events(units.len() as u64)?;
        // Prepay the bounded stack initialization and worst-case encoded writes
        // before touching it. Like UTF8 read admission, this is a work bound,
        // not a claim that every prepaid byte was physically copied.
        op.copy(1024 + (units.len() * 3 + 4) as u64)?;
        let mut scratch = [0_u8; 1024];
        let mut written = 0;
        for (index, &unit) in units.iter().enumerate() {
            let offset = (block * CHECKPOINT_EVENTS as usize + index) as u64;
            let scalar = if let Some((first, first_offset)) = high.take() {
                if !(0xdc00..=0xdfff).contains(&unit) {
                    return Err(op.fail(CoreError::InvalidUtf16 {
                        offset: first_offset,
                    }));
                }
                0x10000 + ((u32::from(first) - 0xd800) << 10) + (u32::from(unit) - 0xdc00)
            } else if (0xd800..=0xdbff).contains(&unit) {
                high = Some((unit, offset));
                continue;
            } else if (0xdc00..=0xdfff).contains(&unit) {
                return Err(op.fail(CoreError::InvalidUtf16 { offset }));
            } else {
                u32::from(unit)
            };
            let character =
                char::from_u32(scalar).ok_or_else(|| op.fail(CoreError::InternalInvariant))?;
            written += character.encode_utf8(&mut scratch[written..]).len();
        }
        // The safe &str construction is a real second stack read and is charged.
        op.scan(written as u64)?;
        let text = std::str::from_utf8(&scratch[..written])
            .map_err(|_| op.fail(CoreError::InternalInvariant))?;
        consume(text, op)?;
    }
    if let Some((_, offset)) = high {
        return Err(op.fail(CoreError::InvalidUtf16 { offset }));
    }
    op.checkpoint_now()
}
