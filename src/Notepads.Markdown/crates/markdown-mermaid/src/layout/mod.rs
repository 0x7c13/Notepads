// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE file in the project root for license information.
// ---------------------------------------------------------------------------------------------

//! native flowchart: one borrowed typed model, one funded compound scene.
mod paint;
pub mod router;
mod routing;
#[cfg(test)]
mod tests;
mod topology;

use crate::{
    canonical::{CanonicalError, FontPlan, PathCommand, PathSink, Rect},
    labels::{self, CompiledLabel, LabelPolicy},
    rich_text::{RichTextProvider, TextDirection},
    scene::{
        self, Color, OwnedScene, Paint, Point, SceneBuilder,
        shapes::Shape,
        style::{self, ResolvedStyle},
    },
};
use markdown_memory::{Family, FundedString, FundedVec, MemoryScope, NativePayload};
use merman_core::{
    OperationControl,
    diagrams::flowchart::{
        FlowEdgeMarker, FlowEdgeStroke, FlowEdgeVisibility, FlowchartModel, FlowchartRenderContext,
    },
};

// Policy counts nested groups. The final leaf is one further vertex level;
// this does not increase the accepted group-nesting limit.
const MAX_GROUP_DEPTH: usize = 8;
const MAX_VERTEX_DEPTH: usize = MAX_GROUP_DEPTH + 1;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum StateMarkerKind {
    Initial,
    Final,
}

