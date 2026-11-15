// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE file in the project root for license information.
// ---------------------------------------------------------------------------------------------

//! Internal, evolving block layout over the accepted semantic model.
//! This module does not measure fonts, own Windows objects or virtualize inside a block.

use crate::error::CoreError;
use crate::identity::LogicalSpan;
use crate::model::AcceptedModel;
use crate::policy::{CancellationToken, Operation};
use std::ops::Range;

mod disclosure;
mod quotes;
mod section;
mod storage;
mod tree;
mod types;
use disclosure::Visibility;
pub use disclosure::{Disclosure, DisclosureOverride};
use quotes::QuoteDirectory;
use storage::LayoutRows;
pub(crate) use tree::footnote_label_span;
pub use types::{
    BlockKind, BlockPlacement, BlockRequest, ContainerContext, LayoutKey, LayoutStyle,
    MeasuredBlock, MeasurementBatch, QuotePlacement, Viewport,
};

pub const MAX_MEASUREMENT_BATCH: usize = 256;

#[derive(Clone, Copy)]
struct Pending {
    start: usize,
    count: usize,
}

/// One layout task. Errors permanently stop it, including on a fresh Operation.
pub struct LayoutDraft {
    requests: LayoutRows<BlockRequest>,
    placements: LayoutRows<BlockPlacement>,
    quotes: QuoteDirectory,
    visibility: Option<Visibility>,
    scope: Option<LogicalSpan>,
    model: AcceptedModel,
    key: LayoutKey,
    cancel: CancellationToken,
    pending: Option<Pending>,
    fault: Option<CoreError>,
    total_height: f64,
}

impl LayoutDraft {
    pub fn new(
        model: AcceptedModel,
        key: LayoutKey,
        op: &mut Operation,
    ) -> Result<Self, CoreError> {
        Self::with_disclosures(model, key, &[], op)
    }

    pub fn with_disclosures(
        model: AcceptedModel,
        key: LayoutKey,
        overrides: &[DisclosureOverride],
        op: &mut Operation,
    ) -> Result<Self, CoreError> {
        Self::prepare(model, key, Some(overrides), None, op)
    }

    /// Visibility-independent requests for document-wide semantic and marker measurements.
    pub fn unfiltered_projection(
        model: AcceptedModel,
        key: LayoutKey,
        op: &mut Operation,
    ) -> Result<Self, CoreError> {
        Self::prepare(model, key, None, None, op)
    }

    /// Measure the accepted heading and its section with normal shared model ownership.
    pub fn section(
        model: AcceptedModel,
        key: LayoutKey,
        heading: u32,
        overrides: Option<&[DisclosureOverride]>,
        op: &mut Operation,
    ) -> Result<Self, CoreError> {
        let index = heading
            .checked_sub(1)
            .ok_or_else(|| op.fail(CoreError::UnknownId))?;
        let heading =
            crate::ir::NodeId::from_index(index as usize).map_err(|error| op.fail(error))?;
        Self::prepare(model, key, overrides, Some(heading), op)
    }

    fn prepare(
        model: AcceptedModel,
        key: LayoutKey,
        overrides: Option<&[DisclosureOverride]>,
        heading: Option<crate::ir::NodeId>,
        op: &mut Operation,
    ) -> Result<Self, CoreError> {
        op.ensure_budget(model.budget())?;
        op.checkpoint_now()?;
        if key.model() != model.identity() {
            return Err(op.fail(CoreError::StaleResult));
        }
        let section = heading
            .map(|node| section::Section::new(&model, node, op))
            .transpose()?;
        let visibility = match overrides {
            Some(overrides) if !overrides.is_empty() || !model.disclosure_regions().is_empty() => {
                Some(Visibility::build(&model, overrides, section.as_ref(), op)?)
            }
            _ => None,
        };
        let filter = tree::BlockFilter {
            visibility: visibility.as_ref(),
            section: section.as_ref(),
        };
        let counts = tree::counts(&model, key, filter, op)?;
        let mut requests = LayoutRows::new(counts.requests, op)?;
        let placements = LayoutRows::new(counts.requests, op)?;
        let mut quotes = QuoteDirectory::new(counts.quotes, op)?;
        tree::requests(&model, key, &counts, filter, &mut requests, &mut quotes, op)?;
        quotes.prepare_padding(counts.requests, key.style().uses_markdown_spacing(), op)?;
        let scope = if section.is_some() {
            let start = requests
                .values
                .first()
                .map_or(0, |block| block.logical.start());
            let end = requests
                .values
                .last()
                .map_or(start, |block| block.logical.end());
            Some(LogicalSpan::new(start, end).map_err(|error| op.fail(error))?)
        } else {
            None
        };
        op.checkpoint_now()?;
        Ok(Self {
            requests,
            placements,
            quotes,
            visibility,
            scope,
            model,
            key,
            cancel: op.cancellation(),
            pending: None,
            fault: None,
            total_height: 0.0,
        })
    }

