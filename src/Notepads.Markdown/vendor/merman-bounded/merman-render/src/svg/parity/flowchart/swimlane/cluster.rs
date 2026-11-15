use super::super::*;
use crate::flowchart::{
    FlowchartLabelMetricsRequest, flowchart_label_is_empty_for_render,
    flowchart_label_metrics_for_layout,
};
use crate::model::{SwimlaneDirection, SwimlaneLaneLayout};

const SWIMLANE_HAND_DRAWN_ROUGHNESS: f32 = 0.7;
const SWIMLANE_HAND_DRAWN_FILL_WEIGHT: f32 = 3.0;
const SWIMLANE_HAND_DRAWN_HACHURE_GAP: f32 = 1.5;

fn rough_style_from_node_style(node_style: &str, mut keep: impl FnMut(&str) -> bool) -> String {
    let mut out = String::new();
    for declaration in node_style.split(';') {
        let declaration = declaration.trim();
        let Some((key, _)) = declaration.split_once(':') else {
            continue;
        };
        if !keep(key.trim()) {
            continue;
        }
        if !out.is_empty() {
            out.push(';');
        }
        out.push_str(declaration);
    }
    out
}

fn parse_css_px_f32(value: Option<&String>, fallback: f32) -> f32 {
    value
        .and_then(|raw| raw.trim_end_matches("px").trim().parse::<f32>().ok())
        .unwrap_or(fallback)
}

