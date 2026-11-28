// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE file in the project root for license information.
// ---------------------------------------------------------------------------------------------
//! Slice driver for the existing streaming parser and first-frame converter.
use super::{
    converter::{ColorOutput, PixelConverter},
    ApplicationMetadata, DecodeOptions, Decoded, DecodingError, FrameDataType, MemoryLimit,
    OutputBuffer, StreamingDecoder,
};
use crate::{
    common::{Block, Frame},
    storage::{self, Buffer, Control},
    AnyExtension, Extension,
};
use markdown_svg_budget::{Error, FailureKind, LimitKind, ResourcesRef};

/// Copy-only frame dimensions; the frame offset is preserved without compositing.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ImageInfo {
    /// First frame width, not the logical screen width.
    pub width: usize,
    /// First frame height, not the logical screen height.
    pub height: usize,
    /// Caller-owned RGBA output length.
    pub output_bytes: usize,
    /// Original first frame horizontal offset.
    pub left: u16,
    /// Original first frame vertical offset.
    pub top: u16,
}

struct SliceReader<'a> {
    input: &'a [u8],
    position: usize,
    at_eof: bool,
    decoder: StreamingDecoder,
    control: Control,
}
impl<'a> SliceReader<'a> {
    fn new(
        input: &'a [u8],
        options: &DecodeOptions,
        control: &Control,
    ) -> Result<Self, DecodingError> {
        Ok(Self {
            input,
            position: 0,
            at_eof: false,
            decoder: StreamingDecoder::with_control(options, control.clone())?,
            control: control.clone(),
        })
    }
    fn next(&mut self, output: &mut OutputBuffer<'_>) -> Result<Option<Decoded>, DecodingError> {
        while !self.at_eof {
            let input = self
                .input
                .get(self.position..)
                .ok_or(DecodingError::UnexpectedEof)?;
            if input.is_empty() {
                return Err(DecodingError::UnexpectedEof);
            }
            let (consumed, event) = self.decoder.update(input, output)?;
            self.position = self
                .position
                .checked_add(consumed)
                .ok_or(DecodingError::UnexpectedEof)?;
            match event {
                Decoded::Nothing => {}
                Decoded::BlockStart(Block::Trailer) => {
                    self.at_eof = true;
                    return Ok(Some(event));
                }
                _ => return Ok(Some(event)),
            }
        }
        Ok(None)
    }
    fn bytes(&mut self, output: &mut OutputBuffer<'_>) -> Result<usize, DecodingError> {
        match self.next(output)? {
            Some(Decoded::BytesDecoded(length)) => Ok(length.get()),
            Some(Decoded::DataEnd) => Ok(0),
            _ => Err(storage::format(&self.control, "unexpected data")),
        }
    }
}

#[derive(Clone, Copy)]
enum ProbeStop {
    Trailer,
    OrdinaryBeforeEnd,
    OrdinaryAfterEnd,
}
#[derive(Clone, Copy)]
enum LoopPart {
    Name,
    Payload,
    Complete,
    Other,
}
fn animated(control: &Control) -> DecodingError {
    storage::resource(control.terminal(Error::new(FailureKind::Unsupported, LimitKind::None)))
}
fn probe(input: &[u8], control: &Control) -> Result<ProbeStop, DecodingError> {
    let mut options = DecodeOptions::new();
    options.skip_frame_decoding(true);
    let mut reader = SliceReader::new(input, &options, control)?;
    let mut frames = 0usize;
    let mut first_end = false;
    let mut loop_part = LoopPart::Name;
    let stop = loop {
        let event = match reader.next(&mut OutputBuffer::None) {
            Ok(Some(event)) => event,
            Ok(None) => break ProbeStop::Trailer,
            Err(error) => {
                if error.resource_error().is_some() {
                    return Err(error);
                }
                storage::check(control)?;
                break if first_end {
                    ProbeStop::OrdinaryAfterEnd
                } else {
                    ProbeStop::OrdinaryBeforeEnd
                };
            }
        };
        match event {
            Decoded::FrameMetadata(_) => {
                frames += 1;
                if first_end && frames >= 2 {
                    // Confirm before the probe's paid palette/ext owners drop.
                    return Err(animated(control));
                }
            }
            Decoded::DataEnd => first_end = true,
            Decoded::BlockStart(Block::Trailer) => break ProbeStop::Trailer,
            Decoded::BlockStart(Block::Extension) => loop_part = LoopPart::Name,
            Decoded::SubBlock {
                ext: AnyExtension(label),
                is_last,
            } if label == Extension::Application as u8 => {
                let data = reader.decoder.last_ext_sub_block();
                loop_part = match loop_part {
                    LoopPart::Name if data == b"NETSCAPE2.0" || data == b"ANIMEXTS1.0" => {
                        LoopPart::Payload
                    }
                    LoopPart::Payload if matches!(data, [1, _, _]) => LoopPart::Complete,
                    LoopPart::Complete => LoopPart::Complete,
                    _ => LoopPart::Other,
                };
                if is_last {
                    if matches!(loop_part, LoopPart::Complete) {
                        return Err(animated(control));
                    }
                    loop_part = LoopPart::Name;
                }
            }
            _ => {}
        }
    };
    // Ordinary candidate tails never poison the request. A real failure from
    // probe cleanup still rejects before entering the first-frame algorithm.
    drop(reader);
    storage::check(control)?;
    Ok(stop)
}

