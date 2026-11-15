// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE file in the project root for license information.
// ---------------------------------------------------------------------------------------------

//! Real core quota/lease integration; the vendor owner tests separately target
//! exact page-directory partial moves. No product fault-injection hook exists.

use super::*;
use crate::config::{ParseConfig, Profile};
use crate::memory::BuildRows;
use crate::model::AcceptedModel;
use crate::policy::{
    CancellationToken, ModelLimits, SourceBudget, SourceLimits, construction_scope,
};
use crate::reader::MainReader;
use crate::source::SourceDocument;
use std::panic::{AssertUnwindSafe, catch_unwind};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum Point {
    Reserve,
    Actual,
    SparseCommit,
}
#[derive(Clone, Copy, Debug)]
enum Action {
    Cancel,
    Panic,
}
#[derive(Clone, Copy)]
struct Injection {
    point: Point,
    ordinal: usize,
    action: Action,
    prior_fault: bool,
}
#[derive(Default)]
struct Trace {
    reserve_ok: usize,
    actual_ok: usize,
    sparse_commit_ok: usize,
    reserve_error: Option<CoreError>,
    actual_error: Option<CoreError>,
    reached: bool,
    request: u64,
    live_at_trigger: u64,
}

struct CheckedHost<'trace, 'native, 'source, 'work, 'id, 'op> {
    binding: PreparedBinding<'native, 'source, 'work, 'id, 'op>,
    trace: &'trace mut Trace,
    injection: Option<Injection>,
    token: &'trace CancellationToken,
    budget: &'trace SourceBudget,
}
impl CheckedHost<'_, '_, '_, '_, '_, '_> {
    fn completed(&mut self, point: Point, bytes: u64) {
        let ordinal = match point {
            Point::Reserve => {
                self.trace.reserve_ok += 1;
                self.trace.reserve_ok
            }
            Point::Actual => {
                self.trace.actual_ok += 1;
                self.trace.actual_ok
            }
            Point::SparseCommit => {
                self.trace.sparse_commit_ok += 1;
                self.trace.sparse_commit_ok
            }
        };
        if let Some(injection) = self.injection
            && injection.point == point
            && injection.ordinal == ordinal
        {
            assert!(!self.trace.reached);
            self.trace.reached = true;
            self.trace.request = bytes;
            self.trace.live_at_trigger = self.budget.usage().auxiliary_capacity;
            if injection.prior_fault {
                self.binding.reject(n::NativeViolation::InvalidProvenance);
            }
            match injection.action {
                Action::Cancel => self.token.cancel(),
                Action::Panic => panic!("prepared-resource-after-success"),
            }
        }
    }
}
impl<'native, 'id> Host for CheckedHost<'_, 'native, '_, '_, 'id, '_> {
    type Error = NativeFault<'native, 'id>;
    type Lease = NativeLease<'id>;

    fn work(&mut self, delta: n::WorkDelta) -> Result<(), Self::Error> {
        self.binding.work(delta)?;
        if matches!(delta, n::WorkDelta::Copy(bytes)
            if bytes == std::mem::size_of::<Option<std::num::NonZeroU32>>() as u64)
        {
            self.completed(Point::SparseCommit, 4);
        }
        Ok(())
    }
    fn admit(&mut self, request: n::NativeAdmission) -> Result<(), Self::Error> {
        self.binding.admit(request)
    }
    fn reserve(&mut self, bytes: u64) -> Result<Self::Lease, Self::Error> {
        let lease = match self.binding.reserve(bytes) {
            Ok(lease) => lease,
            Err(error) => {
                self.trace.reserve_error =
                    Some(self.binding.resolve::<()>(Err(error)).unwrap_err());
                return Err(error);
            }
        };
        // The actual binding's PreparedCall has already closed. A panic here
        // cannot rely on that local guard to poison the enclosing factory.
        self.completed(Point::Reserve, bytes);
        Ok(lease)
    }
    fn admit_actual(&mut self, lease: &mut Self::Lease, bytes: u64) -> Result<(), Self::Error> {
        if let Err(error) = self.binding.admit_actual(lease, bytes) {
            self.trace.actual_error = Some(self.binding.resolve::<()>(Err(error)).unwrap_err());
            return Err(error);
        }
        self.completed(Point::Actual, bytes);
        Ok(())
    }
    fn fact(&mut self, fact: n::AcceptedFact<'_>) -> Result<(), Self::Error> {
        self.binding.fact(fact)
    }
    fn reject(&mut self, violation: n::NativeViolation) -> Self::Error {
        self.binding.reject(violation)
    }
}
impl<'native, 'id> direct::DirectHost<'native> for CheckedHost<'_, 'native, '_, '_, 'id, '_> {
    type ReadBrand = crate::policy::ConstructionId<'id>;

    fn finish<T>(&mut self, result: Result<T, Self::Error>) -> Result<T, Self::Error> {
        direct::DirectHost::finish(&mut self.binding, result)
    }
    fn checkpoint_now(&mut self) -> Result<(), Self::Error> {
        direct::DirectHost::checkpoint_now(&mut self.binding)
    }
}

