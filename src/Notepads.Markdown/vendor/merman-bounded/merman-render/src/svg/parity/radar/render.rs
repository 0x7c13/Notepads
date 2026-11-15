use super::super::theme::RadarTheme;
use super::super::*;
use merman_core::diagrams::radar::RadarDiagramRenderModel;

// Radar diagram SVG renderer implementation (split from parity.rs).

fn radar_css<I>(diagram_id: I, theme: &RadarTheme, effective_config: &serde_json::Value) -> String
where
    I: Copy + std::fmt::Display,
{
    // Keep `:root` last (matches upstream Mermaid radar SVG baselines).
    let mut out = String::new();
    let _ = write!(
        &mut out,
        r#"#{}{{font-family:{};font-size:{};fill:{};}}"#,
        diagram_id, theme.font_family_css, theme.base_font_size_css, theme.text_color
    );
    out.push_str(
        r#"@keyframes edge-animation-frame{from{stroke-dashoffset:0;}}@keyframes dash{to{stroke-dashoffset:0;}}"#,
    );
    let _ = write!(
        &mut out,
        r#"#{} .edge-animation-slow{{stroke-dasharray:9,5!important;stroke-dashoffset:900;animation:dash 50s linear infinite;stroke-linecap:round;}}#{} .edge-animation-fast{{stroke-dasharray:9,5!important;stroke-dashoffset:900;animation:dash 20s linear infinite;stroke-linecap:round;}}"#,
        diagram_id, diagram_id
    );
    let _ = write!(
        &mut out,
        r#"#{} .error-icon{{fill:{};}}#{} .error-text{{fill:{};stroke:{};}}"#,
        diagram_id,
        theme.error_bkg_color,
        diagram_id,
        theme.error_text_color,
        theme.error_text_color
    );
    let _ = write!(
        &mut out,
        r#"#{} .edge-thickness-normal{{stroke-width:1px;}}#{} .edge-thickness-thick{{stroke-width:3.5px;}}#{} .edge-pattern-solid{{stroke-dasharray:0;}}#{} .edge-thickness-invisible{{stroke-width:0;fill:none;}}#{} .edge-pattern-dashed{{stroke-dasharray:3;}}#{} .edge-pattern-dotted{{stroke-dasharray:2;}}"#,
        diagram_id, diagram_id, diagram_id, diagram_id, diagram_id, diagram_id
    );
    let _ = write!(
        &mut out,
        r#"#{} .marker{{fill:{};stroke:{};}}#{} .marker.cross{{stroke:{};}}"#,
        diagram_id, theme.line_color, theme.line_color, diagram_id, theme.line_color
    );
    let _ = write!(
        &mut out,
        r#"#{} svg{{font-family:{};font-size:{};}}#{} p{{margin:0;}}"#,
        diagram_id, theme.font_family_css, theme.base_font_size_css, diagram_id
    );

    let _ = write!(
        &mut out,
        r#"#{} .radarTitle{{font-size:{};color:{};dominant-baseline:hanging;text-anchor:middle;}}"#,
        diagram_id, theme.title_font_size_css, theme.title_color
    );
    let _ = write!(
        &mut out,
        r#"#{} .radarAxisLine{{stroke:{};stroke-width:{};}}"#,
        diagram_id,
        theme.axis_color,
        fmt(theme.axis_stroke_width)
    );
    let _ = write!(
        &mut out,
        r#"#{} .radarAxisLabel{{font-size:{}px;color:{};}}"#,
        diagram_id,
        fmt(theme.axis_label_font_size),
        theme.axis_color
    );
    let _ = write!(
        &mut out,
        r#"#{} .radarGraticule{{fill:{};fill-opacity:{};stroke:{};stroke-width:{};}}"#,
        diagram_id,
        theme.graticule_color,
        fmt(theme.graticule_opacity),
        theme.graticule_color,
        fmt(theme.graticule_stroke_width)
    );
    let _ = write!(
        &mut out,
        r#"#{} .radarLegendText{{text-anchor:start;font-size:{}px;dominant-baseline:hanging;}}"#,
        diagram_id,
        fmt(theme.legend_font_size)
    );

    for (i, c) in theme.series_colors.iter().enumerate() {
        let _ = write!(
            &mut out,
            r#"#{} .radarCurve-{}{{color:{};fill:{};fill-opacity:{};stroke:{};stroke-width:{};}}#{} .radarLegendBox-{}{{fill:{};fill-opacity:{};stroke:{};}}"#,
            diagram_id,
            i,
            c,
            c,
            fmt(theme.curve_opacity),
            c,
            fmt(theme.curve_stroke_width),
            diagram_id,
            i,
            c,
            fmt(theme.curve_opacity),
            c
        );
    }

    let _ = crate::svg::parity::css::write_mermaid_base_css_root_rule_to(
        &mut out,
        diagram_id,
        &crate::config::config_root_font_family_css(effective_config),
    );

    out
}

