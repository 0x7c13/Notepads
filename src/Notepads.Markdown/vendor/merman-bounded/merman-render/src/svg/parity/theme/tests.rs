use super::*;
use serde_json::json;

#[test]
fn presentation_theme_node_diagram_uses_shared_fallbacks() {
    let cfg = json!({});
    let theme = PresentationTheme::new(&cfg);
    let node = theme.node_diagram();

    assert_eq!(node.common.text_color, "#333");
    assert_eq!(node.common.line_color, "#333333");
    assert_eq!(node.node_text_color, "#333");
    assert_eq!(node.title_color, "#333");
    assert_eq!(node.main_bkg, "#ECECFF");
    assert_eq!(node.node_border, "#9370DB");
    assert_eq!(node.arrowhead_color, "#333333");
    assert_eq!(node.stroke_width, "1");
}

#[test]
fn presentation_theme_sequence_neo_uses_drop_shadow_for_label_box_filter() {
    let cfg = json!({
        "look": "neo",
        "themeVariables": {
            "dropShadow": "drop-shadow(1px 2px 3px rgba(0,0,0,.4))"
        }
    });
    let theme = PresentationTheme::new(&cfg);

    let sequence = theme.sequence_diagram();
    assert_eq!(
        sequence.label_box_filter,
        "drop-shadow(1px 2px 3px rgba(0,0,0,.4))"
    );
}

#[test]
fn presentation_theme_xychart_resolves_chart_roles() {
    let cfg = json!({
        "theme": "neo",
        "themeVariables": {
            "primaryColor": "#123456",
            "primaryTextColor": "#f8fafc",
            "background": "#010203",
            "xyChart": {
                "backgroundColor": "#0f172a",
                "titleColor": "#f43f5e",
                "xAxisLabelColor": "#22c55e",
                "plotColorPalette": "#001122, #334455"
            }
        }
    });

    let xychart = PresentationTheme::new(&cfg).xychart();

    assert_eq!(xychart.background_color, "#0f172a");
    assert_eq!(xychart.title_color, "#f43f5e");
    assert_eq!(xychart.legend_text_color, "#f8fafc");
    assert_eq!(xychart.x_axis_title_color, "#f8fafc");
    assert_eq!(xychart.x_axis_label_color, "#22c55e");
    assert_eq!(xychart.y_axis_line_color, "#f8fafc");
    assert_eq!(
        xychart.plot_color_palette,
        vec!["#001122".to_string(), "#334455".to_string()]
    );
}

#[test]
fn presentation_theme_quadrantchart_resolves_chart_roles() {
    let cfg = json!({
        "theme": "redux-dark",
        "themeVariables": {
            "primaryColor": "#123456",
            "primaryTextColor": "#f8fafc",
            "primaryBorderColor": "#445566",
            "quadrant1Fill": "#010203",
            "quadrant2Fill": "#020304",
            "quadrant3Fill": "#030405",
            "quadrant4Fill": "#040506",
            "quadrantPointFill": "#facc15",
            "quadrantPointTextFill": "#111827",
            "quadrantXAxisTextFill": "#22c55e",
            "quadrantYAxisTextFill": "#38bdf8",
            "quadrantTitleFill": "#f43f5e",
            "quadrantInternalBorderStrokeFill": "#aabbcc",
            "quadrantExternalBorderStrokeFill": "#ddeeff"
        }
    });

    let quadrant = PresentationTheme::new(&cfg).quadrantchart();

    assert_eq!(quadrant.quadrant1_fill, "#010203");
    assert_eq!(quadrant.quadrant2_fill, "#020304");
    assert_eq!(quadrant.quadrant1_text_fill, "#f8fafc");
    assert_eq!(quadrant.quadrant_point_fill, "#facc15");
    assert_eq!(quadrant.quadrant_point_text_fill, "#111827");
    assert_eq!(quadrant.quadrant_x_axis_text_fill, "#22c55e");
    assert_eq!(quadrant.quadrant_y_axis_text_fill, "#38bdf8");
    assert_eq!(quadrant.quadrant_title_fill, "#f43f5e");
    assert_eq!(quadrant.quadrant_internal_border_stroke_fill, "#aabbcc");
    assert_eq!(quadrant.quadrant_external_border_stroke_fill, "#ddeeff");
}

