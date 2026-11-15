//! Flowchart v2 rounded rectangle shape.

use std::fmt::Write as _;

use crate::svg::parity::flowchart::escape_attr;
use crate::svg::parity::{fmt, fmt_display};

use super::super::roughjs::roughjs_hachure_paths_for_svg_path;

const FLOWCHART_NODE_HAND_DRAWN_ROUGHNESS: f32 = 0.7;
const FLOWCHART_NODE_HAND_DRAWN_FILL_WEIGHT: f32 = 1.5;
const FLOWCHART_NODE_HAND_DRAWN_HACHURE_GAP: f32 = 1.5;

pub(in crate::svg::parity::flowchart::render::node) fn render_rounded_rect(
    out: &mut String,
    ctx: &crate::svg::parity::flowchart::types::FlowchartRenderCtx<'_>,
    common: &super::super::FlowchartNodeRenderCommon<'_>,
    details: &mut crate::svg::parity::flowchart::types::FlowchartRenderDetails,
) {
    let w = common.layout_node.width.max(1.0);
    let h = common.layout_node.height.max(1.0);
    // roundedRect.ts resolves radius from the effective theme. drawRect only applies
    // truthy overrides: numeric zero keeps Flowchart's absent rx/ry, while "0" applies.
    let config = ctx.config.as_value();
    let default_radius = serde_json::json!(5);
    let radius = config
        .get("themeVariables")
        .and_then(|theme| theme.get("radius"))
        .filter(|radius| !radius.is_null())
        .unwrap_or(&default_radius);
    let radius = crate::config::json_value_is_truthy(radius).then_some(radius);
    if common.look_is_hand_drawn() && radius.is_none() {
        // drawRect uses rc.rectangle when neither resolved radius is truthy.
        super::render_process_rectangle(out, common, details);
        return;
    }

    let rough_paths = if common.look_is_hand_drawn() {
        // Preserve the source SVG arcs, including radii larger than half the box.
        let path_data = rounded_rect_path_data(w, h, radius.unwrap_or(&default_radius));
        super::super::helpers::timed_node_roughjs(common.timing, details, || {
            roughjs_hachure_paths_for_svg_path(
                &path_data,
                common.stroke_width,
                common.stroke_dasharray,
                FLOWCHART_NODE_HAND_DRAWN_FILL_WEIGHT,
                FLOWCHART_NODE_HAND_DRAWN_HACHURE_GAP,
                FLOWCHART_NODE_HAND_DRAWN_ROUGHNESS,
                common.hand_drawn_seed,
            )
        })
    } else {
        None
    };

    if let Some((fill_d, stroke_d)) = rough_paths {
        let _ = write!(
            out,
            r#"<g class="basic label-container" style="{}">"#,
            escape_attr(common.rough_group_style)
        );
        let _ = write!(
            out,
            r#"<path d="{}" stroke="{}" stroke-width="{}" fill="none" stroke-dasharray="0 0"/>"#,
            escape_attr(&fill_d),
            escape_attr(common.fill_color),
            fmt_display(FLOWCHART_NODE_HAND_DRAWN_FILL_WEIGHT as f64),
        );
        let _ = write!(
            out,
            r#"<path d="{}" stroke="{}" stroke-width="{}" fill="none" stroke-dasharray="{}"/>"#,
            escape_attr(&stroke_d),
            escape_attr(common.stroke_color),
            common.stroke_width,
            escape_attr(common.stroke_dasharray),
        );
        out.push_str("</g>");
    } else {
        let _ = write!(
            out,
            r#"<rect class="basic label-container" style="{}" x="{}" y="{}" width="{}" height="{}""#,
            escape_attr(common.style),
            fmt(-w / 2.0),
            fmt(-h / 2.0),
            fmt(w),
            fmt(h)
        );
        if let Some(radius) = radius {
            let radius = radius_text(radius);
            let _ = write!(
                out,
                r#" rx="{}" ry="{}""#,
                escape_attr(&radius),
                escape_attr(&radius)
            );
        }
        out.push_str("/>");
    }
}