// Indexes are in the parsed model, before subgraph anchors are removed from
// the cell array. Producers supply at most 128 entries in increasing order.
#[derive(Clone, Copy, Debug)]
pub(crate) struct StateMarker {
    pub node: usize,
    pub kind: StateMarkerKind,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) enum RelationshipTerminal {
    #[default]
    None,
    Arrow,
    Triangle,
    Diamond,
    OpenDiamond,
    ExactlyOne,
    OptionalOne,
    AtLeastOne,
    OptionalMany,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct RelationshipEnds {
    pub start: RelationshipTerminal,
    pub end: RelationshipTerminal,
}

pub(crate) struct RelationshipEdge {
    pub ends: RelationshipEnds,
    pub multiplicities: [Option<FundedString>; 2],
}

#[derive(Default)]
pub(crate) struct RelationshipStyle<'a> {
    pub header_ends: &'a [Option<usize>],
    pub edges: &'a [RelationshipEdge],
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Direction {
    TopBottom,
    BottomTop,
    LeftRight,
    RightLeft,
}
impl Direction {
    pub fn parse(value: Option<&str>) -> Result<Self, CanonicalError> {
        Ok(match value.unwrap_or("TB") {
            "TB" | "TD" => Self::TopBottom,
            "BT" => Self::BottomTop,
            "LR" => Self::LeftRight,
            "RL" => Self::RightLeft,
            _ => return Err(CanonicalError::UnsupportedProperty),
        })
    }
    fn horizontal(self) -> bool {
        matches!(self, Self::LeftRight | Self::RightLeft)
    }
    fn forward(self) -> usize {
        match self {
            Self::TopBottom => 3,
            Self::BottomTop => 2,
            Self::LeftRight => 1,
            Self::RightLeft => 0,
        }
    }
}

#[derive(Clone, Copy, Debug)]
pub struct LayoutPolicy {
    pub font: FontPlan,
    pub wrap_width: f64,
    pub node_gap: f64,
    pub rank_gap: f64,
    pub padding: f64,
    pub lane_pitch: f64,
    pub clearance: f64,
    pub margin: f64,
}
impl Default for LayoutPolicy {
    fn default() -> Self {
        Self {
            font: FontPlan::default(),
            wrap_width: 220.0,
            node_gap: 32.0,
            rank_gap: 48.0,
            padding: 14.0,
            lane_pitch: 12.0,
            clearance: 10.0,
            margin: 10.0,
        }
    }
}
#[derive(Clone, Copy, Debug)]
pub struct ThemePolicy {
    pub node: Paint,
    pub group: Paint,
    pub edge: Paint,
    pub text: Color,
    pub background: Color,
}
impl Default for ThemePolicy {
    fn default() -> Self {
        Self {
            node: Paint {
                fill: Some(Color::rgb(0xf1f3ff)),
                stroke: Some(Color::rgb(0x515a85)),
                ..Paint::default()
            },
            group: Paint {
                fill: Some(Color::rgb(0xf8f9fc)),
                stroke: Some(Color::rgb(0x929ab0)),
                ..Paint::default()
            },
            edge: Paint {
                fill: None,
                stroke: Some(Color::rgb(0x545c70)),
                ..Paint::default()
            },
            text: Color::rgb(0x202432),
            background: Color::WHITE,
        }
    }
}

struct Cell {
    node: Option<usize>,
    group: Option<usize>,
    parent: usize,
    depth: usize,
    dir: Direction,
    shape: Shape,
    state_marker: Option<StateMarkerKind>,
    style: ResolvedStyle,
    label: Option<CompiledLabel>,
    local: Point,
    bounds: Rect,
    width: f64,
    height: f64,
    rank: usize,
    order: usize,
    portals: [usize; 4],
}
#[derive(Clone, Copy, Default)]
struct RelationshipPortGroup {
    count: usize,
    assigned: usize,
    pitch: f64,
}

struct Edge {
    from: usize,
    to: usize,
    owner: usize,
    style: ResolvedStyle,
    label: Option<CompiledLabel>,
    label_rect: Rect,
    route: FundedVec<Point>,
    feedback: bool,
}
struct RelationshipPorts {
    offsets: [f64; 2],
    multiplicities: [Option<CompiledLabel>; 2],
}

impl Edge {
    fn relationship_endpoints(&self, direction: Direction) -> [(usize, usize); 2] {
        let (from, to) = if self.from == self.to {
            (1, 2)
        } else {
            (direction.forward(), opposite(direction.forward()))
        };
        [(self.from, from), (self.to, to)]
    }
}

#[derive(Clone, Copy)]
struct Portal {
    group: usize,
    side: usize,
    ordinal: usize,
    point: Point,
}
#[derive(Clone, Copy)]
enum Anchor {
    Cell(usize),
    Portal(usize),
}
#[derive(Clone, Copy)]
struct Segment {
    edge: usize,
    owner: usize,
    from: Anchor,
    to: Anchor,
    start: Point,
    end: Point,
    escape_start: Point,
    escape_end: Point,
}

struct Layout<'a> {
    model: &'a FlowchartModel,
    scope: &'a MemoryScope,
    control: &'a OperationControl,
    policy: LayoutPolicy,
    theme: ThemePolicy,
    cells: FundedVec<Cell>,
    edges: FundedVec<Edge>,
    relationship_ports: FundedVec<RelationshipPorts>,
    portals: FundedVec<Portal>,
    segments: FundedVec<Segment>,
    root: usize,
}

fn rect(x: f64, y: f64, w: f64, h: f64) -> Rect {
    Rect {
        left: x,
        top: y,
        right: x + w,
        bottom: y + h,
    }
}
fn center(r: Rect) -> Point {
    Point {
        x: (r.left + r.right) / 2.0,
        y: (r.top + r.bottom) / 2.0,
    }
}
fn expand(r: Rect, p: f64) -> Rect {
    Rect {
        left: r.left - p,
        top: r.top - p,
        right: r.right + p,
        bottom: r.bottom + p,
    }
}
fn normal(side: usize) -> Point {
    match side {
        0 => Point { x: -1.0, y: 0.0 },
        1 => Point { x: 1.0, y: 0.0 },
        2 => Point { x: 0.0, y: -1.0 },
        _ => Point { x: 0.0, y: 1.0 },
    }
}
fn opposite(side: usize) -> usize {
    side ^ 1
}

