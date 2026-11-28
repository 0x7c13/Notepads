use super::*;
use crate::graph_label::flowchart_label_metrics_for_layout;
use crate::text::metrics::style_requests_bold_font_weight;

fn assert_finite_positive_metrics(metrics: TextMetrics) {
    assert!(
        metrics.width.is_finite() && metrics.width > 0.0,
        "{metrics:?}"
    );
    assert!(
        metrics.height.is_finite() && metrics.height > 0.0,
        "{metrics:?}"
    );
    assert!(metrics.line_count > 0, "{metrics:?}");
}

fn assert_same_metrics(actual: TextMetrics, expected: TextMetrics) {
    assert_eq!(actual.width, expected.width);
    assert_eq!(actual.height, expected.height);
    assert_eq!(actual.line_count, expected.line_count);
}

fn assert_same_metrics_after_dom_rounding(actual: TextMetrics, expected: TextMetrics) {
    assert_eq!(actual.width, round_to_1_64_px(expected.width));
    assert_eq!(actual.height, expected.height);
    assert_eq!(actual.line_count, expected.line_count);
}

#[test]
fn fractional_font_weights_use_the_browser_bold_threshold() {
    for (weight, expected) in [("599.5", false), ("700.0", true), ("700.5", true)] {
        let style = TextStyle {
            font_weight: Some(weight.to_string()),
            ..TextStyle::default()
        };
        assert_eq!(
            style_requests_bold_font_weight(&style),
            expected,
            "{weight}"
        );
    }
}

#[test]
fn html_br_trims_trailing_space_before_break_for_flowchart_labels() {
    let plain =
        crate::graph_label::flowchart_label_plain_text_for_layout("Hexagon <br> end", "text", true);
    assert_eq!(plain, "Hexagon\nend");

    let measurer = DeterministicTextMeasurer::default();
    let style = TextStyle {
        font_family: Some("\"trebuchet ms\", verdana, arial, sans-serif".to_string()),
        font_size: 16.0,
        font_weight: None,
        font_style: None,
    };

    let m = measurer.measure_wrapped(&plain, &style, Some(200.0), WrapMode::HtmlLike);
    let first_line = measurer.measure_wrapped("Hexagon", &style, None, WrapMode::HtmlLike);
    let second_line = measurer.measure_wrapped("end", &style, None, WrapMode::HtmlLike);
    assert_eq!(m.line_count, 2);
    assert_eq!(m.width, first_line.width.max(second_line.width));
    assert!(m.height > first_line.height.max(second_line.height));
}

#[test]
fn flowchart_html_text_extraction_preserves_bare_comparison_symbols() {
    let plain = crate::graph_label::flowchart_label_plain_text_for_layout(
        "标题 Unicode — 測試 &amp; &lt; &gt; and x < y > z",
        "text",
        true,
    );
    assert_eq!(plain, "标题 Unicode — 測試 & < > and x < y > z");
}

#[test]
fn flowchart_html_text_extraction_decodes_html5_entities_once_after_tag_removal() {
    let plain = crate::graph_label::flowchart_label_plain_text_for_layout(
        "&copy; &infin; &NotEqualTilde; &lt;b&gt; &amp;lt;",
        "text",
        true,
    );

    assert_eq!(plain, "© ∞ ≂̸ <b> &lt;");

    let split_entity = crate::graph_label::flowchart_label_plain_text_for_layout(
        "&cop<strong>y;</strong>",
        "text",
        true,
    );
    assert_eq!(split_entity, "&copy;");

    for input in ["X&#10;Y", "X&NewLine;Y"] {
        assert_eq!(
            crate::graph_label::flowchart_label_plain_text_for_layout(input, "text", true),
            "X Y",
            "{input:?}",
        );
    }
}

#[test]
fn html_inline_measurement_uses_full_named_entity_decoding() {
    let measurer = DeterministicTextMeasurer::default();
    let style = TextStyle {
        font_family: Some("\"trebuchet ms\", verdana, arial, sans-serif".to_string()),
        font_size: 16.0,
        font_weight: None,
        font_style: None,
    };

    let entities = measure_html_with_inline_styles(
        &measurer,
        "<span>&copy;&infin;&NotEqualTilde;</span>",
        &style,
        None,
        WrapMode::HtmlLike,
    );
    let unicode = measure_html_with_inline_styles(
        &measurer,
        "<span>©∞≂̸</span>",
        &style,
        None,
        WrapMode::HtmlLike,
    );

    assert_same_metrics(entities, unicode);

    for input in ["&copy test", "&#169 test", "&#xA9 test"] {
        let decoded =
            measure_html_with_inline_styles(&measurer, input, &style, None, WrapMode::HtmlLike);
        let unicode =
            measure_html_with_inline_styles(&measurer, "© test", &style, None, WrapMode::HtmlLike);
        assert_same_metrics(decoded, unicode);
    }

    for input in ["X&#10;Y", "X&NewLine;Y"] {
        let decoded =
            measure_html_with_inline_styles(&measurer, input, &style, None, WrapMode::HtmlLike);
        let collapsed =
            measure_html_with_inline_styles(&measurer, "X Y", &style, None, WrapMode::HtmlLike);
        assert_same_metrics(decoded, collapsed);
        assert_eq!(decoded.line_count, 1, "{input:?}: {decoded:?}");
    }
}

#[test]
fn html_break_spaces_uses_decoded_entity_whitespace() {
    let measurer = DeterministicTextMeasurer::default();
    let style = TextStyle {
        font_family: Some("\"trebuchet ms\", verdana, arial, sans-serif".to_string()),
        font_size: 16.0,
        font_weight: None,
        font_style: None,
    };
    let measure = |html: &str| {
        measure_html_with_inline_styles(&measurer, html, &style, Some(39.0), WrapMode::HtmlLike)
    };

    let spaces = measure("A  AA AAA");
    assert_eq!(spaces.line_count, 3, "{spaces:?}");
    assert_same_metrics(measure("A&#32;&#32;AA AAA"), spaces);
    assert_same_metrics(measure("A&#x20;&#x20;AA AAA"), spaces);
    assert_same_metrics(
        measure("A&#32;<strong>&#32;AA</strong> AAA"),
        measure("A <strong> AA</strong> AAA"),
    );

    for (physical, entities) in [
        ("A\tAA AAA", "A&Tab;AA AAA"),
        ("A\tAA AAA", "A&#9;AA AAA"),
        ("A\tAA AAA", "A&#x9;AA AAA"),
        ("A\nAA AAA", "A&NewLine;AA AAA"),
        ("A\nAA AAA", "A&#10;AA AAA"),
        ("A\nAA AAA", "A&#xA;AA AAA"),
    ] {
        assert_same_metrics(measure(entities), measure(physical));
    }
}

