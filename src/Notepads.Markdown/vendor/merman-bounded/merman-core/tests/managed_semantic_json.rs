use merman_core::ManagedSemanticJson;
use serde_json::{Value, json};
use std::io::{self, Write};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

fn chain(depth: usize) -> ManagedSemanticJson {
    let mut value = Value::String("leaf".to_owned());
    for _ in 0..depth {
        value = Value::Array(vec![value]);
    }
    value.into()
}

#[test]
fn compact_writer_preserves_scalar_representation_and_object_order() {
    let value = json!({
        "first": ["quote\"\\\n\t\u{0001}", "雪🦀", null, true],
        "numbers": [i64::MIN, u64::MAX, -0.0, 1.25, 1e30],
        "last": {}
    });
    let expected = serde_json::to_vec(&value).unwrap();
    let managed = ManagedSemanticJson::from(value);
    let mut actual = Vec::new();
    managed.write_json(&mut actual).unwrap();
    assert_eq!(actual, expected);
    assert_eq!(serde_json::to_vec(&managed).unwrap(), expected);
}

#[test]
fn pretty_writer_preserves_serde_whitespace_and_empty_containers() {
    for value in [
        json!({"empty": [{}, [], null], "nested": {"snow": "雪", "n": [1, -0.0]}}),
        json!([]),
        json!({}),
        json!("scalar\n"),
    ] {
        let expected = serde_json::to_vec_pretty(&value).unwrap();
        let mut actual = Vec::new();
        ManagedSemanticJson::from(value)
            .write_json_pretty(&mut actual)
            .unwrap();
        assert_eq!(actual, expected);
    }
}

#[test]
fn generic_serde_checks_actual_container_depth_before_serializing() {
    for depth in [127, 128] {
        let value = chain(depth);
        assert!(
            serde_json::to_vec(&value).is_ok(),
            "depth {depth} is supported"
        );
    }
    let error = serde_json::to_vec(&chain(129)).unwrap_err();
    assert!(error.to_string().contains("128"));
    // An empty container contributes to depth even though it has no child value.
    let mut empty = Value::Array(Vec::new());
    for _ in 0..128 {
        empty = Value::Array(vec![empty]);
    }
    assert!(serde_json::to_vec(&ManagedSemanticJson::from(empty)).is_err());
}

#[test]
fn object_equality_ignores_insertion_order_like_serde_json() {
    let left = ManagedSemanticJson::from(json!({"a": [1, 2], "b": "value"}));
    let right = ManagedSemanticJson::from(json!({"b": "value", "a": [1, 2]}));
    assert_eq!(left, right);
    assert_ne!(
        left,
        ManagedSemanticJson::from(json!({"a": [2, 1], "b": "value"}))
    );
}

#[test]
fn explicitly_unmanaged_extraction_preserves_the_payload() {
    let value = ManagedSemanticJson::from(json!({"value": [1, 2]}));
    assert_eq!(value.into_unmanaged_value(), json!({"value": [1, 2]}));
}

struct FailingWriter {
    remaining: usize,
}

impl Write for FailingWriter {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        if self.remaining == 0 {
            return Err(io::Error::other("intentional partial-output failure"));
        }
        let count = bytes.len().min(self.remaining);
        self.remaining -= count;
        Ok(count)
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

#[test]
fn controlled_writer_preserves_observed_cancellation_when_io_also_fails() {
    use merman_core::{CancelReason, OperationCancelled, OperationControl, OperationPhase};

    struct CancelAndFailWriter<'a> {
        control: &'a OperationControl,
        latch: bool,
        written: usize,
        observed: Option<OperationCancelled>,
    }

