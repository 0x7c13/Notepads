// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE file in the project root for license information.
// ---------------------------------------------------------------------------------------------

use crate::error::CoreError;
use crate::identity::SourceSpan;
use crate::ir::{CompactIr, Rows};
use crate::memory::BuildGrowingVec;
use crate::memory::{BuildKernel, BuildStep, BuildWindow, run_window};
use crate::policy::OwnedWork;
use crate::source::SourceSnapshot;
use std::cmp::Ordering;

#[derive(Clone, Copy)]
pub(super) enum SpanPolicy {
    All,
    Parts,
    Domains,
}

#[derive(Clone, Copy)]
enum SpanStage {
    Event,
    Read,
    Validate,
    Check,
}

struct CheckSpans<'a> {
    ir: &'a CompactIr,
    source: &'a SourceSnapshot,
    index: usize,
    end: usize,
    count: usize,
    previous_end: Option<u64>,
    pending: Option<SourceSpan>,
    policy: SpanPolicy,
    stage: SpanStage,
}

impl<'id> BuildKernel<'id> for CheckSpans<'_> {
    type Output = ();

    fn run<'scope>(
        &'scope mut self,
        window: &mut BuildWindow<'scope, '_, 'id, '_>,
    ) -> Result<BuildStep<()>, CoreError> {
        let spans = match self.ir.sources.task_read(window)? {
            BuildStep::Ready(view) => view,
            BuildStep::Full => return Ok(BuildStep::Full),
            BuildStep::NeedCapacity(_) => return Err(window.fail(CoreError::InternalInvariant)),
        };
        let source = match window.source_text(self.source.text())? {
            BuildStep::Ready(view) => view,
            BuildStep::Full => return Ok(BuildStep::Full),
            BuildStep::NeedCapacity(_) => return Err(window.fail(CoreError::InternalInvariant)),
        };
        while self.index < self.end {
            match self.stage {
                SpanStage::Event => {
                    if matches!(window.event()?, BuildStep::Full) {
                        return Ok(BuildStep::Full);
                    }
                    self.stage = SpanStage::Read;
                }
                SpanStage::Read => {
                    self.pending = Some(match spans.get(self.index, window)? {
                        BuildStep::Ready(Some(span)) => span,
                        BuildStep::Ready(None) => {
                            return Err(window.fail(CoreError::InvalidProvenance));
                        }
                        BuildStep::Full => return Ok(BuildStep::Full),
                        BuildStep::NeedCapacity(_) => {
                            return Err(window.fail(CoreError::InternalInvariant));
                        }
                    });
                    self.stage = SpanStage::Validate;
                }
                SpanStage::Validate => {
                    let span = self
                        .pending
                        .ok_or_else(|| window.fail(CoreError::InternalInvariant))?;
                    if matches!(source.validate_span(span, window)?, BuildStep::Full) {
                        return Ok(BuildStep::Full);
                    }
                    self.stage = SpanStage::Check;
                }
                SpanStage::Check => {
                    if matches!(window.advance()?, BuildStep::Full) {
                        return Ok(BuildStep::Full);
                    }
                    let span = self
                        .pending
                        .ok_or_else(|| window.fail(CoreError::InternalInvariant))?;
                    let valid = match self.policy {
                        SpanPolicy::All => true,
                        SpanPolicy::Parts => {
                            self.previous_end.is_none_or(|end| end <= span.start())
                        }
                        SpanPolicy::Domains => {
                            (!span.is_empty() || self.count == 1)
                                && self.previous_end.is_none_or(|end| end < span.start())
                        }
                    };
                    if !valid {
                        return Err(window.fail(CoreError::InvalidProvenance));
                    }
                    self.previous_end = Some(span.end());
                    self.pending = None;
                    self.index += 1;
                    self.stage = SpanStage::Event;
                }
            }
        }
        Ok(BuildStep::Ready(()))
    }
}

