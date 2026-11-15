// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE file in the project root for license information.
// ---------------------------------------------------------------------------------------------
//! Request-owned gzip framing around the unchanged registry DEFLATE kernel.
//! Header metadata is never retained. One exact paid state serves all members.
use miniz_oxide::{
    DataFormat, MZFlush, MZStatus,
    inflate::stream::{InflateState, inflate},
};
use markdown_svg_budget::{
    BudgetVec, Error, FailureKind, LimitKind, ResourceKind, ResourcesRef,
};

const WINDOW: usize = 4096;
const DICTIONARY: usize = 32 * 1024;
const MAX_HEADER_FIELD: usize = 65_535;

fn invalid(resources: &ResourcesRef) -> Error {
    // Fix the format cause before any output/state owner can refund on exit.
    resources.record(Error::new(FailureKind::Decode, LimitKind::None))
}

struct Input<'a> {
    bytes: &'a [u8],
    at: usize,
    resources: &'a ResourcesRef,
}

impl<'a> Input<'a> {
    fn read(&mut self, length: usize) -> Result<&'a [u8], Error> {
        self.resources.checkpoint(length as u64)?;
        let end = self
            .at
            .checked_add(length)
            .ok_or_else(|| invalid(self.resources))?;
        let result = self
            .bytes
            .get(self.at..end)
            .ok_or_else(|| invalid(self.resources))?;
        self.at = end;
        Ok(result)
    }

    fn header_bytes(&mut self, length: usize, crc: &mut flate2::Crc) -> Result<(), Error> {
        let mut remaining = length;
        while remaining != 0 {
            let count = remaining.min(WINDOW);
            let bytes = self.read(count)?;
            // read paid this bounded borrowed header/CRC pass before execution.
            crc.update(bytes);
            remaining -= count;
        }
        Ok(())
    }

    fn zero_field(&mut self, crc: &mut flate2::Crc) -> Result<(), Error> {
        let mut length = 0;
        loop {
            // Include the terminator in the search; exactly 65535 nonzero bytes
            // followed by zero has the same acceptance as flate2 1.1.10.
            let extent = (MAX_HEADER_FIELD - length + 1)
                .min(WINDOW)
                .min(self.bytes.len() - self.at);
            if extent == 0 {
                return Err(invalid(self.resources));
            }
            self.resources.checkpoint(extent as u64)?;
            let part = &self.bytes[self.at..self.at + extent];
            if let Some(end) = part.iter().position(|&byte| byte == 0) {
                self.header_bytes(end + 1, crc)?;
                return Ok(());
            }
            if extent > MAX_HEADER_FIELD - length {
                return Err(invalid(self.resources));
            }
            self.header_bytes(extent, crc)?;
            length += extent;
        }
    }

    fn header(&mut self) -> Result<(), Error> {
        let fixed = self.read(10)?;
        if fixed[..3] != [0x1f, 0x8b, 8] || fixed[3] & 0xe0 != 0 {
            return Err(invalid(self.resources));
        }
        let flags = fixed[3];
        let mut crc = flate2::Crc::new();
        crc.update(fixed);
        if flags & 4 != 0 {
            let length = self.read(2)?;
            crc.update(length);
            self.header_bytes(
                u16::from_le_bytes([length[0], length[1]]) as usize,
                &mut crc,
            )?;
        }
        if flags & 8 != 0 {
            self.zero_field(&mut crc)?;
        }
        if flags & 16 != 0 {
            self.zero_field(&mut crc)?;
        }
        if flags & 2 != 0 {
            let expected = self.read(2)?;
            if u16::from_le_bytes([expected[0], expected[1]]) != crc.sum() as u16 {
                return Err(invalid(self.resources));
            }
        }
        self.resources.checkpoint(0)
    }
}

pub(super) fn decode(data: &[u8], resources: &ResourcesRef) -> Result<BudgetVec<u8>, Error> {
    resources.checkpoint(0)?;
    resources.check_limit(
        LimitKind::InputBytes,
        data.len() as u64,
        resources.limits().input_bytes,
    )?;
    let result = decode_inner(data, resources);
    // decode_inner's state has physically freed and refunded. Never publish an
    // output after a fault in that natural owner retirement.
    resources.checkpoint(0)?;
    result
}

fn decode_inner(data: &[u8], resources: &ResourcesRef) -> Result<BudgetVec<u8>, Error> {
    let mut input = Input {
        bytes: data,
        at: 0,
        resources,
    };
    input.header()?;
    // Admission of the actual exact state allocation occurs inside BudgetVec.
    // No InflateState constructor is evaluated if reserve or commit rejects.
    resources.checkpoint(size_of::<InflateState>() as u64)?;
    let mut state = BudgetVec::new(resources.clone(), ResourceKind::Scratch);
    state.try_reserve_exact(1)?;
    resources.checkpoint(0)?;
    state.push_reserved(InflateState::new(DataFormat::Raw));
    let mut output = BudgetVec::new(resources.clone(), ResourceKind::Scratch);
    let mut buffer = [0u8; WINDOW];
    loop {
        let mut crc = flate2::Crc::new();
        loop {
            let end = input.at.saturating_add(WINDOW).min(data.len());
            // A call can fill a <WINDOW dictionary tail, wrap, then fill the
            // complete 32 KiB dictionary. It also copies <=WINDOW bytes out.
            // Pay both extra windows, rather than treating dictionary capacity
            // as the total number of bytes one invocation can write.
            resources.checkpoint((end - input.at + DICTIONARY + 2 * WINDOW) as u64)?;
            let step = inflate(
                &mut state[0],
                &data[input.at..end],
                &mut buffer,
                MZFlush::None,
            );
            input.at += step.bytes_consumed;
            if step.status.is_err() {
                return Err(invalid(resources));
            }
            let bytes = &buffer[..step.bytes_written];
            resources.checkpoint(bytes.len() as u64)?;
            crc.update(bytes);
            resources.add_child_encoded(bytes.len())?;
            output.try_extend_from_slice(bytes)?;
            if step.status == Ok(MZStatus::StreamEnd) {
                break;
            }
            if step.bytes_consumed == 0 && step.bytes_written == 0 {
                return Err(invalid(resources));
            }
        }
        let trailer = input.read(8)?;
        let expected_crc = u32::from_le_bytes(trailer[..4].try_into().unwrap());
        let expected_size = u32::from_le_bytes(trailer[4..].try_into().unwrap());
        if expected_crc != crc.sum() || expected_size != crc.amount() {
            return Err(invalid(resources));
        }
        if input.at == data.len() {
            break;
        }
        // MultiGzDecoder rejects any non-member tail. Parse the next complete
        // header before resetting the same paid state; no metadata heap exists.
        input.header()?;
        resources.checkpoint(size_of::<InflateState>() as u64)?;
        state[0].reset(DataFormat::Raw);
    }
    resources.checkpoint(0)?;
    Ok(output)
}
