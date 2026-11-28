// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE file in the project root for license information.
// ---------------------------------------------------------------------------------------------

//! Pre-execution oracle: TABLE-INTERRUPTION-DELTA-v2.md plus the independent
//! review-v2 correction: Quote S[4,6) is anchored at L2, never L1.
//! No candidate output was used to select these expected values.

use super::*;
use markdown_core::identity::LogicalBoundary;
use markdown_core::ir::{AtomRow, ClaimId, HostRole, NativePhase, PayloadOwner, PayloadRole};
use markdown_core::mapping::{Effects, PointResult, Position, QueryResponse, Related};

#[derive(Clone, Copy)]
struct Authority {
    owner: usize,
    kind: ClaimKind,
    role: HostRole,
    phase: NativePhase,
    domain: (u64, u64),
    consumed: (u64, u64),
}

fn authority(model: &AcceptedModel, claim: ClaimId, expected: Authority, sources: &[(u64, u64)]) {
    let mut op = model.budget().begin(CancellationToken::new());
    let value = model.ir().authority(claim, &mut op).unwrap();
    assert_eq!(value.owner.index(), expected.owner);
    assert_eq!(value.host.owner.index(), expected.owner);
    assert_eq!(value.kind, expected.kind);
    assert_eq!(value.host.role, expected.role);
    assert_eq!(value.phase, expected.phase);
    assert_eq!(spans(model, value.host.domains), vec![expected.domain]);
    assert_eq!(value.host.utf8_len, expected.domain.1 - expected.domain.0);
    assert_eq!(
        (value.consumed.start(), value.consumed.end()),
        expected.consumed
    );
    assert_eq!(spans(model, value.sources), sources);
    assert_eq!(op.first_fault(), None);
}

fn block(owner: usize, kind: ClaimKind, domain: (u64, u64), consumed: (u64, u64)) -> Authority {
    Authority {
        owner,
        kind,
        role: HostRole::Block,
        phase: NativePhase::Block,
        domain,
        consumed,
    }
}

fn inline(owner: usize, domain: (u64, u64), consumed: (u64, u64)) -> Authority {
    Authority {
        owner,
        kind: ClaimKind::Block,
        role: HostRole::Inline,
        phase: NativePhase::Inline,
        domain,
        consumed,
    }
}

// Pre-execution static correction: flows emits BlockLineFeed with Structure
// phase, and direct_target::synthetic consequently uses Structure host role.
// Original pre-execution bytes are retained as grammar-tests-preexecution-v1.rs.
fn block_separator(owner: usize, anchor: u64) -> Authority {
    Authority {
        owner,
        kind: ClaimKind::ProjectionSeparator(SyntheticKind::BlockLineFeed),
        role: HostRole::Structure,
        phase: NativePhase::Structure,
        domain: (anchor, anchor),
        consumed: (0, 0),
    }
}

#[derive(Clone, Copy)]
struct Part {
    kind: AtomKind,
    source: (u64, u64),
    target: (u64, u64),
    auth: Authority,
}

fn part(kind: AtomKind, source: (u64, u64), target: (u64, u64), auth: Authority) -> Part {
    Part {
        kind,
        source,
        target,
        auth,
    }
}

/// Bind IDs from the independently checked parent, before querying. Every
/// physical member is retained; grouping never merges owners or authorities.
fn partition(model: &AcceptedModel, expected: &[Part]) -> Vec<Vec<(usize, AtomRow)>> {
    assert_eq!(
        model.identity().source(),
        SourceKey::new([0x63; 16], 1).unwrap()
    );
    assert_eq!(model.identity().generation(), 1);
    assert_eq!(model.identity().config(), ParseConfig::new(Profile::G3));
    let ir = model.ir();
    let flow = ir.flows()[model.main_flow_id().index()];
    let mut result = vec![Vec::new(); expected.len()];
    let mut covered = vec![false; model.source().text().len()];
    let mut op = model.budget().begin(CancellationToken::new());
    for i in flow.atoms.start()..flow.atoms.start() + flow.atoms.len() {
        let atom = ir.atom_at(i, &mut op).unwrap();
        let source = spans(model, atom.sources);
        assert_eq!(source.len(), 1);
        let s = source[0];
        let matches: Vec<_> = expected
            .iter()
            .enumerate()
            .filter(|(_, e)| {
                atom.kind == e.kind
                    && atom.owner.index() == e.auth.owner
                    && if e.source.0 == e.source.1 {
                        s == e.source
                    } else {
                        s.0 < s.1 && e.source.0 <= s.0 && s.1 <= e.source.1
                    }
            })
            .collect();
        assert_eq!(matches.len(), 1, "unexpected/ambiguous atom {atom:?}");
        let (index, e) = matches[0];
        let target = if e.kind == AtomKind::Copy {
            (e.target.0 + s.0 - e.source.0, e.target.0 + s.1 - e.source.0)
        } else {
            e.target
        };
        assert_eq!((atom.target.start, atom.target.end), target);
        assert_eq!(atom.proof, None);
        if matches!(atom.kind, AtomKind::Hidden | AtomKind::Copy) {
            assert_eq!(atom.content, AtomContent::Empty);
        }
        if atom.kind == AtomKind::Synthetic {
            let AtomContent::Pool(text) = atom.content else {
                panic!()
            };
            assert_eq!(ir.text(text).unwrap(), "\n");
        }
        authority(model, atom.claim, e.auth, &source);
        for byte in s.0..s.1 {
            assert!(
                !std::mem::replace(&mut covered[byte as usize], true),
                "duplicate source"
            );
        }
        result[index].push((i, atom));
    }
    assert!(covered.iter().all(|v| *v), "missing source");
    for (e, members) in expected.iter().zip(&result) {
        assert!(!members.is_empty());
        if e.source.0 == e.source.1 {
            assert_eq!(members.len(), 1);
        }
    }
    result
}

