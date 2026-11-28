// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE file in the project root for license information.
// ---------------------------------------------------------------------------------------------

//! Independent finite range expectations written before execution in
//! convergence-v126/RANGE-ASSERTIONS.md. These are actual parser models, not
//! hand-built IR or responses copied from another budget mode.

use markdown_core::config::{ParseConfig, Profile};
use markdown_core::error::CoreError;
use markdown_core::identity::{
    LogicalBoundary, LogicalSpan, PayloadBoundary, PayloadSpan, SourceBoundary, SourceKey,
    SourceSpan,
};
use markdown_core::ir::{
    AtomContent, AtomKind, DetailRow, FlowScope, ObjectRow, Rows, TextTarget,
};
use markdown_core::mapping::{
    Affinity, Correspondence, Direction, Position, RangeInput, RangeQuery, RangeResponse,
    RangeResult, RelatedSpan,
};
use markdown_core::model::AcceptedModel;
use markdown_core::policy::{CancellationToken, SourceBudget, WorkStatsCollection};
use markdown_core::source::SourceDocument;

fn with_model(text: &str, check: impl FnOnce(&AcceptedModel)) {
    let budget = SourceBudget::task_fixture();
    assert_eq!(
        budget.work_stats_collection(),
        WorkStatsCollection::Disabled
    );
    // Keep the real clock and original policy, including during queries.
    let mut op = budget.begin(CancellationToken::new());
    let source = SourceDocument::new(
        SourceKey::new([0x62; 16], 1).unwrap(),
        text.as_bytes(),
        &mut op,
    )
    .unwrap();
    let model =
        AcceptedModel::parse(source.snapshot(), ParseConfig::new(Profile::C0), 1, &mut op).unwrap();
    check(&model);
    assert_eq!(op.first_fault(), None);
    drop(model);
    drop(source);
    drop(op);
    let usage = budget.usage();
    assert_eq!(usage.source_capacity, 0);
    assert_eq!(usage.auxiliary_capacity, 0);
    assert_eq!(usage.source_owners, 0);
}

fn query(model: &AcceptedModel, input: RangeInput) -> Result<RangeResponse, CoreError> {
    let mut op = model.budget().begin(CancellationToken::new());
    let result = model.map_range(
        &RangeQuery {
            model: model.identity(),
            input,
            empty_affinity: Affinity::Before,
        },
        &mut op,
    );
    if result.is_ok() {
        assert_eq!(op.first_fault(), None);
    }
    if let Err(error) = result.as_ref() {
        assert_eq!(op.first_fault(), Some(*error));
    }
    result
}

#[derive(Clone, Copy)]
struct ParentAtom {
    kind: AtomKind,
    source: (u64, u64),
    target: (u64, u64),
}

struct Expected {
    target: TextTarget,
    atoms: Vec<ParentAtom>,
    content_atom: usize,
    boundary_atoms: (Vec<usize>, Vec<usize>),
    direction: Direction,
    kind: AtomKind,
    source: (u64, u64),
    target_span: (u64, u64),
    chosen: (u64, u64),
    relations: (Correspondence, Correspondence),
    envelopes: ((u64, u64), (u64, u64)),
    related: RelatedSpan,
    expanded: bool,
}

fn selected<T: Copy>(rows: Rows<T>, values: &[T]) -> &[T] {
    values
        .get(rows.start()..rows.start().checked_add(rows.len()).unwrap())
        .unwrap()
}

