// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE file in the project root for license information.
// ---------------------------------------------------------------------------------------------

//! Rendering projection for independently exposed authored metadata.

use crate::{
    DiagramError,
    native_common::{check, scope},
};
pub use markdown_core::diagram_accessibility::MAX_DIAGRAM_SOURCE_BYTES;
use markdown_core::diagram_accessibility::inspect;
use markdown_memory::{Family, FundedString};
use merman_core::OperationControl;

/// Suppress admitted statements without moving any authored byte coordinates.
/// Allocate a single funded projection only when there is metadata to mask.
pub fn mask_metadata(
    source: &str,
    control: &OperationControl,
) -> Result<Option<FundedString>, DiagramError> {
    let metadata = inspect(source, |bytes| check(control, bytes))?;
    if metadata.is_limited() {
        return Err(DiagramError::WorkLimited);
    }
    let mut statements = metadata.statements().peekable();
    if statements.peek().is_none() {
        return Ok(None);
    }

    let mut result =
        FundedString::try_with_capacity(&scope(control)?, Family::Parse, source.len())?;
    let mut at = 0;
    for statement in statements {
        check(control, statement.end - at)?;
        result.try_push_str(&source[at..statement.start])?;
        for byte in source[statement.clone()].bytes() {
            result.try_push(if matches!(byte, b'\n' | b'\r') {
                char::from(byte)
            } else {
                ' '
            })?;
        }
        at = statement.end;
    }
    check(control, source.len() - at)?;
    result.try_push_str(&source[at..])?;
    Ok(Some(result))
}
