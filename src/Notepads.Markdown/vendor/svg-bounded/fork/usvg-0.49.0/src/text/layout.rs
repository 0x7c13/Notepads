// Copyright 2022 the Resvg Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT
// Modifications Copyright (c) 2026, Jiaqi (0x7c13) Liu.

use crate::tree::owner::{self, PolicyValue, TreeString, TreeVec, TryCopy};
use std::collections::{HashMap, HashSet};
use std::num::NonZeroU16;
use std::ops::Deref;
use std::sync::Arc;

use fontdb::{Database, ID};
use harfrust::ShapeOptions;
use kurbo::{ParamCurve, ParamCurveDeriv};
use skrifa::MetadataProvider;
use strict_num::NonZeroPositiveF32;
use tiny_skia_path::{NonZeroRect, Transform};
use unicode_script::UnicodeScript;

use crate::tree::BBox;
use crate::{
    AlignmentBaseline, ApproxZeroUlps, BaselineShift, DominantBaseline, Fill, FillRule,
    FontResolver, GlyphId, LengthAdjust, PaintOrder, Path, ShapeRendering, Stroke, Text,
    TextAnchor, TextChunk, TextDecorationStyle, TextFlow, TextPath, TextSpan, WritingMode,
};

type GeometryResult<T> = Result<T, markdown_svg_budget::Error>;

fn geometry_checkpoint(processing: &crate::ProcessingContext, work: u64) -> GeometryResult<()> {
    if let Some(error) = processing.resource_failure() {
        return Err(error);
    }
    if let Some(resources) = processing.resources() {
        resources.checkpoint(work)?;
    }
    Ok(())
}

struct GeometryControl<'a>(&'a crate::ProcessingContext);
impl markdown_svg_budget::WorkControl for GeometryControl<'_> {
    type Error = markdown_svg_budget::Error;
    fn checkpoint(&mut self, work: u64) -> GeometryResult<()> {
        geometry_checkpoint(self.0, work)
    }
}

enum GeometryVec<T> {
    Compat(Vec<T>),
    Paid(markdown_svg_budget::BudgetVec<T>),
}
impl<T> GeometryVec<T> {
    fn new(processing: &crate::ProcessingContext) -> Self {
        match processing.resources() {
            Some(resources) => Self::Paid(markdown_svg_budget::BudgetVec::new(
                resources.clone(),
                markdown_svg_budget::ResourceKind::Scratch,
            )),
            None => Self::Compat(Vec::new()),
        }
    }
    fn with_capacity(
        processing: &crate::ProcessingContext,
        capacity: usize,
    ) -> GeometryResult<Self> {
        let mut value = Self::new(processing);
        match &mut value {
            Self::Compat(values) => values.reserve_exact(capacity),
            Self::Paid(values) => values.try_reserve_exact(capacity)?,
        }
        Ok(value)
    }
    fn push(&mut self, value: T) -> GeometryResult<()> {
        match self {
            Self::Compat(values) => {
                values.push(value);
                Ok(())
            }
            Self::Paid(values) => values.try_push(value),
        }
    }
}
impl<T> Deref for GeometryVec<T> {
    type Target = [T];
    fn deref(&self) -> &[T] {
        match self {
            Self::Compat(values) => values,
            Self::Paid(values) => values.as_slice(),
        }
    }
}
impl<T> std::ops::DerefMut for GeometryVec<T> {
    fn deref_mut(&mut self) -> &mut [T] {
        match self {
            Self::Compat(values) => values,
            Self::Paid(values) => values.as_mut_slice(),
        }
    }
}

fn geometry_builder(
    processing: &crate::ProcessingContext,
    kind: markdown_svg_budget::ResourceKind,
) -> tiny_skia_path::PathBuilder {
    match processing.resources() {
        Some(resources) => tiny_skia_path::PathBuilder::with_resources(resources, kind),
        None => tiny_skia_path::PathBuilder::new(),
    }
}

fn geometry_transform(
    path: tiny_skia_path::Path,
    transform: Transform,
    processing: &crate::ProcessingContext,
    kind: markdown_svg_budget::ResourceKind,
) -> GeometryResult<Option<tiny_skia_path::Path>> {
    match processing.resources() {
        Some(resources) => path.try_transform(transform, resources, kind),
        None => Ok(path.transform(transform)),
    }
}

fn geometry_bounds(
    path: &tiny_skia_path::Path,
    processing: &crate::ProcessingContext,
) -> GeometryResult<Option<tiny_skia_path::Rect>> {
    match processing.resources() {
        Some(resources) => path.try_compute_tight_bounds(resources),
        None => Ok(path.compute_tight_bounds()),
    }
}

/// A glyph that has already been positioned correctly.
///
/// Note that the transform already takes the font size into consideration, so applying the
/// transform to the outline of the glyphs is all that is necessary to display it correctly.
#[derive(Debug)]
pub struct PositionedGlyph {
    pub(crate) local_representation:
        Option<super::flatten::RepresentationRef<super::flatten::LocalRepresentation>>,
    /// Returns the transform of the glyph itself within the cluster. For example,
    /// for zalgo text, it contains the transform to position the glyphs above/below
    /// the main glyph.
    glyph_ts: Transform,
    source_span: Option<usize>,
    source_range: std::ops::Range<usize>,
    raw_x_advance: i32,
    raw_y_advance: i32,
    resolved_context: Option<crate::ReplayGlyphIdentity>,
    layout_font: Option<ResolvedFont>,
    writing_transform: Option<Transform>,
    progression_start: f32,
    progression_advance: f32,
    /// Returns the transform of the whole cluster that the glyph is part of.
    cluster_ts: Transform,
    /// Returns the transform of the span that the glyph is a part of.
    span_ts: Transform,
    /// The units per em of the font the glyph belongs to.
    units_per_em: u16,
    /// The font size the glyph should be scaled to.
    font_size: f32,
    geometry_origin: crate::ReplayGeometryOrigin,
    placement_policy: crate::ReplayPlacementPolicy,
    engine_metrics_midpoint: Option<f32>,
    /// The ID of the glyph.
    pub id: GlyphId,
    /// The text from the original string that corresponds to that glyph.
    pub text: TreeString,
    /// The ID of the font the glyph should be taken from. Can be used with the
    /// [font database of the tree](crate::Tree::fontdb) this glyph is part of.
    pub font: ID,
}

impl PositionedGlyph {
    pub(super) fn coverage_source(
        &self,
        representation: crate::GlyphOutlineKind,
        local_outline_present: bool,
        contributed_to_path: bool,
        outline_transform: Transform,
    ) -> Option<crate::PlacedGlyphSource> {
        Some(crate::PlacedGlyphSource {
            font: self.font,
            glyph: u16::try_from(self.id.0).ok()?,
            units_per_em: self.units_per_em,
            font_size: self.font_size,
            representation,
            local_outline_present,
            contributed_to_path,
            source_span: self.source_span,
            layout_source_range: [self.source_range.start, self.source_range.end],
            source_identity: self.resolved_context,
            geometry_origin: self.geometry_origin,
            placement_policy: self.placement_policy,
            glyph_transform: self.glyph_ts,
            cluster_transform: self.cluster_ts,
            span_transform: self.span_ts,
            outline_transform,
        })
    }

    pub(crate) fn source_span(&self) -> Option<usize> {
        self.source_span
    }
    pub(crate) fn replay_observation<'a>(
        &'a self,
        element_id: &'a str,
        representation: u32,
        representation_transform: Transform,
        scene_transform: Transform,
        path: Option<&'a tiny_skia_path::Path>,
        span: &Span,
        fill_rule: crate::FillRule,
    ) -> crate::ReplayGeometryObservation<'a> {
        crate::ReplayGeometryObservation {
            font: self.font,
            glyph: self.id.0,
            element_id,
            cluster_text: &self.text,
            representation,
            units_per_em: self.units_per_em,
            font_size: self.font_size,
            geometry_origin: self.geometry_origin,
            placement_policy: self.placement_policy,
            engine_metrics_midpoint: self.engine_metrics_midpoint,
            glyph_transform: self.glyph_ts,
            cluster_transform: self.cluster_ts,
            span_transform: self.span_ts,
            representation_transform,
            scene_transform,
            path,
            fill_rule,
            fill_opacity: span.fill.as_ref().map(|fill| fill.opacity().get()),
            visible: span.visible,
        }
    }
    /// Returns the font size for this glyph.
    pub fn font_size(&self) -> f32 {
        self.font_size
    }

    /// Returns the transform of glyph.
    pub fn transform(&self) -> Transform {
        let sx = self.font_size / self.units_per_em as f32;

        self.span_ts
            .pre_concat(self.cluster_ts)
            .pre_concat(Transform::from_scale(sx, sx))
            .pre_concat(self.glyph_ts)
    }

    /// Returns the transform of glyph, assuming that an outline
    /// glyph is being used (i.e. from the `glyf` or `CFF/CFF2` table).
    pub fn outline_transform(&self) -> Transform {
        // Outlines are mirrored by default.
        self.transform()
            .pre_concat(Transform::from_scale(1.0, -1.0))
    }

    /// Returns the transform for the glyph, assuming that a CBTD-based raster glyph
    /// is being used.
    pub fn cbdt_transform(&self, x: f32, y: f32, pixels_per_em: f32) -> Transform {
        self.transform()
            .pre_concat(Transform::from_scale(
                self.units_per_em as f32 / pixels_per_em,
                self.units_per_em as f32 / pixels_per_em,
            ))
            // Right now, the top-left corner of the image would be placed in
            // on the "text cursor", but we want the bottom-left corner to be there,
            // so we need to shift it up and also apply the x/y offset.
            .pre_translate(x, -y)
    }

    /// Returns the transform for the glyph, assuming that a sbix-based raster glyph
    /// is being used.
    pub fn sbix_transform(
        &self,
        x: f32,
        y: f32,
        x_min: f32,
        y_min: f32,
        pixels_per_em: f32,
        height: f32,
    ) -> Transform {
        // In contrast to CBDT, we also need to look at the outline bbox of the glyph and add a shift if necessary.
        let bbox_x_shift = -x_min;

        let bbox_y_shift = if y_min.approx_zero_ulps(4) {
            // For unknown reasons, using Apple Color Emoji will lead to a vertical shift on MacOS, but this shift
            // doesn't seem to be coming from the font and most likely is somehow hardcoded. On Windows,
            // this shift will not be applied. However, if this shift is not applied the emojis are a bit
            // too high up when being together with other text, so we try to imitate this.
            // See also https://github.com/harfbuzz/harfbuzz/issues/2679#issuecomment-1345595425
            // So whenever the y-shift is 0, we approximate this vertical shift that seems to be produced by it.
            // This value seems to be pretty close to what is happening on MacOS.
            // We can still remove this if it turns out to be a problem, but Apple Color Emoji is pretty
            // much the only `sbix` font out there and they all seem to have a y-shift of 0, so it
            // makes sense to keep it.
            0.128 * self.units_per_em as f32
        } else {
            -y_min
        };

        self.transform()
            .pre_concat(Transform::from_translate(bbox_x_shift, bbox_y_shift))
            .pre_concat(Transform::from_scale(
                self.units_per_em as f32 / pixels_per_em,
                self.units_per_em as f32 / pixels_per_em,
            ))
            // Right now, the top-left corner of the image would be placed in
            // on the "text cursor", but we want the bottom-left corner to be there,
            // so we need to shift it up and also apply the x/y offset.
            .pre_translate(x, -height - y)
    }

    /// Returns the transform for the glyph, assuming that an SVG glyph is
    /// being used.
    pub fn svg_transform(&self) -> Transform {
        self.transform()
    }

    /// Returns the transform for the glyph, assuming that a COLR glyph is
    /// being used.
    pub fn colr_transform(&self) -> Transform {
        self.outline_transform()
    }
}

/// A span contains a number of layouted glyphs that share the same fill, stroke, paint order and
/// visibility.
#[derive(Debug)]
pub struct Span {
    pub(crate) computed_color: svgtypes::Color,
    /// The fill of the span.
    pub fill: Option<Fill>,
    /// The stroke of the span.
    pub stroke: Option<Stroke>,
    /// The paint order of the span.
    pub paint_order: PaintOrder,
    /// The font size of the span.
    pub font_size: NonZeroPositiveF32,
    /// Font variation settings for variable fonts.
    pub(crate) variations: TreeVec<crate::FontVariation>,
    /// Font optical sizing mode.
    pub font_optical_sizing: crate::FontOpticalSizing,
    /// The visibility of the span.
    pub visible: bool,
    /// The glyphs that make up the span.
    pub(crate) positioned_glyphs: TreeVec<PositionedGlyph>,
    /// An underline text decoration of the span.
    /// Needs to be rendered before all glyphs.
    pub underline: Option<Path>,
    /// An overline text decoration of the span.
    /// Needs to be rendered before all glyphs.
    pub overline: Option<Path>,
    /// A line-through text decoration of the span.
    /// Needs to be rendered after all glyphs.
    pub line_through: Option<Path>,
}

impl Span {
    /// Borrow this span's retained font axes.
    pub fn variations(&self) -> &[crate::FontVariation] {
        &self.variations
    }
    /// Borrow this span's positioned glyphs.
    pub fn positioned_glyphs(&self) -> &[PositionedGlyph] {
        &self.positioned_glyphs
    }
}

#[derive(Debug)]
struct GlyphCluster {
    byte_idx: ByteIndex,
    source_end: usize,
    logical_parent: Option<usize>,
    resolved_script: Option<unicode_script::Script>,
    spacing_eligible: bool,
    spacing_after: f32,
    inline_scale: f32,
    codepoint: char,
    width: f32,
    advance: f32,
    text_length_spacing: f32,
    ascent: f32,
    descent: f32,
    has_relative_shift: bool,
    glyphs: TreeVec<PositionedGlyph>,
    transform: Transform,
    path_transform: Transform,
    visible: bool,
}

impl GlyphCluster {
    pub(crate) fn height(&self) -> f32 {
        self.ascent - self.descent
    }

    pub(crate) fn transform(&self) -> Transform {
        self.path_transform.post_concat(self.transform)
    }
}

pub(crate) fn layout_text(
    text_node: &Text,
    resolver: &FontResolver,
    fontdb: &mut Arc<fontdb::Database>,
    replay: Option<&crate::ReplayText>,
    replay_owner: Option<&dyn crate::FontReplay>,
    processing: &crate::ProcessingContext,
    representation_cache: Option<&mut crate::Cache>,
) -> Option<(TreeVec<Span>, NonZeroRect)> {
    let candidate = layout_text_inner(
        text_node,
        resolver,
        fontdb,
        replay,
        replay_owner,
        processing,
        representation_cache,
    );
    if let Err(error) = geometry_checkpoint(processing, 0) {
        processing.record_resource(error);
        return None;
    }
    candidate
}

