use super::super::*;

// Mindmap diagram SVG renderer implementation (split from parity.rs).

#[derive(Debug, Clone, Copy)]
enum MindmapPathNumberFormat {
    D3Path,
    JsNumber,
}

fn mindmap_path_number(v: f64, number_format: MindmapPathNumberFormat) -> String {
    match number_format {
        MindmapPathNumberFormat::D3Path => fmt_path(v),
        MindmapPathNumberFormat::JsNumber => fmt_string(v),
    }
}

fn mindmap_cloud_path_d(w: f64, h: f64, number_format: MindmapPathNumberFormat) -> String {
    let r1 = 0.15 * w;
    let r2 = 0.25 * w;
    let r3 = 0.35 * w;
    let r4 = 0.2 * w;
    let n = |v| mindmap_path_number(v, number_format);

    format!(
        "M0 0 a{r1},{r1} 0 0,1 {w25},{wn10} a{r3},{r3} 1 0,1 {w40},{wn10} a{r2},{r2} 1 0,1 {w35},{w20} a{r1},{r1} 1 0,1 {w15},{h35} a{r4},{r4} 1 0,1 {wn15},{h65} a{r2},{r1} 1 0,1 {wn25},{w15} a{r3},{r3} 1 0,1 {wn50},0 a{r1},{r1} 1 0,1 {wn25},{wn15} a{r1},{r1} 1 0,1 {wn10},{hn35} a{r4},{r4} 1 0,1 {w10},{hn65} H0 V0 Z",
        r1 = n(r1),
        r2 = n(r2),
        r3 = n(r3),
        r4 = n(r4),
        w25 = n(w * 0.25),
        w40 = n(w * 0.4),
        w35 = n(w * 0.35),
        w20 = n(w * 0.2),
        w15 = n(w * 0.15),
        w10 = n(w * 0.1),
        wn10 = n(-w * 0.1),
        wn15 = n(-w * 0.15),
        wn25 = n(-w * 0.25),
        wn50 = n(-w * 0.5),
        h35 = n(h * 0.35),
        h65 = n(h * 0.65),
        hn35 = n(-h * 0.35),
        hn65 = n(-h * 0.65),
    )
}

pub(crate) fn mindmap_cloud_rendered_bbox_size_px(w: f64, h: f64) -> Option<(f64, f64)> {
    let d = mindmap_cloud_path_d(w, h, MindmapPathNumberFormat::JsNumber);
    let pb = svg_path_bounds_from_d(&d)?;
    Some((pb.max_x - pb.min_x, pb.max_y - pb.min_y))
}

fn mindmap_bang_path_d(
    w_base: f64,
    effective_w: f64,
    effective_h: f64,
    number_format: MindmapPathNumberFormat,
) -> String {
    let r = 0.15 * w_base;
    let n = |v| mindmap_path_number(v, number_format);

    format!(
        "M0 0 a{r},{r} 1 0,0 {w25},{hn10} a{r},{r} 1 0,0 {w25},0 a{r},{r} 1 0,0 {w25},0 a{r},{r} 1 0,0 {w25},{h10} a{r},{r} 1 0,0 {w15},{h33} a{r08},{r08} 1 0,0 0,{h34} a{r},{r} 1 0,0 {wn15},{h33} a{r},{r} 1 0,0 {wn25},{h15} a{r},{r} 1 0,0 {wn25},0 a{r},{r} 1 0,0 {wn25},0 a{r},{r} 1 0,0 {wn25},{hn15} a{r},{r} 1 0,0 {wn10},{hn33} a{r08},{r08} 1 0,0 0,{hn34} a{r},{r} 1 0,0 {w10},{hn33} H0 V0 Z",
        r = n(r),
        r08 = n(r * 0.8),
        w25 = n(effective_w * 0.25),
        w15 = n(effective_w * 0.15),
        w10 = n(effective_w * 0.1),
        wn10 = n(-effective_w * 0.1),
        wn15 = n(-effective_w * 0.15),
        wn25 = n(-effective_w * 0.25),
        h10 = n(effective_h * 0.1),
        hn10 = n(-effective_h * 0.1),
        h15 = n(effective_h * 0.15),
        hn15 = n(-effective_h * 0.15),
        h33 = n(effective_h * 0.33),
        hn33 = n(-effective_h * 0.33),
        h34 = n(effective_h * 0.34),
        hn34 = n(-effective_h * 0.34),
    )
}

fn include_mindmap_rect_bounds(
    bounds: &mut Option<Bounds>,
    min_x: f64,
    min_y: f64,
    max_x: f64,
    max_y: f64,
) {
    if let Some(cur) = bounds.as_mut() {
        cur.min_x = cur.min_x.min(min_x);
        cur.min_y = cur.min_y.min(min_y);
        cur.max_x = cur.max_x.max(max_x);
        cur.max_y = cur.max_y.max(max_y);
    } else {
        *bounds = Some(Bounds {
            min_x,
            min_y,
            max_x,
            max_y,
        });
    }
}

fn include_mindmap_node_rect_bounds(bounds: &mut Option<Bounds>, n: &LayoutNode) {
    include_mindmap_rect_bounds(
        bounds,
        n.x - n.width / 2.0,
        n.y - n.height / 2.0,
        n.x + n.width / 2.0,
        n.y + n.height / 2.0,
    );
}

fn single_image_paragraph_inner(fragment: &str) -> Option<&str> {
    let fragment = fragment.trim();
    let inner = fragment.strip_prefix("<p>")?.strip_suffix("</p>")?.trim();
    let prefix = inner.get(..4)?;
    if !prefix.eq_ignore_ascii_case("<img") {
        return None;
    }
    if inner
        .as_bytes()
        .get(4)
        .is_some_and(|byte| !byte.is_ascii_whitespace() && !matches!(byte, b'/' | b'>'))
    {
        return None;
    }

    let mut quote = None;
    for (index, ch) in inner.char_indices().skip(4) {
        match (quote, ch) {
            (Some(active), candidate) if active == candidate => quote = None,
            (None, '\'' | '"') => quote = Some(ch),
            (None, '>') => return inner[index + 1..].trim().is_empty().then_some(inner),
            _ => {}
        }
    }
    None
}

fn include_mindmap_path_bounds(
    bounds: &mut Option<Bounds>,
    d: &str,
    translate_x: f64,
    translate_y: f64,
) -> bool {
    let Some(pb) = svg_path_bounds_from_d(d) else {
        return false;
    };
    include_mindmap_rect_bounds(
        bounds,
        pb.min_x + translate_x,
        pb.min_y + translate_y,
        pb.max_x + translate_x,
        pb.max_y + translate_y,
    );
    true
}

