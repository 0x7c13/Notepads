use super::super::*;
use super::SequenceEmitCheckpoints;
use super::geometry::node_left_top;
use super::math_label::{sequence_katex_label, write_sequence_katex_foreign_object};
use crate::sequence::{
    SequenceDrawnTextNode, SequenceMathHeightMode, measure_sequence_drawn_line_height,
    sequence_drawn_text_first_y, sequence_drawn_text_style, sequence_drawn_text_y,
    sequence_note_final_wrapped_lines,
};
use merman_core::diagrams::sequence::{SequenceMessage, SequenceMessageKind};
use rustc_hash::FxHashMap;

pub(super) struct SequenceNoteRenderContext<'a> {
    pub(super) nodes_by_id: &'a FxHashMap<&'a str, &'a LayoutNode>,
    pub(super) measurer: &'a dyn TextMeasurer,
    pub(super) note_margin: f64,
    pub(super) wrap_padding: f64,
    pub(super) note_text_style: &'a TextStyle,
    pub(super) sanitize_config: &'a merman_core::MermaidConfig,
    pub(super) math_renderer: Option<&'a (dyn crate::math::MathRenderer + Send + Sync)>,
    pub(super) checkpoints: SequenceEmitCheckpoints<'a>,
}

pub(super) fn render_sequence_note(
    out: &mut String,
    msg: &SequenceMessage,
    ctx: &SequenceNoteRenderContext<'_>,
) -> Result<()> {
    if msg.semantic_kind() != SequenceMessageKind::Note {
        return Ok(());
    }
    ctx.checkpoints.checkpoint()?;

    let id = &msg.id;
    let raw = msg.message_text();
    let node_id = format!("note-{id}");
    let Some(n) = ctx.nodes_by_id.get(node_id.as_str()).copied() else {
        return Ok(());
    };
    let (x, y) = node_left_top(n);
    let cx = x + (n.width / 2.0);
    let text_y = sequence_drawn_text_first_y(y, ctx.note_margin);
    let _ = write!(out, r#"<g data-et="note" data-id="i{}">"#, escape_attr(id));
    let _ = write!(
        &mut *out,
        r##"<rect x="{x}" y="{y}" fill="#EDF2AE" stroke="#666" width="{w}" height="{h}" class="note"{look_attr}/>"##,
        x = fmt(x),
        y = fmt(y),
        w = fmt(n.width),
        h = fmt(n.height),
        look_attr = if crate::config::config_diagram_look(ctx.sanitize_config.as_value()).as_str()
            == "neo"
        {
            r#" data-look="neo""#
        } else {
            ""
        },
    );
    if let Some(katex) = sequence_katex_label(
        raw,
        ctx.measurer,
        ctx.note_text_style,
        ctx.sanitize_config,
        ctx.math_renderer,
        SequenceMathHeightMode::Draw,
        ctx.checkpoints,
    )? {
        write_sequence_katex_foreign_object(
            out,
            &katex,
            (x + n.width / 2.0 - katex.width / 2.0).round(),
            (y + n.height / 2.0 - katex.height / 2.0).round(),
        );
    } else if msg.wrap {
        // Mermaid@11.12.2 (Sequence) wraps notes *after* placement width is known:
        //   noteModel.message = wrapLabel(msg.message, noteModel.width - 2*wrapPadding, noteFont)
        //
        // Layout already computed the note box width (`n.width`) to match Mermaid's
        // `noteModel.width`, so wrap to `n.width - 2*wrapPadding` here.
        let lines = sequence_note_final_wrapped_lines(
            raw,
            n.width,
            2.0 * ctx.wrap_padding,
            ctx.measurer,
            ctx.note_text_style,
            ctx.checkpoints.text(),
        )?;
        render_sequence_note_lines(out, lines.iter().map(String::as_str), cx, text_y, ctx)?;
    } else {
        render_sequence_note_lines(out, crate::text::split_html_br_lines(raw), cx, text_y, ctx)?;
    }
    out.push_str("</g>");
    ctx.checkpoints.checkpoint()
}

fn render_sequence_note_lines<'a>(
    out: &mut String,
    lines: impl IntoIterator<Item = &'a str>,
    cx: f64,
    text_y: f64,
    ctx: &SequenceNoteRenderContext<'_>,
) -> Result<()> {
    let drawn_style = sequence_drawn_text_style(ctx.note_text_style, ctx.sanitize_config);
    let css = super::settings::sequence_text_style_attribute(ctx.note_text_style);
    let mut preceding_height = 0.0;
    for (i, line) in lines.into_iter().enumerate() {
        ctx.checkpoints.checkpoint_loop(i)?;
        let decoded = merman_core::entities::decode_mermaid_entities_to_unicode(line);
        let text = if decoded.as_ref().is_empty() {
            "\u{200B}"
        } else {
            decoded.as_ref()
        };
        let y = sequence_drawn_text_y(text_y, ctx.note_margin, preceding_height);
        let _ = write!(
            &mut *out,
            r#"<text x="{x}" y="{y}" text-anchor="middle" dominant-baseline="middle" alignment-baseline="middle" class="noteText" dy="1em" style="{css}"><tspan x="{x}">{text}</tspan></text>"#,
            x = fmt(cx),
            y = fmt(y),
            css = escape_attr(&css),
            text = escape_xml(text)
        );
        if ctx.note_margin > 0.0 {
            preceding_height += measure_sequence_drawn_line_height(
                ctx.measurer,
                text,
                &drawn_style,
                SequenceDrawnTextNode::Tspan,
                ctx.checkpoints.text(),
            )?;
        }
    }
    ctx.checkpoints.checkpoint()
}

