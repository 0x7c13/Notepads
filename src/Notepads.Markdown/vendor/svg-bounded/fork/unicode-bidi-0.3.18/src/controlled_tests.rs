// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE file in the project root for license information.
// ---------------------------------------------------------------------------------------------
extern crate std;

use super::*;
use crate::storage::tests::support::{
    checkpoint_error, commit_error, reserve_error, Faults, Fixture, Observation,
};
use alloc::vec::Vec;
use core::cell::Cell;
use markdown_svg_budget::Limits;

struct CountingAsciiSource(Cell<usize>);
impl BidiDataSource for CountingAsciiSource {
    fn bidi_class(&self, ch: char) -> BidiClass {
        self.0.set(self.0.get() + 1);
        match ch {
            '\n' | '\r' => BidiClass::B,
            ' ' => BidiClass::WS,
            '0'..='9' => BidiClass::EN,
            _ => BidiClass::L,
        }
    }
}

#[test]
fn supplied_data_source_borrows_text_and_returns_paid_results() {
    let source = CountingAsciiSource(Cell::new(0));
    let fixture = Fixture::new(Faults::default());
    let text = "a12\nb";
    let observed = Observation::start();
    let info =
        BidiInfo::try_new_with_data_source(&source, text, Some(Level::ltr()), &fixture.resources)
            .unwrap();
    assert_eq!(info.text().as_ptr(), text.as_ptr());
    assert_eq!(info.original_classes().len(), text.len());
    assert_eq!(info.levels().len(), text.len());
    assert_eq!(info.paragraphs().len(), 2);
    assert_eq!(source.0.get(), text.chars().count());
    drop(info);
    observed.finish().assert_freed_before_refund();
    fixture.assert_retired();
}

#[test]
fn constructor_preserves_an_existing_native_first_fault_before_classification() {
    let source = CountingAsciiSource(Cell::new(0));
    let fixture = Fixture::new(Faults::default());
    let first = Error::native(-2_147_024_809);
    fixture.resources.record(first);
    let observed = Observation::start();
    let result = BidiInfo::try_new_with_data_source(&source, "abc", None, &fixture.resources);
    assert_eq!(result.err(), Some(first));
    let order = try_reorder_visual(&[Level::rtl()], &fixture.resources);
    assert_eq!(order.err(), Some(first));
    assert_eq!(source.0.get(), 0);
    assert_eq!(fixture.reserves(), 0);
    assert_eq!(fixture.resources.record(reserve_error()), first);
    observed.finish().assert_freed_before_refund();
    fixture.assert_retired();
}

#[cfg(feature = "hardcoded-data")]
const MIXED: &str = concat!(
    "ab \u{2067}אב 12\u{2069} (A\u{301}) $\u{00ad}$$ +12\n",
    "العربية 12 [cd] \u{2068}x\u{2067}אב\u{2069}\u{2069}\u{2029}",
    "\u{202b}long (אב [12]) +$$\u{202c} tail",
);

#[cfg(feature = "hardcoded-data")]
#[test]
fn whole_paragraph_byte_levels_classes_and_all_orders_match_the_ordinary_core() {
    for base in [None, Some(Level::ltr()), Some(Level::rtl())] {
        // All ordinary allocations happen before observation is enabled.
        let plain = crate::BidiInfo::new(MIXED, base);
        let expected_reordered: Vec<_> = plain
            .paragraphs
            .iter()
            .map(|paragraph| {
                plain.reordered_levels(paragraph, paragraph.range.start..paragraph.range.end)
            })
            .collect();
        let expected_visual: Vec<_> = plain
            .paragraphs
            .iter()
            .map(|paragraph| {
                plain.visual_runs(paragraph, paragraph.range.start..paragraph.range.end)
            })
            .collect();
        let expected_order = crate::BidiInfo::reorder_visual(&plain.levels);
        let fixture = Fixture::new(Faults::default());
        let observed = Observation::start();
        let info = BidiInfo::try_new(MIXED, base, &fixture.resources).unwrap();
        assert_eq!(info.text().as_ptr(), MIXED.as_ptr());
        assert_eq!(info.original_classes(), plain.original_classes.as_slice());
        assert_eq!(info.levels(), plain.levels.as_slice());
        assert_eq!(info.paragraphs(), plain.paragraphs.as_slice());
        for (index, paragraph) in info.paragraphs().iter().enumerate() {
            let levels = info
                .try_reordered_levels(index, paragraph.range.start..paragraph.range.end)
                .unwrap();
            assert_eq!(levels.as_slice(), expected_reordered[index].as_slice());
            assert!(levels.belongs_to(&fixture.resources));
            let visual = info
                .try_visual_runs(index, paragraph.range.start..paragraph.range.end)
                .unwrap();
            assert_eq!(visual.levels(), expected_visual[index].0.as_slice());
            assert_eq!(visual.runs(), expected_visual[index].1.as_slice());
        }
        let order = try_reorder_visual(info.levels(), &fixture.resources).unwrap();
        assert_eq!(order.as_slice(), expected_order.as_slice());
        assert!(order.belongs_to(&fixture.resources));
        drop(order);
        drop(info);
        observed.finish().assert_freed_before_refund();
        fixture.assert_retired();
    }
}

