// Copyright 2019 the Resvg Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT
// Modifications Copyright (c) 2026, Jiaqi (0x7c13) Liu.

use super::resource_cache::DecorationCache;
use crate::tree::owner::{self, Origin, TreeString, TreeVec, TryCopy};
#[cfg(test)]
use std::sync::Arc;

use kurbo::{ParamCurve, ParamCurveArclen};
use svgtypes::{
    FontFamilyKind, FontFamilyParseError, FontFamilySink, Length, LengthUnit,
    parse_font_families_into_with_control,
};

use super::svgtree::{AId, EId, FromValue, SvgNode};
use super::{converter, style};
use crate::*;

impl<'a, 'input: 'a> FromValue<'a, 'input> for TextAnchor {
    fn parse(_: SvgNode, _: AId, value: &str) -> Option<Self> {
        match value {
            "start" => Some(TextAnchor::Start),
            "middle" => Some(TextAnchor::Middle),
            "end" => Some(TextAnchor::End),
            _ => None,
        }
    }
}

impl<'a, 'input: 'a> FromValue<'a, 'input> for AlignmentBaseline {
    fn parse(_: SvgNode, _: AId, value: &str) -> Option<Self> {
        match value {
            "auto" => Some(AlignmentBaseline::Auto),
            "baseline" => Some(AlignmentBaseline::Baseline),
            "before-edge" => Some(AlignmentBaseline::BeforeEdge),
            "text-before-edge" => Some(AlignmentBaseline::TextBeforeEdge),
            "middle" => Some(AlignmentBaseline::Middle),
            "central" => Some(AlignmentBaseline::Central),
            "after-edge" => Some(AlignmentBaseline::AfterEdge),
            "text-after-edge" => Some(AlignmentBaseline::TextAfterEdge),
            "ideographic" => Some(AlignmentBaseline::Ideographic),
            "alphabetic" => Some(AlignmentBaseline::Alphabetic),
            "hanging" => Some(AlignmentBaseline::Hanging),
            "mathematical" => Some(AlignmentBaseline::Mathematical),
            _ => None,
        }
    }
}

impl<'a, 'input: 'a> FromValue<'a, 'input> for DominantBaseline {
    fn parse(_: SvgNode, _: AId, value: &str) -> Option<Self> {
        match value {
            "auto" => Some(DominantBaseline::Auto),
            "use-script" => Some(DominantBaseline::UseScript),
            "no-change" => Some(DominantBaseline::NoChange),
            "reset-size" => Some(DominantBaseline::ResetSize),
            "ideographic" => Some(DominantBaseline::Ideographic),
            "alphabetic" => Some(DominantBaseline::Alphabetic),
            "hanging" => Some(DominantBaseline::Hanging),
            "mathematical" => Some(DominantBaseline::Mathematical),
            "central" => Some(DominantBaseline::Central),
            "middle" => Some(DominantBaseline::Middle),
            "text-after-edge" => Some(DominantBaseline::TextAfterEdge),
            "text-before-edge" => Some(DominantBaseline::TextBeforeEdge),
            _ => None,
        }
    }
}

impl<'a, 'input: 'a> FromValue<'a, 'input> for LengthAdjust {
    fn parse(_: SvgNode, _: AId, value: &str) -> Option<Self> {
        match value {
            "spacing" => Some(LengthAdjust::Spacing),
            "spacingAndGlyphs" => Some(LengthAdjust::SpacingAndGlyphs),
            _ => None,
        }
    }
}

impl<'a, 'input: 'a> FromValue<'a, 'input> for FontStyle {
    fn parse(_: SvgNode, _: AId, value: &str) -> Option<Self> {
        match value {
            "normal" => Some(FontStyle::Normal),
            "italic" => Some(FontStyle::Italic),
            "oblique" => Some(FontStyle::Oblique),
            _ => None,
        }
    }
}

/// A text character position.
///
/// _Character_ is a Unicode codepoint.
#[derive(Clone, Copy, Debug)]
struct CharacterPosition {
    /// An absolute X axis position.
    x: Option<f32>,
    /// An absolute Y axis position.
    y: Option<f32>,
    /// A relative X axis offset.
    dx: Option<f32>,
    /// A relative Y axis offset.
    dy: Option<f32>,
}

pub(crate) fn convert(
    text_node: SvgNode,
    state: &converter::State,
    cache: &mut converter::Cache,
    parent: &mut Group,
) {
    if let Some(owner) = cache.font_replay.as_ref() {
        let controlled = match (cache.processing.resources(), owner.request_resources()) {
            (Some(parser), Some(replay)) => {
                if !parser.is_same_request(&replay) {
                    parser.record(markdown_svg_budget::Error::new(
                        markdown_svg_budget::FailureKind::Invariant,
                        markdown_svg_budget::LimitKind::None,
                    ));
                    return;
                }
                true
            }
            _ => false,
        };
        // Both position/rotate lists below already use the same paid numeric
        // ListBuilder. Do not reserve an additional coarse allowance for them.
        if !controlled {
            let Some(count) = count_chars_checked(text_node) else {
                return;
            };
            let Some(bytes) = count.checked_mul(2 * size_of::<usize>()) else {
                cache.processing.fail(ProcessingFailure::FontReplay);
                return;
            };
            if owner.reserve_replay_bytes(bytes).is_err() {
                cache.processing.fail(ProcessingFailure::FontReplay);
                return;
            }
        }
    }
    let Some(pos_list) = resolve_positions_list(text_node, state) else {
        return;
    };
    let Some(rotate_list) = resolve_rotate_list(text_node) else {
        return;
    };
    let writing_mode = convert_writing_mode(text_node);

    let Some(chunks) = collect_text_chunks(text_node, &pos_list, state, cache) else {
        return;
    };
    if state.opt.processing.failed() {
        return;
    }

    let rendering_mode: TextRendering = text_node
        .find_attribute(AId::TextRendering)
        .unwrap_or(state.opt.text_rendering);

    // Nodes generated by markers must not have an ID. Otherwise we would have duplicates.
    let Ok(id) = TreeString::try_from_str(
        if state.parent_markers.is_empty() {
            text_node.element_id()
        } else {
            ""
        },
        &state.opt.processing,
    ) else {
        return;
    };

    let dummy = Rect::from_xywh(0.0, 0.0, 0.0, 0.0).unwrap();

    let mut text = Text {
        origin: Origin::new(&state.opt.processing),
        id,
        rendering_mode,
        dx: match freeze_position_values(&pos_list, state, |v| v.dx) {
            Some(v) => v,
            None => return,
        },
        dy: match freeze_position_values(&pos_list, state, |v| v.dy) {
            Some(v) => v,
            None => return,
        },
        rotate: rotate_list,
        writing_mode,
        direction: u32::from(text_node.find_attribute::<&str>(AId::Direction) == Some("rtl")),
        chunks,
        abs_transform: parent.abs_transform,
        // All fields below will be reset by `text_to_paths`.
        bounding_box: dummy,
        abs_bounding_box: dummy,
        stroke_bounding_box: dummy,
        abs_stroke_bounding_box: dummy,
        flattened: match crate::scene::boxed(
            &state.opt.processing,
            crate::scene::SceneCost::NODE,
            || Group::empty_in(&state.opt.processing),
        ) {
            Ok(group) => group,
            Err(_) => return,
        },
        layouted: TreeVec::new(&state.opt.processing),
    };

    if text::convert(&mut text, &state.opt.font_resolver, cache).is_none() {
        return;
    }

    let Ok(text) =
        crate::scene::boxed_owned(&state.opt.processing, crate::scene::SceneCost::NODE, text)
    else {
        return;
    };
    if parent
        .children
        .try_push(Node::Text(text), &state.opt.processing)
        .is_err()
    {
        return;
    }
}

struct IterState {
    chars_count: usize,
    chunk_bytes_count: usize,
    split_chunk: bool,
    text_flow: TextFlow,
    chunks: TreeVec<TextChunk>,
    decoration_styles: DecorationCache,
}

fn collect_text_chunks(
    text_node: SvgNode,
    pos_list: &[CharacterPosition],
    state: &converter::State,
    cache: &mut converter::Cache,
) -> Option<TreeVec<TextChunk>> {
    let mut iter_state = IterState {
        chars_count: 0,
        chunk_bytes_count: 0,
        split_chunk: false,
        text_flow: TextFlow::Linear,
        chunks: TreeVec::new(&state.opt.processing),
        decoration_styles: DecorationCache::new(&state.opt.processing),
    };

    collect_text_chunks_impl(text_node, pos_list, state, cache, &mut iter_state)?;
    let chunks = iter_state.chunks;
    drop(iter_state.decoration_styles);
    owner::check(&state.opt.processing, 0).ok()?;
    Some(chunks)
}