fn radius_text(radius: &serde_json::Value) -> String {
    match radius {
        serde_json::Value::String(text) => text.clone(),
        serde_json::Value::Number(number) => {
            fmt_display(number.as_f64().unwrap_or(0.0)).to_string()
        }
        _ => radius.to_string(),
    }
}

fn rounded_rect_path_data(width: f64, height: f64, radius: &serde_json::Value) -> String {
    // Pinned roundedRectPath.ts: no radius clamp or sampled polygon conversion.
    let x = -width / 2.0;
    let y = -height / 2.0;
    // The pinned JS helper concatenates strings for +, but coerces them for -.
    // Keep that distinction: a truthy "0" takes rc.path, unlike numeric zero.
    let numeric_radius = match radius {
        serde_json::Value::Bool(value) => f64::from(u8::from(*value)),
        serde_json::Value::String(value) if value.trim().is_empty() => 0.0,
        _ => crate::config::json_f64(radius).unwrap_or(f64::NAN),
    };
    let add_radius = |coordinate: f64| match radius.as_str() {
        Some(text) => format!("{}{text}", fmt_display(coordinate)),
        None => fmt_display(coordinate + numeric_radius).to_string(),
    };
    let radius = radius_text(radius);
    format!(
        "M {} {} H {} A {} {} 0 0 1 {} {} V {} A {} {} 0 0 1 {} {} H {} A {} {} 0 0 1 {} {} V {} A {} {} 0 0 1 {} {} Z",
        add_radius(x),
        fmt_display(y),
        fmt_display(x + width - numeric_radius),
        radius,
        radius,
        fmt_display(x + width),
        add_radius(y),
        fmt_display(y + height - numeric_radius),
        radius,
        radius,
        fmt_display(x + width - numeric_radius),
        fmt_display(y + height),
        add_radius(x),
        radius,
        radius,
        fmt_display(x),
        fmt_display(y + height - numeric_radius),
        add_radius(y),
        radius,
        radius,
        add_radius(x),
        fmt_display(y),
    )
}

#[cfg(test)]
mod tests {
    use super::rounded_rect_path_data;

    #[test]
    fn rounded_rectangle_keeps_source_arcs_and_oversized_radius() {
        assert_eq!(
            rounded_rect_path_data(150.0, 51.0, &serde_json::json!(12)),
            "M -63 -25.5 H 63 A 12 12 0 0 1 75 -13.5 V 13.5 A 12 12 0 0 1 63 25.5 H -63 A 12 12 0 0 1 -75 13.5 V -13.5 A 12 12 0 0 1 -63 -25.5 Z"
        );
        assert_eq!(
            rounded_rect_path_data(20.0, 10.0, &serde_json::json!(12)),
            "M 2 -5 H -2 A 12 12 0 0 1 10 7 V -7 A 12 12 0 0 1 -2 5 H 2 A 12 12 0 0 1 -10 -7 V 7 A 12 12 0 0 1 2 -5 Z"
        );
    }

    #[test]
    fn rounded_rectangle_retains_source_string_radius_addition() {
        assert_eq!(
            rounded_rect_path_data(150.0, 51.0, &serde_json::json!("0")),
            "M -750 -25.5 H 75 A 0 0 0 0 1 75 -25.50 V 25.5 A 0 0 0 0 1 75 25.5 H -750 A 0 0 0 0 1 -75 25.5 V -25.50 A 0 0 0 0 1 -750 -25.5 Z"
        );
        assert_eq!(
            rounded_rect_path_data(150.0, 51.0, &serde_json::json!("7.5")),
            "M -757.5 -25.5 H 67.5 A 7.5 7.5 0 0 1 75 -25.57.5 V 18 A 7.5 7.5 0 0 1 67.5 25.5 H -757.5 A 7.5 7.5 0 0 1 -75 18 V -25.57.5 A 7.5 7.5 0 0 1 -757.5 -25.5 Z"
        );
    }
}
