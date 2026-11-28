// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE file in the project root for license information.
// ---------------------------------------------------------------------------------------------

//! Handwritten index controls do not manufacture a validation seal or native
//! AcceptedModel. They exercise the same fixed table construction in isolation.

use super::*;
use crate::config::{ParseConfig, Profile};
use crate::identity::{LogicalSpan, ModelIdentity, SourceKey, SourceSpan};
use crate::ir::{
    AtomContent, FlowId, FlowRow, FlowScope, IrBuilder, NodeKind, NodeRow, TargetSpan,
};
use crate::memory::BuildGrowingVec;
use crate::policy::{CancellationToken, Operation, SourceBudget, construction_scope};
use crate::source::SourceDocument;
use std::panic::{AssertUnwindSafe, catch_unwind};

fn compare_responses<T: Copy + Eq + std::fmt::Debug>(
    old: Result<crate::mapping::QueryResponse<T>, CoreError>,
    new: Result<crate::mapping::QueryResponse<T>, CoreError>,
) {
    match (old, new) {
        (Ok(old), Ok(new)) => {
            assert_eq!(old.results(), new.results());
            assert_eq!(old.boundaries(), new.boundaries());
            assert_eq!(old.selections(), new.selections());
            assert_eq!(old.spans(), new.spans());
            assert_eq!(old.anchors(), new.anchors());
            assert_eq!(old.contributors(), new.contributors());
            assert_eq!(old.contributions(), new.contributions());
        }
        (Err(old), Err(new)) => assert_eq!(old, new),
        _ => panic!("mapping success/error differs"),
    }
}

