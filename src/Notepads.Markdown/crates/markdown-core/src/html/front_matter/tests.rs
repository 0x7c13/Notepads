// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE file in the project root for license information.
// ---------------------------------------------------------------------------------------------

use super::*;
use crate::html::tests::preview;
use crate::ir::{DetailRow, NodeKind};

#[test]
fn review_regression_front_matter_reservation_failures_latch_first_fault() {
    use crate::error::LimitKind;
    use crate::identity::SourceKey;
    use crate::policy::{CancellationToken, ModelLimits, SourceBudget, SourceLimits};
    for masking in [false, true] {
        let budget = SourceBudget::new_with_model_limits(
            SourceLimits {
                auxiliary_capacity: if masking { 32 * 1024 * 1024 } else { 4096 },
                ..SourceLimits::default()
            },
            ModelLimits::default(),
        )
        .unwrap();
        let mut op = budget.begin(CancellationToken::new());
        let source = SourceDocument::new(
            SourceKey::new([82; 16], 1).unwrap(),
            b"---\ntitle: x\n---\nBody",
            &mut op,
        )
        .unwrap()
        .snapshot();
        let config = ParseConfig::with_extensions(Profile::G3, Extensions::FRONT_MATTER);
        let error = if masking {
            let prepared = prepare(&source, config, &mut op).unwrap().unwrap();
            let free = budget.limits().auxiliary_capacity - budget.usage().auxiliary_capacity;
            let _held = budget.reserve_auxiliary(free).unwrap();
            prepared.mask(&source, &mut op).err().unwrap()
        } else {
            prepare(&source, config, &mut op).err().unwrap()
        };
        assert!(matches!(
            error,
            CoreError::ResourceLimit {
                kind: LimitKind::AuxiliaryCapacity,
                ..
            }
        ));
        assert_eq!(op.first_fault(), Some(error));
    }
}

fn parse_extensions(source: &str, extensions: Extensions) -> crate::model::AcceptedModel {
    use crate::identity::SourceKey;
    use crate::model::AcceptedModel;
    use crate::policy::{CancellationToken, SourceBudget};
    let budget = SourceBudget::preview();
    let mut op = budget.begin(CancellationToken::new());
    let input = SourceDocument::new(
        SourceKey::new([81; 16], 1).unwrap(),
        source.as_bytes(),
        &mut op,
    )
    .unwrap();
    AcceptedModel::parse(
        input.snapshot(),
        ParseConfig::with_extensions(Profile::G3, extensions),
        1,
        &mut op,
    )
    .unwrap()
}

#[test]
fn citations_use_decoded_yaml_keys_in_every_display_mode() {
    let source = concat!(
        "---\n",
        r#""\u0072eferences": [{id: work, title: Title, author: [{family: Name}]}]"#,
        "\n---\n[@work]"
    );
    for mode in [
        Extensions::FRONT_MATTER,
        Extensions::FRONT_MATTER_CODE,
        Extensions::FRONT_MATTER_HIDE,
    ] {
        let extensions = Extensions::from_bits(mode.bits() | Extensions::CITATIONS.bits()).unwrap();
        let model = parse_extensions(source, extensions);
        assert!(
            preview(&model).contains("(Name n.d.)\nReferences"),
            "{mode:?}"
        );
        assert_eq!(model.original_source().text(), source);
    }
}

#[test]
fn bibliography_emails_stay_literal_in_every_front_matter_mode() {
    use crate::ir::PayloadRole;
    let source = "---\nreferences: [{id: work, title: title@example.com, author: [{family: author@example.org}]}]\n---\n[@work]\n\nbody&#64;example.net";
    for mode in [
        Extensions::FRONT_MATTER,
        Extensions::FRONT_MATTER_CODE,
        Extensions::FRONT_MATTER_HIDE,
    ] {
        let extensions = Extensions::from_bits(mode.bits() | Extensions::CITATIONS.bits()).unwrap();
        let model = parse_extensions(source, extensions);
        let mut destinations = model
            .ir()
            .payloads()
            .iter()
            .filter(|row| row.role == PayloadRole::Destination)
            .map(|row| model.ir().text(row.value).unwrap())
            .collect::<Vec<_>>();
        destinations.sort_unstable();
        let expected = if mode == Extensions::FRONT_MATTER_HIDE {
            // Hide mode retains its existing invisible link barrier.
            vec!["", "#notepads-reference-000", "mailto:body@example.net"]
        } else {
            vec!["#notepads-reference-000", "mailto:body@example.net"]
        };
        assert_eq!(destinations, expected, "{mode:?}");
        let text = preview(&model);
        assert!(text.contains("title@example.com"), "{mode:?}: {text}");
        assert!(text.contains("author@example.org"), "{mode:?}: {text}");
    }
}