pub(crate) fn render_radar_diagram_svg_model(
    layout: &RadarDiagramLayout,
    model: &RadarDiagramRenderModel,
    effective_config: &serde_json::Value,
    diagram_title: Option<&str>,
    options: &SvgExecution<'_>,
) -> Result<root_svg::RootedSvg> {
    let diagram_id = options.diagram_id_or("radar");

    let has_acc_title = model
        .acc_title
        .as_deref()
        .is_some_and(|s| !s.trim().is_empty());
    let has_acc_descr = model
        .acc_descr
        .as_deref()
        .is_some_and(|s| !s.trim().is_empty());

    let render_settings = crate::radar::RadarConfigView::new(effective_config).render_settings();

    let aria_describedby = has_acc_descr.then(|| format!("chart-desc-{diagram_id}"));
    let aria_labelledby = has_acc_title.then(|| format!("chart-title-{diagram_id}"));
    let root_extra_attrs: [(&str, &str); 1] = [("overflow", "visible")];

    let mut out = String::new();
    let mut root_chrome = root_svg::RootChrome::new(diagram_id, "radar");
    root_chrome.extra_attrs = &root_extra_attrs;
    root_chrome.aria_labelledby = aria_labelledby.as_deref();
    root_chrome.aria_describedby = aria_describedby.as_deref();
    root_chrome.dom = root_svg::RootDomProfile {
        fixed_height_placement: root_svg::SvgRootFixedHeightPlacement::AfterXmlns,
        fixed_style_placement: root_svg::RootStylePlacement::Tail,
        trailing_newline: false,
        ..root_svg::RootDomProfile::default()
    };
    let root_document =
        root_svg::RootViewportContext::new(crate::family::RenderFamilyKind::Radar, diagram_id)
            .write_open(
                &mut out,
                root_svg::RootViewportSpec::mermaid(
                    root_svg::DiagramBounds::from_view_box(
                        0.0,
                        0.0,
                        layout.svg_width,
                        layout.svg_height,
                    ),
                    render_settings.use_max_width,
                )
                .with_max_width(root_svg::RootMaxWidth::CssSixSignificant(layout.svg_width)),
                root_chrome,
            )?;
    options.checkpoint_emit()?;

    if has_acc_title {
        let _ = write!(
            &mut out,
            r#"<title id="chart-title-{id}">{text}</title>"#,
            id = diagram_id,
            text = escape_xml(model.acc_title.as_deref().unwrap_or_default())
        );
    }
    if has_acc_descr {
        let _ = write!(
            &mut out,
            r#"<desc id="chart-desc-{id}">{text}</desc>"#,
            id = diagram_id,
            text = escape_xml(model.acc_descr.as_deref().unwrap_or_default())
        );
    }

    let theme = PresentationTheme::new(effective_config).radar();
    let css = radar_css(diagram_id, &theme, effective_config);
    let _ = write!(&mut out, "<style>{}</style>", css);
    out.push_str("<g/>");
    options.checkpoint_emit()?;

    let _ = write!(
        &mut out,
        r#"<g transform="translate({x}, {y})">"#,
        x = fmt_display(layout.center_x),
        y = fmt_display(layout.center_y)
    );

    for g in &layout.graticules {
        if g.kind == "polygon" {
            if g.points.is_empty() {
                out.push_str(r#"<polygon points="" class="radarGraticule"/>"#);
            } else {
                let points = fmt_points(&g.points);
                let _ = write!(
                    &mut out,
                    r#"<polygon points="{points}" class="radarGraticule"/>"#,
                    points = escape_xml(&points)
                );
            }
        } else if let Some(r) = g.r {
            let _ = write!(
                &mut out,
                r#"<circle r="{r}" class="radarGraticule"/>"#,
                r = fmt_display(r)
            );
        }
    }

    for a in &layout.axes {
        let cos_a = a.angle.cos();
        let sin_a = a.angle.sin();
        let text_anchor = if cos_a > 0.01 {
            "start"
        } else if cos_a < -0.01 {
            "end"
        } else {
            "middle"
        };
        let dominant_baseline = if sin_a > 0.01 {
            "hanging"
        } else if sin_a < -0.01 {
            "auto"
        } else {
            "central"
        };
        let label_padding = 4.0;
        let _ = write!(
            &mut out,
            r#"<line x1="0" y1="0" x2="{x2}" y2="{y2}" class="radarAxisLine"/>"#,
            x2 = fmt_display(a.line_x2),
            y2 = fmt_display(a.line_y2)
        );
        let _ = write!(
            &mut out,
            r#"<text x="{x}" y="{y}" text-anchor="{text_anchor}" dominant-baseline="{dominant_baseline}" class="radarAxisLabel">{label}</text>"#,
            x = fmt_display(a.label_x + label_padding * cos_a),
            y = fmt_display(a.label_y + label_padding * sin_a),
            label = escape_xml(&a.label)
        );
    }

    let polygon_curves = layout
        .graticules
        .first()
        .is_some_and(|g| g.kind.trim() == "polygon");
    for c in &layout.curves {
        if polygon_curves && !c.points.is_empty() {
            let points = fmt_points(&c.points);
            let _ = write!(
                &mut out,
                r#"<polygon points="{points}" class="radarCurve-{idx}"/>"#,
                points = escape_xml(&points),
                idx = c.class_index
            );
        } else {
            let _ = write!(
                &mut out,
                r#"<path d="{d}" class="radarCurve-{idx}"/>"#,
                d = escape_xml(&c.path_d),
                idx = c.class_index
            );
        }
    }

    for item in &layout.legend_items {
        let _ = write!(
            &mut out,
            r#"<g transform="translate({x}, {y})">"#,
            x = fmt_display(item.x),
            y = fmt_display(item.y)
        );
        let _ = write!(
            &mut out,
            r#"<rect width="{size}" height="{size}" class="radarLegendBox-{idx}"/>"#,
            size = fmt_display(12.0),
            idx = item.class_index
        );
        let label = model
            .curves
            .get(item.class_index as usize)
            .map(|c| c.label.as_str())
            .unwrap_or("");
        let _ = write!(
            &mut out,
            r#"<text x="{x}" y="{y}" class="radarLegendText">{text}</text>"#,
            x = fmt_display(16.0),
            y = fmt_display(0.0),
            text = escape_xml(label)
        );
        out.push_str("</g>");
    }

    let title = model
        .title
        .as_deref()
        .map(str::trim)
        .filter(|title| !title.is_empty())
        .or_else(|| {
            diagram_title
                .map(str::trim)
                .filter(|title| !title.is_empty())
        });
    match title {
        Some(t) => {
            let _ = write!(
                &mut out,
                r#"<text class="radarTitle" x="0" y="{y}">{text}</text>"#,
                y = fmt(layout.title_y),
                text = escape_xml(t)
            );
        }
        None => {
            let _ = write!(
                &mut out,
                r#"<text class="radarTitle" x="0" y="{y}"/>"#,
                y = fmt(layout.title_y)
            );
        }
    }

    out.push_str("</g></svg>\n");
    options.checkpoint_emit()?;
    root_document.complete(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::RadarAxisLayout;

    #[test]
    fn radar_css_honors_top_level_style_overrides() {
        let cfg = serde_json::json!({
            "themeVariables": {
                "fontFamily": "\"ibm plex sans\", arial, sans-serif",
                "fontSize": "18px",
                "textColor": "#101010",
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

        let theme = PresentationTheme::new(&cfg).radar();
        let css = radar_css("radar", &theme, &cfg);

        assert!(css.contains(r#"#radar .radarTitle{font-size:18px;color:#202020;"#));
        assert!(css.contains(r#"#radar .radarAxisLine{stroke:#606060;stroke-width:4;}"#));
        assert!(css.contains(r#"#radar .radarAxisLabel{font-size:14px;color:#606060;}"#));
        assert!(css.contains(
            r#"#radar .radarGraticule{fill:#707070;fill-opacity:0.8;stroke:#707070;stroke-width:5;}"#
        ));
        assert!(css.contains(
            r#"#radar .radarLegendText{text-anchor:start;font-size:16px;dominant-baseline:hanging;}"#
        ));
        assert!(css.contains(
            r#"#radar .radarCurve-0{color:#303030;fill:#303030;fill-opacity:0.9;stroke:#303030;stroke-width:6;}"#
        ));
    }

    #[test]
    fn radar_css_uses_scoped_theme_variables_when_top_level_is_missing() {
        let cfg = serde_json::json!({
            "themeVariables": {
                "fontFamily": "\"ibm plex sans\", arial, sans-serif",
                "fontSize": "18px",
                "textColor": "#101010",
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
            }
        });

        let theme = PresentationTheme::new(&cfg).radar();
        let css = radar_css("radar", &theme, &cfg);

        assert!(css.contains(r#"#radar .radarAxisLine{stroke:#404040;stroke-width:2;}"#));
        assert!(css.contains(r#"#radar .radarAxisLabel{font-size:12px;color:#404040;}"#));
        assert!(css.contains(
            r#"#radar .radarGraticule{fill:#505050;fill-opacity:0.3;stroke:#505050;stroke-width:1;}"#
        ));
        assert!(css.contains(
            r#"#radar .radarLegendText{text-anchor:start;font-size:12px;dominant-baseline:hanging;}"#
        ));
        assert!(css.contains(
            r#"#radar .radarCurve-0{color:#303030;fill:#303030;fill-opacity:0.5;stroke:#303030;stroke-width:2;}"#
        ));
    }

    #[test]
    fn radar_root_uses_responsive_width_and_max_width_style() {
        let layout = RadarDiagramLayout {
            bounds: None,
            svg_width: 700.0,
            svg_height: 700.0,
            center_x: 350.0,
            center_y: 350.0,
            radius: 300.0,
            axis_label_factor: 1.05,
            title_y: -350.0,
            axes: Vec::new(),
            graticules: Vec::new(),
            curves: Vec::new(),
            legend_items: Vec::new(),
        };
        let options = SvgRenderOptions {
            diagram_id: Some("radarRoot".to_string()),
            ..SvgRenderOptions::default()
        };

        let svg = with_test_svg_execution(&options, |options| {
            render_radar_diagram_svg_model(
                &layout,
                &RadarDiagramRenderModel::default(),
                &serde_json::json!({}),
                None,
                options,
            )
        })
        .unwrap();
        let root_open = svg.split_once('>').expect("root svg open tag").0;

        assert!(root_open.contains(r#"width="100%""#), "{root_open}");
        assert!(
            root_open.contains(r#"style="max-width: 700px;" viewBox="0 0 700 700""#),
            "{root_open}"
        );
        assert!(
            !root_open.contains(r#"height=""#),
            "radar root should not emit fixed height: {root_open}"
        );
    }

    #[test]
    fn radar_root_honors_disabled_max_width() {
        let layout = RadarDiagramLayout {
            bounds: None,
            svg_width: 700.0,
            svg_height: 700.0,
            center_x: 350.0,
            center_y: 350.0,
            radius: 300.0,
            axis_label_factor: 1.05,
            title_y: -350.0,
            axes: Vec::new(),
            graticules: Vec::new(),
            curves: Vec::new(),
            legend_items: Vec::new(),
        };
        let options = SvgRenderOptions {
            diagram_id: Some("radarFixed".to_string()),
            ..SvgRenderOptions::default()
        };

        let svg = with_test_svg_execution(&options, |options| {
            render_radar_diagram_svg_model(
                &layout,
                &RadarDiagramRenderModel::default(),
                &serde_json::json!({"radar": {"useMaxWidth": false}}),
                None,
                options,
            )
        })
        .unwrap();
        let root_open = svg.split_once('>').expect("root svg open tag").0;

        assert!(root_open.contains(r#"width="700""#), "{root_open}");
        assert!(root_open.contains(r#"height="700""#), "{root_open}");
        assert!(
            root_open.contains(r#"viewBox="0 0 700 700""#),
            "{root_open}"
        );
        assert!(!root_open.contains("style="), "{root_open}");
        assert!(!root_open.contains("max-width"), "{root_open}");
    }

    #[test]
    fn radar_root_and_axis_labels_match_mermaid_11_16() {
        let axes = [
            ("Top", -std::f64::consts::FRAC_PI_2, 0.0, -100.0),
            ("Right", 0.0, 100.0, 0.0),
            ("Bottom", std::f64::consts::FRAC_PI_2, 0.0, 100.0),
            ("Left", std::f64::consts::PI, -100.0, 0.0),
        ]
        .into_iter()
        .map(|(label, angle, label_x, label_y)| RadarAxisLayout {
            label: label.to_string(),
            angle,
            line_x2: label_x,
            line_y2: label_y,
            label_x,
            label_y,
        })
        .collect();
        let layout = RadarDiagramLayout {
            bounds: None,
            svg_width: 240.0,
            svg_height: 240.0,
            center_x: 120.0,
            center_y: 120.0,
            radius: 100.0,
            axis_label_factor: 1.0,
            title_y: -120.0,
            axes,
            graticules: Vec::new(),
            curves: Vec::new(),
            legend_items: Vec::new(),
        };

        let svg = with_test_svg_execution(&SvgRenderOptions::default(), |options| {
            render_radar_diagram_svg_model(
                &layout,
                &RadarDiagramRenderModel::default(),
                &serde_json::json!({}),
                None,
                options,
            )
        })
        .unwrap();
        let root_open = svg.split_once('>').expect("root svg open tag").0;

        assert!(root_open.contains(r#"overflow="visible""#), "{root_open}");
        assert!(
            svg.contains(
                r#"<text x="0" y="-104" text-anchor="middle" dominant-baseline="auto" class="radarAxisLabel">Top</text>"#
            ),
            "{svg}"
        );
        assert!(
            svg.contains(
                r#"<text x="104" y="0" text-anchor="start" dominant-baseline="central" class="radarAxisLabel">Right</text>"#
            ),
            "{svg}"
        );
        assert!(
            svg.contains(
                r#"<text x="0" y="104" text-anchor="middle" dominant-baseline="hanging" class="radarAxisLabel">Bottom</text>"#
            ),
            "{svg}"
        );
        assert!(
            svg.contains(
                r#"<text x="-104" y="0" text-anchor="end" dominant-baseline="central" class="radarAxisLabel">Left</text>"#
            ),
            "{svg}"
        );
        assert!(
            svg.contains(".radarAxisLabel{font-size:12px;color:#333333;}"),
            "radar CSS must leave per-axis alignment attributes in control: {svg}"
        );
    }
}
