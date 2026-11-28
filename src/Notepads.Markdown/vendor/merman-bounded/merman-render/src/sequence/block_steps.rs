use super::activation::SequenceActivationState;
use super::message_metrics::{SequenceMessageMetricView, SequenceMessageOwner};
use super::messages::{
    SequenceMessageHorizontalContext, SequenceMessageHorizontalModel,
    sequence_message_horizontal_model,
};
use super::metrics::{
    SequenceMathHeightMode, measure_sequence_math_label, measure_svg_like_with_html_br,
};
use super::notes::{SequenceNoteHorizontalContext, sequence_note_horizontal_model};
use super::{
    SequenceLayoutCheckpoints, bracketize_sequence_block_label, sequence_block_label_wrap_width,
    wrap_sequence_label_like_mermaid_lines,
};
use crate::Result;
use crate::math::MathRenderer;
use crate::text::{TextMeasurer, TextStyle};
use merman_core::MermaidConfig;
use merman_core::diagrams::sequence::{
    SequenceControlKind, SequenceControlRole, SequenceDiagramRenderModel, SequenceMessage,
};
use std::collections::HashMap;

#[derive(Clone, Copy)]
pub(super) struct BlockStepPlanContext<'a> {
    pub(super) model: &'a SequenceDiagramRenderModel,
    pub(super) actor_index: &'a HashMap<&'a str, usize>,
    pub(super) actor_centers_x: &'a [f64],
    pub(super) actor_widths: &'a [f64],
    pub(super) actor_margin: f64,
    pub(super) activation_width: f64,
    pub(super) box_margin: f64,
    pub(super) box_text_margin: f64,
    pub(super) label_box_height: f64,
    pub(super) label_box_width: f64,
    pub(super) sequence_default_width: f64,
    pub(super) wrap_padding: f64,
    pub(super) note_margin: f64,
    pub(super) is_neo: bool,
    pub(super) measurer: &'a dyn TextMeasurer,
    pub(super) msg_text_style: &'a TextStyle,
    pub(super) note_text_style: &'a TextStyle,
    pub(super) math_config: &'a MermaidConfig,
    pub(super) math_renderer: Option<&'a (dyn MathRenderer + Send + Sync)>,
    pub(super) message_metrics: SequenceMessageMetricView<'a>,
    pub(super) checkpoints: SequenceLayoutCheckpoints<'a>,
}

pub(super) struct SequenceBlockPlan {
    pub(super) directive_steps: HashMap<String, f64>,
}

pub(super) fn plan_sequence_blocks(ctx: BlockStepPlanContext<'_>) -> Result<SequenceBlockPlan> {
    let frame_ctx = ctx.frame_width_context();
    let widths_by_id = calculate_sequence_block_widths(ctx)?;
    let step_ctx = BlockStepContext {
        block_base_step_empty: (2.0 * ctx.box_margin + ctx.box_text_margin).max(0.0),
        label_box_height: ctx.label_box_height,
        wrap_padding: ctx.wrap_padding,
    };

    let mut directive_steps = HashMap::new();
    for (message_index, msg) in ctx.model.messages.iter().enumerate() {
        ctx.checkpoints.checkpoint_loop(message_index)?;
        if !is_block_label_directive(msg) {
            continue;
        }
        // Mermaid wraps block labels against the computed loop width. The label box occupies
        // only the leading part of the frame; it does not cap the available loop text width.
        let frame_width = widths_by_id.get(&msg.id).copied();
        directive_steps.insert(
            msg.id.clone(),
            block_label_step(msg.message_text(), frame_width, frame_ctx, step_ctx)?,
        );
    }
    ctx.checkpoints.checkpoint()?;

    Ok(SequenceBlockPlan { directive_steps })
}

pub(super) fn calculate_sequence_block_widths(
    ctx: BlockStepPlanContext<'_>,
) -> Result<HashMap<String, f64>> {
    let bounds_by_id = calculate_sequence_block_bounds(
        &ctx.model.messages,
        ctx.frame_width_context(),
        ctx.checkpoints,
    )?;
    let mut widths_by_id = HashMap::with_capacity(bounds_by_id.len());
    for (index, (id, bounds)) in bounds_by_id.into_iter().enumerate() {
        ctx.checkpoints.checkpoint_loop(index)?;
        widths_by_id.insert(id, bounds.width);
    }
    Ok(widths_by_id)
}

