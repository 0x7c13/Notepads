// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE file in the project root for license information.
// ---------------------------------------------------------------------------------------------

use super::*;

fn epoch(source: u64, provider: u64) -> SourceEpoch {
    SourceEpoch {
        source: SourceKey::new([1; 16], source).unwrap(),
        source_generation: source,
        provider_generation: provider,
    }
}

fn make_policy() -> ImageResourcePolicy {
    let policy = ImageResourcePolicy::new();
    policy.begin_source(epoch(1, 1)).unwrap();
    policy
}

fn start(policy: &ImageResourcePolicy, object: u32) -> RequestPermit {
    policy.desire_variant(object, target()).unwrap();
    policy.start(object).unwrap()
}

fn target() -> ImageTarget {
    ImageTarget {
        font_generation: 1,
        generation: 1,
        available_width_dip: 4.0,
        effective_physical_scale: 1.0,
    }
}

fn metadata() -> ImageMetadata {
    ImageMetadata {
        intrinsic_width: 4,
        intrinsic_height: 3,
        decoded_width: 4,
        decoded_height: 3,
        orientation: 1,
        stride: 16,
    }
}

fn snapshot(policy: &ImageResourcePolicy) -> (SnapshotHeader, Vec<EntrySnapshot>) {
    let mut rows = vec![EntrySnapshot::default(); MAX_ENTRIES];
    let header = policy.snapshot(&mut rows).unwrap();
    rows.truncate(header.written);
    (header, rows)
}

#[test]
fn source_and_provider_replacement_do_not_reopen_old_active_slots() {
    let policy = make_policy();
    let first = start(&policy, 0);
    let second = start(&policy, 1);
    for generation in 2..=20 {
        policy.begin_source(epoch(generation, generation)).unwrap();
        policy.desire_variant(0, target()).unwrap();
        let before = snapshot(&policy);
        assert!(matches!(
            policy.start(0),
            Err(ResourceError::Limit {
                kind: ResourceLimit::ActiveRequests,
                ..
            })
        ));
        assert_eq!(snapshot(&policy), before);
        assert_eq!(
            first.reserve_encoded(1).unwrap_err(),
            ResourceError::StaleResult
        );
        assert_eq!(
            second.reserve_decoded(1).unwrap_err(),
            ResourceError::StaleResult
        );
    }
    drop(first);
    let third = policy.start(0).unwrap();
    assert!(third.key().request > second.key().request);
    assert_eq!(policy.usage().active_requests, 2);
    drop((second, third));
    assert_eq!(policy.usage(), ResourceUsage::default());
}

#[test]
fn old_and_new_pixels_share_one_total_until_actual_last_owner_release() {
    let policy = make_policy();
    let old = start(&policy, 1);
    let pixels = old.reserve_decoded(MAX_DECODED_BYTES - 16).unwrap();
    pixels.admit_actual(MAX_DECODED_BYTES - 16).unwrap();
    let old_clone = pixels.clone();
    policy.begin_source(epoch(2, 1)).unwrap();
    let new = start(&policy, 1);
    let current = new.reserve_decoded(16).unwrap();
    current.admit_actual(16).unwrap();
    assert_eq!(policy.usage().decoded_bytes, MAX_DECODED_BYTES);
    assert!(matches!(
        new.reserve_decoded(1),
        Err(ResourceError::Limit {
            kind: ResourceLimit::DecodedBytes,
            ..
        })
    ));
    drop(pixels);
    assert_eq!(policy.usage().decoded_bytes, MAX_DECODED_BYTES);
    drop(old_clone);
    assert_eq!(policy.usage().decoded_bytes, 16);
    drop((current, old, new));
    assert_eq!(policy.usage(), ResourceUsage::default());
}

#[test]
fn encoded_growth_double_buffers_are_both_charged_and_admit_is_atomic() {
    let policy = make_policy();
    let request = start(&policy, 1);
    let old = request.reserve_encoded(MAX_ENCODED_IMAGE_BYTES).unwrap();
    let new = request.reserve_encoded(MAX_ENCODED_IMAGE_BYTES).unwrap();
    assert_eq!(policy.usage().encoded_bytes, MAX_ENCODED_BYTES);
    let before = policy.usage();
    assert!(request.reserve_encoded(1).is_err());
    assert_eq!(policy.usage(), before);
    assert!(old.admit_actual(MAX_ENCODED_IMAGE_BYTES + 1).is_err());
    assert_eq!(old.actual_bytes(), None);
    assert_eq!(policy.usage(), before);
    old.admit_actual(4).unwrap();
    assert_eq!(policy.usage().encoded_bytes, MAX_ENCODED_IMAGE_BYTES + 4);
    assert_eq!(old.admit_actual(3), Err(ResourceError::InvalidState));
    new.admit_actual(32).unwrap();
    assert_eq!(policy.usage().encoded_bytes, 36);
    drop((old, new, request));
    assert_eq!(policy.usage(), ResourceUsage::default());
}

