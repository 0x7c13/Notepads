// Copyright 2006 The Android Open Source Project
// Modifications Copyright (c) 2026, Jiaqi (0x7c13) Liu.
// Copyright 2020 Yevhenii Reizner
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
use crate::pipeline;
use crate::pipeline::RasterPipelineBuilder;
use crate::wide::u32x8;

#[cfg(all(not(feature = "std"), feature = "no-std-float"))]
use tiny_skia_path::NoStdFloat;

#[derive(Copy, Clone, PartialEq, Debug, Default)]
pub(super) struct FocalData {
    r1: f32,      // r1 after mapping focal point to (0, 0)
    focal_x: f32, // f
    is_swapped: bool,
}

impl FocalData {
    fn set(&mut self, mut r0: f32, mut r1: f32, matrix: &mut Transform) -> bool {
        self.is_swapped = false;
        self.focal_x = r0 / (r0 - r1);

        if (self.focal_x - 1.0).is_nearly_zero() {
            // swap r0, r1
            *matrix = matrix.post_translate(-1.0, 0.0).post_scale(-1.0, 1.0);
            core::mem::swap(&mut r0, &mut r1);

            self.focal_x = 0.0; // because r0 is now 0
            self.is_swapped = true;
        }

        // Map {focal point, (1, 0)} to {(0, 0), (1, 0)}
        let from = [Point::from_xy(self.focal_x, 0.0), Point::from_xy(1.0, 0.0)];
        let to = [Point::from_xy(0.0, 0.0), Point::from_xy(1.0, 0.0)];

        let focal_matrix = match ts_from_poly_to_poly(from[0], from[1], to[0], to[1]) {
            Some(m) => m,
            None => return false,
        };

        *matrix = matrix.post_concat(focal_matrix);
        self.r1 = r1 / (1.0 - self.focal_x).abs(); // focalMatrix has a scale of 1/(1-f).

        // The following transformations are just to accelerate the shader computation by saving
        // some arithmetic operations.
        if self.is_focal_on_circle() {
            *matrix = matrix.post_scale(0.5, 0.5);
        } else {
            *matrix = matrix.post_scale(
                self.r1 / (self.r1 * self.r1 - 1.0),
                1.0 / (self.r1 * self.r1 - 1.0).abs().sqrt(),
            );
        }

        *matrix = matrix.post_scale((1.0 - self.focal_x).abs(), (1.0 - self.focal_x).abs()); // scale |1 - f|

        true
    }

    fn is_focal_on_circle(&self) -> bool {
        (1.0 - self.r1).is_nearly_zero()
    }

    fn is_well_behaved(&self) -> bool {
        !self.is_focal_on_circle() && self.r1 > 1.0
    }

    fn is_natively_focal(&self) -> bool {
        self.focal_x.is_nearly_zero()
    }
}

#[derive(Clone, Copy, PartialEq, Debug)]
pub(super) enum GradientType {
    Radial {
        radius1: f32,
        radius2: f32,
    },
    Strip {
        /// Radius of the first circle scaled by the distance between centers (r0 / d_center)
        scaled_r0: f32,
    },
    Focal(FocalData),
}

/// A 2-point conical gradient shader.
#[derive(Clone, PartialEq, Debug)]
pub struct RadialGradient {
    pub(crate) base: Gradient,
    pub(super) gradient_type: GradientType,
}

impl RadialGradient {
    /// Creates a new two-point conical gradient shader.
    ///
    /// A two-point conical gradient (also known as a radial gradient)
    /// interpolates colors between two circles defined by their center points
    /// and radii.
    ///
    /// Returns `Shader::SolidColor` when:
    /// - `stops.len()` == 1
    ///
    /// Returns `None` when:
    /// - `stops` is empty
    /// - `start_radius` < 0 or `end_radius` < 0
    /// - `transform` is not invertible
    /// - The gradient is degenerate (both radii and centers are equal, except
    ///   in specific pad mode cases)
    #[allow(clippy::new_ret_no_self)]
    pub fn new(
        start_point: Point,
        start_radius: f32,
        end_point: Point,
        end_radius: f32,
        stops: Vec<GradientStop>,
        mode: SpreadMode,
        transform: Transform,
    ) -> Option<Shader<'static>> {
        super::checked::ordinary(build(
            start_point,
            start_radius,
            end_point,
            end_radius,
            RasterVec::Plain(stops),
            mode,
            transform,
            GeometryControl::ordinary(),
        ))
    }
}

