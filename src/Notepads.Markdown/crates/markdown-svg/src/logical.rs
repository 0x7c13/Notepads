// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE file in the project root for license information.
// ---------------------------------------------------------------------------------------------

//! Source-derived parent cells are separate from Native coverage and raw HB
//! ranges. Only the checked owner can be handed to layout/used-gid geometry.
use crate::{
    abi::{self, Failure, Result},
    draft::DraftOwner,
    memory::{Context, RetainedPool, failure_from_resource},
};
use markdown_svg_budget::{BudgetVec, ResourceKind, ResourcesRef};
use std::{ops::Range, sync::Arc};

pub struct OwnedLogicalPlan {
    cells: BudgetVec<usvg::ReplaySourceCell>,
    /// Trusted closure edges derived from original EGCs and actual HB ranges.
    /// The source index and raw glyphs remain immutable during validation.
    joined: BudgetVec<bool>,
    glyph_ranges: BudgetVec<Range<usize>>,
    source_spans: BudgetVec<abi::LogicalSpan>,
    font_parts: BudgetVec<abi::LogicalRun>,
    part_strings: BudgetVec<u8>,
    context: Context,
}
pub struct CheckedPlan {
    work: std::sync::Mutex<crate::memory::WorkCredit>,
    cells: BudgetVec<usvg::ReplaySourceCell>,
    _joined: BudgetVec<bool>,
    _glyph_ranges: BudgetVec<Range<usize>>,
    source_spans: BudgetVec<abi::LogicalSpan>,
    font_parts: BudgetVec<abi::LogicalRun>,
    _part_strings: BudgetVec<u8>,
    context: Context,
}
impl std::fmt::Debug for CheckedPlan {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("CheckedLogicalPlan")
            .field("cells", &self.cells.len())
            .finish_non_exhaustive()
    }
}
impl usvg::ReplayLogicalSource for CheckedPlan {
    fn cells(&self) -> &[usvg::ReplaySourceCell] {
        &self.cells
    }
    fn source_bidi_level(&self, offset: usize) -> Option<u8> {
        let index = self
            .source_spans
            .partition_point(|span| (span.utf8_start + span.utf8_length) as usize <= offset);
        let span = self.source_spans.get(index)?;
        (offset >= span.utf8_start as usize
            && offset < (span.utf8_start + span.utf8_length) as usize)
            .then_some(span.bidi_level as u8)
    }
    fn checkpoint(&self, work: u64) -> core::result::Result<(), ()> {
        self.work
            .lock()
            .map_err(|_| {
                self.context.record_failure(Failure::Contract);
            })?
            .charge(&self.context, work)
            .map_err(|error| {
                self.context.record_failure(error);
            })
    }
}
fn find_next(links: &mut [usize], mut index: usize) -> usize {
    while links[index] != index {
        let next = links[index];
        links[index] = links[next];
        index = next;
    }
    index
}
impl OwnedLogicalPlan {
    pub fn prepare_with_parts(
        context: &Context,
        resources: &ResourcesRef,
        draft: &DraftOwner,
        ranges: impl Clone + Iterator<Item = Range<usize>>,
        parts: Option<&crate::font::OwnedPlan>,
    ) -> Result<Self> {
        let boundaries = draft.source_index.boundaries();
        let count = boundaries
            .len()
            .checked_sub(1)
            .ok_or_else(|| context.record_failure(Failure::Contract))?;
        let mut work = crate::memory::WorkBatch::new(context);
        let mut glyph_count = 0usize;
        for _ in ranges.clone() {
            work.charge(1)?;
            glyph_count = glyph_count
                .checked_add(1)
                .ok_or_else(|| context.record_failure(Failure::Contract))?;
        }
        let part_count = parts.map_or(0, |plan| plan.runs.len());
        let string_count = parts.map_or(0, |plan| plan.strings.len());
        let mut font_parts = BudgetVec::new(resources.clone(), ResourceKind::Scratch);
        font_parts
            .try_reserve_exact(part_count)
            .map_err(|_| context.record_failure(Failure::Allocation))?;
        let mut part_strings = BudgetVec::new(resources.clone(), ResourceKind::Scratch);
        part_strings
            .try_reserve_exact(string_count)
            .map_err(|_| context.record_failure(Failure::Allocation))?;
        if let Some(parts) = parts {
            font_parts
                .try_extend_from_slice(parts.runs.as_slice())
                .map_err(failure_from_resource)?;
            part_strings
                .try_extend_from_slice(parts.strings.as_slice())
                .map_err(failure_from_resource)?;
        }
        let mut source_spans = BudgetVec::new(resources.clone(), ResourceKind::Scratch);
        source_spans
            .try_reserve_exact(draft.spans.len())
            .map_err(|_| context.record_failure(Failure::Allocation))?;
        source_spans
            .try_extend_from_slice(&draft.spans)
            .map_err(failure_from_resource)?;
        let mut joined = BudgetVec::new(resources.clone(), ResourceKind::Scratch);
        joined
            .try_reserve_exact(count + 1)
            .map_err(|_| context.record_failure(Failure::Allocation))?;
        for _ in 0..count + 1 {
            joined.try_push(false).map_err(failure_from_resource)?;
        }
        let mut links = BudgetVec::new(resources.clone(), ResourceKind::Scratch);
        links
            .try_reserve_exact(count + 1)
            .map_err(|_| context.record_failure(Failure::Allocation))?;
        for index in 0..=count {
            links.try_push(index).map_err(failure_from_resource)?;
        }
        let mut glyph_ranges = BudgetVec::new(resources.clone(), ResourceKind::Scratch);
        glyph_ranges
            .try_reserve_exact(glyph_count)
            .map_err(|_| context.record_failure(Failure::Allocation))?;
        for range in ranges {
            work.charge(1)?;
            if range.start >= range.end
                || range.end > draft.text.len()
                || !draft.text.is_char_boundary(range.start)
                || !draft.text.is_char_boundary(range.end)
            {
                return Err(context.record_failure(Failure::Contract));
            }
            let first = draft
                .source_index
                .parent(range.start)
                .map_err(|error| context.record_failure(error))?;
            let last = draft
                .source_index
                .parent(range.end - 1)
                .map_err(|error| context.record_failure(error))?;
            let mut boundary = find_next(&mut links, first + 1);
            while boundary <= last {
                work.charge(1)?;
                joined[boundary] = true;
                links[boundary] = find_next(&mut links, boundary + 1);
                boundary = links[boundary];
            }
            glyph_ranges
                .try_push(range)
                .map_err(failure_from_resource)?;
        }
        let mut cells = BudgetVec::new(resources.clone(), ResourceKind::Scratch);
        cells
            .try_reserve_exact(count + 1)
            .map_err(|_| context.record_failure(Failure::Allocation))?;
        let mut start = 0;
        for i in 1..=count {
            work.charge(1)?;
            if !joined[i] {
                cells
                    .try_push(usvg::ReplaySourceCell {
                        start: boundaries[start],
                        end: boundaries[i],
                    })
                    .map_err(failure_from_resource)?;
                start = i;
            }
        }
        drop(links); // Physical scratch retires before the shared admitted owner.
        Ok(Self {
            cells,
            joined,
            glyph_ranges,
            source_spans,
            font_parts,
            part_strings,
            context: context.clone(),
        })
    }
    pub fn check(
        self,
        draft: &DraftOwner,
        retained: &Arc<RetainedPool>,
    ) -> Result<Arc<CheckedPlan>> {
        let mut work = crate::memory::WorkBatch::new(&self.context);
        let boundaries = draft.source_index.boundaries();
        let mut actual = 0;
        let mut start = 0;
        for i in 1..boundaries.len() {
            work.charge(1)?;
            if !self.joined[i] {
                let expected = usvg::ReplaySourceCell {
                    start: boundaries[start],
                    end: boundaries[i],
                };
                if self.cells.get(actual) != Some(&expected) {
                    return Err(self.context.record_failure(Failure::Contract));
                }
                actual += 1;
                start = i;
            }
        }
        if actual != self.cells.len() {
            return Err(self.context.record_failure(Failure::Contract));
        }
        for part in self.font_parts.iter() {
            work.charge(1)?;
            let end = part
                .language_start
                .checked_add(part.language_length)
                .ok_or_else(|| self.context.record_failure(Failure::Contract))?
                as usize;
            if end > self.part_strings.len() {
                return Err(self.context.record_failure(Failure::Contract));
            }
        }
        for range in self.glyph_ranges.iter() {
            work.charge(1)?;
            let index = self.cells.partition_point(|cell| cell.end <= range.start);
            let cell = self
                .cells
                .get(index)
                .ok_or_else(|| self.context.record_failure(Failure::Contract))?;
            if range.start < cell.start || range.end > cell.end {
                return Err(self.context.record_failure(Failure::Contract));
            }
        }
        retained.share(CheckedPlan {
            work: std::sync::Mutex::new(crate::memory::WorkCredit::default()),
            cells: self.cells,
            _joined: self.joined,
            _glyph_ranges: self.glyph_ranges,
            source_spans: self.source_spans,
            font_parts: self.font_parts,
            _part_strings: self.part_strings,
            context: self.context,
        })
    }
}
impl CheckedPlan {
    pub fn font_parts(&self, start: usize, end: usize) -> Result<Range<usize>> {
        let first = self
            .font_parts
            .partition_point(|part| (part.utf8_start + part.utf8_length) as usize <= start);
        let last = self
            .font_parts
            .partition_point(|part| (part.utf8_start as usize) < end);
        if first >= last
            || first >= self.font_parts.len()
            || self.font_parts[first].utf8_start as usize > start
        {
            return Err(self.context.record_failure(Failure::Contract));
        }
        // Every original part/language was validated before any font use and
        // copied into this immutable owner. Re-scanning every overlapping part
        // once per glyph would become quadratic for repeated HB clusters.
        Ok(first..last)
    }
    pub fn parent(&self, start: usize, end: usize) -> Result<usize> {
        let index = self.cells.partition_point(|cell| cell.end <= start);
        let cell = self
            .cells
            .get(index)
            .ok_or_else(|| self.context.record_failure(Failure::Contract))?;
        if start >= end || start < cell.start || end > cell.end {
            return Err(self.context.record_failure(Failure::Contract));
        }
        Ok(index)
    }
}
