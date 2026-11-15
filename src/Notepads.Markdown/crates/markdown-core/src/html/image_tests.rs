// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE file in the project root for license information.
// ---------------------------------------------------------------------------------------------

use super::*;
use crate::config::{Extensions, ParseConfig, Profile};
use crate::identity::{PayloadBoundary, PayloadSpan, SourceKey};
use crate::ir::{ObjectRow, PayloadRole};
use crate::mapping::{Correspondence, RelatedSpan};
use crate::policy::{CancellationToken, SourceBudget};
use crate::source::SourceDocument;

fn parse(source: &str) -> AcceptedModel {
    let budget = SourceBudget::preview();
    let mut op = budget.begin(CancellationToken::new());
    let document = SourceDocument::new(
        SourceKey::new([9; 16], 1).unwrap(),
        source.as_bytes(),
        &mut op,
    )
    .unwrap();
    AcceptedModel::parse(
        document.snapshot(),
        ParseConfig::with_extensions(Profile::G3, Extensions::ALL),
        1,
        &mut op,
    )
    .unwrap_or_else(|error| panic!("{source:?}: {error:?}"))
}

fn destinations(model: &AcceptedModel) -> Vec<PayloadId> {
    model
        .ir()
        .payloads()
        .iter()
        .enumerate()
        .filter(|(_, row)| row.role == PayloadRole::Destination)
        .map(|(index, _)| model.payload_id(index as u32 + 1).unwrap())
        .collect()
}

fn point(
    model: &AcceptedModel,
    input: PointInput,
    affinity: Affinity,
) -> crate::mapping::PointResponse {
    let mut op = model.budget().begin(CancellationToken::new());
    model
        .map_point(
            &PointQuery {
                model: model.identity(),
                input,
                affinity,
            },
            &mut op,
        )
        .unwrap()
}

#[test]
fn image_src_exact_points_and_ranges_use_attribute_bytes() {
    for (source, value) in [
        (
            "<p><img src='https://example.org/a.png' alt='icon'></p>",
            "https://example.org/a.png",
        ),
        (
            "<p><IMG ALT=icon SRC=assets/icon.png></p>",
            "assets/icon.png",
        ),
        (
            "Before\r\n\r\n<p>é😀<img src=\"assets/中😀.png\" width=128></p>",
            "assets/中😀.png",
        ),
    ] {
        let model = parse(source);
        let ids = destinations(&model);
        assert_eq!(ids.len(), 1, "{source}");
        let id = ids[0];
        assert_eq!(
            model
                .ir()
                .text(model.ir().payloads()[id.index()].value)
                .unwrap(),
            value
        );
        let start = source.find(value).unwrap() as u64;
        let end = start + value.len() as u64;
        for offset in (0..=value.len()).filter(|&at| value.is_char_boundary(at)) {
            for affinity in [Affinity::Before, Affinity::After] {
                let response = point(
                    &model,
                    PointInput::Source {
                        target: TextTarget::Payload(id),
                        byte: SourceBoundary::new(start + offset as u64),
                    },
                    affinity,
                );
                assert_eq!(
                    response.results()[0].chosen,
                    Position::Payload(PayloadBoundary::new(offset as u64)),
                    "{source} at {offset}"
                );
                assert!(
                    !response.results()[0].effects.atomic,
                    "{source} at {offset}"
                );
                let response = point(
                    &model,
                    PointInput::Payload {
                        payload: id,
                        utf8: PayloadBoundary::new(offset as u64),
                    },
                    affinity,
                );
                assert_eq!(
                    response.results()[0].chosen,
                    Position::Source(SourceBoundary::new(start + offset as u64)),
                    "{source} at {offset}"
                );
                assert!(
                    !response.results()[0].effects.atomic,
                    "{source} at {offset}"
                );
            }
        }
        let mut op = model.budget().begin(CancellationToken::new());
        for (from, to) in [(0, value.len() as u64), (1, 4)] {
            let response = model
                .map_range(
                    &RangeQuery {
                        model: model.identity(),
                        input: RangeInput::Payload {
                            payload: id,
                            span: PayloadSpan::new(from, to).unwrap(),
                        },
                        empty_affinity: Affinity::After,
                    },
                    &mut op,
                )
                .unwrap();
            let selection = response.selections()[0];
            assert!(!selection.expanded && !selection.effects.atomic, "{source}");
            assert_eq!(
                selection.related.spans.slice(response.spans()).unwrap(),
                &[RelatedSpan::Source(
                    SourceSpan::new(start + from, start + to).unwrap()
                )]
            );
            assert!(response.contributions().iter().all(|row| !row.expanded));
            let span = SourceSpan::new(start + from, start + to).unwrap();
            let response = model
                .map_range(
                    &RangeQuery {
                        model: model.identity(),
                        input: RangeInput::Source {
                            target: TextTarget::Payload(id),
                            span,
                        },
                        empty_affinity: Affinity::After,
                    },
                    &mut op,
                )
                .unwrap();
            let selection = response.selections()[0];
            assert_eq!(
                selection.start.chosen,
                Position::Payload(PayloadBoundary::new(from))
            );
            assert_eq!(
                selection.end.chosen,
                Position::Payload(PayloadBoundary::new(to))
            );
            assert!(!selection.expanded && !selection.effects.atomic, "{source}");
            assert!(
                response
                    .contributions()
                    .iter()
                    .all(|row| row.source == span && !row.expanded)
            );
        }
        assert_eq!(end, start + model.ir().payloads()[id.index()].utf8_len);
        assert_eq!(model.original_source().text(), source);
    }
}

