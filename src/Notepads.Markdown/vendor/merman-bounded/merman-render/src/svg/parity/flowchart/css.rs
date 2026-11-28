// Modifications Copyright (c) 2026, Jiaqi (0x7c13) Liu. See project LICENSE.
//! Flowchart CSS generation.

use super::*;

pub(in crate::svg::parity::flowchart) fn flowchart_css(
    diagram_id: SvgDiagramId<'_>,
    diagram_type: &str,
    effective_config: &serde_json::Value,
    font_family: &str,
    font_size: f64,
    presentation_policy: Option<crate::presentation::FlowchartPresentationPolicy>,
    emit: FlowchartEmitCheckpoint<'_>,
) -> Result<String> {
    flowchart_css_for_id(
        diagram_id,
        diagram_type,
        effective_config,
        font_family,
        font_size,
        presentation_policy,
        &|| emit.checkpoint(),
    )
}

fn flowchart_css_for_id(
    diagram_id: impl std::fmt::Display + Copy,
    diagram_type: &str,
    effective_config: &serde_json::Value,
    font_family: &str,
    font_size: f64,
    presentation_policy: Option<crate::presentation::FlowchartPresentationPolicy>,
    checkpoint: &dyn Fn() -> Result<()>,
) -> Result<String> {
    let title_class = title_css_class(diagram_type);
    let theme = PresentationTheme::new(effective_config).node_diagram();
    let stroke = theme.common.line_color.as_str();
    let arrowhead_color = theme.arrowhead_color.as_str();
    let node_border = theme.node_border.as_str();
    let main_bkg = theme.main_bkg.as_str();
    let text_color = theme.common.text_color.as_str();
    let node_text_color = theme.node_text_color.as_str();
    let title_color = theme.title_color.as_str();
    let stroke_width = theme.stroke_width.as_str();
    let error_bkg = theme.common.error_bkg.as_str();
    let error_text = theme.common.error_text.as_str();
    let edge_label_background = theme.edge_label_background.as_str();
    let tertiary = theme.tertiary.as_str();
    let cluster_bkg = theme.cluster_bkg.as_str();
    let cluster_border = theme.cluster_border.as_str();
    let tooltip_border = theme_token(effective_config, "border2", cluster_border);

    let label_bkg = css_rgba_fade(edge_label_background, 0.5)?;
    let id = diagram_id;
    let mut out = String::new();
    let _ = write!(
        &mut out,
        r#"#{}{{font-family:{};font-size:{}px;fill:{};}}"#,
        id,
        font_family,
        fmt(font_size),
        text_color
    );
    checkpoint()?;
    out.push_str(
        r#"@keyframes edge-animation-frame{from{stroke-dashoffset:0;}}@keyframes dash{to{stroke-dashoffset:0;}}"#,
    );
    let _ = write!(
        &mut out,
        r#"#{} .edge-animation-slow{{stroke-dasharray:9,5!important;stroke-dashoffset:900;animation:dash 50s linear infinite;stroke-linecap:round;}}#{} .edge-animation-fast{{stroke-dasharray:9,5!important;stroke-dashoffset:900;animation:dash 20s linear infinite;stroke-linecap:round;}}"#,
        id, id
    );
    checkpoint()?;
    let _ = write!(
        &mut out,
        r#"#{} .error-icon{{fill:{};}}#{} .error-text{{fill:{};stroke:{};}}"#,
        id, error_bkg, id, error_text, error_text
    );
    checkpoint()?;
    let _ = write!(
        &mut out,
        r#"#{} .edge-thickness-normal{{stroke-width:{}px;}}#{} .edge-thickness-thick{{stroke-width:3.5px;}}#{} .edge-pattern-solid{{stroke-dasharray:0;}}#{} .edge-thickness-invisible{{stroke-width:0;fill:none;}}#{} .edge-pattern-dashed{{stroke-dasharray:3;}}#{} .edge-pattern-dotted{{stroke-dasharray:2;}}"#,
        id, stroke_width, id, id, id, id, id
    );
    checkpoint()?;
    let _ = write!(
        &mut out,
        r#"#{} .marker{{fill:{};stroke:{};}}#{} .marker.cross{{stroke:{};}}"#,
        id, stroke, stroke, id, stroke
    );
    checkpoint()?;
    let _ = write!(
        &mut out,
        r#"#{} svg{{font-family:{};font-size:{}px;}}#{} p{{margin:0;}}"#,
        id,
        font_family,
        fmt(font_size),
        id
    );
    checkpoint()?;
    if diagram_type != "agentflow" {
        out.push_str(&super::agentflow::flowchart_container_css(
            diagram_id,
            effective_config,
            checkpoint,
        )?);
    }
    let _ = write!(
        &mut out,
        r#"#{} .label{{font-family:{};color:{};}}"#,
        id, font_family, node_text_color
    );
    checkpoint()?;
    let _ = write!(
        &mut out,
        r#"#{} .cluster-label text{{fill:{};}}#{} .cluster-label span{{color:{};}}#{} .cluster-label span p{{background-color:transparent;}}#{} .label text,#{} span{{fill:{};color:{};}}"#,
        id, title_color, id, title_color, id, id, id, node_text_color, node_text_color
    );
    checkpoint()?;
    let _ = write!(
        &mut out,
        r#"#{id} .node rect,#{id} .node circle,#{id} .node ellipse,#{id} .node polygon,#{id} .node path{{fill:{main_bkg};stroke:{node_border};stroke-width:{stroke_width}px;}}#{id} .rough-node .label text,#{id} .node .label text,#{id} .image-shape .label,#{id} .icon-shape .label{{text-anchor:middle;}}#{id} .node .katex path{{fill:#000;stroke:#000;stroke-width:1px;}}#{id} .rough-node .label,#{id} .node .label,#{id} .image-shape .label,#{id} .icon-shape .label{{text-align:center;}}#{id} .node.clickable{{cursor:pointer;}}"#
    );
    checkpoint()?;
    let _ = write!(
        &mut out,
        r#"#{} .root .anchor path{{fill:{}!important;stroke-width:0;stroke:{};}}#{} .arrowheadPath{{fill:{};}}#{} .edgePaths .path{{stroke:{};stroke-width:{}px;}}#{} .flowchart-link{{stroke:{};fill:none;}}"#,
        id, stroke, stroke, id, arrowhead_color, id, stroke, stroke_width, id, stroke
    );
    checkpoint()?;
    let _ = write!(
        &mut out,
        r#"#{} .edgeLabel{{background-color:{};text-align:center;}}#{} .edgeLabel p{{background-color:{};}}#{} .edgeLabel rect{{opacity:0.5;background-color:{};fill:{};}}#{} .labelBkg{{background-color:{};}}"#,
        id,
        edge_label_background,
        id,
        edge_label_background,
        id,
        edge_label_background,
        edge_label_background,
        id,
        label_bkg
    );
    checkpoint()?;
    let _ = write!(
        &mut out,
        r#"#{} .cluster rect{{fill:{};stroke:{};stroke-width:1px;}}#{} .cluster text{{fill:{};}}#{} .cluster span{{color:{};}}#{} .node .collapsed-indicator{{fill:{};stroke:none;opacity:0.6;}}#{} .node .collapsed-separator{{stroke:{};stroke-width:0.75px;}}#{} div.mermaidTooltip{{position:absolute;text-align:center;max-width:200px;padding:2px;font-family:{};font-size:12px;background:{};border:1px solid {};border-radius:2px;pointer-events:none;z-index:100;}}#{} .{title_class}{{text-anchor:middle;font-size:18px;fill:{};}}#{} rect.text{{fill:none;stroke-width:0;}}"#,
        diagram_id,
        cluster_bkg,
        cluster_border,
        diagram_id,
        title_color,
        diagram_id,
        title_color,
        diagram_id,
        cluster_border,
        diagram_id,
        cluster_border,
        diagram_id,
        font_family,
        tertiary,
        tooltip_border,
        diagram_id,
        text_color,
        diagram_id
    );
    checkpoint()?;
    let _ = write!(
        &mut out,
        r#"#{} .icon-shape,#{} .image-shape{{background-color:{};text-align:center;}}#{} .icon-shape p,#{} .image-shape p{{background-color:{};padding:2px;}}#{} .icon-shape .label rect,#{} .image-shape .label rect{{opacity:0.5;background-color:{};fill:{};}}#{} .label-icon{{display:inline-block;height:1em;overflow:visible;vertical-align:-0.125em;}}#{} .node .label-icon path{{fill:currentColor;stroke:revert;stroke-width:revert;}}"#,
        id,
        id,
        edge_label_background,
        id,
        id,
        edge_label_background,
        id,
        id,
        edge_label_background,
        edge_label_background,
        id,
        id,
    );
    crate::svg::parity::css::write_mermaid_common_neo_css(&mut out, id, effective_config);
    checkpoint()?;
    // This edge paint belongs to the typed Merman presentation policy; Mermaid's ordinary Neo
    // look keeps the common stylesheet unchanged.
    let _ = crate::svg::parity::css::write_mermaid_base_css_root_rule_to(
        &mut out,
        id,
        &crate::config::config_root_font_family_css(effective_config),
    );
    checkpoint()?;
    if presentation_policy.is_some() {
        let _ = write!(
            &mut out,
            r#"#{} .flowchart-link[data-look="neo"]{{stroke-linecap:round;stroke-linejoin:round;}}"#,
            id
        );
        checkpoint()?;
    }

    Ok(out)
}

