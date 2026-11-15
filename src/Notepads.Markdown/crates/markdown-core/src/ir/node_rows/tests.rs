// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE file in the project root for license information.
// ---------------------------------------------------------------------------------------------

use super::*;
use crate::identity::LogicalSpan;
use crate::ir::storage::ModelRows;
use crate::memory::{BuildKernel, BuildStep, BuildWindow, run_window};
use crate::policy::{CancellationToken, SourceBudget, construction_scope};

fn seed() -> NodeRow {
    NodeRow {
        parent: None,
        first_child: None,
        next_sibling: None,
        flow: FlowId::from_index(0).unwrap(),
        kind: NodeKind::Document,
        detail: None,
        source_envelope: SourceSpan::new(0, 0).unwrap(),
        logical: LogicalSpan::new(0, 0).unwrap(),
    }
}
fn released(budget: &SourceBudget) {
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
struct ReadNode<'a> {
    rows: &'a ModelRows<NodeRow>,
    index: usize,
}
impl<'id> BuildKernel<'id> for ReadNode<'_> {
    type Output = Option<NodeRow>;
    fn run<'scope>(
        &'scope mut self,
        window: &mut BuildWindow<'scope, '_, 'id, '_>,
    ) -> Result<BuildStep<Self::Output>, CoreError> {
        match self.rows.task_read(window)? {
            BuildStep::Ready(view) => view.get(self.index, window),
            BuildStep::Full => Ok(BuildStep::Full),
            BuildStep::NeedCapacity(n) => Ok(BuildStep::NeedCapacity(n)),
        }
    }
}

#[test]
fn same_node_pages_cross_boundaries_release_status_and_support_completed_task_reads() {
    let budget = SourceBudget::task_fixture();
    let mut operation = budget.begin(CancellationToken::new());
    operation.set_clock_samples_for_test(&[499_999_999]);
    construction_scope(&mut operation, |mut work| {
        let width = (16384 / size_of::<NodeRow>()).clamp(1, 1024);
        let count = width * 2 + 1;
        let mut rows = NodeRows::new(count as u64, &mut work).unwrap();
        for index in 0..count {
            let mut row = seed();
            row.source_envelope = SourceSpan::new(index as u64, index as u64 + 1).unwrap();
            row.logical = LogicalSpan::new(index as u64, index as u64 + 1).unwrap();
            assert_eq!(rows.reserve(row, true, &mut work).unwrap(), index);
            rows.write(index, row, &mut work).unwrap();
        }
        assert_eq!(rows.values.page_count(), 3);
        let addresses: Vec<_> = (0..count)
            .map(|i| rows.values.address_for_test(i).unwrap())
            .collect();
        let before = budget.usage().auxiliary_capacity;
        let completed = ModelRows::completed_nodes(rows.freeze(&mut work).unwrap());
        assert!(
            budget.usage().auxiliary_capacity < before,
            "status storage really releases"
        );
        let view = completed.view();
        assert_eq!(completed.len(), count);
        for (index, address) in addresses.into_iter().enumerate() {
            assert_eq!(view.get(index).unwrap() as *const NodeRow as usize, address);
        }
        let crossing = view.range(width - 1, 3).unwrap();
        assert_eq!(
            crossing
                .iter()
                .map(|r| r.source_envelope.start())
                .collect::<Vec<_>>(),
            vec![(width - 1) as u64, width as u64, (width + 1) as u64]
        );
        assert_eq!(view.iter().rev().count(), count);
        assert!(view.range(count, 1).is_none());
        for index in [0, width - 1, width, count - 1, count] {
            assert_eq!(
                run_window(
                    &mut ReadNode {
                        rows: &completed,
                        index
                    },
                    &mut work
                )
                .unwrap(),
                BuildStep::Ready(view.get(index).copied())
            );
        }
        drop(completed);
        work.finish().unwrap();
    });
    released(&budget);
}

