// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE file in the project root for license information.
// ---------------------------------------------------------------------------------------------

use crate::error::{CoreError, LimitKind};
use crate::identity::{ModelIdentity, SourceSpan};
pub(super) use crate::ir::validation_output::Checkpoint;
use crate::ir::validation_output::{FixedIndexCounts, SPARSE_BYTES, VerifiedFacts, VerifiedParts};
use crate::ir::{AtomContent, AtomId, AtomKind, AtomRow, CompactIr, FlowId, Rows, TextTarget};
use crate::memory::{BuildExactRows, BuildGrowingVec, BuildSortRows, FrozenRows};
use crate::policy::{Operation, OwnedWork, ScalarBatchEnd, ScalarChargeOrder, SourceBudget};
use crate::source::SourceSnapshot;
use std::ops::ControlFlow;

mod task;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) struct Posting {
    pub span: SourceSpan,
    pub atom: AtomId,
    pub part: u32,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) struct TargetPosting {
    pub start: u64,
    pub end: u64,
    pub atom: AtomId,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) struct TargetIndex {
    pub domains: Rows<SourceSpan>,
    pub length: u64,
    pub empty_anchor: Option<u64>,
    pub source: Rows<Posting>,
    pub synthetic: Rows<Posting>,
    pub target: Rows<TargetPosting>,
    pub hidden: Rows<TargetPosting>,
}

struct TargetInput {
    target: TextTarget,
    domains: Rows<SourceSpan>,
    atoms: Rows<AtomRow>,
    length: u64,
}

fn add_count<'id>(count: &mut usize, op: &mut OwnedWork<'id, '_>) -> Result<(), CoreError> {
    *count = count
        .checked_add(1)
        .ok_or_else(|| op.fail(CoreError::ArithmeticOverflow))?;
    Ok(())
}
fn count_target<'id>(
    ir: &CompactIr,
    atoms: Rows<AtomRow>,
    counts: &mut FixedIndexCounts,
    op: &mut OwnedWork<'id, '_>,
) -> Result<(), CoreError> {
    for index in atoms.indices(ir.atoms_len()).map_err(|e| op.fail(e))? {
        let atom = ir.owned_read_atom_at(index, op)?;
        op.events(1)?;
        if atom.target.start < atom.target.end {
            add_count(&mut counts.target, op)?;
        } else {
            add_count(&mut counts.hidden, op)?;
        }
        for span in atom.sources.slice(ir.sources()).map_err(|e| op.fail(e))? {
            op.events(1)?;
            op.scan(size_of::<SourceSpan>() as u64)?;
            if span.is_empty() {
                add_count(&mut counts.synthetic, op)?;
            } else {
                add_count(&mut counts.source, op)?;
            }
        }
    }
    Ok(())
}
fn count_index<'id>(
    ir: &CompactIr,
    op: &mut OwnedWork<'id, '_>,
) -> Result<FixedIndexCounts, CoreError> {
    // Checkpoints are generated once from text, after these six fixed tables
    // have been counted. This total is only the known lower bound on rows.
    let mut counts = FixedIndexCounts {
        targets: ir
            .flows()
            .len()
            .checked_add(ir.payloads().len())
            .ok_or_else(|| op.fail(CoreError::ArithmeticOverflow))?,
        sparse: ir.atoms_len(),
        ..FixedIndexCounts::default()
    };
    for row in ir.flows() {
        op.events(1)?;
        count_target(ir, row.atoms, &mut counts, op)?;
    }
    for row in ir.payloads() {
        op.events(1)?;
        count_target(ir, row.atoms, &mut counts, op)?;
    }
    Ok(counts)
}

/// Built only after graph, provenance and target-partition validation.
pub(crate) struct MapIndex {
    pub(super) identity: ModelIdentity,
    pub(super) budget: SourceBudget,
    flow_count: usize,
    targets: FrozenRows<TargetIndex>,
    pub(super) source: FrozenRows<Posting>,
    pub(super) synthetic: FrozenRows<Posting>,
    pub(super) target: FrozenRows<TargetPosting>,
    pub(super) hidden: FrozenRows<TargetPosting>,
    checkpoints: FrozenRows<Checkpoint>,
    sparse: FrozenRows<Rows<Checkpoint>>,
    #[cfg(test)]
    scalar_steps: u64,
}

struct IndexDraft<'id> {
    pub(super) identity: ModelIdentity,
    flow_count: usize,
    targets: BuildExactRows<'id, TargetIndex>,
    pub(super) source: BuildExactRows<'id, Posting>,
    pub(super) synthetic: BuildExactRows<'id, Posting>,
    pub(super) target: BuildExactRows<'id, TargetPosting>,
    pub(super) hidden: BuildExactRows<'id, TargetPosting>,
    checkpoints: BuildGrowingVec<'id, Checkpoint>,
    sparse: BuildExactRows<'id, Rows<Checkpoint>>,
    #[cfg(test)]
    scalar_steps: u64,
}

