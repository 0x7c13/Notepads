// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE file in the project root for license information.
// ---------------------------------------------------------------------------------------------

use super::disclosure::Visibility;
use super::quotes::{Geometry, QuoteDirectory};
use super::section::Section;
use super::storage::LayoutRows;
use super::{BlockKind, BlockRequest, ContainerContext, LayoutKey};
use crate::error::{CoreError, LimitKind};
use crate::identity::LogicalSpan;
use crate::ir::{DetailRow, NodeId, NodeKind, NodeRow};
use crate::model::AcceptedModel;
use crate::policy::Operation;

/// The existing immutable parent/child/sibling tree is the traversal stack.
struct Walk<'layout> {
    root: NodeId,
    current: Option<NodeId>,
    context: ContainerContext,
    remaining: usize,
    markdown_spacing: bool,
    pending_gap: bool,
    filter: BlockFilter<'layout>,
}

#[derive(Clone, Copy)]
pub(super) struct BlockFilter<'a> {
    pub visibility: Option<&'a Visibility>,
    pub section: Option<&'a Section>,
}

impl<'layout> Walk<'layout> {
    fn new(
        root: NodeId,
        model: &AcceptedModel,
        markdown_spacing: bool,
        filter: BlockFilter<'layout>,
    ) -> Self {
        Self {
            root,
            current: Some(root),
            context: ContainerContext::default(),
            remaining: model.ir().nodes().len(),
            markdown_spacing,
            pending_gap: true,
            filter,
        }
    }

    fn visible_child(
        &self,
        model: &AcceptedModel,
        mut child: Option<NodeId>,
        op: &mut Operation,
    ) -> Result<Option<NodeId>, CoreError> {
        if self.filter.visibility.is_none() && self.filter.section.is_none() {
            return Ok(child);
        }
        while let Some(node) = child {
            let hidden = match self.filter.visibility {
                Some(visibility) => visibility.hides(node, op)?,
                None => false,
            };
            let included = match self.filter.section {
                Some(section) => section.contains(node, op)?,
                None => true,
            };
            if !hidden && included {
                break;
            }
            child = read_node(model, node, op)?.next_sibling;
        }
        Ok(child)
    }

    fn container_boundary(
        &mut self,
        model: &AcceptedModel,
        row: NodeRow,
        op: &mut Operation,
    ) -> Result<(), CoreError> {
        if self.markdown_spacing
            && (row.kind == NodeKind::BlockQuote
                || row.kind == NodeKind::DescriptionList
                || row.kind == NodeKind::TableOfContents && row.first_child.is_some()
                || (row.kind == NodeKind::List && self.context.item_depth == 0)
                || (row.kind == NodeKind::DescriptionItem && !tight_description(model, row, op)?)
                || (row.kind == NodeKind::ListItem && !tight_item(model, row, op)?))
        {
            self.pending_gap = true;
        }
        Ok(())
    }

    fn enter(
        &mut self,
        model: &AcceptedModel,
        op: &mut Operation,
    ) -> Result<Option<(NodeId, NodeRow, ContainerContext)>, CoreError> {
        let Some(id) = self.current else {
            return Ok(None);
        };
        self.remaining = self
            .remaining
            .checked_sub(1)
            .ok_or_else(|| op.fail(CoreError::InvalidProvenance))?;
        let row = read_node(model, id, op)?;
        self.container_boundary(model, row, op)?;
        let depth = match row.kind {
            NodeKind::BlockQuote => Some(&mut self.context.quote_depth),
            NodeKind::ListItem => Some(&mut self.context.item_depth),
            NodeKind::DescriptionDetails => Some(&mut self.context.description_depth),
            _ => None,
        };
        if let Some(depth) = depth {
            *depth = depth
                .checked_add(1)
                .ok_or_else(|| op.fail(CoreError::ArithmeticOverflow))?;
        }
        Ok(Some((id, row, self.context)))
    }

    fn advance(
        &mut self,
        model: &AcceptedModel,
        row: NodeRow,
        descend: bool,
        op: &mut Operation,
    ) -> Result<(), CoreError> {
        self.advance_with_exit(model, row, descend, op, |_, _, _| Ok(()))
    }