#[test]
fn ecmascript_and_html_whitespace_helpers_preserve_next_line_control() {
    let nel = '\u{0085}';
    assert!(!is_ecmascript_whitespace(nel));
    assert!(!is_html_collapsible_ascii_whitespace(nel));
    assert_eq!(
        trim_ecmascript_whitespace("\u{0085}A\u{0085}"),
        "\u{0085}A\u{0085}"
    );
    assert_eq!(
        trim_html_collapsible_ascii_whitespace(" \u{0085} "),
        "\u{0085}"
    );

    let html =
        crate::graph_label::flowchart_label_plain_text_for_layout(" \u{0085} ", "text", true);
    let svg =
        crate::graph_label::flowchart_label_plain_text_for_layout(" \u{0085} ", "text", false);
    assert_eq!(html, "\u{0085}");
    assert_eq!(svg, "\u{0085}");
    assert!(!crate::graph_label::flowchart_label_text_is_empty_for_mode(
        &html, true,
    ));
    assert!(!crate::graph_label::flowchart_label_text_is_empty_for_mode(
        &svg, false,
    ));
}

#[test]
fn flowchart_html_text_extraction_preserves_nbsp_boundaries() {
    let cases = [
        ("&nbsp;A", "\u{00A0}A"),
        ("A&nbsp;", "A\u{00A0}"),
        ("&nbsp;", "\u{00A0}"),
        ("\u{00A0}A", "\u{00A0}A"),
        ("A\u{00A0}", "A\u{00A0}"),
        ("\u{00A0}", "\u{00A0}"),
        ("A<br>&nbsp;", "A\n\u{00A0}"),
    ];

    for label_type in ["text", "string", "markdown"] {
        for (input, expected) in cases {
            assert_eq!(
                crate::graph_label::flowchart_label_plain_text_for_layout(input, label_type, true,),
                expected,
                "label_type={label_type}, input={input:?}",
            );
        }
    }
}

#[test]
fn flowchart_html_text_extraction_preserves_nbsp_and_collapses_ascii_space_runs() {
    assert_eq!(
        crate::graph_label::flowchart_label_plain_text_for_layout(
            "A&nbsp;&nbsp;B  C   D",
            "string",
            true,
        ),
        "A\u{00A0}\u{00A0}B C D",
    );
}

#[test]
fn deterministic_html_wrapping_preserves_nbsp_width() {
    let measurer = DeterministicTextMeasurer::default();
    let style = TextStyle {
        font_family: None,
        font_size: 16.0,
        font_weight: None,
        font_style: None,
    };

    let plain_a = measurer.measure_wrapped("A", &style, Some(200.0), WrapMode::HtmlLike);
    let trailing_ascii_space =
        measurer.measure_wrapped("A ", &style, Some(200.0), WrapMode::HtmlLike);
    let trailing_nbsp =
        measurer.measure_wrapped("A\u{00A0}", &style, Some(200.0), WrapMode::HtmlLike);
    let pure_nbsp = measurer.measure_wrapped("\u{00A0}", &style, Some(200.0), WrapMode::HtmlLike);
    let svg_nbsp = measurer.measure_wrapped("\u{00A0}", &style, Some(200.0), WrapMode::SvgLike);

    assert_same_metrics(trailing_ascii_space, plain_a);
    assert!(trailing_nbsp.width > plain_a.width, "{trailing_nbsp:?}");
    assert_finite_positive_metrics(pure_nbsp);
    assert_finite_positive_metrics(svg_nbsp);
}

#[test]
fn flowchart_svg_text_extraction_matches_create_text_entity_and_whitespace_semantics() {
    assert_eq!(
        crate::graph_label::flowchart_label_plain_text_for_layout(
            "\u{00A0}A\u{00A0}",
            "text",
            false,
        ),
        "A",
    );
    assert_eq!(
        crate::graph_label::flowchart_label_plain_text_for_layout(
            "A\u{00A0}\u{FEFF}B",
            "text",
            false,
        ),
        "A B",
    );
    assert_eq!(
        crate::graph_label::flowchart_label_plain_text_for_layout(
            "\u{0085}A\u{0085}",
            "text",
            false,
        ),
        "\u{0085}A\u{0085}",
    );
    assert_eq!(
        crate::graph_label::flowchart_label_plain_text_for_layout(
            "&amp;A&lt;B&gt;&nbsp;&#160;",
            "text",
            false,
        ),
        "&A<B>&nbsp;&#160;",
    );
    assert_eq!(
        crate::graph_label::flowchart_label_plain_text_for_layout("\u{00A0}", "markdown", false,),
        "\u{00A0}",
    );
    assert_eq!(
        crate::graph_label::flowchart_label_plain_text_for_layout("A\\nB", "text", false),
        "A\nB",
    );
    assert_eq!(
        crate::graph_label::flowchart_label_plain_text_for_layout("A<BR\u{00A0}/>B", "text", false,),
        "A\nB",
    );
    assert!(crate::graph_label::flowchart_label_is_empty_for_render(""));
    assert!(!crate::graph_label::flowchart_label_is_empty_for_render(
        "<img src='x'>"
    ));
}

#[test]
fn flowchart_html_unicode_entities_use_finite_fallback_metrics() {
    let measurer = DeterministicTextMeasurer::default();
    let style = TextStyle {
        font_family: Some("\"trebuchet ms\", verdana, arial, sans-serif".to_string()),
        font_size: 16.0,
        font_weight: None,
        font_style: None,
    };
    let cfg = merman_core::MermaidConfig::default();

    let metrics = crate::graph_label::flowchart_label_metrics_for_layout(
        crate::graph_label::FlowchartLabelMetricsRequest {
            measurer: &measurer,
            raw_label: "标题 Unicode — 測試 & < >",
            label_type: "text",
            style: &style,
            max_width_px: Some(200.0),
            wrap_mode: WrapMode::HtmlLike,
            config: &cfg,
            math_renderer: None,
        },
    );
    assert_finite_positive_metrics(metrics);
    assert_eq!(metrics.line_count, 1);

    let plain_cjk = measurer.measure_wrapped("负责人审批", &style, Some(200.0), WrapMode::HtmlLike);
    let single_cjk = measurer.measure_wrapped("负", &style, Some(200.0), WrapMode::HtmlLike);
    assert_finite_positive_metrics(plain_cjk);
    assert!(plain_cjk.width > single_cjk.width);
}

#[test]
fn flowchart_html_unicode_blocks_produce_finite_metrics() {
    let measurer = DeterministicTextMeasurer::default();
    let style = TextStyle {
        font_family: Some("\"trebuchet ms\", verdana, arial, sans-serif".to_string()),
        font_size: 16.0,
        font_weight: None,
        font_style: None,
    };

    for text in [
        "emoji: 😀😅👍",
        "rtl: שלום-עולם",
        "中文 / 日本語 / 한글",
        "Path: C:\\Temp\\synthetic\\out.svg (Windows-style)",
    ] {
        assert_finite_positive_metrics(measurer.measure_wrapped(
            text,
            &style,
            Some(200.0),
            WrapMode::HtmlLike,
        ));
    }
}