impl MapIndex {
    #[cfg(test)]
    pub(super) fn checkpoint_count(&self) -> usize {
        self.checkpoints.len()
    }
    #[cfg(test)]
    pub(crate) fn storage_for_test(&self) -> (usize, usize, usize, usize) {
        (
            self.sparse.as_slice().as_ptr() as usize,
            self.sparse.len(),
            self.checkpoints.as_slice().as_ptr() as usize,
            self.checkpoints.len(),
        )
    }
    #[cfg(test)]
    pub(crate) fn scalar_steps_for_test(&self) -> u64 {
        self.scalar_steps
    }
    pub(crate) fn build_owned<'id>(
        ir: &CompactIr,
        source: &SourceSnapshot,
        identity: ModelIdentity,
        op: &mut OwnedWork<'id, '_>,
    ) -> Result<Self, CoreError> {
        IndexDraft::build(ir, source, identity, op)
    }
    pub(crate) fn build_verified(
        facts: VerifiedFacts<'_, '_, '_, '_, '_>,
    ) -> Result<(Self, FlowId), CoreError> {
        IndexDraft::build_verified(facts)
    }
    pub(super) fn get(
        &self,
        target: TextTarget,
        op: &mut Operation,
    ) -> Result<TargetIndex, CoreError> {
        op.events(1)?;
        let index = match target {
            TextTarget::Flow(id) if id.index() < self.flow_count => id.index(),
            TextTarget::Flow(_) => return Err(op.fail(CoreError::UnknownId)),
            TextTarget::Payload(id) => {
                let count = self
                    .targets
                    .len()
                    .checked_sub(self.flow_count)
                    .ok_or_else(|| op.fail(CoreError::InternalInvariant))?;
                if id.index() >= count {
                    return Err(op.fail(CoreError::UnknownId));
                }
                self.flow_count
                    .checked_add(id.index())
                    .ok_or_else(|| op.fail(CoreError::ArithmeticOverflow))?
            }
        };
        self.targets
            .as_slice()
            .get(index)
            .copied()
            .ok_or_else(|| op.fail(CoreError::UnknownId))
    }

    /// Seek only positive coverage. Hidden source anchors may occur after an
    /// atomic object while still pointing at its start; their ends are not a
    /// sorted search key in the complete atom stream.
    pub(crate) fn flow_atom_covering(
        &self,
        flow: FlowId,
        position: u64,
        op: &mut Operation,
    ) -> Result<AtomId, CoreError> {
        op.ensure_budget(&self.budget)?;
        let target = self.get(TextTarget::Flow(flow), op)?;
        if position >= target.length {
            return Err(op.fail(CoreError::InvalidRange));
        }
        let positive = target
            .target
            .slice(self.target.as_slice())
            .map_err(|error| op.fail(error))?;
        let at = lower_bound(positive, |row| row.end > position, op)?;
        let row = positive
            .get(at)
            .ok_or_else(|| op.fail(CoreError::InvalidProvenance))?;
        if row.start > position || row.end <= position {
            return Err(op.fail(CoreError::InvalidProvenance));
        }
        Ok(row.atom)
    }

    pub(super) fn byte_to_utf16(
        &self,
        ir: &CompactIr,
        source: &SourceSnapshot,
        id: AtomId,
        byte: u64,
        op: &mut Operation,
    ) -> Result<u64, CoreError> {
        self.convert(ir, source, id, byte, true, op)
    }

    pub(super) fn utf16_to_byte(
        &self,
        ir: &CompactIr,
        source: &SourceSnapshot,
        id: AtomId,
        utf16: u64,
        op: &mut Operation,
    ) -> Result<u64, CoreError> {
        self.convert(ir, source, id, utf16, false, op)
    }

    fn convert(
        &self,
        ir: &CompactIr,
        source: &SourceSnapshot,
        id: AtomId,
        position: u64,
        from_byte: bool,
        op: &mut Operation,
    ) -> Result<u64, CoreError> {
        let atom = ir.read_atom_at(id.index(), op)?;
        let text = atom_text(ir, source, &atom, op)?;
        let end_byte = text.len() as u64;
        let end_utf16 = atom
            .target
            .end
            .checked_sub(atom.target.start)
            .ok_or_else(|| op.fail(CoreError::InvalidProvenance))?;
        let maximum = if from_byte { end_byte } else { end_utf16 };
        if position > maximum {
            return Err(op.fail(CoreError::InvalidBoundary));
        }
        if position == 0 {
            return Ok(0);
        }
        if position == maximum {
            return Ok(if from_byte { end_utf16 } else { end_byte });
        }
        let rows = self
            .sparse
            .as_slice()
            .get(id.index())
            .ok_or_else(|| op.fail(CoreError::InvalidProvenance))?
            .slice(self.checkpoints.as_slice())
            .map_err(|e| op.fail(e))?;
        let bound = lower_bound(
            rows,
            |row| {
                if from_byte {
                    row.byte > position
                } else {
                    row.utf16 > position
                }
            },
            op,
        )?;
        let checkpoint = if bound == 0 {
            Checkpoint { byte: 0, utf16: 0 }
        } else {
            *rows
                .get(bound - 1)
                .ok_or_else(|| op.fail(CoreError::InvalidProvenance))?
        };
        let (mut byte, mut utf16) = (checkpoint.byte, checkpoint.utf16);
        if (if from_byte { byte } else { utf16 }) == position {
            return Ok(if from_byte { utf16 } else { byte });
        }
        let start = usize::try_from(byte).map_err(|_| op.fail(CoreError::ArithmeticOverflow))?;
        let end = rows
            .get(bound)
            .map(|row| row.byte)
            .unwrap_or(text.len() as u64);
        let end = usize::try_from(end).map_err(|_| op.fail(CoreError::ArithmeticOverflow))?;
        if end < start || end - start > SPARSE_BYTES as usize + 3 {
            return Err(op.fail(CoreError::InvalidProvenance));
        }
        charge_boundaries(text, start, end, op)?;
        let window = text
            .get(start..end)
            .ok_or_else(|| op.fail(CoreError::InvalidProvenance))?;
        let mut scalars = window.chars();
        loop {
            match op.visit_scalar_batch(
                &mut scalars,
                ScalarChargeOrder::EventThenScan,
                |scalar| {
                    byte = byte
                        .checked_add(scalar.len_utf8() as u64)
                        .ok_or(CoreError::ArithmeticOverflow)?;
                    utf16 = utf16
                        .checked_add(scalar.len_utf16() as u64)
                        .ok_or(CoreError::ArithmeticOverflow)?;
                    let actual = if from_byte { byte } else { utf16 };
                    if actual == position {
                        return Ok(ControlFlow::Break(()));
                    }
                    if actual > position || byte - checkpoint.byte > SPARSE_BYTES + 4 {
                        return Err(CoreError::InvalidBoundary);
                    }
                    Ok(ControlFlow::Continue(()))
                },
            )? {
                ScalarBatchEnd::Full => {}
                ScalarBatchEnd::Break => return Ok(if from_byte { utf16 } else { byte }),
                ScalarBatchEnd::Exhausted => return Err(op.fail(CoreError::InvalidBoundary)),
            }
        }
    }
}

