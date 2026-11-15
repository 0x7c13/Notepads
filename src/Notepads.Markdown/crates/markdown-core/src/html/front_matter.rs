// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE file in the project root for license information.
// ---------------------------------------------------------------------------------------------

//! YAML, TOML and JSON document metadata with one source-backed projection.

use super::citations::Reference;
use super::fragment::{Fragment, Part};
use crate::config::{Extensions, ParseConfig, Profile};
use crate::error::CoreError;
use crate::policy::{Lease, Operation};
use crate::presentation::RUN_CODE;
use crate::source::{SourceDocument, SourceSnapshot};
use std::ops::Range;

mod structured;
#[cfg(test)]
mod tests;
mod value;

const MAX_METADATA_BYTES: usize = 256 * 1024;
const MAX_OUTPUT_BYTES: usize = 1024 * 1024;

pub(crate) struct Prepared {
    pub(super) span: Range<usize>,
    pub(super) fragment: Fragment,
    pub(super) table: bool,
    pub(super) references: Vec<Reference>,
    pub(super) storage: Lease,
}

impl Prepared {
    pub(crate) fn has_citations(&self) -> bool {
        !self.references.is_empty()
    }
}

// ECMAScript trimEnd whitespace, used by the pinned VS Code marker rule.
fn marker_space(c: char) -> bool {
    matches!(
        c,
        '\t' | '\n' | '\u{000b}' | '\u{000c}' | '\r' | ' ' | '\u{00a0}' | '\u{1680}' | '\u{2000}'
            ..='\u{200a}'
                | '\u{2028}'
                | '\u{2029}'
                | '\u{202f}'
                | '\u{205f}'
                | '\u{3000}'
                | '\u{feff}'
    )
}

fn line(source: &str, start: usize, op: &mut Operation) -> Result<(usize, usize), CoreError> {
    let bytes = source.as_bytes();
    let mut end = start;
    while end < bytes.len() {
        let boundary = (end + 16 * 1024).min(bytes.len());
        if let Some(offset) = bytes[end..boundary]
            .iter()
            .position(|byte| matches!(*byte, b'\r' | b'\n'))
        {
            op.scan((offset + 1) as u64)?;
            end += offset;
            break;
        }
        op.scan((boundary - end) as u64)?;
        end = boundary;
    }
    let next = end
        + if bytes.get(end) == Some(&b'\r') && bytes.get(end + 1) == Some(&b'\n') {
            2
        } else {
            usize::from(end < bytes.len())
        };
    Ok((end, next))
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Format {
    Yaml,
    Toml,
    Json,
}

impl Format {
    fn name(self) -> &'static str {
        match self {
            Self::Yaml => "YAML",
            Self::Toml => "TOML",
            Self::Json => "JSON",
        }
    }

    fn language(self) -> &'static str {
        match self {
            Self::Yaml => "yaml",
            Self::Toml => "toml",
            Self::Json => "json",
        }
    }
}

struct Envelope {
    span: Range<usize>,
    content: Range<usize>,
    format: Format,
    json_values: usize,
}

fn envelope(source: &str, op: &mut Operation) -> Result<Option<Envelope>, CoreError> {
    if source.starts_with('{') {
        let Some((end, values)) = structured::json_envelope(source, op)? else {
            return Ok(None);
        };
        let (line_end, after) = line(source, end, op)?;
        if !source.as_bytes()[end..line_end]
            .iter()
            .all(|byte| matches!(byte, b' ' | b'\t'))
        {
            return Ok(None);
        }
        return Ok(Some(Envelope {
            span: 0..after,
            content: 0..end,
            format: Format::Json,
            json_values: values,
        }));
    }

    let (marker, format) = if source.starts_with("---") {
        ("---", Format::Yaml)
    } else if source.starts_with("+++") {
        ("+++", Format::Toml)
    } else {
        return Ok(None);
    };
    let (end, mut next) = line(source, 0, op)?;
    if source[..end].trim_end_matches(marker_space) != marker || next == end {
        return Ok(None);
    }
    let content_start = next;
    while next < source.len() {
        let start = next;
        let (end, after) = line(source, start, op)?;
        // Leading whitespace is deliberately not trimmed: no indented marker.
        if source[start..end].trim_end_matches(marker_space) == marker {
            return Ok(Some(Envelope {
                span: 0..after,
                content: content_start..start,
                format,
                json_values: 0,
            }));
        }
        next = after;
    }
    Ok(None)
}

