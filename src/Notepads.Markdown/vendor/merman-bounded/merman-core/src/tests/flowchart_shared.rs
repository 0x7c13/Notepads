use crate::*;
use serde_json::json;

#[test]
fn combined_flowchart_variants_construct_one_token_and_accessibility_trace() {
    let cases = [
        #[cfg(feature = "diagram-flowchart")]
        ("flowchart-v2", "flowchart TD"),
        #[cfg(feature = "diagram-flowchart")]
        ("flowchart", "graph TD"),
        #[cfg(feature = "diagram-flowchart")]
        ("flowchart-elk", "flowchart-elk TD"),
        #[cfg(feature = "diagram-swimlane")]
        ("swimlane", "swimlane-beta LR"),
    ];
    let engine = Engine::new();

    for (diagram_type, header) in cases {
        for (tail, should_parse) in [
            ("accTitle: One pass\nA --> B\n", true),
            ("accTitle: One pass\nA((\n", false),
        ] {
            crate::diagrams::flowchart::reset_flowchart_token_trace_construction_count();
            crate::diagrams::flowchart::reset_flowchart_accessibility_scan_count();
            let source = format!("{header}\n{tail}");
            let snapshot = engine
                .parse_diagram_snapshot_with_type_sync(diagram_type, &source)
                .unwrap()
                .expect("built-in Flowchart variant snapshot");

            assert_eq!(
                snapshot.outcome().parsed_model().is_some(),
                should_parse,
                "{diagram_type} strict parser outcome"
            );
            if !should_parse {
                let DiagramParseOutcome::Failed(error) = snapshot.outcome() else {
                    unreachable!("partial recovery token must not satisfy the strict parser");
                };
                assert!(error.to_string().contains("Unterminated node label"));
                let ParsedEditorFacts::Available(facts) = snapshot.editor_facts() else {
                    panic!("failed Flowchart construction must retain editor facts");
                };
                assert_eq!(facts.completeness, EditorSemanticCompleteness::Recovered);
            }
            assert_eq!(
                crate::diagrams::flowchart::flowchart_token_trace_construction_count(),
                1,
                "{diagram_type} token trace"
            );
            assert_eq!(
                crate::diagrams::flowchart::flowchart_accessibility_scan_count(),
                1,
                "{diagram_type} accessibility scan"
            );
        }
    }
}

#[test]
fn warning_producing_flowchart_variants_register_typed_compatibility_sidecars() {
    let engine = Engine::new();
    let cases = [
        #[cfg(feature = "diagram-flowchart")]
        ("flowchart-elk", "flowchart-elk\nA-->B\n", "flowchart-elk"),
        #[cfg(feature = "diagram-flowchart")]
        ("flowchart-v2", "flowchart\nA-->B\n", "flowchart"),
        #[cfg(feature = "diagram-flowchart")]
        ("flowchart", "flowchart\nA-->B\n", "flowchart"),
        #[cfg(feature = "diagram-swimlane")]
        ("swimlane", "swimlane-beta\nA-->B\n", "swimlane-beta"),
    ];

    for (diagram_type, source, keyword) in cases {
        assert!(
            crate::family::warning_semantic_parser(diagram_type).is_some(),
            "{diagram_type} must register its typed warning compatibility parser"
        );

        let snapshot = engine
            .parse_diagram_snapshot_with_type_sync(diagram_type, source)
            .unwrap()
            .unwrap();
        let DiagramParseOutcome::Parsed {
            model,
            warning_facts,
        } = snapshot.outcome()
        else {
            panic!("{diagram_type} warning fixture must parse");
        };
        let public = engine
            .parse_diagram_with_type_sync(diagram_type, source, ParseOptions::strict())
            .unwrap()
            .unwrap();

        assert_eq!(warning_facts.len(), 1, "{diagram_type}");
        assert_eq!(
            warning_facts[0].span,
            Some(SourceSpan::new(0, keyword.len())),
            "{diagram_type}"
        );
        assert_eq!(
            model["warningFacts"],
            json!(warning_facts),
            "{diagram_type}"
        );
        assert_eq!(
            public.model["warningFacts"],
            json!(warning_facts),
            "{diagram_type}"
        );
    }
}
