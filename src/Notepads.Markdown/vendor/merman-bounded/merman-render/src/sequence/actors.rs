use super::SequenceLayoutCheckpoints;
use super::constants::{
    sequence_actor_lifeline_start_y, sequence_actor_stack_height, sequence_actor_visual_height,
};
use super::message_metrics::{
    SequenceMessageBoundMetrics, SequenceMessageMetricSidecar, SequenceMessageOwner,
};
use super::metrics::{SequenceMathHeightMode, measure_sequence_label_for_layout};
use super::wrap_sequence_label_like_mermaid_lines;
use crate::math::MathRenderer;
use crate::model::{LayoutEdge, LayoutNode, LayoutPoint};
use crate::text::{TextMeasurer, TextStyle};
use crate::{Error, Result};
use merman_core::MermaidConfig;
use merman_core::diagrams::sequence::SequenceActor;
use merman_core::diagrams::sequence::SequenceDiagramRenderModel;
use std::collections::{BTreeMap, HashMap};

use super::metrics::measure_svg_like_with_html_br;
pub(super) struct SequenceActorLayoutPlanContext<'a> {
    pub(super) model: &'a SequenceDiagramRenderModel,
    pub(super) measurer: &'a dyn TextMeasurer,
    pub(super) actor_text_style: &'a TextStyle,
    pub(super) note_text_style: &'a TextStyle,
    pub(super) msg_text_style: &'a TextStyle,
    pub(super) math_config: &'a MermaidConfig,
    pub(super) math_renderer: Option<&'a (dyn MathRenderer + Send + Sync)>,
    pub(super) actor_width_min: f64,
    pub(super) actor_height: f64,
    pub(super) is_neo: bool,
    pub(super) actor_margin: f64,
    pub(super) box_margin: f64,
    pub(super) box_text_margin: f64,
    pub(super) wrap_padding: f64,
    pub(super) checkpoints: SequenceLayoutCheckpoints<'a>,
}

pub(super) struct SequenceActorLayoutPlan<'a> {
    pub(super) actor_index: HashMap<&'a str, usize>,
    pub(super) actor_widths: Vec<f64>,
    pub(super) actor_base_heights: Vec<f64>,
    pub(super) actor_text_heights: Vec<f64>,
    pub(super) actor_centers_x: Vec<f64>,
    pub(super) box_layouts: Vec<super::SequenceBoxLayout>,
    pub(super) box_title_height: f64,
    pub(super) actor_top_offset_y: f64,
    pub(super) max_actor_layout_height: f64,
    pub(super) has_boxes: bool,
    pub(super) message_metrics: SequenceMessageMetricSidecar,
}

pub(super) struct SequenceActorLifecycleContext<'a> {
    pub(super) model: &'a SequenceDiagramRenderModel,
    pub(super) actor_index: &'a HashMap<&'a str, usize>,
    pub(super) actor_base_heights: &'a [f64],
    pub(super) actor_height: f64,
    pub(super) checkpoints: SequenceLayoutCheckpoints<'a>,
}

pub(super) fn plan_sequence_actors<'a>(
    ctx: SequenceActorLayoutPlanContext<'a>,
) -> Result<SequenceActorLayoutPlan<'a>> {
    let has_boxes = !ctx.model.boxes.is_empty();

    if ctx.model.actor_order.is_empty() {
        return Err(Error::InvalidModel {
            message: "sequence model has no actorOrder".to_string(),
        });
    }

    let (actor_widths, mut actor_base_heights, actor_text_heights) = measure_actor_boxes(&ctx)?;
    let actor_index = actor_index(&ctx)?;
    let (actor_to_message_width, message_metrics) = actor_message_widths(&ctx, &actor_index)?;
    let actor_margins = actor_margins(&ctx, &actor_widths, &actor_to_message_width)?;
    let (mut box_layouts, box_title_height) = measure_boxes(
        &ctx,
        &actor_index,
        &actor_widths,
        &actor_margins,
        &actor_to_message_width,
    )?;
    let actor_top_offset_y = if has_boxes {
        ctx.box_margin + box_title_height
    } else {
        0.0
    };
    let actor_box = actor_box(&ctx, &actor_index)?;
    let actor_left_x = actor_left_x(
        &ctx,
        &actor_widths,
        &actor_margins,
        &actor_box,
        &mut box_layouts,
    )?;
    let actor_centers_x = actor_centers_x(&ctx, &actor_left_x, &actor_widths)?;
    let max_actor_layout_height = max_actor_layout_height(&ctx, &actor_base_heights)?;
    if ctx.is_neo {
        actor_base_heights.fill(max_actor_layout_height);
    }
    ctx.checkpoints.checkpoint()?;

    Ok(SequenceActorLayoutPlan {
        actor_index,
        actor_widths,
        actor_base_heights,
        actor_text_heights,
        actor_centers_x,
        box_layouts,
        box_title_height,
        actor_top_offset_y,
        max_actor_layout_height,
        has_boxes,
        message_metrics,
    })
}

