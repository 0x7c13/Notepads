#![cfg(all(feature = "diagram-class", feature = "diagram-sequence"))]

use merman_core::{Engine, ParseOptions, RenderSemanticModel};
use std::fmt::Write as _;
use std::fs::{self, File};
use std::io::{self, Write as _};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

const CASE_ENV: &str = "MERMAN_CORE_GRAMMAR_CARRIER_CASE";
const SIZE_ENV: &str = "MERMAN_CORE_GRAMMAR_CARRIER_SIZE";
const WORKER_STACK_BYTES: usize = 2 * 1024 * 1024;
const CHILD_TIMEOUT: Duration = Duration::from_secs(60);

fn source(case: &str, size: usize) -> String {
    match case {
        "class-wide" => {
            let mut source = String::from("classDiagram\n");
            for index in 0..size {
                writeln!(source, "class C{index}").unwrap();
            }
            source
        }
        "sequence-wide" => format!("sequenceDiagram\n{}", "A->>B: message\n".repeat(size)),
        "class-deep" | "class-missing-outer-brace" => {
            let mut source = String::from("classDiagram\n");
            for index in 0..size {
                writeln!(source, "namespace N{index} {{").unwrap();
            }
            source.push_str("class Leaf\n");
            source.push_str(&"}\n".repeat(size - usize::from(case == "class-missing-outer-brace")));
            source
        }
        "sequence-deep" | "sequence-missing-outer-end" => {
            let mut source = String::from("sequenceDiagram\n");
            for index in 0..size {
                writeln!(source, "loop L{index}").unwrap();
            }
            source.push_str("A->>B: message\n");
            source.push_str(
                &"end\n".repeat(size - usize::from(case == "sequence-missing-outer-end")),
            );
            source
        }
        _ => panic!("unknown grammar carrier case {case}"),
    }
}

fn marker(phase: &str, status: &str) {
    println!("phase={phase} status={status}");
    io::stdout()
        .flush()
        .expect("flush carrier lifecycle marker");
}

fn run_worker(case: &str, size: usize) {
    let started = Instant::now();
    let source = source(case, size);
    println!(
        "case={case} size={size} source_bytes={} worker_stack_bytes={WORKER_STACK_BYTES} \
         path=Engine.parse_diagram_for_render_model_sync profile=direct-engine-strict",
        source.len()
    );
    let engine = Engine::new();
    marker("parse", "begin");
    let parsed = engine.parse_diagram_for_render_model_sync(&source, ParseOptions::strict());
    if case.contains("missing-outer") {
        let error = parsed.expect_err("missing outer closer reports the established grammar error");
        marker("parse", "reported-error");
        println!("reported_error={error}");
        drop(error);
    } else {
        let parsed = parsed
            .expect("valid carrier workload parses")
            .expect("carrier diagram is detected");
        marker("parse", "done");
        match parsed.model() {
            RenderSemanticModel::Class(model) => {
                let expected_classes = if case == "class-wide" { size } else { 1 };
                assert_eq!(model.classes.len(), expected_classes);
                if case == "class-deep" {
                    assert_eq!(model.namespaces.len(), size);
                }
                let namespace_id_bytes: usize = model.namespaces.keys().map(String::len).sum();
                println!(
                    "class_records={} namespace_records={} namespace_id_bytes={namespace_id_bytes}",
                    model.classes.len(),
                    model.namespaces.len(),
                );
            }
            RenderSemanticModel::Sequence(model) => {
                let expected_messages = if case == "sequence-wide" {
                    size
                } else {
                    size * 2 + 1
                };
                assert_eq!(model.messages.len(), expected_messages);
                assert_eq!(model.actors.len(), 2);
                println!("message_records={} actor_records=2", model.messages.len());
            }
            _ => panic!("carrier workload returned the wrong family"),
        }
        marker("clone", "begin");
        let cloned = parsed.clone();
        marker("clone", "done");
        drop(cloned);
        marker("clone-drop", "done");
        drop(parsed);
    }
    marker("drop", "done");
    let small = engine
        .parse_diagram_for_render_model_sync(
            "sequenceDiagram\nA->>B: after\n",
            ParseOptions::strict(),
        )
        .expect("host accepts a subsequent small diagram")
        .expect("small diagram is detected");
    drop(small);
    marker("after-small-operation", "done");
    println!("worker_elapsed_ms={}", started.elapsed().as_millis());
}