fn layout_text_inner(
    text_node: &Text,
    resolver: &FontResolver,
    fontdb: &mut Arc<fontdb::Database>,
    replay: Option<&crate::ReplayText>,
    replay_owner: Option<&dyn crate::FontReplay>,
    processing: &crate::ProcessingContext,
    mut representation_cache: Option<&mut crate::Cache>,
) -> Option<(TreeVec<Span>, NonZeroRect)> {
    let _derived_retention = if let (Some(prepared), Some(owner)) = (replay, replay_owner) {
        if let Some(proof) = prepared.logical.as_ref() {
            if proof.checkpoint(1).is_err() {
                processing.fail(crate::ProcessingFailure::FontReplay);
                return None;
            }
            let glyphs = prepared.chunks.iter().flatten();
            let mut count = 0usize;
            let mut source_bytes = 0usize;
            for glyph in glyphs {
                count = count.checked_add(1)?;
                source_bytes = source_bytes.checked_add(glyph.end.checked_sub(glyph.start)?)?;
            }
            let cells = prepared.logical.as_ref()?.cells().len();
            let span_count = text_node
                .chunks
                .iter()
                .map(|chunk| chunk.spans.len())
                .sum::<usize>();
            let metadata = count
                .checked_mul(
                    size_of::<Glyph>()
                        + 3 * size_of::<PositionedGlyph>()
                        + 4 * size_of::<GlyphCluster>()
                        + 128,
                )?
                .checked_add(source_bytes.checked_mul(4)?)?
                .checked_add(cells.checked_mul(4 * size_of::<GlyphCluster>() + 128)?)?
                .checked_add(span_count.checked_mul(128)?)?
                .checked_add(64 * (size_of::<ResolvedFont>() + 128))?;
            // This remains a conservative admission for other unhooked layout
            // containers. The bidi permutation now owns its separate exact
            // payment; no part of this estimate is attributed to that payload.
            Some(owner.reserve_replay_bytes(metadata).ok()?)
        } else {
            None
        }
    } else {
        None
    };
    if replay.is_some_and(|p| p.logical.is_some()) {
        let mut directive_domains = HashMap::new();
        for (domain, chunk) in text_node.chunks.iter().enumerate() {
            for span in &chunk.spans {
                for directive in &span.length_directives {
                    if directive_domains
                        .insert(directive.owner, domain)
                        .is_some_and(|previous| previous != domain)
                    {
                        let _ = replay_owner?.unsupported("UnsupportedLengthAcrossPositionDomains");
                        processing.fail(crate::ProcessingFailure::FontReplay);
                        return None;
                    }
                }
            }
        }
    }
    let mut fonts_cache: FontsCache = HashMap::new();

    if let Some(replay) = replay {
        if replay.chunks.len() != text_node.chunks.len()
            || replay.styles.len() != text_node.chunks.len()
        {
            processing.fail(crate::ProcessingFailure::FontReplay);
            return None;
        }
    }
    for (chunk_index, chunk) in text_node.chunks.iter().enumerate() {
        if replay.is_some_and(|value| value.styles[chunk_index].len() != chunk.spans.len()) {
            processing.fail(crate::ProcessingFailure::FontReplay);
            return None;
        }
        for (span_index, span) in chunk.spans.iter().enumerate() {
            geometry_checkpoint(processing, 0)
                .map_err(|e| processing.record_resource(e))
                .ok()?;
            let font = if let (Some(prepared), Some(owner)) = (replay, replay_owner) {
                load_replay_font(
                    prepared.styles[chunk_index][span_index],
                    owner,
                    fontdb,
                    processing,
                )
            } else {
                (resolver.select_font)(&span.font, fontdb)
                    .and_then(|id| fontdb.load_font(id, &span.font.variations, None, processing))
            };
            geometry_checkpoint(processing, 0)
                .map_err(|e| processing.record_resource(e))
                .ok()?;
            if let Some(font) = font {
                fonts_cache.insert(span as *const TextSpan, font);
            } else if replay.is_some() {
                processing.fail(crate::ProcessingFailure::FontReplay);
                return None;
            }
        }
    }

    let decoration_plan = if let Some(prepared) = replay.filter(|p| p.logical.is_some()) {
        match DecorationPlan::build(text_node, prepared, &fonts_cache, replay_owner?, processing) {
            Ok(plan) => Some(plan),
            Err(DecorationError::Resource(error)) => {
                processing.record_resource(error);
                return None;
            }
            Err(DecorationError::Domain(reason)) => {
                if processing.failed() {
                    return None;
                }
                let _ = replay_owner?.unsupported(reason);
                processing.fail(crate::ProcessingFailure::FontReplay);
                return None;
            }
        }
    } else {
        None
    };
    let mut spans = TreeVec::new(processing);
    let mut char_offset = 0;
    let mut byte_offset = 0;
    let mut last_x = 0.0;
    let mut last_y = 0.0;
    let mut bbox = BBox::default();
    for (chunk_index, chunk) in text_node.chunks.iter().enumerate() {
        let chunk_byte_start = byte_offset;
        byte_offset += chunk.text.len();
        let (x, y) = match chunk.text_flow {
            TextFlow::Linear => (chunk.x.unwrap_or(last_x), chunk.y.unwrap_or(last_y)),
            TextFlow::Path(_) => (0.0, 0.0),
        };

        let mut clusters = if let (Some(prepared), Some(owner)) = (replay, replay_owner) {
            match process_replay_chunk(
                chunk,
                &prepared.chunks[chunk_index],
                &prepared.styles[chunk_index],
                prepared.logical.as_deref(),
                chunk_byte_start,
                owner,
                fontdb,
                processing,
            ) {
                Some(value) => value,
                None => {
                    processing.fail(crate::ProcessingFailure::FontReplay);
                    return None;
                }
            }
        } else {
            match process_chunk(chunk, &fonts_cache, resolver, fontdb, processing) {
                Ok(value) => value,
                Err(error) => {
                    processing.record_resource(error);
                    return None;
                }
            }
        };
        if clusters.is_empty() {
            char_offset += chunk.text.chars().count();
            continue;
        }

        if replay.is_some_and(|prepared| prepared.logical.is_some()) {
            let cache = representation_cache.as_deref_mut()?;
            let proof = replay?.logical.as_ref()?;
            for cluster in &mut clusters {
                if proof.checkpoint(1).is_err() {
                    processing.fail(crate::ProcessingFailure::FontReplay);
                    return None;
                }
                for glyph in &mut cluster.glyphs {
                    if proof.checkpoint(1).is_err() {
                        processing.fail(crate::ProcessingFailure::FontReplay);
                        return None;
                    }
                    let source_span = chunk.spans.get(glyph.source_span?)?;
                    glyph.local_representation =
                        Some(super::flatten::select_local(cache, glyph, source_span)?);
                }
                let all_profile = chunk.text[cluster.byte_idx.value()..cluster.source_end]
                    .chars()
                    .all(harfrust::notepads_markdown_probe::shaping_suppression_candidate);
                let zero = cluster
                    .glyphs
                    .iter()
                    .all(|glyph| glyph.raw_x_advance == 0 && glyph.raw_y_advance == 0);
                let no_ink = cluster.glyphs.iter().all(|glyph| {
                    glyph
                        .local_representation
                        .as_ref()
                        .is_some_and(|local| local.ink() == super::flatten::LocalInk::NoInk)
                });
                cluster.spacing_eligible = !(all_profile && zero && no_ink);
            }
        }
        apply_writing_mode(text_node.writing_mode, &mut clusters);
        apply_letter_spacing(chunk, &mut clusters);
        apply_word_spacing(chunk, &mut clusters);

        if let Some(prepared) = replay.filter(|p| p.logical.is_some()) {
            if let Err(reason) = apply_checked_length_adjust(
                text_node,
                chunk,
                char_offset,
                &mut clusters,
                prepared.logical.as_deref().unwrap(),
                replay_owner?,
            ) {
                let _ = replay_owner?.unsupported(reason);
                processing.fail(crate::ProcessingFailure::FontReplay);
                return None;
            }
        } else {
            apply_length_adjust(chunk, &mut clusters);
        }
        let curr_pos = resolve_clusters_positions(
            text_node,
            chunk,
            char_offset,
            text_node.writing_mode,
            &fonts_cache,
            &mut clusters,
            processing,
        );
        let mut curr_pos = match curr_pos {
            Ok(value) => value,
            Err(error) => {
                processing.record_resource(error);
                return None;
            }
        };

        let mut text_ts = Transform::default();
        if text_node.writing_mode == WritingMode::TopToBottom {
            if let TextFlow::Linear = chunk.text_flow {
                text_ts = text_ts.pre_rotate_at(90.0, x, y);
            }
        }

        let projection_indices = if replay.is_some_and(|p| p.logical.is_some()) {
            let mut lists = vec![Vec::new(); chunk.spans.len()];
            for (index, cluster) in clusters.iter().enumerate() {
                for glyph in &cluster.glyphs {
                    if let Some(span) = glyph.source_span {
                        if lists[span].last() != Some(&index) {
                            lists[span].push(index);
                        }
                    }
                }
                if cluster.glyphs.is_empty() {
                    let span = chunk
                        .spans
                        .partition_point(|span| span.end <= cluster.byte_idx.value());
                    if span < lists.len() {
                        lists[span].push(index);
                    }
                }
            }
            Some(lists)
        } else {
            None
        };
        let decoration_cluster_indices = decoration_plan
            .as_ref()
            .map(|plan| plan.cluster_indices(chunk, &clusters));
        let mut decoration_emitted = HashSet::new();
        for (span_index, span) in chunk.spans.iter().enumerate() {
            if let Err(error) = geometry_checkpoint(processing, 1) {
                processing.record_resource(error);
                return None;
            }
            let font = match fonts_cache.get(&(span as *const TextSpan)) {
                Some(v) => v,
                None => continue,
            };

            let decoration_spans = match collect_decoration_spans(span, &clusters, processing) {
                Ok(value) => value,
                Err(error) => {
                    processing.record_resource(error);
                    return None;
                }
            };

            let mut span_ts = text_ts;
            span_ts = span_ts.pre_translate(x, y);
            if let TextFlow::Linear = chunk.text_flow {
                let shift = resolve_baseline(span, font, text_node.writing_mode);

                // In case of a horizontal flow, shift transform and not clusters,
                // because clusters can be rotated and an additional shift will lead
                // to invalid results.
                span_ts = span_ts.pre_translate(0.0, shift);
            }

            let (underline, overline, line_through) = if let Some(prepared) =
                replay.filter(|p| p.logical.is_some())
            {
                match checked_owner_decorations(
                    text_node,
                    chunk,
                    chunk_index,
                    span,
                    &clusters,
                    decoration_plan.as_ref().unwrap(),
                    decoration_cluster_indices.as_ref().unwrap(),
                    prepared,
                    replay_owner?,
                    text_ts.pre_translate(x, y),
                    &mut decoration_emitted,
                    processing,
                ) {
                    Ok([underline, overline, line_through]) => (underline, overline, line_through),
                    Err(DecorationError::Resource(error)) => {
                        processing.record_resource(error);
                        return None;
                    }
                    Err(DecorationError::Domain(reason)) => {
                        if processing.failed() {
                            return None;
                        }
                        let _ = replay_owner?.unsupported(reason);
                        processing.fail(crate::ProcessingFailure::FontReplay);
                        return None;
                    }
                }
            } else {
                let mut underline = None;
                let mut overline = None;
                let mut line_through = None;

                if let Some(decoration) = span.decoration.underline.try_copy_in(processing).ok()? {
                    // TODO: No idea what offset should be used for top-to-bottom layout.
                    // There is
                    // https://www.w3.org/TR/css-text-decor-3/#text-underline-position-property
                    // but it doesn't go into details.
                    let offset = match text_node.writing_mode {
                        WritingMode::LeftToRight => -font.underline_position(span.font_size.get()),
                        WritingMode::TopToBottom => font.height(span.font_size.get()) / 2.0,
                    };

                    if let Some(path) = convert_decoration(
                        offset,
                        span,
                        font,
                        decoration,
                        &decoration_spans,
                        span_ts,
                        processing,
                    ) {
                        bbox = bbox.expand(path.data.bounds());
                        underline = Some(path);
                    }
                }

                if let Err(error) = geometry_checkpoint(processing, 0) {
                    processing.record_resource(error);
                    return None;
                }
                if let Some(decoration) = span.decoration.overline.try_copy_in(processing).ok()? {
                    let offset = match text_node.writing_mode {
                        WritingMode::LeftToRight => -font.ascent(span.font_size.get()),
                        WritingMode::TopToBottom => -font.height(span.font_size.get()) / 2.0,
                    };

                    if let Some(path) = convert_decoration(
                        offset,
                        span,
                        font,
                        decoration,
                        &decoration_spans,
                        span_ts,
                        processing,
                    ) {
                        bbox = bbox.expand(path.data.bounds());
                        overline = Some(path);
                    }
                }

                if let Err(error) = geometry_checkpoint(processing, 0) {
                    processing.record_resource(error);
                    return None;
                }
                if let Some(decoration) =
                    span.decoration.line_through.try_copy_in(processing).ok()?
                {
                    let offset = match text_node.writing_mode {
                        WritingMode::LeftToRight => {
                            -font.line_through_position(span.font_size.get())
                        }
                        WritingMode::TopToBottom => 0.0,
                    };

                    if let Some(path) = convert_decoration(
                        offset,
                        span,
                        font,
                        decoration,
                        &decoration_spans,
                        span_ts,
                        processing,
                    ) {
                        bbox = bbox.expand(path.data.bounds());
                        line_through = Some(path);
                    }
                }

                if let Err(error) = geometry_checkpoint(processing, 0) {
                    processing.record_resource(error);
                    return None;
                }
                (underline, overline, line_through)
            };
            drop(decoration_spans);
            if let Err(error) = geometry_checkpoint(processing, 0) {
                processing.record_resource(error);
                return None;
            }
            for decoration in [&underline, &overline, &line_through].into_iter().flatten() {
                bbox = bbox.expand(decoration.data.bounds());
            }

            let mut fill = span.fill.try_copy_in(processing).ok()?;
            if let Some(ref mut fill) = fill {
                // The `fill-rule` should be ignored.
                // https://www.w3.org/TR/SVG2/text.html#TextRenderingOrder
                //
                // 'Since the fill-rule property does not apply to SVG text elements,
                // the specific order of the subpaths within the equivalent path does not matter.'
                fill.rule = FillRule::NonZero;
            }

            if let Some((span_fragments, span_bbox)) = convert_span(
                span,
                &clusters,
                span_ts,
                projection_indices
                    .as_ref()
                    .map(|lists| lists[span_index].as_slice()),
                processing,
            ) {
                bbox = bbox.expand(span_bbox);

                let mut positioned_glyphs = TreeVec::new(processing);
                for mut gc in span_fragments {
                    let unit_ts = if gc.inline_scale == 1.0 {
                        gc.transform
                    } else {
                        Transform::from_scale(gc.inline_scale, 1.0).post_concat(gc.transform)
                    };
                    let cluster_ts = gc.path_transform.post_concat(unit_ts);
                    let native_sideways_ts = Transform::from_rotate(-90.0).post_concat(unit_ts);
                    for pg in gc.glyphs.iter_mut() {
                        owner::check(processing, 1).ok()?;
                        // Only the actual Native sideways origin replaces the
                        // normal-origin pivot. HB offsets stay in glyph_ts.
                        pg.cluster_ts = if pg.placement_policy
                            == crate::ReplayPlacementPolicy::NativeSidewaysInverse90
                        {
                            native_sideways_ts
                        } else {
                            cluster_ts
                        };
                        pg.span_ts = if let Some(font) = pg.layout_font.as_ref() {
                            if matches!(chunk.text_flow, TextFlow::Linear) {
                                text_ts.pre_translate(x, y).pre_translate(
                                    0.0,
                                    resolve_baseline_values(
                                        &span.baseline_shift,
                                        span.dominant_baseline,
                                        span.alignment_baseline,
                                        font,
                                        text_node.writing_mode,
                                        pg.font_size,
                                    ),
                                )
                            } else {
                                span_ts
                            }
                        } else {
                            span_ts
                        };
                        if let Some(writing) = pg.writing_transform {
                            pg.cluster_ts = writing.post_concat(unit_ts);
                        }
                        if matches!(chunk.text_flow, TextFlow::Path(_)) {
                            if let Some(font) = pg.layout_font.as_ref() {
                                pg.cluster_ts = pg.cluster_ts.pre_translate(
                                    0.0,
                                    resolve_baseline_values(
                                        &span.baseline_shift,
                                        span.dominant_baseline,
                                        span.alignment_baseline,
                                        font,
                                        text_node.writing_mode,
                                        pg.font_size,
                                    ),
                                );
                            }
                        }
                    }

                    positioned_glyphs
                        .try_append(&mut gc.glyphs, processing)
                        .ok()?;
                    drop(gc);
                    owner::check(processing, 0).ok()?;
                }

                spans
                    .try_push(
                        Span {
                            computed_color: span.computed_color,
                            fill,
                            stroke: span.stroke.try_copy_in(processing).ok()?,
                            paint_order: span.paint_order,
                            font_size: span.font_size,
                            variations: span.font.variations.try_copy_in(processing).ok()?,
                            font_optical_sizing: span.font_optical_sizing,
                            visible: span.visible,
                            positioned_glyphs,
                            underline,
                            overline,
                            line_through,
                        },
                        processing,
                    )
                    .ok()?;
            }
            if let Err(error) = geometry_checkpoint(processing, 0) {
                processing.record_resource(error);
                return None;
            }
        }

        char_offset += chunk.text.chars().count();

        if text_node.writing_mode == WritingMode::TopToBottom {
            if let TextFlow::Linear = chunk.text_flow {
                std::mem::swap(&mut curr_pos.0, &mut curr_pos.1);
            }
        }

        last_x = x + curr_pos.0;
        last_y = y + curr_pos.1;
    }

    let bbox = bbox.to_non_zero_rect()?;

    Some((spans, bbox))
}

fn convert_span(
    span: &TextSpan,
    clusters: &[GlyphCluster],
    text_ts: Transform,
    indices: Option<&[usize]>,
    processing: &crate::ProcessingContext,
) -> Option<(Vec<GlyphCluster>, NonZeroRect)> {
    match convert_span_inner(span, clusters, text_ts, indices, processing) {
        Ok(value) => value,
        Err(error) => {
            processing.record_resource(error);
            None
        }
    }
}

fn convert_span_inner(
    span: &TextSpan,
    clusters: &[GlyphCluster],
    text_ts: Transform,
    indices: Option<&[usize]>,
    processing: &crate::ProcessingContext,
) -> GeometryResult<Option<(Vec<GlyphCluster>, NonZeroRect)>> {
    let candidate = (|| {
        let mut span_clusters = vec![];
        let mut bboxes_builder =
            geometry_builder(processing, markdown_svg_budget::ResourceKind::Scratch);

        for ordinal in 0..indices.map_or(clusters.len(), |indices| indices.len()) {
            geometry_checkpoint(processing, 1)?;
            let cluster = &clusters[indices.map_or(ordinal, |indices| indices[ordinal])];
            if !cluster.visible {
                continue;
            }

            let mut member = false;
            for glyph in &cluster.glyphs {
                geometry_checkpoint(processing, 1)?;
                if glyph.source_range.start >= span.start && glyph.source_range.start < span.end {
                    member = true;
                    break;
                }
            }
            if !member && !(cluster.glyphs.is_empty() && span_contains(span, cluster.byte_idx)) {
                continue;
            }
            let mut projected = cluster.try_copy_in(processing)?;
            // Keep the existing order and capacity without another staging Vec.
            // The fallible scan stops immediately; retain's infallible closure
            // cannot transport a work error before visiting the next glyph.
            let mut retained = 0;
            for at in 0..projected.glyphs.len() {
                geometry_checkpoint(processing, 1)?;
                let glyph = &projected.glyphs[at];
                if glyph.source_range.start >= span.start && glyph.source_range.start < span.end {
                    projected.glyphs.swap(retained, at);
                    retained += 1;
                }
            }
            projected.glyphs.try_truncate(retained, processing)?;
            geometry_checkpoint(processing, 0)?;
            if !projected.glyphs.is_empty()
                || (cluster.glyphs.is_empty() && span_contains(span, cluster.byte_idx))
            {
                span_clusters.push(projected);
            }

            let mut advance = cluster.advance;
            if advance <= 0.0 {
                advance = 1.0;
            }

            // We have to calculate text bbox using font metrics and not glyph shape.
            if let Some(r) = NonZeroRect::from_xywh(0.0, -cluster.ascent, advance, cluster.height())
            {
                if let Some(r) = r.transform(cluster.transform()) {
                    bboxes_builder.try_push_rect(r.to_rect())?;
                }
            }
        }

        let Some(bboxes) = bboxes_builder.try_finish()? else {
            return Ok(None);
        };
        let Some(bboxes) = geometry_transform(
            bboxes,
            text_ts,
            processing,
            markdown_svg_budget::ResourceKind::Scratch,
        )?
        else {
            return Ok(None);
        };
        let bbox = geometry_bounds(&bboxes, processing)?.and_then(|bbox| bbox.to_non_zero_rect());
        drop(bboxes);
        geometry_checkpoint(processing, 0)?;
        Ok(bbox.map(|bbox| (span_clusters, bbox)))
    })();
    geometry_checkpoint(processing, 0)?;
    candidate
}