    pub fn key(&self) -> LayoutKey {
        self.key
    }

    pub fn model(&self) -> &AcceptedModel {
        &self.model
    }

    pub fn requests(&self) -> &[BlockRequest] {
        &self.requests.values
    }

    pub fn scope(&self) -> Option<LogicalSpan> {
        self.scope
    }

    pub fn first_fault(&self) -> Option<CoreError> {
        self.fault
    }

    pub fn completed_blocks(&self) -> usize {
        self.placements.values.len()
    }

    pub fn blocks_len(&self) -> usize {
        self.requests.values.len()
    }

    /// An exact, append-only prefix. No measurement may be outstanding while
    /// presenting it, and the requested interval must end inside the prefix.
    pub fn covers_prefix(&self, bottom: f64, op: &mut Operation) -> Result<bool, CoreError> {
        self.check(op)?;
        let bottom = types::nonnegative(bottom).map_err(|error| op.fail(error))?;
        Ok(self.pending.is_none()
            && self.completed_blocks() < self.blocks_len()
            && bottom < self.total_height)
    }

    pub fn prefix_requests(&self) -> &[BlockRequest] {
        &self.requests.values[..self.completed_blocks()]
    }

    pub fn prefix_placements(&self) -> &[BlockPlacement] {
        &self.placements.values
    }

    pub fn prefix_height(&self) -> f64 {
        self.total_height
    }

    pub fn prefix_quote_count(&self, op: &mut Operation) -> Result<usize, CoreError> {
        self.quotes.prefix_count(self.completed_blocks(), op)
    }

    pub fn charge_prefix_quotes(&self, count: usize, op: &mut Operation) -> Result<(), CoreError> {
        self.quotes.charge_prefix_rows(count, op)
    }

    pub fn prefix_quote(&self, index: usize) -> Option<QuotePlacement> {
        self.quotes.prefix_row(index, self.prefix_placements())
    }

    pub fn disclosures(&self) -> &[Disclosure] {
        self.visibility
            .as_ref()
            .map_or(&[], Visibility::disclosures)
    }

    pub fn hidden_ranges(&self) -> &[crate::identity::LogicalSpan] {
        self.visibility.as_ref().map_or(&[], Visibility::hidden)
    }

    fn check(&self, op: &mut Operation) -> Result<(), CoreError> {
        if let Some(error) = self.fault {
            return Err(op.fail(error));
        }
        same_task(&self.model, &self.cancel, op)
    }

    fn fail(&mut self, error: CoreError, op: &mut Operation) -> CoreError {
        let first = *self.fault.get_or_insert(error);
        op.fail(first)
    }

