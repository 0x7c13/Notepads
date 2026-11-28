use crate::compatibility_json::clone_value_nonrecursive_controlled;
#[cfg(test)]
use crate::compatibility_json::estimated_value_owned_heap_bytes;
pub(crate) use crate::compatibility_json::{clone_value_nonrecursive, drop_value_nonrecursive};
use crate::{OperationControl, OperationControlResult};
use serde_json::{Map, Value};
use std::sync::Arc;

mod appearance;
mod presentation;
mod source_presentation;
pub(crate) use appearance::resolve_appearance;
pub(crate) use presentation::apply_external_presentation_palette;
pub(crate) use source_presentation::is_presentation_field;

pub(crate) const HARDENED_SECURE_KEYS: &[&str] = &[
    "secure",
    "securityLevel",
    "startOnLoad",
    "maxTextSize",
    "suppressErrorRendering",
    "maxEdges",
    "themeCSS",
];

pub(crate) fn apply_hardened_site_policy(config: &mut MermaidConfig) {
    config.set_value(
        "secure",
        Value::Array(
            HARDENED_SECURE_KEYS
                .iter()
                .map(|key| Value::String((*key).to_string()))
                .collect(),
        ),
    );
}

#[derive(Clone)]
pub struct MermaidConfig {
    value: Arc<Value>,
    state: ConfigState,
}

#[derive(Clone, Default)]
struct ConfigState {
    detector_mutations: Option<Vec<String>>,
    presentation_paint: bool,
    presentation_font: bool,
    presentation_c4_line: bool,
    wardley_axis_text_authored: bool,
}

impl std::fmt::Debug for MermaidConfig {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_tuple("MermaidConfig")
            .field(&self.value)
            .finish()
    }
}

impl PartialEq for MermaidConfig {
    fn eq(&self, other: &Self) -> bool {
        self.value == other.value
    }
}

impl Default for MermaidConfig {
    fn default() -> Self {
        Self::empty_object()
    }
}

impl MermaidConfig {
    pub fn empty_object() -> Self {
        Self::from_value(Value::Object(Map::new()))
    }

    pub fn from_value(value: Value) -> Self {
        Self {
            value: Arc::new(value),
            state: ConfigState::default(),
        }
    }

    pub fn as_value(&self) -> &Value {
        self.value.as_ref()
    }

    pub(crate) fn set_presentation_defaults_active(&mut self, active: bool) {
        self.state.presentation_paint = active;
    }

    pub(crate) fn presentation_defaults_active(&self) -> bool {
        self.state.presentation_paint
    }

    pub(crate) fn set_presentation_font_defaults_active(&mut self, active: bool) {
        self.state.presentation_font = active;
    }

    pub(crate) fn presentation_font_defaults_active(&self) -> bool {
        self.state.presentation_font
    }

    pub(crate) fn set_presentation_c4_line_default_active(&mut self, active: bool) {
        self.state.presentation_c4_line = active;
    }

    pub(crate) fn presentation_c4_line_default_active(&self) -> bool {
        self.state.presentation_c4_line
    }

    pub(crate) fn set_wardley_axis_text_authored(&mut self, authored: bool) {
        self.state.wardley_axis_text_authored = authored;
    }

    pub(crate) fn wardley_axis_text_authored(&self) -> bool {
        self.state.wardley_axis_text_authored
    }

    /// Clones this config without recursion while enforcing a retained-size and nesting budget.
    ///
    /// `Ok(None)` means the owned value would exceed either budget. Cancellation remains distinct
    /// from budget rejection so callers can abandon an enclosing operation immediately.
    pub fn clone_value_bounded_controlled(
        &self,
        max_retained_bytes: usize,
        max_nesting_depth: usize,
        control: &OperationControl,
    ) -> OperationControlResult<Option<Value>> {
        clone_value_nonrecursive_controlled(
            self.as_value(),
            max_retained_bytes,
            max_nesting_depth,
            control,
        )
    }

    pub(crate) fn is_empty_object(&self) -> bool {
        matches!(self.as_value(), Value::Object(map) if map.is_empty())
    }

