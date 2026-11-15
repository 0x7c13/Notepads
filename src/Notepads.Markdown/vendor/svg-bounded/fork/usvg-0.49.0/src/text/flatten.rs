// Copyright 2022 the Resvg Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT
// Modifications Copyright (c) 2026, Jiaqi (0x7c13) Liu.

use crate::tree::owner::{self, Origin, PolicyValue, TreeString, TreeVec, TryCopy};
use std::mem;
use std::sync::Arc;

use super::colr_storage::{ColrPath, ColrWriter};
use crate::GlyphId;
use fontdb::{Database, ID};
use skrifa::bitmap::{BitmapData, BitmapFormat};
use skrifa::outline::OutlinePen;
use skrifa::prelude::LocationRef;
use skrifa::raw::types::BoundingBox;
use skrifa::raw::TableProvider as _;
use skrifa::MetadataProvider;
use skrifa::Tag;
use tiny_skia_path::{NonZeroRect, Size, Transform};

use crate::text::colr::GlyphPainter;
use crate::text::OPSZ;
use crate::*;

use super::coverage::Capture;
use super::representation::LocalPayload;
pub(crate) use super::representation::{
    select_local, LocalInk, LocalRepresentation, RepresentationRef,
};

fn resolve_rendering_mode(text: &Text) -> ShapeRendering {
    match text.rendering_mode {
        TextRendering::OptimizeSpeed => ShapeRendering::CrispEdges,
        TextRendering::OptimizeLegibility => ShapeRendering::GeometricPrecision,
        TextRendering::GeometricPrecision => ShapeRendering::GeometricPrecision,
    }
}

/// Returns the effective variation settings for a glyph: the span's explicit
/// variations plus an automatically computed `opsz` value when
/// `font-optical-sizing: auto` is in effect and the font has an `opsz` axis
/// that wasn't set explicitly. This matches browser behavior
/// (CSS font-optical-sizing: auto).
enum EffectiveAxes<'a> {
    Borrowed(&'a [FontVariation]),
    SourceShared(Arc<[FontVariation]>),
    Compat(Vec<FontVariation>),
    Paid(markdown_svg_budget::BudgetVec<FontVariation>),
}

impl std::ops::Deref for EffectiveAxes<'_> {
    type Target = [FontVariation];
    fn deref(&self) -> &[FontVariation] {
        match self {
            Self::Borrowed(value) => value,
            Self::SourceShared(value) => value,
            Self::Compat(value) => value,
            Self::Paid(value) => value,
        }
    }
}

fn retire_effective_axes(value: EffectiveAxes<'_>, processing: &ProcessingContext) -> Option<()> {
    drop(value);
    if let Some(resources) = processing.resources() {
        if let Err(error) = resources.checkpoint(0) {
            processing.record_resource(error);
            return None;
        }
    }
    if processing.failed() {
        None
    } else {
        Some(())
    }
}

fn effective_variations<'a>(
    cache: &mut Cache,
    span: &'a layout::Span,
    glyph: &layout::PositionedGlyph,
) -> Option<EffectiveAxes<'a>> {
    match try_effective_variations(cache, span, glyph) {
        Ok(value) => Some(value),
        Err(error) => {
            cache.processing.record_resource(error);
            None
        }
    }
}

fn try_effective_variations<'a>(
    cache: &mut Cache,
    span: &'a layout::Span,
    glyph: &layout::PositionedGlyph,
) -> Result<EffectiveAxes<'a>, markdown_svg_budget::Error> {
    use markdown_svg_budget::{BudgetVec, Error, FailureKind, LimitKind, ResourceKind};
    let resources = cache.processing.resources().cloned();
    if let Some(resources) = &resources {
        resources.checkpoint(0)?;
    }
    if let Some(owner) = cache.font_replay.as_ref() {
        return match owner.variations(glyph.font) {
            Ok(value) => Ok(EffectiveAxes::SourceShared(value)),
            Err(()) => {
                cache.processing.fail(ProcessingFailure::FontReplay);
                Err(cache
                    .processing
                    .resource_failure()
                    .expect("recorded font replay failure"))
            }
        };
    }
    let mut explicit_opsz = false;
    if span.font_optical_sizing == crate::FontOpticalSizing::Auto {
        for value in &span.variations {
            if let Some(resources) = &resources {
                resources.checkpoint(1)?;
            }
            if &value.tag == b"opsz" {
                explicit_opsz = true;
                break;
            }
        }
    }
    if span.font_optical_sizing != crate::FontOpticalSizing::Auto
        || explicit_opsz
        || !cache.has_opsz_axis(glyph.font)
    {
        if let Some(resources) = &resources {
            resources.checkpoint(0)?;
        }
        return Ok(EffectiveAxes::Borrowed(&span.variations));
    }
    let value = FontVariation::new(*b"opsz", glyph.font_size());
    match resources {
        None => {
            let mut values = span.variations.to_vec();
            values.push(value);
            Ok(EffectiveAxes::Compat(values))
        }
        Some(resources) => {
            let capacity = span.variations.len().checked_add(1).ok_or_else(|| {
                resources.record(Error::new(FailureKind::Quota, LimitKind::Capacity))
            })?;
            let mut values = BudgetVec::new(resources.clone(), ResourceKind::Scratch);
            values.try_reserve_exact(capacity)?;
            for chunk in span.variations.chunks(256) {
                resources.checkpoint(chunk.len() as u64)?;
                values.extend_reserved_copy(chunk);
            }
            resources.checkpoint(1)?;
            values.extend_reserved_copy(&[value]);
            resources.checkpoint(0)?;
            Ok(EffectiveAxes::Paid(values))
        }
    }
}

fn geometry_ready(processing: &ProcessingContext) -> Option<()> {
    if let Some(resources) = processing.resources() {
        if let Err(error) = resources.checkpoint(0) {
            processing.record_resource(error);
            return None;
        }
    }
    if processing.failed() {
        None
    } else {
        Some(())
    }
}

fn geometry_value<T>(
    processing: &ProcessingContext,
    result: Result<T, markdown_svg_budget::Error>,
) -> Option<T> {
    match result {
        Ok(value) => {
            geometry_ready(processing)?;
            Some(value)
        }
        Err(error) => {
            processing.record_resource(error);
            None
        }
    }
}

