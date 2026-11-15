//! Flowchart edge label renderer.

use super::super::*;
use crate::svg::parity::flowchart::util::HTML_LABEL_FOREIGN_OBJECT_OVERFLOW_ATTR;
use std::borrow::Cow;

fn padded_html_edge_label_background(padding: f64, width: f64, height: f64) -> String {
    if padding <= 0.0 || width <= 0.0 || height <= 0.0 {
        return String::new();
    }

    let width = width + 2.0 * padding;
    let height = height + 2.0 * padding;
    format!(
        r#"<rect class="background" x="{}" y="{}" width="{}" height="{}" rx="{}" ry="{}"/>"#,
        fmt_display(-width / 2.0),
        fmt_display(-height / 2.0),
        fmt_display(width),
        fmt_display(height),
        fmt_display(padding),
        fmt_display(padding),
    )
}

#[derive(Debug, Clone, Copy, PartialEq)]
struct SvgEdgeLabelBox {
    translate_x: f64,
    translate_y: f64,
    background_x: f64,
    background_y: f64,
    background_width: f64,
    background_height: f64,
}

fn centered_svg_edge_label_box(
    layout_width: f64,
    layout_height: f64,
    text_bbox_y: f64,
) -> SvgEdgeLabelBox {
    const BACKGROUND_PADDING_PX: f64 = 2.0;

    let background_width = layout_width.max(0.0);
    let background_height = layout_height.max(0.0);
    let text_height = (background_height - 2.0 * BACKGROUND_PADDING_PX).max(0.0);
    SvgEdgeLabelBox {
        // Mermaid emits centered SVG text (`text-anchor="middle"`) and centers the text bbox,
        // not the padded label rectangle. A symmetric deterministic bbox therefore needs no
        // horizontal translation.
        translate_x: 0.0,
        translate_y: -(text_bbox_y + text_height / 2.0),
        background_x: -background_width / 2.0,
        background_y: text_bbox_y - BACKGROUND_PADDING_PX,
        background_width,
        background_height,
    }
}

fn position_flowchart_edge_label(
    dagre_anchor: crate::model::LayoutPoint,
    geom: &FlowchartEdgePathGeom,
    always_recompute: bool,
) -> crate::model::LayoutPoint {
    let rendered_d = geom
        .emitted_d_for_label
        .as_deref()
        .unwrap_or(geom.d.as_str());
    crate::svg::parity::edge_label_geometry::position_edge_label(
        geom.label_anchor.clone().unwrap_or(dagre_anchor),
        Some(geom.original_label_path_points.as_slice()),
        &geom.label_path_points,
        rendered_d,
        geom.label_path_was_explicitly_updated || always_recompute,
    )
}

pub(in crate::svg::parity::flowchart) fn resolve_flowchart_edge_label_position(
    ctx: &FlowchartRenderCtx<'_>,
    layout_edge: &crate::model::LayoutEdge,
    label: &crate::model::LayoutLabel,
    origin_x: f64,
    origin_y: f64,
    edge_cache: &FxHashMap<&str, FlowchartEdgePathCacheEntry>,
    always_recompute: bool,
) -> crate::model::LayoutPoint {
    let dagre_anchor = crate::model::LayoutPoint {
        x: label.x + ctx.tx - origin_x,
        y: label.y + ctx.ty - origin_y,
    };

    if let Some(geom) = edge_cache
        .get(layout_edge.id.as_str())
        .filter(|entry| {
            (entry.origin_x - origin_x).abs() <= 1e-9 && (entry.origin_y - origin_y).abs() <= 1e-9
        })
        .map(|entry| &entry.geom)
    {
        return position_flowchart_edge_label(dagre_anchor, geom, always_recompute);
    }

    if let Some(midpoint) =
        edge_geom::missing_section_label_position(ctx, layout_edge, origin_x, origin_y)
    {
        return midpoint;
    }

    // Geometry caching only skips routes with fewer than two points. For that degenerate case,
    // `calcLabelPosition` either keeps the anchor (empty) or returns the sole waypoint.
    if always_recompute || layout_edge.to_cluster.is_some() || layout_edge.from_cluster.is_some() {
        let points = layout_edge
            .points
            .iter()
            .map(|point| crate::model::LayoutPoint {
                x: point.x + ctx.tx - origin_x,
                y: point.y + ctx.ty - origin_y,
            })
            .collect::<Vec<_>>();
        return crate::svg::parity::edge_label_geometry::position_edge_label(
            dagre_anchor.clone(),
            (!always_recompute).then_some(points.as_slice()),
            &points,
            "",
            true,
        );
    }

    dagre_anchor
}

