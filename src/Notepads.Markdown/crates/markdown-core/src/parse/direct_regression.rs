// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE file in the project root for license information.
// ---------------------------------------------------------------------------------------------

use super::*;
use crate::identity::{ModelIdentity, SourceKey};
use crate::mapping::{
    Affinity, Correspondence, PointInput, PointQuery, Position, RangeInput, RangeQuery,
};
use crate::policy::{CancellationToken, Operation, SourceBudget, construction_scope};
use crate::source::SourceDocument;

fn build(
    input: &ParseInput,
    config: ParseConfig,
    op: &mut Operation,
) -> Result<CompactIr, CoreError> {
    construction_scope(op, |mut work| {
        let ir = crate::parse::build(input, config, &mut work).map_err(|error| work.fail(error))?;
        work.finish()?;
        Ok(ir)
    })
}

fn receive_released(budget: &SourceBudget) {
    assert_eq!(budget.usage().source_capacity, 0);
    assert_eq!(budget.usage().auxiliary_capacity, 0);
    assert_eq!(budget.usage().source_owners, 0);
}

fn actual(text: &str) -> (ParseInput, CompactIr, Operation) {
    let budget = SourceBudget::model_fixture();
    let mut op = budget.begin(CancellationToken::new());
    let source = SourceDocument::new(
        SourceKey::new([7; 16], 1).unwrap(),
        text.as_bytes(),
        &mut op,
    )
    .unwrap()
    .snapshot();
    let input = ParseInput::prepare(source, &mut op).unwrap();
    let ir = build(&input, ParseConfig::new(Profile::C0), &mut op).unwrap_or_else(|e| {
        if text.len() < 256 {
            panic!("build {text:?}: {e:?}");
        }
        panic!("build {} bytes: {e:?}", text.len());
    });
    let built = op.stats();
    if let Err(error) = crate::ir::validate::validate(&ir, input.source(), &mut op) {
        eprintln!(
            "original resource regression: built={built:?} failed={:?}",
            op.stats()
        );
        for (i, row) in ir.nodes().iter().enumerate() {
            eprintln!(
                "node{i}: {:?} {:?} {:?}",
                row.kind, row.source_envelope, row.logical
            );
        }
        for row in ir.attributes() {
            eprintln!(
                "attribute {row:?}: {:?}",
                row.sources.slice(ir.sources()).unwrap()
            );
        }
        for row in ir.details() {
            eprintln!("detail {row:?}");
        }
        for row in ir.proofs() {
            eprintln!(
                "proof {row:?}: {:?} {:?}",
                row.primitives.slice(ir.primitives()).unwrap(),
                row.stages.slice(ir.stages()).unwrap()
            );
        }
        panic!("validate {} bytes: {error:?}", text.len());
    }
    (input, ir, op)
}
fn atom_views(ir: &CompactIr, rows: Rows<AtomRow>, op: &mut Operation) -> Vec<AtomRow> {
    (rows.start()..rows.start() + rows.len())
        .map(|index| ir.atom_at(index, op).unwrap())
        .collect()
}
fn logical(input: &ParseInput, ir: &CompactIr, flow: usize, op: &mut Operation) -> String {
    let row = ir.flows()[flow];
    let mut result = String::new();
    for atom in atom_views(ir, row.atoms, op) {
        match atom.kind {
            AtomKind::Copy => {
                let source = atom.sources.slice(ir.sources()).unwrap()[0];
                result.push_str(
                    &input.source().text()[source.start() as usize..source.end() as usize],
                );
            }
            AtomKind::Transform | AtomKind::Synthetic => {
                let AtomContent::Pool(value) = atom.content else {
                    panic!()
                };
                result.push_str(ir.text(value).unwrap());
            }
            AtomKind::Object => result.push('\u{fffc}'),
            AtomKind::Hidden => {}
        }
    }
    result
}

