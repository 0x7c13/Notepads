// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE file in the project root for license information.
// ---------------------------------------------------------------------------------------------

use super::*;
use crate::identity::SourceKey;
use crate::model::{AcceptedModel, ParseProgress};
use crate::policy::{CancellationToken, SourceBudget};
use crate::source::SourceDocument;

#[test]
fn local_wiki_links_retain_a_raw_destination_and_authored_label() {
    let text = concat!(
        "[[Note A]] [[章节/开发#Plan B|alias 😀]] [[#A local heading]]\n\n",
        "`[[in code]]` and \\[[escaped]] and [[empty|]].",
    );
    let mut op = SourceBudget::preview().begin(CancellationToken::new());
    let source = SourceDocument::new(
        SourceKey::new([137; 16], 1).unwrap(),
        text.as_bytes(),
        &mut op,
    )
    .unwrap()
    .snapshot();
    let mut progress = ParseProgress::default();
    let model = AcceptedModel::experimental_parse_with_progress(
        source,
        ParseConfig::with_extensions(Profile::G3, Extensions::LOCAL_WIKI_LINKS),
        1,
        &mut op,
        &mut progress,
    )
    .unwrap_or_else(|error| panic!("{error:?} at {:?}", progress.stage()));
    let ir = model.ir();
    let mut links = Vec::new();
    for row in ir.nodes().iter().filter(|row| row.kind == NodeKind::Link) {
        let DetailRow::Link { destination, .. } = ir.details()[row.detail.unwrap().index()] else {
            panic!("invalid wiki link detail");
        };
        let raw = ir
            .text_query(ir.payloads()[destination.index()].value, &mut op)
            .unwrap();
        let mut label = String::new();
        crate::reader::text::visit(&model, row.flow, row.logical, &mut op, |value, _| {
            label.push_str(value);
            Ok(())
        })
        .unwrap();
        links.push((raw.to_owned(), label));
    }
    assert_eq!(
        links,
        [
            ("Note A".into(), "Note A".into()),
            ("章节/开发#Plan B".into(), "alias 😀".into()),
            ("#A local heading".into(), "#A local heading".into())
        ]
    );
}

#[test]
fn local_wiki_links_bind_images_to_their_authored_sizes() {
    let text = "![[relative/Tile %25.png|120x90]] ![[pic.SVG|100]] ![[logo.webp|Detailed alt]] ![[Note.md]]\n";
    let mut op = SourceBudget::preview().begin(CancellationToken::new());
    let source = SourceDocument::new(
        SourceKey::new([138; 16], 1).unwrap(),
        text.as_bytes(),
        &mut op,
    )
    .unwrap()
    .snapshot();
    let mut progress = ParseProgress::default();
    let model = AcceptedModel::experimental_parse_with_progress(
        source,
        ParseConfig::with_extensions(Profile::G3, Extensions::LOCAL_WIKI_LINKS),
        1,
        &mut op,
        &mut progress,
    )
    .unwrap_or_else(|error| panic!("{error:?} at {:?}", progress.stage()));
    let sizes = model
        .ir()
        .objects()
        .iter()
        .filter_map(|row| match row {
            ObjectRow::Image { wiki_size, .. } => Some(*wiki_size),
            _ => None,
        })
        .collect::<Vec<_>>();
    assert_eq!(sizes, [Some((120, 90)), Some((100, 0)), Some((0, 0))]);
}

