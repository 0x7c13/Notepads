// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE file in the project root for license information.
// ---------------------------------------------------------------------------------------------

use super::*;
use crate::error::LimitKind;
use crate::policy::{CancellationToken, SourceBudget};
use std::cell::Cell;

struct Increment<'a> {
    rows: &'a mut GrowingVec<u32>,
    fault: &'a Cell<Option<CoreError>>,
    at: usize,
    len: usize,
    pending: Option<u32>,
}

impl TaskKernel for Increment<'_> {
    type Output = usize;

    fn run<'scope>(
        &'scope mut self,
        window: &mut TaskWindow<'scope, '_>,
    ) -> Result<TaskStep<usize>, CoreError> {
        let mut rows =
            match window.write_growing(self.rows, self.fault, TaskAbandon::InternalInvariant)? {
                TaskStep::Ready(rows) => rows,
                TaskStep::Full => return Ok(TaskStep::Full),
                TaskStep::NeedCapacity(_) => unreachable!(),
            };
        while self.at < self.len {
            if let Some(value) = self.pending {
                match rows.replace(self.at, value, window)? {
                    TaskStep::Ready(()) => {
                        self.at += 1;
                        self.pending = None;
                    }
                    TaskStep::Full => return Ok(TaskStep::Full),
                    TaskStep::NeedCapacity(_) => unreachable!(),
                }
            } else {
                match rows.get(self.at, window)? {
                    TaskStep::Ready(Some(value)) => self.pending = Some(value + 1),
                    TaskStep::Ready(None) => return Err(window.fail(CoreError::InvalidProvenance)),
                    TaskStep::Full => return Ok(TaskStep::Full),
                    TaskStep::NeedCapacity(_) => unreachable!(),
                }
            }
        }
        Ok(TaskStep::Ready(self.at))
    }
}

#[test]
fn task_rebinds_and_resumes_without_repeating_a_completed_write() {
    let budget = SourceBudget::task_fixture();
    let mut op = budget.begin(CancellationToken::new());
    let mut rows = GrowingVec::new(2000, LimitKind::IndexRecords, &mut op).unwrap();
    rows.extend_from_slice(&(0..500).collect::<Vec<u32>>(), &mut op)
        .unwrap();
    let fault = Cell::new(None);
    let mut kernel = Increment {
        rows: &mut rows,
        fault: &fault,
        at: 0,
        len: 500,
        pending: None,
    };
    let mut windows = 0;
    loop {
        windows += 1;
        match run_task(&mut kernel, &mut op).unwrap() {
            TaskStep::Full => assert!(windows < 20),
            TaskStep::Ready(count) => {
                assert_eq!(count, 500);
                break;
            }
            TaskStep::NeedCapacity(_) => panic!("replace has no capacity request"),
        }
    }
    assert!(windows > 1);
    assert_eq!(fault.get(), None);
    assert_eq!(rows.as_slice(), &(1..=500).collect::<Vec<u32>>());
    assert_eq!(op.stats(), None);
    drop(rows);
    assert_eq!(budget.usage().auxiliary_capacity, 0);
}

struct Sum<'a> {
    rows: &'a PagedRows<u64>,
    at: usize,
    len: usize,
    sum: u64,
}

impl TaskKernel for Sum<'_> {
    type Output = u64;
    fn run<'scope>(
        &'scope mut self,
        window: &mut TaskWindow<'scope, '_>,
    ) -> Result<TaskStep<u64>, CoreError> {
        let mut rows = match window.read_rows(self.rows)? {
            TaskStep::Ready(rows) => rows,
            TaskStep::Full => return Ok(TaskStep::Full),
            TaskStep::NeedCapacity(_) => unreachable!(),
        };
        while self.at < self.len {
            match rows.get(self.at, window)? {
                TaskStep::Ready(value) => {
                    self.sum += value;
                    self.at += 1;
                }
                TaskStep::Full => return Ok(TaskStep::Full),
                TaskStep::NeedCapacity(_) => unreachable!(),
            }
        }
        Ok(TaskStep::Ready(self.sum))
    }
}

