// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE file in the project root for license information.
// ---------------------------------------------------------------------------------------------
use super::*;

mod native_coverage_job_tests;

fn pod<T: Default>() -> T {
    let mut p = T::default();
    unsafe {
        (&mut p as *mut T)
            .cast::<SvgJobHeader>()
            .write(SvgJobHeader {
                abi_version: 1,
                struct_size: size_of::<T>() as u32,
            })
    };
    p
}
struct JobFixture {
    fixture: Fixture,
    resources: *mut SvgResources,
    job: *mut SvgRendererJob,
    cancel: *mut SvgJobCancel,
    id: SvgJobIdentity,
}
impl JobFixture {
    fn new() -> Self {
        let mut fixture = Fixture::new();
        fixture.create(0);
        let nonce = unsafe { (*fixture.accounts[0]).value.nonce };
        let mut resources = ptr::null_mut();
        assert_eq!(
            unsafe {
                notepads_markdown_svg_resources_create(1, &mut fixture.accounts[0], &mut resources, &mut error())
            },
            0
        );
        Self {
            fixture,
            resources,
            job: ptr::null_mut(),
            cancel: ptr::null_mut(),
            id: SvgJobIdentity {
                job_nonce: nonce,
                source_generation: 1,
                provider_generation: 1,
                font_generation: 1,
                target_generation: 1,
            },
        }
    }
    fn create(&mut self) -> i32 {
        unsafe {
            notepads_markdown_svg_job_create(
                1,
                self.resources,
                &super::access_tests::callbacks(),
                &super::super::shape_coverage(),
                &self.id,
                &mut self.job,
                &mut self.cancel,
                &mut pod(),
            )
        }
    }
    fn prepare(&self, bytes: &[u8]) -> SvgJobNatural {
        let mut out = pod();
        assert_eq!(
            unsafe {
                notepads_markdown_svg_job_prepare(
                    self.job,
                    bytes.as_ptr(),
                    bytes.len() as u64,
                    &mut out,
                    &mut pod(),
                )
            },
            0
        );
        out
    }
    fn target(&self, width: f64) -> SvgJobPlan {
        let target = SvgJobTarget {
            header: SvgJobHeader {
                abi_version: 1,
                struct_size: size_of::<SvgJobTarget>() as u32,
            },
            display_width_dip: width,
            effective_physical_scale: 1.0,
            generation: 1,
        };
        let mut out = pod();
        assert_eq!(
            unsafe { notepads_markdown_svg_job_set_target(self.job, &target, &mut out, &mut pod()) },
            0
        );
        out
    }
    fn finish(&self) -> SvgJobRetirement {
        let mut out = pod();
        assert_eq!(
            unsafe { notepads_markdown_svg_job_finish(self.job, &mut out, &mut pod()) },
            0
        );
        out
    }
    fn destroy(&mut self) -> SvgJobDestroyed {
        assert_eq!(unsafe { notepads_markdown_svg_job_close_cancel(self.job) }, 0);
        let mut out = pod();
        assert_eq!(
            unsafe { notepads_markdown_svg_job_destroy(&mut self.job, &mut out, &mut pod()) },
            0
        );
        assert!(self.job.is_null());
        out
    }
    fn reclaim(&mut self) {
        let mut r = SvgResourcesRetirement {
            header: SvgWorkHeader {
                abi_version: 1,
                struct_size: size_of::<SvgResourcesRetirement>() as u32,
            },
            ..Default::default()
        };
        assert_eq!(
            unsafe { notepads_markdown_svg_resources_reclaim(&mut self.resources, &mut r, &mut error()) },
            0
        );
        assert_eq!(r.cleanup_succeeded, 1);
        self.fixture.accounts[0] = r.account;
    }
}
impl Drop for JobFixture {
    fn drop(&mut self) {
        if !self.job.is_null() {
            let _ = self.finish();
            let _ = self.destroy();
        }
        if !self.resources.is_null() {
            self.reclaim();
        }
    }
}
const RECT:&[u8]=br##"<svg xmlns="http://www.w3.org/2000/svg" width="100.5" height="50.25"><rect width="100.5" height="50.25" fill="#ff0000"/></svg>"##;
#[test]
fn shell_commit_failure_releases_storage_and_rolls_back_renderer_claim() {
    let mut fixture = JobFixture::new();
    let (host, fault) = unsafe { resources_bridge::engine_resources(fixture.resources) };
    let before = host.with(|account, _| Ok(account.snapshot())).unwrap();
    renderer_job::inject(6);
    assert_eq!(fixture.create(), crate::STATUS_OUT_OF_MEMORY);
    assert!(fixture.job.is_null());
    assert!(fixture.cancel.is_null());
    let after = host
        .with_cleanup(true, |account, _| {
            assert_eq!(account.renderer_phase, 0);
            Ok(account.snapshot())
        })
        .unwrap();
    assert_eq!(after.tickets, before.tickets);
    assert_eq!(after.pending, before.pending);
    assert_eq!(after.job_bytes, before.job_bytes);
    assert_eq!(after.root_work_bytes, before.root_work_bytes);
    assert_eq!(after.control_bytes, before.control_bytes);
    drop(fault);
    drop(host);
}
#[cfg(feature = "quality-observation")]
mod quality_observation_tests {
    use super::*;