#[test]
fn typst_relevant_font_intent_keeps_measurement_finite_without_host_font_assets() {
    let payloads = [
        "unknown font family",
        "CJK: 负责人审批",
        "emoji: 😀😅👍",
        "mixed: Source Sans 3 / 測試 / 🚀",
    ];
    let styles = [
        TextStyle {
            font_family: Some("TypstOnlyFont, Arial, sans-serif".to_string()),
            font_size: 13.0,
            font_weight: None,
            font_style: None,
        },
        TextStyle {
            font_family: Some("\"trebuchet ms\", verdana, arial, sans-serif".to_string()),
            font_size: 16.0,
            font_weight: None,
            font_style: None,
        },
    ];
    let measurer = DeterministicTextMeasurer::default();

    for style in &styles {
        for payload in payloads {
            let metrics = measurer.measure_wrapped(payload, style, Some(200.0), WrapMode::HtmlLike);
            assert!(
                metrics.width.is_finite() && metrics.width >= 0.0,
                "{metrics:?}"
            );
            assert!(
                metrics.height.is_finite() && metrics.height >= 0.0,
                "{metrics:?}"
            );
        }
    }
}

#[test]
fn html_inline_styles_delegate_to_the_matching_font_variant() {
    let measurer = DeterministicTextMeasurer::default();
    let regular = TextStyle {
        font_family: Some("\"trebuchet ms\", verdana, arial, sans-serif".to_string()),
        font_size: 16.0,
        font_weight: None,
        font_style: None,
    };
    let bold_italic = TextStyle {
        font_weight: Some("700".to_string()),
        font_style: Some("italic".to_string()),
        ..regular.clone()
    };

    let actual = measure_html_with_inline_styles(
        &measurer,
        "<strong><em>Moving</em></strong>",
        &regular,
        None,
        WrapMode::HtmlLike,
    );
    let expected = measurer.measure_wrapped("Moving", &bold_italic, None, WrapMode::HtmlLike);

    assert_same_metrics_after_dom_rounding(actual, expected);

    let bold = TextStyle {
        font_weight: Some("700".to_string()),
        ..regular.clone()
    };
    let italic = TextStyle {
        font_style: Some("italic".to_string()),
        ..regular.clone()
    };
    let mixed = measure_html_with_inline_styles(
        &measurer,
        "plain<strong>Bold</strong><em>Italic</em>",
        &regular,
        None,
        WrapMode::HtmlLike,
    );
    let mixed_expected = measurer
        .measure_wrapped("plain", &regular, None, WrapMode::HtmlLike)
        .width
        + measurer
            .measure_wrapped("Bold", &bold, None, WrapMode::HtmlLike)
            .width
        + measurer
            .measure_wrapped("Italic", &italic, None, WrapMode::HtmlLike)
            .width;
    assert_eq!(mixed.width, round_to_1_64_px(mixed_expected));
}

#[test]
fn html_inline_metrics_preserve_entity_and_direct_nbsp_boundaries() {
    let measurer = DeterministicTextMeasurer::default();
    let style = TextStyle {
        font_family: Some("\"trebuchet ms\", verdana, arial, sans-serif".to_string()),
        font_size: 16.0,
        font_weight: None,
        font_style: None,
    };

    for (entity, direct) in [
        ("&nbsp;A", "\u{00A0}A"),
        ("A&nbsp;", "A\u{00A0}"),
        ("&nbsp;", "\u{00A0}"),
    ] {
        let entity_metrics =
            measure_html_with_inline_styles(&measurer, entity, &style, None, WrapMode::HtmlLike);
        let direct_metrics =
            measure_html_with_inline_styles(&measurer, direct, &style, None, WrapMode::HtmlLike);
        assert_same_metrics(entity_metrics, direct_metrics);
        assert_finite_positive_metrics(entity_metrics);

        let entity_markdown = measure_markdown_with_inline_styles(
            &measurer,
            entity,
            &style,
            None,
            WrapMode::HtmlLike,
        );
        let direct_markdown = measure_markdown_with_inline_styles(
            &measurer,
            direct,
            &style,
            None,
            WrapMode::HtmlLike,
        );
        assert_same_metrics(entity_markdown, direct_markdown);
        assert_finite_positive_metrics(entity_markdown);
    }

    let plain_a = measurer.measure_wrapped("A", &style, None, WrapMode::HtmlLike);
    let trailing_nbsp =
        measure_html_with_inline_styles(&measurer, "A&nbsp;", &style, None, WrapMode::HtmlLike);
    assert!(trailing_nbsp.width > plain_a.width);

    let styled_nbsp_tail = measure_html_with_inline_styles(
        &measurer,
        "<p>A<br /><strong>&nbsp;</strong></p>",
        &style,
        None,
        WrapMode::HtmlLike,
    );
    assert_eq!(styled_nbsp_tail.line_count, 2, "{styled_nbsp_tail:?}");
    assert!(styled_nbsp_tail.height > trailing_nbsp.height);

    let plain_nbsp_tail = measurer.measure_wrapped("A\n\u{00A0}", &style, None, WrapMode::HtmlLike);
    assert_eq!(plain_nbsp_tail.line_count, 2, "{plain_nbsp_tail:?}");
    assert!(
        plain_nbsp_tail.height > plain_a.height,
        "{plain_nbsp_tail:?}"
    );

    let svg_a = measurer.measure_wrapped("A", &style, None, WrapMode::SvgLike);
    let svg_nbsp_tail = measurer.measure_wrapped("A\n\u{00A0}", &style, None, WrapMode::SvgLike);
    assert_eq!(svg_nbsp_tail.line_count, 2, "{svg_nbsp_tail:?}");
    assert!(svg_nbsp_tail.height > svg_a.height, "{svg_nbsp_tail:?}");
}

#[test]
fn html_break_spaces_preserves_trailing_spaces() {
    let measurer = DeterministicTextMeasurer::default();
    let style = TextStyle {
        font_family: Some("\"trebuchet ms\", verdana, arial, sans-serif".to_string()),
        font_size: 16.0,
        font_weight: None,
        font_style: None,
    };

    let actual =
        measure_html_with_inline_styles(&measurer, "alpha ", &style, None, WrapMode::HtmlLike);
    let expected = measurer.measure_wrapped("alpha ", &style, None, WrapMode::HtmlLike);

    assert_same_metrics_after_dom_rounding(actual, expected);
}

