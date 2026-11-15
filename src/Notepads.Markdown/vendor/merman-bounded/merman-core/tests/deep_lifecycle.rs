#![cfg(feature = "all-diagrams")]

use merman_core::{BuiltinRenderSemantic, Engine, ParseOptions};
use std::fmt::Write as _;
use std::fs::{self, File};
use std::io::{self, Write as _};
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

const CASE_ENV: &str = "MERMAN_CORE_DEEP_LIFECYCLE_CASE";
const DEPTH_ENV: &str = "MERMAN_CORE_DEEP_LIFECYCLE_DEPTH";
const WORKER_STACK_BYTES: usize = 2 * 1024 * 1024;
const CHILD_TIMEOUT: Duration = Duration::from_secs(60);
static CHILD_SEQUENCE: AtomicU64 = AtomicU64::new(0);

#[derive(Clone, Copy)]
enum Case {
    FlowchartValid,
    FlowchartMissingOuterEnd,
    StateTypedDrop,
    StateTypedCloneDrop,
    StateMissingOuterBrace,
    ErValid,
    ErMissingOuterEnd,
    C4MissingOuterBrace,
    BlockValid,
    BlockMissingOuterEnd,
    RailroadEbnfPostfix,
    MindmapJsonDrop,
    MindmapJsonCloneExport,
    TreemapTypedCloneDrop,
    IshikawaTypedCloneDrop,
    #[cfg(feature = "test-support")]
    StateProjectionCancel,
    #[cfg(feature = "test-support")]
    BlockProjectionCancel,
}

