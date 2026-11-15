// Copyright 2019 the Resvg Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT
// Modifications Copyright (c) 2026, Jiaqi (0x7c13) Liu.
// Experimental request-owned error propagation; original license terms retained.

use crate::raster::{Raster, RasterMask};
use crate::render::Context;

pub fn apply(
    clip: &usvg::ClipPath,
    ctx: &Context,
    transform: tiny_skia::Transform,
    pixmap: &mut Raster,
) {
    let _ = ctx.with_geometry(|| {
        if ctx.processing.failed() {
            return None;
        }
        let mut clip_pixmap =
            match Raster::try_create(pixmap.width(), pixmap.height(), ctx.processing.resources()) {
                Ok(value) => value,
                Err(e) => {
                    ctx.processing.record_resource(e);
                    return None;
                }
            };
        clip_pixmap.fill(tiny_skia::Color::BLACK);

        draw_children(
            clip.root(),
            tiny_skia::BlendMode::Clear,
            ctx,
            transform.pre_concat(clip.transform()),
            &mut clip_pixmap.as_mut(),
        );
        if ctx.processing.failed() {
            return None;
        }

        if let Some(clip) = clip.clip_path() {
            apply(clip, ctx, transform, pixmap);
            if ctx.processing.failed() {
                return None;
            }
        }

        let mut mask = match RasterMask::try_from_pixmap(&clip_pixmap, tiny_skia::MaskType::Alpha) {
            Ok(value) => value,
            Err(e) => {
                ctx.processing.record_resource(e);
                return None;
            }
        };
        if let Err(e) = mask.invert() {
            ctx.processing.record_resource(e);
            return None;
        }
        pixmap.apply_mask(mask.as_ref());

        Some(())
    });
}

fn draw_children(
    parent: &usvg::Group,
    mode: tiny_skia::BlendMode,
    ctx: &Context,
    transform: tiny_skia::Transform,
    pixmap: &mut tiny_skia::PixmapMut,
) {
    let _ = ctx.with_geometry(|| {
        let children = parent.children();
        let mut index = 0;
        while index < children.len() {
            if ctx.processing.failed() {
                return None;
            }
            if ctx.coverage.is_some() {
                let count = crate::coverage::direct_span(
                    &children[index..],
                    pixmap.width(),
                    pixmap.height(),
                );
                if count != 0 {
                    let nodes = &children[index..index + count];
                    let paths = nodes.iter().filter_map(|node| match node {
                        usvg::Node::Path(path) => Some(path.as_ref()),
                        _ => None,
                    });
                    let batch = match crate::coverage::PreparedBatch::prepare(
                        paths,
                        ctx,
                        transform,
                        pixmap.width(),
                        pixmap.height(),
                    ) {
                        Ok(batch) => batch,
                        Err(error) => {
                            ctx.processing.record_resource(error);
                            return None;
                        }
                    };
                    for node in nodes {
                        let usvg::Node::Path(path) = node else {
                            unreachable!()
                        };
                        if !path.is_visible() {
                            continue;
                        }
                        let coverage = batch
                            .as_ref()
                            .map_or(crate::coverage::FillCoverage::Original, |batch| {
                                batch.for_path(path, ctx)
                            });
                        crate::path::fill_path_prepared(
                            path, mode, ctx, transform, pixmap, coverage,
                        );
                        if ctx.processing.failed() {
                            return None;
                        }
                    }
                    drop(batch);
                    ctx.geometry_checkpoint().ok()?;
                    index += count;
                    continue;
                }
            }
            match &children[index] {
                usvg::Node::Path(path) => {
                    if !path.is_visible() {
                        index += 1;
                        continue;
                    }

                    crate::path::fill_path(path, mode, ctx, transform, pixmap);
                }
                usvg::Node::Text(text) => {
                    draw_children(text.flattened(), mode, ctx, transform, pixmap);
                }
                usvg::Node::Group(group) => {
                    let transform = transform.pre_concat(group.transform());

                    if let Some(clip) = group.clip_path() {
                        // If a `clipPath` child also has a `clip-path`
                        // then we should render this child on a new canvas,
                        // clip it, and only then draw it to the `clipPath`.
                        clip_group(group, clip, ctx, transform, pixmap);
                    } else {
                        draw_children(group, mode, ctx, transform, pixmap);
                    }
                }
                _ => {}
            }
            index += 1;
        }

        Some(())
    });
}

fn clip_group(
    children: &usvg::Group,
    clip: &usvg::ClipPath,
    ctx: &Context,
    transform: tiny_skia::Transform,
    pixmap: &mut tiny_skia::PixmapMut,
) -> Option<()> {
    ctx.with_geometry(|| {
        let mut clip_pixmap =
            Raster::try_create(pixmap.width(), pixmap.height(), ctx.processing.resources())
                .map_err(|e| ctx.processing.record_resource(e))
                .ok()?;

        draw_children(
            children,
            tiny_skia::BlendMode::SourceOver,
            ctx,
            transform,
            &mut clip_pixmap.as_mut(),
        );
        if ctx.processing.failed() {
            return None;
        }
        apply(clip, ctx, transform, &mut clip_pixmap);
        if ctx.processing.failed() {
            return None;
        }

        let mut paint = tiny_skia::PixmapPaint::default();
        paint.blend_mode = tiny_skia::BlendMode::Xor;
        match ctx.processing.resources() {
            Some(resources) => pixmap
                .try_draw_pixmap(
                    0,
                    0,
                    clip_pixmap.as_ref(),
                    &paint,
                    tiny_skia::Transform::identity(),
                    None,
                    resources,
                )
                .map_err(|error| ctx.processing.record_resource(error))
                .ok()?,
            None => pixmap.draw_pixmap(
                0,
                0,
                clip_pixmap.as_ref(),
                &paint,
                tiny_skia::Transform::identity(),
                None,
            ),
        }

        Some(())
    })
}
