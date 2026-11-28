// Modifications Copyright (c) 2026, Jiaqi (0x7c13) Liu.
//! COLR table instance.

use read_fonts::{
    tables::{
        colr::*,
        variations::{
            DeltaSetIndex, DeltaSetIndexMap, FloatItemDelta, FloatItemDeltaTarget,
            ItemVariationStore,
        },
    },
    types::{BoundingBox, F2Dot14, GlyphId16, Point},
};

use crate::control::{ordinary, Ordinary};
use core::ops::{Deref, Range};
use markdown_svg_budget::WorkControl;
use read_fonts::control::ReadCtl;

/// Unique paint identifier used for detecting cycles in the paint graph.
pub type PaintId = usize;

/// Combination of a `COLR` table and a location in variation space for
/// resolving paints.
///
/// See [`try_resolve_paint`], [`ColorStops::resolve`] and [`try_resolve_clip_box`].
#[derive(Clone)]
pub struct ColrInstance<'f, 'c> {
    colr: Colr<'f>,
    index_map: Option<DeltaSetIndexMap<'f>>,
    var_store: Option<ItemVariationStore<'f>>,
    coords: &'c [F2Dot14],
}

impl<'f, 'c> ColrInstance<'f, 'c> {
    pub fn new(colr: Colr<'f>, coords: &'c [F2Dot14]) -> Self {
        ordinary(Self::try_new(colr, coords, &mut Ordinary::new()))
    }
    pub(crate) fn try_new<C: WorkControl>(
        colr: Colr<'f>,
        coords: &'c [F2Dot14],
        cx: &mut C,
    ) -> Result<Self, C::Error> {
        cx.checkpoint(0)?;
        let index_map = colr.try_var_index_map(cx)?.and_then(Result::ok);
        let var_store = colr.try_item_variation_store(cx)?.and_then(Result::ok);
        cx.checkpoint(0)?;
        Ok(Self {
            colr,
            coords,
            index_map,
            var_store,
        })
    }
    #[cfg(test)]
    fn var_deltas<const N: usize>(&self, base: u32) -> [FloatItemDelta; N] {
        ordinary(self.try_var_deltas::<N, _>(base, &mut Ordinary::new()))
    }
    pub(crate) fn try_var_deltas<const N: usize, C: WorkControl>(
        &self,
        var_index_base: u32,
        cx: &mut C,
    ) -> Result<[FloatItemDelta; N], C::Error> {
        cx.checkpoint(0)?;
        const NO_VARIATION_DELTAS: u32 = 0xFFFFFFFF;
        let mut deltas = [FloatItemDelta::ZERO; N];
        if self.coords.is_empty()
            || self.var_store.is_none()
            || var_index_base == NO_VARIATION_DELTAS
        {
            return Ok(deltas);
        }
        let actual_count = ((u32::MAX - var_index_base) as usize).min(N);
        let var_store = self.var_store.as_ref().unwrap();
        if let Some(index_map) = self.index_map.as_ref() {
            for (i, delta) in deltas.iter_mut().enumerate().take(actual_count) {
                cx.checkpoint(1)?;
                let var_index = var_index_base + i as u32;
                if let Ok(delta_ix) = index_map.try_get(var_index, cx)? {
                    *delta = var_store
                        .try_compute_float_delta(delta_ix, self.coords, cx)?
                        .unwrap_or_default();
                }
            }
        } else {
            for (i, delta) in deltas.iter_mut().enumerate().take(actual_count) {
                cx.checkpoint(1)?;
                let var_index = var_index_base + i as u32;
                let delta_ix = DeltaSetIndex {
                    outer: 0,
                    inner: var_index as u16,
                };
                *delta = var_store
                    .try_compute_float_delta(delta_ix, self.coords, cx)?
                    .unwrap_or_default();
            }
        }
        cx.checkpoint(0)?;
        Ok(deltas)
    }
    pub(crate) fn try_v0_layer<C: WorkControl>(
        &self,
        index: usize,
        cx: &mut C,
    ) -> ReadCtl<(GlyphId16, u16), C> {
        self.colr.try_v0_layer(index, cx)
    }
    pub(crate) fn try_v1_layer<C: WorkControl>(
        &self,
        index: usize,
        cx: &mut C,
    ) -> ReadCtl<(Paint<'f>, PaintId), C> {
        self.colr.try_v1_layer(index, cx)
    }
    pub(crate) fn try_v1_base_glyph<C: WorkControl>(
        &self,
        glyph: read_fonts::types::GlyphId,
        cx: &mut C,
    ) -> ReadCtl<Option<(Paint<'f>, PaintId)>, C> {
        self.colr.try_v1_base_glyph(glyph, cx)
    }
    pub(crate) fn try_v1_clip_box<C: WorkControl>(
        &self,
        glyph: read_fonts::types::GlyphId,
        cx: &mut C,
    ) -> ReadCtl<Option<ClipBox<'f>>, C> {
        self.colr.try_v1_clip_box(glyph, cx)
    }
}
impl<'f> Deref for ColrInstance<'f, '_> {
    type Target = Colr<'f>;
    fn deref(&self) -> &Self::Target {
        &self.colr
    }
}