    impl Write for CancelAndFailWriter<'_> {
        fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
            if self.written == 0 {
                self.written += bytes.len();
                return Ok(bytes.len());
            }
            self.control.cancel();
            if self.latch {
                self.observed = Some(
                    self.control
                        .checkpoint_at(OperationPhase::Layout)
                        .unwrap_err(),
                );
            }
            Err(io::Error::other(
                "writer failed after requesting cancellation",
            ))
        }

        fn flush(&mut self) -> io::Result<()> {
            Ok(())
        }
    }

    for pretty in [false, true] {
        for latch in [false, true] {
            let control = OperationControl::new().for_phase(OperationPhase::Export);
            let value = ManagedSemanticJson::from(json!({"value": [1, 2, 3]}));
            let mut writer = CancelAndFailWriter {
                control: &control,
                latch,
                written: 0,
                observed: None,
            };
            let result = if pretty {
                value.write_json_pretty_controlled(&mut writer, &control)
            } else {
                value.write_json_controlled(&mut writer, &control)
            };
            assert!(writer.written > 0, "failure follows partial output");
            if let Some(observed) = writer.observed {
                assert_eq!(observed.reason, CancelReason::Requested);
                assert_eq!(observed.phase, OperationPhase::Layout);
                assert_eq!(result.unwrap_err(), observed);
                assert_eq!(control.checkpoint().unwrap_err(), observed);
            } else {
                assert!(result.unwrap().unwrap_err().is_io());
                let later = control.checkpoint().unwrap_err();
                assert_eq!(later.reason, CancelReason::Requested);
                assert_eq!(later.phase, OperationPhase::Export);
            }
        }
    }
}

#[cfg(feature = "operation-deadlines")]
struct DeadlineWriter<'a> {
    control: &'a merman_core::OperationControl,
    written: usize,
}