// Writes the amplified class-definition component separately so callers can count and admit it
// before materializing repeated selectors.
pub(super) fn write_flowchart_class_defs_css<W: std::fmt::Write + ?Sized>(
    out: &mut W,
    diagram_id: impl std::fmt::Display + Copy,
    effective_config: &serde_json::Value,
    class_defs: &merman_core::native_collections::OrderedMap<String, Vec<String>>,
) -> std::fmt::Result {
    // Mermaid chooses different selectors based on htmlLabels.
    let html_labels =
        crate::flowchart::FlowchartConfigView::new(effective_config).effective_html_labels();
    let shape_elements: &[&str] = &["rect", "polygon", "ellipse", "circle", "path"];

    for (class, decls) in class_defs {
        if decls.is_empty() {
            continue;
        }
        let mut style = String::new();
        let mut text_color: Option<String> = None;
        for d in decls {
            let Some((k, v)) = parse_style_decl(d) else {
                continue;
            };
            let _ = write!(&mut style, "{}:{}!important;", k, v);
            if k == "color" {
                text_color = Some(v.to_string());
            }
        }
        if style.is_empty() {
            continue;
        }
        let escaped_class = escape_xml(class);
        if html_labels {
            write!(
                out,
                r#"#{} .{}&gt;*{{{}}}#{} .{} span{{{}}}"#,
                diagram_id, escaped_class, style, diagram_id, escaped_class, style
            )?;
        } else {
            for css_element in shape_elements {
                write!(
                    out,
                    r#"#{} .{} {}{{{}}}"#,
                    diagram_id, escaped_class, css_element, style
                )?;
            }
        }
        if let Some(c) = text_color.as_deref() {
            write!(
                out,
                r#"#{} .{} tspan{{fill:{}!important;}}"#,
                diagram_id,
                escaped_class,
                escape_xml(c)
            )?;
        }
    }
    Ok(())
}

