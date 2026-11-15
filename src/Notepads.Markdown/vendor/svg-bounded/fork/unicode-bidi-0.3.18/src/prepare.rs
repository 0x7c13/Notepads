// Copyright 2015 The Servo Project Developers. See the
// COPYRIGHT file at the top-level directory of this distribution.
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.
//
// Modifications Copyright (c) 2026, Jiaqi (0x7c13) Liu.

//! 3.3.3 Preparations for Implicit Processing
//!
//! <http://www.unicode.org/reports/tr9/#Preparations_for_Implicit_Processing>

#[cfg(any(test, not(feature = "smallvec")))]
use alloc::vec::Vec;
use core::cmp::max;
use core::ops::Range;
#[cfg(feature = "smallvec")]
use smallvec::SmallVec;

use super::level::Level;
use super::BidiClass::{self, *};
use crate::storage::{BufferOps, Storage};

/// A maximal substring of characters with the same embedding level.
///
/// Represented as a range of byte indices.
pub type LevelRun = Range<usize>;

#[cfg(feature = "smallvec")]
pub type LevelRunVec = SmallVec<[LevelRun; 8]>;
#[cfg(not(feature = "smallvec"))]
pub type LevelRunVec = Vec<LevelRun>;

/// Output of `isolating_run_sequences` (steps X9-X10)
#[cfg(test)]
#[derive(Debug, PartialEq)]
pub struct IsolatingRunSequence {
    pub runs: Vec<LevelRun>,
    pub sos: BidiClass, // Start-of-sequence type.
    pub eos: BidiClass, // End-of-sequence type.
}

#[cfg(all(test, feature = "smallvec"))]
pub type IsolatingRunSequenceVec = SmallVec<[IsolatingRunSequence; 8]>;
#[cfg(all(test, not(feature = "smallvec")))]
pub type IsolatingRunSequenceVec = Vec<IsolatingRunSequence>;

/// One IRS with storage supplied by the same request as the algorithm.
pub(crate) struct Sequence<C: Storage> {
    pub(crate) runs: C::Buffer<LevelRun>,
    pub(crate) sos: BidiClass,
    pub(crate) eos: BidiClass,
}

impl<C: Storage> Sequence<C> {
    pub(crate) fn iter_forwards_from(
        &self,
        pos: usize,
        level_run_index: usize,
    ) -> impl Iterator<Item = usize> + '_ {
        let runs = &self.runs[level_run_index..];
        #[cfg(feature = "std")]
        debug_assert!(runs[0].start <= pos && pos <= runs[0].end);
        (pos..runs[0].end).chain(runs[1..].iter().map(|r| r.start..r.end).flatten())
    }

    pub(crate) fn iter_backwards_from(
        &self,
        pos: usize,
        level_run_index: usize,
    ) -> impl Iterator<Item = usize> + '_ {
        let prev_runs = &self.runs[..level_run_index];
        let current = &self.runs[level_run_index];
        #[cfg(feature = "std")]
        debug_assert!(current.start <= pos && pos <= current.end);
        (current.start..pos)
            .rev()
            .chain(prev_runs.iter().rev().map(|r| r.start..r.end).flatten())
    }
}

/// Find a non-X9 code unit, paying before every dynamic search step.
fn non_removed_in<C: Storage>(
    classes: &[BidiClass],
    range: Range<usize>,
    reverse: bool,
    cx: &mut C,
) -> Result<Option<usize>, C::Error> {
    if reverse {
        for index in range.rev() {
            cx.before_events(1)?;
            if not_removed_by_x9(&classes[index]) {
                return Ok(Some(index));
            }
        }
    } else {
        for index in range {
            cx.before_events(1)?;
            if not_removed_by_x9(&classes[index]) {
                return Ok(Some(index));
            }
        }
    }
    Ok(None)
}

