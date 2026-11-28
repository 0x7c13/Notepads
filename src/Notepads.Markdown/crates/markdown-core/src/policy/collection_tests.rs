// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE file in the project root for license information.
// ---------------------------------------------------------------------------------------------

use super::*;

#[test]
fn quota_fixtures_collect_and_task_access_keeps_its_protection() {
    for budget in [
        SourceBudget::source_fixture(),
        SourceBudget::model_fixture(),
    ] {
        assert_eq!(budget.work_stats_collection(), WorkStatsCollection::Enabled);
        let stats = budget.begin(CancellationToken::new()).stats().unwrap();
        assert_eq!(stats.total(), 0);
        assert_eq!(stats.charged_units(), 0);
        assert_eq!(budget.elapsed_limit_ns(), None);
    }
    let budget = SourceBudget::task_fixture();
    assert_eq!(
        budget.work_stats_collection(),
        WorkStatsCollection::Disabled
    );
    assert_eq!(budget.active_work_limit(), None);
    assert_eq!(budget.elapsed_limit_ns(), Some(ELAPSED_LIMIT_NS));
    assert_eq!(budget.begin(CancellationToken::new()).stats(), None);
}

#[test]
fn disabled_collection_cannot_create_a_cumulative_admission_root() {
    for model_limits in [None, Some(ModelLimits::default())] {
        assert!(matches!(
            SourceBudget::create_with_admission(
                SourceLimits::default(),
                model_limits,
                ResourceAdmissionMode::CumulativeWork,
                WorkStatsCollection::Disabled,
            ),
            Err(CoreError::UnsupportedConfiguration)
        ));
    }
}

#[test]
fn disabled_collection_stays_absent_through_charges_and_a_latched_first_fault() {
    let budget = SourceBudget::task_fixture();
    let cancel = CancellationToken::new();
    let mut op = budget.begin(cancel.clone());
    op.set_clock_samples_for_test(&[100_000_000]);
    assert_eq!(op.stats(), None);
    op.external_read(3).unwrap();
    assert_eq!(op.stats(), None);
    op.external_write(4).unwrap();
    assert_eq!(op.stats(), None);
    op.external_events(1).unwrap();
    assert_eq!(op.stats(), None);
    op.view_scan(512).unwrap();
    assert_eq!(op.stats(), None);
    op.view_copy(88).unwrap();
    assert_eq!(op.stats(), None);
    op.scan(0).unwrap();
    op.events(0).unwrap();
    op.checkpoint_now().unwrap();
    assert_eq!(op.stats(), None);

    let original = CoreError::InvalidProvenance;
    assert_eq!(op.boundary_failure(original), original);
    cancel.cancel();
    assert_eq!(op.external_read(1), Err(original));
    assert_eq!(op.checkpoint_now(), Err(original));
    assert!(matches!(op.reserve_boundary(1), Err(error) if error == original));
    assert_eq!(op.boundary_failure(CoreError::ArithmeticOverflow), original);
    assert_eq!(op.first_fault(), Some(original));
    assert_eq!(op.stats(), None);
    assert_eq!(budget.usage().auxiliary_capacity, 0);
    assert_eq!(budget.begin(CancellationToken::new()).stats(), None);
}

