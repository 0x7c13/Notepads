use super::SequenceLayoutCheckpoints;
use super::activation::SequenceActivationState;
use super::constants::SEQUENCE_MESSAGE_WRAP_PADDING_SIDES;
use super::message_metrics::SequenceMessageBoundMetrics;
use super::metrics::{
    SequenceDrawnTextNode, SequenceMathHeightMode, measure_drawn_svg_like_with_html_br,
    measure_sequence_label_for_layout, measure_svg_like_with_html_br,
};
use super::wrap_sequence_label_like_mermaid_lines;
use crate::Result;
use crate::math::MathRenderer;
use crate::model::{LayoutEdge, LayoutLabel, LayoutPoint};
use crate::text::{TextMeasurer, TextStyle, split_html_br_lines};
use merman_core::MermaidConfig;
use merman_core::diagrams::sequence::{
    SequenceCentralDecoration, SequenceMessage, SequenceMessageDirection, SequenceMessageMarker,
    SequenceMessageStroke, SequenceSignalSemantics,
};

const CENTRAL_CONNECTION_BASE_OFFSET: f64 = 4.0;
const CENTRAL_CONNECTION_BIDIRECTIONAL_OFFSET: f64 = 6.0;

#[derive(Debug, Clone, Copy, PartialEq)]
pub(super) struct SequenceMessageHorizontalModel {
    pub(super) start_x: f64,
    pub(super) stop_x: f64,
    pub(super) width: f64,
    pub(super) bounded_width: f64,
    pub(super) from_bound: f64,
    pub(super) to_bound: f64,
}

#[derive(Clone, Copy)]
pub(super) struct SequenceMessageHorizontalContext<'a> {
    pub(super) actor_index: &'a std::collections::HashMap<&'a str, usize>,
    pub(super) actor_centers_x: &'a [f64],
    pub(super) activation_state: &'a SequenceActivationState,
    pub(super) default_width: f64,
    pub(super) wrap_padding: f64,
    pub(super) is_neo: bool,
    pub(super) measurer: &'a dyn TextMeasurer,
    pub(super) msg_text_style: &'a TextStyle,
    pub(super) premeasured_bound: Option<SequenceMessageBoundMetrics>,
    pub(super) checkpoints: SequenceLayoutCheckpoints<'a>,
}

#[derive(Debug, Clone, Copy)]
struct MessageHorizontalRequest {
    semantics: SequenceSignalSemantics,
    central_decoration: SequenceCentralDecoration,
    activate: bool,
    is_neo: bool,
    is_self: bool,
    from_bounds: (f64, f64),
    to_bounds: (f64, f64),
    activation_width: f64,
    text_width: f64,
    wrap: bool,
    default_width: f64,
    wrap_padding: f64,
}

pub(super) fn sequence_message_horizontal_model(
    msg: &SequenceMessage,
    ctx: SequenceMessageHorizontalContext<'_>,
) -> Result<Option<SequenceMessageHorizontalModel>> {
    let Some(semantics) = msg.signal_semantics() else {
        return Ok(None);
    };
    let central_decoration = msg.central_decoration().unwrap_or_default();
    let (Some(from), Some(to)) = (msg.from.as_deref(), msg.to.as_deref()) else {
        return Ok(None);
    };
    let (Some(from_index), Some(to_index)) = (
        ctx.actor_index.get(from).copied(),
        ctx.actor_index.get(to).copied(),
    ) else {
        return Ok(None);
    };
    let from_bounds = ctx
        .activation_state
        .actor_bounds(from_index, ctx.actor_centers_x[from_index]);
    let to_bounds = ctx
        .activation_state
        .actor_bounds(to_index, ctx.actor_centers_x[to_index]);
    let text_width = if msg.wrap || msg.message_text().is_empty() {
        0.0
    } else if let Some(metrics) = ctx.premeasured_bound {
        metrics.width()
    } else {
        measure_svg_like_with_html_br(
            ctx.measurer,
            msg.message_text(),
            ctx.msg_text_style,
            ctx.checkpoints.text(),
        )?
        .0
        .max(0.0)
    };

    Ok(Some(message_horizontal_model_from_request(
        MessageHorizontalRequest {
            semantics,
            central_decoration,
            activate: msg.activate,
            is_neo: ctx.is_neo,
            is_self: from == to,
            from_bounds,
            to_bounds,
            activation_width: ctx.activation_state.width(),
            text_width,
            wrap: msg.wrap,
            default_width: ctx.default_width,
            wrap_padding: ctx.wrap_padding,
        },
    )))
}