/// Compute X9-X10 once for both ordinary and request-owned storage.
pub(crate) fn isolating_run_sequences_in<C: Storage>(
    para_level: Level,
    original_classes: &[BidiClass],
    levels: &[Level],
    mut runs: C::Buffer<LevelRun>,
    has_isolate_controls: bool,
    out: &mut C::Buffer<Sequence<C>>,
    cx: &mut C,
) -> Result<(), C::Error> {
    if !has_isolate_controls {
        out.try_reserve_exact(runs.len())?;
        let mut run_drain = runs.drain_all();
        while run_drain.len() != 0 {
            cx.before_events(1)?;
            let run = run_drain.next().unwrap();
            let seq_level =
                levels[non_removed_in(original_classes, run.start..run.end, false, cx)?
                    .unwrap_or(run.start)];
            let end_level = levels[non_removed_in(original_classes, run.start..run.end, true, cx)?
                .unwrap_or(run.end - 1)];
            let pred_level = non_removed_in(original_classes, 0..run.start, true, cx)?
                .map_or(para_level, |idx| levels[idx]);
            let succ_level =
                non_removed_in(original_classes, run.end..original_classes.len(), false, cx)?
                    .map_or(para_level, |idx| levels[idx]);
            let mut sequence_runs = cx.buffer::<LevelRun>();
            cx.before_events(1)?;
            sequence_runs.try_push(run)?;
            cx.before_events(1)?;
            out.try_push(Sequence {
                runs: sequence_runs,
                sos: max(seq_level, pred_level).bidi_class(),
                eos: max(end_level, succ_level).bidi_class(),
            })?;
        }
        return Ok(());
    }

    let mut sequences = cx.buffer::<C::Buffer<LevelRun>>();
    sequences.try_reserve_exact(runs.len())?;
    let mut stack = cx.buffer::<C::Buffer<LevelRun>>();
    cx.before_events(1)?;
    stack.try_push(cx.buffer::<LevelRun>())?;
    {
        let mut run_drain = runs.drain_all();
        while run_drain.len() != 0 {
            cx.before_events(1)?;
            let run = run_drain.next().unwrap();
            assert!(!run.is_empty());
            assert!(!stack.is_empty());
            let start_class = original_classes[run.start];
            let end_class = non_removed_in(original_classes, run.start..run.end, true, cx)?
                .map_or(start_class, |idx| original_classes[idx]);
            let mut sequence = if start_class == PDI && stack.len() > 1 {
                cx.before_events(1)?;
                stack.pop().unwrap()
            } else {
                cx.buffer::<LevelRun>()
            };
            cx.before_events(1)?;
            sequence.try_push(run)?;
            cx.before_events(1)?;
            if matches!(end_class, RLI | LRI | FSI) {
                stack.try_push(sequence)?;
            } else {
                sequences.try_push(sequence)?;
            }
        }
    }
    {
        // Drain keeps the actual outer storage and its fee live during moves.
        let mut remaining = stack.drain_all().rev();
        while remaining.len() != 0 {
            cx.before_events(1)?;
            let sequence = remaining.next().unwrap();
            if !sequence.is_empty() {
                cx.before_events(1)?;
                sequences.try_push(sequence)?;
            }
        }
    }
    {
        let mut sequence_drain = sequences.drain_all();
        while sequence_drain.len() != 0 {
            cx.before_events(1)?;
            let sequence = sequence_drain.next().unwrap();
            assert!(!sequence.is_empty());
            let start_of_seq = sequence[0].start;
            let runs_len = sequence.len();
            let end_of_seq = sequence[runs_len - 1].end;
            let mut result = Sequence::<C> {
                runs: sequence,
                sos: L,
                eos: L,
            };

            let mut first = start_of_seq;
            let mut forwards = result.iter_forwards_from(start_of_seq, 0);
            while let Some(idx) = crate::next_in(&mut forwards, cx)? {
                if not_removed_by_x9(&original_classes[idx]) {
                    first = idx;
                    break;
                }
            }
            drop(forwards);
            let seq_level = levels[first];
            let mut last = end_of_seq - 1;
            let mut backwards = result.iter_backwards_from(end_of_seq, runs_len - 1);
            while let Some(idx) = crate::next_in(&mut backwards, cx)? {
                if not_removed_by_x9(&original_classes[idx]) {
                    last = idx;
                    break;
                }
            }
            drop(backwards);
            let end_level = levels[last];

            #[cfg(test)]
            for run in result.runs.iter() {
                for idx in run.start..run.end {
                    cx.before_events(1)?;
                    if not_removed_by_x9(&original_classes[idx]) {
                        assert_eq!(seq_level, levels[idx]);
                    }
                }
            }

            let pred_level = non_removed_in(original_classes, 0..start_of_seq, true, cx)?
                .map_or(para_level, |idx| levels[idx]);
            let last_non_removed = non_removed_in(original_classes, 0..end_of_seq, true, cx)?
                .map_or(BN, |idx| original_classes[idx]);
            let succ_level = if matches!(last_non_removed, RLI | LRI | FSI) {
                para_level
            } else {
                non_removed_in(
                    original_classes,
                    end_of_seq..original_classes.len(),
                    false,
                    cx,
                )?
                .map_or(para_level, |idx| levels[idx])
            };
            result.sos = max(seq_level, pred_level).bidi_class();
            result.eos = max(end_level, succ_level).bidi_class();
            cx.before_events(1)?;
            out.try_push(result)?;
        }
    }
    Ok(())
}

