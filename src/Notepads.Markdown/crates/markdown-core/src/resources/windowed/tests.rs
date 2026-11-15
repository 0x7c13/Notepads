// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE file in the project root for license information.
// ---------------------------------------------------------------------------------------------

use super::*;
use std::sync::atomic::{AtomicUsize, Ordering};

static SERIAL: Mutex<()> = Mutex::new(());
static NOTIFICATIONS: AtomicUsize = AtomicUsize::new(0);
static CALLBACK_POLICY: Mutex<Option<ImageResourcePolicy>> = Mutex::new(None);

extern "C" fn notified(_: u64) {
    // Would deadlock if called under either ledger lock.
    let _ = process_image_usage();
    if let Some(policy) = CALLBACK_POLICY.lock().unwrap().as_ref() {
        let _ = policy.usage();
    }
    NOTIFICATIONS.fetch_add(1, Ordering::SeqCst);
}

fn epoch(generation: u64) -> SourceEpoch {
    SourceEpoch {
        source: SourceKey::new([91; 16], generation).unwrap(),
        source_generation: generation,
        provider_generation: 1,
    }
}
fn target(generation: u64, width: f64, scale: f64) -> ImageTarget {
    ImageTarget {
        font_generation: 1,
        generation,
        available_width_dip: width,
        effective_physical_scale: scale,
    }
}
fn make_policy() -> ImageResourcePolicy {
    let policy = ImageResourcePolicy::try_new().unwrap();
    policy.begin_source(epoch(1)).unwrap();
    policy
}
fn row(policy: &ImageResourcePolicy, object: u32) -> EntrySnapshot {
    let mut rows = [EntrySnapshot::default(); MAX_ENTRIES];
    let count = policy.snapshot(&mut rows).unwrap().written;
    *rows[..count]
        .iter()
        .find(|row| row.object == object)
        .unwrap()
}
fn metadata(side: u32) -> ImageMetadata {
    ImageMetadata {
        intrinsic_width: 4096,
        intrinsic_height: 4096,
        decoded_width: side,
        decoded_height: side,
        orientation: 1,
        stride: side * 4,
    }
}
fn publish(policy: &ImageResourcePolicy, object: u32, target: ImageTarget) -> ByteReservation {
    policy.desire_variant(object, target).unwrap();
    let request = if row(policy, object).metadata.is_some() {
        policy.start(object)
    } else {
        policy.start_unknown(object)
    }
    .unwrap();
    let plan = metadata(1).raster_plan(target).unwrap();
    let value = metadata(plan.width);
    policy
        .note_variant_metadata(request.key(), value.into())
        .unwrap();
    let bytes = request.reserve_decoded(plan.bytes).unwrap();
    bytes.admit_actual(plan.bytes).unwrap();
    policy
        .complete_variant_ready(request.key(), value.into(), &bytes)
        .unwrap();
    bytes
}
fn limited(policy: &ImageResourcePolicy, object: u32, target: ImageTarget, bytes: u64) {
    policy.desire_variant(object, target).unwrap();
    let request = if row(policy, object).metadata.is_some() {
        policy.start(object)
    } else {
        policy.start_unknown(object)
    }
    .unwrap();
    let result = request.reserve_decoded(bytes);
    assert!(matches!(
        result,
        Err(ResourceError::Limit {
            kind: ResourceLimit::DecodedBytes,
            ..
        })
    ));
    policy
        .complete_failure(
            request.key(),
            ImageFailure {
                kind: FailureKind::Limited,
                code: -1,
            },
        )
        .unwrap();
}

