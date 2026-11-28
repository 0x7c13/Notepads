// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE file in the project root for license information.
// ---------------------------------------------------------------------------------------------

use super::{SourceSnapshot, utf8};
use crate::error::{CoreError, LimitKind};
use crate::identity::{ParseSpan, SourceSpan};
use crate::memory::{BuildString, BuildVec, FrozenRows, FrozenText};
use crate::policy::{CHECKPOINT_EVENTS, Operation, OwnedWork, construction_scope};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum InputPieceKind {
    Copy,
    CrLf,
    Cr,
    Nul,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct InputPiece {
    pub source: SourceSpan,
    pub parse: ParseSpan,
    pub kind: InputPieceKind,
}

const _: () = assert!(size_of::<InputPiece>() <= 40);

enum ParseText {
    Identity,
    Normalized(FrozenText),
}

/// Owns its original S pin, optional normalized P, and admitted minimal N01 pieces.
pub struct ParseInput {
    source: SourceSnapshot,
    text: ParseText,
    pieces: FrozenRows<InputPiece>,
}

impl ParseInput {
    pub fn prepare(source: SourceSnapshot, op: &mut Operation) -> Result<Self, CoreError> {
        construction_scope(op, |mut work| {
            let input = Self::prepare_owned(source, &mut work).map_err(|error| work.fail(error))?;
            work.finish()?;
            Ok(input)
        })
    }

    /// Full model construction shares the original brand through index/freeze;
    /// the public standalone wrapper returns only this immutable input.
    pub(crate) fn prepare_owned<'id>(
        source: SourceSnapshot,
        op: &mut OwnedWork<'id, '_>,
    ) -> Result<Self, CoreError> {
        op.checkpoint_now()?;
        op.ensure_budget(source.budget())?;
        let plan = plan(source.text(), op)?;
        let mut pieces = BuildVec::new(plan.pieces, op)?;
        let text = if !plan.changed {
            if !source.text().is_empty() {
                pieces.push(
                    InputPiece {
                        source: span(0, source.text().len(), op)?,
                        parse: parse_span(0, source.text().len(), op)?,
                        kind: InputPieceKind::Copy,
                    },
                    op,
                )?;
            }
            ParseText::Identity
        } else {
            let mut text = BuildString::auxiliary(plan.output, op)?;
            let mut pending_copy: Option<InputPiece> = None;
            scan(source.text(), op, |start, end, kind, op| {
                let parse_start = text.len();
                let output = match kind {
                    InputPieceKind::Copy => &source.text()[start..end],
                    InputPieceKind::CrLf | InputPieceKind::Cr => "\n",
                    InputPieceKind::Nul => "\u{fffd}",
                };
                text.append(output, op)?;
                let next = InputPiece {
                    source: span(start, end, op)?,
                    parse: parse_span(parse_start, text.len(), op)?,
                    kind,
                };
                if kind == InputPieceKind::Copy {
                    pending_copy = Some(match pending_copy.take() {
                        Some(previous) => InputPiece {
                            source: SourceSpan::new(previous.source.start(), next.source.end())
                                .map_err(|error| op.fail(error))?,
                            parse: ParseSpan::new(previous.parse.start(), next.parse.end())
                                .map_err(|error| op.fail(error))?,
                            kind,
                        },
                        None => next,
                    });
                } else {
                    if let Some(copy) = pending_copy.take() {
                        pieces.push(copy, op)?;
                    }
                    pieces.push(next, op)?;
                }
                Ok(())
            })?;
            if let Some(copy) = pending_copy {
                pieces.push(copy, op)?;
            }
            if text.len() != plan.output || pieces.len() != plan.pieces {
                return Err(op.fail(CoreError::InternalInvariant));
            }
            ParseText::Normalized(text.freeze(op)?)
        };
        op.checkpoint_now()?;
        Ok(Self {
            source,
            text,
            pieces: pieces.freeze(op)?,
        })
    }

    pub fn source(&self) -> &SourceSnapshot {
        &self.source
    }

    pub fn text(&self) -> &str {
        match &self.text {
            ParseText::Identity => self.source.text(),
            ParseText::Normalized(text) => text.as_str(),
        }
    }

    pub fn pieces(&self) -> &[InputPiece] {
        self.pieces.as_slice()
    }

    pub fn is_identity(&self) -> bool {
        matches!(self.text, ParseText::Identity)
    }
}

