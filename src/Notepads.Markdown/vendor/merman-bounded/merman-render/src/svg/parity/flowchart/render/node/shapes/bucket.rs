//! Flowchart v2 bucket shape.

use std::fmt::Write as _;

use crate::svg::parity::flowchart::escape_attr;
use crate::svg::parity::{fmt, fmt_display};

use super::super::roughjs::roughjs_paths_for_svg_path;

pub(in crate::svg::parity::flowchart::render::node) fn render_bucket(
    out: &mut String,
    ctx: &crate::svg::parity::flowchart::types::FlowchartRenderCtx<'_>,
    common: &super::super::FlowchartNodeRenderCommon<'_>,
    label: &mut super::super::FlowchartNodeLabelState<'_>,
    details: &mut crate::svg::parity::flowchart::types::FlowchartRenderDetails,
) {
    let metrics = super::super::helpers::compute_node_label_metrics(
        ctx,
        Some(common.layout_node),
        label.text,
        label.label_type,
        common.node_classes,
        common.node_styles,
    );
    let p = ctx.node_padding.max(0.0);
    let w = (metrics.width + 2.0 * p)
        .max(common.layout_node.width.max(0.0))
        .max(80.0);
    let rim_ry = (w * 0.08).clamp(5.0, 12.0);
    let total_height = (metrics.height + 2.0 * p + rim_ry).max(common.layout_node.height.max(0.0));
    let top_y = -total_height / 2.0 + rim_ry;
    let bottom_y = total_height / 2.0;
    let bottom_w = w * 0.72;

    let mut body = String::new();
    let _ = write!(
        &mut body,
        "M{},{} L{},{} A {} {} 0 0 0 {},{} L{},{} A {} {} 0 0 0 {},{} Z",
        fmt(-w / 2.0),
        fmt(top_y),
        fmt(-bottom_w / 2.0),
        fmt(bottom_y),
        fmt(bottom_w / 2.0),
        fmt(rim_ry),
        fmt(bottom_w / 2.0),
        fmt(bottom_y),
        fmt(w / 2.0),
        fmt(top_y),
        fmt(w / 2.0),
        fmt(rim_ry),
        fmt(-w / 2.0),
        fmt(top_y),
    );

    out.push_str(r#"<g class="basic label-container">"#);
    if common.look_is_hand_drawn() {
        if let Some((fill_d, stroke_d)) =
            super::super::helpers::timed_node_roughjs(common.timing, details, || {
                roughjs_paths_for_svg_path(
                    &body,
                    common.stroke_width,
                    common.stroke_dasharray,
                    common.hand_drawn_seed,
                )
            })
        {
            let _ = write!(
                out,
                r#"<path d="{}" stroke="none" stroke-width="0" fill="{}" style="{}"/><path d="{}" stroke="{}" stroke-width="{}" fill="none" stroke-dasharray="{}" style="{}"/>"#,
                escape_attr(&fill_d),
                escape_attr(common.fill_color),
                escape_attr(common.style),
                escape_attr(&stroke_d),
                escape_attr(common.stroke_color),
                fmt_display(common.stroke_width as f64),
                escape_attr(common.stroke_dasharray),
                escape_attr(common.style),
            );
        } else {
            let _ = write!(
                out,
                r#"<path d="{}" style="{}"/>"#,
                escape_attr(&body),
                escape_attr(common.style),
            );
        }
    } else {
        let _ = write!(
            out,
            r#"<path d="{}" style="{}"/>"#,
            escape_attr(&body),
            escape_attr(common.style),
        );
    }

    let _ = write!(
        out,
        r#"<ellipse cx="0" cy="{}" rx="{}" ry="{}" style="fill:none;stroke:{};stroke-width:1px"/>"#,
        fmt(top_y),
        fmt(w / 2.0),
        fmt(rim_ry),
        escape_attr(common.stroke_color),
    );
    out.push_str("</g>");

    let body_center_y = top_y + (bottom_y - top_y) / 2.0;
    label.dy = body_center_y;
}
