// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE file in the project root for license information.
// ---------------------------------------------------------------------------------------------

//! Pair standalone safe HTML markers across native Markdown blocks.

use super::fragment::{self, Fragment, Part};
use super::tag_filter::Filtered;
use super::work::ProjectionWork;
use crate::error::CoreError;
use crate::identity::LogicalSpan;
use crate::ir::{NodeId, NodeKind};
use crate::model::{AcceptedModel, DisclosureRegion};
use crate::parse::ProjectionFacts;
use crate::policy::{Lease, OwnedWork};
use crate::presentation::RUN_BOLD;

pub(super) const MAX_DISCLOSURES: usize = 128;

#[derive(Clone, Copy)]
pub(super) enum MarkerKind {
    Open {
        summary_offset: usize,
        initially_expanded: bool,
    },
    Remove,
    Close,
}

#[derive(Clone, Copy)]
pub(super) struct Marker {
    pub at: usize,
    pub pair: usize,
    pub kind: MarkerKind,
}

pub(super) struct Markers {
    pub rows: Vec<Marker>,
    pub pairs: usize,
    _storage: Lease,
}

struct Opening {
    at: usize,
    end: usize,
    label_at: usize,
    summary_offset: usize,
    has_authored_summary: bool,
    initially_expanded: bool,
}

#[derive(Clone, Copy, Debug, Default)]
pub(super) struct Projected {
    pub summary: Option<(usize, usize)>,
    pub close: Option<usize>,
    pub initially_expanded: bool,
}

impl Projected {
    pub fn bind(
        self,
        model: &AcceptedModel,
        next: &mut Option<NodeId>,
        work: &mut OwnedWork<'_, '_>,
    ) -> Result<Option<DisclosureRegion>, CoreError> {
        let (Some((start, end)), Some(close)) = (self.summary, self.close) else {
            return Err(work.fail(CoreError::InvalidProvenance));
        };
        let nodes = model.ir().nodes();
        let summary = loop {
            work.events(1)?;
            let Some(id) = *next else { return Ok(None) };
            work.scan(size_of::<crate::ir::NodeRow>() as u64)?;
            let row = nodes[id.index()];
            if row.kind == NodeKind::FootnoteDefinition || row.source_envelope.start() >= end as u64
            {
                return Ok(None);
            }
            *next = row.next_sibling;
            if row.source_envelope.start() >= start as u64 {
                if row.kind != NodeKind::Paragraph || row.logical.is_empty() {
                    return Ok(None);
                }
                let row_end = row.source_envelope.end() as usize;
                if row_end > end {
                    work.scan((row_end - end) as u64)?;
                    if !model.source().text().as_bytes()[end..row_end]
                        .iter()
                        .all(u8::is_ascii_whitespace)
                    {
                        return Ok(None);
                    }
                }
                break id;
            }
        };
        let header = nodes[summary.index()];
        let mut body = header.next_sibling;
        let mut end_logical = header.logical.end();
        while let Some(id) = body {
            work.events(1)?;
            work.scan(size_of::<crate::ir::NodeRow>() as u64)?;
            let row = nodes[id.index()];
            if row.kind == NodeKind::FootnoteDefinition
                || row.source_envelope.start() >= close as u64
            {
                break;
            }
            if row.source_envelope.start() < end as u64 || row.source_envelope.end() > close as u64
            {
                return Ok(None);
            }
            end_logical = row.logical.end();
            body = row.next_sibling;
        }
        let next_node = if let Some(id) = body {
            id.value()
        } else {
            u32::try_from(nodes.len())
                .ok()
                .and_then(|count| count.checked_add(1))
                .ok_or_else(|| work.fail(CoreError::ArithmeticOverflow))?
        };
        let body = LogicalSpan::new(header.logical.end(), end_logical)
            .map_err(|error| work.fail(error))?;
        Ok(Some(DisclosureRegion {
            node: summary,
            summary,
            first_body_node: header.next_sibling.map_or(next_node, NodeId::value),
            next_node,
            summary_start: header.logical.start(),
            body,
            initially_expanded: self.initially_expanded,
        }))
    }
}

fn start_tag<'a>(input: &'a str, name: &str) -> Option<(&'a str, usize)> {
    let prefix = input.as_bytes().get(1..1 + name.len())?;
    if !input.starts_with('<') || !prefix.eq_ignore_ascii_case(name.as_bytes()) {
        return None;
    }
    let rest = input.get(1 + name.len()..)?;
    if !rest.starts_with(|ch: char| ch.is_ascii_whitespace() || ch == '>') {
        return None;
    }
    let mut quote = None;
    for (i, byte) in rest.bytes().enumerate() {
        match (quote, byte) {
            (Some(active), byte) if active == byte => quote = None,
            (None, b'\'' | b'"') => quote = Some(byte),
            (None, b'>') => return Some((&rest[..i], 2 + name.len() + i)),
            (None, b'<') => return None,
            _ => {}
        }
    }
    None
}