#[test]
fn collection_choice_retains_non_work_admission_and_releases_capacity() {
    for collection in [WorkStatsCollection::Enabled, WorkStatsCollection::Disabled] {
        let budget = SourceBudget::create_with_admission(
            SourceLimits {
                retained_source_capacity: 8,
                retained_source_owners: 1,
                auxiliary_capacity: 16,
                ..SourceLimits::default()
            },
            Some(ModelLimits {
                model_records: 2,
                depth: 2,
                ..ModelLimits::default()
            }),
            if collection == WorkStatsCollection::Enabled {
                ResourceAdmissionMode::CumulativeWork
            } else {
                ResourceAdmissionMode::TaskAccess
            },
            collection,
        )
        .unwrap();
        let mut op = budget.begin(CancellationToken::new());
        op.set_clock_samples_for_test(&[100_000_000]);
        op.admit_model(LimitKind::ModelRecords, 1).unwrap();
        op.admit_model(LimitKind::ModelRecords, 1).unwrap();
        let records_error = CoreError::ResourceLimit {
            kind: LimitKind::ModelRecords,
            requested: 3,
            limit: 2,
        };
        assert_eq!(
            op.admit_model(LimitKind::ModelRecords, 1),
            Err(records_error)
        );
        assert_eq!(op.first_fault(), Some(records_error));
        assert_eq!(
            op.stats().is_some(),
            collection == WorkStatsCollection::Enabled
        );

        let mut op = budget.begin(CancellationToken::new());
        op.set_clock_samples_for_test(&[100_000_000]);
        op.admit_model(LimitKind::Depth, 2).unwrap();
        op.admit_model(LimitKind::Depth, 2).unwrap();
        assert_eq!(
            op.admit_model(LimitKind::Depth, 3),
            Err(CoreError::ResourceLimit {
                kind: LimitKind::Depth,
                requested: 3,
                limit: 2,
            })
        );

        let source = budget.reserve_source(8).unwrap();
        assert!(matches!(
            budget.reserve_source(1),
            Err(CoreError::ResourceLimit {
                kind: LimitKind::SourceCapacity,
                requested: 9,
                limit: 8,
            })
        ));
        let owner = budget.reserve_owner().unwrap();
        assert!(matches!(
            budget.reserve_owner(),
            Err(CoreError::ResourceLimit {
                kind: LimitKind::SourceOwners,
                requested: 2,
                limit: 1,
            })
        ));
        drop(source);
        drop(owner);
        assert_eq!(budget.usage().source_capacity, 0);
        assert_eq!(budget.usage().source_owners, 0);

        let mut op = budget.begin(CancellationToken::new());
        op.set_clock_samples_for_test(&[100_000_000]);
        let mut reservation = op.reserve_boundary(8).unwrap();
        reservation.admit_actual(16, &mut op).unwrap();
        let capacity_error = CoreError::ResourceLimit {
            kind: LimitKind::AuxiliaryCapacity,
            requested: 17,
            limit: 16,
        };
        assert_eq!(reservation.admit_actual(17, &mut op), Err(capacity_error));
        assert_eq!(op.external_events(1), Err(capacity_error));
        assert_eq!(budget.usage().auxiliary_capacity, 16);
        assert_eq!(
            op.stats().is_some(),
            collection == WorkStatsCollection::Enabled
        );
        drop(reservation);
        assert_eq!(budget.usage().auxiliary_capacity, 0);
    }
}

#[test]
fn collection_choice_preserves_actual_admission_overflow_and_its_first_fault() {
    for collection in [WorkStatsCollection::Enabled, WorkStatsCollection::Disabled] {
        // A test-only legal quota edge, without modifying diagnostic counters or
        // allocating the admitted byte count. This is not a product limit.
        let budget = SourceBudget::create_with_admission(
            SourceLimits::default(),
            Some(ModelLimits {
                reference_bytes: u64::MAX,
                ..ModelLimits::default()
            }),
            if collection == WorkStatsCollection::Enabled {
                ResourceAdmissionMode::CumulativeWork
            } else {
                ResourceAdmissionMode::TaskAccess
            },
            collection,
        )
        .unwrap();
        let cancel = CancellationToken::new();
        let mut op = budget.begin(cancel.clone());
        op.set_clock_samples_for_test(&[100_000_000]);
        op.admit_model(LimitKind::ReferenceBytes, u64::MAX).unwrap();
        assert_eq!(
            op.admit_model(LimitKind::ReferenceBytes, 1),
            Err(CoreError::ArithmeticOverflow)
        );
        cancel.cancel();
        assert_eq!(
            op.admit_model(LimitKind::ReferenceBytes, 0),
            Err(CoreError::ArithmeticOverflow)
        );
        assert_eq!(op.external_read(1), Err(CoreError::ArithmeticOverflow));
        assert_eq!(op.first_fault(), Some(CoreError::ArithmeticOverflow));
        assert_eq!(
            op.stats().is_some(),
            collection == WorkStatsCollection::Enabled
        );
        assert_eq!(budget.usage().source_capacity, 0);
        assert_eq!(budget.usage().auxiliary_capacity, 0);
        assert_eq!(budget.usage().source_owners, 0);

        let mut fresh = budget.begin(CancellationToken::new());
        fresh.set_clock_samples_for_test(&[100_000_000]);
        fresh.admit_model(LimitKind::ReferenceBytes, 1).unwrap();
        assert_eq!(fresh.first_fault(), None);
    }
}

