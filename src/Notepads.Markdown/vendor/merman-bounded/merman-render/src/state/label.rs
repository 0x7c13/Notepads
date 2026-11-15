use std::borrow::Cow;

use crate::entities::decode_mermaid_entities_for_render_text;
use crate::svg::scanner::find_tag_end;
use crate::text::{
    TextMeasurer, TextMetrics, TextStyle, WrapMode, html_has_soft_break_opportunity,
    measure_html_with_inline_styles, measure_markdown_with_inline_styles,
    mermaid_markdown_to_xhtml_label_fragment,
};

#[derive(Debug, Clone)]
pub(crate) struct StateLabelMeasurement {
    pub(crate) metrics: TextMetrics,
    pub(crate) uses_html_wrapping_table: bool,
}

fn state_label_xhtml(text: &str) -> String {
    let decoded = decode_mermaid_entities_for_render_text(text);
    mermaid_markdown_to_xhtml_label_fragment(decoded.as_ref(), true)
}

fn simple_xhtml_text(fragment: &str) -> Option<Cow<'_, str>> {
    let inner = fragment.strip_prefix("<p>")?.strip_suffix("</p>")?;
    if inner.contains('<') || inner.contains('>') {
        return None;
    }
    Some(merman_core::entities::decode_html_entities_to_unicode(
        inner,
    ))
}

fn escape_xml_attribute(value: &str) -> String {
    let decoded = merman_core::entities::decode_html_entities_to_unicode(value);
    let mut out = String::with_capacity(decoded.len());
    for ch in decoded.chars() {
        if !crate::xml::is_xml_1_0_char(ch) {
            continue;
        }
        match ch {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '"' => out.push_str("&quot;"),
            '\r' => out.push_str("&#13;"),
            '\n' => out.push_str("&#10;"),
            _ => out.push(ch),
        }
    }
    out
}

fn parse_img_attributes(tag: &str) -> Vec<(String, String)> {
    fn is_name_byte(byte: u8) -> bool {
        byte.is_ascii_alphanumeric() || matches!(byte, b':' | b'_' | b'-' | b'.')
    }

    let bytes = tag.as_bytes();
    let mut attributes = Vec::new();
    let mut i = tag.find(char::is_whitespace).unwrap_or(tag.len());
    while i < bytes.len() {
        while i < bytes.len() && (bytes[i].is_ascii_whitespace() || bytes[i] == b'/') {
            i += 1;
        }
        if i >= bytes.len() || bytes[i] == b'>' {
            break;
        }

        let name_start = i;
        while i < bytes.len() && is_name_byte(bytes[i]) {
            i += 1;
        }
        if name_start == i {
            i += 1;
            continue;
        }
        let name = tag[name_start..i].to_ascii_lowercase();
        while i < bytes.len() && bytes[i].is_ascii_whitespace() {
            i += 1;
        }

        let mut value = String::new();
        if i < bytes.len() && bytes[i] == b'=' {
            i += 1;
            while i < bytes.len() && bytes[i].is_ascii_whitespace() {
                i += 1;
            }
            if i < bytes.len() && matches!(bytes[i], b'\'' | b'"') {
                let quote = bytes[i];
                i += 1;
                let value_start = i;
                while i < bytes.len() && bytes[i] != quote {
                    i += 1;
                }
                value.push_str(&tag[value_start..i]);
                if i < bytes.len() {
                    i += 1;
                }
            } else {
                let value_start = i;
                while i < bytes.len()
                    && !bytes[i].is_ascii_whitespace()
                    && !matches!(bytes[i], b'/' | b'>')
                {
                    i += 1;
                }
                value.push_str(&tag[value_start..i]);
            }
        }
        attributes.push((name, value));
    }
    attributes
}