    fn advance_with_exit(
        &mut self,
        model: &AcceptedModel,
        row: NodeRow,
        descend: bool,
        op: &mut Operation,
        mut exit: impl FnMut(NodeId, NodeRow, &mut Operation) -> Result<(), CoreError>,
    ) -> Result<(), CoreError> {
        let mut id = self.current.ok_or(CoreError::InternalInvariant)?;
        if descend && let Some(child) = self.visible_child(model, row.first_child, op)? {
            self.current = Some(child);
            return Ok(());
        }
        let mut row = row;
        loop {
            op.events(1)?;
            exit(id, row, op)?;
            self.container_boundary(model, row, op)?;
            let depth = match row.kind {
                NodeKind::BlockQuote => Some(&mut self.context.quote_depth),
                NodeKind::ListItem => Some(&mut self.context.item_depth),
                NodeKind::DescriptionDetails => Some(&mut self.context.description_depth),
                _ => None,
            };
            if let Some(depth) = depth {
                *depth = depth
                    .checked_sub(1)
                    .ok_or_else(|| op.fail(CoreError::InvalidProvenance))?;
            }
            if id == self.root {
                self.current = None;
                return Ok(());
            }
            let parent = row
                .parent
                .ok_or_else(|| op.fail(CoreError::InvalidProvenance))?;
            if let Some(sibling) = self.visible_child(model, row.next_sibling, op)? {
                self.current = Some(sibling);
                return Ok(());
            }
            id = parent;
            row = read_node(model, id, op)?;
        }
    }
}

fn read_node(model: &AcceptedModel, id: NodeId, op: &mut Operation) -> Result<NodeRow, CoreError> {
    op.events(1)?;
    op.scan(size_of::<NodeRow>() as u64)?;
    let row = model
        .ir()
        .nodes()
        .get(id.index())
        .copied()
        .ok_or_else(|| op.fail(CoreError::InvalidProvenance))?;
    if row.flow != model.main_flow_id() {
        return Err(op.fail(CoreError::OutsideScope));
    }
    Ok(row)
}

fn tight_item(model: &AcceptedModel, item: NodeRow, op: &mut Operation) -> Result<bool, CoreError> {
    let parent = item
        .parent
        .ok_or_else(|| op.fail(CoreError::InvalidProvenance))?;
    let list = read_node(model, parent, op)?;
    if list.kind != NodeKind::List {
        return Err(op.fail(CoreError::InvalidProvenance));
    }
    let detail = list
        .detail
        .ok_or_else(|| op.fail(CoreError::InvalidProvenance))?;
    op.events(1)?;
    op.scan(size_of::<DetailRow>() as u64)?;
    match model.ir().details().get(detail.index()) {
        Some(DetailRow::List { tight, .. }) => Ok(*tight),
        _ => Err(op.fail(CoreError::InvalidProvenance)),
    }
}

fn tight_description(
    model: &AcceptedModel,
    item: NodeRow,
    op: &mut Operation,
) -> Result<bool, CoreError> {
    let id = item
        .detail
        .ok_or_else(|| op.fail(CoreError::InvalidProvenance))?;
    op.scan(size_of::<DetailRow>() as u64)?;
    match model.ir().details().get(id.index()) {
        Some(DetailRow::DescriptionItem { tight, .. }) => Ok(*tight),
        _ => Err(op.fail(CoreError::InvalidProvenance)),
    }
}

fn compact_body(
    model: &AcceptedModel,
    node: NodeId,
    row: NodeRow,
    op: &mut Operation,
) -> Result<bool, CoreError> {
    if row.kind == NodeKind::TableOfContentsEntry {
        return Ok(true);
    }
    if row.kind == NodeKind::ListItem {
        return tight_item(model, row, op);
    }
    if row.kind == NodeKind::Paragraph
        && let Some(parent) = row.parent
    {
        let item = read_node(model, parent, op)?;
        if item.kind == NodeKind::ListItem {
            return tight_item(model, item, op);
        }
        if matches!(
            item.kind,
            NodeKind::DescriptionTerm | NodeKind::DescriptionDetails
        ) && item.first_child == Some(node)
        {
            let owner = read_node(model, item.parent.ok_or(CoreError::InvalidProvenance)?, op)?;
            return tight_description(model, owner, op);
        }
    }
    Ok(false)
}

