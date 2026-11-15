//! Flowchart v2 lined cylinder (Disk storage).

use std::fmt::Write as _;

use crate::svg::parity::flowchart::escape_attr;
use crate::svg::parity::util;

pub(in crate::svg::parity::flowchart::render::node) fn render_lined_cylinder(
    out: &mut String,
    ctx: &crate::svg::parity::flowchart::types::FlowchartRenderCtx<'_>,
    common: &super::super::FlowchartNodeRenderCommon<'_>,
    label: &mut super::super::FlowchartNodeLabelState<'_>,
) {
    // Mirror Mermaid `linedCylinder.ts` (non-handDrawn) + translate.
    let w = common.layout_node.width.max(1.0);
    let rx = w / 2.0;
    let ry = rx / (2.5 + w / 50.0);
    let out_h = common.layout_node.height.max(1.0);
    let h = (out_h - 2.0 * ry).max(0.0);
    let outer_offset = h * 0.1;

    // Mermaid moves the label down by `ry`.
    label.dy = ry;

    let path_data = format!(
        "M0,{ry} a{rx},{ry} 0,0,0 {w},0 a{rx},{ry} 0,0,0 -{w},0 l0,{h} a{rx},{ry} 0,0,0 {w},0 l0,-{h} M0,{y2} a{rx},{ry} 0,0,0 {w},0",
        ry = util::fmt(ry),
        rx = util::fmt(rx),
        w = util::fmt(w),
        h = util::fmt(h),
        y2 = util::fmt(ry + outer_offset),
    );
    let _ = write!(
        out,
        r#"<path d="{}" class="basic label-container outer-path" style="{}""#,
        escape_attr(&path_data),
        escape_attr(common.style),
    );
    if ctx.security_level_loose {
        let _ = write!(out, r#" label-offset-y="{}""#, util::fmt(ry));
    }
    let _ = write!(
        out,
        r#" transform="translate({},{})"/>"#,
        util::fmt(-w / 2.0),
        util::fmt(-(h / 2.0 + ry))
    );
}
