#![cfg(feature = "all-diagrams")]

use merman_core::{EditorSemanticCompleteness, Engine, ParseOptions};
use serde_json::Value;
use std::fmt::Write as _;
use std::fs::{self, File};
use std::io::{self, Write as _};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

const CASE_ENV: &str = "MERMAN_CORE_BOUNDED_LIFECYCLE_CASE";
const WORKER_STACK_BYTES: usize = 2 * 1024 * 1024;
const CHILD_TIMEOUT: Duration = Duration::from_secs(60);
const LOCAL_DEPTH: usize = 256;

fn marker(phase: &str, status: &str) {
    println!("phase={phase} status={status}");
    io::stdout()
        .flush()
        .expect("flush bounded lifecycle marker");
}

fn json_container_depth(value: &Value) -> usize {
    let mut maximum = 0;
    let mut pending = vec![(value, 0usize)];
    while let Some((value, parent_depth)) = pending.pop() {
        match value {
            Value::Array(values) => {
                let depth = parent_depth + 1;
                maximum = maximum.max(depth);
                pending.extend(values.iter().map(|value| (value, depth)));
            }
            Value::Object(values) => {
                let depth = parent_depth + 1;
                maximum = maximum.max(depth);
                pending.extend(values.values().map(|value| (value, depth)));
            }
            _ => {}
        }
    }
    maximum
}

fn accepted(engine: &Engine, source: &str, diagram_type: &str) {
    println!("diagram_type={diagram_type} source_bytes={}", source.len());
    marker("parse", "begin");
    let parsed = engine
        .parse_diagram_for_render_model_sync(source, ParseOptions::strict())
        .expect("accepted local boundary parses")
        .expect("bounded diagram is detected");
    assert_eq!(parsed.metadata().diagram_type, diagram_type);
    marker("parse", "done");
    marker("clone", "begin");
    let cloned = parsed.clone();
    marker("clone", "done");
    drop(cloned);
    marker("clone-drop", "done");
    marker("export", "begin");
    let json = parsed
        .model()
        .compatibility_json(parsed.metadata())
        .expect("accepted local boundary projects compatibility JSON");
    let mut encoded = Vec::new();
    json.write_json(&mut encoded)
        .expect("maintained iterative JSON writer exports the boundary");
    assert!(encoded.starts_with(b"{") && encoded.ends_with(b"}"));
    let depth = json_container_depth(json.as_value());
    println!(
        "json_container_depth={depth} output_bytes={}",
        encoded.len()
    );
    assert_eq!(serde_json::to_vec(&json).is_ok(), depth <= 128);
    {
        struct CancelAfterOutput<'a> {
            output: &'a mut Vec<u8>,
            control: &'a merman_core::OperationControl,
        }
        impl std::io::Write for CancelAfterOutput<'_> {
            fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
                self.output.extend_from_slice(bytes);
                if self.output.len() >= 32 {
                    self.control.cancel();
                }
                Ok(bytes.len())
            }
            fn flush(&mut self) -> std::io::Result<()> {
                Ok(())
            }
        }
        let control = merman_core::OperationControl::new();
        let mut partial_output = Vec::new();
        let cancelled = json
            .write_json_controlled(
                CancelAfterOutput {
                    output: &mut partial_output,
                    control: &control,
                },
                &control,
            )
            .expect_err("writer reports cancellation after useful output");
        assert!(
            !partial_output.is_empty(),
            "cancellation follows useful export work"
        );
        assert_eq!(control.checkpoint().unwrap_err(), cancelled);
        marker("export", "cancelled-after-output");
    }
    let cloned = json.clone();
    assert_eq!(cloned, json);
    drop(cloned);
    drop(json);
    marker("export", "done");
    drop(parsed);
    marker("drop", "done");

    let json_parsed = engine
        .parse_diagram_sync(source, ParseOptions::strict())
        .expect("direct compatibility parse accepts the local boundary")
        .expect("bounded compatibility diagram is detected");
    let mut encoded = Vec::new();
    json_parsed.model.write_json(&mut encoded).unwrap();
    drop(json_parsed);
    marker("compatibility-parse-drop", "done");
}

fn rejected(engine: &Engine, source: &str, message: &str) {
    marker("parse", "begin");
    let error = engine
        .parse_diagram_for_render_model_sync(source, ParseOptions::strict())
        .expect_err("first excess or malformed fragment reports an error");
    assert!(error.to_string().contains(message), "{error}");
    println!("reported_error={error}");
    drop(error);
    marker("parse", "reported-error");
    marker("drop", "done");
}