#[cfg(feature = "hardcoded-data")]
#[test]
fn visual_parts_keep_their_payment_after_the_bidi_parent_is_dropped() {
    let plain = crate::BidiInfo::new("abc אב", None);
    let expected = plain.visual_runs(&plain.paragraphs[0], 0..plain.text.len());
    let fixture = Fixture::new(Faults::default());
    let observed = Observation::start();
    let info = BidiInfo::try_new(plain.text, None, &fixture.resources).unwrap();
    let visual = info.try_visual_runs(0, 0..plain.text.len()).unwrap();
    let (levels, runs): (BudgetVec<Level>, BudgetVec<LevelRun>) = visual.into_parts();
    assert!(levels.belongs_to(&fixture.resources));
    assert!(runs.belongs_to(&fixture.resources));
    drop(info);
    assert!(fixture.live() > 0);
    assert_eq!(levels.as_slice(), expected.0.as_slice());
    assert_eq!(runs.as_slice(), expected.1.as_slice());
    let with_both = fixture.live();
    drop(levels);
    assert!(fixture.live() > 0 && fixture.live() < with_both);
    drop(runs);
    observed.finish().assert_freed_before_refund();
    fixture.assert_retired();
}

#[cfg(feature = "hardcoded-data")]
#[test]
fn empty_text_and_empty_map_succeed_but_a_missing_paragraph_is_typed() {
    let fixture = Fixture::new(Faults::default());
    let observed = Observation::start();
    let info = BidiInfo::try_new("", None, &fixture.resources).unwrap();
    assert!(info.original_classes().is_empty());
    assert!(info.levels().is_empty());
    assert!(info.paragraphs().is_empty());
    let order = try_reorder_visual(&[], &fixture.resources).unwrap();
    assert!(order.is_empty());
    assert!(order.belongs_to(&fixture.resources));
    drop(order);
    let before = fixture.reserves();
    assert_eq!(
        info.try_visual_runs(0, 0..0).err(),
        Some(Error::new(FailureKind::Invariant, LimitKind::None))
    );
    assert_eq!(fixture.reserves(), before);
    drop(info);
    observed.finish().assert_freed_before_refund();
    fixture.assert_retired();
}

#[cfg(feature = "hardcoded-data")]
#[test]
fn an_interior_empty_visual_line_keeps_the_existing_one_empty_run_value() {
    let plain = crate::BidiInfo::new("abc", None);
    let expected = plain.visual_runs(&plain.paragraphs[0], 1..1);
    let expected_tail = plain.reordered_levels(&plain.paragraphs[0], 3..3);
    assert_eq!(expected.1, [1..1]);
    let fixture = Fixture::new(Faults::default());
    let observed = Observation::start();
    let info = BidiInfo::try_new("abc", None, &fixture.resources).unwrap();
    let visual = info.try_visual_runs(0, 1..1).unwrap();
    assert_eq!(visual.levels(), expected.0.as_slice());
    assert_eq!(visual.runs(), expected.1.as_slice());
    let tail = info.try_reordered_levels(0, 3..3).unwrap();
    assert_eq!(tail.as_slice(), expected_tail.as_slice());
    drop(tail);
    drop(visual);
    let before = fixture.reserves();
    assert_eq!(
        info.try_visual_runs(0, 3..3).err(),
        Some(Error::new(FailureKind::Invariant, LimitKind::None))
    );
    assert_eq!(fixture.reserves(), before);
    drop(info);
    observed.finish().assert_freed_before_refund();
    fixture.assert_retired();
}

