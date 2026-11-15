// Copyright 2019 the Resvg Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT
// Modifications Copyright (c) 2026, Jiaqi (0x7c13) Liu.
// Experimental request-owned error propagation; original license terms retained.

use crate::coverage::{FillCoverage, PreparedBatch};
use crate::render::Context;
#[cfg(test)]
mod paint_tests;

pub fn render(
    path: &usvg::Path,
    blend_mode: tiny_skia::BlendMode,
    ctx: &Context,
    transform: tiny_skia::Transform,
    pixmap: &mut tiny_skia::PixmapMut,
) {
    render_prepared(
        path,
        blend_mode,
        ctx,
        transform,
        pixmap,
        FillCoverage::Unprepared,
    );
}

pub(crate) fn render_prepared(
    path: &usvg::Path,
    blend_mode: tiny_skia::BlendMode,
    ctx: &Context,
    transform: tiny_skia::Transform,
    pixmap: &mut tiny_skia::PixmapMut,
    coverage: FillCoverage<'_>,
) {
    let _ = ctx.with_geometry(|| {
        if !path.is_visible() {
            return None;
        }

        // Validate the fill capability before stroke-first patterns can allocate
        // or enter a recursive renderer. Clip fills retain their own validation.
        if path.fill().is_some() {
            if let Err(error) = crate::coverage::validate_source(path, ctx) {
                ctx.processing.record_resource(error);
                return None;
            }
        }

        if path.paint_order() == usvg::PaintOrder::FillAndStroke {
            fill_path_prepared(path, blend_mode, ctx, transform, pixmap, coverage);
            if ctx.processing.failed() {
                return None;
            }
            stroke_path(path, blend_mode, ctx, transform, pixmap);
        } else {
            stroke_path(path, blend_mode, ctx, transform, pixmap);
            if ctx.processing.failed() {
                return None;
            }
            fill_path_prepared(path, blend_mode, ctx, transform, pixmap, coverage);
        }

        Some(())
    });
}

pub fn fill_path(
    path: &usvg::Path,
    blend_mode: tiny_skia::BlendMode,
    ctx: &Context,
    transform: tiny_skia::Transform,
    pixmap: &mut tiny_skia::PixmapMut,
) -> Option<()> {
    fill_path_prepared(
        path,
        blend_mode,
        ctx,
        transform,
        pixmap,
        FillCoverage::Unprepared,
    )
}

pub(crate) fn fill_path_prepared(
    path: &usvg::Path,
    blend_mode: tiny_skia::BlendMode,
    ctx: &Context,
    transform: tiny_skia::Transform,
    pixmap: &mut tiny_skia::PixmapMut,
    coverage: FillCoverage<'_>,
) -> Option<()> {
    ctx.with_geometry(|| {
        let fill = path.fill()?;
        crate::coverage::validate_source(path, ctx)
            .map_err(|error| ctx.processing.record_resource(error))
            .ok()?;

        // Horizontal and vertical lines cannot be filled. Skip.
        if path.data().bounds().width() == 0.0 || path.data().bounds().height() == 0.0 {
            return None;
        }

        let rule = match fill.rule() {
            usvg::FillRule::NonZero => tiny_skia::FillRule::Winding,
            usvg::FillRule::EvenOdd => tiny_skia::FillRule::EvenOdd,
        };

        let mut pattern_pixmap = None;
        let options = tiny_skia::PaintOptions {
            anti_alias: path.rendering_mode().use_shape_antialiasing(),
            blend_mode,
            ..tiny_skia::PaintOptions::default()
        };
        let mut paint = prepare_paint(
            fill.paint(),
            fill.opacity(),
            options,
            ctx,
            transform,
            &mut pattern_pixmap,
            path,
        )?;

        // Recursive pattern construction above finishes before this single tail
        // allocates A8. Direct siblings arrive with an already completed batch.
        let single;
        let coverage = match coverage {
            FillCoverage::Unprepared => {
                single = PreparedBatch::prepare(
                    std::iter::once(path),
                    ctx,
                    transform,
                    pixmap.width(),
                    pixmap.height(),
                )
                .map_err(|error| ctx.processing.record_resource(error))
                .ok()?;
                single
                    .as_ref()
                    .map_or(FillCoverage::Original, |batch| batch.for_path(path, ctx))
            }
            other => other,
        };
        match coverage {
            FillCoverage::Ready(mask) => {
                let Some(resources) = ctx.processing.resources() else {
                    ctx.processing.record_resource(crate::coverage::invariant());
                    return None;
                };
                let RenderPaint::Checked(paint) = &paint else {
                    ctx.processing.record_resource(crate::coverage::invariant());
                    return None;
                };
                pixmap
                    .try_paint_coverage(mask, paint, transform, resources)
                    .map_err(|error| ctx.processing.record_resource(error))
                    .ok()?;
            }
            FillCoverage::Original => match &mut paint {
                RenderPaint::Checked(paint) => pixmap
                    .try_fill_path_checked(
                        path.data(),
                        paint,
                        rule,
                        transform,
                        None,
                        ctx.processing.resources().unwrap(),
                    )
                    .map_err(|error| ctx.processing.record_resource(error))
                    .ok()?,
                RenderPaint::Plain(paint) => {
                    pixmap.fill_path(path.data(), paint, rule, transform, None)
                }
            },
            FillCoverage::Unprepared => unreachable!(),
        }
        Some(())
    })
}