#[test]
fn closed_initial_preamble_uses_real_table_and_keeps_original_source() {
    for ending in ["\n", "\r\n", "\r"] {
        let source = format!(
            "---{ending}title: '中😀'{ending}enabled: true{ending}---{ending}# Body{ending}"
        );
        let model = parse_extensions(&source, Extensions::FRONT_MATTER);
        assert_eq!(model.original_source().text(), source);
        assert_eq!(
            model
                .ir()
                .nodes()
                .iter()
                .filter(|n| n.kind == NodeKind::Table)
                .count(),
            1
        );
        let text = preview(&model);
        assert!(
            text.contains("title")
                && text.contains("中😀")
                && text.contains("true")
                && text.contains("Body"),
            "{text:?}"
        );
        assert!(!text.contains("---"));
        let table = model
            .ir()
            .nodes()
            .iter()
            .position(|n| n.kind == NodeKind::Table)
            .unwrap();
        assert!(model.is_front_matter_table(crate::ir::NodeId::from_index(table).unwrap()));
    }
}

#[test]
fn unclosed_noninitial_bom_indented_and_dots_stay_ordinary_markdown() {
    for source in [
        "---\na: 1\n",
        "\n---\na: 1\n---\n",
        "\u{feff}---\na: 1\n---\n",
        " ---\na: 1\n---\n",
        "---\na: 1\n...\n",
        "# A\n---\na: 1\n---\n",
        "> ---\n> a: 1\n> ---\n",
        "---\na: 1\n ---\n",
    ] {
        let model = parse_extensions(source, Extensions::FRONT_MATTER);
        assert_eq!(model.source().text(), source, "{source:?}");
    }
}

#[test]
fn code_and_hidden_modes_do_not_require_valid_yaml() {
    let source = "---\na: [unclosed\n```html\n<script>bad</script>\n```\n---\n# Body\n";
    let table = parse_extensions(source, Extensions::FRONT_MATTER);
    assert!(preview(&table).contains("Failed to parse YAML front matter"));
    let code = parse_extensions(source, Extensions::from_bits(0x300).unwrap());
    assert!(preview(&code).contains("a: [unclosed\n```html\n<script>bad</script>\n```"));
    assert_eq!(
        code.ir()
            .nodes()
            .iter()
            .filter(|n| n.kind == NodeKind::CodeBlock)
            .count(),
        1
    );
    assert!(code.ir().details().iter().any(|detail| matches!(detail, DetailRow::Code { info: Some(info), .. } if code.ir().text(code.ir().payloads()[info.index()].value).unwrap() == "yaml")));
    let hidden = parse_extensions(source, Extensions::from_bits(0x500).unwrap());
    assert_eq!(preview(&hidden), "Body");
    assert_eq!(hidden.original_source().text(), source);
}

#[test]
fn yaml_markup_is_inert_and_cannot_create_links_images_or_math() {
    let source = "---\nkey: '**bold** [link](https://example.org) ![image](x.png) <script>bad</script> $x$ | `code`'\n---\n# Body\n";
    let model = parse_extensions(source, Extensions::ALL);
    let text = preview(&model);
    assert!(
        text.contains(
            "**bold** [link](https://example.org) ![image](x.png) <script>bad</script> $x$ | `code`"
        ),
        "{text:?}"
    );
    assert_eq!(model.ir().objects().len(), 0);
    assert!(!model.ir().nodes().iter().any(|n| matches!(
        n.kind,
        NodeKind::Link | NodeKind::Emphasis | NodeKind::Strong
    )));
}