#[test]
fn task_page_cursor_crosses_real_pages_and_wrong_root_latches_storage() {
    let budget = SourceBudget::task_fixture();
    let mut op = budget.begin(CancellationToken::new());
    let mut rows = PagedRows::new(3000, LimitKind::IndexRecords, &mut op).unwrap();
    for value in 0..2200 {
        rows.push(value as u64, &mut op).unwrap();
    }
    assert!(rows.page_count() > 1);
    let mut kernel = Sum {
        rows: &rows,
        at: 0,
        len: 2200,
        sum: 0,
    };
    loop {
        match run_task(&mut kernel, &mut op).unwrap() {
            TaskStep::Ready(sum) => {
                assert_eq!(sum, 2199 * 2200 / 2);
                break;
            }
            TaskStep::Full => (),
            TaskStep::NeedCapacity(_) => unreachable!(),
        }
    }
    let mut other = SourceBudget::task_fixture().begin(CancellationToken::new());
    assert_eq!(
        run_task(&mut kernel, &mut other),
        Err(CoreError::WrongBudget)
    );
    let mut fresh = budget.begin(CancellationToken::new());
    assert_eq!(rows.get(0, &mut fresh), Err(CoreError::WrongBudget));
    drop(rows);
    assert_eq!(budget.usage().auxiliary_capacity, 0);
}

struct Abandon<'a> {
    rows: &'a mut PagedRows<u64>,
    fault: &'a Cell<Option<CoreError>>,
}

impl TaskKernel for Abandon<'_> {
    type Output = ();
    fn run<'scope>(
        &'scope mut self,
        window: &mut TaskWindow<'scope, '_>,
    ) -> Result<TaskStep<()>, CoreError> {
        if matches!(
            window.owner(self.fault, TaskAbandon::InvalidProvenance)?,
            TaskStep::Full
        ) {
            return Ok(TaskStep::Full);
        }
        let mut rows = match window.write_rows(self.rows)? {
            TaskStep::Ready(rows) => rows,
            TaskStep::Full => return Ok(TaskStep::Full),
            TaskStep::NeedCapacity(_) => unreachable!(),
        };
        assert!(matches!(rows.replace(0, 42, window)?, TaskStep::Ready(())));
        panic!("deterministic test unwind after private mutation");
    }
}

#[test]
fn task_unwind_poison_preserves_owner_rule_and_rejects_new_operation() {
    let budget = SourceBudget::task_fixture();
    let mut op = budget.begin(CancellationToken::new());
    let mut rows = PagedRows::new(1, LimitKind::IndexRecords, &mut op).unwrap();
    rows.push(7, &mut op).unwrap();
    let fault = Cell::new(None);
    let mut kernel = Abandon {
        rows: &mut rows,
        fault: &fault,
    };
    assert!(
        std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| run_task(
            &mut kernel,
            &mut op
        )))
        .is_err()
    );
    assert_eq!(op.first_fault(), Some(CoreError::InternalInvariant));
    assert_eq!(fault.get(), Some(CoreError::InvalidProvenance));
    let mut fresh = budget.begin(CancellationToken::new());
    assert_eq!(
        run_task(&mut kernel, &mut fresh),
        Err(CoreError::InvalidProvenance)
    );
    drop(rows);
    assert_eq!(budget.usage().auxiliary_capacity, 0);
}

struct Caught<'a> {
    rows: &'a PagedRows<u64>,
    cancel: CancellationToken,
    invalid: bool,
}

impl TaskKernel for Caught<'_> {
    type Output = ();
    fn run<'scope>(
        &'scope mut self,
        window: &mut TaskWindow<'scope, '_>,
    ) -> Result<TaskStep<()>, CoreError> {
        let mut rows = match window.read_rows(self.rows)? {
            TaskStep::Ready(rows) => rows,
            TaskStep::Full => return Ok(TaskStep::Full),
            TaskStep::NeedCapacity(_) => unreachable!(),
        };
        if self.invalid {
            let _caught = rows.get(100, window);
        } else {
            assert!(matches!(rows.get(0, window)?, TaskStep::Ready(7)));
        }
        self.cancel.cancel();
        Ok(TaskStep::Ready(()))
    }
}

