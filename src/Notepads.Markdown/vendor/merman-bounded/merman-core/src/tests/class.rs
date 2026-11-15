use crate::*;
use futures::executor::block_on;
use serde_json::json;

#[test]
fn class_namespace_carriers_preserve_entry_completion_and_source_order() {
    let source = concat!(
        "classDiagram\n",
        "namespace Outer {\n",
        "class Before\n",
        "namespace Inner {\n",
        "class Nested\n",
        "note for Nested \"inside\"\n",
        "}\n",
        "class After\n",
        "note \"outer\"\n",
        "}\n",
        "class Outside\n",
    );
    let parsed = Engine::new()
        .parse_diagram_sync(source, ParseOptions::strict())
        .unwrap()
        .unwrap();
    let classes = &parsed.model["classes"];
    for id in ["Before", "After"] {
        assert_eq!(classes[id]["parent"], "Outer");
    }
    assert_eq!(classes["Nested"]["parent"], "Outer.Inner");
    assert_eq!(classes["Outside"]["parent"], json!(null));
    assert_eq!(
        parsed.model["namespaces"]["Outer"]["classIds"],
        json!(["Before", "After"])
    );
    assert_eq!(
        parsed.model["namespaces"]["Outer.Inner"]["classIds"],
        json!(["Nested"])
    );
    assert_eq!(parsed.model["notes"][0]["parent"], "Outer.Inner");
    assert_eq!(parsed.model["notes"][1]["parent"], "Outer");
    assert_eq!(parsed.model["notes"][0]["text"], "inside");
    assert_eq!(parsed.model["notes"][1]["text"], "outer");
}

#[test]
fn class_namespace_carrier_failures_preserve_eof_and_first_error_recovery() {
    let engine = Engine::new();
    let incomplete = "classDiagram\nnamespace Outer {\nnamespace Inner {\nclass Leaf\n}\n";
    let unexpected = "classDiagram\n}\nnamespace Later {\nclass Leaf\n";
    for (source, span, kind) in [
        (
            incomplete,
            SourceSpan::new(incomplete.len(), incomplete.len()),
            ParseDiagnosticSpanKind::InsertionPoint,
        ),
        (
            unexpected,
            SourceSpan::new("classDiagram\n".len(), "classDiagram\n}".len()),
            ParseDiagnosticSpanKind::Exact,
        ),
    ] {
        let error = engine
            .parse_diagram_sync(source, ParseOptions::strict())
            .expect_err("malformed Class namespace is rejected");
        let Error::DiagramParse { diagnostic, .. } = error else {
            panic!("Class grammar failure must remain a parse error");
        };
        assert_eq!(diagnostic.span(), Some(span));
        assert_eq!(diagnostic.span_kind(), kind);
        let facts = engine
            .parse_editor_semantic_facts_with_type_sync("classDiagram", source)
            .unwrap()
            .unwrap();
        assert_eq!(facts.completeness, EditorSemanticCompleteness::Recovered);
        assert!(facts.symbols.iter().any(|symbol| symbol.name == "Leaf"));
        assert!(facts.diagnostics.iter().any(|diagnostic| {
            diagnostic.kind == EditorSemanticDiagnosticKind::ParserRecovery
                && diagnostic.span == Some(span)
        }));
    }
}

#[test]
fn parse_diagram_class_text_label_member_annotation_and_css_classes() {
    let engine = Engine::new();
    let text = r#"classDiagram
class C1["Class 1 with text label"]
<<interface>> C1
C1: member1
cssClass "C1" styleClass
"#;

    let res = block_on(engine.parse_diagram(text, ParseOptions::default()))
        .unwrap()
        .unwrap();
    assert_eq!(res.meta.diagram_type, "classDiagram");

    let c1 = &res.model["classes"]["C1"];
    assert_eq!(c1["label"], json!("Class 1 with text label"));
    assert_eq!(c1["cssClasses"], json!("default styleClass"));
    assert_eq!(c1["annotations"][0], json!("interface"));
    assert_eq!(c1["members"][0]["displayText"], json!("member1"));
}

#[test]
fn parse_diagram_class_supports_inline_annotation_after_class_identifier() {
    let engine = Engine::new();
    let text = "classDiagram\nclass Shape <<interface>>\n";

    let res = block_on(engine.parse_diagram(text, ParseOptions::strict()))
        .unwrap()
        .unwrap();

    assert_eq!(
        res.model["classes"]["Shape"]["annotations"],
        json!(["interface"])
    );

    let facts = engine
        .parse_editor_semantic_facts_with_type_sync("class", text)
        .unwrap()
        .expect("class editor facts");
    assert_eq!(facts.completeness, EditorSemanticCompleteness::Complete);
    assert!(facts.symbols.iter().any(|symbol| {
        symbol.name == "interface"
            && symbol.role == EditorSemanticRole::Payload
            && symbol.detail.as_deref() == Some("class annotation")
    }));

    let with_members = "classDiagram\nclass Shape <<interface>> {\n  noOfVertices\n  draw()\n}\n";
    let res = block_on(engine.parse_diagram(with_members, ParseOptions::strict()))
        .unwrap()
        .unwrap();
    let shape = &res.model["classes"]["Shape"];
    assert_eq!(shape["annotations"], json!(["interface"]));
    assert_eq!(shape["members"].as_array().unwrap().len(), 1);
    assert_eq!(shape["methods"].as_array().unwrap().len(), 1);
}

