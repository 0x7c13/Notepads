// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE file in the project root for license information.
// ---------------------------------------------------------------------------------------------

use super::*;
use crate::identity::SourceKey;
use crate::mapping::{Affinity, RangeInput, RangeQuery, RangeResult};
use crate::model::{AcceptedModel, ParseProgress};
use crate::policy::{CancellationToken, SourceBudget};
use crate::source::SourceDocument;

#[test]
fn emoji_shortcode_accepts_aliases_longer_than_an_html_entity() {
    let mut op = SourceBudget::preview().begin(CancellationToken::new());
    let source = ":face_with_open_eyes_and_hand_over_mouth: :hand_with_index_finger_and_thumb_crossed: :south_georgia_south_sandwich_islands:";
    let original = SourceDocument::new(
        SourceKey::new([131; 16], 1).unwrap(),
        source.as_bytes(),
        &mut op,
    )
    .unwrap()
    .snapshot();
    let model = AcceptedModel::parse(
        original,
        ParseConfig::with_extensions(Profile::G3, Extensions::EMOJI_SHORTCODES),
        1,
        &mut op,
    )
    .unwrap();
    let mut reader = crate::reader::MainReader::new(model, op).unwrap();
    let mut buffer = [0; 128];
    let read = reader.read_into(&mut buffer).unwrap();
    assert!(read.done);
    assert_eq!(&buffer[..read.written], "🫢 🫰 🇬🇸".as_bytes());
}

#[test]
fn emoji_shortcode_zwj_sequence_maps_as_one_atomic_transform() {
    let source = "中:family_man_woman_girl_boy:Z";
    let mut op = SourceBudget::preview().begin(CancellationToken::new());
    let document = SourceDocument::new(
        SourceKey::new([129; 16], 1).unwrap(),
        source.as_bytes(),
        &mut op,
    )
    .unwrap()
    .snapshot();
    let mut progress = ParseProgress::default();
    let model = AcceptedModel::experimental_parse_with_progress(
        document,
        ParseConfig::with_extensions(Profile::C0, Extensions::EMOJI_SHORTCODES),
        1,
        &mut op,
        &mut progress,
    )
    .unwrap_or_else(|error| panic!("{error:?} at {:?}", progress.stage()));
    let flow = model.main_flow_id();
    let mut visible = String::new();
    crate::reader::text::visit(
        &model,
        flow,
        LogicalSpan::new(0, 13).unwrap(),
        &mut op,
        |text, _| {
            visible.push_str(text);
            Ok(())
        },
    )
    .unwrap();
    assert_eq!(visible, "中👨‍👩‍👧‍👦Z");
    assert_eq!(
        model
            .ir()
            .stages()
            .iter()
            .filter(|row| row.kind == StageKind::EmojiShortcode)
            .count(),
        1
    );
    for (input, expected_source, expected_target) in [
        (
            RangeInput::Source {
                target: TextTarget::Flow(flow),
                span: SourceSpan::new(4, 8).unwrap(),
            },
            (4, 8),
            (1, 12),
        ),
        (
            RangeInput::Flow {
                flow,
                span: LogicalSpan::new(3, 4).unwrap(),
            },
            (3, 30),
            (3, 4),
        ),
    ] {
        let response = model
            .map_range(
                &RangeQuery {
                    model: model.identity(),
                    input,
                    empty_affinity: Affinity::After,
                },
                &mut op,
            )
            .unwrap();
        let [RangeResult::Selection(rows)] = response.results() else {
            panic!()
        };
        assert_eq!(rows.len(), 1);
        let selection = response.selections()[rows.start()];
        assert!(selection.expanded);
        assert_eq!(selection.contributions.len(), 1);
        let contribution = response.contributions()[selection.contributions.start()];
        assert_eq!(contribution.kind, AtomKind::Transform);
        assert_eq!(
            (contribution.source.start(), contribution.source.end()),
            expected_source
        );
        assert_eq!(
            (contribution.target.start, contribution.target.end),
            expected_target
        );
    }
    let invalid = model.map_range(
        &RangeQuery {
            model: model.identity(),
            input: RangeInput::Flow {
                flow,
                span: LogicalSpan::new(2, 3).unwrap(),
            },
            empty_affinity: Affinity::After,
        },
        &mut op,
    );
    assert!(matches!(invalid, Err(CoreError::InvalidBoundary)));
}

#[test]
fn emoji_shortcode_shields_opaque_and_metadata_contexts() {
    let source = "Plain :smile: :+1::thumbsup: :heart: :woman_technologist: :unknown: :shipit:.\n\n\
                  Literal \\:smile: &#58;smile: `:smile:` <code>:smile:</code> <textarea>:smile:</textarea>.\n\n\
                  <b>:smile:</b> [label :smile:](https://example.test/:smile: \":smile:\")\n\n\
                  https://example.test/:smile:/docs\n\n\
                  | Emoji |\n|---|\n| :thumbsup: |";
    let mut op = SourceBudget::preview().begin(CancellationToken::new());
    let document = SourceDocument::new(
        SourceKey::new([130; 16], 1).unwrap(),
        source.as_bytes(),
        &mut op,
    )
    .unwrap()
    .snapshot();
    for enabled in [false, true] {
        let html = Extensions::HTML_INLINE.bits();
        let bits = if enabled {
            html | Extensions::EMOJI_SHORTCODES.bits()
        } else {
            html
        };
        let mut progress = ParseProgress::default();
        let model = AcceptedModel::experimental_parse_with_progress(
            document.clone(),
            ParseConfig::with_extensions(Profile::G3, Extensions::from_bits(bits).unwrap()),
            1,
            &mut op,
            &mut progress,
        )
        .unwrap_or_else(|error| panic!("{error:?} at {:?}", progress.stage()));
        let flow = model.main_flow_id();
        let mut visible = String::new();
        let span = LogicalSpan::new(0, model.ir().flows()[flow.index()].logical_utf16).unwrap();
        crate::reader::text::visit(&model, flow, span, &mut op, |text, _| {
            visible.push_str(text);
            Ok(())
        })
        .unwrap();
        if enabled {
            assert!(
                visible.contains("Plain 😄 👍👍 ❤️ 👩‍💻 :unknown: :shipit:."),
                "{visible:?}"
            );
            assert!(visible.contains("😄 label 😄"), "{visible:?}");
            assert!(visible.ends_with("Emoji\n👍"), "{visible:?}");
        } else {
            assert!(
                visible.contains("Plain :smile: :+1::thumbsup:"),
                "{visible:?}"
            );
            assert!(visible.ends_with("Emoji\n:thumbsup:"), "{visible:?}");
        }
        let textarea = if enabled { "😄" } else { ":smile:" };
        assert!(
            visible.contains(&format!(
                "Literal :smile: :smile: :smile: :smile: <textarea>{textarea}</textarea>."
            )),
            "{visible:?}"
        );
        assert!(
            visible.contains("https://example.test/:smile:/docs"),
            "{visible:?}"
        );
        let title = model
            .ir()
            .payloads()
            .iter()
            .find(|payload| payload.role == PayloadRole::Title)
            .copied()
            .unwrap();
        assert_eq!(model.ir().text(title.value).unwrap(), ":smile:");
    }
}
