// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE file in the project root for license information.
// ---------------------------------------------------------------------------------------------

use super::*;
use crate::slice::{copy_within_with_control, stable_sort_by_with_control};
use alloc::string::String;

#[test]
fn reserved_noncopy_commit_has_no_allocator_or_backend_call() {
    let _isolation = isolate_observer();
    let (resources, _, state) = setup(Limits::default());
    let mut values = BudgetVec::new(resources.clone(), ResourceKind::Scratch);
    values.try_reserve_exact(1).unwrap();
    let value = String::from("one unique owner");
    let calls = state.checkpoint_calls.load(Ordering::Relaxed);
    let reserves = state.reserves.load(Ordering::Relaxed);
    start_alloc_observation();
    values.push_reserved(value);
    ENABLED.store(false, Ordering::Relaxed);
    assert_eq!(ALLOCS.load(Ordering::Relaxed), 0);
    assert_eq!(values[0], "one unique owner");
    assert_eq!(state.checkpoint_calls.load(Ordering::Relaxed), calls);
    assert_eq!(state.reserves.load(Ordering::Relaxed), reserves);
    drop(values);
    resources.validate_retired().unwrap();
}

#[test]
fn resize_stops_before_second_window_and_shrink_retains_its_fee() {
    let _isolation = isolate_observer();
    let (resources, _, state) = setup(Limits {
        work: 512,
        ..Limits::default()
    });
    let mut values = BudgetVec::<u16>::new(resources.clone(), ResourceKind::Scratch);
    let error = values.try_resize_copy(600, 7).unwrap_err();
    assert_eq!(error, Error::new(FailureKind::Quota, LimitKind::Work));
    assert_eq!(values.len(), 256);
    assert!(values.iter().all(|&value| value == 7));
    assert_eq!(values.capacity(), 600);
    drop(values);
    assert_eq!(state.live.load(Ordering::Relaxed), 0);
    resources.validate_retired().unwrap();

    let (resources, _, state) = setup(Limits::default());
    let mut values = BudgetVec::<u16>::new(resources.clone(), ResourceKind::Geometry);
    for length in [255, 256, 257, 600] {
        values.try_resize_copy(length, 9).unwrap();
        assert_eq!(values.len(), length);
    }
    let pointer = values.as_ptr();
    let fee = state.live.load(Ordering::Relaxed);
    values.try_resize_copy(0, 0).unwrap();
    assert_eq!(values.as_ptr(), pointer);
    assert_eq!(state.live.load(Ordering::Relaxed), fee);
    drop(values);
    resources.validate_retired().unwrap();
}

#[test]
fn resize_zst_pays_logical_work_and_large_record_boundary_is_explicit() {
    let _isolation = isolate_observer();
    let (resources, _, state) = setup(Limits {
        work: 256,
        ..Limits::default()
    });
    let mut values = BudgetVec::<()>::new(resources.clone(), ResourceKind::Scratch);
    assert_eq!(
        values.try_resize_copy(513, ()).unwrap_err().limit,
        LimitKind::Work
    );
    assert_eq!(values.len(), 256);
    assert_eq!(state.reserves.load(Ordering::Relaxed), 0);
    drop(values);
    resources.validate_retired().unwrap();

    let (resources, _, _) = setup(Limits::default());
    let mut exact = BudgetVec::<[u8; 65_536]>::new(resources.clone(), ResourceKind::Scratch);
    exact.try_resize_copy(2, [3; 65_536]).unwrap();
    assert_eq!(exact.len(), 2);
    assert_eq!(resources.statistics().work, 131_072);
    drop(exact);
    resources.validate_retired().unwrap();

    let (resources, _, state) = setup(Limits::default());
    let mut oversized = BudgetVec::<[u8; 65_537]>::new(resources.clone(), ResourceKind::Scratch);
    assert_eq!(
        oversized.try_resize_copy(1, [0; 65_537]).unwrap_err(),
        Error::new(FailureKind::Quota, LimitKind::Capacity)
    );
    assert_eq!(state.reserves.load(Ordering::Relaxed), 0);
    assert_eq!(oversized.len(), 0);
    drop(oversized);
    resources.validate_retired().unwrap();
}

#[test]
fn explicit_clone_second_window_never_calls_remaining_callbacks() {
    let _isolation = isolate_observer();
    let (resources, _, state) = setup(Limits {
        work: 256,
        ..Limits::default()
    });
    let mut source = BudgetVec::<u8>::new(resources.clone(), ResourceKind::Scratch);
    // Fixture construction is outside the new clone operation's work scope.
    source.try_extend_from_slice(&[5; 600]).unwrap();
    let pointer = source.as_ptr();
    let mut calls = 0;
    let error = source
        .try_clone_with(|value, request| {
            assert!(request.is_same_request(&resources));
            calls += 1;
            Ok(*value)
        })
        .unwrap_err();
    assert_eq!(error.limit, LimitKind::Work);
    assert_eq!(calls, 256);
    assert_eq!(source.as_ptr(), pointer);
    assert_eq!(source.as_slice(), &[5; 600]);
    assert_eq!(state.live.load(Ordering::Relaxed), 600);
    drop(source);
    resources.validate_retired().unwrap();
}

