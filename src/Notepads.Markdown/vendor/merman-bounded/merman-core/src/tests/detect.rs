use crate::*;
use futures::executor::block_on;
use serde_json::{Value, json};
use std::fmt::Write;

fn test_detector_always_matches(_text: &str, _config: &mut MermaidConfig) -> bool {
    true
}

#[test]
fn canonical_catalog_detects_mindmap() {
    let engine = Engine::new();
    let res = block_on(engine.parse_metadata("mindmap\n  root")).unwrap();
    assert_eq!(res.diagram_type, "mindmap");
}

#[test]
fn usecase_header_requires_whitespace_or_end() {
    let registry = crate::detect::DetectorRegistry::pinned_mermaid_baseline();
    for source in [
        "usecase-beta",
        "  usecase-beta\nactor A",
        "usecase-beta\tLR",
    ] {
        let detected = registry.detect_type(source, &mut MermaidConfig::empty_object());
        assert_eq!(detected.expect("Usecase header"), "usecase");
    }
    for source in [
        "usecase-betaExtra",
        "usecase-beta;",
        "usecase-beta_LR",
        "Usecase-beta",
    ] {
        assert!(
            registry
                .detect_type(source, &mut MermaidConfig::empty_object())
                .is_err()
        );
    }
}

#[test]
fn canonical_catalog_detects_flowchart_elk_and_sets_layout() {
    let engine = Engine::new();
    let res = block_on(engine.parse_metadata("flowchart-elk TD\nA-->B")).unwrap();
    assert_eq!(res.diagram_type, "flowchart-elk");
    assert_eq!(res.effective_config.get_str("layout"), Some("elk"));
}

#[test]
fn generated_defaults_preserve_mermaids_runtime_class_object_override() {
    let expected = json!({
        "theme": "redux-color",
        "look": "neo",
        "hideEmptyMembersBox": false,
        "hierarchicalNamespaces": true
    });
    assert_eq!(
        crate::generated::default_site_config()
            .as_value()
            .get("class"),
        Some(&expected),
        "Mermaid 12 defaultConfig.ts carries appearance defaults into the replacement Class object"
    );
}

#[test]
fn detects_tree_view_beta_as_tree_view() {
    let engine = Engine::new();
    let res = block_on(engine.parse_metadata("treeView-beta\n\"Root\"")).unwrap();
    assert_eq!(res.diagram_type, "treeView");
}

#[test]
fn detects_ishikawa_headers_as_ishikawa() {
    let engine = Engine::new();
    for header in ["ishikawa", "ishikawa-beta", "ISHIKAWA-BETA"] {
        let res = block_on(engine.parse_metadata(&format!("{header}\nProblem"))).unwrap();
        assert_eq!(res.diagram_type, "ishikawa");
    }
}

#[test]
fn detects_eventmodeling_as_eventmodeling() {
    let engine = Engine::new();
    let res = block_on(engine.parse_metadata("eventmodeling\ntf 01 evt Start")).unwrap();
    assert_eq!(res.diagram_type, "eventmodeling");
}

#[test]
fn detects_venn_beta_as_venn() {
    let engine = Engine::new();
    let res = block_on(engine.parse_metadata("venn-beta\nset A")).unwrap();
    assert_eq!(res.diagram_type, "venn");
}

#[test]
fn detects_11_16_new_family_headers_for_metadata() {
    let engine = Engine::new();

    for (source, expected_type) in [
        ("swimlane-beta\nA --> B", "swimlane"),
        ("cynefin-beta:\nDomain: clear", "cynefin"),
        ("railroad-beta\nA ::= B", "railroad"),
        ("RAILROAD-EBNF-BETA\nrule ::= term", "railroadEbnf"),
        ("railroad-abnf-beta\nrule = term", "railroadAbnf"),
        ("railroad-peg-beta\nrule <- term", "railroadPeg"),
        ("wardley-beta\ncomponent A", "wardley"),
    ] {
        let res = engine.parse_metadata_sync(source).unwrap();
        assert_eq!(res.diagram_type, expected_type, "source: {source:?}");
    }
}

