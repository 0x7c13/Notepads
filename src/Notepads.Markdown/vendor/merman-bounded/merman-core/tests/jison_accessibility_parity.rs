#![cfg(feature = "all-diagrams")]

use merman_core::{
    DiagramParseOutcome, EditorSemanticCompleteness, EditorSemanticFacts, Engine, ParseOptions,
    ParsedEditorFacts, RenderSemanticModel,
};
use serde_json::{Value, json};

struct JisonCase {
    family: &'static str,
    diagram_type: &'static str,
    oracle: &'static str,
    source: &'static str,
    accepted: bool,
}

fn available_facts(snapshot: &merman_core::DiagramParseSnapshot) -> &EditorSemanticFacts {
    let ParsedEditorFacts::Available(facts) = snapshot.editor_facts() else {
        panic!("Jison-backed family must retain editor facts");
    };
    facts
}

fn assert_tail_semantics(family: &str, model: &Value) {
    match family {
        "flowchart" => {
            let nodes = model["nodes"].as_array().expect("Flowchart nodes");
            for id in ["A", "B"] {
                assert!(nodes.iter().any(|node| node["id"] == json!(id)));
            }
        }
        "sequence" => assert!(model["actors"].get("A").is_some()),
        "state" => {
            assert!(model["states"].get("A").is_some());
            assert!(model["states"].get("B").is_some());
        }
        "er" => {
            assert!(model["entities"].get("A").is_some());
            assert!(model["entities"].get("B").is_some());
        }
        "gantt" | "journey" | "timeline" | "quadrant" => {
            assert_eq!(model["title"], json!("Tail"));
        }
        "xychart" => {
            assert_eq!(model["xAxis"]["categories"], json!(["a", "b"]));
            assert_eq!(model["plots"].as_array().map(Vec::len), Some(1));
        }
        "requirement" => assert_eq!(model["requirements"][0]["name"], json!("R")),
        "c4" => assert_eq!(model["shapes"][0]["alias"], json!("a")),
        family => panic!("missing tail assertion for {family}"),
    }
}

#[test]
fn pinned_jison_closing_brace_resumes_same_line_lexing() {
    let cases = [
        JisonCase {
            family: "flowchart",
            diagram_type: "flowchart-v2",
            oracle: "flowchart/parser/flow.jison",
            source: "flowchart TD\naccDescr {desc} A-->B",
            accepted: true,
        },
        JisonCase {
            family: "sequence",
            diagram_type: "sequence",
            oracle: "sequence/parser/sequenceDiagram.jison",
            source: "sequenceDiagram\naccDescr {desc} participant A",
            accepted: true,
        },
        JisonCase {
            family: "state",
            diagram_type: "state",
            oracle: "state/parser/stateDiagram.jison",
            source: "stateDiagram-v2\naccDescr {desc} A --> B",
            accepted: true,
        },
        JisonCase {
            family: "er",
            diagram_type: "er",
            oracle: "er/parser/erDiagram.jison",
            source: "erDiagram\naccDescr {desc} A ||--|| B : rel",
            accepted: true,
        },
        JisonCase {
            family: "gantt",
            diagram_type: "gantt",
            oracle: "gantt/parser/gantt.jison",
            source: "gantt\naccDescr {desc} title Tail",
            accepted: true,
        },
        JisonCase {
            family: "journey",
            diagram_type: "journey",
            oracle: "user-journey/parser/journey.jison",
            source: "journey\naccDescr {desc} title Tail",
            accepted: true,
        },
        JisonCase {
            family: "timeline",
            diagram_type: "timeline",
            oracle: "timeline/parser/timeline.jison",
            source: "timeline\naccDescr {desc} title Tail",
            accepted: true,
        },
        JisonCase {
            family: "quadrant",
            diagram_type: "quadrantChart",
            oracle: "quadrant-chart/parser/quadrant.jison",
            source: "quadrantChart\naccDescr {desc}; title Tail",
            accepted: true,
        },
        JisonCase {
            family: "xychart",
            diagram_type: "xychart",
            oracle: "xychart/parser/xychart.jison",
            source: "xychart\naccDescr {desc} x-axis [a,b]\nline [1,2]",
            accepted: true,
        },
        JisonCase {
            family: "requirement",
            diagram_type: "requirement",
            oracle: "requirement/parser/requirementDiagram.jison",
            source: "requirementDiagram\naccDescr {desc} requirement R {\nid: 1\n}",
            accepted: true,
        },
        JisonCase {
            family: "c4",
            diagram_type: "c4",
            oracle: "c4/parser/c4Diagram.jison",
            source: "C4Context\naccDescr {desc} Person(a, \"A\")",
            accepted: true,
        },
        JisonCase {
            family: "class",
            diagram_type: "class",
            oracle: "class/parser/classDiagram.jison",
            source: "classDiagram\naccDescr {desc} class A",
            accepted: false,
        },
        JisonCase {
            family: "block",
            diagram_type: "block",
            oracle: "block/parser/block.jison",
            source: "block-beta\naccDescr {desc} A",
            accepted: false,
        },
    ];

    for case in cases {
        let snapshot = Engine::new()
            .parse_diagram_snapshot_with_type_sync(case.diagram_type, case.source)
            .unwrap_or_else(|error| panic!("{} operation failed: {error}", case.family))
            .unwrap_or_else(|| panic!("{} snapshot missing", case.family));
        let facts = available_facts(&snapshot);

        match snapshot.outcome() {
            DiagramParseOutcome::Parsed { model, .. } if case.accepted => {
                assert_eq!(
                    model["accDescr"],
                    json!("desc"),
                    "{} ({})",
                    case.family,
                    case.oracle
                );
                assert_tail_semantics(case.family, model);
                assert_eq!(facts.completeness, EditorSemanticCompleteness::Complete);
            }
            DiagramParseOutcome::Failed(_) if !case.accepted => {
                assert_eq!(facts.completeness, EditorSemanticCompleteness::Recovered);
            }
            outcome => panic!(
                "{} diverged from {}: accepted={}, outcome={outcome:?}",
                case.family, case.oracle, case.accepted
            ),
        }
    }
}

