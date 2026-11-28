use super::super::*;
use super::SequenceEmitCheckpoints;
use super::math_label::{sequence_katex_label, write_sequence_katex_foreign_object};
use super::model::{SequenceSvgMessagePayload, SequenceSvgModel};
use crate::sequence::{
    SEQUENCE_MESSAGE_WRAP_PADDING_SIDES, SequenceDrawnTextNode, SequenceMathHeightMode,
    measure_sequence_drawn_line_height, sequence_activation_stack_bounds,
    sequence_drawn_text_style, sequence_drawn_text_y,
};
use merman_core::diagrams::sequence::{
    SequenceCentralDecoration, SequenceMessageDirection, SequenceMessageKind,
    SequenceMessageMarker, SequenceMessageStroke,
};
use rustc_hash::FxHashMap;
use std::collections::BTreeMap;

const CENTRAL_CONNECTION_CIRCLE_OFFSET: f64 = 16.5;

pub(super) struct SequenceMessageRenderContext<'a> {
    pub(super) model: &'a SequenceSvgModel,
    pub(super) nodes_by_id: &'a FxHashMap<&'a str, &'a LayoutNode>,
    pub(super) edges_by_id: &'a FxHashMap<&'a str, &'a crate::model::LayoutEdge>,
    pub(super) sanitize_config: &'a merman_core::MermaidConfig,
    pub(super) math_renderer: Option<&'a (dyn crate::math::MathRenderer + Send + Sync)>,
    pub(super) measurer: &'a dyn TextMeasurer,
    pub(super) message_align: &'a str,
    pub(super) diagram_id: SvgDiagramId<'a>,
    pub(super) actor_height: f64,
    pub(super) sequence_width: f64,
    pub(super) activation_width: f64,
    pub(super) wrap_padding: f64,
    pub(super) right_angles: bool,
    pub(super) loop_text_style: &'a TextStyle,
    pub(super) checkpoints: SequenceEmitCheckpoints<'a>,
}

fn marker_attr(attr_name: &str, diagram_id: SvgDiagramId<'_>, local_id: &str) -> String {
    format!(
        r#" {attr_name}="{}""#,
        escape_attr_display(scoped_svg_url(diagram_id, local_id))
    )
}

fn endpoint_marker_local_id(
    marker: SequenceMessageMarker,
    source_endpoint: bool,
) -> Option<&'static str> {
    use SequenceMessageMarker as Marker;

    match (marker, source_endpoint) {
        (Marker::None, _) => None,
        (Marker::Filled, _) => Some("arrowhead"),
        (Marker::Cross, _) => Some("crosshead"),
        (Marker::Point, _) => Some("filled-head"),
        (Marker::FilledHalfTop, false) | (Marker::FilledHalfBottom, true) => {
            Some("solidTopArrowHead")
        }
        (Marker::FilledHalfBottom, false) | (Marker::FilledHalfTop, true) => {
            Some("solidBottomArrowHead")
        }
        (Marker::OpenHalfTop, false) | (Marker::OpenHalfBottom, true) => Some("stickTopArrowHead"),
        (Marker::OpenHalfBottom, false) | (Marker::OpenHalfTop, true) => {
            Some("stickBottomArrowHead")
        }
    }
}

fn message_data_attrs(msg_id: &str, from: &str, to: &str) -> String {
    format!(
        r#" data-et="message" data-id="i{msg_id}" data-from="{}" data-to="{}""#,
        escape_attr(from),
        escape_attr(to)
    )
}

fn is_reverse_arrow_type(msg: &merman_core::diagrams::sequence::SequenceMessage) -> bool {
    msg.signal_semantics()
        .is_some_and(|semantics| semantics.direction == SequenceMessageDirection::Reverse)
}

fn actor_center_x(ctx: &SequenceMessageRenderContext<'_>, actor_id: &str) -> Option<f64> {
    ctx.nodes_by_id
        .get(format!("actor-top-{actor_id}").as_str())
        .map(|node| node.x)
}

