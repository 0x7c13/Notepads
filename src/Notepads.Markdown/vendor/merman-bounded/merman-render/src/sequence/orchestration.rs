use super::activation::SequenceActivationState;
use super::actors::{
    SequenceActorLifecycle, SequenceActorLifecycleContext, SequenceFooterActorContext,
    SequenceTopActorContext, append_sequence_footer_actors, append_sequence_top_actors,
    sequence_actor_is_type_width_limited,
};
use super::block_steps::{
    BlockStepPlanContext, SequenceBlockPlan, is_block_end, is_block_section, is_block_start,
    plan_sequence_blocks,
};
use super::message_metrics::{SequenceMessageMetricView, SequenceMessageOwner};
use super::messages::{SequenceMessageLayoutContext, layout_sequence_message};
use super::notes::{SequenceNoteLayoutContext, layout_sequence_note};
use super::rect::SequenceRectOpen;
use super::{SEQUENCE_FRAME_SIDE_PAD_PX, SequenceLayoutCheckpoints};
use crate::Result;
use crate::math::MathRenderer;
use crate::model::{LayoutEdge, LayoutNode, SequenceBlockLayout};
use crate::text::{TextMeasurer, TextStyle};
use merman_core::MermaidConfig;
use merman_core::diagrams::sequence::{
    SequenceControlKind, SequenceControlRole, SequenceDiagramRenderModel, SequenceMessage,
    SequenceMessageKind,
};
use std::collections::HashMap;

pub(super) struct SequenceLayoutGraphContext<'a> {
    pub(super) model: &'a SequenceDiagramRenderModel,
    pub(super) actor_index: &'a HashMap<&'a str, usize>,
    pub(super) actor_centers_x: &'a [f64],
    pub(super) actor_widths: &'a [f64],
    pub(super) actor_base_heights: &'a [f64],
    pub(super) actor_text_heights: &'a [f64],
    pub(super) actor_top_offset_y: f64,
    pub(super) max_actor_layout_height: f64,
    pub(super) sequence_default_width: f64,
    pub(super) actor_height: f64,
    pub(super) actor_margin: f64,
    pub(super) box_margin: f64,
    pub(super) note_margin: f64,
    pub(super) box_text_margin: f64,
    pub(super) label_box_height: f64,
    pub(super) label_box_width: f64,
    pub(super) right_angles: bool,
    pub(super) is_neo: bool,
    pub(super) wrap_padding: f64,
    pub(super) mirror_actors: bool,
    pub(super) activation_width: f64,
    pub(super) measurer: &'a dyn TextMeasurer,
    pub(super) msg_text_style: &'a TextStyle,
    pub(super) note_text_style: &'a TextStyle,
    pub(super) math_config: &'a MermaidConfig,
    pub(super) math_renderer: Option<&'a (dyn MathRenderer + Send + Sync)>,
    pub(super) message_metrics: SequenceMessageMetricView<'a>,
    pub(super) checkpoints: SequenceLayoutCheckpoints<'a>,
}

pub(super) struct SequenceLayoutGraph {
    pub(super) nodes: Vec<LayoutNode>,
    pub(super) edges: Vec<LayoutEdge>,
    pub(super) block_layouts_by_id: rustc_hash::FxHashMap<String, SequenceBlockLayout>,
    pub(super) bottom_box_top_y: f64,
    pub(super) box_height: f64,
    pub(super) bounds_start_x: f64,
    pub(super) bounds_stop_x: f64,
}

struct OpenSequenceBlockLayout {
    id: String,
    start_y: f64,
    /// Position of this control block in Mermaid's active `sequenceItems` stack.
    sequence_item_index: usize,
    start_x: Option<f64>,
    stop_x: Option<f64>,
    section_ys_by_id: rustc_hash::FxHashMap<String, f64>,
}