fn stroke_path(
    path: &usvg::Path,
    blend_mode: tiny_skia::BlendMode,
    ctx: &Context,
    transform: tiny_skia::Transform,
    pixmap: &mut tiny_skia::PixmapMut,
) -> Option<()> {
    ctx.with_geometry(|| {
        let stroke = path.stroke()?;
        let mut pattern_pixmap = None;
        let options = tiny_skia::PaintOptions {
            anti_alias: path.rendering_mode().use_shape_antialiasing(),
            blend_mode,
            ..tiny_skia::PaintOptions::default()
        };
        let mut paint = prepare_paint(
            stroke.paint(),
            stroke.opacity(),
            options,
            ctx,
            transform,
            &mut pattern_pixmap,
            path,
        )?;

        let geometry = match ctx.processing.resources() {
            Some(resources) => stroke
                .try_to_tiny_skia(resources)
                .map_err(|error| ctx.processing.record_resource(error))
                .ok()?,
            None => stroke.to_tiny_skia(),
        };
        match &mut paint {
            RenderPaint::Checked(paint) => pixmap
                .try_stroke_path_checked(
                    path.data(),
                    paint,
                    &geometry,
                    transform,
                    None,
                    ctx.processing.resources().unwrap(),
                )
                .map_err(|error| ctx.processing.record_resource(error))
                .ok()?,
            RenderPaint::Plain(paint) => {
                pixmap.stroke_path(path.data(), paint, &geometry, transform, None)
            }
        }

        Some(())
    })
}

