// Notepads Markdown modifications: Copyright (c) 2026, Jiaqi (0x7c13) Liu.
// Original upstream MIT/Apache-2.0 licenses and notices are preserved.
use crate::MermaidConfig;
use crate::theme_color::{self, ColorAdjustment, ColorError};
use serde::Deserialize;
use serde_json::{Map, Value};
#[cfg(test)]
use std::collections::BTreeMap;
use std::sync::OnceLock;

mod base;

pub(crate) const SUPPORTED_THEME_NAMES: &[&str] = &[
    "default",
    "base",
    "dark",
    "forest",
    "neutral",
    "neo",
    "neo-dark",
    "redux",
    "redux-dark",
    "redux-color",
    "redux-dark-color",
];

const THEME_RUNTIME_SCHEMA_VERSION: u32 = 2;
#[cfg(test)]
const THEME_AUDIT_SCHEMA_VERSION: u32 = 1;

// Generated from the content-pinned Mermaid runtime by `xtask gen-theme-snapshot`.
static GENERATED_THEME_RUNTIME: OnceLock<GeneratedThemeRuntimeArtifact> = OnceLock::new();

#[cfg(test)]
static GENERATED_THEME_AUDIT: OnceLock<GeneratedThemeAuditArtifact> = OnceLock::new();

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct GeneratedThemeRuntimeStorage {
    schema_version: u32,
    provenance: GeneratedThemeProvenance,
    themes: Map<String, Value>,
    dark_mode_true_overrides: Map<String, Value>,
    oracle_case_count: usize,
}

#[derive(Debug)]
struct GeneratedThemeRuntimeArtifact {
    #[cfg(test)]
    provenance: GeneratedThemeProvenance,
    themes: Map<String, Value>,
    dark_mode_true: Map<String, Value>,
    oracle_case_count: usize,
}

#[cfg(test)]
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct GeneratedThemeAuditArtifact {
    schema_version: u32,
    provenance: GeneratedThemeProvenance,
    oracle_cases: Vec<Value>,
}

#[derive(Debug, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct GeneratedThemeProvenance {
    generator: String,
    mermaid_version: String,
    mermaid_package_sha256: String,
    mermaid_source_tag: String,
    mermaid_source_commit: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ThemeProgramKind {
    Default,
    Base,
    Dark,
    Forest,
    Neutral,
    Extended,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ThemeDependencyGraph {
    None,
    Default,
    Dark,
    Forest,
    Neutral,
    DarkenedScale,
    DarkenedScaleAndGit,
    DynamicGit,
}

/// Pure-Rust execution contract for one pinned Mermaid theme class.
///
/// The generated artifact owns exact release snapshots; this descriptor owns the operations and
/// input dependencies needed between the four `ThemeResolution` stages.
#[derive(Debug, Clone, Copy)]
struct ThemeProgram {
    name: &'static str,
    kind: ThemeProgramKind,
    dependencies: ThemeDependencyGraph,
    evaluated_color_inputs: &'static [&'static str],
}

const DEFAULT_COLOR_INPUTS: &[&str] = &[
    "primaryColor",
    "secondaryColor",
    "cScale0",
    "cScale1",
    "git0",
    "git1",
    "quadrant1Fill",
];
const DARK_COLOR_INPUTS: &[&str] = &[
    "primaryColor",
    "secondaryColor",
    "background",
    "cScale0",
    "cScale1",
    "quadrant1Fill",
];
const FOREST_COLOR_INPUTS: &[&str] = &[
    "primaryColor",
    "secondaryColor",
    "tertiaryColor",
    "cScale0",
    "cScale1",
    "git0",
    "git1",
    "quadrant1Fill",
];
const NEUTRAL_COLOR_INPUTS: &[&str] = &[
    "primaryColor",
    "secondaryColor",
    "border1",
    "cScale0",
    "cScale1",
    "quadrant1Fill",
];
const BASE_COLOR_INPUTS: &[&str] = &[
    "primaryColor",
    "secondaryColor",
    "tertiaryColor",
    "background",
    "cScale0",
    "cScale1",
    "git0",
    "git1",
    "quadrant1Fill",
];
const REDUX_COLOR_INPUTS: &[&str] = &[
    "primaryColor",
    "secondaryColor",
    "tertiaryColor",
    "background",
    "git0",
    "git1",
    "quadrant1Fill",
];

const THEME_PROGRAMS: &[ThemeProgram] = &[
    ThemeProgram::new(
        "default",
        ThemeProgramKind::Default,
        ThemeDependencyGraph::Default,
        DEFAULT_COLOR_INPUTS,
    ),
    ThemeProgram::new(
        "base",
        ThemeProgramKind::Base,
        ThemeDependencyGraph::None,
        BASE_COLOR_INPUTS,
    ),
    ThemeProgram::new(
        "dark",
        ThemeProgramKind::Dark,
        ThemeDependencyGraph::Dark,
        DARK_COLOR_INPUTS,
    ),
    ThemeProgram::new(
        "forest",
        ThemeProgramKind::Forest,
        ThemeDependencyGraph::Forest,
        FOREST_COLOR_INPUTS,
    ),
    ThemeProgram::new(
        "neutral",
        ThemeProgramKind::Neutral,
        ThemeDependencyGraph::Neutral,
        NEUTRAL_COLOR_INPUTS,
    ),
    ThemeProgram::new(
        "neo",
        ThemeProgramKind::Extended,
        ThemeDependencyGraph::None,
        BASE_COLOR_INPUTS,
    ),
    ThemeProgram::new(
        "neo-dark",
        ThemeProgramKind::Extended,
        ThemeDependencyGraph::DarkenedScale,
        BASE_COLOR_INPUTS,
    ),
    ThemeProgram::new(
        "redux",
        ThemeProgramKind::Extended,
        ThemeDependencyGraph::None,
        REDUX_COLOR_INPUTS,
    ),
    ThemeProgram::new(
        "redux-dark",
        ThemeProgramKind::Extended,
        ThemeDependencyGraph::DarkenedScaleAndGit,
        BASE_COLOR_INPUTS,
    ),
    ThemeProgram::new(
        "redux-color",
        ThemeProgramKind::Extended,
        ThemeDependencyGraph::None,
        BASE_COLOR_INPUTS,
    ),
    ThemeProgram::new(
        "redux-dark-color",
        ThemeProgramKind::Extended,
        ThemeDependencyGraph::DynamicGit,
        BASE_COLOR_INPUTS,
    ),
];

const THEME_ORACLE_CASE_COUNT: usize = THEME_PROGRAMS.len() * 5 + 11;

impl ThemeProgram {
    const fn new(
        name: &'static str,
        kind: ThemeProgramKind,
        dependencies: ThemeDependencyGraph,
        evaluated_color_inputs: &'static [&'static str],
    ) -> Self {
        Self {
            name,
            kind,
            dependencies,
            evaluated_color_inputs,
        }
    }

    fn resolve(requested: &str) -> &'static Self {
        THEME_PROGRAMS
            .iter()
            .find(|program| program.name == requested)
            .unwrap_or(&THEME_PROGRAMS[0])
    }

    fn default_snapshot(self) -> &'static Map<String, Value> {
        generated_theme_runtime()
            .themes
            .get(self.name)
            .and_then(Value::as_object)
            .unwrap_or_else(|| panic!("generated theme artifact is missing `{}`", self.name))
    }

    fn dark_mode_snapshot(self) -> &'static Map<String, Value> {
        generated_theme_runtime()
            .dark_mode_true
            .get(self.name)
            .and_then(Value::as_object)
            .unwrap_or_else(|| {
                panic!(
                    "generated theme artifact is missing darkMode=true `{}`",
                    self.name
                )
            })
    }

    fn calculation_snapshot(self, explicit: &Map<String, Value>) -> &'static Map<String, Value> {
        if explicit.get("darkMode").is_some_and(is_js_truthy) {
            self.dark_mode_snapshot()
        } else {
            self.default_snapshot()
        }
    }

    fn exact_snapshot(self, explicit: &Map<String, Value>) -> Option<&'static Map<String, Value>> {
        if self.kind != ThemeProgramKind::Base
            && explicit
                .keys()
                .all(|key| matches!(key.as_str(), "darkMode" | "fontFamily" | "fontSize"))
        {
            Some(self.calculation_snapshot(explicit))
        } else {
            None
        }
    }

    fn normalize_overrides(self, raw: Map<String, Value>) -> Map<String, Value> {
        if self.kind == ThemeProgramKind::Base {
            // initialize() copies into an empty options object before constructing the theme.
            // That merge drops nulls at this depth, but retains non-null objects. In particular,
            // an object supplied as a color must reach the evaluator and fail at its transform.
            return raw
                .into_iter()
                .filter(|(_, value)| !value.is_null())
                .collect();
        }
        let defaults = self.default_snapshot();
        raw.into_iter()
            .filter(|(key, value)| assign_with_depth_accepts_theme_value(defaults.get(key), value))
            .collect()
    }

    fn validate_evaluated_inputs(self, explicit: &Map<String, Value>) -> Result<(), ColorError> {
        for key in self.evaluated_color_inputs {
            let Some(value) = explicit.get(*key) else {
                continue;
            };
            if value.is_null() {
                continue;
            }
            let Value::String(color) = value else {
                return Err(ColorError::UnsupportedFormat {
                    input: value.to_string(),
                });
            };
            theme_color::ThemeColor::parse(color)?;
        }
        Ok(())
    }

    fn execute(self, config: &mut MermaidConfig) -> Result<(), ColorError> {
        match self.kind {
            ThemeProgramKind::Default => apply_default_theme_defaults(config),
            ThemeProgramKind::Base => apply_base_theme_defaults(config),
            ThemeProgramKind::Dark => apply_dark_theme_defaults(config),
            ThemeProgramKind::Forest => apply_forest_theme_defaults(config),
            ThemeProgramKind::Neutral => apply_neutral_theme_defaults(config),
            ThemeProgramKind::Extended => apply_snapshot_theme_defaults(config, self.name),
        }
    }

    fn apply_dependency_graph(
        self,
        explicit: &Map<String, Value>,
        calculated: &mut Map<String, Value>,
    ) -> Result<(), ColorError> {
        match self.dependencies {
            ThemeDependencyGraph::None => Ok(()),
            ThemeDependencyGraph::Default => apply_default_theme_dependencies(explicit, calculated),
            ThemeDependencyGraph::Dark => apply_dark_theme_dependencies(explicit, calculated),
            ThemeDependencyGraph::Forest => apply_forest_theme_dependencies(explicit, calculated),
            ThemeDependencyGraph::Neutral => apply_neutral_theme_dependencies(explicit, calculated),
            ThemeDependencyGraph::DarkenedScale => {
                apply_darkened_scale_dependencies(explicit, calculated)
            }
            ThemeDependencyGraph::DarkenedScaleAndGit => {
                apply_darkened_scale_dependencies(explicit, calculated)?;
                apply_dynamic_git_dependencies(explicit, calculated)
            }
            ThemeDependencyGraph::DynamicGit => {
                apply_dynamic_git_dependencies(explicit, calculated)
            }
        }
    }
}

fn get_truthy_string(map: &Map<String, Value>, key: &str) -> Option<String> {
    map.get(key)
        .and_then(|v| v.as_str())
        .filter(|s| !s.is_empty())
        .map(|s| s.to_string())
}

fn is_js_truthy(value: &Value) -> bool {
    match value {
        Value::Null => false,
        Value::Bool(value) => *value,
        Value::Number(value) => value.as_f64().is_some_and(|value| value != 0.0),
        Value::String(value) => !value.is_empty(),
        Value::Array(_) | Value::Object(_) => true,
    }
}

fn assign_with_depth_accepts_theme_value(default: Option<&Value>, source: &Value) -> bool {
    let source_is_non_null_object = matches!(source, Value::Array(_) | Value::Object(_));
    let source_is_object = matches!(source, Value::Null | Value::Array(_) | Value::Object(_));
    let default_is_object = default
        .is_some_and(|value| matches!(value, Value::Null | Value::Array(_) | Value::Object(_)));

    // Mermaid's site-config merge reaches themeVariables with depth=1. Non-null objects recurse
    // once (and therefore retain nested nulls); dissimilar object/scalar values do not clobber.
    source_is_non_null_object && (default.is_none() || default_is_object)
        || !source_is_object && !default_is_object
}

fn required_color(map: &Map<String, Value>, key: &str) -> Result<String, ColorError> {
    get_truthy_string(map, key).ok_or_else(|| ColorError::UnsupportedFormat {
        input: map
            .get(key)
            .map(Value::to_string)
            .unwrap_or_else(|| format!("missing theme color `{key}`")),
    })
}

fn value_is_missing(map: &Map<String, Value>, key: &str) -> bool {
    map.get(key).is_none_or(|value| !is_js_truthy(value))
}

fn set_if_missing(map: &mut Map<String, Value>, key: &str, value: Value) {
    if value_is_missing(map, key) {
        map.insert(key.to_string(), value);
    }
}

fn set_string_if_missing(map: &mut Map<String, Value>, key: &str, value: impl Into<String>) {
    set_if_missing(map, key, Value::String(value.into()));
}

fn set_string_if_missing_with(
    map: &mut Map<String, Value>,
    key: &str,
    value: impl FnOnce() -> Result<String, ColorError>,
) -> Result<(), ColorError> {
    if value_is_missing(map, key) {
        map.insert(key.to_string(), Value::String(value()?));
    }
    Ok(())
}

fn set_derived_string_unless_explicit(
    map: &mut Map<String, Value>,
    explicit: &Map<String, Value>,
    key: &str,
    value: impl Into<String>,
) {
    if !explicit.contains_key(key) {
        map.insert(key.to_string(), Value::String(value.into()));
    }
}

fn theme_variables_map(config: &MermaidConfig) -> Map<String, Value> {
    match config.as_value().get("themeVariables") {
        Some(Value::Object(m)) => m.clone(),
        _ => Map::new(),
    }
}

fn generated_theme_runtime() -> &'static GeneratedThemeRuntimeArtifact {
    GENERATED_THEME_RUNTIME.get_or_init(|| {
        let storage: GeneratedThemeRuntimeStorage =
            serde_json::from_str(include_str!("generated/theme_variables_12_1_0.json"))
                .expect("generated Mermaid theme runtime JSON is valid");
        assert_eq!(storage.schema_version, THEME_RUNTIME_SCHEMA_VERSION);
        assert_generated_theme_provenance(&storage.provenance);
        let dark_mode_true = storage
            .dark_mode_true_overrides
            .into_iter()
            .map(|(name, changes)| {
                let base = storage.themes[&name]
                    .as_object()
                    .expect("generated theme base is an object");
                (name, Value::Object(restore_theme_overrides(base, changes)))
            })
            .collect();
        let artifact = GeneratedThemeRuntimeArtifact {
            #[cfg(test)]
            provenance: storage.provenance,
            themes: storage.themes,
            dark_mode_true,
            oracle_case_count: storage.oracle_case_count,
        };
        assert_eq!(artifact.oracle_case_count, THEME_ORACLE_CASE_COUNT);
        for program in THEME_PROGRAMS {
            assert!(
                artifact
                    .themes
                    .get(program.name)
                    .is_some_and(Value::is_object)
            );
            assert!(
                artifact
                    .dark_mode_true
                    .get(program.name)
                    .is_some_and(Value::is_object)
            );
        }
        artifact
    })
}

// Schema 2 stores exact top-level set/remove deltas. Assigned nulls and objects replace
// whole values, unlike JSON merge patch. Restore the full maps once before theme evaluation.
fn restore_theme_overrides(base: &Map<String, Value>, changes: Value) -> Map<String, Value> {
    let Value::Object(mut changes) = changes else {
        panic!("generated theme overrides are an object");
    };
    let Value::Object(assignments) = changes.remove("set").expect("generated overrides have set")
    else {
        panic!("generated theme assignments are an object");
    };
    let Value::Array(removals) = changes
        .remove("remove")
        .expect("generated overrides have remove")
    else {
        panic!("generated theme removals are an array");
    };
    assert!(
        changes.is_empty(),
        "unknown generated theme override fields"
    );
    let mut restored = base.clone();
    for key in removals {
        let key = key.as_str().expect("generated theme removal is a string");
        assert!(
            restored.remove(key).is_some(),
            "generated removal requires a base key"
        );
    }
    restored.extend(assignments);
    restored
}

fn assert_generated_theme_provenance(provenance: &GeneratedThemeProvenance) {
    assert_eq!(
        provenance.mermaid_version,
        crate::baseline::PINNED_MERMAID_BASELINE_VERSION
    );
    assert_eq!(
        provenance.mermaid_source_tag,
        crate::baseline::PINNED_MERMAID_BASELINE_TAG
    );
    assert_eq!(
        provenance.generator,
        "cargo run -p xtask -- gen-theme-snapshot"
    );
    assert_eq!(provenance.mermaid_source_commit.len(), 40);
    assert_eq!(provenance.mermaid_package_sha256.len(), 64);
}

#[cfg(test)]
fn generated_theme_audit() -> &'static GeneratedThemeAuditArtifact {
    GENERATED_THEME_AUDIT.get_or_init(|| {
        let artifact: GeneratedThemeAuditArtifact = serde_json::from_str(include_str!(concat!(
            env!("CARGO_MANIFEST_DIR"), "/../fixtures/_verification/theme_variables_oracle_12_1_0.json"
        )))
        .expect("generated Mermaid theme audit JSON is valid");
        assert_eq!(artifact.schema_version, THEME_AUDIT_SCHEMA_VERSION);
        assert_generated_theme_provenance(&artifact.provenance);
        assert_eq!(artifact.provenance, generated_theme_runtime().provenance);
        assert_eq!(artifact.oracle_cases.len(), THEME_ORACLE_CASE_COUNT);
        assert_eq!(
            artifact.oracle_cases.len(),
            generated_theme_runtime().oracle_case_count
        );
        artifact
    })
}

fn merge_theme_variable_defaults(target: &mut Map<String, Value>, defaults: &Map<String, Value>) {
    for (key, default_value) in defaults {
        match (target.get_mut(key), default_value) {
            (Some(Value::Object(target_map)), Value::Object(default_map)) => {
                merge_theme_variable_defaults(target_map, default_map);
            }
            (Some(Value::Null), _) => {
                target.insert(key.clone(), default_value.clone());
            }
            (Some(Value::String(current)), _) if current.trim().is_empty() => {
                target.insert(key.clone(), default_value.clone());
            }
            (None, _) => {
                target.insert(key.clone(), default_value.clone());
            }
            _ => {}
        }
    }
}

fn finish_theme_defaults(
    config: &mut MermaidConfig,
    theme: &str,
    tv: Map<String, Value>,
) -> Result<(), ColorError> {
    let explicit = theme_variables_map(config);
    let resolution = ThemeResolution::new(theme, explicit, tv)?;
    config.set_value(
        "themeVariables",
        Value::Object(resolution.into_resolved_variables()),
    );
    Ok(())
}

#[cfg(test)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ThemeResolutionStage {
    DefaultSnapshot,
    OverridesApplied,
    Calculated,
    ExplicitReplay,
}

#[cfg(test)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ThemeValueOrigin {
    DefaultSnapshot,
    Calculated,
    ExplicitOverride,
}

#[derive(Debug, Clone)]
struct ThemeStageSnapshot {
    #[cfg(test)]
    stage: ThemeResolutionStage,
    variables: Map<String, Value>,
    #[cfg(test)]
    origins: BTreeMap<String, ThemeValueOrigin>,
}

impl ThemeStageSnapshot {
    fn from_variables(variables: Map<String, Value>) -> Self {
        Self {
            #[cfg(test)]
            stage: ThemeResolutionStage::Calculated,
            #[cfg(test)]
            origins: variables
                .keys()
                .map(|key| (key.clone(), ThemeValueOrigin::Calculated))
                .collect(),
            variables,
        }
    }

    fn overlay(&mut self, values: &Map<String, Value>, preserve_object_keys: bool) {
        for (key, value) in values {
            if preserve_object_keys
                && let Some(Value::Object(generated)) = self.variables.get_mut(key)
                && let Value::Object(explicit) = value
            {
                // Mermaid 12.1 applyOverride uses a shallow object spread after updateColors.
                generated.extend(
                    explicit
                        .iter()
                        .map(|(key, value)| (key.clone(), value.clone())),
                );
            } else {
                self.variables.insert(key.clone(), value.clone());
            }
            #[cfg(test)]
            self.origins
                .insert(key.clone(), ThemeValueOrigin::ExplicitOverride);
        }
    }
}

/// Ordered theme resolution shared by every family renderer.
/// Tests retain intermediate snapshots and value origins to audit the same calculation;
/// production carries only the variables needed by the next stage.
#[derive(Debug, Clone)]
struct ThemeResolution {
    #[cfg(test)]
    default_snapshot: ThemeStageSnapshot,
    #[cfg(test)]
    overrides_applied: ThemeStageSnapshot,
    #[cfg(test)]
    calculated: ThemeStageSnapshot,
    explicit_replay: ThemeStageSnapshot,
}