#[test]
fn pending_permit_drop_is_cancelled_but_completed_drop_keeps_terminal_state() {
    let policy = make_policy();
    let pending = start(&policy, 1);
    let key = pending.key();
    let before = snapshot(&policy).0.state_generation;
    drop(pending);
    let (header, rows) = snapshot(&policy);
    assert!(header.state_generation > before);
    assert_eq!(rows[0].state, ImageState::Failed);
    assert_eq!(rows[0].failure.unwrap().kind, FailureKind::Cancelled);
    assert_eq!(
        policy.complete_failure(
            key,
            ImageFailure {
                kind: FailureKind::Decode,
                code: -1
            }
        ),
        Err(ResourceError::InvalidState)
    );
    let completed = start(&policy, 2);
    policy
        .complete_failure(
            completed.key(),
            ImageFailure {
                kind: FailureKind::Unsupported,
                code: -7,
            },
        )
        .unwrap();
    let before = snapshot(&policy);
    drop(completed);
    let after = snapshot(&policy);
    assert_eq!(after.0.state_generation, before.0.state_generation);
    assert_eq!(after.1, before.1);
    assert_eq!(after.1[1].failure.unwrap().kind, FailureKind::Unsupported);
}

#[test]
fn unknown_duplicate_and_stale_completions_are_non_mutating() {
    let policy = make_policy();
    let request = start(&policy, 1);
    let key = request.key();
    let failure = ImageFailure {
        kind: FailureKind::Provider,
        code: -15,
    };
    let mut unknown = key;
    unknown.object = 999;
    let before = snapshot(&policy);
    assert_eq!(
        policy.complete_failure(unknown, failure),
        Err(ResourceError::UnknownObject)
    );
    assert_eq!(snapshot(&policy), before);
    let mut stale = key;
    stale.request += 1;
    assert_eq!(
        policy.complete_failure(stale, failure),
        Err(ResourceError::StaleResult)
    );
    assert_eq!(snapshot(&policy), before);
    policy.complete_failure(key, failure).unwrap();
    let completed = snapshot(&policy);
    assert_eq!(
        policy.complete_failure(key, failure),
        Err(ResourceError::InvalidState)
    );
    assert_eq!(snapshot(&policy), completed);
    policy.begin_source(epoch(2, 1)).unwrap();
    let changed = snapshot(&policy);
    assert_eq!(
        policy.complete_failure(key, failure),
        Err(ResourceError::StaleResult)
    );
    assert_eq!(snapshot(&policy), changed);
}

#[test]
fn ready_snapshot_uses_admitted_matching_pixels_and_preserves_intrinsic_dimensions() {
    let policy = make_policy();
    let request = start(&policy, 1);
    let encoded = request.reserve_encoded(48).unwrap();
    encoded.admit_actual(48).unwrap();
    let decoded = request.reserve_decoded(48).unwrap();
    let before = snapshot(&policy);
    assert_eq!(
        policy.complete_ready(request.key(), metadata(), &encoded),
        Err(ResourceError::WrongReservation)
    );
    assert_eq!(
        policy.complete_ready(request.key(), metadata(), &decoded),
        Err(ResourceError::NotAdmitted)
    );
    assert_eq!(snapshot(&policy), before);
    decoded.admit_actual(48).unwrap();
    let mut image = metadata();
    image.orientation = 6;
    image.decoded_width = 3;
    image.decoded_height = 4;
    image.stride = 12;
    policy
        .complete_ready(request.key(), image, &decoded)
        .unwrap();
    let (header, rows) = snapshot(&policy);
    assert_eq!(rows[0].state, ImageState::Ready);
    assert_eq!(rows[0].metadata, Some(image.into()));
    assert_eq!(
        rows[0].metadata.unwrap().required_capacity(target()),
        Ok(48)
    );
    assert!(rows[0].target_satisfied());
    assert_eq!(rows[0].desired_target, Some(target()));
    assert_eq!(rows[0].published_target, Some(target()));
    assert_eq!(policy.register(1).unwrap(), rows[0]);
    assert_eq!(
        snapshot(&policy).0.state_generation,
        header.state_generation
    );
    assert_eq!(policy.start(1).unwrap_err(), ResourceError::InvalidState);
    assert_eq!(
        policy.complete_ready(request.key(), image, &decoded),
        Err(ResourceError::InvalidState)
    );
    drop((encoded, request));
    assert_eq!(snapshot(&policy).1[0].state, ImageState::Ready);
    let clone = decoded.clone();
    drop(decoded);
    assert_eq!(snapshot(&policy).1[0].state, ImageState::Ready);
    drop(clone);
    let (released, rows) = snapshot(&policy);
    assert!(released.state_generation > header.state_generation);
    assert_eq!(rows[0].state, ImageState::Unrequested);
    assert_eq!(rows[0].metadata, Some(image.into()));
    assert_eq!(rows[0].desired_target, Some(target()));
    assert_eq!(rows[0].published_target, None);
    assert!(!rows[0].target_satisfied());
    assert_eq!(rows[0].failure.unwrap().kind, FailureKind::Released);
    assert_eq!(policy.usage(), ResourceUsage::default());
}