    #[test]
    fn quality_preflight_and_once_install_do_not_replace_the_real_owner() {
        let mut f = JobFixture::new();
        assert_eq!(f.create(), 0);
        assert_eq!(
            unsafe { notepads_markdown_svg_job_quality_enable(f.job, 0, &mut pod()) },
            1
        );
        assert_eq!(
            unsafe { notepads_markdown_svg_job_quality_enable(f.job, 8 * 1024 * 1024 + 1, &mut pod()) },
            1
        );
        assert_eq!(
            unsafe { notepads_markdown_svg_job_quality_enable(f.job, 4096, &mut pod()) },
            0
        );
        assert_eq!(
            unsafe { notepads_markdown_svg_job_quality_enable(f.job, 4096, &mut pod()) },
            5
        );
        let mut info: SvgQualityTraceInfo = pod();
        assert_eq!(
            unsafe { notepads_markdown_svg_job_quality_info(f.job, &mut info, &mut pod()) },
            0
        );
        assert_eq!(
            (
                info.bytes,
                info.capacity,
                info.events,
                info.flags,
                info.trace_error
            ),
            (0, 4096, 0, 1, 0)
        );
        let mut written = 77;
        // Overlapping job storage must be rejected before written is touched.
        assert_eq!(
            unsafe { notepads_markdown_svg_job_quality_copy(f.job, f.job.cast(), 1, &mut written, &mut pod()) },
            1
        );
        assert_eq!(written, 77);
        assert_eq!(
            unsafe { notepads_markdown_svg_job_quality_info(f.job, f.job.cast(), &mut pod()) },
            1
        );
        assert_eq!(f.finish().cleanup_succeeded, 1);
        assert_eq!(f.destroy().cleanup_succeeded, 1);
    }

    #[test]
    fn quality_snapshot_wrong_thread_and_after_finish_are_rejected() {
        let mut f = JobFixture::new();
        assert_eq!(f.create(), 0);
        assert_eq!(
            unsafe { notepads_markdown_svg_job_quality_enable(f.job, 4096, &mut pod()) },
            0
        );
        let address = f.job as usize;
        std::thread::scope(|scope| {
            scope
                .spawn(move || {
                    let job = address as *mut SvgRendererJob;
                    let mut info: SvgQualityTraceInfo = pod();
                    assert_eq!(
                        unsafe { notepads_markdown_svg_job_quality_info(job, &mut info, &mut pod()) },
                        5
                    );
                    let mut bytes = [0xAA; 16];
                    let mut written = 77;
                    assert_eq!(
                        unsafe {
                            notepads_markdown_svg_job_quality_copy(
                                job,
                                bytes.as_mut_ptr(),
                                16,
                                &mut written,
                                &mut pod(),
                            )
                        },
                        5
                    );
                    assert_eq!(written, 0);
                    assert_eq!(bytes, [0xAA; 16]);
                })
                .join()
                .unwrap();
        });
        assert_eq!(f.finish().cleanup_succeeded, 1);
        let mut info: SvgQualityTraceInfo = pod();
        assert_eq!(
            unsafe { notepads_markdown_svg_job_quality_info(f.job, &mut info, &mut pod()) },
            5
        );
        let mut bytes = [0xBB; 16];
        let mut written = 77;
        assert_eq!(
            unsafe {
                notepads_markdown_svg_job_quality_copy(f.job, bytes.as_mut_ptr(), 16, &mut written, &mut pod())
            },
            5
        );
        assert_eq!((written, bytes), (0, [0xBB; 16]));
        assert_eq!(f.destroy().cleanup_succeeded, 1);
    }

