//! Resolve diagram appearance while the source and initialization layers are still distinct.

use crate::{MermaidConfig, family, theme};
use serde_json::Value;

pub(crate) fn resolve_appearance(
    diagram_type: &str,
    source: &MermaidConfig,
    initialize: &MermaidConfig,
    presentation: &MermaidConfig,
    defaults: &MermaidConfig,
    effective: &mut MermaidConfig,
) -> bool {
    let mut external_palette = false;
    let section = family::config_namespace_for_diagram_type(diagram_type).unwrap_or(diagram_type);
    for key in ["theme", "look", "layout"] {
        // Mindmap's database chooses Cose only when the caller supplied no layout.
        // Resolve this while authored layers are available, before global defaults
        // become indistinguishable from an explicit request (including null).
        let mindmap_default = (section == "mindmap" && key == "layout")
            .then(|| Value::String("cose-bilkent".to_string()));
        let authored = [source, initialize]
            .into_iter()
            .find_map(|layer| read_appearance(layer, section, key));
        if key == "theme" {
            external_palette = authored.is_some_and(|value| value.as_str() != Some("null"));
        }
        let value = authored
            .or_else(|| read_appearance(presentation, section, key))
            .or(mindmap_default.as_ref())
            .or_else(|| read_appearance(defaults, section, key));
        let Some(value) = value else {
            continue;
        };
        let scoped_key = format!("{section}.{key}");
        let section_has_key = effective
            .as_value()
            .get(section)
            .and_then(|section| section.get(key))
            .is_some();
        if effective.as_value().get(key) != Some(value) {
            effective.set_value(key, value.clone());
        }
        // Mermaid keeps an existing scoped setting in step with the resolved top-level
        // setting, but does not create appearance keys in otherwise unrelated sections.
        if section_has_key && effective.as_value()[section].get(key) != Some(value) {
            effective.set_value(&scoped_key, value.clone());
        }
    }
    external_palette
}

fn read_appearance<'a>(layer: &'a MermaidConfig, section: &str, key: &str) -> Option<&'a Value> {
    let root = layer.as_value();
    [
        root.get(section).and_then(|section| section.get(key)),
        root.get(key),
    ]
    .into_iter()
    .flatten()
    .find(|value| match key {
        "theme" => value
            .as_str()
            .is_some_and(|name| name == "null" || theme::SUPPORTED_THEME_NAMES.contains(&name)),
        "look" => value
            .as_str()
            .is_some_and(|look| matches!(look, "classic" | "handDrawn" | "neo")),
        // The extensible layout registry owns invalid/unknown-name fallback. In
        // particular a JSON null is present in JavaScript, unlike undefined.
        "layout" => true,
        _ => false,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn resolve(initialize: Value, source: Value) -> MermaidConfig {
        let defaults = MermaidConfig::from_value(json!({
            "theme": "default", "look": "classic", "layout": "elk",
            "flowchart": { "theme": "redux-color", "look": "neo" },
        }));
        let initialize = MermaidConfig::from_value(initialize);
        let source = MermaidConfig::from_value(source);
        let mut effective = defaults.clone();
        effective.deep_merge(initialize.as_value());
        effective.deep_merge(source.as_value());
        resolve_appearance(
            "flowchart-v2",
            &source,
            &initialize,
            &MermaidConfig::empty_object(),
            &defaults,
            &mut effective,
        );
        effective
    }

    #[test]
    fn appearance_precedence_preserves_layers_and_explicit_defaults() {
        for (initialize, source, expected) in [
            (json!({}), json!({}), "redux-color"),
            (json!({ "theme": "default" }), json!({}), "default"),
            (
                json!({ "theme": "dark", "flowchart": { "theme": "forest" } }),
                json!({}),
                "forest",
            ),
            (
                json!({ "flowchart": { "theme": "forest" } }),
                json!({ "theme": "dark" }),
                "dark",
            ),
            (
                json!({ "theme": "dark" }),
                json!({ "theme": "forest", "flowchart": { "theme": "base" } }),
                "base",
            ),
        ] {
            let effective = resolve(initialize, source);
            assert_eq!(effective.get_str("theme"), Some(expected));
            assert_eq!(effective.get_str("flowchart.theme"), Some(expected));
        }
    }

    #[test]
    fn unusable_values_fall_through_without_rejecting_the_null_sentinel() {
        for invalid in [
            json!(null),
            json!(false),
            json!("constructor"),
            json!("bogus"),
        ] {
            let effective = resolve(
                json!({ "theme": "dark" }),
                json!({
                    "theme": "forest", "flowchart": { "theme": invalid, "look": "bogus" }
                }),
            );
            assert_eq!(effective.get_str("theme"), Some("forest"));
            assert_eq!(effective.get_str("look"), Some("neo"));
        }
        let effective = resolve(json!({}), json!({ "flowchart": { "theme": "null" } }));
        assert_eq!(effective.get_str("theme"), Some("null"));
    }

    #[test]
    fn mindmap_default_does_not_override_an_authored_layout() {
        for (initialize, source, expected) in [
            (json!({}), json!({}), json!("cose-bilkent")),
            (json!({"layout": "elk"}), json!({}), json!("elk")),
            (
                json!({}),
                json!({"layout": "tidy-tree"}),
                json!("tidy-tree"),
            ),
            (
                json!({"layout": "elk"}),
                json!({"layout": null}),
                Value::Null,
            ),
            (
                json!({}),
                json!({"mindmap": {"layout": "dagre"}}),
                json!("dagre"),
            ),
        ] {
            let defaults = MermaidConfig::from_value(json!({"layout": "elk"}));
            let mut effective = defaults.clone();
            let initialize = MermaidConfig::from_value(initialize);
            let source = MermaidConfig::from_value(source);
            effective.deep_merge(initialize.as_value());
            effective.deep_merge(source.as_value());
            resolve_appearance(
                "mindmap",
                &source,
                &initialize,
                &MermaidConfig::empty_object(),
                &defaults,
                &mut effective,
            );
            assert_eq!(effective.as_value()["layout"], expected);
        }
    }

    #[test]
    fn layout_values_reach_the_registry_without_appearance_validation() {
        for layout in [json!("custom-layout"), json!(null), json!(false)] {
            let effective = resolve(
                json!({ "layout": "dagre" }),
                json!({
                    "flowchart": { "layout": layout }
                }),
            );
            assert_eq!(effective.as_value()["layout"], layout);
            assert_eq!(effective.as_value()["flowchart"]["layout"], layout);
        }
    }
}