impl ThemeResolution {
    fn new(
        theme: &str,
        explicit: Map<String, Value>,
        calculated: Map<String, Value>,
    ) -> Result<Self, ColorError> {
        let program = ThemeProgram::resolve(theme);
        let has_user_theme_variables = !explicit.is_empty();
        let default_variables = program.default_snapshot();
        #[cfg(test)]
        let default_snapshot = ThemeStageSnapshot {
            stage: ThemeResolutionStage::DefaultSnapshot,
            variables: default_variables.clone(),
            origins: default_variables
                .keys()
                .map(|key| (key.clone(), ThemeValueOrigin::DefaultSnapshot))
                .collect(),
        };

        #[cfg(test)]
        let overrides_applied = {
            let mut snapshot = default_snapshot.clone();
            snapshot.stage = ThemeResolutionStage::OverridesApplied;
            snapshot.overlay(&explicit, false);
            snapshot
        };

        let mut calculated_snapshot = ThemeStageSnapshot::from_variables(calculated);

        if let Some(snapshot) = program.exact_snapshot(&explicit) {
            // Generated snapshots are exact calculation-stage results for branch-only inputs.
            // Typography inputs do not affect updateColors() and are replayed below.
            calculated_snapshot = ThemeStageSnapshot::from_variables(snapshot.clone());
        } else if program.kind != ThemeProgramKind::Base {
            let snapshot = program.calculation_snapshot(&explicit);
            merge_theme_variable_defaults(&mut calculated_snapshot.variables, snapshot);
            #[cfg(test)]
            for key in snapshot.keys() {
                calculated_snapshot
                    .origins
                    .entry(key.clone())
                    .or_insert(ThemeValueOrigin::DefaultSnapshot);
            }

            #[cfg(test)]
            let before_dependencies = calculated_snapshot.variables.clone();
            if program.kind == ThemeProgramKind::Extended {
                apply_extended_theme_visible_derivations(
                    theme,
                    &explicit,
                    &mut calculated_snapshot.variables,
                )?;
            }
            program.apply_dependency_graph(&explicit, &mut calculated_snapshot.variables)?;
            // Only default runs updateColors() in its constructor. Its first-pass
            // container stroke survives later secondary-border overrides.
            if program.kind != ThemeProgramKind::Default
                && !explicit.contains_key("flowContainerStroke")
                && let Some(border) = calculated_snapshot
                    .variables
                    .get("secondaryBorderColor")
                    .cloned()
            {
                calculated_snapshot
                    .variables
                    .insert("flowContainerStroke".to_string(), border);
            }
            #[cfg(test)]
            for (key, value) in &calculated_snapshot.variables {
                if before_dependencies.get(key) != Some(value) {
                    calculated_snapshot
                        .origins
                        .insert(key.clone(), ThemeValueOrigin::Calculated);
                }
            }
        }

        #[cfg(test)]
        let calculated = calculated_snapshot.clone();
        let mut explicit_replay = calculated_snapshot;
        #[cfg(test)]
        {
            explicit_replay.stage = ThemeResolutionStage::ExplicitReplay;
        }

        // `theme-default` constructs and updates its color scale before calculate() applies
        // overrides. A second update darkens the already-created cScale values, while peer and
        // inverse values retain their first-pass values. Restore the generated no-override palette
        // baseline before replaying explicit values; this is why a font-only override must not
        // change Radar/Kanban/Mindmap/Timeline colors.
        if has_user_theme_variables && theme == "default" {
            restore_default_baseline_palette(&mut explicit_replay, default_variables);
        }
        explicit_replay.overlay(&explicit, true);
        // Mermaid base.calculate() disables the inferred gradient after replay so an
        // explicitly supplied node stroke paints under the default neo look.
        if program.kind == ThemeProgramKind::Base
            && explicit.contains_key("nodeBorder")
            && !explicit.contains_key("useGradient")
        {
            explicit_replay
                .variables
                .insert("useGradient".to_string(), Value::Bool(false));
            #[cfg(test)]
            explicit_replay
                .origins
                .insert("useGradient".to_string(), ThemeValueOrigin::Calculated);
        }

        Ok(Self {
            #[cfg(test)]
            default_snapshot,
            #[cfg(test)]
            overrides_applied,
            #[cfg(test)]
            calculated,
            explicit_replay,
        })
    }

    fn into_resolved_variables(self) -> Map<String, Value> {
        self.explicit_replay.variables
    }
}

fn restore_default_baseline_palette(
    target: &mut ThemeStageSnapshot,
    baseline: &Map<String, Value>,
) {
    for prefix in [
        "cScale",
        "cScalePeer",
        "cScaleInv",
        "cScaleLabel",
        "surface",
        "surfacePeer",
    ] {
        for index in 0..12 {
            let key = format!("{prefix}{index}");
            if let Some(value) = baseline.get(&key) {
                #[cfg(test)]
                target
                    .origins
                    .insert(key.clone(), ThemeValueOrigin::DefaultSnapshot);
                target.variables.insert(key, value.clone());
            }
        }
    }
    for index in 1..=12 {
        let key = format!("pie{index}");
        if let Some(value) = baseline.get(&key) {
            #[cfg(test)]
            target
                .origins
                .insert(key.clone(), ThemeValueOrigin::DefaultSnapshot);
            target.variables.insert(key, value.clone());
        }
    }
    if let Some(value) = baseline.get("scaleLabelColor") {
        target
            .variables
            .insert("scaleLabelColor".to_string(), value.clone());
        #[cfg(test)]
        target.origins.insert(
            "scaleLabelColor".to_string(),
            ThemeValueOrigin::DefaultSnapshot,
        );
    }
}

fn mermaid_default_font_family() -> Value {
    Value::String("\"trebuchet ms\", verdana, arial, sans-serif".to_string())
}

fn mk_border(color: &str, dark_mode: bool) -> Result<String, ColorError> {
    theme_color::adjust(
        color,
        ColorAdjustment::hsl(0.0, -40.0, if dark_mode { 10.0 } else { -10.0 }),
    )
}

fn ensure_gradient_theme_defaults(tv: &mut Map<String, Value>) {
    let primary_border_color =
        get_truthy_string(tv, "primaryBorderColor").unwrap_or_else(|| "#9370DB".to_string());
    let secondary_border_color = get_truthy_string(tv, "secondaryBorderColor")
        .unwrap_or_else(|| primary_border_color.clone());

    set_if_missing(tv, "useGradient", Value::Bool(true));
    set_if_missing(tv, "gradientStart", Value::String(primary_border_color));
    set_if_missing(tv, "gradientStop", Value::String(secondary_border_color));
}

fn ensure_xychart_theme_defaults(tv: &mut Map<String, Value>, default_palette: &str) {
    let background = get_truthy_string(tv, "background").unwrap_or_else(|| "white".to_string());
    let primary_text = get_truthy_string(tv, "primaryTextColor")
        .or_else(|| get_truthy_string(tv, "textColor"))
        .unwrap_or_else(|| "#333".to_string());

    let mut xy = match tv.get("xyChart") {
        Some(Value::Object(m)) => m.clone(),
        _ => Map::new(),
    };

    set_if_missing(
        &mut xy,
        "backgroundColor",
        Value::String(background.clone()),
    );
    for key in [
        "titleColor",
        "dataLabelColor",
        "xAxisTitleColor",
        "xAxisLabelColor",
        "xAxisTickColor",
        "xAxisLineColor",
        "yAxisTitleColor",
        "yAxisLabelColor",
        "yAxisTickColor",
        "yAxisLineColor",
    ] {
        set_if_missing(&mut xy, key, Value::String(primary_text.clone()));
    }
    set_if_missing(
        &mut xy,
        "plotColorPalette",
        Value::String(default_palette.to_string()),
    );

    tv.insert("xyChart".to_string(), Value::Object(xy));
}

fn apply_quadrant_theme_defaults(
    tv: &mut Map<String, Value>,
    fill_base: &str,
    text_base: &str,
    border_base: &str,
) -> Result<(), ColorError> {
    set_string_if_missing(tv, "quadrant1Fill", fill_base);
    for (key, adjustment) in [
        ("quadrant2Fill", 5.0),
        ("quadrant3Fill", 10.0),
        ("quadrant4Fill", 15.0),
    ] {
        set_string_if_missing(
            tv,
            key,
            theme_color::adjust(
                fill_base,
                ColorAdjustment::rgb(adjustment, adjustment, adjustment),
            )?,
        );
    }

    set_string_if_missing(tv, "quadrant1TextFill", text_base);
    for (key, adjustment) in [
        ("quadrant2TextFill", -5.0),
        ("quadrant3TextFill", -10.0),
        ("quadrant4TextFill", -15.0),
    ] {
        set_string_if_missing(
            tv,
            key,
            theme_color::adjust(
                text_base,
                ColorAdjustment::rgb(adjustment, adjustment, adjustment),
            )?,
        );
    }

    // Upstream omits the amount argument. Khroma therefore preserves the hue and saturation but
    // serializes the lightness as NaN. Evaluate this even when the point color is explicit: the
    // JavaScript expression computes it during updateColors() before replaying explicit values.
    let quadrant1_fill = required_color(tv, "quadrant1Fill")?;
    let point_fill = if theme_color::is_dark(&quadrant1_fill)? {
        theme_color::lighten(&quadrant1_fill, f64::NAN)?
    } else {
        theme_color::darken(&quadrant1_fill, f64::NAN)?
    };
    set_string_if_missing(tv, "quadrantPointFill", point_fill);

    for key in [
        "quadrantPointTextFill",
        "quadrantXAxisTextFill",
        "quadrantYAxisTextFill",
        "quadrantTitleFill",
    ] {
        set_string_if_missing(tv, key, text_base);
    }
    for key in [
        "quadrantInternalBorderStrokeFill",
        "quadrantExternalBorderStrokeFill",
    ] {
        set_string_if_missing(tv, key, border_base);
    }
    Ok(())
}

fn apply_current_quadrant_theme_defaults(tv: &mut Map<String, Value>) -> Result<(), ColorError> {
    let fill_base = required_color(tv, "primaryColor")?;
    let text_base = required_color(tv, "primaryTextColor")?;
    let border_base = required_color(tv, "primaryBorderColor")?;
    apply_quadrant_theme_defaults(tv, &fill_base, &text_base, &border_base)
}

fn discard_non_explicit_quadrant_snapshot_values(
    tv: &mut Map<String, Value>,
    explicit: &Map<String, Value>,
) {
    for key in [
        "quadrant1Fill",
        "quadrant2Fill",
        "quadrant3Fill",
        "quadrant4Fill",
        "quadrant1TextFill",
        "quadrant2TextFill",
        "quadrant3TextFill",
        "quadrant4TextFill",
        "quadrantPointFill",
        "quadrantPointTextFill",
        "quadrantXAxisTextFill",
        "quadrantYAxisTextFill",
        "quadrantInternalBorderStrokeFill",
        "quadrantExternalBorderStrokeFill",
        "quadrantTitleFill",
    ] {
        if !explicit.contains_key(key) {
            tv.remove(key);
        }
    }
}

#[derive(Debug, Clone, Copy)]
enum ColorTransform {
    Darken(f64),
    Lighten(f64),
}

impl ColorTransform {
    fn apply(self, color: &str) -> Result<String, ColorError> {
        match self {
            Self::Darken(amount) => theme_color::darken(color, amount),
            Self::Lighten(amount) => theme_color::lighten(color, amount),
        }
    }
}

fn validate_explicit_git_transforms(
    tv: &Map<String, Value>,
    explicit: &Map<String, Value>,
    transform: ColorTransform,
) -> Result<(), ColorError> {
    for index in 0..8 {
        let key = format!("git{index}");
        if explicit.contains_key(&key) {
            transform.apply(&required_color(tv, &key)?)?;
        }
    }
    Ok(())
}

fn apply_single_pass_git_palette(
    tv: &mut Map<String, Value>,
    explicit: &Map<String, Value>,
    bases: [String; 8],
    transform: ColorTransform,
) -> Result<(), ColorError> {
    for (index, base) in bases.into_iter().enumerate() {
        let git_key = format!("git{index}");
        let source = if explicit.contains_key(&git_key) {
            required_color(tv, &git_key)?
        } else {
            base
        };
        let transformed = transform.apply(&source)?;
        if !explicit.contains_key(&git_key) {
            tv.insert(git_key, Value::String(transformed.clone()));
        }

        let inverse_key = format!("gitInv{index}");
        if !explicit.contains_key(&inverse_key) {
            tv.insert(
                inverse_key,
                Value::String(theme_color::invert(&transformed)?),
            );
        }
    }
    Ok(())
}

fn calculated_or_fallback_value(
    explicit: &Map<String, Value>,
    key: &str,
    fallback: Value,
) -> Value {
    explicit
        .get(key)
        .filter(|value| is_js_truthy(value))
        .cloned()
        .unwrap_or(fallback)
}

fn apply_scale_label_dependencies(
    explicit: &Map<String, Value>,
    tv: &mut Map<String, Value>,
    fallback: Value,
) {
    let scale_label = calculated_or_fallback_value(explicit, "scaleLabelColor", fallback);
    tv.insert("scaleLabelColor".to_string(), scale_label.clone());
    for index in 0..12 {
        let key = format!("cScaleLabel{index}");
        if !explicit.contains_key(&key) {
            tv.insert(key, scale_label.clone());
        }
    }
}

fn apply_default_theme_dependencies(
    explicit: &Map<String, Value>,
    tv: &mut Map<String, Value>,
) -> Result<(), ColorError> {
    let primary = required_color(tv, "primaryColor")?;
    set_derived_string_unless_explicit(
        tv,
        explicit,
        "rowOdd",
        theme_color::lighten(&primary, 75.0)?,
    );
    set_derived_string_unless_explicit(
        tv,
        explicit,
        "rowEven",
        theme_color::lighten(&primary, 1.0)?,
    );
    Ok(())
}

fn apply_dark_theme_dependencies(
    explicit: &Map<String, Value>,
    tv: &mut Map<String, Value>,
) -> Result<(), ColorError> {
    let fallback = if tv.get("darkMode").is_some_and(is_js_truthy) {
        Value::String("black".to_string())
    } else {
        tv.get("labelTextColor")
            .cloned()
            .unwrap_or_else(|| Value::String("lightgrey".to_string()))
    };
    apply_scale_label_dependencies(explicit, tv, fallback);
    Ok(())
}

fn git_palette_bases(tv: &Map<String, Value>) -> Result<[String; 8], ColorError> {
    let primary = required_color(tv, "primaryColor")?;
    Ok([
        primary.clone(),
        required_color(tv, "secondaryColor")?,
        required_color(tv, "tertiaryColor")?,
        theme_color::adjust(&primary, ColorAdjustment::hsl(-30.0, 0.0, 0.0))?,
        theme_color::adjust(&primary, ColorAdjustment::hsl(-60.0, 0.0, 0.0))?,
        theme_color::adjust(&primary, ColorAdjustment::hsl(-90.0, 0.0, 0.0))?,
        theme_color::adjust(&primary, ColorAdjustment::hsl(60.0, 0.0, 0.0))?,
        theme_color::adjust(&primary, ColorAdjustment::hsl(120.0, 0.0, 0.0))?,
    ])
}

fn apply_dynamic_git_dependencies(
    explicit: &Map<String, Value>,
    tv: &mut Map<String, Value>,
) -> Result<(), ColorError> {
    let transform = if tv.get("darkMode").is_some_and(is_js_truthy) {
        ColorTransform::Lighten(25.0)
    } else {
        ColorTransform::Darken(25.0)
    };
    apply_single_pass_git_palette(tv, explicit, git_palette_bases(tv)?, transform)
}

fn apply_forest_theme_dependencies(
    explicit: &Map<String, Value>,
    tv: &mut Map<String, Value>,
) -> Result<(), ColorError> {
    // Forest constructs `mainBkg` independently from `primaryColor`; overriding the latter must
    // not retarget ER striping. An explicit `mainBkg` still participates in the update pass.
    let main_bkg = if explicit.contains_key("mainBkg") {
        required_color(tv, "mainBkg")?
    } else {
        required_color(
            ThemeProgram::resolve("forest").default_snapshot(),
            "mainBkg",
        )?
    };
    set_derived_string_unless_explicit(
        tv,
        explicit,
        "rowOdd",
        theme_color::lighten(&main_bkg, 75.0)?,
    );
    set_derived_string_unless_explicit(
        tv,
        explicit,
        "rowEven",
        theme_color::lighten(&main_bkg, 20.0)?,
    );
    apply_dynamic_git_dependencies(explicit, tv)
}

fn apply_neutral_theme_dependencies(
    explicit: &Map<String, Value>,
    tv: &mut Map<String, Value>,
) -> Result<(), ColorError> {
    let dark_mode = tv.get("darkMode").is_some_and(is_js_truthy);
    for index in 0..12 {
        let scale = required_color(tv, &format!("cScale{index}"))?;
        let peer = if dark_mode {
            theme_color::lighten(&scale, 10.0)?
        } else {
            theme_color::darken(&scale, 10.0)?
        };
        set_derived_string_unless_explicit(tv, explicit, &format!("cScalePeer{index}"), peer);
        set_derived_string_unless_explicit(
            tv,
            explicit,
            &format!("cScaleInv{index}"),
            theme_color::invert(&scale)?,
        );
    }

    let fallback = if dark_mode {
        Value::String("black".to_string())
    } else {
        tv.get("labelTextColor")
            .cloned()
            .unwrap_or_else(|| Value::String("#333".to_string()))
    };
    let scale_label = calculated_or_fallback_value(explicit, "scaleLabelColor", fallback);
    tv.insert("scaleLabelColor".to_string(), scale_label.clone());
    let scale_one = tv
        .get("cScale1")
        .cloned()
        .unwrap_or_else(|| Value::String("#F4F4F4".to_string()));
    for index in 0..12 {
        let key = format!("cScaleLabel{index}");
        if !explicit.contains_key(&key) {
            tv.insert(
                key,
                if matches!(index, 0 | 2) {
                    scale_one.clone()
                } else {
                    scale_label.clone()
                },
            );
        }
    }
    Ok(())
}

fn darkened_scale_bases(tv: &Map<String, Value>) -> Result<[String; 12], ColorError> {
    let primary = required_color(tv, "primaryColor")?;
    Ok([
        primary.clone(),
        required_color(tv, "secondaryColor")?,
        required_color(tv, "tertiaryColor")?,
        theme_color::adjust(&primary, ColorAdjustment::hsl(30.0, 0.0, 0.0))?,
        theme_color::adjust(&primary, ColorAdjustment::hsl(60.0, 0.0, 0.0))?,
        theme_color::adjust(&primary, ColorAdjustment::hsl(90.0, 0.0, 0.0))?,
        theme_color::adjust(&primary, ColorAdjustment::hsl(120.0, 0.0, 0.0))?,
        theme_color::adjust(&primary, ColorAdjustment::hsl(150.0, 0.0, 0.0))?,
        theme_color::adjust(&primary, ColorAdjustment::hsl(210.0, 0.0, 150.0))?,
        theme_color::adjust(&primary, ColorAdjustment::hsl(270.0, 0.0, 0.0))?,
        theme_color::adjust(&primary, ColorAdjustment::hsl(300.0, 0.0, 0.0))?,
        theme_color::adjust(&primary, ColorAdjustment::hsl(330.0, 0.0, 0.0))?,
    ])
}

fn apply_darkened_scale_dependencies(
    explicit: &Map<String, Value>,
    tv: &mut Map<String, Value>,
) -> Result<(), ColorError> {
    let dark_mode = tv.get("darkMode").is_some_and(is_js_truthy);
    let transform = ColorTransform::Darken(if dark_mode { 75.0 } else { 25.0 });
    let bases = darkened_scale_bases(tv)?;

    for (index, base) in bases.into_iter().enumerate() {
        let scale_key = format!("cScale{index}");
        let source = get_truthy_string(explicit, &scale_key).unwrap_or(base);
        let scale = transform.apply(&source)?;
        if !explicit.contains_key(&scale_key) {
            tv.insert(scale_key, Value::String(scale.clone()));
        }
        set_derived_string_unless_explicit(
            tv,
            explicit,
            &format!("cScaleInv{index}"),
            theme_color::invert(&scale)?,
        );
        let peer = if dark_mode {
            theme_color::lighten(&scale, 10.0)?
        } else {
            theme_color::darken(&scale, 10.0)?
        };
        set_derived_string_unless_explicit(tv, explicit, &format!("cScalePeer{index}"), peer);
    }

    let label_text = tv
        .get("labelTextColor")
        .cloned()
        .unwrap_or_else(|| Value::String("#e0dfdf".to_string()));
    apply_scale_label_dependencies(explicit, tv, label_text);
    Ok(())
}

fn validate_explicit_operation(
    tv: &Map<String, Value>,
    explicit: &Map<String, Value>,
    key: &str,
    operation: impl FnOnce(&str) -> Result<String, ColorError>,
) -> Result<(), ColorError> {
    if explicit.contains_key(key) {
        operation(&required_color(tv, key)?)?;
    }
    Ok(())
}