fn single(model: &AcceptedModel, response: &RangeResponse, expected: Expected) {
    // Resolve identities from the independently specified complete parent atom
    // sequence in this target; never copy expected IDs from the query response.
    let ir = model.ir();
    let rows = match expected.target {
        TextTarget::Flow(flow) => ir.flows()[flow.index()].atoms,
        TextTarget::Payload(payload) => ir.payloads()[payload.index()].atoms,
    };
    assert_eq!(rows.len(), expected.atoms.len());
    let mut op = model.budget().begin(CancellationToken::new());
    for (index, parent) in expected.atoms.iter().enumerate() {
        let atom = ir.atom_at(rows.start() + index, &mut op).unwrap();
        assert_eq!(atom.kind, parent.kind);
        assert_eq!((atom.target.start, atom.target.end), parent.target);
        assert_eq!(atom.sources.len(), 1);
        assert_eq!(
            ir.sources()[atom.sources.start()],
            SourceSpan::new(parent.source.0, parent.source.1).unwrap()
        );
    }
    let atom_base = rows.start();
    let [RangeResult::Selection(rows)] = response.results() else {
        panic!("expected exactly one Selection");
    };
    let [selection] = selected(*rows, response.selections()) else {
        panic!("expected exactly one selected range");
    };
    assert_eq!(selection.model, model.identity());
    assert_eq!(selection.target, expected.target);
    assert_eq!(selection.direction, expected.direction);
    for (boundary, chosen, relation, envelope, contributor_indices) in [
        (
            selection.start,
            expected.chosen.0,
            expected.relations.0,
            expected.envelopes.0,
            &expected.boundary_atoms.0,
        ),
        (
            selection.end,
            expected.chosen.1,
            expected.relations.1,
            expected.envelopes.1,
            &expected.boundary_atoms.1,
        ),
    ] {
        assert_eq!(boundary.model, model.identity());
        assert_eq!(boundary.target, expected.target);
        assert_eq!(boundary.direction, expected.direction);
        let (position, related) = if expected.direction == Direction::TargetToSource {
            (
                Position::Source(SourceBoundary::new(chosen)),
                RelatedSpan::Source(SourceSpan::new(envelope.0, envelope.1).unwrap()),
            )
        } else {
            match expected.target {
                TextTarget::Flow(_) => (
                    Position::Flow(LogicalBoundary::new(chosen)),
                    RelatedSpan::Flow(LogicalSpan::new(envelope.0, envelope.1).unwrap()),
                ),
                TextTarget::Payload(_) => (
                    Position::Payload(PayloadBoundary::new(chosen)),
                    RelatedSpan::Payload(PayloadSpan::new(envelope.0, envelope.1).unwrap()),
                ),
            }
        };
        assert_eq!(boundary.chosen, position);
        assert_eq!(boundary.correspondence, relation);
        assert_eq!(boundary.related.envelope, Some(related));
        let spans = selected(boundary.related.spans, response.spans());
        let anchors = selected(boundary.related.anchors, response.anchors());
        if envelope.0 == envelope.1 {
            assert!(spans.is_empty());
            assert_eq!(anchors, [position]);
        } else {
            assert_eq!(spans, [related]);
            assert!(anchors.is_empty());
        }
        assert_eq!(boundary.effects.atomic, relation == Correspondence::Atomic);
        assert_eq!(boundary.effects.object, relation == Correspondence::Object);
        assert!(!boundary.effects.hidden);
        assert!(!boundary.effects.synthetic);
        let contributors = selected(boundary.contributors, response.contributors());
        assert_eq!(contributors.len(), contributor_indices.len());
        for (contributor, &index) in contributors.iter().zip(contributor_indices) {
            let parent = expected.atoms[index];
            assert_eq!(contributor.atom.index(), atom_base + index);
            assert_eq!(contributor.source_part, 0);
            assert_eq!(contributor.kind, parent.kind);
            assert_eq!(
                contributor.source,
                SourceSpan::new(parent.source.0, parent.source.1).unwrap()
            );
            assert_eq!(
                (contributor.target.start, contributor.target.end),
                parent.target
            );
            assert_eq!(contributor.relation, relation);
        }
    }
    assert_eq!(selection.expanded, expected.expanded);
    assert_eq!(
        selection.effects.atomic,
        expected.kind == AtomKind::Transform
    );
    assert_eq!(selection.effects.object, expected.kind == AtomKind::Object);
    assert!(!selection.effects.hidden);
    assert!(!selection.effects.synthetic);
    let [part] = selected(selection.contributions, response.contributions()) else {
        panic!("expected one complete independent contribution");
    };
    assert_eq!(part.atom.index(), atom_base + expected.content_atom);
    assert_eq!(part.kind, expected.kind);
    assert_eq!(
        part.source,
        SourceSpan::new(expected.source.0, expected.source.1).unwrap()
    );
    assert_eq!((part.target.start, part.target.end), expected.target_span);
    assert_eq!(part.source_part, 0);
    assert_eq!(part.expanded, expected.expanded);
    assert_eq!(
        selected(selection.related.spans, response.spans()),
        [expected.related]
    );
    assert_eq!(selection.related.envelope, Some(expected.related));
    assert!(selected(selection.related.anchors, response.anchors()).is_empty());
}

#[test]
fn two_scalar_entity_partial_range_keeps_its_minimal_atomic_source() {
    with_model("&NotEqualTilde;", |model| {
        let flow = model.main_flow_id();
        let ir = model.ir();
        assert_eq!(model.source().text().len(), 15);
        assert_eq!(ir.flows().len(), 1);
        assert_eq!(ir.flows()[flow.index()].logical_utf16, 2);
        let atoms = ir.flows()[flow.index()].atoms;
        assert_eq!(atoms.len(), 1);
        let mut op = model.budget().begin(CancellationToken::new());
        let atom = ir.atom_at(atoms.start(), &mut op).unwrap();
        let AtomContent::Pool(text) = atom.content else {
            panic!()
        };
        assert_eq!(ir.text(text).unwrap(), "≂\u{0338}");
        for from_source in [false, true] {
            let input = if from_source {
                RangeInput::Source {
                    target: TextTarget::Flow(flow),
                    span: SourceSpan::new(1, 2).unwrap(),
                }
            } else {
                RangeInput::Flow {
                    flow,
                    span: LogicalSpan::new(0, 1).unwrap(),
                }
            };
            let response = query(model, input).unwrap();
            single(
                model,
                &response,
                Expected {
                    target: TextTarget::Flow(flow),
                    atoms: vec![ParentAtom {
                        kind: AtomKind::Transform,
                        source: (0, 15),
                        target: (0, 2),
                    }],
                    content_atom: 0,
                    boundary_atoms: (vec![0], vec![0]),
                    direction: if from_source {
                        Direction::SourceToTarget
                    } else {
                        Direction::TargetToSource
                    },
                    kind: AtomKind::Transform,
                    source: if from_source { (1, 2) } else { (0, 15) },
                    target_span: if from_source { (0, 2) } else { (0, 1) },
                    chosen: if from_source { (0, 2) } else { (0, 15) },
                    relations: if from_source {
                        (Correspondence::Atomic, Correspondence::Atomic)
                    } else {
                        (Correspondence::Exact, Correspondence::Atomic)
                    },
                    envelopes: if from_source {
                        ((0, 2), (0, 2))
                    } else {
                        ((0, 0), (0, 15))
                    },
                    related: if from_source {
                        RelatedSpan::Flow(LogicalSpan::new(0, 2).unwrap())
                    } else {
                        RelatedSpan::Source(SourceSpan::new(0, 15).unwrap())
                    },
                    expanded: true,
                },
            );
        }
    });
}