struct SequenceLayoutLoopState<'a> {
    cursor_y: f64,
    block_stopy_stack: Vec<Option<f64>>,
    open_sequence_blocks: Vec<OpenSequenceBlockLayout>,
    block_layouts_by_id: rustc_hash::FxHashMap<String, SequenceBlockLayout>,
    rect_stack: Vec<SequenceRectOpen>,
    activation_state: SequenceActivationState,
    actor_lifecycle: SequenceActorLifecycle<'a>,
    actor_center_bounds: (f64, f64),
    bounds_start_x: f64,
    bounds_stop_x: f64,
}

impl<'a> SequenceLayoutLoopState<'a> {
    fn new(ctx: &SequenceLayoutGraphContext<'a>) -> Result<Self> {
        let activation_state = SequenceActivationState::new(ctx.activation_width);
        let actor_lifecycle = SequenceActorLifecycle::new(SequenceActorLifecycleContext {
            model: ctx.model,
            actor_index: ctx.actor_index,
            actor_base_heights: ctx.actor_base_heights,
            actor_height: ctx.actor_height,
            checkpoints: ctx.checkpoints,
        });
        let mut bounds = None;
        let mut actor_center_bounds = None;
        for (actor_position, (center_x, width)) in ctx
            .actor_centers_x
            .iter()
            .copied()
            .zip(ctx.actor_widths.iter().copied())
            .enumerate()
        {
            ctx.checkpoints.checkpoint_loop(actor_position)?;
            let actor_bounds = (center_x - width / 2.0, center_x + width / 2.0);
            bounds = Some(bounds.map_or(actor_bounds, |current: (f64, f64)| {
                (current.0.min(actor_bounds.0), current.1.max(actor_bounds.1))
            }));
            actor_center_bounds = Some(
                actor_center_bounds.map_or((center_x, center_x), |current: (f64, f64)| {
                    (current.0.min(center_x), current.1.max(center_x))
                }),
            );
        }
        let (bounds_start_x, bounds_stop_x) =
            bounds.unwrap_or((0.0, ctx.sequence_default_width.max(1.0)));
        let actor_center_bounds = actor_center_bounds.unwrap_or((bounds_start_x, bounds_stop_x));

        Ok(Self {
            cursor_y: ctx.actor_top_offset_y + ctx.max_actor_layout_height,
            block_stopy_stack: Vec::new(),
            open_sequence_blocks: Vec::new(),
            block_layouts_by_id: rustc_hash::FxHashMap::default(),
            rect_stack: Vec::new(),
            activation_state,
            actor_lifecycle,
            actor_center_bounds,
            bounds_start_x,
            bounds_stop_x,
        })
    }

    fn open_content_block(&mut self) {
        self.block_stopy_stack.push(None);
    }

    fn open_sequence_block(&mut self, id: &str, start_y: f64) {
        let sequence_item_index = self.block_stopy_stack.len();
        self.open_content_block();
        self.open_sequence_blocks.push(OpenSequenceBlockLayout {
            id: id.to_string(),
            start_y,
            sequence_item_index,
            start_x: None,
            stop_x: None,
            section_ys_by_id: rustc_hash::FxHashMap::default(),
        });
    }

    fn add_sequence_section(&mut self, id: &str, y: f64) {
        if let Some(open) = self.open_sequence_blocks.last_mut() {
            open.section_ys_by_id.insert(id.to_string(), y);
        }
    }

    fn include_inserted_bottom(&mut self, inserted_bottom_y: f64, box_margin: f64) {
        include_block_stopy(&mut self.block_stopy_stack, inserted_bottom_y, box_margin);
    }

