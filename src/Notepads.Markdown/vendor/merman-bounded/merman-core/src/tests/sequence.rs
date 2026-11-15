use crate::*;
use futures::executor::block_on;
use serde_json::json;

#[test]
fn sequence_fragment_carriers_preserve_nested_control_and_activation_order() {
    let source = concat!(
        "sequenceDiagram\n",
        "loop L\nalt A\nopt O\n",
        "Alice->>+Bob: start\nBob-->>-Alice: done\nend\n",
        "else E\npar P\nAlice->>Bob: left\n",
        "and Q\ncritical C\nAlice->>Bob: protected\n",
        "option R\nbreak B\nAlice->>Bob: stop\n",
        "end\nend\nend\nend\nend\n",
        "Alice->>Bob: after\n",
    );
    let parsed = Engine::new()
        .parse_diagram_sync(source, ParseOptions::strict())
        .unwrap()
        .unwrap();
    let messages = parsed.model["messages"].as_array().unwrap();
    let types: Vec<_> = messages
        .iter()
        .map(|message| message["type"].as_i64().unwrap())
        .collect();
    assert_eq!(
        types,
        [
            10, 12, 15, 0, 17, 1, 18, 16, 13, 19, 0, 20, 27, 0, 28, 30, 0, 31, 29, 21, 14, 11, 0
        ]
    );
    let signal_text: Vec<_> = messages
        .iter()
        .filter(|message| matches!(message["type"].as_i64(), Some(0 | 1)))
        .map(|message| message["message"].as_str().unwrap())
        .collect();
    assert_eq!(
        signal_text,
        ["start", "done", "left", "protected", "stop", "after"]
    );
    assert_eq!(parsed.model["actorOrder"], json!(["Alice", "Bob"]));
}

#[test]
fn sequence_fragment_carrier_failures_preserve_eof_and_first_error_recovery() {
    let engine = Engine::new();
    let incomplete = "sequenceDiagram\nloop Outer\nopt Inner\nA->>B: message\nend\n";
    let unexpected = "sequenceDiagram\nend\nloop Later\nA->>B: message\n";
    for (source, span, kind) in [
        (
            incomplete,
            SourceSpan::new(incomplete.len(), incomplete.len()),
            ParseDiagnosticSpanKind::InsertionPoint,
        ),
        (
            unexpected,
            SourceSpan::new("sequenceDiagram\n".len(), "sequenceDiagram\nend".len()),
            ParseDiagnosticSpanKind::Exact,
        ),
    ] {
        let error = engine
            .parse_diagram_sync(source, ParseOptions::strict())
            .expect_err("malformed Sequence fragment is rejected");
        let Error::DiagramParse { diagnostic, .. } = error else {
            panic!("Sequence grammar failure must remain a parse error");
        };
        assert_eq!(diagnostic.span(), Some(span));
        assert_eq!(diagnostic.span_kind(), kind);
        let facts = engine
            .parse_editor_semantic_facts_with_type_sync("sequence", source)
            .unwrap()
            .unwrap();
        assert_eq!(facts.completeness, EditorSemanticCompleteness::Recovered);
        assert!(facts.symbols.iter().any(|symbol| symbol.name == "B"));
        assert!(facts.diagnostics.iter().any(|diagnostic| {
            diagnostic.kind == EditorSemanticDiagnosticKind::ParserRecovery
                && diagnostic.span == Some(span)
        }));
    }
}

#[test]
fn parse_diagram_sequence_basic_messages_and_notes() {
    let engine = Engine::new();
    let text = r#"sequenceDiagram
Alice->Bob:Hello Bob, how are you?
Note right of Bob: Bob thinks
Bob-->Alice: I am good thanks!"#;

    let res = block_on(engine.parse_diagram(text, ParseOptions::default()))
        .unwrap()
        .unwrap();
    assert_eq!(res.meta.diagram_type, "sequence");

    let msgs = res.model["messages"].as_array().unwrap();
    assert_eq!(msgs.len(), 3);
    assert_eq!(msgs[0]["from"], json!("Alice"));
    assert_eq!(msgs[0]["to"], json!("Bob"));
    assert_eq!(msgs[0]["message"], json!("Hello Bob, how are you?"));
    assert_eq!(msgs[0]["type"], json!(5));
    assert_eq!(msgs[0]["wrap"], json!(false));

    assert_eq!(msgs[1]["type"], json!(2));
    assert_eq!(msgs[1]["placement"], json!(1));
    assert_eq!(msgs[1]["from"], json!("Bob"));
    assert_eq!(msgs[1]["to"], json!("Bob"));
    assert_eq!(msgs[1]["message"], json!("Bob thinks"));

    assert_eq!(msgs[2]["from"], json!("Bob"));
    assert_eq!(msgs[2]["to"], json!("Alice"));
    assert_eq!(msgs[2]["message"], json!("I am good thanks!"));
    assert_eq!(msgs[2]["type"], json!(6));
}

