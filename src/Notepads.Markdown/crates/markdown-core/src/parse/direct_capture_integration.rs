// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE file in the project root for license information.
// ---------------------------------------------------------------------------------------------

//! Capture regressions through the actual parser, independent validator and index.

use crate::config::{ParseConfig, Profile};
use crate::identity::SourceKey;
use crate::ir::{
    AtomContent, AtomKind, ClaimKind, CompactIr, DetailRow, FlowScope, ObjectRow, PayloadId,
    PayloadOwner, PayloadRole, SyntheticKind,
};
use crate::model::AcceptedModel;
use crate::policy::{CancellationToken, Operation, SourceBudget};
use crate::source::SourceDocument;

fn inspect(text: &str, check: impl Fn(&CompactIr, &mut Operation)) {
    // Exercise both independent-validation/index dispatches with unchanged policy.
    for budget in [SourceBudget::model_fixture(), SourceBudget::task_fixture()] {
        let mut prepare = budget.begin(CancellationToken::new());
        let source = SourceDocument::new(
            SourceKey::new([114; 16], 1).unwrap(),
            text.as_bytes(),
            &mut prepare,
        )
        .unwrap()
        .snapshot();
        let mut parse = budget.begin(CancellationToken::new());
        let model =
            AcceptedModel::parse(source.clone(), ParseConfig::new(Profile::C0), 1, &mut parse)
                .unwrap();
        assert_eq!(model.source().text(), text);
        let mut query = budget.begin(CancellationToken::new());
        check(model.ir(), &mut query);
        drop((model, source));
        let usage = budget.usage();
        assert_eq!(usage.auxiliary_capacity, 0);
        assert_eq!(usage.source_capacity, 0);
        assert_eq!(usage.source_owners, 0);
    }
}

fn payload(ir: &CompactIr, id: PayloadId) -> &str {
    ir.text(ir.payloads()[id.index()].value).unwrap()
}

#[test]
fn definitions_keep_history_empty_values_and_independent_reference_owners() {
    inspect(
        "[K]: /win \"A &amp; 中\"\n[k]: /lose\n[unused]: /unused \"\"\n\n[first][K] [second][k] ![alt][K]\n",
        |ir, _| {
            let definitions = ir.definitions();
            assert_eq!(definitions.len(), 3);
            assert!(definitions[0].winner);
            assert!(!definitions[1].winner);
            assert!(definitions[2].winner);
            assert_eq!(payload(ir, definitions[0].destination), "/win");
            assert_eq!(payload(ir, definitions[1].destination), "/lose");
            assert_eq!(payload(ir, definitions[2].destination), "/unused");
            assert_eq!(ir.text(definitions[0].key).unwrap(), "k");
            assert_eq!(ir.text(definitions[1].key).unwrap(), "k");
            assert_eq!(payload(ir, definitions[0].title.unwrap()), "A & 中");
            assert_eq!(definitions[1].title, None);
            assert_eq!(payload(ir, definitions[2].title.unwrap()), "");
            for (index, row) in definitions.iter().enumerate() {
                assert_eq!(
                    ir.payloads()[row.destination.index()].owner,
                    PayloadOwner::Definition(crate::ir::DefinitionId::from_index(index).unwrap())
                );
            }
            let references = ir.references();
            assert_eq!(references.len(), 3);
            assert_eq!(ir.objects().len(), 1);
            for (index, row) in references.iter().enumerate() {
                assert_eq!(row.winner.index(), 0);
                assert_eq!(payload(ir, row.destination), "/win");
                assert_eq!(payload(ir, row.title.unwrap()), "A & 中");
                assert_eq!(
                    ir.payloads()[row.destination.index()].owner,
                    PayloadOwner::Node(row.node)
                );
                assert_eq!(
                    ir.payloads()[row.title.unwrap().index()].owner,
                    PayloadOwner::Node(row.node)
                );
                assert_ne!(row.destination, definitions[0].destination);
                assert_ne!(row.title, definitions[0].title);
                for previous in references.iter().take(index) {
                    assert_ne!(row.node, previous.node);
                    assert_ne!(row.destination, previous.destination);
                    assert_ne!(row.title, previous.title);
                    assert_ne!(row.claim, previous.claim);
                }
            }
            assert!(references.iter().all(|row| row.winner.index() != 2));
        },
    );
}

