// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE file in the project root for license information.
// ---------------------------------------------------------------------------------------------
//! Actual owner boundaries, not complete font/raster or final SVG qualification.

use super::owner::*;
use super::*;
use crate::geometry_test_support::{native, request, Call};
use crate::ProcessingContext;
use markdown_svg_budget::FailureKind;
use std::cell::Cell;
use std::sync::Arc;

fn processing(r: &markdown_svg_budget::ResourcesRef) -> ProcessingContext {
    ProcessingContext::with_resources(r.clone())
}

#[test]
fn some_zst_is_rejected_before_admission_allocation_and_factory_none_keeps_zst() {
    let (r, state) = request(Default::default());
    let p = processing(&r);
    let calls = Cell::new(0);
    let result = crate::scene::boxed(&p, crate::scene::SceneCost::NODE, || {
        calls.set(calls.get() + 1);
    });
    assert_eq!(result.unwrap_err().kind, FailureKind::Invariant);
    assert_eq!(calls.get(), 0);
    assert_eq!(state.counts().reserve, 0);
    assert_eq!(r.statistics().expanded_nodes, 0);
    drop(p);
    r.validate_retired().unwrap();
    drop(
        crate::scene::boxed(
            &ProcessingContext::default(),
            crate::scene::SceneCost::NODE,
            || {
                calls.set(calls.get() + 1);
            },
        )
        .unwrap(),
    );
    assert_eq!(calls.get(), 1);
}

#[test]
fn actual_unique_box_reserve_and_commit_faults_stop_before_factory() {
    for call in [Call::Reserve, Call::Commit] {
        let (r, state) = request(Default::default());
        let p = processing(&r);
        let calls = Cell::new(0);
        state.arm(call, 1, native(-22101));
        let result = SceneBox::try_new_with(&p, || {
            calls.set(calls.get() + 1);
            Ok(17u64)
        });
        assert_eq!(result.unwrap_err(), native(-22101));
        assert_eq!(calls.get(), 0);
        assert_eq!(state.live_len(), 0);
        assert_eq!(p.resource_failure(), Some(native(-22101)));
        drop(p);
        r.validate_retired().unwrap();
    }
}

#[test]
fn unique_box_header_retirement_failure_never_publishes_the_taken_payload() {
    let (r, state) = request(Default::default());
    let p = processing(&r);
    let value = SceneBox::try_new_with(&p, || Ok(19u64)).unwrap();
    assert_eq!(*value, 19);
    state.arm(Call::Release, 1, native(-22102));
    let result = value.try_take(&p);
    let mut consumers = 0;
    if result.is_ok() {
        consumers += 1;
    }
    assert_eq!(consumers, 0);
    assert_eq!(result.unwrap_err(), native(-22102));
    assert_eq!(state.live_len(), 1);
    assert_eq!(r.statistics().release_failures, 1);
    assert!(r.validate_retired().is_err());
}

const SVG: &[u8] = b"<svg xmlns='http://www.w3.org/2000/svg' width='16' height='16'><g id='g'><path id='p' d='M0 0L8 0L8 8Z'/></g></svg>";

fn parse(r: Option<&markdown_svg_budget::ResourcesRef>) -> (Tree, crate::Options<'static>) {
    let mut opt = crate::Options::default();
    if let Some(r) = r {
        opt.processing = Arc::new(processing(r));
    }
    let tree = Tree::from_data_in_request(SVG, &opt).unwrap();
    (tree, opt)
}