fn replay_extended_theme_khroma_operations(
    theme: &str,
    explicit: &Map<String, Value>,
    tv: &mut Map<String, Value>,
) -> Result<(), ColorError> {
    let dark_mode = tv.get("darkMode").is_some_and(is_js_truthy);
    match theme {
        "neo" | "redux" | "redux-color" => {
            for key in ["secondaryColor", "tertiaryColor"] {
                validate_explicit_operation(tv, explicit, key, |color| {
                    mk_border(color, dark_mode)
                })?;
            }
        }
        "neo-dark" => {
            validate_explicit_operation(tv, explicit, "secondaryColor", |color| {
                theme_color::darken(color, 10.0)
            })?;
            validate_explicit_operation(tv, explicit, "tertiaryColor", |color| {
                theme_color::darken(color, if dark_mode { 75.0 } else { 25.0 })
            })?;
        }
        _ => {}
    }

    let scale_policy = match theme {
        "neo" | "neo-dark" | "redux-dark" => Some(Some(ColorTransform::Darken(if dark_mode {
            75.0
        } else {
            25.0
        }))),
        "redux-color" | "redux-dark-color" => Some(None),
        _ => None,
    };
    if let Some(transform) = scale_policy {
        for index in 0..2 {
            let scale_key = format!("cScale{index}");
            if !explicit.contains_key(&scale_key) {
                continue;
            }
            let source = required_color(tv, &scale_key)?;
            let calculated = match transform {
                Some(transform) => transform.apply(&source)?,
                None => source,
            };

            let inverse_key = format!("cScaleInv{index}");
            if !explicit.contains_key(&inverse_key) {
                tv.insert(
                    inverse_key,
                    Value::String(theme_color::invert(&calculated)?),
                );
            }
            let peer_key = format!("cScalePeer{index}");
            if !explicit.contains_key(&peer_key) {
                let peer = if dark_mode {
                    theme_color::lighten(&calculated, 10.0)?
                } else {
                    theme_color::darken(&calculated, 10.0)?
                };
                tv.insert(peer_key, Value::String(peer));
            }
            if theme == "redux-dark-color" {
                let label_key = format!("cScaleLabel{index}");
                if !explicit.contains_key(&label_key) {
                    tv.insert(
                        label_key,
                        Value::String(theme_color::darken(&calculated, 75.0)?),
                    );
                }
            }
        }
    }
    Ok(())
}

pub(crate) fn apply_theme_defaults(config: &mut MermaidConfig) -> Result<(), ColorError> {
    let requested = config.get_str("theme").unwrap_or("default").to_string();
    let program = *ThemeProgram::resolve(&requested);
    let raw = theme_variables_map(config);
    // The complete base evaluator validates only operands actually reached by updateColors().
    // Prevalidating a list would reject valid falsy fallbacks and miss later dependencies.
    if program.kind != ThemeProgramKind::Base {
        program.validate_evaluated_inputs(&raw)?;
    }
    let explicit = program.normalize_overrides(raw);
    config.set_value("themeVariables", Value::Object(explicit));
    program.execute(config)
}

fn apply_snapshot_theme_defaults(
    config: &mut MermaidConfig,
    theme: &str,
) -> Result<(), ColorError> {
    let tv = theme_variables_map(config);
    finish_theme_defaults(config, theme, tv)
}

fn apply_extended_theme_visible_derivations(
    theme: &str,
    explicit: &Map<String, Value>,
    tv: &mut Map<String, Value>,
) -> Result<(), ColorError> {
    if !matches!(
        theme,
        "neo" | "neo-dark" | "redux" | "redux-dark" | "redux-color" | "redux-dark-color"
    ) {
        return Ok(());
    }

    replay_extended_theme_khroma_operations(theme, explicit, tv)?;

    // Mermaid's extended themes run `calculate(overrides)`: copy user base variables, update
    // derived colors, then re-apply explicit user keys. Keep generated snapshots as the default
    // source of truth, but recompute visible derived keys that current renderers consume.
    if explicit.contains_key("primaryColor") {
        let primary = required_color(tv, "primaryColor")?;
        set_derived_string_unless_explicit(tv, explicit, "nodeBkg", primary.clone());
        set_derived_string_unless_explicit(tv, explicit, "tagLabelBackground", primary.clone());

        if matches!(theme, "neo" | "redux" | "redux-color")
            && !explicit.contains_key("secondaryColor")
        {
            let secondary = theme_color::adjust(&primary, ColorAdjustment::hsl(-120.0, 0.0, 0.0))?;
            tv.insert("secondaryColor".to_string(), Value::String(secondary));
        }
    }

    // updateColors() resolves a falsy border before Agentflow reads it. The
    // explicit value is replayed only after all dependent colors are calculated.
    if value_is_missing(tv, "secondaryBorderColor") {
        let secondary = required_color(tv, "secondaryColor")?;
        let dark_mode = tv.get("darkMode").is_some_and(is_js_truthy);
        tv.insert(
            "secondaryBorderColor".to_string(),
            Value::String(mk_border(&secondary, dark_mode)?),
        );
    }

    if explicit.contains_key("background") {
        let background = required_color(tv, "background")?;
        if !explicit.contains_key("lineColor") {
            let line_color = theme_color::invert(&background)?;
            tv.insert("lineColor".to_string(), Value::String(line_color));
        }
        if !explicit.contains_key("arrowheadColor") {
            let arrowhead_color = theme_color::invert(&background)?;
            tv.insert("arrowheadColor".to_string(), Value::String(arrowhead_color));
        }
    }

    if explicit.contains_key("lineColor") || explicit.contains_key("background") {
        let line_color = required_color(tv, "lineColor")?;
        for key in [
            "defaultLinkColor",
            "archEdgeColor",
            "archEdgeArrowColor",
            "relationColor",
            "transitionColor",
            "specialStateColor",
        ] {
            set_derived_string_unless_explicit(tv, explicit, key, line_color.clone());
        }
    }

    if explicit.contains_key("secondaryColor") || explicit.contains_key("primaryColor") {
        let secondary = required_color(tv, "secondaryColor")?;
        let dark_mode = tv.get("darkMode").is_some_and(is_js_truthy);
        let label_background = if dark_mode {
            theme_color::darken(&secondary, 30.0)?
        } else {
            secondary.clone()
        };
        for key in [
            "edgeLabelBackground",
            "activationBkgColor",
            "commitLabelBackground",
            "relationLabelBackground",
        ] {
            set_derived_string_unless_explicit(tv, explicit, key, label_background.clone());
        }
    }

    if explicit.contains_key("mainBkg") {
        let main_bkg = required_color(tv, "mainBkg")?;
        for key in [
            "actorBkg",
            "labelBoxBkgColor",
            "personBkg",
            "stateBkg",
            "labelBackgroundColor",
        ] {
            set_derived_string_unless_explicit(tv, explicit, key, main_bkg.clone());
        }
    }

    if explicit.contains_key("primaryColor")
        && matches!(theme, "neo-dark" | "redux-dark" | "redux-dark-color")
    {
        let primary = required_color(tv, "primaryColor")?;
        set_derived_string_unless_explicit(tv, explicit, "requirementBackground", primary.clone());
        if theme != "redux-dark-color" {
            set_derived_string_unless_explicit(tv, explicit, "pie1", primary);
        }
    }

    // Mermaid 12 uses a visible, direction-aware band on the light extended
    // themes, and resolves dark themes' constructor placeholder from mainBkg.
    if matches!(theme, "neo" | "redux" | "redux-color")
        && explicit.contains_key("background")
        && !explicit.contains_key("rectBkgColor")
    {
        let background = required_color(tv, "background")?;
        let band = if theme_color::is_dark(&background)? {
            theme_color::lighten(&background, 4.0)?
        } else {
            theme_color::darken(&background, 4.0)?
        };
        tv.insert("rectBkgColor".to_string(), Value::String(band));
    }
    if matches!(theme, "neo-dark" | "redux-dark" | "redux-dark-color") {
        let second_is_placeholder =
            explicit.get("secondBkg").and_then(Value::as_str) == Some("calculated");
        if (explicit.contains_key("mainBkg") && !explicit.contains_key("secondBkg"))
            || second_is_placeholder
        {
            let second = theme_color::lighten(&required_color(tv, "mainBkg")?, 16.0)?;
            tv.insert("secondBkg".to_string(), Value::String(second));
        }
        if (explicit.contains_key("mainBkg") || explicit.contains_key("secondBkg"))
            && !explicit.contains_key("doneTaskBkgColor")
            && let Some(second) = tv.get("secondBkg").cloned()
        {
            tv.insert("doneTaskBkgColor".to_string(), second);
        }
    }
    if matches!(theme, "redux-color" | "redux-dark-color") {
        // The categorical scale now drives pie slices and Gantt section bands.
        for index in 0..12 {
            let scale = format!("cScale{index}");
            if explicit.contains_key(&scale) {
                // Both Redux color themes use fixed categorical fallbacks in
                // updateColors(). Consumers read that stage, not the final replay.
                let fallback =
                    ThemeProgram::resolve(theme).calculation_snapshot(explicit)[&scale].clone();
                set_if_missing(tv, &scale, fallback);
                let color = tv[&scale].clone();
                let pie = format!("pie{}", index + 1);
                if !explicit.contains_key(&pie) {
                    tv.insert(pie, color.clone());
                }
                let section = match index {
                    0 => Some("sectionBkgColor"),
                    1 => Some("sectionBkgColor2"),
                    _ => None,
                };
                if let Some(section) = section.filter(|key| !explicit.contains_key(*key)) {
                    tv.insert(section.to_string(), color.clone());
                }
            }
        }
        if theme == "redux-dark-color"
            && explicit.contains_key("background")
            && !explicit.contains_key("altSectionBkgColor")
            && let Some(background) = tv.get("background").cloned()
        {
            tv.insert("altSectionBkgColor".to_string(), background);
        }
        for (palette, prefix) in [("borderColorArray", "venn"), ("bkgColorArray", "fillType")] {
            if prefix == "fillType" && theme == "redux-dark-color" {
                continue;
            }
            if let Some(colors) = explicit.get(palette).and_then(Value::as_array) {
                for index in 0..8 {
                    let key = format!("{prefix}{}", index + usize::from(prefix == "venn"));
                    if explicit.contains_key(&key) {
                        continue;
                    }
                    let color = if prefix == "venn" && !colors.is_empty() {
                        colors.get(index % colors.len())
                    } else {
                        colors.get(index)
                    };
                    if let Some(color) = color {
                        tv.insert(key, color.clone());
                    } else {
                        tv.remove(&key);
                    }
                }
            }
        }
    }

    for i in 0..8 {
        let git_key = format!("git{i}");
        let git_inv_key = format!("gitInv{i}");
        if explicit.contains_key(&git_key) && !explicit.contains_key(&git_inv_key) {
            let git = required_color(tv, &git_key)?;
            let inv = theme_color::invert(&git)?;
            tv.insert(git_inv_key, Value::String(inv));
        }
    }

    discard_non_explicit_quadrant_snapshot_values(tv, explicit);
    let quadrant_fill_base = if matches!(theme, "neo" | "redux" | "redux-color") {
        "#ECECFE".to_string()
    } else {
        required_color(tv, "primaryColor")?
    };
    let quadrant_text_base = required_color(tv, "primaryTextColor")?;
    let quadrant_border_base = required_color(tv, "primaryBorderColor")?;
    apply_quadrant_theme_defaults(
        tv,
        &quadrant_fill_base,
        &quadrant_text_base,
        &quadrant_border_base,
    )?;
    Ok(())
}

