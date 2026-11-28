// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE file in the project root for license information.
// ---------------------------------------------------------------------------------------------

use super::*;

fn epoch(source: u64, provider: u64) -> SourceEpoch {
    SourceEpoch {
        source: SourceKey::new([17; 16], source).unwrap(),
        source_generation: source,
        provider_generation: provider,
    }
}
fn target(generation: u64, scale: f64) -> ImageTarget {
    ImageTarget {
        font_generation: 1,
        generation,
        available_width_dip: 64.0,
        effective_physical_scale: scale,
    }
}
fn policy() -> ImageResourcePolicy {
    let policy = ImageResourcePolicy::new();
    policy.begin_source(epoch(1, 1)).unwrap();
    policy
}
fn row(policy: &ImageResourcePolicy) -> EntrySnapshot {
    let mut rows = [EntrySnapshot::default(); MAX_ENTRIES];
    policy.snapshot(&mut rows).unwrap();
    rows[0]
}
fn metadata(scale: u32) -> SvgImageMetadata {
    SvgImageMetadata {
        natural_width_dip: 64.0,
        natural_height_dip: 32.0,
        decoded_width: 64 * scale,
        decoded_height: 32 * scale,
        stride: 256 * scale,
        source_width: f64::from(64 * scale),
        source_height: f64::from(32 * scale),
        viewport_width_bits: 64f32.to_bits(),
        viewport_height_bits: 32f32.to_bits(),
        scale_bits: (scale as f32).to_bits(),
        resolution_limited: false,
    }
}
fn publish(policy: &ImageResourcePolicy, request: &RequestPermit, scale: u32) -> ByteReservation {
    let pixels = request
        .reserve_decoded(u64::from(8192 * scale * scale))
        .unwrap();
    pixels
        .admit_actual(u64::from(8192 * scale * scale))
        .unwrap();
    policy
        .complete_variant_ready(request.key(), metadata(scale).into(), &pixels)
        .unwrap();
    pixels
}

#[test]
fn replacement_failure_keeps_published_pixels_and_blocks_implicit_retry() {
    let policy = policy();
    policy.desire_variant(1, target(1, 1.0)).unwrap();
    let first = policy.start_variant(1).unwrap();
    let old = publish(&policy, &first, 1);
    drop(first);
    let published = row(&policy);
    policy.desire_variant(1, target(2, 2.0)).unwrap();
    let candidate = policy.start_variant(1).unwrap();
    assert_eq!(row(&policy).state, ImageState::Ready);
    assert_eq!(row(&policy).request, published.request);
    let failure = ImageFailure {
        kind: FailureKind::Limited,
        code: -27,
    };
    policy
        .complete_variant_failure(candidate.key(), failure, (2 << 16) | 1)
        .unwrap();
    let failed = row(&policy);
    assert_eq!(failed.state, ImageState::Ready);
    assert_eq!(failed.metadata, published.metadata);
    assert_eq!(failed.failure, Some(failure));
    assert_eq!(failed.failure_detail, (2 << 16) | 1);
    assert!(policy.start_variant(1).is_err());
    drop(candidate);
    assert_eq!(row(&policy), failed);
    policy.desire_variant(1, target(3, 2.0)).unwrap();
    let next = policy.start_variant(1).unwrap();
    let current = publish(&policy, &next, 2);
    assert_eq!(policy.usage().decoded_bytes, 8192 + 32768);
    drop(old);
    assert_eq!(row(&policy).state, ImageState::Ready);
    assert_eq!(row(&policy).request, Some(next.key()));
    drop((current, next));
    assert_eq!(policy.usage(), ResourceUsage::default());
}