pub(in crate::svg::parity) fn render_flowchart_edge_label(
    out: &mut String,
    ctx: &FlowchartRenderCtx<'_>,
    edge: &crate::flowchart::FlowEdge,
    origin_x: f64,
    origin_y: f64,
    edge_cache: &FxHashMap<&str, FlowchartEdgePathCacheEntry>,
) -> Option<crate::model::LayoutPoint> {
    let label_text = ctx.model.edge_label_for_render(edge).unwrap_or_default();
    let label_type = edge.label_type.as_deref().unwrap_or("text");
    let compiled_label_styles = flowchart_compile_styles(
        ctx.class_defs,
        &edge.classes,
        &ctx.default_edge_style,
        &edge.style,
    );
    let edge_metrics_style = crate::flowchart::flowchart_effective_edge_label_text_style(
        &ctx.text_style,
        ctx.class_defs,
        &edge.classes,
        &ctx.default_edge_style,
        &edge.style,
    );
    let svg_edge_label_text_style: Cow<'_, str> =
        if compiled_label_styles.label_style.contains("color:") {
            Cow::Owned(compiled_label_styles.label_style.replace("color:", "fill:"))
        } else {
            Cow::Borrowed(compiled_label_styles.label_style.as_str())
        };
    let prepared_svg_label = (!ctx.edge_html_labels && label_type != "markdown").then(|| {
        let owner = ctx.svg_label_sidecar.and_then(|sidecar| {
            sidecar.edge_owner(edge.id.as_str(), ctx.swimlane_direction.is_some())
        });
        crate::flowchart::FlowchartSvgLabelRenderPlan::new_with_metrics_style(
            ctx.svg_label_sidecar,
            owner,
            label_text,
            ctx.measurer,
            &ctx.text_style,
            edge_metrics_style.as_ref(),
            Some(FLOWCHART_EDGE_LABEL_WRAP_WIDTH),
            true,
            crate::flowchart::FlowchartSvgWidthMode::Bbox,
        )
    });
    let label_text_plain: Cow<'_, str> = prepared_svg_label.as_ref().map_or_else(
        || {
            Cow::Owned(flowchart_label_plain_text(
                label_text,
                label_type,
                ctx.edge_html_labels,
            ))
        },
        |prepared| Cow::Borrowed(prepared.plain_text()),
    );
    let span_style_attr = OptionalStyleXmlAttr(compiled_label_styles.label_style.as_str());
    let div_style_prefix = crate::svg::parity::flowchart::style::flowchart_label_div_style_prefix(
        &compiled_label_styles,
        false,
    );

    fn fallback_midpoint(
        le: &crate::model::LayoutEdge,
        ctx: &FlowchartRenderCtx<'_>,
        origin_x: f64,
        origin_y: f64,
    ) -> (f64, f64) {
        let anchor = crate::model::LayoutPoint {
            x: ctx.tx - origin_x,
            y: ctx.ty - origin_y,
        };
        let points = le
            .points
            .iter()
            .map(|point| crate::model::LayoutPoint {
                x: point.x + ctx.tx - origin_x,
                y: point.y + ctx.ty - origin_y,
            })
            .collect::<Vec<_>>();
        let position = crate::svg::parity::edge_label_geometry::position_edge_label(
            anchor, None, &points, "", true,
        );
        (position.x, position.y)
    }

    if !ctx.edge_html_labels {
        if let Some(le) = ctx.layout_edges_by_id.get(edge.id.as_str()) {
            if let Some(lbl) = le.label.as_ref() {
                let position = resolve_flowchart_edge_label_position(
                    ctx, le, lbl, origin_x, origin_y, edge_cache, false,
                );
                let x = position.x;
                let y = position.y;

                if crate::flowchart::flowchart_label_text_is_empty_for_mode(
                    &label_text_plain,
                    false,
                ) {
                    if !label_text.is_empty() {
                        let label_box = centered_svg_edge_label_box(4.0, 4.0, 0.0);
                        let _ = write!(
                            out,
                            r#"<g class="edgeLabel" transform="translate({},{})"><g class="label" data-id="{}" transform="translate({},{})"><g><rect class="background" style="" x="{}" y="{}" width="{}" height="{}"/>"#,
                            fmt_display(x),
                            fmt_display(y),
                            escape_xml_display(&edge.id),
                            fmt_display(label_box.translate_x),
                            fmt_display(label_box.translate_y),
                            fmt_display(label_box.background_x),
                            fmt_display(label_box.background_y),
                            fmt_display(label_box.background_width),
                            fmt_display(label_box.background_height),
                        );
                        if label_type == "markdown" {
                            write_flowchart_svg_text_markdown_wrapped_centered(
                                out,
                                label_text,
                                true,
                                ctx.measurer,
                                &ctx.text_style,
                                Some(FLOWCHART_EDGE_LABEL_WRAP_WIDTH),
                            );
                        } else {
                            let wrapped = prepared_svg_label
                                .as_ref()
                                .expect("non-Markdown SVG edge labels are prepared before emission")
                                .wrapped_lines();
                            write_flowchart_svg_source_word_lines_centered_with_style(
                                out,
                                &wrapped,
                                svg_edge_label_text_style.as_ref(),
                            );
                        }
                        out.push_str("</g></g></g>");
                        return Some(position);
                    }
                } else {
                    let label_box = centered_svg_edge_label_box(
                        lbl.width,
                        lbl.height,
                        ctx.measurer.measure_svg_create_text_bbox_y_offset_px(
                            &label_text_plain,
                            edge_metrics_style.as_ref(),
                        ),
                    );
                    let _ = write!(
                        out,
                        r#"<g class="edgeLabel" transform="translate({},{})"><g class="label" data-id="{}" transform="translate({},{})"><g><rect class="background" style="" x="{}" y="{}" width="{}" height="{}"/>"#,
                        fmt_display(x),
                        fmt_display(y),
                        escape_xml_display(&edge.id),
                        fmt_display(label_box.translate_x),
                        fmt_display(label_box.translate_y),
                        fmt_display(label_box.background_x),
                        fmt_display(label_box.background_y),
                        fmt_display(label_box.background_width),
                        fmt_display(label_box.background_height)
                    );
                    if label_type == "markdown" {
                        write_flowchart_svg_text_markdown_wrapped_centered(
                            out,
                            label_text,
                            true,
                            ctx.measurer,
                            &ctx.text_style,
                            Some(FLOWCHART_EDGE_LABEL_WRAP_WIDTH),
                        );
                    } else {
                        let wrapped = prepared_svg_label
                            .as_ref()
                            .expect("non-Markdown SVG edge labels are prepared before emission")
                            .wrapped_lines();
                        write_flowchart_svg_source_word_lines_centered_with_style(
                            out,
                            &wrapped,
                            svg_edge_label_text_style.as_ref(),
                        );
                    }
                    out.push_str("</g></g></g>");
                    return Some(position);
                }
            }

            if !crate::flowchart::flowchart_label_text_is_empty_for_mode(&label_text_plain, false) {
                let (x, y) = fallback_midpoint(le, ctx, origin_x, origin_y);
                let metrics = ctx.measurer.measure_wrapped(
                    &label_text_plain,
                    &ctx.text_style,
                    Some(FLOWCHART_EDGE_LABEL_WRAP_WIDTH),
                    crate::text::WrapMode::SvgLike,
                );
                let label_box = centered_svg_edge_label_box(
                    (metrics.width + 4.0).max(1.0),
                    (metrics.height + 4.0).max(1.0),
                    ctx.measurer.measure_svg_create_text_bbox_y_offset_px(
                        &label_text_plain,
                        edge_metrics_style.as_ref(),
                    ),
                );
                let _ = write!(
                    out,
                    r#"<g class="edgeLabel" transform="translate({},{})"><g class="label" data-id="{}" transform="translate({},{})"><g><rect class="background" style="" x="{}" y="{}" width="{}" height="{}"/>"#,
                    fmt_display(x),
                    fmt_display(y),
                    escape_xml_display(&edge.id),
                    fmt_display(label_box.translate_x),
                    fmt_display(label_box.translate_y),
                    fmt_display(label_box.background_x),
                    fmt_display(label_box.background_y),
                    fmt_display(label_box.background_width),
                    fmt_display(label_box.background_height)
                );
                if label_type == "markdown" {
                    write_flowchart_svg_text_markdown_wrapped_centered(
                        out,
                        label_text,
                        true,
                        ctx.measurer,
                        &ctx.text_style,
                        Some(FLOWCHART_EDGE_LABEL_WRAP_WIDTH),
                    );
                } else {
                    let wrapped = prepared_svg_label
                        .as_ref()
                        .expect("non-Markdown SVG edge labels are prepared before emission")
                        .wrapped_lines();
                    write_flowchart_svg_source_word_lines_centered_with_style(
                        out,
                        &wrapped,
                        svg_edge_label_text_style.as_ref(),
                    );
                }
                out.push_str("</g></g></g>");
                return Some(crate::model::LayoutPoint { x, y });
            }
        }

        let _ = write!(
            out,
            r#"<g class="edgeLabel"><g class="label" data-id="{}" transform="translate(0,0)">"#,
            escape_xml_display(&edge.id)
        );
        write_flowchart_empty_svg_text_centered(out, false);
        out.push_str("</g></g>");
        return None;
    }

    let label_html = if crate::flowchart::flowchart_label_is_empty_for_render(label_text) {
        String::new()
    } else {
        flowchart_label_html(label_text, label_type, ctx.config, ctx.math_renderer)
    };

    if let Some(le) = ctx.layout_edges_by_id.get(edge.id.as_str()) {
        if let Some(lbl) = le.label.as_ref() {
            let position = resolve_flowchart_edge_label_position(
                ctx, le, lbl, origin_x, origin_y, edge_cache, false,
            );
            let x = position.x;
            let y = position.y;

            let layout_w = lbl.width.max(0.0);
            let h = lbl.height.max(0.0);
            let background = padded_html_edge_label_background(ctx.edge_label_padding, layout_w, h);
            let wrapped_style = if layout_w >= FLOWCHART_EDGE_LABEL_WRAP_WIDTH - 0.01 {
                format!(
                    "display: table; white-space: break-spaces; line-height: 1.5; max-width: {mw}px; text-align: center; width: {mw}px;",
                    mw = fmt_display(FLOWCHART_EDGE_LABEL_WRAP_WIDTH)
                )
            } else {
                "display: table-cell; white-space: nowrap; line-height: 1.5; max-width: 200px; text-align: center;".to_string()
            };
            let div_style = if div_style_prefix.is_empty() {
                wrapped_style
            } else {
                format!("{div_style_prefix}{wrapped_style}")
            };
            let _ = write!(
                out,
                r#"<g class="edgeLabel" transform="translate({},{})">{}<g class="label" data-id="{}" transform="translate({},{})"><foreignObject width="{}" height="{}"{}><div xmlns="http://www.w3.org/1999/xhtml" class="labelBkg" style="{}"><span class="edgeLabel"{}>{}</span></div></foreignObject></g></g>"#,
                fmt_display(x),
                fmt_display(y),
                background,
                escape_xml_display(&edge.id),
                fmt_display(-layout_w / 2.0),
                fmt_display(-h / 2.0),
                fmt_display(layout_w),
                fmt_display(h),
                HTML_LABEL_FOREIGN_OBJECT_OVERFLOW_ATTR,
                escape_xml_display(&div_style),
                span_style_attr,
                label_html
            );
            return Some(position);
        }

        if !crate::flowchart::flowchart_label_text_is_empty_for_mode(
            &label_text_plain,
            ctx.edge_html_labels,
        ) {
            let (x, y) = fallback_midpoint(le, ctx, origin_x, origin_y);
            let has_inline_style_tags = if label_type == "markdown" {
                false
            } else {
                let lower = label_text.to_ascii_lowercase();
                crate::text::flowchart_html_has_inline_style_tags(&lower)
            };

            let metrics = if label_type == "markdown" {
                crate::text::measure_markdown_with_inline_styles(
                    ctx.measurer,
                    label_text,
                    &ctx.text_style,
                    Some(FLOWCHART_EDGE_LABEL_WRAP_WIDTH),
                    ctx.edge_wrap_mode,
                )
            } else if has_inline_style_tags {
                crate::text::measure_html_with_inline_styles(
                    ctx.measurer,
                    label_text,
                    &ctx.text_style,
                    Some(FLOWCHART_EDGE_LABEL_WRAP_WIDTH),
                    ctx.edge_wrap_mode,
                )
            } else {
                ctx.measurer.measure_wrapped(
                    &label_text_plain,
                    &ctx.text_style,
                    Some(FLOWCHART_EDGE_LABEL_WRAP_WIDTH),
                    ctx.edge_wrap_mode,
                )
            };
            let layout_w = metrics.width.max(1.0);
            let h = metrics.height.max(1.0);
            let background = padded_html_edge_label_background(ctx.edge_label_padding, layout_w, h);
            let wrapped_style = if layout_w >= FLOWCHART_EDGE_LABEL_WRAP_WIDTH - 0.01 {
                format!(
                    "display: table; white-space: break-spaces; line-height: 1.5; max-width: {mw}px; text-align: center; width: {mw}px;",
                    mw = fmt_display(FLOWCHART_EDGE_LABEL_WRAP_WIDTH)
                )
            } else {
                "display: table-cell; white-space: nowrap; line-height: 1.5; max-width: 200px; text-align: center;".to_string()
            };
            let div_style = if div_style_prefix.is_empty() {
                wrapped_style
            } else {
                format!("{div_style_prefix}{wrapped_style}")
            };
            let _ = write!(
                out,
                r#"<g class="edgeLabel" transform="translate({},{})">{}<g class="label" data-id="{}" transform="translate({},{})"><foreignObject width="{}" height="{}"{}><div xmlns="http://www.w3.org/1999/xhtml" class="labelBkg" style="{}"><span class="edgeLabel"{}>{}</span></div></foreignObject></g></g>"#,
                fmt_display(x),
                fmt_display(y),
                background,
                escape_xml_display(&edge.id),
                fmt_display(-layout_w / 2.0),
                fmt_display(-h / 2.0),
                fmt_display(layout_w),
                fmt_display(h.max(0.0)),
                HTML_LABEL_FOREIGN_OBJECT_OVERFLOW_ATTR,
                escape_xml_display(&div_style),
                span_style_attr,
                label_html
            );
            return Some(crate::model::LayoutPoint { x, y });
        }
    }

    let _ = write!(
        out,
        r#"<g class="edgeLabel"><g class="label" data-id="{}" transform="translate(0,0)"><foreignObject width="0" height="0"><div xmlns="http://www.w3.org/1999/xhtml" class="labelBkg" style="{}display: table-cell; white-space: nowrap; line-height: 1.5; max-width: 200px; text-align: center;"><span class="edgeLabel"{}></span></div></foreignObject></g></g>"#,
        escape_xml_display(&edge.id),
        escape_xml_display(&div_style_prefix),
        span_style_attr
    );
    None
}