fn apply_default_theme_defaults(config: &mut MermaidConfig) -> Result<(), ColorError> {
    let mut tv = theme_variables_map(config);
    let explicit_theme_variables = tv.clone();

    // Mermaid 11.16.1: `theme-default` constructor defaults and `updateColors()`.
    // Source: `repo-ref/mermaid/packages/mermaid/src/themes/theme-default.js`.
    let default_primary = "#ECECFF";
    let default_secondary = "#ffffde";
    let default_tertiary =
        theme_color::adjust(default_primary, ColorAdjustment::hsl(-160.0, 0.0, 0.0))?;
    let default_primary_border = mk_border(default_primary, false)?;

    set_if_missing(&mut tv, "background", Value::String("white".to_string()));
    set_if_missing(
        &mut tv,
        "primaryColor",
        Value::String(default_primary.to_string()),
    );
    set_if_missing(
        &mut tv,
        "secondaryColor",
        Value::String(default_secondary.to_string()),
    );
    set_if_missing(
        &mut tv,
        "tertiaryColor",
        Value::String(default_tertiary.clone()),
    );

    set_if_missing(
        &mut tv,
        "primaryBorderColor",
        Value::String(default_primary_border.clone()),
    );
    set_if_missing(
        &mut tv,
        "secondaryBorderColor",
        Value::String(mk_border(default_secondary, false)?),
    );
    set_if_missing(
        &mut tv,
        "tertiaryBorderColor",
        Value::String(mk_border(&default_tertiary, false)?),
    );

    set_if_missing(
        &mut tv,
        "primaryTextColor",
        Value::String("#131300".to_string()),
    );
    set_if_missing(
        &mut tv,
        "secondaryTextColor",
        Value::String("#000021".to_string()),
    );
    set_if_missing(
        &mut tv,
        "tertiaryTextColor",
        Value::String(theme_color::invert(&default_tertiary)?),
    );

    set_if_missing(
        &mut tv,
        "mainBkg",
        Value::String(default_primary.to_string()),
    );
    set_if_missing(
        &mut tv,
        "secondBkg",
        Value::String(default_secondary.to_string()),
    );
    set_if_missing(&mut tv, "lineColor", Value::String("#333333".to_string()));
    set_if_missing(&mut tv, "border1", Value::String("#9370DB".to_string()));
    set_if_missing(&mut tv, "border2", Value::String("#aaaa33".to_string()));
    set_if_missing(
        &mut tv,
        "arrowheadColor",
        Value::String("#333333".to_string()),
    );
    set_if_missing(&mut tv, "fontFamily", mermaid_default_font_family());
    set_if_missing(&mut tv, "fontSize", Value::String("16px".to_string()));
    set_if_missing(
        &mut tv,
        "labelBackground",
        Value::String("rgba(232,232,232, 0.8)".to_string()),
    );
    set_if_missing(&mut tv, "textColor", Value::String("#333".to_string()));
    set_if_missing(&mut tv, "THEME_COLOR_LIMIT", Value::Number(12.into()));
    set_if_missing(&mut tv, "radius", Value::Number(5.into()));
    set_if_missing(&mut tv, "strokeWidth", Value::Number(1.into()));

    let main_bkg = get_truthy_string(&tv, "mainBkg").unwrap_or_else(|| default_primary.to_string());
    let second_bkg =
        get_truthy_string(&tv, "secondBkg").unwrap_or_else(|| default_secondary.to_string());
    let line_color = get_truthy_string(&tv, "lineColor").unwrap_or_else(|| "#333333".to_string());
    let text_color = get_truthy_string(&tv, "textColor").unwrap_or_else(|| "#333".to_string());
    let border1 = get_truthy_string(&tv, "border1").unwrap_or_else(|| "#9370DB".to_string());
    let border2 = get_truthy_string(&tv, "border2").unwrap_or_else(|| "#aaaa33".to_string());
    let label_background = get_truthy_string(&tv, "labelBackground")
        .unwrap_or_else(|| "rgba(232,232,232, 0.8)".to_string());
    let primary_text_color =
        get_truthy_string(&tv, "primaryTextColor").unwrap_or_else(|| "#131300".to_string());

    // Flowchart and block/class surfaces.
    set_if_missing(&mut tv, "nodeBkg", Value::String(main_bkg.clone()));
    set_if_missing(&mut tv, "nodeBorder", Value::String(border1.clone()));
    set_if_missing(&mut tv, "clusterBkg", Value::String(second_bkg.clone()));
    set_if_missing(&mut tv, "clusterBorder", Value::String(border2.clone()));
    set_if_missing(
        &mut tv,
        "defaultLinkColor",
        Value::String(line_color.clone()),
    );
    set_if_missing(&mut tv, "titleColor", Value::String(text_color.clone()));
    set_if_missing(
        &mut tv,
        "edgeLabelBackground",
        Value::String(label_background.clone()),
    );
    set_if_missing(
        &mut tv,
        "nodeTextColor",
        Value::String(primary_text_color.clone()),
    );

    // Sequence diagram surfaces.
    set_if_missing(&mut tv, "actorBorder", Value::String(border1.clone()));
    set_if_missing(&mut tv, "actorBkg", Value::String(main_bkg.clone()));
    set_if_missing(
        &mut tv,
        "actorTextColor",
        Value::String("black".to_string()),
    );
    let actor_text_color =
        get_truthy_string(&tv, "actorTextColor").unwrap_or_else(|| "black".to_string());
    set_if_missing(&mut tv, "actorLineColor", Value::String(border1.clone()));
    set_if_missing(&mut tv, "labelBoxBkgColor", Value::String(main_bkg.clone()));
    set_if_missing(&mut tv, "signalColor", Value::String(text_color.clone()));
    set_if_missing(
        &mut tv,
        "signalTextColor",
        Value::String(text_color.clone()),
    );
    set_if_missing(
        &mut tv,
        "labelBoxBorderColor",
        Value::String(border1.clone()),
    );
    set_if_missing(
        &mut tv,
        "labelTextColor",
        Value::String(actor_text_color.clone()),
    );
    set_if_missing(
        &mut tv,
        "loopTextColor",
        Value::String(actor_text_color.clone()),
    );
    set_if_missing(&mut tv, "noteBorderColor", Value::String(border2.clone()));
    set_if_missing(
        &mut tv,
        "noteBkgColor",
        Value::String("#fff5ad".to_string()),
    );
    set_if_missing(
        &mut tv,
        "noteTextColor",
        Value::String(actor_text_color.clone()),
    );
    set_if_missing(
        &mut tv,
        "activationBorderColor",
        Value::String("#666".to_string()),
    );
    set_if_missing(
        &mut tv,
        "activationBkgColor",
        Value::String("#f4f4f4".to_string()),
    );
    set_if_missing(
        &mut tv,
        "sequenceNumberColor",
        Value::String("white".to_string()),
    );
    set_if_missing(
        &mut tv,
        "rectBkgColor",
        Value::String(default_tertiary.clone()),
    );

    // Gantt chart surfaces.
    for (key, value) in [
        ("sectionBkgColor", "rgba(102, 102, 255, 0.49)"),
        ("altSectionBkgColor", "white"),
        ("sectionBkgColor2", "#fff400"),
        ("excludeBkgColor", "#eeeeee"),
        ("taskBorderColor", "#534fbc"),
        ("taskBkgColor", "#8a90dd"),
        ("taskTextLightColor", "white"),
        ("taskTextColor", "white"),
        ("taskTextDarkColor", "black"),
        ("taskTextOutsideColor", "black"),
        ("taskTextClickableColor", "#003163"),
        ("activeTaskBorderColor", "#534fbc"),
        ("activeTaskBkgColor", "#bfc7ff"),
        ("gridColor", "lightgrey"),
        ("doneTaskBkgColor", "lightgrey"),
        ("doneTaskBorderColor", "grey"),
        ("critBorderColor", "#ff8888"),
        ("critBkgColor", "red"),
        ("todayLineColor", "red"),
        ("vertLineColor", "navy"),
        ("noteFontWeight", "normal"),
        ("fontWeight", "normal"),
    ] {
        set_if_missing(&mut tv, key, Value::String(value.to_string()));
    }

    // C4 and architecture defaults.
    let primary_border_color = match get_truthy_string(&tv, "primaryBorderColor") {
        Some(color) => color,
        None => mk_border(default_primary, false)?,
    };
    let secondary_border_color = match get_truthy_string(&tv, "secondaryBorderColor") {
        Some(color) => color,
        None => mk_border(default_secondary, false)?,
    };
    set_if_missing(
        &mut tv,
        "personBorder",
        Value::String(primary_border_color.clone()),
    );
    set_if_missing(&mut tv, "personBkg", Value::String(main_bkg.clone()));
    set_if_missing(&mut tv, "archEdgeColor", Value::String(line_color.clone()));
    set_if_missing(
        &mut tv,
        "archEdgeArrowColor",
        Value::String(line_color.clone()),
    );
    set_if_missing(&mut tv, "archEdgeWidth", Value::String("3".to_string()));
    set_if_missing(
        &mut tv,
        "archGroupBorderColor",
        Value::String(primary_border_color.clone()),
    );
    set_if_missing(
        &mut tv,
        "archGroupBorderWidth",
        Value::String("2px".to_string()),
    );

    // ER, state, class, and requirement surfaces.
    set_if_missing(
        &mut tv,
        "rowOdd",
        Value::String(theme_color::lighten(default_primary, 75.0)?),
    );
    set_if_missing(
        &mut tv,
        "rowEven",
        Value::String(theme_color::lighten(default_primary, 1.0)?),
    );
    set_if_missing(
        &mut tv,
        "attributeBackgroundColorOdd",
        Value::String("#ffffff".to_string()),
    );
    set_if_missing(
        &mut tv,
        "attributeBackgroundColorEven",
        Value::String("#f2f2f2".to_string()),
    );
    set_if_missing(&mut tv, "labelColor", Value::String("black".to_string()));
    set_if_missing(
        &mut tv,
        "errorBkgColor",
        Value::String("#552222".to_string()),
    );
    set_if_missing(
        &mut tv,
        "errorTextColor",
        Value::String("#552222".to_string()),
    );
    set_if_missing(
        &mut tv,
        "transitionColor",
        Value::String(line_color.clone()),
    );
    set_if_missing(
        &mut tv,
        "transitionLabelColor",
        Value::String(text_color.clone()),
    );
    let state_label_color = get_truthy_string(&tv, "stateBkg")
        .map(Value::String)
        .unwrap_or_else(|| Value::String(primary_text_color.clone()));
    set_if_missing(&mut tv, "stateLabelColor", state_label_color);
    set_if_missing(&mut tv, "stateBkg", Value::String(main_bkg.clone()));
    let state_bkg = get_truthy_string(&tv, "stateBkg").unwrap_or_else(|| main_bkg.clone());
    set_if_missing(&mut tv, "labelBackgroundColor", Value::String(state_bkg));
    let composite_background = get_truthy_string(&tv, "background")
        .map(Value::String)
        .unwrap_or_else(|| Value::String("white".to_string()));
    set_if_missing(&mut tv, "compositeBackground", composite_background);
    set_if_missing(
        &mut tv,
        "altBackground",
        Value::String("#f0f0f0".to_string()),
    );
    set_if_missing(
        &mut tv,
        "compositeTitleBackground",
        Value::String(main_bkg.clone()),
    );
    let node_border = get_truthy_string(&tv, "nodeBorder").unwrap_or_else(|| border1.clone());
    set_if_missing(
        &mut tv,
        "compositeBorder",
        Value::String(node_border.clone()),
    );
    set_if_missing(&mut tv, "innerEndBackground", Value::String(node_border));
    set_if_missing(
        &mut tv,
        "specialStateColor",
        Value::String(line_color.clone()),
    );
    set_if_missing(
        &mut tv,
        "classText",
        Value::String(primary_text_color.clone()),
    );

    // Color scale.
    let primary_color =
        get_truthy_string(&tv, "primaryColor").unwrap_or_else(|| default_primary.to_string());
    let secondary_color =
        get_truthy_string(&tv, "secondaryColor").unwrap_or_else(|| default_secondary.to_string());
    // The constructor's first update already materializes every tertiary-derived fallback. The
    // calculate pass therefore replays an explicit tertiary token without evaluating it.
    let tertiary_color = default_tertiary.clone();
    let c_scale_bases = [
        primary_color.clone(),
        secondary_color.clone(),
        tertiary_color.clone(),
        theme_color::adjust(&primary_color, ColorAdjustment::hsl(30.0, 0.0, 0.0))?,
        theme_color::adjust(&primary_color, ColorAdjustment::hsl(60.0, 0.0, 0.0))?,
        theme_color::adjust(&primary_color, ColorAdjustment::hsl(90.0, 0.0, 0.0))?,
        theme_color::adjust(&primary_color, ColorAdjustment::hsl(120.0, 0.0, 0.0))?,
        theme_color::adjust(&primary_color, ColorAdjustment::hsl(150.0, 0.0, 0.0))?,
        theme_color::adjust(&primary_color, ColorAdjustment::hsl(210.0, 0.0, 0.0))?,
        theme_color::adjust(&primary_color, ColorAdjustment::hsl(270.0, 0.0, 0.0))?,
        theme_color::adjust(&primary_color, ColorAdjustment::hsl(300.0, 0.0, 0.0))?,
        theme_color::adjust(&primary_color, ColorAdjustment::hsl(330.0, 0.0, 0.0))?,
    ];
    let mut c_scales = Vec::with_capacity(c_scale_bases.len());
    for base in c_scale_bases {
        c_scales.push(theme_color::darken(&base, 10.0)?);
    }

    for (i, v) in c_scales.iter().enumerate() {
        set_if_missing(&mut tv, &format!("cScale{i}"), Value::String(v.clone()));
    }
    set_if_missing(
        &mut tv,
        "cScalePeer1",
        Value::String(theme_color::darken(&secondary_color, 45.0)?),
    );
    set_if_missing(
        &mut tv,
        "cScalePeer2",
        Value::String(theme_color::darken(&tertiary_color, 40.0)?),
    );
    for (i, fallback_color) in c_scales.iter().enumerate() {
        let color =
            get_truthy_string(&tv, &format!("cScale{i}")).unwrap_or_else(|| fallback_color.clone());
        set_if_missing(
            &mut tv,
            &format!("cScalePeer{i}"),
            Value::String(theme_color::darken(&color, 25.0)?),
        );
        set_if_missing(
            &mut tv,
            &format!("cScaleInv{i}"),
            Value::String(theme_color::adjust(
                &color,
                ColorAdjustment::hsl(180.0, 0.0, 0.0),
            )?),
        );
        if i == 0 || i == 3 {
            set_if_missing(
                &mut tv,
                &format!("cScaleLabel{i}"),
                Value::String("#ffffff".to_string()),
            );
        }
        set_if_missing(
            &mut tv,
            &format!("cScaleLabel{i}"),
            Value::String(actor_text_color.clone()),
        );
    }
    set_if_missing(
        &mut tv,
        "scaleLabelColor",
        Value::String(actor_text_color.clone()),
    );

    // Journey and pie color defaults.
    for (key, value) in [
        ("fillType0", primary_color.clone()),
        ("fillType1", secondary_color.clone()),
        (
            "fillType2",
            theme_color::adjust(&primary_color, ColorAdjustment::hsl(64.0, 0.0, 0.0))?,
        ),
        (
            "fillType3",
            theme_color::adjust(&secondary_color, ColorAdjustment::hsl(64.0, 0.0, 0.0))?,
        ),
        (
            "fillType4",
            theme_color::adjust(&primary_color, ColorAdjustment::hsl(-64.0, 0.0, 0.0))?,
        ),
        (
            "fillType5",
            theme_color::adjust(&secondary_color, ColorAdjustment::hsl(-64.0, 0.0, 0.0))?,
        ),
        (
            "fillType6",
            theme_color::adjust(&primary_color, ColorAdjustment::hsl(128.0, 0.0, 0.0))?,
        ),
        (
            "fillType7",
            theme_color::adjust(&secondary_color, ColorAdjustment::hsl(128.0, 0.0, 0.0))?,
        ),
        ("pie1", primary_color.clone()),
        ("pie2", secondary_color.clone()),
        ("pie3", theme_color::darken(&tertiary_color, 40.0)?),
        ("pie4", theme_color::darken(&primary_color, 10.0)?),
        ("pie5", theme_color::darken(&secondary_color, 30.0)?),
        ("pie6", theme_color::darken(&tertiary_color, 20.0)?),
        (
            "pie7",
            theme_color::adjust(&primary_color, ColorAdjustment::hsl(60.0, 0.0, -20.0))?,
        ),
        (
            "pie8",
            theme_color::adjust(&primary_color, ColorAdjustment::hsl(-60.0, 0.0, -40.0))?,
        ),
        (
            "pie9",
            theme_color::adjust(&primary_color, ColorAdjustment::hsl(120.0, 0.0, -40.0))?,
        ),
        (
            "pie10",
            theme_color::adjust(&primary_color, ColorAdjustment::hsl(60.0, 0.0, -40.0))?,
        ),
        (
            "pie11",
            theme_color::adjust(&primary_color, ColorAdjustment::hsl(-90.0, 0.0, -40.0))?,
        ),
        (
            "pie12",
            theme_color::adjust(&primary_color, ColorAdjustment::hsl(120.0, 0.0, -30.0))?,
        ),
    ] {
        set_if_missing(&mut tv, key, Value::String(value));
    }
    for (key, value) in [
        ("pieTitleTextSize", "25px"),
        ("pieTitleTextColor", "black"),
        ("pieSectionTextSize", "17px"),
        ("pieSectionTextColor", text_color.as_str()),
        ("pieLegendTextSize", "17px"),
        ("pieLegendTextColor", "black"),
        ("pieStrokeColor", "black"),
        ("pieStrokeWidth", "2px"),
        ("pieOuterStrokeWidth", "2px"),
        ("pieOuterStrokeColor", "black"),
        ("pieOpacity", "0.7"),
    ] {
        set_if_missing(&mut tv, key, Value::String(value.to_string()));
    }

    // Requirement and git surfaces consumed by current renderers.
    set_if_missing(
        &mut tv,
        "requirementBackground",
        Value::String(default_primary.to_string()),
    );
    set_if_missing(
        &mut tv,
        "requirementBorderColor",
        Value::String(primary_border_color.clone()),
    );
    set_if_missing(
        &mut tv,
        "requirementBorderSize",
        Value::String("1".to_string()),
    );
    set_if_missing(
        &mut tv,
        "requirementTextColor",
        Value::String(primary_text_color.clone()),
    );
    set_if_missing(&mut tv, "relationColor", Value::String(line_color.clone()));
    set_if_missing(
        &mut tv,
        "relationLabelBackground",
        Value::String(label_background),
    );
    set_if_missing(
        &mut tv,
        "relationLabelColor",
        Value::String(actor_text_color.clone()),
    );

    set_if_missing(&mut tv, "tagLabelColor", Value::String(primary_text_color));
    set_if_missing(
        &mut tv,
        "tagLabelBackground",
        Value::String(default_primary.to_string()),
    );
    set_if_missing(
        &mut tv,
        "tagLabelBorder",
        Value::String(primary_border_color.clone()),
    );
    set_if_missing(
        &mut tv,
        "tagLabelFontSize",
        Value::String("10px".to_string()),
    );
    set_if_missing(
        &mut tv,
        "commitLabelColor",
        Value::String("#000021".to_string()),
    );
    set_if_missing(
        &mut tv,
        "commitLabelBackground",
        Value::String(default_secondary.to_string()),
    );
    set_if_missing(
        &mut tv,
        "commitLabelFontSize",
        Value::String("10px".to_string()),
    );

    set_if_missing(&mut tv, "useGradient", Value::Bool(false));
    set_if_missing(
        &mut tv,
        "gradientStart",
        Value::String(primary_border_color),
    );
    set_if_missing(
        &mut tv,
        "gradientStop",
        Value::String(secondary_border_color),
    );
    set_if_missing(
        &mut tv,
        "dropShadow",
        Value::String("drop-shadow(1px 2px 2px rgba(185, 185, 185, 1))".to_string()),
    );

    ensure_xychart_theme_defaults(
        &mut tv,
        "#ECECFF,#8493A6,#FFC3A0,#DCDDE1,#B8E994,#D1A36F,#C3CDE6,#FFB6C1,#496078,#F8F3E3",
    );

    apply_quadrant_theme_defaults(&mut tv, default_primary, "#131300", &default_primary_border)?;
    validate_explicit_git_transforms(&tv, &explicit_theme_variables, ColorTransform::Darken(25.0))?;

    finish_theme_defaults(config, "default", tv)
}

fn apply_dark_theme_defaults(config: &mut MermaidConfig) -> Result<(), ColorError> {
    let mut tv = theme_variables_map(config);

    // Mermaid 11.16.1: `theme-dark` color scale seeds.
    // Source: `repo-ref/mermaid/packages/mermaid/src/themes/theme-dark.js`.
    //
    // Note: `theme-dark` keeps `cScale*` as the provided hex strings, while derived
    // `cScalePeer*` values are produced via `khroma.lighten(...)` (serialized as `hsl(...)`).
    let c_scales_hex: [&str; 12] = [
        "#1f2020", // primaryColor
        "#0b0000", "#4d1037", "#3f5258", "#4f2f1b", "#6e0a0a", "#3b0048", "#995a01", "#154706",
        "#161722", "#00296f", "#01629c",
    ];

    // Mermaid's dark theme is not just a palette switch: most readable text colors are derived
    // from dark surface colors in `updateColors()`. Seed those diagram-facing variables here so
    // headless renderers do not fall back to default-theme black text on dark backgrounds.
    set_string_if_missing(&mut tv, "background", "#333");
    set_string_if_missing(&mut tv, "primaryColor", "#1f2020");
    if get_truthy_string(&tv, "primaryTextColor").is_none()
        && let Some(primary_color) = get_truthy_string(&tv, "primaryColor")
    {
        tv.insert(
            "primaryTextColor".to_string(),
            Value::String(theme_color::invert(&primary_color)?),
        );
    }
    set_string_if_missing(&mut tv, "textColor", "#ccc");
    set_if_missing(&mut tv, "fontFamily", mermaid_default_font_family());
    set_string_if_missing(&mut tv, "fontSize", "16px");
    set_string_if_missing(&mut tv, "border1", "#ccc");
    set_string_if_missing(&mut tv, "border2", "rgba(255, 255, 255, 0.25)");
    set_string_if_missing(&mut tv, "labelBackground", "#181818");
    set_string_if_missing(&mut tv, "titleColor", "#F9FFFE");
    set_if_missing(&mut tv, "THEME_COLOR_LIMIT", Value::Number(12.into()));
    set_if_missing(&mut tv, "radius", Value::Number(5.into()));
    set_if_missing(&mut tv, "strokeWidth", Value::Number(1.into()));
    set_string_if_missing(&mut tv, "errorBkgColor", "#a44141");
    set_string_if_missing(&mut tv, "errorTextColor", "#ddd");

    let primary_color =
        get_truthy_string(&tv, "primaryColor").unwrap_or_else(|| "#1f2020".to_string());
    let default_secondary_color = theme_color::lighten(&primary_color, 16.0)?;
    set_if_missing(
        &mut tv,
        "secondaryColor",
        Value::String(default_secondary_color.clone()),
    );
    set_if_missing(
        &mut tv,
        "primaryBorderColor",
        Value::String("#cccccc".to_string()),
    );
    set_if_missing(
        &mut tv,
        "secondaryBorderColor",
        Value::String(mk_border(&default_secondary_color, false)?),
    );
    let default_tertiary_color =
        theme_color::adjust(&primary_color, ColorAdjustment::hsl(-160.0, 0.0, 0.0))?;
    set_string_if_missing(&mut tv, "tertiaryColor", default_tertiary_color.clone());
    set_string_if_missing(
        &mut tv,
        "tertiaryBorderColor",
        mk_border(&default_tertiary_color, false)?,
    );
    set_string_if_missing(
        &mut tv,
        "secondaryTextColor",
        theme_color::invert(&default_secondary_color)?,
    );
    set_string_if_missing(
        &mut tv,
        "tertiaryTextColor",
        theme_color::invert(&default_tertiary_color)?,
    );
    ensure_gradient_theme_defaults(&mut tv);

    let secondary_color =
        get_truthy_string(&tv, "secondaryColor").unwrap_or_else(|| default_secondary_color.clone());
    let secondary_text_color = match get_truthy_string(&tv, "secondaryTextColor") {
        Some(color) => color,
        None => theme_color::invert(&secondary_color)?,
    };
    let tertiary_color =
        get_truthy_string(&tv, "tertiaryColor").unwrap_or_else(|| default_tertiary_color.clone());
    let background = get_truthy_string(&tv, "background").unwrap_or_else(|| "#333".to_string());
    let primary_text_color =
        get_truthy_string(&tv, "primaryTextColor").unwrap_or_else(|| "#e0dfdf".to_string());
    let text_color = get_truthy_string(&tv, "textColor").unwrap_or_else(|| "#ccc".to_string());
    let line_color = get_truthy_string(&tv, "lineColor").unwrap_or_else(|| "lightgrey".to_string());
    let border1 = get_truthy_string(&tv, "border1").unwrap_or_else(|| "#ccc".to_string());
    let border2 = get_truthy_string(&tv, "border2")
        .unwrap_or_else(|| "rgba(255, 255, 255, 0.25)".to_string());
    let primary_border_color =
        get_truthy_string(&tv, "primaryBorderColor").unwrap_or_else(|| "#cccccc".to_string());
    let secondary_border_color = match get_truthy_string(&tv, "secondaryBorderColor") {
        Some(color) => color,
        None => mk_border(&secondary_color, false)?,
    };

    // theme-dark updates Journey colors unconditionally before replaying explicit values.
    for (key, color) in [
        ("fillType0", primary_color.clone()),
        ("fillType1", secondary_color.clone()),
        (
            "fillType2",
            theme_color::adjust(&primary_color, ColorAdjustment::hsl(64.0, 0.0, 0.0))?,
        ),
        (
            "fillType3",
            theme_color::adjust(&secondary_color, ColorAdjustment::hsl(64.0, 0.0, 0.0))?,
        ),
        (
            "fillType4",
            theme_color::adjust(&primary_color, ColorAdjustment::hsl(-64.0, 0.0, 0.0))?,
        ),
        (
            "fillType5",
            theme_color::adjust(&secondary_color, ColorAdjustment::hsl(-64.0, 0.0, 0.0))?,
        ),
        (
            "fillType6",
            theme_color::adjust(&primary_color, ColorAdjustment::hsl(128.0, 0.0, 0.0))?,
        ),
        (
            "fillType7",
            theme_color::adjust(&secondary_color, ColorAdjustment::hsl(128.0, 0.0, 0.0))?,
        ),
    ] {
        tv.insert(key.to_string(), Value::String(color));
    }

    set_string_if_missing(&mut tv, "mainBkg", primary_color.clone());
    set_string_if_missing(&mut tv, "secondBkg", secondary_color.clone());
    set_string_if_missing(&mut tv, "mainContrastColor", "lightgrey");
    set_string_if_missing(
        &mut tv,
        "darkTextColor",
        "hsl(28.5714285714, 17.3553719008%, 86.2745098039%)",
    );
    set_string_if_missing(&mut tv, "lineColor", "lightgrey");
    set_string_if_missing(&mut tv, "arrowheadColor", "lightgrey");

    // Flowchart/block/class surfaces.
    set_string_if_missing(&mut tv, "nodeBkg", primary_color.clone());
    set_string_if_missing(&mut tv, "mainBkg", primary_color.clone());
    set_string_if_missing(&mut tv, "nodeBorder", border1.clone());
    set_string_if_missing(&mut tv, "clusterBkg", secondary_color.clone());
    set_string_if_missing(&mut tv, "clusterBorder", border2.clone());
    set_string_if_missing(&mut tv, "defaultLinkColor", line_color.clone());
    set_string_if_missing(&mut tv, "edgeLabelBackground", "hsl(0, 0%, 34.4117647059%)");
    set_string_if_missing(&mut tv, "classText", primary_text_color.clone());

    // Sequence diagram and note text must stay light on dark actor/message backgrounds.
    set_string_if_missing(&mut tv, "actorBorder", border1.clone());
    set_string_if_missing(&mut tv, "actorBkg", primary_color.clone());
    set_string_if_missing(&mut tv, "actorTextColor", "lightgrey");
    set_string_if_missing(&mut tv, "actorLineColor", border1.clone());
    set_string_if_missing(&mut tv, "signalColor", "lightgrey");
    set_string_if_missing(&mut tv, "signalTextColor", "lightgrey");
    set_string_if_missing(&mut tv, "labelBoxBkgColor", primary_color.clone());
    set_string_if_missing(&mut tv, "labelBoxBorderColor", border1.clone());
    set_string_if_missing(&mut tv, "labelTextColor", "lightgrey");
    set_string_if_missing(&mut tv, "loopTextColor", "lightgrey");
    set_string_if_missing(&mut tv, "noteBorderColor", secondary_border_color.clone());
    set_string_if_missing(&mut tv, "noteBkgColor", secondary_color.clone());
    set_string_if_missing(&mut tv, "noteTextColor", secondary_text_color.clone());
    set_string_if_missing(&mut tv, "activationBorderColor", border1);
    set_string_if_missing(&mut tv, "activationBkgColor", secondary_color.clone());
    set_string_if_missing(&mut tv, "sequenceNumberColor", "black");
    set_string_if_missing(&mut tv, "rectBkgColor", tertiary_color.clone());

    // Gantt text colors are deliberately not all the same: completed-task labels use the
    // inverse of the light completed-task fill, while outside labels stay light on dark canvas.
    set_string_if_missing(
        &mut tv,
        "sectionBkgColor",
        "hsl(50, 26.087%, 48.2352941176%)",
    );
    set_string_if_missing(&mut tv, "altSectionBkgColor", background.clone());
    set_string_if_missing(&mut tv, "sectionBkgColor2", "#EAE8D9");
    set_string_if_missing(
        &mut tv,
        "excludeBkgColor",
        "hsl(50, 26.087%, 38.2352941176%)",
    );
    set_string_if_missing(
        &mut tv,
        "taskBorderColor",
        theme_color::rgba(255.0, 255.0, 255.0, 70.0)?,
    );
    set_string_if_missing(
        &mut tv,
        "taskBkgColor",
        "hsl(180, 1.5873015873%, 35.3529411765%)",
    );
    set_string_if_missing(
        &mut tv,
        "taskTextColor",
        "hsl(28.5714285714, 17.3553719008%, 86.2745098039%)",
    );
    set_string_if_missing(&mut tv, "taskTextLightColor", "lightgrey");
    set_string_if_missing(&mut tv, "taskTextOutsideColor", "lightgrey");
    set_string_if_missing(&mut tv, "taskTextClickableColor", "#003163");
    set_string_if_missing(
        &mut tv,
        "activeTaskBorderColor",
        theme_color::rgba(255.0, 255.0, 255.0, 50.0)?,
    );
    set_string_if_missing(&mut tv, "activeTaskBkgColor", "#81B1DB");
    set_string_if_missing(&mut tv, "gridColor", "lightgrey");
    set_string_if_missing(&mut tv, "doneTaskBkgColor", "lightgrey");
    set_string_if_missing(&mut tv, "doneTaskBorderColor", "grey");
    set_string_if_missing(&mut tv, "critBorderColor", "#E83737");
    set_string_if_missing(&mut tv, "critBkgColor", "#E83737");
    set_string_if_missing(&mut tv, "taskTextDarkColor", "#2c2c2c");
    set_string_if_missing(&mut tv, "todayLineColor", "#DB5757");
    set_string_if_missing(&mut tv, "vertLineColor", "#00BFFF");

    // C4, architecture, ER, and state surfaces.
    set_string_if_missing(&mut tv, "personBorder", primary_border_color.clone());
    set_string_if_missing(&mut tv, "personBkg", primary_color.clone());
    set_string_if_missing(&mut tv, "archEdgeColor", line_color.clone());
    set_string_if_missing(&mut tv, "archEdgeArrowColor", line_color.clone());
    set_string_if_missing(&mut tv, "archEdgeWidth", "3");
    set_string_if_missing(
        &mut tv,
        "archGroupBorderColor",
        primary_border_color.clone(),
    );
    set_string_if_missing(&mut tv, "archGroupBorderWidth", "2px");
    set_string_if_missing(&mut tv, "rowOdd", "hsl(180, 1.5873015873%, 17.3529411765%)");
    set_string_if_missing(&mut tv, "rowEven", "hsl(180, 1.5873015873%, 2.3529411765%)");
    set_string_if_missing(&mut tv, "transitionColor", line_color.clone());
    set_string_if_missing(&mut tv, "transitionLabelColor", text_color.clone());
    set_string_if_missing(&mut tv, "stateLabelColor", primary_text_color.clone());
    set_string_if_missing(&mut tv, "stateBkg", primary_color.clone());
    set_string_if_missing(&mut tv, "labelBackgroundColor", primary_color.clone());
    set_string_if_missing(&mut tv, "compositeBackground", background.clone());
    set_string_if_missing(&mut tv, "altBackground", "#555");
    set_string_if_missing(&mut tv, "compositeTitleBackground", primary_color.clone());
    let composite_border =
        get_truthy_string(&tv, "nodeBorder").unwrap_or_else(|| "#ccc".to_string());
    set_string_if_missing(&mut tv, "compositeBorder", composite_border);
    set_string_if_missing(&mut tv, "innerEndBackground", primary_border_color.clone());
    set_string_if_missing(&mut tv, "specialStateColor", "#f4f4f4");
    set_string_if_missing_with(&mut tv, "emSwimlaneBackgroundOdd", || {
        theme_color::lighten(&background, 5.0)
    })?;
    set_string_if_missing_with(&mut tv, "emSwimlaneBackgroundStroke", || {
        theme_color::lighten(&background, 12.0)
    })?;
    set_string_if_missing_with(&mut tv, "attributeBackgroundColorOdd", || {
        theme_color::lighten(&background, 12.0)
    })?;
    set_string_if_missing_with(&mut tv, "attributeBackgroundColorEven", || {
        theme_color::lighten(&background, 2.0)
    })?;
    set_string_if_missing(&mut tv, "noteFontWeight", "normal");
    set_string_if_missing(&mut tv, "fontWeight", "normal");
    set_string_if_missing(
        &mut tv,
        "dropShadow",
        "drop-shadow( 1px 2px 2px rgba(185,185,185,1))",
    );

    // Mermaid's `config.ts` calls `theme-dark.getThemeVariables(conf.themeVariables)` without
    // injecting `darkMode=true`, so `theme-dark.js` falls back to `labelTextColor` here.
    let label_text_color =
        get_truthy_string(&tv, "labelTextColor").unwrap_or_else(|| "lightgrey".to_string());
    set_if_missing(
        &mut tv,
        "scaleLabelColor",
        Value::String(label_text_color.clone()),
    );
    let scale_label_color =
        get_truthy_string(&tv, "scaleLabelColor").unwrap_or_else(|| label_text_color.clone());

    for (i, c_hex) in c_scales_hex.iter().enumerate() {
        let c_scale_key = format!("cScale{i}");
        let default_scale = if i == 0 {
            primary_color.clone()
        } else {
            (*c_hex).to_string()
        };
        set_if_missing(&mut tv, &c_scale_key, Value::String(default_scale));

        let c_scale = required_color(&tv, &c_scale_key)?;

        // `theme-dark` peers: `lighten(cScale, 10)`.
        set_if_missing(
            &mut tv,
            &format!("cScalePeer{i}"),
            Value::String(theme_color::lighten(&c_scale, 10.0)?),
        );

        // `theme-dark` inverted scale: `invert(cScale)`.
        set_if_missing(
            &mut tv,
            &format!("cScaleInv{i}"),
            Value::String(theme_color::invert(&c_scale)?),
        );

        // `theme-dark` label scale: `scaleLabelColor`.
        set_if_missing(
            &mut tv,
            &format!("cScaleLabel{i}"),
            Value::String(scale_label_color.clone()),
        );
    }

    set_string_if_missing(&mut tv, "pieTitleTextColor", line_color.clone());
    set_string_if_missing(&mut tv, "pieSectionTextColor", text_color);
    set_string_if_missing(&mut tv, "pieLegendTextColor", line_color.clone());
    set_string_if_missing(&mut tv, "branchLabelColor", "#2c2c2c");
    set_string_if_missing(&mut tv, "gitBranchLabel0", "#2c2c2c");
    set_string_if_missing(&mut tv, "gitBranchLabel1", "lightgrey");
    set_string_if_missing(&mut tv, "gitBranchLabel2", "lightgrey");
    set_string_if_missing(&mut tv, "gitBranchLabel3", "#2c2c2c");
    for i in 4..8 {
        set_string_if_missing(&mut tv, &format!("gitBranchLabel{i}"), "lightgrey");
    }
    set_string_if_missing(&mut tv, "tagLabelColor", primary_text_color);
    set_string_if_missing(&mut tv, "tagLabelBackground", primary_color);
    set_string_if_missing(&mut tv, "tagLabelBorder", primary_border_color);
    set_string_if_missing(&mut tv, "tagLabelFontSize", "10px");
    set_string_if_missing(&mut tv, "commitLabelColor", secondary_text_color);
    set_string_if_missing(&mut tv, "commitLabelBackground", secondary_color);
    set_string_if_missing(&mut tv, "commitLabelFontSize", "10px");

    // `theme-dark` xychart palette + colors.
    // Source: `theme-dark.js`.
    ensure_xychart_theme_defaults(
        &mut tv,
        "#3498db,#2ecc71,#e74c3c,#f1c40f,#bdc3c7,#ffffff,#34495e,#9b59b6,#1abc9c,#e67e22",
    );
    apply_current_quadrant_theme_defaults(&mut tv)?;

    finish_theme_defaults(config, "dark", tv)
}

