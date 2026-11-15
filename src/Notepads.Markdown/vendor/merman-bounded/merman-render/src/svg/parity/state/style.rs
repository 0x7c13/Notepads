use super::*;

#[derive(Debug, Clone)]
pub(super) struct StateThemeDefaults {
    pub(super) background: String,
    pub(super) line_color: String,
    pub(super) main_bkg: String,
    pub(super) state_bkg: String,
    pub(super) state_border: String,
    pub(super) rect_radius: f64,
    pub(super) node_shadow: bool,
    pub(super) stroke_width: String,
    pub(super) stroke_width_px: String,
    pub(super) rough_stroke_width_value: f64,
    pub(super) special_state_color: String,
    pub(super) inner_end_background: String,
    pub(super) note_bkg: String,
    pub(super) note_border: String,
}

impl StateThemeDefaults {
    pub(super) fn from_config(effective_config: &serde_json::Value) -> Self {
        let theme = PresentationTheme::new(effective_config).state_diagram();
        // Mermaid 12 roundedRect uses theme radius ?? 5. drawRect only applies a
        // truthy override, so numeric zero preserves State dataFetcher's rx/ry=10;
        // a numeric string such as "0" still overrides those defaults.
        let numeric_zero = effective_config
            .get("themeVariables")
            .and_then(|theme| theme.get("radius"))
            .and_then(serde_json::Value::as_f64)
            == Some(0.0);
        let rect_radius = if numeric_zero {
            10.0
        } else {
            config_f64(effective_config, &["themeVariables", "radius"]).unwrap_or(5.0)
        };

        Self {
            background: theme.background,
            line_color: theme.common.line_color,
            main_bkg: theme.main_bkg,
            state_bkg: theme.state_bkg,
            state_border: theme.state_border,
            rect_radius,
            node_shadow: effective_config
                .get("themeVariables")
                .and_then(|theme| theme.get("nodeShadow"))
                .is_some_and(crate::config::json_value_is_truthy),
            stroke_width: theme.stroke_width,
            stroke_width_px: theme.stroke_width_px,
            rough_stroke_width_value: theme.rough_stroke_width_value,
            special_state_color: theme.special_state_color,
            inner_end_background: theme.inner_end_background,
            note_bkg: theme.note_bkg,
            note_border: theme.note_border,
        }
    }
}

struct StateScopedDropShadow<'a, I> {
    diagram_id: I,
    source: &'a str,
}

impl<I> std::fmt::Display for StateScopedDropShadow<'_, I>
where
    I: Copy + std::fmt::Display,
{
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let mut parts = self.source.split("url(#drop-shadow)");
        formatter.write_str(parts.next().unwrap_or_default())?;
        for suffix in parts {
            write!(formatter, "url(#{}-drop-shadow)", self.diagram_id)?;
            formatter.write_str(suffix)?;
        }
        Ok(())
    }
}

fn state_shadow_defs(
    out: &mut String,
    diagram_id: SvgDiagramId<'_>,
    effective_config: &serde_json::Value,
) {
    let flood_color = if PresentationTheme::new(effective_config)
        .common()
        .is_dark_theme()
    {
        "#FFFFFF"
    } else {
        "#000000"
    };
    let _ = write!(
        out,
        r#"<defs><filter id="{}-drop-shadow" height="130%" width="130%"><feDropShadow dx="4" dy="4" stdDeviation="0" flood-opacity="0.06" flood-color="{}"/></filter></defs><defs><filter id="{}-drop-shadow-small" height="150%" width="150%"><feDropShadow dx="2" dy="2" stdDeviation="0" flood-opacity="0.06" flood-color="{}"/></filter></defs>"#,
        diagram_id, flood_color, diagram_id, flood_color
    );
}

fn state_gradient_defs(
    out: &mut String,
    diagram_id: SvgDiagramId<'_>,
    effective_config: &serde_json::Value,
) {
    if !config_bool(effective_config, &["themeVariables", "useGradient"]).unwrap_or(false) {
        return;
    }

    let gradient_start = config_string(effective_config, &["themeVariables", "gradientStart"])
        .or_else(|| config_string(effective_config, &["themeVariables", "primaryBorderColor"]))
        .unwrap_or_else(|| "#9370DB".to_string());
    let gradient_stop = config_string(effective_config, &["themeVariables", "gradientStop"])
        .or_else(|| {
            config_string(
                effective_config,
                &["themeVariables", "secondaryBorderColor"],
            )
        })
        .unwrap_or_else(|| gradient_start.clone());

    let gradient_start = escape_xml(&gradient_start);
    let gradient_stop = escape_xml(&gradient_stop);
    let _ = write!(
        out,
        r#"<defs><linearGradient id="{}-gradient" gradientUnits="objectBoundingBox" x1="0%" y1="0%" x2="100%" y2="0%"><stop offset="0%" stop-color="{}" stop-opacity="1"/><stop offset="100%" stop-color="{}" stop-opacity="1"/></linearGradient></defs>"#,
        diagram_id,
        gradient_start.as_str(),
        gradient_stop.as_str()
    );
}

