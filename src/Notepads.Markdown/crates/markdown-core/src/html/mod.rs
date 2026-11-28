// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE file in the project root for license information.
// ---------------------------------------------------------------------------------------------

//! Source projection for HTML blocks in the native preview. The normal parser
//! still decides which source ranges are HTML blocks. The final verified model
//! gives supported fragments the same reading, layout and image behavior as
//! Markdown. Keep original source coordinates at the host boundary.

#[cfg(test)]
mod barrier_tests;
mod citations;
#[cfg(test)]
mod control_tests;
mod controls;
mod details;
mod directives;
mod fragment;
pub(crate) mod front_matter;
#[cfg(test)]
mod image_gallery_tests;
#[cfg(test)]
mod image_tests;
pub(crate) mod picture;
#[cfg(test)]
mod projection_tests;
pub(crate) mod ruby;
mod table;
#[cfg(test)]
mod table_tests;
mod tag_filter;
#[cfg(test)]
mod tests;
pub(crate) mod work;

pub(crate) use ruby::DisplayUnit as RubyDisplayUnit;
pub(crate) use table::CellFacts;

use crate::error::CoreError;
use crate::identity::{LogicalSpan, SourceBoundary, SourceSpan};
use crate::ir::{
    AtomId, AtomKind, DetailRow, FlowId, NodeId, NodeKind, ObjectId, ObjectRow, PayloadId,
    TextTarget,
};
use crate::mapping::{Affinity, Effects, PointInput, PointQuery, Position, RangeInput, RangeQuery};
use crate::model::AcceptedModel;
use crate::policy::{Lease, Operation};
use crate::source::SourceSnapshot;
use std::ops::Range;
use work::ProjectionWork;

const MAX_HTML_BYTES: usize = 256 * 1024;
const MAX_FRAGMENTS: usize = 256;
const MAX_SEGMENTS: usize = 16_384;
const MAX_RUBY_UNITS: usize = 1024;

fn charge_events(op: &mut impl ProjectionWork, mut count: u64) -> Result<(), CoreError> {
    while count != 0 {
        let chunk = count.min(crate::policy::CHECKPOINT_EVENTS);
        op.events(chunk)?;
        count -= chunk;
    }
    Ok(())
}

// A reference definition terminates lists/indented code without contributing a
// reading or layout block. Use a digits-only label: reference normalization's
// Unicode case folding cannot introduce an equivalent spelling elsewhere.
fn empty_barrier(
    source: &str,
    op: &mut impl ProjectionWork,
) -> Result<Option<(String, Lease)>, CoreError> {
    const PREFIX: &[u8] = b"0-0-";
    const WINDOW: usize = 16 * 1024;
    let bytes = source.as_bytes();
    let mut count = 0;
    for start in (0..bytes.len()).step_by(WINDOW) {
        let end = (start + WINDOW).min(bytes.len());
        op.scan((end - start) as u64)?;
        for at in start..end {
            count += usize::from(bytes[at..].starts_with(PREFIX));
        }
    }
    // k prefix occurrences cannot occupy all k+1 canonical decimal labels.
    let Ok(_scratch) = op.reserve_optional((count + 1) as u64) else {
        return Ok(None);
    };
    let mut seen = vec![false; count + 1];
    let mut active = false;
    let mut digits = false;
    let mut value = Some(0usize);
    for start in (0..bytes.len()).step_by(WINDOW) {
        let end = (start + WINDOW).min(bytes.len());
        op.scan((end - start) as u64)?;
        for at in start..end {
            let byte = bytes[at];
            if active && byte.is_ascii_digit() {
                digits = true;
                value = value.and_then(|n| n.checked_mul(10)?.checked_add((byte - b'0') as usize));
            } else {
                if active
                    && digits
                    && let Some(n) = value.filter(|&n| n <= count)
                {
                    seen[n] = true;
                }
                active = false;
            }
            if at + 1 >= PREFIX.len() && bytes[at + 1 - PREFIX.len()..=at] == *PREFIX {
                active = true;
                digits = false;
                value = Some(0);
            }
        }
    }
    if active
        && digits
        && let Some(n) = value.filter(|&n| n <= count)
    {
        seen[n] = true;
    }
    let nonce = seen.iter().position(|&occupied| !occupied).unwrap();
    let Ok(storage) = op.reserve_optional(64) else {
        return Ok(None);
    };
    let mut marker = String::with_capacity(64);
    use std::fmt::Write;
    write!(&mut marker, "\n\n[0-0-{nonce}]: <>\n\n").unwrap();
    Ok(Some((marker, storage)))
}

type PlannedFragment = (
    Range<usize>,
    fragment::Fragment,
    Option<details::Marker>,
    bool,
);

fn overlaps_fragments(fragments: &[PlannedFragment], span: &Range<usize>) -> bool {
    let index = fragments.partition_point(|(block, _, _, _)| block.end <= span.start);
    fragments
        .iter()
        .skip(index)
        .take_while(|(block, _, _, _)| block.start < span.end)
        .any(|(block, _, _, _)| !block.is_empty())
}

fn sort_fragments(
    fragments: &mut [PlannedFragment],
    op: &mut impl ProjectionWork,
) -> Result<(), CoreError> {
    let rows = fragments.len() as u64;
    let mut comparisons = rows * (u64::from(rows.checked_ilog2().unwrap_or(0)) + 1);
    while comparisons != 0 {
        let chunk = comparisons.min(256);
        op.events(chunk)?;
        comparisons -= chunk;
    }
    // A generated line prefix must precede an inline rewrite at the same byte.
    fragments.sort_unstable_by_key(|(span, _, _, _)| (span.start, span.end));
    Ok(())
}

#[derive(Debug)]
struct Segment {
    original: Range<usize>,
    preview: Range<usize>,
    exact: bool,
}

struct Destination {
    link_start: usize,
    payload: Option<PayloadId>,
    payload_length: u64,
    segment: Segment,
}

pub(crate) struct HtmlAnchor {
    pub name: String,
    pub source: u64,
    pub position: u64,
    pub legacy: bool,
    pub node: Option<NodeId>,
    kind: fragment::AnchorKind,
}

impl HtmlAnchor {
    pub fn is_heading(&self) -> bool {
        self.kind == fragment::AnchorKind::Heading
    }

    fn block_kind(&self) -> Option<NodeKind> {
        match self.kind {
            fragment::AnchorKind::Heading => Some(NodeKind::Heading),
            fragment::AnchorKind::Rule => Some(NodeKind::ThematicBreak),
            fragment::AnchorKind::Table => Some(NodeKind::Table),
            _ => None,
        }
    }
}

pub(crate) struct HtmlProjection {
    pub original: SourceSnapshot,
    segments: Vec<Segment>,
    alignments: Vec<(Range<usize>, u32)>,
    images: Vec<(Range<usize>, fragment::ImageSize)>,
    // These are projected source bytes until finish_styles binds them to Main UTF-16.
    styles: Vec<(Range<u64>, u32)>,
    literal_regions: Vec<Range<u64>>,
    ruby: Vec<RubyDisplayUnit>,
    pictures: Vec<picture::Picture>,
    destinations: Vec<Destination>,
    // Projected source boundaries until the final model binds their Main UTF-16 positions.
    anchors: Vec<HtmlAnchor>,
    citation_anchors_start: usize,
    citation_regions: Vec<Range<u64>>,
    bibliography: Option<Range<u64>>,
    disclosures: Vec<details::Projected>,
    static_checkboxes: Vec<controls::StaticCheckbox>,
    static_checkbox_objects: Vec<ObjectId>,
    static_checkbox_flow: Option<FlowId>,
    tables: Vec<table::Table>,
    front_matter_table: Option<Range<usize>>,
    _storage: Lease,
}

