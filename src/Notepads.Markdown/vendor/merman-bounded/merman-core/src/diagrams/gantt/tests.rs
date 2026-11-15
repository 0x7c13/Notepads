use super::*;
use crate::{
    EditorExpectedSyntaxKind, EditorSemanticCompleteness, EditorSemanticRole, Engine, Error,
    MermaidConfig, OperationControl, ParseDiagnosticSpanKind, ParseOptions, RenderSemanticModel,
    SourceSpan,
};
use futures::executor::block_on;
use serde_json::json;

fn meta() -> ParseMetadata {
    ParseMetadata {
        diagram_type: "gantt".to_string(),
        config: MermaidConfig::empty_object(),
        effective_config: MermaidConfig::empty_object(),
        title: None,
    }
}

fn large_gantt_db(task_count: usize) -> GanttDb {
    let mut db = GanttDb::default();
    db.clear();
    db.set_date_format("YYYY-MM-DD");
    db.add_section("Delivery");
    for index in 0..task_count {
        let id = format!("task-{index}");
        let fields = [id.as_str(), "2026-01-01", "1d"];
        let task_info = db.parse_task_info(&fields);
        db.add_task(&format!("Task {index}"), &fields.join(","), task_info);
    }
    db
}

fn parse(text: &str) -> crate::ManagedSemanticJson {
    parse_with_site_config(text, None)
}

fn parse_with_site_config(
    text: &str,
    site_config: Option<MermaidConfig>,
) -> crate::ManagedSemanticJson {
    let engine = match site_config {
        Some(site_config) => Engine::new().with_site_config(site_config),
        None => Engine::new(),
    };
    block_on(engine.parse_diagram(text, ParseOptions::default()))
        .unwrap()
        .unwrap()
        .model
}

fn with_test_local_time_zone<R>(time_zone: crate::time::LocalTimeZone, f: impl FnOnce() -> R) -> R {
    let context = crate::runtime::RuntimePolicy::deterministic()
        .with_local_time_zone(time_zone)
        .begin_operation()
        .expect("test operation context");
    crate::runtime::with_operation_context(&context, f)
}

fn local_ms(y: i32, m0: u32, d: u32, h: u32, min: u32, s: u32) -> i64 {
    let local = CivilDate::new(y, m0 + 1, d)
        .and_then(|date| date.at_hms(h, min, s))
        .expect("valid test civil datetime");
    crate::runtime::resolve_local_datetime(local)
        .expect("test datetime is supported by the active timezone")
        .timestamp_millis()
}

fn utc_ms(y: i32, m: u32, d: u32, h: u32, min: u32, s: u32) -> i64 {
    let utc = CivilDate::new(y, m, d)
        .and_then(|date| date.at_hms(h, min, s))
        .and_then(|civil| OffsetDateTime::from_local(civil, UtcOffset::UTC))
        .expect("valid test UTC datetime");
    utc.timestamp_millis()
}

#[test]
fn gantt_entrypoints_construct_one_semantic_source() {
    let engine = Engine::new();
    let text = concat!(
        "gantt\n",
        "dateFormat YYYY-MM-DD\n",
        "section Delivery\n",
        "Build: build,2026-01-01,2d\n",
    );

    reset_gantt_syntax_construction_count();
    engine
        .parse_diagram_sync(text, ParseOptions::strict())
        .unwrap()
        .unwrap();
    assert_eq!(gantt_syntax_construction_count(), 1);

    reset_gantt_syntax_construction_count();
    engine
        .parse_diagram_for_render_model_sync(text, ParseOptions::strict())
        .unwrap()
        .unwrap();
    assert_eq!(gantt_syntax_construction_count(), 1);

    reset_gantt_syntax_construction_count();
    engine
        .parse_editor_semantic_facts_with_type_sync("gantt", text)
        .unwrap()
        .unwrap();
    assert_eq!(gantt_syntax_construction_count(), 1);
}

#[test]
fn gantt_combined_projection_constructs_once_and_matches_standalone_entrypoints() {
    let text = concat!(
        "gantt\n",
        "title Delivery roadmap\n",
        "accTitle: Delivery\n",
        "accDescr: Delivery tasks\n",
        "dateFormat YYYY-MM-DD\n",
        "section Delivery\n",
        "Build: build,2026-01-01,2d\n",
        "Ship: ship,after build,1d\n",
    );
    let meta = ParseMetadata {
        diagram_type: "gantt".to_string(),
        config: MermaidConfig::default(),
        effective_config: MermaidConfig::default(),
        title: None,
    };
    let standalone_json = parse_gantt(text, &meta).unwrap();
    reset_gantt_syntax_construction_count();
    let (combined_json, combined_editor) = crate::family::test_support::into_result(
        parse_gantt_json_and_editor_facts(text, &meta, &OperationControl::new()),
    )
    .unwrap();

    assert_eq!(gantt_syntax_construction_count(), 1);
    assert_eq!(combined_json, standalone_json);
    assert!(!combined_editor.symbols.is_empty());
}

#[test]
fn gantt_task_finalization_observes_cancellation_inside_large_task_sets() {
    let mut db = large_gantt_db(512);
    let control = OperationControl::new();
    control.cancel_after_checkpoints(2);

    assert!(db.finalize_tasks_controlled(&control).is_err());
}

#[test]
fn gantt_model_projection_observes_cancellation_inside_large_task_sets() {
    let mut db = large_gantt_db(512);
    db.finalize_tasks_controlled(&OperationControl::new())
        .unwrap()
        .unwrap();
    let control = OperationControl::new();
    control.cancel_after_checkpoints(2);

    assert!(super::parse::gantt_db_to_render_model_controlled(db, &control).is_err());
}

#[test]
fn gantt_json_projection_observes_cancellation_inside_large_task_sets() {
    let model = GanttDiagramRenderModel {
        tasks: vec![GanttRenderTask::default(); 512],
        ..GanttDiagramRenderModel::default()
    };
    let control = OperationControl::new();
    control.cancel_after_checkpoints(2);

    assert!(
        super::parse::render_model_to_compat_json_controlled(&model, &meta(), &control).is_err()
    );
}

#[test]
fn gantt_typed_projection_matches_compatibility_semantics() {
    let text = concat!(
        "gantt\n",
        "title Delivery roadmap\n",
        "accTitle: Delivery\n",
        "accDescr: Delivery tasks\n",
        "dateFormat YYYY-MM-DD\n",
        "axisFormat %Y-%m-%d\n",
        "todayMarker stroke-width:2px\n",
        "inclusiveEndDates\n",
        "section Delivery\n",
        "Build: crit,build,2026-01-01,2d\n",
        "click build href \"https://example.com/\" onBuild\n",
    );
    let effective_config = MermaidConfig::from_value(json!({ "securityLevel": "loose" }));
    let meta = ParseMetadata {
        diagram_type: "gantt".to_string(),
        config: MermaidConfig::default(),
        effective_config,
        title: None,
    };
    let compat = parse_gantt(text, &meta).unwrap();
    let typed = parse_gantt_model_for_render(text, &meta).unwrap();

    assert_eq!(render_model_to_compat_json(&typed, &meta).unwrap(), compat);
    assert_eq!(compat["type"], "gantt");
    assert!(compat["tickInterval"].is_null());
    assert_eq!(compat["links"]["build"], "https://example.com/");
    assert_eq!(compat["clickEvents"]["build"]["function_name"], "onBuild");
}

#[test]
fn gantt_typed_constraints_preserve_roles_dependencies_and_source_precision() {
    let text = concat!(
        "gantt\n",
        "dateFormat YYYY-MM-DD HH:mm\n",
        "section Delivery\n",
        "Base: base, 2026-01-01 08:30, 2h\n",
        "Peer: peer, 2026-01-01 09:00, 1h\n",
        "Ceiling: ceiling, 2026-01-02 12:45, 1h\n",
        "Single: single, after base, 30m\n",
        "Fan in: fan, after base peer, until ceiling fixed\n",
        "Fixed: fixed, 2026-01-03 01:15, 2026-01-03 03:45\n",
        "Implicit: 45m\n",
    );
    let model = parse_gantt_model_for_render(text, &meta()).unwrap();
    let task = |id: &str| {
        model
            .tasks
            .iter()
            .find(|task| task.id == id)
            .unwrap_or_else(|| panic!("missing Gantt task {id}"))
    };

    assert_eq!(
        task("base").start_constraint,
        GanttTaskStartConstraint::Fixed {
            value: "2026-01-01 08:30".to_string(),
        }
    );
    assert_eq!(
        task("base").end_constraint,
        GanttTaskEndConstraint::Duration {
            value: "2h".to_string(),
        }
    );
    assert_eq!(
        task("single").start_constraint,
        GanttTaskStartConstraint::After {
            dependency_ids: vec!["base".to_string()],
        }
    );
    assert_eq!(
        task("fan").start_constraint,
        GanttTaskStartConstraint::After {
            dependency_ids: vec!["base".to_string(), "peer".to_string()],
        }
    );
    assert_eq!(
        task("fan").end_constraint,
        GanttTaskEndConstraint::Until {
            dependency_ids: vec!["ceiling".to_string(), "fixed".to_string()],
        }
    );
    assert_eq!(
        task("fixed").end_constraint,
        GanttTaskEndConstraint::Fixed {
            value: "2026-01-03 03:45".to_string(),
        }
    );
    assert_eq!(
        task("task1").start_constraint,
        GanttTaskStartConstraint::PreviousTaskEnd {
            dependency_id: Some("fixed".to_string()),
        }
    );
    assert_eq!(
        task("task1").end_constraint,
        GanttTaskEndConstraint::Duration {
            value: "45m".to_string(),
        }
    );
}

#[test]
fn gantt_direct_task_constraints_have_explicit_json_and_reject_ambiguous_legacy_data() {
    let task = GanttRenderTask {
        start_constraint: GanttTaskStartConstraint::After {
            dependency_ids: vec!["design".to_string(), "review".to_string()],
        },
        end_constraint: GanttTaskEndConstraint::Until {
            dependency_ids: vec!["release".to_string()],
        },
        ..GanttRenderTask::default()
    };
    let mut value = serde_json::to_value(&task).unwrap();

    assert_eq!(value["startConstraint"]["kind"], "after");
    assert_eq!(
        value["startConstraint"]["dependencyIds"],
        json!(["design", "review"])
    );
    assert_eq!(value["endConstraint"]["kind"], "until");
    assert_eq!(value["endConstraint"]["dependencyIds"], json!(["release"]));

    let object = value.as_object_mut().unwrap();
    object.remove("startConstraint");
    object.remove("endConstraint");
    assert!(
        serde_json::from_value::<GanttRenderTask>(value).is_err(),
        "legacy task JSON must not silently discard constraints stored only in raw fields"
    );
}

