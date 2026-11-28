use std::collections::BTreeMap;

#[derive(Debug, Default)]
pub(crate) struct ZenumlSourceStyle {
    pub(crate) typography: super::ZenumlTypography,
    colors: BTreeMap<&'static str, String>,
}

impl ZenumlSourceStyle {
    pub(crate) fn new(config: &merman_core::MermaidConfig) -> Self {
        let namespace = crate::config::value_at(config.as_value(), &["themeVariables", "zenuml"]);
        let configured = |key: &str| {
            namespace
                .and_then(|value| value.get(key))
                .and_then(serde_json::Value::as_str)
        };
        let host = merman_core::presentation_defaults_active_internal(config);
        let mut colors = BTreeMap::new();
        for (key, inherited, global) in PAINT_SLOTS {
            let value = configured(key)
                .or_else(|| inherited.iter().find_map(|key| configured(key)))
                .or_else(|| host.then(|| config.get_str(global)).flatten());
            if let Some(value) = value {
                colors.insert(*key, value.to_owned());
            }
        }
        Self {
            typography: super::ZenumlTypography::from_config(config),
            colors,
        }
    }

    pub(crate) fn configured_color(&self, key: &str) -> Option<&str> {
        self.colors.get(key).map(String::as_str)
    }

    pub(crate) fn color<'a>(&'a self, key: &str, fallback: &'a str) -> &'a str {
        self.configured_color(key).unwrap_or(fallback)
    }
}

const PAINT_SLOTS: &[(&str, &[&str], &str)] = &[
    ("backgroundColor", &[], "themeVariables.background"),
    ("borderColor", &[], "themeVariables.primaryBorderColor"),
    ("titleTextColor", &["textColor"], "themeVariables.textColor"),
    ("textColor", &[], "themeVariables.textColor"),
    (
        "messageTextColor",
        &["textColor"],
        "themeVariables.signalTextColor",
    ),
    (
        "messageLineColor",
        &["lineColor"],
        "themeVariables.signalColor",
    ),
    (
        "lifelineColor",
        &["lineColor"],
        "themeVariables.actorLineColor",
    ),
    (
        "sequenceNumberTextColor",
        &["subtleTextColor", "textColor"],
        "themeVariables.secondaryTextColor",
    ),
    (
        "fragmentBorderColor",
        &["borderColor"],
        "themeVariables.clusterBorder",
    ),
    (
        "actorBackgroundColor",
        &["surfaceColor"],
        "themeVariables.actorBkg",
    ),
    (
        "actorBorderColor",
        &["borderColor"],
        "themeVariables.actorBorder",
    ),
    (
        "actorTextColor",
        &["textColor"],
        "themeVariables.actorTextColor",
    ),
    (
        "noteBackgroundColor",
        &["surfaceColor"],
        "themeVariables.noteBkgColor",
    ),
    (
        "noteBorderColor",
        &["borderColor"],
        "themeVariables.noteBorderColor",
    ),
    (
        "noteTextColor",
        &["textColor"],
        "themeVariables.noteTextColor",
    ),
    (
        "groupBackgroundColor",
        &["surfaceColor"],
        "themeVariables.clusterBkg",
    ),
    (
        "groupBorderColor",
        &["borderColor"],
        "themeVariables.clusterBorder",
    ),
    ("groupTextColor", &["textColor"], "themeVariables.textColor"),
    (
        "activationBackgroundColor",
        &["mutedSurfaceColor", "surfaceColor"],
        "themeVariables.activationBkgColor",
    ),
    (
        "activationBorderColor",
        &["borderColor"],
        "themeVariables.activationBorderColor",
    ),
    (
        "mutedSurfaceColor",
        &["surfaceColor"],
        "themeVariables.tertiaryColor",
    ),
    (
        "fragmentTextColor",
        &["textColor"],
        "themeVariables.textColor",
    ),
    (
        "subtleTextColor",
        &["textColor"],
        "themeVariables.secondaryTextColor",
    ),
];
