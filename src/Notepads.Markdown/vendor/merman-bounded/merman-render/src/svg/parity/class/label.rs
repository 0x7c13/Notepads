use super::super::*;

pub(super) fn class_math_html_label(
    text: &str,
    mermaid_config: Option<&merman_core::MermaidConfig>,
    math_renderer: Option<&(dyn crate::math::MathRenderer + Send + Sync)>,
) -> Option<String> {
    if !crate::math::contains_delimited_math(text) {
        return None;
    }
    let (Some(config), Some(renderer)) = (mermaid_config, math_renderer) else {
        return None;
    };
    crate::math::render_math_html_label(text, config, Some(renderer))
}

pub(super) struct ClassInlineStyles<'a> {
    pub style_attr: String,
    pub fill: Option<&'a str>,
    pub stroke: Option<&'a str>,
    pub stroke_width: Option<&'a str>,
    pub stroke_dasharray: Option<&'a str>,
}

pub(super) struct ClassHtmlLabelSpec<'a> {
    pub span_class: &'a str,
    pub text: &'a str,
    pub include_p: bool,
    pub extra_span_class: Option<&'a str>,
    pub span_style: Option<&'a str>,
    pub prepared_xhtml: Option<&'a str>,
    pub mermaid_config: Option<&'a merman_core::MermaidConfig>,
    pub math_renderer: Option<&'a (dyn crate::math::MathRenderer + Send + Sync)>,
}