fn normalized_image_style(existing: Option<&str>, image_only: bool, font_size: f64) -> String {
    let mut declarations = existing
        .into_iter()
        .flat_map(|style| style.split(';'))
        .filter_map(|declaration| {
            let declaration = declaration.trim();
            let (name, value) = declaration.split_once(':')?;
            let name = name.trim().to_ascii_lowercase();
            if matches!(
                name.as_str(),
                "display" | "flex-direction" | "width" | "min-width" | "max-width"
            ) {
                return None;
            }
            Some((name, value.trim().to_string()))
        })
        .collect::<Vec<_>>();
    declarations.push(("display".to_string(), "flex".to_string()));
    declarations.push(("flex-direction".to_string(), "column".to_string()));
    if image_only {
        let width = font_size.max(1.0) * 5.0;
        let width = if width.fract().abs() < 1e-9 {
            format!("{width:.0}px")
        } else {
            format!("{width}px")
        };
        declarations.push(("min-width".to_string(), width.clone()));
        declarations.push(("max-width".to_string(), width));
    } else {
        declarations.push(("width".to_string(), "100%".to_string()));
    }

    declarations
        .into_iter()
        .map(|(name, value)| format!("{name}: {value};"))
        .collect::<Vec<_>>()
        .join(" ")
}

fn label_contains_only_img_tags(raw: &str) -> bool {
    let lower = raw.to_ascii_lowercase();
    let mut cursor = 0usize;
    let mut visible = String::new();
    while let Some(relative) = lower[cursor..].find("<img") {
        let start = cursor + relative;
        visible.push_str(&raw[cursor..start]);
        let Some(end) = find_tag_end(raw, start + 4) else {
            visible.push_str(&raw[start..]);
            return visible.trim().is_empty();
        };
        cursor = end + 1;
    }
    visible.push_str(&raw[cursor..]);
    visible.trim().is_empty()
}

fn normalize_img_tags(fragment: &str, image_style: Option<(&str, f64)>) -> String {
    if !fragment.to_ascii_lowercase().contains("<img") {
        return fragment.to_string();
    }

    let image_only = image_style.is_some_and(|(raw, _)| label_contains_only_img_tags(raw));
    let lower = fragment.to_ascii_lowercase();
    let mut out = String::with_capacity(fragment.len() + 64);
    let mut cursor = 0usize;
    while let Some(relative) = lower[cursor..].find("<img") {
        let start = cursor + relative;
        out.push_str(&fragment[cursor..start]);
        let Some(end) = find_tag_end(fragment, start + 4) else {
            out.push_str("&lt;");
            cursor = start + 1;
            continue;
        };
        let tag = &fragment[start + 1..=end];
        let mut attributes = parse_img_attributes(tag);
        if let Some((_, font_size)) = image_style {
            let existing = attributes
                .iter()
                .find(|(name, _)| name == "style")
                .map(|(_, value)| value.as_str());
            let style = normalized_image_style(existing, image_only, font_size);
            attributes.retain(|(name, _)| name != "style");
            attributes.push(("style".to_string(), style));
        }

        out.push_str("<img");
        for (name, value) in attributes {
            out.push(' ');
            out.push_str(&name);
            out.push_str("=\"");
            out.push_str(&escape_xml_attribute(&value));
            out.push('"');
        }
        out.push_str(" />");
        cursor = end + 1;
    }
    out.push_str(&fragment[cursor..]);
    out
}

pub(crate) fn state_node_label_xhtml(text: &str, font_size: f64) -> String {
    normalize_img_tags(&state_label_xhtml(text), Some((text, font_size)))
}

pub(crate) fn state_edge_label_xhtml(text: &str) -> String {
    normalize_img_tags(&state_label_xhtml(text), None)
}

