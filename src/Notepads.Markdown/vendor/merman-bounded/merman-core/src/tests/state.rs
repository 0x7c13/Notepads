use crate::*;
use futures::executor::block_on;
use serde_json::json;

#[test]
fn parse_diagram_state_v2_alias_and_colon_description() {
    let engine = Engine::new();

    let res = block_on(engine.parse_diagram(
        r#"stateDiagram-v2
state "Small State 1" as namedState1"#,
        ParseOptions::default(),
    ))
    .unwrap()
    .unwrap();
    assert_eq!(res.meta.diagram_type, "stateDiagram");
    assert_eq!(
        res.model["states"]["namedState1"]["descriptions"][0],
        json!("Small State 1")
    );

    let res = block_on(engine.parse_diagram(
        r#"stateDiagram-v2
namedState1 : Small State 1"#,
        ParseOptions::default(),
    ))
    .unwrap()
    .unwrap();
    assert_eq!(
        res.model["states"]["namedState1"]["descriptions"][0],
        json!("Small State 1")
    );

    let res = block_on(engine.parse_diagram(
        r#"stateDiagram-v2
namedState1:Small State 1"#,
        ParseOptions::default(),
    ))
    .unwrap()
    .unwrap();
    assert_eq!(
        res.model["states"]["namedState1"]["descriptions"][0],
        json!("Small State 1")
    );
}

#[test]
fn parse_diagram_state_v2_multibyte_ids_do_not_panic() {
    let engine = Engine::new();

    let res = block_on(engine.parse_diagram(
        r#"stateDiagram-v2
顧客 --> 完了: 送信
"#,
        ParseOptions::default(),
    ))
    .unwrap()
    .unwrap();

    assert_eq!(res.model["relations"][0]["id1"], json!("顧客"));
    assert_eq!(res.model["relations"][0]["id2"], json!("完了"));
    assert_eq!(res.model["edges"][0]["label"], json!("送信"));
}

#[test]
fn parse_diagram_state_v2_accepts_ecmascript_unicode_whitespace() {
    let engine = Engine::new();
    for whitespace in ['\u{00A0}', '\u{202F}', '\u{3000}', '\u{FEFF}'] {
        for newline in ["\n", "\r\n"] {
            let text = format!(
                "stateDiagram-v2{newline}state{whitespace}\"正常\" as{whitespace}用户{whitespace}{newline}\
                 用户{whitespace}-->{whitespace}完成{newline}"
            );
            let res = engine
                .parse_diagram_sync(&text, ParseOptions::default())
                .unwrap()
                .unwrap();
            assert_eq!(
                res.model["states"]["用户"]["descriptions"][0], "正常",
                "{text:?}"
            );
            assert_eq!(res.model["states"].as_object().unwrap().len(), 2);
            assert_eq!(res.model["edges"].as_array().unwrap().len(), 1);
        }
    }
}

#[test]
fn parse_diagram_state_v2_accepts_unicode_whitespace_in_direction() {
    let engine = Engine::new();
    let text = "stateDiagram-v2\ndirection\u{3000}LR\nA --> B\n";

    let res = engine
        .parse_diagram_sync(text, ParseOptions::default())
        .unwrap()
        .unwrap();
    assert_eq!(res.model["direction"], json!("LR"));
}

#[test]
fn parse_diagram_state_v2_accepts_unicode_whitespace_before_composite_block() {
    let engine = Engine::new();
    let text =
        "stateDiagram-v2\nstate\u{3000}外部\u{3000}\n\u{3000}{\n用户\u{3000}-->\u{3000}完成\n}\n";
    let res = engine
        .parse_diagram_sync(text, ParseOptions::default())
        .unwrap()
        .unwrap();
    assert_eq!(
        res.model["states"]["外部"]["doc"][0]["state1"]["id"],
        "用户"
    );
    assert_eq!(
        res.model["states"]["外部"]["doc"][0]["state2"]["id"],
        "完成"
    );
}

#[test]
fn parse_diagram_state_v2_preserves_colons_in_transition_labels() {
    let res = block_on(Engine::new().parse_diagram(
        r#"stateDiagram-v2
Active --> Deleted: DELETE /users/:id"#,
        ParseOptions::default(),
    ))
    .unwrap()
    .unwrap();

    assert_eq!(res.model["edges"][0]["label"], json!("DELETE /users/:id"));
}

#[test]
fn parse_diagram_state_v2_groups_and_unsafe_ids() {
    let engine = Engine::new();

    let res = block_on(engine.parse_diagram(
        r#"stateDiagram-v2
state "Small State 1" as namedState1
state "Big State 1" as bigState1 {
  bigState1InternalState
}
namedState1 --> bigState1: should point to \nBig State 1 container

state "Small State 2" as namedState2
state bigState2 {
  bigState2InternalState
}
namedState2 --> bigState2: should point to \nbigState2 container"#,
        ParseOptions::default(),
    ))
    .unwrap()
    .unwrap();

    assert_eq!(
        res.model["states"]["bigState1"]["doc"][0]["id"],
        json!("bigState1InternalState")
    );
    assert_eq!(
        res.model["states"]["bigState2"]["doc"][0]["id"],
        json!("bigState2InternalState")
    );
    assert_eq!(res.model["relations"][0]["id1"], json!("namedState1"));
    assert_eq!(res.model["relations"][0]["id2"], json!("bigState1"));
    assert_eq!(res.model["relations"][1]["id1"], json!("namedState2"));
    assert_eq!(res.model["relations"][1]["id2"], json!("bigState2"));

    let res = block_on(engine.parse_diagram(
        r#"stateDiagram-v2
[*] --> __proto__
__proto__ --> [*]"#,
        ParseOptions::default(),
    ))
    .unwrap()
    .unwrap();
    assert!(res.model["states"]["__proto__"].is_object());
    assert!(res.model["states"]["root_start"].is_object());
    assert!(res.model["states"]["root_end"].is_object());
}

#[test]
fn parse_diagram_state_v2_tracks_explicit_composite_direction() {
    let res = block_on(Engine::new().parse_diagram(
        r#"stateDiagram-v2
state Implicit {
  A
}
state Explicit {
  direction LR
  B
}
"#,
        ParseOptions::default(),
    ))
    .unwrap()
    .unwrap();

    let nodes = res.model["nodes"].as_array().expect("state render nodes");
    let node = |id: &str| {
        nodes
            .iter()
            .find(|node| node["id"] == id)
            .unwrap_or_else(|| panic!("missing state node {id}"))
    };

    assert_eq!(node("Implicit")["dir"], json!("TB"));
    assert!(node("Implicit").get("explicitDir").is_none());
    assert_eq!(node("Explicit")["dir"], json!("LR"));
    assert!(node("Explicit").get("explicitDir").is_none());
    assert!(node("A").get("explicitDir").is_none());
    assert!(node("B").get("explicitDir").is_none());
}

