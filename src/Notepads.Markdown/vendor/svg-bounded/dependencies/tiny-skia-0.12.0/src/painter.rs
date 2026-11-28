// Copyright 2006 The Android Open Source Project
// Copyright 2020 Yevhenii Reizner
// Modifications Copyright (c) 2026, Jiaqi (0x7c13) Liu.
//
// Use of this source code is governed by a BSD-style license that can be
// found in the LICENSE file.

use crate::shaders::PaintView;
use crate::*;
use markdown_svg_budget::{
    Error as GeometryError, FailureKind, LimitKind, ResourceKind, ResourcesRef,
};

type GeometryResult<T> = Result<T, GeometryError>;

/// Borrowed policy shared by ordinary and request-controlled drawing kernels.
/// No account or independent failure state is created.
#[derive(Clone, Copy)]
pub(crate) struct GeometryControl<'a>(Option<&'a ResourcesRef>);

impl markdown_svg_budget::WorkControl for GeometryControl<'_> {
    type Error = GeometryError;

    fn checkpoint(&mut self, work: u64) -> GeometryResult<()> {
        self.check(work)
    }
}

impl<'a> GeometryControl<'a> {
    /// Read only the authoritative sticky slot; this never calls the backend.
    pub(crate) fn peek_slot(self) -> GeometryResult<()> {
        match self.0.and_then(|resources| resources.failure()) {
            Some(error) => Err(error),
            None => Ok(()),
        }
    }
    pub(crate) fn invariant(self) -> GeometryError {
        let error = GeometryError::new(FailureKind::Invariant, LimitKind::None);
        self.0.map_or(error, |resources| resources.record(error))
    }
    pub(crate) fn resources(self) -> Option<&'a ResourcesRef> {
        self.0
    }
    pub(crate) fn ordinary() -> Self {
        Self(None)
    }
    pub(crate) fn request(resources: &'a ResourcesRef) -> Self {
        Self(Some(resources))
    }
    pub(crate) fn check(self, work: u64) -> GeometryResult<()> {
        match self.0 {
            Some(resources) => resources.checkpoint(work),
            None => Ok(()),
        }
    }
    pub(crate) fn source(self, path: &Path) -> GeometryResult<()> {
        self.check(0)?;
        if let Some(resources) = self.0 {
            // A valid compatibility Path has nonempty verb/point allocations.
            // Only paid storage reports zero unpaid capacity; its owner must
            // belong to this same request even when the identity path is read.
            match path.retained_unpaid_capacity_bytes() {
                Some(0) if !path.belongs_to(resources) => {
                    return Err(resources
                        .record(GeometryError::new(FailureKind::Invariant, LimitKind::None)));
                }
                None => {
                    return Err(resources
                        .record(GeometryError::new(FailureKind::Quota, LimitKind::Capacity)));
                }
                _ => {}
            }
        }
        Ok(())
    }
    pub(crate) fn finish<T>(self, candidate: GeometryResult<T>) -> GeometryResult<T> {
        match candidate {
            Err(error) => Err(self.0.map_or(error, |resources| resources.record(error))),
            Ok(value) => {
                self.check(0)?;
                Ok(value)
            }
        }
    }
    pub(crate) fn copy(self, path: &Path) -> GeometryResult<Path> {
        match self.0 {
            Some(resources) => path.try_copy_in(resources, ResourceKind::Scratch),
            None => Ok(path.clone()),
        }
    }
    pub(crate) fn transform(
        self,
        path: Path,
        transform: Transform,
    ) -> GeometryResult<Option<Path>> {
        match self.0 {
            Some(resources) => path.try_transform(transform, resources, ResourceKind::Scratch),
            None => Ok(path.transform(transform)),
        }
    }
    fn rect(self, rect: Rect) -> GeometryResult<Path> {
        match self.0 {
            Some(resources) => PathBuilder::try_from_rect(rect, resources, ResourceKind::Scratch),
            None => Ok(PathBuilder::from_rect(rect)),
        }
    }
    fn dash(self, path: &Path, dash: &StrokeDash, scale: f32) -> GeometryResult<Option<Path>> {
        match self.0 {
            Some(resources) => path.try_dash(dash, scale, resources, ResourceKind::Scratch),
            None => Ok(path.dash(dash, scale)),
        }
    }
    fn stroke(self, path: &Path, stroke: &Stroke, scale: f32) -> GeometryResult<Option<Path>> {
        match self.0 {
            Some(resources) => path.try_stroke(stroke, scale, resources, ResourceKind::Scratch),
            None => Ok(path.stroke(stroke, scale)),
        }
    }
}

use tiny_skia_path::{PathStroker, Scalar, SCALAR_MAX};

use crate::blitter::Blitter;
use crate::geom::ScreenIntRect;
use crate::mask::SubMaskRef;
use crate::pipeline::{RasterPipelineBlitter, RasterPipelineBuilder};
use crate::pixmap::SubPixmapMut;
use crate::scan;

use crate::geom::IntSizeExt;
#[cfg(all(not(feature = "std"), feature = "no-std-float"))]
use tiny_skia_path::NoStdFloat;

/// A path filling rule.
#[derive(Copy, Clone, Default, PartialEq, Debug)]
pub enum FillRule {
    /// Specifies that "inside" is computed by a non-zero sum of signed edge crossings.
    #[default]
    Winding,
    /// Specifies that "inside" is computed by an odd number of edge crossings.
    EvenOdd,
}

/// Failure to paint borrowed geometric coverage.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CoveragePaintError<E> {
    /// Coverage and destination dimensions differ.
    SizeMismatch,
    /// Geometric coverage requires anti-aliased paint.
    InvalidPaint,
    /// The device-space shader could not build a raster pipeline.
    Shader,
    /// The caller rejected a checkpoint or pixel charge.
    Checkpoint(E),
}