#[derive(Clone, Copy)]
pub(crate) struct OriginMap<'a> {
    original: &'a SourceSnapshot,
    segments: &'a [Segment],
    scoped: bool,
    payload_length: u64,
    controls: &'a [controls::StaticCheckbox],
    bibliography: Option<&'a Range<u64>>,
}

pub(crate) struct ControlPoint {
    pub atom: AtomId,
    pub logical: LogicalSpan,
    pub chosen: u64,
    pub boundary: bool,
    pub source: SourceSpan,
}

impl HtmlProjection {
    pub fn build(
        facts: Option<&crate::parse::ProjectionFacts>,
        config: crate::config::ParseConfig,
        original: &SourceSnapshot,
        front_matter: Option<front_matter::Prepared>,
        op: &mut impl ProjectionWork,
    ) -> Result<Option<(Self, String, Lease)>, CoreError> {
        op.ensure_budget(original.budget())?;
        if facts.is_some_and(|facts| !facts.matches_original(original)) {
            return Err(op.fail(CoreError::InvalidProvenance));
        }
        let source = original.text();
        let tagfilter = config.profile() == crate::config::Profile::G3;
        let details_markers = if let Some(facts) = facts {
            details::scan(facts, source, tagfilter, op)?
        } else {
            None
        };
        let mut disclosure_count = details_markers.as_ref().map_or(0, |markers| markers.pairs);
        let mut next_marker = details_markers
            .iter()
            .flat_map(|markers| markers.rows.iter().copied())
            .peekable();
        let mut fragments = Vec::new();
        let mut leases = Vec::new();
        let mut total = 0;
        let mut parts = 0;
        let mut rendered_bytes = source.len();
        let mut parser_storage = None;
        let mut barrier = None;
        let mut front_table_source = None;
        let mut static_checkbox_count = 0;
        let mut ruby_count = 0;
        let mut picture_count = 0;
        let mut picture_bytes = 0;
        let mut table_count = 0;
        let mut table_bytes = 0usize;
        let mut anchor_count = 0;
        let mut anchor_bytes = 0usize;
        let mut fragment_disclosures = 0usize;
        let mut references = Vec::new();
        if let Some(front) = front_matter {
            let mut front = front;
            front.fragment.suppress_email_autolinks = true;
            references = front.references;
            if front.fragment.markdown.is_empty() {
                let Some(marker) = empty_barrier(source, op)? else {
                    return Ok(None);
                };
                barrier = Some(marker);
            }
            parts = front.fragment.parts.len() * 2 + 4;
            let leading = if front.fragment.markdown.is_empty() {
                barrier.as_ref().unwrap().0.len()
            } else {
                2
            };
            rendered_bytes =
                rendered_bytes + front.fragment.markdown.len() + leading + 2 - front.span.len();
            if front.table {
                front_table_source = Some(front.span.clone());
            }
            fragments.push((front.span, front.fragment, None, false));
            leases.push(front.storage);
        }
        for node in facts.map_or(&[][..], |facts| facts.blocks()) {
            op.events(1)?;
            let inline_paragraph = node.kind == NodeKind::Paragraph;
            let inline_html_region = node.kind == NodeKind::HtmlInline;
            let span = node.source.start() as usize..node.source.end() as usize;
            if inline_html_region
                && fragments
                    .last()
                    .is_some_and(|(outer, _, _, _)| outer.end > span.start)
            {
                continue;
            }
            let marker = next_marker.next_if(|marker| marker.at == span.start);
            let Some(raw) = source.get(span.clone()) else {
                return Err(op.fail(CoreError::InvalidProvenance));
            };
            if raw.len() > fragment::MAX_FRAGMENT_BYTES {
                continue;
            }
            // A standalone label or named anchor can be a CommonMark paragraph.
            let mut anchor_paragraph = false;
            if inline_paragraph {
                let trimmed = raw.trim_start_matches([' ', '\t', '\r', '\n', '\x0c']);
                let prefix = trimmed.as_bytes();
                let Some((name, closing, _)) =
                    crate::ir::inline_tag_prefix(&prefix[..prefix.len().min(7)])
                else {
                    continue;
                };
                anchor_paragraph = name.eq_ignore_ascii_case(b"a");
                if closing || (!anchor_paragraph && !name.eq_ignore_ascii_case(b"label")) {
                    continue;
                }
            }
            total += raw.len();
            if total > MAX_HTML_BYTES || fragments.len() >= MAX_FRAGMENTS {
                break;
            }
            op.scan(raw.len() as u64)?;
            // Bound the tokenizer, decoded text and source-map scratch before
            // calling the HTML parser. Scratch for a rejected block retires here.
            if parser_storage.is_none() {
                let Ok(storage) = op.reserve_optional(2 * 1024 * 1024) else {
                    break;
                };
                parser_storage = Some(storage);
            }
            let Ok(scratch) = op.reserve_optional(256 * raw.len() as u64) else {
                break;
            };
            let filter_tags =
                marker.is_none() && !inline_paragraph && !inline_html_region && tagfilter;
            let Some(filtered) = tag_filter::Filtered::new(raw, filter_tags) else {
                continue;
            };
            let rendering_input = filtered.text.as_ref();
            if filtered.changed() {
                op.scan(rendering_input.len() as u64)?;
            }
            let table_candidate =
                !inline_paragraph && !inline_html_region && table::candidate(rendering_input);
            if table_candidate && config.profile() == crate::config::Profile::C0 {
                continue;
            }
            let mut table_storage = None;
            let lowered = if inline_html_region {
                fragment::lower_inline(rendering_input)
                    .filter(|fragment| !fragment.ruby.is_empty() || !fragment.pictures.is_empty())
            } else if let Some(marker) = marker {
                match marker.kind {
                    details::MarkerKind::Open { summary_offset, .. } => {
                        details::lower_open(raw, summary_offset, tagfilter)
                    }
                    details::MarkerKind::Close | details::MarkerKind::Remove => {
                        Some(fragment::Fragment::default())
                    }
                }
            } else if table_candidate {
                match table::lower(rendering_input, op)? {
                    Some(prepared) => {
                        table_storage = Some(prepared.storage);
                        Some(prepared.fragment)
                    }
                    None => None,
                }
            } else {
                fragment::lower(rendering_input, filtered.changed())
            };
            // Tokenization is bounded to 64 KiB. Observe cancellation even
            // when unsupported markup otherwise takes the inert fallback.
            op.checkpoint_now()?;
            let Some(mut lowered) = lowered else {
                continue;
            };
            lowered.suppress_email_autolinks = true;
            if filtered.restore(&mut lowered).is_none() {
                continue;
            }
            if inline_paragraph {
                if anchor_paragraph {
                    if lowered.anchors.is_empty() || !lowered.markdown.is_empty() {
                        continue;
                    }
                } else if lowered.static_checkboxes.len() != 1 {
                    continue;
                }
            }
            if !lowered.static_checkboxes.is_empty()
                && config.profile() == crate::config::Profile::C0
            {
                continue;
            }
            if lowered.markdown.is_empty() && barrier.is_none() {
                let Some(marker) = empty_barrier(source, op)? else {
                    break;
                };
                barrier = Some(marker);
            }
            let count = lowered.parts.len() * 2 + 4;
            if parts + count >= MAX_SEGMENTS {
                break;
            }
            if disclosure_count + fragment_disclosures + lowered.disclosures.len()
                > details::MAX_DISCLOSURES
            {
                continue;
            }
            if ruby_count + lowered.ruby.len() > MAX_RUBY_UNITS {
                continue;
            }
            if picture_count + lowered.pictures.len() > picture::MAX_PICTURES {
                continue;
            }
            picture_count += lowered.pictures.len();
            picture_bytes += lowered
                .pictures
                .iter()
                .map(picture::Picture::storage_bytes)
                .sum::<usize>();
            ruby_count += lowered.ruby.len();
            fragment_disclosures += lowered.disclosures.len();
            parts += count;
            let leading = if inline_html_region {
                0
            } else if lowered.markdown.is_empty() {
                barrier.as_ref().unwrap().0.len()
            } else {
                2
            };
            let trailing = if inline_html_region { 0 } else { 2 };
            rendered_bytes =
                rendered_bytes + lowered.markdown.len() + leading + trailing - raw.len();
            static_checkbox_count += lowered.static_checkboxes.len();
            anchor_count += lowered.anchors.len();
            for anchor in &lowered.anchors {
                op.events(1)?;
                anchor_bytes = anchor_bytes
                    .checked_add(anchor.name.capacity())
                    .ok_or_else(|| op.fail(CoreError::ArithmeticOverflow))?;
            }
            if let Some(table) = &lowered.html_table {
                table_count += 1;
                table_bytes = table_bytes
                    .checked_add(table.retained_bytes())
                    .ok_or_else(|| op.fail(CoreError::ArithmeticOverflow))?;
            }
            fragments.push((span, lowered, marker, inline_html_region));
            if let Some(storage) = table_storage {
                leases.push(storage);
            }
            leases.push(scratch);
        }
        // A pair is optional as a whole. Rejected summaries and exhausted
        // fragment quotas must leave both authored markers inert, without
        // discarding unrelated HTML or front-matter projections.
        if let Some(markers) = &details_markers {
            let mut missing = [0u8; details::MAX_DISCLOSURES];
            for marker in &markers.rows {
                op.events(1)?;
                missing[marker.pair] += 1;
            }
            for (_, _, marker, _) in &fragments {
                if let Some(marker) = marker {
                    op.events(1)?;
                    missing[marker.pair] -= 1;
                }
            }
            let mut pair_ids = [0usize; details::MAX_DISCLOSURES];
            let mut complete = 0;
            for pair in 0..disclosure_count {
                if missing[pair] == 0 {
                    pair_ids[pair] = complete;
                    complete += 1;
                }
            }
            fragments.retain_mut(|(span, fragment, marker, _)| {
                let Some(marker) = marker else { return true };
                if missing[marker.pair] == 0 {
                    marker.pair = pair_ids[marker.pair];
                    return true;
                }
                let leading = if fragment.markdown.is_empty() {
                    barrier.as_ref().unwrap().0.len()
                } else {
                    2
                };
                rendered_bytes += span.len();
                rendered_bytes -= fragment.markdown.len() + leading + 2;
                parts -= fragment.parts.len() * 2 + 4;
                ruby_count -= fragment.ruby.len();
                picture_count -= fragment.pictures.len();
                picture_bytes -= fragment
                    .pictures
                    .iter()
                    .map(picture::Picture::storage_bytes)
                    .sum::<usize>();
                fragment_disclosures -= fragment.disclosures.len();
                static_checkbox_count -= fragment.static_checkboxes.len();
                anchor_count -= fragment.anchors.len();
                anchor_bytes -= fragment
                    .anchors
                    .iter()
                    .map(|anchor| anchor.name.capacity())
                    .sum::<usize>();
                if let Some(table) = &fragment.html_table {
                    table_count -= 1;
                    table_bytes -= table.retained_bytes();
                }
                false
            });
            disclosure_count = complete;
        }
        let citation_anchors_start = anchor_count;
        let mut citation_count = 0;
        if let Some(facts) = facts {
            if let Some(prepared) = directives::prepare(
                facts,
                source,
                |span| overlaps_fragments(&fragments, span),
                op,
            )? {
                let mut added_parts = 0;
                let mut added_bytes = 0isize;
                for (source, fragment) in &prepared.fragments {
                    op.events(1)?;
                    added_parts += fragment.parts.len() * 2 + 4;
                    added_bytes += fragment.markdown.len() as isize - source.len() as isize;
                }
                let bytes = rendered_bytes.saturating_add_signed(added_bytes);
                if parts + added_parts < MAX_SEGMENTS
                    && bytes as u64 <= original.budget().limits().max_source_bytes
                {
                    parts += added_parts;
                    rendered_bytes = bytes;
                    fragments.extend(
                        prepared
                            .fragments
                            .into_iter()
                            .map(|(span, fragment)| (span, fragment, None, true)),
                    );
                    sort_fragments(&mut fragments, op)?;
                    leases.push(prepared.storage);
                }
            }
            let overlaps_html = |span: &Range<usize>| overlaps_fragments(&fragments, span);
            if let Some(prepared) =
                citations::prepare(facts, source, &references, overlaps_html, op)?
            {
                let count = prepared.inlines.len() * 6 + 6;
                let inline_bytes = prepared
                    .inlines
                    .iter()
                    .map(|(span, fragment)| fragment.markdown.len() as isize - span.len() as isize)
                    .sum::<isize>();
                let bytes = rendered_bytes.saturating_add_signed(inline_bytes)
                    + prepared.bibliography.markdown.len()
                    + 4;
                if parts + count < MAX_SEGMENTS
                    && bytes as u64 <= original.budget().limits().max_source_bytes
                {
                    parts += count;
                    citation_count = prepared.inlines.len();
                    rendered_bytes = bytes;
                    fragments.extend(
                        prepared
                            .inlines
                            .into_iter()
                            .map(|(span, fragment)| (span, fragment, None, true)),
                    );
                    sort_fragments(&mut fragments, op)?;
                    anchor_count += prepared.bibliography.anchors.len();
                    anchor_bytes += prepared
                        .bibliography
                        .anchors
                        .iter()
                        .map(|anchor| anchor.name.capacity())
                        .sum::<usize>();
                    fragments.push((
                        source.len()..source.len(),
                        prepared.bibliography,
                        None,
                        false,
                    ));
                    leases.push(prepared.storage);
                }
            }
        }
        if fragments.is_empty() {
            return Ok(None);
        }
        // Reuse the source admission limit for the projected source too.
        if rendered_bytes as u64 > original.budget().limits().max_source_bytes {
            return Ok(None);
        }

        let mut style_count = 0usize;
        let mut literal_region_count = 0usize;
        for (_, fragment, _, _) in &fragments {
            op.events(1)?;
            style_count = style_count
                .checked_add(fragment.styles.len())
                .ok_or_else(|| op.fail(CoreError::ArithmeticOverflow))?;
            literal_region_count +=
                usize::from(fragment.suppress_email_autolinks && !fragment.markdown.is_empty());
        }
        let segment_bytes = (parts + 1) * size_of::<Segment>();
        let decoration_bytes = parts
            * (size_of::<(Range<usize>, fragment::ImageSize)>()
                + size_of::<(Range<usize>, u32)>()
                + size_of::<Destination>())
            + style_count * size_of::<(Range<u64>, u32)>()
            + literal_region_count * size_of::<Range<u64>>()
            + static_checkbox_count
                * (size_of::<controls::StaticCheckbox>() + size_of::<ObjectId>())
            + ruby_count * size_of::<RubyDisplayUnit>()
            + picture_bytes
            + table_count * size_of::<table::Table>()
            + table_bytes
            + citation_count * size_of::<Range<u64>>()
            + anchor_count * size_of::<HtmlAnchor>()
            + anchor_bytes
            + (disclosure_count + fragment_disclosures) * size_of::<details::Projected>();
        drop(parser_storage);
        let Ok(storage) = op.reserve_optional((segment_bytes + decoration_bytes) as u64) else {
            return Ok(None);
        };
        let Ok(text_storage) = op.reserve_optional(rendered_bytes as u64) else {
            return Ok(None);
        };
        let mut builder = ProjectionBuilder {
            original: source,
            preview: String::with_capacity(rendered_bytes),
            segments: Vec::with_capacity(parts + 1),
            cursor: 0,
        };
        let mut alignments = Vec::with_capacity(parts);
        let mut images = Vec::with_capacity(parts);
        let mut styles = Vec::with_capacity(style_count);
        let mut literal_regions = Vec::with_capacity(literal_region_count);
        let mut ruby = Vec::with_capacity(ruby_count);
        let mut pictures = Vec::with_capacity(picture_count);
        let mut destinations = Vec::with_capacity(parts);
        let mut anchors = Vec::with_capacity(anchor_count);
        let mut citation_regions = Vec::with_capacity(citation_count);
        let mut bibliography = None;
        let mut disclosures = Vec::with_capacity(disclosure_count + fragment_disclosures);
        disclosures.resize(disclosure_count, details::Projected::default());
        let mut static_checkboxes = Vec::with_capacity(static_checkbox_count);
        let mut tables = Vec::with_capacity(table_count);
        let mut front_matter_table = None;
        for (span, fragment, marker, inline_html_region) in fragments {
            op.checkpoint_now()?;
            builder.copy_to(span.start, op)?;
            let start = builder.preview.len();
            if !inline_html_region {
                builder.emit(
                    span.start..span.start,
                    if fragment.markdown.is_empty() {
                        &barrier.as_ref().unwrap().0
                    } else {
                        "\n\n"
                    },
                    op,
                )?;
            }
            let offset = builder.preview.len();
            let output = offset as u64..(offset + fragment.markdown.len()) as u64;
            if fragment.suppress_email_autolinks && !fragment.markdown.is_empty() {
                literal_regions.push(output.clone());
            }
            if fragment.citation {
                citation_regions.push(output);
            } else if span.is_empty()
                && !fragment.anchors.is_empty()
                && !fragment.markdown.is_empty()
            {
                bibliography = Some(output);
            }
            if let Some(marker) = marker {
                let disclosure = &mut disclosures[marker.pair];
                match marker.kind {
                    details::MarkerKind::Open {
                        initially_expanded, ..
                    } => {
                        disclosure.summary =
                            Some((offset, offset + fragment.markdown.trim_ascii_end().len()));
                        disclosure.initially_expanded = initially_expanded;
                    }
                    details::MarkerKind::Close => disclosure.close = Some(start),
                    details::MarkerKind::Remove => {}
                }
            }
            disclosures.extend(fragment.disclosures.iter().map(|row| {
                details::Projected {
                    summary: row
                        .summary
                        .map(|(begin, end)| (offset + begin, offset + end)),
                    close: row.close.map(|close| offset + close),
                    initially_expanded: row.initially_expanded,
                }
            }));
            if front_table_source.as_ref() == Some(&span) {
                front_matter_table = Some(offset..offset + fragment.markdown.len());
            }
            for part in fragment.parts {
                op.events(1)?;
                let original = span.start + part.source.start..span.start + part.source.end;
                if original.start < builder.cursor || original.end > span.end {
                    return Err(op.fail(CoreError::InvalidProvenance));
                }
                if builder.cursor < original.start {
                    builder.emit(builder.cursor..original.start, "", op)?;
                }
                builder.emit(original, &fragment.markdown[part.output], op)?;
            }
            if builder.cursor < span.end {
                builder.emit(builder.cursor..span.end, "", op)?;
            }
            if !inline_html_region {
                builder.emit(span.end..span.end, "\n\n", op)?;
            }
            alignments.extend(
                fragment
                    .alignments
                    .into_iter()
                    .map(|(range, value)| (offset + range.start..offset + range.end, value)),
            );
            images.extend(
                fragment
                    .images
                    .into_iter()
                    .map(|(range, value)| (offset + range.start..offset + range.end, value)),
            );
            ruby.extend(
                fragment
                    .ruby
                    .into_iter()
                    .map(|unit| unit.offset(offset as u64)),
            );
            pictures.extend(fragment.pictures.into_iter().map(|mut picture| {
                picture.offset(span.start, offset);
                picture
            }));
            styles.extend(fragment.styles.into_iter().map(|(range, value)| {
                (
                    offset as u64 + range.start..offset as u64 + range.end,
                    value,
                )
            }));
            destinations.extend(
                fragment
                    .destinations
                    .into_iter()
                    .map(|destination| Destination {
                        link_start: offset + destination.link_start,
                        payload: None,
                        payload_length: 0,
                        segment: Segment {
                            original: span.start + destination.source.start
                                ..span.start + destination.source.end,
                            preview: offset + destination.output.start
                                ..offset + destination.output.end,
                            exact: destination.exact,
                        },
                    }),
            );
            for anchor in fragment.anchors {
                op.events(1)?;
                let position = (offset + anchor.offset) as u64;
                anchors.push(HtmlAnchor {
                    name: anchor.name,
                    source: position,
                    position,
                    legacy: anchor.legacy,
                    node: None,
                    kind: anchor.kind,
                });
            }
            static_checkboxes.extend(fragment.static_checkboxes.into_iter().map(|control| {
                controls::StaticCheckbox {
                    marker: offset + control.marker.start..offset + control.marker.end,
                    source: span.start + control.source.start..span.start + control.source.end,
                    checked: control.checked,
                    object: None,
                    atom: None,
                    logical: None,
                }
            }));
            if let Some(mut table) = fragment.html_table {
                table.offset(span.start, offset);
                tables.push(table);
            }
        }
        builder.copy_to(source.len(), op)?;
        if disclosures
            .iter()
            .any(|region| region.summary.is_none() || region.close.is_none())
        {
            return Ok(None);
        }
        if !disclosures.is_empty() {
            charge_events(
                op,
                disclosures.len() as u64 * (disclosures.len().ilog2() as u64 + 1),
            )?;
        }
        disclosures.sort_unstable_by_key(|region| region.summary);
        drop(leases);
        Ok(Some((
            Self {
                original: original.clone(),
                segments: builder.segments,
                alignments,
                images,
                styles,
                literal_regions,
                ruby,
                pictures,
                destinations,
                anchors,
                citation_anchors_start,
                citation_regions,
                bibliography,
                disclosures,
                static_checkboxes,
                static_checkbox_objects: Vec::with_capacity(static_checkbox_count),
                static_checkbox_flow: None,
                tables,
                front_matter_table,
                _storage: storage,
            },
            // Keep the buffer's reservation through the caller's source copy.
            builder.preview,
            text_storage,
        )))
    }