fn span_builder(processing: &ProcessingContext) -> tiny_skia_path::PathBuilder {
    match processing.resources() {
        Some(resources) => tiny_skia_path::PathBuilder::with_resources(
            resources,
            markdown_svg_budget::ResourceKind::Geometry,
        ),
        None => tiny_skia_path::PathBuilder::new(),
    }
}

fn transform_outline(
    path: tiny_skia_path::Path,
    transform: Transform,
    processing: &ProcessingContext,
) -> Option<Option<tiny_skia_path::Path>> {
    let candidate = match processing.resources() {
        Some(resources) => path.try_transform(
            transform,
            resources,
            markdown_svg_budget::ResourceKind::Scratch,
        ),
        None => Ok(path.transform(transform)),
    };
    geometry_value(processing, candidate)
}

fn append_outline(
    builder: &mut tiny_skia_path::PathBuilder,
    outline: Option<tiny_skia_path::Path>,
    processing: &ProcessingContext,
) -> Option<bool> {
    let contributed = outline.is_some();
    if let Some(outline) = outline {
        let result = builder.try_push_path(&outline);
        drop(outline);
        geometry_value(processing, result)?;
    }
    geometry_ready(processing)?;
    Some(contributed)
}

fn make_outline_path(
    span: &layout::Span,
    fill: Option<Fill>,
    data: tiny_skia_path::Path,
    rendering_mode: ShapeRendering,
    abs_transform: Transform,
    processing: &ProcessingContext,
) -> Option<Option<Path>> {
    let candidate = match processing.resources() {
        Some(resources) => Path::try_new(
            "",
            span.visible,
            fill,
            span.stroke.try_copy_in(processing).ok()?,
            span.paint_order,
            rendering_mode,
            data,
            abs_transform,
            resources,
        ),
        None => Ok(Path::new(
            "",
            span.visible,
            fill,
            span.stroke.try_copy_in(processing).ok()?,
            span.paint_order,
            rendering_mode,
            Arc::new(data),
            abs_transform,
        )),
    };
    geometry_value(processing, candidate)
}

fn push_outline_paths(
    span: &layout::Span,
    builder: &mut tiny_skia_path::PathBuilder,
    new_children: &mut TreeVec<Node>,
    rendering_mode: ShapeRendering,
    abs_transform: Transform,
    capture: &mut Capture,
    processing: &ProcessingContext,
) -> Option<()> {
    geometry_ready(processing)?;
    let builder = mem::replace(builder, span_builder(processing));
    let data = geometry_value(processing, builder.try_finish())?;
    let path = match data {
        Some(data) => make_outline_path(
            span,
            span.fill.try_copy_in(processing).ok()?,
            data,
            rendering_mode,
            abs_transform,
            processing,
        )?,
        None => None,
    };
    let mut path = match path {
        Some(path) => Some(geometry_value(
            processing,
            crate::scene::boxed_owned(processing, crate::scene::SceneCost::path(&path), path),
        )?),
        None => None,
    };
    geometry_ready(processing)?;
    capture.bind(path.as_mut().map(crate::SceneBox::as_mut), processing)?;
    if let Some(path) = path {
        new_children.try_push(Node::Path(path), processing).ok()?;
    }
    Some(())
}

fn observe_geometry(cache: &mut Cache, value: crate::ReplayGeometryObservation<'_>) -> Option<()> {
    geometry_ready(&cache.processing)?;
    if let Some(owner) = cache.font_replay.as_ref() {
        if owner.observe_geometry(value).is_err() {
            cache.processing.fail(ProcessingFailure::FontReplay);
            return None;
        }
    }
    Some(())
}

fn flatten_synthetic(
    data: Option<crate::SharedOwner<tiny_skia_path::Path>>,
    rule: FillRule,
    glyph: &layout::PositionedGlyph,
    span: &layout::Span,
    element_id: &str,
    cache: &mut Cache,
    span_builder: &mut tiny_skia_path::PathBuilder,
    new_children: &mut TreeVec<Node>,
    rendering_mode: ShapeRendering,
    abs_transform: Transform,
    capture: &mut Capture,
) -> Option<()> {
    geometry_ready(&cache.processing)?;
    // The selected source already crossed the explicit callback import boundary.
    // Check its real shared header and every opaque P owner before observing it.
    data.validate_policy(&cache.processing).ok()?;
    // An absent outline has no paint boundary, regardless of its nominal rule.
    if data.is_some() && rule != FillRule::NonZero {
        push_outline_paths(
            span,
            span_builder,
            new_children,
            rendering_mode,
            abs_transform,
            capture,
            &cache.processing,
        )?;
    }
    let transform = glyph.outline_transform();
    let local_outline_present = data.is_some();
    observe_geometry(
        cache,
        glyph.replay_observation(
            element_id,
            1,
            transform,
            abs_transform,
            data.as_deref(),
            span,
            rule,
        ),
    )?;
    let outline = match data {
        Some(data) => {
            let copied = match cache.processing.resources() {
                Some(resources) => data
                    .as_ref()
                    .try_copy_in(resources, markdown_svg_budget::ResourceKind::Scratch),
                None => Ok(data.as_ref().clone()),
            };
            drop(data);
            geometry_ready(&cache.processing)?;
            let copied = geometry_value(&cache.processing, copied)?;
            transform_outline(copied, transform, &cache.processing)?
        }
        None => None,
    };
    if rule == FillRule::NonZero || !local_outline_present {
        let contributed_to_path = append_outline(span_builder, outline, &cache.processing)?;
        return capture.record(
            glyph,
            GlyphOutlineKind::Synthetic,
            local_outline_present,
            contributed_to_path,
            transform,
            &cache.processing,
        );
    }

    let path = match outline {
        Some(outline) => {
            let mut fill = span.fill.try_copy_in(&cache.processing).ok()?;
            if let Some(fill) = fill.as_mut() {
                fill.rule = rule;
            }
            make_outline_path(
                span,
                fill,
                outline,
                rendering_mode,
                abs_transform,
                &cache.processing,
            )?
        }
        None => None,
    };
    let mut path = match path {
        Some(path) => Some(geometry_value(
            &cache.processing,
            crate::scene::boxed_owned(
                &cache.processing,
                crate::scene::SceneCost::path(&path),
                path,
            ),
        )?),
        None => None,
    };
    geometry_ready(&cache.processing)?;
    capture.record(
        glyph,
        GlyphOutlineKind::Synthetic,
        local_outline_present,
        path.is_some(),
        transform,
        &cache.processing,
    )?;
    capture.bind(
        path.as_mut().map(crate::SceneBox::as_mut),
        &cache.processing,
    )?;
    if let Some(path) = path {
        new_children
            .try_push(Node::Path(path), &cache.processing)
            .ok()?;
    }
    Some(())
}

