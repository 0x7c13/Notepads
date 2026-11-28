use super::*;
use crate::{
    EditorExpectedSyntaxKind, EditorSemanticCompleteness, EditorSemanticRole, Engine, ParseOptions,
    RenderSemanticModel, SourceSpan,
};
use futures::executor::block_on;
use serde_json::Value;

fn parse(text: &str) -> crate::ManagedSemanticJson {
    let engine = Engine::new();
    block_on(engine.parse_diagram(text, ParseOptions::default()))
        .unwrap()
        .unwrap()
        .model
}

fn deep_mindmap_chain(depth: usize) -> String {
    let mut input = String::from("mindmap\n");
    for level in 0..depth {
        input.push_str(&" ".repeat(level));
        input.push_str(&format!("n{level}\n"));
    }
    input
}

fn root_descr(model: &Value) -> &str {
    model["rootNode"]["descr"].as_str().unwrap()
}

#[test]
fn mindmap_simple_root() {
    let model = parse("mindmap\n    root");
    assert_eq!(root_descr(&model), "root");
}

#[test]
fn mindmap_simple_root_shaped_without_id() {
    let model = parse("mindmap\n    (root)");
    assert_eq!(root_descr(&model), "root");
    assert_eq!(model["rootNode"]["nodeId"].as_str().unwrap(), "root");
}

#[test]
fn mindmap_hierarchy_two_children() {
    let model = parse("mindmap\n    root\n      child1\n      child2\n");
    assert_eq!(root_descr(&model), "root");
    assert_eq!(model["rootNode"]["children"].as_array().unwrap().len(), 2);
    assert_eq!(
        model["rootNode"]["children"][0]["descr"].as_str().unwrap(),
        "child1"
    );
    assert_eq!(
        model["rootNode"]["children"][1]["descr"].as_str().unwrap(),
        "child2"
    );
}

#[test]
fn mindmap_deeper_hierarchy() {
    let model = parse("mindmap\n    root\n      child1\n        leaf1\n      child2");
    let mm = &model["rootNode"];
    assert_eq!(mm["descr"].as_str().unwrap(), "root");
    let children = mm["children"].as_array().unwrap();
    assert_eq!(children.len(), 2);
    assert_eq!(children[0]["descr"].as_str().unwrap(), "child1");
    assert_eq!(
        children[0]["children"][0]["descr"].as_str().unwrap(),
        "leaf1"
    );
    assert_eq!(children[1]["descr"].as_str().unwrap(), "child2");
}

#[test]
fn mindmap_multiple_roots_is_error() {
    let engine = Engine::new();
    let err =
        block_on(engine.parse_diagram("mindmap\n    root\n    fakeRoot", ParseOptions::default()))
            .unwrap_err();
    assert!(
        err.to_string()
            .contains("There can be only one root. No parent could be found for (\"fakeRoot\")")
    );
}

#[test]
fn mindmap_real_root_in_wrong_place_is_error() {
    let engine = Engine::new();
    let text = "mindmap\n          root\n        fakeRoot\n    realRootWrongPlace";
    let err = block_on(engine.parse_diagram(text, ParseOptions::default())).unwrap_err();
    assert!(
        err.to_string()
            .contains("There can be only one root. No parent could be found for (\"fakeRoot\")")
    );
}

#[test]
fn mindmap_node_id_and_label_and_type_rect() {
    let model = parse("mindmap\n    root[The root]\n");
    assert_eq!(model["rootNode"]["nodeId"].as_str().unwrap(), "root");
    assert_eq!(root_descr(&model), "The root");
    assert_eq!(
        model["rootNode"]["type"].as_i64().unwrap(),
        NODE_TYPE_RECT as i64
    );
}

#[test]
fn mindmap_child_node_id_and_type_rounded_rect() {
    let model = parse("mindmap\n    root\n      theId(child1)");
    let child = &model["rootNode"]["children"][0];
    assert_eq!(child["descr"].as_str().unwrap(), "child1");
    assert_eq!(child["nodeId"].as_str().unwrap(), "theId");
    assert_eq!(
        child["type"].as_i64().unwrap(),
        NODE_TYPE_ROUNDED_RECT as i64
    );
}

#[test]
fn mindmap_node_types_circle_cloud_bang_hexagon() {
    let circle = parse("mindmap\n root((the root))");
    assert_eq!(
        circle["rootNode"]["type"].as_i64().unwrap(),
        NODE_TYPE_CIRCLE as i64
    );
    assert_eq!(circle["rootNode"]["descr"].as_str().unwrap(), "the root");

    let cloud = parse("mindmap\n root)the root(");
    assert_eq!(
        cloud["rootNode"]["type"].as_i64().unwrap(),
        NODE_TYPE_CLOUD as i64
    );
    assert_eq!(cloud["rootNode"]["descr"].as_str().unwrap(), "the root");

    let bang = parse("mindmap\n root))the root((");
    assert_eq!(
        bang["rootNode"]["type"].as_i64().unwrap(),
        NODE_TYPE_BANG as i64
    );
    assert_eq!(bang["rootNode"]["descr"].as_str().unwrap(), "the root");

    let hex = parse("mindmap\n root{{the root}}");
    assert_eq!(
        hex["rootNode"]["type"].as_i64().unwrap(),
        NODE_TYPE_HEXAGON as i64
    );
    assert_eq!(hex["rootNode"]["descr"].as_str().unwrap(), "the root");
}