#[test]
fn replacing_an_image_source_revokes_pending_and_ready_rows_without_retiring_physical_owners() {
    let _serial = SERIAL.lock().unwrap();
    let policy = make_policy();
    let target = target(1, 128.0, 1.0);
    let ready = publish(&policy, 0, target);
    let ordinary = publish(&policy, 1, target);
    let old = row(&policy, 0);
    let unchanged = row(&policy, 1);
    policy.replace_variant_source(0).unwrap();
    let reset = row(&policy, 0);
    assert_eq!(reset.state, ImageState::Unrequested);
    assert_eq!(
        (reset.metadata, reset.request, reset.pending_request),
        (None, None, None)
    );
    assert_eq!(reset.desired_target, Some(target));
    assert_eq!(row(&policy, 1), unchanged);
    assert_eq!(policy.usage().decoded_bytes, 2 * 128 * 128 * 4);
    assert_eq!(policy.forget_variant(0), Err(ResourceError::InvalidState));
    assert!(old.request.is_some());

    let pending = policy.start_unknown(0).unwrap();
    let variant = metadata(128).into();
    policy
        .note_variant_metadata(pending.key(), variant)
        .unwrap();
    let planned = pending.reserve_decoded(128 * 128 * 4).unwrap();
    planned.admit_actual(128 * 128 * 4).unwrap();
    policy.replace_variant_source(0).unwrap();
    assert_eq!(policy.usage().active_requests, 1);
    assert_eq!(
        policy.complete_variant_ready(pending.key(), variant, &planned),
        Err(ResourceError::StaleResult)
    );
    assert!(row(&policy, 0).metadata.is_none());
    let selected = policy.start_unknown(0).unwrap();
    assert_eq!(policy.usage().active_requests, 2);
    let current = selected.reserve_decoded(128 * 128 * 4).unwrap();
    current.admit_actual(128 * 128 * 4).unwrap();
    policy.complete_variant_ready(selected.key(), variant, &current).unwrap();
    drop(planned);
    drop(pending);
    drop(selected);

    assert_ne!(row(&policy, 0).request, old.request);
    assert_eq!(row(&policy, 1), unchanged);
    drop((ready, ordinary, current));
    assert_eq!(policy.usage().decoded_bytes, 0);
}

#[test]
fn raster_planner_preserves_orientation_portrait_and_bucket_boundaries() {
    let _serial = SERIAL.lock().unwrap();
    for width in [
        1, 127, 128, 129, 255, 256, 257, 511, 512, 513, 1023, 1024, 1025, 2047, 2048, 2049, 8192,
    ] {
        let expected = [128, 256, 512, 1024, 2048]
            .into_iter()
            .find(|side| *side >= width)
            .unwrap_or(2048);
        let plan = metadata(1)
            .raster_plan(target(1, f64::from(width), 1.0))
            .unwrap();
        assert_eq!(plan.width, expected);
        assert_eq!(plan.bytes, u64::from(expected) * u64::from(expected) * 4);
    }
    let portrait = ImageMetadata {
        intrinsic_width: 512,
        intrinsic_height: 4096,
        decoded_width: 1,
        decoded_height: 1,
        orientation: 1,
        stride: 4,
    };
    assert_eq!(
        portrait.raster_plan(target(1, 256.0, 1.0)).unwrap().height,
        2048
    );
    let rotated = ImageMetadata {
        orientation: 6,
        ..portrait
    };
    let plan = rotated.raster_plan(target(1, 256.0, 1.0)).unwrap();
    assert_eq!((plan.width, plan.height), (256, 32));
    assert_eq!(
        metadata(512)
            .raster_plan(target(1, 256.0, 2.0))
            .unwrap()
            .width,
        512
    );
    assert_eq!(
        metadata(2048)
            .raster_plan(target(1, 512.0, f64::MAX))
            .unwrap()
            .width,
        2048
    );
}

#[test]
fn full_process_same_target_waits_for_actual_last_owner_and_cross_control_capacity() {
    let _serial = SERIAL.lock().unwrap();
    let a = make_policy();
    let b = make_policy();
    let c = make_policy();
    *CALLBACK_POLICY.lock().unwrap() = Some(a.clone());
    set_process_image_capacity_notify(Some(notified));
    NOTIFICATIONS.store(0, Ordering::SeqCst);
    let mut owners = Vec::new();
    for policy in [&b, &c] {
        for object in 0..16 {
            owners.push(publish(policy, object, target(1, 2048.0, 1.0)));
        }
    }
    assert_eq!(
        process_image_usage().decoded_bytes,
        MAX_PROCESS_DECODED_BYTES
    );
    let frame = owners[0].clone();
    limited(&a, 9, target(1, 2048.0, 1.0), 16 * 1024 * 1024);
    assert!(!a.retry_eligible(9).unwrap());
    let generation = process_image_usage().generation;
    drop(owners.remove(0));
    assert_eq!(process_image_usage().generation, generation);
    assert_eq!(NOTIFICATIONS.load(Ordering::SeqCst), 0);
    assert!(!a.retry_eligible(9).unwrap());
    std::thread::spawn(move || drop(frame)).join().unwrap();
    assert!(a.retry_eligible(9).unwrap());
    assert!(row(&a, 9).retry_eligible);
    assert_eq!(NOTIFICATIONS.load(Ordering::SeqCst), 1);
    let next = a.start_unknown(9).unwrap();
    let lease = next.reserve_decoded(16 * 1024 * 1024).unwrap();
    lease.admit_actual(16 * 1024 * 1024).unwrap();
    a.complete_variant_ready(next.key(), metadata(2048).into(), &lease)
        .unwrap();
    assert!(row(&a, 9).target_satisfied());
    drop((next, lease, owners));
    set_process_image_capacity_notify(None);
    *CALLBACK_POLICY.lock().unwrap() = None;
    assert_eq!(process_image_usage().decoded_bytes, 0);
}

