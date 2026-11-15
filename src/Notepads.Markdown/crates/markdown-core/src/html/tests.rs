// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE file in the project root for license information.
// ---------------------------------------------------------------------------------------------

use super::*;
use crate::config::{Extensions, ParseConfig, Profile};
use crate::identity::{LogicalBoundary, SourceKey};
use crate::ir::TextTarget;
use crate::mapping::Position;
use crate::policy::{CancellationToken, SourceBudget};
use crate::reader::MainReader;
use crate::source::SourceDocument;

#[test]
fn review_regression_details_collections_exceed_one_event_quantum() {
    for source in [
        "<details>\n\n<summary>Summary</summary>\n\nBody\n\n</details>\n\n".repeat(48),
        "<details><summary>Summary</summary><p>Body</p></details>\n\n".repeat(48),
    ] {
        let model = parse(&source, true);
        assert_eq!(model.disclosure_regions().len(), 48);
    }
}

#[test]
fn review_regression_details_summary_accepts_large_whitespace_gap() {
    let source = format!(
        "<details>\n\n{}\n\n<summary>Summary</summary>\n\nBody\n\n</details>\n",
        " ".repeat(crate::policy::CHECKPOINT_BYTES as usize + 1)
    );
    let model = parse(&source, true);
    assert_eq!(model.disclosure_regions().len(), 1);
    assert_eq!(preview(&model), "Summary\nBody");
}

#[test]
fn inline_html_scripts_preserve_native_text_and_container_structure() {
    use crate::ir::StageKind;
    for (source, expected, endpoints) in [
        ("A <sup>中😀</sup> X<sub>2</sub> B", "A 中😀 X2 B", 4),
        ("A <SUP >**bold** <sub>tail</sub></sup>", "A bold tail", 4),
        (
            "| Head |\n| --- |\n| X<sub>2</sub><sup>3</sup> |",
            "Head\nX23",
            4,
        ),
        ("A <sup>`<sub>x</sub>`</sup> B", "A <sub>x</sub> B", 2),
        ("A <code><sup>x</sup></code> B", "A <sup>x</sup> B", 0),
        ("A <sup></sup> B", "A  B", 2),
        ("![A <sup>中</sup> B](image.png)", "\u{fffc}", 2),
    ] {
        let model = parse_extensions(source, Extensions::HTML_INLINE);
        assert_eq!(preview(&model), expected, "{source:?}");
        assert_eq!(
            model
                .ir()
                .stages()
                .iter()
                .filter(|stage| stage.kind == StageKind::HtmlEmphasisDelimiter)
                .count(),
            endpoints,
            "{source:?}"
        );
    }
}
#[test]
fn exact_points_remain_exact_near_html_and_entity_transformations() {
    use crate::mapping::Correspondence;
    for (source, logical, byte) in [
        ("&amp;&amp;\n\n<p>html</p>", 1, 5),
        ("<p>&amp;&amp;</p>", 1, 8),
        ("<p>x foo&#65;bar y</p>", 3, 6),
    ] {
        let model = parse(source, true);
        for affinity in [Affinity::Before, Affinity::After] {
            let mut op = model.budget().begin(CancellationToken::new());
            let response = model
                .map_point(
                    &PointQuery {
                        model: model.identity(),
                        input: PointInput::Flow {
                            flow: model.main_flow_id(),
                            utf16: LogicalBoundary::new(logical),
                        },
                        affinity,
                    },
                    &mut op,
                )
                .unwrap();
            let point = response.results()[0];
            assert_eq!(
                point.chosen,
                Position::Source(SourceBoundary::new(byte)),
                "{source}"
            );
            assert_eq!(point.correspondence, Correspondence::Exact, "{source}");
            assert!(!point.effects.atomic, "{source}");
        }
    }
}

#[test]
fn decoded_href_provenance_has_consistent_atomic_relations() {
    use crate::identity::{PayloadBoundary, PayloadSpan};
    use crate::ir::PayloadRole;
    use crate::mapping::{Correspondence, RelatedSpan};
    let source = "<p><a href='  &#104;ttps://example.org/Ω?q=&#65;&amp;x=2  '>label</a></p>";
    let model = parse(source, true);
    let (index, payload) = model
        .ir()
        .payloads()
        .iter()
        .enumerate()
        .find(|(_, row)| row.role == PayloadRole::Destination)
        .unwrap();
    let id = model.payload_id(index as u32 + 1).unwrap();
    let start = source.find('\'').unwrap() as u64 + 1;
    let end = source.rfind('\'').unwrap() as u64;
    let raw = SourceSpan::new(start, end).unwrap();
    assert_eq!(
        model.ir().text(payload.value).unwrap(),
        "https://example.org/Ω?q=A&x=2"
    );
    for (affinity, expected) in [(Affinity::Before, start), (Affinity::After, end)] {
        let mut op = model.budget().begin(CancellationToken::new());
        let response = model
            .map_point(
                &PointQuery {
                    model: model.identity(),
                    input: PointInput::Payload {
                        payload: id,
                        utf8: PayloadBoundary::new(3),
                    },
                    affinity,
                },
                &mut op,
            )
            .unwrap();
        let point = response.results()[0];
        assert_eq!(
            point.chosen,
            Position::Source(SourceBoundary::new(expected))
        );
        assert_eq!(point.correspondence, Correspondence::Atomic);
        assert_eq!(
            point.related.spans.slice(response.spans()).unwrap(),
            &[RelatedSpan::Source(raw)]
        );
        assert_eq!(point.related.envelope, Some(RelatedSpan::Source(raw)));
        for anchor in response.anchors() {
            assert!((start..=end).contains(&anchor.value()));
        }
        for span in response.spans() {
            assert!(span.start() >= start && span.end() <= end);
        }
    }
    let mut op = model.budget().begin(CancellationToken::new());
    let response = model
        .map_range(
            &RangeQuery {
                model: model.identity(),
                input: RangeInput::Payload {
                    payload: id,
                    span: PayloadSpan::new(0, payload.utf8_len).unwrap(),
                },
                empty_affinity: Affinity::After,
            },
            &mut op,
        )
        .unwrap();
    let selection = response.selections()[0];
    assert!(selection.effects.atomic);
    assert!(!selection.expanded);
    assert!(response.contributions().iter().all(|row| !row.expanded));
    assert_eq!(
        selection.related.spans.slice(response.spans()).unwrap(),
        &[RelatedSpan::Source(raw)]
    );
    for (span, expanded) in [
        (raw, false),
        (SourceSpan::new(start + 4, start + 5).unwrap(), true),
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
        assert!(selection.effects.atomic);
        assert_eq!(selection.expanded, expanded);
        assert!(!response.contributions().is_empty());
        for row in response.contributions() {
            assert_eq!(row.source, span);
            assert_eq!(row.expanded, expanded);
        }
    }
}

#[test]
fn mixed_html_and_markdown_payload_batches_keep_their_own_origins() {
    use crate::identity::{PayloadBoundary, PayloadSpan};
    use crate::ir::PayloadRole;
    use crate::mapping::RelatedSpan;
    let source = "<p><a href='https://example.org'>One</a> and <a href='https://example.org'>Two</a></p>\n\n[plain](https://example.org)";
    let model = parse(source, true);
    let payloads = model
        .ir()
        .payloads()
        .iter()
        .enumerate()
        .filter(|(_, row)| row.role == PayloadRole::Destination)
        .map(|(index, row)| (model.payload_id(index as u32 + 1).unwrap(), row.utf8_len))
        .collect::<Vec<_>>();
    let urls = source
        .match_indices("https://example.org")
        .map(|(offset, _)| offset as u64)
        .collect::<Vec<_>>();
    assert_eq!(payloads.len(), 3);
    let one = source.find("One").unwrap() as u64;
    let mut points = Vec::new();
    for (index, (id, _)) in payloads.iter().enumerate() {
        points.push(PointQuery {
            model: model.identity(),
            input: PointInput::Payload {
                payload: *id,
                utf8: PayloadBoundary::new(3),
            },
            affinity: Affinity::After,
        });
        points.push(PointQuery {
            model: model.identity(),
            input: PointInput::Flow {
                flow: model.main_flow_id(),
                utf16: LogicalBoundary::new(1),
            },
            affinity: Affinity::After,
        });
        let mut op = model.budget().begin(CancellationToken::new());
        for forbidden in [
            one,
            source.find("</a>").unwrap() as u64,
            urls[(index + 1) % 3],
        ] {
            let result = model.map_point(
                &PointQuery {
                    model: model.identity(),
                    input: PointInput::Source {
                        target: TextTarget::Payload(*id),
                        byte: SourceBoundary::new(forbidden),
                    },
                    affinity: Affinity::After,
                },
                &mut op,
            );
            assert!(matches!(result, Err(CoreError::OutsideScope)));
            op = model.budget().begin(CancellationToken::new());
        }
    }
    let mut op = model.budget().begin(CancellationToken::new());
    let response = model.map_points(&points, &mut op).unwrap();
    for (index, pair) in response.results().chunks_exact(2).enumerate() {
        assert_eq!(
            pair[0].chosen,
            Position::Source(SourceBoundary::new(urls[index] + 3))
        );
        assert_eq!(
            pair[1].chosen,
            Position::Source(SourceBoundary::new(one + 1))
        );
    }
    let ranges = payloads
        .iter()
        .map(|(id, len)| RangeQuery {
            model: model.identity(),
            input: RangeInput::Payload {
                payload: *id,
                span: PayloadSpan::new(0, *len).unwrap(),
            },
            empty_affinity: Affinity::After,
        })
        .collect::<Vec<_>>();
    let mut op = model.budget().begin(CancellationToken::new());
    let response = model.map_ranges(&ranges, &mut op).unwrap();
    for (index, selection) in response.selections().iter().enumerate() {
        assert_eq!(
            selection.related.spans.slice(response.spans()).unwrap(),
            &[RelatedSpan::Source(
                SourceSpan::new(urls[index], urls[index] + 19).unwrap()
            )]
        );
    }
}

#[test]
fn empty_html_preserves_indented_code_boundaries_and_code_text() {
    for separator in [
        "<!-- divider -->",
        "<p></p>",
        "<!-- first -->\n\n<!-- second -->",
    ] {
        let source = format!("    first\n\n{separator}\n\n    second");
        let model = parse(&source, true);
        let code = model
            .ir()
            .nodes()
            .iter()
            .filter(|row| row.kind == NodeKind::CodeBlock)
            .collect::<Vec<_>>();
        assert_eq!(code.len(), 2, "{source:?}");
        let text = preview(&model);
        for (row, expected) in code.into_iter().zip(["first\n", "second\n"]) {
            let start = row.logical.start() as usize;
            let end = row.logical.end() as usize;
            assert_eq!(&text[start..end], expected, "{source:?}");
        }
        for source in [
            format!("- item\n\n{separator}\n\n    code"),
            format!("    code\n\n{separator}\n\n- item"),
        ] {
            let model = parse(&source, true);
            let nodes = model.ir().nodes();
            assert_eq!(
                nodes
                    .iter()
                    .filter(|row| row.kind == NodeKind::List)
                    .count(),
                1,
                "{source:?}"
            );
            let code = nodes
                .iter()
                .filter(|row| row.kind == NodeKind::CodeBlock)
                .collect::<Vec<_>>();
            assert_eq!(code.len(), 1, "{source:?}");
            let row = code[0];
            let text = preview(&model);
            assert_eq!(
                &text[row.logical.start() as usize..row.logical.end() as usize],
                "code\n",
                "{source:?}"
            );
        }
    }
}

#[test]
fn html_link_destination_maps_to_its_href_attribute() {
    use crate::identity::{PayloadBoundary, PayloadSpan};
    use crate::ir::PayloadRole;
    for (source, expected) in [
        (
            "<p><a href='https://example.org'>label</a></p>",
            "https://example.org",
        ),
        (
            "<p><a href=\"https://example.org?q=A&amp;b=B\">label</a></p>",
            "https://example.org?q=A&b=B",
        ),
    ] {
        let model = parse(source, true);
        let (index, payload) = model
            .ir()
            .payloads()
            .iter()
            .enumerate()
            .find(|(_, row)| row.role == PayloadRole::Destination)
            .unwrap();
        let id = model.payload_id(index as u32 + 1).unwrap();
        assert_eq!(model.ir().text(payload.value).unwrap(), expected);
        let raw_start = source.find("https://").unwrap() as u64;
        let raw_end = raw_start + source[raw_start as usize..].find(['\'', '"']).unwrap() as u64;
        let mut op = model.budget().begin(CancellationToken::new());
        let response = model
            .map_range(
                &RangeQuery {
                    model: model.identity(),
                    input: RangeInput::Payload {
                        payload: id,
                        span: PayloadSpan::new(0, payload.utf8_len).unwrap(),
                    },
                    empty_affinity: Affinity::After,
                },
                &mut op,
            )
            .unwrap();
        assert_eq!(
            response.spans(),
            &[crate::mapping::RelatedSpan::Source(
                SourceSpan::new(raw_start, raw_end).unwrap()
            )],
            "{source}"
        );
        let response = model
            .map_point(
                &PointQuery {
                    model: model.identity(),
                    input: PointInput::Source {
                        target: TextTarget::Payload(id),
                        byte: SourceBoundary::new(raw_start + 3),
                    },
                    affinity: Affinity::After,
                },
                &mut op,
            )
            .unwrap();
        assert_eq!(
            response.results()[0].chosen,
            Position::Payload(PayloadBoundary::new(3)),
            "{source}"
        );
        assert_eq!(model.original_source().text(), source);
    }
}

