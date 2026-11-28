// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE file in the project root for license information.
// ---------------------------------------------------------------------------------------------
//! Request-controlled PNG decoding. The parser, inflater and pixel algorithms
//! are shared with the compatibility decoder; owned codec state never escapes.

use crate::DecodingError;
use markdown_svg_budget::ResourcesRef;

/// Controlled PNG's independent per-image admission gate (16 Mi pixels).
pub const MAX_IMAGE_PIXELS: u64 = 16 * 1024 * 1024;

/// Check dimensions before any row/metadata/image allocation derived from them.
/// This uses the existing request and does not create a second counter/ledger.
pub fn check_image_pixels(
    width: u32,
    height: u32,
    resources: &ResourcesRef,
) -> Result<u64, markdown_svg_budget::Error> {
    let pixels = u64::from(width)
        .checked_mul(u64::from(height))
        .ok_or_else(|| {
            resources.record(markdown_svg_budget::Error::new(
                markdown_svg_budget::FailureKind::Quota,
                markdown_svg_budget::LimitKind::Capacity,
            ))
        })?;
    resources.check_limit(
        markdown_svg_budget::LimitKind::Capacity,
        pixels,
        MAX_IMAGE_PIXELS,
    )?;
    Ok(pixels)
}

pub(crate) fn resource(error: markdown_svg_budget::Error) -> DecodingError {
    DecodingError::Resources(error)
}

pub(crate) fn flate(error: fdeflate::DecompressionError) -> DecodingError {
    match error {
        fdeflate::DecompressionError::Resources(error) => resource(error),
        err => DecodingError::Format(
            crate::decoder::stream::FormatErrorInner::CorruptFlateStream { err }.into(),
        ),
    }
}

