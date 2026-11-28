// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE file in the project root for license information.
// ---------------------------------------------------------------------------------------------

//! Real source/control/process funding, independent of render/profile readiness.
use super::*;
use markdown_core::{config::Extensions, source::SourceDocument};

#[test]
fn null_release_obeys_the_error_buffer_protocol() {
    let mut diagnostic = WireError {
        status: crate::STATUS_INVALID_ARGUMENT,
        ..WireError::default()
    };
    // SAFETY: a null handle consumes no owner; diagnostic has the full ABI layout.
    unsafe {
        assert_eq!(
            notepads_markdown_diagram_coordinator_funding_release(ptr::null_mut(), &mut diagnostic),
            crate::STATUS_OK
        );
        assert_eq!(diagnostic, WireError::default());
        diagnostic.status = crate::STATUS_INVALID_ARGUMENT;
        assert_eq!(
            notepads_markdown_diagram_job_release(ptr::null_mut(), &mut diagnostic),
            crate::STATUS_OK
        );
        assert_eq!(diagnostic, WireError::default());
        diagnostic.header.struct_size = 8;
        let before = diagnostic;
        assert_eq!(
            notepads_markdown_diagram_job_release(ptr::null_mut(), &mut diagnostic),
            crate::STATUS_ABI_MISMATCH
        );
        assert_eq!(diagnostic, before);
    }
}

struct Fixture {
    source: SourceBudget,
    model: *mut Owned<CoreModel>,
    runtime: *mut MathRuntime,
    control: *mut MathControl,
}
impl Fixture {
    fn new() -> Self {
        let source = SourceBudget::preview();
        let mut op = source.begin(CancellationToken::new());
        let document = SourceDocument::new(
            SourceKey::new([216; 16], 1).unwrap(),
            b"$x$\n\n```mermaid\nflowchart TD\nA-->B\n```",
            &mut op,
        )
        .unwrap();
        let model = AcceptedModel::parse(
            document.snapshot(),
            ParseConfig::with_extensions(Profile::G3, Extensions::ALL),
            1,
            &mut op,
        )
        .unwrap();
        let model = Slot::new(op.boundary_work())
            .unwrap()
            .commit(CoreModel { model });
        let mut runtime = ptr::null_mut();
        let mut control = ptr::null_mut();
        let mut error = WireError::default();
        // SAFETY: fixture owns independent writable outputs and all returned refs.
        unsafe {
            assert_eq!(
                notepads_markdown_math_runtime_create(&mut runtime, &mut error),
                0
            );
            assert_eq!(
                notepads_markdown_math_control_create(runtime, &mut control, &mut error),
                0
            );
        }
        Self {
            source,
            model,
            runtime,
            control,
        }
    }
    fn diagram(&self) -> *mut DiagramJob {
        let mut job = ptr::null_mut();
        let mut error = WireError::default();
        // SAFETY: shared immutable fixture inputs; independent output.
        unsafe {
            assert_eq!(
                notepads_markdown_diagram_job_prepare(
                    self.control,
                    self.model,
                    2,
                    &mut job,
                    &mut error
                ),
                0
            );
        }
        job
    }
    fn math(&self) -> *mut MathJob {
        let mut job = ptr::null_mut();
        let mut error = WireError::default();
        // SAFETY: shared immutable fixture inputs; independent output.
        unsafe {
            assert_eq!(
                notepads_markdown_math_job_prepare(
                    self.control,
                    self.model,
                    1,
                    &mut job,
                    &mut error
                ),
                0
            );
        }
        job
    }
    fn usage(&self) -> MathUsage {
        let mut usage = MathUsage {
            header: WireHeader {
                abi_version: 2,
                struct_size: size_of::<MathUsage>() as u32,
            },
            ..Default::default()
        };
        let mut error = WireError::default();
        // SAFETY: immutable live control, fully writable independent ABI2 output.
        unsafe {
            assert_eq!(
                notepads_markdown_math_control_usage(self.control, &mut usage, &mut error),
                0
            );
        }
        usage
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        let mut error = WireError::default();
        // SAFETY: tests retire/release all jobs before fixture destruction.
        unsafe {
            assert_eq!(notepads_markdown_core_model_destroy(self.model), 0);
            assert_eq!(
                notepads_markdown_math_control_release(self.control, &mut error),
                0
            );
            assert_eq!(
                notepads_markdown_math_runtime_release(self.runtime, &mut error),
                0
            );
        }
        assert_eq!(self.source.usage().source_owners, 0);
    }
}

