// Copyright 2016 Google Inc.
// Copyright 2020 Yevhenii Reizner
// Modifications Copyright (c) 2026, Jiaqi (0x7c13) Liu.
//
// Use of this source code is governed by a BSD-style license that can be
// found in the LICENSE file.

use crate::{BlendMode, Color, LengthU32, PixmapRef, PremultipliedColorU8};
use crate::{ALPHA_U8_OPAQUE, ALPHA_U8_TRANSPARENT};

use crate::alpha_runs::AlphaRun;
use crate::blitter::{Blitter, Mask};
use crate::color::AlphaU8;
use crate::geom::ScreenIntRect;
use crate::mask::SubMaskRef;
use crate::math::LENGTH_U32_ONE;
use crate::painter::GeometryControl;
use crate::pipeline::{self, RasterPipeline, RasterPipelineBuilder};
use crate::pixmap::SubPixmapMut;
use crate::shaders::PaintView;
use markdown_svg_budget::Error;

pub struct RasterPipelineBlitter<'a, 'b: 'a, 'r> {
    control: GeometryControl<'r>,
    mask: Option<SubMaskRef<'a>>,
    pixmap_src: PixmapRef<'a>,
    pixmap: &'a mut SubPixmapMut<'b>,
    memset2d_color: Option<PremultipliedColorU8>,
    blit_anti_h_rp: RasterPipeline<'r>,
    blit_rect_rp: RasterPipeline<'r>,
    blit_mask_rp: RasterPipeline<'r>,
    is_mask: bool,
}