fn message_horizontal_model_from_request(
    req: MessageHorizontalRequest,
) -> SequenceMessageHorizontalModel {
    let (from_left, from_right) = req.from_bounds;
    let (to_left, to_right) = req.to_bounds;
    let is_arrow_to_right = from_left <= to_left;
    let mut start_x = if is_arrow_to_right {
        from_right
    } else {
        from_left
    };
    let mut stop_x = if is_arrow_to_right { to_left } else { to_right };

    if req.is_neo {
        const NEO_MARKER_OFFSET: f64 = 3.0;
        // Mermaid exempts only SOLID_OPEN, not the headless dotted signal.
        let is_solid_open = req.semantics.stroke == SequenceMessageStroke::Solid
            && req.semantics.target_marker == SequenceMessageMarker::None
            && req.semantics.direction == SequenceMessageDirection::Forward;
        if !is_solid_open {
            stop_x += if is_arrow_to_right {
                -NEO_MARKER_OFFSET
            } else {
                NEO_MARKER_OFFSET
            };
        }
        if req.semantics.direction == SequenceMessageDirection::Bidirectional {
            start_x += if is_arrow_to_right {
                NEO_MARKER_OFFSET
            } else {
                -NEO_MARKER_OFFSET
            };
        }
    }

    start_x += central_decoration_start_offset(
        req.central_decoration,
        req.semantics.direction,
        is_arrow_to_right,
    );
    let is_arrow_to_activation = (to_left - to_right).abs() > 2.0;
    let adjust_value = |value: f64| if is_arrow_to_right { -value } else { value };

    if req.is_self {
        stop_x = start_x;
    } else {
        if req.activate && !is_arrow_to_activation {
            stop_x += adjust_value(req.activation_width / 2.0 - 1.0);
        }
        if marker_shortens_endpoint(req.semantics.target_marker) {
            stop_x += adjust_value(3.0);
        }
        if marker_shortens_endpoint(req.semantics.source_marker) {
            start_x -= adjust_value(3.0);
        }
    }

    let bounded_width = (start_x - stop_x).abs();
    let text_width = if req.wrap {
        0.0
    } else {
        req.text_width + 2.0 * req.wrap_padding
    };
    let width = text_width
        .max(bounded_width + 2.0 * req.wrap_padding)
        .max(req.default_width);

    SequenceMessageHorizontalModel {
        start_x,
        stop_x,
        width,
        bounded_width,
        from_bound: from_left.min(from_right).min(to_left).min(to_right),
        to_bound: from_left.max(from_right).max(to_left).max(to_right),
    }
}

fn marker_shortens_endpoint(marker: SequenceMessageMarker) -> bool {
    matches!(
        marker,
        SequenceMessageMarker::Filled
            | SequenceMessageMarker::Cross
            | SequenceMessageMarker::Point
            | SequenceMessageMarker::FilledHalfTop
            | SequenceMessageMarker::FilledHalfBottom
    )
}