#[test]
fn math_and_diagram_share_one_actual_control_worker_slot_and_retirement_preserves_resident() {
    let fixture = Fixture::new();
    let math = fixture.math();
    let diagram = fixture.diagram();
    let mut error = WireError::default();
    // SAFETY: no native thread is created in this funding mechanism test.
    unsafe {
        assert_eq!(notepads_markdown_math_job_activate(math, &mut error), 0);
        assert_eq!(
            notepads_markdown_diagram_job_activate(diagram, &mut error),
            9
        );
        assert_eq!(error.limit_kind, 104);
        assert_eq!(fixture.usage().control.jobs, 1);
        assert_eq!(fixture.usage().control.residents, 2);
        assert_eq!(notepads_markdown_math_job_release(math, &mut error), 0);
        assert_eq!(
            notepads_markdown_diagram_job_activate(diagram, &mut error),
            0
        );
        assert_eq!(fixture.usage().control.jobs, 1);
        assert_eq!(notepads_markdown_diagram_job_retain(diagram, &mut error), 0);
        assert_eq!(
            notepads_markdown_diagram_job_retire_metadata(diagram, &mut error),
            102
        );
        assert_eq!(fixture.usage().control.jobs, 1);
        assert_eq!(
            notepads_markdown_diagram_job_release(diagram, &mut error),
            0
        );
        assert_eq!(notepads_markdown_diagram_job_cancel(diagram, &mut error), 0);
        assert_eq!(
            notepads_markdown_diagram_job_retire_metadata(diagram, &mut error),
            0
        );
        assert_eq!(notepads_markdown_diagram_job_state(diagram), METADATA_ONLY);
        assert_eq!(fixture.usage().control.jobs, 0);
        assert_eq!(fixture.usage().control.residents, 1);
        assert_eq!(
            notepads_markdown_diagram_job_release(diagram, &mut error),
            0
        );
        assert_eq!(fixture.usage().control.residents, 0);
    }
}

#[test]
fn mixed_family_residents_have_one_shared_boundary_and_refusal_does_not_allocate_a_context() {
    let fixture = Fixture::new();
    let mut math = Vec::new();
    for _ in 0..127 {
        math.push(fixture.math());
    }
    let diagram = fixture.diagram();
    let before = fixture.usage();
    assert_eq!(before.control.residents, 128);
    let mut refused = std::ptr::dangling_mut::<DiagramJob>();
    let mut error = WireError::default();
    // SAFETY: only initialized independent outputs and owned fixture handles.
    unsafe {
        assert_eq!(
            notepads_markdown_diagram_job_prepare(
                fixture.control,
                fixture.model,
                2,
                &mut refused,
                &mut error
            ),
            9
        );
        assert_eq!(error.limit_kind, 108);
        assert!(refused.is_null());
        assert_eq!(fixture.usage(), before);
        assert_eq!(
            notepads_markdown_diagram_job_release(diagram, &mut error),
            0
        );
        for job in math {
            assert_eq!(notepads_markdown_math_job_release(job, &mut error), 0);
        }
    }
    assert_eq!(fixture.usage().control.residents, 0);
}

