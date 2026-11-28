//! Flowchart v2 hexagon shape.

use std::fmt::Write as _;

use crate::svg::parity::flowchart::{OptionalStyleAttr, escape_attr};
use crate::svg::parity::fmt_display;

use super::super::geom::path_from_points;
use super::super::roughjs::roughjs_hachure_paths_for_svg_path;

const FLOWCHART_HEXAGON_HAND_DRAWN_ROUGHNESS: f32 = 0.7;
const FLOWCHART_HEXAGON_HAND_DRAWN_FILL_WEIGHT: f32 = 1.5;
const FLOWCHART_HEXAGON_HAND_DRAWN_HACHURE_GAP: f32 = 1.5;

pub(in crate::svg::parity::flowchart::render::node) fn render_hexagon(
    out: &mut String,
    common: &super::super::FlowchartNodeRenderCommon<'_>,
    details: &mut crate::svg::parity::flowchart::types::FlowchartRenderDetails,
) {
    let geometry = crate::flowchart::HexagonGeometry::from_bounds(
        common.layout_node.width,
        common.layout_node.height,
        common.look_is_neo(),
    );
    let w = geometry.width;
    let h = geometry.height;
    let path_data = path_from_points(&geometry.points);

    let rough_paths = if common.look_is_hand_drawn() {
        super::super::helpers::timed_node_roughjs(common.timing, details, || {
            roughjs_hachure_paths_for_svg_path(
                &path_data,
                common.stroke_width,
                common.stroke_dasharray,
                FLOWCHART_HEXAGON_HAND_DRAWN_FILL_WEIGHT,
                FLOWCHART_HEXAGON_HAND_DRAWN_HACHURE_GAP,
                FLOWCHART_HEXAGON_HAND_DRAWN_ROUGHNESS,
                common.hand_drawn_seed,
            )
        })
    } else {
        None
    };

    if let Some((fill_d, stroke_d)) = rough_paths {
        let _ = write!(
            out,
            r#"<g transform="translate({},{})" style="{}">"#,
            fmt_display(-w / 2.0),
            fmt_display(h / 2.0),
            escape_attr(common.rough_group_style)
        );
        let _ = write!(
            out,
            r#"<path d="{}" stroke="{}" stroke-width="{}" fill="none" stroke-dasharray="0 0"/>"#,
            escape_attr(&fill_d),
            escape_attr(common.fill_color),
            fmt_display(FLOWCHART_HEXAGON_HAND_DRAWN_FILL_WEIGHT as f64),
        );
        let _ = write!(
            out,
            r#"<path d="{}" stroke="{}" stroke-width="{}" fill="none" stroke-dasharray="{}" style="{}"/>"#,
            escape_attr(&stroke_d),
            escape_attr(common.stroke_color),
            fmt_display(common.stroke_width as f64),
            escape_attr(common.stroke_dasharray),
            escape_attr(common.style)
        );
        out.push_str("</g>");
    } else {
        let _ = write!(
            out,
            r#"<polygon points="{},{} {},{} {},{} {},{} {},{} {},{}" class="label-container" transform="translate({},{})"{} />"#,
            fmt_display(geometry.points[0].0),
            fmt_display(geometry.points[0].1),
            fmt_display(geometry.points[1].0),
            fmt_display(geometry.points[1].1),
            fmt_display(geometry.points[2].0),
            fmt_display(geometry.points[2].1),
            fmt_display(geometry.points[3].0),
            fmt_display(geometry.points[3].1),
            fmt_display(geometry.points[4].0),
            fmt_display(geometry.points[4].1),
            fmt_display(geometry.points[5].0),
            fmt_display(geometry.points[5].1),
            fmt_display(-w / 2.0),
            fmt_display(h / 2.0),
            OptionalStyleAttr(common.style)
        );
    }
}