#[test]
fn released_published_pixels_clear_a_completed_replacement_failure() {
    let policy = policy();
    policy.desire_variant(1, target(1, 1.0)).unwrap();
    let first = policy.start_variant(1).unwrap();
    let pixels = publish(&policy, &first, 1);
    drop(first);
    policy.desire_variant(1, target(2, 2.0)).unwrap();
    let candidate = policy.start_variant(1).unwrap();
    assert!(matches!(
        candidate.reserve_decoded(MAX_DECODED_BYTES),
        Err(ResourceError::Limit {
            kind: ResourceLimit::DecodedBytes,
            ..
        })
    ));
    policy
        .complete_variant_failure(
            candidate.key(),
            ImageFailure {
                kind: FailureKind::Limited,
                code: -27,
            },
            (2 << 16) | 1,
        )
        .unwrap();
    drop(candidate);
    let failed = row(&policy);
    assert_eq!(failed.state, ImageState::Ready);
    assert_eq!(failed.failure_detail, (2 << 16) | 1);
    assert!(failed.capacity_wait);
    drop(pixels);
    let released = row(&policy);
    assert_eq!(released.state, ImageState::Unrequested);
    assert_eq!(released.request, None);
    assert_eq!(released.pending_request, None);
    assert_eq!(released.last_attempt_generation, 0);
    assert_eq!(
        released.failure,
        Some(ImageFailure {
            kind: FailureKind::Released,
            code: 0,
        })
    );
    assert_eq!(released.failure_detail, 0);
    assert!(!released.capacity_wait);
    assert!(
        policy.shared.lock().entries[0]
            .as_ref()
            .unwrap()
            .capacity_failure
            .is_none()
    );
    assert_eq!(policy.usage(), ResourceUsage::default());
}

#[test]
fn review_regression_released_ready_pixels_keep_candidate_identity_and_retry_gate() {
    let policy = policy();
    policy.desire_variant(1, target(1, 1.0)).unwrap();
    let first = policy.start_variant(1).unwrap();
    let pixels = publish(&policy, &first, 1);
    drop(first);
    policy.desire_variant(1, target(2, 2.0)).unwrap();
    let candidate = policy.start_variant(1).unwrap();
    drop(pixels);
    let pending = row(&policy);
    assert_eq!(pending.state, ImageState::Pending);
    assert_eq!(pending.request, Some(candidate.key()));
    assert_eq!(pending.pending_request, Some(candidate.key()));
    assert_eq!(pending.last_attempt_generation, 2);
    policy
        .complete_variant_failure(
            candidate.key(),
            ImageFailure {
                kind: FailureKind::Unavailable,
                code: -1,
            },
            0,
        )
        .unwrap();
    drop(candidate);
    assert_eq!(row(&policy).state, ImageState::Unavailable);
    assert!(matches!(
        policy.start_variant(1),
        Err(ResourceError::InvalidState)
    ));
    assert_eq!(policy.usage(), ResourceUsage::default());
}

#[test]
fn superseded_same_object_jobs_keep_two_slots_and_old_drop_cannot_clear_new() {
    let policy = policy();
    policy.desire_variant(1, target(1, 1.0)).unwrap();
    let old = policy.start_variant(1).unwrap();
    let old_bytes = old.reserve_encoded(13).unwrap();
    old_bytes.admit_actual(13).unwrap();
    policy.desire_variant(1, target(2, 2.0)).unwrap();
    let current = policy.start_variant(1).unwrap();
    assert_eq!(policy.usage().active_requests, 2);
    assert_eq!(
        old.reserve_decoded(1).unwrap_err(),
        ResourceError::StaleResult
    );
    policy.desire_variant(1, target(3, 3.0)).unwrap();
    let before = row(&policy);
    assert!(matches!(
        policy.start_variant(1),
        Err(ResourceError::Limit {
            kind: ResourceLimit::ActiveRequests,
            ..
        })
    ));
    assert_eq!(row(&policy), before);
    assert_eq!(before.last_attempt_generation, 2);
    drop(old);
    assert_eq!(policy.usage().active_requests, 1);
    let latest = policy.start_variant(1).unwrap();
    drop(current);
    assert_eq!(row(&policy).pending_request, Some(latest.key()));
    let pixels = publish(&policy, &latest, 3);
    drop((old_bytes, pixels, latest));
    assert_eq!(policy.usage(), ResourceUsage::default());
}

