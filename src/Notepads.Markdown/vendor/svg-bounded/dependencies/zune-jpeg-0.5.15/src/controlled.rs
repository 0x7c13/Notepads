// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE file in the project root for license information.
// ---------------------------------------------------------------------------------------------
//! Same JPEG engine with request-paid storage and caller-owned RGBA output.
use crate::{
    errors::DecodeErrors,
    storage::{self, Control},
    JpegDecoder,
};
use markdown_svg_budget::ResourcesRef;
use zune_core::{colorspace::ColorSpace, options::DecoderOptions};

#[cfg(feature = "std")]
type Input<'a> = std::io::Cursor<&'a [u8]>;
#[cfg(not(feature = "std"))]
type Input<'a> = zune_core::bytestream::ZCursor<&'a [u8]>;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ImageInfo {
    pub width: usize,
    pub height: usize,
    pub output_bytes: usize,
}

pub struct ControlledDecoder<'a> {
    inner: JpegDecoder<Input<'a>>,
}
impl<'a> ControlledDecoder<'a> {
    pub fn new(input: &'a [u8], resources: ResourcesRef) -> Result<Self, DecodeErrors> {
        Self::new_with_options(input, resources, DecoderOptions::default())
    }
    /// Private source API for the original strict/SIMD profile comparisons.
    /// Output remains RGBA; metadata and internal owners never escape.
    #[doc(hidden)]
    pub fn new_with_options(
        input: &'a [u8],
        resources: ResourcesRef,
        options: DecoderOptions,
    ) -> Result<Self, DecodeErrors> {
        let control = Control::paid(resources);
        storage::step(&control, 0)?;
        let options = options.jpeg_set_out_colorspace(ColorSpace::RGBA);
        Ok(Self {
            inner: JpegDecoder::with_control(options, Input::new(input), control),
        })
    }
    pub fn read_info(&mut self) -> Result<ImageInfo, DecodeErrors> {
        self.inner
            .decode_headers()
            .map_err(|error| storage::terminal(&self.inner.control, error))?;
        storage::step(&self.inner.control, 0)?;
        let (width, height) = self.inner.dimensions().ok_or_else(|| {
            storage::terminal(
                &self.inner.control,
                DecodeErrors::FormatStatic("Missing dimensions"),
            )
        })?;
        if self.inner.output_colorspace() != Some(ColorSpace::RGBA) {
            return Err(storage::terminal(
                &self.inner.control,
                DecodeErrors::FormatStatic("Expected RGBA"),
            ));
        }
        let output_bytes = storage::mul(&self.inner.control, &[width, height, 4])?;
        Ok(ImageInfo {
            width,
            height,
            output_bytes,
        })
    }
    pub fn decode_into(&mut self, output: &mut [u8]) -> Result<(), DecodeErrors> {
        // This entry is valid without a preceding read_info call. Establish
        // checked usize RGBA dimensions before the inner legacy unwrap path.
        let info = self.read_info()?;
        if output.len() < info.output_bytes {
            return Err(storage::terminal(
                &self.inner.control,
                DecodeErrors::TooSmallOutput(info.output_bytes, output.len()),
            ));
        }
        storage::step(&self.inner.control, 0)?;
        self.inner
            .decode_into(output)
            .map_err(|error| storage::terminal(&self.inner.control, error))?;
        storage::step(&self.inner.control, 0)
    }
}