fn flatten_selected(
    local: RepresentationRef<LocalRepresentation>,
    glyph: &layout::PositionedGlyph,
    span: &layout::Span,
    element_id: &str,
    cache: &mut Cache,
    span_builder: &mut tiny_skia_path::PathBuilder,
    new_children: &mut TreeVec<Node>,
    rendering_mode: ShapeRendering,
    abs_transform: Transform,
    capture: &mut Capture,
) -> Option<()> {
    let payload = LocalRepresentation::into_payload(local, cache)?;
    let rule = span
        .fill
        .as_ref()
        .map_or(FillRule::NonZero, |fill| fill.rule());
    match payload {
        LocalPayload::Colr(root) => {
            let transform = glyph.colr_transform();
            observe_geometry(
                cache,
                glyph.replay_observation(element_id, 2, transform, abs_transform, None, span, rule),
            )?;
            push_outline_paths(
                span,
                span_builder,
                new_children,
                rendering_mode,
                abs_transform,
                capture,
                &cache.processing,
            )?;
            let mut group = Group {
                transform,
                ..Group::empty_in(&cache.processing)
            };
            // The same-request parsed/cached COLR root, or its admitted
            // representation copy, is moved without a second identity.
            group
                .children
                .try_push(
                    Node::Group(geometry_value(
                        &cache.processing,
                        crate::scene::moved_root(&cache.processing, root),
                    )?),
                    &cache.processing,
                )
                .ok()?;
            group.calculate_bounding_boxes();
            new_children
                .try_push(
                    Node::Group(geometry_value(
                        &cache.processing,
                        crate::scene::boxed_owned(
                            &cache.processing,
                            crate::scene::SceneCost::NODE,
                            group,
                        ),
                    )?),
                    &cache.processing,
                )
                .ok()?;
        }
        LocalPayload::Svg(node) => {
            let transform = glyph.svg_transform();
            observe_geometry(
                cache,
                glyph.replay_observation(element_id, 3, transform, abs_transform, None, span, rule),
            )?;
            push_outline_paths(
                span,
                span_builder,
                new_children,
                rendering_mode,
                abs_transform,
                capture,
                &cache.processing,
            )?;
            let mut group = Group {
                transform,
                ..Group::empty_in(&cache.processing)
            };
            group.children.try_push(node, &cache.processing).ok()?;
            group.calculate_bounding_boxes();
            new_children
                .try_push(
                    Node::Group(geometry_value(
                        &cache.processing,
                        crate::scene::boxed_owned(
                            &cache.processing,
                            crate::scene::SceneCost::NODE,
                            group,
                        ),
                    )?),
                    &cache.processing,
                )
                .ok()?;
        }
        LocalPayload::Bitmap(image) => {
            push_outline_paths(
                span,
                span_builder,
                new_children,
                rendering_mode,
                abs_transform,
                capture,
                &cache.processing,
            )?;
            let transform = if image.is_sbix {
                glyph.sbix_transform(
                    image.x as f32,
                    image.y as f32,
                    image.glyph_bbox.map_or(0, |bbox| bbox.x_min) as f32,
                    image.glyph_bbox.map_or(0, |bbox| bbox.y_min) as f32,
                    image.pixels_per_em as f32,
                    image.image.size.height(),
                )
            } else {
                glyph.cbdt_transform(image.x as f32, image.y as f32, image.pixels_per_em as f32)
            };
            observe_geometry(
                cache,
                glyph.replay_observation(element_id, 4, transform, abs_transform, None, span, rule),
            )?;
            let mut group = Group {
                transform,
                ..Group::empty_in(&cache.processing)
            };
            group
                .children
                .try_push(
                    Node::Image(geometry_value(
                        &cache.processing,
                        crate::scene::boxed_owned(
                            &cache.processing,
                            crate::scene::SceneCost::NODE,
                            image.image,
                        ),
                    )?),
                    &cache.processing,
                )
                .ok()?;
            group.calculate_bounding_boxes();
            new_children
                .try_push(
                    Node::Group(geometry_value(
                        &cache.processing,
                        crate::scene::boxed_owned(
                            &cache.processing,
                            crate::scene::SceneCost::NODE,
                            group,
                        ),
                    )?),
                    &cache.processing,
                )
                .ok()?;
        }
        LocalPayload::Ordinary(path) => {
            let transform = glyph.outline_transform();
            let local_outline_present = path.is_some();
            observe_geometry(
                cache,
                glyph.replay_observation(
                    element_id,
                    0,
                    transform,
                    abs_transform,
                    path.as_ref(),
                    span,
                    rule,
                ),
            )?;
            let path = match path {
                Some(path) => transform_outline(path, transform, &cache.processing)?,
                None => None,
            };
            let contributed_to_path = append_outline(span_builder, path, &cache.processing)?;
            capture.record(
                glyph,
                GlyphOutlineKind::Ordinary,
                local_outline_present,
                contributed_to_path,
                transform,
                &cache.processing,
            )?;
        }
        LocalPayload::Synthetic { data, rule } => {
            flatten_synthetic(
                data,
                rule,
                glyph,
                span,
                element_id,
                cache,
                span_builder,
                new_children,
                rendering_mode,
                abs_transform,
                capture,
            )?;
        }
    }
    geometry_ready(&cache.processing)
}

pub(crate) fn flatten(text: &mut Text, cache: &mut Cache) -> Option<(Group, NonZeroRect)> {
    let processing = cache.processing.clone();
    let candidate = flatten_inner(text, cache);
    geometry_ready(&processing)?;
    candidate
}