struct SequenceAutonumberActivationBounds {
    width: f64,
    depths: BTreeMap<String, usize>,
}

impl SequenceAutonumberActivationBounds {
    fn new(width: f64) -> Self {
        Self {
            width,
            depths: BTreeMap::new(),
        }
    }

    fn handle_directive(
        &mut self,
        msg: &merman_core::diagrams::sequence::SequenceMessage,
        ctx: &SequenceMessageRenderContext<'_>,
    ) -> bool {
        match msg.semantic_kind() {
            SequenceMessageKind::ActivationStart => {
                let Some(actor_id) = msg.from.as_deref() else {
                    return true;
                };
                if actor_center_x(ctx, actor_id).is_none() {
                    return true;
                }
                let depth = self.depths.entry(actor_id.to_string()).or_default();
                *depth = depth.saturating_add(1);
                true
            }
            SequenceMessageKind::ActivationEnd => {
                let Some(actor_id) = msg.from.as_deref() else {
                    return true;
                };
                if let Some(depth) = self.depths.get_mut(actor_id) {
                    *depth = depth.saturating_sub(1);
                }
                true
            }
            _ => false,
        }
    }

    fn actor_bounds(&self, actor_id: &str, center_x: f64) -> (f64, f64) {
        sequence_activation_stack_bounds(
            self.depths.get(actor_id).copied().unwrap_or_default(),
            center_x,
            self.width,
        )
    }
}

fn sequence_number_marker_x(
    activation_bounds: &SequenceAutonumberActivationBounds,
    ctx: &SequenceMessageRenderContext<'_>,
    msg: &merman_core::diagrams::sequence::SequenceMessage,
    from: &str,
    to: &str,
    startx: f64,
    stopx: f64,
) -> Option<f64> {
    let from_center = actor_center_x(ctx, from)?;
    let to_center = actor_center_x(ctx, to)?;
    let (from_left, from_right) = activation_bounds.actor_bounds(from, from_center);
    let (to_left, to_right) = activation_bounds.actor_bounds(to, to_center);
    let from_bounds = from_left.min(from_right).min(to_left).min(to_right);
    let to_bounds = from_left.max(from_right).max(to_left).max(to_right);
    let is_self_message = (startx - stopx).abs() <= f64::EPSILON;
    let is_left_to_right = startx <= stopx;

    Some(if is_self_message {
        from_bounds + 1.0
    } else if is_reverse_arrow_type(msg) {
        if is_left_to_right {
            to_bounds - 1.0
        } else {
            from_bounds + 1.0
        }
    } else if is_left_to_right {
        from_bounds + 1.0
    } else {
        to_bounds - 1.0
    })
}