pub(crate) fn measure_state_markdown_label(
    text: &str,
    measurer: &dyn TextMeasurer,
    style: &TextStyle,
    max_width: Option<f64>,
    wrap_mode: WrapMode,
) -> StateLabelMeasurement {
    let max_width = max_width.filter(|width| width.is_finite() && *width > 0.0);
    let decoded = decode_mermaid_entities_for_render_text(text);
    let fragment = state_label_xhtml(text);
    let simple_text = simple_xhtml_text(&fragment);
    let measure = |width: Option<f64>| match simple_text.as_deref() {
        Some(plain) if wrap_mode == WrapMode::HtmlLike => {
            let natural = measurer.measure_wrapped(plain, style, None, WrapMode::HtmlLike);
            let needs_breakable_wrap = width.is_some_and(|max_width| {
                natural.width > max_width && html_has_soft_break_opportunity(plain)
            });
            if needs_breakable_wrap {
                measure_html_with_inline_styles(
                    measurer,
                    &fragment,
                    style,
                    width,
                    WrapMode::HtmlLike,
                )
            } else {
                natural
            }
        }
        Some(plain) => measurer.measure_wrapped(plain, style, width, wrap_mode),
        None => match wrap_mode {
            WrapMode::HtmlLike => measure_html_with_inline_styles(
                measurer,
                &fragment,
                style,
                width,
                WrapMode::HtmlLike,
            ),
            WrapMode::SvgLike | WrapMode::SvgLikeSingleRun => measure_markdown_with_inline_styles(
                measurer,
                decoded.as_ref(),
                style,
                width,
                wrap_mode,
            ),
        },
    };

    let natural = measure(None);
    let mut metrics = measure(max_width);
    let uses_html_wrapping_table = wrap_mode == WrapMode::HtmlLike
        && max_width.is_some_and(|width| natural.width >= width - 1e-9);

    if let Some(max_width) = max_width
        && metrics.width > max_width + 1e-9
    {
        let expanded_width = metrics.width;
        let reflowed = measure(Some(expanded_width));
        metrics.width = metrics.width.max(reflowed.width);
        metrics.height = reflowed.height;
        metrics.line_count = reflowed.line_count;
    }

    StateLabelMeasurement {
        metrics,
        uses_html_wrapping_table,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::text::DeterministicTextMeasurer;

    #[test]
    fn node_xhtml_normalizes_sanitized_images_like_mermaid_label_helper() {
        let html = state_node_label_xhtml(
            "<a href='https://example.com'><code>note</code></a><br/>\n<img src=x>",
            16.0,
        );

        assert!(html.contains(
            r#"<img src="x" style="display: flex; flex-direction: column; width: 100%;" />"#
        ));
        roxmltree::Document::parse(&format!("<root>{html}</root>"))
            .expect("State label fragment must be valid XHTML");
    }

    #[test]
    fn elk_edge_breaks_follow_mermaid_regex_before_markdown() {
        assert_eq!(
            state_edge_label_xhtml(&crate::text::mermaid_html_breaks_to_newlines(
                "Hello<br/>- l1<br/>- l2"
            )),
            "<p>Hello</p>- l1 - l2"
        );
    }

    #[test]
    fn edge_xhtml_keeps_trailing_underscores_literal() {
        assert_eq!(
            state_edge_label_xhtml("Transition1_____"),
            "<p>Transition1_____</p>"
        );
    }

    #[test]
    fn simple_html_cjk_label_uses_unicode_soft_break_opportunities() {
        let measurer = DeterministicTextMeasurer::default();
        let style = TextStyle {
            font_family: Some("\"trebuchet ms\", verdana, arial, sans-serif".to_string()),
            font_size: 16.0,
            font_weight: None,
            font_style: None,
        };
        let max_width = measurer
            .measure_wrapped("负责", &style, None, WrapMode::HtmlLike)
            .width;

        let measurement = measure_state_markdown_label(
            "负责人审批",
            &measurer,
            &style,
            Some(max_width),
            WrapMode::HtmlLike,
        );

        assert_eq!(measurement.metrics.line_count, 3, "{measurement:?}");
        assert!(measurement.metrics.width <= max_width, "{measurement:?}");
    }
}
