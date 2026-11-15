// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE file in the project root for license information.
// ---------------------------------------------------------------------------------------------
//! Real private job creation/account binding and ordinary-shape retirement.
//! The coverage callback is an unexpected-call detector, not a Native rasterizer.
//! No font replay, text coverage, recursive route, or DWrite/WIC pass is claimed.

use super::*;
use markdown_svg::Host;
use markdown_svg::coverage::{Batch, Callbacks as CoverageCallbacks, Output};
use markdown_svg_budget::Error as BudgetError;
use std::cell::Cell;
use std::sync::Arc;

use super::super::super::resources_bridge::engine_resources;

thread_local! {
    static NATIVE_CALLS: Cell<u32> = const { Cell::new(0) };
}

// No assertion, allocation, pointer dereference, unwrap, or panic crosses C.
unsafe extern "C" fn unexpected_native(_: u64, _: *const Batch, _: *mut Output) -> i32 {
    let _ = NATIVE_CALLS.try_with(|calls| calls.set(calls.get().saturating_add(1)));
    0x8000_4005u32 as i32
}

fn coverage_callbacks() -> CoverageCallbacks {
    CoverageCallbacks {
        size: 24,
        version: 1,
        session: 1,
        cover_batch: Some(unexpected_native),
        ..CoverageCallbacks::default()
    }
}

fn fonts() -> markdown_svg::abi::Callbacks {
    super::super::access_tests::callbacks()
}

fn create_with_coverage(f: &mut JobFixture, coverage: &CoverageCallbacks) -> (i32, SvgJobError) {
    let mut error: SvgJobError = pod();
    let status = unsafe {
        notepads_markdown_svg_job_create(
            1,
            f.resources,
            &fonts(),
            coverage,
            &f.id,
            &mut f.job,
            &mut f.cancel,
            &mut error,
        )
    };
    (status, error)
}

#[derive(Debug, PartialEq, Eq)]
struct OriginalHost {
    address: usize,
    account_nonce: u64,
    coverage_epoch: Option<u64>,
    renderer_phase: u32,
    tickets: u32,
    pending: u32,
    job_bytes: u64,
    root_bytes: u64,
    control_bytes: u64,
    first_fault: Option<BudgetError>,
}

fn original_host(f: &JobFixture) -> OriginalHost {
    // This existing test-only accessor borrows the same resources owner that
    // creation claims. Never retain either extra reference through retirement.
    let (host, fault) = unsafe { engine_resources(f.resources) };
    let (nonce, phase, tickets, pending, bytes, root, control) = host
        .with(|account, _| {
            let snapshot = account.snapshot();
            Ok((
                snapshot.account_nonce,
                account.renderer_phase,
                snapshot.tickets,
                snapshot.pending,
                snapshot.job_bytes,
                snapshot.root_work_bytes,
                snapshot.control_bytes,
            ))
        })
        .unwrap();
    let result = OriginalHost {
        address: Arc::as_ptr(&host) as usize,
        account_nonce: nonce,
        coverage_epoch: host.coverage_provider().map(|provider| provider.epoch()),
        renderer_phase: phase,
        tickets,
        pending,
        job_bytes: bytes,
        root_bytes: root,
        control_bytes: control,
        first_fault: fault.get(),
    };
    drop(fault);
    drop(host);
    result
}

fn assert_no_native_calls() {
    assert_eq!(NATIVE_CALLS.with(Cell::get), 0);
}

fn retire_and_reclaim(f: &mut JobFixture) {
    let retirement = f.finish();
    assert_eq!(retirement.identity, f.id);
    assert_eq!(retirement.cleanup_succeeded, 1);
    assert_eq!(retirement.job_error_kind, 0);
    assert_eq!(retirement.first_fault, SvgFault::default());
    assert_eq!(retirement.remaining_owners, 0);
    assert_eq!(retirement.failed_releases, 0);
    let destroyed = f.destroy();
    assert_eq!(destroyed.identity, f.id);
    assert_eq!((destroyed.consumed, destroyed.cleanup_succeeded), (1, 1));
    assert_eq!(destroyed.first_fault, SvgFault::default());
    assert_eq!(destroyed.unreleased_shell, SvgWorkToken::default());
    assert_eq!(destroyed.unreleased_engine, SvgWorkToken::default());
    f.reclaim();
    assert!(f.resources.is_null());
    assert!(!f.fixture.accounts[0].is_null());
    let account = f.fixture.read(0);
    assert_eq!(account.account_nonce, f.id.job_nonce);
    assert_eq!(
        (account.tickets, account.pending, account.closed),
        (0, 0, 0)
    );
    assert_no_native_calls();
}

