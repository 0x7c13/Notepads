use crate::*;
use serde_json::json;

#[test]
fn parse_cynefin_domains_transitions_and_editor_facts() {
    let engine = Engine::new();
    let text = r#"cynefin-beta
  title Team Practices
  accTitle: Cynefin for team practices
  accDescr: A diagram showing the five Cynefin domains
  complex
    "Retrospectives"
    "Pair programming"
  complicated
    "Code review"
  complex --> complicated : "Pattern emerges"
  complicated --> complicated : "Self-loop"
"#;

    let parsed = engine
        .parse_diagram_snapshot_sync(text)
        .unwrap()
        .expect("cynefin parses");

    assert_eq!(parsed.metadata().diagram_type, "cynefin");
    assert_eq!(
        parsed
            .outcome()
            .parsed_model()
            .expect("expected parsed snapshot"),
        &json!({
            "type": "cynefin",
            "title": "Team Practices",
            "accTitle": "Cynefin for team practices",
            "accDescr": "A diagram showing the five Cynefin domains",
            "domains": [
                {
                    "name": "complex",
                    "items": [
                        { "label": "Retrospectives" },
                        { "label": "Pair programming" }
                    ]
                },
                {
                    "name": "complicated",
                    "items": [
                        { "label": "Code review" }
                    ]
                }
            ],
            "transitions": [
                {
                    "from": "complex",
                    "to": "complicated",
                    "label": "Pattern emerges"
                }
            ]
        })
    );

    let ParsedEditorFacts::Available(facts) = parsed.editor_facts() else {
        panic!("cynefin should expose editor facts");
    };
    assert!(
        facts
            .directive_prefixes
            .iter()
            .any(|prefix| prefix == "title")
    );
    assert!(facts.symbols.iter().any(|symbol| {
        symbol.name == "complex"
            && symbol.detail.as_deref() == Some("cynefin domain")
            && symbol.selection
                == SourceSpan::new(
                    text.find("complex").expect("complex domain"),
                    text.find("complex").expect("complex domain") + "complex".len(),
                )
    }));
    assert!(facts.symbols.iter().any(|symbol| {
        symbol.name == "Retrospectives" && symbol.detail.as_deref() == Some("cynefin domain item")
    }));
    assert!(facts.diagnostics.iter().any(|diagnostic| {
        diagnostic
            .message
            .contains("self-loop transition on domain \"complicated\" is skipped")
    }));
}

#[test]
fn parse_cynefin_accepts_colon_header_comments_single_quotes_and_escapes() {
    let engine = Engine::new();
    let text = "cynefin-beta:\n  %% comment\n  clear\n    'Known \\'good\\' practice %% kept'\n  clear --> chaotic %% transition comment\n";
    let parsed = engine
        .parse_diagram_sync(text, ParseOptions::strict())
        .unwrap()
        .expect("cynefin parses");

    assert_eq!(parsed.meta.diagram_type, "cynefin");
    assert_eq!(parsed.model["domains"][0]["name"], json!("clear"));
    assert_eq!(
        parsed.model["domains"][0]["items"][0]["label"],
        json!("Known 'good' practice %% kept")
    );
    assert_eq!(
        parsed.model["transitions"][0],
        json!({
            "from": "clear",
            "to": "chaotic"
        })
    );
}

#[test]
fn parse_cynefin_accepts_header_domains_and_items_on_the_same_line() {
    let engine = Engine::new();
    let text = r#"cynefin-beta complex "A" "B" complicated "C"
complicated --> clear : "Standardize""#;
    let parsed = engine
        .parse_diagram_snapshot_sync(text)
        .unwrap()
        .expect("cynefin parses tokens separated only by hidden whitespace");

    assert_eq!(
        parsed
            .outcome()
            .parsed_model()
            .expect("expected parsed snapshot")["domains"],
        json!([
            {
                "name": "complex",
                "items": [{ "label": "A" }, { "label": "B" }]
            },
            {
                "name": "complicated",
                "items": [{ "label": "C" }]
            }
        ])
    );
    assert_eq!(
        parsed
            .outcome()
            .parsed_model()
            .expect("expected parsed snapshot")["transitions"],
        json!([{
            "from": "complicated",
            "to": "clear",
            "label": "Standardize"
        }])
    );

    let ParsedEditorFacts::Available(facts) = parsed.editor_facts() else {
        panic!("cynefin should expose editor facts");
    };
    assert!(facts.symbols.iter().any(|symbol| {
        symbol.name == "A" && symbol.detail.as_deref() == Some("cynefin domain item")
    }));
    assert!(facts.symbols.iter().any(|symbol| {
        symbol.name == "complicated" && symbol.detail.as_deref() == Some("cynefin domain")
    }));
}

