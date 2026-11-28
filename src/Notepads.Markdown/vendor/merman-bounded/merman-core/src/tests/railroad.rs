use crate::diagrams::railroad::{RailroadRepeatBound, RailroadRepeatBoundError};
use crate::*;
use serde_json::json;

#[test]
fn railroad_repeat_bound_validates_public_construction() {
    let negative_zero = RailroadRepeatBound::try_from(-0.0).expect("negative zero is valid");
    assert_eq!(negative_zero, RailroadRepeatBound::ZERO);
    assert_eq!(negative_zero.as_f64().to_bits(), 0.0f64.to_bits());

    assert!(RailroadRepeatBound::ZERO.is_zero());
    assert!(!RailroadRepeatBound::ZERO.is_one());
    assert!(RailroadRepeatBound::ONE.is_one());
    assert!(!RailroadRepeatBound::ONE.is_infinite());
    assert!(RailroadRepeatBound::INFINITY.is_infinite());
    assert_eq!(
        RailroadRepeatBound::try_from(f64::INFINITY).unwrap(),
        RailroadRepeatBound::INFINITY
    );
    assert_eq!(
        RailroadRepeatBound::from(u64::MAX).as_f64().to_bits(),
        (u64::MAX as f64).to_bits()
    );

    for (invalid, expected) in [
        (f64::NAN, RailroadRepeatBoundError::NotANumber),
        (f64::NEG_INFINITY, RailroadRepeatBoundError::Negative),
        (-1.0, RailroadRepeatBoundError::Negative),
        (0.5, RailroadRepeatBoundError::Fractional),
    ] {
        assert_eq!(
            RailroadRepeatBound::try_from(invalid),
            Err(expected),
            "unexpected error for invalid repeat bound: {invalid}"
        );
    }
}

#[test]
fn railroad_repeat_bound_serde_preserves_finite_and_infinite_states() {
    for (bound, expected_json) in [
        (RailroadRepeatBound::ZERO, "0"),
        (RailroadRepeatBound::ONE, "1"),
        (
            RailroadRepeatBound::try_from(9_007_199_254_740_992.0).unwrap(),
            "9007199254740992",
        ),
        (RailroadRepeatBound::INFINITY, "null"),
    ] {
        let encoded = serde_json::to_string(&bound).expect("repeat bound serializes");
        assert_eq!(encoded, expected_json);
        let decoded: RailroadRepeatBound =
            serde_json::from_str(&encoded).expect("repeat bound deserializes");
        assert_eq!(decoded, bound);
    }

    for bound in [
        RailroadRepeatBound::from(u64::MAX),
        RailroadRepeatBound::try_from(18_446_744_073_709_551_616.0).unwrap(),
        RailroadRepeatBound::try_from(f64::MAX).unwrap(),
    ] {
        let encoded = serde_json::to_string(&bound).expect("large finite bound serializes");
        let decoded: RailroadRepeatBound =
            serde_json::from_str(&encoded).expect("large finite bound deserializes");
        assert_eq!(decoded.as_f64().to_bits(), bound.as_f64().to_bits());
    }

    for value in [
        json!({
            "type": "repetition",
            "element": { "type": "terminal", "value": "item" },
            "min": 1
        }),
        json!({
            "type": "repetition",
            "element": { "type": "terminal", "value": "item" },
            "min": 1,
            "max": null
        }),
    ] {
        let node: crate::diagrams::railroad::RailroadAstNode =
            serde_json::from_value(value).expect("unbounded repetition deserializes");
        let crate::diagrams::railroad::RailroadAstNode::Repetition { max, .. } = node else {
            panic!("expected repetition node");
        };
        assert!(max.is_infinite());
    }

    for invalid_json in ["-1", "0.5", r#""1""#, "{}", "[]"] {
        assert!(
            serde_json::from_str::<RailroadRepeatBound>(invalid_json).is_err(),
            "invalid repeat bound JSON was accepted: {invalid_json}"
        );
    }
}