fn write_central_connection_circles(
    out: &mut String,
    ctx: &SequenceMessageRenderContext<'_>,
    msg: &merman_core::diagrams::sequence::SequenceMessage,
    from: &str,
    to: &str,
    line_y: f64,
    sequence_number_visible: bool,
) {
    let Some(decoration) = msg.central_decoration() else {
        return;
    };
    if decoration == SequenceCentralDecoration::None {
        return;
    }

    let (Some(mut from_center), Some(mut to_center)) =
        (actor_center_x(ctx, from), actor_center_x(ctx, to))
    else {
        return;
    };
    let is_left_to_right = from_center <= to_center;
    let is_reverse = is_reverse_arrow_type(msg);
    let circle_offset = |is_left_to_right: bool, is_reverse: bool| {
        let base_offset = if is_left_to_right {
            CENTRAL_CONNECTION_CIRCLE_OFFSET
        } else {
            -CENTRAL_CONNECTION_CIRCLE_OFFSET
        };
        if is_reverse {
            -base_offset
        } else {
            base_offset
        }
    };

    if sequence_number_visible {
        match decoration {
            SequenceCentralDecoration::Target if is_reverse => {
                to_center += circle_offset(is_left_to_right, true);
            }
            SequenceCentralDecoration::Source if !is_reverse => {
                from_center += circle_offset(is_left_to_right, false);
            }
            SequenceCentralDecoration::Both => {
                if is_reverse {
                    to_center += circle_offset(is_left_to_right, true);
                } else {
                    from_center += circle_offset(is_left_to_right, false);
                }
            }
            _ => {}
        }
    }

    out.push_str("<g>");
    if matches!(
        decoration,
        SequenceCentralDecoration::Source | SequenceCentralDecoration::Both
    ) {
        let _ = write!(
            out,
            r#"<circle cx="{cx}" cy="{cy}" r="5" width="10" height="10"/>"#,
            cx = fmt(from_center),
            cy = fmt(line_y)
        );
    }
    if matches!(
        decoration,
        SequenceCentralDecoration::Target | SequenceCentralDecoration::Both
    ) {
        let _ = write!(
            out,
            r#"<circle cx="{cx}" cy="{cy}" r="5" width="10" height="10"/>"#,
            cx = fmt(to_center),
            cy = fmt(line_y)
        );
    }
    out.push_str("</g>");
}

