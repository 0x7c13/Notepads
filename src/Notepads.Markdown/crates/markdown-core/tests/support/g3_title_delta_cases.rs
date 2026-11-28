// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE file in the project root for license information.
// ---------------------------------------------------------------------------------------------

//! Independent pre-execution values from TITLE-DELTA-v1 and its clean review.
//! Complete title payload parents are checked before query identities are bound.
//! Other controls check their stated grammar/value scope, not an entire IR oracle.

use super::*;
use markdown_core::identity::{PayloadBoundary, PayloadSpan};
use markdown_core::ir::{
    AtomRow, HostRole, NativePhase, NodeId, PayloadId, PayloadOwner, PayloadRole, PrimitiveKind,
};
use markdown_core::mapping::{
    Effects, PointResult, Position, QueryResponse, Related, RelatedSpan,
};

fn with_profile(text: &str, profile: Profile, check: impl FnOnce(&AcceptedModel)) {
    let budget = SourceBudget::task_fixture();
    assert_eq!(
        budget.work_stats_collection(),
        WorkStatsCollection::Disabled
    );
    let mut op = budget.begin(CancellationToken::new());
    let key = SourceKey::new([0x74; 16], 1).unwrap();
    let source = SourceDocument::new(key, text.as_bytes(), &mut op).unwrap();
    let config = ParseConfig::new(profile);
    let model = AcceptedModel::parse(source.snapshot(), config, 1, &mut op)
        .unwrap_or_else(|e| panic!("{text:?}: {e:?}"));
    assert_eq!(model.identity().source(), key);
    assert_eq!(model.identity().config(), config);
    assert_eq!(model.identity().generation(), 1);
    check(&model);
    assert_eq!(op.first_fault(), None);
    drop(model);
    drop(source);
    drop(op);
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

#[derive(Clone, Copy)]
struct ExpectedAtom {
    kind: AtomKind,
    source: (u64, u64),
    target: (u64, u64),
    consumed: (u64, u64),
    primitive: Option<PrimitiveKind>,
}
fn a(kind: AtomKind, s: (u64, u64), t: (u64, u64), c: (u64, u64)) -> ExpectedAtom {
    ExpectedAtom {
        kind,
        source: s,
        target: t,
        consumed: c,
        primitive: None,
    }
}

fn occurrences(model: &AcceptedModel) -> Vec<(PayloadId, NodeId)> {
    let ir = model.ir();
    let root = ir.flows()[model.main_flow_id().index()].root;
    let mut result = Vec::new();
    for (index, d) in ir.definitions().iter().enumerate() {
        if let Some(id) = d.title {
            let PayloadOwner::Definition(owner) = ir.payloads()[id.index()].owner else {
                panic!()
            };
            assert_eq!(owner.index(), index);
            result.push((id, root));
        }
    }
    for r in ir.references().iter() {
        if let Some(id) = r.title {
            assert_eq!(ir.payloads()[id.index()].owner, PayloadOwner::Node(r.node));
            assert!(!result.iter().any(|(prior, _)| *prior == id));
            result.push((id, r.node));
        }
    }
    assert_eq!(
        result.len(),
        ir.payloads()
            .iter()
            .filter(|p| p.role == PayloadRole::Title)
            .count()
    );
    result
}

fn title_parent(
    model: &AcceptedModel,
    id: PayloadId,
    owner: NodeId,
    value: &str,
    domains: &[(u64, u64)],
    host_len: u64,
    expected: &[ExpectedAtom],
) -> Vec<(usize, AtomRow)> {
    let ir = model.ir();
    let p = ir.payloads()[id.index()];
    assert_eq!(p.role, PayloadRole::Title);
    assert_eq!(ir.text(p.value).unwrap(), value);
    assert_eq!(p.utf8_len, value.len() as u64);
    assert_eq!(spans(model, p.domains), domains);
    assert_eq!(p.atoms.len(), expected.len());
    let mut bound = Vec::new();
    let mut op = model.budget().begin(CancellationToken::new());
    for (offset, e) in expected.iter().enumerate() {
        let index = p.atoms.start() + offset;
        let atom = ir.atom_at(index, &mut op).unwrap();
        assert_eq!(atom.owner, owner);
        assert_eq!(atom.kind, e.kind);
        assert_eq!(spans(model, atom.sources), vec![e.source]);
        assert_eq!((atom.target.start, atom.target.end), e.target);
        let claim = ir.authority(atom.claim, &mut op).unwrap();
        assert_eq!(claim.owner, owner);
        assert_eq!(claim.host.owner, owner);
        assert_eq!(
            (claim.kind, claim.phase, claim.host.role),
            (ClaimKind::Payload, NativePhase::Metadata, HostRole::Title)
        );
        assert_eq!(spans(model, claim.host.domains), domains);
        assert_eq!(claim.host.utf8_len, host_len);
        assert_eq!((claim.consumed.start(), claim.consumed.end()), e.consumed);
        assert_eq!(spans(model, claim.sources), vec![e.source]);
        if let Some(kind) = e.primitive {
            let proof = ir.proofs()[atom.proof.expect("N01 witness").index()];
            assert_eq!(proof.atom.index(), index);
            assert!(proof.stages.is_empty());
            assert_eq!(proof.primitives.len(), 1);
            let primitive = ir.primitives()[proof.primitives.start()];
            assert_eq!(primitive.kind, kind);
            assert_eq!((primitive.source.start(), primitive.source.end()), e.source);
            assert_eq!(ir.text(proof.value).unwrap(), "\n");
            let AtomContent::Pool(text) = atom.content else {
                panic!()
            };
            assert_eq!(ir.text(text).unwrap(), "\n");
        } else {
            assert_eq!(atom.proof, None);
            assert_eq!(atom.content, AtomContent::Empty);
            if atom.kind == AtomKind::Copy {
                assert_eq!(
                    &model.source().text()[e.source.0 as usize..e.source.1 as usize],
                    &value[e.target.0 as usize..e.target.1 as usize]
                );
            }
        }
        bound.push((index, atom));
    }
    assert_eq!(op.first_fault(), None);
    bound
}

fn related<T: Copy>(
    r: &QueryResponse<T>,
    v: Related,
    reverse: bool,
    ranges: &[(u64, u64)],
    anchors: &[u64],
    envelope: (u64, u64),
) {
    let intervals: Vec<_> = r.spans()[v.spans.start()..v.spans.start() + v.spans.len()]
        .iter()
        .map(|s| {
            assert!(if reverse {
                matches!(s, RelatedSpan::Source(_))
            } else {
                matches!(s, RelatedSpan::Payload(_))
            });
            (s.start(), s.end())
        })
        .collect();
    assert_eq!(intervals, ranges);
    let points: Vec<_> = r.anchors()[v.anchors.start()..v.anchors.start() + v.anchors.len()]
        .iter()
        .map(|p| {
            assert!(if reverse {
                matches!(p, Position::Source(_))
            } else {
                matches!(p, Position::Payload(_))
            });
            p.value()
        })
        .collect();
    assert_eq!(points, anchors);
    let e = v.envelope.unwrap();
    assert!(if reverse {
        matches!(e, RelatedSpan::Source(_))
    } else {
        matches!(e, RelatedSpan::Payload(_))
    });
    assert_eq!((e.start(), e.end()), envelope);
}

#[allow(clippy::too_many_arguments)]
fn point<T: Copy>(
    model: &AcceptedModel,
    id: PayloadId,
    r: &QueryResponse<T>,
    p: PointResult,
    reverse: bool,
    chosen: u64,
    ranges: &[(u64, u64)],
    anchors: &[u64],
    envelope: (u64, u64),
    expected: &[(usize, AtomRow, Correspondence)],
    hidden: bool,
) {
    assert_eq!(p.model, model.identity());
    assert_eq!(p.target, TextTarget::Payload(id));
    assert_eq!(
        p.direction,
        if reverse {
            Direction::TargetToSource
        } else {
            Direction::SourceToTarget
        }
    );
    assert_eq!(
        p.chosen,
        if reverse {
            Position::Source(SourceBoundary::new(chosen))
        } else {
            Position::Payload(PayloadBoundary::new(chosen))
        }
    );
    assert_eq!(
        p.effects,
        Effects {
            hidden,
            ..Effects::default()
        }
    );
    assert_eq!(
        p.correspondence,
        if hidden {
            Correspondence::Hidden
        } else {
            Correspondence::Exact
        }
    );
    related(r, p.related, reverse, ranges, anchors, envelope);
    let mut expected = expected.to_vec();
    expected.sort_by_key(|x| x.0);
    let actual =
        &r.contributors()[p.contributors.start()..p.contributors.start() + p.contributors.len()];
    assert_eq!(actual.len(), expected.len());
    for (v, (index, atom, relation)) in actual.iter().zip(expected) {
        assert_eq!(v.atom.index(), index);
        assert_eq!(v.source_part, 0);
        assert_eq!(v.kind, atom.kind);
        assert_eq!(v.source, model.ir().sources()[atom.sources.start()]);
        assert_eq!(v.target, atom.target);
        assert_eq!(v.relation, relation);
    }
}

fn queries(model: &AcceptedModel, id: PayloadId, bound: &[(usize, AtomRow)]) {
    for (source, target, index, hidden) in [(12, 2, 2, true), (16, 3, 3, false)] {
        for affinity in [Affinity::Before, Affinity::After] {
            let mut op = model.budget().begin(CancellationToken::new());
            let r = model
                .map_point(
                    &PointQuery {
                        model: model.identity(),
                        input: PointInput::Source {
                            target: TextTarget::Payload(id),
                            byte: SourceBoundary::new(source),
                        },
                        affinity,
                    },
                    &mut op,
                )
                .unwrap();
            let (i, a) = bound[index];
            assert_eq!(r.results().len(), 1);
            point(
                model,
                id,
                &r,
                r.results()[0],
                false,
                target,
                &[],
                &[target],
                (target, target),
                &[(
                    i,
                    a,
                    if hidden {
                        Correspondence::Hidden
                    } else {
                        Correspondence::Exact
                    },
                )],
                hidden,
            );
        }
    }
    for affinity in [Affinity::Before, Affinity::After] {
        let mut op = model.budget().begin(CancellationToken::new());
        let r = model
            .map_point(
                &PointQuery {
                    model: model.identity(),
                    input: PointInput::Payload {
                        payload: id,
                        utf8: PayloadBoundary::new(3),
                    },
                    affinity,
                },
                &mut op,
            )
            .unwrap();
        let (i, a) = bound[3];
        assert_eq!(r.results().len(), 1);
        point(
            model,
            id,
            &r,
            r.results()[0],
            true,
            16,
            &[],
            &[16],
            (16, 16),
            &[(i, a, Correspondence::Exact)],
            false,
        );
        let r = model
            .map_point(
                &PointQuery {
                    model: model.identity(),
                    input: PointInput::Payload {
                        payload: id,
                        utf8: PayloadBoundary::new(2),
                    },
                    affinity,
                },
                &mut op,
            )
            .unwrap();
        let m = |at, rel| {
            let (i, a) = bound[at];
            (i, a, rel)
        };
        point(
            model,
            id,
            &r,
            r.results()[0],
            true,
            if affinity == Affinity::Before { 11 } else { 15 },
            &[(11, 15)],
            &[11, 15],
            (11, 15),
            &[
                m(1, Correspondence::Exact),
                m(2, Correspondence::Hidden),
                m(3, Correspondence::Exact),
            ],
            true,
        );
    }
    for (reverse, hidden) in [(false, false), (true, false), (false, true)] {
        let (source, target, index) = if hidden {
            ((12, 14), (2, 2), 2)
        } else {
            ((16, 17), (3, 4), 3)
        };
        let mut op = model.budget().begin(CancellationToken::new());
        let input = if reverse {
            RangeInput::Payload {
                payload: id,
                span: PayloadSpan::new(target.0, target.1).unwrap(),
            }
        } else {
            RangeInput::Source {
                target: TextTarget::Payload(id),
                span: SourceSpan::new(source.0, source.1).unwrap(),
            }
        };
        let r = model
            .map_range(
                &RangeQuery {
                    model: model.identity(),
                    input,
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
        assert_eq!(s.target, TextTarget::Payload(id));
        assert_eq!(
            s.direction,
            if reverse {
                Direction::TargetToSource
            } else {
                Direction::SourceToTarget
            }
        );
        assert_eq!(
            s.effects,
            Effects {
                hidden,
                ..Effects::default()
            }
        );
        assert!(!s.expanded);
        assert_eq!(s.contributions.len(), 1);
        let c = r.contributions()[s.contributions.start()];
        let (i, a) = bound[index];
        assert_eq!(c.atom.index(), i);
        assert_eq!(c.kind, a.kind);
        assert_eq!(c.source_part, 0);
        assert!(!c.expanded);
        assert_eq!((c.source.start(), c.source.end()), source);
        assert_eq!((c.target.start, c.target.end), target);
        let other = if reverse { source } else { target };
        if hidden {
            related(&r, s.related, false, &[], &[2], (2, 2));
        } else {
            related(&r, s.related, reverse, &[other], &[], other);
        }
        for (p, chosen) in [(s.start, other.0), (s.end, other.1)] {
            point(
                model,
                id,
                &r,
                p,
                reverse,
                chosen,
                &[],
                &[chosen],
                (chosen, chosen),
                &[(
                    i,
                    a,
                    if hidden {
                        Correspondence::Hidden
                    } else {
                        Correspondence::Exact
                    },
                )],
                hidden,
            );
        }
    }
}

const SMALL: &str = "[r]: /u \"a\n    bcd  \"\n\n[r]\n";
fn small_atoms() -> [ExpectedAtom; 5] {
    [
        a(AtomKind::Hidden, (8, 9), (0, 0), (0, 1)),
        a(AtomKind::Copy, (9, 11), (0, 2), (1, 3)),
        a(AtomKind::Hidden, (11, 15), (2, 2), (3, 7)),
        a(AtomKind::Copy, (15, 20), (2, 7), (7, 12)),
        a(AtomKind::Hidden, (20, 21), (7, 7), (12, 13)),
    ]
}

#[test]
fn reference_title_complete_payload_parents_and_typed_queries_c0_g3() {
    for profile in [Profile::C0, Profile::G3] {
        with_profile(SMALL, profile, |model| {
            let ir = model.ir();
            assert_eq!(main_text(model), "r");
            assert_eq!(ir.nodes().len(), 4);
            for (n, kind) in ir.nodes().iter().zip([
                NodeKind::Document,
                NodeKind::Paragraph,
                NodeKind::Link,
                NodeKind::Text,
            ]) {
                assert_eq!(n.kind, kind);
            }
            assert_eq!(ir.definitions().len(), 1);
            assert_eq!(ir.references().len(), 1);
            assert_eq!(ir.payloads().len(), 5);
            assert_eq!(ir.flows().len(), 1);
            assert!(ir.objects().is_empty());
            let d = ir.definitions()[0];
            assert!(d.winner);
            assert_eq!(ir.text(d.key).unwrap(), "r");
            assert_eq!(spans(model, d.syntax), vec![(0, 22)]);
            let r = ir.references()[0];
            assert_eq!(r.winner.index(), 0);
            assert_eq!(r.node.index(), 2);
            assert_eq!(spans(model, r.syntax), vec![(23, 26)]);
            let DetailRow::Link {
                title, reference, ..
            } = ir.details()[ir.nodes()[2].detail.unwrap().index()]
            else {
                panic!()
            };
            assert_eq!(title, r.title);
            assert_eq!(reference.unwrap().index(), 0);
            let titles = occurrences(model);
            assert_eq!(titles.len(), 2);
            for (id, owner) in titles {
                let parent =
                    title_parent(model, id, owner, "a\nbcd  ", &[(8, 21)], 13, &small_atoms());
                queries(model, id, &parent);
            }
        });
    }
}

#[test]
fn reference_title_original_h_geometry_and_quote_holes() {
    for (text, domains, value, expected) in [
        (
            "[first]: https://example.com\n\"\n    -\n\"\n\n[first]\n",
            vec![(29, 38)],
            "\n-\n",
            vec![
                a(AtomKind::Hidden, (29, 30), (0, 0), (0, 1)),
                a(AtomKind::Copy, (30, 31), (0, 1), (1, 2)),
                a(AtomKind::Hidden, (31, 35), (1, 1), (2, 6)),
                a(AtomKind::Copy, (35, 37), (1, 3), (6, 8)),
                a(AtomKind::Hidden, (37, 38), (3, 3), (8, 9)),
            ],
        ),
        (
            "> [first]: https://example.com\n> \"\n>     -\n> \"\n>\n> [first]\n",
            vec![(33, 35), (37, 43), (45, 46)],
            "\n-\n",
            vec![
                a(AtomKind::Hidden, (33, 34), (0, 0), (0, 1)),
                a(AtomKind::Copy, (34, 35), (0, 1), (1, 2)),
                a(AtomKind::Hidden, (37, 41), (1, 1), (2, 6)),
                a(AtomKind::Copy, (41, 43), (1, 3), (6, 8)),
                a(AtomKind::Hidden, (45, 46), (3, 3), (8, 9)),
            ],
        ),
        (
            "[link]: destination \"\n          text          \"\n\n[link]\n",
            vec![(20, 47)],
            "\ntext          ",
            vec![
                a(AtomKind::Hidden, (20, 21), (0, 0), (0, 1)),
                a(AtomKind::Copy, (21, 22), (0, 1), (1, 2)),
                a(AtomKind::Hidden, (22, 32), (1, 1), (2, 12)),
                a(AtomKind::Copy, (32, 46), (1, 15), (12, 26)),
                a(AtomKind::Hidden, (46, 47), (15, 15), (26, 27)),
            ],
        ),
    ] {
        with_profile(text, Profile::G3, |model| {
            let titles = occurrences(model);
            assert_eq!(titles.len(), 2);
            for (id, owner) in titles {
                title_parent(
                    model,
                    id,
                    owner,
                    value,
                    &domains,
                    domains.iter().map(|(s, e)| e - s).sum(),
                    &expected,
                );
                if domains.len() > 1 {
                    for (start, end) in [(35, 37), (43, 45), (34, 38)] {
                        let mut op = model.budget().begin(CancellationToken::new());
                        assert!(matches!(
                            model.map_range(
                                &RangeQuery {
                                    model: model.identity(),
                                    input: RangeInput::Source {
                                        target: TextTarget::Payload(id),
                                        span: SourceSpan::new(start, end).unwrap()
                                    },
                                    empty_affinity: Affinity::Before
                                },
                                &mut op
                            ),
                            Err(markdown_core::error::CoreError::OutsideScope)
                        ));
                    }
                }
            }
        });
    }
}

#[test]
fn reference_title_multiple_runs_keep_old_stage_coordinates() {
    with_profile("[r]: /u \"a\n  b\n\t c \"\n\n[r]\n", Profile::G3, |model| {
        for (id, owner) in occurrences(model) {
            title_parent(
                model,
                id,
                owner,
                "a\nb\nc ",
                &[(8, 20)],
                12,
                &[
                    a(AtomKind::Hidden, (8, 9), (0, 0), (0, 1)),
                    a(AtomKind::Copy, (9, 11), (0, 2), (1, 3)),
                    a(AtomKind::Hidden, (11, 13), (2, 2), (3, 5)),
                    a(AtomKind::Copy, (13, 15), (2, 4), (5, 7)),
                    a(AtomKind::Hidden, (15, 17), (4, 4), (7, 9)),
                    a(AtomKind::Copy, (17, 19), (4, 6), (9, 11)),
                    a(AtomKind::Hidden, (19, 20), (6, 6), (11, 12)),
                ],
            );
        }
    });
    for (open, close) in [("\"", "\""), ("'", "'"), ("(", ")")] {
        let input = format!("[r]: /u {open}  a\n \tb  {close}\n\n[r]\n");
        with_profile(&input, Profile::G3, |model| {
            for (id, owner) in occurrences(model) {
                title_parent(
                    model,
                    id,
                    owner,
                    "  a\nb  ",
                    &[(8, 19)],
                    11,
                    &[
                        a(AtomKind::Hidden, (8, 9), (0, 0), (0, 1)),
                        a(AtomKind::Copy, (9, 13), (0, 4), (1, 5)),
                        a(AtomKind::Hidden, (13, 15), (4, 4), (5, 7)),
                        a(AtomKind::Copy, (15, 18), (4, 7), (7, 10)),
                        a(AtomKind::Hidden, (18, 19), (7, 7), (10, 11)),
                    ],
                );
            }
        });
    }
}

#[test]
fn reference_title_normalized_line_endings_keep_n01_separate_from_indent() {
    for (ending, kind) in [("\r\n", PrimitiveKind::CrLf), ("\r", PrimitiveKind::Cr)] {
        let input = SMALL.replace('\n', ending);
        with_profile(&input, Profile::G3, |model| {
            let wide = ending.len() == 2;
            assert_eq!(input.len(), if wide { 31 } else { 27 });
            let end = if wide { 22 } else { 21 };
            let after = if wide { 12 } else { 11 };
            let mut lf = a(AtomKind::Transform, (10, after), (1, 2), (2, 3));
            lf.primitive = Some(kind);
            let expected = [
                a(AtomKind::Hidden, (8, 9), (0, 0), (0, 1)),
                a(AtomKind::Copy, (9, 10), (0, 1), (1, 2)),
                lf,
                a(AtomKind::Hidden, (after, after + 4), (2, 2), (3, 7)),
                a(AtomKind::Copy, (after + 4, end - 1), (2, 7), (7, 12)),
                a(AtomKind::Hidden, (end - 1, end), (7, 7), (12, 13)),
            ];
            for (id, owner) in occurrences(model) {
                title_parent(model, id, owner, "a\nbcd  ", &[(8, end)], 13, &expected);
            }
        });
    }
}

#[test]
fn reference_title_entity_and_escape_order_and_inline_isolation() {
    for (raw, value, hidden_count) in [
        ("&#10;    x", "\n    x", 2),
        ("\n&#32; x", "\n  x", 2),
        ("\n\u{a0}x", "\n\u{a0}x", 2),
        ("\n \t&bsol;&ast;", "\n*", 3),
    ] {
        let input = format!("[r]: /u \"{raw}\"\n\n[r]\n");
        with_profile(&input, Profile::G3, |model| {
            for (id, _) in occurrences(model) {
                let ir = model.ir();
                let p = ir.payloads()[id.index()];
                assert_eq!(ir.text(p.value).unwrap(), value);
                let mut op = model.budget().begin(CancellationToken::new());
                let mut hidden = 0;
                for i in p.atoms.start()..p.atoms.start() + p.atoms.len() {
                    hidden += usize::from(ir.atom_at(i, &mut op).unwrap().kind == AtomKind::Hidden);
                }
                assert_eq!(hidden, hidden_count);
            }
        });
    }
    with_profile("[r](/u \"a\n    bcd  \")\n", Profile::G3, |model| {
        assert!(model.ir().definitions().is_empty());
        assert!(model.ir().references().is_empty());
        let values: Vec<_> = model
            .ir()
            .payloads()
            .iter()
            .filter(|p| p.role == PayloadRole::Title)
            .map(|p| model.ir().text(p.value).unwrap().to_owned())
            .collect();
        assert_eq!(values, vec!["a\n    bcd  "]);
    });
}

#[test]
fn reference_title_failure_empty_first_winner_and_image_occurrences() {
    for input in [
        "[foo]: https://example.com\n'[foo]'bar\n",
        "[r]: /u\n\"a\n\nb\"\n\n[r]\n",
    ] {
        with_profile(input, Profile::G3, |model| {
            assert_eq!(model.ir().definitions().len(), 1);
            assert_eq!(model.ir().definitions()[0].title, None);
            assert!(
                model
                    .ir()
                    .payloads()
                    .iter()
                    .all(|p| p.role != PayloadRole::Title)
            );
            if input.starts_with("[foo]") {
                assert_eq!(main_text(model), "'foo'bar");
            }
        });
    }
    for (input, title) in [
        ("[r]: /u \"\"\n\n[r]\n", Some("")),
        ("[r]: /u\n\n[r]\n", None),
        (
            "- [foo]: https://example.com\n'[foo]'\n[foo]\n",
            Some("[foo]"),
        ),
    ] {
        with_profile(input, Profile::G3, |model| {
            let ir = model.ir();
            assert_eq!(ir.definitions().len(), 1);
            assert_eq!(ir.definitions()[0].title.is_some(), title.is_some());
            if let Some(value) = title {
                let entries = occurrences(model);
                assert_eq!(entries.len(), 2);
                for (id, _) in entries {
                    assert_eq!(ir.text(ir.payloads()[id.index()].value).unwrap(), value);
                }
            } else {
                assert!(ir.payloads().iter().all(|p| p.role != PayloadRole::Title));
            }
        });
    }
    with_profile(
        "[r]: /u \"a\n    bcd  \"\n\n[r]: /v \"other\"\n\n[r]\n",
        Profile::G3,
        |model| {
            let ir = model.ir();
            assert_eq!(ir.definitions().len(), 2);
            assert!(ir.definitions()[0].winner);
            assert!(!ir.definitions()[1].winner);
            assert_eq!(ir.references().len(), 1);
            let r = ir.references()[0];
            assert_eq!(r.winner.index(), 0);
            assert_eq!(
                ir.text(ir.payloads()[r.title.unwrap().index()].value)
                    .unwrap(),
                "a\nbcd  "
            );
            assert_eq!(
                ir.text(ir.payloads()[ir.definitions()[1].title.unwrap().index()].value)
                    .unwrap(),
                "other"
            );
        },
    );
    let image = format!("{SMALL}![r]\n");
    with_profile(&image, Profile::G3, |model| {
        let entries = occurrences(model);
        assert_eq!(entries.len(), 3);
        assert_eq!(model.ir().objects().len(), 1);
        assert_eq!(model.ir().flows().len(), 2);
        assert_eq!(
            model.ir().nodes()[entries[1].1.index()].kind,
            NodeKind::Link
        );
        assert_eq!(
            model.ir().nodes()[entries[2].1.index()].kind,
            NodeKind::Image
        );
        for (id, owner) in entries {
            title_parent(model, id, owner, "a\nbcd  ", &[(8, 21)], 13, &small_atoms());
        }
    });
}