    pub fn is_front_matter_table(&self, span: SourceSpan) -> bool {
        self.front_matter_table.as_ref().is_some_and(|range| {
            range.start as u64 <= span.start() && span.end() <= range.end as u64
        })
    }

    pub(crate) fn email_autolink_allowed(
        &self,
        span: SourceSpan,
        work: &mut crate::policy::OwnedWork<'_, '_>,
    ) -> Result<bool, CoreError> {
        let mut lo = 0;
        let mut hi = self.literal_regions.len();
        while lo < hi {
            work.events(1)?;
            let middle = lo + (hi - lo) / 2;
            let range = &self.literal_regions[middle];
            if range.end <= span.start() {
                lo = middle + 1;
            } else {
                hi = middle;
            }
        }
        Ok(!self
            .literal_regions
            .get(lo)
            .is_some_and(|range| range.start < span.end() && span.start() < range.end))
    }

    pub(crate) fn bind_disclosures(
        &self,
        model: &AcceptedModel,
        op: &mut Operation,
    ) -> Result<Option<crate::memory::FrozenRows<crate::model::DisclosureRegion>>, CoreError> {
        if self.disclosures.is_empty() {
            return Ok(None);
        }
        use crate::error::LimitKind;
        use crate::memory::BuildGrowingVec;
        crate::policy::construction_scope(op, |mut work| {
            let maximum = work.require_model_limits()?.model_records;
            let mut rows = BuildGrowingVec::new(maximum, LimitKind::ModelRecords, &mut work)?;
            let mut implicit = model.disclosure_regions().iter().copied().peekable();
            let ir = model.ir();
            let root = ir.flows()[model.main_flow_id().index()].root;
            let mut node = ir.nodes()[root.index()].first_child;
            for projected in &self.disclosures {
                let Some(region) = projected.bind(model, &mut node, &mut work)? else {
                    continue;
                };
                while let Some(prior) = implicit.next_if(|prior| prior.node < region.node) {
                    rows.push(prior, &mut work)?;
                }
                work.admit_model(LimitKind::ModelRecords, 1)?;
                rows.push(region, &mut work)?;
            }
            for prior in implicit {
                rows.push(prior, &mut work)?;
            }
            let rows = if rows.len() == 0 {
                None
            } else {
                Some(rows.freeze(&mut work)?)
            };
            work.finish()?;
            Ok(rows)
        })
    }

