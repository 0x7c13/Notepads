#![cfg(feature = "all-diagrams")]

use merman_core::time::{CivilDate, OffsetDateTime, UtcOffset};
use merman_core::{Engine, MermaidConfig, ParseOptions};
use merman_fixture_render_context::RenderContextCatalog;
use regex::Regex;
use serde_json::{Map, Value};
use std::path::{Path, PathBuf};
use std::sync::OnceLock;

fn notepads_markdown_to_local_iso(ms: i64) -> Option<String> {
    let dt = OffsetDateTime::from_unix_millis(ms, UtcOffset::UTC).utc_datetime();
    Some(format!(
        "{}T{:02}:{:02}:{:02}.{:03}",
        dt.date(),
        dt.hour(),
        dt.minute(),
        dt.second(),
        dt.millisecond()
    ))
}

fn workspace_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("..").join("..")
}

fn fixtures_root() -> PathBuf {
    workspace_root().join("fixtures")
}

fn fixture_render_contexts() -> &'static RenderContextCatalog {
    static CATALOG: OnceLock<RenderContextCatalog> = OnceLock::new();
    CATALOG.get_or_init(|| {
        RenderContextCatalog::load(fixtures_root()).expect("valid fixture render context catalog")
    })
}

fn fixture_site_config_for_path(path: &Path) -> Option<MermaidConfig> {
    fixture_render_contexts()
        .context_for_fixture(path)
        .unwrap_or_else(|error| {
            panic!(
                "invalid fixture render context lookup for {}: {error}",
                path.display()
            )
        })
        .map(|context| MermaidConfig::from_value(context.site_config_value()))
}

fn engine_for_fixture(base: &Engine, path: &Path) -> Engine {
    match fixture_site_config_for_path(path) {
        Some(site_config) => base.clone().with_site_config(site_config),
        None => base.clone(),
    }
}

#[test]
fn newly_cataloged_host_options_change_effective_security_policy() {
    const FIXTURES: &[&str] = &[
        "flowchart/local_flowchart_elk_hardening_cluster_boundary_styles_004.mmd",
        "flowchart/local_flowchart_elk_hardening_compound_parent_child_edges_001.mmd",
        "flowchart/upstream_cypress_flowchart_elk_spec_57_elk_handle_nested_subgraphs_with_outgoing_links_2_017.mmd",
        "flowchart/upstream_cypress_flowchart_elk_spec_57_elk_handle_nested_subgraphs_with_outgoing_links_4_016.mmd",
        "flowchart/upstream_cypress_flowchart_handdrawn_spec_fdh37_should_render_non_escaped_with_html_labels_037.mmd",
    ];

    let base = Engine::new();
    for relative in FIXTURES {
        let path = fixtures_root().join(relative);
        let source = std::fs::read_to_string(&path).expect("read fixture host-option source");
        let default_metadata = base
            .parse_metadata_sync(&source)
            .unwrap_or_else(|error| panic!("parse {relative} without context: {error}"));
        assert_eq!(
            default_metadata.effective_config.get_str("securityLevel"),
            Some("strict"),
            "diagram config alone must not change the secure host policy for {relative}"
        );

        let contextual_metadata = engine_for_fixture(&base, &path)
            .parse_metadata_sync(&source)
            .unwrap_or_else(|error| panic!("parse {relative} with render context: {error}"));
        assert_eq!(
            contextual_metadata
                .effective_config
                .get_str("securityLevel"),
            Some("loose"),
            "cataloged upstream host option must reach effective config for {relative}"
        );
    }
}

fn list_fixture_mmd_files(root: &Path) -> Vec<PathBuf> {
    // Generated reference SVGs and temporary/private trees are not authored source fixtures.
    let mut out = Vec::new();
    let mut stack = vec![root.to_path_buf()];
    while let Some(dir) = stack.pop() {
        if dir
            .file_name()
            .and_then(|n| n.to_str())
            .is_some_and(|name| {
                name.starts_with('_') || name.starts_with('.') || name == "upstream-svgs"
            })
        {
            continue;
        }
        let Ok(entries) = std::fs::read_dir(&dir) else {
            continue;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                if path
                    .file_name()
                    .and_then(|n| n.to_str())
                    .is_some_and(|name| {
                        name.starts_with('_') || name.starts_with('.') || name == "upstream-svgs"
                    })
                {
                    continue;
                }
                stack.push(path);
                continue;
            }
            if path.extension().is_some_and(|e| e == "mmd") {
                out.push(path);
            }
        }
    }
    out.sort();
    out
}