fn collect_text_chunks_impl(
    parent: SvgNode,
    pos_list: &[CharacterPosition],
    state: &converter::State,
    cache: &mut converter::Cache,
    iter_state: &mut IterState,
) -> Option<()> {
    let p = &state.opt.processing;
    if state.opt.processing.failed() {
        return None;
    }
    for child in parent.children() {
        if state.opt.processing.failed() {
            return None;
        }
        if let Some(target) = child.text_reference() {
            let refs = state
                .refs
                .enter(target, p.resources())
                .map_err(|e| p.record_resource(e))
                .ok()?;
            if !cache.note_reference(&refs, state) {
                return None;
            }
        }
        if child.is_element() {
            if child.tag_name() == Some(EId::TextPath) {
                if parent.tag_name() != Some(EId::Text) {
                    // `textPath` can be set only as a direct `text` element child.
                    iter_state.chars_count += count_chars(child);
                    continue;
                }

                match resolve_text_flow(child, state, cache) {
                    Ok(Some(v)) => {
                        iter_state.text_flow = v;
                        if super::shapes::checkpoint(&state.opt.processing, 0).is_err() {
                            return None;
                        }
                    }
                    Ok(None) => {
                        // Skip an invalid text path and all it's children.
                        // We have to update the chars count,
                        // because `pos_list` was calculated including this text path.
                        iter_state.chars_count += count_chars(child);
                        continue;
                    }
                    Err(error) => {
                        state.opt.processing.record_resource(error);
                        return None;
                    }
                }

                iter_state.split_chunk = true;
            }

            collect_text_chunks_impl(child, pos_list, state, cache, iter_state)?;
            if state.opt.processing.failed() {
                return None;
            }

            iter_state.text_flow = TextFlow::Linear;
            if super::shapes::checkpoint(&state.opt.processing, 0).is_err() {
                return None;
            }

            // Next char after `textPath` should be split too.
            if child.tag_name() == Some(EId::TextPath) {
                iter_state.split_chunk = true;
            }

            continue;
        }

        if !parent.is_visible_element(state.opt) {
            iter_state.chars_count += child.text().chars().count();
            continue;
        }

        let anchor = parent.find_attribute(AId::TextAnchor).unwrap_or_default();

        // TODO: what to do when <= 0? UB?
        let font_size = super::units::resolve_font_size(parent, state);
        let font_size = match NonZeroPositiveF32::new(font_size) {
            Some(n) => n,
            None => {
                // Skip this span.
                iter_state.chars_count += child.text().chars().count();
                continue;
            }
        };

        let Some(font) = convert_font(parent, state) else {
            return None;
        };

        let raw_paint_order: svgtypes::PaintOrder =
            parent.find_attribute(AId::PaintOrder).unwrap_or_default();
        let paint_order = super::converter::svg_paint_order_to_usvg(raw_paint_order);

        let mut dominant_baseline = parent
            .find_attribute(AId::DominantBaseline)
            .unwrap_or_default();

        // `no-change` means "use parent".
        if dominant_baseline == DominantBaseline::NoChange {
            dominant_baseline = parent
                .parent_element()
                .unwrap()
                .find_attribute(AId::DominantBaseline)
                .unwrap_or_default();
        }

        let mut apply_kerning = true;
        #[allow(clippy::if_same_then_else)]
        if parent.resolve_length(AId::Kerning, state, -1.0) == 0.0 {
            apply_kerning = false;
        } else if parent.find_attribute::<&str>(AId::FontKerning) == Some("none") {
            apply_kerning = false;
        }

        let font_optical_sizing = match parent.find_attribute::<&str>(AId::FontOpticalSizing) {
            Some("none") => crate::FontOpticalSizing::None,
            _ => crate::FontOpticalSizing::Auto, // "auto" or missing (browser default)
        };

        let mut text_length =
            parent.try_convert_length(AId::TextLength, Units::UserSpaceOnUse, state);
        // Negative values should be ignored.
        if let Some(n) = text_length {
            if n < 0.0 {
                text_length = None;
            }
        }

        let visibility: Visibility = parent.find_attribute(AId::Visibility).unwrap_or_default();

        let ancestor_count = parent
            .ancestors()
            .take_while(|node| node.tag_name() != Some(EId::Svg))
            .count();
        if let Some(owner) = cache.font_replay.as_ref() {
            let count = child.text().chars().count();
            let copies = usize::from(count != 0)
                + (1..count)
                    .filter(|offset| {
                        let position = &pos_list[iter_state.chars_count + offset];
                        position.x.is_some() || position.y.is_some()
                    })
                    .count();
            let per_copy = ancestor_count
                .checked_mul(size_of::<usize>() + size_of::<crate::ReplayLengthDirective>())
                .and_then(|v| v.checked_add(2 * size_of::<TextSpan>()));
            let bytes = per_copy
                .and_then(|v| v.checked_mul(copies))
                .and_then(|v| v.checked_add(ancestor_count * size_of_val(&parent)));
            let Some(bytes) = bytes else {
                cache.processing.fail(ProcessingFailure::FontReplay);
                return None;
            };
            if owner.reserve_replay_bytes(bytes).is_err() {
                cache.processing.fail(ProcessingFailure::FontReplay);
                return None;
            }
        }
        let source_nodes = TreeVec::try_from_iter(
            parent
                .ancestors()
                .take_while(|node| node.tag_name() != Some(EId::Svg)),
            p,
        )
        .ok()?;
        let mut length_directives = TreeVec::new(p);
        for node in source_nodes.iter().rev() {
            if !matches!(
                node.tag_name(),
                Some(EId::Text | EId::Tspan | EId::TextPath)
            ) {
                continue;
            }
            if let Some(target) = node
                .try_convert_length(AId::TextLength, Units::UserSpaceOnUse, state)
                .filter(|n| *n >= 0.0)
            {
                let parent = length_directives
                    .last()
                    .map(|d: &crate::ReplayLengthDirective| d.owner);
                length_directives
                    .try_push(
                        crate::ReplayLengthDirective {
                            owner: node.identity(),
                            parent,
                            depth: length_directives.len(),
                            target,
                            adjust: node.attribute(AId::LengthAdjust).unwrap_or_default(),
                        },
                        p,
                    )
                    .ok()?;
            }
        }
        let decoration_owners = ["underline", "overline", "line-through"].map(|value| {
            parent
                .ancestors()
                .find(|node| {
                    node.attribute::<&str>(AId::TextDecoration)
                        .is_some_and(|text| text.split(' ').any(|word| word == value))
                })
                .map(|node| node.identity())
        });
        let mut decoration_sources = core::array::from_fn(|_| None);
        for (kind, identity) in decoration_owners.iter().enumerate() {
            let Some(identity) = identity else { continue };
            let source = if let Some(style) = iter_state.decoration_styles.get(*identity, p).ok()? {
                style.try_alias_in(p).ok()?
            } else {
                let Some(node) = parent.ancestors().find(|node| node.identity() == *identity)
                else {
                    cache.processing.fail(ProcessingFailure::FontReplay);
                    return None;
                };
                if let Some(owner) = cache.font_replay.as_ref() {
                    let family = node
                        .find_attribute::<&str>(AId::FontFamily)
                        .map_or(0, str::len)
                        + state.opt.font_family.len();
                    let axes = node
                        .find_attribute::<&str>(AId::FontVariationSettings)
                        .map_or(0, str::len);
                    let lang = node
                        .find_attribute::<&str>(AId::XmlLang)
                        .map_or(3, str::len);
                    let bytes = family.checked_mul(64).and_then(|v| {
                        v.checked_add(
                            axes * 8
                                + lang * 2
                                + ancestor_count * 128
                                + size_of::<crate::tree::DecorationOwnerStyle>()
                                + 128,
                        )
                    });
                    let Some(bytes) = bytes else {
                        cache.processing.fail(ProcessingFailure::FontReplay);
                        return None;
                    };
                    if owner.reserve_replay_bytes(bytes).is_err() {
                        cache.processing.fail(ProcessingFailure::FontReplay);
                        return None;
                    }
                }
                let Some(size) =
                    NonZeroPositiveF32::new(super::units::resolve_font_size(node, state))
                else {
                    cache.processing.fail(ProcessingFailure::FontReplay);
                    return None;
                };
                let Some(font) = convert_font(node, state) else {
                    return None;
                };
                let style = crate::SharedOwner::try_new(
                    crate::tree::DecorationOwnerStyle {
                        owner: *identity,
                        font,
                        size,
                        optical: if node.find_attribute::<&str>(AId::FontOpticalSizing)
                            == Some("none")
                        {
                            crate::FontOpticalSizing::None
                        } else {
                            crate::FontOpticalSizing::Auto
                        },
                        language: TreeString::try_from_str(
                            node.find_attribute::<&str>(AId::XmlLang)
                                .filter(|v| !v.is_empty())
                                .unwrap_or("und"),
                            p,
                        )
                        .ok()?,
                        direction: u32::from(
                            node.find_attribute::<&str>(AId::Direction) == Some("rtl"),
                        ),
                        scopes: resolved_bidi_scopes(node, p)?,
                        dominant: node
                            .find_attribute(AId::DominantBaseline)
                            .unwrap_or_default(),
                        alignment: node
                            .find_attribute(AId::AlignmentBaseline)
                            .unwrap_or_default(),
                        baseline_shift: convert_baseline_shift(node, state)?,
                    },
                    p.resources(),
                    markdown_svg_budget::ResourceKind::Scratch,
                )
                .ok()?;
                iter_state
                    .decoration_styles
                    .insert(*identity, style.try_alias_in(p).ok()?, p)
                    .ok()?;
                style
            };
            decoration_sources[kind] = Some(source);
        }
        let fill = style::resolve_fill(parent, true, state, cache);
        if state.opt.processing.failed() {
            return None;
        }
        let stroke = style::resolve_stroke(parent, true, state, cache);
        if state.opt.processing.failed() {
            return None;
        }
        let Some(decoration) = resolve_decoration(parent, state, cache) else {
            return None;
        };
        let span = TextSpan {
            computed_color: parent
                .find_attribute(AId::Color)
                .unwrap_or_else(svgtypes::Color::black),
            source_owner: parent.identity(),
            source_ancestors: TreeVec::try_from_iter(
                source_nodes.iter().map(|node| node.identity()),
                p,
            )
            .ok()?,
            length_directives,
            decoration_owners,
            decoration_sources,

            start: 0,
            end: 0,
            fill,
            stroke,
            paint_order,
            font,
            font_size,
            small_caps: parent.find_attribute::<&str>(AId::FontVariant) == Some("small-caps"),
            apply_kerning,
            font_optical_sizing,
            language: TreeString::try_from_str(
                parent
                    .find_attribute::<&str>(AId::XmlLang)
                    .filter(|v| !v.is_empty())
                    .unwrap_or("und"),
                p,
            )
            .ok()?,
            direction: u32::from(parent.find_attribute::<&str>(AId::Direction) == Some("rtl")),
            bidi_scopes: resolved_bidi_scopes(parent, p)?,
            decoration,
            visible: visibility == Visibility::Visible,
            dominant_baseline,
            alignment_baseline: parent
                .find_attribute(AId::AlignmentBaseline)
                .unwrap_or_default(),
            baseline_shift: convert_baseline_shift(parent, state)?,
            letter_spacing: parent.resolve_length(AId::LetterSpacing, state, 0.0),
            word_spacing: parent.resolve_length(AId::WordSpacing, state, 0.0),
            text_length,
            length_adjust: parent.find_attribute(AId::LengthAdjust).unwrap_or_default(),
        };

        let mut is_new_span = true;
        for c in child.text().chars() {
            owner::check(p, 1).ok()?;
            let char_len = c.len_utf8();

            // Create a new chunk if:
            // - this is the first span (yes, position can be None)
            // - text character has an absolute coordinate assigned to it (via x/y attribute)
            // - `c` is the first char of the `textPath`
            // - `c` is the first char after `textPath`
            let is_new_chunk = pos_list[iter_state.chars_count].x.is_some()
                || pos_list[iter_state.chars_count].y.is_some()
                || iter_state.split_chunk
                || iter_state.chunks.is_empty();

            iter_state.split_chunk = false;

            if is_new_chunk {
                iter_state.chunk_bytes_count = 0;

                let mut span2 = span.try_copy_in(p).ok()?;
                span2.start = 0;
                span2.end = char_len;

                let addresses =
                    TreeVec::try_from_iter(std::iter::once(iter_state.chars_count), p).ok()?;
                let spans = TreeVec::try_from_iter(std::iter::once(span2), p).ok()?;
                let mut text = TreeString::new(p);
                text.try_push_char(c, p).ok()?;
                iter_state
                    .chunks
                    .try_push(
                        TextChunk {
                            source_addresses: addresses,
                            x: pos_list[iter_state.chars_count].x,
                            y: pos_list[iter_state.chars_count].y,
                            anchor,
                            spans,
                            text_flow: iter_state.text_flow.try_copy_in(p).ok()?,
                            text,
                        },
                        p,
                    )
                    .ok()?;
            } else if is_new_span {
                // Add this span to the last text chunk.
                let mut span2 = span.try_copy_in(p).ok()?;
                span2.start = iter_state.chunk_bytes_count;
                span2.end = iter_state.chunk_bytes_count + char_len;

                if let Some(chunk) = iter_state.chunks.last_mut() {
                    chunk.text.try_push_char(c, p).ok()?;
                    chunk
                        .source_addresses
                        .try_push(iter_state.chars_count, p)
                        .ok()?;
                    chunk.spans.try_push(span2, p).ok()?;
                }
            } else {
                // Extend the last span.
                if let Some(chunk) = iter_state.chunks.last_mut() {
                    chunk.text.try_push_char(c, p).ok()?;
                    chunk
                        .source_addresses
                        .try_push(iter_state.chars_count, p)
                        .ok()?;
                    if let Some(span) = chunk.spans.last_mut() {
                        debug_assert_ne!(span.end, 0);
                        span.end += char_len;
                    }
                }
            }

            is_new_span = false;
            iter_state.chars_count += 1;
            iter_state.chunk_bytes_count += char_len;
        }
    }
    owner::check(p, 0).ok()?;
    Some(())
}