#[test]
fn gantt_typed_constraints_round_trip_without_changing_compatibility_json() {
    let text = concat!(
        "gantt\n",
        "dateFormat YYYY-MM-DD\n",
        "section Delivery\n",
        "Base: base, 2026-01-01, 1d\n",
        "Ship: ship, after base, until base\n",
    );
    let model = parse_gantt_model_for_render(text, &meta()).unwrap();
    let typed_json = serde_json::to_value(&model).unwrap();

    assert_eq!(typed_json["tasks"][0]["sectionIndex"], 0);
    assert_eq!(typed_json["tasks"][1]["sectionIndex"], 0);
    assert_eq!(typed_json["tasks"][1]["startConstraint"]["kind"], "after");
    assert_eq!(
        typed_json["tasks"][1]["startConstraint"]["dependencyIds"],
        json!(["base"])
    );
    assert_eq!(typed_json["tasks"][1]["endConstraint"]["kind"], "until");

    let round_trip: GanttDiagramRenderModel = serde_json::from_value(typed_json).unwrap();
    assert_eq!(
        round_trip.tasks[1].start_constraint,
        model.tasks[1].start_constraint
    );
    assert_eq!(
        round_trip.tasks[1].end_constraint,
        model.tasks[1].end_constraint
    );

    let compat = render_model_to_compat_json(&model, &meta()).unwrap();
    for task in compat["tasks"].as_array().unwrap() {
        assert!(task.get("sectionIndex").is_none());
        assert!(task.get("startConstraint").is_none());
        assert!(task.get("endConstraint").is_none());
    }
    assert_eq!(
        compat["tasks"][1]["raw"]["startTime"]["startData"],
        "after base"
    );
    assert_eq!(compat["tasks"][1]["raw"]["endTime"]["data"], "until base");
}

#[test]
fn gantt_typed_projection_preserves_empty_and_header_only_output_states() {
    let meta = ParseMetadata {
        diagram_type: "gantt".to_string(),
        config: MermaidConfig::empty_object(),
        effective_config: MermaidConfig::empty_object(),
        title: None,
    };
    for source in ["", "gantt"] {
        let compat = parse_gantt(source, &meta).unwrap();
        let typed = parse_gantt_model_for_render(source, &meta).unwrap();

        assert_eq!(
            render_model_to_compat_json(&typed, &meta).unwrap(),
            compat,
            "projection drift for {source:?}"
        );
    }
}

#[test]
fn gantt_date_format_consumes_one_separator_and_preserves_extra_whitespace() {
    for (spacing, expected) in [("  ", " YYYY-MM-DD"), ("   ", "  YYYY-MM-DD")] {
        let text =
            format!("gantt\ndateFormat{spacing}YYYY-MM-DD\nsection Demo\nTask: id,2026-01-01,1d\n");
        let model = parse(&text);
        assert_eq!(model["dateFormat"], expected);

        let facts = Engine::new()
            .parse_editor_semantic_facts_with_type_sync("gantt", &text)
            .unwrap()
            .expect("gantt editor facts");
        let value_start = text.find("YYYY-MM-DD").unwrap();
        assert!(facts.symbols.iter().any(|symbol| {
            symbol.name == "YYYY-MM-DD"
                && symbol.detail.as_deref() == Some("gantt date format")
                && symbol.selection
                    == SourceSpan::new(value_start, value_start + "YYYY-MM-DD".len())
        }));
    }
}

#[test]
fn gantt_title_preserves_jison_separator_and_last_assignment_semantics() {
    for (statement, expected) in [("title   Delivery  ", "  Delivery  "), ("title  ", " ")] {
        let text = format!(
            "gantt\n{statement}\ndateFormat YYYY-MM-DD\nsection Demo\nTask: id,2026-01-01,1d\n"
        );
        let model = parse(&text);
        assert_eq!(model["title"], expected, "statement: {statement:?}");
    }

    let model = parse(concat!(
        "gantt\n",
        "title First\n",
        "title  \n",
        "dateFormat YYYY-MM-DD\n",
        "section Demo\n",
        "Task: id,2026-01-01,1d\n",
    ));
    assert_eq!(model["title"], " ");

    let unicode = concat!(
        "gantt\r\n",
        " title  😀 Delivery  \r\n",
        "dateFormat YYYY-MM-DD\r\n",
        "section Demo\r\n",
        "Task: id,2026-01-01,1d\r\n",
    );
    let model = parse(unicode);
    assert_eq!(model["title"], " 😀 Delivery  ");

    let facts = Engine::new()
        .parse_editor_semantic_facts_with_type_sync("gantt", unicode)
        .unwrap()
        .expect("gantt editor facts");
    let title = facts
        .symbols
        .iter()
        .find(|symbol| symbol.detail.as_deref() == Some("gantt title"))
        .expect("title payload");
    let payload_start = unicode.find("😀 Delivery").unwrap();
    assert_eq!(title.name, "😀 Delivery");
    assert_eq!(
        title.selection,
        SourceSpan::new(payload_start, payload_start + "😀 Delivery".len(),)
    );
}

#[test]
fn gantt_title_requires_content_after_the_separator() {
    let text = concat!(
        "gantt\n",
        "title \n",
        "dateFormat YYYY-MM-DD\n",
        "section Demo\n",
        "Task: id,2026-01-01,1d\n",
    );
    let title_start = text.find("title").unwrap();

    let Error::DiagramParse { diagnostic, .. } = Engine::new()
        .parse_diagram_sync(text, ParseOptions::strict())
        .expect_err("a bare title separator must not match the pinned Jison title token")
    else {
        panic!("a bare title separator returned a non-parse error");
    };
    assert!(
        diagnostic
            .message()
            .contains("unrecognized statement: title")
    );
    assert_eq!(
        diagnostic.span(),
        Some(SourceSpan::new(title_start, title_start + "title ".len()))
    );

    let facts = Engine::new()
        .parse_editor_semantic_facts_with_type_sync("gantt", text)
        .unwrap()
        .expect("gantt editor facts");
    assert_eq!(facts.completeness, EditorSemanticCompleteness::Recovered);
    assert!(
        facts
            .diagnostics
            .iter()
            .any(|item| item.message.contains("unrecognized statement: title"))
    );
    assert!(
        !facts
            .symbols
            .iter()
            .any(|symbol| symbol.detail.as_deref() == Some("gantt title"))
    );
}