fn flatten_inner(text: &mut Text, cache: &mut Cache) -> Option<(Group, NonZeroRect)> {
    let mut new_children = TreeVec::new(&cache.processing);
    let mut capture = Capture::new(cache)?;

    let abs_transform = text.abs_transform;
    let rendering_mode = resolve_rendering_mode(text);

    for span in &mut text.layouted {
        geometry_ready(&cache.processing)?;
        if let Some(resources) = cache.processing.resources() {
            geometry_value(&cache.processing, resources.checkpoint(1))?;
        }
        if let Some(path) = span.overline.as_ref() {
            let mut path = geometry_value(
                &cache.processing,
                crate::scene::clone_boxed(&cache.processing, path),
            )?;
            path.as_mut().rendering_mode = rendering_mode;
            new_children
                .try_push(Node::Path(path), &cache.processing)
                .ok()?;
        }

        if let Some(path) = span.underline.as_ref() {
            let mut path = geometry_value(
                &cache.processing,
                crate::scene::clone_boxed(&cache.processing, path),
            )?;
            path.as_mut().rendering_mode = rendering_mode;
            new_children
                .try_push(Node::Path(path), &cache.processing)
                .ok()?;
        }

        // Instead of always processing each glyph separately, we always collect
        // as many outline glyphs as possible by pushing them into the span_builder
        // and only if we encounter a different glyph, or we reach the very end of the
        // span to we push the actual outline paths into new_children. This way, we don't need
        // to create a new path for every glyph if we have many consecutive glyphs
        // with just outlines (which is the most common case).
        let mut span_builder = span_builder(&cache.processing);

        for glyph_index in 0..span.positioned_glyphs.len() {
            geometry_ready(&cache.processing)?;
            if let Some(resources) = cache.processing.resources() {
                geometry_value(&cache.processing, resources.checkpoint(1))?;
            }
            if cache.processing.failed() {
                return None;
            }
            let selected = span.positioned_glyphs[glyph_index]
                .local_representation
                .take();
            let glyph = &span.positioned_glyphs[glyph_index];
            if let Some(selected) = selected {
                flatten_selected(
                    selected,
                    glyph,
                    span,
                    &text.id,
                    cache,
                    &mut span_builder,
                    &mut new_children,
                    rendering_mode,
                    abs_transform,
                    &mut capture,
                )?;
                geometry_ready(&cache.processing)?;
                continue;
            }
            if glyph.source_span().is_some() {
                cache.processing.fail(ProcessingFailure::FontReplay);
                return None;
            }
            let variations = effective_variations(cache, span, glyph)?;

            // A (best-effort conversion of a) COLR glyph.
            if let Some(tree) = {
                let tree =
                    cache.fontdb_colr(glyph.font, glyph.id, &variations, span.computed_color);
                geometry_ready(&cache.processing)?;
                tree
            } {
                retire_effective_axes(variations, &cache.processing)?;
                observe_geometry(
                    cache,
                    glyph.replay_observation(
                        &text.id,
                        2,
                        glyph.colr_transform(),
                        abs_transform,
                        None,
                        span,
                        span.fill.as_ref().map_or(FillRule::NonZero, |f| f.rule()),
                    ),
                )?;
                push_outline_paths(
                    span,
                    &mut span_builder,
                    &mut new_children,
                    rendering_mode,
                    abs_transform,
                    &mut capture,
                    &cache.processing,
                )?;
                let mut group = Group {
                    transform: glyph.colr_transform(),
                    ..Group::empty_in(&cache.processing)
                };
                // TODO: Probably need to update abs_transform of children? Same
                // for SVG and bitmap glyphs.
                // Cache miss/hit already admitted this same-request Tree copy.
                group
                    .children
                    .try_push(
                        Node::Group(geometry_value(
                            &cache.processing,
                            tree.into_root_in(&cache.processing)
                                .and_then(|root| crate::scene::moved_root(&cache.processing, root)),
                        )?),
                        &cache.processing,
                    )
                    .ok()?;
                group.calculate_bounding_boxes();

                new_children
                    .try_push(
                        Node::Group(geometry_value(
                            &cache.processing,
                            crate::scene::boxed_owned(
                                &cache.processing,
                                crate::scene::SceneCost::NODE,
                                group,
                            ),
                        )?),
                        &cache.processing,
                    )
                    .ok()?;
            }
            // An SVG glyph. Will return the usvg node containing the glyph descriptions.
            else if let Some(node) = {
                let node = cache.fontdb_svg_with_color(glyph.font, glyph.id, span.computed_color);
                geometry_ready(&cache.processing)?;
                node
            } {
                retire_effective_axes(variations, &cache.processing)?;
                observe_geometry(
                    cache,
                    glyph.replay_observation(
                        &text.id,
                        3,
                        glyph.svg_transform(),
                        abs_transform,
                        None,
                        span,
                        span.fill.as_ref().map_or(FillRule::NonZero, |f| f.rule()),
                    ),
                )?;
                push_outline_paths(
                    span,
                    &mut span_builder,
                    &mut new_children,
                    rendering_mode,
                    abs_transform,
                    &mut capture,
                    &cache.processing,
                )?;

                let mut group = Group {
                    transform: glyph.svg_transform(),
                    ..Group::empty_in(&cache.processing)
                };
                group.children.try_push(node, &cache.processing).ok()?;
                group.calculate_bounding_boxes();

                new_children
                    .try_push(
                        Node::Group(geometry_value(
                            &cache.processing,
                            crate::scene::boxed_owned(
                                &cache.processing,
                                crate::scene::SceneCost::NODE,
                                group,
                            ),
                        )?),
                        &cache.processing,
                    )
                    .ok()?;
            }
            // A bitmap glyph.
            else if let Some(img) = {
                let image = cache.fontdb_raster(glyph.font, glyph.id);
                geometry_ready(&cache.processing)?;
                image
            } {
                retire_effective_axes(variations, &cache.processing)?;
                push_outline_paths(
                    span,
                    &mut span_builder,
                    &mut new_children,
                    rendering_mode,
                    abs_transform,
                    &mut capture,
                    &cache.processing,
                )?;

                let transform = if img.is_sbix {
                    glyph.sbix_transform(
                        img.x as f32,
                        img.y as f32,
                        img.glyph_bbox.map(|bbox| bbox.x_min).unwrap_or(0) as f32,
                        img.glyph_bbox.map(|bbox| bbox.y_min).unwrap_or(0) as f32,
                        img.pixels_per_em as f32,
                        img.image.size.height(),
                    )
                } else {
                    glyph.cbdt_transform(img.x as f32, img.y as f32, img.pixels_per_em as f32)
                };

                let mut group = Group {
                    transform,
                    ..Group::empty_in(&cache.processing)
                };
                observe_geometry(
                    cache,
                    glyph.replay_observation(
                        &text.id,
                        4,
                        transform,
                        abs_transform,
                        None,
                        span,
                        span.fill.as_ref().map_or(FillRule::NonZero, |f| f.rule()),
                    ),
                )?;
                group
                    .children
                    .try_push(
                        Node::Image(geometry_value(
                            &cache.processing,
                            crate::scene::boxed_owned(
                                &cache.processing,
                                crate::scene::SceneCost::NODE,
                                img.image,
                            ),
                        )?),
                        &cache.processing,
                    )
                    .ok()?;
                group.calculate_bounding_boxes();

                new_children
                    .try_push(
                        Node::Group(geometry_value(
                            &cache.processing,
                            crate::scene::boxed_owned(
                                &cache.processing,
                                crate::scene::SceneCost::NODE,
                                group,
                            ),
                        )?),
                        &cache.processing,
                    )
                    .ok()?;
            } else {
                if cache.processing.failed() {
                    return None;
                }
                let outline = if let Some(owner) = cache.font_replay.as_ref() {
                    match owner.outline(glyph.font, glyph.id.0) {
                        Ok(crate::ReplayOutline::Ordinary) => {
                            cache.fontdb_outline(glyph.font, glyph.id, &variations)
                        }
                        Ok(crate::ReplayOutline::Synthetic { data, rule }) => {
                            let data =
                                crate::geometry::import_outline(data, &cache.processing).ok()?;
                            retire_effective_axes(variations, &cache.processing)?;
                            flatten_synthetic(
                                data,
                                rule,
                                glyph,
                                span,
                                &text.id,
                                cache,
                                &mut span_builder,
                                &mut new_children,
                                rendering_mode,
                                abs_transform,
                                &mut capture,
                            )?;
                            continue;
                        }
                        Err(()) => {
                            cache.processing.fail(ProcessingFailure::FontReplay);
                            return None;
                        }
                    }
                } else {
                    cache.fontdb_outline(glyph.font, glyph.id, &variations)
                };

                retire_effective_axes(variations, &cache.processing)?;

                let transform = glyph.outline_transform();
                let local_outline_present = outline.is_some();
                observe_geometry(
                    cache,
                    glyph.replay_observation(
                        &text.id,
                        0,
                        transform,
                        abs_transform,
                        outline.as_ref(),
                        span,
                        span.fill.as_ref().map_or(FillRule::NonZero, |f| f.rule()),
                    ),
                )?;
                let outline = match outline {
                    Some(path) => transform_outline(path, transform, &cache.processing)?,
                    None => None,
                };
                let contributed_to_path =
                    append_outline(&mut span_builder, outline, &cache.processing)?;
                capture.record(
                    glyph,
                    GlyphOutlineKind::Ordinary,
                    local_outline_present,
                    contributed_to_path,
                    transform,
                    &cache.processing,
                )?;
            }
        }

        push_outline_paths(
            span,
            &mut span_builder,
            &mut new_children,
            rendering_mode,
            abs_transform,
            &mut capture,
            &cache.processing,
        )?;

        if let Some(path) = span.line_through.as_ref() {
            let mut path = geometry_value(
                &cache.processing,
                crate::scene::clone_boxed(&cache.processing, path),
            )?;
            path.as_mut().rendering_mode = rendering_mode;
            new_children
                .try_push(Node::Path(path), &cache.processing)
                .ok()?;
        }
    }

    let mut group = geometry_value(
        &cache.processing,
        crate::scene::try_root(&cache.processing, || {
            Ok(Group {
                id: text.id.try_copy_in(&cache.processing)?,
                ..Group::empty_in(&cache.processing)
            })
        }),
    )?;
    group
        .children
        .try_append(&mut new_children, &cache.processing)
        .ok()?;
    drop(new_children);
    geometry_ready(&cache.processing)?;

    group.calculate_bounding_boxes();
    let stroke_bbox = group.stroke_bounding_box().to_non_zero_rect()?;
    Some((group, stroke_bbox))
}