#[test]
fn smaller_or_same_bucket_targets_reuse_existing_variant() {
    let policy = policy();
    policy.desire_variant(1, target(1, 3.0)).unwrap();
    let request = policy.start_variant(1).unwrap();
    let pixels = publish(&policy, &request, 3);
    drop(request);
    for (generation, scale) in [(2, 1.0), (3, 2.1), (4, 2.99), (5, 3.0)] {
        policy.desire_variant(1, target(generation, scale)).unwrap();
        assert!(row(&policy).target_satisfied());
        assert!(policy.start_variant(1).is_err());
        assert_eq!(policy.usage().active_requests, 0);
    }
    policy.desire_variant(1, target(6, 3.01)).unwrap();
    assert!(!row(&policy).target_satisfied());
    assert!(policy.start_variant(1).is_ok());
    drop(pixels);
}

#[test]
fn font_source_and_provider_changes_withdraw_old_publication() {
    let policy = policy();
    policy.desire_variant(1, target(1, 1.0)).unwrap();
    let old = policy.start_variant(1).unwrap();
    let pixels = publish(&policy, &old, 1);
    let mut changed = target(2, 1.0);
    changed.font_generation = 2;
    policy.desire_variant(1, changed).unwrap();
    assert_eq!(row(&policy).metadata, None);
    assert_eq!(policy.usage().decoded_bytes, 8192);
    let new = policy.start_variant(1).unwrap();
    let before = row(&policy);
    drop((pixels, old));
    assert_eq!(row(&policy), before);
    policy.begin_source(epoch(1, 2)).unwrap();
    assert_eq!(
        new.reserve_encoded(1).unwrap_err(),
        ResourceError::StaleResult
    );
    policy.desire_variant(1, target(1, 1.0)).unwrap();
    let provider = policy.start_variant(1).unwrap();
    policy.begin_source(epoch(2, 2)).unwrap();
    assert_eq!(
        provider.reserve_encoded(1).unwrap_err(),
        ResourceError::StaleResult
    );
    drop((new, provider));
    assert_eq!(policy.usage(), ResourceUsage::default());
}

#[test]
fn invalid_or_nonmonotonic_desired_target_is_atomic() {
    let policy = policy();
    policy.desire_variant(1, target(1, 1.0)).unwrap();
    let before = row(&policy);
    for bad in [0.0, -1.0, f64::NAN, f64::INFINITY] {
        let mut value = target(2, bad);
        assert!(policy.desire_variant(1, value).is_err());
        value = target(2, 1.0);
        value.available_width_dip = bad;
        assert!(policy.desire_variant(2, value).is_err());
        assert_eq!(row(&policy), before);
    }
    assert!(policy.desire_variant(1, target(1, 2.0)).is_err());
    assert_eq!(row(&policy), before);
    policy.desire_variant(1, target(1, 1.0)).unwrap();
    assert_eq!(row(&policy), before);
}

#[test]
fn every_svg_geometry_field_and_target_is_validated_before_publication() {
    let policy = policy();
    let desired = target(1, 2.0);
    policy.desire_variant(1, desired).unwrap();
    let request = policy.start_variant(1).unwrap();
    let pixels = request.reserve_decoded(32768).unwrap();
    pixels.admit_actual(32768).unwrap();
    let before = row(&policy);
    let good = metadata(2);
    let mut cases = Vec::new();
    for value in [f64::NAN, f64::INFINITY, 0.0, -1.0, 64.1, 8193.0] {
        cases.push(SvgImageMetadata {
            natural_width_dip: value,
            ..good
        });
        cases.push(SvgImageMetadata {
            source_width: value,
            ..good
        });
    }
    cases.extend([
        SvgImageMetadata {
            decoded_width: 129,
            ..good
        },
        SvgImageMetadata {
            stride: 513,
            ..good
        },
        SvgImageMetadata {
            scale_bits: 1f32.to_bits(),
            ..good
        },
        SvgImageMetadata {
            viewport_height_bits: 33f32.to_bits(),
            ..good
        },
        SvgImageMetadata {
            resolution_limited: true,
            ..good
        },
    ]);
    for bad in cases {
        assert!(
            policy
                .complete_variant_ready(request.key(), bad.into(), &pixels)
                .is_err()
        );
        assert_eq!(row(&policy), before);
    }
    assert_eq!(good.required_capacity(desired), Ok(32768));
    policy
        .complete_variant_ready(request.key(), good.into(), &pixels)
        .unwrap();
}