fn body_kind(kind: NodeKind) -> Option<BlockKind> {
    Some(match kind {
        NodeKind::Paragraph
        | NodeKind::Diagram
        | NodeKind::MathBlock
        | NodeKind::FootnoteFooter
        | NodeKind::TableOfContentsEntry => BlockKind::Paragraph,
        NodeKind::Heading => BlockKind::Heading,
        NodeKind::CodeBlock => BlockKind::CodeBlock,
        NodeKind::HtmlBlock => BlockKind::HtmlBlock,
        NodeKind::ThematicBreak => BlockKind::ThematicBreak,
        NodeKind::Table => BlockKind::Table,
        _ => return None,
    })
}

/// Nested items own their own body. Their text cannot erase this item's marker.
fn empty_item_range(
    model: &AcceptedModel,
    item: NodeId,
    item_row: NodeRow,
    filter: BlockFilter<'_>,
    op: &mut Operation,
) -> Result<Option<LogicalSpan>, CoreError> {
    let mut walk = Walk::new(item, model, false, filter);
    let mut own_end = item_row.logical.start();
    while let Some((id, row, _)) = walk.enter(model, op)? {
        if id != item && row.kind == NodeKind::ListItem {
            walk.advance(model, row, false, op)?;
            continue;
        }
        if body_kind(row.kind).is_some() {
            return Ok(None);
        }
        if row.kind == NodeKind::TaskCheckbox {
            own_end = own_end.max(row.logical.end());
        }
        walk.advance(model, row, true, op)?;
    }
    LogicalSpan::new(item_row.logical.start(), own_end)
        .map(Some)
        .map_err(|error| op.fail(error))
}

fn request_kind(
    model: &AcceptedModel,
    id: NodeId,
    row: NodeRow,
    filter: BlockFilter<'_>,
    op: &mut Operation,
) -> Result<Option<(BlockKind, LogicalSpan)>, CoreError> {
    if let Some(section) = filter.section
        && !section.contains_body(id, op)?
    {
        return Ok(None);
    }
    if row.kind == NodeKind::FootnoteDefinition {
        Ok(Some((
            BlockKind::Paragraph,
            footnote_label_span(model, row, op)?,
        )))
    } else if row.kind == NodeKind::ListItem {
        Ok(empty_item_range(model, id, row, filter, op)?
            .map(|logical| (BlockKind::EmptyListItem, logical)))
    } else {
        Ok(body_kind(row.kind).map(|kind| (kind, row.logical)))
    }
}

pub(crate) fn footnote_label_span(
    model: &AcceptedModel,
    row: NodeRow,
    op: &mut Operation,
) -> Result<LogicalSpan, CoreError> {
    op.events(1)?;
    op.scan(size_of::<DetailRow>() as u64)?;
    let detail = row
        .detail
        .and_then(|id| model.ir().details().get(id.index()));
    let Some(DetailRow::FootnoteDefinition { ordinal, .. }) = detail else {
        return Err(op.fail(CoreError::InvalidProvenance));
    };
    let digits = ordinal
        .checked_ilog10()
        .ok_or_else(|| op.fail(CoreError::InvalidProvenance))?;
    let start = row.logical.start();
    let end = start
        .checked_add(u64::from(digits) + 2)
        .ok_or_else(|| op.fail(CoreError::ArithmeticOverflow))?;
    if end > row.logical.end() {
        return Err(op.fail(CoreError::InvalidProvenance));
    }
    LogicalSpan::new(start, end).map_err(|error| op.fail(error))
}

pub(super) struct Counts {
    pub requests: usize,
    pub quotes: usize,
    pub depth: usize,
}

