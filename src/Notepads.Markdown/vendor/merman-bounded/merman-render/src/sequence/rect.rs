use super::SequenceLayoutCheckpoints;
use super::constants::{SEQUENCE_FRAME_GEOM_PAD_PX, SEQUENCE_FRAME_SIDE_PAD_PX};
use crate::Result;
use crate::model::{LayoutEdge, LayoutNode};
use merman_core::diagrams::sequence::{
    SequenceControlKind, SequenceControlRole, SequenceDiagramRenderModel, SequenceMessage,
    SequenceMessageKind,
};
use merman_core::geom::Box2;
use std::collections::HashMap;

#[derive(Debug, Clone)]
pub(super) struct SequenceRectOpen {
    start_id: String,
    top_y: f64,
    bounds: Option<Box2>,
}

impl SequenceRectOpen {
    pub(super) fn new(start_id: String, top_y: f64) -> Self {
        Self {
            start_id,
            top_y,
            bounds: None,
        }
    }

    pub(super) fn include_min_max(&mut self, min_x: f64, max_x: f64, max_y: f64) {
        let r = Box2::from_min_max(min_x, self.top_y, max_x, max_y);
        if let Some(ref mut cur) = self.bounds {
            cur.union(r);
        } else {
            self.bounds = Some(r);
        }
    }

    pub(super) fn close(self, actor_center_bounds: (f64, f64)) -> ClosedSequenceRect {
        let rect_left = self
            .bounds
            .map(|b| b.min_x())
            .unwrap_or(actor_center_bounds.0 - SEQUENCE_FRAME_SIDE_PAD_PX);
        let rect_right = self
            .bounds
            .map(|b| b.max_x())
            .unwrap_or(actor_center_bounds.1 + SEQUENCE_FRAME_SIDE_PAD_PX);
        let rect_bottom = self
            .bounds
            .map(|b| b.max_y() + SEQUENCE_FRAME_GEOM_PAD_PX)
            .unwrap_or(self.top_y + SEQUENCE_FRAME_GEOM_PAD_PX);
        let rect_w = (rect_right - rect_left).max(1.0);
        let rect_h = (rect_bottom - self.top_y).max(1.0);

        ClosedSequenceRect {
            node: LayoutNode {
                id: format!("rect-{}", self.start_id),
                x: rect_left + rect_w / 2.0,
                y: self.top_y + rect_h / 2.0,
                width: rect_w,
                height: rect_h,
                is_cluster: false,
                label_width: None,
                label_height: None,
            },
            left: rect_left,
            right: rect_right,
            bottom: rect_bottom,
        }
    }
}

pub(super) struct ClosedSequenceRect {
    pub(super) node: LayoutNode,
    pub(super) left: f64,
    pub(super) right: f64,
    pub(super) bottom: f64,
}

pub(super) struct SequenceRectStackBoundsContext<'a> {
    pub(super) model: &'a SequenceDiagramRenderModel,
    pub(super) actor_index: &'a HashMap<&'a str, usize>,
    pub(super) actor_centers_x: &'a [f64],
    pub(super) edges: &'a [LayoutEdge],
    pub(super) nodes: &'a [LayoutNode],
    pub(super) actor_width_min: f64,
    pub(super) box_margin: f64,
    pub(super) checkpoints: SequenceLayoutCheckpoints<'a>,
}

pub(super) fn sequence_rect_stack_x_bounds(
    ctx: SequenceRectStackBoundsContext<'_>,
) -> Result<HashMap<String, (f64, f64)>> {
    let mut edges_by_id = HashMap::with_capacity(ctx.edges.len());
    for (edge_index, edge) in ctx.edges.iter().enumerate() {
        ctx.checkpoints.checkpoint_loop(edge_index)?;
        edges_by_id.insert(edge.id.as_str(), edge);
    }
    let mut nodes_by_id = HashMap::with_capacity(ctx.nodes.len());
    for (node_index, node) in ctx.nodes.iter().enumerate() {
        ctx.checkpoints.checkpoint_loop(node_index)?;
        nodes_by_id.insert(node.id.as_str(), node);
    }

    let mut stack: Vec<StackFrame> = Vec::new();
    let mut rect_bounds: HashMap<String, (f64, f64)> = HashMap::new();

    for (message_index, msg) in ctx.model.messages.iter().enumerate() {
        ctx.checkpoints.checkpoint_loop(message_index)?;
        match msg.control_semantics() {
            Some(semantics)
                if semantics.kind != SequenceControlKind::Rect
                    && semantics.role == SequenceControlRole::Start =>
            {
                stack.push(StackFrame::control());
            }
            Some(semantics)
                if semantics.kind != SequenceControlKind::Rect
                    && semantics.role == SequenceControlRole::End =>
            {
                close_stack_frame(&mut stack, ctx.box_margin, false, &mut rect_bounds);
            }
            Some(semantics)
                if semantics.kind == SequenceControlKind::Rect
                    && semantics.role == SequenceControlRole::Start =>
            {
                stack.push(StackFrame::rect(msg.id.clone()));
            }
            Some(semantics)
                if semantics.kind == SequenceControlKind::Rect
                    && semantics.role == SequenceControlRole::End =>
            {
                close_stack_frame(&mut stack, ctx.box_margin, true, &mut rect_bounds);
            }
            _ => {
                if stack.is_empty() {
                    continue;
                }
                if let Some((x1, x2)) = message_x_range(
                    msg,
                    ctx.actor_index,
                    ctx.actor_centers_x,
                    &edges_by_id,
                    &nodes_by_id,
                    ctx.actor_width_min,
                ) && let Some(frame) = stack.last_mut()
                {
                    frame
                        .bounds
                        .include(x1 - ctx.box_margin, x2 + ctx.box_margin);
                }
            }
        }
    }

    ctx.checkpoints.checkpoint()?;
    Ok(rect_bounds)
}

