// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE file in the project root for license information.
// ---------------------------------------------------------------------------------------------

use super::*;

const LIVE: u64 = 499_999_900;
const DEAD: u64 = 500_000_000;

#[test]
fn bootstrap_clock_precedes_flag_allocation_and_checks_its_return() {
    let budget = SourceBudget::task_fixture();
    for samples in [vec![LIVE, DEAD], vec![LIVE, LIVE + 1, DEAD]] {
        let mut op = budget.begin_inner(None);
        assert!(op.cancel.is_none());
        assert_eq!(budget.usage().auxiliary_capacity, 0);
        op.set_clock_samples_for_test(&samples);
        assert_eq!(
            op.finish_bootstrap(false),
            Err(CoreError::DeadlineExceeded {
                elapsed_ns: DEAD,
                limit_ns: DEAD
            })
        );
        assert_eq!(op.clock_read_count_for_test(), samples.len());
        // In the second case the allocated flag is still owned by the faulted
        // local operation. The public factory drops that owner on Err.
        assert_eq!(op.cancel.is_some(), samples.len() == 3);
        drop(op);
        assert_eq!(budget.usage().auxiliary_capacity, 0);
    }
}

#[test]
fn bootstrap_backward_clock_is_sticky_and_does_not_allocate_a_flag() {
    let budget = SourceBudget::task_fixture();
    let mut op = budget.begin_inner(None);
    op.set_clock_samples_for_test(&[LIVE, 1]);
    assert_eq!(op.finish_bootstrap(false), Err(CoreError::ClockFailure));
    assert!(op.cancel.is_none());
    assert_eq!(
        op.boundary_failure(CoreError::Cancelled),
        CoreError::ClockFailure
    );
    drop(op);
    assert_eq!(budget.usage().auxiliary_capacity, 0);
}

#[test]
fn initially_cancelled_factory_publishes_no_partial_owner() {
    let budget = SourceBudget::task_fixture();
    assert!(matches!(
        budget.begin_cancellable(true),
        Err(CoreError::Cancelled)
    ));
    assert_eq!(budget.usage().auxiliary_capacity, 0);
    assert_eq!(budget.usage().source_owners, 0);
}

#[test]
fn fresh_flag_admission_failure_leaves_no_partial_operation() {
    let budget = SourceBudget::new(SourceLimits {
        auxiliary_capacity: 1,
        ..SourceLimits::default()
    })
    .unwrap();
    assert!(matches!(
        budget.begin_cancellable(false),
        Err(CoreError::ResourceLimit {
            kind: LimitKind::AuxiliaryCapacity,
            ..
        })
    ));
    assert_eq!(budget.usage().auxiliary_capacity, 0);
}

#[test]
fn flag_capacity_lives_until_its_last_clone_and_flags_are_independent() {
    let budget = SourceBudget::task_fixture();
    let mut first = budget.begin_cancellable(false).unwrap();
    let mut second = budget.begin_cancellable(false).unwrap();
    let flag = first.cancellation();
    let pin = flag.clone();
    assert!(!flag.same_flag(&second.cancellation()));
    flag.cancel();
    assert_eq!(first.checkpoint_now(), Err(CoreError::Cancelled));
    second.checkpoint_now().unwrap();
    drop(first);
    drop(second);
    assert!(budget.usage().auxiliary_capacity > 0);
    drop(flag);
    assert!(budget.usage().auxiliary_capacity > 0);
    drop(pin);
    assert_eq!(budget.usage().auxiliary_capacity, 0);
}