/// Controls how a shape should be painted.
#[derive(Clone, PartialEq, Debug)]
pub struct Paint<'a> {
    /// A paint shader.
    ///
    /// Default: black color
    pub shader: Shader<'a>,

    /// Paint blending mode.
    ///
    /// Default: SourceOver
    pub blend_mode: BlendMode,

    /// Enables anti-aliased painting.
    ///
    /// Default: true
    pub anti_alias: bool,

    /// Colorspace for blending.
    ///
    /// This enables gamma correction during the blend operation.  While skia supports
    /// full color-space conversions, we only support a few (simple) cases.  Note that
    /// any color space other than Linear will force using the high-quality pipeline.
    ///
    /// Default: Linear
    pub colorspace: ColorSpace,

    /// Forces the high quality/precision rendering pipeline.
    ///
    /// `tiny-skia`, just like Skia, has two rendering pipelines:
    /// one uses `f32` and another one uses `u16`. `u16` one is usually way faster,
    /// but less precise. Which can lead to slight differences.
    ///
    /// By default, `tiny-skia` will choose the pipeline automatically,
    /// depending on a blending mode and other parameters.
    /// But you can force the high quality one using this flag.
    ///
    /// This feature is especially useful during testing.
    ///
    /// Unlike high quality pipeline, the low quality one doesn't support all
    /// rendering stages, therefore we cannot force it like hq one.
    ///
    /// Default: false
    pub force_hq_pipeline: bool,
}

impl Default for Paint<'_> {
    fn default() -> Self {
        Paint {
            shader: Shader::SolidColor(Color::BLACK),
            blend_mode: BlendMode::default(),
            anti_alias: true,
            colorspace: ColorSpace::default(),
            force_hq_pipeline: false,
        }
    }
}

impl Paint<'_> {
    /// Sets a paint source to a solid color.
    pub fn set_color(&mut self, color: Color) {
        self.shader = Shader::SolidColor(color);
    }

    /// Sets a paint source to a solid color.
    ///
    /// `self.shader = Shader::SolidColor(Color::from_rgba8(50, 127, 150, 200));` shorthand.
    pub fn set_color_rgba8(&mut self, r: u8, g: u8, b: u8, a: u8) {
        self.set_color(Color::from_rgba8(r, g, b, a))
    }

    /// Checks that the paint source is a solid color.
    pub fn is_solid_color(&self) -> bool {
        matches!(self.shader, Shader::SolidColor(_))
    }
}

impl Pixmap {
    /// Draws a filled rectangle onto the pixmap.
    ///
    /// See [`PixmapMut::fill_rect`](struct.PixmapMut.html#method.fill_rect) for details.
    pub fn fill_rect(
        &mut self,
        rect: Rect,
        paint: &Paint,
        transform: Transform,
        mask: Option<&Mask>,
    ) {
        self.as_mut().fill_rect(rect, paint, transform, mask);
    }

    /// Checked geometry variant using the original request.
    pub fn try_fill_rect(
        &mut self,
        rect: Rect,
        paint: &Paint,
        transform: Transform,
        mask: Option<&Mask>,
        resources: &ResourcesRef,
    ) -> Result<(), GeometryError> {
        self.as_mut()
            .try_fill_rect(rect, paint, transform, mask, resources)
    }

    /// Draws a filled path onto the pixmap.
    ///
    /// See [`PixmapMut::fill_path`](struct.PixmapMut.html#method.fill_path) for details.
    pub fn fill_path(
        &mut self,
        path: &Path,
        paint: &Paint,
        fill_rule: FillRule,
        transform: Transform,
        mask: Option<&Mask>,
    ) {
        self.as_mut()
            .fill_path(path, paint, fill_rule, transform, mask);
    }

    /// Checked geometry variant using the original request.
    pub fn try_fill_path(
        &mut self,
        path: &Path,
        paint: &Paint,
        fill_rule: FillRule,
        transform: Transform,
        mask: Option<&Mask>,
        resources: &ResourcesRef,
    ) -> Result<(), GeometryError> {
        self.as_mut()
            .try_fill_path(path, paint, fill_rule, transform, mask, resources)
    }

    /// Strokes a path.
    ///
    /// See [`PixmapMut::stroke_path`](struct.PixmapMut.html#method.stroke_path) for details.
    pub fn stroke_path(
        &mut self,
        path: &Path,
        paint: &Paint,
        stroke: &Stroke,
        transform: Transform,
        mask: Option<&Mask>,
    ) {
        self.as_mut()
            .stroke_path(path, paint, stroke, transform, mask);
    }

    /// Checked geometry variant using the original request.
    pub fn try_stroke_path(
        &mut self,
        path: &Path,
        paint: &Paint,
        stroke: &Stroke,
        transform: Transform,
        mask: Option<&Mask>,
        resources: &ResourcesRef,
    ) -> Result<(), GeometryError> {
        self.as_mut()
            .try_stroke_path(path, paint, stroke, transform, mask, resources)
    }

    /// Draws a `Pixmap` on top of the current `Pixmap`.
    ///
    /// See [`PixmapMut::draw_pixmap`](struct.PixmapMut.html#method.draw_pixmap) for details.
    pub fn draw_pixmap(
        &mut self,
        x: i32,
        y: i32,
        pixmap: PixmapRef,
        paint: &PixmapPaint,
        transform: Transform,
        mask: Option<&Mask>,
    ) {
        self.as_mut()
            .draw_pixmap(x, y, pixmap, paint, transform, mask);
    }