struct DecorationPlan {
    sources: HashMap<usize, crate::SharedOwner<crate::tree::DecorationOwnerStyle>>,
    ranges: HashMap<(usize, usize), std::ops::Range<usize>>,
    metrics: HashMap<usize, Option<(ResolvedFont, f32, bool)>>,
}
impl DecorationPlan {
    fn build(
        text: &Text,
        prepared: &crate::ReplayText,
        fonts: &FontsCache,
        owner: &dyn crate::FontReplay,
        processing: &crate::ProcessingContext,
    ) -> Result<Self, DecorationError> {
        let count = text.chunks.iter().map(|c| c.spans.len()).sum::<usize>();
        let depth = text
            .chunks
            .iter()
            .flat_map(|c| &c.spans)
            .map(|s| s.source_ancestors.len())
            .max()
            .unwrap_or(0);
        let bytes = count
            .checked_mul((depth + 1).checked_mul(96).ok_or("DecorationCapacity")?)
            .ok_or("DecorationCapacity")?;
        let _retained = owner
            .reserve_replay_bytes(bytes)
            .map_err(|_| "DecorationAdmission")?;
        let mut sources = HashMap::new();
        for span in text.chunks.iter().flat_map(|c| &c.spans) {
            for source in span.decoration_sources.iter().flatten() {
                if !sources.contains_key(&source.owner) {
                    sources.insert(source.owner, source.try_alias_in(processing)?);
                }
            }
        }
        let mut members: HashMap<usize, Vec<(usize, usize)>> = HashMap::new();
        let mut ranges: HashMap<(usize, usize), std::ops::Range<usize>> = HashMap::new();
        for (domain, chunk) in text.chunks.iter().enumerate() {
            for (index, span) in chunk.spans.iter().enumerate() {
                prepared
                    .logical
                    .as_ref()
                    .unwrap()
                    .checkpoint(1)
                    .map_err(|_| "DecorationWork")?;
                for identity in &span.source_ancestors {
                    if sources.contains_key(identity) {
                        members.entry(*identity).or_default().push((domain, index));
                        ranges
                            .entry((*identity, domain))
                            .and_modify(|r| {
                                r.start = r.start.min(span.start);
                                r.end = r.end.max(span.end);
                            })
                            .or_insert(span.start..span.end);
                    }
                }
            }
        }
        let mut actual = HashMap::new();
        for (domain, glyphs) in prepared.chunks.iter().enumerate() {
            for glyph in glyphs {
                prepared
                    .logical
                    .as_ref()
                    .unwrap()
                    .checkpoint(1)
                    .map_err(|_| "DecorationWork")?;
                let id = glyph.identity.ok_or("DecorationIdentity")?;
                let span = text.chunks[domain]
                    .spans
                    .get(id.visual_span)
                    .ok_or("DecorationIdentity")?;
                if glyph.face.id != prepared.styles[domain][id.visual_span].id
                    || !(glyph.start <= span.start && span.start < glyph.end)
                {
                    continue;
                }
                let font = fonts
                    .get(&(span as *const TextSpan))
                    .ok_or("DecorationFont")?;
                actual.entry((domain, id.visual_span)).or_insert_with(|| {
                    (
                        font.clone(),
                        id.semantic_em * glyph.fallback_scale,
                        span.visible,
                    )
                });
            }
        }
        let same_scopes = |a: &[crate::BidiScope], b: &[crate::BidiScope]| {
            a.len() == b.len()
                && a.iter().zip(b).all(|(a, b)| {
                    a.identity == b.identity && a.mode == b.mode && a.direction == b.direction
                })
        };
        let mut metrics = HashMap::new();
        for (&identity, source) in &sources {
            let metric = members.get(&identity).and_then(|members| {
                members.iter().find_map(|&(domain, index)| {
                    let span = &text.chunks[domain].spans[index];
                    if span.font != source.font
                        || span.font_size != source.size
                        || span.font_optical_sizing != source.optical
                        || !span.language.eq_ignore_ascii_case(&source.language)
                        || span.direction != source.direction
                        || !same_scopes(&span.bidi_scopes, &source.scopes)
                    {
                        None
                    } else {
                        actual.get(&(domain, index)).cloned()
                    }
                })
            });
            metrics.insert(identity, metric);
        }
        Ok(Self {
            sources,
            ranges,
            metrics,
        })
    }
    fn cluster_indices(
        &self,
        chunk: &TextChunk,
        clusters: &[GlyphCluster],
    ) -> HashMap<usize, Vec<usize>> {
        let mut output: HashMap<usize, Vec<usize>> = HashMap::new();
        for (index, cluster) in clusters.iter().enumerate() {
            let mut source = |span: &TextSpan| {
                for identity in &span.source_ancestors {
                    if self.sources.contains_key(identity) {
                        let indices = output.entry(*identity).or_default();
                        if indices.last() != Some(&index) {
                            indices.push(index);
                        }
                    }
                }
            };
            if cluster.glyphs.is_empty() {
                if let Some(span) = chunk_span_at(chunk, cluster.byte_idx) {
                    source(span);
                }
            } else {
                for glyph in &cluster.glyphs {
                    if let Some(index) = glyph.source_span {
                        source(&chunk.spans[index]);
                    }
                }
            }
        }
        output
    }
}
enum DecorationError {
    Domain(&'static str),
    Resource(markdown_svg_budget::Error),
}
impl From<&'static str> for DecorationError {
    fn from(value: &'static str) -> Self {
        Self::Domain(value)
    }
}
impl From<markdown_svg_budget::Error> for DecorationError {
    fn from(value: markdown_svg_budget::Error) -> Self {
        Self::Resource(value)
    }
}

fn checked_owner_decorations(
    text: &Text,
    chunk: &TextChunk,
    domain: usize,
    span: &TextSpan,
    clusters: &[GlyphCluster],
    plan: &DecorationPlan,
    cluster_indices: &HashMap<usize, Vec<usize>>,
    prepared: &crate::ReplayText,
    owner: &dyn crate::FontReplay,
    text_ts: Transform,
    emitted: &mut HashSet<(usize, usize)>,
    processing: &crate::ProcessingContext,
) -> Result<[Option<Path>; 3], DecorationError> {
    let candidate = (|| {
        let mut output = [None, None, None];
        for kind in 0..3 {
            geometry_checkpoint(processing, 1)?;
            let Some(source) = span.decoration_sources[kind].as_ref() else {
                continue;
            };
            if !emitted.insert((source.owner, kind)) {
                continue;
            }
            prepared
                .logical
                .as_ref()
                .unwrap()
                .checkpoint(1)
                .map_err(|_| "DecorationWork")?;
            let range = plan
                .ranges
                .get(&(source.owner, domain))
                .ok_or("DecorationScope")?;
            let scope_start = range.start;
            let scope_end = range.end;
            let indices = cluster_indices
                .get(&source.owner)
                .map_or(&[][..], Vec::as_slice);
            let _intervals = owner
                .reserve_replay_bytes(
                    indices
                        .len()
                        .checked_mul(
                            (if processing.resources().is_some() {
                                0
                            } else {
                                2 * size_of::<DecorationSpan>()
                            }) + 128,
                        )
                        .ok_or("DecorationCapacity")?,
                )
                .map_err(|_| "DecorationAdmission")?;
            let mut intervals =
                GeometryVec::<DecorationSpan>::with_capacity(processing, indices.len())?;
            for position in 0..indices.len() {
                geometry_checkpoint(processing, 1)?;
                let index = indices[position];
                let cluster = &clusters[index];
                prepared
                    .logical
                    .as_ref()
                    .unwrap()
                    .checkpoint(1)
                    .map_err(|_| "DecorationWork")?;
                if cluster.source_end <= scope_start || cluster.byte_idx.value() >= scope_end {
                    continue;
                }
                let mut push = |start: f32, width: f32| -> GeometryResult<()> {
                    geometry_checkpoint(processing, 1)?;
                    if width <= 0.0 {
                        return Ok(());
                    }
                    let transform = cluster.transform.pre_translate(start, 0.0);
                    if let Some(last) = intervals.last_mut() {
                        let end_x = last.transform.tx + last.transform.sx * last.width;
                        let end_y = last.transform.ty + last.transform.ky * last.width;
                        if last.transform.sx == transform.sx
                            && last.transform.sy == transform.sy
                            && last.transform.kx == transform.kx
                            && last.transform.ky == transform.ky
                            && end_x == transform.tx
                            && end_y == transform.ty
                        {
                            last.width += width;
                            return Ok(());
                        }
                    }
                    intervals.push(DecorationSpan { width, transform })?;
                    Ok(())
                };
                if scope_start <= cluster.byte_idx.value() && cluster.source_end <= scope_end {
                    let trailing = if cluster.source_end == scope_end {
                        cluster.spacing_after
                    } else {
                        0.0
                    };
                    push(0.0, cluster.advance - trailing)?;
                } else {
                    for glyph in &cluster.glyphs {
                        geometry_checkpoint(processing, 1)?;
                        if glyph.source_span.is_some_and(|index| {
                            chunk.spans[index].source_ancestors.contains(&source.owner)
                        }) {
                            push(
                                glyph.progression_start * cluster.inline_scale,
                                glyph.progression_advance * cluster.inline_scale,
                            )?;
                        }
                    }
                }
            }
            if intervals.is_empty() {
                continue;
            }
            let Some((font, em, visible)) =
                plan.metrics.get(&source.owner).and_then(|v| v.as_ref())
            else {
                return Err("TypedUnsupportedDecorationOwnerMetricsUnavailable".into());
            };
            let em = *em;
            let visible = *visible;
            let mut transform = text_ts;
            if matches!(chunk.text_flow, TextFlow::Linear) {
                transform = transform.pre_translate(
                    0.0,
                    resolve_baseline_values(
                        &source.baseline_shift,
                        source.dominant,
                        source.alignment,
                        font,
                        text.writing_mode,
                        em,
                    ),
                );
            }
            let decoration = match kind {
                0 => span.decoration.underline.try_copy_in(processing)?,
                1 => span.decoration.overline.try_copy_in(processing)?,
                _ => span.decoration.line_through.try_copy_in(processing)?,
            };
            let Some(decoration) = decoration else {
                return Err("DecorationOwnerSource".into());
            };
            let offset = match (kind, text.writing_mode) {
                (0, WritingMode::LeftToRight) => -font.underline_position(em),
                (0, _) => font.height(em) / 2.0,
                (1, WritingMode::LeftToRight) => -font.ascent(em),
                (1, _) => -font.height(em) / 2.0,
                (_, WritingMode::LeftToRight) => -font.line_through_position(em),
                (_, _) => 0.0,
            };
            let _retained = owner
                .reserve_replay_bytes(
                    intervals
                        .len()
                        .checked_mul(
                            (if processing.resources().is_some() {
                                0
                            } else {
                                size_of::<DecorationSpan>()
                            }) + 128,
                        )
                        .ok_or("DecorationCapacity")?,
                )
                .map_err(|_| "DecorationAdmission")?;
            output[kind] = convert_decoration_values(
                offset, em, visible, font, decoration, &intervals, transform, processing,
            )?;
            drop(intervals);
            geometry_checkpoint(processing, 0)?;
        }
        Ok(output)
    })();
    geometry_checkpoint(processing, 0)?;
    candidate
}

fn collect_decoration_spans(
    span: &TextSpan,
    clusters: &[GlyphCluster],
    processing: &crate::ProcessingContext,
) -> GeometryResult<GeometryVec<DecorationSpan>> {
    let mut spans = GeometryVec::new(processing);

    let mut started = false;
    let mut width = 0.0;
    let mut transform = Transform::default();

    for cluster in clusters {
        geometry_checkpoint(processing, 1)?;
        if span_contains(span, cluster.byte_idx) {
            if started && cluster.has_relative_shift {
                started = false;
                spans.push(DecorationSpan { width, transform })?;
            }

            if !started {
                width = cluster.advance;
                started = true;
                transform = cluster.transform;
            } else {
                width += cluster.advance;
            }
        } else if started {
            spans.push(DecorationSpan { width, transform })?;
            started = false;
        }
    }

    if started {
        spans.push(DecorationSpan { width, transform })?;
    }

    geometry_checkpoint(processing, 0)?;
    Ok(spans)
}

pub(crate) fn convert_decoration(
    dy: f32,
    span: &TextSpan,
    font: &ResolvedFont,
    decoration: TextDecorationStyle,
    decoration_spans: &[DecorationSpan],
    transform: Transform,
    processing: &crate::ProcessingContext,
) -> Option<Path> {
    let candidate = convert_decoration_values(
        dy,
        span.font_size.get(),
        span.visible,
        font,
        decoration,
        decoration_spans,
        transform,
        processing,
    );
    match candidate {
        Ok(value) => value,
        Err(error) => {
            processing.record_resource(error);
            None
        }
    }
}
fn convert_decoration_values(
    dy: f32,
    size: f32,
    visible: bool,
    font: &ResolvedFont,
    mut decoration: TextDecorationStyle,
    decoration_spans: &[DecorationSpan],
    transform: Transform,
    processing: &crate::ProcessingContext,
) -> GeometryResult<Option<Path>> {
    let candidate = (|| {
        geometry_checkpoint(processing, 0)?;
        debug_assert!(!decoration_spans.is_empty());

        let thickness = font.underline_thickness(size);

        let mut builder =
            geometry_builder(processing, markdown_svg_budget::ResourceKind::Geometry);
        for dec_span in decoration_spans {
            geometry_checkpoint(processing, 1)?;
            let rect =
                match NonZeroRect::from_xywh(0.0, -thickness / 2.0, dec_span.width, thickness) {
                    Some(v) => v,
                    None => {
                        log::warn!("a decoration span has a malformed bbox");
                        continue;
                    }
                };

            let ts = dec_span.transform.pre_translate(0.0, dy);

            let path = match processing.resources() {
                Some(resources) => tiny_skia_path::PathBuilder::try_from_rect(
                    rect.to_rect(),
                    resources,
                    markdown_svg_budget::ResourceKind::Scratch,
                )?,
                None => tiny_skia_path::PathBuilder::from_rect(rect.to_rect()),
            };
            let Some(path) = geometry_transform(
                path,
                ts,
                processing,
                markdown_svg_budget::ResourceKind::Scratch,
            )?
            else {
                continue;
            };
            let copied = builder.try_push_path(&path);
            drop(path);
            geometry_checkpoint(processing, 0)?;
            copied?;
        }

        let Some(path_data) = builder.try_finish()? else {
            return Ok(None);
        };
        let Some(path_data) = geometry_transform(
            path_data,
            transform,
            processing,
            markdown_svg_budget::ResourceKind::Geometry,
        )?
        else {
            return Ok(None);
        };
        if let Some(resources) = processing.resources() {
            return Path::try_new(
                "",
                visible,
                decoration.fill.take(),
                decoration.stroke.take(),
                PaintOrder::default(),
                ShapeRendering::default(),
                path_data,
                Transform::default(),
                resources,
            );
        }
        Ok(Path::new(
            "",
            visible,
            decoration.fill.take(),
            decoration.stroke.take(),
            PaintOrder::default(),
            ShapeRendering::default(),
            Arc::new(path_data),
            Transform::default(),
        ))
    })();
    geometry_checkpoint(processing, 0)?;
    candidate
}

/// A text decoration span.
///
/// Basically a horizontal line, that will be used for underline, overline and line-through.
/// It doesn't have a height, since it depends on the Font metrics.
#[derive(Clone, Copy)]
pub(crate) struct DecorationSpan {
    pub(crate) width: f32,
    pub(crate) transform: Transform,
}

/// Resolves clusters positions.
///
/// Mainly sets the `transform` property.
///
/// Returns the last text position. The next text chunk should start from that position.
fn resolve_clusters_positions(
    text: &Text,
    chunk: &TextChunk,
    char_offset: usize,
    writing_mode: WritingMode,
    fonts_cache: &FontsCache,
    clusters: &mut [GlyphCluster],
    processing: &crate::ProcessingContext,
) -> GeometryResult<(f32, f32)> {
    geometry_checkpoint(processing, 0)?;
    match chunk.text_flow {
        TextFlow::Linear => Ok(resolve_clusters_positions_horizontal(
            text,
            chunk,
            char_offset,
            writing_mode,
            clusters,
        )),
        TextFlow::Path(ref path) => resolve_clusters_positions_path(
            text,
            chunk,
            char_offset,
            path,
            writing_mode,
            fonts_cache,
            clusters,
            processing,
        ),
    }
}

fn clusters_length(clusters: &[GlyphCluster]) -> f32 {
    clusters.iter().fold(0.0, |w, cluster| w + cluster.advance)
}

fn resolve_relative_shifts(
    text: &Text,
    chunk: &TextChunk,
    offset: usize,
    clusters: &[GlyphCluster],
) -> (Vec<(f32, f32)>, Vec<usize>) {
    // `dx` and `dy` shift a character and all the characters after it in logical order,
    // while the clusters are in visual order, which differs in right-to-left text.
    let mut logical_order: Vec<usize> = (0..clusters.len()).collect();
    logical_order.sort_by_key(|&i| clusters[i].byte_idx.value());

    let mut shifts = vec![(0.0, 0.0); clusters.len()];
    let (mut shift_x, mut shift_y) = (0.0, 0.0);
    let mut source = chunk.text.char_indices().enumerate().peekable();
    for &i in &logical_order {
        let head = clusters[i].byte_idx.value();
        while source.peek().is_some_and(|(_, entry)| entry.0 <= head) {
            let (scalar, _) = source.next().unwrap();
            let cp = chunk
                .source_addresses
                .get(scalar)
                .copied()
                .unwrap_or(offset + scalar);
            shift_x += text.dx.get(cp).copied().unwrap_or(0.0);
            shift_y += text.dy.get(cp).copied().unwrap_or(0.0);
        }
        shifts[i] = (shift_x, shift_y);
    }

    (shifts, logical_order)
}

fn resolve_clusters_positions_horizontal(
    text: &Text,
    chunk: &TextChunk,
    offset: usize,
    writing_mode: WritingMode,
    clusters: &mut [GlyphCluster],
) -> (f32, f32) {
    let (mut shifts, logical_order) = resolve_relative_shifts(text, chunk, offset, clusters);
    let first = logical_order[0];
    let last = logical_order[logical_order.len() - 1];

    // Vertical text is laid out horizontally and rotated by 90 degrees afterwards.
    if writing_mode == WritingMode::TopToBottom {
        for shift in &mut shifts {
            *shift = (shift.1, -shift.0);
        }
    }

    for i in 1..clusters.len() {
        if shifts[i] != shifts[i - 1] {
            clusters[i].has_relative_shift = true;
        }
    }

    let mut left = f32::INFINITY;
    let mut right = f32::NEG_INFINITY;
    let mut advance = 0.0;
    for (cluster, (shift_x, _)) in clusters.iter().zip(&shifts) {
        let start = advance + shift_x;
        let end = start + cluster.advance - cluster.text_length_spacing;
        left = left.min(start).min(end);
        right = right.max(start).max(end);
        advance += cluster.advance;
    }

    let mut x = shifts[first].0 - left
        + process_anchor(
            if chunk.spans.first().is_some_and(|span| span.direction == 1) {
                match chunk.anchor {
                    TextAnchor::Start => TextAnchor::End,
                    TextAnchor::End => TextAnchor::Start,
                    value => value,
                }
            } else {
                chunk.anchor
            },
            right - left,
        );

    for (cluster, (shift_x, shift_y)) in clusters.iter_mut().zip(&shifts) {
        let scalar = cluster.byte_idx.code_point_at(&chunk.text);
        let cp = chunk
            .source_addresses
            .get(scalar)
            .copied()
            .unwrap_or(offset + scalar);

        cluster.transform = cluster.transform.pre_translate(x + shift_x, *shift_y);

        if let Some(angle) = text.rotate.get(cp).cloned() {
            if !angle.approx_zero_ulps(4) {
                cluster.transform = cluster.transform.pre_rotate(angle);
                cluster.has_relative_shift = true;
            }
        }

        x += cluster.advance;
    }

    (x + shifts[last].0, shifts[last].1)
}