#[test]
fn native_coverage_create_binds_original_host_and_shape_retires_without_native_work() {
    let _process = crate::core::images::serialize_process_image_tests();
    NATIVE_CALLS.with(|calls| calls.set(0));
    let mut f = JobFixture::new();
    let before = original_host(&f);
    assert_eq!(before.coverage_epoch, None);
    assert_eq!(before.renderer_phase, 0);
    assert_eq!(before.account_nonce, f.id.job_nonce);
    let mut coverage = coverage_callbacks();
    let (status, error) = create_with_coverage(&mut f, &coverage);
    assert_eq!(status, 0);
    assert_eq!(error.first_fault, SvgFault::default());
    assert!(!f.job.is_null() && !f.cancel.is_null());
    // The table is borrowed only during creation; changing caller storage must
    // not change the provider installed on the original paid account Host.
    coverage.session = 99;
    coverage.version = 99;
    coverage.cover_batch = None;
    let bound = original_host(&f);
    assert_eq!(bound.address, before.address);
    assert_eq!(bound.account_nonce, before.account_nonce);
    assert_eq!(bound.coverage_epoch, Some(1));
    assert_eq!(bound.renderer_phase, 2);
    assert_eq!(bound.first_fault, None);
    assert_eq!(coverage.session, 99);
    assert!(bound.job_bytes > before.job_bytes);
    assert!(bound.root_bytes > before.root_bytes);
    assert!(bound.control_bytes > before.control_bytes);
    assert_no_native_calls();

    let natural = f.prepare(RECT);
    assert_eq!((natural.width_dip, natural.height_dip), (100.5, 50.25));
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
            .any(|pixel| pixel == [0, 0, 255, 255])
    );
    assert!(
        pixels[plan.capacity as usize..]
            .iter()
            .all(|byte| *byte == 0xcc)
    );
    assert_no_native_calls();
    retire_and_reclaim(&mut f);
}

#[test]
fn create_preserves_the_original_host_and_binds_coverage() {
    let _process = crate::core::images::serialize_process_image_tests();
    NATIVE_CALLS.with(|calls| calls.set(0));
    let mut f = JobFixture::new();
    let before = original_host(&f);
    assert_eq!(f.create(), 0);
    let after = original_host(&f);
    assert_eq!(after.address, before.address);
    assert_eq!(after.account_nonce, before.account_nonce);
    assert_eq!(after.coverage_epoch, Some(1));
    assert_eq!(after.renderer_phase, 2);
    assert_eq!(after.first_fault, None);
    retire_and_reclaim(&mut f);
}

#[test]
fn invalid_coverage_table_never_claims_or_binds_and_valid_retry_succeeds() {
    let _process = crate::core::images::serialize_process_image_tests();
    NATIVE_CALLS.with(|calls| calls.set(0));
    let mut f = JobFixture::new();
    let before = original_host(&f);
    for case in 0..5 {
        let mut coverage = coverage_callbacks();
        match case {
            0 => coverage.size = 23,
            1 => coverage.version = 2,
            2 => coverage.session = 0,
            3 => coverage.session = 2,
            4 => coverage.cover_batch = None,
            _ => unreachable!(),
        }
        let (status, error) = create_with_coverage(&mut f, &coverage);
        assert_eq!(status, 1, "coverage table case {case}");
        assert_eq!(error.transport_status, 1);
        assert_eq!(error.job_error_kind, 2);
        assert_eq!(error.first_fault, SvgFault::default());
        assert!(f.job.is_null() && f.cancel.is_null());
        assert_eq!(original_host(&f), before, "coverage table case {case}");
        assert_no_native_calls();
    }
    #[cfg(target_pointer_width = "32")]
    {
        let mut coverage = coverage_callbacks();
        coverage.callback_padding = 1;
        assert_eq!(create_with_coverage(&mut f, &coverage).0, 1);
        assert!(f.job.is_null() && f.cancel.is_null());
        assert_eq!(original_host(&f), before);
    }
    assert_eq!(create_with_coverage(&mut f, &coverage_callbacks()).0, 0);
    assert_eq!(original_host(&f).coverage_epoch, Some(1));
    retire_and_reclaim(&mut f);
}