#[test]
fn gantt_editor_facts_preserve_parser_symbol_spans() {
    let text = concat!(
        "gantt\n",
        "title Roadmap\n",
        "accTitle: Roadmap chart\n",
        "accDescr: Shows release tasks\n",
        "accDescr {\n",
        "  Shows release tasks\n",
        "  across releases\n",
        "}\n",
        "dateFormat YYYY-MM-DD\n",
        "section Demo\n",
        "Task 1: id1,2014-01-01,1d\n",
        "Task 2: id2,after id1,until id1\n",
        "click id2 call open(userId) href \"https://example.com/\"\n",
    );
    let facts = Engine::new()
        .parse_editor_semantic_facts_with_type_sync("gantt", text)
        .unwrap()
        .expect("gantt editor facts");

    assert_eq!(facts.completeness, EditorSemanticCompleteness::Complete);
    assert!(
        facts
            .directive_prefixes
            .iter()
            .any(|prefix| prefix == "dateFormat")
    );
    assert!(
        facts
            .directive_prefixes
            .iter()
            .any(|prefix| prefix == "section")
    );
    assert!(
        facts
            .directive_prefixes
            .iter()
            .any(|prefix| prefix == "click")
    );

    let title_start = text.find("Roadmap").unwrap();
    assert!(facts.symbols.iter().any(|symbol| {
        symbol.name == "Roadmap"
            && symbol.role == EditorSemanticRole::Payload
            && symbol.detail.as_deref() == Some("gantt title")
            && symbol.selection.start == title_start
            && symbol.selection.end == title_start + "Roadmap".len()
    }));

    let date_format_start = text.find("YYYY-MM-DD").unwrap();
    assert!(facts.symbols.iter().any(|symbol| {
        symbol.name == "YYYY-MM-DD"
            && symbol.role == EditorSemanticRole::Payload
            && symbol.detail.as_deref() == Some("gantt date format")
            && symbol.selection.start == date_format_start
            && symbol.selection.end == date_format_start + "YYYY-MM-DD".len()
    }));

    let acc_title_start = text.find("Roadmap chart").unwrap();
    assert!(facts.symbols.iter().any(|symbol| {
        symbol.name == "Roadmap chart"
            && symbol.role == EditorSemanticRole::Payload
            && symbol.detail.as_deref() == Some("gantt accessibility title")
            && symbol.selection.start == acc_title_start
            && symbol.selection.end == acc_title_start + "Roadmap chart".len()
    }));

    let acc_descr_start = text.find("Shows release tasks").unwrap();
    assert!(facts.symbols.iter().any(|symbol| {
        symbol.name == "Shows release tasks"
            && symbol.role == EditorSemanticRole::Payload
            && symbol.detail.as_deref() == Some("gantt accessibility description")
            && symbol.selection.start == acc_descr_start
            && symbol.selection.end == acc_descr_start + "Shows release tasks".len()
    }));

    let multiline_acc_descr_start = text.rfind("Shows release tasks").unwrap();
    let multiline_acc_descr_end = text.find("across releases").unwrap() + "across releases".len();
    assert!(facts.symbols.iter().any(|symbol| {
        symbol.name == "Shows release tasks\n  across releases"
            && symbol.role == EditorSemanticRole::Payload
            && symbol.detail.as_deref() == Some("gantt accessibility description")
            && symbol.selection.start == multiline_acc_descr_start
            && symbol.selection.end == multiline_acc_descr_end
    }));

    let section_start = text.find("Demo").unwrap();
    assert!(facts.symbols.iter().any(|symbol| {
        symbol.name == "Demo"
            && symbol.role == EditorSemanticRole::Outline
            && symbol.detail.as_deref() == Some("gantt section")
            && symbol.selection.start == section_start
            && symbol.selection.end == section_start + "Demo".len()
    }));

    let id1_def_start = text.find("id1,2014").unwrap();
    assert!(facts.symbols.iter().any(|symbol| {
        symbol.name == "id1"
            && symbol.role == EditorSemanticRole::Entity
            && symbol.detail.as_deref() == Some("gantt task")
            && symbol.selection.start == id1_def_start
            && symbol.selection.end == id1_def_start + "id1".len()
    }));

    let id1_after_start = text.find("after id1").unwrap() + "after ".len();
    assert!(facts.symbols.iter().any(|symbol| {
        symbol.name == "id1"
            && symbol.role == EditorSemanticRole::Reference
            && symbol.detail.as_deref() == Some("gantt dependency")
            && symbol.selection.start == id1_after_start
            && symbol.selection.end == id1_after_start + "id1".len()
    }));

    let id1_until_start = text.find("until id1").unwrap() + "until ".len();
    assert!(facts.symbols.iter().any(|symbol| {
        symbol.name == "id1"
            && symbol.role == EditorSemanticRole::Reference
            && symbol.detail.as_deref() == Some("gantt dependency")
            && symbol.selection.start == id1_until_start
            && symbol.selection.end == id1_until_start + "id1".len()
    }));

    let id2_def_start = text.find("id2,after").unwrap();
    assert!(facts.symbols.iter().any(|symbol| {
        symbol.name == "id2"
            && symbol.role == EditorSemanticRole::Entity
            && symbol.detail.as_deref() == Some("gantt task")
            && symbol.selection.start == id2_def_start
            && symbol.selection.end == id2_def_start + "id2".len()
    }));

    let id2_click_start = text.find("click id2").unwrap() + "click ".len();
    assert!(facts.symbols.iter().any(|symbol| {
        symbol.name == "id2"
            && symbol.role == EditorSemanticRole::Reference
            && symbol.detail.as_deref() == Some("gantt click target")
            && symbol.selection.start == id2_click_start
            && symbol.selection.end == id2_click_start + "id2".len()
    }));

    for (label, start) in [
        ("id1", id1_def_start),
        ("id1", id1_after_start),
        ("id1", id1_until_start),
        ("id2", id2_def_start),
        ("id2", id2_click_start),
    ] {
        assert!(
            facts.expected_syntax.iter().any(|expected| {
                expected.kind == EditorExpectedSyntaxKind::NodeIdentifier
                    && expected.span.start <= start
                    && expected.span.end >= start + label.len()
            }),
            "missing gantt node-id expected syntax for {label:?} at {start}"
        );
    }

    let callback_start = text.find("open(userId)").unwrap();
    assert!(facts.symbols.iter().any(|symbol| {
        symbol.name == "open"
            && symbol.role == EditorSemanticRole::Payload
            && symbol.detail.as_deref() == Some("gantt click callback")
            && symbol.selection.start == callback_start
            && symbol.selection.end == callback_start + "open".len()
    }));

    let callback_args_start = text.find("userId").unwrap();
    assert!(facts.symbols.iter().any(|symbol| {
        symbol.name == "userId"
            && symbol.role == EditorSemanticRole::Payload
            && symbol.detail.as_deref() == Some("gantt click callback args")
            && symbol.selection.start == callback_args_start
            && symbol.selection.end == callback_args_start + "userId".len()
    }));

    let href_start = text.find("https://example.com/").unwrap();
    assert!(facts.symbols.iter().any(|symbol| {
        symbol.name == "https://example.com/"
            && symbol.role == EditorSemanticRole::Payload
            && symbol.detail.as_deref() == Some("gantt click href")
            && symbol.selection.start == href_start
            && symbol.selection.end == href_start + "https://example.com/".len()
    }));

    for payload in [
        "Roadmap",
        "YYYY-MM-DD",
        "open",
        "userId",
        "https://example.com/",
    ] {
        let start = text.find(payload).unwrap();
        assert!(
            facts.expected_syntax.iter().any(|expected| {
                expected.kind == EditorExpectedSyntaxKind::Payload
                    && expected.span.start <= start
                    && expected.span.end >= start + payload.len()
            }),
            "missing gantt payload expected syntax for {payload:?}"
        );
    }
}

#[test]
fn gantt_forward_dependency_is_a_reference_before_its_task_definition() {
    let source = concat!(
        "gantt\n",
        "dateFormat YYYY-MM-DD\n",
        "Forward: early,after later,1d\n",
        "Later: later,2026-01-01,1d\n",
    );
    let facts = Engine::new()
        .parse_editor_semantic_facts_with_type_sync("gantt", source)
        .unwrap()
        .expect("gantt editor facts");
    let later: Vec<_> = facts
        .symbols
        .iter()
        .filter(|symbol| symbol.name == "later")
        .collect();

    assert_eq!(later.len(), 2);
    assert_eq!(later[0].role, EditorSemanticRole::Reference);
    assert_eq!(later[1].role, EditorSemanticRole::Entity);
    assert!(
        facts
            .symbols
            .iter()
            .any(|symbol| { symbol.name == "early" && symbol.role == EditorSemanticRole::Entity })
    );
}

#[test]
fn gantt_editor_facts_recovers_unclosed_multiline_acc_descr_payload() {
    let text = concat!("gantt\n", "accDescr {\n", "  Draft release notes\n");
    let facts = Engine::new()
        .parse_editor_semantic_facts_with_type_sync("gantt", text)
        .unwrap()
        .expect("gantt editor facts");

    assert_eq!(facts.completeness, EditorSemanticCompleteness::Recovered);
    assert_eq!(facts.diagnostics.len(), 1);
    let diagnostic = &facts.diagnostics[0];
    assert!(diagnostic.message.contains("unterminated accDescr block"));
    let insertion = text.trim_end().len();
    assert_eq!(
        diagnostic.span.unwrap(),
        crate::SourceSpan::new(insertion, insertion)
    );

    let note_start = text.find("Draft release notes").unwrap();
    assert!(facts.symbols.iter().any(|symbol| {
        symbol.name == "Draft release notes"
            && symbol.role == EditorSemanticRole::Payload
            && symbol.detail.as_deref() == Some("gantt accessibility description")
            && symbol.selection.start == note_start
            && symbol.selection.end == note_start + "Draft release notes".len()
    }));
}

#[test]
fn gantt_editor_facts_recovers_from_incomplete_input() {
    let text = concat!(
        "gantt\n",
        "dateFormat YYYY-MM-DD\n",
        "Task 1: id1,2014-01-01,1d\n",
        "Task 2",
    );
    let facts = Engine::new()
        .parse_editor_semantic_facts_with_type_sync("gantt", text)
        .unwrap()
        .expect("gantt editor facts");

    assert_eq!(facts.completeness, EditorSemanticCompleteness::Recovered);
    let task_start = text.find("Task 2").unwrap();
    assert!(facts.diagnostics.iter().any(|diagnostic| {
        diagnostic.message.contains("unrecognized statement")
            && diagnostic.span == Some(SourceSpan::new(task_start, text.len()))
    }));
    assert!(
        facts.symbols.iter().any(|symbol| {
            symbol.name == "id1" && symbol.detail.as_deref() == Some("gantt task")
        })
    );
    assert!(
        facts
            .directive_prefixes
            .iter()
            .any(|prefix| prefix == "dateFormat")
    );
}

#[test]
fn gantt_editor_facts_skip_leading_mermaid_directives() {
    let text = concat!(
        "%%{init: {\"theme\": \"dark\"}}%%\n",
        "gantt\n",
        "Task 1: id1,2014-01-01,1d\n",
    );
    let facts = Engine::new()
        .parse_editor_semantic_facts_with_type_sync("gantt", text)
        .unwrap()
        .expect("gantt editor facts");

    assert_eq!(facts.completeness, EditorSemanticCompleteness::Complete);
    assert!(
        facts
            .directive_prefixes
            .iter()
            .any(|prefix| prefix == "init")
    );
    assert!(
        facts.symbols.iter().any(|symbol| {
            symbol.name == "id1" && symbol.detail.as_deref() == Some("gantt task")
        })
    );
}

#[test]
fn gantt_editor_facts_reports_invalid_weekday_diagnostic() {
    let text = "gantt\nweekday foo\n";
    reset_gantt_syntax_construction_count();
    let facts = Engine::new()
        .parse_editor_semantic_facts_with_type_sync("gantt", text)
        .unwrap()
        .expect("gantt editor facts");

    assert_eq!(gantt_syntax_construction_count(), 1);
    assert_eq!(facts.completeness, EditorSemanticCompleteness::Recovered);
    let value_start = text.find("foo").unwrap();
    assert!(facts.diagnostics.iter().any(|diagnostic| {
        diagnostic.message.contains("invalid weekday")
            && diagnostic.span == Some(SourceSpan::new(value_start, value_start + "foo".len()))
    }));

    reset_gantt_syntax_construction_count();
    let Error::DiagramParse { diagnostic, .. } = Engine::new()
        .parse_diagram_sync(text, ParseOptions::strict())
        .expect_err("invalid weekday must fail strict Gantt parsing")
    else {
        panic!("invalid weekday returned a non-parse error");
    };
    assert_eq!(gantt_syntax_construction_count(), 1);
    assert_eq!(
        diagnostic.span(),
        Some(SourceSpan::new(value_start, value_start + "foo".len()))
    );
    assert_eq!(diagnostic.span_kind(), ParseDiagnosticSpanKind::Exact);
}

#[test]
fn gantt_editor_facts_skip_frontmatter() {
    let text = concat!(
        "---\n",
        "title: Roadmap\n",
        "---\n",
        "gantt\n",
        "Task 1: id1,2014-01-01,1d\n",
    );
    let facts = Engine::new()
        .parse_editor_semantic_facts_with_type_sync("gantt", text)
        .unwrap()
        .expect("gantt editor facts");

    assert_eq!(facts.completeness, EditorSemanticCompleteness::Complete);
    assert!(
        facts.symbols.iter().any(|symbol| {
            symbol.name == "id1" && symbol.detail.as_deref() == Some("gantt task")
        })
    );
}

#[test]
fn gantt_render_model_uses_fixed_today_for_missing_year_dates() {
    let engine = Engine::new()
        .with_fixed_today(Some(
            CivilDate::new(2026, 2, 15).expect("valid fixed today"),
        ))
        .try_with_fixed_local_offset_minutes(0)
        .expect("UTC is a valid fixed offset");
    let parsed = block_on(engine.parse_diagram_for_render_model(
        r#"
gantt
dateFormat MM-DD
section Demo
Missing year: id1,03-01,1d
Missing ref: id2,after missing,1d
"#,
        ParseOptions::default(),
    ))
    .unwrap()
    .unwrap();
    let RenderSemanticModel::Gantt(model) = parsed.model() else {
        panic!("expected Gantt render model");
    };
    let task = |id: &str| {
        model
            .tasks
            .iter()
            .find(|task| task.id == id)
            .unwrap_or_else(|| panic!("missing Gantt task {id}"))
    };

    assert_eq!(task("id1").start_ms, 1_772_323_200_000);
    assert_eq!(task("id2").start_ms, 1_771_113_600_000);
}

