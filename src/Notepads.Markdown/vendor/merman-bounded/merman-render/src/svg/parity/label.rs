//! Shared Mermaid `createText` SVG label emission.

use super::*;
use crate::svg::parity::util::escape_xml_serialized_text_into;

#[derive(Clone, Copy)]
enum SvgTextEntityMode {
    DecodedModel,
    CreateTextSource,
}

fn write_svg_text_inner_word(
    out: &mut String,
    word_index: usize,
    word: &str,
    entity_mode: SvgTextEntityMode,
) {
    out.push_str(r#"<tspan font-style="normal" class="text-inner-tspan" font-weight="normal">"#);
    if word_index > 0 {
        out.push(' ');
    }
    write_svg_text_word(out, word, entity_mode);
    out.push_str("</tspan>");
}

pub(in crate::svg::parity) fn write_svg_text_centered(
    out: &mut String,
    text: &str,
    include_style: bool,
) {
    write_svg_text_impl(
        out,
        text,
        include_style.then_some(""),
        true,
        true,
        SvgTextEntityMode::DecodedModel,
    );
}

pub(in crate::svg::parity) fn write_svg_text_centered_from_create_text_source(
    out: &mut String,
    text: &str,
    include_style: bool,
) {
    write_svg_text_impl(
        out,
        text,
        include_style.then_some(""),
        true,
        true,
        SvgTextEntityMode::CreateTextSource,
    );
}

pub(in crate::svg::parity) fn write_svg_text_source_word_lines(
    out: &mut String,
    lines: &[Vec<String>],
    include_style: bool,
    center_text: bool,
) {
    write_svg_text_source_word_lines_impl(out, lines, include_style.then_some(""), center_text);
}

pub(in crate::svg::parity) fn write_svg_text_source_word_lines_with_style(
    out: &mut String,
    lines: &[Vec<String>],
    style: &str,
    center_text: bool,
) {
    write_svg_text_source_word_lines_impl(out, lines, Some(style), center_text);
}

fn write_svg_text_source_word_lines_impl(
    out: &mut String,
    lines: &[Vec<String>],
    style: Option<&str>,
    center_text: bool,
) {
    open_svg_text(out, style, center_text);

    if lines.len() == 1 && lines[0].is_empty() {
        write_empty_tspan(out, center_text, true);
        out.push_str("</text>");
        return;
    }

    for (line_index, words) in lines.iter().enumerate() {
        open_tspan(out, line_index, center_text, true);
        for (word_index, word) in words.iter().enumerate() {
            write_svg_text_inner_word(out, word_index, word, SvgTextEntityMode::CreateTextSource);
        }
        out.push_str("</tspan>");
    }

    out.push_str("</text>");
}

fn open_svg_text(out: &mut String, style: Option<&str>, center_text: bool) {
    match (style, center_text) {
        (Some(style), true) => {
            let _ = write!(
                out,
                r#"<text y="-10.1" style="{}" text-anchor="middle">"#,
                escape_attr(style)
            );
        }
        (Some(style), false) => {
            let _ = write!(out, r#"<text y="-10.1" style="{}">"#, escape_attr(style));
        }
        (None, true) => out.push_str(r#"<text y="-10.1" text-anchor="middle">"#),
        (None, false) => out.push_str(r#"<text y="-10.1">"#),
    }
}

fn outer_tspan_class(include_row_class: bool) -> &'static str {
    if include_row_class {
        "row text-outer-tspan"
    } else {
        "text-outer-tspan"
    }
}

fn write_empty_tspan(out: &mut String, center_text: bool, include_row_class: bool) {
    let outer_class = outer_tspan_class(include_row_class);
    if center_text {
        let _ = write!(
            out,
            r#"<tspan class="{}" x="0" y="-0.1em" dy="1.1em" text-anchor="middle"/>"#,
            outer_class
        );
    } else {
        let _ = write!(
            out,
            r#"<tspan class="{}" x="0" y="-0.1em" dy="1.1em"/>"#,
            outer_class
        );
    }
}

fn open_tspan(out: &mut String, index: usize, center_text: bool, include_row_class: bool) {
    let text_anchor = if center_text {
        r#" text-anchor="middle""#
    } else {
        ""
    };
    let outer_class = outer_tspan_class(include_row_class);
    if index == 0 {
        let _ = write!(
            out,
            r#"<tspan class="{}" x="0" y="-0.1em" dy="1.1em"{}>"#,
            outer_class, text_anchor
        );
    } else {
        let y_em = if index == 1 {
            "1em".to_string()
        } else {
            format!("{:.1}em", 1.0 + (index as f64 - 1.0) * 1.1)
        };
        let _ = write!(
            out,
            r#"<tspan class="{}" x="0" y="{}" dy="1.1em"{}>"#,
            outer_class, y_em, text_anchor
        );
    }
}

fn write_svg_text_impl(
    out: &mut String,
    text: &str,
    css: Option<&str>,
    center_text: bool,
    include_row_class: bool,
    entity_mode: SvgTextEntityMode,
) {
    open_svg_text(out, css, center_text);

    let lines = crate::text::DeterministicTextMeasurer::normalized_text_lines(text);
    if lines.len() == 1 && lines[0].is_empty() {
        write_empty_tspan(out, center_text, include_row_class);
        out.push_str("</text>");
        return;
    }

    for (index, line) in lines.iter().enumerate() {
        open_tspan(out, index, center_text, include_row_class);
        for (word_index, word) in crate::text::non_markdown_svg_words(line).enumerate() {
            write_svg_text_inner_word(out, word_index, word, entity_mode);
        }
        out.push_str("</tspan>");
    }

    out.push_str("</text>");
}

fn write_svg_text_word(out: &mut String, word: &str, entity_mode: SvgTextEntityMode) {
    match entity_mode {
        SvgTextEntityMode::DecodedModel => escape_xml_into(out, word),
        SvgTextEntityMode::CreateTextSource => {
            let visible = crate::entities::decode_svg_text_content_entities(word);
            escape_xml_serialized_text_into(out, visible.as_ref());
        }
    }
}

fn normalized_markdown_label(markdown: &str) -> &str {
    markdown
        .strip_prefix('`')
        .and_then(|value| value.strip_suffix('`'))
        .unwrap_or(markdown)
}

fn markdown_to_svg_word_lines(markdown: &str) -> Vec<Vec<(String, bool, bool)>> {
    crate::text::mermaid_markdown_to_lines(markdown, true)
        .into_iter()
        .map(|line| {
            line.into_iter()
                .map(|(word, kind)| {
                    let is_strong = kind == crate::text::MermaidMarkdownWordType::Strong;
                    let is_em = kind == crate::text::MermaidMarkdownWordType::Em;
                    (word, is_strong, is_em)
                })
                .collect()
        })
        .collect()
}

fn markdown_to_wrapped_svg_word_lines(
    measurer: &dyn crate::text::TextMeasurer,
    markdown: &str,
    style: &crate::text::TextStyle,
    max_width_px: Option<f64>,
) -> Vec<Vec<(String, bool, bool)>> {
    crate::text::mermaid_markdown_to_wrapped_word_lines(
        measurer,
        markdown,
        style,
        max_width_px,
        crate::text::WrapMode::SvgLike,
    )
    .into_iter()
    .map(|line| {
        line.into_iter()
            .map(|(word, kind)| {
                let is_strong = kind == crate::text::MermaidMarkdownWordType::Strong;
                let is_em = kind == crate::text::MermaidMarkdownWordType::Em;
                (word, is_strong, is_em)
            })
            .collect()
    })
    .collect()
}

fn write_svg_text_markdown_lines(
    out: &mut String,
    lines: &[Vec<(String, bool, bool)>],
    css: Option<&str>,
    center_text: bool,
    include_row_class: bool,
    entity_mode: SvgTextEntityMode,
) {
    open_svg_text(out, css, center_text);

    if lines.len() == 1 && lines[0].is_empty() {
        write_empty_tspan(out, center_text, include_row_class);
        out.push_str("</text>");
        return;
    }

    for (index, words) in lines.iter().enumerate() {
        open_tspan(out, index, center_text, include_row_class);

        for (word_index, (word, is_strong, is_em)) in words.iter().enumerate() {
            let font_style = if *is_em { "italic" } else { "normal" };
            let font_weight = if *is_strong { "bold" } else { "normal" };
            let _ = write!(
                out,
                r#"<tspan font-style="{}" class="text-inner-tspan" font-weight="{}">"#,
                font_style, font_weight
            );
            if word_index == 0 {
                write_svg_text_word(out, word, entity_mode);
            } else {
                out.push(' ');
                write_svg_text_word(out, word, entity_mode);
            }
            out.push_str("</tspan>");
        }

        out.push_str("</tspan>");
    }

    out.push_str("</text>");
}

pub(in crate::svg::parity) fn write_svg_text_markdown(
    out: &mut String,
    markdown: &str,
    include_style: bool,
) {
    let lines = markdown_to_svg_word_lines(normalized_markdown_label(markdown));
    write_svg_text_markdown_lines(
        out,
        &lines,
        include_style.then_some(""),
        false,
        true,
        SvgTextEntityMode::DecodedModel,
    );
}

pub(in crate::svg::parity) fn write_svg_text_markdown_centered(
    out: &mut String,
    markdown: &str,
    include_style: bool,
) {
    let lines = markdown_to_svg_word_lines(normalized_markdown_label(markdown));
    write_svg_text_markdown_lines(
        out,
        &lines,
        include_style.then_some(""),
        true,
        true,
        SvgTextEntityMode::DecodedModel,
    );
}

pub(in crate::svg::parity) fn write_svg_text_markdown_from_create_text_source(
    out: &mut String,
    markdown: &str,
    include_style: bool,
) {
    let lines = markdown_to_svg_word_lines(normalized_markdown_label(markdown));
    write_svg_text_markdown_lines(
        out,
        &lines,
        include_style.then_some(""),
        false,
        true,
        SvgTextEntityMode::CreateTextSource,
    );
}

pub(in crate::svg::parity) fn write_svg_text_markdown_wrapped_centered_from_create_text_source(
    out: &mut String,
    markdown: &str,
    include_style: bool,
    measurer: &dyn crate::text::TextMeasurer,
    style: &crate::text::TextStyle,
    max_width_px: Option<f64>,
) {
    let lines = markdown_to_wrapped_svg_word_lines(
        measurer,
        normalized_markdown_label(markdown),
        style,
        max_width_px,
    );
    write_svg_text_markdown_lines(
        out,
        &lines,
        include_style.then_some(""),
        true,
        true,
        SvgTextEntityMode::CreateTextSource,
    );
}

pub(in crate::svg::parity) fn write_svg_text_markdown_wrapped_from_create_text_source(
    out: &mut String,
    markdown: &str,
    include_style: bool,
    measurer: &dyn crate::text::TextMeasurer,
    style: &crate::text::TextStyle,
    max_width_px: Option<f64>,
) {
    let lines = markdown_to_wrapped_svg_word_lines(
        measurer,
        normalized_markdown_label(markdown),
        style,
        max_width_px,
    );
    write_svg_text_markdown_lines(
        out,
        &lines,
        include_style.then_some(""),
        false,
        true,
        SvgTextEntityMode::CreateTextSource,
    );
}

pub(in crate::svg::parity) fn write_svg_text_centered_from_create_text_source_with_style(
    out: &mut String,
    text: &str,
    css: &str,
) {
    write_svg_text_impl(
        out,
        text,
        Some(css),
        true,
        true,
        SvgTextEntityMode::CreateTextSource,
    );
}

pub(in crate::svg::parity) fn write_svg_text_markdown_wrapped_centered_with_style(
    out: &mut String,
    markdown: &str,
    css: &str,
    measurer: &dyn crate::text::TextMeasurer,
    style: &crate::text::TextStyle,
    max_width_px: Option<f64>,
) {
    let lines = markdown_to_wrapped_svg_word_lines(
        measurer,
        normalized_markdown_label(markdown),
        style,
        max_width_px,
    );
    write_svg_text_markdown_lines(
        out,
        &lines,
        Some(css),
        true,
        true,
        SvgTextEntityMode::CreateTextSource,
    );
}

/// Emit a node's wrapped Markdown label with the same explicit font style used for measurement.
pub(in crate::svg::parity) fn write_svg_text_markdown_wrapped_with_style(
    out: &mut String,
    markdown: &str,
    css: &str,
    measurer: &dyn crate::text::TextMeasurer,
    style: &crate::text::TextStyle,
    max_width_px: Option<f64>,
) {
    let mut lines = markdown_to_wrapped_svg_word_lines(
        measurer,
        normalized_markdown_label(markdown),
        style,
        max_width_px,
    );
    if style.font_weight.as_deref() == Some("bold") {
        for line in &mut lines {
            for (_, is_strong, _) in line {
                *is_strong = true;
            }
        }
    }
    write_svg_text_markdown_lines(
        out,
        &lines,
        Some(css),
        false,
        true,
        SvgTextEntityMode::CreateTextSource,
    );
}

#[cfg(test)]
mod tests {
    use super::{write_svg_text_centered, write_svg_text_source_word_lines};

    #[test]
    fn non_markdown_svg_text_uses_ecmascript_whitespace_boundaries() {
        let expected = [
            r#"<tspan font-style="normal" class="text-inner-tspan" font-weight="normal">A</tspan>"#,
            r#"<tspan font-style="normal" class="text-inner-tspan" font-weight="normal"> B</tspan>"#,
        ];

        for text in ["A\tB", "A\u{00A0}B", "A\u{2000}B"] {
            let mut out = String::new();
            write_svg_text_centered(&mut out, text, false);
            for fragment in expected {
                assert!(out.contains(fragment), "{text:?}: {out}");
            }
        }
    }

    #[test]
    fn create_text_source_word_lines_preserve_entity_and_tag_provenance() {
        let raw = vec![vec![
            "<span class='foo bar'>".to_string(),
            "X".to_string(),
            "</span>".to_string(),
        ]];
        let mut raw_svg = String::new();
        write_svg_text_source_word_lines(&mut raw_svg, &raw, false, true);
        assert!(
            raw_svg.contains(">&lt;span class=&#39;foo bar&#39;&gt;</tspan>"),
            "{raw_svg}"
        );
        assert_eq!(raw_svg.matches("text-inner-tspan").count(), 3, "{raw_svg}");

        let encoded = vec![vec![
            "&lt;span".to_string(),
            "class='foo".to_string(),
            "bar'&gt;X&lt;/span&gt;".to_string(),
        ]];
        let mut encoded_svg = String::new();
        write_svg_text_source_word_lines(&mut encoded_svg, &encoded, false, true);
        assert!(encoded_svg.contains(">&lt;span</tspan>"), "{encoded_svg}");
        assert!(
            encoded_svg.contains("> class=&#39;foo</tspan>"),
            "{encoded_svg}"
        );
        assert_eq!(
            encoded_svg.matches("text-inner-tspan").count(),
            3,
            "{encoded_svg}"
        );

        let mut angle_svg = String::new();
        write_svg_text_source_word_lines(
            &mut angle_svg,
            &[vec!["&lt;Less&lt;".to_string()]],
            false,
            true,
        );
        assert!(angle_svg.contains(">&lt;Less&lt;</tspan>"), "{angle_svg}");
    }

    #[test]
    fn create_text_source_word_lines_preserve_explicit_empty_rows() {
        let mut svg = String::new();
        write_svg_text_source_word_lines(
            &mut svg,
            &[Vec::new(), Vec::new(), Vec::new()],
            true,
            true,
        );

        assert_eq!(svg.matches(r#"class="row text-outer-tspan""#).count(), 3);
    }

    #[test]
    fn non_markdown_svg_text_tokenizes_tags_anywhere_in_the_line() {
        let mut out = String::new();
        write_svg_text_centered(&mut out, "x <strong>A B</strong> y", false);

        let expected = ["x", " &lt;strong>", " A", " B", " &lt;/strong>", " y"];
        let mut cursor = 0usize;
        for word in expected {
            let next = out[cursor..]
                .find(&format!(">{word}</tspan>"))
                .unwrap_or_else(|| panic!("missing {word:?}: {out}"));
            cursor += next + word.len();
        }
    }
}
