// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE file in the project root for license information.
// ---------------------------------------------------------------------------------------------

use super::*;
use std::sync::Barrier;

#[test]
fn resource_reservation_shrinks_without_migration_growth_or_operation_checks() {
    let budget = SourceBudget::new(SourceLimits {
        auxiliary_capacity: 16,
        ..SourceLimits::default()
    })
    .unwrap();
    let other = budget.reserve_resource_capacity(4).unwrap();
    let mut result = budget.reserve_resource_capacity(12).unwrap();
    let cancel = CancellationToken::new();
    let mut operation = budget.begin(cancel.clone());
    cancel.cancel();
    assert_eq!(operation.checkpoint(), Err(CoreError::Cancelled));

    assert_eq!(result.shrink_to(13), Err(CoreError::InvalidRange));
    assert_eq!(result.reserved_bytes(), 12);
    assert_eq!(budget.usage().auxiliary_capacity, 16);
    result.shrink_to(5).unwrap();
    assert_eq!(result.reserved_bytes(), 5);
    assert_eq!(budget.usage().auxiliary_capacity, 9);
    let replacement = budget.reserve_resource_capacity(7).unwrap();
    assert_eq!(budget.usage().auxiliary_capacity, 16);
    result.shrink_to(5).unwrap();
    assert_eq!(budget.usage().auxiliary_capacity, 16);
    result.shrink_to(0).unwrap();
    assert_eq!(budget.usage().auxiliary_capacity, 11);
    drop(result);
    assert_eq!(budget.usage().auxiliary_capacity, 11);
    drop(other);
    drop(replacement);
    assert_eq!(budget.usage().auxiliary_capacity, 0);
    assert_eq!(operation.first_fault(), Some(CoreError::Cancelled));
}

#[test]
fn oversized_release_preserves_live_usage_and_the_lease() {
    let quota = Quota::new(8, LimitKind::AuxiliaryCapacity);
    let valid = quota.reserve(2).unwrap();
    let mut invalid = BoundaryReservation(Lease {
        quota: quota.clone(),
        amount: 4,
    });

    assert_eq!(invalid.shrink_to(1), Err(CoreError::InternalInvariant));
    assert_eq!(invalid.reserved_bytes(), 4);
    drop(invalid);
    assert_eq!(quota.live.load(Ordering::Acquire), 2);
    assert!(matches!(
        quota.reserve(7),
        Err(CoreError::ResourceLimit { .. })
    ));
    drop(valid);
    assert_eq!(quota.live.load(Ordering::Acquire), 0);
}

#[test]
fn cloned_token_controls_the_worker_flag_and_first_fault_survives() {
    let budget = SourceBudget::source_fixture();
    let cancel = CancellationToken::new();
    let mut op = budget.begin(cancel.clone());
    std::thread::scope(|scope| {
        scope.spawn(|| cancel.cancel()).join().unwrap();
    });
    assert_eq!(op.checkpoint(), Err(CoreError::Cancelled));

    let cancel = CancellationToken::new();
    let mut op = budget.begin(cancel.clone());
    let original = op.fail(CoreError::InvalidEditUtf8 {
        edit_index: 2,
        offset: 7,
    });
    cancel.cancel();
    assert_eq!(op.checkpoint(), Err(original));
    assert_eq!(op.scan(1), Err(original));
    assert_eq!(op.stats().expect("statistics enabled").total(), 0);
}

#[test]
fn stages_share_work_and_new_operations_share_capacity() {
    let budget = SourceBudget::new(SourceLimits {
        max_work: 4,
        retained_source_capacity: 8,
        ..SourceLimits::default()
    })
    .unwrap();
    let mut op = budget.begin(CancellationToken::new());
    op.scan(2).unwrap();
    op.copy(2).unwrap();
    assert_eq!(op.stats().expect("statistics enabled").total(), 4);
    assert_eq!(
        op.events(1),
        Err(CoreError::ResourceLimit {
            kind: LimitKind::Work,
            requested: 5,
            limit: 4,
        })
    );
    let lease = budget.reserve_source(8).unwrap();
    let mut next = budget.clone().begin(CancellationToken::new());
    next.events(1).unwrap();
    assert!(matches!(
        next.budget().reserve_source(1),
        Err(CoreError::ResourceLimit {
            kind: LimitKind::SourceCapacity,
            ..
        })
    ));
    drop(lease);
    assert_eq!(budget.usage().source_capacity, 0);
}