#[test]
fn mindmap_icon_and_class_decorations() {
    let model = parse("mindmap\n    root[The root]\n    :::m-4 p-8\n    ::icon(bomb)\n");
    assert_eq!(model["rootNode"]["class"].as_str().unwrap(), "m-4 p-8");
    assert_eq!(model["rootNode"]["icon"].as_str().unwrap(), "bomb");
}

#[test]
fn mindmap_can_set_icon_then_class_or_class_then_icon() {
    let model = parse("mindmap\n    root[The root]\n    :::m-4 p-8\n    ::icon(bomb)\n");
    assert_eq!(model["rootNode"]["class"].as_str().unwrap(), "m-4 p-8");
    assert_eq!(model["rootNode"]["icon"].as_str().unwrap(), "bomb");

    let model = parse("mindmap\n    root[The root]\n    ::icon(bomb)\n    :::m-4 p-8\n");
    assert_eq!(model["rootNode"]["class"].as_str().unwrap(), "m-4 p-8");
    assert_eq!(model["rootNode"]["icon"].as_str().unwrap(), "bomb");
}

#[test]
fn mindmap_quoted_descriptions_can_contain_delimiters() {
    let model = parse("mindmap\n    root[\"String containing []\"]");
    assert_eq!(model["rootNode"]["nodeId"].as_str().unwrap(), "root");
    assert_eq!(
        model["rootNode"]["descr"].as_str().unwrap(),
        "String containing []"
    );

    let model = parse(
        "mindmap\n    root[\"String containing []\"]\n      child1[\"String containing ()\"]",
    );
    assert_eq!(model["rootNode"]["children"].as_array().unwrap().len(), 1);
    assert_eq!(
        model["rootNode"]["children"][0]["descr"].as_str().unwrap(),
        "String containing ()"
    );
}

#[test]
fn mindmap_child_after_class_assignment_is_attached_to_last_node() {
    let model = parse(
        "mindmap\n  root(Root)\n    Child(Child)\n    :::hot\n      a(a)\n      b[New Stuff]",
    );
    let mm = &model["rootNode"];
    assert_eq!(mm["nodeId"].as_str().unwrap(), "root");
    let child = &mm["children"][0];
    assert_eq!(child["nodeId"].as_str().unwrap(), "Child");
    assert_eq!(child["children"].as_array().unwrap().len(), 2);
    assert_eq!(child["children"][0]["nodeId"].as_str().unwrap(), "a");
    assert_eq!(child["children"][1]["nodeId"].as_str().unwrap(), "b");
}

#[test]
fn mindmap_comment_end_of_line_is_ignored() {
    let model = parse(
        "mindmap\n  root(Root)\n    Child(Child)\n      a(a) %% This is a comment\n      b[New Stuff]\n",
    );
    let child = &model["rootNode"]["children"][0];
    assert_eq!(child["nodeId"].as_str().unwrap(), "Child");
    assert_eq!(child["children"].as_array().unwrap().len(), 2);
    assert_eq!(child["children"][1]["nodeId"].as_str().unwrap(), "b");
}

#[test]
fn mindmap_rows_above_declaration_are_ignored() {
    let model = parse("\n \n\nmindmap\nroot\n A\n \n\n B");
    assert_eq!(model["rootNode"]["nodeId"].as_str().unwrap(), "root");
    assert_eq!(model["rootNode"]["children"].as_array().unwrap().len(), 2);
}

#[test]
fn mindmap_leading_comment_lines_before_declaration_are_ignored() {
    let model = parse("%% comment\n\nmindmap\nroot\n A\n B");
    assert_eq!(model["rootNode"]["nodeId"].as_str().unwrap(), "root");
    assert_eq!(model["rootNode"]["children"].as_array().unwrap().len(), 2);
}

#[test]
fn mindmap_root_without_indent_child_with_indent() {
    let model = parse("mindmap\nroot\n      theId(child1)");
    let mm = &model["rootNode"];
    assert_eq!(mm["nodeId"].as_str().unwrap(), "root");
    assert_eq!(mm["children"].as_array().unwrap().len(), 1);
    let child = &mm["children"][0];
    assert_eq!(child["descr"].as_str().unwrap(), "child1");
    assert_eq!(child["nodeId"].as_str().unwrap(), "theId");
}

#[test]
fn mindmap_rows_with_only_spaces_do_not_interfere() {
    let model = parse("mindmap\nroot\n A\n \n\n B");
    let mm = &model["rootNode"];
    assert_eq!(mm["nodeId"].as_str().unwrap(), "root");
    assert_eq!(mm["children"].as_array().unwrap().len(), 2);
    assert_eq!(mm["children"][0]["nodeId"].as_str().unwrap(), "A");
    assert_eq!(mm["children"][1]["nodeId"].as_str().unwrap(), "B");
}

#[test]
fn mindmap_meaningless_empty_rows_do_not_interfere() {
    let model = parse("mindmap\n  root(Root)\n    Child(Child)\n      a(a)\n\n      b[New Stuff]");
    let mm = &model["rootNode"];
    assert_eq!(mm["nodeId"].as_str().unwrap(), "root");
    let child = &mm["children"][0];
    assert_eq!(child["nodeId"].as_str().unwrap(), "Child");
    assert_eq!(child["children"].as_array().unwrap().len(), 2);
    assert_eq!(child["children"][1]["nodeId"].as_str().unwrap(), "b");
}

