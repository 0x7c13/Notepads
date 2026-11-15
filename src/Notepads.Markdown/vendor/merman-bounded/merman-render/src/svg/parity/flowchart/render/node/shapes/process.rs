//! Flowchart process rectangle.

use std::fmt::Write as _;

use crate::svg::parity::flowchart::escape_attr;
use crate::svg::parity::{fmt, fmt_display};

const HAND_DRAWN_ROUGHNESS: f32 = 0.7;
const HAND_DRAWN_FILL_WEIGHT: f32 = 1.5;
const HAND_DRAWN_HACHURE_GAP: f32 = 1.5;

pub(in crate::svg::parity::flowchart::render::node) fn render_process_rectangle(
    out: &mut String,
    common: &super::super::FlowchartNodeRenderCommon<'_>,
    details: &mut crate::svg::parity::flowchart::types::FlowchartRenderDetails,
) {
    let width = common.layout_node.width.max(1.0);
    let height = common.layout_node.height.max(1.0);
    let rough_paths = if common.look_is_hand_drawn() {
        super::super::helpers::timed_node_roughjs(common.timing, details, || {
            super::super::roughjs::roughjs_hachure_paths_for_rect(
                -width / 2.0,
                -height / 2.0,
                width,
                height,
                common.stroke_width,
                common.stroke_dasharray,
                HAND_DRAWN_FILL_WEIGHT,
                HAND_DRAWN_HACHURE_GAP,
                HAND_DRAWN_ROUGHNESS,
                common.hand_drawn_seed,
            )
        })
    } else {
        None
    };

    if let Some((fill_d, stroke_d)) = rough_paths {
        let _ = write!(
            out,
            r#"<g class="basic label-container" style="{}"><path d="{}" stroke="{}" stroke-width="{}" fill="none" stroke-dasharray="0 0"/><path d="{}" stroke="{}" stroke-width="{}" fill="none" stroke-dasharray="{}"/></g>"#,
            escape_attr(common.rough_group_style),
            escape_attr(&fill_d),
            escape_attr(common.fill_color),
            fmt_display(HAND_DRAWN_FILL_WEIGHT as f64),
            escape_attr(&stroke_d),
            escape_attr(common.stroke_color),
            common.stroke_width,
            escape_attr(common.stroke_dasharray),
        );
        return;
    }

    let _ = write!(
        out,
        r#"<rect class="basic label-container" style="{}" x="{}" y="{}" width="{}" height="{}" />"#,
        escape_attr(common.style),
        fmt(-width / 2.0),
        fmt(-height / 2.0),
        fmt(width),
        fmt(height),
    );
}