fn apply_forest_theme_defaults(config: &mut MermaidConfig) -> Result<(), ColorError> {
    let mut tv = theme_variables_map(config);
    let explicit_theme_variables = tv.clone();

    // Mermaid 11.16.1: `theme-forest` base colors.
    // Source: `repo-ref/mermaid/packages/mermaid/src/themes/theme-forest.js`.
    //
    // NOTE: `theme-forest` is not a thin palette override. It sets several diagram-facing
    // variables (flowchart/state/sequence/...) in its `constructor()` + `updateColors()`.
    // We explicitly seed those values here so headless SVG rendering can match upstream.
    set_if_missing(
        &mut tv,
        "primaryColor",
        Value::String("#cde498".to_string()),
    );
    set_if_missing(
        &mut tv,
        "secondaryColor",
        Value::String("#cdffb2".to_string()),
    );
    set_if_missing(&mut tv, "background", Value::String("white".to_string()));
    set_if_missing(&mut tv, "border1", Value::String("#13540c".to_string()));
    set_if_missing(&mut tv, "border2", Value::String("#6eaa49".to_string()));
    set_if_missing(
        &mut tv,
        "arrowheadColor",
        Value::String("green".to_string()),
    );
    set_if_missing(&mut tv, "fontFamily", mermaid_default_font_family());
    set_if_missing(&mut tv, "fontSize", Value::String("16px".to_string()));
    set_if_missing(&mut tv, "titleColor", Value::String("#333".to_string()));
    set_if_missing(
        &mut tv,
        "edgeLabelBackground",
        Value::String("#e8e8e8".to_string()),
    );
    set_if_missing(
        &mut tv,
        "errorBkgColor",
        Value::String("#552222".to_string()),
    );
    set_if_missing(
        &mut tv,
        "errorTextColor",
        Value::String("#552222".to_string()),
    );

    let primary_color = required_color(&tv, "primaryColor")?;
    if get_truthy_string(&tv, "primaryTextColor").is_none() {
        tv.insert(
            "primaryTextColor".to_string(),
            Value::String(theme_color::invert(&primary_color)?),
        );
    }

    let secondary_color =
        get_truthy_string(&tv, "secondaryColor").unwrap_or_else(|| "#cdffb2".to_string());

    // `theme-forest` diagram-facing surfaces.
    // Source: `theme-forest.js` constructor + `updateColors()`.
    set_if_missing(&mut tv, "mainBkg", Value::String(primary_color.clone()));
    set_if_missing(&mut tv, "secondBkg", Value::String(secondary_color.clone()));
    // Table striping colors (used by ER diagrams).
    // Source: `theme-forest.js`:
    //   rowOdd  = lighten(mainBkg, 75) || '#ffffff'
    //   rowEven = lighten(mainBkg, 20)
    set_if_missing(
        &mut tv,
        "rowOdd",
        Value::String(theme_color::lighten(&primary_color, 75.0)?),
    );
    set_if_missing(
        &mut tv,
        "rowEven",
        Value::String(theme_color::lighten(&primary_color, 20.0)?),
    );

    // `invert('white')` in `khroma` ends up as a pure black in Mermaid's serialized SVG output.
    set_if_missing(&mut tv, "lineColor", Value::String("#000000".to_string()));
    set_if_missing(&mut tv, "textColor", Value::String("#000000".to_string()));

    // Flowchart variables (after `updateColors()`).
    set_if_missing(&mut tv, "nodeBkg", Value::String(primary_color.clone()));
    set_if_missing(&mut tv, "nodeBorder", Value::String("#13540c".to_string()));
    set_if_missing(
        &mut tv,
        "clusterBkg",
        Value::String(secondary_color.clone()),
    );
    set_if_missing(
        &mut tv,
        "clusterBorder",
        Value::String("#6eaa49".to_string()),
    );
    set_if_missing(
        &mut tv,
        "defaultLinkColor",
        Value::String("#000000".to_string()),
    );

    // mkBorder(...) helper (shared across themes).
    let dark_mode = tv.get("darkMode").is_some_and(is_js_truthy);
    set_if_missing(
        &mut tv,
        "primaryBorderColor",
        Value::String(mk_border(&primary_color, dark_mode)?),
    );
    set_if_missing(
        &mut tv,
        "secondaryBorderColor",
        Value::String(mk_border(&secondary_color, dark_mode)?),
    );
    ensure_gradient_theme_defaults(&mut tv);

    // `theme-forest` sets: `tertiaryColor = lighten(primaryColor, 10)`.
    let tertiary_color = if let Some(color) = get_truthy_string(&tv, "tertiaryColor") {
        color
    } else {
        theme_color::lighten(&primary_color, 10.0)?
    };
    set_if_missing(
        &mut tv,
        "tertiaryColor",
        Value::String(tertiary_color.clone()),
    );
    set_if_missing(
        &mut tv,
        "tertiaryBorderColor",
        Value::String(mk_border(&tertiary_color, dark_mode)?),
    );

    // `theme-forest` ends up using black label text (via `actorTextColor`).
    set_if_missing(
        &mut tv,
        "labelTextColor",
        Value::String("black".to_string()),
    );
    set_if_missing(
        &mut tv,
        "scaleLabelColor",
        Value::String("black".to_string()),
    );
    let scale_label_color =
        get_truthy_string(&tv, "scaleLabelColor").unwrap_or_else(|| "black".to_string());

    // Color scales: match `theme-forest` `updateColors()`:
    // - derive from base colors / hue shifts
    // - darken each `cScale*` by 10
    // - `cScalePeer1/2` use special darken amounts, others are darken(`cScale*`, 25)
    let c_scale_bases = [
        primary_color.clone(),
        secondary_color.clone(),
        tertiary_color.clone(),
        theme_color::adjust(&primary_color, ColorAdjustment::hsl(30.0, 0.0, 0.0))?,
        theme_color::adjust(&primary_color, ColorAdjustment::hsl(60.0, 0.0, 0.0))?,
        theme_color::adjust(&primary_color, ColorAdjustment::hsl(90.0, 0.0, 0.0))?,
        theme_color::adjust(&primary_color, ColorAdjustment::hsl(120.0, 0.0, 0.0))?,
        theme_color::adjust(&primary_color, ColorAdjustment::hsl(150.0, 0.0, 0.0))?,
        theme_color::adjust(&primary_color, ColorAdjustment::hsl(210.0, 0.0, 0.0))?,
        theme_color::adjust(&primary_color, ColorAdjustment::hsl(270.0, 0.0, 0.0))?,
        theme_color::adjust(&primary_color, ColorAdjustment::hsl(300.0, 0.0, 0.0))?,
        theme_color::adjust(&primary_color, ColorAdjustment::hsl(330.0, 0.0, 0.0))?,
    ];
    let mut c_scales = Vec::with_capacity(c_scale_bases.len());
    for base in c_scale_bases {
        c_scales.push(theme_color::darken(&base, 10.0)?);
    }

    for (i, v) in c_scales.iter().enumerate() {
        set_if_missing(&mut tv, &format!("cScale{i}"), Value::String(v.clone()));
    }

    set_if_missing(
        &mut tv,
        "cScalePeer1",
        Value::String(theme_color::darken(&secondary_color, 45.0)?),
    );
    set_if_missing(
        &mut tv,
        "cScalePeer2",
        Value::String(theme_color::darken(&tertiary_color, 40.0)?),
    );

    for (i, fallback_color) in c_scales.iter().enumerate() {
        let c_scale_key = format!("cScale{i}");
        let mut color =
            get_truthy_string(&tv, &c_scale_key).unwrap_or_else(|| fallback_color.clone());
        if explicit_theme_variables.contains_key(&c_scale_key) {
            color = theme_color::darken(&color, 10.0)?;
        }
        set_if_missing(
            &mut tv,
            &format!("cScalePeer{i}"),
            Value::String(theme_color::darken(&color, 25.0)?),
        );
        set_if_missing(
            &mut tv,
            &format!("cScaleInv{i}"),
            Value::String(theme_color::adjust(
                &color,
                ColorAdjustment::hsl(180.0, 0.0, 0.0),
            )?),
        );
        set_if_missing(
            &mut tv,
            &format!("cScaleLabel{i}"),
            Value::String(scale_label_color.clone()),
        );
    }

    // `theme-forest` xychart palette + colors.
    // Source: `theme-forest.js`.
    ensure_xychart_theme_defaults(
        &mut tv,
        "#CDE498,#FF6B6B,#A0D2DB,#D7BDE2,#F0F0F0,#FFC3A0,#7FD8BE,#FF9A8B,#FAF3E0,#FFF176",
    );
    apply_current_quadrant_theme_defaults(&mut tv)?;

    finish_theme_defaults(config, "forest", tv)
}