#[test]
fn wrong_object_and_duplicate_start_are_rejected_and_queued_cancel_is_terminal() {
    let fixture = Fixture::new();
    let before = fixture.usage();
    let mut output = ptr::null_mut();
    let mut error = WireError::default();
    // SAFETY: no thread exists; fixture owns all contexts exclusively.
    unsafe {
        assert_eq!(
            notepads_markdown_diagram_job_prepare(
                fixture.control,
                fixture.model,
                1,
                &mut output,
                &mut error
            ),
            8
        );
        assert!(output.is_null());
        assert_eq!(fixture.usage(), before);
        let job = fixture.diagram();
        assert_eq!(
            notepads_markdown_diagram_job_claim_start(job, &mut error),
            0
        );
        assert_eq!(
            notepads_markdown_diagram_job_claim_start(job, &mut error),
            102
        );
        assert_eq!(notepads_markdown_diagram_job_cancel(job, &mut error), 0);
        assert_eq!(notepads_markdown_diagram_job_activate(job, &mut error), 4);
        assert_eq!(notepads_markdown_diagram_job_state(job), QUEUED);
        assert_eq!(
            notepads_markdown_diagram_job_retire_metadata(job, &mut error),
            0
        );
        assert_eq!(notepads_markdown_diagram_job_release(job, &mut error), 0);
    }
    assert_eq!(fixture.usage(), before);
}

#[test]
fn independent_native_coordinator_header_is_prepaid_without_consuming_a_resident_or_worker() {
    let fixture = Fixture::new();
    let before = fixture.usage();
    let mut header = ptr::null_mut();
    let mut error = WireError::default();
    unsafe {
        assert_eq!(
            notepads_markdown_diagram_coordinator_funding_prepare(
                fixture.control,
                fixture.model,
                &mut header,
                &mut error
            ),
            0
        );
        let active = fixture.usage();
        assert_eq!(active.control.jobs, before.control.jobs);
        assert_eq!(active.control.residents, before.control.residents);
        assert_eq!(
            active.control.metadata_bytes - before.control.metadata_bytes,
            NATIVE_COORDINATOR_BYTES + size_of::<DiagramCoordinatorFunding>() as u64
        );
        assert_eq!(
            notepads_markdown_diagram_coordinator_funding_release(header, &mut error),
            0
        );
    }
    assert_eq!(fixture.usage(), before);
}

#[test]
fn unknown_or_zero_actual_stack_headroom_cannot_execute_or_publish_a_diagram() {
    let fixture = Fixture::new();
    let job = fixture.diagram();
    let mut error = WireError::default();
    let fonts = DiagramFontCapability {
        version: 1,
        size: size_of::<DiagramFontCapability>() as u32,
        factory: ptr::null_mut(),
        collection: ptr::null_mut(),
        fallback: ptr::null_mut(),
        font_epoch: 1,
        locale_name: [0; 85],
    };
    let style = DiagramStyle {
        header: diagram_header::<DiagramStyle>(),
        ..Default::default()
    };
    // Mechanism only: there is no native thread or live COM capability. The
    // fixed stack gate must reject before QI or any text/layout computation.
    unsafe {
        assert_eq!(notepads_markdown_diagram_job_activate(job, &mut error), 0);
        assert_eq!(
            notepads_markdown_diagram_job_claim_start(job, &mut error),
            0
        );
        assert_eq!(
            notepads_markdown_diagram_job_run(job, &fonts, &style, 0, &mut error),
            100
        );
        assert_eq!(notepads_markdown_diagram_job_state(job), COMPUTED);
        assert_eq!(fixture.usage().control.jobs, 1);
        let mut summary = DiagramSummary {
            header: diagram_header::<DiagramSummary>(),
            ..Default::default()
        };
        assert_eq!(
            notepads_markdown_diagram_job_summary(job, &mut summary, &mut error),
            100
        );
        assert_eq!(
            notepads_markdown_diagram_job_retire_metadata(job, &mut error),
            0
        );
        assert_eq!(fixture.usage().control.jobs, 0);
        assert_eq!(notepads_markdown_diagram_job_release(job, &mut error), 0);
    }
}

