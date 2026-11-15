//! Mermaid 11.16 bang and cloud path renderers.

use std::fmt::Write as _;

use crate::flowchart::{OrganicShapeGeometry, RelativeArc};
use crate::svg::parity::flowchart::escape_attr;
use crate::svg::parity::{fmt, fmt_display};

const HAND_DRAWN_ROUGHNESS: f32 = 0.7;
const HAND_DRAWN_FILL_WEIGHT: f32 = 1.5;
const HAND_DRAWN_HACHURE_GAP: f32 = 1.5;

fn path_data(arcs: &[RelativeArc]) -> String {
    let mut path = String::from("M0 0\n");
    for arc in arcs {
        let _ = writeln!(
            path,
            "a{},{} {} {},{} {},{}",
            fmt(arc.rx),
            fmt(arc.ry),
            fmt(arc.x_axis_rotation_deg),
            u8::from(arc.large_arc),
            u8::from(arc.sweep),
            fmt(arc.dx),
            fmt(arc.dy),
        );
    }
    path.push_str("H0 V0 Z");
    path
}

fn render_organic_shape(
    out: &mut String,
    common: &super::super::FlowchartNodeRenderCommon<'_>,
    geometry: &OrganicShapeGeometry,
    details: &mut crate::svg::parity::flowchart::types::FlowchartRenderDetails,
) {
    let path = path_data(&geometry.arcs);
    if common.look_is_hand_drawn()
        && let Some((fill_d, stroke_d)) =
            super::super::helpers::timed_node_roughjs(common.timing, details, || {
                super::super::roughjs::roughjs_hachure_paths_for_svg_path(
                    &path,
                    common.stroke_width,
                    common.stroke_dasharray,
                    HAND_DRAWN_FILL_WEIGHT,
                    HAND_DRAWN_HACHURE_GAP,
                    HAND_DRAWN_ROUGHNESS,
                    common.hand_drawn_seed,
                )
            })
    {
        let _ = write!(
            out,
            r#"<g class="basic label-container" style="{}" transform="translate({},{})"><path d="{}" stroke="{}" stroke-width="{}" fill="none" stroke-dasharray="0 0"/><path d="{}" stroke="{}" stroke-width="{}" fill="none" stroke-dasharray="{}"/></g>"#,
            escape_attr(common.rough_group_style),
            fmt(geometry.translate_x),
            fmt(geometry.translate_y),
            escape_attr(&fill_d),
            escape_attr(common.fill_color),
            fmt_display(HAND_DRAWN_FILL_WEIGHT as f64),
            escape_attr(&stroke_d),
            escape_attr(common.stroke_color),
            fmt_display(common.stroke_width as f64),
            escape_attr(common.stroke_dasharray),
        );
        return;
    }

    let _ = write!(
        out,
        r#"<path class="basic label-container" style="{}" d="{}" transform="translate({},{})"/>"#,
        escape_attr(common.style),
        escape_attr(&path),
        fmt(geometry.translate_x),
        fmt(geometry.translate_y),
    );
}

pub(in crate::svg::parity::flowchart::render::node) fn render_bang(
    out: &mut String,
    ctx: &crate::svg::parity::flowchart::types::FlowchartRenderCtx<'_>,
    common: &super::super::FlowchartNodeRenderCommon<'_>,
    label: &mut super::super::FlowchartNodeLabelState<'_>,
    details: &mut crate::svg::parity::flowchart::types::FlowchartRenderDetails,
) {
    let label_width = common.layout_node.label_width.unwrap_or(0.0).max(0.0);
    let label_height = common.layout_node.label_height.unwrap_or(0.0).max(0.0);
    let geometry = crate::flowchart::bang_geometry(label_width, label_height, ctx.node_padding);
    if !ctx.node_html_labels {
        label.dx = -label_width / 2.0;
    }
    render_organic_shape(out, common, &geometry, details);
}

pub(in crate::svg::parity::flowchart::render::node) fn render_cloud(
    out: &mut String,
    ctx: &crate::svg::parity::flowchart::types::FlowchartRenderCtx<'_>,
    common: &super::super::FlowchartNodeRenderCommon<'_>,
    label: &mut super::super::FlowchartNodeLabelState<'_>,
    details: &mut crate::svg::parity::flowchart::types::FlowchartRenderDetails,
) {
    let label_width = common.layout_node.label_width.unwrap_or(0.0).max(0.0);
    let label_height = common.layout_node.label_height.unwrap_or(0.0).max(0.0);
    let geometry = crate::flowchart::cloud_geometry(label_width, label_height, ctx.node_padding);
    if !ctx.node_html_labels {
        label.dx = -label_width / 2.0;
    }
    render_organic_shape(out, common, &geometry, details);
}
