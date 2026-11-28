// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE file in the project root for license information.
// ---------------------------------------------------------------------------------------------

use super::*;
use crate::policy::{CancellationToken, ResourceAdmissionMode, WorkStatsCollection};

#[test]
fn every_legal_policy_keeps_original_charges_stats_and_index_dispatch() {
    let mut legal = 0;
    for model in [None, Some(ModelLimits::default())] {
        for mode in [
            ResourceAdmissionMode::CumulativeWork,
            ResourceAdmissionMode::TaskAccess,
        ] {
            for stats in [WorkStatsCollection::Enabled, WorkStatsCollection::Disabled] {
                let Ok(budget) = SourceBudget::create_with_admission(
                    SourceLimits::default(),
                    model,
                    mode,
                    stats,
                ) else {
                    continue;
                };
                legal += 1;
                let mut original = budget.begin(CancellationToken::new());
                let mut wrapped = budget.begin(CancellationToken::new());
                // Bounded modes already sampled their real monotonic clock
                // at begin. A later synthetic zero would move it backwards.
                original.set_clock_samples_for_test(&[499_999_999]);
                wrapped.set_clock_samples_for_test(&[499_999_999]);
                original.scan(31).unwrap();
                original.copy(17).unwrap();
                original.events(3).unwrap();
                original.view_scan(40).unwrap();
                original.view_copy(40).unwrap();
                let expected_index = original.uses_verified_index();
                construction_scope(&mut wrapped, |mut work| {
                    assert_eq!(work.uses_verified_index(), expected_index);
                    work.scan(31).unwrap();
                    work.copy(17).unwrap();
                    work.events(3).unwrap();
                    work.view_scan(40).unwrap();
                    work.view_copy(40).unwrap();
                    work.finish().unwrap();
                });
                assert_eq!(wrapped.stats(), original.stats());
                assert_eq!(wrapped.first_fault(), None);
            }
        }
    }
    assert_eq!(legal, 3);
}

#[test]
fn abandoned_work_latches_invariant_and_preserves_an_earlier_typed_error() {
    let budget = SourceBudget::model_fixture();
    let mut op = budget.begin(CancellationToken::new());
    construction_scope(&mut op, |_work| {});
    assert_eq!(op.first_fault(), Some(CoreError::InternalInvariant));

    let cancel = CancellationToken::new();
    let mut op = budget.begin(cancel.clone());
    construction_scope(&mut op, |mut work| {
        assert_eq!(
            work.fail(CoreError::InvalidProvenance),
            CoreError::InvalidProvenance
        );
        cancel.cancel();
        assert_eq!(work.events(1), Err(CoreError::InvalidProvenance));
        assert_eq!(
            work.reserve_native(16).err(),
            Some(CoreError::InvalidProvenance)
        );
    });
    assert_eq!(op.first_fault(), Some(CoreError::InvalidProvenance));
    assert_eq!(budget.usage().auxiliary_capacity, 0);
}

#[test]
fn unwind_closes_same_work_without_replacing_a_typed_fault() {
    for typed in [false, true] {
        let budget = SourceBudget::model_fixture();
        let mut op = budget.begin(CancellationToken::new());
        let caught = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            construction_scope(&mut op, |mut work| {
                if typed {
                    work.fail(CoreError::OutOfMemory);
                }
                panic!("construction unwind negative control");
            });
        }));
        assert!(caught.is_err());
        assert_eq!(
            op.first_fault(),
            Some(if typed {
                CoreError::OutOfMemory
            } else {
                CoreError::InternalInvariant
            })
        );
    }
}

#[test]
fn completion_observes_same_flag_and_does_not_publish_a_private_success() {
    let budget = SourceBudget::model_fixture();
    let cancel = CancellationToken::new();
    let mut op = budget.begin(cancel.clone());
    let result = construction_scope(&mut op, |mut work| {
        work.copy(1)?;
        cancel.cancel();
        work.finish()?;
        Ok::<_, CoreError>(42)
    });
    assert_eq!(result, Err(CoreError::Cancelled));
    assert_eq!(op.first_fault(), Some(CoreError::Cancelled));
}

#[test]
fn native_reservation_keeps_quota_identity_actual_growth_and_drop_accounting() {
    let budget = SourceBudget::model_fixture();
    let other = SourceBudget::model_fixture();
    let mut op = budget.begin(CancellationToken::new());
    construction_scope(&mut op, |mut work| {
        let mut lease = work.reserve_native(16).unwrap();
        assert_eq!(budget.usage().auxiliary_capacity, 16);
        work.admit_native_actual(&mut lease, 32).unwrap();
        assert_eq!(budget.usage().auxiliary_capacity, 32);
        work.admit_native_actual(&mut lease, 8).unwrap();
        assert_eq!(budget.usage().auxiliary_capacity, 32);
        drop(lease);
        assert_eq!(budget.usage().auxiliary_capacity, 0);
        // Deliberately forged only in this private policy test. Production
        // NativeLease has no raw constructor or clone.
        let mut foreign = NativeLease {
            lease: other.reserve_auxiliary(1).unwrap(),
            id: PhantomData,
        };
        assert_eq!(
            work.admit_native_actual(&mut foreign, 2),
            Err(CoreError::WrongBudget)
        );
        assert_eq!(other.usage().auxiliary_capacity, 1);
        drop(foreign);
        assert_eq!(work.finish(), Err(CoreError::WrongBudget));
    });
    assert_eq!(budget.usage().auxiliary_capacity, 0);
    assert_eq!(other.usage().auxiliary_capacity, 0);
}