// Baseline resolving in SVG is a mess.
// Not only it's poorly documented, but as soon as you start mixing
// `dominant-baseline` and `alignment-baseline` each application/browser will produce
// different results.
//
// For now, resvg simply tries to match Chrome's output and not the mythical SVG spec output.
//
// See `alignment_baseline_shift` method comment for more details.
pub(crate) fn resolve_baseline(
    span: &TextSpan,
    font: &ResolvedFont,
    writing_mode: WritingMode,
) -> f32 {
    resolve_baseline_values(
        &span.baseline_shift,
        span.dominant_baseline,
        span.alignment_baseline,
        font,
        writing_mode,
        span.font_size.get(),
    )
}
fn resolve_baseline_values(
    shifts: &[BaselineShift],
    dominant: DominantBaseline,
    alignment: AlignmentBaseline,
    font: &ResolvedFont,
    writing_mode: WritingMode,
    size: f32,
) -> f32 {
    let mut shift = -resolve_baseline_shift(shifts, font, size);
    if writing_mode == WritingMode::LeftToRight {
        if alignment == AlignmentBaseline::Auto || alignment == AlignmentBaseline::Baseline {
            shift += font.dominant_baseline_shift(dominant, size);
        } else {
            shift += font.alignment_baseline_shift(alignment, size);
        }
    }
    shift
}

fn resolve_baseline_shift(baselines: &[BaselineShift], font: &ResolvedFont, font_size: f32) -> f32 {
    let mut shift = 0.0;
    for baseline in baselines.iter().rev() {
        match baseline {
            BaselineShift::Baseline => {}
            BaselineShift::Subscript => shift -= font.subscript_offset(font_size),
            BaselineShift::Superscript => shift += font.superscript_offset(font_size),
            BaselineShift::Number(n) => shift += n,
        }
    }

    shift
}

fn resolve_clusters_positions_path(
    text: &Text,
    chunk: &TextChunk,
    char_offset: usize,
    path: &TextPath,
    writing_mode: WritingMode,
    fonts_cache: &FontsCache,
    clusters: &mut [GlyphCluster],
    processing: &crate::ProcessingContext,
) -> GeometryResult<(f32, f32)> {
    let candidate = (|| {
        let mut last_x = 0.0;
        let mut last_y = 0.0;

        let (shifts, _) = resolve_relative_shifts(text, chunk, char_offset, clusters);

        // In the text path mode, chunk's x/y coordinates provide an additional offset along the path.
        // The X coordinate is used in a horizontal mode, and Y in vertical.
        let chunk_offset = match writing_mode {
            WritingMode::LeftToRight => chunk.x.unwrap_or(0.0),
            WritingMode::TopToBottom => chunk.y.unwrap_or(0.0),
        };

        let start_offset = chunk_offset
            + path.start_offset
            + process_anchor(
                if chunk.spans.first().is_some_and(|span| span.direction == 1) {
                    match chunk.anchor {
                        TextAnchor::Start => TextAnchor::End,
                        TextAnchor::End => TextAnchor::Start,
                        value => value,
                    }
                } else {
                    chunk.anchor
                },
                clusters_length(clusters),
            );

        let normals = collect_normals_with(
            text,
            clusters,
            &shifts,
            &path.path,
            start_offset,
            processing,
        )?;
        for ((cluster, normal), (_, dy)) in
            clusters.iter_mut().zip(normals.iter()).zip(shifts.iter())
        {
            geometry_checkpoint(processing, 1)?;
            let dy = *dy;
            let (x, y, angle) = match *normal {
                Some(normal) => (normal.x, normal.y, normal.angle),
                None => {
                    // Hide clusters that are outside the text path.
                    cluster.visible = false;
                    continue;
                }
            };

            // We have to break a decoration line for each cluster during text-on-path.
            cluster.has_relative_shift = true;

            let orig_ts = cluster.transform;

            // Clusters should be rotated by the x-midpoint x baseline position.
            let half_width = cluster.width / 2.0;
            cluster.transform = Transform::default();
            cluster.transform = cluster.transform.pre_translate(x - half_width, y);
            cluster.transform = cluster.transform.pre_rotate_at(angle, half_width, 0.0);

            let scalar = cluster.byte_idx.code_point_at(&chunk.text);
            let cp = chunk
                .source_addresses
                .get(scalar)
                .copied()
                .unwrap_or(char_offset + scalar);

            let baseline_shift = if cluster.logical_parent.is_some() {
                0.0
            } else {
                chunk_span_at(chunk, cluster.byte_idx)
                    .map(|span| {
                        let font = match fonts_cache.get(&(span as *const TextSpan)) {
                            Some(v) => v,
                            None => return 0.0,
                        };
                        -resolve_baseline(span, font, writing_mode)
                    })
                    .unwrap_or(0.0)
            };

            // Shift only by `dy` since we already applied `dx`
            // during offset along the path calculation.
            if !dy.approx_zero_ulps(4) || !baseline_shift.approx_zero_ulps(4) {
                let shift = kurbo::Vec2::new(0.0, (dy - baseline_shift) as f64);
                cluster.transform = cluster
                    .transform
                    .pre_translate(shift.x as f32, shift.y as f32);
            }

            if let Some(angle) = text.rotate.get(cp).cloned() {
                if !angle.approx_zero_ulps(4) {
                    cluster.transform = cluster.transform.pre_rotate(angle);
                }
            }

            // The possible `lengthAdjust` transform should be applied after text-on-path positioning.
            cluster.transform = cluster.transform.pre_concat(orig_ts);

            last_x = x + cluster.advance;
            last_y = y;
        }

        drop(normals);
        geometry_checkpoint(processing, 0)?;
        Ok((last_x, last_y))
    })();
    geometry_checkpoint(processing, 0)?;
    candidate
}

pub(crate) fn process_anchor(a: TextAnchor, text_width: f32) -> f32 {
    match a {
        TextAnchor::Start => 0.0, // Nothing.
        TextAnchor::Middle => -text_width / 2.0,
        TextAnchor::End => -text_width,
    }
}

#[derive(Clone, Copy, Debug)]
pub(crate) struct PathNormal {
    pub(crate) x: f32,
    pub(crate) y: f32,
    pub(crate) angle: f32,
}

#[cfg(test)]
fn collect_normals(
    text: &Text,
    clusters: &[GlyphCluster],
    shifts: &[(f32, f32)],
    path: &tiny_skia_path::Path,
    offset: f32,
) -> Vec<Option<PathNormal>> {
    let processing = crate::ProcessingContext::default();
    match collect_normals_with(text, clusters, shifts, path, offset, &processing)
        .expect("ordinary normal collection")
    {
        GeometryVec::Compat(values) => values,
        GeometryVec::Paid(_) => unreachable!("ordinary test has no resource backend"),
    }
}

fn text_path_accuracy(
    transform: Transform,
    processing: &crate::ProcessingContext,
) -> GeometryResult<f64> {
    use markdown_svg_budget::{Error, FailureKind, LimitKind};
    geometry_checkpoint(processing, 1)?;
    let (sx, sy) = transform.get_scale();
    let product = sx * sy;
    let ordinary = 0.5f32 / product.sqrt().max(1.0);
    let components = [transform.sx, transform.kx, transform.ky, transform.sy];
    if components.iter().any(|value| !value.is_finite()) {
        return Err(processing.record_resource(Error::new(FailureKind::Decode, LimitKind::None)));
    }
    let underflow = components
        .iter()
        .any(|value| *value != 0.0 && *value * *value == 0.0)
        || (sx != 0.0 && sy != 0.0 && product == 0.0);
    if sx.is_finite()
        && sy.is_finite()
        && product.is_finite()
        && !underflow
        && ordinary.is_finite()
        && ordinary > 0.0
    {
        return Ok(ordinary as f64);
    }
    geometry_checkpoint(processing, 1)?;
    // The same two groups of matrix components as P::get_scale. Only an
    // invalid f32 intermediate takes this recovery; normal f32 bits stay exact.
    let nx = (transform.sx as f64).hypot(transform.kx as f64);
    let ny = (transform.ky as f64).hypot(transform.sy as f64);
    let accuracy = 0.5 / (nx.sqrt() * ny.sqrt()).max(1.0);
    if !accuracy.is_finite() || accuracy <= 0.0 {
        return Err(processing.record_resource(Error::new(FailureKind::Invariant, LimitKind::None)));
    }
    Ok(accuracy)
}

fn collect_normals_with(
    text: &Text,
    clusters: &[GlyphCluster],
    shifts: &[(f32, f32)],
    path: &tiny_skia_path::Path,
    offset: f32,
    processing: &crate::ProcessingContext,
) -> GeometryResult<GeometryVec<Option<PathNormal>>> {
    let candidate = (|| {
        geometry_checkpoint(processing, 1)?;
        if let Some(resources) = processing.resources() {
            // A valid Path always has nonempty compatibility arrays. Zero
            // unpaid capacity identifies paid storage without exposing its owner.
            if path.retained_unpaid_capacity_bytes() == Some(0) && !path.belongs_to(resources) {
                return Err(
                    processing.record_resource(markdown_svg_budget::Error::new(
                        markdown_svg_budget::FailureKind::Invariant,
                        markdown_svg_budget::LimitKind::None,
                    )),
                );
            }
        }
        let mut offsets = GeometryVec::with_capacity(processing, clusters.len())?;
        let mut normals = GeometryVec::with_capacity(processing, clusters.len())?;
        {
            let mut advance = offset;
            for (cluster, (shift_x, _)) in clusters.iter().zip(shifts) {
                geometry_checkpoint(processing, 1)?;
                // Clusters should be rotated by the x-midpoint x baseline position.
                let half_width = cluster.width / 2.0;

                let offset = advance + shift_x + half_width;

                // Clusters outside the path have no normals.
                if offset < 0.0 {
                    normals.push(None)?;
                }

                offsets.push(offset as f64)?;
                advance += cluster.advance;
            }
        }

        let mut prev_mx = path.points()[0].x;
        let mut prev_my = path.points()[0].y;
        let mut prev_x = prev_mx;
        let mut prev_y = prev_my;

        fn create_line(px: f32, py: f32, x: f32, y: f32) -> kurbo::PathSeg {
            kurbo::PathSeg::Line(kurbo::Line::new(
                kurbo::Point::new(px as f64, py as f64),
                kurbo::Point::new(x as f64, y as f64),
            ))
        }

        let mut length: f64 = 0.0;
        let mut segments = path.segments();
        loop {
            geometry_checkpoint(processing, 1)?;
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
                tiny_skia_path::PathSegment::LineTo(p) => create_line(prev_x, prev_y, p.x, p.y),
                tiny_skia_path::PathSegment::QuadTo(p1, p) => kurbo::PathSeg::Cubic(
                    kurbo::QuadBez {
                        p0: kurbo::Point::new(prev_x as f64, prev_y as f64),
                        p1: kurbo::Point::new(p1.x as f64, p1.y as f64),
                        p2: kurbo::Point::new(p.x as f64, p.y as f64),
                    }
                    .raise(),
                ),
                tiny_skia_path::PathSegment::CubicTo(p1, p2, p) => {
                    kurbo::PathSeg::Cubic(kurbo::CubicBez {
                        p0: kurbo::Point::new(prev_x as f64, prev_y as f64),
                        p1: kurbo::Point::new(p1.x as f64, p1.y as f64),
                        p2: kurbo::Point::new(p2.x as f64, p2.y as f64),
                        p3: kurbo::Point::new(p.x as f64, p.y as f64),
                    })
                }
                tiny_skia_path::PathSegment::Close => create_line(prev_x, prev_y, prev_mx, prev_my),
            };

            let arclen_accuracy = text_path_accuracy(text.abs_transform, processing)?;
            let mut control = GeometryControl(processing);
            let curve_len = curve
                .try_arclen_with_control(arclen_accuracy, &mut control)
                .map_err(|error| crate::geometry::math_error(processing, error))?;
            if matches!(curve, kurbo::PathSeg::Line(_)) && curve_len == 0.0 {
                // A zero-length straight segment has no direction. Advance its
                // endpoint without invoking Line::inv_arclen (distance / length).
                // Degenerate Quad/Cubic behavior remains on the original branch.
                let end = curve.end();
                prev_x = end.x as f32;
                prev_y = end.y as f32;
                continue;
            }

            for offset in &offsets[normals.len()..] {
                geometry_checkpoint(processing, 1)?;
                if *offset >= length && *offset <= length + curve_len {
                    let mut offset = curve
                        .try_inv_arclen_with_control(offset - length, arclen_accuracy, &mut control)
                        .map_err(|error| crate::geometry::math_error(processing, error))?;
                    // some rounding error may occur, so we give offset a little tolerance
                    debug_assert!((-1.0e-3..=1.0 + 1.0e-3).contains(&offset));
                    offset = offset.clamp(0.0, 1.0);

                    let pos = curve.eval(offset);
                    let d = match curve {
                        kurbo::PathSeg::Line(line) => line.p1 - line.p0,
                        kurbo::PathSeg::Quad(quad) => quad.deriv().eval(offset).to_vec2(),
                        kurbo::PathSeg::Cubic(cubic) => cubic.deriv().eval(offset).to_vec2(),
                    };
                    let d = kurbo::Vec2::new(-d.y, d.x); // tangent
                    let angle = d.atan2().to_degrees() - 90.0;

                    normals.push(Some(PathNormal {
                        x: pos.x as f32,
                        y: pos.y as f32,
                        angle: angle as f32,
                    }))?;

                    if normals.len() == offsets.len() {
                        break;
                    }
                }
            }

            length += curve_len;
            let end = curve.end();
            prev_x = end.x as f32;
            prev_y = end.y as f32;
        }

        // If path ended and we still have unresolved normals - set them to `None`.
        for _ in 0..(offsets.len() - normals.len()) {
            geometry_checkpoint(processing, 1)?;
            normals.push(None)?;
        }

        drop(offsets);
        geometry_checkpoint(processing, 0)?;
        Ok(normals)
    })();
    geometry_checkpoint(processing, 0)?;
    candidate
}

/// Converts a text chunk into a list of outlined clusters.
///
/// This function will do the BIDI reordering, text shaping and glyphs outlining,
/// but not the text layouting. So all clusters are in the 0x0 position.
fn process_chunk(
    chunk: &TextChunk,
    fonts_cache: &FontsCache,
    resolver: &FontResolver,
    fontdb: &mut Arc<fontdb::Database>,
    processing: &crate::ProcessingContext,
) -> Result<Vec<GlyphCluster>, markdown_svg_budget::Error> {
    // The way this function works is a bit tricky.
    //
    // The first problem is BIDI reordering.
    // We cannot shape text span-by-span, because glyph clusters are not guarantee to be continuous.
    //
    // For example:
    // <text>Hel<tspan fill="url(#lg1)">lo של</tspan>ום.</text>
    //
    // Would be shaped as:
    // H e l l o   ש ל  ו  ם .   (characters)
    // 0 1 2 3 4 5 12 10 8 6 14  (cluster indices in UTF-8)
    //       ---         ---     (green span)
    //
    // As you can see, our continuous `lo של` span was split into two separated one.
    // So our 3 spans: black - green - black, become 5 spans: black - green - black - green - black.
    // If we shape `Hel`, then `lo של` an then `ום` separately - we would get an incorrect output.
    // To properly handle this we simply shape the whole chunk.
    //
    // But this introduces another issue - what to do when we have multiple fonts?
    // The easy solution would be to simply shape text with each font,
    // where the first font output is used as a base one and all others overwrite it.
    // This way in case of:
    // <text font-family="Arial">Hello <tspan font-family="Helvetica">world</tspan></text>
    // we would replace Arial glyphs for `world` with Helvetica one. Pretty simple.
    //
    // Well, it would work most of the time, but not always.
    // This is because different fonts can produce different amount of glyphs for the same text.
    // The most common example are ligatures. Some fonts can shape `fi` as two glyphs `f` and `i`,
    // but some can use `ﬁ` (U+FB01) instead.
    // Meaning that during merging we have to overwrite not individual glyphs, but clusters.

    // Glyph splitting assigns distinct glyphs to the same index in the original text, we need to
    // store previously used indices to make sure we do not re-use the same index while overwriting
    // span glyphs.
    let mut positions = HashSet::new();

    let mut glyphs = Vec::new();
    for span in &chunk.spans {
        let font = match fonts_cache.get(&(span as *const TextSpan)) {
            Some(v) => v.clone(),
            None => continue,
        };

        let tmp_glyphs = shape_text(
            &chunk.text,
            font,
            span.small_caps,
            span.apply_kerning,
            &span.font.variations,
            span.font_size.get(),
            span.font_optical_sizing,
            resolver,
            fontdb,
            processing,
        )?;

        // Do nothing with the first run.
        if glyphs.is_empty() {
            glyphs = tmp_glyphs;
            continue;
        }

        positions.clear();

        // Overwrite span's glyphs.
        let mut iter = tmp_glyphs.into_iter();
        while let Some(new_glyph) = iter.next() {
            if !span_contains(span, new_glyph.byte_idx) {
                continue;
            }

            let Some(idx) = glyphs
                .iter()
                .position(|g| g.byte_idx == new_glyph.byte_idx)
                .filter(|pos| !positions.contains(pos))
            else {
                continue;
            };

            positions.insert(idx);

            let prev_cluster_len = glyphs[idx].cluster_len;
            if prev_cluster_len < new_glyph.cluster_len {
                // If the new font represents the same cluster with fewer glyphs
                // then remove remaining glyphs.
                for _ in 1..new_glyph.cluster_len {
                    glyphs.remove(idx + 1);
                }
            } else if prev_cluster_len > new_glyph.cluster_len {
                // If the new font represents the same cluster with more glyphs
                // then insert them after the current one.
                for j in 1..prev_cluster_len {
                    if let Some(g) = iter.next() {
                        glyphs.insert(idx + j, g);
                    }
                }
            }

            glyphs[idx] = new_glyph;
        }
    }

    // Convert glyphs to clusters.
    let mut clusters = Vec::new();
    for (range, byte_idx) in GlyphClusters::new(&glyphs) {
        if let Some(span) = chunk_span_at(chunk, byte_idx) {
            clusters.push(form_glyph_clusters(
                &glyphs[range],
                &chunk.text,
                span.font_size.get(),
                processing,
            )?);
        }
    }

    Ok(clusters)
}

