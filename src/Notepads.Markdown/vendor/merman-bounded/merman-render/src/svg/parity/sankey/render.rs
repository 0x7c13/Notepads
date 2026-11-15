use super::super::*;

#[derive(Clone)]
struct SankeyNodeUid<'a> {
    diagram_id: Option<SvgDiagramId<'a>>,
    local_id: String,
}

impl std::fmt::Display for SankeyNodeUid<'_> {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self.diagram_id {
            Some(diagram_id) => write!(formatter, "{diagram_id}-{}", self.local_id),
            None => formatter.write_str(&self.local_id),
        }
    }
}

pub(crate) fn render_sankey_diagram_svg(
    layout: &SankeyDiagramLayout,
    effective_config: &serde_json::Value,
    options: &SvgExecution<'_>,
) -> Result<root_svg::RootedSvg> {
    let render_settings = crate::sankey::SankeyConfigView::new(effective_config).render_settings();
    let use_max_width = render_settings.use_max_width;
    let show_values = render_settings.show_values;
    let prefix = render_settings.prefix;
    let suffix = render_settings.suffix;
    let link_color = render_settings.link_color;
    let outlined_labels = render_settings.outlined_labels;
    let node_colors = render_settings.node_colors;

    let layout_width = layout.width.max(1.0);
    let layout_height = layout.height.max(1.0);
    let diagram_id = options.diagram_id_or("sankey");
    let scope_generated_ids = options.has_explicit_diagram_id();

    const DEFAULT_ASCENT_EM: f64 = 0.9285714286;
    const DEFAULT_DESCENT_EM: f64 = 0.262;
    let label_font_size: f64 = 14.0;
    let label_gap_x: f64 = 6.0;
    let label_hide_values_dy_em: f64 = 0.35;
    let measurer = options.text_measurer();
    let label_style = crate::text::TextStyle {
        font_family: Some(crate::config::config_font_family_css(effective_config)),
        font_size: label_font_size,
        font_weight: None,
        font_style: None,
    };
    let label_texts: Vec<String> = layout
        .nodes
        .iter()
        .map(|node| {
            if show_values {
                let value = (node.value * 100.0).round() / 100.0;
                format!("{}\n{}{}{}", node.id, prefix, value, suffix)
            } else {
                node.id.clone()
            }
        })
        .collect();

    let mut min_x: f64 = 0.0;
    let mut min_y: f64 = 0.0;
    let mut max_x = layout_width;
    let mut max_y = layout_height;

    for (n, text) in layout.nodes.iter().zip(&label_texts) {
        min_x = min_x.min(n.x0);
        min_y = min_y.min(n.y0);
        max_x = max_x.max(n.x1);
        max_y = max_y.max(n.y1);

        let dy_em = if show_values {
            0.0
        } else {
            label_hide_values_dy_em
        };
        let baseline_y = (n.y0 + n.y1) / 2.0 + dy_em * label_font_size;
        let (ascent, descent) = crate::text::svg_text_vertical_extents_px(
            measurer,
            text,
            &label_style,
            (
                label_font_size * DEFAULT_ASCENT_EM,
                label_font_size * DEFAULT_DESCENT_EM,
            ),
        );
        min_y = min_y.min(baseline_y - ascent);
        max_y = max_y.max(baseline_y + descent);
    }

    for l in &layout.links {
        let sw = l.width.max(1.0);
        let half = sw / 2.0;
        let y0 = l.y0.min(l.y1);
        let y1 = l.y0.max(l.y1);
        min_y = min_y.min(y0 - half);
        max_y = max_y.max(y1 + half);
    }

    let vb_w = (max_x - min_x).max(1.0);
    let vb_h = (max_y - min_y).max(1.0);

    let root_spec = root_svg::RootViewportSpec::mermaid(
        root_svg::DiagramBounds::from_view_box(min_x, min_y, vb_w, vb_h),
        use_max_width,
    )
    .with_max_width(root_svg::RootMaxWidth::SvgNumber(vb_w));

    let mut out = String::new();
    let root_document =
        root_svg::RootViewportContext::new(crate::family::RenderFamilyKind::Sankey, diagram_id)
            .write_open(
                &mut out,
                root_spec,
                root_svg::RootChrome {
                    dom: root_svg::RootDomProfile {
                        fixed_height_placement: root_svg::SvgRootFixedHeightPlacement::AfterXmlns,
                        fixed_style_placement: root_svg::RootStylePlacement::Tail,
                        trailing_newline: false,
                        ..Default::default()
                    },
                    ..root_svg::RootChrome::new(diagram_id, "sankey")
                },
            )?;
    let _ = write!(
        &mut out,
        "<style>{}</style>",
        sankey_css(diagram_id, effective_config)
    );
    options.checkpoint_emit()?;
    out.push_str("<g/>");

    let scheme_tableau10: [&str; 10] = [
        "#4e79a7", "#f28e2c", "#e15759", "#76b7b2", "#59a14f", "#edc949", "#af7aa1", "#ff9da7",
        "#9c755f", "#bab0ab",
    ];

    let mut color_domain: std::collections::HashMap<String, usize> =
        std::collections::HashMap::new();
    let mut color_for = |id: &str| -> String {
        if let Some(color) = node_colors
            .and_then(|colors| colors.get(id))
            .and_then(|color| color.as_str())
        {
            return color.to_string();
        }
        if let Some(&idx) = color_domain.get(id) {
            return scheme_tableau10[idx % scheme_tableau10.len()].to_string();
        }
        let idx = color_domain.len();
        color_domain.insert(id.to_string(), idx);
        scheme_tableau10[idx % scheme_tableau10.len()].to_string()
    };

    let mut uid_count: usize = 0;
    let mut next_generated_id = |prefix: &str| -> SankeyNodeUid<'_> {
        uid_count += 1;
        let local_id = format!("{prefix}{uid_count}");
        SankeyNodeUid {
            diagram_id: scope_generated_ids.then_some(diagram_id),
            local_id,
        }
    };

    let mut node_uid_by_id: std::collections::HashMap<String, SankeyNodeUid<'_>> =
        std::collections::HashMap::new();
    for n in &layout.nodes {
        options.checkpoint_emit()?;
        node_uid_by_id.insert(n.id.clone(), next_generated_id("node-"));
        let _ = color_for(&n.id);
    }

    out.push_str(r#"<g class="nodes">"#);
    for n in &layout.nodes {
        options.checkpoint_emit()?;
        let node_uid = node_uid_by_id
            .get(&n.id)
            .cloned()
            .unwrap_or_else(|| SankeyNodeUid {
                diagram_id: scope_generated_ids.then_some(diagram_id),
                local_id: "node-0".to_string(),
            });
        let x = n.x0;
        let y = n.y0;
        let w = n.x1 - n.x0;
        let h = n.y1 - n.y0;
        let fill = color_for(&n.id);
        let _ = write!(
            &mut out,
            r#"<g class="node" id="{id}" transform="translate({x},{y})" x="{x}" y="{y}"><rect height="{h}" width="{w}" fill="{fill}"/></g>"#,
            id = node_uid,
            x = fmt(x),
            y = fmt(y),
            h = fmt(h),
            w = fmt(w),
            fill = escape_attr(&fill),
        );
    }
    out.push_str("</g>");

    let _ = write!(
        &mut out,
        r#"<g class="node-labels" font-size="{font_size}">"#,
        font_size = fmt(label_font_size)
    );
    let mut max_value = 0.0;
    let mut central_node_layer = 0usize;
    for n in &layout.nodes {
        options.checkpoint_emit()?;
        if n.value > max_value {
            max_value = n.value;
            central_node_layer = n.layer;
        }
    }

    let append_labels = |out: &mut String, class_name: Option<&str>| -> Result<()> {
        for (n, text) in layout.nodes.iter().zip(&label_texts) {
            options.checkpoint_emit()?;
            let y = (n.y0 + n.y1) / 2.0;
            let (x, anchor) = if outlined_labels {
                if n.layer < central_node_layer {
                    (n.x0 - label_gap_x, "end")
                } else {
                    (n.x1 + label_gap_x, "start")
                }
            } else if n.x0 < layout_width / 2.0 {
                (n.x1 + label_gap_x, "start")
            } else {
                (n.x0 - label_gap_x, "end")
            };
            let dy = if show_values {
                "0em".to_string()
            } else {
                format!("{}em", fmt(label_hide_values_dy_em))
            };
            let class_attr = class_name
                .map(|class_name| format!(r#" class="{}""#, escape_attr(class_name)))
                .unwrap_or_default();
            let _ = write!(
                out,
                r#"<text{class_attr} x="{x}" y="{y}" dy="{dy}" text-anchor="{anchor}">{text}</text>"#,
                class_attr = class_attr,
                x = fmt(x),
                y = fmt(y),
                dy = dy,
                anchor = anchor,
                text = escape_xml(text),
            );
        }
        Ok(())
    };
    if outlined_labels {
        append_labels(&mut out, Some("sankey-label-bg"))?;
        append_labels(&mut out, Some("sankey-label-fg"))?;
    } else {
        append_labels(&mut out, None)?;
    }
    out.push_str("</g>");

    out.push_str(r#"<g class="links" fill="none" stroke-opacity="0.5">"#);

    for l in &layout.links {
        options.checkpoint_emit()?;
        let source = layout
            .nodes
            .iter()
            .find(|n| n.id == l.source)
            .ok_or_else(|| Error::InvalidModel {
                message: format!("missing source node {}", l.source),
            })?;
        let target = layout
            .nodes
            .iter()
            .find(|n| n.id == l.target)
            .ok_or_else(|| Error::InvalidModel {
                message: format!("missing target node {}", l.target),
            })?;

        let sx = source.x1;
        let tx = target.x0;
        let mx = (sx + tx) / 2.0;
        let path_d = format!(
            "M{sx},{y0}C{mx},{y0},{mx},{y1},{tx},{y1}",
            sx = fmt(sx),
            y0 = fmt(l.y0),
            mx = fmt(mx),
            y1 = fmt(l.y1),
            tx = fmt(tx),
        );

        out.push_str(r#"<g class="link" style="mix-blend-mode: multiply;">"#);

        let stroke = match link_color.as_str() {
            "source" => color_for(&source.id),
            "target" => color_for(&target.id),
            "gradient" => {
                let gradient_id = next_generated_id("linearGradient-");
                let source_color = color_for(&source.id);
                let target_color = color_for(&target.id);
                let _ = write!(
                    &mut out,
                    r#"<linearGradient id="{id}" gradientUnits="userSpaceOnUse" x1="{x1}" x2="{x2}"><stop offset="0%" stop-color="{c1}"/><stop offset="100%" stop-color="{c2}"/></linearGradient>"#,
                    id = &gradient_id,
                    x1 = fmt(sx),
                    x2 = fmt(tx),
                    c1 = escape_attr(&source_color),
                    c2 = escape_attr(&target_color),
                );
                format!("url(#{})", gradient_id)
            }
            other => other.to_string(),
        };

        let stroke_width = l.width.max(1.0);
        let _ = write!(
            &mut out,
            r#"<path d="{d}" stroke="{stroke}" stroke-width="{sw}"/></g>"#,
            d = escape_xml(&path_d),
            stroke = escape_attr(&stroke),
            sw = fmt(stroke_width),
        );
    }

    out.push_str("</g>");
    out.push_str("</svg>");
    root_document.complete(out)
}
