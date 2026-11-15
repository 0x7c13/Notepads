// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE file in the project root for license information.
// ---------------------------------------------------------------------------------------------

//! Fixed admission and display adapters for TOML and JSON front matter.

use super::value::{DisplayValue, Entry, copy_chunks, event_chunks, scan_chunks};
use super::{MAX_METADATA_BYTES, MAX_OUTPUT_BYTES, append};
use crate::{error::CoreError, policy::Operation};
use serde_json::value::RawValue;
use toml::de::{DeTable, DeValue};
use toml_parser::lexer::TokenKind;

const MAX_DEPTH: usize = 64;
const MAX_TOKENS: usize = 16_384;

/// Find one initial JSON object without admitting an arbitrary brace prefix.
/// The caller separately validates the complete object with the JSON parser.
pub(super) fn json_envelope(
    source: &str,
    op: &mut Operation,
) -> Result<Option<(usize, usize)>, CoreError> {
    let mut stack = [0; MAX_DEPTH];
    let mut depth = 0usize;
    let mut quoted = false;
    let mut escaped = false;
    let mut count = 0;
    let mut start = 0;
    for chunk in source.as_bytes()[..source.len().min(MAX_METADATA_BYTES)].chunks(4096) {
        op.checkpoint_now()?;
        op.scan(chunk.len() as u64)?;
        for (index, &byte) in chunk.iter().enumerate() {
            if byte == 0 {
                return Ok(None);
            }
            if quoted {
                if escaped {
                    escaped = false;
                } else if byte == b'\\' {
                    escaped = true;
                } else if byte == b'"' {
                    quoted = false;
                }
                continue;
            }
            match byte {
                b'"' => quoted = true,
                b'{' | b'[' => {
                    let Some(slot) = stack.get_mut(depth) else {
                        return Ok(None);
                    };
                    *slot = if byte == b'{' { b'}' } else { b']' };
                    depth += 1;
                    count += 1;
                }
                b'}' | b']' => {
                    if depth == 0 || stack[depth - 1] != byte {
                        return Ok(None);
                    }
                    depth -= 1;
                    if depth == 0 {
                        return Ok(Some((start + index + 1, count)));
                    }
                }
                b':' | b',' => count += 1,
                _ => {}
            }
            if count > MAX_TOKENS {
                return Ok(None);
            }
        }
        start += chunk.len();
    }
    Ok(None)
}

pub(super) fn json_entries(
    content: &str,
    value_bound: usize,
    render: bool,
    op: &mut Operation,
) -> Result<Option<Vec<Entry>>, CoreError> {
    if content.len() > MAX_METADATA_BYTES || value_bound > MAX_TOKENS {
        return Ok(None);
    }
    let _scratch = admit_parser(content, value_bound, op)?;
    // JSON numbers keep their original decimal spelling, including integers
    // outside the exactly representable IEEE-754 range.
    let parsed = serde_json::from_str::<indexmap::IndexMap<String, &RawValue>>(content);
    op.checkpoint_now()?;
    let Ok(values) = parsed else {
        return Ok(None);
    };
    if !render {
        return Ok(Some(Vec::new()));
    }
    Ok(Some(entries(values, json_value, op)?))
}

pub(super) fn toml_entries(
    content: &str,
    op: &mut Operation,
) -> Result<Result<Vec<Entry>, String>, CoreError> {
    let Some(tokens) = toml_token_bound(content, op)? else {
        return Ok(Err(
            "TOML front matter exceeds the preview resource limit.".to_owned()
        ));
    };
    let _scratch = admit_parser(content, tokens, op)?;
    let parsed = DeTable::parse(content);
    op.checkpoint_now()?;
    let values = match parsed {
        Ok(values) => values,
        Err(error) => {
            return Ok(Err(format!(
                "Invalid TOML front matter. {}",
                error.message()
            )));
        }
    };
    let values = values
        .into_inner()
        .into_iter()
        .map(|(key, value)| (key.into_inner().into_owned(), value.into_inner()));
    Ok(Ok(entries(values, toml_value, op)?))
}

fn toml_token_bound(content: &str, op: &mut Operation) -> Result<Option<usize>, CoreError> {
    let mut count = 0;
    let mut depth = 0usize;
    let mut nesting = 0;
    let mut dots = 0;
    let mut paths = 0;
    if content.len() > MAX_METADATA_BYTES {
        return Ok(None);
    }
    scan_chunks(op, content.len() as u64)?;
    for token in toml_parser::Source::new(content).lex() {
        op.events(1)?;
        count += 1;
        match token.kind() {
            TokenKind::Dot => {
                dots += 1;
                paths = paths.max(dots);
            }
            TokenKind::LeftSquareBracket | TokenKind::LeftCurlyBracket => {
                dots = 0;
                depth += 1;
                nesting = nesting.max(depth);
            }
            TokenKind::RightSquareBracket | TokenKind::RightCurlyBracket => {
                dots = 0;
                depth = depth.saturating_sub(1);
            }
            TokenKind::Equals | TokenKind::Comma | TokenKind::Newline => dots = 0,
            _ => {}
        }
        // A dotted table header, a dotted root key and each nested inline
        // table can each contribute a full dotted path to the stored depth.
        if count > MAX_TOKENS || (paths + 1) * (nesting + 2) > MAX_DEPTH {
            return Ok(None);
        }
    }
    Ok(Some(count))
}