fn apply_neutral_theme_defaults(config: &mut MermaidConfig) -> Result<(), ColorError> {
    let mut tv = theme_variables_map(config);

    // `theme-neutral` constructor defaults.
    // Source: `repo-ref/mermaid/packages/mermaid/src/themes/theme-neutral.js`.
    set_string_if_missing(&mut tv, "background", "#ffffff");
    set_string_if_missing(&mut tv, "primaryColor", "#eee");
    set_if_missing(&mut tv, "fontFamily", mermaid_default_font_family());
    set_string_if_missing(&mut tv, "fontSize", "16px");
    if get_truthy_string(&tv, "primaryTextColor").is_none()
        && let Some(primary_color) = get_truthy_string(&tv, "primaryColor")
    {
        tv.insert(
            "primaryTextColor".to_string(),
            Value::String(theme_color::invert(&primary_color)?),
        );
    }

    // Mermaid 11.16.1: `theme-neutral` color scale seeds.
    // Source: `repo-ref/mermaid/packages/mermaid/src/themes/theme-neutral.js`.
    let c_scales_hex: [&str; 12] = [
        "#555", "#F4F4F4", "#555", "#BBB", "#777", "#999", "#DDD", "#FFF", "#DDD", "#BBB", "#999",
        "#777",
    ];

    let primary_color =
        get_truthy_string(&tv, "primaryColor").unwrap_or_else(|| "#eee".to_string());
    let contrast = get_truthy_string(&tv, "contrast").unwrap_or_else(|| "#707070".to_string());
    let default_secondary_color = theme_color::lighten(&contrast, 55.0)?;
    let default_tertiary_color =
        theme_color::adjust(&primary_color, ColorAdjustment::hsl(-160.0, 0.0, 0.0))?;
    set_if_missing(
        &mut tv,
        "secondaryColor",
        Value::String(default_secondary_color.clone()),
    );
    set_string_if_missing(&mut tv, "tertiaryColor", default_tertiary_color.clone());
    set_if_missing(
        &mut tv,
        "primaryBorderColor",
        Value::String(mk_border(&primary_color, false)?),
    );
    set_if_missing(
        &mut tv,
        "secondaryBorderColor",
        Value::String(mk_border(&default_secondary_color, false)?),
    );
    set_string_if_missing(
        &mut tv,
        "tertiaryBorderColor",
        mk_border(&default_tertiary_color, false)?,
    );
    set_string_if_missing(
        &mut tv,
        "secondaryTextColor",
        theme_color::invert(&default_secondary_color)?,
    );
    set_string_if_missing(
        &mut tv,
        "tertiaryTextColor",
        theme_color::invert(&default_tertiary_color)?,
    );
    ensure_gradient_theme_defaults(&mut tv);

    let secondary_color =
        get_truthy_string(&tv, "secondaryColor").unwrap_or_else(|| default_secondary_color.clone());
    let tertiary_color =
        get_truthy_string(&tv, "tertiaryColor").unwrap_or_else(|| default_tertiary_color.clone());
    let secondary_text_color = match get_truthy_string(&tv, "secondaryTextColor") {
        Some(color) => color,
        None => theme_color::invert(&secondary_color)?,
    };
    let tertiary_text_color = match get_truthy_string(&tv, "tertiaryTextColor") {
        Some(color) => color,
        None => theme_color::invert(&tertiary_color)?,
    };
    let background = get_truthy_string(&tv, "background").unwrap_or_else(|| "#ffffff".to_string());
    let primary_text_color =
        get_truthy_string(&tv, "primaryTextColor").unwrap_or_else(|| "#111111".to_string());
    let text_color = get_truthy_string(&tv, "textColor").unwrap_or_else(|| "#000000".to_string());
    let primary_border_color = match get_truthy_string(&tv, "primaryBorderColor") {
        Some(color) => color,
        None => mk_border(&primary_color, false)?,
    };
    let contrast = get_truthy_string(&tv, "contrast").unwrap_or_else(|| "#707070".to_string());

    set_string_if_missing(&mut tv, "textColor", "#000000");
    set_string_if_missing(&mut tv, "mainBkg", primary_color.clone());
    set_string_if_missing(&mut tv, "secondBkg", secondary_color.clone());
    set_string_if_missing(&mut tv, "lineColor", "#666");
    set_string_if_missing(&mut tv, "border1", "#999");
    let border1 = required_color(&tv, "border1")?;
    set_string_if_missing(&mut tv, "border2", contrast.clone());
    set_string_if_missing(&mut tv, "note", "#ffa");
    set_string_if_missing(&mut tv, "text", "#333");
    set_string_if_missing(&mut tv, "critical", "#d42");
    set_string_if_missing(&mut tv, "done", "#bbb");
    set_string_if_missing(&mut tv, "arrowheadColor", "#333333");
    set_if_missing(&mut tv, "THEME_COLOR_LIMIT", Value::Number(12.into()));
    set_if_missing(&mut tv, "radius", Value::Number(5.into()));
    set_if_missing(&mut tv, "strokeWidth", Value::Number(1.into()));

    // Flowchart/block/class text follows the neutral foreground family, not the default theme's
    // purple/yellow assumptions.
    set_string_if_missing(&mut tv, "nodeBkg", primary_color.clone());
    set_string_if_missing(&mut tv, "nodeBorder", "#999");
    set_string_if_missing(&mut tv, "clusterBkg", secondary_color.clone());
    set_string_if_missing(&mut tv, "clusterBorder", contrast);
    set_string_if_missing(&mut tv, "defaultLinkColor", "#666");
    set_string_if_missing(&mut tv, "titleColor", "#333");
    set_string_if_missing(&mut tv, "edgeLabelBackground", "white");
    set_string_if_missing(&mut tv, "classText", primary_text_color.clone());

    // Sequence and note colors. Neutral assigns the border transforms unconditionally.
    let actor_border = theme_color::lighten(&border1, 23.0)?;
    tv.insert(
        "actorBorder".to_string(),
        Value::String(actor_border.clone()),
    );
    set_string_if_missing(&mut tv, "actorBkg", primary_color.clone());
    set_string_if_missing(&mut tv, "actorTextColor", "#333");
    tv.insert(
        "actorLineColor".to_string(),
        Value::String(actor_border.clone()),
    );
    set_string_if_missing(&mut tv, "signalColor", "#333");
    set_string_if_missing(&mut tv, "signalTextColor", "#333");
    set_string_if_missing(&mut tv, "labelBoxBkgColor", primary_color.clone());
    tv.insert(
        "labelBoxBorderColor".to_string(),
        Value::String(actor_border),
    );
    set_string_if_missing(&mut tv, "labelTextColor", "#333");
    set_string_if_missing(&mut tv, "loopTextColor", "#333");
    set_string_if_missing(&mut tv, "noteBorderColor", "#999");
    set_string_if_missing(&mut tv, "noteBkgColor", "#666");
    set_string_if_missing(&mut tv, "noteTextColor", "#fff");
    set_string_if_missing(&mut tv, "activationBorderColor", "#666");
    set_string_if_missing(&mut tv, "activationBkgColor", "#f4f4f4");
    set_string_if_missing(&mut tv, "sequenceNumberColor", "white");

    // Gantt and general text colors.
    set_string_if_missing(&mut tv, "sectionBkgColor", "hsl(0, 0%, 73.9215686275%)");
    set_string_if_missing(&mut tv, "altSectionBkgColor", "white");
    set_string_if_missing(&mut tv, "sectionBkgColor2", "hsl(0, 0%, 73.9215686275%)");
    set_string_if_missing(&mut tv, "excludeBkgColor", "#eeeeee");
    set_string_if_missing(&mut tv, "taskBorderColor", "hsl(0, 0%, 34.1176470588%)");
    set_string_if_missing(&mut tv, "taskBkgColor", "#707070");
    set_string_if_missing(&mut tv, "taskTextLightColor", "white");
    set_string_if_missing(&mut tv, "taskTextColor", "white");
    set_string_if_missing(&mut tv, "taskTextDarkColor", "#333");
    set_string_if_missing(&mut tv, "taskTextOutsideColor", "#333");
    set_string_if_missing(&mut tv, "taskTextClickableColor", "#003163");
    set_string_if_missing(
        &mut tv,
        "activeTaskBorderColor",
        "hsl(0, 0%, 34.1176470588%)",
    );
    set_string_if_missing(&mut tv, "activeTaskBkgColor", primary_color.clone());
    tv.insert(
        "gridColor".to_string(),
        Value::String(theme_color::lighten(&border1, 30.0)?),
    );
    set_string_if_missing(&mut tv, "doneTaskBkgColor", "#bbb");
    set_string_if_missing(&mut tv, "doneTaskBorderColor", "#666");
    set_string_if_missing(&mut tv, "critBkgColor", "#d42");
    set_string_if_missing(
        &mut tv,
        "critBorderColor",
        "hsl(9.4736842105, 72.1518987342%, 44.5098039216%)",
    );
    set_string_if_missing(&mut tv, "todayLineColor", "#d42");
    set_string_if_missing(&mut tv, "vertLineColor", "#d42");

    // C4, architecture, ER, and state surfaces.
    set_string_if_missing(&mut tv, "personBorder", primary_border_color.clone());
    set_string_if_missing(&mut tv, "personBkg", primary_color.clone());
    set_string_if_missing(&mut tv, "archEdgeColor", "#666");
    set_string_if_missing(&mut tv, "archEdgeArrowColor", "#666");
    set_string_if_missing(&mut tv, "archEdgeWidth", "3");
    set_string_if_missing(
        &mut tv,
        "archGroupBorderColor",
        primary_border_color.clone(),
    );
    set_string_if_missing(&mut tv, "archGroupBorderWidth", "2px");
    set_string_if_missing(&mut tv, "rowOdd", "hsl(0, 0%, 100%)");
    set_string_if_missing(&mut tv, "rowEven", "#f4f4f4");
    set_string_if_missing(&mut tv, "transitionColor", "#000");
    set_string_if_missing(&mut tv, "transitionLabelColor", text_color.clone());
    set_string_if_missing(&mut tv, "stateLabelColor", primary_text_color.clone());
    set_string_if_missing(&mut tv, "stateBkg", primary_color.clone());
    set_string_if_missing(&mut tv, "labelBackgroundColor", primary_color.clone());
    set_string_if_missing(&mut tv, "compositeBackground", background);
    set_string_if_missing(&mut tv, "altBackground", "#f4f4f4");
    set_string_if_missing(&mut tv, "compositeTitleBackground", primary_color.clone());
    set_string_if_missing(&mut tv, "stateBorder", "#000");
    set_string_if_missing(&mut tv, "innerEndBackground", primary_border_color.clone());
    set_string_if_missing(&mut tv, "specialStateColor", "#222");
    set_string_if_missing(&mut tv, "errorBkgColor", tertiary_color);
    set_string_if_missing(&mut tv, "errorTextColor", tertiary_text_color);
    set_string_if_missing(&mut tv, "attributeBackgroundColorOdd", "#ffffff");
    set_string_if_missing(&mut tv, "attributeBackgroundColorEven", "#f2f2f2");
    set_string_if_missing(&mut tv, "noteFontWeight", "normal");
    set_string_if_missing(&mut tv, "fontWeight", "normal");
    set_string_if_missing(
        &mut tv,
        "dropShadow",
        "drop-shadow( 1px 2px 2px rgba(185,185,185,1))",
    );

    for (key, color) in [
        ("fillType0", primary_color.clone()),
        ("fillType1", secondary_color.clone()),
        (
            "fillType2",
            theme_color::adjust(&primary_color, ColorAdjustment::hsl(64.0, 0.0, 0.0))?,
        ),
        (
            "fillType3",
            theme_color::adjust(&secondary_color, ColorAdjustment::hsl(64.0, 0.0, 0.0))?,
        ),
        (
            "fillType4",
            theme_color::adjust(&primary_color, ColorAdjustment::hsl(-64.0, 0.0, 0.0))?,
        ),
        (
            "fillType5",
            theme_color::adjust(&secondary_color, ColorAdjustment::hsl(-64.0, 0.0, 0.0))?,
        ),
        (
            "fillType6",
            theme_color::adjust(&primary_color, ColorAdjustment::hsl(128.0, 0.0, 0.0))?,
        ),
        (
            "fillType7",
            theme_color::adjust(&secondary_color, ColorAdjustment::hsl(128.0, 0.0, 0.0))?,
        ),
    ] {
        tv.insert(key.to_string(), Value::String(color));
    }

    set_string_if_missing(&mut tv, "scaleLabelColor", "#333");
    let scale_label_color =
        get_truthy_string(&tv, "scaleLabelColor").unwrap_or_else(|| "#333".to_string());

    for (i, c_hex) in c_scales_hex.iter().enumerate() {
        let c_scale_key = format!("cScale{i}");
        set_if_missing(&mut tv, &c_scale_key, Value::String((*c_hex).to_string()));

        let c_scale = required_color(&tv, &c_scale_key)?;

        // `theme-neutral` peers: `darken(cScale, 10)` (darkMode defaults to false).
        set_if_missing(
            &mut tv,
            &format!("cScalePeer{i}"),
            Value::String(theme_color::darken(&c_scale, 10.0)?),
        );

        // `theme-neutral` inverted scale: `invert(cScale)`.
        set_if_missing(
            &mut tv,
            &format!("cScaleInv{i}"),
            Value::String(theme_color::invert(&c_scale)?),
        );

        // `theme-neutral` label scale: `scaleLabelColor`, with special-cased indices.
        // - `cScaleLabel0` and `cScaleLabel2`: `cScale1` (light fill needs dark text)
        if i == 0 || i == 2 {
            set_if_missing(
                &mut tv,
                &format!("cScaleLabel{i}"),
                Value::String(c_scales_hex[1].to_string()),
            );
        }
        set_if_missing(
            &mut tv,
            &format!("cScaleLabel{i}"),
            Value::String(scale_label_color.clone()),
        );
    }

    set_string_if_missing(&mut tv, "pieTitleTextColor", "#333");
    set_string_if_missing(&mut tv, "pieSectionTextColor", text_color);
    set_string_if_missing(&mut tv, "pieLegendTextColor", "#333");
    set_string_if_missing(&mut tv, "branchLabelColor", "#333");
    set_string_if_missing(&mut tv, "gitBranchLabel0", "#333");
    set_string_if_missing(&mut tv, "gitBranchLabel1", "white");
    set_string_if_missing(&mut tv, "gitBranchLabel2", "#333");
    set_string_if_missing(&mut tv, "gitBranchLabel3", "white");
    for i in 4..8 {
        set_string_if_missing(&mut tv, &format!("gitBranchLabel{i}"), "#333");
    }
    set_string_if_missing(&mut tv, "tagLabelColor", primary_text_color);
    set_string_if_missing(&mut tv, "tagLabelBackground", primary_color);
    set_string_if_missing(&mut tv, "tagLabelBorder", primary_border_color);
    set_string_if_missing(&mut tv, "tagLabelFontSize", "10px");
    set_string_if_missing(&mut tv, "commitLabelColor", secondary_text_color);
    set_string_if_missing(&mut tv, "commitLabelBackground", secondary_color);
    set_string_if_missing(&mut tv, "commitLabelFontSize", "10px");

    // `theme-neutral` xychart palette + colors.
    // Source: `repo-ref/mermaid/packages/mermaid/src/themes/theme-neutral.js`.
    ensure_xychart_theme_defaults(
        &mut tv,
        "#EEE,#6BB8E4,#8ACB88,#C7ACD6,#E8DCC2,#FFB2A8,#FFF380,#7E8D91,#FFD8B1,#FAF3E0",
    );
    apply_current_quadrant_theme_defaults(&mut tv)?;

    finish_theme_defaults(config, "neutral", tv)
}

