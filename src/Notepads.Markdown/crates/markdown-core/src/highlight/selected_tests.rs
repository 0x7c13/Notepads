// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE file in the project root for license information.
// ---------------------------------------------------------------------------------------------

use super::*;
use crate::config::{ParseConfig, Profile};
use crate::identity::SourceKey;
use crate::ir::NodeKind;
use crate::policy::{CancellationToken, SourceBudget};
use crate::source::SourceDocument;

fn model(root: &SourceBudget, text: &str, revision: u64) -> AcceptedModel {
    let mut op = root.begin(CancellationToken::new());
    let source = SourceDocument::new(
        SourceKey::new([213; 16], revision).unwrap(),
        text.as_bytes(),
        &mut op,
    )
    .unwrap();
    AcceptedModel::parse(source.snapshot(), ParseConfig::new(Profile::G3), 1, &mut op).unwrap()
}

fn nodes(model: &AcceptedModel) -> Vec<u32> {
    model
        .ir()
        .nodes()
        .iter()
        .enumerate()
        .filter(|(_, node)| node.kind == NodeKind::CodeBlock && node.flow == model.main_flow_id())
        .map(|(index, _)| index as u32 + 1)
        .collect()
}

fn selected(model: AcceptedModel, nodes: &[u32], cache: &mut HighlightCache) -> HighlightSnapshot {
    let mut op = model.budget().begin(CancellationToken::new());
    HighlightSnapshot::build_selected(model, nodes, cache, &mut op).unwrap()
}

#[test]
fn selected_only_builds_requested_blocks_beyond_legacy_limit() {
    let root = SourceBudget::preview_without_deadline_for_test();
    let source: String = (0..70)
        .map(|index| format!("```rust\nlet value = {index};\n```\n\n"))
        .collect();
    let model = model(&root, &source, 1);
    let ids = nodes(&model);
    let mut cache = HighlightCache::new(&model);
    let result = selected(model.clone(), &ids[69..], &mut cache);
    assert_eq!(result.rows().len(), 1);
    assert_eq!(result.rows()[0].node.value(), ids[69]);
    assert_eq!(result.summary().complete_blocks, 1);
    assert_eq!(result.summary().reused_blocks, 0);
    let again = selected(model, &ids[69..], &mut cache);
    assert_eq!(again.summary().reused_blocks, 1);
    assert_eq!(again.spans(), result.spans());
}

#[test]
fn selected_cache_reuses_exact_content_after_alias_position_and_revision_change() {
    let root = SourceBudget::preview_without_deadline_for_test();
    let first = model(&root, "```rs\nlet text = \"你好🌍\";\n```\n", 1);
    let mut cache = HighlightCache::new(&first);
    let original = selected(first.clone(), &nodes(&first), &mut cache);
    let second = model(
        &root,
        "A new paragraph.\n\n```rust extra\nlet text = \"你好🌍\";\n```\n",
        2,
    );
    let reused = selected(second.clone(), &nodes(&second), &mut cache);
    assert_eq!(reused.summary().reused_blocks, 1);
    assert_eq!(reused.spans(), original.spans());
    assert_ne!(reused.rows()[0].logical, original.rows()[0].logical);
    assert_eq!(reused.identity(), second.identity());
    let changed = model(&root, "```rust\nlet text = \"你好🦀\";\n```\n", 3);
    assert_eq!(
        selected(changed.clone(), &nodes(&changed), &mut cache)
            .summary()
            .reused_blocks,
        0
    );
    let language = model(&root, "```js\nlet text = \"你好🌍\";\n```\n", 4);
    assert_eq!(
        selected(language.clone(), &nodes(&language), &mut cache)
            .summary()
            .reused_blocks,
        0
    );
}

#[test]
fn selected_cache_does_not_pin_obsolete_source_owners() {
    let root = SourceBudget::preview_without_deadline_for_test();
    let first = model(&root, "```rust\nlet value = 42;\n```\n", 1);
    let mut cache = HighlightCache::new(&first);
    let result = selected(first.clone(), &nodes(&first), &mut cache);
    drop(result);
    drop(first);
    assert_eq!(root.usage().source_owners, 0);
    assert_eq!(root.usage().source_capacity, 0);
    assert!(root.usage().auxiliary_capacity > 0);
    drop(cache);
    assert_eq!(root.usage().auxiliary_capacity, 0);
}

#[test]
fn selected_rejects_invalid_nodes_and_foreign_cache() {
    let root = SourceBudget::preview_without_deadline_for_test();
    let model = model(&root, "A paragraph.\n\n```rust\n42\n```\n", 1);
    let ids = nodes(&model);
    let mut cache = HighlightCache::new(&model);
    for invalid in [
        vec![0],
        vec![u32::MAX],
        vec![ids[0], ids[0]],
        vec![1],
        vec![ids[0]; MAX_BLOCKS + 1],
    ] {
        let mut op = root.begin(CancellationToken::new());
        assert!(
            HighlightSnapshot::build_selected(model.clone(), &invalid, &mut cache, &mut op)
                .is_err()
        );
    }
    let other_root = SourceBudget::preview_without_deadline_for_test();
    let other = self::model(&other_root, "```rust\n42\n```\n", 1);
    let mut foreign = HighlightCache::new(&other);
    let mut op = root.begin(CancellationToken::new());
    assert!(matches!(
        HighlightSnapshot::build_selected(model, &ids, &mut foreign, &mut op),
        Err(CoreError::WrongBudget)
    ));
}