pub(super) fn render_sequence_messages(
    out: &mut String,
    ctx: &SequenceMessageRenderContext<'_>,
) -> Result<()> {
    let mut sequence_number_visible = false;
    let mut sequence_number = 1.0;
    let mut sequence_number_step = 1.0;
    let mut activation_bounds = SequenceAutonumberActivationBounds::new(ctx.activation_width);

    for (decoration_index, _) in ctx
        .model
        .messages
        .iter()
        .filter(|msg| msg.semantic_kind() == SequenceMessageKind::CentralDecorationRecord)
        .enumerate()
    {
        ctx.checkpoints.checkpoint_loop(decoration_index)?;
        out.push_str("<g/>");
    }
    ctx.checkpoints.checkpoint()?;

    for (message_index, msg) in ctx.model.messages.iter().enumerate() {
        ctx.checkpoints.checkpoint_loop(message_index)?;
        match msg.semantic_kind() {
            SequenceMessageKind::Autonumber => {
                if let SequenceSvgMessagePayload::Autonumber(autonumber) = &msg.message {
                    sequence_number_visible = autonumber.visible;
                    if let Some(start) = autonumber.start {
                        sequence_number = start;
                    }
                    if let Some(step) = autonumber.step {
                        sequence_number_step = step;
                    }
                }
                continue;
            }
            SequenceMessageKind::ActivationStart | SequenceMessageKind::ActivationEnd => {
                let _ = activation_bounds.handle_directive(msg, ctx);
                continue;
            }
            SequenceMessageKind::Note => continue,
            // Central decoration records are routed through the activation drawing path by
            // upstream Mermaid, which leaves an empty group without a visible rectangle.
            SequenceMessageKind::CentralDecorationRecord => continue,
            SequenceMessageKind::Signal => {}
            SequenceMessageKind::Control | SequenceMessageKind::Unknown => continue,
        }

        let Some(signal_semantics) = msg.signal_semantics() else {
            continue;
        };
        let current_sequence_number = sequence_number;
        // Mermaid advances the sequence index for every signal, even while autonumber is hidden.
        sequence_number = round_sequence_number(sequence_number + sequence_number_step);

        let (Some(from), Some(to)) = (msg.from.as_deref(), msg.to.as_deref()) else {
            continue;
        };
        let edge_id = format!("msg-{}", msg.id);
        let Some(edge) = ctx.edges_by_id.get(edge_id.as_str()).copied() else {
            continue;
        };
        if edge.points.len() < 2 {
            continue;
        }

        let p0 = &edge.points[0];
        let p1 = &edge.points[1];
        let sequence_number_x = if sequence_number_visible {
            sequence_number_marker_x(&activation_bounds, ctx, msg, from, to, p0.x, p1.x)
                .unwrap_or(p0.x)
        } else {
            p0.x
        };

        let text = msg.message_text();
        if let Some(lbl) = &edge.label {
            let bounded_width = (p0.x - p1.x).abs().max(0.0);
            // Mermaid aligns message label text based on `sequence.messageAlign`.
            let label_start_x = p0.x.min(p1.x);
            let (label_x, label_anchor) = match ctx.message_align {
                "right" => (label_start_x + bounded_width - ctx.wrap_padding, "end"),
                "left" => (label_start_x + ctx.wrap_padding, "start"),
                _ => (lbl.x, "middle"),
            };
            if let Some(katex) = sequence_katex_label(
                text,
                ctx.measurer,
                ctx.loop_text_style,
                ctx.sanitize_config,
                ctx.math_renderer,
                SequenceMathHeightMode::Draw,
                ctx.checkpoints,
            )? {
                let center_x = (p0.x + p1.x) / 2.0;
                write_sequence_katex_foreign_object(
                    out,
                    &katex,
                    (center_x - katex.width / 2.0).round(),
                    (p0.y - katex.height).round(),
                );
            } else if msg.wrap && !text.is_empty() {
                // Mermaid wraps message labels to
                // `max(boundedWidth + 2*wrapPadding, conf.width)`.
                let wrap_w = (bounded_width
                    + SEQUENCE_MESSAGE_WRAP_PADDING_SIDES * ctx.wrap_padding)
                    .max(ctx.sequence_width)
                    .max(1.0);
                let raw_lines = crate::sequence::wrap_sequence_label_like_mermaid_lines(
                    text,
                    ctx.measurer,
                    ctx.loop_text_style,
                    wrap_w,
                    ctx.checkpoints.text(),
                )?;
                render_sequence_message_text_lines(
                    out,
                    raw_lines.iter().map(String::as_str),
                    SequenceMessageTextLayout {
                        label_y: lbl.y,
                        label_x,
                        label_anchor,
                        margin: ctx.wrap_padding,
                        style: ctx.loop_text_style,
                        measurer: ctx.measurer,
                        config: ctx.sanitize_config,
                    },
                    ctx.checkpoints,
                )?;
            } else {
                render_sequence_message_text_lines(
                    out,
                    crate::text::split_html_br_lines(text),
                    SequenceMessageTextLayout {
                        label_y: lbl.y,
                        label_x,
                        label_anchor,
                        margin: ctx.wrap_padding,
                        style: ctx.loop_text_style,
                        measurer: ctx.measurer,
                        config: ctx.sanitize_config,
                    },
                    ctx.checkpoints,
                )?;
            }
        }

        let (class, style) = if signal_semantics.stroke == SequenceMessageStroke::Dotted {
            (
                "messageLine1",
                r#" style="stroke-dasharray: 3, 3; fill: none;""#,
            )
        } else {
            ("messageLine0", r#" style="fill: none;""#)
        };

        let marker_start = endpoint_marker_local_id(signal_semantics.source_marker, true)
            .map(|local_id| marker_attr("marker-start", ctx.diagram_id, local_id));
        let marker_end = endpoint_marker_local_id(signal_semantics.target_marker, false)
            .map(|local_id| marker_attr("marker-end", ctx.diagram_id, local_id));
        ctx.checkpoints.checkpoint()?;
        let data_attrs = message_data_attrs(&msg.id, from, to);

        // Mermaid uses `stroke="none"` and assigns actual stroke via CSS.
        if from == to {
            let startx = p0.x;
            let y = p0.y;
            let d = if ctx.right_angles {
                let actor_w = ctx
                    .nodes_by_id
                    .get(format!("actor-top-{from}").as_str())
                    .map(|n| n.width)
                    .unwrap_or(ctx.actor_height);
                let text_dx = edge.label.as_ref().map(|l| l.width / 2.0).unwrap_or(0.0);
                let dx = (actor_w / 2.0).max(text_dx);
                format!(
                    "M  {x},{y} H {hx} V {vy} H {x}",
                    x = fmt(startx),
                    y = fmt(y),
                    hx = fmt(startx + dx),
                    vy = fmt(y + 25.0)
                )
            } else {
                format!(
                    "M {x},{y} C {x2},{y2} {x2},{y3} {x},{y4}",
                    x = fmt(startx),
                    y = fmt(y),
                    x2 = fmt(startx + 60.0),
                    y2 = fmt(y - 10.0),
                    y3 = fmt(y + 30.0),
                    y4 = fmt(y + 20.0)
                )
            };
            // Mermaid attaches an `x1` attribute to autonumbered self-reference paths even
            // though the geometry lives in the `d` attribute.
            let path_x1 = if sequence_number_visible {
                Some(if marker_start.is_some() {
                    p0.x + 6.0
                } else {
                    p0.x
                })
            } else {
                None
            };
            let _ = write!(
                out,
                r#"<path d="{d}" class="{class}"{data_attrs} stroke-width="2" stroke="none"{marker_start}{marker_end}{x1}{style}/>"#,
                d = d,
                class = class,
                data_attrs = data_attrs,
                marker_start = marker_start.as_deref().unwrap_or(""),
                marker_end = marker_end.as_deref().unwrap_or(""),
                x1 = path_x1
                    .map(|x1| format!(r#" x1="{x1}""#, x1 = fmt(x1)))
                    .unwrap_or_default(),
                style = style
            );
            write_central_connection_circles(out, ctx, msg, from, to, y, sequence_number_visible);
        } else {
            let _ = write!(
                out,
                r#"<line x1="{x1}" y1="{y1}" x2="{x2}" y2="{y2}" class="{class}"{data_attrs} stroke-width="2" stroke="none"{marker_start}{marker_end}{style}/>"#,
                x1 = fmt(p0.x),
                y1 = fmt(p0.y),
                x2 = fmt(p1.x),
                y2 = fmt(p1.y),
                class = class,
                data_attrs = data_attrs,
                marker_start = marker_start.as_deref().unwrap_or(""),
                marker_end = marker_end.as_deref().unwrap_or(""),
                style = style
            );
            write_central_connection_circles(
                out,
                ctx,
                msg,
                from,
                to,
                p0.y,
                sequence_number_visible,
            );
        }

        if sequence_number_visible {
            let sequence_number_text = format_sequence_number(current_sequence_number);
            let font_size = if sequence_number_text.len() > 5 {
                "7px"
            } else if sequence_number_text.len() > 3 {
                "9px"
            } else {
                "12px"
            };
            let x = sequence_number_x;
            let y = p0.y;
            let _ = write!(
                out,
                r#"<line x1="{x}" y1="{y}" x2="{x}" y2="{y}" stroke-width="0" marker-start="{marker_start}"/>"#,
                x = fmt(x),
                y = fmt(y),
                marker_start =
                    escape_attr_display(scoped_svg_url(ctx.diagram_id, "sequencenumber")),
            );
            ctx.checkpoints.checkpoint()?;
            let _ = write!(
                out,
                r#"<text x="{x}" y="{y}" font-family="sans-serif" font-size="{font_size}" text-anchor="middle" class="sequenceNumber">{n}</text>"#,
                x = fmt(x),
                y = fmt(y + 4.0),
                n = sequence_number_text,
            );
        }

        let _ = (from, to);
        ctx.checkpoints.checkpoint()?;
    }
    ctx.checkpoints.checkpoint()
}

fn round_sequence_number(value: f64) -> f64 {
    (value * 100.0).round() / 100.0
}

fn format_sequence_number(value: f64) -> String {
    if value.is_finite() {
        value.to_string()
    } else {
        String::new()
    }
}

#[derive(Clone, Copy)]
struct SequenceMessageTextLayout<'a> {
    label_y: f64,
    label_x: f64,
    label_anchor: &'a str,
    margin: f64,
    style: &'a TextStyle,
    measurer: &'a dyn TextMeasurer,
    config: &'a merman_core::MermaidConfig,
}

fn render_sequence_message_text_lines<'a>(
    out: &mut String,
    raw_lines: impl IntoIterator<Item = &'a str>,
    layout: SequenceMessageTextLayout<'_>,
    checkpoints: SequenceEmitCheckpoints<'_>,
) -> Result<()> {
    let drawn_style = sequence_drawn_text_style(layout.style, layout.config);
    let css = super::settings::sequence_text_style_attribute(layout.style);
    let mut preceding_height = 0.0;
    for (i, raw) in raw_lines.into_iter().enumerate() {
        checkpoints.checkpoint_loop(i)?;
        let y = sequence_drawn_text_y(layout.label_y, layout.margin, preceding_height);
        let decoded = merman_core::entities::decode_mermaid_entities_to_unicode(raw);
        let line = if decoded.as_ref().is_empty() {
            "\u{200B}"
        } else {
            decoded.as_ref()
        };
        let _ = write!(
            out,
            r#"<text x="{x}" y="{y}" text-anchor="{anchor}" dominant-baseline="middle" alignment-baseline="middle" class="messageText" dy="1em" style="{css}">{text}</text>"#,
            x = fmt(layout.label_x.round()),
            y = fmt(y),
            anchor = layout.label_anchor,
            css = escape_attr(&css),
            text = escape_xml(line)
        );
        if layout.margin > 0.0 {
            preceding_height += measure_sequence_drawn_line_height(
                layout.measurer,
                line,
                &drawn_style,
                SequenceDrawnTextNode::Direct,
                checkpoints.text(),
            )?;
        }
    }
    checkpoints.checkpoint()
}

#[cfg(test)]
mod tests {
    use super::render_sequence_message_text_lines;
    use crate::Error;
    use crate::resources::{OperationWorkMeter, RenderResourcePolicy};
    use merman_core::{OperationControl, OperationPhase};

    struct CancellingLines {
        control: OperationControl,
        index: usize,
        len: usize,
    }

    impl Iterator for CancellingLines {
        type Item = &'static str;

        fn next(&mut self) -> Option<Self::Item> {
            if self.index >= self.len {
                return None;
            }
            if self.index == 64 {
                self.control.cancel();
            }
            self.index += 1;
            Some("message")
        }
    }

    #[test]
    fn message_text_emit_loop_observes_mid_loop_cancellation() {
        let control = OperationControl::new();
        let meter = OperationWorkMeter::new_with_control(
            RenderResourcePolicy::unbounded_for_trusted_input(),
            control.clone(),
        );
        let checkpoints = super::SequenceEmitCheckpoints::new(&meter);
        let lines = CancellingLines {
            control,
            index: 0,
            len: 130,
        };
        let mut out = String::new();

        let error = render_sequence_message_text_lines(
            &mut out,
            lines,
            super::SequenceMessageTextLayout {
                label_y: 10.0,
                label_x: 20.0,
                label_anchor: "middle",
                margin: 10.0,
                style: &crate::text::TextStyle::default(),
                measurer: &crate::text::DeterministicTextMeasurer::default(),
                config: &merman_core::MermaidConfig::default(),
            },
            checkpoints,
        )
        .unwrap_err();
        let Error::Cancelled(error) = error else {
            panic!("expected Sequence message emit cancellation");
        };

        assert_eq!(error.phase, OperationPhase::Emit);
        assert_eq!(out.matches("<text ").count(), 64);
    }
    #[derive(Default)]
    struct RowProbe {
        calls: std::cell::RefCell<Vec<String>>,
        cancel: Option<OperationControl>,
    }

    impl crate::text::TextMeasurer for RowProbe {
        fn measure(&self, _: &str, _: &crate::text::TextStyle) -> crate::text::TextMetrics {
            panic!("message rows must use the raw text height operation")
        }

        fn measure_svg_raw_text_bbox_height_px(
            &self,
            text: &str,
            style: &crate::text::TextStyle,
        ) -> f64 {
            assert_eq!(style.font_family.as_deref(), Some("ThemeFont"));
            self.calls.borrow_mut().push(text.to_string());
            if let Some(control) = &self.cancel {
                control.cancel();
            }
            match text {
                "first" => 10.4,
                "\u{200b}" => 20.4,
                "&" => 8.0,
                _ => panic!("unexpected row"),
            }
        }
    }

    #[test]
    fn message_rows_accumulate_raw_heights_after_decoding_and_preserve_explicit_dy() {
        let meter = OperationWorkMeter::new(RenderResourcePolicy::unbounded_for_trusted_input());
        let style = crate::text::TextStyle::default();
        let config = merman_core::MermaidConfig::from_value(serde_json::json!({
            "themeVariables": {"fontFamily": "ThemeFont"}
        }));
        for margin in [5.0, 0.0, -5.0] {
            let probe = RowProbe::default();
            let first_y = crate::sequence::sequence_drawn_text_first_y(10.25, margin);
            let mut out = String::new();
            render_sequence_message_text_lines(
                &mut out,
                ["first", "", "#38;"],
                super::SequenceMessageTextLayout {
                    label_y: first_y,
                    label_x: 20.0,
                    label_anchor: "middle",
                    margin,
                    style: &style,
                    measurer: &probe,
                    config: &config,
                },
                super::SequenceEmitCheckpoints::new(&meter),
            )
            .unwrap();
            let svg = format!("<svg>{out}</svg>");
            let document = roxmltree::Document::parse(&svg).unwrap();
            let ys: Vec<_> = document
                .descendants()
                .filter(|n| n.has_tag_name("text"))
                .map(|n| n.attribute("y").unwrap())
                .collect();
            assert_eq!(
                ys,
                if margin > 0.0 {
                    vec!["13", "23", "44"]
                } else {
                    vec!["10.25"; 3]
                }
            );
            assert_eq!(out.matches("dy=\"1em\"").count(), 3);
            assert!(out.contains("&amp;</text>"));
            if margin > 0.0 {
                assert_eq!(*probe.calls.borrow(), ["first", "\u{200b}", "&"]);
            } else {
                assert!(probe.calls.borrow().is_empty());
            }
        }
    }

    #[test]
    fn message_height_callback_cancellation_stops_before_the_next_row() {
        let control = OperationControl::new();
        let meter = OperationWorkMeter::new_with_control(
            RenderResourcePolicy::unbounded_for_trusted_input(),
            control.clone(),
        );
        let probe = RowProbe {
            cancel: Some(control),
            ..Default::default()
        };
        let config = merman_core::MermaidConfig::from_value(serde_json::json!({
            "themeVariables": {"fontFamily": "ThemeFont"}
        }));
        let mut out = String::new();
        let error = render_sequence_message_text_lines(
            &mut out,
            ["first", "first"],
            super::SequenceMessageTextLayout {
                label_y: 10.0,
                label_x: 20.0,
                label_anchor: "middle",
                margin: 5.0,
                style: &crate::text::TextStyle::default(),
                measurer: &probe,
                config: &config,
            },
            super::SequenceEmitCheckpoints::new(&meter),
        )
        .unwrap_err();
        let Error::Cancelled(error) = error else {
            panic!("expected cancellation");
        };
        assert_eq!(error.phase, OperationPhase::Emit);
        assert_eq!(probe.calls.borrow().len(), 1);
        assert_eq!(out.matches("<text ").count(), 1);
    }
}