#[test]
fn projected_literal_emails_keep_authored_body_autolinks_and_html_links() {
    use crate::ir::PayloadRole;
    for prefix in [
        "---\ncontact: alice@example.com\nmeta: {contact: alice@example.com}\n---\n",
        "+++\ncontact = \"alice@example.com\"\nmeta = {contact = \"alice@example.com\"}\n+++\n",
        "{\"contact\":\"alice@example.com\",\"meta\":{\"contact\":\"alice@example.com\"}}\n",
        "<p>alice@example.com <a href='mailto:authored@example.net'>authored</a></p>\n",
        "<table><tr><td>alice@example.com</td><td><a href='mailto:authored@example.net'>authored</a></td></tr></table>\n",
        "alice@<ruby>example.com<rt>annotation</rt></ruby>\n",
    ] {
        for newline in ["\n", "\r\n"] {
            let explicit = if prefix.contains("href=") {
                ""
            } else {
                "\n[authored](mailto:authored@example.net)\n"
            };
            let source =
                format!("{prefix}\nbody&#64;example.net\n{explicit}").replace('\n', newline);
            let model = parse_extensions(&source, Extensions::ALL);
            let mut destinations = model
                .ir()
                .payloads()
                .iter()
                .filter(|row| row.role == PayloadRole::Destination)
                .map(|row| model.ir().text(row.value).unwrap())
                .collect::<Vec<_>>();
            destinations.sort_unstable();
            assert_eq!(
                destinations,
                ["mailto:authored@example.net", "mailto:body@example.net"],
                "{source:?}"
            );
            assert!(preview(&model).contains("alice@example.com"));
            assert_eq!(model.original_source().text(), source);
        }
    }
}

#[test]
fn front_matter_retains_and_budgets_every_code_style() {
    use crate::html::HtmlProjection;
    use crate::identity::SourceKey;
    use crate::policy::{CancellationToken, SourceBudget};

    let retained = |count: usize| {
        let source = format!("---\nitems: [{}]\n---\n", vec!["{a: 1}"; count].join(", "));
        let budget = SourceBudget::preview();
        let mut op = budget.begin(CancellationToken::new());
        let document = SourceDocument::new(
            SourceKey::new([81; 16], 1).unwrap(),
            source.as_bytes(),
            &mut op,
        )
        .unwrap();
        let config = ParseConfig::with_extensions(Profile::G3, Extensions::FRONT_MATTER);
        let front = prepare(&document.snapshot(), config, &mut op).unwrap();
        assert_eq!(front.as_ref().unwrap().fragment.parts.len(), 1);
        let (projection, text, text_storage) =
            HtmlProjection::build(None, config, &document.snapshot(), front, &mut op)
                .unwrap()
                .unwrap();
        assert_eq!(projection.styles.len(), count);
        assert_eq!(projection.styles.capacity(), count);
        drop(text);
        drop(text_storage);
        let before = budget.usage().auxiliary_capacity;
        drop(projection);
        before - budget.usage().auxiliary_capacity
    };
    assert_eq!(
        retained(12) - retained(1),
        11 * size_of::<(std::ops::Range<u64>, u32)>() as u64
    );
}

#[test]
fn yaml_html_does_not_consume_body_html_across_the_closing_marker() {
    let source = "---\na: |\n  <div>\n---\n<p>body &amp; 😀</p>\n\n# Heading\n";
    let model = parse_extensions(source, Extensions::ALL);
    let text = preview(&model);
    assert!(text.contains("<div>"), "{text:?}");
    assert!(text.contains("body & 😀"), "{text:?}");
    assert!(!text.contains("<p>"), "{text:?}");
    assert!(text.ends_with("Heading"), "{text:?}");
    assert_eq!(model.original_source().text(), source);
}

#[test]
fn header_definitions_and_math_do_not_reinterpret_body() {
    let source = "---\nref: '[victim]: https://example.org'\nmath: '$x$'\n---\n[victim]\n\n$y$\n";
    let model = parse_extensions(source, Extensions::ALL);
    assert!(!model.ir().nodes().iter().any(|n| n.kind == NodeKind::Link));
    assert_eq!(
        model
            .ir()
            .nodes()
            .iter()
            .filter(|n| n.kind == NodeKind::Math)
            .count(),
        1
    );
    assert!(preview(&model).contains("[victim]"));
}