// Preserve the original internal unit-test surface without a second algorithm.
#[cfg(test)]
fn isolating_run_sequences(
    para_level: Level,
    original_classes: &[BidiClass],
    levels: &[Level],
    runs: LevelRunVec,
    has_isolate_controls: bool,
    out: &mut IsolatingRunSequenceVec,
) {
    let mut sequences = Vec::new();
    isolating_run_sequences_in(
        para_level,
        original_classes,
        levels,
        runs.into_iter().collect(),
        has_isolate_controls,
        &mut sequences,
        &mut crate::storage::NoControl,
    )
    .unwrap_or_else(|never| match never {});
    out.extend(sequences.into_iter().map(|seq| IsolatingRunSequence {
        runs: seq.runs,
        sos: seq.sos,
        eos: seq.eos,
    }));
}

/// Finds the level runs in a paragraph.
///
/// <http://www.unicode.org/reports/tr9/#BD7>
///
/// This is only used by tests; normally level runs are identified during explicit::compute.
#[cfg(test)]
fn level_runs(levels: &[Level], original_classes: &[BidiClass]) -> Vec<LevelRun> {
    assert_eq!(levels.len(), original_classes.len());

    let mut runs = Vec::new();
    if levels.is_empty() {
        return runs;
    }

    let mut current_run_level = levels[0];
    let mut current_run_start = 0;
    for i in 1..levels.len() {
        if !removed_by_x9(original_classes[i]) && levels[i] != current_run_level {
            // End the last run and start a new one.
            runs.push(current_run_start..i);
            current_run_level = levels[i];
            current_run_start = i;
        }
    }
    runs.push(current_run_start..levels.len());

    runs
}

/// Should this character be ignored in steps after X9?
///
/// <http://www.unicode.org/reports/tr9/#X9>
pub fn removed_by_x9(class: BidiClass) -> bool {
    matches!(class, RLE | LRE | RLO | LRO | PDF | BN)
}

