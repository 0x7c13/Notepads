// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE file in the project root for license information.
// ---------------------------------------------------------------------------------------------

//! Ownership/ABI mechanism tests. Test-only stack values never qualify a native
//! entry profile, and a std thread join is not UWP/AppContainer UI acceptance.

use super::*;

#[test]
fn native_derived_table_extents_match_the_shared_core_admission_policy() {
    use markdown_core::resources::derived::{
        MAX_CONTROL_RESIDENTS, MAX_PROCESS_RESIDENTS,
    };
    let header = include_str!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../include/notepads/markdown/core.h"
    ));
    for (name, value) in [
        ("NOTEPADS_MARKDOWN_DERIVED_CONTROL_RESIDENT_LIMIT", MAX_CONTROL_RESIDENTS),
        ("NOTEPADS_MARKDOWN_DERIVED_PROCESS_RESIDENT_LIMIT", MAX_PROCESS_RESIDENTS),
    ] {
        let declaration = format!("#define {name} {value}u");
        assert!(
            header.lines().any(|line| line.trim() == declaration),
            "{name}: native array extent and Rust admission must share the same bound"
        );
    }
}
use markdown_core::config::Extensions;
use markdown_core::resources::derived::JOB_BYTES;
use markdown_core::source::SourceDocument;

fn model(root: &SourceBudget, text: &[u8]) -> *mut Owned<CoreModel> {
    let mut op = root.begin(CancellationToken::new());
    let document =
        SourceDocument::new(SourceKey::new([212; 16], 1).unwrap(), text, &mut op).unwrap();
    let model = AcceptedModel::parse(
        document.snapshot(),
        ParseConfig::with_extensions(Profile::G3, Extensions::MATH),
        1,
        &mut op,
    )
    .unwrap();
    Slot::new(op.boundary_work())
        .unwrap()
        .commit(CoreModel { model })
}
struct Fixture {
    source: SourceBudget,
    runtime: *mut MathRuntime,
    control: *mut MathControl,
    model: *mut Owned<CoreModel>,
}
impl Fixture {
    fn new() -> Self {
        let source = SourceBudget::preview();
        let model = model(&source, b"before $x^2$ after");
        let mut runtime = ptr::null_mut();
        let mut control = ptr::null_mut();
        let mut error = WireError::default();
        // SAFETY: independent initialized outputs; fixture owns all returned handles.
        unsafe {
            assert_eq!(notepads_markdown_math_runtime_create(&mut runtime, &mut error), 0);
            assert_eq!(notepads_markdown_math_control_create(runtime, &mut control, &mut error), 0);
        }
        Self {
            source,
            runtime,
            control,
            model,
        }
    }
    fn prepare(&self) -> *mut MathJob {
        let mut job = ptr::null_mut();
        let mut error = WireError::default();
        // SAFETY: fixture handles are live and outputs independent.
        unsafe {
            assert_eq!(
                notepads_markdown_math_job_prepare(self.control, self.model, 1, &mut job, &mut error),
                0
            );
        }
        job
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        let mut error = WireError::default();
        // SAFETY: tests release all jobs before the fixture; each remaining reference is owned once.
        unsafe {
            notepads_markdown_core_model_destroy(self.model);
            notepads_markdown_math_control_release(self.control, &mut error);
            notepads_markdown_math_runtime_release(self.runtime, &mut error);
        }
    }
}
unsafe fn dispose(job: *mut MathJob) {
    let mut error = WireError::default();
    // SAFETY: test caller has joined any worker and freed copied native arrays.
    unsafe {
        assert_eq!(notepads_markdown_math_job_release(job, &mut error), 0);
    }
}
fn test_stack() -> WorkerStackContract {
    // Mechanism only: this is never exposed through any exported ABI or default.
    WorkerStackContract {
        usable_headroom_bytes: 128 * 1024,
        platform_margin_bytes: 64 * 1024,
        audited_required_bytes: Some(1),
    }
}
unsafe fn compute_mechanism(job: *mut MathJob) {
    // SAFETY: this test owns exclusive nonconcurrent access; no native worker exists.
    unsafe {
        let data = &mut *(*job).data.get();
        data.job
            .as_mut()
            .unwrap()
            .run(
                &(*job).cancelled,
                &FontAssets::new(),
                &MathLimits,
                test_stack(),
            )
            .unwrap();
        (*job).state.store(COMPUTED, Ordering::Release);
    }
}