#[test]
fn pinned_jison_unterminated_accessibility_state_matches_family_outcome() {
    let cases = [
        JisonCase {
            family: "flowchart",
            diagram_type: "flowchart-v2",
            oracle: "flowchart/parser/flow.jison",
            source: "flowchart TD\naccDescr {partial\nA-->B",
            accepted: true,
        },
        JisonCase {
            family: "sequence",
            diagram_type: "sequence",
            oracle: "sequence/parser/sequenceDiagram.jison",
            source: "sequenceDiagram\naccDescr {partial\nparticipant A",
            accepted: true,
        },
        JisonCase {
            family: "state",
            diagram_type: "state",
            oracle: "state/parser/stateDiagram.jison",
            source: "stateDiagram-v2\naccDescr {partial\nA --> B",
            accepted: true,
        },
        JisonCase {
            family: "xychart",
            diagram_type: "xychart",
            oracle: "xychart/parser/xychart.jison",
            source: "xychart\naccDescr {partial\nline [1]",
            accepted: true,
        },
        JisonCase {
            family: "er",
            diagram_type: "er",
            oracle: "er/parser/erDiagram.jison",
            source: "erDiagram\naccDescr {partial",
            accepted: false,
        },
        JisonCase {
            family: "class",
            diagram_type: "class",
            oracle: "class/parser/classDiagram.jison",
            source: "classDiagram\naccDescr {partial",
            accepted: false,
        },
        JisonCase {
            family: "gantt",
            diagram_type: "gantt",
            oracle: "gantt/parser/gantt.jison",
            source: "gantt\naccDescr {partial",
            accepted: false,
        },
        JisonCase {
            family: "journey",
            diagram_type: "journey",
            oracle: "user-journey/parser/journey.jison",
            source: "journey\naccDescr {partial",
            accepted: false,
        },
        JisonCase {
            family: "timeline",
            diagram_type: "timeline",
            oracle: "timeline/parser/timeline.jison",
            source: "timeline\naccDescr {partial",
            accepted: false,
        },
        JisonCase {
            family: "quadrant",
            diagram_type: "quadrantChart",
            oracle: "quadrant-chart/parser/quadrant.jison",
            source: "quadrantChart\naccDescr {partial",
            accepted: false,
        },
        JisonCase {
            family: "requirement",
            diagram_type: "requirement",
            oracle: "requirement/parser/requirementDiagram.jison",
            source: "requirementDiagram\naccDescr {partial",
            accepted: false,
        },
        JisonCase {
            family: "c4",
            diagram_type: "c4",
            oracle: "c4/parser/c4Diagram.jison",
            source: "C4Context\naccDescr {partial",
            accepted: false,
        },
        JisonCase {
            family: "block",
            diagram_type: "block",
            oracle: "block/parser/block.jison",
            source: "block-beta\naccDescr {partial",
            accepted: false,
        },
    ];

    for case in cases {
        let snapshot = Engine::new()
            .parse_diagram_snapshot_with_type_sync(case.diagram_type, case.source)
            .unwrap_or_else(|error| panic!("{} operation failed: {error}", case.family))
            .unwrap_or_else(|| panic!("{} snapshot missing", case.family));
        let facts = available_facts(&snapshot);

        match snapshot.outcome() {
            DiagramParseOutcome::Parsed { model, .. } if case.accepted => {
                // State's multiline lexer emits a final empty value at EOF, overwriting
                // any partial description. Other accepting families leave it unset.
                let expected_description = if case.family == "state" {
                    json!("")
                } else {
                    Value::Null
                };
                assert_eq!(model["accDescr"], expected_description, "{}", case.family);
                // State recognizes the multiline token even when EOF supplies its empty value.
                assert_eq!(
                    facts
                        .directive_prefixes
                        .iter()
                        .any(|prefix| prefix == "accDescr"),
                    case.family == "state",
                    "{}",
                    case.family
                );
                match case.family {
                    "flowchart" => assert_eq!(model["nodes"], json!([])),
                    "sequence" => assert_eq!(model["actors"], json!({})),
                    "state" => assert_eq!(model["states"], json!({})),
                    "xychart" => assert_eq!(model["plots"], json!([])),
                    _ => unreachable!(),
                }
                assert_eq!(facts.completeness, EditorSemanticCompleteness::Complete);
            }
            DiagramParseOutcome::Failed(_) if !case.accepted => {
                assert_eq!(facts.completeness, EditorSemanticCompleteness::Recovered);
            }
            outcome => panic!(
                "{} diverged from {}: accepted={}, outcome={outcome:?}",
                case.family, case.oracle, case.accepted
            ),
        }
    }
}

