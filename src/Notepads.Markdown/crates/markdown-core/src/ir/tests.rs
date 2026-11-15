// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE file in the project root for license information.
// ---------------------------------------------------------------------------------------------

//! Independent manual algorithm controls. These drafts never become a native AcceptedModel.

use super::*;
use crate::error::LimitKind;
use crate::identity::{LocalUtf8Span, LogicalSpan, SourceKey};
use crate::policy::{CancellationToken, Operation, SourceBudget};
use crate::source::{SourceDocument, SourceSnapshot};

struct Manual {
    source: SourceSnapshot,
    builder: IrBuilder,
    op: Operation,
}

fn span(start: u64, end: u64) -> SourceSpan {
    SourceSpan::new(start, end).unwrap()
}
fn logical(start: u64, end: u64) -> LogicalSpan {
    LogicalSpan::new(start, end).unwrap()
}
fn local(start: u64, end: u64) -> LocalUtf8Span {
    LocalUtf8Span::new(start, end).unwrap()
}
fn node(index: usize) -> NodeId {
    NodeId::from_index(index).unwrap()
}
fn flow(index: usize) -> FlowId {
    FlowId::from_index(index).unwrap()
}
fn rows<T: Copy>(start: usize, len: usize) -> Rows<T> {
    Rows::new(start, len).unwrap()
}

impl Manual {
    fn new(text: &str) -> Self {
        Self::with_budget(text, SourceBudget::model_fixture())
    }

    fn with_budget(text: &str, budget: SourceBudget) -> Self {
        let mut op = budget.begin(CancellationToken::new());
        let source = SourceDocument::new(
            SourceKey::new([7; 16], 1).unwrap(),
            text.as_bytes(),
            &mut op,
        )
        .unwrap()
        .snapshot();
        let mut builder = IrBuilder::new(&mut op).unwrap();
        let domains = builder
            .source_parts(&[span(0, text.len() as u64)], &mut op)
            .unwrap();
        builder
            .node(
                NodeRow {
                    parent: None,
                    first_child: None,
                    next_sibling: None,
                    flow: flow(0),
                    kind: NodeKind::Document,
                    detail: None,
                    source_envelope: span(0, text.len() as u64),
                    logical: logical(0, 0),
                },
                &mut op,
            )
            .unwrap();
        builder
            .flow(
                FlowRow {
                    scope: FlowScope::Main,
                    root: node(0),
                    domains,
                    atoms: rows(0, 0),
                    logical_utf16: 0,
                },
                &mut op,
            )
            .unwrap();
        Self {
            source,
            builder,
            op,
        }
    }

    fn claim(
        &mut self,
        owner: NodeId,
        kind: ClaimKind,
        phase: NativePhase,
        role: HostRole,
        sources: &[SourceSpan],
    ) -> ClaimId {
        let domains = self.builder.source_parts(sources, &mut self.op).unwrap();
        let utf8_len = sources.iter().map(|s| s.len()).sum();
        let host = HostRow {
            owner,
            role,
            utf8_len,
            domains,
        };
        self.builder
            .claim(
                AuthoritySpec {
                    owner,
                    host,
                    phase,
                    kind,
                    consumed: local(0, utf8_len),
                    sources: domains,
                },
                &mut self.op,
            )
            .unwrap()
    }

    fn atom(
        &mut self,
        owner: NodeId,
        kind: AtomKind,
        target: (u64, u64),
        sources: &[SourceSpan],
        claim: ClaimId,
        content: AtomContent,
    ) -> AtomId {
        let sources = self.builder.source_parts(sources, &mut self.op).unwrap();
        self.builder
            .atom(
                AtomRow {
                    owner,
                    kind,
                    target: TargetSpan {
                        start: target.0,
                        end: target.1,
                    },
                    sources,
                    claim,
                    proof: None,
                    content,
                },
                &mut self.op,
            )
            .unwrap()
    }

    fn copy(&mut self, source: SourceSpan, start: u64, end: u64) {
        let claim = self.claim(
            node(0),
            ClaimKind::Block,
            NativePhase::Block,
            HostRole::Block,
            &[source],
        );
        self.atom(
            node(0),
            AtomKind::Copy,
            (start, end),
            &[source],
            claim,
            AtomContent::Empty,
        );
    }

    fn hidden(&mut self, source: SourceSpan, at: u64) {
        let claim = self.claim(
            node(0),
            ClaimKind::Delimiter,
            NativePhase::Inline,
            HostRole::Inline,
            &[source],
        );
        self.atom(
            node(0),
            AtomKind::Hidden,
            (at, at),
            &[source],
            claim,
            AtomContent::Empty,
        );
    }

    fn transform(
        &mut self,
        source: SourceSpan,
        target: (u64, u64),
        output: &str,
        primitive: PrimitiveKind,
        stage: Option<StageKind>,
        claim_kind: ClaimKind,
    ) -> AtomId {
        self.transform_at(
            node(0),
            source,
            target,
            output,
            primitive,
            (stage, claim_kind),
        )
    }

    fn transform_at(
        &mut self,
        owner: NodeId,
        source: SourceSpan,
        target: (u64, u64),
        output: &str,
        primitive: PrimitiveKind,
        acceptance: (Option<StageKind>, ClaimKind),
    ) -> AtomId {
        let (stage, claim_kind) = acceptance;
        let claim = self.claim(
            owner,
            claim_kind,
            NativePhase::Inline,
            HostRole::Inline,
            &[source],
        );
        let value = self.builder.text(output, &mut self.op).unwrap();
        let atom = self.atom(
            owner,
            AtomKind::Transform,
            target,
            &[source],
            claim,
            AtomContent::Pool(value),
        );
        let primitives = self
            .builder
            .primitive_parts(
                &[PrimitiveRow {
                    source,
                    kind: primitive,
                }],
                &mut self.op,
            )
            .unwrap();
        let stages = if let Some(kind) = stage {
            let input_len = if primitive == PrimitiveKind::CrLf {
                1
            } else {
                source.len()
            };
            self.builder
                .stage_parts(
                    &[StageRow {
                        kind,
                        input: local(0, input_len),
                        output: value,
                        parameter: 0,
                        accepted_by: claim,
                    }],
                    &mut self.op,
                )
                .unwrap()
        } else {
            rows(self.builder.ir.stages.len(), 0)
        };
        let proof = self
            .builder
            .proof(
                ProofRow {
                    atom,
                    primitives,
                    stages,
                    value,
                },
                &mut self.op,
            )
            .unwrap();
        let mut row = self.builder.ir.test_atom(atom.index());
        row.proof = Some(proof);
        self.builder
            .ir
            .set_test_atom(atom.index(), row, &mut self.op)
            .unwrap();
        atom
    }

    fn payload(
        &mut self,
        owner: PayloadOwner,
        claim_owner: NodeId,
        role: PayloadRole,
        source: SourceSpan,
        value: &str,
    ) -> PayloadId {
        let domains = self.builder.source_parts(&[source], &mut self.op).unwrap();
        let start = self.builder.ir.atoms_len();
        if !source.is_empty() {
            let host_role = match role {
                PayloadRole::Destination => HostRole::Destination,
                PayloadRole::Title => HostRole::Title,
                PayloadRole::FenceInfo => HostRole::FenceInfo,
                PayloadRole::ReferenceLabel => HostRole::ReferenceLabel,
                PayloadRole::MathInput | PayloadRole::FenceInput => {
                    panic!("This fixture factory only creates legacy metadata payloads")
                }
            };
            let claim = self.claim(
                claim_owner,
                ClaimKind::Payload,
                NativePhase::Metadata,
                host_role,
                &[source],
            );
            self.atom(
                claim_owner,
                AtomKind::Copy,
                (0, value.len() as u64),
                &[source],
                claim,
                AtomContent::Empty,
            );
        }
        let value = self.builder.text(value, &mut self.op).unwrap();
        self.builder
            .payload(
                PayloadRow {
                    owner,
                    role,
                    value,
                    domains,
                    atoms: rows(start, self.builder.ir.atoms_len() - start),
                    utf8_len: value.len() as u64,
                },
                &mut self.op,
            )
            .unwrap()
    }

    fn image_nodes(
        &mut self,
        image_source: SourceSpan,
        image_target: (u64, u64),
        alt_sources: &[SourceSpan],
        alt_len: u64,
    ) {
        self.builder
            .node(
                NodeRow {
                    parent: Some(node(0)),
                    first_child: None,
                    next_sibling: None,
                    flow: flow(0),
                    kind: NodeKind::Image,
                    detail: None,
                    source_envelope: image_source,
                    logical: logical(image_target.0, image_target.1),
                },
                &mut self.op,
            )
            .unwrap();
        let mut root = self.builder.ir.nodes()[0];
        root.first_child = Some(node(1));
        self.builder
            .ir
            .nodes
            .replace(0, root, &mut self.op)
            .unwrap();
        self.builder
            .node(
                NodeRow {
                    parent: None,
                    first_child: None,
                    next_sibling: None,
                    flow: flow(1),
                    kind: NodeKind::AltRoot,
                    detail: None,
                    source_envelope: span(
                        alt_sources[0].start(),
                        alt_sources.last().unwrap().end(),
                    ),
                    logical: logical(0, alt_len),
                },
                &mut self.op,
            )
            .unwrap();
        let domains = self
            .builder
            .source_parts(alt_sources, &mut self.op)
            .unwrap();
        self.builder
            .flow(
                FlowRow {
                    scope: FlowScope::ImageAlt {
                        object: ObjectId::from_index(0).unwrap(),
                    },
                    root: node(2),
                    domains,
                    atoms: rows(0, 0),
                    logical_utf16: alt_len,
                },
                &mut self.op,
            )
            .unwrap();
    }

    fn alt_atom(&mut self, source: SourceSpan, target: (u64, u64), hidden: bool) {
        let claim = self.claim(
            node(2),
            if hidden {
                ClaimKind::Delimiter
            } else {
                ClaimKind::Block
            },
            NativePhase::Inline,
            HostRole::Inline,
            &[source],
        );
        self.atom(
            node(2),
            if hidden {
                AtomKind::Hidden
            } else {
                AtomKind::Copy
            },
            target,
            &[source],
            claim,
            AtomContent::Empty,
        );
    }

    fn seal_alt(&mut self, start: usize) {
        let mut alt = self.builder.ir.flows()[1];
        alt.atoms = rows(start, self.builder.ir.atoms_len() - start);
        self.builder.ir.flows.replace(1, alt, &mut self.op).unwrap();
    }

    fn image_object(
        &mut self,
        image_atom: AtomId,
        destination: PayloadId,
        title: Option<PayloadId>,
    ) {
        self.builder
            .object(
                ObjectRow::Image {
                    node: node(1),
                    atom: image_atom,
                    alt: flow(1),
                    destination,
                    title,
                    reference: None,
                    wiki_size: None,
                },
                &mut self.op,
            )
            .unwrap();
    }

    fn seal(&mut self, length: u64) {
        let mut root = self.builder.ir.nodes()[0];
        root.logical = logical(0, length);
        self.builder
            .ir
            .nodes
            .replace(0, root, &mut self.op)
            .unwrap();
        let mut main = self.builder.ir.flows()[0];
        main.logical_utf16 = length;
        main.atoms = rows(0, self.builder.ir.atoms_len());
        self.builder
            .ir
            .flows
            .replace(0, main, &mut self.op)
            .unwrap();
    }

    fn validate(&mut self) -> Result<(), CoreError> {
        let prior_fault = self.op.first_fault();
        let result = self
            .builder
            .ir
            .snapshot(&mut self.op)
            .and_then(|ir| super::validate::validate(&ir, &self.source, &mut self.op));
        if prior_fault.is_none() {
            validated_handoff::assert_legacy_semantics(&self.builder.ir, &self.source, result);
        }
        result
    }
}

fn strong() -> Manual {
    let mut manual = Manual::new("**中&amp;😀**");
    manual.hidden(span(0, 2), 0);
    manual.copy(span(2, 5), 0, 1);
    manual.transform(
        span(5, 10),
        (1, 2),
        "&",
        PrimitiveKind::Copy,
        Some(StageKind::NamedEntity),
        ClaimKind::Entity,
    );
    manual.copy(span(10, 14), 2, 4);
    manual.hidden(span(14, 16), 4);
    manual.seal(4);
    let strong = manual
        .builder
        .node(
            NodeRow {
                parent: Some(node(0)),
                first_child: None,
                next_sibling: None,
                flow: flow(0),
                kind: NodeKind::Strong,
                detail: None,
                source_envelope: span(0, 16),
                logical: logical(0, 4),
            },
            &mut manual.op,
        )
        .unwrap();
    let mut root = manual.builder.ir.nodes()[0];
    root.first_child = Some(strong);
    manual
        .builder
        .ir
        .nodes
        .replace(0, root, &mut manual.op)
        .unwrap();
    manual
}

#[test]
fn manual_strong_entity_and_astral_exact_rows() {
    let mut m = strong();
    assert_eq!(m.source.text().len(), 16);
    assert_eq!(m.validate(), Ok(()));
    assert_eq!(m.builder.ir.atoms_len(), 5);
    assert_eq!(
        m.builder.ir.test_atom(2).target,
        TargetSpan { start: 1, end: 2 }
    );
    assert_eq!(
        m.builder
            .ir
            .test_atom(3)
            .sources
            .slice(m.builder.ir.sources())
            .unwrap(),
        &[span(10, 14)]
    );
    assert_eq!(m.builder.ir.flows()[0].logical_utf16, 4);
}