#[test]
fn coverage_pointer_preflight_rejects_aliases_before_claim_or_output_write() {
    let _process = crate::core::images::serialize_process_image_tests();
    NATIVE_CALLS.with(|calls| calls.set(0));
    let mut f = JobFixture::new();
    let before = original_host(&f);
    // Enough backing storage for every deliberately overlapped typed region.
    // The preflight must reject these addresses without reading those aliases.
    #[repr(C, align(8))]
    struct OverlapStorage {
        coverage: CoverageCallbacks,
        tail: [u64; 3],
    }
    let mut storage = OverlapStorage {
        coverage: coverage_callbacks(),
        tail: [0xabba_cafe; 3],
    };
    let fonts = fonts();
    for case in 0..10 {
        let coverage = (&raw mut storage.coverage).cast_const();
        let mut error: SvgJobError = pod();
        error.transport_status = 123;
        error.job_error_kind = 456;
        let status = unsafe {
            match case {
                0 => notepads_markdown_svg_job_create(
                    1,
                    f.resources,
                    &fonts,
                    ptr::null(),
                    &f.id,
                    &mut f.job,
                    &mut f.cancel,
                    &mut error,
                ),
                1 => notepads_markdown_svg_job_create(
                    1,
                    f.resources,
                    &fonts,
                    (coverage as usize + 1) as *const CoverageCallbacks,
                    &f.id,
                    &mut f.job,
                    &mut f.cancel,
                    &mut error,
                ),
                2 => notepads_markdown_svg_job_create(
                    1,
                    f.resources,
                    &fonts,
                    (&raw const fonts).cast(),
                    &f.id,
                    &mut f.job,
                    &mut f.cancel,
                    &mut error,
                ),
                3 => notepads_markdown_svg_job_create(
                    1,
                    f.resources,
                    &fonts,
                    coverage,
                    coverage.cast(),
                    &mut f.job,
                    &mut f.cancel,
                    &mut error,
                ),
                4 => notepads_markdown_svg_job_create(
                    1,
                    f.resources,
                    &fonts,
                    coverage,
                    &f.id,
                    coverage.cast_mut().cast(),
                    &mut f.cancel,
                    &mut error,
                ),
                5 => notepads_markdown_svg_job_create(
                    1,
                    f.resources,
                    &fonts,
                    coverage,
                    &f.id,
                    &mut f.job,
                    coverage.cast_mut().cast(),
                    &mut error,
                ),
                6 => notepads_markdown_svg_job_create(
                    1,
                    f.resources,
                    &fonts,
                    coverage,
                    &f.id,
                    &mut f.job,
                    &mut f.cancel,
                    coverage.cast_mut().cast(),
                ),
                7 => notepads_markdown_svg_job_create(
                    1,
                    f.resources,
                    &fonts,
                    f.resources.cast(),
                    &f.id,
                    &mut f.job,
                    &mut f.cancel,
                    &mut error,
                ),
                8 => notepads_markdown_svg_job_create(
                    2,
                    f.resources,
                    &fonts,
                    coverage,
                    &f.id,
                    &mut f.job,
                    &mut f.cancel,
                    &mut error,
                ),
                9 => notepads_markdown_svg_job_create(
                    1,
                    f.resources,
                    &fonts,
                    coverage,
                    &f.id,
                    &mut f.job,
                    (&raw mut f.job).cast(),
                    &mut error,
                ),
                _ => unreachable!(),
            }
        };
        assert_eq!(status, if case == 8 { 2 } else { 1 }, "alias case {case}");
        assert_eq!((error.transport_status, error.job_error_kind), (123, 456));
        assert!(f.job.is_null() && f.cancel.is_null(), "alias case {case}");
        assert_eq!(storage.coverage.size, 24);
        assert_eq!(storage.coverage.version, 1);
        assert_eq!(storage.coverage.session, 1);
        assert!(storage.coverage.cover_batch.is_some());
        assert_eq!(storage.tail, [0xabba_cafe; 3]);
        assert_eq!(original_host(&f), before, "alias case {case}");
        assert_no_native_calls();
    }
    assert_eq!(create_with_coverage(&mut f, &storage.coverage).0, 0);
    assert_eq!(original_host(&f).coverage_epoch, Some(1));
    retire_and_reclaim(&mut f);
}
