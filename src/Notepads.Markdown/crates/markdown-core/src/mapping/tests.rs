// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE file in the project root for license information.
// ---------------------------------------------------------------------------------------------

//! Handwritten mapping controls have no native or AcceptedModel authority.
//! Coordinates and assertions are copied from the original frozen independent
//! models/range-goldens, not from a candidate mapping implementation.

use super::*;
use crate::config::{ParseConfig, Profile};
use crate::identity::SourceKey;
use crate::ir::{
    AtomContent, AtomRow, FlowId, FlowRow, FlowScope, IrBuilder, NodeId, NodeKind, NodeRow,
    ObjectId, PayloadId, PayloadOwner, PayloadRole, PayloadRow,
};
use crate::policy::{CancellationToken, ModelLimits, SourceBudget, SourceLimits};
use crate::source::SourceDocument;

const GOLDENS: &str =
    include_str!("../../../../tests/fixtures/m1-range/expected/range-goldens.json");

#[derive(Clone, Copy)]
struct Spec<'a> {
    kind: AtomKind,
    sources: &'a [(u64, u64)],
    target: (u64, u64),
    text: Option<&'a str>,
}

fn atom<'a>(
    kind: AtomKind,
    sources: &'a [(u64, u64)],
    target: (u64, u64),
    text: Option<&'a str>,
) -> Spec<'a> {
    Spec {
        kind,
        sources,
        target,
        text,
    }
}

struct FlowSpec<'a> {
    domains: &'a [(u64, u64)],
    length: u64,
    atoms: &'a [Spec<'a>],
}
struct PayloadSpec<'a> {
    domains: &'a [(u64, u64)],
    value: &'a str,
    atoms: &'a [Spec<'a>],
}

struct Control {
    source: SourceSnapshot,
    ir: CompactIr,
    mapping: MapIndex,
    budget: SourceBudget,
    identity: ModelIdentity,
}

fn control(bytes: &[u8], flows: &[FlowSpec<'_>], payloads: &[PayloadSpec<'_>]) -> Control {
    control_with_budget(bytes, flows, payloads, SourceBudget::model_fixture())
}

