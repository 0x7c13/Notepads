// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE file in the project root for license information.
// ---------------------------------------------------------------------------------------------

use super::{growing, sort};
use crate::error::{CoreError, LimitKind};
use crate::memory::{BuildGrowingVec, BuildRows};
use crate::policy::{CancellationToken, OwnedWork, SourceBudget, SourceLimits, construction_scope};

// Retained original heap algorithm, expressed over ordinary test-owned vectors.
// The oracle compares complete index permutations, including equal-key order.
fn original(keys: &[u64]) -> Vec<u32> {
    let mut values: Vec<_> = (0..keys.len() as u32).collect();
    if values
        .windows(2)
        .all(|pair| keys[pair[0] as usize] <= keys[pair[1] as usize])
    {
        return values;
    }
    fn sift(values: &mut [u32], mut root: usize, end: usize, keys: &[u64]) {
        loop {
            let child = root * 2 + 1;
            if child >= end {
                return;
            }
            let mut selected = child;
            if child + 1 < end && keys[values[child] as usize] < keys[values[child + 1] as usize] {
                selected += 1;
            }
            if keys[values[root] as usize] >= keys[values[selected] as usize] {
                return;
            }
            let a = values[root];
            let b = values[selected];
            values[root] = b;
            values[selected] = a;
            root = selected;
        }
    }
    for root in (0..values.len() / 2).rev() {
        let end = values.len();
        sift(&mut values, root, end, keys);
    }
    for end in (1..values.len()).rev() {
        let a = values[0];
        let b = values[end];
        values[0] = b;
        values[end] = a;
        sift(&mut values, 0, end, keys);
    }
    values
}

fn prepare<'id>(
    keys: &[u64],
    work: &mut OwnedWork<'id, '_>,
) -> (BuildGrowingVec<'id, u32>, BuildRows<'id, u64>) {
    let mut values =
        BuildGrowingVec::new(keys.len() as u64, LimitKind::IndexRecords, work).unwrap();
    let mut backing = BuildRows::new(keys.len() as u64, LimitKind::IndexRecords, work).unwrap();
    for (index, key) in keys.iter().copied().enumerate() {
        values.push(index as u32, work).unwrap();
        backing.push(key, work).unwrap();
    }
    (values, backing)
}

fn check_case(keys: &[u64], budget: SourceBudget) -> Vec<u32> {
    let expected = original(keys);
    let mut op = budget.begin(CancellationToken::new());
    let mut trace = Vec::new();
    construction_scope(&mut op, |mut work| {
        let (mut values, backing) = prepare(keys, &mut work);
        let before = budget.usage();
        sort(
            &mut values,
            |id, work| {
                trace.push(id);
                backing.get(id as usize, work)
            },
            &mut work,
        )
        .unwrap();
        let actual: Vec<_> = (0..values.len())
            .map(|at| growing(&values, at, &mut work).unwrap())
            .collect();
        assert_eq!(actual, expected, "keys={keys:?}");
        assert_eq!(budget.usage().auxiliary_capacity, before.auxiliary_capacity);
        assert_eq!(
            budget.usage().auxiliary_capacity_peak,
            before.auxiliary_capacity_peak
        );
        drop(values);
        drop(backing);
        assert_eq!(budget.usage().auxiliary_capacity, 0);
        work.finish().unwrap();
    });
    trace
}