struct Plan {
    output: usize,
    pieces: usize,
    changed: bool,
}

fn plan(source: &str, op: &mut OwnedWork<'_, '_>) -> Result<Plan, CoreError> {
    let mut plan = Plan {
        output: 0,
        pieces: 0,
        changed: false,
    };
    let mut previous_copy = false;
    scan(source, op, |start, end, kind, op| {
        let output = match kind {
            InputPieceKind::Copy => end - start,
            InputPieceKind::CrLf | InputPieceKind::Cr => 1,
            InputPieceKind::Nul => 3,
        };
        plan.output = plan
            .output
            .checked_add(output)
            .ok_or_else(|| op.fail(CoreError::ArithmeticOverflow))?;
        if kind != InputPieceKind::Copy || !previous_copy {
            plan.pieces = plan
                .pieces
                .checked_add(1)
                .ok_or_else(|| op.fail(CoreError::ArithmeticOverflow))?;
        }
        previous_copy = kind == InputPieceKind::Copy;
        plan.changed |= !previous_copy;
        Ok(())
    })?;
    let output = u64::try_from(plan.output).map_err(|_| op.fail(CoreError::ArithmeticOverflow))?;
    let pieces = u64::try_from(plan.pieces).map_err(|_| op.fail(CoreError::ArithmeticOverflow))?;
    let limits = op.limits();
    op.check_limit(LimitKind::ParseBytes, output, limits.max_parse_bytes)?;
    op.check_limit(LimitKind::InputPieces, pieces, limits.max_input_pieces)?;
    Ok(plan)
}

/// Scan scalar-aligned byte windows. CRLF at a window boundary is deferred as one atom.
fn scan<'id>(
    source: &str,
    op: &mut OwnedWork<'id, '_>,
    mut emit: impl FnMut(usize, usize, InputPieceKind, &mut OwnedWork<'id, '_>) -> Result<(), CoreError>,
) -> Result<(), CoreError> {
    let bytes = source.as_bytes();
    let mut cursor = 0;
    let mut events = 0;
    while cursor < bytes.len() {
        let mut end = utf8::end_owned(source, cursor, op)?;
        if end < bytes.len() {
            op.scan(2)?;
            if bytes[end - 1] == b'\r' && bytes[end] == b'\n' {
                end -= 1;
            }
        }
        op.scan((end - cursor) as u64)?;
        let mut copy_start = cursor;
        while cursor < end {
            let (length, kind) = match bytes[cursor] {
                b'\r' if cursor + 1 < end && bytes[cursor + 1] == b'\n' => {
                    (2, InputPieceKind::CrLf)
                }
                b'\r' => (1, InputPieceKind::Cr),
                0 => (1, InputPieceKind::Nul),
                _ => {
                    cursor += 1;
                    continue;
                }
            };
            if copy_start < cursor {
                emit(copy_start, cursor, InputPieceKind::Copy, op)?;
                event(&mut events, op)?;
            }
            emit(cursor, cursor + length, kind, op)?;
            event(&mut events, op)?;
            cursor += length;
            copy_start = cursor;
        }
        if copy_start < cursor {
            emit(copy_start, cursor, InputPieceKind::Copy, op)?;
            event(&mut events, op)?;
        }
    }
    if events > 0 {
        op.events(events)?;
    }
    op.check()
}

fn event(events: &mut u64, op: &mut OwnedWork<'_, '_>) -> Result<(), CoreError> {
    *events += 1;
    if *events == CHECKPOINT_EVENTS {
        op.events(*events)?;
        *events = 0;
    }
    Ok(())
}

fn span(start: usize, end: usize, op: &mut OwnedWork<'_, '_>) -> Result<SourceSpan, CoreError> {
    SourceSpan::new(start as u64, end as u64).map_err(|error| op.fail(error))
}

fn parse_span(
    start: usize,
    end: usize,
    op: &mut OwnedWork<'_, '_>,
) -> Result<ParseSpan, CoreError> {
    ParseSpan::new(start as u64, end as u64).map_err(|error| op.fail(error))
}