    pub fn bind_destinations(
        &mut self,
        model: &AcceptedModel,
        op: &mut Operation,
    ) -> Result<(), CoreError> {
        for node in model.ir().nodes() {
            op.events(1)?;
            if node.kind != NodeKind::Link {
                continue;
            }
            let Some(detail) = node.detail else {
                return Err(op.fail(CoreError::InvalidProvenance));
            };
            let DetailRow::Link { destination, .. } = model.ir().details()[detail.index()] else {
                return Err(op.fail(CoreError::InvalidProvenance));
            };
            self.bind_destination(
                node.source_envelope.start() as usize,
                destination,
                model,
                op,
            )?;
        }
        for object in model.ir().objects() {
            op.events(1)?;
            if let ObjectRow::Image {
                node, destination, ..
            } = *object
            {
                self.bind_destination(
                    model.ir().nodes()[node.index()].source_envelope.start() as usize,
                    destination,
                    model,
                    op,
                )?;
            }
        }
        self.destinations
            .retain(|destination| destination.payload.is_some());
        let comparisons = u64::from(self.destinations.len().checked_ilog2().unwrap_or(0)) + 1;
        charge_events(op, self.destinations.len() as u64 * comparisons)?;
        self.destinations
            .sort_unstable_by_key(|destination| destination.payload);
        op.checkpoint_now()?;
        Ok(())
    }

