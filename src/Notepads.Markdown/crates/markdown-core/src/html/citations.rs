// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE file in the project root for license information.
// ---------------------------------------------------------------------------------------------

//! Source-local author-year citation projection.

use std::fmt::Write;
use std::ops::Range;

use crate::error::CoreError;
use crate::parse::ProjectionFacts;
use crate::policy::Lease;

use super::fragment::{Anchor, AnchorKind, Fragment, Part};
use super::work::ProjectionWork;

pub(super) const MAX_REFERENCES: usize = 128;

pub(super) struct Reference {
    pub key: String,
    pub author: String,
    pub byline: String,
    pub year: String,
    pub title: String,
}

const DESTINATION_PREFIX: &str = "notepads-reference-";
const MAX_CLUSTER_BYTES: usize = 1024;
const MAX_OCCURRENCES: usize = 512;
const MAX_OUTPUT_BYTES: usize = 512 * 1024;

pub(super) struct Prepared {
    pub inlines: Vec<(Range<usize>, Fragment)>,
    pub bibliography: Fragment,
    pub storage: Lease,
}

pub(super) fn valid_key(value: &str) -> bool {
    value.chars().next().is_some_and(word)
        && value.chars().next_back().is_some_and(word)
        && value.chars().all(key_char)
}

fn word(value: char) -> bool {
    value.is_alphanumeric() || value == '_'
}

fn key_char(value: char) -> bool {
    word(value) || matches!(value, '-' | ':' | '.' | '+')
}

fn link(out: &mut String, label: &str, index: usize) {
    out.push('[');
    plain(out, label);
    write!(out, "](#{DESTINATION_PREFIX}{index:03})").unwrap();
}

fn plain(out: &mut String, value: &str) {
    for c in value.chars() {
        if c.is_ascii_punctuation() {
            write!(out, "&#{};", u32::from(c)).unwrap();
        } else {
            out.push(c);
        }
    }
}

fn index(references: &[Reference], key: &str) -> Option<usize> {
    references
        .binary_search_by(|reference| reference.key.as_str().cmp(key))
        .ok()
}

fn group(
    out: &mut String,
    raw: &str,
    references: &[Reference],
    used: &mut [bool; MAX_REFERENCES],
) -> Option<()> {
    let mut members = [(0, false, ""); 16];
    let mut count = 0;
    for item in raw.split(';') {
        let item = item.trim();
        let (label, locator) = item.split_once(',').unwrap_or((item, ""));
        let (key, suppressed) = if let Some(key) = label.strip_prefix("-@") {
            (key, true)
        } else {
            (label.strip_prefix('@')?, false)
        };
        let locator = locator.trim();
        if !valid_key(key)
            || locator.len() > 96
            || locator.contains(['\\', '[', ']', '@', '`'])
            || locator.chars().any(char::is_control)
        {
            return None;
        }
        *members.get_mut(count)? = (index(references, key)?, suppressed, locator);
        count += 1;
    }
    out.push_str("&#40;");
    for (position, &(i, suppressed, locator)) in members[..count].iter().enumerate() {
        if position != 0 {
            out.push_str("&#59; ");
        }
        let reference = &references[i];
        let mut label = String::new();
        if !suppressed {
            label.push_str(&reference.author);
            label.push(' ');
        }
        label.push_str(&reference.year);
        if !locator.is_empty() {
            label.push_str(", ");
            label.push_str(locator);
        }
        link(out, &label, i);
        used[i] = true;
    }
    out.push_str("&#41;");
    Some(())
}

fn bare_key(tail: &str) -> Option<&str> {
    let mut end = 0;
    for (next, c) in tail.char_indices() {
        if !key_char(c) {
            break;
        }
        if next >= 128 {
            return None;
        }
        if word(c) {
            end = next + c.len_utf8();
        }
    }
    let key = &tail[..end];
    valid_key(key).then_some(key)
}

fn escaped(source: &str, start: usize, op: &mut impl ProjectionWork) -> Result<bool, CoreError> {
    let mut count = 0;
    for window in source.as_bytes()[..start].rchunks(4096) {
        let slashes = window
            .iter()
            .rev()
            .take_while(|&&byte| byte == b'\\')
            .count();
        op.scan(slashes as u64 + u64::from(slashes != window.len()))?;
        count += slashes;
        if slashes != window.len() {
            break;
        }
    }
    Ok(count % 2 != 0)
}

fn group_extent(
    source: &str,
    start: usize,
    op: &mut impl ProjectionWork,
) -> Result<(usize, bool), CoreError> {
    let mut limit = (start + MAX_CLUSTER_BYTES).min(source.len());
    while !source.is_char_boundary(limit) {
        limit -= 1;
    }
    let candidate = &source[start + 1..limit];
    let end = candidate.find([']', '[']);
    op.scan(end.map_or(candidate.len(), |i| i + 1) as u64)?;
    let end = end.map(|i| i + start + 1);
    match end {
        Some(end) if source.as_bytes()[end] == b']' => Ok((end + 1, true)),
        Some(end) => Ok((end, false)),
        None => Ok((limit, false)),
    }
}