#[test]
fn mindmap_header_can_share_line_with_root_node() {
    let model = parse("mindmap root\n  child1\n");
    let mm = &model["rootNode"];
    assert_eq!(mm["descr"].as_str().unwrap(), "root");
    assert_eq!(mm["children"].as_array().unwrap().len(), 1);
    assert_eq!(mm["children"][0]["descr"].as_str().unwrap(), "child1");
}

#[test]
fn mindmap_editor_facts_preserve_parser_node_spans() {
    let engine = Engine::new();
    let text = "mindmap root(Root Node)\n  child1(Child 1)\n  :::hot\n  ::icon(bomb)\n  child2\n";
    let facts = engine
        .parse_editor_semantic_facts_with_type_sync("mindmap", text)
        .unwrap()
        .expect("mindmap editor facts");

    assert_eq!(facts.completeness, EditorSemanticCompleteness::Complete);

    let symbol_at = |name: &str, start: usize| {
        facts
            .symbols
            .iter()
            .find(|symbol| symbol.name == name && symbol.selection.start == start)
            .unwrap_or_else(|| panic!("missing symbol {name} at {start}"))
    };

    let root_start = text.find("root(Root Node)").unwrap();
    assert_eq!(
        symbol_at("root", root_start).selection.end,
        root_start + "root".len()
    );

    let child1_start = text.find("child1").unwrap();
    assert_eq!(
        symbol_at("child1", child1_start).selection.end,
        child1_start + "child1".len()
    );

    let child2_start = text.find("child2").unwrap();
    assert_eq!(
        symbol_at("child2", child2_start).selection.end,
        child2_start + "child2".len()
    );

    let class_start = text.find("hot").unwrap();
    assert!(facts.symbols.iter().any(|symbol| {
        symbol.name == "hot"
            && symbol.role == EditorSemanticRole::Payload
            && symbol.detail.as_deref() == Some("mindmap class")
            && symbol.selection.start == class_start
            && symbol.selection.end == class_start + "hot".len()
    }));

    let icon_start = text.find("bomb").unwrap();
    assert!(facts.symbols.iter().any(|symbol| {
        symbol.name == "bomb"
            && symbol.role == EditorSemanticRole::Payload
            && symbol.detail.as_deref() == Some("mindmap icon")
            && symbol.selection.start == icon_start
            && symbol.selection.end == icon_start + "bomb".len()
    }));

    let label_start = text.find("Root Node").unwrap();
    assert!(facts.symbols.iter().any(|symbol| {
        symbol.name == "Root Node"
            && symbol.role == EditorSemanticRole::Payload
            && symbol.detail.as_deref() == Some("mindmap node label")
            && symbol.selection.start == label_start
            && symbol.selection.end == label_start + "Root Node".len()
    }));

    assert!(facts.expected_syntax.iter().any(|expected| {
        expected.kind == EditorExpectedSyntaxKind::NodeIdentifier
            && expected.span.start == root_start
            && expected.span.end == root_start + "root".len()
    }));
    assert!(facts.expected_syntax.iter().any(|expected| {
        expected.kind == EditorExpectedSyntaxKind::Payload
            && expected.span.start == text.find("Root Node").unwrap()
            && expected.span.end == text.find("Root Node").unwrap() + "Root Node".len()
    }));

    assert!(facts.directive_prefixes.iter().any(|p| p == ":::"));
    assert!(facts.directive_prefixes.iter().any(|p| p == "::icon"));
}

#[test]
fn mindmap_parser_preserves_crlf_unicode_multiline_semantic_spans() {
    let source = concat!(
        "mindmap root[\"`根节点 🌳\r\n",
        "第二行`\"]\r\n",
        "  bare节点\r\n",
        "  重复[\"重复\"]\r\n",
        "  (无 ID)\r\n",
        "  :::hot warm\r\n",
        "  ::icon(lucide:home)\r\n",
        "  %% global comment\r\n",
    );
    let facts = Engine::new()
        .parse_editor_semantic_facts_with_type_sync("mindmap", source)
        .expect("mindmap editor parse")
        .expect("mindmap editor facts");

    assert_eq!(facts.completeness, EditorSemanticCompleteness::Complete);
    let repeated = "重复[\"重复\"]";
    let repeated_start = source.find(repeated).expect("repeated node source");
    let id = facts
        .symbols
        .iter()
        .find(|symbol| symbol.name == "重复" && symbol.detail.as_deref() == Some("mindmap node"))
        .expect("repeated id symbol");
    let label = facts
        .symbols
        .iter()
        .find(|symbol| {
            symbol.name == "重复" && symbol.detail.as_deref() == Some("mindmap node label")
        })
        .expect("repeated label symbol");
    assert_eq!(
        id.selection,
        SourceSpan::new(repeated_start, repeated_start + "重复".len())
    );
    let label_start = repeated_start + "重复[\"".len();
    assert_eq!(
        label.selection,
        SourceSpan::new(label_start, label_start + "重复".len()),
    );
}