#[test]
fn transformed_image_src_has_atomic_attribute_provenance() {
    for (raw, value) in [
        ("  assets/icon.png  ", "assets/icon.png"),
        (
            "&#104;ttps://example.org/Ω?q=&#65;&amp;x=2",
            "https://example.org/Ω?q=A&x=2",
        ),
        ("icons/a b.png", "icons/a b.png"),
        ("https://example.org/a&b.png", "https://example.org/a&b.png"),
    ] {
        let source = format!("<p>é<img alt='😀 icon' src='{raw}' width='128'></p>");
        let model = parse(&source);
        let ids = destinations(&model);
        assert_eq!(ids.len(), 1, "{source}");
        let id = ids[0];
        let payload = model.ir().payloads()[id.index()];
        assert_eq!(model.ir().text(payload.value).unwrap(), value);
        let start = source.find(raw).unwrap() as u64;
        let end = start + raw.len() as u64;
        let raw_span = SourceSpan::new(start, end).unwrap();
        for (affinity, chosen_source, chosen_payload) in [
            (Affinity::Before, start, 0),
            (Affinity::After, end, payload.utf8_len),
        ] {
            let response = point(
                &model,
                PointInput::Payload {
                    payload: id,
                    utf8: PayloadBoundary::new(1),
                },
                affinity,
            );
            let result = response.results()[0];
            assert_eq!(
                result.chosen,
                Position::Source(SourceBoundary::new(chosen_source)),
                "{source}"
            );
            assert_eq!(result.correspondence, Correspondence::Atomic, "{source}");
            assert!(result.effects.atomic, "{source}");
            assert_eq!(
                result.related.spans.slice(response.spans()).unwrap(),
                &[RelatedSpan::Source(raw_span)]
            );
            assert_eq!(result.related.envelope, Some(RelatedSpan::Source(raw_span)));
            assert!(
                response
                    .anchors()
                    .iter()
                    .all(|anchor| (start..=end).contains(&anchor.value()))
            );
            let response = point(
                &model,
                PointInput::Source {
                    target: TextTarget::Payload(id),
                    byte: SourceBoundary::new(start + 1),
                },
                affinity,
            );
            assert_eq!(
                response.results()[0].chosen,
                Position::Payload(PayloadBoundary::new(chosen_payload)),
                "{source}"
            );
            assert_eq!(
                response.results()[0].correspondence,
                Correspondence::Atomic,
                "{source}"
            );
        }
        let mut op = model.budget().begin(CancellationToken::new());
        for (from, to, expanded) in [(0, payload.utf8_len, false), (1, 2, true)] {
            let response = model
                .map_range(
                    &RangeQuery {
                        model: model.identity(),
                        input: RangeInput::Payload {
                            payload: id,
                            span: PayloadSpan::new(from, to).unwrap(),
                        },
                        empty_affinity: Affinity::After,
                    },
                    &mut op,
                )
                .unwrap();
            let selection = response.selections()[0];
            assert!(selection.effects.atomic, "{source}");
            assert_eq!(selection.expanded, expanded, "{source}");
            assert_eq!(
                selection.related.spans.slice(response.spans()).unwrap(),
                &[RelatedSpan::Source(raw_span)]
            );
            assert!(
                response
                    .contributions()
                    .iter()
                    .all(|row| row.expanded == expanded)
            );
        }
        for (span, expanded) in [
            (raw_span, false),
            (SourceSpan::new(start + 1, start + 2).unwrap(), true),
        ] {
            let response = model
                .map_range(
                    &RangeQuery {
                        model: model.identity(),
                        input: RangeInput::Source {
                            target: TextTarget::Payload(id),
                            span,
                        },
                        empty_affinity: Affinity::After,
                    },
                    &mut op,
                )
                .unwrap();
            let selection = response.selections()[0];
            assert!(selection.effects.atomic, "{source}");
            assert_eq!(selection.expanded, expanded, "{source}");
            assert!(!response.contributions().is_empty());
            assert!(
                response
                    .contributions()
                    .iter()
                    .all(|row| row.source == span && row.expanded == expanded)
            );
        }
    }
}