impl<'id> IndexDraft<'id> {
    pub(crate) fn build(
        ir: &CompactIr,
        source: &SourceSnapshot,
        identity: ModelIdentity,
        op: &mut OwnedWork<'id, '_>,
    ) -> Result<MapIndex, CoreError> {
        op.check()?;
        if op.uses_verified_index() {
            return Err(op.fail(CoreError::UnsupportedConfiguration));
        }
        op.ensure_budget(source.budget())?;
        check_source_owned(identity, source, op)?;
        let max = op
            .model_limits()
            .ok_or_else(|| op.fail(CoreError::UnsupportedConfiguration))?
            .index_records;
        let counts = count_index(ir, op)?;
        let total = counts.total(op)?;
        op.check_limit(LimitKind::IndexRecords, total, max)?;
        let mut index = Self {
            identity,
            flow_count: ir.flows().len(),
            targets: BuildExactRows::new(counts.targets, op)?,
            source: BuildExactRows::new(counts.source, op)?,
            synthetic: BuildExactRows::new(counts.synthetic, op)?,
            target: BuildExactRows::new(counts.target, op)?,
            hidden: BuildExactRows::new(counts.hidden, op)?,
            checkpoints: BuildGrowingVec::new(max, LimitKind::IndexRecords, op)?,
            sparse: BuildExactRows::new(counts.sparse, op)?,
            #[cfg(test)]
            scalar_steps: 0,
        };
        for _ in 0..ir.atoms_len() {
            push(
                &mut index.sparse,
                Rows::new(0, 0).map_err(|e| op.fail(e))?,
                op,
            )?;
        }
        index.populate(ir, source, true, op)?;
        index.freeze(op)
    }

    pub(crate) fn build_verified(
        facts: VerifiedFacts<'_, '_, '_, 'id, '_>,
    ) -> Result<(MapIndex, FlowId), CoreError> {
        let VerifiedParts {
            ir,
            source,
            identity,
            op,
            main,
            counts,
            sparse,
            checkpoints,
        } = facts.into_parts();
        // This is the Index phase boundary on the original, exclusively borrowed op.
        op.checkpoint_now()?;
        if !op.uses_verified_index() {
            return Err(op.fail(CoreError::UnsupportedConfiguration));
        }
        op.ensure_budget(source.budget())?;
        op.ensure_budget(&ir.budget)?;
        check_source_owned(identity, source, op)?;
        let max = op
            .model_limits()
            .ok_or_else(|| op.fail(CoreError::UnsupportedConfiguration))?
            .index_records;
        let total = counts.total_with_checkpoints(checkpoints.len(), op)?;
        op.check_limit(LimitKind::IndexRecords, total, max)?;
        let mut index = Self {
            identity,
            flow_count: ir.flows().len(),
            targets: BuildExactRows::new(counts.targets, op)?,
            source: BuildExactRows::new(counts.source, op)?,
            synthetic: BuildExactRows::new(counts.synthetic, op)?,
            target: BuildExactRows::new(counts.target, op)?,
            hidden: BuildExactRows::new(counts.hidden, op)?,
            // Move the once-admitted final allocations; no clone or repeated row admission.
            checkpoints,
            sparse,
            #[cfg(test)]
            scalar_steps: 0,
        };
        if op.uses_task_access() {
            index = task::populate(index, ir, source, op)?;
        } else {
            index.populate(ir, source, false, op)?;
        }
        Ok((index.freeze(op)?, main))
    }

