use crate::model::{Bounds, LayoutCluster};
use crate::text::MERMAID_CREATE_TEXT_DEFAULT_WIDTH_PX;
use std::collections::HashMap;
use std::fmt::Write as _;

use super::super::SvgDiagramId;
use super::super::timing::RenderTiming;
use super::super::{escape_attr_display, escape_xml_display, fmt};
use super::bounds::include_xywh;
use super::context::ClassEmitCheckpoint;
use super::label::{class_math_html_label, write_class_svg_plain_node_text};
use crate::Result;

#[derive(Clone, Copy)]
pub(super) struct ClassNamespaceClusterGroupContext<'a> {
    pub diagram_id: SvgDiagramId<'a>,
    pub content_tx: f64,
    pub content_ty: f64,
    pub bounds_dx: f64,
    pub bounds_dy: f64,
    pub use_html_labels: bool,
    pub look: &'a str,
    pub mermaid_config: Option<&'a merman_core::MermaidConfig>,
    pub math_renderer: Option<&'a (dyn crate::math::MathRenderer + Send + Sync)>,
    pub timing: RenderTiming,
    pub emit: ClassEmitCheckpoint<'a>,
}

pub(super) fn render_class_namespace_cluster_group(
    out: &mut String,
    content_bounds: &mut Option<Bounds>,
    clusters: &[LayoutCluster],
    ctx: ClassNamespaceClusterGroupContext<'_>,
) -> Result<std::time::Duration> {
    let clusters_start = ctx.timing.start();
    out.push_str(r#"<g class="clusters">"#);
    for c in clusters {
        render_class_namespace_cluster(out, content_bounds, c, ctx)?;
    }
    out.push_str("</g>");
    Ok(clusters_start
        .map(|start| start.elapsed())
        .unwrap_or_default())
}

fn render_class_namespace_cluster(
    out: &mut String,
    content_bounds: &mut Option<Bounds>,
    cluster: &LayoutCluster,
    ctx: ClassNamespaceClusterGroupContext<'_>,
) -> Result<()> {
    let w = cluster.width.max(1.0);
    let h = cluster.height.max(1.0);
    let left = cluster.x - w / 2.0 + ctx.content_tx;
    let top = cluster.y - h / 2.0 + ctx.content_ty;
    include_xywh(
        content_bounds,
        left + ctx.bounds_dx,
        top + ctx.bounds_dy,
        w,
        h,
    );

    let label_w = cluster.title_label.width.max(0.0);
    let label_h = cluster.title_label.height.max(0.0);
    let label_x = left + (w - label_w) / 2.0;
    let label_y = top + cluster.title_margin_top;
    include_xywh(
        content_bounds,
        label_x + ctx.bounds_dx,
        label_y + ctx.bounds_dy,
        label_w,
        label_h,
    );

    out.push_str(r#"<g class="cluster undefined" id=""#);
    let _ = write!(out, "{}", ctx.diagram_id);
    ctx.emit.checkpoint()?;
    let _ = write!(
        out,
        r#"-{}" data-look="{}"><rect x="{}" y="{}" width="{}" height="{}" style=""/><g class="cluster-label" transform="translate({}, {})">"#,
        escape_attr_display(&cluster.id),
        escape_attr_display(ctx.look),
        fmt(left),
        fmt(top),
        fmt(w),
        fmt(h),
        fmt(label_x),
        fmt(label_y),
    );
    render_class_namespace_label(out, cluster, ctx);
    out.push_str("</g></g>");
    Ok(())
}

fn render_class_namespace_label(
    out: &mut String,
    cluster: &LayoutCluster,
    ctx: ClassNamespaceClusterGroupContext<'_>,
) {
    if ctx.use_html_labels {
        let _ = write!(
            out,
            r#"<foreignObject width="{}" height="{}"><div xmlns="http://www.w3.org/1999/xhtml" style="display: table-cell; white-space: nowrap; line-height: 1.5; max-width: {}px; text-align: center;"><span class="nodeLabel">{}</span></div></foreignObject>"#,
            fmt(cluster.title_label.width.max(0.0)),
            fmt(cluster.title_label.height.max(0.0)),
            MERMAID_CREATE_TEXT_DEFAULT_WIDTH_PX,
            class_namespace_title_html(&cluster.title, ctx),
        );
    } else {
        write_class_svg_plain_node_text(out, &cluster.title);
    }
}

fn class_namespace_title_html(title: &str, ctx: ClassNamespaceClusterGroupContext<'_>) -> String {
    class_math_html_label(title, ctx.mermaid_config, ctx.math_renderer)
        .unwrap_or_else(|| format!("<p>{}</p>", escape_xml_display(title)))
}

pub(super) fn class_namespace_root_offset(c: &LayoutCluster) -> (f64, f64) {
    let w = c.width.max(1.0);
    let h = c.height.max(1.0);
    (c.x - w / 2.0 - 8.0, c.y - h / 2.0)
}

#[allow(clippy::too_many_arguments)]
pub(super) fn render_class_namespace_clusters_in_root(
    out: &mut String,
    content_bounds: &mut Option<Bounds>,
    clusters_by_id: &HashMap<&str, &LayoutCluster>,
    cluster_ids: &[&str],
    ctx: ClassNamespaceClusterGroupContext<'_>,
    root_ns_id: &str,
    root_dx: f64,
    root_dy: f64,
) -> Result<()> {
    out.push_str(r#"<g class="clusters">"#);
    for ns_id in cluster_ids {
        let c = clusters_by_id
            .get(ns_id)
            .copied()
            .expect("validated Class render cluster id");

        let w = c.width.max(1.0);
        let h = c.height.max(1.0);
        let (left, top) = if *ns_id == root_ns_id {
            (8.0, 8.0)
        } else {
            (
                c.x - w / 2.0 - root_dx,
                c.y - h / 2.0 + ctx.content_ty - root_dy,
            )
        };
        include_xywh(
            content_bounds,
            left + root_dx + ctx.bounds_dx,
            top + root_dy + ctx.bounds_dy,
            w,
            h,
        );

        let label_w = c.title_label.width.max(0.0);
        let label_h = c.title_label.height.max(0.0);
        let label_x = left + (w - label_w) / 2.0;
        let label_y = top + c.title_margin_top;
        include_xywh(
            content_bounds,
            label_x + root_dx + ctx.bounds_dx,
            label_y + root_dy + ctx.bounds_dy,
            label_w,
            label_h,
        );

        out.push_str(r#"<g class="cluster undefined" id=""#);
        let _ = write!(out, "{}", ctx.diagram_id);
        ctx.emit.checkpoint()?;
        let _ = write!(
            out,
            r#"-{}" data-look="{}"><rect x="{}" y="{}" width="{}" height="{}" style=""/><g class="cluster-label" transform="translate({}, {})">"#,
            escape_attr_display(&c.id),
            escape_attr_display(ctx.look),
            fmt(left),
            fmt(top),
            fmt(w),
            fmt(h),
            fmt(label_x),
            fmt(label_y),
        );
        render_class_namespace_label(out, c, ctx);
        out.push_str("</g></g>");
    }
    out.push_str("</g>");
    Ok(())
}