#[test]
fn absent_and_present_empty_initial_are_distinct_until_completion() {
    let budget = SourceBudget::model_fixture();
    let mut operation = budget.begin(CancellationToken::new());
    construction_scope(&mut operation, |mut work| {
        let mut rows = NodeRows::new(2, &mut work).unwrap();
        let empty = SourceSpan::new(0, 0).unwrap();
        let absent = rows.reserve(seed(), false, &mut work).unwrap();
        let present = rows.reserve(seed(), true, &mut work).unwrap();
        assert_eq!(rows.initial(absent, &mut work).unwrap(), None);
        assert_eq!(rows.initial(present, &mut work).unwrap(), Some(empty));
        rows.set_initial(absent, Some(empty), &mut work).unwrap();
        rows.set_initial(present, None, &mut work).unwrap();
        assert_eq!(rows.initial(absent, &mut work).unwrap(), Some(empty));
        assert_eq!(rows.initial(present, &mut work).unwrap(), None);
        rows.write(absent, seed(), &mut work).unwrap();
        rows.write(present, seed(), &mut work).unwrap();
        drop(rows.freeze(&mut work).unwrap());
        work.finish().unwrap();
    });
    released(&budget);
}

#[test]
fn identity_fields_cannot_change_during_completion() {
    for field in 0..4 {
        let budget = SourceBudget::model_fixture();
        let mut operation = budget.begin(CancellationToken::new());
        construction_scope(&mut operation, |mut work| {
            let mut rows = NodeRows::new(1, &mut work).unwrap();
            rows.reserve(seed(), false, &mut work).unwrap();
            let mut changed = seed();
            match field {
                0 => changed.parent = Some(NodeId::from_index(0).unwrap()),
                1 => changed.flow = FlowId::from_index(1).unwrap(),
                2 => changed.kind = NodeKind::Paragraph,
                _ => changed.detail = Some(DetailId::from_index(0).unwrap()),
            }
            assert_eq!(
                rows.write(0, changed, &mut work),
                Err(CoreError::InvalidProvenance)
            );
            assert_eq!(rows.values.raw_value_for_test(0), Some(seed()));
            assert_eq!(rows.states.raw_value_for_test(0), Some(PENDING));
            assert_eq!(
                rows.graph_row(0, &mut work),
                Err(CoreError::InvalidProvenance)
            );
            assert!(matches!(
                rows.freeze(&mut work),
                Err(CoreError::InvalidProvenance)
            ));
            assert_eq!(work.finish(), Err(CoreError::InvalidProvenance));
        });
        released(&budget);
    }
}

#[test]
fn duplicate_invalid_or_falsely_completed_state_never_grants_read_or_freeze() {
    for scenario in 0..4 {
        let budget = SourceBudget::model_fixture();
        let mut operation = budget.begin(CancellationToken::new());
        construction_scope(&mut operation, |mut work| {
            let mut rows = NodeRows::new(1, &mut work).unwrap();
            rows.reserve(seed(), false, &mut work).unwrap();
            let result = match scenario {
                0 => {
                    rows.write(0, seed(), &mut work).unwrap();
                    rows.write(0, seed(), &mut work)
                }
                1 => {
                    rows.states.replace(0, 255, &mut work).unwrap();
                    rows.graph_row(0, &mut work).map(|_| ())
                }
                2 => {
                    // Counter alone must never confer completion authority.
                    rows.pending = 0;
                    rows.ensure_written(&mut work)
                }
                _ => rows.get(0, &mut work).map(|_| ()),
            };
            assert_eq!(result, Err(CoreError::InvalidProvenance));
            assert_eq!(
                rows.set_initial(0, None, &mut work),
                Err(CoreError::InvalidProvenance)
            );
            assert!(matches!(
                rows.freeze(&mut work),
                Err(CoreError::InvalidProvenance)
            ));
            assert_eq!(work.finish(), Err(CoreError::InvalidProvenance));
        });
        released(&budget);
    }
}