    fn populate(
        &mut self,
        ir: &CompactIr,
        source: &SourceSnapshot,
        generate_checkpoints: bool,
        op: &mut OwnedWork<'id, '_>,
    ) -> Result<(), CoreError> {
        for (flow, row) in ir.flows().iter().enumerate() {
            op.events(1)?;
            let target =
                TextTarget::Flow(crate::ir::FlowId::from_index(flow).map_err(|e| op.fail(e))?);
            self.build_target(
                ir,
                source,
                TargetInput {
                    target,
                    domains: row.domains,
                    atoms: row.atoms,
                    length: row.logical_utf16,
                },
                generate_checkpoints,
                op,
            )?;
        }
        for (payload, row) in ir.payloads().iter().enumerate() {
            op.events(1)?;
            let target = TextTarget::Payload(
                crate::ir::PayloadId::from_index(payload).map_err(|e| op.fail(e))?,
            );
            self.build_target(
                ir,
                source,
                TargetInput {
                    target,
                    domains: row.domains,
                    atoms: row.atoms,
                    length: row.utf8_len,
                },
                generate_checkpoints,
                op,
            )?;
        }
        self.targets.complete(op)?;
        self.source.complete(op)?;
        self.synthetic.complete(op)?;
        self.target.complete(op)?;
        self.hidden.complete(op)?;
        self.sparse.complete(op)?;
        op.check()?;
        Ok(())
    }

    fn build_target(
        &mut self,
        ir: &CompactIr,
        source: &SourceSnapshot,
        input: TargetInput,
        generate_checkpoints: bool,
        op: &mut OwnedWork<'id, '_>,
    ) -> Result<(), CoreError> {
        let TargetInput {
            target,
            domains,
            atoms,
            length,
        } = input;
        let (s, y, t, h) = (
            self.source.len(),
            self.synthetic.len(),
            self.target.len(),
            self.hidden.len(),
        );
        let actual_domains = domains.slice(ir.sources()).map_err(|e| op.fail(e))?;
        let empty_anchor = if atoms.is_empty() {
            if length != 0 || actual_domains.len() != 1 || !actual_domains[0].is_empty() {
                return Err(op.fail(CoreError::InvalidProvenance));
            }
            Some(actual_domains[0].start())
        } else {
            None
        };
        if op.uses_task_access() && !generate_checkpoints {
            task::postings(self, ir, atoms, length, op)?;
        } else {
            let mut target_end = 0;
            for (offset, index) in atoms
                .indices(ir.atoms_len())
                .map_err(|e| op.fail(e))?
                .enumerate()
            {
                let atom = ir.owned_read_atom_at(index, op)?;
                op.events(1)?;
                let id = AtomId::from_index(
                    atoms
                        .start()
                        .checked_add(offset)
                        .ok_or_else(|| op.fail(CoreError::ArithmeticOverflow))?,
                )
                .map_err(|e| op.fail(e))?;
                if atom.target.end > atom.target.start {
                    if atom.target.start != target_end {
                        return Err(op.fail(CoreError::InvalidProvenance));
                    }
                    target_end = atom.target.end;
                    push(
                        &mut self.target,
                        TargetPosting {
                            start: atom.target.start,
                            end: atom.target.end,
                            atom: id,
                        },
                        op,
                    )?;
                } else {
                    if atom.kind != AtomKind::Hidden {
                        return Err(op.fail(CoreError::InvalidProvenance));
                    }
                    push(
                        &mut self.hidden,
                        TargetPosting {
                            start: atom.target.start,
                            end: atom.target.end,
                            atom: id,
                        },
                        op,
                    )?;
                }
                for (part, span) in atom
                    .sources
                    .slice(ir.sources())
                    .map_err(|e| op.fail(e))?
                    .iter()
                    .copied()
                    .enumerate()
                {
                    op.events(1)?;
                    let posting = Posting {
                        span,
                        atom: id,
                        part: u32::try_from(part)
                            .map_err(|_| op.fail(CoreError::ArithmeticOverflow))?,
                    };
                    if span.is_empty() {
                        if atom.kind != AtomKind::Synthetic {
                            return Err(op.fail(CoreError::InvalidProvenance));
                        }
                        push(&mut self.synthetic, posting, op)?;
                    } else {
                        push(&mut self.source, posting, op)?;
                    }
                }
                if generate_checkpoints
                    && matches!(target, TextTarget::Flow(_))
                    && matches!(
                        atom.kind,
                        AtomKind::Copy | AtomKind::Transform | AtomKind::Synthetic
                    )
                {
                    let text = atom_text_owned(ir, source, &atom, op)?;
                    let begin = self.checkpoints.len();
                    let (mut byte, mut utf16, mut previous) = (0u64, 0u64, 0u64);
                    let mut scalars = text.chars();
                    #[cfg(test)]
                    let mut scalar_steps = 0_u64;
                    loop {
                        match op.visit_scalar_batch(
                            &mut scalars,
                            ScalarChargeOrder::EventThenScan,
                            |scalar| {
                                #[cfg(test)]
                                {
                                    scalar_steps += 1;
                                }
                                byte = byte
                                    .checked_add(scalar.len_utf8() as u64)
                                    .ok_or(CoreError::ArithmeticOverflow)?;
                                utf16 = utf16
                                    .checked_add(scalar.len_utf16() as u64)
                                    .ok_or(CoreError::ArithmeticOverflow)?;
                                if byte < text.len() as u64 && byte - previous >= SPARSE_BYTES {
                                    Ok(ControlFlow::Break(()))
                                } else {
                                    Ok(ControlFlow::Continue(()))
                                }
                            },
                        )? {
                            ScalarBatchEnd::Full => {}
                            ScalarBatchEnd::Exhausted => break,
                            ScalarBatchEnd::Break => {
                                // Flush only after the scalar batch's completion check.
                                // All seven tables share the Operation's entity count.
                                // GrowingVec admits capacity, not new index entities.
                                op.admit_model(LimitKind::IndexRecords, 1)?;
                                self.checkpoints.push(Checkpoint { byte, utf16 }, op)?;
                                previous = byte;
                            }
                        }
                    }
                    #[cfg(test)]
                    {
                        self.scalar_steps += scalar_steps;
                    }
                    let expected = match target {
                        TextTarget::Flow(_) => utf16,
                        TextTarget::Payload(_) => byte,
                    };
                    if expected != atom.target.end - atom.target.start {
                        return Err(op.fail(CoreError::InvalidProvenance));
                    }
                    let rows =
                        Rows::new(begin, self.checkpoints.len() - begin).map_err(|e| op.fail(e))?;
                    self.sparse.replace(id.index(), rows, op)?;
                }
            }
            if target_end != length {
                return Err(op.fail(CoreError::InvalidProvenance));
            }
        }
        sort(
            &mut self.source,
            s,
            |a, b| {
                (a.span.start(), a.span.end(), a.atom, a.part).cmp(&(
                    b.span.start(),
                    b.span.end(),
                    b.atom,
                    b.part,
                ))
            },
            op,
        )?;
        sort(
            &mut self.synthetic,
            y,
            |a, b| (a.span.start(), a.atom, a.part).cmp(&(b.span.start(), b.atom, b.part)),
            op,
        )?;
        sort(
            &mut self.hidden,
            h,
            |a, b| (a.start, a.atom).cmp(&(b.start, b.atom)),
            op,
        )?;
        // Source postings are a real partition; do not create authority from an envelope.
        if op.uses_task_access() {
            task::check_source_partition(&self.source, s, op)?;
        } else {
            let mut prior_end = None;
            for index in s..self.source.len() {
                let row = self
                    .source
                    .get(index, op)?
                    .ok_or_else(|| op.fail(CoreError::InternalInvariant))?;
                op.events(1)?;
                if prior_end.is_some_and(|end| row.span.start() < end) {
                    return Err(op.fail(CoreError::InvalidProvenance));
                }
                prior_end = Some(row.span.end());
            }
        }
        let row = TargetIndex {
            domains,
            length,
            empty_anchor,
            source: Rows::new(s, self.source.len() - s).map_err(|e| op.fail(e))?,
            synthetic: Rows::new(y, self.synthetic.len() - y).map_err(|e| op.fail(e))?,
            target: Rows::new(t, self.target.len() - t).map_err(|e| op.fail(e))?,
            hidden: Rows::new(h, self.hidden.len() - h).map_err(|e| op.fail(e))?,
        };
        push(&mut self.targets, row, op)
    }