enum RenderPaint<'a> {
    Plain(tiny_skia::Paint<'a>),
    Checked(tiny_skia::CheckedPaint<'a>),
}

fn prepare_paint<'a>(
    source: &usvg::Paint,
    opacity: usvg::Opacity,
    options: tiny_skia::PaintOptions,
    ctx: &Context,
    transform: tiny_skia::Transform,
    pattern_pixmap: &'a mut Option<crate::raster::Raster>,
    path: &usvg::Path,
) -> Option<RenderPaint<'a>> {
    let resources = ctx.processing.resources();
    if let Some(resources) = resources {
        resources
            .checkpoint(0)
            .map_err(|error| ctx.processing.record_resource(error))
            .ok()?;
    }
    let candidate = match source {
        usvg::Paint::Color(c) => {
            let color = tiny_skia::Color::from_rgba8(c.red, c.green, c.blue, opacity.to_u8());
            match resources {
                Some(r) => RenderPaint::Checked(
                    tiny_skia::CheckedPaint::try_solid(color, options, r)
                        .map_err(|e| ctx.processing.record_resource(e))
                        .ok()?,
                ),
                None => {
                    RenderPaint::Plain(plain_paint(tiny_skia::Shader::SolidColor(color), options))
                }
            }
        }
        usvg::Paint::LinearGradient(g) => match resources {
            Some(r) => {
                let (mode, stops) = convert_checked_base_gradient(g, opacity, r)
                    .map_err(|e| ctx.processing.record_resource(e))
                    .ok()?;
                let paint = tiny_skia::CheckedPaint::try_linear(
                    (g.x1(), g.y1()).into(),
                    (g.x2(), g.y2()).into(),
                    stops,
                    mode,
                    g.transform(),
                    options,
                    r,
                )
                .map_err(|e| ctx.processing.record_resource(e))
                .ok()?;
                RenderPaint::Checked(checked_paint(paint, path, ctx)?)
            }
            None => RenderPaint::Plain(plain_paint(
                checked_paint(convert_linear_gradient(g, opacity), path, ctx)?,
                options,
            )),
        },
        usvg::Paint::RadialGradient(g) => match resources {
            Some(r) => {
                let (mode, stops) = convert_checked_base_gradient(g, opacity, r)
                    .map_err(|e| ctx.processing.record_resource(e))
                    .ok()?;
                let paint = tiny_skia::CheckedPaint::try_radial(
                    (g.fx(), g.fy()).into(),
                    g.fr().get(),
                    (g.cx(), g.cy()).into(),
                    g.r().get(),
                    stops,
                    mode,
                    g.transform(),
                    options,
                    r,
                )
                .map_err(|e| ctx.processing.record_resource(e))
                .ok()?;
                RenderPaint::Checked(checked_paint(paint, path, ctx)?)
            }
            None => RenderPaint::Plain(plain_paint(
                checked_paint(convert_radial_gradient(g, opacity), path, ctx)?,
                options,
            )),
        },
        usvg::Paint::Pattern(pattern) => {
            let (pixmap, ts) =
                checked_paint(render_pattern_pixmap(pattern, ctx, transform), path, ctx)?;
            if let Some(r) = resources {
                r.checkpoint(0)
                    .map_err(|e| ctx.processing.record_resource(e))
                    .ok()?;
                if !pixmap
                    .resources()
                    .is_some_and(|owner| owner.is_same_request(r))
                {
                    ctx.processing
                        .record_resource(r.record(crate::coverage::invariant()));
                    return None;
                }
            }
            *pattern_pixmap = Some(pixmap);
            let pixmap = pattern_pixmap.as_ref().unwrap().as_ref();
            match resources {
                Some(r) => RenderPaint::Checked(
                    tiny_skia::CheckedPaint::try_pattern(
                        pixmap,
                        tiny_skia::SpreadMode::Repeat,
                        tiny_skia::FilterQuality::Bicubic,
                        opacity.get(),
                        ts,
                        options,
                        r,
                    )
                    .map_err(|e| ctx.processing.record_resource(e))
                    .ok()?,
                ),
                None => RenderPaint::Plain(plain_paint(
                    tiny_skia::Pattern::new(
                        pixmap,
                        tiny_skia::SpreadMode::Repeat,
                        tiny_skia::FilterQuality::Bicubic,
                        opacity.get(),
                        ts,
                    ),
                    options,
                )),
            }
        }
    };
    if let Some(r) = resources {
        r.checkpoint(0)
            .map_err(|e| ctx.processing.record_resource(e))
            .ok()?;
    }
    Some(candidate)
}
fn plain_paint(
    shader: tiny_skia::Shader<'_>,
    options: tiny_skia::PaintOptions,
) -> tiny_skia::Paint<'_> {
    tiny_skia::Paint {
        shader,
        anti_alias: options.anti_alias,
        blend_mode: options.blend_mode,
        force_hq_pipeline: options.force_hq_pipeline,
        colorspace: options.colorspace,
    }
}
fn convert_checked_base_gradient(
    gradient: &usvg::BaseGradient,
    opacity: usvg::Opacity,
    resources: &markdown_svg_budget::ResourcesRef,
) -> Result<(tiny_skia::SpreadMode, tiny_skia::CheckedStops), markdown_svg_budget::Error> {
    resources.checkpoint(0)?;
    let mode = gradient_spread(gradient);
    let mut points = tiny_skia::CheckedStops::new(resources);
    points.try_reserve(gradient.stops().len())?;
    for stop in gradient.stops() {
        resources.checkpoint(1)?;
        points.try_push(convert_stop(stop, opacity))?;
    }
    Ok((mode, points))
}
fn gradient_spread(gradient: &usvg::BaseGradient) -> tiny_skia::SpreadMode {
    match gradient.spread_method() {
        usvg::SpreadMethod::Pad => tiny_skia::SpreadMode::Pad,
        usvg::SpreadMethod::Reflect => tiny_skia::SpreadMode::Reflect,
        usvg::SpreadMethod::Repeat => tiny_skia::SpreadMode::Repeat,
    }
}
fn convert_stop(stop: &usvg::Stop, opacity: usvg::Opacity) -> tiny_skia::GradientStop {
    let alpha = stop.opacity() * opacity;
    let color = tiny_skia::Color::from_rgba8(
        stop.color().red,
        stop.color().green,
        stop.color().blue,
        alpha.to_u8(),
    );
    tiny_skia::GradientStop::new(stop.offset().get(), color)
}

