// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE file in the project root for license information.
// ---------------------------------------------------------------------------------------------

use super::*;
use crate::config::{Extensions, ParseConfig, Profile};
use crate::error::LimitKind;
use crate::identity::SourceKey;
use crate::ir::{Alignment, DetailRow, FlowScope, NodeKind, ObjectRow};
use crate::policy::SourceBudget;
use crate::source::SourceDocument;

fn parse_model(budget: &SourceBudget, text: &str) -> AcceptedModel {
    parse_model_with_extensions(budget, text, Extensions::NONE)
}

fn parse_model_with_extensions(
    budget: &SourceBudget,
    text: &str,
    extensions: Extensions,
) -> AcceptedModel {
    let mut op = budget.begin(CancellationToken::new());
    let document = SourceDocument::new(
        SourceKey::new([127; 16], 1).unwrap(),
        text.as_bytes(),
        &mut op,
    )
    .unwrap();
    AcceptedModel::parse(
        document.snapshot(),
        ParseConfig::with_extensions(Profile::G3, extensions),
        1,
        &mut op,
    )
    .unwrap()
}

fn key(model: &AcceptedModel, width: f64, indent: f64, gap: f64) -> LayoutKey {
    LayoutKey::new(
        model.identity(),
        1,
        2,
        3,
        width,
        LayoutStyle::new(indent, gap).unwrap(),
    )
    .unwrap()
}

fn new_draft(model: &AcceptedModel, op: &mut Operation) -> Result<LayoutDraft, CoreError> {
    LayoutDraft::new(model.clone(), key(model, 200.0, 16.0, 3.0), op)
}

fn measured(draft: &mut LayoutDraft, heights: &[f64], op: &mut Operation) -> Vec<MeasuredBlock> {
    draft
        .measurement_batch(heights.len(), op)
        .unwrap()
        .unwrap()
        .requests
        .iter()
        .zip(heights)
        .map(|(request, height)| MeasuredBlock {
            node: request.node,
            height: *height,
        })
        .collect()
}

fn complete(
    model: &AcceptedModel,
    width: f64,
    indent: f64,
    gap: f64,
    heights: &[f64],
    op: &mut Operation,
) -> DocumentLayout {
    let layout_key = key(model, width, indent, gap);
    let mut draft = LayoutDraft::new(model.clone(), layout_key, op).unwrap();
    if !heights.is_empty() {
        let measurements = measured(&mut draft, heights, op);
        assert_eq!(measurements.len(), heights.len());
        draft
            .submit_measurements(layout_key, 0, &measurements, op)
            .unwrap();
    }
    draft.finish(op).unwrap()
}

fn released(budget: &SourceBudget) {
    let usage = budget.usage();
    assert_eq!(usage.source_capacity, 0);
    assert_eq!(usage.auxiliary_capacity, 0);
    assert_eq!(usage.source_owners, 0);
}

#[test]
fn review_regression_large_quote_prefix_is_chunked() {
    let budget = SourceBudget::model_fixture();
    let model = parse_model(&budget, &"> quote\n\nseparator\n\n".repeat(800));
    let mut op = budget.begin(CancellationToken::new());
    let mut draft = new_draft(&model, &mut op).unwrap();
    while draft.completed_blocks() < 1400 {
        let batch = draft.measurement_batch(256, &mut op).unwrap().unwrap();
        let (key, start) = (batch.key, batch.start);
        let measurements = batch
            .requests
            .iter()
            .map(|request| MeasuredBlock {
                node: request.node,
                height: 1.0,
            })
            .collect::<Vec<_>>();
        draft
            .submit_measurements(key, start, &measurements, &mut op)
            .unwrap();
    }
    let count = draft.prefix_quote_count(&mut op).unwrap();
    assert!(count >= 700);
    draft.charge_prefix_quotes(count, &mut op).unwrap();
    assert!(draft.prefix_quote(count - 1).unwrap().height > 0.0);
}

fn markdown_layout(model: &AcceptedModel, batch_size: usize, op: &mut Operation) -> DocumentLayout {
    let key = LayoutKey::new(
        model.identity(),
        1,
        2,
        3,
        200.0,
        LayoutStyle::markdown(16.0, 3.0).unwrap(),
    )
    .unwrap();
    let draft = LayoutDraft::new(model.clone(), key, op).unwrap();
    finish_fixed_height_geometry(draft, batch_size, op)
}

fn finish_fixed_height_geometry(
    mut draft: LayoutDraft,
    batch_size: usize,
    op: &mut Operation,
) -> DocumentLayout {
    let key = draft.key();
    while let Some(batch) = draft.measurement_batch(batch_size, op).unwrap() {
        let start = batch.start;
        let metrics = batch
            .requests
            .iter()
            .map(|request| MeasuredBlock {
                node: request.node,
                height: 10.0,
            })
            .collect::<Vec<_>>();
        draft.submit_measurements(key, start, &metrics, op).unwrap();
    }
    draft.finish(op).unwrap()
}

#[test]
fn selected_section_keeps_model_references_and_accepted_heading_identity() {
    let budget = SourceBudget::preview();
    let model = parse_model(
        &budget,
        "# Outside\n\n> Before\n>\n> ## Keep\n>\n> [in section][target]\n>\n> ### Child\n\n## Stop\n\nAfter\n\n[target]: /linked\n",
    );
    let mut op = budget.begin(CancellationToken::new());
    let outline = crate::outline::Outline::build(model.clone(), &mut op).unwrap();
    let [_, first, child, stop] = outline.headings() else {
        panic!("four headings expected")
    };
    let draft = LayoutDraft::section(
        model.clone(),
        key(&model, 200.0, 16.0, 3.0),
        first.node.value(),
        Some(&[]),
        &mut op,
    )
    .unwrap();
    assert_eq!(model.ir().references().len(), 1);
    let requests = draft.requests();
    assert_eq!(requests.len(), 3);
    assert_eq!(requests[0].node, first.node);
    assert_eq!(requests[2].node, child.node);
    assert!(
        requests
            .iter()
            .all(|request| request.logical.start() >= first.logical.start()
                && request.logical.end() <= stop.logical.start())
    );
    assert_eq!(draft.model().identity(), model.identity());
}

#[test]
fn selected_heading_does_not_render_ancestor_markers() {
    let cases = [
        (
            "- Before\n  - ## Keep\n\n    Selected\n\n# Stop\n\nOutside\n",
            Extensions::NONE,
        ),
        (
            "Text[^a]\n\n[^a]: Prefix\n\n    ## Keep\n\n    Selected\n\n    ## Stop\n\n    Outside\n",
            Extensions::FOOTNOTES,
        ),
    ];
    for (source, extensions) in cases {
        let budget = SourceBudget::preview();
        let model = parse_model_with_extensions(&budget, source, extensions);
        let mut op = budget.begin(CancellationToken::new());
        let outline = crate::outline::Outline::build(model.clone(), &mut op).unwrap();
        let first = &outline.headings()[0];
        let draft = LayoutDraft::section(
            model.clone(),
            key(&model, 200.0, 16.0, 3.0),
            first.node.value(),
            Some(&[]),
            &mut op,
        )
        .unwrap();
        let requests = draft.requests();
        assert_eq!(requests.len(), 2);
        assert_eq!(requests[0].kind, BlockKind::Heading);
        assert_eq!(requests[0].node, first.node);
        assert_eq!(requests[1].kind, BlockKind::Paragraph);
    }
}