impl Case {
    const ALL: &'static [Self] = &[
        Self::FlowchartValid,
        Self::FlowchartMissingOuterEnd,
        Self::StateTypedDrop,
        Self::StateTypedCloneDrop,
        Self::StateMissingOuterBrace,
        Self::ErValid,
        Self::ErMissingOuterEnd,
        Self::C4MissingOuterBrace,
        Self::BlockValid,
        Self::BlockMissingOuterEnd,
        Self::RailroadEbnfPostfix,
        Self::MindmapJsonDrop,
        Self::MindmapJsonCloneExport,
        Self::TreemapTypedCloneDrop,
        Self::IshikawaTypedCloneDrop,
        #[cfg(feature = "test-support")]
        Self::StateProjectionCancel,
        #[cfg(feature = "test-support")]
        Self::BlockProjectionCancel,
    ];

    fn selector(self) -> &'static str {
        match self {
            Self::FlowchartValid => "flowchart-valid",
            Self::FlowchartMissingOuterEnd => "flowchart-missing-outer-end",
            Self::StateTypedDrop => "state-typed-drop",
            Self::StateTypedCloneDrop => "state-typed-clone-drop",
            Self::StateMissingOuterBrace => "state-missing-outer-brace",
            Self::ErValid => "er-valid",
            Self::ErMissingOuterEnd => "er-missing-outer-end",
            Self::C4MissingOuterBrace => "c4-missing-outer-brace",
            Self::BlockValid => "block-valid",
            Self::BlockMissingOuterEnd => "block-missing-outer-end",
            Self::RailroadEbnfPostfix => "railroad-ebnf-postfix",
            Self::MindmapJsonDrop => "mindmap-json-drop",
            Self::MindmapJsonCloneExport => "mindmap-json-clone-export",
            Self::TreemapTypedCloneDrop => "treemap-typed-clone-drop",
            Self::IshikawaTypedCloneDrop => "ishikawa-typed-clone-drop",
            #[cfg(feature = "test-support")]
            Self::StateProjectionCancel => "state-projection-cancel",
            #[cfg(feature = "test-support")]
            Self::BlockProjectionCancel => "block-projection-cancel",
        }
    }

    fn from_selector(selector: &str) -> Self {
        Self::ALL
            .iter()
            .copied()
            .find(|case| case.selector() == selector)
            .expect("known lifecycle case selector")
    }

    fn malformed(self) -> bool {
        matches!(
            self,
            Self::FlowchartMissingOuterEnd
                | Self::StateMissingOuterBrace
                | Self::ErMissingOuterEnd
                | Self::C4MissingOuterBrace
                | Self::BlockMissingOuterEnd
        )
    }

    fn source(self, depth: usize) -> String {
        #[cfg(feature = "test-support")]
        match self {
            Self::StateProjectionCancel => return Self::StateTypedDrop.source(depth),
            Self::BlockProjectionCancel => return Self::BlockValid.source(depth),
            _ => {}
        }
        let (header, closer, leaf) = match self {
            Self::FlowchartValid | Self::FlowchartMissingOuterEnd => {
                ("flowchart TD\n", "end\n", "leaf[Leaf]\n")
            }
            Self::StateTypedDrop | Self::StateTypedCloneDrop | Self::StateMissingOuterBrace => {
                ("stateDiagram-v2\n", "}\n", "leaf\n")
            }
            Self::ErValid | Self::ErMissingOuterEnd => ("erDiagram\n", "end\n", "LEAF\n"),
            Self::C4MissingOuterBrace => ("C4Context\n", "}\n", "System(leaf, \"Leaf\")\n"),
            Self::BlockValid | Self::BlockMissingOuterEnd => ("block-beta\n", "end\n", "leaf\n"),
            Self::RailroadEbnfPostfix => {
                return format!(
                    "railroad-ebnf-beta\nrule ::= \"a\"{} ;\n",
                    "?".repeat(depth)
                );
            }
            Self::MindmapJsonDrop | Self::MindmapJsonCloneExport => {
                let mut source = String::from("mindmap\nroot\n");
                for index in 0..depth {
                    writeln!(source, "{}n{index}", "  ".repeat(index + 1)).unwrap();
                }
                return source;
            }
            Self::TreemapTypedCloneDrop => {
                let mut source = String::from("treemap-beta\n");
                for index in 0..depth {
                    writeln!(source, "{}\"n{index}\"", "  ".repeat(index)).unwrap();
                }
                writeln!(source, "{}\"leaf\": 1", "  ".repeat(depth)).unwrap();
                return source;
            }
            Self::IshikawaTypedCloneDrop => {
                let mut source = String::from("ishikawa-beta\n  Root\n");
                for index in 0..depth {
                    writeln!(source, "{}Node {index}", "  ".repeat(index + 2)).unwrap();
                }
                return source;
            }
            #[cfg(feature = "test-support")]
            Self::StateProjectionCancel | Self::BlockProjectionCancel => {
                unreachable!("controlled projection sources returned above")
            }
        };
        let mut source = String::from(header);
        for index in 0..depth {
            match self {
                Self::FlowchartValid | Self::FlowchartMissingOuterEnd => {
                    writeln!(source, "subgraph n{index}").unwrap();
                }
                Self::StateTypedDrop | Self::StateTypedCloneDrop | Self::StateMissingOuterBrace => {
                    writeln!(source, "state s{index} {{").unwrap();
                }
                Self::ErValid | Self::ErMissingOuterEnd => {
                    writeln!(source, "subgraph G{index}").unwrap();
                }
                Self::C4MissingOuterBrace => {
                    writeln!(source, "Boundary(b{index}, \"B{index}\") {{").unwrap();
                }
                Self::BlockValid | Self::BlockMissingOuterEnd => {
                    writeln!(source, "block:b{index}").unwrap();
                }
                _ => unreachable!("indentation and postfix sources returned above"),
            }
        }
        source.push_str(leaf);
        for _ in 0..depth - usize::from(self.malformed()) {
            source.push_str(closer);
        }
        source
    }
}

fn marker(started: Instant, phase: &str, status: &str) {
    println!(
        "phase={phase} status={status} worker_elapsed_ms={}",
        started.elapsed().as_millis()
    );
    io::stdout().flush().expect("flush lifecycle phase marker");
}

fn after_small_operation(engine: &Engine, started: Instant) {
    marker(started, "after-small-operation", "begin");
    let small = engine
        .parse_diagram_for_render_model_sync("flowchart TD\na --> b\n", ParseOptions::strict())
        .expect("host remains able to parse a small diagram")
        .expect("small flowchart is detected");
    assert_eq!(small.metadata().diagram_type, "flowchart-v2");
    drop(small);
    marker(started, "after-small-operation", "done");
}

