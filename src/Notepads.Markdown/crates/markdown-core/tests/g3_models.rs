// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE file in the project root for license information.
// ---------------------------------------------------------------------------------------------

//! Pre-execution expectations: ../g3-candidate/MODEL-ASSERTIONS.md.
//! These tests use the candidate's ordinary public parse entry and real clock.

#[path = "support/g3_grammar_delta_cases.rs"]
mod g3_grammar_delta_cases;

#[path = "support/g3_title_delta_cases.rs"]
mod g3_title_delta_cases;

use markdown_core::config::{ParseConfig, Profile};
use markdown_core::identity::{LogicalSpan, SourceBoundary, SourceKey, SourceSpan};
use markdown_core::ir::{
    Alignment, AtomContent, AtomKind, AttributeKind, ClaimKind, DetailRow, FlowScope, ListMarker,
    NodeKind, ObjectRow, Rows, SyntheticKind, TextTarget,
};
use markdown_core::mapping::{
    Affinity, Correspondence, Direction, PointInput, PointQuery, RangeInput, RangeQuery,
    RangeResult,
};
use markdown_core::model::AcceptedModel;
use markdown_core::policy::{CancellationToken, SourceBudget, WorkStatsCollection};
use markdown_core::source::SourceDocument;

fn with_model(text: &str, check: impl FnOnce(&AcceptedModel)) {
    with_profile(text, Profile::G3, check);
}