#[test]
fn concurrent_reservations_never_overadmit_and_release_on_last_owner() {
    let quota = Quota::new(8, LimitKind::AuxiliaryCapacity);
    let held = Arc::new(Barrier::new(9));
    let release = Arc::new(Barrier::new(9));
    std::thread::scope(|scope| {
        let mut workers = Vec::new();
        for _ in 0..8 {
            let quota = quota.clone();
            let held = held.clone();
            let release = release.clone();
            workers.push(scope.spawn(move || {
                let lease = quota.reserve(2);
                held.wait();
                release.wait();
                lease.is_ok()
            }));
        }
        held.wait();
        assert_eq!(quota.live.load(Ordering::Acquire), 8);
        assert_eq!(quota.peak.load(Ordering::Acquire), 8);
        release.wait();
        let accepted = workers
            .into_iter()
            .map(|worker| worker.join().unwrap())
            .filter(|accepted| *accepted)
            .count();
        assert_eq!(accepted, 4);
    });
    assert_eq!(quota.live.load(Ordering::Acquire), 0);
}

#[test]
fn capacity_topup_transfers_without_leaking_quota_arc() {
    let quota = Quota::new(8, LimitKind::AuxiliaryCapacity);
    let mut lease = quota.reserve(2).unwrap();
    assert_eq!(Arc::strong_count(&quota), 2);
    lease.admit_actual(8).unwrap();
    assert_eq!(Arc::strong_count(&quota), 2);
    assert_eq!(quota.live.load(Ordering::Acquire), 8);
    assert!(lease.admit_actual(9).is_err());
    assert_eq!(quota.live.load(Ordering::Acquire), 8);
    drop(lease);
    assert_eq!(quota.live.load(Ordering::Acquire), 0);
    assert_eq!(Arc::strong_count(&quota), 1);
}

#[test]
fn overflow_and_checkpoint_blocks_are_explicit() {
    let quota = Quota::new(u64::MAX, LimitKind::SourceCapacity);
    let lease = quota.reserve(u64::MAX).unwrap();
    assert!(matches!(
        quota.reserve(1),
        Err(CoreError::ArithmeticOverflow)
    ));
    drop(lease);
    let budget = SourceBudget::source_fixture();
    let mut op = budget.begin(CancellationToken::new());
    assert_eq!(
        op.scan(CHECKPOINT_BYTES + 1),
        Err(CoreError::InternalInvariant)
    );
    let mut op = budget.begin(CancellationToken::new());
    assert_eq!(
        op.events(CHECKPOINT_EVENTS + 1),
        Err(CoreError::InternalInvariant)
    );
    let budget = SourceBudget::new(SourceLimits {
        max_work: u64::MAX,
        ..SourceLimits::default()
    })
    .unwrap();
    let mut op = budget.begin(CancellationToken::new());
    // Represents a legal cumulative counter after prior bounded charges, without billions of calls.
    op.stats.as_mut().expect("statistics enabled").scanned_bytes = u64::MAX;
    assert_eq!(op.copy(1), Err(CoreError::ArithmeticOverflow));
    assert_eq!(op.stats().expect("statistics enabled").total(), u64::MAX);
}
#[test]
fn source_only_roots_do_not_enable_model_admission() {
    let budget = SourceBudget::source_fixture();
    let mut op = budget.begin(CancellationToken::new());
    assert_eq!(
        op.admit_model(LimitKind::ModelRecords, 1),
        Err(CoreError::UnsupportedConfiguration)
    );
    assert_eq!(op.first_fault(), Some(CoreError::UnsupportedConfiguration));
}