fn normalize_model(diagram_type: &str, model: &mut Value) {
    let Value::Object(obj) = model else {
        return;
    };

    obj.remove("config");

    // Mermaid mindmap includes a random UUID v4-based diagram id.
    if diagram_type == "mindmap" && obj.get("diagramId").is_some() {
        obj.insert(
            "diagramId".to_string(),
            Value::String("<dynamic>".to_string()),
        );
    }

    // Mermaid gantt uses local time; epoch millis are timezone-dependent and not portable as goldens.
    // Normalize task timestamps into local ISO strings so snapshots are stable across timezones.
    if diagram_type == "gantt" {
        let date_format = obj
            .get("dateFormat")
            .and_then(Value::as_str)
            .unwrap_or("")
            .trim();
        if matches!(date_format, "x" | "X") {
            return;
        }

        let Some(tasks) = obj.get_mut("tasks").and_then(Value::as_array_mut) else {
            return;
        };
        for task in tasks {
            let Value::Object(task_obj) = task else {
                continue;
            };
            for key in ["startTime", "endTime", "renderEndTime"] {
                let Some(v) = task_obj.get_mut(key) else {
                    continue;
                };
                let Some(ms) = v
                    .as_i64()
                    .or_else(|| v.as_u64().and_then(|n| i64::try_from(n).ok()))
                else {
                    continue;
                };
                if let Some(s) = notepads_markdown_to_local_iso(ms) {
                    *v = Value::String(s);
                }
            }
        }
    }

    // Mermaid gitGraph auto-generates commit ids using random hex suffixes.
    // Normalize these ids so snapshots are stable across runs.
    if diagram_type == "gitGraph" {
        let re = Regex::new(r"\b(\d+)-[0-9a-f]{7}\b").expect("gitGraph id regex must compile");

        fn walk(re: &Regex, v: &mut Value) {
            match v {
                Value::String(s) if re.is_match(s) => {
                    *s = re.replace_all(s, "$1-<dynamic>").to_string();
                }
                Value::Array(arr) => {
                    for item in arr {
                        walk(re, item);
                    }
                }
                Value::Object(map) => {
                    for (_k, val) in map.iter_mut() {
                        walk(re, val);
                    }
                }
                _ => {}
            }
        }

        walk(&re, model);
    }

    // Mermaid block diagram auto-generates internal block ids using random base36 suffixes.
    // Normalize these ids so snapshots are stable across runs.
    if diagram_type == "block" {
        let re = Regex::new(r"id-[a-z0-9]+-(\d+)").expect("block id regex must compile");

        fn walk(re: &Regex, v: &mut Value) {
            match v {
                Value::String(s) if re.is_match(s) => {
                    *s = re.replace_all(s, "id-<id>-$1").to_string();
                }
                Value::Array(arr) => {
                    for item in arr {
                        walk(re, item);
                    }
                }
                Value::Object(map) => {
                    for (_k, val) in map.iter_mut() {
                        walk(re, val);
                    }
                }
                _ => {}
            }
        }

        walk(&re, model);
    }
}

fn snapshot_value(diagram_type: &str, mut model: Value) -> Value {
    normalize_model(diagram_type, &mut model);

    let mut out = Map::new();
    out.insert(
        "diagramType".to_string(),
        Value::String(diagram_type.to_string()),
    );
    out.insert("model".to_string(), model);
    Value::Object(out)
}

#[test]
fn fixtures_match_golden_snapshots() {
    let fixtures = list_fixture_mmd_files(&fixtures_root());
    assert!(
        !fixtures.is_empty(),
        "no fixtures found under {}",
        fixtures_root().display()
    );

    // Keep time-dependent diagrams (e.g. Gantt) deterministic for fixtures.
    let engine = Engine::new()
        .with_fixed_today(Some(CivilDate::new(2026, 2, 15).expect("valid date")))
        // Gantt date handling follows JavaScript local-time semantics, which varies by runner timezone.
        // Pin a fixed offset so snapshots are stable across CI environments.
        .try_with_fixed_local_offset_minutes(0)
        .expect("UTC is a valid fixed offset");
    for mmd_path in fixtures {
        let text = std::fs::read_to_string(&mmd_path)
            .unwrap_or_else(|e| panic!("failed to read fixture {}: {e}", mmd_path.display()));
        let fixture_engine = engine_for_fixture(&engine, &mmd_path);
        let parsed = futures::executor::block_on(fixture_engine.parse_diagram(
            &text,
            ParseOptions {
                suppress_errors: true,
            },
        ))
        .unwrap_or_else(|e| panic!("parse failed for {}: {e}", mmd_path.display()))
        .unwrap_or_else(|| panic!("no diagram detected in {}", mmd_path.display()));

        let snapshot = snapshot_value(
            &parsed.meta.diagram_type,
            parsed.model.into_unmanaged_value(),
        );
        let golden_path = mmd_path.with_extension("golden.json");
        let golden_text = std::fs::read_to_string(&golden_path).unwrap_or_else(|_| {
            panic!(
                "missing golden snapshot {} (generate with `cargo run -p xtask -- update-snapshots`)",
                golden_path.display()
            )
        });
        let golden: Value = serde_json::from_str(&golden_text)
            .unwrap_or_else(|e| panic!("invalid golden JSON {}: {e}", golden_path.display()));

        assert_eq!(
            snapshot,
            golden,
            "snapshot mismatch for {} (update with `cargo run -p xtask -- update-snapshots`)",
            mmd_path.display()
        );
    }
}