    pub(crate) fn ptr_eq(&self, other: &Self) -> bool {
        Arc::ptr_eq(&self.value, &other.value)
    }

    #[cfg(test)]
    pub(crate) fn estimated_owned_heap_bytes(&self) -> usize {
        estimated_value_owned_heap_bytes(self.as_value())
    }

    pub fn as_value_mut(&mut self) -> &mut Value {
        self.record_mutation("");
        self.value_mut()
    }

    pub(crate) fn begin_mutation_tracking(&mut self) -> Self {
        let before = self.clone();
        self.state.detector_mutations = Some(Vec::new());
        before
    }

    pub(crate) fn take_mutation_paths(&mut self, before: &Self) -> Vec<String> {
        // Replacing the entire config in a detector also replaces this private tracker.
        let Some(mut paths) = self.state.detector_mutations.take() else {
            return vec![String::new()];
        };
        if paths.iter().any(String::is_empty) {
            // A mutable root borrow alone claims no palette values. Preserve explicit
            // same-value writes, then add only the accessor's actual changes.
            paths.retain(|path| !path.is_empty());
            record_changed_paths(before.as_value(), self.as_value(), &mut paths);
        }
        paths
    }

    fn record_mutation(&mut self, path: &str) {
        if let Some(paths) = &mut self.state.detector_mutations {
            paths.push(path.to_owned());
        }
    }

    pub fn get_str(&self, dotted_path: &str) -> Option<&str> {
        let mut cur: &Value = self.value.as_ref();
        for segment in dotted_path.split('.') {
            cur = cur.as_object()?.get(segment)?;
        }
        cur.as_str()
    }

    pub fn get_bool(&self, dotted_path: &str) -> Option<bool> {
        let mut cur: &Value = self.value.as_ref();
        for segment in dotted_path.split('.') {
            cur = cur.as_object()?.get(segment)?;
        }
        cur.as_bool()
    }

    pub fn set_value(&mut self, dotted_path: &str, value: Value) {
        self.record_mutation(dotted_path);
        let root_value = self.value_mut();
        // Be defensive: callers can construct `MermaidConfig` from any JSON value via
        // `from_value`. Mermaid configs are objects; if we see a non-object here, coerce it
        // to an object so this API never panics on user input.
        if !root_value.is_object() {
            replace_value_nonrecursive(root_value, Value::Object(Map::new()));
        }

        let Value::Object(root) = root_value else {
            return;
        };
        let mut cur: &mut Map<String, Value> = root;
        let mut segments = dotted_path.split('.').peekable();
        while let Some(seg) = segments.next() {
            if segments.peek().is_none() {
                if let Some(old) = cur.insert(seg.to_string(), value) {
                    drop_value_nonrecursive(old);
                }
                return;
            }
            let slot = cur.entry(seg).or_insert_with(|| Value::Object(Map::new()));
            if !slot.is_object() {
                replace_value_nonrecursive(slot, Value::Object(Map::new()));
            }
            let Some(next) = slot.as_object_mut() else {
                return;
            };
            cur = next;
        }
    }

    pub fn deep_merge(&mut self, other: &Value) {
        if other.as_object().is_some_and(Map::is_empty) {
            return;
        }
        let mut paths = self.state.detector_mutations.take();
        let base = self.value_mut();
        if paths.is_some() {
            assign_with_depth(base, other, 2, &mut MergeRecorder::new(paths.as_mut()));
        } else {
            deep_merge_value(base, other);
        }
        self.state.detector_mutations = paths;
    }

    pub(crate) fn source_filtered_overrides(
        &self,
        overrides: &MermaidConfig,
        control: &OperationControl,
    ) -> OperationControlResult<MermaidConfig> {
        let mut filtered = overrides.clone();
        // Normalize legacy typography before authority checks so a locked themeVariables or
        // fontFamily key cannot be reintroduced through the other spelling after filtering.
        mirror_legacy_font_family_into_theme_variables(&mut filtered);
        remove_secure_keys_recursive(self.as_value(), filtered.value_mut());
        source_presentation::filter(filtered.value_mut(), control)?;
        Ok(filtered)
    }

