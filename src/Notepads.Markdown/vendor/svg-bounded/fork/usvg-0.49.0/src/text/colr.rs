// Copyright 2024 the Resvg Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT
// Modifications Copyright (c) 2026, Jiaqi (0x7c13) Liu.

use super::colr_storage::{ColrPath, ColrWriter};
use super::transform::{skrifa_to_tsp_transform, tsp_to_skrifa_transform};
use crate::parser::OptionLog;
use crate::tree::owner::{self, PolicyValue, Result, TreeVec};
use skrifa::instance::LocationRef;
use skrifa::prelude::Size;
use skrifa::raw::types::Point;
use skrifa::{
    color::{Brush, ColorStop, Extend, Transform},
    outline::DrawSettings,
    MetadataProvider,
};
use svgtypes::Color;

impl PolicyValue for Transform {
    fn validate_policy(&self, p: &crate::ProcessingContext) -> Result<()> {
        owner::check(p, 0)
    }
}
struct Builder<'a, 'p> {
    path: &'a mut ColrPath<'p>,
    min_x: f32,
    min_y: f32,
    max_x: f32,
    max_y: f32,
}
impl<'a, 'p> Builder<'a, 'p> {
    fn new(path: &'a mut ColrPath<'p>) -> Self {
        Self {
            path,
            min_x: f32::MAX,
            min_y: f32::MAX,
            max_x: f32::MIN,
            max_y: f32::MIN,
        }
    }
    fn add_point(&mut self, x: f32, y: f32) {
        self.min_x = self.min_x.min(x);
        self.min_y = self.min_y.min(y);
        self.max_x = self.max_x.max(x);
        self.max_y = self.max_y.max(y);
    }
    fn bounds(&self) -> Option<tiny_skia_path::Rect> {
        if self.min_x <= self.max_x && self.min_y <= self.max_y {
            tiny_skia_path::Rect::from_ltrb(self.min_x, self.min_y, self.max_x, self.max_y)
        } else {
            None
        }
    }
    fn finish(&mut self) -> Result<()> {
        self.path.finish_outline()
    }
}
impl skrifa::outline::OutlinePen for Builder<'_, '_> {
    fn move_to(&mut self, x: f32, y: f32) {
        if owner::check(self.path.processing, 1).is_err() {
            return;
        }
        self.add_point(x, y);
        let _ = self.path.write_fmt(format_args!("M {} {} ", x, y));
    }
    fn line_to(&mut self, x: f32, y: f32) {
        if owner::check(self.path.processing, 1).is_err() {
            return;
        }
        self.add_point(x, y);
        let _ = self.path.write_fmt(format_args!("L {} {} ", x, y));
    }
    fn quad_to(&mut self, cx0: f32, cy0: f32, x: f32, y: f32) {
        if owner::check(self.path.processing, 1).is_err() {
            return;
        }
        self.add_point(cx0, cy0);
        self.add_point(x, y);
        let _ = self
            .path
            .write_fmt(format_args!("Q {} {} {} {} ", cx0, cy0, x, y));
    }
    fn curve_to(&mut self, cx0: f32, cy0: f32, cx1: f32, cy1: f32, x: f32, y: f32) {
        if owner::check(self.path.processing, 1).is_err() {
            return;
        }
        self.add_point(cx0, cy0);
        self.add_point(cx1, cy1);
        self.add_point(x, y);
        let _ = self.path.write_fmt(format_args!(
            "C {} {} {} {} {} {} ",
            cx0, cy0, cx1, cy1, x, y
        ));
    }
    fn close(&mut self) {
        if owner::check(self.path.processing, 1).is_err() {
            return;
        }
        let _ = self.path.push_str("Z ");
    }
}
trait XmlWriterExt {
    fn write_color_attribute(&mut self, name: &str, color: Color) -> Result<()>;
    fn write_transform_attribute(&mut self, name: &str, ts: Transform) -> Result<()>;
    fn write_spread_method_attribute(&mut self, method: Extend) -> Result<()>;
}
impl XmlWriterExt for ColrWriter<'_> {
    fn write_color_attribute(&mut self, name: &str, color: Color) -> Result<()> {
        self.write_attribute_fmt(
            name,
            format_args!("rgb({}, {}, {})", color.red, color.green, color.blue),
        )
    }
    fn write_transform_attribute(&mut self, name: &str, ts: Transform) -> Result<()> {
        owner::check(self.processing, 0)?;
        if ts == Transform::default() {
            return Ok(());
        }
        self.write_attribute_fmt(
            name,
            format_args!(
                "matrix({} {} {} {} {} {})",
                ts.xx, ts.yx, ts.xy, ts.yy, ts.dx, ts.dy
            ),
        )
    }
    fn write_spread_method_attribute(&mut self, extend: Extend) -> Result<()> {
        owner::check(self.processing, 0)?;
        let value = match extend {
            Extend::Pad => "pad",
            Extend::Repeat => "repeat",
            Extend::Reflect => "reflect",
            Extend::Unknown => return Ok(()),
        };
        self.write_attribute("spreadMethod", value)
    }
}
// The original COLR-to-SVG algorithm, with the request interpreter and its
// writer/path/stack owners using the same processing context.
pub(crate) struct GlyphPainter<'f, 'a, 'p> {
    pub(crate) font: &'a skrifa::FontRef<'f>,
    pub(crate) location: LocationRef<'a>,
    pub(crate) svg: &'a mut ColrWriter<'p>,
    pub(crate) path_buf: &'a mut ColrPath<'p>,
    pub(crate) gradient_index: usize,
    pub(crate) clip_path_index: usize,
    pub(crate) foreground_color: Color,
    pub(crate) transform: Transform,
    pub(crate) outline_transform: Transform,
    pub(crate) transforms_stack: TreeVec<Transform>,
    pub(crate) clip_stack: TreeVec<Option<tiny_skia_path::Rect>>,
    pub(crate) aborted: bool,
}
impl GlyphPainter<'_, '_, '_> {
    fn ready(&self) -> Result<()> {
        owner::check(self.svg.processing, 0)
    }
    fn write_gradient_stops(&mut self, stops: &[ColorStop]) -> Result<()> {
        for stop in stops {
            owner::check(self.svg.processing, 1)?;
            let color = self.palette_index_to_color(stop.palette_index, stop.alpha)?;
            self.svg.start_element("stop")?;
            self.svg.write_attribute("offset", &stop.offset)?;
            self.svg.write_color_attribute("stop-color", color)?;
            let opacity = f32::from(color.alpha) / 255.0;
            self.svg.write_attribute("stop-opacity", &opacity)?;
            self.svg.end_element()?;
        }
        self.ready()
    }
    fn paint_solid(&mut self, color: Color) -> Result<()> {
        self.ready()?;
        self.svg.start_element("path")?;
        self.svg.write_color_attribute("fill", color)?;
        let opacity = f32::from(color.alpha) / 255.0;
        self.svg.write_attribute("fill-opacity", &opacity)?;
        self.svg
            .write_transform_attribute("transform", self.outline_transform)?;
        self.svg.write_attribute("d", self.path_buf.as_str())?;
        self.svg.end_element()
    }
    fn paint_linear_gradient(
        &mut self,
        p0: Point<f32>,
        p1: Point<f32>,
        stops: &[ColorStop],
        extend: Extend,
    ) -> Result<()> {
        self.ready()?;
        let id = self.gradient_index;
        self.gradient_index += 1;
        let transform = paint_transform(self.outline_transform, self.transform);
        self.svg.start_element("linearGradient")?;
        self.svg
            .write_attribute_fmt("id", format_args!("lg{}", id))?;
        self.svg.write_attribute("x1", &p0.x)?;
        self.svg.write_attribute("y1", &p0.y)?;
        self.svg.write_attribute("x2", &p1.x)?;
        self.svg.write_attribute("y2", &p1.y)?;
        self.svg
            .write_attribute("gradientUnits", "userSpaceOnUse")?;
        self.svg.write_spread_method_attribute(extend)?;
        self.svg
            .write_transform_attribute("gradientTransform", transform)?;
        self.write_gradient_stops(stops)?;
        self.svg.end_element()?;
        self.svg.start_element("path")?;
        self.svg
            .write_attribute_fmt("fill", format_args!("url(#lg{})", id))?;
        self.svg
            .write_transform_attribute("transform", self.outline_transform)?;
        self.svg.write_attribute("d", self.path_buf.as_str())?;
        self.svg.end_element()
    }
    fn paint_radial_gradient(
        &mut self,
        c0: Point<f32>,
        r0: f32,
        c1: Point<f32>,
        r1: f32,
        stops: &[ColorStop],
        extend: Extend,
    ) -> Result<()> {
        self.ready()?;
        let id = self.gradient_index;
        self.gradient_index += 1;
        let transform = paint_transform(self.outline_transform, self.transform);
        self.svg.start_element("radialGradient")?;
        self.svg
            .write_attribute_fmt("id", format_args!("rg{}", id))?;
        self.svg.write_attribute("cx", &c1.x)?;
        self.svg.write_attribute("cy", &c1.y)?;
        self.svg.write_attribute("r", &r1)?;
        self.svg.write_attribute("fr", &r0)?;
        self.svg.write_attribute("fx", &c0.x)?;
        self.svg.write_attribute("fy", &c0.y)?;
        self.svg
            .write_attribute("gradientUnits", "userSpaceOnUse")?;
        self.svg.write_spread_method_attribute(extend)?;
        self.svg
            .write_transform_attribute("gradientTransform", transform)?;
        self.write_gradient_stops(stops)?;
        self.svg.end_element()?;
        self.svg.start_element("path")?;
        self.svg
            .write_attribute_fmt("fill", format_args!("url(#rg{})", id))?;
        self.svg
            .write_transform_attribute("transform", self.outline_transform)?;
        self.svg.write_attribute("d", self.path_buf.as_str())?;
        self.svg.end_element()
    }
    fn paint_sweep_gradient(
        &mut self,
        _c0: Point<f32>,
        _start: f32,
        _end: f32,
        _stops: &[ColorStop],
        _extend: Extend,
    ) -> Result<()> {
        self.ready()?;
        if self.svg.processing.resources().is_some() {
            return Err(self
                .svg
                .processing
                .record_resource(markdown_svg_budget::Error::new(
                    markdown_svg_budget::FailureKind::Unsupported,
                    markdown_svg_budget::LimitKind::None,
                )));
        }
        Ok(())
    }
    fn outline_glyph(&mut self, id: skrifa::GlyphId) -> Result<Option<tiny_skia_path::Rect>> {
        self.ready()?;
        self.path_buf.clear()?;
        let mut bounds = None;
        let p = self.svg.processing;
        let mut builder = Builder::new(self.path_buf);
        let settings = DrawSettings::unhinted(Size::unscaled(), self.location);
        let draw = match p.resources() {
            Some(resources) => {
                let outlines = skrifa::request::Outlines::try_new(self.font, resources)?;
                let result = match outlines.try_get(id)? {
                    Some(outline) => Some(outline.draw(settings, &mut builder)?),
                    None => None,
                };
                drop(outlines);
                owner::check(p, 0)?;
                result
            }
            None => self
                .font
                .outline_glyphs()
                .get(id)
                .map(|outline| outline.draw(settings, &mut builder)),
        };
        let outlined = if let Some(result) = draw {
            owner::check(p, 0)?;
            if result.is_ok() {
                builder.finish()?;
                bounds = builder.bounds();
            }
            result.is_ok()
        } else {
            false
        };
        owner::check(p, 0)?;
        if !outlined {
            self.path_buf.clear()?;
        }
        self.outline_transform = self.transform;
        Ok(bounds)
    }
    fn paint_brush(&mut self, brush: Brush<'_>) -> Result<()> {
        self.ready()?;
        match brush {
            Brush::Solid {
                palette_index,
                alpha,
            } => {
                let color = self.palette_index_to_color(palette_index, alpha)?;
                self.paint_solid(color)
            }
            Brush::LinearGradient {
                p0,
                p1,
                color_stops,
                extend,
            } => self.paint_linear_gradient(p0, p1, color_stops, extend),
            Brush::RadialGradient {
                c0,
                r0,
                c1,
                r1,
                color_stops,
                extend,
            } => self.paint_radial_gradient(c0, r0, c1, r1, color_stops, extend),
            Brush::SweepGradient {
                c0,
                start_angle,
                end_angle,
                color_stops,
                extend,
            } => self.paint_sweep_gradient(c0, start_angle, end_angle, color_stops, extend),
        }
    }
    fn palette_index_to_color(&self, index: u16, alpha: f32) -> Result<Color> {
        self.ready()?;
        let mut color = if index == u16::MAX {
            self.foreground_color
        } else {
            let color = match self.svg.processing.resources() {
                Some(r) => skrifa::request::ColorPalettes::try_new(self.font, r)?
                    .try_get(0)?
                    .and_then(|palette| palette.colors().get(index as usize).copied()),
                None => self
                    .font
                    .color_palettes()
                    .get(0)
                    .and_then(|palette| palette.colors().get(index as usize).copied()),
            };
            color
                .map(|color| Color {
                    red: color.red,
                    blue: color.blue,
                    green: color.green,
                    alpha: color.alpha,
                })
                .unwrap_or(self.foreground_color)
        };
        self.ready()?;
        color.alpha = ((color.alpha as f32) * alpha) as u8;
        Ok(color)
    }
    fn try_push_transform(&mut self, transform: Transform) -> Result<()> {
        self.ready()?;
        self.transforms_stack
            .try_push(self.transform, self.svg.processing)?;
        self.transform = self.transform * transform;
        Ok(())
    }
    fn try_pop_transform(&mut self) -> Result<()> {
        self.ready()?;
        if let Some(ts) = self.transforms_stack.pop() {
            self.transform = ts;
        }
        self.ready()
    }
    fn try_fill_glyph(
        &mut self,
        id: skrifa::GlyphId,
        transform: Option<Transform>,
        brush: Brush<'_>,
    ) -> Result<()> {
        self.ready()?;
        self.outline_glyph(id)?;
        if let Some(transform) = transform {
            self.try_push_transform(transform)?;
            self.paint_brush(brush)?;
            self.try_pop_transform()
        } else {
            self.paint_brush(brush)
        }
    }
    fn try_push_clip_glyph(&mut self, id: skrifa::GlyphId) -> Result<()> {
        let bounds = self.outline_glyph(id)?;
        // Disjoint borrows serialize the existing path directly; no path String clone.
        write_clip_path(
            self.svg,
            &mut self.clip_path_index,
            self.outline_transform,
            self.path_buf.as_str(),
        )?;
        let root =
            bounds.and_then(|b| map_rect(b, skrifa_to_tsp_transform(self.outline_transform)));
        self.clip_stack.try_push(root, self.svg.processing)
    }
    fn try_push_clip_box(&mut self, b: skrifa::raw::types::BoundingBox<f32>) -> Result<()> {
        self.ready()?;
        let (x_min, x_max, y_min, y_max) = (b.x_min, b.x_max, b.y_min, b.y_max);
        self.path_buf.clear()?;
        self.path_buf.write_fmt(format_args!(
            "M {} {} L {} {} L {} {} L {} {} Z",
            x_min, y_min, x_max, y_min, x_max, y_max, x_min, y_max
        ))?;
        self.outline_transform = self.transform;
        write_clip_path(
            self.svg,
            &mut self.clip_path_index,
            self.outline_transform,
            self.path_buf.as_str(),
        )?;
        let bounds = tiny_skia_path::Rect::from_ltrb(x_min, y_min, x_max, y_max)
            .and_then(|b| map_rect(b, skrifa_to_tsp_transform(self.outline_transform)));
        self.clip_stack.try_push(bounds, self.svg.processing)
    }
    fn try_pop_clip(&mut self) -> Result<()> {
        self.ready()?;
        self.svg.end_element()?;
        self.clip_stack.pop();
        self.ready()
    }
    fn try_fill(&mut self, brush: Brush<'_>) -> Result<()> {
        self.ready()?;
        if self.clip_stack.is_empty() {
            log::warn!("Unclipped COLR fills are not supported.");
            return Ok(());
        }
        let mut region = None;
        for bounds in &self.clip_stack {
            owner::check(self.svg.processing, 1)?;
            let Some(bounds) = bounds else {
                return Ok(());
            };
            region = Some(match region {
                Some(region) => match intersect_rects(region, *bounds) {
                    Some(r) => r,
                    None => return Ok(()),
                },
                None => *bounds,
            });
        }
        let Some(region) = region else {
            return Ok(());
        };
        self.path_buf.clear()?;
        self.path_buf.write_fmt(format_args!(
            "M {} {} L {} {} L {} {} L {} {} Z",
            region.left(),
            region.top(),
            region.right(),
            region.top(),
            region.right(),
            region.bottom(),
            region.left(),
            region.bottom()
        ))?;
        self.outline_transform = Transform::default();
        self.paint_brush(brush)
    }
    fn try_push_layer(&mut self, mode: skrifa::color::CompositeMode) -> Result<()> {
        self.ready()?;
        use skrifa::color::CompositeMode;
        let mode = match mode {
            CompositeMode::SrcOver => "normal",
            CompositeMode::Screen => "screen",
            CompositeMode::Overlay => "overlay",
            CompositeMode::Darken => "darken",
            CompositeMode::Lighten => "lighten",
            CompositeMode::ColorDodge => "color-dodge",
            CompositeMode::ColorBurn => "color-burn",
            CompositeMode::HardLight => "hard-light",
            CompositeMode::SoftLight => "soft-light",
            CompositeMode::Difference => "difference",
            CompositeMode::Exclusion => "exclusion",
            CompositeMode::Multiply => "multiply",
            CompositeMode::HslHue => "hue",
            CompositeMode::HslSaturation => "saturation",
            CompositeMode::HslColor => "color",
            CompositeMode::HslLuminosity => "luminosity",
            _ => "normal",
        };
        self.svg.start_element("g")?;
        self.svg.write_attribute_fmt(
            "style",
            format_args!("mix-blend-mode: {}; isolation: isolate", mode),
        )
    }
    fn try_pop_layer(&mut self) -> Result<()> {
        self.ready()?;
        self.svg.end_element()
    }
}

