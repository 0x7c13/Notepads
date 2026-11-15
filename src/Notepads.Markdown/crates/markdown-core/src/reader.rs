// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE file in the project root for license information.
// ---------------------------------------------------------------------------------------------

//! Bounded logical Main text from an accepted model, without a second parser.

pub(crate) mod text;

use crate::error::CoreError;
use crate::ir::{AtomContent, AtomKind};
use crate::model::AcceptedModel;
use crate::policy::Operation;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ReadResult {
    pub written: usize,
    pub utf16: u64,
    pub done: bool,
}

pub struct MainReader {
    model: AcceptedModel,
    op: Operation,
    atom: usize,
    end: usize,
    byte: usize,
    utf16: u64,
    expected: u64,
}

impl MainReader {
    /// Pre-admit a boundary result write before committing the next batch.
    /// A failed write admission latches the same reader fault and cannot reset work.
    pub fn boundary_result(&mut self, bytes: u64) -> Result<(), CoreError> {
        self.op.copy(bytes)
    }

    pub fn new(model: AcceptedModel, mut op: Operation) -> Result<Self, CoreError> {
        op.ensure_budget(model.budget())?;
        op.events(1)?;
        let flow = model.ir().flows()[model.main_flow_id().index()];
        let end = flow
            .atoms
            .start()
            .checked_add(flow.atoms.len())
            .ok_or_else(|| op.fail(CoreError::ArithmeticOverflow))?;
        op.checkpoint()?;
        Ok(Self {
            model,
            op,
            atom: flow.atoms.start(),
            end,
            byte: 0,
            utf16: 0,
            expected: flow.logical_utf16,
        })
    }

    /// A failed batch may have written bytes; discard the whole failed batch.
    /// Its counters and fault remain latched, while its cursor is not committed.
    pub fn read_into(&mut self, buffer: &mut [u8]) -> Result<ReadResult, CoreError> {
        self.op.checkpoint()?;
        if !(4..=65_536).contains(&buffer.len()) {
            return Err(self.op.fail(CoreError::InvalidRange));
        }
        let mut atom = self.atom;
        let mut byte = self.byte;
        let mut utf16 = self.utf16;
        let mut written = 0;
        while atom < self.end {
            self.op.events(1)?;
            let row = self.model.ir().atom_at(atom, &mut self.op)?;
            let text = match row.kind {
                AtomKind::Hidden => "",
                AtomKind::Object => "\u{fffc}",
                AtomKind::Copy => {
                    let parts = row
                        .sources
                        .slice(self.model.ir().sources())
                        .map_err(|e| self.op.fail(e))?;
                    if parts.len() != 1 {
                        return Err(self.op.fail(CoreError::InternalInvariant));
                    }
                    let start = usize::try_from(parts[0].start())
                        .map_err(|_| self.op.fail(CoreError::ArithmeticOverflow))?;
                    let end = usize::try_from(parts[0].end())
                        .map_err(|_| self.op.fail(CoreError::ArithmeticOverflow))?;
                    self.model
                        .source()
                        .text()
                        .get(start..end)
                        .ok_or_else(|| self.op.fail(CoreError::InvalidBoundary))?
                }
                AtomKind::Transform | AtomKind::Synthetic => {
                    let AtomContent::Pool(value) = row.content else {
                        return Err(self.op.fail(CoreError::InternalInvariant));
                    };
                    self.model.ir().text_query(value, &mut self.op)?
                }
            };
            if !text.is_char_boundary(byte) {
                return Err(self.op.fail(CoreError::InvalidBoundary));
            }
            if byte == text.len() {
                atom += 1;
                byte = 0;
                continue;
            }
            while byte < text.len() && written < buffer.len() {
                // At most 256 bytes/scalars are examined between same-operation
                // checks, including the closed UTF-16 counting loop below. The
                // accepted immutable atom is resolved only once per batch.
                let mut end = (byte + 256)
                    .min(text.len())
                    .min(byte + buffer.len() - written);
                while !text.is_char_boundary(end) {
                    self.op.scan(1)?;
                    end -= 1;
                }
                if end == byte {
                    break;
                }
                let chunk = &text[byte..end];
                self.op.scan(chunk.len() as u64)?;
                let mut units = 0;
                let mut scalars = 0;
                for scalar in chunk.chars() {
                    units += scalar.len_utf16() as u64;
                    scalars += 1;
                }
                self.op.events(scalars)?;
                self.op.copy(chunk.len() as u64)?;
                buffer[written..written + chunk.len()].copy_from_slice(chunk.as_bytes());
                utf16 = utf16
                    .checked_add(units)
                    .ok_or_else(|| self.op.fail(CoreError::ArithmeticOverflow))?;
                written += chunk.len();
                byte = end;
            }
            if byte == text.len() {
                atom += 1;
                byte = 0;
            } else {
                break;
            }
        }
        if atom == self.end && utf16 != self.expected {
            return Err(self.op.fail(CoreError::InternalInvariant));
        }
        self.op.checkpoint()?;
        self.atom = atom;
        self.byte = byte;
        self.utf16 = utf16;
        Ok(ReadResult {
            written,
            utf16,
            done: atom == self.end,
        })
    }
}

#[cfg(test)]
mod tests;