#[test]
fn a_late_scope_even_without_live_payload_prevents_metadata_retirement_and_final_free() {
    let fixture = Fixture::new();
    let job = fixture.diagram();
    let mut error = WireError::default();
    unsafe {
        assert_eq!(notepads_markdown_diagram_job_activate(job, &mut error), 0);
        let scope = (&*(*ptr::addr_of!((*job).data)).get())
            .memory
            .as_ref()
            .unwrap()
            .scope();
        assert_eq!(
            notepads_markdown_diagram_job_retire_metadata(job, &mut error),
            102
        );
        assert_eq!(notepads_markdown_diagram_job_release(job, &mut error), 102);
        assert_eq!(fixture.usage().control.jobs, 1);
        assert_eq!(fixture.usage().control.residents, 1);
        drop(scope);
        assert_eq!(
            notepads_markdown_diagram_job_retire_metadata(job, &mut error),
            0
        );
        assert_eq!(notepads_markdown_diagram_job_release(job, &mut error), 0);
    }
}

#[test]
fn run_publishes_funded_failure_or_original_terminal_reason_and_summary_replays_it() {
    for scenario in 0..3 {
        let fixture = Fixture::new();
        let job = fixture.diagram();
        let mut error = WireError::default();
        let fonts = DiagramFontCapability {
            version: 1,
            size: size_of::<DiagramFontCapability>() as u32,
            factory: ptr::null_mut(),
            collection: ptr::null_mut(),
            fallback: ptr::null_mut(),
            font_epoch: 1,
            locale_name: [0; 85],
        };
        let style = DiagramStyle {
            header: diagram_header::<DiagramStyle>(),
            ..Default::default()
        };
        // SAFETY: this is a real ABI-owned, exclusive queued test fixture;
        // no native thread or cancel/ref callback can overlap the replacement.
        unsafe {
            if scenario != 0 {
                (*job).control = OperationControl::new();
            }
            assert_eq!(notepads_markdown_diagram_job_activate(job, &mut error), 0);
            assert_eq!(
                notepads_markdown_diagram_job_claim_start(job, &mut error),
                0
            );
            let (expected_status, expected_first) = match scenario {
                0 => {
                    let data = &*(*ptr::addr_of!((*job).data)).get();
                    let scope = data.memory.as_ref().unwrap().scope();
                    scope.charge_work(1_000_000).unwrap();
                    assert_eq!(scope.charge_work(1), Err(MemoryError::WorkLimited));
                    (9, DiagramError::Memory(MemoryError::WorkLimited))
                }
                1 => {
                    assert!((*job).control.set_deadline(Duration::ZERO));
                    (24, DiagramError::Deadline)
                }
                _ => {
                    (*job).control.cancel();
                    assert_eq!(
                        (*job).control.checkpoint().unwrap_err().reason,
                        CancelReason::Requested
                    );
                    assert!((*job).control.set_deadline(Duration::ZERO));
                    (4, DiagramError::Cancelled)
                }
            };
            // Initial real control checkpoint must reject before COM/stack use.
            // For WorkLimited its cancellation projection is Requested, so this
            // assertion fails as 4 if job_run's resolve_error map is removed.
            assert_eq!(
                notepads_markdown_diagram_job_run(job, &fonts, &style, 0, &mut error),
                expected_status
            );
            assert_eq!(error.status, expected_status);
            assert_eq!(notepads_markdown_diagram_job_state(job), COMPUTED);
            let data = &*(*ptr::addr_of!((*job).data)).get();
            assert_eq!(data.first, Some(expected_first));
            assert_eq!(fixture.usage().control.jobs, 1);
            let mut summary = DiagramSummary {
                header: diagram_header::<DiagramSummary>(),
                ..Default::default()
            };
            assert_eq!(
                notepads_markdown_diagram_job_summary(job, &mut summary, &mut error),
                expected_status
            );
            assert_eq!(error.status, expected_status);
            assert_eq!(
                notepads_markdown_diagram_job_retire_metadata(job, &mut error),
                0
            );
            assert_eq!(fixture.usage().control.jobs, 0);
            assert_eq!(notepads_markdown_diagram_job_release(job, &mut error), 0);
        }
    }
}