#[test]
fn parse_railroad_ir_rules_and_editor_facts() {
    let engine = Engine::new();
    let text = r#"railroad-beta
title "Simple Grammar"
rule = sequence(terminal("a"), nonterminal("other"), zeroOrMore(special("anything"))) ;
other = terminal("b") ;
"#;

    let parsed = engine
        .parse_diagram_snapshot_sync(text)
        .unwrap()
        .expect("railroad parses");

    assert_eq!(parsed.metadata().diagram_type, "railroad");
    assert_eq!(
        parsed
            .outcome()
            .parsed_model()
            .expect("expected parsed snapshot")["type"],
        json!("railroad")
    );
    assert_eq!(
        parsed
            .outcome()
            .parsed_model()
            .expect("expected parsed snapshot")["title"],
        json!("Simple Grammar")
    );
    assert_eq!(
        parsed
            .outcome()
            .parsed_model()
            .expect("expected parsed snapshot")["rules"][0]["name"],
        json!("rule")
    );
    assert_eq!(
        parsed
            .outcome()
            .parsed_model()
            .expect("expected parsed snapshot")["rules"][0]["definition"],
        json!({
            "type": "sequence",
            "elements": [
                { "type": "terminal", "value": "a" },
                { "type": "nonterminal", "name": "other" },
                {
                    "type": "repetition",
                    "element": { "type": "special", "text": "anything" },
                    "min": 0,
                    "max": null
                }
            ]
        })
    );

    let ParsedEditorFacts::Available(facts) = parsed.editor_facts() else {
        panic!("railroad should expose editor facts");
    };
    assert!(
        facts
            .directive_prefixes
            .iter()
            .any(|prefix| prefix == "title")
    );
    assert!(facts.symbols.iter().any(|symbol| {
        symbol.name == "rule"
            && symbol.detail.as_deref() == Some("railroad rule")
            && symbol.selection
                == SourceSpan::new(
                    text.find("rule =").unwrap(),
                    text.find("rule =").unwrap() + 4,
                )
    }));
    assert!(facts.symbols.iter().any(|symbol| {
        symbol.name == "other" && symbol.detail.as_deref() == Some("railroad nonterminal reference")
    }));
    assert!(facts.symbols.iter().any(|symbol| {
        symbol.name == "a" && symbol.detail.as_deref() == Some("railroad terminal")
    }));
}

#[test]
fn parse_railroad_ebnf_choice_optional_repetition_and_special_sequence() {
    let engine = Engine::new();
    let parsed = engine
        .parse_diagram_sync(
            r#"railroad-ebnf-beta
rule ::= "a" | [ other ] , { ? special ? } ;
"#,
            ParseOptions::strict(),
        )
        .unwrap()
        .expect("railroad ebnf parses");

    assert_eq!(parsed.meta.diagram_type, "railroadEbnf");
    assert_eq!(parsed.model["rules"][0]["name"], json!("rule"));
    assert_eq!(
        parsed.model["rules"][0]["definition"],
        json!({
            "type": "choice",
            "alternatives": [
                { "type": "terminal", "value": "a" },
                {
                    "type": "sequence",
                    "elements": [
                        { "type": "optional", "element": { "type": "nonterminal", "name": "other" } },
                        {
                            "type": "repetition",
                            "element": { "type": "special", "text": "special" },
                            "min": 0,
                            "max": null
                        }
                    ]
                }
            ]
        })
    );
}

#[test]
fn parse_railroad_abnf_repetition_optional_numval_and_comments() {
    let engine = Engine::new();
    let parsed = engine
        .parse_diagram_sync(
            r#"railroad-abnf-beta
; comment
rule = 1*2"hello" / [ other-rule ] / %x41 ;
"#,
            ParseOptions::strict(),
        )
        .unwrap()
        .expect("railroad abnf parses");

    assert_eq!(parsed.meta.diagram_type, "railroadAbnf");
    assert_eq!(
        parsed.model["rules"][0]["definition"],
        json!({
            "type": "choice",
            "alternatives": [
                {
                    "type": "repetition",
                    "element": { "type": "terminal", "value": "hello" },
                    "min": 1,
                    "max": 2
                },
                {
                    "type": "optional",
                    "element": { "type": "nonterminal", "name": "other-rule" }
                },
                { "type": "terminal", "value": "%x41" }
            ]
        })
    );
}