#[test]
fn parse_diagram_class_multibyte_name_does_not_panic() {
    let engine = Engine::new();
    let text = r#"classDiagram
class 顧客
"#;

    let res = block_on(engine.parse_diagram(text, ParseOptions::default()))
        .unwrap()
        .unwrap();
    assert!(res.model["classes"]["顧客"].is_object());
}

#[test]
fn parse_diagram_class_accepts_ecmascript_unicode_whitespace() {
    let engine = Engine::new();
    for whitespace in ['\u{00A0}', '\u{202F}', '\u{3000}', '\u{FEFF}'] {
        for newline in ["\n", "\r\n"] {
            let text = format!(
                "classDiagram{newline}direction{whitespace}LR{whitespace}{newline}\
                 class{whitespace}顾客{whitespace}{newline}\
                 class{whitespace}订单{newline}顾客{whitespace}-->{whitespace}订单{newline}"
            );
            let res = engine
                .parse_diagram_sync(&text, ParseOptions::default())
                .unwrap()
                .unwrap();
            assert!(res.model["classes"]["顾客"].is_object(), "{text:?}");
            assert!(res.model["classes"]["订单"].is_object(), "{text:?}");
            assert_eq!(res.model["direction"], "LR");
            assert_eq!(res.model["relations"].as_array().unwrap().len(), 1);
        }
    }
}

#[test]
fn parse_diagram_class_rejects_characters_outside_mermaid_unicode_text() {
    let engine = Engine::new();
    for name in ["𠀀", "顾客１", "顾客、", "A\u{0345}", "😀"] {
        let text = format!("classDiagram\nclass {name}\n");
        assert!(
            engine
                .parse_diagram_sync(&text, ParseOptions::default())
                .is_err(),
            "{text:?}"
        );
    }
}

#[test]
fn parse_diagram_class_preserves_unicode_in_quoted_names_and_generics() {
    let engine = Engine::new();
    let text = "classDiagram\nclass `𠀀、１`\nclass Box~𠀀、１~\n";
    let res = engine
        .parse_diagram_sync(text, ParseOptions::default())
        .unwrap()
        .unwrap();
    assert!(res.model["classes"]["𠀀、１"].is_object());
    assert_eq!(res.model["classes"]["Box"]["type"], "𠀀、１");
}

#[test]
fn parse_class_editor_facts_recovers_after_invalid_unicode_name() {
    let engine = Engine::new();
    let text = "classDiagram\nclass 𠀀\nclass User\n";
    let facts = engine
        .parse_editor_semantic_facts_with_type_sync("classDiagram", text)
        .unwrap()
        .expect("class editor facts");
    assert_eq!(facts.completeness, EditorSemanticCompleteness::Recovered);
    assert!(facts.symbols.iter().any(|symbol| symbol.name == "User"));
    let start = text.find('𠀀').unwrap();
    assert!(facts.diagnostics.iter().any(|diagnostic| {
        diagnostic.span == Some(SourceSpan::new(start, start + '𠀀'.len_utf8()))
    }));
}

#[test]
fn parse_diagram_class_css_class_shorthand() {
    let engine = Engine::new();
    let text = r#"classDiagram
class C1["Class 1 with text label"]:::styleClass
"#;

    let res = block_on(engine.parse_diagram(text, ParseOptions::default()))
        .unwrap()
        .unwrap();
    let c1 = &res.model["classes"]["C1"];
    assert_eq!(c1["label"], json!("Class 1 with text label"));
    assert_eq!(c1["cssClasses"], json!("default styleClass"));
}

#[test]
fn parse_diagram_class_namespace_and_generic_methods() {
    let engine = Engine::new();
    let text = r#"classDiagram
namespace Company.Project {
  class User {
    +login(username: String, password: String)
    +logout()
  }
}
namespace Company.Project.Module {
  class GenericClass~T~ {
    +addItem(item: T)
    +getItem() T
  }
}
"#;

    let res = block_on(engine.parse_diagram(text, ParseOptions::default()))
        .unwrap()
        .unwrap();
    let user = &res.model["classes"]["User"];
    assert_eq!(user["parent"], json!("Company.Project"));
    assert_eq!(
        user["methods"][0]["displayText"],
        json!("+login(username: String, password: String)")
    );
    assert_eq!(user["methods"][1]["displayText"], json!("+logout()"));

    let generic = &res.model["classes"]["GenericClass"];
    assert_eq!(generic["type"], json!("T"));
    assert_eq!(generic["parent"], json!("Company.Project.Module"));
    assert_eq!(
        generic["methods"][0]["displayText"],
        json!("+addItem(item: T)")
    );
    assert_eq!(
        generic["methods"][1]["displayText"],
        json!("+getItem() : T")
    );
}

