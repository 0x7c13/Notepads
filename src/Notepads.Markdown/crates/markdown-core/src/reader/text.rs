// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE file in the project root for license information.
// ---------------------------------------------------------------------------------------------

use crate::error::CoreError;
use crate::identity::LogicalSpan;
use crate::ir::{AtomContent, AtomKind, AtomRow, FlowId};
use crate::model::AcceptedModel;
use crate::policy::Operation;

pub(crate) fn atoms(
    model: &AcceptedModel,
    flow: FlowId,
    range: LogicalSpan,
    op: &mut Operation,
    mut emit: impl FnMut(AtomRow, LogicalSpan, &mut Operation) -> Result<(), CoreError>,
) -> Result<(), CoreError> {
    op.ensure_budget(model.budget())?;
    let row = model
        .ir()
        .flows()
        .get(flow.index())
        .copied()
        .ok_or_else(|| op.fail(CoreError::UnknownId))?;
    if range.end() > row.logical_utf16 {
        return Err(op.fail(CoreError::InvalidRange));
    }
    if range.is_empty() {
        // Empty endpoints must still be scalar boundaries if inside an atom.
        if range.start() < row.logical_utf16 {
            let index = model.flow_atom_covering(flow, range.start(), op)?.index();
            let atom = model.ir().atom_at(index, op)?;
            let local = range
                .start()
                .checked_sub(atom.target.start)
                .ok_or_else(|| op.fail(CoreError::InvalidProvenance))?;
            let text = atom_text(model, atom, op)?;
            scalar_slice(text, local, local, op)?;
        }
        return Ok(());
    }
    let mut index = model.flow_atom_covering(flow, range.start(), op)?.index();
    let end = row
        .atoms
        .start()
        .checked_add(row.atoms.len())
        .ok_or_else(|| op.fail(CoreError::ArithmeticOverflow))?;
    let mut covered = range.start();
    while index < end {
        let atom = model.ir().atom_at(index, op)?;
        if atom.target.start >= range.end() {
            break;
        }
        index += 1;
        if atom.kind == AtomKind::Hidden || atom.target.start == atom.target.end {
            continue;
        }
        let start = atom.target.start.max(range.start());
        let finish = atom.target.end.min(range.end());
        if start != covered || start >= finish {
            return Err(op.fail(CoreError::InvalidProvenance));
        }
        let local_start = start - atom.target.start;
        let local_end = finish - atom.target.start;
        if start != atom.target.start || finish != atom.target.end {
            scalar_slice(atom_text(model, atom, op)?, local_start, local_end, op)?;
        }
        emit(
            atom,
            LogicalSpan::new(start, finish).map_err(|e| op.fail(e))?,
            op,
        )?;
        covered = finish;
    }
    if covered != range.end() {
        return Err(op.fail(CoreError::InvalidProvenance));
    }
    Ok(())
}

pub(crate) fn visit(
    model: &AcceptedModel,
    flow: FlowId,
    range: LogicalSpan,
    op: &mut Operation,
    mut emit: impl FnMut(&str, &mut Operation) -> Result<(), CoreError>,
) -> Result<(), CoreError> {
    atoms(model, flow, range, op, |atom, part, op| {
        emit(atom_part(model, atom, part, op)?, op)
    })
}

pub(crate) fn atom_part<'a>(
    model: &'a AcceptedModel,
    atom: AtomRow,
    part: LogicalSpan,
    op: &mut Operation,
) -> Result<&'a str, CoreError> {
    let start = part
        .start()
        .checked_sub(atom.target.start)
        .ok_or_else(|| op.fail(CoreError::InvalidRange))?;
    let end = part
        .end()
        .checked_sub(atom.target.start)
        .ok_or_else(|| op.fail(CoreError::InvalidRange))?;
    if part.end() > atom.target.end {
        return Err(op.fail(CoreError::InvalidRange));
    }
    if part.start() == atom.target.start && part.end() == atom.target.end {
        // Independent acceptance already proved both whole-atom endpoints and
        // its UTF-16 width. Partial ranges still resolve scalar boundaries.
        return atom_text(model, atom, op);
    }
    scalar_slice(atom_text(model, atom, op)?, start, end, op)
}

fn atom_text<'a>(
    model: &'a AcceptedModel,
    atom: AtomRow,
    op: &mut Operation,
) -> Result<&'a str, CoreError> {
    match atom.kind {
        AtomKind::Hidden => Ok(""),
        AtomKind::Object => Ok("\u{fffc}"),
        AtomKind::Transform | AtomKind::Synthetic => {
            let AtomContent::Pool(value) = atom.content else {
                return Err(op.fail(CoreError::InvalidProvenance));
            };
            model.ir().text_query(value, op)
        }
        AtomKind::Copy => {
            let parts = atom
                .sources
                .slice(model.ir().sources())
                .map_err(|e| op.fail(e))?;
            if parts.len() != 1 {
                return Err(op.fail(CoreError::InvalidProvenance));
            }
            let start = usize::try_from(parts[0].start())
                .map_err(|_| op.fail(CoreError::ArithmeticOverflow))?;
            let end = usize::try_from(parts[0].end())
                .map_err(|_| op.fail(CoreError::ArithmeticOverflow))?;
            model
                .source()
                .text()
                .get(start..end)
                .ok_or_else(|| op.fail(CoreError::InvalidBoundary))
        }
    }
}

fn scalar_slice<'a>(
    value: &'a str,
    start: u64,
    end: u64,
    op: &mut Operation,
) -> Result<&'a str, CoreError> {
    let mut units = 0_u64;
    let mut first = (start == 0).then_some(0);
    let mut last = (end == 0).then_some(0);
    for (byte, scalar) in value.char_indices() {
        if last.is_some() {
            break;
        }
        op.scan(scalar.len_utf8() as u64)?;
        units = units
            .checked_add(scalar.len_utf16() as u64)
            .ok_or_else(|| op.fail(CoreError::ArithmeticOverflow))?;
        let next = byte + scalar.len_utf8();
        if units == start {
            first = Some(next);
        }
        if units == end {
            last = Some(next);
        }
        if (units > start && first.is_none()) || units > end {
            return Err(op.fail(CoreError::InvalidBoundary));
        }
    }
    let (Some(first), Some(last)) = (first, last) else {
        return Err(op.fail(CoreError::InvalidBoundary));
    };
    value
        .get(first..last)
        .ok_or_else(|| op.fail(CoreError::InvalidBoundary))
}