fn railroad_source(dialect: &str, depth: usize, empty: bool, malformed: bool) -> String {
    let (header, assignment, opener, closer, leaf) = match dialect {
        "railroad" => ("railroad-beta", "=", "optional(", ")", "terminal(\"a\")"),
        "railroadEbnf" => ("railroad-ebnf-beta", "=", "[", "]", "\"a\""),
        "railroadAbnf" => ("railroad-abnf-beta", "=", "[", "]", "\"a\""),
        "railroadPeg" => ("railroad-peg-beta", "<-", "(", ")", "\"a\""),
        _ => unreachable!("known Railroad dialect"),
    };
    format!(
        "{header}\nentry {assignment} {}{}{} ;\nafter {assignment} {leaf} ;\n",
        opener.repeat(depth),
        if empty { "" } else { leaf },
        closer.repeat(depth - usize::from(malformed)),
    )
}

fn railroad_groups(engine: &Engine) {
    for dialect in ["railroad", "railroadEbnf", "railroadAbnf", "railroadPeg"] {
        println!("scenario=group-boundary dialect={dialect}");
        accepted(
            engine,
            &railroad_source(dialect, LOCAL_DEPTH, false, false),
            dialect,
        );
        let excessive = railroad_source(dialect, LOCAL_DEPTH + 1, false, false);
        rejected(engine, &excessive, "railroad nesting depth exceeds 256");
        let facts = engine
            .parse_editor_semantic_facts_with_type_sync(dialect, &excessive)
            .unwrap()
            .unwrap();
        assert_eq!(facts.completeness, EditorSemanticCompleteness::Recovered);
        assert!(facts.symbols.iter().any(|symbol| symbol.name == "after"));
        rejected(
            engine,
            &railroad_source(dialect, LOCAL_DEPTH, true, false),
            "expected",
        );
        rejected(
            engine,
            &railroad_source(dialect, LOCAL_DEPTH, false, true),
            "expected",
        );
    }
}

fn railroad_postfix(engine: &Engine) {
    for operator in ['?', '*', '+'] {
        for count in [LOCAL_DEPTH, LOCAL_DEPTH + 1, 5_000] {
            println!("scenario=postfix operator={operator} count={count}");
            let source = format!(
                "railroad-ebnf-beta\nentry ::= \"a\"{} ;\nafter ::= \"b\" ;\n",
                operator.to_string().repeat(count),
            );
            if count == LOCAL_DEPTH {
                accepted(engine, &source, "railroadEbnf");
                rejected(engine, &source.replacen(" ;", " ) ;", 1), "expected");
            } else {
                rejected(engine, &source, "railroad nesting depth exceeds 256");
                rejected(
                    engine,
                    &source.replacen(" ;", " ) ;", 1),
                    "railroad nesting depth exceeds 256",
                );
                let facts = engine
                    .parse_editor_semantic_facts_with_type_sync("railroadEbnf", &source)
                    .unwrap()
                    .unwrap();
                assert_eq!(facts.completeness, EditorSemanticCompleteness::Recovered);
                assert!(facts.symbols.iter().any(|symbol| symbol.name == "after"));
            }
        }
    }
}

fn zenuml_blocks(engine: &Engine) {
    let source = format!(
        "zenuml\n{}A.call()\n{}",
        "opt {\n".repeat(LOCAL_DEPTH),
        "}\n".repeat(LOCAL_DEPTH)
    );
    accepted(engine, &source, "zenuml");
    let excessive = format!(
        "zenuml\n{}A.call()\n{}After.call()\n",
        "opt {\n".repeat(LOCAL_DEPTH + 1),
        "}\n".repeat(LOCAL_DEPTH + 1)
    );
    rejected(engine, &excessive, "ZenUML nesting depth exceeds 256");
    rejected(
        engine,
        &source.replacen("}\n", "", 1),
        "unterminated ZenUML block",
    );
}

fn zenuml_expressions(engine: &Engine) {
    for depth in [LOCAL_DEPTH, LOCAL_DEPTH + 1] {
        for expression in [
            format!("{}x", "!".repeat(depth)),
            format!("{}x{}", "(".repeat(depth), ")".repeat(depth)),
            format!("{}x", "value=".repeat(depth)),
            format!("{}x{}", "F(".repeat(depth), ")".repeat(depth)),
        ] {
            let source = format!("zenuml\nreturn {expression}\nAfter.call()\n");
            if depth == LOCAL_DEPTH {
                accepted(engine, &source, "zenuml");
            } else {
                rejected(
                    engine,
                    &source,
                    "ZenUML expression nesting depth exceeds 256",
                );
            }
        }
    }
}

