// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE file in the project root for license information.
// ---------------------------------------------------------------------------------------------

use super::*;
use crate::config::Profile;
use crate::identity::{LogicalBoundary, LogicalSpan, SourceBoundary, SourceKey, SourceSpan};
use crate::ir::{AtomId, AtomKind, FlowScope, TargetSpan, TextTarget};
use crate::mapping::{
    Affinity, Contribution, Contributor, Correspondence, Direction, Effects, PointInput,
    PointResult, Position, QueryResponse, RangeInput, RangeQuery, RangeResult, RelatedSpan,
};
use crate::policy::{CancellationToken, ResourceAdmissionMode, WorkStatsCollection};
use crate::reader::MainReader;
use crate::source::SourceDocument;

const COLLECTIONS: [WorkStatsCollection; 1] = [WorkStatsCollection::Disabled];

#[test]
fn review_regression_inline_html_probe_rejects_foreign_budget_before_scanning() {
    let budget = SourceBudget::model_fixture();
    let source = SourceDocument::new(
        SourceKey::new([5; 16], 1).unwrap(),
        b"Text <ruby>x<rt>y</rt></ruby>",
        &mut budget.begin(CancellationToken::new()),
    )
    .unwrap();
    let other = SourceBudget::model_fixture();
    let mut op = other.begin(CancellationToken::new());
    assert!(matches!(
        AcceptedModel::parse(
            source.snapshot(),
            ParseConfig::with_extensions(Profile::G3, Extensions::HTML_INLINE),
            1,
            &mut op
        ),
        Err(CoreError::WrongBudget)
    ));
    assert_eq!(op.stats().unwrap().scanned_bytes(), 0);
    assert_eq!(op.first_fault(), Some(CoreError::WrongBudget));
}

fn operation(budget: &SourceBudget) -> Operation {
    let mut op = budget.begin(CancellationToken::new());
    if matches!(budget.admission_mode(), ResourceAdmissionMode::TaskAccess) {
        assert_eq!(op.first_fault(), None);
        op.set_clock_samples_for_test(&[100_000_000]);
    } else {
        op.set_clock_samples_for_test(&[100_000_000]);
    }
    assert_collection(budget, &op);
    op
}

fn assert_collection(budget: &SourceBudget, op: &Operation) {
    assert_eq!(
        op.stats().is_some(),
        budget.work_stats_collection() == WorkStatsCollection::Enabled
    );
}

fn assert_released(budget: &SourceBudget) {
    let usage = budget.usage();
    assert_eq!(usage.source_capacity, 0);
    assert_eq!(usage.auxiliary_capacity, 0);
    assert_eq!(usage.source_owners, 0);
}

#[test]
fn elapsed_source_to_model_keeps_its_window_and_prior_model_pin() {
    for _collection in COLLECTIONS {
        let budget = SourceBudget::task_fixture();
        let mut op = operation(&budget);
        let document =
            SourceDocument::new(SourceKey::new([4; 16], 1).unwrap(), b"A\r\nB", &mut op).unwrap();
        op.set_clock_samples_for_test(&[490_000_000]);
        let model = AcceptedModel::parse(
            document.snapshot(),
            ParseConfig::new(Profile::C0),
            1,
            &mut op,
        )
        .unwrap();
        assert_eq!(model.ir().flows()[0].logical_utf16, 3);
        assert_eq!(model.source().text(), "A\r\nB");

        op.set_clock_samples_for_test(&[500_000_000]);
        assert!(matches!(
            AcceptedModel::parse(
                document.snapshot(),
                ParseConfig::new(Profile::C0),
                2,
                &mut op,
            ),
            Err(CoreError::DeadlineExceeded {
                elapsed_ns: 500_000_000,
                limit_ns: 500_000_000,
            })
        ));
        assert_collection(&budget, &op);

        // A distinct query can still consume the previously accepted immutable
        // owner; expiry does not invalidate that owner or release an external pin.
        let mut query_op = operation(&budget);
        let response = model
            .map_point(
                &PointQuery {
                    model: model.identity(),
                    input: PointInput::Source {
                        target: TextTarget::Flow(model.main_flow_id()),
                        byte: SourceBoundary::new(4),
                    },
                    affinity: Affinity::After,
                },
                &mut query_op,
            )
            .unwrap();
        assert_eq!(response.results()[0].correspondence, Correspondence::Exact);
        assert_eq!(
            response.results()[0].chosen,
            Position::Flow(LogicalBoundary::new(3))
        );
        assert_collection(&budget, &query_op);
        drop(document);
        assert!(budget.usage().source_capacity > 0);
        drop(model);
        assert_eq!(budget.usage().source_capacity, 0);
        assert!(budget.usage().auxiliary_capacity > 0);
        drop(response);
        assert_released(&budget);
    }
}