#[derive(Default)]
struct PathBuilder {
    builder: tiny_skia_path::PathBuilder,
    commands: usize,
    failure: Option<markdown_svg_budget::Error>,
}

impl PathBuilder {
    fn with_processing(processing: &ProcessingContext) -> Self {
        Self {
            builder: match processing.resources() {
                Some(resources) => tiny_skia_path::PathBuilder::with_resources_default(
                    resources,
                    markdown_svg_budget::ResourceKind::Scratch,
                ),
                None => tiny_skia_path::PathBuilder::default(),
            },
            commands: 0,
            failure: None,
        }
    }

    fn command(
        &mut self,
        action: impl FnOnce(
            &mut tiny_skia_path::PathBuilder,
        ) -> Result<(), markdown_svg_budget::Error>,
    ) {
        if self.failure.is_some() {
            return;
        }
        self.commands = self.commands.saturating_add(1);
        if let Err(error) = action(&mut self.builder) {
            self.failure = Some(error);
        }
    }
}

impl OutlinePen for PathBuilder {
    fn move_to(&mut self, x: f32, y: f32) {
        self.command(|builder| builder.try_move_to(x, y));
    }

    fn line_to(&mut self, x: f32, y: f32) {
        self.command(|builder| builder.try_line_to(x, y));
    }

    fn quad_to(&mut self, cx0: f32, cy0: f32, x: f32, y: f32) {
        self.command(|builder| builder.try_quad_to(cx0, cy0, x, y));
    }

    fn curve_to(&mut self, cx0: f32, cy0: f32, cx1: f32, cy1: f32, x: f32, y: f32) {
        self.command(|builder| builder.try_cubic_to(cx0, cy0, cx1, cy1, x, y));
    }

    fn close(&mut self) {
        self.command(|builder| builder.try_close());
    }
}

/// A successful empty draw is distinct from an absent/invalid outline or a
/// failed reader. Neither missing outlines nor zero bboxes establish NoInk.
pub(crate) enum OwnedOutline {
    Empty,
    NonEmpty(tiny_skia_path::Path),
    Unknown,
}