fn create(
    c0: Point,
    r0: f32,
    c1: Point,
    r1: f32,
    stops: RasterVec<GradientStop>,
    mode: SpreadMode,
    transform: Transform,
    control: GeometryControl,
) -> Result<Option<BuiltShader>, Error> {
    let mut gradient_type;
    let mut gradient_matrix;

    if (c0 - c1).length().is_nearly_zero() {
        if r0.max(r1).is_nearly_zero() || r0.is_nearly_equal(r1) {
            // Degenerate case; avoid dividing by zero. Should have been caught
            // by caller but just in case, recheck here.
            return Ok(None);
        }

        // Concentric case: we can pretend we're radial (with a tiny twist).
        let scale = 1.0 / r0.max(r1);
        gradient_matrix = Transform::from_translate(-c1.x, -c1.y).post_scale(scale, scale);
        gradient_type = GradientType::Radial {
            radius1: r0,
            radius2: r1,
        };
    } else {
        let Some(matrix) = map_to_unit_x(c0, c1) else {
            return Ok(None);
        };
        gradient_matrix = matrix;
        let d_center = (c0 - c1).length();
        gradient_type = if (r0 - r1).is_nearly_zero() {
            let scaled_r0 = r0 / d_center;
            GradientType::Strip { scaled_r0 }
        } else {
            GradientType::Focal(FocalData::default())
        };
    }
    if let GradientType::Focal(ref mut focal_data) = &mut gradient_type {
        let d_center = (c0 - c1).length();
        if !focal_data.set(r0 / d_center, r1 / d_center, &mut gradient_matrix) {
            return Ok(None);
        }
    }

    Ok(Some(BuiltShader::Gradient(
        normalize(stops, mode, transform, gradient_matrix, control)?,
        GradientKind::Radial(gradient_type),
    )))
}

fn map_to_unit_x(origin: Point, x_is_one: Point) -> Option<Transform> {
    ts_from_poly_to_poly(
        origin,
        x_is_one,
        Point::from_xy(0.0, 0.0),
        Point::from_xy(1.0, 0.0),
    )
}

fn ts_from_poly_to_poly(src1: Point, src2: Point, dst1: Point, dst2: Point) -> Option<Transform> {
    let tmp = from_poly2(src1, src2);
    let res = tmp.invert()?;
    let tmp = from_poly2(dst1, dst2);
    Some(tmp.pre_concat(res))
}

fn from_poly2(p0: Point, p1: Point) -> Transform {
    Transform::from_row(
        p1.y - p0.y,
        p0.x - p1.x,
        p1.x - p0.x,
        p1.y - p0.y,
        p0.x,
        p0.y,
    )
}

pub(super) fn build(
    start_point: Point,
    start_radius: f32,
    end_point: Point,
    end_radius: f32,
    stops: RasterVec<GradientStop>,
    mode: SpreadMode,
    transform: Transform,
    control: GeometryControl,
) -> Result<Option<BuiltShader>, Error> {
    if start_radius < 0.0 || end_radius < 0.0 {
        return Ok(None);
    }

    match &*stops {
        [] => return Ok(None),
        [stop] => return Ok(Some(BuiltShader::Solid(stop.color))),
        _ => {}
    }

    if transform.invert().is_none() {
        return Ok(None);
    }

    let length = (start_point - end_point).length();
    if !length.is_finite() {
        return Ok(None);
    }
    if length.is_nearly_zero_within_tolerance(DEGENERATE_THRESHOLD) {
        if start_radius.is_nearly_equal_within_tolerance(end_radius, DEGENERATE_THRESHOLD) {
            // Degenerate case, where the interpolation region area approaches zero. The proper
            // behavior depends on the tile mode, which is consistent with the default degenerate
            // gradient behavior, except when mode = clamp and the radii > 0.
            if mode == SpreadMode::Pad && end_radius > DEGENERATE_THRESHOLD {
                // The interpolation region becomes an infinitely thin ring at the radius, so the
                // final gradient will be the first color repeated from p=0 to 1, and then a hard
                // stop switching to the last color at p=1.
                let start_color = stops.first().unwrap().color;
                let end_color = stops.last().unwrap().color;
                let mut new_stops = stops; // Reuse allocation from stops.
                match &mut new_stops {
                    RasterVec::Plain(v) => v.clear(),
                    RasterVec::Paid(v) => v.clear(),
                };
                for stop in [
                    GradientStop::new(0.0, start_color),
                    GradientStop::new(1.0, start_color),
                    GradientStop::new(1.0, end_color),
                ] {
                    new_stops.push(stop, control)?;
                }
                // If the center positions are the same, then the gradient is the radial variant
                // of a 2 pt conical gradient, an actual radial gradient (startRadius == 0), or
                // it is fully degenerate (startRadius == endRadius).
                // We can treat this gradient as a simple radial, which is faster. If we got
                // here, we know that endRadius is not equal to 0, so this produces a meaningful
                // gradient
                return new_radial_unchecked(
                    start_point,
                    end_radius,
                    new_stops,
                    mode,
                    transform,
                    control,
                );
            }
            // TODO: Consider making a degenerate gradient
            return Ok(None);
        }

        if start_radius.is_nearly_zero_within_tolerance(DEGENERATE_THRESHOLD) {
            // If the center positions are the same, then the gradient
            // is the radial variant of a 2 pt conical gradient,
            // an actual radial gradient (startRadius == 0),
            // or it is fully degenerate (startRadius == endRadius).
            // We can treat this gradient as a simple radial, which is faster. If we got here,
            // we know that endRadius is not equal to 0, so this produces a meaningful gradient.
            return new_radial_unchecked(start_point, end_radius, stops, mode, transform, control);
        }
    }

    create(
        start_point,
        start_radius,
        end_point,
        end_radius,
        stops,
        mode,
        transform,
        control,
    )
}
/// Creates a simple radial gradient shader without validation.
///
/// This is an optimized path for creating radial gradients when the start radius is 0
/// and the gradient is known to be valid. The function computes the points-to-unit
/// transformation internally based on the center point and radius.
///
/// # Parameters
/// - `center`: The center point of the radial gradient
/// - `radius`: The radius of the gradient (assumed to be > 0)
/// - `stops`: Color stops for the gradient (assumed to have length >= 2)
/// - `mode`: How the gradient extends beyond its bounds
/// - `transform`: The gradient's transformation matrix (assumed to be invertible)
fn new_radial_unchecked(
    center: Point,
    radius: f32,
    stops: RasterVec<GradientStop>,
    mode: SpreadMode,
    transform: Transform,
    control: GeometryControl,
) -> Result<Option<BuiltShader>, Error> {
    let inv = radius.invert();
    let points_to_unit = Transform::from_translate(-center.x, -center.y).post_scale(inv, inv);

    Ok(Some(BuiltShader::Gradient(
        normalize(stops, mode, transform, points_to_unit, control)?,
        GradientKind::Radial(GradientType::Radial {
            radius1: 0.0,
            radius2: radius,
        }),
    )))
}