fn one(groups: &[Vec<(usize, AtomRow)>], at: usize) -> (usize, AtomRow) {
    assert_eq!(groups[at].len(), 1);
    groups[at][0]
}

fn related<T: Copy>(
    response: &QueryResponse<T>,
    value: Related,
    reverse: bool,
    intervals: &[(u64, u64)],
    anchors: &[u64],
    envelope: (u64, u64),
) {
    let actual: Vec<_> = response.spans()
        [value.spans.start()..value.spans.start() + value.spans.len()]
        .iter()
        .map(|v| {
            assert_eq!(
                matches!(v, markdown_core::mapping::RelatedSpan::Source(_)),
                reverse
            );
            (v.start(), v.end())
        })
        .collect();
    assert_eq!(actual, intervals);
    let actual: Vec<_> = response.anchors()
        [value.anchors.start()..value.anchors.start() + value.anchors.len()]
        .iter()
        .map(|v| {
            assert_eq!(matches!(v, Position::Source(_)), reverse);
            v.value()
        })
        .collect();
    assert_eq!(actual, anchors);
    assert_eq!(value.envelope.map(|v| (v.start(), v.end())), Some(envelope));
}

#[allow(clippy::too_many_arguments)]
fn boundary<T: Copy>(
    model: &AcceptedModel,
    response: &QueryResponse<T>,
    actual: PointResult,
    reverse: bool,
    chosen: u64,
    intervals: &[(u64, u64)],
    anchors: &[u64],
    envelope: (u64, u64),
    members: &[(usize, AtomRow, Correspondence)],
    effects: Effects,
) {
    assert_eq!(actual.model, model.identity());
    assert_eq!(actual.target, TextTarget::Flow(model.main_flow_id()));
    assert_eq!(
        actual.direction,
        if reverse {
            Direction::TargetToSource
        } else {
            Direction::SourceToTarget
        }
    );
    assert_eq!(
        actual.chosen,
        if reverse {
            Position::Source(SourceBoundary::new(chosen))
        } else {
            Position::Flow(LogicalBoundary::new(chosen))
        }
    );
    assert_eq!(actual.effects, effects);
    assert_eq!(
        actual.correspondence,
        if effects.hidden {
            Correspondence::Hidden
        } else {
            Correspondence::Exact
        }
    );
    related(
        response,
        actual.related,
        reverse,
        intervals,
        anchors,
        envelope,
    );
    let mut expected = members.to_vec();
    expected.sort_by_key(|v| v.0);
    let values = &response.contributors()
        [actual.contributors.start()..actual.contributors.start() + actual.contributors.len()];
    assert_eq!(values.len(), expected.len());
    for (v, (id, atom, relation)) in values.iter().zip(expected) {
        assert_eq!(v.atom.index(), id);
        assert_eq!(v.source_part, 0);
        assert_eq!(v.kind, atom.kind);
        assert_eq!(v.source, model.ir().sources()[atom.sources.start()]);
        assert_eq!(v.target, atom.target);
        assert_eq!(v.relation, relation);
    }
}

fn exact_queries(model: &AcceptedModel, source: u64, target: u64, member: (usize, AtomRow)) {
    for reverse in [false, true] {
        let chosen = if reverse { source } else { target };
        let flow = model.main_flow_id();
        for affinity in [Affinity::Before, Affinity::After] {
            let mut op = model.budget().begin(CancellationToken::new());
            let input = if reverse {
                PointInput::Flow {
                    flow,
                    utf16: LogicalBoundary::new(target),
                }
            } else {
                PointInput::Source {
                    target: TextTarget::Flow(flow),
                    byte: SourceBoundary::new(source),
                }
            };
            let response = model
                .map_point(
                    &PointQuery {
                        model: model.identity(),
                        input,
                        affinity,
                    },
                    &mut op,
                )
                .unwrap();
            assert_eq!(response.results().len(), 1);
            boundary(
                model,
                &response,
                response.results()[0],
                reverse,
                chosen,
                &[],
                &[chosen],
                (chosen, chosen),
                &[(member.0, member.1, Correspondence::Exact)],
                Effects::default(),
            );
        }
        let mut op = model.budget().begin(CancellationToken::new());
        let input = if reverse {
            RangeInput::Flow {
                flow,
                span: LogicalSpan::new(target, target + 1).unwrap(),
            }
        } else {
            RangeInput::Source {
                target: TextTarget::Flow(flow),
                span: SourceSpan::new(source, source + 1).unwrap(),
            }
        };
        let response = model
            .map_range(
                &RangeQuery {
                    model: model.identity(),
                    input,
                    empty_affinity: Affinity::Before,
                },
                &mut op,
            )
            .unwrap();
        let [RangeResult::Selection(rows)] = response.results() else {
            panic!()
        };
        assert_eq!(rows.len(), 1);
        let selection = response.selections()[rows.start()];
        assert_eq!(selection.model, model.identity());
        assert_eq!(selection.target, TextTarget::Flow(flow));
        assert_eq!(
            selection.direction,
            if reverse {
                Direction::TargetToSource
            } else {
                Direction::SourceToTarget
            }
        );
        assert_eq!(selection.effects, Effects::default());
        assert!(!selection.expanded);
        assert_eq!(selection.contributions.len(), 1);
        let c = response.contributions()[selection.contributions.start()];
        assert_eq!(
            (c.atom.index(), c.source_part, c.kind, c.expanded),
            (member.0, 0, AtomKind::Copy, false)
        );
        assert_eq!((c.source.start(), c.source.end()), (source, source + 1));
        assert_eq!((c.target.start, c.target.end), (target, target + 1));
        related(
            &response,
            selection.related,
            reverse,
            &[(chosen, chosen + 1)],
            &[],
            (chosen, chosen + 1),
        );
        for (point, at) in [(selection.start, chosen), (selection.end, chosen + 1)] {
            boundary(
                model,
                &response,
                point,
                reverse,
                at,
                &[],
                &[at],
                (at, at),
                &[(member.0, member.1, Correspondence::Exact)],
                Effects::default(),
            );
        }
    }
}

