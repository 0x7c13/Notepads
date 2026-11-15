// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE file in the project root for license information.
// ---------------------------------------------------------------------------------------------

use super::*;
use crate::identity::SourceKey;
use crate::layout::{LayoutDraft, LayoutKey, LayoutStyle};
use crate::model::{AcceptedModel, ParseProgress};
use crate::policy::{CancellationToken, SourceBudget};
use crate::presentation::{PresentationBatch, RUN_STRIKE};
use crate::reader::MainReader;
use crate::source::SourceDocument;

#[test]
fn inline_scripts_compose_with_html_strike_footnotes_and_strict_links() {
    let source = "H~2~O 2^-10^ ~~H~3~O~~\r\n\r\n\
        ^a<sub>b</sub>c^ and <sup>~x~</sup>.\r\n\r\n\
        ^https://example.test/docs?i=1^!\r\n\r\n\
        Text^[a note] and escaped \\^caret^ or `~code~`.\r\n\r\n\
        ^<mark></mark>^";
    let budget = SourceBudget::preview();
    let mut op = budget.begin(CancellationToken::new());
    let input = SourceDocument::new(
        SourceKey::new([126; 16], 1).unwrap(),
        source.as_bytes(),
        &mut op,
    )
    .unwrap()
    .snapshot();
    let extensions = Extensions::from_bits(
        Extensions::INLINE_SCRIPTS.bits()
            | Extensions::HTML_INLINE.bits()
            | Extensions::FOOTNOTES.bits(),
    )
    .unwrap();
    let mut progress = ParseProgress::default();
    let model = AcceptedModel::experimental_parse_with_progress(
        input,
        ParseConfig::with_extensions(Profile::G3, extensions),
        1,
        &mut op,
        &mut progress,
    )
    .unwrap_or_else(|error| panic!("{error:?} at {:?}", progress.stage()));
    let key = LayoutKey::new(
        model.identity(),
        1,
        1,
        1,
        400.0,
        LayoutStyle::markdown(18.0, 8.0).unwrap(),
    )
    .unwrap();
    let mut draft = LayoutDraft::new(model.clone(), key, &mut op).unwrap();
    let requests = draft.measurement_batch(32, &mut op).unwrap().unwrap();
    let batch = PresentationBatch::build(model.clone(), &requests, &mut op).unwrap();
    let marked: Vec<_> = batch
        .script_runs()
        .iter()
        .map(|script| {
            let index = script.run as usize;
            let run = batch.runs()[index];
            let text = batch
                .texts()
                .iter()
                .find(|text| {
                    (text.runs.start()..text.runs.start() + text.runs.len()).contains(&index)
                })
                .unwrap();
            let base = text.value.start as usize;
            let value =
                std::str::from_utf8(&batch.blob()[base..base + text.value.len as usize]).unwrap();
            let utf16 = value.encode_utf16().collect::<Vec<_>>();
            let piece = String::from_utf16(&utf16[run.start as usize..run.end as usize]).unwrap();
            (piece, script.style, run.flags & RUN_STRIKE != 0)
        })
        .collect();
    for (word, superscript) in [
        ("2", false),
        ("-10", true),
        ("3", false),
        ("a", true),
        ("c", true),
    ] {
        assert!(
            marked.iter().any(
                |(piece, style, _)| piece == word && ((style.relative_em < 0.0) == superscript)
            ),
            "{marked:?}"
        );
    }
    assert!(
        marked
            .iter()
            .any(|(piece, style, struck)| piece == "3" && *struck && style.relative_em > 0.0)
    );
    assert!(
        marked
            .iter()
            .any(|(piece, style, _)| piece == "b" && style.size_scale < 0.8)
    );
    assert!(
        marked
            .iter()
            .any(|(piece, style, _)| piece == "x" && style.size_scale < 0.8)
    );
    let external: Vec<_> = batch
        .links()
        .iter()
        .filter(|link| link.local_position.is_none())
        .collect();
    let [link] = external.as_slice() else {
        panic!("{external:?}")
    };
    let start = link.destination.start as usize;
    assert_eq!(
        &batch.blob()[start..start + link.destination.len as usize],
        b"https://example.test/docs?i=1"
    );
    let mut reader = MainReader::new(model, op).unwrap();
    let mut buffer = [0; 1024];
    let result = reader.read_into(&mut buffer).unwrap();
    assert!(result.done);
    let main = std::str::from_utf8(&buffer[..result.written]).unwrap();
    assert!(
        main.contains("H2O 2-10 H3O\nabc and x.\nhttps://example.test/docs?i=1!"),
        "{main:?}"
    );
    assert!(main.contains("escaped ^caret^ or ~code~."), "{main:?}");
    assert!(main.contains("a note"), "{main:?}");
}

#[test]
fn inline_scripts_are_opt_in_and_do_not_enable_strike_in_commonmark() {
    let mut op = SourceBudget::preview().begin(CancellationToken::new());
    let input = SourceDocument::new(
        SourceKey::new([127; 16], 1).unwrap(),
        b"~sub~ ^^raised^^ ~~strike~~",
        &mut op,
    )
    .unwrap()
    .snapshot();
    for (profile, extensions, expected) in [
        (Profile::C0, Extensions::INLINE_SCRIPTS, [1, 1, 0]),
        (Profile::G3, Extensions::INLINE_SCRIPTS, [1, 1, 1]),
        (Profile::G3, Extensions::NONE, [0, 0, 2]),
    ] {
        let model = AcceptedModel::parse(
            input.clone(),
            ParseConfig::with_extensions(profile, extensions),
            1,
            &mut op,
        )
        .unwrap();
        let nodes = model.ir().nodes();
        let counts = [NodeKind::Subscript, NodeKind::Superscript, NodeKind::Strike]
            .map(|kind| nodes.iter().filter(|row| row.kind == kind).count());
        assert_eq!(counts, expected, "{profile:?} {extensions:?}");
    }
}