#[test]
fn detects_11_16_new_family_headers_with_upstream_boundaries() {
    let registry = DetectorRegistry::pinned_mermaid_baseline();
    let mut config = MermaidConfig::empty_object();

    let cynefin = registry
        .detect_type_precleaned("cynefin-beta:\nClear", &mut config)
        .expect("cynefin colon boundary should match");
    assert_eq!(cynefin, "cynefin");

    let railroad_prefix = registry
        .detect_type_precleaned("railroad-betatron", &mut config)
        .expect("railroad upstream regex has no trailing boundary");
    assert_eq!(railroad_prefix, "railroad");

    let err = registry
        .detect_type_precleaned("swimlane-betatron", &mut config)
        .unwrap_err();
    assert!(
        err.to_string()
            .contains("No diagram type detected matching given configuration"),
        "swimlane-beta uses JS word-boundary semantics: {err}"
    );
}

#[test]
fn supplied_detector_registry_preserves_upstream_first_match_order() {
    let mut registry = DetectorRegistry::new();
    registry.add_fn("host-first", test_detector_always_matches);
    registry.add_fn("host-second", test_detector_always_matches);
    let mut config = MermaidConfig::empty_object();

    // Mermaid detectType iterates the supplied detector records and returns the first match.
    let detected = registry
        .detect_type("sequenceDiagram\nA ->> B: hello", &mut config)
        .unwrap();

    assert_eq!(detected, "host-first");
}

#[test]
fn empty_detector_registry_rejects_builtin_leading_keywords() {
    let registry = DetectorRegistry::new();
    let mut config = MermaidConfig::empty_object();

    let error = registry
        .detect_type_precleaned("sequenceDiagram\nA ->> B: hello", &mut config)
        .unwrap_err();

    assert!(
        error
            .to_string()
            .contains("No diagram type detected matching given configuration"),
        "unexpected error: {error}"
    );
}

#[test]
#[cfg(feature = "diagram-kanban")]
fn kanban_detector_and_known_type_parser_preserve_distinct_upstream_case_rules() {
    let engine = Engine::new();

    let detected = engine.parse_metadata_sync("kanban\nTodo[Todo]\n").unwrap();
    assert_eq!(detected.diagram_type, "kanban");

    let detection_error = engine
        .parse_metadata_sync("KaNbAn\nTodo[Todo]\n")
        .unwrap_err();
    assert!(
        detection_error
            .to_string()
            .contains("No diagram type detected matching given configuration")
    );

    let parsed = engine
        .parse_diagram_with_type_sync("kanban", "KaNbAn\nTodo[Todo]\n", ParseOptions::strict())
        .expect("known-type Kanban parser follows Jison's case-insensitive mode")
        .expect("known-type Kanban parse returns a model");
    assert_eq!(parsed.meta.diagram_type, "kanban");
}

#[test]
fn c4_detector_preserves_upstream_ungrouped_regex_shape() {
    let engine = Engine::new();

    let anchored = engine
        .parse_metadata_sync("  C4Context\nPerson(a, \"A\")")
        .unwrap();
    assert_eq!(anchored.diagram_type, "c4");

    let ungrouped_anywhere = engine.parse_metadata_sync("kanban\nC4Container").unwrap();
    assert_eq!(ungrouped_anywhere.diagram_type, "c4");

    let err = engine
        .parse_metadata_sync("not a diagram C4Context")
        .unwrap_err();
    assert!(
        err.to_string()
            .contains("No diagram type detected matching given configuration"),
        "unexpected error: {err}"
    );
}

#[test]
fn detector_registry_strips_mermaid_comment_lines_without_regex() {
    let registry = DetectorRegistry::pinned_mermaid_baseline();
    let mut config = MermaidConfig::empty_object();

    for source in [
        "\n\n%% This is a comment\nflowchart TD\nA-->B\n",
        "    %% This is a comment\nflowchart TD\nA-->B\n",
        "flowchart TD\nA-->B\n%% This is a comment",
        "%%{init: {'theme': 'forest'}}%%\nflowchart TD\nA-->B\n",
    ] {
        let detected = registry
            .detect_type(source, &mut config)
            .expect("detect type");
        assert_eq!(detected, "flowchart-v2", "source: {source:?}");
    }
}