#[test]
fn html_entity_ranges_preserve_atomic_summary_without_false_expansion() {
    let model = parse("<p>x foo&#65;bar y</p>", true);
    let mut op = model.budget().begin(CancellationToken::new());
    for (start, end) in [(2, 9), (5, 6)] {
        let response = model
            .map_range(
                &RangeQuery {
                    model: model.identity(),
                    input: RangeInput::Flow {
                        flow: model.main_flow_id(),
                        span: LogicalSpan::new(start, end).unwrap(),
                    },
                    empty_affinity: Affinity::After,
                },
                &mut op,
            )
            .unwrap();
        let selection = response.selections()[0];
        assert!(selection.effects.atomic, "{start}..{end}");
        assert!(!selection.expanded, "{start}..{end}");
        assert!(
            response.contributions().iter().all(|row| !row.expanded),
            "{start}..{end}"
        );
    }
    for (span, expanded) in [
        (SourceSpan::new(8, 13).unwrap(), false),
        (SourceSpan::new(9, 10).unwrap(), true),
    ] {
        let response = model
            .map_range(
                &RangeQuery {
                    model: model.identity(),
                    input: RangeInput::Source {
                        target: TextTarget::Flow(model.main_flow_id()),
                        span,
                    },
                    empty_affinity: Affinity::After,
                },
                &mut op,
            )
            .unwrap();
        let selection = response.selections()[0];
        assert!(selection.effects.atomic);
        assert_eq!(selection.expanded, expanded);
        let contributions = selection
            .contributions
            .slice(response.contributions())
            .unwrap();
        assert_eq!(contributions.len(), 1);
        assert_eq!(contributions[0].source, span);
        assert_eq!(contributions[0].expanded, expanded);
    }
    let span = SourceSpan::new(9, 16).unwrap();
    let response = model
        .map_range(
            &RangeQuery {
                model: model.identity(),
                input: RangeInput::Source {
                    target: TextTarget::Flow(model.main_flow_id()),
                    span,
                },
                empty_affinity: Affinity::After,
            },
            &mut op,
        )
        .unwrap();
    assert!(response.selections()[0].expanded);
    let contributions = response.contributions();
    assert_eq!(contributions.len(), 1);
    assert_eq!(contributions[0].source, span);
    assert!(contributions[0].expanded);
    let response = model
        .map_range(
            &RangeQuery {
                model: model.identity(),
                input: RangeInput::Source {
                    target: TextTarget::Flow(model.main_flow_id()),
                    span: SourceSpan::new(13, 16).unwrap(),
                },
                empty_affinity: Affinity::After,
            },
            &mut op,
        )
        .unwrap();
    assert!(!response.selections()[0].effects.atomic);
    assert!(!response.selections()[0].expanded);
    assert!(response.contributions().iter().all(|row| !row.expanded));
}

#[test]
fn inline_html_break_preserves_markdown_structure_and_is_independently_opt_in() {
    use crate::ir::{NodeKind, PayloadRole, StageKind};

    let source = "A<br>B\n\n<div>C<br>D</div>";
    for (bits, paragraph, block) in [
        (0, "A<br>B", "<div>C<br>D</div>"),
        (4, "A<br>B", "C\nD"),
        (8, "A\nB", "<div>C<br>D</div>"),
        (12, "A\nB", "C\nD"),
    ] {
        let model = parse_extensions(source, Extensions::from_bits(bits).unwrap());
        let text = preview(&model);
        assert!(
            text.contains(paragraph) && text.contains(block),
            "{bits}: {text:?}"
        );
    }

    for (source, expected) in [
        ("A<br><BR/>B<br />", "A\n\nB\n"),
        ("# A<br>B", "A\nB"),
        ("> A<br>B", "A\nB"),
        ("- A<br>B", "A\nB"),
        ("| Col |\n| --- |\n| A<br>B |", "Col\nA\nB"),
    ] {
        let model = parse_extensions(source, Extensions::HTML_INLINE);
        let text = preview(&model);
        assert_eq!(text, expected, "{source:?}");
        assert!(
            model
                .ir()
                .nodes()
                .iter()
                .any(|node| node.kind == NodeKind::HardBreak)
        );
    }

    let model = parse_extensions("[A<br>B](#target)", Extensions::HTML_INLINE);
    assert_eq!(preview(&model), "A\nB");
    let ir = model.ir();
    assert_eq!(
        ir.nodes()
            .iter()
            .filter(|node| node.kind == NodeKind::Link)
            .count(),
        1
    );
    assert_eq!(
        ir.stages()
            .iter()
            .filter(|stage| stage.kind == StageKind::HtmlBreakToLineFeed)
            .count(),
        1
    );
    let destination = ir
        .payloads()
        .iter()
        .find(|row| row.role == PayloadRole::Destination)
        .unwrap();
    assert_eq!(ir.text(destination.value).unwrap(), "#target");

    let image = parse_extensions("![A<br>B](image.png)", Extensions::HTML_INLINE);
    let ir = image.ir();
    assert_eq!(preview(&image), "\u{fffc}");
    let alt = ir
        .nodes()
        .iter()
        .find(|node| node.kind == NodeKind::HardBreak)
        .unwrap();
    assert_ne!(alt.flow, image.main_flow_id());
    assert_eq!(alt.logical.end() - alt.logical.start(), 1);
}

#[test]
fn only_accepted_attribute_free_inline_html_breaks_become_newlines() {
    use crate::ir::StageKind;

    for (source, expected) in [
        (r"A\<br>B", "A<br>B"),
        ("A&lt;br&gt;B", "A<br>B"),
        ("A `<br>` B", "A <br> B"),
        ("A<!-- <br> -->B", "AB"),
        ("A<br onclick='boom()'>B", "A<br onclick='boom()'>B"),
        ("A</br>B", "A</br>B"),
        ("A<bravo>B", "A<bravo>B"),
        ("A<br\n>B", "A<br\nB"),
    ] {
        let model = parse_extensions(source, Extensions::HTML_INLINE);
        assert_eq!(preview(&model), expected, "{source:?}");
        assert!(
            model
                .ir()
                .stages()
                .iter()
                .all(|stage| stage.kind != StageKind::HtmlBreakToLineFeed)
        );
    }
}

#[test]
fn inline_html_break_maps_the_complete_tag_between_unicode_scalars() {
    use crate::identity::LogicalSpan;
    use crate::mapping::Correspondence;

    let source = "中😀<BR />尾";
    let model = parse_extensions(source, Extensions::HTML_INLINE);
    assert_eq!(preview(&model), "中😀\n尾");
    let tag_start = source.find('<').unwrap() as u64;
    let tag_end = source.find('>').unwrap() as u64 + 1;
    let tag = SourceSpan::new(tag_start, tag_end).unwrap();
    let flow = model.main_flow_id();
    let mut op = model.budget().begin(CancellationToken::new());
    for (affinity, logical) in [(Affinity::Before, 3), (Affinity::After, 4)] {
        let query = PointQuery {
            model: model.identity(),
            input: PointInput::Source {
                target: TextTarget::Flow(flow),
                byte: SourceBoundary::new(tag_start + 2),
            },
            affinity,
        };
        let result = model.map_point(&query, &mut op).unwrap().results()[0];
        assert_eq!(result.chosen, Position::Flow(LogicalBoundary::new(logical)));
        assert_eq!(result.correspondence, Correspondence::Atomic);
    }
    for (input, expected) in [
        (
            PointInput::Flow {
                flow,
                utf16: LogicalBoundary::new(4),
            },
            Position::Source(SourceBoundary::new(tag_end)),
        ),
        (
            PointInput::Source {
                target: TextTarget::Flow(flow),
                byte: SourceBoundary::new(tag_end),
            },
            Position::Flow(LogicalBoundary::new(4)),
        ),
    ] {
        let query = PointQuery {
            model: model.identity(),
            input,
            affinity: Affinity::After,
        };
        assert_eq!(
            model.map_point(&query, &mut op).unwrap().results()[0].chosen,
            expected
        );
    }

    let selection = model
        .map_range(
            &RangeQuery {
                model: model.identity(),
                input: RangeInput::Flow {
                    flow,
                    span: LogicalSpan::new(3, 4).unwrap(),
                },
                empty_affinity: Affinity::After,
            },
            &mut op,
        )
        .unwrap();
    let [contribution] = selection.contributions() else {
        panic!("expected one contribution")
    };
    assert_eq!(contribution.source, tag);
    assert!(selection.selections()[0].effects.atomic);
}

#[test]
fn inline_html_comments_hide_only_accepted_comment_nodes() {
    for (source, expected) in [
        ("中<!-- *hide* 😀 -->尾", "中尾"),
        ("A<!-->B<!--->C", "ABC"),
        ("A<!-- never closed", "A<!-- never closed"),
        (r"A\<!-- literal -->B", "A<!-- literal -->B"),
        ("A `<!-- code -->` B", "A <!-- code --> B"),
        ("A <code><!-- code --></code> B", "A <!-- code --> B"),
        (
            "A <script><!-- data --></script> B",
            "A <script></script> B",
        ),
        ("[A<!-- hidden -->B](#x)", "AB"),
        ("> 中<!-- line\r\n> second -->尾", "中尾"),
        ("| H |\n| --- |\n| A<!-- x\\|y -->B |", "H\nAB"),
    ] {
        let model = parse_extensions(source, Extensions::HTML_INLINE);
        assert_eq!(preview(&model), expected, "{source:?}");
        assert_eq!(model.source().text(), source);
    }
    let disabled = parse_extensions("A<!-- hidden -->B", Extensions::NONE);
    assert_eq!(preview(&disabled), "A<!-- hidden -->B");
}

pub(super) fn parse(source: &str, enabled: bool) -> AcceptedModel {
    parse_extensions(
        source,
        if enabled {
            Extensions::ALL
        } else {
            Extensions::NONE
        },
    )
}

#[test]
fn inline_html_emphasis_accepts_pairs_and_keeps_native_structure() {
    use crate::ir::{NodeKind, StageKind};

    for (source, expected, endpoints) in [
        (
            "X <b>B</b> <STRONG >S</strong> <i>I</i> <EM\t>E</em> <del>D</del> <s>S</s> <strike>T</strike>",
            "X B S I E D S T",
            14,
        ),
        (
            "A <strong>**中😀** <em>尾<br>行</em></strong> B",
            "A 中😀 尾\n行 B",
            4,
        ),
        (
            "| Header |\r\n| --- |\r\n| <b>中\\|尾</b> |",
            "Header\n中|尾",
            2,
        ),
        ("A\r\n\r\n<b></b>\r\n\r\nB", "A\nB", 2),
        ("> A <b>中\r\n> 尾</b>", "A 中 尾", 2),
        ("![A <b>中</b> B](image.png)", "\u{fffc}", 2),
    ] {
        let model = parse_extensions(source, Extensions::HTML_INLINE);
        assert_eq!(preview(&model), expected, "{source:?}");
        assert_eq!(
            model
                .ir()
                .stages()
                .iter()
                .filter(|row| row.kind == StageKind::HtmlEmphasisDelimiter)
                .count(),
            endpoints,
            "{source:?}"
        );
    }

    let source = "A <em>**中😀**</em> B\n\n<p><b>HTML block</b></p>";
    let model = parse_extensions(source, Extensions::HTML_INLINE);
    assert_eq!(preview(&model), "A 中😀 B\n<p><b>HTML block</b></p>");
    assert!(
        model
            .ir()
            .nodes()
            .iter()
            .any(|row| row.kind == NodeKind::Strong)
    );
    let both = parse_extensions(
        source,
        Extensions::from_bits(Extensions::HTML_INLINE.bits() | Extensions::HTML_BLOCKS.bits())
            .unwrap(),
    );
    assert_eq!(preview(&both), "A 中😀 B\nHTML block");
    let block_only = parse_extensions(source, Extensions::HTML_BLOCKS);
    assert_eq!(preview(&block_only), "A <em>中😀</em> B\nHTML block");
}

#[test]
fn inline_html_emphasis_recovers_after_literal_unbalanced_or_opaque_tags() {
    use crate::ir::StageKind;

    for (source, expected) in [
        ("A <b><i>X</b></i> <s>Y</s>", "A <b><i>X</b></i> Y"),
        ("A <b>X</strong> <s>Y</s>", "A <b>X</strong> Y"),
        (
            "A <b><b class='x'>X</b> <s>Y</s>",
            "A <b><b class='x'>X</b> Y",
        ),
        ("A <b><b/>X</b> <s>Y</s>", "A <b><b/>X</b> Y"),
        (
            "A <b\r\n data-long='123456789012345678901234567890'>X</b> <s>Y</s>",
            "A <b\n data-long='123456789012345678901234567890'>X</b> Y",
        ),
        (
            "A \\<b>X</b> &lt;i&gt;X&lt;/i&gt; <s>Y</s>",
            "A <b>X</b> <i>X</i> Y",
        ),
        ("A `<b>X</b>` <!-- <i>X</i> --> <s>Y</s>", "A <b>X</b>  Y"),
        ("A **<b>X**</b> <s>Y</s>", "A <b>X</b> Y"),
        (
            "A <b>X</b> <bold>Q</bold> <strike-more>Z</strike-more>",
            "A X <bold>Q</bold> <strike-more>Z</strike-more>",
        ),
    ] {
        let model = parse_extensions(source, Extensions::HTML_INLINE);
        assert_eq!(preview(&model), expected, "{source:?}");
        assert_eq!(
            model
                .ir()
                .stages()
                .iter()
                .filter(|row| row.kind == StageKind::HtmlEmphasisDelimiter)
                .count(),
            2,
            "{source:?}"
        );
    }
}

