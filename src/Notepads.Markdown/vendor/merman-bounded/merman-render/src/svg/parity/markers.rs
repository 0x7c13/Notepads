//! Shared Mermaid edge-marker DOM emission.
//!
//! Base markers and colored clones use the same source-backed geometry.

use std::fmt::Write as _;

use super::SvgDiagramId;
use super::util::escape_xml_display;

pub(in crate::svg::parity) fn push_base_edge_markers(
    out: &mut String,
    diagram_id: SvgDiagramId<'_>,
    diagram_type: &str,
) {
    for ends in [
        ["pointEnd", "pointStart"],
        ["circleEnd", "circleStart"],
        ["crossEnd", "crossStart"],
    ] {
        for margin in [false, true] {
            for base in ends {
                push_edge_marker(out, diagram_id, diagram_type, base, margin, None);
            }
        }
    }
    if diagram_type == "agentflow" {
        // Agentflow registers hierarchy markers after the ordinary edge marker set.  These are
        // emitted even when a fixture has no hierarchy edge because Mermaid's marker registry is
        // populated eagerly by the family renderer.
        push_edge_marker(out, diagram_id, diagram_type, "hierarchyEnd", false, None);
        push_edge_marker(out, diagram_id, diagram_type, "hierarchyStart", false, None);
    }
}