pub(super) struct SequenceMessageLayoutContext<'a> {
    pub(super) actor_index: &'a std::collections::HashMap<&'a str, usize>,
    pub(super) actor_centers_x: &'a [f64],
    pub(super) actor_widths: &'a [f64],
    pub(super) activation_state: &'a SequenceActivationState,
    pub(super) msg_idx: usize,
    pub(super) actor_width_min: f64,
    pub(super) box_margin: f64,
    pub(super) wrap_padding: f64,
    pub(super) is_neo: bool,
    pub(super) right_angles: bool,
    pub(super) message_font_size: f64,
    pub(super) cursor_y: f64,
    pub(super) measurer: &'a dyn TextMeasurer,
    pub(super) msg_text_style: &'a TextStyle,
    pub(super) math_config: &'a MermaidConfig,
    pub(super) math_renderer: Option<&'a (dyn MathRenderer + Send + Sync)>,
    pub(super) premeasured_bound: Option<SequenceMessageBoundMetrics>,
    pub(super) created_actor_index: Option<usize>,
    pub(super) destroyed_from_index: Option<usize>,
    pub(super) destroyed_to_index: Option<usize>,
    pub(super) actor_is_type_width_limited: &'a dyn Fn(&str) -> bool,
    pub(super) checkpoints: SequenceLayoutCheckpoints<'a>,
}

pub(super) struct SequenceMessageLayout {
    pub(super) edge: LayoutEdge,
    pub(super) from_x: f64,
    pub(super) to_x: f64,
    pub(super) inserted_min_x: f64,
    pub(super) inserted_max_x: f64,
    pub(super) line_y: f64,
    pub(super) inserted_bottom_y: f64,
    pub(super) cursor_step: f64,
}

pub(super) fn layout_sequence_message(
    msg: &SequenceMessage,
    ctx: SequenceMessageLayoutContext<'_>,
) -> Result<Option<SequenceMessageLayout>> {
    let (Some(from), Some(to)) = (msg.from.as_deref(), msg.to.as_deref()) else {
        return Ok(None);
    };
    let (Some(fi), Some(ti)) = (
        ctx.actor_index.get(from).copied(),
        ctx.actor_index.get(to).copied(),
    ) else {
        return Ok(None);
    };
    let from_x = ctx.actor_centers_x[fi];
    let to_x = ctx.actor_centers_x[ti];

    let Some(horizontal) = sequence_message_horizontal_model(
        msg,
        SequenceMessageHorizontalContext {
            actor_index: ctx.actor_index,
            actor_centers_x: ctx.actor_centers_x,
            activation_state: ctx.activation_state,
            default_width: ctx.actor_width_min,
            wrap_padding: ctx.wrap_padding,
            is_neo: ctx.is_neo,
            measurer: ctx.measurer,
            msg_text_style: ctx.msg_text_style,
            premeasured_bound: ctx.premeasured_bound,
            checkpoints: ctx.checkpoints,
        },
    )?
    else {
        return Ok(None);
    };
    let mut startx = horizontal.start_x;
    let mut stopx = horizontal.stop_x;
    let is_self = from == to;

    let lifecycle_insert = if !is_self {
        adjust_created_destroyed_actor_endpoints(EndpointAdjustmentRequest {
            from,
            to,
            from_x,
            to_x,
            from_idx: fi,
            to_idx: ti,
            startx: &mut startx,
            stopx: &mut stopx,
            ctx: &ctx,
        })
    } else {
        None
    };

    let text = msg.message_text();
    let is_math_message = text.contains("$$");
    let wrapped_text = wrapped_message_text(
        text,
        msg.wrap,
        is_math_message,
        horizontal.bounded_width,
        &ctx,
    )?;
    let effective_text = wrapped_text.as_deref().unwrap_or(text);

    let premeasured_bound = if wrapped_text.is_none() && !is_math_message {
        ctx.premeasured_bound
    } else {
        None
    };
    let vertical = message_vertical_geometry(
        effective_text,
        is_math_message,
        is_self,
        premeasured_bound,
        &ctx,
    )?;

    let x1 = startx;
    let x2 = stopx;
    let label = message_label(
        effective_text,
        is_math_message,
        x1,
        x2,
        vertical.label_y,
        &ctx,
    )?;

    let self_insert = is_self.then(|| {
        let dx = (vertical.text_width / 2.0).max(ctx.actor_width_min / 2.0);
        (startx - dx, stopx + dx)
    });
    let inserted_min_x = horizontal
        .from_bound
        .min(startx)
        .min(stopx)
        .min(self_insert.map(|bounds| bounds.0).unwrap_or(f64::INFINITY))
        .min(
            lifecycle_insert
                .map(|bounds| bounds.0)
                .unwrap_or(f64::INFINITY),
        );
    let inserted_max_x = horizontal
        .to_bound
        .max(startx)
        .max(stopx)
        .max(
            self_insert
                .map(|bounds| bounds.1)
                .unwrap_or(f64::NEG_INFINITY),
        )
        .max(
            lifecycle_insert
                .map(|bounds| bounds.1)
                .unwrap_or(f64::NEG_INFINITY),
        );

    Ok(Some(SequenceMessageLayout {
        edge: LayoutEdge {
            id: format!("msg-{}", msg.id),
            from: from.to_string(),
            to: to.to_string(),
            from_cluster: None,
            to_cluster: None,
            points: vec![
                LayoutPoint {
                    x: x1,
                    y: vertical.line_y,
                },
                LayoutPoint {
                    x: x2,
                    y: vertical.line_y,
                },
            ],
            label,
            start_label_left: None,
            start_label_right: None,
            end_label_left: None,
            end_label_right: None,
            start_marker: None,
            end_marker: None,
            stroke_dasharray: None,
        },
        from_x,
        to_x,
        inserted_min_x,
        inserted_max_x,
        line_y: vertical.line_y,
        inserted_bottom_y: vertical.inserted_bottom_y,
        cursor_step: vertical.cursor_step,
    }))
}

