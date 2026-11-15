//! Shared Mermaid 12 insertLookDefs resources. Callers preserve their root DOM ordering.

use super::*;

pub(super) fn push_look_shadow_defs(
    out: &mut String,
    diagram_id: SvgDiagramId<'_>,
    effective_config_value: &serde_json::Value,
) {
    let flood_color = effective_config_value
        .get("theme")
        .and_then(|v| v.as_str())
        .filter(|theme| theme.contains("dark"))
        .map(|_| "#FFFFFF")
        .unwrap_or("#000000");
    let _ = write!(
        out,
        r#"<defs><filter id="{}-drop-shadow" height="130%" width="130%"><feDropShadow dx="4" dy="4" stdDeviation="0" flood-opacity="0.06" flood-color="{}"/></filter></defs><defs><filter id="{}-drop-shadow-small" height="150%" width="150%"><feDropShadow dx="2" dy="2" stdDeviation="0" flood-opacity="0.06" flood-color="{}"/></filter></defs>"#,
        diagram_id, flood_color, diagram_id, flood_color
    );
}

pub(super) fn push_look_gradient(
    out: &mut String,
    diagram_id: SvgDiagramId<'_>,
    effective_config_value: &serde_json::Value,
) {
    if !config_bool(effective_config_value, &["themeVariables", "useGradient"]).unwrap_or(false) {
        return;
    }

    let gradient_start =
        config_string(effective_config_value, &["themeVariables", "gradientStart"])
            .or_else(|| {
                config_string(
                    effective_config_value,
                    &["themeVariables", "primaryBorderColor"],
                )
            })
            .unwrap_or_else(|| "#9370DB".to_string());
    let gradient_stop = config_string(effective_config_value, &["themeVariables", "gradientStop"])
        .or_else(|| {
            config_string(
                effective_config_value,
                &["themeVariables", "secondaryBorderColor"],
            )
        })
        .unwrap_or_else(|| gradient_start.clone());

    let gradient_start = escape_xml(&gradient_start);
    let gradient_stop = escape_xml(&gradient_stop);
    let _ = write!(
        out,
        r#"<linearGradient id="{}-gradient" gradientUnits="objectBoundingBox" x1="0%" y1="0%" x2="100%" y2="0%"><stop offset="0%" stop-color="{}" stop-opacity="1"/><stop offset="100%" stop-color="{}" stop-opacity="1"/></linearGradient>"#,
        diagram_id,
        gradient_start.as_str(),
        gradient_stop.as_str()
    );
}
