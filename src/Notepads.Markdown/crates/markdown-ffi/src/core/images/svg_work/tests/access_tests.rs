// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE file in the project root for license information.
// ---------------------------------------------------------------------------------------------

use super::super::access::AccountOwner;
use super::*;
use markdown_svg::{CleanupStatus, Host, Identity, JobState, OwnedJob, Target, abi};
use markdown_svg_budget::{FailureKind, Limits, ResourceKind, Ticket};

unsafe extern "C" fn no_plan(_: u64, _: *const abi::Draft, _: *mut abi::Plan) -> i32 {
    -101
}
unsafe extern "C" fn no_font(_: u64, _: u32, _: *mut u8, _: u64, _: *mut u64) -> i32 {
    -102
}
unsafe extern "C" fn no_metrics(
    _: u64,
    _: u32,
    _: u32,
    _: *mut abi::GlyphMetrics,
    _: u32,
    _: *mut u32,
) -> i32 {
    -103
}
unsafe extern "C" fn no_geometry(
    _: u64,
    _: *const abi::GlyphKey,
    _: u32,
    _: *mut abi::GeometryReceipt,
) -> i32 {
    -104
}
unsafe extern "C" fn no_fill_geometry(
    _: u64,
    _: u64,
    _: *mut abi::GlyphPath,
    _: u32,
    _: *mut abi::Command,
    _: u32,
    _: *mut u32,
    _: *mut u32,
) -> i32 {
    -105
}
unsafe extern "C" fn no_retire(_: u64, _: u64) -> i32 {
    -106
}
pub(super) fn callbacks() -> abi::Callbacks {
    abi::Callbacks {
        size: size_of::<abi::Callbacks>() as u32,
        version: abi::FONT_CALLBACKS_VERSION,
        session: 1,
        plan: Some(no_plan),
        fill_font: Some(no_font),
        fill_metrics: Some(no_metrics),
        prepare_geometry: Some(no_geometry),
        fill_geometry: Some(no_fill_geometry),
        retire_geometry: Some(no_retire),
    }
}
fn identity() -> Identity {
    Identity {
        job_nonce: 1,
        source_generation: 1,
        provider_generation: 1,
        font_generation: 1,
        target_generation: 1,
    }
}

fn create_job(owner: &AccountOwner) -> (Box<OwnedJob>, Ticket) {
    let host = owner.host();
    let bytes = markdown_svg::bootstrap_metadata().total_bytes().unwrap();
    let payment = host.reserve(ResourceKind::Scratch, bytes).unwrap();
    let job =
        Box::new(OwnedJob::new(identity(), callbacks(), host.clone(), Limits::default()).unwrap());
    host.allocation_committed(&payment, bytes).unwrap();
    (job, payment)
}

