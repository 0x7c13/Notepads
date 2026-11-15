use super::*;
use crate::{
    Engine, MermaidConfig, OperationControl, ParseMetadata, ParseOptions, RenderSemanticModel,
};

fn model(source: &str) -> StateDiagramRenderModel {
    let parsed = Engine::new()
        .parse_diagram_for_render_model_sync(source, ParseOptions::strict())
        .expect("state parses")
        .expect("state detected");
    let RenderSemanticModel::State(model) = parsed.model() else {
        panic!("expected state model");
    };
    model.clone()
}

fn chain(depth: usize) -> String {
    let mut source = String::from("stateDiagram-v2\n");
    for index in 0..depth {
        source.push_str(&format!("state S{index} {{\n"));
    }
    source.push_str("Leaf\n");
    for _ in 0..depth {
        source.push_str("}\n");
    }
    source
}

fn meta() -> ParseMetadata {
    ParseMetadata {
        diagram_type: "stateDiagram-v2".to_string(),
        config: MermaidConfig::empty_object(),
        effective_config: MermaidConfig::empty_object(),
        title: None,
    }
}

fn compatibility(model: &StateDiagramRenderModel) -> crate::ManagedSemanticJson {
    crate::ManagedSemanticJson::from(
        render_model_to_compat_json(model, &meta()).expect("state compatibility projection"),
    )
}

#[test]
fn state_flat_documents_preserve_dividers_and_nested_semantic_order() {
    let model = model(concat!(
        "stateDiagram-v2\ndirection LR\n",
        "state Outer {\ndirection RL\nstate Inner {\n",
        "[*] --> A: ready\n--\nB --> [*]: done\n}\n",
        "--\nnote right of C : note\nC --> D: next\n}\nOuter --> E\n",
    ));
    let root = model.document.root().expect("canonical root");
    let statements = model.document.get(root).expect("root statements");
    assert_eq!(statements.len(), 3);
    assert_eq!(
        model.document.documents.iter().map(Vec::len).sum::<usize>(),
        13
    );
    let outer = model.states["Outer"].doc.expect("outer document id");
    let partitions = model.document.get(outer).expect("outer partitions");
    assert_eq!(partitions.len(), 2);
    let StateStatement::State(first) = &partitions[0] else {
        panic!("first divider")
    };
    assert_eq!(first.id, "divider-id-2");
    let first_doc = model
        .document
        .get(first.doc.expect("first divider document"))
        .unwrap();
    assert!(matches!(first_doc[0], StateStatement::Direction(ref direction) if direction == "RL"));
    let StateStatement::State(inner) = &first_doc[1] else {
        panic!("inner composite")
    };
    let inner_parts = model.document.get(inner.doc.unwrap()).unwrap();
    let StateStatement::State(inner_first) = &inner_parts[0] else {
        panic!("inner divider")
    };
    assert_eq!(inner_first.id, "divider-id-1");
    let json = compatibility(&model);
    let doc = &json["states"]["Outer"]["doc"];
    assert_eq!(
        doc[0]["doc"][1]["doc"][0]["doc"][0]["state1"]["id"],
        "divider-id-1_start"
    );
    assert_eq!(doc[0]["doc"][1]["doc"][0]["doc"][0]["description"], "ready");
    assert_eq!(model.direction, "LR");
    let a = model.nodes.iter().find(|node| node.id == "A").unwrap();
    assert_eq!(a.parent_id.as_deref(), Some("divider-id-1"));
    assert_eq!(
        model
            .edges
            .iter()
            .filter(|edge| edge.classes == "transition")
            .map(|edge| edge.label.as_str())
            .collect::<Vec<_>>(),
        ["ready", "done", "next", ""]
    );
    assert_eq!(
        model
            .nodes
            .iter()
            .filter(|node| node.shape == "note")
            .count(),
        1
    );
}

#[test]
fn state_compatibility_projection_preserves_nested_documents() {
    for source in [
        "stateDiagram-v2\nA --> B: label\nnote right of A : note\n",
        "stateDiagram-v2\naccTitle: title\naccDescr: description\nstate A {\ndirection LR\nstate B {\nC:::active\n}\n}\nclassDef active fill:red\nclick A href \"https://example.com\"\n",
        "stateDiagram-v2\nstate A {\nB\n--\nC\n}\n",
        "stateDiagram-v2\n",
    ] {
        let model = model(source);
        let json = compatibility(&model);
        assert!(json["states"].is_object());
        assert_eq!(json["type"], "stateDiagram-v2");
        assert!(json["config"].is_object());
        assert!(json.get("document").is_none());
    }
}

#[test]
fn state_compatibility_projection_supports_deep_documents() {
    let model = model(&chain(130));
    let json = compatibility(&model);
    let mut output = Vec::new();
    json.write_json(&mut output)
        .expect("deep state compatibility JSON");
    assert!(output.starts_with(b"{"));
}

#[test]
fn state_compatibility_projection_handles_shared_documents() {
    let mut model = model(&chain(130));
    let doc = model.states["S0"].doc;
    for index in 0..32 {
        let id = format!("Shared{index}");
        model.states.insert(
            id.clone(),
            StateDiagramRenderState {
                id,
                doc,
                ..StateDiagramRenderState::default()
            },
        );
    }
    let json = compatibility(&model);
    assert!(json["states"].is_object());
}

#[test]
fn state_divider_assignment_cancellation_releases_completed_grammar_documents() {
    let mut source = String::from("stateDiagram-v2\nstate Outer {\n");
    for index in 0..300 {
        source.push_str(&format!("S{index}\n--\n"));
    }
    source.push_str("}\n");
    let mut document = StateDocument::default();
    let root = state_grammar::RootParser::new()
        .parse(&mut document, Lexer::new(&source))
        .unwrap();
    document.root = Some(root);
    let control = OperationControl::new();
    control.cancel_after_checkpoints(1);
    let mut count = 0;
    let result = parse::assign_divider_ids(&mut document, &mut count, &control);
    assert!(result.is_err());
    assert!(
        count > 0 && count < 300,
        "assignment had useful work before cancellation"
    );
    drop(document);
    assert_eq!(model("stateDiagram-v2\nA\n").nodes.len(), 1);
}

#[test]
fn state_projection_cancellation_releases_completed_inner_documents() {
    let model = model(&chain(1000));
    let control = OperationControl::new();
    // One state-record checkpoint and one at each 128 projection steps. This reaches the
    // ascent, where completed nested JSON values already live in managed frame collections.
    control.cancel_after_checkpoints(10);
    render_model::reset_projected_document_count();
    assert!(matches!(
        render_model_to_compat_json_controlled(&model, &meta(), &control),
        Err(crate::OperationCancelled { .. })
    ));
    let completed = render_model::projected_document_count();
    assert!(
        completed > 0 && completed < 1000,
        "inner documents completed before projection cancellation"
    );
    let mut output = Vec::new();
    compatibility(&model).write_json(&mut output).unwrap();
    assert!(!output.is_empty());
}

#[test]
fn state_foreign_document_ids_report_projection_failure() {
    let foreign = model(&chain(3));
    let mut target = model("stateDiagram-v2\nA\n");
    target.states.get_mut("A").unwrap().doc = foreign.states["S0"].doc;
    assert!(render_model_to_compat_json(&target, &meta()).is_err());
    assert_eq!(
        target.model_complexity(),
        model("stateDiagram-v2\nA\n").model_complexity()
    );
}