#[test]
fn gantt_fixed_dates() {
    let model = parse(
        r#"
gantt
dateFormat YYYY-MM-DD
section testa1
test1: id1,2013-01-01,2013-01-12
"#,
    );
    let t0 = &model["tasks"][0];
    assert_eq!(t0["id"].as_str().unwrap(), "id1");
    assert_eq!(t0["task"].as_str().unwrap(), "test1");
    assert_eq!(
        t0["startTime"].as_i64().unwrap(),
        local_ms(2013, 0, 1, 0, 0, 0)
    );
    assert_eq!(
        t0["endTime"].as_i64().unwrap(),
        local_ms(2013, 0, 12, 0, 0, 0)
    );
}

#[test]
fn gantt_duration_units() {
    let model = parse(
        r#"
gantt
dateFormat YYYY-MM-DD
section testa1
test1: id1,2013-01-01,2h
"#,
    );
    let t0 = &model["tasks"][0];
    assert_eq!(
        t0["startTime"].as_i64().unwrap(),
        local_ms(2013, 0, 1, 0, 0, 0)
    );
    assert_eq!(
        t0["endTime"].as_i64().unwrap(),
        local_ms(2013, 0, 1, 2, 0, 0)
    );
}

#[test]
fn gantt_relative_after_id() {
    let model = parse(
        r#"
gantt
dateFormat YYYY-MM-DD
section sec1
test1: id1,2013-01-01,2w
test2: id2,after id1,1d
"#,
    );
    let tasks = model["tasks"].as_array().unwrap();
    assert_eq!(
        tasks[1]["startTime"].as_i64().unwrap(),
        local_ms(2013, 0, 15, 0, 0, 0)
    );
    assert_eq!(
        tasks[1]["endTime"].as_i64().unwrap(),
        local_ms(2013, 0, 16, 0, 0, 0)
    );
}

#[test]
fn gantt_relative_until_id() {
    let model = parse(
        r#"
gantt
dateFormat YYYY-MM-DD
section sec1
task1: id1,2013-01-01,until id3
section sec2
task3: id3,2013-02-01,2d
"#,
    );
    let tasks = model["tasks"].as_array().unwrap();
    assert_eq!(
        tasks[0]["endTime"].as_i64().unwrap(),
        local_ms(2013, 1, 1, 0, 0, 0)
    );
}

#[test]
fn gantt_excludes_weekends_and_specific_days() {
    let model = parse(
        r#"
gantt
dateFormat YYYY-MM-DD
excludes weekends 2019-02-06,friday
section weekends skip test
test1: id1,2019-02-01,1d
test2: id2,after id1,2d
"#,
    );
    let tasks = model["tasks"].as_array().unwrap();
    assert_eq!(
        tasks[0]["startTime"].as_i64().unwrap(),
        local_ms(2019, 1, 1, 0, 0, 0)
    );
    assert_eq!(
        tasks[0]["endTime"].as_i64().unwrap(),
        local_ms(2019, 1, 4, 0, 0, 0)
    );
    assert_eq!(
        tasks[0]["renderEndTime"].as_i64().unwrap(),
        local_ms(2019, 1, 2, 0, 0, 0)
    );
    assert_eq!(
        tasks[1]["startTime"].as_i64().unwrap(),
        local_ms(2019, 1, 4, 0, 0, 0)
    );
    assert_eq!(
        tasks[1]["endTime"].as_i64().unwrap(),
        local_ms(2019, 1, 7, 0, 0, 0)
    );
    assert_eq!(
        tasks[1]["renderEndTime"].as_i64().unwrap(),
        local_ms(2019, 1, 6, 0, 0, 0)
    );
}

#[test]
fn gantt_merges_multiple_includes_and_excludes_in_first_seen_order() {
    let model = parse(
        r#"
gantt
dateFormat YYYY-MM-DD
excludes weekends,2019-02-06
excludes WEEKENDS friday
excludes monday,2019-02-06
includes 2019-02-09
includes 2019-02-10,2019-02-09
"#,
    );

    assert_eq!(
        model["excludes"],
        json!(["weekends", "2019-02-06", "friday", "monday"])
    );
    assert_eq!(model["includes"], json!(["2019-02-09", "2019-02-10"]));
}

#[test]
fn gantt_inclusive_end_dates_adds_one_day_for_strict_dates() {
    let model = parse(
        r#"
gantt
dateFormat YYYY-MM-DD
inclusiveEndDates
test1: id1,2019-02-01,1d
test2: id2,2019-02-01,2019-02-03
"#,
    );
    let tasks = model["tasks"].as_array().unwrap();
    assert_eq!(
        tasks[0]["endTime"].as_i64().unwrap(),
        local_ms(2019, 1, 2, 0, 0, 0)
    );
    assert_eq!(
        tasks[1]["endTime"].as_i64().unwrap(),
        local_ms(2019, 1, 4, 0, 0, 0)
    );
    assert!(tasks[1]["renderEndTime"].is_null());
    assert!(tasks[1]["manualEndTime"].as_bool().unwrap());
}

#[test]
fn gantt_rejects_ridiculous_years() {
    let engine = Engine::new();
    let err = block_on(engine.parse_diagram(
        r#"
gantt
dateFormat YYYYMMDD
test1: id1,202304,1d
"#,
        ParseOptions::default(),
    ))
    .unwrap_err();
    assert!(err.to_string().contains("Invalid date:202304"));
}

#[test]
fn gantt_js_date_fallback_year_bounds_match_upstream_guardrail() {
    let utc = crate::time::LocalTimeZone::utc();
    with_test_local_time_zone(utc, || {
        let dt = parse_js_date_fallback("10000").unwrap();
        assert_eq!(dt.year(), 10000);

        let err = parse_js_date_fallback("10001").unwrap_err();
        assert!(err.to_string().contains("Invalid date:10001"));
    });
}

#[test]
fn gantt_js_date_fallback_keeps_ecmascript_expanded_years_distinct_from_civil_dates() {
    let time_zone = crate::time::LocalTimeZone::fixed(8 * 60).unwrap();
    with_test_local_time_zone(time_zone, || {
        for invalid in [
            "-0001-01-01",
            "+0001-01-01",
            "+10000-01-01",
            "-10000-01-01",
            "+010001-01-01",
            "-000000-01-01",
            "-000000-01-01T00:00:00",
            "-000000-01-01T00:00:00Z",
            "-000000-01-01T00:00:00+08:00",
        ] {
            assert!(parse_js_date_fallback(invalid).is_err(), "{invalid}");
        }

        assert_eq!(
            parse_js_date_fallback("+002024-01-01")
                .unwrap()
                .timestamp_millis(),
            utc_ms(2024, 1, 1, 0, 0, 0)
        );
        assert_eq!(parse_js_date_fallback("-000001-01-01").unwrap().year(), -1);
        assert_eq!(
            parse_js_date_fallback("+010000-01-01").unwrap().year(),
            10_000
        );
        assert_eq!(
            parse_js_date_fallback("-010000-01-01").unwrap().year(),
            -10_000
        );

        assert_eq!(
            parse_js_date_fallback("+002024-01-01T00:00:00")
                .unwrap()
                .timestamp_millis(),
            local_ms(2024, 0, 1, 0, 0, 0)
        );
        assert_eq!(
            parse_js_date_fallback("+002024-01-01T00:00:00Z")
                .unwrap()
                .timestamp_millis(),
            utc_ms(2024, 1, 1, 0, 0, 0)
        );
    });
}

#[cfg(feature = "system-timezone")]
#[test]
fn gantt_js_date_fallback_supports_upper_guardrail_in_system_timezone() {
    let system = crate::time::LocalTimeZone::try_system().expect("system time-zone adapter");
    with_test_local_time_zone(system, || {
        let dt = parse_js_date_fallback("10000").unwrap();
        assert_eq!(dt.year(), 10000);
    });
}

#[test]
fn gantt_js_date_fallback_parses_mdy_hm_strings_like_v8() {
    // Mermaid's gantt parser falls back to `new Date(str)`; in V8, the string
    // `08-08-09-01:00` parses as local time `2009-08-08 01:00`.
    let dt = parse_js_date_fallback("08-08-09-01:00").unwrap();
    assert_eq!(dt.year(), 2009);
    assert_eq!(dt.month(), 8);
    assert_eq!(dt.day(), 8);
    assert_eq!(dt.hour(), 1);
    assert_eq!(dt.minute(), 0);
}

#[test]
fn gantt_js_date_fallback_rejects_invalid_calendar_dates() {
    let err = parse_js_date_fallback("2019-02-30").unwrap_err();
    assert!(err.to_string().contains("Invalid date:2019-02-30"));
}

#[test]
fn gantt_parse_duration_matches_upstream_examples() {
    assert_eq!(parse_duration("1d"), (1.0, "d".to_string()));
    assert!(parse_duration("1f").0.is_nan());
    assert_eq!(parse_duration("0.1s"), (0.1, "s".to_string()));
    assert_eq!(parse_duration("1ms"), (1.0, "ms".to_string()));
    assert_eq!(parse_duration(" 2m "), (2.0, "m".to_string()));
    assert_eq!(parse_duration("01M"), (1.0, "M".to_string()));

    for invalid in [".1s", "1.s", "1MS", "1msx", "1 ms", ""] {
        let (value, unit) = parse_duration(invalid);
        assert!(value.is_nan(), "expected invalid duration for {invalid:?}");
        assert_eq!(unit, "ms");
    }
}