pub(super) fn is_block_start(message: &SequenceMessage) -> bool {
    is_block_role(message, SequenceControlRole::Start)
}

pub(super) fn is_block_section(message: &SequenceMessage) -> bool {
    is_block_role(message, SequenceControlRole::Separator)
}

pub(super) fn is_block_end(message: &SequenceMessage) -> bool {
    is_block_role(message, SequenceControlRole::End)
}

fn is_block_label_directive(message: &SequenceMessage) -> bool {
    is_block_start(message) || is_block_section(message)
}

fn is_block_role(message: &SequenceMessage, role: SequenceControlRole) -> bool {
    message.control_semantics().is_some_and(|semantics| {
        semantics.kind != SequenceControlKind::Rect && semantics.role == role
    })
}

fn block_label_step(
    raw_label: &str,
    frame_width: Option<f64>,
    frame_ctx: BlockFrameWidthContext<'_>,
    step_ctx: BlockStepContext,
) -> Result<f64> {
    if raw_label.trim().is_empty() {
        return Ok(step_ctx.block_base_step_empty);
    }

    let label = bracketize_sequence_block_label(raw_label);
    if let Some((_, height)) = measure_sequence_math_label(
        frame_ctx.measurer,
        &label,
        frame_ctx.msg_text_style,
        frame_ctx.math_config,
        frame_ctx.math_renderer,
        SequenceMathHeightMode::Bound,
        frame_ctx.checkpoints.text(),
    )? {
        return Ok(step_ctx.block_base_step_empty + height.max(step_ctx.label_box_height));
    }

    let measured_label = match frame_width {
        Some(width) => wrap_sequence_label_like_mermaid_lines(
            &label,
            frame_ctx.measurer,
            frame_ctx.msg_text_style,
            sequence_block_label_wrap_width(width, step_ctx.wrap_padding),
            frame_ctx.checkpoints.text(),
        )?
        .join("<br/>"),
        None => label,
    };
    let (_, height) = measure_svg_like_with_html_br(
        frame_ctx.measurer,
        &measured_label,
        frame_ctx.msg_text_style,
        frame_ctx.checkpoints.text(),
    )?;
    Ok(step_ctx.block_base_step_empty + height.max(step_ctx.label_box_height))
}

#[derive(Clone, Copy)]
struct BlockFrameWidthContext<'a> {
    actor_index: &'a HashMap<&'a str, usize>,
    actor_centers_x: &'a [f64],
    actor_widths: &'a [f64],
    actor_margin: f64,
    activation_width: f64,
    label_box_width: f64,
    sequence_default_width: f64,
    wrap_padding: f64,
    note_margin: f64,
    is_neo: bool,
    measurer: &'a dyn TextMeasurer,
    msg_text_style: &'a TextStyle,
    note_text_style: &'a TextStyle,
    math_config: &'a MermaidConfig,
    math_renderer: Option<&'a (dyn MathRenderer + Send + Sync)>,
    message_metrics: SequenceMessageMetricView<'a>,
    checkpoints: SequenceLayoutCheckpoints<'a>,
}

#[derive(Clone, Copy)]
struct BlockStepContext {
    block_base_step_empty: f64,
    label_box_height: f64,
    wrap_padding: f64,
}