#[allow(clippy::too_many_arguments)]
fn write_swimlane_rect(
    out: &mut String,
    ctx: &FlowchartRenderCtx<'_>,
    compiled: &FlowchartCompiledStyles,
    class_name: &str,
    node_style: &str,
    x: f64,
    y: f64,
    width: f64,
    height: f64,
    fill: Option<&str>,
    stroke: &str,
) {
    if flowchart_config_look(ctx.config) == "handDrawn" {
        let stroke_width = parse_css_px_f32(compiled.stroke_width.as_ref(), 1.3);
        let stroke_dasharray = compiled.stroke_dasharray.as_deref().unwrap_or("0 0").trim();
        // RoughJS creates the outline before the fill. Generating both paths
        // and omitting the fill path for the body therefore preserves the
        // exact seeded outline used by `fill: none` upstream.
        if let Some((fill_d, stroke_d)) =
            super::super::render::node::roughjs::roughjs_hachure_paths_for_rect(
                x,
                y,
                width,
                height,
                stroke_width,
                stroke_dasharray,
                SWIMLANE_HAND_DRAWN_FILL_WEIGHT,
                SWIMLANE_HAND_DRAWN_HACHURE_GAP,
                SWIMLANE_HAND_DRAWN_ROUGHNESS,
                &ctx.hand_drawn_seed,
            )
        {
            out.push_str("<g>");
            if let Some(fill) = fill {
                let background_style = rough_style_from_node_style(node_style, |key| key == "fill")
                    .replace("fill", "stroke");
                let _ = write!(
                    out,
                    r#"<path d="{}" stroke="{}" stroke-width="{}" fill="none" stroke-dasharray="0 0"{} />"#,
                    escape_xml_display(&fill_d),
                    escape_xml_display(fill),
                    fmt_display(SWIMLANE_HAND_DRAWN_FILL_WEIGHT as f64),
                    OptionalStyleXmlAttr(&background_style),
                );
            }
            let border_style =
                rough_style_from_node_style(node_style, |key| key.contains("stroke"));
            let _ = write!(
                out,
                r#"<path d="{}" stroke="{}" stroke-width="{}" fill="none" stroke-dasharray="{}"{} /></g>"#,
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
        r#"<rect class="{}" style="{}" x="{}" y="{}" width="{}" height="{}" fill="{}" stroke="{}"/>"#,
        escape_xml_display(class_name),
        escape_xml_display(node_style),
        fmt_display(x),
        fmt_display(y),
        fmt_display(width),
        fmt_display(height),
        escape_xml_display(fill.unwrap_or("none")),
        escape_xml_display(stroke),
    );
}

fn lane_label_metrics(
    ctx: &FlowchartRenderCtx<'_>,
    lane: &SwimlaneLaneLayout,
    render_title: &str,
) -> crate::text::TextMetrics {
    if flowchart_label_is_empty_for_render(render_title) {
        return crate::text::TextMetrics {
            width: 0.0,
            height: 0.0,
            line_count: 0,
        };
    }

    let style = if ctx.swimlane_title_html_labels {
        &ctx.html_label_text_style
    } else {
        &ctx.text_style
    };
    let wrap_mode = if ctx.swimlane_title_html_labels {
        WrapMode::HtmlLike
    } else {
        WrapMode::SvgLike
    };
    flowchart_label_metrics_for_layout(FlowchartLabelMetricsRequest {
        measurer: ctx.measurer,
        raw_label: render_title,
        // Mermaid's dedicated Swimlane renderer omits createText's `markdown` option, so its
        // default `true` applies independently of FlowDB's public subgraph labelType.
        label_type: "markdown",
        style,
        max_width_px: Some(lane.width.max(1.0)),
        wrap_mode,
        config: ctx.config,
        math_renderer: ctx.math_renderer,
    })
}

pub(in crate::svg::parity::flowchart) fn render_swimlane_cluster(
    out: &mut String,
    ctx: &FlowchartRenderCtx<'_>,
    cluster: &LayoutCluster,
    lane: &SwimlaneLaneLayout,
    origin_x: f64,
    origin_y: f64,
) -> crate::Result<()> {
    ctx.checkpoint_emit()?;
    let subgraph = ctx.subgraphs_by_id.get(cluster.id.as_str()).copied();
    let subgraph_index = ctx.subgraph_indices_by_id.get(cluster.id.as_str()).copied();
    let (class_names, styles) = subgraph
        .zip(subgraph_index)
        .map(|(subgraph, subgraph_index)| {
            ctx.model.effective_subgraph_css(subgraph_index, subgraph)
        })
        .unwrap_or_default();
    let compiled = flowchart_compile_styles(ctx.class_defs, class_names, styles, &[]);
    let node_style = compiled.node_style.trim();
    let label_style = compiled.label_style.trim();
    let render_title =
        subgraph
            .zip(subgraph_index)
            .map_or(lane.title.as_str(), |(subgraph, subgraph_index)| {
                ctx.model
                    .subgraph_title_for_render(subgraph_index, subgraph)
            });
    let label_metrics = lane_label_metrics(ctx, lane, render_title);
    let label_width = label_metrics.width.max(0.0);
    let label_height = label_metrics.height.max(0.0);

    let padding = lane.padding.max(0.0);
    let width = lane.width.max(label_width + padding);
    let height = lane.height.max(0.0);
    let lane_top = lane.y - height / 2.0 + ctx.ty - origin_y;
    let lane_bottom = lane.y + height / 2.0 + ctx.ty - origin_y;
    let lane_left = lane.x - width / 2.0 + ctx.tx - origin_x;
    let content_top = lane
        .content_top
        .map(|value| value + ctx.ty - origin_y)
        .unwrap_or(lane_top + height / 3.0);
    let is_lr = ctx.swimlane_direction == Some(SwimlaneDirection::Lr);
    let title_padding_y = if is_lr { 4.0 } else { 0.0 };
    let desired_title_size = label_height + 2.0 * title_padding_y;

    let theme = PresentationTheme::new(ctx.config.as_value()).node_diagram();
    let mut classes = String::from("cluster swimlane");
    for class in class_names {
        let class = class.trim();
        if !class.is_empty() {
            classes.push(' ');
            classes.push_str(class);
        }
    }
    let _ = write!(
        out,
        r#"<g class="{}" id="{}" data-id="{}" data-et="cluster""#,
        escape_xml_display(&classes),
        escape_xml_display(&lane.id),
        escape_xml_display(&lane.id),
    );
    let data_look = flowchart_config_look(ctx.config);
    let color_slot = super::super::agentflow::container_color_slot(ctx, &lane.id).or_else(|| {
        (subgraph.is_none()
            && super::super::agentflow::flowchart_palette_len(ctx.config.as_value()) > 0)
            .then_some(0)
    });
    let _ = write!(out, r#" data-look="{}""#, escape_xml_display(data_look));
    if let Some(color_slot) = color_slot {
        let _ = write!(out, r#" data-color-id="color-{color_slot}""#);
    }
    out.push('>');

    let (label_x, label_y, label_transform) = if is_lr {
        let title_width = desired_title_size.max(label_height + 2.0 * title_padding_y);
        let body_x = lane_left + title_width;
        let body_width = (width - title_width).max(0.0);
        write_swimlane_rect(
            out,
            ctx,
            &compiled,
            "swimlane-body",
            node_style,
            body_x,
            lane_top,
            body_width,
            height,
            None,
            &theme.cluster_border,
        );
        write_swimlane_rect(
            out,
            ctx,
            &compiled,
            "swimlane-title",
            node_style,
            lane_left,
            lane_top,
            title_width,
            height,
            Some(&theme.cluster_bkg),
            &theme.cluster_border,
        );
        let center_x = lane_left + title_width / 2.0;
        let center_y = lane.y + ctx.ty - origin_y;
        (
            0.0,
            0.0,
            format!(
                "translate({}, {}) rotate(-90) translate({}, {})",
                fmt_display(center_x),
                fmt_display(center_y),
                fmt_display(-label_width / 2.0),
                fmt_display(-label_height / 2.0),
            ),
        )
    } else {
        let header_max_height = (content_top - lane_top).max(0.0);
        let title_height = desired_title_size.min(header_max_height);
        let body_y = lane_top + title_height;
        let body_height = (lane_bottom - body_y).max(0.0);
        write_swimlane_rect(
            out,
            ctx,
            &compiled,
            "swimlane-body",
            node_style,
            lane_left,
            body_y,
            width,
            body_height,
            None,
            &theme.cluster_border,
        );
        write_swimlane_rect(
            out,
            ctx,
            &compiled,
            "swimlane-title",
            node_style,
            lane_left,
            lane_top,
            width,
            title_height,
            Some(&theme.cluster_bkg),
            &theme.cluster_border,
        );
        (
            lane.x - label_width / 2.0 + ctx.tx - origin_x,
            lane_top + (title_height - label_height) / 2.0,
            String::new(),
        )
    };

    if ctx.swimlane_title_html_labels {
        let title_html =
            flowchart_label_html(render_title, "markdown", ctx.config, ctx.math_renderer);
        let transform = if is_lr {
            label_transform
        } else {
            format!(
                "translate({}, {})",
                fmt_display(label_x),
                fmt_display(label_y)
            )
        };
        let div_style = format!(
            "display: table-cell; white-space: nowrap; line-height: 1.5; max-width: {}px; text-align: center;",
            fmt_display(width),
        );
        let _ = write!(
            out,
            r#"<g class="cluster-label swimlane-label" transform="{}"><foreignObject width="{}" height="{}"><div xmlns="http://www.w3.org/1999/xhtml" style="{}"><span class="nodeLabel"{}>{}</span></div></foreignObject></g>"#,
            escape_xml_display(&transform),
            fmt_display(label_width),
            fmt_display(label_height),
            escape_xml_display(&div_style),
            OptionalStyleXmlAttr(label_style),
            title_html,
        );
    } else {
        let transform = if is_lr {
            label_transform
        } else {
            format!(
                "translate({}, {})",
                fmt_display(label_x),
                fmt_display(label_y)
            )
        };
        let _ = write!(
            out,
            r#"<g class="cluster-label swimlane-label" transform="{}"><g><rect class="background" style="stroke: none"/>"#,
            escape_xml_display(&transform),
        );
        write_flowchart_svg_text_markdown(out, render_title, true);
        out.push_str("</g></g>");
    }
    out.push_str("</g>");
    Ok(())
}