#[test]
fn gantt_duration_arithmetic_rejects_unrepresentable_ranges_without_panicking() {
    let utc = crate::time::LocalTimeZone::utc();
    with_test_local_time_zone(utc, || {
        let base = local_from_civil(CivilDate::new(2026, 1, 1).unwrap().at_midnight()).unwrap();
        let max = OffsetDateTime::from_unix_millis(i64::MAX, UtcOffset::UTC);

        assert!(add_days_local(base, i64::MIN).is_none());
        assert!(add_months_local(base, i64::MAX).is_none());
        assert!(add_months_local(base, i64::MIN).is_none());
        assert!(add_years_local(base, i64::MAX).is_none());
        assert!(add_years_local(base, i64::MIN).is_none());
        let db = GanttDb::default();
        assert_eq!(
            get_end_date(&db, max, "x", "1ms", false).unwrap(),
            Some(max)
        );
        assert_eq!(
            get_end_date(&db, base, "x", "1000000000000000000000000000000M", false).unwrap(),
            Some(base)
        );
        assert_eq!(
            get_end_date(&db, base, "x", "1000000000000000000000000000000y", false).unwrap(),
            Some(base)
        );
    });
}

#[test]
fn gantt_huge_calendar_durations_use_the_existing_zero_duration_fallback() {
    for duration in ["9223372036854775808M", "999999999999999999999999999999999y"] {
        let model = parse(&format!(
            "gantt\ndateFormat x\nsection Boundary\nTask: task,0,{duration}\n"
        ));
        let task = &model["tasks"][0];
        assert_eq!(task["startTime"], task["endTime"], "duration {duration}");
    }
}

#[test]
fn gantt_javascript_time_boundary_supports_one_millisecond_duration() {
    let max_ms = JS_MAX_TIME_MILLIS - 1;
    let source = format!("gantt\ndateFormat x\nsection Boundary\nTask: task,{max_ms},1ms\n");
    let utc = crate::time::LocalTimeZone::utc();
    let parsed = with_test_local_time_zone(utc, || {
        block_on(Engine::new().parse_diagram(&source, ParseOptions::default()))
    })
    .expect("one millisecond remains representable at the JavaScript Date boundary")
    .expect("gantt detected");
    let task = &parsed.model["tasks"][0];
    assert_eq!(task["startTime"], max_ms);
    assert_eq!(task["endTime"], max_ms + 1);
}

#[test]
fn gantt_javascript_time_clip_accepts_only_the_inclusive_endpoints() {
    for milliseconds in [-JS_MAX_TIME_MILLIS, JS_MAX_TIME_MILLIS] {
        let datetime = js_date_from_unix_millis(milliseconds)
            .expect("JavaScript TimeClip endpoint must remain representable");
        assert_eq!(datetime.timestamp_millis(), milliseconds);
    }

    assert!(js_date_from_unix_millis(-JS_MAX_TIME_MILLIS - 1).is_none());
    assert!(js_date_from_unix_millis(JS_MAX_TIME_MILLIS + 1).is_none());
}

#[test]
fn gantt_weekends_can_start_on_friday() {
    let model = parse(
        r#"
gantt
dateFormat YYYY-MM-DD
excludes weekends
weekend friday
section friday-saturday weekends skip test
test1: id1,2024-02-28,3d
"#,
    );
    let t0 = &model["tasks"][0];
    assert_eq!(
        t0["startTime"].as_i64().unwrap(),
        local_ms(2024, 1, 28, 0, 0, 0)
    );
    assert_eq!(
        t0["endTime"].as_i64().unwrap(),
        local_ms(2024, 2, 4, 0, 0, 0)
    );
}

#[test]
fn gantt_seconds_only_date_format_is_accepted() {
    let model = parse(
        r#"
gantt
dateFormat ss
section Network Request
RTT: rtt, 0, 20
"#,
    );
    let tasks = model["tasks"].as_array().unwrap();
    assert_eq!(tasks.len(), 1);
    assert_eq!(tasks[0]["task"].as_str().unwrap(), "RTT");
    assert_eq!(tasks[0]["id"].as_str().unwrap(), "rtt");
}

#[test]
fn gantt_date_year_typos_fall_back_to_js_date_parsing() {
    let model = parse(
        r#"
gantt
dateFormat YYYY-MM-DD
section Vacation
London Trip 1: 2024-12-01, 7d
London Trip 2: 202-12-01, 7d
"#,
    );
    let tasks = model["tasks"].as_array().unwrap();
    assert_eq!(tasks.len(), 2);

    let ms0 = tasks[0]["startTime"].as_i64().unwrap();
    let ms1 = tasks[1]["startTime"].as_i64().unwrap();
    let dt0 = OffsetDateTime::from_unix_millis(ms0, UtcOffset::UTC);
    let dt1 = OffsetDateTime::from_unix_millis(ms1, UtcOffset::UTC);
    assert_eq!(dt0.year(), 2024);
    assert_eq!(dt1.year(), 202);
}

#[test]
fn gantt_preserves_task_creation_order() {
    let model = parse(
        r#"
gantt
dateFormat YYYY-MM-DD
section A
Completed task: done, des1, 2014-01-06,2014-01-08
Active task: active, des2, 2014-01-09, 3d
section B
Future task: des3, after des2, 5d
"#,
    );
    let tasks = model["tasks"].as_array().unwrap();
    assert_eq!(tasks[0]["order"].as_i64().unwrap(), 0);
    assert_eq!(tasks[1]["order"].as_i64().unwrap(), 1);
    assert_eq!(tasks[2]["order"].as_i64().unwrap(), 2);
    assert!(tasks[0]["done"].as_bool().unwrap());
    assert!(tasks[1]["active"].as_bool().unwrap());
    assert_eq!(tasks[0]["id"].as_str().unwrap(), "des1");
    assert_eq!(tasks[1]["id"].as_str().unwrap(), "des2");
    assert_eq!(tasks[2]["id"].as_str().unwrap(), "des3");
}

#[test]
fn gantt_vertical_markers_do_not_consume_task_row_order() {
    let model = parse(
        r#"
gantt
dateFormat YYYY-MM-DD
section Delivery
Task one: task-1,2024-01-01,1d
Release marker: vert,marker-1,2024-01-02,1d
Task two: task-2,2024-01-03,1d
Review marker: vert,marker-2,2024-01-04,1d
Task three: task-3,2024-01-05,1d
"#,
    );
    let tasks = model["tasks"].as_array().unwrap();

    assert_eq!(
        tasks
            .iter()
            .map(|task| (
                task["id"].as_str().unwrap(),
                task["order"].as_i64().unwrap()
            ))
            .collect::<Vec<_>>(),
        vec![
            ("task-1", 0),
            ("marker-1", -1),
            ("task-2", 1),
            ("marker-2", -1),
            ("task-3", 2),
        ]
    );
}

#[test]
fn gantt_date_format_custom_separators_parse_strict() {
    let model = parse(
        r#"
gantt
dateFormat YYYY/MM/DD
section testa1
test1: id1,2013/01/01,2013/01/12
"#,
    );
    let t0 = &model["tasks"][0];
    assert_eq!(t0["id"].as_str().unwrap(), "id1");
    assert_eq!(
        t0["startTime"].as_i64().unwrap(),
        local_ms(2013, 0, 1, 0, 0, 0)
    );
    assert_eq!(
        t0["endTime"].as_i64().unwrap(),
        local_ms(2013, 0, 12, 0, 0, 0)
    );
}

#[test]
fn dayjs_strict_parses_month_names_and_offsets() {
    let dt = parse_dayjs_like_strict("YYYY-MMM-DD", "2013-Jan-02").unwrap();
    assert_eq!(dt.timestamp_millis(), local_ms(2013, 0, 2, 0, 0, 0));

    let dt = parse_dayjs_like_strict("YYYY-MM-DDTHH:mm:ssZ", "2013-01-01T00:00:00+00:00").unwrap();
    assert_eq!(dt.timestamp_millis(), utc_ms(2013, 1, 1, 0, 0, 0));

    let dt = parse_dayjs_like_strict("YYYY-MM-DDTHH:mm:ssZ", "2013-01-01T00:00:00+08:00").unwrap();
    assert_eq!(dt.timestamp_millis(), utc_ms(2012, 12, 31, 16, 0, 0));

    let dt = parse_dayjs_like_strict("YYYY-MM-DDTHH:mm:ssZZ", "2013-01-01T00:00:00+0800").unwrap();
    assert_eq!(dt.timestamp_millis(), utc_ms(2012, 12, 31, 16, 0, 0));

    let dt = parse_dayjs_like_strict("YYYY-MM-DDTHH:mm:ssZ", "2013-01-01T00:00:00Z").unwrap();
    assert_eq!(dt.timestamp_millis(), utc_ms(2013, 1, 1, 0, 0, 0));
}

#[test]
fn gantt_date_parsers_reject_utf8_at_byte_cutoffs_without_panicking() {
    assert!(parse_dayjs_like_strict("YYYY", "你好").is_none());
    assert!(parse_js_date_fallback("2024-01-01T00:你").is_err());
}

#[test]
fn gantt_js_date_fallback_parses_iso_date_only_as_utc() {
    let model = parse(
        r#"
gantt
dateFormat YYYYMMDD
section A
test1: id1,2013-01-01,1d
"#,
    );
    let t0 = &model["tasks"][0];
    assert_eq!(
        t0["startTime"].as_i64().unwrap(),
        utc_ms(2013, 1, 1, 0, 0, 0)
    );
    assert_eq!(t0["endTime"].as_i64().unwrap(), utc_ms(2013, 1, 2, 0, 0, 0));
}

#[test]
fn dayjs_strict_rejects_rather_than_panics_on_multibyte_input() {
    // `parse_int_exact` used to slice at a byte offset without checking char
    // boundaries, panicking on multi-byte (e.g. Japanese) input.
    assert!(parse_dayjs_like_strict("YYYY-MM-DD", "日本語のテキスト").is_none());
    assert!(parse_dayjs_like_strict("MM-DD", "日本-24").is_none());
    assert!(parse_dayjs_like_strict("YYYY-MMM-DD", "２０１３-Jan-０２").is_none());
}

#[test]
fn js_date_fallback_rejects_rather_than_panics_on_multibyte_timezone_suffix() {
    // Same byte-boundary bug as above, reached via the timezone-offset tail
    // of `parse_js_date_fallback`.
    assert!(parse_js_date_fallback("2013-01-01T00:00:00+日本語").is_err());
    assert!(parse_js_date_fallback("2013-01-01T00:00+タ").is_err());
}

#[test]
fn gantt_js_date_fallback_parses_iso_datetime_without_tz_as_local() {
    let model = parse(
        r#"
gantt
dateFormat YYYYMMDD
section A
test1: id1,2013-01-01T00:00:00,1d
"#,
    );
    let t0 = &model["tasks"][0];
    assert_eq!(
        t0["startTime"].as_i64().unwrap(),
        local_ms(2013, 0, 1, 0, 0, 0)
    );
}

