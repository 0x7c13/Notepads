use crate::*;
use futures::executor::block_on;
use serde_json::json;

#[test]
fn parse_diagram_er_allows_standalone_entities() {
    let engine = Engine::new();
    let text = "erDiagram\nISLAND\nMAINLAND\n";
    let res = block_on(engine.parse_diagram(text, ParseOptions::default()))
        .unwrap()
        .unwrap();
    assert_eq!(res.meta.diagram_type, "er");
    assert_eq!(res.model["relationships"].as_array().unwrap().len(), 0);
    assert_eq!(res.model["entities"].as_object().unwrap().len(), 2);
    assert!(res.model["entities"].get("ISLAND").is_some());
    assert!(res.model["entities"].get("MAINLAND").is_some());
}

#[test]
fn parse_diagram_er_parses_alias_and_attributes() {
    let engine = Engine::new();
    let text = r#"erDiagram
foo["bar"] {
  string title PK, FK "comment"
}
"#;
    let res = block_on(engine.parse_diagram(text, ParseOptions::default()))
        .unwrap()
        .unwrap();

    let e = &res.model["entities"]["foo"];
    assert_eq!(e["alias"], json!("bar"));
    assert_eq!(e["attributes"][0]["type"], json!("string"));
    assert_eq!(e["attributes"][0]["name"], json!("title"));
    assert_eq!(e["attributes"][0]["keys"], json!(["PK", "FK"]));
    assert_eq!(e["attributes"][0]["comment"], json!("comment"));
}

#[test]
fn parse_diagram_er_empty_quoted_entity_name_is_error() {
    let engine = Engine::new();
    let err = block_on(engine.parse_diagram("erDiagram\n\"\"\n", ParseOptions::default()))
        .unwrap_err()
        .to_string();
    assert!(err.contains("DiagramParse") || err.contains("Unsupported") || !err.is_empty());
}

#[test]
fn parse_diagram_er_rejects_percent_and_backslash_in_quoted_entity_name() {
    let engine = Engine::new();
    assert!(
        block_on(engine.parse_diagram("erDiagram\n\"Blo%rf\"\n", ParseOptions::default())).is_err()
    );
    assert!(
        block_on(engine.parse_diagram("erDiagram\n\"Blo\\\\rf\"\n", ParseOptions::default()))
            .is_err()
    );
}

#[test]
fn parse_diagram_er_supports_empty_attribute_blocks_and_multiple_blocks() {
    let engine = Engine::new();
    let text = r#"erDiagram
BOOK {}
BOOK {
  string title
}
BOOK{
  string author
}
"#;
    let res = block_on(engine.parse_diagram(text, ParseOptions::default()))
        .unwrap()
        .unwrap();
    let e = &res.model["entities"]["BOOK"];
    assert_eq!(e["attributes"].as_array().unwrap().len(), 2);
    assert_eq!(e["attributes"][0]["name"], json!("title"));
    assert_eq!(e["attributes"][1]["name"], json!("author"));
}