#[test]
fn manual_multiscalar_entity_one_minimal_atom() {
    let mut m = Manual::new("&NotEqualTilde;");
    m.transform(
        span(0, 15),
        (0, 2),
        "\u{2242}\u{338}",
        PrimitiveKind::Copy,
        Some(StageKind::NamedEntity),
        ClaimKind::Entity,
    );
    m.seal(2);
    assert_eq!(m.validate(), Ok(()));
    assert_eq!(m.builder.ir.atoms_len(), 1);
    assert_eq!(
        m.builder.ir.text(m.builder.ir.proofs()[0].value).unwrap(),
        "≂̸"
    );
    let child = m
        .builder
        .node(
            NodeRow {
                parent: Some(node(0)),
                first_child: None,
                next_sibling: None,
                flow: flow(0),
                kind: NodeKind::Text,
                detail: None,
                source_envelope: span(0, 15),
                logical: logical(1, 2),
            },
            &mut m.op,
        )
        .unwrap();
    let mut root = m.builder.ir.nodes()[0];
    root.first_child = Some(child);
    m.builder.ir.nodes.replace(0, root, &mut m.op).unwrap();
    assert_eq!(m.validate(), Ok(())); // combining-scalar seam is a legal UTF16 boundary.
}

#[test]
fn manual_crlf_normalization_then_softbreak_space() {
    let mut m = Manual::new("a\r\nb");
    m.copy(span(0, 1), 0, 1);
    m.transform(
        span(1, 3),
        (1, 2),
        " ",
        PrimitiveKind::CrLf,
        Some(StageKind::SoftBreakToSpace),
        ClaimKind::Break,
    );
    m.copy(span(3, 4), 2, 3);
    m.seal(3);
    assert_eq!(m.validate(), Ok(()));
    assert_eq!(m.builder.ir.primitives()[0].kind, PrimitiveKind::CrLf);
    assert_eq!(m.builder.ir.stages()[0].input, local(0, 1));
}

#[test]
fn manual_empty_main_and_full_hidden_main_shape() {
    let mut empty = Manual::new("");
    empty.seal(0);
    assert_eq!(empty.validate(), Ok(()));
    let mut hidden = Manual::new("[id]: /u");
    hidden.hidden(span(0, 8), 0);
    hidden.seal(0);
    assert_eq!(hidden.validate(), Ok(()));
    assert_eq!(
        hidden.builder.ir.flows()[0]
            .domains
            .slice(hidden.builder.ir.sources())
            .unwrap(),
        &[span(0, 8)]
    );
}

#[test]
fn malformed_graph_partition_and_surrogate_boundaries_latch_faults() {
    let mut m = strong();
    let mut root = m.builder.ir.nodes()[0];
    root.next_sibling = Some(node(0));
    m.builder.ir.nodes.replace(0, root, &mut m.op).unwrap();
    assert_eq!(m.validate(), Err(CoreError::InvalidProvenance));
    assert_eq!(m.op.first_fault(), Some(CoreError::InvalidProvenance));
    let mut m = strong();
    let mut copy = m.builder.ir.test_atom(3);
    copy.target.start = 1;
    m.builder.ir.set_test_atom(3, copy, &mut m.op).unwrap();
    assert_eq!(m.validate(), Err(CoreError::InvalidProvenance));
    let mut m = strong();
    let mut child = m.builder.ir.nodes()[1];
    child.logical = logical(0, 3);
    m.builder.ir.nodes.replace(1, child, &mut m.op).unwrap();
    assert_eq!(m.validate(), Err(CoreError::InvalidProvenance));
}

#[test]
fn adjacent_entities_cannot_be_one_whole_leaf_transform() {
    let mut m = Manual::new("&amp;&amp;");
    let claim = m.claim(
        node(0),
        ClaimKind::Entity,
        NativePhase::Inline,
        HostRole::Inline,
        &[span(0, 10)],
    );
    let amp = m.builder.text("&", &mut m.op).unwrap();
    let value = m.builder.text("&&", &mut m.op).unwrap();
    let atom = m.atom(
        node(0),
        AtomKind::Transform,
        (0, 2),
        &[span(0, 10)],
        claim,
        AtomContent::Pool(value),
    );
    let primitives = m
        .builder
        .primitive_parts(
            &[PrimitiveRow {
                source: span(0, 10),
                kind: PrimitiveKind::Copy,
            }],
            &mut m.op,
        )
        .unwrap();
    let stages = m
        .builder
        .stage_parts(
            &[
                StageRow {
                    kind: StageKind::NamedEntity,
                    input: local(0, 5),
                    output: amp,
                    parameter: 0,
                    accepted_by: claim,
                },
                StageRow {
                    kind: StageKind::NamedEntity,
                    input: local(1, 6),
                    output: amp,
                    parameter: 0,
                    accepted_by: claim,
                },
            ],
            &mut m.op,
        )
        .unwrap();
    let proof = m
        .builder
        .proof(
            ProofRow {
                atom,
                primitives,
                stages,
                value,
            },
            &mut m.op,
        )
        .unwrap();
    let mut row = m.builder.ir.test_atom(0);
    row.proof = Some(proof);
    m.builder.ir.set_test_atom(0, row, &mut m.op).unwrap();
    m.seal(2);
    assert_eq!(m.validate(), Err(CoreError::InvalidProvenance));
    let mut m = Manual::new("&amp;&amp;");
    m.transform(
        span(0, 5),
        (0, 1),
        "&",
        PrimitiveKind::Copy,
        Some(StageKind::NamedEntity),
        ClaimKind::Entity,
    );
    m.transform(
        span(5, 10),
        (1, 2),
        "&",
        PrimitiveKind::Copy,
        Some(StageKind::NamedEntity),
        ClaimKind::Entity,
    );
    m.seal(2);
    assert_eq!(m.validate(), Ok(()));
}

#[test]
fn independent_numeric_policy_and_named_table() {
    assert_eq!(super::entities::numeric("&#x80;"), Some('\u{80}'));
    assert_eq!(super::entities::numeric("&#0;"), Some('\u{fffd}'));
    assert_eq!(super::entities::numeric("&#xD800;"), Some('\u{fffd}'));
    assert_eq!(super::entities::numeric("&#xE000;"), Some('\u{e000}'));
    assert_eq!(super::entities::numeric("&#12345678;"), None);
    assert_eq!(
        super::entities::named("&NotEqualTilde;"),
        Some("\u{2242}\u{338}")
    );
    assert_eq!(super::entities::named("&madeUp;"), None);
}

fn compound_image() -> Manual {
    let mut m = Manual::new("x ![**中** &amp; `😀`](u \"t\") y");
    m.image_nodes(span(2, 32), (2, 3), &[span(4, 24)], 6);
    m.copy(span(0, 2), 0, 2);
    let claim = m.claim(
        node(1),
        ClaimKind::Image,
        NativePhase::Inline,
        HostRole::Inline,
        &[span(2, 32)],
    );
    let object_atom = m.atom(
        node(1),
        AtomKind::Object,
        (2, 3),
        &[span(2, 32)],
        claim,
        AtomContent::Object(ObjectId::from_index(0).unwrap()),
    );
    m.copy(span(32, 34), 3, 5);
    m.seal(5);
    let start = m.builder.ir.atoms_len();
    m.alt_atom(span(4, 6), (0, 0), true);
    m.alt_atom(span(6, 9), (0, 1), false);
    m.alt_atom(span(9, 11), (1, 1), true);
    m.alt_atom(span(11, 12), (1, 2), false);
    m.transform_at(
        node(2),
        span(12, 17),
        (2, 3),
        "&",
        PrimitiveKind::Copy,
        (Some(StageKind::NamedEntity), ClaimKind::Entity),
    );
    m.alt_atom(span(17, 18), (3, 4), false);
    m.alt_atom(span(18, 19), (4, 4), true);
    m.alt_atom(span(19, 23), (4, 6), false);
    m.alt_atom(span(23, 24), (6, 6), true);
    m.seal_alt(start);
    let destination = m.payload(
        PayloadOwner::Node(node(1)),
        node(1),
        PayloadRole::Destination,
        span(26, 27),
        "u",
    );
    let title = m.payload(
        PayloadOwner::Node(node(1)),
        node(1),
        PayloadRole::Title,
        span(29, 30),
        "t",
    );
    m.image_object(object_atom, destination, Some(title));
    m
}

#[test]
fn manual_compound_image_scopes_and_payloads_are_independent() {
    let mut m = compound_image();
    assert_eq!(m.source.text().len(), 34);
    assert_eq!(m.validate(), Ok(()));
    assert_eq!(m.builder.ir.flows()[0].logical_utf16, 5);
    assert_eq!(m.builder.ir.flows()[1].logical_utf16, 6);
    assert_eq!(
        m.builder.ir.flows()[1]
            .domains
            .slice(m.builder.ir.sources())
            .unwrap(),
        &[span(4, 24)]
    );
    assert_eq!(
        m.builder.ir.payloads()[0]
            .domains
            .slice(m.builder.ir.sources())
            .unwrap(),
        &[span(26, 27)]
    );
    assert_eq!(
        m.builder.ir.payloads()[1]
            .domains
            .slice(m.builder.ir.sources())
            .unwrap(),
        &[span(29, 30)]
    );
    let mut object = m.builder.ir.objects()[0];
    if let ObjectRow::Image { ref mut alt, .. } = object {
        *alt = flow(0);
    }
    m.builder.ir.objects.replace(0, object, &mut m.op).unwrap();
    assert_eq!(m.validate(), Err(CoreError::InvalidProvenance));
}

#[test]
fn manual_empty_alt_retains_one_real_zero_domain_no_fake_atom() {
    let mut m = Manual::new("![](u)");
    m.image_nodes(span(0, 6), (0, 1), &[span(2, 2)], 0);
    let claim = m.claim(
        node(1),
        ClaimKind::Image,
        NativePhase::Inline,
        HostRole::Inline,
        &[span(0, 6)],
    );
    let object = m.atom(
        node(1),
        AtomKind::Object,
        (0, 1),
        &[span(0, 6)],
        claim,
        AtomContent::Object(ObjectId::from_index(0).unwrap()),
    );
    m.seal(1);
    m.seal_alt(m.builder.ir.atoms_len());
    let destination = m.payload(
        PayloadOwner::Node(node(1)),
        node(1),
        PayloadRole::Destination,
        span(4, 5),
        "u",
    );
    m.image_object(object, destination, None);
    assert_eq!(m.validate(), Ok(()));
    assert!(m.builder.ir.flows()[1].atoms.is_empty());
    assert_eq!(
        m.builder.ir.flows()[1]
            .domains
            .slice(m.builder.ir.sources())
            .unwrap(),
        &[span(2, 2)]
    );
}

#[test]
fn manual_disjoint_alt_domains_do_not_gain_the_envelope_hole() {
    let mut m = Manual::new("![abc](u)");
    m.image_nodes(span(0, 9), (0, 1), &[span(2, 3), span(4, 5)], 2);
    let claim = m.claim(
        node(1),
        ClaimKind::Image,
        NativePhase::Inline,
        HostRole::Inline,
        &[span(0, 9)],
    );
    let object = m.atom(
        node(1),
        AtomKind::Object,
        (0, 1),
        &[span(0, 9)],
        claim,
        AtomContent::Object(ObjectId::from_index(0).unwrap()),
    );
    m.seal(1);
    let start = m.builder.ir.atoms_len();
    m.alt_atom(span(2, 3), (0, 1), false);
    m.alt_atom(span(4, 5), (1, 2), false);
    m.seal_alt(start);
    let destination = m.payload(
        PayloadOwner::Node(node(1)),
        node(1),
        PayloadRole::Destination,
        span(7, 8),
        "u",
    );
    m.image_object(object, destination, None);
    assert_eq!(m.validate(), Ok(()));
    assert_eq!(m.builder.ir.nodes()[2].source_envelope, span(2, 5));
    let broad = m.builder.source_parts(&[span(2, 5)], &mut m.op).unwrap();
    let mut alt = m.builder.ir.flows()[1];
    alt.domains = broad;
    m.builder.ir.flows.replace(1, alt, &mut m.op).unwrap();
    assert_eq!(m.validate(), Err(CoreError::InvalidProvenance));
}

fn reference() -> Manual {
    let mut m = Manual::new("[x][id]\n\n[id]: /u");
    m.builder
        .node(
            NodeRow {
                parent: Some(node(0)),
                first_child: None,
                next_sibling: None,
                flow: flow(0),
                kind: NodeKind::Link,
                detail: None,
                source_envelope: span(0, 7),
                logical: logical(0, 1),
            },
            &mut m.op,
        )
        .unwrap();
    let mut root = m.builder.ir.nodes()[0];
    root.first_child = Some(node(1));
    m.builder.ir.nodes.replace(0, root, &mut m.op).unwrap();
    let link = m.claim(
        node(1),
        ClaimKind::Link,
        NativePhase::Inline,
        HostRole::Inline,
        &[span(0, 7)],
    );
    m.atom(
        node(1),
        AtomKind::Hidden,
        (0, 0),
        &[span(0, 1)],
        link,
        AtomContent::Empty,
    );
    m.atom(
        node(1),
        AtomKind::Copy,
        (0, 1),
        &[span(1, 2)],
        link,
        AtomContent::Empty,
    );
    m.atom(
        node(1),
        AtomKind::Hidden,
        (1, 1),
        &[span(2, 7)],
        link,
        AtomContent::Empty,
    );
    m.hidden(span(7, 9), 1);
    let definition_claim = m.claim(
        node(0),
        ClaimKind::ReferenceDefinition,
        NativePhase::Block,
        HostRole::Block,
        &[span(9, 17)],
    );
    m.atom(
        node(0),
        AtomKind::Hidden,
        (1, 1),
        &[span(9, 17)],
        definition_claim,
        AtomContent::Empty,
    );
    m.seal(1);
    let definition_id = DefinitionId::from_index(0).unwrap();
    let label = m.payload(
        PayloadOwner::Definition(definition_id),
        node(0),
        PayloadRole::ReferenceLabel,
        span(10, 12),
        "id",
    );
    let destination = m.payload(
        PayloadOwner::Definition(definition_id),
        node(0),
        PayloadRole::Destination,
        span(15, 17),
        "/u",
    );
    let key = m.builder.text("id", &mut m.op).unwrap();
    let syntax = m.builder.source_parts(&[span(9, 17)], &mut m.op).unwrap();
    m.builder
        .definition(
            DefinitionRow {
                syntax,
                label,
                destination,
                title: None,
                key,
                winner: true,
                claim: definition_claim,
            },
            &mut m.op,
        )
        .unwrap();
    let occurrence_destination = m.payload(
        PayloadOwner::Node(node(1)),
        node(1),
        PayloadRole::Destination,
        span(15, 17),
        "/u",
    );
    let claim = m.claim(
        node(1),
        ClaimKind::ReferenceUse,
        NativePhase::Inline,
        HostRole::Inline,
        &[span(0, 7)],
    );
    let syntax = m.builder.source_parts(&[span(0, 7)], &mut m.op).unwrap();
    let reference = m
        .builder
        .reference(
            ReferenceUseRow {
                node: node(1),
                syntax,
                winner: definition_id,
                key,
                destination: occurrence_destination,
                title: None,
                claim,
            },
            &mut m.op,
        )
        .unwrap();
    let detail = m
        .builder
        .detail(
            DetailRow::Link {
                destination: occurrence_destination,
                title: None,
                reference: Some(reference),
                claim: link,
            },
            &mut m.op,
        )
        .unwrap();
    let mut node = m.builder.ir.nodes()[1];
    node.detail = Some(detail);
    m.builder.ir.nodes.replace(1, node, &mut m.op).unwrap();
    m
}

