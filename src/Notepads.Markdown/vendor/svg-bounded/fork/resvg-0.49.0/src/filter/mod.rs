// Copyright 2018 the Resvg Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT
// Modifications Copyright (c) 2026, Jiaqi (0x7c13) Liu.
// Experimental request-owned error propagation; original license terms retained.

use crate::raster::{Raster, ScratchVec, SharedRaster};
use markdown_svg_budget::{Error as ResourceError, ResourcesRef};

use rgb::{FromSlice, RGBA8};
use tiny_skia::IntRect;
use usvg::{ApproxEqUlps, ApproxZeroUlps};

mod box_blur;
mod color_matrix;
mod component_transfer;
mod composite;
mod convolve_matrix;
mod displacement_map;
mod iir_blur;
mod lighting;
mod morphology;
mod turbulence;

// TODO: apply single primitive filters in-place

/// An image reference.
///
/// Image pixels should be stored in RGBA order.
///
/// Some filters will require premultiplied channels, some not.
/// See specific filter documentation for details.
#[derive(Clone, Copy)]
pub struct ImageRef<'a> {
    data: &'a [RGBA8],
    width: u32,
    height: u32,
}

impl<'a> ImageRef<'a> {
    /// Creates a new image reference.
    ///
    /// Doesn't clone the provided data.
    #[inline]
    pub fn new(width: u32, height: u32, data: &'a [RGBA8]) -> Self {
        ImageRef {
            data,
            width,
            height,
        }
    }

    #[inline]
    fn alpha_at(&self, x: u32, y: u32) -> i16 {
        self.data[(self.width * y + x) as usize].a as i16
    }
}

/// A mutable `ImageRef` variant.
pub struct ImageRefMut<'a> {
    resources: Option<ResourcesRef>,
    data: &'a mut [RGBA8],
    width: u32,
    height: u32,
}

impl<'a> ImageRefMut<'a> {
    /// Creates a new mutable image reference.
    ///
    /// Doesn't clone the provided data.
    #[inline]
    pub fn new(
        width: u32,
        height: u32,
        data: &'a mut [RGBA8],
        resources: Option<ResourcesRef>,
    ) -> Self {
        ImageRefMut {
            resources,
            data,
            width,
            height,
        }
    }

    #[inline]
    fn pixel_at(&self, x: u32, y: u32) -> RGBA8 {
        self.data[(self.width * y + x) as usize]
    }

    #[inline]
    fn pixel_at_mut(&mut self, x: u32, y: u32) -> &mut RGBA8 {
        &mut self.data[(self.width * y + x) as usize]
    }
}

// A prepaid bounded batch of real logical operations, sharing the request counter.
// Remaining credit never owns memory or creates a second work authority.
pub(crate) struct Work<'a> {
    resources: Option<&'a ResourcesRef>,
    credit: u16,
}
impl<'a> Work<'a> {
    fn new(resources: Option<&'a ResourcesRef>) -> Self {
        Self {
            resources,
            credit: 0,
        }
    }
    fn step(&mut self) -> bool {
        if let Some(resources) = self.resources {
            if resources.failure().is_some() {
                return false;
            }
            if self.credit == 0 {
                if resources.checkpoint(256).is_err() {
                    return false;
                }
                self.credit = 256;
            }
            self.credit -= 1;
        }
        true
    }
    fn checked(&mut self) -> Result<(), ResourceError> {
        if self.step() {
            Ok(())
        } else {
            Err(self
                .resources
                .expect("failed work has a request")
                .failure()
                .expect("failed work records the first error"))
        }
    }
}

#[derive(Debug)]
pub(crate) enum Error {
    InvalidRegion,
    NoResults,
    RequestFailed,
    Resource(ResourceError),
}

fn invalid_region(resources: Option<&ResourcesRef>) -> Error {
    if let Some(resources) = resources {
        resources.record(ResourceError::new(
            markdown_svg_budget::FailureKind::InvalidFilterRegion,
            markdown_svg_budget::LimitKind::None,
        ));
    }
    Error::InvalidRegion
}

impl From<ResourceError> for Error {
    fn from(error: ResourceError) -> Self {
        Self::Resource(error)
    }
}

trait PixmapExt: Sized {
    fn copy_region(&self, region: IntRect) -> Result<Raster, Error>;
    fn clear(&mut self);
    fn into_srgb(&mut self);
    fn into_linear_rgb(&mut self);
}

impl PixmapExt for Raster {
    fn copy_region(&self, region: IntRect) -> Result<Raster, Error> {
        let rect = IntRect::from_xywh(region.x(), region.y(), region.width(), region.height())
            .ok_or_else(|| invalid_region(self.resources()))?;
        self.try_copy_region(rect).map_err(Into::into)
    }

    fn clear(&mut self) {
        self.fill(tiny_skia::Color::TRANSPARENT);
    }

    fn into_srgb(&mut self) {
        let resources = self.resources().cloned();
        demultiply_alpha(self.data_mut().as_rgba_mut(), resources.as_ref());
        from_linear_rgb(self.data_mut().as_rgba_mut(), resources.as_ref());
        multiply_alpha(self.data_mut().as_rgba_mut(), resources.as_ref());
    }

    fn into_linear_rgb(&mut self) {
        let resources = self.resources().cloned();
        demultiply_alpha(self.data_mut().as_rgba_mut(), resources.as_ref());
        into_linear_rgb(self.data_mut().as_rgba_mut(), resources.as_ref());
        multiply_alpha(self.data_mut().as_rgba_mut(), resources.as_ref());
    }
}

/// Multiplies provided pixels alpha.
fn multiply_alpha(data: &mut [RGBA8], resources: Option<&ResourcesRef>) {
    let mut work = Work::new(resources);
    for p in data {
        if !work.step() {
            return;
        }
        let a = p.a as f32 / 255.0;
        p.b = (p.b as f32 * a + 0.5) as u8;
        p.g = (p.g as f32 * a + 0.5) as u8;
        p.r = (p.r as f32 * a + 0.5) as u8;
    }
}

