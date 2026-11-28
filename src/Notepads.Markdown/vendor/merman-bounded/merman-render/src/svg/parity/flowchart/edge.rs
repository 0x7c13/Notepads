// Modifications Copyright (c) 2026, Jiaqi (0x7c13) Liu. See project LICENSE.
//! Flowchart edge helpers (markers, class attr, marker-color resolution).


pub(super) fn flowchart_edge_marker_end_base(
    edge: &crate::flowchart::FlowEdge,
) -> Option<&'static str> {
    match edge.edge_type.as_deref() {
        Some("double_arrow_point") => Some("pointEnd"),
        Some("double_arrow_circle") => Some("circleEnd"),
        Some("double_arrow_cross") => Some("crossEnd"),
        Some("arrow_point") => Some("pointEnd"),
        Some("arrow_cross") => Some("crossEnd"),
        Some("arrow_circle") => Some("circleEnd"),
        Some("arrow_open") => None,
        _ => Some("pointEnd"),
    }
}

pub(super) fn flowchart_edge_marker_start_base(
    edge: &crate::flowchart::FlowEdge,
) -> Option<&'static str> {
    match edge.edge_type.as_deref() {
        Some("double_arrow_point") => Some("pointStart"),
        Some("double_arrow_circle") => Some("circleStart"),
        Some("double_arrow_cross") => Some("crossStart"),
        _ => None,
    }
}

pub(super) fn flowchart_resolve_stroke_for_marker(
    class_defs: &merman_core::native_collections::OrderedMap<String, Vec<String>>,
    classes: &[String],
    default_edge_style: &[String],
    edge_style: &[String],
) -> Option<String> {
    // Marker ids only depend on the resolved `stroke` value, so avoid allocating the full
    // `FlowchartCompiledStyles` (ordered map + joined style strings) here.
    let mut stroke: Option<&str> = None;

    for c in classes {
        let Some(decls) = class_defs.get(c) else {
            continue;
        };
        for d in decls {
            let Some((k, v)) = super::parse_style_decl(d) else {
                continue;
            };
            if k == "stroke" {
                stroke = Some(v);
            }
        }
    }

    for d in default_edge_style.iter().chain(edge_style.iter()) {
        let Some((k, v)) = super::parse_style_decl(d) else {
            continue;
        };
        if k == "stroke" {
            stroke = Some(v);
        }
    }

    stroke.map(|s| s.to_string())
}

pub(super) fn flowchart_edge_is_animated(
    ctx: &super::FlowchartRenderCtx<'_>,
    edge: &crate::flowchart::FlowEdge,
) -> bool {
    edge.animate == Some(true)
        || edge.animation.is_some()
        || edge
            .classes
            .iter()
            .filter_map(|class| ctx.class_defs.get(class))
            .flatten()
            .any(|declaration| declaration.contains("animation"))
}