pub fn render_flowchart_native_controlled(
    input: &NativePayload<(FlowchartModel, FlowchartRenderContext)>,
    policy: LayoutPolicy,
    theme: ThemePolicy,
    provider: &dyn RichTextProvider,
    control: &OperationControl,
) -> Result<OwnedScene, CanonicalError> {
    render_state_flowchart_native_controlled(input, &[], policy, theme, provider, control)
}

// State markers share topology and routing, while ordinary flowchart double
// circles keep their original contour and paint semantics.
pub(crate) fn render_state_flowchart_native_controlled(
    input: &NativePayload<(FlowchartModel, FlowchartRenderContext)>,
    state_markers: &[StateMarker],
    policy: LayoutPolicy,
    theme: ThemePolicy,
    provider: &dyn RichTextProvider,
    control: &OperationControl,
) -> Result<OwnedScene, CanonicalError> {
    render_flowchart_with_relationship_style(
        input,
        state_markers,
        RelationshipStyle::default(),
        policy,
        theme,
        provider,
        control,
    )
}

pub(crate) fn render_flowchart_with_relationship_style(
    input: &NativePayload<(FlowchartModel, FlowchartRenderContext)>,
    state_markers: &[StateMarker],
    relationships: RelationshipStyle<'_>,
    policy: LayoutPolicy,
    theme: ThemePolicy,
    provider: &dyn RichTextProvider,
    control: &OperationControl,
) -> Result<OwnedScene, CanonicalError> {
    let scope = input.region().scope();
    if control.memory_scope().is_none_or(|s| !s.same_root(scope)) {
        return Err(CanonicalError::Memory(
            markdown_memory::MemoryError::Unfunded,
        ));
    }
    control
        .checkpoint()
        .map_err(|_| CanonicalError::Cancelled)?;
    for value in [
        policy.wrap_width,
        policy.node_gap,
        policy.rank_gap,
        policy.padding,
        policy.lane_pitch,
        policy.clearance,
        policy.margin,
    ] {
        if !scene::valid(value) || value < 0.0 {
            return Err(CanonicalError::InvalidGeometry);
        }
    }
    if policy.lane_pitch < 12.0 || policy.clearance < 8.0 || policy.wrap_width < 16.0 {
        return Err(CanonicalError::InvalidGeometry);
    }
    let (model, context) = &**input;
    if (!relationships.edges.is_empty() && relationships.edges.len() != model.edges.len())
        || (!relationships.header_ends.is_empty()
            && relationships.header_ends.len() != model.nodes.len())
    {
        return Err(CanonicalError::InvalidGeometry);
    }
    let mut layout = Layout::new(
        model,
        context,
        state_markers,
        scope,
        control,
        policy,
        theme,
        provider,
    )?;
    layout.prepare_relationship_ports(relationships.edges, provider)?;
    layout.topology()?;
    layout.plan_segments()?;
    layout.place()?;
    layout.route()?;
    layout.paint_with_relationship_style(provider, relationships)
}