fn measure_actor_boxes(
    ctx: &SequenceActorLayoutPlanContext<'_>,
) -> Result<(Vec<f64>, Vec<f64>, Vec<f64>)> {
    // Measure participant boxes.
    let mut actor_widths: Vec<f64> = Vec::with_capacity(ctx.model.actor_order.len());
    let mut actor_base_heights: Vec<f64> = Vec::with_capacity(ctx.model.actor_order.len());
    let mut actor_text_heights = Vec::with_capacity(ctx.model.actor_order.len());
    for (actor_position, id) in ctx.model.actor_order.iter().enumerate() {
        ctx.checkpoints.checkpoint_loop(actor_position)?;
        let a = ctx
            .model
            .actors
            .get(id)
            .ok_or_else(|| Error::InvalidModel {
                message: format!("missing actor {id}"),
            })?;
        let description = if a.wrap {
            // calculateActorMargins measures the wrapped description before sizing the row.
            let wrap_w = (ctx.actor_width_min - 2.0 * ctx.wrap_padding).max(1.0);
            let wrapped_lines = wrap_sequence_label_like_mermaid_lines(
                &a.description,
                ctx.measurer,
                ctx.actor_text_style,
                wrap_w,
                ctx.checkpoints.text(),
            )?;
            std::borrow::Cow::Owned(wrapped_lines.join("<br>"))
        } else {
            std::borrow::Cow::Borrowed(a.description.as_str())
        };
        let (text_w, text_h) = measure_sequence_label_for_layout(
            ctx.measurer,
            &description,
            ctx.actor_text_style,
            ctx.math_config,
            ctx.math_renderer,
            SequenceMathHeightMode::Actor,
            ctx.checkpoints.text(),
        )?;
        let width = if a.wrap {
            ctx.actor_width_min
        } else {
            (text_w + 2.0 * ctx.wrap_padding).max(ctx.actor_width_min)
        };
        let stack_height = if ctx.is_neo {
            sequence_actor_stack_height(text_h)
        } else if a.wrap {
            text_h
        } else {
            0.0
        };
        actor_text_heights.push(text_h);
        actor_base_heights.push(ctx.actor_height.max(stack_height).max(1.0));
        actor_widths.push(width.max(1.0));
    }
    Ok((actor_widths, actor_base_heights, actor_text_heights))
}

fn actor_index<'a>(ctx: &SequenceActorLayoutPlanContext<'a>) -> Result<HashMap<&'a str, usize>> {
    let mut actor_index: HashMap<&str, usize> = HashMap::new();
    for (i, id) in ctx.model.actor_order.iter().enumerate() {
        ctx.checkpoints.checkpoint_loop(i)?;
        actor_index.insert(id.as_str(), i);
    }
    Ok(actor_index)
}

