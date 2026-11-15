// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE file in the project root for license information.
// ---------------------------------------------------------------------------------------------

use super::*;
use crate::identity::SourceKey;
use crate::parse::prepared::NativeFault;
use crate::policy::{CancellationToken, SourceBudget, construction_scope};
use crate::source::SourceDocument;

struct PartitionCompiler<'id>(std::marker::PhantomData<fn(&'id ()) -> &'id ()>);

impl<'input, 'source, 'work, 'id, 'op>
    n::DirectCompiler<'input, PreparedBinding<'input, 'source, 'work, 'id, 'op>>
    for PartitionCompiler<'id>
{
    type Output = ();
    type State<'view> = CoreState<'view, 'input, 'id>;

    fn ast<'store, 'view, 'arena>(
        &mut self,
        reader: n::AstReader<'store, 'view, 'arena, 'input, ConstructionId<'id>, NativeLease<'id>>,
        host: &mut PreparedBinding<'input, 'source, 'work, 'id, 'op>,
    ) -> Result<Self::State<'view>, NativeFault<'input, 'id>> {
        host.native_parts(|source, work| compile_ast(&AstNative::new(reader, source), work))
    }

    fn provenance<'store, 'view>(
        &mut self,
        reader: n::ProvenanceReader<'store, 'view, 'input, ConstructionId<'id>, NativeLease<'id>>,
        state: Self::State<'view>,
        host: &mut PreparedBinding<'input, 'source, 'work, 'id, 'op>,
    ) -> Result<(), NativeFault<'input, 'id>> {
        host.native_parts(|source, work| {
            let native = Native { reader, source };
            let CoreState {
                graph,
                captured,
                mut builder,
            } = state;
            graph.measure(&native, &mut builder, work)?;
            builder.complete_nodes(work)?;
            // This test exercises the partition oracle on the actual retired
            // graph; it does not publish the intentionally unfinished builder.
            drop(captured);
            let flow = graph.flows.get(0, work)?;
            let finish = graph.nodes.get(flow.root(), work)?.end();
            let source = builder.source_parts(&[SourceSpan::new(0, 1)?], work)?;
            let mut target = Open::new(&mut builder, work, true)?;
            for owner in flow.root()..finish {
                let node = target
                    .writer
                    .with_read(|_, w| graph.nodes.get(owner, w).map(|r| r.node))?;
                for _ in 0..2 {
                    let ticket = target.writer.reserve_next()?;
                    let end = target.cursor + 1;
                    let born = target.writer.commit_owned(
                        ticket,
                        b::DraftOwnedAtom {
                            owner: node,
                            kind: AtomKind::Copy,
                            target: TargetSpan {
                                start: target.cursor,
                                end,
                            },
                            sources: source,
                            proof: None,
                            content: b::DraftAtomContent::Empty,
                            primary: b::DraftPrimaryAuthority {
                                phase: NativePhase::Block,
                                kind: ClaimKind::Block,
                                consumed: LocalUtf8Span::new(0, 1)?,
                                host_role: HostRole::Block,
                                host_utf8_len: 1,
                                host_domains: source,
                            },
                        },
                    )?;
                    target.record(born, owner, end, source, false)?;
                }
            }
            target.writer.with_read(|_, work| {
                for reverse in [false, true] {
                    let mut order = BuildGrowingVec::new(256, LimitKind::SourcePieces, work)?;
                    for at in 0..target.postings.len() {
                        let i = if reverse {
                            target.postings.len() - at - 1
                        } else {
                            at
                        };
                        order.push(i as u32, work)?;
                    }
                    let mut actual = Future::new(
                        flow.root()..finish,
                        &target.emissions,
                        &target.postings,
                        &order,
                        work,
                    )?;
                    assert_eq!(actual.general.is_some(), reverse);
                    let mut oracle = GeneralFuture::new(
                        flow.root()..finish,
                        &target.emissions,
                        &target.postings,
                        &order,
                        work,
                    )?;
                    for pi in 0..=order.len() {
                        for node in flow.root()..finish {
                            let end = graph.nodes.get(node, work)?.end();
                            let mut expected = NONE;
                            for rank in pi..order.len() {
                                let p = target
                                    .postings
                                    .get(growing(&order, rank, work)? as usize, work)?;
                                let owner = target.emissions.get(p.emission as usize, work)?.owner;
                                if node <= owner && owner < end {
                                    expected = rank as u32;
                                    break;
                                }
                            }
                            assert_eq!(
                                actual.minimum(
                                    &target.emissions,
                                    &target.postings,
                                    &order,
                                    pi,
                                    node..end,
                                    work
                                )?,
                                expected
                            );
                            assert_eq!(oracle.minimum(node..end, work)?, expected);
                        }
                        if pi < order.len() {
                            let p = target
                                .postings
                                .get(growing(&order, pi, work)? as usize, work)?;
                            actual.advance(&target.emissions, p, pi, work)?;
                            oracle.advance(
                                target.emissions.get(p.emission as usize, work)?.owner,
                                pi,
                                work,
                            )?;
                        }
                    }
                }
                Ok(())
            })?;
            let closed = target.close()?;
            drop(closed);
            drop(builder);
            drop(graph);
            Ok(())
        })
    }
}

