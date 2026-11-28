#![cfg(feature = "diagram-block")]

use merman_core::{Engine, ParseOptions, RenderSemanticModel};
use std::fmt::Write as _;
use std::fs::{self, File};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

const CASE_ENV: &str = "MERMAN_BLOCK_LIFECYCLE_CASE";
const STACK_BYTES: usize = 2 * 1024 * 1024;

fn source(depth: usize, closers: usize) -> String {
    let mut source = String::from("block\n");
    for index in 0..depth {
        writeln!(source, "block:b{index}").unwrap();
    }
    source.push_str("leaf\n");
    for _ in 0..closers {
        source.push_str("end\n");
    }
    source
}

fn run_worker(case: &str) {
    let engine = Engine::new();
    let (depth, closers) = match case {
        "valid" => (3_000, 3_000),
        "missing-outer" => (10_000, 9_999),
        "missing-all" => (10_000, 0),
        "partial" => (10_000, 5_000),
        _ => panic!("unknown Block lifecycle selector {case}"),
    };
    let source = source(depth, closers);
    println!(
        "case={case} source_bytes={} phase=parse status=begin",
        source.len()
    );
    let result = engine.parse_diagram_for_render_model_sync(&source, ParseOptions::strict());
    if case == "valid" {
        let parsed = result.unwrap().unwrap();
        let RenderSemanticModel::Block(model) = parsed.model() else {
            panic!("expected canonical Block model");
        };
        let records = model.blocks_flat.len();
        let child_ids = model
            .blocks_flat
            .iter()
            .map(|block| block.children.len())
            .sum::<usize>();
        println!("records={records} child_ids={child_ids} phase=parse status=done");
        assert_eq!(records, depth + 2);
        assert_eq!(child_ids, depth + 1);
        assert_eq!(model.root().unwrap().children, vec![1]);
        println!("phase=clone status=begin");
        let cloned = parsed.clone();
        println!("phase=clone status=done");
        drop(cloned);
        println!("phase=debug status=begin");
        let debug = format!("{model:?}");
        assert!(debug.contains("leaf"));
        drop(debug);
        println!("phase=debug status=done");
        drop(parsed);
    } else {
        let error = result.expect_err("unclosed Block input must report its parser failure");
        assert!(error.to_string().contains("expected end for nested block"));
        println!("phase=parse status=error");
        drop(error);
    }
    println!("phase=drop status=done");
    engine
        .parse_diagram_for_render_model_sync("block\nsmall\n", ParseOptions::strict())
        .unwrap()
        .unwrap();
    println!("phase=after-small-operation status=done");
}

#[test]
fn block_lifecycle_child() {
    let Ok(case) = std::env::var(CASE_ENV) else {
        return;
    };
    std::thread::Builder::new()
        .name(format!("block-{case}"))
        .stack_size(STACK_BYTES)
        .spawn(move || run_worker(&case))
        .unwrap()
        .join()
        .unwrap();
}

fn run_child(case: &str) {
    let timestamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("diagnostic timestamp follows the Unix epoch")
        .as_nanos();
    let directory = std::env::temp_dir().join(format!(
        "merman-block-lifecycle-{}-{case}-{timestamp}",
        std::process::id()
    ));
    fs::create_dir(&directory).unwrap_or_else(|error| {
        panic!(
            "create Block lifecycle diagnostic directory {}: {error}",
            directory.display()
        )
    });
    let stdout = directory.join("stdout.txt");
    let stderr = directory.join("stderr.txt");
    let mut child = Command::new(std::env::current_exe().unwrap())
        .args(["--exact", "block_lifecycle_child", "--nocapture"])
        .env(CASE_ENV, case)
        .stdout(Stdio::from(File::create(&stdout).unwrap()))
        .stderr(Stdio::from(File::create(&stderr).unwrap()))
        .spawn()
        .unwrap();
    let started = Instant::now();
    let (status, timed_out) = loop {
        match child.try_wait() {
            Ok(Some(status)) => break (status, false),
            Ok(None) => {}
            Err(error) => {
                let _ = child.kill();
                let _ = child.wait();
                panic!("Block lifecycle child observation failed: {error}");
            }
        }
        if started.elapsed() >= Duration::from_secs(60) {
            let _ = child.kill();
            break (child.wait().unwrap(), true);
        }
        std::thread::sleep(Duration::from_millis(10));
    };
    let stdout = fs::read_to_string(stdout).unwrap();
    let stderr = fs::read_to_string(stderr).unwrap();
    let status_text = format!(
        "case={case} status={status} timed_out={timed_out} elapsed_ms={}",
        started.elapsed().as_millis()
    );
    fs::write(directory.join("status.txt"), &status_text).unwrap();
    println!(
        "{status_text}\ndiagnostics={}\nstdout:\n{stdout}\nstderr:\n{stderr}",
        directory.display()
    );
    assert!(!timed_out, "child timed out: {}", directory.display());
    assert!(status.success(), "child failed: {}", directory.display());
    assert!(stdout.contains("phase=drop status=done"));
    assert!(stdout.contains("phase=after-small-operation status=done"));
}

#[test]
fn block_valid_3000_clones_and_drops_flat_records() {
    run_child("valid");
}

#[test]
fn block_missing_outer_end_10000_disposes_completed_records() {
    run_child("missing-outer");
}

#[test]
fn block_missing_all_ends_10000_disposes_frames() {
    run_child("missing-all");
}

#[test]
fn block_partial_ends_10000_disposes_completed_and_open_records() {
    run_child("partial");
}
