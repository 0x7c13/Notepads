//! Consumer-level contracts for selected parser implementations.
use merman_core::{Engine, Error, ParseOptions, RenderSemanticModel};

#[test]
fn unknown_input_retains_detection_and_suppression_semantics() {
    let engine = Engine::new();
    let source = "not-a-mermaid-diagram\n";
    assert!(matches!(
        engine.parse_diagram_sync(source, ParseOptions::strict()),
        Err(Error::DetectType(_))
    ));
    assert!(
        engine
            .parse_diagram_sync(source, ParseOptions::lenient())
            .unwrap()
            .is_none()
    );
}

#[cfg(not(feature = "diagram-sequence"))]
#[test]
fn disabled_sequence_retains_identity_errors_and_editor_diagnostics() {
    let engine = Engine::new();
    let source = "sequenceDiagram\nAlice->>Bob: Hello\n";
    assert_eq!(
        engine.parse_metadata_sync(source).unwrap().diagram_type,
        "sequence"
    );
    assert!(!merman_core::supported_diagrams().contains(&"sequence"));
    let facts = merman_core::diagram_family_capabilities()
        .iter()
        .find(|fact| fact.diagram_type == "sequence")
        .unwrap();
    assert!(facts.has_detector && facts.has_header);
    assert!(!facts.has_semantic_parser && !facts.has_render_parser && !facts.has_editor_parser);
    assert!(
        matches!(engine.parse_diagram_sync(source, ParseOptions::strict()), Err(Error::UnsupportedDiagram { diagram_type }) if diagram_type == "sequence")
    );
    assert!(
        matches!(engine.parse_diagram_for_render_model_sync(source, ParseOptions::strict()), Err(Error::UnsupportedDiagram { diagram_type }) if diagram_type == "sequence")
    );
    assert_eq!(
        engine
            .parse_diagram_sync(source, ParseOptions::lenient())
            .unwrap()
            .unwrap()
            .meta
            .diagram_type,
        "error"
    );
    let suppressed = engine
        .parse_diagram_for_render_model_sync(source, ParseOptions::lenient())
        .unwrap()
        .unwrap();
    assert!(matches!(suppressed.model(), RenderSemanticModel::Error(_)));
    let snapshot = engine.parse_diagram_snapshot_sync(source).unwrap().unwrap();
    assert!(
        matches!(snapshot.outcome(), merman_core::DiagramParseOutcome::Failed(Error::UnsupportedDiagram { diagram_type }) if diagram_type == "sequence")
    );
    assert!(matches!(
        snapshot.editor_facts(),
        merman_core::ParsedEditorFacts::Unavailable
    ));
}

#[cfg(not(feature = "diagram-sequence"))]
#[test]
fn disabled_builtin_accepts_custom_overlays_without_claiming_builtin_provenance() {
    let mut engine = Engine::new();
    engine
        .diagram_registry_mut()
        .insert("sequence", |_, _, control| {
            control.checkpoint()?;
            Ok(Ok(serde_json::json!({"owner": "semantic"})))
        });
    let source = "sequenceDiagram\nAlice->>Bob: Hello\n";
    let parsed = engine
        .parse_diagram_for_render_model_sync(source, ParseOptions::strict())
        .unwrap()
        .unwrap();
    let RenderSemanticModel::CustomJson(custom) = parsed.model() else {
        panic!("expected custom semantic overlay");
    };
    assert_eq!(custom.value()["owner"], "semantic");
    assert!(!parsed.model().supports_diagram_type("sequence"));
    engine
        .render_diagram_registry_mut()
        .insert("sequence", |_, _, control| {
            control.checkpoint()?;
            Ok(Ok(merman_core::CustomJsonRenderModel::new(
                "sequence",
                serde_json::json!({"owner": "render"}),
            )))
        });
    let parsed = engine
        .parse_diagram_for_render_model_sync(source, ParseOptions::strict())
        .unwrap()
        .unwrap();
    let RenderSemanticModel::CustomJson(custom) = parsed.model() else {
        panic!("expected custom render overlay");
    };
    assert_eq!(custom.value()["owner"], "render");
    assert!(!parsed.model().supports_diagram_type("sequence"));
}