/// Demultiplies provided pixels alpha.
fn demultiply_alpha(data: &mut [RGBA8], resources: Option<&ResourcesRef>) {
    let mut work = Work::new(resources);
    for p in data {
        if !work.step() {
            return;
        }
        let a = p.a as f32 / 255.0;
        p.b = (p.b as f32 / a + 0.5) as u8;
        p.g = (p.g as f32 / a + 0.5) as u8;
        p.r = (p.r as f32 / a + 0.5) as u8;
    }
}

/// Precomputed sRGB to LinearRGB table.
///
/// Since we are storing the result in `u8`, there is no need to compute those
/// values each time. Mainly because it's very expensive.
///
/// ```text
/// if (C_srgb <= 0.04045)
///     C_lin = C_srgb / 12.92;
///  else
///     C_lin = pow((C_srgb + 0.055) / 1.055, 2.4);
/// ```
///
/// Thanks to librsvg for the idea.
#[rustfmt::skip]
const SRGB_TO_LINEAR_RGB_TABLE: &[u8; 256] = &[
    0,   0,   0,   0,   0,   0,  0,    1,   1,   1,   1,   1,   1,   1,   1,   1,
    1,   1,   2,   2,   2,   2,  2,    2,   2,   2,   3,   3,   3,   3,   3,   3,
    4,   4,   4,   4,   4,   5,  5,    5,   5,   6,   6,   6,   6,   7,   7,   7,
    8,   8,   8,   8,   9,   9,  9,   10,  10,  10,  11,  11,  12,  12,  12,  13,
    13,  13,  14,  14,  15,  15,  16,  16,  17,  17,  17,  18,  18,  19,  19,  20,
    20,  21,  22,  22,  23,  23,  24,  24,  25,  25,  26,  27,  27,  28,  29,  29,
    30,  30,  31,  32,  32,  33,  34,  35,  35,  36,  37,  37,  38,  39,  40,  41,
    41,  42,  43,  44,  45,  45,  46,  47,  48,  49,  50,  51,  51,  52,  53,  54,
    55,  56,  57,  58,  59,  60,  61,  62,  63,  64,  65,  66,  67,  68,  69,  70,
    71,  72,  73,  74,  76,  77,  78,  79,  80,  81,  82,  84,  85,  86,  87,  88,
    90,  91,  92,  93,  95,  96,  97,  99, 100, 101, 103, 104, 105, 107, 108, 109,
    111, 112, 114, 115, 116, 118, 119, 121, 122, 124, 125, 127, 128, 130, 131, 133,
    134, 136, 138, 139, 141, 142, 144, 146, 147, 149, 151, 152, 154, 156, 157, 159,
    161, 163, 164, 166, 168, 170, 171, 173, 175, 177, 179, 181, 183, 184, 186, 188,
    190, 192, 194, 196, 198, 200, 202, 204, 206, 208, 210, 212, 214, 216, 218, 220,
    222, 224, 226, 229, 231, 233, 235, 237, 239, 242, 244, 246, 248, 250, 253, 255,
];

/// Precomputed LinearRGB to sRGB table.
///
/// Since we are storing the result in `u8`, there is no need to compute those
/// values each time. Mainly because it's very expensive.
///
/// ```text
/// if (C_lin <= 0.0031308)
///     C_srgb = C_lin * 12.92;
/// else
///     C_srgb = 1.055 * pow(C_lin, 1.0 / 2.4) - 0.055;
/// ```
///
/// Thanks to librsvg for the idea.
#[rustfmt::skip]
const LINEAR_RGB_TO_SRGB_TABLE: &[u8; 256] = &[
    0,  13,  22,  28,  34,  38,  42,  46,  50,  53,  56,  59,  61,  64,  66,  69,
    71,  73,  75,  77,  79,  81,  83,  85,  86,  88,  90,  92,  93,  95,  96,  98,
    99, 101, 102, 104, 105, 106, 108, 109, 110, 112, 113, 114, 115, 117, 118, 119,
    120, 121, 122, 124, 125, 126, 127, 128, 129, 130, 131, 132, 133, 134, 135, 136,
    137, 138, 139, 140, 141, 142, 143, 144, 145, 146, 147, 148, 148, 149, 150, 151,
    152, 153, 154, 155, 155, 156, 157, 158, 159, 159, 160, 161, 162, 163, 163, 164,
    165, 166, 167, 167, 168, 169, 170, 170, 171, 172, 173, 173, 174, 175, 175, 176,
    177, 178, 178, 179, 180, 180, 181, 182, 182, 183, 184, 185, 185, 186, 187, 187,
    188, 189, 189, 190, 190, 191, 192, 192, 193, 194, 194, 195, 196, 196, 197, 197,
    198, 199, 199, 200, 200, 201, 202, 202, 203, 203, 204, 205, 205, 206, 206, 207,
    208, 208, 209, 209, 210, 210, 211, 212, 212, 213, 213, 214, 214, 215, 215, 216,
    216, 217, 218, 218, 219, 219, 220, 220, 221, 221, 222, 222, 223, 223, 224, 224,
    225, 226, 226, 227, 227, 228, 228, 229, 229, 230, 230, 231, 231, 232, 232, 233,
    233, 234, 234, 235, 235, 236, 236, 237, 237, 238, 238, 238, 239, 239, 240, 240,
    241, 241, 242, 242, 243, 243, 244, 244, 245, 245, 246, 246, 246, 247, 247, 248,
    248, 249, 249, 250, 250, 251, 251, 251, 252, 252, 253, 253, 254, 254, 255, 255,
];

