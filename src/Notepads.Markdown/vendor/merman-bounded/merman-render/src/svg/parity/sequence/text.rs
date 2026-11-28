use super::super::*;
use super::SequenceEmitCheckpoints;

/// Mirrors svgDraw's byTspan branch, shared by actor labels and box titles.
pub(super) fn write_centered_tspan_text(
    out: &mut String,
    cx: f64,
    cy: f64,
    label: &str,
    class: &str,
    text_style: &TextStyle,
    checkpoints: SequenceEmitCheckpoints<'_>,
) -> Result<()> {
    checkpoints.checkpoint()?;
    // Split before decoding so escaped <br> remains literal text.
    let lines = crate::text::split_html_br_lines(label);
    let count = lines.len().max(1) as f64;
    let mut style = format!(
        "text-anchor: middle; font-size: {}px;",
        fmt(text_style.font_size)
    );
    if let Some(weight) = &text_style.font_weight {
        let _ = write!(style, " font-weight: {weight};");
    }
    if let Some(family) = crate::sequence::sequence_inline_font_family(text_style) {
        let _ = write!(style, " font-family: {family};");
    }
    for (i, raw) in lines.into_iter().enumerate() {
        checkpoints.checkpoint_loop(i)?;
        let decoded = merman_core::entities::decode_mermaid_entities_to_unicode(raw);
        let dy = (i as f64 - (count - 1.0) / 2.0) * text_style.font_size;
        let _ = write!(
            out,
            r#"<text x="{x}" y="{y}" dominant-baseline="central" alignment-baseline="central" class="{class}" style="{style}"><tspan x="{x}" dy="{dy}">{text}</tspan></text>"#,
            x = fmt(cx),
            y = fmt(cy),
            class = escape_attr(class),
            style = escape_attr(&style),
            dy = fmt(dy),
            text = escape_xml_display(decoded.as_ref())
        );
    }
    checkpoints.checkpoint()
}