#[test]
fn real_c0_accepted_models_keep_ir_and_source_flow_payload_mapping() {
    use crate::identity::{LogicalBoundary, PayloadBoundary, PayloadSpan, SourceBoundary};
    use crate::ir::{PayloadId, TextTarget};
    use crate::mapping::{Affinity, PointInput, PointQuery, RangeInput, RangeQuery};
    use crate::model::AcceptedModel;
    let text = "# 中文 😀\n\n[a *b*][r] ![x &amp; y](img) `a\nb` &amp;\n\n[r]: /u \"title\"\n";
    let old_budget = SourceBudget::task_fixture();
    let new_budget = SourceBudget::task_fixture();
    let make = |budget: &SourceBudget| {
        let mut op = budget.begin(CancellationToken::new());
        let source = SourceDocument::new(
            SourceKey::new([0x4A; 16], 1).unwrap(),
            text.as_bytes(),
            &mut op,
        )
        .unwrap();
        AcceptedModel::parse(source.snapshot(), ParseConfig::new(Profile::C0), 1, &mut op).unwrap()
    };
    let old = make(&old_budget);
    let new = make(&new_budget);
    macro_rules! compare_tables {
        ($($table:ident),+ $(,)?) => {$(assert_eq!(old.ir().$table(), new.ir().$table());)+};
    }
    compare_tables!(
        nodes,
        flows,
        details,
        objects,
        payloads,
        definitions,
        references,
        proofs,
        primitives,
        stages,
        attributes,
        sources,
        alignments
    );
    assert_eq!(old.ir().atoms_len(), new.ir().atoms_len());
    for at in 0..old.ir().atoms_len() {
        let mut a = old_budget.begin(CancellationToken::new());
        let mut b = new_budget.begin(CancellationToken::new());
        assert_eq!(
            old.ir().atom_at(at, &mut a).unwrap(),
            new.ir().atom_at(at, &mut b).unwrap()
        );
    }
    let target = TextTarget::Flow(old.main_flow_id());
    let mut points = Vec::new();
    for at in 0..=text.len() {
        if text.is_char_boundary(at) {
            points.push(PointInput::Source {
                target,
                byte: SourceBoundary::new(at as u64),
            });
        }
    }
    for (index, flow) in old.ir().flows().iter().enumerate() {
        for at in 0..=flow.logical_utf16 {
            points.push(PointInput::Flow {
                flow: FlowId::from_index(index).unwrap(),
                utf16: LogicalBoundary::new(at),
            });
        }
    }
    for (index, payload) in old.ir().payloads().iter().enumerate() {
        for at in 0..=payload.utf8_len {
            points.push(PointInput::Payload {
                payload: PayloadId::from_index(index).unwrap(),
                utf8: PayloadBoundary::new(at),
            });
        }
    }
    for input in points {
        for affinity in [Affinity::Before, Affinity::After] {
            let query = PointQuery {
                model: old.identity(),
                input,
                affinity,
            };
            let mut a = old_budget.begin(CancellationToken::new());
            let mut b = new_budget.begin(CancellationToken::new());
            compare_responses(old.map_point(&query, &mut a), new.map_point(&query, &mut b));
        }
    }
    let mut ranges = vec![RangeInput::Source {
        target,
        span: SourceSpan::new(0, text.len() as u64).unwrap(),
    }];
    ranges.extend(
        old.ir()
            .flows()
            .iter()
            .enumerate()
            .map(|(index, flow)| RangeInput::Flow {
                flow: FlowId::from_index(index).unwrap(),
                span: LogicalSpan::new(0, flow.logical_utf16).unwrap(),
            }),
    );
    ranges.extend(
        old.ir()
            .payloads()
            .iter()
            .enumerate()
            .map(|(index, payload)| RangeInput::Payload {
                payload: PayloadId::from_index(index).unwrap(),
                span: PayloadSpan::new(0, payload.utf8_len).unwrap(),
            }),
    );
    for input in ranges {
        let query = RangeQuery {
            model: old.identity(),
            input,
            empty_affinity: Affinity::Before,
        };
        let mut a = old_budget.begin(CancellationToken::new());
        let mut b = new_budget.begin(CancellationToken::new());
        compare_responses(old.map_range(&query, &mut a), new.map_range(&query, &mut b));
    }
    drop((old, new));
    for budget in [&old_budget, &new_budget] {
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
}

struct Fixture {
    source: SourceSnapshot,
    ir: CompactIr,
    op: Operation,
}

fn fixture(budget: &SourceBudget) -> Fixture {
    let mut op = budget.begin(CancellationToken::new());
    let key = SourceKey::new([0x49; 16], 1).unwrap();
    let source = SourceDocument::new(key, &vec![b'a'; 1_200], &mut op)
        .unwrap()
        .snapshot();
    let mut builder = IrBuilder::new(&mut op).unwrap();
    let domain = builder
        .source_parts(&[SourceSpan::new(0, 1_200).unwrap()], &mut op)
        .unwrap();
    let node = crate::ir::NodeId::from_index(0).unwrap();
    let flow = FlowId::from_index(0).unwrap();
    let mut target = 600;
    let members: Vec<_> = (0..600)
        .map(|i| SourceSpan::new(i, i + 1).unwrap())
        .collect();
    let parts = builder.source_parts(&members, &mut op).unwrap();
    let content = AtomContent::Pool(builder.text(&"a".repeat(600), &mut op).unwrap());
    builder
        .atom(
            AtomRow {
                owner: node,
                kind: AtomKind::Transform,
                target: TargetSpan { start: 0, end: 600 },
                sources: parts,
                claim: IrBuilder::unbound_authority(),
                proof: None,
                content,
            },
            &mut op,
        )
        .unwrap();
    for index in 600..1_200 {
        let kind = match index % 3 {
            0 => AtomKind::Copy,
            1 => AtomKind::Hidden,
            _ => AtomKind::Synthetic,
        };
        let span = SourceSpan::new(index, index + u64::from(kind != AtomKind::Synthetic)).unwrap();
        let sources = builder.source_parts(&[span], &mut op).unwrap();
        let next = target + u64::from(kind != AtomKind::Hidden);
        let content = if kind == AtomKind::Synthetic {
            AtomContent::Pool(builder.text("a", &mut op).unwrap())
        } else {
            AtomContent::Empty
        };
        builder
            .atom(
                AtomRow {
                    owner: node,
                    kind,
                    target: TargetSpan {
                        start: target,
                        end: next,
                    },
                    sources,
                    claim: IrBuilder::unbound_authority(),
                    proof: None,
                    content,
                },
                &mut op,
            )
            .unwrap();
        target = next;
    }
    builder
        .node(
            NodeRow {
                parent: None,
                first_child: None,
                next_sibling: None,
                flow,
                kind: NodeKind::Document,
                detail: None,
                source_envelope: SourceSpan::new(0, 1_200).unwrap(),
                logical: LogicalSpan::new(0, target).unwrap(),
            },
            &mut op,
        )
        .unwrap();
    builder
        .flow(
            FlowRow {
                scope: FlowScope::Main,
                root: node,
                domains: domain,
                atoms: Rows::new(0, 601).unwrap(),
                logical_utf16: target,
            },
            &mut op,
        )
        .unwrap();
    let ir = builder.ir.snapshot(&mut op).unwrap();
    drop(builder);
    Fixture { source, ir, op }
}

fn draft<'id>(
    ir: &CompactIr,
    source: &SourceSnapshot,
    op: &mut OwnedWork<'id, '_>,
) -> (IndexDraft<'id>, u64) {
    let counts = super::super::count_index(ir, op).unwrap();
    let total = counts.total(op).unwrap();
    let max = op.model_limits().unwrap().index_records;
    let mut index = IndexDraft {
        identity: ModelIdentity::new(source.key(), 1, ParseConfig::new(Profile::C0)).unwrap(),
        flow_count: 1,
        targets: BuildExactRows::new(counts.targets, op).unwrap(),
        source: BuildExactRows::new(counts.source, op).unwrap(),
        synthetic: BuildExactRows::new(counts.synthetic, op).unwrap(),
        target: BuildExactRows::new(counts.target, op).unwrap(),
        hidden: BuildExactRows::new(counts.hidden, op).unwrap(),
        checkpoints: BuildGrowingVec::new(max, LimitKind::IndexRecords, op).unwrap(),
        sparse: BuildExactRows::new(counts.sparse, op).unwrap(),
        scalar_steps: 0,
    };
    for _ in 0..ir.atoms_len() {
        super::super::push(&mut index.sparse, Rows::new(0, 0).unwrap(), op).unwrap();
    }
    (index, total)
}