/// `color` carries the raw-token ID suffix and the sanitized SVG color value separately.
pub(in crate::svg::parity) fn push_edge_marker(
    out: &mut String,
    diagram_id: SvgDiagramId<'_>,
    diagram_type: &str,
    base: &str,
    margin: bool,
    color: Option<(&str, Option<&str>)>,
) {
    let (attrs, shape, shape_attrs) = match (base, margin) {
        ("pointEnd", false) => (
            r#"viewBox="0 0 10 10" refX="5" refY="5" markerUnits="userSpaceOnUse" markerWidth="8" markerHeight="8" orient="auto""#,
            "path",
            r#"d="M 0 0 L 10 5 L 0 10 z" class="arrowMarkerPath" style="stroke-width: 1; stroke-dasharray: 1, 0;""#,
        ),
        ("pointStart", false) => (
            r#"viewBox="0 0 10 10" refX="4.5" refY="5" markerUnits="userSpaceOnUse" markerWidth="8" markerHeight="8" orient="auto""#,
            "path",
            r#"d="M 0 5 L 10 10 L 10 0 z" class="arrowMarkerPath" style="stroke-width: 1; stroke-dasharray: 1, 0;""#,
        ),
        ("pointEnd", true) => (
            r#"viewBox="0 0 11.5 14" refX="11.5" refY="7" markerUnits="userSpaceOnUse" markerWidth="10.5" markerHeight="14" orient="auto""#,
            "path",
            r#"d="M 0 0 L 11.5 7 L 0 14 z" class="arrowMarkerPath" style="stroke-width: 0; stroke-dasharray: 1, 0;""#,
        ),
        ("pointStart", true) => (
            r#"viewBox="0 0 11.5 14" refX="1" refY="7" markerUnits="userSpaceOnUse" markerWidth="11.5" markerHeight="14" orient="auto""#,
            "polygon",
            r#"points="0,7 11.5,14 11.5,0" class="arrowMarkerPath" style="stroke-width: 0; stroke-dasharray: 1, 0;""#,
        ),
        ("circleEnd", false) => (
            r#"viewBox="0 0 10 10" refX="11" refY="5" markerUnits="userSpaceOnUse" markerWidth="11" markerHeight="11" orient="auto""#,
            "circle",
            r#"cx="5" cy="5" r="5" class="arrowMarkerPath" style="stroke-width: 1; stroke-dasharray: 1, 0;""#,
        ),
        ("circleStart", false) => (
            r#"viewBox="0 0 10 10" refX="-1" refY="5" markerUnits="userSpaceOnUse" markerWidth="11" markerHeight="11" orient="auto""#,
            "circle",
            r#"cx="5" cy="5" r="5" class="arrowMarkerPath" style="stroke-width: 1; stroke-dasharray: 1, 0;""#,
        ),
        ("circleEnd", true) => (
            r#"viewBox="0 0 10 10" refY="5" refX="12.25" markerUnits="userSpaceOnUse" markerWidth="14" markerHeight="14" orient="auto""#,
            "circle",
            r#"cx="5" cy="5" r="5" class="arrowMarkerPath" style="stroke-width: 0; stroke-dasharray: 1, 0;""#,
        ),
        ("circleStart", true) => (
            r#"viewBox="0 0 10 10" refX="-2" refY="5" markerUnits="userSpaceOnUse" markerWidth="14" markerHeight="14" orient="auto""#,
            "circle",
            r#"cx="5" cy="5" r="5" class="arrowMarkerPath" style="stroke-width: 0; stroke-dasharray: 1, 0;""#,
        ),
        ("crossEnd", false) => (
            r#"viewBox="0 0 11 11" refX="12" refY="5.2" markerUnits="userSpaceOnUse" markerWidth="11" markerHeight="11" orient="auto""#,
            "path",
            r#"d="M 1,1 l 9,9 M 10,1 l -9,9" class="arrowMarkerPath" style="stroke-width: 2; stroke-dasharray: 1, 0;""#,
        ),
        ("crossStart", false) => (
            r#"viewBox="0 0 11 11" refX="-1" refY="5.2" markerUnits="userSpaceOnUse" markerWidth="11" markerHeight="11" orient="auto""#,
            "path",
            r#"d="M 1,1 l 9,9 M 10,1 l -9,9" class="arrowMarkerPath" style="stroke-width: 2; stroke-dasharray: 1, 0;""#,
        ),
        ("crossEnd", true) => (
            r#"viewBox="0 0 15 15" refX="17.7" refY="7.5" markerUnits="userSpaceOnUse" markerWidth="12" markerHeight="12" orient="auto""#,
            "path",
            r#"d="M 1,1 L 14,14 M 1,14 L 14,1" class="arrowMarkerPath" style="stroke-width: 2.5;""#,
        ),
        ("crossStart", true) => (
            r#"viewBox="0 0 15 15" refX="-3.5" refY="7.5" markerUnits="userSpaceOnUse" markerWidth="12" markerHeight="12" orient="auto""#,
            "path",
            r#"d="M 1,1 L 14,14 M 1,14 L 14,1" class="arrowMarkerPath" style="stroke-width: 2.5; stroke-dasharray: 1, 0;""#,
        ),
        ("hierarchyEnd", false) => (
            r#"viewBox="0 0 12 10" refX="10" refY="5" markerUnits="userSpaceOnUse" markerWidth="12" markerHeight="10" orient="auto""#,
            "path",
            r#"d="M 0 0 L 6 5 L 0 10 M 4 0 L 10 5 L 4 10" class="arrowMarkerPath" style="stroke-width: 1; stroke-dasharray: 1, 0; fill: none;""#,
        ),
        ("hierarchyStart", false) => (
            r#"viewBox="0 0 12 10" refX="2" refY="5" markerUnits="userSpaceOnUse" markerWidth="12" markerHeight="10" orient="auto""#,
            "path",
            r#"d="M 12 0 L 6 5 L 12 10 M 8 0 L 2 5 L 8 10" class="arrowMarkerPath" style="stroke-width: 1; stroke-dasharray: 1, 0; fill: none;""#,
        ),
        _ => return,
    };
    let _ = write!(
        out,
        r#"<marker id="{}_{}-{}{}"#,
        diagram_id,
        escape_xml_display(diagram_type),
        base,
        if margin { "-margin" } else { "" }
    );
    if let Some((color_id, _)) = color {
        let _ = write!(out, "_{}", escape_xml_display(color_id));
    }
    let _ = write!(
        out,
        r#"" class="marker {}{}" {}><{} {}"#,
        if base.starts_with("cross") {
            "cross "
        } else if base.starts_with("hierarchy") {
            "hierarchy "
        } else {
            ""
        },
        escape_xml_display(diagram_type),
        attrs,
        shape,
        shape_attrs
    );
    // Mermaid clones only path/circle/line children; the Neo start-point polygon stays intact.
    if shape != "polygon"
        && let Some((_, Some(color))) = color
    {
        let _ = write!(out, r#" stroke="{}""#, escape_xml_display(color));
        if base.starts_with("point") {
            let _ = write!(out, r#" fill="{}""#, escape_xml_display(color));
        }
    }
    out.push_str("/></marker>");
}