fn checked_paint<T>(value: Option<T>, path: &usvg::Path, ctx: &Context) -> Option<T> {
    if value.is_none() && crate::coverage::has_source(path, ctx) {
        ctx.processing.record_resource(crate::coverage::invariant());
    }
    value
}

fn convert_linear_gradient(
    gradient: &usvg::LinearGradient,
    opacity: usvg::Opacity,
) -> Option<tiny_skia::Shader<'static>> {
    let (mode, points) = convert_base_gradient(gradient, opacity)?;

    let shader = tiny_skia::LinearGradient::new(
        (gradient.x1(), gradient.y1()).into(),
        (gradient.x2(), gradient.y2()).into(),
        points,
        mode,
        gradient.transform(),
    )?;

    Some(shader)
}

fn convert_radial_gradient(
    gradient: &usvg::RadialGradient,
    opacity: usvg::Opacity,
) -> Option<tiny_skia::Shader<'static>> {
    let (mode, points) = convert_base_gradient(gradient, opacity)?;

    let shader = tiny_skia::RadialGradient::new(
        (gradient.fx(), gradient.fy()).into(),
        gradient.fr().get(),
        (gradient.cx(), gradient.cy()).into(),
        gradient.r().get(),
        points,
        mode,
        gradient.transform(),
    )?;

    Some(shader)
}

fn convert_base_gradient(
    gradient: &usvg::BaseGradient,
    opacity: usvg::Opacity,
) -> Option<(tiny_skia::SpreadMode, Vec<tiny_skia::GradientStop>)> {
    let mode = gradient_spread(gradient);
    let mut points = Vec::with_capacity(gradient.stops().len());
    for stop in gradient.stops() {
        points.push(convert_stop(stop, opacity));
    }

    Some((mode, points))
}

fn render_pattern_pixmap(
    pattern: &usvg::Pattern,
    ctx: &Context,
    transform: tiny_skia::Transform,
) -> Option<(crate::raster::Raster, tiny_skia::Transform)> {
    let (sx, sy) = {
        let ts2 = transform.pre_concat(pattern.transform());
        ts2.get_scale()
    };

    let rect = pattern.rect();
    let img_size = tiny_skia::IntSize::from_wh(
        (rect.width() * sx).round() as u32,
        (rect.height() * sy).round() as u32,
    )?;
    let mut pixmap = crate::raster::Raster::try_create(
        img_size.width(),
        img_size.height(),
        ctx.processing.resources(),
    )
    .map_err(|error| ctx.processing.record_resource(error))
    .ok()?;

    let transform = tiny_skia::Transform::from_scale(sx, sy);
    crate::render::render_nodes(pattern.root(), ctx, transform, &mut pixmap.as_mut());
    if ctx.processing.failed() {
        return None;
    }

    let mut ts = tiny_skia::Transform::default();
    ts = ts.pre_concat(pattern.transform());
    ts = ts.pre_translate(rect.x(), rect.y());
    ts = ts.pre_scale(1.0 / sx, 1.0 / sy);

    Some((pixmap, ts))
}