#[test]
fn full_budget_upgrade_withdraws_only_its_publication_and_waits_for_old_frame() {
    let _serial = SERIAL.lock().unwrap();
    let policy = make_policy();
    let mut owners = Vec::new();
    for object in 0..15 {
        owners.push(publish(&policy, object, target(1, 2048.0, 1.0)));
    }
    let small = publish(&policy, 15, target(1, 1024.0, 1.0));
    let frame = small.clone();
    limited(&policy, 15, target(2, 2048.0, 1.0), 16 * 1024 * 1024);
    assert_eq!(row(&policy, 15).state, ImageState::Ready);
    policy.evict_variant(15).unwrap();
    let metadata_only = row(&policy, 15);
    assert_eq!(metadata_only.state, ImageState::Unrequested);
    assert_eq!(metadata_only.metadata, Some(metadata(1024).into()));
    assert!(!metadata_only.target_satisfied());
    assert_eq!(policy.usage().decoded_bytes, 244 * 1024 * 1024);
    assert!(policy.forget_variant(15).is_err());
    // Withdrawal preserves the capacity-denial gate. A map erase or one owner
    // drop cannot authorize another attempt while the old frame still pins it.
    drop(small);
    assert!(policy.start(15).is_err());
    assert!(!policy.retry_eligible(15).unwrap());
    drop(frame);
    assert!(policy.retry_eligible(15).unwrap());
    let upgraded = publish(&policy, 15, target(2, 2048.0, 1.0));
    assert_eq!(policy.usage().decoded_bytes, MAX_DECODED_BYTES);
    assert_eq!(row(&policy, 15).metadata, Some(metadata(2048).into()));
    drop((upgraded, owners));
}

#[test]
fn same_bucket_font_changes_and_smaller_targets_do_not_decode() {
    let _serial = SERIAL.lock().unwrap();
    let policy = make_policy();
    let bytes = publish(&policy, 1, target(1, 257.0, 1.0));
    for (generation, width) in [(2, 511.0), (3, 128.0)] {
        let mut next = target(generation, width, 1.0);
        next.font_generation = generation;
        policy.desire_variant(1, next).unwrap();
        assert!(row(&policy, 1).target_satisfied());
        assert!(policy.start(1).is_err());
    }
    drop(bytes);
}

#[test]
fn row_recycling_requires_all_current_owners_and_rejects_stale_completion() {
    let _serial = SERIAL.lock().unwrap();
    let policy = make_policy();
    policy.desire_variant(0, target(1, 128.0, 1.0)).unwrap();
    let old = policy.start_unknown(0).unwrap();
    let old_key = old.key();
    let bytes = old.reserve_encoded(1).unwrap();
    bytes.admit_actual(1).unwrap();
    policy.evict_variant(0).unwrap();
    assert!(policy.forget_variant(0).is_err());
    drop(old);
    assert!(policy.forget_variant(0).is_err());
    drop(bytes);
    policy.forget_variant(0).unwrap();
    for object in 1..=MAX_ENTRIES as u32 {
        policy
            .desire_variant(object, target(1, 128.0, 1.0))
            .unwrap();
    }
    assert!(policy.desire_variant(257, target(1, 128.0, 1.0)).is_err());
    policy.forget_variant(1).unwrap();
    policy.desire_variant(257, target(1, 128.0, 1.0)).unwrap();
    policy.forget_variant(2).unwrap();
    policy.desire_variant(0, target(1, 128.0, 1.0)).unwrap();
    let current = policy.start_unknown(0).unwrap();
    assert!(current.key().request > old_key.request);
    assert_eq!(
        policy.complete_failure(
            old_key,
            ImageFailure {
                kind: FailureKind::Cancelled,
                code: 0
            }
        ),
        Err(ResourceError::StaleResult)
    );
    drop(current);
}