#[derive(Clone, Copy)]
enum OracleInput {
    Source(u64),
    Logical(u64),
    AltSource(u64),
    AltLogical(u64),
}

#[derive(Clone, Copy)]
struct BoundaryOracle {
    chosen: u64,
    envelope: (u64, u64),
    relation: Correspondence,
}

struct PointOracle {
    input: OracleInput,
    affinity: Affinity,
    expected: Option<BoundaryOracle>,
}

fn point(
    input: OracleInput,
    affinity: Affinity,
    expected: (u64, (u64, u64), Correspondence),
) -> PointOracle {
    PointOracle {
        input,
        affinity,
        expected: Some(BoundaryOracle {
            chosen: expected.0,
            envelope: expected.1,
            relation: expected.2,
        }),
    }
}

fn effects(relation: Correspondence) -> Effects {
    Effects {
        atomic: relation == Correspondence::Atomic,
        hidden: relation == Correspondence::Hidden,
        synthetic: relation == Correspondence::Synthetic,
        object: relation == Correspondence::Object,
    }
}

fn check_boundary<T: Copy>(
    response: &QueryResponse<T>,
    actual: &PointResult,
    expected: BoundaryOracle,
    model: &AcceptedModel,
    target: TextTarget,
    direction: Direction,
) {
    assert_eq!(actual.model, model.identity());
    assert_eq!(actual.target, target);
    assert_eq!(actual.direction, direction);
    let (chosen, envelope) = match direction {
        Direction::SourceToTarget => (
            Position::Flow(LogicalBoundary::new(expected.chosen)),
            RelatedSpan::Flow(LogicalSpan::new(expected.envelope.0, expected.envelope.1).unwrap()),
        ),
        Direction::TargetToSource => (
            Position::Source(SourceBoundary::new(expected.chosen)),
            RelatedSpan::Source(SourceSpan::new(expected.envelope.0, expected.envelope.1).unwrap()),
        ),
    };
    assert_eq!(actual.chosen, chosen);
    assert_eq!(actual.related.envelope, Some(envelope));
    assert_eq!(actual.effects, effects(expected.relation));
    assert_eq!(actual.correspondence, expected.relation);
    assert!(
        !actual
            .contributors
            .slice(response.contributors())
            .unwrap()
            .is_empty()
    );
}

fn check_points(model: &AcceptedModel, points: &[PointOracle]) {
    for control in points {
        let (alt, source, position) = match control.input {
            OracleInput::Source(value) => (false, true, value),
            OracleInput::Logical(value) => (false, false, value),
            OracleInput::AltSource(value) => (true, true, value),
            OracleInput::AltLogical(value) => (true, false, value),
        };
        let flow = if alt {
            let flows: Vec<_> = model
                .ir()
                .flows()
                .iter()
                .enumerate()
                .filter(|(_, row)| matches!(row.scope, FlowScope::ImageAlt { .. }))
                .map(|(index, _)| FlowId::from_index(index).unwrap())
                .collect();
            assert_eq!(flows.len(), 1);
            flows[0]
        } else {
            model.main_flow_id()
        };
        let input = if source {
            PointInput::Source {
                target: TextTarget::Flow(flow),
                byte: SourceBoundary::new(position),
            }
        } else {
            PointInput::Flow {
                flow,
                utf16: LogicalBoundary::new(position),
            }
        };
        let mut op = operation(model.budget());
        let response = model.map_point(
            &PointQuery {
                model: model.identity(),
                input,
                affinity: control.affinity,
            },
            &mut op,
        );
        if let Some(expected) = control.expected {
            let response = response.unwrap();
            assert_eq!(response.results().len(), 1);
            check_boundary(
                &response,
                &response.results()[0],
                expected,
                model,
                TextTarget::Flow(flow),
                if source {
                    Direction::SourceToTarget
                } else {
                    Direction::TargetToSource
                },
            );
        } else {
            assert!(matches!(response, Err(CoreError::OutsideScope)));
        }
        assert_collection(model.budget(), &op);
    }
}