    /// At most one exact batch is outstanding. A new maximum cannot resize it.
    pub fn measurement_batch(
        &mut self,
        maximum: usize,
        op: &mut Operation,
    ) -> Result<Option<MeasurementBatch<'_>>, CoreError> {
        let prepare = (|| {
            self.check(op)?;
            if !(1..=MAX_MEASUREMENT_BATCH).contains(&maximum) {
                return Err(op.fail(CoreError::InvalidRange));
            }
            let start = self.completed_blocks();
            let pending = self.pending.or_else(|| {
                (start < self.blocks_len()).then(|| Pending {
                    start,
                    count: maximum.min(self.blocks_len() - start),
                })
            });
            op.events(1)?;
            op.checkpoint_now()?;
            Ok(pending)
        })();
        match prepare {
            Err(error) => Err(self.fail(error, op)),
            Ok(None) => Ok(None),
            Ok(Some(pending)) => {
                self.pending = Some(pending);
                Ok(Some(MeasurementBatch {
                    key: self.key,
                    start: pending.start,
                    requests: &self.requests.values[pending.start..pending.start + pending.count],
                    scope: self.scope,
                }))
            }
        }
    }

    pub fn submit_measurements(
        &mut self,
        key: LayoutKey,
        start: usize,
        measurements: &[MeasuredBlock],
        op: &mut Operation,
    ) -> Result<(), CoreError> {
        let result = self.submit_inner(key, start, measurements, op);
        result.map_err(|error| self.fail(error, op))
    }

    fn submit_inner(
        &mut self,
        key: LayoutKey,
        start: usize,
        measurements: &[MeasuredBlock],
        op: &mut Operation,
    ) -> Result<(), CoreError> {
        self.check(op)?;
        if key != self.key {
            return Err(op.fail(CoreError::StaleResult));
        }
        let pending = self
            .pending
            .ok_or_else(|| op.fail(CoreError::InvalidState))?;
        if start != pending.start || start != self.completed_blocks() {
            return Err(op.fail(CoreError::InvalidState));
        }
        if measurements.len() != pending.count {
            return Err(op.fail(CoreError::InvalidRange));
        }
        let end = start
            .checked_add(measurements.len())
            .ok_or_else(|| op.fail(CoreError::ArithmeticOverflow))?;
        if end > self.requests.values.len() || end > self.placements.values.capacity() {
            return Err(op.fail(CoreError::InternalInvariant));
        }
        let mut staged = [BlockPlacement::default(); MAX_MEASUREMENT_BATCH];
        let mut bottom = self.total_height;
        for (index, measured) in measurements.iter().enumerate() {
            op.events(1)?;
            op.scan(size_of::<MeasuredBlock>() as u64)?;
            if measured.node != self.requests.values[start + index].node {
                return Err(op.fail(CoreError::InvalidState));
            }
            let height = types::nonnegative(measured.height).map_err(|error| op.fail(error))?;
            let insets = self.quotes.insets(start + index, op)?;
            let top = if start + index == 0 {
                0.0
            } else {
                let gap = if self.requests.values[start + index].block_gap_before {
                    self.key.style().block_gap()
                } else {
                    0.0
                };
                types::nonnegative(bottom + gap).map_err(|error| op.fail(error))?
            };
            let y = types::nonnegative(top + insets.before).map_err(|error| op.fail(error))?;
            let body_bottom = types::nonnegative(y + height).map_err(|error| op.fail(error))?;
            bottom =
                types::nonnegative(body_bottom + insets.after).map_err(|error| op.fail(error))?;
            staged[index] = BlockPlacement { y, height };
        }
        op.copy((measurements.len() * size_of::<BlockPlacement>()) as u64)?;
        op.checkpoint_now()?;
        // All fallible checks/charges precede this <=256-row, preallocated commit.
        self.placements
            .values
            .extend_from_slice(&staged[..measurements.len()]);
        self.total_height = bottom;
        self.pending = None;
        Ok(())
    }

    /// Consumes this task; incomplete or failed drafts never produce a layout.
    pub fn finish(mut self, op: &mut Operation) -> Result<DocumentLayout, CoreError> {
        let result = (|| {
            self.check(op)?;
            if self.pending.is_some() || self.completed_blocks() != self.blocks_len() {
                return Err(op.fail(CoreError::InvalidState));
            }
            self.quotes.finalize(&self.placements.values, op)?;
            op.checkpoint_now()
        })();
        if let Err(error) = result {
            return Err(self.fail(error, op));
        }
        Ok(DocumentLayout {
            requests: self.requests,
            placements: self.placements,
            quotes: self.quotes.rows,
            visibility: self.visibility,
            model: self.model,
            key: self.key,
            cancel: self.cancel,
            total_height: self.total_height,
        })
    }
}