fn mindmap_viewport_bounds_from_layout(
    layout: &MindmapDiagramLayout,
    model: &merman_core::diagrams::mindmap::MindmapDiagramRenderModel,
) -> Option<Bounds> {
    let mut layout_nodes: std::collections::BTreeMap<&str, &LayoutNode> =
        std::collections::BTreeMap::new();
    for n in &layout.nodes {
        layout_nodes.insert(n.id.as_str(), n);
    }

    let mut bounds: Option<Bounds> = None;
    for n in &model.nodes {
        let Some(ln) = layout_nodes.get(n.id.as_str()) else {
            continue;
        };

        let padding = n.padding.max(0.0);
        let half_padding = padding / 2.0;
        match n.shape.as_str() {
            "cloud" => {
                let bbox_w = ln
                    .label_width
                    .unwrap_or_else(|| (ln.width - 2.0 * half_padding).max(1.0));
                let bbox_h = ln
                    .label_height
                    .unwrap_or_else(|| (ln.height - 2.0 * half_padding).max(1.0));
                let w = (bbox_w + 2.0 * half_padding).max(1.0);
                let h = (bbox_h + 2.0 * half_padding).max(1.0);
                let d = mindmap_cloud_path_d(w, h, MindmapPathNumberFormat::JsNumber);
                if !include_mindmap_path_bounds(&mut bounds, &d, ln.x - w / 2.0, ln.y - h / 2.0) {
                    include_mindmap_node_rect_bounds(&mut bounds, ln);
                }
                include_mindmap_rect_bounds(
                    &mut bounds,
                    ln.x - bbox_w / 2.0,
                    ln.y - bbox_h / 2.0,
                    ln.x + bbox_w / 2.0,
                    ln.y + bbox_h / 2.0,
                );
            }
            "bang" => {
                let w = ln.width.max(1.0);
                let h = ln.height.max(1.0);
                let bbox_w = ln
                    .label_width
                    .unwrap_or_else(|| (w - 10.0 * half_padding).max(1.0));
                let bbox_h = ln
                    .label_height
                    .unwrap_or_else(|| (h - 8.0 * half_padding).max(1.0));
                let w_base = bbox_w + 10.0 * half_padding;
                let d = mindmap_bang_path_d(w_base, w, h, MindmapPathNumberFormat::JsNumber);
                if !include_mindmap_path_bounds(&mut bounds, &d, ln.x - w / 2.0, ln.y - h / 2.0) {
                    include_mindmap_node_rect_bounds(&mut bounds, ln);
                }
                include_mindmap_rect_bounds(
                    &mut bounds,
                    ln.x - bbox_w / 2.0,
                    ln.y - bbox_h / 2.0,
                    ln.x + bbox_w / 2.0,
                    ln.y + bbox_h / 2.0,
                );
            }
            _ => include_mindmap_node_rect_bounds(&mut bounds, ln),
        }
    }

    for lane in &layout.swimlane_lanes {
        include_mindmap_rect_bounds(
            &mut bounds,
            lane.x - lane.width / 2.0,
            lane.y - lane.height / 2.0,
            lane.x + lane.width / 2.0,
            lane.y + lane.height / 2.0,
        );
    }
    for e in &layout.edges {
        for p in &e.points {
            include_mindmap_rect_bounds(&mut bounds, p.x, p.y, p.x, p.y);
        }
    }

    bounds
}

fn mindmap_model_look(model_look: &str, config: &merman_core::MermaidConfig) -> String {
    let model_look = model_look.trim();
    if model_look.is_empty() || model_look == "default" {
        crate::config::mermaid_config_diagram_look(config)
            .as_str()
            .to_string()
    } else {
        model_look.to_string()
    }
}