/// Converts input pixel from sRGB into LinearRGB.
///
/// Provided pixels should have an **unpremultiplied alpha**.
///
/// RGB channels order of the input image doesn't matter, but alpha channel must be the last one.
fn into_linear_rgb(data: &mut [RGBA8], resources: Option<&ResourcesRef>) {
    let mut work = Work::new(resources);
    for p in data {
        if !work.step() {
            return;
        }
        p.r = SRGB_TO_LINEAR_RGB_TABLE[p.r as usize];
        p.g = SRGB_TO_LINEAR_RGB_TABLE[p.g as usize];
        p.b = SRGB_TO_LINEAR_RGB_TABLE[p.b as usize];
    }
}

/// Converts input pixel from LinearRGB into sRGB.
///
/// Provided pixels should have an **unpremultiplied alpha**.
///
/// RGB channels order of the input image doesn't matter, but alpha channel must be the last one.
fn from_linear_rgb(data: &mut [RGBA8], resources: Option<&ResourcesRef>) {
    let mut work = Work::new(resources);
    for p in data {
        if !work.step() {
            return;
        }
        p.r = LINEAR_RGB_TO_SRGB_TABLE[p.r as usize];
        p.g = LINEAR_RGB_TO_SRGB_TABLE[p.g as usize];
        p.b = LINEAR_RGB_TO_SRGB_TABLE[p.b as usize];
    }
}

// TODO: https://github.com/rust-lang/rust/issues/44095
#[inline]
fn f32_bound(min: f32, val: f32, max: f32) -> f32 {
    debug_assert!(min.is_finite());
    debug_assert!(max.is_finite());

    if val > max {
        max
    } else if val >= min {
        val
    } else {
        // Catches `val < min` as well as a NaN `val`.
        min
    }
}

#[derive(Clone)]
struct Image {
    /// Filter primitive result.
    ///
    /// All images have the same size which is equal to the current filter region.
    image: SharedRaster,

    /// Image's region that has actual data.
    ///
    /// Region is in global coordinates and not in `image` one.
    ///
    /// Image's content outside this region will be transparent/cleared.
    ///
    /// Currently used only for `feTile`.
    region: IntRect,

    /// The current color space.
    color_space: usvg::filter::ColorInterpolation,
}

impl Image {
    fn from_image(
        image: Raster,
        color_space: usvg::filter::ColorInterpolation,
    ) -> Result<Self, Error> {
        let (w, h) = (image.width(), image.height());
        Ok(Image {
            image: SharedRaster::try_new(image)?,
            region: IntRect::from_xywh(0, 0, w, h).unwrap(),
            color_space,
        })
    }

    fn into_color_space(
        self,
        color_space: usvg::filter::ColorInterpolation,
    ) -> Result<Self, Error> {
        if color_space != self.color_space {
            let region = self.region;

            let mut image = self.take()?;

            match color_space {
                usvg::filter::ColorInterpolation::SRGB => image.into_srgb(),
                usvg::filter::ColorInterpolation::LinearRGB => image.into_linear_rgb(),
            }

            Ok(Image {
                image: SharedRaster::try_new(image)?,
                region,
                color_space,
            })
        } else {
            Ok(self)
        }
    }

    fn take(self) -> Result<Raster, Error> {
        self.image.take().map_err(Into::into)
    }

    fn width(&self) -> u32 {
        self.image.width()
    }

    fn height(&self) -> u32 {
        self.image.height()
    }

    fn as_ref(&self) -> &Raster {
        self.image.as_ref()
    }
}

struct FilterResult<'a> {
    name: &'a str,
    image: Image,
}

pub fn apply(
    filter: &usvg::filter::Filter,
    ctx: &crate::render::Context,
    ts: tiny_skia::Transform,
    source: &mut Raster,
) {
    if ctx.processing.failed() {
        return;
    }
    let result = apply_inner(filter, ctx, ts, source);
    let result = result.and_then(|(image, region)| apply_to_canvas(image, region, source));

    // A request error never becomes transparent success. Only a proven empty
    // filter result is normal transparent output; no work continues after fault.
    if ctx.processing.failed() {
        return;
    }
    match result {
        Ok(_) => {}
        Err(Error::InvalidRegion) => {
            ctx.processing
                .fail(usvg::ProcessingFailure::InvalidFilterRegion);
        }
        Err(Error::RequestFailed) => {}
        Err(Error::Resource(error)) => {
            ctx.processing.record_resource(error);
        }
        Err(Error::NoResults) => source.fill(tiny_skia::Color::TRANSPARENT),
    }
}