#[test]
fn stopping_heading_does_not_render_ancestor_markers() {
    for (source, expected) in [
        (
            "# Keep\n\nSelected\n\n- # Stop\n\n  Outside\n",
            &[BlockKind::Heading, BlockKind::Paragraph][..],
        ),
        (
            "# Keep\n\nSelected\n\n- Earlier\n- - # Stop\n\n    Outside\n",
            &[
                BlockKind::Heading,
                BlockKind::Paragraph,
                BlockKind::Paragraph,
            ][..],
        ),
    ] {
        let budget = SourceBudget::preview();
        let model = parse_model(&budget, source);
        let mut op = budget.begin(CancellationToken::new());
        let outline = crate::outline::Outline::build(model.clone(), &mut op).unwrap();
        let [first, _] = outline.headings() else {
            panic!("two headings expected");
        };
        let draft = LayoutDraft::section(
            model.clone(),
            key(&model, 200.0, 16.0, 3.0),
            first.node.value(),
            Some(&[]),
            &mut op,
        )
        .unwrap();
        assert_eq!(
            draft
                .requests()
                .iter()
                .map(|request| request.kind)
                .collect::<Vec<_>>(),
            expected
        );
    }
}

#[test]
fn markdown_spacing_compacts_tight_lists_without_changing_uniform_layout() {
    let budget = SourceBudget::preview();
    let model = parse_model(&budget, "# Intro\n\n- a\n- b\n  - c\n  - d\n- e\n\nAfter");
    let mut op = budget.begin(CancellationToken::new());
    let reading = markdown_layout(&model, 256, &mut op);
    let uniform = complete(&model, 200.0, 16.0, 3.0, &[10.0; 7], &mut op);
    assert_eq!(
        reading
            .placements()
            .iter()
            .map(|row| row.y)
            .collect::<Vec<_>>(),
        [0.0, 13.0, 23.0, 33.0, 43.0, 53.0, 66.0]
    );
    assert_eq!(reading.total_height(), 76.0);
    assert_eq!(
        uniform
            .placements()
            .iter()
            .map(|row| row.y)
            .collect::<Vec<_>>(),
        [0.0, 13.0, 26.0, 39.0, 52.0, 65.0, 78.0]
    );
    assert_eq!(
        reading
            .requests()
            .iter()
            .map(|row| (row.node, row.logical, row.x))
            .collect::<Vec<_>>(),
        uniform
            .requests()
            .iter()
            .map(|row| (row.node, row.logical, row.x))
            .collect::<Vec<_>>()
    );
    assert_ne!(reading.key(), uniform.key());
    drop((reading, uniform, op, model));
    released(&budget);
}

#[test]
fn markdown_spacing_preserves_loose_lists_and_block_boundaries() {
    for (source, expected) in [
        ("- a\n\n- b", vec![0.0, 13.0]),
        (
            "- parent\n  - child a\n  - child b\n- last\n\nTail",
            vec![0.0, 10.0, 20.0, 30.0, 43.0],
        ),
        (
            "- - inner a\n  - inner b\n\n- - inner c\n  - inner d",
            vec![0.0, 13.0, 23.0, 36.0, 49.0, 59.0],
        ),
        (
            "> - a\n> - b\n\n- c\n- d\n\nTail",
            vec![3.0, 13.0, 29.0, 39.0, 52.0],
        ),
        (
            "- parent\n  > - quote a\n  > - quote b\n- last",
            vec![0.0, 16.0, 26.0, 42.0],
        ),
        (
            "- first\n\n  continuation\n\n- second",
            vec![0.0, 13.0, 26.0],
        ),
        (
            "- text\n  ```\n  code\n  ```\n- last",
            vec![0.0, 13.0, 26.0],
        ),
        ("- text\n  ### title\n- last", vec![0.0, 13.0, 26.0]),
        ("-\n- [ ]\n- child", vec![0.0, 10.0, 20.0]),
        ("- a\n- b\n\n1. c\n2. d", vec![0.0, 10.0, 23.0, 33.0]),
    ] {
        let budget = SourceBudget::preview();
        let model = parse_model(&budget, source);
        let mut op = budget.begin(CancellationToken::new());
        let layout = markdown_layout(&model, 256, &mut op);
        assert_eq!(
            layout
                .placements()
                .iter()
                .map(|row| row.y)
                .collect::<Vec<_>>(),
            expected,
            "{source}"
        );
        assert_eq!(
            layout.total_height(),
            expected.last().unwrap() + 10.0,
            "{source}"
        );
        drop((layout, op, model));
        released(&budget);
    }
}

#[test]
fn markdown_spacing_is_independent_of_measurement_batch_boundaries() {
    let budget = SourceBudget::preview();
    let mut source = String::from("# Title\n\n");
    for index in 0..300 {
        source.push_str(&format!("- item {index}\n"));
    }
    source.push_str("\nTail");
    let model = parse_model(&budget, &source);
    let mut op = budget.begin(CancellationToken::new());
    let single = markdown_layout(&model, 1, &mut op);
    let full = markdown_layout(&model, 256, &mut op);
    assert_eq!(single.placements(), full.placements());
    assert_eq!(full.placements()[300].y, 3003.0);
    assert_eq!(full.placements()[301].y, 3016.0);
    assert_eq!(full.total_height(), 3026.0);
    drop((single, full, op, model));
    released(&budget);
}

#[test]
fn actual_g3_tree_keeps_block_order_containers_table_and_alt_scope() {
    let budget = SourceBudget::preview();
    let input = "# title\n\nplain **bold** ![alt *words*](img)\n\n> quoted\n>\n> - item\n>   - nested\n\n```rs\nx\n```\n\n<div>html</div>\n\n---\n\n| a | b |\n| --- | :---: |\n| x | y |\n";
    let model = parse_model(&budget, input);
    let mut op = budget.begin(CancellationToken::new());
    let mut draft = new_draft(&model, &mut op).unwrap();
    let requests = draft
        .measurement_batch(256, &mut op)
        .unwrap()
        .unwrap()
        .requests;
    assert_eq!(
        requests.iter().map(|r| r.kind).collect::<Vec<_>>(),
        [
            BlockKind::Heading,
            BlockKind::Paragraph,
            BlockKind::Paragraph,
            BlockKind::Paragraph,
            BlockKind::Paragraph,
            BlockKind::CodeBlock,
            BlockKind::HtmlBlock,
            BlockKind::ThematicBreak,
            BlockKind::Table,
        ]
    );
    assert_eq!(
        requests.iter().map(|r| r.x).collect::<Vec<_>>(),
        [0.0, 0.0, 16.0, 32.0, 48.0, 0.0, 0.0, 0.0, 0.0]
    );
    assert!(requests.iter().all(|r| r.flow == model.main_flow_id()));
    assert_eq!(requests[4].context.item_depth, 2);
    assert_eq!(requests[4].context.quote_depth, 1);
    assert_eq!(requests[4].available_width, 152.0);
    assert!(
        model
            .ir()
            .flows()
            .iter()
            .any(|flow| { matches!(flow.scope, FlowScope::ImageAlt { .. }) })
    );
    for request in requests {
        let node = model.ir().nodes()[request.node.index()];
        assert_eq!(request.logical, node.logical);
    }
    let table = model.ir().nodes()[requests[8].node.index()];
    assert_eq!(table.kind, NodeKind::Table);
    let DetailRow::Table {
        columns,
        alignments,
        ..
    } = model.ir().details()[table.detail.unwrap().index()]
    else {
        panic!("original table detail missing");
    };
    assert_eq!(columns, 2);
    assert_eq!(
        alignments
            .slice(model.ir().alignments())
            .unwrap()
            .iter()
            .copied()
            .collect::<Vec<_>>(),
        [Alignment::None, Alignment::Center]
    );
    assert_eq!(
        model
            .ir()
            .nodes()
            .iter()
            .filter(|n| n.kind == NodeKind::TableCell)
            .count(),
        4
    );
    drop((draft, op, model));
    released(&budget);
}