#[test]
fn markdown_inline_styles_delegate_to_operation_specific_font_variants() {
    let measurer = DeterministicTextMeasurer::default();
    let regular = TextStyle {
        font_family: Some("\"trebuchet ms\", verdana, arial, sans-serif".to_string()),
        font_size: 16.0,
        font_weight: None,
        font_style: None,
    };
    let bold = TextStyle {
        font_weight: Some("700".to_string()),
        ..regular.clone()
    };
    let italic = TextStyle {
        font_style: Some("italic".to_string()),
        ..regular.clone()
    };

    let italic_actual = measure_markdown_with_inline_styles(
        &measurer,
        "*Moving*",
        &regular,
        None,
        WrapMode::HtmlLike,
    );
    let italic_expected = measurer.measure_wrapped("Moving", &italic, None, WrapMode::HtmlLike);
    assert_same_metrics_after_dom_rounding(italic_actual, italic_expected);

    let bold_actual = measure_markdown_with_inline_styles(
        &measurer,
        "**Two**",
        &regular,
        None,
        WrapMode::SvgLike,
    );
    let bold_expected = measurer.measure_svg_text_computed_length_px("Two", &bold);
    assert_eq!(bold_actual.width, round_to_1_64_px(bold_expected));

    let mixed = measure_markdown_with_inline_styles(
        &measurer,
        "plain **Bold** *Italic*",
        &regular,
        None,
        WrapMode::HtmlLike,
    );
    let mixed_expected = measurer
        .measure_wrapped("plain ", &regular, None, WrapMode::HtmlLike)
        .width
        + measurer
            .measure_wrapped("Bold", &bold, None, WrapMode::HtmlLike)
            .width
        + measurer
            .measure_wrapped(" ", &regular, None, WrapMode::HtmlLike)
            .width
        + measurer
            .measure_wrapped("Italic", &italic, None, WrapMode::HtmlLike)
            .width;
    assert_eq!(mixed.width, round_to_1_64_px(mixed_expected));
}

#[test]
fn flowchart_html_unwrapped_measurement_scales_with_font_size() {
    let measurer = DeterministicTextMeasurer::default();
    let style_15 = TextStyle {
        font_family: Some("\"trebuchet ms\", verdana, arial, sans-serif".to_string()),
        font_size: 15.0,
        font_weight: None,
        font_style: None,
    };
    let style_30 = TextStyle {
        font_size: 30.0,
        ..style_15.clone()
    };

    let small =
        measurer.measure_wrapped("synthetic scale probe", &style_15, None, WrapMode::HtmlLike);
    let large =
        measurer.measure_wrapped("synthetic scale probe", &style_30, None, WrapMode::HtmlLike);
    assert_eq!(small.line_count, 1);
    assert_eq!(large.line_count, 1);
    assert!((large.width / small.width - 2.0).abs() < 0.01);
    assert!((large.height / small.height - 2.0).abs() < 0.01);
}

#[test]
fn flowchart_html_fontawesome_icon_width_uses_nominal_boundary() {
    // Model standard FontAwesome icons using Mermaid 11.15's inline FA box width instead of
    // the browser's per-icon glyph advance.
    let measurer = DeterministicTextMeasurer::default();
    let style = TextStyle {
        font_family: Some("\"trebuchet ms\", verdana, arial, sans-serif".to_string()),
        font_size: 16.0,
        font_weight: None,
        font_style: None,
    };

    let html = "<p><i class=\"fa fa-car\"></i> Car</p>";
    let m =
        measure_html_with_inline_styles(&measurer, html, &style, Some(200.0), WrapMode::HtmlLike);
    let plain = measure_html_with_inline_styles(
        &measurer,
        "<p>Car</p>",
        &style,
        Some(200.0),
        WrapMode::HtmlLike,
    );
    assert_finite_positive_metrics(m);
    assert!(m.width > plain.width);
    assert_eq!(m.height, plain.height);
    assert_eq!(m.line_count, 1);
}

#[test]
fn flowchart_html_fontawesome_custom_pack_icon_width_uses_nominal_boundary() {
    // Mermaid 11.15 keeps the inline icon box width even for the documented custom-pack example.
    let measurer = DeterministicTextMeasurer::default();
    let style = TextStyle {
        font_family: Some("\"trebuchet ms\", verdana, arial, sans-serif".to_string()),
        font_size: 16.0,
        font_weight: None,
        font_style: None,
    };

    let html = "<p><i class=\"fab fa-truck-bold\"></i> a custom icon</p>";
    let m =
        measure_html_with_inline_styles(&measurer, html, &style, Some(200.0), WrapMode::HtmlLike);
    let plain = measure_html_with_inline_styles(
        &measurer,
        "<p>a custom icon</p>",
        &style,
        Some(200.0),
        WrapMode::HtmlLike,
    );
    assert_finite_positive_metrics(m);
    assert!(m.width > plain.width);
    assert_eq!(m.height, plain.height);
    assert_eq!(m.line_count, 1);
}

#[test]
fn fontawesome_icon_substitution_matches_mermaid_source_boundaries() {
    assert_eq!(
        replace_fontawesome_icons("This is an icon: fa:fa-user and fab:fa-github"),
        r#"This is an icon: <i class="fa fa-user"></i> and <i class="fab fa-github"></i>"#
    );
    assert_eq!(
        replace_fontawesome_icons("Icons galore: fa:fa-arrow-right, fak:fa-truck, fas:fa-home"),
        r#"Icons galore: <i class="fa fa-arrow-right"></i>, <i class="fak fa-truck"></i>, <i class="fas fa-home"></i>"#
    );
    assert_eq!(
        replace_fontawesome_icons(
            "Here is a long icon: fak:fa-truck-driving-long-winding-road in use"
        ),
        r#"Here is a long icon: <i class="fak fa-truck-driving-long-winding-road"></i> in use"#
    );
    assert_eq!(
        replace_fontawesome_icons("no icons: faa:fa-user fa:fa- fa:fa-éclair"),
        "no icons: faa:fa-user fa:fa- fa:fa-éclair"
    );
    assert_eq!(
        replace_fontawesome_icons("prefix can match inside text: xfa:fa-user!"),
        r#"prefix can match inside text: x<i class="fa fa-user"></i>!"#
    );
}

