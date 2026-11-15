// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE file in the project root for license information.
// ---------------------------------------------------------------------------------------------
//! Structural admission is an atomic cumulative pair, not an allocation lease.

use super::*;

fn counts(resources: &ResourcesRef) -> (u64, u64) {
    let stats = resources.statistics();
    (stats.expanded_nodes, stats.converted_segments)
}

#[test]
fn default_scene_limits_admit_both_boundaries_and_reject_the_next_unit() {
    let _isolation = isolate_observer();
    let limits = Limits::default();
    assert_eq!(limits.expanded_nodes, 131_072);
    assert_eq!(limits.converted_segments, 1_048_576);
    for dimension in [LimitKind::ExpandedNodes, LimitKind::ConvertedSegments] {
        let (resources, _, state) = setup(limits);
        resources.admit_scene(131_071, 1_048_575).unwrap();
        assert_eq!(counts(&resources), (131_071, 1_048_575));
        resources.admit_scene(1, 1).unwrap();
        assert_eq!(counts(&resources), (131_072, 1_048_576));
        let extra = if dimension == LimitKind::ExpandedNodes {
            (1, 0)
        } else {
            (0, 1)
        };
        assert_eq!(
            resources.admit_scene(extra.0, extra.1),
            Err(Error::new(FailureKind::Quota, dimension))
        );
        assert_eq!(counts(&resources), (131_072, 1_048_576));
        assert_eq!(state.reserves.load(Ordering::Relaxed), 0);
        resources.validate_retired().unwrap();
        assert_eq!(counts(&resources), (131_072, 1_048_576));
    }
}

#[test]
fn rejected_pair_keeps_both_counters_and_the_first_dimension_error() {
    let _isolation = isolate_observer();
    for requested in [(4, 1), (1, 10), (4, 10)] {
        let (resources, _, _) = setup(Limits {
            expanded_nodes: 5,
            converted_segments: 11,
            ..Limits::default()
        });
        resources.admit_scene(2, 3).unwrap();
        let dimension = if requested.0 > 3 {
            LimitKind::ExpandedNodes
        } else {
            LimitKind::ConvertedSegments
        };
        let first = Error::new(FailureKind::Quota, dimension);
        assert_eq!(resources.admit_scene(requested.0, requested.1), Err(first));
        assert_eq!(counts(&resources), (2, 3));
        assert_eq!(resources.admit_scene(0, 0), Err(first));
        assert_eq!(resources.record(Error::native(-901)), first);
    }
}

#[test]
fn full_u32_limits_and_u64_addition_overflow_never_wrap_or_partially_commit() {
    let _isolation = isolate_observer();
    for requested in [(u64::MAX, 0), (0, u64::MAX)] {
        let (resources, _, _) = setup(Limits {
            expanded_nodes: u32::MAX,
            converted_segments: u32::MAX,
            ..Limits::default()
        });
        resources
            .admit_scene(u64::from(u32::MAX), u64::from(u32::MAX))
            .unwrap();
        let before = counts(&resources);
        let dimension = if requested.0 != 0 {
            LimitKind::ExpandedNodes
        } else {
            LimitKind::ConvertedSegments
        };
        assert_eq!(
            resources.admit_scene(requested.0, requested.1),
            Err(Error::new(FailureKind::Quota, dimension))
        );
        assert_eq!(counts(&resources), before);
    }
}

#[test]
fn scene_admission_reads_cancel_native_and_preexisting_first_fault_before_commit() {
    let _isolation = isolate_observer();
    for error in [
        Error::new(FailureKind::Cancelled, LimitKind::Work),
        Error::native(-902),
        Error::new(FailureKind::Decode, LimitKind::XmlNodes),
    ] {
        let (resources, slot, state) = setup(Limits::default());
        resources.admit_scene(1, 4).unwrap();
        if error.kind == FailureKind::Cancelled {
            state.fail_on_checkpoint.store(2, Ordering::Relaxed);
        } else if error.kind == FailureKind::Native {
            state
                .failure
                .store(error.native_hr.unwrap() as u32 as u64, Ordering::Relaxed);
        } else {
            slot.record(error);
        }
        assert_eq!(resources.admit_scene(1, 4), Err(error));
        assert_eq!(counts(&resources), (1, 4));
        assert_eq!(resources.failure(), Some(error));
        assert_eq!(resources.admit_scene(0, 0), Err(error));
    }
}