impl<'a, 'b: 'a, 'r> RasterPipelineBlitter<'a, 'b, 'r> {
    pub fn new_with_view(
        paint: PaintView<'_, 'a, '_>,
        mask: Option<SubMaskRef<'a>>,
        pixmap: &'a mut SubPixmapMut<'b>,
        control: GeometryControl<'r>,
    ) -> Result<Option<Self>, Error> {
        paint.validate(control)?;
        let candidate = (|pixmap: &'a mut SubPixmapMut<'b>| {
            // Make sure that `mask` has the same size as `pixmap`.
            if let Some(mask) = mask {
                if mask.size.width() != pixmap.size.width()
                    || mask.size.height() != pixmap.size.height()
                {
                    log::warn!("Pixmap and Mask are expected to have the same size");
                    return Ok(None);
                }
            }

            // Fast-reject.
            // This is basically SkInterpretXfermode().
            match paint.options.blend_mode {
                // `Destination` keep the pixmap unchanged. Nothing to do here.
                BlendMode::Destination => return Ok(None),
                BlendMode::DestinationIn if paint.is_opaque(control)? && paint.is_solid_color() => {
                    return Ok(None)
                }
                _ => {}
            }

            // We can strength-reduce SourceOver into Source when opaque.
            let mut blend_mode = paint.options.blend_mode;
            if paint.is_opaque(control)? && blend_mode == BlendMode::SourceOver && mask.is_none() {
                blend_mode = BlendMode::Source;
            }

            // When we're drawing a constant color in Source mode, we can sometimes just memset.
            let mut memset2d_color = None;
            if paint.is_solid_color() && blend_mode == BlendMode::Source && mask.is_none() {
                // Unlike Skia, our shader cannot be constant.
                // Therefore there is no need to run a raster pipeline to get shader's color.
                if let Some(color) = paint.solid_color(control)? {
                    memset2d_color = Some(color.premultiply().to_color_u8());
                }
            };

            // Clear is just a transparent color memset.
            if blend_mode == BlendMode::Clear && !paint.options.anti_alias && mask.is_none() {
                blend_mode = BlendMode::Source;
                memset2d_color = Some(PremultipliedColorU8::TRANSPARENT);
            }

            let blit_anti_h_rp = {
                let mut p = RasterPipelineBuilder::new_with_control(control)?;
                p.set_force_hq_pipeline(paint.options.force_hq_pipeline);
                if !paint.push_stages(&mut p)? {
                    return Ok(None);
                }

                if mask.is_some() {
                    p.push(pipeline::Stage::MaskU8);
                }

                if blend_mode.should_pre_scale_coverage() {
                    p.push(pipeline::Stage::Scale1Float);
                    p.push(pipeline::Stage::LoadDestination);
                    if let Some(stage) = paint.options.colorspace.expand_dest_stage() {
                        p.push(stage);
                    }
                    if let Some(blend_stage) = blend_mode.to_stage() {
                        p.push(blend_stage);
                    }
                } else {
                    p.push(pipeline::Stage::LoadDestination);
                    if let Some(stage) = paint.options.colorspace.expand_dest_stage() {
                        p.push(stage);
                    }
                    if let Some(blend_stage) = blend_mode.to_stage() {
                        p.push(blend_stage);
                    }

                    p.push(pipeline::Stage::Lerp1Float);
                }

                if let Some(stage) = paint.options.colorspace.compress_stage() {
                    p.push(stage);
                }
                p.push(pipeline::Stage::Store);

                p.compile()?
            };

            let blit_rect_rp = {
                let mut p = RasterPipelineBuilder::new_with_control(control)?;
                p.set_force_hq_pipeline(paint.options.force_hq_pipeline);
                if !paint.push_stages(&mut p)? {
                    return Ok(None);
                }

                if mask.is_some() {
                    p.push(pipeline::Stage::MaskU8);
                }

                if blend_mode == BlendMode::SourceOver && mask.is_none() {
                    if let Some(stage) = paint.options.colorspace.compress_stage() {
                        p.push(stage);
                    }
                    // TODO: ignore when dither_rate is non-zero
                    p.push(pipeline::Stage::SourceOverRgba);
                } else {
                    if blend_mode != BlendMode::Source {
                        p.push(pipeline::Stage::LoadDestination);
                        if let Some(blend_stage) = blend_mode.to_stage() {
                            if let Some(stage) = paint.options.colorspace.expand_dest_stage() {
                                p.push(stage);
                            }
                            p.push(blend_stage);
                        }
                    }

                    if let Some(stage) = paint.options.colorspace.compress_stage() {
                        p.push(stage);
                    }
                    p.push(pipeline::Stage::Store);
                }

                p.compile()?
            };

            let blit_mask_rp = {
                let mut p = RasterPipelineBuilder::new_with_control(control)?;
                p.set_force_hq_pipeline(paint.options.force_hq_pipeline);
                if !paint.push_stages(&mut p)? {
                    return Ok(None);
                }

                if mask.is_some() {
                    p.push(pipeline::Stage::MaskU8);
                }

                if blend_mode.should_pre_scale_coverage() {
                    p.push(pipeline::Stage::ScaleU8);
                    p.push(pipeline::Stage::LoadDestination);
                    if let Some(stage) = paint.options.colorspace.expand_dest_stage() {
                        p.push(stage);
                    }
                    if let Some(blend_stage) = blend_mode.to_stage() {
                        p.push(blend_stage);
                    }
                } else {
                    p.push(pipeline::Stage::LoadDestination);
                    if let Some(stage) = paint.options.colorspace.expand_dest_stage() {
                        p.push(stage);
                    }
                    if let Some(blend_stage) = blend_mode.to_stage() {
                        p.push(blend_stage);
                    }

                    p.push(pipeline::Stage::LerpU8);
                }

                if let Some(stage) = paint.options.colorspace.compress_stage() {
                    p.push(stage);
                }
                p.push(pipeline::Stage::Store);

                p.compile()?
            };

            let pixmap_src = paint.pixmap();

            Ok(Some(RasterPipelineBlitter {
                control,
                mask,
                pixmap_src,
                pixmap,
                memset2d_color,
                blit_anti_h_rp,
                blit_rect_rp,
                blit_mask_rp,
                is_mask: false,
            }))
        })(pixmap);
        control.finish(candidate)
    }

    pub fn new_mask_with_control(
        pixmap: &'a mut SubPixmapMut<'b>,
        control: GeometryControl<'r>,
    ) -> Result<Option<Self>, Error> {
        control.peek_slot()?;
        let candidate = (|pixmap: &'a mut SubPixmapMut<'b>| {
            let color = Color::WHITE.premultiply();

            let memset2d_color = Some(color.to_color_u8());

            let blit_anti_h_rp = {
                let mut p = RasterPipelineBuilder::new_with_control(control)?;
                p.push_uniform_color(color);
                p.push(pipeline::Stage::LoadDestinationU8);
                p.push(pipeline::Stage::Lerp1Float);
                p.push(pipeline::Stage::StoreU8);
                p.compile()?
            };

            let blit_rect_rp = {
                let mut p = RasterPipelineBuilder::new_with_control(control)?;
                p.push_uniform_color(color);
                p.push(pipeline::Stage::StoreU8);
                p.compile()?
            };

            let blit_mask_rp = {
                let mut p = RasterPipelineBuilder::new_with_control(control)?;
                p.push_uniform_color(color);
                p.push(pipeline::Stage::LoadDestinationU8);
                p.push(pipeline::Stage::LerpU8);
                p.push(pipeline::Stage::StoreU8);
                p.compile()?
            };

            Ok(Some(RasterPipelineBlitter {
                control,
                mask: None,
                pixmap_src: PixmapRef::from_bytes(&[0, 0, 0, 0], 1, 1).unwrap(),
                pixmap,
                memset2d_color,
                blit_anti_h_rp,
                blit_rect_rp,
                blit_mask_rp,
                is_mask: true,
            }))
        })(pixmap);
        control.finish(candidate)
    }
}