#[test]
fn flowchart_label_metrics_for_layout_fontawesome_uses_nominal_boundary() {
    // Non-markdown Flowchart icon labels should use the same HTML fragment measurement path as
    // emitted `<foreignObject>` content, with the same Mermaid 11.15 icon width boundary.
    let measurer = DeterministicTextMeasurer::default();
    let style = TextStyle {
        font_family: Some("\"trebuchet ms\", verdana, arial, sans-serif".to_string()),
        font_size: 16.0,
        font_weight: None,
        font_style: None,
    };
    let cfg = merman_core::MermaidConfig::default();

    let actual = crate::graph_label::flowchart_label_metrics_for_layout(
        crate::graph_label::FlowchartLabelMetricsRequest {
            measurer: &measurer,
            raw_label: "fa:fa-car Car",
            label_type: "text",
            style: &style,
            max_width_px: Some(200.0),
            wrap_mode: WrapMode::HtmlLike,
            config: &cfg,
            math_renderer: None,
        },
    );
    let html = format!("<p>{}</p>", replace_fontawesome_icons("fa:fa-car Car"));
    let expected =
        measure_html_with_inline_styles(&measurer, &html, &style, Some(200.0), WrapMode::HtmlLike);
    assert_same_metrics(actual, expected);
}

#[test]
fn flowchart_label_metrics_plain_text_uses_dom_text_operation() {
    let measurer = DeterministicTextMeasurer::default();
    let style = TextStyle {
        font_family: Some("\"trebuchet ms\", verdana, arial, sans-serif".to_string()),
        font_size: 16.0,
        font_weight: None,
        font_style: None,
    };
    let cfg = merman_core::MermaidConfig::default();

    let actual = crate::graph_label::flowchart_label_metrics_for_layout(
        crate::graph_label::FlowchartLabelMetricsRequest {
            measurer: &measurer,
            raw_label: "synthetic",
            label_type: "text",
            style: &style,
            max_width_px: Some(200.0),
            wrap_mode: WrapMode::HtmlLike,
            config: &cfg,
            math_renderer: None,
        },
    );
    let expected = measurer.measure_wrapped("synthetic", &style, Some(200.0), WrapMode::HtmlLike);
    assert_same_metrics(actual, expected);
}

#[test]
fn flowchart_label_metrics_for_layout_fontawesome_icon_only_lines_preserve_breaks() {
    let measurer = DeterministicTextMeasurer::default();
    let style = TextStyle {
        font_family: Some("\"trebuchet ms\", verdana, arial, sans-serif".to_string()),
        font_size: 16.0,
        font_weight: None,
        font_style: None,
    };
    let cfg = merman_core::MermaidConfig::default();

    let twitter = crate::graph_label::flowchart_label_metrics_for_layout(
        crate::graph_label::FlowchartLabelMetricsRequest {
            measurer: &measurer,
            raw_label: "fa:fa-twitter<br/>for peace",
            label_type: "text",
            style: &style,
            max_width_px: Some(200.0),
            wrap_mode: WrapMode::HtmlLike,
            config: &cfg,
            math_renderer: None,
        },
    );
    assert_finite_positive_metrics(twitter);
    assert_eq!(twitter.line_count, 2);

    let camera = crate::graph_label::flowchart_label_metrics_for_layout(
        crate::graph_label::FlowchartLabelMetricsRequest {
            measurer: &measurer,
            raw_label: "fa:fa-camera-retro<br/>capture<br/>moments",
            label_type: "text",
            style: &style,
            max_width_px: Some(200.0),
            wrap_mode: WrapMode::HtmlLike,
            config: &cfg,
            math_renderer: None,
        },
    );
    assert_finite_positive_metrics(camera);
    assert_eq!(camera.line_count, 3);
    assert!(camera.height > twitter.height);
}

#[test]
fn flowchart_label_metrics_for_layout_fontawesome_keeps_icon_runs_bounded() {
    // Mermaid upstream fixture:
    // fixtures/upstream-svgs/flowchart/upstream_cypress_flowchart_handdrawn_spec_fhd7_should_render_a_flowchart_full_of_icons_007.svg
    let measurer = DeterministicTextMeasurer::default();
    let style = TextStyle {
        font_family: Some("\"trebuchet ms\", verdana, arial, sans-serif".to_string()),
        font_size: 16.0,
        font_weight: None,
        font_style: None,
    };
    let cfg = merman_core::MermaidConfig::default();

    let database = crate::graph_label::flowchart_label_metrics_for_layout(
        crate::graph_label::FlowchartLabelMetricsRequest {
            measurer: &measurer,
            raw_label: r"fa:fa-database [DBServer\SharedDbInstance]",
            label_type: "text",
            style: &style,
            max_width_px: Some(200.0),
            wrap_mode: WrapMode::HtmlLike,
            config: &cfg,
            math_renderer: None,
        },
    );
    assert!(database.width > 0.0 && database.width <= 200.0);
    assert!(database.line_count >= 1);

    let support_db = crate::graph_label::flowchart_label_metrics_for_layout(
        crate::graph_label::FlowchartLabelMetricsRequest {
            measurer: &measurer,
            raw_label: r"fa:fa-circle [DBServer\SharedDbInstance].[SupportDb]",
            label_type: "text",
            style: &style,
            max_width_px: Some(200.0),
            wrap_mode: WrapMode::HtmlLike,
            config: &cfg,
            math_renderer: None,
        },
    );
    assert!(support_db.width > 0.0 && support_db.width <= 200.0);
    assert!(support_db.line_count >= database.line_count);
    assert!(support_db.height >= database.height);
}

#[test]
fn default_font_html_advance_is_monotonic_for_appended_text() {
    let measurer = DeterministicTextMeasurer::default();
    let style = TextStyle {
        font_family: Some("\"trebuchet ms\", verdana, arial, sans-serif".to_string()),
        font_size: 16.0,
        font_weight: None,
        font_style: None,
    };

    let metrics = [
        "synthetic",
        "synthetic label",
        "synthetic label with punctuation: []{}",
    ]
    .map(|text| measurer.measure_wrapped(text, &style, None, WrapMode::HtmlLike));

    for metrics in metrics {
        assert_finite_positive_metrics(metrics);
        assert_eq!(metrics.line_count, 1);
    }
    assert!(metrics.windows(2).all(|pair| pair[1].width > pair[0].width));
}

#[test]
fn default_font_repeated_glyph_runs_have_monotonic_advance() {
    let measurer = DeterministicTextMeasurer::default();
    let style = TextStyle {
        font_family: Some("\"trebuchet ms\", verdana, arial, sans-serif".to_string()),
        font_size: 16.0,
        font_weight: None,
        font_style: None,
    };

    let widths = ["s", "ss", "sss", "ssss", "sssss"].map(|text| {
        let metrics = measurer.measure_wrapped(text, &style, None, WrapMode::HtmlLike);
        assert_finite_positive_metrics(metrics);
        metrics.width
    });
    assert!(
        widths.windows(2).all(|pair| pair[1] > pair[0]),
        "appending a visible glyph must increase advance: {widths:?}"
    );

    let mixed = ["ttts", "tttss", "tttsss"].map(|text| {
        measurer
            .measure_wrapped(text, &style, None, WrapMode::HtmlLike)
            .width
    });
    assert!(mixed.windows(2).all(|pair| pair[1] > pair[0]));
}