pub(crate) fn prepare(
    source: &SourceSnapshot,
    config: ParseConfig,
    op: &mut Operation,
) -> Result<Option<Prepared>, CoreError> {
    let extensions = config.extensions();
    if !extensions.contains(Extensions::FRONT_MATTER) {
        return Ok(None);
    }
    let code = extensions.contains(Extensions::FRONT_MATTER_CODE);
    let hide = extensions.contains(Extensions::FRONT_MATTER_HIDE);
    if config.profile() == Profile::C0 && !code && !hide {
        return Err(op.fail(CoreError::UnsupportedConfiguration));
    }
    let Some(Envelope {
        span,
        content,
        format,
        json_values,
    }) = envelope(source.text(), op)?
    else {
        return Ok(None);
    };
    let storage = op
        .budget()
        .reserve_auxiliary((2 * MAX_OUTPUT_BYTES + 1024 * 1024) as u64)
        .map_err(|error| op.fail(error))?;
    let mut fragment = Fragment::default();
    let mut table = false;
    let mut references = Vec::new();
    let citations = extensions.contains(Extensions::CITATIONS)
        && format == Format::Yaml
        && content.len() <= MAX_METADATA_BYTES;
    let invalid_input = || {
        format!(
            "{} input exceeds the preview resource limit.",
            format.name()
        )
    };
    let invalid_output = || {
        format!(
            "{} output exceeds the preview resource limit.",
            format.name()
        )
    };
    if !hide && content.len() > MAX_METADATA_BYTES {
        error(&mut fragment, format, &invalid_input(), op)?;
    } else if !hide || format == Format::Json || citations {
        let raw = normalized(&source.text()[content], op)?;
        let raw = raw.strip_suffix('\n').unwrap_or(&raw);
        let entries = if format == Format::Json {
            // Braces alone can be prose. Require a complete valid object before
            // projecting it, including in Code and Hide modes.
            let Some(entries) = structured::json_entries(raw, json_values, !code && !hide, op)?
            else {
                return Ok(None);
            };
            Ok(entries)
        } else if format == Format::Toml {
            if hide || code {
                Ok(Vec::new())
            } else {
                structured::toml_entries(raw, op)?
            }
        } else if raw.len() <= MAX_METADATA_BYTES {
            let table = !hide && !code;
            if table || citations {
                value::parse_metadata(raw, table, citations, op)?.map(|metadata| {
                    references = metadata.references;
                    metadata.entries
                })
            } else {
                Ok(Vec::new())
            }
        } else {
            Err(invalid_input())
        };
        if code {
            fenced(&mut fragment.markdown, raw, format.language(), op)?;
        } else if !hide {
            match entries {
                Ok(entries) => {
                    for (index, entry) in entries.into_iter().enumerate() {
                        fragment.markdown.push('|');
                        plain(&mut fragment.markdown, &entry.key, op)?;
                        fragment.markdown.push('|');
                        display(&mut fragment, &entry.value, 0, op)?;
                        fragment.markdown.push_str("|\n");
                        if index == 0 {
                            fragment.markdown.push_str("|---|---|\n");
                        }
                        if fragment.markdown.len() > MAX_OUTPUT_BYTES {
                            table = false;
                            error(&mut fragment, format, &invalid_output(), op)?;
                            break;
                        }
                        table = true;
                        op.events(1)?;
                    }
                }
                Err(message) => error(&mut fragment, format, &message, op)?,
            }
        }
    }
    if fragment.markdown.len() > MAX_OUTPUT_BYTES {
        table = false;
        error(&mut fragment, format, &invalid_output(), op)?;
    }
    // Sorting JS integer properties and expanding aliases is not an exact,
    // monotonic field correspondence. Preserve one honest atomic source block.
    fragment.parts.push(Part {
        source: 0..span.len(),
        output: 0..fragment.markdown.len(),
    });
    Ok(Some(Prepared {
        span,
        fragment,
        table,
        references,
        storage,
    }))
}

impl Prepared {
    pub(crate) fn mask(
        &self,
        source: &SourceSnapshot,
        op: &mut Operation,
    ) -> Result<SourceSnapshot, CoreError> {
        let _storage = op
            .budget()
            .reserve_auxiliary(source.text().len() as u64)
            .map_err(|error| op.fail(error))?;
        let mut bytes = Vec::new();
        bytes
            .try_reserve_exact(source.text().len())
            .map_err(|_| op.fail(CoreError::OutOfMemory))?;
        for chunk in source.text().as_bytes().chunks(16 * 1024) {
            op.copy(chunk.len() as u64)?;
            let start = bytes.len();
            bytes.extend_from_slice(chunk);
            let masked = self.span.end.saturating_sub(start).min(chunk.len());
            op.scan(masked as u64)?;
            for byte in &mut bytes[start..start + masked] {
                if !matches!(*byte, b'\r' | b'\n') {
                    *byte = b' ';
                }
            }
        }
        Ok(SourceDocument::new(source.key(), &bytes, op)?.snapshot())
    }
}