fn apply_base_theme_defaults(config: &mut MermaidConfig) -> Result<(), ColorError> {
    let tv = base::calculate(&theme_variables_map(config))?;
    finish_theme_defaults(config, "base", tv)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn supported_theme_names_match_core_expansion_surface() {
        assert_eq!(
            crate::supported_themes(),
            &[
                "default",
                "base",
                "dark",
                "forest",
                "neutral",
                "neo",
                "neo-dark",
                "redux",
                "redux-dark",
                "redux-color",
                "redux-dark-color"
            ]
        );
    }

    #[test]
    fn all_themes_preserve_generated_object_keys_after_partial_overrides() {
        let mut covered = std::collections::BTreeSet::new();
        for theme in crate::supported_themes() {
            let mut baseline = MermaidConfig::from_value(json!({"theme": theme}));
            apply_theme_defaults(&mut baseline).unwrap();
            let generated = theme_variables_map(&baseline);
            for (variable, value) in &generated {
                let Some(nested) = value.as_object().filter(|value| value.len() > 1) else {
                    continue;
                };
                let first = nested.keys().next().unwrap();
                let mut config = MermaidConfig::from_value(json!({
                    "theme": theme,
                    "themeVariables": { (variable): { (first): "#123456" } }
                }));
                apply_theme_defaults(&mut config).unwrap();
                let resolved = theme_variables_map(&config);
                let observed = resolved.get(variable).and_then(Value::as_object).unwrap();
                assert_eq!(
                    observed.get(first),
                    Some(&json!("#123456")),
                    "{theme}/{variable}"
                );
                for (key, expected) in nested {
                    if key != first {
                        assert_eq!(
                            observed.get(key),
                            Some(expected),
                            "{theme}/{variable}/{key}"
                        );
                    }
                }
                covered.insert(variable.clone());
            }
        }
        for variable in ["xyChart", "radar", "cynefin"] {
            assert!(
                covered.contains(variable),
                "missing object-valued theme variable {variable}"
            );
        }
    }

    #[test]
    fn theme_replay_object_merge_is_shallow_and_retains_scalar_replacement() {
        let mut snapshot = ThemeStageSnapshot::from_variables(
            json!({
                "chart": { "nested": { "keep": 1, "replace": 2 }, "defaultColor": "blue" },
                "array": [1, 2], "scalar": "old", "nullable": "old"
            })
            .as_object()
            .unwrap()
            .clone(),
        );
        snapshot.overlay(
            json!({
                "chart": { "nested": { "replace": 3 } },
                "array": [3], "scalar": "new", "nullable": null
            })
            .as_object()
            .unwrap(),
            true,
        );
        assert_eq!(
            snapshot.variables["chart"],
            json!({
                "nested": { "replace": 3 }, "defaultColor": "blue"
            })
        );
        assert_eq!(snapshot.variables["array"], json!([3]));
        assert_eq!(snapshot.variables["scalar"], json!("new"));
        assert_eq!(snapshot.variables["nullable"], Value::Null);
    }

    #[test]
    fn generated_theme_overrides_preserve_null_replacement_and_removal() {
        let base = serde_json::json!({"unchanged": false, "deleted": 1, "nullable": "old", "object": {"old": 1}});
        let restored = restore_theme_overrides(
            base.as_object().unwrap(),
            serde_json::json!({
                "set": {"nullable": null, "object": {"new": 2}, "added": 0},
                "remove": ["deleted"]
            }),
        );
        assert_eq!(
            Value::Object(restored),
            serde_json::json!({
                "unchanged": false, "nullable": null, "object": {"new": 2}, "added": 0
            })
        );
    }

    #[test]
    fn supported_theme_defaults_match_upstream_snapshot() {
        for &theme in SUPPORTED_THEME_NAMES {
            let mut cfg = MermaidConfig::from_value(json!({
                "theme": theme
            }));
            apply_theme_defaults(&mut cfg).unwrap();

            let actual = cfg
                .as_value()
                .get("themeVariables")
                .and_then(|v| v.as_object())
                .unwrap();
            let expected = ThemeProgram::resolve(theme).default_snapshot();

            assert_eq!(actual, expected, "theme {theme}");
        }
    }

    #[test]
    fn dark_mode_branch_matches_generated_mermaid_oracle_for_every_theme() {
        for &theme in SUPPORTED_THEME_NAMES {
            let mut config = MermaidConfig::from_value(json!({
                "theme": theme,
                "themeVariables": { "darkMode": true }
            }));
            apply_theme_defaults(&mut config).unwrap();
            let actual = config
                .as_value()
                .get("themeVariables")
                .and_then(Value::as_object)
                .unwrap();
            let expected = ThemeProgram::resolve(theme).dark_mode_snapshot();
            assert_eq!(actual, expected, "theme {theme}");
        }
    }

    #[test]
    fn generated_mermaid_oracle_locks_override_value_semantics() {
        fn value_at_path<'a>(root: &'a Value, path: &str) -> Option<&'a Value> {
            path.split('.')
                .try_fold(root, |value, key| value.as_object()?.get(key))
        }

        let mut mismatches = Vec::new();
        for case in &generated_theme_audit().oracle_cases {
            let id = case.get("id").and_then(Value::as_str).unwrap();
            let theme = case.get("theme").and_then(Value::as_str).unwrap();
            let overrides = case.get("overrides").cloned().unwrap();
            let expected_status = case.get("status").and_then(Value::as_str).unwrap();
            let mut config = MermaidConfig::from_value(json!({
                "theme": theme,
                "themeVariables": overrides
            }));
            let result = apply_theme_defaults(&mut config);

            if expected_status == "error" {
                if result.is_ok() {
                    mismatches.push(format!("{theme}/{id}: expected an error"));
                }
                continue;
            }
            if let Err(error) = result {
                mismatches.push(format!("{theme}/{id}: unexpected error: {error}"));
                continue;
            }

            let actual = config.as_value().get("themeVariables").unwrap();
            let selected = case.get("selected").and_then(Value::as_object).unwrap();
            for (path, expected) in selected {
                let state = expected.get("state").and_then(Value::as_str).unwrap();
                let actual_value = value_at_path(actual, path);
                match state {
                    "missing" if actual_value.is_some() => mismatches.push(format!(
                        "{theme}/{id}/{path}: expected missing, found {}",
                        actual_value.unwrap()
                    )),
                    "value" if actual_value != expected.get("value") => mismatches.push(format!(
                        "{theme}/{id}/{path}: expected {}, found {}",
                        expected.get("value").unwrap(),
                        actual_value
                            .map(Value::to_string)
                            .unwrap_or_else(|| "missing".to_string())
                    )),
                    "missing" | "value" => {}
                    other => mismatches
                        .push(format!("{theme}/{id}/{path}: unknown oracle state {other}")),
                }
            }
        }

        assert!(
            mismatches.is_empty(),
            "Mermaid theme oracle mismatches:\n{}",
            mismatches.join("\n")
        );
    }

    #[test]
    fn font_only_override_preserves_upstream_derived_palette_for_public_themes() {
        for &theme in SUPPORTED_THEME_NAMES {
            let mut cfg = MermaidConfig::from_value(json!({
                "theme": theme,
                "themeVariables": {
                    "fontFamily": "Inter, sans-serif"
                }
            }));
            apply_theme_defaults(&mut cfg).unwrap();

            let actual = cfg
                .as_value()
                .get("themeVariables")
                .and_then(Value::as_object)
                .unwrap();
            let expected = ThemeProgram::resolve(theme).default_snapshot();

            for key in [
                "cScale0",
                "cScale1",
                "cScalePeer0",
                "cScaleInv0",
                "cScaleLabel0",
            ] {
                assert_eq!(
                    actual.get(key),
                    expected.get(key),
                    "theme {theme} has derived palette drift at {key}"
                );
            }
            for (key, expected_value) in expected {
                if key == "fontFamily" {
                    continue;
                }
                assert_eq!(
                    actual.get(key),
                    Some(expected_value),
                    "theme {theme} has a non-font snapshot drift at {key}"
                );
            }
            assert_eq!(
                actual.get("fontFamily").and_then(Value::as_str),
                Some("Inter, sans-serif"),
                "theme {theme} should replay the explicit font override"
            );
        }
    }

    #[test]
    fn base_node_border_override_controls_inferred_gradient() {
        for (overrides, expected) in [
            (json!({ "nodeBorder": "#225577" }), false),
            (
                json!({ "nodeBorder": "#225577", "useGradient": true }),
                true,
            ),
            (json!({ "mainBkg": "#ffe1ef" }), true),
            (json!({ "useGradient": false }), false),
        ] {
            let mut config = MermaidConfig::from_value(json!({
                "theme": "base", "themeVariables": overrides
            }));
            apply_theme_defaults(&mut config).unwrap();
            assert_eq!(config.as_value()["themeVariables"]["useGradient"], expected);
        }
    }

    #[test]
    fn agentflow_stroke_follows_upstream_constructor_order() {
        for &theme in SUPPORTED_THEME_NAMES {
            let mut config = MermaidConfig::from_value(json!({
                "theme": theme,
                "themeVariables": { "secondaryBorderColor": "#123456" }
            }));
            apply_theme_defaults(&mut config).unwrap();
            let expected = if theme == "default" {
                json!("hsl(60, 60%, 83.5294117647%)")
            } else {
                json!("#123456")
            };
            assert_eq!(
                config.as_value()["themeVariables"]["flowContainerStroke"],
                expected,
                "{theme}"
            );
        }
    }

    #[test]
    fn extended_theme_bands_follow_mermaid_12_background_derivations() {
        for theme in ["neo", "redux", "redux-color"] {
            for (background, expected) in [
                ("#000000", "hsl(0, 0%, 4%)"),
                ("#ffffff", "hsl(0, 0%, 96%)"),
            ] {
                let mut config = MermaidConfig::from_value(json!({
                    "theme": theme, "themeVariables": { "background": background }
                }));
                apply_theme_defaults(&mut config).unwrap();
                assert_eq!(
                    config.as_value()["themeVariables"]["rectBkgColor"],
                    expected,
                    "{theme}"
                );
            }
        }
        for theme in ["neo-dark", "redux-dark", "redux-dark-color"] {
            let mut config = MermaidConfig::from_value(json!({
                "theme": theme, "themeVariables": { "mainBkg": "#101112" }
            }));
            apply_theme_defaults(&mut config).unwrap();
            let variables = &config.as_value()["themeVariables"];
            assert_eq!(
                variables["secondBkg"], "hsl(210, 5.8823529412%, 22.6666666667%)",
                "{theme}"
            );
            assert_eq!(
                variables["doneTaskBkgColor"], variables["secondBkg"],
                "{theme}"
            );
        }
    }

    #[test]
    fn redux_color_categorical_overrides_feed_pie_gantt_and_journey() {
        for theme in ["redux-color", "redux-dark-color"] {
            let mut config = MermaidConfig::from_value(json!({
                "theme": theme,
                "themeVariables": {
                    "primaryColor": "#123456", "cScale0": "#234567", "cScale1": "#345678",
                    "borderColorArray": ["#456789"], "bkgColorArray": ["#567890"],
                    "background": "#000000", "pie2": "#678901"
                }
            }));
            apply_theme_defaults(&mut config).unwrap();
            let variables = &config.as_value()["themeVariables"];
            assert_eq!(variables["pie1"], "#234567", "{theme}");
            assert_eq!(variables["pie2"], "#678901", "{theme}");
            assert_eq!(variables["sectionBkgColor"], "#234567", "{theme}");
            assert_eq!(variables["sectionBkgColor2"], "#345678", "{theme}");
            assert_eq!(variables["venn8"], "#456789", "{theme}");
            if theme == "redux-color" {
                assert_eq!(variables["fillType0"], "#567890");
                assert!(variables.get("fillType1").is_none());
            } else {
                assert_eq!(variables["fillType0"], "#701a75");
                assert_eq!(variables["altSectionBkgColor"], "#000000");
            }
        }
    }

    #[test]
    fn redux_categorical_falsy_override_replays_after_pie_derivation() {
        for theme in ["redux-color", "redux-dark-color"] {
            for explicit_scale in [json!(false), json!(0)] {
                let mut config = MermaidConfig::from_value(json!({
                    "theme": theme,
                    "themeVariables": { "cScale2": explicit_scale }
                }));
                apply_theme_defaults(&mut config).unwrap();
                let variables = &config.as_value()["themeVariables"];
                assert_eq!(variables["cScale2"], explicit_scale, "{theme}");
                assert_eq!(variables["pie3"], "#ffb86a", "{theme}");
            }
        }
    }

    #[test]
    fn agentflow_falsy_border_replays_after_container_stroke_derivation() {
        for (theme, expected_stroke) in [
            ("redux-color", "hsl(-120, 0%, 70%)"),
            ("redux-dark-color", "hsl(180, 0%, 18.3529411765%)"),
        ] {
            for explicit_border in [json!(false), json!(0)] {
                let mut config = MermaidConfig::from_value(json!({
                    "theme": theme,
                    "themeVariables": { "secondaryBorderColor": explicit_border }
                }));
                apply_theme_defaults(&mut config).unwrap();
                let variables = &config.as_value()["themeVariables"];
                assert_eq!(
                    variables["secondaryBorderColor"], explicit_border,
                    "{theme}"
                );
                assert_eq!(variables["flowContainerStroke"], expected_stroke, "{theme}");
            }
        }
    }

    #[test]
    fn explicit_scale_override_recomputes_peer_and_inverse_from_override_stage() {
        // Oracle values from Mermaid 11.16.1 `getThemeVariables()` with the same overrides.
        let cases = [
            (
                "default",
                "hsl(240, 100%, 61.2745098039%)",
                "hsl(60, 100%, 86.2745098039%)",
            ),
            ("dark", "hsl(210, 68%, 90.3921568627%)", "#543210"),
            (
                "forest",
                "hsl(210, 68%, 45.3921568627%)",
                "hsl(30, 68%, 70.3921568627%)",
            ),
            ("neutral", "hsl(210, 68%, 70.3921568627%)", "#543210"),
            (
                "base",
                "hsl(210, 68%, 45.3921568627%)",
                "rgb(191.1000000002, 113.7500000001, 36.4)",
            ),
        ];

        for (theme, expected_peer, expected_inverse) in cases {
            let mut cfg = MermaidConfig::from_value(json!({
                "theme": theme,
                "themeVariables": {
                    "primaryColor": "#123456",
                    "cScale0": "#abcdef"
                }
            }));
            apply_theme_defaults(&mut cfg).unwrap();
            let actual = cfg
                .as_value()
                .get("themeVariables")
                .and_then(Value::as_object)
                .unwrap();

            assert_eq!(
                actual.get("cScale0").and_then(Value::as_str),
                Some("#abcdef"),
                "theme {theme} must replay explicit cScale0"
            );
            assert_eq!(
                actual.get("cScalePeer0").and_then(Value::as_str),
                Some(expected_peer),
                "theme {theme} must derive cScalePeer0 from the override stage"
            );
            assert_eq!(
                actual.get("cScaleInv0").and_then(Value::as_str),
                Some(expected_inverse),
                "theme {theme} must derive cScaleInv0 from the override stage"
            );
        }
    }

    #[test]
    fn extended_theme_scale_override_replays_after_source_ordered_derivations() {
        // Oracle: Mermaid 11.16.1 getThemeVariables({ cScale0: '#abcdef' }). Theme names do not
        // imply darkMode; only an explicit darkMode value changes the scale transform branch.
        let cases = [
            (
                "neo",
                "hsl(210, 68%, 45.3921568627%)",
                "rgb(191.1000000002, 113.7500000001, 36.4)",
                "#333",
            ),
            (
                "neo-dark",
                "hsl(210, 68%, 45.3921568627%)",
                "rgb(191.1000000002, 113.7500000001, 36.4)",
                "#e0dfdf",
            ),
            (
                "redux",
                "hsl(0, 0%, 65%)",
                "rgb(63.75, 63.75, 63.75)",
                "#28253D",
            ),
            (
                "redux-dark",
                "hsl(210, 68%, 45.3921568627%)",
                "rgb(191.1000000002, 113.7500000001, 36.4)",
                "#e0dfdf",
            ),
            (
                "redux-color",
                "hsl(210, 68%, 70.3921568627%)",
                "#543210",
                "#28253D",
            ),
            (
                "redux-dark-color",
                "hsl(210, 68%, 70.3921568627%)",
                "#543210",
                "hsl(210, 68%, 5.3921568627%)",
            ),
        ];

        for (theme, expected_peer, expected_inverse, expected_label) in cases {
            let mut config = MermaidConfig::from_value(json!({
                "theme": theme,
                "themeVariables": { "cScale0": "#abcdef" }
            }));
            apply_theme_defaults(&mut config).unwrap();
            let variables = config
                .as_value()
                .get("themeVariables")
                .and_then(Value::as_object)
                .unwrap();

            for (key, expected) in [
                ("cScale0", "#abcdef"),
                ("cScalePeer0", expected_peer),
                ("cScaleInv0", expected_inverse),
                ("cScaleLabel0", expected_label),
            ] {
                assert_eq!(
                    variables.get(key).and_then(Value::as_str),
                    Some(expected),
                    "theme {theme} has incorrect {key}"
                );
            }
        }
    }

    #[test]
    fn primary_color_override_follows_each_upstream_theme_scale_contract() {
        // Oracle values from Mermaid 11.16.1 `getThemeVariables()` with the same overrides.
        let cases = [
            (
                "default",
                "hsl(240, 100%, 76.2745098039%)",
                "hsl(240, 100%, 61.2745098039%)",
                "hsl(60, 100%, 86.2745098039%)",
            ),
            (
                "dark",
                "#123456",
                "hsl(210, 65.3846153846%, 30.3921568627%)",
                "#edcba9",
            ),
            (
                "forest",
                "hsl(210, 65.3846153846%, 10.3921568627%)",
                "hsl(210, 65.3846153846%, 0%)",
                "hsl(30, 65.3846153846%, 10.3921568627%)",
            ),
            ("neutral", "#555", "hsl(0, 0%, 23.3333333333%)", "#aaaaaa"),
            (
                "base",
                "hsl(210, 65.3846153846%, 0%)",
                "hsl(210, 65.3846153846%, 0%)",
                "#ffffff",
            ),
        ];

        for (theme, expected_scale, expected_peer, expected_inverse) in cases {
            let mut cfg = MermaidConfig::from_value(json!({
                "theme": theme,
                "themeVariables": {
                    "primaryColor": "#123456"
                }
            }));
            apply_theme_defaults(&mut cfg).unwrap();
            let actual = cfg
                .as_value()
                .get("themeVariables")
                .and_then(Value::as_object)
                .unwrap();

            for (key, expected) in [
                ("cScale0", expected_scale),
                ("cScalePeer0", expected_peer),
                ("cScaleInv0", expected_inverse),
            ] {
                assert_eq!(
                    actual.get(key).and_then(Value::as_str),
                    Some(expected),
                    "theme {theme} has incorrect {key} after primaryColor override"
                );
            }
        }
    }

    #[test]
    fn quadrant_primary_override_matches_mermaid_11_16_theme_lifecycles() {
        // Oracle: Mermaid 11.16.1 `mermaid.initialize()` + `mermaidAPI.getConfig()`.
        let cases = [
            ("default", "#ECECFF", "#f1f1ff", "hsl(240, 100%, NaN%)"),
            (
                "dark",
                "#123456",
                "#17395b",
                "hsl(210, 65.3846153846%, NaN%)",
            ),
            (
                "forest",
                "#123456",
                "#17395b",
                "hsl(210, 65.3846153846%, NaN%)",
            ),
            (
                "neutral",
                "#123456",
                "#17395b",
                "hsl(210, 65.3846153846%, NaN%)",
            ),
            (
                "base",
                "#123456",
                "#17395b",
                "hsl(210, 65.3846153846%, NaN%)",
            ),
            ("neo", "#ECECFE", "#f1f1ff", "hsl(240, 90%, NaN%)"),
            (
                "neo-dark",
                "#123456",
                "#17395b",
                "hsl(210, 65.3846153846%, NaN%)",
            ),
            ("redux", "#ECECFE", "#f1f1ff", "hsl(240, 90%, NaN%)"),
            (
                "redux-dark",
                "#123456",
                "#17395b",
                "hsl(210, 65.3846153846%, NaN%)",
            ),
            ("redux-color", "#ECECFE", "#f1f1ff", "hsl(240, 90%, NaN%)"),
            (
                "redux-dark-color",
                "#123456",
                "#17395b",
                "hsl(210, 65.3846153846%, NaN%)",
            ),
        ];

        for (theme, expected_q1, expected_q2, expected_point) in cases {
            let mut config = MermaidConfig::from_value(json!({
                "theme": theme,
                "themeVariables": { "primaryColor": "#123456" }
            }));
            apply_theme_defaults(&mut config).unwrap();
            let variables = config
                .as_value()
                .get("themeVariables")
                .and_then(Value::as_object)
                .unwrap();

            for (key, expected) in [
                ("quadrant1Fill", expected_q1),
                ("quadrant2Fill", expected_q2),
                ("quadrantPointFill", expected_point),
            ] {
                assert_eq!(
                    variables.get(key).and_then(Value::as_str),
                    Some(expected),
                    "theme {theme} has incorrect {key}"
                );
            }
        }
    }

    #[test]
    fn quadrant_partial_and_text_overrides_match_mermaid_11_16_replay_order() {
        let mut config = MermaidConfig::from_value(json!({
            "theme": "base",
            "themeVariables": {
                "primaryTextColor": "#123456",
                "quadrant1Fill": "rgba(18, 52, 86, 0.5)",
                "quadrant2Fill": "#abcdef"
            }
        }));
        apply_theme_defaults(&mut config).unwrap();
        let variables = config
            .as_value()
            .get("themeVariables")
            .and_then(Value::as_object)
            .unwrap();

        for (key, expected) in [
            ("quadrant1Fill", "rgba(18, 52, 86, 0.5)"),
            ("quadrant2Fill", "#abcdef"),
            ("quadrant3Fill", "#fffee7"),
            ("quadrant1TextFill", "#123456"),
            ("quadrant2TextFill", "#0d2f51"),
            ("quadrantPointFill", "hsla(210, 65.3846153846%, NaN%, 0.5)"),
        ] {
            assert_eq!(variables.get(key).and_then(Value::as_str), Some(expected));
        }

        let mut default_config = MermaidConfig::from_value(json!({
            "theme": "default",
            "themeVariables": { "primaryTextColor": "#123456" }
        }));
        apply_theme_defaults(&mut default_config).unwrap();
        assert_eq!(
            default_config.get_str("themeVariables.quadrant1TextFill"),
            Some("#131300")
        );
    }

    #[test]
    fn quadrant_accepts_khroma_named_rgb_and_alpha_colors() {
        // Oracle: Mermaid 11.16.1 `mermaid.initialize()` + `mermaidAPI.getConfig()`.
        let cases = [
            (
                "rebeccapurple",
                "#6b389e",
                "hsl(270, 50%, NaN%)",
                "hsl(270, 10%, 30%)",
            ),
            (
                "rgb(18, 52, 86)",
                "#17395b",
                "hsl(210, 65.3846153846%, NaN%)",
                "hsl(210, 25.3846153846%, 10.3921568627%)",
            ),
            (
                "rgba(18, 52, 86, 0.5)",
                "rgba(23, 57, 91, 0.5)",
                "hsla(210, 65.3846153846%, NaN%, 0.5)",
                "hsla(210, 25.3846153846%, 10.3921568627%, 0.5)",
            ),
        ];

        for (primary, expected_q2, expected_point, expected_border) in cases {
            let mut config = MermaidConfig::from_value(json!({
                "theme": "base",
                "themeVariables": { "primaryColor": primary }
            }));
            apply_theme_defaults(&mut config).unwrap();
            let variables = config
                .as_value()
                .get("themeVariables")
                .and_then(Value::as_object)
                .unwrap();

            for (key, expected) in [
                ("quadrant2Fill", expected_q2),
                ("quadrantPointFill", expected_point),
                ("quadrantInternalBorderStrokeFill", expected_border),
            ] {
                assert_eq!(
                    variables.get(key).and_then(Value::as_str),
                    Some(expected),
                    "primaryColor {primary} has incorrect {key}"
                );
            }
        }
    }

    #[test]
    fn invalid_color_timing_matches_mermaid_11_16_initialize_matrix() {
        // Oracle: Mermaid 11.16.1 `mermaid.initialize()` + `mermaidAPI.getConfig()` using
        // `not-a-color` for each field independently. Fields absent from `errors` are deliberate
        // pass-through values at theme-calculation time and must not be validated early.
        let keys = [
            "primaryColor",
            "secondaryColor",
            "tertiaryColor",
            "background",
            "primaryBorderColor",
            "secondaryBorderColor",
            "tertiaryBorderColor",
            "border1",
            "cScale0",
            "cScale1",
            "git0",
            "git1",
            "gitInv0",
            "pie1",
            "pie3",
            "quadrant1Fill",
        ];
        let cases: [(&str, &[&str]); 11] = [
            (
                "default",
                &[
                    "primaryColor",
                    "secondaryColor",
                    "cScale0",
                    "cScale1",
                    "git0",
                    "git1",
                    "quadrant1Fill",
                ],
            ),
            (
                "dark",
                &[
                    "primaryColor",
                    "secondaryColor",
                    "background",
                    "cScale0",
                    "cScale1",
                    "quadrant1Fill",
                ],
            ),
            (
                "forest",
                &[
                    "primaryColor",
                    "secondaryColor",
                    "tertiaryColor",
                    "cScale0",
                    "cScale1",
                    "git0",
                    "git1",
                    "quadrant1Fill",
                ],
            ),
            (
                "neutral",
                &[
                    "primaryColor",
                    "secondaryColor",
                    "border1",
                    "cScale0",
                    "cScale1",
                    "quadrant1Fill",
                ],
            ),
            (
                "base",
                &[
                    "primaryColor",
                    "secondaryColor",
                    "tertiaryColor",
                    "background",
                    "cScale0",
                    "cScale1",
                    "git0",
                    "git1",
                    "quadrant1Fill",
                ],
            ),
            (
                "neo",
                &[
                    "primaryColor",
                    "secondaryColor",
                    "tertiaryColor",
                    "background",
                    "cScale0",
                    "cScale1",
                    "git0",
                    "git1",
                    "quadrant1Fill",
                ],
            ),
            (
                "neo-dark",
                &[
                    "primaryColor",
                    "secondaryColor",
                    "tertiaryColor",
                    "background",
                    "cScale0",
                    "cScale1",
                    "git0",
                    "git1",
                    "quadrant1Fill",
                ],
            ),
            (
                "redux",
                &[
                    "primaryColor",
                    "secondaryColor",
                    "tertiaryColor",
                    "background",
                    "git0",
                    "git1",
                    "quadrant1Fill",
                ],
            ),
            (
                "redux-dark",
                &[
                    "primaryColor",
                    "secondaryColor",
                    "tertiaryColor",
                    "background",
                    "cScale0",
                    "cScale1",
                    "git0",
                    "git1",
                    "quadrant1Fill",
                ],
            ),
            (
                "redux-color",
                &[
                    "primaryColor",
                    "secondaryColor",
                    "tertiaryColor",
                    "background",
                    "cScale0",
                    "cScale1",
                    "git0",
                    "git1",
                    "quadrant1Fill",
                ],
            ),
            (
                "redux-dark-color",
                &[
                    "primaryColor",
                    "secondaryColor",
                    "tertiaryColor",
                    "background",
                    "cScale0",
                    "cScale1",
                    "git0",
                    "git1",
                    "quadrant1Fill",
                ],
            ),
        ];

        let mut mismatches = Vec::new();
        for (theme, error_keys) in cases {
            for key in keys {
                let mut config = MermaidConfig::from_value(json!({
                    "theme": theme,
                    "themeVariables": { (key): "not-a-color" }
                }));
                let actual_error = apply_theme_defaults(&mut config).is_err();
                let expected_error = error_keys.contains(&key);
                if actual_error != expected_error {
                    mismatches.push(format!(
                        "{theme}.{key}: expected error={expected_error}, actual={actual_error}"
                    ));
                }
            }
        }
        assert!(
            mismatches.is_empty(),
            "invalid-color timing mismatches:\n{}",
            mismatches.join("\n")
        );
    }

    #[test]
    fn invalid_theme_colors_fail_at_direct_and_site_config_operation_boundaries() {
        let mut config = MermaidConfig::from_value(json!({
            "theme": "base",
            "themeVariables": { "primaryColor": "not-a-color" }
        }));
        assert!(matches!(
            apply_theme_defaults(&mut config),
            Err(ColorError::UnsupportedFormat { .. })
        ));

        let engine = crate::Engine::new().with_site_config(MermaidConfig::from_value(json!({
            "theme": "base",
            "themeVariables": { "primaryColor": "not-a-color" }
        })));
        assert!(matches!(
            engine.parse_metadata_sync("flowchart TD\n  A"),
            Err(crate::Error::ThemeColor(
                ColorError::UnsupportedFormat { .. }
            ))
        ));
    }

    #[test]
    fn invalid_frontmatter_theme_color_fails_at_parse_operation_boundary() {
        let engine = crate::Engine::new().with_site_config(MermaidConfig::from_value(json!({
            "secure": [
                "secure",
                "securityLevel",
                "startOnLoad",
                "maxTextSize",
                "suppressErrorRendering",
                "maxEdges"
            ]
        })));
        let source = r#"---
config:
  theme: base
  themeVariables:
    primaryColor: not-a-color
---
flowchart TD
  A
"#;

        assert!(matches!(
            engine.parse_metadata_sync(source),
            Err(crate::Error::ThemeColor(
                ColorError::UnsupportedFormat { .. }
            ))
        ));
    }

    #[test]
    fn theme_resolution_records_stage_and_final_value_provenance() {
        let explicit = json!({
            "fontFamily": "Inter, sans-serif",
            "cScale0": "#abcdef"
        })
        .as_object()
        .unwrap()
        .clone();
        let calculated = json!({
            "fontFamily": "Inter, sans-serif",
            "cScale0": "hsl(210, 68%, 70.3921568627%)",
            "cScalePeer0": "hsl(210, 68%, 55.3921568627%)"
        })
        .as_object()
        .unwrap()
        .clone();

        let resolution = ThemeResolution::new("default", explicit, calculated).unwrap();

        assert_eq!(
            resolution.default_snapshot.stage,
            ThemeResolutionStage::DefaultSnapshot
        );
        assert_eq!(
            resolution.overrides_applied.stage,
            ThemeResolutionStage::OverridesApplied
        );
        assert_eq!(
            resolution.calculated.stage,
            ThemeResolutionStage::Calculated
        );
        assert_eq!(
            resolution.explicit_replay.stage,
            ThemeResolutionStage::ExplicitReplay
        );
        assert_eq!(
            resolution.explicit_replay.origins.get("fontFamily"),
            Some(&ThemeValueOrigin::ExplicitOverride)
        );
        assert_eq!(
            resolution.explicit_replay.origins.get("cScale0"),
            Some(&ThemeValueOrigin::ExplicitOverride)
        );
        assert_eq!(
            resolution.explicit_replay.origins.get("cScalePeer0"),
            Some(&ThemeValueOrigin::DefaultSnapshot)
        );
    }

    #[test]
    fn default_theme_populates_mermaid_theme_variables() {
        let mut cfg = MermaidConfig::from_value(json!({
            "theme": "default"
        }));
        apply_theme_defaults(&mut cfg).unwrap();

        let tv = cfg
            .as_value()
            .get("themeVariables")
            .and_then(|v| v.as_object())
            .unwrap();

        assert_eq!(tv.get("background").and_then(|v| v.as_str()), Some("white"));
        assert_eq!(
            tv.get("primaryColor").and_then(|v| v.as_str()),
            Some("#ECECFF")
        );
        assert_eq!(
            tv.get("secondaryColor").and_then(|v| v.as_str()),
            Some("#ffffde")
        );
        assert_eq!(tv.get("pie1").and_then(|v| v.as_str()), Some("#ECECFF"));
        assert_eq!(tv.get("pie2").and_then(|v| v.as_str()), Some("#ffffde"));
        assert_eq!(tv.get("mainBkg").and_then(|v| v.as_str()), Some("#ECECFF"));
        assert_eq!(
            tv.get("nodeBorder").and_then(|v| v.as_str()),
            Some("#9370DB")
        );
        assert_eq!(
            tv.get("edgeLabelBackground").and_then(|v| v.as_str()),
            Some("rgba(232,232,232, 0.8)")
        );
        assert_eq!(
            tv.get("classText").and_then(|v| v.as_str()),
            Some("#131300")
        );
        assert_eq!(
            tv.get("noteTextColor").and_then(|v| v.as_str()),
            Some("black")
        );
        assert_eq!(tv.get("useGradient").and_then(|v| v.as_bool()), Some(false));
        assert_eq!(
            tv.get("gradientStart").and_then(|v| v.as_str()),
            Some("hsl(240, 60%, 86.2745098039%)")
        );

        let xy = tv.get("xyChart").and_then(|v| v.as_object()).unwrap();
        assert_eq!(
            xy.get("backgroundColor").and_then(|v| v.as_str()),
            Some("white")
        );
        assert_eq!(
            xy.get("dataLabelColor").and_then(|v| v.as_str()),
            Some("#131300")
        );
    }

    #[test]
    fn default_theme_preserves_user_overrides_after_derivation() {
        let mut cfg = MermaidConfig::from_value(json!({
            "theme": "default",
            "themeVariables": {
                "primaryColor": "#111111",
                "mainBkg": "#101010",
                "classText": "#abcdef",
                "xyChart": {
                    "titleColor": "red"
                }
            }
        }));
        apply_theme_defaults(&mut cfg).unwrap();

        let tv = cfg
            .as_value()
            .get("themeVariables")
            .and_then(|v| v.as_object())
            .unwrap();

        assert_eq!(
            tv.get("primaryColor").and_then(|v| v.as_str()),
            Some("#111111")
        );
        assert_eq!(tv.get("pie1").and_then(|v| v.as_str()), Some("#ECECFF"));
        assert_eq!(tv.get("pie2").and_then(|v| v.as_str()), Some("#ffffde"));
        assert_eq!(tv.get("mainBkg").and_then(|v| v.as_str()), Some("#101010"));
        assert_eq!(tv.get("nodeBkg").and_then(|v| v.as_str()), Some("#101010"));
        assert_eq!(
            tv.get("classText").and_then(|v| v.as_str()),
            Some("#abcdef")
        );
        assert_eq!(
            tv.get("primaryTextColor").and_then(|v| v.as_str()),
            Some("#131300")
        );

        let xy = tv.get("xyChart").and_then(|v| v.as_object()).unwrap();
        assert_eq!(xy.get("titleColor").and_then(|v| v.as_str()), Some("red"));
        // applyOverride keeps updateColors' generated keys when the user supplies only a title.
        assert_eq!(xy.get("dataLabelColor"), tv.get("primaryTextColor"));
        assert!(xy.get("plotColorPalette").is_some_and(Value::is_string));
    }

    #[test]
    fn default_theme_merges_unrelated_theme_variable_overrides_without_hsl_rewriting_pie_base() {
        let mut cfg = MermaidConfig::from_value(json!({
            "theme": "default",
            "themeVariables": {
                "pieOuterStrokeWidth": "5px"
            }
        }));
        apply_theme_defaults(&mut cfg).unwrap();

        let tv = cfg
            .as_value()
            .get("themeVariables")
            .and_then(|v| v.as_object())
            .unwrap();

        assert_eq!(tv.get("pie1").and_then(|v| v.as_str()), Some("#ECECFF"));
        assert_eq!(tv.get("pie2").and_then(|v| v.as_str()), Some("#ffffde"));
        assert_eq!(
            tv.get("pieOuterStrokeWidth").and_then(|v| v.as_str()),
            Some("5px")
        );
    }

    #[test]
    fn unknown_theme_falls_back_to_default_theme_variables() {
        let mut cfg = MermaidConfig::from_value(json!({
            "theme": "unknown"
        }));
        apply_theme_defaults(&mut cfg).unwrap();

        let tv = cfg
            .as_value()
            .get("themeVariables")
            .and_then(|v| v.as_object())
            .unwrap();

        assert_eq!(
            tv.get("primaryColor").and_then(|v| v.as_str()),
            Some("#ECECFF")
        );
        assert_eq!(
            tv.get("classText").and_then(|v| v.as_str()),
            Some("#131300")
        );
    }

    #[test]
    fn mermaid_11_16_extended_theme_names_use_their_snapshots() {
        let cases = [
            ("neo", "#cccccc", "#000000"),
            ("neo-dark", "#1f2020", "#ccc"),
            ("redux", "#cccccc", "#28253D"),
            ("redux-dark", "#1f2020", "#FFFFFF"),
            ("redux-color", "#cccccc", "#28253D"),
            ("redux-dark-color", "#1f2020", "#FFFFFF"),
        ];

        for (theme, primary, node_border) in cases {
            let mut cfg = MermaidConfig::from_value(json!({
                "theme": theme
            }));
            apply_theme_defaults(&mut cfg).unwrap();

            let tv = cfg
                .as_value()
                .get("themeVariables")
                .and_then(|v| v.as_object())
                .unwrap();

            assert_eq!(
                tv.get("primaryColor").and_then(|v| v.as_str()),
                Some(primary),
                "theme {theme}"
            );
            assert_eq!(
                tv.get("nodeBorder").and_then(|v| v.as_str()),
                Some(node_border),
                "theme {theme}"
            );
        }
    }

    #[test]
    fn extended_theme_preserves_explicit_theme_variable_overrides() {
        let mut cfg = MermaidConfig::from_value(json!({
            "theme": "redux",
            "themeVariables": {
                "primaryColor": "#123456",
                "nodeBorder": "#abcdef"
            }
        }));
        apply_theme_defaults(&mut cfg).unwrap();

        let tv = cfg
            .as_value()
            .get("themeVariables")
            .and_then(|v| v.as_object())
            .unwrap();

        assert_eq!(
            tv.get("primaryColor").and_then(|v| v.as_str()),
            Some("#123456")
        );
        assert_eq!(
            tv.get("nodeBorder").and_then(|v| v.as_str()),
            Some("#abcdef")
        );
        assert_eq!(
            tv.get("fontFamily").and_then(|v| v.as_str()),
            Some("\"Recursive Variable\", arial, sans-serif")
        );
    }

    #[test]
    fn extended_theme_recomputes_visible_derivations_from_base_overrides() {
        let mut cfg = MermaidConfig::from_value(json!({
            "theme": "redux",
            "themeVariables": {
                "primaryColor": "#123456"
            }
        }));
        apply_theme_defaults(&mut cfg).unwrap();

        let tv = cfg
            .as_value()
            .get("themeVariables")
            .and_then(|v| v.as_object())
            .unwrap();

        assert_eq!(
            tv.get("primaryColor").and_then(|v| v.as_str()),
            Some("#123456")
        );
        assert_eq!(tv.get("nodeBkg").and_then(|v| v.as_str()), Some("#123456"));
        assert_eq!(
            tv.get("secondaryColor").and_then(|v| v.as_str()),
            Some("hsl(90, 65.3846153846%, 20.3921568627%)")
        );
        assert_eq!(
            tv.get("edgeLabelBackground").and_then(|v| v.as_str()),
            Some("hsl(90, 65.3846153846%, 20.3921568627%)")
        );
        assert_eq!(
            tv.get("tagLabelBackground").and_then(|v| v.as_str()),
            Some("#123456")
        );
        assert_eq!(
            tv.get("fontFamily").and_then(|v| v.as_str()),
            Some("\"Recursive Variable\", arial, sans-serif")
        );
        assert_eq!(
            tv.get("git0").and_then(|v| v.as_str()),
            Some("hsl(240, 90%, 71.0784313725%)")
        );
    }

    #[test]
    fn extended_theme_explicit_derived_overrides_still_win() {
        let mut cfg = MermaidConfig::from_value(json!({
            "theme": "redux",
            "themeVariables": {
                "primaryColor": "#123456",
                "nodeBkg": "#abcdef",
                "edgeLabelBackground": "#fedcba"
            }
        }));
        apply_theme_defaults(&mut cfg).unwrap();

        let tv = cfg
            .as_value()
            .get("themeVariables")
            .and_then(|v| v.as_object())
            .unwrap();

        assert_eq!(tv.get("nodeBkg").and_then(|v| v.as_str()), Some("#abcdef"));
        assert_eq!(
            tv.get("edgeLabelBackground").and_then(|v| v.as_str()),
            Some("#fedcba")
        );
        assert_eq!(
            tv.get("secondaryColor").and_then(|v| v.as_str()),
            Some("hsl(90, 65.3846153846%, 20.3921568627%)")
        );
    }

    #[test]
    fn extended_theme_recomputes_background_and_main_background_derivations() {
        let mut cfg = MermaidConfig::from_value(json!({
            "theme": "redux",
            "themeVariables": {
                "background": "#010203",
                "mainBkg": "#101112"
            }
        }));
        apply_theme_defaults(&mut cfg).unwrap();

        let tv = cfg
            .as_value()
            .get("themeVariables")
            .and_then(|v| v.as_object())
            .unwrap();

        for key in [
            "lineColor",
            "arrowheadColor",
            "defaultLinkColor",
            "archEdgeColor",
            "archEdgeArrowColor",
            "relationColor",
            "transitionColor",
            "specialStateColor",
        ] {
            assert_eq!(
                tv.get(key).and_then(|v| v.as_str()),
                Some("#fefdfc"),
                "key {key}"
            );
        }

        for key in [
            "actorBkg",
            "labelBoxBkgColor",
            "personBkg",
            "stateBkg",
            "labelBackgroundColor",
        ] {
            assert_eq!(
                tv.get(key).and_then(|v| v.as_str()),
                Some("#101112"),
                "key {key}"
            );
        }
        assert_eq!(tv.get("nodeBkg").and_then(|v| v.as_str()), Some("#cccccc"));
    }

    #[test]
    fn dark_extended_theme_recomputes_primary_visible_derivations() {
        let mut cfg = MermaidConfig::from_value(json!({
            "theme": "redux-dark",
            "themeVariables": {
                "primaryColor": "#123456"
            }
        }));
        apply_theme_defaults(&mut cfg).unwrap();

        let tv = cfg
            .as_value()
            .get("themeVariables")
            .and_then(|v| v.as_object())
            .unwrap();

        for key in ["requirementBackground", "pie1", "quadrant1Fill"] {
            assert_eq!(
                tv.get(key).and_then(|v| v.as_str()),
                Some("#123456"),
                "key {key}"
            );
        }
        assert_eq!(
            tv.get("git0").and_then(|v| v.as_str()),
            Some("hsl(210, 65.3846153846%, 0%)")
        );
        assert_eq!(
            tv.get("git1").and_then(|v| v.as_str()),
            Some("hsl(180, 1.5873015873%, 3.3529411765%)")
        );
        assert_eq!(
            tv.get("git3").and_then(|v| v.as_str()),
            Some("hsl(180, 65.3846153846%, 0%)")
        );
        assert_eq!(tv.get("gitInv0").and_then(|v| v.as_str()), Some("#ffffff"));
        assert_eq!(
            tv.get("gitInv1").and_then(|v| v.as_str()),
            Some("rgb(246.5857142856, 246.3142857142, 246.3142857142)")
        );
    }

    #[test]
    fn extended_theme_explicit_git_color_derives_git_inverse_unless_explicit() {
        let mut cfg = MermaidConfig::from_value(json!({
            "theme": "redux",
            "themeVariables": {
                "git0": "#000000",
                "git1": "#111111",
                "gitInv1": "#222222"
            }
        }));
        apply_theme_defaults(&mut cfg).unwrap();

        let tv = cfg
            .as_value()
            .get("themeVariables")
            .and_then(|v| v.as_object())
            .unwrap();

        assert_eq!(tv.get("git0").and_then(|v| v.as_str()), Some("#000000"));
        assert_eq!(tv.get("gitInv0").and_then(|v| v.as_str()), Some("#ffffff"));
        assert_eq!(tv.get("git1").and_then(|v| v.as_str()), Some("#111111"));
        assert_eq!(tv.get("gitInv1").and_then(|v| v.as_str()), Some("#222222"));
    }

    #[test]
    fn base_theme_derivation_matches_upstream_fixture_values() {
        let mut cfg = MermaidConfig::from_value(json!({
            "theme": "base",
            "themeVariables": {
                "primaryColor": "#411d4e",
                "titleColor": "white",
                "darkMode": true
            }
        }));
        apply_theme_defaults(&mut cfg).unwrap();

        let tv = cfg
            .as_value()
            .get("themeVariables")
            .and_then(|v| v.as_object())
            .unwrap();

        assert_eq!(tv.get("textColor").and_then(|v| v.as_str()), Some("#eee"));
        assert_eq!(
            tv.get("lineColor").and_then(|v| v.as_str()),
            Some("#0b0b0b")
        );
        assert_eq!(
            tv.get("nodeBorder").and_then(|v| v.as_str()),
            Some("hsl(284.0816326531, 5.7943925234%, 30.9803921569%)")
        );
        assert_eq!(
            tv.get("secondaryBorderColor").and_then(|v| v.as_str()),
            Some("hsl(164.0816326531, 5.7943925234%, 30.9803921569%)")
        );
        assert_eq!(tv.get("useGradient").and_then(|v| v.as_bool()), Some(true));
        assert_eq!(
            tv.get("gradientStart").and_then(|v| v.as_str()),
            Some("hsl(284.0816326531, 5.7943925234%, 30.9803921569%)")
        );
        assert_eq!(
            tv.get("gradientStop").and_then(|v| v.as_str()),
            Some("hsl(164.0816326531, 5.7943925234%, 30.9803921569%)")
        );
        assert_eq!(tv.get("mainBkg").and_then(|v| v.as_str()), Some("#411d4e"));
        assert_eq!(
            tv.get("clusterBkg").and_then(|v| v.as_str()),
            Some("hsl(104.0816326531, 45.7943925234%, 25.9803921569%)")
        );
        assert_eq!(
            tv.get("clusterBorder").and_then(|v| v.as_str()),
            Some("hsl(104.0816326531, 5.7943925234%, 35.9803921569%)")
        );
        assert_eq!(
            tv.get("edgeLabelBackground").and_then(|v| v.as_str()),
            Some("hsl(164.0816326531, 45.7943925234%, 0%)")
        );
        assert_eq!(
            tv.get("errorBkgColor").and_then(|v| v.as_str()),
            Some("hsl(104.0816326531, 45.7943925234%, 25.9803921569%)")
        );
        assert_eq!(
            tv.get("errorTextColor").and_then(|v| v.as_str()),
            Some("rgb(202.9906542056, 158.4112149531, 219.0887850467)")
        );
        assert_eq!(tv.get("titleColor").and_then(|v| v.as_str()), Some("white"));
    }

    #[test]
    fn forest_theme_derives_cscale_palette_like_upstream() {
        let mut cfg = MermaidConfig::from_value(json!({
            "theme": "forest"
        }));
        apply_theme_defaults(&mut cfg).unwrap();

        let tv = cfg
            .as_value()
            .get("themeVariables")
            .and_then(|v| v.as_object())
            .unwrap();

        assert_eq!(
            tv.get("cScale0").and_then(|v| v.as_str()),
            Some("hsl(78.1578947368, 58.4615384615%, 64.5098039216%)")
        );
        assert_eq!(
            tv.get("cScalePeer0").and_then(|v| v.as_str()),
            Some("hsl(78.1578947368, 58.4615384615%, 39.5098039216%)")
        );
        assert_eq!(
            tv.get("cScalePeer1").and_then(|v| v.as_str()),
            Some("hsl(98.961038961, 100%, 39.9019607843%)")
        );
        assert_eq!(
            tv.get("cScalePeer2").and_then(|v| v.as_str()),
            Some("hsl(78.1578947368, 58.4615384615%, 44.5098039216%)")
        );
        assert_eq!(tv.get("useGradient").and_then(|v| v.as_bool()), Some(true));
        assert_eq!(
            tv.get("gradientStart").and_then(|v| v.as_str()),
            Some("hsl(78.1578947368, 18.4615384615%, 64.5098039216%)")
        );
        assert_eq!(
            tv.get("gradientStop").and_then(|v| v.as_str()),
            Some("hsl(98.961038961, 60%, 74.9019607843%)")
        );
    }

    #[test]
    fn dark_theme_derives_peer_and_inverted_scales_like_upstream() {
        let mut cfg = MermaidConfig::from_value(json!({
            "theme": "dark"
        }));
        apply_theme_defaults(&mut cfg).unwrap();

        let tv = cfg
            .as_value()
            .get("themeVariables")
            .and_then(|v| v.as_object())
            .unwrap();

        assert_eq!(tv.get("cScale1").and_then(|v| v.as_str()), Some("#0b0000"));
        assert_eq!(
            tv.get("cScalePeer1").and_then(|v| v.as_str()),
            Some("hsl(0, 100%, 12.1568627451%)")
        );
        assert_eq!(
            tv.get("cScaleInv1").and_then(|v| v.as_str()),
            Some("#f4ffff")
        );
        assert_eq!(
            tv.get("cScaleLabel1").and_then(|v| v.as_str()),
            Some("lightgrey")
        );
        assert_eq!(tv.get("useGradient").and_then(|v| v.as_bool()), Some(true));
        assert_eq!(
            tv.get("gradientStart").and_then(|v| v.as_str()),
            Some("#cccccc")
        );
        assert_eq!(tv.get("mainBkg").and_then(|v| v.as_str()), Some("#1f2020"));
        assert_eq!(
            tv.get("lineColor").and_then(|v| v.as_str()),
            Some("lightgrey")
        );
        assert_eq!(
            tv.get("actorTextColor").and_then(|v| v.as_str()),
            Some("lightgrey")
        );
        assert_eq!(
            tv.get("classText").and_then(|v| v.as_str()),
            Some("#e0dfdf")
        );
        assert_eq!(
            tv.get("noteTextColor").and_then(|v| v.as_str()),
            Some("rgb(183.8476190475, 181.5523809523, 181.5523809523)")
        );
        assert_eq!(
            tv.get("taskTextDarkColor").and_then(|v| v.as_str()),
            Some("#2c2c2c")
        );
        assert_eq!(
            tv.get("attributeBackgroundColorOdd")
                .and_then(|v| v.as_str()),
            Some("hsl(0, 0%, 32%)")
        );
    }

    #[test]
    fn neutral_theme_derives_peer_and_label_scales_like_upstream() {
        let mut cfg = MermaidConfig::from_value(json!({
            "theme": "neutral"
        }));
        apply_theme_defaults(&mut cfg).unwrap();

        let tv = cfg
            .as_value()
            .get("themeVariables")
            .and_then(|v| v.as_object())
            .unwrap();

        assert_eq!(tv.get("cScale0").and_then(|v| v.as_str()), Some("#555"));
        assert_eq!(
            tv.get("cScalePeer0").and_then(|v| v.as_str()),
            Some("hsl(0, 0%, 23.3333333333%)")
        );
        assert_eq!(
            tv.get("cScaleInv0").and_then(|v| v.as_str()),
            Some("#aaaaaa")
        );
        assert_eq!(
            tv.get("cScaleLabel0").and_then(|v| v.as_str()),
            Some("#F4F4F4")
        );
        assert_eq!(tv.get("useGradient").and_then(|v| v.as_bool()), Some(true));
        assert_eq!(
            tv.get("gradientStart").and_then(|v| v.as_str()),
            Some("hsl(0, 0%, 83.3333333333%)")
        );
        assert_eq!(
            tv.get("gradientStop").and_then(|v| v.as_str()),
            Some("hsl(0, 0%, 88.9215686275%)")
        );
        assert_eq!(tv.get("mainBkg").and_then(|v| v.as_str()), Some("#eee"));
        assert_eq!(
            tv.get("textColor").and_then(|v| v.as_str()),
            Some("#000000")
        );
        assert_eq!(
            tv.get("actorTextColor").and_then(|v| v.as_str()),
            Some("#333")
        );
        assert_eq!(
            tv.get("classText").and_then(|v| v.as_str()),
            Some("#111111")
        );
        assert_eq!(
            tv.get("noteBkgColor").and_then(|v| v.as_str()),
            Some("#666")
        );
        assert_eq!(
            tv.get("noteTextColor").and_then(|v| v.as_str()),
            Some("#fff")
        );
        assert_eq!(
            tv.get("taskTextOutsideColor").and_then(|v| v.as_str()),
            Some("#333")
        );
    }
}
