// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE file in the project root for license information.
// ---------------------------------------------------------------------------------------------

use super::*;
use crate::policy::{CancellationToken, SourceBudget, SourceLimits, construction_scope};

struct FaultKernel {
    ignore: bool,
}
impl<'id> BuildKernel<'id> for FaultKernel {
    type Output = ();
    fn run<'scope>(
        &'scope mut self,
        window: &mut BuildWindow<'scope, '_, 'id, '_>,
    ) -> Result<BuildStep<()>, CoreError> {
        if self.ignore {
            let _ = window.fail(CoreError::InvalidProvenance);
            assert_eq!(window.event(), Err(CoreError::InvalidProvenance));
            Ok(BuildStep::Ready(()))
        } else {
            // Deliberately raw: run_window must latch this before guard Drop.
            Err(CoreError::InvalidBoundary)
        }
    }
}
#[test]
fn build_window_latches_typed_error_and_ignored_error_before_drop() {
    for ignore in [false, true] {
        let budget = SourceBudget::task_fixture();
        let mut op = budget.begin(CancellationToken::new());
        construction_scope(&mut op, |mut work| {
            let expected = if ignore {
                CoreError::InvalidProvenance
            } else {
                CoreError::InvalidBoundary
            };
            assert_eq!(
                run_window(&mut FaultKernel { ignore }, &mut work),
                Err(expected)
            );
            assert_eq!(work.first_fault(), Some(expected));
            assert_eq!(BuildString::auxiliary(4, &mut work).err(), Some(expected));
            assert_eq!(work.finish(), Err(expected));
        });
        assert_eq!(budget.usage().auxiliary_capacity, 0);
    }
}

struct PanicKernel;
impl<'id> BuildKernel<'id> for PanicKernel {
    type Output = ();
    fn run<'scope>(
        &'scope mut self,
        _: &mut BuildWindow<'scope, '_, 'id, '_>,
    ) -> Result<BuildStep<()>, CoreError> {
        panic!("test unwind while physical window is open")
    }
}
#[test]
fn build_unclosed_window_unwind_latches_original_work_and_releases_storage() {
    let budget = SourceBudget::task_fixture();
    let mut op = budget.begin(CancellationToken::new());
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        construction_scope(&mut op, |mut work| {
            let mut rows = BuildGrowingVec::new(4, LimitKind::IndexRecords, &mut work).unwrap();
            rows.push(7_u64, &mut work).unwrap();
            let _ = run_window(&mut PanicKernel, &mut work);
        });
    }));
    assert!(result.is_err());
    assert_eq!(op.first_fault(), Some(CoreError::InternalInvariant));
    assert_eq!(budget.usage().auxiliary_capacity, 0);
}

#[test]
fn build_growth_overlaps_real_leases_and_failure_keeps_existing_buffer() {
    let budget = SourceBudget::new(SourceLimits {
        auxiliary_capacity: 16,
        ..SourceLimits::default()
    })
    .unwrap();
    let mut op = budget.begin(CancellationToken::new());
    construction_scope(&mut op, |mut work| {
        let mut rows = BuildGrowingVec::new(4, LimitKind::IndexRecords, &mut work).unwrap();
        rows.push(7_u64, &mut work).unwrap();
        let pointer = rows.as_slice().as_ptr();
        assert_eq!(budget.usage().auxiliary_capacity, 8);
        // The new 16-byte allocation must be admitted while the old 8 bytes live.
        let error = rows.push(8, &mut work).unwrap_err();
        assert_eq!(
            error,
            CoreError::ResourceLimit {
                kind: LimitKind::AuxiliaryCapacity,
                requested: 24,
                limit: 16
            }
        );
        assert_eq!(rows.as_slice(), &[7]);
        assert_eq!(rows.as_slice().as_ptr(), pointer);
        assert_eq!(budget.usage().auxiliary_capacity, 8);
        assert_eq!(rows.push(9, &mut work), Err(error));
        drop(rows);
        assert_eq!(budget.usage().auxiliary_capacity, 0);
        assert_eq!(work.finish(), Err(error));
    });
}