#[test]
fn whole_tree_none_same_request_foreign_and_downgrade_matrix_preserves_both_slots() {
    for form in 0..4 {
        let (source, source_state) = request(Default::default());
        let (target, target_state) = request(Default::default());
        let (tree, options) = parse((form != 0).then_some(&source));
        let p = match form {
            1 => processing(&source),
            3 => ProcessingContext::default(),
            _ => processing(&target),
        };
        let before = source.statistics().expanded_nodes;
        let result = tree.try_clone_in(&p);
        if form == 1 {
            let copied = result.unwrap();
            assert_eq!(copied.size(), tree.size());
            assert_eq!(source.statistics().expanded_nodes, before * 2);
            drop(copied);
        } else {
            assert_eq!(result.unwrap_err().kind, FailureKind::Invariant);
            assert_eq!(source.statistics().expanded_nodes, before);
            assert_eq!(target.statistics().expanded_nodes, 0);
            if form != 0 {
                assert_eq!(source.failure().unwrap().kind, FailureKind::Invariant);
            }
            if form != 3 {
                assert_eq!(target.failure().unwrap().kind, FailureKind::Invariant);
            }
        }
        drop((p, tree, options));
        assert_eq!(source_state.live_len(), 0);
        assert_eq!(target_state.live_len(), 0);
        source.validate_retired().unwrap();
        target.validate_retired().unwrap();
    }
}

#[test]
fn foreign_owner_rejection_preserves_target_then_source_native_priority() {
    for target_fault in [None, Some(native(-22103))] {
        let (source, source_state) = request(Default::default());
        let (target, target_state) = request(Default::default());
        let source_p = processing(&source);
        let text = TreeString::try_from_str("source", &source_p).unwrap();
        source.record(native(-22104));
        if let Some(error) = target_fault {
            target.record(error);
        }
        let target_p = processing(&target);
        let before = target_state.counts();
        assert_eq!(
            text.validate_policy(&target_p).unwrap_err(),
            target_fault.unwrap_or(native(-22104))
        );
        assert_eq!(target.failure(), target_fault.or(Some(native(-22104))));
        assert_eq!(source.failure(), Some(native(-22104)));
        assert_eq!(target_state.counts().reserve, before.reserve);
        drop((text, source_p, target_p));
        assert_eq!(source_state.live_len(), 0);
        assert_eq!(target_state.live_len(), 0);
        source.validate_retired().unwrap();
        target.validate_retired().unwrap();
    }
}

#[test]
fn same_outer_tree_policy_cannot_hide_plain_or_foreign_inner_metadata() {
    for foreign in [false, true] {
        let (r, state) = request(Default::default());
        let (other, other_state) = request(Default::default());
        let (mut tree, options) = parse(Some(&r));
        let other_p = processing(&other);
        tree.root.id = if foreign {
            TreeString::try_from_str("bad", &other_p).unwrap()
        } else {
            TreeString::plain("bad".into())
        };
        let before = r.statistics().expanded_nodes;
        let result = tree.try_clone_in(&options.processing);
        assert_eq!(result.unwrap_err().kind, FailureKind::Invariant);
        assert_eq!(r.statistics().expanded_nodes, before);
        if foreign {
            assert_eq!(other.failure().unwrap().kind, FailureKind::Invariant);
        }
        drop((tree, options, other_p));
        assert_eq!(state.live_len(), 0);
        assert_eq!(other_state.live_len(), 0);
        r.validate_retired().unwrap();
        other.validate_retired().unwrap();
    }
}

#[test]
fn source_strings_keep_utf8_boundaries_and_pay_each_bounded_output_window() {
    let source = "中é𐀀".repeat(1024);
    let (r, state) = request(Default::default());
    let p = processing(&r);
    let text = TreeString::try_from_str(&source, &p).unwrap();
    assert_eq!(text.as_str(), source);
    assert!(state
        .lock()
        .events
        .iter()
        .filter(|e| e.call == Call::Checkpoint)
        .all(|e| e.work <= 256));
    assert!(
        state
            .lock()
            .events
            .iter()
            .filter(|e| e.call == Call::Checkpoint && e.work > 0)
            .count()
            > 32
    );
    drop((text, p));
    assert_eq!(state.live_len(), 0);
    r.validate_retired().unwrap();
}
