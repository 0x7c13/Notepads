// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE file in the project root for license information.
// ---------------------------------------------------------------------------------------------
//! Shared SVG raster geometry for the image pipeline. Natural dimensions are
//! 96-DPI DIPs; the supplied effective scale already contains the host's DPI.
//! This module owns no image, job, cache, platform object or reservation.

use super::{MAX_IMAGE_DIMENSION, MAX_IMAGE_PIXELS};

pub const TARGET_QUANTUM: u32 = 64;
pub const MAX_RASTER_SIDE: u32 = 4096;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SvgSizeError {
    InvalidNaturalSize,
    InvalidDisplayTarget,
    UnrepresentableTarget,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SvgNaturalSize {
    width: f64,
    height: f64,
}

impl SvgNaturalSize {
    pub fn new(width: f64, height: f64) -> Result<Self, SvgSizeError> {
        let maximum = f64::from(MAX_IMAGE_DIMENSION);
        if !positive_finite(width)
            || !positive_finite(height)
            || width > maximum
            || height > maximum
        {
            return Err(SvgSizeError::InvalidNaturalSize);
        }
        Ok(Self { width, height })
    }
    pub fn width(self) -> f64 {
        self.width
    }
    pub fn height(self) -> f64 {
        self.height
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SvgSourceRect {
    pub left: f64,
    pub top: f64,
    pub right: f64,
    pub bottom: f64,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SvgRasterPlan {
    pub natural: SvgNaturalSize,
    pub requested_long_side_pixels: f64,
    /// Uncapped upward quantization. Kept as finite f64 so a large valid
    /// request does not acquire an arbitrary u64 size limit.
    pub requested_bucket_pixels: f64,
    pub target_bucket: u32,
    pub decoded_width: u32,
    pub decoded_height: u32,
    pub stride: u32,
    pub pixel_capacity: u64,
    pub uniform_scale: f64,
    pub effective_source_rect: SvgSourceRect,
    pub resolution_limited: bool,
}

/// Exact geometry consumed by the f32 renderer, shared with image admission.
/// Pixel counts ceiling these extents; they never define a second axis scale.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SvgPixelGeometry {
    pub scale_bits: u32,
    pub extent_width: f64,
    pub extent_height: f64,
    pub width: u32,
    pub height: u32,
    pub stride: u32,
    pub capacity: u64,
}

impl SvgRasterPlan {
    pub fn freeze_renderer(
        self,
        viewport_width_bits: u32,
        viewport_height_bits: u32,
    ) -> Result<SvgPixelGeometry, SvgSizeError> {
        let fail = SvgSizeError::UnrepresentableTarget;
        let vw = f32::from_bits(viewport_width_bits);
        let vh = f32::from_bits(viewport_height_bits);
        if !vw.is_finite() || vw <= 0.0 || !vh.is_finite() || vh <= 0.0 {
            return Err(fail);
        }
        let bucket = f64::from(self.target_bucket);
        let q = self
            .uniform_scale
            .min(bucket / f64::from(vw))
            .min(bucket / f64::from(vh));
        if !q.is_finite() || q <= 0.0 || q > f64::from(f32::MAX) {
            return Err(fail);
        }
        let upper = q as f32;
        if !upper.is_finite() {
            return Err(fail);
        }
        let upper_bits = upper
            .to_bits()
            .checked_sub(u32::from(f64::from(upper) > q))
            .filter(|&bits| bits != 0)
            .ok_or(fail)?;
        let within = |bits| {
            let scale = f32::from_bits(bits);
            [vw, vh].iter().all(|&axis| {
                let exact = f64::from(axis) * f64::from(scale);
                let rounded: f32 = axis * scale;
                exact <= bucket && rounded.is_finite() && f64::from(rounded) <= bucket
            })
        };
        let (mut low, mut high, mut selected) = (1u32, upper_bits, 0u32);
        for _ in 0..32 {
            if low > high {
                break;
            }
            let middle = low + (high - low) / 2;
            if within(middle) {
                selected = middle;
                low = middle + 1;
            } else {
                high = middle - 1;
            }
        }
        if selected == 0 {
            return Err(fail);
        }
        let scale = f32::from_bits(selected);
        let (ew, eh): (f32, f32) = (vw * scale, vh * scale);
        if !ew.is_finite() || ew <= 0.0 || !eh.is_finite() || eh <= 0.0 {
            return Err(fail);
        }
        let extent_width = f64::from(ew);
        let extent_height = f64::from(eh);
        let width = extent_width.ceil() as u32;
        let height = extent_height.ceil() as u32;
        if width == 0
            || height == 0
            || width > MAX_RASTER_SIDE
            || height > MAX_RASTER_SIDE
            || u64::from(width) * u64::from(height) > MAX_IMAGE_PIXELS
        {
            return Err(fail);
        }
        let stride = width.checked_mul(4).ok_or(fail)?;
        let capacity = u64::from(stride)
            .checked_mul(u64::from(height))
            .ok_or(fail)?;
        Ok(SvgPixelGeometry {
            scale_bits: selected,
            extent_width,
            extent_height,
            width,
            height,
            stride,
            capacity,
        })
    }

    /// Compute a target from actual display width and one effective physical
    /// scale. Callers must not multiply LogicalDpi into that scale again.
    pub fn for_display(
        natural: SvgNaturalSize,
        display_width_dip: f64,
        effective_physical_scale: f64,
    ) -> Result<Self, SvgSizeError> {
        if !positive_finite(display_width_dip) || !positive_finite(effective_physical_scale) {
            return Err(SvgSizeError::InvalidDisplayTarget);
        }
        let long = natural.width.max(natural.height);
        let requested = requested_long_side(
            display_width_dip,
            effective_physical_scale,
            long,
            natural.width,
        )
        .ok_or(SvgSizeError::UnrepresentableTarget)?;
        let maximum = f64::from(MAX_RASTER_SIDE);
        let quantum = f64::from(TARGET_QUANTUM);
        let requested_bucket_pixels = if requested <= quantum {
            quantum
        } else {
            (requested / quantum).ceil() * quantum
        };
        if !positive_finite(requested_bucket_pixels) {
            return Err(SvgSizeError::UnrepresentableTarget);
        }
        // No epsilon: a representable request just above a bucket boundary
        // really belongs to the next bucket.
        let target_bucket = Some(requested_bucket_pixels.min(maximum) as u32)
            .filter(|value| *value > 0 && *value <= MAX_RASTER_SIDE)
            .ok_or(SvgSizeError::UnrepresentableTarget)?;
        let long_extent = f64::from(target_bucket);
        let uniform_scale = long_extent / long;
        let short_extent = positive_mul_div(natural.width.min(natural.height), long_extent, long)
            .ok_or(SvgSizeError::UnrepresentableTarget)?;
        if !positive_finite(uniform_scale) {
            return Err(SvgSizeError::UnrepresentableTarget);
        }
        // Keep the dominant axis exact rather than multiplying the natural
        // long side back by a rounded quotient and accidentally ceiling to4097.
        let (width, height) = if natural.width >= natural.height {
            (long_extent, short_extent)
        } else {
            (short_extent, long_extent)
        };
        let decoded_width = width.ceil() as u32;
        let decoded_height = height.ceil() as u32;
        let pixels = u64::from(decoded_width) * u64::from(decoded_height);
        if decoded_width == 0
            || decoded_height == 0
            || decoded_width > MAX_RASTER_SIDE
            || decoded_height > MAX_RASTER_SIDE
            || pixels > MAX_IMAGE_PIXELS
        {
            return Err(SvgSizeError::UnrepresentableTarget);
        }
        let stride = decoded_width
            .checked_mul(4)
            .ok_or(SvgSizeError::UnrepresentableTarget)?;
        let pixel_capacity = u64::from(stride)
            .checked_mul(u64::from(decoded_height))
            .ok_or(SvgSizeError::UnrepresentableTarget)?;
        Ok(Self {
            natural,
            requested_long_side_pixels: requested,
            requested_bucket_pixels,
            target_bucket,
            decoded_width,
            decoded_height,
            stride,
            pixel_capacity,
            uniform_scale,
            effective_source_rect: SvgSourceRect {
                left: 0.0,
                top: 0.0,
                right: width,
                bottom: height,
            },
            resolution_limited: requested > maximum,
        })
    }
}

fn positive_finite(value: f64) -> bool {
    value.is_finite() && value > 0.0
}

fn positive_mul(a: f64, b: f64) -> Option<f64> {
    let result = a * b;
    positive_finite(result).then_some(result)
}

// Prefer an ordinary multiply/divide. Retry an equivalent ordering only when
// an intermediate is outside f64; this does not use an epsilon or round a
// request down to a bucket. No nonfinite or zero intermediate is published.
fn positive_mul_div(a: f64, b: f64, divisor: f64) -> Option<f64> {
    if let Some(product) = positive_mul(a, b) {
        let result = product / divisor;
        if positive_finite(result) {
            return Some(result);
        }
    }
    let ratio = a / divisor;
    if positive_finite(ratio) {
        if let Some(result) = positive_mul(ratio, b) {
            return Some(result);
        }
    }
    let ratio = b / divisor;
    if positive_finite(ratio) {
        return positive_mul(ratio, a);
    }
    None
}

fn requested_long_side(display: f64, scale: f64, long: f64, width: f64) -> Option<f64> {
    if long == width {
        return positive_mul(display, scale);
    }
    if let Some(physical) = positive_mul(display, scale) {
        let ratio = long / width;
        if positive_finite(ratio) {
            if let Some(result) = positive_mul(physical, ratio) {
                return Some(result);
            }
        }
        if let Some(result) = positive_mul_div(physical, long, width) {
            return Some(result);
        }
    }
    // An underflowed physical width can still have a representable long side
    // for a very narrow portrait. Try both bounded factorizations.
    for (a, b) in [(display, scale), (scale, display)] {
        let ratio = a / width;
        if positive_finite(ratio) {
            if let Some(product) = positive_mul(b, long) {
                if let Some(result) = positive_mul(ratio, product) {
                    return Some(result);
                }
            }
        }
        if let Some(part) = positive_mul_div(a, long, width) {
            if let Some(result) = positive_mul(part, b) {
                return Some(result);
            }
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    fn plan(width: f64, height: f64, display: f64, scale: f64) -> SvgRasterPlan {
        SvgRasterPlan::for_display(SvgNaturalSize::new(width, height).unwrap(), display, scale)
            .unwrap()
    }

    #[test]
    fn fractional_natural_size_and_one_dpi_scale_have_independent_oracles() {
        for (scale, bucket, height) in [
            (1.0, 128, 64),
            (1.5, 192, 96),
            (2.0, 256, 128),
            (3.0, 320, 160),
        ] {
            let p = plan(100.5, 50.25, 100.5, scale);
            assert_eq!((p.decoded_width, p.decoded_height), (bucket, height));
            assert_eq!(p.uniform_scale, f64::from(bucket) / 100.5);
            assert!(!p.resolution_limited);
        }
    }

    #[test]
    fn fractional_short_axis_uses_effective_rect_without_stretching_or_extra_row() {
        let p = plan(100.5, 50.2, 100.5, 1.0);
        assert_eq!((p.decoded_width, p.decoded_height), (128, 64));
        assert_eq!(p.effective_source_rect.right, 128.0);
        assert!((p.effective_source_rect.bottom - 64.0 * 100.4 / 100.5).abs() < 1e-12);
        assert!(p.effective_source_rect.bottom < f64::from(p.decoded_height));
        assert_eq!(p.stride, 512);
        assert_eq!(p.pixel_capacity, 32768);
    }

    #[test]
    fn bucket_boundary_is_not_hidden_by_epsilon() {
        let above = f64::from_bits(64.0f64.to_bits() + 1);
        let below = f64::from_bits(64.0f64.to_bits() - 1);
        assert_eq!(plan(100.0, 50.0, below, 1.0).target_bucket, 64);
        assert_eq!(plan(100.0, 50.0, 64.0, 1.0).target_bucket, 64);
        assert_eq!(plan(100.0, 50.0, above, 1.0).target_bucket, 128);
    }

    #[test]
    fn portrait_and_square_keep_a_single_scale_and_bounded_pixel_count() {
        let p = plan(50.25, 100.5, 50.25, 1.5);
        assert_eq!((p.decoded_width, p.decoded_height), (96, 192));
        let p = plan(8192.0, 8192.0, 8192.0, 3.0);
        assert_eq!((p.decoded_width, p.decoded_height), (4096, 4096));
        assert_eq!(p.pixel_capacity, 64 * 1024 * 1024);
        assert_eq!(p.requested_bucket_pixels, 24576.0);
        assert_eq!(p.target_bucket, 4096);
        assert!(p.resolution_limited);
    }

    #[test]
    fn representable_subnormal_geometry_survives_reordered_intermediates() {
        let tiny = f64::from_bits(1);
        let p = plan(tiny, 1.0, tiny, 1.0);
        assert_eq!(p.requested_long_side_pixels, 1.0);
        assert_eq!(p.requested_bucket_pixels, 64.0);
        assert_eq!((p.decoded_width, p.decoded_height), (1, 64));
        assert_eq!(p.effective_source_rect.right, tiny * 64.0);
        let p = plan(tiny, 2.0, tiny, 1.0);
        assert_eq!(p.requested_long_side_pixels, 2.0);
        assert_eq!(p.effective_source_rect.right, tiny * 32.0);
        let p = plan(tiny, 1.0, tiny, 0.5);
        assert_eq!(p.requested_long_side_pixels, 0.5);
    }

    #[test]
    fn rounded_dominant_axis_never_becomes_4097() {
        for natural in [0.1, 100.1, 100.5, 4095.9, 8192.0] {
            let p = plan(natural, natural / 3.0, 4096.0, 1.0);
            assert_eq!(p.decoded_width, 4096);
            assert_eq!(p.effective_source_rect.right, 4096.0);
            assert!(!p.resolution_limited);
        }
    }

    #[test]
    fn invalid_and_unrepresentable_inputs_cannot_return_partial_plans() {
        for bad in [0.0, -1.0, f64::NAN, f64::INFINITY, f64::NEG_INFINITY] {
            assert!(SvgNaturalSize::new(bad, 1.0).is_err());
            let natural = SvgNaturalSize::new(100.0, 50.0).unwrap();
            assert!(SvgRasterPlan::for_display(natural, bad, 1.0).is_err());
            assert!(SvgRasterPlan::for_display(natural, 1.0, bad).is_err());
        }
        assert!(SvgNaturalSize::new(8192.1, 1.0).is_err());
        let n = SvgNaturalSize::new(100.0, 50.0).unwrap();
        assert_eq!(
            SvgRasterPlan::for_display(n, f64::MAX, 2.0),
            Err(SvgSizeError::UnrepresentableTarget)
        );
        assert_eq!(
            SvgRasterPlan::for_display(n, f64::from_bits(1), 0.5),
            Err(SvgSizeError::UnrepresentableTarget)
        );
        let n = SvgNaturalSize::new(f64::from_bits(1), 8192.0).unwrap();
        assert_eq!(
            SvgRasterPlan::for_display(n, 1.0, 1.0),
            Err(SvgSizeError::UnrepresentableTarget)
        );
    }
}