fn read_span(model: &crate::model::AcceptedModel, span: crate::identity::LogicalSpan) -> String {
    let mut result = String::new();
    let mut op = model
        .budget()
        .begin(crate::policy::CancellationToken::new());
    crate::reader::text::visit(model, model.main_flow_id(), span, &mut op, |text, _| {
        result.push_str(text);
        Ok(())
    })
    .unwrap();
    result
}

#[test]
fn front_matter_ranges_are_atomic_and_body_points_remain_exact() {
    use crate::identity::{LogicalBoundary, LogicalSpan, SourceBoundary, SourceSpan};
    use crate::mapping::{Affinity, PointInput, PointQuery, Position, RangeInput, RangeQuery};
    use crate::policy::CancellationToken;
    let source = "---\ntitle: 中😀\n---\n# BodyXYZ\n";
    let model = parse_extensions(source, Extensions::FRONT_MATTER);
    let text = preview(&model);
    let start = text[..text.find("中😀").unwrap()].encode_utf16().count() as u64;
    let mut op = model.budget().begin(CancellationToken::new());
    let response = model
        .map_range(
            &RangeQuery {
                model: model.identity(),
                input: RangeInput::Flow {
                    flow: model.main_flow_id(),
                    span: LogicalSpan::new(start, start + 3).unwrap(),
                },
                empty_affinity: Affinity::After,
            },
            &mut op,
        )
        .unwrap();
    let header_end = source.find("# BodyXYZ").unwrap() as u64;
    assert!(!response.contributions().is_empty());
    for part in response.contributions() {
        assert_eq!(part.source, SourceSpan::new(0, header_end).unwrap());
    }
    let logical = text[..text.find('X').unwrap()].encode_utf16().count() as u64;
    for affinity in [Affinity::Before, Affinity::After] {
        let point = model
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
        assert_eq!(
            point.results()[0].chosen,
            Position::Source(SourceBoundary::new(source.find('X').unwrap() as u64))
        );
        assert!(!point.results()[0].effects.atomic);
    }
}

#[test]
fn front_matter_source_ranges_preserve_original_intersections() {
    use crate::identity::SourceSpan;
    use crate::ir::{AtomKind, TextTarget};
    use crate::mapping::{Affinity, RangeInput, RangeQuery, RelatedSpan};
    use crate::policy::CancellationToken;

    let source = "---\na: one\n---\n\nhi";
    let model = parse_extensions(source, Extensions::FRONT_MATTER);
    assert_eq!(preview(&model), "a\tone\nhi");
    for (start, end) in [(1, 2), (7, 8), (12, 13)] {
        for affinity in [Affinity::Before, Affinity::After] {
            let span = SourceSpan::new(start, end).unwrap();
            let mut op = model.budget().begin(CancellationToken::new());
            let response = model
                .map_range(
                    &RangeQuery {
                        model: model.identity(),
                        input: RangeInput::Source {
                            target: TextTarget::Flow(model.main_flow_id()),
                            span,
                        },
                        empty_affinity: affinity,
                    },
                    &mut op,
                )
                .unwrap();
            assert_eq!(response.selections().len(), 1);
            assert!(!response.contributions().is_empty());
            assert!(
                response
                    .contributions()
                    .iter()
                    .all(|row| row.source == span)
            );
            let selection = response.selections()[0];
            assert!(selection.effects.atomic && selection.effects.hidden);
            assert!(!selection.effects.synthetic);
            assert!(selection.expanded);
            assert!(
                response
                    .contributions()
                    .iter()
                    .all(|row| matches!(row.kind, AtomKind::Transform | AtomKind::Hidden))
            );
            let related = selection.related.spans.slice(response.spans()).unwrap();
            assert_eq!(
                related,
                [
                    RelatedSpan::Flow(crate::identity::LogicalSpan::new(0, 1).unwrap()),
                    RelatedSpan::Flow(crate::identity::LogicalSpan::new(2, 5).unwrap()),
                ]
            );
        }
    }
}