#[test]
fn real_strong_entity_cjk_astral_and_mapping() {
    let (input, ir, mut op) = actual("**中&amp;😀**");
    assert_eq!(logical(&input, &ir, 0, &mut op), "中&😀");
    assert_eq!(ir.flows()[0].logical_utf16, 4);
    assert_eq!(ir.flows()[0].atoms.len(), 5);
    let identity =
        ModelIdentity::new(input.source().key(), 1, ParseConfig::new(Profile::C0)).unwrap();
    let index = crate::mapping::MapIndex::build(&ir, input.source(), identity, &mut op).unwrap();
    for (affinity, expected) in [(Affinity::Before, 1), (Affinity::After, 2)] {
        let query = PointQuery {
            model: identity,
            input: PointInput::Source {
                target: TextTarget::Flow(FlowId::from_index(0).unwrap()),
                byte: crate::identity::SourceBoundary::new(7),
            },
            affinity,
        };
        let result = index
            .map_point(&ir, input.source(), &query, &mut op)
            .unwrap();
        assert_eq!(
            result.results()[0].chosen,
            Position::Flow(crate::identity::LogicalBoundary::new(expected))
        );
    }
}
#[test]
fn real_multi_entity_and_adjacent_entities_are_minimal() {
    let (input, ir, mut op) = actual("&NotEqualTilde;");
    assert_eq!(logical(&input, &ir, 0, &mut op), "≂\u{0338}");
    assert_eq!(ir.proofs().len(), 1);
    let (_, ir, _) = actual("&amp;&amp;");
    assert_eq!(ir.proofs().len(), 2);
}
#[test]
fn real_crlf_soft_break_has_normalization_and_decoder_closure() {
    let (input, ir, mut op) = actual("a\r\nb");
    assert_eq!(logical(&input, &ir, 0, &mut op), "a b");
    assert!(
        ir.primitives()
            .iter()
            .any(|p| p.kind == PrimitiveKind::CrLf)
    );
    assert_eq!(ir.stages().len(), 1);
    assert_eq!(ir.stages()[0].kind, StageKind::SoftBreakToSpace);
}
#[test]
fn real_compound_image_and_empty_alt_have_own_permissions() {
    let (input, ir, mut op) = actual("x ![**中** &amp; `😀`](u \"t\") y");
    assert_eq!(logical(&input, &ir, 0, &mut op), "x \u{fffc} y");
    assert_eq!(logical(&input, &ir, 1, &mut op), "中 & 😀");
    assert_eq!(ir.payloads().len(), 2);
    let (input, ir, mut op) = actual("![](u)");
    assert_eq!(logical(&input, &ir, 0, &mut op), "\u{fffc}");
    assert!(ir.flows()[1].atoms.is_empty());
    assert!(
        ir.flows()[1]
            .domains
            .slice(ir.sources())
            .unwrap()
            .iter()
            .eq([SourceSpan::new(2, 2).unwrap()].iter())
    );
}

#[test]
fn real_nested_image_queue_preserves_main_alt_values_sources_and_object_bijection() {
    let text = "before *a **b** c* ![outer ![inner](v) tail](u) ![](empty) after\n\nend";
    let (input, ir, mut op) = actual(text);
    assert_eq!(
        logical(&input, &ir, 0, &mut op),
        "before a b c \u{fffc} \u{fffc} after\nend"
    );
    assert_eq!(ir.objects().len(), 3);
    assert_eq!(ir.flows().len(), 4);
    for (index, object) in ir.objects().iter().enumerate() {
        let ObjectRow::Image {
            node,
            alt,
            destination,
            ..
        } = *object
        else {
            panic!()
        };
        let url = ir.text(ir.payloads()[destination.index()].value).unwrap();
        let (expected, label) = match url {
            "u" => ("outer \u{fffc} tail", "outer ![inner](v) tail"),
            "v" => ("inner", "inner"),
            "empty" => ("", ""),
            _ => panic!("unexpected image destination {url}"),
        };
        assert_eq!(logical(&input, &ir, alt.index(), &mut op), expected);
        assert_eq!(ir.nodes()[node.index()].kind, NodeKind::Image);
        assert_eq!(ir.nodes()[node.index()].first_child, None);
        let flow = ir.flows()[alt.index()];
        assert_eq!(
            flow.scope,
            FlowScope::ImageAlt {
                object: ObjectId::from_index(index).unwrap()
            }
        );
        let root = ir.nodes()[flow.root.index()];
        assert_eq!(root.kind, NodeKind::AltRoot);
        assert_eq!(root.parent, None);
        let start = if label.is_empty() {
            text.find("![](empty)").unwrap() + 2
        } else {
            text.find(label).unwrap()
        };
        assert!(
            flow.domains
                .slice(ir.sources())
                .unwrap()
                .iter()
                .eq([SourceSpan::new(start as u64, (start + label.len()) as u64).unwrap()].iter())
        );
        if url == "v" {
            assert_ne!(ir.nodes()[node.index()].flow.index(), 0);
        } else {
            assert_eq!(ir.nodes()[node.index()].flow.index(), 0);
        }
    }
    drop(ir);
    drop(input);
    receive_released(op.budget());
}