#[test]
fn math_preflight_keeps_success_pods_and_work_state_unchanged() {
    let fixture = Fixture::new();
    let job = fixture.prepare();
    let mut error = WireError::default();
    let mut summary = MathSummary {
        header: math_header_value::<MathSummary>(),
        commands: 79,
        ..Default::default()
    };
    let original = summary;
    // SAFETY: valid owned handles and independent fully allocated output/error PODs.
    unsafe {
        summary.header.abi_version = 0;
        assert_eq!(
            notepads_markdown_math_job_summary(job, &mut summary, &mut error),
            STATUS_ABI_MISMATCH
        );
        summary.header = original.header;
        assert_eq!(notepads_markdown_math_job_summary(job, &mut summary, &mut error), 5);
        assert_eq!(summary, original);
        let mut row = MathCommand::default();
        let mut output = MathReadResult {
            header: math_header_value::<MathReadResult>(),
            written: 93,
            ..Default::default()
        };
        let original = output;
        assert_eq!(
            notepads_markdown_math_job_read(
                job,
                1,
                0,
                257,
                56,
                ptr::from_mut(&mut row).cast(),
                &mut output,
                &mut error
            ),
            1
        );
        assert_eq!(output, original);
        assert_eq!(
            notepads_markdown_math_job_read(
                job,
                1,
                0,
                1,
                55,
                ptr::from_mut(&mut row).cast(),
                &mut output,
                &mut error
            ),
            1
        );
        assert_eq!(output, original);
        assert_eq!(notepads_markdown_math_job_state(job), QUEUED);
        error.header.abi_version = 0;
        assert_eq!(notepads_markdown_math_job_activate(job, &mut error), STATUS_ABI_MISMATCH);
        assert_eq!(notepads_markdown_math_job_state(job), QUEUED);
        dispose(job);
    }
}

#[test]
fn native_stack_profile_uses_the_fixed_total_threshold() {
    let empty = native_stack_contract(0);
    assert!(matches!(
        empty.admit(),
        Err(MathError::StackUnavailable { .. })
    ));
    let below = native_stack_contract(1_572_863);
    assert!(matches!(
        below.admit(),
        Err(MathError::StackUnavailable { .. })
    ));
    let exact = native_stack_contract(1_572_864);
    if exact.audited_required_bytes.is_some() {
        assert_eq!(exact.audited_required_bytes, Some(1_507_328));
        assert_eq!(exact.platform_margin_bytes, 65_536);
        assert!(exact.admit().is_ok());
    } else {
        assert!(matches!(
            exact.admit(),
            Err(MathError::StackUnavailable { .. })
        ));
    }
}

#[test]
fn native_start_claim_is_consumed_once_even_without_a_worker() {
    let fixture = Fixture::new();
    let job = fixture.prepare();
    let mut error = WireError::default();
    // SAFETY: exclusive fixture/context; no worker or concurrent data access.
    unsafe {
        assert_eq!(notepads_markdown_math_job_claim_start(job, &mut error), 0);
        assert_eq!(notepads_markdown_math_job_activate(job, &mut error), 0);
        let source = fixture.source.usage();
        let policy = (*fixture.control).policy.usage();
        assert_eq!(notepads_markdown_math_job_claim_start(job, &mut error), 5);
        assert_eq!(notepads_markdown_math_job_claim_start(job, &mut error), 5);
        assert_eq!(notepads_markdown_math_job_state(job), ACTIVATED);
        assert_eq!(fixture.source.usage(), source);
        assert_eq!((*fixture.control).policy.usage(), policy);
        dispose(job);
        assert_eq!((*fixture.control).policy.usage().jobs, 0);
    }
}

