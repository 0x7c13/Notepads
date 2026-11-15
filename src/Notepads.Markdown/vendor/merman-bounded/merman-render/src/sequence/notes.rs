use super::SequenceLayoutCheckpoints;
use super::constants::SEQUENCE_FRAME_GEOM_PAD_PX;
use super::metrics::{
    SequenceDrawnTextNode, SequenceMathHeightMode, measure_drawn_svg_like_with_html_br,
    measure_sequence_label_for_layout, measure_svg_like_with_html_br,
    wrap_sequence_label_like_mermaid_lines,
};
use crate::Result;
use crate::math::MathRenderer;
use crate::model::LayoutNode;
use crate::text::{TextMeasurer, TextStyle};
use merman_core::MermaidConfig;
use merman_core::diagrams::sequence::SequenceMessage;
use std::collections::HashMap;

#[derive(Debug, Clone, PartialEq)]
pub(super) struct SequenceNoteHorizontalModel {
    pub(super) start_x: f64,
    pub(super) width: f64,
    pub(super) effective_text: String,
}

#[derive(Clone, Copy)]
pub(super) struct SequenceNoteHorizontalContext<'a> {
    pub(super) actor_index: &'a HashMap<&'a str, usize>,
    pub(super) actor_centers_x: &'a [f64],
    pub(super) actor_widths: &'a [f64],
    pub(super) actor_margin: f64,
    pub(super) note_default_width: f64,
    pub(super) note_margin: f64,
    pub(super) wrap_padding: f64,
    pub(super) measurer: &'a dyn TextMeasurer,
    pub(super) note_text_style: &'a TextStyle,
    pub(super) math_config: &'a MermaidConfig,
    pub(super) math_renderer: Option<&'a (dyn MathRenderer + Send + Sync)>,
    pub(super) checkpoints: SequenceLayoutCheckpoints<'a>,
}

#[derive(Debug, Clone, Copy)]
struct NoteHorizontalRequest {
    placement: i32,
    is_self: bool,
    from_center_x: f64,
    to_center_x: f64,
    from_width: f64,
    to_width: f64,
    actor_margin: f64,
    default_width: f64,
    note_margin: f64,
    should_wrap: bool,
    text_width: f64,
}

#[derive(Debug, Clone, Copy, PartialEq)]
struct NoteHorizontalGeometry {
    start_x: f64,
    width: f64,
}

pub(super) fn sequence_note_horizontal_model(
    msg: &SequenceMessage,
    ctx: SequenceNoteHorizontalContext<'_>,
) -> Result<Option<SequenceNoteHorizontalModel>> {
    let (Some(from), Some(to)) = (msg.from.as_deref(), msg.to.as_deref()) else {
        return Ok(None);
    };
    let (Some(from_index), Some(to_index)) = (
        ctx.actor_index.get(from).copied(),
        ctx.actor_index.get(to).copied(),
    ) else {
        return Ok(None);
    };
    let (Some(from_center_x), Some(to_center_x)) = (
        ctx.actor_centers_x.get(from_index).copied(),
        ctx.actor_centers_x.get(to_index).copied(),
    ) else {
        return Ok(None);
    };
    let (Some(from_width), Some(to_width)) = (
        ctx.actor_widths.get(from_index).copied(),
        ctx.actor_widths.get(to_index).copied(),
    ) else {
        return Ok(None);
    };
    let text = msg.message_text();
    let should_wrap = msg.wrap && !text.is_empty();
    let is_math_note = text.contains("$$");
    let placement = msg.placement.unwrap_or(2);
    let mut text_width = if is_math_note {
        measure_sequence_label_for_layout(
            ctx.measurer,
            text,
            ctx.note_text_style,
            ctx.math_config,
            ctx.math_renderer,
            SequenceMathHeightMode::Bound,
            ctx.checkpoints.text(),
        )?
        .0
    } else if should_wrap {
        let wrapped = wrap_sequence_label_like_mermaid_lines(
            text,
            ctx.measurer,
            ctx.note_text_style,
            ctx.note_default_width.max(1.0),
            ctx.checkpoints.text(),
        )?
        .join("<br/>");
        measure_svg_like_with_html_br(
            ctx.measurer,
            &wrapped,
            ctx.note_text_style,
            ctx.checkpoints.text(),
        )?
        .0
    } else {
        measure_svg_like_with_html_br(
            ctx.measurer,
            text,
            ctx.note_text_style,
            ctx.checkpoints.text(),
        )?
        .0
    }
    .max(0.0);
    if !matches!(placement, 0 | 1) && from == to {
        let measured_text = if should_wrap {
            wrap_sequence_label_like_mermaid_lines(
                text,
                ctx.measurer,
                ctx.note_text_style,
                ctx.note_default_width.max(from_width).max(1.0),
                ctx.checkpoints.text(),
            )?
            .join("<br/>")
        } else {
            text.to_string()
        };
        text_width = measure_svg_like_with_html_br(
            ctx.measurer,
            &measured_text,
            ctx.note_text_style,
            ctx.checkpoints.text(),
        )?
        .0;
    }
    let geometry = note_horizontal_model_from_request(NoteHorizontalRequest {
        placement,
        is_self: from == to,
        from_center_x,
        to_center_x,
        from_width,
        to_width,
        actor_margin: ctx.actor_margin,
        default_width: ctx.note_default_width,
        note_margin: ctx.note_margin,
        should_wrap,
        text_width,
    });
    let effective_text = if should_wrap {
        sequence_note_final_wrapped_lines(
            text,
            geometry.width,
            2.0 * ctx.wrap_padding,
            ctx.measurer,
            ctx.note_text_style,
            ctx.checkpoints.text(),
        )?
        .join("<br/>")
    } else {
        text.to_string()
    };

    Ok(Some(SequenceNoteHorizontalModel {
        start_x: geometry.start_x,
        width: geometry.width,
        effective_text,
    }))
}

