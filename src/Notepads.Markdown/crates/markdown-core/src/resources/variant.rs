// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE file in the project root for license information.
// ---------------------------------------------------------------------------------------------

//! Image target identities and admission of the renderer's exact geometry.

use super::svg_size::{SvgNaturalSize, SvgPixelGeometry, SvgRasterPlan};
use super::{ImageMetadata, MAX_IMAGE_PIXELS, ResourceError};

/// Shipping raster buckets. The decoder validation ceiling is a separate
/// boundary and does not authorize increasing this product maximum.
pub const RASTER_MAXIMUM_SIDE: u32 = 2048;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct RasterPlan {
    pub width: u32,
    pub height: u32,
    pub stride: u32,
    pub maximum_side: u32,
    pub bytes: u64,
}

impl ImageMetadata {
    /// Plan from original intrinsic dimensions and EXIF orientation. Layout
    /// stays in intrinsic DIPs; only the immutable physical payload is bucketed.
    pub fn raster_plan(self, target: ImageTarget) -> Result<RasterPlan, ResourceError> {
        target.validate()?;
        // Reuse the complete intrinsic/orientation validation, independently of
        // the old decoded bucket retained solely for stable placeholder layout.
        self.required_capacity()?;
        let (width, height) = if self.orientation >= 5 {
            (self.intrinsic_height, self.intrinsic_width)
        } else {
            (self.intrinsic_width, self.intrinsic_height)
        };
        let longest = width.max(height);
        let display_ratio = (target.available_width_dip / f64::from(width)).min(1.0);
        let needed = f64::from(longest) * display_ratio * target.effective_physical_scale;
        let mut bucket = 128;
        while bucket < RASTER_MAXIMUM_SIDE && f64::from(bucket) < needed {
            bucket *= 2;
        }
        let maximum_side = bucket.min(longest);
        let width = (u64::from(width) * u64::from(maximum_side) / u64::from(longest)).max(1) as u32;
        let height =
            (u64::from(height) * u64::from(maximum_side) / u64::from(longest)).max(1) as u32;
        let stride = width * 4;
        Ok(RasterPlan {
            width,
            height,
            stride,
            maximum_side,
            bytes: u64::from(stride) * u64::from(height),
        })
    }

    pub fn raster_satisfies(self, target: ImageTarget) -> bool {
        self.raster_plan(target).is_ok_and(|plan| {
            self.decoded_width >= plan.width && self.decoded_height >= plan.height
        })
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ImageTarget {
    pub font_generation: u64,
    pub generation: u64,
    pub available_width_dip: f64,
    pub effective_physical_scale: f64,
}

impl ImageTarget {
    pub fn validate(self) -> Result<(), ResourceError> {
        if self.font_generation == 0
            || self.generation == 0
            || !self.available_width_dip.is_finite()
            || self.available_width_dip <= 0.0
            || !self.effective_physical_scale.is_finite()
            || self.effective_physical_scale <= 0.0
        {
            return Err(ResourceError::InvalidMetadata);
        }
        Ok(())
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SvgImageMetadata {
    pub natural_width_dip: f64,
    pub natural_height_dip: f64,
    pub decoded_width: u32,
    pub decoded_height: u32,
    pub stride: u32,
    pub source_width: f64,
    pub source_height: f64,
    pub viewport_width_bits: u32,
    pub viewport_height_bits: u32,
    pub scale_bits: u32,
    pub resolution_limited: bool,
}

impl SvgImageMetadata {
    fn plan(self, target: ImageTarget) -> Result<(SvgRasterPlan, SvgPixelGeometry), ResourceError> {
        target.validate()?;
        let natural = SvgNaturalSize::new(self.natural_width_dip, self.natural_height_dip)
            .map_err(|_| ResourceError::InvalidMetadata)?;
        if self.natural_width_dip * self.natural_height_dip > MAX_IMAGE_PIXELS as f64
            || (self.natural_width_dip as f32).to_bits() != self.viewport_width_bits
            || (self.natural_height_dip as f32).to_bits() != self.viewport_height_bits
        {
            return Err(ResourceError::InvalidMetadata);
        }
        let plan = SvgRasterPlan::for_display(
            natural,
            self.natural_width_dip.min(target.available_width_dip),
            target.effective_physical_scale,
        )
        .map_err(|_| ResourceError::InvalidMetadata)?;
        let frozen = plan
            .freeze_renderer(self.viewport_width_bits, self.viewport_height_bits)
            .map_err(|_| ResourceError::InvalidMetadata)?;
        Ok((plan, frozen))
    }

    pub fn required_capacity(self, target: ImageTarget) -> Result<u64, ResourceError> {
        let (plan, frozen) = self.plan(target)?;
        if self.decoded_width != frozen.width
            || self.decoded_height != frozen.height
            || self.stride != frozen.stride
            || self.source_width != frozen.extent_width
            || self.source_height != frozen.extent_height
            || self.scale_bits != frozen.scale_bits
            || self.resolution_limited != plan.resolution_limited
        {
            return Err(ResourceError::InvalidMetadata);
        }
        Ok(frozen.capacity)
    }

    /// Adequate existing pixels need no lower-resolution render. Comparing
    /// actual scale also covers the capped4096 case without a retry loop.
    pub fn satisfies(self, target: ImageTarget) -> bool {
        self.plan(target).is_ok_and(|(_, frozen)| {
            f32::from_bits(self.scale_bits) >= f32::from_bits(frozen.scale_bits)
        })
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum ImageVariantMetadata {
    Raster(ImageMetadata),
    Svg(SvgImageMetadata),
}

impl From<ImageMetadata> for ImageVariantMetadata {
    fn from(value: ImageMetadata) -> Self {
        Self::Raster(value)
    }
}

impl From<SvgImageMetadata> for ImageVariantMetadata {
    fn from(value: SvgImageMetadata) -> Self {
        Self::Svg(value)
    }
}

impl ImageVariantMetadata {
    pub fn required_capacity(self, target: ImageTarget) -> Result<u64, ResourceError> {
        match self {
            Self::Raster(value) => {
                let plan = value.raster_plan(target)?;
                if value.decoded_width != plan.width
                    || value.decoded_height != plan.height
                    || value.stride != plan.stride
                {
                    return Err(ResourceError::InvalidMetadata);
                }
                value.required_capacity()
            }
            Self::Svg(value) => value.required_capacity(target),
        }
    }
}