/// Resolves a clip box, applying variation deltas using the given
/// instance.
pub(crate) fn try_resolve_clip_box<C: WorkControl>(
    instance: &ColrInstance,
    clip_box: &ClipBox,
    cx: &mut C,
) -> Result<BoundingBox<f32>, C::Error> {
    cx.checkpoint(1)?;
    Ok(match clip_box {
        ClipBox::Format1(cbox) => BoundingBox {
            x_min: cbox.x_min().to_i16() as f32,
            y_min: cbox.y_min().to_i16() as f32,
            x_max: cbox.x_max().to_i16() as f32,
            y_max: cbox.y_max().to_i16() as f32,
        },
        ClipBox::Format2(cbox) => {
            let deltas = instance.try_var_deltas::<4, _>(cbox.var_index_base(), cx)?;
            BoundingBox {
                x_min: cbox.x_min().apply_float_delta(deltas[0]),
                y_min: cbox.y_min().apply_float_delta(deltas[1]),
                x_max: cbox.x_max().apply_float_delta(deltas[2]),
                y_max: cbox.y_max().apply_float_delta(deltas[3]),
            }
        }
    })
}

/// Simplified version of a [`ColorStop`] or [`VarColorStop`] with applied
/// variation deltas.
#[derive(Clone, Debug)]
pub struct ResolvedColorStop {
    pub offset: f32,
    pub palette_index: u16,
    pub alpha: f32,
}

/// Collection of [`ColorStop`] or [`VarColorStop`].
// Note: only one of these fields is used at any given time, but this structure
// was chosen over the obvious enum approach for simplicity in generating a
// single concrete type for the `impl Iterator` return type of the `resolve`
// method.
#[derive(Clone)]
pub struct ColorStops<'a> {
    stops: &'a [ColorStop],
    var_stops: &'a [VarColorStop],
}

impl ColorStops<'_> {
    pub fn len(&self) -> usize {
        self.stops.len() + self.var_stops.len()
    }

    pub fn is_empty(&self) -> bool {
        self.stops.is_empty() && self.var_stops.is_empty()
    }
}

impl<'a> From<ColorLine<'a>> for ColorStops<'a> {
    fn from(value: ColorLine<'a>) -> Self {
        Self {
            stops: value.color_stops(),
            var_stops: &[],
        }
    }
}

impl<'a> From<VarColorLine<'a>> for ColorStops<'a> {
    fn from(value: VarColorLine<'a>) -> Self {
        Self {
            stops: &[],
            var_stops: value.color_stops(),
        }
    }
}

impl<'f> ColorStops<'f> {
    pub(crate) fn cursor(&self) -> StopCursor<'f> {
        StopCursor {
            stops: self.stops,
            var_stops: self.var_stops,
            index: 0,
        }
    }
    pub fn resolve<'s>(
        &self,
        instance: &'s ColrInstance<'f, '_>,
    ) -> impl Iterator<Item = ResolvedColorStop> + 's
    where
        'f: 's,
    {
        let mut cursor = self.cursor();
        core::iter::from_fn(move || ordinary(cursor.try_next(instance, &mut Ordinary::new())))
    }
}
pub(crate) struct StopCursor<'f> {
    stops: &'f [ColorStop],
    var_stops: &'f [VarColorStop],
    index: usize,
}
impl StopCursor<'_> {
    pub(crate) fn try_next<C: WorkControl>(
        &mut self,
        instance: &ColrInstance,
        cx: &mut C,
    ) -> Result<Option<ResolvedColorStop>, C::Error> {
        cx.checkpoint(0)?;
        let value = if let Some(stop) = self.stops.get(self.index) {
            cx.checkpoint(1)?;
            ResolvedColorStop {
                offset: stop.stop_offset().to_f32(),
                palette_index: stop.palette_index(),
                alpha: stop.alpha().to_f32(),
            }
        } else if let Some(stop) = self
            .var_stops
            .get(self.index.saturating_sub(self.stops.len()))
        {
            cx.checkpoint(1)?;
            let deltas = instance.try_var_deltas::<2, _>(stop.var_index_base(), cx)?;
            ResolvedColorStop {
                offset: stop.stop_offset().apply_float_delta(deltas[0]),
                palette_index: stop.palette_index(),
                alpha: stop.alpha().apply_float_delta(deltas[1]),
            }
        } else {
            return Ok(None);
        };
        self.index += 1;
        cx.checkpoint(0)?;
        Ok(Some(value))
    }
}