#[cfg(test)]
mod tests {
    use crate::resources::{OperationWorkMeter, RenderResourcePolicy};

    #[test]
    fn empty_note_rows_render_the_upstream_zero_width_space() {
        let mut out = String::new();
        let meter = OperationWorkMeter::new(RenderResourcePolicy::unbounded_for_trusted_input());
        super::render_sequence_note_lines(
            &mut out,
            ["first", "", "last"],
            50.0,
            10.0,
            &super::SequenceNoteRenderContext {
                nodes_by_id: &Default::default(),
                measurer: &crate::text::DeterministicTextMeasurer::default(),
                note_margin: 10.0,
                wrap_padding: 10.0,
                note_text_style: &crate::text::TextStyle::default(),
                sanitize_config: &merman_core::MermaidConfig::default(),
                math_renderer: None,
                checkpoints: super::SequenceEmitCheckpoints::new(&meter),
            },
        )
        .unwrap();

        assert!(out.contains("<tspan x=\"50\">\u{200b}</tspan>"), "{out}");
    }
    #[test]
    fn note_rows_use_tspan_heights_and_keep_explicit_dy_at_nonpositive_margins() {
        struct TspanProbe(std::cell::Cell<usize>);
        impl crate::text::TextMeasurer for TspanProbe {
            fn measure(&self, _: &str, _: &crate::text::TextStyle) -> crate::text::TextMetrics {
                panic!("notes must use the tspan height operation")
            }
            fn measure_svg_tspan_text_bbox_height_px(
                &self,
                text: &str,
                style: &crate::text::TextStyle,
            ) -> f64 {
                assert_eq!(style.font_weight.as_deref(), Some("700"));
                self.0.set(self.0.get() + 1);
                match text {
                    "first" => 10.4,
                    "\u{200b}" => 20.4,
                    "&" => 8.0,
                    _ => panic!("unexpected row"),
                }
            }
        }
        let meter = OperationWorkMeter::new(RenderResourcePolicy::unbounded_for_trusted_input());
        let settings = super::super::settings::SequenceRenderSettings::from_effective_config(
            &serde_json::json!({"sequence": {"noteFontWeight": 700}}),
        );
        for margin in [5.0, 0.0, -5.0] {
            let probe = TspanProbe(std::cell::Cell::new(0));
            let mut out = String::new();
            super::render_sequence_note_lines(
                &mut out,
                ["first", "", "#38;"],
                50.0,
                crate::sequence::sequence_drawn_text_first_y(10.25, margin),
                &super::SequenceNoteRenderContext {
                    nodes_by_id: &Default::default(),
                    measurer: &probe,
                    note_margin: margin,
                    wrap_padding: 10.0,
                    note_text_style: &settings.note_text_style,
                    sanitize_config: &merman_core::MermaidConfig::default(),
                    math_renderer: None,
                    checkpoints: super::SequenceEmitCheckpoints::new(&meter),
                },
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
            for node in document
                .descendants()
                .filter(|node| node.has_tag_name("text"))
            {
                assert!(
                    node.attribute("style")
                        .unwrap()
                        .contains("font-weight: 700;")
                );
            }
            assert_eq!(out.matches("dy=\"1em\"").count(), 3);
            assert!(out.contains("&amp;</tspan>"));
            assert_eq!(probe.0.get(), if margin > 0.0 { 3 } else { 0 });
        }
    }
}