pub(super) fn prepare(
    facts: &ProjectionFacts,
    source: &str,
    references: &[Reference],
    overlaps_html: impl Fn(&Range<usize>) -> bool,
    op: &mut impl ProjectionWork,
) -> Result<Option<Prepared>, CoreError> {
    if references.is_empty() || facts.citation_text().next().is_none() {
        return Ok(None);
    }
    // Reserve the retained fragments and their bounded temporary assembly.
    let Ok(storage) = op.reserve_optional(
        (4 * MAX_OUTPUT_BYTES + MAX_OCCURRENCES * std::mem::size_of::<Fragment>() * 2) as u64,
    ) else {
        return Ok(None);
    };
    // The presentation resolves these destinations only inside emitted citation segments.
    let mut inlines = Vec::new();
    let mut used = [false; MAX_REFERENCES];
    let mut generated = 0;
    let mut blocked_until = 0;
    let mut scope = None;
    for entry in facts.citation_text() {
        if scope != Some(entry.scope) {
            blocked_until = 0;
            scope = Some(entry.scope);
        }
        let span = entry.span.start() as usize..entry.span.end() as usize;
        let raw = source
            .get(span.clone())
            .ok_or_else(|| op.fail(CoreError::InvalidProvenance))?;
        for window in raw.as_bytes().chunks(4096) {
            op.scan(window.len() as u64)?;
        }
        for (at, marker) in raw.match_indices(['[', '@']) {
            op.events(1)?;
            let start = span.start + at;
            if start < blocked_until {
                continue;
            }
            let rest = &raw[at..];
            let (end, group_body, bare) = if marker == "[" {
                if !(rest.starts_with("[@") || rest.starts_with("[-@")) {
                    continue;
                }
                let (end, closed) = group_extent(source, start, op)?;
                blocked_until = end;
                if !closed
                    || end > span.end
                    || source[..start].ends_with('[')
                    || escaped(source, start, op)?
                    || source[end..].starts_with(']')
                {
                    continue;
                }
                let body = &source[start + 1..end - 1];
                if body.contains(['\r', '\n']) {
                    continue;
                }
                (end, Some(body), None)
            } else {
                if let Some(opener) = source[..start]
                    .strip_suffix('[')
                    .or_else(|| source[..start].strip_suffix("[-"))
                {
                    blocked_until = group_extent(source, opener.len(), op)?.0;
                    continue;
                }
                let previous = source[..start].chars().next_back();
                if previous.is_some_and(|c| !c.is_whitespace() && !matches!(c, '(' | '"' | '“')) {
                    continue;
                }
                let Some(key) = bare_key(&rest[1..]) else {
                    continue;
                };
                let end = start + 1 + key.len();
                if source[end..]
                    .chars()
                    .next()
                    .is_some_and(|c| c == '@' || word(c) || c == '/')
                {
                    continue;
                }
                let Some(i) = index(references, key) else {
                    continue;
                };
                (end, None, Some(i))
            };
            let occurrence = start..end;
            if overlaps_html(&occurrence) {
                continue;
            }
            let mut markdown = String::new();
            if let Some(body) = group_body {
                if group(&mut markdown, body, references, &mut used).is_none() {
                    continue;
                }
            } else if let Some(i) = bare {
                let reference = &references[i];
                let label = format!("{} ({})", reference.author, reference.year);
                link(&mut markdown, &label, i);
                used[i] = true;
            }
            generated += markdown.len();
            if inlines.len() == MAX_OCCURRENCES || generated > MAX_OUTPUT_BYTES {
                return Ok(None);
            }
            blocked_until = end;
            for window in markdown.as_bytes().chunks(4096) {
                op.copy(window.len() as u64)?;
            }
            inlines.push((
                occurrence,
                Fragment {
                    parts: vec![Part {
                        source: 0..end - start,
                        output: 0..markdown.len(),
                    }],
                    markdown,
                    citation: true,
                    ..Fragment::default()
                },
            ));
        }
    }
    if inlines.is_empty() {
        return Ok(None);
    }

    let mut bibliography = Fragment {
        suppress_email_autolinks: true,
        ..Fragment::default()
    };
    bibliography.markdown.push_str("## References\n\n");
    for (i, reference) in references.iter().enumerate().filter(|(i, _)| used[*i]) {
        op.events(1)?;
        bibliography.markdown.push_str("- ");
        bibliography.anchors.push(Anchor {
            name: format!("{DESTINATION_PREFIX}{i:03}"),
            offset: bibliography.markdown.len(),
            legacy: false,
            kind: AnchorKind::Text,
        });
        plain(&mut bibliography.markdown, &reference.byline);
        bibliography.markdown.push_str(" &#40;");
        plain(&mut bibliography.markdown, &reference.year);
        bibliography.markdown.push_str("&#41;. ");
        plain(&mut bibliography.markdown, &reference.title);
        bibliography.markdown.push('\n');
        if generated + bibliography.markdown.len() > MAX_OUTPUT_BYTES {
            return Ok(None);
        }
    }
    for window in bibliography.markdown.as_bytes().chunks(4096) {
        op.copy(window.len() as u64)?;
    }
    bibliography.parts.push(Part {
        source: 0..0,
        output: 0..bibliography.markdown.len(),
    });
    Ok(Some(Prepared {
        inlines,
        bibliography,
        storage,
    }))
}
