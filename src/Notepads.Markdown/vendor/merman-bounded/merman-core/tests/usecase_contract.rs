use merman_core::{
    EditorRenamePolicy, EditorSemanticCompleteness, EditorSemanticKind, EditorSemanticRole, Engine,
    ParseOptions, RenderSemanticModel,
};

#[test]
fn usecase_public_parse_routes_preserve_preprocessed_source_identity() {
    let source = "---\ntitle: 订单\n---\nusecase-beta\nactor Customer(\"客户\")\nsystemBoundary \"Order system\"\n  Checkout(\"Place order\")\nend\nCustomer --> Checkout\n";
    let engine = Engine::new();
    let parsed = engine
        .parse_diagram_sync(source, ParseOptions::strict())
        .expect("semantic parse")
        .expect("Usecase detection");
    let typed = engine
        .parse_diagram_for_render_model_sync(source, ParseOptions::strict())
        .expect("typed parse")
        .expect("Usecase detection");
    let RenderSemanticModel::Usecase(model) = typed.model() else {
        panic!("Usecase must have its own typed semantic model");
    };
    assert_eq!(typed.metadata().title.as_deref(), Some("订单"));
    assert_eq!(model.nodes[0].label, "客户");
    assert_eq!(model.boundaries[0].members, ["Checkout"]);
    assert_eq!(model.relationships[0].source, "Customer");
    assert_eq!(model.relationships[0].target, "Checkout");
    assert_eq!(
        typed
            .model()
            .compatibility_json(typed.metadata())
            .expect("JSON projection"),
        parsed.model
    );

    let facts = engine
        .parse_editor_semantic_facts_with_type_sync("usecase", source)
        .expect("editor parse")
        .expect("parser-backed facts");
    assert!(facts.diagnostics.is_empty(), "{:?}", facts.diagnostics);
    for id in ["Customer", "Checkout"] {
        let occurrences = facts
            .symbols
            .iter()
            .filter(|symbol| symbol.name == id)
            .collect::<Vec<_>>();
        assert_eq!(
            occurrences.len(),
            2,
            "declaration and relationship endpoint for {id}"
        );
        for occurrence in occurrences {
            assert_eq!(
                &source[occurrence.selection.start..occurrence.selection.end],
                id
            );
            assert_eq!(
                occurrence.rename_policy,
                EditorRenamePolicy::UsecaseIdentifier
            );
        }
    }
}

#[test]
fn usecase_rename_accepts_only_a_complete_identifier_token() {
    let policy = EditorRenamePolicy::UsecaseIdentifier;
    for accepted in [
        "Customer",
        "_user",
        "123",
        "123abc",
        "actor2",
        "systemBoundary2",
    ] {
        assert!(policy.accepts(accepted), "{accepted}");
    }
    for rejected in [
        "",
        "two words",
        "customer-id",
        "客户",
        "actor",
        "direction",
        "end",
        "systemBoundary",
    ] {
        assert!(!policy.accepts(rejected), "{rejected}");
    }
}

#[test]
fn usecase_recovery_preserves_declared_and_implicit_identity() {
    let engine = Engine::new();
    let source = concat!(
        "usecase-beta\n",
        "User link@--> Login\n",
        "actor User\n",
        "Login(Sign in)\n",
        "json Data@{\"x\":1}\n",
        "systemBoundary System\n",
        "Extra(Extra)\n",
        "end\n",
        "A --> B\n",
        "note for User \"Note\"\n",
        "style System fill:red\n",
        "style Data fill:red\n",
        "style link stroke:red\n",
    );
    let complete = engine
        .parse_editor_semantic_facts_with_type_sync("usecase", source)
        .expect("editor parse")
        .expect("parser-backed facts");
    assert_eq!(complete.completeness, EditorSemanticCompleteness::Complete);
    assert!(
        complete.diagnostics.is_empty(),
        "{:?}",
        complete.diagnostics
    );
    for (name, kind) in [
        ("User", EditorSemanticKind::Variable),
        ("Login", EditorSemanticKind::Function),
        ("Data", EditorSemanticKind::Object),
        ("System", EditorSemanticKind::Namespace),
        ("link", EditorSemanticKind::Event),
        ("A", EditorSemanticKind::Function),
        ("B", EditorSemanticKind::Function),
    ] {
        let occurrences = complete
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
        assert!(!occurrences.is_empty(), "{name}");
        assert_eq!(
            occurrences
                .iter()
                .filter(|symbol| symbol.role == EditorSemanticRole::Entity)
                .count(),
            1,
            "{name}"
        );
        for occurrence in occurrences {
            assert_eq!(occurrence.kind, kind, "{name}");
            assert_eq!(
                occurrence.rename_policy,
                EditorRenamePolicy::UsecaseIdentifier,
                "{name}"
            );
        }
    }
    for suffix in [
        "C -->",
        "systemBoundary Pending\nactor Inner\n",
        "style Ghost fill:red\n",
    ] {
        let recovered = engine
            .parse_editor_semantic_facts_with_type_sync("usecase", &format!("{source}{suffix}"))
            .expect("editor recovery")
            .expect("parser-backed facts");
        assert_eq!(
            recovered.completeness,
            EditorSemanticCompleteness::Recovered,
            "{suffix}"
        );
        assert!(!recovered.diagnostics.is_empty(), "{suffix}");
        assert_eq!(
            &recovered.symbols[..complete.symbols.len()],
            complete.symbols.as_slice(),
            "{suffix}"
        );
        assert!(
            !recovered
                .symbols
                .iter()
                .any(|symbol| symbol.name == "Ghost" && symbol.role == EditorSemanticRole::Entity)
        );
        if suffix.starts_with("systemBoundary") {
            for (name, kind) in [
                ("Pending", EditorSemanticKind::Namespace),
                ("Inner", EditorSemanticKind::Variable),
            ] {
                assert!(recovered.symbols.iter().any(|symbol| symbol.name == name
                    && symbol.role == EditorSemanticRole::Entity
                    && symbol.kind == kind));
            }
        }
    }
}

