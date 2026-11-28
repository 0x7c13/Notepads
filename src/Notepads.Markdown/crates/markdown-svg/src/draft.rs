// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE file in the project root for license information.
// ---------------------------------------------------------------------------------------------
//! One same-cascade Text draft. Bidi resolves over the entire paragraph before
//! Native font coverage splitting. Actual SVG language and scope metadata only.
use crate::abi::{self, Failure, Result};
use crate::grapheme::{ResolvedContext, SourceIndex};
use crate::memory::{Context, failure_from_resource};
use markdown_svg_budget::{BudgetVec, PaidUtf8, ResourceKind, ResourcesRef};
use std::ops::Range;

#[cfg(test)]
#[path = "bidi_phase.rs"]
pub(crate) mod bidi_phase;

pub struct DraftOwner {
    pub text: PaidUtf8,
    pub styles: BudgetVec<abi::Style>,
    pub request_styles: BudgetVec<abi::Style>,
    pub source_index: SourceIndex,
    pub contexts: BudgetVec<ContextSlice>,
    pub axes: BudgetVec<abi::Axis>,
    pub spans: BudgetVec<abi::LogicalSpan>,
    pub strings: BudgetVec<u8>,
    pub chunks: BudgetVec<Range<usize>>,
    pub paragraphs: BudgetVec<Range<usize>>,
    pub id: u64,
    pub element_id_start: u32,
    pub element_id_length: u32,
}
pub struct ContextSlice {
    pub range: Range<usize>,
    pub parent: usize,
    pub resolved: ResolvedContext,
}
fn same_scope(a: &[usvg::BidiScope], b: &[usvg::BidiScope]) -> bool {
    a.len() == b.len()
        && a.iter().zip(b).all(|(a, b)| {
            a.identity == b.identity && a.mode == b.mode && a.direction == b.direction
        })
}
fn same_request(a: &abi::Style, b: &abi::Style, axes: &[abi::Axis], strings: &[u8]) -> bool {
    let bytes = |start: u32, len: u32| &strings[start as usize..(start + len) as usize];
    a.weight == b.weight
        && a.style == b.style
        && a.stretch == b.stretch
        && a.direction == b.direction
        && a.optical_sizing == b.optical_sizing
        && a.size.to_bits() == b.size.to_bits()
        && bytes(a.family_start, a.family_length) == bytes(b.family_start, b.family_length)
        && bytes(a.language_start, a.language_length) == bytes(b.language_start, b.language_length)
        && a.axis_count == b.axis_count
        && axes[a.axis_start as usize..(a.axis_start + a.axis_count) as usize]
            .iter()
            .zip(&axes[b.axis_start as usize..(b.axis_start + b.axis_count) as usize])
            .all(|(a, b)| a.tag_be == b.tag_be && a.value.to_bits() == b.value.to_bits())
}
struct StyleRef<'a> {
    span: &'a usvg::TextSpan,
    start: usize,
    end: usize,
}
fn control_open(
    text: &mut PaidUtf8,
    map: &mut BudgetVec<usize>,
    scope: &usvg::BidiScope,
    source: usize,
) -> Result<()> {
    let mut push = |ch: char| -> Result<()> {
        text.try_push(ch).map_err(failure_from_resource)?;
        for _ in 0..ch.len_utf8() {
            map.try_push(source).map_err(failure_from_resource)?;
        }
        Ok(())
    };
    match scope.mode {
        1 => push(if scope.direction == 1 {
            '\u{202B}'
        } else {
            '\u{202A}'
        })?,
        2 => push(if scope.direction == 1 {
            '\u{202E}'
        } else {
            '\u{202D}'
        })?,
        3 => push(if scope.direction == 1 {
            '\u{2067}'
        } else {
            '\u{2066}'
        })?,
        4 => {
            push(if scope.direction == 1 {
                '\u{2067}'
            } else {
                '\u{2066}'
            })?;
            push(if scope.direction == 1 {
                '\u{202E}'
            } else {
                '\u{202D}'
            })?;
        }
        5 => push('\u{2068}')?,
        _ => {}
    }
    Ok(())
}
fn control_close(
    text: &mut PaidUtf8,
    map: &mut BudgetVec<usize>,
    scope: &usvg::BidiScope,
    source: usize,
) -> Result<()> {
    let mut push = |ch: char| -> Result<()> {
        text.try_push(ch).map_err(failure_from_resource)?;
        for _ in 0..ch.len_utf8() {
            map.try_push(source).map_err(failure_from_resource)?;
        }
        Ok(())
    };
    match scope.mode {
        1 | 2 => push('\u{202C}')?,
        3 | 5 => push('\u{2069}')?,
        4 => {
            push('\u{202C}')?;
            push('\u{2069}')?;
        }
        _ => {}
    }
    Ok(())
}
fn checked_add(callbacks: &Context, a: usize, b: usize) -> Result<usize> {
    a.checked_add(b)
        .ok_or_else(|| callbacks.record_failure(Failure::Contract))
}
impl DraftOwner {
    pub fn build(
        callbacks: &Context,
        resources: &ResourcesRef,
        text: &usvg::Text,
        id: u64,
    ) -> Result<Self> {
        if id == 0 || text.id().len() > 4096 || text.id().contains('\0') {
            return Err(callbacks.record_failure(Failure::Contract));
        }
        let mut length = 0usize;
        let mut styles_count = 0usize;
        let mut axes_count = 0usize;
        let mut scope_count = 0usize;
        let mut strings_capacity = text.id().len();
        for chunk in text.chunks() {
            length = checked_add(callbacks, length, chunk.text().len())?;
            styles_count = checked_add(callbacks, styles_count, chunk.spans().len())?;
            for span in chunk.spans() {
                axes_count = checked_add(callbacks, axes_count, span.font().variations().len())?;
                scope_count = checked_add(callbacks, scope_count, span.bidi_scopes().len())?;
                if span.language().is_empty()
                    || span.language().len() > 255
                    || span.language().contains('\0')
                {
                    return Err(callbacks.record_failure(Failure::Contract));
                }
                let family_bytes = crate::family::encoded_len(callbacks, span.font().families())?;
                strings_capacity = checked_add(
                    callbacks,
                    strings_capacity,
                    checked_add(callbacks, family_bytes, span.language().len())?,
                )?;
            }
        }
        if length > 1048576
            || styles_count > 65535
            || axes_count > 4096
            || strings_capacity > 1048576
        {
            return Err(callbacks.record_failure(Failure::Contract));
        }
        let expansion = checked_add(
            callbacks,
            length,
            scope_count
                .checked_mul(12)
                .ok_or_else(|| callbacks.record_failure(Failure::Contract))?,
        )?;
        let mut source = PaidUtf8::new(resources.clone(), ResourceKind::Scratch);

        let mut refs = BudgetVec::new(resources.clone(), ResourceKind::Scratch);
        refs.try_reserve_exact(styles_count)
            .map_err(|_| callbacks.record_failure(Failure::Allocation))?;
        let mut chunks = BudgetVec::new(resources.clone(), ResourceKind::Scratch);
        chunks
            .try_reserve_exact(text.chunks().len())
            .map_err(|_| callbacks.record_failure(Failure::Allocation))?;
        let mut styles = BudgetVec::new(resources.clone(), ResourceKind::Scratch);
        styles
            .try_reserve_exact(styles_count)
            .map_err(|_| callbacks.record_failure(Failure::Allocation))?;
        let mut axes = BudgetVec::new(resources.clone(), ResourceKind::Scratch);
        axes.try_reserve_exact(axes_count)
            .map_err(|_| callbacks.record_failure(Failure::Allocation))?;
        let mut strings = BudgetVec::new(resources.clone(), ResourceKind::Scratch);
        strings
            .try_reserve_exact(strings_capacity)
            .map_err(|_| callbacks.record_failure(Failure::Allocation))?;
        let element_id_start = if text.id().is_empty() {
            0
        } else {
            strings
                .try_extend_from_slice(text.id().as_bytes())
                .map_err(failure_from_resource)?;
            0
        };
        let element_id_length = text.id().len() as u32;
        for chunk in text.chunks() {
            let base = source.len();
            source
                .try_push_str(chunk.text())
                .map_err(failure_from_resource)?;
            chunks
                .try_push(base..source.len())
                .map_err(failure_from_resource)?;
            for span in chunk.spans() {
                let start = base + span.start();
                let end = base + span.end();
                if start >= end
                    || !source.is_char_boundary(start)
                    || !source.is_char_boundary(end)
                    || refs.last().is_some_and(|last: &StyleRef| last.end != start)
                {
                    return Err(callbacks.record_failure(Failure::Contract));
                }
                let family_start = strings.len() as u32;
                crate::family::append(callbacks, span.font().families(), &mut strings)?;
                let family_length = strings.len() as u32 - family_start;
                let language_start = strings.len() as u32;
                strings
                    .try_extend_from_slice(span.language().as_bytes())
                    .map_err(failure_from_resource)?;
                let axis_start = axes.len() as u32;
                for variation in span.font().variations() {
                    if !variation.value.is_finite() {
                        return Err(callbacks.record_failure(Failure::Contract));
                    }
                    axes.try_push(abi::Axis {
                        tag_be: u32::from_be_bytes(variation.tag),
                        value: variation.value,
                    })
                    .map_err(failure_from_resource)?;
                }
                styles
                    .try_push(abi::Style {
                        utf8_start: start as u32,
                        utf8_length: (end - start) as u32,
                        family_start,
                        family_length,
                        language_start,
                        language_length: span.language().len() as u32,
                        weight: span.font().weight() as u32,
                        style: match span.font().style() {
                            usvg::FontStyle::Normal => 0,
                            usvg::FontStyle::Oblique => 1,
                            usvg::FontStyle::Italic => 2,
                        },
                        stretch: span.font().stretch() as u32 + 1,
                        direction: span.direction(),
                        axis_start,
                        axis_count: axes.len() as u32 - axis_start,
                        optical_sizing: u32::from(
                            span.font_optical_sizing() == usvg::FontOpticalSizing::Auto,
                        ),
                        size: span.font_size().get(),
                    })
                    .map_err(failure_from_resource)?;
                refs.try_push(StyleRef { span, start, end })
                    .map_err(failure_from_resource)?;
            }
        }
        if refs.first().is_some_and(|value| value.start != 0)
            || refs.last().map_or(length != 0, |value| value.end != length)
        {
            return Err(callbacks.record_failure(Failure::Contract));
        }
        let mut expanded = PaidUtf8::new(resources.clone(), ResourceKind::Scratch);

        let mut expanded_to_source = BudgetVec::new(resources.clone(), ResourceKind::Scratch);
        expanded_to_source
            .try_reserve_exact(expansion + 1)
            .map_err(|_| callbacks.record_failure(Failure::Allocation))?;
        expanded_to_source
            .try_push(0)
            .map_err(failure_from_resource)?;
        let mut source_to_expanded = BudgetVec::new(resources.clone(), ResourceKind::Scratch);
        source_to_expanded
            .try_reserve_exact(length + 1)
            .map_err(|_| callbacks.record_failure(Failure::Allocation))?;
        for _ in 0..length + 1 {
            source_to_expanded
                .try_push(0)
                .map_err(failure_from_resource)?;
        }
        let mut previous: &[usvg::BidiScope] = &[];
        for item in refs.iter() {
            let scopes = item.span.bidi_scopes();
            let common = previous
                .iter()
                .zip(scopes)
                .take_while(|(a, b)| {
                    a.identity == b.identity && a.mode == b.mode && a.direction == b.direction
                })
                .count();
            for scope in previous[common..].iter().rev() {
                control_close(&mut expanded, &mut expanded_to_source, scope, item.start)?;
            }
            for scope in &scopes[common..] {
                control_open(&mut expanded, &mut expanded_to_source, scope, item.start)?;
            }
            for (offset, ch) in source[item.start..item.end].char_indices() {
                let logical = item.start + offset;
                for byte in 0..ch.len_utf8() {
                    source_to_expanded[logical + byte] = expanded.len() + byte;
                }
                expanded.try_push(ch).map_err(failure_from_resource)?;
                for byte in 1..=ch.len_utf8() {
                    expanded_to_source
                        .try_push(logical + byte)
                        .map_err(failure_from_resource)?;
                }
                source_to_expanded[logical + ch.len_utf8()] = expanded.len();
            }
            previous = scopes;
        }
        for scope in previous.iter().rev() {
            control_close(&mut expanded, &mut expanded_to_source, scope, length)?;
        }
        if expanded.len() > expansion || expanded_to_source.len() != expanded.len() + 1 {
            return Err(callbacks.record_failure(Failure::Contract));
        }
        #[cfg(test)]
        let bidi_stage = bidi_phase::begin(bidi_phase::Kind::Draft, resources);
        let bidi = unicode_bidi::controlled::BidiInfo::try_new(
            &expanded,
            Some(if text.direction() == 1 {
                unicode_bidi::Level::rtl()
            } else {
                unicode_bidi::Level::ltr()
            }),
            resources,
        )
        .map_err(failure_from_resource)?;
        #[cfg(test)]
        bidi_stage.complete(bidi.levels().len());
        let mut paragraphs = BudgetVec::new(resources.clone(), ResourceKind::Scratch);
        for paragraph in bidi.paragraphs() {
            let start = expanded_to_source[paragraph.range.start];
            let end = expanded_to_source[paragraph.range.end];
            if start < end {
                paragraphs
                    .try_push(start..end)
                    .map_err(failure_from_resource)?;
            }
        }
        let mut work = crate::memory::WorkBatch::new(callbacks);
        let source_index = SourceIndex::build_batched(callbacks, &mut work, &source)?;
        let mut contexts = BudgetVec::new(resources.clone(), ResourceKind::Scratch);
        contexts
            .try_reserve_exact(source.chars().count())
            .map_err(|_| callbacks.record_failure(Failure::Allocation))?;
        let mut preferred = unicode_script::Script::Common;
        let mut previous_scopes: &[usvg::BidiScope] = &[];
        let mut ref_index = 0;
        let mut paragraph_index = 0;
        for (parent, bounds) in source_index.boundaries().windows(2).enumerate() {
            let mut start = bounds[0];
            while start < bounds[1] {
                work.charge(1)?;
                while refs[ref_index].end <= start {
                    ref_index += 1;
                }
                let scopes = refs[ref_index].span.bidi_scopes();
                let mut end = bounds[1];
                for next in &refs[ref_index + 1..] {
                    if next.start >= end {
                        break;
                    }
                    if !same_scope(scopes, next.span.bidi_scopes()) {
                        end = next.start;
                        break;
                    }
                }
                while paragraphs
                    .get(paragraph_index)
                    .is_some_and(|p| p.end <= start)
                {
                    paragraph_index += 1;
                }
                let paragraph_start = paragraphs
                    .get(paragraph_index)
                    .is_some_and(|p| p.start == start);
                let control=source[start..end].chars().any(|ch|matches!(ch,'\u{202a}'..='\u{202e}'|'\u{2066}'..='\u{2069}'|'\n'|'\r'|'\u{2029}'));
                if paragraph_start || !same_scope(previous_scopes, scopes) || control {
                    preferred = unicode_script::Script::Common;
                }
                let resolved = crate::grapheme::resolve_checked(
                    &source[start..end],
                    preferred,
                    text.writing_mode() == usvg::WritingMode::TopToBottom,
                    |n| work.charge(n),
                )
                .map_err(|error| crate::grapheme::record_semantic_failure(callbacks, error))?;
                preferred = if control {
                    unicode_script::Script::Common
                } else {
                    resolved.script
                };
                contexts
                    .try_push(ContextSlice {
                        range: start..end,
                        parent,
                        resolved,
                    })
                    .map_err(failure_from_resource)?;
                previous_scopes = scopes;
                start = end;
            }
        }
        let mut spans: BudgetVec<abi::LogicalSpan> =
            BudgetVec::new(resources.clone(), ResourceKind::Scratch);
        spans
            .try_reserve_exact(source.chars().count())
            .map_err(|_| callbacks.record_failure(Failure::Allocation))?;
        let mut style_index = 0;
        let mut context_index = 0;
        let mut paragraph_index = 0;
        for (start, ch) in source.char_indices() {
            work.charge(ch.len_utf8() as u64)?;
            while styles[style_index].utf8_start as usize + styles[style_index].utf8_length as usize
                <= start
            {
                style_index += 1;
            }
            while contexts[context_index].range.end <= start {
                context_index += 1;
            }
            let style = styles[style_index];
            let decision = contexts[context_index].resolved;
            let tag = decision.script.short_name().as_bytes();
            let script_be = u32::from_be_bytes(
                tag.try_into()
                    .map_err(|_| callbacks.record_failure(Failure::Contract))?,
            );
            let level = bidi.levels()[source_to_expanded[start]].number() as u32;
            let end = start + ch.len_utf8();
            while paragraphs
                .get(paragraph_index)
                .is_some_and(|p| p.end <= start)
            {
                paragraph_index += 1;
            }
            let paragraph_start = paragraphs
                .get(paragraph_index)
                .is_some_and(|p| p.start == start);
            let scope_start = style_index > 0
                && refs[style_index].start == start
                && !same_scope(
                    refs[style_index - 1].span.bidi_scopes(),
                    refs[style_index].span.bidi_scopes(),
                );
            if let Some(last) = spans.last_mut().filter(|last| {
                !paragraph_start
                    && !scope_start
                    && last.script_be == script_be
                    && last.bidi_level == level
                    && last.orientation == decision.orientation
                    && strings[last.language_start as usize
                        ..(last.language_start + last.language_length) as usize]
                        == strings[style.language_start as usize
                            ..(style.language_start + style.language_length) as usize]
                    && last.utf8_start as usize + last.utf8_length as usize == start
            }) {
                last.utf8_length += ch.len_utf8() as u32;
            } else {
                spans
                    .try_push(abi::LogicalSpan {
                        utf8_start: start as u32,
                        utf8_length: (end - start) as u32,
                        script_be,
                        language_start: style.language_start,
                        language_length: style.language_length,
                        bidi_level: level,
                        orientation: decision.orientation,
                    })
                    .map_err(failure_from_resource)?;
            }
        }
        let mut request_styles: BudgetVec<abi::Style> =
            BudgetVec::new(resources.clone(), ResourceKind::Scratch);
        request_styles
            .try_reserve_exact(styles.len())
            .map_err(|_| callbacks.record_failure(Failure::Allocation))?;
        for (i, style) in styles.iter().enumerate() {
            let at_chunk = chunks
                .binary_search_by_key(&(style.utf8_start as usize), |chunk| chunk.start)
                .is_ok();
            let merge = i > 0
                && !at_chunk
                && same_request(&styles[i - 1], style, &axes, &strings)
                && same_scope(refs[i - 1].span.bidi_scopes(), refs[i].span.bidi_scopes())
                && refs[i - 1].span.small_caps() == refs[i].span.small_caps()
                && refs[i - 1].span.apply_kerning() == refs[i].span.apply_kerning()
                && (refs[i - 1].span.letter_spacing() != 0.0)
                    == (refs[i].span.letter_spacing() != 0.0);
            if merge {
                request_styles
                    .last_mut()
                    .ok_or_else(|| callbacks.record_failure(Failure::Contract))?
                    .utf8_length += style.utf8_length;
            } else {
                request_styles
                    .try_push(*style)
                    .map_err(failure_from_resource)?;
            }
        }
        if spans.len() > 65535 {
            return Err(callbacks.record_failure(Failure::Contract));
        }
        // The constructor results are request owners. A release fault must
        // stop here before Native font planning receives this draft.
        drop(bidi);
        resources.checkpoint(0).map_err(failure_from_resource)?;
        Ok(Self {
            text: source,
            styles,
            request_styles,
            source_index,
            contexts,
            axes,
            spans,
            strings,
            chunks,
            paragraphs,
            id,
            element_id_start,
            element_id_length,
        })
    }
    pub fn raw(&self) -> abi::Draft {
        abi::Draft {
            size: size_of::<abi::Draft>() as u32,
            version: abi::ABI_VERSION,
            draft_id: self.id,
            text: self.text.as_ptr(),
            text_length: self.text.len() as u32,
            styles: self.request_styles.as_ptr(),
            style_count: self.request_styles.len() as u32,
            axes: self.axes.as_ptr(),
            axis_count: self.axes.len() as u32,
            logical_spans: self.spans.as_ptr(),
            logical_span_count: self.spans.len() as u32,
            strings: self.strings.as_ptr(),
            strings_length: self.strings.len() as u32,
            element_id_start: self.element_id_start,
            element_id_length: self.element_id_length,
        }
    }
}
