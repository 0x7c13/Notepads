//! Usecase role colors and opt-in participant palettes from Mermaid 12 styles.ts.

use super::*;
use serde_json::Value;

pub(super) fn font_style(config: &Value) -> TextStyle {
    TextStyle {
        font_family: config_string(config, &["themeVariables", "fontFamily"])
            .or_else(|| config_string(config, &["fontFamily"])),
        font_size: config
            .pointer("/themeVariables/fontSize")
            .and_then(|value| {
                value
                    .as_f64()
                    .or_else(|| value.as_str()?.trim_end_matches("px").parse().ok())
            })
            .unwrap_or(16.0),
        ..Default::default()
    }
}

fn palette(config: &Value) -> &[Value] {
    if !matches!(
        config.get("theme").and_then(Value::as_str),
        Some("redux-color" | "redux-dark-color")
    ) {
        return &[];
    }
    config
        .pointer("/themeVariables/borderColorArray")
        .and_then(Value::as_array)
        .map_or(&[], Vec::as_slice)
}

fn look(config: &Value) -> String {
    let value = match config.get("look") {
        Some(Value::String(value)) => value.clone(),
        Some(Value::Number(value)) => value.to_string(),
        _ => String::new(),
    };
    if !value.is_empty()
        && value
            .bytes()
            .all(|ch| ch.is_ascii_alphanumeric() || ch == b'_' || ch == b'-')
    {
        value
    } else {
        "classic".into()
    }
}