#[test]
fn invalid_metadata_and_wrong_root_reservations_do_not_publish() {
    let policy = make_policy();
    let request = start(&policy, 1);
    let pixels = request.reserve_decoded(47).unwrap();
    pixels.admit_actual(47).unwrap();
    assert_eq!(
        policy.complete_ready(request.key(), metadata(), &pixels),
        Err(ResourceError::InvalidMetadata)
    );
    let other = make_policy();
    let foreign_request = start(&other, 1);
    let foreign = foreign_request.reserve_decoded(48).unwrap();
    foreign.admit_actual(48).unwrap();
    assert_eq!(request.key(), foreign_request.key());
    assert!(!foreign.is_same_request(&request));
    assert_eq!(
        policy.complete_ready(request.key(), metadata(), &foreign),
        Err(ResourceError::WrongReservation)
    );
    let mut invalid = metadata();
    invalid.orientation = 9;
    assert_eq!(
        invalid.required_capacity(),
        Err(ResourceError::InvalidMetadata)
    );
    invalid = metadata();
    invalid.stride = 15;
    assert_eq!(
        invalid.required_capacity(),
        Err(ResourceError::InvalidMetadata)
    );
    invalid = metadata();
    invalid.intrinsic_width = 8192;
    invalid.intrinsic_height = 8192;
    assert_eq!(
        invalid.required_capacity(),
        Err(ResourceError::InvalidMetadata)
    );
    invalid = metadata();
    invalid.decoded_width = 5;
    assert_eq!(
        invalid.required_capacity(),
        Err(ResourceError::InvalidMetadata)
    );
    invalid = metadata();
    invalid.intrinsic_width = 0;
    assert_eq!(
        invalid.required_capacity(),
        Err(ResourceError::InvalidMetadata)
    );
    assert_eq!(snapshot(&policy).1[0].state, ImageState::Pending);
}

#[test]
fn rejected_epoch_and_registration_limit_leave_state_unchanged() {
    let policy = make_policy();
    let request = start(&policy, 0);
    let before = snapshot(&policy);
    assert_eq!(
        policy.begin_source(epoch(1, 1)),
        Err(ResourceError::InvalidEpoch)
    );
    let mut wrong = epoch(1, 2);
    wrong.source = SourceKey::new([2; 16], 1).unwrap();
    assert_eq!(policy.begin_source(wrong), Err(ResourceError::InvalidEpoch));
    wrong = epoch(2, 0);
    assert_eq!(policy.begin_source(wrong), Err(ResourceError::InvalidEpoch));
    assert_eq!(snapshot(&policy), before);
    assert_eq!(policy.register(0).unwrap(), before.1[0]);
    for object in 1..MAX_ENTRIES as u32 {
        policy.register(object).unwrap();
    }
    let full = snapshot(&policy);
    assert!(matches!(
        policy.register(MAX_ENTRIES as u32),
        Err(ResourceError::Limit {
            kind: ResourceLimit::Entries,
            ..
        })
    ));
    assert_eq!(snapshot(&policy), full);
    let mut sentinel = [EntrySnapshot {
        object: 777,
        ..EntrySnapshot::default()
    }; 2];
    let original = sentinel;
    assert_eq!(
        policy.snapshot(&mut sentinel),
        Err(ResourceError::SnapshotTooSmall {
            required: MAX_ENTRIES
        })
    );
    assert_eq!(sentinel, original);
    policy.begin_source(epoch(1, 2)).unwrap();
    assert_eq!(policy.usage().active_requests, 1);
    assert_eq!(
        request.reserve_encoded(1).unwrap_err(),
        ResourceError::StaleResult
    );
    drop(request);
    assert_eq!(policy.usage(), ResourceUsage::default());
}

