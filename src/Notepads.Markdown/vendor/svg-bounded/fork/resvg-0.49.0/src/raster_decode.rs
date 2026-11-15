// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE file in the project root for license information.
// ---------------------------------------------------------------------------------------------
//! Request-owned raster codec bridges. PNG retains its separate decoder.
use markdown_svg_budget::codec::{Buffer, Control};
use markdown_svg_budget::{Error, FailureKind, LimitKind, ResourcesRef};

const MAX_SCRATCH_PROFILE_BYTES: u64 = 64 * 1024 * 1024;
const MAX_IMAGE_PIXELS: u64 = 16 * 1024 * 1024;

fn begin(resources: &ResourcesRef) -> Result<(), Error> {
    resources.checkpoint(0)?;
    if resources.limits().scratch_bytes > MAX_SCRATCH_PROFILE_BYTES {
        return Err(resources.record(Error::new(FailureKind::Quota, LimitKind::Capacity)));
    }
    Ok(())
}

fn target(
    width: usize,
    height: usize,
    resources: &ResourcesRef,
) -> Result<crate::raster::Raster, Error> {
    let capacity = || resources.record(Error::new(FailureKind::Quota, LimitKind::Capacity));
    let width = u32::try_from(width).map_err(|_| capacity())?;
    let height = u32::try_from(height).map_err(|_| capacity())?;
    let pixels = u64::from(width)
        .checked_mul(u64::from(height))
        .ok_or_else(capacity)?;
    resources.check_limit(LimitKind::Capacity, pixels, MAX_IMAGE_PIXELS)?;
    crate::raster::Raster::try_create(width, height, Some(resources))
}

fn premultiply(raster: &mut crate::raster::Raster, resources: &ResourcesRef) -> Result<(), Error> {
    for batch in raster.data_mut().chunks_mut(4096) {
        resources.checkpoint((batch.len() / 4) as u64)?;
        for pixel in batch.chunks_exact_mut(4) {
            // Preserve the original resvg GIF/WebP f64 expression exactly.
            let alpha = f64::from(pixel[3]) / 255.0;
            pixel[0] = (f64::from(pixel[0]) * alpha + 0.5) as u8;
            pixel[1] = (f64::from(pixel[1]) * alpha + 0.5) as u8;
            pixel[2] = (f64::from(pixel[2]) * alpha + 0.5) as u8;
        }
    }
    Ok(())
}

pub(crate) fn decode_gif_controlled(
    encoded: &[u8],
    resources: &ResourcesRef,
) -> Result<crate::raster::Raster, Error> {
    begin(resources)?;
    let map_error = |error: gif::DecodingError| {
        resources.record(
            error
                .resource_error()
                .unwrap_or_else(|| Error::new(FailureKind::ImageDecode, LimitKind::None)),
        )
    };
    let mut decoder =
        gif::controlled::ControlledDecoder::new(encoded, resources.clone()).map_err(map_error)?;
    let info = decoder.read_info().map_err(map_error)?;
    let mut raster = target(info.width, info.height, resources)?;
    decoder.decode_into(raster.data_mut()).map_err(map_error)?;
    drop(decoder);
    premultiply(&mut raster, resources)?;
    resources.checkpoint(0)?;
    Ok(raster)
}

pub(crate) fn decode_webp_controlled(
    encoded: &[u8],
    resources: &ResourcesRef,
) -> Result<crate::raster::Raster, Error> {
    begin(resources)?;
    let map_error = |error: image_webp::DecodingError| {
        resources.record(error.resource_error().unwrap_or_else(|| {
            Error::new(
                if matches!(error, image_webp::DecodingError::StaticUnsupported(_)) {
                    FailureKind::Unsupported
                } else {
                    FailureKind::ImageDecode
                },
                LimitKind::None,
            )
        }))
    };
    let mut decoder = image_webp::controlled::ControlledDecoder::new(encoded, resources.clone())
        .map_err(map_error)?;
    let info = decoder.read_info().map_err(map_error)?;
    let mut raster = target(info.width as usize, info.height as usize, resources)?;
    let control = Control::paid(resources.clone());
    let mut raw = Buffer::new(&control);
    raw.try_resize(info.output_bytes, 0u8)?;
    decoder.decode_into(&mut raw).map_err(map_error)?;
    drop(decoder);
    let channels = if info.has_alpha { 4 } else { 3 };
    for (source, output) in raw
        .chunks(channels * 1024)
        .zip(raster.data_mut().chunks_mut(4096))
    {
        resources.checkpoint((output.len() / 4) as u64)?;
        for (source, output) in source
            .chunks_exact(channels)
            .zip(output.chunks_exact_mut(4))
        {
            if info.has_alpha {
                let alpha = f64::from(source[3]) / 255.0;
                output[0] = (f64::from(source[0]) * alpha + 0.5) as u8;
                output[1] = (f64::from(source[1]) * alpha + 0.5) as u8;
                output[2] = (f64::from(source[2]) * alpha + 0.5) as u8;
                output[3] = source[3];
            } else {
                output[..3].copy_from_slice(&source[..3]);
                output[3] = 255;
            }
        }
    }
    drop(raw);
    resources.checkpoint(0)?;
    Ok(raster)
}

pub(crate) fn decode_jpeg_controlled(
    encoded: &[u8],
    resources: &ResourcesRef,
) -> Result<crate::raster::Raster, Error> {
    // All RGBA targets use the same Scratch account. Its existing 64 MiB
    // ceiling also bounds the sum of live bridge targets to 16 Mi pixels.
    resources.checkpoint(0)?;
    if resources.limits().scratch_bytes > MAX_SCRATCH_PROFILE_BYTES {
        return Err(resources.record(Error::new(FailureKind::Quota, LimitKind::Capacity)));
    }
    let map_error = |error: zune_jpeg::errors::DecodeErrors| {
        resources.record(
            error
                .resource_error()
                .unwrap_or_else(|| Error::new(FailureKind::ImageDecode, LimitKind::None)),
        )
    };
    let mut decoder = zune_jpeg::controlled::ControlledDecoder::new(encoded, resources.clone())
        .map_err(map_error)?;
    let info = decoder.read_info().map_err(map_error)?;
    let capacity = || resources.record(Error::new(FailureKind::Quota, LimitKind::Capacity));
    let width = u32::try_from(info.width).map_err(|_| capacity())?;
    let height = u32::try_from(info.height).map_err(|_| capacity())?;
    let pixels = u64::from(width)
        .checked_mul(u64::from(height))
        .ok_or_else(capacity)?;
    resources.check_limit(LimitKind::Capacity, pixels, MAX_IMAGE_PIXELS)?;
    let mut target = crate::raster::Raster::try_create(width, height, Some(resources))?;
    // The original decoder writes RGBA with opaque alpha; no colour conversion,
    // EXIF rotation, metadata clone or intermediate output Vec is introduced.
    decoder.decode_into(target.data_mut()).map_err(map_error)?;
    // Decoder scratch must be physically freed before the last publication
    // check: a late release failure must not escape as an apparently ready image.
    drop(decoder);
    resources.checkpoint(0)?;
    Ok(target)
}
