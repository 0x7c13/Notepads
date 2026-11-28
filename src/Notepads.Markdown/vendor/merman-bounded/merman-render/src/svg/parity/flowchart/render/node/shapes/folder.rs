//! Mermaid 11.17 folder/directory flowchart shape.

use std::fmt::Write as _;

use crate::svg::parity::flowchart::escape_attr;
use crate::svg::parity::util;

use super::super::geom::path_from_points;
use super::super::helpers;
use super::super::roughjs::roughjs_paths_for_svg_path;

fn folder_geometry(label_width: f64, label_height: f64, padding: f64) -> (f64, f64, f64) {
    let padding = padding.max(0.0);
    let width = (label_width + 2.0 * padding).max(90.0);
    let content_height = label_height + 2.0 * padding;
    let tab_height = (content_height * 0.16).clamp(8.0, 14.0);
    let total_height = content_height + tab_height;
    (width, total_height, tab_height)
}

fn folder_points(width: f64, total_height: f64, tab_height: f64) -> Vec<(f64, f64)> {
    let tab_width = (width * 0.38).max(28.0);
    let top = -total_height / 2.0;
    vec![
        (-width / 2.0, top),
        (-width / 2.0 + tab_width, top),
        (-width / 2.0 + tab_width, top + tab_height),
        (width / 2.0, top + tab_height),
        (width / 2.0, total_height / 2.0),
        (-width / 2.0, total_height / 2.0),
    ]
}

pub(in crate::svg::parity::flowchart::render::node) fn render_folder(
    out: &mut String,
    ctx: &crate::svg::parity::flowchart::types::FlowchartRenderCtx<'_>,
    common: &super::super::FlowchartNodeRenderCommon<'_>,
    label: &mut super::super::FlowchartNodeLabelState<'_>,
    details: &mut crate::svg::parity::flowchart::types::FlowchartRenderDetails,
) {
    let metrics = helpers::compute_node_label_metrics(
        ctx,
        Some(common.layout_node),
        label.text,
        label.label_type,
        common.node_classes,
        common.node_styles,
    );
    let (measured_width, measured_height, tab_height) =
        folder_geometry(metrics.width, metrics.height, ctx.node_padding);
    // Mermaid takes the larger of the rendered label box and any pre-existing node dimensions.
    let width = measured_width.max(common.layout_node.width.max(0.0));
    let total_height = measured_height.max(common.layout_node.height.max(0.0));
    let points = folder_points(width, total_height, tab_height.min(total_height));
    let path_data = path_from_points(&points);

    if common.look_is_hand_drawn() {
        let rough_paths = helpers::timed_node_roughjs(common.timing, details, || {
            roughjs_paths_for_svg_path(
                &path_data,
                common.stroke_width,
                common.stroke_dasharray,
                common.hand_drawn_seed,
            )
        });
        if let Some((fill_d, stroke_d)) = rough_paths {
            let _ = write!(
                out,
                r#"<g class="basic label-container" style="{}"><path d="{}" stroke="none" stroke-width="0" fill="{}" style="{}"/><path d="{}" stroke="{}" stroke-width="{}" fill="none" stroke-dasharray="{}" style="{}"/></g>"#,
                escape_attr(common.rough_group_style),
                escape_attr(&fill_d),
                escape_attr(common.fill_color),
                escape_attr(common.style),
                escape_attr(&stroke_d),
                escape_attr(common.stroke_color),
                util::fmt_display(common.stroke_width as f64),
                escape_attr(common.stroke_dasharray),
                escape_attr(common.style),
            );
        } else {
            let _ = write!(
                out,
                r#"<path d="{}" class="basic label-container" style="{}"/>"#,
                escape_attr(&path_data),
                escape_attr(common.style),
            );
        }
    } else {
        let _ = write!(
            out,
            r#"<path d="{}" class="basic label-container" style="{}"/>"#,
            escape_attr(&path_data),
            escape_attr(common.style),
        );
    }

    let body_height = total_height - tab_height.min(total_height);
    label.dy = -total_height / 2.0 + tab_height.min(total_height) + body_height / 2.0;
}