fn admit_parser(
    content: &str,
    tokens: usize,
    op: &mut Operation,
) -> Result<crate::policy::BoundaryReservation, CoreError> {
    op.checkpoint_now()?;
    scan_chunks(op, content.len() as u64)?;
    event_chunks(op, tokens as u64)?;
    // Like the YAML parser, admit bounded parser buffers, map nodes, decoded
    // strings, and their short-lived display projection before parsing.
    let scratch = content.len() * 16 + tokens * 512 + MAX_OUTPUT_BYTES * 3;
    op.reserve_boundary(scratch as u64)
}

fn entries<V, I>(
    values: I,
    display: fn(V, &mut Operation) -> Result<DisplayValue, CoreError>,
    op: &mut Operation,
) -> Result<Vec<Entry>, CoreError>
where
    I: IntoIterator<Item = (String, V)>,
{
    let values = values.into_iter();
    let mut entries = Vec::new();
    entries
        .try_reserve_exact(values.size_hint().0)
        .map_err(|_| op.fail(CoreError::OutOfMemory))?;
    for (key, value) in values {
        op.events(1)?;
        copy_chunks(op, key.len() as u64)?;
        entries.push(Entry {
            key,
            value: display(value, op)?,
        });
    }
    Ok(entries)
}

fn json_value(value: &RawValue, op: &mut Operation) -> Result<DisplayValue, CoreError> {
    let raw = value.get();
    op.events(1)?;
    scan_chunks(op, raw.len() as u64)?;
    let value = match raw.as_bytes().first() {
        Some(b'"') => serde_json::from_str(raw)
            .map(DisplayValue::Text)
            .unwrap_or_else(|_| DisplayValue::Code(raw.to_owned())),
        Some(b'[') => {
            let values = serde_json::from_str::<Vec<&RawValue>>(raw)
                .map_err(|_| op.fail(CoreError::InvalidProvenance))?;
            DisplayValue::List(
                values
                    .into_iter()
                    .map(|value| json_value(value, op))
                    .collect::<Result<_, _>>()?,
            )
        }
        Some(b'{') => DisplayValue::Code(raw.to_owned()),
        _ => DisplayValue::Text(raw.to_owned()),
    };
    count_display(value, op)
}

fn toml_value(value: DeValue<'_>, op: &mut Operation) -> Result<DisplayValue, CoreError> {
    op.events(1)?;
    let value = match value {
        DeValue::String(value) => DisplayValue::Text(value.into_owned()),
        DeValue::Array(values) => DisplayValue::List(
            values
                .into_iter()
                .map(|value| toml_value(value.into_inner(), op))
                .collect::<Result<_, _>>()?,
        ),
        value @ DeValue::Table(_) => {
            let mut output = String::new();
            compact_toml(&value, &mut output, op)?;
            return Ok(DisplayValue::Code(output));
        }
        DeValue::Integer(value) => DisplayValue::Text(value.to_string()),
        DeValue::Float(value) => DisplayValue::Text(value.to_string()),
        DeValue::Boolean(value) => DisplayValue::Text(value.to_string()),
        DeValue::Datetime(value) => DisplayValue::Text(value.to_string()),
    };
    count_display(value, op)
}

fn compact_toml(
    value: &DeValue<'_>,
    output: &mut String,
    op: &mut Operation,
) -> Result<(), CoreError> {
    op.events(1)?;
    match value {
        DeValue::String(value) => append(output, &json_string(value, op)?, op),
        DeValue::Integer(value) => append(output, &value.to_string(), op),
        DeValue::Float(value) => append(output, &value.to_string(), op),
        DeValue::Boolean(value) => append(output, if *value { "true" } else { "false" }, op),
        DeValue::Datetime(value) => append(output, &value.to_string(), op),
        DeValue::Array(values) => {
            append(output, "[", op)?;
            for (index, child) in values.iter().enumerate() {
                if index != 0 {
                    append(output, ", ", op)?;
                }
                compact_toml(child.get_ref(), output, op)?;
            }
            append(output, "]", op)
        }
        DeValue::Table(values) => {
            append(output, "{ ", op)?;
            for (index, (key, child)) in values.iter().enumerate() {
                if index != 0 {
                    append(output, ", ", op)?;
                }
                append(output, &json_string(key.get_ref(), op)?, op)?;
                append(output, " = ", op)?;
                compact_toml(child.get_ref(), output, op)?;
            }
            append(output, " }", op)
        }
    }
}

fn json_string(value: &str, op: &mut Operation) -> Result<String, CoreError> {
    scan_chunks(op, value.len() as u64)?;
    serde_json::to_string(value).map_err(|_| op.fail(CoreError::InvalidProvenance))
}

fn count_display(value: DisplayValue, op: &mut Operation) -> Result<DisplayValue, CoreError> {
    match &value {
        DisplayValue::Text(value) | DisplayValue::Code(value) => {
            copy_chunks(op, value.len() as u64)?
        }
        DisplayValue::List(_) => {}
    }
    Ok(value)
}