pub(crate) trait DatabaseExt {
    /// Probe the actual glyph's representations without decoding or painting.
    /// A color table elsewhere in the face does not make every glyph colored.
    fn has_color_glyph(
        &self,
        id: ID,
        glyph_id: GlyphId,
        processing: &ProcessingContext,
    ) -> Result<Option<bool>, markdown_svg_budget::Error>;
    fn outline(
        &self,
        id: ID,
        glyph_id: GlyphId,
        variations: &[crate::FontVariation],
        observer: Option<&dyn crate::FontReplay>,
        processing: &crate::ProcessingContext,
    ) -> Option<tiny_skia_path::Path>;
    fn owned_outline(
        &self,
        id: ID,
        glyph_id: GlyphId,
        variations: &[crate::FontVariation],
        observer: Option<&dyn crate::FontReplay>,
        processing: &crate::ProcessingContext,
    ) -> Result<OwnedOutline, markdown_svg_budget::Error>;
    fn has_opsz_axis(&self, id: ID, processing: &crate::ProcessingContext) -> bool;
    fn raster(
        &self,
        id: ID,
        glyph_id: GlyphId,
        processing: &crate::ProcessingContext,
    ) -> Result<Option<BitmapImage>, markdown_svg_budget::Error>;
    fn svg(
        &self,
        id: ID,
        glyph_id: GlyphId,
        current_color: svgtypes::Color,
        options: &Options,
    ) -> Result<Option<Node>, markdown_svg_budget::Error>;
    fn colr(
        &self,
        id: ID,
        glyph_id: GlyphId,
        variations: &[crate::FontVariation],
        foreground_color: svgtypes::Color,
        options: &Options,
    ) -> Result<Option<Tree>, markdown_svg_budget::Error>;
}

#[derive(Debug)]
pub(crate) struct BitmapImage {
    pub(crate) image: Image,
    x: i16,
    y: i16,
    pixels_per_em: u16,
    glyph_bbox: Option<BoundingBox<i16>>,
    is_sbix: bool,
}

impl DatabaseExt for Database {
    fn has_color_glyph(
        &self,
        id: ID,
        glyph_id: GlyphId,
        processing: &ProcessingContext,
    ) -> Result<Option<bool>, markdown_svg_budget::Error> {
        super::font_instance::with_source(self, id, processing, false, |data, face_index| {
            let Some(font) = super::font_instance::read_font(data, face_index, processing)? else {
                return Ok(None);
            };
            let (colr, svg, bitmap) = if let Some(r) = processing.resources() {
                let colr = skrifa::request::ColorGlyphs::try_new(&font, r)?
                    .try_get(glyph_id.into())?
                    .is_some();
                let mut cx = markdown_svg_budget::RequestControl(r);
                let svg = if font.try_table_data(Tag::new(b"SVG "), &mut cx)?.is_some() {
                    let Ok(table) = super::font_instance::read_table::<
                        skrifa::raw::tables::svg::Svg,
                    >(&font, r)?
                    else {
                        return Ok(None);
                    };
                    let Ok(data) = table.try_glyph_data(glyph_id.into(), &mut cx)? else {
                        return Ok(None);
                    };
                    data.is_some()
                } else {
                    false
                };
                let bitmap = skrifa::request::BitmapStrikes::try_new(&font, r)?
                    .try_glyph_for_size(skrifa::prelude::Size::unscaled(), glyph_id.into())?
                    .is_some_and(|image| {
                        matches!(
                            image.data(),
                            skrifa::request::BitmapDataRef::Png(_)
                                | skrifa::request::BitmapDataRef::Bgra(_)
                        )
                    });
                (colr, svg, bitmap)
            } else {
                let colr = font.color_glyphs().get(glyph_id.into()).is_some();
                let svg = if font.table_data(Tag::new(b"SVG ")).is_some() {
                    let Ok(table) = font.svg() else {
                        return Ok(None);
                    };
                    let Ok(data) = table.glyph_data(glyph_id.into()) else {
                        return Ok(None);
                    };
                    data.is_some()
                } else {
                    false
                };
                let bitmap = font
                    .bitmap_strikes()
                    .glyph_for_size(skrifa::prelude::Size::unscaled(), glyph_id.into())
                    .is_some_and(|image| {
                        matches!(image.data, BitmapData::Png(_) | BitmapData::Bgra(_))
                    });
                (colr, svg, bitmap)
            };
            Ok(Some(colr || svg || bitmap))
        })
    }
    #[inline(never)]
    fn outline(
        &self,
        id: ID,
        glyph_id: GlyphId,
        variations: &[crate::FontVariation],
        observer: Option<&dyn crate::FontReplay>,
        processing: &crate::ProcessingContext,
    ) -> Option<tiny_skia_path::Path> {
        match self.owned_outline(id, glyph_id, variations, observer, processing) {
            Ok(OwnedOutline::NonEmpty(path)) => Some(path),
            Ok(OwnedOutline::Empty | OwnedOutline::Unknown) => None,
            Err(error) => {
                processing.record_resource(error);
                None
            }
        }
    }

    #[inline(never)]
    fn owned_outline(
        &self,
        id: ID,
        glyph_id: GlyphId,
        variations: &[crate::FontVariation],
        observer: Option<&dyn crate::FontReplay>,
        processing: &crate::ProcessingContext,
    ) -> Result<OwnedOutline, markdown_svg_budget::Error> {
        if let Some(error) = processing.resource_failure() {
            return Err(error);
        }
        let candidate = self
            .with_face_data(id, |data, face_index| -> Option<OwnedOutline> {
                let instance = super::font_instance::FontInstance::new(
                    data,
                    face_index,
                    variations,
                    processing.resources(),
                )
                .map_err(|e| processing.record_resource(e))
                .ok()??;
                let mut builder = PathBuilder::with_processing(processing);
                let draw = instance
                    .draw(glyph_id.into(), &mut builder)
                    .map_err(|e| processing.record_resource(e))
                    .ok()??;
                if let Some(error) = builder.failure {
                    processing.record_resource(error);
                    return None;
                }
                draw.ok()?;

                let commands = builder.commands;
                let path = match builder.builder.try_finish() {
                    Ok(path) => path,
                    Err(error) => {
                        processing.record_resource(error);
                        return None;
                    }
                };
                if let Some(observer) = observer {
                    if observer
                        .observe_outline(crate::ReplayOutlineObservation {
                            font: id,
                            glyph: glyph_id.0,
                            face_index,
                            units_per_em: instance
                                .units_per_em()
                                .map_err(|e| processing.record_resource(e))
                                .ok()??,
                            variations,
                            location: instance.location(),
                            axes: instance.axes(),
                            path: path.as_ref(),
                        })
                        .is_err()
                    {
                        processing.fail(ProcessingFailure::FontReplay);
                        return None;
                    }
                }
                Some(match path {
                    Some(path) => OwnedOutline::NonEmpty(path),
                    None if commands == 0 => OwnedOutline::Empty,
                    None => OwnedOutline::Unknown,
                })
            })
            .flatten()
            .unwrap_or(OwnedOutline::Unknown);
        // Face-data, location and pen scopes have exited. In particular, an
        // empty/invalid result cannot hide the last geometry owner's failure.
        if let Some(resources) = processing.resources() {
            resources.checkpoint(0)?;
        }
        match processing.resource_failure() {
            Some(error) => Err(error),
            None => Ok(candidate),
        }
    }