fn resolved_bidi_scopes(
    parent: SvgNode,
    p: &ProcessingContext,
) -> Option<TreeVec<crate::BidiScope>> {
    let mut scopes = TreeVec::new(p);
    for node in parent.ancestors() {
        owner::check(p, 1).ok()?;
        let mode = match node.attribute::<&str>(AId::UnicodeBidi) {
            Some("embed") => 1,
            Some("bidi-override") => 2,
            Some("isolate") => 3,
            Some("isolate-override") => 4,
            Some("plaintext") => 5,
            _ => continue,
        };
        scopes
            .try_push(
                crate::BidiScope {
                    identity: node.identity(),
                    direction: u32::from(
                        node.find_attribute::<&str>(AId::Direction) == Some("rtl"),
                    ),
                    mode,
                },
                p,
            )
            .ok()?;
    }
    scopes.try_reverse(p).ok()?;
    Some(scopes)
}

fn resolve_text_flow(
    node: SvgNode,
    state: &converter::State,
    cache: &mut converter::Cache,
) -> Result<Option<TextFlow>, markdown_svg_budget::Error> {
    use crate::geometry::SharedOwner;
    use markdown_svg_budget::ResourceKind;
    let processing = &state.opt.processing;
    super::shapes::checkpoint(processing, 0)?;
    let candidate = (|| {
        let Some(linked_node) = node.attribute::<SvgNode>(AId::Href) else {
            return Ok(None);
        };
        let refs = state
            .refs
            .enter(linked_node.reference_id(), processing.resources())
            .map_err(|e| processing.record_resource(e))?;
        if !cache.note_reference(&refs, state) {
            return Err(processing
                .resource_failure()
                .expect("recorded reference failure"));
        }
        let Some(path) = super::shapes::convert(linked_node, state)? else {
            return Ok(None);
        };

        // The reference path's transform needs to be applied
        let transform = linked_node.resolve_transform(AId::Transform, state);
        let path = if !transform.is_identity() {
            let path_copy = match processing.resources() {
                Some(resources) => path
                    .as_ref()
                    .try_copy_in(resources, ResourceKind::Geometry)?
                    .try_transform(transform, resources, ResourceKind::Geometry)?,
                None => path.as_ref().clone().transform(transform),
            };
            // Both the previous SharedOwner header and the source COW alias retire
            // before the next arclength/ID consumer, including legal None.
            drop(path);
            super::shapes::checkpoint(processing, 0)?;
            let Some(path_copy) = path_copy else {
                return Ok(None);
            };
            SharedOwner::try_new(path_copy, processing.resources(), ResourceKind::Geometry)?
        } else {
            path
        };

        let start_offset: Length = node.attribute(AId::StartOffset).unwrap_or_default();
        let start_offset = if start_offset.unit == LengthUnit::Percent {
            // 'If a percentage is given, then the `startOffset` represents
            // a percentage distance along the entire path.'
            let path_len = path_length(&path, processing)?;
            (path_len * (start_offset.number / 100.0)) as f32
        } else {
            node.resolve_length(AId::StartOffset, state, 0.0)
        };

        let Some(id) = NonEmptyString::try_new(linked_node.element_id(), processing)? else {
            return Ok(None);
        };
        Ok(Some(TextFlow::Path(SharedOwner::try_new(
            TextPath {
                id,
                start_offset,
                path,
            },
            processing.resources(),
            ResourceKind::Scratch,
        )?)))
    })();
    super::shapes::checkpoint(processing, 0)?;
    candidate
}

struct FamilySink<'p> {
    families: TreeVec<FontFamily>,
    processing: &'p ProcessingContext,
}
impl FontFamilySink<markdown_svg_budget::Error> for FamilySink<'_> {
    type Name = TreeString;
    fn new_name(&mut self) -> owner::Result<TreeString> {
        owner::check(self.processing, 0)?;
        Ok(TreeString::new(self.processing))
    }
    fn push_char(&mut self, name: &mut TreeString, ch: char) -> owner::Result<()> {
        name.try_push_char(ch, self.processing)
    }
    fn finish_name(&mut self, name: TreeString, kind: FontFamilyKind) -> owner::Result<()> {
        let p = self.processing;
        owner::check(p, 0)?;
        let family = match kind {
            FontFamilyKind::Named => FontFamily::Named(name),
            generic => {
                drop(name);
                owner::check(p, 0)?;
                match generic {
                    FontFamilyKind::Serif => FontFamily::Serif,
                    FontFamilyKind::SansSerif => FontFamily::SansSerif,
                    FontFamilyKind::Cursive => FontFamily::Cursive,
                    FontFamilyKind::Fantasy => FontFamily::Fantasy,
                    FontFamilyKind::Monospace => FontFamily::Monospace,
                    FontFamilyKind::Named => unreachable!(),
                }
            }
        };
        self.families.try_push(family, p)
    }
}

fn convert_font(node: SvgNode, state: &converter::State) -> Option<Font> {
    if state.opt.processing.failed() {
        return None;
    }
    let style: FontStyle = node.find_attribute(AId::FontStyle).unwrap_or_default();
    let stretch = conv_font_stretch(node);
    let weight = resolve_font_weight(node, state)?;
    let mut variations = parse_font_variation_settings(node, &state.opt.processing)?;

    // Auto-map standard font properties to variation axes if not explicitly set.
    // This allows variable fonts to work with regular font-weight/font-stretch properties.
    let has_wght = variations.iter().any(|v| &v.tag == b"wght");
    let has_wdth = variations.iter().any(|v| &v.tag == b"wdth");
    let has_ital = variations.iter().any(|v| &v.tag == b"ital");
    let has_slnt = variations.iter().any(|v| &v.tag == b"slnt");

    // Map font-weight to wght axis (if not already set)
    if !has_wght {
        variations
            .try_push(
                FontVariation::new(*b"wght", weight as f32),
                &state.opt.processing,
            )
            .ok()?;
    }

    // Map font-stretch to wdth axis (if not already set)
    // CSS font-stretch percentages: ultra-condensed=50%, condensed=75%, normal=100%, expanded=125%, ultra-expanded=200%
    if !has_wdth {
        let wdth = match stretch {
            FontStretch::UltraCondensed => 50.0,
            FontStretch::ExtraCondensed => 62.5,
            FontStretch::Condensed => 75.0,
            FontStretch::SemiCondensed => 87.5,
            FontStretch::Normal => 100.0,
            FontStretch::SemiExpanded => 112.5,
            FontStretch::Expanded => 125.0,
            FontStretch::ExtraExpanded => 150.0,
            FontStretch::UltraExpanded => 200.0,
        };
        if wdth != 100.0 {
            variations
                .try_push(FontVariation::new(*b"wdth", wdth), &state.opt.processing)
                .ok()?;
        }
    }

    // Map font-style: italic to ital axis (if not already set)
    if !has_ital && style == FontStyle::Italic {
        variations
            .try_push(FontVariation::new(*b"ital", 1.0), &state.opt.processing)
            .ok()?;
    }

    // Map font-style: oblique to slnt axis (if not already set)
    // Default oblique angle is typically 12-14 degrees
    if !has_slnt && style == FontStyle::Oblique {
        variations
            .try_push(FontVariation::new(*b"slnt", -12.0), &state.opt.processing)
            .ok()?;
    }

    let font_families = if let Some(n) = node.ancestors().find(|n| n.has_attribute(AId::FontFamily))
    {
        n.attribute(AId::FontFamily).unwrap_or("")
    } else {
        ""
    };

    let p = &state.opt.processing;
    let mut sink = FamilySink {
        families: TreeVec::new(p),
        processing: p,
    };
    let parsed = parse_font_families_into_with_control(
        font_families,
        &mut |bytes| {
            #[cfg(test)]
            let _phase = font_family_source_tests::SourcePhase::enter();
            owner::check(p, bytes as u64)
        },
        &mut sink,
    );
    let mut families = match parsed {
        Ok(()) => sink.families,
        Err(FontFamilyParseError::Syntax(_)) => {
            drop(sink);
            owner::check(p, 0).ok()?;
            log::warn!(
                "Failed to parse {} value: '{}'. Falling back to {}.",
                AId::FontFamily,
                font_families,
                state.opt.font_family
            );
            TreeVec::new(p)
        }
        Err(FontFamilyParseError::Control(error)) => {
            p.record_resource(error);
            drop(sink);
            let _ = owner::check(p, 0);
            return None;
        }
    };
    if families.is_empty() {
        let name = TreeString::try_from_str(&state.opt.font_family, p).ok()?;
        families.try_push(FontFamily::Named(name), p).ok()?;
    }

    Some(Font {
        families,
        style,
        stretch,
        weight,
        variations,
    })
}

