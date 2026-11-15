use super::*;

fn meta() -> ParseMetadata {
    ParseMetadata {
        diagram_type: "usecase".to_string(),
        config: crate::MermaidConfig::empty_object(),
        effective_config: crate::MermaidConfig::empty_object(),
        title: None,
    }
}
fn model(source: &str) -> UsecaseDiagramRenderModel {
    parse_usecase_model_for_render_controlled(source, &meta(), &OperationControl::new())
        .unwrap()
        .unwrap()
}
fn rejected(body: &str) -> Error {
    parse_usecase_model_for_render_controlled(
        &format!("usecase-beta\n{body}"),
        &meta(),
        &OperationControl::new(),
    )
    .unwrap()
    .unwrap_err()
}

#[test]
fn declaration_resolution_is_independent_of_relation_order() {
    let a = model("usecase-beta\nUser --> Login\nactor User\nLogin[Sign in]");
    let b = model("usecase-beta\nactor User\nLogin[Sign in]\nUser --> Login");
    assert_eq!(a, b);
    assert_eq!(a.nodes[0].kind, UsecaseNodeKind::Actor);
    assert_eq!(a.nodes[1].label, "Sign in");
    assert_eq!(a.nodes[1].kind, UsecaseNodeKind::UseCaseRect);
}

#[test]
fn boundaries_merge_equivalent_declarations_in_source_order() {
    let parsed = model(
        "usecase-beta\nsystemBoundary Auth(Authentication)@{type: package}:::external\nactor User(Person)@{type:hollow} <<Human>>\nactor User(Person)@{type:hollow} <<Human>>\nLogin[Sign in]\nend\nsystemBoundary Auth(Authentication)\nLogout\nend",
    );
    assert_eq!(parsed.boundaries[0].members, ["User", "Login", "Logout"]);
    assert!(parsed.boundaries[0].package);
    assert_eq!(parsed.nodes[0].actor_type, UsecaseActorType::Hollow);
    assert_eq!(parsed.nodes[0].parent_id.as_deref(), Some("Auth"));
}

#[test]
fn all_arrow_types_preserve_written_endpoint_order_and_minlen() {
    let parsed = model(
        "usecase-beta\nA --> B\nA <-- B\nA --- B\nA --o B\nA --x B\nA o-- B\nA x-- B\nA -- label ---> B\nA <-- label ---- B\nA ..>:InClUdE B\nA ..>:extend B\nA --|> B",
    );
    assert_eq!(
        parsed
            .relationships
            .iter()
            .take(7)
            .map(|edge| u8::from(edge.arrow_type))
            .collect::<Vec<_>>(),
        [0, 1, 2, 3, 4, 5, 6]
    );
    assert!(
        parsed
            .relationships
            .iter()
            .all(|edge| edge.source == "A" && edge.target == "B")
    );
    assert_eq!(parsed.relationships[2].minlen, 2);
    assert_eq!(parsed.relationships[7].minlen, 2);
    assert_eq!(parsed.relationships[8].minlen, 3);
    assert_eq!(parsed.relationships[9].label.as_deref(), Some("include"));
    assert!(parsed.relationships[9].dotted);
    assert_eq!(
        parsed.relationships[11].relationship_type,
        UsecaseRelationshipType::Generalization
    );
}

#[test]
fn multiline_markdown_metadata_json_and_accessibility_are_structured() {
    let parsed = model(
        "usecase-beta\r\naccTitle: Sign in\r\naccDescr {\r\n  first line\r\n  second line\r\n}\r\nactor User@{\r\ntype: hollow\r\n,business: true,\r\n}\r\nLogin(\"`**Sign in**\nsecurely`\")\njson Data@{\"10\":1,\"2\":{\"b\":2,\"a\":1},\"list\":[false,null]}\nData --> Login\nnote for Login \"`Please\ntry again`\"\n",
    );
    assert_eq!(parsed.nodes[1].label_type, UsecaseLabelType::Markdown);
    assert_eq!(parsed.nodes[1].label, "**Sign in**\nsecurely");
    assert_eq!(parsed.json_nodes[0].property_order[""], ["10", "2", "list"]);
    assert_eq!(parsed.json_nodes[0].property_order["/2"], ["b", "a"]);
    assert_eq!(parsed.notes[0].label_type, UsecaseLabelType::Markdown);
    assert_eq!(parsed.acc_title.as_deref(), Some("Sign in"));
    assert_eq!(
        parsed.acc_description.as_deref(),
        Some("first line\r\n  second line")
    );
}

#[test]
fn deferred_metadata_classes_and_styles_resolve_explicit_edges() {
    let parsed = model(
        "usecase-beta\nlink@{animation:fast}\nlink@{animate:false}\nclass A,link important,selected\nstyle link stroke-width:2px,stroke-dasharray:2\\,3\nclassDef important fill:red,border:1px solid blue\nA link@--> B\nA --> C\nactor User\nUser@{type:hollow}\n",
    );
    assert_eq!(parsed.relationships[0].id, "link");
    assert_eq!(parsed.relationships[1].id, "edge-0");
    assert_eq!(
        parsed.relationships[0].animation,
        Some(UsecaseAnimation::Fast)
    );
    assert!(parsed.relationships[0].animate);
    assert_eq!(parsed.relationships[0].classes, ["important", "selected"]);
    assert_eq!(
        parsed.relationships[0].styles,
        ["stroke-width:2px", "stroke-dasharray:2,3"]
    );
    assert_eq!(parsed.class_defs[0].styles[1], "border:1px solid blue");
}