#[test]
fn build_deadline_after_new_allocation_releases_new_lease_and_keeps_old_identity() {
    let budget = SourceBudget::task_fixture();
    let mut op = budget.begin(CancellationToken::new());
    op.set_clock_samples_for_test(&[100_000_000]);
    construction_scope(&mut op, |mut work| {
        let mut rows = BuildGrowingVec::new(4, LimitKind::IndexRecords, &mut work).unwrap();
        rows.push(7_u64, &mut work).unwrap();
        let pointer = rows.as_slice().as_ptr();
        let retained = budget.usage().auxiliary_capacity;
        work.set_clock_samples_for_test(&[100_000_000, 500_000_000]);
        let error = CoreError::DeadlineExceeded {
            elapsed_ns: 500_000_000,
            limit_ns: 500_000_000,
        };
        assert_eq!(rows.push(8, &mut work), Err(error));
        assert_eq!(rows.as_slice(), &[7]);
        assert_eq!(rows.as_slice().as_ptr(), pointer);
        assert_eq!(budget.usage().auxiliary_capacity, retained);
        assert!(budget.usage().auxiliary_capacity_peak > retained);
        assert_eq!(work.first_fault(), Some(error));
        drop(rows);
        assert_eq!(budget.usage().auxiliary_capacity, 0);
        assert_eq!(work.finish(), Err(error));
    });
}

#[test]
fn build_growth_and_freeze_keep_actual_allocation_without_new_admission() {
    let budget = SourceBudget::model_fixture();
    let mut op = budget.begin(CancellationToken::new());
    let frozen = construction_scope(&mut op, |mut work| {
        let mut rows = BuildGrowingVec::new(4, LimitKind::IndexRecords, &mut work).unwrap();
        rows.push(7_u64, &mut work).unwrap();
        rows.push(8, &mut work).unwrap();
        assert!(budget.usage().auxiliary_capacity_peak >= 24);
        let pointer = rows.as_slice().as_ptr();
        let capacity = rows.values.capacity();
        let retained = budget.usage().auxiliary_capacity;
        let peak = budget.usage().auxiliary_capacity_peak;
        let frozen = rows.freeze(&mut work).unwrap();
        assert_eq!(frozen.as_slice().as_ptr(), pointer);
        assert_eq!(frozen.values.capacity(), capacity);
        assert_eq!(budget.usage().auxiliary_capacity, retained);
        assert_eq!(budget.usage().auxiliary_capacity_peak, peak);
        work.finish().unwrap();
        frozen
    });
    assert_eq!(frozen.as_slice(), &[7, 8]);
    drop(frozen);
    assert_eq!(budget.usage().auxiliary_capacity, 0);
}

struct LeaseProbe {
    budget: SourceBudget,
}
impl Drop for LeaseProbe {
    fn drop(&mut self) {
        assert!(self.budget.usage().auxiliary_capacity >= size_of::<Self>() as u64);
    }
}
#[test]
fn build_and_frozen_elements_are_destroyed_while_allocation_lease_is_live() {
    for freeze in [false, true] {
        let budget = SourceBudget::model_fixture();
        let mut op = budget.begin(CancellationToken::new());
        construction_scope(&mut op, |mut work| {
            let mut rows = BuildVec::new(1, &mut work).unwrap();
            rows.push(
                LeaseProbe {
                    budget: budget.clone(),
                },
                &mut work,
            )
            .unwrap();
            if freeze {
                drop(rows.freeze(&mut work).unwrap());
            } else {
                drop(rows);
            }
            assert_eq!(budget.usage().auxiliary_capacity, 0);
            work.finish().unwrap();
        });
    }
}

#[test]
fn build_retired_page_cannot_be_read_or_resurrected_after_first_fault() {
    let budget = SourceBudget::model_fixture();
    let mut op = budget.begin(CancellationToken::new());
    construction_scope(&mut op, |mut work| {
        let mut rows = BuildRows::new(4, LimitKind::IndexRecords, &mut work).unwrap();
        rows.push(7_u64, &mut work).unwrap();
        let before = budget.usage().auxiliary_capacity;
        rows.retire_page(0, &mut work).unwrap();
        assert!(budget.usage().auxiliary_capacity < before);
        assert_eq!(rows.get(0, &mut work), Err(CoreError::InternalInvariant));
        assert_eq!(rows.push(8, &mut work), Err(CoreError::InternalInvariant));
        drop(rows);
        assert_eq!(budget.usage().auxiliary_capacity, 0);
        assert_eq!(work.finish(), Err(CoreError::InternalInvariant));
    });
}