#[test]
fn parse_diagram_class_method_parser_matches_upstream_greedy_regex_boundary() {
    let engine = Engine::new();
    let text = r#"classDiagram
class Parser {
  +outer(inner)(value) Result$
}
"#;

    let res = block_on(engine.parse_diagram(text, ParseOptions::default()))
        .unwrap()
        .unwrap();
    let method = &res.model["classes"]["Parser"]["methods"][0];
    assert_eq!(
        method["displayText"],
        json!("+outer(inner)(value) : Result")
    );
    assert_eq!(method["id"], json!("outer(inner)"));
    assert_eq!(method["parameters"], json!("value"));
    assert_eq!(method["returnType"], json!("Result"));
    assert_eq!(method["cssStyle"], json!("text-decoration:underline;"));
}

#[test]
fn parse_diagram_class_acc_descr_multiline_collapses_newline_whitespace_without_regex() {
    let engine = Engine::new();
    let text = r#"classDiagram
accTitle: My Title
accDescr {
  This is my multi
  line description
}
"#;

    let res = block_on(engine.parse_diagram(text, ParseOptions::default()))
        .unwrap()
        .unwrap();
    assert_eq!(res.model["accTitle"], json!("My Title"));
    assert_eq!(
        res.model["accDescr"],
        json!("This is my multi\nline description")
    );
}

#[test]
fn parse_diagram_class_hierarchical_dotted_namespace_and_notes() {
    let engine = Engine::new();
    let text = r#"classDiagram
namespace Company.Project.Module {
  class User
  note "Module scoped note"
}
"#;

    let res = block_on(engine.parse_diagram(text, ParseOptions::default()))
        .unwrap()
        .unwrap();

    let namespaces = res.model["namespaces"].as_object().unwrap();
    assert!(namespaces.contains_key("Company"));
    assert!(namespaces.contains_key("Company.Project"));
    assert!(namespaces.contains_key("Company.Project.Module"));
    assert_eq!(namespaces["Company"]["label"], json!("Company"));
    assert_eq!(namespaces["Company"]["parent"], json!(null));
    assert_eq!(namespaces["Company"]["explicit"], json!(false));
    assert_eq!(namespaces["Company.Project"]["label"], json!("Project"));
    assert_eq!(namespaces["Company.Project"]["parent"], json!("Company"));
    assert_eq!(namespaces["Company.Project"]["explicit"], json!(false));
    assert_eq!(
        namespaces["Company.Project.Module"]["label"],
        json!("Module")
    );
    assert_eq!(
        namespaces["Company.Project.Module"]["parent"],
        json!("Company.Project")
    );
    assert_eq!(
        namespaces["Company.Project.Module"]["classIds"],
        json!(["User"])
    );
    assert_eq!(
        namespaces["Company.Project.Module"]["noteIds"],
        json!(["note0"])
    );

    assert_eq!(
        res.model["classes"]["User"]["parent"],
        json!("Company.Project.Module")
    );
    assert_eq!(
        res.model["notes"][0]["parent"],
        json!("Company.Project.Module")
    );
    assert_eq!(res.model["notes"][0]["class"], json!(null));
    assert_eq!(res.model["notes"][0]["text"], json!("Module scoped note"));
}

#[test]
fn parse_diagram_class_nested_namespace_syntax_builds_qualified_parents() {
    let engine = Engine::new();
    let text = r#"classDiagram
namespace Company {
  namespace Project {
    class User
  }
}
"#;

    let res = block_on(engine.parse_diagram(text, ParseOptions::default()))
        .unwrap()
        .unwrap();

    assert_eq!(res.model["namespaces"]["Company"]["explicit"], json!(true));
    assert_eq!(
        res.model["namespaces"]["Company.Project"]["parent"],
        json!("Company")
    );
    assert_eq!(
        res.model["namespaces"]["Company.Project"]["explicit"],
        json!(true)
    );
    assert_eq!(
        res.model["classes"]["User"]["parent"],
        json!("Company.Project")
    );
}

#[test]
fn parse_diagram_class_hierarchical_namespaces_can_be_disabled() {
    let engine = Engine::new();
    let text = r#"---
config:
  class:
    hierarchicalNamespaces: false
---
classDiagram
namespace Company.Project.Module {
  class User
  note "Module scoped note"
}
"#;

    let res = block_on(engine.parse_diagram(text, ParseOptions::default()))
        .unwrap()
        .unwrap();

    let namespaces = res.model["namespaces"].as_object().unwrap();
    assert_eq!(
        namespaces.keys().cloned().collect::<Vec<_>>(),
        vec!["Company.Project.Module"]
    );
    assert_eq!(
        namespaces["Company.Project.Module"]["label"],
        json!("Company.Project.Module")
    );
    assert_eq!(namespaces["Company.Project.Module"]["parent"], json!(null));
    assert_eq!(
        res.model["classes"]["User"]["parent"],
        json!("Company.Project.Module")
    );
    assert_eq!(
        res.model["notes"][0]["parent"],
        json!("Company.Project.Module")
    );
}