fn actor_message_widths(
    ctx: &SequenceActorLayoutPlanContext<'_>,
    actor_index: &HashMap<&str, usize>,
) -> Result<(Vec<f64>, SequenceMessageMetricSidecar)> {
    let mut actor_to_message_width: Vec<f64> = vec![0.0; ctx.model.actor_order.len()];
    let mut message_metrics =
        SequenceMessageMetricSidecar::new(ctx.model, ctx.msg_text_style, ctx.measurer);
    for (message_index, msg) in ctx.model.messages.iter().enumerate() {
        ctx.checkpoints.checkpoint_loop(message_index)?;
        let (Some(from), Some(to)) = (msg.from.as_deref(), msg.to.as_deref()) else {
            continue;
        };
        let Some(&from_idx) = actor_index.get(from) else {
            continue;
        };
        let Some(&to_idx) = actor_index.get(to) else {
            continue;
        };

        let placement = msg.placement;
        // If this is the first actor, and the note is left of it, no need to calculate the margin.
        if placement == Some(0) && to_idx == 0 {
            continue;
        }
        // If this is the last actor, and the note is right of it, no need to calculate the margin.
        if placement == Some(1) && to_idx + 1 == ctx.model.actor_order.len() {
            continue;
        }

        let is_note = placement.is_some();
        let is_message = !is_note;
        let style = if is_note {
            ctx.note_text_style
        } else {
            ctx.msg_text_style
        };
        let text = msg.message_text();
        if text.is_empty() {
            continue;
        }

        let is_math = text.contains("$$");
        let (w0, h0) = if is_math {
            measure_sequence_label_for_layout(
                ctx.measurer,
                text,
                style,
                ctx.math_config,
                ctx.math_renderer,
                SequenceMathHeightMode::Bound,
                ctx.checkpoints.text(),
            )?
        } else {
            let measured_text = if msg.wrap {
                // Upstream uses `wrapLabel(message, conf.width - 2*wrapPadding, ...)` when
                // computing max per-actor message widths for spacing.
                let wrap_w = (ctx.actor_width_min - 2.0 * ctx.wrap_padding).max(1.0);
                let lines = wrap_sequence_label_like_mermaid_lines(
                    text,
                    ctx.measurer,
                    style,
                    wrap_w,
                    ctx.checkpoints.text(),
                )?;
                lines.join("<br>")
            } else {
                text.to_string()
            };
            measure_svg_like_with_html_br(
                ctx.measurer,
                &measured_text,
                style,
                ctx.checkpoints.text(),
            )?
        };
        if is_message && !msg.wrap && !is_math {
            // Final direct `<text>` drawing is a distinct DOM probe and deliberately does not use
            // this bound-metric sidecar.
            message_metrics.record(
                SequenceMessageOwner::from_model_index(message_index),
                SequenceMessageBoundMetrics::new(w0.max(0.0), h0.max(0.0)),
            );
        }
        let message_w = (w0 + 2.0 * ctx.wrap_padding).max(0.0);

        let prev_idx = if to_idx > 0 { Some(to_idx - 1) } else { None };
        let next_idx = if to_idx + 1 < ctx.model.actor_order.len() {
            Some(to_idx + 1)
        } else {
            None
        };

        if is_message && next_idx.is_some_and(|n| n == from_idx) {
            actor_to_message_width[to_idx] = actor_to_message_width[to_idx].max(message_w);
        } else if is_message && prev_idx.is_some_and(|p| p == from_idx) {
            actor_to_message_width[from_idx] = actor_to_message_width[from_idx].max(message_w);
        } else if is_message && from_idx == to_idx {
            let half = message_w / 2.0;
            actor_to_message_width[from_idx] = actor_to_message_width[from_idx].max(half);
            actor_to_message_width[to_idx] = actor_to_message_width[to_idx].max(half);
        } else if placement == Some(1) {
            // RIGHTOF
            actor_to_message_width[from_idx] = actor_to_message_width[from_idx].max(message_w);
        } else if placement == Some(0) {
            // LEFTOF
            if let Some(p) = prev_idx {
                actor_to_message_width[p] = actor_to_message_width[p].max(message_w);
            }
        } else if placement == Some(2) {
            // OVER
            if let Some(p) = prev_idx {
                actor_to_message_width[p] = actor_to_message_width[p].max(message_w / 2.0);
            }
            if next_idx.is_some() {
                actor_to_message_width[from_idx] =
                    actor_to_message_width[from_idx].max(message_w / 2.0);
            }
        }
    }
    Ok((actor_to_message_width, message_metrics))
}