/// Parses the `font-variation-settings` CSS property.
///
/// Syntax: `normal | [ <string> <number> ]#`
/// Example: `"wght" 700, "wdth" 50`
fn parse_font_variation_settings(
    node: SvgNode,
    p: &ProcessingContext,
) -> Option<TreeVec<FontVariation>> {
    owner::check(p, 0).ok()?;
    let mut variations = TreeVec::new(p);
    let Some(n) = node
        .ancestors()
        .find(|n| n.has_attribute(AId::FontVariationSettings))
    else {
        return Some(variations);
    };
    let value = n
        .attribute::<&str>(AId::FontVariationSettings)
        .unwrap_or("");
    // StdFloatOpaqueV1: this loop only prepays the complete comma scan; it
    // does not read the source bytes. The persistent Split cursor consumes
    // that credit once. Check both sides of each opaque next() call.
    for chunk in value.as_bytes().chunks(4096) {
        owner::check(p, chunk.len() as u64).ok()?;
    }
    if value.eq_ignore_ascii_case("normal") || value.is_empty() {
        return Some(variations);
    }
    let mut parts = value.split(',');
    loop {
        owner::check(p, 0).ok()?;
        let part = parts.next();
        owner::check(p, 0).ok()?;
        let Some(part) = part else {
            break;
        };
        owner::check(p, 1).ok()?;
        let part = trim_font_variation_source(part, p)?;
        if part.is_empty() {
            continue;
        }
        let mut chars = part.char_indices();
        let quote = match chars.next() {
            Some((_, c @ ('"' | '\''))) => c,
            _ => continue,
        };
        let start = quote.len_utf8();
        let mut end = part.len();
        let mut tail = part.len();
        for (index, ch) in chars {
            owner::check(p, 1).ok()?;
            if ch == quote {
                end = index;
                tail = index + ch.len_utf8();
                break;
            }
        }
        let tag = &part[start..end];
        if tag.len() != 4 {
            log::warn!(
                "Invalid font-variation-settings tag: '{}' (must be 4 characters)",
                tag
            );
            continue;
        }
        let numeric = trim_font_variation_source(&part[tail..], p)?;
        // Preserve the ordinary whole-token f32 conversion. Its second input
        // pass is paid separately, and even a syntax error must pass the
        // shared-slot tail before warning or publishing a variation.
        owner::check(p, numeric.len() as u64).ok()?;
        let parsed = numeric.parse::<f32>();
        owner::check(p, 0).ok()?;
        let Ok(value) = parsed else {
            log::warn!("Invalid font-variation-settings value: '{}'", numeric);
            continue;
        };
        let bytes = tag.as_bytes();
        variations
            .try_push(
                FontVariation::new([bytes[0], bytes[1], bytes[2], bytes[3]], value),
                p,
            )
            .ok()?;
    }
    owner::check(p, 0).ok()?;
    Some(variations)
}

// The standard trim may inspect both ends of its slice. Conservatively prepay
// twice its length; retain Unicode whitespace and the ordinary borrowed result.
// Like the named std conversion, this bounded opaque call is checked at both
// sides, rather than claimed to contain request checkpoints.
fn trim_font_variation_source<'a>(value: &'a str, p: &ProcessingContext) -> Option<&'a str> {
    owner::check(p, (value.len() as u64).saturating_mul(2)).ok()?;
    let trimmed = value.trim();
    owner::check(p, 0).ok()?;
    Some(trimmed)
}

#[cfg(test)]
mod variation_conversion_tests {
    use super::*;
    use crate::geometry_test_support::{Call, request};
    use markdown_svg_budget::{Error, FailureKind, LimitKind, Limits};

    fn source(numeric: &str) -> String {
        format!(
            "<svg xmlns='http://www.w3.org/2000/svg' width='8' height='8' font-variation-settings=\"'wght' &#x2003;{numeric}&#x2003;\"/>"
        )
    }

    #[test]
    fn variation_conversion_keeps_original_f32_bits_and_unicode_trim() {
        for numeric in ["-0", "+0.5", "16777217", "1e9999", "NaN", "bad"] {
            let source = source(numeric);
            let xml = roxmltree::Document::parse(&source).unwrap();
            let doc = super::super::svgtree::Document::parse_tree_for_source(
                &xml,
                None,
                super::super::source_kind::SourceKind::GeneralImage,
                None,
            )
            .unwrap();
            let (r, _) = request(Limits::default());
            let p = ProcessingContext::with_resources(r.clone());
            let values = parse_font_variation_settings(doc.root_element(), &p).unwrap();
            match numeric.parse::<f32>() {
                Ok(expected) => {
                    assert_eq!(values.len(), 1);
                    assert_eq!(values[0].tag, *b"wght");
                    assert_eq!(values[0].value.to_bits(), expected.to_bits());
                }
                Err(_) => assert!(values.is_empty()),
            }
            drop((values, p));
            r.validate_retired().unwrap();
        }
    }

    #[test]
    fn variation_conversion_pre_and_post_faults_cannot_publish_or_recover_syntax() {
        for suffix in ["5", "bad"] {
            let numeric = format!("0.{}{suffix}", "0".repeat(257));
            let source = source(&numeric);
            let xml = roxmltree::Document::parse(&source).unwrap();
            let doc = super::super::svgtree::Document::parse_tree_for_source(
                &xml,
                None,
                super::super::source_kind::SourceKind::GeneralImage,
                None,
            )
            .unwrap();
            let (r, state) = request(Limits::default());
            let p = ProcessingContext::with_resources(r.clone());
            let values = parse_font_variation_settings(doc.root_element(), &p).unwrap();
            let events = state.lock().events.clone();
            let matching: Vec<_> = events
                .iter()
                .enumerate()
                .filter(|(_, event)| {
                    event.call == Call::Checkpoint && event.work == numeric.len() as u64
                })
                .collect();
            assert_eq!(matching.len(), 1);
            let (index, before) = matching[0];
            let after = events[index + 1];
            assert_eq!(after.call, Call::Checkpoint);
            assert_eq!(after.work, 0);
            assert_eq!(after.ordinal, before.ordinal + 1);
            let ordinals = [before.ordinal, after.ordinal];
            drop((values, p));
            r.validate_retired().unwrap();
            for ordinal in ordinals {
                for expected in [
                    Error::new(FailureKind::Cancelled, LimitKind::Work),
                    Error::native(-9702),
                ] {
                    let (r, state) = request(Limits::default());
                    state.arm(Call::Checkpoint, ordinal, expected);
                    let p = ProcessingContext::with_resources(r.clone());
                    assert!(parse_font_variation_settings(doc.root_element(), &p).is_none());
                    assert_eq!(r.failure(), Some(expected));
                    assert_eq!(p.resource_failure(), Some(expected));
                    assert_eq!(state.counts().checkpoint, ordinal);
                    assert_eq!(state.counts().reserve, 0);
                    drop(p);
                    r.validate_retired().unwrap();
                }
            }
        }
    }
}

// TODO: properly resolve narrower/wider
fn conv_font_stretch(node: SvgNode) -> FontStretch {
    if let Some(n) = node.ancestors().find(|n| n.has_attribute(AId::FontStretch)) {
        match n.attribute(AId::FontStretch).unwrap_or("") {
            "narrower" | "condensed" => FontStretch::Condensed,
            "ultra-condensed" => FontStretch::UltraCondensed,
            "extra-condensed" => FontStretch::ExtraCondensed,
            "semi-condensed" => FontStretch::SemiCondensed,
            "semi-expanded" => FontStretch::SemiExpanded,
            "wider" | "expanded" => FontStretch::Expanded,
            "extra-expanded" => FontStretch::ExtraExpanded,
            "ultra-expanded" => FontStretch::UltraExpanded,
            _ => FontStretch::Normal,
        }
    } else {
        FontStretch::Normal
    }
}

fn resolve_font_weight(node: SvgNode, state: &converter::State) -> Option<u16> {
    let p = &state.opt.processing;
    fn bound(min: usize, val: usize, max: usize) -> usize {
        std::cmp::max(min, std::cmp::min(max, val))
    }

    let nodes = TreeVec::try_from_iter(node.ancestors(), p).ok()?;
    let mut weight = 400;
    for n in nodes.iter().rev().skip(1) {
        owner::check(p, 1).ok()?;
        // skip Root
        weight = match n.attribute(AId::FontWeight).unwrap_or("") {
            "normal" => 400,
            "bold" => 700,
            "100" => 100,
            "200" => 200,
            "300" => 300,
            "400" => 400,
            "500" => 500,
            "600" => 600,
            "700" => 700,
            "800" => 800,
            "900" => 900,
            "bolder" => {
                // By the CSS2 spec the default value should be 400
                // so `bolder` will result in 500.
                // But Chrome and Inkscape will give us 700.
                // Have no idea is it a bug or something, but
                // we will follow such behavior for now.
                let step = if weight == 400 { 300 } else { 100 };

                bound(100, weight + step, 900)
            }
            "lighter" => {
                // By the CSS2 spec the default value should be 400
                // so `lighter` will result in 300.
                // But Chrome and Inkscape will give us 200.
                // Have no idea is it a bug or something, but
                // we will follow such behavior for now.
                let step = if weight == 400 { 200 } else { 100 };

                bound(100, weight - step, 900)
            }
            _ => weight,
        };
    }

    owner::check(p, 0).ok()?;
    Some(weight as u16)
}