pub(super) fn render_class_html_label(out: &mut String, spec: &ClassHtmlLabelSpec<'_>) {
    out.push_str(r#"<span class=""#);
    escape_xml_into(out, spec.span_class);
    if let Some(extra) = spec
        .extra_span_class
        .map(str::trim)
        .filter(|value| !value.is_empty())
    {
        out.push(' ');
        escape_xml_into(out, extra);
    }
    let span_style = spec.span_style.map(str::trim).unwrap_or("");
    if spec.span_class == "nodeLabel" || !span_style.is_empty() {
        out.push_str(r#"" style=""#);
        super::super::util::escape_attr_into(out, span_style);
        out.push_str(r#"">"#);
    } else {
        out.push_str(r#"">"#);
    }

    let html = spec
        .prepared_xhtml
        .map(std::borrow::Cow::Borrowed)
        .unwrap_or_else(|| {
            std::borrow::Cow::Owned(
                class_math_html_label(spec.text, spec.mermaid_config, spec.math_renderer)
                    .unwrap_or_else(|| {
                        crate::text::mermaid_markdown_to_xhtml_label_fragment(spec.text, true)
                    }),
            )
        });
    if spec.include_p {
        out.push_str(&html);
    } else {
        let inner = html
            .strip_prefix("<p>")
            .and_then(|s| s.strip_suffix("</p>"))
            .unwrap_or(html.as_ref());
        out.push_str(inner);
    }
    out.push_str("</span>");
}

pub(super) fn write_class_svg_plain_node_text(out: &mut String, text: &str) {
    let lines = crate::text::DeterministicTextMeasurer::normalized_text_lines(text)
        .into_iter()
        .map(|line| {
            crate::text::non_markdown_svg_words(&line)
                .map(str::to_owned)
                .collect()
        })
        .collect::<Vec<_>>();
    out.push_str(r#"<g><rect class="background" style="stroke: none"/>"#);
    crate::svg::parity::label::write_svg_text_source_word_lines(out, &lines, true, false);
    out.push_str("</g>");
}

pub(super) fn write_class_svg_text_markdown(out: &mut String, markdown: &str, include_style: bool) {
    crate::svg::parity::label::write_svg_text_markdown(out, markdown, include_style);
}

pub(super) fn write_class_svg_edge_text(out: &mut String, text: &str, include_style: bool) {
    crate::svg::parity::label::write_svg_text_centered(out, text, include_style);
}

pub(super) fn write_class_svg_edge_text_markdown(
    out: &mut String,
    markdown: &str,
    include_style: bool,
) {
    crate::svg::parity::label::write_svg_text_markdown_centered(out, markdown, include_style);
}

pub(super) fn class_html_div_style(width: f64, max_width_px: i64) -> String {
    let max_width_px = max_width_px.max(0);
    if width >= max_width_px as f64 - 0.01 {
        format!(
            "display: table; white-space: break-spaces; line-height: 1.5; max-width: {max_width_px}px; text-align: center; width: {max_width_px}px;"
        )
    } else {
        format!(
            "display: table-cell; white-space: nowrap; line-height: 1.5; max-width: {max_width_px}px; text-align: center;"
        )
    }
}

pub(super) fn class_note_html_div_style(width: f64, max_width_px: i64) -> String {
    let max_width_px = max_width_px.max(0);
    if width >= max_width_px as f64 - 0.01 {
        format!(
            "text-align: center; white-space: break-spaces; display: table; line-height: 1.5; max-width: {max_width_px}px; width: {max_width_px}px;"
        )
    } else {
        format!(
            "text-align: center; white-space: nowrap; display: table-cell; line-height: 1.5; max-width: {max_width_px}px;"
        )
    }
}

pub(super) fn class_html_label_metrics(
    measurer: &dyn TextMeasurer,
    style: &TextStyle,
    text: &str,
    max_width_px: i64,
    css_style: &str,
) -> crate::text::TextMetrics {
    crate::class::class_html_measure_label_metrics(measurer, style, text, max_width_px, css_style)
}

pub(super) fn class_html_title_metrics(
    measurer: &dyn TextMeasurer,
    style: &TextStyle,
    text: &str,
    max_width_px: i64,
) -> crate::text::TextMetrics {
    let markdown = crate::text::DeterministicTextMeasurer::normalized_text_lines(text)
        .into_iter()
        .map(|line| format!("**{line}**"))
        .collect::<Vec<_>>()
        .join("\n");
    crate::text::measure_markdown_with_inline_styles(
        measurer,
        markdown.as_str(),
        style,
        Some(max_width_px.max(1) as f64),
        WrapMode::HtmlLike,
    )
}

pub(super) fn class_svg_label_rect(
    metrics: &crate::text::TextMetrics,
    y_offset: f64,
) -> Option<super::Rect> {
    if !(metrics.width.is_finite() && metrics.height.is_finite()) {
        return None;
    }
    let w = metrics.width.max(0.0);
    let h = metrics.height.max(0.0);
    if w <= 0.0 || h <= 0.0 {
        return None;
    }
    let lines = metrics.line_count.max(1) as f64;
    let y = y_offset - (h / (2.0 * lines));
    Some(super::Rect::from_min_max(0.0, y, w, y + h))
}

pub(super) fn wrap_class_svg_text_like_mermaid(
    text: &str,
    measurer: &dyn TextMeasurer,
    style: &TextStyle,
    wrap_probe_font_size: f64,
    bold: bool,
) -> String {
    let Some(wrap_width_px) =
        mermaid_class_svg_create_text_width_px(measurer, text, style, wrap_probe_font_size)
    else {
        return text.to_string();
    };

    let mut lines: Vec<String> = Vec::new();
    for line in crate::text::DeterministicTextMeasurer::normalized_text_lines(text) {
        let mut tokens = std::collections::VecDeque::from(
            crate::text::DeterministicTextMeasurer::split_line_to_words(&line),
        );
        let mut cur = String::new();

        while let Some(tok) = tokens.pop_front() {
            if cur.is_empty() && tok == " " {
                continue;
            }

            let candidate = format!("{cur}{tok}");
            let candidate_w =
                class_svg_text_computed_length_px(measurer, candidate.trim_end(), style, bold);
            if candidate_w <= wrap_width_px {
                cur = candidate;
                continue;
            }

            if !cur.trim().is_empty() {
                lines.push(cur.trim_end().to_string());
                cur.clear();
                tokens.push_front(tok);
                continue;
            }

            if tok == " " {
                continue;
            }

            let chars = tok.chars().collect::<Vec<_>>();
            let mut cut = 1usize;
            while cut < chars.len() {
                let head: String = chars[..cut].iter().collect();
                let head_w =
                    class_svg_text_computed_length_px(measurer, head.as_str(), style, bold);
                if head_w > wrap_width_px {
                    break;
                }
                cut += 1;
            }
            cut = cut.saturating_sub(1).max(1);
            let head: String = chars[..cut].iter().collect();
            let tail: String = chars[cut..].iter().collect();
            lines.push(head);
            if !tail.is_empty() {
                tokens.push_front(tail);
            }
        }

        if !cur.trim().is_empty() {
            lines.push(cur.trim_end().to_string());
        }
    }

    if lines.len() <= 1 {
        text.to_string()
    } else {
        lines.join("\n")
    }
}

fn mermaid_class_svg_create_text_width_px(
    measurer: &dyn TextMeasurer,
    text: &str,
    style: &TextStyle,
    wrap_probe_font_size: f64,
) -> Option<f64> {
    let wrap_probe_style = TextStyle {
        font_family: style
            .font_family
            .clone()
            .or_else(|| Some("Arial".to_string())),
        font_size: wrap_probe_font_size.max(1.0),
        font_weight: None,
        font_style: None,
    };
    // Reuse the same body-attached calculateTextDimensions operation as Class layout.
    // Combining a generic bbox width and wrapped-text height bypasses host probe semantics.
    let width = crate::class::class_html_create_text_width_px(text, measurer, &wrap_probe_style);
    (width > 0).then_some(width as f64)
}

fn class_svg_text_computed_length_px(
    measurer: &dyn TextMeasurer,
    text: &str,
    style: &TextStyle,
    bold: bool,
) -> f64 {
    if bold {
        let bold_style = TextStyle {
            font_family: style.font_family.clone(),
            font_size: style.font_size,
            font_weight: Some("bolder".to_string()),
            font_style: None,
        };
        measurer.measure_svg_text_computed_length_px(text, &bold_style)
    } else {
        measurer.measure_svg_text_computed_length_px(text, style)
    }
}

pub(super) fn class_apply_inline_styles<'a>(
    node: &'a super::ClassSvgNode,
) -> ClassInlineStyles<'a> {
    let mut style_attr = String::new();
    let mut fill: Option<&str> = None;
    let mut stroke: Option<&str> = None;
    let mut stroke_width: Option<&str> = None;
    let mut stroke_dasharray: Option<&str> = None;

    for raw in &node.styles {
        let trimmed = raw.trim().trim_end_matches(';').trim();
        if trimmed.is_empty() {
            continue;
        }
        if !style_attr.is_empty() {
            style_attr.push(';');
        }
        style_attr.push_str(trimmed);

        let Some((k, v)) = trimmed.split_once(':') else {
            continue;
        };
        let key = k.trim();
        let val = v.trim().trim_end_matches(';').trim();
        if key.eq_ignore_ascii_case("fill") && !val.is_empty() {
            fill = Some(val);
        }
        if key.eq_ignore_ascii_case("stroke") && !val.is_empty() {
            stroke = Some(val);
        }
        if key.eq_ignore_ascii_case("stroke-width") && !val.is_empty() {
            stroke_width = Some(val);
        }
        if key.eq_ignore_ascii_case("stroke-dasharray") && !val.is_empty() {
            stroke_dasharray = Some(val);
        }
    }

    ClassInlineStyles {
        style_attr,
        fill,
        stroke,
        stroke_width,
        stroke_dasharray,
    }
}

#[cfg(test)]
mod tests {
    use super::{ClassHtmlLabelSpec, render_class_html_label};

    #[test]
    fn class_svg_wrap_width_uses_the_same_host_dimensions_probe_as_layout() {
        struct HostProbe;

        impl crate::text::TextMeasurer for HostProbe {
            fn measure(
                &self,
                _text: &str,
                _style: &crate::text::TextStyle,
            ) -> crate::text::TextMetrics {
                panic!("Class width must use the dedicated calculateTextDimensions operation");
            }

            fn measure_mermaid_calculate_text_dimensions(
                &self,
                text: &str,
                style: &crate::text::TextStyle,
            ) -> crate::text::TextMetrics {
                assert_eq!(text, "member");
                assert_eq!(style.font_size, 10.0);
                assert_eq!(style.font_weight, None);
                assert_eq!(style.font_style, None);
                let (width, height) = match style.font_family.as_deref() {
                    Some("sans-serif") => (120.4, 20.0),
                    Some("HostFont") => (80.4, 30.0),
                    family => panic!("unexpected probe font: {family:?}"),
                };
                crate::text::TextMetrics {
                    width,
                    height,
                    line_count: 1,
                }
            }
        }

        let render_style = crate::text::TextStyle {
            font_family: Some("HostFont".to_string()),
            font_size: 24.0,
            font_weight: Some("bold".to_string()),
            font_style: Some("italic".to_string()),
        };
        let width = super::mermaid_class_svg_create_text_width_px(
            &HostProbe,
            "member",
            &render_style,
            10.0,
        );
        // Mermaid keeps the configured font when sans-serif is wider but not taller.
        // Its rounded width (80) then receives shapeUtil's 50px wrapping allowance.
        assert_eq!(width, Some(130.0));
    }

    #[test]
    fn class_html_label_serializes_raw_html_and_escaped_generics_structurally() {
        let mut rich = String::new();
        render_class_html_label(
            &mut rich,
            &ClassHtmlLabelSpec {
                span_class: "nodeLabel",
                text: "<a href='https://example.com'><code>Entity</code></a>",
                include_p: true,
                extra_span_class: Some("markdown-node-label"),
                span_style: None,
                prepared_xhtml: None,
                mermaid_config: None,
                math_renderer: None,
            },
        );
        assert!(rich.contains(r#"<a href='https://example.com'><code>Entity</code></a>"#));
        assert!(!rich.contains("&lt;code&gt;"));

        let mut generic = String::new();
        render_class_html_label(
            &mut generic,
            &ClassHtmlLabelSpec {
                span_class: "nodeLabel",
                text: "Generic&lt;T&gt; driver_license",
                include_p: true,
                extra_span_class: Some("markdown-node-label"),
                span_style: None,
                prepared_xhtml: None,
                mermaid_config: None,
                math_renderer: None,
            },
        );
        assert!(generic.contains("<p>Generic&lt;T&gt; driver_license</p>"));
        assert!(!generic.contains("&amp;lt;"));
    }
}
