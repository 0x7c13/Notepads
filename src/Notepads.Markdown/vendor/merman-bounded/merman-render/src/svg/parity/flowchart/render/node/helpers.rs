//! Node-level helpers (link sanitization, class building, placeholders).

use crate::svg::icon_registry::mermaid_unknown_icon_svg;
use crate::svg::parity::flowchart::types::{FlowchartRenderCtx, FlowchartRenderDetails};
use crate::svg::parity::util::escape_attr_display;
use crate::svg::parity::{escape_xml_display, escape_xml_into, fmt_display};
use merman_core::svg_security::{
    MermaidNavigationSecurity, SerializedMermaidNavigationHref, prepare_mermaid_navigation_href,
};
use std::fmt::Write as _;

pub(in crate::svg::parity::flowchart::render::node) fn icon_svg_or_placeholder(
    ctx: &FlowchartRenderCtx<'_>,
    node_id: &str,
    icon_name: &str,
    icon_size: f64,
) -> crate::Result<String> {
    let icon = match ctx.icon_registry {
        Some(registry) => {
            let prefix = ctx.icon_scope_prefix.ok_or_else(|| {
                crate::Error::icon_processing("flowchart icon scope prefix is unavailable")
            })?;
            let id_scope = prefix.scope_parts(&[node_id], ctx.work_meter)?;
            registry.render_icon(crate::svg::icon_registry::IconRenderRequest {
                icon_name,
                width_px: icon_size,
                height_px: icon_size,
                fallback_prefix: None,
                extra_class: None,
                id_scope,
                effective_config: ctx.config,
                work_meter: ctx.work_meter,
            })?
        }
        None => None,
    };
    Ok(icon.unwrap_or_else(|| {
        mermaid_unknown_icon_svg(fmt_display(icon_size), fmt_display(icon_size))
    }))
}

fn is_self_loop_label_node_id(id: &str) -> bool {
    let mut parts = id.split("---");
    let Some(a) = parts.next() else {
        return false;
    };
    let Some(b) = parts.next() else {
        return false;
    };
    let Some(n) = parts.next() else {
        return false;
    };
    parts.next().is_none() && a == b && (n == "1" || n == "2")
}

pub(super) fn try_render_self_loop_label_placeholder(
    out: &mut String,
    node_id: &str,
    x: f64,
    y: f64,
    html_labels: bool,
) -> bool {
    if !is_self_loop_label_node_id(node_id) {
        return false;
    }

    let _ = write!(
        out,
        r#"<g class="label edgeLabel" id="{}" transform="translate({},{})"><rect width="0.1" height="0.1"/><g class="label" style="" transform="translate(0,0)"><rect/>"#,
        escape_xml_display(node_id),
        fmt_display(x),
        fmt_display(y)
    );
    if html_labels {
        out.push_str(
            r#"<foreignObject width="0" height="0"><div xmlns="http://www.w3.org/1999/xhtml" style="display: table-cell; white-space: nowrap; line-height: 1.5; max-width: 10px; text-align: center;"><span class="nodeLabel"></span></div></foreignObject>"#,
        );
    } else {
        out.push_str(
            r#"<g><rect class="background" style="stroke: none"/><text y="-10.1" style=""><tspan class="text-outer-tspan row" x="0" y="-0.1em" dy="1.1em"/></text></g>"#,
        );
    }
    out.push_str("</g></g>");
    true
}

fn write_class_attr(out: &mut String, base: &str, classes: &[String]) {
    escape_xml_into(out, base);
    for c in classes {
        let t = c.trim();
        if t.is_empty() {
            continue;
        }
        out.push(' ');
        escape_xml_into(out, t);
    }
}