#[test]
fn metadata_retirement_releases_worker_but_keeps_original_context_and_resident() {
    let fixture = Fixture::new();
    let base = fixture.source.usage().auxiliary_capacity;
    let job = fixture.prepare();
    let metadata = size_of::<MathJob>() as u64 + NATIVE_JOB_BYTES;
    let mut error = WireError::default();
    // SAFETY: unique context, no thread was ever created and no native authority exists.
    unsafe {
        assert_eq!(notepads_markdown_math_job_activate(job, &mut error), 0);
        assert_eq!((*fixture.control).policy.usage().jobs, 1);
        assert_eq!(notepads_markdown_math_job_retire_metadata(job, &mut error), 0);
        assert_eq!(notepads_markdown_math_job_state(job), METADATA_ONLY);
        let value = (*fixture.control).policy.usage();
        assert_eq!(value.jobs, 0);
        assert_eq!(value.retained_bytes, 0);
        assert_eq!(value.metadata_bytes, 1024 + metadata);
        assert_eq!(value.residents, 1);
        assert_eq!(fixture.source.usage().auxiliary_capacity, base + metadata);
        assert_eq!(notepads_markdown_math_job_retire_metadata(job, &mut error), 5);
        assert_eq!(notepads_markdown_math_job_activate(job, &mut error), 5);
        assert_eq!(notepads_markdown_math_job_claim_start(job, &mut error), 5);
        assert_eq!(notepads_markdown_math_job_run(job, u64::MAX, &mut error), 5);
        assert_eq!(notepads_markdown_math_job_charge_native(job, 3, 1, 1, &mut error), 5);
        assert_eq!(notepads_markdown_math_job_finalize_native(job, metadata, &mut error), 5);
        assert_eq!(notepads_markdown_math_job_cancel(job, &mut error), 0);
        dispose(job);
        assert_eq!((*fixture.control).policy.usage().residents, 0);
        assert_eq!(fixture.source.usage().auxiliary_capacity, base);
    }
}

#[test]
fn metadata_retirement_refuses_extra_authority_and_running_or_finalized_states() {
    let fixture = Fixture::new();
    let job = fixture.prepare();
    let mut error = WireError::default();
    // SAFETY: test owns both references; no actual worker exists. The manual
    // states below test refusal only and are restored before any payload access.
    unsafe {
        assert_eq!(notepads_markdown_math_job_activate(job, &mut error), 0);
        assert_eq!(notepads_markdown_math_job_retain(job, &mut error), 0);
        let before = (*fixture.control).policy.usage();
        assert_eq!(notepads_markdown_math_job_retire_metadata(job, &mut error), 5);
        assert_eq!((*fixture.control).policy.usage(), before);
        assert_eq!(notepads_markdown_math_job_release(job, &mut error), 0);
        for state in [RUNNING, FINALIZED, POISONED] {
            (*job).state.store(state, Ordering::Release);
            assert_eq!(notepads_markdown_math_job_retire_metadata(job, &mut error), 5);
            assert_eq!((*fixture.control).policy.usage(), before);
        }
        (*job).state.store(ACTIVATED, Ordering::Release);
        assert_eq!(notepads_markdown_math_job_retire_metadata(job, &mut error), 0);
        dispose(job);
    }
}