impl<'a> Layout<'a> {
    fn checkpoint(&self, work: usize) -> Result<(), CanonicalError> {
        self.control
            .checkpoint()
            .map_err(|_| CanonicalError::Cancelled)?;
        self.scope.charge_work(work)?;
        Ok(())
    }
    fn new(
        model: &'a FlowchartModel,
        context: &'a FlowchartRenderContext,
        state_markers: &[StateMarker],
        scope: &'a MemoryScope,
        control: &'a OperationControl,
        policy: LayoutPolicy,
        theme: ThemePolicy,
        provider: &dyn RichTextProvider,
    ) -> Result<Self, CanonicalError> {
        if model.nodes.len() > 160 || model.edges.len() > 256 || model.subgraphs.len() > 32 {
            return Err(CanonicalError::Limited);
        }
        if state_markers.len() > 128 {
            return Err(CanonicalError::InvalidReference);
        }
        let mut previous = None;
        for marker in state_markers {
            control
                .checkpoint()
                .map_err(|_| CanonicalError::Cancelled)?;
            scope.charge_work(1)?;
            let node = model
                .nodes
                .get(marker.node)
                .ok_or(CanonicalError::InvalidReference)?;
            let expected = match marker.kind {
                StateMarkerKind::Initial => Shape::Circle,
                StateMarkerKind::Final => Shape::DoubleCircle,
            };
            if previous.is_some_and(|index| marker.node <= index)
                || node.is_subgraph_anchor()
                || Shape::parse(node.layout_shape.as_deref().or(node.shape.as_deref()))
                    != Ok(expected)
            {
                return Err(CanonicalError::InvalidReference);
            }
            previous = Some(marker.node);
        }
        scope.charge_work(model.nodes.len())?;
        let authored = model
            .nodes
            .iter()
            .filter(|n| !n.is_subgraph_anchor())
            .count();
        if authored > 128 {
            return Err(CanonicalError::Limited);
        }
        let root = authored + model.subgraphs.len();
        let cell_ticket = scope.reserve_array::<Cell>(root + 1, Family::Graph)?;
        let edge_ticket = scope.reserve_array::<Edge>(model.edges.len(), Family::Graph)?;
        let portal_ticket = scope.reserve_array::<Portal>(
            model
                .edges
                .len()
                .checked_mul(16)
                .ok_or(CanonicalError::Limited)?,
            Family::Graph,
        )?;
        let segment_ticket = scope.reserve_array::<Segment>(
            model
                .edges
                .len()
                .checked_mul(17)
                .ok_or(CanonicalError::Limited)?,
            Family::Graph,
        )?;
        let mut cells = FundedVec::try_with_prepared(cell_ticket, root + 1)?;
        let edges = FundedVec::try_with_prepared(edge_ticket, model.edges.len())?;
        let portals = FundedVec::try_with_prepared(portal_ticket, model.edges.len() * 16)?;
        let segments = FundedVec::try_with_prepared(segment_ticket, model.edges.len() * 17)?;
        let dir = Direction::parse(model.direction.as_deref())?;
        let empty = || Cell {
            node: None,
            group: None,
            parent: root,
            depth: 0,
            dir,
            shape: Shape::Rectangle,
            state_marker: None,
            style: ResolvedStyle::default(),
            label: None,
            local: Point::default(),
            bounds: Rect::default(),
            width: 0.0,
            height: 0.0,
            rank: 0,
            order: 0,
            portals: [0; 4],
        };
        let mut label_bytes = 0usize;
        let mut markers = state_markers.iter().peekable();
        for (index, node) in model.nodes.iter().enumerate() {
            scope.charge_work(1)?;
            if node.is_subgraph_anchor() {
                continue;
            }
            if node.id.len() > 128
                || node.have_callback
                || node.link.is_some()
                || node.icon.is_some()
                || node.img.is_some()
            {
                return Err(CanonicalError::UnsupportedElement);
            }
            let mut cell = empty();
            cell.node = Some(index);
            cell.shape = Shape::parse(node.layout_shape.as_deref().or(node.shape.as_deref()))?;
            if markers.peek().is_some_and(|marker| marker.node == index) {
                cell.state_marker = markers.next().map(|marker| marker.kind);
            }
            cell.style = ResolvedStyle {
                paint: theme.node,
                color: theme.text,
                font: policy.font,
            };
            apply_class(&mut cell.style, model, "default", scope)?;
            apply_class(&mut cell.style, model, "node", scope)?;
            apply_classes(&mut cell.style, model, &node.classes, scope)?;
            for source in &node.styles {
                style::apply(&mut cell.style, source, scope)?;
            }
            let label = context.node_label_for_render(node).unwrap_or(&node.id);
            admit_label(&mut label_bytes, label)?;
            cell.label = Some(labels::compile_label(
                label,
                node.label_type.as_deref() == Some("markdown"),
                LabelPolicy {
                    font: cell.style.font,
                    wrap_width: policy.wrap_width,
                    direction: TextDirection::Auto,
                    line_gap: 0.0,
                },
                scope,
                control,
                provider,
            )?);
            let label = cell.label.as_ref().ok_or(CanonicalError::InvalidGeometry)?;
            (cell.width, cell.height) = if cell.state_marker.is_some() {
                (14.0, 14.0)
            } else {
                cell.shape
                    .dimensions(label.width, label.height, policy.padding)
            };
            cells.try_push(cell)?;
        }
        for (index, group) in model.subgraphs.iter().enumerate() {
            scope.charge_work(1)?;
            if group.id.len() > 128 || group.metadata.is_some() {
                return Err(CanonicalError::UnsupportedElement);
            }
            let mut cell = empty();
            cell.group = Some(index);
            cell.dir = Direction::parse(group.dir.as_deref().or(model.direction.as_deref()))?;
            cell.style = ResolvedStyle {
                paint: theme.group,
                color: theme.text,
                font: policy.font,
            };
            let (classes, styles) = context.effective_subgraph_css(index, group);
            apply_classes(&mut cell.style, model, classes, scope)?;
            for source in styles {
                style::apply(&mut cell.style, source, scope)?;
            }
            let raw_title = context.subgraph_title_for_render(index, group);
            admit_label(&mut label_bytes, raw_title)?;
            cell.label = Some(labels::compile_label(
                raw_title,
                group.label_type.as_deref() == Some("markdown"),
                LabelPolicy {
                    font: cell.style.font,
                    wrap_width: policy.wrap_width,
                    direction: TextDirection::Auto,
                    line_gap: 0.0,
                },
                scope,
                control,
                provider,
            )?);
            cells.try_push(cell)?;
        }
        cells.try_push(empty())?;
        let mut layout = Self {
            model,
            scope,
            control,
            policy,
            theme,
            cells,
            edges,
            relationship_ports: FundedVec::new_in(scope.clone(), Family::Graph),
            portals,
            segments,
            root,
        };
        layout.membership()?;
        for edge in &model.edges {
            layout.checkpoint(1)?;
            if edge.animate == Some(true)
                || edge.animation.is_some()
                || edge.interpolate.as_deref().is_some_and(|s| s != "linear")
                || model
                    .edge_defaults
                    .as_ref()
                    .and_then(|x| x.interpolate.as_deref())
                    .is_some_and(|s| s != "linear")
            {
                return Err(CanonicalError::UnsupportedProperty);
            }
            let from = layout.lookup(&edge.from)?;
            let to = layout.lookup(&edge.to)?;
            let owner = layout.owner(from, to)?;
            let mut resolved = ResolvedStyle {
                paint: theme.edge,
                color: theme.text,
                font: policy.font,
            };
            match edge.stroke_kind {
                FlowEdgeStroke::Dotted => {
                    resolved.paint.dash = [4.0, 4.0, 0.0, 0.0];
                    resolved.paint.dash_len = 2;
                }
                FlowEdgeStroke::Thick => {
                    resolved.paint.stroke_width = resolved.paint.stroke_width.max(3.5)
                }
                _ => {}
            }
            apply_classes(&mut resolved, model, &edge.classes, scope)?;
            if let Some(defaults) = &model.edge_defaults {
                for source in &defaults.style {
                    style::apply(&mut resolved, source, scope)?;
                }
            }
            for source in &edge.style {
                style::apply(&mut resolved, source, scope)?;
            }
            let label = context
                .edge_label_for_render(edge)
                .filter(|s| !s.is_empty())
                .map(|raw| {
                    admit_label(&mut label_bytes, raw)?;
                    labels::compile_label(
                        raw,
                        edge.label_type.as_deref() == Some("markdown"),
                        LabelPolicy {
                            font: resolved.font,
                            wrap_width: policy.wrap_width,
                            direction: TextDirection::Auto,
                            line_gap: 0.0,
                        },
                        scope,
                        control,
                        provider,
                    )
                })
                .transpose()?;
            layout.edges.try_push(Edge {
                from,
                to,
                owner,
                style: resolved,
                label,
                label_rect: Rect::default(),
                route: FundedVec::new_in(scope.clone(), Family::Graph),
                feedback: false,
            })?;
        }
        scope.charge_work(layout.cells.len() + layout.edges.len())?;
        let max_stroke = layout
            .cells
            .iter()
            .map(|c| c.style.paint.stroke_width)
            .chain(layout.edges.iter().map(|e| e.style.paint.stroke_width))
            .fold(0.0f64, f64::max);
        layout.policy.lane_pitch = layout.policy.lane_pitch.max(max_stroke + 16.0);
        layout.policy.clearance = layout.policy.clearance.max(max_stroke + 12.0);
        layout.policy.node_gap = layout
            .policy
            .node_gap
            .max(2.0 * layout.policy.clearance + 4.0);
        Ok(layout)
    }
    fn prepare_relationship_ports(
        &mut self,
        relationships: &[RelationshipEdge],
        provider: &dyn RichTextProvider,
    ) -> Result<(), CanonicalError> {
        if relationships.is_empty() {
            return Ok(());
        }
        if self.root != self.model.nodes.len() || !self.model.subgraphs.is_empty() {
            return Err(CanonicalError::InvalidReference);
        }

        let mut groups = FundedVec::new_in(self.scope.clone(), Family::Graph);
        groups.try_resize(self.root, [RelationshipPortGroup::default(); 4])?;
        let direction = self.cells[self.root].dir;
        for (edge, decoration) in self.edges.iter().zip(relationships) {
            self.scope.charge_work(2)?;
            self.control
                .checkpoint()
                .map_err(|_| CanonicalError::Cancelled)?;
            let mut multiplicities = [None, None];
            for (index, (node, side)) in edge
                .relationship_endpoints(direction)
                .into_iter()
                .enumerate()
            {
                let label = decoration.multiplicities[index]
                    .as_ref()
                    .map(|raw| {
                        labels::compile_label(
                            raw.as_str(),
                            false,
                            LabelPolicy {
                                font: edge.style.font,
                                wrap_width: self.policy.wrap_width.min(96.0),
                                ..LabelPolicy::default()
                            },
                            self.scope,
                            self.control,
                            provider,
                        )
                    })
                    .transpose()?;
                let group = &mut groups[node][side];
                let span =
                    label.as_ref().map_or(
                        0.0,
                        |label| {
                            if side < 2 { label.height } else { label.width }
                        },
                    );
                group.count += 1;
                group.pitch = group.pitch.max((span + 8.0).max(16.0));
                multiplicities[index] = label;
            }
            self.relationship_ports.try_push(RelationshipPorts {
                offsets: [0.0; 2],
                multiplicities,
            })?;
        }

        self.checkpoint(self.root)?;
        for (cell, ports) in self.cells.iter_mut().zip(groups.iter()) {
            let extra =
                |side: usize| ports[side].pitch * ports[side].count.saturating_sub(1) as f64;
            cell.width = cell
                .width
                .max(2.0 * self.policy.padding + extra(2).max(extra(3)));
            cell.height = cell
                .height
                .max(2.0 * self.policy.padding + extra(0).max(extra(1)));
        }

        self.checkpoint(self.edges.len())?;
        for (edge, ports) in self.edges.iter().zip(self.relationship_ports.iter_mut()) {
            for (index, (node, side)) in edge
                .relationship_endpoints(direction)
                .into_iter()
                .enumerate()
            {
                let group = &mut groups[node][side];
                ports.offsets[index] = group.pitch
                    * (group.assigned as f64 - (group.count.saturating_sub(1) as f64 / 2.0));
                group.assigned += 1;
            }
        }
        Ok(())
    }

