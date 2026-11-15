// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE file in the project root for license information.
// ---------------------------------------------------------------------------------------------

//! Exercise the real private carrier and the original operation's fault latch.

use super::*;
use crate::policy::{CancellationToken, SourceBudget, construction_scope};
use crate::source::SourceDocument;
use direct::DirectHost;
use std::cell::Cell;

fn input(budget: &SourceBudget) -> ParseInput {
    let mut op = budget.begin(CancellationToken::new());
    op.set_clock_samples_for_test(&[100_000_000]);
    let source = SourceDocument::new(SourceKey::new([0x73; 16], 1).unwrap(), b"x", &mut op)
        .unwrap()
        .snapshot();
    ParseInput::prepare(source, &mut op).unwrap()
}

fn assert_released(budget: &SourceBudget) {
    let usage = budget.usage();
    assert_eq!(
        (
            usage.source_capacity,
            usage.auxiliary_capacity,
            usage.source_owners
        ),
        (0, 0, 0)
    );
}

#[test]
fn every_typed_payload_round_trips_through_the_original_latch() {
    const LARGE: u64 = 0x9abc_def0_1234_5678;
    let errors = [
        CoreError::InvalidState,
        CoreError::StaleResult,
        CoreError::InvalidRevision,
        CoreError::WrongDocument,
        CoreError::WrongRevision {
            expected: LARGE,
            actual: u64::MAX,
        },
        CoreError::ResyncRequired {
            expected: LARGE,
            actual: u64::MAX - 1,
        },
        CoreError::WrongBudget,
        CoreError::WrongGeneration {
            expected: LARGE,
            actual: u64::MAX - 2,
        },
        CoreError::UnknownId,
        CoreError::OutsideScope,
        CoreError::InvalidRange,
        CoreError::UnsupportedConfiguration,
        CoreError::InvalidProvenance,
        CoreError::InvalidBoundary,
        CoreError::InvalidEdits,
        CoreError::InvalidUtf8 { offset: LARGE },
        CoreError::InvalidUtf16 { offset: LARGE + 1 },
        CoreError::InvalidEditUtf8 {
            edit_index: LARGE,
            offset: u64::MAX,
        },
        CoreError::ArithmeticOverflow,
        CoreError::ResourceLimit {
            kind: LimitKind::AuxiliaryCapacity,
            requested: u64::MAX,
            limit: LARGE,
        },
        CoreError::OutOfMemory,
        CoreError::Cancelled,
        CoreError::DeadlineExceeded {
            elapsed_ns: u64::MAX,
            limit_ns: LARGE,
        },
        CoreError::ClockFailure,
        CoreError::InternalInvariant,
    ];
    assert_eq!(size_of::<NativeFault<'_, '_>>(), 0);
    assert_eq!(size_of::<Result<(), NativeFault<'_, '_>>>(), 1);
    for error in errors {
        let budget = SourceBudget::model_fixture();
        let input = input(&budget);
        let mut op = budget.begin(CancellationToken::new());
        construction_scope(&mut op, |mut work| {
            let result = with_prepared(&input, &mut work, |_, mut binding| {
                let signal = binding.carry::<()>(Err(error)).unwrap_err();
                assert_eq!(binding.work.first_fault(), Some(error));
                binding.resolve::<()>(Err(signal))
            });
            assert_eq!(result, Err(error));
            assert_eq!(work.finish(), Err(error));
        });
        assert_eq!(op.first_fault(), Some(error));
        drop(op);
        drop(input);
        assert_released(&budget);
    }
}

#[test]
fn carrier_success_adds_no_work_clock_or_cancellation_poll() {
    let budget = SourceBudget::model_fixture();
    let input = input(&budget);
    let cancel = CancellationToken::new();
    let mut op = budget.begin(cancel.clone());
    op.set_clock_samples_for_test(&[100_000_000]);
    construction_scope(&mut op, |mut work| {
        let result = with_prepared(&input, &mut work, |_, mut binding| {
            let stats = binding.work.stats();
            binding.work.set_clock_samples_for_test(&[u64::MAX]);
            cancel.cancel();
            let result = binding.carry(Ok(7));
            assert_eq!(binding.resolve(result), Ok(7));
            assert_eq!(binding.work.first_fault(), None);
            assert_eq!(binding.work.stats(), stats);
            assert_eq!(binding.work.clock_read_count_for_test(), 0);
            Ok(())
        });
        // The unchanged factory success check, rather than the carrier, observes cancellation.
        assert_eq!(result, Err(CoreError::Cancelled));
        assert_eq!(work.clock_read_count_for_test(), 0);
        assert_eq!(work.finish(), Err(CoreError::Cancelled));
    });
    drop(op);
    drop(input);
    assert_released(&budget);
}

#[test]
fn typed_signal_and_repeated_err_finish_never_poll_or_replace_the_first_fault() {
    for prior in [false, true] {
        let budget = SourceBudget::task_fixture();
        let input = input(&budget);
        let cancel = CancellationToken::new();
        let mut op = budget.begin(cancel.clone());
        op.set_clock_samples_for_test(&[100_000_000]);
        let expected = if prior {
            CoreError::WrongRevision {
                expected: u64::MAX,
                actual: 0x1_0000_0001,
            }
        } else {
            CoreError::InvalidBoundary
        };
        construction_scope(&mut op, |mut work| {
            let result = with_prepared(&input, &mut work, |_, mut binding| {
                if prior {
                    let _ = binding.carry::<()>(Err(expected));
                }
                let stats = binding.work.stats();
                binding.work.set_clock_samples_for_test(&[u64::MAX]);
                cancel.cancel();
                let signal = binding
                    .carry::<()>(Err(CoreError::InvalidBoundary))
                    .unwrap_err();
                for _ in 0..3 {
                    let result = binding.finish::<()>(Err(signal));
                    assert_eq!(binding.resolve(result), Err(expected));
                }
                assert_eq!(binding.work.stats(), stats);
                assert_eq!(binding.work.clock_read_count_for_test(), 0);
                binding.resolve::<()>(Err(signal))
            });
            assert_eq!(result, Err(expected));
            assert_eq!(work.check(), Err(expected));
            assert_eq!(work.finish(), Err(expected));
        });
        drop(op);
        drop(input);
        assert_released(&budget);
    }
}