#[test]
fn flowchart_multiline_html_label_uses_widest_measured_line() {
    let measurer = DeterministicTextMeasurer::default();
    let style = TextStyle {
        font_family: Some("\"trebuchet ms\", verdana, arial, sans-serif".to_string()),
        font_size: 16.0,
        font_weight: None,
        font_style: None,
    };
    let cfg = merman_core::MermaidConfig::default();
    let lines = [
        "short run",
        "a substantially wider synthetic run",
        "middle run",
    ];
    let raw_label = lines.join("<br/>");

    let metrics = crate::graph_label::flowchart_label_metrics_for_layout(
        crate::graph_label::FlowchartLabelMetricsRequest {
            measurer: &measurer,
            raw_label: &raw_label,
            label_type: "text",
            style: &style,
            max_width_px: None,
            wrap_mode: WrapMode::HtmlLike,
            config: &cfg,
            math_renderer: None,
        },
    );
    let widest_line = lines
        .map(|line| {
            measurer
                .measure_wrapped(line, &style, None, WrapMode::HtmlLike)
                .width
        })
        .into_iter()
        .fold(0.0, f64::max);

    assert_eq!(metrics.line_count, lines.len());
    assert_eq!(metrics.width, round_to_1_64_px(widest_line));
    assert!(metrics.height > style.font_size);
}

#[test]
fn deterministic_ascii_punctuation_has_finite_advances() {
    let measurer = DeterministicTextMeasurer::default();
    let style = TextStyle {
        font_family: Some("\"trebuchet ms\", verdana, arial, sans-serif".to_string()),
        font_size: 16.0,
        font_weight: None,
        font_style: None,
    };

    let open_brace = measurer.measure_wrapped("{", &style, None, WrapMode::HtmlLike);
    let close_brace = measurer.measure_wrapped("}", &style, None, WrapMode::HtmlLike);
    assert_finite_positive_metrics(open_brace);
    assert_finite_positive_metrics(close_brace);

    let bracketed = measurer.measure_wrapped("[x] {y} (z)", &style, None, WrapMode::HtmlLike);
    assert_finite_positive_metrics(bracketed);
    assert!(bracketed.width > open_brace.width + close_brace.width);
}

#[test]
fn deterministic_nbsp_uses_regular_space_advance() {
    let measurer = DeterministicTextMeasurer::default();
    let style = TextStyle {
        font_family: Some("\"trebuchet ms\", verdana, arial, sans-serif".to_string()),
        font_size: 16.0,
        font_weight: None,
        font_style: None,
    };

    let regular_space = measurer.measure_wrapped("A B", &style, None, WrapMode::HtmlLike);
    let non_breaking_space =
        measurer.measure_wrapped("A\u{00A0}B", &style, None, WrapMode::HtmlLike);

    assert_finite_positive_metrics(regular_space);
    assert_finite_positive_metrics(non_breaking_space);
    assert_eq!(non_breaking_space.width, regular_space.width);
    assert_eq!(non_breaking_space.line_count, regular_space.line_count);
}

#[test]
fn deterministic_v_comma_pair_uses_additive_heuristic_advance() {
    let measurer = DeterministicTextMeasurer::default();
    let style = TextStyle {
        font_family: Some("\"trebuchet ms\", verdana, arial, sans-serif".to_string()),
        font_size: 16.0,
        font_weight: None,
        font_style: None,
    };

    let v = measurer.measure_wrapped("v", &style, None, WrapMode::HtmlLike);
    let comma = measurer.measure_wrapped(",", &style, None, WrapMode::HtmlLike);
    let pair = measurer.measure_wrapped("v,", &style, None, WrapMode::HtmlLike);
    assert_finite_positive_metrics(pair);
    assert!(pair.width <= v.width + comma.width);
    assert!(pair.width > v.width.max(comma.width));
}

#[test]
fn html_measurement_ignores_inactive_wrap_limit() {
    let measurer = DeterministicTextMeasurer::default();
    let style = TextStyle {
        font_family: Some("\"trebuchet ms\", verdana, arial, sans-serif".to_string()),
        font_size: 16.0,
        font_weight: None,
        font_style: None,
    };
    let text = "untrained inactive wrap probe";

    let unwrapped = measurer.measure_wrapped(text, &style, None, WrapMode::HtmlLike);
    let wrapped = measurer.measure_wrapped(
        text,
        &style,
        Some(unwrapped.width + style.font_size),
        WrapMode::HtmlLike,
    );
    assert_same_metrics(wrapped, unwrapped);
}

#[test]
fn calculate_text_dimensions_collapses_svg_tspan_ascii_whitespace() {
    let measurer = DeterministicTextMeasurer::default();
    let style = TextStyle {
        font_family: Some("\"trebuchet ms\", verdana, arial, sans-serif;".to_string()),
        font_size: 16.0,
        font_weight: Some("400".to_string()),
        font_style: None,
    };

    let single = measurer.measure_mermaid_calculate_text_dimensions("A B", &style);
    let repeated = measurer.measure_mermaid_calculate_text_dimensions("  A  B  ", &style);
    let ascii_controls = measurer.measure_mermaid_calculate_text_dimensions("\tA\n\r B\t", &style);
    let non_breaking =
        measurer.measure_mermaid_calculate_text_dimensions("A\u{00a0}\u{00a0}B", &style);

    assert_eq!(repeated.width.to_bits(), single.width.to_bits());
    assert_eq!(repeated.height.to_bits(), single.height.to_bits());
    assert_eq!(ascii_controls.width.to_bits(), single.width.to_bits());
    assert_eq!(ascii_controls.height.to_bits(), single.height.to_bits());
    assert!(non_breaking.width > single.width);
}

#[test]
fn svg_wrapped_width_tracks_a_bounded_emitted_line() {
    let measurer = DeterministicTextMeasurer::default();
    let style = TextStyle {
        font_family: Some("\"trebuchet ms\", verdana, arial, sans-serif".to_string()),
        font_size: 16.0,
        font_weight: None,
        font_style: None,
    };
    let text = "A synthetic cluster title with punctuation: (q/r/s)";
    let unwrapped = measurer.measure_wrapped(text, &style, None, WrapMode::SvgLike);
    let metrics =
        measurer.measure_wrapped(text, &style, Some(unwrapped.width / 2.0), WrapMode::SvgLike);
    assert_finite_positive_metrics(metrics);
    assert!(metrics.line_count > 1);
    assert!(metrics.width < unwrapped.width);
    assert!(metrics.height > unwrapped.height);
}