#[test]
fn mindmap_unquoted_multiline_nodes_continue_until_the_shape_delimiter() {
    let source = concat!(
        "mindmap\n",
        "  root[\n",
        "    Multi-line root\n",
        "    with Unicode 🤓\n",
        "  ]\n",
        "    child[Later child]\n",
    );
    let parsed = Engine::new()
        .parse_diagram_snapshot_sync(source)
        .expect("unquoted multiline mindmap parses")
        .expect("mindmap model");

    assert_eq!(
        parsed
            .outcome()
            .parsed_model()
            .expect("expected parsed snapshot")["rootNode"]["nodeId"],
        "root"
    );
    assert_eq!(
        parsed
            .outcome()
            .parsed_model()
            .expect("expected parsed snapshot")["rootNode"]["descr"],
        "\n    Multi-line root\n    with Unicode 🤓\n  "
    );
    assert_eq!(
        parsed
            .outcome()
            .parsed_model()
            .expect("expected parsed snapshot")["rootNode"]["children"][0]["nodeId"],
        "child"
    );
    let crate::ParsedEditorFacts::Available(facts) = parsed.editor_facts() else {
        panic!("mindmap editor facts");
    };
    assert_eq!(facts.completeness, EditorSemanticCompleteness::Complete);
}

#[test]
fn mindmap_bare_markdown_continuation_stays_open_across_same_indent_content() {
    let source = concat!(
        "mindmap\n",
        "    id1[`**Start** with\n",
        "    a same-indent second line`]\n",
        "      child[Later child]\n",
    );
    let parsed = Engine::new()
        .parse_diagram_snapshot_sync(source)
        .expect("bare markdown mindmap parses")
        .expect("mindmap model");

    assert_eq!(
        parsed
            .outcome()
            .parsed_model()
            .expect("expected parsed snapshot")["rootNode"]["nodeId"],
        "id1"
    );
    assert_eq!(
        parsed
            .outcome()
            .parsed_model()
            .expect("expected parsed snapshot")["rootNode"]["descr"],
        "`**Start** with\n    a same-indent second line`"
    );
    assert_eq!(
        parsed
            .outcome()
            .parsed_model()
            .expect("expected parsed snapshot")["rootNode"]["children"][0]["nodeId"],
        "child"
    );
    let crate::ParsedEditorFacts::Available(facts) = parsed.editor_facts() else {
        panic!("mindmap editor facts");
    };
    assert_eq!(facts.completeness, EditorSemanticCompleteness::Complete);
}

#[test]
fn mindmap_unmatched_bare_backtick_does_not_hide_the_shape_closing_delimiter() {
    let source = "mindmap\n  root[Use ` unmatched marker]\n";
    let parsed = Engine::new()
        .parse_diagram_snapshot_sync(source)
        .expect("mindmap node with unmatched bare backtick parses")
        .expect("mindmap model");

    assert_eq!(
        parsed
            .outcome()
            .parsed_model()
            .expect("expected parsed snapshot")["rootNode"]["nodeId"],
        "root"
    );
    assert_eq!(
        parsed
            .outcome()
            .parsed_model()
            .expect("expected parsed snapshot")["rootNode"]["descr"],
        "Use ` unmatched marker"
    );
    let crate::ParsedEditorFacts::Available(facts) = parsed.editor_facts() else {
        panic!("mindmap editor facts");
    };
    assert_eq!(facts.completeness, EditorSemanticCompleteness::Complete);
}

#[test]
fn mindmap_recovery_keeps_failed_prefix_and_later_node_semantics() {
    let source = concat!(
        "mindmap\n",
        " root\n",
        "  broken[unterminated\n",
        "  ::icon(lucide:home)\n",
        "  后续(After)\n",
        "  another[unterminated\n",
        "  :::hot\n",
    );
    let engine = Engine::new();
    let error = engine
        .parse_diagram_sync(source, ParseOptions::strict())
        .expect_err("strict mindmap parse must return the first syntax error");
    assert!(error.to_string().contains("unterminated node delimiter"));

    let facts = engine
        .parse_editor_semantic_facts_with_type_sync("mindmap", source)
        .expect("mindmap recovery parse")
        .expect("mindmap recovery facts");

    assert_eq!(facts.completeness, EditorSemanticCompleteness::Recovered);
    assert!(facts.symbols.iter().any(|symbol| symbol.name == "后续"));
    assert!(
        facts
            .diagnostics
            .iter()
            .any(|diagnostic| diagnostic.message.contains("unterminated node delimiter"))
    );
}

#[test]
fn mindmap_recovery_constructs_one_parser_outcome() {
    reset_mindmap_syntax_construction_count();
    let facts = Engine::new()
        .parse_editor_semantic_facts_with_type_sync(
            "mindmap",
            "mindmap\n root\n  broken[unterminated\n  after\n",
        )
        .expect("mindmap recovery parse")
        .expect("mindmap recovery facts");

    assert_eq!(facts.completeness, EditorSemanticCompleteness::Recovered);
    assert_eq!(mindmap_syntax_construction_count(), 1);
}

#[test]
fn mindmap_editor_facts_recovers_from_incomplete_input() {
    let engine = Engine::new();
    let text = "mindmap\nroot\n child[unterminated";
    let facts = engine
        .parse_editor_semantic_facts_with_type_sync("mindmap", text)
        .unwrap()
        .expect("mindmap editor facts");

    assert_eq!(facts.completeness, EditorSemanticCompleteness::Recovered);
    let child_start = text.find("child").unwrap();
    assert!(facts.diagnostics.iter().any(|diagnostic| {
        diagnostic.message.contains("unterminated node delimiter")
            && diagnostic.span == Some(SourceSpan::new(child_start, text.len()))
    }));
    assert!(facts.symbols.iter().any(|symbol| symbol.name == "root"));
    assert!(!facts.symbols.iter().any(|symbol| symbol.name == "child"));
}