    fn include_inserted_horizontal(&mut self, min_x: f64, max_x: f64, box_margin: f64) {
        let nested_margin = self.block_stopy_stack.len() as f64 * box_margin;
        self.bounds_start_x = self.bounds_start_x.min(min_x - nested_margin);
        self.bounds_stop_x = self.bounds_stop_x.max(max_x + nested_margin);

        // Mermaid updates every active sequence item for each insertion. The outermost item gets
        // the largest margin, while the innermost item gets one box margin. `block_stopy_stack`
        // mirrors that stack and also includes `rect` items, which do not have a control frame.
        let sequence_item_depth = self.block_stopy_stack.len();
        for block in &mut self.open_sequence_blocks {
            let depth_margin =
                sequence_item_depth.saturating_sub(block.sequence_item_index) as f64 * box_margin;
            let frame_start_x = min_x - depth_margin;
            let frame_stop_x = max_x + depth_margin;
            block.start_x = Some(
                block
                    .start_x
                    .map_or(frame_start_x, |current| current.min(frame_start_x)),
            );
            block.stop_x = Some(
                block
                    .stop_x
                    .map_or(frame_stop_x, |current| current.max(frame_stop_x)),
            );
        }
    }

    fn close_content_block(&mut self, box_margin: f64) {
        self.cursor_y = close_block_cursor(&mut self.block_stopy_stack, self.cursor_y, box_margin);
    }

    fn close_sequence_block(&mut self, box_margin: f64) {
        self.close_content_block(box_margin);
        if let Some(open) = self.open_sequence_blocks.pop() {
            self.block_layouts_by_id.insert(
                open.id,
                SequenceBlockLayout {
                    start_y: open.start_y,
                    stop_y: self.cursor_y,
                    start_x: open.start_x,
                    stop_x: open.stop_x,
                    section_ys_by_id: open.section_ys_by_id,
                },
            );
        }
    }
}

fn include_block_stopy(
    block_stopy_stack: &mut [Option<f64>],
    inserted_bottom_y: f64,
    box_margin: f64,
) {
    if let Some(stopy) = block_stopy_stack.last_mut() {
        let candidate = inserted_bottom_y + box_margin;
        *stopy = Some(stopy.map_or(candidate, |current| current.max(candidate)));
    }
}

fn close_block_cursor(
    block_stopy_stack: &mut Vec<Option<f64>>,
    cursor_y: f64,
    box_margin: f64,
) -> f64 {
    let closed_stopy = block_stopy_stack.pop().flatten();
    if let Some(closed_stopy) = closed_stopy {
        if let Some(parent_stopy) = block_stopy_stack.last_mut() {
            let candidate = closed_stopy + box_margin;
            *parent_stopy = Some(parent_stopy.map_or(candidate, |current| current.max(candidate)));
        }
        closed_stopy
    } else {
        cursor_y + box_margin
    }
}

fn handle_sequence_directive<'a>(
    msg: &'a SequenceMessage,
    directive_steps: &HashMap<String, f64>,
    state: &mut SequenceLayoutLoopState<'a>,
    ctx: &SequenceLayoutGraphContext<'a>,
) -> bool {
    if state
        .activation_state
        .handle_directive(msg, ctx.actor_index, ctx.actor_centers_x)
    {
        return true;
    }

    if is_block_start(msg) {
        let start_y = state.cursor_y + ctx.box_margin;
        state.open_sequence_block(&msg.id, start_y);
        state.cursor_y += directive_steps
            .get(msg.id.as_str())
            .copied()
            .unwrap_or_default();
        return true;
    }
    if is_block_end(msg) {
        state.close_sequence_block(ctx.box_margin);
        return true;
    }
    if is_block_section(msg) {
        let section_y = state.cursor_y + ctx.box_margin + ctx.box_text_margin;
        state.add_sequence_section(&msg.id, section_y);
        state.cursor_y += directive_steps
            .get(msg.id.as_str())
            .copied()
            .unwrap_or_default();
        return true;
    }
    false
}