fn central_decoration_start_offset(
    central_decoration: SequenceCentralDecoration,
    direction: SequenceMessageDirection,
    is_arrow_to_right: bool,
) -> f64 {
    let has_source_decoration = matches!(
        central_decoration,
        SequenceCentralDecoration::Source | SequenceCentralDecoration::Both
    );
    if !has_source_decoration {
        return 0.0;
    }

    let bidirectional_offset = if direction == SequenceMessageDirection::Bidirectional {
        if is_arrow_to_right {
            0.0
        } else {
            -CENTRAL_CONNECTION_BIDIRECTIONAL_OFFSET
        }
    } else {
        0.0
    };

    CENTRAL_CONNECTION_BASE_OFFSET + bidirectional_offset
}

struct EndpointAdjustmentRequest<'a, 'b> {
    from: &'a str,
    to: &'a str,
    from_x: f64,
    to_x: f64,
    from_idx: usize,
    to_idx: usize,
    startx: &'b mut f64,
    stopx: &'b mut f64,
    ctx: &'b SequenceMessageLayoutContext<'a>,
}

fn adjust_created_destroyed_actor_endpoints(
    req: EndpointAdjustmentRequest<'_, '_>,
) -> Option<(f64, f64)> {
    // Mermaid adjusts creating/destroying messages so arrowheads land outside the actor box.
    const ACTOR_TYPE_WIDTH_HALF: f64 = 18.0;

    if req.ctx.created_actor_index == Some(req.ctx.msg_idx) {
        let adjustment = if (req.ctx.actor_is_type_width_limited)(req.to) {
            ACTOR_TYPE_WIDTH_HALF + 3.0
        } else {
            req.ctx.actor_widths[req.to_idx] / 2.0 + 3.0
        };
        if req.to_x < req.from_x {
            let inserted = ((*req.stopx - adjustment).min(*req.startx), *req.startx);
            *req.stopx += adjustment;
            Some(inserted)
        } else {
            let inserted = (*req.startx, (*req.stopx + adjustment).max(*req.startx));
            *req.stopx -= adjustment;
            Some(inserted)
        }
    } else if req.ctx.destroyed_from_index == Some(req.ctx.msg_idx) {
        let adjustment = if (req.ctx.actor_is_type_width_limited)(req.from) {
            ACTOR_TYPE_WIDTH_HALF
        } else {
            req.ctx.actor_widths[req.from_idx] / 2.0
        };
        if req.from_x < req.to_x {
            let inserted_start = *req.startx - adjustment;
            let inserted = (
                inserted_start.min(*req.stopx),
                inserted_start.max(*req.stopx),
            );
            *req.startx += adjustment;
            Some(inserted)
        } else {
            let inserted_stop = *req.startx + adjustment;
            let inserted = (
                (*req.stopx).min(inserted_stop),
                (*req.stopx).max(inserted_stop),
            );
            *req.startx -= adjustment;
            Some(inserted)
        }
    } else if req.ctx.destroyed_to_index == Some(req.ctx.msg_idx) {
        let adjustment = if (req.ctx.actor_is_type_width_limited)(req.to) {
            ACTOR_TYPE_WIDTH_HALF + 3.0
        } else {
            req.ctx.actor_widths[req.to_idx] / 2.0 + 3.0
        };
        if req.to_x < req.from_x {
            let inserted = ((*req.stopx - adjustment).min(*req.startx), *req.startx);
            *req.stopx += adjustment;
            Some(inserted)
        } else {
            let inserted = (*req.startx, (*req.stopx + adjustment).max(*req.startx));
            *req.stopx -= adjustment;
            Some(inserted)
        }
    } else {
        None
    }
}