fn note_horizontal_model_from_request(req: NoteHorizontalRequest) -> NoteHorizontalGeometry {
    match req.placement {
        1 => NoteHorizontalGeometry {
            width: if req.should_wrap {
                req.default_width.max(req.text_width)
            } else {
                (req.from_width / 2.0 + req.to_width / 2.0)
                    .max(req.text_width + 2.0 * req.note_margin)
            },
            start_x: req.from_center_x + req.actor_margin / 2.0,
        },
        0 => {
            let width = if req.should_wrap {
                req.default_width
                    .max(req.text_width + 2.0 * req.note_margin)
            } else {
                (req.from_width / 2.0 + req.to_width / 2.0)
                    .max(req.text_width + 2.0 * req.note_margin)
            };
            NoteHorizontalGeometry {
                start_x: req.from_center_x - width - req.actor_margin / 2.0,
                width,
            }
        }
        _ if req.is_self => {
            let width = if req.should_wrap {
                req.default_width.max(req.from_width)
            } else {
                req.from_width
                    .max(req.default_width)
                    .max(req.text_width + 2.0 * req.note_margin)
            };
            NoteHorizontalGeometry {
                start_x: req.from_center_x - width / 2.0,
                width,
            }
        }
        _ => NoteHorizontalGeometry {
            start_x: req.from_center_x.min(req.to_center_x) - req.actor_margin / 2.0,
            width: (req.from_center_x - req.to_center_x).abs() + req.actor_margin,
        },
    }
}

pub(super) struct SequenceNoteLayoutContext<'a> {
    pub(super) actor_index: &'a HashMap<&'a str, usize>,
    pub(super) actor_centers_x: &'a [f64],
    pub(super) actor_widths: &'a [f64],
    pub(super) actor_margin: f64,
    pub(super) note_default_width: f64,
    pub(super) note_margin: f64,
    pub(super) wrap_padding: f64,
    pub(super) box_margin: f64,
    pub(super) cursor_y: f64,
    pub(super) measurer: &'a dyn TextMeasurer,
    pub(super) note_text_style: &'a TextStyle,
    pub(super) math_config: &'a MermaidConfig,
    pub(super) math_renderer: Option<&'a (dyn MathRenderer + Send + Sync)>,
    pub(super) checkpoints: SequenceLayoutCheckpoints<'a>,
}

pub(super) struct SequenceNoteLayout {
    pub(super) node: LayoutNode,
    pub(super) rect_min_x: f64,
    pub(super) rect_max_x: f64,
    pub(super) rect_max_y: f64,
    pub(super) cursor_step: f64,
}

pub(super) fn layout_sequence_note(
    msg: &SequenceMessage,
    ctx: SequenceNoteLayoutContext<'_>,
) -> Result<Option<SequenceNoteLayout>> {
    let Some(horizontal) = sequence_note_horizontal_model(
        msg,
        SequenceNoteHorizontalContext {
            actor_index: ctx.actor_index,
            actor_centers_x: ctx.actor_centers_x,
            actor_widths: ctx.actor_widths,
            actor_margin: ctx.actor_margin,
            note_default_width: ctx.note_default_width,
            note_margin: ctx.note_margin,
            wrap_padding: ctx.wrap_padding,
            measurer: ctx.measurer,
            note_text_style: ctx.note_text_style,
            math_config: ctx.math_config,
            math_renderer: ctx.math_renderer,
            checkpoints: ctx.checkpoints,
        },
    )?
    else {
        return Ok(None);
    };
    let is_math_note = horizontal.effective_text.contains("$$");
    let (_, height) = if is_math_note {
        measure_sequence_label_for_layout(
            ctx.measurer,
            &horizontal.effective_text,
            ctx.note_text_style,
            ctx.math_config,
            ctx.math_renderer,
            SequenceMathHeightMode::Draw,
            ctx.checkpoints.text(),
        )?
    } else {
        measure_drawn_svg_like_with_html_br(
            ctx.measurer,
            &horizontal.effective_text,
            ctx.note_text_style,
            SequenceDrawnTextNode::Tspan,
            ctx.math_config,
            ctx.checkpoints.text(),
        )?
    };
    let note_x = horizontal.start_x;
    let note_w = horizontal.width;
    let note_h = (height.round() + 2.0 * ctx.note_margin).max(1.0);
    let note_y = ctx.cursor_y + ctx.box_margin;

    Ok(Some(SequenceNoteLayout {
        node: LayoutNode {
            id: format!("note-{}", msg.id),
            x: note_x + note_w / 2.0,
            y: note_y + note_h / 2.0,
            width: note_w.max(1.0),
            height: note_h,
            is_cluster: false,
            label_width: None,
            label_height: None,
        },
        rect_min_x: note_x - SEQUENCE_FRAME_GEOM_PAD_PX,
        rect_max_x: note_x + note_w + SEQUENCE_FRAME_GEOM_PAD_PX,
        rect_max_y: note_y + note_h,
        cursor_step: ctx.box_margin + note_h,
    }))
}