pub(super) fn counts(
    model: &AcceptedModel,
    key: LayoutKey,
    filter: BlockFilter<'_>,
    op: &mut Operation,
) -> Result<Counts, CoreError> {
    let max = model
        .budget()
        .model_limits()
        .ok_or_else(|| op.fail(CoreError::UnsupportedConfiguration))?
        .model_records;
    let root = model.ir().flows()[model.main_flow_id().index()].root;
    let mut walk = Walk::new(root, model, key.style().uses_markdown_spacing(), filter);
    let mut counts = Counts {
        requests: 0,
        quotes: 0,
        depth: 0,
    };
    while let Some((id, row, context)) = walk.enter(model, op)? {
        let depth = context
            .quote_depth
            .checked_add(context.item_depth)
            .and_then(|depth| depth.checked_add(context.description_depth))
            .ok_or_else(|| op.fail(CoreError::ArithmeticOverflow))?;
        counts.depth = counts.depth.max(depth as usize);
        if row.kind == NodeKind::BlockQuote {
            counts.quotes = counts
                .quotes
                .checked_add(1)
                .ok_or_else(|| op.fail(CoreError::ArithmeticOverflow))?;
        }
        if request_kind(model, id, row, filter, op)?.is_some() {
            counts.requests = counts
                .requests
                .checked_add(1)
                .ok_or_else(|| op.fail(CoreError::ArithmeticOverflow))?;
        }
        let records = counts
            .requests
            .checked_add(counts.quotes)
            .ok_or_else(|| op.fail(CoreError::ArithmeticOverflow))?;
        op.check_limit(LimitKind::ModelRecords, records as u64, max)?;
        walk.advance(model, row, body_kind(row.kind).is_none(), op)?;
    }
    Ok(counts)
}

pub(super) fn requests(
    model: &AcceptedModel,
    key: LayoutKey,
    counts: &Counts,
    filter: BlockFilter<'_>,
    requests: &mut LayoutRows<BlockRequest>,
    quotes: &mut QuoteDirectory,
    op: &mut Operation,
) -> Result<(), CoreError> {
    let root = model.ir().flows()[model.main_flow_id().index()].root;
    let markdown_spacing = key.style().uses_markdown_spacing();
    let mut walk = Walk::new(root, model, markdown_spacing, filter);
    let mut geometry = Geometry::new(key, counts.depth, op)?;
    let mut disclosures = model.disclosure_regions().iter().peekable();
    while let Some((id, row, context)) = walk.enter(model, op)? {
        let callout = if row.kind == NodeKind::BlockQuote
            && let Some(id) = row.detail
        {
            op.events(1)?;
            op.scan(size_of::<DetailRow>() as u64)?;
            match model.ir().details().get(id.index()) {
                Some(DetailRow::Callout { kind, .. }) => Some(*kind),
                _ => return Err(op.fail(CoreError::InvalidProvenance)),
            }
        } else {
            None
        };
        geometry.enter(
            id,
            (row.kind, callout),
            context,
            requests.values.len(),
            quotes,
            op,
        )?;
        let body = body_kind(row.kind);
        if let Some((kind, logical)) = request_kind(model, id, row, filter, op)? {
            let compact = markdown_spacing && compact_body(model, id, row, op)?;
            let (mut x, mut available_width, marker_x) = geometry.body();
            if row.kind == NodeKind::TableOfContentsEntry {
                op.scan(size_of::<DetailRow>() as u64)?;
                let Some(DetailRow::TableOfContentsEntry { depth, .. }) = row
                    .detail
                    .and_then(|id| model.ir().details().get(id.index()))
                else {
                    return Err(op.fail(CoreError::InvalidProvenance));
                };
                let inset = (f64::from(*depth) * key.style().indent()).min(available_width / 2.0);
                x += inset;
                available_width -= inset;
            }
            while disclosures.next_if(|region| region.node < id).is_some() {
                op.events(1)?;
            }
            if let Some(region) = disclosures.peek() {
                op.scan(size_of::<crate::model::DisclosureRegion>() as u64)?;
                if region.node == id && region.summary == id {
                    let inset = (available_width * 0.3).min(18.0);
                    x += inset;
                    available_width -= inset;
                }
            }
            requests.push(
                BlockRequest {
                    node: id,
                    kind,
                    block_gap_before: !compact || walk.pending_gap,
                    flow: row.flow,
                    logical,
                    context,
                    marker_x,
                    x,
                    available_width,
                },
                op,
            )?;
            walk.pending_gap = !compact;
        }
        // A semantic table is one request. Its cells, inline descendants and
        // image alt flows stay in the accepted model rather than becoming blocks.
        walk.advance_with_exit(model, row, body.is_none(), op, |id, row, op| {
            geometry.exit(id, row.kind, requests.values.len(), quotes, op)
        })?;
    }
    if !geometry.is_empty()
        || requests.values.len() != counts.requests
        || quotes.rows.values.len() != counts.quotes
    {
        return Err(op.fail(CoreError::InternalInvariant));
    }
    Ok(())
}
