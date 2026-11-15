//! Flowchart cluster renderer.

use super::super::*;
use crate::svg::parity::flowchart::util::HTML_LABEL_FOREIGN_OBJECT_OVERFLOW_ATTR;

const FLOWCHART_CLUSTER_TITLE_WRAP_WIDTH: f64 = 200.0;
const FLOWCHART_CLUSTER_HAND_DRAWN_ROUGHNESS: f32 = 0.7;
const FLOWCHART_CLUSTER_HAND_DRAWN_FILL_WEIGHT: f32 = 3.0;
const FLOWCHART_CLUSTER_HAND_DRAWN_HACHURE_GAP: f32 = 1.5;

fn rounded_rect_path_d(x: f64, y: f64, w: f64, h: f64, r: f64) -> String {
    let mut out = String::new();
    let _ = write!(
        &mut out,
        "M {} {} H {} A {} {} 0 0 1 {} {} V {} A {} {} 0 0 1 {} {} H {} A {} {} 0 0 1 {} {} V {} A {} {} 0 0 1 {} {} Z",
        fmt_display(x + r),
        fmt_display(y),
        fmt_display(x + w - r),
        fmt_display(r),
        fmt_display(r),
        fmt_display(x + w),
        fmt_display(y + r),
        fmt_display(y + h - r),
        fmt_display(r),
        fmt_display(r),
        fmt_display(x + w - r),
        fmt_display(y + h),
        fmt_display(x + r),
        fmt_display(r),
        fmt_display(r),
        fmt_display(x),
        fmt_display(y + h - r),
        fmt_display(y + r),
        fmt_display(r),
        fmt_display(r),
        fmt_display(x + r),
        fmt_display(y),
    );
    out
}

fn rough_style_from_node_style(node_style: &str, mut keep: impl FnMut(&str) -> bool) -> String {
    let mut out = String::new();
    for decl in node_style.split(';') {
        let decl = decl.trim();
        let Some((key, _)) = decl.split_once(':') else {
            continue;
        };
        if !keep(key.trim()) {
            continue;
        }
        if !out.is_empty() {
            out.push(';');
        }
        out.push_str(decl);
    }
    out
}

fn cluster_rough_background_style(node_style: &str) -> String {
    rough_style_from_node_style(node_style, |key| key == "fill").replace("fill", "stroke")
}

fn cluster_rough_border_style(node_style: &str) -> String {
    rough_style_from_node_style(node_style, |key| key.contains("stroke"))
}

fn parse_css_px_f32(v: Option<&String>, fallback: f32) -> f32 {
    v.and_then(|raw| raw.trim_end_matches("px").trim().parse::<f32>().ok())
        .unwrap_or(fallback)
}

#[allow(clippy::too_many_arguments)]
fn write_flowchart_cluster_shape(
    out: &mut String,
    ctx: &FlowchartRenderCtx<'_>,
    compiled_styles: &FlowchartCompiledStyles,
    rect_style: &str,
    left: f64,
    top: f64,
    rect_w: f64,
    rect_h: f64,
) {
    if ctx.diagram_type == "agentflow" {
        let stroke = crate::svg::parity::util::theme_token(
            ctx.config.as_value(),
            "flowContainerStroke",
            &crate::svg::parity::util::theme_token(
                ctx.config.as_value(),
                "secondaryBorderColor",
                &PresentationTheme::new(ctx.config.as_value())
                    .node_diagram()
                    .cluster_border,
            ),
        );
        if flowchart_config_look(ctx.config) == "handDrawn" {
            let path = rounded_rect_path_d(left, top, rect_w, rect_h, 10.0);
            if let Some(stroke_d) =
                super::node::roughjs::roughjs_hand_drawn_stroke_path_for_svg_path(
                    &path,
                    FLOWCHART_CLUSTER_HAND_DRAWN_ROUGHNESS,
                    &ctx.hand_drawn_seed,
                )
            {
                let stroke_dasharray = compiled_styles.stroke_dasharray.as_deref().unwrap_or("0 0");
                let _ = write!(
                    out,
                    r#"<g><path d="{}" stroke="{}" stroke-width="0.75" fill="none" stroke-dasharray="{}"/></g>"#,
                    escape_xml_display(&stroke_d),
                    escape_xml_display(&stroke),
                    escape_xml_display(stroke_dasharray),
                );
                return;
            }
        }
        let _ = write!(
            out,
            r#"<rect rx="10" ry="10" x="{}" y="{}" width="{}" height="{}" fill="none" stroke="{}" stroke-width="0.75px"/>"#,
            fmt_display(left),
            fmt_display(top),
            fmt_display(rect_w),
            fmt_display(rect_h),
            escape_xml_display(&stroke),
        );
        return;
    }

    if flowchart_config_look(ctx.config) == "handDrawn" {
        let theme = PresentationTheme::new(ctx.config.as_value()).node_diagram();
        let fill = theme.cluster_bkg.as_str();
        let stroke = theme.cluster_border.as_str();
        let stroke_width = parse_css_px_f32(compiled_styles.stroke_width.as_ref(), 1.3);
        let stroke_dasharray = compiled_styles
            .stroke_dasharray
            .as_deref()
            .unwrap_or("0 0")
            .trim();
        let path = rounded_rect_path_d(left, top, rect_w, rect_h, 0.0);

        if let Some((fill_d, stroke_d)) = super::node::roughjs::roughjs_hachure_paths_for_svg_path(
            &path,
            stroke_width,
            stroke_dasharray,
            FLOWCHART_CLUSTER_HAND_DRAWN_FILL_WEIGHT,
            FLOWCHART_CLUSTER_HAND_DRAWN_HACHURE_GAP,
            FLOWCHART_CLUSTER_HAND_DRAWN_ROUGHNESS,
            &ctx.hand_drawn_seed,
        ) {
            let background_style = cluster_rough_background_style(rect_style);
            let border_style = cluster_rough_border_style(rect_style);
            let _ = write!(
                out,
                r#"<g><path d="{}" stroke="{}" stroke-width="{}" fill="none" stroke-dasharray="0 0"{} /><path d="{}" stroke="{}" stroke-width="{}" fill="none" stroke-dasharray="{}"{} /></g>"#,
                escape_xml_display(&fill_d),
                escape_xml_display(fill),
                fmt_display(FLOWCHART_CLUSTER_HAND_DRAWN_FILL_WEIGHT as f64),
                OptionalStyleXmlAttr(&background_style),
                escape_xml_display(&stroke_d),
                escape_xml_display(stroke),
                fmt_display(stroke_width as f64),
                escape_xml_display(stroke_dasharray),
                OptionalStyleXmlAttr(&border_style),
            );
            return;
        }
    }

    let _ = write!(
        out,
        r#"<rect style="{}" x="{}" y="{}" width="{}" height="{}"/>"#,
        escape_xml_display(rect_style),
        fmt_display(left),
        fmt_display(top),
        fmt_display(rect_w),
        fmt_display(rect_h)
    );
}

