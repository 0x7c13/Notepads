// Copyright 2018 the Resvg Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT
// Modifications Copyright (c) 2026, Jiaqi (0x7c13) Liu.
// Experimental request-owned error propagation; original license terms retained.

use crate::raster::Raster;

pub struct Context<'a> {
    pub max_bbox: tiny_skia::IntRect,
    pub processing: std::sync::Arc<usvg::ProcessingContext>,
    pub coverage: Option<&'a crate::CoverageConfig<'a>>,
}

impl Context<'_> {
    pub(crate) fn geometry_checkpoint(&self) -> Result<(), markdown_svg_budget::Error> {
        if let Some(error) = self.processing.resource_failure() {
            return Err(self.processing.record_resource(error));
        }
        if let Some(resources) = self.processing.resources() {
            resources
                .checkpoint(0)
                .map_err(|error| self.processing.record_resource(error))?;
        }
        Ok(())
    }

    // All temporary owners created by this closure have exited before its
    // successful value can pass the same first-failure slot to another consumer.
    pub(crate) fn with_geometry<T>(&self, make: impl FnOnce() -> Option<T>) -> Option<T> {
        self.geometry_checkpoint().ok()?;
        let candidate = make();
        self.geometry_checkpoint().ok()?;
        candidate
    }
}

pub fn render_nodes(
    parent: &usvg::Group,
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
                        if !ctx.processing.enter_render() {
                            return None;
                        }
                        let usvg::Node::Path(path) = node else {
                            unreachable!()
                        };
                        let coverage = batch
                            .as_ref()
                            .map_or(crate::coverage::FillCoverage::Original, |batch| {
                                batch.for_path(path, ctx)
                            });
                        crate::path::render_prepared(
                            path,
                            tiny_skia::BlendMode::SourceOver,
                            ctx,
                            transform,
                            pixmap,
                            coverage,
                        );
                        if ctx.processing.failed() {
                            return None;
                        }
                    }
                    // Drop all A8 before any group/image/pattern or recursive surface.
                    drop(batch);
                    ctx.geometry_checkpoint().ok()?;
                    index += count;
                    continue;
                }
            }
            render_node(&children[index], ctx, transform, pixmap);
            if ctx.processing.failed() {
                return None;
            }
            index += 1;
        }

        Some(())
    });
}

pub fn render_node(
    node: &usvg::Node,
    ctx: &Context,
    transform: tiny_skia::Transform,
    pixmap: &mut tiny_skia::PixmapMut,
) {
    let _ = ctx.with_geometry(|| {
        if !ctx.processing.enter_render() {
            return None;
        }
        match node {
            usvg::Node::Group(group) => {
                render_group(group, ctx, transform, pixmap);
            }
            usvg::Node::Path(path) => {
                crate::path::render(
                    path,
                    tiny_skia::BlendMode::SourceOver,
                    ctx,
                    transform,
                    pixmap,
                );
            }
            usvg::Node::Image(image) => {
                crate::image::render(image, ctx, transform, pixmap);
            }
            usvg::Node::Text(text) => {
                render_group(text.flattened(), ctx, transform, pixmap);
            }
        }

        Some(())
    });
}

fn render_group(
    group: &usvg::Group,
    ctx: &Context,
    transform: tiny_skia::Transform,
    pixmap: &mut tiny_skia::PixmapMut,
) -> Option<()> {
    ctx.with_geometry(|| {
        if ctx.processing.failed() {
            return None;
        }
        let transform = transform.pre_concat(group.transform());

        if !group.should_isolate() {
            render_nodes(group, ctx, transform, pixmap);
            if ctx.processing.failed() {
                return None;
            }
            return Some(());
        }

        let rest = if group.filters().is_empty() {
            tiny_skia::Transform::identity()
        } else {
            let sx = transform.sx.hypot(transform.ky);
            let sy = transform.kx.hypot(transform.sy);
            tiny_skia::Transform::from_row(
                transform.sx / sx,
                transform.ky / sx,
                transform.kx / sy,
                transform.sy / sy,
                0.0,
                0.0,
            )
        };
        let inv_rest = rest.invert()?;

        let max_bbox = ctx.max_bbox.to_rect().transform(inv_rest)?.round_out()?;
        let (layer, ibbox) = render_layer(
            group,
            &Context {
                max_bbox,
                processing: ctx.processing.clone(),
                coverage: ctx.coverage,
            },
            inv_rest.pre_concat(transform),
        )?;

        let paint = tiny_skia::Paint {
            shader: tiny_skia::Pattern::new(
                layer.as_ref(),
                tiny_skia::SpreadMode::Pad,
                tiny_skia::FilterQuality::Bilinear,
                group.opacity().get(),
                tiny_skia::Transform::from_translate(ibbox.x() as f32, ibbox.y() as f32),
            ),
            blend_mode: convert_blend_mode(group.blend_mode()),
            anti_alias: true,
            ..tiny_skia::Paint::default()
        };
        let path = match ctx.processing.resources() {
            Some(resources) => tiny_skia::PathBuilder::try_from_rect(
                ibbox.to_rect(),
                resources,
                markdown_svg_budget::ResourceKind::Scratch,
            )
            .map_err(|error| ctx.processing.record_resource(error))
            .ok()?,
            None => tiny_skia::PathBuilder::from_rect(ibbox.to_rect()),
        };
        match ctx.processing.resources() {
            Some(resources) => pixmap
                .try_fill_path(
                    &path,
                    &paint,
                    tiny_skia::FillRule::Winding,
                    rest,
                    None,
                    resources,
                )
                .map_err(|error| ctx.processing.record_resource(error))
                .ok()?,
            None => pixmap.fill_path(&path, &paint, tiny_skia::FillRule::Winding, rest, None),
        }

        Some(())
    })
}