#[test]
fn duplicate_src_uses_first_attribute_and_its_own_source() {
    for source in [
        "<p><img SRC='first.png' src='second.png' alt='icon'></p>",
        "<p><img src='first.png' SRC='second.png' src='third.png'></p>",
    ] {
        let model = parse(source);
        let ids = destinations(&model);
        assert_eq!(ids.len(), 1);
        let id = ids[0];
        assert_eq!(
            model
                .ir()
                .text(model.ir().payloads()[id.index()].value)
                .unwrap(),
            "first.png"
        );
        let start = source.find("first.png").unwrap() as u64;
        let response = point(
            &model,
            PointInput::Payload {
                payload: id,
                utf8: PayloadBoundary::new(3),
            },
            Affinity::After,
        );
        assert_eq!(
            response.results()[0].chosen,
            Position::Source(SourceBoundary::new(start + 3))
        );
        let mut op = model.budget().begin(CancellationToken::new());
        assert!(matches!(
            model.map_point(
                &PointQuery {
                    model: model.identity(),
                    input: PointInput::Source {
                        target: TextTarget::Payload(id),
                        byte: SourceBoundary::new(source.find("second.png").unwrap() as u64)
                    },
                    affinity: Affinity::After,
                },
                &mut op
            ),
            Err(CoreError::OutsideScope)
        ));
    }
}

