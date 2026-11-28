// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE file in the project root for license information.
// ---------------------------------------------------------------------------------------------

use super::storage::LayoutRows;
use super::types::{nonnegative, positive};
use super::{BlockPlacement, ContainerContext, LayoutKey, QuotePlacement};
use crate::error::CoreError;
use crate::ir::{NodeId, NodeKind};
use crate::policy::Operation;

const VERTICAL_PADDING: f64 = 3.0;

#[derive(Clone, Copy)]
struct RequestRange {
    start: usize,
    end: usize,
    leading: f64,
    trailing: f64,
}

#[derive(Clone, Copy, Default)]
pub(super) struct VerticalInsets {
    pub before: f64,
    pub after: f64,
}

/// Ranges exist only while measuring. All retained geometry has its own funding.
pub(super) struct QuoteDirectory {
    pub rows: LayoutRows<QuotePlacement>,
    ranges: LayoutRows<RequestRange>,
    insets: LayoutRows<VerticalInsets>,
}

impl QuoteDirectory {
    pub fn new(count: usize, op: &mut Operation) -> Result<Self, CoreError> {
        Ok(Self {
            rows: LayoutRows::new(count, op)?,
            ranges: LayoutRows::new(count, op)?,
            insets: LayoutRows::new(0, op)?,
        })
    }

    fn push(
        &mut self,
        row: QuotePlacement,
        start: usize,
        op: &mut Operation,
    ) -> Result<usize, CoreError> {
        let index = self.rows.values.len();
        self.rows.push(row, op)?;
        self.ranges.push(
            RequestRange {
                start,
                end: start,
                leading: 0.0,
                trailing: 0.0,
            },
            op,
        )?;
        Ok(index)
    }

    fn close(&mut self, index: usize, end: usize, op: &mut Operation) -> Result<(), CoreError> {
        op.scan(size_of::<RequestRange>() as u64)?;
        let range = self
            .ranges
            .values
            .get_mut(index)
            .ok_or_else(|| op.fail(CoreError::InternalInvariant))?;
        if end < range.start {
            return Err(op.fail(CoreError::InternalInvariant));
        }
        op.copy(size_of::<RequestRange>() as u64)?;
        range.end = end;
        Ok(())
    }

    pub fn prepare_padding(
        &mut self,
        blocks: usize,
        markdown: bool,
        op: &mut Operation,
    ) -> Result<(), CoreError> {
        if !markdown || self.ranges.values.is_empty() {
            return Ok(());
        }
        let mut insets = LayoutRows::new(blocks, op)?;
        for _ in 0..blocks {
            op.events(1)?;
            insets.push(VerticalInsets::default(), op)?;
        }
        // Real, visible request ranges decide whether a container has a body.
        // Empty quotes consume no space, including ancestors with only empty children.
        for range in &mut self.ranges.values {
            op.events(1)?;
            op.scan(size_of::<RequestRange>() as u64)?;
            if range.start > range.end || range.end > blocks {
                return Err(op.fail(CoreError::InternalInvariant));
            }
            if range.start == range.end {
                continue;
            }
            op.scan((2 * size_of::<VerticalInsets>()) as u64)?;
            op.copy((2 * size_of::<f64>() + 2 * size_of::<VerticalInsets>()) as u64)?;
            range.leading = insets.values[range.start].before;
            insets.values[range.start].before =
                nonnegative(range.leading + VERTICAL_PADDING).map_err(|error| op.fail(error))?;
            range.trailing = insets.values[range.end - 1].after;
            insets.values[range.end - 1].after =
                nonnegative(range.trailing + VERTICAL_PADDING).map_err(|error| op.fail(error))?;
        }
        // Preorder puts outer quotes first at shared boundaries. Subtracting
        // their preceding totals gives each rectangle its own nested edges.
        for range in &mut self.ranges.values {
            op.events(1)?;
            op.scan(size_of::<RequestRange>() as u64)?;
            if range.start < range.end {
                op.scan((2 * size_of::<VerticalInsets>()) as u64)?;
                op.copy((2 * size_of::<f64>()) as u64)?;
                range.leading = insets.values[range.start].before - range.leading;
                range.trailing = insets.values[range.end - 1].after - range.trailing;
            }
        }
        self.insets = insets;
        op.checkpoint_now()
    }