fn with_profile(text: &str, profile: Profile, check: impl FnOnce(&AcceptedModel)) {
    let budget = SourceBudget::task_fixture();
    assert_eq!(
        budget.work_stats_collection(),
        WorkStatsCollection::Disabled
    );
    let mut op = budget.begin(CancellationToken::new());
    let source = SourceDocument::new(
        SourceKey::new([0x63; 16], 1).unwrap(),
        text.as_bytes(),
        &mut op,
    )
    .unwrap();
    let model = AcceptedModel::parse(source.snapshot(), ParseConfig::new(profile), 1, &mut op)
        .unwrap_or_else(|error| panic!("{text:?}: {error:?}"));
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

fn spans(model: &AcceptedModel, rows: Rows<SourceSpan>) -> Vec<(u64, u64)> {
    (rows.start()..rows.start() + rows.len())
        .map(|i| {
            let v = model.ir().sources()[i];
            (v.start(), v.end())
        })
        .collect()
}

fn main_text(model: &AcceptedModel) -> String {
    let ir = model.ir();
    let flow = ir.flows()[model.main_flow_id().index()];
    let mut op = model.budget().begin(CancellationToken::new());
    let mut text = String::new();
    for at in flow.atoms.start()..flow.atoms.start() + flow.atoms.len() {
        let atom = ir.atom_at(at, &mut op).unwrap();
        match (atom.kind, atom.content) {
            (AtomKind::Hidden, _) => {}
            (AtomKind::Copy, _) => {
                for (a, b) in spans(model, atom.sources) {
                    text.push_str(&model.source().text()[a as usize..b as usize]);
                }
            }
            (AtomKind::Object, _) => text.push('\u{fffc}'),
            (_, AtomContent::Pool(value)) => text.push_str(ir.text(value).unwrap()),
            _ => panic!("unexpected atom content"),
        }
    }
    assert_eq!(op.first_fault(), None);
    text
}

#[test]
fn github_bare_autolinks_keep_authored_labels_utf16_ranges_and_destinations() {
    let source = "> 中😀 www.example.com/docs, https://example.org/a(b)).\r\n> alice&#64;example.net.\r\n> www.continuation.example\r\n\r\n- List item\r\n  name@example.org.\r\n\r\nFTP ftp://files.example.org/pub!\r\n\r\n`www.literal.example` [www.label.example](https://explicit.example) ![www.alt.example](pic.png)";
    with_model(source, |model| {
        let ir = model.ir();
        let text: Vec<_> = main_text(model).encode_utf16().collect();
        let links: Vec<_> = ir
            .nodes()
            .iter()
            .filter(|node| node.kind == NodeKind::Link)
            .collect();
        assert_eq!(links.len(), 7);
        for (node, (label, destination, prefix)) in links.into_iter().zip([
            (
                "www.example.com/docs",
                "http://www.example.com/docs",
                Some("http://"),
            ),
            ("https://example.org/a(b)", "https://example.org/a(b)", None),
            (
                "alice@example.net",
                "mailto:alice@example.net",
                Some("mailto:"),
            ),
            (
                "www.continuation.example",
                "http://www.continuation.example",
                Some("http://"),
            ),
            (
                "name@example.org",
                "mailto:name@example.org",
                Some("mailto:"),
            ),
            (
                "ftp://files.example.org/pub",
                "ftp://files.example.org/pub",
                None,
            ),
            ("www.label.example", "https://explicit.example", None),
        ]) {
            assert_eq!(
                String::from_utf16(
                    &text[node.logical.start() as usize..node.logical.end() as usize]
                )
                .unwrap(),
                label
            );
            let DetailRow::Link {
                destination: payload,
                ..
            } = ir.details()[node.detail.unwrap().index()]
            else {
                panic!("link node has no destination");
            };
            let payload = ir.payloads()[payload.index()];
            assert_eq!(ir.text(payload.value).unwrap(), destination);
            if let Some(prefix) = prefix {
                let mut op = model.budget().begin(CancellationToken::new());
                let atom = ir.atom_at(payload.atoms.start(), &mut op).unwrap();
                assert_eq!(atom.kind, AtomKind::Synthetic);
                assert_eq!(atom.sources.len(), 1);
                let anchor = ir.sources()[atom.sources.start()];
                assert_eq!(anchor.start(), node.source_envelope.start());
                assert_eq!(anchor.end(), anchor.start());
                let AtomContent::Pool(value) = atom.content else {
                    panic!("missing generated scheme");
                };
                assert_eq!(ir.text(value).unwrap(), prefix);
            }
        }
        assert!(
            main_text(model).starts_with("中😀 www.example.com/docs, https://example.org/a(b)).")
        );
    });
    with_profile(source, Profile::C0, |model| {
        assert_eq!(
            model
                .ir()
                .nodes()
                .iter()
                .filter(|node| node.kind == NodeKind::Link)
                .count(),
            1
        );
    });
}

#[test]
fn g3_task_raw_admission_matrix_keeps_literals_empty_labels_and_readonly_actions() {
    // This matrix instantiates the pre-execution policy in MODEL-ASSERTIONS.md.
    for (source, text, task) in [
        ("- [x]", "[x]", None),
        ("- [x] z", "\u{fffc} z", Some((true, true))),
        ("- [x] 中", "\u{fffc} 中", Some((true, true))),
        ("- [x] 中\n", "\u{fffc} 中", Some((true, true))),
        ("- [x] 😀", "\u{fffc} 😀", Some((true, true))),
        ("- [x] ", "\u{fffc}", Some((true, true))),
        ("- [x]\n", "\u{fffc}", Some((true, true))),
        ("- [x]\r\n", "\u{fffc}", Some((true, true))),
        ("- [ ] z\n", "\u{fffc} z", Some((false, true))),
        ("- [X] z\n", "\u{fffc} z", Some((true, true))),
        ("- [\t] z\n", "\u{fffc} z", Some((false, false))),
        ("- [\r\n] z\n", "\u{fffc} z", Some((false, false))),
        ("- [&#x78;] z\n", "[x] z", None),
        ("- &lbrack;x] z\n", "[x] z", None),
        ("- [x]&#32;z\n", "[x] z", None),
        ("- \\[x] z\n", "[x] z", None),
        ("- [x]\u{a0}z\n", "[x]\u{a0}z", None),
    ] {
        with_model(source, |model| {
            assert_eq!(main_text(model), text, "{source:?}");
            assert_eq!(model.ir().flows().len(), 1);
            match task {
                None => assert!(model.ir().objects().is_empty(), "{source:?}"),
                Some((expected_checked, action_allowed)) => {
                    assert_eq!(model.ir().objects().len(), 1);
                    let ObjectRow::Task {
                        checked, action, ..
                    } = model.ir().objects()[0]
                    else {
                        panic!("expected one task: {source:?}");
                    };
                    assert_eq!(checked, expected_checked);
                    assert_eq!(action.is_some(), action_allowed);
                    let label = text != "\u{fffc}";
                    assert_eq!(
                        model
                            .ir()
                            .nodes()
                            .iter()
                            .filter(|n| n.kind == NodeKind::Paragraph)
                            .count(),
                        usize::from(label)
                    );
                }
            }
        });
    }
}

#[test]
fn g3_task_eof_label_envelope_starts_after_the_real_spacer() {
    for (source, end) in [("- [x] z", 7), ("- [x] 中", 9), ("- [x] 😀", 10)] {
        with_model(source, |model| {
            let nodes = model.ir().nodes();
            let checkbox = nodes
                .iter()
                .find(|n| n.kind == NodeKind::TaskCheckbox)
                .unwrap();
            let label = nodes
                .iter()
                .find(|n| n.kind == NodeKind::Paragraph)
                .unwrap();
            assert_eq!(checkbox.source_envelope, SourceSpan::new(2, 5).unwrap());
            assert_eq!(label.source_envelope, SourceSpan::new(6, end).unwrap());
            assert_eq!(checkbox.parent, label.parent);
        });
    }
}

#[test]
fn g3_strike_accepts_matching_single_and_double_tildes() {
    for (source, expected, strikes) in [
        ("~~Hi~~ Hello, ~there~ world!", "Hi Hello, there world!", 2),
        ("a ~x~ b", "a x b", 1),
        ("a ~**中😀**~ b", "a 中😀 b", 1),
        ("a ~x~~ b", "a ~x~~ b", 0),
        ("a ~~x~ b", "a ~~x~ b", 0),
        ("~a~~ b ~c~", "~a~~ b c", 1),
        ("~a~~ b **bold**", "~a~~ b bold", 0),
        ("a ~x ~~y~~ z~ b", "a x y z b", 2),
        ("a ~~x ~y~ z~~ b", "a x y z b", 2),
        ("a ~~~x~~~ b", "a ~~~x~~~ b", 0),
        ("a ~~~~x~~~~ b", "a ~~~~x~~~~ b", 0),
        ("a ~~x~~~ b", "a ~~x~~~ b", 0),
        ("a ~~~x~~ b", "a ~~~x~~ b", 0),
        ("a ~~x~~ b", "a x b", 1),
        ("a ~~~x~~~ ~~y~~ b", "a ~~~x~~~ y b", 1),
        ("a \\~x~ b", "a ~x~ b", 0),
        ("a `~x~` b", "a ~x~ b", 0),
    ] {
        with_model(source, |model| {
            assert_eq!(main_text(model), expected, "{source:?}");
            assert_eq!(
                model
                    .ir()
                    .nodes()
                    .iter()
                    .filter(|n| n.kind == NodeKind::Strike)
                    .count(),
                strikes
            );
            assert!(model.ir().objects().is_empty());
            assert!(model.ir().payloads().is_empty());
        });
    }
}

#[derive(Clone, Copy)]
struct Node {
    kind: NodeKind,
    parent: Option<usize>,
    logical: (u64, u64),
}

fn structural(
    model: &AcceptedModel,
    nodes: &[Node],
    text: &str,
    details: usize,
    objects: usize,
    attributes: usize,
) {
    structural_with_proofs(model, nodes, text, details, objects, attributes, (0, 0, 0));
}

fn structural_with_proofs(
    model: &AcceptedModel,
    nodes: &[Node],
    text: &str,
    details: usize,
    objects: usize,
    attributes: usize,
    proofs: (usize, usize, usize),
) {
    let ir = model.ir();
    assert_eq!(ir.nodes().len(), nodes.len());
    for (i, (actual, expected)) in ir.nodes().iter().zip(nodes).enumerate() {
        assert_eq!(actual.kind, expected.kind, "node {i}");
        assert_eq!(
            actual.parent.map(|id| id.index()),
            expected.parent,
            "node {i}"
        );
        assert_eq!(
            (actual.logical.start(), actual.logical.end()),
            expected.logical,
            "node {i}"
        );
        assert_eq!(actual.flow, model.main_flow_id());
        let first = nodes.iter().position(|v| v.parent == Some(i));
        let next = nodes
            .iter()
            .enumerate()
            .skip(i + 1)
            .find(|(_, v)| v.parent == expected.parent)
            .map(|(index, _)| index);
        assert_eq!(actual.first_child.map(|id| id.index()), first);
        assert_eq!(actual.next_sibling.map(|id| id.index()), next);
    }
    assert_eq!(ir.flows().len(), 1);
    let main = ir.flows()[model.main_flow_id().index()];
    assert_eq!(main.scope, FlowScope::Main);
    assert_eq!(main.root.index(), 0);
    assert_eq!(main.logical_utf16, text.encode_utf16().count() as u64);
    assert_eq!(
        spans(model, main.domains),
        vec![(0, model.source().text().len() as u64)]
    );
    assert_eq!(ir.details().len(), details);
    assert_eq!(ir.objects().len(), objects);
    assert_eq!(ir.attributes().len(), attributes);
    assert!(ir.payloads().is_empty());
    assert!(ir.definitions().is_empty());
    assert!(ir.references().is_empty());
    assert_eq!(ir.proofs().len(), proofs.0);
    assert_eq!(ir.primitives().len(), proofs.1);
    assert_eq!(ir.stages().len(), proofs.2);
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct Separator {
    kind: SyntheticKind,
    owner: usize,
    source: u64,
    target: (u64, u64),
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct Transform {
    source: (usize, usize),
    target: (u64, u64),
    owner: usize,
    output: &'static str,
}

/// Compare every source byte's role, every Copy correspondence, every projected
/// character and every synthetic anchor. Adjacent equivalent Copy/Hidden rows
/// may be split without making their source coverage optional.
fn projection(
    model: &AcceptedModel,
    expected: &str,
    copies: &[(usize, usize, usize, usize)],
    object_sources: &[(usize, usize)],
    separators: &[Separator],
) {
    projection_with_transforms(model, expected, copies, object_sources, separators, &[]);
}

fn projection_with_transforms(
    model: &AcceptedModel,
    expected: &str,
    copies: &[(usize, usize, usize, usize)],
    object_sources: &[(usize, usize)],
    separators: &[Separator],
    transforms: &[Transform],
) {
    let ir = model.ir();
    let raw = model.source().text();
    assert!(raw.is_ascii());
    let mut expected_roles = vec![AtomKind::Hidden; raw.len()];
    let mut expected_copy = Vec::new();
    for &(start, end, target, owner) in copies {
        for (offset, role) in expected_roles[start..end].iter_mut().enumerate() {
            assert_eq!(*role, AtomKind::Hidden);
            *role = AtomKind::Copy;
            expected_copy.push((start + offset, target + offset, owner));
        }
    }
    for &(start, end) in object_sources {
        expected_roles[start..end].fill(AtomKind::Object);
    }
    for t in transforms {
        expected_roles[t.source.0..t.source.1].fill(AtomKind::Transform);
    }
    let mut roles = vec![None; raw.len()];
    let mut actual_copy = Vec::new();
    let mut actual_separators = Vec::new();
    let mut transform_at = 0;
    let mut rendered = String::new();
    let mut op = model.budget().begin(CancellationToken::new());
    let main = ir.flows()[model.main_flow_id().index()];
    for index in main.atoms.start()..main.atoms.start() + main.atoms.len() {
        let atom = ir.atom_at(index, &mut op).unwrap();
        let source = spans(model, atom.sources);
        for &(start, end) in &source {
            for (offset, role) in roles[start as usize..end as usize].iter_mut().enumerate() {
                let byte = start as usize + offset;
                assert!(
                    role.replace(atom.kind).is_none(),
                    "overlapping source {byte}"
                );
            }
        }
        match atom.kind {
            AtomKind::Hidden => {
                assert_eq!(atom.target.start, atom.target.end);
                assert_eq!(atom.content, AtomContent::Empty);
            }
            AtomKind::Copy => {
                let mut target = atom.target.start as usize;
                for &(start, end) in &source {
                    rendered.push_str(&raw[start as usize..end as usize]);
                    for byte in start as usize..end as usize {
                        actual_copy.push((byte, target, atom.owner.index()));
                        target += 1;
                    }
                }
                assert_eq!(target as u64, atom.target.end);
            }
            AtomKind::Synthetic => {
                let [(anchor, end)] = source.as_slice() else {
                    panic!("one synthetic anchor");
                };
                assert_eq!(anchor, end);
                let authority = ir.authority(atom.claim, &mut op).unwrap();
                let ClaimKind::ProjectionSeparator(kind) = authority.kind else {
                    panic!();
                };
                let AtomContent::Pool(text) = atom.content else {
                    panic!();
                };
                rendered.push_str(ir.text(text).unwrap());
                actual_separators.push(Separator {
                    kind,
                    owner: atom.owner.index(),
                    source: *anchor,
                    target: (atom.target.start, atom.target.end),
                });
            }
            AtomKind::Object => {
                assert_eq!(atom.target.end - atom.target.start, 1);
                rendered.push('\u{fffc}');
            }
            AtomKind::Transform => {
                let value = transforms.get(transform_at).expect("no extra Transform");
                transform_at += 1;
                assert_eq!(source, vec![(value.source.0 as u64, value.source.1 as u64)]);
                assert_eq!((atom.target.start, atom.target.end), value.target);
                assert_eq!(atom.owner.index(), value.owner);
                let AtomContent::Pool(text) = atom.content else {
                    panic!();
                };
                assert_eq!(ir.text(text).unwrap(), value.output);
                assert!(atom.proof.is_some());
                rendered.push_str(value.output);
            }
        }
    }
    assert_eq!(rendered, expected);
    assert_eq!(actual_copy, expected_copy);
    assert_eq!(actual_separators, separators);
    assert_eq!(transform_at, transforms.len());
    assert_eq!(
        roles,
        expected_roles.into_iter().map(Some).collect::<Vec<_>>()
    );
    assert_eq!(op.first_fault(), None);
}

fn node(kind: NodeKind, parent: Option<usize>, start: u64, end: u64) -> Node {
    Node {
        kind,
        parent,
        logical: (start, end),
    }
}

fn range_content(
    model: &AcceptedModel,
    direction: Direction,
    input: (u64, u64),
    kind: AtomKind,
    source: (u64, u64),
    target: (u64, u64),
    expanded: bool,
) {
    let mut op = model.budget().begin(CancellationToken::new());
    let flow = model.main_flow_id();
    let input = match direction {
        Direction::SourceToTarget => RangeInput::Source {
            target: TextTarget::Flow(flow),
            span: SourceSpan::new(input.0, input.1).unwrap(),
        },
        Direction::TargetToSource => RangeInput::Flow {
            flow,
            span: LogicalSpan::new(input.0, input.1).unwrap(),
        },
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
        panic!();
    };
    assert_eq!(rows.len(), 1);
    let selection = response.selections()[rows.start()];
    assert_eq!(selection.model, model.identity());
    assert_eq!(selection.target, TextTarget::Flow(flow));
    assert_eq!(selection.direction, direction);
    assert_eq!(selection.expanded, expanded);
    assert_eq!(selection.contributions.len(), 1);
    let contribution = response.contributions()[selection.contributions.start()];
    assert_eq!(contribution.kind, kind);
    assert_eq!(
        (contribution.source.start(), contribution.source.end()),
        source
    );
    assert_eq!((contribution.target.start, contribution.target.end), target);
    assert_eq!(contribution.expanded, expanded);
    assert_eq!(op.first_fault(), None);
}

fn inside_point(
    model: &AcceptedModel,
    byte: u64,
    kind: AtomKind,
    relation: Correspondence,
    source: (u64, u64),
    target: (u64, u64),
) {
    for (affinity, chosen) in [(Affinity::Before, target.0), (Affinity::After, target.1)] {
        let mut op = model.budget().begin(CancellationToken::new());
        let response = model
            .map_point(
                &PointQuery {
                    model: model.identity(),
                    input: PointInput::Source {
                        target: TextTarget::Flow(model.main_flow_id()),
                        byte: SourceBoundary::new(byte),
                    },
                    affinity,
                },
                &mut op,
            )
            .unwrap();
        let [point] = response.results() else {
            panic!();
        };
        assert_eq!(point.model, model.identity());
        assert_eq!(point.target, TextTarget::Flow(model.main_flow_id()));
        assert_eq!(point.direction, Direction::SourceToTarget);
        assert_eq!(point.chosen.value(), chosen);
        assert_eq!(point.correspondence, relation);
        assert_eq!(point.contributors.len(), 1);
        let contributor = response.contributors()[point.contributors.start()];
        assert_eq!(contributor.kind, kind);
        assert_eq!(contributor.relation, relation);
        assert_eq!(
            (contributor.source.start(), contributor.source.end()),
            source
        );
        assert_eq!((contributor.target.start, contributor.target.end), target);
        assert_eq!(
            point.related.envelope.map(|v| (v.start(), v.end())),
            Some(target)
        );
        assert_eq!(op.first_fault(), None);
    }
}

#[test]
fn g3_quoted_table_has_persistent_alignment_witnesses_and_real_cell_anchor() {
    use NodeKind::*;
    with_model("> a | b\n> :-- | --:\n", |model| {
        structural(
            model,
            &[
                node(Document, None, 0, 3),
                node(BlockQuote, Some(0), 0, 3),
                node(Table, Some(1), 0, 3),
                node(TableRow, Some(2), 0, 3),
                node(TableCell, Some(3), 0, 1),
                node(Text, Some(4), 0, 1),
                node(TableCell, Some(3), 2, 3),
                node(Text, Some(6), 2, 3),
            ],
            "a\tb",
            4,
            0,
            5,
        );
        projection(
            model,
            "a\tb",
            &[(2, 3, 0, 5), (6, 7, 2, 7)],
            &[],
            &[Separator {
                kind: SyntheticKind::CellTab,
                owner: 6,
                source: 5,
                target: (1, 2),
            }],
        );
        let ir = model.ir();
        let detail = |i: usize| ir.details()[ir.nodes()[i].detail.unwrap().index()];
        let DetailRow::Table {
            columns,
            alignments,
            ..
        } = detail(2)
        else {
            panic!();
        };
        assert_eq!(columns, 2);
        assert_eq!(
            (alignments.start()..alignments.start() + alignments.len())
                .map(|i| ir.alignments()[i])
                .collect::<Vec<_>>(),
            vec![Alignment::Left, Alignment::Right]
        );
        assert!(matches!(
            detail(3),
            DetailRow::Row {
                index: 0,
                header: true,
                ..
            }
        ));
        assert!(matches!(
            detail(4),
            DetailRow::Cell {
                column: 0,
                missing: false,
                ..
            }
        ));
        assert!(matches!(
            detail(6),
            DetailRow::Cell {
                column: 1,
                missing: false,
                ..
            }
        ));
        let attributes: Vec<_> = ir
            .attributes()
            .iter()
            .map(|a| (a.owner.index(), a.kind, spans(model, a.sources)))
            .collect();
        assert_eq!(
            attributes,
            vec![
                (2, AttributeKind::TableColumns, vec![(10, 20)]),
                (2, AttributeKind::TableAlignment, vec![(10, 13), (16, 19)]),
                (3, AttributeKind::RowHeader, vec![(2, 8)]),
                (4, AttributeKind::CellMissing, vec![(2, 4)]),
                (6, AttributeKind::CellMissing, vec![(5, 7)]),
            ]
        );
        range_content(
            model,
            Direction::TargetToSource,
            (1, 2),
            AtomKind::Synthetic,
            (5, 5),
            (1, 2),
            false,
        );
    });
}

fn task_details(model: &AcceptedModel, tight: bool, spacer: (u64, u64)) {
    let ir = model.ir();
    let list = ir.details()[ir.nodes()[1].detail.unwrap().index()];
    assert!(
        matches!(list, DetailRow::List { marker: ListMarker::Bullet(b'-'), start: 0, tight: t, .. } if t == tight)
    );
    let item_detail = ir.details()[ir.nodes()[2].detail.unwrap().index()];
    assert!(matches!(
        item_detail,
        DetailRow::Item {
            occurrence: 0,
            marker: ListMarker::Bullet(b'-'),
            value: 0,
            ..
        }
    ));
    let ObjectRow::Task {
        node,
        atom,
        item,
        checked,
        marker,
        spacer: actual_spacer,
        action,
    } = ir.objects()[0]
    else {
        panic!();
    };
    assert_eq!(node.index(), 3);
    assert_eq!(item.index(), 2);
    assert!(checked);
    assert_eq!(spans(model, marker), vec![(2, 5)]);
    assert_eq!(spans(model, actual_spacer), vec![spacer]);
    let action = action.unwrap();
    assert_eq!(action.source, SourceSpan::new(2, 5).unwrap());
    assert_eq!(action.expected, *b"[x]");
    let mut op = model.budget().begin(CancellationToken::new());
    let actual = ir.atom_at(atom.index(), &mut op).unwrap();
    assert_eq!(actual.kind, AtomKind::Object);
    assert_eq!(actual.owner, node);
    assert_eq!((actual.target.start, actual.target.end), (0, 1));
    for index in 0..ir.atoms_len() {
        let atom = ir.atom_at(index, &mut op).unwrap();
        if atom.kind == AtomKind::Hidden
            && spans(model, atom.sources)
                .iter()
                .any(|&(a, b)| a < spacer.1 && b > spacer.0)
        {
            assert_eq!(atom.owner, item);
        }
    }
}

#[test]
fn g3_task_nonempty_label_keeps_item_and_three_byte_action() {
    use NodeKind::*;
    with_model("- [x] ok\n", |model| {
        structural(
            model,
            &[
                node(Document, None, 0, 4),
                node(List, Some(0), 0, 4),
                node(ListItem, Some(1), 0, 4),
                node(TaskCheckbox, Some(2), 0, 1),
                node(Paragraph, Some(2), 2, 4),
                node(Text, Some(4), 2, 4),
            ],
            "\u{fffc} ok",
            2,
            1,
            4,
        );
        task_details(model, true, (5, 6));
        projection(
            model,
            "\u{fffc} ok",
            &[(6, 8, 2, 5)],
            &[(2, 5)],
            &[Separator {
                kind: SyntheticKind::TaskLabelSpace,
                owner: 3,
                source: 6,
                target: (1, 2),
            }],
        );
        range_content(
            model,
            Direction::SourceToTarget,
            (3, 4),
            AtomKind::Object,
            (3, 4),
            (0, 1),
            true,
        );
        range_content(
            model,
            Direction::TargetToSource,
            (0, 1),
            AtomKind::Object,
            (2, 5),
            (0, 1),
            false,
        );
        inside_point(
            model,
            3,
            AtomKind::Object,
            Correspondence::Object,
            (2, 5),
            (0, 1),
        );
        range_content(
            model,
            Direction::SourceToTarget,
            (5, 6),
            AtomKind::Hidden,
            (5, 6),
            (1, 1),
            false,
        );
    });
}

#[test]
fn g3_empty_task_has_no_fake_paragraph_or_label_space_before_later_block() {
    use NodeKind::*;
    with_model("- [x] \n\n  later\n", |model| {
        structural(
            model,
            &[
                node(Document, None, 0, 7),
                node(List, Some(0), 0, 7),
                node(ListItem, Some(1), 0, 7),
                node(TaskCheckbox, Some(2), 0, 1),
                node(Paragraph, Some(2), 2, 7),
                node(Text, Some(4), 2, 7),
            ],
            "\u{fffc}\nlater",
            2,
            1,
            4,
        );
        task_details(model, false, (5, 7));
        projection(
            model,
            "\u{fffc}\nlater",
            &[(10, 15, 2, 5)],
            &[(2, 5)],
            &[Separator {
                kind: SyntheticKind::BlockLineFeed,
                owner: 4,
                source: 10,
                target: (1, 2),
            }],
        );
        range_content(
            model,
            Direction::SourceToTarget,
            (3, 4),
            AtomKind::Object,
            (3, 4),
            (0, 1),
            true,
        );
        range_content(
            model,
            Direction::TargetToSource,
            (0, 1),
            AtomKind::Object,
            (2, 5),
            (0, 1),
            false,
        );
        inside_point(
            model,
            3,
            AtomKind::Object,
            Correspondence::Object,
            (2, 5),
            (0, 1),
        );
    });
}

#[test]
fn g3_strike_delimiters_and_body_keep_complete_source_domain() {
    use NodeKind::*;
    with_model("~~x~~", |model| {
        structural(
            model,
            &[
                node(Document, None, 0, 1),
                node(Paragraph, Some(0), 0, 1),
                node(Strike, Some(1), 0, 1),
                node(Text, Some(2), 0, 1),
            ],
            "x",
            0,
            0,
            0,
        );
        projection(model, "x", &[(2, 3, 0, 3)], &[], &[]);
        range_content(
            model,
            Direction::SourceToTarget,
            (2, 3),
            AtomKind::Copy,
            (2, 3),
            (0, 1),
            false,
        );
        range_content(
            model,
            Direction::TargetToSource,
            (0, 1),
            AtomKind::Copy,
            (2, 3),
            (0, 1),
            false,
        );
        inside_point(
            model,
            1,
            AtomKind::Hidden,
            Correspondence::Hidden,
            (0, 2),
            (0, 0),
        );
        let mut op = model.budget().begin(CancellationToken::new());
        for index in 0..model.ir().atoms_len() {
            let atom = model.ir().atom_at(index, &mut op).unwrap();
            if atom.kind == AtomKind::Hidden {
                assert_eq!(atom.owner.index(), 2);
            }
        }
    });
}

#[test]
fn g3_table_minimum_pipe_stages_keep_cell_authority_and_missing_extra_policy() {
    use NodeKind::*;
    use markdown_core::ir::{HostRole, NativePhase, PrimitiveKind, StageKind};
    with_model(
        "| A | B |\n| -- | -- |\n| x\\|y | `a\\|b` |\n| &amp; | \\* | ignored |\n| z |\n",
        |model| {
            structural_with_proofs(
                model,
                &[
                    node(Document, None, 0, 18),
                    node(Table, Some(0), 0, 18),
                    node(TableRow, Some(1), 0, 3),
                    node(TableCell, Some(2), 0, 1),
                    node(Text, Some(3), 0, 1),
                    node(TableCell, Some(2), 2, 3),
                    node(Text, Some(5), 2, 3),
                    node(TableRow, Some(1), 4, 11),
                    node(TableCell, Some(7), 4, 7),
                    node(Text, Some(8), 4, 7),
                    node(TableCell, Some(7), 8, 11),
                    node(InlineCode, Some(10), 8, 11),
                    node(TableRow, Some(1), 12, 15),
                    node(TableCell, Some(12), 12, 13),
                    node(Text, Some(13), 12, 13),
                    node(TableCell, Some(12), 14, 15),
                    node(Text, Some(15), 14, 15),
                    node(TableRow, Some(1), 16, 18),
                    node(TableCell, Some(17), 16, 17),
                    node(Text, Some(18), 16, 17),
                    node(TableCell, Some(17), 18, 18),
                ],
                "A\tB\nx|y\ta|b\n&\t*\nz\t",
                13,
                0,
                14,
                (4, 4, 4),
            );
            let separators = [
                Separator {
                    kind: SyntheticKind::CellTab,
                    owner: 5,
                    source: 5,
                    target: (1, 2),
                },
                Separator {
                    kind: SyntheticKind::RowLineFeed,
                    owner: 7,
                    source: 23,
                    target: (3, 4),
                },
                Separator {
                    kind: SyntheticKind::CellTab,
                    owner: 10,
                    source: 30,
                    target: (7, 8),
                },
                Separator {
                    kind: SyntheticKind::RowLineFeed,
                    owner: 12,
                    source: 41,
                    target: (11, 12),
                },
                Separator {
                    kind: SyntheticKind::CellTab,
                    owner: 15,
                    source: 49,
                    target: (13, 14),
                },
                Separator {
                    kind: SyntheticKind::RowLineFeed,
                    owner: 17,
                    source: 66,
                    target: (15, 16),
                },
                Separator {
                    kind: SyntheticKind::CellTab,
                    owner: 20,
                    source: 69,
                    target: (17, 18),
                },
            ];
            let transforms = [
                Transform {
                    source: (25, 27),
                    target: (5, 6),
                    owner: 9,
                    output: "|",
                },
                Transform {
                    source: (33, 35),
                    target: (9, 10),
                    owner: 11,
                    output: "|",
                },
                Transform {
                    source: (42, 47),
                    target: (12, 13),
                    owner: 14,
                    output: "&",
                },
                Transform {
                    source: (50, 52),
                    target: (14, 15),
                    owner: 16,
                    output: "*",
                },
            ];
            projection_with_transforms(
                model,
                "A\tB\nx|y\ta|b\n&\t*\nz\t",
                &[
                    (2, 3, 0, 4),
                    (6, 7, 2, 6),
                    (24, 25, 4, 9),
                    (27, 28, 6, 9),
                    (32, 33, 8, 11),
                    (35, 36, 10, 11),
                    (67, 68, 16, 19),
                ],
                &[],
                &separators,
                &transforms,
            );
            let ir = model.ir();
            let detail = |i: usize| ir.details()[ir.nodes()[i].detail.unwrap().index()];
            let DetailRow::Table {
                columns,
                alignments,
                ..
            } = detail(1)
            else {
                panic!();
            };
            assert_eq!(columns, 2);
            assert_eq!(
                (alignments.start()..alignments.start() + alignments.len())
                    .map(|i| ir.alignments()[i])
                    .collect::<Vec<_>>(),
                vec![Alignment::None, Alignment::None]
            );
            for (index, &row) in [2, 7, 12, 17].iter().enumerate() {
                assert!(
                    matches!(detail(row), DetailRow::Row { index: n, header, .. } if n as usize == index && header == (index == 0))
                );
            }
            for (index, &cell) in [3, 5, 8, 10, 13, 15, 18, 20].iter().enumerate() {
                assert!(
                    matches!(detail(cell), DetailRow::Cell { column, missing, .. } if column as usize == index % 2 && missing == (cell == 20))
                );
            }
            let attributes: Vec<_> = ir
                .attributes()
                .iter()
                .map(|a| (a.owner.index(), a.kind, spans(model, a.sources)))
                .collect();
            assert_eq!(
                attributes,
                vec![
                    (1, AttributeKind::TableColumns, vec![(10, 22)]),
                    (1, AttributeKind::TableAlignment, vec![(12, 14), (17, 19)]),
                    (2, AttributeKind::RowHeader, vec![(0, 10)]),
                    (3, AttributeKind::CellMissing, vec![(1, 4)]),
                    (5, AttributeKind::CellMissing, vec![(5, 8)]),
                    (7, AttributeKind::RowHeader, vec![(22, 40)]),
                    (8, AttributeKind::CellMissing, vec![(23, 29)]),
                    (10, AttributeKind::CellMissing, vec![(30, 38)]),
                    (12, AttributeKind::RowHeader, vec![(40, 65)]),
                    (13, AttributeKind::CellMissing, vec![(41, 48)]),
                    (15, AttributeKind::CellMissing, vec![(49, 53)]),
                    (17, AttributeKind::RowHeader, vec![(65, 71)]),
                    (18, AttributeKind::CellMissing, vec![(66, 69)]),
                    (20, AttributeKind::CellMissing, vec![(69, 69)]),
                ]
            );
            let mut op = model.budget().begin(CancellationToken::new());
            let mut stages = Vec::new();
            for proof in ir.proofs() {
                assert_eq!(proof.primitives.len(), 1);
                let primitive = ir.primitives()[proof.primitives.start()];
                assert_eq!(primitive.kind, PrimitiveKind::Copy);
                assert_eq!(proof.stages.len(), 1);
                let stage = ir.stages()[proof.stages.start()];
                let authority = ir.authority(stage.accepted_by, &mut op).unwrap();
                stages.push((
                    stage.kind,
                    primitive.source.start(),
                    primitive.source.end(),
                    ir.text(stage.output).unwrap(),
                ));
                assert_eq!(stage.parameter, 0);
                assert_eq!(
                    (stage.input.start(), stage.input.end()),
                    (0, primitive.source.len())
                );
                if stage.kind == StageKind::TableEscapedPipe {
                    let (cell, consumed) = match primitive.source.start() {
                        25 => (8, (3, 5)),
                        33 => (10, (11, 13)),
                        _ => panic!("unexpected pipe source"),
                    };
                    assert_eq!(authority.owner.index(), cell);
                    assert_eq!(authority.host.owner.index(), cell);
                    assert_eq!(authority.kind, ClaimKind::Table);
                    assert_eq!(authority.phase, NativePhase::Block);
                    assert_eq!(authority.host.role, HostRole::Block);
                    assert_eq!(authority.host.utf8_len, 18);
                    assert_eq!(spans(model, authority.host.domains), vec![(22, 40)]);
                    assert_eq!(
                        (authority.consumed.start(), authority.consumed.end()),
                        consumed
                    );
                    assert_eq!(
                        spans(model, authority.sources),
                        vec![(primitive.source.start(), primitive.source.end())]
                    );
                }
            }
            assert_eq!(
                stages,
                vec![
                    (StageKind::TableEscapedPipe, 25, 27, "|"),
                    (StageKind::TableEscapedPipe, 33, 35, "|"),
                    (StageKind::NamedEntity, 42, 47, "&"),
                    (StageKind::AsciiEscape, 50, 52, "*"),
                ]
            );
            range_content(
                model,
                Direction::SourceToTarget,
                (25, 26),
                AtomKind::Transform,
                (25, 26),
                (5, 6),
                true,
            );
            range_content(
                model,
                Direction::TargetToSource,
                (5, 6),
                AtomKind::Transform,
                (25, 27),
                (5, 6),
                false,
            );
            inside_point(
                model,
                26,
                AtomKind::Transform,
                Correspondence::Atomic,
                (25, 27),
                (5, 6),
            );
            range_content(
                model,
                Direction::TargetToSource,
                (17, 18),
                AtomKind::Synthetic,
                (69, 69),
                (17, 18),
                false,
            );
        },
    );
}

#[test]
fn table_pipe_payload_keeps_cell_authority_through_nested_image_alt_scopes() {
    use markdown_core::ir::{PayloadOwner, PayloadRole, StageKind};
    for source in [
        "|h|\n|--|\n|![[x](u\\|v)](d)|\n",
        "|h|\n|--|\n|![![[x](u\\|v)](a)](b)|\n",
    ] {
        with_model(source, |model| {
            let ir = model.ir();
            let (id, link) = ir
                .nodes()
                .iter()
                .enumerate()
                .find(|(_, row)| row.kind == NodeKind::Link)
                .unwrap();
            let link_id = ir.flows()[link.flow.index()].root;
            assert_ne!(link.flow, model.main_flow_id());
            assert_eq!(ir.nodes()[link_id.index()].kind, NodeKind::AltRoot);
            let payload = ir
                .payloads()
                .iter()
                .find(|p| {
                    p.role == PayloadRole::Destination
                        && matches!(p.owner, PayloadOwner::Node(n) if n.index() == id)
                })
                .unwrap();
            let mut op = model.budget().begin(CancellationToken::new());
            let mut value = String::new();
            let mut pipes = 0;
            for index in payload.atoms.start()..payload.atoms.start() + payload.atoms.len() {
                let atom = ir.atom_at(index, &mut op).unwrap();
                match (atom.kind, atom.content) {
                    (AtomKind::Hidden, _) => {}
                    (AtomKind::Copy, _) => {
                        for (start, end) in spans(model, atom.sources) {
                            value.push_str(&model.source().text()[start as usize..end as usize]);
                        }
                    }
                    (_, AtomContent::Pool(text)) => value.push_str(ir.text(text).unwrap()),
                    _ => panic!("unexpected payload atom"),
                }
                if let Some(proof) = atom.proof {
                    for index in ir.proofs()[proof.index()].stages.start()
                        ..ir.proofs()[proof.index()].stages.start()
                            + ir.proofs()[proof.index()].stages.len()
                    {
                        let stage = ir.stages()[index];
                        if stage.kind == StageKind::TableEscapedPipe {
                            let claim = ir.authority(stage.accepted_by, &mut op).unwrap();
                            let cell = ir.nodes()[claim.owner.index()];
                            assert_eq!(cell.kind, NodeKind::TableCell);
                            assert_eq!(cell.flow, model.main_flow_id());
                            assert_eq!(claim.host.owner, claim.owner);
                            pipes += 1;
                        }
                    }
                }
            }
            assert_eq!(value, "u|v");
            assert_eq!(pipes, 1);
            assert_eq!(op.first_fault(), None);
        });
    }
}