    #[test]
    fn quality_failed_job_keeps_readable_prefix_and_still_physically_retires() {
        let mut f = JobFixture::new();
        assert_eq!(f.create(), 0);
        assert_eq!(
            unsafe { notepads_markdown_svg_job_quality_enable(f.job, 4096, &mut pod()) },
            0
        );
        let bad = b"<svg";
        let mut natural: SvgJobNatural = pod();
        let mut original: SvgJobError = pod();
        assert_ne!(
            unsafe {
                notepads_markdown_svg_job_prepare(
                    f.job,
                    bad.as_ptr(),
                    bad.len() as u64,
                    &mut natural,
                    &mut original,
                )
            },
            0
        );
        let fault = original.first_fault;
        let mut info: SvgQualityTraceInfo = pod();
        assert_eq!(
            unsafe { notepads_markdown_svg_job_quality_info(f.job, &mut info, &mut pod()) },
            0
        );
        assert_eq!((info.bytes, info.events, info.flags), (0, 0, 1));
        let mut written = 77;
        assert_eq!(
            unsafe { notepads_markdown_svg_job_quality_copy(f.job, ptr::null_mut(), 0, &mut written, &mut pod()) },
            0
        );
        assert_eq!(written, 0);
        let retired = f.finish();
        assert_eq!(retired.first_fault, fault);
        assert_eq!(
            (
                retired.cleanup_succeeded,
                retired.remaining_owners,
                retired.failed_releases
            ),
            (1, 0, 0)
        );
        assert_eq!(f.destroy().cleanup_succeeded, 1);
    }
}
#[test]
fn real_job_abi_prepare_poison_input_and_single_scale_render_then_retire() {
    let _process = crate::core::images::serialize_process_image_tests();
    let mut f = JobFixture::new();
    assert_eq!(f.create(), 0);
    let mut source = RECT.to_vec();
    let natural = f.prepare(&source);
    assert_eq!((natural.width_dip, natural.height_dip), (100.5, 50.25));
    source.fill(0xff);
    drop(source);
    let plan = f.target(201.0);
    assert_eq!(plan.identity, f.id);
    let mut pixels = vec![0xcc; plan.capacity as usize + 16];
    assert_eq!(
        unsafe {
            notepads_markdown_svg_job_run(
                f.job,
                &f.id,
                pixels.as_mut_ptr(),
                plan.capacity,
                plan.stride,
                &mut pod(),
            )
        },
        0
    );
    assert!(
        pixels[..plan.capacity as usize]
            .chunks_exact(4)
            .any(|p| p == [0, 0, 255, 255])
    );
    assert!(pixels[plan.capacity as usize..].iter().all(|b| *b == 0xcc));
    let r = f.finish();
    assert_eq!(
        (r.terminal_state, r.cleanup_succeeded, r.job_error_kind),
        (5, 1, 0)
    );
    assert_eq!(r.first_fault, SvgFault::default());
    let d = f.destroy();
    assert_eq!((d.consumed, d.cleanup_succeeded, d.cancelled), (1, 1, 0));
    assert_eq!(d.unreleased_engine, SvgWorkToken::default());
    assert_eq!(d.unreleased_shell, SvgWorkToken::default());
}
#[test]
fn original_account_once_survives_resources_recreate() {
    let _process = crate::core::images::serialize_process_image_tests();
    let mut f = JobFixture::new();
    assert_eq!(f.create(), 0);
    f.prepare(RECT);
    assert_eq!(f.finish().cleanup_succeeded, 1);
    f.destroy();
    f.reclaim();
    assert_eq!(
        unsafe {
            notepads_markdown_svg_resources_create(
                1,
                &mut f.fixture.accounts[0],
                &mut f.resources,
                &mut error(),
            )
        },
        0
    );
    let mut error: SvgJobError = pod();
    assert_eq!(
        unsafe {
            notepads_markdown_svg_job_create(
                1,
                f.resources,
                &super::access_tests::callbacks(),
                &super::super::shape_coverage(),
                &f.id,
                &mut f.job,
                &mut f.cancel,
                &mut error,
            )
        },
        5
    );
    assert!(f.job.is_null());
}
#[test]
fn mismatched_epoch_and_nonce_reject_without_consuming_the_account_claim() {
    let _process = crate::core::images::serialize_process_image_tests();
    let mut f = JobFixture::new();
    let original = f.id;
    f.id.job_nonce += 1;
    assert_eq!(f.create(), 1);
    assert!(f.job.is_null());
    f.id = original;
    f.id.source_generation += 1;
    assert_eq!(f.create(), 1);
    f.id = original;
    assert_eq!(f.create(), 0);
    assert_eq!(f.finish().cleanup_succeeded, 1);
    f.destroy();
}
struct CancelLease(*mut SvgJobCancel);
// SAFETY: test retains while canonical job is pinned; joins before destruction.
unsafe impl Send for CancelLease {}
impl CancelLease {
    fn request_release(self) {
        assert_eq!(unsafe { notepads_markdown_svg_job_cancel_request(self.0) }, 0);
        assert_eq!(unsafe { notepads_markdown_svg_job_cancel_release(self.0) }, 0);
    }
}
#[test]
fn retained_late_cancel_blocks_destroy_until_lease_retires() {
    let _process = crate::core::images::serialize_process_image_tests();
    let mut f = JobFixture::new();
    assert_eq!(f.create(), 0);
    f.prepare(RECT);
    let p = f.target(100.5);
    let mut bytes = vec![0; p.capacity as usize];
    assert_eq!(
        unsafe {
            notepads_markdown_svg_job_run(
                f.job,
                &f.id,
                bytes.as_mut_ptr(),
                p.capacity,
                p.stride,
                &mut pod(),
            )
        },
        0
    );
    assert_eq!(unsafe { notepads_markdown_svg_job_cancel_retain(f.cancel) }, 0);
    let lease = CancelLease(f.cancel);
    let (go_tx, go_rx) = std::sync::mpsc::sync_channel(0);
    let thread = std::thread::spawn(move || {
        go_rx.recv().unwrap();
        lease.request_release();
    });
    assert_eq!(f.finish().cleanup_succeeded, 1);
    assert_eq!(unsafe { notepads_markdown_svg_job_close_cancel(f.job) }, 0);
    assert_eq!(unsafe { notepads_markdown_svg_job_cancel_retain(f.cancel) }, 5);
    let original = f.job;
    let mut out: SvgJobDestroyed = pod();
    assert_eq!(
        unsafe { notepads_markdown_svg_job_destroy(&mut f.job, &mut out, &mut pod()) },
        5
    );
    assert_eq!(f.job, original);
    assert_eq!(out.consumed, 0);
    go_tx.send(()).unwrap();
    thread.join().unwrap();
    let result = f.destroy();
    assert_eq!(result.cancelled, 1);
}
#[test]
fn original_core_cancellation_is_observed_by_the_render_job() {
    let _process = crate::core::images::serialize_process_image_tests();
    let mut f = JobFixture::new();
    assert_eq!(f.create(), 0);
    assert_eq!(unsafe { notepads_markdown_svg_job_cancel_retain(f.cancel) }, 0);
    assert_eq!(unsafe { notepads_markdown_svg_job_cancel_request(f.cancel) }, 0);
    assert_eq!(unsafe { notepads_markdown_svg_job_cancel_release(f.cancel) }, 0);
    let mut out: SvgJobNatural = pod();
    assert_eq!(
        unsafe {
            notepads_markdown_svg_job_prepare(
                f.job,
                RECT.as_ptr(),
                RECT.len() as u64,
                &mut out,
                &mut pod(),
            )
        },
        4
    );
    let r = f.finish();
    assert_eq!(
        (r.cleanup_succeeded, r.cancelled, r.first_fault.kind),
        (1, 1, 4)
    );
    assert_eq!(f.destroy().cleanup_succeeded, 1);
}
#[test]
fn versions_aliases_and_state_rejection_preserve_success_output() {
    let _process = crate::core::images::serialize_process_image_tests();
    let mut f = JobFixture::new();
    let mut error: SvgJobError = pod();
    error.header.abi_version = 99;
    assert_eq!(
        unsafe {
            notepads_markdown_svg_job_create(
                1,
                f.resources,
                &super::access_tests::callbacks(),
                &super::super::shape_coverage(),
                &f.id,
                &mut f.job,
                &mut f.cancel,
                &mut error,
            )
        },
        2
    );
    assert!(f.job.is_null());
    assert_eq!(f.create(), 0);
    let mut plan: SvgJobPlan = pod();
    plan.width = 0x1234;
    let t: SvgJobTarget = pod();
    assert_eq!(
        unsafe { notepads_markdown_svg_job_set_target(f.job, &t, &mut plan, &mut pod()) },
        5
    );
    assert_eq!(plan.width, 0x1234);
    assert_eq!(
        unsafe {
            notepads_markdown_svg_job_prepare(
                f.job,
                RECT.as_ptr(),
                RECT.len() as u64,
                f.job.cast(),
                &mut pod(),
            )
        },
        1
    );
    assert_eq!(f.finish().cleanup_succeeded, 1);
    f.destroy();
}