fn wrapped_message_text(
    text: &str,
    should_wrap: bool,
    is_math_message: bool,
    bounded_width: f64,
    ctx: &SequenceMessageLayoutContext<'_>,
) -> Result<Option<String>> {
    if text.is_empty() || !should_wrap || is_math_message {
        return Ok(None);
    }

    // Upstream wraps message labels to `max(boundedWidth + 2*wrapPadding, conf.width)`.
    let wrap_w = (bounded_width + SEQUENCE_MESSAGE_WRAP_PADDING_SIDES * ctx.wrap_padding)
        .max(ctx.actor_width_min)
        .max(1.0);
    let lines = wrap_sequence_label_like_mermaid_lines(
        text,
        ctx.measurer,
        ctx.msg_text_style,
        wrap_w,
        ctx.checkpoints.text(),
    )?;
    Ok(Some(lines.join("<br>")))
}

#[derive(Debug, Clone, Copy, PartialEq)]
struct SequenceMessageVerticalGeometry {
    text_width: f64,
    line_y: f64,
    label_y: f64,
    cursor_step: f64,
    inserted_bottom_y: f64,
}

fn message_vertical_geometry(
    effective_text: &str,
    is_math_message: bool,
    is_self: bool,
    premeasured_bound: Option<SequenceMessageBoundMetrics>,
    ctx: &SequenceMessageLayoutContext<'_>,
) -> Result<SequenceMessageVerticalGeometry> {
    let (text_width, text_height) = if effective_text.is_empty() {
        (0.0, 0.0)
    } else if let Some(metrics) = premeasured_bound {
        (metrics.width(), metrics.height())
    } else {
        measure_sequence_label_for_layout(
            ctx.measurer,
            effective_text,
            ctx.msg_text_style,
            ctx.math_config,
            ctx.math_renderer,
            SequenceMathHeightMode::Bound,
            ctx.checkpoints.text(),
        )?
    };

    let lines = split_html_br_lines(effective_text).len().max(1);
    let mut geometry = message_vertical_geometry_from_measurement(SequenceMessageVerticalRequest {
        cursor_y: ctx.cursor_y,
        text_height,
        line_count: lines,
        is_math_message,
        is_self,
        right_angles: ctx.right_angles,
        box_margin: ctx.box_margin,
        wrap_padding: ctx.wrap_padding,
    });
    geometry.text_width = text_width;
    Ok(geometry)
}

#[derive(Debug, Clone, Copy)]
struct SequenceMessageVerticalRequest {
    cursor_y: f64,
    text_height: f64,
    line_count: usize,
    is_math_message: bool,
    is_self: bool,
    right_angles: bool,
    box_margin: f64,
    wrap_padding: f64,
}