fn apply_length_adjust(chunk: &TextChunk, clusters: &mut [GlyphCluster]) {
    let is_horizontal = matches!(chunk.text_flow, TextFlow::Linear);

    for span in &chunk.spans {
        let target_width = match span.text_length {
            Some(v) => v,
            None => continue,
        };

        let mut width = 0.0;
        let mut cluster_indexes = Vec::new();
        for i in span.start..span.end {
            if let Some(index) = clusters.iter().position(|c| c.byte_idx.value() == i) {
                cluster_indexes.push(index);
            }
        }
        // Complex scripts can have multi-codepoint clusters therefore we have to remove duplicates.
        cluster_indexes.sort();
        cluster_indexes.dedup();

        for i in &cluster_indexes {
            // Use the original cluster `width` and not `advance`.
            // This method essentially discards any `word-spacing` and `letter-spacing`.
            width += clusters[*i].width;
        }

        if cluster_indexes.is_empty() {
            continue;
        }

        if span.length_adjust == LengthAdjust::Spacing {
            let factor = if cluster_indexes.len() > 1 {
                (target_width - width) / (cluster_indexes.len() - 1) as f32
            } else {
                0.0
            };

            for i in cluster_indexes {
                clusters[i].advance = clusters[i].width + factor;
                clusters[i].text_length_spacing = factor;
            }
        } else {
            let factor = target_width / width;
            // Prevent multiplying by zero.
            if factor < 0.001 {
                continue;
            }

            for i in cluster_indexes {
                clusters[i].transform = clusters[i].transform.pre_scale(factor, 1.0);

                // Technically just a hack to support the current text-on-path algorithm.
                if !is_horizontal {
                    clusters[i].advance *= factor;
                    clusters[i].width *= factor;
                }
            }
        }
    }
}

#[derive(Clone)]
struct LengthScope {
    directive: crate::ReplayLengthDirective,
    start: usize,
    end: usize,
}
struct LengthToken {
    indices: Vec<usize>,
    fixed: bool,
    start: f32,
    width: f32,
}
fn apply_checked_length_adjust(
    text: &Text,
    chunk: &TextChunk,
    char_offset: usize,
    clusters: &mut [GlyphCluster],
    proof: &dyn crate::ReplayLogicalSource,
    owner: &dyn crate::FontReplay,
) -> Result<(), &'static str> {
    let depth = chunk
        .spans
        .iter()
        .map(|span| span.length_directives.len())
        .max()
        .unwrap_or(0);
    if depth == 0 {
        return Ok(());
    }
    let bytes = clusters
        .len()
        .checked_mul((depth + 1).checked_mul(96).ok_or("LengthCapacity")?)
        .ok_or("LengthCapacity")?;
    let _retained = owner
        .reserve_replay_bytes(bytes)
        .map_err(|_| "LengthAdmission")?;
    let mut scopes: Vec<LengthScope> = Vec::new();
    let mut lookup = HashMap::new();
    for span in &chunk.spans {
        for directive in &span.length_directives {
            if let Some(&index) = lookup.get(&directive.owner) {
                let scope: &mut LengthScope = &mut scopes[index];
                scope.start = scope.start.min(span.start);
                scope.end = scope.end.max(span.end);
            } else {
                lookup.insert(directive.owner, scopes.len());
                scopes.push(LengthScope {
                    directive: *directive,
                    start: span.start,
                    end: span.end,
                });
            }
        }
    }
    scopes.sort_by_key(|scope| {
        (
            std::cmp::Reverse(scope.directive.depth),
            scope.start,
            scope.directive.owner,
        )
    });
    for scope in &scopes {
        proof.checkpoint(1).map_err(|_| "LengthWork")?;
        let mut indices = Vec::new();
        for (i, cluster) in clusters.iter().enumerate() {
            proof.checkpoint(1).map_err(|_| "LengthWork")?;
            let start = cluster.byte_idx.value();
            let end = cluster.source_end;
            if start < scope.end && scope.start < end {
                if start < scope.start || end > scope.end {
                    return Err("UnsupportedPartialUnitTextLength");
                }
                if cluster.spacing_eligible {
                    indices.push(i);
                }
            }
        }
        if indices.is_empty() {
            if scope.directive.target != 0.0 {
                return Err("UnsupportedLengthNoAdjustableGap");
            } else {
                continue;
            }
        }
        let (shifts, _) = resolve_relative_shifts(text, chunk, char_offset, clusters);
        let mut at = Vec::with_capacity(clusters.len());
        let mut pen = 0.0;
        for (cluster, shift) in clusters.iter().zip(shifts) {
            let inline = if text.writing_mode == WritingMode::TopToBottom {
                shift.1
            } else {
                shift.0
            };
            at.push(pen + inline);
            pen += cluster.advance;
        }
        let mut tokens: Vec<LengthToken> = Vec::new();
        for &i in &indices {
            let source = clusters[i].byte_idx.value();
            let child = scopes.iter().find(|child| {
                child.directive.parent == Some(scope.directive.owner)
                    && child.start <= source
                    && source < child.end
            });
            if let Some(child) = child {
                if let Some(last) = tokens.last_mut().filter(|token| {
                    token.fixed
                        && clusters[token.indices[0]].byte_idx.value() >= child.start
                        && clusters[token.indices[0]].byte_idx.value() < child.end
                }) {
                    last.indices.push(i);
                    continue;
                }
            }
            tokens.push(LengthToken {
                indices: vec![i],
                fixed: child.is_some(),
                start: 0.0,
                width: 0.0,
            });
        }
        for token in &mut tokens {
            let first = token.indices[0];
            let last = *token.indices.last().unwrap();
            token.start = at[first];
            token.width =
                at[last] + clusters[last].advance - clusters[last].spacing_after - token.start;
            if !token.width.is_finite() || token.width < 0.0 {
                return Err("UnsupportedLengthConstraint");
            }
        }
        let first = tokens[0].start;
        let last = tokens.last().unwrap();
        let width = last.start + last.width - first;
        let target = scope.directive.target;
        if !width.is_finite() || !target.is_finite() {
            return Err("UnsupportedLengthConstraint");
        }
        let delta = target - width;
        if delta.abs() <= f32::EPSILON * width.abs().max(1.0) {
            continue;
        }
        let mut gaps = Vec::new();
        for pair in tokens.windows(2) {
            gaps.push(pair[1].start - pair[0].start - pair[0].width);
        }
        if scope.directive.adjust == LengthAdjust::Spacing {
            if gaps.is_empty() {
                return Err("UnsupportedLengthNoAdjustableGap");
            }
            let extra = delta / gaps.len() as f32;
            for (index, token) in tokens[..tokens.len() - 1].iter().enumerate() {
                let previous = tokens[index].start + extra * index as f32;
                let next = tokens[index + 1].start + extra * (index + 1) as f32;
                if next < previous {
                    return Err("UnsupportedLengthConstraint");
                }
                clusters[*token.indices.last().unwrap()].advance += extra;
            }
        } else {
            let fixed: f32 = tokens
                .iter()
                .filter(|token| token.fixed)
                .map(|token| token.width)
                .sum();
            let free: f32 = tokens
                .iter()
                .filter(|token| !token.fixed)
                .map(|token| token.width)
                .sum();
            let denominator = free + gaps.iter().sum::<f32>();
            let factor = (target - fixed) / denominator;
            if denominator <= 0.0 || !factor.is_finite() || factor <= 0.0 {
                return Err("UnsupportedNestedLengthConstraint");
            }
            for (index, token) in tokens.iter().enumerate() {
                if !token.fixed {
                    if token.indices.len() != 1 {
                        return Err("UnsupportedNestedLengthConstraint");
                    }
                    let cluster = &mut clusters[token.indices[0]];
                    cluster.inline_scale *= factor;
                    cluster.width *= factor;
                    cluster.advance += (factor - 1.0) * token.width;
                }
                if let Some(gap) = gaps.get(index) {
                    clusters[*token.indices.last().unwrap()].advance += (factor - 1.0) * gap;
                }
            }
        }
    }
    Ok(())
}

/// Rotates clusters according to
/// [Unicode Vertical_Orientation Property](https://www.unicode.org/reports/tr50/tr50-19.html).
fn apply_writing_mode(writing_mode: WritingMode, clusters: &mut [GlyphCluster]) {
    if writing_mode != WritingMode::TopToBottom {
        return;
    }

    for cluster in clusters {
        if cluster.logical_parent.is_some() {
            for glyph in &mut cluster.glyphs {
                let context = glyph.resolved_context.unwrap();
                let font = glyph.layout_font.as_ref().unwrap();
                let ascent = font.ascent(glyph.font_size);
                let descent = font.descent(glyph.font_size);
                let midpoint = (ascent + descent) / 2.0;
                let ts = if context.native_orientation == 1 {
                    if glyph.geometry_origin
                        == crate::ReplayGeometryOrigin::NativeSidewaysRunPenNormalized
                    {
                        glyph.placement_policy =
                            crate::ReplayPlacementPolicy::NativeSidewaysInverse90;
                        glyph.engine_metrics_midpoint = None;
                        Transform::from_rotate(-90.0)
                    } else {
                        glyph.placement_policy =
                            crate::ReplayPlacementPolicy::EngineMetricsMidpointUpright;
                        glyph.engine_metrics_midpoint = Some(midpoint);
                        Transform::default()
                            .pre_translate(0.0, midpoint)
                            .pre_rotate_at(-90.0, cluster.width / 2.0, -midpoint)
                    }
                } else {
                    glyph.placement_policy =
                        crate::ReplayPlacementPolicy::EngineMetricsMidpointRotated;
                    glyph.engine_metrics_midpoint = Some(midpoint);
                    Transform::from_translate(0.0, midpoint)
                };
                glyph.writing_transform = Some(ts);
            }
            if cluster
                .glyphs
                .first()
                .is_some_and(|g| g.resolved_context.unwrap().native_orientation == 1)
            {
                cluster.ascent = cluster.width / 2.0;
                cluster.descent = -cluster.width / 2.0;
            }
            continue;
        }
        let orientation = unicode_vo::char_orientation(cluster.codepoint);
        let midpoint = (cluster.ascent + cluster.descent) / 2.0;
        if orientation == unicode_vo::Orientation::Upright {
            let mut ts = Transform::default();
            // Position glyph in the center of vertical axis.
            ts = ts.pre_translate(0.0, (cluster.ascent + cluster.descent) / 2.0);
            // Rotate by 90 degrees in the center.
            ts = ts.pre_rotate_at(
                -90.0,
                cluster.width / 2.0,
                -(cluster.ascent + cluster.descent) / 2.0,
            );

            cluster.path_transform = ts;
            for glyph in &mut cluster.glyphs {
                if glyph.geometry_origin
                    == crate::ReplayGeometryOrigin::NativeSidewaysRunPenNormalized
                {
                    glyph.placement_policy = crate::ReplayPlacementPolicy::NativeSidewaysInverse90;
                    glyph.engine_metrics_midpoint = None;
                } else {
                    glyph.placement_policy =
                        crate::ReplayPlacementPolicy::EngineMetricsMidpointUpright;
                    glyph.engine_metrics_midpoint = Some(midpoint);
                }
            }

            // Move "baseline" to the middle and make height equal to width.
            cluster.ascent = cluster.width / 2.0;
            cluster.descent = -cluster.width / 2.0;
        } else {
            // Could not find a spec that explains this,
            // but this is how other applications are shifting the "rotated" characters
            // in the top-to-bottom mode.
            cluster.transform = cluster
                .transform
                .pre_translate(0.0, (cluster.ascent + cluster.descent) / 2.0);
            for glyph in &mut cluster.glyphs {
                glyph.placement_policy = crate::ReplayPlacementPolicy::EngineMetricsMidpointRotated;
                glyph.engine_metrics_midpoint = Some(midpoint);
            }
        }
    }
}

/// Applies the `letter-spacing` property to a text chunk clusters.
///
/// [In the CSS spec](https://www.w3.org/TR/css-text-3/#letter-spacing-property).
fn apply_letter_spacing(chunk: &TextChunk, clusters: &mut [GlyphCluster]) {
    let eligible: Vec<_> = clusters
        .iter()
        .enumerate()
        .filter_map(|(i, c)| c.spacing_eligible.then_some(i))
        .collect();
    for pair in eligible.windows(2) {
        let left = pair[0];
        let right = pair[1];
        let spacing = |cluster: &GlyphCluster| {
            let script = cluster
                .resolved_script
                .unwrap_or_else(|| cluster.codepoint.script());
            if !script_supports_letter_spacing(script) {
                return 0.0;
            }
            chunk_span_at(chunk, cluster.byte_idx).map_or(0.0, |span| span.letter_spacing)
        };
        let gap = (spacing(&clusters[left]) + spacing(&clusters[right])) / 2.0;
        clusters[left].advance += gap;
        clusters[left].spacing_after += gap;
    }
}

/// Applies the `word-spacing` property to a text chunk clusters.
///
/// [In the CSS spec](https://www.w3.org/TR/css-text-3/#propdef-word-spacing).
fn apply_word_spacing(chunk: &TextChunk, clusters: &mut [GlyphCluster]) {
    // At least one span should have a non-zero spacing.
    if !chunk
        .spans
        .iter()
        .any(|span| !span.word_spacing.approx_zero_ulps(4))
    {
        return;
    }

    for cluster in clusters {
        if is_word_separator_characters(cluster.codepoint) {
            if let Some(span) = chunk_span_at(chunk, cluster.byte_idx) {
                // Technically, word spacing 'should be applied half on each
                // side of the character', but it doesn't affect us in any way,
                // so we are ignoring this.
                cluster.advance += span.word_spacing;

                // After word spacing, `advance` can be negative.
            }
        }
    }
}

fn form_glyph_clusters(
    glyphs: &[Glyph],
    text: &str,
    font_size: f32,
    processing: &crate::ProcessingContext,
) -> GeometryResult<GlyphCluster> {
    debug_assert!(!glyphs.is_empty());

    let mut x = 0.0;
    let mut width = 0.0;
    let mut advance = 0.0;

    let mut positioned_glyphs = TreeVec::new(processing);
    let first_size =
        glyphs[0].identity.map_or(font_size, |id| id.semantic_em) * glyphs[0].fallback_scale;
    let homogeneous = glyphs.iter().all(|g| {
        g.font.units_per_em == glyphs[0].font.units_per_em
            && (g.identity.map_or(font_size, |id| id.semantic_em) * g.fallback_scale).to_bits()
                == first_size.to_bits()
    });
    let mut pen_dip = 0.0;

    for glyph in glyphs {
        owner::check(processing, 1)?;
        let semantic_em = glyph.identity.map_or(font_size, |id| id.semantic_em);
        let actual_size = semantic_em * glyph.fallback_scale;
        let sx = glyph.font.scale(actual_size);

        // Apply offset.
        //
        // The first glyph in the cluster will have an offset from 0x0,
        // but the later one will have an offset from the "current position".
        // So we have to keep an advance.
        // TODO: should be done only inside a single text span
        let ts = Transform::from_translate(
            if homogeneous { x } else { pen_dip / sx } + glyph.dx as f32,
            -glyph.dy as f32,
        );

        positioned_glyphs.try_push(
            PositionedGlyph {
                local_representation: None,
                glyph_ts: ts,
                source_span: glyph.identity.map(|id| id.visual_span),
                source_range: glyph.byte_idx.value()..glyph.byte_idx.value() + glyph.cluster_len,
                raw_x_advance: glyph.width,
                raw_y_advance: glyph.raw_y_advance,
                resolved_context: glyph.identity,
                layout_font: glyph.identity.map(|_| glyph.font.clone()),
                writing_transform: None,
                progression_start: if homogeneous { x * sx } else { pen_dip },
                progression_advance: glyph.width as f32 * sx,
                // Will be set later.
                cluster_ts: Transform::default(),
                // Will be set later.
                span_ts: Transform::default(),
                units_per_em: glyph.font.units_per_em.get(),
                font_size: actual_size,
                geometry_origin: glyph.geometry_origin,
                placement_policy: crate::ReplayPlacementPolicy::UnchangedHorizontal,
                engine_metrics_midpoint: None,
                font: glyph.font.id,
                text: glyph.text.try_copy_in(processing)?,
                id: glyph.id,
            },
            processing,
        )?;

        x += glyph.width as f32;

        let glyph_width = glyph.width as f32 * sx;
        advance += glyph_width;
        pen_dip += glyph_width;
        if glyph_width > width {
            width = glyph_width;
        }
    }

    let byte_idx = glyphs[0].byte_idx;
    let font = glyphs[0].font.clone();
    Ok(GlyphCluster {
        byte_idx,
        source_end: glyphs
            .iter()
            .map(|g| g.byte_idx.value() + g.cluster_len)
            .max()
            .unwrap(),
        logical_parent: glyphs[0].identity.map(|id| id.parent),
        resolved_script: glyphs[0].identity.and_then(|id| chunk_script(id.script_be)),
        spacing_eligible: true,
        spacing_after: 0.0,
        inline_scale: 1.0,
        codepoint: byte_idx.char_from(text),
        width,
        advance,
        text_length_spacing: 0.0,
        ascent: font.ascent(font_size * glyphs[0].fallback_scale),
        descent: font.descent(font_size * glyphs[0].fallback_scale),
        has_relative_shift: false,
        transform: Transform::default(),
        path_transform: Transform::default(),
        glyphs: positioned_glyphs,
        visible: true,
    })
}

pub(crate) trait DatabaseExt {
    fn load_font(
        &self,
        id: ID,
        variations: &[crate::FontVariation],
        observer: Option<&dyn crate::FontReplay>,
        processing: &crate::ProcessingContext,
    ) -> Option<ResolvedFont>;
    fn try_has_char(
        &self,
        id: ID,
        c: char,
        resources: Option<&markdown_svg_budget::ResourcesRef>,
    ) -> GeometryResult<bool>;
}