#[test]
fn manual_reference_occurrence_uses_its_own_payload_owner_and_winner() {
    let mut m = reference();
    assert_eq!(m.source.text().len(), 17);
    assert_eq!(m.validate(), Ok(()));
    assert_ne!(
        m.builder.ir.definitions()[0].destination,
        m.builder.ir.references()[0].destination
    );
    assert_eq!(
        m.builder.ir.payloads()[2].owner,
        PayloadOwner::Node(node(1))
    );
    let mut definition = m.builder.ir.definitions()[0];
    definition.winner = false;
    m.builder
        .ir
        .definitions
        .replace(0, definition, &mut m.op)
        .unwrap();
    assert_eq!(m.validate(), Err(CoreError::InvalidProvenance));
    let mut m = reference();
    let mut detail = m.builder.ir.details()[0];
    if let DetailRow::Link {
        ref mut reference, ..
    } = detail
    {
        *reference = None;
    }
    m.builder.ir.details.replace(0, detail, &mut m.op).unwrap();
    assert_eq!(m.validate(), Err(CoreError::InvalidProvenance));
}

#[test]
fn partial_tab_and_table_pipe_use_real_block_phase_and_parameter() {
    let mut m = Manual::new("\t");
    m.transform(
        span(0, 1),
        (0, 1),
        " ",
        PrimitiveKind::Copy,
        Some(StageKind::PartialTab),
        ClaimKind::ContainerPrefix,
    );
    m.seal(1);
    let mut claim = m.builder.ir.test_authority_at(0);
    claim.phase = NativePhase::Block;
    claim.host.role = HostRole::Block;
    m.builder
        .ir
        .set_test_authority_at(0, claim, &mut m.op)
        .unwrap();
    let mut stage = m.builder.ir.stages()[0];
    stage.parameter = 3;
    m.builder.ir.stages.replace(0, stage, &mut m.op).unwrap();
    assert_eq!(m.validate(), Ok(()));
    let mut m = Manual::new("\\|");
    m.transform(
        span(0, 2),
        (0, 1),
        "|",
        PrimitiveKind::Copy,
        Some(StageKind::TableEscapedPipe),
        ClaimKind::Table,
    );
    m.seal(1);
    let mut claim = m.builder.ir.test_authority_at(0);
    claim.phase = NativePhase::Block;
    claim.host.role = HostRole::Block;
    m.builder
        .ir
        .set_test_authority_at(0, claim, &mut m.op)
        .unwrap();
    assert_eq!(m.validate(), Ok(()));
    let mut stage = m.builder.ir.stages()[0];
    stage.parameter = 1;
    m.builder.ir.stages.replace(0, stage, &mut m.op).unwrap();
    assert_eq!(m.validate(), Err(CoreError::InvalidProvenance));
}

#[test]
fn repeated_entity_and_body_entity_escape_are_rejected() {
    for (text, first_end, first_output, second_kind) in [
        ("&amp;amp;", 5, "&", StageKind::NamedEntity),
        ("&bsol;*", 6, "\\", StageKind::AsciiEscape),
    ] {
        let mut m = Manual::new(text);
        let first = m.claim(
            node(0),
            ClaimKind::Entity,
            NativePhase::Inline,
            HostRole::Inline,
            &[span(0, first_end)],
        );
        let last = m.claim(
            node(0),
            if second_kind == StageKind::AsciiEscape {
                ClaimKind::Escape
            } else {
                ClaimKind::Entity
            },
            NativePhase::Inline,
            HostRole::Inline,
            &[span(0, text.len() as u64)],
        );
        let intermediate = m.builder.text(first_output, &mut m.op).unwrap();
        let output = m
            .builder
            .text(
                if second_kind == StageKind::AsciiEscape {
                    "*"
                } else {
                    "&"
                },
                &mut m.op,
            )
            .unwrap();
        let atom = m.atom(
            node(0),
            AtomKind::Transform,
            (0, 1),
            &[span(0, text.len() as u64)],
            last,
            AtomContent::Pool(output),
        );
        let primitives = m
            .builder
            .primitive_parts(
                &[PrimitiveRow {
                    source: span(0, text.len() as u64),
                    kind: PrimitiveKind::Copy,
                }],
                &mut m.op,
            )
            .unwrap();
        let stages = m
            .builder
            .stage_parts(
                &[
                    StageRow {
                        kind: StageKind::NamedEntity,
                        input: local(0, first_end),
                        output: intermediate,
                        parameter: 0,
                        accepted_by: first,
                    },
                    StageRow {
                        kind: second_kind,
                        input: local(
                            0,
                            if second_kind == StageKind::AsciiEscape {
                                2
                            } else {
                                5
                            },
                        ),
                        output,
                        parameter: 0,
                        accepted_by: last,
                    },
                ],
                &mut m.op,
            )
            .unwrap();
        let proof = m
            .builder
            .proof(
                ProofRow {
                    atom,
                    primitives,
                    stages,
                    value: output,
                },
                &mut m.op,
            )
            .unwrap();
        let mut atom_row = m.builder.ir.test_atom(0);
        atom_row.proof = Some(proof);
        m.builder.ir.set_test_atom(0, atom_row, &mut m.op).unwrap();
        m.seal(1);
        assert_eq!(m.validate(), Err(CoreError::InvalidProvenance));
    }
}

fn metadata_closure() -> Manual {
    let mut m = Manual::new("&bsol;&ast;");
    m.builder
        .node(
            NodeRow {
                parent: Some(node(0)),
                first_child: None,
                next_sibling: None,
                flow: flow(0),
                kind: NodeKind::Link,
                detail: None,
                source_envelope: span(0, 11),
                logical: logical(0, 0),
            },
            &mut m.op,
        )
        .unwrap();
    let mut root = m.builder.ir.nodes()[0];
    root.first_child = Some(node(1));
    m.builder.ir.nodes.replace(0, root, &mut m.op).unwrap();
    m.hidden(span(0, 11), 0);
    m.seal(0);
    let first = m.claim(
        node(1),
        ClaimKind::Entity,
        NativePhase::Metadata,
        HostRole::Destination,
        &[span(0, 6)],
    );
    let second = m.claim(
        node(1),
        ClaimKind::Entity,
        NativePhase::Metadata,
        HostRole::Destination,
        &[span(6, 11)],
    );
    let last = m.claim(
        node(1),
        ClaimKind::Escape,
        NativePhase::Metadata,
        HostRole::Destination,
        &[span(0, 11)],
    );
    let slash = m.builder.text("\\", &mut m.op).unwrap();
    let star = m.builder.text("*", &mut m.op).unwrap();
    let start = m.builder.ir.atoms_len();
    let atom = m.atom(
        node(1),
        AtomKind::Transform,
        (0, 1),
        &[span(0, 11)],
        last,
        AtomContent::Pool(star),
    );
    let primitives = m
        .builder
        .primitive_parts(
            &[PrimitiveRow {
                source: span(0, 11),
                kind: PrimitiveKind::Copy,
            }],
            &mut m.op,
        )
        .unwrap();
    let stages = m
        .builder
        .stage_parts(
            &[
                StageRow {
                    kind: StageKind::NamedEntity,
                    input: local(0, 6),
                    output: slash,
                    parameter: 0,
                    accepted_by: first,
                },
                StageRow {
                    kind: StageKind::NamedEntity,
                    input: local(1, 6),
                    output: star,
                    parameter: 0,
                    accepted_by: second,
                },
                StageRow {
                    kind: StageKind::AsciiEscape,
                    input: local(0, 2),
                    output: star,
                    parameter: 0,
                    accepted_by: last,
                },
            ],
            &mut m.op,
        )
        .unwrap();
    let proof = m
        .builder
        .proof(
            ProofRow {
                atom,
                primitives,
                stages,
                value: star,
            },
            &mut m.op,
        )
        .unwrap();
    let mut row = m.builder.ir.test_atom(atom.index());
    row.proof = Some(proof);
    m.builder
        .ir
        .set_test_atom(atom.index(), row, &mut m.op)
        .unwrap();
    let domains = m.builder.source_parts(&[span(0, 11)], &mut m.op).unwrap();
    let destination = m
        .builder
        .payload(
            PayloadRow {
                owner: PayloadOwner::Node(node(1)),
                role: PayloadRole::Destination,
                value: star,
                domains,
                atoms: rows(start, 1),
                utf8_len: 1,
            },
            &mut m.op,
        )
        .unwrap();
    let link = m.claim(
        node(1),
        ClaimKind::Link,
        NativePhase::Inline,
        HostRole::Inline,
        &[span(0, 11)],
    );
    let detail = m
        .builder
        .detail(
            DetailRow::Link {
                destination,
                title: None,
                reference: None,
                claim: link,
            },
            &mut m.op,
        )
        .unwrap();
    let mut row = m.builder.ir.nodes()[1];
    row.detail = Some(detail);
    m.builder.ir.nodes.replace(1, row, &mut m.op).unwrap();
    m
}

#[test]
fn actual_metadata_entity_then_escape_joins_two_entity_sources() {
    let mut m = metadata_closure();
    assert_eq!(m.validate(), Ok(()));
    assert_eq!(
        m.builder
            .ir
            .test_atom(1)
            .sources
            .slice(m.builder.ir.sources())
            .unwrap(),
        &[span(0, 11)]
    );
    let mut first = m.builder.ir.stages()[0];
    first.parameter = 1;
    m.builder.ir.stages.replace(0, first, &mut m.op).unwrap();
    assert_eq!(m.validate(), Err(CoreError::InvalidProvenance));
}

fn heading(attributes: bool) -> Manual {
    let mut m = Manual::new("# x");
    m.builder
        .node(
            NodeRow {
                parent: Some(node(0)),
                first_child: None,
                next_sibling: None,
                flow: flow(0),
                kind: NodeKind::Heading,
                detail: None,
                source_envelope: span(0, 3),
                logical: logical(0, 1),
            },
            &mut m.op,
        )
        .unwrap();
    let mut root = m.builder.ir.nodes()[0];
    root.first_child = Some(node(1));
    m.builder.ir.nodes.replace(0, root, &mut m.op).unwrap();
    let claim = m.claim(
        node(1),
        ClaimKind::Block,
        NativePhase::Block,
        HostRole::Block,
        &[span(0, 3)],
    );
    m.atom(
        node(1),
        AtomKind::Hidden,
        (0, 0),
        &[span(0, 2)],
        claim,
        AtomContent::Empty,
    );
    m.atom(
        node(1),
        AtomKind::Copy,
        (0, 1),
        &[span(2, 3)],
        claim,
        AtomContent::Empty,
    );
    m.seal(1);
    let detail = m
        .builder
        .detail(
            DetailRow::Heading {
                level: 1,
                form: HeadingForm::Atx,
                claim,
            },
            &mut m.op,
        )
        .unwrap();
    let mut row = m.builder.ir.nodes()[1];
    row.detail = Some(detail);
    m.builder.ir.nodes.replace(1, row, &mut m.op).unwrap();
    if attributes {
        for kind in [AttributeKind::HeadingLevel, AttributeKind::HeadingForm] {
            let sources = m.builder.source_parts(&[span(0, 1)], &mut m.op).unwrap();
            m.builder
                .attribute(
                    AttributeRow {
                        owner: node(1),
                        kind,
                        claim,
                        sources,
                    },
                    &mut m.op,
                )
                .unwrap();
        }
    }
    m
}

#[test]
fn mandatory_heading_attributes_match_real_marker_and_detail() {
    let mut m = heading(true);
    assert_eq!(m.validate(), Ok(()));
    let mut detail = m.builder.ir.details()[0];
    if let DetailRow::Heading { ref mut level, .. } = detail {
        *level = 2;
    }
    m.builder.ir.details.replace(0, detail, &mut m.op).unwrap();
    assert_eq!(m.validate(), Err(CoreError::InvalidProvenance));
    let mut m = heading(false);
    assert_eq!(m.validate(), Err(CoreError::InvalidProvenance));
}

#[test]
fn attribute_masks_reject_duplicate_and_cross_owner_rows() {
    let mut duplicate = heading(true);
    let attribute = duplicate.builder.ir.attributes.as_slice()[0];
    duplicate
        .builder
        .ir
        .attributes
        .push(attribute, &mut duplicate.op)
        .unwrap();
    assert_eq!(duplicate.validate(), Err(CoreError::InvalidProvenance));
    assert_eq!(
        duplicate.op.first_fault(),
        Some(CoreError::InvalidProvenance)
    );

    let mut crossed = heading(true);
    let mut attribute = crossed.builder.ir.attributes.as_slice()[0];
    attribute.owner = node(0);
    crossed
        .builder
        .ir
        .attributes
        .replace(0, attribute, &mut crossed.op)
        .unwrap();
    assert_eq!(crossed.validate(), Err(CoreError::InvalidProvenance));
    assert_eq!(crossed.op.first_fault(), Some(CoreError::InvalidProvenance));
}