#[test]
fn gantt_js_date_fallback_parses_timezone_offsets_with_or_without_colon() {
    let dt = parse_js_date_fallback("2013-01-01T00:00:00+0800").unwrap();
    assert_eq!(dt.timestamp_millis(), utc_ms(2012, 12, 31, 16, 0, 0));

    let dt = parse_js_date_fallback("2013-01-01T00:00:00+08:00").unwrap();
    assert_eq!(dt.timestamp_millis(), utc_ms(2012, 12, 31, 16, 0, 0));

    let dt = parse_js_date_fallback("2013-01-01T00:00:00Z").unwrap();
    assert_eq!(dt.timestamp_millis(), utc_ms(2013, 1, 1, 0, 0, 0));
}

#[test]
fn gantt_js_date_fallback_parses_slash_dates_as_local() {
    let dt = parse_js_date_fallback("2013/01/01").unwrap();
    assert_eq!(dt.timestamp_millis(), local_ms(2013, 0, 1, 0, 0, 0));

    let dt = parse_js_date_fallback("2013/01/01 00:00:00").unwrap();
    assert_eq!(dt.timestamp_millis(), local_ms(2013, 0, 1, 0, 0, 0));
}

#[test]
fn gantt_excludes_weekday_names_use_full_names() {
    let model = parse(
        r#"
gantt
dateFormat YYYY-MM-DD
excludes friday
section A
test1: id1,2019-02-07,2d
"#,
    );
    let t0 = &model["tasks"][0];
    assert_eq!(
        t0["startTime"].as_i64().unwrap(),
        local_ms(2019, 1, 7, 0, 0, 0)
    );
    assert_eq!(
        t0["endTime"].as_i64().unwrap(),
        local_ms(2019, 1, 10, 0, 0, 0)
    );
    assert_eq!(
        t0["renderEndTime"].as_i64().unwrap(),
        local_ms(2019, 1, 10, 0, 0, 0)
    );
}

#[test]
fn gantt_rejects_excludes_that_cover_every_weekday() {
    let engine = Engine::new();
    let err = block_on(engine.parse_diagram(
        r#"
gantt
dateFormat YYYY-MM-DD
excludes weekends,monday,tuesday,wednesday,thursday,friday
section A
task :a, 2025-01-01, 1d
"#,
        ParseOptions::default(),
    ))
    .unwrap_err();

    assert!(
        err.to_string().contains("excludes every weekday"),
        "unexpected error: {err}"
    );
}

#[test]
fn gantt_rejects_long_exclude_runs_instead_of_looping() {
    let engine = Engine::new();
    let err = block_on(engine.parse_diagram(
        r#"
gantt
dateFormat YYYY-MM-DD
excludes weekends,monday,tuesday,wednesday,thursday,friday,2025-01-08
includes 2026-01-09
section A
task :a, 2025-01-01, 1d
"#,
        ParseOptions::default(),
    ))
    .unwrap_err();

    assert!(
        err.to_string()
            .contains("no includable date found within one year"),
        "unexpected error: {err}"
    );
}

#[test]
fn gantt_click_call_is_ignored_unless_security_level_loose() {
    let model = parse(
        r#"
gantt
dateFormat YYYY-MM-DD
section A
task: id1, 2013-01-01, 1d
click id1 call myFn("a,b", c)
"#,
    );
    assert!(model["clickEvents"].as_object().unwrap().is_empty());
}

#[test]
fn gantt_click_call_parses_args_and_defaults_to_id() {
    let model = parse_with_site_config(
        r#"
gantt
dateFormat YYYY-MM-DD
section A
task: id1, 2013-01-01, 1d
task2: id2, 2013-01-02, 1d
click id2 call myFn("a,b", c)
click id1 call myFn2()
"#,
        Some(MermaidConfig::from_value(json!({
            "securityLevel": "loose"
        }))),
    );
    let ev1 = &model["clickEvents"]["id1"];
    assert_eq!(ev1["function_name"].as_str().unwrap(), "myFn2");
    assert_eq!(ev1["function_args"][0].as_str().unwrap(), "id1");
    assert!(ev1["raw_function_args"].is_null());

    let ev2 = &model["clickEvents"]["id2"];
    assert_eq!(ev2["function_name"].as_str().unwrap(), "myFn");
    assert_eq!(ev2["function_args"][0].as_str().unwrap(), "a,b");
    assert_eq!(ev2["function_args"][1].as_str().unwrap(), "c");
    assert_eq!(ev2["raw_function_args"].as_str().unwrap(), "\"a,b\", c");
}

#[test]
fn gantt_click_bare_callback_parses_like_mermaid() {
    let model = parse_with_site_config(
        r#"
gantt
dateFormat YYYY-MM-DD
section A
task: id1, 2013-01-01, 1d
task2: id2, 2013-01-02, 1d
click id1 myFn
click id2 href "https://example.com/" myOtherFn
"#,
        Some(MermaidConfig::from_value(json!({
            "securityLevel": "loose"
        }))),
    );

    let ev1 = &model["clickEvents"]["id1"];
    assert_eq!(ev1["function_name"].as_str().unwrap(), "myFn");
    assert_eq!(ev1["function_args"][0].as_str().unwrap(), "id1");

    let ev2 = &model["clickEvents"]["id2"];
    assert_eq!(ev2["function_name"].as_str().unwrap(), "myOtherFn");
    assert_eq!(ev2["function_args"][0].as_str().unwrap(), "id2");
    assert_eq!(
        model["links"]["id2"].as_str().unwrap(),
        "https://example.com/"
    );
}

#[test]
fn gantt_click_bare_callback_can_start_with_keyword_prefix() {
    let model = parse_with_site_config(
        r#"
gantt
dateFormat YYYY-MM-DD
section A
task: id1, 2013-01-01, 1d
task2: id2, 2013-01-02, 1d
click id1 hrefHandler
click id2 callHandler
"#,
        Some(MermaidConfig::from_value(json!({
            "securityLevel": "loose"
        }))),
    );

    let ev1 = &model["clickEvents"]["id1"];
    assert_eq!(ev1["function_name"].as_str().unwrap(), "hrefHandler");
    assert_eq!(ev1["function_args"][0].as_str().unwrap(), "id1");

    let ev2 = &model["clickEvents"]["id2"];
    assert_eq!(ev2["function_name"].as_str().unwrap(), "callHandler");
    assert_eq!(ev2["function_args"][0].as_str().unwrap(), "id2");
}

#[test]
fn gantt_click_href_sanitizes_unsafe_urls_for_new_callback_forms() {
    let model = parse_with_site_config(
        r#"
gantt
dateFormat YYYY-MM-DD
section A
task: id1, 2013-01-01, 1d
click id1 href "javascript:alert(1)" myFn
"#,
        Some(MermaidConfig::from_value(json!({
            "securityLevel": "strict"
        }))),
    );

    assert_eq!(model["links"]["id1"].as_str().unwrap(), "about:blank");
    assert!(model["clickEvents"].as_object().unwrap().is_empty());
}

#[test]
fn gantt_click_href_accepts_quoted_tooltip_without_callback() {
    let model = parse_with_site_config(
        r#"
gantt
dateFormat YYYY-MM-DD
section A
task: id1, 2013-01-01, 1d
click id1 href "https://example.com/" "open details"
"#,
        Some(MermaidConfig::from_value(json!({
            "securityLevel": "loose"
        }))),
    );

    assert_eq!(
        model["links"]["id1"].as_str().unwrap(),
        "https://example.com/"
    );
    assert!(model["clickEvents"].as_object().unwrap().is_empty());
}

#[test]
fn gantt_task_text_starting_with_click_is_not_a_click_statement() {
    let model = parse(
        r#"
gantt
dateFormat YYYY-MM-DD
section A
Clickable task: id1, 2026-01-01, 1d
clickhouse migration: id2, 2026-01-02, 1d
"#,
    );

    let tasks = model["tasks"].as_array().unwrap();
    assert_eq!(tasks.len(), 2);
    assert_eq!(tasks[0]["task"].as_str().unwrap(), "Clickable task");
    assert_eq!(tasks[1]["task"].as_str().unwrap(), "clickhouse migration");
    assert!(model["clickEvents"].as_object().unwrap().is_empty());
    assert!(model["links"].as_object().unwrap().is_empty());
}

#[test]
fn gantt_click_rejects_missing_action() {
    let source = r#"gantt
dateFormat YYYY-MM-DD
section A
task: id1, 2013-01-01, 1d
click id1
"#;
    let err = block_on(Engine::new().parse_diagram(source, ParseOptions::default())).unwrap_err();

    assert!(
        err.to_string().contains("invalid click statement"),
        "unexpected error: {err}"
    );

    let Error::DiagramParse { diagnostic, .. } = err else {
        panic!("expected gantt parse error");
    };
    let click_start = source.find("click id1").unwrap();
    assert_eq!(
        diagnostic.span(),
        Some(SourceSpan::new(
            click_start,
            click_start + "click id1".len()
        ))
    );
    assert_eq!(diagnostic.span_kind(), ParseDiagnosticSpanKind::Exact);
}

#[test]
fn gantt_click_rejects_duplicate_href_or_call() {
    for source in [
        r#"
gantt
dateFormat YYYY-MM-DD
section A
task: id1, 2013-01-01, 1d
click id1 href "https://example.com/1" href "https://example.com/2"
"#,
        r#"
gantt
dateFormat YYYY-MM-DD
section A
task: id1, 2013-01-01, 1d
click id1 call first() call second()
"#,
    ] {
        let err =
            block_on(Engine::new().parse_diagram(source, ParseOptions::default())).unwrap_err();

        assert!(
            err.to_string().contains("invalid click statement"),
            "unexpected error: {err}"
        );
    }
}

#[test]
fn gantt_click_rejects_unrecognized_tail() {
    let err = block_on(Engine::new().parse_diagram(
        r#"
gantt
dateFormat YYYY-MM-DD
section A
task: id1, 2013-01-01, 1d
click id1 href "https://example.com/" garbage tail
"#,
        ParseOptions::default(),
    ))
    .unwrap_err();

    assert!(
        err.to_string().contains("invalid click statement"),
        "unexpected error: {err}"
    );
}

#[test]
fn gantt_common_db_sanitizes_title_and_accessibility_fields() {
    let model = parse(
        r#"
gantt
title <script>alert(1)</script><b>ok</b>
accTitle: <script>alert(1)</script><b>AT</b>
accDescr { <script>alert(1)</script>line1
    line2
}
"#,
    );
    assert_eq!(model["title"], json!("<b>ok</b>"));
    assert_eq!(model["accTitle"], json!("<b>AT</b>"));
    assert_eq!(model["accDescr"], json!("line1\nline2"));
}