/// Resolves text's character positions.
///
/// This includes: x, y, dx, dy.
///
/// # The character
///
/// The first problem with this task is that the *character* itself
/// is basically undefined in the SVG spec. Sometimes it's an *XML character*,
/// sometimes a *glyph*, and sometimes just a *character*.
///
/// There is an ongoing [discussion](https://github.com/w3c/svgwg/issues/537)
/// on the SVG working group that addresses this by stating that a character
/// is a Unicode code point. But it's not final.
///
/// Also, according to the SVG 2 spec, *character* is *a Unicode code point*.
///
/// Anyway, we treat a character as a Unicode code point.
///
/// # Algorithm
///
/// To resolve positions, we have to iterate over descendant nodes and
/// if the current node is a `tspan` and has x/y/dx/dy attribute,
/// than the positions from this attribute should be assigned to the characters
/// of this `tspan` and it's descendants.
///
/// Positions list can have more values than characters in the `tspan`,
/// so we have to clamp it, because values should not overlap, e.g.:
///
/// (we ignore whitespaces for example purposes,
/// so the `text` content is `Text` and not `T ex t`)
///
/// ```text
/// <text>
///   a
///   <tspan x="10 20 30">
///     bc
///   </tspan>
///   d
/// </text>
/// ```
///
/// In this example, the `d` position should not be set to `30`.
/// And the result should be: `[None, 10, 20, None]`
///
/// Another example:
///
/// ```text
/// <text>
///   <tspan x="100 110 120 130">
///     a
///     <tspan x="50">
///       bc
///     </tspan>
///   </tspan>
///   d
/// </text>
/// ```
///
/// The result should be: `[100, 50, 120, None]`
fn freeze_position_values(
    list: &[CharacterPosition],
    state: &converter::State,
    get: impl Fn(&CharacterPosition) -> Option<f32>,
) -> Option<crate::NumberList> {
    let resources = state.opt.processing.resources();
    let mut output = super::numeric::ListBuilder::new(resources);
    output.reserve_exact(list.len()).ok()?;
    for (index, value) in list.iter().enumerate() {
        if index % 256 == 0 {
            super::numeric::step(resources, (list.len() - index).min(256)).ok()?;
        }
        output.push(get(value).unwrap_or(0.0)).ok()?;
    }
    output.freeze(resources).ok()
}
fn count_string_chars(
    text: &str,
    resources: Option<&markdown_svg_budget::ResourcesRef>,
) -> Option<usize> {
    let mut count = 0usize;
    for chunk in text.as_bytes().chunks(4096) {
        if let Some(r) = resources {
            r.consume_scan_work(chunk.len()).ok()?;
        }
        count = count.checked_add(chunk.iter().filter(|byte| **byte & 0xc0 != 0x80).count())?;
    }
    Some(count)
}
fn count_chars_checked(node: SvgNode) -> Option<usize> {
    let mut count = 0usize;
    for child in node.descendants().filter(|n| n.is_text()) {
        count = count.checked_add(count_string_chars(child.text(), node.resources())?)?;
    }
    super::numeric::checked(node.resources()).ok()?;
    Some(count)
}

fn resolve_positions_list(
    text_node: SvgNode,
    state: &converter::State,
) -> Option<super::numeric::ListBuilder<CharacterPosition>> {
    // Allocate a list that has all characters positions set to `None`.
    let total_chars = count_chars_checked(text_node)?;
    let mut list = super::numeric::ListBuilder::filled(
        state.opt.processing.resources(),
        total_chars,
        CharacterPosition {
            x: None,
            y: None,
            dx: None,
            dy: None,
        },
    )
    .ok()?;

    let mut offset = 0;
    for child in text_node.descendants() {
        if child.is_element() {
            // We must ignore text positions on `textPath`.
            if !matches!(child.tag_name(), Some(EId::Text) | Some(EId::Tspan)) {
                continue;
            }

            let child_chars = count_chars_checked(child)?;
            macro_rules! push_list {
                ($aid:expr, $field:ident) => {
                    if let Some(num_list) = super::units::convert_list(child, $aid, state) {
                        // Note that we are using not the total count,
                        // but the amount of characters in the current `tspan` (with children).
                        let len = std::cmp::min(num_list.len(), child_chars);
                        for i in 0..len {
                            if i % 256 == 0 {
                                super::numeric::step(
                                    state.opt.processing.resources(),
                                    (len - i).min(256),
                                )
                                .ok()?;
                            }
                            list[offset + i].$field = Some(num_list[i]);
                        }
                    }
                };
            }

            push_list!(AId::X, x);
            push_list!(AId::Y, y);
            push_list!(AId::Dx, dx);
            push_list!(AId::Dy, dy);
        } else if child.is_text() {
            // Advance the offset.
            offset = offset.checked_add(count_string_chars(child.text(), child.resources())?)?;
        }
    }

    super::numeric::checked(text_node.resources()).ok()?;
    Some(list)
}

/// Resolves characters rotation.
///
/// The algorithm is well explained
/// [in the SVG spec](https://www.w3.org/TR/SVG11/text.html#TSpanElement) (scroll down a bit).
///
/// ![](https://www.w3.org/TR/SVG11/images/text/tspan05-diagram.png)
///
/// Note: this algorithm differs from the position resolving one.
fn resolve_rotate_list(text_node: SvgNode) -> Option<crate::NumberList> {
    // Allocate a list that has all characters angles set to `0.0`.
    let mut list = super::numeric::ListBuilder::filled(
        text_node.resources(),
        count_chars_checked(text_node)?,
        0.0,
    )
    .ok()?;
    let mut last = 0.0;
    let mut offset = 0;
    for child in text_node.descendants() {
        if child.is_element() {
            if let Some(rotate) = child.attribute::<crate::NumberList>(AId::Rotate) {
                let child_chars = count_chars_checked(child)?;
                for i in 0..child_chars {
                    if i % 256 == 0 {
                        super::numeric::step(text_node.resources(), (child_chars - i).min(256))
                            .ok()?;
                    }
                    if let Some(a) = rotate.get(i).cloned() {
                        list[offset + i] = a;
                        last = a;
                    } else {
                        // If the rotate list doesn't specify the rotation for
                        // this character - use the last one.
                        list[offset + i] = last;
                    }
                }
            }
        } else if child.is_text() {
            // Advance the offset.
            offset = offset.checked_add(count_string_chars(child.text(), child.resources())?)?;
        }
    }

    list.freeze(text_node.resources()).ok()
}

/// Resolves node's `text-decoration` property.
fn resolve_decoration(
    tspan: SvgNode,
    state: &converter::State,
    cache: &mut converter::Cache,
) -> Option<TextDecoration> {
    if state.opt.processing.failed() {
        return None;
    }
    // Checks if a decoration is present in a single node.
    fn find_decoration(node: SvgNode, value: &str) -> bool {
        if let Some(str_value) = node.attribute::<&str>(AId::TextDecoration) {
            str_value.split(' ').any(|v| v == value)
        } else {
            false
        }
    }

    // The algorithm is as follows: First, we check whether the given text decoration appears in ANY
    // ancestor, i.e. it can also appear in ancestors outside of the <text> element. If the text
    // decoration is declared somewhere, it means that this tspan will have it. However, we still
    // need to find the corresponding fill/stroke for it. To do this, we iterate through all
    // ancestors (i.e. tspans) until we find the text decoration declared. If not, we will
    // stop at latest at the text node, and use its fill/stroke.
    let mut gen_style = |text_decoration: &str| {
        if !tspan
            .ancestors()
            .any(|n| find_decoration(n, text_decoration))
        {
            return None;
        }

        let mut fill_node = None;
        let mut stroke_node = None;

        for node in tspan.ancestors() {
            if find_decoration(node, text_decoration) || node.tag_name() == Some(EId::Text) {
                fill_node = fill_node.map_or(Some(node), Some);
                stroke_node = stroke_node.map_or(Some(node), Some);
                break;
            }
        }

        let fill = fill_node.and_then(|node| style::resolve_fill(node, true, state, cache));
        if state.opt.processing.failed() {
            return None;
        }
        let stroke = stroke_node.and_then(|node| style::resolve_stroke(node, true, state, cache));
        if state.opt.processing.failed() {
            return None;
        }
        Some(TextDecorationStyle { fill, stroke })
    };

    let underline = gen_style("underline");
    if state.opt.processing.failed() {
        return None;
    }
    let overline = gen_style("overline");
    if state.opt.processing.failed() {
        return None;
    }
    let line_through = gen_style("line-through");
    if state.opt.processing.failed() {
        return None;
    }
    Some(TextDecoration {
        underline,
        overline,
        line_through,
    })
}

fn convert_baseline_shift(
    node: SvgNode,
    state: &converter::State,
) -> Option<TreeVec<BaselineShift>> {
    let p = &state.opt.processing;
    let mut shift = TreeVec::new(p);
    let nodes = TreeVec::try_from_iter(
        node.ancestors()
            .take_while(|n| n.tag_name() != Some(EId::Text)),
        p,
    )
    .ok()?;
    for &n in nodes.iter() {
        owner::check(p, 1).ok()?;
        if let Some(len) = n.try_attribute::<Length>(AId::BaselineShift) {
            if len.unit == LengthUnit::Percent {
                let n = super::units::resolve_font_size(n, state) * (len.number as f32 / 100.0);
                shift.try_push(BaselineShift::Number(n), p).ok()?;
            } else {
                let n = super::units::convert_length(
                    len,
                    n,
                    AId::BaselineShift,
                    Units::ObjectBoundingBox,
                    state,
                );
                shift.try_push(BaselineShift::Number(n), p).ok()?;
            }
        } else if let Some(s) = n.attribute(AId::BaselineShift) {
            match s {
                "sub" => shift.try_push(BaselineShift::Subscript, p).ok()?,
                "super" => shift.try_push(BaselineShift::Superscript, p).ok()?,
                _ => shift.try_push(BaselineShift::Baseline, p).ok()?,
            }
        }
    }

    if shift
        .iter()
        .all(|base| matches!(base, BaselineShift::Baseline))
    {
        shift.try_clear(p).ok()?;
    }

    owner::check(p, 0).ok()?;
    Some(shift)
}

fn count_chars(node: SvgNode) -> usize {
    node.descendants()
        .filter(|n| n.is_text())
        .fold(0, |w, n| w + n.text().chars().count())
}