fn normalized(content: &str, op: &mut Operation) -> Result<String, CoreError> {
    let mut out = String::new();
    out.try_reserve_exact(content.len())
        .map_err(|_| op.fail(CoreError::OutOfMemory))?;
    let mut previous_cr = false;
    let mut scanned = 0;
    let mut copied = 0;
    for c in content.chars() {
        scanned += c.len_utf8();
        if !(c == '\n' && previous_cr) {
            let next = match c {
                '\r' => '\n',
                '\0' => '\u{fffd}',
                other => other,
            };
            out.push(next);
            copied += next.len_utf8();
        }
        previous_cr = c == '\r';
        if scanned >= 8192 {
            op.scan(scanned as u64)?;
            op.copy(copied as u64)?;
            scanned = 0;
            copied = 0;
        }
    }
    op.scan(scanned as u64)?;
    op.copy(copied as u64)?;
    Ok(out)
}

fn append(out: &mut String, text: &str, op: &mut Operation) -> Result<(), CoreError> {
    let mut start = 0;
    while start < text.len() {
        let mut end = (start + 16 * 1024).min(text.len());
        while !text.is_char_boundary(end) {
            end -= 1;
        }
        op.copy((end - start) as u64)?;
        out.push_str(&text[start..end]);
        start = end;
    }
    Ok(())
}

fn fenced(
    out: &mut String,
    content: &str,
    language: &str,
    op: &mut Operation,
) -> Result<(), CoreError> {
    let mut longest = 0;
    let mut run = 0;
    for chunk in content.as_bytes().chunks(16 * 1024) {
        op.scan(chunk.len() as u64)?;
        for byte in chunk {
            run = if *byte == b'`' { run + 1 } else { 0 };
            longest = longest.max(run);
        }
    }
    let fence = "`".repeat(3.max(longest + 1));
    append(out, &fence, op)?;
    out.push_str(language);
    out.push('\n');
    append(out, content, op)?;
    if !content.ends_with('\n') {
        out.push('\n');
    }
    append(out, &fence, op)?;
    out.push('\n');
    Ok(())
}

fn error(
    fragment: &mut Fragment,
    format: Format,
    message: &str,
    op: &mut Operation,
) -> Result<(), CoreError> {
    fragment.markdown.clear();
    fragment.styles.clear();
    fragment.markdown.push_str("**Failed to parse ");
    fragment.markdown.push_str(format.name());
    fragment.markdown.push_str(" front matter**\n\n");
    fenced(&mut fragment.markdown, message, "text", op)
}

fn plain(out: &mut String, value: &str, op: &mut Operation) -> Result<(), CoreError> {
    let mut space = false;
    let mut seen = false;
    let mut scanned = 0;
    let mut copied_from = out.len();
    for c in value.chars() {
        scanned += c.len_utf8();
        if scanned >= 4096 {
            op.scan(scanned as u64)?;
            op.copy((out.len() - copied_from) as u64)?;
            scanned = 0;
            copied_from = out.len();
        }
        if out.len() > MAX_OUTPUT_BYTES {
            break;
        }
        if matches!(c, '\t' | '\n' | '\r' | '\u{000c}' | ' ') {
            space |= seen;
            continue;
        }
        if space {
            out.push(' ');
            space = false;
        }
        // Entity decoding happens after syntax recognition, keeping all YAML
        // punctuation inert under both ordinary G3 and experimental G4.
        if c.is_ascii_punctuation() {
            use std::fmt::Write;
            write!(out, "&#{};", c as u32).unwrap();
        } else {
            out.push(c);
        }
        seen = true;
    }
    op.scan(scanned as u64)?;
    op.copy((out.len() - copied_from) as u64)?;
    Ok(())
}

fn display(
    fragment: &mut Fragment,
    value: &value::DisplayValue,
    depth: usize,
    op: &mut Operation,
) -> Result<(), CoreError> {
    op.events(1)?;
    match value {
        value::DisplayValue::Text(text) => plain(&mut fragment.markdown, text, op)?,
        value::DisplayValue::Code(text) => {
            let start = fragment.markdown.len() as u64;
            plain(&mut fragment.markdown, text, op)?;
            fragment
                .styles
                .push((start..fragment.markdown.len() as u64, RUN_CODE));
        }
        value::DisplayValue::List(items) => {
            for (index, item) in items.iter().enumerate() {
                if fragment.markdown.len() > MAX_OUTPUT_BYTES {
                    return Ok(());
                }
                if index != 0 {
                    fragment.markdown.push_str("&#10;");
                }
                for _ in 0..depth {
                    fragment.markdown.push_str("&#160;&#160;");
                }
                fragment.markdown.push_str("• ");
                if matches!(item, value::DisplayValue::List(_)) {
                    fragment.markdown.push_str("&#10;");
                }
                display(fragment, item, depth + 1, op)?;
            }
        }
    }
    Ok(())
}