/// Immutable block geometry. Exact font metrics still belong to the platform.
pub struct DocumentLayout {
    requests: LayoutRows<BlockRequest>,
    placements: LayoutRows<BlockPlacement>,
    quotes: LayoutRows<QuotePlacement>,
    visibility: Option<Visibility>,
    model: AcceptedModel,
    key: LayoutKey,
    cancel: CancellationToken,
    total_height: f64,
}

impl DocumentLayout {
    pub fn key(&self) -> LayoutKey {
        self.key
    }

    pub fn model(&self) -> &AcceptedModel {
        &self.model
    }

    pub fn requests(&self) -> &[BlockRequest] {
        &self.requests.values
    }

    pub fn placements(&self) -> &[BlockPlacement] {
        &self.placements.values
    }

    pub fn quotes(&self) -> &[QuotePlacement] {
        &self.quotes.values
    }

    pub fn disclosures(&self) -> &[Disclosure] {
        self.visibility
            .as_ref()
            .map_or(&[], Visibility::disclosures)
    }

    pub fn hidden_ranges(&self) -> &[crate::identity::LogicalSpan] {
        self.visibility.as_ref().map_or(&[], Visibility::hidden)
    }

    pub fn total_height(&self) -> f64 {
        self.total_height
    }

    /// The host supplies its current key and original task flag before install.
    pub fn check_install(&self, current: LayoutKey, op: &mut Operation) -> Result<(), CoreError> {
        same_task(&self.model, &self.cancel, op)?;
        if current != self.key {
            return Err(op.fail(CoreError::StaleResult));
        }
        op.checkpoint_now()
    }

    /// Smallest directory envelope containing positive-height intersecting blocks.
    /// Interior zero-height blocks may be returned; edges are trimmed with checkpoints.
    pub fn visible_blocks(
        &self,
        viewport: Viewport,
        op: &mut Operation,
    ) -> Result<Range<usize>, CoreError> {
        op.ensure_budget(self.model.budget())?;
        op.checkpoint_now()?;
        visible_placements(&self.placements.values, viewport, op)
    }
}

pub fn visible_placements(
    rows: &[BlockPlacement],
    viewport: Viewport,
    op: &mut Operation,
) -> Result<Range<usize>, CoreError> {
    let interval = viewport.interval().map_err(|error| op.fail(error))?;
    let Some((top, bottom)) = interval else {
        return Ok(0..0);
    };
    let mut lo = 0;
    let mut hi = rows.len();
    while lo < hi {
        op.events(1)?;
        let mid = lo + (hi - lo) / 2;
        if rows[mid].bottom() <= top {
            lo = mid + 1;
        } else {
            hi = mid;
        }
    }
    let mut start = lo;
    hi = rows.len();
    while lo < hi {
        op.events(1)?;
        let mid = lo + (hi - lo) / 2;
        if rows[mid].y < bottom {
            lo = mid + 1;
        } else {
            hi = mid;
        }
    }
    let mut end = lo;
    while start < end && rows[start].height == 0.0 {
        op.events(1)?;
        start += 1;
    }
    while start < end && rows[end - 1].height == 0.0 {
        op.events(1)?;
        end -= 1;
    }
    op.checkpoint_now()?;
    Ok(if start == end { 0..0 } else { start..end })
}

fn same_task(
    model: &AcceptedModel,
    cancel: &CancellationToken,
    op: &mut Operation,
) -> Result<(), CoreError> {
    op.ensure_budget(model.budget())?;
    op.checkpoint_now()?;
    if !op.uses_cancellation(cancel) {
        return Err(op.fail(CoreError::InvalidState));
    }
    Ok(())
}

#[cfg(test)]
mod tests;