#[test]
fn image_src_scope_rejects_tag_attributes_and_overlapping_ranges() {
    for raw in ["assets/中😀.png", "  assets/中😀.png  "] {
        let source = format!("<p><img alt='caption' src='{raw}' width=128></p>");
        let model = parse(&source);
        let id = destinations(&model)[0];
        let start = source.find(raw).unwrap() as u64;
        let end = start + raw.len() as u64;
        for byte in [
            0,
            start - 1,
            end + 1,
            source.find("caption").unwrap() as u64,
            source.find("128").unwrap() as u64,
        ] {
            let mut op = model.budget().begin(CancellationToken::new());
            assert!(
                matches!(
                    model.map_point(
                        &PointQuery {
                            model: model.identity(),
                            input: PointInput::Source {
                                target: TextTarget::Payload(id),
                                byte: SourceBoundary::new(byte)
                            },
                            affinity: Affinity::After,
                        },
                        &mut op
                    ),
                    Err(CoreError::OutsideScope)
                ),
                "{source} at {byte}"
            );
        }
        for span in [
            SourceSpan::new(start - 1, end).unwrap(),
            SourceSpan::new(start, end + 1).unwrap(),
        ] {
            let mut op = model.budget().begin(CancellationToken::new());
            assert!(
                matches!(
                    model.map_range(
                        &RangeQuery {
                            model: model.identity(),
                            input: RangeInput::Source {
                                target: TextTarget::Payload(id),
                                span
                            },
                            empty_affinity: Affinity::After,
                        },
                        &mut op
                    ),
                    Err(CoreError::OutsideScope)
                ),
                "{source}"
            );
        }
        let invalid_source = source.find('😀').unwrap() as u64 + 1;
        let value = model
            .ir()
            .text(model.ir().payloads()[id.index()].value)
            .unwrap();
        let invalid_payload = value.find('😀').unwrap() as u64 + 1;
        for input in [
            PointInput::Source {
                target: TextTarget::Payload(id),
                byte: SourceBoundary::new(invalid_source),
            },
            PointInput::Payload {
                payload: id,
                utf8: PayloadBoundary::new(invalid_payload),
            },
        ] {
            let mut op = model.budget().begin(CancellationToken::new());
            assert!(
                matches!(
                    model.map_point(
                        &PointQuery {
                            model: model.identity(),
                            input,
                            affinity: Affinity::After
                        },
                        &mut op
                    ),
                    Err(CoreError::InvalidBoundary)
                ),
                "{source}"
            );
        }
        for input in [
            RangeInput::Source {
                target: TextTarget::Payload(id),
                span: SourceSpan::new(start, invalid_source).unwrap(),
            },
            RangeInput::Payload {
                payload: id,
                span: PayloadSpan::new(0, invalid_payload).unwrap(),
            },
        ] {
            let mut op = model.budget().begin(CancellationToken::new());
            assert!(
                matches!(
                    model.map_range(
                        &RangeQuery {
                            model: model.identity(),
                            input,
                            empty_affinity: Affinity::After,
                        },
                        &mut op
                    ),
                    Err(CoreError::InvalidBoundary)
                ),
                "{source}"
            );
        }
    }
}