fn apply_inner(
    filter: &usvg::filter::Filter,
    ctx: &crate::render::Context,
    ts: usvg::Transform,
    source: &Raster,
) -> Result<(Image, IntRect), Error> {
    let region = filter
        .rect()
        .transform(ts)
        .map(|r| r.to_int_rect())
        .ok_or_else(|| {
            ctx.processing
                .fail(usvg::ProcessingFailure::InvalidFilterRegion);
            Error::InvalidRegion
        })?;

    // The source pixmap is clamped to `max_bbox` in `render_group`, so the filter
    // region (derived directly from the unclamped filter rect) can be larger than
    // the buffer we actually render into.
    let source_rect =
        IntRect::from_xywh(0, 0, source.width(), source.height()).ok_or_else(|| {
            ctx.processing
                .fail(usvg::ProcessingFailure::InvalidFilterRegion);
            Error::InvalidRegion
        })?;
    let region = crate::geom::fit_to_rect(region, source_rect).ok_or(Error::NoResults)?;

    let cropped_source = if region != source_rect {
        // TODO: Avoid copying?
        Some(source.copy_region(region)?)
    } else {
        None
    };
    let source = cropped_source.as_ref().unwrap_or(source);
    let output_region = region;
    let ts = ts.post_translate(-region.x() as f32, -region.y() as f32);
    let region = region.translate_to(0, 0).ok_or_else(|| {
        ctx.processing
            .fail(usvg::ProcessingFailure::InvalidFilterRegion);
        Error::InvalidRegion
    })?;

    let mut results: ScratchVec<FilterResult<'_>> = ScratchVec::new(source.resources());

    let mut graph_work = Work::new(source.resources());
    for primitive in filter.primitives() {
        graph_work.checked()?;
        if ctx.processing.failed() {
            return Err(Error::RequestFailed);
        }
        let mut subregion = primitive
            .rect()
            .transform(ts)
            .map(|r| r.to_int_rect())
            .ok_or_else(|| {
                ctx.processing
                    .fail(usvg::ProcessingFailure::InvalidFilterRegion);
                Error::InvalidRegion
            })?;

        // `feOffset` inherits its region from the input.
        if let usvg::filter::Kind::Offset(fe) = primitive.kind() {
            if let usvg::filter::Input::Reference(name) = fe.input() {
                for res in results.iter().rev() {
                    graph_work.checked()?;
                    if res.name == name.as_str() {
                        subregion = res.image.region;
                        break;
                    }
                }
            }
        }

        let cs = primitive.color_interpolation();

        let mut result = match primitive.kind() {
            usvg::filter::Kind::Blend(fe) => {
                let input1 = get_input(fe.input1(), region, source, &results)?;
                let input2 = get_input(fe.input2(), region, source, &results)?;
                apply_blend(fe, cs, region, input1, input2)
            }
            usvg::filter::Kind::DropShadow(fe) => {
                let input = get_input(fe.input(), region, source, &results)?;
                apply_drop_shadow(fe, cs, ts, input)
            }
            usvg::filter::Kind::Flood(fe) => apply_flood(fe, region, ctx.processing.resources()),
            usvg::filter::Kind::GaussianBlur(fe) => {
                let input = get_input(fe.input(), region, source, &results)?;
                apply_blur(fe, cs, ts, input)
            }
            usvg::filter::Kind::Offset(fe) => {
                let input = get_input(fe.input(), region, source, &results)?;
                apply_offset(fe, ts, input)
            }
            usvg::filter::Kind::Composite(fe) => {
                let input1 = get_input(fe.input1(), region, source, &results)?;
                let input2 = get_input(fe.input2(), region, source, &results)?;
                apply_composite(fe, cs, region, input1, input2)
            }
            usvg::filter::Kind::Merge(fe) => apply_merge(fe, cs, region, source, &results),
            usvg::filter::Kind::Tile(fe) => {
                let input = get_input(fe.input(), region, source, &results)?;
                apply_tile(input, region)
            }
            usvg::filter::Kind::Image(fe) => apply_image(fe, ctx, region, subregion, ts),
            usvg::filter::Kind::ComponentTransfer(fe) => {
                let input = get_input(fe.input(), region, source, &results)?;
                apply_component_transfer(fe, cs, input)
            }
            usvg::filter::Kind::ColorMatrix(fe) => {
                let input = get_input(fe.input(), region, source, &results)?;
                apply_color_matrix(fe, cs, input)
            }
            usvg::filter::Kind::ConvolveMatrix(fe) => {
                let input = get_input(fe.input(), region, source, &results)?;
                apply_convolve_matrix(fe, cs, input)
            }
            usvg::filter::Kind::Morphology(fe) => {
                let input = get_input(fe.input(), region, source, &results)?;
                apply_morphology(fe, cs, ts, input)
            }
            usvg::filter::Kind::DisplacementMap(fe) => {
                let input1 = get_input(fe.input1(), region, source, &results)?;
                let input2 = get_input(fe.input2(), region, source, &results)?;
                apply_displacement_map(fe, region, cs, ts, input1, input2)
            }
            usvg::filter::Kind::Turbulence(fe) => {
                apply_turbulence(fe, region, cs, ts, ctx.processing.resources())
            }
            usvg::filter::Kind::DiffuseLighting(fe) => {
                let input = get_input(fe.input(), region, source, &results)?;
                apply_diffuse_lighting(fe, region, cs, ts, input)
            }
            usvg::filter::Kind::SpecularLighting(fe) => {
                let input = get_input(fe.input(), region, source, &results)?;
                apply_specular_lighting(fe, region, cs, ts, input)
            }
        }?;

        if region != subregion {
            // Clip result.

            // TODO: explain
            let subregion2 = if let usvg::filter::Kind::Offset(..) = primitive.kind() {
                // We do not support clipping on feOffset.
                region.translate_to(0, 0)
            } else {
                subregion.translate(-region.x(), -region.y())
            }
            .ok_or_else(|| {
                ctx.processing
                    .fail(usvg::ProcessingFailure::InvalidFilterRegion);
                Error::InvalidRegion
            })?;

            let color_space = result.color_space;

            let pixmap = {
                // This is cropping by clearing the pixels outside the region.
                let mut paint = tiny_skia::Paint::default();
                paint.set_color(tiny_skia::Color::BLACK);
                paint.blend_mode = tiny_skia::BlendMode::Clear;

                let mut pixmap = result.take()?;
                let w = pixmap.width() as f32;
                let h = pixmap.height() as f32;

                if let Some(rect) = tiny_skia::Rect::from_xywh(0.0, 0.0, w, subregion2.y() as f32) {
                    pixmap.fill_rect(rect, &paint, tiny_skia::Transform::identity(), None);
                }

                if let Some(rect) = tiny_skia::Rect::from_xywh(0.0, 0.0, subregion2.x() as f32, h) {
                    pixmap.fill_rect(rect, &paint, tiny_skia::Transform::identity(), None);
                }

                if let Some(rect) = tiny_skia::Rect::from_xywh(subregion2.right() as f32, 0.0, w, h)
                {
                    pixmap.fill_rect(rect, &paint, tiny_skia::Transform::identity(), None);
                }

                if let Some(rect) =
                    tiny_skia::Rect::from_xywh(0.0, subregion2.bottom() as f32, w, h)
                {
                    pixmap.fill_rect(rect, &paint, tiny_skia::Transform::identity(), None);
                }

                pixmap
            };

            result = Image {
                image: SharedRaster::try_new(pixmap)?,
                region: subregion,
                color_space,
            };
        }

        results.try_push(FilterResult {
            name: primitive.result(),
            image: result,
        })?;
    }

    if let Some(res) = results.pop() {
        Ok((res.image, output_region))
    } else {
        Err(Error::NoResults)
    }
}

