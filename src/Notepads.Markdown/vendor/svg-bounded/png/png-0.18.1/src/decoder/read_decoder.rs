// Modifications copyright (c) 2026, Jiaqi (0x7c13) Liu. MIT license.
use super::storage::Control;
use super::stream::{DecodeOptions, Decoded, DecodingError, FormatErrorInner, StreamingDecoder};
use super::zlib::UnfilterBuf;
use super::Limits;

use std::io::{BufRead, ErrorKind, Read, Seek};

use crate::chunk;
use crate::common::Info;

/// Helper for encapsulating reading input from `Read` and feeding it into a `StreamingDecoder`
/// while hiding low-level `Decoded` events and only exposing a few high-level reading operations
/// like:
///
/// * `read_header_info` - reading until `IHDR` chunk
/// * `read_until_image_data` - reading until `IDAT` / `fdAT` sequence
/// * `decode_image_data` - reading from `IDAT` / `fdAT` sequence into `Vec<u8>`
/// * `finish_decoding_image_data()` - discarding remaining data from `IDAT` / `fdAT` sequence
/// * `read_until_end_of_input()` - reading until `IEND` chunk
pub(crate) struct ReadDecoder<R: Read> {
    reader: R,
    decoder: StreamingDecoder,
}

impl<R: BufRead + Seek> ReadDecoder<R> {
    pub(crate) fn control(&self) -> &Control {
        &self.decoder.control
    }
    pub(crate) fn info_view(&self) -> std::borrow::Cow<'_, Info<'_>> {
        self.decoder
            .metadata
            .view(self.info().expect("header already read"))
    }
    #[cfg(feature = "notepads-markdown-resources")]
    pub(crate) fn controlled(reader: R, control: Control) -> Result<Self, DecodingError> {
        let mut options = DecodeOptions::default();
        options.set_ignore_adler32(false);
        let mut decoder = StreamingDecoder::with_control(options, control)?;
        decoder.limits = Limits::default();
        Ok(Self { reader, decoder })
    }

    #[cfg(feature = "notepads-markdown-resources")]
    pub(crate) fn input_position(&mut self) -> Result<u64, DecodingError> {
        self.reader.stream_position().map_err(DecodingError::from)
    }

    pub fn new(r: R) -> Self {
        Self {
            reader: r,
            decoder: StreamingDecoder::new(),
        }
    }

    pub fn with_options(r: R, options: DecodeOptions) -> Self {
        let mut decoder = StreamingDecoder::new_with_options(options);
        decoder.limits = Limits::default();

        Self { reader: r, decoder }
    }

    pub fn set_limits(&mut self, limits: Limits) {
        self.decoder.limits = limits;
    }

    pub fn reserve_bytes(&mut self, bytes: usize) -> Result<(), DecodingError> {
        self.decoder.limits.reserve_bytes(bytes)
    }

    pub fn set_ignore_text_chunk(&mut self, ignore_text_chunk: bool) {
        self.decoder.set_ignore_text_chunk(ignore_text_chunk);
    }

    pub fn set_ignore_iccp_chunk(&mut self, ignore_iccp_chunk: bool) {
        self.decoder.set_ignore_iccp_chunk(ignore_iccp_chunk);
    }

    pub fn ignore_checksums(&mut self, ignore_checksums: bool) {
        self.decoder.set_ignore_adler32(ignore_checksums);
        self.decoder.set_ignore_crc(ignore_checksums);
    }

    /// Returns the next decoded chunk. If the chunk is an ImageData chunk, its contents are written
    /// into image_data.
    fn decode_next(
        &mut self,
        image_data: Option<&mut UnfilterBuf<'_>>,
    ) -> Result<Decoded, DecodingError> {
        let result = self.decode_next_impl(image_data);
        self.control().guard(result)
    }
    fn decode_next_impl(
        &mut self,
        image_data: Option<&mut UnfilterBuf<'_>>,
    ) -> Result<Decoded, DecodingError> {
        let (consumed, result) = {
            let buf = self.reader.fill_buf()?;
            if buf.is_empty() {
                return Err(DecodingError::IoError(ErrorKind::UnexpectedEof.into()));
            }
            self.decoder.update(buf, image_data)?
        };
        self.reader.consume(consumed);
        Ok(result)
    }

    /// Reads until the end of `IHDR` chunk.
    ///
    /// Prerequisite: None (idempotent).
    pub fn read_header_info(&mut self) -> Result<&Info<'static>, DecodingError> {
        while self.info().is_none() {
            if let Decoded::ChunkComplete(chunk::IEND) = self.decode_next(None)? {
                unreachable!()
            }
        }
        Ok(self.info().unwrap())
    }

    /// Reads until the start of the next `IDAT` or `fdAT` chunk.
    ///
    /// Prerequisite: **Not** within `IDAT` / `fdAT` chunk sequence.
    pub fn read_until_image_data(&mut self) -> Result<(), DecodingError> {
        loop {
            match self.decode_next(None)? {
                Decoded::ChunkBegin(_, chunk::IDAT) | Decoded::ChunkBegin(_, chunk::fdAT) => break,
                Decoded::ChunkComplete(chunk::IEND) => {
                    return Err(DecodingError::Format(
                        FormatErrorInner::MissingImageData.into(),
                    ))
                }
                // Ignore all other chunk events. Any other chunk may be between IDAT chunks, fdAT
                // chunks and their control chunks.
                _ => {}
            }
        }
        Ok(())
    }

    /// Reads `image_data` and reports whether there may be additional data afterwards (i.e. if it
    /// is okay to call `decode_image_data` and/or `finish_decoding_image_data` again)..
    ///
    /// Prerequisite: Input is currently positioned within `IDAT` / `fdAT` chunk sequence.
    pub fn decode_image_data(
        &mut self,
        image_data: Option<&mut UnfilterBuf<'_>>,
    ) -> Result<ImageDataCompletionStatus, DecodingError> {
        match self.decode_next(image_data)? {
            Decoded::ImageData => Ok(ImageDataCompletionStatus::ExpectingMoreData),
            Decoded::ImageDataFlushed => Ok(ImageDataCompletionStatus::Done),
            // Ignore other events that may happen within an `IDAT` / `fdAT` chunks sequence.
            _ => Ok(ImageDataCompletionStatus::ExpectingMoreData),
        }
    }

    /// Consumes and discards the rest of an `IDAT` / `fdAT` chunk sequence.
    ///
    /// Prerequisite: Input is currently positioned within `IDAT` / `fdAT` chunk sequence.
    pub fn finish_decoding_image_data(&mut self) -> Result<(), DecodingError> {
        loop {
            if let ImageDataCompletionStatus::Done = self.decode_image_data(None)? {
                return Ok(());
            }
        }
    }

    /// Reads until the `IEND` chunk.
    ///
    /// Prerequisite: `IEND` chunk hasn't been reached yet.
    pub fn read_until_end_of_input(&mut self) -> Result<(), DecodingError> {
        while !matches!(self.decode_next(None)?, Decoded::ChunkComplete(chunk::IEND)) {}
        Ok(())
    }

    pub fn info(&self) -> Option<&Info<'static>> {
        self.decoder.info.as_ref()
    }
}

#[derive(Debug, Eq, PartialEq)]
pub(crate) enum ImageDataCompletionStatus {
    ExpectingMoreData,
    Done,
}