#[test]
fn real_ordinary_small_groups_keep_cropped_text_values_origins_hidden_and_full_acceptance() {
    // Exact one-block recipe of the frozen ordinary workload, independently expected below.
    let group =
        "# Title\n\n中文 **bold** [link](https://example.org) and `code`.\n\n- first\n- second\n\n";
    assert_eq!(group.len(), 84);
    for count in [1, 2] {
        let text = group.repeat(count);
        let budget = SourceBudget::model_fixture();
        let mut op = budget.begin(CancellationToken::new());
        let source = SourceDocument::new(
            SourceKey::new([5; 16], 1).unwrap(),
            text.as_bytes(),
            &mut op,
        )
        .unwrap();
        let model = crate::model::AcceptedModel::parse(
            source.snapshot(),
            ParseConfig::new(Profile::C0),
            1,
            &mut op,
        )
        .unwrap();
        let input = ParseInput::prepare(source.snapshot(), &mut op).unwrap();
        let ir = model.ir();
        let expected = vec!["Title\n中文 bold link and code.\nfirst\nsecond"; count].join("\n");
        assert_eq!(
            logical(&input, ir, model.main_flow_id().index(), &mut op),
            expected
        );
        assert_eq!(
            ir.details()
                .iter()
                .filter(|detail| matches!(detail, DetailRow::Item { .. }))
                .count(),
            2 * count
        );
        let atoms = atom_views(ir, ir.flows()[model.main_flow_id().index()].atoms, &mut op);
        for block in 0..count {
            for literal in ["Title", "bold", "link", "code", "first", "second"] {
                let start = block * group.len() + group.find(literal).unwrap();
                let span = SourceSpan::new(start as u64, (start + literal.len()) as u64).unwrap();
                let atom = atoms
                    .iter()
                    .find(|atom| {
                        atom.kind == AtomKind::Copy
                            && atom
                                .sources
                                .slice(ir.sources())
                                .unwrap()
                                .iter()
                                .eq([span].iter())
                    })
                    .unwrap_or_else(|| panic!("missing copy {literal} at {span:?}"));
                let owner = ir.nodes()[atom.owner.index()];
                if literal == "bold" || literal == "link" {
                    assert_eq!(owner.source_envelope, span);
                }
                if literal == "code" {
                    assert_eq!(owner.kind, NodeKind::InlineCode);
                }
                let point = model
                    .map_point(
                        &PointQuery {
                            model: model.identity(),
                            input: PointInput::Source {
                                target: TextTarget::Flow(model.main_flow_id()),
                                byte: crate::identity::SourceBoundary::new(start as u64 + 1),
                            },
                            affinity: Affinity::After,
                        },
                        &mut op,
                    )
                    .unwrap();
                assert_eq!(point.results()[0].correspondence, Correspondence::Exact);
            }
            let marker = block * group.len() + group.find("**bold").unwrap();
            let point = model
                .map_point(
                    &PointQuery {
                        model: model.identity(),
                        input: PointInput::Source {
                            target: TextTarget::Flow(model.main_flow_id()),
                            byte: crate::identity::SourceBoundary::new(marker as u64 + 1),
                        },
                        affinity: Affinity::After,
                    },
                    &mut op,
                )
                .unwrap();
            assert_eq!(point.results()[0].correspondence, Correspondence::Hidden);
        }
        assert_eq!(
            ir.payloads()
                .iter()
                .filter(|payload| payload.role == PayloadRole::Destination
                    && ir.text(payload.value).unwrap() == "https://example.org")
                .count(),
            count
        );
        assert_eq!(op.first_fault(), None);
        drop(input);
        drop(model);
        drop(source);
        receive_released(&budget);
    }
}
#[test]
fn real_reference_definition_empty_and_definition_only() {
    let (input, ir, mut op) = actual("[x][id]\n\n[id]: /u");
    assert_eq!(logical(&input, &ir, 0, &mut op), "x");
    assert_eq!(ir.definitions().len(), 1);
    assert_eq!(ir.references().len(), 1);
    assert!(ir.definitions()[0].winner);
    let (_, ir, _) = actual("[id]: /u");
    assert_eq!(ir.flows()[0].logical_utf16, 0);
    assert!(!ir.flows()[0].atoms.is_empty());
    let (_, ir, _) = actual("");
    assert!(ir.flows()[0].atoms.is_empty());
}
#[test]
fn real_metadata_closures_and_block_attributes() {
    for text in [
        r#"![a&amp;b](<u\&amp;x> "t&amp;\!")"#,
        r#"[x](u "&bsol;&ast;")"#,
        "# h ##\r\n\r\n> p\n\n- q\n\n``` lang&amp;\\!\ncode\n```\n",
        "    a\n",
        "` a `",
        "<foo bar =\n \"hi\"> \n",
    ] {
        actual(text);
    }
}

#[test]
fn real_c0_structures_and_incomplete_inputs_keep_native_semantics() {
    for text in [
        "\u{feff}#\n",
        "> a\n> b\n",
        "- \n",
        "0. a\n1. b\n",
        "a\n---\n",
        "***x***",
        "a  \nb",
        "a\\\nb",
        "<https://example.com>",
        "<a@b.com>",
        "![[x](u)](v)",
        "[x](u \"\")",
        "[x]()",
        "unfinished [`a *",
        "a\0b\r\nc",
        "    a\n\np\n",
        "    a",
        ">\tcode\n",
    ] {
        actual(text);
    }
}