#[test]
fn gfm_tagfilter_inline_keeps_delimiters_with_normal_content_and_preserves_code() {
    let source = "A <script><b>Bold</b><!-- hidden --><code>https://inside.test a@b.test</code>\
                  </script> <textarea>:smile:</textarea> https://outside.test";
    let extensions =
        Extensions::from_bits(Extensions::HTML_INLINE.bits() | Extensions::EMOJI_SHORTCODES.bits())
            .unwrap();
    let model = parse_extensions(source, extensions);
    assert_eq!(
        preview(&model),
        "A <script>Boldhttps://inside.test a@b.test</script> <textarea>😄</textarea> https://outside.test"
    );
    let nodes = model.ir().nodes();
    assert_eq!(
        nodes
            .iter()
            .filter(|node| node.kind == NodeKind::InlineCode)
            .count(),
        1
    );
    assert_eq!(
        nodes
            .iter()
            .filter(|node| node.kind == NodeKind::Link)
            .count(),
        1
    );
    assert_eq!(model.original_source().text(), source);

    let c0 = parse_profile_extensions(Profile::C0, source, extensions);
    assert!(preview(&c0).contains("<script><b>Bold</b><!-- hidden --><code>"));
    assert_eq!(c0.original_source().text(), source);

    let code = r#"A <code><script>let x = "</code>";</script><b>keep</b></code>"#;
    assert_eq!(
        preview(&parse_extensions(code, extensions)),
        r#"A <script>let x = "</code>";</script><b>keep</b>"#
    );
}

#[test]
fn commonmark_inline_html_keeps_raw_text_and_code_like_containers_opaque() {
    for (source, expected) in [
        (
            r#"A <script>let x = "<b>keep</b>";</script> <s>Y</s>"#,
            r#"A <script>let x = "<b>keep</b>";</script> Y"#,
        ),
        (
            "A <textarea>[<b>X</b>](u)</textarea> <s>Y</s>",
            "A <textarea><b>X</b></textarea> Y",
        ),
        ("A <code>**<b>X</b>**</code> <s>Y</s>", "A **<b>X</b>** Y"),
        (
            "A <style><i>X</i></style> <s>Y</s>",
            "A <style><i>X</i></style> Y",
        ),
        (
            "A **<script>** <b>X</b> </script> <s>Y</s>",
            "A <script> <b>X</b> </script> Y",
        ),
        ("A <script>**</script>** <b>X</b>", "A <script></script> X"),
        (
            "A <b>before <code>literal</code> after</b> <code/> <b>Z</b>",
            "A before literal after <code/> Z",
        ),
        (
            "A <code><code>x</code><b>keep</b></code> <s>Y</s>",
            "A <code>x</code><b>keep</b> Y",
        ),
        (
            "A <kbd><kbd>x</kbd><b>keep</b></kbd> <samp          /> <b>Y</b>",
            "A <kbd>x</kbd><b>keep</b> <samp          /> Y",
        ),
        (
            "A <code data-x='中😀' /> <b>Y</b>",
            "A <code data-x='中😀' /> Y",
        ),
        (
            "A <code data-x=y/><b>keep</b></code> <s>Y</s>",
            "A <code data-x=y/><b>keep</b></code> Y",
        ),
        (
            "A <code data-x=y /> <b>Y</b> <code data-x='y'/> <i>Z</i>",
            "A <code data-x=y /> Y <code data-x='y'/> Z",
        ),
        ("A <code data-x=y\x0b/><b>Y</b>", "A <code data-x=y\x0b/>Y"),
        (
            "A <code\x0bdata-x=y><b>keep</b></code\x0b> <s>Y</s>",
            "A <code\x0bdata-x=y><b>keep</b></code\x0b> Y",
        ),
        (
            r#"A <code><script>let x = "</code>";</script><b>keep</b></code> <s>Y</s>"#,
            r#"A <script>let x = "</code>";</script><b>keep</b> Y"#,
        ),
    ] {
        let model = parse_profile_extensions(Profile::C0, source, Extensions::HTML_INLINE);
        assert_eq!(preview(&model), expected, "{source:?}");
    }
}

#[test]
fn inline_html_emphasis_follows_synthetic_separators_at_the_same_source_byte() {
    for (source, expected) in [
        ("A\n\n<b>B</b>", "A\nB"),
        ("A\n\n<em>B</em>", "A\nB"),
        ("|A|\n|-|\n|<b>B</b>|", "A\nB"),
    ] {
        let model = parse_extensions(source, Extensions::HTML_INLINE);
        assert_eq!(preview(&model), expected, "{source:?}");
    }
    let model = parse_extensions("- [x] <b>B</b>", Extensions::HTML_INLINE);
    assert!(preview(&model).ends_with(" B"));
}

#[test]
fn inline_html_emphasis_preserves_overdepth_groups_and_other_siblings() {
    let opens = "<b>".repeat(257);
    let closes = "</b>".repeat(257);
    let source = format!("A {opens}X{closes} <s>Y</s>");
    let model = parse_extensions(&source, Extensions::HTML_INLINE);
    assert_eq!(preview(&model), format!("A {opens}X{closes} Y"));
    let source = format!("A {opens}X");
    let model = parse_extensions(&source, Extensions::HTML_INLINE);
    assert_eq!(preview(&model), source);

    let source = format!("A {}[<i>X</i>](u){}", "<b>".repeat(256), "</b>".repeat(256));
    let model = parse_extensions(&source, Extensions::HTML_INLINE);
    assert_eq!(preview(&model), "A X");
}

#[test]
fn inline_html_emphasis_maps_delimiters_at_utf16_boundaries() {
    use crate::identity::LogicalSpan;
    use crate::mapping::Correspondence;
    let source = "中😀<B>尾😀</b>后";
    let model = parse_extensions(source, Extensions::HTML_INLINE);
    assert_eq!(preview(&model), "中😀尾😀后");
    let flow = model.main_flow_id();
    let mut op = model.budget().begin(CancellationToken::new());
    for (tag, at) in [("<B>", 3), ("</b>", 6)] {
        let start = source.find(tag).unwrap() as u64;
        for affinity in [Affinity::Before, Affinity::After] {
            let query = PointQuery {
                model: model.identity(),
                input: PointInput::Source {
                    target: TextTarget::Flow(flow),
                    byte: SourceBoundary::new(start + 1),
                },
                affinity,
            };
            let result = model.map_point(&query, &mut op).unwrap().results()[0];
            assert_eq!(result.chosen, Position::Flow(LogicalBoundary::new(at)));
            assert_eq!(result.correspondence, Correspondence::Hidden);
        }
    }
    let body_start = source.find('尾').unwrap() as u64;
    let body_end = source.find("</b>").unwrap() as u64;
    let reverse = model
        .map_point(
            &PointQuery {
                model: model.identity(),
                input: PointInput::Flow {
                    flow,
                    utf16: LogicalBoundary::new(4),
                },
                affinity: Affinity::After,
            },
            &mut op,
        )
        .unwrap();
    assert_eq!(
        reverse.results()[0].chosen,
        Position::Source(SourceBoundary::new(body_start + '尾'.len_utf8() as u64)),
    );
    assert_eq!(reverse.results()[0].correspondence, Correspondence::Exact);
    let range = model
        .map_range(
            &RangeQuery {
                model: model.identity(),
                input: RangeInput::Flow {
                    flow,
                    span: LogicalSpan::new(3, 6).unwrap(),
                },
                empty_affinity: Affinity::After,
            },
            &mut op,
        )
        .unwrap();
    let contributions = range.contributions();
    assert_eq!(contributions.first().unwrap().source.start(), body_start);
    assert_eq!(contributions.last().unwrap().source.end(), body_end);
    assert!(
        contributions
            .iter()
            .all(|part| { part.source.start() >= body_start && part.source.end() <= body_end })
    );
}

#[test]
fn inline_html_code_owns_its_body_before_markdown_parses_it() {
    use crate::ir::NodeKind;
    let source = "A <CODE > *中😀* [link](https://example.test) ![x](img) $x$ &amp; </code\t> B";
    let extensions =
        Extensions::from_bits(Extensions::HTML_INLINE.bits() | Extensions::MATH.bits()).unwrap();
    let model = parse_extensions(source, extensions);
    assert_eq!(
        preview(&model),
        "A  *中😀* [link](https://example.test) ![x](img) $x$ &amp;  B"
    );
    assert_eq!(
        model
            .ir()
            .nodes()
            .iter()
            .filter(|row| row.kind == NodeKind::InlineCode)
            .count(),
        1,
    );
    assert!(model.ir().objects().is_empty());
    assert!(
        !model
            .ir()
            .nodes()
            .iter()
            .any(|row| matches!(row.kind, NodeKind::Link | NodeKind::Emphasis))
    );
}

#[test]
fn inline_html_code_boundaries_compose_with_markdown_and_containers() {
    use crate::ir::{NodeKind, StageKind};

    for (source, expected, pair_count) in [
        (
            "A <code>one</code> <kbd>Ctrl+P</kbd> <samp>out</samp>",
            "A one Ctrl+P out",
            3,
        ),
        ("*before <code>*</code> after*", "before * after", 1),
        ("A <code>[x</code>](u)", "A [x](u)", 1),
        ("A <code>`x</code>`", "A `x`", 1),
        ("[A <code>]($x$)</code> B](#target)", "A ]($x$) B", 1),
        ("A\n\n<code>B</code>", "A\nB", 1),
        ("A\n\n<code></code>\n\nB", "A\nB", 1),
        ("- [x] <kbd>B</kbd>", "\u{fffc} B", 1),
        ("|A|\n|-|\n|<code>a\\|b</code>|", "A\na|b", 1),
        ("> A <code>a\r\n> b</code>", "A a\nb", 1),
        ("A <code>a\r\nb\0c</code>", "A a\nb\u{fffd}c", 1),
        ("![<script>](img) <code>x</code>", "\u{fffc} x", 1),
        ("![<code>*alt*</code>](img)", "\u{fffc}", 1),
        (
            "A <script>![</script><code>X</code>](img)</script> <kbd>Y</kbd>",
            "A <script>\u{fffc}</script> Y",
            2,
        ),
        (
            r#"A <code><span title="</code>"> <!-- </code> -->x</code>"#,
            r#"A <span title="</code>"> <!-- </code> -->x"#,
            1,
        ),
    ] {
        let model = parse_extensions(source, Extensions::HTML_INLINE);
        assert_eq!(preview(&model), expected, "{source:?}");
        let stages = model
            .ir()
            .stages()
            .iter()
            .filter(|stage| stage.kind == StageKind::HtmlCodeDelimiter);
        assert_eq!(stages.count(), pair_count * 2, "{source:?}");
    }
    let source = "[A <code>]($x$)</code> B](#target)";
    let model = parse_extensions(source, Extensions::HTML_INLINE);
    assert_eq!(
        model
            .ir()
            .nodes()
            .iter()
            .filter(|row| row.kind == NodeKind::Link)
            .count(),
        1
    );
    let no_inline = parse_extensions("A <code>*x*</code>", Extensions::HTML_BLOCKS);
    assert_eq!(preview(&no_inline), "A <code>x</code>");
}

#[test]
fn inline_html_code_leaves_malformed_or_non_html_tokens_literal() {
    use crate::ir::StageKind;
    for (source, expected, pairs) in [
        (
            "A <code><kbd>x</code></kbd> <samp>Y</samp>",
            "A <code><kbd>x</code></kbd> Y",
            1,
        ),
        (
            "A <code x='y'>[x](u)</code> <kbd>Y</kbd>",
            "A <code x='y'>x</code> Y",
            1,
        ),
        (
            "A <code data-x=y/><kbd>x</kbd></code> <samp>Y</samp>",
            "A <code data-x=y/><kbd>x</kbd></code> Y",
            1,
        ),
        (
            "A <code data-x=y /> <kbd>Y</kbd>",
            "A <code data-x=y /> Y",
            1,
        ),
        ("A <code>X\n\nB</code>", "A <code>X\nB</code>", 0),
        (
            "A \\<code>X</code> &lt;kbd&gt;X&lt;/kbd&gt; `<samp>X</samp>`",
            "A <code>X</code> <kbd>X</kbd> <samp>X</samp>",
            0,
        ),
    ] {
        let model = parse_extensions(source, Extensions::HTML_INLINE);
        assert_eq!(preview(&model), expected, "{source:?}");
        assert_eq!(
            model
                .ir()
                .stages()
                .iter()
                .filter(|row| row.kind == StageKind::HtmlCodeDelimiter)
                .count(),
            2 * pairs
        );
    }
    let source = format!("A {}X", "<code>".repeat(1024));
    assert_eq!(
        preview(&parse_extensions(&source, Extensions::HTML_INLINE)),
        source
    );
}

#[test]
fn inline_html_code_respects_accepted_image_label_boundaries() {
    for (source, expected) in [
        ("![<code>](img) <kbd>X</kbd>", "\u{fffc} X"),
        ("![<code>](img) <code>X</code>", "\u{fffc} X"),
        ("![<code><script>](img) <samp>X</samp>", "\u{fffc} X"),
        ("![<code>][ref] <code>X</code>\n\n[ref]: img", "\u{fffc} X"),
        ("![<code>[x](u)</code>](img) <kbd>Y</kbd>", "\u{fffc} Y"),
        ("![<code>`[`](img) <kbd>Y</kbd>", "\u{fffc} Y"),
        ("![literal <code>x] text</code>", "![literal x] text"),
        (
            "A <script>![</script>] <code>*X*</code>",
            "A <script>![</script>] *X*",
        ),
        (
            "A <script>![</script> <code>*X*</code>",
            "A <script>![</script> *X*",
        ),
        (
            "![![<code>](inner) <kbd>alt</kbd>](outer) <samp>X</samp>",
            "\u{fffc} X",
        ),
    ] {
        assert_eq!(
            preview(&parse_extensions(source, Extensions::HTML_INLINE)),
            expected,
            "{source:?}"
        );
    }
}