#[test]
fn empty_items_and_empty_task_objects_survive_without_fake_paragraphs() {
    for (input, task) in [("-\n", false), ("- [x] ", true)] {
        let budget = SourceBudget::preview();
        let model = parse_model(&budget, input);
        let mut op = budget.begin(CancellationToken::new());
        let mut draft = new_draft(&model, &mut op).unwrap();
        let requests = draft
            .measurement_batch(8, &mut op)
            .unwrap()
            .unwrap()
            .requests;
        assert_eq!(requests.len(), 1, "{input:?}");
        let request = requests[0];
        assert_eq!(request.kind, BlockKind::EmptyListItem);
        assert_eq!(request.context.item_depth, 1);
        assert_eq!(
            model.ir().nodes()[request.node.index()].kind,
            NodeKind::ListItem
        );
        assert_eq!(request.logical.len(), u64::from(task));
        assert!(
            !model
                .ir()
                .nodes()
                .iter()
                .any(|n| n.kind == NodeKind::Paragraph)
        );
        let tasks = model
            .ir()
            .objects()
            .iter()
            .filter(|o| matches!(o, ObjectRow::Task { .. }))
            .count();
        assert_eq!(tasks, usize::from(task));
        if task {
            let ObjectRow::Task { checked, item, .. } = model.ir().objects()[0] else {
                panic!("missing real task object");
            };
            assert!(checked);
            assert_eq!(item, request.node);
        }
        drop((draft, op, model));
        released(&budget);
    }
}

#[test]
fn nested_item_body_does_not_erase_parent_marker_or_duplicate_ordinary_body() {
    for (input, kinds, depths) in [
        (
            "-\n  - child\n",
            vec![BlockKind::EmptyListItem, BlockKind::Paragraph],
            vec![1, 2],
        ),
        (
            "- own\n  - child\n",
            vec![BlockKind::Paragraph, BlockKind::Paragraph],
            vec![1, 2],
        ),
    ] {
        let budget = SourceBudget::preview();
        let model = parse_model(&budget, input);
        let mut op = budget.begin(CancellationToken::new());
        let mut draft = new_draft(&model, &mut op).unwrap();
        let requests = draft
            .measurement_batch(16, &mut op)
            .unwrap()
            .unwrap()
            .requests;
        assert_eq!(requests.iter().map(|r| r.kind).collect::<Vec<_>>(), kinds);
        assert_eq!(
            requests
                .iter()
                .map(|r| r.context.item_depth)
                .collect::<Vec<_>>(),
            depths
        );
        if requests[0].kind == BlockKind::EmptyListItem {
            assert!(requests[0].logical.is_empty());
            assert!(
                !model.ir().nodes()[requests[0].node.index()]
                    .logical
                    .is_empty()
            );
        }
        drop((draft, op, model));
        released(&budget);
    }
}

#[test]
fn explicit_heights_give_independent_vertical_geometry_without_trailing_gap() {
    let budget = SourceBudget::preview();
    let model = parse_model(&budget, "# title\n\n> quote\n\n- list\n\nlast\n");
    let mut op = budget.begin(CancellationToken::new());
    let layout = complete(&model, 100.0, 8.0, 3.0, &[10.0, 20.0, 0.0, 30.0], &mut op);
    assert_eq!(
        layout.placements(),
        &[
            BlockPlacement {
                y: 0.0,
                height: 10.0
            },
            BlockPlacement {
                y: 13.0,
                height: 20.0
            },
            BlockPlacement {
                y: 36.0,
                height: 0.0
            },
            BlockPlacement {
                y: 39.0,
                height: 30.0
            },
        ]
    );
    assert_eq!(layout.total_height(), 69.0);
    assert_eq!(
        layout.requests().iter().map(|r| r.x).collect::<Vec<_>>(),
        [0.0, 8.0, 8.0, 0.0]
    );
    assert_eq!(layout.requests()[1].available_width, 92.0);
    drop((layout, op, model));
    released(&budget);
}

#[test]
fn exact_pending_batch_reborrow_cannot_resize_and_prefixes_advance_once() {
    let budget = SourceBudget::preview();
    let model = parse_model(&budget, "a\n\nb\n\nc\n\nd\n");
    let cancel = CancellationToken::new();
    let mut op = budget.begin(cancel.clone());
    let mut draft = new_draft(&model, &mut op).unwrap();
    assert_eq!(
        draft
            .measurement_batch(3, &mut op)
            .unwrap()
            .unwrap()
            .requests
            .len(),
        3
    );
    assert_eq!(
        draft
            .measurement_batch(1, &mut op)
            .unwrap()
            .unwrap()
            .requests
            .len(),
        3
    );
    let first = measured(&mut draft, &[1.0, 2.0, 3.0], &mut op);
    let key = draft.key();
    // The same flag can be reborrowed by a new operation, without changing task identity.
    let mut next_op = budget.begin(cancel);
    draft
        .submit_measurements(key, 0, &first, &mut next_op)
        .unwrap();
    let last = measured(&mut draft, &[4.0], &mut next_op);
    assert_eq!(
        draft
            .measurement_batch(256, &mut next_op)
            .unwrap()
            .unwrap()
            .start,
        3
    );
    draft
        .submit_measurements(key, 3, &last, &mut next_op)
        .unwrap();
    assert!(draft.measurement_batch(1, &mut next_op).unwrap().is_none());
    let layout = draft.finish(&mut next_op).unwrap();
    assert_eq!(layout.total_height(), 19.0);
    drop((layout, next_op, op, model));
    released(&budget);
}

#[test]
fn incomplete_unborrowed_truncated_repeated_and_reordered_batches_are_rejected() {
    for case in 0..6 {
        let budget = SourceBudget::preview();
        let model = parse_model(&budget, "a\n\nb\n\nc\n");
        let mut op = budget.begin(CancellationToken::new());
        let mut draft = new_draft(&model, &mut op).unwrap();
        let key = draft.key();
        if case == 0 {
            assert!(matches!(
                draft.finish(&mut op),
                Err(CoreError::InvalidState)
            ));
            drop((op, model));
            released(&budget);
            continue;
        }
        if case == 1 {
            assert_eq!(
                draft.submit_measurements(key, 0, &[], &mut op),
                Err(CoreError::InvalidState)
            );
        } else {
            let mut rows = measured(&mut draft, &[1.0, 2.0, 3.0], &mut op);
            let result = match case {
                2 => draft.submit_measurements(key, 0, &rows[..2], &mut op),
                3 => draft.submit_measurements(key, 1, &rows, &mut op),
                4 => {
                    rows.swap(0, 1);
                    draft.submit_measurements(key, 0, &rows, &mut op)
                }
                5 => {
                    draft.submit_measurements(key, 0, &rows, &mut op).unwrap();
                    draft.submit_measurements(key, 0, &rows, &mut op)
                }
                _ => unreachable!(),
            };
            assert!(result.is_err());
        }
        assert!(draft.first_fault().is_some());
        assert!(draft.finish(&mut op).is_err());
        drop((op, model));
        released(&budget);
    }
}

