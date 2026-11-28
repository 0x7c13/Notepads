// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE file in the project root for license information.
// ---------------------------------------------------------------------------------------------
//! Request-bound color readers and typed painter callbacks.

use crate::color::{
    self, Brush, ColorGlyphFormat, CompositeMode, PaintCachedColorGlyph, PaintError, Transform,
};
use crate::control::{
    paint_checkpoint, paint_value, validate_resources, KernelError, PaintKernelResult,
};
use crate::instance::{LocationRef, Size};
use crate::{FontRef, GlyphId};
use markdown_svg_budget::{Error, RequestControl, ResourcesRef};
use read_fonts::types::BoundingBox;

/// A painter bound to the same request as the checked color glyph.
///
/// A failed method must record its error before its own temporary owners retire.
/// `abort_color_glyph` discards the current candidate without painting, composing
/// layers or finishing output. The caller still owns and retires its resources.
pub trait ColorPainter {
    fn resources(&self) -> &ResourcesRef;
    fn push_transform(&mut self, transform: Transform) -> Result<(), Error>;
    fn pop_transform(&mut self) -> Result<(), Error>;
    fn push_clip_glyph(&mut self, glyph: GlyphId) -> Result<(), Error>;
    fn push_clip_box(&mut self, bounds: BoundingBox<f32>) -> Result<(), Error>;
    fn pop_clip(&mut self) -> Result<(), Error>;
    fn fill(&mut self, brush: Brush<'_>) -> Result<(), Error>;
    // Required: the ordinary trait's multi-action default cannot be checked
    // between its actions by an outside adapter.
    fn fill_glyph(
        &mut self,
        glyph: GlyphId,
        transform: Option<Transform>,
        brush: Brush<'_>,
    ) -> Result<(), Error>;
    fn push_layer(&mut self, mode: CompositeMode) -> Result<(), Error>;
    fn pop_layer_with_mode(&mut self, mode: CompositeMode) -> Result<(), Error>;
    fn paint_cached_color_glyph(
        &mut self,
        _glyph: GlyphId,
    ) -> Result<Result<PaintCachedColorGlyph, PaintError>, Error> {
        self.resources().checkpoint(0)?;
        Ok(Ok(PaintCachedColorGlyph::Unimplemented))
    }
    fn abort_color_glyph(&mut self);
}

/// Checked color selection. No ordinary owned wrapper can be extracted.
pub struct ColorGlyphs<'f, 'r> {
    inner: color::ColorGlyphCollection<'f>,
    resources: &'r ResourcesRef,
}
impl<'f, 'r> ColorGlyphs<'f, 'r> {
    pub fn try_new(font: &FontRef<'f>, resources: &'r ResourcesRef) -> Result<Self, Error> {
        resources.checkpoint(0)?;
        let inner = color::ColorGlyphCollection::try_new(font, &mut RequestControl(resources))?;
        resources.checkpoint(0)?;
        Ok(Self { inner, resources })
    }
    pub fn try_get(&self, glyph: GlyphId) -> Result<Option<ColorGlyph<'f, 'r>>, Error> {
        let value = self
            .inner
            .try_get(glyph, &mut RequestControl(self.resources))?;
        self.resources.checkpoint(0)?;
        Ok(value.map(|inner| ColorGlyph {
            inner,
            resources: self.resources,
        }))
    }
    pub fn try_get_with_format(
        &self,
        glyph: GlyphId,
        format: ColorGlyphFormat,
    ) -> Result<Option<ColorGlyph<'f, 'r>>, Error> {
        let value =
            self.inner
                .try_get_with_format(glyph, format, &mut RequestControl(self.resources))?;
        self.resources.checkpoint(0)?;
        Ok(value.map(|inner| ColorGlyph {
            inner,
            resources: self.resources,
        }))
    }
}

/// Font bytes, variation coordinates and resources have independent lifetimes.
pub struct ColorGlyph<'f, 'r> {
    inner: color::ColorGlyph<'f>,
    resources: &'r ResourcesRef,
}
impl ColorGlyph<'_, '_> {
    pub fn format(&self) -> ColorGlyphFormat {
        self.inner.format()
    }
    pub fn try_bounding_box(
        &self,
        location: LocationRef<'_>,
        size: Size,
    ) -> Result<Option<BoundingBox<f32>>, Error> {
        let value =
            self.inner
                .try_bounding_box(location, size, &mut RequestControl(self.resources))?;
        self.resources.checkpoint(0)?;
        Ok(value)
    }
    pub fn paint(
        &self,
        location: LocationRef<'_>,
        painter: &mut impl ColorPainter,
    ) -> Result<Result<(), PaintError>, Error> {
        // Reject a foreign painter before dispatching any operation to it.
        validate_resources(painter.resources(), self.resources)?;
        let result = self.inner.try_paint(
            location,
            &mut RequestPainter(painter),
            &mut RequestControl(self.resources),
        );
        match result {
            Err(error) => {
                let error = self.resources.record(error);
                painter.abort_color_glyph();
                // Abort may retire real painter-owned resources; first fault wins.
                self.resources.checkpoint(0)?;
                Err(error)
            }
            Ok(Err(error)) => {
                painter.abort_color_glyph();
                self.resources.checkpoint(0)?;
                Ok(Err(error))
            }
            Ok(Ok(())) => {
                self.resources.checkpoint(0)?;
                Ok(Ok(()))
            }
        }
    }
}

/// Palette views borrow the original fixed color records; no decoded owner.
pub struct ColorPalettes<'f, 'r> {
    inner: color::ColorPalettes<'f>,
    resources: &'r ResourcesRef,
}
impl<'f, 'r> ColorPalettes<'f, 'r> {
    pub fn try_new(font: &FontRef<'f>, resources: &'r ResourcesRef) -> Result<Self, Error> {
        resources.checkpoint(0)?;
        let inner = color::ColorPalettes::try_new(font, &mut RequestControl(resources))?;
        resources.checkpoint(0)?;
        Ok(Self { inner, resources })
    }
    pub fn len(&self) -> u16 {
        self.inner.len()
    }
    pub fn is_empty(&self) -> bool {
        self.inner.is_empty()
    }
    pub fn try_get(&self, index: u16) -> Result<Option<ColorPalette<'f, 'r>>, Error> {
        let value = self
            .inner
            .try_get(index, &mut RequestControl(self.resources))?;
        self.resources.checkpoint(0)?;
        Ok(value.map(|inner| ColorPalette {
            inner,
            resources: self.resources,
        }))
    }
    pub fn try_color_label(&self, index: u16) -> Result<Option<crate::string::StringId>, Error> {
        let value = self
            .inner
            .try_color_label(index, &mut RequestControl(self.resources))?;
        self.resources.checkpoint(0)?;
        Ok(value)
    }
}
pub struct ColorPalette<'f, 'r> {
    inner: color::ColorPalette<'f>,
    resources: &'r ResourcesRef,
}
impl ColorPalette<'_, '_> {
    pub fn colors(&self) -> &[color::Color] {
        self.inner.colors()
    }
    pub fn index(&self) -> u16 {
        self.inner.index()
    }
    pub fn try_palette_type(&self) -> Result<Option<read_fonts::tables::cpal::PaletteType>, Error> {
        let value = self
            .inner
            .try_palette_type(&mut RequestControl(self.resources))?;
        self.resources.checkpoint(0)?;
        Ok(value)
    }
    pub fn try_label(&self) -> Result<Option<crate::string::StringId>, Error> {
        let value = self.inner.try_label(&mut RequestControl(self.resources))?;
        self.resources.checkpoint(0)?;
        Ok(value)
    }
}

struct RequestPainter<'p, P>(&'p mut P);
#[cfg(test)]
#[path = "request_color_tests.rs"]
mod tests;
impl<'r, P: ColorPainter> color::PainterMode<RequestControl<'r>> for RequestPainter<'_, P> {
    fn push_transform(
        &mut self,
        value: Transform,
        cx: &mut RequestControl<'r>,
    ) -> PaintKernelResult<(), Error> {
        paint_checkpoint(cx, 1)?;
        let value = self.0.push_transform(value);
        paint_checkpoint(cx, 0)?;
        value.map_err(|error| KernelError::Control(cx.0.record(error)))
    }
    fn pop_transform(&mut self, cx: &mut RequestControl<'r>) -> PaintKernelResult<(), Error> {
        paint_checkpoint(cx, 1)?;
        let value = self.0.pop_transform();
        paint_checkpoint(cx, 0)?;
        value.map_err(|error| KernelError::Control(cx.0.record(error)))
    }
    fn push_clip_glyph(
        &mut self,
        glyph: GlyphId,
        cx: &mut RequestControl<'r>,
    ) -> PaintKernelResult<(), Error> {
        paint_checkpoint(cx, 1)?;
        let value = self.0.push_clip_glyph(glyph);
        paint_checkpoint(cx, 0)?;
        value.map_err(|error| KernelError::Control(cx.0.record(error)))
    }
    fn push_clip_box(
        &mut self,
        bounds: BoundingBox<f32>,
        cx: &mut RequestControl<'r>,
    ) -> PaintKernelResult<(), Error> {
        paint_checkpoint(cx, 1)?;
        let value = self.0.push_clip_box(bounds);
        paint_checkpoint(cx, 0)?;
        value.map_err(|error| KernelError::Control(cx.0.record(error)))
    }
    fn pop_clip(&mut self, cx: &mut RequestControl<'r>) -> PaintKernelResult<(), Error> {
        paint_checkpoint(cx, 1)?;
        let value = self.0.pop_clip();
        paint_checkpoint(cx, 0)?;
        value.map_err(|error| KernelError::Control(cx.0.record(error)))
    }
    fn fill(
        &mut self,
        brush: Brush<'_>,
        cx: &mut RequestControl<'r>,
    ) -> PaintKernelResult<(), Error> {
        paint_checkpoint(cx, 1)?;
        let value = self.0.fill(brush);
        paint_checkpoint(cx, 0)?;
        value.map_err(|error| KernelError::Control(cx.0.record(error)))
    }
    fn fill_glyph(
        &mut self,
        glyph: GlyphId,
        transform: Option<Transform>,
        brush: Brush<'_>,
        cx: &mut RequestControl<'r>,
    ) -> PaintKernelResult<(), Error> {
        paint_checkpoint(cx, 1)?;
        let value = self.0.fill_glyph(glyph, transform, brush);
        paint_checkpoint(cx, 0)?;
        value.map_err(|error| KernelError::Control(cx.0.record(error)))
    }
    fn push_layer(
        &mut self,
        mode: CompositeMode,
        cx: &mut RequestControl<'r>,
    ) -> PaintKernelResult<(), Error> {
        paint_checkpoint(cx, 1)?;
        let value = self.0.push_layer(mode);
        paint_checkpoint(cx, 0)?;
        value.map_err(|error| KernelError::Control(cx.0.record(error)))
    }
    fn pop_layer_with_mode(
        &mut self,
        mode: CompositeMode,
        cx: &mut RequestControl<'r>,
    ) -> PaintKernelResult<(), Error> {
        paint_checkpoint(cx, 1)?;
        let value = self.0.pop_layer_with_mode(mode);
        paint_checkpoint(cx, 0)?;
        value.map_err(|error| KernelError::Control(cx.0.record(error)))
    }
    fn paint_cached_color_glyph(
        &mut self,
        glyph: GlyphId,
        cx: &mut RequestControl<'r>,
    ) -> PaintKernelResult<PaintCachedColorGlyph, Error> {
        paint_checkpoint(cx, 1)?;
        let value = self.0.paint_cached_color_glyph(glyph);
        paint_checkpoint(cx, 0)?;
        paint_value(value.map_err(|error| cx.0.record(error)))
    }
}