#[test]
fn sort_matches_original_complete_permutation_for_duplicate_and_deep_cases() {
    for keys in [
        vec![],
        vec![1],
        vec![1, 0],
        vec![0, 2, 1],
        vec![2, 0, 1],
        vec![4, 4, 4, 4],
        vec![2, 1, 2, 1, 2, 1, 0],
        vec![0, 8, 7, 6, 5, 4, 3, 2, 1],
        (0..65).collect(),
        (0..65).rev().collect(),
    ] {
        check_case(&keys, SourceBudget::preview());
        check_case(&keys, SourceBudget::new(SourceLimits::default()).unwrap());
    }
    // Exhaust all small ternary-key vectors, then deterministic larger mixes.
    for len in 0..=6_u32 {
        for mut code in 0..3_u32.pow(len) {
            let keys: Vec<_> = (0..len)
                .map(|_| {
                    let key = (code % 3) as u64;
                    code /= 3;
                    key
                })
                .collect();
            check_case(&keys, SourceBudget::preview());
        }
    }
    let mut state = 0x4d61_726b_5374_7261_u64;
    for len in [7, 8, 15, 16, 31, 32, 63, 64, 127] {
        let keys: Vec<_> = (0..len)
            .map(|_| {
                state = state.wrapping_mul(6364136223846793005).wrapping_add(1);
                state >> 60
            })
            .collect();
        check_case(&keys, SourceBudget::preview());
    }
}

#[test]
fn sort_preserves_first_read_order_and_does_not_read_leaf_roots() {
    assert!(check_case(&[], SourceBudget::preview()).is_empty());
    assert!(check_case(&[7], SourceBudget::preview()).is_empty());
    assert_eq!(check_case(&[1, 0], SourceBudget::preview()), [0, 1, 0, 1]);
    let trace = check_case(&[0, 2, 1], SourceBudget::preview());
    // Presorted scan 0/1/2, then the first two-child sift left/right/root.
    assert_eq!(&trace[..6], &[0, 1, 2, 1, 2, 0]);
    assert_eq!(
        check_case(&[0, 1, 2, 3], SourceBudget::preview()),
        [0, 1, 2, 3]
    );
}

#[test]
fn sort_key_first_fault_dominates_later_cancellation_and_releases_rows() {
    let budget = SourceBudget::preview();
    let cancel = CancellationToken::new();
    let mut op = budget.begin(cancel.clone());
    construction_scope(&mut op, |mut work| {
        let (mut values, backing) = prepare(&[0, 2, 1], &mut work);
        let mut calls = 0;
        let error = sort(
            &mut values,
            |id, work| {
                calls += 1;
                let value = backing.get(id as usize, work)?;
                if calls == 4 {
                    let error = work.fail(CoreError::InvalidBoundary);
                    cancel.cancel();
                    return Err(error);
                }
                Ok(value)
            },
            &mut work,
        )
        .unwrap_err();
        assert_eq!(calls, 4);
        assert_eq!(error, CoreError::InvalidBoundary);
        assert_eq!(work.check(), Err(error));
        assert_eq!(values.push(0, &mut work), Err(error));
        drop(values);
        drop(backing);
        assert_eq!(budget.usage().auxiliary_capacity, 0);
        assert_eq!(work.finish(), Err(error));
    });
}

#[test]
fn sort_cancellation_during_a_real_key_read_cannot_return_success() {
    let budget = SourceBudget::preview();
    let cancel = CancellationToken::new();
    let mut op = budget.begin(cancel.clone());
    construction_scope(&mut op, |mut work| {
        let (mut values, backing) = prepare(&[1, 0], &mut work);
        let mut calls = 0;
        let error = sort(
            &mut values,
            |id, work| {
                calls += 1;
                let key = backing.get(id as usize, work)?;
                if calls == 4 {
                    cancel.cancel();
                }
                Ok(key)
            },
            &mut work,
        )
        .unwrap_err();
        assert_eq!(error, CoreError::Cancelled);
        assert_eq!(work.first_fault(), Some(error));
        drop(values);
        drop(backing);
        assert_eq!(budget.usage().auxiliary_capacity, 0);
        assert_eq!(work.finish(), Err(error));
    });
}

