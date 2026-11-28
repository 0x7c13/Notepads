// Copyright 2006 The Android Open Source Project
// Copyright 2020 Yevhenii Reizner
// Modifications Copyright (c) 2026, Jiaqi (0x7c13) Liu.
//
// Use of this source code is governed by a BSD-style license that can be
// found in the LICENSE file.

use crate::edge_builder::RasterVec;
use crate::painter::GeometryControl;
use alloc::vec::Vec;
use markdown_svg_budget::Error;

use tiny_skia_path::{NormalizedF32, Scalar};

use crate::{Color, ColorSpace, SpreadMode, Transform};

use crate::pipeline::RasterPipelineBuilder;
use crate::pipeline::{self, EvenlySpaced2StopGradientCtx, GradientColor, GradientCtx};

// The default SCALAR_NEARLY_ZERO threshold of .0024 is too big and causes regressions for svg
// gradients defined in the wild.
pub const DEGENERATE_THRESHOLD: f32 = 1.0 / (1 << 15) as f32;

/// A gradient point.
#[allow(missing_docs)]
#[derive(Clone, Copy, PartialEq, Debug)]
pub struct GradientStop {
    pub(crate) position: NormalizedF32,
    pub(crate) color: Color,
}

impl GradientStop {
    /// Creates a new gradient point.
    ///
    /// `position` will be clamped to a 0..=1 range.
    pub fn new(position: f32, color: Color) -> Self {
        GradientStop {
            position: NormalizedF32::new_clamped(position),
            color,
        }
    }
}

#[derive(Clone, PartialEq, Debug)]
pub struct Gradient {
    pub(super) stops: Vec<GradientStop>,
    pub(super) tile_mode: SpreadMode,
    pub(crate) transform: Transform,
    pub(super) points_to_unit: Transform,
    pub(crate) colors_are_opaque: bool,
    pub(super) has_uniform_stops: bool,
}

impl Gradient {
    pub(super) fn view(&self) -> GradientView<'_, 'static> {
        GradientView {
            stops: &self.stops,
            tile_mode: self.tile_mode,
            transform: self.transform,
            points_to_unit: self.points_to_unit,
            colors_are_opaque: self.colors_are_opaque,
            has_uniform_stops: self.has_uniform_stops,
            opacity_ops: &[],
        }
    }
    pub fn apply_opacity(&mut self, opacity: f32) {
        for stop in &mut self.stops {
            stop.color.apply_opacity(opacity);
        }

        self.colors_are_opaque = self.stops.iter().all(|p| p.color.is_opaque());
    }
}

#[derive(Clone, Copy)]
pub(super) struct GradientMeta {
    pub tile_mode: SpreadMode,
    pub transform: Transform,
    pub points_to_unit: Transform,
    pub colors_are_opaque: bool,
    pub has_uniform_stops: bool,
}
pub(super) struct GradientConstruction {
    pub stops: RasterVec<GradientStop>,
    pub meta: GradientMeta,
}
impl GradientConstruction {
    pub fn into_plain(self) -> Gradient {
        let RasterVec::Plain(stops) = self.stops else {
            unreachable!("private ordinary factory")
        };
        Gradient {
            stops,
            tile_mode: self.meta.tile_mode,
            transform: self.meta.transform,
            points_to_unit: self.meta.points_to_unit,
            colors_are_opaque: self.meta.colors_are_opaque,
            has_uniform_stops: self.meta.has_uniform_stops,
        }
    }
}
pub(super) fn normalize(
    mut stops: RasterVec<GradientStop>,
    tile_mode: SpreadMode,
    transform: Transform,
    points_to_unit: Transform,
    control: GeometryControl,
) -> Result<GradientConstruction, Error> {
    debug_assert!(stops.len() > 1);

    // Note: we let the caller skip the first and/or last position.
    // i.e. pos[0] = 0.3, pos[1] = 0.7
    // In these cases, we insert dummy entries to ensure that the final data
    // will be bracketed by [0, 1].
    // i.e. our_pos[0] = 0, our_pos[1] = 0.3, our_pos[2] = 0.7, our_pos[3] = 1
    let dummy_first = stops[0].position.get() != 0.0;
    let dummy_last = stops[stops.len() - 1].position.get() != 1.0;

    // Now copy over the colors, adding the dummies as needed.
    if dummy_first {
        stops.insert_first(GradientStop::new(0.0, stops[0].color), control)?;
    }

    if dummy_last {
        stops.push(
            GradientStop::new(1.0, stops[stops.len() - 1].color),
            control,
        )?;
    }

    let mut colors_are_opaque = true;
    for stop in stops.iter() {
        control.check(1)?;
        if !stop.color.is_opaque() {
            colors_are_opaque = false;
            break;
        }
    }

    // Pin the last value to 1.0, and make sure positions are monotonic.
    let start_index = if dummy_first { 0 } else { 1 };
    let mut prev = 0.0;
    let mut has_uniform_stops = true;
    let uniform_step = stops[start_index].position.get() - prev;
    for i in start_index..stops.len() {
        control.check(1)?;
        let curr = if i + 1 == stops.len() {
            // The last one must be zero.
            1.0
        } else {
            stops[i].position.get().bound(prev, 1.0)
        };

        has_uniform_stops &= uniform_step.is_nearly_equal(curr - prev);
        stops[i].position = NormalizedF32::new_clamped(curr);
        prev = curr;
    }

    Ok(GradientConstruction {
        stops,
        meta: GradientMeta {
            tile_mode,
            transform,
            points_to_unit,
            colors_are_opaque,
            has_uniform_stops,
        },
    })
}