#[test]
fn real_postings_match_original_tables_and_admit_each_record_once() {
    let old_budget = SourceBudget::model_fixture();
    let new_budget = SourceBudget::task_fixture();
    let build = |budget: &SourceBudget, task: bool| {
        let Fixture { source, ir, mut op } = fixture(budget);
        let (index, total) = construction_scope(&mut op, |mut work| {
            let (mut index, total) = draft(&ir, &source, &mut work);
            if task {
                index = populate(index, &ir, &source, &mut work).unwrap();
            } else {
                index.populate(&ir, &source, false, &mut work).unwrap();
            }
            let index = index.freeze(&mut work).unwrap();
            let max = budget.model_limits().unwrap().index_records;
            work.admit_model(LimitKind::IndexRecords, max - total)
                .unwrap();
            let error = work.admit_model(LimitKind::IndexRecords, 1).unwrap_err();
            assert!(
                matches!(error, CoreError::ResourceLimit { kind: LimitKind::IndexRecords, requested, limit } if requested == max + 1 && limit == max)
            );
            assert_eq!(work.finish(), Err(error));
            (index, total)
        });
        (index, total, ir, source)
    };
    let old = build(&old_budget, false);
    let new = build(&new_budget, true);
    assert_eq!(new.0.targets.as_slice(), old.0.targets.as_slice());
    assert_eq!(new.0.source.as_slice(), old.0.source.as_slice());
    assert_eq!(new.0.synthetic.as_slice(), old.0.synthetic.as_slice());
    assert_eq!(new.0.target.as_slice(), old.0.target.as_slice());
    assert_eq!(new.0.hidden.as_slice(), old.0.hidden.as_slice());
    assert_eq!(new.0.source.len(), 1_000);
    assert_eq!(new.0.synthetic.len(), 200);
    assert_eq!(new.0.target.len(), 401);
    assert_eq!(new.0.hidden.len(), 200);
    assert_eq!(new.0.checkpoints.len(), 0);
    assert_eq!(new.1, old.1);
    drop((new, old));
    for budget in [&old_budget, &new_budget] {
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
}

#[test]
fn abandoned_construction_destroys_unpublished_index_and_latches_original_work() {
    let budget = SourceBudget::task_fixture();
    let Fixture { source, ir, mut op } = fixture(&budget);
    let retained = budget.usage().auxiliary_capacity;
    let mut written = (0, 0);
    let unwind = catch_unwind(AssertUnwindSafe(|| {
        construction_scope(&mut op, |mut work| {
            let (mut index, _) = draft(&ir, &source, &mut work);
            let mut cursor = Postings {
                index: &mut index,
                ir: &ir,
                position: 0,
                end: ir.atoms_len(),
                source_count: ir.sources().len(),
                length: ir.flows()[0].logical_utf16,
                target_end: 0,
                atom: None,
                member: 0,
                member_end: 0,
                pending: None,
                stage: Stage::ReadAtom,
                finished: false,
            };
            // Each lazy allocation is a complete original push after all views end.
            for _ in 0..2 {
                match run_window(&mut cursor, &mut work).unwrap() {
                    BuildStep::Ready(Outside::LazyPush(pending)) => {
                        match pending {
                            Pending::Target { hidden, row } => {
                                let rows = if hidden {
                                    &mut cursor.index.hidden
                                } else {
                                    &mut cursor.index.target
                                };
                                super::super::push(rows, row, &mut work).unwrap();
                            }
                            Pending::Source { synthetic, row } => {
                                let rows = if synthetic {
                                    &mut cursor.index.synthetic
                                } else {
                                    &mut cursor.index.source
                                };
                                super::super::push(rows, row, &mut work).unwrap();
                            }
                        }
                        cursor.after_push(pending);
                    }
                    _ => panic!("first two writes allocate target and source"),
                }
            }
            assert!(matches!(
                run_window(&mut cursor, &mut work).unwrap(),
                BuildStep::Full
            ));
            assert!(cursor.index.source.len() > 1);
            written = (cursor.index.target.len(), cursor.index.source.len());
            panic!("unwind after real unpublished writes");
        });
    }));
    assert!(unwind.is_err());
    assert!(written.0 > 0 && written.1 > 1);
    assert_eq!(op.first_fault(), Some(CoreError::InternalInvariant));
    assert_eq!(op.checkpoint(), Err(CoreError::InternalInvariant));
    assert_eq!(budget.usage().auxiliary_capacity, retained);
    // Independent frozen input remains readable; the consumed draft has no
    // handle with which a fresh operation could resume partial construction.
    let mut fresh = budget.begin(CancellationToken::new());
    assert!(ir.atom_at(0, &mut fresh).is_ok());
    drop((ir, source));
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
fn source_posting_partition_rejects_overlap_and_locks_original_work() {
    let budget = SourceBudget::task_fixture();
    let mut op = budget.begin(CancellationToken::new());
    construction_scope(&mut op, |mut work| {
        let mut rows = BuildExactRows::new(2, &mut work).unwrap();
        for (start, end) in [(0, 2), (1, 3)] {
            rows.push(
                Posting {
                    span: SourceSpan::new(start, end).unwrap(),
                    atom: AtomId::from_index(0).unwrap(),
                    part: 0,
                },
                &mut work,
            )
            .unwrap();
        }
        assert_eq!(
            check_source_partition(&rows, 0, &mut work),
            Err(CoreError::InvalidProvenance)
        );
        assert_eq!(work.first_fault(), Some(CoreError::InvalidProvenance));
        assert_eq!(
            check_source_partition(&rows, 0, &mut work),
            Err(CoreError::InvalidProvenance)
        );
        assert_eq!(rows.raw_value_for_test(1).unwrap().span.start(), 1);
        drop(rows);
        assert_eq!(budget.usage().auxiliary_capacity, 0);
        assert_eq!(work.finish(), Err(CoreError::InvalidProvenance));
    });
}