#[cfg(feature = "test-support")]
fn zenuml_projection_cancellation(engine: &Engine) {
    use merman_core::diagrams::zenuml::ZenumlStatementKind;

    let source = format!(
        "zenuml\n{}A.call()\n{}After.call()\n",
        "opt {\n".repeat(LOCAL_DEPTH),
        "}\n".repeat(LOCAL_DEPTH)
    );
    println!(
        "projection_depth={LOCAL_DEPTH} source_bytes={}",
        source.len()
    );
    let parsed = engine
        .parse_diagram_for_render_model_sync(&source, ParseOptions::strict())
        .unwrap()
        .unwrap();
    let merman_core::RenderSemanticModel::Zenuml(model) = parsed.model() else {
        panic!("expected the maximum accepted ZenUML model");
    };
    assert_eq!(model.statements.len(), 2);
    assert!(model.groups.is_empty());
    let mut statement = &model.statements[0];
    let mut fragment_depth = 0;
    while let ZenumlStatementKind::Fragment { sections, .. } = &statement.kind {
        assert_eq!(sections.len(), 1);
        assert_eq!(sections[0].statements.len(), 1);
        statement = &sections[0].statements[0];
        fragment_depth += 1;
    }
    assert_eq!(fragment_depth, LOCAL_DEPTH);
    let ZenumlStatementKind::Message { body, .. } = &statement.kind else {
        panic!("the deepest accepted fragment contains a message");
    };
    assert!(body.is_empty());

    // The public entry and participant/group batches precede the root statement-list task.
    // Each one-section fragment uses ten checkpoints: four descent tasks, two array
    // completions with one child-transfer checkpoint each, and two object completions.
    // The empty-body message uses four tasks. Cancel at the next statement task, after
    // the complete 256-level first branch is owned by the managed completed collection.
    let successful_checkpoints = 1
        + model.participants.len().div_ceil(128)
        + model.groups.len().div_ceil(128)
        + 1
        + 10 * fragment_depth
        + 4;
    let control = merman_core::OperationControl::new();
    control.cancel_after_checkpoints(successful_checkpoints);
    marker("projection", "begin");
    let cancelled = parsed
        .model()
        .compatibility_json_controlled(parsed.metadata(), &control)
        .expect_err("projection cancels after the maximum accepted deep branch completes");
    assert_eq!(cancelled.reason, merman_core::CancelReason::Requested);
    assert_eq!(cancelled.phase, merman_core::OperationPhase::Semantic);
    assert_eq!(control.checkpoint().unwrap_err(), cancelled);
    println!("fragment_depth={fragment_depth} scheduled_checkpoints={successful_checkpoints}");
    marker("projection", "cancelled-after-completed-deep-branch");
    drop(parsed);
    marker("drop", "done");
}

fn tree_view(engine: &Engine) {
    let mut source = String::from("treeView-beta\n");
    for depth in 0..LOCAL_DEPTH {
        writeln!(source, "{}\"n{depth}\"", " ".repeat(depth)).unwrap();
    }
    accepted(engine, &source, "treeView");
    writeln!(source, "{}\"excess\"", " ".repeat(LOCAL_DEPTH)).unwrap();
    rejected(engine, &source, "treeView nesting depth exceeds 256");
}

fn usecase_json(engine: &Engine) {
    for depth in [126, 127, 128, 300] {
        for scalar in ["1", "1e309"] {
            println!("scenario=usecase-json depth={depth} scalar={scalar}");
            let source = format!(
                "usecase-beta\njson Data@{{\"value\":{}{scalar}{}}}\n",
                "[".repeat(depth),
                "]".repeat(depth)
            );
            if depth == 126 {
                accepted(engine, &source, "usecase");
            } else {
                rejected(engine, &source, "JSON");
            }
        }
    }
}

