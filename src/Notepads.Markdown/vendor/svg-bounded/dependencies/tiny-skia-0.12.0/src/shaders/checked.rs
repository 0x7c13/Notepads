// Copyright 2006 The Android Open Source Project
// Copyright 2020 Yevhenii Reizner
// Modifications Copyright (c) 2026, Jiaqi (0x7c13) Liu.
//
// Use of this source code is governed by a BSD-style license that can be
// found in the LICENSE file.

//! Request-owned paint. Ordinary shaders cannot contain these owners.
use super::gradient::{GradientConstruction, GradientMeta, GradientView};
use super::radial_gradient::GradientType;
use crate::edge_builder::RasterVec;
use crate::painter::GeometryControl;
use crate::pipeline::RasterPipelineBuilder;
use crate::{
    BlendMode, Color, ColorSpace, FilterQuality, GradientStop, Paint, Pattern, PixmapRef, Point,
    Shader, SpreadMode, Transform,
};
use markdown_svg_budget::{BudgetVec, Error, PaidShared, ResourceKind, ResourcesRef};
use tiny_skia_path::{NormalizedF32, Scalar};

/// Scalar drawing options shared by plain and checked paints.
#[derive(Clone, Copy, Debug)]
pub struct PaintOptions {
    /// Pixel blend operation.
    pub blend_mode: BlendMode,
    /// Enable geometric antialiasing.
    pub anti_alias: bool,
    /// Force the original high precision pipeline.
    pub force_hq_pipeline: bool,
    /// Color interpolation and destination color space.
    pub colorspace: ColorSpace,
}
impl Default for PaintOptions {
    fn default() -> Self {
        Self {
            blend_mode: BlendMode::default(),
            anti_alias: true,
            force_hq_pipeline: false,
            colorspace: ColorSpace::default(),
        }
    }
}
impl From<&Paint<'_>> for PaintOptions {
    fn from(p: &Paint) -> Self {
        Self {
            blend_mode: p.blend_mode,
            anti_alias: p.anti_alias,
            force_hq_pipeline: p.force_hq_pipeline,
            colorspace: p.colorspace,
        }
    }
}

/// Stops whose sole physical buffer is charged to the original request.
#[derive(Debug)]
pub struct CheckedStops {
    values: BudgetVec<GradientStop>,
}
impl CheckedStops {
    /// Create an empty scratch buffer in the original request.
    pub fn new(resources: &ResourcesRef) -> Self {
        Self {
            values: BudgetVec::new(resources.clone(), ResourceKind::Scratch),
        }
    }
    /// Reserve before accepting stops.
    pub fn try_reserve(&mut self, additional: usize) -> Result<(), Error> {
        self.values.try_reserve(additional)
    }
    /// Append one stop through the paid owner.
    pub fn try_push(&mut self, stop: GradientStop) -> Result<(), Error> {
        self.values.resources().checkpoint(1)?;
        self.values.try_push(stop)
    }
    fn validate(&self, resources: &ResourcesRef) -> Result<(), Error> {
        let control = GeometryControl::request(resources);
        control.peek_slot()?;
        if !self.values.resources().is_same_request(resources) {
            return Err(control.invariant());
        }
        Ok(())
    }
}

#[derive(Clone, Copy)]
pub(super) enum GradientKind {
    Linear,
    Radial(GradientType),
    Sweep(f32, f32),
}
pub(super) enum BuiltShader {
    Solid(Color),
    Gradient(GradientConstruction, GradientKind),
}
/// Convert only the private ordinary factory's plain storage. No paid owner can escape.
pub(super) fn ordinary(result: Result<Option<BuiltShader>, Error>) -> Option<Shader<'static>> {
    result
        .expect("ordinary gradient policy has no fallible resource backend")
        .map(|shader| match shader {
            BuiltShader::Solid(color) => Shader::SolidColor(color),
            BuiltShader::Gradient(base, kind) => {
                let base = base.into_plain();
                match kind {
                    GradientKind::Linear => Shader::LinearGradient(super::LinearGradient { base }),
                    GradientKind::Radial(gradient_type) => {
                        Shader::RadialGradient(super::RadialGradient {
                            base,
                            gradient_type,
                        })
                    }
                    GradientKind::Sweep(t0, t1) => {
                        Shader::SweepGradient(super::SweepGradient { base, t0, t1 })
                    }
                }
            }
        })
}

