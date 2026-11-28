//! Mermaid 11.17 collapsed Flowchart subgraph shape.

use std::fmt::Write as _;

use crate::svg::parity::flowchart::escape_attr;
use crate::svg::parity::{fmt, fmt_display};

use super::super::helpers;
use super::super::roughjs::roughjs_paths_for_svg_path_single_set;

const INDICATOR_ROW_HEIGHT: f64 = 20.0;
const SEPARATOR_GAP: f64 = 8.0;
const MIN_WIDTH: f64 = 80.0;
const RADIUS: f64 = 8.0;

pub(in crate::svg::parity::flowchart::render::node) struct CollapsedGroupGeometry {
    left: f64,
    width: f64,
    separator_y: f64,
    border: String,
}

fn rounded_rect_path_d(x: f64, y: f64, width: f64, height: f64, radius: f64) -> String {
    let radius = radius.min(width / 2.0).min(height / 2.0).max(0.0);
    format!(
        "M {} {} H {} A {} {} 0 0 1 {} {} V {} A {} {} 0 0 1 {} {} H {} A {} {} 0 0 1 {} {} V {} A {} {} 0 0 1 {} {} Z",
        fmt(x + radius),
        fmt(y),
        fmt(x + width - radius),
        fmt(radius),
        fmt(radius),
        fmt(x + width),
        fmt(y + radius),
        fmt(y + height - radius),
        fmt(radius),
        fmt(radius),
        fmt(x + width - radius),
        fmt(y + height),
        fmt(x + radius),
        fmt(radius),
        fmt(radius),
        fmt(x),
        fmt(y + height - radius),
        fmt(y + radius),
        fmt(radius),
        fmt(radius),
        fmt(x + radius),
        fmt(y),
    )
}

pub(in crate::svg::parity::flowchart::render::node) fn render_collapsed_group_body(
    out: &mut String,
    ctx: &crate::svg::parity::flowchart::types::FlowchartRenderCtx<'_>,
    common: &super::super::FlowchartNodeRenderCommon<'_>,
    label: &mut super::super::FlowchartNodeLabelState<'_>,
    details: &mut crate::svg::parity::flowchart::types::FlowchartRenderDetails,
) -> CollapsedGroupGeometry {
    let metrics = helpers::compute_node_label_metrics(
        ctx,
        Some(common.layout_node),
        label.text,
        label.label_type,
        common.node_classes,
        common.node_styles,
    );
    let padding = 8.0;
    let width = (metrics.width + padding * 2.0)
        .max(MIN_WIDTH)
        .max(common.layout_node.width.max(0.0));
    let height = (metrics.height + SEPARATOR_GAP + INDICATOR_ROW_HEIGHT + padding * 2.0)
        .max(common.layout_node.height.max(0.0));
    let left = -width / 2.0;
    let top = -height / 2.0;
    let is_agentflow_flow = ctx.diagram_type == "agentflow";
    let (radius, fill, border, stroke_width, class) = if is_agentflow_flow {
        (
            10.0,
            "none".to_owned(),
            crate::svg::parity::util::theme_token(
                ctx.config.as_value(),
                "flowContainerStroke",
                &crate::svg::parity::util::theme_token(
                    ctx.config.as_value(),
                    "secondaryBorderColor",
                    "#aaaa33",
                ),
            ),
            0.75,
            "flow-collapsed",
        )
    } else {
        (
            RADIUS,
            crate::svg::parity::util::theme_token(ctx.config.as_value(), "clusterBkg", "#ffffde"),
            crate::svg::parity::util::theme_token(
                ctx.config.as_value(),
                "clusterBorder",
                "#aaaa33",
            ),
            common.stroke_width as f64,
            "collapsed-group",
        )
    };

    if common.look_is_hand_drawn() {
        let path = rounded_rect_path_d(left, top, width, height, radius);
        let effective_stroke_width = if is_agentflow_flow {
            stroke_width as f32
        } else {
            common.stroke_width
        };
        if let Some((fill_d, stroke_d)) =
            helpers::timed_node_roughjs(common.timing, details, || {
                roughjs_paths_for_svg_path_single_set(
                    &path,
                    effective_stroke_width,
                    common.stroke_dasharray,
                    common.hand_drawn_seed,
                )
            })
        {
            let _ = write!(
                out,
                r#"<g class="basic label-container {class}"><path d="{}" stroke="none" stroke-width="0" fill="{}" style="{}"/><path d="{}" stroke="{}" stroke-width="{}" fill="none" stroke-dasharray="{}" style="{}"/></g>"#,
                escape_attr(&fill_d),
                escape_attr(&fill),
                escape_attr(common.style),
                escape_attr(&stroke_d),
                escape_attr(&border),
                fmt_display(effective_stroke_width as f64),
                escape_attr(common.stroke_dasharray),
                escape_attr(common.style),
            );
        } else {
            let _ = write!(
                out,
                r#"<path class="basic label-container {class}" d="{}" style="{}"/>"#,
                escape_attr(&path),
                escape_attr(common.style),
            );
        }
    } else {
        let _ = write!(
            out,
            r#"<rect class="basic label-container {class}" style="{}" rx="{}" ry="{}" x="{}" y="{}" width="{}" height="{}" fill="{}" stroke="{}"{} />"#,
            escape_attr(common.style),
            fmt(radius),
            fmt(radius),
            fmt(left),
            fmt(top),
            fmt(width),
            fmt(height),
            escape_attr(&fill),
            escape_attr(&border),
            if is_agentflow_flow {
                format!(r#" stroke-width="{}px""#, fmt(stroke_width))
            } else {
                String::new()
            },
        );
    }

    let separator_y = top + padding + metrics.height + SEPARATOR_GAP;
    label.dy = -(SEPARATOR_GAP + INDICATOR_ROW_HEIGHT) / 2.0;

    CollapsedGroupGeometry {
        left,
        width,
        separator_y,
        border,
    }
}

pub(in crate::svg::parity::flowchart::render::node) fn render_collapsed_group_indicators(
    out: &mut String,
    geometry: CollapsedGroupGeometry,
) {
    let CollapsedGroupGeometry {
        left,
        width,
        separator_y,
        border,
    } = geometry;
    let _ = write!(
        out,
        r#"<line class="collapsed-separator" x1="{}" y1="{}" x2="{}" y2="{}" stroke="{}" stroke-dasharray="3, 3"/>"#,
        fmt(left + 8.0),
        fmt(separator_y),
        fmt(left + width - 8.0),
        fmt(separator_y),
        escape_attr(&border),
    );
    let dot_y = separator_y + INDICATOR_ROW_HEIGHT / 2.0;
    for x in [-10.0, 0.0, 10.0] {
        let _ = write!(
            out,
            r#"<circle class="collapsed-indicator" cx="{}" cy="{}" r="2.5" fill="{}"/>"#,
            fmt(x),
            fmt(dot_y),
            escape_attr(&border),
        );
    }
}