fn message_vertical_geometry_from_measurement(
    req: SequenceMessageVerticalRequest,
) -> SequenceMessageVerticalGeometry {
    let text_height = req.text_height.max(0.0);
    let line_height = if req.is_math_message {
        0.0
    } else {
        text_height / req.line_count.max(1) as f64
    };
    let self_margin = req.is_self && req.right_angles;
    let box_margin = if self_margin { 0.0 } else { req.box_margin };
    let line_y = req.cursor_y + text_height + line_height + box_margin;
    let self_advance = if req.is_self { 30.0 } else { 0.0 };

    SequenceMessageVerticalGeometry {
        text_width: 0.0,
        line_y,
        // drawMessage sets y=starty+10; drawText then centers the first line within wrapPadding.
        label_y: super::sequence_drawn_text_first_y(req.cursor_y + 10.0, req.wrap_padding),
        cursor_step: line_y + self_advance - req.cursor_y,
        inserted_bottom_y: line_y + if req.is_self { 60.0 } else { 0.0 },
    }
}

fn message_label(
    effective_text: &str,
    is_math_message: bool,
    x1: f64,
    x2: f64,
    label_y: f64,
    ctx: &SequenceMessageLayoutContext<'_>,
) -> Result<Option<LayoutLabel>> {
    if effective_text.is_empty() {
        // Mermaid renders an (empty) message text node even when the label is empty (e.g.
        // trailing colon `Alice->Bob:`). Keep a placeholder label to preserve DOM structure.
        return Ok(Some(LayoutLabel {
            x: ((x1 + x2) / 2.0).round(),
            y: label_y,
            width: 1.0,
            height: ctx.message_font_size.max(1.0),
        }));
    }

    let (w, h) = if is_math_message {
        measure_sequence_label_for_layout(
            ctx.measurer,
            effective_text,
            ctx.msg_text_style,
            ctx.math_config,
            ctx.math_renderer,
            SequenceMathHeightMode::Draw,
            ctx.checkpoints.text(),
        )?
    } else {
        // Sequence sets `textObj.tspan = false` for messages, so the final label bbox is a direct
        // `<text>` measurement even though the earlier layout probe used `drawSimpleText` with a
        // child `<tspan>`.
        measure_drawn_svg_like_with_html_br(
            ctx.measurer,
            effective_text,
            ctx.msg_text_style,
            SequenceDrawnTextNode::Direct,
            ctx.math_config,
            ctx.checkpoints.text(),
        )?
    };
    Ok(Some(LayoutLabel {
        x: ((x1 + x2) / 2.0).round(),
        y: label_y,
        width: w.max(1.0),
        height: h.max(1.0),
    }))
}

#[cfg(test)]
mod tests {
    use super::{
        MessageHorizontalRequest, SequenceMessageVerticalRequest,
        message_horizontal_model_from_request, message_vertical_geometry_from_measurement,
    };
    use merman_core::diagrams::sequence::{
        SequenceCentralDecoration, SequenceMessageDirection, SequenceMessageMarker,
        SequenceMessageStroke, SequenceSignalSemantics,
    };

    fn horizontal_request(semantics: SequenceSignalSemantics) -> MessageHorizontalRequest {
        MessageHorizontalRequest {
            semantics,
            central_decoration: SequenceCentralDecoration::None,
            activate: false,
            is_neo: false,
            is_self: false,
            from_bounds: (99.0, 101.0),
            to_bounds: (199.0, 201.0),
            activation_width: 10.0,
            text_width: 0.0,
            wrap: false,
            default_width: 0.0,
            wrap_padding: 0.0,
        }
    }

    fn forward(target_marker: SequenceMessageMarker) -> SequenceSignalSemantics {
        SequenceSignalSemantics {
            stroke: SequenceMessageStroke::Solid,
            source_marker: SequenceMessageMarker::None,
            target_marker,
            direction: SequenceMessageDirection::Forward,
        }
    }

    fn reverse(source_marker: SequenceMessageMarker) -> SequenceSignalSemantics {
        SequenceSignalSemantics {
            stroke: SequenceMessageStroke::Solid,
            source_marker,
            target_marker: SequenceMessageMarker::None,
            direction: SequenceMessageDirection::Reverse,
        }
    }