#[test]
fn release_of_published_pixels_preserves_inflight_replacement_authority() {
    let policy = policy();
    policy.desire_variant(1, target(1, 1.0)).unwrap();
    let old = policy.start_variant(1).unwrap();
    let pixels = publish(&policy, &old, 1);
    drop(old);
    policy.desire_variant(1, target(2, 2.0)).unwrap();
    let new = policy.start_variant(1).unwrap();
    drop(pixels);
    assert_eq!(row(&policy).state, ImageState::Pending);
    assert_eq!(row(&policy).pending_request, Some(new.key()));
    let next = publish(&policy, &new, 2);
    assert_eq!(row(&policy).state, ImageState::Ready);
    drop((next, new));
    assert_eq!(policy.usage(), ResourceUsage::default());
}

#[test]
fn auto_classifies_raster_in_place_and_releases_to_metadata_only_for_replacement() {
    let policy = policy();
    policy.desire_variant(1, target(1, 2.0)).unwrap();
    let request = policy.start_unknown(1).unwrap();
    assert_eq!(request.target(), target(1, 2.0));
    let pixels = request.reserve_decoded(48).unwrap();
    pixels.admit_actual(48).unwrap();
    let raster = ImageMetadata {
        intrinsic_width: 4,
        intrinsic_height: 3,
        decoded_width: 3,
        decoded_height: 4,
        orientation: 6,
        stride: 12,
    };
    let mut invalid = raster;
    invalid.stride = 11;
    let before = row(&policy);
    assert!(
        policy
            .complete_variant_ready(request.key(), invalid.into(), &pixels)
            .is_err()
    );
    assert_eq!(row(&policy), before);
    policy
        .complete_variant_ready(request.key(), raster.into(), &pixels)
        .unwrap();
    assert_eq!(policy.usage().active_requests, 1);
    assert_eq!(row(&policy).desired_target, Some(target(1, 2.0)));
    assert_eq!(row(&policy).published_target, Some(target(1, 2.0)));
    assert_eq!(row(&policy).last_attempt_generation, 1);
    assert!(row(&policy).target_satisfied());
    assert_eq!(row(&policy).metadata, Some(raster.into()));
    drop((pixels, request));
    let released = row(&policy);
    assert_eq!(released.state, ImageState::Unrequested);
    assert_eq!(released.metadata, Some(raster.into()));
    assert_eq!(released.published_target, None);
    assert!(!released.target_satisfied());
    policy.desire_variant(1, target(2, 1.0)).unwrap();
    assert!(policy.start_unknown(1).is_err());
    let replacement = policy.start(1).unwrap();
    let replacement_pixels = replacement.reserve_decoded(48).unwrap();
    replacement_pixels.admit_actual(48).unwrap();
    policy
        .complete_ready(replacement.key(), raster, &replacement_pixels)
        .unwrap();
    assert!(row(&policy).target_satisfied());
}

#[test]
fn auto_svg_classification_survives_pixel_release_and_font_withdrawal() {
    let policy = policy();
    policy.desire_variant(1, target(1, 1.0)).unwrap();
    let request = policy.start_unknown(1).unwrap();
    let pixels = publish(&policy, &request, 1);
    drop((pixels, request));
    policy.desire_variant(1, target(2, 2.0)).unwrap();
    let before = row(&policy);
    assert!(policy.start_unknown(1).is_err());
    assert_eq!(row(&policy), before);
    let replacement = policy.start_variant(1).unwrap();
    let next = publish(&policy, &replacement, 2);
    drop(replacement);
    let mut font = target(3, 2.0);
    font.font_generation = 2;
    policy.desire_variant(1, font).unwrap();
    assert!(row(&policy).metadata.is_none());
    assert!(policy.start_unknown(1).is_err());
    assert!(policy.start_variant(1).is_ok());
    drop(next);
}