/// Converts the writing mode.
///
/// [SVG 2] references [CSS Writing Modes Level 3] for the definition of the
/// 'writing-mode' property, there are only two writing modes:
/// horizontal left-to-right and vertical right-to-left.
///
/// That specification introduces new values for the property. The SVG 1.1
/// values are obsolete but must still be supported by converting the specified
/// values to computed values as follows:
///
/// - `lr`, `lr-tb`, `rl`, `rl-tb` => `horizontal-tb`
/// - `tb`, `tb-rl` => `vertical-rl`
///
/// The current `vertical-lr` behaves exactly the same as `vertical-rl`.
///
/// Also, looks like no one really supports the `rl` and `rl-tb`, except `Batik`.
/// And I'm not sure if its behaviour is correct.
///
/// So we will ignore it as well, mainly because I have no idea how exactly
/// it should affect the rendering.
///
/// [SVG 2]: https://www.w3.org/TR/SVG2/text.html#WritingModeProperty
/// [CSS Writing Modes Level 3]: https://www.w3.org/TR/css-writing-modes-3/#svg-writing-mode-css
fn convert_writing_mode(text_node: SvgNode) -> WritingMode {
    if let Some(n) = text_node
        .ancestors()
        .find(|n| n.has_attribute(AId::WritingMode))
    {
        match n.attribute(AId::WritingMode).unwrap_or("lr-tb") {
            "tb" | "tb-rl" | "vertical-rl" | "vertical-lr" => WritingMode::TopToBottom,
            _ => WritingMode::LeftToRight,
        }
    } else {
        WritingMode::LeftToRight
    }
}

fn path_length(
    path: &tiny_skia_path::Path,
    processing: &ProcessingContext,
) -> Result<f64, markdown_svg_budget::Error> {
    super::shapes::checkpoint(processing, 0)?;
    let mut prev_mx = path.points()[0].x;
    let mut prev_my = path.points()[0].y;
    let mut prev_x = prev_mx;
    let mut prev_y = prev_my;

    fn create_curve_from_line(px: f32, py: f32, x: f32, y: f32) -> kurbo::CubicBez {
        let line = kurbo::Line::new(
            kurbo::Point::new(px as f64, py as f64),
            kurbo::Point::new(x as f64, y as f64),
        );
        let p1 = line.eval(0.33);
        let p2 = line.eval(0.66);
        kurbo::CubicBez::new(line.p0, p1, p2, line.p1)
    }

    let mut length = 0.0;
    let mut segments = path.segments();
    loop {
        super::shapes::checkpoint(processing, 1)?;
        let Some(seg) = segments.next() else {
            break;
        };
        let curve = match seg {
            tiny_skia_path::PathSegment::MoveTo(p) => {
                prev_mx = p.x;
                prev_my = p.y;
                prev_x = p.x;
                prev_y = p.y;
                continue;
            }
            tiny_skia_path::PathSegment::LineTo(p) => {
                create_curve_from_line(prev_x, prev_y, p.x, p.y)
            }
            tiny_skia_path::PathSegment::QuadTo(p1, p) => kurbo::QuadBez::new(
                kurbo::Point::new(prev_x as f64, prev_y as f64),
                kurbo::Point::new(p1.x as f64, p1.y as f64),
                kurbo::Point::new(p.x as f64, p.y as f64),
            )
            .raise(),
            tiny_skia_path::PathSegment::CubicTo(p1, p2, p) => kurbo::CubicBez::new(
                kurbo::Point::new(prev_x as f64, prev_y as f64),
                kurbo::Point::new(p1.x as f64, p1.y as f64),
                kurbo::Point::new(p2.x as f64, p2.y as f64),
                kurbo::Point::new(p.x as f64, p.y as f64),
            ),
            tiny_skia_path::PathSegment::Close => {
                create_curve_from_line(prev_x, prev_y, prev_mx, prev_my)
            }
        };

        length += match processing.resources() {
            Some(resources) => curve
                .try_arclen_with_control(0.5, &mut crate::geometry::RequestControl(resources))
                .map_err(|error| crate::geometry::math_error(processing, error))?,
            None => curve.arclen(0.5),
        };
        prev_x = curve.p3.x as f32;
        prev_y = curve.p3.y as f32;
    }

    super::shapes::checkpoint(processing, 0)?;
    Ok(length)
}

#[cfg(test)]
mod geometry_control_tests {
    use super::*;
    use markdown_svg_budget::{
        Backend, Error as ResourceError, FirstFailureRef, Limits, ResourceKind, ResourcesRef,
        Ticket,
    };
    use std::sync::{
        Mutex,
        atomic::{AtomicUsize, Ordering},
    };

    #[derive(Default)]
    struct Epoch {
        next: AtomicUsize,
        live: AtomicUsize,
        calls: AtomicUsize,
        fail_call: AtomicUsize,
        error_mode: AtomicUsize,
        releases: AtomicUsize,
        fail_release: usize,
        retained: Mutex<Option<Ticket>>,
    }
    struct EpochBackend(Arc<Epoch>);
    impl Backend for EpochBackend {
        fn reserve(&self, _: ResourceKind, bytes: usize) -> Result<Ticket, ResourceError> {
            let slot = self.0.next.fetch_add(1, Ordering::Relaxed) + 1;
            self.0.live.fetch_add(bytes, Ordering::Relaxed);
            Ticket::new(bytes as u64, slot as u32, 1)
        }
        fn allocation_committed(&self, _: &Ticket, _: usize) -> Result<(), ResourceError> {
            Ok(())
        }
        fn release_after_free(&self, ticket: Ticket) -> Result<(), ResourceError> {
            let call = self.0.releases.fetch_add(1, Ordering::Relaxed) + 1;
            if call == self.0.fail_release {
                assert!(self.0.retained.lock().unwrap().replace(ticket).is_none());
                return Err(ResourceError::native(-961));
            }
            self.0
                .live
                .fetch_sub(ticket.account_nonce() as usize, Ordering::Relaxed);
            Ok(())
        }
        fn checkpoint(&self, _: u64) -> Result<(), ResourceError> {
            let call = self.0.calls.fetch_add(1, Ordering::Relaxed) + 1;
            if call == self.0.fail_call.load(Ordering::Relaxed) {
                Err(epoch_error(self.0.error_mode.load(Ordering::Relaxed)))
            } else {
                Ok(())
            }
        }
    }
    fn epoch_error(mode: usize) -> ResourceError {
        use markdown_svg_budget::{FailureKind, LimitKind};
        match mode {
            1 => ResourceError::new(FailureKind::Cancelled, LimitKind::Work),
            2 => ResourceError::new(FailureKind::Quota, LimitKind::Work),
            _ => ResourceError::native(-962),
        }
    }
    fn request(fail_release: usize) -> (ResourcesRef, Arc<Epoch>) {
        let epoch = Arc::new(Epoch {
            fail_release,
            ..Epoch::default()
        });
        let resources = ResourcesRef::new_prepaid(
            EpochBackend(epoch.clone()),
            FirstFailureRef::new_prepaid(),
            Limits::default(),
        );
        (resources, epoch)
    }
    fn flow(
        source: &str,
        resources: Option<&ResourcesRef>,
    ) -> Result<Option<TextFlow>, ResourceError> {
        let xml = roxmltree::Document::parse(source).unwrap();
        // Only the test DOM is ordinary. The actual production geometry entry
        // below gets the original resource capability through State/Options.
        let document = crate::parser::svgtree::Document::parse_tree_for_source(
            &xml,
            None,
            crate::parser::source_kind::SourceKind::GeneralImage,
            None,
        )
        .unwrap();
        let node = document
            .root()
            .descendants()
            .find(|node| node.element_id() == "tp")
            .unwrap();
        let mut opt = Options::default();
        if let Some(resources) = resources {
            opt.processing = Arc::new(ProcessingContext::with_resources(resources.clone()));
        }
        let state = converter::State {
            refs: crate::parser::reference::RefPath::default(),
            context_proofs: [crate::parser::reference::PaintProof::default(); 2],
            opt: &opt,
            parent_clip_path: None,
            parent_markers: TreeVec::new(&opt.processing),
            context_element: None,
            fe_image_link: false,
            view_box: NonZeroRect::from_xywh(0.0, 0.0, 100.0, 100.0).unwrap(),
            use_size: (None, None),
        };
        let mut cache =
            converter::Cache::with_processing(opt.processing.clone(), opt.fontdb.clone());
        resolve_text_flow(node, &state, &mut cache)
    }
    const TRIANGLE: &str = r#"<svg xmlns='http://www.w3.org/2000/svg'><path id='base' transform='scale(2)' d='M0 0L3 0L3 4z'/><text><textPath id='tp' href='#base' startOffset='25%'>x</textPath></text></svg>"#;
    const OVERFLOW: &str = r#"<svg xmlns='http://www.w3.org/2000/svg'><path id='base' transform='matrix(3e38 0 0 3e38 0 0)' d='M1 1L3 1L3 4z'/><text><textPath id='tp' href='#base' startOffset='25%'>x</textPath></text></svg>"#;

    #[test]
    fn text_path_transform_percent_length_and_paid_owner_match_the_ordinary_result() {
        let Some(TextFlow::Path(ordinary)) = flow(TRIANGLE, None).unwrap() else {
            panic!("missing ordinary textPath");
        };
        assert_eq!(ordinary.start_offset.to_bits(), 6.0f32.to_bits());
        let (resources, epoch) = request(0);
        {
            let Some(TextFlow::Path(controlled)) = flow(TRIANGLE, Some(&resources)).unwrap() else {
                panic!("missing controlled textPath");
            };
            assert!(controlled.path.belongs_to(&resources));
            assert!(controlled.path.is_paid());
            assert_eq!(
                controlled.start_offset.to_bits(),
                ordinary.start_offset.to_bits()
            );
            let bits = |path: &tiny_skia_path::Path| {
                path.points()
                    .iter()
                    .map(|point| [point.x.to_bits(), point.y.to_bits()])
                    .collect::<Vec<_>>()
            };
            assert_eq!(controlled.path.verbs(), ordinary.path.verbs());
            assert_eq!(bits(&controlled.path), bits(&ordinary.path));
        }
        assert_eq!(epoch.live.load(Ordering::Relaxed), 0);
        resources.validate_retired().unwrap();
    }

    fn triangle(resources: Option<&ResourcesRef>) -> tiny_skia_path::Path {
        let mut builder = match resources {
            Some(resources) => {
                tiny_skia_path::PathBuilder::with_resources(resources, ResourceKind::Geometry)
            }
            None => tiny_skia_path::PathBuilder::new(),
        };
        builder.try_move_to(0.0, 0.0).unwrap();
        builder.try_line_to(3.0, 0.0).unwrap();
        builder.try_line_to(3.0, 4.0).unwrap();
        builder.try_close().unwrap();
        builder.try_finish().unwrap().unwrap()
    }