#[test]
fn mindmap_editor_facts_preserve_prior_symbols_when_later_node_is_invalid() {
    let engine = Engine::new();
    let text = "mindmap\nroot\n child1\n child2[broken]]\n";
    let facts = engine
        .parse_editor_semantic_facts_with_type_sync("mindmap", text)
        .unwrap()
        .expect("mindmap editor facts");

    assert_eq!(facts.completeness, EditorSemanticCompleteness::Recovered);
    assert!(facts.symbols.iter().any(|symbol| symbol.name == "root"));
    assert!(facts.symbols.iter().any(|symbol| symbol.name == "child1"));
    assert!(!facts.symbols.iter().any(|symbol| symbol.name == "child2"));
    assert!(
        facts
            .diagnostics
            .iter()
            .any(|diagnostic| diagnostic.message.contains("unexpected trailing input"))
    );
}

#[test]
fn mindmap_editor_facts_report_database_validation_errors() {
    let text = "mindmap\r\n    root\r\n    fakeRoot\r\n";
    let invalid = "fakeRoot";
    let start = text.find(invalid).unwrap();
    let expected_span = SourceSpan::new(start, start + invalid.len());
    let engine = Engine::new();

    let error = engine
        .parse_diagram_sync(text, ParseOptions::strict())
        .expect_err("multiple mindmap roots must fail strict parsing");
    let crate::Error::DiagramParse { diagnostic, .. } = error else {
        panic!("expected mindmap parse diagnostic");
    };
    assert_eq!(diagnostic.span(), Some(expected_span));

    let facts = engine
        .parse_editor_semantic_facts_with_type_sync("mindmap", text)
        .unwrap()
        .expect("mindmap editor recovery facts");
    assert_eq!(facts.completeness, EditorSemanticCompleteness::Recovered);
    assert!(facts.symbols.iter().any(|symbol| symbol.name == "root"));
    assert!(facts.symbols.iter().any(|symbol| symbol.name == "fakeRoot"));
    assert!(facts.diagnostics.iter().any(|diagnostic| {
        diagnostic.kind == crate::EditorSemanticDiagnosticKind::ParserRecovery
            && diagnostic.span == Some(expected_span)
            && diagnostic.message.contains("There can be only one root")
    }));
}

#[test]
fn mindmap_editor_facts_report_invalid_header() {
    let text = "  mindmap-beta\r\nroot\r\n";
    let invalid = "mindmap-beta";
    let start = text.find(invalid).unwrap();
    let expected_span = SourceSpan::new(start, start + invalid.len());
    let facts = Engine::new()
        .parse_editor_semantic_facts_with_type_sync("mindmap", text)
        .unwrap()
        .expect("mindmap editor recovery facts");

    assert_eq!(facts.completeness, EditorSemanticCompleteness::Recovered);
    assert!(facts.symbols.is_empty());
    assert!(facts.diagnostics.iter().any(|diagnostic| {
        diagnostic.kind == crate::EditorSemanticDiagnosticKind::ParserRecovery
            && diagnostic.span == Some(expected_span)
            && diagnostic.message.contains("expected mindmap header")
    }));
}

#[test]
fn mindmap_multiline_markdown_string_node_description_is_parsed() {
    let model = parse(
        "mindmap\n    id1[\"`**Root** with\n\
a second line\n\
Unicode works too: 🤓`\"]\n      id2[\"`The dog in **the** hog... a *very long text* that wraps to a new line`\"]\n      id3[Regular labels still works]\n",
    );
    let root = &model["rootNode"];
    assert_eq!(root["nodeId"].as_str().unwrap(), "id1");
    let descr = root["descr"].as_str().unwrap();
    assert!(descr.contains("Root"));
    assert!(descr.contains("a second line"));
    assert!(descr.contains("🤓"));
}

#[test]
fn mindmap_get_data_empty_when_no_nodes() {
    let model = parse("mindmap\n");
    assert_eq!(model["nodes"].as_array().unwrap().len(), 0);
    assert_eq!(model["edges"].as_array().unwrap().len(), 0);
    assert!(model.get("rootNode").is_none());
    assert!(model.get("config").is_some());
}

#[test]
fn mindmap_get_data_basic_nodes_edges_and_layout_defaults() {
    let model = parse("mindmap\nroot(Root Node)\n child1(Child 1)\n child2(Child 2)\n");

    assert_eq!(model["nodes"].as_array().unwrap().len(), 3);
    assert_eq!(model["edges"].as_array().unwrap().len(), 2);
    assert_eq!(model["config"]["layout"].as_str().unwrap(), "cose-bilkent");
    assert!(model["diagramId"].as_str().unwrap().starts_with("mindmap-"));

    let root = model["nodes"]
        .as_array()
        .unwrap()
        .iter()
        .find(|n| n["id"].as_str() == Some("0"))
        .unwrap();
    assert_eq!(root["label"].as_str().unwrap(), "Root Node");
    assert_eq!(root["level"].as_i64().unwrap(), 0);

    let edge_0_1 = model["edges"]
        .as_array()
        .unwrap()
        .iter()
        .find(|e| e["start"].as_str() == Some("0") && e["end"].as_str() == Some("1"))
        .unwrap();
    assert_eq!(edge_0_1["depth"].as_i64().unwrap(), 0);
}

#[test]
fn mindmap_diagram_id_is_stable_for_the_same_operation_seed() {
    let source = "mindmap\nroot\n child\n";

    let first = parse(source);
    let repeated = parse(source);

    assert_eq!(first["diagramId"], repeated["diagramId"]);
}