struct Increment<'a, 'id> {
    rows: &'a mut BuildGrowingVec<'id, u32>,
    at: usize,
    len: usize,
    pending: Option<u32>,
}
impl<'id> BuildKernel<'id> for Increment<'_, 'id> {
    type Output = usize;
    fn run<'scope>(
        &'scope mut self,
        window: &mut BuildWindow<'scope, '_, 'id, '_>,
    ) -> Result<BuildStep<usize>, CoreError> {
        let mut rows = match window.write_growing(self.rows)? {
            BuildStep::Ready(rows) => rows,
            BuildStep::Full => return Ok(BuildStep::Full),
            BuildStep::NeedCapacity(_) => unreachable!(),
        };
        while self.at < self.len {
            if let Some(value) = self.pending {
                match rows.replace(self.at, value, window)? {
                    BuildStep::Ready(()) => {
                        self.at += 1;
                        self.pending = None;
                    }
                    BuildStep::Full => return Ok(BuildStep::Full),
                    BuildStep::NeedCapacity(_) => unreachable!(),
                }
            } else {
                match rows.get(self.at, window)? {
                    BuildStep::Ready(Some(value)) => self.pending = Some(value + 1),
                    BuildStep::Ready(None) => return Err(window.fail(CoreError::InvalidProvenance)),
                    BuildStep::Full => return Ok(BuildStep::Full),
                    BuildStep::NeedCapacity(_) => unreachable!(),
                }
            }
        }
        Ok(BuildStep::Ready(self.at))
    }
}
#[test]
fn build_windows_rebind_actual_storage_without_repeating_completed_writes() {
    let budget = SourceBudget::task_fixture();
    let mut op = budget.begin(CancellationToken::new());
    construction_scope(&mut op, |mut work| {
        let mut rows = BuildGrowingVec::new(500, LimitKind::IndexRecords, &mut work).unwrap();
        rows.extend_from_slice(&(0..500).collect::<Vec<u32>>(), &mut work)
            .unwrap();
        let mut kernel = Increment {
            rows: &mut rows,
            at: 0,
            len: 500,
            pending: None,
        };
        let mut count = 0;
        loop {
            count += 1;
            match run_window(&mut kernel, &mut work).unwrap() {
                BuildStep::Ready(done) => {
                    assert_eq!(done, 500);
                    break;
                }
                BuildStep::Full => assert!(count < 20),
                BuildStep::NeedCapacity(_) => unreachable!(),
            }
        }
        assert!(count > 1);
        assert_eq!(rows.as_slice(), &(1..=500).collect::<Vec<u32>>());
        drop(rows);
        work.finish().unwrap();
    });
    assert_eq!(op.stats(), None);
    assert_eq!(budget.usage().auxiliary_capacity, 0);
}

struct CancelAfterWrite<'a, 'id> {
    rows: &'a mut BuildGrowingVec<'id, u32>,
    cancel: CancellationToken,
}
impl<'id> BuildKernel<'id> for CancelAfterWrite<'_, 'id> {
    type Output = ();
    fn run<'scope>(
        &'scope mut self,
        window: &mut BuildWindow<'scope, '_, 'id, '_>,
    ) -> Result<BuildStep<()>, CoreError> {
        let mut rows = match window.write_growing(self.rows)? {
            BuildStep::Ready(rows) => rows,
            _ => panic!("one binding fits"),
        };
        assert_eq!(rows.replace(0, 2, window)?, BuildStep::Ready(()));
        self.cancel.cancel();
        Ok(BuildStep::Ready(()))
    }
}
#[test]
fn build_window_exit_cancellation_rejects_completed_private_write() {
    let budget = SourceBudget::task_fixture();
    let cancel = CancellationToken::new();
    let mut op = budget.begin(cancel.clone());
    construction_scope(&mut op, |mut work| {
        let mut rows = BuildGrowingVec::new(1, LimitKind::IndexRecords, &mut work).unwrap();
        rows.push(1, &mut work).unwrap();
        assert_eq!(
            run_window(
                &mut CancelAfterWrite {
                    rows: &mut rows,
                    cancel
                },
                &mut work
            ),
            Err(CoreError::Cancelled)
        );
        assert_eq!(rows.as_slice(), &[2]);
        assert_eq!(rows.replace(0, 3, &mut work), Err(CoreError::Cancelled));
        assert_eq!(work.first_fault(), Some(CoreError::Cancelled));
        drop(rows);
        assert_eq!(work.finish(), Err(CoreError::Cancelled));
    });
    assert_eq!(budget.usage().auxiliary_capacity, 0);
}