#[test]
fn captured_fields_keep_decoded_utf8_across_chunks_and_distinct_final_segments() {
    let long = "中😀".repeat(733); // 5,131 bytes; boundaries cannot assume ASCII.
    let text = format!(
        "[none](u) [empty](<> \"\") [long](</a&amp;b> \"{long} &amp; \\*\")\n\n```{long}&amp;\\*\nx\n```\n"
    );
    inspect(&text, |ir, _| {
        let links: Vec<_> = ir
            .details()
            .iter()
            .filter_map(|row| match row {
                DetailRow::Link {
                    destination, title, ..
                } => Some((*destination, *title)),
                _ => None,
            })
            .collect();
        assert_eq!(links.len(), 3);
        assert_eq!(links[0].1, None);
        assert_eq!(payload(ir, links[1].0), "");
        assert_eq!(payload(ir, links[1].1.unwrap()), "");
        assert_eq!(payload(ir, links[2].0), "/a&b");
        let title = links[2].1.unwrap();
        assert_eq!(payload(ir, title), format!("{long} & *"));
        let info = ir
            .details()
            .iter()
            .find_map(|row| match row {
                DetailRow::Code { info: Some(id), .. } => Some(*id),
                _ => None,
            })
            .unwrap();
        assert_eq!(ir.payloads()[info.index()].role, PayloadRole::FenceInfo);
        assert_eq!(payload(ir, info), format!("{long}&*"));
        let title_range = ir.payloads()[title.index()].value;
        let info_range = ir.payloads()[info.index()].value;
        assert!(title_range.len() > 4096 && info_range.len() > 4096);
        assert!(title_range.start() + title_range.len() <= info_range.start());
        assert!(!ir.proofs().is_empty());
        // Each full field remains contiguous; the final pool has multiple actual
        // segments. This does not claim that one TextSlice straddles segments.
        let mut large_segments = 0;
        ir.text.for_each_segment(|segment| {
            if segment.len() > 4096 {
                large_segments += 1;
            }
        });
        assert!(large_segments >= 2);
    });
}

#[test]
fn captured_mailto_value_keeps_the_real_prefix_authority() {
    inspect("<a.b@example.org>\n", |ir, op| {
        let destination = ir
            .payloads()
            .iter()
            .find(|row| row.role == PayloadRole::Destination)
            .copied()
            .unwrap();
        assert_eq!(
            ir.text(destination.value).unwrap(),
            "mailto:a.b@example.org"
        );
        let atom = ir.atom_at(destination.atoms.start(), op).unwrap();
        assert_eq!(atom.kind, AtomKind::Synthetic);
        assert_eq!(atom.target.start, 0);
        assert_eq!(atom.target.end, 7);
        let AtomContent::Pool(prefix) = atom.content else {
            panic!("mailto prefix must retain its actual text");
        };
        assert_eq!(ir.text(prefix).unwrap(), "mailto:");
        assert_eq!(
            ir.authority(atom.claim, op).unwrap().kind,
            ClaimKind::ProjectionSeparator(SyntheticKind::MailtoScheme)
        );
    });
}

#[test]
fn list_marker_fallback_previous_item_and_image_alt_fifo_survive_ast_retirement() {
    inspect("3. ![first](one)\n4. ![second](two)\n", |ir, _| {
        assert!(
            ir.details()
                .iter()
                .any(|row| matches!(row, DetailRow::List { start: 3, .. }))
        );
        let items: Vec<_> = ir
            .details()
            .iter()
            .filter_map(|row| match row {
                DetailRow::Item {
                    occurrence, value, ..
                } => Some((*occurrence, *value)),
                _ => None,
            })
            .collect();
        assert_eq!(items, [(0, 3), (1, 4)]);
        assert_eq!(ir.objects().len(), 2);
        assert_eq!(ir.flows().len(), 3);
        assert_eq!(ir.flows()[0].scope, FlowScope::Main);
        for (index, object) in ir.objects().iter().enumerate() {
            let ObjectRow::Image {
                node,
                alt,
                destination,
                ..
            } = object
            else {
                panic!("fixture contains only image objects");
            };
            assert_eq!(alt.index(), index + 1);
            assert_eq!(
                ir.flows()[alt.index()].scope,
                FlowScope::ImageAlt {
                    object: crate::ir::ObjectId::from_index(index).unwrap()
                }
            );
            assert_eq!(payload(ir, *destination), ["one", "two"][index]);
            assert_eq!(
                ir.payloads()[destination.index()].owner,
                PayloadOwner::Node(*node)
            );
            assert_eq!(ir.flows()[alt.index()].logical_utf16, [5, 6][index]);
        }
    });
}