fn handle_sequence_rect(
    msg: &SequenceMessage,
    state: &mut SequenceLayoutLoopState<'_>,
    box_margin: f64,
    rect_step_start: f64,
    nodes: &mut Vec<LayoutNode>,
) -> bool {
    match msg.control_semantics() {
        Some(semantics)
            if semantics.kind == SequenceControlKind::Rect
                && semantics.role == SequenceControlRole::Start =>
        {
            state.rect_stack.push(SequenceRectOpen::new(
                msg.id.clone(),
                state.cursor_y + box_margin,
            ));
            state.cursor_y += rect_step_start;
            state.open_content_block();
            true
        }
        Some(semantics)
            if semantics.kind == SequenceControlKind::Rect
                && semantics.role == SequenceControlRole::End =>
        {
            state.close_content_block(box_margin);
            if let Some(open) = state.rect_stack.pop() {
                let closed = open.close(state.actor_center_bounds);
                nodes.push(closed.node);

                if let Some(parent) = state.rect_stack.last_mut() {
                    parent.include_min_max(
                        closed.left - SEQUENCE_FRAME_SIDE_PAD_PX,
                        closed.right + SEQUENCE_FRAME_SIDE_PAD_PX,
                        closed.bottom,
                    );
                }
            }
            true
        }
        _ => false,
    }
}

fn handle_sequence_note(
    msg: &SequenceMessage,
    state: &mut SequenceLayoutLoopState<'_>,
    ctx: &SequenceLayoutGraphContext<'_>,
    nodes: &mut Vec<LayoutNode>,
) -> Result<bool> {
    if msg.semantic_kind() != SequenceMessageKind::Note {
        return Ok(false);
    }

    let Some(note) = layout_sequence_note(
        msg,
        SequenceNoteLayoutContext {
            actor_index: ctx.actor_index,
            actor_centers_x: ctx.actor_centers_x,
            actor_widths: ctx.actor_widths,
            actor_margin: ctx.actor_margin,
            note_default_width: ctx.sequence_default_width,
            note_margin: ctx.note_margin,
            wrap_padding: ctx.wrap_padding,
            box_margin: ctx.box_margin,
            cursor_y: state.cursor_y,
            measurer: ctx.measurer,
            note_text_style: ctx.note_text_style,
            math_config: ctx.math_config,
            math_renderer: ctx.math_renderer,
            checkpoints: ctx.checkpoints,
        },
    )?
    else {
        return Ok(true);
    };

    include_rect_stack_bounds(
        &mut state.rect_stack,
        note.rect_min_x,
        note.rect_max_x,
        note.rect_max_y,
    );
    state.include_inserted_horizontal(
        note.node.x - note.node.width / 2.0,
        note.node.x + note.node.width / 2.0,
        ctx.box_margin,
    );
    state.include_inserted_bottom(note.rect_max_y, ctx.box_margin);

    nodes.push(note.node);
    state.cursor_y += note.cursor_step;
    Ok(true)
}

fn handle_sequence_message(
    msg: &SequenceMessage,
    msg_idx: usize,
    state: &mut SequenceLayoutLoopState<'_>,
    ctx: &SequenceLayoutGraphContext<'_>,
    edges: &mut Vec<LayoutEdge>,
    actor_is_type_width_limited: &dyn Fn(&str) -> bool,
) -> Result<bool> {
    let Some(message) = layout_sequence_message(
        msg,
        SequenceMessageLayoutContext {
            actor_index: ctx.actor_index,
            actor_centers_x: ctx.actor_centers_x,
            actor_widths: ctx.actor_widths,
            activation_state: &state.activation_state,
            msg_idx,
            actor_width_min: ctx.sequence_default_width,
            box_margin: ctx.box_margin,
            wrap_padding: ctx.wrap_padding,
            is_neo: ctx.is_neo,
            right_angles: ctx.right_angles,
            message_font_size: ctx.msg_text_style.font_size,
            cursor_y: state.cursor_y,
            measurer: ctx.measurer,
            msg_text_style: ctx.msg_text_style,
            math_config: ctx.math_config,
            math_renderer: ctx.math_renderer,
            premeasured_bound: ctx
                .message_metrics
                .get(SequenceMessageOwner::from_model_index(msg_idx), msg),
            created_actor_index: msg
                .to
                .as_deref()
                .and_then(|to| state.actor_lifecycle.created_actor_index(to)),
            destroyed_from_index: msg
                .from
                .as_deref()
                .and_then(|from| state.actor_lifecycle.destroyed_actor_index(from)),
            destroyed_to_index: msg
                .to
                .as_deref()
                .and_then(|to| state.actor_lifecycle.destroyed_actor_index(to)),
            actor_is_type_width_limited,
            checkpoints: ctx.checkpoints,
        },
    )?
    else {
        return Ok(false);
    };
    let super::messages::SequenceMessageLayout {
        edge,
        from_x,
        to_x,
        inserted_min_x,
        inserted_max_x,
        line_y,
        inserted_bottom_y,
        cursor_step,
    } = message;

    include_rect_stack_bounds(
        &mut state.rect_stack,
        from_x.min(to_x) - SEQUENCE_FRAME_SIDE_PAD_PX,
        from_x.max(to_x) + SEQUENCE_FRAME_SIDE_PAD_PX,
        inserted_bottom_y,
    );
    state.include_inserted_horizontal(inserted_min_x, inserted_max_x, ctx.box_margin);
    state.cursor_y += cursor_step;
    let lifecycle_adjustment = state
        .actor_lifecycle
        .apply_message_y_adjustment(msg_idx, &edge.from, &edge.to, line_y);
    state.cursor_y += lifecycle_adjustment;
    state.include_inserted_bottom(inserted_bottom_y + lifecycle_adjustment, ctx.box_margin);
    edges.push(edge);
    Ok(true)
}

