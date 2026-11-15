// Copyright 2006 The Android Open Source Project
// Modifications Copyright (c) 2026, Jiaqi (0x7c13) Liu.
//
// Use of this source code is governed by a BSD-style license that can be
// found in the LICENSE file.

use super::checked::{BuiltShader, GradientKind};
use super::gradient::{normalize, GradientView};
use crate::edge_builder::RasterVec;
use crate::painter::GeometryControl;
use alloc::vec::Vec;
use markdown_svg_budget::Error;

use tiny_skia_path::Scalar;

use crate::{ColorSpace, GradientStop, Point, Shader, SpreadMode, Transform};

use super::gradient::{Gradient, DEGENERATE_THRESHOLD};
use crate::pipeline::{RasterPipelineBuilder, Stage};

#[cfg(all(not(feature = "std"), feature = "no-std-float"))]
use tiny_skia_path::NoStdFloat;

/// A radial gradient.
#[derive(Clone, PartialEq, Debug)]
pub struct SweepGradient {
    pub(crate) base: Gradient,
    pub(super) t0: f32,
    pub(super) t1: f32,
}

impl SweepGradient {
    /// Creates a new 2-point conical gradient shader.
    #[allow(clippy::new_ret_no_self)]
    pub fn new(
        center: Point,
        start_angle: f32,
        end_angle: f32,
        stops: Vec<GradientStop>,
        mode: SpreadMode,
        transform: Transform,
    ) -> Option<Shader<'static>> {
        super::checked::ordinary(build(
            center,
            start_angle,
            end_angle,
            RasterVec::Plain(stops),
            mode,
            transform,
            GeometryControl::ordinary(),
        ))
    }

    pub(crate) fn is_opaque(&self) -> bool {
        self.base.colors_are_opaque
    }
}

pub(super) fn build(
    center: Point,
    start_angle: f32,
    end_angle: f32,
    stops: RasterVec<GradientStop>,
    mut mode: SpreadMode,
    transform: Transform,
    control: GeometryControl,
) -> Result<Option<BuiltShader>, Error> {
    if !start_angle.is_finite() || !end_angle.is_finite() || start_angle > end_angle {
        return Ok(None);
    }

    match &*stops {
        [] => return Ok(None),
        [stop] => return Ok(Some(BuiltShader::Solid(stop.color))),
        _ => (),
    }
    if transform.invert().is_none() {
        return Ok(None);
    }
    if start_angle.is_nearly_equal_within_tolerance(end_angle, DEGENERATE_THRESHOLD) {
        if mode == SpreadMode::Pad && end_angle > DEGENERATE_THRESHOLD {
            // In this case, the first color is repeated from 0 to the angle, then a hardstop
            // switches to the last color (all other colors are compressed to the infinitely
            // thin interpolation region).
            let front_color = stops.first().unwrap().color;
            let back_color = stops.last().unwrap().color;
            let mut new_stops = stops;
            match &mut new_stops {
                RasterVec::Plain(v) => v.clear(),
                RasterVec::Paid(v) => v.clear(),
            };
            for stop in [
                GradientStop::new(0.0, front_color),
                GradientStop::new(1.0, front_color),
                GradientStop::new(1.0, back_color),
            ] {
                new_stops.push(stop, control)?;
            }
            return build(center, 0.0, end_angle, new_stops, mode, transform, control);
        }
        // TODO: Consider making a degenerate fallback shader similar to Skia. Tiny Skia
        // currently opts to return `None` in some places.
        return Ok(None);
    }
    if start_angle <= 0.0 && end_angle >= 360.0 {
        mode = SpreadMode::Pad;
    }
    let t0 = start_angle / 360.0;
    let t1 = end_angle / 360.0;
    Ok(Some(BuiltShader::Gradient(
        normalize(
            stops,
            mode,
            transform,
            Transform::from_translate(-center.x, -center.y),
            control,
        )?,
        GradientKind::Sweep(t0, t1),
    )))
}

pub(super) fn push_stages(
    base: GradientView,
    t0: f32,
    t1: f32,
    cs: ColorSpace,
    p: &mut RasterPipelineBuilder,
) -> Result<bool, Error> {
    let scale = 1.0 / (t1 - t0);
    let bias = -scale * t0;
    p.context_mut()?.two_point_conical_gradient.p0 = scale;
    p.context_mut()?.two_point_conical_gradient.p1 = bias;
    base.push_stages(
        p,
        cs,
        &|p| {
            p.push(Stage::XYToUnitAngle);
            if scale != 1.0 && bias != 0.0 {
                p.push(Stage::ApplyConcentricScaleBias)
            }
        },
        &|_| {},
    )
}