/// Simplified version of `Paint` with applied variation deltas.
///
/// These are constructed with the [`try_resolve_paint`] function.
///
/// This is roughly equivalent to FreeType's
/// [`FT_COLR_Paint`](https://freetype.org/freetype2/docs/reference/ft2-layer_management.html#ft_colr_paint)
/// type.
pub enum ResolvedPaint<'a> {
    ColrLayers {
        range: Range<usize>,
    },
    Solid {
        palette_index: u16,
        alpha: f32,
    },
    LinearGradient {
        x0: f32,
        y0: f32,
        x1: f32,
        y1: f32,
        x2: f32,
        y2: f32,
        color_stops: ColorStops<'a>,
        extend: Extend,
    },
    RadialGradient {
        x0: f32,
        y0: f32,
        radius0: f32,
        x1: f32,
        y1: f32,
        radius1: f32,
        color_stops: ColorStops<'a>,
        extend: Extend,
    },
    SweepGradient {
        center_x: f32,
        center_y: f32,
        start_angle: f32,
        end_angle: f32,
        color_stops: ColorStops<'a>,
        extend: Extend,
    },
    Glyph {
        glyph_id: GlyphId16,
        paint: Paint<'a>,
    },
    ColrGlyph {
        glyph_id: GlyphId16,
    },
    Transform {
        xx: f32,
        yx: f32,
        xy: f32,
        yy: f32,
        dx: f32,
        dy: f32,
        paint: Paint<'a>,
    },
    Translate {
        dx: f32,
        dy: f32,
        paint: Paint<'a>,
    },
    Scale {
        scale_x: f32,
        scale_y: f32,
        around_center: Option<Point<f32>>,
        paint: Paint<'a>,
    },
    Rotate {
        angle: f32,
        around_center: Option<Point<f32>>,
        paint: Paint<'a>,
    },
    Skew {
        x_skew_angle: f32,
        y_skew_angle: f32,
        around_center: Option<Point<f32>>,
        paint: Paint<'a>,
    },
    Composite {
        source_paint: Paint<'a>,
        mode: CompositeMode,
        backdrop_paint: Paint<'a>,
    },
}