#[test]
fn semantic_initialization_failure_publishes_owned_shell_and_cleans_on_worker() {
    let _process = crate::core::images::serialize_process_image_tests();
    let mut f = JobFixture::new();
    super::super::renderer_job::inject(1);
    assert_eq!(f.create(), 7);
    assert!(!f.job.is_null() && !f.cancel.is_null());
    let r = f.finish();
    assert_eq!(
        (r.cleanup_succeeded, r.job_error_kind, r.first_fault.kind),
        (1, 4, 3)
    );
    assert_eq!(f.destroy().cleanup_succeeded, 1);
}
#[test]
fn initialization_admit_failure_keeps_formed_engine_for_mandatory_finish() {
    let _process = crate::core::images::serialize_process_image_tests();
    let mut f = JobFixture::new();
    super::super::renderer_job::inject(2);
    assert_ne!(f.create(), 0);
    assert!(!f.job.is_null());
    let r = f.finish();
    assert_eq!(
        (
            r.cleanup_succeeded,
            r.job_error_kind,
            r.first_fault.native_hr
        ),
        (1, 4, -778)
    );
    let d = f.destroy();
    assert_eq!(d.cleanup_succeeded, 1);
    assert_eq!(d.unreleased_engine, SvgWorkToken::default());
}
#[test]
fn destroyed_shell_panic_has_consumed_receipt_and_keeps_original_pins() {
    let _process = crate::core::images::serialize_process_image_tests();
    let mut f = JobFixture::new();
    assert_eq!(f.create(), 0);
    assert_eq!(f.finish().cleanup_succeeded, 1);
    assert_eq!(unsafe { notepads_markdown_svg_job_close_cancel(f.job) }, 0);
    super::super::renderer_job::inject(3);
    let mut out: SvgJobDestroyed = pod();
    assert_eq!(
        unsafe { notepads_markdown_svg_job_destroy(&mut f.job, &mut out, &mut pod()) },
        STATUS_PANIC
    );
    assert!(f.job.is_null());
    assert_eq!(
        (out.consumed, out.cleanup_succeeded, out.first_fault.kind),
        (1, 0, 7)
    );
    assert_ne!(out.unreleased_shell, SvgWorkToken::default());
    assert_ne!(out.unreleased_engine, SvgWorkToken::default());
    // Permanent quarantine: preserve the canonical resources/request/model pins.
    std::mem::forget(f);
}
#[test]
fn cancel_numerical_pointer_overflow_is_rejected_before_atomic_access() {
    let _process = crate::core::images::serialize_process_image_tests();
    let p = (usize::MAX & !7usize) as *mut SvgJobCancel;
    assert_eq!(unsafe { notepads_markdown_svg_job_cancel_retain(p) }, 1);
    assert_eq!(unsafe { notepads_markdown_svg_job_cancel_request(p) }, 1);
    assert_eq!(unsafe { notepads_markdown_svg_job_cancel_release(p) }, 1);
}

