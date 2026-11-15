//! Flowchart v2 basic geometry shapes.

use std::fmt::Write as _;

use crate::svg::parity::flowchart::{OptionalStyleAttr, escape_attr};
use crate::svg::parity::{fmt, fmt_display};

use super::super::geom::path_from_points;
use super::super::roughjs::roughjs_hachure_paths_for_svg_path;

const FLOWCHART_DIAMOND_HAND_DRAWN_ROUGHNESS: f32 = 0.7;
const FLOWCHART_DIAMOND_HAND_DRAWN_FILL_WEIGHT: f32 = 1.5;
const FLOWCHART_DIAMOND_HAND_DRAWN_HACHURE_GAP: f32 = 1.5;

pub(in crate::svg::parity::flowchart::render::node) fn render_diamond(
    out: &mut String,
    common: &super::super::FlowchartNodeRenderCommon<'_>,
    details: &mut crate::svg::parity::flowchart::types::FlowchartRenderDetails,
) {
    let w = common.layout_node.width.max(1.0);
    let h = common.layout_node.height.max(1.0);
    let tx = -w / 2.0 + 0.5;
    let ty = h / 2.0;

    let rough_paths = if common.look_is_hand_drawn() {
        // RoughJS normalizes this path to a left-edge hachure start. `roughr` is start-point
        // sensitive for this diamond, so feed the same polygon from the left vertex to keep the
        // hand-drawn silhouette centered instead of visually skewing into a parallelogram.
        let rough_points = [
            (0.0, -h / 2.0),
            (w / 2.0, 0.0),
            (w, -h / 2.0),
            (w / 2.0, -h),
        ];
        let path_data = path_from_points(&rough_points);
        super::super::helpers::timed_node_roughjs(common.timing, details, || {
            roughjs_hachure_paths_for_svg_path(
                &path_data,
                common.stroke_width,
                common.stroke_dasharray,
                FLOWCHART_DIAMOND_HAND_DRAWN_FILL_WEIGHT,
                FLOWCHART_DIAMOND_HAND_DRAWN_HACHURE_GAP,
                FLOWCHART_DIAMOND_HAND_DRAWN_ROUGHNESS,
                common.hand_drawn_seed,
            )
        })
    } else {
        None
    };

    if let Some((fill_d, stroke_d)) = rough_paths {
        let _ = write!(
            out,
            r#"<g transform="translate({},{})" style="{}"><path d="{}" stroke="{}" stroke-width="{}" fill="none" stroke-dasharray="0 0"/><path d="{}" stroke="{}" stroke-width="{}" fill="none" stroke-dasharray="{}"/></g>"#,
            fmt(tx),
            fmt(ty),
            escape_attr(common.rough_group_style),
            escape_attr(&fill_d),
            escape_attr(common.fill_color),
            fmt_display(FLOWCHART_DIAMOND_HAND_DRAWN_FILL_WEIGHT as f64),
            escape_attr(&stroke_d),
            escape_attr(common.stroke_color),
            fmt_display(common.stroke_width as f64),
            escape_attr(common.stroke_dasharray),
        );
        return;
    }

    let _ = write!(
        out,
        r#"<polygon points="{},0 {},{} {},{} 0,{}" class="label-container" transform="translate({},{})"{} />"#,
        fmt(w / 2.0),
        fmt(w),
        fmt(-h / 2.0),
        fmt(w / 2.0),
        fmt(-h),
        fmt(-h / 2.0),
        fmt(tx),
        fmt(ty),
        OptionalStyleAttr(common.style)
    );
}

pub(in crate::svg::parity::flowchart::render::node) fn render_circle(
    out: &mut String,
    common: &super::super::FlowchartNodeRenderCommon<'_>,
) {
    let w = common.layout_node.width.max(1.0);
    let h = common.layout_node.height.max(1.0);
    let r = (w.min(h) / 2.0).max(0.5);
    let _ = write!(
        out,
        r#"<circle class="basic label-container" style="{}" r="{}" cx="0" cy="0"/>"#,
        escape_attr(common.style),
        fmt(r),
    );
}

pub(in crate::svg::parity::flowchart::render::node) fn render_double_circle(
    out: &mut String,
    common: &super::super::FlowchartNodeRenderCommon<'_>,
) {
    let geometry = crate::flowchart::DoubleCircleGeometry::from_outer_diameter(
        common.layout_node.width.min(common.layout_node.height),
        common.look_is_neo(),
    );
    let _ = write!(
        out,
        r#"<g class="basic label-container" style="{}"><circle class="outer-circle" cx="0" cy="0" r="{}" style="{}"/><circle class="inner-circle" cx="0" cy="0" r="{}" style="{}"/></g>"#,
        escape_attr(common.style),
        fmt(geometry.outer_radius),
        escape_attr(common.style),
        fmt(geometry.inner_radius),
        escape_attr(common.style),
    );
}
