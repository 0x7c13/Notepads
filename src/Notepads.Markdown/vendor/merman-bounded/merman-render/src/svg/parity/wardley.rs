use super::*;
use crate::wardley::{
    WardleyAnnotationsBoxLayout, WardleyArrowLayout, WardleyCircleLayout, WardleyDiagramLayout,
    WardleyDominantBaseline, WardleyFontWeight, WardleyLineLayout, WardleyNodeShapeLayout,
    WardleySourceOverlayLayout, WardleyTextAnchor, WardleyTextLayout,
};
use merman_core::diagrams::wardley::WardleyDiagramRenderModel;

struct WardleyTheme {
    background_color: String,
    axis_color: String,
    axis_text_color: String,
    grid_color: String,
    component_fill: String,
    component_stroke: String,
    component_label_color: String,
    link_stroke: String,
    evolution_stroke: String,
    anchor_label_color: String,
    stage_divider_color: String,
    annotation_text_color: String,
    arrow_fill: String,
}

impl WardleyTheme {
    fn from_config(config: &merman_core::MermaidConfig) -> Self {
        let host = merman_core::presentation_defaults_active_internal(config);
        let raw = config.as_value();
        let nested = |key, fallback: &str| {
            config_string(raw, &["themeVariables", "wardley", key])
                .unwrap_or_else(|| fallback.to_string())
        };
        let nested_or_root = |key, root_key, fallback: &str| {
            config_string(raw, &["themeVariables", "wardley", key])
                .or_else(|| config_string(raw, &["themeVariables", root_key]))
                .unwrap_or_else(|| fallback.to_string())
        };

        let host_role = |key, root_key, fallback: &str| {
            config_string(raw, &["themeVariables", "wardley", key])
                .or_else(|| {
                    host.then(|| config.get_str(root_key).map(str::to_owned))
                        .flatten()
                })
                .unwrap_or_else(|| fallback.to_owned())
        };
        let annotation_text_color =
            if host && !merman_core::wardley_axis_text_authored_internal(config) {
                "#222".to_owned()
            } else {
                nested_or_root("axisTextColor", "primaryTextColor", "#222")
            };
        Self {
            background_color: nested_or_root("backgroundColor", "background", "#fff"),
            axis_color: host_role("axisColor", "themeVariables.lineColor", "#000"),
            axis_text_color: nested_or_root("axisTextColor", "primaryTextColor", "#222"),
            grid_color: nested("gridColor", "rgba(100, 100, 100, 0.2)"),
            component_fill: host_role("componentFill", "themeVariables.primaryColor", "#fff"),
            component_stroke: host_role(
                "componentStroke",
                "themeVariables.primaryBorderColor",
                "#000",
            ),
            component_label_color: nested_or_root(
                "componentLabelColor",
                "primaryTextColor",
                "#222",
            ),
            link_stroke: host_role("linkStroke", "themeVariables.lineColor", "#000"),
            evolution_stroke: nested("evolutionStroke", "#dc3545"),
            anchor_label_color: host_role(
                "anchorLabelColor",
                "themeVariables.primaryTextColor",
                "#000",
            ),
            stage_divider_color: host_role("stageDividerColor", "themeVariables.lineColor", "#000"),
            annotation_text_color,
            arrow_fill: if host {
                config
                    .get_str("themeVariables.background")
                    .unwrap_or("white")
                    .to_owned()
            } else {
                "white".to_owned()
            },
        }
    }
}

fn text_anchor(anchor: WardleyTextAnchor) -> &'static str {
    match anchor {
        WardleyTextAnchor::Start => "start",
        WardleyTextAnchor::Middle => "middle",
    }
}

fn font_weight(weight: WardleyFontWeight) -> &'static str {
    match weight {
        WardleyFontWeight::Normal => "normal",
        WardleyFontWeight::Bold => "bold",
    }
}

fn dominant_baseline(baseline: WardleyDominantBaseline) -> &'static str {
    match baseline {
        WardleyDominantBaseline::Auto => "auto",
        WardleyDominantBaseline::Middle => "middle",
        WardleyDominantBaseline::Central => "central",
    }
}