fn run_worker(case: &str) {
    println!(
        "case={case} worker_stack_bytes={WORKER_STACK_BYTES} profile=direct-engine-strict build_mode={}",
        if cfg!(debug_assertions) {
            "debug"
        } else {
            "release"
        }
    );
    let engine = Engine::new();
    match case {
        "railroad-groups" => railroad_groups(&engine),
        "railroad-postfix" => railroad_postfix(&engine),
        "zenuml-blocks" => zenuml_blocks(&engine),
        "zenuml-expressions" => zenuml_expressions(&engine),
        #[cfg(feature = "test-support")]
        "zenuml-projection-cancel" => zenuml_projection_cancellation(&engine),
        "tree-view" => tree_view(&engine),
        "usecase-json" => usecase_json(&engine),
        _ => panic!("unknown bounded lifecycle case {case}"),
    }
    drop(
        engine
            .parse_diagram_for_render_model_sync(
                "railroad-beta\nafter = terminal(\"ok\") ;\n",
                ParseOptions::strict(),
            )
            .unwrap()
            .unwrap(),
    );
    marker("after-small-operation", "done");
}

fn run_child(case: &str) {
    let started = Instant::now();
    let timestamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("diagnostic timestamp follows the Unix epoch")
        .as_nanos();
    let directory = std::env::temp_dir().join(format!(
        "merman-core-bounded-{}-{case}-{timestamp}",
        std::process::id()
    ));
    fs::create_dir(&directory).unwrap_or_else(|error| {
        panic!(
            "create bounded child diagnostic directory {}: {error}",
            directory.display()
        )
    });
    let stdout_path = directory.join("stdout.txt");
    let stderr_path = directory.join("stderr.txt");
    let mut child = Command::new(std::env::current_exe().unwrap())
        .args(["--exact", "bounded_lifecycle_child", "--nocapture"])
        .env(CASE_ENV, case)
        .stdout(Stdio::from(File::create(&stdout_path).unwrap()))
        .stderr(Stdio::from(File::create(&stderr_path).unwrap()))
        .spawn()
        .expect("spawn bounded lifecycle child");
    let (status, timed_out, process_error) = loop {
        match child.try_wait() {
            Ok(Some(status)) => break (status, false, None),
            Ok(None) => {}
            Err(error) => {
                let _ = child.kill();
                break (
                    child.wait().expect("reap failed child"),
                    false,
                    Some(error.to_string()),
                );
            }
        }
        if started.elapsed() >= CHILD_TIMEOUT {
            let error = child.kill().err().map(|error| error.to_string());
            break (child.wait().expect("reap timed-out child"), true, error);
        }
        std::thread::sleep(Duration::from_millis(10));
    };
    let stdout = fs::read_to_string(&stdout_path).unwrap();
    let stderr = fs::read_to_string(&stderr_path).unwrap();
    let summary = format!(
        "case={case} status={status} timed_out={timed_out} elapsed_ms={} process_error={process_error:?}",
        started.elapsed().as_millis()
    );
    fs::write(directory.join("status.txt"), &summary).unwrap();
    println!(
        "{summary}\ndiagnostics={}\nstdout:\n{stdout}\nstderr:\n{stderr}",
        directory.display()
    );
    assert!(!timed_out, "bounded child timed out: {summary}");
    assert!(
        process_error.is_none(),
        "bounded child observation failed: {summary}"
    );
    assert!(status.success(), "bounded child failed: {summary}");
    assert!(stdout.contains("phase=drop status=done"));
    assert!(stdout.contains("phase=after-small-operation status=done"));
    if case == "zenuml-projection-cancel" {
        assert!(stdout.contains("phase=projection status=cancelled-after-completed-deep-branch"));
    }
}

#[test]
fn bounded_lifecycle_child() {
    let Ok(case) = std::env::var(CASE_ENV) else {
        return;
    };
    std::thread::Builder::new()
        .name(format!("bounded-{case}"))
        .stack_size(WORKER_STACK_BYTES)
        .spawn(move || run_worker(&case))
        .expect("spawn 2 MiB bounded worker")
        .join()
        .expect("bounded worker completes");
}

#[test]
fn railroad_constructed_depth_lifecycle_on_host_stack() {
    run_child("railroad-postfix");
    run_child("railroad-groups");
}

#[test]
fn zenuml_bounded_lifecycle_on_host_stack() {
    run_child("zenuml-blocks");
    run_child("zenuml-expressions");
}

#[cfg(feature = "test-support")]
#[test]
fn zenuml_projection_cancellation_after_maximum_depth_branch_on_host_stack() {
    run_child("zenuml-projection-cancel");
}

#[test]
fn tree_view_bounded_lifecycle_on_host_stack() {
    run_child("tree-view");
}

#[test]
fn usecase_json_bounded_lifecycle_on_host_stack() {
    run_child("usecase-json");
}