#[test]
#[cfg(feature = "diagram-flowchart")]
fn leading_utf8_bom_is_handled_consistently_across_public_entrypoints() {
    let source = "\u{feff}flowchart TD\nA-->B\n";
    let registry = DetectorRegistry::pinned_mermaid_baseline();
    let mut config = MermaidConfig::empty_object();

    let detected = registry
        .detect_type(source, &mut config)
        .expect("detector accepts a leading UTF-8 BOM");
    assert_eq!(detected, "flowchart-v2");

    let preprocessed =
        preprocess_diagram(source, &registry).expect("preprocessor accepts a leading UTF-8 BOM");
    assert_eq!(preprocessed.code(), "flowchart TD\nA-->B\n");
    let mapped = preprocessed
        .source
        .try_map_span(SourceSpan::new(0, "flowchart".len()))
        .expect("diagram header remains exactly mapped");
    assert_eq!(&source[mapped.start..mapped.end], "flowchart");

    let parsed = Engine::new()
        .parse_diagram_sync(source, ParseOptions::strict())
        .expect("public parse accepts a leading UTF-8 BOM")
        .expect("flowchart model");
    assert_eq!(parsed.meta.diagram_type, "flowchart-v2");

    let metadata = Engine::new()
        .parse_metadata_sync("\u{feff}---\ntitle: BOM title\n---\nflowchart TD\nA-->B\n")
        .expect("BOM is removed before frontmatter extraction");
    assert_eq!(metadata.title.as_deref(), Some("BOM title"));
}

#[test]
#[cfg(feature = "diagram-flowchart")]
fn malformed_directive_json_is_removed_without_rejecting_the_diagram() {
    let source = "%%{init: {\"theme\": }}%%\nflowchart TD\nA-->B\n";
    let registry = DetectorRegistry::pinned_mermaid_baseline();

    let preprocessed =
        preprocess_diagram(source, &registry).expect("invalid directive config is fail-soft");
    assert_eq!(preprocessed.code(), "flowchart TD\nA-->B\n");
    assert!(preprocessed.config.is_empty_object());

    let parsed = Engine::new()
        .parse_diagram_sync(source, ParseOptions::strict())
        .expect("invalid directive config does not reject a valid diagram")
        .expect("flowchart model");
    assert_eq!(parsed.meta.diagram_type, "flowchart-v2");
}

#[test]
fn strict_unterminated_directive_marker_truncates_like_mermaid() {
    let source = "%%{\nflowchart TD\nA-->B\n";
    let registry = DetectorRegistry::pinned_mermaid_baseline();
    let mut config = MermaidConfig::empty_object();

    assert!(registry.detect_type(source, &mut config).is_err());

    let preprocessed = preprocess_diagram(source, &registry)
        .expect("strict preprocessing truncates an unterminated directive");
    assert_eq!(preprocessed.code(), "");

    let error = Engine::new()
        .parse_diagram_sync(source, ParseOptions::strict())
        .unwrap_err();
    assert!(matches!(error, crate::Error::DetectType(_)));
}

#[test]
#[cfg(feature = "diagram-flowchart")]
fn lenient_unterminated_directive_marker_recovers_the_following_diagram() {
    let source = concat!(
        "%%{init: {\"config\": {\"curve\": \"linear\"}}}%%\n",
        "%%{\n",
        "flowchart TD\nA-->B\n",
    );

    let parsed = Engine::new()
        .parse_diagram_sync(source, ParseOptions::lenient())
        .expect("lenient preprocessing recovers an unterminated directive")
        .expect("flowchart model");
    assert_eq!(parsed.meta.diagram_type, "flowchart-v2");
    assert_eq!(
        parsed.meta.config.get_str("flowchart.curve"),
        Some("linear")
    );
}

#[test]
fn editor_unterminated_directive_marker_recovers_the_following_diagram() {
    let source = "%%{\nflowchart TD\nA-->B\n";

    let snapshot = Engine::new()
        .parse_diagram_snapshot_sync(source)
        .expect("editor preprocessing recovers an unterminated directive")
        .expect("flowchart snapshot");

    assert_eq!(snapshot.metadata().diagram_type, "flowchart-v2");
}

#[test]
fn preprocess_strips_mermaid_comment_at_eof_without_regex() {
    let registry = DetectorRegistry::pinned_mermaid_baseline();
    let result = preprocess_diagram("flowchart TD\nA-->B\n%% This is a comment", &registry)
        .expect("preprocess succeeds");

    assert_eq!(result.code(), "flowchart TD\nA-->B\n");
}

#[test]
fn preprocess_normalizes_crlf_without_regex() {
    let registry = DetectorRegistry::pinned_mermaid_baseline();
    let result = preprocess_diagram("flowchart TD\r\nA-->B\r%% This is a comment", &registry)
        .expect("preprocess succeeds");

    assert_eq!(result.code(), "flowchart TD\nA-->B\n");
}

