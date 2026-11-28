use crate::*;
use futures::executor::block_on;
use serde_json::json;

#[test]
fn parse_swimlane_reuses_flowchart_semantics_and_editor_facts() {
    let engine = Engine::new();
    let text = "swimlane-beta LR\nA[Start] --> B[Done]\n";
    let parsed = engine
        .parse_diagram_snapshot_sync(text)
        .unwrap()
        .expect("swimlane parses through flowchart semantics");

    assert_eq!(parsed.metadata().diagram_type, "swimlane");
    assert_eq!(
        parsed.metadata().effective_config.get_str("layout"),
        Some("swimlane")
    );
    assert_eq!(
        parsed
            .outcome()
            .parsed_model()
            .expect("expected parsed snapshot")["type"],
        json!("swimlane")
    );
    assert_eq!(
        parsed
            .outcome()
            .parsed_model()
            .expect("expected parsed snapshot")["keyword"],
        json!("swimlane-beta")
    );
    assert_eq!(
        parsed
            .outcome()
            .parsed_model()
            .expect("expected parsed snapshot")["direction"],
        json!("LR")
    );
    assert_eq!(
        parsed
            .outcome()
            .parsed_model()
            .expect("expected parsed snapshot")["nodes"][0]["id"],
        json!("A")
    );
    assert_eq!(
        parsed
            .outcome()
            .parsed_model()
            .expect("expected parsed snapshot")["edges"][0]["from"],
        json!("A")
    );

    let ParsedEditorFacts::Available(facts) = parsed.editor_facts() else {
        panic!("swimlane should reuse flowchart editor facts");
    };
    let a_start = text.find("A[").expect("A node");
    let a = facts
        .symbols
        .iter()
        .find(|symbol| symbol.name == "A")
        .expect("A editor symbol");
    assert_eq!(a.selection.start, a_start);
    assert_eq!(a.selection.end, a_start + "A".len());
}

#[test]
fn parse_swimlane_reuses_flowchart_apostrophe_semantics() {
    let engine = Engine::new();
    let text = "swimlane-beta LR\nsubgraph Supplier\nA[Update the RFQs based on the supplier's response]\nB[Done]\nend\nA -->|'Owner's review'| B\n";
    let parsed = block_on(engine.parse_diagram(text, ParseOptions::strict()))
        .expect("Swimlane accepts apostrophes through the shared Flowchart parser")
        .expect("swimlane diagram detected");

    assert_eq!(parsed.meta.diagram_type, "swimlane");
    assert_eq!(
        parsed.model["nodes"][0]["label"],
        json!("Update the RFQs based on the supplier's response")
    );
    assert_eq!(parsed.model["nodes"][0]["labelType"], json!("text"));
    assert_eq!(parsed.model["edges"][0]["label"], json!("'Owner's review'"));
    assert_eq!(parsed.model["edges"][0]["labelType"], json!("text"));
}

#[test]
fn parse_swimlane_layout_default_respects_user_config_precedence() {
    let default = Engine::new()
        .parse_metadata_sync("swimlane-beta LR\nA-->B\n")
        .unwrap();
    assert_eq!(default.effective_config.get_str("layout"), Some("swimlane"));
    // Mermaid 12 gives an explicit initialized layout precedence over the family default.
    let engine = Engine::new().with_site_config(MermaidConfig::from_value(json!({
        "layout": "dagre"
    })));

    let site_default = engine
        .parse_metadata_sync("swimlane-beta LR\nA-->B\n")
        .expect("swimlane metadata");
    assert_eq!(
        site_default.effective_config.get_str("layout"),
        Some("dagre")
    );

    let user_override = engine
        .parse_metadata_sync("%%{init: {\"layout\": \"elk\"}}%%\nswimlane-beta LR\nA-->B\n")
        .expect("swimlane metadata with user layout");
    assert_eq!(user_override.config.get_str("layout"), Some("elk"));
    assert_eq!(
        user_override.effective_config.get_str("layout"),
        Some("elk")
    );

    let cleared_override = engine
        .parse_metadata_sync("%%{init: {\"layout\": null}}%%\nswimlane-beta LR\nA-->B\n")
        .expect("swimlane metadata with a null layout override");
    assert_eq!(
        cleared_override.effective_config.get_str("layout"),
        Some("dagre")
    );

    let known_type = engine
        .parse_metadata_with_type_sync("swimlane", "swimlane-beta LR\nA-->B\n")
        .expect("known-type swimlane metadata");
    assert_eq!(known_type.effective_config.get_str("layout"), Some("dagre"));
}

#[test]
fn parse_swimlane_render_model_reuses_flowchart_semantics() {
    let engine = Engine::new();
    let parsed = engine
        .parse_diagram_for_render_model_sync("swimlane-beta LR\nA-->B\n", ParseOptions::strict())
        .expect("swimlane render parse succeeds")
        .expect("swimlane render model");

    assert_eq!(parsed.metadata().diagram_type, "swimlane");
    assert_eq!(
        parsed.metadata().effective_config.get_str("layout"),
        Some("swimlane")
    );
    let RenderSemanticModel::Flowchart(model) = parsed.model() else {
        panic!("swimlane should reuse the flowchart semantic model");
    };
    assert_eq!(model.keyword, "swimlane-beta");
    assert_eq!(model.direction.as_deref(), Some("LR"));
    assert_eq!(model.nodes.len(), 2);
    assert_eq!(model.edges.len(), 1);
}