    fn lookup(&self, id: &str) -> Result<usize, CanonicalError> {
        for (index, cell) in self.cells.iter().enumerate() {
            self.checkpoint(1)?;
            if let Some(g) = cell.group
                && self.model.subgraphs[g].id == id
            {
                return Ok(index);
            }
        }
        for (index, cell) in self.cells.iter().enumerate() {
            self.checkpoint(1)?;
            if let Some(n) = cell.node
                && self.model.nodes[n].id == id
            {
                return Ok(index);
            }
        }
        Err(CanonicalError::InvalidReference)
    }
    fn ancestor(&self, mut child: usize, parent: usize) -> Result<bool, CanonicalError> {
        for _ in 0..=MAX_VERTEX_DEPTH {
            self.checkpoint(1)?;
            if child == parent {
                return Ok(true);
            }
            if child == self.root {
                return Ok(false);
            }
            child = self.cells[child].parent;
        }
        Err(CanonicalError::Limited)
    }
    fn membership(&mut self) -> Result<(), CanonicalError> {
        for group_cell in 0..self.root {
            let Some(g) = self.cells[group_cell].group else {
                continue;
            };
            for id in &self.model.subgraphs[g].nodes {
                let child = self.lookup(id)?;
                if child == group_cell
                    || self.cells[child].parent != self.root
                        && self.cells[child].parent != group_cell
                {
                    return Err(CanonicalError::InvalidReference);
                }
                self.cells[child].parent = group_cell;
            }
        }
        for index in 0..self.root {
            let mut current = index;
            let mut depth = 0;
            let mut seen = [0u64; 3]; // At most 160 model vertices, no recursion.
            loop {
                self.checkpoint(1)?;
                if current == self.root {
                    break;
                }
                let bit = 1u64 << (current % 64);
                if seen[current / 64] & bit != 0 {
                    return Err(CanonicalError::InvalidReference);
                }
                seen[current / 64] |= bit;
                depth += 1;
                current = self.cells[current].parent;
            }
            let limit = if self.cells[index].group.is_some() {
                MAX_GROUP_DEPTH
            } else {
                MAX_VERTEX_DEPTH
            };
            if depth > limit {
                return Err(CanonicalError::Limited);
            }
            self.cells[index].depth = depth;
        }
        Ok(())
    }
    fn owner(&self, from: usize, to: usize) -> Result<usize, CanonicalError> {
        if self.cells[from].group.is_some() && self.ancestor(to, from)? {
            return Ok(from);
        }
        if self.cells[to].group.is_some() && self.ancestor(from, to)? {
            return Ok(to);
        }
        let mut owner = self.cells[from].parent;
        for _ in 0..=8 {
            if self.ancestor(to, owner)? {
                return Ok(owner);
            }
            if owner == self.root {
                break;
            }
            owner = self.cells[owner].parent;
        }
        Err(CanonicalError::InvalidReference)
    }
    fn projected(
        &self,
        mut endpoint: usize,
        owner: usize,
    ) -> Result<Option<usize>, CanonicalError> {
        if endpoint == owner {
            return Ok(None);
        }
        for _ in 0..=8 {
            self.checkpoint(1)?;
            if self.cells[endpoint].parent == owner {
                return Ok(Some(endpoint));
            }
            if endpoint == self.root {
                return Err(CanonicalError::InvalidReference);
            }
            endpoint = self.cells[endpoint].parent;
        }
        Err(CanonicalError::Limited)
    }
}

fn apply_classes(
    style: &mut ResolvedStyle,
    model: &FlowchartModel,
    classes: &[String],
    scope: &MemoryScope,
) -> Result<(), CanonicalError> {
    for class in classes {
        apply_class(style, model, class, scope)?;
    }
    Ok(())
}

fn apply_class(
    style: &mut ResolvedStyle,
    model: &FlowchartModel,
    class: &str,
    scope: &MemoryScope,
) -> Result<(), CanonicalError> {
    scope.charge_work(model.class_defs.len() + 1)?;
    if let Some(definitions) = model.class_defs.get(class) {
        for source in definitions {
            style::apply(style, source, scope)?;
        }
    }
    Ok(())
}

fn admit_label(total: &mut usize, raw: &str) -> Result<(), CanonicalError> {
    *total = total
        .checked_add(raw.len())
        .ok_or(CanonicalError::Limited)?;
    if *total > 16 * 1024 {
        return Err(CanonicalError::Limited);
    }
    Ok(())
}
