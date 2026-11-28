// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE file in the project root for license information.
// ---------------------------------------------------------------------------------------------

use super::*;
use crate::identity::{LocalUtf8Span, LogicalSpan};
use crate::policy::CancellationToken;
use crate::policy::{SourceBudget, construction_scope};
use std::panic::{AssertUnwindSafe, catch_unwind};

struct Prefix<K> {
    inner: K,
    left: usize,
}

impl<'id, K: BuildKernel<'id>> BuildKernel<'id> for Prefix<K> {
    type Output = K::Output;

    fn run<'scope>(
        &'scope mut self,
        window: &mut BuildWindow<'scope, '_, 'id, '_>,
    ) -> Result<BuildStep<Self::Output>, CoreError> {
        while self.left != 0 {
            match window.advance()? {
                BuildStep::Ready(()) => {}
                BuildStep::Full => return Ok(BuildStep::Full),
                BuildStep::NeedCapacity(count) => return Ok(BuildStep::NeedCapacity(count)),
            }
            self.left -= 1;
        }
        self.inner.run(window)
    }
}

fn packet() -> AtomPacket {
    AtomPacket {
        atom: AtomFields {
            owner: NodeId::from_index(0).unwrap(),
            kind: AtomKind::Copy,
            target: TargetSpan { start: 0, end: 1 },
            sources: Rows::new(0, 0).unwrap(),
            proof: None,
            content: AtomContent::Empty,
        },
        authority: AuthorityBinding::Owned(PrimaryAuthority {
            phase: NativePhase::Block,
            kind: ClaimKind::Block,
            consumed: LocalUtf8Span::new(0, 1).unwrap(),
            host_role: HostRole::Block,
            host_utf8_len: 1,
            host_domains: Rows::new(0, 0).unwrap(),
        }),
    }
}

#[test]
fn task_reserve_yields_after_admission_without_readmission() {
    eprintln!(
        "Task4 IR continuation layout: Reserve<AtomId>={}B, CommitCheck={}B, CommitWrite={}B, RowTarget<NodeRow>={}B, WriteRow<NodeId,NodeRow>={}B, AtomPacket={}B",
        size_of::<Reserve<'_, '_, AtomId>>(),
        size_of::<CommitCheck<'_, '_>>(),
        size_of::<CommitWrite<'_, '_, '_>>(),
        size_of::<RowTarget<'_, '_, NodeRow>>(),
        size_of::<WriteRow<'_, '_, NodeId, NodeRow>>(),
        size_of::<AtomPacket>(),
    );
    let budget = SourceBudget::task_fixture();
    let mut test_operation = budget.begin(CancellationToken::new());
    construction_scope(&mut test_operation, |mut op| {
        let mut builder = ExactBuilder::new(
            ExactCounts {
                atoms: 2,
                ..ExactCounts::default()
            },
            &mut op,
        )
        .unwrap();
        builder.reserve_atom(0, &mut op).unwrap();
        let mut kernel = Prefix {
            inner: Reserve::<AtomId> {
                builder: &mut builder,
                index: 1,
                phase: ReservePhase::Check,
                _type: std::marker::PhantomData,
            },
            // 188 advances + binding/check/state/admit fill the 192-action window.
            left: 188,
        };
        assert_eq!(run_window(&mut kernel, &mut op).unwrap(), BuildStep::Full);
        assert!(matches!(kernel.inner.phase, ReservePhase::Write));
        assert_eq!(
            &(0..kernel.inner.builder.states[Table::Atoms as usize]
                .bits
                .len())
                .map(|i| kernel.inner.builder.states[Table::Atoms as usize]
                    .bits
                    .raw_value_for_test(i)
                    .unwrap())
                .collect::<Vec<_>>(),
            &[1, 0]
        );
        assert_eq!(complete(&mut kernel, &mut op).unwrap().value.index(), 1);
        // Both actual slots admitted exactly once, including the suspended one.
        let limit = budget.model_limits().unwrap().model_records;
        op.admit_model(LimitKind::ModelRecords, limit - 2).unwrap();
        assert_eq!(
            op.admit_model(LimitKind::ModelRecords, 1),
            Err(CoreError::ResourceLimit {
                kind: LimitKind::ModelRecords,
                requested: limit + 1,
                limit,
            })
        );
        drop(builder);
        assert_eq!(budget.usage().auxiliary_capacity, 0);
        let first = op.first_fault();
        assert_eq!(op.finish().err(), first);
    });
}

