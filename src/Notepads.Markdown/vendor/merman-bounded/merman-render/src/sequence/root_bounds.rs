use super::SequenceLayoutCheckpoints;
use super::constants::{sequence_actor_popup_panel_height, sequence_actor_popup_rect_height};
use super::message_metrics::{SequenceMessageMetricView, SequenceMessageOwner};
use super::metrics::{SequenceMathHeightMode, measure_sequence_label_for_layout};
use crate::Result;
use crate::math::MathRenderer;
use crate::model::{Bounds, LayoutEdge, LayoutNode};
use crate::text::{TextMeasurer, TextStyle};
use merman_core::MermaidConfig;
use merman_core::diagrams::sequence::{SequenceDiagramRenderModel, SequenceMessageKind};
use std::collections::HashMap;

pub(super) struct SequenceRootBoundsContext<'a> {
    pub(super) model: &'a SequenceDiagramRenderModel,
    pub(super) diagram_title: Option<&'a str>,
    pub(super) nodes: &'a [LayoutNode],
    pub(super) edges: &'a [LayoutEdge],
    pub(super) bounds_start_x: f64,
    pub(super) bounds_stop_x: f64,
    pub(super) actor_index: &'a HashMap<&'a str, usize>,
    pub(super) actor_centers_x: &'a [f64],
    pub(super) box_layouts: &'a [super::SequenceBoxLayout],
    pub(super) actor_width_min: f64,
    pub(super) actor_height: f64,
    pub(super) bottom_box_top_y: f64,
    pub(super) diagram_margin_x: f64,
    pub(super) diagram_margin_y: f64,
    pub(super) bottom_margin_adj: f64,
    pub(super) box_margin: f64,
    pub(super) wrap_padding: f64,
    pub(super) has_boxes: bool,
    pub(super) mirror_actors: bool,
    pub(super) measurer: &'a dyn TextMeasurer,
    pub(super) actor_text_style: &'a TextStyle,
    pub(super) msg_text_style: &'a TextStyle,
    pub(super) math_config: &'a MermaidConfig,
    pub(super) math_renderer: Option<&'a (dyn MathRenderer + Send + Sync)>,
    pub(super) message_metrics: SequenceMessageMetricView<'a>,
    pub(super) checkpoints: SequenceLayoutCheckpoints<'a>,
}

pub(super) fn sequence_root_bounds(ctx: SequenceRootBoundsContext<'_>) -> Result<Bounds> {
    let mut content = sequence_content_bounds(&ctx)?;

    include_actor_popup_bottoms(&mut content, &ctx)?;
    include_actor_popup_widths(&mut content, &ctx)?;

    // Mermaid (11.12.2) expands the viewBox vertically when a sequence title is present.
    // See `sequenceRenderer.ts`: `extraVertForTitle = title ? 40 : 0`.
    let extra_vert_for_title =
        if super::sequence_render_title(ctx.model.title.as_deref(), ctx.diagram_title).is_some() {
            40.0
        } else {
            0.0
        };

    // Mermaid's sequence renderer sets the viewBox y origin to
    // `-(diagramMarginY + extraVertForTitle)` regardless of diagram contents.
    let vb_min_y = -(ctx.diagram_margin_y + extra_vert_for_title);

    // Mermaid's sequence renderer uses a bounds box with `starty = 0` and computes `height` from
    // `stopy - starty`. Our headless layout models message spacing in content coordinates, but for
    // viewBox parity we must follow the upstream formula.
    let mut bounds_box_stopy = if ctx.mirror_actors {
        content.max_y + ctx.bottom_margin_adj
    } else {
        content.max_y
    }
    .max(0.0);

    // When boxes exist, Mermaid's bounds logic extends the vertical bounds by `boxMargin`
    // (diagramMarginY covers the remaining box padding), so include it here.
    if ctx.has_boxes {
        bounds_box_stopy += ctx.box_margin;
    }

    let mut bounds_box = ActorHorizontalBounds::from_content(ctx.bounds_start_x, ctx.bounds_stop_x);
    bounds_box.include(content.min_x, content.max_x);
    bounds_box.include_actor_boxes(&ctx)?;
    include_self_message_bounds(&mut bounds_box, &ctx)?;
    ctx.checkpoints.checkpoint()?;

    Ok(Bounds {
        min_x: bounds_box.start_x - ctx.diagram_margin_x,
        min_y: vb_min_y,
        max_x: bounds_box.stop_x + ctx.diagram_margin_x,
        max_y: bounds_box_stopy + ctx.diagram_margin_y,
    })
}