#[derive(Clone, Copy)]
enum Mutation {
    Reserve,
    Initial,
    Write,
    Freeze,
}
fn deadline_mutation(kind: Mutation, bytes_left: Option<u64>, at: Option<usize>) -> (usize, bool) {
    let budget = SourceBudget::task_fixture();
    let mut operation = budget.begin(CancellationToken::new());
    operation.set_clock_samples_for_test(&[100_000_000]);
    let observed = construction_scope(&mut operation, |mut work| {
        let mut rows = NodeRows::new(3, &mut work).unwrap();
        if !matches!(kind, Mutation::Reserve) {
            rows.reserve(seed(), false, &mut work).unwrap();
        }
        if matches!(kind, Mutation::Freeze) {
            rows.write(0, seed(), &mut work).unwrap();
        }
        if let Some(bytes_left) = bytes_left {
            // Reset the existing schedule through a real clock boundary, then
            // charge one legal block. No production checkpoint or test ledger.
            work.checkpoint_now().unwrap();
            work.scan(crate::policy::CHECKPOINT_BYTES - bytes_left)
                .unwrap();
        }
        let mut samples = vec![100_000_000; at.unwrap_or(1)];
        if let Some(at) = at {
            samples[at - 1] = 500_000_000;
        }
        work.set_clock_samples_for_test(&samples);
        if matches!(kind, Mutation::Freeze) {
            let result = rows.freeze(&mut work);
            let reads = work.clock_read_count_for_test();
            if at.is_some() {
                let error = CoreError::DeadlineExceeded {
                    elapsed_ns: 500_000_000,
                    limit_ns: 500_000_000,
                };
                assert!(matches!(result, Err(e) if e == error));
                assert_eq!(work.check(), Err(error));
                assert_eq!(work.finish(), Err(error));
            } else {
                drop(result.unwrap());
                work.finish().unwrap();
            }
            return (reads, false);
        }
        let mut changed = seed();
        changed.source_envelope = SourceSpan::new(1, 2).unwrap();
        let result = match kind {
            Mutation::Reserve => rows.reserve(seed(), false, &mut work).map(|_| ()),
            Mutation::Initial => rows.set_initial(0, Some(changed.source_envelope), &mut work),
            Mutation::Write => rows.write(0, changed, &mut work),
            Mutation::Freeze => unreachable!(),
        };
        let reads = work.clock_read_count_for_test();
        let partial = match kind {
            Mutation::Reserve => rows.values.len() == 1 && rows.states.len() == 0,
            _ => {
                rows.values.raw_value_for_test(0) == Some(changed)
                    && rows.states.raw_value_for_test(0) == Some(PENDING)
            }
        };
        if at.is_some() {
            let error = CoreError::DeadlineExceeded {
                elapsed_ns: 500_000_000,
                limit_ns: 500_000_000,
            };
            assert_eq!(result, Err(error));
            assert_eq!(rows.graph_row(0, &mut work), Err(error));
            assert_eq!(rows.write(0, seed(), &mut work), Err(error));
            assert!(matches!(rows.freeze(&mut work), Err(e) if e == error));
            assert_eq!(work.finish(), Err(error));
        } else {
            result.unwrap();
            drop(rows);
            work.finish().unwrap();
        }
        (reads, partial)
    });
    released(&budget);
    observed
}