#[test]
fn parse_state_render_model_preserves_alias_trailing_description() {
    let input = r#"stateDiagram-v2
state "Display label" as S1: Trailing description
"#;
    let parsed = Engine::new()
        .parse_diagram_for_render_model_sync(input, ParseOptions::strict())
        .unwrap()
        .unwrap();
    let RenderSemanticModel::State(model) = parsed.model() else {
        panic!("expected typed state render model");
    };
    let node = model
        .nodes
        .iter()
        .find(|node| node.id == "S1")
        .expect("state render node S1");

    assert_eq!(node.label, Some(json!("Display label")));
    assert_eq!(
        node.description.as_deref(),
        Some(["Trailing description".to_string()].as_slice())
    );
    assert_eq!(node.shape, "rectWithTitle");

    let parsed = block_on(Engine::new().parse_diagram(input, ParseOptions::strict()))
        .unwrap()
        .unwrap();
    let json_node = parsed.model["nodes"]
        .as_array()
        .expect("JSON state render nodes")
        .iter()
        .find(|node| node["id"] == "S1")
        .expect("JSON state render node S1");

    assert_eq!(json_node.get("label"), node.label.as_ref());
    assert_eq!(
        json_node["description"],
        json!(node.description.as_ref().expect("typed descriptions"))
    );
    assert_eq!(json_node["shape"].as_str(), Some(node.shape.as_str()));
}

#[test]
fn parse_state_render_model_tracks_explicit_composite_direction() {
    let parsed = Engine::new()
        .parse_diagram_for_render_model_sync(
            r#"stateDiagram-v2
state Implicit {
  A
}
state Explicit {
  direction LR
  B
}
"#,
            ParseOptions::strict(),
        )
        .unwrap()
        .unwrap();
    let RenderSemanticModel::State(model) = parsed.model() else {
        panic!("expected typed state render model");
    };
    let node = |id: &str| {
        model
            .nodes
            .iter()
            .find(|node| node.id == id)
            .unwrap_or_else(|| panic!("missing state render node {id}"))
    };

    assert_eq!(node("Implicit").dir.as_deref(), Some("TB"));
    assert_eq!(node("Implicit").explicit_dir, Some(false));
    assert_eq!(node("Explicit").dir.as_deref(), Some("LR"));
    assert_eq!(node("Explicit").explicit_dir, Some(true));
    assert_eq!(node("A").explicit_dir, None);
    assert_eq!(node("B").explicit_dir, None);
}

#[test]
fn parse_diagram_state_rejects_same_line_multi_word_composite_state_name() {
    let engine = Engine::new();
    let text = r#"stateDiagram-v2
state Invalid Name {
  Idle
}
"#;
    crate::diagrams::state::reset_state_syntax_construction_count();
    let Error::DiagramParse { diagnostic, .. } =
        block_on(engine.parse_diagram(text, ParseOptions::default())).unwrap_err()
    else {
        panic!("expected state parse error");
    };
    let offset = text.find("Name").unwrap();

    assert!(
        diagnostic
            .message()
            .contains("State name must be a single word")
    );
    assert_eq!(
        diagnostic.span(),
        Some(SourceSpan::new(offset, offset + "Name".len()))
    );
    assert_eq!(diagnostic.span_kind(), ParseDiagnosticSpanKind::Exact);
    assert_eq!(
        crate::diagrams::state::state_syntax_construction_count(),
        1,
        "strict malformed State parsing must construct one lexical event tape"
    );

    crate::diagrams::state::reset_state_syntax_construction_count();
    let facts = engine
        .parse_editor_semantic_facts_with_type_sync("stateDiagram", text)
        .expect("malformed State editor parse returns recovery facts")
        .expect("malformed State editor facts are available");
    assert_eq!(facts.completeness, EditorSemanticCompleteness::Recovered);
    assert_eq!(
        crate::diagrams::state::state_syntax_construction_count(),
        1,
        "malformed State recovery must reuse one lexical event tape"
    );
    assert!(facts.diagnostics.iter().any(|diagnostic| {
        diagnostic.kind == EditorSemanticDiagnosticKind::ParserRecovery
            && diagnostic.span == Some(SourceSpan::new(offset, offset + "Name".len()))
    }));
}

#[test]
fn parse_diagram_state_keeps_newline_composite_block_compatibility() {
    let engine = Engine::new();
    let res = block_on(engine.parse_diagram(
        r#"stateDiagram-v2
state Valid
{
  Idle
}
"#,
        ParseOptions::default(),
    ))
    .unwrap()
    .unwrap();

    assert_eq!(res.model["states"]["Valid"]["doc"][0]["id"], json!("Idle"));
}

#[test]
fn parse_diagram_state_v2_classdef_class_and_shorthand() {
    let engine = Engine::new();

    let res = block_on(engine.parse_diagram(
        r#"stateDiagram-v2
classDef exampleStyleClass background:#bbb,border:1.5px solid red;
a --> b:::exampleStyleClass
class a exampleStyleClass"#,
        ParseOptions::default(),
    ))
    .unwrap()
    .unwrap();

    assert_eq!(
        res.model["styleClasses"]["exampleStyleClass"]["styles"][0],
        json!("background:#bbb")
    );
    assert_eq!(
        res.model["styleClasses"]["exampleStyleClass"]["styles"][1],
        json!("border:1.5px solid red")
    );
    assert_eq!(
        res.model["states"]["a"]["classes"][0],
        json!("exampleStyleClass")
    );
    assert_eq!(
        res.model["states"]["b"]["classes"][0],
        json!("exampleStyleClass")
    );

    let res = block_on(engine.parse_diagram(
        r#"stateDiagram-v2
classDef exampleStyleClass background:#bbb,border:1px solid red;
[*]:::exampleStyleClass --> b"#,
        ParseOptions::default(),
    ))
    .unwrap()
    .unwrap();
    assert_eq!(
        res.model["states"]["root_start"]["classes"][0],
        json!("exampleStyleClass")
    );

    let res = block_on(engine.parse_diagram(
        r#"stateDiagram-v2
classDef exampleStyleClass background:#bbb,border:1px solid red;
a-->b
class a,b,c, d, e exampleStyleClass"#,
        ParseOptions::default(),
    ))
    .unwrap()
    .unwrap();
    for id in ["a", "b", "c", "d", "e"] {
        assert_eq!(
            res.model["states"][id]["classes"][0],
            json!("exampleStyleClass")
        );
    }
}

#[test]
fn parse_diagram_state_v2_style_statement_sets_node_styles_and_ignores_comments() {
    let engine = Engine::new();

    let res = block_on(engine.parse_diagram(
        r#"stateDiagram-v2
id1
id2
style id1,id2 background:#bbb, font-weight:bold, font-style:italic;"#,
        ParseOptions::default(),
    ))
    .unwrap()
    .unwrap();

    assert_eq!(
        res.model["nodes"][0]["cssStyles"],
        json!(["background:#bbb", "font-weight:bold", "font-style:italic"])
    );
    assert_eq!(
        res.model["nodes"][1]["cssStyles"],
        json!(["background:#bbb", "font-weight:bold", "font-style:italic"])
    );

    let res = block_on(engine.parse_diagram(
        r#"stateDiagram-v2
[*] --> Moving
Moving --> Still
Moving --> Crash
state Moving {
%% comment inside state
slow  --> fast
}"#,
        ParseOptions::default(),
    ))
    .unwrap()
    .unwrap();

    assert_eq!(
        res.model["states"]["Moving"]["doc"]
            .as_array()
            .unwrap()
            .len(),
        1
    );
}