#[test]
fn presentation_theme_tree_view_resolves_tree_view_roles() {
    let cfg = json!({
        "themeVariables": {
            "treeView": {
                "labelFontSize": "20px",
                "labelColor": "#FF0000",
                "lineColor": "#00FF00",
                "iconColor": "#111111",
                "descriptionColor": "#222222",
                "highlightBg": "rgba(1, 2, 3, 0.4)",
                "highlightStroke": "#333333"
            }
        }
    });

    let tree_view = PresentationTheme::new(&cfg).tree_view();

    assert_eq!(tree_view.label_font_size, 20.0);
    assert_eq!(tree_view.label_font_size_css, "20px");
    assert_eq!(tree_view.label_color, "#FF0000");
    assert_eq!(tree_view.line_color, "#00FF00");
    assert_eq!(tree_view.icon_color, "#111111");
    assert_eq!(tree_view.description_color, "#222222");
    assert_eq!(tree_view.highlight_bg, "rgba(1, 2, 3, 0.4)");
    assert_eq!(tree_view.highlight_stroke, "#333333");
}

#[test]
fn presentation_theme_tree_view_uses_default_tree_view_roles() {
    let cfg = json!({});

    let tree_view = PresentationTheme::new(&cfg).tree_view();

    assert_eq!(tree_view.label_font_size, 16.0);
    assert_eq!(tree_view.label_font_size_css, "16px");
    assert_eq!(tree_view.label_color, "black");
    assert_eq!(tree_view.line_color, "black");
    assert_eq!(tree_view.icon_color, "#546e7a");
    assert_eq!(tree_view.description_color, "#6a9955");
    assert_eq!(tree_view.highlight_bg, "rgba(255, 193, 7, 0.15)");
    assert_eq!(tree_view.highlight_stroke, "#ffc107");
}

#[test]
fn presentation_theme_treemap_resolves_top_level_treemap_roles() {
    let cfg = json!({
        "theme": "custom",
        "themeVariables": {
            "textColor": "#101010",
            "titleColor": "#202020",
            "labelTextColor": "rgb(10, 20, 30)",
            "cScale0": "#010203",
            "cScalePeer0": "#040506",
            "cScaleLabel0": "#070809"
        },
        "treemap": {
            "titleColor": "#777777",
            "labelColor": "#555555",
            "valueColor": "#666666",
            "sectionStrokeColor": "#111111",
            "sectionStrokeWidth": 2,
            "sectionFillColor": "#222222",
            "leafStrokeColor": "#333333",
            "leafStrokeWidth": "3px",
            "leafFillColor": "#444444",
            "labelFontSize": "13px",
            "valueFontSize": "11px",
            "titleFontSize": "15px"
        }
    });

    let treemap = PresentationTheme::new(&cfg).treemap().unwrap();

    assert_eq!(treemap.title_color, "#777777");
    assert_eq!(treemap.label_color, "#555555");
    assert_eq!(treemap.value_color, "#666666");
    assert_eq!(treemap.section_stroke_color, "#111111");
    assert_eq!(treemap.section_stroke_width, "2");
    assert_eq!(treemap.section_fill_color, "#222222");
    assert_eq!(treemap.leaf_stroke_color, "#333333");
    assert_eq!(treemap.leaf_stroke_width, "3px");
    assert_eq!(treemap.leaf_fill_color, "#444444");
    assert_eq!(treemap.label_font_size, "13px");
    assert_eq!(treemap.value_font_size, "11px");
    assert_eq!(treemap.title_font_size, "15px");
    assert_eq!(treemap.color_scale[0], "#010203");
    assert_eq!(treemap.color_scale_peer[0], "#040506");
    assert_eq!(treemap.color_scale_label[0], "#070809");
}