fn mindmap_data_look_attr(model_look: &str, config: &merman_core::MermaidConfig) -> String {
    let look = mindmap_model_look(model_look, config);
    if look.is_empty() {
        String::new()
    } else {
        format!(r#" data-look="{}""#, escape_attr(&look))
    }
}

fn mindmap_dom_id(diagram_id: SvgDiagramId<'_>, raw_id: &str) -> String {
    format!("{diagram_id}-{raw_id}")
}

fn mindmap_wrap_section_index(index: i64) -> i64 {
    if index >= 11 { index % 11 } else { index }
}

fn mindmap_normalize_section_class_token(token: &str) -> String {
    for prefix in ["section-edge-", "section-"] {
        let Some(rest) = token.strip_prefix(prefix) else {
            continue;
        };
        let Ok(index) = rest.parse::<i64>() else {
            return token.to_string();
        };
        return format!("{prefix}{}", mindmap_wrap_section_index(index));
    }
    token.to_string()
}

fn mindmap_normalize_section_classes(classes: &str) -> String {
    classes
        .split_whitespace()
        .map(mindmap_normalize_section_class_token)
        .collect::<Vec<_>>()
        .join(" ")
}

fn mindmap_gradient_defs(
    diagram_id: impl std::fmt::Display,
    effective_config: &serde_json::Value,
) -> String {
    if !config_bool(effective_config, &["themeVariables", "useGradient"]).unwrap_or(false) {
        return String::new();
    }

    let Some(gradient_start) =
        config_string(effective_config, &["themeVariables", "gradientStart"])
    else {
        return String::new();
    };
    let Some(gradient_stop) = config_string(effective_config, &["themeVariables", "gradientStop"])
    else {
        return String::new();
    };

    format!(
        r#"<defs><linearGradient id="{}-gradient" gradientUnits="objectBoundingBox" x1="0%" y1="0%" x2="100%" y2="0%"><stop offset="0%" stop-color="{}" stop-opacity="1"/><stop offset="100%" stop-color="{}" stop-opacity="1"/></linearGradient></defs>"#,
        diagram_id,
        escape_xml(&gradient_start),
        escape_xml(&gradient_stop)
    )
}

fn push_mindmap_shadow_defs(
    out: &mut String,
    diagram_id: impl Copy + std::fmt::Display,
    effective_config: &serde_json::Value,
) {
    let flood_color = effective_config
        .get("theme")
        .and_then(|v| v.as_str())
        .filter(|theme| theme.contains("dark"))
        .map(|_| "#FFFFFF")
        .unwrap_or("#000000");
    let _ = write!(
        out,
        r#"<defs><filter id="{}-drop-shadow" height="130%" width="130%"><feDropShadow dx="4" dy="4" stdDeviation="0" flood-opacity="0.06" flood-color="{}"/></filter></defs><defs><filter id="{}-drop-shadow-small" height="150%" width="150%"><feDropShadow dx="2" dy="2" stdDeviation="0" flood-opacity="0.06" flood-color="{}"/></filter></defs>"#,
        diagram_id, flood_color, diagram_id, flood_color
    );
}

fn mindmap_css(
    diagram_id: impl Copy + std::fmt::Display,
    effective_config: &serde_json::Value,
) -> String {
    // Mirrors pinned Mermaid `diagrams/mindmap/styles.ts` + shared base stylesheet ordering.
    //
    // Keep `:root` last (matches upstream fixtures).
    let parts = info_css_parts_with_config(diagram_id, effective_config);
    let mut out = parts.css_prefix;

    let _ = write!(&mut out, r#"#{} .edge{{stroke-width:3;}}"#, diagram_id);

    // Mermaid default theme resolves `cScale0..11` into this palette for mindmap/kanban/timeline.
    // The first generated section is `section--1` (i=0).
    const DEFAULT_FILLS: [&str; 12] = [
        "hsl(240, 100%, 76.2745098039%)",
        "hsl(60, 100%, 73.5294117647%)",
        "hsl(80, 100%, 76.2745098039%)",
        "hsl(270, 100%, 76.2745098039%)",
        "hsl(300, 100%, 76.2745098039%)",
        "hsl(330, 100%, 76.2745098039%)",
        "hsl(0, 100%, 76.2745098039%)",
        "hsl(30, 100%, 76.2745098039%)",
        "hsl(90, 100%, 76.2745098039%)",
        "hsl(150, 100%, 76.2745098039%)",
        "hsl(180, 100%, 76.2745098039%)",
        "hsl(210, 100%, 76.2745098039%)",
    ];
    const DEFAULT_INV_FILLS: [&str; 12] = [
        "hsl(60, 100%, 86.2745098039%)",
        "hsl(240, 100%, 83.5294117647%)",
        "hsl(260, 100%, 86.2745098039%)",
        "hsl(90, 100%, 86.2745098039%)",
        "hsl(120, 100%, 86.2745098039%)",
        "hsl(150, 100%, 86.2745098039%)",
        "hsl(180, 100%, 86.2745098039%)",
        "hsl(210, 100%, 86.2745098039%)",
        "hsl(270, 100%, 86.2745098039%)",
        "hsl(330, 100%, 86.2745098039%)",
        "hsl(0, 100%, 86.2745098039%)",
        "hsl(30, 100%, 86.2745098039%)",
    ];

    fn default_mindmap_fill(i: usize) -> &'static str {
        DEFAULT_FILLS[i % DEFAULT_FILLS.len()]
    }

    fn default_mindmap_inv_fill(i: usize) -> &'static str {
        DEFAULT_INV_FILLS[i % DEFAULT_INV_FILLS.len()]
    }

    fn default_mindmap_label(i: usize) -> &'static str {
        if i == 0 || i == 3 { "#ffffff" } else { "black" }
    }

    let theme = config_string(effective_config, &["theme"]).unwrap_or_default();
    let look = config_diagram_look(effective_config);
    let theme_color_limit = config_f64(effective_config, &["themeVariables", "THEME_COLOR_LIMIT"])
        .map(|v| v.round() as i64)
        .filter(|v| *v > 0)
        .unwrap_or(DEFAULT_FILLS.len() as i64)
        .clamp(1, 64) as usize;
    let root_fill = theme_token(effective_config, "git0", "hsl(240, 100%, 46.2745098039%)");
    let root_label = theme_token(effective_config, "gitBranchLabel0", "#ffffff");
    let node_border = theme_token(effective_config, "nodeBorder", root_label.as_str());
    let main_bkg = theme_token(effective_config, "mainBkg", root_fill.as_str());
    let stroke_width = crate::config::config_css_number_or_string(
        effective_config,
        &["themeVariables", "strokeWidth"],
    )
    .unwrap_or_else(|| "2".to_string());
    let drop_shadow = crate::config::config_css_number_or_string(
        effective_config,
        &["themeVariables", "dropShadow"],
    )
    .unwrap_or_else(|| "none".to_string());
    let scoped_drop_shadow = scoped_drop_shadow(diagram_id, &drop_shadow);
    let use_gradient =
        config_bool(effective_config, &["themeVariables", "useGradient"]).unwrap_or(false);

    for i in 0..theme_color_limit {
        let section = i as i64 - 1;
        let c_scale = theme_token(
            effective_config,
            &format!("cScale{}", i),
            default_mindmap_fill(i),
        );
        let c_scale_inv = theme_token(
            effective_config,
            &format!("cScaleInv{}", i),
            default_mindmap_inv_fill(i),
        );
        let c_scale_label = theme_token(
            effective_config,
            &format!("cScaleLabel{}", i),
            default_mindmap_label(i),
        );
        let sw = if look.is_neo() {
            (10_i64 - (section * 2)).max(2)
        } else {
            17_i64 - 3_i64 * (i as i64)
        };
        let neo_node_fill = if theme == "redux" || theme == "redux-dark" || theme == "neutral" {
            main_bkg.as_str()
        } else {
            c_scale.as_str()
        };
        let neo_node_stroke = if theme == "redux" || theme == "redux-dark" {
            node_border.as_str()
        } else {
            c_scale.as_str()
        };
        let neo_edge_stroke = if theme.contains("redux") || theme == "neo-dark" {
            node_border.as_str()
        } else {
            c_scale.as_str()
        };
        let neo_text_label_index = if theme == "neutral" { 1 } else { i };
        let neo_text_label = if theme == "redux" || theme == "redux-dark" {
            node_border.clone()
        } else {
            theme_token(
                effective_config,
                &format!("cScaleLabel{}", neo_text_label_index),
                default_mindmap_label(neo_text_label_index),
            )
        };
        let _ = write!(
            &mut out,
            r#"#{} .section-{} rect,#{} .section-{} path,#{} .section-{} circle,#{} .section-{} polygon,#{} .section-{} path{{fill:{};}}"#,
            diagram_id,
            section,
            diagram_id,
            section,
            diagram_id,
            section,
            diagram_id,
            section,
            diagram_id,
            section,
            c_scale
        );
        let _ = write!(
            &mut out,
            r#"#{} .section-{} text{{fill:{};}}"#,
            diagram_id, section, c_scale_label
        );
        let _ = write!(
            &mut out,
            r#"#{} .section-{} span{{color:{};}}"#,
            diagram_id, section, c_scale_label
        );
        let _ = write!(
            &mut out,
            r#"#{} .node-icon-{}{{font-size:40px;color:{};}}"#,
            diagram_id, section, c_scale_label
        );
        let _ = write!(
            &mut out,
            r#"#{} .section-edge-{}{{stroke:{};}}"#,
            diagram_id, section, c_scale
        );
        let _ = write!(
            &mut out,
            r#"#{} .edge-depth-{}{{stroke-width:{};}}"#,
            diagram_id, section, sw
        );
        let _ = write!(
            &mut out,
            r#"#{} .section-{} line{{stroke:{};stroke-width:3;}}"#,
            diagram_id, section, c_scale_inv
        );
        let _ = write!(
            &mut out,
            r#"#{} .disabled,#{} .disabled circle,#{} .disabled text{{fill:lightgray;}}#{} .disabled text{{fill:#efefef;}}"#,
            diagram_id, diagram_id, diagram_id, diagram_id
        );
        let _ = write!(
            &mut out,
            r#"#{} [data-look="neo"].mindmap-node.section-{} rect,#{} [data-look="neo"].mindmap-node.section-{} path,#{} [data-look="neo"].mindmap-node.section-{} circle,#{} [data-look="neo"].mindmap-node.section-{} polygon{{fill:{};stroke:{};stroke-width:{}px;}}"#,
            diagram_id,
            section,
            diagram_id,
            section,
            diagram_id,
            section,
            diagram_id,
            section,
            neo_node_fill,
            neo_node_stroke,
            stroke_width
        );
        let _ = write!(
            &mut out,
            r#"#{} [data-look="neo"].section-edge-{}{{stroke:{};}}"#,
            diagram_id, section, neo_edge_stroke
        );
        let _ = write!(
            &mut out,
            r#"#{} [data-look="neo"].mindmap-node.section-{} text{{fill:{};}}"#,
            diagram_id, section, neo_text_label
        );
    }

    // Root section overrides.
    let root_span = if theme.contains("redux") {
        node_border.as_str()
    } else {
        root_label.as_str()
    };
    let _ = write!(
        &mut out,
        r#"#{} .section-root rect,#{} .section-root path,#{} .section-root circle,#{} .section-root polygon{{fill:{};}}"#,
        diagram_id, diagram_id, diagram_id, diagram_id, root_fill
    );
    let _ = write!(
        &mut out,
        r#"#{} .section-root text{{fill:{};}}"#,
        diagram_id, root_label
    );
    let _ = write!(
        &mut out,
        r#"#{} .section-root span{{color:{};}}"#,
        diagram_id, root_span
    );
    let _ = write!(
        &mut out,
        r#"#{} .icon-container{{height:100%;display:flex;justify-content:center;align-items:center;}}"#,
        diagram_id
    );
    let _ = write!(&mut out, r#"#{} .edge{{fill:none;}}"#, diagram_id);
    let _ = write!(
        &mut out,
        r#"#{} .mindmap-node-label{{dy:1em;alignment-baseline:middle;text-anchor:middle;dominant-baseline:middle;text-align:center;}}"#,
        diagram_id
    );
    let _ = write!(
        &mut out,
        r#"#{} [data-look="neo"].mindmap-node{{filter:{scoped_drop_shadow};}}"#,
        diagram_id
    );
    let neo_root_fill = if theme.contains("redux") {
        main_bkg.as_str()
    } else {
        root_fill.as_str()
    };
    let neo_root_text_label_index = if theme == "neutral" { 1 } else { 0 };
    let neo_root_text = if theme.contains("redux") {
        node_border
    } else {
        theme_token(
            effective_config,
            &format!("cScaleLabel{}", neo_root_text_label_index),
            default_mindmap_label(neo_root_text_label_index),
        )
    };
    let _ = write!(
        &mut out,
        r#"#{} [data-look="neo"].mindmap-node.section-root rect,#{} [data-look="neo"].mindmap-node.section-root path,#{} [data-look="neo"].mindmap-node.section-root circle,#{} [data-look="neo"].mindmap-node.section-root polygon{{fill:{};}}"#,
        diagram_id, diagram_id, diagram_id, diagram_id, neo_root_fill
    );
    let _ = write!(
        &mut out,
        r#"#{} [data-look="neo"].mindmap-node.section-root .text-inner-tspan{{fill:{};}}"#,
        diagram_id, neo_root_text
    );
    if use_gradient {
        for i in 0..theme_color_limit {
            let section = i as i64 - 1;
            let _ = write!(
                &mut out,
                r#"#{} [data-look="neo"].mindmap-node.section-{} rect,#{} [data-look="neo"].mindmap-node.section-{} path,#{} [data-look="neo"].mindmap-node.section-{} circle,#{} [data-look="neo"].mindmap-node.section-{} polygon{{stroke:url(#{}-gradient);fill:{};}}"#,
                diagram_id,
                section,
                diagram_id,
                section,
                diagram_id,
                section,
                diagram_id,
                section,
                diagram_id,
                main_bkg
            );
            let _ = write!(
                &mut out,
                r#"#{} .section-{} line{{stroke-width:0;}}"#,
                diagram_id, section
            );
        }
    }

    out.push_str(&parts.root_rule);
    out
}