#[test]
fn gantt_duration_minutes_and_seconds_match_upstream() {
    let model = parse(
        r#"
gantt
dateFormat YYYY-MM-DD
section testa1
test1: id1,2013-01-01,2m
test2: id2,2013-01-01,2s
"#,
    );
    let tasks = model["tasks"].as_array().unwrap();
    assert_eq!(
        tasks[0]["endTime"].as_i64().unwrap(),
        local_ms(2013, 0, 1, 0, 2, 0)
    );
    assert_eq!(
        tasks[1]["endTime"].as_i64().unwrap(),
        local_ms(2013, 0, 1, 0, 0, 2)
    );
}

#[test]
fn gantt_fixed_dates_without_id_match_upstream() {
    let model = parse(
        r#"
gantt
dateFormat YYYY-MM-DD
section testa1
test1: 2013-01-01,2013-01-12
"#,
    );
    let t0 = &model["tasks"][0];
    assert_eq!(t0["id"].as_str().unwrap(), "task1");
    assert_eq!(
        t0["startTime"].as_i64().unwrap(),
        local_ms(2013, 0, 1, 0, 0, 0)
    );
    assert_eq!(
        t0["endTime"].as_i64().unwrap(),
        local_ms(2013, 0, 12, 0, 0, 0)
    );
}

#[test]
fn gantt_relative_after_auto_task_ids_works_with_custom_date_format() {
    let model = parse(
        r#"
gantt
dateFormat DD.MM.YYYY
section Section
Task 1 :done, 07.01.2024, 08.02.2024
Task 2 :active, after task1, 20d
"#,
    );
    let tasks = model["tasks"].as_array().unwrap();
    assert_eq!(tasks[0]["id"].as_str().unwrap(), "task1");
    assert_eq!(tasks[1]["id"].as_str().unwrap(), "task2");
    assert_eq!(
        tasks[1]["startTime"].as_i64().unwrap(),
        local_ms(2024, 1, 8, 0, 0, 0)
    );
}

#[test]
fn gantt_relative_refs_work_across_sections_like_upstream() {
    let model = parse(
        r#"
gantt
dateFormat YYYY-MM-DD
section sec1
test1: id1,2013-01-01,2w
test2: id2,after id3,1d
section sec2
test3: id3,after id1,2d
"#,
    );
    let tasks = model["tasks"].as_array().unwrap();
    assert_eq!(
        tasks[1]["startTime"].as_i64().unwrap(),
        local_ms(2013, 0, 17, 0, 0, 0)
    );
    assert_eq!(
        tasks[1]["endTime"].as_i64().unwrap(),
        local_ms(2013, 0, 18, 0, 0, 0)
    );
}

#[test]
fn gantt_relative_after_multiple_ids_uses_latest_end_like_upstream() {
    let model = parse(
        r#"
gantt
dateFormat YYYY-MM-DD
section sec1
task1: id1,after id2 id3 id4,1d
task2: id2,2013-01-01,1d
task3: id3,2013-02-01,3d
task4: id4,2013-02-01,2d
"#,
    );
    let tasks = model["tasks"].as_array().unwrap();
    assert_eq!(
        tasks[0]["endTime"].as_i64().unwrap(),
        local_ms(2013, 1, 5, 0, 0, 0)
    );
}

#[test]
fn gantt_relative_until_multiple_ids_uses_earliest_start_like_upstream() {
    let model = parse(
        r#"
gantt
dateFormat YYYY-MM-DD
section sec1
task1: id1,2013-01-01,until id2 id3 id4
task2: id2,2013-01-11,1d
task3: id3,2013-02-10,1d
task4: id4,2013-02-12,1d
"#,
    );
    let tasks = model["tasks"].as_array().unwrap();
    assert_eq!(
        tasks[0]["endTime"].as_i64().unwrap(),
        local_ms(2013, 0, 11, 0, 0, 0)
    );
}

#[test]
fn gantt_relative_refs_accept_ascii_word_and_hyphen_ids_like_upstream() {
    let model = parse(
        r#"
gantt
dateFormat YYYY-MM-DD
section sec1
base: ref_1-2,2013-01-01,2d
next: id2,after ref_1-2,1d
limit: until_1-2,2013-02-01,1d
bounded: id3,2013-01-01,until until_1-2
"#,
    );
    let tasks = model["tasks"].as_array().unwrap();
    assert_eq!(
        tasks[1]["startTime"].as_i64().unwrap(),
        local_ms(2013, 0, 3, 0, 0, 0)
    );
    assert_eq!(
        tasks[3]["endTime"].as_i64().unwrap(),
        local_ms(2013, 1, 1, 0, 0, 0)
    );
}

#[test]
fn gantt_relative_ref_ids_preserve_source_regex_space_backtracking() {
    assert_eq!(relative_ref_ids("after id_1-2", "after"), Some("id_1-2"));
    assert_eq!(relative_ref_ids("after  #", "after"), Some(" "));
    assert_eq!(relative_ref_ids("after \t #", "after"), Some(" "));
    assert_eq!(relative_ref_ids("after #", "after"), None);
    assert_eq!(relative_ref_ids("After id1", "after"), None);
}

#[test]
fn gantt_relative_keywords_are_case_sensitive_like_upstream() {
    let engine = Engine::new();
    let err = block_on(engine.parse_diagram(
        r#"
gantt
dateFormat YYYY-MM-DD
section sec1
base: id1,2013-01-01,2d
next: id2,After id1,1d
"#,
        ParseOptions::default(),
    ))
    .unwrap_err();
    assert!(err.to_string().contains("Invalid date:After id1"));

    let model = parse(
        r#"
gantt
dateFormat YYYY-MM-DD
section sec1
limit: id2,2013-02-01,1d
bounded: id1,2013-01-01,Until id2
"#,
    );
    let tasks = model["tasks"].as_array().unwrap();
    assert_eq!(
        tasks[1]["endTime"].as_i64().unwrap(),
        local_ms(2013, 0, 1, 0, 0, 0)
    );
}

#[test]
fn gantt_timestamp_formats_x_and_x_support_signed_and_seconds() {
    let model = parse(
        r#"
gantt
dateFormat x
section T
t1: id1,-1,1ms
"#,
    );
    let t0 = &model["tasks"][0];
    assert_eq!(t0["startTime"].as_i64().unwrap(), -1);
    assert_eq!(t0["endTime"].as_i64().unwrap(), 0);

    let model = parse(
        r#"
gantt
dateFormat X
section T
t1: id1,20,1s
"#,
    );
    let t0 = &model["tasks"][0];
    assert_eq!(t0["startTime"].as_i64().unwrap(), 20);
    assert_eq!(t0["endTime"].as_i64().unwrap(), 1_020);
}

#[test]
fn dayjs_unix_seconds_floor_negative_milliseconds() {
    let utc = crate::time::LocalTimeZone::utc();
    with_test_local_time_zone(utc, || {
        for (milliseconds, expected) in [
            (-1_001, "-2"),
            (-1_000, "-1"),
            (-999, "-1"),
            (-1, "-1"),
            (0, "0"),
            (999, "0"),
            (1_000, "1"),
        ] {
            let datetime = OffsetDateTime::from_unix_millis(milliseconds, UtcOffset::UTC);
            assert_eq!(format_dayjs_like(datetime, "X"), expected);
        }
    });
}

#[test]
fn gantt_ignore_weekends_matches_upstream() {
    let model = parse(
        r#"
gantt
dateFormat YYYY-MM-DD
excludes weekends 2019-02-06,friday
section weekends skip test
test1: id1,2019-02-01,1d
test2: id2,after id1,2d
test3: id3,after id2,7d
test4: id4,2019-02-01,2019-02-20
test5: id5,after id4,1d
section full ending task on last day
test6: id6,2019-02-13,2d
test7: id7,after id6,1d
"#,
    );
    let tasks = model["tasks"].as_array().unwrap();
    assert_eq!(tasks.len(), 7);

    assert_eq!(
        tasks[0]["startTime"].as_i64().unwrap(),
        local_ms(2019, 1, 1, 0, 0, 0)
    );
    assert_eq!(
        tasks[0]["endTime"].as_i64().unwrap(),
        local_ms(2019, 1, 4, 0, 0, 0)
    );
    assert_eq!(
        tasks[0]["renderEndTime"].as_i64().unwrap(),
        local_ms(2019, 1, 2, 0, 0, 0)
    );
    assert_eq!(tasks[0]["id"].as_str().unwrap(), "id1");
    assert_eq!(tasks[0]["task"].as_str().unwrap(), "test1");

    assert_eq!(
        tasks[1]["startTime"].as_i64().unwrap(),
        local_ms(2019, 1, 4, 0, 0, 0)
    );
    assert_eq!(
        tasks[1]["endTime"].as_i64().unwrap(),
        local_ms(2019, 1, 7, 0, 0, 0)
    );
    assert_eq!(
        tasks[1]["renderEndTime"].as_i64().unwrap(),
        local_ms(2019, 1, 6, 0, 0, 0)
    );
    assert_eq!(tasks[1]["id"].as_str().unwrap(), "id2");
    assert_eq!(tasks[1]["task"].as_str().unwrap(), "test2");

    assert_eq!(
        tasks[2]["startTime"].as_i64().unwrap(),
        local_ms(2019, 1, 7, 0, 0, 0)
    );
    assert_eq!(
        tasks[2]["endTime"].as_i64().unwrap(),
        local_ms(2019, 1, 20, 0, 0, 0)
    );
    assert_eq!(
        tasks[2]["renderEndTime"].as_i64().unwrap(),
        local_ms(2019, 1, 20, 0, 0, 0)
    );
    assert_eq!(tasks[2]["id"].as_str().unwrap(), "id3");
    assert_eq!(tasks[2]["task"].as_str().unwrap(), "test3");

    assert_eq!(
        tasks[3]["startTime"].as_i64().unwrap(),
        local_ms(2019, 1, 1, 0, 0, 0)
    );
    assert_eq!(
        tasks[3]["endTime"].as_i64().unwrap(),
        local_ms(2019, 1, 20, 0, 0, 0)
    );
    assert!(tasks[3]["renderEndTime"].is_null());
    assert!(tasks[3]["manualEndTime"].as_bool().unwrap());
    assert_eq!(tasks[3]["id"].as_str().unwrap(), "id4");
    assert_eq!(tasks[3]["task"].as_str().unwrap(), "test4");

    assert_eq!(
        tasks[4]["startTime"].as_i64().unwrap(),
        local_ms(2019, 1, 20, 0, 0, 0)
    );
    assert_eq!(
        tasks[4]["endTime"].as_i64().unwrap(),
        local_ms(2019, 1, 21, 0, 0, 0)
    );
    assert_eq!(
        tasks[4]["renderEndTime"].as_i64().unwrap(),
        local_ms(2019, 1, 21, 0, 0, 0)
    );
    assert_eq!(tasks[4]["id"].as_str().unwrap(), "id5");
    assert_eq!(tasks[4]["task"].as_str().unwrap(), "test5");

    assert_eq!(
        tasks[5]["startTime"].as_i64().unwrap(),
        local_ms(2019, 1, 13, 0, 0, 0)
    );
    assert_eq!(
        tasks[5]["endTime"].as_i64().unwrap(),
        local_ms(2019, 1, 18, 0, 0, 0)
    );
    assert_eq!(
        tasks[5]["renderEndTime"].as_i64().unwrap(),
        local_ms(2019, 1, 15, 0, 0, 0)
    );
    assert_eq!(tasks[5]["id"].as_str().unwrap(), "id6");
    assert_eq!(tasks[5]["task"].as_str().unwrap(), "test6");

    assert_eq!(
        tasks[6]["startTime"].as_i64().unwrap(),
        local_ms(2019, 1, 18, 0, 0, 0)
    );
    assert_eq!(
        tasks[6]["endTime"].as_i64().unwrap(),
        local_ms(2019, 1, 19, 0, 0, 0)
    );
    assert_eq!(tasks[6]["id"].as_str().unwrap(), "id7");
    assert_eq!(tasks[6]["task"].as_str().unwrap(), "test7");
}