    fn bind_destination(
        &mut self,
        start: usize,
        payload: PayloadId,
        model: &AcceptedModel,
        op: &mut Operation,
    ) -> Result<(), CoreError> {
        op.events(u64::from(self.destinations.len().checked_ilog2().unwrap_or(0)) + 1)?;
        if let Ok(index) = self
            .destinations
            .binary_search_by_key(&start, |row| row.link_start)
        {
            let row = &mut self.destinations[index];
            if row.payload.is_some() {
                return Err(op.fail(CoreError::InvalidProvenance));
            }
            row.payload = Some(payload);
            row.payload_length = model.ir().payloads()[payload.index()].utf8_len;
        }
        Ok(())
    }

    pub fn mapping(
        &self,
        target: TextTarget,
        op: &mut Operation,
    ) -> Result<OriginMap<'_>, CoreError> {
        if !self.static_checkboxes.is_empty() {
            op.events(u64::from(self.static_checkboxes.len().checked_ilog2().unwrap_or(0)) + 1)?;
        }
        let destination = if let TextTarget::Payload(payload) = target {
            op.events(u64::from(self.destinations.len().checked_ilog2().unwrap_or(0)) + 1)?;
            self.destinations
                .binary_search_by_key(&Some(payload), |destination| destination.payload)
                .ok()
                .map(|index| &self.destinations[index])
        } else {
            None
        };
        Ok(OriginMap {
            original: &self.original,
            segments: destination.map_or(&self.segments, |destination| {
                std::slice::from_ref(&destination.segment)
            }),
            scoped: destination.is_some(),
            payload_length: destination.map_or(0, |destination| destination.payload_length),
            controls: if self.static_checkbox_flow.map(TextTarget::Flow) == Some(target) {
                &self.static_checkboxes
            } else {
                &[]
            },
            bibliography: destination
                .is_none()
                .then_some(self.bibliography.as_ref())
                .flatten(),
        })
    }

    pub fn from_preview(&self, byte: u64, affinity: Affinity) -> u64 {
        OriginMap {
            original: &self.original,
            segments: &self.segments,
            scoped: false,
            payload_length: 0,
            controls: &[],
            bibliography: self.bibliography.as_ref(),
        }
        .from_preview(byte, affinity)
    }

    pub fn finish_styles(
        &mut self,
        model: &AcceptedModel,
        op: &mut Operation,
    ) -> Result<(), CoreError> {
        if self.styles.is_empty() {
            return Ok(());
        }
        let target = TextTarget::Flow(model.main_flow_id());
        let point = |offset, affinity| PointQuery {
            model: model.identity(),
            input: PointInput::Source {
                target,
                byte: SourceBoundary::new(offset),
            },
            affinity,
        };
        let limits = op
            .budget()
            .model_limits()
            .copied()
            .ok_or_else(|| op.fail(CoreError::UnsupportedConfiguration))?;
        let chunk_size = (limits.query_requests / 2).min(128) as usize;
        if chunk_size == 0 {
            self.styles.clear();
            return Ok(());
        }
        let mut queries = [point(0, Affinity::After); 256];
        let mut previous_end = 0;
        for styles in self.styles.chunks_mut(chunk_size) {
            for ((span, _), queries) in styles.iter().zip(queries.chunks_exact_mut(2)) {
                queries[0] = point(span.start, Affinity::After);
                queries[1] = point(span.end, Affinity::Before);
            }
            let response = model.map_points(&queries[..styles.len() * 2], op)?;
            for ((span, _), pair) in styles.iter_mut().zip(response.results().chunks_exact(2)) {
                let (Position::Flow(start), Position::Flow(end)) = (pair[0].chosen, pair[1].chosen)
                else {
                    return Err(op.fail(CoreError::InternalInvariant));
                };
                let start = start.get().max(previous_end);
                let end = end.get().max(start);
                *span = start..end;
                previous_end = end;
            }
        }
        self.styles.retain(|(span, _)| !span.is_empty());
        Ok(())
    }

    pub fn finish_ruby(
        &mut self,
        model: &AcceptedModel,
        op: &mut Operation,
    ) -> Result<(), CoreError> {
        if self.ruby.is_empty() {
            return Ok(());
        }
        let target = TextTarget::Flow(model.main_flow_id());
        let point = |offset, affinity| PointQuery {
            model: model.identity(),
            input: PointInput::Source {
                target,
                byte: SourceBoundary::new(offset),
            },
            affinity,
        };
        let limits = op
            .budget()
            .model_limits()
            .copied()
            .ok_or_else(|| op.fail(CoreError::UnsupportedConfiguration))?;
        let chunk_size = (limits.query_requests / 3).min(64) as usize;
        if chunk_size == 0 {
            return Err(op.fail(CoreError::UnsupportedConfiguration));
        }
        let mut queries = [point(0, Affinity::After); 192];
        for units in self.ruby.chunks_mut(chunk_size) {
            for (unit, points) in units.iter().zip(queries.chunks_exact_mut(3)) {
                points[0] = point(unit.start, Affinity::After);
                points[1] = point(unit.base_end, Affinity::After);
                points[2] = point(unit.end, Affinity::Before);
            }
            let mapped = model.map_points(&queries[..units.len() * 3], op)?;
            for (unit, points) in units.iter_mut().zip(mapped.results().chunks_exact(3)) {
                let mut boundary = |index: usize, byte, side| {
                    let point = points[index];
                    // At a relocated inline footnote seam the reading text and
                    // its reference share a source boundary. Prefer the text on
                    // the ruby side of the seam over the global extremum.
                    let contributors = point
                        .contributors
                        .slice(mapped.contributors())
                        .map_err(|error| op.fail(error))?;
                    for contributor in contributors {
                        op.events(1)?;
                        if !matches!(contributor.kind, AtomKind::Copy | AtomKind::Transform) {
                            continue;
                        }
                        match side {
                            Affinity::Before if contributor.source.end() == byte => {
                                return Ok(contributor.target.end);
                            }
                            Affinity::After if contributor.source.start() == byte => {
                                return Ok(contributor.target.start);
                            }
                            _ => {}
                        }
                    }
                    if let Position::Flow(boundary) = point.chosen {
                        Ok(boundary.get())
                    } else {
                        Err(op.fail(CoreError::InvalidProvenance))
                    }
                };
                let start = boundary(0, unit.start, Affinity::After)?;
                let base_end = boundary(1, unit.base_end, Affinity::After)?;
                let end = boundary(2, unit.end, Affinity::Before)?;
                if !(start == base_end && base_end == end || start < base_end && base_end < end) {
                    return Err(op.fail(CoreError::InvalidProvenance));
                }
                *unit = RubyDisplayUnit {
                    start,
                    base_end,
                    end,
                };
            }
        }

        // Unused notes can disappear and referenced notes can move to reading order.
        self.ruby.retain(|unit| unit.start < unit.end);
        if self.ruby.len() > 1 {
            let comparisons = u64::from(self.ruby.len().ilog2()) + 1;
            charge_events(op, self.ruby.len() as u64 * comparisons)?;
            self.ruby.sort_unstable_by_key(|unit| unit.start);
            if self.ruby.windows(2).any(|pair| pair[0].end > pair[1].start) {
                return Err(op.fail(CoreError::InvalidProvenance));
            }
        }
        Ok(())
    }

    pub fn ruby(&self, span: LogicalSpan) -> &[RubyDisplayUnit] {
        let first = self.ruby.partition_point(|unit| unit.end <= span.start());
        let end = self.ruby.partition_point(|unit| unit.start < span.end());
        &self.ruby[first..end]
    }

    pub fn styles(&self, span: LogicalSpan) -> &[(Range<u64>, u32)] {
        let first = self
            .styles
            .partition_point(|(range, _)| range.end <= span.start());
        let end = self
            .styles
            .partition_point(|(range, _)| range.start < span.end());
        &self.styles[first..end]
    }

    pub fn finish_anchors(
        &mut self,
        model: &AcceptedModel,
        op: &mut Operation,
    ) -> Result<(), CoreError> {
        if self.anchors.is_empty() {
            return Ok(());
        }
        let limit = op
            .budget()
            .model_limits()
            .copied()
            .ok_or_else(|| op.fail(CoreError::UnsupportedConfiguration))?
            .query_requests;
        let chunk_size = limit.min(128) as usize;
        if chunk_size == 0 {
            self.anchors.clear();
            self.citation_anchors_start = 0;
            return Ok(());
        }
        let point = |offset| PointQuery {
            model: model.identity(),
            input: PointInput::Source {
                target: TextTarget::Flow(model.main_flow_id()),
                byte: SourceBoundary::new(offset),
            },
            affinity: Affinity::After,
        };
        let mut queries = [point(0); 128];
        for anchors in self.anchors.chunks_mut(chunk_size) {
            for (anchor, query) in anchors.iter().zip(&mut queries) {
                *query = point(anchor.position);
            }
            let response = model.map_points(&queries[..anchors.len()], op)?;
            for (anchor, result) in anchors.iter_mut().zip(response.results()) {
                let Position::Flow(boundary) = result.chosen else {
                    return Err(op.fail(CoreError::InternalInvariant));
                };
                anchor.position = boundary.get();
            }
        }
        if self
            .anchors
            .iter()
            .any(|anchor| anchor.block_kind().is_some())
        {
            let depth = u64::from(self.anchors.len().ilog2()) + 1;
            for (index, node) in model.ir().nodes().iter().enumerate() {
                op.events(1)?;
                if !matches!(
                    node.kind,
                    NodeKind::Heading | NodeKind::ThematicBreak | NodeKind::Table
                ) {
                    continue;
                }
                op.events(depth)?;
                let start = self
                    .anchors
                    .partition_point(|anchor| anchor.source < node.source_envelope.start());
                for anchor in self.anchors.iter_mut().skip(start) {
                    op.events(1)?;
                    if anchor.source >= node.source_envelope.end() {
                        break;
                    }
                    if anchor.block_kind() == Some(node.kind) {
                        anchor.node =
                            Some(NodeId::from_index(index).map_err(|error| op.fail(error))?);
                    }
                }
            }
        }
        Ok(())
    }

    pub fn anchors(&self) -> &[HtmlAnchor] {
        &self.anchors[..self.citation_anchors_start]
    }

    pub fn citation_region(&self, span: SourceSpan, op: &mut Operation) -> Result<bool, CoreError> {
        if self.citation_regions.is_empty() {
            return Ok(false);
        }
        op.events(u64::from(self.citation_regions.len().ilog2()) + 1)?;
        let index = self
            .citation_regions
            .partition_point(|range| range.end <= span.start());
        Ok(self
            .citation_regions
            .get(index)
            .is_some_and(|range| range.start <= span.start() && span.end() <= range.end))
    }

    pub fn citation_destination(
        &self,
        destination: &str,
        op: &mut Operation,
    ) -> Result<Option<u64>, CoreError> {
        let Some(name) = destination.strip_prefix('#') else {
            return Ok(None);
        };
        let anchors = &self.anchors[self.citation_anchors_start..];
        op.events(u64::from(anchors.len().checked_ilog2().unwrap_or(0)) + 1)?;
        Ok(anchors
            .binary_search_by(|anchor| anchor.name.as_str().cmp(name))
            .ok()
            .map(|index| anchors[index].position))
    }
}