fn hidden_queries(
    model: &AcceptedModel,
    input: (u64, u64),
    point: u64,
    target: u64,
    member: (usize, AtomRow),
) {
    let flow = model.main_flow_id();
    let hidden = Effects {
        hidden: true,
        ..Effects::default()
    };
    for affinity in [Affinity::Before, Affinity::After] {
        let mut op = model.budget().begin(CancellationToken::new());
        let r = model
            .map_point(
                &PointQuery {
                    model: model.identity(),
                    input: PointInput::Source {
                        target: TextTarget::Flow(flow),
                        byte: SourceBoundary::new(point),
                    },
                    affinity,
                },
                &mut op,
            )
            .unwrap();
        assert_eq!(r.results().len(), 1);
        boundary(
            model,
            &r,
            r.results()[0],
            false,
            target,
            &[],
            &[target],
            (target, target),
            &[(member.0, member.1, Correspondence::Hidden)],
            hidden,
        );
    }
    let mut op = model.budget().begin(CancellationToken::new());
    let r = model
        .map_range(
            &RangeQuery {
                model: model.identity(),
                input: RangeInput::Source {
                    target: TextTarget::Flow(flow),
                    span: SourceSpan::new(input.0, input.1).unwrap(),
                },
                empty_affinity: Affinity::After,
            },
            &mut op,
        )
        .unwrap();
    let [RangeResult::Selection(rows)] = r.results() else {
        panic!()
    };
    assert_eq!(rows.len(), 1);
    let s = r.selections()[rows.start()];
    assert_eq!(s.model, model.identity());
    assert_eq!(s.target, TextTarget::Flow(flow));
    assert_eq!(s.direction, Direction::SourceToTarget);
    assert_eq!(s.effects, hidden);
    assert!(!s.expanded);
    assert_eq!(s.contributions.len(), 1);
    let c = r.contributions()[s.contributions.start()];
    assert_eq!(
        (c.atom.index(), c.source_part, c.kind, c.expanded),
        (member.0, 0, AtomKind::Hidden, false)
    );
    assert_eq!((c.source.start(), c.source.end()), input);
    assert_eq!((c.target.start, c.target.end), (target, target));
    related(&r, s.related, false, &[], &[target], (target, target));
    for b in [s.start, s.end] {
        boundary(
            model,
            &r,
            b,
            false,
            target,
            &[],
            &[target],
            (target, target),
            &[(member.0, member.1, Correspondence::Hidden)],
            hidden,
        );
    }
}

