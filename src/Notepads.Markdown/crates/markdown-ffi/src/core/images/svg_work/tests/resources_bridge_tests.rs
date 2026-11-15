// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE file in the project root for license information.
// ---------------------------------------------------------------------------------------------
use super::super::resources_bridge::engine_resources;
use super::*;
use markdown_svg::Host;

unsafe fn resources(f: &mut Fixture) -> *mut SvgResources {
    f.create(0);
    let mut p = ptr::null_mut();
    assert_eq!(
        unsafe { notepads_markdown_svg_resources_create(1, &mut f.accounts[0], &mut p, &mut error()) },
        0
    );
    assert!(f.accounts[0].is_null());
    p
}
fn receipt() -> SvgResourcesRetirement {
    SvgResourcesRetirement {
        header: SvgWorkHeader {
            abi_version: 1,
            struct_size: size_of::<SvgResourcesRetirement>() as u32,
        },
        ..Default::default()
    }
}

#[test]
fn svg_structural_faults_round_trip_on_actual_resources_and_keep_first_on_retirement() {
    let _process = crate::core::images::serialize_process_image_tests();
    for limit in [14, 15] {
        let mut fixture = Fixture::new();
        // SAFETY: fixture owns valid independent account/output PODs and the
        // resource is reclaimed exactly once below before fixture destruction.
        unsafe {
            let mut handle = resources(&mut fixture);
            let unknown = SvgFault {
                kind: 2,
                limit: 16,
                ..Default::default()
            };
            assert_eq!(
                notepads_markdown_svg_resources_record_fault(handle, &unknown, &mut error()),
                wire_error(CoreError::InvalidRange).status
            );
            let mut observed = SvgFault::default();
            assert_eq!(
                notepads_markdown_svg_resources_read_fault(handle, &mut observed, &mut error()),
                0
            );
            assert_eq!(
                observed,
                SvgFault::default(),
                "invalid input cannot poison first fault"
            );
            let original = SvgFault {
                kind: 2,
                limit,
                ..Default::default()
            };
            assert_eq!(
                notepads_markdown_svg_resources_record_fault(handle, &original, &mut error()),
                0
            );
            let later = SvgFault {
                kind: 2,
                limit: if limit == 14 { 15 } else { 14 },
                ..Default::default()
            };
            assert_eq!(
                notepads_markdown_svg_resources_record_fault(handle, &later, &mut error()),
                0
            );
            assert_eq!(
                notepads_markdown_svg_resources_read_fault(handle, &mut observed, &mut error()),
                0
            );
            assert_eq!(observed, original);
            assert_eq!(reclaim(&mut fixture, &mut handle).first_fault, original);
        }
    }
}