fn task(marker_text: &str, checked: bool, action: Option<[u8; 3]>) -> Manual {
    let text = format!("- {marker_text} a");
    let marker_end = 2 + marker_text.len() as u64;
    let end = text.len() as u64;
    let mut m = Manual::new(&text);
    for row in [
        NodeRow {
            parent: Some(node(0)),
            first_child: Some(node(2)),
            next_sibling: None,
            flow: flow(0),
            kind: NodeKind::List,
            detail: None,
            source_envelope: span(0, end),
            logical: logical(0, 3),
        },
        NodeRow {
            parent: Some(node(1)),
            first_child: Some(node(3)),
            next_sibling: None,
            flow: flow(0),
            kind: NodeKind::ListItem,
            detail: None,
            source_envelope: span(0, end),
            logical: logical(0, 3),
        },
        NodeRow {
            parent: Some(node(2)),
            first_child: None,
            next_sibling: Some(node(4)),
            flow: flow(0),
            kind: NodeKind::TaskCheckbox,
            detail: None,
            source_envelope: span(2, marker_end),
            logical: logical(0, 2),
        },
        NodeRow {
            parent: Some(node(2)),
            first_child: None,
            next_sibling: None,
            flow: flow(0),
            kind: NodeKind::Paragraph,
            detail: None,
            source_envelope: span(end - 1, end),
            logical: logical(2, 3),
        },
    ] {
        m.builder.node(row, &mut m.op).unwrap();
    }
    let mut root = m.builder.ir.nodes()[0];
    root.first_child = Some(node(1));
    m.builder.ir.nodes.replace(0, root, &mut m.op).unwrap();
    let list = m.claim(
        node(1),
        ClaimKind::ContainerPrefix,
        NativePhase::Block,
        HostRole::Block,
        &[span(0, 2)],
    );
    let item = m.claim(
        node(2),
        ClaimKind::ContainerPrefix,
        NativePhase::Block,
        HostRole::Block,
        &[span(0, 2)],
    );
    let list_detail = m
        .builder
        .detail(
            DetailRow::List {
                marker: ListMarker::Bullet(b'-'),
                start: 0,
                tight: true,
                claim: list,
            },
            &mut m.op,
        )
        .unwrap();
    let item_detail = m
        .builder
        .detail(
            DetailRow::Item {
                occurrence: 0,
                marker: ListMarker::Bullet(b'-'),
                value: 0,
                claim: item,
            },
            &mut m.op,
        )
        .unwrap();
    for (index, detail) in [(1, list_detail), (2, item_detail)] {
        let mut row = m.builder.ir.nodes()[index];
        row.detail = Some(detail);
        m.builder.ir.nodes.replace(index, row, &mut m.op).unwrap();
    }
    for (owner, kind, claim) in [
        (node(1), AttributeKind::ListMarker, list),
        (node(1), AttributeKind::ListStart, list),
        (node(1), AttributeKind::ListTightness, list),
        (node(2), AttributeKind::ItemMarker, item),
    ] {
        let sources = m.builder.source_parts(&[span(0, 1)], &mut m.op).unwrap();
        m.builder
            .attribute(
                AttributeRow {
                    owner,
                    kind,
                    claim,
                    sources,
                },
                &mut m.op,
            )
            .unwrap();
    }
    m.atom(
        node(1),
        AtomKind::Hidden,
        (0, 0),
        &[span(0, 2)],
        list,
        AtomContent::Empty,
    );
    let marker_claim = m.claim(
        node(3),
        ClaimKind::Task,
        NativePhase::Block,
        HostRole::Block,
        &[span(2, marker_end)],
    );
    let object = m.atom(
        node(3),
        AtomKind::Object,
        (0, 1),
        &[span(2, marker_end)],
        marker_claim,
        AtomContent::Object(ObjectId::from_index(0).unwrap()),
    );
    let space_claim = m.claim(
        node(2),
        ClaimKind::Whitespace,
        NativePhase::Block,
        HostRole::Block,
        &[span(marker_end, marker_end + 1)],
    );
    m.atom(
        node(2),
        AtomKind::Hidden,
        (1, 1),
        &[span(marker_end, marker_end + 1)],
        space_claim,
        AtomContent::Empty,
    );
    let anchor = span(marker_end + 1, marker_end + 1);
    let separator = m.claim(
        node(3),
        ClaimKind::ProjectionSeparator(SyntheticKind::TaskLabelSpace),
        NativePhase::Structure,
        HostRole::Structure,
        &[anchor],
    );
    let value = m.builder.text(" ", &mut m.op).unwrap();
    m.atom(
        node(3),
        AtomKind::Synthetic,
        (1, 2),
        &[anchor],
        separator,
        AtomContent::Pool(value),
    );
    let claim = m.claim(
        node(4),
        ClaimKind::Block,
        NativePhase::Block,
        HostRole::Block,
        &[span(end - 1, end)],
    );
    m.atom(
        node(4),
        AtomKind::Copy,
        (2, 3),
        &[span(end - 1, end)],
        claim,
        AtomContent::Empty,
    );
    m.seal(3);
    let marker = m
        .builder
        .source_parts(&[span(2, marker_end)], &mut m.op)
        .unwrap();
    let spacer = m
        .builder
        .source_parts(&[span(marker_end, marker_end + 1)], &mut m.op)
        .unwrap();
    m.builder
        .object(
            ObjectRow::Task {
                node: node(3),
                atom: object,
                item: node(2),
                checked,
                marker,
                spacer,
                action: action.map(|expected| TaskAction {
                    source: span(2, marker_end),
                    expected,
                }),
            },
            &mut m.op,
        )
        .unwrap();
    m
}

#[test]
fn task_actions_require_three_original_contiguous_bytes() {
    let mut m = task("[x]", true, Some(*b"[x]"));
    assert_eq!(m.validate(), Ok(()));
    let mut m = task("[\t]", false, None);
    assert_eq!(m.validate(), Ok(()));
    let mut m = task("[\r\n]", false, None);
    assert_eq!(m.validate(), Ok(()));
    let mut m = task("[\t]", false, Some(*b"[ ]"));
    assert_eq!(m.validate(), Err(CoreError::InvalidProvenance));
    let mut m = task("[x]", false, None);
    assert_eq!(m.validate(), Err(CoreError::InvalidProvenance));
}

#[test]
fn validation_cancel_and_wrong_budget_preserve_first_fault() {
    let mut m = strong();
    let cancel = CancellationToken::new();
    cancel.cancel();
    m.op = m.source.budget().begin(cancel);
    assert_eq!(m.validate(), Err(CoreError::Cancelled));
    assert_eq!(m.op.first_fault(), Some(CoreError::Cancelled));
    let mut m = strong();
    m.op = SourceBudget::model_fixture().begin(CancellationToken::new());
    assert_eq!(m.validate(), Err(CoreError::WrongBudget));
    assert_eq!(m.op.first_fault(), Some(CoreError::WrongBudget));
}

#[test]
fn manual_definition_only_has_full_main_scope_and_definition_payloads() {
    let mut m = Manual::new("[id]: /u");
    let claim = m.claim(
        node(0),
        ClaimKind::ReferenceDefinition,
        NativePhase::Block,
        HostRole::Block,
        &[span(0, 8)],
    );
    m.atom(
        node(0),
        AtomKind::Hidden,
        (0, 0),
        &[span(0, 8)],
        claim,
        AtomContent::Empty,
    );
    m.seal(0);
    let id = DefinitionId::from_index(0).unwrap();
    let label = m.payload(
        PayloadOwner::Definition(id),
        node(0),
        PayloadRole::ReferenceLabel,
        span(1, 3),
        "id",
    );
    let destination = m.payload(
        PayloadOwner::Definition(id),
        node(0),
        PayloadRole::Destination,
        span(6, 8),
        "/u",
    );
    let key = m.builder.text("id", &mut m.op).unwrap();
    let syntax = m.builder.source_parts(&[span(0, 8)], &mut m.op).unwrap();
    m.builder
        .definition(
            DefinitionRow {
                syntax,
                label,
                destination,
                title: None,
                key,
                winner: true,
                claim,
            },
            &mut m.op,
        )
        .unwrap();
    assert_eq!(m.validate(), Ok(()));
    assert_eq!(m.builder.ir.flows()[0].logical_utf16, 0);
    assert_eq!(
        m.builder.ir.flows()[0]
            .domains
            .slice(m.builder.ir.sources())
            .unwrap(),
        &[span(0, 8)]
    );
}

fn nested_image() -> Manual {
    let mut m = Manual::new("![![](v)](u)");
    m.image_nodes(span(0, 12), (0, 1), &[span(2, 8)], 1);
    m.builder
        .node(
            NodeRow {
                parent: Some(node(2)),
                first_child: None,
                next_sibling: None,
                flow: flow(1),
                kind: NodeKind::Image,
                detail: None,
                source_envelope: span(2, 8),
                logical: logical(0, 1),
            },
            &mut m.op,
        )
        .unwrap();
    let mut alt_root = m.builder.ir.nodes()[2];
    alt_root.first_child = Some(node(3));
    m.builder.ir.nodes.replace(2, alt_root, &mut m.op).unwrap();
    m.builder
        .node(
            NodeRow {
                parent: None,
                first_child: None,
                next_sibling: None,
                flow: flow(2),
                kind: NodeKind::AltRoot,
                detail: None,
                source_envelope: span(4, 4),
                logical: logical(0, 0),
            },
            &mut m.op,
        )
        .unwrap();
    let empty_domains = m.builder.source_parts(&[span(4, 4)], &mut m.op).unwrap();
    m.builder
        .flow(
            FlowRow {
                scope: FlowScope::ImageAlt {
                    object: ObjectId::from_index(1).unwrap(),
                },
                root: node(4),
                domains: empty_domains,
                atoms: rows(2, 0),
                logical_utf16: 0,
            },
            &mut m.op,
        )
        .unwrap();
    let outer_claim = m.claim(
        node(1),
        ClaimKind::Image,
        NativePhase::Inline,
        HostRole::Inline,
        &[span(0, 12)],
    );
    let outer = m.atom(
        node(1),
        AtomKind::Object,
        (0, 1),
        &[span(0, 12)],
        outer_claim,
        AtomContent::Object(ObjectId::from_index(0).unwrap()),
    );
    m.seal(1);
    let inner_claim = m.claim(
        node(3),
        ClaimKind::Image,
        NativePhase::Inline,
        HostRole::Inline,
        &[span(2, 8)],
    );
    let inner = m.atom(
        node(3),
        AtomKind::Object,
        (0, 1),
        &[span(2, 8)],
        inner_claim,
        AtomContent::Object(ObjectId::from_index(1).unwrap()),
    );
    m.seal_alt(1);
    let outer_destination = m.payload(
        PayloadOwner::Node(node(1)),
        node(1),
        PayloadRole::Destination,
        span(10, 11),
        "u",
    );
    let inner_destination = m.payload(
        PayloadOwner::Node(node(3)),
        node(3),
        PayloadRole::Destination,
        span(6, 7),
        "v",
    );
    m.image_object(outer, outer_destination, None);
    m.builder
        .object(
            ObjectRow::Image {
                node: node(3),
                atom: inner,
                alt: flow(2),
                destination: inner_destination,
                title: None,
                reference: None,
                wiki_size: None,
            },
            &mut m.op,
        )
        .unwrap();
    m
}

#[test]
fn nested_alt_ownership_terminates_at_main_and_cycles_fail() {
    let mut m = nested_image();
    assert_eq!(m.validate(), Ok(()));
    let mut outer = m.builder.ir.objects()[0];
    let mut inner = m.builder.ir.objects()[1];
    if let ObjectRow::Image { ref mut alt, .. } = outer {
        *alt = flow(2);
    }
    if let ObjectRow::Image { ref mut alt, .. } = inner {
        *alt = flow(1);
    }
    m.builder.ir.objects.replace(0, outer, &mut m.op).unwrap();
    m.builder.ir.objects.replace(1, inner, &mut m.op).unwrap();
    let mut alt = m.builder.ir.flows()[1];
    alt.scope = FlowScope::ImageAlt {
        object: ObjectId::from_index(1).unwrap(),
    };
    m.builder.ir.flows.replace(1, alt, &mut m.op).unwrap();
    let mut alt = m.builder.ir.flows()[2];
    alt.scope = FlowScope::ImageAlt {
        object: ObjectId::from_index(0).unwrap(),
    };
    m.builder.ir.flows.replace(2, alt, &mut m.op).unwrap();
    assert_eq!(m.validate(), Err(CoreError::InvalidProvenance));
}

#[test]
fn validation_depth_is_finite_and_scratch_leases_drop_on_failure() {
    let mut m = Manual::new("");
    let limit = m.op.budget().model_limits().unwrap().depth;
    for index in 1..=limit as usize {
        let mut parent = m.builder.ir.nodes()[index - 1];
        parent.first_child = Some(node(index));
        m.builder
            .ir
            .nodes
            .replace(index - 1, parent, &mut m.op)
            .unwrap();
        m.builder
            .node(
                NodeRow {
                    parent: Some(node(index - 1)),
                    first_child: None,
                    next_sibling: None,
                    flow: flow(0),
                    kind: NodeKind::BlockQuote,
                    detail: None,
                    source_envelope: span(0, 0),
                    logical: logical(0, 0),
                },
                &mut m.op,
            )
            .unwrap();
    }
    let before = m.source.budget().usage();
    assert_eq!(
        m.validate(),
        Err(CoreError::ResourceLimit {
            kind: LimitKind::Depth,
            requested: limit + 1,
            limit
        })
    );
    assert_eq!(
        m.source.budget().usage().auxiliary_capacity,
        before.auxiliary_capacity
    );
    assert_eq!(
        m.source.budget().usage().source_owners,
        before.source_owners
    );
    let mut m = compound_image();
    let mut alt = m.builder.ir.flows()[1];
    alt.atoms = rows(0, 3);
    m.builder.ir.flows.replace(1, alt, &mut m.op).unwrap();
    let before = m.source.budget().usage();
    assert_eq!(m.validate(), Err(CoreError::InvalidProvenance));
    assert_eq!(
        m.source.budget().usage().auxiliary_capacity,
        before.auxiliary_capacity
    );
}