#[test]
fn computed_failure_can_retire_to_metadata_and_make_room_without_another_source() {
    let fixture = Fixture::new();
    let first = fixture.prepare();
    let second = fixture.prepare();
    let mut error = WireError::default();
    // SAFETY: mechanism test invokes synchronously, not a UWP thread-exit proof.
    unsafe {
        assert_eq!(notepads_markdown_math_job_activate(first, &mut error), 0);
        assert_eq!(notepads_markdown_math_job_activate(second, &mut error), 9);
        compute_mechanism(first);
        assert_eq!(notepads_markdown_math_job_retire_metadata(first, &mut error), 0);
        assert_eq!((*fixture.control).policy.usage().jobs, 0);
        assert_eq!((*fixture.control).policy.usage().residents, 2);
        assert_eq!(notepads_markdown_math_job_activate(second, &mut error), 0);
        assert_eq!(notepads_markdown_math_job_state(first), METADATA_ONLY);
        dispose(first);
        dispose(second);
    }
}

#[test]
fn queued_retirement_preserves_original_deadline_and_wire_resident_gauge() {
    let fixture = Fixture::new();
    let job = fixture.prepare();
    let mut error = WireError::default();
    let mut output = MathUsage {
        header: math_header_value::<MathUsage>(),
        ..Default::default()
    };
    // SAFETY: exclusive queued context and fully initialized independent outputs.
    unsafe {
        let deadline = (*(*job).data.get()).deadline;
        assert_eq!(notepads_markdown_math_job_retire_metadata(job, &mut error), 0);
        assert_eq!((*(*job).data.get()).deadline, deadline);
        assert_eq!(
            notepads_markdown_math_control_usage(fixture.control, &mut output, &mut error),
            0
        );
        assert_eq!(output.control.residents, 1);
        assert_eq!(output.process.residents, 1);
        assert_eq!(output.control.jobs, 0);
        assert_eq!(notepads_markdown_math_job_retain(job, &mut error), 0);
        dispose(job);
        assert_eq!((*fixture.control).policy.usage().residents, 1);
        dispose(job);
        assert_eq!((*fixture.control).policy.usage().residents, 0);
    }
}

#[test]
fn queued_context_keeps_independent_metadata_through_active_same_source_job() {
    let mut fixture = Fixture::new();
    let base = fixture.source.usage().auxiliary_capacity;
    let job = fixture.prepare();
    let expected = size_of::<MathJob>() as u64 + NATIVE_JOB_BYTES;
    let mut error = WireError::default();
    // SAFETY: exclusive queued/activated context, no worker has started.
    unsafe {
        assert_eq!(fixture.source.usage().auxiliary_capacity, base + expected);
        assert_eq!((*fixture.control).policy.usage().jobs, 0);
        let deadline = (*(*job).data.get()).deadline;
        assert_eq!(notepads_markdown_math_job_activate(job, &mut error), 0);
        assert_eq!((*(*job).data.get()).deadline, deadline);
        assert_eq!(
            fixture.source.usage().auxiliary_capacity,
            base + JOB_BYTES + expected
        );
        assert_eq!((*fixture.control).policy.usage().jobs, 1);
        assert_eq!(
            (*fixture.control).policy.usage().metadata_bytes,
            1024 + expected
        );
        assert_eq!(
            (*(*job).data.get())
                .job
                .as_ref()
                .unwrap()
                .prepaid_metadata_bytes(),
            expected
        );
        assert_eq!(notepads_markdown_math_job_activate(job, &mut error), 5);
        assert_eq!(
            fixture.source.usage().auxiliary_capacity,
            base + JOB_BYTES + expected
        );
        assert_eq!(notepads_markdown_core_model_destroy(fixture.model), 0);
        fixture.model = ptr::null_mut();
        dispose(job);
    }
    assert_eq!(fixture.source.usage().auxiliary_capacity, 0);
}