#[test]
fn g3_delta_split_has_complete_parent_partition_and_bidirectional_queries() {
    use NodeKind::*;
    with_model("p\n|abc|\n|---|\n", |model| {
        structural(
            model,
            &[
                node(Document, None, 0, 5),
                node(Paragraph, Some(0), 0, 1),
                node(Text, Some(1), 0, 1),
                node(Table, Some(0), 2, 5),
                node(TableRow, Some(3), 2, 5),
                node(TableCell, Some(4), 2, 5),
                node(Text, Some(5), 2, 5),
            ],
            "p\nabc",
            3,
            0,
            4,
        );
        assert_eq!(main_text(model), "p\nabc");
        let ir = model.ir();
        for (n, e) in
            ir.nodes()
                .iter()
                .zip([(0, 14), (0, 2), (0, 1), (2, 14), (2, 8), (3, 6), (3, 6)])
        {
            assert_eq!((n.source_envelope.start(), n.source_envelope.end()), e);
        }
        assert_eq!(
            ir.alignments().iter().copied().collect::<Vec<_>>(),
            vec![Alignment::None]
        );
        let detail = |i: usize| ir.details()[ir.nodes()[i].detail.unwrap().index()];
        let DetailRow::Table {
            columns,
            alignments,
            claim,
        } = detail(3)
        else {
            panic!()
        };
        assert_eq!(columns, 1);
        assert_eq!(alignments.len(), 1);
        authority(
            model,
            claim,
            block(3, ClaimKind::Table, (8, 14), (0, 6)),
            &[(8, 14)],
        );
        let DetailRow::Row {
            index,
            header,
            claim,
        } = detail(4)
        else {
            panic!()
        };
        assert_eq!((index, header), (0, true));
        authority(
            model,
            claim,
            block(4, ClaimKind::Table, (2, 8), (0, 6)),
            &[(2, 8)],
        );
        let DetailRow::Cell {
            column,
            missing,
            claim,
        } = detail(5)
        else {
            panic!()
        };
        assert_eq!((column, missing), (0, false));
        authority(
            model,
            claim,
            block(5, ClaimKind::Table, (2, 8), (1, 4)),
            &[(3, 6)],
        );
        for (a, (owner, kind, source)) in ir.attributes().iter().zip([
            (3, AttributeKind::TableColumns, (8, 14)),
            (3, AttributeKind::TableAlignment, (9, 12)),
            (4, AttributeKind::RowHeader, (2, 8)),
            (5, AttributeKind::CellMissing, (3, 6)),
        ]) {
            assert_eq!((a.owner.index(), a.kind), (owner, kind));
            assert_eq!(spans(model, a.sources), vec![source]);
            assert_eq!(
                a.claim,
                match detail(owner) {
                    DetailRow::Table { claim, .. }
                    | DetailRow::Row { claim, .. }
                    | DetailRow::Cell { claim, .. } => claim,
                    _ => panic!(),
                }
            );
        }
        let groups = partition(
            model,
            &[
                part(AtomKind::Copy, (0, 1), (0, 1), inline(2, (0, 2), (0, 1))),
                part(AtomKind::Hidden, (1, 2), (1, 1), inline(1, (0, 2), (1, 2))),
                part(
                    AtomKind::Hidden,
                    (2, 3),
                    (2, 2),
                    block(3, ClaimKind::Table, (2, 8), (0, 1)),
                ),
                part(AtomKind::Synthetic, (2, 2), (1, 2), block_separator(3, 2)),
                part(AtomKind::Copy, (3, 6), (2, 5), inline(6, (3, 6), (0, 3))),
                part(
                    AtomKind::Hidden,
                    (6, 8),
                    (5, 5),
                    block(3, ClaimKind::Table, (2, 8), (4, 6)),
                ),
                part(
                    AtomKind::Hidden,
                    (8, 14),
                    (5, 5),
                    block(3, ClaimKind::Table, (8, 14), (0, 6)),
                ),
            ],
        );
        exact_queries(model, 4, 3, one(&groups, 4));
        hidden_queries(model, (9, 10), 10, 5, one(&groups, 6));
        let mut op = model.budget().begin(CancellationToken::new());
        let flow = model.main_flow_id();
        let r = model
            .map_range(
                &RangeQuery {
                    model: model.identity(),
                    input: RangeInput::Flow {
                        flow,
                        span: LogicalSpan::new(1, 2).unwrap(),
                    },
                    empty_affinity: Affinity::Before,
                },
                &mut op,
            )
            .unwrap();
        let [RangeResult::Selection(rows)] = r.results() else {
            panic!()
        };
        assert_eq!(rows.len(), 1);
        let s = r.selections()[rows.start()];
        assert_eq!(s.model, model.identity());
        assert_eq!(s.target, TextTarget::Flow(flow));
        assert_eq!(s.direction, Direction::TargetToSource);
        assert!(!s.expanded);
        assert_eq!(
            s.effects,
            Effects {
                synthetic: true,
                ..Effects::default()
            }
        );
        assert_eq!(s.contributions.len(), 1);
        let c = r.contributions()[s.contributions.start()];
        assert_eq!(
            (c.atom.index(), c.source_part, c.kind, c.expanded),
            (one(&groups, 3).0, 0, AtomKind::Synthetic, false)
        );
        assert_eq!((c.source.start(), c.source.end()), (2, 2));
        assert_eq!((c.target.start, c.target.end), (1, 2));
        related(&r, s.related, true, &[], &[2], (2, 2));
        let member = |g, rel| {
            let (id, a) = one(&groups, g);
            (id, a, rel)
        };
        boundary(
            model,
            &r,
            s.start,
            true,
            1,
            &[(1, 2)],
            &[1, 2],
            (1, 2),
            &[
                member(0, Correspondence::Exact),
                member(1, Correspondence::Hidden),
                member(3, Correspondence::Exact),
            ],
            Effects {
                hidden: true,
                ..Effects::default()
            },
        );
        boundary(
            model,
            &r,
            s.end,
            true,
            3,
            &[(2, 3)],
            &[2, 3],
            (2, 3),
            &[
                member(2, Correspondence::Hidden),
                member(3, Correspondence::Exact),
                member(4, Correspondence::Exact),
            ],
            Effects {
                hidden: true,
                ..Effects::default()
            },
        );
    });
}