pub(crate) fn inflate_bounded(
    input: &[u8],
    limit: usize,
    control: &crate::decoder::storage::Control,
) -> Result<crate::decoder::storage::Buffer<u8>, DecodingError> {
    use crate::decoder::storage::{Buffer, One};
    let r = control
        .resources
        .as_ref()
        .expect("controlled inflater capability");
    let mut decoder = One::new(control, || {
        fdeflate::Decompressor::new_controlled(r.clone()).map_err(flate)
    })?;
    let mut output = Buffer::new(control);
    output.resize(1024.min(limit), 0)?;
    let (mut read, mut written) = (0, 0);
    loop {
        control.checkpoint(0)?;
        if written == output.len() {
            // Exact-cap output may still need its EOB and zlib trailer from a
            // later input window. One paid probe byte distinguishes that tail
            // from actual output beyond the independent PNG codec cap.
            let next = if written == limit {
                let Some(next) = limit.checked_add(1) else {
                    return control.guard(Err(DecodingError::LimitsExceeded));
                };
                next
            } else {
                written.saturating_add(32 * 1024).min(limit)
            };
            output.resize(next, 0)?;
        }
        let end = (read + 64 * 1024).min(input.len());
        let (consumed, produced) = decoder
            .read(&input[read..end], &mut output, written, end == input.len())
            .map_err(flate)?;
        read += consumed;
        written += produced;
        if written > limit {
            return control.guard(Err(DecodingError::LimitsExceeded));
        }
        if decoder.is_done() {
            output.truncate(written);
            return Ok(output);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use markdown_svg_budget::{Backend, Error, FirstFailureRef, Limits, ResourceKind, Ticket};
    use std::sync::{Arc, Mutex};

    struct TestBackend(Arc<Mutex<Vec<usize>>>);
    impl Backend for TestBackend {
        fn reserve(&self, _: ResourceKind, bytes: usize) -> Result<Ticket, Error> {
            let mut sizes = self.0.lock().unwrap();
            sizes.push(bytes);
            Ticket::new(1, sizes.len() as u32, 1)
        }
        fn allocation_committed(&self, _: &Ticket, _: usize) -> Result<(), Error> {
            Ok(())
        }
        fn release_after_free(&self, ticket: Ticket) -> Result<(), Error> {
            let mut sizes = self.0.lock().unwrap();
            assert_ne!(
                std::mem::replace(&mut sizes[ticket.slot() as usize - 1], 0),
                0
            );
            Ok(())
        }
        fn checkpoint(&self, _: u64) -> Result<(), Error> {
            Ok(())
        }
    }

    #[test]
    fn iccp_inflate_accepts_exact_cap_with_later_trailer_and_rejects_one_byte_over() {
        let profile = vec![0x55u8; 65536];
        // Stored blocks and legal empty trailing blocks put the zlib trailer
        // in a later 64 KiB input window after the exact-cap output is full.
        let mut compressed = vec![0x78, 0x01];
        let (mut a, mut b) = (1u32, 0u32);
        for block in profile.chunks(65535) {
            compressed.push(0);
            let len = block.len() as u16;
            compressed.extend_from_slice(&len.to_le_bytes());
            compressed.extend_from_slice(&(!len).to_le_bytes());
            compressed.extend_from_slice(block);
            for byte in block {
                a = (a + u32::from(*byte)) % 65521;
                b = (b + a) % 65521;
            }
        }
        for index in 0..20000 {
            compressed.extend_from_slice(&[u8::from(index == 19999), 0, 0, 0xff, 0xff]);
        }
        compressed.extend_from_slice(&((b << 16) | a).to_be_bytes());
        for (limit, accepted) in [(profile.len(), true), (profile.len() - 1, false)] {
            let sizes = Arc::new(Mutex::new(Vec::new()));
            let resources = ResourcesRef::new_prepaid(
                TestBackend(sizes.clone()),
                FirstFailureRef::new_prepaid(),
                Limits::default(),
            );
            let control = Control {
                resources: Some(resources.clone()),
            };
            let result = inflate_bounded(&compressed, limit, &control);
            assert_eq!(result.is_ok(), accepted, "inflated cap {limit}");
            if let Ok(bytes) = &result {
                assert_eq!(&bytes[..], &profile);
            }
            drop(result);
            drop(control);
            assert!(sizes.lock().unwrap().iter().all(|bytes| *bytes == 0));
            assert_eq!(resources.statistics().scratch_bytes, 0);
            if accepted {
                resources.validate_retired().unwrap();
            } else {
                assert_eq!(
                    resources.failure().unwrap().kind,
                    markdown_svg_budget::FailureKind::Quota
                );
            }
        }
    }
}

use crate::decoder::storage::{Buffer, Control, One};
use std::io::Cursor;

/// Scalar output information; no metadata or codec owner can escape.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ControlledInfo {
    /// Canvas width.
    pub width: u32,
    /// Canvas height.
    pub height: u32,
    /// Normalized output color model.
    pub color_type: crate::ColorType,
    /// Normalized output bit depth (eight).
    pub bit_depth: crate::BitDepth,
    /// Maximum normalized row bytes.
    pub line_size: usize,
}
/// Exact validated container boundary, independent of opaque accepted tail.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct FinishInfo {
    /// Bytes consumed through the IEND CRC.
    pub consumed_bytes: usize,
    /// Total borrowed encoded bytes, including any uninterpreted tail.
    pub encoded_bytes: usize,
}
/// Decoded first-image shape and validated PNG container boundary.
#[derive(Debug)]
pub struct DecodedInfo {
    /// Same first-frame geometry as the compatibility decoder.
    pub output: crate::OutputInfo,
    /// Actual consumed boundary through IEND.
    pub finish: FinishInfo,
}
/// Configuration borrowing the caller's encoded input.
pub struct ControlledDecoder<'a> {
    input: &'a [u8],
    resources: ResourcesRef,
    limits: crate::Limits,
}
impl<'a> ControlledDecoder<'a> {
    /// Use the caller's existing request ledger. Input bytes remain borrowed.
    pub fn new(input: &'a [u8], resources: ResourcesRef) -> Result<Self, DecodingError> {
        resources.checkpoint(0).map_err(resource)?;
        Ok(Self {
            input,
            resources,
            limits: crate::Limits::default(),
        })
    }
    /// The codec limit is an additional format gate, never the memory ledger.
    pub fn set_limits(&mut self, limits: crate::Limits) {
        self.limits = limits;
    }
    /// Read metadata through the original PNG state machine into paid owners.
    pub fn read_info(self) -> Result<ControlledReader<'a>, DecodingError> {
        let control = Control {
            resources: Some(self.resources.clone()),
        };
        let result = One::new(&control, || {
            let mut decoder =
                crate::Decoder::controlled(Cursor::new(self.input), self.resources.clone())?;
            decoder.set_limits(self.limits);
            decoder.read_info()
        });
        let result = control.guard(result);
        match result {
            Ok(reader) => Ok(ControlledReader {
                reader,
                control,
                encoded_bytes: self.input.len(),
            }),
            Err(error) => {
                control.checkpoint(0)?;
                Err(error)
            }
        }
    }
}
/// Same parser/row decoder with private, paid state and metadata.
pub struct ControlledReader<'a> {
    encoded_bytes: usize,
    reader: One<crate::Reader<Cursor<&'a [u8]>>>,
    control: Control,
}
impl ControlledReader<'_> {
    /// Copy scalar information only; legacy `Info` ownership is never exposed.
    pub fn info(&self) -> ControlledInfo {
        let info = self.reader.info();
        let (color_type, bit_depth) = self.reader.output_color_type();
        ControlledInfo {
            width: info.width,
            height: info.height,
            color_type,
            bit_depth,
            line_size: self
                .reader
                .output_line_size(info.width)
                .expect("validated row size"),
        }
    }
    /// Decode into the caller's row buffer, retaining no caller pointer.
    /// Rows are provisional until finish validates the first image and IEND.
    pub fn read_row(
        &mut self,
        row: &mut [u8],
    ) -> Result<Option<crate::InterlaceInfo>, DecodingError> {
        if row.len() < self.info().line_size {
            return self.control.guard(Err(DecodingError::LimitsExceeded));
        }
        let result = self.reader.read_row(row);
        self.control.guard(result)
    }
    /// Consume tail chunks through IEND, then free every owned codec temporary
    /// before the final shared failure check. Bytes after IEND remain accepted.
    pub fn finish(mut self) -> Result<FinishInfo, DecodingError> {
        let result = (|| {
            if self.reader.has_frame_rows() {
                let mut discard = Buffer::new(&self.control);
                discard.resize(self.info().line_size, 0)?;
                loop {
                    let result = self.reader.read_row(&mut discard);
                    if self.control.guard(result)?.is_none() {
                        break;
                    }
                }
            } else {
                // Even if the caller read the last row but did not poll None,
                // consume its end code/checksum through the same inflater.
                let result = self.reader.read_row(&mut []);
                self.control.guard(result)?;
            }
            let result = self.reader.finish();
            self.control.guard(result)?;
            let result = self.reader.input_position();
            let consumed = self.control.guard(result)?;
            Ok(FinishInfo {
                consumed_bytes: consumed as usize,
                encoded_bytes: self.encoded_bytes,
            })
        })();
        let result = self.control.guard(result);
        let control = self.control.clone();
        drop(self);
        control.checkpoint(0)?;
        result
    }

    /// Decode straight RGBA8 into a caller-owned paid target. The codec never
    /// owns or releases this slice. Alpha premultiplication belongs to the host.
    pub fn decode_rgba_into(mut self, target: &mut [u8]) -> Result<DecodedInfo, DecodingError> {
        let info = self.info();
        let len = (info.width as usize)
            .checked_mul(info.height as usize)
            .and_then(|v| v.checked_mul(4));
        if len != Some(target.len()) {
            return self.control.guard(Err(DecodingError::LimitsExceeded));
        }
        let (frame_width, frame_height) = self.reader.frame_dimensions();
        let mut row = Buffer::new(&self.control);
        row.resize(info.width as usize * 4, 0)?;
        let result = (|| {
            // The compatibility decoder zero-initializes its full canvas
            // output, then expands opaque formats with alpha=255. Preserve
            // those static-first-image tail pixels without APNG compositing.
            let alpha = if matches!(
                info.color_type,
                crate::ColorType::Grayscale | crate::ColorType::Rgb
            ) {
                255
            } else {
                0
            };
            for part in target.chunks_mut(4096) {
                self.control.checkpoint(part.len())?;
                for pixel in part.chunks_exact_mut(4) {
                    pixel.copy_from_slice(&[0, 0, 0, alpha]);
                }
            }

            while let Some(interlace) = self.read_row(&mut row)? {
                let samples = match interlace {
                    crate::InterlaceInfo::Adam7(a) => a.samples as usize,
                    _ => frame_width as usize,
                };
                let channels = info.color_type.samples();
                for index in (0..samples).rev() {
                    if index % 1024 == 0 {
                        self.control.checkpoint((samples - index).min(1024) * 4)?;
                    }
                    let src = index * channels;
                    let dst = index * 4;
                    let rgba = match info.color_type {
                        crate::ColorType::Grayscale => [row[src], row[src], row[src], 255],
                        crate::ColorType::GrayscaleAlpha => {
                            [row[src], row[src], row[src], row[src + 1]]
                        }
                        crate::ColorType::Rgb => [row[src], row[src + 1], row[src + 2], 255],
                        crate::ColorType::Rgba => {
                            [row[src], row[src + 1], row[src + 2], row[src + 3]]
                        }
                        crate::ColorType::Indexed => unreachable!("palette expansion enabled"),
                    };
                    row[dst..dst + 4].copy_from_slice(&rgba);
                }
                match interlace {
                    crate::InterlaceInfo::Adam7(a) => crate::adam7::expand_pass_controlled(
                        target,
                        frame_width as usize * 4,
                        &row[..samples * 4],
                        &a,
                        32,
                        &self.control,
                    )?,
                    _ => {
                        let start = interlace.line_number() as usize * frame_width as usize * 4;
                        for (source, destination) in row[..samples * 4]
                            .chunks(4096)
                            .zip(target[start..start + samples * 4].chunks_mut(4096))
                        {
                            self.control.checkpoint(source.len())?;
                            destination.copy_from_slice(source);
                        }
                    }
                }
            }
            self.reader.finish()?;
            let consumed = self.reader.input_position()?;
            Ok(DecodedInfo {
                finish: FinishInfo {
                    consumed_bytes: consumed as usize,
                    encoded_bytes: self.encoded_bytes,
                },
                output: crate::OutputInfo {
                    width: frame_width,
                    height: frame_height,
                    color_type: crate::ColorType::Rgba,
                    bit_depth: crate::BitDepth::Eight,
                    line_size: frame_width as usize * 4,
                },
            })
        })();
        let result = self.control.guard(result);
        let control = self.control.clone();
        drop(row);
        drop(self);
        control.checkpoint(0)?;
        result
    }
}