fn run_child(case: &str, size: usize) {
    let started = Instant::now();
    let timestamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("diagnostic timestamp follows the Unix epoch")
        .as_nanos();
    let directory = std::env::temp_dir().join(format!(
        "merman-core-grammar-carrier-{}-{case}-{size}-{timestamp}",
        std::process::id()
    ));
    fs::create_dir(&directory).unwrap_or_else(|error| {
        panic!(
            "create carrier child diagnostic directory {}: {error}",
            directory.display()
        )
    });
    let stdout_path = directory.join("stdout.txt");
    let stderr_path = directory.join("stderr.txt");
    let mut child = Command::new(std::env::current_exe().expect("current test executable"))
        .args(["--exact", "grammar_carrier_child", "--nocapture"])
        .env(CASE_ENV, case)
        .env(SIZE_ENV, size.to_string())
        .stdout(Stdio::from(
            File::create(&stdout_path).expect("child stdout"),
        ))
        .stderr(Stdio::from(
            File::create(&stderr_path).expect("child stderr"),
        ))
        .spawn()
        .expect("spawn carrier lifecycle child");
    let (status, timed_out, process_error) = loop {
        match child.try_wait() {
            Ok(Some(status)) => break (status, false, None),
            Ok(None) => {}
            Err(error) => {
                let _ = child.kill();
                break (
                    child.wait().expect("reap child after wait failure"),
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
    let stdout = fs::read_to_string(stdout_path).expect("read carrier child stdout");
    let stderr = fs::read_to_string(stderr_path).expect("read carrier child stderr");
    let summary = format!(
        "case={case} size={size} status={status} timed_out={timed_out} \
         elapsed_ms={} process_error={process_error:?}",
        started.elapsed().as_millis()
    );
    fs::write(directory.join("status.txt"), &summary).expect("preserve carrier child status");
    println!(
        "{summary}\ndiagnostics={}\nstdout:\n{stdout}\nstderr:\n{stderr}",
        directory.display()
    );
    assert!(!timed_out, "carrier child timed out: {summary}");
    assert!(
        process_error.is_none(),
        "carrier child observation failed: {summary}"
    );
    assert!(status.success(), "carrier child failed: {summary}");
    assert!(stdout.contains("phase=drop status=done"));
    assert!(stdout.contains("phase=after-small-operation status=done"));
}

#[test]
fn grammar_carrier_child() {
    let Ok(case) = std::env::var(CASE_ENV) else {
        return;
    };
    let size = std::env::var(SIZE_ENV)
        .expect("carrier child size")
        .parse::<usize>()
        .expect("numeric carrier child size");
    assert!(size > 0);
    std::thread::Builder::new()
        .name(format!("grammar-carrier-{case}"))
        .stack_size(WORKER_STACK_BYTES)
        .spawn(move || run_worker(&case, size))
        .expect("spawn 2 MiB carrier worker")
        .join()
        .expect("carrier worker completes");
}

#[test]
fn class_and_sequence_carrier_lifecycles_complete_on_host_stack() {
    // Qualified namespace strings remain a separate semantic/output cost.
    run_child("class-deep", 512);
    run_child("class-missing-outer-brace", 10_000);
    run_child("class-wide", 10_000);
    run_child("sequence-deep", 3_000);
    run_child("sequence-missing-outer-end", 10_000);
    run_child("sequence-wide", 10_000);
}