#[test]
fn front_matter_source_boundaries_and_batched_ranges_cover_table_code_hide_and_html() {
    use crate::identity::{SourceBoundary, SourceSpan};
    use crate::ir::{AtomKind, TextTarget};
    use crate::mapping::{
        Affinity, Effects, PointInput, PointQuery, RangeInput, RangeQuery, RangeResult,
    };
    use crate::policy::CancellationToken;

    for header in [
        "---\na: 中😀\nb: two\n---",
        "+++\na = '中😀'\nb = 'two'\n+++",
        "{\"a\":\"中😀\",\"b\":\"two\"}",
    ] {
        for ending in ["\n", "\r\n", "\r"] {
            let source = format!("{header}\n\n<p>x&amp;y</p>\n\nhi").replace('\n', ending);
            let boundaries = (0..=source.len())
                .filter(|&byte| source.is_char_boundary(byte))
                .map(|byte| byte as u64)
                .collect::<Vec<_>>();
            for bits in [0x10f, 0x30f, 0x50f] {
                let model = parse_extensions(&source, Extensions::from_bits(bits).unwrap());
                let target = TextTarget::Flow(model.main_flow_id());
                let points = boundaries
                    .iter()
                    .flat_map(|&byte| {
                        [Affinity::Before, Affinity::After].map(|affinity| PointQuery {
                            model: model.identity(),
                            input: PointInput::Source {
                                target,
                                byte: SourceBoundary::new(byte),
                            },
                            affinity,
                        })
                    })
                    .collect::<Vec<_>>();
                for queries in points.chunks(64) {
                    let mut op = model.budget().begin(CancellationToken::new());
                    let response = model.map_points(queries, &mut op).unwrap();
                    assert_eq!(response.results().len(), queries.len());
                    for point in response.results() {
                        point.related.spans.slice(response.spans()).unwrap();
                        point.related.anchors.slice(response.anchors()).unwrap();
                        for row in point.contributors.slice(response.contributors()).unwrap() {
                            model.original_source().validate_span(row.source).unwrap();
                        }
                    }
                }
                let mut ranges = Vec::new();
                for (index, &start) in boundaries.iter().enumerate() {
                    for &end in &boundaries[index..] {
                        for affinity in [Affinity::Before, Affinity::After] {
                            ranges.push(RangeQuery {
                                model: model.identity(),
                                input: RangeInput::Source {
                                    target,
                                    span: SourceSpan::new(start, end).unwrap(),
                                },
                                empty_affinity: affinity,
                            });
                        }
                    }
                }
                for queries in ranges.chunks(64) {
                    let mut op = model.budget().begin(CancellationToken::new());
                    let response = model.map_ranges(queries, &mut op).unwrap_or_else(|error| {
                        panic!("{ending:?}, bits={bits:x}, {queries:?}: {error:?}")
                    });
                    let mut content_rows = 0;
                    assert_eq!(response.results().len(), queries.len());
                    for (query, result) in queries.iter().zip(response.results()) {
                        let RangeInput::Source { span, .. } = query.input else {
                            unreachable!()
                        };
                        let selection = match result {
                            RangeResult::Boundary(rows) => {
                                assert_eq!(rows.slice(response.boundaries()).unwrap().len(), 1);
                                continue;
                            }
                            RangeResult::Selection(rows) => {
                                let selections = rows.slice(response.selections()).unwrap();
                                assert_eq!(selections.len(), 1);
                                selections[0]
                            }
                        };
                        selection.related.spans.slice(response.spans()).unwrap();
                        selection.related.anchors.slice(response.anchors()).unwrap();
                        for point in [selection.start, selection.end] {
                            point.related.spans.slice(response.spans()).unwrap();
                            point.related.anchors.slice(response.anchors()).unwrap();
                            for row in point.contributors.slice(response.contributors()).unwrap() {
                                model.original_source().validate_span(row.source).unwrap();
                            }
                        }
                        let mut effects = Effects::default();
                        effects.include(selection.start.effects);
                        effects.include(selection.end.effects);
                        let contributions = selection
                            .contributions
                            .slice(response.contributions())
                            .unwrap();
                        content_rows += contributions.len();
                        for row in contributions {
                            model.original_source().validate_span(row.source).unwrap();
                            assert!(
                                span.start() <= row.source.start()
                                    && row.source.end() <= span.end()
                            );
                            if row.source.is_empty() {
                                assert!(
                                    span.start() < row.source.start()
                                        && row.source.start() < span.end()
                                );
                            }
                            effects.include(Effects {
                                atomic: row.kind == AtomKind::Transform,
                                hidden: row.kind == AtomKind::Hidden,
                                synthetic: row.kind == AtomKind::Synthetic,
                                object: row.kind == AtomKind::Object,
                            });
                            assert!(!row.expanded || selection.expanded);
                        }
                        assert_eq!(selection.effects, effects, "bits={bits:x}, {span:?}");
                    }
                    assert_eq!(content_rows, response.contributions().len());
                }
            }
        }
    }
}