fn check_main(model: &AcceptedModel, expected: &str) {
    let mut reader = MainReader::new(model.clone(), operation(model.budget())).unwrap();
    let mut buffer = [0; 4];
    let mut actual = String::new();
    loop {
        let batch = reader.read_into(&mut buffer).unwrap();
        actual.push_str(std::str::from_utf8(&buffer[..batch.written]).unwrap());
        assert_eq!(batch.utf16, actual.encode_utf16().count() as u64);
        if batch.done {
            break;
        }
        assert_ne!(batch.written, 0);
    }
    assert_eq!(actual, expected);
    assert_eq!(
        model.ir().flows()[model.main_flow_id().index()].logical_utf16,
        expected.encode_utf16().count() as u64
    );
}

#[test]
fn elapsed_collections_keep_five_original_parser_main_and_point_oracles() {
    use Affinity::{After as A, Before as B};
    use Correspondence::{Atomic, Exact, Hidden, Object};
    use OracleInput::{AltLogical as AL, AltSource as AS, Logical as L, Source as S};

    // Literals copied from m1-provenance.1 independent-goldens.json, not from
    // any budget lane's output. This covers all 20 original point queries;
    // private production AtomIds are not the oracle's fragment IDs.
    let partial_tab = [
        point(L(1), B, (2, (2, 3), Atomic)),
        point(L(1), A, (3, (2, 3), Atomic)),
        point(L(0), B, (0, (0, 2), Hidden)),
        point(L(0), A, (2, (0, 2), Hidden)),
        point(S(5), A, (4, (4, 4), Exact)),
    ];
    let image = [
        point(S(5), A, (1, (0, 1), Object)),
        point(L(1), B, (11, (11, 11), Exact)),
        point(AL(1), B, (3, (3, 3), Exact)),
        point(AL(2), B, (4, (4, 5), Hidden)),
        point(AL(2), A, (5, (4, 5), Hidden)),
        PointOracle {
            input: AS(9),
            affinity: A,
            expected: None,
        },
    ];
    let reference = [
        point(L(1), B, (2, (2, 29), Hidden)),
        point(L(1), A, (29, (2, 29), Hidden)),
        point(S(23), B, (1, (1, 1), Hidden)),
    ];
    let code = [
        point(S(2), B, (1, (1, 1), Exact)),
        point(L(2), A, (3, (3, 3), Exact)),
    ];
    let entities = [
        point(S(5), B, (1, (1, 1), Exact)),
        point(S(5), A, (1, (1, 1), Exact)),
        point(L(1), B, (5, (5, 5), Exact)),
        point(L(1), A, (5, (5, 5), Exact)),
    ];
    let fixtures: [(&[u8], u8, &str, &[PointOracle]); 5] = [
        (
            include_bytes!(
                "../../../../tests/fixtures/m1-provenance/inputs/e04.container.partial-tab-code.md"
            ),
            0x09,
            "  foo\n",
            &partial_tab,
        ),
        (
            include_bytes!(
                "../../../../tests/fixtures/m1-provenance/inputs/e04.image.emphasis-alt.md"
            ),
            0x0b,
            "\u{fffc}",
            &image,
        ),
        (
            include_bytes!(
                "../../../../tests/fixtures/m1-provenance/inputs/e04.reference.multiline-first-wins.md"
            ),
            0x0e,
            "x",
            &reference,
        ),
        (
            include_bytes!(
                "../../../../tests/fixtures/m1-provenance/inputs/e04.fidelity.code-copy-interior.md"
            ),
            0x11,
            "abc",
            &code,
        ),
        (
            include_bytes!(
                "../../../../tests/fixtures/m1-provenance/inputs/e04.fidelity.adjacent-entities.md"
            ),
            0x12,
            "&&",
            &entities,
        ),
    ];
    for budget in [SourceBudget::model_fixture(), SourceBudget::task_fixture()] {
        for (bytes, last_key_byte, main, points) in fixtures {
            let mut key = [0; 16];
            key[0] = 4;
            key[15] = last_key_byte;
            let mut op = operation(&budget);
            let document =
                SourceDocument::new(SourceKey::new(key, 1).unwrap(), bytes, &mut op).unwrap();
            let model = AcceptedModel::parse(
                document.snapshot(),
                ParseConfig::new(Profile::C0),
                1,
                &mut op,
            )
            .unwrap();
            assert_collection(&budget, &op);
            check_main(&model, main);
            check_points(&model, points);
            if last_key_byte == 0x11 {
                check_code_ranges(&model);
            }
            if last_key_byte == 0x0e {
                check_reference_winner(&model);
            }
            drop(model);
            drop(document);
            assert_released(&budget);
        }
    }
}

