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
fn review_regression_large_toc_labels_survive_emission_and_validation() {
    let label = "a".repeat(crate::policy::CHECKPOINT_BYTES as usize + 1);
    let text = format!("[TOC]\n\n# {label}\n");
    let budget = SourceBudget::model_fixture();
    let mut op = budget.begin(CancellationToken::new());
    let source = SourceDocument::new(
        SourceKey::new([137; 16], 1).unwrap(),
        text.as_bytes(),
        &mut op,
    )
    .unwrap();
    let model = AcceptedModel::parse(
        source.snapshot(),
        ParseConfig::with_extensions(Profile::G3, Extensions::TABLE_OF_CONTENTS),
        1,
        &mut op,
    )
    .unwrap();
    let entry = model
        .ir()
        .nodes()
        .iter()
        .find(|row| row.kind == NodeKind::TableOfContentsEntry)
        .unwrap();
    let mut actual = String::new();
    crate::reader::text::visit(&model, entry.flow, entry.logical, &mut op, |part, _| {
        actual.push_str(part);
        Ok(())
    })
    .unwrap();
    assert_eq!(actual, label);
}

#[test]
fn table_of_contents_recognizes_an_exact_standalone_marker() {
    let source = "Lead\r\n[ToC]\r\nContinued.\r\n\r\n   [[_toc_]] \r\n\r\n\
        A literal [TOC] here, `[TOC]` and \\[TOC].\r\n\r\n\
        ```md\r\n[TOC]\r\n```\r\n\r\n\
        | Header |\r\n|---|\r\n| [TOC] |\r\n\r\n\
        [TOC](/target)\r\n";
    let mut op = SourceBudget::preview().begin(CancellationToken::new());
    let original = SourceDocument::new(
        SourceKey::new([134; 16], 1).unwrap(),
        source.as_bytes(),
        &mut op,
    )
    .unwrap()
    .snapshot();
    for extensions in [Extensions::NONE, Extensions::TABLE_OF_CONTENTS] {
        let mut progress = ParseProgress::default();
        let model = AcceptedModel::experimental_parse_with_progress(
            original.clone(),
            ParseConfig::with_extensions(Profile::G3, extensions),
            1,
            &mut op,
            &mut progress,
        )
        .unwrap_or_else(|error| panic!("{error:?} at {:?}", progress.stage()));
        let ir = model.ir();
        let markers = ir
            .nodes()
            .iter()
            .filter(|row| row.kind == NodeKind::TableOfContents)
            .map(|row| {
                &source[row.source_envelope.start() as usize..row.source_envelope.end() as usize]
            })
            .collect::<Vec<_>>();
        if extensions == Extensions::NONE {
            assert!(markers.is_empty());
        } else {
            assert_eq!(markers, ["[ToC]", "   [[_toc_]] "]);
        }
    }
}

#[test]
fn table_of_contents_entries_bind_distinct_headings_in_document_order() {
    let source = "[TOC]\n\n## repeat {#one}\n\n#### ![img *alt*](url) `a b` :rocket:\n\n## repeat {#two}\n\n#\n#\n\n[[_TOC_]]\n";
    let mut op = SourceBudget::preview().begin(CancellationToken::new());
    let original = SourceDocument::new(
        SourceKey::new([135; 16], 1).unwrap(),
        source.as_bytes(),
        &mut op,
    )
    .unwrap()
    .snapshot();
    let mut progress = ParseProgress::default();
    let model = AcceptedModel::experimental_parse_with_progress(
        original,
        ParseConfig::with_extensions(Profile::G3, Extensions::ALL),
        1,
        &mut op,
        &mut progress,
    )
    .unwrap_or_else(|error| panic!("{error:?} at {:?}", progress.stage()));
    let ir = model.ir();
    let headings = ir
        .nodes()
        .iter()
        .enumerate()
        .filter(|(_, row)| row.kind == NodeKind::Heading)
        .map(|(at, _)| NodeId::from_index(at).unwrap())
        .collect::<Vec<_>>();
    assert_eq!(headings.len(), 5);
    let entries = ir
        .details()
        .iter()
        .filter_map(|detail| match detail {
            DetailRow::TableOfContentsEntry { heading, depth, .. } => Some((*heading, *depth)),
            _ => None,
        })
        .collect::<Vec<_>>();
    let expected = headings
        .iter()
        .copied()
        .zip([0, 1, 0, 0, 0])
        .collect::<Vec<_>>();
    assert_eq!(entries, expected.repeat(2));
}

#[test]
fn table_of_contents_uses_the_reading_outline_labels() {
    let source = r#"[TOC]

# Same *text* [label](/path) &amp; `code` ![img *alt*](/p) $`x²`$ :rocket:

## <code>a &amp; __b__</code> Before<br>after <mark>marked</mark>

## <span id="x"></span>

Quoted heading  
one more line
--------------------

## Heading [^note] and ^[direct note]

[^note]: note

    ### Note heading

> # Nested quote

- # Nested list
"#;
    let root = SourceBudget::preview();
    let mut op = root.begin(CancellationToken::new());
    let source = SourceDocument::new(
        SourceKey::new([136; 16], 1).unwrap(),
        source.as_bytes(),
        &mut op,
    )
    .unwrap()
    .snapshot();
    for extensions in [Extensions::TABLE_OF_CONTENTS, Extensions::ALL] {
        let mut progress = ParseProgress::default();
        let model = AcceptedModel::experimental_parse_with_progress(
            source.clone(),
            ParseConfig::with_extensions(Profile::G3, extensions),
            1,
            &mut op,
            &mut progress,
        )
        .unwrap_or_else(|error| panic!("{error:?} at {:?}", progress.stage()));
        let outline = crate::outline::Outline::build(model.clone(), &mut op).unwrap();
        let marker = model
            .ir()
            .nodes()
            .iter()
            .find(|row| row.kind == NodeKind::TableOfContents)
            .unwrap();
        let mut entry = marker.first_child;
        for target in outline.headings() {
            let row = model.ir().nodes()[entry.unwrap().index()];
            let DetailRow::TableOfContentsEntry { heading, .. } =
                model.ir().details()[row.detail.unwrap().index()]
            else {
                panic!("missing TOC detail")
            };
            assert_eq!(heading, target.node);
            let mut label = String::new();
            crate::reader::text::visit(&model, row.flow, row.logical, &mut op, |value, _| {
                label.push_str(value);
                Ok(())
            })
            .unwrap();
            let from = target.title.start as usize;
            let to = from + target.title.len as usize;
            let expected = std::str::from_utf8(&outline.blob()[from..to]).unwrap();
            assert_eq!(
                label,
                if expected.is_empty() {
                    EMPTY_TOC_HEADING
                } else {
                    expected
                }
            );
            entry = row.next_sibling;
        }
        assert!(entry.is_none());
    }
}