#[test]
fn selected_does_not_cache_skips_or_cancelled_work() {
    let root = SourceBudget::preview_without_deadline_for_test();
    let model = model(&root, "```rust\n42\n```\n\n```unknown\n42\n```\n", 1);
    let ids = nodes(&model);
    let mut cache = HighlightCache::new(&model);
    let first = selected(model.clone(), &ids, &mut cache);
    assert_eq!(first.summary().complete_blocks, 1);
    assert_eq!(first.summary().skipped_blocks, 1);
    assert_eq!(cache.len(), 1);
    let token = CancellationToken::new();
    token.cancel();
    let mut op = root.begin(token);
    let mut cancelled_cache = HighlightCache::new(&model);
    assert!(matches!(
        HighlightSnapshot::build_selected(model.clone(), &ids, &mut cancelled_cache, &mut op),
        Err(CoreError::Cancelled)
    ));
    assert_eq!(cancelled_cache.len(), 0);
    let retry = selected(model.clone(), &ids, &mut cache);
    assert_eq!(retry.summary().reused_blocks, 1);
    assert_eq!(retry.summary().skipped_blocks, 1);
    assert_eq!(cache.len(), 1);
    assert!(selected(model, &[], &mut cache).rows().is_empty());
}

#[test]
fn optional_cache_quota_rejection_keeps_computed_snapshot_and_request_healthy() {
    let root = SourceBudget::preview_without_deadline_for_test();
    let code = "let answer = [1, 2, 3, 4];\n";
    let mut source: String = (0..MAX_BLOCKS)
        .map(|index| format!("```rust\n{index}\n```\n\n"))
        .collect();
    source.push_str(&format!("```rust\n{code}```\n"));
    let model = model(&root, &source, 1);
    let ids = nodes(&model);
    let mut cache = HighlightCache::new(&model);
    assert_eq!(
        selected(model.clone(), &ids[..MAX_BLOCKS], &mut cache)
            .summary()
            .complete_blocks,
        MAX_BLOCKS as u32
    );
    assert_eq!(cache.len(), MAX_BLOCKS);
    let retained = cache.retained_bytes();
    // Fund exactly the selected row, temporary code, and initial token capacity.
    // The retained token copy has no remaining root quota and must be omitted.
    // Its admission must preserve the full cache, including the oldest entry.
    let needed = size_of::<HighlightBlock>() as u64
        + code.len() as u64
        + (64 * size_of::<TokenSpan>()) as u64;
    let remaining = root.limits().auxiliary_capacity - root.usage().auxiliary_capacity;
    let filler = root.reserve_resource_capacity(remaining - needed).unwrap();
    let mut op = root.begin(CancellationToken::new());
    let result =
        HighlightSnapshot::build_selected(model.clone(), &ids[MAX_BLOCKS..], &mut cache, &mut op)
            .unwrap();
    assert_eq!(op.first_fault(), None);
    assert_eq!(result.summary().status, SnapshotStatus::Complete);
    assert_eq!(result.summary().complete_blocks, 1);
    assert_eq!(cache.len(), MAX_BLOCKS);
    assert_eq!(cache.retained_bytes(), retained);
    drop(filler);
    assert_eq!(
        selected(model.clone(), &ids[..MAX_BLOCKS], &mut cache)
            .summary()
            .reused_blocks,
        MAX_BLOCKS as u32
    );
    assert_eq!(
        selected(model.clone(), &ids[MAX_BLOCKS..], &mut cache)
            .summary()
            .reused_blocks,
        0
    );
    assert_eq!(
        selected(model, &ids[MAX_BLOCKS..], &mut cache)
            .summary()
            .reused_blocks,
        1
    );
}

#[test]
fn selected_cache_retains_only_bounded_complete_entries() {
    let root = SourceBudget::preview_without_deadline_for_test();
    let source: String = (0..70)
        .map(|index| {
            format!(
                "```rust\n// {index}\n{}```\n\n",
                "// small retained cache line\n".repeat(600)
            )
        })
        .collect();
    let model = model(&root, &source, 1);
    let mut cache = HighlightCache::new(&model);
    let ids = nodes(&model);
    for &id in &ids {
        assert_eq!(
            selected(model.clone(), &[id], &mut cache)
                .summary()
                .complete_blocks,
            1
        );
        assert!(cache.retained_bytes() <= MAX_CACHE_BYTES);
        assert!(cache.len() <= MAX_BLOCKS);
    }
    assert_eq!(
        selected(model.clone(), &ids[69..], &mut cache)
            .summary()
            .reused_blocks,
        1
    );
    assert_eq!(
        selected(model, &ids[..1], &mut cache)
            .summary()
            .reused_blocks,
        0
    );
}