#[test]
fn real_reference_raw_label_and_sole_first_winner_are_distinct() {
    let (input, ir, mut op) = actual("[A&amp;]: /first\n[A&amp;]: /later\n\n[x][A&amp;]");
    assert_eq!(logical(&input, &ir, 0, &mut op), "x");
    assert_eq!(ir.definitions().len(), 2);
    assert!(ir.definitions()[0].winner);
    assert!(!ir.definitions()[1].winner);
    let label = ir.payloads()[ir.definitions()[0].label.index()];
    assert_eq!(ir.text(label.value).unwrap(), "A&amp;");
    assert_eq!(ir.text(ir.definitions()[0].key).unwrap(), "a&amp;");
    assert!(
        atom_views(&ir, label.atoms, &mut op)
            .iter()
            .all(|a| a.kind == AtomKind::Copy)
    );
}

#[test]
fn real_body_never_redecodes_entity_output() {
    for (text, expected) in [
        ("&bsol;*", "\\*"),
        ("&amp;amp;", "&amp;"),
        ("&#xE000; &#xD800; &#xDFFF;", "\u{e000} \u{fffd} \u{fffd}"),
    ] {
        let (input, ir, mut op) = actual(text);
        assert_eq!(logical(&input, &ir, 0, &mut op), expected);
    }
}

#[test]
fn actual_large_plain_and_cancelled_build_use_the_same_root() {
    let text = "a".repeat(1_048_576);
    let (input, ir, mut op) = actual(&text);
    assert_eq!(ir.flows()[0].logical_utf16, text.len() as u64);
    assert_eq!(logical(&input, &ir, 0, &mut op), text);
    let budget = SourceBudget::model_fixture();
    let cancel = CancellationToken::new();
    let mut op = budget.begin(cancel.clone());
    let source = SourceDocument::new(SourceKey::new([8; 16], 1).unwrap(), b"text", &mut op)
        .unwrap()
        .snapshot();
    let input = ParseInput::prepare(source, &mut op).unwrap();
    let baseline = budget.usage();
    cancel.cancel();
    assert!(matches!(
        build(&input, ParseConfig::new(Profile::C0), &mut op),
        Err(CoreError::Cancelled)
    ));
    assert_eq!(
        budget.usage().auxiliary_capacity,
        baseline.auxiliary_capacity
    );
    drop(input);
    assert_eq!(budget.usage().auxiliary_capacity, 0);
    assert_eq!(budget.usage().source_capacity, 0);
}

#[test]
fn real_c0_indent_blank_html_reference_and_code_consumption() {
    let mut failures = Vec::new();
    for text in [
        "   # h\n",
        "   p\n",
        "  > p\n",
        "  - p\n",
        "  ```\nx\n  ```\n",
        " \t \n",
        "a\n   b\n",
        "a\n\n   b\n",
        "   ---\n",
        "# h \t\n",
        "a \t\n",
        "a\n\n",
        ">\n",
        "> \n>\n",
        "- a\n  b\n",
        "- a\n\n  b\n",
        "1.\n",
        "a\n==== \t\n",
        "``\n`\n``",
        "`a\r\nb`",
        "    a\n    \n    b\n",
        "<div>\n  x\n</div>\n",
        "<!--x-->\n",
        "a <b>x</b> y",
        "[id]: /u 't'\n\n[x][id]",
        "[id]: /u\n  \"t\"\n\n[id]",
        "[foo]: /url\n'[foo]'bar\n\n[foo]",
        "[x](  /u  )",
        "[x](<>)",
        "![a\nb](u)",
        "[a](u 't\\!')",
        "a\\*b",
        "\\&amp;",
    ] {
        if std::panic::catch_unwind(|| actual(text)).is_err() {
            failures.push(text);
        }
    }
    assert!(
        failures.is_empty(),
        "native consumption failures: {failures:?}"
    );
}

#[test]
fn native_quota_failure_publishes_nothing() {
    use crate::policy::{ModelLimits, SourceLimits};
    let budget = SourceBudget::new_with_model_limits(
        SourceLimits::default(),
        ModelLimits {
            native_records: 3,
            ..ModelLimits::default()
        },
    )
    .unwrap();
    let mut op = budget.begin(CancellationToken::new());
    let source = SourceDocument::new(SourceKey::new([9; 16], 2).unwrap(), b"**x**", &mut op)
        .unwrap()
        .snapshot();
    let input = ParseInput::prepare(source, &mut op).unwrap();
    let baseline = budget.usage().auxiliary_capacity;
    assert!(matches!(
        build(&input, ParseConfig::new(Profile::C0), &mut op),
        Err(CoreError::ResourceLimit {
            kind: LimitKind::NativeRecords,
            ..
        })
    ));
    assert!(matches!(
        op.first_fault(),
        Some(CoreError::ResourceLimit {
            kind: LimitKind::NativeRecords,
            ..
        })
    ));
    assert_eq!(budget.usage().auxiliary_capacity, baseline);
    drop(input);
    assert_eq!(budget.usage().source_capacity, 0);
    assert_eq!(budget.usage().auxiliary_capacity, 0);
}

