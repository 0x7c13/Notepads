// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE file in the project root for license information.
// ---------------------------------------------------------------------------------------------
//! Request-owned static WebP decoding using the original parser and codec algorithms.
use crate::{DecodingError, WebPDecoder};
use markdown_svg_budget::ResourcesRef;
/// Scalar header information; no internal owner escapes.
#[derive(Clone, Copy, Debug)]
pub struct ImageInfo {
    /// Image width.
    pub width: u32,
    /// Image height.
    pub height: u32,
    /// Required RGB/RGBA bytes.
    pub output_bytes: usize,
    /// Whether output has alpha.
    pub has_alpha: bool,
}
/// A request-owned static WebP decoder.
pub struct ControlledDecoder<'a> {
    inner: WebPDecoder<std::io::Cursor<&'a [u8]>>,
}
impl<'a> ControlledDecoder<'a> {
    /// Parse the original headers before decoding any pixels.
    pub fn new(input: &'a [u8], resources: ResourcesRef) -> Result<Self, DecodingError> {
        let inner = WebPDecoder::with_control(
            std::io::Cursor::new(input),
            Default::default(),
            crate::storage::Control::paid(resources),
        )?;
        Ok(Self { inner })
    }
    /// Return only checked scalar dimensions.
    pub fn read_info(&self) -> Result<ImageInfo, DecodingError> {
        crate::storage::step(&self.inner.control, 0)?;
        let (width, height) = self.inner.dimensions();
        let output_bytes = self
            .inner
            .output_buffer_size()
            .ok_or_else(|| crate::storage::capacity(&self.inner.control))?;
        Ok(ImageInfo {
            width,
            height,
            output_bytes,
            has_alpha: self.inner.has_alpha(),
        })
    }
    /// Borrow caller-owned pixel storage; the output is the original RGB/RGBA representation.
    pub fn decode_into(&mut self, output: &mut [u8]) -> Result<(), DecodingError> {
        self.inner.read_image(output)?;
        crate::storage::step(&self.inner.control, 0)
    }
}
