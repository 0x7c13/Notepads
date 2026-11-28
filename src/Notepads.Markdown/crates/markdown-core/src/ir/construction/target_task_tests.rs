// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE file in the project root for license information.
// ---------------------------------------------------------------------------------------------

use super::tests::{header, owned, released};
use super::*;
use crate::policy::CancellationToken;
use crate::policy::{SourceBudget, construction_scope};
use std::cell::Cell;
use std::panic::{AssertUnwindSafe, catch_unwind};

#[test]
fn task_writer_keeps_first_allocation_pending_and_exactly_one_lower() {
    let budget = SourceBudget::task_fixture();
    let mut test_operation = budget.begin(CancellationToken::new());
    construction_scope(&mut test_operation, |mut op| {
        let (mut builder, owner, sources, _) = header(
            ExactCounts {
                atoms: 3,
                ..ExactCounts::default()
            },
            1,
            1,
            &mut op,
        );
        assert_eq!(builder.states[Table::Atoms as usize].bits.len(), 0);
        assert!(!builder.ir.packets.has_allocation());
        let calls = Cell::new(0);
        let mut writer = builder.begin_target(3, &mut op).unwrap();
        for index in 0..3 {
            let ticket = writer.reserve_next().unwrap();
            assert_eq!(writer.pending, Some(ticket.reference.value));
            assert_eq!(writer.builder.states[Table::Atoms as usize].bits.len(), 3);
            assert_eq!(
                writer.builder.states[Table::Atoms as usize]
                    .bits
                    .raw_value_for_test(index)
                    .unwrap(),
                1
            );
            let reference = writer
                .commit(ticket, |builder, op| {
                    calls.set(calls.get() + 1);
                    assert_eq!(builder.ir.packets.has_allocation(), index != 0);
                    assert_eq!(builder.ir.packets.len(), index);
                    assert_eq!(
                        builder.states[Table::Atoms as usize]
                            .bits
                            .raw_value_for_test(index)
                            .unwrap(),
                        1
                    );
                    let (atom, primary) = owned(owner, sources, 1).lower(builder, op)?;
                    Ok((atom, AuthorityBinding::Owned(primary)))
                })
                .unwrap();
            assert_eq!(reference.value.index(), index);
            assert_eq!(writer.pending, None);
            assert_eq!(writer.committed, index + 1);
            assert_eq!(writer.builder.ir.packets.len(), index + 1);
            assert_eq!(
                writer.builder.states[Table::Atoms as usize]
                    .bits
                    .raw_value_for_test(index)
                    .unwrap(),
                2
            );
        }
        assert_eq!(calls.get(), 3);
        let block = writer.close().unwrap();
        assert_eq!(block.reference.value.len(), 3);
        drop(builder);
        released(&budget);
        let first = op.first_fault();
        assert_eq!(op.finish().err(), first);
    });
}

#[test]
fn task_allocated_writer_rejects_cancel_before_lower_and_owner_reuse() {
    let budget = SourceBudget::task_fixture();
    let cancel = CancellationToken::new();
    let mut test_operation = budget.begin(cancel.clone());
    construction_scope(&mut test_operation, |mut op| {
        let (mut builder, owner, sources, _) = header(
            ExactCounts {
                atoms: 2,
                ..ExactCounts::default()
            },
            1,
            1,
            &mut op,
        );
        let mut writer = builder.begin_target(2, &mut op).unwrap();
        let first = writer.reserve_next().unwrap();
        writer
            .commit_owned(first, owned(owner, sources, 1))
            .unwrap();
        let second = writer.reserve_next().unwrap();
        let calls = Cell::new(0);
        cancel.cancel();
        assert_eq!(
            writer.commit(second, |builder, op| {
                calls.set(calls.get() + 1);
                let (atom, primary) = owned(owner, sources, 1).lower(builder, op)?;
                Ok((atom, AuthorityBinding::Owned(primary)))
            }),
            Err(CoreError::Cancelled)
        );
        assert_eq!(calls.get(), 0);
        assert_eq!(writer.builder.ir.packets.len(), 1);
        assert_eq!(
            &(0..writer.builder.states[Table::Atoms as usize].bits.len())
                .map(|i| writer.builder.states[Table::Atoms as usize]
                    .bits
                    .raw_value_for_test(i)
                    .unwrap())
                .collect::<Vec<_>>(),
            &[2, 1]
        );
        drop(writer);
        assert_eq!(op.first_fault(), Some(CoreError::Cancelled));
        assert!(matches!(
            builder.begin_target(0, &mut op),
            Err(CoreError::Cancelled)
        ));
        drop(builder);
        released(&budget);
        let first = op.first_fault();
        assert_eq!(op.finish().err(), first);
    });
}

#[test]
fn task_writer_caught_lower_fault_keeps_first_error_after_cancel() {
    let budget = SourceBudget::task_fixture();
    let cancel = CancellationToken::new();
    let mut test_operation = budget.begin(cancel.clone());
    construction_scope(&mut test_operation, |mut op| {
        let (mut builder, owner, sources, _) = header(
            ExactCounts {
                atoms: 2,
                ..ExactCounts::default()
            },
            1,
            1,
            &mut op,
        );
        let mut writer = builder.begin_target(2, &mut op).unwrap();
        let first = writer.reserve_next().unwrap();
        writer
            .commit_owned(first, owned(owner, sources, 1))
            .unwrap();
        let ticket = writer.reserve_next().unwrap();
        let calls = Cell::new(0);
        assert_eq!(
            writer.commit(ticket, |_builder, _op| {
                calls.set(calls.get() + 1);
                Err(CoreError::InvalidProvenance)
            }),
            Err(CoreError::InvalidProvenance)
        );
        assert_eq!(calls.get(), 1);
        cancel.cancel();
        assert_eq!(writer.close(), Err(CoreError::InvalidProvenance));
        assert_eq!(op.first_fault(), Some(CoreError::InvalidProvenance));
        assert!(matches!(
            builder.begin_target(0, &mut op),
            Err(CoreError::InvalidProvenance)
        ));
        drop(builder);
        released(&budget);
        let first = op.first_fault();
        assert_eq!(op.finish().err(), first);
    });
}

#[test]
fn task_writer_drop_after_caught_lower_panic_preserves_abandon_rule() {
    for allocated in [false, true] {
        let budget = SourceBudget::task_fixture();
        let mut test_operation = budget.begin(CancellationToken::new());
        construction_scope(&mut test_operation, |mut op| {
            let (mut builder, owner, sources, _) = header(
                ExactCounts {
                    atoms: 2,
                    ..ExactCounts::default()
                },
                1,
                1,
                &mut op,
            );
            let mut writer = builder.begin_target(2, &mut op).unwrap();
            if allocated {
                let first = writer.reserve_next().unwrap();
                writer
                    .commit_owned(first, owned(owner, sources, 1))
                    .unwrap();
            }
            let ticket = writer.reserve_next().unwrap();
            assert!(
                catch_unwind(AssertUnwindSafe(|| {
                    let _ = writer.commit(ticket, |_builder, _op| {
                        panic!("injected immutable lower panic")
                    });
                }))
                .is_err()
            );
            drop(writer);
            assert_eq!(op.first_fault(), Some(CoreError::InvalidProvenance));
            assert!(matches!(
                builder.begin_target(0, &mut op),
                Err(CoreError::InvalidProvenance)
            ));
            drop(builder);
            released(&budget);
            let first = op.first_fault();
            assert_eq!(op.finish().err(), first);
        });
    }
}