impl OriginMap<'_> {
    fn source_control(&self, byte: u64) -> Option<&controls::StaticCheckbox> {
        let index = self
            .controls
            .partition_point(|control| (control.source.end as u64) < byte);
        self.controls
            .get(index)
            .filter(|control| control.source.start as u64 <= byte)
    }

    pub fn control_lookup_work(&self) -> u64 {
        if self.controls.is_empty() {
            0
        } else {
            u64::from(self.controls.len().checked_ilog2().unwrap_or(0)) + 1
        }
    }

    pub fn control_end(&self, byte: u64) -> bool {
        self.source_control(byte)
            .is_some_and(|control| control.source.end as u64 == byte)
    }

    pub fn control_point(&self, byte: u64, affinity: Affinity) -> Option<ControlPoint> {
        if affinity == Affinity::After && self.control_end(byte) {
            return None;
        }
        let control = self.source_control(byte)?;
        let logical = control.logical?;
        let boundary = byte == control.source.start as u64 || byte == control.source.end as u64;
        let chosen = if byte == control.source.start as u64 {
            logical.start()
        } else if byte == control.source.end as u64 || affinity == Affinity::After {
            logical.end()
        } else {
            logical.start()
        };
        Some(ControlPoint {
            atom: control.atom?,
            logical,
            chosen,
            boundary,
            source: SourceSpan::new(control.source.start as u64, control.source.end as u64).ok()?,
        })
    }

    pub fn control_expansion(
        &self,
        atom: AtomId,
        source: SourceSpan,
        kind: AtomKind,
        input: RangeInput,
    ) -> Option<bool> {
        if kind != AtomKind::Object || source.is_empty() {
            return None;
        }
        let RangeInput::Source { span, .. } = input else {
            return None;
        };
        let index = self
            .controls
            .partition_point(|control| control.marker.end as u64 <= source.start());
        let control = self.controls.get(index)?;
        (control.atom == Some(atom)
            && control.marker.start as u64 <= source.start()
            && source.end() <= control.marker.end as u64)
            .then_some(
                span.start() > control.source.start as u64
                    || span.end() < control.source.end as u64,
            )
    }

    pub fn expansion(&self, input: RangeInput) -> Option<bool> {
        if !self.scoped || self.segments[0].exact {
            return None;
        }
        match input {
            RangeInput::Payload { span, .. } => {
                Some(span.start() != 0 || span.end() != self.payload_length)
            }
            RangeInput::Source { span, .. } => {
                let original = &self.segments[0].original;
                Some(span.start() != original.start as u64 || span.end() != original.end as u64)
            }
            RangeInput::Flow { .. } => None,
        }
    }

    pub fn to_preview(
        &self,
        boundary: SourceBoundary,
        affinity: Affinity,
    ) -> Result<SourceBoundary, CoreError> {
        let value = boundary.get();
        self.original
            .validate_span(SourceSpan::new(value, value)?)?;
        self.check_scope(value)?;
        Ok(SourceBoundary::new(self.map(value, affinity, true)))
    }

    pub fn point_query(&self, mut query: PointQuery) -> Result<PointQuery, CoreError> {
        if let PointInput::Source { target, byte } = query.input {
            let mut projected = self.to_preview(byte, query.affinity)?;
            if let Some(control) = self.source_control(byte.get())
                && !(query.affinity == Affinity::After && self.control_end(byte.get()))
            {
                projected = SourceBoundary::new(control.marker.start as u64 + 1);
                if byte.get() == control.source.start as u64 {
                    query.affinity = Affinity::Before;
                }
                if byte.get() == control.source.end as u64 {
                    query.affinity = Affinity::After;
                }
            }
            query.input = PointInput::Source {
                target,
                byte: projected,
            };
        }
        Ok(query)
    }

    pub fn source_effects(&self, offset: u64) -> Effects {
        let offset = offset as usize;
        let index = self
            .segments
            .partition_point(|segment| segment.original.end <= offset);
        if let Some(segment) = self.segments.get(index)
            && !segment.exact
            && segment.original.start < offset
        {
            return Effects {
                atomic: !segment.preview.is_empty(),
                hidden: segment.preview.is_empty(),
                ..Effects::default()
            };
        }
        Effects::default()
    }

    pub fn range_query(&self, mut query: RangeQuery) -> Result<RangeQuery, CoreError> {
        if let RangeInput::Source { target, span } = query.input {
            self.original.validate_span(span)?;
            self.check_scope(span.start())?;
            self.check_scope(span.end())?;
            let (start, end) = if span.is_empty() {
                let byte = if let Some(control) = self.source_control(span.start())
                    && !(query.empty_affinity == Affinity::After && self.control_end(span.start()))
                {
                    if span.start() == control.source.start as u64 {
                        query.empty_affinity = Affinity::Before;
                    }
                    if span.start() == control.source.end as u64 {
                        query.empty_affinity = Affinity::After;
                    }
                    control.marker.start as u64 + 1
                } else {
                    self.map(span.start(), query.empty_affinity, true)
                };
                (byte, byte)
            } else {
                (
                    self.source_control(span.start()).map_or_else(
                        || self.map(span.start(), Affinity::Before, true),
                        |control| {
                            if span.start() == control.source.end as u64 {
                                if self.control_end(span.start()) {
                                    self.map(span.start(), Affinity::After, true)
                                } else {
                                    control.marker.end as u64
                                }
                            } else {
                                control.marker.start as u64 + 1
                            }
                        },
                    ),
                    self.source_control(span.end()).map_or_else(
                        || self.map(span.end(), Affinity::After, true),
                        |control| {
                            if span.end() == control.source.start as u64 {
                                control.marker.start as u64
                            } else {
                                control.marker.start as u64 + 2
                            }
                        },
                    ),
                )
            };
            query.input = RangeInput::Source {
                target,
                span: SourceSpan::new(start, end)?,
            };
        }
        Ok(query)
    }

    pub fn original_span(&self, span: SourceSpan) -> Result<SourceSpan, CoreError> {
        SourceSpan::new(
            self.from_preview(span.start(), Affinity::Before),
            self.from_preview(span.end(), Affinity::After),
        )
    }

    /// Contributor spans describe content. At a collapsed tag boundary use
    /// the side inside the content, unlike a diagnostic envelope.
    pub fn original_content(
        &self,
        span: SourceSpan,
    ) -> Result<(SourceSpan, bool, bool), CoreError> {
        let at = |offset: u64| {
            let index = self
                .segments
                .partition_point(|segment| segment.preview.end <= offset as usize);
            self.segments.get(index)
        };
        let first = at(span.start());
        let last = at(span.end());
        let inside_rewrite = |segment: Option<&Segment>, offset: u64| {
            segment.is_some_and(|segment| !segment.exact && segment.preview.start < offset as usize)
        };
        let start = self.from_preview(
            span.start(),
            if inside_rewrite(first, span.start()) {
                Affinity::Before
            } else {
                Affinity::After
            },
        );
        let end = if span.is_empty() {
            start
        } else {
            self.from_preview(
                span.end(),
                if inside_rewrite(last, span.end()) {
                    Affinity::After
                } else {
                    Affinity::Before
                },
            )
        };
        let exact = first.is_some_and(|segment| {
            segment.exact
                && segment.preview.start as u64 <= span.start()
                && segment.preview.end as u64 >= span.end()
        });
        let expanded = inside_rewrite(first, span.start()) || inside_rewrite(last, span.end());
        Ok((SourceSpan::new(start, end)?, exact, expanded))
    }

    pub fn from_preview(&self, byte: u64, affinity: Affinity) -> u64 {
        self.map(byte, affinity, false)
    }

    pub fn bibliography_boundary(&self, byte: u64) -> bool {
        self.bibliography.is_some()
            && self.from_preview(byte, Affinity::Before) == self.original.text().len() as u64
            && self.from_preview(byte, Affinity::After) == self.original.text().len() as u64
    }

    pub fn synthetic_span(&self, span: SourceSpan) -> bool {
        self.bibliography
            .is_some_and(|range| range.start <= span.start() && span.end() <= range.end)
    }

    pub fn synthetic_point(&self, byte: u64, affinity: Affinity) -> bool {
        self.bibliography.is_some_and(|range| {
            range.start < byte && byte < range.end
                || byte == range.start && affinity == Affinity::After
                || byte == range.end && affinity == Affinity::Before
        })
    }

    fn map(&self, byte: u64, affinity: Affinity, to_preview: bool) -> u64 {
        let affinity = if to_preview
            && self.bibliography.is_some()
            && byte == self.original.text().len() as u64
        {
            Affinity::Before
        } else {
            affinity
        };
        let mut byte = byte as usize;
        fn ranges(segment: &Segment, to_preview: bool) -> (&Range<usize>, &Range<usize>) {
            if to_preview {
                (&segment.original, &segment.preview)
            } else {
                (&segment.preview, &segment.original)
            }
        }
        if self.scoped {
            let (source, _) = ranges(&self.segments[0], to_preview);
            byte = byte.clamp(source.start, source.end);
        }
        // At a shared boundary, use the adjacent segment on the selected side.
        // This also handles consecutive zero-length insertions and deletions.
        let index = self.segments.partition_point(|segment| {
            let (source, _) = ranges(segment, to_preview);
            source.end < byte || (source.end == byte && affinity == Affinity::After)
        });
        let Some(segment) = self.segments.get(index) else {
            let (_, target) = ranges(
                self.segments.last().expect("nonempty projection"),
                to_preview,
            );
            return target.end as u64;
        };
        let (source, target) = ranges(segment, to_preview);
        if segment.exact {
            (target.start + byte - source.start) as u64
        } else if byte == source.end && !source.is_empty()
            || byte != source.start && affinity == Affinity::After
        {
            target.end as u64
        } else {
            target.start as u64
        }
    }

    fn check_scope(&self, byte: u64) -> Result<(), CoreError> {
        if self.scoped {
            let span = &self.segments[0].original;
            if byte < span.start as u64 || byte > span.end as u64 {
                return Err(CoreError::OutsideScope);
            }
        }
        Ok(())
    }
}