#[test]
fn real_empty_definition_and_alt_mapping_preserve_permissions() {
    use crate::identity::{LogicalBoundary, SourceBoundary};
    for (text, flow, anchor) in [("", 0, 0), ("![](u)", 1, 2)] {
        let (input, ir, mut op) = actual(text);
        let identity =
            ModelIdentity::new(input.source().key(), 1, ParseConfig::new(Profile::C0)).unwrap();
        let index =
            crate::mapping::MapIndex::build(&ir, input.source(), identity, &mut op).unwrap();
        let flow = FlowId::from_index(flow).unwrap();
        for (point, chosen) in [
            (
                PointInput::Flow {
                    flow,
                    utf16: LogicalBoundary::new(0),
                },
                anchor,
            ),
            (
                PointInput::Source {
                    target: TextTarget::Flow(flow),
                    byte: SourceBoundary::new(anchor),
                },
                0,
            ),
        ] {
            let result = index
                .map_point(
                    &ir,
                    input.source(),
                    &PointQuery {
                        model: identity,
                        input: point,
                        affinity: Affinity::Before,
                    },
                    &mut op,
                )
                .unwrap();
            assert_eq!(result.results()[0].chosen.value(), chosen);
            assert_eq!(result.results()[0].correspondence, Correspondence::Exact);
            assert!(result.contributors().is_empty());
        }
        let mut negative = input.source().budget().begin(CancellationToken::new());
        let query = RangeQuery {
            model: identity,
            input: RangeInput::Source {
                target: TextTarget::Flow(flow),
                span: SourceSpan::new(anchor, anchor + 1).unwrap(),
            },
            empty_affinity: Affinity::Before,
        };
        let expected = if text.is_empty() {
            CoreError::InvalidRange
        } else {
            CoreError::OutsideScope
        };
        assert!(
            matches!(index.map_range(&ir, input.source(), &query, &mut negative), Err(error) if error == expected)
        );
    }
    let (input, ir, mut op) = actual("[id]: /u");
    let identity =
        ModelIdentity::new(input.source().key(), 1, ParseConfig::new(Profile::C0)).unwrap();
    let index = crate::mapping::MapIndex::build(&ir, input.source(), identity, &mut op).unwrap();
    let query = PointQuery {
        model: identity,
        input: PointInput::Source {
            target: TextTarget::Flow(FlowId::from_index(0).unwrap()),
            byte: SourceBoundary::new(4),
        },
        affinity: Affinity::After,
    };
    let result = index
        .map_point(&ir, input.source(), &query, &mut op)
        .unwrap();
    assert_eq!(result.results()[0].chosen.value(), 0);
    assert_eq!(result.results()[0].correspondence, Correspondence::Hidden);
    assert!(!result.contributors().is_empty());
}

#[test]
fn real_entity_range_seam_and_unicode_boundary_queries() {
    use crate::identity::{LogicalBoundary, SourceBoundary};
    let (input, ir, mut op) = actual("&NotEqualTilde;");
    let identity =
        ModelIdentity::new(input.source().key(), 1, ParseConfig::new(Profile::C0)).unwrap();
    let index = crate::mapping::MapIndex::build(&ir, input.source(), identity, &mut op).unwrap();
    let flow = FlowId::from_index(0).unwrap();
    for (affinity, source) in [(Affinity::Before, 0), (Affinity::After, 15)] {
        let point = PointQuery {
            model: identity,
            input: PointInput::Flow {
                flow,
                utf16: LogicalBoundary::new(1),
            },
            affinity,
        };
        let result = index
            .map_point(&ir, input.source(), &point, &mut op)
            .unwrap();
        assert_eq!(result.results()[0].chosen.value(), source);
        assert_eq!(result.results()[0].correspondence, Correspondence::Atomic);
    }
    let query = RangeQuery {
        model: identity,
        input: RangeInput::Flow {
            flow,
            span: LogicalSpan::new(0, 1).unwrap(),
        },
        empty_affinity: Affinity::Before,
    };
    let result = index
        .map_range(&ir, input.source(), &query, &mut op)
        .unwrap();
    assert!(result.selections()[0].expanded);
    assert_eq!(
        result.contributions()[0].source,
        SourceSpan::new(0, 15).unwrap()
    );

    let (input, ir, mut op) = actual("&amp;&amp;");
    let identity =
        ModelIdentity::new(input.source().key(), 1, ParseConfig::new(Profile::C0)).unwrap();
    let index = crate::mapping::MapIndex::build(&ir, input.source(), identity, &mut op).unwrap();
    let query = PointQuery {
        model: identity,
        input: PointInput::Source {
            target: TextTarget::Flow(flow),
            byte: SourceBoundary::new(5),
        },
        affinity: Affinity::Before,
    };
    let result = index
        .map_point(&ir, input.source(), &query, &mut op)
        .unwrap();
    assert_eq!(result.results()[0].correspondence, Correspondence::Exact);
    assert_eq!(result.contributors().len(), 2);

    let (input, ir, mut op) = actual("**中&amp;😀**");
    let identity =
        ModelIdentity::new(input.source().key(), 1, ParseConfig::new(Profile::C0)).unwrap();
    let index = crate::mapping::MapIndex::build(&ir, input.source(), identity, &mut op).unwrap();
    for input_query in [
        PointInput::Flow {
            flow,
            utf16: LogicalBoundary::new(3),
        },
        PointInput::Source {
            target: TextTarget::Flow(flow),
            byte: SourceBoundary::new(3),
        },
    ] {
        let mut negative = input.source().budget().begin(CancellationToken::new());
        assert!(matches!(
            index.map_point(
                &ir,
                input.source(),
                &PointQuery {
                    model: identity,
                    input: input_query,
                    affinity: Affinity::Before
                },
                &mut negative
            ),
            Err(CoreError::InvalidBoundary)
        ));
    }
}