#[test]
fn model_admission_is_cumulative_and_depth_checks_the_current_value() {
    let budget = SourceBudget::new_with_model_limits(
        SourceLimits::default(),
        ModelLimits {
            model_records: 2,
            depth: 2,
            ..ModelLimits::default()
        },
    )
    .unwrap();
    let mut op = budget.begin(CancellationToken::new());
    op.admit_model(LimitKind::Depth, 2).unwrap();
    op.admit_model(LimitKind::Depth, 2).unwrap();
    op.admit_model(LimitKind::ModelRecords, 1).unwrap();
    op.admit_model(LimitKind::ModelRecords, 1).unwrap();
    assert_eq!(
        op.admit_model(LimitKind::ModelRecords, 1),
        Err(CoreError::ResourceLimit {
            kind: LimitKind::ModelRecords,
            requested: 3,
            limit: 2,
        })
    );
    let mut next = budget.begin(CancellationToken::new());
    assert_eq!(
        next.admit_model(LimitKind::Depth, 3),
        Err(CoreError::ResourceLimit {
            kind: LimitKind::Depth,
            requested: 3,
            limit: 2,
        })
    );
}

#[test]
fn model_id_limits_fit_the_shared_flat_indices() {
    assert!(matches!(
        SourceBudget::new_with_model_limits(
            SourceLimits::default(),
            ModelLimits {
                model_records: u64::from(u32::MAX) + 1,
                ..ModelLimits::default()
            }
        ),
        Err(CoreError::ArithmeticOverflow)
    ));
}

fn view_budget(max_work: u64) -> SourceBudget {
    SourceBudget::create(
        SourceLimits {
            max_work,
            ..SourceLimits::default()
        },
        Some(ModelLimits::default()),
    )
    .unwrap()
}

#[test]
fn fixed_views_keep_byte_diagnostics_and_bounded_admission_units() {
    let budget = SourceBudget::model_fixture();
    let mut op = budget.begin(CancellationToken::new());
    op.scan(7).unwrap();
    op.view_scan(56).unwrap();
    op.view_copy(88).unwrap();
    op.events(3).unwrap();
    op.copy(11).unwrap();
    let stats = op.stats().unwrap();
    assert_eq!(
        (stats.scanned_bytes(), stats.copied_bytes(), stats.events()),
        (63, 99, 3)
    );
    assert_eq!(stats.total(), 165);
    assert_eq!(stats.metadata_scanned_bytes(), 56);
    assert_eq!(stats.metadata_copied_bytes(), 88);
    assert_eq!(stats.metadata_actions(), 2);
    assert_eq!(stats.charged_units(), 23);
}

#[test]
fn view_policy_exact_quota_failure_is_atomic_and_stages_stay_cumulative() {
    let budget = view_budget(4);
    let mut op = budget.begin(CancellationToken::new());
    op.view_scan(512).unwrap();
    op.view_copy(512).unwrap();
    op.events(2).unwrap();
    let before = op.stats().expect("statistics enabled");
    assert_eq!(before.charged_units(), 4);
    assert_eq!(before.total(), 1026);
    let expected = CoreError::ResourceLimit {
        kind: LimitKind::Work,
        requested: 5,
        limit: 4,
    };
    assert_eq!(op.scan(1), Err(expected));
    assert_eq!(op.stats().expect("statistics enabled"), before);
    assert_eq!(op.view_copy(1), Err(expected));
    assert_eq!(op.stats().expect("statistics enabled"), before);
}

#[test]
fn view_zero_width_and_oversize_obey_cancel_and_existing_first_fault() {
    let budget = view_budget(0);
    let mut op = budget.begin(CancellationToken::new());
    let before = op.stats().expect("statistics enabled");
    op.view_scan(0).unwrap();
    op.view_copy(0).unwrap();
    assert_eq!(op.stats().expect("statistics enabled"), before);
    assert_eq!(op.view_scan(513), Err(CoreError::InternalInvariant));
    assert_eq!(op.stats().expect("statistics enabled"), before);
    let cancel = CancellationToken::new();
    cancel.cancel();
    let mut op = budget.begin(cancel);
    let before = op.stats().expect("statistics enabled");
    assert_eq!(op.view_copy(513), Err(CoreError::Cancelled));
    assert_eq!(op.stats().expect("statistics enabled"), before);
    assert_eq!(op.view_scan(0), Err(CoreError::Cancelled));
    assert_eq!(op.stats().expect("statistics enabled"), before);
    let mut op = budget.begin(CancellationToken::new());
    op.fail(CoreError::InvalidProvenance);
    assert_eq!(op.view_scan(513), Err(CoreError::InvalidProvenance));
    assert_eq!(op.stats().expect("statistics enabled"), before);
}