impl HtmlProjection {
    pub fn alignment(&self, model: &AcceptedModel, node: NodeId) -> u32 {
        let start = model.ir().nodes()[node.index()].source_envelope.start() as usize;
        lookup(&self.alignments, start).unwrap_or_default()
    }

    pub fn picture_sources(&self, model: &AcceptedModel, node: NodeId) -> [Option<&str>; 2] {
        let start = model.ir().nodes()[node.index()].source_envelope.start() as usize;
        let index = self
            .pictures
            .partition_point(|picture| picture.image.end <= start);
        self.pictures
            .get(index)
            .filter(|picture| picture.image.contains(&start))
            .map_or([None, None], |picture| {
                [
                    picture
                        .light
                        .as_ref()
                        .map(|source| source.destination.as_str()),
                    picture
                        .dark
                        .as_ref()
                        .map(|source| source.destination.as_str()),
                ]
            })
    }

    pub fn image_size(&self, model: &AcceptedModel, node: NodeId) -> (u32, u32) {
        let start = model.ir().nodes()[node.index()].source_envelope.start() as usize;
        let size = lookup(&self.images, start).unwrap_or_default();
        (size.width, size.height)
    }
}

fn lookup<T: Copy>(values: &[(Range<usize>, T)], start: usize) -> Option<T> {
    let index = values.partition_point(|(span, _)| span.end <= start);
    values
        .get(index)
        .filter(|(span, _)| span.contains(&start))
        .map(|(_, value)| *value)
}