#[test]
fn presentation_theme_treemap_uses_text_and_title_fallbacks() {
    let cfg = json!({
        "themeVariables": {
            "textColor": "#101010",
            "titleColor": "#202020"
        }
    });

    let treemap = PresentationTheme::new(&cfg).treemap().unwrap();

    assert_eq!(treemap.title_color, "#202020");
    assert_eq!(treemap.label_color, "#101010");
    assert_eq!(treemap.value_color, "#101010");
}

#[test]
fn presentation_theme_treemap_uses_default_scales_and_label_inversion() {
    let cfg = json!({
        "themeVariables": {
            "labelTextColor": "rgb(10, 20, 30)"
        }
    });

    let treemap = PresentationTheme::new(&cfg).treemap().unwrap();

    assert_eq!(treemap.color_scale[0], "hsl(240, 100%, 76.2745098039%)");
    assert_eq!(
        treemap.color_scale_peer[0],
        "hsl(240, 100%, 61.2745098039%)"
    );
    assert_eq!(treemap.color_scale_label[0], "#f5ebe1");
    assert_eq!(treemap.color_scale_label[1], "rgb(10, 20, 30)");
    assert_eq!(treemap.color_scale_label[3], "#f5ebe1");
    assert_eq!(
        treemap.readable_leaf_label_fill("transparent", "", "#ffffff".to_string()),
        "#333"
    );
}