fn nested_source(resources: &ResourcesRef) -> BudgetVec<BudgetVec<u8>> {
    let mut source = BudgetVec::new(resources.clone(), ResourceKind::Scratch);
    source.try_reserve_exact(3).unwrap();
    for value in 0..3 {
        let mut child = BudgetVec::new(resources.clone(), ResourceKind::Geometry);
        child.try_resize_copy(3, value).unwrap();
        source.push_reserved(child);
    }
    source
}

#[test]
fn nested_clone_admission_failure_preserves_source_and_retires_partial_target() {
    let _isolation = isolate_observer();
    let (resources, _, state) = setup(Limits::default());
    let source = nested_source(&resources);
    let paid = state.live.load(Ordering::Relaxed);
    let admissions = state.admissions.load(Ordering::Relaxed);
    state
        .fail_on_admission
        .store(admissions + 2, Ordering::Relaxed);
    let mut calls = 0;
    let error = source
        .try_clone_with(|child, request| {
            assert!(child.belongs_to(request));
            calls += 1;
            child.try_clone_with(|value, _| Ok(*value))
        })
        .unwrap_err();
    assert_eq!(error, Error::native(-88));
    assert_eq!(calls, 1);
    assert_eq!(state.live.load(Ordering::Relaxed), paid);
    for (index, child) in source.iter().enumerate() {
        assert_eq!(child.as_slice(), &[index as u8; 3]);
    }
    drop(source);
    resources.validate_retired().unwrap();
}

#[test]
fn clone_records_callback_error_before_partial_target_refunds_can_fail() {
    let _isolation = isolate_observer();
    let (resources, _, state) = setup(Limits::default());
    let source = nested_source(&resources);
    let mut calls = 0;
    let error = source
        .try_clone_with(|child, _| {
            calls += 1;
            if calls == 2 {
                state.failed_release.store(true, Ordering::Relaxed);
                return Err(Error::native(-900));
            }
            let copied = child.try_clone_with(|value, _| Ok(*value))?;
            WATCH_PTR.store(copied.as_ptr() as usize, Ordering::Relaxed);
            WATCH_FREED.store(false, Ordering::Relaxed);
            Ok(copied)
        })
        .unwrap_err();
    assert_eq!(error, Error::native(-900));
    assert_eq!(resources.failure(), Some(error));
    assert_eq!(calls, 2);
    assert!(WATCH_FREED.load(Ordering::Acquire));
    assert_eq!(resources.statistics().release_failures, 2);
    // Only the deliberately failed target refunds remain after the source
    // retires normally. Physical target buffers were already freed above.
    state.failed_release.store(false, Ordering::Relaxed);
    drop(source);
    assert_eq!(resources.statistics().live_leases, 2);
    assert!(resources.validate_retired().is_err());
    assert_eq!(resources.failure(), Some(error));
}

#[test]
fn callback_local_scratch_retirement_fault_blocks_the_next_copy_callback() {
    let _isolation = isolate_observer();
    let (resources, _, state) = setup(Limits::default());
    let mut source = BudgetVec::new(resources.clone(), ResourceKind::Scratch);
    source.try_extend_from_slice(&[1u8, 2, 3]).unwrap();
    struct ResetRelease<'a>(&'a AtomicBool);
    impl Drop for ResetRelease<'_> {
        fn drop(&mut self) {
            self.0.store(false, Ordering::Relaxed);
        }
    }
    let mut callbacks = 0;
    let result = source.try_clone_with(|value, request| {
        callbacks += 1;
        // Reset is declared first so the genuine callback-local scratch frees
        // and fails its refund before fixture bookkeeping clears the fault.
        let _reset = ResetRelease(&state.failed_release);
        let mut scratch = BudgetVec::new(request.clone(), ResourceKind::Scratch);
        scratch.try_resize_copy(1, *value)?;
        let copied = scratch[0];
        state.failed_release.store(true, Ordering::Relaxed);
        Ok(copied)
    });
    assert_eq!(result.unwrap_err(), Error::native(-77));
    assert_eq!(
        callbacks, 1,
        "no next callback after local retirement fixed the slot"
    );
    assert_eq!(source.as_slice(), &[1, 2, 3]);
    assert_eq!(resources.statistics().release_failures, 1);
    drop(source);
    assert_eq!(resources.statistics().live_leases, 1);
    assert!(resources.validate_retired().is_err());
    // Unit callback/owner boundary only, not HB/RF/K producer qualification.
}