#[test]
fn parse_diagram_class_relation_only_generic_classes_keep_type_params() {
    let engine = Engine::new();
    let text = r#"classDiagram
Class01~T~ <|-- AveryLongClass
Class03~T~ *-- Class04~T~
Class08 <--> C2: Cool label
"#;

    let res = block_on(engine.parse_diagram(text, ParseOptions::default()))
        .unwrap()
        .unwrap();

    let class01 = &res.model["classes"]["Class01"];
    assert_eq!(class01["type"], json!("T"));
    assert_eq!(class01["text"], json!("Class01&lt;T&gt;"));

    let class03 = &res.model["classes"]["Class03"];
    assert_eq!(class03["type"], json!("T"));
    assert_eq!(class03["text"], json!("Class03&lt;T&gt;"));

    let class04 = &res.model["classes"]["Class04"];
    assert_eq!(class04["type"], json!("T"));
    assert_eq!(class04["text"], json!("Class04&lt;T&gt;"));
}

#[test]
fn parse_diagram_class_relation_with_label_and_direction() {
    let engine = Engine::new();
    let text = r#"classDiagram
direction LR
class Admin
class Report
Admin --> Report : generates
"#;

    let res = block_on(engine.parse_diagram(text, ParseOptions::default()))
        .unwrap()
        .unwrap();
    assert_eq!(res.model["direction"], json!("LR"));

    let rels = res.model["relations"].as_array().unwrap();
    assert_eq!(rels.len(), 1);
    assert_eq!(rels[0]["id1"], json!("Admin"));
    assert_eq!(rels[0]["id2"], json!("Report"));
    assert_eq!(rels[0]["title"], json!("generates"));
    assert_eq!(rels[0]["relation"]["lineType"], json!(0));
    assert_eq!(rels[0]["relation"]["type1"], json!(-1));
    assert_eq!(rels[0]["relation"]["type2"], json!(3));
}

#[test]
fn parse_diagram_class_style_statement_sets_node_styles() {
    let engine = Engine::new();
    let text = r#"classDiagram
class Class01
style Class01 fill:#f9f,stroke:#333,stroke-width:4px
"#;

    let res = block_on(engine.parse_diagram(text, ParseOptions::default()))
        .unwrap()
        .unwrap();
    let c = &res.model["classes"]["Class01"];
    assert_eq!(
        c["styles"],
        json!(["fill:#f9f", "stroke:#333", "stroke-width:4px"])
    );
}

#[test]
fn parse_diagram_class_classdef_applies_styles_to_css_classes() {
    let engine = Engine::new();
    let text = r#"classDiagram
class Class01
cssClass "Class01" pink
classDef pink fill:#f9f
"#;

    let res = block_on(engine.parse_diagram(text, ParseOptions::default()))
        .unwrap()
        .unwrap();
    let c = &res.model["classes"]["Class01"];
    assert_eq!(c["cssClasses"], json!("default pink"));
    assert_eq!(c["styles"], json!(["fill:#f9f"]));
}

#[test]
fn parse_diagram_class_multiple_classdefs_merge_styles() {
    let engine = Engine::new();
    let text = r#"classDiagram
class Class01:::pink
cssClass "Class01" bold
classDef pink fill:#f9f
classDef bold stroke:#333,stroke-width:6px
"#;

    let res = block_on(engine.parse_diagram(text, ParseOptions::default()))
        .unwrap()
        .unwrap();
    let c = &res.model["classes"]["Class01"];
    assert_eq!(c["cssClasses"], json!("default pink bold"));
    assert_eq!(
        c["styles"],
        json!(["fill:#f9f", "stroke:#333", "stroke-width:6px"])
    );
}

#[test]
fn parse_diagram_class_link_and_click_statements_set_clickable_and_metadata() {
    let engine = Engine::new();
    let text = r#"classDiagram
class Class1
link Class1 "google.com" "A tooltip" _self
click Class1 href "example.com" "B tooltip" _blank
click Class1 call functionCall(test, test1, test2) "C tooltip"
callback Class1 "otherCall" "D tooltip"
"#;

    let res = block_on(engine.parse_diagram(text, ParseOptions::default()))
        .unwrap()
        .unwrap();

    let c = &res.model["classes"]["Class1"];
    assert!(c["cssClasses"].as_str().unwrap().contains("clickable"));
    assert_eq!(c["link"], json!("example.com"));
    assert_eq!(c["linkTarget"], json!("_blank"));
    assert_eq!(c["tooltip"], json!("D tooltip"));
    assert_eq!(c["haveCallback"], json!(true));
    assert_eq!(c["callback"]["function"], json!("otherCall"));
    assert_eq!(c["callbackEffective"], json!(false));
}

#[test]
fn parse_diagram_class_href_sanitizes_javascript_urls_when_not_loose() {
    let engine = Engine::new();
    let res = block_on(engine.parse_diagram(
        r#"classDiagram
class Class1
click Class1 href "javascript:alert(1)" "A tooltip" _self"#,
        ParseOptions::default(),
    ))
    .unwrap()
    .unwrap();

    let c = &res.model["classes"]["Class1"];
    assert_eq!(c["link"], json!("about:blank"));
    assert_eq!(c["linkTarget"], json!("_self"));
    assert_eq!(c["tooltip"], json!("A tooltip"));
}