#[test]
fn invalid_last_measurement_is_atomic_and_fresh_operation_cannot_resume() {
    for invalid in [f64::NAN, f64::INFINITY, -1.0] {
        let budget = SourceBudget::preview();
        let model = parse_model(&budget, "a\n\nb\n\nc\n");
        let cancel = CancellationToken::new();
        let mut op = budget.begin(cancel.clone());
        let mut draft = new_draft(&model, &mut op).unwrap();
        let rows = measured(&mut draft, &[10.0, 20.0, invalid], &mut op);
        let key = draft.key();
        assert_eq!(
            draft.submit_measurements(key, 0, &rows, &mut op),
            Err(CoreError::InvalidBoundary)
        );
        assert_eq!(draft.completed_blocks(), 0);
        assert_eq!(draft.total_height, 0.0);
        assert_eq!(draft.pending.unwrap().count, 3);
        let mut fresh = budget.begin(cancel);
        assert!(matches!(
            draft.measurement_batch(1, &mut fresh),
            Err(CoreError::InvalidBoundary)
        ));
        assert_eq!(draft.first_fault(), Some(CoreError::InvalidBoundary));
        assert!(matches!(
            draft.finish(&mut fresh),
            Err(CoreError::InvalidBoundary)
        ));
        drop((fresh, op, model));
        released(&budget);
    }
}

#[test]
fn submit_final_clock_failure_does_not_commit_any_geometry() {
    let budget = SourceBudget::preview();
    let model = parse_model(&budget, "a\n\nb\n\nc\n");
    let mut op = budget.begin(CancellationToken::new());
    let mut draft = new_draft(&model, &mut op).unwrap();
    let rows = measured(&mut draft, &[10.0, 20.0, 30.0], &mut op);
    op.set_clock_samples_for_test(&[4_999_999_999, 5_000_000_000]);
    let result = draft.submit_measurements(draft.key(), 0, &rows, &mut op);
    assert!(matches!(result, Err(CoreError::DeadlineExceeded { .. })));
    assert_eq!(
        op.clock_read_count_for_test(),
        2,
        "entry succeeds; final checkpoint fails"
    );
    assert_eq!(draft.completed_blocks(), 0);
    assert_eq!(draft.total_height, 0.0);
    assert_eq!(draft.pending.unwrap().start, 0);
    drop((draft, op, model));
    released(&budget);
}

#[test]
fn model_width_generations_and_style_values_all_belong_to_the_key() {
    let budget = SourceBudget::preview();
    let model = parse_model(&budget, "body");
    let original = key(&model, 200.0, 16.0, 3.0);
    let config = ParseConfig::new(Profile::G3);
    let other_model =
        crate::identity::ModelIdentity::new(SourceKey::new([12; 16], 7).unwrap(), 9, config)
            .unwrap();
    let variants = [
        LayoutKey::new(other_model, 1, 2, 3, 200.0, original.style()).unwrap(),
        LayoutKey::new(original.model(), 8, 2, 3, 200.0, original.style()).unwrap(),
        LayoutKey::new(original.model(), 1, 8, 3, 200.0, original.style()).unwrap(),
        LayoutKey::new(original.model(), 1, 2, 8, 200.0, original.style()).unwrap(),
        key(&model, 201.0, 16.0, 3.0),
        key(&model, 200.0, 17.0, 3.0),
        key(&model, 200.0, 16.0, 4.0),
    ];
    for variant in variants {
        let mut op = budget.begin(CancellationToken::new());
        let mut draft = new_draft(&model, &mut op).unwrap();
        let rows = measured(&mut draft, &[10.0], &mut op);
        assert_eq!(
            draft.submit_measurements(variant, 0, &rows, &mut op),
            Err(CoreError::StaleResult)
        );
        assert_eq!(draft.completed_blocks(), 0);
    }
    let mut op = budget.begin(CancellationToken::new());
    assert!(matches!(
        LayoutDraft::new(model.clone(), variants[0], &mut op),
        Err(CoreError::StaleResult)
    ));
    drop((op, model));
    released(&budget);
}

#[test]
fn bad_flag_and_budget_stop_draft_and_cannot_be_replaced_to_recover() {
    for wrong_budget in [false, true] {
        let budget = SourceBudget::preview();
        let model = parse_model(&budget, "body");
        let cancel = CancellationToken::new();
        let mut op = budget.begin(cancel.clone());
        let mut draft = new_draft(&model, &mut op).unwrap();
        let other = SourceBudget::preview();
        let mut wrong = if wrong_budget {
            other.begin(cancel.clone())
        } else {
            budget.begin(CancellationToken::new())
        };
        let expected = if wrong_budget {
            CoreError::WrongBudget
        } else {
            CoreError::InvalidState
        };
        assert!(matches!(draft.measurement_batch(1, &mut wrong), Err(error) if error == expected));
        let mut fresh = budget.begin(cancel);
        assert!(matches!(draft.measurement_batch(1, &mut fresh), Err(error) if error == expected));
        assert!(matches!(draft.finish(&mut fresh), Err(error) if error == expected));
        drop((fresh, wrong, op, model));
        released(&budget);
        released(&other);
    }
}

#[test]
fn cancellation_and_prior_typed_fault_never_publish_partial_layout() {
    for typed_first in [false, true] {
        let budget = SourceBudget::preview();
        let model = parse_model(&budget, "a\n\nb\n");
        let cancel = CancellationToken::new();
        let mut op = budget.begin(cancel.clone());
        let mut draft = new_draft(&model, &mut op).unwrap();
        let rows = measured(&mut draft, &[1.0, 2.0], &mut op);
        if typed_first {
            op.fail(CoreError::InvalidRange);
        }
        cancel.cancel();
        let expected = if typed_first {
            CoreError::InvalidRange
        } else {
            CoreError::Cancelled
        };
        assert_eq!(
            draft.submit_measurements(draft.key(), 0, &rows, &mut op),
            Err(expected)
        );
        assert_eq!(draft.completed_blocks(), 0);
        assert!(matches!(draft.finish(&mut op), Err(error) if error == expected));
        drop((op, model));
        released(&budget);
    }
}