#[test]
fn only_recoverable_capacity_failure_retries_and_generation_without_fit_does_not() {
    let _serial = SERIAL.lock().unwrap();
    let policy = make_policy();
    let mut owners = Vec::new();
    for object in 0..16 {
        owners.push(publish(&policy, object, target(1, 2048.0, 1.0)));
    }
    limited(&policy, 18, target(1, 2048.0, 1.0), 16 * 1024 * 1024);
    let other = make_policy();
    other.desire_variant(1, target(1, 128.0, 1.0)).unwrap();
    let request = other.start_unknown(1).unwrap();
    let encoded = request.reserve_encoded(1).unwrap();
    encoded.admit_actual(1).unwrap();
    drop(encoded); // Advances physical generation, but decoded capacity is full.
    assert!(!policy.retry_eligible(18).unwrap());
    other
        .complete_failure(
            request.key(),
            ImageFailure {
                kind: FailureKind::Limited,
                code: -1,
            },
        )
        .unwrap();
    assert!(!other.retry_eligible(1).unwrap()); // No genuine failed capacity reserve.
    drop((request, owners));
    assert!(!other.retry_eligible(1).unwrap());
}

#[test]
fn competing_controls_cannot_oversubscribe_and_failed_admit_keeps_original_charge() {
    let _serial = SERIAL.lock().unwrap();
    let policies: Vec<_> = (0..8).map(|_| make_policy()).collect();
    let barrier = Arc::new(std::sync::Barrier::new(8));
    let workers: Vec<_> = policies
        .iter()
        .cloned()
        .map(|policy| {
            let barrier = barrier.clone();
            std::thread::spawn(move || {
                policy.desire_variant(1, target(1, 2048.0, 1.0)).unwrap();
                let request = policy.start_unknown(1).unwrap();
                barrier.wait();
                let result = request.reserve_decoded(128 * 1024 * 1024);
                barrier.wait();
                (request, result.ok())
            })
        })
        .collect();
    let results: Vec<_> = workers
        .into_iter()
        .map(|worker| worker.join().unwrap())
        .collect();
    assert_eq!(
        results.iter().filter(|(_, bytes)| bytes.is_some()).count(),
        4
    );
    assert_eq!(
        process_image_usage().decoded_bytes,
        MAX_PROCESS_DECODED_BYTES
    );
    let bytes = results
        .iter()
        .find_map(|(_, bytes)| bytes.as_ref())
        .unwrap();
    assert!(bytes.admit_actual(129 * 1024 * 1024).is_err());
    assert_eq!(bytes.reserved_bytes(), 128 * 1024 * 1024);
    assert_eq!(
        process_image_usage().decoded_bytes,
        MAX_PROCESS_DECODED_BYTES
    );
    drop(results);
    assert_eq!(process_image_usage().decoded_bytes, 0);
}

#[test]
fn reused_pixels_share_one_charge_and_last_drop_clears_all_current_bindings() {
    let _serial = SERIAL.lock().unwrap();
    let policy = make_policy();
    let pixels = publish(&policy, 1, target(1, 128.0, 1.0));
    let birth = pixels.key();
    policy.begin_source(epoch(2)).unwrap();
    for object in [7, 8] {
        policy
            .desire_variant(object, target(2, 128.0, 1.0))
            .unwrap();
        let request = policy.start_unknown(object).unwrap();
        assert_eq!(
            policy.complete_variant_ready(request.key(), metadata(128).into(), &pixels),
            Err(ResourceError::WrongReservation)
        );
        policy
            .complete_reused_variant(request.key(), metadata(128).into(), &pixels)
            .unwrap();
        assert_eq!(row(&policy, object).request, Some(request.key()));
    }
    assert_eq!(pixels.key(), birth);
    assert_eq!(policy.usage().reservations, 1);
    assert_eq!(policy.usage().decoded_bytes, 128 * 128 * 4);
    assert_eq!(process_image_usage().decoded_bytes, 128 * 128 * 4);
    let mut rows = [EntrySnapshot::default(); MAX_ENTRIES];
    let before = policy.snapshot(&mut rows).unwrap().state_generation;
    std::thread::spawn(move || drop(pixels)).join().unwrap();
    assert_eq!(policy.usage().reservations, 0);
    assert_eq!(policy.usage().decoded_bytes, 0);
    assert_eq!(process_image_usage().decoded_bytes, 0);
    assert!(policy.snapshot(&mut rows).unwrap().state_generation > before);
    for object in [7, 8] {
        let value = row(&policy, object);
        assert_eq!(value.state, ImageState::Unrequested);
        assert!(value.request.is_none() && value.published_target.is_none());
        assert_eq!(value.metadata, Some(metadata(128).into()));
        assert_eq!(value.failure.unwrap().kind, FailureKind::Released);
        policy.forget_variant(object).unwrap();
    }
}

