use super::*;
use std::collections::BTreeMap;

fn assert_fragments_in_order(css: &str, fragments: &[&str]) {
    let mut cursor = 0;
    for fragment in fragments {
        let offset = css[cursor..]
            .find(fragment)
            .unwrap_or_else(|| panic!("missing CSS fragment: {fragment}"));
        cursor += offset + fragment.len();
    }
}

fn css_properties_for_selector(css: &str, selector: &str) -> BTreeMap<String, String> {
    css.split('}')
        .filter_map(|rule| rule.rsplit_once('{'))
        .find_map(|(selectors, declarations)| {
            selectors
                .split(',')
                .map(str::trim)
                .any(|candidate| candidate == selector)
                .then(|| {
                    declarations
                        .split(';')
                        .filter_map(|declaration| declaration.split_once(':'))
                        .map(|(property, value)| {
                            (property.trim().to_string(), value.trim().to_string())
                        })
                        .collect()
                })
        })
        .unwrap_or_else(|| panic!("missing CSS selector: {selector}"))
}

fn assert_css_properties(css: &str, selector: &str, expected: &[(&str, &str)]) {
    let actual = css_properties_for_selector(css, selector);
    for (property, value) in expected {
        assert_eq!(
            actual.get(*property).map(String::as_str),
            Some(*value),
            "{selector} {property}"
        );
    }
}