pub(crate) fn sequence_note_final_wrapped_lines(
    text: &str,
    note_width: f64,
    note_text_pad_total: f64,
    measurer: &dyn TextMeasurer,
    note_text_style: &TextStyle,
    checkpoints: super::SequenceTextCheckpoints<'_>,
) -> Result<Vec<String>> {
    let wrap_w = (note_width - note_text_pad_total).max(1.0);
    wrap_sequence_label_like_mermaid_lines(text, measurer, note_text_style, wrap_w, checkpoints)
}

#[cfg(test)]
mod tests {
    use super::{
        NoteHorizontalRequest, SequenceNoteHorizontalContext, note_horizontal_model_from_request,
        sequence_note_horizontal_model,
    };
    use crate::resources::{OperationWorkMeter, RenderResourcePolicy};
    use crate::text::{TextMeasurer, TextMetrics, TextStyle};
    use merman_core::MermaidConfig;
    use merman_core::diagrams::sequence::{SequenceMessage, SequenceMessagePayload};
    use std::collections::HashMap;

    #[test]
    fn note_horizontal_model_uses_actor_margin_and_actor_widths() {
        let base = NoteHorizontalRequest {
            placement: 1,
            is_self: false,
            from_center_x: 100.0,
            to_center_x: 200.0,
            from_width: 80.0,
            to_width: 120.0,
            actor_margin: 70.0,
            default_width: 150.0,
            note_margin: 10.0,
            should_wrap: false,
            text_width: 30.0,
        };

        let right = note_horizontal_model_from_request(base);
        let left = note_horizontal_model_from_request(NoteHorizontalRequest {
            placement: 0,
            ..base
        });
        let over = note_horizontal_model_from_request(NoteHorizontalRequest {
            placement: 2,
            ..base
        });
        let over_self = note_horizontal_model_from_request(NoteHorizontalRequest {
            placement: 2,
            is_self: true,
            to_center_x: 100.0,
            from_width: 220.0,
            to_width: 220.0,
            should_wrap: true,
            ..base
        });

        assert_eq!((right.start_x, right.width), (135.0, 100.0));
        assert_eq!((left.start_x, left.width), (-35.0, 100.0));
        assert_eq!((over.start_x, over.width), (65.0, 170.0));
        assert_eq!((over_self.start_x, over_self.width), (-10.0, 220.0));
    }

    struct FontSizeMeasurer;

    impl TextMeasurer for FontSizeMeasurer {
        fn measure(&self, _text: &str, style: &TextStyle) -> TextMetrics {
            TextMetrics {
                width: style.font_size,
                height: style.font_size,
                line_count: 1,
            }
        }
    }

    #[test]
    fn note_horizontal_model_measures_with_note_font() {
        let message = SequenceMessage {
            id: "note".to_string(),
            from: Some("A".to_string()),
            to: Some("B".to_string()),
            message_type: 2,
            message: SequenceMessagePayload::Text("font-sensitive".to_string()),
            wrap: false,
            activate: false,
            placement: Some(1),
            central_connection: 0,
        };
        let actor_index = HashMap::from([("A", 0), ("B", 1)]);
        let note_style = TextStyle {
            font_size: 90.0,
            ..TextStyle::default()
        };
        let meter = OperationWorkMeter::new(RenderResourcePolicy::unbounded_for_trusted_input());

        let model = sequence_note_horizontal_model(
            &message,
            SequenceNoteHorizontalContext {
                actor_index: &actor_index,
                actor_centers_x: &[100.0, 200.0],
                actor_widths: &[80.0, 120.0],
                actor_margin: 70.0,
                note_default_width: 150.0,
                note_margin: 10.0,
                wrap_padding: 10.0,
                measurer: &FontSizeMeasurer,
                note_text_style: &note_style,
                math_config: &MermaidConfig::default(),
                math_renderer: None,
                checkpoints: crate::sequence::SequenceLayoutCheckpoints::new(&meter),
            },
        )
        .unwrap()
        .unwrap();

        assert_eq!(model.width, 110.0);
        assert_eq!(model.start_x, 135.0);
    }
}