#[test]
fn parse_diagram_er_alias_applies_even_when_relationship_is_defined_first() {
    let engine = Engine::new();
    let text = r#"erDiagram
foo ||--o{ bar : rel
foo["batman"]
"#;
    let res = block_on(engine.parse_diagram(text, ParseOptions::default()))
        .unwrap()
        .unwrap();
    assert_eq!(res.model["entities"]["foo"]["alias"], json!("batman"));
    assert_eq!(res.model["entities"]["bar"]["alias"], json!(""));
}

#[test]
fn parse_diagram_er_allows_multiple_statements_without_newlines_like_upstream_jison() {
    let engine = Engine::new();
    let text = r#"erDiagram
foo ||--o{ bar : rel
buzz foo["batman"]
"#;
    let res = block_on(engine.parse_diagram(text, ParseOptions::default()))
        .unwrap()
        .unwrap();

    assert_eq!(res.model["entities"]["foo"]["alias"], json!("batman"));
    assert_eq!(res.model["entities"]["bar"]["alias"], json!(""));
    assert!(res.model["entities"].get("buzz").is_some());
}

#[test]
fn parse_diagram_er_self_relationship_does_not_duplicate_entity() {
    let engine = Engine::new();
    let text = r#"erDiagram
NODE ||--o{ NODE : "leads to"
"#;
    let res = block_on(engine.parse_diagram(text, ParseOptions::default()))
        .unwrap()
        .unwrap();
    assert_eq!(res.model["entities"].as_object().unwrap().len(), 1);
    let rels = res.model["relationships"].as_array().unwrap();
    assert_eq!(rels.len(), 1);
    assert_eq!(rels[0]["entityA"], rels[0]["entityB"]);
}

#[test]
fn parse_diagram_er_inline_class_assignment_applies_css_classes() {
    let engine = Engine::new();
    let text = r#"erDiagram
FOO:::pink
classDef pink fill:#f9f
"#;
    let res = block_on(engine.parse_diagram(text, ParseOptions::default()))
        .unwrap()
        .unwrap();
    let e = &res.model["entities"]["FOO"];
    assert_eq!(e["cssClasses"], json!("default pink"));
    assert_eq!(res.model["classes"]["pink"]["styles"], json!(["fill:#f9f"]));
}

#[test]
fn parse_diagram_er_direction_statement_sets_direction() {
    let engine = Engine::new();
    let text = r#"erDiagram
direction LR
A ||--o{ B : has
"#;
    let res = block_on(engine.parse_diagram(text, ParseOptions::default()))
        .unwrap()
        .unwrap();
    assert_eq!(res.model["direction"], json!("LR"));
}

#[test]
fn parse_diagram_er_allows_hyphen_and_underscore_in_unquoted_entity_name() {
    let engine = Engine::new();
    let text = "erDiagram\nDUCK-BILLED_PLATYPUS\n";
    let res = block_on(engine.parse_diagram(text, ParseOptions::default()))
        .unwrap()
        .unwrap();
    assert!(res.model["entities"].get("DUCK-BILLED_PLATYPUS").is_some());
}

#[test]
fn parse_diagram_er_rejects_unquoted_entity_names_with_non_identifier_punctuation() {
    let engine = Engine::new();
    assert!(
        block_on(engine.parse_diagram("erDiagram\nBlo@rf\n", ParseOptions::default())).is_err()
    );
    assert!(
        block_on(engine.parse_diagram("erDiagram\nBlo?rf\n", ParseOptions::default())).is_err()
    );
}

#[test]
fn parse_diagram_er_supports_attribute_name_brackets_and_parens() {
    let engine = Engine::new();
    let text = r#"erDiagram
BOOK {
  string author-ref[name](1)
}
"#;
    let res = block_on(engine.parse_diagram(text, ParseOptions::default()))
        .unwrap()
        .unwrap();
    let attrs = res.model["entities"]["BOOK"]["attributes"]
        .as_array()
        .unwrap();
    assert_eq!(attrs.len(), 1);
    assert_eq!(attrs[0]["name"], json!("author-ref[name](1)"));
}

#[test]
fn parse_diagram_er_allows_asterisk_at_start_of_attribute_name() {
    let engine = Engine::new();
    let text = r#"erDiagram
BOOK {
  string *title
  id *the_Primary_Key
}
"#;
    let res = block_on(engine.parse_diagram(text, ParseOptions::default()))
        .unwrap()
        .unwrap();
    let attrs = res.model["entities"]["BOOK"]["attributes"]
        .as_array()
        .unwrap();
    assert_eq!(attrs.len(), 2);
}

#[test]
fn parse_diagram_er_rejects_attribute_names_with_leading_numbers_dashes_or_brackets() {
    let engine = Engine::new();
    for bad in ["0author", "-author", "[author", "(author"] {
        let text = format!("erDiagram\nBOOK {{\n  string {bad}\n}}\n");
        assert!(block_on(engine.parse_diagram(&text, ParseOptions::default())).is_err());
    }
}

#[test]
fn parse_diagram_er_supports_generic_and_array_and_limited_length_types() {
    let engine = Engine::new();
    let text = r#"erDiagram
BOOK {
  type~T~ type
  option~T~ readable "comment"
  string[] readers FK
  character(10) isbn FK
  varchar(5) postal_code "Five digits"
}
"#;
    let res = block_on(engine.parse_diagram(text, ParseOptions::default()))
        .unwrap()
        .unwrap();
    let attrs = res.model["entities"]["BOOK"]["attributes"]
        .as_array()
        .unwrap();
    assert_eq!(attrs.len(), 5);
    assert_eq!(attrs[3]["type"], json!("character(10)"));
    assert_eq!(attrs[4]["type"], json!("varchar(5)"));
    assert_eq!(attrs[4]["comment"], json!("Five digits"));
}

#[test]
fn parse_diagram_er_supports_mermaid_11_16_attribute_tokens() {
    let engine = Engine::new();
    let text = r#"erDiagram
PLACE {
  public.geometry(point,4326) location
  string? nickname
  `custom type` `display name` UK
}
"#;
    let res = block_on(engine.parse_diagram(text, ParseOptions::default()))
        .unwrap()
        .unwrap();
    let attrs = res.model["entities"]["PLACE"]["attributes"]
        .as_array()
        .unwrap();

    assert_eq!(attrs[0]["type"], json!("public.geometry(point,4326)"));
    assert_eq!(attrs[0]["name"], json!("location"));
    assert_eq!(attrs[1]["type"], json!("string?"));
    assert_eq!(attrs[1]["name"], json!("nickname"));
    assert_eq!(attrs[2]["type"], json!("custom type"));
    assert_eq!(attrs[2]["name"], json!("display name"));
    assert_eq!(attrs[2]["keys"], json!(["UK"]));
}

#[test]
fn parse_diagram_er_parses_many_constraints_and_comments() {
    let engine = Engine::new();
    let text = r#"erDiagram
CUSTOMER {
  int customer_number PK, FK "comment1"
  datetime customer_status_start_datetime PK,UK, FK
  datetime customer_status_end_datetime PK , UK "comment3"
  string customer_firstname
  string customer_lastname "comment5"
}
"#;
    let res = block_on(engine.parse_diagram(text, ParseOptions::default()))
        .unwrap()
        .unwrap();
    let attrs = res.model["entities"]["CUSTOMER"]["attributes"]
        .as_array()
        .unwrap();
    assert_eq!(attrs[0]["keys"], json!(["PK", "FK"]));
    assert_eq!(attrs[0]["comment"], json!("comment1"));
    assert_eq!(attrs[1]["keys"], json!(["PK", "UK", "FK"]));
    assert_eq!(attrs[2]["keys"], json!(["PK", "UK"]));
    assert_eq!(attrs[2]["comment"], json!("comment3"));
    assert_eq!(attrs[3]["keys"], json!([]));
    assert_eq!(attrs[4]["keys"], json!([]));
    assert_eq!(attrs[4]["comment"], json!("comment5"));
}

#[test]
fn parse_diagram_er_allows_multiple_relationships_between_same_two_entities() {
    let engine = Engine::new();
    let text = r#"erDiagram
CAR ||--o{ PERSON : "insured for"
CAR }o--|| PERSON : "owned by"
"#;
    let res = block_on(engine.parse_diagram(text, ParseOptions::default()))
        .unwrap()
        .unwrap();
    assert_eq!(res.model["entities"].as_object().unwrap().len(), 2);
    assert_eq!(res.model["relationships"].as_array().unwrap().len(), 2);
}

#[test]
fn parse_diagram_er_supports_one_or_more_cardinality_markers() {
    let engine = Engine::new();
    let text = r#"erDiagram
A ||--|{ B : has
"#;
    let res = block_on(engine.parse_diagram(text, ParseOptions::default()))
        .unwrap()
        .unwrap();
    let rels = res.model["relationships"].as_array().unwrap();
    assert_eq!(rels.len(), 1);
    assert_eq!(rels[0]["relSpec"]["cardA"], json!("ONE_OR_MORE"));
    assert_eq!(rels[0]["relSpec"]["cardB"], json!("ONLY_ONE"));
}

#[test]
fn parse_diagram_er_relationship_matrix_matches_upstream_spec_minimally() {
    let engine = Engine::new();
    let cases: &[(&str, &str, &str, &str)] = &[
        ("A ||--|{ B : has", "ONE_OR_MORE", "ONLY_ONE", "IDENTIFYING"),
        (
            "A ||..o{ B : has",
            "ZERO_OR_MORE",
            "ONLY_ONE",
            "NON_IDENTIFYING",
        ),
        (
            "A |o..o{ B : has",
            "ZERO_OR_MORE",
            "ZERO_OR_ONE",
            "NON_IDENTIFYING",
        ),
        (
            "A |o--|{ B : has",
            "ONE_OR_MORE",
            "ZERO_OR_ONE",
            "IDENTIFYING",
        ),
        ("A }|--|| B : has", "ONLY_ONE", "ONE_OR_MORE", "IDENTIFYING"),
        (
            "A }o--|| B : has",
            "ONLY_ONE",
            "ZERO_OR_MORE",
            "IDENTIFYING",
        ),
        (
            "A }o..o| B : has",
            "ZERO_OR_ONE",
            "ZERO_OR_MORE",
            "NON_IDENTIFYING",
        ),
        (
            "A }|..o| B : has",
            "ZERO_OR_ONE",
            "ONE_OR_MORE",
            "NON_IDENTIFYING",
        ),
        (
            "A |o..|| B : has",
            "ONLY_ONE",
            "ZERO_OR_ONE",
            "NON_IDENTIFYING",
        ),
        (
            "A ||..|| B : has",
            "ONLY_ONE",
            "ONLY_ONE",
            "NON_IDENTIFYING",
        ),
        ("A ||--o| B : has", "ZERO_OR_ONE", "ONLY_ONE", "IDENTIFYING"),
        (
            "A |o..o| B : has",
            "ZERO_OR_ONE",
            "ZERO_OR_ONE",
            "NON_IDENTIFYING",
        ),
        (
            "A }o--o{ B : has",
            "ZERO_OR_MORE",
            "ZERO_OR_MORE",
            "IDENTIFYING",
        ),
        (
            "A }|..|{ B : has",
            "ONE_OR_MORE",
            "ONE_OR_MORE",
            "NON_IDENTIFYING",
        ),
        (
            "A }o--|{ B : has",
            "ONE_OR_MORE",
            "ZERO_OR_MORE",
            "IDENTIFYING",
        ),
        (
            "A }|..o{ B : has",
            "ZERO_OR_MORE",
            "ONE_OR_MORE",
            "NON_IDENTIFYING",
        ),
        // relType variants
        (
            "A ||.-o{ B : has",
            "ZERO_OR_MORE",
            "ONLY_ONE",
            "NON_IDENTIFYING",
        ),
        (
            "A ||-.o{ B : has",
            "ZERO_OR_MORE",
            "ONLY_ONE",
            "NON_IDENTIFYING",
        ),
    ];

    for (line, card_a, card_b, rel_type) in cases {
        let text = format!("erDiagram\n{line}\n");
        let res = block_on(engine.parse_diagram(&text, ParseOptions::default()))
            .unwrap()
            .unwrap();
        let rels = res.model["relationships"].as_array().unwrap();
        assert_eq!(rels.len(), 1, "{line}");
        assert_eq!(rels[0]["relSpec"]["cardA"], json!(*card_a), "{line}");
        assert_eq!(rels[0]["relSpec"]["cardB"], json!(*card_b), "{line}");
        assert_eq!(rels[0]["relSpec"]["relType"], json!(*rel_type), "{line}");
    }
}

#[test]
fn parse_diagram_er_relationship_word_aliases_match_upstream_spec_minimally() {
    let engine = Engine::new();
    let cases: &[(&str, &str, &str, &str)] = &[
        (
            "A one or zero to many B : has",
            "ZERO_OR_MORE",
            "ZERO_OR_ONE",
            "IDENTIFYING",
        ),
        (
            "A one or many optionally to zero or one B : has",
            "ZERO_OR_ONE",
            "ONE_OR_MORE",
            "NON_IDENTIFYING",
        ),
        (
            "A zero or more to zero or many B : has",
            "ZERO_OR_MORE",
            "ZERO_OR_MORE",
            "IDENTIFYING",
        ),
        (
            "A many(0) to many(1) B : has",
            "ONE_OR_MORE",
            "ZERO_OR_MORE",
            "IDENTIFYING",
        ),
        (
            "A many optionally to one B : has",
            "ONLY_ONE",
            "ZERO_OR_MORE",
            "NON_IDENTIFYING",
        ),
        (
            "A only one optionally to 1+ B : has",
            "ONE_OR_MORE",
            "ONLY_ONE",
            "NON_IDENTIFYING",
        ),
        (
            "A 0+ optionally to 1 B : has",
            "ONLY_ONE",
            "ZERO_OR_MORE",
            "NON_IDENTIFYING",
        ),
        (
            "HOUSE one to one ROOM : contains",
            "ONLY_ONE",
            "ONLY_ONE",
            "IDENTIFYING",
        ),
    ];

    for (line, card_a, card_b, rel_type) in cases {
        let text = format!("erDiagram\n{line}\n");
        let res = block_on(engine.parse_diagram(&text, ParseOptions::default()))
            .unwrap()
            .unwrap();
        let rels = res.model["relationships"].as_array().unwrap();
        assert_eq!(rels.len(), 1, "{line}");
        assert_eq!(rels[0]["relSpec"]["cardA"], json!(*card_a), "{line}");
        assert_eq!(rels[0]["relSpec"]["cardB"], json!(*card_b), "{line}");
        assert_eq!(rels[0]["relSpec"]["relType"], json!(*rel_type), "{line}");
    }
}

#[test]
fn parse_diagram_er_entity_names_may_start_with_cardinality_words() {
    // Mermaid's lexer matches `many`, `one`, `to` (and the multi-word aliases) only at a word
    // boundary (`/^(?:to\b)/i`), so `tokens` or `oneshot` are plain entity names and labels.
    let engine = Engine::new();
    for name in [
        "tokens",
        "topic",
        "total",
        "toy",
        "oneshot",
        "manyToMany",
        "many_items",
        "TOTAL",
    ] {
        let text = format!("erDiagram\nA ||--o{{ {name} : has\n{name} ||--|| B : {name}\n");
        let res = block_on(engine.parse_diagram(&text, ParseOptions::strict()))
            .unwrap_or_else(|e| panic!("{name}: {e}"))
            .unwrap();
        let entities = res.model["entities"].as_object().unwrap();
        assert_eq!(entities.len(), 3, "{name}");
        assert!(entities.get(name).is_some(), "{name}");
        let rels = res.model["relationships"].as_array().unwrap();
        assert_eq!(rels.len(), 2, "{name}");
        assert_eq!(rels[1]["roleA"], json!(name), "{name}");
    }

    // A word alias followed by a name that starts with a cardinality word.
    let text = "erDiagram\nA one to many tokens : has\n";
    let res = block_on(engine.parse_diagram(text, ParseOptions::strict()))
        .unwrap()
        .unwrap();
    assert!(res.model["entities"].get("tokens").is_some());
    let rels = res.model["relationships"].as_array().unwrap();
    assert_eq!(rels[0]["relSpec"]["cardA"], json!("ZERO_OR_MORE"));
    assert_eq!(rels[0]["relSpec"]["cardB"], json!("ONLY_ONE"));
}

#[test]
fn parse_diagram_er_cardinality_suffixes_require_a_word_boundary() {
    let engine = Engine::new();
    for (cardinality, expected) in [
        ("|o", "ZERO_OR_ONE"),
        ("}o", "ZERO_OR_MORE"),
        ("one or zero", "ZERO_OR_ONE"),
        ("zero or one", "ZERO_OR_ONE"),
        ("one or more", "ONE_OR_MORE"),
        ("one or many", "ONE_OR_MORE"),
        ("zero or more", "ZERO_OR_MORE"),
        ("zero or many", "ZERO_OR_MORE"),
        ("only one", "ONLY_ONE"),
    ] {
        let text = format!("erDiagram\nA ||--{cardinality} B : has\n");
        let res = block_on(engine.parse_diagram(&text, ParseOptions::strict()))
            .unwrap_or_else(|e| panic!("{cardinality}: {e}"))
            .unwrap();
        assert_eq!(
            res.model["relationships"][0]["relSpec"]["cardA"],
            json!(expected),
            "{cardinality}"
        );
        assert!(res.model["entities"].get("B").is_some(), "{cardinality}");

        // Without a boundary, consuming the cardinality would silently create entity B.
        let text = format!("erDiagram\nA ||--{cardinality}B : has\n");
        assert!(
            block_on(engine.parse_diagram(&text, ParseOptions::strict())).is_err(),
            "{cardinality}"
        );
    }
}

#[test]
fn parse_diagram_er_cardinality_word_boundary_is_ascii() {
    // `\b` in Mermaid's (non-Unicode) regexes treats `-`, `.` and non-ASCII characters as word
    // boundaries, so these stay cardinality words and the lines are rejected, as in Mermaid.
    let engine = Engine::new();
    for line in [
        "A ||--o{ to : has",
        "A ||--o{ one : has",
        "A ||--o{ many : has",
        "A ||--o{ to注文 : has",
        "A ||--o{ B : one-to-one",
        // Used to be accepted with the entity silently renamed to `rous` / `self`.
        "A one to onerous : x",
        "A only one to oneself : has",
        "A one optionally toone B : has",
    ] {
        let text = format!("erDiagram\n{line}\n");
        assert!(
            block_on(engine.parse_diagram(&text, ParseOptions::strict())).is_err(),
            "{line}"
        );
    }
}

#[test]
fn parse_diagram_er_keeps_multi_digit_entity_after_numeric_cardinality() {
    let engine = Engine::new();
    let text = "erDiagram\na many to 1 12: label\n";
    let res = block_on(engine.parse_diagram(text, ParseOptions::strict()))
        .unwrap()
        .unwrap();

    assert!(res.model["entities"].get("a").is_some());
    assert!(res.model["entities"].get("12").is_some());
    assert_eq!(res.model["relationships"].as_array().unwrap().len(), 1);
    assert_eq!(
        res.model["relationships"][0]["relSpec"]["cardA"],
        json!("ONLY_ONE")
    );
    assert_eq!(
        res.model["relationships"][0]["relSpec"]["cardB"],
        json!("ZERO_OR_MORE")
    );
}

#[test]
fn parse_diagram_er_rejects_empty_backtick_attribute_words() {
    let engine = Engine::new();

    for attribute in ["`` name", "string ``"] {
        let text = format!("erDiagram\nBOOK {{\n  {attribute}\n}}\n");
        assert!(
            block_on(engine.parse_diagram(&text, ParseOptions::strict())).is_err(),
            "empty backtick attribute word unexpectedly parsed: {attribute}"
        );
    }
}

#[test]
fn parse_diagram_er_rejects_invalid_relationship_syntax() {
    let engine = Engine::new();
    assert!(
        block_on(engine.parse_diagram("erDiagram\nA xxx B : has\n", ParseOptions::default()))
            .is_err()
    );
}

#[test]
fn parse_diagram_er_style_statement_applies_styles() {
    let engine = Engine::new();
    let text = r#"erDiagram
CUSTOMER
style CUSTOMER color:red,stroke:blue,fill:#f9f
"#;
    let res = block_on(engine.parse_diagram(text, ParseOptions::default()))
        .unwrap()
        .unwrap();
    assert_eq!(
        res.model["entities"]["CUSTOMER"]["cssStyles"],
        json!(["color:red", "stroke:blue", "fill:#f9f"])
    );
}

#[test]
fn parse_diagram_er_style_statements_append_across_multiple_lines() {
    let engine = Engine::new();
    let text = r#"erDiagram
CUSTOMER
style CUSTOMER color:red
style CUSTOMER fill:#f9f
"#;
    let res = block_on(engine.parse_diagram(text, ParseOptions::default()))
        .unwrap()
        .unwrap();
    assert_eq!(
        res.model["entities"]["CUSTOMER"]["cssStyles"],
        json!(["color:red", "fill:#f9f"])
    );
}

#[test]
fn parse_diagram_er_class_statement_assigns_classes() {
    let engine = Engine::new();
    let text = r#"erDiagram
CUSTOMER
class CUSTOMER firstClass, secondClass, thirdClass
"#;
    let res = block_on(engine.parse_diagram(text, ParseOptions::default()))
        .unwrap()
        .unwrap();
    assert_eq!(
        res.model["entities"]["CUSTOMER"]["cssClasses"],
        json!("default firstClass secondClass thirdClass")
    );
}

#[test]
fn parse_diagram_er_class_statement_appends_across_multiple_lines() {
    let engine = Engine::new();
    let text = r#"erDiagram
CUSTOMER
class CUSTOMER firstClass
class CUSTOMER secondClass
"#;
    let res = block_on(engine.parse_diagram(text, ParseOptions::default()))
        .unwrap()
        .unwrap();
    assert_eq!(
        res.model["entities"]["CUSTOMER"]["cssClasses"],
        json!("default firstClass secondClass")
    );
}

#[test]
fn parse_diagram_er_classdef_defines_styles_and_text_styles() {
    let engine = Engine::new();
    let text = r#"erDiagram
classDef myClass fill:#f9f, stroke: red, color: pink
"#;
    let res = block_on(engine.parse_diagram(text, ParseOptions::default()))
        .unwrap()
        .unwrap();
    assert_eq!(
        res.model["classes"]["myClass"],
        json!({
            "id": "myClass",
            "styles": ["fill:#f9f", "stroke:red", "color:pink"],
            "textStyles": ["color:pink"]
        })
    );
}

#[test]
fn parse_diagram_er_classdef_supports_multiple_classes_in_one_statement() {
    let engine = Engine::new();
    let text = r#"erDiagram
classDef firstClass,secondClass fill:#f9f, stroke: red, color: pink
"#;
    let res = block_on(engine.parse_diagram(text, ParseOptions::default()))
        .unwrap()
        .unwrap();
    assert_eq!(
        res.model["classes"]["firstClass"]["styles"][1],
        json!("stroke:red")
    );
    assert_eq!(
        res.model["classes"]["secondClass"]["styles"][2],
        json!("color:pink")
    );
}

#[test]
fn parse_diagram_er_shorthand_class_assignment_variants_work() {
    let engine = Engine::new();
    let text = r#"erDiagram
CUSTOMER:::myClass
CUSTOMER2:::myClass {}
CUSTOMER3:::myClass {
  string name
}
"#;
    let res = block_on(engine.parse_diagram(text, ParseOptions::default()))
        .unwrap()
        .unwrap();
    assert_eq!(
        res.model["entities"]["CUSTOMER"]["cssClasses"],
        json!("default myClass")
    );
    assert_eq!(
        res.model["entities"]["CUSTOMER2"]["cssClasses"],
        json!("default myClass")
    );
    assert_eq!(
        res.model["entities"]["CUSTOMER3"]["cssClasses"],
        json!("default myClass")
    );
}

#[test]
fn parse_diagram_er_shorthand_assignment_supports_multiple_classes_and_alias() {
    let engine = Engine::new();
    let text = r#"erDiagram
c[CUSTOMER]:::firstClass,secondClass
"#;
    let res = block_on(engine.parse_diagram(text, ParseOptions::default()))
        .unwrap()
        .unwrap();
    assert_eq!(res.model["entities"]["c"]["alias"], json!("CUSTOMER"));
    assert_eq!(
        res.model["entities"]["c"]["cssClasses"],
        json!("default firstClass secondClass")
    );
}

#[test]
fn parse_diagram_er_shorthand_assignment_works_in_relationships() {
    let engine = Engine::new();
    let text = r#"erDiagram
CUSTOMER:::myClass ||--o{ PERSON:::myClass : allows
"#;
    let res = block_on(engine.parse_diagram(text, ParseOptions::default()))
        .unwrap()
        .unwrap();
    assert_eq!(
        res.model["entities"]["CUSTOMER"]["cssClasses"],
        json!("default myClass")
    );
    assert_eq!(
        res.model["entities"]["PERSON"]["cssClasses"],
        json!("default myClass")
    );
}

#[test]
fn parse_diagram_er_relationship_labels_allow_empty_quoted_and_unquoted() {
    let engine = Engine::new();
    let res_empty = block_on(engine.parse_diagram(
        "erDiagram\nCUSTOMER ||--|{ ORDER : \"\"\n",
        ParseOptions::default(),
    ))
    .unwrap()
    .unwrap();
    assert_eq!(res_empty.model["relationships"][0]["roleA"], json!(""));

    let res_unquoted = block_on(engine.parse_diagram(
        "erDiagram\nCUSTOMER ||--|{ ORDER : places\n",
        ParseOptions::default(),
    ))
    .unwrap()
    .unwrap();
    assert_eq!(
        res_unquoted.model["relationships"][0]["roleA"],
        json!("places")
    );
}

#[test]
fn parse_diagram_er_parent_child_relationship_sets_md_parent_cardinality() {
    let engine = Engine::new();
    let text = r#"erDiagram
PROJECT u--o{ TEAM_MEMBER : "parent"
"#;
    let res = block_on(engine.parse_diagram(text, ParseOptions::default()))
        .unwrap()
        .unwrap();
    let rel = &res.model["relationships"][0];
    assert_eq!(rel["relSpec"]["cardB"], json!("MD_PARENT"));
    assert_eq!(rel["relSpec"]["cardA"], json!("ZERO_OR_MORE"));
}

#[test]
fn parse_diagram_er_allows_prototype_like_entity_names() {
    let engine = Engine::new();
    for name in ["__proto__", "constructor", "prototype"] {
        let text = format!("erDiagram\n{name} ||--|{{ ORDER : place\n");
        assert!(block_on(engine.parse_diagram(&text, ParseOptions::default())).is_ok());
    }
}

#[test]
fn parse_diagram_er_parses_relationship_cardinality_and_type() {
    let engine = Engine::new();
    let text = r#"erDiagram
CAR ||--o{ DRIVER : "insured for"
"#;
    let res = block_on(engine.parse_diagram(text, ParseOptions::default()))
        .unwrap()
        .unwrap();

    assert_eq!(res.model["entities"].as_object().unwrap().len(), 2);
    let rels = res.model["relationships"].as_array().unwrap();
    assert_eq!(rels.len(), 1);
    assert_eq!(rels[0]["roleA"], json!("insured for"));
    assert_eq!(rels[0]["relSpec"]["cardA"], json!("ZERO_OR_MORE"));
    assert_eq!(rels[0]["relSpec"]["cardB"], json!("ONLY_ONE"));
    assert_eq!(rels[0]["relSpec"]["relType"], json!("IDENTIFYING"));
}

#[test]
fn parse_diagram_er_supports_numeric_cardinality_shorthands() {
    let engine = Engine::new();
    let text = r#"erDiagram
A 1+--0+ B : has
"#;
    let res = block_on(engine.parse_diagram(text, ParseOptions::default()))
        .unwrap()
        .unwrap();
    let rels = res.model["relationships"].as_array().unwrap();
    assert_eq!(rels.len(), 1);
    assert_eq!(rels[0]["relSpec"]["cardA"], json!("ZERO_OR_MORE"));
    assert_eq!(rels[0]["relSpec"]["cardB"], json!("ONE_OR_MORE"));
}

#[test]
fn parse_diagram_er_acc_title_and_multiline_description() {
    let engine = Engine::new();
    let text = r#"erDiagram
accTitle: graph title
accDescr { this graph is
  about
  stuff
}
A ||--o{ B : has
"#;
    let res = block_on(engine.parse_diagram(text, ParseOptions::default()))
        .unwrap()
        .unwrap();
    assert_eq!(res.model["accTitle"], json!("graph title"));
    assert_eq!(res.model["accDescr"], json!("this graph is\nabout\nstuff"));
}

#[test]
fn parse_diagram_er_multibyte_attribute_does_not_panic() {
    let engine = Engine::new();
    let text = "erDiagram\n顧客 {\n  文字列 名前\n}\n";
    let res = block_on(engine.parse_diagram(text, ParseOptions::default()))
        .unwrap()
        .unwrap();
    let attr = &res.model["entities"]["顧客"]["attributes"][0];
    assert_eq!(attr["type"], json!("文字列"));
    assert_eq!(attr["name"], json!("名前"));
}

#[test]
fn parse_er_editor_facts_preserve_parser_symbol_spans() {
    let engine = Engine::new();
    let text = r#"erDiagram
CUSTOMER ||--o{ ORDER : places
CUSTOMER {
  string customer_id PK, FK "primary key"
}
ORDER:::important
class CUSTOMER vip
classDef important fill:#f9f
style ORDER fill:#eee
"#;
    let facts = engine
        .parse_editor_semantic_facts_with_type_sync("er", text)
        .unwrap()
        .expect("er editor facts");

    assert_eq!(facts.completeness, EditorSemanticCompleteness::Complete);

    let symbol_at = |name: &str, start: usize| {
        facts
            .symbols
            .iter()
            .find(|symbol| symbol.name == name && symbol.selection.start == start)
            .unwrap_or_else(|| panic!("missing symbol {name} at {start}"))
    };

    let customer_start = text.find("CUSTOMER").unwrap();
    let first_customer = symbol_at("CUSTOMER", customer_start);
    assert_eq!(
        first_customer.selection.end,
        customer_start + "CUSTOMER".len()
    );
    assert_eq!(first_customer.role, EditorSemanticRole::Entity);
    assert_eq!(first_customer.detail.as_deref(), Some("er implicit entity"));

    let order_start = text.find("ORDER :").unwrap();
    let first_order = symbol_at("ORDER", order_start);
    assert_eq!(first_order.selection.end, order_start + "ORDER".len());
    assert_eq!(first_order.role, EditorSemanticRole::Entity);
    assert_eq!(first_order.detail.as_deref(), Some("er implicit entity"));

    let declared_customer_start = text.find("CUSTOMER {").unwrap();
    assert_eq!(
        symbol_at("CUSTOMER", declared_customer_start).role,
        EditorSemanticRole::Entity
    );

    let attribute_start = text.find("customer_id").unwrap();
    let attribute = symbol_at("customer_id", attribute_start);
    assert_eq!(
        attribute.selection.end,
        attribute_start + "customer_id".len()
    );
    assert_eq!(attribute.role, EditorSemanticRole::Outline);

    let type_start = text.find("string customer_id").unwrap();
    let ty = symbol_at("string", type_start);
    assert_eq!(ty.role, EditorSemanticRole::Payload);
    assert_eq!(ty.detail.as_deref(), Some("er attribute type"));

    let pk_start = text.find("PK").unwrap();
    let pk = symbol_at("PK", pk_start);
    assert_eq!(pk.role, EditorSemanticRole::Payload);
    assert_eq!(pk.detail.as_deref(), Some("er attribute key"));

    let fk_start = text.find("FK").unwrap();
    let fk = symbol_at("FK", fk_start);
    assert_eq!(fk.role, EditorSemanticRole::Payload);
    assert_eq!(fk.detail.as_deref(), Some("er attribute key"));

    let comment_start = text.find("\"primary key\"").unwrap();
    let comment = symbol_at("primary key", comment_start + 1);
    assert_eq!(comment.role, EditorSemanticRole::Payload);
    assert_eq!(comment.detail.as_deref(), Some("er attribute comment"));
    assert_eq!(comment.span.start, comment_start);
    assert_eq!(comment.selection.start, comment_start + 1);
    assert_eq!(
        comment.selection.end,
        comment_start + "\"primary key\"".len() - 1
    );

    let important_start = text.find("important").unwrap();
    assert_eq!(
        symbol_at("important", important_start).selection.end,
        important_start + "important".len()
    );

    let class_definition_start = text.rfind("important").unwrap();
    let class_definition = symbol_at("important", class_definition_start);
    assert_eq!(class_definition.role, EditorSemanticRole::ClassDefinition);
    assert_eq!(
        class_definition.detail.as_deref(),
        Some("er class definition")
    );

    let inline_class_start = text.find("important").unwrap();
    let inline_class = symbol_at("important", inline_class_start);
    assert_eq!(inline_class.role, EditorSemanticRole::Payload);
    assert_eq!(inline_class.detail.as_deref(), Some("er inline class"));

    let class_target_start = text.find("class CUSTOMER").unwrap() + "class ".len();
    let class_target = symbol_at("CUSTOMER", class_target_start);
    assert_eq!(class_target.role, EditorSemanticRole::Reference);
    assert_eq!(class_target.detail.as_deref(), Some("er class target"));

    let class_name_start = text.find("class CUSTOMER").unwrap() + "class CUSTOMER ".len();
    let class_name = symbol_at("vip", class_name_start);
    assert_eq!(class_name.role, EditorSemanticRole::Payload);
    assert_eq!(class_name.detail.as_deref(), Some("er class name"));

    let style_target_start = text.find("style ORDER").unwrap() + "style ".len();
    let style_target = symbol_at("ORDER", style_target_start);
    assert_eq!(style_target.role, EditorSemanticRole::Reference);
    assert_eq!(style_target.detail.as_deref(), Some("er style target"));

    assert!(facts.directive_prefixes.iter().any(|p| p == "class"));
    assert!(facts.directive_prefixes.iter().any(|p| p == "classDef"));
    assert!(facts.directive_prefixes.iter().any(|p| p == "style"));
}

#[test]
fn parse_er_editor_facts_distinguish_declarations_implicit_entities_and_references() {
    let engine = Engine::new();
    let text = concat!(
        "erDiagram\n",
        "KNOWN\n",
        "KNOWN ||--o{ LATER : first\n",
        "KNOWN:::hot ||--|| LATER:::cold : second\n",
        "LATER {\n",
        "  string id\n",
        "}\n",
    );
    let facts = engine
        .parse_editor_semantic_facts_with_type_sync("er", text)
        .unwrap()
        .expect("er editor facts");
    let symbols_for = |name: &str| {
        facts
            .symbols
            .iter()
            .filter(|symbol| symbol.name == name && symbol.kind == EditorSemanticKind::Struct)
            .collect::<Vec<_>>()
    };

    let known = symbols_for("KNOWN");
    assert_eq!(known.len(), 3);
    assert_eq!(known[0].role, EditorSemanticRole::Entity);
    assert!(
        known[1..]
            .iter()
            .all(|symbol| symbol.role == EditorSemanticRole::Reference)
    );

    let later = symbols_for("LATER");
    assert_eq!(later.len(), 3);
    assert_eq!(later[0].role, EditorSemanticRole::Entity);
    assert_eq!(later[0].detail.as_deref(), Some("er implicit entity"));
    assert_eq!(later[1].role, EditorSemanticRole::Reference);
    assert_eq!(later[2].role, EditorSemanticRole::Entity);
}

#[test]
fn parse_er_editor_facts_preserve_mermaid_11_16_attribute_spans() {
    let engine = Engine::new();
    let text = r#"erDiagram
PLACE {
  `public.geometry(point,4326)` `display name`
  string? nickname
}
"#;
    let facts = engine
        .parse_editor_semantic_facts_with_type_sync("er", text)
        .unwrap()
        .expect("er editor facts");

    assert_eq!(facts.completeness, EditorSemanticCompleteness::Complete);

    let symbol_at = |name: &str, start: usize| {
        facts
            .symbols
            .iter()
            .find(|symbol| symbol.name == name && symbol.selection.start == start)
            .unwrap_or_else(|| panic!("missing symbol {name} at {start}"))
    };

    let geometry_start = text.find("public.geometry").unwrap();
    let geometry = symbol_at("public.geometry(point,4326)", geometry_start);
    assert_eq!(geometry.detail.as_deref(), Some("er attribute type"));
    assert_eq!(
        geometry.selection.end,
        geometry_start + "public.geometry(point,4326)".len()
    );

    let display_name_start = text.find("display name").unwrap();
    let display_name = symbol_at("display name", display_name_start);
    assert_eq!(display_name.detail.as_deref(), Some("er attribute"));
    assert_eq!(
        display_name.selection.end,
        display_name_start + "display name".len()
    );

    let nullable_type_start = text.find("string?").unwrap();
    let nullable_type = symbol_at("string", nullable_type_start);
    assert_eq!(nullable_type.detail.as_deref(), Some("er attribute type"));

    let nickname_start = text.find("nickname").unwrap();
    let nickname = symbol_at("nickname", nickname_start);
    assert_eq!(nickname.detail.as_deref(), Some("er attribute"));
}

#[test]
fn parse_er_editor_facts_recovers_from_incomplete_input() {
    let engine = Engine::new();
    let text = "erDiagram\nCUSTOMER ||--o{ ORDER : places\nCUSTOMER {";
    let facts = engine
        .parse_editor_semantic_facts_with_type_sync("er", text)
        .unwrap()
        .expect("er editor facts");

    assert_eq!(facts.completeness, EditorSemanticCompleteness::Recovered);
    assert!(facts.symbols.iter().any(|symbol| symbol.name == "CUSTOMER"));
    assert!(facts.symbols.iter().any(|symbol| symbol.name == "ORDER"));
}

#[test]
fn parse_er_editor_facts_record_expected_class_name_spans() {
    let engine = Engine::new();
    let text = "erDiagram\nclassDef pink fill:#f9f\n";
    let facts = engine
        .parse_editor_semantic_facts_with_type_sync("er", text)
        .unwrap()
        .expect("er editor facts");

    assert!(facts.expected_syntax.iter().any(|expected| {
        expected.kind == EditorExpectedSyntaxKind::ClassName
            && expected.span.start == text.find("pink").unwrap()
    }));
}

#[test]
fn parse_er_editor_facts_do_not_consume_classdef_names_across_line_endings() {
    let engine = Engine::new();

    for line_ending in ["\n", "\r", "\r\n"] {
        let text = [
            "erDiagram",
            "classDef",
            "CUSTOMER ||--o{ ORDER : places",
            "",
        ]
        .join(line_ending);
        let facts = engine
            .parse_editor_semantic_facts_with_type_sync("er", &text)
            .unwrap()
            .expect("er editor facts");

        assert!(
            !facts.symbols.iter().any(|symbol| {
                symbol.name == "CUSTOMER" && symbol.role == EditorSemanticRole::ClassDefinition
            }),
            "classDef consumed the next physical line for {line_ending:?}"
        );
    }
}

#[test]
fn parse_er_editor_facts_preserve_every_crlf_unicode_occurrence_span() {
    let engine = Engine::new();
    let text = concat!(
        "---\r\n",
        "config:\r\n",
        "  theme: dark\r\n",
        "---\r\n",
        "%%{init: {\"theme\": \"default\"}}%%\r\n",
        "erDiagram\r\n",
        "孤島\r\n",
        "顧客[\"客戶別名\"] {\r\n",
        "  文字列 名稱 PK\r\n",
        "  文字列 名稱 FK\r\n",
        "}\r\n",
        "顧客 ||--o{ 訂單 : places 群島\r\n",
        "顧客 ||--|| 顧客 : refers\r\n",
        "class 顧客 important\r\n",
    );
    let facts = engine
        .parse_editor_semantic_facts_with_type_sync("er", text)
        .unwrap()
        .expect("ER editor facts");

    assert_eq!(facts.completeness, EditorSemanticCompleteness::Complete);
    for name in ["孤島", "顧客", "名稱", "群島"] {
        let expected = text
            .match_indices(name)
            .map(|(start, value)| SourceSpan::new(start, start + value.len()))
            .collect::<Vec<_>>();
        let actual = facts
            .symbols
            .iter()
            .filter(|symbol| symbol.name == name)
            .map(|symbol| symbol.selection)
            .collect::<Vec<_>>();
        assert_eq!(
            actual, expected,
            "ER lost or collapsed a {name:?} occurrence"
        );
    }
    let payload = |name: &str, detail: &str| {
        facts
            .symbols
            .iter()
            .find(|symbol| symbol.name == name && symbol.detail.as_deref() == Some(detail))
            .unwrap_or_else(|| panic!("missing ER payload {name:?} ({detail})"))
    };

    let alias_start = text.find("客戶別名").unwrap();
    let alias = payload("客戶別名", "er entity alias");
    assert_eq!(alias.role, EditorSemanticRole::Payload);
    assert_eq!(alias.kind, EditorSemanticKind::String);
    assert_eq!(
        alias.span,
        SourceSpan::new(alias_start - 1, alias.selection.end + 1)
    );
    assert_eq!(
        alias.selection,
        SourceSpan::new(alias_start, alias_start + "客戶別名".len())
    );

    for role_name in ["places", "refers"] {
        let start = text.find(role_name).unwrap();
        let role = payload(role_name, "er relationship role");
        assert_eq!(role.role, EditorSemanticRole::Payload);
        assert_eq!(role.kind, EditorSemanticKind::String);
        assert_eq!(role.span, SourceSpan::new(start, start + role_name.len()));
        assert_eq!(role.selection, role.span);
    }
}

#[test]
fn parse_er_editor_recovery_reuses_one_lexical_event_stream_and_reports_exact_span() {
    let engine = Engine::new();
    let text = concat!(
        "---\r\n",
        "config:\r\n",
        "  theme: dark\r\n",
        "---\r\n",
        "%%{init: {\"theme\": \"default\"}}%%\r\n",
        "erDiagram\r\n",
        "顧客 ||--o{ 訂單 : places\r\n",
        "@",
    );
    let invalid_start = text.find('@').unwrap();
    crate::diagrams::er::reset_er_syntax_construction_count();

    let facts = engine
        .parse_editor_semantic_facts_with_type_sync("er", text)
        .unwrap()
        .expect("ER recovery facts");

    assert_eq!(facts.completeness, EditorSemanticCompleteness::Recovered);
    assert_eq!(crate::diagrams::er::er_syntax_construction_count(), 1);
    assert!(facts.diagnostics.iter().any(|diagnostic| {
        diagnostic.kind == EditorSemanticDiagnosticKind::ParserRecovery
            && diagnostic.span == Some(SourceSpan::new(invalid_start, invalid_start + 1))
    }));

    let error = engine
        .parse_diagram_sync(text, ParseOptions::strict())
        .expect_err("strict ER parsing rejects the invalid token");
    let Error::DiagramParse { diagnostic, .. } = error else {
        panic!("invalid ER token returned a non-parse error");
    };
    assert_eq!(
        diagnostic.span(),
        Some(SourceSpan::new(invalid_start, invalid_start + 1))
    );
    assert_eq!(diagnostic.span_kind(), ParseDiagnosticSpanKind::Exact);
}

#[test]
fn parse_diagram_er_keyword_boundaries_match_ascii_word_characters() {
    let engine = Engine::new();
    // Mermaid 12's generated Jison rules end keywords with non-Unicode `\b`.
    for name in [
        "end-user",
        "style-guide",
        "end注文",
        "END-user",
        "class-guide",
        "classDef注文",
    ] {
        let source = format!("erDiagram\n{name}\n");
        assert!(
            engine
                .parse_diagram_sync(&source, ParseOptions::strict())
                .is_err(),
            "{name}"
        );
        let quoted = format!("erDiagram\n\"{name}\"\n");
        let parsed = engine
            .parse_diagram_sync(&quoted, ParseOptions::strict())
            .unwrap()
            .unwrap();
        assert!(parsed.model["entities"].get(name).is_some(), "{name}");
    }
    for name in [
        "ending",
        "end_注文",
        "style_guide",
        "classDefinition",
        "subgraph2",
    ] {
        let source = format!("erDiagram\n{name}\n");
        let parsed = engine
            .parse_diagram_sync(&source, ParseOptions::strict())
            .unwrap()
            .unwrap();
        assert!(parsed.model["entities"].get(name).is_some(), "{name}");
    }
}

#[test]
fn parse_diagram_er_direction_is_an_entity_without_a_direction_rule_match() {
    let engine = Engine::new();
    for source in [
        "erDiagram\ndirection\n",
        "erDiagram\ndirection {}\n",
        "erDiagram\ndirection sideways\n",
        "erDiagram\nA||--||direction: owns\n",
    ] {
        let parsed = engine
            .parse_diagram_sync(source, ParseOptions::strict())
            .unwrap()
            .unwrap();
        assert!(
            parsed.model["entities"].get("direction").is_some(),
            "{source}"
        );
        assert_eq!(parsed.model["direction"], json!("TB"));
    }
    for (statement, expected) in [
        ("direction LR", "LR"),
        ("DiReCtIoN\tbt", "BT"),
        ("direction\u{00a0}RL", "RL"),
        ("direction\nLR", "LR"),
        ("direction LRtail", "LR"),
        ("xdirection BT", "BT"),
        ("direction RL direction TB", "TB"),
        ("direction\u{feff}BT", "BT"),
    ] {
        let source = format!("erDiagram\n{statement}\nA\n");
        let parsed = engine
            .parse_diagram_sync(&source, ParseOptions::strict())
            .unwrap()
            .unwrap();
        assert_eq!(parsed.model["direction"], json!(expected), "{statement}");
        assert_eq!(
            parsed.model["entities"].as_object().unwrap().len(),
            1,
            "{statement}"
        );
    }
}

#[test]
fn parse_diagram_er_parent_cardinality_does_not_require_leading_whitespace() {
    let engine = Engine::new();
    for name in ["u-table", "U-table", "u.table"] {
        for indent in ["", " ", "\t"] {
            let source = format!("erDiagram\n{indent}{name}\n");
            assert!(
                engine
                    .parse_diagram_sync(&source, ParseOptions::strict())
                    .is_err(),
                "{source}"
            );
        }
        let source = format!("erDiagram\n\"{name}\"\n");
        assert!(
            engine
                .parse_diagram_sync(&source, ParseOptions::strict())
                .unwrap()
                .unwrap()
                .model["entities"]
                .get(name)
                .is_some()
        );
    }
    for marker in ["u", "U"] {
        let source = format!("erDiagram\n\"A\"{marker}--o{{ B : owns\n");
        let parsed = engine
            .parse_diagram_sync(&source, ParseOptions::strict())
            .unwrap()
            .unwrap();
        assert_eq!(
            parsed.model["relationships"][0]["relSpec"]["cardB"],
            json!("MD_PARENT")
        );
    }
}

#[test]
fn parse_er_editor_direction_selection_tracks_the_matched_rule() {
    let engine = Engine::new();
    for (statement, value) in [
        ("direction RL direction TB", "TB"),
        ("xdirection BT", "BT"),
        ("direction\u{00a0}RL", "RL"),
        ("direction\nLR", "LR"),
        ("direction LRtail", "LR"),
    ] {
        let source = format!("erDiagram\n{statement}\n");
        let facts = engine
            .parse_editor_semantic_facts_with_type_sync("er", &source)
            .unwrap()
            .unwrap();
        assert_eq!(facts.completeness, EditorSemanticCompleteness::Complete);
        let selected = facts
            .expected_syntax
            .iter()
            .filter(|expected| expected.kind == EditorExpectedSyntaxKind::CardinalDirectionValue)
            .collect::<Vec<_>>();
        assert_eq!(selected.len(), 1, "{statement}");
        let start = source.rfind(value).unwrap();
        assert_eq!(
            selected[0].span,
            SourceSpan::new(start, start + value.len()),
            "{statement}"
        );
    }
}

#[test]
fn parse_diagram_er_requires_style_payload_at_eof_or_newline() {
    let engine = Engine::new();
    for statement in ["style A", "classDef C", "style-guide", "classDef注文"] {
        for suffix in ["", "\n", " ;", " ;\n"] {
            let source = format!("erDiagram\n{statement}{suffix}");
            assert!(
                engine
                    .parse_diagram_sync(&source, ParseOptions::strict())
                    .is_err(),
                "{source}"
            );
        }
    }
}

#[test]
fn parse_diagram_er_accessibility_direction_uses_raw_cursor() {
    let engine = Engine::new();
    for keyword in ["accTitle:", "accDescr:"] {
        for prefix in ["", "  ", "\t", "\u{00a0}", "\u{feff}"] {
            let source = format!("erDiagram\n{prefix}{keyword} direction LR\nA\n");
            let parsed = engine
                .parse_diagram_sync(&source, ParseOptions::default())
                .unwrap()
                .unwrap();
            assert_eq!(
                parsed.model["direction"],
                if prefix.is_empty() { "TB" } else { "LR" },
                "{source}"
            );
            let field = if keyword == "accTitle:" {
                "accTitle"
            } else {
                "accDescr"
            };
            if prefix.is_empty() {
                assert_eq!(parsed.model[field], "direction LR", "{source}");
            } else {
                assert!(parsed.model[field].is_null(), "{source}");
                let facts = engine
                    .parse_editor_semantic_facts_with_type_sync("er", &source)
                    .unwrap()
                    .unwrap();
                assert_eq!(
                    facts.completeness,
                    EditorSemanticCompleteness::Complete,
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
    }
}

#[test]
fn parse_diagram_er_accessibility_requires_complete_prefix() {
    let engine = Engine::new();
    for name in ["accTitle", "accDescr"] {
        let source = format!("erDiagram\n{name}\n");
        let parsed = engine
            .parse_diagram_sync(&source, ParseOptions::default())
            .unwrap()
            .unwrap();
        assert!(parsed.model["entities"].get(name).is_some(), "{source}");
        let facts = engine
            .parse_editor_semantic_facts_with_type_sync("er", &source)
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
    let source = "erDiagram\naccDescr nonsense direction LR: label\n";
    let parsed = engine
        .parse_diagram_sync(source, ParseOptions::default())
        .unwrap()
        .unwrap();
    assert_eq!(parsed.model["direction"], "LR");
    assert!(parsed.model["accDescr"].is_null());
    for (keyword, field) in [("accTitle", "accTitle"), ("accDescr", "accDescr")] {
        for (before_colon, after_colon) in [("\u{feff}", "\u{feff}"), ("", "\n"), ("\n", " ")] {
            let source = format!("erDiagram\n{keyword}{before_colon}:{after_colon}Heading\n");
            let parsed = engine
                .parse_diagram_sync(&source, ParseOptions::default())
                .unwrap()
                .unwrap();
            assert_eq!(parsed.model[field], "Heading", "{source}");
            assert!(
                parsed.model["entities"].get("Heading").is_none(),
                "{source}"
            );
        }
    }
}

#[test]
fn parse_diagram_er_accessibility_multiline_uses_ecmascript_trim() {
    let engine = Engine::new();
    for (body, expected) in [
        ("\u{feff}Heading\u{feff}", "Heading"),
        ("\u{0085}Heading\u{0085}", "\u{0085}Heading\u{0085}"),
    ] {
        let source = format!("erDiagram\naccDescr {{{body}}}\nA\n");
        let parsed = engine
            .parse_diagram_sync(&source, ParseOptions::default())
            .unwrap()
            .unwrap();
        assert_eq!(parsed.model["accDescr"], expected, "{source}");
    }
}

#[test]
fn parse_diagram_er_nested_relationships_preserve_completion_and_declaration_order() {
    let source = concat!(
        "erDiagram\n",
        "direction LR\n",
        "subgraph Outer [Outer Domain]\n",
        "PRE\n",
        "subgraph Inner [Inner Domain]\n",
        "direction TB\n",
        "A ||--|| B : inner_relation\n",
        "end\n",
        "PRE ||--|| C : outer_relation\n",
        "Inner ||--|| C : group_relation\n",
        "style Inner fill:#eee\n",
        "direction BT\n",
        "end\n",
        "B ||--|| D : cross_relation\n",
    );
    let parsed = Engine::new()
        .parse_diagram_sync(source, ParseOptions::strict())
        .unwrap()
        .unwrap();
    let model = &parsed.model;
    assert_eq!(model["direction"], "LR");
    assert_eq!(model["subgraphs"][0]["id"], "Inner");
    assert_eq!(model["subgraphs"][0]["nodes"], json!(["A", "B"]));
    assert_eq!(model["subgraphs"][0]["dir"], "TB");
    assert_eq!(model["subgraphs"][0]["cssStyles"], json!(["fill:#eee"]));
    assert_eq!(model["subgraphs"][1]["id"], "Outer");
    assert_eq!(model["subgraphs"][1]["nodes"], json!(["PRE", "Inner", "C"]));
    assert_eq!(model["subgraphs"][1]["dir"], "BT");
    let entities = model["entities"].as_object().unwrap();
    assert!(
        entities
            .keys()
            .map(String::as_str)
            .eq(["PRE", "A", "B", "C", "D"])
    );
    assert_eq!(entities["PRE"]["id"], "entity-PRE-0");
    let relationships = model["relationships"].as_array().unwrap();
    assert!(
        relationships
            .iter()
            .map(|relationship| relationship["roleA"].as_str().unwrap())
            .eq([
                "inner_relation",
                "outer_relation",
                "group_relation",
                "cross_relation",
            ])
    );
    assert_eq!(relationships[2]["entityA"], "Inner");
    assert_eq!(relationships[3]["entityA"], "entity-B-2");
}

#[test]
fn parse_diagram_er_shared_members_belong_to_the_first_completed_subgraph() {
    let source = concat!(
        "erDiagram\n",
        "subgraph Outer\n",
        "A\n",
        "subgraph First\nA\nB\nend\n",
        "A\nC\n",
        "subgraph Empty\nend\n",
        "end\n",
        "subgraph Peer\nA\nB\nC\nD\nFirst\nFirst\nend\n",
    );
    let parsed = Engine::new()
        .parse_diagram_sync(source, ParseOptions::strict())
        .unwrap()
        .unwrap();
    let subgraphs = parsed.model["subgraphs"].as_array().unwrap();
    assert!(
        subgraphs
            .iter()
            .map(|subgraph| subgraph["id"].as_str().unwrap())
            .eq(["First", "Empty", "Outer", "Peer",])
    );
    assert_eq!(subgraphs[0]["nodes"], json!(["A", "B"]));
    assert_eq!(subgraphs[1]["nodes"], json!([]));
    assert_eq!(subgraphs[2]["nodes"], json!(["First", "C", "Empty"]));
    assert_eq!(subgraphs[3]["nodes"], json!(["D"]));
}

#[test]
fn parse_diagram_er_repeated_subgraph_ids_keep_records_and_update_the_latest_lookup() {
    let source = concat!(
        "erDiagram\n",
        "subgraph Same [Outer]\nA\n",
        "subgraph Same [Inner]\nB\nend\n",
        "class Same inner_class\n",
        "style Same fill:#abc\n",
        "A ||--|| Same : before_outer_completion\n",
        "end\n",
        "class Same outer_class\n",
        "style Same stroke:#def\n",
        "subgraph Same [Sibling]\nB\nC\nend\n",
        "class Same last_class\n",
        "style Same color:#123\n",
        "Same ||--|| X : after_last_completion\n",
    );
    let parsed = Engine::new()
        .parse_diagram_sync(source, ParseOptions::strict())
        .unwrap()
        .unwrap();
    let subgraphs = parsed.model["subgraphs"].as_array().unwrap();
    assert_eq!(subgraphs.len(), 3);
    for subgraph in subgraphs {
        assert_eq!(subgraph["id"], "Same");
    }
    assert_eq!(subgraphs[0]["title"], "Inner");
    assert_eq!(subgraphs[0]["nodes"], json!(["B"]));
    assert_eq!(subgraphs[0]["classes"], json!(["inner_class"]));
    assert_eq!(subgraphs[0]["cssStyles"], json!(["fill:#abc"]));
    assert_eq!(subgraphs[1]["title"], "Outer");
    assert_eq!(subgraphs[1]["nodes"], json!(["A", "Same"]));
    assert_eq!(subgraphs[1]["classes"], json!(["outer_class"]));
    assert_eq!(subgraphs[1]["cssStyles"], json!(["stroke:#def"]));
    assert_eq!(subgraphs[2]["title"], "Sibling");
    assert_eq!(subgraphs[2]["nodes"], json!(["C"]));
    assert_eq!(subgraphs[2]["classes"], json!(["last_class"]));
    assert_eq!(subgraphs[2]["cssStyles"], json!(["color:#123"]));
    assert_eq!(parsed.model["relationships"][0]["entityB"], "Same");
    assert_eq!(parsed.model["relationships"][1]["entityA"], "Same");
}

#[test]
fn parse_diagram_er_empty_subgraph_and_trimmed_member_deduplication_match_upstream() {
    let source = concat!(
        "erDiagram\n",
        "subgraph Empty\nend\n",
        "subgraph Members\n\" A \"\nA\nend\n",
        "Empty ||--|| Members : groups\n",
    );
    let parsed = Engine::new()
        .parse_diagram_sync(source, ParseOptions::strict())
        .unwrap()
        .unwrap();
    assert_eq!(parsed.model["subgraphs"][0]["nodes"], json!([]));
    assert!(parsed.model["subgraphs"][0]["dir"].is_null());
    assert_eq!(parsed.model["subgraphs"][1]["nodes"], json!([" A "]));
    assert_eq!(parsed.model["relationships"][0]["entityA"], "Empty");
    assert_eq!(parsed.model["relationships"][0]["entityB"], "Members");
}

#[test]
fn parse_er_missing_outer_end_preserves_recovery_facts_and_eof_span() {
    let source = "erDiagram\nsubgraph Outer\nsubgraph Inner\n顧客\nend\n";
    let engine = Engine::new();
    let error = engine
        .parse_diagram_sync(source, ParseOptions::strict())
        .unwrap_err();
    let Error::DiagramParse { diagnostic, .. } = error else {
        panic!("missing end must remain an ER parse error");
    };
    assert_eq!(
        diagnostic.span(),
        Some(SourceSpan::new(source.len(), source.len()))
    );
    let facts = engine
        .parse_editor_semantic_facts_with_type_sync("er", source)
        .unwrap()
        .unwrap();
    assert_eq!(facts.completeness, EditorSemanticCompleteness::Recovered);
    let entity_start = source.find("顧客").unwrap();
    assert!(facts.symbols.iter().any(|symbol| symbol.name == "顧客"
        && symbol.selection == SourceSpan::new(entity_start, entity_start + "顧客".len())));
    assert!(
        facts
            .diagnostics
            .iter()
            .any(|diagnostic| diagnostic.span == Some(SourceSpan::new(source.len(), source.len())))
    );
}