impl DatabaseExt for Database {
    #[inline(never)]
    fn load_font(
        &self,
        id: ID,
        variations: &[crate::FontVariation],
        observer: Option<&dyn crate::FontReplay>,
        processing: &crate::ProcessingContext,
    ) -> Option<ResolvedFont> {
        geometry_checkpoint(processing, 0)
            .map_err(|e| processing.record_resource(e))
            .ok()?;
        let candidate = self
            .with_face_data(id, |data, face_index| -> Option<ResolvedFont> {
                let instance = super::font_instance::FontInstance::new(
                    data,
                    face_index,
                    variations,
                    processing.resources(),
                )
                .map_err(|e| processing.record_resource(e))
                .ok()??;
                #[cfg(test)]
                font_consumer_tail_tests::metrics_enter();
                let metrics = instance
                    .metrics()
                    .map_err(|e| processing.record_resource(e))
                    .ok()?;
                if let Some(observer) = observer {
                    observer
                        .observe_metrics(crate::ReplayMetricsObservation {
                            font: id,
                            face_index,
                            variations,
                            location: instance.location(),
                            axes: instance.axes(),
                            metrics,
                        })
                        .ok()?;
                }

                // Reject fonts with an out-of-range unitsPerEm, like `ttf-parser` did.
                if !(16..=16384).contains(&metrics.units_per_em) {
                    return None;
                }
                let units_per_em = NonZeroU16::new(metrics.units_per_em)?;

                let ascent = metrics.ascent;
                let descent = metrics.descent;

                let x_height = metrics
                    .x_height
                    .filter(|x| *x > 0.0)
                    .and_then(|x| u16::try_from(x.round() as i32).ok())
                    .and_then(NonZeroU16::new);
                let x_height = match x_height {
                    Some(height) => height,
                    None => {
                        // If not set - fallback to height * 45%.
                        // 45% is what Firefox uses.
                        u16::try_from(((ascent - descent) * 0.45).round() as i32)
                            .ok()
                            .and_then(NonZeroU16::new)?
                    }
                };

                let line_through = metrics.strikeout;
                let line_through_position = match line_through {
                    Some(metrics) => metrics.offset.round() as i16,
                    None => x_height.get() as i16 / 2,
                };

                let (underline_position, underline_thickness) = match metrics.underline {
                    Some(metrics) => {
                        let thickness = u16::try_from(metrics.thickness.round() as i32)
                            .ok()
                            .and_then(NonZeroU16::new)
                            // `skrifa` guarantees that units_per_em is > 0
                            .unwrap_or_else(|| NonZeroU16::new(units_per_em.get() / 12).unwrap());

                        (metrics.offset.round() as i16, thickness)
                    }
                    None => (
                        -(units_per_em.get() as i16) / 9,
                        NonZeroU16::new(units_per_em.get() / 12).unwrap(),
                    ),
                };

                // 0.2 and 0.4 are generic offsets used by some applications (Inkscape/librsvg).
                let mut subscript_offset = (units_per_em.get() as f32 / 0.2).round() as i16;
                let mut superscript_offset = (units_per_em.get() as f32 / 0.4).round() as i16;

                (subscript_offset, superscript_offset) = instance
                    .script_offsets(subscript_offset, superscript_offset)
                    .map_err(|e| processing.record_resource(e))
                    .ok()?;

                Some(ResolvedFont {
                    id,
                    units_per_em,
                    ascent: ascent.round() as i16,
                    descent: descent.round() as i16,
                    x_height,
                    underline_position,
                    underline_thickness,
                    line_through_position,
                    subscript_offset,
                    superscript_offset,
                })
            })
            .flatten();
        // Axes/location temporaries are gone before returning a scalar font or
        // allowing caller fallback, including unsuccessful font-domain results.
        geometry_checkpoint(processing, 0)
            .map_err(|e| processing.record_resource(e))
            .ok()?;
        candidate
    }

    #[inline(never)]
    fn try_has_char(
        &self,
        id: ID,
        c: char,
        resources: Option<&markdown_svg_budget::ResourcesRef>,
    ) -> GeometryResult<bool> {
        if let Some(resources) = resources {
            resources.checkpoint(0)?;
        }
        let result = self.with_face_data(id, |data, index| -> GeometryResult<bool> {
            Ok(match resources {
                Some(resources) => {
                    let Ok(font) = skrifa::FontRef::try_from_index(
                        data,
                        index,
                        &mut markdown_svg_budget::RequestControl(resources),
                    )?
                    else {
                        return Ok(false);
                    };
                    skrifa::request::Charmap::try_new(&font, resources)?
                        .try_map(c as u32)?
                        .is_some()
                }
                None => {
                    let Ok(font) = skrifa::FontRef::from_index(data, index) else {
                        return Ok(false);
                    };
                    skrifa::charmap::Charmap::new(&font).map(c).is_some()
                }
            })
        });
        // The database borrow and any lazy mapping owners are already gone.
        if let Some(resources) = resources {
            resources.checkpoint(0)?;
        }
        result.unwrap_or(Ok(false))
    }
}

fn load_replay_font(
    face: crate::ReplayFace,
    owner: &dyn crate::FontReplay,
    db: &fontdb::Database,
    processing: &crate::ProcessingContext,
) -> Option<ResolvedFont> {
    let variations = owner.variations(face.id).ok()?;
    let mut font = db.load_font(face.id, &variations, Some(owner), processing)?;
    if let Some(metrics) = face.synthetic_metrics {
        if metrics.units_per_em != u32::from(font.units_per_em.get()) {
            return None;
        }
        font.ascent = i16::try_from(metrics.ascent).ok()?;
        font.descent = i16::try_from(metrics.descent).ok()?;
        // DWRITE_FONT_METRICS defines xHeight0 as unspecified for older fonts.
        // Preserve the existing explicit usvg 45%-height fallback in that case.
        if metrics.x_height > 0 {
            font.x_height = u16::try_from(metrics.x_height)
                .ok()
                .and_then(NonZeroU16::new)?;
        }
        font.underline_position = i16::try_from(metrics.underline_position).ok()?;
        font.underline_thickness = u16::try_from(metrics.underline_thickness)
            .ok()
            .and_then(NonZeroU16::new)?;
        font.line_through_position = i16::try_from(metrics.strike_position).ok()?;
    }
    owner
        .observe_layout_metrics(crate::ReplayLayoutMetricsObservation {
            font: face.id,
            units_per_em: font.units_per_em.get(),
            synthetic: face.synthetic_metrics.is_some(),
            values: [
                i32::from(font.ascent),
                i32::from(font.descent),
                i32::from(font.x_height.get()),
                i32::from(font.underline_position),
                i32::from(font.underline_thickness.get()),
                i32::from(font.line_through_position),
            ],
        })
        .ok()?;
    Some(font)
}
fn chunk_script(tag: u32) -> Option<unicode_script::Script> {
    unicode_script::Script::from_short_name(core::str::from_utf8(&tag.to_be_bytes()).ok()?)
}

fn process_replay_chunk(
    chunk: &TextChunk,
    prepared: &[crate::ReplayGlyph],
    primary_faces: &[crate::ReplayFace],
    logical: Option<&dyn crate::ReplayLogicalSource>,
    byte_base: usize,
    owner: &dyn crate::FontReplay,
    db: &fontdb::Database,
    processing: &crate::ProcessingContext,
) -> Option<Vec<GlyphCluster>> {
    let mut fonts: HashMap<ID, ResolvedFont> = HashMap::new();
    let mut glyphs = Vec::with_capacity(prepared.len());
    for glyph in prepared {
        if glyph.y_advance != 0 {
            let _ = owner.unsupported("NonzeroYAdvanceLayout");
            return None;
        }
        if let Some(proof) = logical {
            proof.checkpoint(1).ok()?;
        }
        if glyph.start >= glyph.end
            || !chunk.text.is_char_boundary(glyph.start)
            || !chunk.text.is_char_boundary(glyph.end)
            || !glyph.fallback_scale.is_finite()
            || glyph.fallback_scale <= 0.0
        {
            return None;
        }
        let font = if let Some(font) = fonts.get(&glyph.face.id) {
            font.clone()
        } else {
            let font = load_replay_font(glyph.face, owner, db, processing)?;
            fonts.insert(glyph.face.id, font.clone());
            font
        };
        glyphs.push(Glyph {
            identity: glyph.identity,
            raw_y_advance: glyph.y_advance,
            byte_idx: ByteIndex::new(glyph.start),
            cluster_len: glyph.end - glyph.start,
            text: TreeString::try_from_str(&chunk.text[glyph.start..glyph.end], processing).ok()?,
            id: GlyphId(glyph.glyph),
            dx: glyph.x_offset,
            dy: glyph.y_offset,
            width: glyph.advance,
            fallback_scale: glyph.fallback_scale,
            geometry_origin: glyph.geometry_origin,
            font,
        });
    }
    let mut clusters = Vec::new();
    for (range, index) in GlyphClusters::new(&glyphs) {
        let span = chunk_span_at(chunk, index)?;
        let mut cluster = form_glyph_clusters(
            &glyphs[range],
            &chunk.text,
            span.font_size.get(),
            processing,
        )
        .ok()?;
        if let (Some(parent), Some(proof)) = (cluster.logical_parent, logical) {
            let cell = proof.cells().get(parent)?;
            cluster.byte_idx = ByteIndex::new(cell.start.max(byte_base) - byte_base);
            cluster.source_end = cell.end.min(byte_base + chunk.text.len()) - byte_base;
            cluster.codepoint = cluster.byte_idx.char_from(&chunk.text);
        }
        clusters.push(cluster);
    }
    if let Some(proof) = logical {
        let begin = proof.cells().partition_point(|cell| cell.end <= byte_base);
        let end = proof
            .cells()
            .partition_point(|cell| cell.start < byte_base + chunk.text.len());
        let mut by_parent = HashMap::with_capacity(clusters.len());
        for cluster in clusters {
            let parent = cluster.logical_parent?;
            if by_parent.insert(parent, cluster).is_some() {
                let _ = owner.unsupported("NoncontiguousVisualParent");
                return None;
            }
        }
        let mut logical_clusters = Vec::with_capacity(end - begin);
        let mut levels = Vec::with_capacity(end - begin);
        for (offset, cell) in proof.cells()[begin..end].iter().enumerate() {
            proof.checkpoint(1).ok()?;
            let parent = begin + offset;
            let head = cell.start.max(byte_base) - byte_base;
            let cluster = if let Some(cluster) = by_parent.remove(&parent) {
                cluster
            } else {
                let visual = chunk.spans.partition_point(|span| span.end <= head);
                let span = chunk.spans.get(visual)?;
                let font = load_replay_font(*primary_faces.get(visual)?, owner, db, processing)?;
                GlyphCluster {
                    byte_idx: ByteIndex::new(head),
                    source_end: cell.end.min(byte_base + chunk.text.len()) - byte_base,
                    logical_parent: Some(parent),
                    resolved_script: None,
                    spacing_eligible: true,
                    spacing_after: 0.0,
                    inline_scale: 1.0,
                    codepoint: chunk.text[head..].chars().next()?,
                    width: 0.0,
                    advance: 0.0,
                    text_length_spacing: 0.0,
                    ascent: font.ascent(span.font_size.get()),
                    descent: font.descent(span.font_size.get()),
                    has_relative_shift: false,
                    glyphs: TreeVec::new(processing),
                    transform: Transform::default(),
                    path_transform: Transform::default(),
                    visible: true,
                }
            };
            levels.push(unicode_bidi::Level::new(proof.source_bidi_level(byte_base + head)?).ok()?);
            logical_clusters.push(Some(cluster));
        }
        if !by_parent.is_empty() {
            return None;
        }
        let resources = owner.request_resources();
        let order = match resources.as_ref() {
            Some(resources) => {
                match unicode_bidi::controlled::try_reorder_visual(&levels, resources) {
                    Ok(order) => BidiOwnedSlice::Funded(order),
                    Err(error) => {
                        resources.record(error);
                        return None;
                    }
                }
            }
            None => BidiOwnedSlice::Ordinary(unicode_bidi::BidiInfo::reorder_visual(&levels)),
        };
        return match order.collect_visual(&mut logical_clusters, resources.as_ref()) {
            Ok(clusters) => Some(clusters),
            Err(error) => {
                if let Some(resources) = resources.as_ref() {
                    resources.record(error);
                }
                None
            }
        };
    }
    Some(clusters)
}

/// Text shaping with font fallback.
pub(crate) fn shape_text(
    text: &str,
    font: ResolvedFont,
    small_caps: bool,
    apply_kerning: bool,
    variations: &[crate::FontVariation],
    font_size: f32,
    font_optical_sizing: crate::FontOpticalSizing,
    resolver: &FontResolver,
    fontdb: &mut Arc<fontdb::Database>,
    processing: &crate::ProcessingContext,
) -> Result<Vec<Glyph>, markdown_svg_budget::Error> {
    if let Some(error) = processing
        .resources()
        .and_then(|resources| resources.failure())
    {
        return Err(error);
    }
    let mut glyphs = shape_text_with_font(
        text,
        font,
        small_caps,
        apply_kerning,
        variations,
        font_size,
        font_optical_sizing,
        fontdb,
        processing,
    )?
    .unwrap_or_default();

    // Remember all fonts used for shaping.
    let mut used_fonts = TreeVec::new(processing);
    used_fonts.try_push(font.id, processing)?;

    // Loop until all glyphs become resolved or until no more fonts are left.
    'outer: loop {
        let mut missing = None;
        for glyph in &glyphs {
            geometry_checkpoint(processing, 1)?;
            if glyph.is_missing() {
                missing = Some(glyph.byte_idx.char_from(text));
                break;
            }
        }

        if let Some(c) = missing {
            geometry_checkpoint(processing, 0)?;
            let selected =
                resolver
                    .select_fallback
                    .try_select(c, &used_fonts, fontdb, processing)?;
            let fallback_font =
                match selected.and_then(|id| fontdb.load_font(id, variations, None, processing)) {
                    Some(v) => v,
                    None => {
                        geometry_checkpoint(processing, 0)?;
                        break 'outer;
                    }
                };
            geometry_checkpoint(processing, 0)?;

            // Shape again, using a new font.
            let fallback_glyphs = shape_text_with_font(
                text,
                fallback_font,
                small_caps,
                apply_kerning,
                variations,
                font_size,
                font_optical_sizing,
                fontdb,
                processing,
            )?
            .unwrap_or_default();

            let all_matched = fallback_glyphs.iter().all(|g| !g.is_missing());
            if all_matched {
                // Replace all glyphs when all of them were matched.
                let old = std::mem::replace(&mut glyphs, fallback_glyphs);
                drop(old);
                geometry_checkpoint(processing, 0)?;
                break 'outer;
            }

            // We assume, that shaping with an any font will produce the same amount of glyphs.
            // This is incorrect, but good enough for now.
            if glyphs.len() != fallback_glyphs.len() {
                drop(fallback_glyphs);
                geometry_checkpoint(processing, 0)?;
                break 'outer;
            }

            // TODO: Replace clusters and not glyphs. This should be more accurate.

            // Copy new glyphs.
            for i in 0..glyphs.len() {
                geometry_checkpoint(processing, 1)?;
                if glyphs[i].is_missing() && !fallback_glyphs[i].is_missing() {
                    let copied = fallback_glyphs[i].try_copy_in(processing)?;
                    let old = std::mem::replace(&mut glyphs[i], copied);
                    drop(old);
                    geometry_checkpoint(processing, 0)?;
                }
            }

            // Remember this font.
            drop(fallback_glyphs);
            geometry_checkpoint(processing, 0)?;
            used_fonts.try_push(fallback_font.id, processing)?;
        } else {
            break 'outer;
        }
    }

    // Warn about missing glyphs.
    for glyph in &glyphs {
        geometry_checkpoint(processing, 1)?;
        if glyph.is_missing() {
            let c = glyph.byte_idx.char_from(text);
            // TODO: print a full grapheme
            log::warn!(
                "No fonts with a {}/U+{:X} character were found.",
                c,
                c as u32
            );
        }
    }

    drop(used_fonts);
    geometry_checkpoint(processing, 0)?;
    Ok(glyphs)
}

/// Either the original compatibility storage or one individually paid payload.
/// Only borrowed slices escape this local wrapper; there is no unpaid Vec clone.
enum BidiOwnedSlice<T> {
    Ordinary(Vec<T>),
    Funded(markdown_svg_budget::BudgetVec<T>),
}
impl<T> Deref for BidiOwnedSlice<T> {
    type Target = [T];
    fn deref(&self) -> &[T] {
        match self {
            Self::Ordinary(values) => values,
            Self::Funded(values) => values,
        }
    }
}

impl BidiOwnedSlice<usize> {
    fn collect_visual<T>(
        self,
        values: &mut [Option<T>],
        resources: Option<&markdown_svg_budget::ResourcesRef>,
    ) -> Result<Vec<T>, markdown_svg_budget::Error> {
        let result = self
            .iter()
            .map(|&index| values[index].take().unwrap())
            .collect();
        // Finish consuming the paid permutation before the replay caller can
        // continue. Its release callback may be the first failing operation.
        drop(self);
        if let Some(resources) = resources {
            resources.checkpoint(0)?;
        }
        Ok(result)
    }
}

fn bidi_visual_runs_for_shape(
    text: &str,
    resources: Option<&markdown_svg_budget::ResourcesRef>,
) -> Result<
    (
        BidiOwnedSlice<unicode_bidi::Level>,
        BidiOwnedSlice<unicode_bidi::LevelRun>,
    ),
    markdown_svg_budget::Error,
> {
    match resources {
        Some(resources) => {
            #[cfg(test)]
            bidi_resource_tests::gate(bidi_resource_tests::Stage::BeforeConstructor, resources);
            let info = unicode_bidi::controlled::BidiInfo::try_new(
                text,
                Some(unicode_bidi::Level::ltr()),
                resources,
            )?;
            let paragraph = info.paragraphs().first().ok_or_else(|| {
                resources.record(markdown_svg_budget::Error::new(
                    markdown_svg_budget::FailureKind::Invariant,
                    markdown_svg_budget::LimitKind::None,
                ))
            })?;
            #[cfg(test)]
            bidi_resource_tests::gate(bidi_resource_tests::Stage::BeforeVisualRuns, resources);
            let visual = info.try_visual_runs(0, paragraph.range.start..paragraph.range.end)?;
            let (levels, runs) = visual.into_parts();
            // Releasing the constructor owners may itself record the first
            // resource fault. Observe it before creating the HB shaper.
            drop(info);
            resources.checkpoint(0)?;
            #[cfg(test)]
            bidi_resource_tests::gate(bidi_resource_tests::Stage::AfterVisualRuns, resources);
            Ok((BidiOwnedSlice::Funded(levels), BidiOwnedSlice::Funded(runs)))
        }
        None => {
            let info = unicode_bidi::BidiInfo::new(text, Some(unicode_bidi::Level::ltr()));
            let paragraph = &info.paragraphs[0];
            let (levels, runs) = info.visual_runs(paragraph, paragraph.range.clone());
            Ok((
                BidiOwnedSlice::Ordinary(levels),
                BidiOwnedSlice::Ordinary(runs),
            ))
        }
    }
}