pub(super) fn check_spans<'id>(
    ir: &CompactIr,
    source: &SourceSnapshot,
    range: Rows<SourceSpan>,
    policy: SpanPolicy,
    op: &mut OwnedWork<'id, '_>,
) -> Result<(), CoreError> {
    let range = range.indices(ir.sources().len()).map_err(|e| op.fail(e))?;
    let mut cursor = CheckSpans {
        ir,
        source,
        index: range.start,
        end: range.end,
        count: range.len(),
        previous_end: None,
        pending: None,
        policy,
        stage: SpanStage::Event,
    };
    loop {
        match run_window(&mut cursor, op)? {
            BuildStep::Ready(()) => return Ok(()),
            BuildStep::Full => {}
            BuildStep::NeedCapacity(_) => return Err(op.fail(CoreError::InternalInvariant)),
        }
    }
}

struct CheckFlags<'a, 'id> {
    rows: &'a BuildGrowingVec<'id, u8>,
    expected: u8,
    index: usize,
    end: usize,
    event_done: bool,
}

impl<'id> BuildKernel<'id> for CheckFlags<'_, 'id> {
    type Output = ();

    fn run<'scope>(
        &'scope mut self,
        window: &mut BuildWindow<'scope, '_, 'id, '_>,
    ) -> Result<BuildStep<()>, CoreError> {
        let rows = match window.read_growing(self.rows)? {
            BuildStep::Ready(view) => view,
            BuildStep::Full => return Ok(BuildStep::Full),
            BuildStep::NeedCapacity(_) => return Err(window.fail(CoreError::InternalInvariant)),
        };
        while self.index < self.end {
            if !self.event_done {
                if matches!(window.event()?, BuildStep::Full) {
                    return Ok(BuildStep::Full);
                }
                self.event_done = true;
            }
            let value = match rows.get(self.index, window)? {
                BuildStep::Ready(Some(value)) => value,
                BuildStep::Ready(None) => return Err(window.fail(CoreError::InvalidProvenance)),
                BuildStep::Full => return Ok(BuildStep::Full),
                BuildStep::NeedCapacity(_) => return Err(window.fail(CoreError::InternalInvariant)),
            };
            if value != self.expected {
                return Err(window.fail(CoreError::InvalidProvenance));
            }
            self.index += 1;
            self.event_done = false;
        }
        Ok(BuildStep::Ready(()))
    }
}

pub(super) fn check_flags<'id>(
    rows: &BuildGrowingVec<'id, u8>,
    expected: u8,
    op: &mut OwnedWork<'id, '_>,
) -> Result<(), CoreError> {
    let mut cursor = CheckFlags {
        rows,
        expected,
        index: 0,
        end: rows.len(),
        event_done: false,
    };
    loop {
        match run_window(&mut cursor, op)? {
            BuildStep::Ready(()) => return Ok(()),
            BuildStep::Full => {}
            BuildStep::NeedCapacity(_) => return Err(op.fail(CoreError::InternalInvariant)),
        }
    }
}

/// Only offsets survive a closed window. A pair is charged and read once; an
/// unequal pair returns immediately without observing the remaining suffix.
struct CompareBytes<'input> {
    left: &'input [u8],
    right: &'input [u8],
    offset: usize,
}

impl<'id> BuildKernel<'id> for CompareBytes<'_> {
    type Output = Ordering;

    fn run<'scope>(
        &'scope mut self,
        window: &mut BuildWindow<'scope, '_, 'id, '_>,
    ) -> Result<BuildStep<Ordering>, CoreError> {
        let pairs = match window.byte_pairs(self.left, self.right)? {
            BuildStep::Ready(pairs) => pairs,
            BuildStep::Full => return Ok(BuildStep::Full),
            BuildStep::NeedCapacity(_) => {
                return Err(window.fail(CoreError::InternalInvariant));
            }
        };
        while self.offset < self.left.len().min(self.right.len()) {
            match pairs.pair(self.offset, window)? {
                BuildStep::Ready(Ordering::Equal) => self.offset += 1,
                BuildStep::Ready(order) => return Ok(BuildStep::Ready(order)),
                BuildStep::Full => return Ok(BuildStep::Full),
                BuildStep::NeedCapacity(_) => {
                    return Err(window.fail(CoreError::InternalInvariant));
                }
            }
        }
        pairs.lengths(window)
    }
}