    fn has_opsz_axis(&self, id: ID, processing: &crate::ProcessingContext) -> bool {
        self.with_face_data(id, |data, face_index| -> Option<bool> {
            if let Some(resources) = processing.resources() {
                let font = skrifa::FontRef::try_from_index(
                    data,
                    face_index,
                    &mut markdown_svg_budget::RequestControl(resources),
                )
                .map_err(|e| processing.record_resource(e))
                .ok()?
                .ok()?;
                return Some(
                    skrifa::request::Axes::try_new(&font, resources)
                        .and_then(|axes| axes.try_get_by_tag(OPSZ))
                        .map_err(|e| processing.record_resource(e))
                        .ok()?
                        .is_some(),
                );
            }
            let font = skrifa::FontRef::from_index(data, face_index).ok()?;
            Some(font.axes().get_by_tag(OPSZ).is_some())
        })
        .flatten()
        .unwrap_or(false)
    }

    fn raster(
        &self,
        id: ID,
        glyph_id: GlyphId,
        processing: &crate::ProcessingContext,
    ) -> Result<Option<BitmapImage>, markdown_svg_budget::Error> {
        super::font_instance::with_source(self, id, processing, false, |data, face_index| {
            let Some(font) = super::font_instance::read_font(data, face_index, processing)? else {
                return Ok(None);
            };
            // Preserve the original largest-image selection and default coords.
            let size = skrifa::prelude::Size::unscaled();
            let location = LocationRef::default();
            let (png, width, height, x, y, ppem, is_sbix) = match processing.resources() {
                Some(resources) => {
                    let strikes = skrifa::request::BitmapStrikes::try_new(&font, resources)?;
                    let Some(image) = strikes.try_glyph_for_size(size, glyph_id.into())? else {
                        return Ok(None);
                    };
                    let skrifa::request::BitmapDataRef::Png(png) = image.data() else {
                        return Ok(None);
                    };
                    let m = image.metadata();
                    (
                        png,
                        m.width,
                        m.height,
                        m.inner_bearing_x,
                        m.inner_bearing_y,
                        m.ppem_x,
                        strikes.format() == Some(BitmapFormat::Sbix),
                    )
                }
                None => {
                    let strikes = font.bitmap_strikes();
                    let Some(image) = strikes.glyph_for_size(size, glyph_id.into()) else {
                        return Ok(None);
                    };
                    let BitmapData::Png(png) = image.data else {
                        return Ok(None);
                    };
                    (
                        png,
                        image.width,
                        image.height,
                        image.inner_bearing_x,
                        image.inner_bearing_y,
                        image.ppem_x,
                        strikes.format() == Some(BitmapFormat::Sbix),
                    )
                }
            };
            let encoded = match processing.resources() {
                Some(resources) => crate::EncodedImageData::try_copy_from_slice(resources, png)?,
                None => Arc::new(png.to_vec()).into(),
            };
            let bounds = match processing.resources() {
                Some(resources) => {
                    skrifa::request::GlyphMetrics::try_new(&font, size, location, resources)?
                        .try_bounds(glyph_id.into())?
                }
                None => font.glyph_metrics(size, location).bounds(glyph_id.into()),
            };
            owner::check(processing, 0)?;
            let bounding_box = bounds.map(|bbox| BoundingBox {
                x_min: bbox.x_min as i16,
                y_min: bbox.y_min as i16,
                x_max: bbox.x_max as i16,
                y_max: bbox.y_max as i16,
            });
            let Some(size) = Size::from_wh(width as f32, height as f32) else {
                return Ok(None);
            };
            let Some(abs_bounding_box) =
                NonZeroRect::from_xywh(0.0, 0.0, width as f32, height as f32)
            else {
                return Ok(None);
            };
            Ok(Some(BitmapImage {
                image: Image {
                    origin: Origin::new(processing),
                    id: TreeString::new(processing),
                    visible: true,
                    size,
                    rendering_mode: ImageRendering::OptimizeQuality,
                    kind: ImageKind::PNG(encoded),
                    abs_transform: Transform::default(),
                    abs_bounding_box,
                },
                x: x as i16,
                y: y as i16,
                pixels_per_em: ppem as u16,
                glyph_bbox: bounding_box,
                is_sbix,
            }))
        })
    }

    fn svg(
        &self,
        id: ID,
        glyph_id: GlyphId,
        current_color: svgtypes::Color,
        options: &Options,
    ) -> Result<Option<Node>, markdown_svg_budget::Error> {
        let p = &options.processing;
        super::font_instance::with_source(
            self,
            id,
            p,
            options.font_replay.is_some(),
            |data, face_index| {
                let Some(font) = super::font_instance::read_font(data, face_index, p)? else {
                    return Ok(None);
                };
                let (image_data, units_per_em) = match p.resources() {
                    Some(r) => {
                        let mut cx = markdown_svg_budget::RequestControl(r);
                        let Ok(svg) = super::font_instance::read_table::<
                            skrifa::raw::tables::svg::Svg,
                        >(&font, r)?
                        else {
                            return Ok(None);
                        };
                        let Ok(Some(bytes)) = svg.try_glyph_data(glyph_id.into(), &mut cx)? else {
                            return Ok(None);
                        };
                        let Ok(head) = super::font_instance::read_table::<
                            skrifa::raw::tables::head::Head,
                        >(&font, r)?
                        else {
                            return Ok(None);
                        };
                        (bytes, head.units_per_em())
                    }
                    None => {
                        let Ok(svg) = font.svg() else { return Ok(None) };
                        let Ok(Some(bytes)) = svg.glyph_data(glyph_id.into()) else {
                            return Ok(None);
                        };
                        let Ok(head) = font.head() else {
                            return Ok(None);
                        };
                        (bytes, head.units_per_em())
                    }
                };
                let mut nested = Options::font_source_options(
                    options.processing.clone(),
                    options.fontdb.clone(),
                    options.font_replay.clone(),
                )?;
                nested.source_kind = crate::parser::source_kind::SourceKind::FontGlyph {
                    glyph: glyph_id.0,
                    units_per_em,
                    current_color,
                };
                let candidate = (|| {
                    let Some(size) = Size::from_wh(units_per_em as f32, units_per_em as f32) else {
                        return Ok(None);
                    };
                    nested.default_size = size;
                    match Tree::from_data_in_request(image_data, &nested) {
                        Ok(tree) => tree
                            .into_root_in(p)
                            .and_then(|root| crate::scene::moved_root(p, root))
                            .map(|group| Some(Node::Group(group))),
                        Err(_) => Err(p.record_resource(markdown_svg_budget::Error::new(
                            ProcessingFailure::FontReplay.into(),
                            markdown_svg_budget::LimitKind::None,
                        ))),
                    }
                })()
                .map_err(|error| p.record_resource(error));
                drop(nested);
                owner::finish(p, candidate)
            },
        )
    }