#[test]
fn sort_budget_failure_between_the_two_real_sift_writes_is_terminal() {
    let baseline = SourceBudget::new(SourceLimits::default()).unwrap();
    let mut op = baseline.begin(CancellationToken::new());
    let before_swap = construction_scope(&mut op, |mut work| {
        let (mut values, backing) = prepare(&[0, 2, 1], &mut work);
        let mut calls = 0;
        let mut before_swap = None;
        sort(
            &mut values,
            |id, work| {
                calls += 1;
                let value = backing.get(id as usize, work)?;
                // Three presorted reads, then left/right/root in the first
                // heap sift. Root key 0 is below the selected child key 2.
                if calls == 6 {
                    before_swap = Some(work.stats().unwrap().charged_units());
                }
                Ok(value)
            },
            &mut work,
        )
        .unwrap();
        drop(values);
        drop(backing);
        work.finish().unwrap();
        before_swap.unwrap()
    });
    // The first sift replace charges one event and the real four-byte slot
    // write. The second replace's event fails after that first write commits.
    let limit = before_swap + 1 + size_of::<u32>() as u64;
    let budget = SourceBudget::new(SourceLimits {
        max_work: limit,
        ..SourceLimits::default()
    })
    .unwrap();
    let mut op = budget.begin(CancellationToken::new());
    construction_scope(&mut op, |mut work| {
        let (mut values, backing) = prepare(&[0, 2, 1], &mut work);
        let error = sort(
            &mut values,
            |id, work| backing.get(id as usize, work),
            &mut work,
        )
        .unwrap_err();
        assert_eq!(
            error,
            CoreError::ResourceLimit {
                kind: LimitKind::Work,
                requested: limit + 1,
                limit,
            }
        );
        assert_eq!(work.stats().unwrap().charged_units(), limit);
        assert_eq!(work.first_fault(), Some(error));
        // The copied child is physically at root, while the original root has
        // not yet replaced the child. This is unpublished, poisoned scratch.
        assert_eq!(values.raw_value_for_test(0), Some(1));
        assert_eq!(values.raw_value_for_test(1), Some(1));
        assert_eq!(values.raw_value_for_test(2), Some(2));
        assert_eq!(growing(&values, 0, &mut work), Err(error));
        drop(values);
        drop(backing);
        assert_eq!(budget.usage().auxiliary_capacity, 0);
        assert_eq!(work.finish(), Err(error));
    });
}

#[test]
fn sort_deadline_at_each_remaining_real_clock_read_releases_all_rows() {
    let budget = SourceBudget::task_fixture();
    let mut op = budget.begin(CancellationToken::new());
    op.set_clock_samples_for_test(&[100_000_000]);
    let reads = construction_scope(&mut op, |mut work| {
        let (mut values, backing) = prepare(&[0, 8, 7, 6, 5, 4, 3, 2, 1], &mut work);
        work.set_clock_samples_for_test(&[100_000_000]);
        sort(
            &mut values,
            |id, work| backing.get(id as usize, work),
            &mut work,
        )
        .unwrap();
        let reads = work.clock_read_count_for_test();
        drop(values);
        drop(backing);
        work.finish().unwrap();
        reads
    });
    assert!(reads > 0);
    for at in 1..=reads {
        let budget = SourceBudget::task_fixture();
        let mut op = budget.begin(CancellationToken::new());
        op.set_clock_samples_for_test(&[100_000_000]);
        construction_scope(&mut op, |mut work| {
            let (mut values, backing) = prepare(&[0, 8, 7, 6, 5, 4, 3, 2, 1], &mut work);
            let mut samples = vec![100_000_000; at];
            samples[at - 1] = 500_000_000;
            work.set_clock_samples_for_test(&samples);
            let error = sort(
                &mut values,
                |id, work| backing.get(id as usize, work),
                &mut work,
            )
            .unwrap_err();
            assert_eq!(
                error,
                CoreError::DeadlineExceeded {
                    elapsed_ns: 500_000_000,
                    limit_ns: 500_000_000,
                }
            );
            assert_eq!(work.check(), Err(error));
            drop(values);
            drop(backing);
            assert_eq!(budget.usage().auxiliary_capacity, 0);
            assert_eq!(work.finish(), Err(error));
        });
    }
}