// These private holders dispatch to the same HB kernels. Only the request
// branch can create paid caches and output; it never constructs a plain shaper.
enum TextShapeData<'f> {
    Ordinary {
        data: harfrust::ShaperData,
        instance: Option<harfrust::ShaperInstance>,
    },
    Request {
        data: harfrust::request::Data<'f>,
        instance: Option<harfrust::request::Instance<'f>>,
    },
}
enum TextShaper<'a, 'f> {
    Ordinary(harfrust::Shaper<'a>),
    Request {
        data: &'a harfrust::request::Data<'f>,
        instance: Option<&'a harfrust::request::Instance<'f>>,
        resources: &'a markdown_svg_budget::ResourcesRef,
    },
}
enum TextShapeOutput {
    Ordinary(harfrust::GlyphBuffer),
    Request(harfrust::request::Output),
}
impl TextShapeOutput {
    fn infos(&self) -> &[harfrust::GlyphInfo] {
        match self {
            Self::Ordinary(v) => v.glyph_infos(),
            Self::Request(v) => v.glyph_infos(),
        }
    }
    fn positions(&self) -> &[harfrust::GlyphPosition] {
        match self {
            Self::Ordinary(v) => v.glyph_positions(),
            Self::Request(v) => v.glyph_positions(),
        }
    }
}
impl<'f> TextShapeData<'f> {
    fn new(
        font: &harfrust::FontRef<'f>,
        variations: &[harfrust::Variation],
        resources: Option<&markdown_svg_budget::ResourcesRef>,
    ) -> GeometryResult<Self> {
        Ok(match resources {
            Some(resources) => {
                let data = harfrust::request::Data::try_new(font, resources)?;
                let instance = if variations.is_empty() {
                    None
                } else {
                    Some(harfrust::request::Instance::try_from_variations(
                        &data, variations,
                    )?)
                };
                Self::Request { data, instance }
            }
            None => {
                let instance = (!variations.is_empty())
                    .then(|| harfrust::ShaperInstance::from_variations(font, variations));
                Self::Ordinary {
                    data: harfrust::ShaperData::new(font),
                    instance,
                }
            }
        })
    }
    fn shaper<'a>(
        &'a self,
        font: &'a harfrust::FontRef<'f>,
        resources: Option<&'a markdown_svg_budget::ResourcesRef>,
    ) -> TextShaper<'a, 'f> {
        match self {
            Self::Ordinary { data, instance } => {
                TextShaper::Ordinary(data.shaper(font).instance(instance.as_ref()).build())
            }
            Self::Request { data, instance } => TextShaper::Request {
                data,
                instance: instance.as_ref(),
                resources: resources.expect("request holder"),
            },
        }
    }
}
impl TextShaper<'_, '_> {
    fn shape(
        &self,
        text: &str,
        direction: harfrust::Direction,
        features: &[harfrust::Feature],
    ) -> GeometryResult<TextShapeOutput> {
        Ok(match self {
            Self::Ordinary(shaper) => {
                let mut buffer = harfrust::UnicodeBuffer::new();
                buffer.push_str(text);
                buffer.set_direction(direction);
                buffer.guess_segment_properties();
                TextShapeOutput::Ordinary(
                    shaper.shape(buffer, ShapeOptions::new().features(features)),
                )
            }
            Self::Request {
                data,
                instance,
                resources,
            } => {
                let mut buffer = harfrust::request::Buffer::try_new(resources)?;
                buffer.try_push_str(text)?;
                buffer.try_set_direction(direction)?;
                buffer.try_guess_segment_properties()?;
                TextShapeOutput::Request(harfrust::request::try_shape(
                    resources,
                    data,
                    *instance,
                    buffer,
                    ShapeOptions::new().features(features),
                    None,
                )?)
            }
        })
    }
}

/// Converts a text into a list of glyph IDs.
///
/// This function will do the BIDI reordering and text shaping.
fn shape_text_with_font(
    text: &str,
    font: ResolvedFont,
    small_caps: bool,
    apply_kerning: bool,
    variations: &[crate::FontVariation],
    font_size: f32,
    font_optical_sizing: crate::FontOpticalSizing,
    fontdb: &fontdb::Database,
    processing: &crate::ProcessingContext,
) -> Result<Option<Vec<Glyph>>, markdown_svg_budget::Error> {
    if let Some(error) = processing
        .resources()
        .and_then(|resources| resources.failure())
    {
        return Err(error);
    }
    let mut bidi_error = None;
    let glyphs = fontdb.with_face_data(font.id, |font_data, face_index| -> Option<Vec<Glyph>> {
        use harfrust::{Feature, Tag, Variation};

        use crate::text::OPSZ;

        let hr_font = match processing.resources() {
            Some(resources) => harfrust::FontRef::try_from_index(
                font_data,
                face_index,
                &mut markdown_svg_budget::RequestControl(resources),
            )
            .map_err(|error| processing.record_resource(error))
            .ok()?
            .ok()?,
            None => harfrust::FontRef::from_index(font_data, face_index).ok()?,
        };

        // Build the list of variations to apply
        let mut applied = GeometryVec::with_capacity(processing, variations.len())
            .map_err(|error| processing.record_resource(error))
            .ok()?;
        for variation in variations {
            geometry_checkpoint(processing, 1)
                .map_err(|error| processing.record_resource(error))
                .ok()?;
            applied
                .push(Variation {
                    tag: Tag::from_be_bytes(variation.tag),
                    value: variation.value,
                })
                .map_err(|error| processing.record_resource(error))
                .ok()?;
        }

        // Automatic optical sizing: if font-optical-sizing is auto and the font has
        // an 'opsz' axis that isn't explicitly set, auto-set it to match font size.
        // This matches browser behavior (CSS font-optical-sizing: auto).
        if font_optical_sizing == crate::FontOpticalSizing::Auto {
            let mut has_explicit_opsz = false;
            for value in applied.iter() {
                geometry_checkpoint(processing, 1)
                    .map_err(|error| processing.record_resource(error))
                    .ok()?;
                if value.tag == *b"opsz" {
                    has_explicit_opsz = true;
                    break;
                }
            }
            if !has_explicit_opsz {
                let has_opsz = match processing.resources() {
                    Some(resources) => skrifa::request::Axes::try_new(&hr_font, resources)
                        .and_then(|axes| axes.try_get_by_tag(OPSZ))
                        .map_err(|error| processing.record_resource(error))
                        .ok()?
                        .is_some(),
                    None => hr_font.axes().get_by_tag(OPSZ).is_some(),
                };
                if has_opsz {
                    applied
                        .push(Variation {
                            tag: OPSZ,
                            value: font_size,
                        })
                        .map_err(|error| processing.record_resource(error))
                        .ok()?;
                }
            }
        }

        let (levels, runs) = match bidi_visual_runs_for_shape(text, processing.resources()) {
            Ok(value) => value,
            Err(error) => {
                bidi_error = Some(error);
                return None;
            }
        };

        let mut glyphs = Vec::new();

        let shaping = TextShapeData::new(&hr_font, &applied, processing.resources())
            .map_err(|error| processing.record_resource(error))
            .ok()?;
        let shaper = shaping.shaper(&hr_font, processing.resources());

        let mut features = GeometryVec::new(processing);
        if small_caps {
            features
                .push(Feature::new(Tag::new(b"smcp"), 1, ..))
                .map_err(|error| processing.record_resource(error))
                .ok()?;
        }
        if !apply_kerning {
            features
                .push(Feature::new(Tag::new(b"kern"), 0, ..))
                .map_err(|error| processing.record_resource(error))
                .ok()?;
        }

        for run in runs.iter() {
            let sub_text = &text[run.clone()];
            if sub_text.is_empty() {
                continue;
            }

            let ltr = levels[run.start].is_ltr();
            let direction = if ltr {
                harfrust::Direction::LeftToRight
            } else {
                harfrust::Direction::RightToLeft
            };

            #[cfg(test)]
            font_consumer_tail_tests::run_enter(sub_text);
            let output = shaper
                .shape(sub_text, direction, &features)
                .map_err(|error| processing.record_resource(error))
                .ok()?;
            let positions = output.positions();
            let infos = output.infos();

            for i in 0..infos.len() {
                geometry_checkpoint(processing, 1)
                    .map_err(|error| processing.record_resource(error))
                    .ok()?;
                let pos = positions[i];
                let info = infos[i];
                let idx = run.start + info.cluster as usize;

                let start = info.cluster as usize;

                let end = if ltr {
                    i.checked_add(1)
                } else {
                    i.checked_sub(1)
                }
                .and_then(|last| infos.get(last))
                .map_or(sub_text.len(), |info| info.cluster as usize);

                glyphs.push(Glyph {
                    identity: None,
                    raw_y_advance: 0,
                    byte_idx: ByteIndex::new(idx),
                    cluster_len: end.checked_sub(start).unwrap_or(0), // TODO: can fail?
                    text: TreeString::try_from_str(&sub_text[start..end], processing).ok()?,
                    id: GlyphId(info.glyph_id),
                    dx: pos.x_offset,
                    dy: pos.y_offset,
                    width: pos.x_advance,
                    fallback_scale: 1.0,
                    geometry_origin: crate::ReplayGeometryOrigin::FontDesignOriginYUp,
                    font: font.clone(),
                });
            }
            // A refund callback may be the first failure. Observe it before
            // entering the next bidi run or any missing-glyph fallback.
            #[cfg(test)]
            font_consumer_tail_tests::before_output_drop();
            drop(output);
            geometry_checkpoint(processing, 0)
                .map_err(|error| processing.record_resource(error))
                .ok()?;
        }

        Some(glyphs)
    });
    // The paid visual levels/runs are dropped at closure exit. Their release
    // faults must also stop missing-glyph fallback before the next font.
    if let Some(error) = processing
        .resources()
        .and_then(|resources| resources.failure())
    {
        return Err(error);
    }
    match bidi_error {
        Some(error) => Err(error),
        None => Ok(glyphs.flatten()),
    }
}

/// An iterator over glyph clusters.
///
/// Input:  0 2 2 2 3 4 4 5 5
/// Result: 0 1     4 5   7
pub(crate) struct GlyphClusters<'a> {
    data: &'a [Glyph],
    idx: usize,
}

impl<'a> GlyphClusters<'a> {
    pub(crate) fn new(data: &'a [Glyph]) -> Self {
        GlyphClusters { data, idx: 0 }
    }
}

impl Iterator for GlyphClusters<'_> {
    type Item = (std::ops::Range<usize>, ByteIndex);

    fn next(&mut self) -> Option<Self::Item> {
        if self.idx == self.data.len() {
            return None;
        }

        let start = self.idx;
        let cluster = self.data[self.idx].byte_idx;
        let identity = self.data[self.idx].identity;
        for g in &self.data[self.idx..] {
            let same = match (identity, g.identity) {
                (Some(a), Some(b)) => {
                    a.parent == b.parent && a.position_domain == b.position_domain
                }
                (None, None) => g.byte_idx == cluster,
                _ => false,
            };
            if !same {
                break;
            }

            self.idx += 1;
        }

        Some((start..self.idx, cluster))
    }
}

/// Checks that selected script supports letter spacing.
///
/// [In the CSS spec](https://www.w3.org/TR/css-text-3/#cursive-tracking).
///
/// The list itself is from: https://github.com/harfbuzz/harfbuzz/issues/64
pub(crate) fn script_supports_letter_spacing(script: unicode_script::Script) -> bool {
    use unicode_script::Script;

    !matches!(
        script,
        Script::Arabic
            | Script::Syriac
            | Script::Nko
            | Script::Manichaean
            | Script::Psalter_Pahlavi
            | Script::Mandaic
            | Script::Mongolian
            | Script::Phags_Pa
            | Script::Devanagari
            | Script::Bengali
            | Script::Gurmukhi
            | Script::Modi
            | Script::Sharada
            | Script::Syloti_Nagri
            | Script::Tirhuta
            | Script::Ogham
    )
}

/// A glyph.
///
/// Basically, a glyph ID and it's metrics.
pub(crate) struct Glyph {
    pub(crate) identity: Option<crate::ReplayGlyphIdentity>,
    pub(crate) raw_y_advance: i32,
    /// The glyph ID in the font.
    pub(crate) id: GlyphId,

    /// Position in bytes in the original string.
    ///
    /// We use it to match a glyph with a character in the text chunk and therefore with the style.
    pub(crate) byte_idx: ByteIndex,

    // The length of the cluster in bytes.
    pub(crate) cluster_len: usize,

    /// The text from the original string that corresponds to that glyph.
    pub(crate) text: TreeString,

    /// The glyph offset in font units.
    pub(crate) dx: i32,

    /// The glyph offset in font units.
    pub(crate) dy: i32,

    /// The glyph width / X-advance in font units.
    pub(crate) width: i32,
    pub(crate) fallback_scale: f32,
    pub(crate) geometry_origin: crate::ReplayGeometryOrigin,

    /// Reference to the source font.
    ///
    /// Each glyph can have it's own source font.
    pub(crate) font: ResolvedFont,
}

impl Glyph {
    fn is_missing(&self) -> bool {
        self.id.0 == 0
    }
}

#[derive(Clone, Copy, Debug)]
pub(crate) struct ResolvedFont {
    pub(crate) id: ID,

    units_per_em: NonZeroU16,

    // All values below are in font units.
    ascent: i16,
    descent: i16,
    x_height: NonZeroU16,

    underline_position: i16,
    underline_thickness: NonZeroU16,

    // line-through thickness should be the the same as underline thickness
    // according to the TrueType spec:
    // https://docs.microsoft.com/en-us/typography/opentype/spec/os2#ystrikeoutsize
    line_through_position: i16,

    subscript_offset: i16,
    superscript_offset: i16,
}

pub(crate) fn chunk_span_at(chunk: &TextChunk, byte_offset: ByteIndex) -> Option<&TextSpan> {
    chunk
        .spans
        .iter()
        .find(|&span| span_contains(span, byte_offset))
}

pub(crate) fn span_contains(span: &TextSpan, byte_offset: ByteIndex) -> bool {
    byte_offset.value() >= span.start && byte_offset.value() < span.end
}

/// Checks that the selected character is a word separator.
///
/// According to: https://www.w3.org/TR/css-text-3/#word-separator
pub(crate) fn is_word_separator_characters(c: char) -> bool {
    matches!(
        c as u32,
        0x0020 | 0x00A0 | 0x1361 | 0x010100 | 0x010101 | 0x01039F | 0x01091F
    )
}

impl ResolvedFont {
    #[inline]
    pub(crate) fn scale(&self, font_size: f32) -> f32 {
        font_size / self.units_per_em.get() as f32
    }

    #[inline]
    pub(crate) fn ascent(&self, font_size: f32) -> f32 {
        self.ascent as f32 * self.scale(font_size)
    }

    #[inline]
    pub(crate) fn descent(&self, font_size: f32) -> f32 {
        self.descent as f32 * self.scale(font_size)
    }

    #[inline]
    pub(crate) fn height(&self, font_size: f32) -> f32 {
        self.ascent(font_size) - self.descent(font_size)
    }

    #[inline]
    pub(crate) fn x_height(&self, font_size: f32) -> f32 {
        self.x_height.get() as f32 * self.scale(font_size)
    }

    #[inline]
    pub(crate) fn underline_position(&self, font_size: f32) -> f32 {
        self.underline_position as f32 * self.scale(font_size)
    }

    #[inline]
    fn underline_thickness(&self, font_size: f32) -> f32 {
        self.underline_thickness.get() as f32 * self.scale(font_size)
    }

    #[inline]
    pub(crate) fn line_through_position(&self, font_size: f32) -> f32 {
        self.line_through_position as f32 * self.scale(font_size)
    }

    #[inline]
    fn subscript_offset(&self, font_size: f32) -> f32 {
        self.subscript_offset as f32 * self.scale(font_size)
    }

    #[inline]
    fn superscript_offset(&self, font_size: f32) -> f32 {
        self.superscript_offset as f32 * self.scale(font_size)
    }

    fn dominant_baseline_shift(&self, baseline: DominantBaseline, font_size: f32) -> f32 {
        let alignment = match baseline {
            DominantBaseline::Auto => AlignmentBaseline::Auto,
            DominantBaseline::UseScript => AlignmentBaseline::Auto, // unsupported
            DominantBaseline::NoChange => AlignmentBaseline::Auto,  // already resolved
            DominantBaseline::ResetSize => AlignmentBaseline::Auto, // unsupported
            DominantBaseline::Ideographic => AlignmentBaseline::Ideographic,
            DominantBaseline::Alphabetic => AlignmentBaseline::Alphabetic,
            DominantBaseline::Hanging => AlignmentBaseline::Hanging,
            DominantBaseline::Mathematical => AlignmentBaseline::Mathematical,
            DominantBaseline::Central => AlignmentBaseline::Central,
            DominantBaseline::Middle => AlignmentBaseline::Middle,
            DominantBaseline::TextAfterEdge => AlignmentBaseline::TextAfterEdge,
            DominantBaseline::TextBeforeEdge => AlignmentBaseline::TextBeforeEdge,
        };

        self.alignment_baseline_shift(alignment, font_size)
    }