#[test]
fn gantt_maintains_task_creation_order_matches_upstream_sample() {
    let model = parse(
        r#"
gantt
accTitle: Project Execution
dateFormat YYYY-MM-DD
section section A section
Completed task: done,    des1, 2014-01-06,2014-01-08
Active task: active,  des2, 2014-01-09, 3d
Future task: des3, after des2, 5d
Future task2: des4, after des3, 5d

section section Critical tasks
Completed task in the critical line: crit, done, 2014-01-06,24h
Implement parser and jison: crit, done, after des1, 2d
Create tests for parser: crit, active, 3d
Future task in critical line: crit, 5d
Create tests for renderer: 2d
Add to mermaid: 1d

section section Documentation
Describe gantt syntax: active, a1, after des1, 3d
Add gantt diagram to demo page: after a1  , 20h
Add another diagram to demo page: doc1, after a1  , 48h

section section Last section
Describe gantt syntax: after doc1, 3d
Add gantt diagram to demo page: 20h
Add another diagram to demo page: 48h
"#,
    );

    let tasks = model["tasks"].as_array().unwrap();
    assert_eq!(tasks.len(), 16);

    for (i, t) in tasks.iter().enumerate() {
        assert_eq!(t["order"].as_i64().unwrap(), i as i64);
    }

    assert_eq!(tasks[0]["id"].as_str().unwrap(), "des1");
    assert_eq!(tasks[0]["task"].as_str().unwrap(), "Completed task");
    assert_eq!(
        tasks[0]["startTime"].as_i64().unwrap(),
        local_ms(2014, 0, 6, 0, 0, 0)
    );
    assert_eq!(
        tasks[0]["endTime"].as_i64().unwrap(),
        local_ms(2014, 0, 8, 0, 0, 0)
    );

    assert_eq!(tasks[1]["id"].as_str().unwrap(), "des2");
    assert_eq!(tasks[1]["task"].as_str().unwrap(), "Active task");
    assert_eq!(
        tasks[1]["startTime"].as_i64().unwrap(),
        local_ms(2014, 0, 9, 0, 0, 0)
    );
    assert_eq!(
        tasks[1]["endTime"].as_i64().unwrap(),
        local_ms(2014, 0, 12, 0, 0, 0)
    );

    assert_eq!(
        tasks[11]["task"].as_str().unwrap(),
        "Add gantt diagram to demo page"
    );
    assert_eq!(
        tasks[11]["startTime"].as_i64().unwrap(),
        local_ms(2014, 0, 11, 0, 0, 0)
    );
    assert_eq!(
        tasks[11]["endTime"].as_i64().unwrap(),
        local_ms(2014, 0, 11, 20, 0, 0)
    );

    assert_eq!(
        tasks[14]["task"].as_str().unwrap(),
        "Add gantt diagram to demo page"
    );
    assert_eq!(
        tasks[14]["startTime"].as_i64().unwrap(),
        local_ms(2014, 0, 16, 0, 0, 0)
    );
    assert_eq!(
        tasks[14]["endTime"].as_i64().unwrap(),
        local_ms(2014, 0, 16, 20, 0, 0)
    );

    assert_eq!(
        tasks[15]["task"].as_str().unwrap(),
        "Add another diagram to demo page"
    );
    assert_eq!(
        tasks[15]["startTime"].as_i64().unwrap(),
        local_ms(2014, 0, 16, 20, 0, 0)
    );
    assert_eq!(
        tasks[15]["endTime"].as_i64().unwrap(),
        local_ms(2014, 0, 18, 20, 0, 0)
    );
}

#[test]
fn gantt_end_date_on_31st_matches_upstream() {
    let model = parse(
        r#"
gantt
dateFormat YYYY-MM-DD
section Task endTime is on the 31st day of the month
test1: id1,2019-09-30,11d
test2: id2,after id1,20d
"#,
    );
    let tasks = model["tasks"].as_array().unwrap();
    assert_eq!(tasks.len(), 2);

    assert_eq!(
        tasks[0]["startTime"].as_i64().unwrap(),
        local_ms(2019, 8, 30, 0, 0, 0)
    );
    assert_eq!(
        tasks[0]["endTime"].as_i64().unwrap(),
        local_ms(2019, 9, 11, 0, 0, 0)
    );
    assert_eq!(tasks[0]["id"].as_str().unwrap(), "id1");
    assert_eq!(tasks[0]["task"].as_str().unwrap(), "test1");

    assert_eq!(
        tasks[1]["startTime"].as_i64().unwrap(),
        local_ms(2019, 9, 11, 0, 0, 0)
    );
    assert_eq!(
        tasks[1]["endTime"].as_i64().unwrap(),
        local_ms(2019, 9, 31, 0, 0, 0)
    );
    assert!(tasks[1]["renderEndTime"].is_null());
    assert_eq!(tasks[1]["id"].as_str().unwrap(), "id2");
    assert_eq!(tasks[1]["task"].as_str().unwrap(), "test2");
}

#[test]
fn gantt_today_marker_is_stored() {
    let model = parse(
        r#"
gantt
todayMarker off
"#,
    );
    assert_eq!(model["todayMarker"].as_str().unwrap(), "off");

    let model = parse(
        r#"
gantt
todayMarker stoke:stroke-width:5px,stroke:#00f,opacity:0.5
"#,
    );
    assert_eq!(
        model["todayMarker"].as_str().unwrap(),
        "stoke:stroke-width:5px,stroke:#00f,opacity:0.5"
    );
}

#[test]
fn gantt_section_allows_hash_character() {
    let model = parse(
        r#"
gantt
section A #1
test: id1,2013-01-01,1d
"#,
    );
    assert_eq!(model["sections"][0].as_str().unwrap(), "A #1");
}

#[test]
fn gantt_weekday_rejects_unknown_values() {
    let engine = Engine::new();
    let text = "gantt\nweekday foo\n";
    let err = block_on(engine.parse_diagram(text, ParseOptions::default())).unwrap_err();
    assert!(err.to_string().contains("invalid weekday"));

    let Error::DiagramParse { diagnostic, .. } = err else {
        panic!("expected gantt parse error");
    };
    let token_start = text.find("foo").unwrap();
    assert_eq!(
        diagnostic.span(),
        Some(SourceSpan::new(token_start, token_start + "foo".len()))
    );
    assert_eq!(diagnostic.span_kind(), ParseDiagnosticSpanKind::Exact);
}

#[test]
fn gantt_weekend_rejects_unknown_values() {
    let engine = Engine::new();
    let err = block_on(engine.parse_diagram(
        r#"
gantt
weekend monday
"#,
        ParseOptions::default(),
    ))
    .unwrap_err();
    assert!(err.to_string().contains("invalid weekend"));
}

#[test]
fn gantt_after_missing_id_defaults_to_today_midnight_like_upstream() {
    let model = parse(
        r#"
gantt
dateFormat YYYY-MM-DD
section testa1
test1: id1,2013-01-01,2w
test2: id2,after missing,1d
"#,
    );
    let tasks = model["tasks"].as_array().unwrap();
    let expected = today_midnight_local()
        .expect("current date is supported by the active timezone")
        .timestamp_millis();
    assert_eq!(tasks[1]["startTime"].as_i64().unwrap(), expected);
}

#[test]
fn gantt_unknown_reference_warnings_preserve_fallback_and_resolved_dates() {
    let engine = Engine::new()
        .with_fixed_today(Some(CivilDate::new(2026, 2, 15).unwrap()))
        .try_with_fixed_local_offset_minutes(0)
        .unwrap();
    let parsed = engine
        .parse_diagram_sync(
            concat!(
                "gantt\ndateFormat YYYY-MM-DD\n",
                "Base: base,2013-01-01,2d\n",
                "Missing after: missing_after,after missing,1d\n",
                "Missing until: missing_until,2013-01-01,until missing\n",
                "Mixed after: mixed_after,after missing  base,1d\n",
                "Mixed until: mixed_until,2012-12-01,until missing  base\n",
                "M1: milestone, 2023-01-01, m1\n",
            ),
            ParseOptions::strict(),
        )
        .unwrap()
        .unwrap();
    let tasks = parsed.model["tasks"].as_array().unwrap();
    assert_eq!(tasks[1]["startTime"], json!(1_771_113_600_000_i64));
    assert_eq!(tasks[2]["endTime"], json!(1_771_113_600_000_i64));
    assert_eq!(tasks[3]["startTime"], tasks[0]["endTime"]);
    assert_eq!(tasks[4]["endTime"], tasks[0]["startTime"]);
    assert_eq!(tasks[5]["startTime"], tasks[5]["endTime"]);
}