    fn freeze(self, op: &mut OwnedWork<'id, '_>) -> Result<MapIndex, CoreError> {
        Ok(MapIndex {
            identity: self.identity,
            budget: op.frozen_budget(),
            flow_count: self.flow_count,
            targets: self.targets.freeze(op)?,
            source: self.source.freeze(op)?,
            synthetic: self.synthetic.freeze(op)?,
            target: self.target.freeze(op)?,
            hidden: self.hidden.freeze(op)?,
            checkpoints: self.checkpoints.freeze(op)?,
            sparse: self.sparse.freeze(op)?,
            #[cfg(test)]
            scalar_steps: self.scalar_steps,
        })
    }
}

pub(super) fn atom_text<'a>(
    ir: &'a CompactIr,
    source: &'a SourceSnapshot,
    atom: &AtomRow,
    op: &mut Operation,
) -> Result<&'a str, CoreError> {
    match atom.content {
        AtomContent::Pool(value) => ir.text_query(value, op),
        AtomContent::Empty if atom.kind == AtomKind::Copy => {
            let parts = atom.sources.slice(ir.sources()).map_err(|e| op.fail(e))?;
            if parts.len() != 1 {
                return Err(op.fail(CoreError::InvalidProvenance));
            }
            let (start, end) = (
                usize::try_from(parts[0].start())
                    .map_err(|_| op.fail(CoreError::ArithmeticOverflow))?,
                usize::try_from(parts[0].end())
                    .map_err(|_| op.fail(CoreError::ArithmeticOverflow))?,
            );
            charge_boundaries(source.text(), start, end, op)?;
            source
                .text()
                .get(start..end)
                .ok_or_else(|| op.fail(CoreError::InvalidProvenance))
        }
        _ => Err(op.fail(CoreError::InvalidProvenance)),
    }
}