#[test]
fn g3_delta_reference_residual_has_complete_definition_payload_and_main_model() {
    with_model("[r]: /u\n    abc\n", |model| {
        let ir = model.ir();
        assert_eq!(main_text(model), "abc");
        assert_eq!(ir.nodes().len(), 3);
        assert_eq!(ir.flows().len(), 1);
        let flow = ir.flows()[model.main_flow_id().index()];
        assert_eq!(flow.scope, FlowScope::Main);
        assert_eq!(flow.root.index(), 0);
        assert_eq!(flow.logical_utf16, 3);
        assert_eq!(spans(model, flow.domains), vec![(0, 16)]);
        for (i, (kind, parent, child, env)) in [
            (NodeKind::Document, None, Some(1), (0, 16)),
            (NodeKind::Paragraph, Some(0), Some(2), (0, 16)),
            (NodeKind::Text, Some(1), None, (12, 15)),
        ]
        .into_iter()
        .enumerate()
        {
            let n = ir.nodes()[i];
            assert_eq!(n.kind, kind);
            assert_eq!(n.parent.map(|v| v.index()), parent);
            assert_eq!(n.first_child.map(|v| v.index()), child);
            assert_eq!(n.next_sibling, None);
            assert_eq!(n.detail, None);
            assert_eq!(n.flow, model.main_flow_id());
            assert_eq!((n.logical.start(), n.logical.end()), (0, 3));
            assert_eq!((n.source_envelope.start(), n.source_envelope.end()), env);
        }
        assert!(ir.details().is_empty());
        assert!(ir.attributes().is_empty());
        assert!(ir.objects().is_empty());
        assert!(ir.references().is_empty());
        assert!(ir.alignments().is_empty());
        assert!(ir.proofs().is_empty());
        assert!(ir.stages().is_empty());
        assert!(ir.primitives().is_empty());
        assert_eq!(ir.definitions().len(), 1);
        assert_eq!(ir.payloads().len(), 2);
        assert_eq!(ir.atoms_len(), 6);
        let d = ir.definitions()[0];
        assert!(d.winner);
        assert_eq!(ir.text(d.key).unwrap(), "r");
        assert_eq!(d.title, None);
        assert_eq!(spans(model, d.syntax), vec![(0, 8)]);
        authority(
            model,
            d.claim,
            block(0, ClaimKind::ReferenceDefinition, (0, 16), (0, 8)),
            &[(0, 8)],
        );
        let mut op = model.budget().begin(CancellationToken::new());
        assert_ne!(d.label, d.destination);
        for (id, role, value, domain, host_role) in [
            (
                d.label,
                PayloadRole::ReferenceLabel,
                "r",
                (1, 2),
                HostRole::ReferenceLabel,
            ),
            (
                d.destination,
                PayloadRole::Destination,
                "/u",
                (5, 7),
                HostRole::Destination,
            ),
        ] {
            let p = ir.payloads()[id.index()];
            let PayloadOwner::Definition(owner) = p.owner else {
                panic!()
            };
            assert_eq!(owner.index(), 0);
            assert_eq!(p.role, role);
            assert_eq!(ir.text(p.value).unwrap(), value);
            assert_eq!(p.utf8_len, value.len() as u64);
            assert_eq!(spans(model, p.domains), vec![domain]);
            assert_eq!(p.atoms.len(), 1);
            let a = ir.atom_at(p.atoms.start(), &mut op).unwrap();
            assert_eq!(a.kind, AtomKind::Copy);
            assert_eq!(a.owner.index(), 0);
            assert_eq!(a.proof, None);
            assert_eq!(a.content, AtomContent::Empty);
            assert_eq!(spans(model, a.sources), vec![domain]);
            assert_eq!((a.target.start, a.target.end), (0, value.len() as u64));
            assert_eq!(
                &model.source().text()[domain.0 as usize..domain.1 as usize],
                value
            );
            authority(
                model,
                a.claim,
                Authority {
                    owner: 0,
                    kind: ClaimKind::Payload,
                    role: host_role,
                    phase: NativePhase::Metadata,
                    domain,
                    consumed: (0, value.len() as u64),
                },
                &[domain],
            );
        }
        let groups = partition(
            model,
            &[
                part(
                    AtomKind::Hidden,
                    (0, 8),
                    (0, 0),
                    block(1, ClaimKind::ReferenceDefinition, (0, 16), (0, 8)),
                ),
                part(
                    AtomKind::Hidden,
                    (8, 12),
                    (0, 0),
                    block(1, ClaimKind::Whitespace, (0, 16), (8, 12)),
                ),
                part(
                    AtomKind::Copy,
                    (12, 15),
                    (0, 3),
                    inline(2, (0, 16), (12, 15)),
                ),
                part(
                    AtomKind::Hidden,
                    (15, 16),
                    (3, 3),
                    inline(1, (0, 16), (15, 16)),
                ),
            ],
        );
        exact_queries(model, 13, 1, one(&groups, 2));
        hidden_queries(model, (9, 11), 10, 0, one(&groups, 1));
    });
}