#[test]
fn image_main_object_and_alt_range_keep_separate_source_permissions() {
    with_model("![a](u)", |model| {
        let ir = model.ir();
        assert_eq!(ir.objects().len(), 1);
        assert_eq!(ir.flows().len(), 2);
        let ObjectRow::Image { alt, .. } = ir.objects()[0] else {
            panic!()
        };
        assert!(matches!(
            ir.flows()[alt.index()].scope,
            FlowScope::ImageAlt { .. }
        ));
        for (flow, kind, source) in [
            (model.main_flow_id(), AtomKind::Object, (0, 7)),
            (alt, AtomKind::Copy, (2, 3)),
        ] {
            assert_eq!(ir.flows()[flow.index()].logical_utf16, 1);
            let response = query(
                model,
                RangeInput::Flow {
                    flow,
                    span: LogicalSpan::new(0, 1).unwrap(),
                },
            )
            .unwrap();
            single(
                model,
                &response,
                Expected {
                    target: TextTarget::Flow(flow),
                    atoms: vec![ParentAtom {
                        kind,
                        source,
                        target: (0, 1),
                    }],
                    content_atom: 0,
                    boundary_atoms: (vec![0], vec![0]),
                    direction: Direction::TargetToSource,
                    kind,
                    source,
                    target_span: (0, 1),
                    chosen: source,
                    relations: (Correspondence::Exact, Correspondence::Exact),
                    envelopes: ((source.0, source.0), (source.1, source.1)),
                    related: RelatedSpan::Source(SourceSpan::new(source.0, source.1).unwrap()),
                    expanded: false,
                },
            );
        }
        assert!(matches!(
            query(
                model,
                RangeInput::Source {
                    target: TextTarget::Flow(alt),
                    span: SourceSpan::new(5, 6).unwrap(),
                }
            ),
            Err(CoreError::OutsideScope)
        ));
    });
}

#[test]
fn decoded_link_payload_range_keeps_its_original_entity_source() {
    with_model("[x](a&amp;b)", |model| {
        let ir = model.ir();
        let links: Vec<_> = ir
            .details()
            .iter()
            .filter_map(|row| match row {
                DetailRow::Link { destination, .. } => Some(*destination),
                _ => None,
            })
            .collect();
        let [payload] = links.as_slice() else {
            panic!()
        };
        let value = ir.payloads()[payload.index()];
        assert_eq!(ir.text(value.value).unwrap(), "a&b");
        assert_eq!(value.utf8_len, 3);
        let response = query(
            model,
            RangeInput::Payload {
                payload: *payload,
                span: PayloadSpan::new(1, 2).unwrap(),
            },
        )
        .unwrap();
        single(
            model,
            &response,
            Expected {
                target: TextTarget::Payload(*payload),
                atoms: vec![
                    ParentAtom {
                        kind: AtomKind::Copy,
                        source: (4, 5),
                        target: (0, 1),
                    },
                    ParentAtom {
                        kind: AtomKind::Transform,
                        source: (5, 10),
                        target: (1, 2),
                    },
                    ParentAtom {
                        kind: AtomKind::Copy,
                        source: (10, 11),
                        target: (2, 3),
                    },
                ],
                content_atom: 1,
                boundary_atoms: (vec![0, 1], vec![1, 2]),
                direction: Direction::TargetToSource,
                kind: AtomKind::Transform,
                source: (5, 10),
                target_span: (1, 2),
                chosen: (5, 10),
                relations: (Correspondence::Exact, Correspondence::Exact),
                envelopes: ((5, 5), (10, 10)),
                related: RelatedSpan::Source(SourceSpan::new(5, 10).unwrap()),
                expanded: false,
            },
        );
        assert!(matches!(
            query(
                model,
                RangeInput::Source {
                    target: TextTarget::Payload(*payload),
                    span: SourceSpan::new(1, 2).unwrap(),
                }
            ),
            Err(CoreError::OutsideScope)
        ));
    });
}