#[test]
fn all_controls_share_one_process_and_metadata_survives_job_refusal() {
    let fixture = Fixture::new();
    let first = fixture.prepare();
    let mut second_control = ptr::null_mut();
    let mut third_control = ptr::null_mut();
    let mut second = ptr::null_mut();
    let mut third = ptr::null_mut();
    let mut error = WireError::default();
    // SAFETY: fixture owns runtime/model; each output is a fresh independently owned handle.
    unsafe {
        assert_eq!(
            notepads_markdown_math_control_create(fixture.runtime, &mut second_control, &mut error),
            0
        );
        assert_eq!(
            notepads_markdown_math_control_create(fixture.runtime, &mut third_control, &mut error),
            0
        );
        assert_eq!(
            notepads_markdown_math_job_prepare(second_control, fixture.model, 1, &mut second, &mut error),
            0
        );
        assert_eq!(
            notepads_markdown_math_job_prepare(third_control, fixture.model, 1, &mut third, &mut error),
            0
        );
        assert_eq!(notepads_markdown_math_job_activate(first, &mut error), 0);
        assert_eq!(notepads_markdown_math_job_activate(second, &mut error), 0);
        let before = (*fixture.runtime).policy.usage();
        let source_before = fixture.source.usage().auxiliary_capacity;
        assert_eq!(notepads_markdown_math_job_activate(third, &mut error), 9);
        assert_eq!(error.limit_kind, 103);
        assert_eq!((*fixture.runtime).policy.usage(), before);
        assert_eq!(fixture.source.usage().auxiliary_capacity, source_before);
        assert_eq!(notepads_markdown_math_job_state(third), QUEUED);
        dispose(first);
        assert_eq!(notepads_markdown_math_job_activate(third, &mut error), 0);
        dispose(second);
        dispose(third);
        assert_eq!((*fixture.runtime).policy.usage().jobs, 0);
        assert_eq!(notepads_markdown_math_control_release(second_control, &mut error), 0);
        assert_eq!(notepads_markdown_math_control_release(third_control, &mut error), 0);
    }
}

#[test]
fn queue_source_exhaustion_and_cancelled_activation_have_no_partial_job() {
    let fixture = Fixture::new();
    let before = fixture.source.usage().auxiliary_capacity;
    let bytes = size_of::<MathJob>() as u64 + NATIVE_JOB_BYTES;
    let blocker = fixture
        .source
        .reserve_resource_capacity(fixture.source.limits().auxiliary_capacity - before - bytes + 1)
        .unwrap();
    let blocked = fixture.source.usage().auxiliary_capacity;
    let mut output = ptr::dangling_mut::<MathJob>();
    let mut error = WireError::default();
    // SAFETY: valid handles and independent output/error; no worker exists.
    unsafe {
        assert_eq!(
            notepads_markdown_math_job_prepare(fixture.control, fixture.model, 1, &mut output, &mut error),
            9
        );
        assert!(output.is_null());
        assert_eq!(fixture.source.usage().auxiliary_capacity, blocked);
        assert_eq!((*fixture.control).policy.usage().jobs, 0);
        drop(blocker);
        let job = fixture.prepare();
        assert_eq!(notepads_markdown_math_job_cancel(job, &mut error), 0);
        assert_eq!(notepads_markdown_math_job_activate(job, &mut error), 4);
        assert_eq!(notepads_markdown_math_job_state(job), QUEUED);
        assert_eq!((*fixture.control).policy.usage().jobs, 0);
        assert_eq!(fixture.source.usage().auxiliary_capacity, before + bytes);
        dispose(job);
    }
    assert_eq!(fixture.source.usage().auxiliary_capacity, before);
}