#[test]
fn actual_account_owned_job_renders_then_physically_retires() {
    let _process = crate::core::images::serialize_process_image_tests();
    let mut fixture = Fixture::new();
    fixture.create(0);
    let baseline = fixture.budget.usage().auxiliary_capacity;
    let mut owner = unsafe { AccountOwner::take(&mut fixture.accounts[0]).unwrap() };
    assert!(fixture.accounts[0].is_null());
    let host = owner.host();
    let (mut job, bootstrap) = create_job(&owner);
    let cancel = job.cancellation();
    let natural = job.prepare(br##"<svg xmlns="http://www.w3.org/2000/svg" width="100.5" height="50.25"><rect width="100.5" height="50.25" fill="#ff0000"/></svg>"##).unwrap();
    assert_eq!((natural.width_dip, natural.height_dip), (100.5, 50.25));
    let plan = job
        .set_target(Target {
            display_width_dip: 201.0,
            effective_physical_scale: 1.0,
            generation: 1,
        })
        .unwrap();
    let mut pixels = vec![0xcc; plan.capacity + 16];
    job.run_into(plan.identity, &mut pixels[..plan.capacity], plan.stride)
        .unwrap();
    assert!(
        pixels[..plan.capacity]
            .chunks_exact(4)
            .any(|p| p == [0, 0, 255, 255])
    );
    assert!(pixels[plan.capacity..].iter().all(|b| *b == 0xcc));
    let receipt = job.finish_on_worker().unwrap();
    assert_eq!(receipt.terminal_state, JobState::Rendered);
    assert_eq!(receipt.cleanup_status, CleanupStatus::Succeeded);
    assert_eq!(receipt.first_fault, None);
    assert!(job.can_destroy_shell());
    drop(job);
    drop(cancel);
    host.release_after_free(bootstrap).unwrap();
    assert_eq!(host.snapshot().0, 1); // Only real AccountAccess Arc remains.
    assert_eq!(host.snapshot().1, 0);
    assert!(owner.reclaim().is_err()); // The independent Host still pins it.
    drop(host);
    fixture.accounts[0] = owner.reclaim().unwrap();
    assert!(fixture.budget.usage().auxiliary_capacity >= baseline);
    fixture.destroy(0);
    assert_eq!(
        unsafe { (&(*fixture.policy).value.owner).policy.svg_work_bytes() },
        0
    );
}

#[test]
fn real_account_admit_and_foreign_identity_keep_pending_until_actual_cleanup() {
    let _process = crate::core::images::serialize_process_image_tests();
    let mut fixture = Fixture::new();
    fixture.create(0);
    fixture.create(1);
    let mut a = unsafe { AccountOwner::take(&mut fixture.accounts[0]).unwrap() };
    let mut b = unsafe { AccountOwner::take(&mut fixture.accounts[1]).unwrap() };
    let ha = a.host();
    let hb = b.host();
    let ticket = ha.reserve(ResourceKind::Scratch, 128).unwrap();
    assert_eq!(ha.snapshot().1, 1);
    let storage = vec![7u8; 128];
    let error = hb
        .allocation_committed(&ticket, storage.capacity())
        .unwrap_err();
    assert_eq!(error.kind, FailureKind::Invariant);
    assert_eq!(ha.snapshot().1, 1);
    ha.allocation_committed(&ticket, storage.capacity())
        .unwrap();
    assert_eq!(ha.snapshot().1, 0);
    assert!(
        ha.allocation_committed(&ticket, storage.capacity())
            .is_err()
    );
    drop(storage);
    ha.release_after_free(ticket).unwrap();
    assert_eq!(ha.snapshot().0, 1);
    drop(ha);
    drop(hb);
    fixture.accounts[0] = a.reclaim().unwrap();
    fixture.accounts[1] = b.reclaim().unwrap();
    fixture.destroy(0);
    fixture.destroy(1);
}

#[test]
fn actual_cancelled_engine_still_retires_same_account_without_clearing_fault() {
    let _process = crate::core::images::serialize_process_image_tests();
    let mut fixture = Fixture::new();
    fixture.create(0);
    let mut owner = unsafe { AccountOwner::take(&mut fixture.accounts[0]).unwrap() };
    let host = owner.host();
    let (mut job, bootstrap) = create_job(&owner);
    // Real original Core cancellation token, independent of the engine flag.
    unsafe {
        (*fixture.cancel).value.token.cancel();
    }
    assert!(
        job.prepare(br#"<svg xmlns="http://www.w3.org/2000/svg" width="10" height="10"/>"#)
            .is_err()
    );
    assert_eq!(host.snapshot().3, Some(CoreError::Cancelled));
    let receipt = job.finish_on_worker().unwrap();
    assert_eq!(receipt.cleanup_status, CleanupStatus::Succeeded);
    assert_eq!(receipt.first_fault.unwrap().kind, FailureKind::Cancelled);
    drop(job);
    host.release_after_free(bootstrap).unwrap();
    drop(host);
    fixture.accounts[0] = owner.reclaim().unwrap();
    fixture.destroy(0);
}

#[test]
fn actual_two_engine_jobs_share_control_and_keep_separate_ticket_identity() {
    let _process = crate::core::images::serialize_process_image_tests();
    let mut fixture = Fixture::new();
    fixture.create(0);
    fixture.create(1);
    let mut a = unsafe { AccountOwner::take(&mut fixture.accounts[0]).unwrap() };
    let mut b = unsafe { AccountOwner::take(&mut fixture.accounts[1]).unwrap() };
    let ha = a.host();
    let hb = b.host();
    let (mut ja, pa) = create_job(&a);
    let (mut jb, pb) = create_job(&b);
    for job in [&mut ja, &mut jb] {
        job.prepare(br#"<svg xmlns="http://www.w3.org/2000/svg" width="32" height="16"><circle cx="8" cy="8" r="4"/></svg>"#).unwrap();
    }
    assert_ne!(pa.account_nonce(), pb.account_nonce());
    let control = unsafe { (&(*fixture.policy).value.owner).policy.svg_work_bytes() };
    assert!(control > 2 * 32768);
    assert!(
        ha.reserve(
            ResourceKind::Scratch,
            128 * 1024 * 1024 - control as usize + 1
        )
        .is_err()
    );
    assert_eq!(
        unsafe { (&(*fixture.policy).value.owner).policy.svg_work_bytes() },
        control
    );
    assert_eq!(
        ha.snapshot().3,
        Some(CoreError::ResourceLimit {
            kind: markdown_core::error::LimitKind::AuxiliaryCapacity,
            requested: 128 * 1024 * 1024 + 1,
            limit: 128 * 1024 * 1024,
        })
    );
    assert_eq!(
        ja.finish_on_worker().unwrap().cleanup_status,
        CleanupStatus::Succeeded
    );
    assert_eq!(
        jb.finish_on_worker().unwrap().cleanup_status,
        CleanupStatus::Succeeded
    );
    drop(ja);
    drop(jb);
    ha.release_after_free(pa).unwrap();
    hb.release_after_free(pb).unwrap();
    drop(ha);
    drop(hb);
    fixture.accounts[0] = a.reclaim().unwrap();
    fixture.accounts[1] = b.reclaim().unwrap();
    fixture.destroy(0);
    fixture.destroy(1);
}

#[test]
fn panic_poison_forbids_new_work_but_actual_free_cleanup_remains_available() {
    let _process = crate::core::images::serialize_process_image_tests();
    let mut fixture = Fixture::new();
    fixture.create(0);
    let mut owner = unsafe { AccountOwner::take(&mut fixture.accounts[0]).unwrap() };
    let host = owner.host();
    let ticket = host.reserve(ResourceKind::Scratch, 64).unwrap();
    let bytes = vec![0u8; 64];
    host.allocation_committed(&ticket, bytes.capacity())
        .unwrap();
    assert!(
        std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| host.panic_with_lock())).is_err()
    );
    assert!(host.reserve(ResourceKind::Scratch, 1).is_err());
    assert_eq!(host.snapshot().3, Some(CoreError::InternalInvariant));
    drop(bytes);
    host.release_after_free(ticket).unwrap();
    drop(host);
    fixture.accounts[0] = owner.reclaim().unwrap();
    fixture.destroy(0);
}

#[test]
fn real_account_splits_engine_work_batches_without_resetting_operation() {
    let _process = crate::core::images::serialize_process_image_tests();
    let mut fixture = Fixture::new();
    fixture.create(0);
    let mut owner = unsafe { AccountOwner::take(&mut fixture.accounts[0]).unwrap() };
    let host = owner.host();
    for work in [0, 1, 255, 256, 257, 4096, 65536] {
        host.checkpoint(work).unwrap();
    }
    assert_eq!(host.snapshot().3, None);
    drop(host);
    fixture.accounts[0] = owner.reclaim().unwrap();
    fixture.destroy(0);
}

#[test]
fn poison_observed_by_lock_rejects_waiter_that_started_before_the_panic() {
    let _process = crate::core::images::serialize_process_image_tests();
    let mut fixture = Fixture::new();
    fixture.create(0);
    let mut owner = unsafe { AccountOwner::take(&mut fixture.accounts[0]).unwrap() };
    let host = owner.host();
    let original = host.snapshot();
    let (locked_tx, locked_rx) = std::sync::mpsc::sync_channel(0);
    let (panic_tx, panic_rx) = std::sync::mpsc::sync_channel(0);
    let panicking = host.clone();
    let holder = std::thread::spawn(move || {
        panicking.while_locked(|| {
            locked_tx.send(()).unwrap();
            panic_rx.recv().unwrap();
            panic!("concurrent account poison");
        })
    });
    locked_rx.recv().unwrap();
    let (before_tx, before_rx) = std::sync::mpsc::sync_channel(0);
    let (continue_tx, continue_rx) = std::sync::mpsc::sync_channel(0);
    let waiting = host.clone();
    let waiter = std::thread::spawn(move || {
        super::super::access::AccountAccess::before_next_lock(move || {
            before_tx.send(()).unwrap();
            continue_rx.recv().unwrap();
        });
        waiting.reserve(ResourceKind::Scratch, 1)
    });
    before_rx.recv().unwrap(); // Waiter is immediately before the real lock.
    panic_tx.send(()).unwrap();
    assert!(holder.join().is_err());
    continue_tx.send(()).unwrap();
    assert_eq!(
        waiter.join().unwrap().unwrap_err().kind,
        FailureKind::Invariant
    );
    let after = host.snapshot();
    assert_eq!(
        (after.0, after.1, after.2),
        (original.0, original.1, original.2)
    );
    assert_eq!(after.3, Some(CoreError::InternalInvariant));
    drop(host);
    fixture.accounts[0] = owner.reclaim().unwrap();
    fixture.destroy(0);
}

#[test]
fn real_system_arc_deallocation_still_observes_original_root_and_control_payment() {
    let _process = crate::core::images::serialize_process_image_tests();
    let mut fixture = Fixture::new();
    fixture.create(0);
    let mut owner = unsafe { AccountOwner::take(&mut fixture.accounts[0]).unwrap() };
    let (pointer, bytes) = owner.actual_arc_allocation();
    let root_before = fixture.budget.usage().auxiliary_capacity;
    let policy = unsafe { &(&(*fixture.policy).value.owner).policy };
    let control_before = policy.svg_work_bytes();
    unsafe {
        arm_retained_probe(&fixture.budget, policy, [pointer, 0, 0]);
    }
    fixture.accounts[0] = owner.reclaim().unwrap();
    let (count, events) = take_retained_probe();
    assert_eq!(count, 1);
    assert_eq!(events[0], (pointer, bytes, root_before, control_before));
    assert_eq!(
        fixture.budget.usage().auxiliary_capacity,
        root_before - bytes as u64
    );
    assert_eq!(policy.svg_work_bytes(), control_before - bytes as u64);
    fixture.destroy(0);
}

#[test]
fn reclaim_without_an_intervening_host_call_preserves_mutex_poison() {
    let _process = crate::core::images::serialize_process_image_tests();
    let mut fixture = Fixture::new();
    fixture.create(0);
    let mut owner = unsafe { AccountOwner::take(&mut fixture.accounts[0]).unwrap() };
    let host = owner.host();
    assert!(
        std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| host.panic_with_lock())).is_err()
    );
    drop(host);
    fixture.accounts[0] = owner.reclaim().unwrap();
    let mut sentinel = SvgWorkToken {
        account_nonce: 99,
        slot: 77,
        generation: 66,
    };
    assert_ne!(
        unsafe { notepads_markdown_svg_work_reserve(fixture.accounts[0], 0, 16, &mut sentinel, &mut error()) },
        0
    );
    assert_eq!(
        sentinel,
        SvgWorkToken {
            account_nonce: 99,
            slot: 77,
            generation: 66
        }
    );
    fixture.destroy(0);
}