fn attach_empty_proof(
    m: &mut Manual,
    atom: AtomId,
    source: SourceSpan,
    claim: ClaimId,
    kind: StageKind,
) {
    let value = m.builder.text("", &mut m.op).unwrap();
    let primitives = m
        .builder
        .primitive_parts(
            &[PrimitiveRow {
                source,
                kind: PrimitiveKind::Copy,
            }],
            &mut m.op,
        )
        .unwrap();
    let stages = m
        .builder
        .stage_parts(
            &[StageRow {
                kind,
                input: local(0, source.len()),
                output: value,
                parameter: 0,
                accepted_by: claim,
            }],
            &mut m.op,
        )
        .unwrap();
    let proof = m
        .builder
        .proof(
            ProofRow {
                atom,
                primitives,
                stages,
                value,
            },
            &mut m.op,
        )
        .unwrap();
    let mut row = m.builder.ir.test_atom(atom.index());
    row.proof = Some(proof);
    m.builder
        .ir
        .set_test_atom(atom.index(), row, &mut m.op)
        .unwrap();
}

#[test]
fn code_edge_deletion_is_hidden_with_an_independent_empty_proof() {
    let mut m = Manual::new(" x ");
    let claim = m.claim(
        node(0),
        ClaimKind::Code,
        NativePhase::Inline,
        HostRole::Inline,
        &[span(0, 3)],
    );
    let first = m.atom(
        node(0),
        AtomKind::Hidden,
        (0, 0),
        &[span(0, 1)],
        claim,
        AtomContent::Empty,
    );
    attach_empty_proof(&mut m, first, span(0, 1), claim, StageKind::CodeEdgeTrim);
    m.copy(span(1, 2), 0, 1);
    let last = m.atom(
        node(0),
        AtomKind::Hidden,
        (1, 1),
        &[span(2, 3)],
        claim,
        AtomContent::Empty,
    );
    attach_empty_proof(&mut m, last, span(2, 3), claim, StageKind::CodeEdgeTrim);
    m.seal(1);
    assert_eq!(m.validate(), Ok(()));
    let mut stage = m.builder.ir.stages()[0];
    stage.kind = StageKind::AsciiTrim;
    m.builder.ir.stages.replace(0, stage, &mut m.op).unwrap();
    assert_eq!(m.validate(), Err(CoreError::InvalidProvenance));
}

fn fence_info_fixture(
    raw: &str,
    value: &str,
) -> (Manual, NodeId, ClaimId, TextSlice, Rows<SourceSpan>) {
    let text = format!("```{raw}\n```");
    let mut m = Manual::new(&text);
    let end = text.len() as u64;
    let info = span(3, 3 + raw.len() as u64);
    let owner = m
        .builder
        .node(
            NodeRow {
                parent: Some(node(0)),
                first_child: None,
                next_sibling: None,
                flow: flow(0),
                kind: NodeKind::CodeBlock,
                detail: None,
                source_envelope: span(0, end),
                logical: logical(0, 0),
            },
            &mut m.op,
        )
        .unwrap();
    let mut root = m.builder.ir.nodes()[0];
    root.first_child = Some(owner);
    m.builder.ir.nodes.replace(0, root, &mut m.op).unwrap();
    let code = m.claim(
        owner,
        ClaimKind::Code,
        NativePhase::Block,
        HostRole::Block,
        &[span(0, end)],
    );
    m.atom(
        owner,
        AtomKind::Hidden,
        (0, 0),
        &[span(0, end)],
        code,
        AtomContent::Empty,
    );
    m.seal(0);
    for kind in [AttributeKind::CodeForm, AttributeKind::CodeFence] {
        let sources = m.builder.source_parts(&[span(0, 3)], &mut m.op).unwrap();
        m.builder
            .attribute(
                AttributeRow {
                    owner,
                    kind,
                    claim: code,
                    sources,
                },
                &mut m.op,
            )
            .unwrap();
    }
    let domains = m.builder.source_parts(&[info], &mut m.op).unwrap();
    let value = m.builder.text(value, &mut m.op).unwrap();
    (m, owner, code, value, domains)
}

fn finish_fence_info(
    m: &mut Manual,
    owner: NodeId,
    code: ClaimId,
    value: TextSlice,
    domains: Rows<SourceSpan>,
    atoms: Rows<AtomRow>,
) {
    let info = m
        .builder
        .payload(
            PayloadRow {
                owner: PayloadOwner::Node(owner),
                role: PayloadRole::FenceInfo,
                value,
                domains,
                atoms,
                utf8_len: value.len() as u64,
            },
            &mut m.op,
        )
        .unwrap();
    let detail = m
        .builder
        .detail(
            DetailRow::Code {
                form: CodeForm::Fenced {
                    byte: b'`',
                    length: 3,
                },
                info: Some(info),
                claim: code,
            },
            &mut m.op,
        )
        .unwrap();
    let mut row = m.builder.ir.nodes()[owner.index()];
    row.detail = Some(detail);
    m.builder
        .ir
        .nodes
        .replace(owner.index(), row, &mut m.op)
        .unwrap();
}

fn trimmed_entity_info() -> Manual {
    let (mut m, owner, code, empty, domains) = fence_info_fixture("&NewLine;", "");
    let source = span(3, 12);
    let entity = m.claim(
        owner,
        ClaimKind::Entity,
        NativePhase::Metadata,
        HostRole::FenceInfo,
        &[source],
    );
    let trim = m.claim(
        owner,
        ClaimKind::Payload,
        NativePhase::Metadata,
        HostRole::FenceInfo,
        &[source],
    );
    let newline = m.builder.text("\n", &mut m.op).unwrap();
    let start = m.builder.ir.atoms_len();
    let atom = m.atom(
        owner,
        AtomKind::Hidden,
        (0, 0),
        &[source],
        trim,
        AtomContent::Empty,
    );
    let primitives = m
        .builder
        .primitive_parts(
            &[PrimitiveRow {
                source,
                kind: PrimitiveKind::Copy,
            }],
            &mut m.op,
        )
        .unwrap();
    let stages = m
        .builder
        .stage_parts(
            &[
                StageRow {
                    kind: StageKind::NamedEntity,
                    input: local(0, 9),
                    output: newline,
                    parameter: 0,
                    accepted_by: entity,
                },
                StageRow {
                    kind: StageKind::AsciiTrim,
                    input: local(0, 1),
                    output: empty,
                    parameter: 0,
                    accepted_by: trim,
                },
            ],
            &mut m.op,
        )
        .unwrap();
    let proof = m
        .builder
        .proof(
            ProofRow {
                atom,
                primitives,
                stages,
                value: empty,
            },
            &mut m.op,
        )
        .unwrap();
    let mut row = m.builder.ir.test_atom(atom.index());
    row.proof = Some(proof);
    m.builder
        .ir
        .set_test_atom(atom.index(), row, &mut m.op)
        .unwrap();
    finish_fence_info(&mut m, owner, code, empty, domains, rows(start, 1));
    m
}

#[test]
fn fence_info_entity_then_trim_can_delete_a_connected_closure() {
    let mut m = trimmed_entity_info();
    assert_eq!(m.validate(), Ok(()));
    for role in [HostRole::Destination, HostRole::Title] {
        let mut m = trimmed_entity_info();
        for stage in 0..2 {
            let claim = m.builder.ir.stages()[stage].accepted_by;
            let mut authority = m.builder.ir.test_authority(claim);
            authority.host.role = role;
            m.builder
                .ir
                .set_test_authority(claim, authority, &mut m.op)
                .unwrap();
        }
        let mut payload = m.builder.ir.payloads()[0];
        payload.role = if role == HostRole::Title {
            PayloadRole::Title
        } else {
            PayloadRole::Destination
        };
        m.builder
            .ir
            .payloads
            .replace(0, payload, &mut m.op)
            .unwrap();
        assert_eq!(m.validate(), Err(CoreError::InvalidProvenance));
    }
}

#[test]
fn long_ascii_trim_uses_bounded_windows_and_preserves_literal_remainder() {
    let raw = format!("{}x", " ".repeat(65_537));
    let (mut m, owner, code, value, domains) = fence_info_fixture(&raw, "x");
    let source = span(3, 65_540);
    let remainder = span(65_540, 65_541);
    let trim = m.claim(
        owner,
        ClaimKind::Payload,
        NativePhase::Metadata,
        HostRole::FenceInfo,
        &[source],
    );
    let start = m.builder.ir.atoms_len();
    let hidden = m.atom(
        owner,
        AtomKind::Hidden,
        (0, 0),
        &[source],
        trim,
        AtomContent::Empty,
    );
    attach_empty_proof(&mut m, hidden, source, trim, StageKind::AsciiTrim);
    let literal = m.claim(
        owner,
        ClaimKind::Payload,
        NativePhase::Metadata,
        HostRole::FenceInfo,
        &[remainder],
    );
    m.atom(
        owner,
        AtomKind::Copy,
        (0, 1),
        &[remainder],
        literal,
        AtomContent::Empty,
    );
    finish_fence_info(&mut m, owner, code, value, domains, rows(start, 2));
    assert_eq!(m.validate(), Ok(()));
}

fn table_with_missing_cell() -> Manual {
    let mut m = Manual::new("A|B\n:-|--:\nx|\n");
    for row in [
        NodeRow {
            parent: Some(node(0)),
            first_child: Some(node(2)),
            next_sibling: None,
            flow: flow(0),
            kind: NodeKind::Table,
            detail: None,
            source_envelope: span(0, 14),
            logical: logical(0, 6),
        },
        NodeRow {
            parent: Some(node(1)),
            first_child: Some(node(3)),
            next_sibling: Some(node(5)),
            flow: flow(0),
            kind: NodeKind::TableRow,
            detail: None,
            source_envelope: span(0, 4),
            logical: logical(0, 3),
        },
        NodeRow {
            parent: Some(node(2)),
            first_child: None,
            next_sibling: Some(node(4)),
            flow: flow(0),
            kind: NodeKind::TableCell,
            detail: None,
            source_envelope: span(0, 1),
            logical: logical(0, 1),
        },
        NodeRow {
            parent: Some(node(2)),
            first_child: None,
            next_sibling: None,
            flow: flow(0),
            kind: NodeKind::TableCell,
            detail: None,
            source_envelope: span(2, 3),
            logical: logical(2, 3),
        },
        NodeRow {
            parent: Some(node(1)),
            first_child: Some(node(6)),
            next_sibling: None,
            flow: flow(0),
            kind: NodeKind::TableRow,
            detail: None,
            source_envelope: span(11, 14),
            logical: logical(4, 6),
        },
        NodeRow {
            parent: Some(node(5)),
            first_child: None,
            next_sibling: Some(node(7)),
            flow: flow(0),
            kind: NodeKind::TableCell,
            detail: None,
            source_envelope: span(11, 12),
            logical: logical(4, 5),
        },
        NodeRow {
            parent: Some(node(5)),
            first_child: None,
            next_sibling: None,
            flow: flow(0),
            kind: NodeKind::TableCell,
            detail: None,
            source_envelope: span(13, 13),
            logical: logical(6, 6),
        },
    ] {
        m.builder.node(row, &mut m.op).unwrap();
    }
    let mut root = m.builder.ir.nodes()[0];
    root.first_child = Some(node(1));
    m.builder.ir.nodes.replace(0, root, &mut m.op).unwrap();
    let table = m.claim(
        node(1),
        ClaimKind::Table,
        NativePhase::Block,
        HostRole::Block,
        &[span(0, 14)],
    );
    let alignments = m
        .builder
        .alignment_parts(&[Alignment::Left, Alignment::Right], &mut m.op)
        .unwrap();
    let detail = m
        .builder
        .detail(
            DetailRow::Table {
                columns: 2,
                alignments,
                claim: table,
            },
            &mut m.op,
        )
        .unwrap();
    let mut row = m.builder.ir.nodes()[1];
    row.detail = Some(detail);
    m.builder.ir.nodes.replace(1, row, &mut m.op).unwrap();
    for kind in [AttributeKind::TableColumns, AttributeKind::TableAlignment] {
        let sources = m
            .builder
            .source_parts(&[span(4, 6), span(7, 10)], &mut m.op)
            .unwrap();
        m.builder
            .attribute(
                AttributeRow {
                    owner: node(1),
                    kind,
                    claim: table,
                    sources,
                },
                &mut m.op,
            )
            .unwrap();
    }
    let mut claims = [table; 8];
    for (index, column, missing, row_index, header) in [
        (2, None, false, 0, true),
        (3, Some(0), false, 0, true),
        (4, Some(1), false, 0, true),
        (5, None, false, 1, false),
        (6, Some(0), false, 1, false),
        (7, Some(1), true, 1, false),
    ] {
        let envelope = m.builder.ir.nodes()[index].source_envelope;
        let claim = m.claim(
            node(index),
            ClaimKind::Table,
            NativePhase::Block,
            HostRole::Block,
            &[envelope],
        );
        claims[index] = claim;
        let detail = if let Some(column) = column {
            DetailRow::Cell {
                column,
                missing,
                claim,
            }
        } else {
            DetailRow::Row {
                index: row_index,
                header,
                claim,
            }
        };
        let id = m.builder.detail(detail, &mut m.op).unwrap();
        let mut row = m.builder.ir.nodes()[index];
        row.detail = Some(id);
        m.builder.ir.nodes.replace(index, row, &mut m.op).unwrap();
        let sources = m.builder.source_parts(&[envelope], &mut m.op).unwrap();
        let kind = if column.is_some() {
            AttributeKind::CellMissing
        } else {
            AttributeKind::RowHeader
        };
        m.builder
            .attribute(
                AttributeRow {
                    owner: node(index),
                    kind,
                    claim,
                    sources,
                },
                &mut m.op,
            )
            .unwrap();
    }
    m.atom(
        node(3),
        AtomKind::Copy,
        (0, 1),
        &[span(0, 1)],
        claims[3],
        AtomContent::Empty,
    );
    m.atom(
        node(2),
        AtomKind::Hidden,
        (1, 1),
        &[span(1, 2)],
        claims[2],
        AtomContent::Empty,
    );
    let tab = m.builder.text("\t", &mut m.op).unwrap();
    let newline = m.builder.text("\n", &mut m.op).unwrap();
    {
        let (owner, source, target, kind, value) =
            (node(4), span(2, 2), (1, 2), SyntheticKind::CellTab, tab);
        let claim = m.claim(
            owner,
            ClaimKind::ProjectionSeparator(kind),
            NativePhase::Structure,
            HostRole::Structure,
            &[source],
        );
        m.atom(
            owner,
            AtomKind::Synthetic,
            target,
            &[source],
            claim,
            AtomContent::Pool(value),
        );
    }
    m.atom(
        node(4),
        AtomKind::Copy,
        (2, 3),
        &[span(2, 3)],
        claims[4],
        AtomContent::Empty,
    );
    m.atom(
        node(1),
        AtomKind::Hidden,
        (3, 3),
        &[span(3, 11)],
        table,
        AtomContent::Empty,
    );
    let claim = m.claim(
        node(5),
        ClaimKind::ProjectionSeparator(SyntheticKind::RowLineFeed),
        NativePhase::Structure,
        HostRole::Structure,
        &[span(11, 11)],
    );
    m.atom(
        node(5),
        AtomKind::Synthetic,
        (3, 4),
        &[span(11, 11)],
        claim,
        AtomContent::Pool(newline),
    );
    m.atom(
        node(6),
        AtomKind::Copy,
        (4, 5),
        &[span(11, 12)],
        claims[6],
        AtomContent::Empty,
    );
    m.atom(
        node(5),
        AtomKind::Hidden,
        (5, 5),
        &[span(12, 14)],
        claims[5],
        AtomContent::Empty,
    );
    let claim = m.claim(
        node(7),
        ClaimKind::ProjectionSeparator(SyntheticKind::CellTab),
        NativePhase::Structure,
        HostRole::Structure,
        &[span(13, 13)],
    );
    m.atom(
        node(7),
        AtomKind::Synthetic,
        (5, 6),
        &[span(13, 13)],
        claim,
        AtomContent::Pool(tab),
    );
    m.seal(6);
    m
}