fn get_input(
    input: &usvg::filter::Input,
    region: IntRect,
    source: &Raster,
    results: &[FilterResult],
) -> Result<Image, Error> {
    match input {
        usvg::filter::Input::SourceGraphic => {
            let image = source.try_clone()?;

            Ok(Image {
                image: SharedRaster::try_new(image)?,
                region,
                color_space: usvg::filter::ColorInterpolation::SRGB,
            })
        }
        usvg::filter::Input::SourceAlpha => {
            let mut image = source.try_clone()?;
            // Set RGB to black. Keep alpha as is.
            let resources = image.resources().cloned();
            let mut work = Work::new(resources.as_ref());
            for p in image.data_mut().as_rgba_mut() {
                work.checked()?;
                p.r = 0;
                p.g = 0;
                p.b = 0;
            }

            Ok(Image {
                image: SharedRaster::try_new(image)?,
                region,
                color_space: usvg::filter::ColorInterpolation::SRGB,
            })
        }
        usvg::filter::Input::Reference(name) => {
            let mut work = Work::new(source.resources());
            for v in results.iter().rev() {
                work.checked()?;
                if v.name == name.as_str() {
                    return Ok(v.image.clone());
                }
            }
            // Preserve the original fallback semantics for a missing reference.
            log::warn!("Unknown filter primitive reference '{}'.", name);
            get_input(&usvg::filter::Input::SourceGraphic, region, source, results)
        }
    }
}