/// Same first-frame decoder with all internal allocations paid by one request.
pub struct ControlledDecoder<'a> {
    input: &'a [u8],
    control: Control,
    reader: Option<SliceReader<'a>>,
    converter: PixelConverter,
    metadata: ApplicationMetadata,
    frame: Frame<'static>,
    local_palette: Option<Buffer<u8>>,
    info: Option<ImageInfo>,
}
impl<'a> ControlledDecoder<'a> {
    /// Choose the paid policy before the first parser or LZW allocation.
    pub fn new(input: &'a [u8], resources: ResourcesRef) -> Result<Self, DecodingError> {
        let control = Control::paid(resources);
        storage::step(&control, 0)?;
        Ok(Self {
            input,
            reader: None,
            converter: PixelConverter::with_control(ColorOutput::RGBA, control.clone()),
            metadata: ApplicationMetadata::new(control.clone()),
            frame: Frame::default(),
            local_palette: None,
            info: None,
            control,
        })
    }
    /// Confirm static animation semantics and read the original first frame.
    pub fn read_info(&mut self) -> Result<ImageInfo, DecodingError> {
        storage::check(&self.control)?;
        if let Some(info) = self.info {
            return Ok(info);
        }
        let _stop = probe(self.input, &self.control)?;
        let options = DecodeOptions::new();
        let mut reader = SliceReader::new(self.input, &options, &self.control)?;
        // This borrowed helper owns no temporary paid data. Record ordinary
        // terminal errors here before the local reader can start cleanup.
        let result = self.prepare(&mut reader, &options.memory_limit);
        let info = match result {
            Ok(info) => info,
            Err(error) => return Err(storage::terminal(&self.control, error)),
        };
        self.reader = Some(reader);
        storage::check(&self.control)?;
        self.info = Some(info);
        Ok(info)
    }
    fn prepare(
        &mut self,
        reader: &mut SliceReader<'a>,
        memory_limit: &MemoryLimit,
    ) -> Result<ImageInfo, DecodingError> {
        let mut header_end = false;
        loop {
            match reader.next(&mut OutputBuffer::None)? {
                Some(Decoded::GlobalPaletteBorrowed) => {
                    self.converter
                        .set_paid_global_palette(reader.decoder.take_global_palette())?;
                }
                Some(Decoded::SubBlock {
                    ext: AnyExtension(label),
                    is_last,
                }) if !header_end && label == Extension::Application as u8 => {
                    self.metadata.read(
                        reader.decoder.last_ext_sub_block(),
                        is_last,
                        memory_limit,
                    )?;
                }
                Some(Decoded::HeaderEnd) => header_end = true,
                Some(Decoded::FrameMetadata(FrameDataType::Pixels)) => {
                    self.frame = reader.decoder.current_frame_mut().take();
                    self.local_palette = reader.decoder.take_local_palette();
                    storage::check(&self.control)?;
                    if self.local_palette.is_none() && self.converter.global_palette().is_none() {
                        return Err(storage::format(
                            &self.control,
                            "no color table available for current frame",
                        ));
                    }
                    // Apply the existing controlled per-image pixel gate while
                    // dimensions are scalar, before the legacy frame-memory
                    // check or any size-dependent caller output allocation.
                    if let Some(resources) = self.control.resources() {
                        resources
                            .check_limit(
                                LimitKind::Capacity,
                                u64::from(self.frame.width) * u64::from(self.frame.height),
                                16 * 1024 * 1024,
                            )
                            .map_err(storage::resource)?;
                    }
                    let output_bytes = self
                        .converter
                        .check_buffer_size(&self.frame, memory_limit)?;
                    return Ok(ImageInfo {
                        width: self.frame.width as usize,
                        height: self.frame.height as usize,
                        output_bytes,
                        left: self.frame.left,
                        top: self.frame.top,
                    });
                }
                Some(Decoded::BlockStart(Block::Trailer)) | None => {
                    return Err(storage::format(
                        &self.control,
                        "file does not contain any image data",
                    ));
                }
                Some(_) => {}
            }
        }
    }
    /// Decode RGBA directly into caller storage; no Frame/Cow/Vec can escape.
    pub fn decode_into(&mut self, output: &mut [u8]) -> Result<(), DecodingError> {
        let info = self.read_info()?;
        if output.len() < info.output_bytes {
            return Err(storage::terminal(
                &self.control,
                storage::format(&self.control, "buffer too small"),
            ));
        }
        let reader = self.reader.as_mut().ok_or_else(|| {
            storage::terminal(&self.control, storage::format(&self.control, "bad state"))
        })?;
        self.converter
            .read_into_buffer_with_palette(
                &self.frame,
                self.local_palette.as_deref(),
                output,
                &mut |buffer| reader.bytes(buffer),
            )
            .map_err(|error| storage::terminal(&self.control, error))?;
        storage::check(&self.control)
    }
}