#[derive(Debug, Clone)]
enum StackFrameKind {
    Rect { start_id: String },
    Control,
}

#[derive(Debug, Clone, Copy)]
struct StackFrameBounds {
    min_x: f64,
    max_x: f64,
}

impl StackFrameBounds {
    fn empty() -> Self {
        Self {
            min_x: f64::INFINITY,
            max_x: f64::NEG_INFINITY,
        }
    }

    fn include(&mut self, min_x: f64, max_x: f64) {
        self.min_x = self.min_x.min(min_x);
        self.max_x = self.max_x.max(max_x);
    }

    fn is_finite(self) -> bool {
        self.min_x.is_finite() && self.max_x.is_finite()
    }
}

#[derive(Debug, Clone)]
struct StackFrame {
    kind: StackFrameKind,
    bounds: StackFrameBounds,
}

impl StackFrame {
    fn control() -> Self {
        Self {
            kind: StackFrameKind::Control,
            bounds: StackFrameBounds::empty(),
        }
    }

    fn rect(start_id: String) -> Self {
        Self {
            kind: StackFrameKind::Rect { start_id },
            bounds: StackFrameBounds::empty(),
        }
    }
}

fn close_stack_frame(
    stack: &mut Vec<StackFrame>,
    box_margin: f64,
    record_rect: bool,
    rect_bounds: &mut HashMap<String, (f64, f64)>,
) {
    let Some(frame) = stack.pop() else {
        return;
    };

    if record_rect
        && frame.bounds.is_finite()
        && let StackFrameKind::Rect { start_id } = &frame.kind
    {
        rect_bounds.insert(start_id.clone(), (frame.bounds.min_x, frame.bounds.max_x));
    }

    if frame.bounds.is_finite()
        && let Some(parent) = stack.last_mut()
    {
        parent.bounds.include(
            frame.bounds.min_x - box_margin,
            frame.bounds.max_x + box_margin,
        );
    }
}

fn message_x_range(
    msg: &SequenceMessage,
    actor_index: &HashMap<&str, usize>,
    actor_centers_x: &[f64],
    edges_by_id: &HashMap<&str, &LayoutEdge>,
    nodes_by_id: &HashMap<&str, &LayoutNode>,
    actor_width_min: f64,
) -> Option<(f64, f64)> {
    if msg.semantic_kind() == SequenceMessageKind::Note {
        let note_id = format!("note-{}", msg.id);
        let n = nodes_by_id.get(note_id.as_str()).copied()?;
        return Some((n.x - n.width / 2.0, n.x + n.width / 2.0));
    }

    let (Some(from), Some(to)) = (msg.from.as_deref(), msg.to.as_deref()) else {
        return None;
    };
    let edge_id = format!("msg-{}", msg.id);
    let e = edges_by_id.get(edge_id.as_str()).copied()?;

    if from == to {
        let line_x = e
            .points
            .first()
            .map(|p| p.x)
            .or_else(|| actor_index.get(from).map(|&i| actor_centers_x[i] + 1.0))?;
        let label_width = e.label.as_ref().map(|label| label.width).unwrap_or(1.0);
        let dx = (label_width / 2.0).max(actor_width_min / 2.0);
        return Some((line_x - dx, line_x + dx));
    }

    let mut min_x = f64::INFINITY;
    let mut max_x = f64::NEG_INFINITY;
    for p in &e.points {
        min_x = min_x.min(p.x);
        max_x = max_x.max(p.x);
    }
    if !min_x.is_finite() || !max_x.is_finite() {
        return None;
    }
    Some((min_x, max_x))
}

#[cfg(test)]
mod tests {
    use super::{SEQUENCE_FRAME_SIDE_PAD_PX, SequenceRectOpen, StackFrame, close_stack_frame};
    use std::collections::HashMap;

    #[test]
    fn nested_stack_frames_propagate_horizontal_margins_on_close() {
        let mut stack = vec![
            StackFrame::rect("outer".to_string()),
            StackFrame::control(),
            StackFrame::rect("inner".to_string()),
        ];
        stack
            .last_mut()
            .expect("inner frame")
            .bounds
            .include(90.0, 210.0);
        let mut rect_bounds = HashMap::new();

        close_stack_frame(&mut stack, 10.0, true, &mut rect_bounds);
        close_stack_frame(&mut stack, 10.0, false, &mut rect_bounds);
        close_stack_frame(&mut stack, 10.0, true, &mut rect_bounds);

        assert_eq!(rect_bounds["inner"], (90.0, 210.0));
        assert_eq!(rect_bounds["outer"], (70.0, 230.0));
    }

    #[test]
    fn empty_rect_uses_precomputed_actor_extrema() {
        let closed = SequenceRectOpen::new("empty".to_string(), 20.0).close((100.0, 300.0));

        assert_eq!(closed.left, 100.0 - SEQUENCE_FRAME_SIDE_PAD_PX);
        assert_eq!(closed.right, 300.0 + SEQUENCE_FRAME_SIDE_PAD_PX);
        assert_eq!(closed.node.width, 200.0 + 2.0 * SEQUENCE_FRAME_SIDE_PAD_PX);
    }
}
