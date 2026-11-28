//! Flowchart v2 browser shape.

use std::fmt::Write as _;

use crate::svg::parity::flowchart::escape_attr;
use crate::svg::parity::{fmt, fmt_display};

use super::super::roughjs::roughjs_paths_for_svg_path;

fn rounded_rect_path_d(x: f64, y: f64, w: f64, h: f64, r: f64) -> String {
    let r = r.min(w / 2.0).min(h / 2.0).max(0.0);
    format!(
        "M {} {} H {} A {} {} 0 0 1 {} {} V {} A {} {} 0 0 1 {} {} H {} A {} {} 0 0 1 {} {} V {} A {} {} 0 0 1 {} {} Z",
        fmt(x + r),
        fmt(y),
        fmt(x + w - r),
        fmt(r),
        fmt(r),
        fmt(x + w),
        fmt(y + r),
        fmt(y + h - r),
        fmt(r),
        fmt(r),
        fmt(x + w - r),
        fmt(y + h),
        fmt(x + r),
        fmt(r),
        fmt(r),
        fmt(x),
        fmt(y + h - r),
        fmt(y + r),
        fmt(r),
        fmt(r),
        fmt(x + r),
        fmt(y),
    )
}

pub(in crate::svg::parity::flowchart::render::node) fn render_browser(
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
    let bar_height = 18.0;
    let radius = 12.0;
    let w = (metrics.width + 2.0 * p)
        .max(common.layout_node.width.max(0.0))
        .max(90.0);
    let h = (metrics.height + 2.0 * p + bar_height).max(common.layout_node.height.max(0.0));
    let top = -h / 2.0;
    let body_path = rounded_rect_path_d(-w / 2.0, top, w, h, radius);
    let accent = common.stroke_color;

    out.push_str(r#"<g class="basic label-container">"#);
    if common.look_is_hand_drawn() {
        if let Some((fill_d, stroke_d)) =
            super::super::helpers::timed_node_roughjs(common.timing, details, || {
                roughjs_paths_for_svg_path(
                    &body_path,
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
                escape_attr(&body_path),
                escape_attr(common.style),
            );
        }
    } else {
        let _ = write!(
            out,
            r#"<rect x="{}" y="{}" width="{}" height="{}" rx="{}" ry="{}" style="{}"/>"#,
            fmt(-w / 2.0),
            fmt(top),
            fmt(w),
            fmt(h),
            fmt(radius),
            fmt(radius),
            escape_attr(common.style),
        );
    }

    let _ = write!(
        out,
        r#"<line x1="{}" y1="{}" x2="{}" y2="{}" style="stroke:{};stroke-width:1px"/>"#,
        fmt(-w / 2.0),
        fmt(top + bar_height),
        fmt(w / 2.0),
        fmt(top + bar_height),
        escape_attr(accent),
    );
    for i in 0..3 {
        let _ = write!(
            out,
            r#"<circle cx="{}" cy="{}" r="2.5" style="fill:{};stroke:none"/>"#,
            fmt(-w / 2.0 + 12.0 + i as f64 * 9.0),
            fmt(top + bar_height / 2.0),
            escape_attr(accent),
        );
    }
    let _ = write!(
        out,
        r#"<rect class="browser-address-bar" x="{}" y="{}" width="{}" height="{}" rx="3" ry="3" style="fill:none;stroke:{};stroke-width:1px;opacity:0.6"/>"#,
        fmt(-w / 2.0 + 44.0),
        fmt(top + 4.0),
        fmt((w - 56.0).max(10.0)),
        fmt(bar_height - 8.0),
        escape_attr(accent),
    );
    out.push_str("</g>");

    label.dy = top + bar_height + (h - bar_height) / 2.0;
}