#[test]
fn view_policy_does_not_discount_actual_bytes_or_external_writes() {
    let budget = view_budget(8);
    let mut op = budget.begin(CancellationToken::new());
    op.external_read(3).unwrap();
    op.external_write(4).unwrap();
    op.external_events(1).unwrap();
    let before = op.stats().expect("statistics enabled");
    assert_eq!(before.charged_units(), 8);
    assert_eq!(before.total(), 8);
    assert_eq!(before.metadata_actions(), 0);
    assert_eq!(
        op.external_write(1),
        Err(CoreError::ResourceLimit {
            kind: LimitKind::Work,
            requested: 9,
            limit: 8,
        })
    );
    assert_eq!(op.stats().expect("statistics enabled"), before);
}

#[test]
fn view_policy_overflow_keeps_all_statistics_unchanged() {
    let budget = view_budget(u64::MAX);
    // Inject cumulative edge states without requiring billions of bounded operations.
    let mut op = budget.begin(CancellationToken::new());
    op.stats.as_mut().expect("statistics enabled").scanned_bytes = u64::MAX;
    op.stats
        .as_mut()
        .expect("statistics enabled")
        .metadata_scanned_bytes = u64::MAX;
    op.stats
        .as_mut()
        .expect("statistics enabled")
        .metadata_actions = u64::MAX;
    op.stats.as_mut().expect("statistics enabled").charged_units = u64::MAX;
    let before = op.stats().expect("statistics enabled");
    assert_eq!(op.copy(1), Err(CoreError::ArithmeticOverflow));
    assert_eq!(op.stats().expect("statistics enabled"), before);
    assert_eq!(op.stats().expect("statistics enabled").total(), u64::MAX);
    let mut op = budget.begin(CancellationToken::new());
    op.stats.as_mut().expect("statistics enabled").copied_bytes = u64::MAX;
    op.stats.as_mut().expect("statistics enabled").charged_units = u64::MAX;
    let before = op.stats().expect("statistics enabled");
    assert_eq!(op.view_copy(1), Err(CoreError::ArithmeticOverflow));
    assert_eq!(op.stats().expect("statistics enabled"), before);
    let mut op = budget.begin(CancellationToken::new());
    op.stats.as_mut().expect("statistics enabled").events = u64::MAX;
    op.stats.as_mut().expect("statistics enabled").charged_units = u64::MAX;
    let before = op.stats().expect("statistics enabled");
    assert_eq!(op.events(1), Err(CoreError::ArithmeticOverflow));
    assert_eq!(op.stats().expect("statistics enabled"), before);
}

#[test]
fn view_policy_root_identity_cannot_be_substituted_by_same_revision() {
    let budget = view_budget(8);
    let cloned = budget.clone();
    let foreign = view_budget(8);
    let mut op = budget.begin(CancellationToken::new());
    op.ensure_budget(&cloned).unwrap();
    op.view_scan(512).unwrap();
    let before = op.stats().expect("statistics enabled");
    assert_eq!(op.ensure_budget(&foreign), Err(CoreError::WrongBudget));
    assert_eq!(op.view_copy(512), Err(CoreError::WrongBudget));
    assert_eq!(op.stats().expect("statistics enabled"), before);
}
#[test]
fn review_regression_chunked_scans_keep_work_limits_and_empty_cancellation() {
    let budget = SourceBudget::new(SourceLimits {
        max_work: CHECKPOINT_BYTES + 1,
        ..SourceLimits::default()
    })
    .unwrap();
    let mut op = budget.begin(CancellationToken::new());
    let error = op.scan_chunked(2 * CHECKPOINT_BYTES + 1).unwrap_err();
    assert!(matches!(
        error,
        CoreError::ResourceLimit {
            kind: LimitKind::Work,
            ..
        }
    ));
    assert_eq!(op.first_fault(), Some(error));
    assert_eq!(op.stats().unwrap().scanned_bytes(), CHECKPOINT_BYTES);

    let cancel = CancellationToken::new();
    cancel.cancel();
    let mut op = budget.begin(cancel);
    assert_eq!(op.scan_chunked(0), Err(CoreError::Cancelled));
}