    fn value_mut(&mut self) -> &mut Value {
        if Arc::strong_count(&self.value) != 1 || Arc::weak_count(&self.value) != 0 {
            self.value = Arc::new(clone_value_nonrecursive(self.value.as_ref()));
        }
        Arc::make_mut(&mut self.value)
    }
}

impl Drop for MermaidConfig {
    fn drop(&mut self) {
        if let Some(value) = Arc::get_mut(&mut self.value) {
            let old = std::mem::replace(value, Value::Null);
            drop_value_nonrecursive(old);
        }
    }
}

fn remove_secure_keys_recursive(site_config: &Value, overrides: &mut Value) {
    let secure_keys = site_config
        .get("secure")
        .and_then(Value::as_array)
        .map(|items| items.iter().filter_map(Value::as_str).collect::<Vec<_>>())
        .unwrap_or_default();

    let mut stack = vec![overrides];
    while let Some(current) = stack.pop() {
        match current {
            Value::Object(map) => {
                if let Some(old) = map.remove("secure") {
                    drop_value_nonrecursive(old);
                }
                for key in &secure_keys {
                    if let Some(old) = map.remove(*key) {
                        drop_value_nonrecursive(old);
                    }
                }
                for child in map.values_mut().rev() {
                    stack.push(child);
                }
            }
            Value::Array(items) => {
                for child in items.iter_mut().rev() {
                    stack.push(child);
                }
            }
            Value::Null | Value::Bool(_) | Value::Number(_) | Value::String(_) => {}
        }
    }
}

pub(crate) fn mirror_legacy_font_family_into_theme_variables(config: &mut MermaidConfig) {
    let value = config.value_mut();
    mirror_legacy_font_family_into_theme_variables_value(value);
}

pub(crate) fn mirror_legacy_font_family_into_theme_variables_value(value: &mut Value) {
    let Some(root) = value.as_object_mut() else {
        return;
    };
    let Some(font_family) = root
        .get("fontFamily")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_string)
    else {
        return;
    };

    let has_theme_font_family = root
        .get("themeVariables")
        .and_then(Value::as_object)
        .and_then(|theme_variables| theme_variables.get("fontFamily"))
        .and_then(Value::as_str)
        .is_some_and(|s| !s.trim().is_empty());
    if has_theme_font_family {
        return;
    }

    let theme_variables = root
        .entry("themeVariables")
        .or_insert_with(|| Value::Object(Map::new()));
    if !theme_variables.is_object() {
        replace_value_nonrecursive(theme_variables, Value::Object(Map::new()));
    }
    if let Some(theme_variables) = theme_variables.as_object_mut()
        && let Some(old) =
            theme_variables.insert("fontFamily".to_string(), Value::String(font_family))
    {
        drop_value_nonrecursive(old);
    }
}

fn record_changed_paths(before: &Value, after: &Value, paths: &mut Vec<String>) {
    let mut pending = vec![(String::new(), Some(before), Some(after))];
    while let Some((path, before, after)) = pending.pop() {
        if let (Some(Value::Object(before)), Some(Value::Object(after))) = (before, after) {
            let child_path = |key: &str| {
                if path.is_empty() {
                    key.to_owned()
                } else {
                    format!("{path}.{key}")
                }
            };
            for (key, value) in after {
                let previous = before.get(key);
                if previous.is_some_and(|previous| {
                    crate::compatibility_json::equal_values(previous, value)
                }) {
                    continue;
                }
                pending.push((child_path(key), previous, Some(value)));
            }
            for key in before.keys().filter(|key| !after.contains_key(*key)) {
                paths.push(child_path(key));
            }
        } else if !matches!((before, after), (Some(before), Some(after))
            if crate::compatibility_json::equal_values(before, after))
        {
            // Arrays and changed container types own their complete replacement value.
            paths.push(path);
        }
    }
}

struct MergeRecorder<'a> {
    writes: Option<&'a mut Vec<String>>,
    path: Vec<String>,
}

