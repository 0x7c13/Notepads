// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE file in the project root for license information.
// ---------------------------------------------------------------------------------------------

use super::*;
use crate::policy::{CancellationToken, SourceBudget};
use std::cell::Cell;

struct Append<'a> {
    values: &'a mut ExactRows<u64>,
    states: &'a mut ExactRows<u8>,
    fault: &'a Cell<Option<CoreError>>,
    next: usize,
    end: usize,
}

impl TaskKernel for Append<'_> {
    type Output = usize;
    fn run<'scope>(
        &'scope mut self,
        window: &mut TaskWindow<'scope, '_>,
    ) -> Result<TaskStep<usize>, CoreError> {
        let mut rows =
            match window.write_exact(self.values, self.fault, TaskAbandon::InvalidProvenance)? {
                TaskStep::Ready(rows) => rows,
                TaskStep::Full => return Ok(TaskStep::Full),
                TaskStep::NeedCapacity(count) => return Ok(TaskStep::NeedCapacity(count)),
            };
        let mut states =
            match window.write_exact(self.states, self.fault, TaskAbandon::InvalidProvenance)? {
                TaskStep::Ready(rows) => rows,
                TaskStep::Full => return Ok(TaskStep::Full),
                TaskStep::NeedCapacity(count) => return Ok(TaskStep::NeedCapacity(count)),
            };
        while self.next < self.end {
            match rows.push_and_mark(self.next as u64, &mut states, self.next, window)? {
                TaskStep::Ready(()) => self.next += 1,
                TaskStep::Full => return Ok(TaskStep::Full),
                TaskStep::NeedCapacity(_) => unreachable!(),
            }
        }
        Ok(TaskStep::Ready(self.next))
    }
}

#[test]
fn task_exact_capacity_exit_precedes_lazy_allocation_and_any_write() {
    let budget = SourceBudget::task_fixture();
    let mut op = budget.begin(CancellationToken::new());
    let mut values = ExactRows::<u64>::new(3, &mut op).unwrap();
    let mut states = ExactRows::<u8>::new(3, &mut op).unwrap();
    for _ in 0..3 {
        states.push(1, &mut op).unwrap();
    }
    let retained = budget.usage().auxiliary_capacity;
    let fault = Cell::new(None);
    let mut kernel = Append {
        values: &mut values,
        states: &mut states,
        fault: &fault,
        next: 0,
        end: 3,
    };
    assert_eq!(
        run_task(&mut kernel, &mut op),
        Ok(TaskStep::NeedCapacity(3))
    );
    assert_eq!(kernel.next, 0);
    assert_eq!(budget.usage().auxiliary_capacity, retained);
    assert_eq!(fault.get(), None);
    assert!(!values.has_allocation());
    assert_eq!(states.as_slice(), &[1, 1, 1]);
}

#[test]
fn task_exact_resumes_complete_pairs_without_growth_or_duplicate_rows() {
    let budget = SourceBudget::task_fixture();
    let mut op = budget.begin(CancellationToken::new());
    let mut values = ExactRows::<u64>::new(600, &mut op).unwrap();
    let mut states = ExactRows::<u8>::new(600, &mut op).unwrap();
    for _ in 0..600 {
        states.push(1, &mut op).unwrap();
    }
    values.push(0, &mut op).unwrap();
    states.replace(0, 2, &mut op).unwrap();
    let retained = budget.usage().auxiliary_capacity;
    let fault = Cell::new(None);
    let mut kernel = Append {
        values: &mut values,
        states: &mut states,
        fault: &fault,
        next: 1,
        end: 600,
    };
    let mut windows = 0;
    loop {
        windows += 1;
        match run_task(&mut kernel, &mut op).unwrap() {
            TaskStep::Full => assert!(windows < 20),
            TaskStep::Ready(count) => {
                assert_eq!(count, 600);
                break;
            }
            TaskStep::NeedCapacity(_) => panic!("existing allocations cannot request growth"),
        }
    }
    assert!(windows > 1);
    assert_eq!(fault.get(), None);
    assert_eq!(values.as_slice(), (0..600).collect::<Vec<u64>>());
    assert!(states.as_slice().iter().all(|value| *value == 2));
    assert_eq!(budget.usage().auxiliary_capacity, retained);
    drop(values);
    drop(states);
    assert_eq!(budget.usage().auxiliary_capacity, 0);
}

#[test]
fn task_exact_invalid_state_rejects_before_append_and_new_owner_use() {
    let budget = SourceBudget::task_fixture();
    let mut op = budget.begin(CancellationToken::new());
    let mut values = ExactRows::<u64>::new(2, &mut op).unwrap();
    let mut states = ExactRows::<u8>::new(2, &mut op).unwrap();
    values.push(0, &mut op).unwrap();
    states.push(2, &mut op).unwrap();
    states.push(0, &mut op).unwrap();
    let fault = Cell::new(None);
    let mut kernel = Append {
        values: &mut values,
        states: &mut states,
        fault: &fault,
        next: 1,
        end: 2,
    };
    assert_eq!(
        run_task(&mut kernel, &mut op),
        Err(CoreError::InvalidProvenance)
    );
    assert_eq!(kernel.next, 1);
    let mut fresh = budget.begin(CancellationToken::new());
    assert_eq!(
        run_task(&mut kernel, &mut fresh),
        Err(CoreError::InvalidProvenance)
    );
    assert_eq!(values.as_slice(), &[0]);
    assert_eq!(states.as_slice(), &[2, 0]);
    assert_eq!(fault.get(), Some(CoreError::InvalidProvenance));
    drop(values);
    drop(states);
    assert_eq!(budget.usage().auxiliary_capacity, 0);
}