#[test]
fn finish_checks_cancellation_after_all_batches_and_install_rechecks_current_context() {
    let budget = SourceBudget::preview();
    let model = parse_model(&budget, "body");
    let cancel = CancellationToken::new();
    let mut op = budget.begin(cancel.clone());
    let mut draft = new_draft(&model, &mut op).unwrap();
    let rows = measured(&mut draft, &[5.0], &mut op);
    draft
        .submit_measurements(draft.key(), 0, &rows, &mut op)
        .unwrap();
    cancel.cancel();
    assert!(matches!(draft.finish(&mut op), Err(CoreError::Cancelled)));
    drop(op);
    let cancel = CancellationToken::new();
    let mut op = budget.begin(cancel.clone());
    let layout = complete(&model, 200.0, 16.0, 3.0, &[5.0], &mut op);
    layout.check_install(layout.key(), &mut op).unwrap();
    let mut other_flag = budget.begin(CancellationToken::new());
    assert_eq!(
        layout.check_install(layout.key(), &mut other_flag),
        Err(CoreError::InvalidState)
    );
    let mut fresh = budget.begin(cancel.clone());
    assert_eq!(
        layout.check_install(key(&model, 201.0, 16.0, 3.0), &mut fresh),
        Err(CoreError::StaleResult)
    );
    cancel.cancel();
    assert_eq!(
        layout.check_install(layout.key(), &mut op),
        Err(CoreError::Cancelled)
    );
    drop((layout, fresh, other_flag, op, model));
    released(&budget);
}

#[test]
fn visible_half_open_ranges_trim_zero_height_edges_and_keep_internal_zeros() {
    let budget = SourceBudget::preview();
    let model = parse_model(&budget, "a\n\nb\n\nc\n\nd\n\ne\n");
    let mut op = budget.begin(CancellationToken::new());
    let layout = complete(
        &model,
        100.0,
        0.0,
        0.0,
        &[0.0, 10.0, 0.0, 20.0, 0.0],
        &mut op,
    );
    for (y, height, overscan, expected) in [
        (0.0, 30.0, 0.0, 1..4),
        (0.0, 10.0, 0.0, 1..2),
        (10.0, 1.0, 0.0, 3..4),
        (9.0, 2.0, 0.0, 1..4),
        (30.0, 1.0, 0.0, 0..0),
        (10.0, 0.0, 100.0, 0..0),
        (10.0, 1.0, 2.0, 1..4),
    ] {
        assert_eq!(
            layout
                .visible_blocks(
                    Viewport {
                        y,
                        height,
                        overscan
                    },
                    &mut op
                )
                .unwrap(),
            expected
        );
    }
    drop((layout, op, model));
    released(&budget);
}

#[test]
fn all_zero_heights_and_gap_only_viewports_are_empty() {
    let budget = SourceBudget::preview();
    let model = parse_model(&budget, "a\n\nb\n\nc\n");
    let mut op = budget.begin(CancellationToken::new());
    let layout = complete(&model, 100.0, 0.0, 5.0, &[0.0, 0.0, 0.0], &mut op);
    assert_eq!(layout.total_height(), 10.0);
    assert_eq!(
        layout
            .visible_blocks(
                Viewport {
                    y: 0.0,
                    height: 100.0,
                    overscan: 0.0
                },
                &mut op
            )
            .unwrap(),
        0..0
    );
    drop(layout);
    let layout = complete(&model, 100.0, 0.0, 5.0, &[10.0, 0.0, 10.0], &mut op);
    assert_eq!(
        layout
            .visible_blocks(
                Viewport {
                    y: 10.0,
                    height: 10.0,
                    overscan: 0.0
                },
                &mut op
            )
            .unwrap(),
        0..0
    );
    drop((layout, op, model));
    released(&budget);
}

#[test]
fn invalid_numbers_and_geometry_overflow_are_typed() {
    for value in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY, -1.0] {
        assert_eq!(
            LayoutStyle::new(value, 0.0),
            Err(CoreError::InvalidBoundary)
        );
        assert_eq!(
            LayoutStyle::new(0.0, value),
            Err(CoreError::InvalidBoundary)
        );
    }
    let budget = SourceBudget::preview();
    let model = parse_model(&budget, "> > body\n");
    for width in [0.0, -1.0, f64::INFINITY, f64::NAN] {
        assert!(matches!(
            LayoutKey::new(
                model.identity(),
                1,
                1,
                1,
                width,
                LayoutStyle::new(0.0, 0.0).unwrap()
            ),
            Err(CoreError::InvalidBoundary)
        ));
    }
    for generations in [(0, 1, 1), (1, 0, 1), (1, 1, 0)] {
        assert!(matches!(
            LayoutKey::new(
                model.identity(),
                generations.0,
                generations.1,
                generations.2,
                100.0,
                LayoutStyle::new(0.0, 0.0).unwrap()
            ),
            Err(CoreError::InvalidRevision)
        ));
    }
    {
        let mut op = budget.begin(CancellationToken::new());
        assert!(matches!(
            LayoutDraft::new(model.clone(), key(&model, f64::MAX, f64::MAX, 0.0), &mut op),
            Err(CoreError::InvalidBoundary)
        ));
    }
    drop(model);
    let model = parse_model(&budget, "a\n\nb\n");
    let mut op = budget.begin(CancellationToken::new());
    let mut draft =
        LayoutDraft::new(model.clone(), key(&model, 100.0, 0.0, f64::MAX), &mut op).unwrap();
    let rows = measured(&mut draft, &[f64::MAX, 0.0], &mut op);
    assert_eq!(
        draft.submit_measurements(draft.key(), 0, &rows, &mut op),
        Err(CoreError::InvalidBoundary)
    );
    assert_eq!(draft.completed_blocks(), 0);
    drop((draft, op, model));
    released(&budget);
}

#[test]
fn narrow_nested_blocks_preserve_body_width_and_semantic_depth() {
    let budget = SourceBudget::preview();
    for input in [
        "> > body\n",
        "> - body\n",
        "> -\n",
        "> > ```\n> > code\n> > ```\n",
        "> > | a |\n> > | - |\n> > | b |\n",
    ] {
        let model = parse_model(&budget, input);
        // The first two rows explicitly replace WR03's old ordinary-width
        // rejection oracle; overflow/nonfinite negatives remain above.
        for (width, indent, expected_x, expected_width) in [
            (20.0, 10.0, 10.0, 10.0),
            (20.0, 11.0, 9.0, 11.0),
            (56.0, 28.0, 28.0, 28.0),
            (57.0, 28.0, 29.0, 28.0),
            (12.0, 28.0, 0.0, 12.0),
            (28.0, 28.0, 0.0, 28.0),
            (800.0, 28.0, 56.0, 744.0),
            (20.0, 0.0, 0.0, 20.0),
        ] {
            let mut op = budget.begin(CancellationToken::new());
            let mut draft =
                LayoutDraft::new(model.clone(), key(&model, width, indent, 0.0), &mut op).unwrap();
            let batch = draft.measurement_batch(256, &mut op).unwrap().unwrap();
            assert_eq!(batch.requests.len(), 1, "{input:?}");
            let request = batch.requests[0];
            assert_eq!(request.x, expected_x, "{input:?}");
            assert_eq!(request.available_width, expected_width, "{input:?}");
            assert_eq!(request.context.quote_depth + request.context.item_depth, 2);
            assert_eq!(request.flow, model.main_flow_id());
            assert_eq!(
                request.logical,
                model.ir().nodes()[request.node.index()].logical
            );
        }
        drop(model);
        released(&budget);
    }
    let model = parse_model(&budget, "- outer\n  - inner\n");
    let mut op = budget.begin(CancellationToken::new());
    let mut draft = LayoutDraft::new(model.clone(), key(&model, 40.0, 28.0, 0.0), &mut op).unwrap();
    let batch = draft.measurement_batch(256, &mut op).unwrap().unwrap();
    assert_eq!(batch.requests.len(), 2);
    for (index, request) in batch.requests.iter().enumerate() {
        assert_eq!(request.context.item_depth, index as u32 + 1);
        assert_eq!(request.x, 12.0);
        assert_eq!(request.available_width, 28.0);
    }
    drop((draft, op, model));
    released(&budget);
}