#[test]
fn table_alignment_missing_cell_and_external_separators_are_validated() {
    let mut m = table_with_missing_cell();
    assert_eq!(m.validate(), Ok(()));
    assert_eq!(m.builder.ir.nodes()[7].logical, logical(6, 6));
    assert_eq!(
        m.builder.ir.test_atom(8).target,
        TargetSpan { start: 5, end: 6 }
    );
    m.builder
        .ir
        .alignments
        .replace(0, Alignment::Right, &mut m.op)
        .unwrap();
    assert_eq!(m.validate(), Err(CoreError::InvalidProvenance));
    let mut m = table_with_missing_cell();
    let mut main = m.builder.ir.flows()[0];
    main.domains = rows(0, 0);
    m.builder.ir.flows.replace(0, main, &mut m.op).unwrap();
    assert_eq!(m.validate(), Err(CoreError::InvalidProvenance));
}

#[test]
fn review_zero_anchor_authority_hole() {
    let budget = SourceBudget::model_fixture();
    let mut op = budget.begin(CancellationToken::new());
    let result = crate::policy::construction_scope(&mut op, |mut work| {
        let result = super::replay::covered(
            span(5, 5),
            (&[span(0, 1), span(10, 11)][..]).into(),
            &mut work,
        );
        let _ = work.finish();
        result
    });
    println!("hole anchor5 in domains[0,1),[10,11): {:?}", result);
    assert_eq!(result, Ok(false));
}

#[test]
fn review_payload_can_steal_definition_atom_owner() {
    let mut m = reference();
    let original_index = m.builder.ir.payloads()[1].atoms.start();
    let occurrence_index = m.builder.ir.payloads()[2].atoms.start();
    let original = m.builder.ir.test_atom(original_index);
    let mut occurrence = m.builder.ir.test_atom(occurrence_index);
    occurrence.owner = original.owner;
    occurrence.claim = original.claim;
    m.builder
        .ir
        .set_test_atom(occurrence_index, occurrence, &mut m.op)
        .unwrap();
    let result = m.validate();
    println!("use payload borrows definition owner/claim: {:?}", result);
    assert_eq!(result, Err(CoreError::InvalidProvenance));
}

#[test]
fn review_code_newline_then_edge_trim() {
    let mut m = Manual::new("\nx\n");
    let claim = m.claim(
        node(0),
        ClaimKind::Code,
        NativePhase::Inline,
        HostRole::Inline,
        &[span(0, 3)],
    );
    for (raw, at) in [(span(0, 1), 0), (span(2, 3), 1)] {
        let atom = m.atom(
            node(0),
            AtomKind::Hidden,
            (at, at),
            &[raw],
            claim,
            AtomContent::Empty,
        );
        let blank = m.builder.text(" ", &mut m.op).unwrap();
        let empty = m.builder.text("", &mut m.op).unwrap();
        let primitives = m
            .builder
            .primitive_parts(
                &[PrimitiveRow {
                    source: raw,
                    kind: PrimitiveKind::Copy,
                }],
                &mut m.op,
            )
            .unwrap();
        let stages = m
            .builder
            .stage_parts(
                &[
                    StageRow {
                        kind: StageKind::CodeLineEndingToSpace,
                        input: local(0, 1),
                        output: blank,
                        parameter: 0,
                        accepted_by: claim,
                    },
                    StageRow {
                        kind: StageKind::CodeEdgeTrim,
                        input: local(0, 1),
                        output: empty,
                        parameter: 0,
                        accepted_by: claim,
                    },
                ],
                &mut m.op,
            )
            .unwrap();
        let proof = m
            .builder
            .proof(
                ProofRow {
                    atom,
                    primitives,
                    stages,
                    value: empty,
                },
                &mut m.op,
            )
            .unwrap();
        let mut row = m.builder.ir.test_atom(atom.index());
        row.proof = Some(proof);
        m.builder
            .ir
            .set_test_atom(atom.index(), row, &mut m.op)
            .unwrap();
    }
    m.copy(span(1, 2), 0, 1);
    m.seal(1);
    let result = m.validate();
    println!("CodeLineEndingToSpace -> CodeEdgeTrim: {:?}", result);
    assert_eq!(result, Ok(()));
}

#[test]
fn review_hidden_complexity_with_fully_validated_ir() {
    let count = 4096u64;
    let mut m = Manual::new(&"x".repeat(count as usize));
    let pieces: Vec<_> = (0..count).map(|x| span(x, x + 1)).collect();
    let claim = m.claim(
        node(0),
        ClaimKind::Delimiter,
        NativePhase::Inline,
        HostRole::Inline,
        &[span(0, count)],
    );
    m.atom(
        node(0),
        AtomKind::Hidden,
        (0, 0),
        &pieces,
        claim,
        AtomContent::Empty,
    );
    m.seal(0);
    assert_eq!(m.validate(), Ok(()));
    let identity = crate::identity::ModelIdentity::new(
        m.source.key(),
        1,
        crate::config::ParseConfig::new(crate::config::Profile::C0),
    )
    .unwrap();
    let frozen = m.builder.ir.snapshot(&mut m.op).unwrap();
    let index = crate::mapping::MapIndex::build(&frozen, &m.source, identity, &mut m.op).unwrap();
    let mut op = m.source.budget().begin(CancellationToken::new());
    let response = index
        .map_range(
            &frozen,
            &m.source,
            &crate::mapping::RangeQuery {
                model: identity,
                input: crate::mapping::RangeInput::Source {
                    target: TextTarget::Flow(flow(0)),
                    span: span(count - 2, count - 1),
                },
                empty_affinity: crate::mapping::Affinity::Before,
            },
            &mut op,
        )
        .unwrap();
    println!(
        "full validated Hidden IR: content={}, contributors={}, events={}",
        response.contributions().len(),
        response.contributors().len(),
        op.stats().expect("statistics enabled").events()
    );
    assert_eq!(response.contributions().len(), 1);
    assert!(op.stats().expect("statistics enabled").events() < 300);
}

#[test]
fn valid_owned_copy_is_read_once_and_unused_valid_pool_needs_no_rescan() {
    let length = 65_536;
    let mut m = Manual::new(&"x".repeat(length));
    m.copy(span(0, length as u64), 0, length as u64);
    m.builder.text(&"😀".repeat(length), &mut m.op).unwrap();
    m.seal(length as u64);
    // Test-only fixture freezing is outside the measured validator/replay work.
    let frozen = m.builder.ir.snapshot(&mut m.op).unwrap();
    let before = m.op.stats().expect("statistics enabled").scanned_bytes();
    assert_eq!(
        super::validate::validate(&frozen, &m.source, &mut m.op),
        Ok(())
    );
    // The source is scanned once; six atom views and three complete Extra
    // permission views read only fixed metadata, including one selector.
    let view =
        (size_of::<AtomFields>() + size_of::<ClaimId>()) as u64 + types::AUTHORITY_SELECTOR_BYTES;
    let selector = types::AUTHORITY_SELECTOR_BYTES + size_of::<ClaimId>() as u64;
    assert_eq!(
        m.op.stats().expect("statistics enabled").scanned_bytes() - before,
        length as u64 + 6 * view + 3 * size_of::<ExtraAuthority>() as u64 + selector
    );
}

#[test]
fn borrowed_copy_still_rejects_cr_nul_bad_source_boundaries_and_wrong_length() {
    for text in ["\r", "\0"] {
        let mut m = Manual::new(text);
        m.copy(span(0, 1), 0, 1);
        m.seal(1);
        assert_eq!(m.validate(), Err(CoreError::InvalidProvenance));
    }
    let mut split = Manual::new("😀");
    split.copy(span(0, 1), 0, 1);
    split.seal(1);
    assert_eq!(split.validate(), Err(CoreError::InvalidBoundary));
    let mut wrong = Manual::new("😀");
    wrong.copy(span(0, 4), 0, 4);
    wrong.seal(4);
    assert_eq!(wrong.validate(), Err(CoreError::InvalidProvenance));
    let mut tail = Manual::new("\r\n");
    tail.hidden(span(0, 1), 0);
    tail.copy(span(1, 2), 0, 1);
    tail.seal(1);
    assert_eq!(tail.validate(), Err(CoreError::InvalidProvenance));
}

#[test]
fn interior_node_boundaries_stop_at_the_prefix_and_reject_half_a_surrogate() {
    for (position, valid) in [(3, true), (2, false)] {
        let text = format!("a😀{}", "x".repeat(65_536));
        let mut m = Manual::new(&text);
        let length = text.encode_utf16().count() as u64;
        m.copy(span(0, text.len() as u64), 0, length);
        m.builder
            .node(
                NodeRow {
                    parent: Some(node(0)),
                    first_child: None,
                    next_sibling: None,
                    flow: flow(0),
                    kind: NodeKind::Text,
                    detail: None,
                    source_envelope: span(1, 5),
                    logical: logical(1, position),
                },
                &mut m.op,
            )
            .unwrap();
        let mut root = m.builder.ir.nodes()[0];
        root.first_child = Some(node(1));
        m.builder.ir.nodes.replace(0, root, &mut m.op).unwrap();
        m.seal(length);
        // Test-only fixture freezing is outside the measured validator/replay work.
        let frozen = m.builder.ir.snapshot(&mut m.op).unwrap();
        let before = m.op.stats().expect("statistics enabled").scanned_bytes();
        assert_eq!(
            super::validate::validate(&frozen, &m.source, &mut m.op),
            if valid {
                Ok(())
            } else {
                Err(CoreError::InvalidProvenance)
            }
        );
        // Copy totals are checked fully; the two node boundaries read a + a😀.
        // Failure stops before the last two fixed atom views.
        let view = (size_of::<AtomFields>() + size_of::<ClaimId>()) as u64
            + types::AUTHORITY_SELECTOR_BYTES;
        let selector = types::AUTHORITY_SELECTOR_BYTES + size_of::<ClaimId>() as u64;
        assert_eq!(
            m.op.stats().expect("statistics enabled").scanned_bytes() - before,
            text.len() as u64
                + 6
                + if valid { 10 } else { 8 } * view
                + 3 * size_of::<ExtraAuthority>() as u64
                + selector
        );
    }
}

#[test]
fn inline_html_break_requires_a_transform_owned_by_the_break_node() {
    let mut model = Manual::new("AB<br>");
    model.copy(span(0, 2), 0, 2);
    model.hidden(span(2, 6), 2);
    let owner = node(1);
    let claim = model.claim(
        owner,
        ClaimKind::Html,
        NativePhase::Inline,
        HostRole::Inline,
        &[span(2, 6)],
    );
    let detail = model
        .builder
        .detail(DetailRow::Html { claim }, &mut model.op)
        .unwrap();
    assert_eq!(
        model
            .builder
            .node(
                NodeRow {
                    parent: Some(node(0)),
                    first_child: None,
                    next_sibling: None,
                    flow: flow(0),
                    kind: NodeKind::HardBreak,
                    detail: Some(detail),
                    source_envelope: span(2, 6),
                    logical: logical(1, 2),
                },
                &mut model.op,
            )
            .unwrap(),
        owner,
    );
    let mut root = model.builder.ir.nodes()[0];
    root.first_child = Some(owner);
    model
        .builder
        .ir
        .nodes
        .replace(0, root, &mut model.op)
        .unwrap();
    model.seal(2);
    let ir = model.builder.ir.snapshot(&mut model.op).unwrap();
    assert_eq!(
        super::validate::validate(&ir, &model.source, &mut model.op),
        Err(CoreError::InvalidProvenance),
    );
}

#[test]
fn inline_html_break_replay_rejects_a_shared_authority_with_identical_source() {
    let mut model = Manual::new("<br>");
    let atom = model.transform(
        span(0, 4),
        (0, 1),
        "\n",
        PrimitiveKind::Copy,
        Some(StageKind::HtmlBreakToLineFeed),
        ClaimKind::Html,
    );
    model.seal(1);
    assert_eq!(
        replay_fixture(&model.builder.ir, &model.source, atom, None, &mut model.op),
        Err(CoreError::InvalidProvenance),
    );
}