fn run_worker(case: Case, depth: usize) {
    let started = Instant::now();
    let source = case.source(depth);
    let path = if matches!(case, Case::MindmapJsonDrop | Case::MindmapJsonCloneExport) {
        "Engine.parse_diagram_sync"
    } else {
        "Engine.parse_diagram_for_render_model_sync"
    };
    println!(
        "case={} depth={depth} source_bytes={} path={path} profile=direct-engine-strict \
         worker_stack_bytes={WORKER_STACK_BYTES} build_mode={}",
        case.selector(),
        source.len(),
        if cfg!(debug_assertions) {
            "debug"
        } else {
            "release"
        }
    );
    io::stdout().flush().expect("flush lifecycle case metadata");
    let engine = Engine::new();
    marker(started, "parse", "begin");

    #[cfg(feature = "test-support")]
    if matches!(
        case,
        Case::StateProjectionCancel | Case::BlockProjectionCancel
    ) {
        let parsed = engine
            .parse_diagram_for_render_model_sync(&source, ParseOptions::strict())
            .unwrap()
            .unwrap();
        marker(started, "parse", "done");
        marker(started, "projection", "begin");
        let (meta, model) = parsed.into_parts();
        let control =
            merman_core::OperationControl::new().for_phase(merman_core::OperationPhase::Export);
        let (cancelled, scheduled_checkpoints) = match model {
            merman_core::RenderSemanticModel::State(mut model) => {
                // Retain one record to make HashMap iteration irrelevant. Its document still
                // owns the complete canonical chain. Projection descends through `depth - 1`
                // state statements and one leaf, then completes nested documents on ascent.
                model.states.retain(|id, _| id == "s0");
                assert_eq!(model.states.len(), 1);
                assert!(model.states["s0"].doc.is_some());
                // One record checkpoint precedes document steps checked every 128 steps.
                // Cancel halfway through ascent, after at least depth / 2 nested documents
                // have completed and become a deep managed value in an open parent frame.
                let scheduled = 1 + (depth + depth / 2 + 1).div_ceil(128);
                control.cancel_after_checkpoints(scheduled);
                let cancelled =
                    BuiltinRenderSemantic::compatibility_json_controlled(&model, &meta, &control)
                        .expect_err("projection cancels after completed inner documents");
                drop(model);
                (cancelled, scheduled)
            }
            merman_core::RenderSemanticModel::Block(model) => {
                assert_eq!(model.blocks_flat.len(), depth + 2);
                // Root-to-leaf descent uses `depth + 1` steps, followed by leaf completion.
                // One entry checkpoint plus each 128-step traversal checkpoint succeeds;
                // cancellation halfway through ascent releases a completed inner subtree
                // containing at least depth / 2 nested ancestors.
                let scheduled = 1 + (depth + depth / 2 + 3).div_ceil(128);
                control.cancel_after_checkpoints(scheduled);
                let cancelled =
                    BuiltinRenderSemantic::compatibility_json_controlled(&model, &meta, &control)
                        .expect_err("projection cancels after a completed inner Block subtree");
                drop(model);
                (cancelled, scheduled)
            }
            other => panic!("unexpected controlled projection family: {}", other.kind()),
        };
        assert_eq!(cancelled.reason, merman_core::CancelReason::Requested);
        assert_eq!(cancelled.phase, merman_core::OperationPhase::Export);
        assert_eq!(control.checkpoint().unwrap_err(), cancelled);
        println!("scheduled_checkpoints={scheduled_checkpoints}");
        marker(started, "projection", "cancelled-after-nested-completion");
        marker(started, "drop", "done");
        after_small_operation(&engine, started);
        return;
    }

    if matches!(case, Case::MindmapJsonDrop | Case::MindmapJsonCloneExport) {
        let parsed = engine
            .parse_diagram_sync(&source, ParseOptions::strict())
            .expect("Mindmap compatibility JSON parses")
            .expect("Mindmap is detected");
        marker(started, "parse", "done");
        assert_eq!(parsed.meta.diagram_type, "mindmap");
        marker(started, "model", "done");
        if matches!(case, Case::MindmapJsonCloneExport) {
            marker(started, "export", "begin");
            let mut encoded = Vec::new();
            parsed
                .model
                .write_json(&mut encoded)
                .expect("deep managed JSON export");
            assert!(encoded.starts_with(b"{") && encoded.ends_with(b"}"));
            if depth >= 128 {
                assert!(serde_json::to_vec(&parsed.model).is_err());
            }
            marker(started, "export", "done");
            marker(started, "clone", "begin");
            let cloned = parsed.clone();
            assert_eq!(cloned.model, parsed.model);
            marker(started, "clone", "done");
            drop(cloned);
        } else {
            marker(started, "export", "not-requested");
            marker(started, "clone", "not-requested");
        }
        marker(started, "drop", "begin");
        drop(parsed);
        marker(started, "drop", "done");
    } else {
        let parsed =
            match engine.parse_diagram_for_render_model_sync(&source, ParseOptions::strict()) {
                Ok(Some(parsed)) => {
                    assert!(
                        !case.malformed(),
                        "missing outer closer must report an error"
                    );
                    marker(started, "parse", "done");
                    parsed
                }
                Ok(None) => panic!("lifecycle source must detect a diagram"),
                Err(error) => {
                    assert!(
                        case.malformed()
                            || (matches!(case, Case::RailroadEbnfPostfix)
                                && depth > merman_core::MAX_DIAGRAM_NESTING_DEPTH),
                        "valid lifecycle source must parse: {error}"
                    );
                    marker(started, "parse", "reported-error");
                    println!("reported_error={error}");
                    io::stdout().flush().expect("flush reported family error");
                    marker(started, "model", "not-returned");
                    marker(started, "export", "not-requested");
                    marker(started, "clone", "not-requested");
                    marker(started, "drop", "begin");
                    drop(error);
                    marker(started, "drop", "done");
                    after_small_operation(&engine, started);
                    return;
                }
            };
        println!("model_kind={}", parsed.model().kind());
        marker(started, "model", "done");
        // A typed ownership probe must not introduce compatibility JSON ownership.
        marker(started, "export", "not-requested");
        if matches!(case, Case::StateTypedDrop) {
            marker(started, "clone", "not-requested");
        } else {
            marker(started, "clone", "begin");
            let cloned = parsed.clone();
            marker(started, "clone", "done");
            marker(started, "clone-drop", "begin");
            drop(cloned);
            marker(started, "clone-drop", "done");
        }
        marker(started, "drop", "begin");
        drop(parsed);
        marker(started, "drop", "done");
    }
    after_small_operation(&engine, started);
}