#[test]
fn parse_diagram_class_security_level_sandbox_forces_link_target_top() {
    let engine = Engine::new().with_site_config({
        let mut cfg = MermaidConfig::empty_object();
        cfg.set_value("securityLevel", json!("sandbox"));
        cfg
    });

    let text = r#"classDiagram
class Class1
click Class1 href "google.com" "A tooltip" _self
"#;

    let res = block_on(engine.parse_diagram(text, ParseOptions::default()))
        .unwrap()
        .unwrap();
    let c = &res.model["classes"]["Class1"];
    assert_eq!(c["link"], json!("google.com"));
    assert_eq!(c["linkTarget"], json!("_top"));
}

#[test]
fn parse_diagram_class_security_level_loose_marks_callback_effective() {
    let engine = Engine::new().with_site_config({
        let mut cfg = MermaidConfig::empty_object();
        cfg.set_value("securityLevel", json!("loose"));
        cfg
    });

    let text = r#"classDiagram
class Class1
click Class1 call functionCall() "A tooltip"
"#;

    let res = block_on(engine.parse_diagram(text, ParseOptions::default()))
        .unwrap()
        .unwrap();
    let c = &res.model["classes"]["Class1"];
    assert_eq!(c["haveCallback"], json!(true));
    assert_eq!(c["callback"]["function"], json!("functionCall"));
    assert!(c["callback"].get("args").is_none());
    assert_eq!(c["callbackEffective"], json!(true));
}