#[test]
fn g3_delta_indent_and_quote_preserve_consumed_owner_partitions() {
    with_model("p\n |abc|\n |---|\n", |model| {
        assert_eq!(main_text(model), "p\nabc");
        assert_eq!(model.ir().nodes().len(), 7);
        let groups = partition(
            model,
            &[
                part(AtomKind::Copy, (0, 1), (0, 1), inline(2, (0, 2), (0, 1))),
                part(AtomKind::Hidden, (1, 2), (1, 1), inline(1, (0, 2), (1, 2))),
                part(
                    AtomKind::Hidden,
                    (2, 4),
                    (2, 2),
                    block(3, ClaimKind::Table, (2, 9), (0, 2)),
                ),
                part(AtomKind::Synthetic, (3, 3), (1, 2), block_separator(3, 3)),
                part(AtomKind::Copy, (4, 7), (2, 5), inline(6, (4, 7), (0, 3))),
                part(
                    AtomKind::Hidden,
                    (7, 9),
                    (5, 5),
                    block(3, ClaimKind::Table, (2, 9), (5, 7)),
                ),
                part(
                    AtomKind::Hidden,
                    (9, 10),
                    (5, 5),
                    block(3, ClaimKind::Whitespace, (0, 16), (9, 10)),
                ),
                part(
                    AtomKind::Hidden,
                    (10, 16),
                    (5, 5),
                    block(3, ClaimKind::Table, (10, 16), (0, 6)),
                ),
            ],
        );
        exact_queries(model, 5, 3, one(&groups, 4));
        // The delimiter indentation has its own BlockWhitespace authority.
        assert_eq!(one(&groups, 6).1.owner.index(), 3);
    });
    with_model("> p\n>  |abc|\n>  |---|\n", |model| {
        assert_eq!(main_text(model), "p\nabc");
        let ir = model.ir();
        assert_eq!(ir.nodes().len(), 8);
        assert_eq!(ir.nodes()[1].kind, NodeKind::BlockQuote);
        assert_eq!(ir.nodes()[2].kind, NodeKind::Paragraph);
        assert_eq!(ir.nodes()[4].kind, NodeKind::Table);
        assert_eq!(ir.nodes()[2].parent.unwrap().index(), 1);
        assert_eq!(ir.nodes()[4].parent.unwrap().index(), 1);
        assert_eq!(ir.nodes()[2].next_sibling.unwrap().index(), 4);
        let groups = partition(
            model,
            &[
                part(
                    AtomKind::Hidden,
                    (0, 2),
                    (0, 0),
                    block(1, ClaimKind::ContainerPrefix, (0, 22), (0, 2)),
                ),
                part(AtomKind::Copy, (2, 3), (0, 1), inline(3, (2, 4), (0, 1))),
                part(AtomKind::Hidden, (3, 4), (1, 1), inline(2, (2, 4), (1, 2))),
                // Accepted independent correction: L2, not the v2 prose typo L1.
                part(
                    AtomKind::Hidden,
                    (4, 6),
                    (2, 2),
                    block(1, ClaimKind::ContainerPrefix, (0, 22), (4, 6)),
                ),
                part(
                    AtomKind::Hidden,
                    (6, 8),
                    (2, 2),
                    block(4, ClaimKind::Table, (6, 13), (0, 2)),
                ),
                part(AtomKind::Synthetic, (7, 7), (1, 2), block_separator(4, 7)),
                part(AtomKind::Copy, (8, 11), (2, 5), inline(7, (8, 11), (0, 3))),
                part(
                    AtomKind::Hidden,
                    (11, 13),
                    (5, 5),
                    block(4, ClaimKind::Table, (6, 13), (5, 7)),
                ),
                part(
                    AtomKind::Hidden,
                    (13, 15),
                    (5, 5),
                    block(1, ClaimKind::ContainerPrefix, (0, 22), (13, 15)),
                ),
                part(
                    AtomKind::Hidden,
                    (15, 16),
                    (5, 5),
                    block(4, ClaimKind::Whitespace, (0, 22), (15, 16)),
                ),
                part(
                    AtomKind::Hidden,
                    (16, 22),
                    (5, 5),
                    block(4, ClaimKind::Table, (16, 22), (0, 6)),
                ),
            ],
        );
        exact_queries(model, 9, 3, one(&groups, 6));
        let (id, atom) = one(&groups, 3);
        for affinity in [Affinity::Before, Affinity::After] {
            let mut op = model.budget().begin(CancellationToken::new());
            let r = model
                .map_point(
                    &PointQuery {
                        model: model.identity(),
                        input: PointInput::Source {
                            target: TextTarget::Flow(model.main_flow_id()),
                            byte: SourceBoundary::new(5),
                        },
                        affinity,
                    },
                    &mut op,
                )
                .unwrap();
            assert_eq!(r.results().len(), 1);
            boundary(
                model,
                &r,
                r.results()[0],
                false,
                2,
                &[],
                &[2],
                (2, 2),
                &[(id, atom, Correspondence::Hidden)],
                Effects {
                    hidden: true,
                    ..Effects::default()
                },
            );
        }
    });
}