#[test]
fn every_mutation_clock_boundary_latches_partial_values_and_freeze_tail_errors() {
    for kind in [
        Mutation::Reserve,
        Mutation::Initial,
        Mutation::Write,
        Mutation::Freeze,
    ] {
        // Small replacements need not reach a clock boundary from an empty
        // schedule. Sweep a finite row-sized suffix of the original byte
        // window, including the one-byte status write after the value commit.
        let alignments: Vec<_> = if matches!(kind, Mutation::Initial | Mutation::Write) {
            (1..=(4 * size_of::<NodeRow>() + 32) as u64)
                .map(Some)
                .collect()
        } else {
            vec![None]
        };
        let mut clock_boundaries = 0;
        let mut saw_partial = false;
        for bytes_left in alignments {
            let (reads, _) = deadline_mutation(kind, bytes_left, None);
            clock_boundaries += reads;
            for at in 1..=reads {
                saw_partial |= deadline_mutation(kind, bytes_left, Some(at)).1;
            }
        }
        assert!(clock_boundaries > 0);
        if !matches!(kind, Mutation::Freeze) {
            assert!(saw_partial);
        }
    }
}

#[test]
fn cancellation_and_caught_guard_unwind_preserve_first_fault_and_release_pages() {
    for mode in 0..3 {
        let budget = SourceBudget::model_fixture();
        let token = CancellationToken::new();
        let mut operation = budget.begin(token.clone());
        construction_scope(&mut operation, |mut work| {
            let mut rows = NodeRows::new(1, &mut work).unwrap();
            rows.reserve(seed(), false, &mut work).unwrap();
            let expected = if mode == 0 {
                token.cancel();
                assert_eq!(rows.write(0, seed(), &mut work), Err(CoreError::Cancelled));
                CoreError::Cancelled
            } else {
                let caught = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                    // The same production guard used by NodeRows. Panic occurs
                    // after an actual row write; no private panic hook is added.
                    let _: Result<(), CoreError> = guarded_callback(&mut work, |work| {
                        rows.write(0, seed(), work)?;
                        if mode == 2 {
                            work.fail(CoreError::ArithmeticOverflow);
                        }
                        panic!("test abandons node construction after its real write");
                    });
                }));
                assert!(caught.is_err());
                if mode == 2 {
                    CoreError::ArithmeticOverflow
                } else {
                    CoreError::InvalidProvenance
                }
            };
            token.cancel();
            assert_eq!(rows.get(0, &mut work), Err(expected));
            assert_eq!(rows.write(0, seed(), &mut work), Err(expected));
            assert!(matches!(rows.freeze(&mut work), Err(e) if e == expected));
            assert_eq!(work.finish(), Err(expected));
        });
        released(&budget);
    }
}

#[test]
fn unified_node_layout_is_reported_without_claiming_capacity_success() {
    fn report<T>(name: &str) {
        println!(
            "UNIFIED_NODE_LAYOUT\t{}\t{}\t{}",
            name,
            size_of::<T>(),
            align_of::<T>()
        );
    }
    report::<NodeRow>("NodeRow");
    report::<crate::ir::storage::FinalSlot<NodeRow>>("FinalSlot<NodeRow>");
    report::<NodeRows<'static>>("NodeRows");
    report::<NodeStorage<'static>>("NodeStorage");
    report::<crate::ir::CompactIr>("CompactIr");
}

#[test]
fn early_completion_retires_status_and_keeps_pages_through_later_consumers_and_freeze() {
    let budget = SourceBudget::model_fixture();
    let mut operation = budget.begin(CancellationToken::new());
    construction_scope(&mut operation, |mut work| {
        let count = (16384 / size_of::<NodeRow>()).clamp(1, 1024) + 1;
        let mut storage = NodeStorage::new(count as u64, &mut work).unwrap();
        for index in 0..count {
            let mut row = seed();
            row.logical = LogicalSpan::new(index as u64, index as u64 + 1).unwrap();
            storage.reserve(row, false, &mut work).unwrap();
            storage.write(index, row, &mut work).unwrap();
        }
        let NodeStorage::Building(building) = &storage else {
            panic!("actual construction state required");
        };
        let addresses: Vec<_> = (0..count)
            .map(|i| building.values.address_for_test(i).unwrap())
            .collect();
        let before = budget.usage().auxiliary_capacity;
        storage.complete(&mut work).unwrap();
        assert!(budget.usage().auxiliary_capacity < before);
        let NodeStorage::Completed(completed) = &storage else {
            panic!("actual completed owner required");
        };
        for (index, address) in addresses.iter().copied().enumerate() {
            assert_eq!(
                completed.pages.get(index).unwrap() as *const NodeRow as usize,
                address
            );
            assert_eq!(
                storage.get(index, &mut work).unwrap().logical.start(),
                index as u64
            );
            assert_eq!(
                storage.graph_row(index, &mut work).unwrap().kind,
                NodeKind::Document
            );
            assert!(storage.is_written(index, &mut work).unwrap());
        }
        let final_rows = ModelRows::completed_nodes(storage.freeze(&mut work).unwrap());
        for (index, address) in addresses.into_iter().enumerate() {
            assert_eq!(
                final_rows.view().get(index).unwrap() as *const NodeRow as usize,
                address
            );
        }
        drop(final_rows);
        work.finish().unwrap();
    });
    released(&budget);
}