#[test]
fn real_large_fenced_eof_has_native_terminal_policy() {
    let text = format!("```\n{}", "x".repeat(262_144));
    let (input, ir, mut op) = actual(&text);
    let rendered = logical(&input, &ir, 0, &mut op);
    assert_eq!(rendered.len(), 262_145);
    assert!(rendered.ends_with('\n'));
    assert!(rendered[..262_144].bytes().all(|b| b == b'x'));
    assert!(
        atom_views(&ir, Rows::new(0, ir.atoms_len()).unwrap(), &mut op)
            .iter()
            .any(|a| a.kind == AtomKind::Synthetic)
    );
}

#[test]
fn real_many_list_items_and_alt_targets_keep_finite_transport() {
    for text in ["```", "```\n```", "```\n```\n", "    ", "``a\nb``", "`\n`"] {
        actual(text);
    }
    let (_, ir, _) = actual(&"- a\n".repeat(4_096));
    assert_eq!(
        ir.details()
            .iter()
            .filter(|d| matches!(d, DetailRow::Item { .. }))
            .count(),
        4_096
    );
    let (_, ir, _) = actual(&"![a](u) ".repeat(1_024));
    assert_eq!(ir.flows().len(), 1_025);
    assert_eq!(ir.objects().len(), 1_024);
}

#[test]
fn real_long_decoder_chains_have_bounded_growth_and_tail_links() {
    fn measured(text: &str, expected: &str) -> u64 {
        let budget = SourceBudget::model_fixture();
        let mut op = budget.begin(CancellationToken::new());
        let source = SourceDocument::new(
            SourceKey::new([8; 16], 1).unwrap(),
            text.as_bytes(),
            &mut op,
        )
        .unwrap()
        .snapshot();
        let input = ParseInput::prepare(source, &mut op).unwrap();
        let before = op.stats().expect("statistics enabled");
        let ir = build(&input, ParseConfig::new(Profile::C0), &mut op).unwrap();
        let finished = op.stats().expect("statistics enabled");
        eprintln!(
            "{}B direct-native-final-compiler: scan={} copy={} events={} total={}",
            text.len(),
            finished.scanned_bytes() - before.scanned_bytes(),
            finished.copied_bytes() - before.copied_bytes(),
            finished.events() - before.events(),
            finished.total() - before.total(),
        );
        crate::ir::validate::validate(&ir, input.source(), &mut op).unwrap();
        let validated = op.stats().expect("statistics enabled");
        eprintln!(
            "{}B independent-validation: scan={} copy={} events={} total={}",
            text.len(),
            validated.scanned_bytes() - finished.scanned_bytes(),
            validated.copied_bytes() - finished.copied_bytes(),
            validated.events() - finished.events(),
            validated.total() - finished.total()
        );
        assert_eq!(logical(&input, &ir, 0, &mut op), expected);
        let work = finished.total() - before.total();
        drop(ir);
        drop(input);
        assert_eq!(budget.usage().source_capacity, 0);
        assert_eq!(budget.usage().auxiliary_capacity, 0);
        work
    }
    let mut prior = None;
    for count in [1_024, 2_048, 4_096, 8_192, 16_384, 32_768] {
        let body = format!("a{}", "\nb".repeat(count));
        let text = format!("`{body}`");
        let work = measured(&text, &body.replace('\n', " "));
        if let Some(previous) = prior {
            assert!(work < previous * 5 / 2);
        }
        prior = Some(work);
    }
    let title = "&bsol;&ast;".repeat(1_024);
    let text = format!("[x](u \"{title}\")");
    let (input, ir, mut op) = actual(&text);
    let title = ir
        .payloads()
        .iter()
        .find(|p| p.role == PayloadRole::Title)
        .unwrap();
    assert_eq!(ir.text(title.value).unwrap(), "*".repeat(1_024));
    assert_eq!(logical(&input, &ir, 0, &mut op), "x");
    for (text, expected) in [
        ("*` a\r\nb `* z", "a b z"),
        ("x``\n`\n``y", "x`y"),
        ("` a `&amp;z", "a&z"),
        ("a` b `z", "abz"),
    ] {
        let (input, ir, mut op) = actual(text);
        assert_eq!(logical(&input, &ir, 0, &mut op), expected);
    }
}