#[test]
fn parse_class_editor_facts_preserve_parser_symbol_spans() {
    let engine = Engine::new();
    let text = r#"classDiagram
namespace Company {
  class User {
    +login()
    -password: String
  }
}
User <|-- Admin
User: email
class Visible["Visible label"]
User <|-- Admin : manages
Class1 "1" *-- "many" Class02 : contains
<<interface>> User
note for User "Primary user"
note "Floating note"
click User href "https://example.com" "Open user" _blank
click Admin call open(userId) "Open admin"
callback User "refreshUser" "Refresh user"
accTitle: Class chart
accDescr: Shows class relationships
classDef service fill:#eee
class User:::service
cssClass "User,Admin" service
style User fill:#fff
"#;
    let facts = engine
        .parse_editor_semantic_facts_with_type_sync("classDiagram", text)
        .unwrap()
        .expect("class editor facts");

    assert_eq!(facts.completeness, EditorSemanticCompleteness::Complete);

    let symbol_at = |name: &str, start: usize| {
        facts
            .symbols
            .iter()
            .find(|symbol| symbol.name == name && symbol.selection.start == start)
            .unwrap_or_else(|| panic!("missing symbol {name} at {start}"))
    };
    let symbol_with_detail = |name: &str, detail: &str| {
        facts
            .symbols
            .iter()
            .find(|symbol| symbol.name == name && symbol.detail.as_deref() == Some(detail))
            .unwrap_or_else(|| panic!("missing symbol {name} with detail {detail}"))
    };

    let company_start = text.find("Company").unwrap();
    assert_eq!(
        symbol_at("Company", company_start).selection.end,
        company_start + "Company".len()
    );

    let user_start = text.find("User {").unwrap();
    assert_eq!(
        symbol_at("User", user_start).selection.end,
        user_start + "User".len()
    );

    let login_start = text.find("+login()").unwrap();
    let login = symbol_at("+login()", login_start);
    assert_eq!(login.role, EditorSemanticRole::Outline);
    assert_eq!(login.detail.as_deref(), Some("class member"));

    let password_start = text.find("-password: String").unwrap();
    let password = symbol_at("-password: String", password_start);
    assert_eq!(password.role, EditorSemanticRole::Outline);
    assert_eq!(password.detail.as_deref(), Some("class member"));

    let admin_start = text.find("Admin").unwrap();
    assert_eq!(
        symbol_at("Admin", admin_start).selection.end,
        admin_start + "Admin".len()
    );

    let email_start = text.find("email").unwrap();
    let email = symbol_at("email", email_start);
    assert_eq!(email.role, EditorSemanticRole::Outline);
    assert_eq!(email.detail.as_deref(), Some("class member"));

    let display_label = facts
        .symbols
        .iter()
        .find(|symbol| {
            symbol.name == "Visible label"
                && symbol.detail.as_deref() == Some("class display label")
        })
        .unwrap();
    assert_eq!(display_label.role, EditorSemanticRole::Payload);

    let annotation_start = text.find("interface").unwrap();
    let annotation = symbol_at("interface", annotation_start);
    assert_eq!(annotation.role, EditorSemanticRole::Payload);
    assert_eq!(annotation.detail.as_deref(), Some("class annotation"));

    let relation_label = facts
        .symbols
        .iter()
        .find(|symbol| {
            symbol.name == "manages" && symbol.detail.as_deref() == Some("class relation label")
        })
        .unwrap();
    assert_eq!(relation_label.role, EditorSemanticRole::Payload);

    let left_multiplicity = facts
        .symbols
        .iter()
        .find(|symbol| {
            symbol.name == "1" && symbol.detail.as_deref() == Some("class relation multiplicity")
        })
        .unwrap();
    assert_eq!(left_multiplicity.role, EditorSemanticRole::Payload);

    let right_multiplicity = facts
        .symbols
        .iter()
        .find(|symbol| {
            symbol.name == "many" && symbol.detail.as_deref() == Some("class relation multiplicity")
        })
        .unwrap();
    assert_eq!(right_multiplicity.role, EditorSemanticRole::Payload);

    let note_for = facts
        .symbols
        .iter()
        .find(|symbol| {
            symbol.name == "Primary user" && symbol.detail.as_deref() == Some("class note")
        })
        .unwrap();
    assert_eq!(note_for.role, EditorSemanticRole::Payload);

    let floating_note = facts
        .symbols
        .iter()
        .find(|symbol| {
            symbol.name == "Floating note" && symbol.detail.as_deref() == Some("class note")
        })
        .unwrap();
    assert_eq!(floating_note.role, EditorSemanticRole::Payload);

    let acc_title = facts
        .symbols
        .iter()
        .find(|symbol| {
            symbol.name == "Class chart"
                && symbol.detail.as_deref() == Some("class accessibility title")
        })
        .unwrap();
    assert_eq!(acc_title.role, EditorSemanticRole::Payload);

    let acc_descr = facts
        .symbols
        .iter()
        .find(|symbol| {
            symbol.name == "Shows class relationships"
                && symbol.detail.as_deref() == Some("class accessibility description")
        })
        .unwrap();
    assert_eq!(acc_descr.role, EditorSemanticRole::Payload);

    let href_start = text.find("https://example.com").unwrap();
    let href = symbol_at("https://example.com", href_start);
    assert_eq!(href.role, EditorSemanticRole::Payload);
    assert_eq!(href.detail.as_deref(), Some("class interaction string"));

    let href_target_start = text.find("click User href").unwrap() + "click ".len();
    let href_target = symbol_at("User", href_target_start);
    assert_eq!(href_target.role, EditorSemanticRole::Reference);
    assert_eq!(href_target.kind, EditorSemanticKind::Class);
    assert_eq!(
        href_target.detail.as_deref(),
        Some("class interaction target")
    );

    let tooltip_start = text.find("Open user").unwrap();
    let tooltip = symbol_at("Open user", tooltip_start);
    assert_eq!(tooltip.role, EditorSemanticRole::Payload);
    assert_eq!(tooltip.detail.as_deref(), Some("class interaction string"));

    let target_start = text.find("_blank").unwrap();
    let target = symbol_at("_blank", target_start);
    assert_eq!(target.role, EditorSemanticRole::Payload);
    assert_eq!(target.detail.as_deref(), Some("class link target"));

    let service_start = text.find("service").unwrap();
    let service = symbol_at("service", service_start);
    assert_eq!(service.selection.end, service_start + "service".len());
    assert_eq!(service.role, EditorSemanticRole::ClassDefinition);
    assert_eq!(service.detail.as_deref(), Some("class definition"));

    let class_def_style = symbol_with_detail("fill:#eee", "class definition style");
    assert_eq!(class_def_style.role, EditorSemanticRole::Payload);

    let inline_service_start = text.find(":::service").unwrap() + ":::".len();
    let inline_service = symbol_at("service", inline_service_start);
    assert_eq!(inline_service.role, EditorSemanticRole::Payload);
    assert_eq!(inline_service.detail.as_deref(), Some("class inline class"));

    let css_admin = symbol_with_detail("Admin", "class css target");
    assert_eq!(css_admin.role, EditorSemanticRole::Reference);

    let css_service = symbol_with_detail("service", "class css reference");
    assert_eq!(css_service.role, EditorSemanticRole::Payload);

    let class_style = symbol_with_detail("fill:#fff", "class style");
    assert_eq!(class_style.role, EditorSemanticRole::Payload);

    let callback = symbol_with_detail("open", "class callback");
    assert_eq!(callback.role, EditorSemanticRole::Payload);
    assert_eq!(callback.kind, EditorSemanticKind::Function);

    let call_target_start = text.find("click Admin call").unwrap() + "click ".len();
    let call_target = symbol_at("Admin", call_target_start);
    assert_eq!(call_target.kind, EditorSemanticKind::Class);

    let callback_args = symbol_with_detail("userId", "class callback args");
    assert_eq!(callback_args.role, EditorSemanticRole::Payload);

    let callback_target_start = text.find("callback User").unwrap() + "callback ".len();
    let callback_target = symbol_at("User", callback_target_start);
    assert_eq!(callback_target.kind, EditorSemanticKind::Class);

    let callback_statement = symbol_with_detail("refreshUser", "class callback");
    assert_eq!(callback_statement.role, EditorSemanticRole::Payload);
    assert_eq!(callback_statement.kind, EditorSemanticKind::Function);

    assert!(facts.directive_prefixes.iter().any(|p| p == "click"));
    assert!(facts.directive_prefixes.iter().any(|p| p == "callback"));
    assert!(facts.directive_prefixes.iter().any(|p| p == "cssClass"));
    assert!(facts.directive_prefixes.iter().any(|p| p == "style"));
    assert!(facts.directive_prefixes.iter().any(|p| p == "classDef"));
}