#[test]
fn auto_stale_completion_does_not_classify_or_clear_the_latest_candidate() {
    let policy = policy();
    policy.desire_variant(1, target(1, 1.0)).unwrap();
    let old = policy.start_unknown(1).unwrap();
    let pixels = old.reserve_decoded(8192).unwrap();
    pixels.admit_actual(8192).unwrap();
    policy.desire_variant(1, target(2, 2.0)).unwrap();
    let current = policy.start_unknown(1).unwrap();
    let before = row(&policy);
    assert_eq!(
        policy.complete_variant_ready(old.key(), metadata(1).into(), &pixels),
        Err(ResourceError::StaleResult)
    );
    assert_eq!(row(&policy), before);
    drop((old, pixels));
    let next = publish(&policy, &current, 2);
    drop((current, next));
    assert_eq!(policy.usage(), ResourceUsage::default());
}

#[test]
fn completed_source_failures_do_not_reopen_for_geometry() {
    for (kind, detail) in [
        (FailureKind::Unavailable, 0),
        (FailureKind::Unsupported, 0),
        (FailureKind::Decode, 5 << 16),
    ] {
        let policy = policy();
        policy.desire_variant(1, target(1, 1.0)).unwrap();
        let request = policy.start_unknown(1).unwrap();
        policy
            .complete_variant_failure(request.key(), ImageFailure { kind, code: -9 }, detail)
            .unwrap();
        drop(request);
        let before = row(&policy);
        let generation = policy
            .snapshot(&mut [EntrySnapshot::default(); MAX_ENTRIES])
            .unwrap();
        let mut resized = target(2, 2.0);
        resized.available_width_dip = 32.0;
        policy.desire_variant(1, resized).unwrap();
        assert_eq!(row(&policy), before, "{kind:?}");
        assert_eq!(
            policy
                .snapshot(&mut [EntrySnapshot::default(); MAX_ENTRIES])
                .unwrap(),
            generation
        );
        assert!(policy.start_unknown(1).is_err());
        assert_eq!(policy.usage(), ResourceUsage::default());
        assert!(policy.desire_variant(1, target(0, 1.0)).is_err());
        assert!(policy.desire_variant(1, target(1, 2.0)).is_err());
        policy.begin_source(epoch(1, 2)).unwrap();
        policy.desire_variant(1, resized).unwrap();
        assert!(
            policy.start_unknown(1).is_ok(),
            "new provider must permit IO"
        );
    }
}

#[test]
fn geometry_retry_keeps_transient_and_target_failures_retryable() {
    for (kind, detail) in [
        (FailureKind::Provider, 0),
        (FailureKind::Cancelled, 0),
        (FailureKind::Limited, (2 << 16) | 1),
        (FailureKind::Released, 0),
        (FailureKind::Decode, 0),
        (FailureKind::Decode, (5 << 16) | 1),
        (FailureKind::Unsupported, 4 << 16),
    ] {
        let policy = policy();
        policy.desire_variant(1, target(1, 1.0)).unwrap();
        let request = policy.start_unknown(1).unwrap();
        policy
            .complete_variant_failure(request.key(), ImageFailure { kind, code: -9 }, detail)
            .unwrap();
        drop(request);
        policy.desire_variant(1, target(2, 2.0)).unwrap();
        assert_eq!(row(&policy).state, ImageState::Unrequested);
        assert!(policy.start_unknown(1).is_ok(), "{kind:?}/{detail}");
    }
}

#[test]
fn font_retry_can_coalesce_twice_before_starting_despite_old_decode_failure() {
    let policy = policy();
    policy.desire_variant(1, target(1, 1.0)).unwrap();
    let request = policy.start_unknown(1).unwrap();
    policy
        .complete_variant_failure(
            request.key(),
            ImageFailure {
                kind: FailureKind::Decode,
                code: -9,
            },
            5 << 16,
        )
        .unwrap();
    drop(request);
    let mut changed_font = target(2, 1.0);
    changed_font.font_generation = 2;
    policy.desire_variant(1, changed_font).unwrap();
    assert_eq!(row(&policy).state, ImageState::Unrequested);
    changed_font.generation = 3;
    changed_font.available_width_dip = 32.0;
    policy.desire_variant(1, changed_font).unwrap();
    assert_eq!(row(&policy).desired_target, Some(changed_font));
    assert_eq!(row(&policy).last_attempt_generation, 1);
    assert!(policy.start_unknown(1).is_ok());
}