#[test]
fn usecase_recovery_disables_rename_for_conflicting_declaration_kinds() {
    let facts = Engine::new()
        .parse_editor_semantic_facts_with_type_sync(
            "usecase",
            "usecase-beta\nactor A\nA(Usecase)\nA --> B\nC -->",
        )
        .expect("editor recovery")
        .expect("parser-backed facts");
    assert_eq!(facts.completeness, EditorSemanticCompleteness::Recovered);
    let occurrences = facts
        .symbols
        .iter()
        .filter(|symbol| symbol.name == "A")
        .collect::<Vec<_>>();
    assert_eq!(occurrences.len(), 3);
    assert!(
        occurrences
            .iter()
            .all(|symbol| symbol.rename_policy == EditorRenamePolicy::None)
    );
}

#[test]
fn usecase_utf16_json_keeps_semantic_typed_and_editor_routes_available() {
    let source = r#"usecase-beta
json Data@{"\ud800":"\udc00","normal":"value"}
Data --> Consumer
"#;
    let engine = Engine::new();
    let semantic = engine
        .parse_diagram_sync(source, ParseOptions::strict())
        .unwrap()
        .unwrap();
    let typed = engine
        .parse_diagram_for_render_model_sync(source, ParseOptions::strict())
        .unwrap()
        .unwrap();
    assert_eq!(
        typed.model().compatibility_json(typed.metadata()).unwrap(),
        semantic.model
    );
    assert_eq!(
        semantic.model["jsonNodes"][0]["stringEncoding"],
        "json-utf16"
    );
    let facts = engine
        .parse_editor_semantic_facts_with_type_sync("usecase", source)
        .unwrap()
        .unwrap();
    assert_eq!(facts.completeness, EditorSemanticCompleteness::Complete);
    assert!(facts.diagnostics.is_empty(), "{:?}", facts.diagnostics);
    assert!(
        facts
            .symbols
            .iter()
            .any(|symbol| symbol.name == "Data" && symbol.kind == EditorSemanticKind::Object)
    );
}

#[test]
fn usecase_eof_diagnostics_keep_original_positions_across_public_routes() {
    let engine = Engine::new();
    for body in ["actor", "A -->"] {
        for separator in ["\n", "\r\n"] {
            let source = format!(
                "---{separator}title: EOF{separator}---{separator}usecase-beta{separator}%% retained source line{separator}{body}"
            );
            let expected = merman_core::SourceSpan::new(source.len(), source.len());
            for error in [
                engine
                    .parse_diagram_sync(&source, ParseOptions::strict())
                    .unwrap_err(),
                engine
                    .parse_diagram_for_render_model_sync(&source, ParseOptions::strict())
                    .unwrap_err(),
            ] {
                let merman_core::Error::DiagramParse { diagnostic, .. } = error else {
                    panic!("expected Usecase parse diagnostic");
                };
                assert_eq!(diagnostic.span(), Some(expected), "{body}/{separator:?}");
            }
            let facts = engine
                .parse_editor_semantic_facts_with_type_sync("usecase", &source)
                .unwrap()
                .unwrap();
            assert_eq!(facts.completeness, EditorSemanticCompleteness::Recovered);
            assert!(
                facts
                    .diagnostics
                    .iter()
                    .any(|diagnostic| diagnostic.span == Some(expected)),
                "{body}: {:?}",
                facts.diagnostics
            );
        }
    }
}