#[test]
fn parse_class_editor_facts_preserve_quoted_numeric_class_selection_spans() {
    let engine = Engine::new();
    let text =
        "classDiagram\nclass `123`\n`123` --> Service\nclick `123` href \"https://example.com\"\n";
    let facts = engine
        .parse_editor_semantic_facts_with_type_sync("classDiagram", text)
        .unwrap()
        .expect("class editor facts");

    let numeric_starts = text
        .match_indices("123")
        .map(|(start, _)| start)
        .collect::<Vec<_>>();
    assert_eq!(numeric_starts.len(), 3);

    let symbol_at = |detail: &str, start: usize| {
        facts
            .symbols
            .iter()
            .find(|symbol| {
                symbol.name == "classId-123"
                    && symbol.detail.as_deref() == Some(detail)
                    && symbol.selection.start == start
            })
            .unwrap_or_else(|| panic!("missing quoted numeric class symbol {detail} at {start}"))
    };

    for (detail, start) in [
        ("class", numeric_starts[0]),
        ("class relation target", numeric_starts[1]),
        ("class interaction target", numeric_starts[2]),
    ] {
        let symbol = symbol_at(detail, start);
        assert_eq!(symbol.selection.end, start + "123".len());
    }

    assert_eq!(
        symbol_at("class", numeric_starts[0]).role,
        EditorSemanticRole::Entity
    );
    assert_eq!(
        symbol_at("class relation target", numeric_starts[1]).role,
        EditorSemanticRole::Reference
    );
    assert_eq!(
        symbol_at("class interaction target", numeric_starts[2]).role,
        EditorSemanticRole::Reference
    );
}

#[test]
fn class_relations_create_only_the_first_implicit_entity_occurrence() {
    let engine = Engine::new();
    let text = concat!(
        "classDiagram\n",
        "Future --> Known\n",
        "Known --> Future\n",
        "class Known\n",
        "style Later fill:#fff\n",
        "class Later\n",
        "<<interface>> Annotated\n",
        "class Annotated\n",
    );
    let facts = engine
        .parse_editor_semantic_facts_with_type_sync("classDiagram", text)
        .unwrap()
        .expect("class editor facts");

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
        ]
    );
    assert_eq!(
        roles("Later"),
        [EditorSemanticRole::Reference, EditorSemanticRole::Entity]
    );
    assert_eq!(
        roles("Annotated"),
        [EditorSemanticRole::Entity, EditorSemanticRole::Entity]
    );
}

#[test]
fn parse_class_editor_facts_preserve_click_call_callback_name_span_after_whitespace() {
    let engine = Engine::new();
    let text = "classDiagram\nclass Admin\nclick Admin call   open(userId)\n";
    let facts = engine
        .parse_editor_semantic_facts_with_type_sync("classDiagram", text)
        .unwrap()
        .expect("class editor facts");

    let open_start = text.find("open").unwrap();
    let callback = facts
        .symbols
        .iter()
        .find(|symbol| symbol.name == "open" && symbol.detail.as_deref() == Some("class callback"))
        .expect("callback symbol");

    assert_eq!(callback.selection.start, open_start);
    assert_eq!(callback.selection.end, open_start + "open".len());
}

#[test]
fn parse_class_editor_facts_preserve_every_crlf_unicode_occurrence_and_payload_span() {
    let engine = Engine::new();
    let text = concat!(
        "---\r\n",
        "config:\r\n",
        "  theme: dark\r\n",
        "---\r\n",
        "%%{init: {\"theme\": \"default\"}}%%\r\n",
        "classDiagram-v2\r\n",
        "namespace 公司 {\r\n",
        "  class 顧客[\"客戶\"] {\r\n",
        "    +查詢(id) 結果\r\n",
        "  }\r\n",
        "  note for 顧客 \"Primary customer\"\r\n",
        "}\r\n",
        "class 訂單\r\n",
        "顧客 \"1\" *-- \"many\" 訂單 : owns 群組\r\n",
        "note for 顧客 \"Important account\"\r\n",
        "click 顧客 call open(customerId) \"Open customer\"\r\n",
        "cssClass \"顧客,訂單\" service\r\n",
        "style 顧客 fill:#fff\r\n",
    );
    let facts = engine
        .parse_editor_semantic_facts_with_type_sync("classDiagram", text)
        .unwrap()
        .expect("Class editor facts");

    assert_eq!(facts.completeness, EditorSemanticCompleteness::Complete);
    for name in ["顧客", "訂單"] {
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
            "Class lost or collapsed a {name:?} occurrence"
        );
    }

    let payload = |name: &str, detail: &str| {
        facts
            .symbols
            .iter()
            .find(|symbol| symbol.name == name && symbol.detail.as_deref() == Some(detail))
            .unwrap_or_else(|| panic!("missing Class payload {name:?} ({detail})"))
    };
    for (name, detail) in [
        ("客戶", "class display label"),
        ("1", "class relation multiplicity"),
        ("many", "class relation multiplicity"),
        ("owns 群組", "class relation label"),
        ("Primary customer", "class note"),
        ("Important account", "class note"),
        ("open", "class callback"),
        ("customerId", "class callback args"),
        ("Open customer", "class interaction string"),
        ("fill:#fff", "class style"),
    ] {
        let start = text.find(name).unwrap();
        assert_eq!(
            payload(name, detail).selection,
            SourceSpan::new(start, start + name.len()),
            "Class payload {name:?} span drifted"
        );
    }
}