pub(crate) fn render_mindmap_diagram_svg_model_with_config(
    layout: &MindmapDiagramLayout,
    model: &merman_core::diagrams::mindmap::MindmapDiagramRenderModel,
    config: &merman_core::MermaidConfig,
    options: &SvgExecution<'_>,
) -> Result<root_svg::RootedSvg> {
    let timing = options.timing();
    let mut timings = super::super::timing::RenderTimings::default();
    let total_timer = timing.start();

    #[derive(Debug, Clone, serde::Serialize)]
    struct Pt {
        x: f64,
        y: f64,
    }

    let max_node_width_px = crate::mindmap::mindmap_max_node_width_px(config.as_value());

    struct MindmapLabelSpec<'a> {
        text: &'a str,
        label_bkg: bool,
        width: f64,
        height: f64,
        tx: f64,
        ty: f64,
        max_node_width_px: f64,
    }

    fn mk_label(
        out: &mut String,
        spec: MindmapLabelSpec<'_>,
        config: &merman_core::MermaidConfig,
        math_renderer: Option<&(dyn crate::math::MathRenderer + Send + Sync)>,
    ) -> Result<()> {
        let MindmapLabelSpec {
            text,
            label_bkg,
            width,
            height,
            tx,
            ty,
            max_node_width_px,
        } = spec;

        let div_class = if label_bkg {
            r#" class="labelBkg""#
        } else {
            ""
        };

        let max_node_width_px = if max_node_width_px.is_finite() && max_node_width_px > 0.0 {
            max_node_width_px
        } else {
            200.0
        };

        // Mermaid flips the `<div>` to a fixed-width wrapping container when the measured label
        // reaches/exceeds the configured max width (default 200px), even if the emitted
        // `<foreignObject width="...">` reflects the overflow width.
        let wrap_container = width >= max_node_width_px - 1e-3;
        out.push_str(r#"<g class="label" style="" transform="translate("#);
        fmt_into(out, tx);
        out.push_str(", ");
        fmt_into(out, ty);
        out.push_str(r#")"><rect/><foreignObject width=""#);
        fmt_into(out, width.max(1.0));
        out.push_str(r#"" height=""#);
        fmt_into(out, height.max(1.0));
        out.push_str(r#""><div xmlns="http://www.w3.org/1999/xhtml""#);
        out.push_str(div_class);
        out.push_str(r#" style=""#);
        if wrap_container {
            out.push_str(
                "display: table; white-space: break-spaces; line-height: 1.5; max-width: ",
            );
            fmt_into(out, max_node_width_px);
            out.push_str("px; text-align: center; width: ");
            fmt_into(out, max_node_width_px);
            out.push_str("px;");
        } else {
            out.push_str("display: table-cell; white-space: nowrap; line-height: 1.5; max-width: ");
            fmt_into(out, max_node_width_px);
            out.push_str("px; text-align: center;");
        }
        out.push_str(r#""><span class="nodeLabel markdown-node-label">"#);
        fn markdown_to_sanitized_xhtml(text: &str, config: &merman_core::MermaidConfig) -> String {
            let html_out = crate::text::mermaid_markdown_to_xhtml_label_fragment(text, true);
            let html_out = crate::text::replace_fontawesome_icons(&html_out);
            let html_out = merman_core::sanitize::sanitize_text(&html_out, config);
            let html_out = html_out
                .replace("<br>", "<br />")
                .replace("<br/>", "<br />");
            // Mermaid inserts the sanitized fragment without trimming it. This is observable for
            // indented-code labels once the 200px container switches to `break-spaces`: a trailing
            // indentation-only source line still owns a browser line box.
            single_image_paragraph_inner(&html_out)
                .map(str::to_string)
                .unwrap_or(html_out)
        }

        fn escape_amp_preserving_entities(raw: &str) -> String {
            crate::xml::normalize_html_entities_for_xml(raw).into_owned()
        }

        if crate::math::contains_delimited_math(text) {
            let html = math_renderer
                .and_then(|renderer| renderer.render_html_label(text, config))
                .ok_or_else(|| Error::MissingCapability {
                    capability: crate::RenderCapability::Math,
                    diagram_type: "mindmap".to_string(),
                })?;
            let html = merman_core::sanitize::sanitize_text(&html, config)
                .replace("<br>", "<br />")
                .replace("<br/>", "<br />");
            out.push_str(&escape_amp_preserving_entities(&html));
        } else {
            let html = markdown_to_sanitized_xhtml(text, config);
            let html = decode_mermaid_entities_for_render_text(&html);
            out.push_str(&escape_amp_preserving_entities(html.as_ref()));
        }

        out.push_str("</span></div></foreignObject></g>");
        Ok(())
    }

    fn mk_edge_label(out: &mut String, edge_id: &str) {
        let _ = write!(
            out,
            r#"<g class="edgeLabel"><g class="label" data-id="{id}" transform="translate(0, 0)"><foreignObject width="0" height="0"><div xmlns="http://www.w3.org/1999/xhtml" class="labelBkg" style="display: table-cell; white-space: nowrap; line-height: 1.5; max-width: 200px; text-align: center;"><span class="edgeLabel"></span></div></foreignObject></g></g>"#,
            id = escape_xml(edge_id),
        );
    }

    let _g_build_ctx = timing.section(&mut timings.build_ctx);

    let diagram_id = options.diagram_id_or("mindmap");
    let math_renderer = options.math_renderer();

    let mut node_by_id: std::collections::BTreeMap<String, &crate::model::LayoutNode> =
        std::collections::BTreeMap::new();
    for n in &layout.nodes {
        node_by_id.insert(n.id.clone(), n);
    }

    let edge_by_id: std::collections::BTreeMap<_, _> = layout
        .edges
        .iter()
        .map(|edge| (edge.id.as_str(), edge))
        .collect();
    let source_node_by_id: std::collections::BTreeMap<_, _> = model
        .nodes
        .iter()
        .map(|node| (node.id.as_str(), node))
        .collect();
    let backend = crate::mindmap::layout_backend(config.as_value());

    drop(_g_build_ctx);

    let _g_viewbox = timing.section(&mut timings.viewbox);

    let padding = 10.0;
    let viewport_bounds =
        mindmap_viewport_bounds_from_layout(layout, model).or_else(|| layout.bounds.clone());
    let (vx, vy, vw, vh) = viewport_bounds
        .as_ref()
        .map(|b| {
            let w = (b.max_x - b.min_x).max(0.0);
            let h = (b.max_y - b.min_y).max(0.0);
            (
                b.min_x - padding,
                b.min_y - padding,
                w + 2.0 * padding,
                h + 2.0 * padding,
            )
        })
        .unwrap_or((0.0, 0.0, 100.0, 100.0));

    let root_spec = root_svg::RootViewportSpec::responsive(root_svg::DiagramBounds::from_view_box(
        vx, vy, vw, vh,
    ))
    .with_max_width(root_svg::RootMaxWidth::CssSixSignificant(vw));

    drop(_g_viewbox);

    let _g_render_svg = timing.section(&mut timings.render_svg);

    let mut out = String::new();
    let root_document =
        root_svg::RootViewportContext::new(crate::family::RenderFamilyKind::Mindmap, diagram_id)
            .write_open(
                &mut out,
                root_spec,
                root_svg::RootChrome {
                    class: Some("mindmapDiagram"),
                    dom: root_svg::RootDomProfile {
                        trailing_newline: false,
                        ..Default::default()
                    },
                    ..root_svg::RootChrome::new(diagram_id, "mindmap")
                },
            )?;
    options.checkpoint_emit()?;
    let css = mindmap_css(diagram_id, config.as_value());
    let _ = write!(&mut out, "<style>{}</style>", css);
    out.push_str(&mindmap_gradient_defs(diagram_id, config.as_value()));
    out.push_str("<g>");

    let _ = write!(
        &mut out,
        r#"<marker id="{id}_mindmap-pointEnd" class="marker mindmap" viewBox="0 0 10 10" refX="5" refY="5" markerUnits="userSpaceOnUse" markerWidth="8" markerHeight="8" orient="auto"><path d="M 0 0 L 10 5 L 0 10 z" class="arrowMarkerPath" style="stroke-width: 1; stroke-dasharray: 1, 0;"/></marker>"#,
        id = diagram_id
    );
    let _ = write!(
        &mut out,
        r#"<marker id="{id}_mindmap-pointStart" class="marker mindmap" viewBox="0 0 10 10" refX="4.5" refY="5" markerUnits="userSpaceOnUse" markerWidth="8" markerHeight="8" orient="auto"><path d="M 0 5 L 10 10 L 10 0 z" class="arrowMarkerPath" style="stroke-width: 1; stroke-dasharray: 1, 0;"/></marker>"#,
        id = diagram_id
    );
    let _ = write!(
        &mut out,
        r#"<marker id="{id}_mindmap-pointEnd-margin" class="marker mindmap" viewBox="0 0 11.5 14" refX="11.5" refY="7" markerUnits="userSpaceOnUse" markerWidth="10.5" markerHeight="14" orient="auto"><path d="M 0 0 L 11.5 7 L 0 14 z" class="arrowMarkerPath" style="stroke-width: 0; stroke-dasharray: 1, 0;"/></marker>"#,
        id = diagram_id
    );
    let _ = write!(
        &mut out,
        r#"<marker id="{id}_mindmap-pointStart-margin" class="marker mindmap" viewBox="0 0 11.5 14" refX="1" refY="7" markerUnits="userSpaceOnUse" markerWidth="11.5" markerHeight="14" orient="auto"><polygon points="0,7 11.5,14 11.5,0" class="arrowMarkerPath" style="stroke-width: 0; stroke-dasharray: 1, 0;"/></marker>"#,
        id = diagram_id
    );
    options.checkpoint_emit()?;

    if layout.swimlane_lanes.is_empty() {
        out.push_str(r#"<g class="subgraphs"/>"#);
    } else {
        out.push_str(r#"<g class="subgraphs">"#);
        let theme = PresentationTheme::new(config.as_value()).node_diagram();
        for lane in &layout.swimlane_lanes {
            let x = lane.x - lane.width / 2.0;
            let y = lane.y - lane.height / 2.0;
            let _ = write!(
                out,
                r#"<g class="cluster swimlane" id="{id}" data-id="{id}" data-et="cluster" data-look="{look}"><rect class="swimlane-body" style="" x="{x}" y="{y}" width="{width}" height="{height}" fill="none" stroke="{stroke}"/><rect class="swimlane-title" style="" x="{x}" y="{y}" width="{width}" height="0" fill="{fill}" stroke="{stroke}"/><g class="cluster-label swimlane-label" transform="translate({center}, {y})"><foreignObject width="0" height="0"><div xmlns="http://www.w3.org/1999/xhtml" style="display: table-cell; white-space: nowrap; line-height: 1.5; max-width: {width}px; text-align: center;"><span class="nodeLabel"></span></div></foreignObject></g></g>"#,
                id = escape_xml(&lane.id),
                look = escape_attr(crate::config::mermaid_config_diagram_look(config).as_str()),
                x = fmt(x),
                y = fmt(y),
                width = fmt(lane.width),
                height = fmt(lane.height),
                center = fmt(lane.x),
                fill = escape_attr(&theme.cluster_bkg),
                stroke = escape_attr(&theme.cluster_border)
            );
        }
        out.push_str("</g>");
    }

    out.push_str(r#"<g class="edgePaths">"#);
    for e in &model.edges {
        let (sx, sy, tx, ty) = match (node_by_id.get(&e.start), node_by_id.get(&e.end)) {
            (Some(a), Some(b)) => (a.x, a.y, b.x, b.y),
            _ => (0.0, 0.0, 0.0, 0.0),
        };

        let points_for_data_points = if backend == crate::mindmap::MindmapLayoutBackend::Cose {
            // Mermaid mindmap edges use `curveBasis` and offset endpoints from node centers
            // along the direction of the edge.
            let (vx, vy) = (tx - sx, ty - sy);
            let v_len = (vx * vx + vy * vy).sqrt();
            let (ux, uy) = if v_len == 0.0 {
                (0.0, 0.0)
            } else {
                (vx / v_len, vy / v_len)
            };
            let endpoint_offset = 15.0;
            let start_x = sx + endpoint_offset * ux;
            let start_y = sy + endpoint_offset * uy;
            let end_x = tx - endpoint_offset * ux;
            let end_y = ty - endpoint_offset * uy;
            let mid_x = (start_x + end_x) / 2.0;
            let mid_y = (start_y + end_y) / 2.0;

            let points = [
                Pt {
                    x: start_x,
                    y: start_y,
                },
                Pt { x: mid_x, y: mid_y },
                Pt { x: end_x, y: end_y },
            ];
            points
                .iter()
                .map(|p| crate::model::LayoutPoint { x: p.x, y: p.y })
                .collect::<Vec<_>>()
        } else if backend == crate::mindmap::MindmapLayoutBackend::TidyTree {
            edge_by_id
                .get(e.id.as_str())
                .map(|edge| edge.points.clone())
                .unwrap_or_default()
        } else {
            let points = edge_by_id
                .get(e.id.as_str())
                .map(|edge| edge.points.clone())
                .unwrap_or_default();
            let start = node_by_id.get(&e.start).copied();
            let end = node_by_id.get(&e.end).copied();
            let is_circle = |id: &str| {
                source_node_by_id
                    .get(id)
                    .is_some_and(|node| node.shape == "mindmapCircle")
            };
            match (start, end) {
                (Some(start), Some(end)) => {
                    #[cfg(feature = "layout-elk")]
                    if backend == crate::mindmap::MindmapLayoutBackend::Elk {
                        use crate::elk_edge_geometry::{Outline, Shape, sanitize};
                        {
                            let mut elk_points = Vec::with_capacity(points.len() + 2);
                            elk_points.push(crate::model::LayoutPoint {
                                x: start.x,
                                y: start.y,
                            });
                            elk_points.extend(points.iter().cloned());
                            elk_points.push(crate::model::LayoutPoint { x: end.x, y: end.y });
                            sanitize(
                                &elk_points,
                                Shape {
                                    intersection: None,
                                    node: start,
                                    outline: if is_circle(&e.start) {
                                        Outline::Ellipse
                                    } else {
                                        Outline::Rect
                                    },
                                },
                                Shape {
                                    intersection: None,
                                    node: end,
                                    outline: if is_circle(&e.end) {
                                        Outline::Ellipse
                                    } else {
                                        Outline::Rect
                                    },
                                },
                            )
                        }
                    } else {
                        crate::mindmap::dagre_shape_points(
                            points,
                            start,
                            end,
                            is_circle(&e.start),
                            is_circle(&e.end),
                        )
                    }
                    #[cfg(not(feature = "layout-elk"))]
                    crate::mindmap::dagre_shape_points(
                        points,
                        start,
                        end,
                        is_circle(&e.start),
                        is_circle(&e.end),
                    )
                }
                _ => points,
            }
        };
        let data_points = base64::engine::general_purpose::STANDARD
            .encode(json_stringify_points(&points_for_data_points));

        let d = if backend == crate::mindmap::MindmapLayoutBackend::Swimlane {
            curve::curve_rounded_path_d_and_bounds(&points_for_data_points, 10.0, false, None).0
        } else if e.curve.trim() == "basis" {
            curve::curve_basis_path_d(&points_for_data_points)
        } else {
            curve::curve_linear_path_d(&points_for_data_points)
        };
        let edge_classes = mindmap_normalize_section_classes(&e.classes);
        let class = format!(
            "edge-thickness-{} edge-pattern-solid {}",
            e.thickness.trim(),
            edge_classes.trim()
        );
        let data_look_attr = mindmap_data_look_attr(&e.look, config);
        let edge_dom_id = mindmap_dom_id(diagram_id, &e.id);
        options.checkpoint_emit()?;
        let _ = write!(
            &mut out,
            r#"<path d="{d}" id="{dom_id}" class="{class}"{look_attr} data-edge="true" data-et="edge" data-id="{id}" data-points="{pts}"/>"#,
            d = escape_attr(&d),
            dom_id = escape_xml(&edge_dom_id),
            class = escape_xml(&class),
            look_attr = data_look_attr,
            id = escape_xml(&e.id),
            pts = escape_xml(&data_points),
        );
    }
    out.push_str("</g>");

    out.push_str(r#"<g class="edgeLabels">"#);
    for e in &model.edges {
        mk_edge_label(&mut out, &e.id);
    }
    out.push_str("</g>");

    out.push_str(r#"<g class="nodes">"#);
    for n in &model.nodes {
        let (x, y, w, h, label_w, label_h) = node_by_id
            .get(&n.id)
            .map(|ln| {
                (
                    ln.x,
                    ln.y,
                    ln.width,
                    ln.height,
                    ln.label_width,
                    ln.label_height,
                )
            })
            .unwrap_or((0.0, 0.0, 80.0, 44.0, None, None));
        let padding = n.padding.max(0.0);
        let half_padding = padding / 2.0;
        let node_classes = mindmap_normalize_section_classes(&n.css_classes);
        let class = format!("node {}", node_classes.trim());
        let data_look_attr = mindmap_data_look_attr(&n.look, config);
        let _ = write!(
            &mut out,
            r#"<g class="{class}" id="{dom_id}"{look_attr} transform="translate({x}, {y})">"#,
            class = escape_xml(&class),
            dom_id = escape_xml(&mindmap_dom_id(diagram_id, &n.dom_id)),
            look_attr = data_look_attr,
            x = fmt(x),
            y = fmt(y),
        );
        options.checkpoint_emit()?;

        match n.shape.as_str() {
            "defaultMindmapNode" => {
                let rd = 5.0;
                let rect_path = format!(
                    "\n    M{} {}\n    v{}\n    q0,-{} {},-{}\n    h{}\n    q{},0 {},{}\n    v{}\n    q0,{} -{},{}\n    h{}\n    q-{},0 -{},-{}\n    Z\n  ",
                    fmt_path(-(w / 2.0)),
                    fmt_path(h / 2.0 - rd),
                    fmt_path(-h + 2.0 * rd),
                    fmt_path(rd),
                    fmt_path(rd),
                    fmt_path(rd),
                    fmt_path(w - 2.0 * rd),
                    fmt_path(rd),
                    fmt_path(rd),
                    fmt_path(rd),
                    fmt_path(h - 2.0 * rd),
                    fmt_path(rd),
                    fmt_path(rd),
                    fmt_path(rd),
                    fmt_path(-w + 2.0 * rd),
                    fmt_path(rd),
                    fmt_path(rd),
                    fmt_path(rd),
                );

                // Recover label bbox dimensions from the rendered node size + padding rules.
                let bbox_w = (w - 8.0 * half_padding).max(1.0);
                let bbox_h = (h - 2.0 * half_padding).max(1.0);
                let _ = write!(
                    &mut out,
                    r#"<path id="{id}" class="node-bkg node-0" style="" d="{d}"/>"#,
                    id = escape_xml(&mindmap_dom_id(diagram_id, &n.dom_id)),
                    d = escape_attr(&rect_path),
                );
                options.checkpoint_emit()?;
                let _ = write!(
                    &mut out,
                    r#"<line class="node-line-" x1="{x1}" y1="{y}" x2="{x2}" y2="{y}"/>"#,
                    x1 = fmt(-(w / 2.0)),
                    x2 = fmt(w / 2.0),
                    y = fmt(h / 2.0),
                );
                mk_label(
                    &mut out,
                    MindmapLabelSpec {
                        text: &n.label,
                        label_bkg: n.icon.is_some(),
                        width: bbox_w,
                        height: bbox_h,
                        tx: -bbox_w / 2.0,
                        ty: -bbox_h / 2.0,
                        max_node_width_px,
                    },
                    config,
                    math_renderer,
                )?;
            }
            "rect" => {
                // `rect` mindmap nodes use: w = bbox_w + 2*padding, h = bbox_h + padding.
                let bbox_w = (w - 2.0 * padding).max(1.0);
                let bbox_h = (h - padding).max(1.0);
                let _ = write!(
                    &mut out,
                    r#"<rect class="basic label-container" style="" x="{x}" y="{y}" width="{w}" height="{h}"/>"#,
                    x = fmt(-(w / 2.0)),
                    y = fmt(-(h / 2.0)),
                    w = fmt(w.max(1.0)),
                    h = fmt(h.max(1.0)),
                );
                mk_label(
                    &mut out,
                    MindmapLabelSpec {
                        text: &n.label,
                        label_bkg: n.icon.is_some(),
                        width: bbox_w,
                        height: bbox_h,
                        tx: -bbox_w / 2.0,
                        ty: -bbox_h / 2.0,
                        max_node_width_px,
                    },
                    config,
                    math_renderer,
                )?;
            }
            "rounded" => {
                let w = w.max(1.0);
                let h = h.max(1.0);
                let _ = write!(
                    &mut out,
                    r#"<rect class="basic label-container" style="" rx="5" ry="5" x="{x}" y="{y}" width="{w}" height="{h}"/>"#,
                    x = fmt(-(w / 2.0)),
                    y = fmt(-(h / 2.0)),
                    w = fmt(w),
                    h = fmt(h),
                );

                let bbox_w = label_w.unwrap_or_else(|| (w - 2.0 * padding).max(1.0));
                let bbox_h = label_h.unwrap_or_else(|| (h - 2.0 * padding).max(1.0));
                mk_label(
                    &mut out,
                    MindmapLabelSpec {
                        text: &n.label,
                        label_bkg: n.icon.is_some(),
                        width: bbox_w,
                        height: bbox_h,
                        tx: -bbox_w / 2.0,
                        ty: -bbox_h / 2.0,
                        max_node_width_px,
                    },
                    config,
                    math_renderer,
                )?;
            }
            "mindmapCircle" => {
                let r = (w.max(h) / 2.0).max(1.0);
                let _ = write!(
                    &mut out,
                    r#"<circle class="basic label-container" style="" r="{r}" cx="0" cy="0"/>"#,
                    r = fmt(r),
                );
                // Mermaid sizes the circle diameter using `bbox.width`, but label placement still
                // uses the true label bbox height (not a square).
                let bbox_w = label_w.unwrap_or_else(|| (w - 2.0 * padding).max(1.0));
                let bbox_h = label_h.unwrap_or_else(|| (h - 2.0 * padding).max(1.0));
                mk_label(
                    &mut out,
                    MindmapLabelSpec {
                        text: &n.label,
                        label_bkg: n.icon.is_some(),
                        width: bbox_w,
                        height: bbox_h,
                        tx: -bbox_w / 2.0,
                        ty: -bbox_h / 2.0,
                        max_node_width_px,
                    },
                    config,
                    math_renderer,
                )?;
            }
            "cloud" => {
                let bbox_w = label_w.unwrap_or_else(|| (w - 2.0 * half_padding).max(1.0));
                let bbox_h = label_h.unwrap_or_else(|| (h - 2.0 * half_padding).max(1.0));
                let w = (bbox_w + 2.0 * half_padding).max(1.0);
                let h = (bbox_h + 2.0 * half_padding).max(1.0);

                let cloud_path = mindmap_cloud_path_d(w, h, MindmapPathNumberFormat::D3Path);

                let _ = write!(
                    &mut out,
                    r#"<path class="basic label-container" style="" d="{d}" transform="translate({tx}, {ty})"/>"#,
                    d = escape_attr(&cloud_path),
                    tx = fmt(-(w / 2.0)),
                    ty = fmt(-(h / 2.0)),
                );
                mk_label(
                    &mut out,
                    MindmapLabelSpec {
                        text: &n.label,
                        label_bkg: n.icon.is_some(),
                        width: bbox_w,
                        height: bbox_h,
                        tx: -bbox_w / 2.0,
                        ty: -bbox_h / 2.0,
                        max_node_width_px,
                    },
                    config,
                    math_renderer,
                )?;
            }
            "hexagon" => {
                let w = w.max(1.0);
                let h = h.max(1.0);
                let fixed_length = h / 4.0;
                let points = format!(
                    "{},0 {},0 {},{} {},{} {},{} 0,{}",
                    fmt_string(fixed_length),
                    fmt_string(w - fixed_length),
                    fmt_string(w),
                    fmt_string(-h / 2.0),
                    fmt_string(w - fixed_length),
                    fmt_string(-h),
                    fmt_string(fixed_length),
                    fmt_string(-h),
                    fmt_string(-h / 2.0),
                );
                let _ = write!(
                    &mut out,
                    r#"<polygon points="{points}" class="label-container" transform="translate({tx},{ty})"/>"#,
                    points = escape_attr(&points),
                    tx = fmt(-(w / 2.0)),
                    ty = fmt(h / 2.0),
                );
                let label_width = label_w.unwrap_or_else(|| w.max(1.0));
                let label_height = label_h.unwrap_or_else(|| h.max(1.0));
                mk_label(
                    &mut out,
                    MindmapLabelSpec {
                        text: &n.label,
                        label_bkg: n.icon.is_some(),
                        width: label_width,
                        height: label_height,
                        tx: -label_width / 2.0,
                        ty: -label_height / 2.0,
                        max_node_width_px,
                    },
                    config,
                    math_renderer,
                )?;
            }
            "bang" => {
                let bbox_w = label_w.unwrap_or_else(|| (w - 10.0 * half_padding).max(1.0));
                let bbox_h = label_h.unwrap_or_else(|| (h - 8.0 * half_padding).max(1.0));

                let w_base = bbox_w + 10.0 * half_padding;
                let effective_w = w.max(1.0);
                let effective_h = h.max(1.0);

                let bang_path = mindmap_bang_path_d(
                    w_base,
                    effective_w,
                    effective_h,
                    MindmapPathNumberFormat::D3Path,
                );

                let _ = write!(
                    &mut out,
                    r#"<path class="basic label-container" style="" d="{d}" transform="translate({tx}, {ty})"/>"#,
                    d = escape_attr(&bang_path),
                    tx = fmt(-(effective_w / 2.0)),
                    ty = fmt(-(effective_h / 2.0)),
                );
                mk_label(
                    &mut out,
                    MindmapLabelSpec {
                        text: &n.label,
                        label_bkg: n.icon.is_some(),
                        width: bbox_w,
                        height: bbox_h,
                        tx: -bbox_w / 2.0,
                        ty: -bbox_h / 2.0,
                        max_node_width_px,
                    },
                    config,
                    math_renderer,
                )?;
            }
            _ => {
                let _ = write!(
                    &mut out,
                    r#"<rect class="basic label-container" style="" x="{x}" y="{y}" width="{w}" height="{h}"/>"#,
                    x = fmt(-(w / 2.0)),
                    y = fmt(-(h / 2.0)),
                    w = fmt(w.max(1.0)),
                    h = fmt(h.max(1.0)),
                );
                mk_label(
                    &mut out,
                    MindmapLabelSpec {
                        text: &n.label,
                        label_bkg: n.icon.is_some(),
                        width: w.max(1.0),
                        height: h.max(1.0),
                        tx: -w / 2.0,
                        ty: -h / 2.0,
                        max_node_width_px,
                    },
                    config,
                    math_renderer,
                )?;
            }
        }

        out.push_str("</g>");
    }
    out.push_str("</g>");

    out.push_str("</g>");
    push_mindmap_shadow_defs(&mut out, diagram_id, config.as_value());
    out.push_str("</svg>\n");
    options.checkpoint_emit()?;

    drop(_g_render_svg);

    timings.total = total_timer
        .map(merman_core::runtime::OperationTimer::elapsed)
        .unwrap_or_default();
    if timing.is_enabled() {
        eprintln!(
            "[render-timing] diagram=mindmap total={:?} deserialize={:?} build_ctx={:?} viewbox={:?} render_svg={:?} finalize={:?} nodes={} edges={}",
            timings.total,
            timings.deserialize_model,
            timings.build_ctx,
            timings.viewbox,
            timings.render_svg,
            timings.finalize_svg,
            model.nodes.len(),
            model.edges.len(),
        );
    }

    root_document.complete(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn single_image_paragraph_is_unwrapped_from_the_final_xhtml_shape() {
        assert_eq!(
            single_image_paragraph_inner(r#"<p><img src="a>b" /></p>"#),
            Some(r#"<img src="a>b" />"#)
        );
        assert_eq!(single_image_paragraph_inner("<p>text</p>"), None);
        assert_eq!(
            single_image_paragraph_inner("<p><img src=x><span>extra</span></p>"),
            None
        );
    }

    #[test]
    fn mindmap_css_honors_mermaid_11_15_theme_sections() {
        let cfg = serde_json::json!({
            "theme": "redux",
            "look": "neo",
            "themeVariables": {
                "THEME_COLOR_LIMIT": 3,
                "cScale0": "#101010",
                "cScaleLabel0": "#f0f0f0",
                "cScaleInv0": "#202020",
                "cScale1": "#303030",
                "cScaleLabel1": "#404040",
                "cScaleInv1": "#505050",
                "cScale2": "#606060",
                "cScaleLabel2": "#707070",
                "cScaleInv2": "#808080",
                "git0": "#909090",
                "gitBranchLabel0": "#a0a0a0",
                "nodeBorder": "#b0b0b0"
            }
        });

        let css = mindmap_css("mm", &cfg);

        assert!(css.contains(r#"#mm .section--1 rect,#mm .section--1 path,#mm .section--1 circle,#mm .section--1 polygon,#mm .section--1 path{fill:#101010;}"#));
        assert!(css.contains(r#"#mm .section--1 span{color:#f0f0f0;}"#));
        assert!(css.contains(r#"#mm .section-0 span{color:#404040;}"#));
        assert!(css.contains(r#"#mm .section-1 line{stroke:#808080;stroke-width:3;}"#));
        assert!(css.contains(r#"#mm .edge-depth--1{stroke-width:12;}"#));
        assert!(css.contains(r#"#mm .edge-depth-0{stroke-width:10;}"#));
        assert!(css.contains(r#"#mm .section-root rect,#mm .section-root path,#mm .section-root circle,#mm .section-root polygon{fill:#909090;}"#));
        assert!(css.contains(r#"#mm .section-root text{fill:#a0a0a0;}"#));
        assert!(css.contains(r#"#mm .section-root span{color:#b0b0b0;}"#));
    }

    #[test]
    fn viewport_bounds_include_cloud_path_bbox() {
        let layout = MindmapDiagramLayout {
            swimlane_lanes: Vec::new(),
            nodes: vec![LayoutNode {
                id: "0".to_string(),
                x: 63.953125,
                y: 32.0,
                width: 97.90625,
                height: 34.0,
                is_cluster: false,
                label_width: Some(87.90625),
                label_height: Some(24.0),
            }],
            edges: Vec::new(),
            bounds: Some(Bounds {
                min_x: 15.0,
                min_y: 15.0,
                max_x: 112.90625,
                max_y: 49.0,
            }),
        };
        let model = merman_core::diagrams::mindmap::MindmapDiagramRenderModel {
            nodes: vec![merman_core::diagrams::mindmap::MindmapDiagramRenderNode {
                id: "0".to_string(),
                dom_id: "node_0".to_string(),
                label: "I am a cloud".to_string(),
                label_type: String::new(),
                is_group: false,
                shape: "cloud".to_string(),
                width: 0.0,
                height: 0.0,
                padding: 10.0,
                css_classes: "mindmap-node section-root section--1".to_string(),
                css_styles: Vec::new(),
                look: String::new(),
                icon: None,
                x: None,
                y: None,
                level: 0,
                node_id: "id".to_string(),
                node_type: -1,
                section: Some(-1),
            }],
            edges: Vec::new(),
        };

        let layout_bounds = layout.bounds.as_ref().expect("layout bounds");
        let bounds = mindmap_viewport_bounds_from_layout(&layout, &model).expect("bounds");

        assert!(bounds.min_x < layout_bounds.min_x);
        assert!(bounds.min_y < layout_bounds.min_y);
        assert!(bounds.max_x > layout_bounds.max_x);
        assert!(bounds.max_y > layout_bounds.max_y);
    }
}