/// Resolves this paint with the given instance.
///
/// Resolving means that all numeric values are converted to 32-bit floating
/// point, variation deltas are applied (also computed fully in floating
/// point), and the various transform paints are collapsed into a single value
/// for their category (transform, translate, scale, rotate and skew).
///
/// This provides a simpler type for consumers that are more interested
/// in extracting the semantics of the graph rather than working with the
/// raw encoded structures.
pub(crate) fn try_resolve_paint<'f, C: WorkControl>(
    instance: &ColrInstance<'f, '_>,
    paint: &Paint<'f>,
    cx: &mut C,
) -> ReadCtl<ResolvedPaint<'f>, C> {
    macro_rules! font_value {
        ($v:expr) => {
            match $v {
                Ok(value) => value,
                Err(error) => return Ok(Err(error)),
            }
        };
    }
    cx.checkpoint(1)?;
    Ok(Ok(match paint {
        Paint::ColrLayers(layers) => {
            let start = layers.first_layer_index() as usize;
            let end = match start.checked_add(layers.num_layers() as usize) {
                Some(end) => end,
                None => return Ok(Err(read_fonts::ReadError::OutOfBounds)),
            };
            ResolvedPaint::ColrLayers { range: start..end }
        }
        Paint::Solid(solid) => ResolvedPaint::Solid {
            palette_index: solid.palette_index(),
            alpha: solid.alpha().to_f32(),
        },
        Paint::VarSolid(solid) => {
            let deltas = instance.try_var_deltas::<1, _>(solid.var_index_base(), cx)?;
            ResolvedPaint::Solid {
                palette_index: solid.palette_index(),
                alpha: solid.alpha().apply_float_delta(deltas[0]),
            }
        }
        Paint::LinearGradient(gradient) => {
            let color_line = font_value!(gradient.try_color_line(cx)?);
            let extend = color_line.extend();
            ResolvedPaint::LinearGradient {
                x0: gradient.x0().to_i16() as f32,
                y0: gradient.y0().to_i16() as f32,
                x1: gradient.x1().to_i16() as f32,
                y1: gradient.y1().to_i16() as f32,
                x2: gradient.x2().to_i16() as f32,
                y2: gradient.y2().to_i16() as f32,
                color_stops: color_line.into(),
                extend,
            }
        }
        Paint::VarLinearGradient(gradient) => {
            let color_line = font_value!(gradient.try_color_line(cx)?);
            let extend = color_line.extend();
            let deltas = instance.try_var_deltas::<6, _>(gradient.var_index_base(), cx)?;
            ResolvedPaint::LinearGradient {
                x0: gradient.x0().apply_float_delta(deltas[0]),
                y0: gradient.y0().apply_float_delta(deltas[1]),
                x1: gradient.x1().apply_float_delta(deltas[2]),
                y1: gradient.y1().apply_float_delta(deltas[3]),
                x2: gradient.x2().apply_float_delta(deltas[4]),
                y2: gradient.y2().apply_float_delta(deltas[5]),
                color_stops: color_line.into(),
                extend,
            }
        }
        Paint::RadialGradient(gradient) => {
            let color_line = font_value!(gradient.try_color_line(cx)?);
            let extend = color_line.extend();
            ResolvedPaint::RadialGradient {
                x0: gradient.x0().to_i16() as f32,
                y0: gradient.y0().to_i16() as f32,
                radius0: gradient.radius0().to_u16() as f32,
                x1: gradient.x1().to_i16() as f32,
                y1: gradient.y1().to_i16() as f32,
                radius1: gradient.radius1().to_u16() as f32,
                color_stops: color_line.into(),
                extend,
            }
        }
        Paint::VarRadialGradient(gradient) => {
            let color_line = font_value!(gradient.try_color_line(cx)?);
            let extend = color_line.extend();
            let deltas = instance.try_var_deltas::<6, _>(gradient.var_index_base(), cx)?;
            ResolvedPaint::RadialGradient {
                x0: gradient.x0().apply_float_delta(deltas[0]),
                y0: gradient.y0().apply_float_delta(deltas[1]),
                radius0: gradient.radius0().apply_float_delta(deltas[2]),
                x1: gradient.x1().apply_float_delta(deltas[3]),
                y1: gradient.y1().apply_float_delta(deltas[4]),
                radius1: gradient.radius1().apply_float_delta(deltas[5]),
                color_stops: color_line.into(),
                extend,
            }
        }
        Paint::SweepGradient(gradient) => {
            let color_line = font_value!(gradient.try_color_line(cx)?);
            let extend = color_line.extend();
            ResolvedPaint::SweepGradient {
                center_x: gradient.center_x().to_i16() as f32,
                center_y: gradient.center_y().to_i16() as f32,
                start_angle: gradient.start_angle().to_f32(),
                end_angle: gradient.end_angle().to_f32(),
                color_stops: color_line.into(),
                extend,
            }
        }
        Paint::VarSweepGradient(gradient) => {
            let color_line = font_value!(gradient.try_color_line(cx)?);
            let extend = color_line.extend();
            let deltas = instance.try_var_deltas::<4, _>(gradient.var_index_base(), cx)?;
            ResolvedPaint::SweepGradient {
                center_x: gradient.center_x().apply_float_delta(deltas[0]),
                center_y: gradient.center_y().apply_float_delta(deltas[1]),
                start_angle: gradient.start_angle().apply_float_delta(deltas[2]),
                end_angle: gradient.end_angle().apply_float_delta(deltas[3]),
                color_stops: color_line.into(),
                extend,
            }
        }
        Paint::Glyph(glyph) => ResolvedPaint::Glyph {
            glyph_id: glyph.glyph_id(),
            paint: font_value!(glyph.try_paint(cx)?),
        },
        Paint::ColrGlyph(glyph) => ResolvedPaint::ColrGlyph {
            glyph_id: glyph.glyph_id(),
        },
        Paint::Transform(transform) => {
            let affine = font_value!(transform.try_transform(cx)?);
            let paint = font_value!(transform.try_paint(cx)?);
            ResolvedPaint::Transform {
                xx: affine.xx().to_f32(),
                yx: affine.yx().to_f32(),
                xy: affine.xy().to_f32(),
                yy: affine.yy().to_f32(),
                dx: affine.dx().to_f32(),
                dy: affine.dy().to_f32(),
                paint,
            }
        }
        Paint::VarTransform(transform) => {
            let affine = font_value!(transform.try_transform(cx)?);
            let paint = font_value!(transform.try_paint(cx)?);
            let deltas = instance.try_var_deltas::<6, _>(affine.var_index_base(), cx)?;
            ResolvedPaint::Transform {
                xx: affine.xx().apply_float_delta(deltas[0]),
                yx: affine.yx().apply_float_delta(deltas[1]),
                xy: affine.xy().apply_float_delta(deltas[2]),
                yy: affine.yy().apply_float_delta(deltas[3]),
                dx: affine.dx().apply_float_delta(deltas[4]),
                dy: affine.dy().apply_float_delta(deltas[5]),
                paint,
            }
        }
        Paint::Translate(transform) => ResolvedPaint::Translate {
            dx: transform.dx().to_i16() as f32,
            dy: transform.dy().to_i16() as f32,
            paint: font_value!(transform.try_paint(cx)?),
        },
        Paint::VarTranslate(transform) => {
            let deltas = instance.try_var_deltas::<2, _>(transform.var_index_base(), cx)?;
            ResolvedPaint::Translate {
                dx: transform.dx().apply_float_delta(deltas[0]),
                dy: transform.dy().apply_float_delta(deltas[1]),
                paint: font_value!(transform.try_paint(cx)?),
            }
        }
        Paint::Scale(transform) => ResolvedPaint::Scale {
            scale_x: transform.scale_x().to_f32(),
            scale_y: transform.scale_y().to_f32(),
            around_center: None,
            paint: font_value!(transform.try_paint(cx)?),
        },
        Paint::VarScale(transform) => {
            let deltas = instance.try_var_deltas::<2, _>(transform.var_index_base(), cx)?;
            ResolvedPaint::Scale {
                scale_x: transform.scale_x().apply_float_delta(deltas[0]),
                scale_y: transform.scale_y().apply_float_delta(deltas[1]),
                around_center: None,
                paint: font_value!(transform.try_paint(cx)?),
            }
        }
        Paint::ScaleAroundCenter(transform) => ResolvedPaint::Scale {
            scale_x: transform.scale_x().to_f32(),
            scale_y: transform.scale_y().to_f32(),
            around_center: Some(Point::new(
                transform.center_x().to_i16() as f32,
                transform.center_y().to_i16() as f32,
            )),
            paint: font_value!(transform.try_paint(cx)?),
        },
        Paint::VarScaleAroundCenter(transform) => {
            let deltas = instance.try_var_deltas::<4, _>(transform.var_index_base(), cx)?;
            ResolvedPaint::Scale {
                scale_x: transform.scale_x().apply_float_delta(deltas[0]),
                scale_y: transform.scale_y().apply_float_delta(deltas[1]),
                around_center: Some(Point::new(
                    transform.center_x().apply_float_delta(deltas[2]),
                    transform.center_y().apply_float_delta(deltas[3]),
                )),
                paint: font_value!(transform.try_paint(cx)?),
            }
        }
        Paint::ScaleUniform(transform) => {
            let scale = transform.scale().to_f32();
            ResolvedPaint::Scale {
                scale_x: scale,
                scale_y: scale,
                around_center: None,
                paint: font_value!(transform.try_paint(cx)?),
            }
        }
        Paint::VarScaleUniform(transform) => {
            let deltas = instance.try_var_deltas::<1, _>(transform.var_index_base(), cx)?;
            let scale = transform.scale().apply_float_delta(deltas[0]);
            ResolvedPaint::Scale {
                scale_x: scale,
                scale_y: scale,
                around_center: None,
                paint: font_value!(transform.try_paint(cx)?),
            }
        }
        Paint::ScaleUniformAroundCenter(transform) => {
            let scale = transform.scale().to_f32();
            ResolvedPaint::Scale {
                scale_x: scale,
                scale_y: scale,
                around_center: Some(Point::new(
                    transform.center_x().to_i16() as f32,
                    transform.center_y().to_i16() as f32,
                )),
                paint: font_value!(transform.try_paint(cx)?),
            }
        }
        Paint::VarScaleUniformAroundCenter(transform) => {
            let deltas = instance.try_var_deltas::<3, _>(transform.var_index_base(), cx)?;
            let scale = transform.scale().apply_float_delta(deltas[0]);
            ResolvedPaint::Scale {
                scale_x: scale,
                scale_y: scale,
                around_center: Some(Point::new(
                    transform.center_x().apply_float_delta(deltas[1]),
                    transform.center_y().apply_float_delta(deltas[2]),
                )),
                paint: font_value!(transform.try_paint(cx)?),
            }
        }
        Paint::Rotate(transform) => ResolvedPaint::Rotate {
            angle: transform.angle().to_f32(),
            around_center: None,
            paint: font_value!(transform.try_paint(cx)?),
        },
        Paint::VarRotate(transform) => {
            let deltas = instance.try_var_deltas::<1, _>(transform.var_index_base(), cx)?;
            ResolvedPaint::Rotate {
                angle: transform.angle().apply_float_delta(deltas[0]),
                around_center: None,
                paint: font_value!(transform.try_paint(cx)?),
            }
        }
        Paint::RotateAroundCenter(transform) => ResolvedPaint::Rotate {
            angle: transform.angle().to_f32(),
            around_center: Some(Point::new(
                transform.center_x().to_i16() as f32,
                transform.center_y().to_i16() as f32,
            )),
            paint: font_value!(transform.try_paint(cx)?),
        },
        Paint::VarRotateAroundCenter(transform) => {
            let deltas = instance.try_var_deltas::<3, _>(transform.var_index_base(), cx)?;
            ResolvedPaint::Rotate {
                angle: transform.angle().apply_float_delta(deltas[0]),
                around_center: Some(Point::new(
                    transform.center_x().apply_float_delta(deltas[1]),
                    transform.center_y().apply_float_delta(deltas[2]),
                )),
                paint: font_value!(transform.try_paint(cx)?),
            }
        }
        Paint::Skew(transform) => ResolvedPaint::Skew {
            x_skew_angle: transform.x_skew_angle().to_f32(),
            y_skew_angle: transform.y_skew_angle().to_f32(),
            around_center: None,
            paint: font_value!(transform.try_paint(cx)?),
        },
        Paint::VarSkew(transform) => {
            let deltas = instance.try_var_deltas::<2, _>(transform.var_index_base(), cx)?;
            ResolvedPaint::Skew {
                x_skew_angle: transform.x_skew_angle().apply_float_delta(deltas[0]),
                y_skew_angle: transform.y_skew_angle().apply_float_delta(deltas[1]),
                around_center: None,
                paint: font_value!(transform.try_paint(cx)?),
            }
        }
        Paint::SkewAroundCenter(transform) => ResolvedPaint::Skew {
            x_skew_angle: transform.x_skew_angle().to_f32(),
            y_skew_angle: transform.y_skew_angle().to_f32(),
            around_center: Some(Point::new(
                transform.center_x().to_i16() as f32,
                transform.center_y().to_i16() as f32,
            )),
            paint: font_value!(transform.try_paint(cx)?),
        },
        Paint::VarSkewAroundCenter(transform) => {
            let deltas = instance.try_var_deltas::<4, _>(transform.var_index_base(), cx)?;
            ResolvedPaint::Skew {
                x_skew_angle: transform.x_skew_angle().apply_float_delta(deltas[0]),
                y_skew_angle: transform.y_skew_angle().apply_float_delta(deltas[1]),
                around_center: Some(Point::new(
                    transform.center_x().apply_float_delta(deltas[2]),
                    transform.center_y().apply_float_delta(deltas[3]),
                )),
                paint: font_value!(transform.try_paint(cx)?),
            }
        }
        Paint::Composite(composite) => ResolvedPaint::Composite {
            source_paint: font_value!(composite.try_source_paint(cx)?),
            mode: composite.composite_mode(),
            backdrop_paint: font_value!(composite.try_backdrop_paint(cx)?),
        },
    }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use raw::{FontRef, TableProvider};

    /// OSS Fuzz caught add with overflow when computing delta indices.
    /// See <https://oss-fuzz.com/testcase-detail/5180237819478016>
    /// and <https://g-issues.oss-fuzz.com/issues/439498857>
    #[test]
    fn var_delta_index_overflow() {
        let font = FontRef::new(font_test_data::COLRV0V1_VARIABLE).unwrap();
        let coords = &[F2Dot14::from_f32(0.5)];
        let instance = ColrInstance::new(font.colr().unwrap(), coords);
        // Just don't panic with overflow
        let _: [FloatItemDelta; 4] = instance.var_deltas(0xFFFFFFFE);
    }
}