#[test]
fn nested_body_geometry_remains_positive_at_extreme_finite_scales() {
    let budget = SourceBudget::preview();
    let model = parse_model(&budget, "> > body\n");
    let smallest = f64::from_bits(1);
    for (width, indent, expected_x, expected_width) in [
        (smallest, smallest, 0.0, smallest),
        (smallest * 2.0, smallest, smallest, smallest),
        (smallest, 28.0, 0.0, smallest),
        (smallest, 0.0, 0.0, smallest),
        (f64::MIN_POSITIVE, f64::MIN_POSITIVE, 0.0, f64::MIN_POSITIVE),
        (f64::MAX, 1.0, 2.0, f64::MAX),
        (1.0, smallest, smallest * 2.0, 1.0),
    ] {
        let mut op = budget.begin(CancellationToken::new());
        let mut draft =
            LayoutDraft::new(model.clone(), key(&model, width, indent, 0.0), &mut op).unwrap();
        let batch = draft.measurement_batch(256, &mut op).unwrap().unwrap();
        let request = batch.requests[0];
        assert_eq!(request.x, expected_x);
        assert_eq!(request.available_width, expected_width);
        assert!(request.x.is_finite() && request.x >= 0.0 && request.x < width);
        assert!(request.available_width.is_finite() && request.available_width > 0.0);
    }
    drop(model);
    released(&budget);
}

#[test]
fn viewport_rejects_invalid_parameters_and_floating_point_overflow() {
    let budget = SourceBudget::preview();
    let model = parse_model(&budget, "body");
    let mut op = budget.begin(CancellationToken::new());
    let layout = complete(&model, 100.0, 0.0, 0.0, &[10.0], &mut op);
    for viewport in [
        Viewport {
            y: f64::NAN,
            height: 1.0,
            overscan: 0.0,
        },
        Viewport {
            y: 0.0,
            height: -1.0,
            overscan: 0.0,
        },
        Viewport {
            y: 0.0,
            height: 1.0,
            overscan: f64::INFINITY,
        },
        Viewport {
            y: f64::MAX,
            height: f64::MAX,
            overscan: 0.0,
        },
        Viewport {
            y: f64::MAX,
            height: 0.0,
            overscan: f64::MAX,
        },
    ] {
        let mut query = budget.begin(CancellationToken::new());
        assert_eq!(
            layout.visible_blocks(viewport, &mut query),
            Err(CoreError::InvalidBoundary)
        );
    }
    drop((layout, op, model));
    released(&budget);
}

#[test]
fn empty_document_and_definition_only_document_have_no_layout_blocks() {
    for input in ["", "[ref]: /destination\n"] {
        let budget = SourceBudget::preview();
        let model = parse_model(&budget, input);
        let mut op = budget.begin(CancellationToken::new());
        let layout = complete(&model, 100.0, 0.0, 4.0, &[], &mut op);
        assert!(layout.requests().is_empty());
        assert!(layout.placements().is_empty());
        assert_eq!(layout.total_height(), 0.0);
        assert_eq!(
            layout
                .visible_blocks(
                    Viewport {
                        y: 0.0,
                        height: 100.0,
                        overscan: 100.0
                    },
                    &mut op
                )
                .unwrap(),
            0..0
        );
        drop((layout, op, model));
        released(&budget);
    }
}

#[test]
fn quota_failure_releases_partial_directories_and_layout_pin_retains_the_model() {
    let budget = SourceBudget::preview();
    let model = parse_model(&budget, "body");
    let mut op = budget.begin(CancellationToken::new());
    let baseline = budget.usage();
    let spare = budget.limits().auxiliary_capacity - baseline.auxiliary_capacity;
    let hold = op
        .reserve_boundary(spare - size_of::<BlockRequest>() as u64)
        .unwrap();
    let held = budget.usage().auxiliary_capacity;
    assert!(matches!(
        new_draft(&model, &mut op),
        Err(CoreError::ResourceLimit {
            kind: LimitKind::AuxiliaryCapacity,
            ..
        })
    ));
    assert_eq!(budget.usage().auxiliary_capacity, held);
    drop((hold, op));
    let mut op = budget.begin(CancellationToken::new());
    let baseline = budget.usage();
    let layout = complete(&model, 100.0, 0.0, 0.0, &[10.0], &mut op);
    assert!(budget.usage().auxiliary_capacity > baseline.auxiliary_capacity);
    drop(model);
    assert_eq!(layout.model().source().text(), "body");
    assert!(budget.usage().source_capacity > 0);
    drop((layout, op));
    released(&budget);
}

#[test]
fn invalid_batch_maximum_is_a_permanent_task_error() {
    for maximum in [0, MAX_MEASUREMENT_BATCH + 1] {
        let budget = SourceBudget::preview();
        let model = parse_model(&budget, "body");
        let cancel = CancellationToken::new();
        let mut op = budget.begin(cancel.clone());
        let mut draft = new_draft(&model, &mut op).unwrap();
        assert!(matches!(
            draft.measurement_batch(maximum, &mut op),
            Err(CoreError::InvalidRange)
        ));
        let mut fresh = budget.begin(cancel);
        assert!(matches!(
            draft.finish(&mut fresh),
            Err(CoreError::InvalidRange)
        ));
        drop((fresh, op, model));
        released(&budget);
    }
}

fn markdown_geometry(
    model: &AcceptedModel,
    width: f64,
    indent: f64,
    batch_size: usize,
    op: &mut Operation,
) -> DocumentLayout {
    let key = LayoutKey::new(
        model.identity(),
        1,
        2,
        3,
        width,
        LayoutStyle::markdown(indent, 3.0).unwrap(),
    )
    .unwrap();
    let mut draft = LayoutDraft::new(model.clone(), key, op).unwrap();
    while let Some(batch) = draft.measurement_batch(batch_size, op).unwrap() {
        let start = batch.start;
        let metrics = batch
            .requests
            .iter()
            .map(|request| MeasuredBlock {
                node: request.node,
                height: 10.0,
            })
            .collect::<Vec<_>>();
        draft.submit_measurements(key, start, &metrics, op).unwrap();
    }
    draft.finish(op).unwrap()
}

#[test]
fn quote_and_list_ancestry_preserves_each_container_edge_and_owning_marker() {
    let budget = SourceBudget::preview();
    let model = parse_model(
        &budget,
        "> - list\n>   > nested\n>\n> tail\n\n- > list quote\n\n- ordinary\n",
    );
    let mut op = budget.begin(CancellationToken::new());
    let layout = markdown_geometry(&model, 600.0, 28.0, 1, &mut op);
    assert_eq!(
        layout
            .requests()
            .iter()
            .map(|r| (
                r.context.quote_depth,
                r.context.item_depth,
                r.x,
                r.available_width,
                r.marker_x
            ))
            .collect::<Vec<_>>(),
        [
            (1, 1, 43.0, 541.0, 15.0),
            (2, 1, 58.0, 510.0, 15.0),
            (1, 0, 15.0, 569.0, 0.0),
            (1, 1, 43.0, 541.0, 0.0),
            (0, 1, 28.0, 572.0, 0.0),
        ]
    );
    assert_eq!(
        layout
            .quotes()
            .iter()
            .map(|q| (q.depth, q.x, q.width, q.border_width))
            .collect::<Vec<_>>(),
        [
            (1, 0.0, 600.0, 5.0),
            (2, 43.0, 541.0, 5.0),
            (1, 28.0, 572.0, 5.0),
        ]
    );
    drop((layout, op, model));
    released(&budget);
}