#[test]
fn flowchart_html_punctuation_wraps_at_spaces() {
    let measurer = DeterministicTextMeasurer::default();
    let style = TextStyle {
        font_family: Some("\"trebuchet ms\", verdana, arial, sans-serif".to_string()),
        font_size: 16.0,
        font_weight: None,
        font_style: None,
    };
    let title = "Synthetic punctuation (q/r/s) + dashes - and spaces";
    let unwrapped = measurer.measure_wrapped(title, &style, None, WrapMode::HtmlLike);
    let limit = unwrapped.width / 2.0;
    let metrics = measurer.measure_wrapped(title, &style, Some(limit), WrapMode::HtmlLike);
    assert_finite_positive_metrics(metrics);
    assert!(metrics.line_count > 1);
    assert!(
        metrics.width <= limit + 1.0 / 64.0,
        "DOM width may differ from the wrap limit by at most one 1/64px lattice step: {metrics:?}, limit={limit}"
    );
    assert!(metrics.height > unwrapped.height);
}

#[test]
fn flowchart_svg_layout_metrics_follow_the_shared_text_operation() {
    let measurer = DeterministicTextMeasurer::default();
    let style = TextStyle {
        font_family: Some("\"trebuchet ms\", verdana, arial, sans-serif".to_string()),
        font_size: 16.0,
        font_weight: None,
        font_style: None,
    };
    let text = "synthetic node alpha";

    let direct = measurer.measure_wrapped(text, &style, Some(200.0), WrapMode::SvgLike);
    let extended =
        measurer.measure_wrapped("synthetic node alpha beta", &style, None, WrapMode::SvgLike);
    assert!(extended.width > direct.width);

    let cfg = merman_core::MermaidConfig::default();
    let layout =
        flowchart_label_metrics_for_layout(crate::graph_label::FlowchartLabelMetricsRequest {
            measurer: &measurer,
            raw_label: text,
            label_type: "text",
            style: &style,
            max_width_px: Some(200.0),
            wrap_mode: WrapMode::SvgLike,
            config: &cfg,
            math_renderer: None,
        });
    assert_same_metrics(layout, direct);
}

#[test]
fn svg_and_html_operations_keep_operation_specific_heights() {
    let measurer = DeterministicTextMeasurer::default();
    let style = TextStyle {
        font_family: None,
        font_size: 16.0,
        font_weight: None,
        font_style: None,
    };
    let text = "synthetic";

    let svg = measurer.measure_wrapped(text, &style, None, WrapMode::SvgLike);
    let html = measurer.measure_wrapped(text, &style, None, WrapMode::HtmlLike);
    assert_finite_positive_metrics(svg);
    assert_finite_positive_metrics(html);
    assert_eq!(svg.line_count, 1);
    assert_eq!(html.line_count, 1);
    assert!(svg.height < html.height);
}

#[test]
fn default_font_html_hyphenated_compound_wraps_at_dynamic_limit() {
    let measurer = DeterministicTextMeasurer::default();
    let style = TextStyle {
        font_family: Some("\"trebuchet ms\", verdana, arial, sans-serif".to_string()),
        font_size: 16.0,
        font_weight: None,
        font_style: None,
    };
    let text = "Synthetic prose before half-rounded-compound suffix";
    let unwrapped = measurer.measure_wrapped(text, &style, None, WrapMode::HtmlLike);
    let limit = unwrapped.width / 2.0;

    let metrics = measurer.measure_wrapped(text, &style, Some(limit), WrapMode::HtmlLike);
    assert!(metrics.width <= limit);
    assert!(metrics.height > unwrapped.height);
    assert!(metrics.line_count > 1);
}

#[test]
fn svg_title_bbox_vertical_extents_are_font_agnostic() {
    let trebuchet = TextStyle {
        font_family: Some("\"trebuchet ms\", verdana, arial, sans-serif".to_string()),
        font_size: 18.0,
        font_weight: None,
        font_style: None,
    };
    let courier = TextStyle {
        font_family: Some("courier".to_string()),
        font_size: 18.0,
        font_weight: None,
        font_style: None,
    };
    let courier_stack = TextStyle {
        font_family: Some("\"Courier New\", courier, monospace;".to_string()),
        font_size: 18.0,
        font_weight: None,
        font_style: None,
    };

    assert_eq!(
        svg_title_bbox_vertical_extents_px(&courier_stack),
        svg_title_bbox_vertical_extents_px(&courier)
    );
    assert_eq!(
        svg_title_bbox_vertical_extents_px(&courier_stack),
        svg_title_bbox_vertical_extents_px(&trebuchet)
    );
}

#[test]
fn flowchart_title_bbox_uses_symmetric_shared_advance() {
    let measurer = DeterministicTextMeasurer::default();
    let style = TextStyle {
        font_family: Some("\"trebuchet ms\", verdana, arial, sans-serif".to_string()),
        font_size: 18.0,
        font_weight: None,
        font_style: None,
    };
    let text = "synthetic title probe";

    let (left, right) = measurer.measure_svg_title_bbox_x(text, &style);
    let bbox_width = measurer.measure_svg_simple_text_bbox_width_px(text, &style);
    assert!(left.is_finite() && left > 0.0);
    assert_eq!(left, right);
    assert!(bbox_width.is_finite() && bbox_width >= left + right);
}

#[test]
fn svg_single_run_keeps_literal_br_with_backslash_t_on_one_line() {
    let measurer = DeterministicTextMeasurer::default();
    let style = TextStyle {
        font_family: Some("\"trebuchet ms\", verdana, arial, sans-serif;".to_string()),
        font_size: 16.0,
        font_weight: None,
        font_style: None,
    };

    // Mermaid `lineBreakRegex` should not treat this as a `<br>` break because `\\t` is a
    // literal backslash + `t`, not whitespace.
    let text = "multiline<br \\t/>text";
    assert_eq!(split_html_br_lines(text), vec![text]);

    let literal = measurer.measure_wrapped(text, &style, None, WrapMode::SvgLikeSingleRun);
    let without_literal_marker =
        measurer.measure_wrapped("multilinetext", &style, None, WrapMode::SvgLikeSingleRun);
    assert_eq!(literal.line_count, 1);
    assert!(literal.width.is_finite() && literal.width > without_literal_marker.width);
}

#[test]
fn deterministic_svg_bbox_operations_scale_with_font_size() {
    let measurer = DeterministicTextMeasurer::default();
    let style_16 = TextStyle {
        font_family: Some("\"trebuchet ms\", verdana, arial, sans-serif;".to_string()),
        font_size: 16.0,
        font_weight: None,
        font_style: None,
    };
    let style_32 = TextStyle {
        font_size: 32.0,
        ..style_16.clone()
    };
    let text = "synthetic-sequence-probe-omega-42";

    for (width_16, width_32) in [
        (
            measurer.measure_svg_simple_text_bbox_width_px(text, &style_16),
            measurer.measure_svg_simple_text_bbox_width_px(text, &style_32),
        ),
        (
            measurer.measure_svg_raw_text_bbox_width_px(text, &style_16),
            measurer.measure_svg_raw_text_bbox_width_px(text, &style_32),
        ),
        (
            measurer.measure_svg_tspan_text_bbox_width_px(text, &style_16),
            measurer.measure_svg_tspan_text_bbox_width_px(text, &style_32),
        ),
    ] {
        assert!(width_16.is_finite() && width_16 > 0.0);
        assert!(width_32.is_finite() && width_32 > width_16);
        assert!(
            (width_32 / width_16 - 2.0).abs() < 0.01,
            "deterministic SVG measurement should scale with font size: {width_16} -> {width_32}"
        );
    }
}