#[test]
fn mindmap_diagram_id_is_domain_derived_from_the_operation_seed() {
    fn diagram_id(seed: u64) -> String {
        let engine = Engine::new().with_runtime_policy(
            crate::runtime::RuntimePolicy::deterministic().with_fixed_seed(seed),
        );
        block_on(engine.parse_diagram("mindmap\nroot\n", ParseOptions::default()))
            .unwrap()
            .unwrap()
            .model["diagramId"]
            .as_str()
            .unwrap()
            .to_string()
    }

    let first = diagram_id(11);
    let other = diagram_id(22);
    let replayed = diagram_id(11);

    assert_eq!(first, replayed);
    assert_ne!(first, other);

    let uuid = first.strip_prefix("mindmap-").unwrap();
    let groups = uuid.split('-').collect::<Vec<_>>();
    assert_eq!(
        groups.iter().map(|group| group.len()).collect::<Vec<_>>(),
        [8, 4, 4, 4, 12]
    );
    assert!(groups[2].starts_with('4'));
    assert!(matches!(groups[3].as_bytes()[0], b'8' | b'9' | b'a' | b'b'));
}

#[test]
fn mindmap_typed_render_model_projects_exact_compatibility_json() {
    let source = concat!(
        "mindmap root(Root Node)\n",
        "  :::root-class\n",
        "  child1[Child 1]\n",
        "  ::icon(bomb)\n",
        "  child2[\"`**Markdown** child`\"]\n",
    );
    let engine = Engine::new();
    let parsed = block_on(engine.parse_diagram(source, ParseOptions::strict()))
        .unwrap()
        .unwrap();
    let typed = parse_mindmap_model_for_render(source, &parsed.meta).unwrap();
    let mut projected = render_model_to_compat_json(&typed, &parsed.meta).unwrap();
    let mut expected = parsed.model.into_unmanaged_value();

    projected["diagramId"] = Value::String("<dynamic>".to_string());
    expected["diagramId"] = Value::String("<dynamic>".to_string());
    assert_eq!(
        projected, expected,
        "Mindmap typed compatibility projection must preserve the exact public JSON"
    );
    assert_eq!(projected["rootNode"]["class"], "root-class");
    assert_eq!(projected["rootNode"]["children"][0]["padding"], 20);
    assert_eq!(projected["rootNode"]["children"][0]["icon"], "bomb");
    assert_eq!(projected["nodes"][0]["labelType"], "markdown");
    assert_eq!(projected["nodes"][1]["labelType"], "markdown");
    assert_eq!(projected["nodes"][2]["labelType"], "markdown");
}

#[test]
fn mindmap_json_projection_observes_cancellation_inside_large_models() {
    let source = deep_mindmap_chain(512);
    let engine = Engine::new();
    let parsed = block_on(engine.parse_diagram(&source, ParseOptions::strict()))
        .unwrap()
        .unwrap();
    let typed = parse_mindmap_model_for_render(&source, &parsed.meta).unwrap();
    let control = crate::OperationControl::new();
    control.cancel_after_checkpoints(2);

    assert!(
        super::render_model::render_model_to_compat_json_controlled(
            &typed,
            &parsed.meta,
            &control,
        )
        .is_err()
    );
}

#[cfg(feature = "all-diagrams")]
#[test]
fn mindmap_projection_cancels_after_deep_completed_subtree_child() {
    crate::diagrams::treemap::tests::run_lifecycle_child(
        "diagrams::mindmap::tests::mindmap_projection_cancels_after_deep_completed_subtree_child",
        || {
            const DEPTH: usize = 3_000;
            let mut source = deep_mindmap_chain(DEPTH + 1);
            source.push_str(" sibling\n");
            let parsed = Engine::new()
                .parse_diagram_for_render_model_sync(&source, ParseOptions::strict())
                .unwrap()
                .unwrap();
            let RenderSemanticModel::Mindmap(model) = parsed.model() else {
                panic!("typed mindmap");
            };
            let control = crate::OperationControl::new();
            let node_count = DEPTH + 2;
            let edge_count = DEPTH + 1;
            // Cancel at the second root-child transfer; the first deep child is in a managed vector.
            let checkpoints = node_count.div_ceil(128)
                + 2
                + edge_count.div_ceil(128)
                + (2 * node_count).div_ceil(128)
                + edge_count
                + DEPTH;
            control.cancel_after_checkpoints(checkpoints);
            assert!(matches!(
                super::render_model::mindmap_root_node_to_compat_json_controlled(
                    model,
                    parsed.metadata(),
                    &control
                ),
                Err(crate::OperationCancelled { .. })
            ));
            let projected =
                super::render_model::render_model_to_compat_json(model, parsed.metadata()).unwrap();
            drop(crate::ManagedSemanticJson::from_value(projected));
            drop(parsed);
        },
    );
}

