// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE file in the project root for license information.
// ---------------------------------------------------------------------------------------------

use crate::error::CoreError;
use crate::identity::{LogicalSpan, ModelIdentity};
use crate::ir::{FlowId, NodeId};

/// DIP spacing used by this first, vertical-only layout implementation.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct LayoutStyle {
    indent: f64,
    block_gap: f64,
    markdown_spacing: bool,
}

impl LayoutStyle {
    pub fn new(indent: f64, block_gap: f64) -> Result<Self, CoreError> {
        nonnegative(indent)?;
        nonnegative(block_gap)?;
        Ok(Self {
            indent,
            block_gap,
            markdown_spacing: false,
        })
    }

    /// Markdown reading keeps tight-list text contiguous while preserving block separators.
    pub fn markdown(indent: f64, block_gap: f64) -> Result<Self, CoreError> {
        Ok(Self {
            markdown_spacing: true,
            ..Self::new(indent, block_gap)?
        })
    }

    pub fn indent(self) -> f64 {
        self.indent
    }

    pub fn block_gap(self) -> f64 {
        self.block_gap
    }

    pub(super) fn uses_markdown_spacing(self) -> bool {
        self.markdown_spacing
    }
}

/// A measurement result belongs to this exact model and geometry snapshot.
/// Callers advance generations when replacing a layout, style or measurement policy.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct LayoutKey {
    model: ModelIdentity,
    layout_generation: u64,
    theme_generation: u64,
    measurement_generation: u64,
    document_width: f64,
    style: LayoutStyle,
}

impl LayoutKey {
    pub fn new(
        model: ModelIdentity,
        layout_generation: u64,
        theme_generation: u64,
        measurement_generation: u64,
        document_width: f64,
        style: LayoutStyle,
    ) -> Result<Self, CoreError> {
        if layout_generation == 0 || theme_generation == 0 || measurement_generation == 0 {
            return Err(CoreError::InvalidRevision);
        }
        positive(document_width)?;
        Ok(Self {
            model,
            layout_generation,
            theme_generation,
            measurement_generation,
            document_width,
            style,
        })
    }

    pub fn model(self) -> ModelIdentity {
        self.model
    }

    pub fn layout_generation(self) -> u64 {
        self.layout_generation
    }

    pub fn theme_generation(self) -> u64 {
        self.theme_generation
    }

    pub fn measurement_generation(self) -> u64 {
        self.measurement_generation
    }

    pub fn document_width(self) -> f64 {
        self.document_width
    }

    pub fn style(self) -> LayoutStyle {
        self.style
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum BlockKind {
    Paragraph,
    Heading,
    CodeBlock,
    HtmlBlock,
    ThematicBreak,
    Table,
    /// Keeps this item's own marker/task object when it has no body block.
    EmptyListItem,
}

/// Original ancestors remain available through the request's node and model pin.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct ContainerContext {
    pub quote_depth: u32,
    pub item_depth: u32,
    pub description_depth: u32,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct BlockRequest {
    pub node: NodeId,
    pub kind: BlockKind,
    /// Apply the style's block gap before this block (except the first block).
    pub block_gap_before: bool,
    pub flow: FlowId,
    /// UTF-16 positions in this flow, not source UTF-8 or a text-pool offset.
    pub logical: LogicalSpan,
    pub context: ContainerContext,
    /// The nearest owning item's gutter before its indent and descendant quotes.
    /// Zero when this request is outside a list item.
    pub marker_x: f64,
    pub x: f64,
    pub available_width: f64,
}

/// One borrowed, exact outstanding batch. Reborrowing does not change its count.
#[derive(Debug)]
pub struct MeasurementBatch<'a> {
    pub key: LayoutKey,
    pub start: usize,
    pub requests: &'a [BlockRequest],
    /// Accepted logical bounds of an excerpt, independent of this batch's window.
    pub scope: Option<LogicalSpan>,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct MeasuredBlock {
    pub node: NodeId,
    pub height: f64,
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct BlockPlacement {
    pub y: f64,
    pub height: f64,
}

impl BlockPlacement {
    pub fn bottom(self) -> f64 {
        self.y + self.height
    }
}

/// One real quote container, in semantic preorder, independent of visible blocks.
/// Its rectangle spans descendant bodies, internal gaps and Markdown inner padding.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct QuotePlacement {
    pub node: NodeId,
    pub depth: u32,
    pub callout: Option<comrak::native::CalloutKind>,
    pub x: f64,
    pub width: f64,
    pub y: f64,
    pub height: f64,
    /// Five DIP normally; clipped to the actual left advance at narrow widths.
    pub border_width: f64,
}

/// Logical DIP viewport. A zero-height viewport stays empty despite overscan.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Viewport {
    pub y: f64,
    pub height: f64,
    pub overscan: f64,
}

impl Viewport {
    pub(super) fn interval(self) -> Result<Option<(f64, f64)>, CoreError> {
        nonnegative(self.y)?;
        nonnegative(self.height)?;
        nonnegative(self.overscan)?;
        let end = nonnegative(nonnegative(self.y + self.height)? + self.overscan)?;
        if self.height == 0.0 {
            return Ok(None);
        }
        Ok(Some(((self.y - self.overscan).max(0.0), end)))
    }
}

pub(super) fn nonnegative(value: f64) -> Result<f64, CoreError> {
    if !value.is_finite() || value < 0.0 {
        Err(CoreError::InvalidBoundary)
    } else {
        Ok(value)
    }
}

pub(super) fn positive(value: f64) -> Result<f64, CoreError> {
    if nonnegative(value)? == 0.0 {
        Err(CoreError::InvalidBoundary)
    } else {
        Ok(value)
    }
}