#[test]
fn toml_and_json_front_matter_keep_modes_literals_and_wide_numbers() {
    for (language, metadata, title, number) in [
        (
            "toml",
            "+++\ntitle = '中文 😀'\ncount = 9007199254740993\ntags = ['note', 'preview']\n[team]\nowner = 'Ada'\n+++",
            "中文 😀",
            "9007199254740993",
        ),
        (
            "json",
            r#"{
  "title": "JSON 😀",
  "count": 1844674407370955161600000,
  "tags": ["note", "preview"],
  "nested": {"value": "} and \"quoted\"", "$serde_json::private::Number": "123"}
}"#,
            "JSON 😀",
            "1844674407370955161600000",
        ),
    ] {
        let source = format!("{metadata}\n\n# Body 😀\n");
        let table = parse_extensions(&source, Extensions::FRONT_MATTER);
        let text = preview(&table);
        assert!(
            text.contains(title) && text.contains(number) && text.contains("note"),
            "{text:?}"
        );
        assert_eq!(table.original_source().text(), source);
        assert!(table.ir().nodes().iter().enumerate().any(|(index, node)| {
            node.kind == NodeKind::Table
                && table.is_front_matter_table(crate::ir::NodeId::from_index(index).unwrap())
        }));
        if language == "json" {
            assert!(text.contains("$serde_json::private::Number"), "{text:?}");
        }
        let code = parse_extensions(&source, Extensions::FRONT_MATTER_CODE);
        assert!(preview(&code).contains(title));
        assert!(code.ir().details().iter().any(|detail| matches!(detail,
            DetailRow::Code { info: Some(info), .. }
            if code.ir().text(code.ir().payloads()[info.index()].value).unwrap() == language)));
        let hidden = parse_extensions(&source, Extensions::FRONT_MATTER_HIDE);
        assert_eq!(preview(&hidden), "Body 😀");
        assert_eq!(hidden.original_source().text(), source);
    }
}

#[test]
fn structured_front_matter_chunks_large_tokens_and_long_strings() {
    let long = "字符".repeat(12_000);
    for source in [
        format!(
            "+++\na = '{long}'\ntags = [{}]\n+++\n\nBody",
            "1,".repeat(300)
        ),
        format!(
            "{{\"a\":\"{long}\",\"tags\":[{}0]}}\n\nBody",
            "1,".repeat(300)
        ),
    ] {
        let model = parse_extensions(&source, Extensions::FRONT_MATTER);
        let projected = preview(&model);
        assert!(projected.contains(&long) && projected.ends_with("Body"));
        assert_eq!(model.original_source().text(), source);
    }
}

#[test]
fn toml_field_names_remain_authored_and_stored_depth_is_bounded() {
    let source =
        "+++\nx = {\"$__toml_private_datetime\" = '1979-05-27', title = 'Hello'}\n+++\n\nBody";
    let shown = preview(&parse_extensions(source, Extensions::FRONT_MATTER));
    assert!(
        shown.contains("$__toml_private_datetime")
            && shown.contains("Hello")
            && shown.ends_with("Body")
    );

    let key = (0..10)
        .map(|index| format!("k{index}"))
        .collect::<Vec<_>>()
        .join(".");
    let opening = format!("{key} = {{");
    let source = format!(
        "+++\nx = {{{}{}}}\n+++\n\nBody",
        opening.repeat(8),
        format!("answer = 42{}", "}".repeat(8))
    );
    let shown = preview(&parse_extensions(&source, Extensions::FRONT_MATTER));
    assert!(
        shown.contains("TOML front matter exceeds") && shown.ends_with("Body"),
        "{shown:?}"
    );
}