#[test]
fn inline_html_code_charges_only_the_literal_body_for_embedded_image_punctuation() {
    use crate::policy::{ModelLimits, SourceLimits};

    for (prefix, expected, unit) in [
        ("A ", "A ", "![x]"),
        ("A ![logo](img) ", "A \u{fffc} ", "![x]"),
        ("A ", "A ", "![x] <kbd>k</kbd> "),
    ] {
        let budget = SourceBudget::new_with_model_limits(
            SourceLimits::default(),
            ModelLimits {
                native_records: 128,
                reference_lookups: 1,
                ..ModelLimits::default()
            },
        )
        .unwrap();
        let mut op = budget.begin(CancellationToken::new());
        let body = unit.repeat(128);
        let text = format!("{prefix}<code>{body}</code>");
        let source = SourceDocument::new(
            SourceKey::new([14; 16], 1).unwrap(),
            text.as_bytes(),
            &mut op,
        )
        .unwrap();
        let model = AcceptedModel::parse(
            source.snapshot(),
            ParseConfig::with_extensions(Profile::C0, Extensions::HTML_INLINE),
            1,
            &mut op,
        )
        .unwrap();
        assert_eq!(preview(&model), format!("{expected}{body}"));
    }
}

#[test]
fn inline_html_code_probe_does_not_admit_speculative_semantic_output() {
    use crate::ir::{NodeKind, StageKind};
    use crate::policy::{ModelLimits, SourceLimits};

    for (body, suffix) in [
        ("![x]".repeat(128), ""),
        ("![x]".repeat(128), "\n\n[unused]: u"),
        (format!("{}] tail", "a".repeat(1024)), ""),
    ] {
        let budget = SourceBudget::new_with_model_limits(
            SourceLimits::default(),
            ModelLimits {
                native_records: 128,
                reference_lookups: 1,
                decoded_bytes: 1400,
                ..ModelLimits::default()
            },
        )
        .unwrap();
        let mut op = budget.begin(CancellationToken::new());
        let text = format!("![<code>{body}</code>](img){suffix}");
        let document = SourceDocument::new(
            SourceKey::new([18; 16], 1).unwrap(),
            text.as_bytes(),
            &mut op,
        )
        .unwrap();
        let model = AcceptedModel::parse(
            document.snapshot(),
            ParseConfig::with_extensions(Profile::C0, Extensions::HTML_INLINE),
            1,
            &mut op,
        )
        .unwrap();
        assert_eq!(preview(&model), "\u{fffc}");
        assert_eq!(
            model
                .ir()
                .nodes()
                .iter()
                .filter(|node| node.kind == NodeKind::Image)
                .count(),
            1
        );
        assert_eq!(
            model
                .ir()
                .stages()
                .iter()
                .filter(|stage| stage.kind == StageKind::HtmlCodeDelimiter)
                .count(),
            2
        );
        assert_eq!(model.ir().flows()[1].logical_utf16, body.len() as u64);
    }
}

#[test]
fn inline_html_code_pair_index_does_not_keep_inert_tags_in_large_literal_bodies() {
    use crate::policy::{ModelLimits, SourceLimits};

    let budget = SourceBudget::new_with_model_limits(
        SourceLimits {
            auxiliary_capacity: 4 * 1024 * 1024,
            ..SourceLimits::default()
        },
        ModelLimits::default(),
    )
    .unwrap();
    let mut op = budget.begin(CancellationToken::new());
    let body = "<b>".repeat(32_768);
    let text = format!("![<code>](img) <kbd>{body}</kbd>");
    let document = SourceDocument::new(
        SourceKey::new([16; 16], 1).unwrap(),
        text.as_bytes(),
        &mut op,
    )
    .unwrap();
    let model = AcceptedModel::parse(
        document.snapshot(),
        ParseConfig::with_extensions(Profile::C0, Extensions::HTML_INLINE),
        1,
        &mut op,
    )
    .unwrap();
    assert_eq!(preview(&model), format!("\u{fffc} {body}"));
}

#[test]
fn inline_html_code_recovers_when_earlier_code_changes_an_image_label_end() {
    let text = r#"![<code>`a</code> <kbd>](new) <samp>Y</samp> ` z](old)"#;
    let model = parse_extensions(text, Extensions::HTML_INLINE);
    assert_eq!(preview(&model), "\u{fffc} Y ` z](old)");
}

#[test]
fn inline_html_image_label_code_respects_the_profile_of_raw_tags() {
    use crate::ir::{NodeKind, StageKind};

    for (text, images) in [
        (
            r#"A <code>`a</code> <script>![</script><kbd>X</kbd>](img) ` </script>"#,
            1,
        ),
        (
            r#"![<code>`a</code> <script>![</script><kbd>X</kbd>](inner) ` tail](outer)"#,
            2,
        ),
        (
            r#"![<code>`a]</code> <script>![</script><kbd>X</kbd>](inner) ` tail](outer)"#,
            2,
        ),
    ] {
        for (profile, pairs) in [(Profile::C0, 1), (Profile::G3, 2)] {
            let model = parse_profile_extensions(profile, text, Extensions::HTML_INLINE);
            assert_eq!(
                model
                    .ir()
                    .nodes()
                    .iter()
                    .filter(|node| node.kind == NodeKind::Image)
                    .count(),
                images,
                "{profile:?}: {text:?}"
            );
            assert_eq!(
                model
                    .ir()
                    .stages()
                    .iter()
                    .filter(|stage| stage.kind == StageKind::HtmlCodeDelimiter)
                    .count(),
                pairs * 2,
                "{profile:?}: {text:?}"
            );
        }
    }
}

#[test]
fn inline_html_code_preserves_nested_images_exposed_by_an_earlier_pair() {
    use crate::ir::NodeKind;

    for text in [
        r#"![<code>`a</code> ![<kbd>](inner) X</kbd> ` tail](outer)"#,
        r#"![<code>`a]</code> ![<kbd>](inner) X</kbd> ` tail](outer)"#,
    ] {
        let model = parse_extensions(text, Extensions::HTML_INLINE);
        assert_eq!(preview(&model), "\u{fffc}", "{text:?}");
        assert_eq!(
            model
                .ir()
                .nodes()
                .iter()
                .filter(|node| node.kind == NodeKind::Image)
                .count(),
            2,
            "{text:?}"
        );
    }
}

#[test]
fn inline_html_code_resolves_crossing_and_inherited_scope_on_the_actual_branch() {
    use crate::ir::{NodeKind, StageKind};

    let crossing = r#"A <script>![</script><kbd>`a</kbd> ] bad ` tail](img)"#;
    let reference = "A <script>![</script><kbd>X</kbd>][ref]\n\n[ref]: image.png";
    for (text, profile, images, pairs) in [
        (crossing, Profile::C0, 1, 0),
        (crossing, Profile::G3, 0, 1),
        (reference, Profile::C0, 1, 0),
        (reference, Profile::G3, 1, 1),
        (
            r#"![<code>`<code>` </code> <kbd>`a</kbd> ` label](img) ` tail </code>"#,
            Profile::G3,
            1,
            0,
        ),
    ] {
        let model = parse_profile_extensions(profile, text, Extensions::HTML_INLINE);
        assert_eq!(
            model
                .ir()
                .nodes()
                .iter()
                .filter(|node| node.kind == NodeKind::Image)
                .count(),
            images,
            "{profile:?}: {text:?}"
        );
        assert_eq!(
            model
                .ir()
                .stages()
                .iter()
                .filter(|stage| stage.kind == StageKind::HtmlCodeDelimiter)
                .count(),
            2 * pairs,
            "{profile:?}: {text:?}"
        );
    }

    let text = r#"![<code>\(a]</code> ![<kbd>](inner) X</kbd> \) tail](outer)"#;
    let model = parse_extensions(
        text,
        Extensions::from_bits(Extensions::HTML_INLINE.bits() | Extensions::MATH.bits()).unwrap(),
    );
    assert_eq!(preview(&model), "\u{fffc}");
    assert_eq!(
        model
            .ir()
            .nodes()
            .iter()
            .filter(|node| node.kind == NodeKind::Image)
            .count(),
        2,
    );
}

#[test]
fn inline_html_code_uses_the_actual_autolink_gap_after_a_selected_link_consumes_an_image_opener() {
    use crate::ir::{NodeKind, StageKind};

    for (text, links, html_code_pairs) in [
        (
            "A <script> https://example.test/![</script><code>x</code>](img)",
            1,
            1,
        ),
        (
            "A https://example.test/![<script>](img) <code>x</code> </script>",
            1,
            1,
        ),
        (
            "A https://example.test/![<code> https://inside.test</code>](img)",
            2,
            0,
        ),
    ] {
        let mut op = SourceBudget::preview().begin(CancellationToken::new());
        let source = SourceDocument::new(
            SourceKey::new([17; 16], 1).unwrap(),
            text.as_bytes(),
            &mut op,
        )
        .unwrap();
        let model = AcceptedModel::parse(
            source.snapshot(),
            ParseConfig::with_extensions(Profile::G3, Extensions::HTML_INLINE),
            1,
            &mut op,
        )
        .unwrap();
        assert!(
            model
                .ir()
                .nodes()
                .iter()
                .all(|node| node.kind != NodeKind::Image),
            "{text:?}"
        );
        assert_eq!(
            model
                .ir()
                .nodes()
                .iter()
                .filter(|node| node.kind == NodeKind::Link)
                .count(),
            links,
            "{text:?}"
        );
        assert_eq!(
            model
                .ir()
                .stages()
                .iter()
                .filter(|stage| stage.kind == StageKind::HtmlCodeDelimiter)
                .count(),
            2 * html_code_pairs,
            "{text:?}"
        );
    }
}

#[test]
fn inline_html_code_pair_index_respects_the_end_of_each_autolink_gap() {
    use crate::ir::{NodeKind, StageKind};

    for (prefix, images, pairs) in [("", 0, 1), ("![<kbd>x</kbd>](img) ", 1, 2)] {
        let text = format!(
            "A {prefix}<script> https://example.test/![</script><code>`<!-- </code>` https://inside.test -->](img)"
        );
        let mut op = SourceBudget::preview().begin(CancellationToken::new());
        let source = SourceDocument::new(
            SourceKey::new([19; 16], 1).unwrap(),
            text.as_bytes(),
            &mut op,
        )
        .unwrap();
        let model = AcceptedModel::parse(
            source.snapshot(),
            ParseConfig::with_extensions(Profile::G3, Extensions::HTML_INLINE),
            1,
            &mut op,
        )
        .unwrap();
        assert_eq!(
            model
                .ir()
                .nodes()
                .iter()
                .filter(|node| node.kind == NodeKind::Link)
                .count(),
            2
        );
        assert_eq!(
            model
                .ir()
                .nodes()
                .iter()
                .filter(|node| node.kind == NodeKind::Image)
                .count(),
            images
        );
        assert_eq!(
            model
                .ir()
                .stages()
                .iter()
                .filter(|stage| stage.kind == StageKind::HtmlCodeDelimiter)
                .count(),
            2 * pairs
        );
    }
}

#[test]
fn inline_html_code_recovers_after_actual_markdown_ends_an_unsuccessful_raw_group() {
    use crate::ir::StageKind;

    for source in [
        r#"<code>\<script></code> <samp>y</samp>"#,
        r#"<code>`<code>`</code> <samp>y</samp>"#,
    ] {
        for (prefix, pairs) in [("A ", 1), ("A ![<kbd>x</kbd>](img) ", 2)] {
            let model = parse_extensions(&format!("{prefix}{source}"), Extensions::HTML_INLINE);
            assert!(preview(&model).ends_with(" y"), "{source:?}");
            assert_eq!(
                model
                    .ir()
                    .stages()
                    .iter()
                    .filter(|stage| stage.kind == StageKind::HtmlCodeDelimiter)
                    .count(),
                2 * pairs
            );
        }
    }
}

#[test]
fn inline_html_code_index_keeps_declarations_at_the_first_byte_of_special_bodies() {
    use crate::ir::NodeKind;

    for source in [
        "<code><!--<!X </code> <samp>y</samp>",
        "<code><?<!X </code> <samp>y</samp>",
        "<code><![CDATA[<!X </code> <samp>y</samp>",
    ] {
        for prefix in ["A ", "A ![<kbd>x</kbd>](img) "] {
            let model = parse_extensions(&format!("{prefix}{source}"), Extensions::HTML_INLINE);
            assert!(
                model.ir().nodes().iter().all(|node| {
                    node.kind != NodeKind::InlineCode
                        || node.source_envelope.start() != prefix.len() as u64
                }),
                "{prefix}{source}"
            );
        }
    }
}