struct Dropped<'a>(&'a Cell<usize>);
impl Drop for Dropped<'_> {
    fn drop(&mut self) {
        self.0.set(self.0.get() + 1);
    }
}

#[test]
fn ignored_signal_prevents_success_values_and_releases_each_value_once() {
    let budget = SourceBudget::model_fixture();
    let input = input(&budget);
    let drops = Cell::new(0);
    let mut op = budget.begin(CancellationToken::new());
    construction_scope(&mut op, |mut work| {
        let result = with_prepared(&input, &mut work, |_, mut binding| {
            let _ = binding.carry::<()>(Err(CoreError::InvalidBoundary));
            assert!(binding.carry(Ok(Dropped(&drops))).is_err());
            assert_eq!(drops.get(), 1);
            assert!(binding.resolve(Ok(Dropped(&drops))).is_err());
            assert_eq!(drops.get(), 2);
            let result = binding.finish(Ok(Dropped(&drops)));
            assert!(binding.resolve(result).is_err());
            assert_eq!(drops.get(), 3);
            Ok(())
        });
        assert_eq!(result, Err(CoreError::InvalidBoundary));
        let retry: Result<(), _> = with_prepared(&input, &mut work, |_, _| {
            panic!("faulted factory reentered")
        });
        assert_eq!(retry, Err(CoreError::InvalidBoundary));
        assert_eq!(work.finish(), Err(CoreError::InvalidBoundary));
    });
    assert_eq!(drops.get(), 3);
    drop(op);
    drop(input);
    assert_released(&budget);
}

#[test]
fn forged_signal_without_a_latch_rejects_without_poll_or_panic() {
    for finish in [false, true] {
        let budget = SourceBudget::task_fixture();
        let input = input(&budget);
        let cancel = CancellationToken::new();
        let mut op = budget.begin(cancel.clone());
        op.set_clock_samples_for_test(&[100_000_000]);
        construction_scope(&mut op, |mut work| {
            let result: Result<(), _> = with_prepared(&input, &mut work, |_, mut binding| {
                let signal = NativeFault {
                    native: PhantomData,
                    construction: PhantomData,
                };
                let stats = binding.work.stats();
                binding.work.set_clock_samples_for_test(&[u64::MAX]);
                cancel.cancel();
                let result: Result<(), _> = if finish {
                    binding.finish(Err(signal))
                } else {
                    Err(signal)
                };
                assert_eq!(binding.resolve(result), Err(CoreError::InternalInvariant));
                assert_eq!(
                    binding.work.first_fault(),
                    Some(CoreError::InternalInvariant)
                );
                assert_eq!(binding.work.stats(), stats);
                assert_eq!(binding.work.clock_read_count_for_test(), 0);
                Err(CoreError::InternalInvariant)
            });
            assert_eq!(result, Err(CoreError::InternalInvariant));
            assert_eq!(work.finish(), Err(CoreError::InternalInvariant));
        });
        drop(op);
        drop(input);
        assert_released(&budget);
    }
}

#[test]
fn actual_host_failures_resolve_the_same_complete_fault() {
    for point in ["work", "admit", "reserve", "actual", "checkpoint"] {
        let budget = if point == "checkpoint" {
            SourceBudget::task_fixture()
        } else {
            SourceBudget::model_fixture()
        };
        let input = input(&budget);
        let mut op = budget.begin(CancellationToken::new());
        op.set_clock_samples_for_test(&[100_000_000]);
        construction_scope(&mut op, |mut work| {
            let result = with_prepared(&input, &mut work, |_, mut binding| {
                let result = match point {
                    "work" => binding.work(n::WorkDelta::Events(u64::MAX)),
                    "admit" => binding.admit(n::NativeAdmission::Record(u64::MAX)),
                    "reserve" => binding.reserve(u64::MAX).map(drop),
                    "actual" => {
                        let result = binding.reserve(64);
                        let mut lease = binding.resolve(result)?;
                        binding.admit_actual(&mut lease, u64::MAX)
                    }
                    "checkpoint" => {
                        binding.work.set_clock_samples_for_test(&[u64::MAX]);
                        binding.checkpoint_now()
                    }
                    _ => unreachable!(),
                };
                assert!(result.is_err(), "{point}");
                let error = binding.resolve(result).unwrap_err();
                assert_eq!(binding.work.first_fault(), Some(error));
                if point == "work" {
                    // One native delta still obeys the original 256-event bound.
                    assert_eq!(error, CoreError::InternalInvariant);
                } else if point == "checkpoint" {
                    assert_eq!(
                        error,
                        CoreError::DeadlineExceeded {
                            elapsed_ns: u64::MAX,
                            limit_ns: 500_000_000
                        }
                    );
                } else {
                    assert!(
                        matches!(
                            error,
                            CoreError::ResourceLimit { .. } | CoreError::ArithmeticOverflow
                        ),
                        "{point}: {error:?}"
                    );
                }
                Err(error)
            });
            assert_eq!(result, Err(work.first_fault().unwrap()));
            assert_eq!(work.finish(), result);
        });
        drop(op);
        drop(input);
        assert_released(&budget);
    }
}