    /// Checked geometry variant using the original request.
    pub fn try_draw_pixmap(
        &mut self,
        x: i32,
        y: i32,
        pixmap: PixmapRef,
        paint: &PixmapPaint,
        transform: Transform,
        mask: Option<&Mask>,
        resources: &ResourcesRef,
    ) -> Result<(), GeometryError> {
        self.as_mut()
            .try_draw_pixmap(x, y, pixmap, paint, transform, mask, resources)
    }

    /// Applies a masks.
    ///
    /// See [`PixmapMut::apply_mask`](struct.PixmapMut.html#method.apply_mask) for details.
    pub fn apply_mask(&mut self, mask: &Mask) {
        self.as_mut().apply_mask(mask);
    }
}

impl PixmapMut<'_> {
    /// Paints tightly packed A8 as geometric coverage, borrowing device-space paint.
    ///
    /// The caller must transform the shader once before calling and enable AA.
    /// Checkpoints run at entry/normal exit with zero, and before each window of
    /// at most 4096 coverage pixels is read. An error may leave a painted prefix;
    /// the caller must not publish the destination on failure.
    pub fn paint_coverage_checked<E>(
        &mut self,
        coverage: MaskRef<'_>,
        paint: &Paint<'_>,
        mut checkpoint: impl FnMut(u64) -> Result<(), E>,
    ) -> Result<(), CoveragePaintError<E>> {
        self.paint_coverage_with_view(
            coverage,
            PaintView::plain(paint),
            &mut checkpoint,
            &mut |_| CoveragePaintError::Shader,
            GeometryControl::ordinary(),
        )
    }
    /// Paint geometric A8 using immutable paint and the original request account.
    /// The shader receives this transform once; the A8 stays in device space.
    pub fn try_paint_coverage(
        &mut self,
        coverage: MaskRef<'_>,
        paint: &CheckedPaint<'_>,
        paint_transform: Transform,
        resources: &ResourcesRef,
    ) -> Result<(), GeometryError> {
        let control = GeometryControl::request(resources);
        let mut view = paint.view(resources)?;
        view.transform(paint_transform);
        let result = self.paint_coverage_with_view(
            coverage,
            view,
            &mut |_| Ok(()),
            &mut CoveragePaintError::Checkpoint,
            control,
        );
        match result {
            Ok(()) => control.check(0),
            Err(CoveragePaintError::Checkpoint(error)) => Err(resources.record(error)),
            Err(_) => Err(control.invariant()),
        }
    }
    fn paint_coverage_with_view<E>(
        &mut self,
        coverage: MaskRef<'_>,
        mut paint: PaintView<'_, '_, '_>,
        checkpoint: &mut impl FnMut(u64) -> Result<(), E>,
        map_resource: &mut impl FnMut(GeometryError) -> CoveragePaintError<E>,
        control: GeometryControl<'_>,
    ) -> Result<(), CoveragePaintError<E>> {
        control.peek_slot().map_err(&mut *map_resource)?;
        paint.validate(control).map_err(&mut *map_resource)?;
        checkpoint(0).map_err(CoveragePaintError::Checkpoint)?;
        if coverage.width() != self.width() || coverage.height() != self.height() {
            return Err(CoveragePaintError::SizeMismatch);
        }
        if !paint.options.anti_alias {
            return Err(CoveragePaintError::InvalidPaint);
        }
        if paint.options.blend_mode == BlendMode::Destination
            || (paint.options.blend_mode == BlendMode::DestinationIn
                && paint.is_solid_color()
                && paint.is_opaque(control).map_err(&mut *map_resource)?)
        {
            return checkpoint(0).map_err(CoveragePaintError::Checkpoint);
        }

        // Clear has no source-color dependency. Keep its geometric blend mode,
        // but do not evaluate or allocate storage for an unrelated shader.
        if paint.options.blend_mode == BlendMode::Clear {
            paint.clear_source();
        } else if !paint.coverage_coordinates_valid() {
            return Err(CoveragePaintError::Shader);
        }

        let width = self.width();
        let height = self.height();
        let mut subpix = self.as_subpixmap();
        let candidate = (|| {
            let mut blitter =
                RasterPipelineBlitter::new_with_view(paint, None, &mut subpix, control)
                    .map_err(&mut *map_resource)?
                    .ok_or(CoveragePaintError::Shader)?;
            let mut read_credit = 0usize;
            for y in 0..height {
                control.check(0).map_err(&mut *map_resource)?;
                // MaskRef checked the tight allocation size, including usize bounds.
                let row = y as usize * width as usize;
                let mut x = 0;
                while x < width {
                    let count = (width - x).min(4096);
                    checkpoint(u64::from(count)).map_err(CoveragePaintError::Checkpoint)?;
                    let end = x + count;
                    // Run detection and pair lookahead never cross the paid window.
                    while x < end {
                        control.peek_slot().map_err(&mut *map_resource)?;
                        let alpha = coverage_read(
                            coverage.data(),
                            row + x as usize,
                            &mut read_credit,
                            control,
                        )
                        .map_err(&mut *map_resource)?;
                        if alpha == 0 || alpha == 255 {
                            let start = x;
                            x += 1;
                            while x < end
                                && coverage_read(
                                    coverage.data(),
                                    row + x as usize,
                                    &mut read_credit,
                                    control,
                                )
                                .map_err(&mut *map_resource)?
                                    == alpha
                            {
                                x += 1;
                            }
                            if alpha == 255 {
                                blitter.blit_h(start, y, LengthU32::new(x - start).unwrap());
                            }
                        } else if end - x >= 2 {
                            let next = coverage_read(
                                coverage.data(),
                                row + x as usize + 1,
                                &mut read_credit,
                                control,
                            )
                            .map_err(&mut *map_resource)?;
                            blitter.blit_anti_h2(x, y, alpha, next);
                            x += 2;
                        } else {
                            blitter.blit_v(x, y, crate::math::LENGTH_U32_ONE, alpha);
                            x += 1;
                        }
                        control.peek_slot().map_err(&mut *map_resource)?;
                    }
                }
            }
            checkpoint(0).map_err(CoveragePaintError::Checkpoint)
        })();
        // Blitter's real arrays and headers have retired before the tail slot read.
        control.check(0).map_err(&mut *map_resource)?;
        candidate
    }

    // TODO: accept NonZeroRect?
    /// Draws a filled rectangle onto the pixmap.
    ///
    /// This function is usually slower than filling a rectangular path,
    /// but it produces better results. Mainly it doesn't suffer from weird
    /// clipping of horizontal/vertical edges.
    ///
    /// Used mainly to render a pixmap onto a pixmap.
    pub fn fill_rect(
        &mut self,
        rect: Rect,
        paint: &Paint,
        transform: Transform,
        mask: Option<&Mask>,
    ) {
        let _ = self.fill_rect_with_control(
            rect,
            PaintView::plain(paint),
            transform,
            mask,
            GeometryControl::ordinary(),
        );
    }

    /// Checked geometry variant; raster/AA/shader internals retain their original scope.
    pub fn try_fill_rect(
        &mut self,
        rect: Rect,
        paint: &Paint,
        transform: Transform,
        mask: Option<&Mask>,
        resources: &ResourcesRef,
    ) -> Result<(), GeometryError> {
        let paint = CheckedPaint::try_from_plain(paint, resources)?;
        let candidate = self.try_fill_rect_checked(rect, &paint, transform, mask, resources);
        drop(paint);
        GeometryControl::request(resources).finish(candidate)
    }
    /// Draw using immutable paint owned by this same request.
    pub fn try_fill_rect_checked(
        &mut self,
        rect: Rect,
        paint: &CheckedPaint,
        transform: Transform,
        mask: Option<&Mask>,
        resources: &ResourcesRef,
    ) -> Result<(), GeometryError> {
        let view = paint.view(resources)?;
        self.fill_rect_with_control(
            rect,
            view,
            transform,
            mask,
            GeometryControl::request(resources),
        )
    }

    fn fill_rect_with_control(
        &mut self,
        rect: Rect,
        paint: PaintView<'_, '_, '_>,
        transform: Transform,
        mask: Option<&Mask>,
        control: GeometryControl<'_>,
    ) -> GeometryResult<()> {
        control.check(1)?;
        let candidate = (|| {
            // TODO: we probably can use tiler for rect too
            if transform.is_identity() && !DrawTiler::required(self.width(), self.height()) {
                // TODO: ignore rects outside the pixmap

                let clip = self.size().to_screen_int_rect(0, 0);

                let mask = mask.map(|mask| mask.as_submask());
                let mut subpix = self.as_subpixmap();
                let mut blitter = match RasterPipelineBlitter::new_with_view(
                    paint,
                    mask,
                    &mut subpix,
                    control,
                )? {
                    Some(v) => v,
                    None => return Ok(()), // nothing to do, all good
                };

                if paint.options.anti_alias {
                    control.check(0)?;
                    scan::hairline_aa::fill_rect_with_control(&rect, &clip, &mut blitter, control)?;
                } else {
                    control.check(0)?;
                    scan::fill_rect(&rect, &clip, &mut blitter);
                }
            } else {
                let path = control.rect(rect)?;
                self.fill_path_with_control(
                    &path,
                    paint,
                    FillRule::Winding,
                    transform,
                    mask,
                    control,
                )?;
            }

            Ok(())
        })();
        control.finish(candidate)
    }

    /// Draws a filled path onto the pixmap.
    pub fn fill_path(
        &mut self,
        path: &Path,
        paint: &Paint,
        fill_rule: FillRule,
        transform: Transform,
        mask: Option<&Mask>,
    ) {
        let _ = self.fill_path_with_control(
            path,
            PaintView::plain(paint),
            fill_rule,
            transform,
            mask,
            GeometryControl::ordinary(),
        );
    }

    /// Checked geometry variant; raster/AA/shader internals retain their original scope.
    pub fn try_fill_path(
        &mut self,
        path: &Path,
        paint: &Paint,
        fill_rule: FillRule,
        transform: Transform,
        mask: Option<&Mask>,
        resources: &ResourcesRef,
    ) -> Result<(), GeometryError> {
        let paint = CheckedPaint::try_from_plain(paint, resources)?;
        let candidate =
            self.try_fill_path_checked(path, &paint, fill_rule, transform, mask, resources);
        drop(paint);
        GeometryControl::request(resources).finish(candidate)
    }
    /// Draw using immutable paint owned by this same request.
    pub fn try_fill_path_checked(
        &mut self,
        path: &Path,
        paint: &CheckedPaint,
        fill_rule: FillRule,
        transform: Transform,
        mask: Option<&Mask>,
        resources: &ResourcesRef,
    ) -> Result<(), GeometryError> {
        let view = paint.view(resources)?;
        self.fill_path_with_control(
            path,
            view,
            fill_rule,
            transform,
            mask,
            GeometryControl::request(resources),
        )
    }

    fn fill_path_with_control(
        &mut self,
        path: &Path,
        paint: PaintView<'_, '_, '_>,
        fill_rule: FillRule,
        transform: Transform,
        mask: Option<&Mask>,
        control: GeometryControl<'_>,
    ) -> GeometryResult<()> {
        control.check(1)?;
        let candidate = (|| {
            control.source(path)?;
            if transform.is_identity() {
                // This is sort of similar to SkDraw::drawPath

                // Skip empty paths and horizontal/vertical lines.
                let path_bounds = path.bounds();
                if path_bounds.width().is_nearly_zero() || path_bounds.height().is_nearly_zero() {
                    log::warn!("empty paths and horizontal/vertical lines cannot be filled");
                    return Ok(());
                }

                if is_too_big_for_math(path) {
                    log::warn!("path coordinates are too big");
                    return Ok(());
                }

                // TODO: ignore paths outside the pixmap

                if let Some(tiler) = DrawTiler::new(self.width(), self.height()) {
                    let mut path = control.copy(path)?; // TODO: avoid cloning
                    let mut paint = paint;

                    for tile in tiler {
                        control.check(1)?;
                        let ts = Transform::from_translate(-(tile.x() as f32), -(tile.y() as f32));
                        path = match control.transform(path, ts)? {
                            Some(v) => v,
                            None => {
                                log::warn!("path transformation failed");
                                return Ok(());
                            }
                        };
                        paint.transform(ts);

                        let clip_rect = tile.size().to_screen_int_rect(0, 0);
                        let mut subpix = match self.subpixmap(tile.to_int_rect()) {
                            Some(v) => v,
                            None => continue, // technically unreachable
                        };

                        let submask = mask.and_then(|mask| mask.submask(tile.to_int_rect()));
                        let mut blitter = match RasterPipelineBlitter::new_with_view(
                            paint,
                            submask,
                            &mut subpix,
                            control,
                        )? {
                            Some(v) => v,
                            None => continue, // nothing to do, all good
                        };

                        // We're ignoring "errors" here, because `fill_path` will return `None`
                        // when rendering a tile that doesn't have a path on it.
                        // Which is not an error in this case.
                        if paint.options.anti_alias {
                            control.check(0)?;
                            scan::path_aa::fill_path_with_control(
                                &path,
                                fill_rule,
                                &clip_rect,
                                &mut blitter,
                                control,
                            )?;
                        } else {
                            control.check(0)?;
                            scan::path::fill_path_with_control(
                                &path,
                                fill_rule,
                                &clip_rect,
                                &mut blitter,
                                control,
                            )?;
                        }

                        drop(blitter);
                        control.check(0)?;
                        let ts = Transform::from_translate(tile.x() as f32, tile.y() as f32);
                        path = match control.transform(path, ts)? {
                            Some(v) => v,
                            None => return Ok(()), // technically unreachable
                        };
                        paint.transform(ts);
                    }
                } else {
                    let clip_rect = self.size().to_screen_int_rect(0, 0);
                    let submask = mask.map(|mask| mask.as_submask());
                    let mut subpix = self.as_subpixmap();
                    let mut blitter = match RasterPipelineBlitter::new_with_view(
                        paint,
                        submask,
                        &mut subpix,
                        control,
                    )? {
                        Some(v) => v,
                        None => return Ok(()), // nothing to do, all good
                    };

                    if paint.options.anti_alias {
                        control.check(0)?;
                        scan::path_aa::fill_path_with_control(
                            path,
                            fill_rule,
                            &clip_rect,
                            &mut blitter,
                            control,
                        )?;
                    } else {
                        control.check(0)?;
                        scan::path::fill_path_with_control(
                            path,
                            fill_rule,
                            &clip_rect,
                            &mut blitter,
                            control,
                        )?;
                    }
                }
            } else {
                let path = match control.transform(control.copy(path)?, transform)? {
                    Some(v) => v,
                    None => {
                        log::warn!("path transformation failed");
                        return Ok(());
                    }
                };

                let mut paint = paint;
                paint.transform(transform);

                self.fill_path_with_control(
                    &path,
                    paint,
                    fill_rule,
                    Transform::identity(),
                    mask,
                    control,
                )?;
            }

            Ok(())
        })();
        control.finish(candidate)
    }

    /// Strokes a path.
    ///
    /// Stroking is implemented using two separate algorithms:
    ///
    /// 1. If a stroke width is wider than 1px (after applying the transformation),
    ///    a path will be converted into a stroked path and then filled using `fill_path`.
    ///    Which means that we have to allocate a separate `Path`, that can be 2-3x larger
    ///    then the original path.
    /// 2. If a stroke width is thinner than 1px (after applying the transformation),
    ///    we will use hairline stroking, which doesn't involve a separate path allocation.
    ///
    /// Also, if a `stroke` has a dash array, then path will be converted into
    /// a dashed path first and then stroked. Which means a yet another allocation.
    pub fn stroke_path(
        &mut self,
        path: &Path,
        paint: &Paint,
        stroke: &Stroke,
        transform: Transform,
        mask: Option<&Mask>,
    ) {
        let _ = self.stroke_path_with_control(
            path,
            PaintView::plain(paint),
            stroke,
            transform,
            mask,
            GeometryControl::ordinary(),
        );
    }

    /// Checked geometry variant; raster/AA/shader internals retain their original scope.
    pub fn try_stroke_path(
        &mut self,
        path: &Path,
        paint: &Paint,
        stroke: &Stroke,
        transform: Transform,
        mask: Option<&Mask>,
        resources: &ResourcesRef,
    ) -> Result<(), GeometryError> {
        let paint = CheckedPaint::try_from_plain(paint, resources)?;
        let candidate =
            self.try_stroke_path_checked(path, &paint, stroke, transform, mask, resources);
        drop(paint);
        GeometryControl::request(resources).finish(candidate)
    }
    /// Draw using immutable paint owned by this same request.
    pub fn try_stroke_path_checked(
        &mut self,
        path: &Path,
        paint: &CheckedPaint,
        stroke: &Stroke,
        transform: Transform,
        mask: Option<&Mask>,
        resources: &ResourcesRef,
    ) -> Result<(), GeometryError> {
        let view = paint.view(resources)?;
        self.stroke_path_with_control(
            path,
            view,
            stroke,
            transform,
            mask,
            GeometryControl::request(resources),
        )
    }

    fn stroke_path_with_control(
        &mut self,
        path: &Path,
        paint: PaintView<'_, '_, '_>,
        stroke: &Stroke,
        transform: Transform,
        mask: Option<&Mask>,
        control: GeometryControl<'_>,
    ) -> GeometryResult<()> {
        control.check(1)?;
        let candidate = (|| {
            control.source(path)?;
            if stroke.width < 0.0 {
                log::warn!("negative stroke width isn't allowed");
                return Ok(());
            }

            let res_scale = PathStroker::compute_resolution_scale(&transform);

            let dash_path;
            let path = if let Some(ref dash) = stroke.dash {
                dash_path = match control.dash(path, dash, res_scale)? {
                    Some(v) => v,
                    None => {
                        log::warn!("path dashing failed");
                        return Ok(());
                    }
                };
                &dash_path
            } else {
                path
            };

            if let Some(coverage) = treat_as_hairline(paint.options.anti_alias, stroke, transform) {
                let opacity;
                let mut paint = if coverage == 1.0 {
                    paint
                } else if paint.options.blend_mode.should_pre_scale_coverage() {
                    // This is the old technique, which we preserve for now so
                    // we don't change previous results (testing)
                    // the new way seems fine, its just (a tiny bit) different.
                    let scale = (coverage * 256.0) as i32;
                    let new_alpha = (255 * scale) >> 8;
                    opacity = [new_alpha as f32 / 255.0];
                    paint.with_opacity(&opacity)
                } else {
                    paint
                };

                if let Some(tiler) = DrawTiler::new(self.width(), self.height()) {
                    let mut path = control.copy(path)?; // TODO: avoid cloning
                    let mut paint = paint;

                    if !transform.is_identity() {
                        paint.transform(transform);
                        path = match control.transform(path, transform)? {
                            Some(v) => v,
                            None => {
                                log::warn!("path transformation failed");
                                return Ok(());
                            }
                        };
                    }

                    for tile in tiler {
                        control.check(1)?;
                        let ts = Transform::from_translate(-(tile.x() as f32), -(tile.y() as f32));
                        path = match control.transform(path, ts)? {
                            Some(v) => v,
                            None => {
                                log::warn!("path transformation failed");
                                return Ok(());
                            }
                        };
                        paint.transform(ts);

                        let mut subpix = match self.subpixmap(tile.to_int_rect()) {
                            Some(v) => v,
                            None => continue, // technically unreachable
                        };
                        let submask = mask.and_then(|mask| mask.submask(tile.to_int_rect()));

                        // We're ignoring "errors" here, because `stroke_hairline` will return `None`
                        // when rendering a tile that doesn't have a path on it.
                        // Which is not an error in this case.
                        control.check(0)?;
                        Self::stroke_hairline(
                            &path,
                            paint,
                            stroke.line_cap,
                            submask,
                            &mut subpix,
                            control,
                        )?;

                        let ts = Transform::from_translate(tile.x() as f32, tile.y() as f32);
                        path = match control.transform(path, ts)? {
                            Some(v) => v,
                            None => return Ok(()),
                        };
                        paint.transform(ts);
                    }
                } else {
                    let subpix = &mut self.as_subpixmap();
                    let submask = mask.map(|mask| mask.as_submask());
                    if !transform.is_identity() {
                        paint.transform(transform);

                        // TODO: avoid clone
                        let path = match control.transform(control.copy(path)?, transform)? {
                            Some(v) => v,
                            None => {
                                log::warn!("path transformation failed");
                                return Ok(());
                            }
                        };

                        control.check(0)?;
                        Self::stroke_hairline(
                            &path,
                            paint,
                            stroke.line_cap,
                            submask,
                            subpix,
                            control,
                        )?;
                    } else {
                        control.check(0)?;
                        Self::stroke_hairline(
                            path,
                            paint,
                            stroke.line_cap,
                            submask,
                            subpix,
                            control,
                        )?;
                    }
                }
            } else {
                let path = match control.stroke(path, stroke, res_scale)? {
                    Some(v) => v,
                    None => {
                        log::warn!("path stroking failed");
                        return Ok(());
                    }
                };

                self.fill_path_with_control(
                    &path,
                    paint,
                    FillRule::Winding,
                    transform,
                    mask,
                    control,
                )?;
            }

            Ok(())
        })();
        control.finish(candidate)
    }

    /// A stroking for paths with subpixel/hairline width.
    fn stroke_hairline(
        path: &Path,
        paint: PaintView<'_, '_, '_>,
        line_cap: LineCap,
        mask: Option<SubMaskRef>,
        pixmap: &mut SubPixmapMut,
        control: GeometryControl<'_>,
    ) -> GeometryResult<()> {
        control.check(0)?;
        let candidate = (|| {
            let clip = pixmap.size.to_screen_int_rect(0, 0);
            let mut blitter =
                match RasterPipelineBlitter::new_with_view(paint, mask, pixmap, control)? {
                    Some(v) => v,
                    None => return Ok(()), // nothing to do, all good
                };
            if paint.options.anti_alias {
                scan::hairline_aa::stroke_path_with_control(
                    path,
                    line_cap,
                    &clip,
                    &mut blitter,
                    control,
                )?;
                control.check(0)?;
            } else {
                scan::hairline::stroke_path_with_control(
                    path,
                    line_cap,
                    &clip,
                    &mut blitter,
                    control,
                )?;
            }
            Ok(())
        })();
        // Blitter-owned temporaries have exited before the authoritative tail.
        control.finish(candidate)
    }

    /// Draws a `Pixmap` on top of the current `Pixmap`.
    ///
    /// The same as filling a rectangle with a `pixmap` pattern.
    pub fn draw_pixmap(
        &mut self,
        x: i32,
        y: i32,
        pixmap: PixmapRef,
        paint: &PixmapPaint,
        transform: Transform,
        mask: Option<&Mask>,
    ) {
        let _ = self.draw_pixmap_with_control(
            x,
            y,
            pixmap,
            paint,
            transform,
            mask,
            GeometryControl::ordinary(),
        );
    }

    /// Checked geometry variant; raster/AA/shader internals retain their original scope.
    pub fn try_draw_pixmap(
        &mut self,
        x: i32,
        y: i32,
        pixmap: PixmapRef,
        paint: &PixmapPaint,
        transform: Transform,
        mask: Option<&Mask>,
        resources: &ResourcesRef,
    ) -> Result<(), GeometryError> {
        self.draw_pixmap_with_control(
            x,
            y,
            pixmap,
            paint,
            transform,
            mask,
            GeometryControl::request(resources),
        )
    }

    fn draw_pixmap_with_control(
        &mut self,
        x: i32,
        y: i32,
        pixmap: PixmapRef,
        paint: &PixmapPaint,
        transform: Transform,
        mask: Option<&Mask>,
        control: GeometryControl<'_>,
    ) -> GeometryResult<()> {
        control.check(1)?;
        let candidate = (|| {
            let rect = pixmap.size().to_int_rect(x, y).to_rect();

            // TODO: SkSpriteBlitter
            // TODO: partially clipped
            // TODO: clipped out

            // Translate pattern as well as bounds.
            let patt_transform = Transform::from_translate(x as f32, y as f32);

            let paint = Paint {
                shader: Pattern::new(
                    pixmap,
                    SpreadMode::Pad, // Pad, otherwise we will get weird borders overlap.
                    paint.quality,
                    paint.opacity,
                    patt_transform,
                ),
                blend_mode: paint.blend_mode,
                anti_alias: false,        // Skia doesn't use it too.
                force_hq_pipeline: false, // Pattern will use hq anyway.
                colorspace: ColorSpace::default(),
            };

            match control.resources() {
                Some(resources) => {
                    let checked = CheckedPaint::try_from_plain(&paint, resources)?;
                    let candidate = self.fill_rect_with_control(
                        rect,
                        checked.view(resources)?,
                        transform,
                        mask,
                        control,
                    );
                    drop(checked);
                    control.finish(candidate)?;
                }
                None => self.fill_rect_with_control(
                    rect,
                    PaintView::plain(&paint),
                    transform,
                    mask,
                    control,
                )?,
            }

            Ok(())
        })();
        control.finish(candidate)
    }

    /// Applies a masks.
    ///
    /// When a `Mask` is passed to drawing methods, it will be used to mask-out
    /// content we're about to draw.
    /// This method masks-out an already drawn content.
    /// It's not as fast, but can be useful when a mask is not available during drawing.
    ///
    /// This method is similar to filling the whole pixmap with an another,
    /// mask-like pixmap using the `DestinationOut` blend mode.
    ///
    /// `Mask` must have the same size as `Pixmap`. No transform or offset are allowed.
    pub fn apply_mask(&mut self, mask: &Mask) {
        self.apply_mask_ref(mask.as_ref());
    }

    /// Applies a borrowed alpha mask without cloning its backing storage.
    pub fn apply_mask_ref(&mut self, mask: crate::mask::MaskRef<'_>) {
        let _ = self.apply_mask_ref_with_control(mask, GeometryControl::ordinary());
    }
    /// Apply an existing mask through the original request's pixel pipeline.
    pub fn try_apply_mask_ref(
        &mut self,
        mask: crate::mask::MaskRef<'_>,
        resources: &ResourcesRef,
    ) -> Result<(), GeometryError> {
        self.apply_mask_ref_with_control(mask, GeometryControl::request(resources))
    }
    fn apply_mask_ref_with_control(
        &mut self,
        mask: crate::mask::MaskRef<'_>,
        control: GeometryControl<'_>,
    ) -> Result<(), GeometryError> {
        control.check(0)?;
        let candidate = (|| {
            if self.size() != mask.size() {
                log::warn!("Pixmap and Mask are expected to have the same size");
                return Ok(());
            }

            // Just a dummy.
            let pixmap_src = PixmapRef::from_bytes(&[0, 0, 0, 0], 1, 1).unwrap();

            let mut p = RasterPipelineBuilder::new_with_control(control)?;
            p.push(pipeline::Stage::LoadMaskU8);
            p.push(pipeline::Stage::LoadDestination);
            p.push(pipeline::Stage::DestinationIn);
            p.push(pipeline::Stage::Store);
            let mut p = p.compile()?;
            let rect = self.size().to_screen_int_rect(0, 0);
            p.run(
                &rect,
                pipeline::AAMaskCtx::default(),
                mask.as_submask().mask_ctx(),
                pixmap_src,
                &mut self.as_subpixmap(),
            )?;
            Ok(())
        })();
        control.finish(candidate)
    }
}