struct GradientData {
    values: BudgetVec<GradientStop>,
    meta: GradientMeta,
    kind: GradientKind,
}
enum CheckedShader<'img> {
    Solid(Color),
    Gradient(PaidShared<GradientData>),
    Pattern(Pattern<'img>),
}
/// Immutable request paint. It has no ordinary shader, mutable stop or deep Clone escape.
pub struct CheckedPaint<'img> {
    resources: ResourcesRef,
    shader: CheckedShader<'img>,
    options: PaintOptions,
}
impl core::fmt::Debug for CheckedPaint<'_> {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("CheckedPaint")
            .field("options", &self.options)
            .finish_non_exhaustive()
    }
}
impl CheckedPaint<'static> {
    /// Construct a linear gradient, preserving the ordinary degenerate cases.
    pub fn try_linear(
        start: Point,
        end: Point,
        stops: CheckedStops,
        spread: SpreadMode,
        transform: Transform,
        options: PaintOptions,
        resources: &ResourcesRef,
    ) -> Result<Option<Self>, Error> {
        stops.validate(resources)?;
        Self::from_built(
            super::linear_gradient::build(
                start,
                end,
                RasterVec::Paid(stops.values),
                spread,
                transform,
                GeometryControl::request(resources),
            ),
            options,
            resources,
        )
    }
    /// Construct a two point conical gradient with the original scalar algorithm.
    pub fn try_radial(
        start: Point,
        start_radius: f32,
        end: Point,
        end_radius: f32,
        stops: CheckedStops,
        spread: SpreadMode,
        transform: Transform,
        options: PaintOptions,
        resources: &ResourcesRef,
    ) -> Result<Option<Self>, Error> {
        stops.validate(resources)?;
        Self::from_built(
            super::radial_gradient::build(
                start,
                start_radius,
                end,
                end_radius,
                RasterVec::Paid(stops.values),
                spread,
                transform,
                GeometryControl::request(resources),
            ),
            options,
            resources,
        )
    }
    /// Construct an angular gradient with the original hard stop cases.
    pub fn try_sweep(
        center: Point,
        start_angle: f32,
        end_angle: f32,
        stops: CheckedStops,
        spread: SpreadMode,
        transform: Transform,
        options: PaintOptions,
        resources: &ResourcesRef,
    ) -> Result<Option<Self>, Error> {
        stops.validate(resources)?;
        Self::from_built(
            super::sweep_gradient::build(
                center,
                start_angle,
                end_angle,
                RasterVec::Paid(stops.values),
                spread,
                transform,
                GeometryControl::request(resources),
            ),
            options,
            resources,
        )
    }
    fn from_built(
        candidate: Result<Option<BuiltShader>, Error>,
        options: PaintOptions,
        resources: &ResourcesRef,
    ) -> Result<Option<Self>, Error> {
        let control = GeometryControl::request(resources);
        // Factory parameters and any degenerate stop owner have already retired.
        let candidate = control.finish(candidate)?;
        let shader = match candidate {
            None => return control.finish(Ok(None)),
            Some(BuiltShader::Solid(color)) => CheckedShader::Solid(color),
            Some(BuiltShader::Gradient(base, kind)) => {
                let RasterVec::Paid(values) = base.stops else {
                    return Err(control.invariant());
                };
                CheckedShader::Gradient(PaidShared::try_new(
                    resources,
                    ResourceKind::Scratch,
                    GradientData {
                        values,
                        meta: base.meta,
                        kind,
                    },
                )?)
            }
        };
        control.finish(Ok(Some(Self {
            resources: resources.clone(),
            shader,
            options,
        })))
    }
}
impl<'img> CheckedPaint<'img> {
    /// Create a solid paint without a heap allocation.
    pub fn try_solid(
        color: Color,
        options: PaintOptions,
        resources: &ResourcesRef,
    ) -> Result<Self, Error> {
        resources.checkpoint(1)?;
        Ok(Self {
            resources: resources.clone(),
            shader: CheckedShader::Solid(color),
            options,
        })
    }
    /// Borrow pattern pixels; their producer retains and validates the raster owner.
    pub fn try_pattern(
        pixmap: PixmapRef<'img>,
        spread: SpreadMode,
        quality: FilterQuality,
        opacity: f32,
        transform: Transform,
        options: PaintOptions,
        resources: &ResourcesRef,
    ) -> Result<Self, Error> {
        resources.checkpoint(1)?;
        let Shader::Pattern(pattern) = Pattern::new(pixmap, spread, quality, opacity, transform)
        else {
            unreachable!()
        };
        Ok(Self {
            resources: resources.clone(),
            shader: CheckedShader::Pattern(pattern),
            options,
        })
    }
    /// Compatibility copy of already normalized plain stops; normalization is not repeated.
    pub fn try_from_plain(paint: &Paint<'img>, resources: &ResourcesRef) -> Result<Self, Error> {
        let control = GeometryControl::request(resources);
        control.check(1)?;
        let view = PaintView::plain(paint);
        let shader = match view.shader {
            ShaderView::Solid(color) => CheckedShader::Solid(color),
            ShaderView::Pattern(pattern) => CheckedShader::Pattern(pattern),
            ShaderView::Gradient(base, kind) => {
                let mut values = BudgetVec::new(resources.clone(), ResourceKind::Scratch);
                values.try_reserve(base.stops.len())?;
                for chunk in base.stops.chunks(256) {
                    control.check(chunk.len() as u64)?;
                    for &stop in chunk {
                        values.push_reserved(stop);
                    }
                }
                CheckedShader::Gradient(PaidShared::try_new(
                    resources,
                    ResourceKind::Scratch,
                    GradientData {
                        values,
                        kind,
                        meta: GradientMeta {
                            tile_mode: base.tile_mode,
                            transform: base.transform,
                            points_to_unit: base.points_to_unit,
                            colors_are_opaque: base.colors_are_opaque,
                            has_uniform_stops: base.has_uniform_stops,
                        },
                    },
                )?)
            }
        };
        control.finish(Ok(Self {
            resources: resources.clone(),
            shader,
            options: paint.into(),
        }))
    }
    /// Explicit same-request alias. It never copies stops or creates another allocation fee.
    pub fn try_clone_in(&self, resources: &ResourcesRef) -> Result<Self, Error> {
        self.view(resources)?;
        resources.checkpoint(1)?;
        let shader = match &self.shader {
            CheckedShader::Solid(c) => CheckedShader::Solid(*c),
            CheckedShader::Pattern(p) => CheckedShader::Pattern(*p),
            CheckedShader::Gradient(g) => CheckedShader::Gradient(g.clone()),
        };
        Ok(Self {
            resources: resources.clone(),
            shader,
            options: self.options,
        })
    }
    pub(crate) fn view<'s>(
        &'s self,
        resources: &ResourcesRef,
    ) -> Result<PaintView<'s, 'img, 'static>, Error> {
        let control = GeometryControl::request(resources);
        control.peek_slot()?;
        if !self.resources.is_same_request(resources) {
            return Err(control.invariant());
        }
        let shader = match &self.shader {
            CheckedShader::Solid(c) => ShaderView::Solid(*c),
            CheckedShader::Pattern(p) => ShaderView::Pattern(*p),
            CheckedShader::Gradient(g) => {
                if !g.belongs_to(resources) || !g.values.resources().is_same_request(resources) {
                    return Err(control.invariant());
                }
                let m = g.meta;
                ShaderView::Gradient(
                    GradientView {
                        stops: &g.values,
                        tile_mode: m.tile_mode,
                        transform: m.transform,
                        points_to_unit: m.points_to_unit,
                        colors_are_opaque: m.colors_are_opaque,
                        has_uniform_stops: m.has_uniform_stops,
                        opacity_ops: &[],
                    },
                    g.kind,
                )
            }
        };
        Ok(PaintView {
            shader,
            options: self.options,
            opacity_ops: &[],
            request: Some(&self.resources),
        })
    }
}