#[test]
fn monotone_and_general_match_every_remaining_prefix_and_real_owner_subtree() {
    let budget = SourceBudget::model_fixture();
    let mut operation = budget.begin(CancellationToken::new());
    let source = SourceDocument::new(
        SourceKey::new([97; 16], 1).unwrap(),
        b"a &amp; b *c* d ![alt](u) ![](v)",
        &mut operation,
    )
    .unwrap()
    .snapshot();
    let input = ParseInput::prepare(source, &mut operation).unwrap();
    construction_scope(&mut operation, |mut work| {
        with_prepared(&input, &mut work, |input, mut host| {
            let result = n::compile(
                input,
                &super::options::c0(),
                &mut host,
                PartitionCompiler(std::marker::PhantomData),
            );
            host.resolve(result)
        })
        .unwrap();
        work.finish().unwrap();
    });
    drop(input);
    assert_eq!(budget.usage().auxiliary_capacity, 0);
    assert_eq!(budget.usage().source_capacity, 0);
}

#[test]
fn zero_order_uses_empty_first_member_and_original_id_without_assuming_positive_source_order() {
    let budget = SourceBudget::model_fixture();
    let mut operation = budget.begin(CancellationToken::new());
    let ir = construction_scope(&mut operation, |mut work| {
        let mut builder = b::DirectBuilder::new(&mut work).unwrap();
        let flow = builder.reserve_flow(&mut work).unwrap();
        let node = builder
            .reserve_node(
                b::DraftNodeSeed {
                    parent: None,
                    flow,
                    kind: NodeKind::Document,
                    detail: None,
                    initial: Some(SourceSpan::new(0, 10).unwrap()),
                },
                &mut work,
            )
            .unwrap();
        builder
            .write_node(
                node,
                b::DraftNodeRow {
                    parent: None,
                    first_child: None,
                    next_sibling: None,
                    flow,
                    kind: NodeKind::Document,
                    detail: None,
                    source_envelope: SourceSpan::new(0, 10).unwrap(),
                    logical: LogicalSpan::new(0, 2).unwrap(),
                },
                &mut work,
            )
            .unwrap();
        let domains = builder
            .source_parts(&[SourceSpan::new(0, 10).unwrap()], &mut work)
            .unwrap();
        let mut target = Open::new(&mut builder, &mut work, false).unwrap();
        for (spans, width) in [
            (vec![(8, 9)], 1u64),
            (vec![(1, 1), (7, 8)], 0),
            (vec![(1, 1), (6, 7)], 0),
            (vec![(2, 3)], 1),
        ] {
            let spans: Vec<_> = spans
                .into_iter()
                .map(|(a, b)| SourceSpan::new(a, b).unwrap())
                .collect();
            let (sources, text) = target
                .writer
                .with_rows(|b, w| {
                    Ok((
                        b.source_parts(&spans, w)?,
                        b.text(if width == 0 { "" } else { "x" }, w)?,
                    ))
                })
                .unwrap();
            let ticket = target.writer.reserve_next().unwrap();
            let end = target.cursor + width;
            let born = target
                .writer
                .commit_owned(
                    ticket,
                    b::DraftOwnedAtom {
                        owner: node,
                        kind: AtomKind::Synthetic,
                        target: TargetSpan {
                            start: target.cursor,
                            end,
                        },
                        sources,
                        proof: None,
                        content: b::DraftAtomContent::Pool(text),
                        primary: b::DraftPrimaryAuthority {
                            phase: NativePhase::Structure,
                            kind: ClaimKind::ProjectionSeparator(SyntheticKind::BlockLineFeed),
                            consumed: LocalUtf8Span::new(0, 0).unwrap(),
                            host_role: HostRole::Structure,
                            host_utf8_len: 0,
                            host_domains: sources,
                        },
                    },
                )
                .unwrap();
            target.record(born, 0, end, sources, false).unwrap();
        }
        let closed = target.close().unwrap();
        let block = closed.into_block(&mut work).unwrap();
        builder
            .write_flow(
                flow,
                b::DraftFlowRow {
                    scope: b::DraftFlowScope::Main,
                    root: node,
                    domains,
                    atoms: block.into(),
                    logical_utf16: 2,
                },
                &mut work,
            )
            .unwrap();
        let ir = builder.finish(&mut work).unwrap();
        work.finish().unwrap();
        ir
    });
    // Private permutation control only; this deliberately unusual target is
    // never passed to independent validation or published as an AcceptedModel.
    let actual: Vec<Vec<(u64, u64)>> = (0..ir.atoms_len())
        .map(|i| {
            let atom = ir.read_atom_at(i, &mut operation).unwrap();
            atom.sources
                .slice(ir.sources())
                .unwrap()
                .iter()
                .map(|s| (s.start(), s.end()))
                .collect()
        })
        .collect();
    assert_eq!(
        actual,
        [
            vec![(1, 1), (7, 8)],
            vec![(1, 1), (6, 7)],
            vec![(8, 9)],
            vec![(2, 3)]
        ]
    );
    drop(ir);
    assert_eq!(budget.usage().auxiliary_capacity, 0);
}