fn prepared(bytes: &[u8], revision: u64, budget: &SourceBudget) -> ParseInput {
    let mut op = budget.begin(CancellationToken::new());
    let source = SourceDocument::new(
        SourceKey::new([0x72; 16], revision).unwrap(),
        bytes,
        &mut op,
    )
    .unwrap()
    .snapshot();
    ParseInput::prepare(source, &mut op).unwrap()
}

fn old_model(budget: &SourceBudget) -> AcceptedModel {
    let input = prepared(b"old &amp; pin", 1, budget);
    let mut op = budget.begin(CancellationToken::new());
    AcceptedModel::parse(
        input.source().clone(),
        ParseConfig::new(Profile::C0),
        1,
        &mut op,
    )
    .unwrap()
}

fn live(budget: &SourceBudget) -> (u64, u64, u64) {
    let usage = budget.usage();
    (
        usage.source_capacity,
        usage.auxiliary_capacity,
        usage.source_owners,
    )
}

fn assert_old_pin(model: &AcceptedModel, budget: &SourceBudget) {
    assert_eq!(
        model.identity().source(),
        SourceKey::new([0x72; 16], 1).unwrap()
    );
    assert_eq!(model.source().text(), "old &amp; pin");
    let mut reader =
        MainReader::new(model.clone(), budget.begin(CancellationToken::new())).unwrap();
    let mut bytes = [0; 64];
    let read = reader.read_into(&mut bytes).unwrap();
    assert!(read.done);
    assert_eq!(&bytes[..read.written], b"old & pin");
}

fn assert_closed<'id>(
    input: &ParseInput,
    mut rows: BuildRows<'id, u32>,
    mut work: OwnedWork<'id, '_>,
    expected: CoreError,
) {
    assert_eq!(work.first_fault(), Some(expected));
    assert_eq!(rows.get(0, &mut work), Err(expected));
    assert_eq!(rows.push(9, &mut work), Err(expected));
    assert_eq!(rows.raw_value_for_test(0), Some(7));
    assert_eq!(rows.raw_value_for_test(1), None);
    let retry: Result<(), _> = with_prepared(input, &mut work, |_, _| {
        panic!("a faulted factory cannot issue another input")
    });
    assert_eq!(retry, Err(expected));
    assert_eq!(work.finish(), Err(expected));
    drop(rows);
}

#[test]
fn real_native_allocation_rejects_small_auxiliary_budget_and_releases_to_old_pins() {
    const AUXILIARY: u64 = 1_048_576;
    let budget = SourceBudget::new_with_model_limits(
        SourceLimits {
            auxiliary_capacity: AUXILIARY,
            ..SourceLimits::default()
        },
        ModelLimits::default(),
    )
    .unwrap();
    let old = old_model(&budget);
    let input = prepared("a &amp; b\n\n".repeat(1024).as_bytes(), 2, &budget);
    let retained = live(&budget);
    let token = CancellationToken::new();
    let mut op = budget.begin(token.clone());
    let mut trace = Trace::default();
    construction_scope(&mut op, |mut work| {
        let mut rows = BuildRows::new(4, LimitKind::ModelRecords, &mut work).unwrap();
        rows.push(7_u32, &mut work).unwrap();
        let result: Result<(), _> = with_prepared(&input, &mut work, |native, binding| {
            let mut host = CheckedHost {
                binding,
                trace: &mut trace,
                injection: None,
                token: &token,
                budget: &budget,
            };
            let result = direct::parse(native, &super::super::options::c0(), &mut host, |_| {
                panic!("the fixed small auxiliary budget must reject before finalization")
            });
            host.binding.resolve(result)
        });
        let error = result.unwrap_err();
        assert!(matches!(error, CoreError::ResourceLimit {
            kind: LimitKind::AuxiliaryCapacity, requested, limit: AUXILIARY
        } if requested > AUXILIARY));
        assert!(trace.reserve_ok > 0 && trace.actual_ok > 0);
        assert!(trace.reserve_error == Some(error) || trace.actual_error == Some(error));
        assert_closed(&input, rows, work, error);
    });
    assert_eq!(live(&budget), retained);
    assert_old_pin(&old, &budget);
    drop((old, input));
    assert_eq!(live(&budget), (0, 0, 0));
}