impl Blitter for RasterPipelineBlitter<'_, '_, '_> {
    fn blit_h(&mut self, x: u32, y: u32, width: LengthU32) {
        if self.control.peek_slot().is_err() {
            return;
        }
        let r = ScreenIntRect::from_xywh_safe(x, y, width, LENGTH_U32_ONE);
        self.blit_rect(&r);
    }

    fn blit_anti_h(&mut self, mut x: u32, y: u32, aa: &mut [AlphaU8], runs: &mut [AlphaRun]) {
        if self.control.peek_slot().is_err() {
            return;
        }
        let mask_ctx = self.mask.map(|c| c.mask_ctx()).unwrap_or_default();

        let mut aa_offset = 0;
        let mut run_offset = 0;
        let mut run_opt = runs[0];
        while let Some(run) = run_opt {
            if self.control.check(1).is_err() {
                return;
            }
            let width = LengthU32::from(run);

            match aa[aa_offset] {
                ALPHA_U8_TRANSPARENT => {}
                ALPHA_U8_OPAQUE => {
                    self.blit_h(x, y, width);
                }
                alpha => {
                    let Ok(context) = self.blit_anti_h_rp.context_mut() else {
                        return;
                    };
                    context.current_coverage = alpha as f32 * (1.0 / 255.0);

                    let rect = ScreenIntRect::from_xywh_safe(x, y, width, LENGTH_U32_ONE);
                    let _ = self.blit_anti_h_rp.run(
                        &rect,
                        pipeline::AAMaskCtx::default(),
                        mask_ctx,
                        self.pixmap_src,
                        self.pixmap,
                    );
                }
            }

            if self.control.peek_slot().is_err() {
                return;
            }
            x += width.get();
            run_offset += usize::from(run.get());
            aa_offset += usize::from(run.get());
            run_opt = runs[run_offset];
        }
    }

    fn blit_v(&mut self, x: u32, y: u32, height: LengthU32, alpha: AlphaU8) {
        if self.control.peek_slot().is_err() {
            return;
        }
        let bounds = ScreenIntRect::from_xywh_safe(x, y, LENGTH_U32_ONE, height);

        let mask = Mask {
            image: [alpha, alpha],
            bounds,
            row_bytes: 0, // so we reuse the 1 "row" for all of height
        };

        self.blit_mask(&mask, &bounds);
    }

    fn blit_anti_h2(&mut self, x: u32, y: u32, alpha0: AlphaU8, alpha1: AlphaU8) {
        if self.control.peek_slot().is_err() {
            return;
        }
        let bounds = ScreenIntRect::from_xywh(x, y, 2, 1).unwrap();

        let mask = Mask {
            image: [alpha0, alpha1],
            bounds,
            row_bytes: 2,
        };

        self.blit_mask(&mask, &bounds);
    }

    fn blit_anti_v2(&mut self, x: u32, y: u32, alpha0: AlphaU8, alpha1: AlphaU8) {
        if self.control.peek_slot().is_err() {
            return;
        }
        let bounds = ScreenIntRect::from_xywh(x, y, 1, 2).unwrap();

        let mask = Mask {
            image: [alpha0, alpha1],
            bounds,
            row_bytes: 1,
        };

        self.blit_mask(&mask, &bounds);
    }

    fn blit_rect(&mut self, rect: &ScreenIntRect) {
        if self.control.peek_slot().is_err() {
            return;
        }
        if let Some(c) = self.memset2d_color {
            if self.is_mask {
                for y in 0..rect.height() {
                    let start = self
                        .pixmap
                        .offset(rect.x() as usize, (rect.y() + y) as usize);
                    let end = start + rect.width() as usize;
                    for chunk in self.pixmap.data[start..end].chunks_mut(256) {
                        if self.control.check(chunk.len() as u64).is_err() {
                            return;
                        }
                        chunk.fill(c.alpha());
                    }
                }
            } else {
                for y in 0..rect.height() {
                    let start = self
                        .pixmap
                        .offset(rect.x() as usize, (rect.y() + y) as usize);
                    let end = start + rect.width() as usize;
                    for chunk in self.pixmap.pixels_mut()[start..end].chunks_mut(256) {
                        if self.control.check(chunk.len() as u64).is_err() {
                            return;
                        }
                        chunk.fill(c);
                    }
                }
            }

            return;
        }

        let mask_ctx = self.mask.map(|c| c.mask_ctx()).unwrap_or_default();

        let _ = self.blit_rect_rp.run(
            rect,
            pipeline::AAMaskCtx::default(),
            mask_ctx,
            self.pixmap_src,
            self.pixmap,
        );
    }

    fn blit_mask(&mut self, mask: &Mask, clip: &ScreenIntRect) {
        if self.control.peek_slot().is_err() {
            return;
        }
        let aa_mask_ctx = pipeline::AAMaskCtx {
            pixels: mask.image,
            stride: mask.row_bytes,
            shift: (mask.bounds.left() + mask.bounds.top() * mask.row_bytes) as usize,
        };

        let mask_ctx = self.mask.map(|c| c.mask_ctx()).unwrap_or_default();

        let _ = self
            .blit_mask_rp
            .run(clip, aa_mask_ctx, mask_ctx, self.pixmap_src, self.pixmap);
    }
}