#[test]
fn disabled_collection_keeps_real_charge_windows_and_block_invariants() {
    let budget = SourceBudget::task_fixture();
    let deadline = CoreError::DeadlineExceeded {
        elapsed_ns: ELAPSED_LIMIT_NS,
        limit_ns: ELAPSED_LIMIT_NS,
    };
    let mut op = budget.begin(CancellationToken::new());
    op.set_clock_samples_for_test(&[100_000_000, ELAPSED_LIMIT_NS]);
    op.copy(40_000).unwrap();
    op.scan(30_000).unwrap();
    // The complete 30,000-byte crossing block remains in the new window.
    assert_eq!(op.copy(36_000), Err(deadline));
    assert_eq!(op.stats(), None);

    let mut op = budget.begin(CancellationToken::new());
    op.set_clock_samples_for_test(&[100_000_000, ELAPSED_LIMIT_NS]);
    op.events(200).unwrap();
    op.events(100).unwrap();
    assert_eq!(op.events(156), Err(deadline));
    assert_eq!(op.stats(), None);

    let mut op = budget.begin(CancellationToken::new());
    op.set_clock_samples_for_test(&[100_000_000, ELAPSED_LIMIT_NS]);
    op.scan(CHECKPOINT_BYTES).unwrap();
    assert_eq!(op.events(1), Err(deadline));
    assert_eq!(op.stats(), None);
    let mut op = budget.begin(CancellationToken::new());
    op.set_clock_samples_for_test(&[100_000_000, ELAPSED_LIMIT_NS]);
    op.events(CHECKPOINT_EVENTS).unwrap();
    assert_eq!(op.copy(1), Err(deadline));
    assert_eq!(op.stats(), None);

    for kind in [
        WorkKind::Scan,
        WorkKind::Copy,
        WorkKind::Events,
        WorkKind::ViewScan,
        WorkKind::ViewCopy,
    ] {
        let mut op = budget.begin(CancellationToken::new());
        op.set_clock_samples_for_test(&[ELAPSED_LIMIT_NS]);
        let result = match kind {
            WorkKind::Scan => op.scan(CHECKPOINT_BYTES + 1),
            WorkKind::Copy => op.copy(CHECKPOINT_BYTES + 1),
            WorkKind::Events => op.events(CHECKPOINT_EVENTS + 1),
            WorkKind::ViewScan => op.view_scan(MAX_METADATA_VIEW_BYTES + 1),
            WorkKind::ViewCopy => op.view_copy(MAX_METADATA_VIEW_BYTES + 1),
        };
        // Block validation still precedes the scheduled clock read.
        assert_eq!(result, Err(CoreError::InternalInvariant));
        assert_eq!(op.checkpoint_now(), Err(CoreError::InternalInvariant));
        assert_eq!(op.stats(), None);
    }
}

#[test]
fn disabled_collection_keeps_metadata_bytes_actions_and_mixed_clock_windows() {
    let budget = SourceBudget::task_fixture();
    let deadline = CoreError::DeadlineExceeded {
        elapsed_ns: ELAPSED_LIMIT_NS,
        limit_ns: ELAPSED_LIMIT_NS,
    };
    for (width, accepted) in [(512, 127), (1, 255)] {
        let mut op = budget.begin(CancellationToken::new());
        op.set_clock_samples_for_test(&[ELAPSED_LIMIT_NS]);
        for _ in 0..accepted {
            op.view_scan(width).unwrap();
        }
        assert_eq!(op.stats(), None);
        assert_eq!(op.view_copy(width), Err(deadline));
        assert_eq!(op.stats(), None);
    }
    let mut op = budget.begin(CancellationToken::new());
    op.set_clock_samples_for_test(&[ELAPSED_LIMIT_NS]);
    op.scan(CHECKPOINT_BYTES - 512).unwrap();
    assert_eq!(op.view_scan(512), Err(deadline));
    assert_eq!(op.stats(), None);
}