fn check_reference_winner(model: &AcceptedModel) {
    // The original Main text alone cannot prove which definition won.
    let ir = model.ir();
    assert_eq!(ir.references().len(), 1);
    assert_eq!(ir.definitions().len(), 2);
    assert_eq!(ir.definitions().iter().filter(|row| row.winner).count(), 1);
    let reference = ir.references()[0];
    assert_eq!(ir.text(reference.key).unwrap(), "a");
    let definition = ir.definitions()[reference.winner.index()];
    assert!(definition.winner);
    // Definition and reference payloads have distinct owners and IDs. Compare
    // each semantic value with the independent oracle, not those private IDs.
    assert_eq!(
        ir.text(ir.payloads()[definition.destination.index()].value)
            .unwrap(),
        "/u"
    );
    assert_eq!(
        ir.text(ir.payloads()[definition.title.unwrap().index()].value)
            .unwrap(),
        "t"
    );
    assert_eq!(
        ir.text(ir.payloads()[reference.destination.index()].value)
            .unwrap(),
        "/u"
    );
    assert_eq!(
        ir.text(ir.payloads()[reference.title.unwrap().index()].value)
            .unwrap(),
        "t"
    );
    let rejected = ir.definitions().iter().find(|row| !row.winner).unwrap();
    assert_eq!(
        ir.text(ir.payloads()[rejected.destination.index()].value)
            .unwrap(),
        "/v"
    );
    assert!(rejected.title.is_none());
}