fn open_attribute(mut input: &str) -> Option<bool> {
    let mut open = false;
    loop {
        input = input.trim_ascii_start();
        if input.is_empty() {
            return Some(open);
        }
        let end = input
            .find(|ch: char| ch.is_ascii_whitespace() || ch == '=')
            .unwrap_or(input.len());
        if end == 0
            || !input[..end]
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_'))
        {
            return None;
        }
        open |= input[..end].eq_ignore_ascii_case("open");
        input = input[end..].trim_ascii_start();
        if let Some(rest) = input.strip_prefix('=') {
            input = rest.trim_ascii_start();
            let first = input.chars().next()?;
            if matches!(first, '\'' | '"') {
                let end = input[1..].find(first)? + 2;
                input = input.get(end..)?;
                if input.starts_with(|ch: char| !ch.is_ascii_whitespace()) {
                    return None;
                }
            } else {
                let end = input
                    .find(|ch: char| ch.is_ascii_whitespace())
                    .unwrap_or(input.len());
                if input[..end].contains(['\'', '"', '`', '=', '<', '>']) {
                    return None;
                }
                input = &input[end..];
            }
        }
    }
}

fn summary_tag(summary: &str) -> Option<()> {
    let (_, header_end) = start_tag(summary, "summary")?;
    let close = summary.len().checked_sub("</summary>".len())?;
    (close >= header_end && summary.get(close..)?.eq_ignore_ascii_case("</summary>")).then_some(())
}

fn opening(raw: &str, at: usize) -> Option<Opening> {
    let trimmed = raw.trim_ascii_start();
    let (attributes, end) = start_tag(trimmed, "details")?;
    let initially_expanded = open_attribute(attributes)?;
    let summary = trimmed[end..].trim_ascii();
    if !summary.is_empty() {
        summary_tag(summary)?;
    }
    let summary_offset = if summary.is_empty() {
        raw.len() - trimmed.len() + end
    } else {
        raw.len() - trimmed[end..].trim_ascii_start().len()
    };
    Some(Opening {
        at,
        end: at + raw.len(),
        label_at: at,
        summary_offset,
        has_authored_summary: !summary.is_empty(),
        initially_expanded,
    })
}

