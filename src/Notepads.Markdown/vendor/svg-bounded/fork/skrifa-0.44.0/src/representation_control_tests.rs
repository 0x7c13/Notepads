// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE file in the project root for license information.
// ---------------------------------------------------------------------------------------------
use super::{storage::SmallBuffer, test_support::*, *};
use markdown_svg_budget::{FailureKind, LimitKind, Limits};

#[derive(Clone, Copy, Default, Debug)]
struct Stop {
    offset: f32,
    index: u32,
}

fn fill<C: FontControl>(cx: &mut C, count: usize) -> Result<C::Small<Stop, 32>, C::Error> {
    let mut stops = cx.small()?;
    for index in 0..count {
        // Finite F2Dot14 values with duplicates, including both signed zeros.
        let offset = match index % 7 {
            0 => -0.0,
            1 => 0.0,
            _ => ((count - index) % 13) as f32 / 16.0 - 0.25,
        };
        stops.try_push(Stop {
            offset,
            index: index as u32,
        })?;
    }
    Ok(stops)
}

fn compare(left: &Stop, right: &Stop) -> core::cmp::Ordering {
    left.offset
        .partial_cmp(&right.offset)
        .unwrap_or(core::cmp::Ordering::Equal)
}

#[test]
fn finite_sort_preserves_every_offset_bit_and_stable_tie_across_spills() {
    for count in [0, 1, 31, 32, 33, 255, 256, 257] {
        let mut plain_cx = Ordinary::new();
        let mut plain = fill(&mut plain_cx, count).unwrap();
        plain_cx
            .stable_sort::<Stop, 32>(&mut plain, &mut compare)
            .unwrap();
        let (r, _) = resources(None, Limits::default());
        let mut cx = RequestControl(&r);
        let mut paid = fill(&mut cx, count).unwrap();
        cx.stable_sort::<Stop, 32>(&mut paid, &mut compare).unwrap();
        let values = |stops: &[Stop]| {
            stops
                .iter()
                .map(|v| (v.offset.to_bits(), v.index))
                .collect::<Vec<_>>()
        };
        assert_eq!(values(&plain), values(&paid), "count={count}");
        drop(paid);
        r.checkpoint(0).unwrap();
        r.validate_retired().unwrap();
    }
}

#[test]
fn sort_allocation_work_and_last_natural_refund_remain_typed_errors() {
    let (r, events) = resources(None, Limits::default());
    let mut cx = RequestControl(&r);
    let mut paid = fill(&mut cx, 257).unwrap();
    let before = {
        let v = events.lock().unwrap();
        (v.reserve, v.commit, v.release, v.positive_work)
    };
    cx.stable_sort::<Stop, 32>(&mut paid, &mut compare).unwrap();
    let work = events.lock().unwrap().positive_work - before.3;
    assert!(work > 256, "exercise actual long merge/copyback work");
    drop(paid);
    r.validate_retired().unwrap();
    for fault in [
        Fault::Reserve(before.0 + 1),
        Fault::Commit(before.1 + 1),
        Fault::Release(before.2 + 1),
        Fault::Work(before.3 + 1),
        Fault::Work(before.3 + work / 2),
        Fault::Work(before.3 + work),
    ] {
        let (r, events) = resources(Some(fault), Limits::default());
        let mut cx = RequestControl(&r);
        let mut paid = fill(&mut cx, 257).unwrap();
        let result = cx.stable_sort::<Stop, 32>(&mut paid, &mut compare);
        assert_eq!(result, Err(FAULT), "{fault:?}");
        assert_eq!(r.failure(), Some(FAULT));
        assert_eq!(
            events.lock().unwrap().injected,
            1,
            "fault must actually hit"
        );
        drop(paid);
        assert_eq!(r.checkpoint(0), Err(FAULT));
        if matches!(fault, Fault::Release(_)) {
            assert!(
                r.statistics().scratch_bytes > 0,
                "failed refund remains charged"
            );
            assert!(events.lock().unwrap().failed_release_bytes > 0);
        } else {
            assert_eq!(r.statistics().scratch_bytes, 0);
        }
    }
}

#[test]
fn cross_request_sort_does_not_compare_and_a_failed_source_cannot_be_revived() {
    for old_source_error in [false, true] {
        let (source, _) = resources(None, Limits::default());
        let (target, events) = resources(None, Limits::default());
        let mut values = fill(&mut RequestControl(&source), 33).unwrap();
        if old_source_error {
            source.record(FAULT);
        }
        let mut comparisons = 0;
        let result = RequestControl(&target).stable_sort::<Stop, 32>(&mut values, &mut |a, b| {
            comparisons += 1;
            compare(a, b)
        });
        let expected = if old_source_error {
            FAULT
        } else {
            markdown_svg_budget::Error::new(FailureKind::Invariant, LimitKind::None)
        };
        assert_eq!(result, Err(expected));
        assert_eq!(comparisons, 0);
        assert_eq!(source.failure(), Some(expected));
        assert_eq!(target.failure(), Some(expected));
        assert_eq!(events.lock().unwrap().reserve, 0);
        drop(values);
    }
}