    #[test]
    fn production_path_length_transports_each_actual_geometry_and_j_checkpoint_failure() {
        let ordinary = triangle(None);
        let expected = path_length(&ordinary, &ProcessingContext::default()).unwrap();
        assert!((expected - 12.0).abs() < 1e-12);
        let (baseline, epoch) = request(0);
        let path = triangle(Some(&baseline));
        let processing = ProcessingContext::with_resources(baseline.clone());
        epoch.calls.store(0, Ordering::Relaxed);
        assert_eq!(
            path_length(&path, &processing).unwrap().to_bits(),
            expected.to_bits()
        );
        let calls = epoch.calls.load(Ordering::Relaxed);
        assert!(calls > 20);
        drop(path);
        drop(processing);
        baseline.validate_retired().unwrap();
        for mode in 0..3 {
            for reject in 1..=calls {
                let (resources, epoch) = request(0);
                let path = triangle(Some(&resources));
                let processing = ProcessingContext::with_resources(resources.clone());
                epoch.calls.store(0, Ordering::Relaxed);
                epoch.fail_call.store(reject, Ordering::Relaxed);
                epoch.error_mode.store(mode, Ordering::Relaxed);
                assert_eq!(path_length(&path, &processing), Err(epoch_error(mode)));
                assert_eq!(epoch.calls.load(Ordering::Relaxed), reject);
                drop(path);
                drop(processing);
                assert_eq!(epoch.live.load(Ordering::Relaxed), 0);
                resources.validate_retired().unwrap();
            }
        }
        eprintln!("geometry-path-length-matrix checkpoints={calls} errorModes=3");
    }

    #[test]
    fn invalid_transformed_text_path_never_turns_a_temporary_release_failure_into_legal_none() {
        let (baseline, epoch) = request(0);
        assert!(flow(OVERFLOW, Some(&baseline)).unwrap().is_none());
        let releases = epoch.releases.load(Ordering::Relaxed);
        assert!(releases > 3);
        baseline.validate_retired().unwrap();
        for reject in 1..=releases {
            let (resources, epoch) = request(reject);
            assert!(
                matches!(flow(OVERFLOW, Some(&resources)), Err(error) if error == ResourceError::native(-961)),
                "release {reject}/{releases}"
            );
            assert_eq!(resources.failure(), Some(ResourceError::native(-961)));
            assert_eq!(resources.statistics().release_failures, 1);
            assert!(epoch.retained.lock().unwrap().is_some());
            assert!(resources.validate_retired().is_err());
        }
        eprintln!("geometry-text-flow-release-matrix releases={releases}");
    }
}

#[cfg(test)]
mod font_family_source_tests {
    use super::*;
    use markdown_svg_budget::{
        Backend, Error as ResourceError, FailureKind, FirstFailureRef, LimitKind, Limits,
        ResourceKind, ResourcesRef, Ticket,
    };
    use std::sync::{
        Mutex,
        atomic::{AtomicUsize, Ordering},
    };

    thread_local! {static SOURCE_PHASE:std::cell::Cell<bool>=const{std::cell::Cell::new(false)};}
    pub(super) struct SourcePhase(bool);
    impl SourcePhase {
        pub(super) fn enter() -> Self {
            Self(SOURCE_PHASE.with(|p| p.replace(true)))
        }
    }
    impl Drop for SourcePhase {
        fn drop(&mut self) {
            SOURCE_PHASE.with(|p| p.set(self.0));
        }
    }
    struct Checkpoints {
        source_work: std::sync::atomic::AtomicU64,
        source_calls: Mutex<Vec<u64>>,
        calls: Mutex<Vec<u64>>,
        zero_calls: AtomicUsize,
        reject_zero: usize,
        error: ResourceError,
        next: AtomicUsize,
        live: AtomicUsize,
    }

    struct CheckpointBackend(Arc<Checkpoints>);

    impl Backend for CheckpointBackend {
        fn reserve(&self, _: ResourceKind, bytes: usize) -> Result<Ticket, ResourceError> {
            let slot = self.0.next.fetch_add(1, Ordering::SeqCst) + 1;
            self.0.live.fetch_add(bytes, Ordering::SeqCst);
            Ticket::new(bytes as u64, slot as u32, 1)
        }

        fn allocation_committed(&self, _: &Ticket, _: usize) -> Result<(), ResourceError> {
            Ok(())
        }

        fn release_after_free(&self, ticket: Ticket) -> Result<(), ResourceError> {
            self.0
                .live
                .fetch_sub(ticket.account_nonce() as usize, Ordering::SeqCst);
            Ok(())
        }

        fn checkpoint(&self, work: u64) -> Result<(), ResourceError> {
            if SOURCE_PHASE.with(|p| p.get()) {
                self.0.source_work.fetch_add(work, Ordering::SeqCst);
                self.0.source_calls.lock().unwrap().push(work);
            }
            self.0.calls.lock().unwrap().push(work);
            if work == 0
                && self.0.zero_calls.fetch_add(1, Ordering::SeqCst) + 1 == self.0.reject_zero
            {
                Err(self.0.error)
            } else {
                Ok(())
            }
        }
    }

    fn controlled_request(
        reject_zero: usize,
        error: ResourceError,
        work_limit: u64,
    ) -> (ResourcesRef, Arc<Checkpoints>) {
        let checkpoints = Arc::new(Checkpoints {
            source_work: std::sync::atomic::AtomicU64::new(0),
            source_calls: Mutex::new(Vec::new()),
            calls: Mutex::new(Vec::new()),
            zero_calls: AtomicUsize::new(0),
            reject_zero,
            error,
            next: AtomicUsize::new(0),
            live: AtomicUsize::new(0),
        });
        let limits = Limits {
            work: work_limit,
            ..Limits::default()
        };
        let resources = ResourcesRef::new_prepaid(
            CheckpointBackend(checkpoints.clone()),
            FirstFailureRef::new_prepaid(),
            limits,
        );
        (resources, checkpoints)
    }

    fn named(value: &str) -> FontFamily {
        FontFamily::Named(TreeString::plain(value.to_owned()))
    }

    fn source_font(source: &str, resources: Option<&ResourcesRef>) -> Option<Font> {
        let xml = roxmltree::Document::parse(source).unwrap();
        let document = crate::parser::svgtree::Document::parse_tree_for_source(
            &xml,
            None,
            crate::parser::source_kind::SourceKind::GeneralImage,
            resources.filter(|r| r.failure().is_none()).cloned(),
        )
        .ok()?;
        let node = document
            .root()
            .descendants()
            .find(|node| node.element_id() == "target")
            .unwrap();
        let mut opt = Options::default();
        opt.font_family = "Parser Default Sentinel".to_owned();
        if let Some(resources) = resources {
            opt.processing = Arc::new(crate::ProcessingContext::with_resources(resources.clone()));
        }
        let state = converter::State {
            refs: crate::parser::reference::RefPath::default(),
            context_proofs: [crate::parser::reference::PaintProof::default(); 2],
            opt: &opt,
            parent_clip_path: None,
            parent_markers: TreeVec::new(&opt.processing),
            context_element: None,
            fe_image_link: false,
            view_box: crate::NonZeroRect::from_xywh(0.0, 0.0, 100.0, 100.0).unwrap(),
            use_size: (None, None),
        };
        convert_font(node, &state)
    }

    fn source_families(source: &str) -> TreeVec<FontFamily> {
        source_font(source, None).unwrap().families
    }

    #[test]
    fn presentation_attribute_preserves_named_and_generic_source_types() {
        let source = r#"<svg xmlns='http://www.w3.org/2000/svg'><text id='target' font-family='"SERIF", SeRiF, "\73 erif", \73 erif'>x</text></svg>"#;
        assert_eq!(
            source_families(source),
            vec![
                named("SERIF"),
                FontFamily::Serif,
                named("serif"),
                FontFamily::Serif
            ]
        );
    }

    #[test]
    fn presentation_attribute_decodes_escapes_without_splitting_names() {
        let source = r#"<svg xmlns='http://www.w3.org/2000/svg'><text id='target' font-family='"\41 rial", ACME\,Inc, ACME\ Sans, ACME\\Font, "A\\", "A\\\&quot;B"'>x</text></svg>"#;
        assert_eq!(
            source_families(source),
            vec![
                named("Arial"),
                named("ACME,Inc"),
                named("ACME Sans"),
                named("ACME\\Font"),
                named("A\\"),
                named("A\\\"B"),
            ]
        );
    }

    #[test]
    fn presentation_attribute_keeps_unicode_and_token_comments() {
        let source = "<svg xmlns='http://www.w3.org/2000/svg'><text id='target' font-family='/*lead*/\u{80}Face/**/Sans, 𐐀Name, &quot;A/*literal*/B&quot;, sans-serif Extra'>x</text></svg>";
        assert_eq!(
            source_families(source),
            vec![
                named("\u{80}Face Sans"),
                named("𐐀Name"),
                named("A/*literal*/B"),
                named("sans-serif Extra"),
            ]
        );
    }

    #[test]
    fn inherited_inline_style_decodes_family_list() {
        let source = r#"<svg xmlns='http://www.w3.org/2000/svg'><g style='font-family: "\4E2D \6587", ACME\,Inc, SANS-SERIF'><text id='target'>x</text></g></svg>"#;
        assert_eq!(
            source_families(source),
            vec![named("中文"), named("ACME,Inc"), FontFamily::SansSerif]
        );
    }

    #[test]
    fn stylesheet_cascade_uses_the_selected_list_without_reclassifying_named() {
        let source = r#"<svg xmlns='http://www.w3.org/2000/svg'><style>text { font-family: Wrong; } .chosen { font-family: "\53 ERIF", \73 erif, ACME\ Sans; }</style><text id='target' class='chosen' font-family='AlsoWrong'>x</text></svg>"#;
        assert_eq!(
            source_families(source),
            vec![named("SERIF"), FontFamily::Serif, named("ACME Sans")]
        );
    }

    #[test]
    fn inline_style_string_line_continuation_and_source_replacement_are_decoded() {
        let source = r#"<svg xmlns='http://www.w3.org/2000/svg'><text id='target' style='font-family: "A\&#10;B", "\0 \D800 \110000"'>x</text></svg>"#;
        assert_eq!(source_families(source), vec![named("AB"), named("���")]);
    }

