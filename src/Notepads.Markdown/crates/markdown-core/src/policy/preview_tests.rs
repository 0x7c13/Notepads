// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE file in the project root for license information.
// ---------------------------------------------------------------------------------------------

use super::*;

#[test]
fn preview_policy_uses_current_task_access_and_keeps_its_limits() {
    let preview = SourceBudget::preview();
    assert_eq!(preview.elapsed_limit_ns(), Some(5_000_000_000));
    assert_eq!(preview.limits().auxiliary_capacity, 268_435_456);
    assert_eq!(
        preview.work_stats_collection(),
        WorkStatsCollection::Disabled
    );
    assert_eq!(preview.active_work_limit(), None);
    let op = preview.begin(CancellationToken::new());
    assert!(op.uses_task_access());
    assert!(op.uses_verified_index());
    assert_eq!(op.stats(), None);
}

#[test]
fn preview_deadline_uses_the_existing_monotonic_clock_and_first_fault() {
    let budget = SourceBudget::preview();
    let cancel = CancellationToken::new();
    let mut op = budget.begin(cancel.clone());
    op.set_clock_samples_for_test(&[500_000_000, 4_999_999_999, 5_000_000_000]);
    op.checkpoint_now().unwrap();
    op.checkpoint_now().unwrap();
    let deadline = CoreError::DeadlineExceeded {
        elapsed_ns: 5_000_000_000,
        limit_ns: 5_000_000_000,
    };
    assert_eq!(op.checkpoint_now(), Err(deadline));
    cancel.cancel();
    assert_eq!(op.checkpoint_now(), Err(deadline));
    assert_eq!(op.reserve_boundary(1).err(), Some(deadline));

    let cancel = CancellationToken::new();
    let mut cancelled = budget.begin(cancel.clone());
    cancelled.set_clock_samples_for_test(&[5_000_000_000]);
    cancel.cancel();
    assert_eq!(cancelled.checkpoint_now(), Err(CoreError::Cancelled));
    assert_eq!(
        cancelled.reserve_boundary(1).err(),
        Some(CoreError::Cancelled)
    );
    assert_eq!(budget.usage().auxiliary_capacity, 0);
}

#[test]
fn preview_root_caps_shared_reservations_and_releases_them() {
    let budget = SourceBudget::preview();
    let mut op = budget.begin(CancellationToken::new());
    // Reservation-only control: this does not allocate or measure 256MiB.
    let reservation = op.reserve_boundary(268_435_456).unwrap();
    assert_eq!(budget.usage().auxiliary_capacity, 268_435_456);
    let mut other = budget.clone().begin(CancellationToken::new());
    assert_eq!(
        other.reserve_boundary(1).err(),
        Some(CoreError::ResourceLimit {
            kind: LimitKind::AuxiliaryCapacity,
            requested: 268_435_457,
            limit: 268_435_456,
        })
    );
    drop(reservation);
    assert_eq!(budget.usage().auxiliary_capacity, 0);
    assert_eq!(budget.usage().source_capacity, 0);
    assert_eq!(budget.usage().source_owners, 0);
}