// For use as a predicate for `position` / `rposition`
pub fn not_removed_by_x9(class: &BidiClass) -> bool {
    !removed_by_x9(*class)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_level_runs() {
        assert_eq!(level_runs(&Level::vec(&[]), &[]), &[]);
        assert_eq!(
            level_runs(&Level::vec(&[0, 0, 0, 1, 1, 2, 0, 0]), &[L; 8]),
            &[0..3, 3..5, 5..6, 6..8]
        );
    }

    // From <http://www.unicode.org/reports/tr9/#BD13>
    #[rustfmt::skip]
    #[test]
    fn test_isolating_run_sequences() {

        // == Example 1 ==
        // text1·RLE·text2·PDF·RLE·text3·PDF·text4
        // index        0    1  2    3    4  5    6  7
        let classes = &[L, RLE, L, PDF, RLE, L, PDF, L];
        let levels =  &[0,   1, 1,   1,   1, 1,   1, 0];
        let para_level = Level::ltr();
        let mut sequences = IsolatingRunSequenceVec::new();
        isolating_run_sequences(
            para_level,
            classes,
            &Level::vec(levels),
            level_runs(&Level::vec(levels), classes).into(),
            false,
            &mut sequences);
        sequences.sort_by(|a, b| a.runs[0].clone().cmp(b.runs[0].clone()));
        assert_eq!(
            sequences.iter().map(|s| s.runs.clone()).collect::<Vec<_>>(),
            vec![vec![0..2], vec![2..7], vec![7..8]]
        );

        // == Example 2 ==
        // text1·RLI·text2·PDI·RLI·text3·PDI·text4
        // index        0    1  2    3    4  5    6  7
        let classes = &[L, RLI, L, PDI, RLI, L, PDI, L];
        let levels =  &[0,   0, 1,   0,   0, 1,   0, 0];
        let para_level = Level::ltr();
        let mut sequences = IsolatingRunSequenceVec::new();
        isolating_run_sequences(
            para_level,
            classes,
            &Level::vec(levels),
            level_runs(&Level::vec(levels), classes).into(),
            true,
            &mut sequences);
        sequences.sort_by(|a, b| a.runs[0].clone().cmp(b.runs[0].clone()));
        assert_eq!(
            sequences.iter().map(|s| s.runs.clone()).collect::<Vec<_>>(),
            vec![vec![0..2, 3..5, 6..8], vec![2..3], vec![5..6]]
        );

        // == Example 3 ==
        // text1·RLI·text2·LRI·text3·RLE·text4·PDF·text5·PDI·text6·PDI·text7
        // index        0    1  2    3  4    5  6    7  8    9  10  11  12
        let classes = &[L, RLI, L, LRI, L, RLE, L, PDF, L, PDI, L, PDI,  L];
        let levels =  &[0,   0, 1,   1, 2,   3, 3,   3, 2,   1, 1,   0,  0];
        let para_level = Level::ltr();
        let mut sequences = IsolatingRunSequenceVec::new();
        isolating_run_sequences(
            para_level,
            classes,
            &Level::vec(levels),
            level_runs(&Level::vec(levels), classes).into(),
            true,
            &mut sequences);
        sequences.sort_by(|a, b| a.runs[0].clone().cmp(b.runs[0].clone()));
        assert_eq!(
            sequences.iter().map(|s| s.runs.clone()).collect::<Vec<_>>(),
            vec![vec![0..2, 11..13], vec![2..4, 9..11], vec![4..6], vec![6..8], vec![8..9]]
        );
    }

    // From <http://www.unicode.org/reports/tr9/#X10>
    #[rustfmt::skip]
    #[test]
    fn test_isolating_run_sequences_sos_and_eos() {

        // == Example 1 ==
        // text1·RLE·text2·LRE·text3·PDF·text4·PDF·RLE·text5·PDF·text6
        // index        0    1  2    3  4    5  6    7    8  9   10  11
        let classes = &[L, RLE, L, LRE, L, PDF, L, PDF, RLE, L, PDF,  L];
        let levels =  &[0,   1, 1,   2, 2,   2, 1,   1,   1, 1,   1,  0];
        let para_level = Level::ltr();
        let mut sequences = IsolatingRunSequenceVec::new();
        isolating_run_sequences(
            para_level,
            classes,
            &Level::vec(levels),
            level_runs(&Level::vec(levels), classes).into(),
            false,
            &mut sequences);
        sequences.sort_by(|a, b| a.runs[0].clone().cmp(b.runs[0].clone()));

        // text1
        assert_eq!(
            &sequences[0],
            &IsolatingRunSequence {
                runs: vec![0..2],
                sos: L,
                eos: R,
            }
        );

        // text2
        assert_eq!(
            &sequences[1],
            &IsolatingRunSequence {
                runs: vec![2..4],
                sos: R,
                eos: L,
            }
        );

        // text3
        assert_eq!(
            &sequences[2],
            &IsolatingRunSequence {
                runs: vec![4..6],
                sos: L,
                eos: L,
            }
        );

        // text4 text5
        assert_eq!(
            &sequences[3],
            &IsolatingRunSequence {
                runs: vec![6..11],
                sos: L,
                eos: R,
            }
        );

        // text6
        assert_eq!(
            &sequences[4],
            &IsolatingRunSequence {
                runs: vec![11..12],
                sos: R,
                eos: L,
            }
        );

        // == Example 2 ==
        // text1·RLI·text2·LRI·text3·PDI·text4·PDI·RLI·text5·PDI·text6
        // index        0    1  2    3  4    5  6    7    8  9   10  11
        let classes = &[L, RLI, L, LRI, L, PDI, L, PDI, RLI, L, PDI,  L];
        let levels =  &[0,   0, 1,   1, 2,   1, 1,   0,   0, 1,   0,  0];
        let para_level = Level::ltr();
        let mut sequences = IsolatingRunSequenceVec::new();
        isolating_run_sequences(
            para_level,
            classes,
            &Level::vec(levels),
            level_runs(&Level::vec(levels), classes).into(),
            true,
            &mut sequences);
        sequences.sort_by(|a, b| a.runs[0].clone().cmp(b.runs[0].clone()));

        // text1·RLI·PDI·RLI·PDI·text6
        assert_eq!(
            &sequences[0],
            &IsolatingRunSequence {
                runs: vec![0..2, 7..9, 10..12],
                sos: L,
                eos: L,
            }
        );

        // text2·LRI·PDI·text4
        assert_eq!(
            &sequences[1],
            &IsolatingRunSequence {
                runs: vec![2..4, 5..7],
                sos: R,
                eos: R,
            }
        );

        // text3
        assert_eq!(
            &sequences[2],
            &IsolatingRunSequence {
                runs: vec![4..5],
                sos: L,
                eos: L,
            }
        );

        // text5
        assert_eq!(
            &sequences[3],
            &IsolatingRunSequence {
                runs: vec![9..10],
                sos: R,
                eos: R,
            }
        );
    }

    #[test]
    fn test_removed_by_x9() {
        let rem_classes = &[RLE, LRE, RLO, LRO, PDF, BN];
        let not_classes = &[L, RLI, AL, LRI, PDI];
        for x in rem_classes {
            assert_eq!(removed_by_x9(*x), true);
        }
        for x in not_classes {
            assert_eq!(removed_by_x9(*x), false);
        }
    }

    #[test]
    fn test_not_removed_by_x9() {
        let non_x9_classes = &[L, R, AL, EN, ES, ET, AN, CS, NSM, B, S, WS, ON, LRI, RLI, FSI, PDI];
        for x in non_x9_classes {
            assert_eq!(not_removed_by_x9(&x), true);
        }
    }
}