#[test]
fn parse_diagram_state_v2_click_and_href_store_links() {
    let engine = Engine::new();

    let res = block_on(engine.parse_diagram(
        r#"stateDiagram-v2
S1
click S1 "https://example.com" "Go to Example""#,
        ParseOptions::default(),
    ))
    .unwrap()
    .unwrap();
    assert_eq!(
        res.model["links"]["S1"]["url"],
        json!("https://example.com")
    );
    assert_eq!(res.model["links"]["S1"]["tooltip"], json!("Go to Example"));

    let res = block_on(engine.parse_diagram(
        r#"stateDiagram-v2
S0
click S0 "https://example.com/empty" """#,
        ParseOptions::default(),
    ))
    .unwrap()
    .unwrap();
    assert_eq!(res.model["links"]["S0"]["tooltip"], json!(""));

    let res = block_on(engine.parse_diagram(
        r#"stateDiagram-v2
S2
click S2 href "https://example.com""#,
        ParseOptions::default(),
    ))
    .unwrap()
    .unwrap();
    assert_eq!(
        res.model["links"]["S2"]["url"],
        json!("https://example.com")
    );
    assert_eq!(res.model["links"]["S2"]["tooltip"], json!(""));

    let res = block_on(engine.parse_diagram(
        r#"stateDiagram-v2
S3
click S3 href "jav&#x61;script:alert(1)""#,
        ParseOptions::default(),
    ))
    .unwrap()
    .unwrap();
    assert_eq!(
        res.model["links"]["S3"]["url"],
        json!("jav&ﬂ°x61¶ßscript:alert(1)")
    );
}

#[test]
fn parse_diagram_state_v2_repeated_clicks_preserve_declaration_order() {
    let engine = Engine::new();

    let res = block_on(engine.parse_diagram(
        r#"stateDiagram-v2
S1
click S1 "https://example.com/first" "First"
click S1 "https://example.com/last" "Last""#,
        ParseOptions::default(),
    ))
    .unwrap()
    .unwrap();

    assert_eq!(
        res.model["links"]["S1"],
        json!([
            {
                "url": "https://example.com/first",
                "tooltip": "First"
            },
            {
                "url": "https://example.com/last",
                "tooltip": "Last"
            }
        ])
    );
}

#[test]
fn typed_state_links_preserve_declaration_order_and_empty_tooltips() {
    let parsed = Engine::new()
        .parse_diagram_for_render_model_sync(
            r#"stateDiagram-v2
S1
S2
click S1 "https://example.com/first" "First"
click S1 "https://example.com/last" ""
click S2 href "https://example.com/omitted"
"#,
            ParseOptions::strict(),
        )
        .unwrap()
        .unwrap();
    let RenderSemanticModel::State(model) = parsed.model() else {
        panic!("expected typed state render model");
    };

    let crate::diagrams::state::StateDiagramRenderLinks::Many(repeated) =
        model.links.get("S1").expect("S1 links")
    else {
        panic!("expected repeated S1 links");
    };
    assert_eq!(repeated.len(), 2);
    assert_eq!(repeated[0].url, "https://example.com/first");
    assert_eq!(repeated[0].tooltip, "First");
    assert_eq!(repeated[1].url, "https://example.com/last");
    assert_eq!(repeated[1].tooltip, "");

    let crate::diagrams::state::StateDiagramRenderLinks::One(omitted) =
        model.links.get("S2").expect("S2 link")
    else {
        panic!("expected one S2 link");
    };
    assert_eq!(omitted.url, "https://example.com/omitted");
    assert_eq!(omitted.tooltip, "");
}

#[test]
fn parse_diagram_state_v2_note_right_of_and_block_note() {
    let engine = Engine::new();

    let res = block_on(engine.parse_diagram(
        r#"stateDiagram-v2
[*] --> A
note right of A : This is a note"#,
        ParseOptions::default(),
    ))
    .unwrap()
    .unwrap();
    assert_eq!(
        res.model["states"]["A"]["note"]["position"],
        json!("right of")
    );
    assert_eq!(
        res.model["states"]["A"]["note"]["text"],
        json!("This is a note")
    );

    let res = block_on(engine.parse_diagram(
        r#"stateDiagram-v2
[*] --> A
note right of A
  line1
  line2
end note"#,
        ParseOptions::default(),
    ))
    .unwrap()
    .unwrap();
    let note_text = res.model["states"]["A"]["note"]["text"].as_str().unwrap();
    assert_eq!(note_text, "line1\nline2");

    let res = block_on(engine.parse_diagram(
        r#"stateDiagram
foo: bar
note "This is a floating note" as N1"#,
        ParseOptions::default(),
    ))
    .unwrap()
    .unwrap();
    // Mermaid `@11.12.2` treats floating notes as a no-op in state diagrams.
    // (See upstream `stateDiagram floating notes` specs.)
    assert!(res.model["states"].get("N1").is_none());
}

#[test]
fn parse_diagram_state_note_closes_only_on_a_dedicated_end_note_line() {
    let engine = Engine::new();
    let text = r#"stateDiagram-v2
State1
note right of State1
  this sentence contains end note as part of the note text
end note
State1 --> State2"#;

    let res = block_on(engine.parse_diagram(text, ParseOptions::strict()))
        .unwrap()
        .unwrap();

    assert_eq!(
        res.model["states"]["State1"]["note"]["text"],
        json!("this sentence contains end note as part of the note text")
    );
    let relations = res.model["relations"].as_array().unwrap();
    assert_eq!(relations.len(), 1);
    assert_eq!(relations[0]["id1"], json!("State1"));
    assert_eq!(relations[0]["id2"], json!("State2"));

    let facts = engine
        .parse_editor_semantic_facts_with_type_sync("stateDiagram", text)
        .unwrap()
        .expect("state editor facts");
    assert_eq!(facts.completeness, EditorSemanticCompleteness::Complete);
}

#[test]
fn parse_diagram_state_v2_getdata_edges_and_note_edges() {
    let engine = Engine::new();

    let res = block_on(engine.parse_diagram(
        r#"stateDiagram-v2
A --> B: hello"#,
        ParseOptions::default(),
    ))
    .unwrap()
    .unwrap();

    assert_eq!(res.model["edges"][0]["start"], json!("A"));
    assert_eq!(res.model["edges"][0]["end"], json!("B"));
    assert_eq!(res.model["edges"][0]["label"], json!("hello"));
    assert_eq!(res.model["edges"][0]["arrowhead"], json!("normal"));

    let res = block_on(engine.parse_diagram(
        r#"stateDiagram-v2
[*] --> A
note left of A : note text"#,
        ParseOptions::default(),
    ))
    .unwrap()
    .unwrap();

    let note_edge = res.model["edges"]
        .as_array()
        .unwrap()
        .iter()
        .find(|e| e["arrowhead"] == json!("none"))
        .unwrap();
    assert_eq!(note_edge["classes"], json!("transition note-edge"));
}

#[test]
fn parse_diagram_state_v2_uses_neo_arrow_type_when_look_is_neo() {
    let engine = Engine::new();

    let res = block_on(engine.parse_diagram(
        r#"%%{init: {"look": "neo"}}%%
stateDiagram-v2
A --> B: hello"#,
        ParseOptions::default(),
    ))
    .unwrap()
    .unwrap();

    assert_eq!(
        res.model["edges"][0]["arrowTypeEnd"],
        json!("arrow_barb_neo")
    );
}

#[test]
fn parse_diagram_state_v2_sanitizes_edge_labels_like_mermaid_common() {
    let engine = Engine::new();
    let res = block_on(engine.parse_diagram(
        r#"stateDiagram-v2
A --> B: hello<script>alert(1)</script>world"#,
        ParseOptions::default(),
    ))
    .unwrap()
    .unwrap();
    assert_eq!(res.model["edges"][0]["label"], json!("helloworld"));
}

#[test]
fn parse_diagram_state_v2_getdata_dom_id_counter_and_note_padding_match_mermaid() {
    let engine = Engine::new();

    let res = block_on(engine.parse_diagram(
        r#"stateDiagram-v2
A --> B
note right of A : note text"#,
        ParseOptions::default(),
    ))
    .unwrap()
    .unwrap();

    let nodes = res.model["nodes"].as_array().unwrap();
    let node_a = nodes.iter().find(|n| n["id"] == json!("A")).unwrap();
    let node_b = nodes.iter().find(|n| n["id"] == json!("B")).unwrap();
    let note_group = nodes
        .iter()
        .find(|n| n["id"] == json!("A----parent"))
        .unwrap();
    let note_node = nodes
        .iter()
        .find(|n| n["id"] == json!("A----note-1"))
        .unwrap();

    assert_eq!(node_a["domId"], json!("state-A-1"));
    assert_eq!(node_b["domId"], json!("state-B-0"));
    assert_eq!(note_group["domId"], json!("state-A----parent-1"));
    assert_eq!(note_node["domId"], json!("state-A----note-1"));
    assert_eq!(note_group["padding"], json!(16));
    assert_eq!(note_node["padding"], json!(15));
    assert_eq!(note_node["parentId"], json!("A----parent"));
}

#[test]
fn parse_diagram_state_v2_keeps_multiple_note_groups_distinct() {
    let engine = Engine::new();
    let res = block_on(engine.parse_diagram(
        r#"stateDiagram-v2
A
note left of A : left text
note right of A : right text"#,
        ParseOptions::default(),
    ))
    .unwrap()
    .unwrap();

    let nodes = res.model["nodes"].as_array().unwrap();
    let groups: Vec<_> = nodes
        .iter()
        .filter(|node| node["shape"] == json!("noteGroup"))
        .collect();
    assert_eq!(groups.len(), 2, "each note needs its own group node");
    assert_ne!(groups[0]["id"], groups[1]["id"]);
    assert!(nodes.iter().any(|node| node["label"] == json!("left text")));
    assert!(
        nodes
            .iter()
            .any(|node| node["label"] == json!("right text"))
    );
    assert!(
        nodes
            .iter()
            .any(|node| node["position"] == json!("left of"))
    );
    assert!(
        nodes
            .iter()
            .any(|node| node["position"] == json!("right of"))
    );
}

fn deep_state_composite_chain(depth: usize) -> String {
    let mut input = String::from("stateDiagram-v2\n");
    for level in 0..depth {
        input.push_str(&format!("state S{level} {{\n"));
    }
    input.push_str("Leaf\n");
    for _ in 0..depth {
        input.push_str("}\n");
    }
    input
}

#[test]
fn state_deep_composite_chain_semantic_and_render_model_use_heap_traversal() {
    const DEPTH: usize = 1200;
    let input = deep_state_composite_chain(DEPTH);
    let engine = Engine::new();

    let parsed = block_on(engine.parse_diagram(&input, ParseOptions::strict()))
        .expect("parse ok")
        .expect("diagram detected");
    assert_eq!(parsed.meta.diagram_type, "stateDiagram");
    assert!(parsed.model["states"]["S0"]["doc"].is_array());
    assert!(
        parsed.model["nodes"]
            .as_array()
            .expect("nodes array")
            .iter()
            .any(|node| node["id"] == json!("Leaf"))
    );

    let parsed = engine
        .parse_diagram_for_render_model_sync(&input, ParseOptions::strict())
        .expect("render model parse ok")
        .expect("diagram detected");
    assert_eq!(parsed.metadata().diagram_type, "stateDiagram");
}

#[test]
fn state_family_entrypoints_construct_one_lexical_event_tape() {
    let engine = Engine::new();
    let input = concat!(
        "stateDiagram-v2\n",
        "state Idle\n",
        "Idle --> Running: starts\n",
        "classDef active fill:#0f0\n",
        "class Running active\n",
    );

    crate::diagrams::state::reset_state_syntax_construction_count();
    engine
        .parse_diagram_sync(input, ParseOptions::strict())
        .expect("State JSON parse succeeds")
        .expect("State JSON parse returns a diagram");
    assert_eq!(
        crate::diagrams::state::state_syntax_construction_count(),
        1,
        "the State JSON entrypoint must construct one lexical event tape"
    );

    crate::diagrams::state::reset_state_syntax_construction_count();
    engine
        .parse_diagram_for_render_model_sync(input, ParseOptions::strict())
        .expect("State typed parse succeeds")
        .expect("State typed parse returns a diagram");
    assert_eq!(
        crate::diagrams::state::state_syntax_construction_count(),
        1,
        "the State typed entrypoint must construct one lexical event tape"
    );

    crate::diagrams::state::reset_state_syntax_construction_count();
    engine
        .parse_editor_semantic_facts_with_type_sync("stateDiagram", input)
        .expect("State editor parse succeeds")
        .expect("State editor parse returns facts");
    assert_eq!(
        crate::diagrams::state::state_syntax_construction_count(),
        1,
        "the State editor entrypoint must construct one lexical event tape"
    );
}

#[test]
fn state_combined_projection_constructs_once_and_matches_standalone_entrypoints() {
    let engine = Engine::new();
    let input = concat!(
        "stateDiagram-v2\n",
        "accTitle: Lifecycle\n",
        "accDescr: State transitions\n",
        "state \"Waiting\" as Idle\n",
        "Idle --> Running: starts\n",
        "note right of Running : Active work\n",
        "classDef active fill:#0f0,border:#333\n",
        "class Running active\n",
        "click Running \"https://example.com/run\" \"Run details\"\n",
    );
    let standalone = engine
        .parse_diagram_sync(input, ParseOptions::strict())
        .expect("standalone State JSON parse succeeds")
        .expect("standalone State JSON parse returns a diagram");
    let standalone_editor = engine
        .parse_editor_semantic_facts_with_type_sync("stateDiagram", input)
        .expect("standalone State editor parse succeeds")
        .expect("standalone State editor parse returns facts");

    crate::diagrams::state::reset_state_syntax_construction_count();
    let (combined_json, mut combined_editor) = crate::family::test_support::into_result(
        crate::diagrams::state::parse_state_json_and_editor_facts(
            input,
            &standalone.meta,
            &crate::OperationControl::new(),
        ),
    )
    .expect("combined State parse succeeds");
    combined_editor.family_semantics =
        crate::family::diagram_type_editor_semantics(&standalone.meta.diagram_type)
            .expect("State has typed editor family semantics");

    assert_eq!(
        crate::diagrams::state::state_syntax_construction_count(),
        1,
        "one combined State request must construct syntax once"
    );
    assert_eq!(
        combined_json, standalone.model,
        "State JSON projection drift"
    );
    assert_eq!(
        combined_editor, standalone_editor,
        "State editor projection drift"
    );
}

#[test]
fn state_typed_render_model_projects_exact_compatibility_json() {
    let engine = Engine::new();
    let input = concat!(
        "stateDiagram-v2\n",
        "direction LR\n",
        "accTitle: Lifecycle\n",
        "accDescr: State transitions\n",
        "state \"Waiting\" as Idle\n",
        "state Running {\n",
        "  direction LR\n",
        "  [*] --> Working\n",
        "  Working --> [*]\n",
        "}\n",
        "Idle --> Running: starts\n",
        "note right of Running : Active work\n",
        "classDef active fill:#0f0,border:#333\n",
        "class Running active\n",
        "click Running \"https://example.com/run\" \"Run details\"\n",
    );
    let compat = engine
        .parse_diagram_sync(input, ParseOptions::strict())
        .expect("State JSON parse succeeds")
        .expect("State JSON parse returns a diagram")
        .model;
    let typed = engine
        .parse_diagram_for_render_model_sync(input, ParseOptions::strict())
        .expect("State typed parse succeeds")
        .expect("State typed parse returns a diagram");
    let RenderSemanticModel::State(model) = typed.model() else {
        panic!("State typed parse returned another family");
    };

    assert_eq!(
        crate::diagrams::state::render_model_to_compat_json(model, typed.metadata()).unwrap(),
        compat
    );
}

#[test]
fn parse_state_editor_facts_preserve_parser_state_spans() {
    let engine = Engine::new();
    let text = r#"stateDiagram-v2
[*] --> Idle
Idle --> Running
Idle: Waiting state
Idle --> Running: starts
state Running {
  [*] --> Active
}
state "Paused State" as Paused
note right of Running : Running details
note "Floating note" as note1
classDef activeStyle fill:#0f0,border:#333
class Idle, Running activeStyle
style Running fill:#f00
accTitle: Lifecycle chart
accDescr: Shows state transitions
click Running "https://example.com/run" "Run details""#;
    let facts = engine
        .parse_editor_semantic_facts_with_type_sync("stateDiagram", text)
        .unwrap()
        .expect("state editor facts");

    assert_eq!(facts.completeness, EditorSemanticCompleteness::Complete);

    let symbol_at = |name: &str, start: usize| {
        facts
            .symbols
            .iter()
            .find(|symbol| symbol.name == name && symbol.selection.start == start)
            .unwrap_or_else(|| panic!("missing symbol {name} at {start}"))
    };

    let idle_start = text.find("Idle").unwrap();
    assert_eq!(
        symbol_at("Idle", idle_start).selection.end,
        idle_start + "Idle".len()
    );

    let running_start = text.find("Running").unwrap();
    assert_eq!(
        symbol_at("Running", running_start).selection.end,
        running_start + "Running".len()
    );

    let idle_relation_source_start = text.find("Idle --> Running").unwrap();
    assert!(facts.symbols.iter().any(|symbol| {
        symbol.name == "Idle"
            && symbol.detail.as_deref() == Some("state reference")
            && symbol.selection.start == idle_relation_source_start
            && symbol.role == EditorSemanticRole::Reference
    }));

    let running_relation_target_start = text.find("Idle --> Running").unwrap() + "Idle --> ".len();
    assert_eq!(
        symbol_at("Running", running_relation_target_start).role,
        EditorSemanticRole::Entity
    );

    let repeated_running_target_start =
        text.find("Idle --> Running: starts").unwrap() + "Idle --> ".len();
    assert!(facts.symbols.iter().any(|symbol| {
        symbol.name == "Running"
            && symbol.detail.as_deref() == Some("state reference")
            && symbol.selection.start == repeated_running_target_start
            && symbol.role == EditorSemanticRole::Reference
    }));

    let active_start = text.find("Active").unwrap();
    assert_eq!(
        symbol_at("Active", active_start).selection.end,
        active_start + "Active".len()
    );

    let paused_start = text.rfind("Paused").unwrap();
    assert_eq!(
        symbol_at("Paused", paused_start).selection.end,
        paused_start + "Paused".len()
    );

    let display_label = facts
        .symbols
        .iter()
        .find(|symbol| {
            symbol.name == "Paused State" && symbol.detail.as_deref() == Some("state display label")
        })
        .unwrap();
    assert_eq!(display_label.role, EditorSemanticRole::Payload);

    let state_description = facts
        .symbols
        .iter()
        .find(|symbol| {
            symbol.name == "Waiting state" && symbol.detail.as_deref() == Some("state description")
        })
        .unwrap();
    assert_eq!(state_description.role, EditorSemanticRole::Payload);

    let relation_label = facts
        .symbols
        .iter()
        .find(|symbol| {
            symbol.name == "starts" && symbol.detail.as_deref() == Some("state relation label")
        })
        .unwrap();
    assert_eq!(relation_label.role, EditorSemanticRole::Payload);

    let positioned_note = facts
        .symbols
        .iter()
        .find(|symbol| {
            symbol.name == "Running details" && symbol.detail.as_deref() == Some("state note")
        })
        .unwrap();
    assert_eq!(positioned_note.role, EditorSemanticRole::Payload);

    let floating_note = facts
        .symbols
        .iter()
        .find(|symbol| {
            symbol.name == "Floating note" && symbol.detail.as_deref() == Some("state note")
        })
        .unwrap();
    assert_eq!(floating_note.role, EditorSemanticRole::Payload);
    assert!(!facts.symbols.iter().any(|symbol| symbol.name == "note1"));

    let active_style_start = text.find("activeStyle").unwrap();
    let active_style = facts
        .symbols
        .iter()
        .find(|symbol| {
            symbol.name == "activeStyle"
                && symbol.detail.as_deref() == Some("state class definition")
        })
        .unwrap();
    assert_eq!(active_style.role, EditorSemanticRole::ClassDefinition);
    assert_eq!(active_style.selection.start, active_style_start);

    let idle_class_target = facts
        .symbols
        .iter()
        .find(|symbol| {
            symbol.name == "Idle" && symbol.detail.as_deref() == Some("state class target")
        })
        .unwrap();
    assert_eq!(idle_class_target.role, EditorSemanticRole::Reference);

    let running_style = facts
        .symbols
        .iter()
        .find(|symbol| {
            symbol.name == "fill:#f00" && symbol.detail.as_deref() == Some("state style")
        })
        .unwrap();
    assert_eq!(running_style.role, EditorSemanticRole::Payload);
    assert!(running_style.selection.start > running_style.span.start);

    let acc_title = facts
        .symbols
        .iter()
        .find(|symbol| symbol.name == "Lifecycle chart")
        .unwrap();
    assert_eq!(acc_title.role, EditorSemanticRole::Payload);
    assert_eq!(
        acc_title.detail.as_deref(),
        Some("state accessibility title")
    );

    let acc_descr = facts
        .symbols
        .iter()
        .find(|symbol| symbol.name == "Shows state transitions")
        .unwrap();
    assert_eq!(acc_descr.role, EditorSemanticRole::Payload);
    assert_eq!(
        acc_descr.detail.as_deref(),
        Some("state accessibility description")
    );

    let click_url = facts
        .symbols
        .iter()
        .find(|symbol| symbol.name == "https://example.com/run")
        .unwrap();
    assert_eq!(click_url.role, EditorSemanticRole::Payload);
    assert_eq!(click_url.detail.as_deref(), Some("state click url"));

    let click_target_start = text.find("click Running").unwrap() + "click ".len();
    let click_target = symbol_at("Running", click_target_start);
    assert_eq!(click_target.role, EditorSemanticRole::Reference);
    assert_eq!(click_target.detail.as_deref(), Some("state click target"));

    let click_tooltip = facts
        .symbols
        .iter()
        .find(|symbol| symbol.name == "Run details")
        .unwrap();
    assert_eq!(click_tooltip.role, EditorSemanticRole::Payload);
    assert_eq!(click_tooltip.detail.as_deref(), Some("state click tooltip"));
}

#[test]
fn parse_state_editor_facts_record_expected_syntax_spans() {
    let engine = Engine::new();
    let text = concat!(
        "stateDiagram-v2\n",
        "state \"Small State\" as namedState\n",
        "namedState: Waiting state\n",
        "classDef exampleStyleClass background:#bbb,border:1px solid red\n",
        "a --> b:::exampleStyleClass\n",
        "class namedState exampleStyleClass\n",
        "style namedState fill:#f00\n",
        "click namedState \"https://example.com/run\" \"Run details\"",
    );
    let facts = engine
        .parse_editor_semantic_facts_with_type_sync("stateDiagram", text)
        .unwrap()
        .expect("state editor facts");

    assert_eq!(facts.completeness, EditorSemanticCompleteness::Complete);

    assert_expected_syntax_covers(
        &facts,
        EditorExpectedSyntaxKind::Payload,
        text,
        "state \"Small State\" as namedState",
        "Small State",
        "state display label payload",
    );
    assert_expected_syntax_covers(
        &facts,
        EditorExpectedSyntaxKind::NodeIdentifier,
        text,
        "state \"Small State\" as namedState",
        "namedState",
        "state alias node identifier",
    );
    assert_expected_syntax_covers(
        &facts,
        EditorExpectedSyntaxKind::Payload,
        text,
        "namedState: Waiting state",
        "Waiting state",
        "state description payload",
    );
    assert_expected_syntax_covers(
        &facts,
        EditorExpectedSyntaxKind::ClassName,
        text,
        "classDef exampleStyleClass background:#bbb,border:1px solid red",
        "exampleStyleClass",
        "state class definition name",
    );
    assert_expected_syntax_covers(
        &facts,
        EditorExpectedSyntaxKind::NodeIdentifier,
        text,
        "a --> b:::exampleStyleClass",
        "b",
        "state inline class node identifier",
    );
    assert_expected_syntax_covers(
        &facts,
        EditorExpectedSyntaxKind::ClassName,
        text,
        "class namedState exampleStyleClass",
        "exampleStyleClass",
        "state class name",
    );
    assert_expected_syntax_covers(
        &facts,
        EditorExpectedSyntaxKind::ClassName,
        text,
        "a --> b:::exampleStyleClass",
        "exampleStyleClass",
        "state inline class name",
    );
    assert_expected_syntax_covers(
        &facts,
        EditorExpectedSyntaxKind::IdList,
        text,
        "class namedState exampleStyleClass",
        "namedState",
        "state class target list",
    );
    assert_expected_syntax_covers(
        &facts,
        EditorExpectedSyntaxKind::IdList,
        text,
        "style namedState fill:#f00",
        "namedState",
        "state style target list",
    );
    assert_expected_syntax_covers(
        &facts,
        EditorExpectedSyntaxKind::Payload,
        text,
        "style namedState fill:#f00",
        "fill:#f00",
        "state style payload",
    );
    assert_expected_syntax_covers(
        &facts,
        EditorExpectedSyntaxKind::NodeIdentifier,
        text,
        "click namedState \"https://example.com/run\" \"Run details\"",
        "namedState",
        "state click target",
    );
    assert_expected_syntax_covers(
        &facts,
        EditorExpectedSyntaxKind::Payload,
        text,
        "click namedState \"https://example.com/run\" \"Run details\"",
        "https://example.com/run",
        "state click url",
    );
    assert_expected_syntax_covers(
        &facts,
        EditorExpectedSyntaxKind::Payload,
        text,
        "click namedState \"https://example.com/run\" \"Run details\"",
        "Run details",
        "state click tooltip",
    );
}

#[test]
fn state_relations_create_only_the_first_implicit_entity_occurrence() {
    let engine = Engine::new();
    let text = concat!(
        "stateDiagram-v2\n",
        "Future --> Known\n",
        "Known --> Future\n",
        "state Known\n",
        "style Later fill:#fff\n",
        "state Later\n",
        "click Known \"https://example.com\"\n",
        "note right of Known : Existing state note\n",
    );
    let facts = engine
        .parse_editor_semantic_facts_with_type_sync("stateDiagram", text)
        .unwrap()
        .expect("state editor facts");

    let roles = |name: &str| {
        facts
            .symbols
            .iter()
            .filter(|symbol| symbol.name == name)
            .map(|symbol| symbol.role)
            .collect::<Vec<_>>()
    };
    assert_eq!(
        roles("Future"),
        [EditorSemanticRole::Entity, EditorSemanticRole::Reference]
    );
    assert_eq!(
        roles("Known"),
        [
            EditorSemanticRole::Entity,
            EditorSemanticRole::Reference,
            EditorSemanticRole::Entity,
            EditorSemanticRole::Reference,
            EditorSemanticRole::Reference,
        ]
    );
    assert_eq!(
        roles("Later"),
        [EditorSemanticRole::Reference, EditorSemanticRole::Entity]
    );
}

#[test]
fn parse_state_editor_facts_do_not_consume_classdef_names_across_line_endings() {
    let engine = Engine::new();

    for line_ending in ["\n", "\r", "\r\n"] {
        let text = ["stateDiagram-v2", "classDef", "Active --> Idle", ""].join(line_ending);
        let facts = engine
            .parse_editor_semantic_facts_with_type_sync("stateDiagram", &text)
            .unwrap()
            .expect("state editor facts");

        assert!(
            !facts.symbols.iter().any(|symbol| {
                symbol.name == "Active" && symbol.role == EditorSemanticRole::ClassDefinition
            }),
            "classDef consumed the next physical line for {line_ending:?}"
        );
    }
}

#[test]
fn parse_state_editor_facts_recovers_from_incomplete_input() {
    let engine = Engine::new();
    let text = "stateDiagram-v2\nIdle --> Running\nRunning -->";
    crate::diagrams::state::reset_state_syntax_construction_count();
    let facts = engine
        .parse_editor_semantic_facts_with_type_sync("stateDiagram", text)
        .unwrap()
        .expect("state editor facts");

    assert_eq!(facts.completeness, EditorSemanticCompleteness::Recovered);
    assert_eq!(facts.diagnostics.len(), 1);
    let diagnostic = &facts.diagnostics[0];
    assert!(diagnostic.message.contains("state parser recovered"));
    assert_eq!(
        diagnostic.span,
        Some(SourceSpan::new(text.len(), text.len()))
    );
    assert!(facts.symbols.iter().any(|symbol| symbol.name == "Idle"));
    assert!(facts.symbols.iter().any(|symbol| symbol.name == "Running"));
    assert_eq!(
        crate::diagrams::state::state_syntax_construction_count(),
        1,
        "incomplete State editor recovery must reuse one lexical event tape"
    );

    crate::diagrams::state::reset_state_syntax_construction_count();
    let Error::DiagramParse { diagnostic, .. } = engine
        .parse_diagram_sync(text, ParseOptions::strict())
        .expect_err("strict State parsing rejects the incomplete relation")
    else {
        panic!("incomplete State relation returned a non-parse error");
    };
    assert_eq!(
        crate::diagrams::state::state_syntax_construction_count(),
        1,
        "strict incomplete State parsing must construct one lexical event tape"
    );
    assert_eq!(
        diagnostic.span(),
        Some(SourceSpan::new(text.len(), text.len()))
    );
    assert_eq!(
        diagnostic.span_kind(),
        ParseDiagnosticSpanKind::InsertionPoint
    );
}

#[test]
fn parse_state_editor_facts_stop_after_non_advancing_lexer_error() {
    let engine = Engine::new();
    let text = "stateDiagram-v2\nstate {\nIdle --> Running\n";
    crate::diagrams::state::reset_state_syntax_construction_count();
    let facts = engine
        .parse_editor_semantic_facts_with_type_sync("stateDiagram", text)
        .unwrap()
        .expect("state editor facts");

    assert_eq!(facts.completeness, EditorSemanticCompleteness::Recovered);
    assert!(
        facts
            .diagnostics
            .iter()
            .any(|diagnostic| diagnostic.message.contains("state parser recovered"))
    );
    assert_eq!(
        crate::diagrams::state::state_syntax_construction_count(),
        1,
        "a non-advancing State lexer error must terminate the one shared event tape"
    );
}

fn assert_expected_syntax_covers(
    facts: &EditorSemanticFacts,
    kind: EditorExpectedSyntaxKind,
    text: &str,
    marker: &str,
    target: &str,
    label: &str,
) {
    let marker_start = text
        .find(marker)
        .unwrap_or_else(|| panic!("missing {label} source text"));
    let target_start = text[marker_start..]
        .find(target)
        .map(|offset| marker_start + offset)
        .unwrap_or_else(|| panic!("missing {label} target"));
    let end = target_start + target.len();
    assert!(
        facts.expected_syntax.iter().any(|expected| {
            expected.kind == kind && expected.span.start <= target_start && expected.span.end >= end
        }),
        "missing {label}"
    );
}

#[test]
fn parse_diagram_state_direction_name_is_not_a_direction_statement() {
    let engine = Engine::new();
    for source in [
        "stateDiagram-v2\ndirection\n",
        "stateDiagram-v2\ndirection --> Done\n",
    ] {
        let parsed = engine
            .parse_diagram_sync(source, ParseOptions::default())
            .unwrap()
            .unwrap();
        assert!(
            parsed.model["states"].get("direction").is_some(),
            "{source}"
        );
    }
    for (statement, expected) in [
        ("direction lrignored", "LR"),
        ("prefixdirection BT", "BT"),
        ("direction LR direction TB", "TB"),
    ] {
        let source = format!("stateDiagram-v2\n{statement}\nA --> B\n");
        let parsed = engine
            .parse_diagram_sync(&source, ParseOptions::default())
            .unwrap()
            .unwrap();
        assert_eq!(parsed.model["direction"], expected, "{source}");
    }
}

#[test]
fn parse_diagram_state_direction_does_not_hide_prior_initial_tokens() {
    let engine = Engine::new();
    for statement in [
        r#"click A href "https://example.test/direction LR""#,
        r#"click A "https://example.test" "direction LR""#,
        r#""direction LR" as A"#,
        r#"href "direction LR""#,
        "default direction LR",
    ] {
        let source = format!("stateDiagram-v2\nA\n{statement}\n");
        assert!(
            engine
                .parse_diagram_sync(&source, ParseOptions::default())
                .is_err(),
            "{source}"
        );
    }
    // The STATE introducer has lower lexer priority than a whole-line direction.
    let source = "stateDiagram-v2\nstate \"direction LR\" as A\n";
    let parsed = engine
        .parse_diagram_sync(source, ParseOptions::default())
        .unwrap()
        .unwrap();
    assert_eq!(parsed.model["direction"], "LR");
    assert!(parsed.model["states"].get("A").is_none());
}

#[test]
fn parse_diagram_state_struct_directives_precede_greedy_direction_rules() {
    let engine = Engine::new();
    for (statement, kind, id, field, value) in [
        (
            r#"state "direction LR" as Inner"#,
            "state",
            "Inner",
            "description",
            "direction LR",
        ),
        (
            "classDef custom fill:direction LR",
            "classDef",
            "custom",
            "classes",
            "fill:direction LR",
        ),
        (
            "style Inner fill:direction LR",
            "style",
            "Inner",
            "styleClass",
            "fill:direction LR",
        ),
        (
            "class Inner direction LR",
            "applyClass",
            "Inner",
            "styleClass",
            "direction LR",
        ),
    ] {
        let source = format!("stateDiagram-v2\nstate Outer {{\n{statement}\n}}\n");
        let parsed = engine
            .parse_diagram_sync(&source, ParseOptions::strict())
            .unwrap()
            .unwrap();
        let statement = &parsed.model["states"]["Outer"]["doc"][0];
        assert_eq!(statement["stmt"], kind, "{source}");
        assert_eq!(statement["id"], id, "{source}");
        assert_eq!(statement[field], value, "{source}");
    }
    // NOTE remains lower priority than directions in the upstream struct lexer.
    let source = "stateDiagram-v2\nstate Outer {\nnote \"direction LR\" as N\n}\n";
    let parsed = engine
        .parse_diagram_sync(source, ParseOptions::strict())
        .unwrap()
        .unwrap();
    assert_eq!(
        parsed.model["states"]["Outer"]["doc"][0],
        json!({"stmt": "dir", "value": "LR"})
    );
}

#[test]
fn parse_state_editor_struct_state_label_preserves_entity_selection() {
    let engine = Engine::new();
    let source = "stateDiagram-v2\nstate Outer {\nstate \"direction LR\" as Inner\n}\n";
    let facts = engine
        .parse_editor_semantic_facts_with_type_sync("stateDiagram", source)
        .unwrap()
        .unwrap();
    assert_eq!(facts.completeness, EditorSemanticCompleteness::Complete);
    let start = source.find("Inner").unwrap();
    assert!(facts.symbols.iter().any(|symbol| symbol.name == "Inner"
        && symbol.selection == SourceSpan::new(start, start + "Inner".len())));
    assert!(
        !facts
            .expected_syntax
            .iter()
            .any(|expected| expected.kind == EditorExpectedSyntaxKind::CardinalDirectionValue)
    );
}

#[test]
fn parse_diagram_state_initial_direction_uses_raw_cursor() {
    let engine = Engine::new();
    for prefix in ["  ", "\t", "\u{00a0}", "\u{feff}"] {
        for statement in [
            r#"click A href "direction LR""#,
            "default direction LR",
            r#""direction LR" as Hidden"#,
            "# direction LR",
            "accTitle: direction LR",
        ] {
            let source = format!("stateDiagram-v2\nA\n{prefix}{statement}\n");
            let parsed = engine
                .parse_diagram_sync(&source, ParseOptions::default())
                .unwrap()
                .unwrap();
            assert_eq!(parsed.model["direction"], "LR", "{source}");
            assert!(parsed.model["states"].get("A").is_some(), "{source}");
            assert!(parsed.model["states"].get("Hidden").is_none(), "{source}");
            let facts = engine
                .parse_editor_semantic_facts_with_type_sync("stateDiagram", &source)
                .unwrap()
                .unwrap();
            assert_eq!(
                facts.completeness,
                EditorSemanticCompleteness::Complete,
                "{source}"
            );
            assert!(
                !facts.symbols.iter().any(|symbol| symbol.name == "Hidden"),
                "{source}"
            );
            let selected = facts
                .expected_syntax
                .iter()
                .find(|item| item.kind == EditorExpectedSyntaxKind::CardinalDirectionValue)
                .unwrap();
            let start = source.rfind("LR").unwrap();
            assert_eq!(selected.span, SourceSpan::new(start, start + 2), "{source}");
        }
    }
    // The header owns its whitespace suffix, including the first declaration's indent.
    assert!(
        engine
            .parse_diagram_sync(
                "stateDiagram-v2\n  default direction LR\n",
                ParseOptions::default()
            )
            .is_err()
    );
}

#[test]
fn parse_diagram_state_struct_whitespace_and_comments_precede_direction() {
    let engine = Engine::new();
    let source =
        "stateDiagram-v2\nstate Outer {\n  # direction LR\n  state \"direction LR\" as Inner\n}\n";
    let parsed = engine
        .parse_diagram_sync(source, ParseOptions::default())
        .unwrap()
        .unwrap();
    assert_eq!(parsed.model["direction"], "TB");
    assert_eq!(parsed.model["states"]["Outer"]["doc"][0]["id"], "Inner");
}

#[test]
fn parse_diagram_state_accessibility_requires_complete_prefix() {
    let engine = Engine::new();
    for name in ["accTitle", "accDescr"] {
        let source = format!("stateDiagram-v2\n{name}\n");
        let parsed = engine
            .parse_diagram_sync(&source, ParseOptions::default())
            .unwrap()
            .unwrap();
        assert!(parsed.model["states"].get(name).is_some(), "{source}");
        let facts = engine
            .parse_editor_semantic_facts_with_type_sync("stateDiagram", &source)
            .unwrap()
            .unwrap();
        assert_eq!(
            facts.completeness,
            EditorSemanticCompleteness::Complete,
            "{source}"
        );
        let start = source.find(name).unwrap();
        assert!(
            facts.symbols.iter().any(|symbol| symbol.name == name
                && symbol.selection == SourceSpan::new(start, start + name.len())),
            "{source}"
        );
    }
    let source = "stateDiagram-v2\naccDescr nonsense direction LR: label\n";
    let parsed = engine
        .parse_diagram_sync(source, ParseOptions::default())
        .unwrap()
        .unwrap();
    assert_eq!(parsed.model["direction"], "LR");
    assert!(parsed.model["accDescr"].is_null());
    for (keyword, field) in [("accTitle", "accTitle"), ("accDescr", "accDescr")] {
        for (before_colon, after_colon) in [("\u{feff}", "\u{feff}"), ("", "\n"), ("\n", " ")] {
            let source = format!("stateDiagram-v2\n{keyword}{before_colon}:{after_colon}Heading\n");
            let parsed = engine
                .parse_diagram_sync(&source, ParseOptions::default())
                .unwrap()
                .unwrap();
            assert_eq!(parsed.model[field], "Heading", "{source}");
            assert!(parsed.model["states"].get("Heading").is_none(), "{source}");
        }
    }
}

#[test]
fn parse_diagram_state_accessibility_multiline_uses_ecmascript_trim() {
    let engine = Engine::new();
    for (body, expected) in [
        ("\u{feff}Heading\u{feff}", "Heading"),
        ("\u{0085}Heading\u{0085}", "\u{0085}Heading\u{0085}"),
    ] {
        let source = format!("stateDiagram-v2\naccDescr {{{body}}}\nA\n");
        let parsed = engine
            .parse_diagram_sync(&source, ParseOptions::default())
            .unwrap()
            .unwrap();
        assert_eq!(parsed.model["accDescr"], expected, "{source}");
    }
}

#[test]
fn parse_diagram_state_unterminated_accessibility_block_keeps_eof_assignment() {
    let engine = Engine::new();
    let source = "stateDiagram-v2\naccDescr: previous\naccDescr { unclosed\n";
    let parsed = engine
        .parse_diagram_sync(source, ParseOptions::default())
        .unwrap()
        .unwrap();
    assert_eq!(parsed.model["accDescr"], "");
}