#[test]
fn parse_railroad_abnf_matches_javascript_repeat_number_semantics() {
    let huge = "9".repeat(400);
    let source = format!(
        r#"railroad-abnf-beta
rule = 9007199254740991"a" / 9007199254740992"b" / 9007199254740993"c" / 18446744073709551615*"d" / 18446744073709551616*"e" / *3"f" / 2*"g" / *"h" / 5*2"i" / *1"j" / {huge}"k" / 00000000000000000002*00000000000000000003"l" / {huge}*1"m" / 1*{huge}"n" ;
"#
    );
    let parsed = Engine::new()
        .parse_diagram_sync(&source, ParseOptions::strict())
        .unwrap()
        .expect("all valid ABNF repetition bounds should parse");

    let alternatives = parsed.model["rules"][0]["definition"]["alternatives"]
        .as_array()
        .expect("choice alternatives");
    assert_eq!(alternatives.len(), 14);
    assert_eq!(alternatives[0]["min"].as_u64(), Some(9_007_199_254_740_991));
    assert_eq!(alternatives[0]["max"].as_u64(), Some(9_007_199_254_740_991));
    assert_eq!(
        alternatives[1]["min"].as_f64(),
        Some(9_007_199_254_740_992.0)
    );
    assert_eq!(
        alternatives[2]["min"].as_f64(),
        Some(9_007_199_254_740_992.0)
    );
    assert_eq!(
        alternatives[3]["min"].as_f64(),
        Some(18_446_744_073_709_551_616.0)
    );
    assert_eq!(
        alternatives[4]["min"].as_f64(),
        Some(18_446_744_073_709_551_616.0)
    );
    assert_eq!(alternatives[3]["max"], json!(null));
    assert_eq!(alternatives[4]["max"], json!(null));
    assert_eq!(
        (
            alternatives[5]["min"].as_u64(),
            alternatives[5]["max"].as_u64()
        ),
        (Some(0), Some(3))
    );
    assert_eq!(
        (alternatives[6]["min"].as_u64(), &alternatives[6]["max"]),
        (Some(2), &json!(null))
    );
    assert_eq!(
        (alternatives[7]["min"].as_u64(), &alternatives[7]["max"]),
        (Some(0), &json!(null))
    );
    assert_eq!(
        (
            alternatives[8]["min"].as_u64(),
            alternatives[8]["max"].as_u64()
        ),
        (Some(5), Some(2))
    );
    assert_eq!(alternatives[9]["type"], json!("optional"));
    assert_eq!(alternatives[10]["min"], json!(null));
    assert_eq!(alternatives[10]["max"], json!(null));
    assert_eq!(
        (
            alternatives[11]["min"].as_u64(),
            alternatives[11]["max"].as_u64()
        ),
        (Some(2), Some(3))
    );
    assert_eq!(alternatives[12]["min"], json!(null));
    assert_eq!(alternatives[12]["max"].as_u64(), Some(1));
    assert_eq!(alternatives[13]["min"].as_u64(), Some(1));
    assert_eq!(alternatives[13]["max"], json!(null));
}

#[test]
fn parse_railroad_peg_repetitions_keep_ordinary_json_shape() {
    let parsed = Engine::new()
        .parse_diagram_sync(
            "railroad-peg-beta\nrule <- zero* one+ ;\n",
            ParseOptions::strict(),
        )
        .unwrap()
        .expect("railroad PEG repetitions parse");

    assert_eq!(
        parsed.model["rules"][0]["definition"],
        json!({
            "type": "sequence",
            "elements": [
                {
                    "type": "repetition",
                    "element": { "type": "nonterminal", "name": "zero" },
                    "min": 0,
                    "max": null
                },
                {
                    "type": "repetition",
                    "element": { "type": "nonterminal", "name": "one" },
                    "min": 1,
                    "max": null
                }
            ]
        })
    );
}

#[test]
fn parse_railroad_peg_prefix_suffix_any_and_editor_facts() {
    let engine = Engine::new();
    let text = r#"railroad-peg-beta
rule <- &"a" !"b" . other? ;
"#;
    let parsed = engine
        .parse_diagram_snapshot_sync(text)
        .unwrap()
        .expect("railroad peg parses");

    assert_eq!(parsed.metadata().diagram_type, "railroadPeg");
    assert_eq!(
        parsed
            .outcome()
            .parsed_model()
            .expect("expected parsed snapshot")["rules"][0]["definition"],
        json!({
            "type": "sequence",
            "elements": [
                { "type": "special", "text": "&\"a\"" },
                { "type": "special", "text": "!\"b\"" },
                { "type": "special", "text": "." },
                { "type": "optional", "element": { "type": "nonterminal", "name": "other" } }
            ]
        })
    );

    let ParsedEditorFacts::Available(facts) = parsed.editor_facts() else {
        panic!("railroad PEG should expose editor facts");
    };
    assert!(facts.symbols.iter().any(|symbol| {
        symbol.name == "other"
            && symbol.detail.as_deref() == Some("railroad peg nonterminal reference")
    }));
}

#[test]
fn parse_railroad_variants_expose_typed_render_models() {
    let engine = Engine::new();
    for (diagram_type, source) in [
        ("railroad", "railroad-beta\nrule = terminal(\"a\") ;\n"),
        ("railroadEbnf", "railroad-ebnf-beta\nrule = \"a\" ;\n"),
        ("railroadAbnf", "railroad-abnf-beta\nrule = \"a\" ;\n"),
        ("railroadPeg", "railroad-peg-beta\nrule <- \"a\" ;\n"),
    ] {
        let parsed = engine
            .parse_diagram_for_render_model_sync(source, ParseOptions::strict())
            .unwrap()
            .unwrap();

        assert_eq!(parsed.metadata().diagram_type, diagram_type);
        assert!(parsed.model().supports_diagram_type(diagram_type));
        let RenderSemanticModel::Railroad(model) = parsed.model() else {
            panic!("expected railroad render model for {diagram_type}");
        };
        assert_eq!(model.rules.len(), 1);
        assert_eq!(model.rules[0].name, "rule");
    }
}