pub(super) fn build_sequence_layout_graph(
    ctx: SequenceLayoutGraphContext<'_>,
) -> Result<SequenceLayoutGraph> {
    let mut nodes: Vec<LayoutNode> = Vec::new();
    let mut edges: Vec<LayoutEdge> = Vec::new();

    append_sequence_top_actors(
        &mut nodes,
        SequenceTopActorContext {
            actor_order: &ctx.model.actor_order,
            actors: &ctx.model.actors,
            actor_widths: ctx.actor_widths,
            actor_centers_x: ctx.actor_centers_x,
            actor_base_heights: ctx.actor_base_heights,
            actor_text_heights: ctx.actor_text_heights,
            is_neo: ctx.is_neo,
            actor_top_offset_y: ctx.actor_top_offset_y,
            label_box_height: ctx.label_box_height,
            checkpoints: ctx.checkpoints,
        },
    )?;

    let SequenceBlockPlan { directive_steps } = plan_sequence_blocks(BlockStepPlanContext {
        model: ctx.model,
        actor_index: ctx.actor_index,
        actor_centers_x: ctx.actor_centers_x,
        actor_widths: ctx.actor_widths,
        actor_margin: ctx.actor_margin,
        activation_width: ctx.activation_width,
        box_margin: ctx.box_margin,
        box_text_margin: ctx.box_text_margin,
        label_box_height: ctx.label_box_height,
        label_box_width: ctx.label_box_width,
        sequence_default_width: ctx.sequence_default_width,
        wrap_padding: ctx.wrap_padding,
        note_margin: ctx.note_margin,
        is_neo: ctx.is_neo,
        measurer: ctx.measurer,
        msg_text_style: ctx.msg_text_style,
        note_text_style: ctx.note_text_style,
        math_config: ctx.math_config,
        math_renderer: ctx.math_renderer,
        message_metrics: ctx.message_metrics,
        checkpoints: ctx.checkpoints,
    })?;

    let rect_step_start = 2.0 * ctx.box_margin;
    let mut state = SequenceLayoutLoopState::new(&ctx)?;
    let actor_is_type_width_limited = |actor_id: &str| -> bool {
        sequence_actor_is_type_width_limited(&ctx.model.actors, actor_id)
    };

    for (msg_idx, msg) in ctx.model.messages.iter().enumerate() {
        ctx.checkpoints.checkpoint_loop(msg_idx)?;
        if handle_sequence_directive(msg, &directive_steps, &mut state, &ctx) {
            continue;
        }

        if handle_sequence_rect(msg, &mut state, ctx.box_margin, rect_step_start, &mut nodes) {
            continue;
        }

        if handle_sequence_note(msg, &mut state, &ctx, &mut nodes)? {
            continue;
        }

        if handle_sequence_message(
            msg,
            msg_idx,
            &mut state,
            &ctx,
            &mut edges,
            &actor_is_type_width_limited,
        )? {
            continue;
        }
    }

    let bottom_margin = 2.0 * ctx.box_margin;
    let bottom_box_top_y = state.cursor_y + bottom_margin;

    state
        .actor_lifecycle
        .apply_created_top_actor_positions(&mut nodes)?;

    append_sequence_footer_actors(
        &mut nodes,
        &mut edges,
        SequenceFooterActorContext {
            actor_order: &ctx.model.actor_order,
            actors: &ctx.model.actors,
            actor_widths: ctx.actor_widths,
            actor_centers_x: ctx.actor_centers_x,
            actor_base_heights: ctx.actor_base_heights,
            actor_text_heights: ctx.actor_text_heights,
            is_neo: ctx.is_neo,
            actor_lifecycle: &state.actor_lifecycle,
            actor_top_offset_y: ctx.actor_top_offset_y,
            bottom_box_top_y,
            mirror_actors: ctx.mirror_actors,
            label_box_height: ctx.label_box_height,
            box_text_margin: ctx.box_text_margin,
            checkpoints: ctx.checkpoints,
        },
    )?;
    ctx.checkpoints.checkpoint()?;

    // drawActors advances one shared footer cursor, including destroyed actors' heights.
    let box_height = if ctx.mirror_actors {
        let mut max_footer_height = 0.0_f64;
        for (index, node) in nodes.iter().enumerate() {
            ctx.checkpoints.checkpoint_loop(index)?;
            if node.id.starts_with("actor-bottom-") {
                max_footer_height = max_footer_height.max(node.height);
            }
        }
        bottom_box_top_y + max_footer_height + ctx.box_margin
    } else {
        state.cursor_y
    };
    Ok(SequenceLayoutGraph {
        nodes,
        edges,
        block_layouts_by_id: state.block_layouts_by_id,
        bottom_box_top_y,
        box_height,
        bounds_start_x: state.bounds_start_x,
        bounds_stop_x: state.bounds_stop_x,
    })
}