pub(super) fn state_markers(
    out: &mut String,
    diagram_id: SvgDiagramId<'_>,
    effective_config: &serde_json::Value,
) {
    let theme = PresentationTheme::new(effective_config).state_diagram();
    let transition_color = theme.transition_color.as_str();

    if theme.common.is_neo() {
        let _ = write!(
            out,
            r#"<defs><marker id="{diagram_id}_stateDiagram-barbEnd" refX="19" refY="7" markerWidth="20" markerHeight="14" markerUnits="strokeWidth" orient="auto"><path d="M 19,7 L11,14 L13,7 L11,0 Z"/></marker></defs><defs><marker id="{diagram_id}_stateDiagram-barbEnd-margin" refX="17" refY="7" markerWidth="20" markerHeight="14" markerUnits="userSpaceOnUse" orient="auto"><path d="M 19,7 L11,14 L13,7 L11,0 Z" fill="{}"/></marker></defs>"#,
            escape_xml(transition_color)
        );
    } else {
        let _ = write!(
            out,
            r#"<defs><marker id="{diagram_id}_stateDiagram-barbEnd" refX="19" refY="7" markerWidth="20" markerHeight="14" markerUnits="userSpaceOnUse" orient="auto"><path d="M 19,7 L9,13 L14,7 L9,1 Z"/></marker></defs>"#
        );
    }
}

pub(super) fn state_root_defs(
    out: &mut String,
    diagram_id: SvgDiagramId<'_>,
    effective_config: &serde_json::Value,
) {
    state_shadow_defs(out, diagram_id, effective_config);
    state_gradient_defs(out, diagram_id, effective_config);
}

// colorThemeGate.ts uses the same palette length for CSS and container slots.
pub(super) fn state_palette_size(config: &serde_json::Value) -> usize {
    if !matches!(
        config.get("theme").and_then(serde_json::Value::as_str),
        Some("redux-color" | "redux-dark-color")
    ) {
        return 0;
    }
    config
        .pointer("/themeVariables/borderColorArray")
        .and_then(serde_json::Value::as_array)
        .map_or(0, Vec::len)
}

fn write_state_palette_css<I: Copy + std::fmt::Display>(
    out: &mut String,
    id: I,
    config: &serde_json::Value,
) {
    if state_palette_size(config) == 0 {
        return;
    }
    let Some(borders) = config
        .pointer("/themeVariables/borderColorArray")
        .and_then(serde_json::Value::as_array)
    else {
        return;
    };
    let backgrounds = config
        .pointer("/themeVariables/bkgColorArray")
        .and_then(serde_json::Value::as_array)
        .filter(|colors| !colors.is_empty());
    let look = config
        .get("look")
        .and_then(|value| match value {
            serde_json::Value::String(value) => Some(value.clone()),
            serde_json::Value::Number(value) => Some(value.to_string()),
            _ => None,
        })
        .filter(|look| {
            !look.is_empty()
                && look
                    .bytes()
                    .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'-'))
        })
        .unwrap_or_else(|| "classic".into());
    let css_color = |value: &serde_json::Value| {
        value
            .as_str()
            .map(|value| value.trim().to_owned())
            .unwrap_or_else(|| value.to_string())
    };
    for (index, border) in borders.iter().enumerate() {
        let border = css_color(border);
        let tint = backgrounds.map(|colors| css_color(&colors[index % colors.len()]));
        // Keep the source order and leave the body fill (including rough hatching) unchanged.
        for (selector, stroke, fill) in [
            ("rect.outer", true, true),
            ("rect.inner", true, false),
            ("rect.divider", true, true),
            (".outer path[stroke='none']", false, true),
            (".outer path[fill='none']", true, false),
            (".divider path[stroke='none']", false, true),
            (".divider path[fill='none']", true, false),
        ] {
            // Stylis removes empty rules when an outline-only palette has no tint.
            if !stroke && tint.is_none() {
                continue;
            }
            let _ = write!(
                out,
                r#"#{id} [data-look="{look}"][data-color-id="color-{index}"].statediagram-cluster {selector}{{"#
            );
            if stroke {
                let _ = write!(out, "stroke:{border};");
            }
            if fill && let Some(tint) = &tint {
                let _ = write!(out, "fill:{tint};");
            }
            out.push('}');
        }
    }
}