fn treat_as_hairline(anti_alias: bool, stroke: &Stroke, mut ts: Transform) -> Option<f32> {
    fn fast_len(p: Point) -> f32 {
        let mut x = p.x.abs();
        let mut y = p.y.abs();
        if x < y {
            core::mem::swap(&mut x, &mut y);
        }

        x + y.half()
    }

    debug_assert!(stroke.width >= 0.0);

    if stroke.width == 0.0 {
        return Some(1.0);
    }

    if !anti_alias {
        return None;
    }

    // We don't care about translate.
    ts.tx = 0.0;
    ts.ty = 0.0;

    // We need to try to fake a thick-stroke with a modulated hairline.
    let mut points = [
        Point::from_xy(stroke.width, 0.0),
        Point::from_xy(0.0, stroke.width),
    ];
    ts.map_points(&mut points);

    let len0 = fast_len(points[0]);
    let len1 = fast_len(points[1]);

    if len0 <= 1.0 && len1 <= 1.0 {
        return Some(len0.ave(len1));
    }

    None
}

/// Sometimes in the drawing pipeline, we have to perform math on path coordinates, even after
/// the path is in device-coordinates. Tessellation and clipping are two examples. Usually this
/// is pretty modest, but it can involve subtracting/adding coordinates, or multiplying by
/// small constants (e.g. 2,3,4). To try to preflight issues where these optionations could turn
/// finite path values into infinities (or NaNs), we allow the upper drawing code to reject
/// the path if its bounds (in device coordinates) is too close to max float.
pub(crate) fn is_too_big_for_math(path: &Path) -> bool {
    // This value is just a guess. smaller is safer, but we don't want to reject largish paths
    // that we don't have to.
    const SCALE_DOWN_TO_ALLOW_FOR_SMALL_MULTIPLIES: f32 = 0.25;
    const MAX: f32 = SCALAR_MAX * SCALE_DOWN_TO_ALLOW_FOR_SMALL_MULTIPLIES;

    let b = path.bounds();

    // use ! expression so we return true if bounds contains NaN
    !(b.left() >= -MAX && b.top() >= -MAX && b.right() <= MAX && b.bottom() <= MAX)
}