fn actor_margins(
    ctx: &SequenceActorLayoutPlanContext<'_>,
    actor_widths: &[f64],
    actor_to_message_width: &[f64],
) -> Result<Vec<f64>> {
    let mut actor_margins: Vec<f64> = vec![ctx.actor_margin; actor_to_message_width.len()];
    for i in 0..actor_to_message_width.len() {
        ctx.checkpoints.checkpoint_loop(i)?;
        let msg_w = actor_to_message_width[i];
        if msg_w <= 0.0 {
            continue;
        }
        let w0 = actor_widths[i];
        let actor_w = if i + 1 < actor_to_message_width.len() {
            let w1 = actor_widths[i + 1];
            msg_w + ctx.actor_margin - (w0 / 2.0) - (w1 / 2.0)
        } else {
            msg_w + ctx.actor_margin - (w0 / 2.0)
        };
        actor_margins[i] = actor_w.max(ctx.actor_margin);
    }
    Ok(actor_margins)
}

fn measure_boxes(
    ctx: &SequenceActorLayoutPlanContext<'_>,
    actor_index: &HashMap<&str, usize>,
    actor_widths: &[f64],
    actor_margins: &[f64],
    actor_to_message_width: &[f64],
) -> Result<(Vec<super::SequenceBoxLayout>, f64)> {
    // Mermaid's `calculateActorMargins(...)` computes per-box `box.margin` based on total actor
    // widths/margins and the box title width. For totalWidth, Mermaid only counts `actor.margin`
    // if it was set (actors without messages have `margin === undefined` until render-time).
    let mut box_layouts = Vec::with_capacity(ctx.model.boxes.len());
    let mut max_title_height = 0.0_f64;
    let mut membership_index = 0usize;
    for (box_idx, b) in ctx.model.boxes.iter().enumerate() {
        ctx.checkpoints.checkpoint_loop(box_idx)?;
        let mut total_width = 0.0;
        for actor_key in &b.actor_keys {
            ctx.checkpoints.checkpoint_loop(membership_index)?;
            membership_index = membership_index.saturating_add(1);
            let Some(&i) = actor_index.get(actor_key.as_str()) else {
                continue;
            };
            let actor_margin_for_box = if actor_to_message_width[i] > 0.0 {
                actor_margins[i]
            } else {
                0.0
            };
            total_width += actor_widths[i] + actor_margin_for_box;
        }

        total_width += ctx.box_margin * 8.0;
        total_width -= 2.0 * ctx.box_text_margin;

        let mut layout = super::SequenceBoxLayout {
            label: b.name.clone(),
            margin: ctx.box_text_margin,
            x: None,
            width: 0.0,
        };
        let text_width = if let Some(name) = layout.label.as_mut() {
            if b.wrap {
                *name = wrap_sequence_label_like_mermaid_lines(
                    name,
                    ctx.measurer,
                    ctx.msg_text_style,
                    (total_width - 2.0 * ctx.wrap_padding).max(1.0),
                    ctx.checkpoints.text(),
                )?
                .join("<br>");
            }
            // Box titles always use calculateTextDimensions, even when they contain math.
            let (text_w, text_h) = measure_svg_like_with_html_br(
                ctx.measurer,
                name,
                ctx.msg_text_style,
                ctx.checkpoints.text(),
            )?;
            max_title_height = max_title_height.max(text_h);
            text_w
        } else {
            0.0
        };
        let min_width = total_width.max(text_width + 2.0 * ctx.wrap_padding);
        if total_width < min_width {
            layout.margin += (min_width - total_width) / 2.0;
        }
        box_layouts.push(layout);
    }
    Ok((box_layouts, max_title_height))
}

fn actor_box(
    ctx: &SequenceActorLayoutPlanContext<'_>,
    actor_index: &HashMap<&str, usize>,
) -> Result<Vec<Option<usize>>> {
    // Assign each actor to at most one box (Mermaid's db assigns a single `actor.box` reference).
    let mut actor_box: Vec<Option<usize>> = vec![None; ctx.model.actor_order.len()];
    let mut membership_index = 0usize;
    for (box_idx, b) in ctx.model.boxes.iter().enumerate() {
        ctx.checkpoints.checkpoint_loop(box_idx)?;
        for actor_key in &b.actor_keys {
            ctx.checkpoints.checkpoint_loop(membership_index)?;
            membership_index = membership_index.saturating_add(1);
            let Some(&i) = actor_index.get(actor_key.as_str()) else {
                continue;
            };
            actor_box[i] = Some(box_idx);
        }
    }
    Ok(actor_box)
}

