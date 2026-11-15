//! Flowchart v2 stadium shape.

use std::fmt::Write as _;

use crate::svg::parity::{escape_attr, fmt, fmt_display};

use super::super::geom::path_from_points;
use super::super::roughjs::{roughjs_hachure_paths_for_svg_path, roughjs_paths_for_svg_path};

const FLOWCHART_STADIUM_HAND_DRAWN_ROUGHNESS: f32 = 0.7;
const FLOWCHART_STADIUM_HAND_DRAWN_FILL_WEIGHT: f32 = 1.5;
const FLOWCHART_STADIUM_HAND_DRAWN_HACHURE_GAP: f32 = 1.5;

pub(in crate::svg::parity::flowchart::render::node) fn render_stadium(
    out: &mut String,
    ctx: &crate::svg::parity::flowchart::types::FlowchartRenderCtx<'_>,
    common: &super::super::FlowchartNodeRenderCommon<'_>,
    label: &super::super::FlowchartNodeLabelState<'_>,
    details: &mut crate::svg::parity::flowchart::types::FlowchartRenderDetails,
) {
    // Port of Mermaid `stadium.ts` points + `createPathFromPoints`.
    // Note that Mermaid's `generateCirclePoints()` pushes negated coordinates.
    // Use one source geometry for layout, paint, and intersection. The sampled arc list is
    // retained for the path, while the source theoretical width remains the node dimension;
    // re-sampling it during layout would shrink the same stadium twice.
    //
    // Mermaid sizes the path from the `labelHelper(...)` bbox, so HTML labels are measured with
    // the CSS theme font size (e.g. `12.5px`) rather than the integer `parseFontSize` number.
    let metrics = super::super::helpers::compute_node_label_metrics(
        ctx,
        Some(common.layout_node),
        label.text,
        label.label_type,
        common.node_classes,
        common.node_styles,
    );
    let geometry = crate::flowchart::StadiumGeometry::from_label(
        metrics.width,
        metrics.height,
        ctx.node_padding,
        crate::config::mermaid_config_diagram_look(ctx.config).is_neo(),
    );
    let path_data = path_from_points(&geometry.points);
    let w = geometry.width;
    let h = geometry.height;
    let radius = h / 2.0;

    if common.look_is_hand_drawn() {
        if let Some((fill_d, stroke_d)) =
            super::super::helpers::timed_node_roughjs(common.timing, details, || {
                roughjs_hachure_paths_for_svg_path(
                    &path_data,
                    common.stroke_width,
                    common.stroke_dasharray,
                    FLOWCHART_STADIUM_HAND_DRAWN_FILL_WEIGHT,
                    FLOWCHART_STADIUM_HAND_DRAWN_HACHURE_GAP,
                    FLOWCHART_STADIUM_HAND_DRAWN_ROUGHNESS,
                    common.hand_drawn_seed,
                )
            })
        {
            out.push_str(r#"<g class="basic label-container outer-path">"#);
            let _ = write!(
                out,
                r#"<path d="{}" stroke="{}" stroke-width="{}" fill="none" stroke-dasharray="0 0"/>"#,
                escape_attr(&fill_d),
                escape_attr(common.fill_color),
                fmt_display(FLOWCHART_STADIUM_HAND_DRAWN_FILL_WEIGHT as f64),
            );
            let _ = write!(
                out,
                r#"<path d="{}" stroke="{}" stroke-width="{}" fill="none" stroke-dasharray="{}"/>"#,
                escape_attr(&stroke_d),
                escape_attr(common.stroke_color),
                fmt_display(common.stroke_width as f64),
                escape_attr(common.stroke_dasharray),
            );
            out.push_str("</g>");
            return;
        }
    } else if let Some((fill_d, stroke_d)) =
        super::super::helpers::timed_node_roughjs(common.timing, details, || {
            roughjs_paths_for_svg_path(
                &path_data,
                common.stroke_width,
                common.stroke_dasharray,
                common.hand_drawn_seed,
            )
        })
    {
        out.push_str(r#"<g class="basic label-container outer-path">"#);
        let _ = write!(
            out,
            r#"<path d="{}" stroke="none" stroke-width="0" fill="{}" style="{}"/>"#,
            escape_attr(&fill_d),
            escape_attr(common.fill_color),
            escape_attr(common.style)
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
        return;
    }

    let _ = write!(
        out,
        r#"<rect class="basic label-container" style="{}" x="{}" y="{}" width="{}" height="{}" rx="{}" ry="{}"/>"#,
        escape_attr(common.style),
        fmt(-w / 2.0),
        fmt(-h / 2.0),
        fmt(w),
        fmt(h),
        fmt(radius),
        fmt(radius)
    );
}