#[test]
fn inline_html_code_validation_requires_the_entire_visible_body() {
    for (hide_body, trim_syntax) in [(false, false), (true, false), (true, true)] {
        let raw = "<code>ab</code>";
        let mut model = Manual::new(raw);
        let len = raw.len() as u64;
        let width = if hide_body { 0 } else { 2 };
        let host_domains = model.builder.ir.flows()[0].domains;
        let host = HostRow {
            owner: node(2),
            role: HostRole::Inline,
            utf8_len: len,
            domains: host_domains,
        };
        let syntax_sources = if trim_syntax {
            model
                .builder
                .source_parts(&[span(0, 6), span(8, len)], &mut model.op)
                .unwrap()
        } else {
            host_domains
        };
        let detail_claim = model
            .builder
            .claim(
                AuthoritySpec {
                    owner: host.owner,
                    host,
                    phase: NativePhase::Inline,
                    kind: ClaimKind::Html,
                    consumed: local(0, len - (2 - width)),
                    sources: syntax_sources,
                },
                &mut model.op,
            )
            .unwrap();
        let detail = model
            .builder
            .detail(
                DetailRow::Html {
                    claim: detail_claim,
                },
                &mut model.op,
            )
            .unwrap();
        for (owner, parent, child, kind, detail) in [
            (node(1), node(0), Some(node(2)), NodeKind::Paragraph, None),
            (node(2), node(1), None, NodeKind::InlineCode, Some(detail)),
        ] {
            let actual = model
                .builder
                .node(
                    NodeRow {
                        parent: Some(parent),
                        first_child: child,
                        next_sibling: None,
                        flow: flow(0),
                        kind,
                        detail,
                        source_envelope: span(0, len),
                        logical: logical(0, width),
                    },
                    &mut model.op,
                )
                .unwrap();
            assert_eq!(actual, owner);
        }
        let mut root = model.builder.ir.nodes()[0];
        root.first_child = Some(node(1));
        model
            .builder
            .ir
            .nodes
            .replace(0, root, &mut model.op)
            .unwrap();
        let empty = model.builder.text("", &mut model.op).unwrap();
        for (source, target, consumed, is_tag) in [
            (span(0, 6), (0, 0), local(0, 6), true),
            (span(6, 8), (0, width), local(6, 8), false),
            (
                span(8, len),
                (width, width),
                local(6 + width, len - (2 - width)),
                true,
            ),
        ] {
            let sources = model
                .builder
                .source_parts(&[source], &mut model.op)
                .unwrap();
            let proof = if is_tag {
                let atom = AtomId::from_index(model.builder.ir.atoms_len()).unwrap();
                let tag = HtmlCodeDelimiter::parse(
                    &raw.as_bytes()[source.start() as usize..source.end() as usize],
                )
                .unwrap();
                let primitives = model
                    .builder
                    .primitive_parts(
                        &[PrimitiveRow {
                            source,
                            kind: PrimitiveKind::Copy,
                        }],
                        &mut model.op,
                    )
                    .unwrap();
                let stages = model
                    .builder
                    .stage_parts(
                        &[StageRow {
                            kind: StageKind::HtmlCodeDelimiter,
                            input: local(0, source.len()),
                            output: empty,
                            parameter: tag.parameter(),
                            accepted_by: ClaimId::primary(atom),
                        }],
                        &mut model.op,
                    )
                    .unwrap();
                Some(
                    model
                        .builder
                        .proof(
                            ProofRow {
                                atom,
                                primitives,
                                stages,
                                value: empty,
                            },
                            &mut model.op,
                        )
                        .unwrap(),
                )
            } else {
                None
            };
            model
                .builder
                .ir
                .packets
                .push(
                    AtomPacket {
                        atom: AtomFields {
                            owner: if trim_syntax && !is_tag {
                                node(1)
                            } else {
                                host.owner
                            },
                            kind: if is_tag || hide_body {
                                AtomKind::Hidden
                            } else {
                                AtomKind::Copy
                            },
                            target: TargetSpan {
                                start: target.0,
                                end: target.1,
                            },
                            sources,
                            proof,
                            content: AtomContent::Empty,
                        },
                        authority: AuthorityBinding::Owned(PrimaryAuthority {
                            phase: NativePhase::Inline,
                            kind: if is_tag {
                                ClaimKind::Html
                            } else {
                                ClaimKind::Code
                            },
                            consumed,
                            host_role: host.role,
                            host_utf8_len: host.utf8_len,
                            host_domains,
                        }),
                    },
                    &mut model.op,
                )
                .unwrap();
        }
        model.seal(width);
        let expected = if hide_body {
            Err(CoreError::InvalidProvenance)
        } else {
            Ok(())
        };
        assert_eq!(model.validate(), expected);
    }
}

#[test]
fn inline_html_emphasis_validation_requires_both_owned_endpoint_proofs() {
    let mut model = Manual::new("a<b>X</b>z");
    let host_domains = model.builder.ir.flows()[0].domains;
    let paragraph = model
        .builder
        .node(
            NodeRow {
                parent: Some(node(0)),
                first_child: Some(node(2)),
                next_sibling: None,
                flow: flow(0),
                kind: NodeKind::Paragraph,
                detail: None,
                source_envelope: span(0, 10),
                logical: logical(0, 3),
            },
            &mut model.op,
        )
        .unwrap();
    let mut root = model.builder.ir.nodes()[0];
    root.first_child = Some(paragraph);
    model
        .builder
        .ir
        .nodes
        .replace(0, root, &mut model.op)
        .unwrap();
    let value = model.builder.text("", &mut model.op).unwrap();
    model.copy(span(0, 1), 0, 1);
    for (at, raw, parameter) in [(1, span(1, 4), 1), (2, span(5, 9), 33)] {
        let owner = node(1 + at as usize);
        let sources = model.builder.source_parts(&[raw], &mut model.op).unwrap();
        let host = HostRow {
            owner,
            role: HostRole::Inline,
            utf8_len: 10,
            domains: host_domains,
        };
        let consumed = local(raw.start(), raw.end());
        let detail_claim = model
            .builder
            .claim(
                AuthoritySpec {
                    owner,
                    host,
                    phase: NativePhase::Inline,
                    kind: ClaimKind::Html,
                    consumed,
                    sources,
                },
                &mut model.op,
            )
            .unwrap();
        let detail = model
            .builder
            .detail(
                DetailRow::Html {
                    claim: detail_claim,
                },
                &mut model.op,
            )
            .unwrap();
        assert_eq!(
            model
                .builder
                .node(
                    NodeRow {
                        parent: Some(paragraph),
                        first_child: None,
                        next_sibling: (at == 1).then(|| node(3)),
                        flow: flow(0),
                        kind: NodeKind::HtmlInline,
                        detail: Some(detail),
                        source_envelope: raw,
                        logical: logical(at, at),
                    },
                    &mut model.op
                )
                .unwrap(),
            owner
        );
        let atom = AtomId::from_index(model.builder.ir.atoms_len()).unwrap();
        let primitives = model
            .builder
            .primitive_parts(
                &[PrimitiveRow {
                    source: raw,
                    kind: PrimitiveKind::Copy,
                }],
                &mut model.op,
            )
            .unwrap();
        let stages = model
            .builder
            .stage_parts(
                &[StageRow {
                    kind: StageKind::HtmlEmphasisDelimiter,
                    input: local(0, raw.len()),
                    output: value,
                    parameter,
                    accepted_by: ClaimId::primary(atom),
                }],
                &mut model.op,
            )
            .unwrap();
        let proof = model
            .builder
            .proof(
                ProofRow {
                    atom,
                    primitives,
                    stages,
                    value,
                },
                &mut model.op,
            )
            .unwrap();
        model
            .builder
            .ir
            .packets
            .push(
                AtomPacket {
                    atom: AtomFields {
                        owner,
                        kind: AtomKind::Hidden,
                        target: TargetSpan { start: at, end: at },
                        sources,
                        proof: Some(proof),
                        content: AtomContent::Empty,
                    },
                    authority: AuthorityBinding::Owned(PrimaryAuthority {
                        phase: NativePhase::Inline,
                        kind: ClaimKind::Html,
                        consumed,
                        host_role: host.role,
                        host_utf8_len: host.utf8_len,
                        host_domains,
                    }),
                },
                &mut model.op,
            )
            .unwrap();
        if at == 1 {
            model.copy(span(4, 5), 1, 2);
        }
    }
    model.copy(span(9, 10), 2, 3);
    model.seal(3);
    assert_eq!(model.validate(), Ok(()));

    // Swap two visible source ranges while preserving all logical positions
    // and endpoint proofs. Coverage alone would still accept the partition.
    let before = model.builder.ir.packets.as_slice()[0];
    let inside = model.builder.ir.packets.as_slice()[2];
    let mut early = inside;
    let mut late = before;
    early.atom.target = before.atom.target;
    late.atom.target = inside.atom.target;
    model
        .builder
        .ir
        .packets
        .replace(0, early, &mut model.op)
        .unwrap();
    model
        .builder
        .ir
        .packets
        .replace(2, late, &mut model.op)
        .unwrap();
    assert_eq!(model.validate(), Err(CoreError::InvalidProvenance));

    model.op = model.op.budget().begin(CancellationToken::new());
    model
        .builder
        .ir
        .packets
        .replace(0, before, &mut model.op)
        .unwrap();
    model
        .builder
        .ir
        .packets
        .replace(2, inside, &mut model.op)
        .unwrap();
    let close = model.builder.ir.packets.as_slice()[3];
    let mut misplaced = close;
    misplaced.atom.target = TargetSpan { start: 0, end: 0 };
    model
        .builder
        .ir
        .packets
        .replace(3, misplaced, &mut model.op)
        .unwrap();
    assert_eq!(model.validate(), Err(CoreError::InvalidProvenance));

    model.op = model.op.budget().begin(CancellationToken::new());
    let mut wrong_edge = model.builder.ir.stages()[1];
    wrong_edge.parameter = 18; // strong rather than b
    model
        .builder
        .ir
        .packets
        .replace(3, close, &mut model.op)
        .unwrap();
    model
        .builder
        .ir
        .stages
        .replace(1, wrong_edge, &mut model.op)
        .unwrap();
    assert_eq!(model.validate(), Err(CoreError::InvalidProvenance));

    model.op = model.op.budget().begin(CancellationToken::new());
    let invalid_empty = TextSlice {
        start: u32::MAX,
        len: 0,
    };
    wrong_edge.parameter = 17;
    wrong_edge.output = invalid_empty;
    model
        .builder
        .ir
        .stages
        .replace(1, wrong_edge, &mut model.op)
        .unwrap();
    let proof = close.atom.proof.unwrap();
    let mut proof_row = model.builder.ir.proofs()[proof.index()];
    proof_row.value = invalid_empty;
    model
        .builder
        .ir
        .proofs
        .replace(proof.index(), proof_row, &mut model.op)
        .unwrap();
    assert_eq!(model.validate(), Err(CoreError::InvalidProvenance));

    model.op = model.op.budget().begin(CancellationToken::new());
    let mut unproved = close;
    unproved.atom.proof = None;
    model
        .builder
        .ir
        .packets
        .replace(3, unproved, &mut model.op)
        .unwrap();
    assert_eq!(model.validate(), Err(CoreError::InvalidProvenance));
}

#[test]
fn whole_short_stage_replays_without_scalar_heap_rows() {
    let mut m = Manual::new("&NotEqualTilde;");
    let atom = m.transform(
        span(0, 15),
        (0, 2),
        "≂\u{338}",
        PrimitiveKind::Copy,
        Some(StageKind::NamedEntity),
        ClaimKind::Entity,
    );
    m.seal(2);
    // Test-only fixture freezing is outside the measured validator/replay work.
    let frozen = m.builder.ir.snapshot(&mut m.op).unwrap();
    let before = m.op.stats().expect("statistics enabled").copied_bytes();
    let capacity_before = m.op.budget().usage().auxiliary_capacity_peak;
    assert_eq!(
        crate::policy::construction_scope(&mut m.op, |mut work| {
            super::replay::replay(&frozen, &m.source, atom, None, &mut work)?;
            work.finish()
        }),
        Ok(())
    );
    // By-value metadata views are stack values; no scalar heap is created.
    assert_eq!(
        m.op.stats().expect("statistics enabled").copied_bytes() - before,
        (size_of::<AtomRow>() + size_of::<AuthoritySpec>()) as u64
    );
    assert_eq!(
        m.op.budget().usage().auxiliary_capacity_peak,
        capacity_before
    );
    assert_eq!(
        super::validate::validate(&frozen, &m.source, &mut m.op),
        Ok(())
    );
}

#[test]
fn partial_single_stage_keeps_generic_minimal_closure_rejection() {
    let mut m = Manual::new("&amp;x");
    let atom = m.transform(
        span(0, 6),
        (0, 2),
        "&x",
        PrimitiveKind::Copy,
        Some(StageKind::NamedEntity),
        ClaimKind::Entity,
    );
    let mut stage = m.builder.ir.stages()[0];
    stage.input = local(0, 5);
    stage.output = m.builder.text("&", &mut m.op).unwrap();
    m.builder.ir.stages.replace(0, stage, &mut m.op).unwrap();
    let before = m.op.stats().expect("statistics enabled").copied_bytes();
    assert_eq!(
        replay_fixture(&m.builder.ir, &m.source, atom, None, &mut m.op),
        Err(CoreError::InvalidProvenance)
    );
    assert!(m.op.stats().expect("statistics enabled").copied_bytes() > before);
}