#[test]
fn bounded_overlap_copy_matches_memmove_in_both_directions() {
    let _isolation = isolate_observer();
    for (start, end, destination) in [
        (0, 600, 128),
        (128, 728, 0),
        (3, 3, 800),
        (0, 800, 0),
        (200, 760, 30),
    ] {
        let original = (0..800u32).collect::<Vec<_>>();
        let mut expected = original.clone();
        expected.copy_within(start..end, destination);
        let mut plain = original.clone();
        copy_within_with_control(
            &mut plain,
            start..end,
            destination,
            &mut NoControl::<Error>::new(),
        )
        .unwrap();
        let (resources, _, _) = setup(Limits::default());
        let mut paid = original;
        copy_within_with_control(
            &mut paid,
            start..end,
            destination,
            &mut RequestControl(&resources),
        )
        .unwrap();
        assert_eq!(plain, expected);
        assert_eq!(paid, expected);
        resources.validate_retired().unwrap();
    }
}

#[test]
fn overlap_copy_stops_before_its_second_rightward_window() {
    let _isolation = isolate_observer();
    let (resources, _, _) = setup(Limits {
        work: 1024,
        ..Limits::default()
    });
    let mut actual = (0..800u32).collect::<Vec<_>>();
    let mut expected = actual.clone();
    expected.copy_within(344..600, 472);
    let error = copy_within_with_control(&mut actual, 0..600, 128, &mut RequestControl(&resources))
        .unwrap_err();
    assert_eq!(error.limit, LimitKind::Work);
    assert_eq!(actual, expected);
    resources.validate_retired().unwrap();
}

#[test]
fn invalid_borrowed_ranges_return_typed_error_before_touching_storage() {
    let _isolation = isolate_observer();
    for (start, end, destination) in [(4, 3, 0), (0, 9, 0), (0, 2, usize::MAX)] {
        let (resources, _, state) = setup(Limits::default());
        let mut values = [1u32; 8];
        let error = copy_within_with_control(
            &mut values,
            start..end,
            destination,
            &mut RequestControl(&resources),
        )
        .map_err(|error| resources.record(error))
        .unwrap_err();
        assert_eq!(
            error,
            Error::new(FailureKind::Invariant, LimitKind::Capacity)
        );
        assert_eq!(resources.failure(), Some(error));
        assert_eq!(values, [1; 8]);
        assert_eq!(state.reserves.load(Ordering::Relaxed), 0);
        resources.validate_retired().unwrap();
    }
}

#[test]
fn borrowed_stable_sort_matches_original_equal_key_order_without_allocating() {
    let _isolation = isolate_observer();
    for length in [0, 1, 2, 255, 256, 257, 513, 1025] {
        let original = (0..length)
            .map(|i| (((i * 73) % 19) as u32, i))
            .collect::<Vec<_>>();
        let mut expected = original.clone();
        expected.sort_by_key(|value| value.0);
        let mut values = original.clone();
        let mut scratch = original;
        let (resources, _, state) = setup(Limits::default());
        start_alloc_observation();
        stable_sort_by_with_control(
            &mut values,
            &mut scratch,
            &mut RequestControl(&resources),
            &mut |a, b, _| Ok(a.0.cmp(&b.0)),
        )
        .unwrap();
        ENABLED.store(false, Ordering::Relaxed);
        assert_eq!(ALLOCS.load(Ordering::Relaxed), 0);
        assert_eq!(values, expected);
        assert_eq!(
            state.reserves.load(Ordering::Relaxed),
            0,
            "borrowed algorithm owns no scratch"
        );
        resources.validate_retired().unwrap();
    }
}

#[test]
fn stable_sort_preserves_callback_fault_and_stops_on_real_control_failures() {
    let _isolation = isolate_observer();
    let (resources, _, _) = setup(Limits::default());
    let mut values = (0..600u32).rev().collect::<Vec<_>>();
    let mut scratch = values.clone();
    let mut calls = 0;
    let error = stable_sort_by_with_control(
        &mut values,
        &mut scratch,
        &mut RequestControl(&resources),
        &mut |a, b, _| {
            calls += 1;
            if calls == 10 {
                Err(resources.record(Error::native(-902)))
            } else {
                Ok(a.cmp(b))
            }
        },
    )
    .map_err(|error| resources.record(error))
    .unwrap_err();
    assert_eq!(error, Error::native(-902));
    assert_eq!(calls, 10);
    resources.validate_retired().unwrap();

    for ordinal in [2, 40, 300] {
        let (resources, _, state) = setup(Limits::default());
        state.fail_on_checkpoint.store(ordinal, Ordering::Relaxed);
        let mut values = (0..600u32).rev().collect::<Vec<_>>();
        let mut scratch = values.clone();
        assert_eq!(
            stable_sort_by_with_control(
                &mut values,
                &mut scratch,
                &mut RequestControl(&resources),
                &mut |a, b, _| Ok(a.cmp(b))
            )
            .unwrap_err()
            .kind,
            FailureKind::Cancelled
        );
        assert_eq!(state.checkpoint_calls.load(Ordering::Relaxed), ordinal);
        resources.validate_retired().unwrap();
    }
}
