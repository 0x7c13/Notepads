//! Flowchart v2 lean/trapezoid polygon shapes.

use std::fmt::Write as _;

use crate::svg::parity::flowchart::{OptionalStyleAttr, escape_attr};
use crate::svg::parity::fmt_display;

use super::super::geom::path_from_points;
use super::super::roughjs::roughjs_hachure_paths_for_svg_path;

const FLOWCHART_POLYGON_HAND_DRAWN_ROUGHNESS: f32 = 0.7;
const FLOWCHART_POLYGON_HAND_DRAWN_FILL_WEIGHT: f32 = 1.5;
const FLOWCHART_POLYGON_HAND_DRAWN_HACHURE_GAP: f32 = 1.5;

fn render_polygon_shape(
    out: &mut String,
    common: &super::super::FlowchartNodeRenderCommon<'_>,
    details: &mut crate::svg::parity::flowchart::types::FlowchartRenderDetails,
    pts: &[(f64, f64)],
    tx: f64,
    ty: f64,
) {
    if common.look_is_hand_drawn() {
        let path_data = path_from_points(pts);
        if let Some((fill_d, stroke_d)) =
            super::super::helpers::timed_node_roughjs(common.timing, details, || {
                roughjs_hachure_paths_for_svg_path(
                    &path_data,
                    common.stroke_width,
                    common.stroke_dasharray,
                    FLOWCHART_POLYGON_HAND_DRAWN_FILL_WEIGHT,
                    FLOWCHART_POLYGON_HAND_DRAWN_HACHURE_GAP,
                    FLOWCHART_POLYGON_HAND_DRAWN_ROUGHNESS,
                    common.hand_drawn_seed,
                )
            })
        {
            let _ = write!(
                out,
                r#"<g transform="translate({},{})" style="{}"><path d="{}" stroke="{}" stroke-width="{}" fill="none" stroke-dasharray="0 0"/><path d="{}" stroke="{}" stroke-width="{}" fill="none" stroke-dasharray="{}"/></g>"#,
                fmt_display(tx),
                fmt_display(ty),
                escape_attr(common.rough_group_style),
                escape_attr(&fill_d),
                escape_attr(common.fill_color),
                fmt_display(FLOWCHART_POLYGON_HAND_DRAWN_FILL_WEIGHT as f64),
                escape_attr(&stroke_d),
                escape_attr(common.stroke_color),
                fmt_display(common.stroke_width as f64),
                escape_attr(common.stroke_dasharray),
            );
            return;
        }
    }

    let mut points_attr = String::new();
    for (idx, (px, py)) in pts.iter().copied().enumerate() {
        if idx > 0 {
            points_attr.push(' ');
        }
        let _ = write!(&mut points_attr, "{},{}", fmt_display(px), fmt_display(py));
    }
    let _ = write!(
        out,
        r#"<polygon points="{}" class="label-container" transform="translate({},{})"{} />"#,
        points_attr,
        fmt_display(tx),
        fmt_display(ty),
        OptionalStyleAttr(common.style)
    );
}

pub(in crate::svg::parity::flowchart::render::node) fn render_lean_right(
    out: &mut String,
    common: &super::super::FlowchartNodeRenderCommon<'_>,
    details: &mut crate::svg::parity::flowchart::types::FlowchartRenderDetails,
) {
    let geometry = crate::flowchart::LeanGeometry::from_bounds(
        crate::flowchart::LeanKind::Right,
        common.layout_node.width,
        common.layout_node.height,
    );
    render_polygon_shape(
        out,
        common,
        details,
        &geometry.points,
        geometry.translate_x,
        geometry.translate_y,
    );
}

pub(in crate::svg::parity::flowchart::render::node) fn render_lean_left(
    out: &mut String,
    common: &super::super::FlowchartNodeRenderCommon<'_>,
    details: &mut crate::svg::parity::flowchart::types::FlowchartRenderDetails,
) {
    let geometry = crate::flowchart::LeanGeometry::from_bounds(
        crate::flowchart::LeanKind::Left,
        common.layout_node.width,
        common.layout_node.height,
    );
    render_polygon_shape(
        out,
        common,
        details,
        &geometry.points,
        geometry.translate_x,
        geometry.translate_y,
    );
}

pub(in crate::svg::parity::flowchart::render::node) fn render_trapezoid(
    out: &mut String,
    common: &super::super::FlowchartNodeRenderCommon<'_>,
    details: &mut crate::svg::parity::flowchart::types::FlowchartRenderDetails,
) {
    let geometry = crate::flowchart::LeanGeometry::from_bounds(
        crate::flowchart::LeanKind::Trapezoid,
        common.layout_node.width,
        common.layout_node.height,
    );
    render_polygon_shape(
        out,
        common,
        details,
        &geometry.points,
        geometry.translate_x,
        geometry.translate_y,
    );
}

pub(in crate::svg::parity::flowchart::render::node) fn render_inv_trapezoid(
    out: &mut String,
    common: &super::super::FlowchartNodeRenderCommon<'_>,
    details: &mut crate::svg::parity::flowchart::types::FlowchartRenderDetails,
) {
    let geometry = crate::flowchart::LeanGeometry::from_bounds(
        crate::flowchart::LeanKind::InvertedTrapezoid,
        common.layout_node.width,
        common.layout_node.height,
    );
    render_polygon_shape(
        out,
        common,
        details,
        &geometry.points,
        geometry.translate_x,
        geometry.translate_y,
    );
}