trait PixmapToImageRef<'a> {
    fn as_image_ref(&'a self) -> ImageRef<'a>;
    fn as_image_ref_mut(&'a mut self) -> ImageRefMut<'a>;
}

impl<'a> PixmapToImageRef<'a> for Raster {
    fn as_image_ref(&'a self) -> ImageRef<'a> {
        ImageRef::new(self.width(), self.height(), self.data().as_rgba())
    }

    fn as_image_ref_mut(&'a mut self) -> ImageRefMut<'a> {
        let resources = self.resources().cloned();
        ImageRefMut::new(
            self.width(),
            self.height(),
            self.data_mut().as_rgba_mut(),
            resources,
        )
    }
}

fn apply_drop_shadow(
    fe: &usvg::filter::DropShadow,
    cs: usvg::filter::ColorInterpolation,
    ts: usvg::Transform,
    input: Image,
) -> Result<Image, Error> {
    let resources = input.as_ref().resources().cloned();
    let (dx, dy) = match scale_coordinates(fe.dx(), fe.dy(), ts) {
        Some(v) => v,
        None => return Ok(input),
    };

    let mut pixmap = Raster::try_create(input.width(), input.height(), resources.as_ref())?;
    let input_pixmap = input.into_color_space(cs)?.take()?;
    let mut shadow_pixmap = input_pixmap.try_clone()?;

    if let Some((std_dx, std_dy, use_box_blur)) = resolve_std_dev(
        fe.std_dev_x().get(),
        fe.std_dev_y().get(),
        ts,
        resources.as_ref(),
    )? {
        if use_box_blur {
            box_blur::apply(std_dx, std_dy, shadow_pixmap.as_image_ref_mut());
        } else {
            iir_blur::apply(std_dx, std_dy, shadow_pixmap.as_image_ref_mut());
        }
    }

    // flood
    let color = tiny_skia::Color::from_rgba8(
        fe.color().red,
        fe.color().green,
        fe.color().blue,
        fe.opacity().to_u8(),
    );
    let mut work = Work::new(resources.as_ref());
    for p in shadow_pixmap.pixels_mut() {
        work.checked()?;
        let mut color = color;
        color.apply_opacity(p.alpha() as f32 / 255.0);
        *p = color.premultiply().to_color_u8();
    }

    match cs {
        usvg::filter::ColorInterpolation::SRGB => shadow_pixmap.into_srgb(),
        usvg::filter::ColorInterpolation::LinearRGB => shadow_pixmap.into_linear_rgb(),
    }

    pixmap.draw_pixmap(
        dx as i32,
        dy as i32,
        shadow_pixmap.as_ref(),
        &tiny_skia::PixmapPaint::default(),
        tiny_skia::Transform::identity(),
        None,
    );

    pixmap.draw_pixmap(
        0,
        0,
        input_pixmap.as_ref(),
        &tiny_skia::PixmapPaint::default(),
        tiny_skia::Transform::identity(),
        None,
    );

    Image::from_image(pixmap, cs)
}

fn apply_blur(
    fe: &usvg::filter::GaussianBlur,
    cs: usvg::filter::ColorInterpolation,
    ts: usvg::Transform,
    input: Image,
) -> Result<Image, Error> {
    let resources = input.as_ref().resources().cloned();
    let (std_dx, std_dy, use_box_blur) = match resolve_std_dev(
        fe.std_dev_x().get(),
        fe.std_dev_y().get(),
        ts,
        resources.as_ref(),
    )? {
        Some(v) => v,
        None => return Ok(input),
    };

    let mut pixmap = input.into_color_space(cs)?.take()?;

    if use_box_blur {
        box_blur::apply(std_dx, std_dy, pixmap.as_image_ref_mut());
    } else {
        iir_blur::apply(std_dx, std_dy, pixmap.as_image_ref_mut());
    }

    Image::from_image(pixmap, cs)
}

fn apply_offset(
    fe: &usvg::filter::Offset,
    ts: usvg::Transform,
    input: Image,
) -> Result<Image, Error> {
    let resources = input.as_ref().resources().cloned();
    let (dx, dy) = match scale_coordinates(fe.dx(), fe.dy(), ts) {
        Some(v) => v,
        None => return Ok(input),
    };

    if dx.approx_zero_ulps(4) && dy.approx_zero_ulps(4) {
        return Ok(input);
    }

    let mut pixmap = Raster::try_create(input.width(), input.height(), resources.as_ref())?;
    pixmap.draw_pixmap(
        dx as i32,
        dy as i32,
        input.as_ref().as_ref(),
        &tiny_skia::PixmapPaint::default(),
        tiny_skia::Transform::identity(),
        None,
    );

    Image::from_image(pixmap, input.color_space)
}

fn apply_blend(
    fe: &usvg::filter::Blend,
    cs: usvg::filter::ColorInterpolation,
    region: IntRect,
    input1: Image,
    input2: Image,
) -> Result<Image, Error> {
    let resources = input1.as_ref().resources().cloned();
    let input1 = input1.into_color_space(cs)?;
    let input2 = input2.into_color_space(cs)?;

    let mut pixmap = Raster::try_create(region.width(), region.height(), resources.as_ref())?;

    pixmap.draw_pixmap(
        0,
        0,
        input2.as_ref().as_ref(),
        &tiny_skia::PixmapPaint::default(),
        tiny_skia::Transform::identity(),
        None,
    );

    pixmap.draw_pixmap(
        0,
        0,
        input1.as_ref().as_ref(),
        &tiny_skia::PixmapPaint {
            blend_mode: crate::render::convert_blend_mode(fe.mode()),
            ..tiny_skia::PixmapPaint::default()
        },
        tiny_skia::Transform::identity(),
        None,
    );

    Image::from_image(pixmap, cs)
}

fn apply_composite(
    fe: &usvg::filter::Composite,
    cs: usvg::filter::ColorInterpolation,
    region: IntRect,
    input1: Image,
    input2: Image,
) -> Result<Image, Error> {
    let resources = input1.as_ref().resources().cloned();
    use usvg::filter::CompositeOperator as Operator;

    let input1 = input1.into_color_space(cs)?;
    let input2 = input2.into_color_space(cs)?;

    let mut pixmap = Raster::try_create(region.width(), region.height(), resources.as_ref())?;

    if let Operator::Arithmetic { k1, k2, k3, k4 } = fe.operator() {
        let pixmap1 = input1.take()?;
        let pixmap2 = input2.take()?;

        composite::arithmetic(
            k1,
            k2,
            k3,
            k4,
            pixmap1.as_image_ref(),
            pixmap2.as_image_ref(),
            pixmap.as_image_ref_mut(),
        );

        return Image::from_image(pixmap, cs);
    }

    pixmap.draw_pixmap(
        0,
        0,
        input2.as_ref().as_ref(),
        &tiny_skia::PixmapPaint::default(),
        tiny_skia::Transform::identity(),
        None,
    );

    let blend_mode = match fe.operator() {
        Operator::Over => tiny_skia::BlendMode::SourceOver,
        Operator::In => tiny_skia::BlendMode::SourceIn,
        Operator::Out => tiny_skia::BlendMode::SourceOut,
        Operator::Atop => tiny_skia::BlendMode::SourceAtop,
        Operator::Xor => tiny_skia::BlendMode::Xor,
        Operator::Lighter => tiny_skia::BlendMode::Plus,
        Operator::Arithmetic { .. } => tiny_skia::BlendMode::SourceOver,
    };

    pixmap.draw_pixmap(
        0,
        0,
        input1.as_ref().as_ref(),
        &tiny_skia::PixmapPaint {
            blend_mode,
            ..tiny_skia::PixmapPaint::default()
        },
        tiny_skia::Transform::identity(),
        None,
    );

    Image::from_image(pixmap, cs)
}

fn apply_merge(
    fe: &usvg::filter::Merge,
    cs: usvg::filter::ColorInterpolation,
    region: IntRect,
    source: &Raster,
    results: &[FilterResult],
) -> Result<Image, Error> {
    let resources = source.resources().cloned();
    let mut pixmap = Raster::try_create(region.width(), region.height(), resources.as_ref())?;

    let mut graph_work = Work::new(source.resources());
    for input in fe.inputs() {
        graph_work.checked()?;
        let input = get_input(input, region, source, results)?;
        let input = input.into_color_space(cs)?;
        pixmap.draw_pixmap(
            0,
            0,
            input.as_ref().as_ref(),
            &tiny_skia::PixmapPaint::default(),
            tiny_skia::Transform::identity(),
            None,
        );
    }

    Image::from_image(pixmap, cs)
}

fn apply_flood(
    fe: &usvg::filter::Flood,
    region: IntRect,
    resources: Option<&ResourcesRef>,
) -> Result<Image, Error> {
    let resources = resources.cloned();
    let c = fe.color();

    let mut pixmap = Raster::try_create(region.width(), region.height(), resources.as_ref())?;
    pixmap.fill(tiny_skia::Color::from_rgba8(
        c.red,
        c.green,
        c.blue,
        fe.opacity().to_u8(),
    ));

    Image::from_image(pixmap, usvg::filter::ColorInterpolation::SRGB)
}

fn apply_tile(input: Image, region: IntRect) -> Result<Image, Error> {
    let resources = input.as_ref().resources().cloned();
    let subregion = input.region.translate(-region.x(), -region.y()).unwrap();

    let tile_pixmap = input.image.copy_region(subregion)?;
    let mut paint = tiny_skia::Paint::default();
    paint.shader = tiny_skia::Pattern::new(
        tile_pixmap.as_ref(),
        tiny_skia::SpreadMode::Repeat,
        tiny_skia::FilterQuality::Bicubic,
        1.0,
        tiny_skia::Transform::from_translate(subregion.x() as f32, subregion.y() as f32),
    );

    let mut pixmap = Raster::try_create(region.width(), region.height(), resources.as_ref())?;
    let rect = tiny_skia::Rect::from_xywh(0.0, 0.0, region.width() as f32, region.height() as f32)
        .unwrap();
    pixmap.fill_rect(rect, &paint, tiny_skia::Transform::identity(), None);

    Image::from_image(pixmap, usvg::filter::ColorInterpolation::SRGB)
}

fn apply_image(
    fe: &usvg::filter::Image,
    parent_ctx: &crate::render::Context,
    region: IntRect,
    subregion: IntRect,
    ts: usvg::Transform,
) -> Result<Image, Error> {
    let resources = parent_ctx.processing.resources().cloned();
    let mut pixmap = Raster::try_create(region.width(), region.height(), resources.as_ref())?;

    let (sx, sy) = ts.get_scale();
    let transform = tiny_skia::Transform::from_row(
        sx,
        0.0,
        0.0,
        sy,
        subregion.x() as f32,
        subregion.y() as f32,
    );

    let ctx = crate::render::Context {
        max_bbox: tiny_skia::IntRect::from_xywh(0, 0, region.width(), region.height()).unwrap(),
        processing: parent_ctx.processing.clone(),
        coverage: parent_ctx.coverage,
    };

    crate::render::render_nodes(fe.root(), &ctx, transform, &mut pixmap.as_mut());
    if ctx.processing.failed() {
        return Err(Error::RequestFailed);
    }

    Image::from_image(pixmap, usvg::filter::ColorInterpolation::SRGB)
}

fn apply_component_transfer(
    fe: &usvg::filter::ComponentTransfer,
    cs: usvg::filter::ColorInterpolation,
    input: Image,
) -> Result<Image, Error> {
    let resources = input.as_ref().resources().cloned();
    let mut pixmap = input.into_color_space(cs)?.take()?;

    demultiply_alpha(pixmap.data_mut().as_rgba_mut(), resources.as_ref());
    component_transfer::apply(fe, pixmap.as_image_ref_mut());
    multiply_alpha(pixmap.data_mut().as_rgba_mut(), resources.as_ref());

    Image::from_image(pixmap, cs)
}

fn apply_color_matrix(
    fe: &usvg::filter::ColorMatrix,
    cs: usvg::filter::ColorInterpolation,
    input: Image,
) -> Result<Image, Error> {
    let resources = input.as_ref().resources().cloned();
    let mut pixmap = input.into_color_space(cs)?.take()?;

    demultiply_alpha(pixmap.data_mut().as_rgba_mut(), resources.as_ref());
    color_matrix::apply(fe.kind(), pixmap.as_image_ref_mut());
    multiply_alpha(pixmap.data_mut().as_rgba_mut(), resources.as_ref());

    Image::from_image(pixmap, cs)
}

fn apply_convolve_matrix(
    fe: &usvg::filter::ConvolveMatrix,
    cs: usvg::filter::ColorInterpolation,
    input: Image,
) -> Result<Image, Error> {
    let resources = input.as_ref().resources().cloned();
    let mut pixmap = input.into_color_space(cs)?.take()?;

    if fe.preserve_alpha() {
        demultiply_alpha(pixmap.data_mut().as_rgba_mut(), resources.as_ref());
    }

    convolve_matrix::apply(fe, pixmap.as_image_ref_mut());

    Image::from_image(pixmap, cs)
}

fn apply_morphology(
    fe: &usvg::filter::Morphology,
    cs: usvg::filter::ColorInterpolation,
    ts: usvg::Transform,
    input: Image,
) -> Result<Image, Error> {
    let mut pixmap = input.into_color_space(cs)?.take()?;

    let (rx, ry) = match scale_coordinates(fe.radius_x().get(), fe.radius_y().get(), ts) {
        Some(v) => v,
        None => return Image::from_image(pixmap, cs),
    };

    if !(rx > 0.0 && ry > 0.0) {
        pixmap.clear();
        return Image::from_image(pixmap, cs);
    }

    morphology::apply(fe.operator(), rx, ry, pixmap.as_image_ref_mut());

    Image::from_image(pixmap, cs)
}

fn apply_displacement_map(
    fe: &usvg::filter::DisplacementMap,
    region: IntRect,
    cs: usvg::filter::ColorInterpolation,
    ts: usvg::Transform,
    input1: Image,
    input2: Image,
) -> Result<Image, Error> {
    let resources = input1.as_ref().resources().cloned();
    let pixmap1 = input1.into_color_space(cs)?.take()?;
    let pixmap2 = input2.into_color_space(cs)?.take()?;

    let mut pixmap = Raster::try_create(region.width(), region.height(), resources.as_ref())?;

    let (sx, sy) = match scale_coordinates(fe.scale(), fe.scale(), ts) {
        Some(v) => v,
        None => return Image::from_image(pixmap1, cs),
    };

    displacement_map::apply(
        fe,
        sx,
        sy,
        pixmap1.as_image_ref(),
        pixmap2.as_image_ref(),
        pixmap.as_image_ref_mut(),
    );

    Image::from_image(pixmap, cs)
}

fn apply_turbulence(
    fe: &usvg::filter::Turbulence,
    region: IntRect,
    cs: usvg::filter::ColorInterpolation,
    ts: usvg::Transform,
    resources: Option<&ResourcesRef>,
) -> Result<Image, Error> {
    let resources = resources.cloned();
    let mut pixmap = Raster::try_create(region.width(), region.height(), resources.as_ref())?;

    let (sx, sy) = finite_scale(ts, resources.as_ref())?;
    if sx.approx_zero_ulps(4) || sy.approx_zero_ulps(4) {
        return Image::from_image(pixmap, cs);
    }

    turbulence::apply(
        region.x() as f64 - ts.tx as f64,
        region.y() as f64 - ts.ty as f64,
        sx as f64,
        sy as f64,
        fe.base_frequency_x().get() as f64,
        fe.base_frequency_y().get() as f64,
        fe.num_octaves(),
        fe.seed(),
        fe.stitch_tiles(),
        fe.kind() == usvg::filter::TurbulenceKind::FractalNoise,
        pixmap.as_image_ref_mut(),
    );

    multiply_alpha(pixmap.data_mut().as_rgba_mut(), resources.as_ref());

    Image::from_image(pixmap, cs)
}

fn apply_diffuse_lighting(
    fe: &usvg::filter::DiffuseLighting,
    region: IntRect,
    cs: usvg::filter::ColorInterpolation,
    ts: usvg::Transform,
    input: Image,
) -> Result<Image, Error> {
    let resources = input.as_ref().resources().cloned();
    let mut pixmap = Raster::try_create(region.width(), region.height(), resources.as_ref())?;

    let light_source = transform_light_source(fe.light_source(), region, ts);

    lighting::diffuse_lighting(
        fe,
        light_source,
        input.as_ref().as_image_ref(),
        pixmap.as_image_ref_mut(),
    );

    Image::from_image(pixmap, cs)
}

fn apply_specular_lighting(
    fe: &usvg::filter::SpecularLighting,
    region: IntRect,
    cs: usvg::filter::ColorInterpolation,
    ts: usvg::Transform,
    input: Image,
) -> Result<Image, Error> {
    let resources = input.as_ref().resources().cloned();
    let mut pixmap = Raster::try_create(region.width(), region.height(), resources.as_ref())?;

    let light_source = transform_light_source(fe.light_source(), region, ts);

    lighting::specular_lighting(
        fe,
        light_source,
        input.as_ref().as_image_ref(),
        pixmap.as_image_ref_mut(),
    );

    Image::from_image(pixmap, cs)
}

// TODO: do not modify LightSource
fn transform_light_source(
    mut source: usvg::filter::LightSource,
    region: IntRect,
    ts: usvg::Transform,
) -> usvg::filter::LightSource {
    use std::f32::consts::SQRT_2;
    use usvg::filter::LightSource;

    match &mut source {
        LightSource::DistantLight(..) => {}
        LightSource::PointLight(light) => {
            let mut point = tiny_skia::Point::from_xy(light.x, light.y);
            ts.map_point(&mut point);
            light.x = point.x - region.x() as f32;
            light.y = point.y - region.y() as f32;
            light.z = light.z * (ts.sx * ts.sx + ts.sy * ts.sy).sqrt() / SQRT_2;
        }
        LightSource::SpotLight(light) => {
            let sz = (ts.sx * ts.sx + ts.sy * ts.sy).sqrt() / SQRT_2;

            let mut point = tiny_skia::Point::from_xy(light.x, light.y);
            ts.map_point(&mut point);
            light.x = point.x - region.x() as f32;
            light.y = point.y - region.y() as f32;
            light.z *= sz;

            let mut point = tiny_skia::Point::from_xy(light.points_at_x, light.points_at_y);
            ts.map_point(&mut point);
            light.points_at_x = point.x - region.x() as f32;
            light.points_at_y = point.y - region.y() as f32;
            light.points_at_z *= sz;
        }
    }

    source
}

fn apply_to_canvas(input: Image, region: IntRect, pixmap: &mut Raster) -> Result<(), Error> {
    let input = input.into_color_space(usvg::filter::ColorInterpolation::SRGB)?;

    pixmap.fill(tiny_skia::Color::TRANSPARENT);
    pixmap.draw_pixmap(
        region.x(),
        region.y(),
        input.as_ref().as_ref(),
        &tiny_skia::PixmapPaint::default(),
        tiny_skia::Transform::identity(),
        None,
    );

    Ok(())
}

/// Calculates Gaussian blur sigmas for the current world transform.
///
/// If the last flag is set, then a box blur should be used. Or IIR otherwise.
fn resolve_std_dev(
    std_dx: f32,
    std_dy: f32,
    ts: usvg::Transform,
    resources: Option<&ResourcesRef>,
) -> Result<Option<(f64, f64, bool)>, Error> {
    let (sx, sy) = finite_scale(ts, resources)?;
    // Preserve defined f32 results, widening only products that would overflow.
    let scale = |v: f32, s: f64| {
        let narrow = v * s as f32;
        if narrow.is_finite() {
            narrow as f64
        } else {
            v as f64 * s
        }
    };
    let (mut std_dx, mut std_dy) = (scale(std_dx, sx), scale(std_dy, sy));
    if !std_dx.is_finite() || !std_dy.is_finite() {
        return Err(invalid_region(resources));
    }

    // 'A negative value or a value of zero disables the effect of the given filter primitive
    // (i.e., the result is the filter input image).'
    if std_dx.approx_eq_ulps(&0.0, 4) && std_dy.approx_eq_ulps(&0.0, 4) {
        return Ok(None);
    }

    // Ignore tiny sigmas. In case of IIR blur it can lead to a transparent image.
    if std_dx < 0.05 {
        std_dx = 0.0;
    }

    if std_dy < 0.05 {
        std_dy = 0.0;
    }

    const BLUR_SIGMA_THRESHOLD: f64 = 2.0;
    // Check that the current feGaussianBlur filter can be applied using a box blur.
    let box_blur = std_dx >= BLUR_SIGMA_THRESHOLD || std_dy >= BLUR_SIGMA_THRESHOLD;

    Ok(Some((std_dx, std_dy, box_blur)))
}

fn finite_scale(
    ts: usvg::Transform,
    resources: Option<&ResourcesRef>,
) -> Result<(f64, f64), Error> {
    if ![ts.sx, ts.kx, ts.ky, ts.sy, ts.tx, ts.ty]
        .into_iter()
        .all(f32::is_finite)
    {
        return Err(invalid_region(resources));
    }
    let (sx, sy) = ts.get_scale();
    Ok((
        if sx.is_finite() {
            sx as f64
        } else {
            (ts.sx as f64).hypot(ts.kx as f64)
        },
        if sy.is_finite() {
            sy as f64
        } else {
            (ts.ky as f64).hypot(ts.sy as f64)
        },
    ))
}

fn scale_coordinates(x: f32, y: f32, ts: usvg::Transform) -> Option<(f32, f32)> {
    let (sx, sy) = ts.get_scale();
    Some((x * sx, y * sy))
}