#[test]
fn task_caught_fault_remains_first_and_exit_observes_same_flag() {
    for invalid in [false, true] {
        let budget = SourceBudget::task_fixture();
        let mut preparation = budget.begin(CancellationToken::new());
        let mut rows = PagedRows::new(1, LimitKind::IndexRecords, &mut preparation).unwrap();
        rows.push(7, &mut preparation).unwrap();
        let cancel = CancellationToken::new();
        let mut op = budget.begin(cancel.clone());
        let mut kernel = Caught {
            rows: &rows,
            cancel,
            invalid,
        };
        let expected = if invalid {
            CoreError::InternalInvariant
        } else {
            CoreError::Cancelled
        };
        assert_eq!(run_task(&mut kernel, &mut op), Err(expected));
        let mut fresh = budget.begin(CancellationToken::new());
        assert_eq!(
            rows.get(0, &mut fresh),
            if invalid { Err(expected) } else { Ok(7) }
        );
        assert_eq!(op.stats(), None);
        if invalid {
            // A later foreign writer must observe the existing physical-store
            // fault before checking a new operation's quota root.
            let owner = Cell::new(None);
            let foreign = SourceBudget::task_fixture();
            let mut foreign_op = foreign.begin(CancellationToken::new());
            assert_eq!(
                run_task(
                    &mut Abandon {
                        rows: &mut rows,
                        fault: &owner
                    },
                    &mut foreign_op
                ),
                Err(expected)
            );
        }
    }
}

#[test]
fn task_runner_rejects_collected_fixture_before_entering_kernel() {
    let budget = SourceBudget::model_fixture();
    let mut op = budget.begin(CancellationToken::new());
    let mut rows = PagedRows::new(1, LimitKind::IndexRecords, &mut op).unwrap();
    rows.push(7, &mut op).unwrap();
    let mut kernel = Sum {
        rows: &rows,
        at: 0,
        len: 1,
        sum: 0,
    };
    assert_eq!(
        run_task(&mut kernel, &mut op),
        Err(CoreError::UnsupportedConfiguration)
    );
    assert_eq!(kernel.at, 0);
    assert_eq!(kernel.sum, 0);
}

struct SourceCheck<'a> {
    text: &'a ChargedString,
    root: &'a SourceBudget,
    span: crate::identity::SourceSpan,
}

impl TaskKernel for SourceCheck<'_> {
    type Output = ();
    fn run<'scope>(
        &'scope mut self,
        window: &mut TaskWindow<'scope, '_>,
    ) -> Result<TaskStep<()>, CoreError> {
        let view = match window.source_text(self.text, self.root)? {
            TaskStep::Ready(view) => view,
            TaskStep::Full => return Ok(TaskStep::Full),
            TaskStep::NeedCapacity(_) => unreachable!(),
        };
        view.validate_span(self.span, window)
    }
}

#[test]
fn task_source_checks_real_lease_scalar_boundaries_and_keeps_readonly_source_valid() {
    let budget = SourceBudget::task_fixture();
    let mut prepare = budget.begin(CancellationToken::new());
    let mut text = ChargedString::source(8, &mut prepare).unwrap();
    text.append("A😀中", &mut prepare).unwrap();
    let mut kernel = SourceCheck {
        text: &text,
        root: &budget,
        span: crate::identity::SourceSpan::new(1, 4).unwrap(),
    };
    let mut op = budget.begin(CancellationToken::new());
    assert_eq!(
        run_task(&mut kernel, &mut op),
        Err(CoreError::InvalidBoundary)
    );
    kernel.span = crate::identity::SourceSpan::new(1, 8).unwrap();
    let mut fresh = budget.begin(CancellationToken::new());
    assert_eq!(run_task(&mut kernel, &mut fresh), Ok(TaskStep::Ready(())));
    let foreign = SourceBudget::task_fixture();
    kernel.root = &foreign;
    let mut foreign_op = foreign.begin(CancellationToken::new());
    assert_eq!(
        run_task(&mut kernel, &mut foreign_op),
        Err(CoreError::WrongBudget)
    );
    assert_eq!(text.as_str(), "A😀中");
    drop(text);
    assert_eq!(budget.usage().source_capacity, 0);
    assert_eq!(foreign.usage().source_capacity, 0);
}