impl<'a> MergeRecorder<'a> {
    fn new(writes: Option<&'a mut Vec<String>>) -> Self {
        Self {
            writes,
            path: Vec::new(),
        }
    }

    fn push(&mut self, key: &str) {
        if self.writes.is_some() {
            self.path.push(key.to_owned());
        }
    }

    fn pop(&mut self) {
        self.path.pop();
    }

    fn record(&mut self) {
        if let Some(writes) = &mut self.writes {
            writes.push(self.path.join("."));
        }
    }
}

fn deep_merge_value(base: &mut Value, incoming: &Value) {
    // Mermaid 11.16.1 uses `assignWithDepth(dst, src)` with its default depth of two for site,
    // frontmatter, and directive configuration. Keep that bounded traversal instead of turning
    // configuration merging into an unbounded recursive deep merge.
    assign_with_depth(base, incoming, 2, &mut MergeRecorder::new(None));
}

fn assign_with_depth(
    destination: &mut Value,
    source: &Value,
    depth: usize,
    recorder: &mut MergeRecorder<'_>,
) {
    if let Value::Array(source_items) = source {
        match destination {
            Value::Array(destination_items) => {
                merge_arrays(destination_items, source_items);
                recorder.record();
            }
            Value::Object(_) => merge_array_of_sources(destination, source_items, depth, recorder),
            Value::Null | Value::Bool(_) | Value::Number(_) | Value::String(_) => {}
        }
        return;
    }

    if destination.is_null() {
        // Mermaid 11.16.1 treats a null destination as an absent value and returns the source.
        // Mutating the slot in place is the JSON equivalent of the caller assigning that return
        // value back to the parent property.
        replace_value_nonrecursive(destination, clone_value_nonrecursive(source));
        recorder.record();
        return;
    }

    if source.is_null() {
        // `assignWithDepth` ignores null-valued object properties when the destination is present.
        return;
    }

    if depth == 0 {
        object_assign(destination, source, recorder);
        return;
    }

    let (Value::Object(destination_map), Value::Object(source_map)) = (destination, source) else {
        return;
    };

    for (key, source_child) in source_map {
        recorder.push(key);
        if is_non_null_js_object(source_child) {
            match destination_map.get_mut(key) {
                Some(destination_child) if is_js_object(destination_child) => {
                    assign_with_depth(destination_child, source_child, depth - 1, recorder);
                }
                Some(_) => {}
                None => {
                    destination_map.insert(key.clone(), empty_container_for(source_child));
                    if let Some(destination_child) = destination_map.get_mut(key) {
                        assign_with_depth(destination_child, source_child, depth - 1, recorder);
                    }
                }
            }
        } else if !source_child.is_null()
            && destination_map
                .get(key)
                .is_none_or(|value| !is_js_object(value))
        {
            insert_cloned(destination_map, key, source_child);
            recorder.record();
        }
        recorder.pop();
    }
}

fn merge_array_of_sources(
    destination: &mut Value,
    source_items: &[Value],
    depth: usize,
    recorder: &mut MergeRecorder<'_>,
) {
    let mut stack = source_items.iter().rev().collect::<Vec<_>>();
    while let Some(source) = stack.pop() {
        if let Value::Array(items) = source {
            stack.extend(items.iter().rev());
        } else {
            assign_with_depth(destination, source, depth, recorder);
        }
    }
}

fn merge_arrays(destination: &mut Vec<Value>, source: &[Value]) {
    for source_item in source {
        let already_present = is_json_primitive(source_item)
            && destination
                .iter()
                .any(|destination_item| same_json_primitive(destination_item, source_item));
        if !already_present {
            destination.push(clone_value_nonrecursive(source_item));
        }
    }
}