#[derive(Clone, Copy)]
enum ShaderView<'s, 'img, 'ops> {
    Solid(Color),
    Gradient(GradientView<'s, 'ops>, GradientKind),
    Pattern(Pattern<'img>),
}
#[derive(Clone, Copy)]
pub(crate) struct PaintView<'s, 'img, 'ops> {
    shader: ShaderView<'s, 'img, 'ops>,
    pub(crate) options: PaintOptions,
    opacity_ops: &'ops [f32],
    request: Option<&'s ResourcesRef>,
}
impl<'s, 'img, 'ops> PaintView<'s, 'img, 'ops> {
    pub(crate) fn plain(paint: &'s Paint<'img>) -> PaintView<'s, 'img, 'static> {
        let shader = match &paint.shader {
            Shader::SolidColor(c) => ShaderView::Solid(*c),
            Shader::Pattern(p) => ShaderView::Pattern(*p),
            Shader::LinearGradient(g) => ShaderView::Gradient(g.base.view(), GradientKind::Linear),
            Shader::RadialGradient(g) => {
                ShaderView::Gradient(g.base.view(), GradientKind::Radial(g.gradient_type))
            }
            Shader::SweepGradient(g) => {
                ShaderView::Gradient(g.base.view(), GradientKind::Sweep(g.t0, g.t1))
            }
        };
        PaintView {
            shader,
            options: paint.into(),
            opacity_ops: &[],
            request: None,
        }
    }
    pub(crate) fn validate(&self, control: GeometryControl) -> Result<(), Error> {
        control.peek_slot()?;
        match (self.request, control.resources()) {
            (None, None) => Ok(()),
            (Some(a), Some(b)) if a.is_same_request(b) => Ok(()),
            (Some(a), None) => Err(a.record(control.invariant())),
            _ => Err(control.invariant()),
        }
    }
    pub(crate) fn with_opacity<'o>(self, ops: &'o [f32]) -> PaintView<'s, 'img, 'o> {
        let shader = match self.shader {
            ShaderView::Solid(c) => ShaderView::Solid(c),
            ShaderView::Pattern(p) => ShaderView::Pattern(p),
            ShaderView::Gradient(g, kind) => ShaderView::Gradient(
                GradientView {
                    stops: g.stops,
                    tile_mode: g.tile_mode,
                    transform: g.transform,
                    points_to_unit: g.points_to_unit,
                    colors_are_opaque: g.colors_are_opaque,
                    has_uniform_stops: g.has_uniform_stops,
                    opacity_ops: ops,
                },
                kind,
            ),
        };
        PaintView {
            shader,
            options: self.options,
            opacity_ops: ops,
            request: self.request,
        }
    }
    pub(crate) fn transform(&mut self, ts: Transform) {
        match &mut self.shader {
            ShaderView::Solid(_) => {}
            ShaderView::Gradient(g, _) => g.transform = g.transform.post_concat(ts),
            ShaderView::Pattern(p) => p.transform = p.transform.post_concat(ts),
        }
    }
    pub(crate) fn clear_source(&mut self) {
        self.shader = ShaderView::Solid(Color::TRANSPARENT);
        self.opacity_ops = &[];
    }
    pub(crate) fn is_solid_color(&self) -> bool {
        matches!(self.shader, ShaderView::Solid(_))
    }
    pub(crate) fn solid_color(&self, control: GeometryControl) -> Result<Option<Color>, Error> {
        if let ShaderView::Solid(mut c) = self.shader {
            for &opacity in self.opacity_ops {
                control.check(1)?;
                c.apply_opacity(opacity);
            }
            Ok(Some(c))
        } else {
            Ok(None)
        }
    }
    pub(crate) fn is_opaque(&self, control: GeometryControl) -> Result<bool, Error> {
        match self.shader {
            ShaderView::Solid(_) => Ok(self.solid_color(control)?.unwrap().is_opaque()),
            ShaderView::Gradient(_, GradientKind::Radial(_)) | ShaderView::Pattern(_) => Ok(false),
            ShaderView::Gradient(g, _) => g.is_opaque(control),
        }
    }
    pub(crate) fn coverage_coordinates_valid(&self) -> bool {
        match self.shader {
            ShaderView::Solid(_) => true,
            ShaderView::Pattern(p) => p.coverage_coordinates_valid(),
            ShaderView::Gradient(g, _) => g.coverage_coordinates_valid(),
        }
    }
    pub(crate) fn pixmap(&self) -> PixmapRef<'img> {
        match self.shader {
            ShaderView::Pattern(p) => p.pixmap,
            _ => PixmapRef::from_bytes(&[0, 0, 0, 0], 1, 1).unwrap(),
        }
    }
    pub(crate) fn push_stages(&self, p: &mut RasterPipelineBuilder) -> Result<bool, Error> {
        let cs = self.options.colorspace;
        match self.shader {
            ShaderView::Solid(_) => {
                p.push_uniform_color(
                    cs.expand_color(self.solid_color(p.control())?.unwrap())
                        .premultiply(),
                );
                Ok(true)
            }
            ShaderView::Pattern(mut pattern) => {
                for &opacity in self.opacity_ops {
                    p.control().check(1)?;
                    pattern.opacity =
                        NormalizedF32::new(pattern.opacity.get() * opacity.bound(0.0, 1.0))
                            .unwrap();
                }
                pattern.push_stages(cs, p)
            }
            ShaderView::Gradient(base, GradientKind::Linear) => {
                base.push_stages(p, cs, &|_| {}, &|_| {})
            }
            ShaderView::Gradient(base, GradientKind::Radial(kind)) => {
                super::radial_gradient::push_stages(base, kind, cs, p)
            }
            ShaderView::Gradient(base, GradientKind::Sweep(t0, t1)) => {
                super::sweep_gradient::push_stages(base, t0, t1, cs, p)
            }
        }
    }
}