/// Only the checked branch can construct this adapter. Ordinary callers keep
/// their existing ColorPainter override/default dispatch.
pub(crate) struct RequestGlyphPainter<'f, 'a, 'p, 'w, 'r> {
    inner: &'w mut GlyphPainter<'f, 'a, 'p>,
    resources: &'r markdown_svg_budget::ResourcesRef,
}
impl<'f, 'a, 'p> GlyphPainter<'f, 'a, 'p> {
    pub(crate) fn request<'w, 'r>(
        &'w mut self,
        resources: &'r markdown_svg_budget::ResourcesRef,
    ) -> Result<RequestGlyphPainter<'f, 'a, 'p, 'w, 'r>> {
        owner::validate_owner(self.svg.processing, Some(resources))?;
        Ok(RequestGlyphPainter {
            inner: self,
            resources,
        })
    }
}
impl skrifa::request::ColorPainter for RequestGlyphPainter<'_, '_, '_, '_, '_> {
    fn resources(&self) -> &markdown_svg_budget::ResourcesRef {
        self.resources
    }
    fn push_transform(&mut self, ts: Transform) -> Result<()> {
        self.inner.try_push_transform(ts)
    }
    fn pop_transform(&mut self) -> Result<()> {
        self.inner.try_pop_transform()
    }
    fn push_clip_glyph(&mut self, id: skrifa::GlyphId) -> Result<()> {
        self.inner.try_push_clip_glyph(id)
    }
    fn push_clip_box(&mut self, b: skrifa::raw::types::BoundingBox<f32>) -> Result<()> {
        self.inner.try_push_clip_box(b)
    }
    fn pop_clip(&mut self) -> Result<()> {
        self.inner.try_pop_clip()
    }
    fn fill(&mut self, brush: Brush<'_>) -> Result<()> {
        self.inner.try_fill(brush)
    }
    fn fill_glyph(
        &mut self,
        id: skrifa::GlyphId,
        ts: Option<Transform>,
        brush: Brush<'_>,
    ) -> Result<()> {
        self.inner.try_fill_glyph(id, ts, brush)
    }
    fn push_layer(&mut self, mode: skrifa::color::CompositeMode) -> Result<()> {
        self.inner.try_push_layer(mode)
    }
    fn pop_layer_with_mode(&mut self, _: skrifa::color::CompositeMode) -> Result<()> {
        self.inner.try_pop_layer()
    }
    fn abort_color_glyph(&mut self) {
        // The owning caller drops stacks, writer and path; no closing XML or
        // normal layer pop may turn the discarded candidate into new output.
        self.inner.aborted = true;
    }
}
fn write_clip_path(
    svg: &mut ColrWriter<'_>,
    index: &mut usize,
    transform: Transform,
    path: &str,
) -> Result<()> {
    owner::check(svg.processing, 0)?;
    let id = *index;
    *index += 1;
    svg.start_element("clipPath")?;
    svg.write_attribute_fmt("id", format_args!("cp{}", id))?;
    svg.start_element("path")?;
    svg.write_transform_attribute("transform", transform)?;
    svg.write_attribute("d", path)?;
    svg.end_element()?;
    svg.end_element()?;
    svg.start_element("g")?;
    svg.write_attribute_fmt("clip-path", format_args!("url(#cp{})", id))
}
impl skrifa::color::ColorPainter for GlyphPainter<'_, '_, '_> {
    fn push_transform(&mut self, ts: Transform) {
        let _ = self.try_push_transform(ts);
    }
    fn pop_transform(&mut self) {
        let _ = self.try_pop_transform();
    }
    fn fill_glyph(&mut self, id: skrifa::GlyphId, ts: Option<Transform>, brush: Brush<'_>) {
        let _ = self.try_fill_glyph(id, ts, brush);
    }
    fn push_clip_glyph(&mut self, id: skrifa::GlyphId) {
        let _ = self.try_push_clip_glyph(id);
    }
    fn push_clip_box(&mut self, b: skrifa::raw::types::BoundingBox<f32>) {
        let _ = self.try_push_clip_box(b);
    }
    fn pop_clip(&mut self) {
        let _ = self.try_pop_clip();
    }
    fn fill(&mut self, brush: Brush<'_>) {
        let _ = self.try_fill(brush);
    }
    fn push_layer(&mut self, mode: skrifa::color::CompositeMode) {
        let _ = self.try_push_layer(mode);
    }
    fn pop_layer(&mut self) {
        let _ = self.try_pop_layer();
    }
}