#[test]
fn completed_node_storage_rejects_every_construction_mutation_and_initial_access() {
    for mode in 0..5 {
        let budget = SourceBudget::model_fixture();
        let mut operation = budget.begin(CancellationToken::new());
        construction_scope(&mut operation, |mut work| {
            let mut storage = NodeStorage::new(2, &mut work).unwrap();
            storage.reserve(seed(), false, &mut work).unwrap();
            storage.write(0, seed(), &mut work).unwrap();
            storage.complete(&mut work).unwrap();
            let error = match mode {
                0 => storage.reserve(seed(), false, &mut work).map(|_| ()),
                1 => storage.write(0, seed(), &mut work),
                2 => storage.set_initial(0, None, &mut work),
                3 => storage.initial(0, &mut work).map(|_| ()),
                _ => storage.complete(&mut work),
            };
            assert_eq!(error, Err(CoreError::InvalidProvenance));
            assert_eq!(storage.get(0, &mut work), Err(CoreError::InvalidProvenance));
            drop(storage);
            assert_eq!(work.finish(), Err(CoreError::InvalidProvenance));
        });
        released(&budget);
    }
}

#[test]
fn early_node_completion_deadline_at_every_real_clock_read_cannot_resume() {
    fn prepared<'id>(work: &mut OwnedWork<'id, '_>) -> NodeStorage<'id> {
        let mut storage = NodeStorage::new(2, work).unwrap();
        storage.reserve(seed(), false, work).unwrap();
        storage.write(0, seed(), work).unwrap();
        storage
    }
    let budget = SourceBudget::task_fixture();
    let mut operation = budget.begin(CancellationToken::new());
    operation.set_clock_samples_for_test(&[100_000_000]);
    let reads = construction_scope(&mut operation, |mut work| {
        let mut storage = prepared(&mut work);
        work.set_clock_samples_for_test(&[100_000_000]);
        storage.complete(&mut work).unwrap();
        let reads = work.clock_read_count_for_test();
        drop(storage);
        work.finish().unwrap();
        reads
    });
    released(&budget);
    assert!(reads >= 4);
    for at in 1..=reads {
        let budget = SourceBudget::task_fixture();
        let mut operation = budget.begin(CancellationToken::new());
        operation.set_clock_samples_for_test(&[100_000_000]);
        construction_scope(&mut operation, |mut work| {
            let mut storage = prepared(&mut work);
            let mut samples = vec![100_000_000; at];
            samples[at - 1] = 500_000_000;
            work.set_clock_samples_for_test(&samples);
            let error = storage.complete(&mut work).unwrap_err();
            assert!(matches!(error, CoreError::DeadlineExceeded { .. }));
            assert_eq!(storage.get(0, &mut work), Err(error));
            assert_eq!(storage.complete(&mut work), Err(error));
            drop(storage);
            assert_eq!(work.finish(), Err(error));
        });
        released(&budget);
    }
}
