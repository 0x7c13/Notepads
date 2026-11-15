// Copyright 2019 the Resvg Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT
// Modifications Copyright (c) 2026, Jiaqi (0x7c13) Liu.
// Experimental request-owned error propagation; original license terms retained.

use crate::raster::{Raster, RasterMask};
use crate::render::Context;

pub fn apply(
    mask: &usvg::Mask,
    ctx: &Context,
    transform: tiny_skia::Transform,
    pixmap: &mut Raster,
) {
    let _ = ctx.with_geometry(|| {
        if ctx.processing.failed() {
            return None;
        }
        if mask.root().children().is_empty() {
            pixmap.fill(tiny_skia::Color::TRANSPARENT);
            return None;
        }

        let mut mask_pixmap =
            match Raster::try_create(pixmap.width(), pixmap.height(), ctx.processing.resources()) {
                Ok(value) => value,
                Err(e) => {
                    ctx.processing.record_resource(e);
                    return None;
                }
            };

        {
            // TODO: only when needed
            // Mask has to be clipped by mask.region
            let mut alpha_mask = match RasterMask::try_new(
                pixmap.width(),
                pixmap.height(),
                ctx.processing.resources(),
            ) {
                Ok(value) => value,
                Err(e) => {
                    ctx.processing.record_resource(e);
                    return None;
                }
            };
            {
                let path = match ctx.processing.resources() {
                    Some(resources) => tiny_skia::PathBuilder::try_from_rect(
                        mask.rect().to_rect(),
                        resources,
                        markdown_svg_budget::ResourceKind::Scratch,
                    )
                    .map_err(|error| ctx.processing.record_resource(error))
                    .ok()?,
                    None => tiny_skia::PathBuilder::from_rect(mask.rect().to_rect()),
                };
                match ctx.processing.resources() {
                    Some(resources) => alpha_mask
                        .as_mut()
                        .try_fill_path(
                            &path,
                            tiny_skia::FillRule::Winding,
                            true,
                            transform,
                            resources,
                        )
                        .map_err(|error| ctx.processing.record_resource(error))
                        .ok()?,
                    None => alpha_mask.as_mut().fill_path(
                        &path,
                        tiny_skia::FillRule::Winding,
                        true,
                        transform,
                    ),
                }
            }
            ctx.geometry_checkpoint().ok()?;

            crate::render::render_nodes(mask.root(), ctx, transform, &mut mask_pixmap.as_mut());
            if ctx.processing.failed() {
                return None;
            }

            mask_pixmap.apply_mask(alpha_mask.as_ref());
        }
        ctx.geometry_checkpoint().ok()?;

        if let Some(mask) = mask.mask() {
            self::apply(mask, ctx, transform, pixmap);
            if ctx.processing.failed() {
                return None;
            }
        }

        let mask_type = match mask.kind() {
            usvg::MaskType::Luminance => tiny_skia::MaskType::Luminance,
            usvg::MaskType::Alpha => tiny_skia::MaskType::Alpha,
        };

        let mask = match RasterMask::try_from_pixmap(&mask_pixmap, mask_type) {
            Ok(value) => value,
            Err(e) => {
                ctx.processing.record_resource(e);
                return None;
            }
        };
        pixmap.apply_mask(mask.as_ref());

        Some(())
    });
}