fn run_child(case: Case, depth: usize) {
    let started = Instant::now();
    let timestamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("diagnostic timestamp follows the Unix epoch")
        .as_nanos();
    let directory = std::env::temp_dir().join(format!(
        "merman-core-deep-lifecycle-{}-{}-{depth}-{}-{timestamp}",
        std::process::id(),
        case.selector(),
        CHILD_SEQUENCE.fetch_add(1, Ordering::Relaxed)
    ));
    fs::create_dir(&directory).unwrap_or_else(|error| {
        panic!(
            "create child diagnostic directory {}: {error}",
            directory.display()
        )
    });
    let stdout_path = directory.join("stdout.txt");
    let stderr_path = directory.join("stderr.txt");
    let mut child = Command::new(std::env::current_exe().expect("current test executable"))
        .args(["--exact", "lifecycle_child", "--nocapture"])
        .env(CASE_ENV, case.selector())
        .env(DEPTH_ENV, depth.to_string())
        .stdout(Stdio::from(
            File::create(&stdout_path).expect("create child stdout file"),
        ))
        .stderr(Stdio::from(
            File::create(&stderr_path).expect("create child stderr file"),
        ))
        .spawn()
        .expect("spawn lifecycle child");
    let (status, timed_out, process_error) = loop {
        match child.try_wait() {
            Ok(Some(status)) => break (status, false, None),
            Ok(None) => {}
            Err(error) => {
                let _ = child.kill();
                let status = child.wait().expect("reap child after wait failure");
                break (status, false, Some(error.to_string()));
            }
        }
        if started.elapsed() >= CHILD_TIMEOUT {
            let kill_error = child.kill().err().map(|error| error.to_string());
            let status = child.wait().expect("reap timed-out lifecycle child");
            break (status, true, kill_error);
        }
        std::thread::sleep(Duration::from_millis(10));
    };
    let stdout = fs::read_to_string(&stdout_path).expect("read child stdout");
    let stderr = fs::read_to_string(&stderr_path).expect("read child stderr");
    let summary = format!(
        "case={} depth={depth} status={status} timed_out={timed_out} \
         parent_elapsed_ms={} timeout_ms={} process_error={process_error:?}",
        case.selector(),
        started.elapsed().as_millis(),
        CHILD_TIMEOUT.as_millis()
    );
    fs::write(directory.join("status.txt"), &summary).expect("preserve child exit status");
    println!(
        "{summary}\ndiagnostics={}\nstdout:\n{stdout}\nstderr:\n{stderr}",
        directory.display()
    );
    assert!(
        !timed_out,
        "lifecycle child timed out; diagnostics={}",
        directory.display()
    );
    assert!(
        process_error.is_none(),
        "child process observation failed; {summary}"
    );
    assert!(
        status.success(),
        "lifecycle child failed; diagnostics={}",
        directory.display()
    );
    assert!(
        stdout.contains("phase=drop status=done"),
        "child must finish ordinary disposal"
    );
    assert!(
        stdout.contains("phase=after-small-operation status=done"),
        "child must complete a subsequent small operation"
    );
    #[cfg(feature = "test-support")]
    if matches!(
        case,
        Case::StateProjectionCancel | Case::BlockProjectionCancel
    ) {
        assert!(stdout.contains("phase=projection status=cancelled-after-nested-completion"));
    }
}