pub(super) fn compare_bytes<'id>(
    left: &[u8],
    right: &[u8],
    op: &mut OwnedWork<'id, '_>,
) -> Result<Ordering, CoreError> {
    let mut cursor = CompareBytes {
        left,
        right,
        offset: 0,
    };
    loop {
        match run_window(&mut cursor, op)? {
            BuildStep::Ready(order) => return Ok(order),
            BuildStep::Full => {}
            BuildStep::NeedCapacity(_) => return Err(op.fail(CoreError::InternalInvariant)),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::error::LimitKind;
    use crate::identity::SourceKey;
    use crate::ir::IrBuilder;
    use crate::memory::BuildStep as TaskStep;
    use crate::policy::{CancellationToken, SourceBudget};
    use crate::policy::{Operation, construction_scope};
    use crate::source::SourceDocument;

    #[test]
    fn source_rows_resume_and_keep_independent_boundary_and_order_checks() {
        let budget = SourceBudget::task_fixture();
        let mut op = budget.begin(CancellationToken::new());
        let source = SourceDocument::new(
            SourceKey::new([0x41; 16], 1).unwrap(),
            &vec![b'a'; 2_000],
            &mut op,
        )
        .unwrap()
        .snapshot();
        let mut builder = IrBuilder::new(&mut op).unwrap();
        let spans: Vec<_> = (0..1_000)
            .map(|i| SourceSpan::new(i * 2, i * 2 + 1).unwrap())
            .collect();
        let range = builder.source_parts(&spans, &mut op).unwrap();
        let ir = builder.ir.snapshot(&mut op).unwrap();
        drop(builder);
        let mut cursor = CheckSpans {
            ir: &ir,
            source: &source,
            index: 0,
            end: 1_000,
            count: 1_000,
            previous_end: None,
            pending: None,
            policy: SpanPolicy::Domains,
            stage: SpanStage::Event,
        };
        let mut windows = 0;
        loop {
            windows += 1;
            match run_task(&mut cursor, &mut op).unwrap() {
                TaskStep::Full => assert!(cursor.index < 1_000),
                TaskStep::Ready(()) => break,
                TaskStep::NeedCapacity(_) => unreachable!(),
            }
        }
        assert!(windows > 1);
        assert_eq!(cursor.index, 1_000);
        let actual = construction_scope(&mut op, |mut work| {
            let value = super::super::domains(&ir, &source, range, &mut work).unwrap();
            work.finish().unwrap();
            value
        });
        assert!(actual.iter().eq(spans.iter()));
        let foreign = SourceBudget::task_fixture();
        let mut bad = foreign.begin(CancellationToken::new());
        assert_eq!(
            check_spans(&ir, &source, range, SpanPolicy::All, &mut bad),
            Err(CoreError::WrongBudget)
        );
        let mut fresh = budget.begin(CancellationToken::new());
        check_spans(&ir, &source, range, SpanPolicy::Domains, &mut fresh).unwrap();
        drop((ir, source));
        assert_eq!(budget.usage().auxiliary_capacity, 0);
        assert_eq!(budget.usage().source_capacity, 0);
    }

    #[test]
    fn source_validation_rejects_bad_boundaries_empty_domains_and_overlap_without_poisoning() {
        let budget = SourceBudget::task_fixture();
        let mut op = budget.begin(CancellationToken::new());
        let source = SourceDocument::new(
            SourceKey::new([0x42; 16], 1).unwrap(),
            "a你b".as_bytes(),
            &mut op,
        )
        .unwrap()
        .snapshot();
        let mut builder = IrBuilder::new(&mut op).unwrap();
        let adjacent = builder
            .source_parts(
                &[
                    SourceSpan::new(0, 1).unwrap(),
                    SourceSpan::new(1, 4).unwrap(),
                ],
                &mut op,
            )
            .unwrap();
        let overlap = builder
            .source_parts(
                &[
                    SourceSpan::new(0, 4).unwrap(),
                    SourceSpan::new(1, 4).unwrap(),
                ],
                &mut op,
            )
            .unwrap();
        let bad_boundary = builder
            .source_parts(&[SourceSpan::new(2, 4).unwrap()], &mut op)
            .unwrap();
        let bad_empty = builder
            .source_parts(
                &[
                    SourceSpan::new(0, 0).unwrap(),
                    SourceSpan::new(1, 4).unwrap(),
                ],
                &mut op,
            )
            .unwrap();
        let single_empty = builder
            .source_parts(&[SourceSpan::new(0, 0).unwrap()], &mut op)
            .unwrap();
        let ir = builder.ir.snapshot(&mut op).unwrap();
        drop(builder);
        for (range, policy, expected) in [
            (adjacent, SpanPolicy::Parts, Ok(())),
            (
                adjacent,
                SpanPolicy::Domains,
                Err(CoreError::InvalidProvenance),
            ),
            (
                overlap,
                SpanPolicy::Parts,
                Err(CoreError::InvalidProvenance),
            ),
            (
                bad_boundary,
                SpanPolicy::All,
                Err(CoreError::InvalidBoundary),
            ),
            (
                bad_empty,
                SpanPolicy::Domains,
                Err(CoreError::InvalidProvenance),
            ),
            (single_empty, SpanPolicy::Domains, Ok(())),
        ] {
            let mut attempt = budget.begin(CancellationToken::new());
            assert_eq!(
                check_spans(&ir, &source, range, policy, &mut attempt),
                expected
            );
            let mut fresh = budget.begin(CancellationToken::new());
            check_spans(&ir, &source, adjacent, SpanPolicy::Parts, &mut fresh).unwrap();
        }
        drop((ir, source));
        assert_eq!(budget.usage().auxiliary_capacity, 0);
        assert_eq!(budget.usage().source_capacity, 0);
    }

    #[test]
    fn fixed_flag_validation_crosses_windows_and_rejects_omitted_ownership() {
        let budget = SourceBudget::task_fixture();
        let mut op = budget.begin(CancellationToken::new());
        construction_scope(&mut op, |mut op| {
            let mut rows = BuildGrowingVec::new(1_000, LimitKind::IndexRecords, &mut op).unwrap();
            for _ in 0..1_000 {
                rows.push(1, &mut op).unwrap();
            }
            check_flags(&rows, 1, &mut op).unwrap();
            rows.replace(700, 0, &mut op).unwrap();
            assert_eq!(
                check_flags(&rows, 1, &mut op),
                Err(CoreError::InvalidProvenance)
            );
            assert_eq!(
                rows.replace(700, 1, &mut op),
                Err(CoreError::InvalidProvenance)
            );
            assert_eq!(
                check_flags(&rows, 1, &mut op),
                Err(CoreError::InvalidProvenance)
            );
            drop(rows);
            assert_eq!(budget.usage().auxiliary_capacity, 0);
            let first = op.first_fault();
            assert_eq!(op.finish().err(), first);
        });
    }

    #[test]
    fn compare_first_middle_last_difference_and_lengths_match_lexicographic_order() {
        let cases: &[(&[u8], &[u8])] = &[
            (b"", b""),
            (b"", b"a"),
            (b"a", b""),
            (b"same", b"same"),
            (b"same", b"same-longer"),
            (b"same-longer", b"same"),
            (b"abcd", b"zbcd"),
            (b"abcd", b"abzd"),
            (b"abcd", b"abcz"),
        ];
        for &(left, right) in cases {
            let budget = SourceBudget::task_fixture();
            let mut op = budget.begin(CancellationToken::new());
            assert_eq!(
                compare_bytes(left, right, &mut op).unwrap(),
                left.cmp(right)
            );
            assert_eq!(op.first_fault(), None);
        }
    }

    #[test]
    fn compare_resumes_same_offsets_and_stops_at_first_difference() {
        let left = vec![b'a'; 1_000];
        let mut right = left.clone();
        right[700] = b'b';
        let budget = SourceBudget::task_fixture();
        let mut op = budget.begin(CancellationToken::new());
        let mut cursor = CompareBytes {
            left: &left,
            right: &right,
            offset: 0,
        };
        let mut windows = 0;
        let mut prior = 0;
        loop {
            windows += 1;
            match run_task(&mut cursor, &mut op).unwrap() {
                TaskStep::Full => {
                    assert!(cursor.offset > prior);
                    assert!(cursor.offset < 701);
                    prior = cursor.offset;
                }
                TaskStep::Ready(order) => {
                    assert_eq!(order, Ordering::Less);
                    assert_eq!(cursor.offset, 700);
                    break;
                }
                TaskStep::NeedCapacity(_) => panic!("byte comparison never allocates"),
            }
        }
        assert!(windows > 1);
        for position in [0, 1, 190, 191, 192, 999] {
            let mut right = left.clone();
            right[position] = b'0';
            let mut cursor = CompareBytes {
                left: &left,
                right: &right,
                offset: 0,
            };
            loop {
                match run_task(&mut cursor, &mut op).unwrap() {
                    TaskStep::Full => {}
                    TaskStep::Ready(order) => {
                        assert_eq!(order, Ordering::Greater);
                        assert_eq!(cursor.offset, position);
                        break;
                    }
                    TaskStep::NeedCapacity(_) => unreachable!(),
                }
            }
        }
        assert_eq!(
            compare_bytes(&left, &left[..999], &mut op),
            Ok(Ordering::Greater)
        );
        assert_eq!(compare_bytes(&left, &left, &mut op), Ok(Ordering::Equal));
    }

    #[test]
    fn compare_preserves_first_fault_and_original_cancellation_flag() {
        let budget = SourceBudget::task_fixture();
        let cancel = CancellationToken::new();
        let mut op = budget.begin(cancel.clone());
        let left = [b'a'; 1_000];
        let mut cursor = CompareBytes {
            left: &left,
            right: &left,
            offset: 0,
        };
        assert_eq!(run_task(&mut cursor, &mut op), Ok(TaskStep::Full));
        let offset = cursor.offset;
        cancel.cancel();
        assert_eq!(run_task(&mut cursor, &mut op), Err(CoreError::Cancelled));
        assert_eq!(cursor.offset, offset);
        let first = op.fail(CoreError::ArithmeticOverflow);
        assert_eq!(first, CoreError::Cancelled);
        assert_eq!(compare_bytes(b"a", b"z", &mut op), Err(first));
    }
    fn run_task<K, T: crate::memory::BuildValue>(
        kernel: &mut K,
        op: &mut Operation,
    ) -> Result<BuildStep<T>, CoreError>
    where
        K: for<'id> BuildKernel<'id, Output = T>,
    {
        construction_scope(op, |mut work| {
            let value = run_window(kernel, &mut work)?;
            work.finish()?;
            Ok(value)
        })
    }
    fn check_spans(
        ir: &CompactIr,
        source: &SourceSnapshot,
        rows: Rows<SourceSpan>,
        policy: SpanPolicy,
        op: &mut Operation,
    ) -> Result<(), CoreError> {
        construction_scope(op, |mut work| {
            work.ensure_budget(&ir.budget)?;
            super::check_spans(ir, source, rows, policy, &mut work)?;
            work.finish()
        })
    }
    fn compare_bytes(left: &[u8], right: &[u8], op: &mut Operation) -> Result<Ordering, CoreError> {
        construction_scope(op, |mut work| {
            let value = super::compare_bytes(left, right, &mut work)?;
            work.finish()?;
            Ok(value)
        })
    }
}