fn charge_boundaries(
    text: &str,
    start: usize,
    end: usize,
    op: &mut Operation,
) -> Result<(), CoreError> {
    // str::get checks at most one byte per interior endpoint; no UTF8 revalidation.
    op.scan(u64::from(start > 0 && start < text.len()) + u64::from(end > 0 && end < text.len()))
}

pub(super) fn check_source(
    identity: ModelIdentity,
    source: &SourceSnapshot,
    op: &mut Operation,
) -> Result<(), CoreError> {
    let expected = identity.source();
    let actual = source.key();
    if expected.document() != actual.document() {
        return Err(op.fail(CoreError::WrongDocument));
    }
    if expected.revision() != actual.revision() {
        return Err(op.fail(CoreError::WrongRevision {
            expected: expected.revision(),
            actual: actual.revision(),
        }));
    }
    Ok(())
}

pub(super) fn atom_text_owned<'a>(
    ir: &'a CompactIr,
    source: &'a SourceSnapshot,
    atom: &AtomRow,
    op: &mut OwnedWork<'_, '_>,
) -> Result<&'a str, CoreError> {
    match atom.content {
        AtomContent::Pool(value) => ir.text_owned(value, op),
        AtomContent::Empty if atom.kind == AtomKind::Copy => {
            let parts = atom.sources.slice(ir.sources()).map_err(|e| op.fail(e))?;
            if parts.len() != 1 {
                return Err(op.fail(CoreError::InvalidProvenance));
            }
            let (start, end) = (
                usize::try_from(parts[0].start())
                    .map_err(|_| op.fail(CoreError::ArithmeticOverflow))?,
                usize::try_from(parts[0].end())
                    .map_err(|_| op.fail(CoreError::ArithmeticOverflow))?,
            );
            charge_boundaries_owned(source.text(), start, end, op)?;
            source
                .text()
                .get(start..end)
                .ok_or_else(|| op.fail(CoreError::InvalidProvenance))
        }
        _ => Err(op.fail(CoreError::InvalidProvenance)),
    }
}

fn charge_boundaries_owned(
    text: &str,
    start: usize,
    end: usize,
    op: &mut OwnedWork<'_, '_>,
) -> Result<(), CoreError> {
    // str::get checks at most one byte per interior endpoint; no UTF8 revalidation.
    op.scan(u64::from(start > 0 && start < text.len()) + u64::from(end > 0 && end < text.len()))
}

pub(super) fn check_source_owned(
    identity: ModelIdentity,
    source: &SourceSnapshot,
    op: &mut OwnedWork<'_, '_>,
) -> Result<(), CoreError> {
    let expected = identity.source();
    let actual = source.key();
    if expected.document() != actual.document() {
        return Err(op.fail(CoreError::WrongDocument));
    }
    if expected.revision() != actual.revision() {
        return Err(op.fail(CoreError::WrongRevision {
            expected: expected.revision(),
            actual: actual.revision(),
        }));
    }
    Ok(())
}

pub(super) fn lower_bound<T: Copy>(
    rows: &[T],
    predicate: impl Fn(&T) -> bool,
    op: &mut Operation,
) -> Result<usize, CoreError> {
    lower_bound_view(rows.into(), predicate, op)
}

pub(super) fn lower_bound_view<T: Copy>(
    rows: crate::ir::RowView<'_, T>,
    predicate: impl Fn(&T) -> bool,
    op: &mut Operation,
) -> Result<usize, CoreError> {
    let (mut low, mut high) = (0, rows.len());
    while low < high {
        op.events(1)?;
        let middle = low + (high - low) / 2;
        if predicate(&rows[middle]) {
            high = middle;
        } else {
            low = middle + 1;
        }
    }
    Ok(low)
}

fn push<'id, T: Copy>(
    rows: &mut BuildExactRows<'id, T>,
    value: T,
    op: &mut OwnedWork<'id, '_>,
) -> Result<(), CoreError> {
    op.admit_model(LimitKind::IndexRecords, 1)?;
    rows.push(value, op)
}

fn sort<'id, T: Copy>(
    rows: &mut impl BuildSortRows<'id, T>,
    start: usize,
    compare: impl Fn(T, T) -> std::cmp::Ordering,
    op: &mut OwnedWork<'id, '_>,
) -> Result<(), CoreError> {
    crate::memory::sort_build_rows_by(rows, start, |a, b, _| Ok(compare(a, b)), op)
}
#[cfg(test)]
impl MapIndex {
    pub(crate) fn build(
        ir: &CompactIr,
        source: &SourceSnapshot,
        identity: ModelIdentity,
        op: &mut Operation,
    ) -> Result<Self, CoreError> {
        crate::policy::construction_scope(op, |mut work| {
            let index = Self::build_owned(ir, source, identity, &mut work)?;
            work.finish()?;
            Ok(index)
        })
    }
}