/// A control checks grammar/anchor only; it is not represented as a complete
/// model oracle. Complete parent and authority checks are the tests above.
fn table_control(text: &str, main: &str, anchor: u64, parent: NodeKind) {
    with_model(text, |model| {
        assert_eq!(main_text(model), main);
        let ir = model.ir();
        let tables: Vec<_> = ir
            .nodes()
            .iter()
            .enumerate()
            .filter(|(_, n)| n.kind == NodeKind::Table)
            .collect();
        assert_eq!(tables.len(), 1);
        let (index, table) = tables[0];
        assert_eq!(ir.nodes()[table.parent.unwrap().index()].kind, parent);
        let mut op = model.budget().begin(CancellationToken::new());
        let flow = ir.flows()[model.main_flow_id().index()];
        let mut separators = 0;
        for i in flow.atoms.start()..flow.atoms.start() + flow.atoms.len() {
            let a = ir.atom_at(i, &mut op).unwrap();
            if a.owner.index() == index && a.kind == AtomKind::Synthetic {
                let claim = ir.authority(a.claim, &mut op).unwrap();
                if claim.kind == ClaimKind::ProjectionSeparator(SyntheticKind::BlockLineFeed) {
                    assert_eq!(spans(model, a.sources), vec![(anchor, anchor)]);
                    separators += 1;
                }
            }
        }
        assert_eq!(separators, 1);
        assert!(ir.objects().is_empty());
    });
}

#[test]
fn g3_delta_split_retry_and_container_controls() {
    for (text, main, anchor, parent) in [
        ("p\nq\n|abc|\n|---|\n", "p q\nabc", 4, NodeKind::Document),
        ("> p\n> |abc|\n> |---|\n", "p\nabc", 6, NodeKind::BlockQuote),
        ("- p\n  |abc|\n  |---|\n", "p\nabc", 6, NodeKind::ListItem),
        ("p\r\n|abc|\r\n|---|\r\n", "p\nabc", 3, NodeKind::Document),
        ("p\n   |abc|\n   |---|\n", "p\nabc", 5, NodeKind::Document),
        (
            "p\n|---|---|\n|abc|\n|---|\n",
            "p |---|---|\nabc",
            12,
            NodeKind::Document,
        ),
        ("p\n\n|abc|\n|---|\n", "p\nabc", 3, NodeKind::Document),
        ("> p\n\n|abc|\n|---|\n", "p\nabc", 5, NodeKind::Document),
    ] {
        table_control(text, main, anchor, parent);
    }
    with_model("[r]: /u\n===\np\n|abc|\n|---|\n", |model| {
        assert_eq!(main_text(model), "=== p\nabc");
        assert_eq!(model.ir().definitions().len(), 1);
        let d = model.ir().definitions()[0];
        assert!(d.winner);
        assert_eq!(model.ir().text(d.key).unwrap(), "r");
        assert!(
            model
                .ir()
                .nodes()
                .iter()
                .all(|n| n.kind != NodeKind::Heading)
        );
    });
    table_control(
        "[r]: /u\n===\np\n|abc|\n|---|\n",
        "=== p\nabc",
        14,
        NodeKind::Document,
    );
}

#[test]
fn g3_delta_rejected_probe_keeps_original_paragraph_and_single_header_rules() {
    for (text, main) in [
        ("p\n\\|abc|\n|---|\n", "p |abc| |---|"),
        ("p\n&#124;abc|\n|---|\n", "p |abc| |---|"),
        ("p\nabc\n|---|\n", "p abc |---|"),
        ("p\nabc|\n|---|\n", "p abc| |---|"),
        ("p\n|abc|\n|--x|\n", "p |abc| |--x|"),
        ("p\n|abc|\n|---|---|\n", "p |abc| |---|---|"),
        ("abc\n---|---\n", "abc ---|---"),
        (
            "lorem ipsum\nA | B\n---|---\nfoo | bar\n",
            "lorem ipsum A | B ---|--- foo | bar",
        ),
    ] {
        with_model(text, |model| {
            assert_eq!(main_text(model), main);
            assert_eq!(
                model
                    .ir()
                    .nodes()
                    .iter()
                    .filter(|n| n.kind == NodeKind::Paragraph)
                    .count(),
                1
            );
            assert!(model.ir().nodes().iter().all(|n| n.kind != NodeKind::Table));
            assert!(model.ir().definitions().is_empty());
            assert!(model.ir().objects().is_empty());
        });
    }
    with_model("    |abc|\n    |---|\n", |model| {
        assert_eq!(main_text(model), "|abc|\n|---|\n");
        assert!(model.ir().nodes().iter().all(|n| n.kind != NodeKind::Table));
        assert_eq!(
            model
                .ir()
                .nodes()
                .iter()
                .filter(|n| n.kind == NodeKind::CodeBlock)
                .count(),
            1
        );
    });
    with_model("a | b\n---|---\n", |model| {
        assert_eq!(main_text(model), "a\tb");
        assert_eq!(
            model
                .ir()
                .nodes()
                .iter()
                .filter(|n| n.kind == NodeKind::Table)
                .count(),
            1
        );
        assert!(
            model
                .ir()
                .nodes()
                .iter()
                .all(|n| n.kind != NodeKind::Paragraph)
        );
    });
}

fn block_whitespace_sources(model: &AcceptedModel) -> Vec<(u64, u64)> {
    let ir = model.ir();
    let flow = ir.flows()[model.main_flow_id().index()];
    let mut op = model.budget().begin(CancellationToken::new());
    let mut result = Vec::new();
    for i in flow.atoms.start()..flow.atoms.start() + flow.atoms.len() {
        let a = ir.atom_at(i, &mut op).unwrap();
        let claim = ir.authority(a.claim, &mut op).unwrap();
        if claim.kind == ClaimKind::Whitespace
            && claim.phase == NativePhase::Block
            && ir.nodes()[a.owner.index()].kind == NodeKind::Paragraph
        {
            assert_eq!(a.kind, AtomKind::Hidden);
            assert_eq!(claim.owner, a.owner);
            assert_eq!(claim.host.owner, a.owner);
            assert_eq!(claim.host.role, HostRole::Block);
            result.extend(spans(model, a.sources));
        }
    }
    result.sort_unstable();
    result
}