#[cfg(feature = "hardcoded-data")]
#[test]
fn invalid_line_index_order_bounds_and_utf8_boundaries_fail_before_copying() {
    for (index, start, end) in [(3, 0, 1), (0, 2, 1), (0, 0, 20), (0, 2, 3), (1, 0, 1)] {
        // UTF-8 bytes: a[0], א[1..3], newline[3], b[4].
        let fixture = Fixture::new(Faults::default());
        let observed = Observation::start();
        let info = BidiInfo::try_new("aא\nb", None, &fixture.resources).unwrap();
        let before = fixture.reserves();
        let error = info.try_reordered_levels(index, start..end).err();
        assert_eq!(
            error,
            Some(Error::new(FailureKind::Invariant, LimitKind::None))
        );
        assert_eq!(fixture.reserves(), before);
        assert_eq!(info.try_visual_runs(0, 0..1).err(), error);
        assert_eq!(fixture.reserves(), before);
        drop(info);
        observed.finish().assert_freed_before_refund();
        fixture.assert_retired();
    }
}

#[cfg(feature = "hardcoded-data")]
fn whole_request(fixture: &Fixture) -> Result<(), Error> {
    let info = BidiInfo::try_new(MIXED, None, &fixture.resources)?;
    for (index, paragraph) in info.paragraphs().iter().enumerate() {
        let line = paragraph.range.start..paragraph.range.end;
        let adjusted = info.try_reordered_levels(index, line.start..line.end)?;
        let visual = info.try_visual_runs(index, line)?;
        assert_eq!(adjusted.as_slice(), visual.levels());
        let order = try_reorder_visual(visual.levels(), &fixture.resources)?;
        assert_eq!(order.len(), info.text().len());
    }
    Ok(())
}

#[cfg(feature = "hardcoded-data")]
#[test]
fn every_actual_bidi_reserve_and_commit_can_fail_with_its_first_typed_record() {
    let baseline = Fixture::new(Faults::default());
    let observed = Observation::start();
    whole_request(&baseline).unwrap();
    observed.finish().assert_freed_before_refund();
    baseline.assert_retired();
    let (reserves, commits) = (baseline.reserves(), baseline.commits());
    assert!(reserves > 12);
    assert_eq!(reserves, commits);
    for number in 1..=reserves {
        let fixture = Fixture::new(Faults {
            reserve: number,
            ..Faults::default()
        });
        let observed = Observation::start();
        assert_eq!(whole_request(&fixture), Err(reserve_error()));
        observed.finish().assert_freed_before_refund();
        assert_eq!(fixture.reserves(), number);
        assert_eq!(fixture.resources.record(commit_error()), reserve_error());
        fixture.assert_retired();
    }
    for number in 1..=commits {
        let fixture = Fixture::new(Faults {
            commit: number,
            ..Faults::default()
        });
        let observed = Observation::start();
        assert_eq!(whole_request(&fixture), Err(commit_error()));
        observed.finish().assert_freed_before_refund();
        assert_eq!(fixture.commits(), number);
        assert_eq!(fixture.resources.record(checkpoint_error()), commit_error());
        fixture.assert_retired();
    }
    std::eprintln!(
        "BIDI_FAULT_MATRIX family=whole_request reserve_cases={} commit_cases={} each_actual_event_rejected=true actual_free_before_refund=true",
        reserves, commits
    );
}

#[cfg(feature = "hardcoded-data")]
#[test]
fn every_actual_bidi_checkpoint_can_cancel_without_returning_an_owned_result() {
    let baseline = Fixture::new(Faults::default());
    whole_request(&baseline).unwrap();
    baseline.assert_retired();
    let checkpoints = baseline.checkpoints();
    assert!(checkpoints > 20);
    for checkpoint in 1..=checkpoints {
        let fixture = Fixture::new(Faults {
            checkpoint,
            ..Faults::default()
        });
        let observed = Observation::start();
        assert_eq!(whole_request(&fixture), Err(checkpoint_error()));
        observed.finish().assert_freed_before_refund();
        assert_eq!(fixture.checkpoints(), checkpoint);
        assert_eq!(fixture.resources.failure(), Some(checkpoint_error()));
        fixture.assert_retired();
    }
    std::eprintln!(
        "BIDI_FAULT_MATRIX family=whole_request checkpoint_cases={} each_actual_checkpoint_cancelled=true actual_free_before_refund=true",
        checkpoints
    );
}

#[cfg(feature = "hardcoded-data")]
#[test]
fn work_quota_returns_the_original_limit_instead_of_default_success() {
    let mut limits = Limits::default();
    limits.work = 1;
    let fixture = Fixture::with_limits(Faults::default(), limits);
    let observed = Observation::start();
    let result = BidiInfo::try_new(MIXED, None, &fixture.resources);
    assert_eq!(
        result.err(),
        Some(Error::new(FailureKind::Quota, LimitKind::Work))
    );
    observed.finish().assert_freed_before_refund();
    fixture.assert_retired();
}