#[test]
fn malformed_json_and_noninitial_markers_do_not_hide_prose() {
    for raw in [
        "{ ordinary prose }\n\nBody",
        "{\"key\": }\n\nBody",
        "{\"key\": [1, 2}\n\nBody",
        "{\"key\": \"text}\n\nBody",
        "{\"key\": true} trailing prose\n\nBody",
        "\n{\"key\": true}\n\nBody",
        "+++\ntitle = 'no closing fence'\n\nBody",
    ] {
        let shown = preview(&parse_extensions(raw, Extensions::FRONT_MATTER_HIDE));
        assert!(
            shown.ends_with("Body") && shown != "Body",
            "{raw:?}: {shown:?}"
        );
    }
    let raw = "+++\na=1\na=2\n+++\n\nBody";
    let shown = preview(&parse_extensions(raw, Extensions::FRONT_MATTER));
    assert!(
        shown.starts_with("Failed to parse TOML front matter") && shown.contains("Invalid TOML")
    );
    assert_eq!(
        preview(&parse_extensions(raw, Extensions::FRONT_MATTER_HIDE)),
        "Body"
    );
}

#[test]
fn c0_table_configuration_is_rejected_without_changing_the_c0_body_dialect() {
    use crate::identity::SourceKey;
    use crate::model::AcceptedModel;
    use crate::policy::{CancellationToken, SourceBudget};
    let source = "---\ntitle: test\n---\n|a|b|\n|---|---|\n|x|y|\n";
    for mode in [0x100, 0x300, 0x500] {
        let budget = SourceBudget::preview();
        let mut op = budget.begin(CancellationToken::new());
        let input = SourceDocument::new(
            SourceKey::new([82; 16], 1).unwrap(),
            source.as_bytes(),
            &mut op,
        )
        .unwrap();
        let result = AcceptedModel::parse(
            input.snapshot(),
            ParseConfig::with_extensions(Profile::C0, Extensions::from_bits(mode).unwrap()),
            1,
            &mut op,
        );
        if mode == 0x100 {
            assert!(matches!(result, Err(CoreError::UnsupportedConfiguration)));
        } else {
            assert!(
                !result
                    .unwrap()
                    .ir()
                    .nodes()
                    .iter()
                    .any(|node| node.kind == NodeKind::Table)
            );
        }
    }
}

#[test]
fn all_parse_entries_reject_a_foreign_source_budget_before_masking() {
    use crate::identity::SourceKey;
    use crate::model::{AcceptedModel, ParseProgress};
    use crate::policy::{CancellationToken, SourceBudget};
    for mode in [0x100, 0x300, 0x500] {
        for entry in 0..2 {
            let original = SourceBudget::preview();
            let foreign = SourceBudget::preview();
            let mut source_op = original.begin(CancellationToken::new());
            let source = SourceDocument::new(
                SourceKey::new([83; 16], 1).unwrap(),
                b"---\ntitle: test\n---\n# Body",
                &mut source_op,
            )
            .unwrap()
            .snapshot();
            let mut op = foreign.begin(CancellationToken::new());
            let config =
                ParseConfig::with_extensions(Profile::G3, Extensions::from_bits(mode).unwrap());
            let result = match entry {
                0 => AcceptedModel::parse(source, config, 1, &mut op),
                _ => AcceptedModel::experimental_parse_with_progress(
                    source,
                    config,
                    1,
                    &mut op,
                    &mut ParseProgress::default(),
                ),
            };
            assert!(
                matches!(result, Err(CoreError::WrongBudget)),
                "mode={mode} entry={entry}"
            );
            assert_eq!(foreign.usage().auxiliary_capacity, 0);
        }
    }
}

#[test]
fn long_lines_do_not_exceed_the_operation_transport_chunk_contract() {
    let ordinary = format!("---{}x\n\nBody", " ".repeat(70_000));
    let model = parse_extensions(&ordinary, Extensions::FRONT_MATTER);
    assert_eq!(model.source().text(), ordinary);
    let source = format!("---\nvalue: {}\n---\n# Body", "x".repeat(70_000));
    let model = parse_extensions(&source, Extensions::FRONT_MATTER);
    assert!(preview(&model).contains(&"x".repeat(70_000)));
}