#[test]
fn cancellation_is_not_suppressed_for_unavailable_or_unknown_input() {
    let engine = Engine::new();
    let control = merman_core::OperationControl::new();
    control.cancel();
    for source in [
        "sequenceDiagram\nAlice->>Bob: Hello\n",
        "not-a-mermaid-diagram\n",
    ] {
        let cancelled = engine
            .parse_diagram_for_render_model_controlled_sync(
                source,
                ParseOptions::lenient(),
                &control,
            )
            .unwrap_err();
        assert_eq!(cancelled.reason, merman_core::CancelReason::Requested);
    }
}

#[cfg(feature = "diagram-gantt")]
#[test]
fn selected_gantt_preserves_typed_and_json_projections() {
    let engine = Engine::new();
    let source = "gantt\ndateFormat YYYY-MM-DD\nsection Work\nTask :a, 2024-01-01, 1d\n";
    let json = engine
        .parse_diagram_sync(source, ParseOptions::strict())
        .unwrap()
        .unwrap();
    let typed = engine
        .parse_diagram_for_render_model_sync(source, ParseOptions::strict())
        .unwrap()
        .unwrap();
    assert!(matches!(typed.model(), RenderSemanticModel::Gantt(_)));
    assert_eq!(
        typed.model().compatibility_json(typed.metadata()).unwrap(),
        json.model
    );
}

#[cfg(any(
    feature = "diagram-flowchart",
    feature = "diagram-swimlane",
    feature = "diagram-agentflow"
))]
#[test]
fn shared_flowchart_model_does_not_enable_the_other_language() {
    let engine = Engine::new();
    for (source, enabled) in [
        ("flowchart TD\nA-->B\n", cfg!(feature = "diagram-flowchart")),
        ("graph TD\nA-->B\n", cfg!(feature = "diagram-flowchart")),
        (
            "swimlane-beta LR\nA-->B\n",
            cfg!(feature = "diagram-swimlane"),
        ),
    ] {
        let result = engine.parse_diagram_for_render_model_sync(source, ParseOptions::strict());
        if enabled {
            let parsed = result.unwrap().unwrap();
            assert_eq!(parsed.model().kind(), "flowchart");
        } else {
            assert!(matches!(result, Err(Error::UnsupportedDiagram { .. })));
        }
    }
}

#[test]
fn mermaid_12_families_preserve_identity_and_select_only_their_implementation() {
    let engine = Engine::new();
    for (family, source, enabled) in [
        (
            "agentflow",
            "agentflow-beta\nA --> B\n",
            cfg!(feature = "diagram-agentflow"),
        ),
        (
            "usecase",
            "usecase-beta\nactor User\nUser --> Login\n",
            cfg!(feature = "diagram-usecase"),
        ),
    ] {
        assert_eq!(
            engine.parse_metadata_sync(source).unwrap().diagram_type,
            family
        );
        let capability = merman_core::diagram_family_capabilities()
            .iter()
            .find(|fact| fact.diagram_type == family)
            .expect("known Mermaid 12 family");
        assert!(capability.has_detector && capability.has_header);
        assert_eq!(capability.has_semantic_parser, enabled);
        assert_eq!(capability.has_render_parser, enabled);
        assert_eq!(capability.has_editor_parser, enabled);
        let semantic = engine.parse_diagram_sync(source, ParseOptions::strict());
        let typed = engine.parse_diagram_for_render_model_sync(source, ParseOptions::strict());
        if enabled {
            let semantic = semantic.unwrap().unwrap();
            let typed = typed.unwrap().unwrap();
            assert_eq!(typed.model().kind(), family);
            assert_eq!(
                typed.model().compatibility_json(typed.metadata()).unwrap(),
                semantic.model
            );
        } else {
            assert!(
                matches!(semantic, Err(Error::UnsupportedDiagram { diagram_type }) if diagram_type == family)
            );
            assert!(
                matches!(typed, Err(Error::UnsupportedDiagram { diagram_type }) if diagram_type == family)
            );
        }
    }
}