#[test]
fn build_owned_text_chunks_allow_allocation_only_after_the_physical_window() {
    let budget = SourceBudget::task_fixture();
    let mut op = budget.begin(CancellationToken::new());
    construction_scope(&mut op, |mut work| {
        let expected = "é".repeat(200);
        let mut source = BuildGrowingText::new(800, &mut work).unwrap();
        source.append(&expected, &mut work).unwrap();
        let mut target = BuildExactText::new(expected.len(), &mut work).unwrap();
        let mut at = 0;
        while at < expected.len() {
            let BuildStep::Ready(chunk) = run_window(
                &mut TextChunk {
                    text: &source,
                    start: at,
                },
                &mut work,
            )
            .unwrap() else {
                panic!("single finite chunk fits");
            };
            if at == 0 {
                // The output owns its bytes: original storage can now really
                // grow while this value lives, without a stale borrowed view.
                source.append("é", &mut work).unwrap();
            }
            target.append(chunk.as_str(), &mut work).unwrap();
            at += chunk.len();
        }
        let pointer = target.as_str().as_ptr();
        let frozen = target.freeze(&mut work).unwrap();
        assert_eq!(frozen.as_str().as_ptr(), pointer);
        assert_eq!(frozen.as_str(), expected);
        drop(frozen);
        drop(source);
        work.finish().unwrap();
    });
    assert_eq!(budget.usage().auxiliary_capacity, 0);
}

struct TextChunk<'a, 'id> {
    text: &'a BuildGrowingText<'id>,
    start: usize,
}
impl<'id> BuildKernel<'id> for TextChunk<'_, 'id> {
    type Output = BuildTextChunk;
    fn run<'scope>(
        &'scope mut self,
        window: &mut BuildWindow<'scope, '_, 'id, '_>,
    ) -> Result<BuildStep<Self::Output>, CoreError> {
        match window.growing_text(self.text)? {
            BuildStep::Ready(text) => text.chunk_next(self.start, self.text.len(), 5, window),
            BuildStep::Full => Ok(BuildStep::Full),
            BuildStep::NeedCapacity(_) => unreachable!(),
        }
    }
}
#[test]
fn build_owned_chunk_keeps_utf8_boundary_without_speculative_fault() {
    let budget = SourceBudget::task_fixture();
    let mut op = budget.begin(CancellationToken::new());
    let chunk = construction_scope(&mut op, |mut work| {
        let mut text = BuildGrowingText::new(8, &mut work).unwrap();
        text.append("😀é", &mut work).unwrap();
        let BuildStep::Ready(chunk) = run_window(
            &mut TextChunk {
                text: &text,
                start: 0,
            },
            &mut work,
        )
        .unwrap() else {
            panic!("one bounded read fits");
        };
        assert_eq!(chunk.as_str(), "😀");
        let BuildStep::Ready(last) = run_window(
            &mut TextChunk {
                text: &text,
                start: chunk.len(),
            },
            &mut work,
        )
        .unwrap() else {
            panic!("one bounded read fits");
        };
        assert_eq!(last.as_str(), "é");
        assert_eq!(work.first_fault(), None);
        drop(text);
        work.finish().unwrap();
        chunk
    });
    assert_eq!(chunk.as_str(), "😀");
    assert_eq!(budget.usage().auxiliary_capacity, 0);
}