#[test]
fn last_owners_can_outlive_policy_and_release_on_another_thread() {
    let policy = make_policy();
    let observer = Arc::downgrade(&policy.shared);
    let request = start(&policy, 1);
    let bytes = request.reserve_decoded(48).unwrap();
    bytes.admit_actual(48).unwrap();
    let keeper = bytes.clone();
    drop((bytes, policy));
    assert_eq!(observer.upgrade().unwrap().lock().usage.decoded_bytes, 48);
    std::thread::spawn(move || {
        drop(request);
        assert_eq!(keeper.owner.shared.lock().usage.active_requests, 0);
        drop(keeper);
    })
    .join()
    .unwrap();
    assert!(observer.upgrade().is_none());
}

#[test]
fn global_reservation_control_storage_is_bounded_across_source_changes() {
    let policy = make_policy();
    assert!(std::mem::size_of::<State>() <= 256);
    assert_eq!(policy.shared.lock().entries.len(), MAX_ENTRIES);
    assert!(policy.table_capacity_bytes() >= ImageResourcePolicy::table_request_bytes());
    println!(
        "resource policy: State={} Shared={} Entry={} table request={} capacity={} bytes",
        std::mem::size_of::<State>(),
        std::mem::size_of::<Shared>(),
        std::mem::size_of::<Option<Entry>>(),
        ImageResourcePolicy::table_request_bytes(),
        policy.table_capacity_bytes()
    );
    let first = start(&policy, 1);
    let mut reservations = Vec::new();
    for _ in 0..MAX_RESERVATIONS {
        reservations.push(first.reserve_decoded(1).unwrap());
    }
    assert_eq!(policy.usage().reservations, MAX_RESERVATIONS);
    assert_eq!(policy.usage().decoded_bytes, u64::from(MAX_RESERVATIONS));
    policy.begin_source(epoch(2, 1)).unwrap();
    assert_eq!(policy.shared.lock().entries.len(), MAX_ENTRIES);
    let second = start(&policy, 1);
    assert!(matches!(
        second.reserve_decoded(1),
        Err(ResourceError::Limit {
            kind: ResourceLimit::Reservations,
            ..
        })
    ));
    drop(reservations.pop());
    let replacement = second.reserve_decoded(1).unwrap();
    drop((replacement, reservations, first, second));
    assert_eq!(policy.usage(), ResourceUsage::default());
}

#[test]
fn checked_identifiers_and_admission_overflow_never_partially_mutate() {
    let policy = make_policy();
    policy.desire_variant(1, target()).unwrap();
    policy.shared.lock().last_request = u64::MAX;
    let before = snapshot(&policy);
    assert_eq!(
        policy.start(1).unwrap_err(),
        ResourceError::ArithmeticOverflow
    );
    assert_eq!(snapshot(&policy), before);
    policy.shared.lock().last_request = 0;
    let request = policy.start(1).unwrap();
    policy.shared.lock().last_reservation = u64::MAX;
    let before = policy.usage();
    assert_eq!(
        request.reserve_decoded(1).unwrap_err(),
        ResourceError::ArithmeticOverflow
    );
    assert_eq!(policy.usage(), before);
    policy.shared.lock().last_reservation = 0;
    let first = request.reserve_decoded(1).unwrap();
    let second = request.reserve_decoded(1).unwrap();
    let before = policy.usage();
    assert_eq!(
        first.admit_actual(u64::MAX),
        Err(ResourceError::ArithmeticOverflow)
    );
    assert_eq!(policy.usage(), before);
    assert_eq!(first.actual_bytes(), None);
    first.admit_actual(1).unwrap();
    assert!(matches!(
        second.admit_actual(MAX_DECODED_BYTES),
        Err(ResourceError::Limit {
            kind: ResourceLimit::DecodedBytes,
            ..
        })
    ));
    assert_eq!(second.actual_bytes(), None);
    drop((first, second, request));
    assert_eq!(policy.usage(), ResourceUsage::default());
}

#[test]
fn state_generation_exhaustion_rejects_publication_but_drop_still_returns_gauges() {
    let policy = make_policy();
    let request = start(&policy, 1);
    let pixels = request.reserve_decoded(48).unwrap();
    pixels.admit_actual(48).unwrap();
    policy
        .complete_ready(request.key(), metadata(), &pixels)
        .unwrap();
    policy.shared.lock().generation = u64::MAX;
    let before = snapshot(&policy);
    assert_eq!(policy.register(2), Err(ResourceError::ArithmeticOverflow));
    assert_eq!(snapshot(&policy), before);
    drop((pixels, request));
    assert_eq!(policy.usage(), ResourceUsage::default());
    let mut rows = [EntrySnapshot::default(); MAX_ENTRIES];
    assert_eq!(
        policy.snapshot(&mut rows),
        Err(ResourceError::ArithmeticOverflow)
    );
}