pub(in crate::svg::parity) fn render_flowchart_cluster(
    out: &mut String,
    ctx: &FlowchartRenderCtx<'_>,
    cluster: &LayoutCluster,
    origin_x: f64,
    origin_y: f64,
) -> crate::Result<()> {
    if let Some(lane) = ctx.swimlane_lanes_by_id.get(cluster.id.as_str())
        && lane.parent_id.is_none()
    {
        super::super::swimlane::render_swimlane_cluster(
            out, ctx, cluster, lane, origin_x, origin_y,
        )?;
        return Ok(());
    }

    let Some(sg) = ctx.subgraphs_by_id.get(cluster.id.as_str()) else {
        return Ok(());
    };
    let Some(subgraph_index) = ctx.subgraph_indices_by_id.get(cluster.id.as_str()).copied() else {
        return Ok(());
    };
    if !ctx.subgraph_has_children(cluster.id.as_str())
        && !super::flowchart_elk_renders_empty_subgraph_as_cluster(ctx)
    {
        return Ok(());
    }

    let (classes, styles) = ctx.model.effective_subgraph_css(subgraph_index, sg);
    let compiled_styles = flowchart_compile_styles(ctx.class_defs, classes, styles, &[]);
    let rect_style = compiled_styles.node_style.trim();
    let label_style = compiled_styles.label_style.trim();

    let left = (cluster.x - cluster.width / 2.0) + ctx.tx - origin_x;
    let top = (cluster.y - cluster.height / 2.0) + ctx.ty - origin_y;
    let rect_w = cluster.width.max(1.0);
    let rect_h = cluster.height.max(1.0);
    let label_top = top + cluster.title_margin_top.max(0.0);
    let cluster_dom_id = format!("{}-{}", ctx.diagram_id, cluster.id);
    ctx.checkpoint_emit()?;

    let label_type = sg.label_type.as_deref().unwrap_or("text");
    let render_title = ctx.model.subgraph_title_for_render(subgraph_index, sg);
    let title_text_style = crate::flowchart::flowchart_effective_text_style_for_classes(
        if ctx.edge_html_labels {
            &ctx.html_label_text_style
        } else {
            &ctx.text_style
        },
        ctx.class_defs,
        classes,
        styles,
    );
    // ELK paints Markdown after the final frame is known. clusters.js passes node.width to
    // createText, so these paint metrics differ from the wrapped pre-layout placeholder.
    let markdown_wrap_width = if ctx.uses_elk_adapter_dom {
        rect_w
    } else {
        FLOWCHART_CLUSTER_TITLE_WRAP_WIDTH
    };
    let painted_markdown = (ctx.uses_elk_adapter_dom && label_type == "markdown").then(|| {
        crate::flowchart::flowchart_label_metrics_for_layout(
            crate::flowchart::FlowchartLabelMetricsRequest {
                measurer: ctx.measurer,
                raw_label: render_title,
                label_type,
                style: title_text_style.as_ref(),
                max_width_px: Some(markdown_wrap_width),
                wrap_mode: ctx.edge_wrap_mode,
                config: ctx.config,
                math_renderer: ctx.math_renderer,
            },
        )
    });
    let label_w = painted_markdown
        .map_or(cluster.title_label.width, |metrics| metrics.width)
        .max(0.0);
    let label_h = painted_markdown
        .map_or(cluster.title_label.height, |metrics| metrics.height)
        .max(0.0);
    let label_left = left + rect_w / 2.0 - label_w / 2.0;

    let mut class_attr = String::new();
    for c in classes {
        let c = c.trim();
        if c.is_empty() {
            continue;
        }
        if !class_attr.is_empty() {
            class_attr.push(' ');
        }
        class_attr.push_str(c);
    }
    if !class_attr.is_empty() {
        class_attr.push(' ');
    }
    class_attr.push_str("cluster");
    if ctx.diagram_type == "agentflow" {
        class_attr.push_str(" flow-cluster");
    }
    let color_attr = super::super::agentflow::container_color_slot(ctx, &cluster.id)
        .map(|slot| format!(r#" data-color-id="color-{slot}""#))
        .unwrap_or_default();
    let data_look = flowchart_config_look(ctx.config);

    // Mermaid renders subgraph titles using the same `flowchart.htmlLabels` toggle as edge labels.
    if !ctx.edge_html_labels {
        let _ = write!(
            out,
            r#"<g class="{}" id="{}" data-look="{}"{color_attr}>"#,
            escape_xml_display(&class_attr),
            escape_xml_display(&cluster_dom_id),
            escape_xml_display(data_look),
        );
        write_flowchart_cluster_shape(
            out,
            ctx,
            &compiled_styles,
            rect_style,
            left,
            top,
            rect_w,
            rect_h,
        );
        let _ = write!(
            out,
            r#"<g class="cluster-label" transform="translate({},{})"><g><rect class="background" style="stroke: none"/>"#,
            fmt_display(label_left),
            fmt_display(label_top)
        );
        if label_type == "markdown" {
            if ctx.uses_elk_adapter_dom {
                write_flowchart_svg_text_markdown_wrapped(
                    out,
                    render_title,
                    true,
                    ctx.measurer,
                    title_text_style.as_ref(),
                    Some(markdown_wrap_width),
                );
            } else {
                write_flowchart_svg_text_markdown(out, render_title, true);
            }
        } else {
            let owner = ctx
                .svg_label_sidecar
                .and_then(|sidecar| sidecar.subgraph_title_owner(cluster.id.as_str()));
            let prepared = crate::flowchart::FlowchartSvgLabelRenderPlan::new(
                ctx.svg_label_sidecar,
                owner,
                render_title,
                ctx.measurer,
                title_text_style.as_ref(),
                None,
                true,
                crate::flowchart::FlowchartSvgWidthMode::Bbox,
            );
            let source_lines = prepared.wrapped_lines();
            write_flowchart_svg_source_word_lines(out, &source_lines, true);
        }
        out.push_str("</g></g></g>");
        return Ok(());
    }

    let title_html = flowchart_label_html(render_title, label_type, ctx.config, ctx.math_renderer);

    let span_style_attr = OptionalStyleXmlAttr(label_style);
    let markdown_uses_wrapped_box = if ctx.uses_elk_adapter_dom {
        (label_w - markdown_wrap_width).abs() < 1e-3
    } else {
        label_w >= markdown_wrap_width - 1e-3
    };
    let div_style = if label_type != "markdown" {
        "display: table-cell; white-space: nowrap; line-height: 1.5;".to_string()
    } else if markdown_uses_wrapped_box {
        format!(
            "display: table; white-space: break-spaces; line-height: 1.5; max-width: {mw}px; text-align: center; width: {mw}px;",
            mw = fmt_display(markdown_wrap_width)
        )
    } else {
        format!(
            "display: table-cell; white-space: nowrap; line-height: 1.5; max-width: {mw}px; text-align: center;",
            mw = fmt_display(markdown_wrap_width)
        )
    };

    let _ = write!(
        out,
        r#"<g class="{}" id="{}" data-look="{}"{color_attr}>"#,
        escape_xml_display(&class_attr),
        escape_xml_display(&cluster_dom_id),
        escape_xml_display(data_look),
    );
    write_flowchart_cluster_shape(
        out,
        ctx,
        &compiled_styles,
        rect_style,
        left,
        top,
        rect_w,
        rect_h,
    );
    let _ = write!(
        out,
        r#"<g class="cluster-label" transform="translate({},{})"><foreignObject width="{}" height="{}"{}><div xmlns="http://www.w3.org/1999/xhtml" style="{}"><span class="nodeLabel"{}>{}</span></div></foreignObject></g></g>"#,
        fmt_display(label_left),
        fmt_display(label_top),
        fmt_display(label_w),
        fmt_display(label_h),
        HTML_LABEL_FOREIGN_OBJECT_OVERFLOW_ATTR,
        escape_xml_display(&div_style),
        span_style_attr,
        title_html
    );
    Ok(())
}
