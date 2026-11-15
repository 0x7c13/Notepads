//! Flowchart v2 stacked document shape.

use std::fmt::Write as _;

use crate::svg::parity::{escape_xml_display, fmt_display};

use super::super::geom::path_from_points;
use super::super::helpers;
use super::super::roughjs::roughjs_paths_for_svg_path;

pub(in crate::svg::parity::flowchart::render::node) fn render_stacked_document(
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

    let geometry = crate::flowchart::flowchart_stacked_document_geometry(
        metrics.width,
        metrics.height,
        ctx.node_padding,
        common.look_is_neo(),
    );
    label.dx = geometry.label_dx;
    label.dy = geometry.label_dy;
    let outer_path = path_from_points(&geometry.outer_points);
    let inner_path = path_from_points(&geometry.inner_points);

    let _ = write!(
        out,
        r#"<g class="basic label-container outer-path" transform="translate(0,{})">"#,
        fmt_display(geometry.group_dy)
    );
    if let Some((fill_d, stroke_d)) =
        super::super::helpers::timed_node_roughjs(common.timing, details, || {
            roughjs_paths_for_svg_path(
                &outer_path,
                common.stroke_width,
                common.stroke_dasharray,
                common.hand_drawn_seed,
            )
        })
    {
        let _ = write!(
            out,
            r#"<path d="{}" stroke="none" stroke-width="0" fill="{}" style="{}"/>"#,
            escape_xml_display(&fill_d),
            escape_xml_display(common.fill_color),
            escape_xml_display(common.style)
        );
        let _ = write!(
            out,
            r#"<path d="{}" stroke="{}" stroke-width="{}" fill="none" stroke-dasharray="{}" style="{}"/>"#,
            escape_xml_display(&stroke_d),
            escape_xml_display(common.stroke_color),
            fmt_display(common.stroke_width as f64),
            escape_xml_display(common.stroke_dasharray),
            escape_xml_display(common.style)
        );
    }
    out.push_str("<g>");
    if let Some((fill_d, stroke_d)) =
        super::super::helpers::timed_node_roughjs(common.timing, details, || {
            roughjs_paths_for_svg_path(
                &inner_path,
                common.stroke_width,
                common.stroke_dasharray,
                common.hand_drawn_seed,
            )
        })
    {
        let _ = write!(
            out,
            r#"<path d="{}" stroke="none" stroke-width="0" fill="{}" style="{}"/>"#,
            escape_xml_display(&fill_d),
            escape_xml_display(common.fill_color),
            escape_xml_display(common.style)
        );
        let _ = write!(
            out,
            r#"<path d="{}" stroke="{}" stroke-width="{}" fill="none" stroke-dasharray="{}" style="{}"/>"#,
            escape_xml_display(&stroke_d),
            escape_xml_display(common.stroke_color),
            fmt_display(common.stroke_width as f64),
            escape_xml_display(common.stroke_dasharray),
            escape_xml_display(common.style)
        );
    }
    out.push_str("</g></g>");
}