#[test]
fn parse_sequence_editor_facts_preserve_actor_and_box_spans() {
    let engine = Engine::new();
    let text = r#"sequenceDiagram
title: Diagram Title
accTitle: Accessible Title
accDescr: Accessible Description
participant Alice
actor Bob
box rgb(240,240,240) Team
participant Carol
end
Alice->>Bob: Hello
Note over Alice,Bob: Review
details Alice: {"owner": "platform"}"#;
    let facts = engine
        .parse_editor_semantic_facts_with_type_sync("sequence", text)
        .unwrap()
        .expect("sequence editor facts");

    assert_eq!(facts.completeness, EditorSemanticCompleteness::Complete);

    let first_symbol = |name: &str| {
        facts
            .symbols
            .iter()
            .find(|symbol| symbol.name == name)
            .unwrap_or_else(|| panic!("missing symbol {name}"))
    };

    let alice_start = text.find("Alice").unwrap();
    assert_eq!(first_symbol("Alice").selection.start, alice_start);
    assert_eq!(
        first_symbol("Alice").selection.end,
        alice_start + "Alice".len()
    );

    let team_start = text.find("Team").unwrap();
    let team = first_symbol("Team");
    assert_eq!(team.detail.as_deref(), Some("sequence box"));
    assert_eq!(team.role, EditorSemanticRole::Payload);
    assert_eq!(team.kind, EditorSemanticKind::String);
    assert_eq!(team.selection.start, team_start);
    assert_eq!(team.selection.end, team_start + "Team".len());

    assert!(facts.symbols.iter().any(|symbol| {
        symbol.name == "Bob" && symbol.detail.as_deref() == Some("sequence actor")
    }));
    assert!(facts.symbols.iter().any(|symbol| {
        symbol.name == "Bob" && symbol.detail.as_deref() == Some("sequence participant reference")
    }));

    let payload_symbol = |name: &str, detail: &str| {
        facts
            .symbols
            .iter()
            .find(|symbol| symbol.name == name && symbol.detail.as_deref() == Some(detail))
            .unwrap_or_else(|| panic!("missing payload {name:?} / {detail:?}"))
    };

    for (name, detail) in [
        ("Diagram Title", "sequence title"),
        ("Accessible Title", "sequence accessibility title"),
        (
            "Accessible Description",
            "sequence accessibility description",
        ),
        ("Hello", "sequence message"),
        ("Review", "sequence note"),
        (r#"{"owner": "platform"}"#, "sequence interaction payload"),
    ] {
        let symbol = payload_symbol(name, detail);
        let start = text.find(name).unwrap();
        assert_eq!(symbol.role, EditorSemanticRole::Payload);
        assert_eq!(symbol.kind, EditorSemanticKind::String);
        assert_eq!(symbol.selection.start, start);
        assert_eq!(symbol.selection.end, start + name.len());
    }

    for prefix in ["title", "accTitle", "accDescr", "details"] {
        assert!(facts.directive_prefixes.iter().any(|p| p == prefix));
    }

    for payload in ["Hello", "Review", r#"{"owner": "platform"}"#] {
        let start = text.find(payload).unwrap();
        assert!(
            facts.expected_syntax.iter().any(|expected| {
                expected.kind == EditorExpectedSyntaxKind::Payload
                    && expected.span.start <= start
                    && expected.span.end >= start + payload.len()
            }),
            "missing payload expected syntax for {payload:?}"
        );
    }
}

#[test]
fn parse_sequence_editor_box_payload_reuses_db_color_semantics() {
    let engine = Engine::new();
    let text = concat!(
        "sequenceDiagram\n",
        "box aqua\n",
        "participant Alice\n",
        "end\n",
        "box rebeccapurple Platform\n",
        "participant Bob\n",
        "end\n",
    );
    let facts = engine
        .parse_editor_semantic_facts_with_type_sync("sequence", text)
        .unwrap()
        .expect("sequence editor facts");

    assert!(!facts.symbols.iter().any(|symbol| {
        symbol.name == "aqua" && symbol.detail.as_deref() == Some("sequence box")
    }));
    let platform = facts
        .symbols
        .iter()
        .find(|symbol| {
            symbol.name == "Platform" && symbol.detail.as_deref() == Some("sequence box")
        })
        .expect("labeled box payload");
    let start = text.find("Platform").unwrap();
    assert_eq!(platform.role, EditorSemanticRole::Payload);
    assert_eq!(
        platform.selection,
        SourceSpan::new(start, start + "Platform".len())
    );
}

#[test]
fn parse_sequence_editor_payload_spans_skip_directive_prefix_text() {
    let engine = Engine::new();
    let text = r#"sequenceDiagram
title: title
accTitle: Title
accDescr: accDescr
Alice->>Bob: Alice"#;
    let facts = engine
        .parse_editor_semantic_facts_with_type_sync("sequence", text)
        .unwrap()
        .expect("sequence editor facts");

    assert_eq!(facts.completeness, EditorSemanticCompleteness::Complete);

    for (name, detail) in [
        ("title", "sequence title"),
        ("Title", "sequence accessibility title"),
        ("accDescr", "sequence accessibility description"),
        ("Alice", "sequence message"),
    ] {
        let symbol = facts
            .symbols
            .iter()
            .find(|symbol| symbol.name == name && symbol.detail.as_deref() == Some(detail))
            .unwrap_or_else(|| panic!("missing payload {name:?} / {detail:?}"));
        let start = text.rfind(name).unwrap();
        assert_eq!(symbol.role, EditorSemanticRole::Payload);
        assert_eq!(symbol.kind, EditorSemanticKind::String);
        assert_eq!(symbol.selection.start, start);
        assert_eq!(symbol.selection.end, start + name.len());
    }
}

#[test]
fn parse_sequence_editor_facts_frontmatter_spans_use_original_source() {
    let engine = Engine::new();
    let text = r#"---
config:
  theme: dark
---
sequenceDiagram
participant Alice
Alice->>Bob: Hello"#;
    let facts = engine
        .parse_editor_semantic_facts_with_type_sync("sequence", text)
        .unwrap()
        .expect("sequence editor facts");

    assert_eq!(facts.completeness, EditorSemanticCompleteness::Complete);

    let alice = facts
        .symbols
        .iter()
        .find(|symbol| {
            symbol.name == "Alice" && symbol.detail.as_deref() == Some("sequence participant")
        })
        .expect("Alice participant symbol");
    let alice_start = text.find("Alice").unwrap();
    assert_eq!(alice.selection.start, alice_start);
    assert_eq!(alice.selection.end, alice_start + "Alice".len());
}

#[test]
fn parse_sequence_editor_facts_crlf_frontmatter_spans_use_original_source() {
    let engine = Engine::new();
    let text = concat!(
        "---\r\n",
        "config:\r\n",
        "  theme: dark\r\n",
        "---\r\n",
        "sequenceDiagram\r\n",
        "participant Alice\r\n",
        "Alice->>Bob: Hello",
    );
    let facts = engine
        .parse_editor_semantic_facts_with_type_sync("sequence", text)
        .unwrap()
        .expect("sequence editor facts");

    assert_eq!(facts.completeness, EditorSemanticCompleteness::Complete);

    let alice = facts
        .symbols
        .iter()
        .find(|symbol| {
            symbol.name == "Alice" && symbol.detail.as_deref() == Some("sequence participant")
        })
        .expect("Alice participant symbol");
    let alice_start = text.find("Alice").unwrap();
    assert_eq!(alice.selection.start, alice_start);
    assert_eq!(alice.selection.end, alice_start + "Alice".len());
}

#[test]
fn parse_sequence_editor_facts_init_directive_spans_use_original_source() {
    let engine = Engine::new();
    let text = r#"%%{init: {"theme": "dark"}}%%
sequenceDiagram
participant Alice
Alice->>Bob: Hello"#;
    let facts = engine
        .parse_editor_semantic_facts_with_type_sync("sequence", text)
        .unwrap()
        .expect("sequence editor facts");

    assert_eq!(facts.completeness, EditorSemanticCompleteness::Complete);

    let bob = facts
        .symbols
        .iter()
        .find(|symbol| symbol.name == "Bob" && symbol.role == EditorSemanticRole::Entity)
        .expect("implicit Bob participant definition");
    let bob_start = text.find("Bob").unwrap();
    assert_eq!(bob.selection.start, bob_start);
    assert_eq!(bob.selection.end, bob_start + "Bob".len());
}

#[test]
fn parse_sequence_editor_facts_crlf_init_directive_spans_use_original_source() {
    let engine = Engine::new();
    let text = concat!(
        "%%{init: {\"theme\": \"dark\"}}%%\r\n",
        "sequenceDiagram\r\n",
        "participant Alice\r\n",
        "Alice->>Bob: Hello",
    );
    let facts = engine
        .parse_editor_semantic_facts_with_type_sync("sequence", text)
        .unwrap()
        .expect("sequence editor facts");

    assert_eq!(facts.completeness, EditorSemanticCompleteness::Complete);

    let bob = facts
        .symbols
        .iter()
        .find(|symbol| symbol.name == "Bob" && symbol.role == EditorSemanticRole::Entity)
        .expect("implicit Bob participant definition");
    let bob_start = text.find("Bob").unwrap();
    assert_eq!(bob.selection.start, bob_start);
    assert_eq!(bob.selection.end, bob_start + "Bob".len());
}

#[test]
fn parse_sequence_editor_facts_crlf_frontmatter_init_unicode_spans_use_original_source() {
    let engine = Engine::new();
    let text = concat!(
        "---\r\n",
        "config:\r\n",
        "  theme: dark\r\n",
        "---\r\n",
        "%%{init: {\"theme\": \"default\"}}%%\r\n",
        "sequenceDiagram\r\n",
        "participant 顧客\r\n",
        "顧客->>サーバー: こんにちは",
    );
    let facts = engine
        .parse_editor_semantic_facts_with_type_sync("sequence", text)
        .unwrap()
        .expect("sequence editor facts");

    assert_eq!(facts.completeness, EditorSemanticCompleteness::Complete);

    let customer = facts
        .symbols
        .iter()
        .find(|symbol| {
            symbol.name == "顧客" && symbol.detail.as_deref() == Some("sequence participant")
        })
        .expect("Unicode participant symbol");
    let customer_start = text.find("顧客").unwrap();
    assert_eq!(customer.selection.start, customer_start);
    assert_eq!(customer.selection.end, customer_start + "顧客".len());
}

#[test]
fn parse_sequence_editor_facts_preserve_every_repeated_unicode_occurrence() {
    let engine = Engine::new();
    let text = concat!(
        "---\r\n",
        "config:\r\n",
        "  theme: dark\r\n",
        "---\r\n",
        "%%{init: {\"theme\": \"default\"}}%%\r\n",
        "sequenceDiagram\r\n",
        "participant 顧客\r\n",
        "顧客->>サーバー: こんにちは\r\n",
        "Note over 顧客,サーバー: 確認\r\n",
        "サーバー-->>顧客: 完了\r\n",
    );
    let facts = engine
        .parse_editor_semantic_facts_with_type_sync("sequence", text)
        .unwrap()
        .expect("sequence editor facts");

    assert_eq!(facts.completeness, EditorSemanticCompleteness::Complete);
    for name in ["顧客", "サーバー"] {
        let expected = text
            .match_indices(name)
            .map(|(start, _)| SourceSpan::new(start, start + name.len()))
            .collect::<Vec<_>>();
        let occurrences = facts
            .symbols
            .iter()
            .filter(|symbol| {
                symbol.name == name
                    && matches!(
                        symbol.role,
                        EditorSemanticRole::Entity | EditorSemanticRole::Reference
                    )
            })
            .collect::<Vec<_>>();
        let actual = occurrences
            .iter()
            .map(|symbol| symbol.selection)
            .collect::<Vec<_>>();
        assert_eq!(actual, expected, "lost or reordered {name} occurrences");
        assert_eq!(
            occurrences.first().map(|symbol| symbol.role),
            Some(EditorSemanticRole::Entity),
            "the first {name} occurrence must own the participant definition"
        );
        assert!(
            occurrences
                .iter()
                .skip(1)
                .all(|symbol| symbol.role == EditorSemanticRole::Reference),
            "later {name} occurrences must remain navigation-only references"
        );
    }
}

#[test]
fn parse_sequence_editor_facts_remap_entity_normalization_without_losing_other_facts() {
    let engine = Engine::new();
    let text = concat!(
        "---\n",
        "title: quoted\n",
        "---\n",
        "sequenceDiagram\n",
        "participant Alice\n",
        "Alice->>Bob: #quot;\n",
    );

    let facts = engine
        .parse_editor_semantic_facts_with_type_sync("sequence", text)
        .unwrap()
        .expect("sequence editor facts");

    assert_eq!(facts.completeness, EditorSemanticCompleteness::Complete);
    assert!(
        facts
            .symbols
            .iter()
            .any(|symbol| symbol.name == "Alice" && symbol.role == EditorSemanticRole::Entity)
    );
    let alice = facts
        .symbols
        .iter()
        .find(|symbol| symbol.name == "Alice" && symbol.role == EditorSemanticRole::Entity)
        .expect("Alice participant");
    assert_eq!(alice.selection.start, text.find("Alice").unwrap());
    assert!(
        facts
            .symbols
            .iter()
            .any(|symbol| symbol.name == "Bob" && symbol.role == EditorSemanticRole::Entity)
    );
    assert!(
        facts.diagnostics.is_empty(),
        "editor facts must parse the preprocessed Mermaid body, not the original frontmatter-bearing source"
    );
}

#[test]
fn parse_sequence_editor_facts_recovers_from_incomplete_input() {
    let engine = Engine::new();
    let text = "sequenceDiagram\nAlice->>Bob: Hello\nBob->>";
    let facts = engine
        .parse_editor_semantic_facts_with_type_sync("sequence", text)
        .unwrap()
        .expect("sequence editor facts");

    assert_eq!(facts.completeness, EditorSemanticCompleteness::Recovered);
    assert!(facts.symbols.iter().any(|symbol| symbol.name == "Alice"));
    assert!(facts.symbols.iter().any(|symbol| symbol.name == "Bob"));
}

#[test]
fn parse_sequence_editor_facts_stop_after_non_advancing_lexer_error() {
    let engine = Engine::new();
    // A spaced config is valid in 12.1; whitespace inside the actor id remains invalid.
    let text =
        "sequenceDiagram\nparticipant Alice\nparticipant Bob Worker @{\nAlice->>Bob: Hello\n";
    crate::diagrams::sequence::reset_sequence_syntax_construction_count();
    let facts = engine
        .parse_editor_semantic_facts_with_type_sync("sequence", text)
        .unwrap()
        .expect("sequence editor facts");

    assert_eq!(facts.completeness, EditorSemanticCompleteness::Recovered);
    assert_eq!(
        crate::diagrams::sequence::sequence_syntax_construction_count(),
        1,
        "a non-advancing lexer error must terminate the one shared token tape"
    );
    assert!(facts.symbols.iter().any(|symbol| symbol.name == "Alice"));
    assert!(
        facts
            .symbols
            .iter()
            .any(|symbol| symbol.name == "Bob Worker")
    );
    let invalid_start = text.find("@{").unwrap();
    assert!(facts.diagnostics.iter().any(|diagnostic| {
        diagnostic.kind == EditorSemanticDiagnosticKind::ParserRecovery
            && diagnostic.span == Some(SourceSpan::new(invalid_start, invalid_start + 2))
    }));
}

#[test]
fn parse_sequence_editor_facts_continue_after_advancing_lexer_error() {
    let engine = Engine::new();
    let text = "sequenceDiagram\nparticipant Alice\n<\nparticipant Bob\nAlice->>Bob: Hello\n";
    crate::diagrams::sequence::reset_sequence_syntax_construction_count();
    let facts = engine
        .parse_editor_semantic_facts_with_type_sync("sequence", text)
        .unwrap()
        .expect("sequence editor facts");

    assert_eq!(facts.completeness, EditorSemanticCompleteness::Recovered);
    assert_eq!(
        crate::diagrams::sequence::sequence_syntax_construction_count(),
        1,
        "an advancing lexer error must not rebuild the token tape"
    );
    assert!(facts.symbols.iter().any(|symbol| symbol.name == "Alice"));
    assert!(facts.symbols.iter().any(|symbol| symbol.name == "Bob"));
    let invalid_start = text.find('<').unwrap();
    assert!(facts.diagnostics.iter().any(|diagnostic| {
        diagnostic.kind == EditorSemanticDiagnosticKind::ParserRecovery
            && diagnostic.span == Some(SourceSpan::new(invalid_start, invalid_start + 1))
    }));
}

#[test]
fn parse_sequence_strict_failure_and_editor_recovery_share_exact_lexer_span() {
    let engine = Engine::new();
    let text = concat!(
        "---\r\n",
        "config:\r\n",
        "  theme: dark\r\n",
        "---\r\n",
        "%%{init: {\"theme\": \"default\"}}%%\r\n",
        "sequenceDiagram\r\n",
        "顧客->>サーバー: こんにちは\r\n",
        "<\r\n",
        "participant 後続\r\n",
    );
    let invalid_start = text.find('<').unwrap();

    let facts = engine
        .parse_editor_semantic_facts_with_type_sync("sequence", text)
        .unwrap()
        .expect("sequence recovery facts");
    assert_eq!(facts.completeness, EditorSemanticCompleteness::Recovered);
    assert!(facts.diagnostics.iter().any(|diagnostic| {
        diagnostic.kind == EditorSemanticDiagnosticKind::ParserRecovery
            && diagnostic.span == Some(SourceSpan::new(invalid_start, invalid_start + 1))
    }));
    assert!(facts.symbols.iter().any(|symbol| symbol.name == "後続"));

    let error = engine
        .parse_diagram_sync(text, ParseOptions::strict())
        .expect_err("strict Sequence parsing rejects the invalid token");
    let Error::DiagramParse { diagnostic, .. } = error else {
        panic!("invalid Sequence token returned a non-parse error");
    };
    assert_eq!(
        diagnostic.span(),
        Some(SourceSpan::new(invalid_start, invalid_start + 1))
    );
    assert_eq!(diagnostic.span_kind(), ParseDiagnosticSpanKind::Exact);
}

#[test]
fn parse_sequence_minimal_signals_preserve_canonical_db_order_without_a_second_grammar() {
    let engine = Engine::new();
    let text = concat!(
        "sequenceDiagram\n",
        "Alice->>+Bob: Start\n",
        "Bob-->>-Alice: Done\n",
    );

    let parsed = engine
        .parse_diagram_sync(text, ParseOptions::strict())
        .unwrap()
        .expect("minimal Sequence parse");
    let messages = parsed.model["messages"].as_array().unwrap();
    assert_eq!(parsed.model["actorOrder"], json!(["Alice", "Bob"]));
    assert_eq!(messages.len(), 4);
    assert_eq!(messages[0]["message"], "Start");
    assert_eq!(messages[1]["type"], 17);
    assert_eq!(messages[2]["message"], "Done");
    assert_eq!(messages[3]["type"], 18);
}

#[test]
fn parse_diagram_sequence_multibyte_actor_ids_do_not_panic() {
    let engine = Engine::new();
    let text = r#"sequenceDiagram
顧客->>サーバー:こんにちは
"#;

    let res = block_on(engine.parse_diagram(text, ParseOptions::default()))
        .unwrap()
        .unwrap();
    assert_eq!(res.model["actorOrder"], json!(["顧客", "サーバー"]));
    assert_eq!(res.model["messages"][0]["message"], json!("こんにちは"));
}

#[test]
fn parse_diagram_sequence_central_connections_use_upstream_message_model() {
    let engine = Engine::new();
    let text = r#"sequenceDiagram
participant Alice
participant John
Alice->>()John: Hello John
Alice()->>John: How are you?
John()->>()Alice: Great!"#;

    let res = block_on(engine.parse_diagram(text, ParseOptions::default()))
        .unwrap()
        .unwrap();

    assert_eq!(res.model["actorOrder"], json!(["Alice", "John"]));
    let actors = res.model["actors"].as_object().unwrap();
    assert!(actors.get("Alice").is_some());
    assert!(actors.get("John").is_some());
    assert!(actors.get("Alice()").is_none());
    assert!(actors.get("()John").is_none());

    let msgs = res.model["messages"].as_array().unwrap();
    assert_eq!(msgs.len(), 7);
    assert_eq!(msgs[0]["id"], json!("0"));
    assert_eq!(msgs[0]["from"], json!("Alice"));
    assert_eq!(msgs[0]["to"], json!("John"));
    assert_eq!(msgs[0]["centralConnection"], json!(59));
    assert_eq!(msgs[0]["activate"], json!(true));
    assert_eq!(msgs[1]["type"], json!(59));
    assert_eq!(msgs[2]["id"], json!("2"));
    assert_eq!(msgs[2]["from"], json!("Alice"));
    assert_eq!(msgs[2]["to"], json!("John"));
    assert_eq!(msgs[2]["centralConnection"], json!(60));
    assert_eq!(msgs[3]["type"], json!(60));
    assert_eq!(msgs[4]["id"], json!("4"));
    assert_eq!(msgs[4]["from"], json!("John"));
    assert_eq!(msgs[4]["to"], json!("Alice"));
    assert_eq!(msgs[4]["centralConnection"], json!(61));
    assert_eq!(msgs[4]["activate"], json!(true));
    assert_eq!(msgs[5]["type"], json!(59));
    assert_eq!(msgs[6]["type"], json!(60));
}

#[test]
fn parse_diagram_sequence_autonumber_allows_decimal_start_and_step() {
    let engine = Engine::new();
    let text = r#"sequenceDiagram
autonumber 10.1 .01
Alice->>Bob:Hello
Bob-->>Alice:Back"#;

    let res = block_on(engine.parse_diagram(text, ParseOptions::default()))
        .unwrap()
        .unwrap();

    let msgs = res.model["messages"].as_array().unwrap();
    assert_eq!(msgs[0]["type"], json!(26));
    assert_eq!(msgs[0]["message"]["start"].as_f64(), Some(10.1));
    assert_eq!(msgs[0]["message"]["step"].as_f64(), Some(0.01));
    assert_eq!(msgs[0]["message"]["visible"], json!(true));
    assert_eq!(msgs[1]["message"], json!("Hello"));
    assert_eq!(msgs[2]["message"], json!("Back"));
}

#[test]
fn parse_diagram_sequence_autonumber_rejects_thousandths() {
    let engine = Engine::new();
    let text = r#"sequenceDiagram
autonumber 10.001
Alice->>Bob:Hello"#;

    let res = block_on(engine.parse_diagram(text, ParseOptions::default()));
    assert!(
        res.is_err(),
        "expected Mermaid 11.15-compatible parse failure for thousandths"
    );
}

#[test]
fn parse_diagram_sequence_is_stateless_across_multiple_parses() {
    let engine = Engine::new();
    let first = r#"sequenceDiagram
Alice->Bob:Hello Bob, how are you?
Bob-->Alice:I am good thanks!"#;
    let second = r#"sequenceDiagram
Alice->John:Hello John, how are you?
John-->Alice:I am good thanks!"#;

    let a = block_on(engine.parse_diagram(first, ParseOptions::default()))
        .unwrap()
        .unwrap();
    let b = block_on(engine.parse_diagram(second, ParseOptions::default()))
        .unwrap()
        .unwrap();

    let a_msgs = a.model["messages"].as_array().unwrap();
    let b_msgs = b.model["messages"].as_array().unwrap();

    assert_eq!(a_msgs.len(), 2);
    assert_eq!(a_msgs[0]["id"], json!("0"));
    assert_eq!(a_msgs[1]["id"], json!("1"));
    assert_eq!(a_msgs[0]["from"], json!("Alice"));
    assert_eq!(a_msgs[0]["to"], json!("Bob"));
    assert_eq!(a_msgs[1]["from"], json!("Bob"));
    assert_eq!(a_msgs[1]["to"], json!("Alice"));

    assert_eq!(b_msgs.len(), 2);
    assert_eq!(b_msgs[0]["id"], json!("0"));
    assert_eq!(b_msgs[1]["id"], json!("1"));
    assert_eq!(b_msgs[0]["from"], json!("Alice"));
    assert_eq!(b_msgs[0]["to"], json!("John"));
    assert_eq!(b_msgs[1]["from"], json!("John"));
    assert_eq!(b_msgs[1]["to"], json!("Alice"));
}

#[test]
fn parse_diagram_sequence_title_and_accessibility_fields() {
    let engine = Engine::new();
    let text = r#"sequenceDiagram
title: Diagram Title
accTitle: Accessible Title
accDescr: Accessible Description
Alice->Bob:Hello"#;

    let res = block_on(engine.parse_diagram(text, ParseOptions::default()))
        .unwrap()
        .unwrap();

    assert_eq!(res.model["title"], json!("Diagram Title"));
    assert_eq!(res.model["accTitle"], json!("Accessible Title"));
    assert_eq!(res.model["accDescr"], json!("Accessible Description"));
}

#[test]
fn parse_diagram_sequence_wrap_directive_controls_default_wrap() {
    let engine = Engine::new();
    let text = r#"%%{wrap}%%
sequenceDiagram
Alice->Bob:Hello
Alice->Bob:nowrap:Hello again"#;

    let res = block_on(engine.parse_diagram(text, ParseOptions::default()))
        .unwrap()
        .unwrap();
    let msgs = res.model["messages"].as_array().unwrap();

    assert_eq!(msgs[0]["wrap"], json!(true));
    assert_eq!(msgs[1]["wrap"], json!(false));
    assert_eq!(msgs[1]["message"], json!("Hello again"));
}

#[test]
fn parse_diagram_sequence_links() {
    let engine = Engine::new();
    let text = r#"sequenceDiagram
participant a as Alice
participant b as Bob
participant c as Charlie
links a: { "Repo": "https://repo.contoso.com/", "Dashboard": "https://dashboard.contoso.com/" }
links b: { "Dashboard": "https://dashboard.contoso.com/" }
links a: { "On-Call": "https://oncall.contoso.com/?svc=alice" }
link a: Endpoint @ https://alice.contoso.com
link a: Swagger @ https://swagger.contoso.com
link a: Tests @ https://tests.contoso.com/?svc=alice@contoso.com
"#;

    let res = block_on(engine.parse_diagram(text, ParseOptions::default()))
        .unwrap()
        .unwrap();
    let actors = res.model["actors"].as_object().unwrap();
    assert_eq!(
        actors["a"]["links"]["Repo"],
        json!("https://repo.contoso.com/")
    );
    assert_eq!(actors["b"]["links"].get("Repo"), None);
    assert_eq!(
        actors["a"]["links"]["Dashboard"],
        json!("https://dashboard.contoso.com/")
    );
    assert_eq!(
        actors["b"]["links"]["Dashboard"],
        json!("https://dashboard.contoso.com/")
    );
    assert_eq!(
        actors["a"]["links"]["On-Call"],
        json!("https://oncall.contoso.com/?svc=alice")
    );
    assert_eq!(actors["c"]["links"].get("Dashboard"), None);
    assert_eq!(
        actors["a"]["links"]["Endpoint"],
        json!("https://alice.contoso.com")
    );
    assert_eq!(
        actors["a"]["links"]["Swagger"],
        json!("https://swagger.contoso.com")
    );
    assert_eq!(
        actors["a"]["links"]["Tests"],
        json!("https://tests.contoso.com/?svc=alice@contoso.com")
    );
}

#[test]
fn parse_diagram_sequence_limits_keyword_like_actor_ids_to_id_states() {
    let engine = Engine::new();
    let text = r#"sequenceDiagram
participant AS as AppService
participant DB as Store
participant END as End Service
participant loop as Loop Service
participant RECT as Rectangle Worker
AS->>DB: get recorded file timestamps"#;

    let res = block_on(engine.parse_diagram(text, ParseOptions::default()))
        .unwrap()
        .unwrap();

    let actors = res.model["actors"].as_object().unwrap();
    assert_eq!(actors["AS"]["description"], json!("AppService"));
    assert_eq!(actors["DB"]["description"], json!("Store"));
    assert_eq!(actors["END"]["description"], json!("End Service"));
    assert_eq!(actors["loop"]["description"], json!("Loop Service"));
    assert_eq!(actors["RECT"]["description"], json!("Rectangle Worker"));

    let msgs = res.model["messages"].as_array().unwrap();
    assert_eq!(msgs[0]["from"], json!("AS"));
    assert_eq!(msgs[0]["to"], json!("DB"));
    assert_eq!(msgs.len(), 1);

    for statement in [
        "loop->>B: reserved source",
        "A->>loop: reserved target",
        "END->>B: case-insensitive reserved source",
        "A->>RECT: case-insensitive reserved target",
        "note-taker->>B: keyword-prefixed source",
        "A->>off: reserved target",
    ] {
        let text = format!("sequenceDiagram\n{statement}");
        assert!(
            block_on(engine.parse_diagram(&text, ParseOptions::default())).is_err(),
            "pinned INITIAL keywords must precede implicit ACTOR scanning for {statement:?}"
        );
    }
}

#[test]
fn parse_diagram_sequence_id_states_precede_title_line_lexing() {
    let engine = Engine::new();
    let text = r#"sequenceDiagram
participant title worker
activate title worker
deactivate title worker
destroy title worker"#;

    let res = block_on(engine.parse_diagram(text, ParseOptions::default()))
        .unwrap()
        .unwrap();
    assert!(res.model["actors"].get("title worker").is_some());
    assert!(res.model["destroyedActors"].get("title worker").is_some());
    let messages = res.model["messages"].as_array().unwrap();
    assert!(messages.iter().any(|message| {
        message["type"] == json!(17) && message["from"] == json!("title worker")
    }));
    assert!(messages.iter().any(|message| {
        message["type"] == json!(18) && message["from"] == json!("title worker")
    }));
}

#[test]
fn parse_diagram_sequence_preserves_mermaid_valid_spaced_actor_ids() {
    let engine = Engine::new();
    let text = r#"sequenceDiagram
participant cron job
participant data svc
participant data=svc as Data Service
participant as worker
participant 客户 服务
participant 数据 库
cron job->>data svc: run
data svc-->>cron job: done
cron job->>customer-notifier: notify
customer-notifier-->>data=svc: stored
as worker->>data svc: reserved prefix
客户 服务->>数据 库: 查询"#;

    let res = block_on(engine.parse_diagram(text, ParseOptions::default()))
        .unwrap()
        .unwrap();

    let actors = res.model["actors"].as_object().unwrap();
    for actor in [
        "cron job",
        "data svc",
        "as worker",
        "customer-notifier",
        "客户 服务",
        "数据 库",
    ] {
        assert!(actors.contains_key(actor), "missing actor {actor:?}");
    }
    assert_eq!(actors["cron job"]["description"], json!("cron job"));
    assert_eq!(actors["data svc"]["description"], json!("data svc"));
    assert_eq!(actors["data=svc"]["description"], json!("Data Service"));

    let messages = res.model["messages"].as_array().unwrap();
    let endpoints = messages
        .iter()
        .map(|message| {
            (
                message["from"].as_str().unwrap(),
                message["to"].as_str().unwrap(),
                message["message"].as_str().unwrap(),
            )
        })
        .collect::<Vec<_>>();
    assert_eq!(
        endpoints,
        vec![
            ("cron job", "data svc", "run"),
            ("data svc", "cron job", "done"),
            ("cron job", "customer-notifier", "notify"),
            ("customer-notifier", "data=svc", "stored"),
            ("as worker", "data svc", "reserved prefix"),
            ("客户 服务", "数据 库", "查询"),
        ]
    );

    let facts = engine
        .parse_editor_semantic_facts_with_type_sync("sequence", text)
        .unwrap()
        .expect("sequence editor facts");
    for actor in ["cron job", "data svc", "客户 服务", "数据 库"] {
        let symbol = facts
            .symbols
            .iter()
            .find(|symbol| symbol.name == actor)
            .unwrap_or_else(|| panic!("missing editor symbol {actor:?}"));
        assert_eq!(&text[symbol.selection.start..symbol.selection.end], actor);
    }
}

#[test]
fn parse_diagram_sequence_keeps_pinned_spaced_alias_boundary() {
    let engine = Engine::new();
    let text = "sequenceDiagram\nparticipant cron job as Cron";

    let res = block_on(engine.parse_diagram(text, ParseOptions::default()))
        .unwrap()
        .unwrap();
    let actors = res.model["actors"].as_object().unwrap();

    assert_eq!(actors.len(), 1);
    assert_eq!(
        actors["cron job as Cron"]["description"],
        json!("cron job as Cron")
    );
}

#[test]
fn parse_diagram_sequence_rejects_config_on_spaced_declaration_ids() {
    let engine = Engine::new();

    for actor in ["cron job", "cron\u{a0}job", "api\u{feff}svc"] {
        let text = format!("sequenceDiagram\nparticipant {actor}@{{ \"type\": \"database\" }}");
        assert!(
            block_on(engine.parse_diagram(&text, ParseOptions::default())).is_err(),
            "config must not extend the pinned declaration-ID grammar for {actor:?}"
        );
    }

    let valid = "sequenceDiagram\nparticipant api\u{85}svc@{ \"type\": \"database\" }";
    let parsed = block_on(engine.parse_diagram(valid, ParseOptions::default()))
        .unwrap()
        .unwrap();
    assert!(parsed.model["actors"].get("api\u{85}svc").is_some());
}

#[test]
fn parse_diagram_sequence_requires_whitespace_before_alias_after_config() {
    let engine = Engine::new();

    let invalid = "sequenceDiagram\nparticipant A@{ \"type\": \"database\" }as Label";
    assert!(
        block_on(engine.parse_diagram(invalid, ParseOptions::default())).is_err(),
        "the pinned CONFIG state requires whitespace before its AS transition"
    );

    let valid = "sequenceDiagram\nparticipant A@{ \"type\": \"database\" }\u{feff}as\u{a0}Label";
    let parsed = block_on(engine.parse_diagram(valid, ParseOptions::default()))
        .unwrap()
        .unwrap();
    assert_eq!(parsed.model["actors"]["A"]["description"], json!("Label"));
}

#[test]
fn parse_diagram_sequence_preserves_empty_alias_descriptions() {
    let engine = Engine::new();
    let text = concat!(
        "sequenceDiagram\n",
        "participant A as\n",
        "participant B@{ \"type\": \"database\" } as\n",
    );

    let parsed = block_on(engine.parse_diagram(text, ParseOptions::default()))
        .unwrap()
        .unwrap();
    assert_eq!(parsed.model["actors"]["A"]["description"], json!(""));
    assert_eq!(parsed.model["actors"]["B"]["description"], json!(""));
    assert_eq!(parsed.model["actors"]["B"]["type"], json!("database"));
}

#[test]
fn parse_diagram_sequence_rejects_invalid_actor_token_extensions() {
    let engine = Engine::new();

    for statement in [
        "A()B->>C: embedded central marker",
        "foo--bar->>C: bare double dash source",
        "A->>foo--bar: bare double dash target",
        "left of-worker->>B: relative-note keyword source",
        "A->>right of-worker: relative-note keyword target",
    ] {
        let text = format!("sequenceDiagram\n{statement}");
        assert!(
            block_on(engine.parse_diagram(&text, ParseOptions::default())).is_err(),
            "pinned Mermaid rejects the private actor token extension {statement:?}"
        );
    }

    let valid = "sequenceDiagram\nA() ->>B: spaced central suffix";
    assert!(
        block_on(engine.parse_diagram(valid, ParseOptions::default())).is_ok(),
        "a terminal central marker may be separated from its signal by inline whitespace"
    );
}

#[test]
fn parse_diagram_sequence_rejects_reference_private_quoted_actor_ids() {
    let engine = Engine::new();
    let text = "sequenceDiagram\n\"A->B\" ->> C: quoted arrow name";

    assert!(
        block_on(engine.parse_diagram(text, ParseOptions::default())).is_err(),
        "pinned Mermaid rejects the moving reference's quoted actor extension"
    );
}

#[test]
fn parse_diagram_sequence_preserves_comment_markers_inside_actor_tokens() {
    let engine = Engine::new();
    let text = r#"sequenceDiagram
participant C#
participant A%%tag
C#->>B#tag: hash ids
A%%tag-->>B%%tag: percent comment
😀%%tag->>B: emoji percent id"#;

    let parsed = block_on(engine.parse_diagram(text, ParseOptions::default()))
        .unwrap()
        .unwrap();
    let actors = parsed.model["actors"].as_object().unwrap();
    for actor in ["C#", "A%%tag", "B#tag", "😀%%tag", "B"] {
        assert!(actors.contains_key(actor), "missing actor {actor:?}");
    }
    assert!(!actors.contains_key("B%%tag"));

    let messages = parsed.model["messages"].as_array().unwrap();
    assert_eq!(messages.len(), 2);
    assert_eq!(messages[0]["from"], json!("C#"));
    assert_eq!(messages[0]["to"], json!("B#tag"));
    assert_eq!(messages[1]["from"], json!("😀%%tag"));
    assert_eq!(messages[1]["to"], json!("B"));
}

#[test]
fn parse_diagram_sequence_keeps_percent_text_in_exclusive_line_states() {
    let engine = Engine::new();
    let text = r#"sequenceDiagram
participant A as B%%tag
loop A%%tag
A->>A: work%%item
end"#;

    let parsed = block_on(engine.parse_diagram(text, ParseOptions::default()))
        .unwrap()
        .unwrap();
    assert_eq!(parsed.model["actors"]["A"]["description"], json!("B%%tag"));
    let messages = parsed.model["messages"].as_array().unwrap();
    assert!(
        messages.iter().any(|message| {
            message["type"] == json!(10) && message["message"] == json!("A%%tag")
        })
    );
    assert!(messages.iter().any(|message| {
        message["from"] == json!("A") && message["message"] == json!("work%%item")
    }));
}

#[test]
fn parse_diagram_sequence_preserves_distinct_id_and_actor_character_sets() {
    let engine = Engine::new();
    let text = r#"sequenceDiagram
participant C++
participant api(v2)
participant api-xray
activate api-xray
deactivate api-xray
alice@example.com->>data@example.com: mail"#;

    let res = block_on(engine.parse_diagram(text, ParseOptions::default()))
        .unwrap()
        .unwrap();
    let actors = res.model["actors"].as_object().unwrap();
    for actor in ["C++", "api(v2)", "alice@example.com", "data@example.com"] {
        assert!(actors.contains_key(actor), "missing actor {actor:?}");
    }

    let messages = res.model["messages"].as_array().unwrap();
    assert!(messages.iter().any(|message| {
        message["from"] == json!("alice@example.com")
            && message["to"] == json!("data@example.com")
            && message["message"] == json!("mail")
    }));
}

#[test]
fn parse_diagram_sequence_preserves_spaced_actor_statement_contexts() {
    let engine = Engine::new();
    let text = r#"sequenceDiagram
participant cron job
participant data svc
activate cron job
deactivate cron job
cron job->>+data svc: inline start
data svc-->>-cron job: inline end
links cron job: { "Docs": "https://example.com/cron" }
properties data svc: {"class": "service"}
details cron job: {"owner": "platform"}
destroy cron job
data svc--xcron job: stop"#;

    let res = block_on(engine.parse_diagram(text, ParseOptions::default()))
        .unwrap()
        .unwrap();
    let actors = res.model["actors"].as_object().unwrap();
    assert_eq!(
        actors["cron job"]["links"]["Docs"],
        json!("https://example.com/cron")
    );
    assert_eq!(actors["data svc"]["properties"]["class"], json!("service"));
    assert!(res.model["destroyedActors"].get("cron job").is_some());

    let messages = res.model["messages"].as_array().unwrap();
    assert!(
        messages.iter().any(|message| {
            message["type"] == json!(17) && message["from"] == json!("cron job")
        })
    );
    assert!(
        messages.iter().any(|message| {
            message["type"] == json!(18) && message["from"] == json!("cron job")
        })
    );
    assert!(messages.iter().any(|message| {
        message["from"] == json!("data svc")
            && message["to"] == json!("cron job")
            && message["message"] == json!("stop")
    }));
}

#[test]
fn parse_diagram_sequence_accepts_pinned_unicode_whitespace_boundaries() {
    let engine = Engine::new();
    let text = "sequenceDiagram\ntitle\u{a0}Unicode spacing\nparticipant A\nNote left of\u{a0}A: left\nNote right of\u{a0}A: right";

    let res = block_on(engine.parse_diagram(text, ParseOptions::default()))
        .unwrap()
        .unwrap();
    assert_eq!(res.model["title"], json!("Unicode spacing"));
    let notes = res.model["notes"].as_array().unwrap();
    assert_eq!(notes.len(), 2);
    assert_eq!(notes[0]["message"], json!("left"));
    assert_eq!(notes[1]["message"], json!("right"));
}

#[test]
fn parse_diagram_sequence_keeps_fragment_labels_with_arrows_out_of_actor_scanning() {
    let engine = Engine::new();
    let text = r#"sequenceDiagram
participant cron job
participant data svc
participant worker as Worker -> primary
alt cache hit
cron job->>data svc: hit
else fall back -> origin: yes
cron job->>data svc: miss
end"#;

    let res = block_on(engine.parse_diagram(text, ParseOptions::default()))
        .unwrap()
        .unwrap();
    let actors = res.model["actors"].as_object().unwrap();
    assert_eq!(actors.len(), 3);
    assert!(!actors.contains_key("else fall back"));
    assert_eq!(actors["worker"]["description"], json!("Worker -> primary"));

    let messages = res.model["messages"].as_array().unwrap();
    assert_eq!(
        messages
            .iter()
            .filter(|message| message["type"] == json!(0))
            .count(),
        2
    );
    assert!(messages.iter().any(|message| {
        message["type"] == json!(13) && message["message"] == json!("fall back -> origin: yes")
    }));
}

#[test]
fn parse_diagram_sequence_properties() {
    let engine = Engine::new();
    let text = r#"sequenceDiagram
participant a as Alice
participant b as Bob
participant c as Charlie
properties a: {"class": "internal-service-actor", "icon": "@clock"}
properties b: {"class": "external-service-actor", "icon": "@computer"}
"#;

    let res = block_on(engine.parse_diagram(text, ParseOptions::default()))
        .unwrap()
        .unwrap();
    let actors = res.model["actors"].as_object().unwrap();
    assert_eq!(
        actors["a"]["properties"]["class"],
        json!("internal-service-actor")
    );
    assert_eq!(
        actors["b"]["properties"]["class"],
        json!("external-service-actor")
    );
    assert_eq!(actors["a"]["properties"]["icon"], json!("@clock"));
    assert_eq!(actors["b"]["properties"]["icon"], json!("@computer"));
    assert_eq!(actors["c"]["properties"].get("class"), None);
}

#[test]
fn parse_diagram_sequence_box_color_and_membership() {
    let engine = Engine::new();
    let text = r#"sequenceDiagram
box green Group 1
participant a as Alice
participant b as Bob
end
participant c as Charlie
links a: { "Repo": "https://repo.contoso.com/", "Dashboard": "https://dashboard.contoso.com/" }
links b: { "Dashboard": "https://dashboard.contoso.com/" }
links a: { "On-Call": "https://oncall.contoso.com/?svc=alice" }
link a: Endpoint @ https://alice.contoso.com
link a: Swagger @ https://swagger.contoso.com
link a: Tests @ https://tests.contoso.com/?svc=alice@contoso.com
"#;

    let res = block_on(engine.parse_diagram(text, ParseOptions::default()))
        .unwrap()
        .unwrap();
    let boxes = res.model["boxes"].as_array().unwrap();
    assert_eq!(boxes[0]["name"], json!("Group 1"));
    assert_eq!(boxes[0]["actorKeys"], json!(["a", "b"]));
    assert_eq!(boxes[0]["fill"], json!("green"));
}

#[test]
fn parse_diagram_sequence_box_without_color_defaults_to_transparent() {
    let engine = Engine::new();
    let text = r#"sequenceDiagram
box Group 1
participant a as Alice
participant b as Bob
end
"#;

    let res = block_on(engine.parse_diagram(text, ParseOptions::default()))
        .unwrap()
        .unwrap();
    let boxes = res.model["boxes"].as_array().unwrap();
    assert_eq!(boxes[0]["name"], json!("Group 1"));
    assert_eq!(boxes[0]["actorKeys"], json!(["a", "b"]));
    assert_eq!(boxes[0]["fill"], json!("transparent"));
}

#[test]
fn parse_diagram_sequence_box_without_description_has_falsy_name() {
    let engine = Engine::new();
    let text = r#"sequenceDiagram
box aqua
participant a as Alice
participant b as Bob
end
"#;

    let res = block_on(engine.parse_diagram(text, ParseOptions::default()))
        .unwrap()
        .unwrap();
    let boxes = res.model["boxes"].as_array().unwrap();
    assert!(boxes[0]["name"].is_null());
    assert_eq!(boxes[0]["actorKeys"], json!(["a", "b"]));
    assert_eq!(boxes[0]["fill"], json!("aqua"));
}

#[test]
fn parse_diagram_sequence_box_rgb_color() {
    let engine = Engine::new();
    let text = r#"sequenceDiagram
box rgb(34, 56, 0) Group1
participant a as Alice
participant b as Bob
end
"#;

    let res = block_on(engine.parse_diagram(text, ParseOptions::default()))
        .unwrap()
        .unwrap();
    let boxes = res.model["boxes"].as_array().unwrap();
    assert_eq!(boxes[0]["name"], json!("Group1"));
    assert_eq!(boxes[0]["fill"], json!("rgb(34, 56, 0)"));
    assert_eq!(boxes[0]["actorKeys"], json!(["a", "b"]));
}

#[test]
fn parse_diagram_sequence_create_participant_and_actor() {
    let engine = Engine::new();
    let text = r#"sequenceDiagram
participant a as Alice
a ->>b: Hello Bob?
create participant c
b-->>c: Hello c!
c ->> b: Hello b?
create actor d as Donald
a ->> d: Hello Donald?
"#;

    let res = block_on(engine.parse_diagram(text, ParseOptions::default()))
        .unwrap()
        .unwrap();
    let actors = res.model["actors"].as_object().unwrap();
    let created = res.model["createdActors"].as_object().unwrap();

    assert_eq!(actors["c"]["name"], json!("c"));
    assert_eq!(actors["c"]["description"], json!("c"));
    assert_eq!(actors["c"]["type"], json!("participant"));
    assert_eq!(created["c"], json!(1));

    assert_eq!(actors["d"]["name"], json!("d"));
    assert_eq!(actors["d"]["description"], json!("Donald"));
    assert_eq!(actors["d"]["type"], json!("actor"));
    assert_eq!(created["d"], json!(3));
}

#[test]
fn parse_diagram_sequence_destroy_participant_marks_destroyed_actor_index() {
    let engine = Engine::new();
    let text = r#"sequenceDiagram
participant a as Alice
a ->>b: Hello Bob?
destroy a
b-->>a: Hello Alice!
b ->> c: Where is Alice?
destroy c
b ->> c: Where are you?
"#;

    let res = block_on(engine.parse_diagram(text, ParseOptions::default()))
        .unwrap()
        .unwrap();
    let destroyed = res.model["destroyedActors"].as_object().unwrap();
    assert_eq!(destroyed["a"], json!(1));
    assert_eq!(destroyed["c"], json!(3));
}

#[test]
fn parse_diagram_sequence_create_and_destroy_same_actor() {
    let engine = Engine::new();
    let text = r#"sequenceDiagram
a ->>b: Hello Bob?
create participant c
b ->>c: Hello c!
c ->> b: Hello b?
destroy c
b ->> c : Bye c !
"#;

    let res = block_on(engine.parse_diagram(text, ParseOptions::default()))
        .unwrap()
        .unwrap();
    let created = res.model["createdActors"].as_object().unwrap();
    let destroyed = res.model["destroyedActors"].as_object().unwrap();
    assert_eq!(created["c"], json!(1));
    assert_eq!(destroyed["c"], json!(3));
}

#[test]
fn parse_diagram_sequence_extended_participant_syntax_parses_type_override() {
    let engine = Engine::new();
    let text = r#"sequenceDiagram
participant Alice@{ "type" : "database" }
participant Bob@{ "type" : "database" }
participant Carl@{ type: "database" }
participant David@{ "type" : 'database' }
participant Eve@{ type: 'database' }
participant Favela@{ "type" : "database"    }
Bob->>+Alice: Hi Alice
Alice->>+Bob: Hi Bob
"#;

    let res = block_on(engine.parse_diagram(text, ParseOptions::default()))
        .unwrap()
        .unwrap();
    let actors = res.model["actors"].as_object().unwrap();

    for id in ["Alice", "Bob", "Carl", "David", "Eve", "Favela"] {
        assert_eq!(actors[id]["type"], json!("database"));
        assert_eq!(actors[id]["description"], json!(id));
    }
}

#[test]
fn parse_diagram_sequence_extended_participant_syntax_mixed_types_and_implicit_participants() {
    let engine = Engine::new();
    let text = r#"sequenceDiagram
participant lead
participant dsa@{ "type" : "queue" }
API->>+Database: getUserb
Database-->>-API: userb
dsa --> Database: hello
"#;

    let res = block_on(engine.parse_diagram(text, ParseOptions::default()))
        .unwrap()
        .unwrap();
    let actors = res.model["actors"].as_object().unwrap();

    assert_eq!(actors["lead"]["type"], json!("participant"));
    assert_eq!(actors["lead"]["description"], json!("lead"));
    assert_eq!(actors["dsa"]["type"], json!("queue"));
    assert_eq!(actors["dsa"]["description"], json!("dsa"));

    assert_eq!(actors["API"]["type"], json!("participant"));
    assert_eq!(actors["Database"]["type"], json!("participant"));
}

#[test]
fn parse_diagram_sequence_extended_participant_syntax_supports_aliases_and_actor_keyword() {
    let engine = Engine::new();
    let text = r#"sequenceDiagram
participant API@{ "type" : "boundary", "alias": "Internal API" } as Public API
actor DB@{ "type" : "database" } as Data Store
actor Queue@{ "type" : "queue", "alias": "Message Queue" }
API->>DB: query
DB->>Queue: enqueue
"#;

    let res = block_on(engine.parse_diagram(text, ParseOptions::default()))
        .unwrap()
        .unwrap();
    let actors = res.model["actors"].as_object().unwrap();

    assert_eq!(actors["API"]["type"], json!("boundary"));
    assert_eq!(actors["API"]["description"], json!("Public API"));
    assert_eq!(actors["DB"]["type"], json!("database"));
    assert_eq!(actors["DB"]["description"], json!("Data Store"));
    assert_eq!(actors["Queue"]["type"], json!("queue"));
    assert_eq!(actors["Queue"]["description"], json!("Message Queue"));
}

#[test]
fn parse_diagram_sequence_extended_participant_syntax_invalid_config_fails() {
    let engine = Engine::new();
    let bad_json = r#"sequenceDiagram
participant D@{ "type: "entity" }
participant E@{ "type": "dat
abase }
"#;
    assert!(block_on(engine.parse_diagram(bad_json, ParseOptions::default())).is_err());

    let missing_colon = r#"sequenceDiagram
participant C@{ "type" "control" }
C ->> C: action
"#;
    assert!(block_on(engine.parse_diagram(missing_colon, ParseOptions::default())).is_err());

    let missing_brace = r#"sequenceDiagram
participant E@{ "type": "entity"
E ->> E: process
"#;
    assert!(block_on(engine.parse_diagram(missing_brace, ParseOptions::default())).is_err());
}

#[test]
fn parse_diagram_sequence_deactivate_inactive_participant_fails_like_upstream() {
    let engine = Engine::new();
    let text = r#"sequenceDiagram
participant user as End User
participant Server as Server
participant System as System
participant System2 as System2

user->>+Server: Test
user->>+Server: Test2
user->>System: Test
Server->>-user: Test
Server->>-user: Test2

%% The following deactivation of Server will fail
Server->>-user: Test3"#;

    let err = block_on(engine.parse_diagram(text, ParseOptions::default())).unwrap_err();
    assert!(
        err.to_string()
            .contains("Trying to inactivate an inactive participant (Server)"),
        "unexpected error: {err}"
    );
}

#[test]
fn parse_diagram_sequence_alt_multiple_elses_inserts_control_messages() {
    let engine = Engine::new();
    let text = r#"sequenceDiagram
Alice->Bob: Hello Bob, how are you?

%% Comment
Note right of Bob: Bob thinks
alt isWell

Bob-->Alice: I am good thanks!
else isSick
Bob-->Alice: Feel sick...
else default
Bob-->Alice: :-)
end"#;

    let res = block_on(engine.parse_diagram(text, ParseOptions::default()))
        .unwrap()
        .unwrap();
    let messages = res.model["messages"].as_array().unwrap();

    assert_eq!(messages.len(), 9);
    assert_eq!(messages[1]["from"], json!("Bob"));
    assert_eq!(messages[2]["type"], json!(12));
    assert_eq!(messages[3]["from"], json!("Bob"));
    assert_eq!(messages[4]["type"], json!(13));
    assert_eq!(messages[5]["from"], json!("Bob"));
    assert_eq!(messages[6]["type"], json!(13));
    assert_eq!(messages[7]["from"], json!("Bob"));
    assert_eq!(messages[8]["type"], json!(14));
}

#[test]
fn parse_diagram_sequence_critical_without_options() {
    let engine = Engine::new();
    let text = r#"sequenceDiagram
critical Establish a connection to the DB
Service-->DB: connect
end"#;

    let res = block_on(engine.parse_diagram(text, ParseOptions::default()))
        .unwrap()
        .unwrap();
    let messages = res.model["messages"].as_array().unwrap();

    assert_eq!(messages.len(), 3);
    assert_eq!(messages[0]["type"], json!(27));
    assert_eq!(messages[1]["from"], json!("Service"));
    assert_eq!(messages[2]["type"], json!(29));
}

#[test]
fn parse_diagram_sequence_critical_with_options() {
    let engine = Engine::new();
    let text = r#"sequenceDiagram
critical Establish a connection to the DB
Service-->DB: connect
option Network timeout
Service-->Service: Log error
option Credentials rejected
Service-->Service: Log different error
end"#;

    let res = block_on(engine.parse_diagram(text, ParseOptions::default()))
        .unwrap()
        .unwrap();
    let messages = res.model["messages"].as_array().unwrap();

    assert_eq!(messages.len(), 7);
    assert_eq!(messages[0]["type"], json!(27));
    assert_eq!(messages[1]["from"], json!("Service"));
    assert_eq!(messages[2]["type"], json!(28));
    assert_eq!(messages[3]["from"], json!("Service"));
    assert_eq!(messages[4]["type"], json!(28));
    assert_eq!(messages[5]["from"], json!("Service"));
    assert_eq!(messages[6]["type"], json!(29));
}

#[test]
fn parse_diagram_sequence_break_block_inserts_control_messages() {
    let engine = Engine::new();
    let text = r#"sequenceDiagram
Consumer-->API: Book something
API-->BookingService: Start booking process
break when the booking process fails
API-->Consumer: show failure
end
API-->BillingService: Start billing process"#;

    let res = block_on(engine.parse_diagram(text, ParseOptions::default()))
        .unwrap()
        .unwrap();
    let messages = res.model["messages"].as_array().unwrap();

    assert_eq!(messages.len(), 6);
    assert_eq!(messages[0]["from"], json!("Consumer"));
    assert_eq!(messages[1]["from"], json!("API"));
    assert_eq!(messages[2]["type"], json!(30));
    assert_eq!(messages[3]["from"], json!("API"));
    assert_eq!(messages[4]["type"], json!(31));
    assert_eq!(messages[5]["from"], json!("API"));
}

#[test]
fn parse_diagram_sequence_par_over_block() {
    let engine = Engine::new();
    let text = r#"sequenceDiagram
par_over Parallel overlap
Alice ->> Bob: Message
Note left of Alice: Alice note
Note right of Bob: Bob note
end"#;

    let res = block_on(engine.parse_diagram(text, ParseOptions::default()))
        .unwrap()
        .unwrap();
    let messages = res.model["messages"].as_array().unwrap();

    assert_eq!(messages.len(), 5);
    assert_eq!(messages[0]["type"], json!(32));
    assert_eq!(messages[0]["message"], json!("Parallel overlap"));
    assert_eq!(messages[1]["from"], json!("Alice"));
    assert_eq!(messages[2]["from"], json!("Alice"));
    assert_eq!(messages[3]["from"], json!("Bob"));
    assert_eq!(messages[4]["type"], json!(21));
}

#[test]
fn parse_diagram_sequence_special_characters_in_loop_opt_alt_par() {
    let engine = Engine::new();

    let res = block_on(engine.parse_diagram(
        r#"sequenceDiagram
Alice->Bob: Hello Bob, how are you?
loop -:<>,;# comment
Bob-->Alice: I am good thanks!
end"#,
        ParseOptions::default(),
    ))
    .unwrap()
    .unwrap();
    let messages = res.model["messages"].as_array().unwrap();
    assert_eq!(messages[1]["message"], json!("-:<>,"));

    let res = block_on(engine.parse_diagram(
        r#"sequenceDiagram
Alice->Bob: Hello Bob, how are you?
opt -:<>,;# comment
Bob-->Alice: I am good thanks!
end"#,
        ParseOptions::default(),
    ))
    .unwrap()
    .unwrap();
    let messages = res.model["messages"].as_array().unwrap();
    assert_eq!(messages[1]["message"], json!("-:<>,"));

    let res = block_on(engine.parse_diagram(
        r#"sequenceDiagram
Alice->Bob: Hello Bob, how are you?
alt -:<>,;# comment
Bob-->Alice: I am good thanks!
else ,<>:-#; comment
Bob-->Alice: I am good thanks!
end"#,
        ParseOptions::default(),
    ))
    .unwrap()
    .unwrap();
    let messages = res.model["messages"].as_array().unwrap();
    assert_eq!(messages[1]["message"], json!("-:<>,"));
    assert_eq!(messages[3]["message"], json!(",<>:-"));

    let res = block_on(engine.parse_diagram(
        r#"sequenceDiagram
Alice->Bob: Hello Bob, how are you?
par -:<>,;# comment
Bob-->Alice: I am good thanks!
and ,<>:-#; comment
Bob-->Alice: I am good thanks!
end"#,
        ParseOptions::default(),
    ))
    .unwrap()
    .unwrap();
    let messages = res.model["messages"].as_array().unwrap();
    assert_eq!(messages[1]["message"], json!("-:<>,"));
    assert_eq!(messages[3]["message"], json!(",<>:-"));
}

#[test]
fn parse_diagram_sequence_no_label_loop_opt_alt_par() {
    let engine = Engine::new();

    let res = block_on(engine.parse_diagram(
        r#"sequenceDiagram
Alice->Bob: Hello Bob, how are you?
loop
Bob-->Alice: I am good thanks!
end"#,
        ParseOptions::default(),
    ))
    .unwrap()
    .unwrap();
    let messages = res.model["messages"].as_array().unwrap();
    assert_eq!(messages[1]["message"], json!(""));
    assert_eq!(messages[2]["message"], json!("I am good thanks!"));

    let res = block_on(engine.parse_diagram(
        r#"sequenceDiagram
Alice->Bob: Hello Bob, how are you?
opt # comment
Bob-->Alice: I am good thanks!
end"#,
        ParseOptions::default(),
    ))
    .unwrap()
    .unwrap();
    let messages = res.model["messages"].as_array().unwrap();
    assert_eq!(messages[1]["message"], json!(""));
    assert_eq!(messages[2]["message"], json!("I am good thanks!"));

    let res = block_on(engine.parse_diagram(
        r#"sequenceDiagram
Alice->Bob: Hello Bob, how are you?
alt;Bob-->Alice: I am good thanks!
else # comment
Bob-->Alice: I am good thanks!
end"#,
        ParseOptions::default(),
    ))
    .unwrap()
    .unwrap();
    let messages = res.model["messages"].as_array().unwrap();
    assert_eq!(messages[1]["message"], json!(""));
    assert_eq!(messages[2]["message"], json!("I am good thanks!"));
    assert_eq!(messages[3]["message"], json!(""));
    assert_eq!(messages[4]["message"], json!("I am good thanks!"));

    let res = block_on(engine.parse_diagram(
        r#"sequenceDiagram
Alice->Bob: Hello Bob, how are you?
par;Bob-->Alice: I am good thanks!
and # comment
Bob-->Alice: I am good thanks!
end"#,
        ParseOptions::default(),
    ))
    .unwrap()
    .unwrap();
    let messages = res.model["messages"].as_array().unwrap();
    assert_eq!(messages[1]["message"], json!(""));
    assert_eq!(messages[2]["message"], json!("I am good thanks!"));
    assert_eq!(messages[3]["message"], json!(""));
    assert_eq!(messages[4]["message"], json!("I am good thanks!"));
}

#[test]
fn sequence_actor_configs_accept_whitespace_and_hyphenated_ids() {
    let engine = Engine::new();
    for (keyword, id, gap) in [
        ("participant", "Harry", " "),
        ("actor", "lead-actor", ""),
        ("participant", "order-svc", "\t"),
        ("actor", "data=svc", "\u{feff}"),
    ] {
        let text = format!(
            "sequenceDiagram\n{keyword} {id}{gap}@{{ \"type\": \"database\" }} as Display Name\n"
        );
        let parsed = engine
            .parse_diagram_sync(&text, ParseOptions::strict())
            .unwrap()
            .unwrap();
        assert_eq!(parsed.model["actors"][id]["type"], "database");
        assert_eq!(parsed.model["actors"][id]["description"], "Display Name");
        let facts = engine
            .parse_editor_semantic_facts_with_type_sync("sequence", &text)
            .unwrap()
            .unwrap();
        assert_eq!(facts.completeness, EditorSemanticCompleteness::Complete);
        let symbol = facts
            .symbols
            .iter()
            .find(|symbol| symbol.name == id)
            .unwrap();
        assert_eq!(&text[symbol.selection.start..symbol.selection.end], id);
    }
}

#[test]
fn sequence_actor_menu_keywords_are_message_endpoints_and_keep_menu_statements() {
    let engine = Engine::new();
    for id in ["Link", "link", "LINK", "Links", "Properties", "Details"] {
        for declared in [false, true] {
            let declaration = if declared {
                format!("participant {id} as Channel\n")
            } else {
                String::new()
            };
            let text = format!(
                "sequenceDiagram\n{declaration}{id}  -->>A: message\nA->>{id} : reply\nlink {id}: Help @ https://example.com/help\nlinks {id}: {{\"Repo\": \"https://example.com/repo\"}}\nproperties {id}: {{\"color\": \"red\"}}\n"
            );
            let parsed = engine
                .parse_diagram_sync(&text, ParseOptions::strict())
                .unwrap()
                .unwrap();
            assert_eq!(parsed.model["messages"][0]["from"], id);
            assert_eq!(parsed.model["messages"][1]["to"], id);
            assert_eq!(
                parsed.model["actors"][id]["links"]["Help"],
                "https://example.com/help"
            );
            assert_eq!(
                parsed.model["actors"][id]["links"]["Repo"],
                "https://example.com/repo"
            );
            assert_eq!(parsed.model["actors"][id]["properties"]["color"], "red");
            if declared {
                assert_eq!(parsed.model["actors"][id]["description"], "Channel");
            }
        }
    }
}