#[test]
fn wrap_label_like_mermaid_respects_generalized_probe_thresholds() {
    let measurer = DeterministicTextMeasurer::default();
    let style = TextStyle {
        font_family: Some("\"trebuchet ms\", verdana, arial, sans-serif;".to_string()),
        font_size: 16.0,
        font_weight: None,
        font_style: None,
    };
    let text = "This is a longer message that should be wrapped by Mermaid's default behavior";

    let probe = measurer.measure_svg_simple_text_bbox_width_for_wrap_px(text, &style);
    assert!(probe.is_finite() && probe > 0.0);
    assert_eq!(
        wrap_label_like_mermaid_lines(text, &measurer, &style, probe + 1.0),
        vec![text.to_string()],
        "a threshold above the measured candidate must preserve the line"
    );

    let wrapped = wrap_label_like_mermaid_lines(text, &measurer, &style, probe / 2.0);
    assert!(
        wrapped.len() > 1,
        "a threshold below the measured candidate must use the normal Mermaid wrapLabel flow"
    );
}

#[test]
fn wrap_label_like_mermaid_does_not_split_escaped_br() {
    let measurer = DeterministicTextMeasurer::default();
    let style = TextStyle {
        font_family: Some("\"trebuchet ms\", verdana, arial, sans-serif;".to_string()),
        font_size: 16.0,
        font_weight: None,
        font_style: None,
    };

    let lines =
        wrap_label_like_mermaid_lines("multiline<br>using #lt;br#gt;", &measurer, &style, 10_000.0);
    assert_eq!(
        lines,
        vec!["multiline".to_string(), "using #lt;br#gt;".to_string()],
        "wrapLabel should short-circuit when explicit `<br>` breaks are present, and must not treat escaped `#lt;br#gt;` as a break"
    );
}

#[test]
fn flowchart_label_metrics_for_layout_measures_markdown_inline_html_like_mermaid() {
    let measurer = DeterministicTextMeasurer::default();
    let style = TextStyle {
        font_family: Some("\"trebuchet ms\", verdana, arial, sans-serif".to_string()),
        font_size: 16.0,
        font_weight: None,
        font_style: None,
    };
    let cfg = merman_core::MermaidConfig::default();
    let markdown = "This is **bold** </br>and <strong>strong</strong>";
    assert!(mermaid_markdown_contains_html_tags(markdown));

    let html = mermaid_markdown_to_html_label_fragment(markdown, true);
    let html_metrics =
        measure_html_with_inline_styles(&measurer, &html, &style, Some(200.0), WrapMode::HtmlLike);
    assert_finite_positive_metrics(html_metrics);
    assert_eq!(html_metrics.line_count, 2);

    let metrics = crate::graph_label::flowchart_label_metrics_for_layout(
        crate::graph_label::FlowchartLabelMetricsRequest {
            measurer: &measurer,
            raw_label: markdown,
            label_type: "markdown",
            style: &style,
            max_width_px: Some(200.0),
            wrap_mode: WrapMode::HtmlLike,
            config: &cfg,
            math_renderer: None,
        },
    );
    assert_same_metrics(metrics, html_metrics);
}

#[test]
fn flowchart_html_markdown_metrics_preserve_paragraph_break_height() {
    let measurer = DeterministicTextMeasurer::default();
    let style = TextStyle {
        font_family: Some("\"trebuchet ms\", verdana, arial, sans-serif".to_string()),
        font_size: 16.0,
        font_weight: None,
        font_style: None,
    };
    let cfg = merman_core::MermaidConfig::default();
    let measure = |markdown: &str| {
        crate::graph_label::flowchart_label_metrics_for_layout(
            crate::graph_label::FlowchartLabelMetricsRequest {
                measurer: &measurer,
                raw_label: markdown,
                label_type: "markdown",
                style: &style,
                max_width_px: None,
                wrap_mode: WrapMode::HtmlLike,
                config: &cfg,
                math_renderer: None,
            },
        )
    };

    let single_paragraph = measure("Synthetic first sentence.\nSynthetic second sentence.");
    let two_paragraphs = measure("Synthetic first sentence.\n\nSynthetic second sentence.");
    assert_finite_positive_metrics(single_paragraph);
    assert_finite_positive_metrics(two_paragraphs);
    assert!(two_paragraphs.line_count > single_paragraph.line_count);
    assert!(two_paragraphs.height > single_paragraph.height);
}

#[test]
fn markdown_svg_wrapping_keeps_raw_html_tags_literal_but_wraps_like_mermaid() {
    use MermaidMarkdownWordType::*;

    let measurer = DeterministicTextMeasurer::default();
    let style = TextStyle {
        font_family: Some("\"trebuchet ms\", verdana, arial, sans-serif".to_string()),
        font_size: 16.0,
        font_weight: None,
        font_style: None,
    };

    let lines = mermaid_markdown_to_wrapped_word_lines(
        &measurer,
        "This is **bold** </br>and <strong>strong</strong>",
        &style,
        Some(200.0),
        WrapMode::SvgLike,
    );
    assert_eq!(
        lines,
        vec![
            vec![
                ("This".to_string(), Normal),
                ("is".to_string(), Normal),
                ("bold".to_string(), Strong),
            ],
            vec![
                ("and".to_string(), Normal),
                ("<strong>".to_string(), Normal),
                ("strong".to_string(), Normal),
            ],
            vec![("</strong>".to_string(), Normal)],
        ]
    );

    let entity_max_width = ["&nbsp;Edge", "markdown&nbsp;"]
        .into_iter()
        .map(|word| measurer.measure_svg_text_computed_length_px(word, &style))
        .fold(0.0_f64, f64::max);
    let entity_lines = mermaid_markdown_to_wrapped_word_lines(
        &measurer,
        "&nbsp;Edge markdown&nbsp;",
        &style,
        Some(entity_max_width),
        WrapMode::SvgLike,
    );
    assert_eq!(
        entity_lines,
        vec![
            vec![("&nbsp;Edge".to_string(), Normal)],
            vec![("markdown&nbsp;".to_string(), Normal)],
        ]
    );
}