fn check_code_ranges(model: &AcceptedModel) {
    // m1-range.1 code.logical-excludes-edge-hidden and
    // code.source-includes-hidden. Resolve original fragment identities by
    // their independently specified kind/source/target, never by raw AtomId.
    // Complete the independent m1-range.1 code parent before reusing its
    // queries. The production Document is a scaffold around its Paragraph.
    use crate::ir::NodeKind;
    let ir = model.ir();
    assert_eq!(model.source().text(), "`abc`");
    assert_eq!(ir.flows().len(), 1);
    let main = ir.flows()[model.main_flow_id().index()];
    assert_eq!(main.scope, FlowScope::Main);
    assert_eq!(main.logical_utf16, 3);
    assert!(
        main.domains
            .slice(ir.sources())
            .unwrap()
            .iter()
            .eq([SourceSpan::new(0, 5).unwrap()].iter())
    );
    assert!(ir.objects().is_empty());
    assert!(ir.payloads().is_empty());
    assert!(ir.references().is_empty());
    assert!(ir.definitions().is_empty());
    assert_eq!(ir.nodes().len(), 3);
    let root = ir.nodes()[main.root.index()];
    assert_eq!(root.kind, NodeKind::Document);
    assert!(root.parent.is_none());
    let paragraph_id = root.first_child.unwrap();
    let paragraph = ir.nodes()[paragraph_id.index()];
    assert_eq!(paragraph.kind, NodeKind::Paragraph);
    assert_eq!(paragraph.parent, Some(main.root));
    assert!(paragraph.next_sibling.is_none());
    let code = ir.nodes()[paragraph.first_child.unwrap().index()];
    assert_eq!(code.kind, NodeKind::InlineCode);
    assert_eq!(code.parent, Some(paragraph_id));
    assert!(code.next_sibling.is_none());
    assert!(code.first_child.is_none());
    for node in [paragraph, code] {
        assert_eq!(node.flow, model.main_flow_id());
        assert_eq!(node.source_envelope, SourceSpan::new(0, 5).unwrap());
        assert_eq!(node.logical, LogicalSpan::new(0, 3).unwrap());
    }
    let expected_atoms = [
        (AtomKind::Hidden, (0, 1), (0, 0)),
        (AtomKind::Copy, (1, 4), (0, 3)),
        (AtomKind::Hidden, (4, 5), (3, 3)),
    ];
    let flow = model.main_flow_id();
    let atoms = model.ir().flows()[flow.index()].atoms;
    assert_eq!(atoms.len(), 3);
    let mut ids = Vec::new();
    let mut op = operation(model.budget());
    for (kind, source, target) in expected_atoms {
        let mut matched = None;
        for index in atoms.indices(model.ir().atoms_len()).unwrap() {
            let row = model.ir().atom_at(index, &mut op).unwrap();
            if row.kind == kind
                && row.target
                    == (TargetSpan {
                        start: target.0,
                        end: target.1,
                    })
                && row
                    .sources
                    .slice(model.ir().sources())
                    .unwrap()
                    .iter()
                    .eq([SourceSpan::new(source.0, source.1).unwrap()].iter())
            {
                assert!(
                    matched
                        .replace(AtomId::from_index(index).unwrap())
                        .is_none()
                );
            }
        }
        ids.push(matched.expect("original code atom must exist exactly once"));
    }
    assert_collection(model.budget(), &op);
    for from_source in [false, true] {
        let mut op = operation(model.budget());
        let direction = if from_source {
            Direction::SourceToTarget
        } else {
            Direction::TargetToSource
        };
        let response = model
            .map_range(
                &RangeQuery {
                    model: model.identity(),
                    input: if from_source {
                        RangeInput::Source {
                            target: TextTarget::Flow(flow),
                            span: SourceSpan::new(0, 5).unwrap(),
                        }
                    } else {
                        RangeInput::Flow {
                            flow,
                            span: LogicalSpan::new(0, 3).unwrap(),
                        }
                    },
                    empty_affinity: Affinity::Before,
                },
                &mut op,
            )
            .unwrap();
        assert_eq!(response.results().len(), 1);
        let RangeResult::Selection(rows) = response.results()[0] else {
            panic!("nonempty code query must return Selection");
        };
        let selections = rows.slice(response.selections()).unwrap();
        assert_eq!(selections.len(), 1);
        let selection = selections[0];
        assert_eq!(selection.model, model.identity());
        assert_eq!(selection.target, TextTarget::Flow(flow));
        assert_eq!(selection.direction, direction);
        assert!(!selection.expanded);
        let boundaries = if from_source {
            [
                BoundaryOracle {
                    chosen: 0,
                    envelope: (0, 0),
                    relation: Correspondence::Exact,
                },
                BoundaryOracle {
                    chosen: 3,
                    envelope: (3, 3),
                    relation: Correspondence::Exact,
                },
            ]
        } else {
            [
                BoundaryOracle {
                    chosen: 0,
                    envelope: (0, 1),
                    relation: Correspondence::Hidden,
                },
                BoundaryOracle {
                    chosen: 5,
                    envelope: (4, 5),
                    relation: Correspondence::Hidden,
                },
            ]
        };
        for (actual, expected) in [selection.start, selection.end].iter().zip(boundaries) {
            check_boundary(
                &response,
                actual,
                expected,
                model,
                TextTarget::Flow(flow),
                direction,
            );
        }
        let expected_contributors = |indices: &[usize], relations: &[Correspondence]| {
            indices
                .iter()
                .zip(relations)
                .map(|(&index, &relation)| {
                    let (kind, source, target) = expected_atoms[index];
                    Contributor {
                        atom: ids[index],
                        source_part: 0,
                        kind,
                        source: SourceSpan::new(source.0, source.1).unwrap(),
                        target: TargetSpan {
                            start: target.0,
                            end: target.1,
                        },
                        relation,
                    }
                })
                .collect::<Vec<_>>()
        };
        let (start_contributors, end_contributors) = if from_source {
            (
                expected_contributors(&[0], &[Correspondence::Exact]),
                expected_contributors(&[2], &[Correspondence::Exact]),
            )
        } else {
            (
                expected_contributors(&[0, 1], &[Correspondence::Hidden, Correspondence::Exact]),
                expected_contributors(&[1, 2], &[Correspondence::Exact, Correspondence::Hidden]),
            )
        };
        assert_eq!(
            selection
                .start
                .contributors
                .slice(response.contributors())
                .unwrap(),
            start_contributors
        );
        assert_eq!(
            selection
                .end
                .contributors
                .slice(response.contributors())
                .unwrap(),
            end_contributors
        );
        let expected_content: Vec<_> = (0..3)
            .filter(|&index| from_source || index == 1)
            .map(|index| {
                let (kind, source, target) = expected_atoms[index];
                Contribution {
                    atom: ids[index],
                    source_part: 0,
                    kind,
                    source: SourceSpan::new(source.0, source.1).unwrap(),
                    target: TargetSpan {
                        start: target.0,
                        end: target.1,
                    },
                    expanded: false,
                }
            })
            .collect();
        assert_eq!(
            selection
                .contributions
                .slice(response.contributions())
                .unwrap(),
            expected_content
        );
        let span = if from_source {
            RelatedSpan::Flow(LogicalSpan::new(0, 3).unwrap())
        } else {
            RelatedSpan::Source(SourceSpan::new(1, 4).unwrap())
        };
        assert_eq!(
            selection.related.spans.slice(response.spans()).unwrap(),
            [span]
        );
        assert_eq!(selection.related.envelope, Some(span));
        let anchors = if from_source {
            vec![
                Position::Flow(LogicalBoundary::new(0)),
                Position::Flow(LogicalBoundary::new(3)),
            ]
        } else {
            Vec::new()
        };
        assert_eq!(
            selection.related.anchors.slice(response.anchors()).unwrap(),
            anchors
        );
        assert_eq!(
            selection.effects,
            Effects {
                hidden: from_source,
                ..Effects::default()
            }
        );
        assert_collection(model.budget(), &op);
    }
}