    fn bidirectional() -> SequenceSignalSemantics {
        SequenceSignalSemantics {
            stroke: SequenceMessageStroke::Solid,
            source_marker: SequenceMessageMarker::Filled,
            target_marker: SequenceMessageMarker::Filled,
            direction: SequenceMessageDirection::Bidirectional,
        }
    }

    #[test]
    fn horizontal_model_preserves_headless_and_open_half_endpoints() {
        let headless = forward(SequenceMessageMarker::None);
        let headless_model = message_horizontal_model_from_request(horizontal_request(headless));
        let open_half = message_horizontal_model_from_request(horizontal_request(forward(
            SequenceMessageMarker::OpenHalfTop,
        )));
        let solid = message_horizontal_model_from_request(horizontal_request(forward(
            SequenceMessageMarker::Filled,
        )));
        let mut neo_headless_request = horizontal_request(headless);
        neo_headless_request.is_neo = true;
        let neo_headless = message_horizontal_model_from_request(neo_headless_request);
        let mut neo_open_half_request =
            horizontal_request(forward(SequenceMessageMarker::OpenHalfTop));
        neo_open_half_request.is_neo = true;
        let neo_open_half = message_horizontal_model_from_request(neo_open_half_request);
        let mut dotted_headless = headless;
        dotted_headless.stroke = SequenceMessageStroke::Dotted;
        let mut neo_dotted_headless_request = horizontal_request(dotted_headless);
        neo_dotted_headless_request.is_neo = true;
        let neo_dotted_headless =
            message_horizontal_model_from_request(neo_dotted_headless_request);

        assert_eq!(
            (headless_model.start_x, headless_model.stop_x),
            (101.0, 199.0)
        );
        assert_eq!((open_half.start_x, open_half.stop_x), (101.0, 199.0));
        assert_eq!((solid.start_x, solid.stop_x), (101.0, 196.0));
        assert_eq!((neo_headless.start_x, neo_headless.stop_x), (101.0, 199.0));
        assert_eq!(
            (neo_open_half.start_x, neo_open_half.stop_x),
            (101.0, 196.0)
        );
        assert_eq!(
            (neo_dotted_headless.start_x, neo_dotted_headless.stop_x),
            (101.0, 196.0)
        );
    }

    #[test]
    fn horizontal_model_applies_reverse_bidirectional_central_and_neo_rules() {
        let reverse_model = message_horizontal_model_from_request(horizontal_request(reverse(
            SequenceMessageMarker::FilledHalfTop,
        )));
        let bidirectional_model =
            message_horizontal_model_from_request(horizontal_request(bidirectional()));

        let mut central_request = horizontal_request(bidirectional());
        central_request.central_decoration = SequenceCentralDecoration::Both;
        let central = message_horizontal_model_from_request(central_request);

        let mut neo_request = horizontal_request(bidirectional());
        neo_request.is_neo = true;
        let neo = message_horizontal_model_from_request(neo_request);
        let mut neo_reverse_request =
            horizontal_request(reverse(SequenceMessageMarker::FilledHalfTop));
        neo_reverse_request.is_neo = true;
        let neo_reverse = message_horizontal_model_from_request(neo_reverse_request);

        assert_eq!(
            (reverse_model.start_x, reverse_model.stop_x),
            (104.0, 199.0)
        );
        assert_eq!(
            (bidirectional_model.start_x, bidirectional_model.stop_x),
            (104.0, 196.0)
        );
        assert_eq!((central.start_x, central.stop_x), (108.0, 196.0));
        assert_eq!((neo.start_x, neo.stop_x), (107.0, 193.0));
        assert_eq!((neo_reverse.start_x, neo_reverse.stop_x), (104.0, 196.0));
    }