impl<'a> BlockStepPlanContext<'a> {
    fn frame_width_context(self) -> BlockFrameWidthContext<'a> {
        BlockFrameWidthContext {
            actor_index: self.actor_index,
            actor_centers_x: self.actor_centers_x,
            actor_widths: self.actor_widths,
            actor_margin: self.actor_margin,
            activation_width: self.activation_width,
            label_box_width: self.label_box_width,
            sequence_default_width: self.sequence_default_width,
            wrap_padding: self.wrap_padding,
            note_margin: self.note_margin,
            is_neo: self.is_neo,
            measurer: self.measurer,
            msg_text_style: self.msg_text_style,
            note_text_style: self.note_text_style,
            math_config: self.math_config,
            math_renderer: self.math_renderer,
            message_metrics: self.message_metrics,
            checkpoints: self.checkpoints,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
struct BlockHorizontalBounds {
    from: f64,
    to: f64,
    width: f64,
}

impl BlockHorizontalBounds {
    fn empty() -> Self {
        Self {
            from: f64::INFINITY,
            to: f64::NEG_INFINITY,
            width: 0.0,
        }
    }
}

#[derive(Debug, Clone, Copy)]
struct BlockBoundsSummary {
    // For input bounds (F, T, W), width becomes:
    // max(W + p, T - F + r, T + t, -F + f, c).
    min_from: f64,
    max_to: f64,
    prior_width_offset: f64,
    range_width_offset: f64,
    max_to_width_offset: f64,
    neg_min_from_width_offset: f64,
    constant_width: f64,
}

impl BlockBoundsSummary {
    fn identity() -> Self {
        Self {
            min_from: f64::INFINITY,
            max_to: f64::NEG_INFINITY,
            prior_width_offset: 0.0,
            range_width_offset: f64::NEG_INFINITY,
            max_to_width_offset: f64::NEG_INFINITY,
            neg_min_from_width_offset: f64::NEG_INFINITY,
            constant_width: f64::NEG_INFINITY,
        }
    }

    fn fixed_width(from: f64, to: f64, width: f64, label_box_width: f64) -> Self {
        Self {
            min_from: from,
            max_to: to,
            prior_width_offset: -label_box_width,
            range_width_offset: f64::NEG_INFINITY,
            max_to_width_offset: f64::NEG_INFINITY,
            neg_min_from_width_offset: f64::NEG_INFINITY,
            constant_width: width - label_box_width,
        }
    }

    fn envelope_width(from: f64, to: f64, label_box_width: f64) -> Self {
        Self {
            min_from: from,
            max_to: to,
            prior_width_offset: -label_box_width,
            range_width_offset: -label_box_width,
            max_to_width_offset: -from - label_box_width,
            neg_min_from_width_offset: to - label_box_width,
            constant_width: to - from - label_box_width,
        }
    }

    fn then(self, next: Self) -> Self {
        Self {
            min_from: self.min_from.min(next.min_from),
            max_to: self.max_to.max(next.max_to),
            prior_width_offset: self.prior_width_offset + next.prior_width_offset,
            range_width_offset: (self.range_width_offset + next.prior_width_offset)
                .max(next.range_width_offset),
            max_to_width_offset: (self.max_to_width_offset + next.prior_width_offset)
                .max(next.range_width_offset - self.min_from)
                .max(next.max_to_width_offset),
            neg_min_from_width_offset: (self.neg_min_from_width_offset + next.prior_width_offset)
                .max(next.range_width_offset + self.max_to)
                .max(next.neg_min_from_width_offset),
            constant_width: (self.constant_width + next.prior_width_offset)
                .max(next.range_width_offset + self.max_to - self.min_from)
                .max(next.max_to_width_offset + self.max_to)
                .max(next.neg_min_from_width_offset - self.min_from)
                .max(next.constant_width),
        }
    }

    fn resolve(self) -> BlockHorizontalBounds {
        if self.min_from == f64::INFINITY && self.max_to == f64::NEG_INFINITY {
            return BlockHorizontalBounds::empty();
        }

        // Every block summary is ultimately evaluated from the empty (F, T, W) state.
        let width = self.prior_width_offset.max(self.constant_width);
        BlockHorizontalBounds {
            from: self.min_from,
            to: self.max_to,
            width,
        }
    }
}

fn message_bounds_summary(
    msg: &SequenceMessage,
    model: SequenceMessageHorizontalModel,
    ctx: BlockFrameWidthContext<'_>,
) -> Option<BlockBoundsSummary> {
    if model.start_x == model.stop_x {
        let actor_id = msg.from.as_deref()?;
        let actor_index = ctx.actor_index.get(actor_id).copied()?;
        let actor_center_x = ctx.actor_centers_x.get(actor_index).copied()?;
        let actor_width = ctx.actor_widths.get(actor_index).copied()?;
        let actor_left_x = actor_center_x - actor_width / 2.0;
        let from = (actor_left_x - model.width / 2.0).min(actor_left_x - actor_width / 2.0);
        let to = (actor_left_x + model.width / 2.0).max(actor_left_x + actor_width / 2.0);
        Some(BlockBoundsSummary::envelope_width(
            from,
            to,
            ctx.label_box_width,
        ))
    } else {
        Some(BlockBoundsSummary::fixed_width(
            model.start_x,
            model.stop_x,
            model.width,
            ctx.label_box_width,
        ))
    }
}

#[derive(Debug)]
struct OpenBlock {
    aliases: Vec<String>,
    summary: BlockBoundsSummary,
}

impl OpenBlock {
    fn new(id: String) -> Self {
        Self {
            aliases: vec![id],
            summary: BlockBoundsSummary::identity(),
        }
    }

    fn include(&mut self, summary: BlockBoundsSummary) {
        self.summary = self.summary.then(summary);
    }
}

fn calculate_sequence_block_bounds(
    messages: &[SequenceMessage],
    ctx: BlockFrameWidthContext<'_>,
    checkpoints: SequenceLayoutCheckpoints<'_>,
) -> Result<HashMap<String, BlockHorizontalBounds>> {
    let mut completed = HashMap::new();
    let mut stack: Vec<OpenBlock> = Vec::new();
    let mut activation_state = SequenceActivationState::new(ctx.activation_width);

    for (message_index, msg) in messages.iter().enumerate() {
        checkpoints.checkpoint_loop(message_index)?;
        if is_block_start(msg) {
            stack.push(OpenBlock::new(msg.id.clone()));
            continue;
        }
        if is_block_section(msg) {
            if !msg.message_text().is_empty()
                && let Some(current) = stack.last_mut()
            {
                current.aliases.push(msg.id.clone());
            }
            continue;
        }
        if is_block_end(msg) {
            if let Some(current) = stack.pop() {
                if let Some(parent) = stack.last_mut() {
                    parent.include(current.summary);
                }
                let bounds = current.summary.resolve();
                for (alias_index, alias) in current.aliases.into_iter().enumerate() {
                    checkpoints.checkpoint_loop(alias_index)?;
                    completed.insert(alias, bounds);
                }
            }
            continue;
        }
        if activation_state.handle_directive(msg, ctx.actor_index, ctx.actor_centers_x) {
            continue;
        }
        let Some(current) = stack.last_mut() else {
            continue;
        };

        if msg.placement.is_some() {
            let Some(note) = sequence_note_horizontal_model(
                msg,
                SequenceNoteHorizontalContext {
                    actor_index: ctx.actor_index,
                    actor_centers_x: ctx.actor_centers_x,
                    actor_widths: ctx.actor_widths,
                    actor_margin: ctx.actor_margin,
                    note_default_width: ctx.sequence_default_width,
                    note_margin: ctx.note_margin,
                    wrap_padding: ctx.wrap_padding,
                    measurer: ctx.measurer,
                    note_text_style: ctx.note_text_style,
                    math_config: ctx.math_config,
                    math_renderer: ctx.math_renderer,
                    checkpoints,
                },
            )?
            else {
                continue;
            };
            current.include(BlockBoundsSummary::envelope_width(
                note.start_x,
                note.start_x + note.width,
                ctx.label_box_width,
            ));
            continue;
        }

        let Some(message) = sequence_message_horizontal_model(
            msg,
            SequenceMessageHorizontalContext {
                actor_index: ctx.actor_index,
                actor_centers_x: ctx.actor_centers_x,
                activation_state: &activation_state,
                default_width: ctx.sequence_default_width,
                wrap_padding: ctx.wrap_padding,
                is_neo: ctx.is_neo,
                measurer: ctx.measurer,
                msg_text_style: ctx.msg_text_style,
                premeasured_bound: ctx
                    .message_metrics
                    .get(SequenceMessageOwner::from_model_index(message_index), msg),
                checkpoints,
            },
        )?
        else {
            continue;
        };
        if let Some(summary) = message_bounds_summary(msg, message, ctx) {
            current.include(summary);
        }
    }

    checkpoints.checkpoint()?;
    Ok(completed)
}

#[cfg(test)]
mod tests {
    use super::{
        BlockBoundsSummary, BlockFrameWidthContext, BlockHorizontalBounds, BlockStepContext,
        SequenceMessageMetricView, block_label_step, calculate_sequence_block_bounds,
    };
    use crate::resources::{OperationWorkMeter, RenderResourcePolicy};
    use crate::text::{DeterministicTextMeasurer, TextMeasurer, TextMetrics, TextStyle};
    use merman_core::MermaidConfig;
    use merman_core::diagrams::sequence::{SequenceMessage, SequenceMessagePayload};
    use std::collections::HashMap;

    struct ExpectedBlockLabelMeasurer;

    impl TextMeasurer for ExpectedBlockLabelMeasurer {
        fn measure(&self, text: &str, _style: &TextStyle) -> TextMetrics {
            assert_eq!(text, "[[Action 1]]");
            TextMetrics {
                width: 100.0,
                height: 16.0,
                line_count: 1,
            }
        }
    }

    fn message(
        id: &str,
        message_type: i32,
        from: Option<&str>,
        to: Option<&str>,
        text: &str,
    ) -> SequenceMessage {
        SequenceMessage {
            id: id.to_string(),
            from: from.map(str::to_string),
            to: to.map(str::to_string),
            message_type,
            message: SequenceMessagePayload::Text(text.to_string()),
            wrap: false,
            activate: false,
            placement: (message_type == 2).then_some(1),
            central_connection: 0,
        }
    }

    fn bounds(messages: &[SequenceMessage]) -> HashMap<String, super::BlockHorizontalBounds> {
        let actor_index = HashMap::from([("A", 0), ("B", 1), ("C", 2)]);
        let centers = [100.0, 300.0, 500.0];
        let widths = [80.0, 80.0, 80.0];
        let measurer = DeterministicTextMeasurer::default();
        let msg_style = TextStyle::default();
        let note_style = TextStyle::default();
        let math_config = MermaidConfig::default();
        let work_meter =
            OperationWorkMeter::new(RenderResourcePolicy::unbounded_for_trusted_input());
        let checkpoints = crate::sequence::SequenceLayoutCheckpoints::new(&work_meter);

        calculate_sequence_block_bounds(
            messages,
            BlockFrameWidthContext {
                actor_index: &actor_index,
                actor_centers_x: &centers,
                actor_widths: &widths,
                actor_margin: 40.0,
                activation_width: 10.0,
                label_box_width: 0.0,
                sequence_default_width: 0.0,
                wrap_padding: 0.0,
                note_margin: 0.0,
                is_neo: false,
                measurer: &measurer,
                msg_text_style: &msg_style,
                note_text_style: &note_style,
                math_config: &math_config,
                math_renderer: None,
                message_metrics: SequenceMessageMetricView::empty(),
                checkpoints,
            },
            checkpoints,
        )
        .unwrap()
    }

    #[test]
    fn block_bounds_summaries_preserve_ordered_width_updates() {
        let fixed_updates = [
            (300.0, 100.0, 240.0),
            (450.0, 50.0, 400.0),
            (100.0, 500.0, 400.0),
        ];
        let envelope_updates = [(80.0, 200.0), (40.0, 180.0), (250.0, 650.0)];

        for label_box_width in [0.0, 7.0, 50.0] {
            for envelope_mask in 0..8 {
                let mut reference = BlockHorizontalBounds::empty();
                let mut summaries = [BlockBoundsSummary::identity(); 3];

                for index in 0..3 {
                    if envelope_mask & (1 << index) == 0 {
                        let (from, to, width) = fixed_updates[index];
                        reference.from = reference.from.min(from);
                        reference.to = reference.to.max(to);
                        reference.width = reference.width.max(width) - label_box_width;
                        summaries[index] =
                            BlockBoundsSummary::fixed_width(from, to, width, label_box_width);
                    } else {
                        let (from, to) = envelope_updates[index];
                        reference.from = reference.from.min(from);
                        reference.to = reference.to.max(to);
                        reference.width =
                            reference.width.max((reference.to - reference.from).abs())
                                - label_box_width;
                        summaries[index] =
                            BlockBoundsSummary::envelope_width(from, to, label_box_width);
                    }
                }

                let composed = summaries
                    .into_iter()
                    .fold(BlockBoundsSummary::identity(), BlockBoundsSummary::then)
                    .resolve();
                let nested = summaries[0].then(summaries[1].then(summaries[2])).resolve();

                assert_eq!(composed, reference);
                assert_eq!(nested, reference);
            }
        }
    }

    #[test]
    fn activation_state_crosses_nested_block_boundaries() {
        let messages = vec![
            message("active", 17, Some("A"), Some("A"), ""),
            message("outer", 10, None, None, "outer"),
            message("inner", 15, None, None, "inner"),
            message("inner-end", 16, None, None, ""),
            message("edge", 5, Some("A"), Some("B"), ""),
            message("outer-end", 11, None, None, ""),
            message("inactive", 18, Some("A"), Some("A"), ""),
        ];

        let calculated = bounds(&messages);
        let outer = &calculated["outer"];

        assert_eq!((outer.from, outer.to, outer.width), (105.0, 299.0, 194.0));
        assert_eq!(calculated["inner"].width, 0.0);
    }

    #[test]
    fn ordinary_message_updates_bounds_after_self_message_and_note() {
        let messages = vec![
            message("loop", 10, None, None, "loop"),
            message("self", 5, Some("A"), Some("A"), ""),
            message("note", 2, Some("A"), Some("A"), ""),
            message("edge", 5, Some("B"), Some("C"), ""),
            message("end", 11, None, None, ""),
        ];

        let calculated = bounds(&messages);
        let loop_bounds = &calculated["loop"];

        assert_eq!(loop_bounds.from, 20.0);
        assert_eq!(loop_bounds.to, 499.0);
        assert_eq!(loop_bounds.width, 198.0);
    }

    #[test]
    fn ordinary_message_bounds_survive_later_self_message_and_note() {
        let messages = vec![
            message("loop", 10, None, None, "loop"),
            message("far", 5, Some("A"), Some("C"), ""),
            message("self", 5, Some("B"), Some("B"), ""),
            message("note", 2, Some("B"), Some("B"), ""),
            message("end", 11, None, None, ""),
        ];

        let calculated = bounds(&messages);
        let loop_bounds = &calculated["loop"];

        assert_eq!(loop_bounds.from, 101.0);
        assert_eq!(loop_bounds.to, 499.0);
        assert_eq!(loop_bounds.width, 398.0);
    }

    #[test]
    fn nested_blocks_and_section_aliases_share_final_width() {
        let messages = vec![
            message("outer", 10, None, None, "outer"),
            message("alt", 12, None, None, "first"),
            message("near", 5, Some("A"), Some("B"), ""),
            message("else", 13, None, None, "otherwise"),
            message("far", 5, Some("A"), Some("C"), ""),
            message("alt-end", 14, None, None, ""),
            message("outer-end", 11, None, None, ""),
        ];

        let calculated = bounds(&messages);

        assert_eq!(calculated["alt"], calculated["else"]);
        assert_eq!(calculated["alt"].width, 398.0);
        assert_eq!(calculated["outer"].width, 398.0);
    }

    #[test]
    fn empty_block_keeps_zero_width() {
        let messages = vec![
            message("empty", 10, None, None, "empty"),
            message("end", 11, None, None, ""),
        ];

        assert_eq!(bounds(&messages)["empty"].width, 0.0);
    }

    #[test]
    fn layout_measures_an_added_bracket_pair_around_a_bracketed_source_title() {
        let actor_index: HashMap<&str, usize> = HashMap::new();
        let actor_centers_x = [];
        let actor_widths = [];
        let measurer = ExpectedBlockLabelMeasurer;
        let text_style = TextStyle::default();
        let math_config = MermaidConfig::default();
        let work_meter =
            OperationWorkMeter::new(RenderResourcePolicy::unbounded_for_trusted_input());
        let checkpoints = crate::sequence::SequenceLayoutCheckpoints::new(&work_meter);

        let step = block_label_step(
            "[Action 1]",
            None,
            BlockFrameWidthContext {
                actor_index: &actor_index,
                actor_centers_x: &actor_centers_x,
                actor_widths: &actor_widths,
                actor_margin: 0.0,
                activation_width: 0.0,
                label_box_width: 0.0,
                sequence_default_width: 0.0,
                wrap_padding: 0.0,
                note_margin: 0.0,
                is_neo: false,
                measurer: &measurer,
                msg_text_style: &text_style,
                note_text_style: &text_style,
                math_config: &math_config,
                math_renderer: None,
                message_metrics: SequenceMessageMetricView::empty(),
                checkpoints,
            },
            BlockStepContext {
                block_base_step_empty: 0.0,
                label_box_height: 0.0,
                wrap_padding: 0.0,
            },
        )
        .unwrap();

        assert_eq!(step, 16.0);
    }
}