fn write_text(
    out: &mut String,
    text: &WardleyTextLayout,
    class: Option<&str>,
    fill: &str,
    include_font_weight: bool,
) {
    out.push_str("<text");
    if let Some(class) = class {
        let _ = write!(out, r#" class="{}""#, escape_attr_display(class));
    }
    let _ = write!(
        out,
        r#" x="{}" y="{}" fill="{}" font-size="{}""#,
        fmt(text.x),
        fmt(text.y),
        escape_attr_display(fill),
        fmt(text.font_size)
    );
    if include_font_weight {
        let _ = write!(out, r#" font-weight="{}""#, font_weight(text.font_weight));
    }
    let _ = write!(out, r#" text-anchor="{}""#, text_anchor(text.text_anchor));
    if let Some(baseline) = text.dominant_baseline {
        let _ = write!(
            out,
            r#" dominant-baseline="{}""#,
            dominant_baseline(baseline)
        );
    }
    if let Some(rotation) = text.rotation {
        let _ = write!(
            out,
            r#" transform="rotate({} {} {})""#,
            fmt(rotation.degrees),
            fmt(rotation.cx),
            fmt(rotation.cy)
        );
    }
    let _ = write!(out, ">{}</text>", escape_xml_display(&text.text));
}

fn write_line(
    out: &mut String,
    line: WardleyLineLayout,
    class: Option<&str>,
    stroke: &str,
    stroke_width: Option<f64>,
    dash: Option<&str>,
) {
    out.push_str("<line");
    if let Some(class) = class {
        let _ = write!(out, r#" class="{}""#, escape_attr_display(class));
    }
    let _ = write!(
        out,
        r#" x1="{}" x2="{}" y1="{}" y2="{}" stroke="{}""#,
        fmt(line.x1),
        fmt(line.x2),
        fmt(line.y1),
        fmt(line.y2),
        escape_attr_display(stroke)
    );
    if let Some(stroke_width) = stroke_width {
        let _ = write!(out, r#" stroke-width="{}""#, fmt(stroke_width));
    }
    if let Some(dash) = dash {
        let _ = write!(out, r#" stroke-dasharray="{}""#, escape_attr_display(dash));
    }
    out.push_str("/>");
}

fn write_circle(
    out: &mut String,
    class: Option<&str>,
    circle: WardleyCircleLayout,
    fill: &str,
    stroke: &str,
    stroke_width: f64,
) {
    out.push_str("<circle");
    if let Some(class) = class {
        let _ = write!(out, r#" class="{}""#, escape_attr_display(class));
    }
    let _ = write!(
        out,
        r#" cx="{}" cy="{}" r="{}" fill="{}" stroke="{}" stroke-width="{}"/>"#,
        fmt(circle.center.x),
        fmt(circle.center.y),
        fmt(circle.radius),
        escape_attr_display(fill),
        escape_attr_display(stroke),
        fmt(stroke_width)
    );
}

fn write_accessibility(
    out: &mut String,
    diagram_id: SvgDiagramId<'_>,
    acc_title: Option<&str>,
    acc_descr: Option<&str>,
) {
    if let Some(title) = acc_title {
        let _ = write!(
            out,
            r#"<title id="chart-title-{}">{}</title>"#,
            diagram_id,
            escape_xml_display(title)
        );
    }
    if let Some(description) = acc_descr {
        let _ = write!(
            out,
            r#"<desc id="chart-desc-{}">{}</desc>"#,
            diagram_id,
            escape_xml_display(description)
        );
    }
}

fn write_axes(out: &mut String, layout: &WardleyDiagramLayout, theme: &WardleyTheme) {
    out.push_str(r#"<g class="wardley-axes">"#);
    write_line(
        out,
        layout.axes.x_axis,
        None,
        &theme.axis_color,
        Some(1.0),
        None,
    );
    write_line(
        out,
        layout.axes.y_axis,
        None,
        &theme.axis_color,
        Some(1.0),
        None,
    );
    write_text(
        out,
        &layout.axes.x_label,
        Some("wardley-axis-label wardley-axis-label-x"),
        &theme.axis_text_color,
        true,
    );
    write_text(
        out,
        &layout.axes.y_label,
        Some("wardley-axis-label wardley-axis-label-y"),
        &theme.axis_text_color,
        true,
    );
    out.push_str("</g>");
}

fn write_stages(out: &mut String, layout: &WardleyDiagramLayout, theme: &WardleyTheme) {
    if layout.stages.is_empty() {
        return;
    }
    out.push_str(r#"<g class="wardley-stages">"#);
    for stage in &layout.stages {
        if let Some(divider) = stage.divider {
            write_line(
                out,
                divider,
                None,
                &theme.stage_divider_color,
                Some(1.0),
                Some("5 5"),
            );
            let insert_at = out.len() - 2;
            out.insert_str(insert_at, r#" opacity="0.8""#);
        }
        write_text(
            out,
            &stage.label,
            Some("wardley-stage-label"),
            &theme.axis_text_color,
            false,
        );
    }
    out.push_str("</g>");
}

fn write_grid(out: &mut String, layout: &WardleyDiagramLayout, theme: &WardleyTheme) {
    if layout.grid.is_empty() {
        return;
    }
    out.push_str(r#"<g class="wardley-grid">"#);
    for grid in &layout.grid {
        write_line(
            out,
            grid.vertical,
            None,
            &theme.grid_color,
            None,
            Some("2 6"),
        );
        write_line(
            out,
            grid.horizontal,
            None,
            &theme.grid_color,
            None,
            Some("2 6"),
        );
    }
    out.push_str("</g>");
}

fn write_pipelines(
    out: &mut String,
    layout: &WardleyDiagramLayout,
    model: &WardleyDiagramRenderModel,
    theme: &WardleyTheme,
) {
    if model.pipelines.is_empty() {
        return;
    }

    out.push_str(r#"<g class="wardley-pipelines">"#);
    for pipeline in &layout.pipeline_boxes {
        let rect = pipeline.rect;
        let _ = write!(
            out,
            r#"<rect class="wardley-pipeline-box" x="{}" y="{}" width="{}" height="{}" fill="none" stroke="{}" stroke-width="1.5" rx="{}" ry="{}"/>"#,
            fmt(rect.x),
            fmt(rect.y),
            fmt(rect.width),
            fmt(rect.height),
            escape_attr_display(&theme.axis_color),
            fmt(rect.corner_radius),
            fmt(rect.corner_radius)
        );
    }
    out.push_str("</g>");

    out.push_str(r#"<g class="wardley-pipeline-links">"#);
    for link in &layout.pipeline_links {
        write_line(
            out,
            link.line,
            Some("wardley-pipeline-evolution-link"),
            &theme.link_stroke,
            Some(1.0),
            Some("4 4"),
        );
    }
    out.push_str("</g>");
}

fn write_links(
    out: &mut String,
    layout: &WardleyDiagramLayout,
    theme: &WardleyTheme,
    diagram_id: SvgDiagramId<'_>,
    options: &SvgExecution<'_>,
) -> Result<()> {
    out.push_str(r#"<g class="wardley-links">"#);
    for link in &layout.links {
        options.checkpoint_emit()?;
        let class = if link.dashed {
            "wardley-link wardley-link--dashed"
        } else {
            "wardley-link"
        };
        write_line(
            out,
            link.line,
            Some(class),
            &theme.link_stroke,
            Some(1.0),
            link.dashed.then_some("6 6"),
        );
        let insert_at = out.len() - 2;
        let mut marker_attrs = String::new();
        if link.markers.end {
            let _ = write!(
                marker_attrs,
                r#" marker-end="url(#link-arrow-end-{diagram_id})""#,
            );
        }
        if link.markers.start {
            let _ = write!(
                marker_attrs,
                r#" marker-start="url(#link-arrow-start-{diagram_id})""#,
            );
        }
        out.insert_str(insert_at, &marker_attrs);
        options.checkpoint_emit()?;
    }
    for link in &layout.links {
        if let Some(label) = &link.label {
            write_text(
                out,
                label,
                Some("wardley-link-label"),
                &theme.axis_text_color,
                false,
            );
        }
    }
    out.push_str("</g>");
    Ok(())
}

fn write_trends(
    out: &mut String,
    layout: &WardleyDiagramLayout,
    theme: &WardleyTheme,
    diagram_id: SvgDiagramId<'_>,
    options: &SvgExecution<'_>,
) -> Result<()> {
    out.push_str(r#"<g class="wardley-trends">"#);
    for trend in &layout.trends {
        options.checkpoint_emit()?;
        write_line(
            out,
            trend.line,
            Some("wardley-trend"),
            &theme.evolution_stroke,
            Some(1.0),
            Some("4 4"),
        );
        let insert_at = out.len() - 2;
        out.insert_str(
            insert_at,
            &format!(r#" marker-end="url(#arrow-{diagram_id})""#),
        );
        options.checkpoint_emit()?;
    }
    out.push_str("</g>");
    Ok(())
}

fn write_source_overlay(
    out: &mut String,
    overlay: &WardleySourceOverlayLayout,
    theme: &WardleyTheme,
) {
    match overlay {
        WardleySourceOverlayLayout::Build { circle } => write_circle(
            out,
            Some("wardley-build-overlay"),
            *circle,
            "#eee",
            "#000",
            1.0,
        ),
        WardleySourceOverlayLayout::Buy { circle } => write_circle(
            out,
            Some("wardley-buy-overlay"),
            *circle,
            "#ccc",
            &theme.component_stroke,
            1.0,
        ),
        WardleySourceOverlayLayout::Outsource { circle } => write_circle(
            out,
            Some("wardley-outsource-overlay"),
            *circle,
            "#666",
            &theme.component_stroke,
            1.0,
        ),
        WardleySourceOverlayLayout::Market {
            outer_circle,
            connectors,
            dots,
        } => {
            write_circle(
                out,
                Some("wardley-market-overlay"),
                *outer_circle,
                "white",
                &theme.component_stroke,
                1.0,
            );
            for connector in connectors {
                write_line(
                    out,
                    *connector,
                    Some("wardley-market-line"),
                    &theme.component_stroke,
                    Some(1.0),
                    None,
                );
            }
            for dot in dots {
                write_circle(
                    out,
                    Some("wardley-market-dot"),
                    *dot,
                    "white",
                    &theme.component_stroke,
                    2.0,
                );
            }
        }
    }
}

fn write_nodes(out: &mut String, layout: &WardleyDiagramLayout, theme: &WardleyTheme) {
    out.push_str(r#"<g class="wardley-nodes">"#);
    for node in &layout.nodes {
        out.push_str(r#"<g class="wardley-node"#);
        if let Some(class_name) = node.class_name.as_deref().filter(|class| !class.is_empty()) {
            let _ = write!(out, " wardley-node--{}", escape_attr_display(class_name));
        }
        out.push_str(r#"">"#);

        if let Some(overlay) = &node.source_overlay {
            write_source_overlay(out, overlay, theme);
        }
        match &node.shape {
            WardleyNodeShapeLayout::Circle { circle } => write_circle(
                out,
                None,
                *circle,
                &theme.component_fill,
                &theme.component_stroke,
                1.0,
            ),
            WardleyNodeShapeLayout::PipelineSquare { rect } => {
                let _ = write!(
                    out,
                    r#"<rect x="{}" y="{}" width="{}" height="{}" fill="{}" stroke="{}" stroke-width="1"/>"#,
                    fmt(rect.x),
                    fmt(rect.y),
                    fmt(rect.width),
                    fmt(rect.height),
                    escape_attr_display(&theme.component_fill),
                    escape_attr_display(&theme.component_stroke)
                );
            }
            WardleyNodeShapeLayout::Anchor | WardleyNodeShapeLayout::None => {}
        }
        if let Some(inertia) = node.inertia {
            write_line(
                out,
                inertia,
                Some("wardley-inertia"),
                &theme.component_stroke,
                Some(6.0),
                None,
            );
        }
        let label_fill = match node.class_name.as_deref() {
            Some("evolved") => &theme.evolution_stroke,
            Some("anchor") => &theme.anchor_label_color,
            _ => &theme.component_label_color,
        };
        write_text(
            out,
            &node.label_layout,
            Some("wardley-node-label"),
            label_fill,
            true,
        );
        out.push_str("</g>");
    }
    out.push_str("</g>");
}

fn write_annotations_box(
    out: &mut String,
    annotations_box: &WardleyAnnotationsBoxLayout,
    theme: &WardleyTheme,
) {
    out.push_str(r#"<g class="wardley-annotations-box">"#);
    if let Some(rect) = annotations_box.rect {
        let _ = write!(
            out,
            r#"<rect x="{}" y="{}" width="{}" height="{}" fill="white" stroke="{}" stroke-width="1.5" rx="{}" ry="{}"/>"#,
            fmt(rect.x),
            fmt(rect.y),
            fmt(rect.width),
            fmt(rect.height),
            escape_attr_display(&theme.axis_color),
            fmt(rect.corner_radius),
            fmt(rect.corner_radius)
        );
    }
    for line in &annotations_box.lines {
        write_text(out, line, None, &theme.annotation_text_color, false);
    }
    out.push_str("</g>");
}

fn write_annotations(out: &mut String, layout: &WardleyDiagramLayout, theme: &WardleyTheme) {
    if layout.annotations.is_empty() {
        return;
    }
    out.push_str(r#"<g class="wardley-annotations">"#);
    for annotation in &layout.annotations {
        for segment in &annotation.segments {
            write_line(
                out,
                *segment,
                Some("wardley-annotation-line"),
                &theme.axis_color,
                Some(1.5),
                Some("4 4"),
            );
        }
        for point in &annotation.points {
            out.push_str(r#"<g class="wardley-annotation">"#);
            write_circle(
                out,
                None,
                WardleyCircleLayout {
                    center: point.center,
                    radius: point.radius,
                },
                "white",
                &theme.axis_color,
                1.5,
            );
            write_text(out, &point.label, None, &theme.annotation_text_color, true);
            out.push_str("</g>");
        }
    }
    if let Some(annotations_box) = &layout.annotations_box {
        write_annotations_box(out, annotations_box, theme);
    }
    out.push_str("</g>");
}

fn write_notes(out: &mut String, layout: &WardleyDiagramLayout, theme: &WardleyTheme) {
    if layout.notes.is_empty() {
        return;
    }
    out.push_str(r#"<g class="wardley-notes">"#);
    for note in &layout.notes {
        write_text(out, &note.text, None, &theme.axis_text_color, true);
    }
    out.push_str("</g>");
}

fn write_arrow(out: &mut String, arrow: &WardleyArrowLayout, theme: &WardleyTheme) {
    out.push_str(r#"<path d="M "#);
    for (index, point) in arrow.path.iter().enumerate() {
        if index > 0 {
            out.push_str(" L ");
        }
        let _ = write!(out, "{} {}", fmt(point.x), fmt(point.y));
    }
    let _ = write!(
        out,
        r#" Z" fill="{}" stroke="{}" stroke-width="1"/>"#,
        escape_attr_display(&theme.arrow_fill),
        escape_attr_display(&theme.component_stroke)
    );
    write_text(out, &arrow.label, None, &theme.axis_text_color, true);
}

fn write_arrows(
    out: &mut String,
    class: &str,
    arrows: &[WardleyArrowLayout],
    theme: &WardleyTheme,
) {
    if arrows.is_empty() {
        return;
    }
    let _ = write!(out, r#"<g class="{}">"#, escape_attr_display(class));
    for arrow in arrows {
        write_arrow(out, arrow, theme);
    }
    out.push_str("</g>");
}

fn write_defs(out: &mut String, diagram_id: SvgDiagramId<'_>, theme: &WardleyTheme) {
    let _ = write!(
        out,
        r#"<defs><marker id="arrow-{diagram_id}" viewBox="0 0 10 10" refX="9" refY="5" markerWidth="6" markerHeight="6" orient="auto-start-reverse"><path d="M 0 0 L 10 5 L 0 10 z" fill="{}" stroke="none"/></marker><marker id="link-arrow-end-{diagram_id}" viewBox="0 0 10 10" refX="9" refY="5" markerWidth="5" markerHeight="5" orient="auto"><path d="M 0 0 L 10 5 L 0 10 z" fill="{}" stroke="none"/></marker><marker id="link-arrow-start-{diagram_id}" viewBox="0 0 10 10" refX="1" refY="5" markerWidth="5" markerHeight="5" orient="auto"><path d="M 10 0 L 0 5 L 10 10 z" fill="{}" stroke="none"/></marker></defs>"#,
        escape_attr_display(&theme.evolution_stroke),
        escape_attr_display(&theme.link_stroke),
        escape_attr_display(&theme.link_stroke)
    );
}

pub(crate) fn render_wardley_diagram_svg_model(
    layout: &WardleyDiagramLayout,
    model: &WardleyDiagramRenderModel,
    effective_config: &merman_core::MermaidConfig,
    _diagram_title: Option<&str>,
    options: &SvgExecution<'_>,
) -> Result<root_svg::RootedSvg> {
    let diagram_id = options.diagram_id_or("wardley");
    let acc_title = model.acc_title.as_deref().filter(|value| !value.is_empty());
    let acc_descr = model.acc_descr.as_deref().filter(|value| !value.is_empty());
    let aria_labelledby = acc_title.map(|_| format!("chart-title-{diagram_id}"));
    let aria_describedby = acc_descr.map(|_| format!("chart-desc-{diagram_id}"));
    let theme = WardleyTheme::from_config(effective_config);

    let root_bounds = root_svg::DiagramBounds::from_view_box(0.0, 0.0, layout.width, layout.height);
    let root_spec = root_svg::RootViewportSpec::mermaid(root_bounds, layout.use_max_width);
    let mut root_chrome = root_svg::RootChrome::new(diagram_id, "wardley");
    root_chrome.aria_labelledby = aria_labelledby.as_deref();
    root_chrome.aria_describedby = aria_describedby.as_deref();
    root_chrome.dom.trailing_newline = false;

    let mut out = String::new();
    let root_document =
        root_svg::RootViewportContext::new(crate::family::RenderFamilyKind::Wardley, diagram_id)
            .write_open(&mut out, root_spec, root_chrome)?;
    write_accessibility(&mut out, diagram_id, acc_title, acc_descr);
    options.checkpoint_emit()?;

    out.push_str(r#"<g class="wardley-map">"#);
    let _ = write!(
        out,
        r#"<rect class="wardley-background" width="{}" height="{}" fill="{}"/>"#,
        fmt(layout.width),
        fmt(layout.height),
        escape_attr_display(&theme.background_color)
    );
    if let Some(title) = &layout.title {
        write_text(
            &mut out,
            title,
            Some("wardley-title"),
            &theme.axis_text_color,
            true,
        );
    }
    write_axes(&mut out, layout, &theme);
    write_stages(&mut out, layout, &theme);
    write_grid(&mut out, layout, &theme);
    write_pipelines(&mut out, layout, model, &theme);
    write_links(&mut out, layout, &theme, diagram_id, options)?;
    write_trends(&mut out, layout, &theme, diagram_id, options)?;
    write_nodes(&mut out, layout, &theme);
    write_annotations(&mut out, layout, &theme);
    write_notes(&mut out, layout, &theme);
    write_arrows(
        &mut out,
        "wardley-accelerators",
        &layout.accelerators,
        &theme,
    );
    write_arrows(
        &mut out,
        "wardley-deaccelerators",
        &layout.deaccelerators,
        &theme,
    );
    out.push_str("</g>");
    write_defs(&mut out, diagram_id, &theme);
    out.push_str("</svg>");
    options.checkpoint_emit()?;
    root_document.complete(out)
}