#[cfg(test)]
mod sort_tests {
    use super::*;
    use crate::config::{ParseConfig, Profile};
    use crate::identity::{SourceBoundary, SourceKey};
    use crate::mapping::{Affinity, Correspondence, PointInput, PointQuery};
    use crate::model::AcceptedModel;
    use crate::policy::{CancellationToken, ModelLimits, SourceLimits, construction_scope};
    use crate::source::SourceDocument;

    #[derive(Clone, Copy, Debug, Eq, PartialEq)]
    struct Row {
        key: u32,
        identity: u32,
    }

    #[test]
    fn bounded_merge_preserves_prefix_and_stable_arbitrary_keys() {
        for count in [0, 1, 2, 3, 5, 17, 257] {
            for pattern in 0..4 {
                let mut expected = vec![
                    Row {
                        key: 99,
                        identity: 0,
                    },
                    Row {
                        key: 98,
                        identity: 1,
                    },
                ];
                expected.extend((0..count).map(|i| Row {
                    key: match pattern {
                        0 => i,
                        1 => count - i,
                        2 => (i * 13) % 7,
                        _ => u32::from(i + 2 < count) * i,
                    },
                    identity: i + 2,
                }));
                let budget = SourceBudget::model_fixture();
                let mut op = budget.begin(CancellationToken::new());
                construction_scope(&mut op, |mut op| {
                    let mut rows =
                        BuildGrowingVec::new(259, LimitKind::IndexRecords, &mut op).unwrap();
                    rows.extend_from_slice(&expected, &mut op).unwrap();
                    let baseline = budget.usage().auxiliary_capacity;
                    expected[2..].sort_by_key(|row| row.key);
                    sort(&mut rows, 2, |a, b| a.key.cmp(&b.key), &mut op).unwrap();
                    assert_eq!(snapshot(&rows), expected);
                    assert_eq!(budget.usage().auxiliary_capacity, baseline);
                    drop(rows);
                    assert_eq!(budget.usage().auxiliary_capacity, 0);
                    let fault = op.first_fault();
                    assert_eq!(op.finish().err(), fault);
                });
            }
        }
    }