pub(super) fn push_stages(
    base: GradientView,
    gradient_type: GradientType,
    cs: ColorSpace,
    p: &mut RasterPipelineBuilder,
) -> Result<bool, Error> {
    let (p0, p1) = match gradient_type {
        GradientType::Radial { radius1, radius2 } => {
            if radius1 == 0.0 {
                (1.0, 0.0)
            } else {
                let d_radius = radius2 - radius1;
                // For concentric gradients: t = t * scale + bias
                let p0 = radius1.max(radius2) / d_radius;
                let p1 = -radius1 / d_radius;
                (p0, p1)
            }
        }
        GradientType::Strip { scaled_r0 } => {
            (scaled_r0 * scaled_r0, 0.0 /*unused*/)
        }
        GradientType::Focal(fd) => (1.0 / fd.r1, fd.focal_x),
    };

    p.context_mut()?.two_point_conical_gradient = pipeline::TwoPointConicalGradientCtx {
        mask: u32x8::default(),
        p0,
        p1,
    };

    base.push_stages(
        p,
        cs,
        &|p| {
            match gradient_type {
                GradientType::Radial { .. } => {
                    p.push(pipeline::Stage::XYToRadius);
                    // Apply scale/bias to map t from [0, 1] based on r_max to proper t where
                    // t=0 at r0 and t=1 at r1
                    if (p0, p1) != (1.0, 0.0) {
                        p.push(pipeline::Stage::ApplyConcentricScaleBias);
                    }
                }
                GradientType::Strip { .. } => {
                    p.push(pipeline::Stage::XYTo2PtConicalStrip);
                    p.push(pipeline::Stage::Mask2PtConicalNan);
                }
                GradientType::Focal(fd) => {
                    if fd.is_focal_on_circle() {
                        p.push(pipeline::Stage::XYTo2PtConicalFocalOnCircle);
                    } else if fd.is_well_behaved() {
                        p.push(pipeline::Stage::XYTo2PtConicalWellBehaved);
                    } else if fd.is_swapped || (1.0 - fd.focal_x) < 0.0 {
                        p.push(pipeline::Stage::XYTo2PtConicalSmaller);
                    } else {
                        p.push(pipeline::Stage::XYTo2PtConicalGreater);
                    }

                    if !fd.is_well_behaved() {
                        p.push(pipeline::Stage::Mask2PtConicalDegenerates);
                    }

                    if (1.0 - fd.focal_x) < 0.0 {
                        p.push(pipeline::Stage::NegateX);
                    }

                    if !fd.is_natively_focal() {
                        p.push(pipeline::Stage::Alter2PtConicalCompensateFocal);
                    }

                    if fd.is_swapped {
                        p.push(pipeline::Stage::Alter2PtConicalUnswap);
                    }
                }
            }
        },
        &|p| match gradient_type {
            GradientType::Strip { .. } => p.push(pipeline::Stage::ApplyVectorMask),
            GradientType::Focal(fd) if !fd.is_well_behaved() => {
                p.push(pipeline::Stage::ApplyVectorMask)
            }
            _ => {}
        },
    )
}