fn contract_text(nodes: &[serde_json::Value], depth: usize) -> String {
    let mut out = String::new();
    for node in nodes {
        if node["kind"] == "list" {
            for (index, item) in node["items"].as_array().unwrap().iter().enumerate() {
                if index != 0 {
                    out.push('\n');
                }
                for _ in 0..depth {
                    out.push_str("\u{a0}\u{a0}");
                }
                out.push_str("• ");
                let children = item.as_array().unwrap();
                if children
                    .first()
                    .is_some_and(|child| child["kind"] == "list")
                {
                    out.push('\n');
                }
                out.push_str(&contract_text(children, depth + 1));
            }
        } else {
            out.push_str(node["renderedText"].as_str().unwrap());
        }
    }
    out
}

#[test]
fn pinned_vscode_contract_survives_the_actual_projection_parser_and_reader() {
    let contract: serde_json::Value = serde_json::from_str(include_str!(
        "../../../tests/front-matter-vscode-1.145.2/oracle-contract.json"
    ))
    .unwrap();
    let cases = contract["cases"].as_array().unwrap();
    assert_eq!(cases.len(), 61);
    for case in cases {
        let name = case["name"].as_str().unwrap();
        let source = case["input"].as_str().unwrap();
        let model = parse_extensions(source, Extensions::FRONT_MATTER);
        assert_eq!(model.original_source().text(), source, "{name}");
        if !case["recognition"]["recognized"].as_bool().unwrap()
            || case["recognition"]["nested"] == true
        {
            assert_eq!(
                model.source().text(),
                source,
                "{name}: ordinary input must not be rewritten"
            );
            continue;
        }
        if name == "recursive-map" || name == "recursive-array" || name == "recursive-root-array" {
            assert!(
                preview(&model).contains("Failed to parse YAML front matter"),
                "{name}: safe bounded cycle error"
            );
        } else {
            match case["table"]["kind"].as_str().unwrap() {
                "table" => {
                    let table = model
                        .ir()
                        .nodes()
                        .iter()
                        .find(|node| node.kind == NodeKind::Table)
                        .unwrap_or_else(|| panic!("{name}: {:?}", preview(&model)));
                    let mut row = table.first_child;
                    for expected in case["table"]["rows"].as_array().unwrap() {
                        let current = &model.ir().nodes()[row.expect(name).index()];
                        let key_id = current.first_child.unwrap();
                        let key = &model.ir().nodes()[key_id.index()];
                        let value_id = key.next_sibling.unwrap();
                        let value = &model.ir().nodes()[value_id.index()];
                        assert_eq!(
                            read_span(&model, key.logical),
                            expected["renderedKey"].as_str().unwrap(),
                            "{name}: key"
                        );
                        assert_eq!(
                            read_span(&model, value.logical),
                            contract_text(expected["value"].as_array().unwrap(), 0),
                            "{name}: value"
                        );
                        row = current.next_sibling;
                    }
                    assert!(row.is_none(), "{name}: exact row count");
                }
                "empty" => assert!(
                    !model
                        .ir()
                        .nodes()
                        .iter()
                        .any(|node| node.kind == NodeKind::Table),
                    "{name}"
                ),
                "error" | "throws" => assert!(
                    preview(&model).contains("Failed to parse YAML front matter"),
                    "{name}"
                ),
                other => panic!("unexpected recognized table state {other}: {name}"),
            }
        }
        let code = parse_extensions(source, Extensions::from_bits(0x300).unwrap());
        let block = code
            .ir()
            .nodes()
            .iter()
            .find(|node| node.kind == NodeKind::CodeBlock)
            .unwrap_or_else(|| panic!("{name}: code mode"));
        let raw = case["codeBlock"]["rawContent"].as_str().unwrap();
        let expected = if raw.ends_with('\n') {
            raw.to_owned()
        } else {
            format!("{raw}\n")
        };
        assert_eq!(
            read_span(&code, block.logical),
            expected,
            "{name}: exact normalized YAML code content"
        );
        let hidden = parse_extensions(source, Extensions::from_bits(0x500).unwrap());
        assert_eq!(hidden.original_source().text(), source, "{name}");
        assert!(
            !hidden
                .ir()
                .nodes()
                .iter()
                .any(|node| node.kind == NodeKind::Table),
            "{name}: hidden metadata"
        );
    }
}