#[test]
fn preprocess_encodes_entities_without_entity_regex() {
    let registry = DetectorRegistry::pinned_mermaid_baseline();
    let result = preprocess_diagram("flowchart TD\nA[#there;]\nB[#77653;]", &registry)
        .expect("preprocess succeeds");

    assert!(
        result.code().contains("A[ﬂ°there¶ß]"),
        "{:?}",
        result.code()
    );
    assert!(
        result.code().contains("B[ﬂ°°77653¶ß]"),
        "{:?}",
        result.code()
    );
}

#[test]
fn preprocess_rewrites_html_attributes_without_regex() {
    let registry = DetectorRegistry::pinned_mermaid_baseline();
    let result = preprocess_diagram(
        r#"flowchart TD
A["<span title="alpha" data-empty="">Label</span>"]
B["<é title="unchanged">Local</é>"]"#,
        &registry,
    )
    .expect("preprocess succeeds");

    assert!(
        result
            .code()
            .contains(r#"A["<span title='alpha' data-empty=''>Label</span>"]"#),
        "{:?}",
        result.code()
    );
    assert!(
        result
            .code()
            .contains(r#"B["<é title="unchanged">Local</é>"]"#),
        "{:?}",
        result.code()
    );
}

#[test]
fn detector_registry_strips_deep_frontmatter_with_small_stack() {
    const DEPTH: usize = 512;
    let mut text = String::from("---\nconfig: {\"sequence\": ");
    for idx in 0..DEPTH {
        write!(&mut text, r#"{{"k{idx}":"#).expect("write frontmatter config");
    }
    text.push_str("\"leaf\"");
    for _ in 0..DEPTH {
        text.push('}');
    }
    text.push_str("}\n---\nsequenceDiagram\nAlice->Bob: Hi\n");
    let registry = DetectorRegistry::pinned_mermaid_baseline();

    let handle = std::thread::Builder::new()
        .name("detector-deep-frontmatter-strip".to_string())
        .stack_size(64 * 1024)
        .spawn(move || {
            let mut config = MermaidConfig::empty_object();
            let detected = registry
                .detect_type(&text, &mut config)
                .expect("detect type");
            assert_eq!(detected, "sequence");
        })
        .expect("spawn detector deep frontmatter test");
    handle
        .join()
        .expect("detector frontmatter stripping should finish without stack overflow");
}

#[test]
fn detector_registry_requires_matching_frontmatter_indentation() {
    let registry = DetectorRegistry::pinned_mermaid_baseline();

    let mut config = MermaidConfig::empty_object();
    let detected = registry
        .detect_type(
            "   ---\n   title: Flow\n   ---\n   sequenceDiagram\n   Alice->Bob: Hi\n",
            &mut config,
        )
        .expect("matching indented frontmatter should be stripped before detection");
    assert_eq!(detected, "sequence");

    let mut config = MermaidConfig::empty_object();
    let detected = registry
        .detect_type(
            "   ---\ntitle: Flow\n---\nsequenceDiagram\nAlice->Bob: Hi\n",
            &mut config,
        )
        .expect("mismatched frontmatter should remain visible to the pseudo detector");
    assert_eq!(detected, "---");

    let mut config = MermaidConfig::empty_object();
    let detected = registry
        .detect_type(
            "---\ntitle: Flow\n   ---\nsequenceDiagram\nAlice->Bob: Hi\n",
            &mut config,
        )
        .expect("indented closing delimiter must not close column-zero frontmatter");
    assert_eq!(detected, "---");
}

#[test]
fn auto_detect_common_headers_with_deep_config_small_stack() {
    const DEPTH: usize = 1_024;
    let mut value = Value::String("#778899".to_string());
    for idx in (0..DEPTH).rev() {
        let mut map = serde_json::Map::new();
        map.insert(format!("k{idx}"), value);
        value = Value::Object(map);
    }
    let mut root = serde_json::Map::new();
    root.insert("retainedConfig".to_string(), value);
    let engine = Engine::new().with_site_config(MermaidConfig::from_value(Value::Object(root)));

    let handle = std::thread::Builder::new()
        .name("detect-common-headers-deep-config".to_string())
        .stack_size(128 * 1024)
        .spawn(move || {
            for (source, expected_type) in [
                ("block\n  A\n", "block"),
                ("sankey\nA,B,1\n", "sankey"),
                ("treemap\n\"A\": 1\n", "treemap"),
                ("C4Context\nPerson(a, \"A\")\n", "c4"),
            ] {
                let meta = engine.parse_metadata_sync(source).expect("parse succeeds");
                assert_eq!(meta.diagram_type, expected_type);
            }
        })
        .expect("spawn common header detect test");
    handle
        .join()
        .expect("common header detection should finish without stack overflow");
}