    pub fn insets(&self, index: usize, op: &mut Operation) -> Result<VerticalInsets, CoreError> {
        if self.insets.values.is_empty() {
            return Ok(VerticalInsets::default());
        }
        op.scan(size_of::<VerticalInsets>() as u64)?;
        self.insets
            .values
            .get(index)
            .copied()
            .ok_or_else(|| op.fail(CoreError::InternalInvariant))
    }

    pub fn finalize(
        &mut self,
        placements: &[BlockPlacement],
        op: &mut Operation,
    ) -> Result<(), CoreError> {
        if self.rows.values.len() != self.ranges.values.len() {
            return Err(op.fail(CoreError::InternalInvariant));
        }
        for (row, range) in self.rows.values.iter_mut().zip(&self.ranges.values) {
            op.events(1)?;
            op.scan((size_of::<QuotePlacement>() + size_of::<RequestRange>()) as u64)?;
            if range.start > range.end || range.end > placements.len() {
                return Err(op.fail(CoreError::InternalInvariant));
            }
            let (y, height) = if range.start == range.end {
                (0.0, 0.0)
            } else {
                op.scan((2 * size_of::<BlockPlacement>()) as u64)?;
                let y = nonnegative(placements[range.start].y - range.leading)
                    .map_err(|error| op.fail(error))?;
                let bottom = nonnegative(placements[range.end - 1].bottom() + range.trailing)
                    .map_err(|error| op.fail(error))?;
                let height = nonnegative(bottom - y).map_err(|error| op.fail(error))?;
                (y, height)
            };
            op.copy((2 * size_of::<f64>()) as u64)?;
            row.y = y;
            row.height = height;
        }
        op.checkpoint_now()
    }

    pub fn prefix_count(&self, blocks: usize, op: &mut Operation) -> Result<usize, CoreError> {
        // The tree walk enters containers in nondecreasing request order.
        let (mut start, mut end) = (0, self.ranges.values.len());
        while start < end {
            op.events(1)?;
            op.scan(size_of::<RequestRange>() as u64)?;
            let middle = start + (end - start) / 2;
            if self.ranges.values[middle].start < blocks {
                start = middle + 1;
            } else {
                end = middle;
            }
        }
        Ok(start)
    }

    pub fn charge_prefix_rows(&self, count: usize, op: &mut Operation) -> Result<(), CoreError> {
        let row_bytes = size_of::<QuotePlacement>()
            + size_of::<RequestRange>()
            + 2 * size_of::<BlockPlacement>()
            + size_of::<VerticalInsets>();
        let bytes = row_bytes
            .checked_mul(count)
            .ok_or_else(|| op.fail(CoreError::ArithmeticOverflow))?;
        op.scan_chunked(bytes as u64)
    }

    pub fn prefix_row(
        &self,
        index: usize,
        placements: &[BlockPlacement],
    ) -> Option<QuotePlacement> {
        let range = self.ranges.values.get(index)?;
        let mut row = *self.rows.values.get(index)?;
        let end = range.end.min(placements.len());
        if range.start < end {
            row.y = placements[range.start].y - range.leading;
            let trailing = if end == range.end {
                range.trailing
            } else {
                // An open parent includes already committed child closing padding.
                self.insets.values.get(end - 1).map_or(0.0, |v| v.after)
            };
            row.height = placements[end - 1].bottom() + trailing - row.y;
        }
        Some(row)
    }
}

#[derive(Clone, Copy)]
struct Frame {
    node: NodeId,
    left: f64,
    right: f64,
    marker_x: f64,
    quote: Option<usize>,
}

/// Entry order matters: a list consumes the rectangle left by its real parent.
/// Only active Quote/ListItem ancestors need scratch, not all model nodes.
pub(super) struct Geometry {
    frames: LayoutRows<Frame>,
    left: f64,
    right: f64,
    marker_x: f64,
    key: LayoutKey,
}