#[inline]
pub(super) fn write_flowchart_edge_class_attr(out: &mut String, edge: &crate::flowchart::FlowEdge) {
    // Mermaid includes a 2-part class tuple (thickness/pattern) for flowchart edge paths. The
    // second tuple is `edge-thickness-normal edge-pattern-solid` in Mermaid@11.12.2 baselines,
    // even for dotted/thick strokes.
    let (thickness_1, pattern_1) = match edge.stroke.as_deref() {
        Some("thick") => ("edge-thickness-thick", "edge-pattern-solid"),
        Some("invisible") => ("edge-thickness-invisible", "edge-pattern-solid"),
        Some("dotted") => ("edge-thickness-normal", "edge-pattern-dotted"),
        _ => ("edge-thickness-normal", "edge-pattern-solid"),
    };

    if thickness_1 == "edge-thickness-invisible" {
        // Mermaid@11.12.2 does *not* include the second tuple nor `flowchart-link` for invisible
        // edges.
        out.push_str(thickness_1);
        out.push(' ');
        out.push_str(pattern_1);
        return;
    }

    out.push_str(thickness_1);
    out.push(' ');
    out.push_str(pattern_1);
    out.push_str(" edge-thickness-normal edge-pattern-solid flowchart-link");

    // Mermaid attaches animation classes directly on the edge path element when enabled via
    // edge-id `@{ ... }` blocks (e.g. `e1@{ animate: true }` or `e1@{ animation: fast }`).
    if edge.animate == Some(false) {
        return;
    }
    let animation_class = match edge.animation.as_deref() {
        Some("slow") => Some("edge-animation-slow"),
        Some(_) => Some("edge-animation-fast"),
        None => match edge.animate {
            Some(true) => Some("edge-animation-fast"),
            _ => None,
        },
    };
    if let Some(cls) = animation_class {
        out.push(' ');
        out.push_str(cls);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn khroma_named_edge_label_background_preserves_channels() {
        let css = flowchart_css_for_id(
            "theme_named_color",
            "flowchart-v2",
            &json!({
                "themeVariables": {
                    "edgeLabelBackground": "rebeccapurple"
                }
            }),
            "\"trebuchet ms\",verdana,arial,sans-serif",
            16.0,
            None,
            &|| Ok(()),
        )
        .expect("valid khroma color");

        assert!(
            css.contains("#theme_named_color .labelBkg{background-color:rgba(102, 51, 153, 0.5);}")
        );
    }

    #[test]
    fn unsupported_edge_label_background_returns_color_error() {
        let error = flowchart_css_for_id(
            "theme_unknown_color",
            "flowchart-v2",
            &json!({
                "themeVariables": {
                    "edgeLabelBackground": "not-a-css-color"
                }
            }),
            "\"trebuchet ms\",verdana,arial,sans-serif",
            16.0,
            None,
            &|| Ok(()),
        )
        .expect_err("unsupported khroma color must fail");

        assert!(error.to_string().contains("not-a-css-color"));
    }

    #[test]
    fn profile_policy_adds_neo_edge_line_style() {
        let css = flowchart_css_for_id(
            "profile_neo",
            "flowchart-v2",
            &json!({"look": "neo"}),
            "\"trebuchet ms\",verdana,arial,sans-serif",
            16.0,
            Some(crate::presentation::FlowchartPresentationPolicy::default()),
            &|| Ok(()),
        )
        .expect("valid profile CSS");

        assert!(css.contains(
            r#".flowchart-link[data-look="neo"]{stroke-linecap:round;stroke-linejoin:round;}"#
        ));
    }

    #[test]
    fn ordinary_neo_without_profile_keeps_mermaid_common_css_only() {
        let css = flowchart_css_for_id(
            "ordinary_neo",
            "flowchart-v2",
            &json!({"look": "neo"}),
            "\"trebuchet ms\",verdana,arial,sans-serif",
            16.0,
            None,
            &|| Ok(()),
        )
        .expect("valid Mermaid CSS");

        assert!(!css.contains(
            r#".flowchart-link[data-look="neo"]{stroke-linecap:round;stroke-linejoin:round;}"#
        ));
    }
}