struct ProjectionBuilder<'a> {
    original: &'a str,
    preview: String,
    segments: Vec<Segment>,
    cursor: usize,
}

impl ProjectionBuilder<'_> {
    fn copy_to(&mut self, end: usize, op: &mut impl ProjectionWork) -> Result<(), CoreError> {
        op.checkpoint_now()?;
        if end > self.cursor {
            let span = self.cursor..end;
            self.emit(span.clone(), &self.original[span], op)?;
        }
        op.checkpoint_now()
    }

    fn emit(
        &mut self,
        original: Range<usize>,
        text: &str,
        op: &mut impl ProjectionWork,
    ) -> Result<(), CoreError> {
        const WINDOW: usize = 4096;
        op.checkpoint()?;
        let exact = match self.original.get(original.clone()) {
            Some(source) if source.len() == text.len() => {
                if source.as_ptr() == text.as_ptr() {
                    // copy_to already holds the identical immutable source slice.
                    true
                } else {
                    let mut equal = true;
                    for (left, right) in source
                        .as_bytes()
                        .chunks(WINDOW)
                        .zip(text.as_bytes().chunks(WINDOW))
                    {
                        op.scan((left.len() + right.len()) as u64)?;
                        if left != right {
                            equal = false;
                            break;
                        }
                    }
                    equal
                }
            }
            _ => false,
        };
        let start = self.preview.len();
        let mut cursor = 0;
        while cursor < text.len() {
            let mut end = (cursor + WINDOW).min(text.len());
            loop {
                op.scan(1)?;
                if text.is_char_boundary(end) {
                    break;
                }
                end -= 1;
            }
            op.copy((end - cursor) as u64)?;
            self.preview.push_str(&text[cursor..end]);
            cursor = end;
        }
        self.cursor = original.end;
        if exact
            && let Some(last) = self.segments.last_mut()
            && last.exact
        {
            last.original.end = original.end;
            last.preview.end = self.preview.len();
            return Ok(());
        }
        self.segments.push(Segment {
            exact,
            original,
            preview: start..self.preview.len(),
        });
        Ok(())
    }
}