pub(in crate::svg::parity::flowchart) fn render_swimlane_edge_label_node(
    out: &mut String,
    ctx: &FlowchartRenderCtx<'_>,
    node_id: &str,
    edge: &crate::flowchart::FlowEdge,
    origin_x: f64,
    origin_y: f64,
    edge_cache: &FxHashMap<&str, FlowchartEdgePathCacheEntry>,
) -> Option<crate::model::LayoutPoint> {
    let layout_edge = ctx.layout_edges_by_id.get(edge.id.as_str())?;
    let label = layout_edge.label.as_ref()?;

    let label_text = ctx.model.edge_label_for_render(edge).unwrap_or_default();
    // Mermaid's Swimlane adapter creates a fresh `labelRect` without copying `edge.labelType`.
    // The node renderer therefore follows ordinary non-Markdown createText semantics.
    let label_type = "text";
    let first_label_style = ctx
        .default_edge_style
        .first()
        .or_else(|| edge.style.first())
        .map_or("", String::as_str);
    let label_text_style = crate::flowchart::flowchart_swimlane_label_rect_text_style(
        &ctx.text_style,
        &ctx.default_edge_style,
        &edge.style,
    );
    // Swimlane's private `positionEdgeLabel` recomputes whenever `insertEdge` returns a paths
    // object, independent of the ordinary updated-path heuristic.
    let position = resolve_flowchart_edge_label_position(
        ctx,
        layout_edge,
        label,
        origin_x,
        origin_y,
        edge_cache,
        true,
    );
    let x = position.x;
    let y = position.y;
    let width = label.width.max(0.0);
    let height = label.height.max(0.0);

    let _ = write!(
        out,
        r#"<g class="label edgeLabel" id="{}" transform="translate({}, {})"><rect width="0.1" height="0.1"/><g class="label"{} transform="translate({}, {})"><rect/>"#,
        escape_xml_display(node_id),
        fmt_display(x),
        fmt_display(y),
        OptionalStyleXmlAttr(first_label_style),
        fmt_display(-width / 2.0),
        fmt_display(-height / 2.0),
    );

    if ctx.node_html_labels {
        let label_html =
            flowchart_label_html(label_text, label_type, ctx.config, ctx.math_renderer);
        // `createText` maps only the first literal `fill:` occurrence to `color:` before applying
        // the copied label style to the HTML span and div. Later fixed declarations override the
        // corresponding inline properties exactly as Mermaid's chained D3 `.style()` calls do.
        let html_label_style = first_label_style.replacen("fill:", "color:", 1);
        let mut div_style = String::new();
        if !html_label_style.trim().is_empty() {
            div_style.push_str(html_label_style.trim());
            div_style.push(';');
        }
        let _ = write!(
            &mut div_style,
            "display: table-cell; white-space: nowrap; line-height: 1.5; max-width: {}px; text-align: center;",
            fmt_display(ctx.wrapping_width),
        );
        let _ = write!(
            out,
            r#"<foreignObject width="{}" height="{}"><div xmlns="http://www.w3.org/1999/xhtml" style="{}"><span class="nodeLabel"{}>{}</span></div></foreignObject></g></g>"#,
            fmt_display(width),
            fmt_display(height),
            escape_xml_display(&div_style),
            OptionalStyleXmlAttr(&html_label_style),
            label_html,
        );
        return Some(position);
    }

    out.push_str(r#"<g><rect class="background" style="stroke: none"/>"#);
    let owner = ctx
        .svg_label_sidecar
        .and_then(|sidecar| sidecar.edge_owner(edge.id.as_str(), true));
    let prepared = crate::flowchart::FlowchartSvgLabelRenderPlan::new(
        ctx.svg_label_sidecar,
        owner,
        label_text,
        ctx.measurer,
        label_text_style.as_ref(),
        Some(ctx.wrapping_width),
        true,
        crate::flowchart::FlowchartSvgWidthMode::Bbox,
    );
    let wrapped = prepared.wrapped_lines();
    write_flowchart_svg_source_word_lines(out, &wrapped, true);
    out.push_str("</g></g></g>");
    Some(position)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn point(x: f64, y: f64) -> crate::model::LayoutPoint {
        crate::model::LayoutPoint { x, y }
    }

    fn geom(d: &str, points: Vec<crate::model::LayoutPoint>) -> FlowchartEdgePathGeom {
        FlowchartEdgePathGeom {
            d: d.to_string(),
            pb: None,
            data_points: points.clone(),
            data_points_b64: String::new(),
            original_path_length: None,
            path_length: None,
            line_hop_applied: false,
            original_label_path_points: points.clone(),
            label_path_points: points,
            label_anchor: None,
            label_path_was_explicitly_updated: false,
            emitted_d_for_label: None,
            bounds_skipped_for_viewbox: false,
        }
    }

    #[test]
    fn svg_edge_label_box_centers_text_and_its_padded_background() {
        let label_box = centered_svg_edge_label_box(104.0, 24.0, 3.0);
        assert_eq!(label_box.translate_x, 0.0);
        assert_eq!(label_box.translate_y, -13.0);
        assert_eq!(label_box.background_x, -52.0);
        assert_eq!(label_box.background_y, 1.0);
        assert_eq!(label_box.background_width, 104.0);
        assert_eq!(label_box.background_height, 24.0);

        let translated_background_center_y =
            label_box.translate_y + label_box.background_y + label_box.background_height / 2.0;
        assert_eq!(translated_background_center_y, 0.0);
    }

    #[test]
    fn ordinary_dagre_keeps_anchor_when_paint_does_not_move_the_route_midpoint() {
        let anchor = point(4.0, 5.0);
        let mut geometry = geom(
            "M0,0L10,0L20,0",
            vec![point(0.0, 0.0), point(10.0, 0.0), point(20.0, 0.0)],
        );

        let unchanged = position_flowchart_edge_label(anchor.clone(), &geometry, false);
        assert_eq!((unchanged.x, unchanged.y), (4.0, 5.0));

        geometry.label_path_was_explicitly_updated = true;
        let updated = position_flowchart_edge_label(anchor, &geometry, false);
        assert_eq!((updated.x, updated.y), (4.0, 5.0));
    }

    #[test]
    fn cluster_cut_moves_the_anchor_by_the_midpoint_delta() {
        let mut geometry = geom("M0,0L100,0", vec![point(0.0, 0.0), point(100.0, 0.0)]);
        geometry.original_label_path_points = vec![point(-20.0, 0.0), point(100.0, 0.0)];
        geometry.label_path_was_explicitly_updated = true;
        let label = position_flowchart_edge_label(point(12.0, 8.0), &geometry, false);
        assert_eq!((label.x, label.y), (22.0, 8.0));
    }

    #[test]
    fn elk_paints_the_projected_anchor_without_recentering_on_the_whole_route() {
        let mut geometry = geom(
            "M0,0L100,0L100,100",
            vec![point(0.0, 0.0), point(100.0, 0.0), point(100.0, 100.0)],
        );
        geometry.label_anchor = Some(point(60.0, 0.0));
        let label = position_flowchart_edge_label(point(60.0, 10.0), &geometry, false);
        assert_eq!((label.x, label.y), (60.0, 0.0));
        assert_ne!((label.x, label.y), (100.0, 0.0));
    }

    #[test]
    fn swimlane_recomputes_from_returned_waypoints_by_delta() {
        let mut geometry = geom(
            "M0,0L10,0L20,0",
            vec![point(0.0, 0.0), point(10.0, 0.0), point(20.0, 0.0)],
        );
        geometry.original_label_path_points =
            vec![point(-20.0, 0.0), point(0.0, 0.0), point(20.0, 0.0)];
        let positioned = position_flowchart_edge_label(point(4.0, 5.0), &geometry, true);
        assert_eq!((positioned.x, positioned.y), (14.0, 5.0));
    }

    #[test]
    fn rough_actual_emitted_path_drives_updated_path_detection() {
        let anchor = point(4.0, 5.0);
        let mut geometry = geom(
            "M0,0L10,20L30,20",
            vec![point(0.0, 0.0), point(10.0, 20.0), point(30.0, 20.0)],
        );

        let logical_curve = position_flowchart_edge_label(anchor.clone(), &geometry, false);
        assert_eq!((logical_curve.x, logical_curve.y), (4.0, 5.0));

        geometry.emitted_d_for_label = Some("M1.1,2.2C3.3,4.4,5.5,6.6,7.7,8.8".to_string());
        let rough_curve = position_flowchart_edge_label(anchor, &geometry, false);
        assert_eq!((rough_curve.x, rough_curve.y), (4.0, 5.0));
    }

    #[test]
    fn shared_geometry_handles_empty_single_degenerate_and_polyline_paths() {
        let anchor = point(4.0, 5.0);
        let empty = geom("", Vec::new());
        let positioned = position_flowchart_edge_label(anchor.clone(), &empty, true);
        assert_eq!((positioned.x, positioned.y), (4.0, 5.0));

        let single = geom("M2,3", vec![point(2.0, 3.0)]);
        let positioned = position_flowchart_edge_label(anchor.clone(), &single, true);
        assert_eq!((positioned.x, positioned.y), (4.0, 5.0));

        let degenerate = geom("M2,3L2,3", vec![point(2.0, 3.0), point(2.0, 3.0)]);
        let positioned = position_flowchart_edge_label(anchor.clone(), &degenerate, true);
        assert_eq!((positioned.x, positioned.y), (4.0, 5.0));

        let polyline = geom(
            "M0,0L6,0L6,8",
            vec![point(0.0, 0.0), point(6.0, 0.0), point(6.0, 8.0)],
        );
        let positioned = position_flowchart_edge_label(anchor, &polyline, true);
        assert_eq!((positioned.x, positioned.y), (4.0, 5.0));
    }
}