#[test]
fn resources_snapshot_reads_original_paid_rows_and_preserves_rejected_output() {
    let _process = crate::core::images::serialize_process_image_tests();
    let mut f = Fixture::new();
    let mut p = unsafe { resources(&mut f) };
    let mut snapshot = SvgWorkSnapshot {
        header: SvgWorkHeader {
            abi_version: SVG_WORK_ABI_VERSION,
            struct_size: size_of::<SvgWorkSnapshot>() as u32,
        },
        ..Default::default()
    };
    assert_eq!(
        unsafe { notepads_markdown_svg_resources_read_work(p, &mut snapshot, &mut error()) },
        0
    );
    let baseline = snapshot;
    assert_ne!(baseline.account_nonce, 0);
    let mut ticket = SvgWorkToken::default();
    assert_eq!(
        unsafe { notepads_markdown_svg_resources_reserve(p, 6, 64, &mut ticket, &mut error()) },
        0
    );
    assert_eq!(
        unsafe { notepads_markdown_svg_resources_read_work(p, &mut snapshot, &mut error()) },
        0
    );
    assert_eq!(snapshot.account_nonce, ticket.account_nonce);
    assert_eq!(snapshot.tickets, baseline.tickets + 1);
    assert_eq!(snapshot.pending, baseline.pending + 1);
    assert_eq!(snapshot.job_bytes, baseline.job_bytes + 64);
    assert_eq!(snapshot.root_work_bytes, baseline.root_work_bytes + 64);
    assert_eq!(snapshot.control_bytes, baseline.control_bytes + 64);
    snapshot.header.abi_version = 99;
    snapshot.job_bytes = u64::MAX;
    assert_eq!(
        unsafe { notepads_markdown_svg_resources_read_work(p, &mut snapshot, &mut error()) },
        2
    );
    assert_eq!(snapshot.job_bytes, u64::MAX);
    snapshot.header.abi_version = SVG_WORK_ABI_VERSION;
    assert_eq!(
        unsafe { notepads_markdown_svg_resources_read_work(p, &mut snapshot, (&raw mut snapshot).cast()) },
        1
    );
    assert_eq!(snapshot.job_bytes, u64::MAX);
    assert_eq!(
        unsafe { notepads_markdown_svg_resources_read_work(p, p.cast(), &mut error()) },
        1
    );
    assert_eq!(
        unsafe { notepads_markdown_svg_resources_release_after_free(p, &ticket, &mut error()) },
        0
    );
    let fault = SvgFault {
        kind: 1,
        native_hr: -987,
        native_hr_present: 1,
        ..Default::default()
    };
    assert_eq!(
        unsafe { notepads_markdown_svg_resources_record_fault(p, &fault, &mut error()) },
        0
    );
    assert_eq!(
        unsafe { notepads_markdown_svg_resources_read_work(p, &mut snapshot, &mut error()) },
        0
    );
    assert_eq!(
        (snapshot.tickets, snapshot.pending),
        (baseline.tickets, baseline.pending)
    );
    assert_eq!(snapshot.job_bytes, baseline.job_bytes);
    let mut observed = SvgFault::default();
    assert_eq!(
        unsafe { notepads_markdown_svg_resources_read_fault(p, &mut observed, &mut error()) },
        0
    );
    assert_eq!(observed, fault);
    assert_eq!(unsafe { reclaim(&mut f, &mut p) }.first_fault, fault);
}
unsafe fn reclaim(f: &mut Fixture, p: &mut *mut SvgResources) -> SvgResourcesRetirement {
    let mut out = receipt();
    assert_eq!(
        unsafe { notepads_markdown_svg_resources_reclaim(p, &mut out, &mut error()) },
        0
    );
    assert!(p.is_null());
    assert_eq!((out.consumed, out.cleanup_succeeded), (1, 1));
    assert_eq!(out.unreleased_header, SvgWorkToken::default());
    assert_eq!(out.unreleased_account, SvgWorkToken::default());
    f.accounts[0] = out.account;
    f.destroy(0);
    out
}
#[test]
fn resources_transfer_real_account_allocate_admit_free_then_reclaim() {
    let _process = crate::core::images::serialize_process_image_tests();
    let mut f = Fixture::new();
    let mut p = unsafe { resources(&mut f) };
    let mut ticket = SvgWorkToken::default();
    assert_eq!(
        unsafe { notepads_markdown_svg_resources_reserve(p, 6, 64, &mut ticket, &mut error()) },
        0
    );
    let bytes = vec![9u8; 64];
    assert_eq!(
        unsafe { notepads_markdown_svg_resources_admit(p, &ticket, bytes.capacity() as u64, &mut error()) },
        0
    );
    drop(bytes);
    assert_eq!(
        unsafe { notepads_markdown_svg_resources_release_after_free(p, &ticket, &mut error()) },
        0
    );
    assert_eq!(
        unsafe { reclaim(&mut f, &mut p) }.first_fault,
        SvgFault::default()
    );
}
#[test]
fn native_original_fault_survives_cleanup_and_blocks_new_allocation() {
    let _process = crate::core::images::serialize_process_image_tests();
    let mut f = Fixture::new();
    let mut p = unsafe { resources(&mut f) };
    let mut ticket = SvgWorkToken::default();
    assert_eq!(
        unsafe { notepads_markdown_svg_resources_reserve(p, 6, 64, &mut ticket, &mut error()) },
        0
    );
    let original = SvgFault {
        kind: 1,
        limit: 0,
        native_hr: -101,
        native_hr_present: 1,
    };
    assert_eq!(
        unsafe { notepads_markdown_svg_resources_record_fault(p, &original, &mut error()) },
        0
    );
    let later = SvgFault {
        kind: 1,
        limit: 0,
        native_hr: -777,
        native_hr_present: 1,
    };
    assert_eq!(
        unsafe { notepads_markdown_svg_resources_record_fault(p, &later, &mut error()) },
        0
    );
    let mut sentinel = SvgWorkToken {
        account_nonce: 99,
        slot: 77,
        generation: 66,
    };
    let expected = sentinel;
    assert_ne!(
        unsafe { notepads_markdown_svg_resources_reserve(p, 6, 1, &mut sentinel, &mut error()) },
        0
    );
    assert_eq!(sentinel, expected);
    assert_eq!(
        unsafe { notepads_markdown_svg_resources_release_after_free(p, &ticket, &mut error()) },
        0
    );
    assert_eq!(unsafe { reclaim(&mut f, &mut p) }.first_fault, original);
}
#[test]
fn shared_fault_engine_finish_and_busy_reclaim_keep_exact_owners() {
    let _process = crate::core::images::serialize_process_image_tests();
    let mut f = Fixture::new();
    let mut p = unsafe { resources(&mut f) };
    let (host, fault) = unsafe { engine_resources(p) };
    let metadata = markdown_svg::bootstrap_metadata();
    let bytes = metadata.total_bytes().unwrap() - metadata.first_failure_arc_bytes;
    let payment = host
        .reserve(markdown_svg_budget::ResourceKind::Scratch, bytes)
        .unwrap();
    // Reuse only callback declarations: no fake parser, allocation or account.
    let callbacks = super::access_tests::callbacks();
    let mut job = Box::new(
        markdown_svg::OwnedJob::new_with_first_failure(
            markdown_svg::Identity {
                job_nonce: 1,
                source_generation: 1,
                provider_generation: 1,
                font_generation: 1,
                target_generation: 1,
            },
            callbacks,
            host.clone(),
            Default::default(),
            fault,
        )
        .unwrap(),
    );
    host.allocation_committed(&payment, bytes).unwrap();
    let mut out = receipt();
    let original = p;
    assert_ne!(
        unsafe { notepads_markdown_svg_resources_reclaim(&mut p, &mut out, &mut error()) },
        0
    );
    assert_eq!(p, original);
    assert_eq!(out.consumed, 0);
    job.prepare(br#"<svg xmlns="http://www.w3.org/2000/svg" width="10" height="10"/>"#)
        .unwrap();
    assert_eq!(
        job.finish_on_worker().unwrap().cleanup_status,
        markdown_svg::CleanupStatus::Succeeded
    );
    drop(job);
    host.release_after_free(payment).unwrap();
    drop(host);
    assert_eq!(
        unsafe { reclaim(&mut f, &mut p) }.first_fault,
        SvgFault::default()
    );
}
#[test]
fn fault_only_extra_reference_rejects_close_without_latching_an_error() {
    let _process = crate::core::images::serialize_process_image_tests();
    let mut f = Fixture::new();
    let mut p = unsafe { resources(&mut f) };
    let (host, fault) = unsafe { engine_resources(p) };
    drop(host);
    let mut out = receipt();
    assert_ne!(
        unsafe { notepads_markdown_svg_resources_reclaim(&mut p, &mut out, &mut error()) },
        0
    );
    let mut observed = SvgFault::default();
    assert_eq!(
        unsafe { notepads_markdown_svg_resources_read_fault(p, &mut observed, &mut error()) },
        0
    );
    assert_eq!(observed, SvgFault::default());
    drop(fault);
    unsafe { reclaim(&mut f, &mut p) };
}
#[test]
fn shared_font_subcap_uses_existing_rows_and_refunds_pending_claims() {
    let _process = crate::core::images::serialize_process_image_tests();
    let mut f = Fixture::new();
    let mut p = unsafe { resources(&mut f) };
    let mut a = SvgWorkToken::default();
    let mut b = SvgWorkToken::default();
    assert_eq!(
        unsafe { notepads_markdown_svg_resources_reserve(p, 1, 32 * 1024 * 1024, &mut a, &mut error()) },
        0
    );
    assert_eq!(
        unsafe { notepads_markdown_svg_resources_reserve(p, 1, 32 * 1024 * 1024, &mut b, &mut error()) },
        0
    );
    let mut sentinel = SvgWorkToken::default();
    assert_eq!(
        unsafe { notepads_markdown_svg_resources_reserve(p, 1, 1, &mut sentinel, &mut error()) },
        9
    );
    assert_eq!(
        unsafe { notepads_markdown_svg_resources_release_after_free(p, &a, &mut error()) },
        0
    );
    assert_eq!(
        unsafe { notepads_markdown_svg_resources_release_after_free(p, &b, &mut error()) },
        0
    );
    let out = unsafe { reclaim(&mut f, &mut p) };
    assert_eq!(out.first_fault.kind, 2);
}
#[test]
fn resource_alias_range_and_bad_fault_preflight_preserve_owner() {
    let _process = crate::core::images::serialize_process_image_tests();
    let mut f = Fixture::new();
    let mut p = unsafe { resources(&mut f) };
    let original = p;
    let mut out = receipt();
    out.header.abi_version = 99;
    assert_eq!(
        unsafe { notepads_markdown_svg_resources_reclaim(&mut p, &mut out, &mut error()) },
        2
    );
    assert_eq!(p, original);
    assert_eq!(out.consumed, 0);
    let bad = SvgFault {
        kind: 1,
        limit: 0,
        native_hr: 0,
        native_hr_present: 1,
    };
    assert_ne!(
        unsafe { notepads_markdown_svg_resources_record_fault(p, &bad, &mut error()) },
        0
    );
    assert_ne!(
        unsafe { notepads_markdown_svg_resources_reserve(p, 6, 1, p.cast::<SvgWorkToken>(), &mut error()) },
        0
    );
    unsafe { reclaim(&mut f, &mut p) };
}

#[test]
fn create_actual_header_allocation_failure_returns_the_original_canonical_account() {
    let _process = crate::core::images::serialize_process_image_tests();
    let mut f = Fixture::new();
    f.create(0);
    let original = f.accounts[0];
    let sentinel = 1usize as *mut SvgResources;
    let mut output = sentinel;
    super::super::resources_bridge::inject(1);
    assert_eq!(
        unsafe { notepads_markdown_svg_resources_create(1, &mut f.accounts[0], &mut output, &mut error()) },
        7
    );
    assert_eq!(f.accounts[0], original);
    assert_eq!(output, sentinel);
    let mut output = ptr::null_mut();
    assert_eq!(
        unsafe { notepads_markdown_svg_resources_create(1, &mut f.accounts[0], &mut output, &mut error()) },
        0
    );
    unsafe { reclaim(&mut f, &mut output) };
}
#[test]
fn post_free_release_failure_consumes_handle_and_keeps_real_fee_in_quarantine() {
    let _process = crate::core::images::serialize_process_image_tests();
    let mut f = Fixture::new();
    let mut p = unsafe { resources(&mut f) };
    let mut out = receipt();
    super::super::resources_bridge::inject(2);
    assert_ne!(
        unsafe { notepads_markdown_svg_resources_reclaim(&mut p, &mut out, &mut error()) },
        0
    );
    assert!(p.is_null());
    assert_eq!((out.consumed, out.cleanup_succeeded), (1, 0));
    assert!(!out.account.is_null());
    assert_ne!(out.unreleased_header, SvgWorkToken::default());
    assert_eq!(out.unreleased_account, SvgWorkToken::default());
    assert_eq!(out.first_fault.kind, 7);
    // Test-only observation of the original funded row, not permission to retry
    // the quarantined account. Keep the original request/model/cancel pins.
    assert!(unsafe { (*out.account).value.funding.stats().job_bytes } > 0);
    std::mem::forget(f);
}
#[test]
fn post_commit_panic_has_stable_consumed_receipt_and_preserves_native_first_fault() {
    let _process = crate::core::images::serialize_process_image_tests();
    let mut f = Fixture::new();
    let mut p = unsafe { resources(&mut f) };
    let mut out = receipt();
    let original = SvgFault {
        kind: 1,
        limit: 0,
        native_hr: -101,
        native_hr_present: 1,
    };
    assert_eq!(
        unsafe { notepads_markdown_svg_resources_record_fault(p, &original, &mut error()) },
        0
    );
    super::super::resources_bridge::inject(3);
    assert_eq!(
        unsafe { notepads_markdown_svg_resources_reclaim(&mut p, &mut out, &mut error()) },
        STATUS_PANIC
    );
    assert!(p.is_null());
    assert_eq!((out.consumed, out.cleanup_succeeded), (1, 0));
    assert_eq!(out.first_fault, original);
    assert_ne!(out.unreleased_header, SvgWorkToken::default());
    assert_ne!(out.unreleased_account, SvgWorkToken::default());
    assert!(unsafe { (*out.account).value.funding.stats().job_bytes } > 0);
    std::mem::forget(f);
}

#[test]
fn capability_creation_errors_clear_output_without_transferring_inputs() {
    let _process = crate::core::images::serialize_process_image_tests();
    let mut fixture = Fixture::new();
    fixture.create(0);
    let original = fixture.accounts[0];
    let mut handle = ptr::dangling_mut::<SvgResources>();
    assert_eq!(
        unsafe {
            notepads_markdown_svg_resources_create(
                2,
                &mut fixture.accounts[0],
                &mut handle,
                &mut error(),
            )
        },
        STATUS_ABI_MISMATCH
    );
    assert_eq!(handle, ptr::dangling_mut::<SvgResources>());
    assert_eq!(fixture.accounts[0], original);
    assert_eq!(unsafe {
        notepads_markdown_svg_resources_create(1, &mut fixture.accounts[0], &mut handle, &mut error())
    }, 0);
    let mut allocation = ptr::null_mut();
    let mut retained = ptr::dangling_mut::<Owned<SvgRetained>>();
    let mut initial = SvgWorkToken::default();
    assert_eq!(
        unsafe {
            notepads_markdown_svg_resources_retained_prepare(
                2,
                handle,
                1,
                ptr::null(),
                &mut allocation,
                256,
                0,
                &mut retained,
                &mut initial,
                &mut error(),
            )
        },
        STATUS_ABI_MISMATCH
    );
    assert!(retained.is_null());
    assert!(allocation.is_null());
    assert_eq!(initial, SvgWorkToken::default());
    unsafe { reclaim(&mut fixture, &mut handle) };
}

#[test]
fn retained_never_allocated_transaction_uses_owned_capability_and_original_root() {
    let _process = crate::core::images::serialize_process_image_tests();
    let mut f = Fixture::new();
    let mut p = unsafe { resources(&mut f) };
    let mut preview = ptr::null_mut();
    let mut retained = ptr::null_mut();
    let mut ticket = SvgWorkToken::default();
    assert_eq!(
        unsafe {
            notepads_markdown_svg_resources_retained_prepare(
                1,
                p,
                1,
                ptr::null(),
                &mut preview,
                256,
                0,
                &mut retained,
                &mut ticket,
                &mut error(),
            )
        },
        0
    );
    assert!(!preview.is_null() && !retained.is_null());
    // No physical payload was allocated: unwind the actual pending creator,
    // preview reservation/header, then retained record in the original order.
    assert_eq!(
        unsafe { notepads_markdown_svg_resources_return_retained(p, &ticket, &mut error()) },
        0
    );
    assert_eq!(
        unsafe { crate::core::preview::notepads_markdown_preview_allocation_destroy(preview) },
        0
    );
    unsafe { notepads_markdown_svg_retained_abort_unallocated(retained) };
    unsafe { reclaim(&mut f, &mut p) };
}