fn sequence_content_bounds(ctx: &SequenceRootBoundsContext<'_>) -> Result<ContentBounds> {
    let mut content = ContentBounds::new();

    for (node_index, n) in ctx.nodes.iter().enumerate() {
        ctx.checkpoints.checkpoint_loop(node_index)?;
        let left = n.x - n.width / 2.0;
        let right = n.x + n.width / 2.0;
        let bottom = n.y + n.height / 2.0;
        content.include_x(left, right);
        // When `mirrorActors=false`, Mermaid does not draw footer actor boxes. The internal
        // bottom actor placeholders still anchor lifelines in our layout, but upstream root
        // sizing ignores those invisible placeholders and uses message/popup geometry instead.
        if ctx.mirror_actors || !n.id.starts_with("actor-bottom-") {
            content.include_y(bottom);
        }
    }

    include_footer_row_height(&mut content, ctx)?;

    if !ctx.mirror_actors {
        for (edge_index, e) in ctx.edges.iter().enumerate() {
            ctx.checkpoints.checkpoint_loop(edge_index)?;
            if e.id.starts_with("lifeline-") {
                continue;
            }
            for (point_index, p) in e.points.iter().enumerate() {
                ctx.checkpoints.checkpoint_loop(point_index)?;
                content.include_y(p.y);
            }
            if let Some(label) = e.label.as_ref() {
                let margin = crate::config::config_f64(
                    ctx.math_config.as_value(),
                    &["sequence", "wrapPadding"],
                )
                .unwrap_or(10.0);
                let first_y = super::sequence_drawn_text_y(label.y, margin, 0.0);
                content.include_y(first_y + label.height / 2.0);
            }
        }
    }

    Ok(content.or_fallback(
        ctx.actor_width_min.max(1.0),
        (ctx.bottom_box_top_y + ctx.actor_height).max(1.0),
    ))
}

fn include_footer_row_height(
    content: &mut ContentBounds,
    ctx: &SequenceRootBoundsContext<'_>,
) -> Result<()> {
    if !ctx.mirror_actors {
        return Ok(());
    }

    // Mermaid's footer draw pass bumps the shared bounds cursor by the maximum rendered actor
    // height for the whole footer row, even when some actors were destroyed earlier and have their
    // own `stopy`. The root viewport follows that cursor, not just each individual footer node.
    let mut max_footer_height = 0.0_f64;
    for (node_index, node) in ctx.nodes.iter().enumerate() {
        ctx.checkpoints.checkpoint_loop(node_index)?;
        if node.id.starts_with("actor-bottom-") {
            max_footer_height = max_footer_height.max(node.height);
        }
    }
    if max_footer_height > 0.0 {
        content.include_y(ctx.bottom_box_top_y + max_footer_height);
    }
    Ok(())
}

fn include_actor_popup_widths(
    content: &mut ContentBounds,
    ctx: &SequenceRootBoundsContext<'_>,
) -> Result<()> {
    let node_prefix = if ctx.mirror_actors {
        "actor-bottom-"
    } else {
        "actor-top-"
    };
    for (node_index, node) in ctx.nodes.iter().enumerate() {
        ctx.checkpoints.checkpoint_loop(node_index)?;
        let Some(actor_id) = node.id.strip_prefix(node_prefix) else {
            continue;
        };
        let Some(actor) = ctx.model.actors.get(actor_id) else {
            continue;
        };
        if actor.links.is_empty() {
            continue;
        }
        let min_width = crate::sequence::sequence_actor_popup_min_width(
            actor,
            ctx.measurer,
            ctx.actor_text_style,
            ctx.wrap_padding,
            ctx.box_margin,
        );
        let panel_width = node.width.max(min_width);
        let left = node.x - node.width / 2.0;
        content.include_x(left, left + panel_width);
    }
    Ok(())
}