#[test]
fn old_reused_drop_never_clears_a_recycled_row_or_new_reservation() {
    let _serial = SERIAL.lock().unwrap();
    let policy = make_policy();
    let old = publish(&policy, 1, target(1, 128.0, 1.0));
    policy.begin_source(epoch(2)).unwrap();
    for object in [7, 8] {
        policy
            .desire_variant(object, target(2, 128.0, 1.0))
            .unwrap();
        let request = policy.start_unknown(object).unwrap();
        policy
            .complete_reused_variant(request.key(), metadata(128).into(), &old)
            .unwrap();
    }
    assert_eq!(policy.forget_variant(7), Err(ResourceError::InvalidState));
    policy.evict_variant(7).unwrap();
    policy.forget_variant(7).unwrap();
    let replacement = publish(&policy, 7, target(2, 128.0, 1.0));
    let replacement_row = row(&policy, 7);
    drop(old);
    assert_eq!(row(&policy, 7), replacement_row);
    assert_eq!(row(&policy, 8).state, ImageState::Unrequested);
    assert_eq!(policy.usage().reservations, 1);
    assert_eq!(policy.usage().decoded_bytes, 128 * 128 * 4);
    policy.evict_variant(7).unwrap();
    assert_eq!(policy.forget_variant(7), Err(ResourceError::InvalidState));
    drop(replacement);
    policy.forget_variant(7).unwrap();
    assert_eq!(process_image_usage().decoded_bytes, 0);
}

#[test]
fn reuse_requires_current_target_same_authority_admitted_decoded_bytes() {
    let _serial = SERIAL.lock().unwrap();
    let policy = make_policy();
    let pixels = publish(&policy, 1, target(1, 128.0, 1.0));
    policy.begin_source(epoch(2)).unwrap();
    policy.desire_variant(7, target(2, 128.0, 1.0)).unwrap();
    let request = policy.start_unknown(7).unwrap();
    let before = row(&policy, 7);
    let foreign = make_policy();
    let foreign_pixels = publish(&foreign, 7, target(1, 128.0, 1.0));
    assert_eq!(
        policy.complete_reused_variant(request.key(), metadata(128).into(), &foreign_pixels),
        Err(ResourceError::WrongReservation)
    );
    let encoded = request.reserve_encoded(16).unwrap();
    encoded.admit_actual(16).unwrap();
    assert_eq!(
        policy.complete_reused_variant(request.key(), metadata(128).into(), &encoded),
        Err(ResourceError::WrongReservation)
    );
    let unadmitted = request.reserve_decoded(128 * 128 * 4).unwrap();
    assert_eq!(
        policy.complete_reused_variant(request.key(), metadata(128).into(), &unadmitted),
        Err(ResourceError::NotAdmitted)
    );
    assert_eq!(
        policy.complete_reused_variant(request.key(), metadata(256).into(), &pixels),
        Err(ResourceError::InvalidMetadata)
    );
    assert_eq!(row(&policy, 7), before);
    policy.desire_variant(7, target(3, 256.0, 1.0)).unwrap();
    assert_eq!(
        policy.complete_reused_variant(request.key(), metadata(128).into(), &pixels),
        Err(ResourceError::StaleResult)
    );
    drop((request, pixels, foreign_pixels, encoded, unadmitted));
    assert_eq!(process_image_usage().decoded_bytes, 0);
}