fn after_success(action: Action) {
    for point in [Point::Reserve, Point::Actual] {
        for prior_fault in [false, true] {
            let budget = SourceBudget::model_fixture();
            let old = old_model(&budget);
            let input = prepared(b"*a &amp; b* [r]\n\n[r]: /u \"x\"\n", 2, &budget);
            let retained = live(&budget);
            let token = CancellationToken::new();
            let mut op = budget.begin(token.clone());
            let mut trace = Trace::default();
            let expected = if prior_fault {
                CoreError::InvalidProvenance
            } else {
                match action {
                    Action::Cancel => CoreError::Cancelled,
                    Action::Panic => CoreError::InternalInvariant,
                }
            };
            construction_scope(&mut op, |mut work| {
                let mut rows = BuildRows::new(4, LimitKind::ModelRecords, &mut work).unwrap();
                rows.push(7_u32, &mut work).unwrap();
                let before_parse = budget.usage().auxiliary_capacity;
                let result = catch_unwind(AssertUnwindSafe(|| {
                    with_prepared(&input, &mut work, |native, binding| {
                        let mut host = CheckedHost {
                            binding,
                            trace: &mut trace,
                            injection: Some(Injection {
                                point,
                                ordinal: 2,
                                action,
                                prior_fault,
                            }),
                            token: &token,
                            budget: &budget,
                        };
                        let result = direct::parse(
                            native,
                            &super::super::options::c0(),
                            &mut host,
                            |_| -> Result<(), NativeFault<'_, '_>> {
                                panic!("injection must stop grammar before finalization")
                            },
                        );
                        host.binding.resolve(result)
                    })
                }));
                match action {
                    Action::Cancel => assert_eq!(result.unwrap(), Err(expected)),
                    Action::Panic => {
                        let payload = result.expect_err("selected Host completion must unwind");
                        assert_eq!(
                            payload.downcast_ref::<&str>().copied(),
                            Some("prepared-resource-after-success")
                        );
                    }
                }
                assert!(trace.reached);
                assert!(trace.request > 0);
                assert!(trace.live_at_trigger > before_parse);
                assert_closed(&input, rows, work, expected);
            });
            assert_eq!(op.first_fault(), Some(expected));
            assert_eq!(live(&budget), retained);
            assert_old_pin(&old, &budget);
            drop((old, input));
            assert_eq!(live(&budget), (0, 0, 0));
        }
    }
}

#[test]
fn cancellation_after_real_host_success_preserves_first_fault_and_old_pins() {
    after_success(Action::Cancel);
}

#[test]
fn unwind_after_real_host_success_permanently_closes_work_and_releases_leases() {
    after_success(Action::Panic);
}

#[test]
fn caught_native_sparse_commit_unwind_faults_binding_before_factory_unwinds() {
    assert_eq!(std::mem::size_of::<Option<std::num::NonZeroU32>>(), 4);
    for prior_fault in [false, true] {
        let budget = SourceBudget::model_fixture();
        let old = old_model(&budget);
        let input = prepared(b"abc", 2, &budget);
        let retained = live(&budget);
        let token = CancellationToken::new();
        let mut op = budget.begin(token.clone());
        let mut trace = Trace::default();
        let expected = if prior_fault {
            CoreError::InvalidProvenance
        } else {
            CoreError::InternalInvariant
        };
        construction_scope(&mut op, |mut work| {
            let mut rows = BuildRows::new(4, LimitKind::ModelRecords, &mut work).unwrap();
            rows.push(7_u32, &mut work).unwrap();
            let before_parse = live(&budget);
            let result: Result<(), CoreError> =
                with_prepared(&input, &mut work, |native, binding| {
                    let mut host = CheckedHost {
                        binding,
                        trace: &mut trace,
                        injection: Some(Injection {
                            point: Point::SparseCommit,
                            ordinal: 1,
                            action: Action::Panic,
                            prior_fault,
                        }),
                        token: &token,
                        budget: &budget,
                    };
                    // Catch inside the factory body. PreparedFactory has NOT
                    // unwound and cannot be the source of the fault below.
                    let panic = catch_unwind(AssertUnwindSafe(|| {
                        direct::parse(
                            native,
                            &super::super::options::c0(),
                            &mut host,
                            |_| -> Result<(), NativeFault<'_, '_>> {
                                panic!("sparse commit must precede finalization")
                            },
                        )
                    }));
                    let payload = panic.expect_err("actual sparse ID commit must be reached");
                    assert_eq!(
                        payload.downcast_ref::<&str>().copied(),
                        Some("prepared-resource-after-success")
                    );
                    assert!(host.trace.reached);
                    assert_eq!(host.trace.sparse_commit_ok, 1);
                    assert_eq!(host.trace.request, 4);
                    assert_eq!(host.binding.work.first_fault(), Some(expected));
                    assert_eq!(
                        host.binding.with_parts(|_, work| rows.get(0, work)),
                        Err(expected)
                    );
                    assert_eq!(rows.get(0, host.binding.work), Err(expected));
                    assert_eq!(rows.push(9, host.binding.work), Err(expected));
                    assert_eq!(host.binding.work.check(), Err(expected));
                    let finished = direct::DirectHost::finish(&mut host.binding, Ok(()));
                    assert_eq!(host.binding.resolve(finished), Err(expected));
                    assert_eq!(live(&budget), before_parse);
                    // Deliberately ignore the caught panic. The factory still
                    // must reject, without manufacturing a fresh operation.
                    Ok(())
                });
            assert_eq!(result, Err(expected));
            assert_closed(&input, rows, work, expected);
        });
        assert_eq!(live(&budget), retained);
        assert_old_pin(&old, &budget);
        drop((old, input));
        assert_eq!(live(&budget), (0, 0, 0));
    }
}