#[test]
fn parse_class_editor_facts_record_expected_class_name_spans() {
    let engine = Engine::new();
    let text = "classDiagram\nclassDef service fill:#eee\n";
    let facts = engine
        .parse_editor_semantic_facts_with_type_sync("classDiagram", text)
        .unwrap()
        .expect("class editor facts");

    assert!(facts.expected_syntax.iter().any(|expected| {
        expected.kind == EditorExpectedSyntaxKind::ClassName
            && expected.span.start == text.find("service").unwrap()
    }));
}

#[test]
fn parse_class_editor_facts_do_not_consume_classdef_names_across_line_endings() {
    let engine = Engine::new();

    for line_ending in ["\n", "\r", "\r\n"] {
        let text = ["classDiagram", "classDef", "User <|-- Admin", ""].join(line_ending);
        let facts = engine
            .parse_editor_semantic_facts_with_type_sync("classDiagram", &text)
            .unwrap()
            .expect("class editor facts");

        assert!(
            !facts.symbols.iter().any(|symbol| {
                symbol.name == "User" && symbol.role == EditorSemanticRole::ClassDefinition
            }),
            "classDef consumed the next physical line for {line_ending:?}"
        );
    }
}

#[test]
fn parse_class_editor_facts_recovers_from_incomplete_input() {
    let engine = Engine::new();
    let text = "classDiagram\nclass User\nUser <|--";
    let facts = engine
        .parse_editor_semantic_facts_with_type_sync("classDiagram", text)
        .unwrap()
        .expect("class editor facts");

    assert_eq!(facts.completeness, EditorSemanticCompleteness::Recovered);
    assert!(facts.symbols.iter().any(|symbol| symbol.name == "User"));
}

#[test]
fn parse_class_editor_facts_stop_after_non_advancing_lexer_error() {
    let engine = Engine::new();
    let text = "classDiagram\nclass User {\n  +name";
    crate::diagrams::class::reset_class_syntax_construction_count();
    let facts = engine
        .parse_editor_semantic_facts_with_type_sync("classDiagram", text)
        .unwrap()
        .expect("class editor facts");

    assert_eq!(facts.completeness, EditorSemanticCompleteness::Recovered);
    assert!(facts.symbols.iter().any(|symbol| symbol.name == "User"));
    assert_eq!(crate::diagrams::class::class_syntax_construction_count(), 1);
    assert!(facts.diagnostics.iter().any(|diagnostic| {
        diagnostic.kind == EditorSemanticDiagnosticKind::ParserRecovery
            && diagnostic.span == Some(SourceSpan::new(text.len(), text.len()))
    }));

    crate::diagrams::class::reset_class_syntax_construction_count();
    let error = engine
        .parse_diagram_sync(text, ParseOptions::strict())
        .expect_err("strict Class parsing rejects an unterminated body");
    assert_eq!(crate::diagrams::class::class_syntax_construction_count(), 1);
    let Error::DiagramParse { diagnostic, .. } = error else {
        panic!("unterminated Class body returned a non-parse error");
    };
    assert_eq!(
        diagnostic.span(),
        Some(SourceSpan::new(text.len(), text.len()))
    );
    assert_eq!(diagnostic.span_kind(), ParseDiagnosticSpanKind::Exact);
}

#[test]
fn parse_class_editor_facts_continue_after_advancing_lexer_error() {
    let engine = Engine::new();
    let text = "classDiagram\n\"\nclass User\n";
    let invalid_start = text.find('"').unwrap();
    crate::diagrams::class::reset_class_syntax_construction_count();
    let facts = engine
        .parse_editor_semantic_facts_with_type_sync("classDiagram", text)
        .unwrap()
        .expect("class editor facts");

    assert_eq!(facts.completeness, EditorSemanticCompleteness::Recovered);
    assert!(facts.symbols.iter().any(|symbol| symbol.name == "User"));
    assert_eq!(crate::diagrams::class::class_syntax_construction_count(), 1);
    assert!(facts.diagnostics.iter().any(|diagnostic| {
        diagnostic.kind == EditorSemanticDiagnosticKind::ParserRecovery
            && diagnostic.span == Some(SourceSpan::new(invalid_start, invalid_start + 1))
    }));

    crate::diagrams::class::reset_class_syntax_construction_count();
    let error = engine
        .parse_diagram_sync(text, ParseOptions::strict())
        .expect_err("strict Class parsing rejects an unterminated string");
    assert_eq!(crate::diagrams::class::class_syntax_construction_count(), 1);
    let Error::DiagramParse { diagnostic, .. } = error else {
        panic!("unterminated Class string returned a non-parse error");
    };
    assert_eq!(
        diagnostic.span(),
        Some(SourceSpan::new(invalid_start, invalid_start + 1))
    );
    assert_eq!(diagnostic.span_kind(), ParseDiagnosticSpanKind::Exact);
}