#[test]
fn sequence_multiline_accessibility_preserves_an_ordinary_next_line() {
    let source = "sequenceDiagram\naccDescr {desc}\nparticipant A";
    let snapshot = Engine::new()
        .parse_diagram_snapshot_with_type_sync("sequence", source)
        .expect("sequence operation")
        .expect("sequence snapshot");
    let DiagramParseOutcome::Parsed { model, .. } = snapshot.outcome() else {
        panic!("ordinary next-line sequence input must parse");
    };

    assert_eq!(model["accDescr"], json!("desc"));
    assert!(model["actors"].get("A").is_some());
    assert_eq!(
        available_facts(&snapshot).completeness,
        EditorSemanticCompleteness::Complete
    );
}

#[test]
fn sequence_empty_comments_preserve_semantics_render_models_and_editor_facts() {
    let engine = Engine::new();
    for source in [
        "sequenceDiagram\n%%",
        "sequenceDiagram\n%%\nparticipant A\nA->>B: hello\n%%",
        "sequenceDiagram\r\n%%\r\nparticipant A\r\nA->>B: hello\r\n%%\r\n",
        "sequenceDiagram\naccDescr {desc}\n%%\nparticipant A\n",
    ] {
        let plain = source.replace("%%", "");
        let expected = engine
            .parse_diagram_sync(&plain, ParseOptions::strict())
            .expect("comment-free input parses")
            .expect("sequence diagram");
        let parsed = engine
            .parse_diagram_sync(source, ParseOptions::strict())
            .expect("empty comments terminate")
            .expect("sequence diagram");
        assert_eq!(parsed.model, expected.model, "{source:?}");

        let rendered = engine
            .parse_diagram_for_render_model_sync(source, ParseOptions::strict())
            .expect("render-model parsing terminates")
            .expect("sequence render model");
        let expected_render = engine
            .parse_diagram_for_render_model_sync(&plain, ParseOptions::strict())
            .expect("comment-free render model")
            .expect("sequence render model");
        let RenderSemanticModel::Sequence(rendered) = rendered.model() else {
            panic!("expected Sequence render model");
        };
        let RenderSemanticModel::Sequence(expected_render) = expected_render.model() else {
            panic!("expected comment-free Sequence render model");
        };
        assert_eq!(
            serde_json::to_value(rendered).unwrap(),
            serde_json::to_value(expected_render).unwrap(),
            "{source:?}"
        );

        let snapshot = engine
            .parse_diagram_snapshot_with_type_sync("sequence", source)
            .expect("editor parsing terminates")
            .expect("sequence snapshot");
        let DiagramParseOutcome::Parsed { model, .. } = snapshot.outcome() else {
            panic!("empty comments must preserve parsing: {source:?}");
        };
        assert_eq!(model, &expected.model);
        assert_eq!(
            available_facts(&snapshot).completeness,
            EditorSemanticCompleteness::Complete,
            "{source:?}"
        );
    }
}