#[cfg(feature = "all-diagrams")]
#[test]
fn mindmap_projection_failure_drops_deep_completed_children_child() {
    crate::diagrams::treemap::tests::run_lifecycle_child(
        "diagrams::mindmap::tests::mindmap_projection_failure_drops_deep_completed_children_child",
        || {
            let source = deep_mindmap_chain(3_001);
            let parsed = Engine::new()
                .parse_diagram_for_render_model_sync(&source, ParseOptions::strict())
                .unwrap()
                .unwrap();
            let RenderSemanticModel::Mindmap(model) = parsed.model() else {
                panic!("typed mindmap");
            };
            let mut model = model.clone();
            let root = model.nodes[0].id.clone();
            model.nodes[0].id = "invalid-number".to_string();
            for edge in &mut model.edges {
                if edge.start == root {
                    edge.start = "invalid-number".to_string();
                }
            }
            assert!(
                super::render_model::mindmap_root_node_to_compat_json_controlled(
                    &model,
                    parsed.metadata(),
                    &crate::OperationControl::new()
                )
                .unwrap()
                .is_err()
            );
            drop(model);
            drop(parsed);
        },
    );
}

#[test]
fn mindmap_get_data_projects_look_and_theme_shape_like_mermaid_11_15() {
    let model = parse(
        r#"%%{init: {"theme": "redux", "look": "neo"}}%%
mindmap
root
 child
"#,
    );

    let nodes = model["nodes"].as_array().unwrap();
    let root = nodes
        .iter()
        .find(|n| n["id"].as_str() == Some("0"))
        .unwrap();
    let child = nodes
        .iter()
        .find(|n| n["id"].as_str() == Some("1"))
        .unwrap();
    assert_eq!(root["look"].as_str().unwrap(), "neo");
    assert_eq!(child["look"].as_str().unwrap(), "neo");
    assert_eq!(root["shape"].as_str().unwrap(), "rounded");
    assert_eq!(child["shape"].as_str().unwrap(), "rounded");
    assert_eq!(model["shapes"]["0"]["shape"].as_str().unwrap(), "rounded");

    let edge = model["edges"].as_array().unwrap()[0].as_object().unwrap();
    assert_eq!(edge["look"].as_str().unwrap(), "neo");

    let default_model = parse("mindmap\nroot\n child\n");
    assert_eq!(
        default_model["nodes"][0]["look"].as_str().unwrap(),
        "classic"
    );
    assert_eq!(
        default_model["nodes"][0]["shape"].as_str().unwrap(),
        "defaultMindmapNode"
    );
}

#[test]
fn mindmap_render_model_projects_same_look_and_theme_shape_as_json_model() {
    let engine = Engine::new();
    let input = r#"%%{init: {"theme": "redux", "look": "neo"}}%%
mindmap
root
 child
"#;

    let parsed = engine
        .parse_diagram_for_render_model_sync(input, ParseOptions::strict())
        .unwrap()
        .unwrap();

    match parsed.model() {
        RenderSemanticModel::Mindmap(model) => {
            assert_eq!(model.nodes[0].look, "neo");
            assert_eq!(model.nodes[1].look, "neo");
            assert_eq!(model.nodes[0].shape, "rounded");
            assert_eq!(model.nodes[1].shape, "rounded");
            assert_eq!(model.edges[0].look, "neo");
        }
        other => panic!("mindmap render parse should return typed model, got {other:?}"),
    }
}

#[test]
fn mindmap_deep_chain_semantic_and_render_model_use_heap_traversal() {
    const DEPTH: usize = 1200;
    let input = deep_mindmap_chain(DEPTH);

    let model = parse(&input);
    let nodes = model["nodes"].as_array().expect("nodes array");
    let edges = model["edges"].as_array().expect("edges array");
    assert_eq!(nodes.len(), DEPTH);
    assert_eq!(edges.len(), DEPTH - 1);
    assert_eq!(nodes[0]["label"].as_str(), Some("n0"));
    let expected_last = format!("n{}", DEPTH - 1);
    assert_eq!(
        nodes
            .last()
            .and_then(|node| node.get("label"))
            .and_then(Value::as_str),
        Some(expected_last.as_str())
    );

    let parsed = Engine::new()
        .parse_diagram_for_render_model_sync(&input, ParseOptions::strict())
        .unwrap()
        .unwrap();
    match parsed.model() {
        RenderSemanticModel::Mindmap(model) => {
            assert_eq!(model.nodes.len(), DEPTH);
            assert_eq!(model.edges.len(), DEPTH - 1);
            assert_eq!(model.nodes[0].label, "n0");
            assert_eq!(
                model.nodes.last().map(|node| node.label.as_str()),
                Some(expected_last.as_str())
            );
        }
        other => panic!("mindmap render parse should return typed model, got {other:?}"),
    }
}

