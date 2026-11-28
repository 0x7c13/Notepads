// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE file in the project root for license information.
// ---------------------------------------------------------------------------------------------
//! Request-owned PNG decoding used by the raster image renderer.
use markdown_svg_budget::{Error, FailureKind, LimitKind, ResourcesRef};

/// Every target from this bridge has exactly four bytes per decoded pixel,
/// paid as Scratch by the real Raster::try_create in the same immutable request.
/// Other scratch is conservative overhead, so Scratch<=64MiB proves that all
/// simultaneously live RGBA targets from this bridge total <=16Mi pixels.
/// This does not qualify other codecs or external ResourceKind::Pixels owners.
const MAX_SCRATCH_PROFILE_BYTES: u64 = 64 * 1024 * 1024;

pub(crate) fn decode_png_controlled(
    encoded: &[u8],
    resources: &ResourcesRef,
) -> Result<crate::raster::Raster, Error> {
    if resources.limits().scratch_bytes > MAX_SCRATCH_PROFILE_BYTES {
        return Err(resources.record(Error::new(FailureKind::Quota, LimitKind::Capacity)));
    }
    let map_error = |error| match error {
        png::DecodingError::Resources(error) => resources.record(error),
        _ => resources.record(Error::new(FailureKind::ImageDecode, LimitKind::None)),
    };
    let reader = png::controlled::ControlledDecoder::new(encoded, resources.clone())
        .and_then(|decoder| decoder.read_info())
        .map_err(map_error)?;
    let info = reader.info();
    png::controlled::check_image_pixels(info.width, info.height, resources)?;
    let mut target = crate::raster::Raster::try_create(info.width, info.height, Some(resources))?;
    // Caller owns this existing PaidRaster; PNG borrows only its byte slice.
    reader
        .decode_rgba_into(target.data_mut())
        .map_err(map_error)?;
    // The original tiny-skia ColorU8 method calls the identical premultiply_u8
    // used by Pixmap::decode_png. Do not use the numerically different pipeline.
    for batch in target.data_mut().chunks_mut(4096) {
        resources.checkpoint((batch.len() / 4) as u64)?;
        for pixel in batch.chunks_exact_mut(4) {
            let color =
                tiny_skia::ColorU8::from_rgba(pixel[0], pixel[1], pixel[2], pixel[3]).premultiply();
            pixel.copy_from_slice(&[color.red(), color.green(), color.blue(), color.alpha()]);
        }
    }
    resources.checkpoint(0)?;
    Ok(target)
}