#[test]
fn disabled_collection_keeps_final_clock_boundaries_and_first_fault_order() {
    let budget = SourceBudget::task_fixture();
    for elapsed_ns in [ELAPSED_LIMIT_NS - 1, ELAPSED_LIMIT_NS, ELAPSED_LIMIT_NS + 1] {
        let mut op = budget.begin(CancellationToken::new());
        op.set_clock_samples_for_test(&[elapsed_ns]);
        let expected = if elapsed_ns < ELAPSED_LIMIT_NS {
            Ok(())
        } else {
            Err(CoreError::DeadlineExceeded {
                elapsed_ns,
                limit_ns: ELAPSED_LIMIT_NS,
            })
        };
        assert_eq!(op.view_scan(0), expected);
        assert_eq!(op.events(0), expected);
        assert_eq!(op.checkpoint_now(), expected);
        assert_eq!(op.stats(), None);
    }

    let cancel = CancellationToken::new();
    let mut op = budget.begin(cancel.clone());
    op.set_clock_samples_for_test(&[100_000_080, 100_000_079]);
    op.checkpoint_now().unwrap();
    assert_eq!(op.checkpoint_now(), Err(CoreError::ClockFailure));
    cancel.cancel();
    assert_eq!(op.checkpoint_now(), Err(CoreError::ClockFailure));
    assert_eq!(op.events(1), Err(CoreError::ClockFailure));
    assert_eq!(op.stats(), None);

    let mut op = budget.begin(cancel.clone());
    op.set_clock_samples_for_test(&[ELAPSED_LIMIT_NS]);
    assert_eq!(op.checkpoint_now(), Err(CoreError::Cancelled));
    assert_eq!(
        op.view_copy(MAX_METADATA_VIEW_BYTES + 1),
        Err(CoreError::Cancelled)
    );
    assert_eq!(op.stats(), None);
    let typed_cancel = CancellationToken::new();
    let mut op = budget.begin(typed_cancel.clone());
    op.boundary_failure(CoreError::InvalidProvenance);
    typed_cancel.cancel();
    op.set_clock_samples_for_test(&[ELAPSED_LIMIT_NS]);
    assert_eq!(op.checkpoint_now(), Err(CoreError::InvalidProvenance));
    assert_eq!(op.events(0), Err(CoreError::InvalidProvenance));
    assert_eq!(op.stats(), None);
}

#[test]
fn disabled_collection_keeps_allocation_clock_checks_and_post_failure_release() {
    let budget = SourceBudget::task_fixture();
    let deadline = CoreError::DeadlineExceeded {
        elapsed_ns: ELAPSED_LIMIT_NS,
        limit_ns: ELAPSED_LIMIT_NS,
    };
    let mut op = budget.begin(CancellationToken::new());
    op.set_clock_samples_for_test(&[ELAPSED_LIMIT_NS]);
    assert!(matches!(op.reserve_boundary(8), Err(error) if error == deadline));
    assert_eq!(budget.usage().auxiliary_capacity, 0);
    assert_eq!(op.stats(), None);

    let mut op = budget.begin(CancellationToken::new());
    op.set_clock_samples_for_test(&[100_000_000]);
    let mut reservation = op.reserve_boundary(8).unwrap();
    op.set_clock_samples_for_test(&[100_000_000, ELAPSED_LIMIT_NS]);
    assert_eq!(reservation.admit_actual(16, &mut op), Err(deadline));
    assert_eq!(budget.usage().auxiliary_capacity, 16);
    assert_eq!(op.stats(), None);
    drop(reservation);
    assert_eq!(budget.usage().auxiliary_capacity, 0);
    assert_eq!(op.checkpoint_now(), Err(deadline));
    assert_eq!(op.stats(), None);
    assert_eq!(budget.usage().source_capacity, 0);
    assert_eq!(budget.usage().source_owners, 0);
}