#[test]
fn inline_html_declarations_and_failed_image_code_searches_scale_with_input() {
    fn scans(text: &str, extensions: Extensions) -> u64 {
        let mut op = SourceBudget::model_fixture().begin(CancellationToken::new());
        let source = SourceDocument::new(
            SourceKey::new([13; 16], 1).unwrap(),
            text.as_bytes(),
            &mut op,
        )
        .unwrap();
        let before = op.stats().unwrap().scanned_bytes();
        AcceptedModel::parse(
            source.snapshot(),
            ParseConfig::with_extensions(Profile::C0, extensions),
            1,
            &mut op,
        )
        .unwrap();
        op.stats().unwrap().scanned_bytes() - before
    }

    for (prefix, unit, flags) in [
        ("A ", "<!X ", Extensions::HTML_BLOCKS),
        ("A <code>", "<!X ", Extensions::HTML_INLINE),
        ("A ", "![<code><script>](img) ", Extensions::HTML_INLINE),
    ] {
        let small = scans(&format!("{prefix}{}", unit.repeat(64)), flags);
        let large = scans(&format!("{prefix}{}", unit.repeat(128)), flags);
        assert!(large <= 3 * small, "{unit:?}: {small} to {large}");
    }

    let nested = |count| {
        format!(
            "{}{} <samp>Z</samp>",
            "![<code>`a</code> <kbd>](new) ` ".repeat(count),
            " z](old)".repeat(count),
        )
    };
    let small = scans(&nested(32), Extensions::HTML_INLINE);
    let large = scans(&nested(64), Extensions::HTML_INLINE);
    assert!(large <= 3 * small, "nested images: {small} to {large}");
    for (prefix, unit, suffix) in [
        ("![", r#"<kbd>\``] text</kbd> "#, "](img)"),
        ("![", r#"<kbd>\``] text</kbd> "#, " `](img)"),
        ("![", "<kbd><!-- ] text</kbd> ", "](img)"),
    ] {
        let text = |count| format!("{prefix}{}{suffix}", unit.repeat(count));
        let small = scans(&text(128), Extensions::HTML_INLINE);
        let large = scans(&text(256), Extensions::HTML_INLINE);
        assert!(large <= 3 * small, "{unit:?}: {small} to {large}");
    }
    let math =
        Extensions::from_bits(Extensions::HTML_INLINE.bits() | Extensions::MATH.bits()).unwrap();
    for (unit, suffix) in [
        (r#"<kbd>\( ] text</kbd> "#, "](img)"),
        (r#"<kbd>\( ] text</kbd> "#, r#"\)](img)"#),
        (r#"<kbd>\\$x] text</kbd> "#, "](img)"),
        (r#"<kbd>\\$x] text</kbd> "#, "x$ ](img)"),
    ] {
        let text = |count| format!("![{}{suffix}", unit.repeat(count));
        let small = scans(&text(128), math);
        let large = scans(&text(256), math);
        assert!(
            large <= 3 * small,
            "math lookahead {unit:?}: {small} to {large}"
        );
    }
    let shared_math_tail = |count| {
        format!(
            r"A {}\) ] tail",
            r"<script>![</script><code>\(a</code> ".repeat(count)
        )
    };
    let small = scans(&shared_math_tail(256), math);
    let large = scans(&shared_math_tail(512), math);
    eprintln!("raw inheritance shared math scan: {small} to {large}");
    assert!(
        large * 10 <= 24 * small,
        "raw inheritance with shared math close: {small} to {large}"
    );
    let overlap_cases: [(&str, fn(usize) -> String); 2] = [
        ("overlapping rejected pairs", |count| {
            format!(
                "A {}{}",
                "![<code>](img) ".repeat(count),
                "</code>".repeat(count)
            )
        }),
        ("unaccepted raw-inherited images", |count| {
            format!(
                "A <script>{}</script> {}",
                "![x ".repeat(count),
                "] text ".repeat(count)
            )
        }),
    ];
    for (label, text) in overlap_cases {
        let small = scans(&text(32), Extensions::HTML_INLINE);
        let large = scans(&text(64), Extensions::HTML_INLINE);
        assert!(large <= 3 * small, "{label}: {small} to {large}");
    }
    for (unit, terminator) in [
        ("<!X ", ">"),
        ("<?x ", "?>"),
        ("<!-- x ", "-->"),
        ("<![CDATA[x ", "]]>"),
    ] {
        let text = |count| format!("![<code>{}{terminator}Y</code>](img)", unit.repeat(count));
        let small = scans(&text(64), Extensions::HTML_INLINE);
        let large = scans(&text(128), Extensions::HTML_INLINE);
        assert!(
            large <= 3 * small,
            "{unit:?} with terminator: {small} to {large}"
        );
    }
}

#[test]
fn inline_html_code_reused_pair_index_shields_special_markup() {
    for body in [
        "<!X </kbd>Y",
        "<?before <?x </kbd> ?>Y",
        "<!--before <!--x </kbd> -->Y",
        "<![CDATA[before <![CDATA[x </kbd> ]]>Y",
        "<![cDaTa[x </kbd> ]]>Y",
        r#"<span title="</kbd>">Y"#,
        "<script> </kbd> </script>Y",
    ] {
        let text = format!("![<code>](img) <kbd>{body}</kbd>");
        let model = parse_extensions(&text, Extensions::HTML_INLINE);
        assert_eq!(preview(&model), format!("\u{fffc} {body}"));
    }
}

#[test]
fn inline_html_code_preserves_autolink_lexical_owners() {
    let source = "![<code>`[`](img) A <code>https://example.test [x](u) a@b.test</code> https://example.test";
    let mut op = SourceBudget::preview().begin(CancellationToken::new());
    let source = SourceDocument::new(
        SourceKey::new([11; 16], 1).unwrap(),
        source.as_bytes(),
        &mut op,
    )
    .unwrap();
    let model = AcceptedModel::parse(
        source.snapshot(),
        ParseConfig::with_extensions(Profile::G3, Extensions::HTML_INLINE),
        1,
        &mut op,
    )
    .unwrap();
    assert_eq!(
        preview(&model),
        "\u{fffc} A https://example.test [x](u) a@b.test https://example.test"
    );
    assert_eq!(
        model
            .ir()
            .nodes()
            .iter()
            .filter(|row| row.kind == crate::ir::NodeKind::Link)
            .count(),
        1
    );
}

#[test]
fn inline_html_code_survives_autolink_consuming_a_rejected_image_bracket() {
    let source = "A <script>![</script> https://example.test/] <code>*X*</code>";
    let mut op = SourceBudget::preview().begin(CancellationToken::new());
    let source = SourceDocument::new(
        SourceKey::new([14; 16], 2).unwrap(),
        source.as_bytes(),
        &mut op,
    )
    .unwrap();
    let model = AcceptedModel::parse(
        source.snapshot(),
        ParseConfig::with_extensions(Profile::G3, Extensions::HTML_INLINE),
        1,
        &mut op,
    )
    .unwrap();
    assert_eq!(
        preview(&model),
        "A <script>![</script> https://example.test/] *X*"
    );
}

#[test]
fn inline_html_code_maps_literal_utf16_body_and_hidden_tag_boundaries() {
    use crate::identity::LogicalSpan;
    use crate::mapping::Correspondence;

    let source = "中😀<code>尾😀&amp;</code>后";
    let model = parse_extensions(source, Extensions::HTML_INLINE);
    assert_eq!(preview(&model), "中😀尾😀&amp;后");
    let flow = model.main_flow_id();
    let mut op = model.budget().begin(CancellationToken::new());
    for (tag, at) in [("<code>", 3), ("</code>", 11)] {
        let byte = SourceBoundary::new(source.find(tag).unwrap() as u64 + 2);
        for affinity in [Affinity::Before, Affinity::After] {
            let query = PointQuery {
                model: model.identity(),
                input: PointInput::Source {
                    target: TextTarget::Flow(flow),
                    byte,
                },
                affinity,
            };
            let point = model.map_point(&query, &mut op).unwrap().results()[0];
            assert_eq!(point.chosen, Position::Flow(LogicalBoundary::new(at)));
            assert_eq!(point.correspondence, Correspondence::Hidden);
        }
    }
    let at = source.find("&amp;").unwrap() as u64;
    let point = model
        .map_point(
            &PointQuery {
                model: model.identity(),
                input: PointInput::Flow {
                    flow,
                    utf16: LogicalBoundary::new(7),
                },
                affinity: Affinity::After,
            },
            &mut op,
        )
        .unwrap()
        .results()[0];
    assert_eq!(point.chosen, Position::Source(SourceBoundary::new(at + 1)));
    assert_eq!(point.correspondence, Correspondence::Exact);

    let range = model
        .map_range(
            &RangeQuery {
                model: model.identity(),
                input: RangeInput::Flow {
                    flow,
                    span: LogicalSpan::new(3, 11).unwrap(),
                },
                empty_affinity: Affinity::After,
            },
            &mut op,
        )
        .unwrap();
    let body = SourceSpan::new(
        source.find('尾').unwrap() as u64,
        source.find("</code>").unwrap() as u64,
    )
    .unwrap();
    let contributions = range.contributions();
    assert_eq!(contributions.first().unwrap().source.start(), body.start());
    assert_eq!(contributions.last().unwrap().source.end(), body.end());
    assert!(
        contributions
            .iter()
            .all(|part| part.source.start() >= body.start() && part.source.end() <= body.end())
    );
}

#[test]
fn preview_folds_soft_breaks_and_preserves_authored_punctuation() {
    let body = concat!(
        "\"**Word** ...\", and Ossie's 'quote': A -- B --- C.\n",
        "Second *line*.\u{20}\u{20}\n",
        "Two-space break.\\\n",
        "Backslash break.\n\n",
        "[literal -- label](docs--path.html) and `\"code\" -- ...`",
    );
    let expected = concat!(
        "\"Word ...\", and Ossie's 'quote': A -- B --- C. Second line.\n",
        "Two-space break.\nBackslash break.\n",
        "literal -- label and \"code\" -- ...",
    );
    for ending in ["\n", "\r\n", "\r"] {
        let source = body.replace('\n', ending);
        let model = parse_extensions(&source, Extensions::ALL);
        assert_eq!(preview(&model), expected);
        assert_eq!(model.original_source().text(), source);
    }
}

#[test]
fn colon_directives_render_closed_containers_with_nested_disclosures() {
    use crate::ir::DetailRow;
    use comrak::native::{CalloutDisclosure, CalloutKind};
    let input = concat!(
        ":::: info **Outer** 😀\n",
        "Intro\n\n",
        "::: details Fold me {open}\n",
        "```text\n",
        ":::\n",
        "```\n",
        ":::\n",
        "After inner\n",
        "::::\n",
        "Outside\n\n",
        ":::{warning}\n",
        "Watch out\n",
        ":::\n",
    );
    for end in ["\n", "\r\n", "\r"] {
        let source = input.replace('\n', end);
        let model = parse_extensions(&source, Extensions::CALLOUTS);
        assert_eq!(model.original_source().text(), source);
        let rendered = preview(&model);
        for text in [
            "Outer 😀",
            "Intro",
            "Fold me",
            "After inner",
            "Outside",
            "Watch out",
        ] {
            assert!(rendered.contains(text), "{rendered:?}");
        }
        assert_eq!(rendered.matches(":::").count(), 1, "{rendered:?}");
        assert_eq!(
            model
                .ir()
                .details()
                .iter()
                .filter_map(|detail| match detail {
                    DetailRow::Callout {
                        kind, disclosure, ..
                    } => Some((*kind, *disclosure)),
                    _ => None,
                })
                .collect::<Vec<_>>(),
            vec![
                (CalloutKind::Info, CalloutDisclosure::Static),
                (CalloutKind::Quote, CalloutDisclosure::InitiallyOpen),
                (CalloutKind::Warning, CalloutDisclosure::Static),
            ]
        );
        assert_eq!(model.disclosure_regions().len(), 1);
    }
}

#[test]
fn colon_directives_skip_unreferenced_footnotes_without_other_projection_features() {
    let fixture = "Visible.{reference}\n\n[^n]: First\n:::note\nHidden\n:::\n\n\
        :::tip\nOutside\n:::";
    let notes_and_callouts =
        Extensions::from_bits(Extensions::FOOTNOTES.bits() | Extensions::CALLOUTS.bits()).unwrap();
    for extensions in [notes_and_callouts, Extensions::ALL] {
        for reference in ["", "[^n]"] {
            let source = fixture.replace("{reference}", reference);
            let model = parse_extensions(&source, extensions);
            assert_eq!(
                preview(&model),
                if reference.is_empty() {
                    "Visible.\nTip\nOutside"
                } else {
                    "Visible.1\nNote\nHidden\nTip\nOutside\n1.\nFirst\n↩"
                }
            );
            assert_eq!(model.original_source().text(), source);
        }
    }
}

#[test]
fn colon_directives_close_after_lazy_list_and_quote_lines() {
    for (body, shown) in [("- item 😀", "item 😀"), ("> quoted 😀", "quoted 😀")] {
        let source = format!("::: info\n{body}\n:::\nOutside");
        let model = parse_extensions(&source, Extensions::CALLOUTS);
        let view = preview(&model);
        assert!(
            view.contains(&format!("{shown}\nOutside")),
            "{source:?}: {view:?}"
        );
        let container = model
            .ir()
            .nodes()
            .iter()
            .find(|row| row.kind == NodeKind::BlockQuote)
            .unwrap();
        let end = view.find("Outside").unwrap();
        assert!(container.logical.end() <= view[..end].encode_utf16().count() as u64);
        assert_eq!(model.original_source().text(), source);
        let start = view.find('😀').unwrap();
        let mut op = model.budget().begin(CancellationToken::new());
        let point = model
            .map_point(
                &PointQuery {
                    model: model.identity(),
                    input: PointInput::Flow {
                        flow: model.main_flow_id(),
                        utf16: LogicalBoundary::new(view[..start].encode_utf16().count() as u64),
                    },
                    affinity: Affinity::After,
                },
                &mut op,
            )
            .unwrap();
        assert_eq!(
            point.results()[0].chosen,
            Position::Source(SourceBoundary::new(source.find('😀').unwrap() as u64))
        );
        assert_eq!(
            point.results()[0].correspondence,
            crate::mapping::Correspondence::Exact
        );
    }
}

#[test]
fn colon_directives_keep_unclosed_and_literal_markers_in_place() {
    for source in [
        "::: info\nBody\n",
        "````\n::: info\nBody\n:::\n````",
        "`\n::: info\nBody\n:::\n`",
        "    ::: info\n    Body\n    :::",
        "::: unexpected\nBody\n:::",
        "::: note\n::: tip\nBody\n:::\n:::",
        "<!--\n::: info\nBody\n:::\n-->",
        "::: info\n```\n:::\n",
    ] {
        let model = parse_extensions(source, Extensions::CALLOUTS);
        assert!(
            model
                .ir()
                .details()
                .iter()
                .all(|row| !matches!(row, crate::ir::DetailRow::Callout { .. })),
            "{source}"
        );
        assert_eq!(model.original_source().text(), source);
    }
    let model = parse_extensions(
        "::: details\n\nName\n---\n:::\nOutside",
        Extensions::CALLOUTS,
    );
    let view = preview(&model);
    assert!(view.contains("Details\nName\nOutside"), "{view}");
    assert_eq!(model.disclosure_regions().len(), 1);
}

#[test]
fn yaml_citations_resolve_once_in_all_metadata_display_modes() {
    let metadata = r#"---
references:
  - id: smith
    title: 'Marks [and] | bars'
    author:
      - family: '史密斯'
        given: Ada
    issued:
      date-parts: [[2020]]
  - id: team:spec
    title: 'Team spec'
    author:
      - literal: 'Team'
---
"#;
    let body = r#"[@smith, p. 3; @team:spec] from @smith and [-@smith].

[@smith; @missing] and [@unknown].
`[@smith]` and \[@smith] and [@smith](relative.md).
<a href="url">@smith</a> and user@smith.example.

| Test |
| ---- |
| [@team:spec] |

# End
"#;
    for mode in [
        Extensions::NONE,
        Extensions::FRONT_MATTER_CODE,
        Extensions::FRONT_MATTER_HIDE,
    ] {
        let source = format!("{metadata}{body}").replace('\n', "\r\n");
        let model = parse_extensions(
            &source,
            Extensions::from_bits(
                Extensions::FRONT_MATTER.bits()
                    | Extensions::CITATIONS.bits()
                    | Extensions::HTML_INLINE.bits()
                    | mode.bits(),
            )
            .unwrap(),
        );
        let view = preview(&model);
        assert_eq!(model.original_source().text(), source);
        assert!(
            view.contains("(史密斯 2020, p. 3; Team n.d.) from 史密斯 (2020) and (2020)."),
            "{view}"
        );
        assert!(
            view.contains("[@smith; @missing] and [@unknown]."),
            "{view}"
        );
        assert!(view.contains("[@smith] and [@smith] and @smith."), "{view}");
        assert!(view.contains("user@smith.example."), "{view}");
        assert!(view.contains("Test\n(Team n.d.)"), "{view}");
        assert!(
            view.contains(
                "End\nReferences\nAda 史密斯 (2020). Marks [and] | bars\nTeam (n.d.). Team spec"
            ),
            "{view}"
        );
        assert!(model.html_anchors().is_empty());
    }
}

#[test]
fn yaml_citations_count_only_notes_reachable_from_the_document() {
    let source = "---\nreferences: [{id: work, title: Title, author: [{family: Name}]}]\n---\n\n\
        Visible.{reference}\n\n[^parent]: See [^child].\n\n[^child]: [@work]";
    let extensions =
        Extensions::from_bits(Extensions::ALL.bits() | Extensions::FRONT_MATTER_HIDE.bits())
            .unwrap();
    for reference in ["", "[^parent]"] {
        let source = source.replace("{reference}", reference);
        let model = parse_extensions(&source, extensions);
        let text = preview(&model);
        assert_eq!(
            text.contains("References\nName (n.d.). Title"),
            !reference.is_empty(),
            "{text}"
        );
        if reference.is_empty() {
            assert_eq!(text, "Visible.");
        }
        assert_eq!(model.original_source().text(), source);
    }
}

#[test]
fn yaml_citations_leave_duplicate_and_unsupported_keys_literal() {
    let source = r#"---
references:
  - id: duplicate
    title: Work
    author: [{family: Name}]
  - id: duplicate
    title: Missing author
  - id: valid
    title: Other
    author: [{family: Author}]
---
[@duplicate] [@valid; @duplicate] \@valid

[@missing;
 @valid]

```
[@valid]
```
"#;
    let model = parse_extensions(
        source,
        Extensions::from_bits(Extensions::FRONT_MATTER_HIDE.bits() | Extensions::CITATIONS.bits())
            .unwrap(),
    );
    let view = preview(&model);
    assert!(
        view.contains("[@duplicate] [@valid; @duplicate] @valid"),
        "{view}"
    );
    assert!(view.contains("[@valid]"), "{view}");
    assert!(
        view.contains("[@missing;") && view.contains("@valid]"),
        "{view}"
    );
    assert!(!view.contains("References"), "{view}");
}

#[test]
fn yaml_citations_keep_escape_parity_and_unclosed_groups_local() {
    let source = r#"---
references: [{id: work, title: Title, author: [{family: Name}]}]
---
\[@work] vs \\[@work].

[@unknown; @work

@work follows.
"#;
    let model = parse_extensions(
        source,
        Extensions::from_bits(Extensions::FRONT_MATTER_HIDE.bits() | Extensions::CITATIONS.bits())
            .unwrap(),
    );
    let view = preview(&model);
    assert!(view.contains(r"[@work] vs \(Name n.d.)."), "{view}");
    assert!(
        view.contains("[@unknown; @work\nName (n.d.) follows."),
        "{view}"
    );
}

#[test]
fn yaml_citations_fall_back_at_open_literal_block_boundaries() {
    let metadata = "---\nreferences: [{id: work, title: Title, author: [{family: Name}]}]\n---\n";
    for (tail, open) in [
        ("```text\ncode", true),
        ("~~~text\ncode\n~~~", false),
        ("<!-- unclosed", true),
        ("<!-- closed -->", false),
        ("<script>\nx", true),
        ("<script>\nx\n</SCRIPT>", false),
        ("<div>ordinary", false),
    ] {
        let source = format!("{metadata}[@work]\n\n{tail}");
        let model = parse_extensions(
            &source,
            Extensions::from_bits(
                Extensions::FRONT_MATTER_HIDE.bits() | Extensions::CITATIONS.bits(),
            )
            .unwrap(),
        );
        let view = preview(&model);
        assert_eq!(model.original_source().text(), source);
        assert_eq!(view.contains("References"), !open, "{tail:?}: {view:?}");
        assert_eq!(view.contains("[@work]"), open, "{tail:?}: {view:?}");
    }
}

#[test]
fn yaml_citations_map_bibliography_as_synthetic_at_original_eof() {
    use crate::mapping::Correspondence;
    let source = "---\r\nreferences: [{id: work, title: Title, author: [{family: Name}]}]\r\n---\r\n[@work] 原文";
    let model = parse_extensions(
        source,
        Extensions::from_bits(Extensions::FRONT_MATTER_HIDE.bits() | Extensions::CITATIONS.bits())
            .unwrap(),
    );
    let output = preview(&model);
    let position = output.find("Name (n.d.). Title").unwrap();
    let utf16 = output[..position + 3].encode_utf16().count() as u64;
    let eof = SourceBoundary::new(source.len() as u64);
    let mut op = model.budget().begin(CancellationToken::new());
    for affinity in [Affinity::Before, Affinity::After] {
        let point = |input, op: &mut _| {
            model
                .map_point(
                    &PointQuery {
                        model: model.identity(),
                        input,
                        affinity,
                    },
                    op,
                )
                .unwrap()
        };
        let result = point(
            PointInput::Flow {
                flow: model.main_flow_id(),
                utf16: LogicalBoundary::new(utf16),
            },
            &mut op,
        );
        let point_result = result.results()[0];
        assert_eq!(point_result.chosen, Position::Source(eof));
        assert_eq!(point_result.correspondence, Correspondence::Synthetic);
        assert!(point_result.effects.synthetic);
        let result = point(
            PointInput::Source {
                target: TextTarget::Flow(model.main_flow_id()),
                byte: eof,
            },
            &mut op,
        );
        let Position::Flow(boundary) = result.results()[0].chosen else {
            panic!("missing flow")
        };
        assert!(boundary.get() < utf16);
    }
    let utf16_at = |byte| output[..byte].encode_utf16().count() as u64;
    let range = |from, to, op: &mut _| {
        model
            .map_range(
                &RangeQuery {
                    model: model.identity(),
                    input: RangeInput::Flow {
                        flow: model.main_flow_id(),
                        span: LogicalSpan::new(utf16_at(from), utf16_at(to)).unwrap(),
                    },
                    empty_affinity: Affinity::Before,
                },
                op,
            )
            .unwrap()
    };
    let start = output.find("原文").unwrap();
    let original = range(start, start + "原文".len(), &mut op);
    assert!(!original.selections()[0].effects.synthetic);
    let punctuation = position + "Name ".len();
    let generated = range(punctuation, punctuation + 1, &mut op);
    assert!(generated.selections()[0].effects.synthetic);
    assert!(!generated.selections()[0].effects.atomic);
    assert!(!generated.selections()[0].expanded);
}

#[test]
fn bibliography_does_not_change_unrelated_hidden_and_atomic_range_effects() {
    let source = "---\nreferences: [{id: work, title: Title, author: [{family: Name}]}]\n---\n\n**bold** &amp; tail.\n\n[@work]";
    let plain = parse_extensions(source, Extensions::FRONT_MATTER_HIDE);
    let cited = parse_extensions(
        source,
        Extensions::from_bits(Extensions::FRONT_MATTER_HIDE.bits() | Extensions::CITATIONS.bits())
            .unwrap(),
    );
    assert!(preview(&cited).contains("References"));
    for (start, length, hidden, atomic, expanded) in [
        (source.find("**bold").unwrap(), 2, true, false, false),
        (source.find("amp;").unwrap(), 2, false, true, true),
        (source.find("references").unwrap(), 3, true, false, false),
    ] {
        let query = |model: &AcceptedModel| {
            let mut op = model.budget().begin(CancellationToken::new());
            model
                .map_range(
                    &RangeQuery {
                        model: model.identity(),
                        input: RangeInput::Source {
                            target: TextTarget::Flow(model.main_flow_id()),
                            span: SourceSpan::new(start as u64, (start + length) as u64).unwrap(),
                        },
                        empty_affinity: Affinity::Before,
                    },
                    &mut op,
                )
                .unwrap()
        };
        let expected = query(&plain);
        let actual = query(&cited);
        let selections = |response: &crate::mapping::RangeResponse| {
            response
                .selections()
                .iter()
                .map(|selection| (selection.effects, selection.expanded))
                .collect::<Vec<_>>()
        };
        let boundaries = |response: &crate::mapping::RangeResponse| {
            response
                .boundaries()
                .iter()
                .map(|point| point.effects)
                .collect::<Vec<_>>()
        };
        assert_eq!(
            selections(&actual),
            selections(&expected),
            "source range {start}..{}",
            start + length
        );
        assert_eq!(boundaries(&actual), boundaries(&expected));
        let (effects, did_expand) = if let Some(selection) = actual.selections().first() {
            (selection.effects, selection.expanded)
        } else {
            (actual.boundaries()[0].effects, false)
        };
        assert_eq!(
            (effects.hidden, effects.atomic, did_expand),
            (hidden, atomic, expanded)
        );
    }
}

#[test]
fn unsupported_disclosure_summary_keeps_other_projections_and_both_markers() {
    for summary in [
        "<summary><object></object></summary>",
        "<summary>A</summary><p>B</p><summary>C</summary>",
    ] {
        let source = format!(
            "---\nsecret: hidden metadata\n---\n\n<p id='kept'>Kept &amp; text.</p>\n\n\
             <details>{summary}\n\nLiteral body.\n\n</details>\n\n\
             <details><summary>Valid summary</summary>\n\nValid body.\n\n</details>\n"
        );
        let model = parse_extensions(
            &source,
            Extensions::from_bits(
                Extensions::HTML_BLOCKS.bits() | Extensions::FRONT_MATTER_HIDE.bits(),
            )
            .unwrap(),
        );
        let output = preview(&model);
        assert!(!output.contains("hidden metadata"), "{output:?}");
        assert!(output.contains("Kept & text."), "{output:?}");
        assert!(!output.contains("<p id='kept'>"), "{output:?}");
        assert!(
            output.contains(&format!("<details>{summary}")),
            "{output:?}"
        );
        assert!(output.contains("</details>"), "{output:?}");
        assert_eq!(model.disclosure_regions().len(), 1, "{output:?}");
        assert_eq!(model.original_source().text(), source);

        let source_byte = source.find("Kept").unwrap() as u64 + 2;
        let expected = output[..output.find("Kept").unwrap()]
            .encode_utf16()
            .count() as u64
            + 2;
        let mut op = model.budget().begin(CancellationToken::new());
        let mapped = model
            .map_point(
                &PointQuery {
                    model: model.identity(),
                    input: PointInput::Source {
                        target: TextTarget::Flow(model.main_flow_id()),
                        byte: SourceBoundary::new(source_byte),
                    },
                    affinity: Affinity::After,
                },
                &mut op,
            )
            .unwrap();
        assert_eq!(
            mapped.results()[0].chosen,
            Position::Flow(LogicalBoundary::new(expected))
        );
    }
}

#[test]
fn disclosure_pair_crossing_projection_quota_keeps_prior_html_and_front_matter() {
    for filler in [
        "<p>Filler</p>\n\n".repeat(MAX_FRAGMENTS),
        format!("<div>{}</div>\n\n", "x".repeat(60_000)).repeat(5),
    ] {
        let source = format!(
            "---\nsecret: hidden metadata\n---\n\n<p>Kept &amp; text.</p>\n\n\
             <details>\n\n<summary>Pending summary</summary>\n\n{filler}</details>\n"
        );
        let model = parse_extensions(
            &source,
            Extensions::from_bits(
                Extensions::HTML_BLOCKS.bits() | Extensions::FRONT_MATTER_HIDE.bits(),
            )
            .unwrap(),
        );
        let output = preview(&model);
        assert!(!output.contains("hidden metadata"));
        assert!(output.contains("Kept & text."));
        assert!(output.contains("<details>"));
        assert!(output.contains("<summary>Pending summary</summary>"));
        assert!(output.contains("</details>"));
        assert!(model.disclosure_regions().is_empty());
        assert_eq!(model.original_source().text(), source);
    }
}

fn parse_extensions(source: &str, extensions: Extensions) -> AcceptedModel {
    parse_profile_extensions(Profile::G3, source, extensions)
}

fn parse_profile_extensions(
    profile: Profile,
    source: &str,
    extensions: Extensions,
) -> AcceptedModel {
    let budget = SourceBudget::preview();
    let mut op = budget.begin(CancellationToken::new());
    let input = SourceDocument::new(
        SourceKey::new([7; 16], 1).unwrap(),
        source.as_bytes(),
        &mut op,
    )
    .unwrap();
    let config = ParseConfig::with_extensions(profile, extensions);
    AcceptedModel::parse(input.snapshot(), config, 1, &mut op)
        .unwrap_or_else(|error| panic!("{source:?}: {error:?}"))
}

pub(super) fn preview(model: &AcceptedModel) -> String {
    let mut reader = MainReader::new(
        model.clone(),
        model.budget().begin(CancellationToken::new()),
    )
    .unwrap();
    let mut output = String::new();
    let mut buffer = [0; 256];
    loop {
        let read = reader.read_into(&mut buffer).unwrap();
        output.push_str(std::str::from_utf8(&buffer[..read.written]).unwrap());
        if read.done {
            return output;
        }
    }
}

#[test]
fn readme_header_uses_native_headings_images_links_and_alignment() {
    let source = r#"<p align="center">
  <img width="128" align="center" src="src/Notepads.App/Assets/appicon_ws.gif">
</p>
<h1 align="center">
  Notepads
</h1>
<p align="CENTER">
  A modern, lightweight text editor with a minimalist design.
</p>
<p align="center">
  <a style="text-decoration:none" href="https://example.org/store">
    <img src="https://example.org/store.svg?style=flat-square&amp;label=Store" alt="Store link" />
  </a>
  <a style="text-decoration:none">
    <img src="https://example.org/platform.svg" alt="Platform">
  </a>
</p>

## What is Notepads and why do I care?

Normal **Markdown**.
"#;
    let model = parse(source, true);
    let text = preview(&model);
    assert!(text.contains("Notepads"), "{text:?}");
    assert!(
        text.contains("A modern, lightweight text editor"),
        "{text:?}"
    );
    assert!(
        text.contains("What is Notepads and why do I care?"),
        "{text:?}"
    );
    assert!(text.contains("Normal Markdown."), "{text:?}");
    assert!(!text.contains("<p"), "{text:?}");
    assert_eq!(text.matches('\u{fffc}').count(), 3, "{text:?}");
    assert_eq!(model.original_source().text(), source);

    let nodes = model.ir().nodes();
    let html_heading = nodes
        .iter()
        .position(|n| n.kind == NodeKind::Heading)
        .unwrap();
    assert_eq!(
        model.html_alignment(NodeId::from_index(html_heading).unwrap()),
        2
    );
    let image = nodes
        .iter()
        .position(|n| n.kind == NodeKind::Image)
        .unwrap();
    assert_eq!(
        model.html_image_size(NodeId::from_index(image).unwrap()),
        (128, 0)
    );
    assert_eq!(nodes.iter().filter(|n| n.kind == NodeKind::Link).count(), 1);
}

#[test]
fn html_is_opt_in_and_code_samples_are_literal() {
    let source = "<p>render me</p>\n\n~~~html\n<p>fenced</p>\n~~~\n\n    <p>indented</p>\n\nInline ~<p>sample</p>~.\n";
    let disabled = preview(&parse(source, false));
    assert!(disabled.contains("<p>render me</p>"), "{disabled:?}");
    let enabled = preview(&parse(source, true));
    assert!(!enabled.contains("<p>render me</p>"), "{enabled:?}");
    assert!(enabled.contains("render me"), "{enabled:?}");
    for expected in ["<p>fenced</p>", "<p>indented</p>", "<p>sample</p>"] {
        assert!(enabled.contains(expected), "{enabled:?}");
    }
}

#[test]
fn gfm_tagfilter_preserves_original_text_and_projects_supported_html_between_literal_tags() {
    let source = "<div>起😀 <ScRiPt>**原样** <b>仍在加粗</b></sCrIpT> <title/> \
                  <iframe src='https://example.invalid/'>显示</iframe> 结尾.</div>";
    let model = parse(source, true);
    let actual = preview(&model);
    assert_eq!(
        actual,
        "起😀 <ScRiPt>**原样** 仍在加粗</sCrIpT> <title/> \
         <iframe src='https://example.invalid/'>显示</iframe> 结尾."
    );
    assert_eq!(model.original_source().text(), source);
    let bold = actual.find("仍在加粗").unwrap();
    let offset = actual[..bold].encode_utf16().count() as u64;
    assert!(
        model
            .html_styles(LogicalSpan::new(offset, offset + 4).unwrap())
            .iter()
            .any(|(range, flags)| range.contains(&offset)
                && flags & crate::presentation::RUN_BOLD != 0)
    );
    assert!(model.ir().objects().is_empty());
    assert!(
        !model
            .ir()
            .nodes()
            .iter()
            .any(|node| node.kind == NodeKind::Link)
    );
    assert_eq!(preview(&parse(source, false)).trim_end(), source);

    let table = "<table><caption><style>名</style></caption>\
                 <tr><td><xmp><b>值</b></xmp></td></tr></table>";
    let model = parse(table, true);
    assert_eq!(preview(&model), "<style>名</style>\n<xmp>值</xmp>");
    assert_eq!(model.original_source().text(), table);
    assert!(
        model
            .ir()
            .nodes()
            .iter()
            .any(|node| node.kind == NodeKind::Table)
    );
}

#[test]
fn html_ruby_fragment_retains_only_base_and_annotation_source_text() {
    let source = "<ruby>漢<rp>(</rp><rt>かん</rt><rp>)</rp>字<rt>じ</rt></ruby>";
    let fragment = super::fragment::lower_inline(source).unwrap();
    assert_eq!(fragment.inline_text, "漢かん字じ");
    assert_eq!(
        fragment.ruby,
        [
            super::ruby::DisplayUnit {
                start: 0,
                base_end: 3,
                end: 9
            },
            super::ruby::DisplayUnit {
                start: 9,
                base_end: 12,
                end: 15
            },
        ]
    );
    let authored: Vec<_> = fragment
        .parts
        .iter()
        .map(|part| &source[part.source.clone()])
        .collect();
    assert_eq!(authored, ["漢", "かん", "字", "じ"]);
    assert!(super::fragment::lower_inline("<rt>unowned</rt>").is_none());
    assert!(super::fragment::lower_inline("<code><ruby>A<rt>a</rt></ruby></code>").is_none());
}

#[test]
fn html_ruby_projection_binds_utf16_units_through_paragraph_and_table() {
    let source = "<p>前😀 <ruby>漢<rp>(</rp><rt>かん</rt><rp>)</rp></ruby> 後</p>\n\n\
                  <table><tr><td><ruby>茶<rt>ちゃ</rt></ruby></td></tr></table>";
    let model = parse(source, true);
    assert_eq!(preview(&model), "前😀 漢かん 後\n茶ちゃ");
    assert_eq!(model.original_source().text(), source);
    assert_eq!(
        model.html_ruby(LogicalSpan::new(0, 13).unwrap()),
        [
            super::RubyDisplayUnit {
                start: 4,
                base_end: 5,
                end: 7
            },
            super::RubyDisplayUnit {
                start: 10,
                base_end: 11,
                end: 13
            },
        ]
    );
}

#[test]
fn html_anchors_are_empty_when_query_admission_is_disabled() {
    use crate::policy::{ModelLimits, SourceLimits};
    let budget = SourceBudget::new_with_model_limits(
        SourceLimits::default(),
        ModelLimits {
            query_requests: 0,
            ..ModelLimits::default()
        },
    )
    .unwrap();
    let mut op = budget.begin(CancellationToken::new());
    let source = SourceDocument::new(
        SourceKey::new([1; 16], 1).unwrap(),
        b"<p id='target'>body</p>",
        &mut op,
    )
    .unwrap();
    let model = AcceptedModel::parse(
        source.snapshot(),
        ParseConfig::with_extensions(Profile::G3, Extensions::HTML_BLOCKS),
        1,
        &mut op,
    )
    .unwrap();
    assert_eq!(preview(&model), "body");
    assert!(model.html_anchors().is_empty());
}

#[test]
fn html_ruby_tracks_reordered_footnotes_and_skips_unused_definitions() {
    let model = parse_extensions(
        "[^n]: <ruby>字<rt>じ</rt></ruby>\n\n\
         [^unused]: <ruby>茶<rt>ちゃ</rt></ruby>\n\n\
         Body <ruby>漢<rt>かん</rt></ruby> [^n]",
        Extensions::ALL,
    );
    let text = preview(&model);
    let units = model.html_ruby(LogicalSpan::new(0, text.encode_utf16().count() as u64).unwrap());
    let utf16: Vec<_> = text.encode_utf16().collect();
    assert_eq!(units.len(), 2);
    assert_eq!(
        String::from_utf16(&utf16[units[0].start as usize..units[0].end as usize]).unwrap(),
        "漢かん"
    );
    assert_eq!(
        String::from_utf16(&utf16[units[1].start as usize..units[1].end as usize]).unwrap(),
        "字じ"
    );
    assert!(!text.contains('茶'));
}

#[test]
fn html_ruby_at_inline_note_boundary_binds_to_reordered_body() {
    for source in [
        "A sentence.^[<ruby>漢<rt>かん</rt></ruby>]",
        "Before^[<ruby>a<rt>b</rt></ruby>] after.",
    ] {
        let model = parse_extensions(source, Extensions::ALL);
        let text: Vec<_> = preview(&model).encode_utf16().collect();
        let units = model.html_ruby(LogicalSpan::new(0, text.len() as u64).unwrap());
        assert_eq!(units.len(), 1, "{source}");
        let unit = units[0];
        let (base, annotation) = if source.contains('漢') {
            ("漢", "かん")
        } else {
            ("a", "b")
        };
        assert_eq!(
            String::from_utf16(&text[unit.start as usize..unit.base_end as usize]).unwrap(),
            base
        );
        assert_eq!(
            String::from_utf16(&text[unit.base_end as usize..unit.end as usize]).unwrap(),
            annotation
        );
        assert_eq!(model.original_source().text(), source);
    }
}

#[test]
fn html_ruby_does_not_form_an_emoji_shortcode_with_adjacent_markdown() {
    for source in [
        ":<ruby>a<rt>b</rt></ruby>:",
        ":<ruby>&#97;<rt>b</rt></ruby>:",
    ] {
        let model = parse_extensions(source, Extensions::ALL);
        assert_eq!(preview(&model), ":ab:");
        assert_eq!(
            model.html_ruby(LogicalSpan::new(0, 4).unwrap()),
            &[super::ruby::DisplayUnit {
                start: 1,
                base_end: 2,
                end: 3
            }],
        );
        assert_eq!(model.original_source().text(), source);
    }
}

#[test]
fn hidden_footnote_picture_id_does_not_rename_visible_heading_anchor() {
    let picture = r#"<picture><source media="(prefers-color-scheme: dark)" srcset="d.png"><img id="foo" src="l.png" alt="x"></picture>"#;
    for reference in ["", " [^image]"] {
        let source = format!("# Foo\n\n[jump](#foo){reference}\n\n[^image]: {picture}");
        let model = parse_extensions(&source, Extensions::ALL);
        assert_eq!(
            model.html_anchors().len(),
            usize::from(!reference.is_empty())
        );
        let mut op = model.budget().begin(CancellationToken::new());
        let outline = crate::outline::Outline::build(model, &mut op).unwrap();
        let anchor = outline.headings()[0].anchor;
        assert_eq!(
            &outline.blob()[anchor.start as usize..(anchor.start + anchor.len) as usize],
            if reference.is_empty() {
                &b"foo"[..]
            } else {
                &b"foo-1"[..]
            },
        );
    }
}

#[test]
fn html_ruby_resumes_after_a_balanced_opaque_region() {
    let source = r#"Before <code class=x><code class=y>nested</code> <ruby>L<rt>literal</rt></ruby></code> <ruby>字<rt>じ</rt></ruby>"#;
    let model = parse_extensions(source, Extensions::HTML_INLINE);
    let shown = preview(&model);
    assert!(shown.contains("<ruby>L<rt>literal</rt></ruby>"), "{shown}");
    assert!(shown.ends_with("字じ"), "{shown}");
    assert_eq!(
        model
            .html_ruby(LogicalSpan::new(0, shown.encode_utf16().count() as u64).unwrap())
            .len(),
        1
    );
    assert_eq!(model.original_source().text(), source);
}

#[test]
fn html_ruby_inline_uses_only_admitted_groups_without_moving_parent_markdown() {
    let source = "# A <ruby>漢<rt>かん</rt></ruby>\n\n\
                  > Quote <RUBY>茶<rt>ちゃ</rt></RUBY>\n\n\
                  | Column |\n| --- |\n| Before <ruby>字<rt>じ</rt></ruby> after |\n\n\
                  [<ruby>青<rt>あお</rt></ruby>](#target)\n\n\
                  Literal <code><ruby>code<rt>reading</rt></ruby></code>";
    let model = parse_extensions(source, Extensions::HTML_INLINE);
    let main = preview(&model);
    assert_eq!(
        main,
        "A 漢かん\nQuote 茶ちゃ\nColumn\nBefore 字じ after\n青あお\nLiteral <ruby>code<rt>reading</rt></ruby>"
    );
    let span = LogicalSpan::new(0, main.encode_utf16().count() as u64).unwrap();
    assert_eq!(model.html_ruby(span).len(), 4);
    assert_eq!(model.original_source().text(), source);
    for kind in [
        NodeKind::Heading,
        NodeKind::BlockQuote,
        NodeKind::Table,
        NodeKind::Link,
        NodeKind::InlineCode,
    ] {
        assert!(
            model.ir().nodes().iter().any(|node| node.kind == kind),
            "{kind:?}"
        );
    }

    for source in [
        r#"A <code title="opaque"><ruby>x<rt>y</rt></ruby></code>"#,
        "A <ruby>**bold**<rt>annotation</rt></ruby>",
        "A <ruby>base<br><rt>annotation</rt></ruby>",
        "A <ruby>base</ruby>",
    ] {
        let model = parse_extensions(source, Extensions::HTML_INLINE);
        let text = preview(&model);
        assert!(text.contains("<ruby>"), "{text:?}");
        let span = LogicalSpan::new(0, text.encode_utf16().count() as u64).unwrap();
        assert!(model.html_ruby(span).is_empty(), "{text:?}");
    }
}

#[test]
fn gfm_tagfilter_matches_the_filtered_tags_in_the_pinned_example() {
    let source = include_str!("../../../../tests/fixtures/m1/inputs/gfm.0652.bin");
    let html = include_str!("../../../../tests/fixtures/m1/expected/gfm.0652.html");
    let model = parse_extensions(
        source,
        Extensions::from_bits(Extensions::HTML_BLOCKS.bits() | Extensions::HTML_INLINE.bits())
            .unwrap(),
    );
    let actual = preview(&model);
    assert_eq!(model.original_source().text(), source);
    let mut after = actual.as_str();
    for suffix in html.split("&lt;").skip(1) {
        let end = suffix.find('>').unwrap() + 1;
        let tag = format!("<{}", &suffix[..end]);
        after = after
            .split_once(&tag)
            .unwrap_or_else(|| panic!("{tag}: {actual:?}"))
            .1;
    }
    // The blockquote is real HTML; the filter does not turn it into literal text.
    assert!(!actual.contains("<blockquote>"));
    assert!(
        model
            .ir()
            .nodes()
            .iter()
            .any(|node| node.kind == NodeKind::BlockQuote)
    );
}

#[test]
fn gfm_tagfilter_composes_with_disclosures_and_static_checkboxes() {
    let details = "<details><summary><script>literal</script> Details</summary>\n\n\
                   **body**\n\n</details>\n\n<div><style><b>After</b></style></div>";
    let model = parse(details, true);
    let main = preview(&model);
    assert!(
        main.contains("<script>literal</script> Details"),
        "{main:?}"
    );
    assert!(main.contains("body\n<style>After</style>"), "{main:?}");
    assert_eq!(model.disclosure_regions().len(), 1);
    assert_eq!(model.original_source().text(), details);

    for (raw, expected) in [
        (
            "<label title='<script>'>\n<input type=checkbox disabled>Keep me\n</label>",
            "\u{fffc} Keep me",
        ),
        (
            "<label>\n<input type=checkbox disabled><script>x</script>\n</label>",
            "\u{fffc} <script>x</script>",
        ),
    ] {
        let model = parse(raw, true);
        assert_eq!(preview(&model), expected);
        assert_eq!(model.ir().objects().len(), 1);
        assert_eq!(model.original_source().text(), raw);
    }
}

#[test]
fn markup_does_not_create_active_content_or_markdown_from_text() {
    let source = "<p onclick='launch()'>**literal** &lt;img src=x&gt; \
        <a href='JaVa&#x53;Cript:alert(1)'>bad link</a> \
        <img src='file:///C:/private.png' alt='blocked image'> \
        <img src='//example.org/tracker.png' alt='network path'> \
        <img src='%5C%5Cexample.org/share/tracker.png' alt='encoded UNC'> \
        <img src='%2f%2fexample.org/share/tracker.png' alt='encoded slashes'> \
        <img src='C%3A/private.png' alt='encoded drive'></p>\n\n\
        <script>fetch('https://example.org')</script>\n\n\
        <!-- invisible -->\n\n\
        <p>After</p>\n";
    let model = parse(source, true);
    let text = preview(&model);
    assert!(
        text.contains(
            "**literal** <img src=x> bad link blocked image network path encoded UNC encoded slashes encoded drive"
        ),
        "{text:?}"
    );
    assert!(text.contains("After"), "{text:?}");
    assert!(!text.contains("invisible"), "{text:?}");
    assert!(model.ir().objects().is_empty());
    assert!(
        !model
            .ir()
            .nodes()
            .iter()
            .any(|n| n.kind == NodeKind::Link || n.kind == NodeKind::Strong)
    );
}

#[test]
fn adjacent_styles_are_native_runs_without_visible_markdown_delimiters() {
    use crate::presentation::{RUN_BOLD, RUN_CODE, RUN_ITALIC};
    let model = parse(
        "<p>A<em> B </em>C<em>one</em><em>two</em><strong><code>&lt;x&gt;</code></strong><strong></strong></p>",
        true,
    );
    assert_eq!(preview(&model), "A B Conetwo<x>");
    let span = LogicalSpan::new(0, 14).unwrap();
    let styles = model.html_styles(span);
    let flags_at = |offset| {
        styles
            .iter()
            .find(|(range, _)| range.contains(&offset))
            .map_or(0, |(_, flags)| *flags)
    };
    assert_eq!(flags_at(0), 0);
    assert_eq!(flags_at(2), RUN_ITALIC);
    assert_eq!(flags_at(4), 0);
    assert_eq!(flags_at(5), RUN_ITALIC);
    assert_eq!(flags_at(8), RUN_ITALIC);
    assert_eq!(flags_at(11), RUN_BOLD | RUN_CODE);
}

#[test]
fn empty_blocks_and_encoded_whitespace_do_not_create_accidental_structure() {
    let model = parse(
        "<p><strong></strong></p><h1></h1><p>&#32;&#32;&#32;&#32;Body&#10;&#10;tail</p>",
        true,
    );
    let nodes = model.ir().nodes();
    assert!(
        !nodes
            .iter()
            .any(|row| matches!(row.kind, NodeKind::CodeBlock | NodeKind::ThematicBreak))
    );
    let heading = nodes
        .iter()
        .find(|row| row.kind == NodeKind::Heading)
        .unwrap();
    assert_eq!(heading.logical.start(), heading.logical.end());
    assert_eq!(
        nodes
            .iter()
            .filter(|row| row.kind == NodeKind::Paragraph)
            .count(),
        1
    );
    assert!(preview(&model).contains("Body tail"));
}

#[test]
fn breaks_stay_inside_their_native_heading_or_paragraph() {
    let model = parse("<h1>A<br>B</h1><p>C<br><br>D<br></p>", true);
    let text = preview(&model);
    assert!(
        text.contains("A\nB") && text.contains("C\n\nD\n"),
        "{text:?}"
    );
    assert_eq!(
        model
            .ir()
            .nodes()
            .iter()
            .filter(|row| row.kind == NodeKind::Paragraph)
            .count(),
        1
    );
}

#[test]
fn an_empty_html_block_keeps_adjacent_markdown_lists_separate() {
    let model = parse(
        "- first\n\n<!-- divider -->\n\n- second\n\n<p>rendered</p>",
        true,
    );
    assert_eq!(
        model
            .ir()
            .nodes()
            .iter()
            .filter(|row| row.kind == NodeKind::List)
            .count(),
        2
    );
    assert!(preview(&model).contains("rendered"));
}

#[test]
fn html_whitespace_collapses_across_nested_tags_and_entities() {
    for (source, expected) in [
        ("<p>A <em> B</em>C</p>", "A BC"),
        ("<p>A <a href='#x'> B </a> C</p>", "A B C"),
        (
            "<p><a href='#x'></a>one <a href='#y'> two </a>end</p>",
            "one two end",
        ),
        ("<p> A &#32; &#10; B </p>", "A B"),
        ("<p>A <img> B</p>", "A B"),
    ] {
        assert_eq!(preview(&parse(source, true)), expected, "{source}");
    }
}

#[test]
fn malformed_and_unsupported_blocks_keep_their_literal_fallback() {
    for source in [
        "<div><p>not closed",
        "<div>é&#9;<strong></em></s></div>",
        "<p><object data='https://example.org'></object></p>",
        "<svg><image href='https://example.org'></image></svg>",
    ] {
        let model = parse(source, true);
        assert_eq!(model.source().text(), source);
        assert!(preview(&model).contains(source));
        assert!(model.ir().objects().is_empty());
    }
}

#[test]
fn points_and_ranges_return_original_byte_offsets_after_crlf_html() {
    let source =
        "Before\r\n\r\n<h2 align='right'>Hé😀ading</h2>\r\n<p>other</p>\r\n\r\nAfter 中😀 end";
    let model = parse(source, true);
    let text = preview(&model);
    let flow = model.main_flow_id();
    let mut op = model.budget().begin(CancellationToken::new());
    for needle in ["Before", "Hé😀ading", "other", "After", "中😀", "end"] {
        let start = source.find(needle).unwrap() as u64;
        let logical = text[..text.find(needle).unwrap()].encode_utf16().count() as u64;
        let point = PointQuery {
            model: model.identity(),
            input: PointInput::Source {
                target: TextTarget::Flow(flow),
                byte: SourceBoundary::new(start),
            },
            affinity: Affinity::After,
        };
        let chosen = model.map_point(&point, &mut op).unwrap().results()[0].chosen;
        assert_eq!(
            chosen,
            Position::Flow(LogicalBoundary::new(logical)),
            "{needle}"
        );
        let inverse = PointQuery {
            input: PointInput::Flow {
                flow,
                utf16: LogicalBoundary::new(logical),
            },
            ..point
        };
        let chosen = model.map_point(&inverse, &mut op).unwrap().results()[0].chosen;
        assert_eq!(
            chosen,
            Position::Source(SourceBoundary::new(start)),
            "{needle}"
        );
    }
    let start = source.find("After").unwrap() as u64;
    let span = SourceSpan::new(start, start + 5).unwrap();
    let selection = model
        .map_range(
            &RangeQuery {
                model: model.identity(),
                input: RangeInput::Source {
                    target: TextTarget::Flow(flow),
                    span,
                },
                empty_affinity: Affinity::After,
            },
            &mut op,
        )
        .unwrap();
    assert_eq!(selection.contributions()[0].source, span);
}

#[test]
fn punctuation_and_entities_do_not_expand_adjacent_exact_text_selections() {
    let source = "<p>Hello, R&amp;D!</p>";
    let model = parse(source, true);
    let flow = model.main_flow_id();
    let mut op = model.budget().begin(CancellationToken::new());
    let span = SourceSpan::new(4, 5).unwrap();
    let response = model
        .map_range(
            &RangeQuery {
                model: model.identity(),
                input: RangeInput::Source {
                    target: TextTarget::Flow(flow),
                    span,
                },
                empty_affinity: Affinity::After,
            },
            &mut op,
        )
        .unwrap();
    let selection = response.selections()[0];
    assert_eq!(
        (selection.start.chosen.value(), selection.end.chosen.value()),
        (1, 2)
    );
    assert_eq!(response.contributions()[0].source, span);
}

#[test]
fn preview_contributions_use_tight_original_content_spans() {
    use crate::ir::AtomKind;
    let model = parse("<p>A!</p>", true);
    let mut op = model.budget().begin(CancellationToken::new());
    let response = model
        .map_range(
            &RangeQuery {
                model: model.identity(),
                input: RangeInput::Flow {
                    flow: model.main_flow_id(),
                    span: LogicalSpan::new(0, 1).unwrap(),
                },
                empty_affinity: Affinity::After,
            },
            &mut op,
        )
        .unwrap();
    assert_eq!(
        response.contributions()[0].source,
        SourceSpan::new(3, 4).unwrap()
    );
    assert_eq!(response.contributions()[0].kind, AtomKind::Copy);
}

#[test]
fn points_inside_an_html_entity_use_atomic_affinities() {
    use crate::mapping::Correspondence;
    let model = parse("<p>A&amp;B</p>", true);
    let target = TextTarget::Flow(model.main_flow_id());
    let mut op = model.budget().begin(CancellationToken::new());
    for (byte, affinity, expected) in [(5, Affinity::After, 2), (8, Affinity::Before, 1)] {
        let response = model
            .map_point(
                &PointQuery {
                    model: model.identity(),
                    input: PointInput::Source {
                        target,
                        byte: SourceBoundary::new(byte),
                    },
                    affinity,
                },
                &mut op,
            )
            .unwrap();
        let result = response.results()[0];
        assert_eq!(
            result.chosen,
            Position::Flow(LogicalBoundary::new(expected))
        );
        assert_eq!(result.correspondence, Correspondence::Atomic);
    }
}