fn object_assign(destination: &mut Value, source: &Value, recorder: &mut MergeRecorder<'_>) {
    match (destination, source) {
        (Value::Object(destination_map), Value::Object(source_map)) => {
            for (key, source_child) in source_map {
                insert_cloned(destination_map, key, source_child);
                recorder.push(key);
                recorder.record();
                recorder.pop();
            }
        }
        // JavaScript arrays can own named properties, while JSON arrays cannot represent them.
        // Numeric config keys are not part of Mermaid's public config shape, so leave this
        // unrepresentable object-to-array case unchanged.
        (Value::Array(_), Value::Object(_)) => {}
        (destination, source) => {
            replace_value_nonrecursive(destination, clone_value_nonrecursive(source));
            recorder.record();
        }
    }
}

fn insert_cloned(destination: &mut Map<String, Value>, key: &str, source: &Value) {
    if let Some(previous) = destination.insert(key.to_string(), clone_value_nonrecursive(source)) {
        drop_value_nonrecursive(previous);
    }
}

fn empty_container_for(value: &Value) -> Value {
    if value.is_array() {
        Value::Array(Vec::new())
    } else {
        Value::Object(Map::new())
    }
}

fn is_js_object(value: &Value) -> bool {
    matches!(value, Value::Null | Value::Array(_) | Value::Object(_))
}

fn is_non_null_js_object(value: &Value) -> bool {
    matches!(value, Value::Array(_) | Value::Object(_))
}

fn is_json_primitive(value: &Value) -> bool {
    matches!(
        value,
        Value::Null | Value::Bool(_) | Value::Number(_) | Value::String(_)
    )
}

fn same_json_primitive(left: &Value, right: &Value) -> bool {
    match (left, right) {
        (Value::Null, Value::Null) => true,
        (Value::Bool(left), Value::Bool(right)) => left == right,
        (Value::Number(left), Value::Number(right)) => left.as_f64() == right.as_f64(),
        (Value::String(left), Value::String(right)) => left == right,
        _ => false,
    }
}