#[test]
fn source_only_budget_rejection_latches_and_releases() {
    let budget = SourceBudget::source_fixture();
    let mut op = budget.begin(CancellationToken::new());
    let source = SourceDocument::new(SourceKey::new([9; 16], 1).unwrap(), b"x", &mut op)
        .unwrap()
        .snapshot();
    let input = ParseInput::prepare(source, &mut op).unwrap();
    let baseline = budget.usage().auxiliary_capacity;
    assert!(matches!(
        build(&input, ParseConfig::new(Profile::C0), &mut op),
        Err(CoreError::UnsupportedConfiguration)
    ));
    assert_eq!(op.first_fault(), Some(CoreError::UnsupportedConfiguration));
    assert_eq!(budget.usage().auxiliary_capacity, baseline);
    drop(input);
    assert_eq!(budget.usage().auxiliary_capacity, 0);
    assert_eq!(budget.usage().source_capacity, 0);
}

#[test]
fn real_metadata_expansion_uses_native_output_coordinates() {
    let cases = [
        ("[x](&nGt;)", PayloadRole::Destination, "\u{226b}\u{20d2}"),
        ("[x](u \"&nGt;\")", PayloadRole::Title, "\u{226b}\u{20d2}"),
        (
            "[r]: &nGt;\n\n[x][r]",
            PayloadRole::Destination,
            "\u{226b}\u{20d2}",
        ),
        (
            "``` &nGt;\nbody\n```",
            PayloadRole::FenceInfo,
            "\u{226b}\u{20d2}",
        ),
        (
            "[x](u \"中😀&nGt;&amp;&nGt;后\")",
            PayloadRole::Title,
            "中😀\u{226b}\u{20d2}&\u{226b}\u{20d2}后",
        ),
        (
            "[x](&nGt;&#65;&amp;&bsol;&ast;)",
            PayloadRole::Destination,
            "\u{226b}\u{20d2}A&*",
        ),
        (
            "[x](u \"&amp;&nGt;&bsol;&ast;\")",
            PayloadRole::Title,
            "&\u{226b}\u{20d2}*",
        ),
        (
            "``` &Tab;&nGt;&bsol;&ast;&Tab;\nbody\n```",
            PayloadRole::FenceInfo,
            "\u{226b}\u{20d2}*",
        ),
        (
            "[&nGt;]: /u\n\n[x][&nGt;]",
            PayloadRole::ReferenceLabel,
            "&nGt;",
        ),
        ("[x](u \"\")", PayloadRole::Title, ""),
        ("[x](&amp;)", PayloadRole::Destination, "&"),
    ];
    for (text, role, expected) in cases {
        let budget = SourceBudget::model_fixture();
        let mut op = budget.begin(CancellationToken::new());
        let source = SourceDocument::new(
            SourceKey::new([9; 16], 1).unwrap(),
            text.as_bytes(),
            &mut op,
        )
        .unwrap();
        // The public path includes actual native build, independent validation and MapIndex.
        let model = crate::model::AcceptedModel::parse(
            source.snapshot(),
            ParseConfig::new(Profile::C0),
            1,
            &mut op,
        )
        .unwrap_or_else(|error| panic!("metadata {text:?}: {error:?}"));
        let ir = model.ir();
        let payload = ir.payloads().iter().find(|p| p.role == role).unwrap();
        assert_eq!(ir.text(payload.value).unwrap(), expected, "{text:?}");
        assert_eq!(payload.utf8_len, expected.len() as u64);
        if expected.contains('\u{226b}') {
            assert!(ir.stages().iter().any(|s| s.kind == StageKind::NamedEntity));
            for (start, _) in text.match_indices("&nGt;") {
                let expected_source = SourceSpan::new(start as u64, (start + 5) as u64).unwrap();
                assert!(atom_views(ir, payload.atoms, &mut op).iter().any(|atom| {
                    matches!(atom.content, AtomContent::Pool(value)
                        if ir.text(value).unwrap() == "\u{226b}\u{20d2}")
                        && atom
                            .sources
                            .slice(ir.sources())
                            .unwrap()
                            .iter()
                            .eq([expected_source].iter())
                }));
            }
        }
        if text.contains("&bsol;&ast;") {
            assert!(ir.stages().iter().any(|s| s.kind == StageKind::AsciiEscape));
        }
        if text.contains("&Tab;") {
            assert!(ir.stages().iter().any(|s| s.kind == StageKind::AsciiTrim));
        }
        if role == PayloadRole::ReferenceLabel {
            assert_eq!(ir.text(ir.definitions()[0].key).unwrap(), "&ngt;");
            assert!(!ir.stages().iter().any(|s| s.kind == StageKind::NamedEntity));
        }
        assert_eq!(op.first_fault(), None);
        drop(model);
        drop(source);
        assert_eq!(budget.usage().source_capacity, 0);
        assert_eq!(budget.usage().auxiliary_capacity, 0);
    }
}