fn control_with_budget(
    bytes: &[u8],
    flows: &[FlowSpec<'_>],
    payloads: &[PayloadSpec<'_>],
    budget: SourceBudget,
) -> Control {
    let key = SourceKey::new([4; 16], 1).unwrap();
    let identity = ModelIdentity::new(key, 1, ParseConfig::new(Profile::C0)).unwrap();
    let mut op = budget.begin(CancellationToken::new());
    let source = SourceDocument::new(key, bytes, &mut op).unwrap().snapshot();
    let mut builder = IrBuilder::new(&mut op).unwrap();
    for (index, spec) in flows.iter().enumerate() {
        let flow = FlowId::from_index(index).unwrap();
        let node = builder
            .node(
                NodeRow {
                    parent: None,
                    first_child: None,
                    next_sibling: None,
                    flow,
                    kind: if index == 0 {
                        NodeKind::Document
                    } else {
                        NodeKind::AltRoot
                    },
                    detail: None,
                    source_envelope: SourceSpan::new(0, bytes.len() as u64).unwrap(),
                    logical: LogicalSpan::new(0, spec.length).unwrap(),
                },
                &mut op,
            )
            .unwrap();
        let domains = source_rows(&mut builder, spec.domains, &mut op);
        let begin = builder.ir.atoms_len();
        for row in spec.atoms {
            append_atom(&mut builder, node, *row, &mut op);
        }
        let scope = if index == 0 {
            FlowScope::Main
        } else {
            FlowScope::ImageAlt {
                object: ObjectId::from_index(0).unwrap(),
            }
        };
        builder
            .flow(
                FlowRow {
                    scope,
                    root: node,
                    domains,
                    atoms: Rows::new(begin, spec.atoms.len()).unwrap(),
                    logical_utf16: spec.length,
                },
                &mut op,
            )
            .unwrap();
    }
    for spec in payloads {
        let domains = source_rows(&mut builder, spec.domains, &mut op);
        let begin = builder.ir.atoms_len();
        for row in spec.atoms {
            append_atom(&mut builder, NodeId::from_index(0).unwrap(), *row, &mut op);
        }
        let value = builder.text(spec.value, &mut op).unwrap();
        builder
            .payload(
                PayloadRow {
                    owner: PayloadOwner::Node(NodeId::from_index(0).unwrap()),
                    role: PayloadRole::Destination,
                    value,
                    domains,
                    atoms: Rows::new(begin, spec.atoms.len()).unwrap(),
                    utf8_len: spec.value.len() as u64,
                },
                &mut op,
            )
            .unwrap();
    }
    let ir = builder.ir.snapshot(&mut op).unwrap();
    drop(builder);
    let mapping = MapIndex::build(&ir, &source, identity, &mut op).unwrap();
    Control {
        source,
        ir,
        mapping,
        budget,
        identity,
    }
}

fn source_rows(
    builder: &mut IrBuilder,
    parts: &[(u64, u64)],
    op: &mut Operation,
) -> Rows<SourceSpan> {
    let values: Vec<_> = parts
        .iter()
        .map(|(a, b)| SourceSpan::new(*a, *b).unwrap())
        .collect();
    builder.source_parts(&values, op).unwrap()
}

fn append_atom(builder: &mut IrBuilder, owner: NodeId, row: Spec<'_>, op: &mut Operation) {
    let sources = source_rows(builder, row.sources, op);
    let content = row
        .text
        .map(|text| AtomContent::Pool(builder.text(text, op).unwrap()))
        .unwrap_or(AtomContent::Empty);
    builder
        .atom(
            AtomRow {
                owner,
                kind: row.kind,
                target: TargetSpan {
                    start: row.target.0,
                    end: row.target.1,
                },
                sources,
                claim: IrBuilder::unbound_authority(),
                proof: None,
                content,
            },
            op,
        )
        .unwrap();
}

fn main_target() -> TextTarget {
    TextTarget::Flow(FlowId::from_index(0).unwrap())
}
fn alt_target() -> TextTarget {
    TextTarget::Flow(FlowId::from_index(1).unwrap())
}
fn point_source(target: TextTarget, byte: u64) -> PointInput {
    PointInput::Source {
        target,
        byte: SourceBoundary::new(byte),
    }
}
fn point_flow(index: usize, position: u64) -> PointInput {
    PointInput::Flow {
        flow: FlowId::from_index(index).unwrap(),
        utf16: LogicalBoundary::new(position),
    }
}
fn source_range(target: TextTarget, start: u64, end: u64) -> RangeInput {
    RangeInput::Source {
        target,
        span: SourceSpan::new(start, end).unwrap(),
    }
}
fn flow_range(index: usize, start: u64, end: u64) -> RangeInput {
    RangeInput::Flow {
        flow: FlowId::from_index(index).unwrap(),
        span: LogicalSpan::new(start, end).unwrap(),
    }
}

impl Control {
    fn point(&self, input: PointInput, affinity: Affinity) -> Result<PointResponse, CoreError> {
        let mut op = self.budget.begin(CancellationToken::new());
        self.mapping.map_point(
            &self.ir,
            &self.source,
            &PointQuery {
                model: self.identity,
                input,
                affinity,
            },
            &mut op,
        )
    }
    fn range(&self, input: RangeInput, affinity: Affinity) -> Result<RangeResponse, CoreError> {
        let mut op = self.budget.begin(CancellationToken::new());
        self.mapping.map_range(
            &self.ir,
            &self.source,
            &RangeQuery {
                model: self.identity,
                input,
                empty_affinity: affinity,
            },
            &mut op,
        )
    }
}

fn selection(response: &RangeResponse) -> Selection {
    match response.results()[0] {
        RangeResult::Selection(result) => result.slice(response.selections()).unwrap()[0],
        _ => panic!("selection expected"),
    }
}
fn ids(response: &PointResponse) -> Vec<usize> {
    response
        .contributors()
        .iter()
        .map(|x| x.atom.index())
        .collect()
}
fn related_spans<T: Copy>(response: &QueryResponse<T>, rows: Related) -> Vec<(u64, u64)> {
    rows.spans
        .slice(response.spans())
        .unwrap()
        .iter()
        .map(|x| (x.start(), x.end()))
        .collect()
}
fn related_anchors<T: Copy>(response: &QueryResponse<T>, rows: Related) -> Vec<u64> {
    rows.anchors
        .slice(response.anchors())
        .unwrap()
        .iter()
        .map(|x| x.value())
        .collect()
}

fn strong() -> Control {
    control(
        include_bytes!(
            "../../../../tests/fixtures/m1/inputs/project.map.strong-entity-cjk-astral.bin"
        ),
        &[FlowSpec {
            domains: &[(0, 16)],
            length: 4,
            atoms: &[
                atom(AtomKind::Hidden, &[(0, 2)], (0, 0), None),
                atom(AtomKind::Copy, &[(2, 5)], (0, 1), None),
                atom(AtomKind::Transform, &[(5, 10)], (1, 2), Some("&")),
                atom(AtomKind::Copy, &[(10, 14)], (2, 4), None),
                atom(AtomKind::Hidden, &[(14, 16)], (4, 4), None),
            ],
        }],
        &[],
    )
}

#[test]
fn strong_affinities_atoms_and_scalar_rejections() {
    let c = strong();
    for (affinity, chosen) in [(Affinity::Before, 1), (Affinity::After, 2)] {
        let r = c.point(point_source(main_target(), 7), affinity).unwrap();
        assert_eq!(r.results()[0].chosen.value(), chosen);
        assert_eq!(r.results()[0].correspondence, Correspondence::Atomic);
        assert_eq!(related_spans(&r, r.results()[0].related), vec![(1, 2)]);
        assert_eq!(ids(&r), vec![2]);
    }
    assert!(matches!(
        c.point(point_flow(0, 3), Affinity::Before),
        Err(CoreError::InvalidBoundary)
    ));
    assert!(matches!(
        c.point(point_source(main_target(), 3), Affinity::Before),
        Err(CoreError::InvalidBoundary)
    ));
    let r = c
        .range(source_range(main_target(), 6, 9), Affinity::Before)
        .unwrap();
    let result = selection(&r);
    assert!(result.expanded);
    // Frozen m1-range.1 entities.partial-first: only the opposite side expands.
    assert_eq!(r.contributions()[0].source, SourceSpan::new(6, 9).unwrap());
    assert_eq!(related_spans(&r, result.related), vec![(1, 2)]);
    let full = c.range(flow_range(0, 1, 2), Affinity::Before).unwrap();
    assert!(!selection(&full).expanded);
    assert!(selection(&full).effects.atomic);
}

#[test]
fn one_entity_has_two_scalars_and_combining_boundary_is_atomic() {
    let c = control(
        b"&NotEqualTilde;",
        &[FlowSpec {
            domains: &[(0, 15)],
            length: 2,
            atoms: &[atom(
                AtomKind::Transform,
                &[(0, 15)],
                (0, 2),
                Some("\u{2242}\u{338}"),
            )],
        }],
        &[],
    );
    for (affinity, position) in [(Affinity::Before, 0), (Affinity::After, 15)] {
        let r = c.point(point_flow(0, 1), affinity).unwrap();
        assert_eq!(r.results()[0].chosen.value(), position);
        assert_eq!(r.results()[0].correspondence, Correspondence::Atomic);
        assert_eq!(related_spans(&r, r.results()[0].related), vec![(0, 15)]);
    }
}

#[test]
fn adjacent_entities_keep_two_exact_seam_contributors() {
    assert!(
        include_str!("../../../../tests/fixtures/m1-range/models/entities.json")
            .contains("\"logical_utf16\": 2")
    );
    let c = control(
        include_bytes!("../../../../tests/fixtures/m1-range/inputs/entities.bin"),
        &[FlowSpec {
            domains: &[(0, 10)],
            length: 2,
            atoms: &[
                atom(AtomKind::Transform, &[(0, 5)], (0, 1), Some("&")),
                atom(AtomKind::Transform, &[(5, 10)], (1, 2), Some("&")),
            ],
        }],
        &[],
    );
    let r = c
        .point(point_source(main_target(), 5), Affinity::After)
        .unwrap();
    assert_eq!(r.results()[0].correspondence, Correspondence::Exact);
    assert_eq!(r.results()[0].chosen.value(), 1);
    assert_eq!(ids(&r), vec![0, 1]);
    let r = c.point(point_flow(0, 1), Affinity::Before).unwrap();
    assert_eq!(r.results()[0].chosen.value(), 5);
    assert_eq!(ids(&r), vec![0, 1]);
    assert!(!r.results()[0].effects.atomic);
}

#[test]
fn crlf_transform_point_and_content_are_minimal() {
    let c = control(
        b"a\r\nb",
        &[FlowSpec {
            domains: &[(0, 4)],
            length: 3,
            atoms: &[
                atom(AtomKind::Copy, &[(0, 1)], (0, 1), None),
                atom(AtomKind::Transform, &[(1, 3)], (1, 2), Some(" ")),
                atom(AtomKind::Copy, &[(3, 4)], (2, 3), None),
            ],
        }],
        &[],
    );
    let r = c
        .point(point_source(main_target(), 2), Affinity::After)
        .unwrap();
    assert_eq!(r.results()[0].chosen.value(), 2);
    assert_eq!(r.results()[0].correspondence, Correspondence::Atomic);
    let r = c
        .range(source_range(main_target(), 1, 3), Affinity::After)
        .unwrap();
    assert_eq!(related_spans(&r, selection(&r).related), vec![(1, 2)]);
    assert!(!selection(&r).expanded);
}

#[test]
fn original_empty_main_is_exact_without_contributors() {
    assert!(GOLDENS.contains("empty.source"));
    assert!(
        include_str!("../../../../tests/fixtures/m1-range/models/empty.json")
            .contains("\"fragments\": []")
    );
    let c = control(
        include_bytes!("../../../../tests/fixtures/m1-range/inputs/empty.bin"),
        &[FlowSpec {
            domains: &[(0, 0)],
            length: 0,
            atoms: &[],
        }],
        &[],
    );
    for input in [point_flow(0, 0), point_source(main_target(), 0)] {
        let r = c.point(input, Affinity::After).unwrap();
        assert_eq!(r.results()[0].chosen.value(), 0);
        assert_eq!(r.results()[0].correspondence, Correspondence::Exact);
        assert!(r.contributors().is_empty());
        assert_eq!(related_anchors(&r, r.results()[0].related), vec![0]);
    }
}

#[test]
fn original_definition_only_main_retains_full_hidden_source_permission() {
    assert!(GOLDENS.contains("definition.empty-after"));
    assert!(
        include_str!("../../../../tests/fixtures/m1-range/models/definition.json")
            .contains("ReferenceDefinition")
    );
    let c = control(
        include_bytes!("../../../../tests/fixtures/m1-range/inputs/definition.bin"),
        &[FlowSpec {
            domains: &[(0, 7)],
            length: 0,
            atoms: &[atom(AtomKind::Hidden, &[(0, 7)], (0, 0), None)],
        }],
        &[],
    );
    for (affinity, chosen) in [(Affinity::Before, 0), (Affinity::After, 7)] {
        let r = c.point(point_flow(0, 0), affinity).unwrap();
        assert_eq!(r.results()[0].chosen.value(), chosen);
        assert_eq!(r.results()[0].correspondence, Correspondence::Hidden);
        assert_eq!(related_spans(&r, r.results()[0].related), vec![(0, 7)]);
    }
    let r = c
        .range(source_range(main_target(), 0, 7), Affinity::Before)
        .unwrap();
    assert_eq!(related_anchors(&r, selection(&r).related), vec![0]);
    assert_eq!(selection(&r).start.correspondence, Correspondence::Exact);
    assert_eq!(selection(&r).end.correspondence, Correspondence::Exact);
    assert!(selection(&r).effects.hidden);
}

#[test]
fn original_empty_alt_uses_only_the_real_source_anchor() {
    assert!(GOLDENS.contains("empty-alt.source-anchor"));
    assert!(
        include_str!("../../../../tests/fixtures/m1-range/models/empty-alt.json")
            .contains("\"empty_anchor\": 2")
    );
    let c = control(
        include_bytes!("../../../../tests/fixtures/m1-range/inputs/empty-alt.bin"),
        &[
            FlowSpec {
                domains: &[(0, 6)],
                length: 1,
                atoms: &[atom(AtomKind::Object, &[(0, 6)], (0, 1), None)],
            },
            FlowSpec {
                domains: &[(2, 2)],
                length: 0,
                atoms: &[],
            },
        ],
        &[],
    );
    let r = c.point(point_flow(1, 0), Affinity::After).unwrap();
    assert_eq!(r.results()[0].chosen.value(), 2);
    assert_eq!(related_anchors(&r, r.results()[0].related), vec![2]);
    assert!(r.contributors().is_empty());
    let r = c
        .point(point_source(alt_target(), 2), Affinity::Before)
        .unwrap();
    assert_eq!(r.results()[0].chosen.value(), 0);
    assert_eq!(r.results()[0].correspondence, Correspondence::Exact);
    assert!(matches!(
        c.range(source_range(alt_target(), 2, 3), Affinity::Before),
        Err(CoreError::OutsideScope)
    ));
}

fn disjoint() -> Control {
    control(
        include_bytes!("../../../../tests/fixtures/m1-range/inputs/disjoint-alt.bin"),
        &[
            FlowSpec {
                domains: &[(0, 9)],
                length: 1,
                atoms: &[atom(AtomKind::Object, &[(0, 9)], (0, 1), None)],
            },
            FlowSpec {
                domains: &[(2, 3), (4, 5)],
                length: 2,
                atoms: &[
                    atom(AtomKind::Copy, &[(2, 3)], (0, 1), None),
                    atom(AtomKind::Copy, &[(4, 5)], (1, 2), None),
                ],
            },
        ],
        &[PayloadSpec {
            domains: &[(7, 8)],
            value: "u",
            atoms: &[atom(AtomKind::Copy, &[(7, 8)], (0, 1), None)],
        }],
    )
}

#[test]
fn original_disjoint_alt_preserves_holes_and_both_seam_anchors() {
    assert!(GOLDENS.contains("domains.source-hole"));
    assert!(
        include_str!("../../../../tests/fixtures/m1-range/models/disjoint-alt.json")
            .contains("\"readable_utf8\": \"ab\"")
    );
    let c = disjoint();
    assert!(matches!(
        c.range(source_range(alt_target(), 2, 5), Affinity::Before),
        Err(CoreError::OutsideScope)
    ));
    let r = c.range(flow_range(1, 0, 2), Affinity::Before).unwrap();
    assert_eq!(
        related_spans(&r, selection(&r).related),
        vec![(2, 3), (4, 5)]
    );
    assert_eq!(
        selection(&r).related.envelope.unwrap(),
        RelatedSpan::Source(SourceSpan::new(2, 5).unwrap())
    );
    for (affinity, chosen) in [(Affinity::Before, 3), (Affinity::After, 4)] {
        let r = c.point(point_flow(1, 1), affinity).unwrap();
        assert_eq!(r.results()[0].chosen.value(), chosen);
        assert_eq!(related_anchors(&r, r.results()[0].related), vec![3, 4]);
        assert_eq!(ids(&r), vec![1, 2]);
        assert_eq!(r.results()[0].correspondence, Correspondence::Exact);
    }
    let payload = PayloadId::from_index(0).unwrap();
    assert!(matches!(
        c.point(
            point_source(TextTarget::Payload(payload), 2),
            Affinity::Before
        ),
        Err(CoreError::OutsideScope)
    ));
    let r = c
        .point(
            PointInput::Payload {
                payload,
                utf8: PayloadBoundary::new(0),
            },
            Affinity::Before,
        )
        .unwrap();
    assert_eq!(r.results()[0].chosen.value(), 7);
}

fn multi() -> Control {
    control(
        include_bytes!("../../../../tests/fixtures/m1-range/inputs/multi-hidden-synthetic.bin"),
        &[FlowSpec {
            domains: &[(0, 6)],
            length: 3,
            atoms: &[
                atom(AtomKind::Hidden, &[(0, 2)], (0, 0), None),
                atom(AtomKind::Hidden, &[(2, 4)], (0, 0), None),
                atom(AtomKind::Synthetic, &[(4, 4)], (0, 1), Some("\t")),
                atom(AtomKind::Hidden, &[(4, 5)], (1, 1), None),
                atom(AtomKind::Synthetic, &[(5, 5)], (1, 2), Some("\n")),
                atom(AtomKind::Copy, &[(5, 6)], (2, 3), None),
            ],
        }],
        &[],
    )
}

#[test]
fn original_multi_hidden_synthetic_keeps_endpoints_out_of_content() {
    assert!(GOLDENS.contains("multi.synthetic-at-end-excluded"));
    assert!(
        include_str!("../../../../tests/fixtures/m1-range/models/multi-hidden-synthetic.json")
            .contains("ControlSeparator")
    );
    let c = multi();
    let r = c
        .range(source_range(main_target(), 0, 4), Affinity::Before)
        .unwrap();
    let result = selection(&r);
    assert_eq!(result.end.chosen.value(), 1);
    assert!(result.end.effects.synthetic);
    assert!(!result.effects.synthetic);
    assert_eq!(related_spans(&r, result.related), vec![]);
    assert_eq!(related_anchors(&r, result.related), vec![0]);
    assert_eq!(
        r.contributions()
            .iter()
            .map(|x| x.atom.index())
            .collect::<Vec<_>>(),
        vec![0, 1]
    );
    let r = c.range(flow_range(0, 0, 1), Affinity::Before).unwrap();
    let result = selection(&r);
    assert_eq!(result.start.chosen.value(), 0);
    assert_eq!(result.end.chosen.value(), 5);
    assert!(result.start.effects.hidden);
    assert!(!result.effects.hidden);
    assert_eq!(
        r.contributions()
            .iter()
            .map(|x| x.atom.index())
            .collect::<Vec<_>>(),
        vec![2]
    );
    assert_eq!(related_anchors(&r, result.related), vec![4]);
    let r = c.range(flow_range(0, 0, 3), Affinity::Before).unwrap();
    assert_eq!(related_spans(&r, selection(&r).related), vec![(4, 6)]);
    assert_eq!(related_anchors(&r, selection(&r).related), vec![4, 5]);
}

#[test]
fn original_compound_image_main_alt_and_payload_permissions_are_independent() {
    assert!(GOLDENS.contains("compound-image.alt-cross-entity"));
    assert!(
        include_str!("../../../../tests/fixtures/m1-range/models/compound-image.json")
            .contains("\"logical_utf16\": 6")
    );
    let c = control(
        include_bytes!("../../../../tests/fixtures/m1-range/inputs/compound-image.bin"),
        &[
            FlowSpec {
                domains: &[(0, 34)],
                length: 5,
                atoms: &[
                    atom(AtomKind::Copy, &[(0, 2)], (0, 2), None),
                    atom(AtomKind::Object, &[(2, 32)], (2, 3), None),
                    atom(AtomKind::Copy, &[(32, 34)], (3, 5), None),
                ],
            },
            FlowSpec {
                domains: &[(4, 24)],
                length: 6,
                atoms: &[
                    atom(AtomKind::Hidden, &[(4, 6)], (0, 0), None),
                    atom(AtomKind::Copy, &[(6, 9)], (0, 1), None),
                    atom(AtomKind::Hidden, &[(9, 11)], (1, 1), None),
                    atom(AtomKind::Copy, &[(11, 12)], (1, 2), None),
                    atom(AtomKind::Transform, &[(12, 17)], (2, 3), Some("&")),
                    atom(AtomKind::Copy, &[(17, 18)], (3, 4), None),
                    atom(AtomKind::Hidden, &[(18, 19)], (4, 4), None),
                    atom(AtomKind::Copy, &[(19, 23)], (4, 6), None),
                    atom(AtomKind::Hidden, &[(23, 24)], (6, 6), None),
                ],
            },
        ],
        &[
            PayloadSpec {
                domains: &[(26, 27)],
                value: "u",
                atoms: &[atom(AtomKind::Copy, &[(26, 27)], (0, 1), None)],
            },
            PayloadSpec {
                domains: &[(29, 30)],
                value: "t",
                atoms: &[atom(AtomKind::Copy, &[(29, 30)], (0, 1), None)],
            },
        ],
    );
    let r = c.range(flow_range(0, 1, 4), Affinity::Before).unwrap();
    let result = selection(&r);
    assert_eq!(
        (result.start.chosen.value(), result.end.chosen.value()),
        (1, 33)
    );
    assert_eq!(related_spans(&r, result.related), vec![(1, 33)]);
    assert_eq!(
        r.contributions()
            .iter()
            .map(|x| x.atom.index())
            .collect::<Vec<_>>(),
        vec![0, 1, 2]
    );
    assert!(result.effects.object);
    assert!(!result.expanded);
    let r = c.range(flow_range(1, 1, 4), Affinity::Before).unwrap();
    let result = selection(&r);
    assert_eq!(
        (result.start.chosen.value(), result.end.chosen.value()),
        (9, 19)
    );
    assert_eq!(related_spans(&r, result.related), vec![(11, 18)]);
    assert_eq!(
        r.contributions()
            .iter()
            .map(|x| x.atom.index())
            .collect::<Vec<_>>(),
        vec![6, 7, 8]
    );
    assert!(result.start.effects.hidden && result.end.effects.hidden);
    assert!(!result.effects.hidden);
    let payload = PayloadId::from_index(0).unwrap();
    assert!(matches!(
        c.point(point_source(alt_target(), 26), Affinity::Before),
        Err(CoreError::OutsideScope)
    ));
    assert!(matches!(
        c.point(
            point_source(TextTarget::Payload(payload), 29),
            Affinity::Before
        ),
        Err(CoreError::OutsideScope)
    ));
    assert!(matches!(
        c.point(point_flow(1, 5), Affinity::Before),
        Err(CoreError::InvalidBoundary)
    ));
}

#[test]
fn original_compound_table_retains_every_fixed_separator_and_range_effect() {
    assert!(GOLDENS.contains("table.source-all-kinds"));
    assert!(
        include_str!("../../../../tests/fixtures/m1-range/models/compound-table.json")
            .contains("\"logical_utf16\": 12")
    );
    let c = control(
        include_bytes!("../../../../tests/fixtures/m1-range/inputs/compound-table.bin"),
        &[
            FlowSpec {
                domains: &[(0, 41)],
                length: 12,
                atoms: &[
                    atom(AtomKind::Hidden, &[(0, 1)], (0, 0), None),
                    atom(AtomKind::Hidden, &[(1, 2)], (0, 0), None),
                    atom(AtomKind::Copy, &[(2, 3)], (0, 1), None),
                    atom(AtomKind::Hidden, &[(3, 4)], (1, 1), None),
                    atom(AtomKind::Hidden, &[(4, 5)], (1, 1), None),
                    atom(AtomKind::Synthetic, &[(5, 5)], (1, 2), Some("\t")),
                    atom(AtomKind::Copy, &[(5, 6)], (2, 3), None),
                    atom(AtomKind::Hidden, &[(6, 17)], (3, 3), None),
                    atom(AtomKind::Synthetic, &[(17, 17)], (3, 4), Some("\n")),
                    atom(AtomKind::Hidden, &[(17, 18)], (4, 4), None),
                    atom(AtomKind::Copy, &[(18, 19)], (4, 5), None),
                    atom(AtomKind::Transform, &[(19, 21)], (5, 6), Some("|")),
                    atom(AtomKind::Copy, &[(21, 22)], (6, 7), None),
                    atom(AtomKind::Hidden, &[(22, 23)], (7, 7), None),
                    atom(AtomKind::Hidden, &[(23, 24)], (7, 7), None),
                    atom(AtomKind::Synthetic, &[(24, 24)], (7, 8), Some("\t")),
                    atom(AtomKind::Object, &[(24, 33)], (8, 9), None),
                    atom(AtomKind::Hidden, &[(33, 38)], (9, 9), None),
                    atom(AtomKind::Synthetic, &[(38, 38)], (9, 10), Some("\n")),
                    atom(AtomKind::Copy, &[(38, 39)], (10, 11), None),
                    atom(AtomKind::Hidden, &[(39, 40)], (11, 11), None),
                    atom(AtomKind::Synthetic, &[(40, 40)], (11, 12), Some("\t")),
                    atom(AtomKind::Hidden, &[(40, 41)], (12, 12), None),
                ],
            },
            FlowSpec {
                domains: &[(26, 29)],
                length: 1,
                atoms: &[atom(AtomKind::Copy, &[(26, 29)], (0, 1), None)],
            },
        ],
        &[PayloadSpec {
            domains: &[(31, 32)],
            value: "u",
            atoms: &[atom(AtomKind::Copy, &[(31, 32)], (0, 1), None)],
        }],
    );
    assert_eq!(
        c.mapping
            .synthetic
            .as_slice()
            .iter()
            .map(|x| x.span.start())
            .collect::<Vec<_>>(),
        vec![5, 17, 24, 38, 40]
    );
    let source = c
        .range(source_range(main_target(), 18, 33), Affinity::Before)
        .unwrap();
    let result = selection(&source);
    assert_eq!(
        (result.start.chosen.value(), result.end.chosen.value()),
        (4, 9)
    );
    assert_eq!(related_spans(&source, result.related), vec![(4, 9)]);
    assert_eq!(related_anchors(&source, result.related), vec![7]);
    assert_eq!(
        source
            .contributions()
            .iter()
            .map(|x| x.atom.index())
            .collect::<Vec<_>>(),
        vec![10, 11, 12, 13, 14, 15, 16]
    );
    assert_eq!(
        result.effects,
        Effects {
            hidden: true,
            synthetic: true,
            atomic: true,
            object: true
        }
    );
    assert!(!result.expanded);
    let target = c.range(flow_range(0, 4, 9), Affinity::Before).unwrap();
    let result = selection(&target);
    assert_eq!(
        (result.start.chosen.value(), result.end.chosen.value()),
        (17, 38)
    );
    assert_eq!(related_spans(&target, result.related), vec![(18, 33)]);
    assert_eq!(related_anchors(&target, result.related), vec![24]);
    assert_eq!(
        target
            .contributions()
            .iter()
            .map(|x| x.atom.index())
            .collect::<Vec<_>>(),
        vec![10, 11, 12, 13, 14, 15, 16]
    );
}

#[test]
fn multi_source_atomic_closure_keeps_full_points_and_clips_range_input() {
    let c = control(
        b"abcde",
        &[FlowSpec {
            domains: &[(0, 5)],
            length: 2,
            atoms: &[
                atom(AtomKind::Hidden, &[(1, 3)], (0, 0), None),
                atom(AtomKind::Transform, &[(0, 1), (3, 5)], (0, 2), Some("xy")),
            ],
        }],
        &[],
    );
    let r = c.point(point_flow(0, 1), Affinity::After).unwrap();
    assert_eq!(r.results()[0].chosen.value(), 5);
    assert_eq!(
        related_spans(&r, r.results()[0].related),
        vec![(0, 1), (3, 5)]
    );
    assert_eq!(
        r.contributors()
            .iter()
            .map(|x| x.source_part)
            .collect::<Vec<_>>(),
        vec![0, 1]
    );
    let r = c
        .range(source_range(main_target(), 3, 4), Affinity::Before)
        .unwrap();
    assert_eq!(r.contributions().len(), 1);
    assert_eq!(r.contributions()[0].source_part, 1);
    assert_eq!(r.contributions()[0].source, SourceSpan::new(3, 4).unwrap());
    assert_eq!(r.contributions()[0].target, TargetSpan { start: 0, end: 2 });
    assert_eq!(related_spans(&r, selection(&r).related), vec![(0, 2)]);
    assert!(selection(&r).expanded);
    let target_half = c.range(flow_range(0, 0, 1), Affinity::Before).unwrap();
    assert_eq!(target_half.contributions().len(), 2);
    for (part, index, source) in [
        (&target_half.contributions()[0], 0, (0, 1)),
        (&target_half.contributions()[1], 1, (3, 5)),
    ] {
        assert_eq!(part.source_part, index);
        assert_eq!(part.source, SourceSpan::new(source.0, source.1).unwrap());
        assert_eq!(part.target, TargetSpan { start: 0, end: 1 });
        assert!(part.expanded);
    }
    assert_eq!(
        related_spans(&target_half, selection(&target_half).related),
        vec![(0, 1), (3, 5)]
    );
    let hidden_only = c
        .range(source_range(main_target(), 1, 3), Affinity::Before)
        .unwrap();
    assert_eq!(hidden_only.contributions().len(), 1);
    assert_eq!(hidden_only.contributions()[0].kind, AtomKind::Hidden);
    assert!(selection(&hidden_only).effects.hidden);
    assert!(!selection(&hidden_only).effects.atomic);
    let r = c
        .point(point_source(main_target(), 3), Affinity::Before)
        .unwrap();
    assert!(r.results()[0].effects.atomic);
}

#[test]
fn sparse_utf8_utf16_scans_are_bounded_and_surrogates_are_rejected() {
    let text = format!(
        "{}😀{}中{}",
        "a".repeat(1023),
        "b".repeat(1023),
        "c".repeat(1024)
    );
    let c = control(
        text.as_bytes(),
        &[FlowSpec {
            domains: &[(0, text.len() as u64)],
            length: 3073,
            atoms: &[atom(
                AtomKind::Copy,
                &[(0, text.len() as u64)],
                (0, 3073),
                None,
            )],
        }],
        &[],
    );
    let mut op = c.budget.begin(CancellationToken::new());
    let r = c
        .mapping
        .map_point(
            &c.ir,
            &c.source,
            &PointQuery {
                model: c.identity,
                input: point_flow(0, 2048),
                affinity: Affinity::After,
            },
            &mut op,
        )
        .unwrap();
    assert_eq!(r.results()[0].chosen.value(), 2050);
    let view = (size_of::<crate::ir::AtomFields>()
        + size_of::<crate::ir::ClaimId>()
        + size_of::<std::mem::Discriminant<crate::ir::AuthorityBinding>>()) as u64;
    assert!(op.stats().expect("statistics enabled").scanned_bytes() <= 2056 + 4 * view); // Two bounded UTF8 windows and four fixed views.
    assert!(matches!(
        c.point(point_flow(0, 1024), Affinity::Before),
        Err(CoreError::InvalidBoundary)
    ));
}

#[test]
fn pooled_text_queries_scan_only_the_nearest_sparse_window() {
    // Deliberately oversized manual Transform stresses pool lookup and sparse
    // conversion. It has no minimal proof or accepted/native model authority.
    let text = format!(
        "{}😀{}中{}",
        "a".repeat(1023),
        "b".repeat(1023),
        "c".repeat(131072)
    );
    let logical = text.encode_utf16().count() as u64;
    let c = control(
        b"xy",
        &[FlowSpec {
            domains: &[(0, 2)],
            length: logical + 1,
            atoms: &[
                atom(AtomKind::Copy, &[(0, 1)], (0, 1), Some("x")),
                atom(
                    AtomKind::Transform,
                    &[(1, 2)],
                    (1, logical + 1),
                    Some(&text),
                ),
            ],
        }],
        &[],
    );
    let mut op = c.budget.begin(CancellationToken::new());
    let r = c
        .mapping
        .map_point(
            &c.ir,
            &c.source,
            &PointQuery {
                model: c.identity,
                input: point_flow(0, 2049),
                affinity: Affinity::After,
            },
            &mut op,
        )
        .unwrap();
    assert_eq!(r.results()[0].chosen.value(), 2);
    assert!(r.results()[0].effects.atomic);
    let view = (size_of::<crate::ir::AtomFields>()
        + size_of::<crate::ir::ClaimId>()
        + size_of::<std::mem::Discriminant<crate::ir::AuthorityBinding>>()) as u64;
    let mut segments = 0;
    c.ir.text.for_each_segment(|_| segments += 1);
    assert_eq!(segments, 1);
    // The final representation now has a real directory: the single-segment
    // binary search reads one descriptor and reuses that immutable borrow.
    // Preserve the original text/sparse-window + atom-view bound unchanged.
    let descriptor = c.ir.text.directory_record_bytes() as u64;
    assert!(descriptor != 0);
    let directory = descriptor;
    let actual = op.stats().expect("statistics enabled").scanned_bytes();
    assert!(actual >= directory);
    assert!(
        actual - directory <= 1031 + 3 * view,
        "actual={actual} directory={directory} original_bound={}",
        1031 + 3 * view
    );
    assert_eq!(related_spans(&r, r.results()[0].related), vec![(1, 2)]);
    assert!(matches!(
        c.point(point_flow(0, 1025), Affinity::After),
        Err(CoreError::InvalidBoundary)
    ));
}

#[test]
fn short_copy_transform_and_synthetic_atoms_use_virtual_endpoints_and_scalar_boundaries() {
    // Geometry-only drafts: this deliberately does not assert transform proof
    // acceptance or native grammar coverage.
    let text = "中😀a\u{301}";
    let length = text.encode_utf16().count() as u64;
    for kind in [AtomKind::Copy, AtomKind::Transform, AtomKind::Synthetic] {
        let source = if kind == AtomKind::Copy {
            text.as_bytes()
        } else {
            b"z"
        };
        let source_parts = if kind == AtomKind::Synthetic {
            vec![(0, 0)]
        } else {
            vec![(0, source.len() as u64)]
        };
        let c = control(
            source,
            &[FlowSpec {
                domains: &[(0, source.len() as u64)],
                length,
                atoms: &[atom(
                    kind,
                    &source_parts,
                    (0, length),
                    if kind == AtomKind::Copy {
                        None
                    } else {
                        Some(text)
                    },
                )],
            }],
            &[],
        );
        assert_eq!(c.mapping.checkpoint_count(), 0);
        let id = AtomId::from_index(0).unwrap();
        for byte in 0..=text.len() {
            let mut op = c.budget.begin(CancellationToken::new());
            let result = c
                .mapping
                .byte_to_utf16(&c.ir, &c.source, id, byte as u64, &mut op);
            if let Some(prefix) = text.get(..byte) {
                let utf16 = prefix.encode_utf16().count() as u64;
                assert_eq!(result, Ok(utf16));
                assert_eq!(
                    c.mapping
                        .utf16_to_byte(&c.ir, &c.source, id, utf16, &mut op),
                    Ok(byte as u64)
                );
            } else {
                assert_eq!(result, Err(CoreError::InvalidBoundary));
            }
        }
        for position in [2, length + 1] {
            let mut op = c.budget.begin(CancellationToken::new());
            assert_eq!(
                c.mapping
                    .utf16_to_byte(&c.ir, &c.source, id, position, &mut op),
                Err(CoreError::InvalidBoundary)
            );
        }
        let budget = c.budget.clone();
        drop(c);
        assert_eq!(budget.usage().auxiliary_capacity, 0);
        assert_eq!(budget.usage().source_capacity, 0);
    }
}

#[test]
fn sparse_interiors_distinguish_1023_1024_1027_and_astral_endpoints() {
    for (text, interiors) in [
        ("a".repeat(1023), 0),
        ("a".repeat(1024), 0),
        ("a".repeat(1027), 1),
        (format!("{}😀", "a".repeat(1023)), 0),
        (format!("{}😀中e\u{301}", "a".repeat(1023)), 1),
    ] {
        let length = text.encode_utf16().count() as u64;
        let c = control(
            text.as_bytes(),
            &[FlowSpec {
                domains: &[(0, text.len() as u64)],
                length,
                atoms: &[atom(
                    AtomKind::Copy,
                    &[(0, text.len() as u64)],
                    (0, length),
                    None,
                )],
            }],
            &[],
        );
        assert_eq!(c.mapping.checkpoint_count(), interiors);
        let id = AtomId::from_index(0).unwrap();
        for byte in [0, 1023, 1024, 1027, text.len()] {
            if byte > text.len() {
                continue;
            }
            let mut op = c.budget.begin(CancellationToken::new());
            let result = c
                .mapping
                .byte_to_utf16(&c.ir, &c.source, id, byte as u64, &mut op);
            if let Some(prefix) = text.get(..byte) {
                let utf16 = prefix.encode_utf16().count() as u64;
                assert_eq!(result, Ok(utf16));
                assert_eq!(
                    c.mapping
                        .utf16_to_byte(&c.ir, &c.source, id, utf16, &mut op),
                    Ok(byte as u64)
                );
            } else {
                assert_eq!(result, Err(CoreError::InvalidBoundary));
            }
        }
        if text.contains('😀') {
            let mut op = c.budget.begin(CancellationToken::new());
            assert_eq!(
                c.mapping.utf16_to_byte(&c.ir, &c.source, id, 1024, &mut op),
                Err(CoreError::InvalidBoundary)
            );
        }
    }
}

#[test]
fn checkpoint_storage_allocates_only_for_real_interiors() {
    use super::index::{Checkpoint, Posting, TargetIndex, TargetPosting};

    for (length, checkpoints) in [(1, 0), (1024, 0), (1025, 1), (3073, 3)] {
        let text = "x".repeat(length);
        let c = control(
            text.as_bytes(),
            &[FlowSpec {
                domains: &[(0, length as u64)],
                length: length as u64,
                atoms: &[atom(
                    AtomKind::Copy,
                    &[(0, length as u64)],
                    (0, length as u64),
                    None,
                )],
            }],
            &[],
        );
        let baseline = c.budget.usage().auxiliary_capacity;
        let mut op = c.budget.begin(CancellationToken::new());
        let index = MapIndex::build(&c.ir, &c.source, c.identity, &mut op).unwrap();
        assert_eq!(index.checkpoint_count(), checkpoints);
        let fixed = (size_of::<TargetIndex>()
            + size_of::<Posting>()
            + size_of::<TargetPosting>()
            + size_of::<Rows<Checkpoint>>()) as u64;
        let allocated = c.budget.usage().auxiliary_capacity - baseline;
        if checkpoints == 0 {
            assert_eq!(
                allocated, fixed,
                "virtual endpoints must not allocate checkpoints"
            );
        } else {
            assert!(allocated >= fixed + (checkpoints * size_of::<Checkpoint>()) as u64);
        }
        drop(index);
        assert_eq!(c.budget.usage().auxiliary_capacity, baseline);
        let budget = c.budget.clone();
        drop(c);
        assert_eq!(budget.usage().auxiliary_capacity, 0);
        assert_eq!(budget.usage().source_capacity, 0);
        assert_eq!(budget.usage().source_owners, 0);
    }
}

#[test]
fn checkpoint_quota_includes_all_seven_tables_and_prior_operation_rows() {
    let budget = SourceBudget::new_with_model_limits(
        SourceLimits::default(),
        ModelLimits {
            index_records: 16,
            ..ModelLimits::default()
        },
    )
    .unwrap();
    let text = "a".repeat(2050);
    let synthetic = "b".repeat(2049);
    let c = control_with_budget(
        text.as_bytes(),
        &[FlowSpec {
            domains: &[(0, 2050)],
            length: 4098,
            atoms: &[
                atom(AtomKind::Copy, &[(0, 2049)], (0, 2049), None),
                atom(AtomKind::Hidden, &[(2049, 2050)], (2049, 2049), None),
                atom(
                    AtomKind::Synthetic,
                    &[(2050, 2050)],
                    (2049, 4098),
                    Some(&synthetic),
                ),
            ],
        }],
        &[],
        budget.clone(),
    );
    // One target directory, two source postings, one synthetic posting, two
    // positive targets, one hidden target, three sparse directories and four
    // real checkpoints: 14 entities, including every one of the seven tables.
    assert_eq!(c.mapping.checkpoint_count(), 4);
    let baseline = budget.usage().auxiliary_capacity;
    let mut exact = budget.begin(CancellationToken::new());
    exact.admit_model(LimitKind::IndexRecords, 2).unwrap();
    let index = MapIndex::build(&c.ir, &c.source, c.identity, &mut exact).unwrap();
    assert_eq!(index.checkpoint_count(), 4);
    let expected = CoreError::ResourceLimit {
        kind: LimitKind::IndexRecords,
        requested: 17,
        limit: 16,
    };
    assert_eq!(exact.admit_model(LimitKind::IndexRecords, 1), Err(expected));
    drop(index);
    assert_eq!(budget.usage().auxiliary_capacity, baseline);

    for prior in [3, 4] {
        let cancel = CancellationToken::new();
        let mut op = budget.begin(cancel.clone());
        op.admit_model(LimitKind::IndexRecords, prior).unwrap();
        // prior=3 fails on the last directory; prior=4 fails generating the
        // second synthetic checkpoint, after earlier checkpoint growth.
        assert!(
            matches!(MapIndex::build(&c.ir, &c.source, c.identity, &mut op), Err(e) if e == expected)
        );
        assert_eq!(op.first_fault(), Some(expected));
        cancel.cancel();
        assert_eq!(op.checkpoint(), Err(expected));
        assert_eq!(budget.usage().auxiliary_capacity, baseline);
    }
    drop(c);
    assert_eq!(budget.usage().auxiliary_capacity, 0);
    assert_eq!(budget.usage().source_capacity, 0);
    assert_eq!(budget.usage().source_owners, 0);
}

#[test]
fn checkpoint_rows_survive_growth_between_atoms() {
    let text = format!(
        "{}{}{}",
        "a".repeat(1025),
        "b".repeat(4097),
        "c".repeat(2049)
    );
    let c = control(
        text.as_bytes(),
        &[FlowSpec {
            domains: &[(0, 7171)],
            length: 7171,
            atoms: &[
                atom(AtomKind::Copy, &[(0, 1025)], (0, 1025), None),
                atom(AtomKind::Copy, &[(1025, 5122)], (1025, 5122), None),
                atom(AtomKind::Copy, &[(5122, 7171)], (5122, 7171), None),
            ],
        }],
        &[],
    );
    assert_eq!(c.mapping.checkpoint_count(), 7);
    let mut op = c.budget.begin(CancellationToken::new());
    for (atom_index, length) in [(0, 1025), (1, 4097), (2, 2049)] {
        let id = AtomId::from_index(atom_index).unwrap();
        for offset in [0, 1023, 1024, 1025, length - 1, length] {
            assert_eq!(
                c.mapping
                    .byte_to_utf16(&c.ir, &c.source, id, offset, &mut op),
                Ok(offset)
            );
            assert_eq!(
                c.mapping
                    .utf16_to_byte(&c.ir, &c.source, id, offset, &mut op),
                Ok(offset)
            );
        }
    }
    let budget = c.budget.clone();
    drop(c);
    assert_eq!(budget.usage().auxiliary_capacity, 0);
    assert_eq!(budget.usage().source_capacity, 0);
    assert_eq!(budget.usage().source_owners, 0);
}

#[test]
fn checkpoint_growth_auxiliary_failure_releases_draft_and_keeps_first_fault() {
    use super::index::{Checkpoint, Posting, TargetPosting};

    let text = "x".repeat(3073);
    let c = control(
        text.as_bytes(),
        &[FlowSpec {
            domains: &[(0, 3073)],
            length: 3073,
            atoms: &[atom(AtomKind::Copy, &[(0, 3073)], (0, 3073), None)],
        }],
        &[],
    );
    let budget = c.budget.clone();
    let baseline = budget.usage().auxiliary_capacity;
    // Targets are allocated at the end; while generating checkpoint 3 only
    // these three fixed buffers plus old(2)+new(4) checkpoint slots are live.
    let fixed = size_of::<Posting>() + size_of::<TargetPosting>() + size_of::<Rows<Checkpoint>>();
    let overlap = (fixed + 6 * size_of::<Checkpoint>()) as u64;
    let blocker = budget
        .reserve_auxiliary(budget.limits().auxiliary_capacity - baseline - overlap + 1)
        .unwrap();
    let held = budget.usage().auxiliary_capacity;
    let cancel = CancellationToken::new();
    let mut op = budget.begin(cancel.clone());
    let error = match MapIndex::build(&c.ir, &c.source, c.identity, &mut op) {
        Err(error) => error,
        Ok(_) => panic!("growth must admit the full new allocation while old is live"),
    };
    assert_eq!(
        error,
        CoreError::ResourceLimit {
            kind: LimitKind::AuxiliaryCapacity,
            requested: budget.limits().auxiliary_capacity + 1,
            limit: budget.limits().auxiliary_capacity,
        }
    );
    // The failed growth is reached after scanning the third 1024B segment,
    // rather than failing early before any initialized checkpoints exist.
    assert!(
        op.stats().expect("statistics enabled").scanned_bytes()
            - op.stats()
                .expect("statistics enabled")
                .metadata_scanned_bytes()
            >= 3072
    );
    assert_eq!(budget.usage().auxiliary_capacity, held);
    assert_eq!(op.first_fault(), Some(error));
    cancel.cancel();
    assert_eq!(op.checkpoint(), Err(error));
    drop(blocker);
    assert_eq!(budget.usage().auxiliary_capacity, baseline);
    drop(c);
    assert_eq!(budget.usage().auxiliary_capacity, 0);
    assert_eq!(budget.usage().source_capacity, 0);
    assert_eq!(budget.usage().source_owners, 0);
}

#[test]
fn identities_ids_domains_and_first_fault_have_contract_precedence() {
    let c = strong();
    let invalid_id = FlowId::from_index(99).unwrap();
    let invalid_input = PointInput::Flow {
        flow: invalid_id,
        utf16: LogicalBoundary::new(999),
    };
    let revision =
        ModelIdentity::new(SourceKey::new([4; 16], 2).unwrap(), 1, c.identity.config()).unwrap();
    let generation = ModelIdentity::new(c.identity.source(), 2, c.identity.config()).unwrap();
    let config = ModelIdentity::new(c.identity.source(), 1, ParseConfig::new(Profile::G3)).unwrap();
    let document =
        ModelIdentity::new(SourceKey::new([5; 16], 1).unwrap(), 1, c.identity.config()).unwrap();
    for (identity, expected) in [
        (document, CoreError::WrongDocument),
        (
            revision,
            CoreError::WrongRevision {
                expected: 1,
                actual: 2,
            },
        ),
        (
            generation,
            CoreError::WrongGeneration {
                expected: 1,
                actual: 2,
            },
        ),
        (config, CoreError::UnsupportedConfiguration),
        (c.identity, CoreError::UnknownId),
    ] {
        let cancel = CancellationToken::new();
        let mut op = c.budget.begin(cancel.clone());
        let query = PointQuery {
            model: identity,
            input: invalid_input,
            affinity: Affinity::Before,
        };
        assert!(
            matches!(c.mapping.map_point(&c.ir, &c.source, &query, &mut op), Err(error) if error == expected)
        );
        cancel.cancel();
        assert!(
            matches!(c.mapping.map_point(&c.ir, &c.source, &query, &mut op), Err(error) if error == expected)
        );
    }
    let cancel = CancellationToken::new();
    cancel.cancel();
    let mut op = c.budget.begin(cancel);
    assert!(matches!(
        c.mapping.map_point(
            &c.ir,
            &c.source,
            &PointQuery {
                model: revision,
                input: invalid_input,
                affinity: Affinity::Before
            },
            &mut op
        ),
        Err(CoreError::Cancelled)
    ));
    let other = SourceBudget::model_fixture();
    let mut op = other.begin(CancellationToken::new());
    assert!(matches!(
        c.mapping.map_point(
            &c.ir,
            &c.source,
            &PointQuery {
                model: revision,
                input: invalid_input,
                affinity: Affinity::Before
            },
            &mut op
        ),
        Err(CoreError::WrongBudget)
    ));
}

#[test]
fn failed_batch_is_atomic_and_all_temporary_capacity_is_released() {
    let c = strong();
    let before = c.budget.usage().auxiliary_capacity;
    let mut op = c.budget.begin(CancellationToken::new());
    let queries = [
        PointQuery {
            model: c.identity,
            input: point_flow(0, 1),
            affinity: Affinity::Before,
        },
        PointQuery {
            model: c.identity,
            input: point_flow(0, 3),
            affinity: Affinity::Before,
        },
    ];
    assert!(matches!(
        c.mapping.map_points(&c.ir, &c.source, &queries, &mut op),
        Err(CoreError::InvalidBoundary)
    ));
    assert_eq!(c.budget.usage().auxiliary_capacity, before);
    let mut op = c.budget.begin(CancellationToken::new());
    let good = c
        .mapping
        .map_points(&c.ir, &c.source, &queries[..1], &mut op)
        .unwrap();
    assert_eq!(good.results().len(), 1);
    assert!(c.budget.usage().auxiliary_capacity > before);
    drop(good);
    assert_eq!(c.budget.usage().auxiliary_capacity, before);
}

#[test]
fn query_row_capacity_and_request_quotas_are_typed_and_release_every_lease() {
    for (limits, expected) in [
        (
            ModelLimits {
                query_records: 2,
                ..ModelLimits::default()
            },
            LimitKind::QueryRecords,
        ),
        (
            ModelLimits {
                query_capacity: 200,
                ..ModelLimits::default()
            },
            LimitKind::QueryCapacity,
        ),
        (
            ModelLimits {
                query_requests: 0,
                ..ModelLimits::default()
            },
            LimitKind::QueryRequests,
        ),
    ] {
        let budget = SourceBudget::new_with_model_limits(SourceLimits::default(), limits).unwrap();
        let c = control_with_budget(
            b"x",
            &[FlowSpec {
                domains: &[(0, 1)],
                length: 1,
                atoms: &[atom(AtomKind::Copy, &[(0, 1)], (0, 1), None)],
            }],
            &[],
            budget,
        );
        let before = c.budget.usage().auxiliary_capacity;
        assert!(
            matches!(c.point(point_flow(0, 0), Affinity::Before), Err(CoreError::ResourceLimit { kind, .. }) if kind == expected)
        );
        assert_eq!(c.budget.usage().auxiliary_capacity, before);
    }
}

#[test]
fn retained_response_outlives_model_and_drop_ignores_cancellation() {
    let c = multi();
    let budget = c.budget.clone();
    let cancel = CancellationToken::new();
    let mut op = budget.begin(cancel.clone());
    let response = c
        .mapping
        .map_range(
            &c.ir,
            &c.source,
            &RangeQuery {
                model: c.identity,
                input: flow_range(0, 0, 3),
                empty_affinity: Affinity::After,
            },
            &mut op,
        )
        .unwrap();
    drop(c);
    assert_eq!(budget.usage().source_owners, 0);
    assert_eq!(
        related_spans(&response, selection(&response).related),
        vec![(4, 6)]
    );
    cancel.cancel();
    drop(response);
    assert_eq!(budget.usage().auxiliary_capacity, 0);
}

#[test]
fn immutable_mapping_and_response_are_send_sync() {
    fn assert_send_sync<T: Send + Sync>() {}
    assert_send_sync::<MapIndex>();
    assert_send_sync::<PointResponse>();
    assert_send_sync::<RangeResponse>();
}

#[test]
fn query_growth_admits_old_and_new_together_before_allocating() {
    let metadata = (size_of::<QueryScope>() + 2 * size_of::<usize>()) as u64;
    let budget = SourceBudget::new_with_model_limits(
        SourceLimits::default(),
        ModelLimits {
            query_capacity: metadata + 40,
            ..ModelLimits::default()
        },
    )
    .unwrap();
    let mut op = budget.begin(CancellationToken::new());
    let scope = QueryScope::new(&mut op).unwrap();
    let mut rows = QueryVec::new(&scope);
    rows.push(1u64, &mut op).unwrap();
    rows.push(2u64, &mut op).unwrap();
    assert!(
        matches!(rows.push(3u64, &mut op), Err(CoreError::ResourceLimit {
        kind: LimitKind::QueryCapacity, requested, limit
    }) if requested == metadata + 48 && limit == metadata + 40)
    );
    assert_eq!(rows.as_slice(), &[1, 2]);
    assert_eq!(budget.usage().auxiliary_capacity, metadata + 16);
    drop(rows);
    drop(scope);
    assert_eq!(budget.usage().auxiliary_capacity, 0);
}

#[test]
fn sorting_observes_the_same_flag_during_comparisons_and_releases_scratch() {
    use std::cell::Cell;
    let budget = SourceBudget::model_fixture();
    let cancel = CancellationToken::new();
    let mut op = budget.begin(cancel.clone());
    let scope = QueryScope::new(&mut op).unwrap();
    let mut rows = QueryVec::new(&scope);
    for value in (0u64..512).rev() {
        rows.push(value, &mut op).unwrap();
    }
    let capacity = budget.usage().auxiliary_capacity;
    let calls = Cell::new(0u64);
    assert_eq!(
        storage::sort(
            &mut rows,
            0,
            |a, b| {
                calls.set(calls.get() + 1);
                if calls.get() == 100 {
                    cancel.cancel();
                }
                a.cmp(&b)
            },
            &mut op
        ),
        Err(CoreError::Cancelled)
    );
    assert_eq!(calls.get(), 100);
    assert_eq!(budget.usage().auxiliary_capacity, capacity);
    drop(rows);
    drop(scope);
    assert_eq!(budget.usage().auxiliary_capacity, 0);
}

#[test]
fn live_query_cancellation_releases_draft_response_but_keeps_retained_index() {
    let input = vec![b'x'; 16_384];
    let parts: Vec<_> = (0u64..16_384).map(|i| [(i, i + 1)]).collect();
    let atoms: Vec<_> = parts
        .iter()
        .map(|piece| atom(AtomKind::Hidden, piece, (0, 0), None))
        .collect();
    let c = control(
        &input,
        &[FlowSpec {
            domains: &[(0, 16_384)],
            length: 0,
            atoms: &atoms,
        }],
        &[],
    );
    let baseline = c.budget.usage().auxiliary_capacity;
    let cancel = CancellationToken::new();
    let mut op = c.budget.begin(cancel.clone());
    let query = PointQuery {
        model: c.identity,
        input: point_flow(0, 0),
        affinity: Affinity::Before,
    };
    let result = c.mapping.map_points_inner(
        &c.ir,
        &c.source,
        &[query],
        None,
        &mut op,
        Some(&|| {
            // The response is still live. Wait for a different thread to
            // cancel before the query can reach its publication checkpoint.
            assert!(c.budget.usage().auxiliary_capacity > baseline);
            std::thread::scope(|scope| scope.spawn(|| cancel.cancel()).join().unwrap());
        }),
    );
    assert_eq!(result.err(), Some(CoreError::Cancelled));
    assert!(op.stats().expect("statistics enabled").events() > 0);
    assert_eq!(c.budget.usage().auxiliary_capacity, baseline);
    assert!(c.point(point_flow(0, 0), Affinity::After).is_ok());
}

#[test]
fn payload_utf8_domain_remains_separate_from_flow_utf16() {
    let c = control(
        "中😀".as_bytes(),
        &[FlowSpec {
            domains: &[(0, 7)],
            length: 3,
            atoms: &[atom(AtomKind::Copy, &[(0, 7)], (0, 3), None)],
        }],
        &[
            PayloadSpec {
                domains: &[(0, 7)],
                value: "中😀",
                atoms: &[atom(AtomKind::Copy, &[(0, 7)], (0, 7), None)],
            },
            PayloadSpec {
                domains: &[(0, 7)],
                value: "中😀",
                atoms: &[atom(AtomKind::Copy, &[(0, 7)], (0, 7), None)],
            },
        ],
    );
    let payload = PayloadId::from_index(1).unwrap();
    assert_eq!(c.mapping.checkpoint_count(), 0); // Neither short Flow nor UTF8 payload stores endpoints.
    let r = c
        .point(
            PointInput::Payload {
                payload,
                utf8: PayloadBoundary::new(3),
            },
            Affinity::Before,
        )
        .unwrap();
    assert_eq!(r.results()[0].chosen.value(), 3);
    let r = c.point(point_flow(0, 1), Affinity::Before).unwrap();
    assert_eq!(r.results()[0].chosen.value(), 3);
    assert!(matches!(
        c.point(
            PointInput::Payload {
                payload,
                utf8: PayloadBoundary::new(1)
            },
            Affinity::Before
        ),
        Err(CoreError::InvalidBoundary)
    ));
    assert!(matches!(
        c.point(point_flow(0, 2), Affinity::Before),
        Err(CoreError::InvalidBoundary)
    ));
    let unknown = PayloadId::from_index(99).unwrap();
    assert!(matches!(
        c.point(
            PointInput::Payload {
                payload: unknown,
                utf8: PayloadBoundary::new(0)
            },
            Affinity::Before
        ),
        Err(CoreError::UnknownId)
    ));
}

#[test]
fn source_range_selecting_a_later_hidden_piece_keeps_its_target_anchor() {
    let c = control(
        b"abcd",
        &[FlowSpec {
            domains: &[(0, 4)],
            length: 2,
            atoms: &[
                atom(AtomKind::Hidden, &[(0, 1), (3, 4)], (0, 0), None),
                atom(AtomKind::Copy, &[(1, 3)], (0, 2), None),
            ],
        }],
        &[],
    );
    let r = c
        .range(source_range(main_target(), 3, 4), Affinity::Before)
        .unwrap();
    assert_eq!(r.contributions().len(), 1);
    assert_eq!(r.contributions()[0].source_part, 1);
    assert_eq!(related_anchors(&r, selection(&r).related), vec![0]);
}

#[test]
fn successful_point_and_range_batches_keep_every_result_row_range_separate() {
    let c = strong();
    let mut op = c.budget.begin(CancellationToken::new());
    let points = [
        PointQuery {
            model: c.identity,
            input: point_source(main_target(), 7),
            affinity: Affinity::Before,
        },
        PointQuery {
            model: c.identity,
            input: point_source(main_target(), 7),
            affinity: Affinity::After,
        },
        PointQuery {
            model: c.identity,
            input: point_flow(0, 4),
            affinity: Affinity::After,
        },
    ];
    let result = c
        .mapping
        .map_points(&c.ir, &c.source, &points, &mut op)
        .unwrap();
    assert_eq!(
        result
            .results()
            .iter()
            .map(|x| x.chosen.value())
            .collect::<Vec<_>>(),
        vec![1, 2, 16]
    );
    assert_eq!(
        related_spans(&result, result.results()[0].related),
        vec![(1, 2)]
    );
    assert_eq!(
        related_spans(&result, result.results()[1].related),
        vec![(1, 2)]
    );
    assert_eq!(
        related_spans(&result, result.results()[2].related),
        vec![(14, 16)]
    );
    let mut op = c.budget.begin(CancellationToken::new());
    let ranges = [
        RangeQuery {
            model: c.identity,
            input: source_range(main_target(), 5, 10),
            empty_affinity: Affinity::Before,
        },
        RangeQuery {
            model: c.identity,
            input: flow_range(0, 1, 2),
            empty_affinity: Affinity::After,
        },
        RangeQuery {
            model: c.identity,
            input: flow_range(0, 4, 4),
            empty_affinity: Affinity::After,
        },
    ];
    let result = c
        .mapping
        .map_ranges(&c.ir, &c.source, &ranges, &mut op)
        .unwrap();
    assert_eq!(result.results().len(), 3);
    if let RangeResult::Selection(x) = result.results()[0] {
        let x = x.slice(result.selections()).unwrap()[0];
        assert_eq!(related_spans(&result, x.related), vec![(1, 2)]);
    } else {
        panic!("first selection expected");
    }
    if let RangeResult::Selection(x) = result.results()[1] {
        let x = x.slice(result.selections()).unwrap()[0];
        assert_eq!(related_spans(&result, x.related), vec![(5, 10)]);
    } else {
        panic!("second selection expected");
    }
    if let RangeResult::Boundary(x) = result.results()[2] {
        let x = x.slice(result.boundaries()).unwrap()[0];
        assert_eq!(x.chosen.value(), 16);
    } else {
        panic!("last boundary expected");
    }
}

#[test]
fn review_hidden_range_cost_is_window_bounded() {
    for count in [16u64, 4096u64] {
        let bytes = vec![b'x'; count as usize];
        let parts: Vec<_> = (0..count).map(|x| (x, x + 1)).collect();
        let c = control(
            &bytes,
            &[FlowSpec {
                domains: &[(0, count)],
                length: 0,
                atoms: &[atom(AtomKind::Hidden, &parts, (0, 0), None)],
            }],
            &[],
        );
        let mut op = c.budget.begin(CancellationToken::new());
        let query = RangeQuery {
            model: c.identity,
            input: source_range(main_target(), count - 2, count - 1),
            empty_affinity: Affinity::Before,
        };
        let result = c
            .mapping
            .map_range(&c.ir, &c.source, &query, &mut op)
            .unwrap();
        println!(
            "hidden parts={count}, returned content={}, endpoints={}, events={}",
            result.contributions().len(),
            result.contributors().len(),
            op.stats().expect("statistics enabled").events()
        );
        assert_eq!(result.contributions().len(), 1);
        if count > 100 {
            assert!(
                op.stats().expect("statistics enabled").events() < 300,
                "single piece range scans unrelated parts"
            );
        }
    }
}