fn paint_transform(outline_transform: Transform, transform: Transform) -> Transform {
    let outline_transform = skrifa_to_tsp_transform(outline_transform);
    let gradient_transform = skrifa_to_tsp_transform(transform);

    let gradient_transform = outline_transform
        .invert()
        .log_none(|| log::warn!("Failed to calculate transform for gradient in glyph."))
        .unwrap_or_default()
        .pre_concat(gradient_transform);

    tsp_to_skrifa_transform(gradient_transform)
}

/// Returns the bounding box of `rect` transformed by `ts`.
fn map_rect(
    rect: tiny_skia_path::Rect,
    ts: tiny_skia_path::Transform,
) -> Option<tiny_skia_path::Rect> {
    let mut points = [
        tiny_skia_path::Point::from_xy(rect.left(), rect.top()),
        tiny_skia_path::Point::from_xy(rect.right(), rect.top()),
        tiny_skia_path::Point::from_xy(rect.left(), rect.bottom()),
        tiny_skia_path::Point::from_xy(rect.right(), rect.bottom()),
    ];
    ts.map_points(&mut points);
    let min_x = points.iter().map(|p| p.x).fold(f32::MAX, f32::min);
    let min_y = points.iter().map(|p| p.y).fold(f32::MAX, f32::min);
    let max_x = points.iter().map(|p| p.x).fold(f32::MIN, f32::max);
    let max_y = points.iter().map(|p| p.y).fold(f32::MIN, f32::max);
    tiny_skia_path::Rect::from_ltrb(min_x, min_y, max_x, max_y)
}

/// Returns the intersection of two rects, or `None` when they do not overlap.
fn intersect_rects(
    a: tiny_skia_path::Rect,
    b: tiny_skia_path::Rect,
) -> Option<tiny_skia_path::Rect> {
    tiny_skia_path::Rect::from_ltrb(
        a.left().max(b.left()),
        a.top().max(b.top()),
        a.right().min(b.right()),
        a.bottom().min(b.bottom()),
    )
}