#[test]
fn quote_rectangles_fill_internal_gaps_but_keep_sibling_identity_and_external_gap() {
    let budget = SourceBudget::preview();
    let model = parse_model(
        &budget,
        "> first\n>\n> - item\n>\n> ```text\n> code\n> ```\n>\n> last\n\noutside\n\n> sibling\n",
    );
    let mut op = budget.begin(CancellationToken::new());
    let single = markdown_geometry(&model, 600.0, 28.0, 1, &mut op);
    let full = markdown_geometry(&model, 600.0, 28.0, 256, &mut op);
    assert_eq!(single.requests(), full.requests());
    assert_eq!(single.quotes(), full.quotes());
    assert_eq!(full.quotes().len(), 2);
    let first = full.quotes()[0];
    let sibling = full.quotes()[1];
    assert_ne!(first.node, sibling.node);
    assert_eq!((first.y, first.height), (0.0, 55.0));
    assert_eq!((sibling.y, sibling.height), (71.0, 16.0));
    assert_eq!(full.placements()[0].y - first.y, 3.0);
    assert_eq!(first.y + first.height - full.placements()[3].bottom(), 3.0);
    assert_eq!(full.placements()[4].y - (first.y + first.height), 3.0);
    assert!(first.y + first.height < full.placements()[4].y);
    // A viewport wholly in an internal gap has no leaf body. Its quote
    // rectangle remains available independently of the visible block range.
    assert_eq!(
        full.visible_blocks(
            Viewport {
                y: 14.0,
                height: 1.0,
                overscan: 0.0,
            },
            &mut op
        )
        .unwrap(),
        0..0
    );
    assert!(first.y <= 14.0 && first.y + first.height >= 15.0);
    drop((single, full, op, model));
    released(&budget);
}

#[test]
fn quote_padding_tracks_each_nested_edge_and_preserves_generic_geometry() {
    let budget = SourceBudget::preview();
    let model = parse_model(&budget, "> > first\n>\n> tail\n\noutside\n");
    let mut op = budget.begin(CancellationToken::new());
    let reading = markdown_geometry(&model, 600.0, 28.0, 1, &mut op);
    let generic = complete(&model, 600.0, 28.0, 3.0, &[10.0; 3], &mut op);
    assert_eq!(
        reading
            .placements()
            .iter()
            .map(|row| row.y)
            .collect::<Vec<_>>(),
        [6.0, 22.0, 38.0]
    );
    assert_eq!(
        reading
            .quotes()
            .iter()
            .map(|row| (row.y, row.height))
            .collect::<Vec<_>>(),
        [(0.0, 35.0), (3.0, 16.0)]
    );
    assert_eq!(reading.total_height(), 48.0);
    assert_eq!(
        generic
            .placements()
            .iter()
            .map(|row| row.y)
            .collect::<Vec<_>>(),
        [0.0, 13.0, 26.0]
    );
    assert_eq!(
        generic
            .quotes()
            .iter()
            .map(|row| (row.y, row.height))
            .collect::<Vec<_>>(),
        [(0.0, 23.0), (0.0, 10.0)]
    );
    assert_eq!(generic.total_height(), 36.0);
    drop((reading, generic, op, model));
    released(&budget);
}

#[test]
fn quote_padding_prefix_includes_closed_children_and_keeps_completed_edges_stable() {
    let budget = SourceBudget::preview();
    let model = parse_model(&budget, "> > first\n>\n> tail\n\noutside\n");
    let mut op = budget.begin(CancellationToken::new());
    let key = LayoutKey::new(
        model.identity(),
        1,
        2,
        3,
        600.0,
        LayoutStyle::markdown(28.0, 3.0).unwrap(),
    )
    .unwrap();
    let mut draft = LayoutDraft::new(model.clone(), key, &mut op).unwrap();
    let mut closed_inner = None;
    let mut closed_outer = None;
    for (index, total) in [19.0, 35.0, 48.0].into_iter().enumerate() {
        let batch = draft.measurement_batch(1, &mut op).unwrap().unwrap();
        let measured = [MeasuredBlock {
            node: batch.requests[0].node,
            height: 10.0,
        }];
        draft
            .submit_measurements(key, index, &measured, &mut op)
            .unwrap();
        assert_eq!(draft.prefix_height(), total);
        assert_eq!(draft.prefix_quote_count(&mut op).unwrap(), 2);
        let outer = draft.prefix_quote(0).unwrap();
        let inner = draft.prefix_quote(1).unwrap();
        if index == 0 {
            assert_eq!((outer.y, outer.height), (0.0, 19.0));
            assert_eq!((inner.y, inner.height), (3.0, 16.0));
            closed_inner = Some(inner);
        } else {
            assert_eq!(Some(inner), closed_inner);
            assert_eq!((outer.y, outer.height), (0.0, 35.0));
            closed_outer = Some(outer);
        }
    }
    let layout = draft.finish(&mut op).unwrap();
    assert_eq!(Some(layout.quotes()[0]), closed_outer);
    assert_eq!(Some(layout.quotes()[1]), closed_inner);
    drop((layout, op, model));
    released(&budget);
}

#[test]
fn nested_directive_details_keep_equal_padding_when_closed_or_open() {
    let budget = SourceBudget::preview();
    for initial in ["", " {open}"] {
        let source = format!(
            ":::: info Outer\n::: details Fold me{initial}\nInner body.\n:::\n::::\n\nOutside\n"
        );
        let model = parse_model_with_extensions(&budget, &source, Extensions::CALLOUTS);
        let mut op = budget.begin(CancellationToken::new());
        let region = model.disclosure_regions()[0];
        for expanded in [false, true] {
            let key = LayoutKey::new(
                model.identity(),
                1,
                2,
                3,
                600.0,
                LayoutStyle::markdown(28.0, 3.0).unwrap(),
            )
            .unwrap();
            let switches = [DisclosureOverride {
                node: region.node.value(),
                expanded: u32::from(expanded),
            }];
            let draft =
                LayoutDraft::with_disclosures(model.clone(), key, &switches, &mut op).unwrap();
            let layout = finish_fixed_height_geometry(draft, 1, &mut op);
            assert_eq!(layout.requests().len(), if expanded { 4 } else { 3 });
            let [outer, inner] = layout.quotes() else {
                panic!("two nested containers expected")
            };
            let bodies = layout.placements();
            assert_eq!(bodies[0].y - outer.y, 3.0);
            assert_eq!(bodies[1].y - inner.y, 3.0);
            assert_eq!(
                inner.y + inner.height - bodies[bodies.len() - 2].bottom(),
                3.0
            );
            assert_eq!(outer.y + outer.height - (inner.y + inner.height), 3.0);
            assert_eq!(bodies.last().unwrap().y - (outer.y + outer.height), 3.0);
            assert_eq!(inner.height, if expanded { 29.0 } else { 16.0 });
            assert_eq!(layout.total_height(), bodies.last().unwrap().bottom());
            assert_eq!(layout.disclosures()[0].expanded, expanded);
            drop(layout);
        }
        drop((op, model));
        released(&budget);
    }
}