#[test]
fn build_sort_preserves_stability_and_releases_actual_scratch() {
    let budget = SourceBudget::model_fixture();
    let mut op = budget.begin(CancellationToken::new());
    construction_scope(&mut op, |mut work| {
        let mut rows = BuildGrowingVec::new(4, LimitKind::IndexRecords, &mut work).unwrap();
        rows.extend_from_slice(&[(2_u32, 0_u32), (1, 1), (2, 2), (1, 3)], &mut work)
            .unwrap();
        let retained = budget.usage().auxiliary_capacity;
        sort_build_rows_by(&mut rows, 0, |a, b, _| Ok(a.0.cmp(&b.0)), &mut work).unwrap();
        assert_eq!(rows.as_slice(), &[(1, 1), (1, 3), (2, 0), (2, 2)]);
        assert_eq!(budget.usage().auxiliary_capacity, retained);
        assert!(budget.usage().auxiliary_capacity_peak > retained);
        drop(rows);
        work.finish().unwrap();
    });
    assert_eq!(budget.usage().auxiliary_capacity, 0);
}

#[test]
fn build_sort_latches_comparator_error_while_admitted_scratch_is_live() {
    let budget = SourceBudget::model_fixture();
    let mut op = budget.begin(CancellationToken::new());
    construction_scope(&mut op, |mut work| {
        let mut rows = BuildGrowingVec::new(128, LimitKind::IndexRecords, &mut work).unwrap();
        rows.extend_from_slice(&(0..128_u32).rev().collect::<Vec<_>>(), &mut work)
            .unwrap();
        let retained = budget.usage().auxiliary_capacity;
        let mut inside_merge = false;
        let error = sort_build_rows_by(
            &mut rows,
            0,
            |a, b, _| {
                if budget.usage().auxiliary_capacity > retained {
                    inside_merge = true;
                    return Err(CoreError::InvalidBoundary);
                }
                Ok(a.cmp(&b))
            },
            &mut work,
        )
        .unwrap_err();
        assert!(inside_merge);
        assert_eq!(error, CoreError::InvalidBoundary);
        assert_eq!(work.first_fault(), Some(error));
        assert_eq!(budget.usage().auxiliary_capacity, retained);
        assert_eq!(rows.push(129, &mut work), Err(error));
        drop(rows);
        assert_eq!(budget.usage().auxiliary_capacity, 0);
        assert_eq!(work.finish(), Err(error));
    });
}

#[test]
fn build_sort_keeps_original_normal_algorithm_fees_without_metadata_duplicates() {
    let cases = [
        vec![(1_u32, 0_u32), (2, 1), (3, 2), (4, 3)],
        vec![(4, 0), (3, 1), (2, 2), (1, 3)],
        vec![(2, 0), (1, 1), (2, 2), (1, 3)],
        vec![(1, 0), (1, 1), (1, 2), (1, 3)],
    ];
    for input in cases {
        let legacy_budget = SourceBudget::model_fixture();
        let mut legacy_op = legacy_budget.begin(CancellationToken::new());
        let mut legacy = GrowingVec::new(4, LimitKind::IndexRecords, &mut legacy_op).unwrap();
        legacy.extend_from_slice(&input, &mut legacy_op).unwrap();
        sort_by(
            &mut legacy,
            0,
            |a, b, op| {
                op.scan(16)?;
                Ok(a.0.cmp(&b.0))
            },
            &mut legacy_op,
        )
        .unwrap();

        let budget = SourceBudget::model_fixture();
        let mut op = budget.begin(CancellationToken::new());
        let frozen = construction_scope(&mut op, |mut work| {
            let mut rows = BuildGrowingVec::new(4, LimitKind::IndexRecords, &mut work).unwrap();
            rows.extend_from_slice(&input, &mut work).unwrap();
            sort_build_rows_by(
                &mut rows,
                0,
                |a, b, work| {
                    work.scan(16)?;
                    Ok(a.0.cmp(&b.0))
                },
                &mut work,
            )
            .unwrap();
            let frozen = rows.freeze(&mut work).unwrap();
            work.finish().unwrap();
            frozen
        });
        assert_eq!(frozen.as_slice(), legacy.as_slice());
        assert_eq!(op.stats(), legacy_op.stats());
        assert_eq!(
            budget.usage().auxiliary_capacity_peak,
            legacy_budget.usage().auxiliary_capacity_peak
        );
        drop(frozen);
        drop(legacy);
        assert_eq!(budget.usage().auxiliary_capacity, 0);
        assert_eq!(legacy_budget.usage().auxiliary_capacity, 0);
    }
}
