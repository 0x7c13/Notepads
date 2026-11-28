// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE file in the project root for license information.
// ---------------------------------------------------------------------------------------------

use super::*;
use crate::identity::SourceKey;
use crate::model::{AcceptedModel, ParseProgress};
use crate::policy::{CancellationToken, SourceBudget};
use crate::source::SourceDocument;
use native::{CalloutDisclosure, CalloutKind};

#[test]
fn callout_titles_share_main_text_and_keep_their_quote_source() {
    let source = "> [!NOTE]\r\n> Default body.\r\n\r\n\
        > [!TIP] Styled **标题**\r\n> Custom body.\r\n\r\n\
        > > [!FAQ]-\r\n> > Nested body.\r\n\r\n\
        > [!unknown] Ordinary quote.\r\n";
    let budget = SourceBudget::preview();
    let mut op = budget.begin(CancellationToken::new());
    let snapshot = SourceDocument::new(
        SourceKey::new([116; 16], 1).unwrap(),
        source.as_bytes(),
        &mut op,
    )
    .unwrap()
    .snapshot();
    let mut progress = ParseProgress::default();
    let model = AcceptedModel::experimental_parse_with_progress(
        snapshot,
        ParseConfig::with_extensions(Profile::G3, Extensions::CALLOUTS),
        1,
        &mut op,
        &mut progress,
    )
    .unwrap_or_else(|error| panic!("{error:?} at {:?}", progress.stage()));
    assert_eq!(model.original_source().text(), source);
    let flow = model.main_flow_id();
    let length = model.ir().flows()[flow.index()].logical_utf16;
    let mut text = String::new();
    crate::reader::text::visit(
        &model,
        flow,
        LogicalSpan::new(0, length).unwrap(),
        &mut op,
        |piece, _| {
            text.push_str(piece);
            Ok(())
        },
    )
    .unwrap();
    assert_eq!(
        text,
        "Note\nDefault body.\nStyled 标题\nCustom body.\nQuestion\nNested body.\n[!unknown] Ordinary quote."
    );
    let titles: Vec<_> = model
        .ir()
        .details()
        .iter()
        .filter_map(|detail| match *detail {
            DetailRow::Callout {
                kind,
                disclosure,
                title,
                ..
            } => {
                let row = model.ir().nodes()[title.index()];
                let quote = model.ir().nodes()[row.parent.unwrap().index()];
                assert_eq!(quote.kind, NodeKind::BlockQuote);
                assert_eq!(quote.first_child, Some(title));
                assert_eq!(row.kind, NodeKind::Paragraph);
                Some((kind, disclosure, row.source_envelope.is_empty()))
            }
            _ => None,
        })
        .collect();
    assert_eq!(
        titles,
        [
            (CalloutKind::Note, CalloutDisclosure::Static, true),
            (CalloutKind::Tip, CalloutDisclosure::Static, false),
            (
                CalloutKind::Question,
                CalloutDisclosure::InitiallyClosed,
                true
            ),
        ]
    );
    use crate::layout::{LayoutDraft, LayoutKey, LayoutStyle, MeasuredBlock};
    use crate::presentation::{PresentationBatch, RUN_BOLD};

    let key = LayoutKey::new(
        model.identity(),
        1,
        1,
        1,
        400.0,
        LayoutStyle::markdown(16.0, 8.0).unwrap(),
    )
    .unwrap();
    let mut draft = LayoutDraft::new(model.clone(), key, &mut op).unwrap();
    let measurement = draft.measurement_batch(32, &mut op).unwrap().unwrap();
    let batch = PresentationBatch::build(model, &measurement, &mut op).unwrap();
    for block in batch.texts() {
        let start = block.value.start as usize;
        let end = start + block.value.len as usize;
        let value = std::str::from_utf8(&batch.blob()[start..end]).unwrap();
        let is_title = matches!(value, "Note" | "Styled 标题" | "Question");
        assert!(
            batch.runs()[block.runs.indices(batch.runs().len()).unwrap()]
                .iter()
                .all(|run| (run.flags & RUN_BOLD != 0) == is_title),
            "{value:?}"
        );
    }
    let measured: Vec<_> = measurement
        .requests
        .iter()
        .map(|request| MeasuredBlock {
            node: request.node,
            height: 18.0,
        })
        .collect();
    draft
        .submit_measurements(key, 0, &measured, &mut op)
        .unwrap();
    let layout = draft.finish(&mut op).unwrap();
    assert_eq!(
        layout
            .quotes()
            .iter()
            .map(|quote| quote.callout)
            .collect::<Vec<_>>(),
        [
            Some(CalloutKind::Note),
            Some(CalloutKind::Tip),
            None,
            Some(CalloutKind::Question),
            None
        ]
    );
}