#[test]
fn reference_only_setext_candidate_keeps_original_buffer_coordinates() {
    use crate::identity::{LogicalBoundary, SourceBoundary};
    use crate::mapping::RangeResult;
    use crate::model::AcceptedModel;

    for text in [
        "[foo]: /url\n===\n[foo]\n",
        "[foo]: /url\r\n===\r\n[foo]\r\n",
        "[foo]: /url\n[bar]: /other\n===\n[foo]\n",
        "> [foo]: /url\n> ===\n> [foo]\n",
        "- [foo]: /url\n  ===\n \t[foo]\n",
    ] {
        for task4 in [false, true] {
            let budget = if task4 {
                SourceBudget::task_fixture()
            } else {
                SourceBudget::model_fixture()
            };
            let mut op = budget.begin(CancellationToken::new());
            let source = SourceDocument::new(
                SourceKey::new([57; 16], 1).unwrap(),
                text.as_bytes(),
                &mut op,
            )
            .unwrap();
            let model =
                AcceptedModel::parse(source.snapshot(), ParseConfig::new(Profile::C0), 1, &mut op)
                    .unwrap_or_else(|error| panic!("{text:?} task4={task4}: {error:?}"));
            assert!(
                !model
                    .ir()
                    .nodes()
                    .iter()
                    .any(|row| row.kind == NodeKind::Heading)
            );
            assert_eq!(
                model.ir().definitions().len(),
                if text.contains("[bar]") { 2 } else { 1 }
            );
            let ir = model.ir();
            assert_eq!(ir.references().len(), 1);
            let reference = ir.references()[0];
            let winner = ir.definitions()[reference.winner.index()];
            assert!(winner.winner);
            for destination in [reference.destination, winner.destination] {
                assert_eq!(
                    ir.text(ir.payloads()[destination.index()].value).unwrap(),
                    "/url"
                );
            }
            let flow = model.main_flow_id();
            let start = text.find("===").unwrap() as u64;
            for (input, chosen) in [
                (
                    PointInput::Source {
                        target: TextTarget::Flow(flow),
                        byte: SourceBoundary::new(start + 1),
                    },
                    Position::Flow(LogicalBoundary::new(1)),
                ),
                (
                    PointInput::Flow {
                        flow,
                        utf16: LogicalBoundary::new(2),
                    },
                    Position::Source(SourceBoundary::new(start + 2)),
                ),
            ] {
                let result = model
                    .map_point(
                        &PointQuery {
                            model: model.identity(),
                            input,
                            affinity: Affinity::Before,
                        },
                        &mut op,
                    )
                    .unwrap();
                assert_eq!(result.results()[0].chosen, chosen);
                assert_eq!(result.results()[0].correspondence, Correspondence::Exact);
            }
            let result = model
                .map_range(
                    &RangeQuery {
                        model: model.identity(),
                        input: RangeInput::Source {
                            target: TextTarget::Flow(flow),
                            span: SourceSpan::new(start, start + 3).unwrap(),
                        },
                        empty_affinity: Affinity::Before,
                    },
                    &mut op,
                )
                .unwrap();
            let [RangeResult::Selection(rows)] = result.results() else {
                panic!()
            };
            let [selection] = rows.slice(result.selections()).unwrap() else {
                panic!()
            };
            assert!(!selection.expanded);
            assert_eq!(
                selection.start.chosen,
                Position::Flow(LogicalBoundary::new(0))
            );
            assert_eq!(
                selection.end.chosen,
                Position::Flow(LogicalBoundary::new(3))
            );
            drop(result);
            drop(model);
            drop(source);
            drop(op);
            receive_released(&budget);
        }
    }
}