pub(crate) fn replace_value_nonrecursive(slot: &mut Value, value: Value) {
    let old = std::mem::replace(slot, value);
    drop_value_nonrecursive(old);
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn mirror_legacy_font_family_populates_missing_theme_variable() {
        let mut cfg = MermaidConfig::from_value(json!({
            "fontFamily": "Courier"
        }));

        mirror_legacy_font_family_into_theme_variables(&mut cfg);

        assert_eq!(cfg.get_str("themeVariables.fontFamily"), Some("Courier"));
    }

    #[test]
    fn mirror_legacy_font_family_preserves_explicit_theme_variable() {
        let mut cfg = MermaidConfig::from_value(json!({
            "fontFamily": "Courier",
            "themeVariables": {
                "fontFamily": "Inter"
            }
        }));

        mirror_legacy_font_family_into_theme_variables(&mut cfg);

        assert_eq!(cfg.get_str("themeVariables.fontFamily"), Some("Inter"));
    }

    #[test]
    fn deep_merge_ignores_null_source_values() {
        let mut config = MermaidConfig::from_value(json!({
            "theme": "default",
            "flowchart": {
                "htmlLabels": true
            }
        }));

        config.deep_merge(&json!({
            "theme": null,
            "flowchart": {
                "htmlLabels": null,
                "curve": "basis"
            },
            "newValue": null
        }));

        assert_eq!(
            config.as_value(),
            &json!({
                "theme": "default",
                "flowchart": {
                    "htmlLabels": true,
                    "curve": "basis"
                }
            })
        );

        config.deep_merge(&Value::Null);
        assert_eq!(config.get_str("theme"), Some("default"));
    }

    #[test]
    fn deep_merge_preserves_null_values_inside_arrays() {
        let mut config = MermaidConfig::from_value(json!({
            "values": ["old"]
        }));

        config.deep_merge(&json!({
            "values": [1, null, 2]
        }));

        assert_eq!(config.as_value()["values"], json!(["old", 1, null, 2]));
    }

    #[test]
    fn deep_merge_preserves_null_values_at_the_depth_boundary() {
        let mut config = MermaidConfig::default();

        config.deep_merge(&json!({
            "flowchart": {
                "curve": "basis",
                "nested": {
                    "ignored": null,
                    "kept": true
                },
                "values": [1, null, { "insideArray": null }]
            }
        }));

        assert_eq!(
            config.as_value()["flowchart"],
            json!({
                "curve": "basis",
                "nested": {
                    "ignored": null,
                    "kept": true
                },
                "values": [1, null, { "insideArray": null }]
            })
        );
    }

    #[test]
    fn deep_merge_deduplicates_primitives_in_new_arrays() {
        let mut config = MermaidConfig::default();

        config.deep_merge(&json!({
            "values": [1, 1.0, null, null, { "value": 1 }, { "value": 1 }]
        }));

        assert_eq!(
            config.as_value()["values"],
            json!([1, null, { "value": 1 }, { "value": 1 }])
        );
    }

    #[test]
    fn deep_merge_does_not_clobber_dissimilar_types_but_replaces_null_destinations() {
        let mut config = MermaidConfig::from_value(json!({
            "object": { "kept": true },
            "scalar": "kept",
            "nullObject": null,
            "nullScalar": null
        }));

        config.deep_merge(&json!({
            "object": "ignored",
            "scalar": { "ignored": true },
            "nullObject": { "accepted": true },
            "nullScalar": "accepted"
        }));

        assert_eq!(
            config.as_value(),
            &json!({
                "object": { "kept": true },
                "scalar": "kept",
                "nullObject": { "accepted": true },
                "nullScalar": null
            })
        );

        let mut root_null = MermaidConfig::from_value(Value::Null);
        root_null.deep_merge(&json!({ "accepted": true }));
        assert_eq!(root_null.as_value(), &json!({ "accepted": true }));
    }

    #[test]
    fn deep_merge_uses_object_assign_at_the_default_depth_boundary() {
        let mut config = MermaidConfig::from_value(json!({
            "bar": {
                "bar": {
                    "foo": {
                        "message": "old",
                        "willBe": "clobbered"
                    },
                    "preservedSibling": true
                }
            }
        }));

        config.deep_merge(&json!({
            "bar": {
                "bar": {
                    "foo": {
                        "message": "new"
                    }
                }
            }
        }));

        assert_eq!(
            config.as_value(),
            &json!({
                "bar": {
                    "bar": {
                        "foo": {
                            "message": "new"
                        },
                        "preservedSibling": true
                    }
                }
            })
        );
    }

    #[test]
    fn deep_merge_replaces_nested_null_at_the_depth_boundary() {
        let mut config = MermaidConfig::from_value(json!({
            "outer": {
                "inner": {
                    "slot": null
                }
            }
        }));

        config.deep_merge(&json!({
            "outer": {
                "inner": {
                    "slot": { "accepted": true }
                }
            }
        }));

        assert_eq!(
            config.as_value()["outer"]["inner"]["slot"],
            json!({ "accepted": true })
        );
    }

    fn deep_config_value(depth: usize) -> Value {
        let mut value = Value::String("leaf".to_string());
        for idx in (0..depth).rev() {
            let mut map = Map::new();
            map.insert(format!("k{idx}"), value);
            value = Value::Object(map);
        }
        value
    }

    #[test]
    fn clone_on_write_handles_deep_config_with_small_stack() {
        const DEPTH: usize = 2_048;
        let value = deep_config_value(DEPTH);
        let handle = std::thread::Builder::new()
            .name("mermaid-config-deep-clone-on-write".to_string())
            .stack_size(64 * 1024)
            .spawn(move || {
                let original = MermaidConfig::from_value(value);
                let mut cloned = original.clone();
                cloned.set_value("theme", Value::String("default".to_string()));
                assert_eq!(cloned.get_str("theme"), Some("default"));
            })
            .expect("spawn deep config clone-on-write test");
        handle
            .join()
            .expect("deep config clone-on-write should finish without stack overflow");
    }

    #[test]
    fn bounded_controlled_config_clone_preserves_values_within_budget() {
        let config = MermaidConfig::from_value(json!({
            "theme": "dark",
            "flowchart": { "htmlLabels": false },
        }));
        let control = OperationControl::new();

        let cloned = config
            .clone_value_bounded_controlled(64 * 1024, 16, &control)
            .expect("active control")
            .expect("small config fits the materialization budget");

        assert_eq!(&cloned, config.as_value());
    }

    #[test]
    fn bounded_controlled_config_clone_rejects_weight_and_depth_before_cloning() {
        let oversized = MermaidConfig::from_value(json!({ "payload": "x".repeat(4 * 1024) }));
        let deep = MermaidConfig::from_value(deep_config_value(8));
        let control = OperationControl::new();

        assert!(
            oversized
                .clone_value_bounded_controlled(1_024, 16, &control)
                .expect("active control")
                .is_none()
        );
        assert!(
            deep.clone_value_bounded_controlled(64 * 1024, 4, &control)
                .expect("active control")
                .is_none()
        );
    }

    #[test]
    fn bounded_controlled_config_clone_observes_cancellation() {
        let config = MermaidConfig::from_value(json!({ "theme": "dark" }));
        let control = OperationControl::new();
        control.cancel();

        assert!(matches!(
            config.clone_value_bounded_controlled(64 * 1024, 16, &control),
            Err(crate::OperationCancelled { .. })
        ));
    }

    #[test]
    fn upstream_secure_key_list_matches_mermaid_runtime() {
        let upstream = crate::generated::upstream_default_config();
        let secure = upstream
            .as_value()
            .get("secure")
            .and_then(Value::as_array)
            .expect("upstream secure array")
            .iter()
            .map(|value| value.as_str().expect("secure key string"))
            .collect::<Vec<_>>();

        assert_eq!(
            secure,
            [
                "secure",
                "securityLevel",
                "startOnLoad",
                "maxTextSize",
                "suppressErrorRendering",
                "maxEdges"
            ]
        );
    }

    #[test]
    fn default_site_config_applies_hardened_secure_policy() {
        let default = crate::generated::default_site_config();
        let secure = default
            .as_value()
            .get("secure")
            .and_then(Value::as_array)
            .expect("hardened secure array")
            .iter()
            .map(|value| value.as_str().expect("secure key string"))
            .collect::<Vec<_>>();

        assert_eq!(secure, HARDENED_SECURE_KEYS);
    }

    #[test]
    fn secure_filtered_overrides_removes_default_secure_keys_recursively() {
        let site_config = crate::generated::default_site_config();
        let overrides = MermaidConfig::from_value(json!({
            "securityLevel": "loose",
            "fontFamily": "diagram-font",
            "flowchart": {
                "securityLevel": "sandbox",
                "htmlLabels": false,
                "nested": [
                    {
                        "securityLevel": "loose",
                        "shape": "rect"
                    }
                ]
            }
        }));

        let filtered = site_config
            .source_filtered_overrides(&overrides, &OperationControl::new())
            .unwrap();

        assert_eq!(filtered.get_str("fontFamily"), Some("diagram-font"));
        assert_eq!(filtered.get_bool("flowchart.htmlLabels"), Some(false));
        assert_eq!(
            filtered.as_value()["flowchart"]["nested"][0]["shape"],
            json!("rect")
        );
        assert!(filtered.get_str("securityLevel").is_none());
        assert!(filtered.get_str("flowchart.securityLevel").is_none());
        assert!(
            filtered.as_value()["flowchart"]["nested"][0]
                .get("securityLevel")
                .is_none()
        );
    }

    #[test]
    fn secure_filtered_overrides_keeps_the_merged_secure_policy() {
        let mut site_config = crate::generated::default_site_config();
        site_config.deep_merge(&json!({
            "secure": ["fontSize"]
        }));
        let overrides = MermaidConfig::from_value(json!({
            "secure": ["theme"],
            "fontSize": 99,
            "securityLevel": "loose",
            "theme": "dark"
        }));

        let filtered = site_config
            .source_filtered_overrides(&overrides, &OperationControl::new())
            .unwrap();

        assert!(filtered.as_value().get("secure").is_none());
        assert!(filtered.as_value().get("fontSize").is_none());
        assert!(filtered.get_str("securityLevel").is_none());
        assert_eq!(filtered.get_str("theme"), Some("dark"));
    }
}