fn actor_left_x(
    ctx: &SequenceActorLayoutPlanContext<'_>,
    actor_widths: &[f64],
    actor_margins: &[f64],
    actor_box: &[Option<usize>],
    box_layouts: &mut [super::SequenceBoxLayout],
) -> Result<Vec<f64>> {
    let mut actor_left_x: Vec<f64> = Vec::with_capacity(ctx.model.actor_order.len());
    let mut prev_width = 0.0;
    let mut prev_margin = 0.0;
    let mut prev_box: Option<usize> = None;
    for i in 0..ctx.model.actor_order.len() {
        ctx.checkpoints.checkpoint_loop(i)?;
        let w = actor_widths[i];
        let cur_box = actor_box[i];

        // end of box
        if prev_box.is_some()
            && prev_box != cur_box
            && let Some(prev) = prev_box
        {
            prev_margin += ctx.box_margin + box_layouts[prev].margin;
        }

        // new box
        if cur_box.is_some()
            && cur_box != prev_box
            && let Some(bi) = cur_box
        {
            box_layouts[bi].x = Some(prev_width + prev_margin);
            prev_margin += box_layouts[bi].margin;
        }

        // Mermaid widens the margin before a created actor by `actor.width / 2`.
        if ctx.model.created_actor_message_index_at(i).is_some() {
            prev_margin += w / 2.0;
        }
        let x = prev_width + prev_margin;
        actor_left_x.push(x);
        prev_width += w + prev_margin;
        if let Some(bi) = cur_box {
            let box_layout = &mut box_layouts[bi];
            box_layout.width = prev_width + box_layout.margin - box_layout.x.unwrap_or(0.0);
        }
        prev_margin = actor_margins[i];
        prev_box = cur_box;
    }
    Ok(actor_left_x)
}

fn actor_centers_x(
    ctx: &SequenceActorLayoutPlanContext<'_>,
    actor_left_x: &[f64],
    actor_widths: &[f64],
) -> Result<Vec<f64>> {
    let mut actor_centers_x: Vec<f64> = Vec::with_capacity(actor_left_x.len());
    for i in 0..actor_left_x.len() {
        ctx.checkpoints.checkpoint_loop(i)?;
        actor_centers_x.push(actor_left_x[i] + actor_widths[i] / 2.0);
    }
    Ok(actor_centers_x)
}

fn max_actor_layout_height(
    ctx: &SequenceActorLayoutPlanContext<'_>,
    actor_base_heights: &[f64],
) -> Result<f64> {
    let mut max_height = 0.0_f64;
    for (actor_position, height) in actor_base_heights.iter().copied().enumerate() {
        ctx.checkpoints.checkpoint_loop(actor_position)?;
        max_height = max_height.max(height);
    }
    Ok(max_height.max(1.0))
}

pub(super) struct SequenceActorLifecycle<'a> {
    ctx: SequenceActorLifecycleContext<'a>,
    created_top_center_y: BTreeMap<String, f64>,
    destroyed_bottom_top_y: BTreeMap<String, f64>,
}

pub(super) struct SequenceFooterActorContext<'a, 'b> {
    pub(super) actor_order: &'a [String],
    pub(super) actors: &'a BTreeMap<String, SequenceActor>,
    pub(super) actor_widths: &'a [f64],
    pub(super) actor_centers_x: &'a [f64],
    pub(super) actor_base_heights: &'a [f64],
    pub(super) actor_text_heights: &'a [f64],
    pub(super) is_neo: bool,
    pub(super) actor_lifecycle: &'b SequenceActorLifecycle<'a>,
    pub(super) actor_top_offset_y: f64,
    pub(super) bottom_box_top_y: f64,
    pub(super) mirror_actors: bool,
    pub(super) label_box_height: f64,
    pub(super) box_text_margin: f64,
    pub(super) checkpoints: SequenceLayoutCheckpoints<'a>,
}

