// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE file in the project root for license information.
// ---------------------------------------------------------------------------------------------

use super::*;
use crate::identity::SourceKey;
use crate::layout::{LayoutDraft, LayoutKey, LayoutStyle};
use crate::model::{AcceptedModel, ParseProgress};
use crate::policy::{CancellationToken, SourceBudget};
use crate::presentation::{PresentationBatch, RUN_BOLD};
use crate::source::SourceDocument;

#[test]
fn description_list_preserves_multiple_definitions_terms_and_body_geometry() {
    let source = "中文 *term*\r\n: First definition.\r\n: Second definition.\r\n\r\n\
        Next term\r\n~ Tilde definition.\r\n\r\nAfterward.\r\n";
    let budget = SourceBudget::preview();
    let mut op = budget.begin(CancellationToken::new());
    let source = SourceDocument::new(
        SourceKey::new([121; 16], 1).unwrap(),
        source.as_bytes(),
        &mut op,
    )
    .unwrap()
    .snapshot();
    let mut progress = ParseProgress::default();
    let model = AcceptedModel::experimental_parse_with_progress(
        source,
        ParseConfig::with_extensions(Profile::G3, Extensions::DESCRIPTION_LISTS),
        1,
        &mut op,
        &mut progress,
    )
    .unwrap_or_else(|error| panic!("{error:?} at {:?}", progress.stage()));
    let ir = model.ir();
    assert_eq!(
        ir.nodes()
            .iter()
            .filter(|row| row.kind == NodeKind::DescriptionItem)
            .count(),
        3
    );
    assert_eq!(
        ir.nodes()
            .iter()
            .filter(|row| row.kind == NodeKind::DescriptionTerm)
            .count(),
        2
    );
    let flow = model.main_flow_id();
    let mut main = String::new();
    crate::reader::text::visit(
        &model,
        flow,
        LogicalSpan::new(0, ir.flows()[flow.index()].logical_utf16).unwrap(),
        &mut op,
        |piece, _| {
            main.push_str(piece);
            Ok(())
        },
    )
    .unwrap();
    assert_eq!(
        main,
        "中文 term\nFirst definition.\nSecond definition.\nNext term\nTilde definition.\nAfterward."
    );
    let key = LayoutKey::new(
        model.identity(),
        1,
        1,
        1,
        360.0,
        LayoutStyle::markdown(18.0, 8.0).unwrap(),
    )
    .unwrap();
    let mut layout = LayoutDraft::new(model.clone(), key, &mut op).unwrap();
    let requests = layout.measurement_batch(32, &mut op).unwrap().unwrap();
    assert_eq!(
        requests
            .requests
            .iter()
            .map(|row| row.x)
            .collect::<Vec<_>>(),
        [0.0, 18.0, 18.0, 0.0, 18.0, 0.0]
    );
    let batch = PresentationBatch::build(model, &requests, &mut op).unwrap();
    for (index, text) in batch.texts().iter().enumerate() {
        let runs = text.runs.indices(batch.runs().len()).unwrap();
        assert!(
            batch.runs()[runs]
                .iter()
                .all(|run| (run.flags & RUN_BOLD != 0) == matches!(index, 0 | 3))
        );
    }
}

#[test]
fn nested_description_indent_does_not_erase_quote_or_list_geometry() {
    let source = "> Quoted term\n> :\tDefinition.\n>\n>     Second paragraph.\n>\n>     - Nested list item.\n\nTail.\n";
    let budget = SourceBudget::preview();
    let mut op = budget.begin(CancellationToken::new());
    let document = SourceDocument::new(
        SourceKey::new([122; 16], 1).unwrap(),
        source.as_bytes(),
        &mut op,
    )
    .unwrap()
    .snapshot();
    let model = AcceptedModel::parse(
        document.clone(),
        ParseConfig::with_extensions(Profile::G3, Extensions::DESCRIPTION_LISTS),
        1,
        &mut op,
    )
    .unwrap();
    let plain = AcceptedModel::parse(document, ParseConfig::new(Profile::G3), 2, &mut op).unwrap();
    assert!(
        plain
            .ir()
            .nodes()
            .iter()
            .all(|row| row.kind != NodeKind::DescriptionList)
    );
    let key = LayoutKey::new(
        model.identity(),
        1,
        1,
        1,
        260.0,
        LayoutStyle::markdown(18.0, 8.0).unwrap(),
    )
    .unwrap();
    let mut layout = LayoutDraft::new(model, key, &mut op).unwrap();
    let batch = layout.measurement_batch(32, &mut op).unwrap().unwrap();
    let [term, first, second, nested, tail] = batch.requests else {
        panic!("{:?}", batch.requests)
    };
    assert_eq!(
        [term.x, first.x, second.x, nested.x, tail.x],
        [15.0, 33.0, 33.0, 51.0, 0.0]
    );
    assert_eq!(nested.marker_x, first.x);
    assert!(second.block_gap_before);
    assert_eq!(nested.context.quote_depth, 1);
    assert_eq!(nested.context.description_depth, 1);
}

#[test]
fn invisible_reference_definition_does_not_reject_a_description_model() {
    for separator in ["\n", "\n\n"] {
        let source = format!("[r]: /url{separator}: Definition.\n\nFollow [r].\n");
        let budget = SourceBudget::preview();
        let mut op = budget.begin(CancellationToken::new());
        let document = SourceDocument::new(
            SourceKey::new([123; 16], 1).unwrap(),
            source.as_bytes(),
            &mut op,
        )
        .unwrap()
        .snapshot();
        let mut progress = ParseProgress::default();
        AcceptedModel::experimental_parse_with_progress(
            document,
            ParseConfig::with_extensions(Profile::G3, Extensions::DESCRIPTION_LISTS),
            1,
            &mut op,
            &mut progress,
        )
        .unwrap_or_else(|error| panic!("{source:?}: {error:?} at {:?}", progress.stage()));
    }
}