#[test]
fn lifecycle_child() {
    let Ok(selector) = std::env::var(CASE_ENV) else {
        return;
    };
    let case = Case::from_selector(&selector);
    let depth = std::env::var(DEPTH_ENV)
        .expect("child logical depth")
        .parse::<usize>()
        .expect("numeric child logical depth");
    assert!(depth > 0, "lifecycle nesting depth must be positive");
    std::thread::Builder::new()
        .name(format!("lifecycle-{}", case.selector()))
        .stack_size(WORKER_STACK_BYTES)
        .spawn(move || run_worker(case, depth))
        .expect("spawn 2 MiB lifecycle worker")
        .join()
        .expect("lifecycle worker completes");
}

#[test]
fn small_lifecycle_cases_complete_on_host_stack() {
    for &case in Case::ALL {
        run_child(case, 4);
    }
}

#[test]
fn deep_flowchart_valid_10000() {
    run_child(Case::FlowchartValid, 10_000);
}

#[test]
fn deep_flowchart_missing_outer_end_10000() {
    run_child(Case::FlowchartMissingOuterEnd, 10_000);
}

#[test]
fn deep_state_typed_drop_5000() {
    run_child(Case::StateTypedDrop, 5_000);
}

#[test]
fn deep_state_typed_clone_drop_5000() {
    run_child(Case::StateTypedCloneDrop, 5_000);
}

#[test]
fn deep_state_missing_outer_brace_10000() {
    run_child(Case::StateMissingOuterBrace, 10_000);
}

#[cfg(feature = "test-support")]
#[test]
fn deep_state_projection_cancellation_releases_completed_documents_on_host_stack() {
    run_child(Case::StateProjectionCancel, 3_000);
}

#[cfg(feature = "test-support")]
#[test]
fn deep_block_projection_cancellation_releases_completed_subtrees_on_host_stack() {
    run_child(Case::BlockProjectionCancel, 3_000);
}

#[test]
fn deep_er_valid_3000() {
    run_child(Case::ErValid, 3_000);
}

#[test]
fn deep_er_missing_outer_end_3000() {
    run_child(Case::ErMissingOuterEnd, 3_000);
}

#[test]
fn deep_c4_missing_outer_brace_15000() {
    run_child(Case::C4MissingOuterBrace, 15_000);
}

#[test]
fn deep_block_valid_3000() {
    run_child(Case::BlockValid, 3_000);
}

#[test]
fn deep_block_missing_outer_end_10000() {
    run_child(Case::BlockMissingOuterEnd, 10_000);
}

#[test]
fn deep_railroad_ebnf_postfix_5000() {
    run_child(Case::RailroadEbnfPostfix, 5_000);
}

#[test]
fn deep_mindmap_json_drop_3000() {
    run_child(Case::MindmapJsonDrop, 3_000);
}

#[test]
fn deep_treemap_typed_clone_drop_5000() {
    run_child(Case::TreemapTypedCloneDrop, 5_000);
}

#[test]
fn deep_ishikawa_typed_clone_drop_5000() {
    run_child(Case::IshikawaTypedCloneDrop, 5_000);
}

#[test]
fn deep_mindmap_json_clone_export_3000() {
    run_child(Case::MindmapJsonCloneExport, 3_000);
}