/// Splits the target pixmap into a list of tiles.
///
/// Skia/tiny-skia uses a lot of fixed-point math during path rendering.
/// Probably more for precision than performance.
/// And our fixed-point types are limited by 8192 and 32768.
/// Which means that we cannot render a path larger than 8192 onto a pixmap.
/// When pixmap is smaller than 8192, the path will be automatically clipped anyway,
/// but for large pixmaps we have to render in tiles.
pub(crate) struct DrawTiler {
    image_width: u32,
    image_height: u32,
    x_offset: u32,
    y_offset: u32,
    finished: bool,
}

impl DrawTiler {
    // 8K is 1 too big, since 8K << supersample == 32768 which is too big for Fixed.
    const MAX_DIMENSIONS: u32 = 8192 - 1;

    fn required(image_width: u32, image_height: u32) -> bool {
        image_width > Self::MAX_DIMENSIONS || image_height > Self::MAX_DIMENSIONS
    }

    pub(crate) fn new(image_width: u32, image_height: u32) -> Option<Self> {
        if Self::required(image_width, image_height) {
            Some(DrawTiler {
                image_width,
                image_height,
                x_offset: 0,
                y_offset: 0,
                finished: false,
            })
        } else {
            None
        }
    }
}

impl Iterator for DrawTiler {
    type Item = ScreenIntRect;

