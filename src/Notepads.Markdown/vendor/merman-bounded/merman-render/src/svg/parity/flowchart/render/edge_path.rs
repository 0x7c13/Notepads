//! Flowchart edge path renderer.

use super::super::defs::write_flowchart_marker_id_xml;
use super::super::*;
#[cfg(test)]
use merman_core::diagrams::flowchart::{FlowEdgeMarker, FlowEdgeStroke, FlowEdgeVisibility};

pub(in crate::svg::parity::flowchart) fn render_flowchart_edge_path(
    out: &mut String,
    ctx: &FlowchartRenderCtx<'_>,
    edge: &crate::flowchart::FlowEdge,
    origin_x: f64,
    origin_y: f64,
    scratch: &mut FlowchartEdgeDataPointsScratch,
    edge_cache: &mut FxHashMap<&str, FlowchartEdgePathCacheEntry>,
) -> crate::Result<()> {
    let trace_enabled = ctx.trace_edge_id.is_some_and(|id| id == edge.id.as_str());

    let cached_geom = edge_cache
        .get(edge.id.as_str())
        .filter(|c| (c.origin_x - origin_x).abs() <= 1e-9 && (c.origin_y - origin_y).abs() <= 1e-9)
        .map(|c| &c.geom);

    // Trace collection recomputes the geometry before graph-wide postprocessing for diagnostics, but the emitted SVG
    // must still consume the post-processed cache. Enabling diagnostics must not alter rendering.
    let owned_geom = if cached_geom.is_none() || trace_enabled {
        if let Some(layout_edge) = ctx.layout_edges_by_id.get(edge.id.as_str()) {
            ctx.work_meter.charge(layout_edge.points.len().max(2))?;
        }
        flowchart_compute_edge_path_geom(
            FlowchartEdgePathGeomRequest {
                ctx,
                edge,
                origin_x,
                origin_y,
                trace_enabled,
            },
            scratch,
        )
    } else {
        None
    };
    let geom = if let Some(g) = cached_geom {
        g
    } else {
        let Some(g) = owned_geom.as_ref() else {
            return Ok(());
        };
        g
    };
    let d = geom.d.as_str();
    let data_points_b64 = geom.data_points_b64.as_str();
    let data_look = flowchart_config_look(ctx.config);
    let hand_drawn = data_look == "handDrawn";
    let rough_d = if hand_drawn && !geom.line_hop_applied {
        super::node::roughjs::roughjs_hand_drawn_stroke_path_for_svg_path(
            d,
            0.3,
            &ctx.hand_drawn_seed,
        )
    } else {
        None
    };
    let d = rough_d.as_deref().unwrap_or(d);

    let mut marker_color: Option<&str> = None;
    for raw in ctx.default_edge_style.iter().chain(edge.style.iter()) {
        // Mirror Mermaid: handDrawn passes the full `stroke:...` style token to edgeMarker,
        // while classic/neo passes the captured color value from final pathStyle.
        let s = raw.trim_start();
        let Some(rest) = s.strip_prefix("stroke:") else {
            continue;
        };
        if !rest.trim().is_empty() {
            marker_color = Some(if hand_drawn { s } else { rest });
            break;
        }
    }

    // If no inline `stroke:` exists, Mermaid still colors markers based on class-derived stroke
    // styles (see `edges.js` `stylesFromClasses` + `edgeMarker.ts` `strokeColor` extraction).
    // We approximate this by compiling the edge styles using class defs and reusing the resulting
    // `stroke` value for the marker id suffix.
    let compiled_marker_color = if !hand_drawn && marker_color.is_none() && !edge.classes.is_empty()
    {
        flowchart_resolve_stroke_for_marker(
            ctx.class_defs,
            &edge.classes,
            &ctx.default_edge_style,
            &edge.style,
        )
    } else {
        None
    };
    if marker_color.is_none() {
        marker_color = compiled_marker_color.as_deref();
    }

    fn write_style_joined(out: &mut String, a: &[String], b: &[String]) {
        let mut first = true;
        for part in a.iter().chain(b.iter()) {
            if first {
                first = false;
            } else {
                out.push(';');
            }
            // Decode each original token before adding CSS delimiters: a trailing semicolon
            // would make a literal hex color look like a Mermaid entity (#abcdef;).
            out.push_str(crate::entities::decode_mermaid_entities_for_render_text(part).as_ref());
        }
    }

    let _ = write!(
        out,
        r#"<path d="{}" id="{}-{}" class=""#,
        d,
        ctx.diagram_id,
        escape_xml_display(&edge.id),
    );
    ctx.checkpoint_emit()?;
    css::write_flowchart_edge_class_attr(out, edge);
    if hand_drawn {
        out.push_str(" transition");
    }
    out.push_str(r#"" style=""#);
    scratch.edge_style.clear();
    if data_look == "neo"
        && !flowchart_edge_is_animated(ctx, edge)
        && let Some(path_length) = geom.original_path_length
    {
        write_flowchart_neo_edge_mask(&mut scratch.edge_style, path_length, edge);
    }
    if hand_drawn {
        write_style_joined(
            &mut scratch.edge_style,
            &ctx.default_edge_style,
            &edge.style,
        );
    } else if ctx.default_edge_style.is_empty() && edge.style.is_empty() {
        scratch.edge_style.push(';');
    } else {
        let style_start = scratch.edge_style.len();
        write_style_joined(
            &mut scratch.edge_style,
            &ctx.default_edge_style,
            &edge.style,
        );
        let style_end = scratch.edge_style.len();
        scratch.edge_style.push_str(";;;");
        scratch
            .edge_style
            .extend_from_within(style_start..style_end);
    }
    // lineJump.ts rewrites the complete painted style after replacing the path. In
    // particular, its first four dash numbers can come from a repeated dotted pattern.
    let style = if geom.line_hop_applied {
        crate::svg::parity::line_hops::rewrite_style_after_line_hop(
            &scratch.edge_style,
            d,
            ctx.work_meter,
        )?
    } else {
        std::borrow::Cow::Borrowed(scratch.edge_style.as_str())
    };
    let _ = write!(out, "{}", escape_attr_display(&style));
    if hand_drawn {
        out.push_str(r##"" stroke="#000" stroke-width="1" fill="none"##);
    }
    let _ = write!(
        out,
        r#"" data-edge="true" data-et="edge" data-id="{}" data-points="{}" data-look="{}""#,
        escape_xml_display(&edge.id),
        data_points_b64,
        escape_xml_display(data_look),
    );
    if let Some(base) = flowchart_edge_marker_start_base(edge) {
        out.push_str(r#" marker-start="url(#"#);
        write_flowchart_marker_id_xml(
            out,
            ctx.diagram_id,
            ctx.diagram_type,
            base,
            data_look == "neo" && !flowchart_edge_is_animated(ctx, edge),
            marker_color,
        );
        out.push_str(r#")""#);
    }
    if let Some(base) = flowchart_edge_marker_end_base(edge) {
        out.push_str(r#" marker-end="url(#"#);
        write_flowchart_marker_id_xml(
            out,
            ctx.diagram_id,
            ctx.diagram_type,
            base,
            data_look == "neo" && !flowchart_edge_is_animated(ctx, edge),
            marker_color,
        );
        out.push_str(r#")""#);
    }
    out.push_str(" />");

    if let Some(emitted_d_for_label) = rough_d
        && let Some(cache_entry) = edge_cache.get_mut(edge.id.as_str())
        && (cache_entry.origin_x - origin_x).abs() <= 1e-9
        && (cache_entry.origin_y - origin_y).abs() <= 1e-9
    {
        cache_entry.geom.emitted_d_for_label = Some(emitted_d_for_label);
    }
    Ok(())
}

fn write_flowchart_neo_edge_mask(
    out: &mut String,
    path_length: f64,
    edge: &crate::flowchart::FlowEdge,
) {
    let (arrow_type_start, arrow_type_end) =
        super::super::edge_geom::arrow_types_for_edge(edge.edge_type.as_deref());
    crate::svg::parity::edge_path::write_neo_edge_mask(
        out,
        path_length,
        arrow_type_start,
        arrow_type_end,
        matches!(edge.stroke.as_deref(), Some("dotted" | "dashed")),
        false,
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    fn edge(edge_type: &str, stroke: &str) -> crate::flowchart::FlowEdge {
        crate::flowchart::FlowEdge {
            id: "edge".to_string(),
            from: "A".to_string(),
            to: "B".to_string(),
            label: None,
            label_type: None,
            edge_type: Some(edge_type.to_string()),
            arrow: String::new(),
            start_marker: FlowEdgeMarker::None,
            end_marker: if edge_type.starts_with("arrow_open") {
                FlowEdgeMarker::None
            } else {
                FlowEdgeMarker::Point
            },
            is_user_defined_id: false,
            stroke: Some(stroke.to_string()),
            stroke_kind: match stroke {
                "dotted" => FlowEdgeStroke::Dotted,
                "thick" => FlowEdgeStroke::Thick,
                _ => FlowEdgeStroke::Normal,
            },
            visibility: if stroke == "invisible" {
                FlowEdgeVisibility::Invisible
            } else {
                FlowEdgeVisibility::Visible
            },
            interpolate: None,
            classes: Vec::new(),
            style: Vec::new(),
            animate: None,
            animation: None,
            length: 1,
        }
    }

    #[test]
    fn neo_solid_mask_uses_original_length_and_marker_offsets() {
        let mut style = String::new();
        write_flowchart_neo_edge_mask(&mut style, 40.0, &edge("arrow_point", "normal"));
        assert_eq!(style, "stroke-dasharray: 0 0 36 4; stroke-dashoffset: 0;");

        style.clear();
        write_flowchart_neo_edge_mask(&mut style, 50.0, &edge("double_arrow_circle", "normal"));
        assert_eq!(
            style,
            "stroke-dasharray: 0 12.5 25 12.5; stroke-dashoffset: 0;"
        );
    }

    #[test]
    fn neo_dotted_mask_preserves_upstream_two_pixel_pattern() {
        let mut style = String::new();
        write_flowchart_neo_edge_mask(&mut style, 16.0, &edge("arrow_open", "dotted"));
        assert_eq!(
            style,
            "stroke-dasharray: 0 0 2 2 2 2 2 2 2 2 0; stroke-dashoffset: 0;"
        );
    }
}