#[test]
fn local_wiki_links_compose_with_explicit_markdown_and_gfm_cell_escapes() {
    let text = concat!(
        "[[ordinary]](/destination) [source definition][ref]\n\n",
        "[[explicit]][ref]\n\n",
        "[[A]][[B]] [[title|bracket \\[abc\\]]]\n\n",
        "[[unclosed paren]](unfinished\n\n",
        "[[undefined reference]][not present]\n\n",
        "[[unclosed bracket]][\n\n",
        "| Links |\n| --- |\n| [[filename\\|lab label]] |\n\n",
        "[ref]: /reference\n",
    );
    let root = SourceBudget::preview();
    let mut op = root.begin(CancellationToken::new());
    let source = SourceDocument::new(
        SourceKey::new([139; 16], 1).unwrap(),
        text.as_bytes(),
        &mut op,
    )
    .unwrap()
    .snapshot();
    let mut progress = ParseProgress::default();
    let model = AcceptedModel::experimental_parse_with_progress(
        source,
        ParseConfig::with_extensions(Profile::G3, Extensions::LOCAL_WIKI_LINKS),
        1,
        &mut op,
        &mut progress,
    )
    .unwrap_or_else(|error| panic!("{error:?} at {:?}", progress.stage()));
    let ir = model.ir();
    let mut links = Vec::new();
    for row in ir.nodes().iter().filter(|row| row.kind == NodeKind::Link) {
        let DetailRow::Link {
            destination, claim, ..
        } = ir.details()[row.detail.unwrap().index()]
        else {
            panic!("invalid link detail");
        };
        let target = ir
            .text_query(ir.payloads()[destination.index()].value, &mut op)
            .unwrap();
        let mut label = String::new();
        crate::reader::text::visit(&model, row.flow, row.logical, &mut op, |part, _| {
            label.push_str(part);
            Ok(())
        })
        .unwrap();
        links.push((
            target.to_owned(),
            label,
            ir.authority(claim, &mut op).unwrap().kind == ClaimKind::WikiLink,
        ));
    }
    assert!(links.contains(&("/destination".into(), "[ordinary]".into(), false)));
    assert!(
        links.contains(&("/reference".into(), "[explicit]".into(), false)),
        "{links:?}"
    );
    assert!(links.contains(&("A".into(), "A".into(), true)));
    assert!(links.contains(&("B".into(), "B".into(), true)));
    assert!(links.contains(&("title".into(), "bracket [abc]".into(), true)));
    assert!(links.contains(&("filename".into(), "lab label".into(), true)));
    for label in ["unclosed paren", "undefined reference", "unclosed bracket"] {
        assert!(
            links.contains(&(label.into(), label.into(), true)),
            "{links:?}"
        );
    }
}

#[test]
fn local_note_embeds_hold_an_authored_target_and_one_parent_object() {
    let text = concat!(
        "Before\n\n![[Sub/Child 1.2#Chapter#Next]]\n\n",
        "![[#In this note]]\n\n",
        "`![[code]]` · ![[https://example.org/remote.md]] · ![[page|not a note name]]\n\n",
        "After\n",
    );
    let mut op = SourceBudget::preview().begin(CancellationToken::new());
    let source = SourceDocument::new(
        SourceKey::new([140; 16], 1).unwrap(),
        text.as_bytes(),
        &mut op,
    )
    .unwrap()
    .snapshot();
    let mut progress = ParseProgress::default();
    let model = AcceptedModel::experimental_parse_with_progress(
        source,
        ParseConfig::with_extensions(Profile::G3, Extensions::NOTE_EMBEDS),
        1,
        &mut op,
        &mut progress,
    )
    .unwrap_or_else(|error| panic!("{error:?} at {:?}", progress.stage()));
    let ir = model.ir();
    let targets = ir
        .objects()
        .iter()
        .filter_map(|row| match row {
            ObjectRow::Derived {
                kind: DerivedKind::NoteEmbed,
                node,
                input,
                ..
            } => {
                let row = ir.nodes()[node.index()];
                assert_eq!(row.logical.len(), 1);
                assert_eq!(row.kind, NodeKind::NoteEmbed);
                Some(
                    ir.text_query(ir.payloads()[input.index()].value, &mut op)
                        .unwrap(),
                )
            }
            _ => None,
        })
        .collect::<Vec<_>>();
    assert_eq!(targets, ["Sub/Child 1.2#Chapter#Next", "#In this note"]);
    assert_eq!(ir.flows().len(), 1);
}
