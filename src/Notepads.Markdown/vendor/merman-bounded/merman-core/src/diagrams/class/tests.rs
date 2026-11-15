use super::*;
use crate::{MermaidConfig, ParseMetadata};

fn meta() -> ParseMetadata {
    ParseMetadata {
        diagram_type: "classDiagram".to_string(),
        config: MermaidConfig::default(),
        effective_config: MermaidConfig::default(),
        title: None,
    }
}

#[test]
fn authoritative_parser_preserves_the_former_fast_subset() {
    let code = r#"classDiagram
class C1 {
  +String field1
  +method1()
}
C1 <|-- C2 : inherits
"#;
    let meta = meta();
    let compat = parse::parse_class(code, &meta).expect("compat parse");
    let typed = parse::parse_class_typed(code, &meta).expect("typed parse");
    assert_eq!(compat, render_model_to_compat_json(&typed, &meta).unwrap());
}

#[test]
fn typed_relations_distinguish_authored_none_from_the_compatibility_sentinel() {
    let code = r#"classDiagram
class A
class B
A --> B
A "none" --> B
A "" --> B
"#;

    let model = parse::parse_class_typed(code, &meta()).expect("class diagram should parse");

    assert_eq!(model.relations[0].relation_title_1, None);
    assert_eq!(model.relations[1].relation_title_1.as_deref(), Some("none"));
    assert_eq!(model.relations[2].relation_title_1.as_deref(), Some(""));

    let typed_json = serde_json::to_value(&model).unwrap();
    assert!(typed_json["relations"][0]["relationTitle1"].is_null());
    assert_eq!(typed_json["relations"][1]["relationTitle1"], "none");
    let round_trip: crate::models::class_diagram::ClassDiagram =
        serde_json::from_value(typed_json).unwrap();
    assert_eq!(round_trip, model);

    let compatibility = render_model_to_compat_json(&model, &meta()).unwrap();
    assert_eq!(compatibility["relations"][0]["relationTitle1"], "none");
    assert_eq!(compatibility["relations"][1]["relationTitle1"], "none");
    assert_eq!(compatibility["relations"][2]["relationTitle1"], "");
}

#[test]
fn synthetic_note_and_interface_ids_do_not_collide_with_authored_classes() {
    let code = r#"classDiagram
class note0
note "standalone"
class interface0
IService ()-- Service
"#;

    let model = parse::parse_class_typed(code, &meta()).expect("class diagram should parse");

    assert!(model.classes.contains_key("note0"));
    assert!(model.classes.contains_key("interface0"));
    assert_ne!(model.notes[0].id, "note0");
    assert_ne!(model.interfaces[0].id, "interface0");
    assert_eq!(model.relations[0].id1, model.interfaces[0].id);
    assert_eq!(model.relations[0].id2, "Service");
}

#[test]
fn namespace_qualified_relation_endpoints_create_facade_classes_like_mermaid() {
    let code = r#"classDiagram
namespace Platform["Platform Layer"] {
  namespace FFI {
    class DartBinding
    class PythonBinding
  }
  namespace Core {
    class Renderer
  }
}
Platform.FFI.DartBinding --> Platform.Core.Renderer : calls
Platform.FFI.PythonBinding --> Platform.Core.Renderer : calls
"#;

    let model = parse::parse_class_typed(code, &meta()).expect("class diagram should parse");

    assert_eq!(
        model.classes.keys().cloned().collect::<Vec<_>>(),
        vec![
            "DartBinding",
            "PythonBinding",
            "Renderer",
            "Platform.FFI.DartBinding",
            "Platform.Core.Renderer",
            "Platform.FFI.PythonBinding"
        ]
    );
    assert_eq!(model.relations[0].id1, "Platform.FFI.DartBinding");
    assert_eq!(model.relations[0].id2, "Platform.Core.Renderer");
    assert_eq!(model.relations[1].id1, "Platform.FFI.PythonBinding");
    assert_eq!(model.relations[1].id2, "Platform.Core.Renderer");
    assert_eq!(
        model.namespaces["Platform.FFI"].class_ids,
        vec!["DartBinding", "PythonBinding"]
    );
    assert_eq!(
        model.namespaces["Platform.Core"].class_ids,
        vec!["Renderer"]
    );
    assert_eq!(
        model.namespace_facade_aliases,
        std::collections::BTreeMap::from([
            ("Platform.Core.Renderer".to_string(), "Renderer".to_string(),),
            (
                "Platform.FFI.DartBinding".to_string(),
                "DartBinding".to_string(),
            ),
            (
                "Platform.FFI.PythonBinding".to_string(),
                "PythonBinding".to_string(),
            ),
        ])
    );

    let typed_json = serde_json::to_value(&model).expect("typed class model should serialize");
    assert_eq!(
        typed_json["namespaceFacadeAliases"]["Platform.FFI.DartBinding"],
        "DartBinding"
    );
    let round_trip: crate::models::class_diagram::ClassDiagram =
        serde_json::from_value(typed_json).expect("typed class model should deserialize");
    assert_eq!(round_trip, model);

    let compatibility =
        render_model_to_compat_json(&model, &meta()).expect("compatibility projection should work");
    assert!(compatibility.get("namespaceFacadeAliases").is_none());
}

#[test]
fn explicit_qualified_class_is_not_a_synthetic_namespace_facade() {
    let code = r#"classDiagram
namespace N {
  class C
}
class N.C["Distinct"]
class D
N.C --> D
"#;

    let model = parse::parse_class_typed(code, &meta()).expect("class diagram should parse");

    assert!(model.namespace_facade_aliases.is_empty());
    assert_eq!(model.classes["N.C"].text, "Distinct");
    assert_eq!(model.relations[0].id1, "N.C");
}