impl Geometry {
    pub fn new(key: LayoutKey, depth: usize, op: &mut Operation) -> Result<Self, CoreError> {
        Ok(Self {
            frames: LayoutRows::new(depth, op)?,
            left: 0.0,
            right: key.document_width(),
            marker_x: 0.0,
            key,
        })
    }

    pub fn enter(
        &mut self,
        node: NodeId,
        container: (NodeKind, Option<comrak::native::CalloutKind>),
        context: ContainerContext,
        request_index: usize,
        quotes: &mut QuoteDirectory,
        op: &mut Operation,
    ) -> Result<(), CoreError> {
        let (kind, callout) = container;
        if !matches!(
            kind,
            NodeKind::BlockQuote | NodeKind::ListItem | NodeKind::DescriptionDetails
        ) {
            return Ok(());
        }
        let width = positive(self.right - self.left).map_err(|error| op.fail(error))?;
        let minimum_body = self.key.document_width().min(self.key.style().indent());
        // next_down retains a positive body when indent is zero or subnormal.
        let slack = (width - minimum_body).max(0.0).min(width.next_down());
        let markdown = self.key.style().uses_markdown_spacing();
        let (left_advance, right_advance) = if kind == NodeKind::BlockQuote && markdown {
            let left = 15.0_f64.min(slack / 2.0);
            (left, 16.0_f64.min(slack - left))
        } else {
            (self.key.style().indent().min(slack), 0.0)
        };
        let mut left = (self.left + left_advance)
            .min(self.right.next_down())
            .max(self.left);
        let mut right = (self.right - right_advance)
            .max(left.next_up())
            .min(self.right);
        // Preserve exact legacy arithmetic for generic layouts and Markdown
        // lists outside quotes, including extreme finite width/indent ratios.
        if !markdown || context.quote_depth == 0 {
            let depth = context
                .quote_depth
                .checked_add(context.item_depth)
                .and_then(|depth| depth.checked_add(context.description_depth))
                .ok_or_else(|| op.fail(CoreError::ArithmeticOverflow))?;
            let nominal = nonnegative(f64::from(depth) * self.key.style().indent())
                .map_err(|error| op.fail(error))?;
            left = nominal
                .min(self.key.document_width() - minimum_body)
                .min(self.key.document_width().next_down());
            right = self.key.document_width();
        }
        positive(right - left).map_err(|error| op.fail(error))?;
        let quote = if kind == NodeKind::BlockQuote {
            Some(quotes.push(
                QuotePlacement {
                    node,
                    depth: context.quote_depth,
                    callout,
                    x: self.left,
                    width,
                    y: 0.0,
                    height: 0.0,
                    border_width: 5.0_f64.min((left - self.left).max(0.0)),
                },
                request_index,
                op,
            )?)
        } else {
            None
        };
        self.frames.push(
            Frame {
                node,
                left: self.left,
                right: self.right,
                marker_x: self.marker_x,
                quote,
            },
            op,
        )?;
        if kind == NodeKind::ListItem {
            self.marker_x = self.left;
        }
        self.left = left;
        self.right = right;
        Ok(())
    }

    pub fn exit(
        &mut self,
        node: NodeId,
        kind: NodeKind,
        request_index: usize,
        quotes: &mut QuoteDirectory,
        op: &mut Operation,
    ) -> Result<(), CoreError> {
        if !matches!(
            kind,
            NodeKind::BlockQuote | NodeKind::ListItem | NodeKind::DescriptionDetails
        ) {
            return Ok(());
        }
        op.events(1)?;
        op.scan(size_of::<Frame>() as u64)?;
        let frame = self
            .frames
            .values
            .pop()
            .ok_or_else(|| op.fail(CoreError::InternalInvariant))?;
        if frame.node != node {
            return Err(op.fail(CoreError::InternalInvariant));
        }
        if let Some(index) = frame.quote {
            quotes.close(index, request_index, op)?;
        }
        self.left = frame.left;
        self.right = frame.right;
        self.marker_x = frame.marker_x;
        Ok(())
    }

    pub fn body(&self) -> (f64, f64, f64) {
        (self.left, self.right - self.left, self.marker_x)
    }

    pub fn is_empty(&self) -> bool {
        self.frames.values.is_empty()
    }
}
