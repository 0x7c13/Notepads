// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE file in the project root for license information.
// ---------------------------------------------------------------------------------------------

//! Closed colon containers projected onto the existing native callout grammar.

use std::ops::Range;

use crate::error::CoreError;
use crate::parse::ProjectionFacts;
use crate::policy::Lease;

use super::fragment::{Fragment, Part};
use super::work::ProjectionWork;

const MAX_FENCES: usize = 256;
const MAX_DEPTH: usize = 8;
const MAX_LINES: usize = 1536;
const MAX_TITLE: usize = 1024;

pub(super) struct Prepared {
    pub fragments: Vec<(Range<usize>, Fragment)>,
    pub storage: Lease,
}

#[derive(Clone, Copy)]
struct Line {
    start: usize,
    body_end: usize,
    end: usize,
}

impl Line {
    fn read(source: &str, start: usize) -> Option<Self> {
        if start == source.len() {
            return None;
        }
        let body_end = start
            + source[start..]
                .find(['\r', '\n'])
                .unwrap_or(source.len() - start);
        let end = if source[body_end..].starts_with("\r\n") {
            body_end + 2
        } else {
            (body_end + 1).min(source.len())
        };
        Some(Self {
            start,
            body_end,
            end,
        })
    }
}

struct Opening {
    fence: usize,
    label: &'static str,
    title: Range<usize>,
    default_title: Option<&'static str>,
    fold: Option<bool>,
}

struct Marker {
    line: Line,
    parent_depth: usize,
    opening: Option<Opening>,
}

fn trim_horizontal(value: &str) -> &str {
    value.trim_matches([' ', '\t'])
}

fn opening(raw: &str, start: usize, fence: usize) -> Option<Opening> {
    let info = trim_horizontal(raw);
    let begin = start + (info.as_ptr() as usize - raw.as_ptr() as usize);
    let (kind, title, myst) = if let Some(brace) = info.strip_prefix('{') {
        let (kind, title) = brace.split_once('}')?;
        if !trim_horizontal(title).is_empty() {
            return None;
        }
        (kind, title, true)
    } else {
        let end = info.find([' ', '\t']).unwrap_or(info.len());
        (&info[..end], &info[end..], false)
    };
    let label = match (myst, kind) {
        (false, "info") => "INFO",
        (false, "details") => "QUOTE",
        (_, "note") => "NOTE",
        (_, "tip") | (true, "hint") => "TIP",
        (_, "warning") | (true, "attention") => "WARNING",
        (_, "danger") => "DANGER",
        (true, "caution") => "CAUTION",
        (true, "important") => "IMPORTANT",
        _ => return None,
    };
    let mut title = trim_horizontal(title);
    let mut fold = (kind == "details").then_some(false);
    if fold.is_some()
        && let Some(base) = title.strip_suffix("{open}")
        && (base.is_empty() || base.ends_with([' ', '\t']))
    {
        fold = Some(true);
        title = base.trim_end_matches([' ', '\t']);
    }
    if title.len() > MAX_TITLE || title.ends_with('}') || title.chars().any(char::is_control) {
        return None;
    }
    let default_title = (fold.is_some() && title.is_empty()).then_some("Details");
    let title = if default_title.is_some() {
        begin..begin + kind.len()
    } else {
        let from = start + (title.as_ptr() as usize - raw.as_ptr() as usize);
        from..from + title.len()
    };
    Some(Opening {
        fence,
        label,
        title,
        default_title,
        fold,
    })
}

fn part(fragment: &mut Fragment, raw: Range<usize>, text: &str) {
    if !text.is_empty() {
        let start = fragment.markdown.len();
        fragment.markdown.push_str(text);
        fragment.parts.push(Part {
            source: raw,
            output: start..fragment.markdown.len(),
        });
    }
}

fn marker_fragment(marker: &Marker, source: &str) -> Fragment {
    let line = marker.line;
    let mut fragment = Fragment::default();
    let mut prefix = "> ".repeat(marker.parent_depth);
    if let Some(opening) = &marker.opening {
        prefix.push_str("> [!");
        prefix.push_str(opening.label);
        prefix.push(']');
        if let Some(expanded) = opening.fold {
            prefix.push(if expanded { '+' } else { '-' });
        }
        if !opening.title.is_empty() {
            prefix.push(' ');
        }
        part(&mut fragment, 0..0, &prefix);
        part(
            &mut fragment,
            opening.title.start - line.start..opening.title.end - line.start,
            opening
                .default_title
                .unwrap_or(&source[opening.title.clone()]),
        );
    } else {
        part(&mut fragment, 0..0, &prefix);
    }
    part(
        &mut fragment,
        line.body_end - line.start..line.end - line.start,
        &source[line.body_end..line.end],
    );
    fragment
}