#[test]
fn task_packet_group_yields_before_any_write_and_resumes_once() {
    let budget = SourceBudget::task_fixture();
    let mut test_operation = budget.begin(CancellationToken::new());
    construction_scope(&mut test_operation, |mut op| {
        let mut builder = ExactBuilder::new(
            ExactCounts {
                atoms: 2,
                ..ExactCounts::default()
            },
            &mut op,
        )
        .unwrap();
        builder.reserve_atom(0, &mut op).unwrap();
        builder.ir.packets.push(packet(), &mut op).unwrap();
        builder.states[Table::Atoms as usize]
            .bits
            .replace(0, 2, &mut op)
            .unwrap();
        let second = builder.reserve_atom(1, &mut op).unwrap();
        let mut pending = Some(second.reference.value);
        let mut committed = 1;
        let mut kernel = Prefix {
            inner: CommitWrite {
                builder: &mut builder,
                pending: &mut pending,
                committed: &mut committed,
                index: 1,
                packet: packet(),
                control_ready: false,
            },
            // 189 advances + two bindings/control fill the window before grouped write.
            left: 189,
        };
        assert_eq!(run_window(&mut kernel, &mut op).unwrap(), BuildStep::Full);
        assert!(kernel.inner.control_ready);
        assert_eq!(kernel.inner.builder.ir.packets.len(), 1);
        assert_eq!(
            &(0..kernel.inner.builder.states[Table::Atoms as usize]
                .bits
                .len())
                .map(|i| kernel.inner.builder.states[Table::Atoms as usize]
                    .bits
                    .raw_value_for_test(i)
                    .unwrap())
                .collect::<Vec<_>>(),
            &[2, 1]
        );
        assert_eq!(*kernel.inner.pending, Some(second.reference.value));
        assert_eq!(*kernel.inner.committed, 1);
        complete(&mut kernel, &mut op).unwrap();
        assert_eq!(builder.ir.packets.len(), 2);
        assert_eq!(
            &(0..builder.states[Table::Atoms as usize].bits.len())
                .map(|i| builder.states[Table::Atoms as usize]
                    .bits
                    .raw_value_for_test(i)
                    .unwrap())
                .collect::<Vec<_>>(),
            &[2, 2]
        );
        assert_eq!(pending, None);
        assert_eq!(committed, 2);
        drop(builder);
        assert_eq!(budget.usage().auxiliary_capacity, 0);
        let first = op.first_fault();
        assert_eq!(op.finish().err(), first);
    });
}

struct PanicAfter<K>(K);

impl<'id, K: BuildKernel<'id>> BuildKernel<'id> for PanicAfter<K> {
    type Output = K::Output;

    fn run<'scope>(
        &'scope mut self,
        window: &mut BuildWindow<'scope, '_, 'id, '_>,
    ) -> Result<BuildStep<Self::Output>, CoreError> {
        let _ = self.0.run(window)?;
        panic!("injected abandon after actual construction action");
    }
}

#[test]
fn task_construction_window_abandon_preserves_first_owner_error() {
    let budget = SourceBudget::task_fixture();
    let mut test_operation = budget.begin(CancellationToken::new());
    construction_scope(&mut test_operation, |mut op| {
        let mut builder = ExactBuilder::new(
            ExactCounts {
                atoms: 2,
                ..ExactCounts::default()
            },
            &mut op,
        )
        .unwrap();
        builder.reserve_atom(0, &mut op).unwrap();
        let mut kernel = PanicAfter(Reserve::<AtomId> {
            builder: &mut builder,
            index: 1,
            phase: ReservePhase::Check,
            _type: std::marker::PhantomData,
        });
        assert!(
            catch_unwind(AssertUnwindSafe(|| {
                let _ = run_window(&mut kernel, &mut op);
            }))
            .is_err()
        );
        assert_eq!(op.first_fault(), Some(CoreError::InternalInvariant));
        assert!(matches!(
            builder.reserve_atom(1, &mut op),
            Err(CoreError::InternalInvariant)
        ));
        drop(builder);
        assert_eq!(budget.usage().auxiliary_capacity, 0);
        let first = op.first_fault();
        assert_eq!(op.finish().err(), first);
    });
}

#[test]
fn task_existing_node_table_write_keeps_unique_completion() {
    let budget = SourceBudget::task_fixture();
    let mut test_operation = budget.begin(CancellationToken::new());
    construction_scope(&mut test_operation, |mut op| {
        let mut builder = ExactBuilder::new(
            ExactCounts {
                nodes: 2,
                flows: 1,
                ..ExactCounts::default()
            },
            &mut op,
        )
        .unwrap();
        let nodes = builder.reserve_nodes(&mut op).unwrap();
        let flows = builder.reserve_flows(&mut op).unwrap();
        let flow = flows.at(0, &mut op).unwrap();
        for index in 0..2 {
            let node = nodes.at(index, &mut op).unwrap();
            builder
                .write_node(
                    node,
                    DraftNodeRow {
                        parent: None,
                        first_child: None,
                        next_sibling: None,
                        flow,
                        kind: NodeKind::Document,
                        detail: None,
                        source_envelope: SourceSpan::new(0, 0).unwrap(),
                        logical: LogicalSpan::new(0, 0).unwrap(),
                    },
                    &mut op,
                )
                .unwrap();
        }
        assert_eq!(builder.ir.nodes.len(), 2);
        assert_eq!(
            &(0..builder.states[Table::Nodes as usize].bits.len())
                .map(|i| builder.states[Table::Nodes as usize]
                    .bits
                    .raw_value_for_test(i)
                    .unwrap())
                .collect::<Vec<_>>(),
            &[2, 2]
        );
        let duplicate = nodes.at(1, &mut op).unwrap();
        assert!(matches!(
            builder.write_node(
                duplicate,
                DraftNodeRow {
                    parent: None,
                    first_child: None,
                    next_sibling: None,
                    flow,
                    kind: NodeKind::Document,
                    detail: None,
                    source_envelope: SourceSpan::new(0, 0).unwrap(),
                    logical: LogicalSpan::new(0, 0).unwrap(),
                },
                &mut op
            ),
            Err(CoreError::InvalidProvenance)
        ));
        drop((nodes, flows, builder));
        assert_eq!(budget.usage().auxiliary_capacity, 0);
        let first = op.first_fault();
        assert_eq!(op.finish().err(), first);
    });
}