#[test]
fn empty_scene_admission_is_allocation_free_and_not_a_capacity_reservation() {
    let _isolation = isolate_observer();
    let (resources, _, state) = setup(Limits::default());
    let before = ALLOCS.load(Ordering::Relaxed);
    ENABLED.store(true, Ordering::Relaxed);
    resources.admit_scene(0, 0).unwrap();
    resources.admit_scene(2, 9).unwrap();
    resources.admit_scene(3, 7).unwrap();
    ENABLED.store(false, Ordering::Relaxed);
    assert_eq!(ALLOCS.load(Ordering::Relaxed), before);
    assert_eq!(counts(&resources), (5, 16));
    assert_eq!(state.reserves.load(Ordering::Relaxed), 0);
    assert_eq!(state.admissions.load(Ordering::Relaxed), 0);
    assert_eq!(state.releases.load(Ordering::Relaxed), 0);
    assert_eq!(resources.statistics().work, 0);
    assert_eq!(state.checkpoint_calls.load(Ordering::Relaxed), 3);
    resources.validate_retired().unwrap();
}

#[test]
fn scene_totals_follow_the_request_while_other_requests_remain_independent() {
    let _isolation = isolate_observer();
    let (first, _, _) = setup(Limits::default());
    let (second, _, _) = setup(Limits::default());
    let alias = first.clone();
    first.admit_scene(3, 20).unwrap();
    alias.admit_scene(4, 30).unwrap();
    second.admit_scene(1, 2).unwrap();
    assert_eq!(counts(&first), (7, 50));
    assert_eq!(counts(&second), (1, 2));
    drop(alias);
    first.validate_retired().unwrap();
    second.validate_retired().unwrap();
}

#[test]
fn concurrent_scene_admissions_publish_only_complete_pairs() {
    let _isolation = isolate_observer();
    let (resources, _, _) = setup(Limits::default());
    let done = AtomicBool::new(false);
    std::thread::scope(|scope| {
        scope.spawn(|| {
            while !done.load(Ordering::Acquire) {
                let (nodes, segments) = counts(&resources);
                assert_eq!(segments, nodes * 7);
            }
        });
        let writers: Vec<_> = (0..4)
            .map(|_| {
                scope.spawn(|| {
                    for _ in 0..2_048 {
                        resources.admit_scene(1, 7).unwrap();
                    }
                })
            })
            .collect();
        let mut writer_failed = false;
        for writer in writers {
            writer_failed |= writer.join().is_err();
        }
        done.store(true, Ordering::Release);
        assert!(!writer_failed, "a scene-admission writer failed");
    });
    assert_eq!(counts(&resources), (8_192, 57_344));
    resources.validate_retired().unwrap();
}

#[test]
fn structural_error_dimensions_round_trip_without_accepting_unknown_codes() {
    let _isolation = isolate_observer();
    for dimension in [LimitKind::ExpandedNodes, LimitKind::ConvertedSegments] {
        let error = Error::new(FailureKind::Quota, dimension);
        assert_eq!(Error::unpack(error.packed()), Some(error));
        let slot = FirstFailureSlot::new();
        assert_eq!(slot.record(error), error);
        assert_eq!(slot.get(), Some(error));
    }
    let unknown = FailureKind::Quota as u64 | (u64::from(u16::MAX) << 8);
    assert_eq!(
        Error::unpack(unknown),
        Some(Error::new(FailureKind::Invariant, LimitKind::None))
    );
}