#[derive(Clone, Copy)]
pub(super) struct GradientView<'s, 'ops> {
    pub stops: &'s [GradientStop],
    pub tile_mode: SpreadMode,
    pub transform: Transform,
    pub points_to_unit: Transform,
    pub colors_are_opaque: bool,
    pub has_uniform_stops: bool,
    pub opacity_ops: &'ops [f32],
}
impl GradientView<'_, '_> {
    pub fn effective_color(&self, index: usize, control: GeometryControl) -> Result<Color, Error> {
        let mut color = self.stops[index].color;
        for &opacity in self.opacity_ops {
            control.check(1)?;
            color.apply_opacity(opacity);
        }
        Ok(color)
    }
    pub fn is_opaque(&self, control: GeometryControl) -> Result<bool, Error> {
        if self.opacity_ops.is_empty() {
            return Ok(self.colors_are_opaque);
        }
        for i in 0..self.stops.len() {
            control.check(1)?;
            if !self.effective_color(i, control)?.is_opaque() {
                return Ok(false);
            }
        }
        Ok(true)
    }
    pub fn coverage_coordinates_valid(&self) -> bool {
        self.transform.is_finite()
            && self.transform.invert().is_some_and(|inverse| {
                inverse.is_finite() && inverse.post_concat(self.points_to_unit).is_finite()
            })
    }
    pub fn push_stages(
        &self,
        p: &mut RasterPipelineBuilder,
        cs: ColorSpace,
        push_stages_pre: &dyn Fn(&mut RasterPipelineBuilder),
        push_stages_post: &dyn Fn(&mut RasterPipelineBuilder),
    ) -> Result<bool, Error> {
        let control = p.control();
        control.peek_slot()?;
        p.push(pipeline::Stage::SeedShader);

        let ts = match self.transform.invert() {
            Some(v) => v,
            None => {
                log::warn!("failed to invert a gradient transform. Nothing will be rendered");
                return Ok(false);
            }
        };
        let ts = ts.post_concat(self.points_to_unit);
        p.push_transform(ts);

        push_stages_pre(p);

        match self.tile_mode {
            SpreadMode::Reflect => {
                p.push(pipeline::Stage::ReflectX1);
            }
            SpreadMode::Repeat => {
                p.push(pipeline::Stage::RepeatX1);
            }
            SpreadMode::Pad => {
                if self.has_uniform_stops {
                    // We clamp only when the stops are evenly spaced.
                    // If not, there may be hard stops, and clamping ruins hard stops at 0 and/or 1.
                    // In that case, we must make sure we're using the general "gradient" stage,
                    // which is the only stage that will correctly handle unclamped t.
                    p.push(pipeline::Stage::PadX1);
                }
            }
        }

        // The two-stop case with stops at 0 and 1.
        if self.stops.len() == 2 {
            debug_assert!(self.has_uniform_stops);

            let c0 = cs.expand_color(self.effective_color(0, control)?);
            let c1 = cs.expand_color(self.effective_color(1, control)?);

            p.context_mut()?.evenly_spaced_2_stop_gradient = EvenlySpaced2StopGradientCtx {
                factor: GradientColor::new(
                    c1.red() - c0.red(),
                    c1.green() - c0.green(),
                    c1.blue() - c0.blue(),
                    c1.alpha() - c0.alpha(),
                ),
                bias: GradientColor::from(c0),
            };

            p.push(pipeline::Stage::EvenlySpaced2StopGradient);
        } else {
            // Unlike Skia, we do not support the `evenly_spaced_gradient` stage.
            // In our case, there is no performance difference.

            let mut ctx = GradientCtx::new(control);

            // Note: In order to handle clamps in search, the search assumes
            // a stop conceptually placed at -inf.
            // Therefore, the max number of stops is `self.points.len()+1`.
            //
            // We also need at least 16 values for lowp pipeline.
            ctx.factors.reserve((self.stops.len() + 1).max(16))?;
            ctx.biases.reserve((self.stops.len() + 1).max(16))?;

            ctx.t_values.reserve(self.stops.len() + 1)?;

            // Remove the dummy stops inserted by Gradient::new
            // because they are naturally handled by the search method.
            let (first_stop, last_stop) = if self.stops.len() > 2 {
                let first =
                    if self.effective_color(0, control)? != self.effective_color(1, control)? {
                        0
                    } else {
                        1
                    };

                let len = self.stops.len();
                let last = if self.effective_color(len - 2, control)?
                    != self.effective_color(len - 1, control)?
                {
                    len - 1
                } else {
                    len - 2
                };
                (first, last)
            } else {
                (0, 1)
            };

            let mut t_l = self.stops[first_stop].position.get();
            let mut c_l =
                GradientColor::from(cs.expand_color(self.effective_color(first_stop, control)?));
            ctx.push_const_color(c_l, control)?;
            ctx.t_values.push(NormalizedF32::ZERO, control)?;
            // N.B. lastStop is the index of the last stop, not one after.
            for i in first_stop..last_stop {
                control.check(1)?;
                let t_r = self.stops[i + 1].position.get();
                let c_r =
                    GradientColor::from(cs.expand_color(self.effective_color(i + 1, control)?));
                debug_assert!(t_l <= t_r);
                if t_l < t_r {
                    // For each stop we calculate a bias B and a scale factor F, such that
                    // for any t between stops n and n+1, the color we want is B[n] + F[n]*t.
                    let f = GradientColor::new(
                        (c_r.r - c_l.r) / (t_r - t_l),
                        (c_r.g - c_l.g) / (t_r - t_l),
                        (c_r.b - c_l.b) / (t_r - t_l),
                        (c_r.a - c_l.a) / (t_r - t_l),
                    );
                    ctx.factors.push(f, control)?;

                    ctx.biases.push(
                        GradientColor::new(
                            c_l.r - f.r * t_l,
                            c_l.g - f.g * t_l,
                            c_l.b - f.b * t_l,
                            c_l.a - f.a * t_l,
                        ),
                        control,
                    )?;

                    ctx.t_values
                        .push(NormalizedF32::new_clamped(t_l), control)?;
                }

                t_l = t_r;
                c_l = c_r;
            }

            ctx.push_const_color(c_l, control)?;
            ctx.t_values
                .push(NormalizedF32::new_clamped(t_l), control)?;

            ctx.len = ctx.factors.len();

            // All lists must have the same length.
            debug_assert_eq!(ctx.factors.len(), ctx.t_values.len());
            debug_assert_eq!(ctx.biases.len(), ctx.t_values.len());

            // Will with zeros until we have enough data to fit into F32x16.
            while ctx.factors.len() < 16 {
                ctx.factors.push(GradientColor::default(), control)?;
                ctx.biases.push(GradientColor::default(), control)?;
            }

            p.push(pipeline::Stage::Gradient);
            p.context_mut()?.gradient = ctx;
        }

        if !self.is_opaque(control)? {
            p.push(pipeline::Stage::Premultiply);
        }

        push_stages_post(p);

        control.peek_slot()?;
        Ok(true)
    }
}
