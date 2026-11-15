#![cfg(feature = "diagram-zenuml")]

use merman_core::{EditorSemanticCompleteness, Engine, ParsedEditorFacts};

const ADVANCED_ZENUML: &str = r#"zenuml
title Order Service
@Actor Client #FFEBE6
@Boundary OrderController #0747A6
@EC2 <<BFF>> OrderService #E3FCEF
group BusinessService {
  @Lambda PurchaseService
  @AzureFunction InvoiceService
}

@Starter(Client)
// `POST /orders`
OrderController.post(payload) {
  OrderService.create(payload) {
    order = new Order(payload)
    if(order != null) {
      par {
        PurchaseService.createPO(order)
        InvoiceService.createInvoice(order)
      }
    }
  }
}
"#;

#[test]
fn official_advanced_zenuml_builds_semantics_and_editor_facts() {
    let engine = Engine::new();
    let parsed = engine
        .parse_diagram_snapshot_sync(ADVANCED_ZENUML)
        .expect("advanced ZenUML must parse")
        .expect("advanced ZenUML must be detected");

    assert_eq!(parsed.metadata().diagram_type, "zenuml");
    let ParsedEditorFacts::Available(facts) = parsed.editor_facts() else {
        panic!("ZenUML must expose family-owned editor facts");
    };
    assert_eq!(facts.completeness, EditorSemanticCompleteness::Complete);
    assert!(facts.symbols.iter().any(|fact| fact.name == "OrderService"));
    assert!(
        facts
            .symbols
            .iter()
            .any(|fact| fact.name == "createInvoice")
    );
}

#[test]
fn invalid_zenuml_recovers_facts_on_both_sides_of_the_error() {
    let source = "zenuml\n@Starter(Client)\nA.call()\nif( {\nB.call()\nC.call()\n";
    let engine = Engine::new();
    let facts = engine
        .parse_editor_semantic_facts_with_type_sync("zenuml", source)
        .expect("facts request must complete")
        .expect("ZenUML facts must be available");

    assert_eq!(facts.completeness, EditorSemanticCompleteness::Recovered);
    assert!(facts.symbols.iter().any(|fact| fact.name == "A"));
    assert!(facts.symbols.iter().any(|fact| fact.name == "C"));
    assert!(
        facts
            .diagnostics
            .iter()
            .any(|diagnostic| diagnostic.span.is_some())
    );
}