#[test]
fn parse_cynefin_accepts_multiline_accessibility_directive_after_inline_header() {
    let engine = Engine::new();
    let text = r#"cynefin-beta accDescr {
  Inline header description
}
complex "A""#;
    let parsed = engine
        .parse_diagram_sync(text, ParseOptions::strict())
        .unwrap()
        .expect("cynefin parses inline header before multiline common directive");

    assert_eq!(parsed.model["accDescr"], json!("Inline header description"));
    assert_eq!(parsed.model["domains"][0]["name"], json!("complex"));
    assert_eq!(parsed.model["domains"][0]["items"][0]["label"], json!("A"));
}

#[test]
fn parse_cynefin_unescapes_common_strings_like_langium() {
    let engine = Engine::new();
    let parsed = engine
        .parse_diagram_sync(
            r#"cynefin-beta
  complex
    "Probe\nsense\trespond\bnow"
  complex --> clear : 'Move\rnow\vsoon\0'
"#,
            ParseOptions::strict(),
        )
        .unwrap()
        .expect("cynefin parses escaped strings");

    assert_eq!(
        parsed.model["domains"][0]["items"][0]["label"],
        json!("Probensensetrespondbnow")
    );
    assert_eq!(
        parsed.model["transitions"][0]["label"],
        json!("Movernowvsoon0")
    );
}

#[test]
fn parse_cynefin_transition_labels_follow_javascript_truthiness() {
    let engine = Engine::new();
    let parsed = engine
        .parse_diagram_sync(
            "cynefin-beta\n  complex\n  complicated\n  clear\n  complex --> complicated : \"\"\n  complicated --> clear : \"   \"\n",
            ParseOptions::strict(),
        )
        .unwrap()
        .expect("cynefin parses");

    assert!(parsed.model["transitions"][0].get("label").is_none());
    assert_eq!(parsed.model["transitions"][1]["label"], json!("   "));
}

#[test]
fn parse_cynefin_normalizes_common_fields_and_multiline_acc_descr() {
    let engine = Engine::new();
    let parsed = engine
        .parse_diagram_snapshot_sync(
            "cynefin-beta\n  title    Team\t\tPractices   Map  \n  accTitle:   Team\t\tMap  \n  accDescr {\n    First    line\n\n      Second\t\tline   \n  }\n  clear\n",
        )
        .unwrap()
        .expect("cynefin parses multiline accessibility metadata");

    assert_eq!(
        parsed
            .outcome()
            .parsed_model()
            .expect("expected parsed snapshot")["title"],
        json!("Team Practices Map")
    );
    assert_eq!(
        parsed
            .outcome()
            .parsed_model()
            .expect("expected parsed snapshot")["accTitle"],
        json!("Team Map")
    );
    assert_eq!(
        parsed
            .outcome()
            .parsed_model()
            .expect("expected parsed snapshot")["accDescr"],
        json!("First line\nSecond line")
    );

    let ParsedEditorFacts::Available(facts) = parsed.editor_facts() else {
        panic!("cynefin should expose editor facts");
    };
    assert!(facts.symbols.iter().any(|symbol| {
        symbol.detail.as_deref() == Some("cynefin accessibility description")
            && symbol.name == "First line\nSecond line"
    }));
}

#[test]
fn parse_cynefin_duplicate_domain_replaces_prior_items_like_upstream_map_set() {
    let engine = Engine::new();
    let parsed = engine
        .parse_diagram_sync(
            "cynefin-beta\n  complex\n    \"Old\"\n  complex\n    \"New\"\n",
            ParseOptions::strict(),
        )
        .unwrap()
        .expect("cynefin parses");

    assert_eq!(
        parsed.model["domains"],
        json!([
            {
                "name": "complex",
                "items": [
                    { "label": "New" }
                ]
            }
        ])
    );
}

#[test]
fn parse_cynefin_render_model_uses_typed_model() {
    let engine = Engine::new();
    let parsed = engine
        .parse_diagram_for_render_model_sync("cynefin-beta\n  complex\n", ParseOptions::strict())
        .unwrap()
        .expect("cynefin render model parses");

    assert_eq!(parsed.metadata().diagram_type, "cynefin");
    assert_eq!(parsed.model().kind(), "cynefin");
    assert!(parsed.model().supports_diagram_type("cynefin"));
    let RenderSemanticModel::Cynefin(model) = parsed.model() else {
        panic!("expected typed cynefin render model");
    };
    assert_eq!(model.domains[0].name, "complex");
}