#[test]
fn clipped_candidates_use_clipped_length_then_original_heap_rank() {
    let budget = SourceBudget::model_fixture();
    let mut operation = budget.begin(CancellationToken::new());
    construction_scope(&mut operation, |mut work| {
        let mut rows = BuildRows::new(4, LimitKind::SourcePieces, &mut work).unwrap();
        // The first native span was wider before clipping; selection must use
        // these actual domain intersections, then the original sorted rank.
        for (part, a, b) in [(0, 3, 5), (1, 3, 6), (2, 3, 5)] {
            rows.push(
                Candidate {
                    part,
                    span: SourceSpan::new(a, b).unwrap(),
                },
                &mut work,
            )
            .unwrap();
        }
        let mut order = BuildGrowingVec::new(4, LimitKind::SourcePieces, &mut work).unwrap();
        for i in 0..3 {
            order.push(i, &mut work).unwrap();
        }
        sort(
            &mut order,
            |i, w| {
                let s = rows.get(i as usize, w)?.span;
                Ok((s.start(), s.end()))
            },
            &mut work,
        )
        .unwrap();
        let mut active = Active::new(4, &mut work).unwrap();
        for rank in 0..3 {
            active.push(rank, &rows, &order, &mut work).unwrap();
        }
        let first = growing(&order, 0, &mut work).unwrap();
        assert_eq!(
            active.best(3, &rows, &order, &mut work).unwrap().part,
            rows.get(first as usize, &mut work).unwrap().part
        );
        assert_eq!(active.best(5, &rows, &order, &mut work).unwrap().part, 1);
        drop((rows, order, active));
        work.finish().unwrap();
    });
}

#[test]
fn hidden_heap_crosses_pages_and_presorted_ties_keep_the_original_input() {
    let budget = SourceBudget::task_fixture();
    let mut operation = budget.begin(CancellationToken::new());
    construction_scope(&mut operation, |mut work| {
        work.set_clock_samples_for_test(&[100_000_000]);
        let mut parts = BuildRows::new(4096, LimitKind::SourcePieces, &mut work).unwrap();
        for id in 0..1050u32 {
            let start = ((id * 97) % 29) as u64;
            parts
                .push(
                    Part {
                        claim: id as usize,
                        span: SourceSpan::new(start, start + 2).unwrap(),
                    },
                    &mut work,
                )
                .unwrap();
        }
        assert!(parts.page_count() > 1);
        let mut order = BuildGrowingVec::new(4096, LimitKind::SourcePieces, &mut work).unwrap();
        for id in 0..1050 {
            order.push(id, &mut work).unwrap();
        }
        sort(
            &mut order,
            |id, w| {
                let s = parts.get(id as usize, w)?.span;
                Ok((s.start(), s.end()))
            },
            &mut work,
        )
        .unwrap();
        let sorted: Vec<_> = (0..order.len())
            .map(|at| growing(&order, at, &mut work).unwrap())
            .collect();
        let mut members = sorted.clone();
        members.sort();
        assert_eq!(members, (0..1050).collect::<Vec<_>>());
        for pair in sorted.windows(2) {
            assert!(
                parts.get(pair[0] as usize, &mut work).unwrap().span.start()
                    <= parts.get(pair[1] as usize, &mut work).unwrap().span.start()
            );
        }
        let tied = [58, 29, 0, 87];
        let mut order = BuildGrowingVec::new(4, LimitKind::SourcePieces, &mut work).unwrap();
        for id in tied {
            order.push(id, &mut work).unwrap();
        }
        sort(
            &mut order,
            |id, w| {
                let s = parts.get(id as usize, w)?.span;
                Ok((s.start(), s.end()))
            },
            &mut work,
        )
        .unwrap();
        assert_eq!(
            (0..4)
                .map(|at| growing(&order, at, &mut work).unwrap())
                .collect::<Vec<_>>(),
            tied
        );
        drop((order, parts));
        work.finish().unwrap();
    });
}