    fn next(&mut self) -> Option<Self::Item> {
        if self.finished {
            return None;
        }

        // TODO: iterate only over tiles that actually affected by the shape

        if self.x_offset < self.image_width && self.y_offset < self.image_height {
            let h = if self.y_offset < self.image_height {
                (self.image_height - self.y_offset).min(Self::MAX_DIMENSIONS)
            } else {
                self.image_height
            };

            let r = ScreenIntRect::from_xywh(
                self.x_offset,
                self.y_offset,
                (self.image_width - self.x_offset).min(Self::MAX_DIMENSIONS),
                h,
            );

            self.x_offset += Self::MAX_DIMENSIONS;
            if self.x_offset >= self.image_width {
                self.x_offset = 0;
                self.y_offset += Self::MAX_DIMENSIONS;
            }

            return r;
        }

        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    const MAX_DIM: u32 = DrawTiler::MAX_DIMENSIONS;

    #[test]
    fn skip() {
        assert!(DrawTiler::new(100, 500).is_none());
    }

    #[test]
    fn horizontal() {
        let mut iter = DrawTiler::new(10000, 500).unwrap();
        assert_eq!(iter.next(), ScreenIntRect::from_xywh(0, 0, MAX_DIM, 500));
        assert_eq!(
            iter.next(),
            ScreenIntRect::from_xywh(MAX_DIM, 0, 10000 - MAX_DIM, 500)
        );
        assert_eq!(iter.next(), None);
    }

    #[test]
    fn vertical() {
        let mut iter = DrawTiler::new(500, 10000).unwrap();
        assert_eq!(iter.next(), ScreenIntRect::from_xywh(0, 0, 500, MAX_DIM));
        assert_eq!(
            iter.next(),
            ScreenIntRect::from_xywh(0, MAX_DIM, 500, 10000 - MAX_DIM)
        );
        assert_eq!(iter.next(), None);
    }

    #[test]
    fn rect() {
        let mut iter = DrawTiler::new(10000, 10000).unwrap();
        // Row 1
        assert_eq!(
            iter.next(),
            ScreenIntRect::from_xywh(0, 0, MAX_DIM, MAX_DIM)
        );
        assert_eq!(
            iter.next(),
            ScreenIntRect::from_xywh(MAX_DIM, 0, 10000 - MAX_DIM, MAX_DIM)
        );
        // Row 2
        assert_eq!(
            iter.next(),
            ScreenIntRect::from_xywh(0, MAX_DIM, MAX_DIM, 10000 - MAX_DIM)
        );
        assert_eq!(
            iter.next(),
            ScreenIntRect::from_xywh(MAX_DIM, MAX_DIM, 10000 - MAX_DIM, 10000 - MAX_DIM)
        );
        assert_eq!(iter.next(), None);
    }
}

// Coverage segmentation remains 4096 as before. Read work uses independent
// <=256-event windows and never changes the original run/pair boundaries.
fn coverage_read(
    data: &[u8],
    index: usize,
    credit: &mut usize,
    control: GeometryControl,
) -> GeometryResult<u8> {
    if *credit == 0 {
        control.check(256)?;
        *credit = 256;
    }
    control.peek_slot()?;
    *credit -= 1;
    Ok(data[index])
}