fn include_rect_stack_bounds(
    rect_stack: &mut [SequenceRectOpen],
    min_x: f64,
    max_x: f64,
    max_y: f64,
) {
    if let Some(open) = rect_stack.last_mut() {
        open.include_min_max(min_x, max_x, max_y);
    }
}

#[cfg(test)]
mod tests {
    use super::{close_block_cursor, include_block_stopy};

    #[test]
    fn block_stopy_uses_configured_margin_and_self_message_bounds() {
        let mut ordinary = vec![None];
        include_block_stopy(&mut ordinary, 100.0, 7.0);
        assert_eq!(close_block_cursor(&mut ordinary, 100.0, 7.0), 107.0);

        let mut self_message = vec![None];
        include_block_stopy(&mut self_message, 160.0, 7.0);
        assert_eq!(close_block_cursor(&mut self_message, 130.0, 7.0), 167.0);
    }

    #[test]
    fn nested_blocks_keep_numeric_depth_margins() {
        let mut stack = vec![None, None];
        include_block_stopy(&mut stack, 160.0, 7.0);

        assert_eq!(close_block_cursor(&mut stack, 130.0, 7.0), 167.0);
        assert_eq!(close_block_cursor(&mut stack, 167.0, 7.0), 174.0);
    }

    #[test]
    fn empty_nested_block_does_not_replace_parent_content_bounds() {
        let mut stack = vec![Some(107.0), None];

        assert_eq!(close_block_cursor(&mut stack, 200.0, 7.0), 207.0);
        assert_eq!(stack, vec![Some(107.0)]);
    }

    #[test]
    fn later_content_consumes_an_earlier_self_message_overhang() {
        let mut stack = vec![None];
        include_block_stopy(&mut stack, 160.0, 10.0);
        include_block_stopy(&mut stack, 220.0, 10.0);

        assert_eq!(close_block_cursor(&mut stack, 220.0, 10.0), 230.0);
    }
}