#[test]
fn returned_worker_keeps_full_funding_until_real_thread_join_and_cleanup() {
    let fixture = Fixture::new();
    let job = fixture.prepare();
    let mut error = WireError::default();
    // SAFETY: exclusively owned activation; parent keeps this job reference until join.
    unsafe {
        assert_eq!(notepads_markdown_math_job_activate(job, &mut error), 0);
        let funded = fixture.source.usage().auxiliary_capacity;
        assert_eq!(
            notepads_markdown_math_job_run(job, 0, &mut error),
            crate::STATUS_INVALID_STATE
        );
        assert_eq!(notepads_markdown_math_job_state(job), ACTIVATED);
        assert_eq!(fixture.source.usage().auxiliary_capacity, funded);
        assert_eq!((*fixture.control).policy.usage().jobs, 1);
        assert_eq!(notepads_markdown_math_job_claim_start(job, &mut error), 0);
    }
    let address = job as usize;
    let (returned_tx, returned_rx) = std::sync::mpsc::channel();
    let (leave_tx, leave_rx) = std::sync::mpsc::channel();
    let worker = std::thread::spawn(move || {
        let job = address as *mut MathJob;
        let mut error = WireError::default();
        // SAFETY: unique worker call; parent owns the reference through actual join.
        // No trusted native stack measurement on this mechanism-only std
        // thread. Supply zero: a qualified profile must still refuse parsing.
        let code = unsafe { notepads_markdown_math_job_run(job, 0, &mut error) };
        returned_tx.send((code, error.actual)).unwrap();
        leave_rx.recv().unwrap();
    });
    let returned = returned_rx.recv().unwrap();
    // No post-join API is called yet: only public atomic state and policy gauges.
    let state = unsafe { notepads_markdown_math_job_state(job) };
    let policy = unsafe { (*fixture.control).policy.usage() };
    leave_tx.send(()).unwrap();
    worker.join().unwrap();
    assert_eq!(returned, (100, 0));
    assert_eq!(state, COMPUTED);
    assert_eq!(policy.jobs, 1);
    assert_eq!(policy.retained_bytes, 0);
    // SAFETY: real std thread has joined; independent summary retains its success sentinel.
    unsafe {
        let mut summary = MathSummary {
            header: math_header_value::<MathSummary>(),
            commands: 71,
            ..Default::default()
        };
        let original = summary;
        assert_eq!(notepads_markdown_math_job_summary(job, &mut summary, &mut error), 100);
        assert_eq!(summary, original);
        assert_eq!(
            (*(*job).data.get()).job.as_ref().unwrap().reserved_bytes(),
            JOB_BYTES
        );
        dispose(job);
        assert_eq!((*fixture.control).policy.usage().jobs, 0);
    }
}