    #[test]
    fn horizontal_model_applies_leftward_central_bidirectional_offset() {
        let mut request = horizontal_request(bidirectional());
        request.from_bounds = (199.0, 201.0);
        request.to_bounds = (99.0, 101.0);
        request.central_decoration = SequenceCentralDecoration::Both;

        let model = message_horizontal_model_from_request(request);

        assert_eq!((model.start_x, model.stop_x), (194.0, 104.0));
    }

    #[test]
    fn horizontal_model_targets_first_and_existing_activations() {
        let mut first_activation_request = horizontal_request(forward(SequenceMessageMarker::None));
        first_activation_request.activate = true;
        let first_activation = message_horizontal_model_from_request(first_activation_request);

        let mut existing_activation_request =
            horizontal_request(forward(SequenceMessageMarker::None));
        existing_activation_request.activate = true;
        existing_activation_request.to_bounds = (195.0, 205.0);
        let existing_activation =
            message_horizontal_model_from_request(existing_activation_request);

        assert_eq!(first_activation.stop_x, 195.0);
        assert_eq!(existing_activation.stop_x, 195.0);
    }

    #[test]
    fn ordinary_multiline_message_uses_measured_total_and_per_line_heights() {
        let geometry = message_vertical_geometry_from_measurement(SequenceMessageVerticalRequest {
            cursor_y: 100.0,
            text_height: 51.0,
            line_count: 3,
            is_math_message: false,
            is_self: false,
            right_angles: false,
            box_margin: 10.0,
            wrap_padding: 10.0,
        });

        assert_eq!(geometry.line_y, 178.0);
        assert_eq!(geometry.cursor_step, 78.0);
        assert_eq!(geometry.inserted_bottom_y, 178.0);
        assert_eq!(geometry.label_y, 115.0);
    }

    #[test]
    fn math_message_skips_the_plain_text_line_height_bump() {
        let geometry = message_vertical_geometry_from_measurement(SequenceMessageVerticalRequest {
            cursor_y: 100.0,
            text_height: 28.0,
            line_count: 1,
            is_math_message: true,
            is_self: false,
            right_angles: false,
            box_margin: 10.0,
            wrap_padding: 10.0,
        });

        assert_eq!(geometry.line_y, 138.0);
        assert_eq!(geometry.cursor_step, 38.0);
    }

    #[test]
    fn self_message_tracks_cursor_and_inserted_bounds_overhangs() {
        let curved = message_vertical_geometry_from_measurement(SequenceMessageVerticalRequest {
            cursor_y: 100.0,
            text_height: 20.0,
            line_count: 1,
            is_math_message: false,
            is_self: true,
            right_angles: false,
            box_margin: 10.0,
            wrap_padding: 10.0,
        });
        let right_angled =
            message_vertical_geometry_from_measurement(SequenceMessageVerticalRequest {
                right_angles: true,
                ..SequenceMessageVerticalRequest {
                    cursor_y: 100.0,
                    text_height: 20.0,
                    line_count: 1,
                    is_math_message: false,
                    is_self: true,
                    right_angles: false,
                    box_margin: 10.0,
                    wrap_padding: 10.0,
                }
            });

        assert_eq!(curved.line_y, 150.0);
        assert_eq!(curved.cursor_step, 80.0);
        assert_eq!(curved.inserted_bottom_y, 210.0);
        assert_eq!(right_angled.line_y, 140.0);
        assert_eq!(right_angled.cursor_step, 70.0);
        assert_eq!(right_angled.inserted_bottom_y, 200.0);
    }
    #[test]
    fn message_first_row_keeps_fractional_position_and_only_positive_margin_offset() {
        for margin in [5.0, 0.0, -5.0] {
            let geometry =
                message_vertical_geometry_from_measurement(SequenceMessageVerticalRequest {
                    cursor_y: 100.25,
                    text_height: 30.8,
                    line_count: 2,
                    is_math_message: false,
                    is_self: false,
                    right_angles: false,
                    box_margin: 10.0,
                    wrap_padding: margin,
                });
            assert_eq!(geometry.label_y, if margin > 0.0 { 112.75 } else { 110.25 });
        }
    }
}