/// Renders an isolated group into a new layer, returning it and its position.
fn render_layer(
    group: &usvg::Group,
    ctx: &Context,
    transform: tiny_skia::Transform,
) -> Option<(Raster, tiny_skia::IntRect)> {
    ctx.with_geometry(|| {
        let bbox = group.layer_bounding_box().transform(transform)?;

        let mut ibbox = if group.filters().is_empty() {
            // Convert group bbox into an integer one, expanding each side outwards by 2px
            // to make sure that anti-aliased pixels would not be clipped.
            tiny_skia::IntRect::from_xywh(
                (bbox.x().floor() as i32).checked_sub(2)?,
                (bbox.y().floor() as i32).checked_sub(2)?,
                (bbox.width().ceil() as u32).checked_add(4)?,
                (bbox.height().ceil() as u32).checked_add(4)?,
            )?
        } else {
            // The bounding box for groups with filters is special and should not be expanded by 2px,
            // because it's already acting as a clipping region.
            let bbox = tiny_skia::IntRect::from_xywh(
                bbox.x().floor() as i32,
                bbox.y().floor() as i32,
                bbox.width().ceil().max(1.0) as u32,
                bbox.height().ceil().max(1.0) as u32,
            )?;
            // Make sure our filter region is not bigger than 4x the canvas size.
            // This is required mainly to prevent huge filter regions that would tank the performance.
            // It should not affect the final result in any way.
            crate::geom::fit_to_rect(bbox, ctx.max_bbox)?
        };

        // Make sure our layer is not bigger than 4x the canvas size.
        // This is required to prevent huge layers.
        if group.filters().is_empty() {
            ibbox = crate::geom::fit_to_rect(ibbox, ctx.max_bbox)?;
        }

        let shift_ts = {
            // Original shift.
            let mut dx = bbox.x();
            let mut dy = bbox.y();

            // Account for subpixel positioned layers.
            dx -= bbox.x() - ibbox.x() as f32;
            dy -= bbox.y() - ibbox.y() as f32;

            tiny_skia::Transform::from_translate(-dx, -dy)
        };

        let transform = shift_ts.pre_concat(transform);

        let mut sub_pixmap =
            Raster::try_create(ibbox.width(), ibbox.height(), ctx.processing.resources())
                .map_err(|e| ctx.processing.record_resource(e))
                .ok()?;

        render_nodes(group, ctx, transform, &mut sub_pixmap.as_mut());
        if ctx.processing.failed() {
            return None;
        }

        if !group.filters().is_empty() {
            for filter in group.filters() {
                crate::filter::apply(filter, ctx, transform, &mut sub_pixmap);
                if ctx.processing.failed() {
                    return None;
                }
            }
        }

        if let Some(clip_path) = group.clip_path() {
            crate::clip::apply(clip_path, ctx, transform, &mut sub_pixmap);
            if ctx.processing.failed() {
                return None;
            }
        }

        if let Some(mask) = group.mask() {
            crate::mask::apply(mask, ctx, transform, &mut sub_pixmap);
            if ctx.processing.failed() {
                return None;
            }
        }

        Some((sub_pixmap, ibbox))
    })
}

pub fn convert_blend_mode(mode: usvg::BlendMode) -> tiny_skia::BlendMode {
    match mode {
        usvg::BlendMode::Normal => tiny_skia::BlendMode::SourceOver,
        usvg::BlendMode::Multiply => tiny_skia::BlendMode::Multiply,
        usvg::BlendMode::Screen => tiny_skia::BlendMode::Screen,
        usvg::BlendMode::Overlay => tiny_skia::BlendMode::Overlay,
        usvg::BlendMode::Darken => tiny_skia::BlendMode::Darken,
        usvg::BlendMode::Lighten => tiny_skia::BlendMode::Lighten,
        usvg::BlendMode::ColorDodge => tiny_skia::BlendMode::ColorDodge,
        usvg::BlendMode::ColorBurn => tiny_skia::BlendMode::ColorBurn,
        usvg::BlendMode::HardLight => tiny_skia::BlendMode::HardLight,
        usvg::BlendMode::SoftLight => tiny_skia::BlendMode::SoftLight,
        usvg::BlendMode::Difference => tiny_skia::BlendMode::Difference,
        usvg::BlendMode::Exclusion => tiny_skia::BlendMode::Exclusion,
        usvg::BlendMode::Hue => tiny_skia::BlendMode::Hue,
        usvg::BlendMode::Saturation => tiny_skia::BlendMode::Saturation,
        usvg::BlendMode::Color => tiny_skia::BlendMode::Color,
        usvg::BlendMode::Luminosity => tiny_skia::BlendMode::Luminosity,
        usvg::BlendMode::PlusLighter => tiny_skia::BlendMode::Plus,
    }
}

#[cfg(test)]
mod tests {

    // Derived from https://github.com/servo/servo/issues/42258.
    #[test]
    fn filter_bbox_outside_int_rect() {
        let svg = r#"<svg filter="url(#f)"><filter id="f" x="2em"><feFlood/></filter><path d="M0 0H1e8V1"/></svg>"#;
        let tree = usvg::Tree::from_str(svg, &usvg::Options::default()).unwrap();
        let mut pixmap = tiny_skia::Pixmap::new(1, 1).unwrap();

        // Just make sure we don't panic.
        let _ = crate::render(
            &tree,
            tiny_skia::Transform::identity(),
            &mut pixmap.as_mut(),
        );
    }
}
