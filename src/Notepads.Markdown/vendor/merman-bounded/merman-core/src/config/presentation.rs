//! Re-evaluate authored palettes without promoting renderer defaults to initialize inputs.

use super::{MermaidConfig, clone_value_nonrecursive, drop_value_nonrecursive};
use crate::{theme, theme_color::ColorError};
use serde_json::{Map, Value};

pub(crate) fn apply_external_presentation_palette(
    effective: &mut MermaidConfig,
    defaults: &MermaidConfig,
    non_paint_defaults: &MermaidConfig,
    initialize: &MermaidConfig,
    source: &MermaidConfig,
    detector_writes: Option<&[String]>,
) -> Result<(), ColorError> {
    let mut raw = non_paint_defaults.clone();
    raw.deep_merge(initialize.as_value());
    raw.deep_merge(source.as_value());
    let detector_writes = detector_writes.unwrap_or_default();
    for path in detector_writes {
        if path.is_empty() {
            if let Some(variables) = effective.as_value().get("themeVariables") {
                raw.set_value("themeVariables", clone_value_nonrecursive(variables));
            }
        } else if path == "themeVariables" || path.starts_with("themeVariables.") {
            if let Some(value) = value_at(effective, path) {
                raw.set_value(path, clone_value_nonrecursive(value));
            } else {
                remove_value(&mut raw, path);
            }
        }
    }
    effective.set_value(
        "themeVariables",
        raw.as_value()
            .get("themeVariables")
            .map(clone_value_nonrecursive)
            .unwrap_or_else(|| Value::Object(Map::new())),
    );
    theme::apply_theme_defaults(effective)?;

    // These roots are emitted by Presentation's compiler and precede palette roles in
    // their terminal readers. Replace only defaults for which that reader has an
    // actual selected-palette value; Packet/C4 and Treemap surfaces have no such role.
    for (path, role, fallback_role) in [
        ("radar.axisColor", "radar.axisColor", None),
        ("radar.graticuleColor", "radar.graticuleColor", None),
        ("treemap.titleColor", "titleColor", Some("textColor")),
        ("treemap.labelColor", "textColor", None),
        ("treemap.valueColor", "textColor", None),
        ("darkMode", "darkMode", None),
    ] {
        if value_at(defaults, path).is_none()
            || value_at(initialize, path).is_some()
            || value_at(source, path).is_some()
            || detector_writes.iter().any(|write| claims(write, path))
        {
            continue;
        }
        let role = format!("themeVariables.{role}");
        let value = value_at(effective, &role).or_else(|| {
            fallback_role.and_then(|role| value_at(effective, &format!("themeVariables.{role}")))
        });
        if let Some(value) = value {
            effective.set_value(path, clone_value_nonrecursive(value));
        }
    }

    // Fill holes rather than merging arrays: an external scale array owns its complete
    // order and length, while extension roles absent upstream retain their host fallback.
    if let Some(defaults) = defaults.as_value().get("themeVariables") {
        let mut variables = effective
            .as_value()
            .get("themeVariables")
            .map(clone_value_nonrecursive)
            .unwrap_or_else(|| Value::Object(Map::new()));
        // Optional override roles retain the existing readers' selected-palette fallback.
        // Filling these aliases from a host preset would mask the already resolved color.
        let mut fallbacks = clone_value_nonrecursive(defaults);
        if let Some(fallbacks) = fallbacks.as_object_mut() {
            for (role, fallback) in [
                ("nodeTextColor", "textColor"),
                ("stateBorder", "nodeBorder"),
                ("requirementEdgeLabelBackground", "edgeLabelBackground"),
                ("commitLineColor", "lineColor"),
                ("emSwimlaneBackgroundOdd", "emSwimlaneBackground"),
                ("emSwimlaneBackgroundStroke", "emSwimlaneBorder"),
            ] {
                if variables.get(role).is_none()
                    && variables.get(fallback).and_then(Value::as_str).is_some()
                {
                    fallbacks.remove(role);
                }
            }
        }
        fill_missing(&mut variables, &fallbacks);
        effective.set_value("themeVariables", variables);
    }
    Ok(())
}

fn value_at<'a>(config: &'a MermaidConfig, path: &str) -> Option<&'a Value> {
    path.split('.')
        .try_fold(config.as_value(), |value, key| value.get(key))
}

fn remove_value(config: &mut MermaidConfig, path: &str) {
    let (parent_path, key) = path.rsplit_once('.').unwrap_or(("", path));
    let mut parent = config.as_value_mut();
    if !parent_path.is_empty() {
        for segment in parent_path.split('.') {
            let Some(child) = parent.get_mut(segment) else {
                return;
            };
            parent = child;
        }
    }
    if let Some(parent) = parent.as_object_mut()
        && let Some(value) = parent.remove(key)
    {
        drop_value_nonrecursive(value);
    }
}

fn claims(write: &str, path: &str) -> bool {
    write.is_empty()
        || write == path
        || path
            .strip_prefix(write)
            .is_some_and(|suffix| suffix.starts_with('.'))
}

fn fill_missing(value: &mut Value, defaults: &Value) {
    let mut pending = vec![(value, defaults)];
    while let Some((value, defaults)) = pending.pop() {
        let (Value::Object(value), Value::Object(defaults)) = (value, defaults) else {
            continue;
        };
        for (key, default) in defaults {
            if !value.contains_key(key) {
                value.insert(key.clone(), clone_value_nonrecursive(default));
            }
        }
        for (key, child) in value.iter_mut() {
            if let Some(default) = defaults.get(key) {
                pending.push((child, default));
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn palette_fallback_fills_nested_holes_without_extending_authored_arrays() {
        let mut palette = json!({"scale": ["source"], "nested": {"color": "source"}});
        fill_missing(
            &mut palette,
            &json!({
                "scale": ["host", "extra"], "nested": {"color": "host", "missing": "fallback"},
                "extension": "fallback",
            }),
        );
        assert_eq!(
            palette,
            json!({
                "scale": ["source"], "nested": {"color": "source", "missing": "fallback"},
                "extension": "fallback",
            })
        );
    }

    #[test]
    fn detector_tracking_records_same_value_writes_and_only_applied_merge_leaves() {
        let mut config = MermaidConfig::from_value(json!({"themeVariables": {
            "textColor": "#abcdef", "shape": {"fill": "#123456"}
        }}));
        let before = config.begin_mutation_tracking();
        config.set_value("themeVariables.textColor", json!("#abcdef"));
        config.deep_merge(&json!({"themeVariables": {
            "textColor": "#abcdef", "ignored": null, "shape": "incompatible"
        }}));
        assert_eq!(config, before);
        assert_eq!(
            config.take_mutation_paths(&before),
            ["themeVariables.textColor", "themeVariables.textColor"]
        );
        let before = config.begin_mutation_tracking();
        let _ = config.as_value_mut();
        assert!(config.take_mutation_paths(&before).is_empty());
        let before = config.begin_mutation_tracking();
        config.as_value_mut()["themeVariables"]["shape"]["fill"] = json!("#234567");
        assert_eq!(
            config.take_mutation_paths(&before),
            ["themeVariables.shape.fill"]
        );
        let before = config.begin_mutation_tracking();
        config = MermaidConfig::from_value(json!({"themeVariables": {"textColor": "#123456"}}));
        assert_eq!(config.take_mutation_paths(&before), [""]);
    }
}