#[cfg(feature = "operation-deadlines")]
impl Write for DeadlineWriter<'_> {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        self.written += bytes.len();
        if self.written >= 256 {
            self.control.set_deadline(Duration::ZERO);
        }
        Ok(bytes.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

#[test]
fn managed_json_lifecycle_child() {
    if std::env::var_os("MERMAN_MANAGED_JSON_LIFECYCLE_CHILD").is_none() {
        return;
    }
    std::thread::Builder::new()
        .name("managed-json-lifecycle".to_owned())
        .stack_size(2 * 1024 * 1024)
        .spawn(|| {
            let value = chain(5_000);
            let cloned = value.clone();
            assert_eq!(value, cloned);
            let mut encoded = Vec::new();
            value.write_json(&mut encoded).unwrap();
            value.write_json_pretty(io::sink()).unwrap();
            assert_eq!(encoded.len(), 10_006);
            assert!(encoded.starts_with(b"[[[["));
            assert!(encoded.ends_with(b"]]]]"));
            let diagnostic = format!("{value:?}");
            assert!(diagnostic.contains("leaf"));
            assert!(serde_json::to_vec(&value).is_err());
            assert!(
                value
                    .write_json(FailingWriter { remaining: 3_000 })
                    .is_err()
            );
            #[cfg(feature = "operation-deadlines")]
            {
                let control = merman_core::OperationControl::new();
                let mut writer = DeadlineWriter {
                    control: &control,
                    written: 0,
                };
                let error = value
                    .write_json_controlled(&mut writer, &control)
                    .unwrap_err();
                assert!(
                    writer.written >= 256,
                    "deadline expires after output begins"
                );
                assert_eq!(error.reason, merman_core::CancelReason::DeadlineExceeded);
                assert_eq!(control.checkpoint().unwrap_err(), error);
            }
            #[cfg(feature = "test-support")]
            {
                let control = merman_core::OperationControl::new();
                control.cancel_after_checkpoints(120);
                assert!(value.clone_controlled(&control).is_err());
                let control = merman_core::OperationControl::new();
                control.cancel_after_checkpoints(20);
                assert!(value.write_json_controlled(io::sink(), &control).is_err());
            }
            drop(cloned);
            drop(value);
            println!("managed-deep-clone-equality-export-debug-drop=done");
            io::stdout().flush().unwrap();
            let source = "sequenceDiagram\nAlice->>Bob: Hello\n";
            let mut engine = merman_core::Engine::new();
            engine.diagram_registry_mut().insert("sequence", |_, _, _| {
                Ok(Ok(chain(5_000).into_unmanaged_value()))
            });
            let parsed = engine
                .parse_diagram_sync(source, merman_core::ParseOptions::strict())
                .unwrap()
                .unwrap();
            drop(parsed.clone());
            parsed.model.write_json(io::sink()).unwrap();
            drop(parsed);
            let snapshot = engine.parse_diagram_snapshot_sync(source).unwrap().unwrap();
            let model = snapshot.outcome().parsed_model().unwrap();
            model.write_json(io::sink()).unwrap();
            drop(model.clone());
            drop(snapshot);
            let typed = engine
                .parse_diagram_for_render_model_sync(source, merman_core::ParseOptions::strict())
                .unwrap()
                .unwrap();
            assert_eq!(
                merman_core::resources::ModelComplexity::from_render_model(typed.model())
                    .nesting_depth,
                4_999
            );
            assert!(
                merman_core::resources::InputResourcePolicy::default()
                    .check_render_model(typed.model())
                    .is_err()
            );
            drop(typed.clone());
            drop(typed);
            engine
                .diagram_registry_mut()
                .insert("sequence", |_, _, control| {
                    let value = chain(5_000);
                    control.cancel();
                    Ok(Ok(value.into_unmanaged_value()))
                });
            let control = merman_core::OperationControl::new();
            let error = engine
                .parse_diagram_for_render_model_controlled_sync(
                    source,
                    merman_core::ParseOptions::lenient(),
                    &control,
                )
                .unwrap_err();
            assert_eq!(error, control.checkpoint().unwrap_err());
            let control = merman_core::OperationControl::new();
            let error = engine
                .parse_diagram_snapshot_controlled_sync(source, &control)
                .unwrap_err();
            assert_eq!(error, control.checkpoint().unwrap_err());
            println!("managed-custom-overlay-snapshot-budget-cancellation-drop=done");
            io::stdout().flush().unwrap();
            let shallow = ManagedSemanticJson::from(json!({"still": "usable"}));
            shallow.write_json(io::sink()).unwrap();
            drop(shallow);
            println!("managed-after-small-operation=done");
            io::stdout().flush().unwrap();
        })
        .unwrap()
        .join()
        .unwrap();
}

#[test]
fn deep_managed_json_lifecycle_finishes_on_a_2_mib_worker() {
    let run_stamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let directory = std::env::temp_dir().join(format!(
        "merman-managed-json-{}-{run_stamp}",
        std::process::id()
    ));
    std::fs::create_dir(&directory).unwrap_or_else(|error| {
        panic!(
            "create managed JSON diagnostic directory {}: {error}",
            directory.display()
        )
    });
    let stdout_path = directory.join("stdout.txt");
    let stderr_path = directory.join("stderr.txt");
    let mut child = Command::new(std::env::current_exe().unwrap())
        .args(["--exact", "managed_json_lifecycle_child", "--nocapture"])
        .env("MERMAN_MANAGED_JSON_LIFECYCLE_CHILD", "1")
        .stdout(Stdio::from(std::fs::File::create(&stdout_path).unwrap()))
        .stderr(Stdio::from(std::fs::File::create(&stderr_path).unwrap()))
        .spawn()
        .unwrap();
    let started = Instant::now();
    let status = loop {
        match child.try_wait() {
            Ok(Some(status)) => break status,
            Ok(None) if started.elapsed() < Duration::from_secs(60) => {
                std::thread::sleep(Duration::from_millis(10));
            }
            result => {
                let _ = child.kill();
                let _ = child.wait();
                panic!(
                    "managed JSON child failed or timed out: {result:?}; {}",
                    directory.display()
                );
            }
        }
    };
    let stdout = std::fs::read_to_string(stdout_path).unwrap();
    let stderr = std::fs::read_to_string(stderr_path).unwrap();
    assert!(
        status.success(),
        "{status}; stdout={stdout}; stderr={stderr}"
    );
    assert!(stdout.contains("managed-deep-clone-equality-export-debug-drop=done"));
    assert!(stdout.contains("managed-custom-overlay-snapshot-budget-cancellation-drop=done"));
    assert!(stdout.contains("managed-after-small-operation=done"));
}