#[test]
fn railroad_detection_is_case_insensitive_but_semantic_headers_are_exact() {
    let engine = Engine::new();
    for (expected_type, source) in [
        ("railroad", "RAILROAD-BETA\nrule = terminal(\"a\") ;\n"),
        ("railroadEbnf", "RAILROAD-EBNF-BETA\nrule = \"a\" ;\n"),
        ("railroadAbnf", "RAILROAD-ABNF-BETA\nrule = \"a\" ;\n"),
        ("railroadPeg", "RAILROAD-PEG-BETA\nrule <- \"a\" ;\n"),
    ] {
        let metadata = engine.parse_metadata_sync(source).unwrap();
        assert_eq!(metadata.diagram_type, expected_type);

        let error = engine
            .parse_diagram_sync(source, ParseOptions::strict())
            .expect_err("Langium railroad header literals are case-sensitive");
        assert!(
            error.to_string().contains("expected railroad"),
            "unexpected {expected_type} parse error: {error}"
        );
    }
}

#[test]
fn railroad_constructed_depth_counts_postfix_groups_and_collection_nodes() {
    use crate::diagrams::railroad::RailroadAstNode;

    let engine = Engine::new();
    let limit = MAX_DIAGRAM_NESTING_DEPTH;
    let cases = [
        (
            "railroad-beta",
            "=",
            format!(
                "{}terminal(\"a\"){}",
                "optional(".repeat(limit),
                ")".repeat(limit)
            ),
            format!(
                "{}terminal(\"a\"){}",
                "optional(".repeat(limit + 1),
                ")".repeat(limit + 1)
            ),
        ),
        (
            "railroad-ebnf-beta",
            "=",
            format!("{}\"a\"?{}", "[".repeat(limit - 1), "]".repeat(limit - 1)),
            format!("{}\"a\"?{}", "[".repeat(limit), "]".repeat(limit)),
        ),
        (
            "railroad-abnf-beta",
            "=",
            format!("*{}\"a\"{}", "[".repeat(limit - 1), "]".repeat(limit - 1)),
            format!("*{}\"a\"{}", "[".repeat(limit), "]".repeat(limit)),
        ),
        (
            "railroad-peg-beta",
            "<-",
            format!("{}\"a\"{}", "(".repeat(limit), ")?".repeat(limit)),
            format!("{}\"a\"?{}", "(".repeat(limit), ")?".repeat(limit)),
        ),
    ];
    for (header, assignment, accepted, excessive) in cases {
        let source = format!("{header}\nentry {assignment} {accepted} ;\n");
        let parsed = engine
            .parse_diagram_for_render_model_sync(&source, ParseOptions::strict())
            .expect("exact constructed-depth boundary parses")
            .unwrap();
        let RenderSemanticModel::Railroad(model) = parsed.model() else {
            panic!("expected Railroad model");
        };
        let mut pending = vec![(&model.rules[0].definition, 0usize)];
        let mut actual_depth = 0;
        while let Some((node, depth)) = pending.pop() {
            actual_depth = actual_depth.max(depth);
            match node {
                RailroadAstNode::Sequence { elements, .. } => {
                    pending.extend(elements.iter().map(|child| (child, depth + 1)));
                }
                RailroadAstNode::Choice { alternatives, .. } => {
                    pending.extend(alternatives.iter().map(|child| (child, depth + 1)));
                }
                RailroadAstNode::Optional { element, .. } => pending.push((element, depth + 1)),
                RailroadAstNode::Repetition {
                    element, separator, ..
                } => {
                    pending.push((element, depth + 1));
                    if let Some(separator) = separator {
                        pending.push((separator, depth + 1));
                    }
                }
                _ => {}
            }
        }
        assert_eq!(actual_depth, limit, "{header}");
        let source = format!("{header}\nentry {assignment} {excessive} ;\n");
        let error = engine
            .parse_diagram_for_render_model_sync(&source, ParseOptions::strict())
            .expect_err("first constructed-depth excess must be rejected");
        assert!(
            error
                .to_string()
                .contains("railroad nesting depth exceeds 256"),
            "{header}: {error}"
        );
    }

    let deepest = format!("\"a\"{}", "?".repeat(limit));
    for expression in [
        format!("{deepest}, \"b\""),
        format!("{deepest} | \"b\""),
        format!("{deepest} - \"b\""),
    ] {
        let source = format!("railroad-ebnf-beta\nentry = {expression} ;\n");
        let error = engine
            .parse_diagram_for_render_model_sync(&source, ParseOptions::strict())
            .expect_err("sequence, choice, and exception nodes count as owned layers");
        assert!(
            error
                .to_string()
                .contains("railroad nesting depth exceeds 256"),
            "{error}"
        );
    }
}