pub(super) struct NodeWrapperAttrs<'a> {
    pub(super) diagram_id: crate::svg::parity::SvgDiagramId<'a>,
    pub(super) diagram_type: &'a str,
    pub(super) node_id: &'a str,
    pub(super) dom_idx: Option<usize>,
    pub(super) class_attr_base: &'a str,
    pub(super) node_classes: &'a [String],
    pub(super) wrapped_in_a: bool,
    pub(super) href: Option<&'a SerializedMermaidNavigationHref>,
    pub(super) target: Option<&'a str>,
    pub(super) x: f64,
    pub(super) y: f64,
    pub(super) tooltip_enabled: bool,
    pub(super) tooltip: &'a str,
    pub(super) look: &'a str,
    pub(super) color_slot: Option<usize>,
}

pub(super) fn open_node_wrapper(out: &mut String, attrs: NodeWrapperAttrs<'_>) {
    let NodeWrapperAttrs {
        diagram_id,
        diagram_type,
        node_id,
        dom_idx,
        class_attr_base,
        node_classes,
        wrapped_in_a,
        href,
        target,
        x,
        y,
        tooltip_enabled,
        tooltip,
        look,
        color_slot,
    } = attrs;
    // Mermaid uses the stable `flowchart` DOM namespace for ordinary flowcharts,
    // even when the internal parser type is `flowchart-v2` or another variant.
    let dom_diagram_type = if diagram_type == "agentflow" {
        "agentflow"
    } else {
        "flowchart"
    };

    if wrapped_in_a {
        if let Some(href) = href {
            out.push_str(r#"<a xlink:href=""#);
            out.push_str(href.as_serialized_str());
            out.push('"');
            if let Some(target) = target {
                out.push_str(r#" target=""#);
                escape_xml_into(out, target);
                out.push('"');
            }
            out.push_str(r#" transform="translate("#);
            crate::svg::parity::util::fmt_into(out, x);
            out.push(',');
            crate::svg::parity::util::fmt_into(out, y);
            out.push_str(r#")" data-look=""#);
            escape_xml_into(out, look);
            out.push_str(r#"">"#);
        } else {
            out.push_str(r#"<a transform="translate("#);
            crate::svg::parity::util::fmt_into(out, x);
            out.push(',');
            crate::svg::parity::util::fmt_into(out, y);
            out.push_str(r#")" data-look=""#);
            escape_xml_into(out, look);
            out.push_str(r#"">"#);
        }
        out.push_str(r#"<g class=""#);
        write_class_attr(out, class_attr_base, node_classes);
        if let Some(dom_idx) = dom_idx {
            out.push_str(r#"" id=""#);
            let _ = write!(out, "{diagram_id}");
            out.push('-');
            escape_xml_into(out, dom_diagram_type);
            out.push('-');
            escape_xml_into(out, node_id);
            let _ = write!(out, "-{dom_idx}\"");
        } else {
            out.push_str(r#"" id=""#);
            let _ = write!(out, "{diagram_id}");
            out.push('-');
            escape_xml_into(out, node_id);
            out.push('"');
        }
    } else {
        out.push_str(r#"<g class=""#);
        write_class_attr(out, class_attr_base, node_classes);
        if let Some(dom_idx) = dom_idx {
            out.push_str(r#"" id=""#);
            let _ = write!(out, "{diagram_id}");
            out.push('-');
            escape_xml_into(out, dom_diagram_type);
            out.push('-');
            escape_xml_into(out, node_id);
            let _ = write!(out, r#"-{dom_idx}" transform="translate("#);
            crate::svg::parity::util::fmt_into(out, x);
            out.push(',');
            crate::svg::parity::util::fmt_into(out, y);
            out.push_str(r#")" data-look=""#);
            escape_xml_into(out, look);
            out.push('"');
        } else {
            out.push_str(r#"" id=""#);
            let _ = write!(out, "{diagram_id}");
            out.push('-');
            escape_xml_into(out, node_id);
            out.push_str(r#"" transform="translate("#);
            crate::svg::parity::util::fmt_into(out, x);
            out.push(',');
            crate::svg::parity::util::fmt_into(out, y);
            out.push_str(r#")" data-look=""#);
            escape_xml_into(out, look);
            out.push('"');
        }
    }
    if tooltip_enabled {
        let _ = write!(out, r#" title="{}""#, escape_attr_display(tooltip));
    }
    if let Some(slot) = color_slot {
        let _ = write!(out, r#" data-color-id="color-{slot}""#);
    }
    out.push('>');
}

pub(super) fn flowchart_node_label_span_class(label_type: &str) -> &'static str {
    if label_type == "markdown" {
        "nodeLabel markdown-node-label"
    } else {
        "nodeLabel"
    }
}

pub(super) fn timed_node_roughjs<T>(
    timing: crate::svg::parity::timing::RenderTiming,
    details: &mut FlowchartRenderDetails,
    f: impl FnOnce() -> T,
) -> T {
    if let Some(start) = timing.start() {
        details.node_roughjs_calls += 1;
        let out = f();
        details.node_roughjs += start.elapsed();
        out
    } else {
        f()
    }
}

pub(super) fn timed_node_label_html<T>(
    timing: crate::svg::parity::timing::RenderTiming,
    details: &mut FlowchartRenderDetails,
    f: impl FnOnce() -> T,
) -> T {
    if let Some(start) = timing.start() {
        details.node_label_html_calls += 1;
        let out = f();
        details.node_label_html += start.elapsed();
        out
    } else {
        f()
    }
}

pub(super) struct ResolvedNodeRenderInfo<'a> {
    pub(super) dom_idx: Option<usize>,
    pub(super) class_attr_base: &'static str,
    pub(super) wrapped_in_a: bool,
    pub(super) href: Option<SerializedMermaidNavigationHref>,
    pub(super) target: Option<&'a str>,
    pub(super) label_text: &'a str,
    pub(super) label_text_is_node_id: bool,
    pub(super) label_type: &'a str,
    pub(super) shape: &'a str,
    pub(super) node_icon: Option<&'a str>,
    pub(super) node_img: Option<&'a str>,
    pub(super) node_pos: Option<&'a str>,
    pub(super) node_constraint: Option<&'a str>,
    pub(super) node_asset_width: Option<f64>,
    pub(super) node_asset_height: Option<f64>,
    pub(super) node_styles: &'a [String],
    pub(super) node_classes: &'a [String],
}

pub(super) fn resolve_node_render_info<'a>(
    ctx: &'a FlowchartRenderCtx<'a>,
    node_id: &str,
) -> Option<ResolvedNodeRenderInfo<'a>> {
    if ctx.is_subgraph_collapsed(node_id)
        && let Some(sg) = ctx.subgraphs_by_id.get(node_id)
    {
        let subgraph_index = ctx.subgraph_indices_by_id.get(node_id).copied()?;
        let (node_classes, node_styles) = ctx.model.effective_subgraph_css(subgraph_index, sg);
        return Some(ResolvedNodeRenderInfo {
            // Mermaid's collapsed subgraph is emitted as a synthetic leaf node and therefore has
            // the plain diagram-id suffix rather than a FlowDB vertex-counter suffix.
            dom_idx: None,
            class_attr_base: "node",
            wrapped_in_a: false,
            href: None,
            target: None,
            label_text: ctx.model.subgraph_title_for_render(subgraph_index, sg),
            label_text_is_node_id: false,
            label_type: sg.label_type.as_deref().unwrap_or("text"),
            shape: "collapsedGroup",
            node_icon: None,
            node_img: None,
            node_pos: None,
            node_constraint: None,
            node_asset_width: None,
            node_asset_height: None,
            node_styles,
            node_classes,
        });
    }

    if let Some(sg) = ctx.subgraphs_by_id.get(node_id)
        && !ctx.subgraph_has_children(node_id)
    {
        let subgraph_index = ctx.subgraph_indices_by_id.get(node_id).copied()?;
        let (node_classes, node_styles) = ctx.model.effective_subgraph_css(subgraph_index, sg);
        return Some(ResolvedNodeRenderInfo {
            dom_idx: None,
            class_attr_base: "node",
            wrapped_in_a: false,
            href: None,
            target: None,
            label_text: ctx.model.subgraph_title_for_render(subgraph_index, sg),
            label_text_is_node_id: false,
            label_type: sg.label_type.as_deref().unwrap_or("text"),
            shape: "squareRect",
            node_icon: None,
            node_img: None,
            node_pos: None,
            node_constraint: None,
            node_asset_width: None,
            node_asset_height: None,
            node_styles,
            node_classes,
        });
    }

    if let Some(node) = ctx.nodes_by_id.get(node_id) {
        let dom_idx = Some(ctx.node_dom_index.get(node_id).copied().unwrap_or(0));
        let shape = node.layout_shape.as_deref().unwrap_or("squareRect");

        // Mermaid flowchart-v2 uses a distinct wrapper class for icon/image nodes.
        let class_attr_base = if shape == "imageSquare" {
            "image-shape default"
        } else if shape == "icon" || shape.starts_with("icon") {
            "icon-shape default"
        } else {
            "node default"
        };

        let link = node.link.as_deref().filter(|u| !u.is_empty());
        let link_present = link.is_some();
        // Mermaid stores the original click link in the model, but the final SVG sanitizer removes
        // unsafe or unknown schemes from the emitted anchor while keeping the `<a>` wrapper.
        let security_level_loose = ctx.config.get_str("securityLevel") == Some("loose");
        let href = link.and_then(|url| {
            prepare_mermaid_navigation_href(
                url,
                MermaidNavigationSecurity::from_security_level_loose(security_level_loose),
            )
        });
        // Mermaid wraps nodes in `<a>` only when a link is present. Callback-based
        // interactions (`click A someFn`) still mark the node as clickable, but do not
        // emit an anchor element in the SVG.
        let wrapped_in_a = link_present;
        let target = security_level_loose
            .then_some(node.link_target.as_deref())
            .flatten()
            .map(str::trim)
            .filter(|target| !target.is_empty());

        let (label_text, label_text_is_node_id) =
            if let Some(v) = ctx.model.node_label_for_render(node) {
                (v, false)
            } else {
                ("", true)
            };

        Some(ResolvedNodeRenderInfo {
            dom_idx,
            class_attr_base,
            wrapped_in_a,
            href,
            target,
            label_text,
            label_text_is_node_id,
            label_type: node.label_type.as_deref().unwrap_or("text"),
            shape,
            node_icon: node.icon.as_deref(),
            node_img: node.img.as_deref(),
            node_pos: node.pos.as_deref(),
            node_constraint: node.constraint.as_deref(),
            node_asset_width: node.asset_width,
            node_asset_height: node.asset_height,
            node_styles: &node.styles,
            node_classes: &node.classes,
        })
    } else {
        None
    }
}

pub(in crate::svg::parity::flowchart::render::node) fn compute_node_label_metrics(
    ctx: &FlowchartRenderCtx<'_>,
    layout_node: Option<&crate::model::LayoutNode>,
    label_text: &str,
    label_type: &str,
    node_classes: &[String],
    node_styles: &[String],
) -> crate::text::TextMetrics {
    // Layout metrics are authoritative. Callers without them can reuse a prepared label or
    // measure with the same effective styles as layout.
    let label_base_style = if ctx.node_wrap_mode == crate::text::WrapMode::HtmlLike {
        &ctx.html_label_text_style
    } else {
        &ctx.text_style
    };
    let node_text_style = crate::flowchart::flowchart_effective_text_style_for_node_classes(
        label_base_style,
        ctx.class_defs,
        node_classes,
        node_styles,
    );
    // Shapes such as hourglass clear their label after layout. Its cached bounds then belong
    // to a different label; let the measurement layer handle the actual empty render payload.
    let layout_node = layout_node.filter(|_| !label_text.is_empty());
    let prepared_metrics =
        || prepared_node_label_metrics(ctx, layout_node?.id.as_str(), label_text, &node_text_style);
    let metrics = if let Some(layout_node) = layout_node {
        if let (Some(width), Some(height)) = (layout_node.label_width, layout_node.label_height) {
            crate::text::TextMetrics {
                width,
                height,
                line_count: 0,
            }
        } else if let Some(metrics) = prepared_metrics() {
            metrics
        } else {
            crate::flowchart::flowchart_label_metrics_for_layout(
                crate::flowchart::FlowchartLabelMetricsRequest {
                    measurer: ctx.measurer,
                    raw_label: label_text,
                    label_type,
                    style: &node_text_style,
                    max_width_px: Some(ctx.wrapping_width),
                    wrap_mode: ctx.node_wrap_mode,
                    config: ctx.config,
                    math_renderer: ctx.math_renderer,
                },
            )
        }
    } else if let Some(metrics) = prepared_metrics() {
        metrics
    } else {
        crate::flowchart::flowchart_label_metrics_for_layout(
            crate::flowchart::FlowchartLabelMetricsRequest {
                measurer: ctx.measurer,
                raw_label: label_text,
                label_type,
                style: &node_text_style,
                max_width_px: Some(ctx.wrapping_width),
                wrap_mode: ctx.node_wrap_mode,
                config: ctx.config,
                math_renderer: ctx.math_renderer,
            },
        )
    };

    // The measurement layer owns empty-label semantics. HTML line boxes and icons can have
    // nonzero dimensions without plain text; preserve their measured bounds during painting.

    let min_width = layout_node
        .filter(|node| !ctx.subgraphs_by_id.contains_key(node.id.as_str()))
        .and_then(|node| ctx.nodes_by_id.get(node.id.as_str()))
        .map_or(0.0, |node| {
            crate::flowchart::flowchart_node_label_min_width(
                label_text,
                node.layout_shape.as_deref(),
                ctx.config,
            )
        });
    metrics.with_label_min_width(label_text, min_width, None)
}

pub(in crate::svg::parity::flowchart::render::node) fn prepared_node_label_metrics(
    ctx: &FlowchartRenderCtx<'_>,
    node_id: &str,
    label_text: &str,
    style: &crate::text::TextStyle,
) -> Option<crate::text::TextMetrics> {
    let sidecar = ctx.svg_label_sidecar?;
    let owner = sidecar.node_owner(node_id, ctx.swimlane_direction.is_some())?;
    let metrics = sidecar.prepared_metrics(
        owner,
        label_text,
        ctx.measurer,
        style,
        Some(ctx.wrapping_width),
        true,
        crate::flowchart::FlowchartSvgWidthMode::Bbox,
    )?;
    let min_width = ctx
        .nodes_by_id
        .get(node_id)
        .filter(|_| !ctx.subgraphs_by_id.contains_key(node_id))
        .map_or(0.0, |node| {
            crate::flowchart::flowchart_node_label_min_width(
                label_text,
                node.layout_shape.as_deref(),
                ctx.config,
            )
        });
    Some(metrics.with_label_min_width(label_text, min_width, None))
}

/// Match createText's wrapping decision for labels with an icon/image background.
pub(in crate::svg::parity::flowchart::render::node) fn asset_label_div_style(
    ctx: &FlowchartRenderCtx<'_>,
    label_bbox_width: f64,
) -> String {
    let width = fmt_display(ctx.wrapping_width);
    if ctx.node_wrap_mode == crate::text::WrapMode::HtmlLike
        && label_bbox_width >= ctx.wrapping_width
    {
        format!(
            "display: table; white-space: break-spaces; line-height: 1.5; max-width: {width}px; text-align: center; width: {width}px;"
        )
    } else {
        format!(
            "display: table-cell; white-space: nowrap; line-height: 1.5; max-width: {width}px; text-align: center;"
        )
    }
}