    #[test]
    fn bounded_merge_faults_release_scratch_and_keep_first_fault() {
        for count in [0, 1] {
            let budget = SourceBudget::model_fixture();
            let cancel = CancellationToken::new();
            let mut op = budget.begin(cancel.clone());
            construction_scope(&mut op, |mut op| {
                let mut rows = BuildGrowingVec::new(1, LimitKind::IndexRecords, &mut op).unwrap();
                if count == 1 {
                    rows.push(
                        Row {
                            key: 0,
                            identity: 0,
                        },
                        &mut op,
                    )
                    .unwrap();
                }
                let first = op.fail(CoreError::ArithmeticOverflow);
                cancel.cancel();
                assert_eq!(
                    sort(&mut rows, 0, |a, b| a.key.cmp(&b.key), &mut op),
                    Err(first)
                );
                assert_eq!(op.first_fault(), Some(first));
                drop(rows);
                assert_eq!(budget.usage().auxiliary_capacity, 0);
                let fault = op.first_fault();
                assert_eq!(op.finish().err(), fault);
            });
        }
        let original: Vec<_> = (0..17)
            .rev()
            .map(|i| Row {
                key: i,
                identity: i,
            })
            .collect();
        let bytes = std::mem::size_of_val(original.as_slice()) as u64;
        for (capacity, work, kind) in [
            (bytes, 268_435_456, LimitKind::AuxiliaryCapacity),
            (bytes * 3, bytes * 2 + 20, LimitKind::Work),
        ] {
            let budget = SourceBudget::new_with_model_limits(
                SourceLimits {
                    auxiliary_capacity: capacity,
                    max_work: work,
                    ..SourceLimits::default()
                },
                ModelLimits::default(),
            )
            .unwrap();
            let mut op = budget.begin(CancellationToken::new());
            construction_scope(&mut op, |mut op| {
                let mut rows = BuildGrowingVec::new(17, LimitKind::IndexRecords, &mut op).unwrap();
                rows.extend_from_slice(&original, &mut op).unwrap();
                let baseline = budget.usage().auxiliary_capacity;
                let error = sort(&mut rows, 0, |a, b| a.key.cmp(&b.key), &mut op).unwrap_err();
                assert!(
                    matches!(error, CoreError::ResourceLimit { kind: actual, .. } if actual == kind)
                );
                assert_eq!(op.first_fault(), Some(error));
                assert_eq!(budget.usage().auxiliary_capacity, baseline);
                if kind == LimitKind::AuxiliaryCapacity {
                    assert_eq!(snapshot(&rows), original);
                }
                drop(rows);
                assert_eq!(budget.usage().auxiliary_capacity, 0);
                let fault = op.first_fault();
                assert_eq!(op.finish().err(), fault);
            });
        }
        let budget = SourceBudget::model_fixture();
        let cancel = CancellationToken::new();
        let mut op = budget.begin(cancel.clone());
        construction_scope(&mut op, |mut op| {
            let mut rows = BuildGrowingVec::new(17, LimitKind::IndexRecords, &mut op).unwrap();
            rows.extend_from_slice(&original, &mut op).unwrap();
            let baseline = budget.usage().auxiliary_capacity;
            cancel.cancel();
            assert_eq!(
                sort(&mut rows, 0, |a, b| a.key.cmp(&b.key), &mut op),
                Err(CoreError::Cancelled)
            );
            assert_eq!(op.first_fault(), Some(CoreError::Cancelled));
            assert_eq!(budget.usage().auxiliary_capacity, baseline);
            assert_eq!(snapshot(&rows), original);
            drop(rows);
            assert_eq!(budget.usage().auxiliary_capacity, 0);
            let fault = op.first_fault();
            assert_eq!(op.finish().err(), fault);
        });

        // A flag set by the final ordered comparison is still observed before
        // the no-copy fast path returns.
        let budget = SourceBudget::model_fixture();
        let cancel = CancellationToken::new();
        let mut op = budget.begin(cancel.clone());
        construction_scope(&mut op, |mut op| {
            let mut rows = BuildGrowingVec::new(2, LimitKind::IndexRecords, &mut op).unwrap();
            rows.extend_from_slice(
                &[
                    Row {
                        key: 0,
                        identity: 0,
                    },
                    Row {
                        key: 1,
                        identity: 1,
                    },
                ],
                &mut op,
            )
            .unwrap();
            assert_eq!(
                sort(
                    &mut rows,
                    0,
                    |a, b| {
                        cancel.cancel();
                        a.key.cmp(&b.key)
                    },
                    &mut op
                ),
                Err(CoreError::Cancelled)
            );
            assert_eq!(op.first_fault(), Some(CoreError::Cancelled));
            drop(rows);
            assert_eq!(budget.usage().auxiliary_capacity, 0);
            let fault = op.first_fault();
            assert_eq!(op.finish().err(), fault);
        });

        // Signal deterministically from an actual merge comparison; this checks
        // the same cancellation flag and unwind path without timing assertions.
        let original: Vec<_> = (0..257)
            .rev()
            .map(|i| Row {
                key: i,
                identity: i,
            })
            .collect();
        let budget = SourceBudget::model_fixture();
        let cancel = CancellationToken::new();
        let mut op = budget.begin(cancel.clone());
        construction_scope(&mut op, |mut op| {
            let mut rows = BuildGrowingVec::new(257, LimitKind::IndexRecords, &mut op).unwrap();
            rows.extend_from_slice(&original, &mut op).unwrap();
            let baseline = budget.usage().auxiliary_capacity;
            let comparisons = std::sync::atomic::AtomicUsize::new(0);
            assert_eq!(
                sort(
                    &mut rows,
                    0,
                    |a, b| {
                        if comparisons.fetch_add(1, std::sync::atomic::Ordering::Relaxed) + 1 == 50
                        {
                            cancel.cancel();
                        }
                        a.key.cmp(&b.key)
                    },
                    &mut op
                ),
                Err(CoreError::Cancelled)
            );
            assert_eq!(comparisons.load(std::sync::atomic::Ordering::Relaxed), 50);
            assert_eq!(op.first_fault(), Some(CoreError::Cancelled));
            assert_eq!(budget.usage().auxiliary_capacity, baseline);
            drop(rows);
            assert_eq!(budget.usage().auxiliary_capacity, 0);
            let fault = op.first_fault();
            assert_eq!(op.finish().err(), fault);
        });
    }

    fn snapshot<T: Copy>(rows: &BuildGrowingVec<'_, T>) -> Vec<T> {
        (0..rows.len())
            .map(|index| rows.raw_value_for_test(index).unwrap())
            .collect()
    }

    #[test]
    fn real_64k_inline_code_accepts_and_maps_in_one_default_operation() {
        let text = format!("`a{}`", "\nb".repeat(32_768));
        let budget = SourceBudget::model_fixture();
        let mut op = budget.begin(CancellationToken::new());
        let source = SourceDocument::new(
            SourceKey::new([17; 16], 1).unwrap(),
            text.as_bytes(),
            &mut op,
        )
        .unwrap()
        .snapshot();
        let model =
            AcceptedModel::parse(source, ParseConfig::new(Profile::C0), 1, &mut op).unwrap();
        assert_eq!(model.ir().flows()[0].logical_utf16, 65_537);
        let accepted = op.stats().expect("statistics enabled");
        eprintln!(
            "65539B AcceptedModel same operation: scan={} copy={} events={} total={}",
            accepted.scanned_bytes(),
            accepted.copied_bytes(),
            accepted.events(),
            accepted.total()
        );
        let result = model
            .map_point(
                &PointQuery {
                    model: model.identity(),
                    input: PointInput::Source {
                        target: TextTarget::Flow(crate::ir::FlowId::from_index(0).unwrap()),
                        byte: SourceBoundary::new(0),
                    },
                    affinity: Affinity::Before,
                },
                &mut op,
            )
            .unwrap();
        assert_eq!(result.results()[0].chosen.value(), 0);
        assert_eq!(result.results()[0].correspondence, Correspondence::Exact);
        assert_eq!(op.first_fault(), None);
        drop(result);
        drop(model);
        assert_eq!(budget.usage().auxiliary_capacity, 0);
        assert_eq!(budget.usage().source_capacity, 0);
    }
}