#[test]
fn elapsed_parser_mapping_keeps_utf8_utf16_boundaries_across_sparse_checkpoint() {
    use Correspondence::Exact;
    use OracleInput::{Logical as L, Source as S};

    // 1023 ASCII bytes precede a four-byte/two-unit scalar, a three-byte/one-unit
    // scalar and one ASCII byte. These endpoints follow Unicode encoding, not
    // the parser's private atom or checkpoint representation.
    let text = format!("{}😀中z", "a".repeat(1023));
    assert_eq!(text.len(), 1031);
    assert_eq!(text.encode_utf16().count(), 1027);
    for budget in [SourceBudget::task_fixture()] {
        let mut op = operation(&budget);
        let document = SourceDocument::new(
            SourceKey::new([0x23; 16], 1).unwrap(),
            text.as_bytes(),
            &mut op,
        )
        .unwrap();
        let model = AcceptedModel::parse(
            document.snapshot(),
            ParseConfig::new(Profile::C0),
            1,
            &mut op,
        )
        .unwrap();
        assert_collection(&budget, &op);
        check_main(&model, &text);
        for (source, logical) in [
            (0, 0),
            (1023, 1023),
            (1027, 1025),
            (1030, 1026),
            (1031, 1027),
        ] {
            for affinity in [Affinity::Before, Affinity::After] {
                check_points(
                    &model,
                    &[
                        point(S(source), affinity, (logical, (logical, logical), Exact)),
                        point(L(logical), affinity, (source, (source, source), Exact)),
                    ],
                );
            }
        }
        let flow = model.main_flow_id();
        let invalid_sources = [1024, 1025, 1026, 1028, 1029].map(|byte| PointInput::Source {
            target: TextTarget::Flow(flow),
            byte: SourceBoundary::new(byte),
        });
        for input in invalid_sources.into_iter().chain([PointInput::Flow {
            flow,
            utf16: LogicalBoundary::new(1024),
        }]) {
            for affinity in [Affinity::Before, Affinity::After] {
                let mut query_op = operation(&budget);
                assert!(matches!(
                    model.map_point(
                        &PointQuery {
                            model: model.identity(),
                            input,
                            affinity,
                        },
                        &mut query_op,
                    ),
                    Err(CoreError::InvalidBoundary)
                ));
                assert_collection(&budget, &query_op);
            }
        }
        drop(model);
        drop(document);
        drop(op);
        assert_released(&budget);
    }
}