#[test]
fn presentation_theme_gantt_resolves_gantt_roles() {
    let cfg = json!({
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

    let gantt = PresentationTheme::new(&cfg).gantt();

    assert_eq!(gantt.font_family, r#""ibm plex sans",arial,sans-serif"#);
    assert_eq!(gantt.text_color, "#707070");
    assert_eq!(gantt.exclude_bkg_color, "#101010");
    assert_eq!(gantt.section_bkg_color, "#202020");
    assert_eq!(gantt.section_bkg_color2, "#303030");
    assert_eq!(gantt.alt_section_bkg_color, "#404040");
    assert_eq!(gantt.title_color, "#505050");
    assert_eq!(gantt.title_text_color, "#505050");
    assert_eq!(gantt.grid_color, "#606060");
    assert_eq!(gantt.today_line_color, "#808080");
    assert_eq!(gantt.task_text_dark_color, "#909090");
    assert_eq!(gantt.task_text_clickable_color, "#a0a0a0");
    assert_eq!(gantt.task_text_color, "#b0b0b0");
    assert_eq!(gantt.task_bkg_color, "#c0c0c0");
    assert_eq!(gantt.task_border_color, "#d0d0d0");
    assert_eq!(gantt.task_text_outside_color, "#e0e0e0");
    assert_eq!(gantt.active_task_bkg_color, "#111111");
    assert_eq!(gantt.active_task_border_color, "#222222");
    assert_eq!(gantt.done_task_border_color, "#333333");
    assert_eq!(gantt.done_task_bkg_color, "#444444");
    assert_eq!(gantt.crit_border_color, "#555555");
    assert_eq!(gantt.crit_bkg_color, "#666666");
    assert_eq!(gantt.vert_line_color, "#777777");
}

#[test]
fn presentation_theme_gantt_uses_text_color_for_empty_title_color() {
    let cfg = json!({
        "themeVariables": {
            "textColor": "#707070",
            "titleColor": "   "
        }
    });

    let gantt = PresentationTheme::new(&cfg).gantt();

    assert_eq!(gantt.title_color, "   ");
    assert_eq!(gantt.title_text_color, "#707070");
}

#[test]
fn presentation_theme_kanban_resolves_theme_roles() {
    let cfg = json!({
        "themeVariables": {
            "background": "#0f172a",
            "nodeBorder": "#38bdf8",
            "textColor": "#f8fafc",
            "git0": "#22c55e",
            "gitBranchLabel0": "#020617",
            "cScale0": "hsl(160, 80%, 40%)",
            "cScaleLabel0": "#f8fafc",
            "cScaleInv0": "#111827"
        }
    });

    let kanban = PresentationTheme::new(&cfg).kanban().unwrap();

    assert_eq!(kanban.text_color, "#f8fafc");
    assert_eq!(kanban.background, "#0f172a");
    assert_eq!(kanban.node_border, "#38bdf8");
    assert_eq!(kanban.root_fill, "#22c55e");
    assert_eq!(kanban.root_label, "#020617");
    assert_eq!(kanban.sections[0].c_scale, "hsl(160, 80%, 40%)");
    assert_eq!(kanban.sections[0].section_fill, "hsl(160, 80%, 50%)");
    assert_eq!(kanban.sections[0].c_scale_label, "#f8fafc");
    assert_eq!(kanban.sections[0].c_scale_inv, "#111827");
}

#[test]
fn presentation_theme_kanban_dark_mode_adjusts_section_fill_down() {
    let cfg = json!({
        "darkMode": true
    });

    let kanban = PresentationTheme::new(&cfg).kanban().unwrap();

    assert_eq!(kanban.sections[0].c_scale, "hsl(240, 100%, 76.2745098039%)");
    assert_eq!(
        kanban.sections[0].section_fill,
        "hsl(240, 100%, 66.2745098039%)"
    );
    assert_eq!(kanban.sections[0].c_scale_label, "#ffffff");
    assert_eq!(
        kanban.sections[0].c_scale_inv,
        "hsl(60, 100%, 86.2745098039%)"
    );
}

#[test]
fn presentation_theme_kanban_uses_khroma_for_every_supported_color_syntax() {
    let cases = [
        ("#ff0000", "hsl(0, 100%, 60%)"),
        ("rebeccapurple", "hsl(270, 50%, 50%)"),
        (
            "rgb(18 52 86 / .5)",
            "hsla(210, 65.3846153846%, 30.3921568627%, 0.5)",
        ),
    ];

    for (input, expected) in cases {
        let cfg = json!({ "themeVariables": { "cScale0": input } });
        let kanban = PresentationTheme::new(&cfg).kanban().unwrap();
        assert_eq!(kanban.sections[0].section_fill, expected, "{input}");
    }
}

#[test]
fn presentation_theme_kanban_rejects_invalid_derived_colors() {
    let cfg = json!({ "themeVariables": { "cScale0": "var(--runtime-color)" } });
    let error = PresentationTheme::new(&cfg).kanban().unwrap_err();
    assert!(matches!(error, crate::Error::Color(_)));
}

#[test]
fn presentation_theme_timeline_resolves_timeline_roles() {
    let cfg = json!({
        "theme": "redux-color",
        "themeVariables": {
            "THEME_COLOR_LIMIT": 2,
            "strokeWidth": 5,
            "fontWeight": 600,
            "mainBkg": "#111827",
            "nodeBorder": "#38bdf8",
            "dropShadow": "drop-shadow(1px 2px 2px rgba(0,0,0,.4))",
            "tertiaryColor": "#334155",
            "clusterBorder": "#f97316",
            "git0": "#22c55e",
            "gitBranchLabel0": "#020617",
            "borderColorArray": ["#ef4444", "#f59e0b"],
            "cScale0": "#ef4444",
            "cScaleLabel0": "#e879f9",
            "cScaleInv0": "#334155",
            "cScale1": "#172554",
            "cScaleLabel1": "#f8fafc",
            "cScaleInv1": "#475569"
        }
    });

    let timeline = PresentationTheme::new(&cfg).timeline();

    assert!(timeline.is_redux_theme);
    assert!(!timeline.is_dark_theme);
    assert!(timeline.is_color_theme);
    assert_eq!(timeline.stroke_width, "5");
    assert_eq!(timeline.font_weight, "600");
    assert_eq!(timeline.main_bkg, "#111827");
    assert_eq!(timeline.node_border, "#38bdf8");
    assert_eq!(timeline.disabled_fill, "#334155");
    assert_eq!(timeline.disabled_text_fill, "#f97316");
    assert_eq!(timeline.root_fill, "#22c55e");
    assert_eq!(timeline.root_label, "#020617");
    assert_eq!(timeline.border_colors, vec!["#ef4444", "#f59e0b"]);
    assert_eq!(timeline.sections.len(), 2);
    assert_eq!(timeline.sections[0].c_scale, "#ef4444");
    assert_eq!(timeline.sections[0].c_scale_label, "#e879f9");
    assert_eq!(timeline.sections[0].c_scale_inv, "#334155");
    assert_eq!(timeline.sections[1].c_scale, "#172554");
    assert_eq!(timeline.sections[1].c_scale_label, "#f8fafc");
    assert_eq!(timeline.sections[1].c_scale_inv, "#475569");
}

#[test]
fn presentation_theme_timeline_uses_default_timeline_roles() {
    let cfg = json!({});

    let timeline = PresentationTheme::new(&cfg).timeline();

    assert!(!timeline.is_redux_theme);
    assert!(!timeline.is_dark_theme);
    assert!(!timeline.is_color_theme);
    assert_eq!(timeline.stroke_width, "1");
    assert_eq!(timeline.font_weight, "normal");
    assert_eq!(timeline.main_bkg, "#ECECFF");
    assert_eq!(timeline.node_border, "#9370DB");
    assert_eq!(timeline.disabled_fill, "lightgray");
    assert_eq!(timeline.disabled_text_fill, "#efefef");
    assert_eq!(timeline.root_fill, "hsl(240, 100%, 46.2745098039%)");
    assert_eq!(timeline.root_label, "#ffffff");
    assert!(timeline.border_colors.is_empty());
    assert_eq!(timeline.sections.len(), 12);
    assert_eq!(
        timeline.sections[0].c_scale,
        "hsl(240, 100%, 76.2745098039%)"
    );
    assert_eq!(timeline.sections[0].c_scale_label, "#ffffff");
    assert_eq!(
        timeline.sections[0].c_scale_inv,
        "hsl(60, 100%, 86.2745098039%)"
    );
    assert_eq!(timeline.sections[1].c_scale_label, "black");
}

#[test]
fn presentation_theme_eventmodeling_resolves_eventmodeling_roles() {
    let cfg = json!({
        "fontFamily": "Inter, sans-serif",
        "themeVariables": {
            "textColor": "#111111",
            "emUiFill": "#fefefe",
            "emUiStroke": "#222222",
            "emCommandFill": "#DDEEFF",
            "emCommandStroke": "#336699",
            "emSwimlaneBackgroundOdd": "#fafafa",
            "emSwimlaneBackgroundStroke": "#efefef",
            "emRelationStroke": "#135790",
            "emArrowhead": "#02468a"
        }
    });

    let eventmodeling = PresentationTheme::new(&cfg).eventmodeling();

    assert_eq!(eventmodeling.font_family_css, "Inter,sans-serif");
    assert_eq!(eventmodeling.text_color, "#111111");
    assert_eq!(eventmodeling.ui_fill, "#fefefe");
    assert_eq!(eventmodeling.ui_stroke, "#222222");
    assert_eq!(eventmodeling.command_fill, "#DDEEFF");
    assert_eq!(eventmodeling.command_stroke, "#336699");
    assert_eq!(eventmodeling.swimlane_background_fill, "#fafafa");
    assert_eq!(eventmodeling.swimlane_background_stroke, "#efefef");
    assert_eq!(eventmodeling.relation_stroke, "#135790");
    assert_eq!(eventmodeling.arrowhead_fill, "#02468a");
}

#[test]
fn presentation_theme_eventmodeling_uses_default_eventmodeling_roles() {
    let cfg = json!({});

    let eventmodeling = PresentationTheme::new(&cfg).eventmodeling();

    assert_eq!(
        eventmodeling.font_family_css,
        "\"trebuchet ms\",verdana,arial,sans-serif"
    );
    assert_eq!(eventmodeling.text_color, "#333");
    assert_eq!(eventmodeling.ui_fill, "white");
    assert_eq!(eventmodeling.ui_stroke, "#dbdada");
    assert_eq!(eventmodeling.swimlane_background_fill, "rgb(250,250,250)");
    assert_eq!(eventmodeling.swimlane_background_stroke, "rgb(240,240,240)");
    assert_eq!(eventmodeling.relation_stroke, "#000");
    assert_eq!(eventmodeling.arrowhead_fill, "#000000");
}

#[test]
fn presentation_theme_ishikawa_resolves_ishikawa_roles() {
    let cfg = json!({
        "fontFamily": "Inter, sans-serif",
        "themeVariables": {
            "lineColor": "#008800",
            "mainBkg": "#FFFFFF",
            "textColor": "#111111",
            "fontFamily": "Ignored, sans-serif"
        }
    });

    let ishikawa = PresentationTheme::new(&cfg).ishikawa();

    assert_eq!(ishikawa.line_color, "#008800");
    assert_eq!(ishikawa.main_bkg, "#FFFFFF");
    assert_eq!(ishikawa.text_color, "#111111");
    assert_eq!(ishikawa.font_family, "Inter, sans-serif");
}

#[test]
fn presentation_theme_ishikawa_uses_default_ishikawa_roles() {
    let cfg = json!({});

    let ishikawa = PresentationTheme::new(&cfg).ishikawa();

    assert_eq!(ishikawa.line_color, "#333");
    assert_eq!(ishikawa.main_bkg, "#fff");
    assert_eq!(ishikawa.text_color, "#333");
    assert_eq!(
        ishikawa.font_family,
        "trebuchet ms, verdana, arial, sans-serif"
    );
}

#[test]
fn presentation_theme_venn_resolves_venn_roles() {
    let cfg = json!({
        "theme": "dark",
        "fontFamily": "Inter, sans-serif",
        "themeVariables": {
            "background": "#111111",
            "vennTitleTextColor": "#f43f5e",
            "vennSetTextColor": "#22c55e",
            "venn1": "#123456",
            "venn2": "#abcdef",
            "primaryColor": "#987654",
            "primaryTextColor": "#eeeeee",
            "textColor": "#dddddd"
        }
    });

    let venn = PresentationTheme::new(&cfg).venn().unwrap();

    assert_eq!(venn.font_family_css, "Inter,sans-serif");
    assert_eq!(venn.title_color, "#f43f5e");
    assert_eq!(venn.set_text_color, "#22c55e");
    assert_eq!(venn.circle_colors, vec!["#123456", "#abcdef"]);
    assert_eq!(venn.primary_color, "#987654");
    assert!(venn.is_dark_theme);
    assert_eq!(
        venn.circle_text_color("#123456").unwrap(),
        "hsl(210, 65.3846153846%, 50.3921568627%)"
    );
}

#[test]
fn presentation_theme_venn_uses_default_venn_roles() {
    let cfg = json!({});

    let venn = PresentationTheme::new(&cfg).venn().unwrap();

    assert_eq!(
        venn.font_family_css,
        "\"trebuchet ms\",verdana,arial,sans-serif"
    );
    assert_eq!(venn.title_color, "#333");
    assert_eq!(venn.set_text_color, "#333");
    assert!(venn.circle_colors.is_empty());
    assert_eq!(venn.primary_color, "#ECECFF");
    assert!(!venn.is_dark_theme);
    assert_eq!(
        venn.circle_text_color("#abc").unwrap(),
        "hsl(210, 25%, 43.3333333333%)"
    );
    assert!(matches!(
        venn.circle_text_color("not-a-color"),
        Err(crate::Error::Color(_))
    ));
}

#[test]
fn presentation_theme_venn_uses_background_color_not_theme_name_for_contrast() {
    let dark_named_background = json!({
        "theme": "base",
        "themeVariables": { "background": "rebeccapurple" }
    });
    let theme = PresentationTheme::new(&dark_named_background)
        .venn()
        .unwrap();
    assert!(theme.is_dark_theme);
    assert_eq!(
        theme.circle_text_color("rebeccapurple").unwrap(),
        "hsl(270, 50%, 70%)"
    );

    let invalid_background = json!({
        "theme": "dark",
        "themeVariables": { "background": "not-a-color" }
    });
    assert!(matches!(
        PresentationTheme::new(&invalid_background).venn(),
        Err(crate::Error::Color(_))
    ));
}

#[test]
fn presentation_theme_journey_resolves_style_roles() {
    let cfg = json!({
        "themeVariables": {
            "fontFamily": "\"ibm plex sans\", arial, sans-serif",
            "textColor": "#101010",
            "lineColor": "#202020",
            "faceColor": "#303030",
            "mainBkg": "#404040",
            "nodeBorder": "#505050",
            "arrowheadColor": "#606060",
            "edgeLabelBackground": "#707070",
            "titleColor": "#808080",
            "tertiaryColor": "#909090",
            "border2": "#a0a0a0",
            "fillType0": "#b0b0b0",
            "fillType1": "#c0c0c0",
            "actor0": "#d0d0d0",
            "actor1": "#e0e0e0"
        }
    });

    let journey = PresentationTheme::new(&cfg).journey();

    assert_eq!(
        journey.font_family_css,
        "\"ibm plex sans\",arial,sans-serif"
    );
    assert_eq!(journey.text_color, "#101010");
    assert_eq!(journey.line_color, "#202020");
    assert_eq!(journey.face_color, "#303030");
    assert_eq!(journey.main_bkg, "#404040");
    assert_eq!(journey.node_border, "#505050");
    assert_eq!(journey.arrowhead_color, "#606060");
    assert_eq!(journey.edge_label_background, "#707070");
    assert_eq!(journey.title_color, "#808080");
    assert_eq!(journey.tertiary_color, "#909090");
    assert_eq!(journey.border2, "#a0a0a0");
    assert_eq!(journey.fill_types[0], "#b0b0b0");
    assert_eq!(journey.fill_types[1], "#c0c0c0");
    assert_eq!(journey.fill_types[7], "hsl(188, 100%, 93.5294117647%)");
    assert_eq!(journey.actor_colors[0].as_deref(), Some("#d0d0d0"));
    assert_eq!(journey.actor_colors[1].as_deref(), Some("#e0e0e0"));
    assert_eq!(journey.actor_colors[5], None);
}

#[test]
fn presentation_theme_journey_uses_default_style_roles() {
    let cfg = json!({});

    let journey = PresentationTheme::new(&cfg).journey();

    assert_eq!(
        journey.font_family_css,
        "\"trebuchet ms\",verdana,arial,sans-serif"
    );
    assert_eq!(journey.text_color, "#333");
    assert_eq!(journey.line_color, "#333333");
    assert_eq!(journey.face_color, "#FFF8DC");
    assert_eq!(journey.main_bkg, "#ECECFF");
    assert_eq!(journey.node_border, "#9370DB");
    assert_eq!(journey.arrowhead_color, "#333333");
    assert_eq!(journey.edge_label_background, "rgba(232,232,232, 0.8)");
    assert_eq!(journey.title_color, "#333");
    assert_eq!(journey.tertiary_color, "hsl(80, 100%, 96.2745098039%)");
    assert_eq!(journey.border2, "#aaaa33");
    assert_eq!(journey.fill_types.len(), 8);
    assert_eq!(journey.fill_types[0], "#ECECFF");
    assert_eq!(journey.fill_types[1], "#ffffde");
    assert_eq!(journey.fill_types[2], "hsl(304, 100%, 96.2745098039%)");
    assert_eq!(
        journey.actor_colors,
        vec![None, None, None, None, None, None]
    );
}

#[test]
fn presentation_theme_radar_resolves_style_roles() {
    let cfg = json!({
        "fontFamily": "Ignored, sans-serif",
        "themeVariables": {
            "fontFamily": "\"ibm plex sans\", arial, sans-serif",
            "fontSize": 18,
            "textColor": "#101010",
            "lineColor": "#111111",
            "errorBkgColor": "#121212",
            "errorTextColor": "#131313",
            "titleColor": "#202020",
            "cScale0": "#303030",
            "radar": {
                "axisColor": "#404040",
                "axisStrokeWidth": 2,
                "axisLabelFontSize": 12,
                "graticuleColor": "#505050",
                "graticuleOpacity": 0.3,
                "graticuleStrokeWidth": 1,
                "legendFontSize": 12,
                "curveOpacity": 0.5,
                "curveStrokeWidth": 2
            }
        },
        "radar": {
            "axisColor": "#606060",
            "axisStrokeWidth": 4,
            "axisLabelFontSize": 14,
            "graticuleColor": "#707070",
            "graticuleOpacity": 0.8,
            "graticuleStrokeWidth": 5,
            "legendFontSize": 16,
            "curveOpacity": 0.9,
            "curveStrokeWidth": 6
        }
    });

    let radar = PresentationTheme::new(&cfg).radar();

    assert_eq!(radar.font_family_css, "\"ibm plex sans\",arial,sans-serif");
    assert_eq!(radar.base_font_size_css, "18");
    assert_eq!(radar.title_font_size_css, "18");
    assert_eq!(radar.text_color, "#101010");
    assert_eq!(radar.line_color, "#111111");
    assert_eq!(radar.error_bkg_color, "#121212");
    assert_eq!(radar.error_text_color, "#131313");
    assert_eq!(radar.title_color, "#202020");
    assert_eq!(radar.axis_color, "#606060");
    assert_eq!(radar.axis_stroke_width, 4.0);
    assert_eq!(radar.axis_label_font_size, 14.0);
    assert_eq!(radar.graticule_color, "#707070");
    assert_eq!(radar.graticule_opacity, 0.8);
    assert_eq!(radar.graticule_stroke_width, 5.0);
    assert_eq!(radar.legend_font_size, 16.0);
    assert_eq!(radar.curve_opacity, 0.9);
    assert_eq!(radar.curve_stroke_width, 6.0);
    assert_eq!(radar.series_colors[0], "#303030");
    assert_eq!(radar.series_colors[11], "hsl(210, 100%, 76.2745098039%)");
}

#[test]
fn presentation_theme_radar_uses_default_style_roles() {
    let cfg = json!({});

    let radar = PresentationTheme::new(&cfg).radar();

    assert_eq!(
        radar.font_family_css,
        "\"trebuchet ms\",verdana,arial,sans-serif"
    );
    assert_eq!(radar.base_font_size_css, "16px");
    assert_eq!(radar.title_font_size_css, "16px");
    assert_eq!(radar.text_color, "#333");
    assert_eq!(radar.line_color, "#333333");
    assert_eq!(radar.error_bkg_color, "#552222");
    assert_eq!(radar.error_text_color, "#552222");
    assert_eq!(radar.title_color, "#333");
    assert_eq!(radar.axis_color, "#333333");
    assert_eq!(radar.axis_stroke_width, 2.0);
    assert_eq!(radar.axis_label_font_size, 12.0);
    assert_eq!(radar.graticule_color, "#DEDEDE");
    assert_eq!(radar.graticule_opacity, 0.3);
    assert_eq!(radar.graticule_stroke_width, 1.0);
    assert_eq!(radar.legend_font_size, 12.0);
    assert_eq!(radar.curve_opacity, 0.5);
    assert_eq!(radar.curve_stroke_width, 2.0);
    assert_eq!(radar.series_colors.len(), 12);
    assert_eq!(radar.series_colors[0], "hsl(240, 100%, 76.2745098039%)");
}