pub(super) struct SequenceTopActorContext<'a> {
    pub(super) actor_order: &'a [String],
    pub(super) actors: &'a BTreeMap<String, SequenceActor>,
    pub(super) actor_widths: &'a [f64],
    pub(super) actor_centers_x: &'a [f64],
    pub(super) actor_base_heights: &'a [f64],
    pub(super) actor_text_heights: &'a [f64],
    pub(super) is_neo: bool,
    pub(super) actor_top_offset_y: f64,
    pub(super) label_box_height: f64,
    pub(super) checkpoints: SequenceLayoutCheckpoints<'a>,
}

impl<'a> SequenceActorLifecycle<'a> {
    pub(super) fn new(ctx: SequenceActorLifecycleContext<'a>) -> Self {
        Self {
            ctx,
            created_top_center_y: BTreeMap::new(),
            destroyed_bottom_top_y: BTreeMap::new(),
        }
    }

    pub(super) fn created_actor_index(&self, actor_id: &str) -> Option<usize> {
        let actor_index = self.ctx.actor_index.get(actor_id).copied()?;
        self.ctx.model.created_actor_message_index_at(actor_index)
    }

    pub(super) fn destroyed_actor_index(&self, actor_id: &str) -> Option<usize> {
        let actor_index = self.ctx.actor_index.get(actor_id).copied()?;
        self.ctx.model.destroyed_actor_message_index_at(actor_index)
    }

    pub(super) fn created_top_center_y(&self, actor_id: &str) -> Option<f64> {
        self.created_top_center_y.get(actor_id).copied()
    }

    pub(super) fn destroyed_bottom_top_y(&self, actor_id: &str) -> Option<f64> {
        self.destroyed_bottom_top_y.get(actor_id).copied()
    }

    pub(super) fn apply_message_y_adjustment(
        &mut self,
        msg_idx: usize,
        from: &str,
        to: &str,
        line_y: f64,
    ) -> f64 {
        // Mermaid updates created/destroyed actor vertical anchors while processing messages and
        // advances the cursor by half of the actor's pre-render layout height. Type-specific SVG
        // glyph drawing may later mutate the visual height, but that does not feed back into this
        // lifecycle cursor adjustment.
        if self.created_actor_index(to) == Some(msg_idx) {
            let h = self.actor_lifecycle_height(to);
            self.created_top_center_y.insert(to.to_string(), line_y);
            h / 2.0
        } else if self.destroyed_actor_index(from) == Some(msg_idx) {
            let h = self.actor_lifecycle_height(from);
            self.destroyed_bottom_top_y
                .insert(from.to_string(), line_y - h / 2.0);
            h / 2.0
        } else if self.destroyed_actor_index(to) == Some(msg_idx) {
            let h = self.actor_lifecycle_height(to);
            self.destroyed_bottom_top_y
                .insert(to.to_string(), line_y - h / 2.0);
            h / 2.0
        } else {
            0.0
        }
    }

    pub(super) fn apply_created_top_actor_positions(&self, nodes: &mut [LayoutNode]) -> Result<()> {
        // Created actors render from `lineStartY - actor.height / 2` in Mermaid's
        // `adjustCreatedDestroyedData(...)`. Type-specific drawing can use a taller visual node,
        // but that visual height does not move the creation anchor.
        for (node_index, node) in nodes.iter_mut().enumerate() {
            self.ctx.checkpoints.checkpoint_loop(node_index)?;
            let Some(actor_id) = node.id.strip_prefix("actor-top-") else {
                continue;
            };
            if let Some(y) = self.created_top_center_y(actor_id) {
                let h = self.actor_lifecycle_height(actor_id);
                node.y = y - h / 2.0 + node.height / 2.0;
            }
        }
        Ok(())
    }

    fn actor_lifecycle_height(&self, actor_id: &str) -> f64 {
        let Some(idx) = self.ctx.actor_index.get(actor_id).copied() else {
            return self.ctx.actor_height.max(1.0);
        };
        self.ctx
            .actor_base_heights
            .get(idx)
            .copied()
            .unwrap_or(self.ctx.actor_height)
            .max(1.0)
    }
}