#[test]
fn loose_item_continuation_code_and_quote_first_tasks_keep_their_item_gutter() {
    let budget = SourceBudget::preview();
    let model = parse_model(
        &budget,
        "- parent\n\n  continuation\n\n  ```\n  code\n  ```\n\n  > quote\n\n- [x] \n\n  > task quote\n\n- last\n",
    );
    let mut op = budget.begin(CancellationToken::new());
    let layout = markdown_geometry(&model, 600.0, 28.0, 256, &mut op);
    assert_eq!(
        layout
            .requests()
            .iter()
            .map(|r| (r.x, r.marker_x))
            .collect::<Vec<_>>(),
        [
            (28.0, 0.0),
            (28.0, 0.0),
            (28.0, 0.0),
            (43.0, 0.0),
            (43.0, 0.0),
            (28.0, 0.0),
        ]
    );
    assert_eq!(layout.requests()[2].kind, BlockKind::CodeBlock);
    assert_eq!(
        layout.quotes().iter().map(|q| q.x).collect::<Vec<_>>(),
        [28.0, 28.0]
    );
    drop((layout, op, model));
    released(&budget);
}

#[test]
fn empty_quote_nodes_have_zero_height_without_synthetic_body_requests() {
    let budget = SourceBudget::preview();
    let model = parse_model(&budget, ">\n\n> >\n\nbody\n");
    let mut op = budget.begin(CancellationToken::new());
    let layout = markdown_geometry(&model, 600.0, 28.0, 256, &mut op);
    assert_eq!(layout.requests().len(), 1);
    assert_eq!(layout.quotes().len(), 3);
    assert!(
        layout
            .quotes()
            .iter()
            .all(|q| q.y == 0.0 && q.height == 0.0)
    );
    assert_eq!(
        layout.quotes().iter().map(|q| q.depth).collect::<Vec<_>>(),
        [1, 1, 2]
    );
    drop((layout, op, model));
    released(&budget);
}

#[test]
fn empty_nested_quote_ranges_add_no_padding_to_their_nonempty_parent() {
    let budget = SourceBudget::preview();
    let model = parse_model(&budget, "> >\n>\n> body\n>\n> >\n\noutside\n");
    let mut op = budget.begin(CancellationToken::new());
    let layout = markdown_geometry(&model, 600.0, 28.0, 1, &mut op);
    assert_eq!(layout.requests().len(), 2);
    assert_eq!(layout.quotes().len(), 3);
    assert_eq!(
        (layout.quotes()[0].y, layout.quotes()[0].height),
        (0.0, 16.0)
    );
    assert!(layout.quotes()[1..].iter().all(|row| row.height == 0.0));
    assert_eq!(layout.placements()[0].y, 3.0);
    assert_eq!(layout.placements()[1].y, 19.0);
    assert_eq!(layout.total_height(), 29.0);
    drop((layout, op, model));
    released(&budget);
}

#[test]
fn narrow_quotes_follow_real_ancestry_and_keep_positive_nested_rectangles() {
    let budget = SourceBudget::preview();
    for source in [
        "> > > body\n",
        "- > > body\n",
        "> - > body\n",
        "> > - body\n",
        "- - > body\n",
    ] {
        let model = parse_model(&budget, source);
        for width in [64.0, 16.0, 4.0, 0.5, f64::from_bits(1)] {
            for indent in [28.0, 0.0] {
                let mut op = budget.begin(CancellationToken::new());
                let layout = markdown_geometry(&model, width, indent, 1, &mut op);
                let body = layout.requests().last().unwrap();
                assert!(body.x.is_finite() && body.x >= 0.0);
                assert!(body.available_width.is_finite() && body.available_width > 0.0);
                assert!(body.x + body.available_width <= width, "{source}, {width}");
                let mut parent_left = 0.0;
                let mut parent_right = width;
                for quote in layout.quotes() {
                    assert!(quote.width.is_finite() && quote.width > 0.0);
                    assert!(quote.x >= parent_left);
                    assert!(quote.x + quote.width <= parent_right);
                    assert!(quote.border_width >= 0.0 && quote.border_width <= 5.0);
                    assert!(quote.border_width <= quote.width);
                    parent_left = quote.x;
                    parent_right = quote.x + quote.width;
                }
                assert!(body.x >= parent_left);
                assert!(body.x + body.available_width <= parent_right);
            }
        }
        drop(model);
        released(&budget);
    }
}

#[test]
fn deep_quote_branch_does_not_shrink_its_siblings_or_change_logical_mapping() {
    let budget = SourceBudget::preview();
    let source = format!("> 中文 😀\n\n{}深 😀\n\n> last\n", "> ".repeat(64));
    let model = parse_model(&budget, &source);
    let mut op = budget.begin(CancellationToken::new());
    let reading = markdown_geometry(&model, 600.0, 28.0, 1, &mut op);
    let generic = complete(&model, 600.0, 28.0, 3.0, &[10.0; 3], &mut op);
    assert_eq!(reading.quotes().len(), 66);
    assert_eq!(reading.requests()[0].x, 15.0);
    assert_eq!(reading.requests()[2].x, 15.0);
    assert_eq!(reading.requests()[0].available_width, 569.0);
    assert_eq!(reading.requests()[2].available_width, 569.0);
    assert_eq!(
        reading
            .requests()
            .iter()
            .map(|r| (r.node, r.kind, r.flow, r.logical, r.context))
            .collect::<Vec<_>>(),
        generic
            .requests()
            .iter()
            .map(|r| (r.node, r.kind, r.flow, r.logical, r.context))
            .collect::<Vec<_>>()
    );
    for row in reading.requests() {
        assert!(row.available_width >= 28.0);
    }
    drop((reading, generic, op, model));
    released(&budget);
}

#[test]
fn quote_directory_crosses_batches_and_allocation_failure_releases_every_reservation() {
    let budget = SourceBudget::preview();
    let source = "> body\n\n".repeat(300);
    let model = parse_model(&budget, &source);
    let mut op = budget.begin(CancellationToken::new());
    let layout = markdown_geometry(&model, 600.0, 28.0, 256, &mut op);
    assert_eq!(layout.quotes().len(), 300);
    for (quote, body) in layout.quotes().iter().zip(layout.placements()) {
        assert_eq!((quote.y + 3.0, quote.height - 6.0), (body.y, body.height));
    }
    drop(layout);
    let baseline = budget.usage();
    let spare = budget.limits().auxiliary_capacity - baseline.auxiliary_capacity;
    let hold = op
        .reserve_boundary(
            spare - (300 * (size_of::<BlockRequest>() + size_of::<BlockPlacement>())) as u64,
        )
        .unwrap();
    let held = budget.usage().auxiliary_capacity;
    assert!(matches!(
        new_draft(&model, &mut op),
        Err(CoreError::ResourceLimit {
            kind: LimitKind::AuxiliaryCapacity,
            ..
        })
    ));
    assert_eq!(budget.usage().auxiliary_capacity, held);
    drop((hold, op, model));
    released(&budget);
}