#[test]
fn g3_delta_residual_first_line_whitespace_controls() {
    for (text, main, indent, parent) in [
        (
            "[r]: /u\n    abc\n    def\n",
            "abc def",
            (8, 12),
            NodeKind::Document,
        ),
        ("[r]: /u\n\tabc\n", "abc", (8, 9), NodeKind::Document),
        ("[r]: /u\r\n    abc\r\n", "abc", (9, 13), NodeKind::Document),
        (
            "> [r]: /u\n>     abc\n",
            "abc",
            (12, 16),
            NodeKind::BlockQuote,
        ),
        (
            "- [r]: /u\n      abc\n",
            "abc",
            (12, 16),
            NodeKind::ListItem,
        ),
        (
            "[linkme]: foo\n    - baz\n",
            "- baz",
            (14, 18),
            NodeKind::Document,
        ),
    ] {
        with_model(text, |model| {
            assert_eq!(main_text(model), main);
            assert_eq!(model.ir().definitions().len(), 1);
            assert!(model.ir().definitions()[0].winner);
            assert_eq!(block_whitespace_sources(model), vec![indent]);
            let paragraphs: Vec<_> = model
                .ir()
                .nodes()
                .iter()
                .filter(|n| n.kind == NodeKind::Paragraph)
                .collect();
            assert_eq!(paragraphs.len(), 1);
            assert_eq!(
                model.ir().nodes()[paragraphs[0].parent.unwrap().index()].kind,
                parent
            );
            assert!(
                model
                    .ir()
                    .nodes()
                    .iter()
                    .all(|n| n.kind != NodeKind::CodeBlock && n.kind != NodeKind::Table)
            );
        });
    }
    // Original pd.0137: the three-SP branch remains a List; four SP remains
    // prose following a definition. This is a grammar control, not a new HTML oracle.
    with_model(
        "[linkme-3]: a\n   - a\n\n[linkme-4]: a\n    - b\n\nList can interrupt the paragraph at the start of a link definition if it starts with three spaces, but not four.\n",
        |model| {
            assert_eq!(
                main_text(model),
                "a\n- b\nList can interrupt the paragraph at the start of a link definition if it starts with three spaces, but not four."
            );
            assert_eq!(model.ir().definitions().len(), 2);
            assert!(model.ir().definitions().iter().all(|d| d.winner));
            assert_eq!(
                model
                    .ir()
                    .nodes()
                    .iter()
                    .filter(|n| n.kind == NodeKind::List)
                    .count(),
                1
            );
        },
    );
}

#[test]
fn g3_delta_residual_does_not_trim_untriggered_or_title_content() {
    for (text, main, definitions, paragraphs) in [
        ("p\n    abc\n", "p abc", 0, 1),
        // A sole next-line "abc" could itself be a valid destination.
        ("[r]:\n    abc def\n", "[r]: abc def", 0, 1),
        ("[r]: /u\n", "", 1, 0),
        ("[r]: /u\n\nabc\n", "abc", 1, 1),
    ] {
        with_model(text, |model| {
            assert_eq!(main_text(model), main);
            assert_eq!(model.ir().definitions().len(), definitions);
            assert_eq!(
                model
                    .ir()
                    .nodes()
                    .iter()
                    .filter(|n| n.kind == NodeKind::Paragraph)
                    .count(),
                paragraphs
            );
            assert!(block_whitespace_sources(model).is_empty());
        });
    }
    for (text, title) in [
        (
            "[first]: https://example.com\n\"\n    -\n\"\n\n[first]\n",
            // TITLE-DELTA-v1 approved reference-only continuation indentation.
            // Ordinary P BlockWhitespace remains absent below.
            "\n-\n",
        ),
        (
            "> [first]: https://example.com\n> \"\n>     -\n> \"\n>\n> [first]\n",
            "\n-\n",
        ),
        (
            "[link]: destination \"\n          text          \"\n\n[link]\n",
            "\ntext          ",
        ),
        ("- [foo]: https://example.com\n'[foo]'\n[foo]\n", "[foo]"),
    ] {
        with_model(text, |model| {
            let ir = model.ir();
            assert_eq!(ir.definitions().len(), 1);
            let d = ir.definitions()[0];
            assert!(d.winner);
            let p = ir.payloads()[d.title.expect("preserved definition title").index()];
            assert_eq!(p.role, PayloadRole::Title);
            assert_eq!(ir.text(p.value).unwrap(), title);
            assert!(block_whitespace_sources(model).is_empty());
            for p in ir
                .payloads()
                .iter()
                .filter(|p| p.role == PayloadRole::Title)
            {
                assert_eq!(ir.text(p.value).unwrap(), title);
            }
        });
    }
    with_model("[foo]: https://example.com\n'[foo]'bar\n", |model| {
        assert_eq!(model.ir().definitions().len(), 1);
        assert_eq!(model.ir().definitions()[0].title, None);
        assert_eq!(main_text(model), "'foo'bar");
    });
}