pub(super) fn sequence_actor_is_type_width_limited(
    actors: &BTreeMap<String, SequenceActor>,
    actor_id: &str,
) -> bool {
    actors
        .get(actor_id)
        .map(|a| {
            matches!(
                a.actor_type.as_str(),
                "actor" | "control" | "entity" | "database"
            )
        })
        .unwrap_or(false)
}

pub(super) fn append_sequence_top_actors(
    nodes: &mut Vec<LayoutNode>,
    ctx: SequenceTopActorContext<'_>,
) -> Result<()> {
    for (idx, id) in ctx.actor_order.iter().enumerate() {
        ctx.checkpoints.checkpoint_loop(idx)?;
        let w = ctx.actor_widths[idx];
        let cx = ctx.actor_centers_x[idx];
        let base_h = ctx.actor_base_heights[idx];
        let actor_type = ctx
            .actors
            .get(id)
            .map(|a| a.actor_type.as_str())
            .unwrap_or("participant");
        let visual_h = if ctx.is_neo {
            base_h
        } else {
            sequence_actor_visual_height(actor_type, w, base_h, ctx.label_box_height)
        };
        let top_y = ctx.actor_top_offset_y + visual_h / 2.0;
        nodes.push(LayoutNode {
            id: format!("actor-top-{id}"),
            x: cx,
            y: top_y,
            width: w,
            height: visual_h,
            is_cluster: false,
            label_width: None,
            label_height: ctx.is_neo.then_some(ctx.actor_text_heights[idx]),
        });
    }
    Ok(())
}

pub(super) fn append_sequence_footer_actors(
    nodes: &mut Vec<LayoutNode>,
    edges: &mut Vec<LayoutEdge>,
    ctx: SequenceFooterActorContext<'_, '_>,
) -> Result<()> {
    for (idx, id) in ctx.actor_order.iter().enumerate() {
        ctx.checkpoints.checkpoint_loop(idx)?;
        let w = ctx.actor_widths[idx];
        let cx = ctx.actor_centers_x[idx];
        let base_h = ctx.actor_base_heights[idx];
        let actor_type = ctx
            .actors
            .get(id)
            .map(|a| a.actor_type.as_str())
            .unwrap_or("participant");
        let visual_h = if ctx.is_neo {
            base_h
        } else {
            sequence_actor_visual_height(actor_type, w, base_h, ctx.label_box_height)
        };
        let bottom_top_y = ctx
            .actor_lifecycle
            .destroyed_bottom_top_y(id)
            .unwrap_or(ctx.bottom_box_top_y);
        let bottom_visual_h = if ctx.mirror_actors { visual_h } else { 0.0 };
        nodes.push(LayoutNode {
            id: format!("actor-bottom-{id}"),
            x: cx,
            y: bottom_top_y + bottom_visual_h / 2.0,
            width: w,
            height: bottom_visual_h,
            is_cluster: false,
            label_width: None,
            label_height: ctx.is_neo.then_some(ctx.actor_text_heights[idx]),
        });

        let top_center_y = ctx
            .actor_lifecycle
            .created_top_center_y(id)
            .unwrap_or(ctx.actor_top_offset_y + visual_h / 2.0);
        let top_left_y = top_center_y - visual_h / 2.0;
        let lifeline_start_y = top_left_y
            + if ctx.is_neo {
                base_h
            } else {
                sequence_actor_lifeline_start_y(actor_type, base_h, ctx.box_text_margin)
            };

        edges.push(LayoutEdge {
            id: format!("lifeline-{id}"),
            from: format!("actor-top-{id}"),
            to: format!("actor-bottom-{id}"),
            from_cluster: None,
            to_cluster: None,
            points: vec![
                LayoutPoint {
                    x: cx,
                    y: lifeline_start_y,
                },
                LayoutPoint {
                    x: cx,
                    y: bottom_top_y,
                },
            ],
            label: None,
            start_label_left: None,
            start_label_right: None,
            end_label_left: None,
            end_label_right: None,
            start_marker: None,
            end_marker: None,
            stroke_dasharray: None,
        });
    }
    Ok(())
}