pub(super) fn state_css<I>(
    diagram_id: I,
    model: &StateSvgModel,
    effective_config: &serde_json::Value,
) -> String
where
    I: Copy + std::fmt::Display,
{
    fn normalize_decl(s: &str) -> Option<(String, String)> {
        let (k, v) = crate::mermaid_style::parse_safe_style_decl(s)?;
        let key = if k.starts_with("--") {
            k.to_string()
        } else {
            k.to_ascii_lowercase()
        };
        let value = v.trim();
        let value = value
            .rsplit_once('!')
            .filter(|(_, priority)| priority.trim().eq_ignore_ascii_case("important"))
            .map(|(value, _)| value.trim_end())
            .unwrap_or(value);
        let value = if matches!(
            key.as_str(),
            "background"
                | "background-color"
                | "border-color"
                | "color"
                | "fill"
                | "outline-color"
                | "stroke"
        ) {
            super::super::util::cssom_color_value(value)
        } else {
            value.to_string()
        };
        Some((key, format!("{value}!important")))
    }

    fn class_decl_block(styles: &[String]) -> String {
        let mut out = String::new();
        for raw in styles {
            let Some((k, v)) = normalize_decl(raw) else {
                continue;
            };
            let _ = write!(&mut out, "{}:{};", k, v);
        }
        out
    }

    let theme = PresentationTheme::new(effective_config).state_diagram();
    let ff = theme.common.font_family_css.as_str();
    let font_size = theme.common.font_size_px;
    let id = diagram_id;
    let text_color = theme.common.text_color.as_str();
    let error_bkg = theme.common.error_bkg.as_str();
    let error_text = theme.common.error_text.as_str();
    let line_color = theme.common.line_color.as_str();
    let transition_color = theme.transition_color.as_str();
    let node_border = theme.node_border.as_str();
    let state_label_color = theme.state_label_color.as_str();
    let defaults = StateThemeDefaults::from_config(effective_config);
    let main_bkg = &defaults.main_bkg;
    let background = &defaults.background;
    let alt_background = theme.alt_background.as_str();
    let stroke_width = &defaults.stroke_width;
    let stroke_width_px = &defaults.stroke_width_px;
    let note_border = &defaults.note_border;
    let note_bkg = &defaults.note_bkg;
    let note_text = theme.note_text.as_str();
    let label_background = theme.label_background.as_str();
    let edge_label_background = theme.edge_label_background.as_str();
    let transition_label_color = theme.transition_label_color.as_str();
    let special_state_color = &defaults.special_state_color;
    let inner_end_background = &defaults.inner_end_background;
    let composite_background = theme.composite_background.as_str();
    let state_bkg = &defaults.state_bkg;
    let state_border = &defaults.state_border;
    let composite_title_background = theme.composite_title_background.as_str();
    let use_gradient =
        config_bool(effective_config, &["themeVariables", "useGradient"]).unwrap_or(false);
    let neo_cluster_stroke = if use_gradient {
        format!("url(#{diagram_id}-gradient)")
    } else {
        state_border.clone()
    };
    let neo_radius =
        crate::config::config_f64_css_px(effective_config, &["themeVariables", "radius"])
            .unwrap_or(5.0)
            .max(0.0);
    let neo_drop_shadow = StateScopedDropShadow {
        diagram_id,
        source: &theme.drop_shadow,
    };

    // Mirrors Mermaid 11.15 `diagrams/state/styles.js` + shared base stylesheet ordering.
    let mut css = String::new();
    let font_size_s = fmt(font_size);
    let _ = write!(
        &mut css,
        r#"#{}{{font-family:{};font-size:{}px;fill:{};}}"#,
        id, ff, font_size_s, text_color
    );
    css.push_str("@keyframes edge-animation-frame{from{stroke-dashoffset:0;}}");
    css.push_str("@keyframes dash{to{stroke-dashoffset:0;}}");
    let _ = write!(
        &mut css,
        r#"#{} .edge-animation-slow{{stroke-dasharray:9,5!important;stroke-dashoffset:900;animation:dash 50s linear infinite;stroke-linecap:round;}}"#,
        id
    );
    let _ = write!(
        &mut css,
        r#"#{} .edge-animation-fast{{stroke-dasharray:9,5!important;stroke-dashoffset:900;animation:dash 20s linear infinite;stroke-linecap:round;}}"#,
        id
    );
    let _ = write!(&mut css, r#"#{} .error-icon{{fill:{};}}"#, id, error_bkg);
    let _ = write!(
        &mut css,
        r#"#{} .error-text{{fill:{};stroke:{};}}"#,
        id, error_text, error_text
    );
    let _ = write!(
        &mut css,
        r#"#{} .edge-thickness-normal{{stroke-width:{};}}"#,
        id, stroke_width_px
    );
    let _ = write!(
        &mut css,
        r#"#{} .edge-thickness-thick{{stroke-width:3.5px;}}"#,
        id
    );
    let _ = write!(
        &mut css,
        r#"#{} .edge-pattern-solid{{stroke-dasharray:0;}}"#,
        id
    );
    let _ = write!(
        &mut css,
        r#"#{} .edge-thickness-invisible{{stroke-width:0;fill:none;}}"#,
        id
    );
    let _ = write!(
        &mut css,
        r#"#{} .edge-pattern-dashed{{stroke-dasharray:3;}}"#,
        id
    );
    let _ = write!(
        &mut css,
        r#"#{} .edge-pattern-dotted{{stroke-dasharray:2;}}"#,
        id
    );
    let _ = write!(
        &mut css,
        r#"#{} .marker{{fill:{};stroke:{};}}"#,
        id, line_color, line_color
    );
    let _ = write!(
        &mut css,
        r#"#{} .marker.cross{{stroke:{};}}"#,
        id, line_color
    );
    let _ = write!(
        &mut css,
        r#"#{} svg{{font-family:{};font-size:{}px;}}"#,
        id, ff, font_size_s
    );
    let _ = write!(&mut css, r#"#{} p{{margin:0;}}"#, id);
    write_state_palette_css(&mut css, id, effective_config);
    let _ = write!(
        &mut css,
        r#"#{} defs [id$="-barbEnd"]{{fill:{};stroke:{};}}"#,
        id, transition_color, transition_color
    );
    let _ = write!(
        &mut css,
        r#"#{} g.stateGroup text{{fill:{};stroke:none;font-size:10px;}}"#,
        id, node_border
    );
    let _ = write!(
        &mut css,
        r#"#{} g.stateGroup text{{fill:{};stroke:none;font-size:10px;}}"#,
        id, text_color
    );
    let _ = write!(
        &mut css,
        r#"#{} g.stateGroup .state-title{{font-weight:bolder;fill:{};}}"#,
        id, state_label_color
    );
    let _ = write!(
        &mut css,
        r#"#{} g.stateGroup rect{{fill:{};stroke:{};}}"#,
        id, main_bkg, node_border
    );
    let _ = write!(
        &mut css,
        r#"#{} g.stateGroup line{{stroke:{};stroke-width:{};}}"#,
        id, line_color, stroke_width
    );
    let _ = write!(
        &mut css,
        r#"#{} .transition{{stroke:{};stroke-width:{};fill:none;}}"#,
        id, transition_color, stroke_width
    );
    let _ = write!(
        &mut css,
        r#"#{} .stateGroup .composit{{fill:{};border-bottom:1px;}}"#,
        id, background
    );
    let _ = write!(
        &mut css,
        r#"#{} .stateGroup .alt-composit{{fill:#e0e0e0;border-bottom:1px;}}"#,
        id
    );
    let _ = write!(
        &mut css,
        r#"#{} .state-note{{stroke:{};fill:{};}}"#,
        id, note_border, note_bkg
    );
    let _ = write!(
        &mut css,
        r#"#{} .state-note text{{fill:{};stroke:none;font-size:10px;}}"#,
        id, note_text
    );
    let _ = write!(
        &mut css,
        r#"#{} .stateLabel .box{{stroke:none;stroke-width:0;fill:{};opacity:0.5;}}"#,
        id, main_bkg
    );
    let _ = write!(
        &mut css,
        r#"#{} .edgeLabel .label rect{{fill:{};opacity:0.5;}}"#,
        id, label_background
    );
    let _ = write!(
        &mut css,
        r#"#{} .edgeLabel{{background-color:{};text-align:center;}}"#,
        id, edge_label_background
    );
    let _ = write!(
        &mut css,
        r#"#{} .edgeLabel p{{background-color:{};}}"#,
        id, edge_label_background
    );
    let _ = write!(
        &mut css,
        r#"#{} .edgeLabel rect{{opacity:0.5;background-color:{};fill:{};}}"#,
        id, edge_label_background, edge_label_background
    );
    let _ = write!(
        &mut css,
        r#"#{} .edgeLabel .label text{{fill:{};}}"#,
        id, transition_label_color
    );
    let _ = write!(
        &mut css,
        r#"#{} .label div .edgeLabel{{color:{};}}"#,
        id, transition_label_color
    );
    let _ = write!(
        &mut css,
        r#"#{} .stateLabel text{{fill:{};font-size:10px;font-weight:bold;}}"#,
        id, state_label_color
    );
    let _ = write!(
        &mut css,
        r#"#{} .node circle.state-start{{fill:{};stroke:{};}}"#,
        id, special_state_color, special_state_color
    );
    let _ = write!(
        &mut css,
        r#"#{} .node .fork-join{{fill:{};stroke:{};}}"#,
        id, special_state_color, special_state_color
    );
    let _ = write!(
        &mut css,
        r#"#{} .node circle.state-end{{fill:{};stroke:{};stroke-width:1.5;}}"#,
        id, inner_end_background, background
    );
    let _ = write!(
        &mut css,
        r#"#{} .end-state-inner{{fill:{};stroke-width:1.5;}}"#,
        id, composite_background
    );
    let _ = write!(
        &mut css,
        r#"#{} .node rect{{fill:{};stroke:{};stroke-width:{};}}"#,
        id, state_bkg, state_border, stroke_width_px
    );
    let _ = write!(
        &mut css,
        r#"#{} .node polygon{{fill:{};stroke:{};stroke-width:{};}}"#,
        id, main_bkg, state_border, stroke_width_px
    );
    let _ = write!(
        &mut css,
        r#"#{} [id$="-barbEnd"]{{fill:{};}}"#,
        id, line_color
    );
    let _ = write!(
        &mut css,
        r#"#{} .statediagram-cluster rect{{fill:{};stroke:{};stroke-width:{};}}"#,
        id, composite_title_background, state_border, stroke_width_px
    );
    let _ = write!(
        &mut css,
        r#"#{} .cluster-label,#{} .nodeLabel{{color:{};}}"#,
        id, id, state_label_color
    );
    let _ = write!(
        &mut css,
        r#"#{} .statediagram-cluster rect.outer{{rx:5px;ry:5px;}}"#,
        id
    );
    let _ = write!(
        &mut css,
        r#"#{} .statediagram-state .divider{{stroke:{};}}"#,
        id, state_border
    );
    let _ = write!(
        &mut css,
        r#"#{} .statediagram-state .title-state{{rx:5px;ry:5px;}}"#,
        id
    );
    let _ = write!(
        &mut css,
        r#"#{} .statediagram-cluster.statediagram-cluster .inner{{fill:{};}}"#,
        id, composite_background
    );
    let _ = write!(
        &mut css,
        r#"#{} .statediagram-cluster.statediagram-cluster-alt .inner{{fill:{};}}"#,
        id, alt_background
    );
    let _ = write!(
        &mut css,
        r#"#{} .statediagram-cluster .inner{{rx:0;ry:0;}}"#,
        id
    );
    let _ = write!(
        &mut css,
        r#"#{} .statediagram-state rect.basic{{rx:5px;ry:5px;}}"#,
        id
    );
    let _ = write!(
        &mut css,
        r#"#{} .statediagram-state rect.divider{{stroke-dasharray:10,10;fill:{};}}"#,
        id, alt_background
    );
    let _ = write!(&mut css, r#"#{} .note-edge{{stroke-dasharray:5;}}"#, id);
    let _ = write!(
        &mut css,
        r#"#{} .statediagram-note rect{{fill:{};stroke:{};stroke-width:1px;rx:0;ry:0;}}"#,
        id, note_bkg, note_border
    );
    let _ = write!(
        &mut css,
        r#"#{} .statediagram-note rect{{fill:{};stroke:{};stroke-width:1px;rx:0;ry:0;}}"#,
        id, note_bkg, note_border
    );
    let _ = write!(
        &mut css,
        r#"#{} .statediagram-note text{{fill:{};}}"#,
        id, note_text
    );
    let _ = write!(
        &mut css,
        r#"#{} .statediagram-note .nodeLabel{{color:{};}}"#,
        id, note_text
    );
    let _ = write!(
        &mut css,
        r#"#{} .statediagram .edgeLabel{{color:red;}}"#,
        id
    );
    // State's dependency rule uses `strokeWidth || 1`; the shared edge rule uses `?? 1`.
    let dependency_stroke_width = if effective_config
        .pointer("/themeVariables/strokeWidth")
        .and_then(serde_json::Value::as_f64)
        == Some(0.0)
    {
        "1"
    } else {
        stroke_width.as_str()
    };
    let _ = write!(
        &mut css,
        r#"#{} [id$="-dependencyStart"],#{} [id$="-dependencyEnd"]{{fill:{};stroke:{};stroke-width:{};}}"#,
        id, id, line_color, line_color, dependency_stroke_width
    );
    let _ = write!(
        &mut css,
        r#"#{} .statediagramTitleText{{text-anchor:middle;font-size:18px;fill:{};}}"#,
        id, text_color
    );
    let _ = write!(
        &mut css,
        r#"#{} [data-look="neo"].statediagram-cluster rect{{fill:{};stroke:{};stroke-width:{};}}"#,
        id, main_bkg, neo_cluster_stroke, stroke_width
    );
    let _ = write!(
        &mut css,
        r#"#{} [data-look="neo"].statediagram-cluster rect.outer{{rx:{}px;ry:{}px;filter:{};}}"#,
        id,
        fmt(neo_radius),
        fmt(neo_radius),
        neo_drop_shadow
    );
    let _ = write!(
        &mut css,
        r#"#{} .node .neo-node{{stroke:{};}}"#,
        id, state_border
    );
    let _ = write!(
        &mut css,
        r#"#{} [data-look="neo"].node rect,#{} [data-look="neo"].cluster rect,#{} [data-look="neo"].node polygon{{stroke:{};filter:{};}}"#,
        id, id, id, state_border, neo_drop_shadow
    );
    let _ = write!(
        &mut css,
        r#"#{} [data-look="neo"].swimlane.cluster rect{{filter:none;}}"#,
        id
    );
    let _ = write!(
        &mut css,
        r#"#{} [data-look="neo"].node path{{stroke:{};stroke-width:{};}}"#,
        id, state_border, stroke_width_px
    );
    let _ = write!(
        &mut css,
        r#"#{} [data-look="neo"].node .outer-path{{filter:{};}}"#,
        id, neo_drop_shadow
    );
    let _ = write!(
        &mut css,
        r#"#{} [data-look="neo"].node .neo-line path{{stroke:{};filter:none;}}"#,
        id, state_border
    );
    let _ = write!(
        &mut css,
        r#"#{} [data-look="neo"].node circle{{stroke:{};filter:{};}}"#,
        id, state_border, neo_drop_shadow
    );
    let _ = write!(
        &mut css,
        r#"#{} [data-look="neo"].node circle .state-start{{fill:#000000;}}"#,
        id
    );
    let _ = write!(
        &mut css,
        r#"#{} [data-look="neo"].icon-shape .icon{{fill:{};filter:{};}}"#,
        id, state_border, neo_drop_shadow
    );
    let _ = write!(
        &mut css,
        r#"#{} [data-look="neo"].icon-shape .icon-neo path{{stroke:{};filter:{};}}"#,
        id, state_border, neo_drop_shadow
    );
    let _ = crate::svg::parity::css::write_mermaid_base_css_root_rule_to(
        &mut css,
        id,
        &crate::config::config_root_font_family_css(effective_config),
    );

    if !model.style_classes.is_empty() {
        let html_labels = crate::state::StateConfigView::new(effective_config)
            .render_settings()
            .html_labels;
        for sc in model.style_classes.values() {
            let styles = class_decl_block(&sc.styles);
            if !styles.is_empty() {
                if html_labels {
                    let _ = write!(
                        &mut css,
                        r#"#{} .{}&gt;*{{{}}}#{} .{} span{{{}}}"#,
                        id, sc.id, styles, id, sc.id, styles
                    );
                } else {
                    for element in ["rect", "polygon", "ellipse", "circle", "path"] {
                        let _ = write!(&mut css, r#"#{} .{} {}{{{}}}"#, id, sc.id, element, styles);
                    }
                }
            }

            let text_styles = class_decl_block(&sc.text_styles);
            if !text_styles.is_empty() {
                let _ = write!(&mut css, r#"#{} .{} tspan{{{}}}"#, id, sc.id, text_styles);
            }
        }
    }

    css
}

pub(super) fn state_value_to_label_text(v: &serde_json::Value) -> String {
    match v {
        serde_json::Value::String(s) => s.clone(),
        serde_json::Value::Array(a) => {
            let mut parts: Vec<&str> = Vec::new();
            for item in a {
                if let Some(s) = item.as_str() {
                    parts.push(s);
                }
            }
            if parts.is_empty() {
                return "".to_string();
            }
            parts.join("\n")
        }
        _ => "".to_string(),
    }
}

pub(super) fn state_node_label_text(n: &StateSvgNode) -> String {
    n.label
        .as_ref()
        .map(state_value_to_label_text)
        .unwrap_or_else(|| n.id.clone())
}

#[derive(Debug, Clone, Copy)]
pub(super) struct StateInlineDecl<'a> {
    pub(super) key: &'a str,
    pub(super) val: &'a str,
}

pub(super) fn state_parse_inline_decl(raw: &str) -> Option<StateInlineDecl<'_>> {
    let raw = raw.trim().trim_end_matches(';').trim();
    if raw.is_empty() {
        return None;
    }
    let (k, v) = raw.split_once(':')?;
    let key = k.trim();
    let val = v.trim();
    if key.is_empty() || val.is_empty() {
        return None;
    }
    Some(StateInlineDecl { key, val })
}

pub(super) fn state_is_text_style_key(key: &str) -> bool {
    let k = key.trim().to_ascii_lowercase();
    k == "color" || k.starts_with("font-") || k.starts_with("text-")
}

pub(super) fn state_compact_style_attr(decls: &[StateInlineDecl<'_>]) -> String {
    let mut out = String::new();
    for (idx, d) in decls.iter().enumerate() {
        if idx > 0 {
            out.push(';');
        }
        out.push_str(d.key.trim());
        out.push(':');
        out.push_str(d.val.trim());
        if !d.val.to_ascii_lowercase().contains("!important") {
            out.push_str(" !important");
        }
    }
    out
}

pub(super) fn state_div_style_prefix(decls: &[StateInlineDecl<'_>]) -> String {
    let mut out = String::new();
    for d in decls {
        out.push_str(d.key.trim());
        out.push_str(": ");
        out.push_str(d.val.trim());
        if !d.val.to_ascii_lowercase().contains("!important") {
            out.push_str(" !important");
        }
        out.push_str("; ");
    }
    out
}

pub(super) fn state_node_label_html_with_style(
    raw: &str,
    span_style: Option<&str>,
    font_size: f64,
) -> String {
    let style_attr = span_style
        .filter(|s| !s.is_empty())
        .map(|s| format!(r#" style="{}""#, escape_xml_display(s)))
        .unwrap_or_default();
    format!(
        r#"<span{} class="nodeLabel markdown-node-label">{}</span>"#,
        style_attr,
        crate::state::state_node_label_xhtml(raw, font_size)
    )
}

fn state_escape_amp_preserving_entities(raw: &str) -> String {
    crate::xml::normalize_html_entities_for_xml(raw).into_owned()
}

fn state_normalize_br_tags(raw: &str) -> String {
    let bytes = raw.as_bytes();
    let mut out = String::with_capacity(raw.len());
    let mut cur = 0usize;
    let mut i = 0usize;
    while i + 2 < bytes.len() {
        if bytes[i] != b'<' {
            i += 1;
            continue;
        }
        let b1 = bytes[i + 1];
        let b2 = bytes[i + 2];
        if !matches!(b1, b'b' | b'B') || !matches!(b2, b'r' | b'R') {
            i += 1;
            continue;
        }
        let next = bytes.get(i + 3).copied();
        if let Some(n) = next
            && !matches!(n, b'>' | b'/' | b' ' | b'\t' | b'\r' | b'\n')
        {
            i += 1;
            continue;
        }
        if i > cur {
            out.push_str(&raw[cur..i]);
        }
        let Some(end_rel) = bytes[i..].iter().position(|&c| c == b'>') else {
            cur = i;
            break;
        };
        out.push('\n');
        i = i + end_rel + 1;
        cur = i;
    }
    if cur < raw.len() {
        out.push_str(&raw[cur..]);
    }
    out
}

fn write_state_html_lines_with_br(out: &mut String, normalized: &str) {
    for (idx, line) in normalized.split('\n').enumerate() {
        if idx > 0 {
            out.push_str("<br />");
        }
        // State diagram labels are sanitized upstream (entities + limited tags). Preserve entities
        // like `&lt;` without double-escaping, while still making stray `&` XML-safe.
        out.push_str(&state_escape_amp_preserving_entities(line));
    }
}

fn state_html_with_br(raw: &str, wrap_paragraph: bool) -> String {
    let decoded = crate::svg::parity::util::decode_mermaid_entities_for_render_text(raw);
    let normalized = state_normalize_br_tags(decoded.as_ref());
    let mut out = String::new();
    if wrap_paragraph {
        out.push_str("<p>");
    }
    write_state_html_lines_with_br(&mut out, &normalized);
    if wrap_paragraph {
        out.push_str("</p>");
    }
    out
}

fn html_paragraph_with_br(raw: &str) -> String {
    state_html_with_br(raw, true)
}

pub(super) fn state_node_label_html(raw: &str, font_size: f64) -> String {
    format!(
        r#"<span class="nodeLabel markdown-node-label">{}</span>"#,
        crate::state::state_node_label_xhtml(raw, font_size)
    )
}

pub(super) fn state_node_label_plain_html(raw: &str) -> String {
    format!(
        r#"<span class="nodeLabel">{}</span>"#,
        html_paragraph_with_br(raw)
    )
}

pub(super) fn state_edge_label_html(raw: &str) -> String {
    crate::state::state_edge_label_xhtml(raw)
}

pub(super) fn state_svg_text_label(
    raw: &str,
    center_text: bool,
    style_attr: Option<&str>,
) -> String {
    let decoded = crate::svg::parity::util::decode_mermaid_entities_for_render_text(raw);
    let normalized = state_normalize_br_tags(decoded.as_ref());
    let text_anchor = if center_text {
        r#" text-anchor="middle""#
    } else {
        ""
    };
    let style_attr = style_attr
        .filter(|s| !s.trim().is_empty())
        .map(|s| format!(r#" style="{}""#, escape_xml_display(s)))
        .unwrap_or_default();

    let mut out = format!(r#"<text y="-10.1"{text_anchor}{style_attr}>"#);
    for (idx, line) in normalized.split('\n').enumerate() {
        let y = idx as f64 * 1.1 - 0.1;
        let _ = write!(
            &mut out,
            r#"<tspan class="text-outer-tspan row" x="0" y="{}em" dy="1.1em"><tspan font-style="normal" class="text-inner-tspan" font-weight="normal">{}</tspan></tspan>"#,
            fmt_display(y),
            escape_xml_display(line)
        );
    }
    out.push_str("</text>");
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use merman_core::diagrams::state::StateDiagramRenderStyleClass;
    use serde_json::json;

    #[test]
    fn state_rect_radius_preserves_source_fallback_and_zero_override() {
        for (config, expected) in [
            (json!({}), 5.0),
            (json!({"themeVariables": {"radius": null}}), 5.0),
            (
                json!({"look": "neo", "themeVariables": {"radius": 12}}),
                12.0,
            ),
            (
                json!({"look": "classic", "themeVariables": {"radius": 7.5}}),
                7.5,
            ),
            (json!({"themeVariables": {"radius": 0}}), 10.0),
            (json!({"themeVariables": {"radius": "0"}}), 0.0),
            (json!({"themeVariables": {"radius": "7.5"}}), 7.5),
            // Invalid values retain the existing numeric configuration contract.
            (json!({"themeVariables": {"radius": "invalid"}}), 5.0),
        ] {
            assert_eq!(
                StateThemeDefaults::from_config(&config).rect_radius,
                expected
            );
        }
    }

    fn model_with_hot_class() -> StateSvgModel {
        let mut model = StateSvgModel::default();
        model.style_classes.insert(
            "hot".to_string(),
            StateDiagramRenderStyleClass {
                id: "hot".to_string(),
                styles: vec![
                    "fill:#ffdddd".to_string(),
                    "stroke:#d33".to_string(),
                    "stroke-width:2px".to_string(),
                    "color:#222".to_string(),
                ],
                text_styles: vec!["fill:#222".to_string()],
            },
        );
        model
    }

    #[test]
    fn state_class_defs_keep_shape_and_text_styles_separate() {
        let css = state_css("st", &model_with_hot_class(), &json!({}));

        let declarations = "fill:rgb(255, 221, 221)!important;stroke:rgb(221, 51, 51)!important;stroke-width:2px!important;color:rgb(34, 34, 34)!important;";
        assert!(css.ends_with(&format!(
            "#st .hot&gt;*{{{declarations}}}#st .hot span{{{declarations}}}#st .hot tspan{{fill:rgb(34, 34, 34)!important;}}"
        )));
        assert_eq!(css.matches("#st .hot&gt;*").count(), 1);
        assert!(!css.contains("color:rgb(34, 34, 34)!important;fill:rgb(34, 34, 34)!important"));
    }

    #[test]
    fn state_class_defs_use_shape_selectors_without_html_labels() {
        let css = state_css(
            "st",
            &model_with_hot_class(),
            &json!({ "htmlLabels": false }),
        );

        for element in ["rect", "polygon", "ellipse", "circle", "path"] {
            assert!(css.contains(&format!("#st .hot {element}{{")));
        }
        assert!(!css.contains("#st .hot&gt;*"));
        assert!(!css.contains("#st .hot span{"));
        assert_eq!(css.matches("#st .hot tspan{").count(), 1);
    }

    #[test]
    fn state_class_defs_do_not_duplicate_font_rules() {
        let mut model = StateSvgModel::default();
        model.style_classes.insert(
            "emphasis".to_string(),
            StateDiagramRenderStyleClass {
                id: "emphasis".to_string(),
                styles: vec!["font-style:italic".to_string()],
                text_styles: vec![],
            },
        );

        let css = state_css("st", &model, &json!({}));
        assert_eq!(css.matches("#st .emphasis&gt;*").count(), 1);
        assert_eq!(css.matches("#st .emphasis span{").count(), 1);
    }

    #[test]
    fn state_css_honors_mermaid_12_theme_options() {
        let cfg = json!({
            "themeVariables": {
                "fontFamily": "Inter, Arial",
                "textColor": "#101010",
                "errorBkgColor": "#111111",
                "errorTextColor": "#121212",
                "transitionColor": "#202020",
                "lineColor": "#303030",
                "nodeBorder": "#404040",
                "stateLabelColor": "#505050",
                "mainBkg": "#606060",
                "background": "#707070",
                "altBackground": "#808080",
                "strokeWidth": 4,
                "noteBorderColor": "#909090",
                "noteBkgColor": "#a0a0a0",
                "noteTextColor": "#b0b0b0",
                "labelBackgroundColor": "#c0c0c0",
                "edgeLabelBackground": "#d0d0d0",
                "transitionLabelColor": "#e0e0e0",
                "specialStateColor": "#f0f0f0",
                "innerEndBackground": "#010101",
                "compositeBackground": "#020202",
                "stateBkg": "#030303",
                "stateBorder": "#040404",
                "compositeTitleBackground": "#050505"
            }
        });

        let css = state_css("st", &StateSvgModel::default(), &cfg);

        assert!(css.contains(r#"#st{font-family:Inter,Arial;font-size:16px;fill:#101010;}"#));
        assert!(css.contains(
            r#"#st .error-icon{fill:#111111;}#st .error-text{fill:#121212;stroke:#121212;}"#
        ));
        assert!(css.contains(
            r#"#st .marker{fill:#303030;stroke:#303030;}#st .marker.cross{stroke:#303030;}"#
        ));
        assert!(css.contains(r#"#st defs [id$="-barbEnd"]{fill:#202020;stroke:#202020;}"#));
        assert!(css.contains(r#"#st g.stateGroup rect{fill:#606060;stroke:#404040;}"#));
        assert!(css.contains(r#"#st .transition{stroke:#202020;stroke-width:4;fill:none;}"#));
        assert!(css.contains(r#"#st .edge-thickness-normal{stroke-width:4px;}"#));
        assert!(css.contains(r#"#st .state-note{stroke:#909090;fill:#a0a0a0;}"#));
        assert!(css.contains(r#"#st .edgeLabel .label rect{fill:#c0c0c0;opacity:0.5;}"#));
        assert!(css.contains(r#"#st .edgeLabel{background-color:#d0d0d0;text-align:center;}"#));
        assert!(css.contains(r#"#st .edgeLabel .label text{fill:#e0e0e0;}"#));
        assert!(
            css.contains(r#"#st .stateLabel text{fill:#505050;font-size:10px;font-weight:bold;}"#)
        );
        assert!(css.contains(r#"#st .node circle.state-start{fill:#f0f0f0;stroke:#f0f0f0;}"#));
        assert!(css.contains(
            r#"#st .node circle.state-end{fill:#010101;stroke:#707070;stroke-width:1.5;}"#
        ));
        assert!(css.contains(r#"#st .node rect{fill:#030303;stroke:#040404;stroke-width:4px;}"#));
        assert!(css.contains(
            r#"#st .statediagram-cluster rect{fill:#050505;stroke:#040404;stroke-width:4px;}"#
        ));
        assert!(css.contains(r#"#st .statediagram-note text{fill:#b0b0b0;}"#));
        assert!(css.contains(
            r#"#st .statediagramTitleText{text-anchor:middle;font-size:18px;fill:#101010;}"#
        ));
        assert!(css.contains(
            r##"#st [id$="-dependencyStart"],#st [id$="-dependencyEnd"]{fill:#303030;stroke:#303030;stroke-width:4;}"##
        ));
    }

    #[test]
    fn state_css_preserves_nullish_and_truthy_stroke_width_fallbacks() {
        for (stroke_width, edge_width, dependency_width) in [
            (json!(0), "0px", "1"),
            (json!("0"), "0px", "0"),
            (json!(null), "1px", "1"),
        ] {
            let css = state_css(
                "st",
                &StateSvgModel::default(),
                &json!({"themeVariables": {"strokeWidth": stroke_width}}),
            );
            assert!(css.contains(&format!(
                "#st .edge-thickness-normal{{stroke-width:{edge_width};}}"
            )));
            assert!(css.contains(&format!(
                r##"#st [id$="-dependencyStart"],#st [id$="-dependencyEnd"]{{fill:#333333;stroke:#333333;stroke-width:{dependency_width};}}"##
            )));
        }
    }

    #[test]
    fn state_css_emits_neo_cluster_theme_rules() {
        let cfg = json!({
            "look": "neo",
            "themeVariables": {
                "mainBkg": "#606060",
                "stateBorder": "#040404",
                "strokeWidth": 4,
                "useGradient": true,
                "gradientStart": "#112233",
                "gradientStop": "#445566",
                "dropShadow": "url(#drop-shadow)",
                "radius": 3
            }
        });

        let css = state_css("st", &StateSvgModel::default(), &cfg);

        assert!(css.contains(
            r##"#st [data-look="neo"].statediagram-cluster rect{fill:#606060;stroke:url(#st-gradient);stroke-width:4;}"##
        ));
        assert!(css.contains(
            r##"#st [data-look="neo"].statediagram-cluster rect.outer{rx:3px;ry:3px;filter:url(#st-drop-shadow);}"##
        ));
    }
}