#[test]
fn mechanism_geometry_batches_finalize_after_actual_copies_and_last_free() {
    let mut fixture = Fixture::new();
    let job = fixture.prepare();
    let mut error = WireError::default();
    // SAFETY: no OS worker; explicit mechanism-only profile never enters exported run.
    unsafe {
        assert_eq!(notepads_markdown_math_job_activate(job, &mut error), 0);
        compute_mechanism(job);
        let mut summary = MathSummary {
            header: math_header_value::<MathSummary>(),
            ..Default::default()
        };
        assert_eq!(notepads_markdown_math_job_summary(job, &mut summary, &mut error), 0);
        assert!(summary.commands > 0 && summary.primitives > 0);
        let commands_bytes = summary.commands * size_of::<MathCommand>() as u64;
        let primitive_bytes = summary.primitives * size_of::<MathPrimitive>() as u64;
        assert_eq!(
            notepads_markdown_math_job_charge_native(job, 1, commands_bytes, summary.commands, &mut error),
            0
        );
        let mut commands = vec![MathCommand::default(); summary.commands as usize];
        assert_eq!(
            notepads_markdown_math_job_charge_native(job, 2, primitive_bytes, summary.primitives, &mut error),
            0
        );
        let mut primitives = vec![MathPrimitive::default(); summary.primitives as usize];
        let mut result = MathReadResult {
            header: math_header_value::<MathReadResult>(),
            ..Default::default()
        };
        assert_eq!(
            notepads_markdown_math_job_read(
                job,
                1,
                0,
                summary.commands,
                56,
                commands.as_mut_ptr().cast(),
                &mut result,
                &mut error
            ),
            0
        );
        assert_eq!(result.written, summary.commands);
        assert_eq!(
            notepads_markdown_math_job_read(
                job,
                2,
                0,
                summary.primitives,
                40,
                primitives.as_mut_ptr().cast(),
                &mut result,
                &mut error
            ),
            0
        );
        assert_eq!(result.written, summary.primitives);
        assert!(commands.iter().any(|row| row.kind == 5));
        assert_eq!(notepads_markdown_core_model_destroy(fixture.model), 0);
        fixture.model = ptr::null_mut();
        let actual = summary.prepaid_metadata_bytes + commands_bytes + primitive_bytes;
        assert_eq!(notepads_markdown_math_job_finalize_native(job, actual, &mut error), 0);
        assert_eq!(notepads_markdown_math_job_state(job), FINALIZED);
        assert_eq!((*fixture.control).policy.usage().jobs, 0);
        assert_eq!((*fixture.control).policy.usage().retained_bytes, actual);
        assert_eq!(
            fixture.source.usage().auxiliary_capacity,
            actual + size_of::<MathJob>() as u64 + NATIVE_JOB_BYTES
        );
        assert_eq!(notepads_markdown_math_job_finalize_native(job, actual, &mut error), 5);
        assert_eq!(notepads_markdown_math_job_retain(job, &mut error), 0);
        assert_eq!(notepads_markdown_math_job_release(job, &mut error), 0);
        assert_eq!(
            fixture.source.usage().auxiliary_capacity,
            actual + size_of::<MathJob>() as u64 + NATIVE_JOB_BYTES
        );
        drop(commands);
        drop(primitives);
        dispose(job);
    }
    assert_eq!(fixture.source.usage().auxiliary_capacity, 0);
}

#[test]
fn failed_native_finalization_is_single_attempt_and_keeps_full_grant() {
    let fixture = Fixture::new();
    let job = fixture.prepare();
    let mut error = WireError::default();
    // SAFETY: exclusive mechanism-only execution, then no live worker/native allocation.
    unsafe {
        assert_eq!(notepads_markdown_math_job_activate(job, &mut error), 0);
        compute_mechanism(job);
        let before = (*fixture.control).policy.usage();
        let source_before = fixture.source.usage().auxiliary_capacity;
        assert_eq!(notepads_markdown_math_job_finalize_native(job, u64::MAX, &mut error), 102);
        assert_eq!(notepads_markdown_math_job_state(job), FINISH_FAILED);
        assert_eq!((*fixture.control).policy.usage(), before);
        // Finalize drops the model pin but fixture still owns its original model handle.
        assert_eq!(fixture.source.usage().auxiliary_capacity, source_before);
        assert_eq!(
            (*(*job).data.get()).job.as_ref().unwrap().reserved_bytes(),
            JOB_BYTES
        );
        assert_eq!(notepads_markdown_math_job_finalize_native(job, 4096, &mut error), 5);
        assert_eq!((*fixture.control).policy.usage().jobs, 1);
        dispose(job);
    }
}

#[test]
fn invalid_objects_null_handles_and_panics_never_publish_or_unwind() {
    let fixture = Fixture::new();
    let mut output = ptr::dangling_mut::<MathJob>();
    let mut error = WireError::default();
    // SAFETY: valid independent outputs; null/invalid scalar inputs exercise preflight only.
    unsafe {
        assert_eq!(
            notepads_markdown_math_job_prepare(fixture.control, fixture.model, 0, &mut output, &mut error),
            17
        );
        assert!(output.is_null());
        assert_eq!(notepads_markdown_math_job_cancel(ptr::null_mut(), &mut error), 1);
        assert_eq!(
            math_action(&mut error, || panic!("contained math boundary test")),
            STATUS_PANIC
        );
        assert_eq!(error.status, STATUS_PANIC);
        assert_eq!((*fixture.control).policy.usage().jobs, 0);
    }
}