#[test]
fn constructor_host_error_and_unwind_keep_a_finishable_owned_shell() {
    let _process = crate::core::images::serialize_process_image_tests();
    for point in [1, 2] {
        let mut f = JobFixture::new();
        super::super::access::AccountAccess::inject_engine_reserve(point);
        assert_ne!(f.create(), 0);
        assert!(!f.job.is_null() && !f.cancel.is_null());
        let r = f.finish();
        assert_eq!(r.cleanup_succeeded, 1, "constructor reserve {point}");
        assert_eq!(r.remaining_owners, 0);
        assert_eq!(r.failed_releases, 0);
        if point == 1 {
            assert_eq!((r.first_fault.kind, r.first_fault.native_hr), (1, -779));
        } else {
            assert_eq!((r.first_fault.kind, r.job_error_kind), (7, 5));
        }
        let d = f.destroy();
        assert_eq!(d.cleanup_succeeded, 1);
        assert_eq!(d.first_fault, r.first_fault);
        assert_eq!(d.unreleased_engine, SvgWorkToken::default());
        assert_eq!(d.unreleased_shell, SvgWorkToken::default());
    }
}

#[test]
fn each_destroy_release_failure_preserves_exact_unreleased_fees_and_receipt() {
    let _process = crate::core::images::serialize_process_image_tests();
    for (point, prior_cancel) in [(4, false), (5, false), (4, true), (5, true)] {
        let mut f = JobFixture::new();
        assert_eq!(f.create(), 0);
        if prior_cancel {
            assert_eq!(unsafe { notepads_markdown_svg_job_cancel_retain(f.cancel) }, 0);
            assert_eq!(unsafe { notepads_markdown_svg_job_cancel_request(f.cancel) }, 0);
            assert_eq!(unsafe { notepads_markdown_svg_job_cancel_release(f.cancel) }, 0);
            assert_eq!(
                unsafe {
                    notepads_markdown_svg_job_prepare(
                        f.job,
                        RECT.as_ptr(),
                        RECT.len() as u64,
                        &mut pod(),
                        &mut pod(),
                    )
                },
                4
            );
        }
        let retired = f.finish();
        assert_eq!(retired.cleanup_succeeded, 1);
        assert_eq!(unsafe { notepads_markdown_svg_job_close_cancel(f.job) }, 0);
        let policy = unsafe { &(&(*f.fixture.policy).value.owner).policy };
        let root_before = f.fixture.budget.usage().auxiliary_capacity;
        let control_before = policy.svg_work_bytes();
        let engine_bytes = {
            let metadata = markdown_svg::bootstrap_metadata();
            (metadata.total_bytes().unwrap()
                - metadata.shell_bytes
                - metadata.first_failure_arc_bytes) as u64
        };
        super::super::renderer_job::inject(point);
        let mut out: SvgJobDestroyed = pod();
        assert_ne!(
            unsafe { notepads_markdown_svg_job_destroy(&mut f.job, &mut out, &mut pod()) },
            0
        );
        assert!(f.job.is_null());
        assert_eq!((out.consumed, out.cleanup_succeeded), (1, 0));
        assert_eq!(out.identity, f.id);
        assert_eq!(out.first_fault.kind, if prior_cancel { 4 } else { 7 });
        if prior_cancel {
            assert_eq!(out.first_fault, retired.first_fault);
        }
        assert_ne!(out.unreleased_shell, SvgWorkToken::default());
        let released = if point == 4 {
            assert_ne!(out.unreleased_engine, SvgWorkToken::default());
            0
        } else {
            assert_eq!(out.unreleased_engine, SvgWorkToken::default());
            engine_bytes
        };
        assert_eq!(
            f.fixture.budget.usage().auxiliary_capacity,
            root_before - released
        );
        assert_eq!(policy.svg_work_bytes(), control_before - released);
        let mut fault = SvgFault::default();
        assert_eq!(
            unsafe { notepads_markdown_svg_resources_read_fault(f.resources, &mut fault, &mut error()) },
            0
        );
        assert_eq!(fault, out.first_fault);
        // Physical shell/engine storage is gone. Preserve the canonical original
        // task/resources/request and all unreleased funding; never retry free.
        std::mem::forget(f);
    }
}
