// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE file in the project root for license information.
// ---------------------------------------------------------------------------------------------
//! Actual default/nested/custom resolver boundaries, independent of FontReplay.

use super::*;
use crate::geometry_test_support::{cancelled, request, Call};
use crate::{Options, ProcessingContext, Tree};
use markdown_svg_budget::Limits;
use std::sync::atomic::{AtomicUsize, Ordering};

fn database() -> (Arc<Database>, ID) {
    let mut db = Database::new();
    db.load_font_data(
        include_bytes!("../../../../dependencies/fontdb-0.24.0/tests/fonts/Tuffy.ttf").to_vec(),
    );
    let first = db.faces().next().unwrap().id;
    // Two distinct catalog faces with a known A mapping. The upstream trimmed
    // Cantarell fixture has no A and cannot serve as this positive control.
    db.load_font_data(
        include_bytes!("../../../../dependencies/fontdb-0.24.0/tests/fonts/Tuffy.ttf").to_vec(),
    );
    assert!(db.faces().count() >= 2);
    (Arc::new(db), first)
}

#[test]
fn default_fallback_and_nested_borrow_keep_identity_output_and_every_cancel_boundary() {
    let (mut db, first) = database();
    let selector = FontResolver::default_fallback_selector();
    let expected = selector('A', &[first], &mut db).expect("a real second font supports A");
    let (resources, state) = request(Limits::default());
    let options = Options {
        fontdb: db.clone(),
        processing: Arc::new(ProcessingContext::with_resources(resources.clone())),
        ..Options::default()
    };
    let nested = options.nested_image_options().unwrap();
    assert!(matches!(
        nested.font_resolver.select_fallback,
        FallbackSelectionFn::Default
    ));
    assert!(matches!(
        nested.font_resolver.select_fallback.borrowed(),
        FallbackSelectionFn::Default
    ));
    let before = state.counts().checkpoint;
    assert_eq!(
        nested
            .font_resolver
            .select_fallback
            .try_select('A', &[first], &mut db, &nested.processing)
            .unwrap(),
        Some(expected)
    );
    let checkpoints = state.counts().checkpoint - before;
    assert!(
        checkpoints > 10,
        "the actual font/cmap reader is inside this observation"
    );
    println!(
        "DEFAULT_FALLBACK checkpoints={checkpoints} cancelled_ordinals=1..={checkpoints} default_and_nested_identity_preserved=true"
    );
    drop(nested);
    drop(options);
    resources.validate_retired().unwrap();
    assert_eq!(state.live_len(), 0);

    // Preserve the complete positive callback sequence, including all deep
    // reader checkpoints. No ordinal is replaced with a guessed count.
    for ordinal in 1..=checkpoints {
        let (resources, state) = request(Limits::default());
        let options = Options {
            fontdb: db.clone(),
            processing: Arc::new(ProcessingContext::with_resources(resources.clone())),
            ..Options::default()
        };
        let nested = options.nested_image_options().unwrap();
        let before = state.counts().checkpoint;
        state.arm(Call::Checkpoint, ordinal, cancelled());
        let result = nested.font_resolver.select_fallback.try_select(
            'A',
            &[first],
            &mut db,
            &nested.processing,
        );
        assert_eq!(result, Err(cancelled()), "ordinal {ordinal}");
        assert_eq!(
            state.counts().checkpoint - before,
            ordinal,
            "no backend call after the first fault"
        );
        drop(nested);
        drop(options);
        assert_eq!(state.live_len(), 0);
        assert_eq!(resources.statistics().scratch_bytes, 0);
        assert_eq!(resources.failure(), Some(cancelled()));
    }
}

#[test]
fn owned_and_borrowed_custom_fallback_cannot_publish_some_or_none_after_fault() {
    let (mut db, first) = database();
    for returns_some in [false, true] {
        for borrowed in [false, true] {
            let (resources, state) = request(Limits::default());
            let processing = ProcessingContext::with_resources(resources.clone());
            let calls = AtomicUsize::new(0);
            let callback = |_: char, _: &[ID], _: &mut Arc<Database>| {
                calls.fetch_add(1, Ordering::Relaxed);
                resources.record(cancelled());
                returns_some.then_some(first)
            };
            let selector = if borrowed {
                FallbackSelectionFn::Borrowed(&callback)
            } else {
                FallbackSelectionFn::Owned(Box::new(callback))
            };
            assert_eq!(
                selector.try_select('A', &[first], &mut db, &processing),
                Err(cancelled())
            );
            assert_eq!(calls.load(Ordering::Relaxed), 1);
            assert_eq!(state.live_len(), 0);
        }
    }
}

#[test]
fn first_primary_resolver_fault_stops_the_next_actual_text_span() {
    let (db, first) = database();
    let (resources, state) = request(Limits::default());
    let calls = Arc::new(AtomicUsize::new(0));
    let seen = calls.clone();
    let request = resources.clone();
    let options = Options {
        fontdb: db,
        processing: Arc::new(ProcessingContext::with_resources(resources.clone())),
        font_resolver: FontResolver {
            select_font: FontSelectionFn::Owned(Box::new(move |_, _| {
                seen.fetch_add(1, Ordering::Relaxed);
                request.record(cancelled());
                Some(first)
            })),
            select_fallback: FallbackSelectionFn::Default,
        },
        ..Options::default()
    };
    let source = b"<svg xmlns='http://www.w3.org/2000/svg' width='400' height='80'><text x='4' y='30'><tspan font-family='First'>one</tspan><tspan font-family='Second'>two</tspan></text></svg>";
    assert!(Tree::from_data_controlled(source, &options).is_err());
    assert_eq!(calls.load(Ordering::Relaxed), 1);
    assert_eq!(resources.failure(), Some(cancelled()));
    drop(options);
    assert_eq!(state.live_len(), 0);
    assert_eq!(resources.statistics().scratch_bytes, 0);
}