#[test]
fn mindmap_get_data_assigns_section_classes_to_nodes_and_edges() {
    let model = parse("mindmap\nA\n a0\n  aa0\n a1\n  aaa\n a2\n");
    let nodes = model["nodes"].as_array().unwrap();

    let node_a = nodes
        .iter()
        .find(|n| n["label"].as_str() == Some("A"))
        .unwrap();
    let node_a0 = nodes
        .iter()
        .find(|n| n["label"].as_str() == Some("a0"))
        .unwrap();
    let node_aa0 = nodes
        .iter()
        .find(|n| n["label"].as_str() == Some("aa0"))
        .unwrap();
    let node_a1 = nodes
        .iter()
        .find(|n| n["label"].as_str() == Some("a1"))
        .unwrap();
    let node_aaa = nodes
        .iter()
        .find(|n| n["label"].as_str() == Some("aaa"))
        .unwrap();
    let node_a2 = nodes
        .iter()
        .find(|n| n["label"].as_str() == Some("a2"))
        .unwrap();

    assert!(node_a.get("section").is_none());
    assert_eq!(
        node_a["cssClasses"].as_str().unwrap(),
        "mindmap-node section-root section--1"
    );
    assert_eq!(node_a0["section"].as_i64().unwrap(), 0);
    assert_eq!(node_aa0["section"].as_i64().unwrap(), 0);
    assert_eq!(node_a1["section"].as_i64().unwrap(), 1);
    assert_eq!(node_aaa["section"].as_i64().unwrap(), 1);
    assert_eq!(node_a2["section"].as_i64().unwrap(), 2);

    let edges = model["edges"].as_array().unwrap();
    assert_eq!(edges.len(), 5);

    let edge_0_1 = edges
        .iter()
        .find(|e| e["start"].as_str() == Some("0") && e["end"].as_str() == Some("1"))
        .unwrap();
    let edge_1_2 = edges
        .iter()
        .find(|e| e["start"].as_str() == Some("1") && e["end"].as_str() == Some("2"))
        .unwrap();
    let edge_0_3 = edges
        .iter()
        .find(|e| e["start"].as_str() == Some("0") && e["end"].as_str() == Some("3"))
        .unwrap();
    let edge_3_4 = edges
        .iter()
        .find(|e| e["start"].as_str() == Some("3") && e["end"].as_str() == Some("4"))
        .unwrap();
    let edge_0_5 = edges
        .iter()
        .find(|e| e["start"].as_str() == Some("0") && e["end"].as_str() == Some("5"))
        .unwrap();

    assert_eq!(
        edge_0_1["classes"].as_str().unwrap(),
        "edge section-edge-0 edge-depth-1"
    );
    assert_eq!(
        edge_1_2["classes"].as_str().unwrap(),
        "edge section-edge-0 edge-depth-2"
    );
    assert_eq!(
        edge_0_3["classes"].as_str().unwrap(),
        "edge section-edge-1 edge-depth-1"
    );
    assert_eq!(
        edge_3_4["classes"].as_str().unwrap(),
        "edge section-edge-1 edge-depth-2"
    );
    assert_eq!(
        edge_0_5["classes"].as_str().unwrap(),
        "edge section-edge-2 edge-depth-1"
    );

    assert_eq!(edge_0_1["section"].as_i64().unwrap(), 0);
    assert_eq!(edge_1_2["section"].as_i64().unwrap(), 0);
    assert_eq!(edge_0_3["section"].as_i64().unwrap(), 1);
    assert_eq!(edge_3_4["section"].as_i64().unwrap(), 1);
    assert_eq!(edge_0_5["section"].as_i64().unwrap(), 2);
}

#[test]
fn mindmap_get_data_edge_ids_are_unique() {
    let model = parse("mindmap\nroot\n child1\n child2\n child3\n");
    let edges = model["edges"].as_array().unwrap();
    assert_eq!(edges.len(), 3);

    let ids: Vec<&str> = edges.iter().map(|e| e["id"].as_str().unwrap()).collect();
    let unique: std::collections::BTreeSet<&str> = ids.iter().copied().collect();
    assert_eq!(unique.len(), ids.len());
}

#[test]
fn mindmap_get_data_missing_optional_properties_are_absent() {
    let model = parse("mindmap\nroot\n");
    let nodes = model["nodes"].as_array().unwrap();
    assert_eq!(nodes.len(), 1);
    let node = nodes[0].as_object().unwrap();

    assert!(node.get("section").is_none());
    assert_eq!(
        node.get("cssClasses").and_then(|v| v.as_str()).unwrap(),
        "mindmap-node section-root section--1"
    );
    assert!(node.get("icon").is_none());
    assert!(node.get("x").is_none());
    assert!(node.get("y").is_none());
}

#[test]
fn mindmap_get_data_preserves_custom_classes_while_adding_section_classes() {
    let model = parse(
        "mindmap\nroot(Root Node)\n:::custom-root-class\n child(Child Node)\n :::custom-child-class\n",
    );

    let nodes = model["nodes"].as_array().unwrap();
    let root = nodes
        .iter()
        .find(|n| n["label"].as_str() == Some("Root Node"))
        .unwrap();
    let child = nodes
        .iter()
        .find(|n| n["label"].as_str() == Some("Child Node"))
        .unwrap();

    assert_eq!(
        root["cssClasses"].as_str().unwrap(),
        "mindmap-node section-root section--1 custom-root-class"
    );
    assert_eq!(
        child["cssClasses"].as_str().unwrap(),
        "mindmap-node section-0 custom-child-class"
    );
}

#[test]
fn mindmap_padding_doubles_for_rect_like_nodes() {
    let model = parse("mindmap\nroot[Root]\n");
    let node = model["nodes"]
        .as_array()
        .unwrap()
        .iter()
        .find(|n| n["id"].as_str() == Some("0"))
        .unwrap();
    assert_eq!(node["type"].as_i64().unwrap(), NODE_TYPE_RECT as i64);
    assert_eq!(node["padding"].as_i64().unwrap(), 20);
}

#[test]
fn mindmap_empty_rows_and_comments_do_not_interfere() {
    let model = parse(
        "mindmap\n  root(Root)\n    Child(Child)\n      a(a)\n\n      %% This is a comment\n      b[New Stuff]\n",
    );
    let child = &model["rootNode"]["children"][0];
    assert_eq!(child["nodeId"].as_str().unwrap(), "Child");
    assert_eq!(child["children"].as_array().unwrap().len(), 2);
    assert_eq!(child["children"][1]["nodeId"].as_str().unwrap(), "b");
}
