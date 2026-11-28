#![cfg(feature = "diagram-event-modeling")]

use merman_core::{EditorSemanticCompleteness, Engine, Error, ParseOptions, SourceSpan};

#[test]
fn duplicate_eventmodeling_frame_ids_fail_all_parse_routes_with_source_spans() {
    let engine = Engine::new();
    for first in ["tf", "rf"] {
        for second in ["tf", "rf"] {
            let source = format!(
                "---\ntitle: Frame IDs\n---\neventmodeling\n{first} 01 ui First\n%% comment preserves the original source line\n{second} 01 evt Second\ntf 02 cmd Later\n"
            );
            let start = source.rfind("01").unwrap();
            let expected = SourceSpan::new(start, start + 2);
            for error in [
                engine
                    .parse_diagram_sync(&source, ParseOptions::strict())
                    .unwrap_err(),
                engine
                    .parse_diagram_for_render_model_sync(&source, ParseOptions::strict())
                    .unwrap_err(),
            ] {
                let Error::DiagramParse { diagnostic, .. } = error else {
                    panic!("expected duplicate-frame parse error");
                };
                assert!(
                    diagnostic
                        .message()
                        .contains("Duplicate event modeling frame ID")
                );
                assert_eq!(diagnostic.span(), Some(expected), "{first}/{second}");
            }
            let facts = engine
                .parse_editor_semantic_facts_with_type_sync("eventmodeling", &source)
                .unwrap()
                .unwrap();
            assert_eq!(facts.completeness, EditorSemanticCompleteness::Recovered);
            assert!(
                facts
                    .diagnostics
                    .iter()
                    .any(|diagnostic| diagnostic.span == Some(expected)
                        && diagnostic
                            .message
                            .contains("Duplicate event modeling frame ID")),
                "{:?}",
                facts.diagnostics
            );
            assert!(
                facts.symbols.iter().any(|symbol| symbol.name == "02"),
                "later valid frame facts must survive recovery"
            );
        }
    }
    let valid = "eventmodeling\ntf 01 ui Fresh\n";
    assert!(
        engine
            .parse_diagram_for_render_model_sync(valid, ParseOptions::strict())
            .unwrap()
            .is_some()
    );
}