fn include_actor_popup_bottoms(
    content: &mut ContentBounds,
    ctx: &SequenceRootBoundsContext<'_>,
) -> Result<()> {
    // Mermaid's root `getBBox()` still includes actor popup menu panels when links/directives are
    // present, even when they are emitted hidden by default. Account for the menu panel bottom so
    // root height stays aligned with upstream for link-only fixtures.
    let is_neo = crate::config::config_diagram_look(ctx.math_config.as_value()).is_neo();
    let node_prefix = if ctx.mirror_actors {
        "actor-bottom-"
    } else {
        "actor-top-"
    };
    for (node_index, node) in ctx.nodes.iter().enumerate() {
        ctx.checkpoints.checkpoint_loop(node_index)?;
        let Some(actor_id) = node.id.strip_prefix(node_prefix) else {
            continue;
        };
        let Some(actor) = ctx.model.actors.get(actor_id) else {
            continue;
        };
        if actor.links.is_empty() {
            continue;
        }
        let popup_bottom = sequence_actor_popup_rect_height(
            &actor.actor_type,
            node.height,
            ctx.actor_height,
            is_neo,
            ctx.mirror_actors,
        ) + sequence_actor_popup_panel_height(actor.links.len());
        let popup_content_bottom = if ctx.mirror_actors {
            popup_bottom - ctx.diagram_margin_y - if ctx.has_boxes { ctx.box_margin } else { 0.0 }
        } else {
            popup_bottom
        };
        content.include_y(popup_content_bottom.max(0.0));
    }
    Ok(())
}

fn include_self_message_bounds(
    bounds_box: &mut ActorHorizontalBounds,
    ctx: &SequenceRootBoundsContext<'_>,
) -> Result<()> {
    // Mermaid's self-message bounds insert expands horizontally by
    // `dx = max(textWidth/2, conf.width/2)`, where `conf.width` is the configured actor width
    // (150 by default). This can increase `box.stopx` by ~1px due to `from_x + 1` rounding
    // behavior in message geometry, affecting viewBox width.
    for (message_index, msg) in ctx.model.messages.iter().enumerate() {
        ctx.checkpoints.checkpoint_loop(message_index)?;
        let (Some(from), Some(to)) = (msg.from.as_deref(), msg.to.as_deref()) else {
            continue;
        };
        if from != to {
            continue;
        }
        // Notes can use `from==to` for `rightOf`/`leftOf`; ignore them here.
        if msg.semantic_kind() == SequenceMessageKind::Note {
            continue;
        }
        let Some(&i) = ctx.actor_index.get(from) else {
            continue;
        };
        let center_x = ctx.actor_centers_x[i] + 1.0;
        let text = msg.message_text();
        let premeasured_bound = ctx
            .message_metrics
            .get(SequenceMessageOwner::from_model_index(message_index), msg);
        let (text_w, _text_h) = if text.is_empty() {
            (1.0, 1.0)
        } else if let Some(metrics) = premeasured_bound {
            (metrics.width(), 0.0)
        } else {
            measure_sequence_label_for_layout(
                ctx.measurer,
                text,
                ctx.msg_text_style,
                ctx.math_config,
                ctx.math_renderer,
                SequenceMathHeightMode::Bound,
                ctx.checkpoints.text(),
            )?
        };
        let dx = (text_w.max(1.0) / 2.0).max(ctx.actor_width_min / 2.0);
        bounds_box.include(center_x - dx, center_x + dx);
    }
    Ok(())
}

#[derive(Clone, Copy)]
struct ContentBounds {
    min_x: f64,
    max_x: f64,
    max_y: f64,
}

impl ContentBounds {
    fn new() -> Self {
        Self {
            min_x: f64::INFINITY,
            max_x: f64::NEG_INFINITY,
            max_y: f64::NEG_INFINITY,
        }
    }

    fn include_x(&mut self, left: f64, right: f64) {
        self.min_x = self.min_x.min(left);
        self.max_x = self.max_x.max(right);
    }

    fn include_y(&mut self, y: f64) {
        self.max_y = self.max_y.max(y);
    }

    fn or_fallback(mut self, fallback_width: f64, fallback_max_y: f64) -> Self {
        if !self.min_x.is_finite() {
            self.min_x = 0.0;
            self.max_x = fallback_width;
            self.max_y = fallback_max_y;
        }
        self
    }
}

struct ActorHorizontalBounds {
    start_x: f64,
    stop_x: f64,
}

impl ActorHorizontalBounds {
    fn from_content(min_x: f64, max_x: f64) -> Self {
        Self {
            start_x: min_x,
            stop_x: max_x,
        }
    }

    fn include_actor_boxes(&mut self, ctx: &SequenceRootBoundsContext<'_>) -> Result<()> {
        // addActorRenderingData records box coordinates before created-actor spacing.
        for (index, box_layout) in ctx.box_layouts.iter().enumerate() {
            ctx.checkpoints.checkpoint_loop(index)?;
            if let Some(x) = box_layout.x {
                self.include(x, x + box_layout.width);
            }
        }
        Ok(())
    }

    fn include(&mut self, left: f64, right: f64) {
        self.start_x = self.start_x.min(left);
        self.stop_x = self.stop_x.max(right);
    }
}
