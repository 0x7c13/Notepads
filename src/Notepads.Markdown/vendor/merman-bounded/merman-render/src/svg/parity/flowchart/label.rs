//! Flowchart label rendering helpers (HTML/SVG text).

use super::*;

pub(in crate::svg::parity) fn flowchart_label_html(
    label: &str,
    label_type: &str,
    config: &merman_core::MermaidConfig,
    math_renderer: Option<&(dyn crate::math::MathRenderer + Send + Sync)>,
) -> String {
    flowchart_label_html_impl(label, label_type, config, math_renderer)
}

fn flowchart_label_html_impl(
    label: &str,
    label_type: &str,
    config: &merman_core::MermaidConfig,
    math_renderer: Option<&(dyn crate::math::MathRenderer + Send + Sync)>,
) -> String {
    if crate::flowchart::flowchart_label_text_is_empty_for_mode(label, true) {
        return String::new();
    }

    fn normalize_flowchart_img_tags(input: &str, fixed_width: bool) -> String {
        // Mermaid flowchart-v2 adds inline styles to `<img>` tags inside HTML labels to constrain
        // their layout. The SVG baseline uses XHTML, so we also self-close the tags later.
        if !input.to_ascii_lowercase().contains("<img") {
            return input.to_string();
        }

        let style = if fixed_width {
            "display: flex; flex-direction: column; min-width: 80px; max-width: 80px;"
        } else {
            "display: flex; flex-direction: column; width: 100%;"
        };

        fn extract_img_src(tag: &str) -> Option<String> {
            let lower = tag.to_ascii_lowercase();
            let idx = lower.find("src=")?;
            let rest = &tag[idx + 4..];
            let rest = rest.trim_start();
            let quote = rest.chars().next()?;
            if quote != '"' && quote != '\'' {
                return None;
            }
            let mut val = String::new();
            let mut it = rest.chars();
            let _ = it.next(); // consume quote
            for ch in it {
                if ch == quote {
                    break;
                }
                val.push(ch);
            }
            let val = val.trim().to_string();
            if val.is_empty() { None } else { Some(val) }
        }

        let mut out = String::with_capacity(input.len());
        let bytes = input.as_bytes();
        let mut i = 0usize;
        while i < bytes.len() {
            if bytes[i] == b'<' && i + 3 < bytes.len() {
                let rest = &input[i..];
                if bytes[i..i + 4].eq_ignore_ascii_case(b"<img") {
                    let Some(rel_end) = rest.find('>') else {
                        out.push_str(rest);
                        break;
                    };
                    let tag = &rest[..=rel_end];
                    let src = extract_img_src(tag);
                    out.push_str("<img");
                    if let Some(src) = src {
                        let _ = write!(out, r#" src="{}""#, escape_attr(&src));
                    }
                    out.push_str(r#" style=""#);
                    out.push_str(style);
                    out.push('"');
                    out.push('>');
                    i += rel_end + 1;
                    continue;
                }
            }
            let Some(ch) = input[i..].chars().next() else {
                break;
            };
            out.push(ch);
            i += ch.len_utf8();
        }
        out
    }

    fn is_single_img_label(label: &str) -> bool {
        let t = crate::flowchart::flowchart_trim_html_collapsible_whitespace(label);
        let lower = t.to_ascii_lowercase();
        if !lower.starts_with("<img") {
            return false;
        }
        let Some(end) = t.find('>') else {
            return false;
        };
        crate::flowchart::flowchart_trim_html_collapsible_whitespace(&t[end + 1..]).is_empty()
    }

    fn trim_markdown_trailing_newlines(
        input: std::borrow::Cow<'_, str>,
    ) -> std::borrow::Cow<'_, str> {
        if input.ends_with('\n') {
            std::borrow::Cow::Owned(input.trim_end_matches('\n').to_string())
        } else {
            input
        }
    }

    if let Some(html) = crate::math::render_math_html_label(label, config, math_renderer) {
        return html;
    }

    match label_type {
        "markdown" => {
            let decoded = decode_mermaid_entities_for_render_text(label);
            let decoded = if decoded.contains("\\\\") {
                std::borrow::Cow::Owned(decoded.replace("\\\\", "\\"))
            } else {
                decoded
            };
            let decoded = trim_markdown_trailing_newlines(decoded);
            let markdown_auto_wrap = config
                .as_value()
                .get("markdownAutoWrap")
                .and_then(serde_json::Value::as_bool)
                .unwrap_or(true);
            let html_out = crate::text::mermaid_markdown_to_html_label_fragment(
                decoded.as_ref(),
                markdown_auto_wrap,
            );
            let html_out =
                crate::flowchart::flowchart_trim_html_collapsible_whitespace(&html_out).to_string();
            let html_out = crate::text::replace_fontawesome_icons(&html_out);
            crate::xml::normalize_html_fragment_for_xhtml(&merman_core::sanitize::sanitize_text(
                &html_out, config,
            ))
        }
        _ => {
            let can_skip_sanitizer = !label.contains('<')
                && !label.contains('>')
                && !label.contains('&')
                && !label.contains(":fa-");
            let label = crate::flowchart::flowchart_non_markdown_label_for_html(label, label_type);

            // Fast path for the overwhelmingly common case: plain text labels (no HTML, no
            // entities, no Mermaid icon syntax). In upstream Mermaid, these go through
            // `sanitizeText(...)` but the output is unchanged; skipping the HTML sanitizer here is
            // a large win in flowcharts with many nodes.
            if can_skip_sanitizer {
                return format!("<p>{label}</p>");
            }

            // Mermaid's nonMarkdownToHTML() wraps every non-empty label in one paragraph.
            // Markdown block classification must not leak into this branch.
            let fixed_img_width = is_single_img_label(&label);
            let label = normalize_flowchart_img_tags(&label, fixed_img_width);
            let wrapped = format!("<p>{}</p>", label);
            let wrapped = if wrapped.contains(":fa-") {
                crate::text::replace_fontawesome_icons(&wrapped)
            } else {
                wrapped
            };
            crate::xml::normalize_html_fragment_for_xhtml(&merman_core::sanitize::sanitize_text(
                &wrapped, config,
            ))
        }
    }
}

pub(in crate::svg::parity) fn flowchart_label_plain_text(
    label: &str,
    label_type: &str,
    html_labels: bool,
) -> String {
    crate::flowchart::flowchart_label_plain_text_for_layout(label, label_type, html_labels)
}

pub(in crate::svg::parity) fn write_flowchart_empty_svg_text_centered(
    out: &mut String,
    include_style: bool,
) {
    crate::svg::parity::label::write_svg_text_source_word_lines(
        out,
        &[Vec::new()],
        include_style,
        true,
    );
}

pub(in crate::svg::parity) fn write_flowchart_svg_source_word_lines(
    out: &mut String,
    lines: &[Vec<String>],
    include_style: bool,
) {
    crate::svg::parity::label::write_svg_text_source_word_lines(out, lines, include_style, false);
}

pub(in crate::svg::parity) fn write_flowchart_svg_source_word_lines_centered_with_style(
    out: &mut String,
    lines: &[Vec<String>],
    style: &str,
) {
    crate::svg::parity::label::write_svg_text_source_word_lines_with_style(out, lines, style, true);
}

#[cfg(test)]
pub(in crate::svg::parity) fn wrap_flowchart_svg_source_word_lines(
    measurer: &dyn crate::text::TextMeasurer,
    lines: &[Vec<String>],
    style: &crate::text::TextStyle,
    max_width_px: Option<f64>,
    break_long_words: bool,
) -> Vec<Vec<String>> {
    crate::flowchart::flowchart_wrap_svg_source_word_lines(
        measurer,
        lines,
        style,
        max_width_px,
        break_long_words,
    )
}

pub(in crate::svg::parity) fn write_flowchart_svg_text_markdown(
    out: &mut String,
    markdown: &str,
    include_style: bool,
) {
    crate::svg::parity::label::write_svg_text_markdown_from_create_text_source(
        out,
        markdown,
        include_style,
    );
}

pub(in crate::svg::parity) fn write_flowchart_svg_text_markdown_wrapped_centered(
    out: &mut String,
    markdown: &str,
    include_style: bool,
    measurer: &dyn crate::text::TextMeasurer,
    style: &crate::text::TextStyle,
    max_width_px: Option<f64>,
) {
    crate::svg::parity::label::write_svg_text_markdown_wrapped_centered_from_create_text_source(
        out,
        markdown,
        include_style,
        measurer,
        style,
        max_width_px,
    );
}

pub(in crate::svg::parity) fn write_flowchart_svg_text_markdown_wrapped(
    out: &mut String,
    markdown: &str,
    include_style: bool,
    measurer: &dyn crate::text::TextMeasurer,
    style: &crate::text::TextStyle,
    max_width_px: Option<f64>,
) {
    crate::svg::parity::label::write_svg_text_markdown_wrapped_from_create_text_source(
        out,
        markdown,
        include_style,
        measurer,
        style,
        max_width_px,
    );
}

#[cfg(test)]
mod tests {
    use super::{
        flowchart_label_html_impl, wrap_flowchart_svg_source_word_lines,
        write_flowchart_svg_source_word_lines,
    };

    #[test]
    fn markdown_html_output_follows_upstream_raw_tokens_and_escapes() {
        for security_level in ["strict", "loose", "antiscript", "sandbox"] {
            let config = merman_core::MermaidConfig::from_value(serde_json::json!({
                "htmlLabels": true,
                "securityLevel": security_level,
            }));
            for (input, text) in [
                ("`<span style='font-size:64px'>AVAVAV</span>`", "`AVAVAV`"),
                (
                    "``<span style='font-size:64px'>`**AVAVAV**`</span>``",
                    "```**AVAVAV**```",
                ),
                (r"\<span style='font-size:64px'>AVAVAV\</span>", "AVAVAV"),
                (r"\\<span style='font-size:64px'>AVAVAV\\</span>", "AVAVAV"),
            ] {
                let output = flowchart_label_html_impl(input, "markdown", &config, None);
                let document = roxmltree::Document::parse(&output).unwrap();
                let span = document
                    .descendants()
                    .find(|node| node.has_tag_name("span"))
                    .unwrap_or_else(|| panic!("expected effective HTML: {input}: {output}"));
                assert_eq!(span.attribute("style"), Some("font-size:64px"));
                assert_eq!(
                    crate::text::mermaid_xhtml_label_text_content(&output).as_deref(),
                    Some(text)
                );
            }
        }
    }

    #[test]
    fn markdown_html_output_decodes_mermaid_entities_before_html_projection() {
        let config = merman_core::MermaidConfig::from_value(serde_json::json!({
            "htmlLabels": true,
            "securityLevel": "loose"
        }));
        for input in [
            "`#60;span style=#quot;font-size:64px#quot;#62;AVAVAV#60;/span#62;`\n\n",
            "`ﬂ°°60¶ßspan style=ﬂ°quot¶ßfont-size:64pxﬂ°quot¶ßﬂ°°62¶ßAVAVAVﬂ°°60¶ß/spanﬂ°°62¶ß`\n\n",
        ] {
            let output = flowchart_label_html_impl(input, "markdown", &config, None);
            assert_eq!(
                output,
                "<p>`<span style=\"font-size:64px\">AVAVAV</span>`</p>"
            );
        }
    }

    #[test]
    fn markdown_html_output_preserves_authored_raw_tags() {
        let config = merman_core::MermaidConfig::from_value(serde_json::json!({
            "htmlLabels": true,
            "securityLevel": "loose"
        }));
        let input = "Before <span style='font-size:64px'>AVAVAV</span> after\n\n";
        let output = flowchart_label_html_impl(input, "markdown", &config, None);
        let document = roxmltree::Document::parse(&output).unwrap();
        let span = document
            .descendants()
            .find(|node| node.has_tag_name("span"))
            .unwrap();
        assert_eq!(span.attribute("style"), Some("font-size:64px"));
        assert_eq!(span.text(), Some("AVAVAV"));
        assert_eq!(
            crate::text::mermaid_xhtml_label_text_content(&output).as_deref(),
            Some("Before AVAVAV after")
        );
    }

    #[test]
    fn html_image_prefix_matching_preserves_unicode_and_malformed_tags() {
        for security_level in ["strict", "loose", "antiscript", "sandbox"] {
            let config = merman_core::MermaidConfig::from_value(serde_json::json!({
                "htmlLabels": true,
                "securityLevel": security_level
            }));
            for surrounding in ["", "<span>中文😀</span>", "&lt;img&gt;", "<é>α</é>"] {
                for suffix in [" src='x'>", "inary src='x'>", ">", " src=x>"] {
                    let lower = format!("{surrounding}<img{suffix}");
                    let expected = flowchart_label_html_impl(&lower, "string", &config, None);
                    for prefix in ["<IMG", "<Img", "<iMg", "<imG"] {
                        let input = format!("{surrounding}{prefix}{suffix}");
                        assert_eq!(
                            flowchart_label_html_impl(&input, "string", &config, None),
                            expected,
                            "security={security_level}, input={input:?}"
                        );
                    }
                }
            }
        }
    }

    #[test]
    fn html_image_after_many_tags_preserves_text_and_image_style() {
        let config = merman_core::MermaidConfig::from_value(serde_json::json!({
            "htmlLabels": true,
            "securityLevel": "strict"
        }));
        let spans = "<span>中文😀</span>".repeat(1_600);
        let input = format!("{spans}<ImG src='https://example.com/x.svg'>");
        let output = flowchart_label_html_impl(&input, "string", &config, None);
        assert_eq!(output.matches("中文😀").count(), 1_600);
        assert_eq!(output.matches("<img ").count(), 1);
        assert!(output.contains("https://example.com/x.svg"), "{output}");
        assert!(output.contains("width: 100%;"), "{output}");
        assert!(output.ends_with(" /></p>"), "{output}");
    }

    #[test]
    fn html_unclosed_image_preserves_original_prefix_case() {
        let config = merman_core::MermaidConfig::from_value(serde_json::json!({
            "htmlLabels": true,
            "securityLevel": "strict"
        }));
        assert_eq!(
            flowchart_label_html_impl("<IMG src='x'", "string", &config, None),
            "<p><IMG src='x' />"
        );
    }

    #[test]
    fn html_label_output_preserves_entity_and_direct_nbsp_boundaries() {
        let config = merman_core::MermaidConfig::from_value(serde_json::json!({
            "htmlLabels": true,
            "securityLevel": "loose"
        }));
        let cases = [
            ("&nbsp;A", "<p>\u{00A0}A</p>"),
            ("A&nbsp;", "<p>A\u{00A0}</p>"),
            ("&nbsp;", "<p>\u{00A0}</p>"),
            ("\u{00A0}A", "<p>\u{00A0}A</p>"),
            ("A\u{00A0}", "<p>A\u{00A0}</p>"),
            ("\u{00A0}", "<p>\u{00A0}</p>"),
            ("A<br>&nbsp;", "<p>A<br />\u{00A0}</p>"),
        ];

        for label_type in ["string", "markdown"] {
            for (input, expected) in cases {
                assert_eq!(
                    flowchart_label_html_impl(input, label_type, &config, None),
                    expected,
                    "label_type={label_type}, input={input:?}",
                );
            }
        }
    }

    #[test]
    fn svg_source_word_wrapping_keeps_attribute_spaces_inside_the_tag_word() {
        let source = crate::flowchart::flowchart_non_markdown_svg_source_word_lines(
            "<span class='foo bar'>X</span>",
        );
        let wrapped = wrap_flowchart_svg_source_word_lines(
            &crate::text::DeterministicTextMeasurer::default(),
            &source,
            &crate::text::TextStyle::default(),
            Some(1_000.0),
            true,
        );
        assert_eq!(wrapped, source);

        let mut svg = String::new();
        write_flowchart_svg_source_word_lines(&mut svg, &wrapped, false);
        assert_eq!(svg.matches("text-inner-tspan").count(), 3, "{svg}");
        assert!(
            svg.contains(">&lt;span class=&#39;foo bar&#39;&gt;</tspan>"),
            "{svg}"
        );
    }
}