#[test]
fn ascii_trim_stays_generic_and_rejects_non_whitespace() {
    for raw in ["a", " a", "😀"] {
        let mut m = Manual::new(raw);
        let source = span(0, raw.len() as u64);
        let claim = m.claim(
            node(0),
            ClaimKind::Payload,
            NativePhase::Metadata,
            HostRole::Destination,
            &[source],
        );
        let atom = m.atom(
            node(0),
            AtomKind::Hidden,
            (0, 0),
            &[source],
            claim,
            AtomContent::Empty,
        );
        attach_empty_proof(&mut m, atom, source, claim, StageKind::AsciiTrim);
        assert_eq!(
            replay_fixture(
                &m.builder.ir,
                &m.source,
                atom,
                Some(PayloadRole::Destination),
                &mut m.op
            ),
            Err(CoreError::InvalidProvenance)
        );
    }
}

#[test]
fn short_stage_keeps_low_proof_quota_and_first_fault_in_generic_path() {
    let budget = SourceBudget::new_with_model_limits(
        crate::policy::SourceLimits::default(),
        crate::policy::ModelLimits {
            proof_records: 3,
            ..crate::policy::ModelLimits::default()
        },
    )
    .unwrap();
    let mut m = Manual::with_budget("&amp;", budget.clone());
    let atom = m.transform(
        span(0, 5),
        (0, 1),
        "&",
        PrimitiveKind::Copy,
        Some(StageKind::NamedEntity),
        ClaimKind::Entity,
    );
    let error = replay_fixture(&m.builder.ir, &m.source, atom, None, &mut m.op).unwrap_err();
    assert_eq!(
        error,
        CoreError::ResourceLimit {
            kind: LimitKind::ProofRecords,
            requested: 4,
            limit: 3
        }
    );
    assert_eq!(m.op.first_fault(), Some(error));
    drop(m);
    assert_eq!(budget.usage().source_capacity, 0);
    assert_eq!(budget.usage().auxiliary_capacity, 0);
}

#[path = "validation_output_tests.rs"]
mod validated_handoff;

fn replay_fixture(
    ir: &super::manual::ManualIr,
    source: &SourceSnapshot,
    atom: AtomId,
    payload: Option<PayloadRole>,
    op: &mut Operation,
) -> Result<(), CoreError> {
    let frozen = ir.snapshot(op)?;
    crate::policy::construction_scope(op, |mut work| {
        super::replay::replay(&frozen, source, atom, payload, &mut work)?;
        work.finish()
    })
}

#[test]
fn payload_pipe_authority_rejects_sibling_descendant_relabel_and_wrong_context() {
    // This is a local permission control over a manual graph, not a complete
    // accepted table model. The independent CRLF table oracle covers that path.
    let mut m = Manual::new("\\|");
    let budget = m.builder.ir.budget.clone();
    for (index, parent, kind) in [
        (1, 0, NodeKind::TableCell),
        (2, 1, NodeKind::Link),
        (3, 0, NodeKind::TableCell), // identical geometry, different Cell
        (4, 2, NodeKind::TableCell), // descendant cannot grant ancestor rights
    ] {
        let id = m
            .builder
            .node(
                NodeRow {
                    parent: Some(node(parent)),
                    first_child: None,
                    next_sibling: None,
                    flow: flow(0),
                    kind,
                    detail: None,
                    source_envelope: span(0, 2),
                    logical: logical(0, 1),
                },
                &mut m.op,
            )
            .unwrap();
        assert_eq!(id, node(index));
    }
    let short = m.builder.source_parts(&[span(0, 1)], &mut m.op).unwrap();
    let ir = m.builder.ir.snapshot(&mut m.op).unwrap();
    let good = AuthoritySpec {
        owner: node(1),
        host: HostRow {
            owner: node(1),
            role: HostRole::Block,
            utf8_len: 2,
            domains: rows(0, 1),
        },
        phase: NativePhase::Block,
        kind: ClaimKind::Table,
        consumed: local(0, 2),
        sources: rows(0, 1),
    };
    let mut op = budget.begin(CancellationToken::new());
    crate::policy::construction_scope(&mut op, |mut work| {
        let check = |claim, payload, stage, work: &mut crate::policy::OwnedWork<'_, '_>| {
            validate::payload_stage_owner(&ir, node(2), payload, stage, claim, work)
        };
        let payload = PayloadOwner::Node(node(2));
        assert!(check(
            good,
            payload,
            StageKind::TableEscapedPipe,
            &mut work
        )?);
        for wrong in [node(0), node(2), node(3), node(4)] {
            let mut claim = good;
            claim.owner = wrong;
            claim.host.owner = wrong;
            assert!(!check(
                claim,
                payload,
                StageKind::TableEscapedPipe,
                &mut work
            )?);
        }
        let mut claim = good;
        claim.host.owner = node(3);
        assert!(!check(
            claim,
            payload,
            StageKind::TableEscapedPipe,
            &mut work
        )?);
        let mut claim = good;
        claim.phase = NativePhase::Metadata;
        assert!(!check(
            claim,
            payload,
            StageKind::TableEscapedPipe,
            &mut work
        )?);
        let mut claim = good;
        claim.host.role = HostRole::Destination;
        assert!(!check(
            claim,
            payload,
            StageKind::TableEscapedPipe,
            &mut work
        )?);
        let mut claim = good;
        claim.kind = ClaimKind::Payload;
        assert!(!check(
            claim,
            payload,
            StageKind::TableEscapedPipe,
            &mut work
        )?);
        let mut claim = good;
        claim.consumed = local(0, 1);
        assert!(!check(
            claim,
            payload,
            StageKind::TableEscapedPipe,
            &mut work
        )?);
        let mut claim = good;
        claim.sources = short;
        assert!(!check(
            claim,
            payload,
            StageKind::TableEscapedPipe,
            &mut work
        )?);
        assert!(!check(
            good,
            PayloadOwner::Definition(DefinitionId::from_index(0)?),
            StageKind::TableEscapedPipe,
            &mut work
        )?);
        assert!(!check(good, payload, StageKind::NumericEntity, &mut work)?);
        let mut ordinary = good;
        ordinary.owner = node(2);
        ordinary.host.owner = node(2);
        assert!(check(
            ordinary,
            payload,
            StageKind::NumericEntity,
            &mut work
        )?);
        work.finish()
    })
    .unwrap();
    drop((ir, m, op));
    assert_eq!(budget.usage().source_capacity, 0);
    assert_eq!(budget.usage().source_owners, 0);
    assert_eq!(budget.usage().auxiliary_capacity, 0);
}

#[derive(Clone, Copy, Debug)]
enum PipeAltCase {
    OneLevel,
    Nested,
    SiblingCell,
    WrongScopeObject,
    WrongObjectKind,
    WrongAltBackreference,
    WrongFlowRoot,
    WrongImageKind,
    SameFlowImage,
    CrossFlowParent,
    InvalidObject,
    InvalidImage,
    ScopeCycle,
}

fn pipe_alt_permission_fixture() -> Manual {
    // Local permission geometry only, not a complete accepted image/table IR.
    // Atom/payload references in Object rows are unused numeric placeholders;
    // this helper checks the actual node/flow/object scope chain, not replay.
    let mut m = Manual::new("\\|");
    for (index, parent, scope, kind) in [
        (1, Some(0), 0, NodeKind::TableCell),
        (2, Some(1), 0, NodeKind::Image),
        (3, None, 1, NodeKind::AltRoot),
        (4, Some(3), 1, NodeKind::Link),
        (5, Some(0), 0, NodeKind::TableCell), // same source geometry, sibling Cell
        (6, Some(3), 1, NodeKind::Image),
        (7, None, 2, NodeKind::AltRoot),
        (8, Some(7), 2, NodeKind::Link),
        (9, Some(5), 0, NodeKind::Image),
        (10, None, 3, NodeKind::AltRoot),
        (11, Some(10), 3, NodeKind::Link),
        (12, Some(7), 2, NodeKind::Image), // used only by the malformed cycle
    ] {
        let id = m
            .builder
            .node(
                NodeRow {
                    parent: parent.map(node),
                    first_child: None,
                    next_sibling: None,
                    flow: flow(scope),
                    kind,
                    detail: None,
                    source_envelope: span(0, 2),
                    logical: logical(0, 1),
                },
                &mut m.op,
            )
            .unwrap();
        assert_eq!(id, node(index));
    }
    for (index, image, alt, root) in [(0, 2, 1, 3), (1, 6, 2, 7), (2, 9, 3, 10)] {
        let object = ObjectId::from_index(index).unwrap();
        assert_eq!(
            m.builder
                .object(
                    ObjectRow::Image {
                        node: node(image),
                        atom: AtomId::from_index(0).unwrap(),
                        alt: flow(alt),
                        destination: PayloadId::from_index(0).unwrap(),
                        title: None,
                        reference: None,
                        wiki_size: None,
                    },
                    &mut m.op
                )
                .unwrap(),
            object
        );
        assert_eq!(
            m.builder
                .flow(
                    FlowRow {
                        scope: FlowScope::ImageAlt { object },
                        root: node(root),
                        domains: rows(0, 1),
                        atoms: rows(0, 0),
                        logical_utf16: 1,
                    },
                    &mut m.op
                )
                .unwrap(),
            flow(alt)
        );
    }
    m
}

#[test]
fn payload_pipe_alt_scope_bridge_accepts_only_its_unique_image_chain() {
    for case in [
        PipeAltCase::OneLevel,
        PipeAltCase::Nested,
        PipeAltCase::SiblingCell,
        PipeAltCase::WrongScopeObject,
        PipeAltCase::WrongObjectKind,
        PipeAltCase::WrongAltBackreference,
        PipeAltCase::WrongFlowRoot,
        PipeAltCase::WrongImageKind,
        PipeAltCase::SameFlowImage,
        PipeAltCase::CrossFlowParent,
        PipeAltCase::InvalidObject,
        PipeAltCase::InvalidImage,
        PipeAltCase::ScopeCycle,
    ] {
        let mut m = pipe_alt_permission_fixture();
        let budget = m.builder.ir.budget.clone();
        let owner = node(if matches!(case, PipeAltCase::Nested) {
            8
        } else {
            4
        });
        let mut claim = AuthoritySpec {
            owner: node(1),
            host: HostRow {
                owner: node(1),
                role: HostRole::Block,
                utf8_len: 2,
                domains: rows(0, 1),
            },
            phase: NativePhase::Block,
            kind: ClaimKind::Table,
            consumed: local(0, 2),
            sources: rows(0, 1),
        };
        match case {
            PipeAltCase::OneLevel | PipeAltCase::Nested => {}
            PipeAltCase::SiblingCell => {
                claim.owner = node(5);
                claim.host.owner = node(5);
            }
            PipeAltCase::WrongScopeObject | PipeAltCase::InvalidObject => {
                let mut row = m.builder.ir.flows()[1];
                row.scope = FlowScope::ImageAlt {
                    object: ObjectId::from_index(if matches!(case, PipeAltCase::InvalidObject) {
                        99
                    } else {
                        2
                    })
                    .unwrap(),
                };
                m.builder.ir.flows.replace(1, row, &mut m.op).unwrap();
            }
            PipeAltCase::WrongObjectKind => {
                m.builder
                    .ir
                    .objects
                    .replace(
                        0,
                        ObjectRow::Task {
                            node: node(2),
                            atom: AtomId::from_index(0).unwrap(),
                            item: node(0),
                            checked: false,
                            marker: rows(0, 1),
                            spacer: rows(0, 1),
                            action: None,
                        },
                        &mut m.op,
                    )
                    .unwrap();
            }
            PipeAltCase::WrongAltBackreference
            | PipeAltCase::SameFlowImage
            | PipeAltCase::InvalidImage
            | PipeAltCase::ScopeCycle => {
                let ObjectRow::Image {
                    node: original_image,
                    atom,
                    alt,
                    destination,
                    title,
                    reference,
                    wiki_size,
                } = m.builder.ir.objects()[0]
                else {
                    unreachable!()
                };
                let image = match case {
                    PipeAltCase::SameFlowImage => node(6),
                    PipeAltCase::InvalidImage => node(99),
                    PipeAltCase::ScopeCycle => node(12),
                    _ => original_image,
                };
                m.builder
                    .ir
                    .objects
                    .replace(
                        0,
                        ObjectRow::Image {
                            node: image,
                            atom,
                            alt: if matches!(case, PipeAltCase::WrongAltBackreference) {
                                flow(2)
                            } else {
                                alt
                            },
                            destination,
                            title,
                            reference,
                            wiki_size,
                        },
                        &mut m.op,
                    )
                    .unwrap();
            }
            PipeAltCase::WrongFlowRoot => {
                let mut row = m.builder.ir.flows()[1];
                row.root = node(4);
                m.builder.ir.flows.replace(1, row, &mut m.op).unwrap();
            }
            PipeAltCase::WrongImageKind => {
                let mut row = m.builder.ir.nodes()[2];
                row.kind = NodeKind::Link;
                m.builder.ir.nodes.replace(2, row, &mut m.op).unwrap();
            }
            PipeAltCase::CrossFlowParent => {
                let mut row = m.builder.ir.nodes()[4];
                row.parent = Some(node(2));
                m.builder.ir.nodes.replace(4, row, &mut m.op).unwrap();
            }
        }
        let ir = m.builder.ir.snapshot(&mut m.op).unwrap();
        let expected = match case {
            PipeAltCase::OneLevel | PipeAltCase::Nested => Ok(true),
            PipeAltCase::InvalidObject | PipeAltCase::InvalidImage | PipeAltCase::ScopeCycle => {
                Err(CoreError::InvalidProvenance)
            }
            _ => Ok(false),
        };
        let mut op = budget.begin(CancellationToken::new());
        let actual = crate::policy::construction_scope(&mut op, |mut work| {
            match validate::payload_stage_owner(
                &ir,
                owner,
                PayloadOwner::Node(owner),
                StageKind::TableEscapedPipe,
                claim,
                &mut work,
            ) {
                Ok(permitted) => work.finish().map(|()| permitted),
                Err(error) => {
                    let first = work.fail(error);
                    assert_eq!(work.finish(), Err(first));
                    Err(first)
                }
            }
        });
        assert_eq!(actual, expected, "{case:?}");
        drop((ir, m, op));
        assert_eq!(budget.usage().source_capacity, 0);
        assert_eq!(budget.usage().source_owners, 0);
        assert_eq!(budget.usage().auxiliary_capacity, 0);
    }
}