#[test]
fn mermaid_base_css_fragments_keep_parity_order() {
    let cfg = serde_json::json!({});
    let base_fragments = [
        r#"#diag{font-family:"trebuchet ms",verdana,arial,sans-serif;font-size:16px;fill:#333;}"#,
        r#"@keyframes edge-animation-frame{from{stroke-dashoffset:0;}}@keyframes dash{to{stroke-dashoffset:0;}}"#,
        r#"#diag .edge-animation-slow{stroke-dasharray:9,5!important;stroke-dashoffset:900;animation:dash 50s linear infinite;stroke-linecap:round;}#diag .edge-animation-fast{stroke-dasharray:9,5!important;stroke-dashoffset:900;animation:dash 20s linear infinite;stroke-linecap:round;}"#,
        r#"#diag .error-icon{fill:#552222;}#diag .error-text{fill:#552222;stroke:#552222;}"#,
        r#"#diag .edge-thickness-normal{stroke-width:1px;}#diag .edge-thickness-thick{stroke-width:3.5px;}#diag .edge-pattern-solid{stroke-dasharray:0;}#diag .edge-thickness-invisible{stroke-width:0;fill:none;}#diag .edge-pattern-dashed{stroke-dasharray:3;}#diag .edge-pattern-dotted{stroke-dasharray:2;}"#,
        r#"#diag .marker{fill:#333333;stroke:#333333;}#diag .marker.cross{stroke:#333333;}"#,
        r#"#diag svg{font-family:"trebuchet ms",verdana,arial,sans-serif;font-size:16px;}#diag p{margin:0;}"#,
    ];

    let info = info_css_with_config("diag", &cfg);
    assert_fragments_in_order(&info, &base_fragments);
    assert!(info.ends_with(
        r#"#diag :root{--mermaid-font-family:"trebuchet ms",verdana,arial,sans-serif;}"#
    ));

    #[cfg(feature = "layout-cytoscape")]
    {
        let architecture = architecture_css_with_config("diag", &cfg);
        assert_fragments_in_order(
            &architecture,
            &[
                &base_fragments[..],
                &[r#"#diag .edge{stroke-width:3;stroke:#333333;fill:none;}"#],
            ]
            .concat(),
        );
        assert!(architecture.ends_with(
            r#"#diag :root{--mermaid-font-family:"trebuchet ms",verdana,arial,sans-serif;}"#
        ));
    }

    let er = er_css("diag", &cfg).expect("valid ER theme colors");
    assert_fragments_in_order(
        &er,
        &[
            &base_fragments[..],
            &[r#"#diag .entityBox{fill:#ECECFF;stroke:#9370DB;}"#],
        ]
        .concat(),
    );
    assert!(er.ends_with(
        r#"#diag :root{--mermaid-font-family:"trebuchet ms",verdana,arial,sans-serif;}"#
    ));
}

#[test]
fn explicit_empty_root_font_omits_the_rule_without_changing_theme_font() {
    for root_font in ["", "   "] {
        let config = serde_json::json!({
            "fontFamily": root_font,
            "themeVariables": { "fontFamily": "Arial, sans-serif" }
        });
        for css in [
            info_css_with_config("diag", &config),
            er_css("diag", &config).unwrap(),
        ] {
            assert!(css.starts_with("#diag{font-family:Arial,sans-serif;"));
            assert!(!css.contains("--mermaid-font-family"));
        }
    }
}

#[test]
fn shared_and_er_css_keep_theme_font_separate_from_root_custom_property() {
    let config = serde_json::json!({
        "fontFamily": "Courier, monospace",
        "themeVariables": { "fontFamily": "Arial, sans-serif" }
    });
    for css in [
        info_css_with_config("diag", &config),
        er_css("diag", &config).unwrap(),
    ] {
        assert!(css.starts_with("#diag{font-family:Arial,sans-serif;"));
        assert!(css.ends_with("#diag :root{--mermaid-font-family:Courier,monospace;}"));
    }
}

#[test]
fn mermaid_base_css_exposes_the_complete_common_neo_contract() {
    let css = info_css_with_config(
        "diag",
        &serde_json::json!({
            "themeVariables": {
                "nodeBorder": "#123456",
                "strokeWidth": 3,
                "useGradient": true,
                "dropShadow": "url(#drop-shadow)"
            }
        }),
    );
    let gradient = "url(#diag-gradient)";
    let shadow = "url(#diag-drop-shadow)";

    assert_css_properties(
        &css,
        "#diag .edge-thickness-normal",
        &[("stroke-width", "3px")],
    );
    assert_css_properties(&css, "#diag .node .neo-node", &[("stroke", "#123456")]);
    for selector in [
        "#diag [data-look=\"neo\"].node rect",
        "#diag [data-look=\"neo\"].cluster rect",
        "#diag [data-look=\"neo\"].node polygon",
    ] {
        assert_css_properties(&css, selector, &[("stroke", gradient), ("filter", shadow)]);
    }
    assert_css_properties(
        &css,
        "#diag [data-look=\"neo\"].swimlane.cluster rect",
        &[("filter", "none")],
    );
    assert_css_properties(
        &css,
        "#diag [data-look=\"neo\"].node path",
        &[("stroke", gradient), ("stroke-width", "3px")],
    );
    assert_css_properties(
        &css,
        "#diag [data-look=\"neo\"].node .outer-path",
        &[("filter", shadow)],
    );
    assert_css_properties(
        &css,
        "#diag [data-look=\"neo\"].node .neo-line path",
        &[("stroke", "#123456"), ("filter", "none")],
    );
    assert_css_properties(
        &css,
        "#diag [data-look=\"neo\"].node circle",
        &[("stroke", gradient), ("filter", shadow)],
    );
    assert_css_properties(
        &css,
        "#diag [data-look=\"neo\"].node circle .state-start",
        &[("fill", "#000000")],
    );
    assert_css_properties(
        &css,
        "#diag [data-look=\"neo\"].icon-shape .icon",
        &[("fill", gradient), ("filter", shadow)],
    );
    assert_css_properties(
        &css,
        "#diag [data-look=\"neo\"].icon-shape .icon-neo path",
        &[("stroke", gradient), ("filter", shadow)],
    );
}

#[cfg(feature = "layout-cytoscape")]
#[test]
fn architecture_css_with_config_honors_font_and_theme_colors() {
    let cfg = serde_json::json!({
        "fontFamily": "\"courier new\", courier, monospace;",
        "fontSize": 18,
        "themeVariables": {
            "textColor": "#112233",
            "lineColor": "#445566",
            "primaryBorderColor": "#778899",
            "archEdgeColor": "#010203",
            "archEdgeArrowColor": "#040506",
            "archEdgeWidth": 7,
            "archGroupBorderColor": "#070809",
            "archGroupBorderWidth": "6px",
        }
    });

    let css = architecture_css_with_config("diag", &cfg);

    assert!(css.contains(
        r#"#diag{font-family:"courier new",courier,monospace;font-size:18px;fill:#112233;}"#
    ));
    assert!(css.contains(r#"#diag .edge{stroke-width:7;stroke:#010203;fill:none;}"#));
    assert!(css.contains(r#"#diag .arrow{fill:#040506;}"#));
    assert!(css.contains(
        r#"#diag .node-bkg{fill:none;stroke:#070809;stroke-width:6px;stroke-dasharray:8;}"#
    ));
    assert!(css.contains(r#"#diag :root{--mermaid-font-family:"courier new",courier,monospace;}"#));
}

#[cfg(feature = "layout-cytoscape")]
#[test]
fn architecture_css_prefers_theme_font_family_over_legacy_root() {
    let cfg = serde_json::json!({
        "fontFamily": "Courier, monospace",
        "themeVariables": {
            "fontFamily": "\"IBM Plex Sans\", Arial, sans-serif"
        }
    });

    let css = architecture_css_with_config("diag", &cfg);

    assert!(css.contains(
        r#"#diag{font-family:"IBM Plex Sans",Arial,sans-serif;font-size:16px;fill:#333;}"#
    ));
    assert!(css.contains(r#"#diag :root{--mermaid-font-family:Courier,monospace;}"#));
}

#[test]
fn sankey_css_honors_mermaid_11_15_theme_options() {
    let cfg = serde_json::json!({
        "fontFamily": "\"source sans\", arial, sans-serif",
        "themeVariables": {
            "fontFamily": "\"ibm plex sans\", arial, sans-serif",
            "textColor": "#123456",
            "mainBkg": "#abcdef",
        }
    });

    let css = sankey_css("sk", &cfg);

    assert!(css.contains(r#"#sk .label{font-family:"ibm plex sans",arial,sans-serif;}"#));
    assert!(css.contains(r#"#sk .node-labels{font-family:"ibm plex sans",arial,sans-serif;}"#));
    assert!(css.contains(r#"#sk .sankey-label-bg{stroke:#abcdef;"#));
    assert!(css.contains(r#"#sk .sankey-label-fg{fill:#123456;}"#));
    assert!(css.contains(r#"#sk :root{--mermaid-font-family:"source sans",arial,sans-serif;}"#));
}

#[test]
fn pie_css_honors_mermaid_11_16_theme_options() {
    let cfg = serde_json::json!({
        "themeVariables": {
            "fontFamily": "\"ibm plex sans\", arial, sans-serif",
            "textColor": "#111111",
            "taskTextDarkColor": "#222222",
            "pieStrokeColor": "#333333",
            "pieStrokeWidth": "4px",
            "pieOpacity": "0.9",
            "pieOuterStrokeColor": "#444444",
            "pieOuterStrokeWidth": "5px",
            "pieTitleTextSize": "26px",
            "pieTitleTextColor": "#555555",
            "pieSectionTextSize": "18px",
            "pieSectionTextColor": "#666666",
            "pieLegendTextSize": "19px",
            "pieLegendTextColor": "#777777"
        }
    });

    let plan = PieCss::new(&cfg);
    let mut css = String::new();
    plan.write_for_normalized_id(&mut css, "pie").unwrap();

    assert!(css.contains(r#"#pie .pieCircle{stroke:#333333;stroke-width:4px;opacity:0.9;}"#));
    assert!(css.contains(r#"#pie .pieCircle.highlighted{scale:1.05;opacity:1;}"#));
    assert!(css.contains(
        r#"#pie .pieCircle.highlightedOnHover:hover{transition-duration:250ms;scale:1.05;opacity:1;}"#
    ));
    assert!(css.contains(r#"#pie .pieOuterCircle{stroke:#444444;stroke-width:5px;fill:none;}"#));
    assert!(css.contains(r#"#pie .pieTitleText{text-anchor:middle;font-size:26px;fill:#555555;font-family:"ibm plex sans",arial,sans-serif;}"#));
    assert!(css.contains(
        r#"#pie .slice{font-family:"ibm plex sans",arial,sans-serif;fill:#666666;font-size:18px;}"#
    ));
    assert!(css.contains(r#"#pie .legend text{fill:#777777;font-family:"ibm plex sans",arial,sans-serif;font-size:19px;}"#));
}

#[test]
fn er_css_honors_mermaid_11_15_theme_options() {
    let cfg = serde_json::json!({
        "look": "neo",
        "themeVariables": {
            "fontFamily": "\"ibm plex sans\", arial, sans-serif",
            "fontSize": "18px",
            "textColor": "#101010",
            "lineColor": "#202020",
            "errorBkgColor": "#303030",
            "errorTextColor": "#404040",
            "mainBkg": "#505050",
            "nodeBorder": "#606060",
            "nodeTextColor": "#707070",
            "tertiaryColor": "hsl(80, 100%, 96%)",
            "edgeLabelBackground": "#b0c0d0",
            "strokeWidth": 3
        }
    });

    let css = er_css("er", &cfg).expect("valid ER theme colors");

    assert!(css.contains(
        r#"#er{font-family:"ibm plex sans",arial,sans-serif;font-size:18px;fill:#101010;}"#
    ));
    assert!(css.contains(
        r#"#er .error-icon{fill:#303030;}#er .error-text{fill:#404040;stroke:#404040;}"#
    ));
    assert!(css.contains(r#"#er .marker{fill:#202020;stroke:#202020;}"#));
    assert!(css.contains(r#"#er .entityBox{fill:#505050;stroke:#606060;}"#));
    assert!(css.contains(
        r#"#er .relationshipLabelBox{fill:hsl(80, 100%, 96%);opacity:0.7;background-color:hsl(80, 100%, 96%);}"#
    ));
    assert!(css.contains(r#"#er .labelBkg{background-color:rgba(248.2, 255, 234.6, 0.5);}"#));
    assert!(css.contains(r#"#er .edgeLabel{background-color:#b0c0d0;}#er .edgeLabel .label rect{fill:#b0c0d0;}#er .edgeLabel .label text{fill:#101010;}#er .edgeLabel .label{fill:#606060;font-size:14px;}"#));
    assert!(
        css.contains(r#"#er .label{font-family:"ibm plex sans",arial,sans-serif;color:#707070;}"#)
    );
    assert!(css.contains(r#"#er .node rect,#er .node circle,#er .node ellipse,#er .node polygon{fill:#505050;stroke:#606060;stroke-width:3;}"#));
    assert!(css.contains(r#"#er .relationshipLine{stroke:#202020;stroke-width:3;fill:none;}"#));
    assert!(
        css.contains(
            r#"#er .marker{fill:none!important;stroke:#202020!important;stroke-width:1;}"#
        )
    );
}

#[test]
fn er_css_rejects_unsupported_tertiary_color() {
    let error = er_css(
        "er",
        &serde_json::json!({
            "themeVariables": {
                "tertiaryColor": "not-a-css-color"
            }
        }),
    )
    .expect_err("unsupported khroma color must fail stylesheet generation");

    assert!(error.to_string().contains("not-a-css-color"));
}

#[test]
fn gantt_css_honors_mermaid_11_15_theme_options() {
    let cfg = serde_json::json!({
        "themeVariables": {
            "fontFamily": "\"ibm plex sans\", arial, sans-serif",
            "textColor": "#707070",
            "excludeBkgColor": "#101010",
            "sectionBkgColor": "#202020",
            "sectionBkgColor2": "#303030",
            "altSectionBkgColor": "#404040",
            "titleColor": "#505050",
            "gridColor": "#606060",
            "todayLineColor": "#808080",
            "taskTextDarkColor": "#909090",
            "taskTextClickableColor": "#a0a0a0",
            "taskTextColor": "#b0b0b0",
            "taskBkgColor": "#c0c0c0",
            "taskBorderColor": "#d0d0d0",
            "taskTextOutsideColor": "#e0e0e0",
            "activeTaskBkgColor": "#111111",
            "activeTaskBorderColor": "#222222",
            "doneTaskBorderColor": "#333333",
            "doneTaskBkgColor": "#444444",
            "critBorderColor": "#555555",
            "critBkgColor": "#666666",
            "vertLineColor": "#777777"
        }
    });

    let css = gantt_css("g", &cfg);

    assert!(css.contains(r#"#g .exclude-range{fill:#101010;}"#));
    assert!(css.contains(r#"#g .section0{fill:#202020;}"#));
    assert!(css.contains(r#"#g .section2{fill:#303030;}"#));
    assert!(css.contains(r#"#g .grid .tick{stroke:#606060;"#));
    assert!(
        css.contains(r#"#g .taskText0,#g .taskText1,#g .taskText2,#g .taskText3{fill:#b0b0b0;}"#)
    );
    assert!(
        css.contains(r#"#g .task0,#g .task1,#g .task2,#g .task3{fill:#c0c0c0;stroke:#d0d0d0;}"#)
    );
    assert!(
        css.contains(r#"#g .doneText0.taskTextOutsideLeft,#g .doneText0.taskTextOutsideRight"#)
    );
    assert!(css.contains(r#"fill:#e0e0e0!important;"#));
    assert!(css.contains(r#"#g .titleText{text-anchor:middle;font-size:18px;fill:#505050;font-family:"ibm plex sans",arial,sans-serif;}"#));
}

#[test]
fn treemap_css_honors_mermaid_11_15_style_options() {
    let cfg = serde_json::json!({
        "themeVariables": {
            "fontFamily": "\"ibm plex sans\", arial, sans-serif",
            "textColor": "#123456",
            "titleColor": "#654321"
        },
        "treemap": {
            "sectionStrokeColor": "#111111",
            "sectionStrokeWidth": 2,
            "sectionFillColor": "#222222",
            "leafStrokeColor": "#333333",
            "leafStrokeWidth": "3",
            "leafFillColor": "#444444",
            "labelColor": "#555555",
            "valueColor": "#666666",
            "titleColor": "#777777",
            "labelFontSize": "13px",
            "valueFontSize": "11px",
            "titleFontSize": "15px"
        }
    });

    let css = treemap_css("tm", &cfg).unwrap();

    assert!(css.contains("#tm .treemapNode.section{stroke:#111111;stroke-width:2;fill:#222222;}"));
    assert!(css.contains("#tm .treemapNode.leaf{stroke:#333333;stroke-width:3;fill:#444444;}"));
    assert!(css.contains("#tm .treemapLabel{fill:#555555;font-size:13px;}"));
    assert!(css.contains("#tm .treemapValue{fill:#666666;font-size:11px;}"));
    assert!(css.contains("#tm .treemapTitle{fill:#777777;font-size:15px;}"));
}

#[test]
fn requirement_css_honors_mermaid_11_15_theme_options() {
    let cfg = serde_json::json!({
        "look": "neo",
        "themeVariables": {
            "fontFamily": "\"ibm plex sans\", arial, sans-serif",
            "fontSize": "18px",
            "textColor": "#101010",
            "nodeTextColor": "#111111",
            "relationColor": "#222222",
            "lineColor": "#333333",
            "requirementBackground": "#444444",
            "requirementBorderColor": "#555555",
            "requirementBorderSize": 2,
            "requirementTextColor": "#666666",
            "relationLabelBackground": "#777777",
            "relationLabelColor": "#888888",
            "edgeLabelBackground": "#999999",
            "requirementEdgeLabelBackground": "#aaaaaa",
            "nodeBorder": "#bbbbbb",
            "strokeWidth": 3
        }
    });

    let css = requirement_css("req", &cfg);

    assert!(css.contains(r#"#req marker{fill:#222222;stroke:#222222;}"#));
    assert!(css.contains(r#"#req marker.cross{stroke:#333333;}"#));
    assert!(
        css.contains(
            r#"#req .reqBox{fill:#444444;fill-opacity:1.0;stroke:#555555;stroke-width:2;}"#
        )
    );
    assert!(css.contains(r#"#req .reqTitle,#req .reqLabel{fill:#666666;}"#));
    assert!(css.contains(r#"#req .reqLabelBox{fill:#777777;fill-opacity:1.0;}"#));
    assert!(css.contains(r#"#req .relationshipLine{stroke:#222222;stroke-width:3;}"#));
    assert!(css.contains(r#"#req .relationshipLabel{fill:#888888;}"#));
    assert!(css.contains(r#"#req .edgeLabel .label rect{fill:#999999;}"#));
    assert!(css.contains(r#"#req .labelBkg{background-color:#aaaaaa;}"#));
    assert!(css.contains(r#"#req [data-look="neo"].node path{stroke:#bbbbbb;stroke-width:3px;}"#));
}