pub(super) fn scan(
    facts: &ProjectionFacts,
    source: &str,
    tagfilter: bool,
    op: &mut impl ProjectionWork,
) -> Result<Option<Markers>, CoreError> {
    let bytes = MAX_DISCLOSURES * (size_of::<Opening>() + 3 * size_of::<Marker>());
    let mut storage = None;
    let mut stack: Vec<Opening> = Vec::new();
    let mut rows = Vec::new();
    let mut pairs = 0;
    let mut nested_bytes = 0usize;
    for block in facts.blocks() {
        op.events(1)?;
        if block.kind != NodeKind::HtmlBlock {
            continue;
        }
        if block.source.len() > fragment::MAX_FRAGMENT_BYTES as u64 {
            // A block beyond the tokenizer limit cannot prove that any
            // pending outer scope reaches its next standalone closing tag.
            stack.clear();
            continue;
        }
        let start = block.source.start() as usize;
        let raw = source
            .get(start..block.source.end() as usize)
            .ok_or_else(|| op.fail(CoreError::InvalidProvenance))?;
        op.scan(raw.len() as u64)?;
        let trimmed = raw.trim_ascii();
        if trimmed.eq_ignore_ascii_case("</details>") {
            if let Some(open) = stack.pop() {
                if pairs == MAX_DISCLOSURES {
                    return Ok(None);
                }
                if open.label_at != open.at {
                    rows.push(Marker {
                        at: open.at,
                        pair: pairs,
                        kind: MarkerKind::Remove,
                    });
                }
                rows.push(Marker {
                    at: open.label_at,
                    pair: pairs,
                    kind: MarkerKind::Open {
                        summary_offset: open.summary_offset,
                        initially_expanded: open.initially_expanded,
                    },
                });
                rows.push(Marker {
                    at: start,
                    pair: pairs,
                    kind: MarkerKind::Close,
                });
                pairs += 1;
            }
        } else if let Some(open) = opening(raw, start) {
            if storage.is_none() {
                let Ok(lease) = op.reserve_optional(bytes as u64) else {
                    return Ok(None);
                };
                storage = Some(lease);
                if stack.try_reserve_exact(MAX_DISCLOSURES).is_err()
                    || rows.try_reserve_exact(3 * MAX_DISCLOSURES).is_err()
                {
                    return Ok(None);
                }
            }
            if stack.len() == MAX_DISCLOSURES {
                return Ok(None);
            }
            stack.push(open);
        } else if let Some(open) = stack.last_mut() {
            if !open.has_authored_summary
                && summary_tag(trimmed).is_some()
                && let Some(gap) = source.get(open.end..start)
            {
                let mut whitespace = true;
                for chunk in gap
                    .as_bytes()
                    .chunks(crate::policy::CHECKPOINT_BYTES as usize)
                {
                    op.scan(chunk.len() as u64)?;
                    if !chunk.iter().all(u8::is_ascii_whitespace) {
                        whitespace = false;
                        break;
                    }
                }
                if whitespace {
                    open.label_at = start;
                    open.summary_offset = raw.len() - raw.trim_ascii_start().len();
                    open.has_authored_summary = true;
                    continue;
                }
            }
            // A nested opener can follow other tags in this native HTML block.
            // Keep the pending outer scope only if the entire nested fragment
            // balances under the same safe tokenizer used for projection.
            if raw.as_bytes().windows(9).any(|part| {
                part[..8].eq_ignore_ascii_case(b"<details")
                    && (part[8] == b'>' || part[8].is_ascii_whitespace())
            }) {
                nested_bytes += raw.len();
                let balanced = if nested_bytes <= super::MAX_HTML_BYTES
                    && let Ok(scratch) =
                        op.reserve_optional(2 * 1024 * 1024 + 256 * raw.len() as u64)
                {
                    let result = if let Some(filtered) = Filtered::new(raw, tagfilter) {
                        if filtered.changed() {
                            op.scan(filtered.text.len() as u64)?;
                        }
                        fragment::lower(&filtered.text, filtered.changed()).is_some()
                    } else {
                        false
                    };
                    drop(scratch);
                    op.checkpoint_now()?;
                    result
                } else {
                    false
                };
                if !balanced {
                    stack.clear();
                }
            }
        }
    }
    if rows.is_empty() {
        return Ok(None);
    }
    super::charge_events(op, rows.len() as u64 * (rows.len().ilog2() as u64 + 1))?;
    rows.sort_unstable_by_key(|row| row.at);
    Ok(Some(Markers {
        rows,
        pairs,
        _storage: storage.ok_or_else(|| op.fail(CoreError::InternalInvariant))?,
    }))
}

pub(super) fn lower_open(raw: &str, summary_offset: usize, tagfilter: bool) -> Option<Fragment> {
    let summary = raw.get(summary_offset..)?.trim_ascii_end();
    if !summary.is_empty() {
        let filtered = Filtered::new(summary, tagfilter)?;
        let mut lowered = fragment::lower_summary(&filtered.text, filtered.changed())?;
        filtered.restore(&mut lowered)?;
        if !lowered.markdown.trim_ascii().is_empty() {
            for part in &mut lowered.parts {
                part.source.start += summary_offset;
                part.source.end += summary_offset;
            }
            for destination in &mut lowered.destinations {
                destination.source.start += summary_offset;
                destination.source.end += summary_offset;
            }
            for picture in &mut lowered.pictures {
                for variant in picture.variants() {
                    variant.source.start += summary_offset;
                    variant.source.end += summary_offset;
                }
            }
            return Some(lowered);
        }
    }
    let label = String::from("Details");
    let end = label.len();
    Some(Fragment {
        markdown: label,
        parts: vec![Part {
            source: summary_offset..summary_offset,
            output: 0..end,
        }],
        styles: vec![(0..end as u64, RUN_BOLD)],
        ..Fragment::default()
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn summary_picture_variants_keep_their_original_attribute_spans() {
        let raw = "<details open>  <summary>Image <picture>\
                   <source media='(prefers-color-scheme: light)' srcset='light&amp;image.png'>\
                   <source media='(prefers-color-scheme: dark)' srcset='dark.png'>\
                   <img src='fallback.png'></picture></summary>\n";
        let offset = raw.find("<summary>").unwrap();
        let mut fragment = lower_open(raw, offset, false).unwrap();
        let [picture] = fragment.pictures.as_mut_slice() else {
            panic!("expected one picture");
        };
        let variants = picture
            .variants()
            .map(|variant| {
                (
                    raw[variant.source.clone()].to_owned(),
                    variant.destination.clone(),
                )
            })
            .collect::<Vec<_>>();
        assert_eq!(
            variants,
            [
                (
                    "light&amp;image.png".to_owned(),
                    "light&image.png".to_owned()
                ),
                ("dark.png".to_owned(), "dark.png".to_owned())
            ]
        );
    }
}