    #[test]
    fn inherited_inline_font_shorthand_keeps_the_complete_family_tail() {
        let source = r#"<svg xmlns='http://www.w3.org/2000/svg'><g style='font: italic 12px "\53 ERIF", \73 erif, ACME\,Inc'><text id='target'>x</text></g></svg>"#;
        assert_eq!(
            source_families(source),
            vec![named("SERIF"), FontFamily::Serif, named("ACME,Inc")]
        );
    }

    #[test]
    fn stylesheet_font_shorthand_uses_the_selected_complete_family_tail() {
        let source = r#"<svg xmlns='http://www.w3.org/2000/svg'><style>.chosen { font: 12px "\53 ERIF", \73 erif, ACME\ Sans; }</style><text id='target' class='chosen' font-family='Wrong'>x</text></svg>"#;
        assert_eq!(
            source_families(source),
            vec![named("SERIF"), FontFamily::Serif, named("ACME Sans")]
        );
    }

    #[test]
    fn inline_font_extent_protects_delimiters_and_keeps_important_precedence() {
        let source = r#"<svg xmlns='http://www.w3.org/2000/svg'><text id='target' style='font-family: "A; } !", ACME\;Inc, A\}B, A\!B/* ; } !important */ !important; font-family: Wrong'>x</text></svg>"#;
        assert_eq!(
            source_families(source),
            vec![
                named("A; } !"),
                named("ACME;Inc"),
                named("A}B"),
                named("A!B")
            ]
        );
    }

    #[test]
    fn inline_font_extent_keeps_quote_parity_crlf_and_unicode_name_edges() {
        let source = "<svg xmlns='http://www.w3.org/2000/svg'><text id='target' style='font-family: &quot;A\\\\&quot;, &quot;A\\\\\\&quot;B&quot;, &quot;C\\&#13;&#10;D&quot;, \u{80}Face, Face\u{2003}'>x</text></svg>";
        assert_eq!(
            source_families(source),
            vec![
                named("A\\"),
                named("A\\\"B"),
                named("CD"),
                named("\u{80}Face"),
                named("Face\u{2003}")
            ]
        );
    }

    #[test]
    fn inline_font_unclosed_string_rejects_whole_value_and_comment_eof_keeps_original_rule() {
        let bad = r#"<svg xmlns='http://www.w3.org/2000/svg'><text id='target' style='font-family: First, "unfinished; } !important; fill:red'>x</text></svg>"#;
        assert_eq!(source_families(bad), vec![named("Parser Default Sentinel")]);
        // CSS comment EOF remains the existing family parser's trivia recovery;
        // the intake retains that entire tail and cannot expose its delimiters.
        let comment = r#"<svg xmlns='http://www.w3.org/2000/svg'><text id='target' style='font-family: First/* unfinished; } !important; fill:red'>x</text></svg>"#;
        assert_eq!(source_families(comment), vec![named("First")]);
    }

    #[test]
    fn bad_string_uses_original_syntax_fallback() {
        let source = r#"<svg xmlns='http://www.w3.org/2000/svg'><text id='target' font-family='"bad&#10;string"'>x</text></svg>"#;
        assert_eq!(
            source_families(source),
            vec![named("Parser Default Sentinel")]
        );
    }

    #[test]
    fn unescaped_numeric_family_still_uses_original_syntax_fallback() {
        let source = r#"<svg xmlns='http://www.w3.org/2000/svg'><text id='target' font-family='123Face'>x</text></svg>"#;
        assert_eq!(
            source_families(source),
            vec![named("Parser Default Sentinel")]
        );
    }

    #[test]
    fn controlled_source_prepays_family_bytes_and_rejects_work_quota() {
        let family = format!(
            "/*{}*/ \"{}\", SeRiF",
            "中".repeat(3000),
            "A\\41 ".repeat(2000)
        );
        let source = format!(
            "<svg xmlns='http://www.w3.org/2000/svg'><text id='target' font-family='{family}'>x</text></svg>"
        );
        let (resources, checkpoints) =
            controlled_request(usize::MAX, ResourceError::native(-1374), u64::MAX);
        let font = source_font(&source, Some(&resources)).unwrap();
        assert_eq!(
            font.families,
            vec![named(&"AA".repeat(2000)), FontFamily::Serif]
        );
        assert_eq!(
            checkpoints.source_work.load(Ordering::SeqCst),
            family.len() as u64
        );
        assert!(resources.statistics().work >= checkpoints.source_work.load(Ordering::SeqCst));
        eprintln!(
            "font-source-work={} full-work={}",
            checkpoints.source_work.load(Ordering::SeqCst),
            resources.statistics().work
        );
        assert!(
            checkpoints
                .source_calls
                .lock()
                .unwrap()
                .iter()
                .all(|work| *work <= 4096)
        );
        drop(font);
        resources.validate_retired().unwrap();

        let (resources, _) = controlled_request(usize::MAX, ResourceError::native(-1374), 4095);
        assert!(source_font(&source, Some(&resources)).is_none());
        assert_eq!(
            resources.failure(),
            Some(ResourceError::new(FailureKind::Quota, LimitKind::Work))
        );
        resources.validate_retired().unwrap();
    }

    #[test]
    fn preexisting_resource_first_error_never_becomes_default_family() {
        let source = "<svg xmlns='http://www.w3.org/2000/svg'><text id='target' font-family='123'>x</text></svg>";
        let (resources, checkpoints) =
            controlled_request(usize::MAX, ResourceError::native(-1374), u64::MAX);
        let first = ResourceError::native(-1618);
        resources.record(first);
        assert!(source_font(source, Some(&resources)).is_none());
        assert_eq!(resources.failure(), Some(first));
        assert!(checkpoints.calls.lock().unwrap().is_empty());
        resources.validate_retired().unwrap();
    }

    #[test]
    fn text_and_decoration_font_checkpoint_failures_stop_without_partial_text() {
        let source = "<svg xmlns='http://www.w3.org/2000/svg'><text id='target' font-family='First' text-decoration='underline'><tspan>x</tspan>y</text></svg>";
        for (reject_zero, error) in [
            (1, ResourceError::native(-1374)),
            (
                2,
                ResourceError::new(FailureKind::Cancelled, LimitKind::Work),
            ),
        ] {
            let xml = roxmltree::Document::parse(source).unwrap();
            let document = crate::parser::svgtree::Document::parse_tree_for_source(
                &xml,
                None,
                crate::parser::source_kind::SourceKind::GeneralImage,
                None,
            )
            .unwrap();
            let node = document
                .root()
                .descendants()
                .find(|node| node.element_id() == "target")
                .unwrap();
            let (resources, checkpoints) = controlled_request(reject_zero, error, u64::MAX);
            let mut opt = Options::default();
            opt.processing = Arc::new(crate::ProcessingContext::with_resources(resources.clone()));
            let state = converter::State {
                refs: crate::parser::reference::RefPath::default(),
                context_proofs: [crate::parser::reference::PaintProof::default(); 2],
                opt: &opt,
                parent_clip_path: None,
                parent_markers: TreeVec::new(&opt.processing),
                context_element: None,
                fe_image_link: false,
                view_box: crate::NonZeroRect::from_xywh(0.0, 0.0, 100.0, 100.0).unwrap(),
                use_size: (None, None),
            };
            let mut cache = converter::Cache::new(opt.fontdb.clone());
            cache.processing = opt.processing.clone();
            let mut parent = Group::empty();
            convert(node, &state, &mut cache, &mut parent);
            assert!(parent.children().is_empty());
            assert_eq!(opt.processing.resource_failure(), Some(error));
            assert_eq!(resources.failure(), Some(error));
            assert_eq!(checkpoints.zero_calls.load(Ordering::SeqCst), reject_zero);
            drop(state);
            drop((parent, cache, opt, document, xml));
            assert_eq!(checkpoints.live.load(Ordering::SeqCst), 0);
            resources.validate_retired().unwrap();
        }
    }
}

#[cfg(test)]
mod numeric_position_tests {
    use super::*;
    #[test]
    fn paid_unicode_positions_overlay_and_last_rotation_match_the_original_rules() {
        let (resources, _) = crate::parser::numeric::tests::request();
        let source = "<svg xmlns='http://www.w3.org/2000/svg'><text><tspan x='100 110 120 130' dx='1 2 3 4' rotate='10 20'>中<tspan x='50' dx='9' rotate='30'>β𐐀</tspan></tspan>d</text></svg>";
        let xml = roxmltree::Document::parse(source).unwrap();
        let document = crate::parser::svgtree::Document::parse_tree_for_source(
            &xml,
            None,
            crate::parser::source_kind::SourceKind::GeneralImage,
            Some(resources.clone()),
        )
        .unwrap();
        let node = document
            .root()
            .descendants()
            .find(|n| n.tag_name() == Some(EId::Text))
            .unwrap();
        let mut opt = Options::default();
        opt.processing =
            std::sync::Arc::new(crate::ProcessingContext::with_resources(resources.clone()));
        let state = converter::State {
            refs: crate::parser::reference::RefPath::default(),
            context_proofs: [crate::parser::reference::PaintProof::default(); 2],
            opt: &opt,
            parent_clip_path: None,
            parent_markers: TreeVec::new(&opt.processing),
            context_element: None,
            fe_image_link: false,
            view_box: crate::NonZeroRect::from_xywh(0.0, 0.0, 100.0, 100.0).unwrap(),
            use_size: (None, None),
        };
        let positions = resolve_positions_list(node, &state).unwrap();
        assert_eq!(
            positions.iter().map(|v| v.x).collect::<Vec<_>>(),
            vec![Some(100.0), Some(50.0), Some(120.0), None]
        );
        let dx = freeze_position_values(&positions, &state, |v| v.dx).unwrap();
        let rotation = resolve_rotate_list(node).unwrap();
        assert_eq!(dx.as_slice(), &[1.0, 9.0, 3.0, 0.0]);
        assert_eq!(rotation.as_slice(), &[10.0, 30.0, 30.0, 0.0]);
        assert!(dx.belongs_to(&resources) && rotation.belongs_to(&resources));
        drop(state);
        drop((dx, rotation, positions, document, xml, opt));
        resources.validate_retired().unwrap();
    }
}