    fn colr(
        &self,
        id: ID,
        glyph_id: GlyphId,
        variations: &[crate::FontVariation],
        foreground_color: svgtypes::Color,
        options: &Options,
    ) -> Result<Option<Tree>, markdown_svg_budget::Error> {
        let p = &options.processing;
        owner::check(p, 0)?;
        super::font_instance::with_source(
            self,
            id,
            p,
            options.font_replay.is_some(),
            |data, face_index| -> Result<Option<Tree>, markdown_svg_budget::Error> {
                let Some(instance) = super::font_instance::FontInstance::new(
                    data,
                    face_index,
                    variations,
                    p.resources(),
                )?
                else {
                    return Ok(None);
                };
                let font = instance.font();
                let location = instance.validated_location()?;
                let mut svg = ColrWriter::new(xmlwriter::Options::default(), p)?;
                let mut path_buf = ColrPath::new(p)?;
                let candidate = (|| -> Result<Option<Tree>, markdown_svg_budget::Error> {
                    svg.start_element("svg")?;
                    svg.write_attribute("xmlns", "http://www.w3.org/2000/svg")?;
                    svg.write_attribute("xmlns:xlink", "http://www.w3.org/1999/xlink")?;
                    svg.start_element("g")?;
                    enum ColorGlyph<'f, 'r> {
                        Ordinary(skrifa::color::ColorGlyph<'f>),
                        Request(
                            skrifa::request::ColorGlyph<'f, 'r>,
                            &'r markdown_svg_budget::ResourcesRef,
                        ),
                    }
                    let glyph = match p.resources() {
                        Some(r) => skrifa::request::ColorGlyphs::try_new(font, r)?
                            .try_get(glyph_id.into())?
                            .map(|glyph| ColorGlyph::Request(glyph, r)),
                        None => font
                            .color_glyphs()
                            .get(glyph_id.into())
                            .map(ColorGlyph::Ordinary),
                    };
                    let Some(glyph) = glyph else {
                        return Ok(None);
                    };
                    let transforms_stack = TreeVec::try_from_iter(
                        std::iter::once(skrifa::color::Transform::default()),
                        p,
                    )?;
                    let mut painter = GlyphPainter {
                        font,
                        location,
                        svg: &mut svg,
                        path_buf: &mut path_buf,
                        gradient_index: 1,
                        clip_path_index: 1,
                        foreground_color,
                        transform: skrifa::color::Transform::default(),
                        outline_transform: skrifa::color::Transform::default(),
                        transforms_stack,
                        clip_stack: TreeVec::new(p),
                        aborted: false,
                    };
                    let painted = match glyph {
                        ColorGlyph::Ordinary(glyph) => Ok(glyph.paint(location, &mut painter)),
                        ColorGlyph::Request(glyph, resources) => {
                            let mut request_painter = painter.request(resources)?;
                            glyph.paint(location, &mut request_painter)
                        }
                    }
                    .map_err(|error| p.record_resource(error));
                    let aborted = painter.aborted;
                    // Temporary stacks retire before inspecting a font-domain result
                    // or writing the next XML element/candidate.
                    drop(painter);
                    owner::check(p, 0)?;
                    if painted?.is_err() || aborted {
                        return Ok(None);
                    }
                    svg.end_element()?;
                    let mut nested = Options::font_source_options(
                        options.processing.clone(),
                        options.fontdb.clone(),
                        options.font_replay.clone(),
                    )?;
                    nested.source_kind =
                        crate::parser::source_kind::SourceKind::GeneratedColorGlyph;
                    let xml = svg.finish()?;
                    let candidate = Tree::from_data_in_request(xml, &nested);
                    drop(nested);
                    owner::check(p, 0)?;
                    match candidate {
                        Ok(tree) => Ok(Some(tree)),
                        Err(_) => match p.resource_failure() {
                            Some(e) => Err(e),
                            None => Ok(None),
                        },
                    }
                })()
                .map_err(|e| p.record_resource(e));
                drop(path_buf);
                drop(svg);
                drop(instance);
                owner::check(p, 0)?;
                candidate
            },
        )
    }
}

#[cfg(test)]
#[path = "geometry_flatten_tests.rs"]
mod geometry_flatten_tests;

#[cfg(test)]
#[path = "scene_text_tests.rs"]
mod scene_text_tests;

#[cfg(test)]
#[path = "font_representation_tests.rs"]
mod font_representation_tests;

impl PolicyValue for BitmapImage {
    fn validate_policy(&self, p: &ProcessingContext) -> Result<(), markdown_svg_budget::Error> {
        self.image.validate_policy(p)
    }
}
impl TryCopy for BitmapImage {
    fn try_copy_in(&self, p: &ProcessingContext) -> Result<Self, markdown_svg_budget::Error> {
        self.validate_policy(p)?;
        let value = Self {
            image: self.image.try_copy_in(p)?,
            x: self.x,
            y: self.y,
            pixels_per_em: self.pixels_per_em,
            glyph_bbox: self.glyph_bbox,
            is_sbix: self.is_sbix,
        };
        owner::finish(p, Ok(value))
    }
}