pub(super) fn appearance_attributes(config: &Value, color_index: Option<usize>) -> String {
    let mut attributes = format!(r#" data-look="{}""#, look(config));
    if let Some(index) = color_index.filter(|_| !palette(config).is_empty()) {
        let _ = write!(
            attributes,
            r#" data-color-id="color-{}""#,
            index % palette(config).len()
        );
    }
    attributes
}

pub(super) fn write_css(out: &mut String, id: SvgDiagramId<'_>, cfg: &Value) {
    let token = |keys: &[&str], default: &str| {
        keys.iter()
            .find_map(|key| config_string(cfg, &["themeVariables", key]))
            .unwrap_or_else(|| default.into())
    };
    let main = token(&["mainBkg", "primaryColor"], "#ECECFF");
    let node_border = token(&["nodeBorder", "primaryColor"], "#9370DB");
    let body = token(&["usecaseBkg", "mainBkg"], &main);
    let border = token(
        &["usecaseBorder", "nodeBorder", "primaryColor"],
        &node_border,
    );
    let actor = token(&["usecaseActorBkg", "actorBkg", "mainBkg"], &main);
    let actor_border = token(
        &["usecaseActorBorder", "actorBorder", "primaryColor"],
        &node_border,
    );
    let boundary = token(&["usecaseBoundaryBkg", "clusterBkg"], "#ffffde");
    let boundary_border = token(&["usecaseBoundaryBorder", "clusterBorder"], "#aaaa33");
    let line = token(&["lineColor"], "#333");
    let include = token(&["usecaseIncludeLine", "lineColor"], &line);
    let extend = token(&["usecaseExtendLine", "lineColor"], &line);
    let text = token(&["primaryTextColor"], "#333");
    let root_text = token(&["textColor"], "#333");
    let actor_text = token(&["actorTextColor", "primaryTextColor"], &text);
    let title = token(&["titleColor", "primaryTextColor"], &text);
    let note = token(&["noteBkgColor"], "#fff5ad");
    let note_border = token(&["noteBorderColor"], "#aaaa33");
    let note_text = token(&["noteTextColor"], &text);
    let edge_background = token(&["edgeLabelBackground"], &main);
    let mut css = String::new();
    // HTML label metrics exclude the browser's default paragraph margins, as does
    // Mermaid's shared styles.ts reset. Apply it before the family-specific styles.
    let _ = super::super::css::write_mermaid_paragraph_css_to(&mut css, id);
    let font = font_style(cfg);
    let _ = write!(
        css,
        "#{id}{{font-family:{};font-size:{}px;fill:{root_text}}}",
        font.font_family.as_deref().unwrap_or("sans-serif"),
        fmt(font.font_size)
    );
    let look = look(cfg);
    let rotate = cfg.pointer("/usecase/colorScheme").and_then(Value::as_str) == Some("rotate");
    let backgrounds = cfg
        .pointer("/themeVariables/bkgColorArray")
        .and_then(Value::as_array);
    for (index, color) in palette(cfg).iter().enumerate() {
        let color = color
            .as_str()
            .map(str::to_owned)
            .unwrap_or_else(|| color.to_string());
        let fill = backgrounds
            .filter(|values| !values.is_empty())
            .map(|values| {
                let value = &values[index % values.len()];
                format!(
                    "fill:{};",
                    value
                        .as_str()
                        .map(str::to_owned)
                        .unwrap_or_else(|| value.to_string())
                )
            })
            .unwrap_or_default();
        let slot = format!(r##"#{id} [data-look="{look}"][data-color-id="color-{index}"]"##);
        let _ = write!(
            css,
            "{slot}.system-boundary rect.boundary-body,{slot}.system-boundary rect.boundary-tab,{slot}.system-boundary .boundary-body path,{slot}.system-boundary .boundary-tab path{{stroke:{color};{fill}}}"
        );
        if rotate {
            let _ = write!(
                css,
                "{slot}.usecase-element ellipse,{slot}.usecase-element rect{{stroke:{color};{fill}}}{slot}.usecase-element .usecase-business-marker{{stroke:{color}}}{slot}.usecase-actor .usecase-actor-glyph{{stroke:{color};{fill}}}"
            );
            if look != "handDrawn" {
                let _ = write!(
                    css,
                    "{slot}.usecase-actor .usecase-actor-glyph path,{slot}.usecase-actor .usecase-actor-glyph circle{{stroke:{color}}}"
                );
            }
        }
    }
    let _ = write!(
        css,
        r#"
#{id} text,#{id} .nodeLabel{{fill:{text};color:{text}}}
#{id} .usecase-element ellipse,#{id} .usecase-element rect{{fill:{body};stroke:{border};stroke-width:2px}}
#{id} .usecase-actor-shape,#{id} .usecase-actor-hollow,#{id} .usecase-actor-awesome,#{id} .usecase-actor-icon{{fill:{actor};stroke:{actor_border};stroke-width:2px}}
#{id} .usecase-actor .nodeLabel,#{id} .actor-label{{color:{actor_text};fill:{actor_text}}}
#{id} .usecase-actor .nodeLabel,#{id} .actor-label{{font-family:var(--mermaid-usecase-actor-font-family);font-size:var(--mermaid-usecase-actor-font-size);font-weight:var(--mermaid-usecase-actor-font-weight)}}
#{id} .usecase-element .nodeLabel,#{id} .usecase-label{{font-family:var(--mermaid-usecase-font-family);font-size:var(--mermaid-usecase-font-size);font-weight:var(--mermaid-usecase-font-weight)}}
#{id} .system-boundary rect.boundary-body,#{id} .system-boundary rect.boundary-tab,#{id} .system-boundary-package-tab{{fill:{boundary};stroke:{boundary_border};stroke-width:1px}}
#{id} .system-boundary-title text,#{id} .system-boundary-title span{{fill:{title};color:{title}}}
#{id} .relationship{{fill:none;stroke:{line};stroke-width:1px}}
#{id} .relationship-include{{stroke:{include};stroke-dasharray:3}}
#{id} .relationship-extend{{stroke:{extend};stroke-dasharray:3}}
#{id} .relationship-note{{stroke-dasharray:3}}
#{id} .marker{{fill:{line};stroke:{line}}}#{id} .marker.extension{{fill:{main};stroke:{line}}}
#{id} .usecase-note{{fill:{note};stroke:{note_border};color:{note_text}}}
#{id} .usecase-note .nodeLabel{{color:{note_text};fill:{note_text}}}
#{id} .usecase-json-table,#{id} .usecase-json-table rect,#{id} .usecase-json-cell{{fill:{main};stroke:{node_border}}}
#{id} .usecase-stereotype,#{id} .usecase-business-marker{{stroke:{border};fill:{text};color:{text}}}
#{id} .usecase-json-title,#{id} .usecase-json-key,#{id} .usecase-json-value{{fill:{text};color:{text}}}
#{id} .edgeLabel,#{id} .edgeLabel p{{background-color:{edge_background}}}
#{id} .edgeLabel .label rect{{fill:{edge_background}}}
@keyframes dash{{to{{stroke-dashoffset:0}}}}
#{id} .edge-animation-fast{{stroke-dasharray:9,5!important;stroke-dashoffset:900;animation:dash 20s linear infinite;stroke-linecap:round}}
#{id} .edge-animation-slow{{stroke-dasharray:9,5!important;stroke-dashoffset:900;animation:dash 50s linear infinite;stroke-linecap:round}}
"#
    );
    // Mermaid appends shared neo paint after family CSS. The role selectors must
    // name the actual glyph/body children to outrank direct neo path/rect rules.
    if look != "handDrawn" {
        let _ = write!(
            css,
            r#"#{id} .node.usecase-actor .usecase-actor-glyph path,#{id} .node.usecase-actor .usecase-actor-glyph circle{{stroke:{actor_border}}}
#{id} [data-look="{look}"].node.usecase-element ellipse,#{id} [data-look="{look}"].node.usecase-element rect{{fill:{body};stroke:{border}}}
#{id} [data-look="{look}"].node.usecase-element .usecase-business-marker{{stroke:{border}}}"#
        );
    }
    super::super::css::write_mermaid_common_neo_css(&mut css, id, cfg);
    // CSS is XML character data. Preserve declarations while preventing a token from closing style.
    out.push_str("<style>");
    util::escape_xml_raw_into(out, &css);
    out.push_str("</style>");
}