fn append_group(
    source: &str,
    markers: &[Marker],
    edits: &mut Vec<(Range<usize>, Fragment)>,
    op: &mut impl ProjectionWork,
) -> Result<bool, CoreError> {
    let mut cursor = markers[0].line.start;
    let stop = markers.last().unwrap().line.end;
    for window in source.as_bytes()[cursor..stop].chunks(4096) {
        op.scan(window.len() as u64)?;
    }
    let mut markers = markers.iter().peekable();
    let mut depth = 0;
    while cursor < stop {
        let line =
            Line::read(source, cursor).ok_or_else(|| op.fail(CoreError::InvalidProvenance))?;
        op.events(1)?;
        if edits.len() >= MAX_LINES {
            return Ok(false);
        }
        if let Some(marker) = markers.next_if(|marker| marker.line.start == line.start) {
            let fragment = marker_fragment(marker, source);
            op.copy(fragment.markdown.len() as u64)?;
            edits.push((line.start..line.end, fragment));
            depth = marker.parent_depth + usize::from(marker.opening.is_some());
        } else {
            let mut fragment = Fragment::default();
            part(&mut fragment, 0..0, &"> ".repeat(depth));
            op.copy(fragment.markdown.len() as u64)?;
            edits.push((line.start..line.start, fragment));
        }
        cursor = line.end;
    }
    Ok(true)
}

pub(super) fn prepare(
    facts: &ProjectionFacts,
    source: &str,
    overlaps_projection: impl Fn(&Range<usize>) -> bool,
    op: &mut impl ProjectionWork,
) -> Result<Option<Prepared>, CoreError> {
    let mut eligible = facts.directive_text().peekable();
    if eligible.peek().is_none() {
        return Ok(None);
    }
    let Ok(storage) = op.reserve_optional(2 * 1024 * 1024) else {
        return Ok(None);
    };
    for window in source.as_bytes().chunks(4096) {
        op.scan(window.len() as u64)?;
    }
    let mut pending = Vec::new();
    let mut stack = Vec::new();
    let mut fragments = Vec::new();
    let mut valid = true;
    let mut cursor = 0;
    while let Some(line) = Line::read(source, cursor) {
        cursor = line.end;
        op.events(1)?;
        let raw = &source[line.start..line.body_end];
        let start = raw
            .as_bytes()
            .iter()
            .take(4)
            .take_while(|&&byte| byte == b' ')
            .count();
        if start > 3 || !raw[start..].starts_with(":::") {
            continue;
        }
        let delimiter = (line.start + start) as u64;
        while eligible
            .peek()
            .is_some_and(|(span, _)| span.end() <= delimiter)
        {
            eligible.next();
            op.events(1)?;
        }
        let Some((proof, root_text)) = eligible.peek() else {
            break;
        };
        if !root_text && start != 0 && !stack.last().is_some_and(|&(_, indent)| start == indent) {
            continue;
        }
        if proof.start() > delimiter || proof.end() < delimiter + 3 {
            continue;
        }
        let fence = raw.as_bytes()[start..]
            .iter()
            .take_while(|&&b| b == b':')
            .count();
        if proof.end() < delimiter + fence as u64 {
            continue;
        }
        let rest = &raw[start + fence..];
        if trim_horizontal(rest).is_empty() {
            if !stack.last().is_some_and(|&(width, _)| fence >= width) {
                continue;
            }
            stack.pop();
            pending.push(Marker {
                line,
                parent_depth: stack.len(),
                opening: None,
            });
            if stack.is_empty() {
                let span = pending[0].line.start..line.end;
                if valid && !overlaps_projection(&span) {
                    let before = fragments.len();
                    if !append_group(source, &pending, &mut fragments, op)? {
                        fragments.truncate(before);
                    }
                }
                pending.clear();
                valid = true;
            }
        } else {
            // Long literal lines or unknown kinds remain ordinary Markdown.
            if rest.len() > MAX_TITLE + 64 {
                continue;
            }
            let Some(opening) = opening(rest, line.start + start + fence, fence) else {
                continue;
            };
            if stack.len() == MAX_DEPTH {
                return Ok(None);
            }
            valid &= stack.last().is_none_or(|&(outer, _)| opening.fence < outer);
            let depth = stack.len();
            stack.push((opening.fence, start));
            pending.push(Marker {
                line,
                parent_depth: depth,
                opening: Some(opening),
            });
        }
        if pending.len() >= MAX_FENCES {
            return Ok(None);
        }
    }
    Ok((!fragments.is_empty()).then_some(Prepared { fragments, storage }))
}