    // The `alignment-baseline` property is a mess.
    //
    // The SVG 1.1 spec (https://www.w3.org/TR/SVG11/text.html#BaselineAlignmentProperties)
    // goes on and on about what this property suppose to do, but doesn't actually explain
    // how it should be implemented. It's just a very verbose overview.
    //
    // As of Nov 2022, only Chrome and Safari support `alignment-baseline`. Firefox isn't.
    // Same goes for basically every SVG library in existence.
    // Meaning we have no idea how exactly it should be implemented.
    //
    // And even Chrome and Safari cannot agree on how to handle `baseline`, `after-edge`,
    // `text-after-edge` and `ideographic` variants. Producing vastly different output.
    //
    // As per spec, a proper implementation should get baseline values from the font itself,
    // using `BASE` and `bsln` TrueType tables. If those tables are not present,
    // we have to synthesize them (https://drafts.csswg.org/css-inline/#baseline-synthesis-fonts).
    // And in the worst case scenario simply fallback to hardcoded values.
    //
    // Also, most fonts do not provide `BASE` and `bsln` tables to begin with.
    //
    // Again, as of Nov 2022, Chrome does only the latter:
    // https://github.com/chromium/chromium/blob/main/third_party/blink/renderer/platform/fonts/font_metrics.cc#L153
    //
    // Since baseline TrueType tables parsing and baseline synthesis are pretty hard,
    // we do what Chrome does - use hardcoded values. And it seems like Safari does the same.
    //
    //
    // But that's not all! SVG 2 and CSS Inline Layout 3 did a baseline handling overhaul,
    // and it's far more complex now. Not sure if anyone actually supports it.
    fn alignment_baseline_shift(&self, alignment: AlignmentBaseline, font_size: f32) -> f32 {
        match alignment {
            AlignmentBaseline::Auto => 0.0,
            AlignmentBaseline::Baseline => 0.0,
            AlignmentBaseline::BeforeEdge | AlignmentBaseline::TextBeforeEdge => {
                self.ascent(font_size)
            }
            AlignmentBaseline::Middle => self.x_height(font_size) * 0.5,
            AlignmentBaseline::Central => self.ascent(font_size) - self.height(font_size) * 0.5,
            AlignmentBaseline::AfterEdge | AlignmentBaseline::TextAfterEdge => {
                self.descent(font_size)
            }
            AlignmentBaseline::Ideographic => self.descent(font_size),
            AlignmentBaseline::Alphabetic => 0.0,
            AlignmentBaseline::Hanging => self.ascent(font_size) * 0.8,
            AlignmentBaseline::Mathematical => self.ascent(font_size) * 0.5,
        }
    }
}

pub(crate) type FontsCache = HashMap<*const TextSpan, ResolvedFont>;

/// A read-only text index in bytes.
///
/// Guarantee to be on a char boundary and in text bounds.
#[derive(Clone, Copy, PartialEq, Debug)]
pub(crate) struct ByteIndex(usize);

impl ByteIndex {
    fn new(i: usize) -> Self {
        ByteIndex(i)
    }

    pub(crate) fn value(&self) -> usize {
        self.0
    }

    /// Converts byte position into a code point position.
    pub(crate) fn code_point_at(&self, text: &str) -> usize {
        text.char_indices()
            .take_while(|(i, _)| *i != self.0)
            .count()
    }

    /// Converts byte position into a character.
    pub(crate) fn char_from(&self, text: &str) -> char {
        text[self.0..].chars().next().unwrap()
    }
}

#[cfg(test)]
mod text_path_line_tests {
    use super::*;

    fn text() -> Text {
        let bounds = tiny_skia_path::Rect::from_xywh(0.0, 0.0, 1.0, 1.0).unwrap();
        Text {
            origin: crate::tree::owner::Origin::Plain,
            id: TreeString::plain("line-normal-test".into()),
            rendering_mode: crate::TextRendering::default(),
            dx: crate::NumberList::from_static(&[]),
            dy: crate::NumberList::from_static(&[]),
            rotate: crate::NumberList::from_static(&[]),
            writing_mode: WritingMode::LeftToRight,
            direction: 0,
            chunks: TreeVec::plain(Vec::new()),
            abs_transform: Transform::identity(),
            bounding_box: bounds,
            abs_bounding_box: bounds,
            stroke_bounding_box: bounds,
            abs_stroke_bounding_box: bounds,
            flattened: crate::SceneBox::try_new_with(&crate::ProcessingContext::default(), || {
                Ok(crate::Group::empty())
            })
            .unwrap(),
            layouted: TreeVec::plain(Vec::new()),
        }
    }

    fn cluster(width: f32, advance: f32) -> GlyphCluster {
        GlyphCluster {
            byte_idx: ByteIndex::new(0),
            source_end: 1,
            logical_parent: None,
            resolved_script: None,
            spacing_eligible: true,
            spacing_after: 0.0,
            inline_scale: 1.0,
            codepoint: 'a',
            width,
            advance,
            text_length_spacing: 0.0,
            ascent: 1.0,
            descent: 0.0,
            has_relative_shift: false,
            glyphs: TreeVec::plain(Vec::new()),
            transform: Transform::identity(),
            path_transform: Transform::identity(),
            visible: true,
        }
    }

    fn path(build: impl FnOnce(&mut tiny_skia_path::PathBuilder)) -> tiny_skia_path::Path {
        let mut builder = tiny_skia_path::PathBuilder::new();
        build(&mut builder);
        builder.finish().unwrap()
    }

    fn sample(path: &tiny_skia_path::Path, distances: &[f32]) -> Vec<Option<PathNormal>> {
        assert!(!distances.is_empty());
        assert!(distances.windows(2).all(|pair| pair[0] <= pair[1]));
        let clusters: Vec<_> = distances
            .iter()
            .enumerate()
            .map(|(i, distance)| {
                cluster(
                    0.0,
                    distances.get(i + 1).map_or(0.0, |next| next - distance),
                )
            })
            .collect();
        let normals = collect_normals(
            &text(),
            &clusters,
            &vec![(0.0, 0.0); clusters.len()],
            path,
            distances[0],
        );
        assert_eq!(normals.len(), distances.len());
        for normal in normals.iter().flatten() {
            assert!(normal.x.is_finite() && normal.y.is_finite() && normal.angle.is_finite());
        }
        normals
    }

    fn normal(value: &Option<PathNormal>, x: f32, y: f32, angle: f32) {
        let value = value.as_ref().expect("distance lies on a nonzero segment");
        assert_eq!([value.x, value.y], [x, y]);
        assert!(
            (value.angle - angle).abs() <= 0.00001,
            "{} != {angle}",
            value.angle
        );
    }

    #[test]
    fn p07_line_midpoints_have_exact_frozen_reference_origins() {
        // Independent frozen P07 geometry, not values recomputed with inv_arclen.
        const EXPECTED_X: [f32; 4] = [16.0, 32.625, 48.625, 81.25];
        const OLD_X: [f32; 4] = [15.975594, 32.48709, 48.642372, 81.28691];
        let widths = [16.625, 16.0, 32.625, 32.625];
        let clusters: Vec<_> = widths.iter().map(|&width| cluster(width, width)).collect();
        let line = path(|p| {
            p.move_to(16.0, 200.0);
            p.line_to(740.0, 200.0);
        });
        let normals = collect_normals(&text(), &clusters, &[(0.0, 0.0); 4], &line, 0.0);
        assert_eq!(normals.len(), 4);
        let actual: Vec<_> = normals
            .iter()
            .zip(widths)
            .map(|(value, width)| {
                let value = value.as_ref().unwrap();
                assert_eq!([value.y, value.angle], [200.0, 0.0]);
                value.x - width / 2.0
            })
            .collect();
        assert_eq!(
            actual, EXPECTED_X,
            "frozen old implementation origins: {OLD_X:?}"
        );
    }

    #[test]
    fn horizontal_vertical_diagonal_and_reverse_lines_use_distance_and_direction() {
        let horizontal = path(|p| {
            p.move_to(10.0, 20.0);
            p.line_to(110.0, 20.0);
        });
        let values = sample(&horizontal, &[0.0, 25.0, 100.0]);
        normal(&values[0], 10.0, 20.0, 0.0);
        normal(&values[1], 35.0, 20.0, 0.0);
        normal(&values[2], 110.0, 20.0, 0.0);

        let vertical = path(|p| {
            p.move_to(2.0, 3.0);
            p.line_to(2.0, 23.0);
        });
        let values = sample(&vertical, &[0.0, 5.0, 20.0]);
        normal(&values[0], 2.0, 3.0, 90.0);
        normal(&values[1], 2.0, 8.0, 90.0);
        normal(&values[2], 2.0, 23.0, 90.0);

        let diagonal = path(|p| {
            p.move_to(0.0, 0.0);
            p.line_to(3.0, 4.0);
        });
        let values = sample(&diagonal, &[0.0, 2.5, 5.0]);
        normal(&values[0], 0.0, 0.0, 53.130104);
        normal(&values[1], 1.5, 2.0, 53.130104);
        normal(&values[2], 3.0, 4.0, 53.130104);

        let reverse = path(|p| {
            p.move_to(3.0, 4.0);
            p.line_to(0.0, 0.0);
        });
        let values = sample(&reverse, &[0.0, 2.5, 5.0]);
        normal(&values[0], 3.0, 4.0, -126.869896);
        normal(&values[1], 1.5, 2.0, -126.869896);
        normal(&values[2], 0.0, 0.0, -126.869896);
    }

    #[test]
    fn segment_corner_uses_first_closed_interval_and_keeps_outside_slots() {
        let line = path(|p| {
            p.move_to(0.0, 0.0);
            p.line_to(10.0, 0.0);
            p.line_to(10.0, 10.0);
        });
        let values = sample(&line, &[-2.0, -1.0, 0.0, 9.0, 10.0, 11.0, 20.0, 21.0]);
        assert!(values[0].is_none() && values[1].is_none() && values[7].is_none());
        normal(&values[2], 0.0, 0.0, 0.0);
        normal(&values[3], 9.0, 0.0, 0.0);
        normal(&values[4], 10.0, 0.0, 0.0);
        normal(&values[5], 10.0, 1.0, 90.0);
        normal(&values[6], 10.0, 10.0, 90.0);
    }

    #[test]
    fn close_returns_to_recent_move_and_subpaths_keep_cumulative_distance() {
        let line = path(|p| {
            p.move_to(0.0, 0.0);
            p.line_to(3.0, 4.0);
            p.close();
            p.line_to(0.0, 5.0);
            p.move_to(20.0, 30.0);
            p.line_to(23.0, 34.0);
            p.close();
        });
        let values = sample(&line, &[5.0, 7.5, 10.0, 12.5, 17.5, 22.5, 25.0, 26.0]);
        normal(&values[0], 3.0, 4.0, 53.130104);
        normal(&values[1], 1.5, 2.0, -126.869896);
        normal(&values[2], 0.0, 0.0, -126.869896);
        normal(&values[3], 0.0, 2.5, 90.0);
        normal(&values[4], 21.5, 32.0, 53.130104);
        normal(&values[5], 21.5, 32.0, -126.869896);
        normal(&values[6], 20.0, 30.0, -126.869896);
        assert!(values[7].is_none());
    }

    #[test]
    fn zero_length_lines_and_closes_do_not_claim_normals_or_divide_by_zero() {
        let line = path(|p| {
            p.move_to(1.0, 2.0);
            p.line_to(1.0, 2.0);
            p.close();
            p.line_to(1.0, 7.0);
            p.line_to(1.0, 7.0);
            p.line_to(1.0, 12.0);
            p.line_to(1.0, 12.0);
        });
        let values = sample(&line, &[0.0, 2.5, 5.0, 7.5, 10.0, 11.0]);
        for (value, y) in values[..5].iter().zip([2.0, 4.5, 7.0, 9.5, 12.0]) {
            normal(value, 1.0, y, 90.0);
        }
        assert!(values[5].is_none());
        let zero = path(|p| {
            p.move_to(4.0, 5.0);
            p.line_to(4.0, 5.0);
            p.close();
        });
        let values = sample(&zero, &[-1.0, 0.0, 1.0]);
        assert!(values.iter().all(Option::is_none));
    }

    #[test]
    fn curve_samples_preserve_the_archived_pre_repair_normal_bits() {
        // Actual old-algorithm samples archived in BEFORE-FIX-TEST-V1.log.
        // The expectations do not run or duplicate an inverse-arclength oracle.
        let quad = path(|p| {
            p.move_to(0.0, 0.0);
            p.quad_to(5.0, 10.0, 10.0, 0.0);
        });
        let cubic = path(|p| {
            p.move_to(0.0, 0.0);
            p.cubic_to(4.0, 8.0, 8.0, 8.0, 12.0, 0.0);
        });
        let quad_bits = [
            [0, 0, 1115536739],
            [1066948347, 1074148736, 1113778378],
            [1077035593, 1082170313, 1109796362],
            [1087420087, 1083255592, 3254432898],
        ];
        let cubic_bits = [
            [0, 0, 1115536739],
            [1066836837, 1074257258, 1114143017],
            [1076869608, 1082623149, 1111314445],
            [1086464293, 1086323188, 3215107850],
        ];
        for (name, curve, expected) in [("quad", quad, quad_bits), ("cubic", cubic, cubic_bits)] {
            let values = sample(&curve, &[0.0, 2.0, 5.0, 9.0]);
            assert!(values.iter().all(Option::is_some));
            let bits: Vec<_> = values
                .iter()
                .map(|value| {
                    let value = value.as_ref().unwrap();
                    [value.x.to_bits(), value.y.to_bits(), value.angle.to_bits()]
                })
                .collect();
            assert_eq!(bits, expected, "unchanged {name} branch");
            normal(&values[0], 0.0, 0.0, 63.434948);
        }
        let mixed = path(|p| {
            p.move_to(0.0, 0.0);
            p.line_to(10.0, 0.0);
            p.cubic_to(14.0, 8.0, 18.0, 8.0, 22.0, 0.0);
        });
        let values = sample(&mixed, &[0.0, 10.0, 12.0, 15.0, 19.0]);
        assert!(values.iter().all(Option::is_some));
        let bits: Vec<_> = values
            .iter()
            .map(|value| {
                let value = value.as_ref().unwrap();
                [value.x.to_bits(), value.y.to_bits(), value.angle.to_bits()]
            })
            .collect();
        assert_eq!(
            bits[2..],
            [
                [1093850221, 1074257258, 1114143017],
                [1095495290, 1082623149, 1111314445],
                [1098942537, 1086323188, 3215107850],
            ],
            "archived curve samples after the line segment"
        );
        normal(&values[0], 0.0, 0.0, 0.0);
        normal(&values[1], 10.0, 0.0, 0.0);
    }
}

#[cfg(test)]
#[path = "bidi_resource_tests.rs"]
mod bidi_resource_tests;

#[cfg(test)]
#[path = "font_consumer_tail_tests.rs"]
mod font_consumer_tail_tests;
#[cfg(test)]
#[path = "geometry_layout_tests.rs"]
mod geometry_layout_tests;

macro_rules! layout_leaf {($($ty:ty),* $(,)?)=>{$(
 impl PolicyValue for $ty{fn validate_policy(&self,p:&crate::ProcessingContext)->GeometryResult<()>{owner::check(p,0)}}
 impl TryCopy for $ty{fn try_copy_in(&self,p:&crate::ProcessingContext)->GeometryResult<Self>{owner::check(p,0)?;Ok(*self)}}
)*}}
layout_leaf!(
    ResolvedFont,
    ByteIndex,
    char,
    GlyphId,
    ID,
    unicode_script::Script,
    crate::ReplayGlyphIdentity,
    crate::ReplayGeometryOrigin,
    crate::ReplayPlacementPolicy
);
impl PolicyValue for std::ops::Range<usize> {
    fn validate_policy(&self, p: &crate::ProcessingContext) -> GeometryResult<()> {
        owner::check(p, 0)
    }
}
impl TryCopy for std::ops::Range<usize> {
    fn try_copy_in(&self, p: &crate::ProcessingContext) -> GeometryResult<Self> {
        owner::check(p, 0)?;
        Ok(self.start..self.end)
    }
}
macro_rules! layout_record {
 ($ty:ty{$($field:ident),* $(,)?})=>{
 impl PolicyValue for $ty{fn validate_policy(&self,p:&crate::ProcessingContext)->GeometryResult<()>{owner::check(p,0)?;$(PolicyValue::validate_policy(&self.$field,p)?;)*owner::check(p,0)}}
 impl TryCopy for $ty{fn try_copy_in(&self,p:&crate::ProcessingContext)->GeometryResult<Self>{self.validate_policy(p)?;let result=(||Ok(Self{$($field:TryCopy::try_copy_in(&self.$field,p)?),*}))();owner::finish(p,result)}}
 }
}
layout_record!(PositionedGlyph {
    local_representation,
    glyph_ts,
    source_span,
    source_range,
    raw_x_advance,
    raw_y_advance,
    resolved_context,
    layout_font,
    writing_transform,
    progression_start,
    progression_advance,
    cluster_ts,
    span_ts,
    units_per_em,
    font_size,
    geometry_origin,
    placement_policy,
    engine_metrics_midpoint,
    id,
    text,
    font
});
layout_record!(Span {
    computed_color,
    fill,
    stroke,
    paint_order,
    font_size,
    variations,
    font_optical_sizing,
    visible,
    positioned_glyphs,
    underline,
    overline,
    line_through
});
layout_record!(GlyphCluster {
    byte_idx,
    source_end,
    logical_parent,
    resolved_script,
    spacing_eligible,
    spacing_after,
    inline_scale,
    codepoint,
    width,
    advance,
    text_length_spacing,
    ascent,
    descent,
    has_relative_shift,
    glyphs,
    transform,
    path_transform,
    visible
});
layout_record!(Glyph {
    identity,
    raw_y_advance,
    id,
    byte_idx,
    cluster_len,
    text,
    dx,
    dy,
    width,
    fallback_scale,
    geometry_origin,
    font
});

impl Span {
    pub(crate) fn queue_policy<'a>(
        &'a self,
        w: &mut crate::tree::validate::Walker<'a, '_>,
    ) -> GeometryResult<()> {
        use crate::tree::validate::Frame;
        let p = w.processing;
        self.variations.validate_container(p)?;
        self.positioned_glyphs.validate_container(p)?;
        if let Some(v) = &self.fill {
            w.push(Frame::Fill(v))?;
        }
        if let Some(v) = &self.stroke {
            w.push(Frame::Stroke(v))?;
        }
        for v in [&self.underline, &self.overline, &self.line_through]
            .into_iter()
            .flatten()
        {
            w.push(Frame::Path(v))?;
        }
        for v in &self.positioned_glyphs {
            w.push(Frame::PositionedGlyph(v))?;
        }
        owner::check(p, 0)
    }
}
impl PositionedGlyph {
    pub(crate) fn queue_policy<'a>(
        &'a self,
        w: &mut crate::tree::validate::Walker<'a, '_>,
    ) -> GeometryResult<()> {
        self.text.validate_policy(w.processing)?;
        if let Some(v) = &self.local_representation {
            w.representation(v)?;
        }
        owner::check(w.processing, 0)
    }
}
