//! Flowchart v2 divided rectangle (Divided process).

use std::fmt::Write as _;

use crate::svg::parity::flowchart::escape_attr;

use super::super::roughjs::roughjs_paths_for_polygon;

pub(in crate::svg::parity::flowchart::render::node) fn render_divided_rect(
    out: &mut String,
    ctx: &crate::svg::parity::flowchart::types::FlowchartRenderCtx<'_>,
    common: &super::super::FlowchartNodeRenderCommon<'_>,
    label: &mut super::super::FlowchartNodeLabelState<'_>,
    details: &mut crate::svg::parity::flowchart::types::FlowchartRenderDetails,
) {
    // Mermaid draws the polygon using `h` and then the rendered bbox expands to
    // `out_h = h + rectOffset` where `rectOffset = h * 0.2`, i.e. `out_h = 1.2*h`.
    let out_w = common.layout_node.width.max(1.0);
    let out_h = common.layout_node.height.max(1.0);
    let h = out_h / 1.2;
    let w = out_w;
    let rect_offset = h * 0.2;
    let x = -w / 2.0;
    let y = -h / 2.0 - rect_offset / 2.0;

    // Mermaid's labelHelper transform retains the authored node padding even when Neo uses
    // fixed shape padding. Convert that source transform to the local centered-label wrapper.
    let authored_padding = ctx.node_padding;
    let shape_padding = if common.look_is_neo() {
        16.0
    } else {
        authored_padding
    };
    let padding_delta = (authored_padding - shape_padding) / 2.0;
    label.dx = padding_delta;
    label.dy = padding_delta + rect_offset / 2.0;

    let pts: Vec<(f64, f64)> = vec![
        (x, y + rect_offset),
        (-x, y + rect_offset),
        (-x, -y),
        (x, -y),
        (x, y),
        (-x, y),
        (-x, y + rect_offset),
    ];
    let (fill_d, stroke_d) =
        super::super::helpers::timed_node_roughjs(common.timing, details, || {
            roughjs_paths_for_polygon(&pts, common.stroke_width, common.hand_drawn_seed)
        })
        .unwrap_or_else(|| ("M0,0".to_string(), "M0,0".to_string()));
    let _ = write!(
        out,
        r##"<g class="basic label-container outer-path"><path d="{}" stroke="none" stroke-width="0" fill="{}" fill-rule="evenodd" style="{}"/><path d="{}" stroke="{}" stroke-width="{}" fill="none" stroke-dasharray="{}" style="{}"/></g>"##,
        escape_attr(&fill_d),
        escape_attr(common.fill_color),
        escape_attr(common.style),
        escape_attr(&stroke_d),
        escape_attr(common.stroke_color),
        crate::svg::parity::util::fmt_display(common.stroke_width as f64),
        escape_attr(common.stroke_dasharray),
        escape_attr(common.style),
    );
}