#[test]
fn linked_images_and_mixed_markdown_payloads_keep_distinct_origins() {
    let source = "<p><a href='same.png'><img src='same.png' alt='one'></a> <img src='same.png'> <a href='same.png'><img src='same.png' alt='two'></a></p>\n\n![plain](same.png) and [link](same.png)";
    let model = parse(source);
    let ids = destinations(&model);
    let starts = source
        .match_indices("same.png")
        .map(|(offset, _)| offset as u64)
        .collect::<Vec<_>>();
    assert_eq!(ids.len(), 7);
    assert_eq!(
        model
            .ir()
            .objects()
            .iter()
            .filter(|row| matches!(row, ObjectRow::Image { .. }))
            .count(),
        4
    );
    assert_eq!(
        model
            .ir()
            .nodes()
            .iter()
            .filter(|row| row.kind == NodeKind::Link)
            .count(),
        3
    );
    // Identify each destination through its image/link owner, independently of
    // the source mapping under test. Equal URL text must not permit two owners
    // to exchange their href/src origins while still passing a set comparison.
    let ir = model.ir();
    let syntax = |node: &crate::ir::NodeRow| {
        &model.source().text()
            [node.source_envelope.start() as usize..node.source_envelope.end() as usize]
    };
    let mut expected = Vec::new();
    for object in ir.objects() {
        let ObjectRow::Image {
            node, destination, ..
        } = *object
        else {
            continue;
        };
        let node = &ir.nodes()[node.index()];
        let occurrence = match syntax(node) {
            "![one](<same.png>)" => 1,
            "![](<same.png>)" => 2,
            "![two](<same.png>)" => 4,
            "![plain](same.png)" => 5,
            other => panic!("Unexpected image owner syntax: {other:?}"),
        };
        expected.push((destination, starts[occurrence]));
        let parent = &ir.nodes()[node.parent.unwrap().index()];
        if matches!(occurrence, 1 | 4) {
            assert_eq!(parent.kind, NodeKind::Link);
            let DetailRow::Link { destination, .. } = ir.details()[parent.detail.unwrap().index()]
            else {
                panic!("Linked image parent must own a link destination");
            };
            expected.push((destination, starts[occurrence - 1]));
        } else {
            assert_ne!(parent.kind, NodeKind::Link);
        }
    }
    let standalone = ir
        .nodes()
        .iter()
        .find(|node| node.kind == NodeKind::Link && syntax(node) == "[link](same.png)")
        .unwrap();
    let DetailRow::Link { destination, .. } = ir.details()[standalone.detail.unwrap().index()]
    else {
        panic!("Standalone Markdown link must own a destination");
    };
    expected.push((destination, starts[6]));
    let mut expected_ids = expected.iter().map(|&(id, _)| id).collect::<Vec<_>>();
    expected_ids.sort_unstable();
    assert_eq!(
        expected_ids, ids,
        "Every destination has one authored owner"
    );
    let queries = expected
        .iter()
        .map(|&(id, _)| PointQuery {
            model: model.identity(),
            input: PointInput::Payload {
                payload: id,
                utf8: PayloadBoundary::new(3),
            },
            affinity: Affinity::After,
        })
        .collect::<Vec<_>>();
    let mut op = model.budget().begin(CancellationToken::new());
    let response = model.map_points(&queries, &mut op).unwrap();
    for (result, &(_, start)) in response.results().iter().zip(&expected) {
        assert_eq!(
            result.chosen,
            Position::Source(SourceBoundary::new(start + 3))
        );
    }
    for (id, start) in expected {
        let response = point(
            &model,
            PointInput::Source {
                target: TextTarget::Payload(id),
                byte: SourceBoundary::new(start + 3),
            },
            Affinity::After,
        );
        assert_eq!(
            response.results()[0].chosen,
            Position::Payload(PayloadBoundary::new(3))
        );
        for other in starts.iter().copied().filter(|&other| other != start) {
            let mut op = model.budget().begin(CancellationToken::new());
            assert!(matches!(
                model.map_point(
                    &PointQuery {
                        model: model.identity(),
                        input: PointInput::Source {
                            target: TextTarget::Payload(id),
                            byte: SourceBoundary::new(other + 3)
                        },
                        affinity: Affinity::After,
                    },
                    &mut op
                ),
                Err(CoreError::OutsideScope)
            ));
        }
        let mut op = model.budget().begin(CancellationToken::new());
        let response = model
            .map_range(
                &RangeQuery {
                    model: model.identity(),
                    input: RangeInput::Payload {
                        payload: id,
                        span: PayloadSpan::new(0, 8).unwrap(),
                    },
                    empty_affinity: Affinity::After,
                },
                &mut op,
            )
            .unwrap();
        assert_eq!(
            response.selections()[0]
                .related
                .spans
                .slice(response.spans())
                .unwrap(),
            &[RelatedSpan::Source(
                SourceSpan::new(start, start + 8).unwrap()
            )]
        );
    }
}

#[test]
fn rejected_src_values_do_not_create_destinations() {
    for raw in [
        "javascript:alert(1)",
        "mailto:me@example.org",
        "file:///C:/icon.png",
        "//example.org/icon.png",
        "%5c%5cserver/icon.png",
        "C%3a/icon.png",
        "",
    ] {
        let source = format!("<p><img src='{raw}' src='safe.png' alt='blocked'></p>");
        let model = parse(&source);
        assert!(destinations(&model).is_empty(), "{source}");
        assert!(model.ir().objects().is_empty(), "{source}");
    }
}