#[test]
fn rejects_invalid_grammar_and_semantics_without_silent_fallbacks() {
    for body in [
        "actor A[Wrong]",
        "A --> B --> C",
        "A; B",
        "direction XX",
        "actor A %% inline",
        "systemBoundary S\nA --> B\nend",
        "systemBoundary S\nsystemBoundary T\nend\nend",
        "A(\"unterminated)",
        "A((nested))",
        "A@{business:1.5}",
        "A(A)@{business:true business:false}",
        "A ..> B",
        "A <.. B",
        "A --> B: label",
        "package S",
        "note for Missing note",
        "actor Shared\nShared",
        "A(First)\nA(Second)",
        "\"A B\"\n\"A-B\"",
        "actor User@{icon:\"fa:user\",type:hollow}",
        "A[Rect]@{business:true}",
        "actor A\nA ..>:include B",
        "actor A\nA --|> B",
        "json J@{}\nA --o J",
        "systemBoundary S\nend\nA --> S",
        "A:::paint --> B",
        "Ghost@{business:true}",
        "A e@--> B\nB e@--> C",
        "actor e\nA e@--> B",
        "class Ghost paint",
        "style Ghost fill:red",
    ] {
        let error = rejected(body);
        assert!(
            matches!(error, Error::DiagramParse { .. }),
            "{body}: {error}"
        );
    }
}

#[test]
fn editor_facts_have_exact_declarations_references_and_failure_spans() {
    let source = "usecase-beta\nactor User\nUser --> Login\nnote for Login \"Try again\"";
    let construction = construct(source, &meta(), &OperationControl::new())
        .unwrap()
        .unwrap_or_else(|failure| panic!("{}", failure.into_error()));
    let user: Vec<_> = construction
        .editor_facts
        .symbols
        .iter()
        .filter(|symbol| symbol.name == "User")
        .collect();
    assert_eq!(user.len(), 2);
    assert_eq!(
        &source[user[0].selection.start..user[0].selection.end],
        "User"
    );
    assert_eq!(user[0].role, crate::EditorSemanticRole::Entity);
    assert_eq!(user[1].role, crate::EditorSemanticRole::Reference);
    assert_eq!(user[1].kind, crate::EditorSemanticKind::Variable);
    let login: Vec<_> = construction
        .editor_facts
        .symbols
        .iter()
        .filter(|symbol| symbol.name == "Login")
        .collect();
    assert_eq!(login[0].role, crate::EditorSemanticRole::Entity);
    assert_eq!(login[1].role, crate::EditorSemanticRole::Reference);
    let bad = "usecase-beta\nactor User\nA -->";
    let failure = construct(bad, &meta(), &OperationControl::new())
        .unwrap()
        .err()
        .unwrap();
    let (error, facts) = failure.into_parts();
    assert!(facts.symbols.iter().any(|symbol| symbol.name == "User"));
    assert_eq!(
        facts.completeness,
        crate::EditorSemanticCompleteness::Recovered
    );
    let Error::DiagramParse { diagnostic, .. } = error else {
        panic!("expected structured parse diagnostic")
    };
    assert_eq!(
        diagnostic.span(),
        Some(SourceSpan::new(bad.len(), bad.len()))
    );
}

#[test]
fn generated_names_follow_javascript_utf16_and_are_not_renameable() {
    let source = "usecase-beta\n\"😀 login\"";
    let parsed = construct(source, &meta(), &OperationControl::new())
        .unwrap()
        .unwrap_or_else(|failure| panic!("{}", failure.into_error()));
    assert_eq!(parsed.model.nodes[0].id, "___login");
    assert_eq!(
        parsed.editor_facts.symbols[0].rename_policy,
        crate::EditorRenamePolicy::None
    );
}

#[test]
fn cancelled_parse_does_not_publish_partial_model() {
    let control = OperationControl::new();
    control.cancel();
    assert!(
        parse_usecase_model_for_render_controlled("usecase-beta\nactor A", &meta(), &control)
            .is_err()
    );
}

#[test]
fn json_infinity_presentation_survives_typed_roundtrip_without_changing_compat_json() {
    let parsed = model(
        r#"usecase-beta
json Data@{"positive":1e309,"negative":-1e309,"null":null}
"#,
    );
    let encoded = serde_json::to_value(&parsed).unwrap();
    assert_eq!(
        encoded["json_nodes"][0]["nonFiniteNumbers"]["/positive"],
        "positive"
    );
    assert_eq!(
        encoded["json_nodes"][0]["nonFiniteNumbers"]["/negative"],
        "negative"
    );
    assert_eq!(
        serde_json::from_value::<UsecaseDiagramRenderModel>(encoded).unwrap(),
        parsed
    );
    let compat = render_model_to_compat_json(&parsed, &meta()).unwrap();
    assert_eq!(
        compat["jsonNodes"][0]["value"],
        serde_json::json!({"positive":null,"negative":null,"null":null})
    );
    assert!(compat["jsonNodes"][0].get("nonFiniteNumbers").is_none());
    let old: UsecaseJsonNode = serde_json::from_value(compat["jsonNodes"][0].clone()).unwrap();
    assert!(old.non_finite_numbers.is_empty());
    assert!(
        serde_json::to_value(old)
            .unwrap()
            .get("nonFiniteNumbers")
            .is_none()
    );
}

#[test]
fn stereotype_uses_ecmascript_trim_and_excludes_u0085() {
    let trimmed = model("usecase-beta\nA <<\u{feff} Human \u{3000}>>");
    assert_eq!(trimmed.nodes[0].stereotype.as_deref(), Some("Human"));
    let preserved = model("usecase-beta\nA <<\u{0085}>>");
    assert_eq!(preserved.nodes[0].stereotype.as_deref(), Some("\u{0085}"));
    assert!(
        parse_usecase_model_for_render_controlled(
            "usecase-beta\nA <<\u{feff}>>",
            &meta(),
            &OperationControl::new(),
        )
        .unwrap()
        .is_err()
    );
}
