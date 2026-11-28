// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE file in the project root for license information.
// ---------------------------------------------------------------------------------------------
//! Actual R paint construction, Ready dispatch and natural owner retirement.
//! Default options/font setup is test bootstrap, not production allocation evidence.
use super::*;
use markdown_svg_budget::{
    Backend, Error, FirstFailureRef, Limits, ResourceKind, ResourcesRef, Ticket,
};
use std::{
    collections::BTreeMap,
    sync::{
        atomic::{AtomicU64, Ordering},
        Arc, Mutex,
    },
};
const FAULT: Error = Error::native(-914);
#[derive(Default)]
struct State {
    next: AtomicU64,
    reserves: AtomicU64,
    releases: AtomicU64,
    fail_release: AtomicU64,
    ledger: Mutex<BTreeMap<u32, usize>>,
}
struct Host(Arc<State>);
impl Backend for Host {
    fn reserve(&self, _: ResourceKind, bytes: usize) -> Result<Ticket, Error> {
        self.0.reserves.fetch_add(1, Ordering::SeqCst);
        let id = self.0.next.fetch_add(1, Ordering::SeqCst) as u32;
        assert!(self.0.ledger.lock().unwrap().insert(id, bytes).is_none());
        Ticket::new(1, id, 1)
    }
    fn allocation_committed(&self, ticket: &Ticket, bytes: usize) -> Result<(), Error> {
        assert_eq!(self.0.ledger.lock().unwrap()[&ticket.slot()], bytes);
        Ok(())
    }
    fn release_after_free(&self, ticket: Ticket) -> Result<(), Error> {
        self.0
            .ledger
            .lock()
            .unwrap()
            .remove(&ticket.slot())
            .unwrap();
        let n = self.0.releases.fetch_add(1, Ordering::SeqCst) + 1;
        if n == self.0.fail_release.load(Ordering::SeqCst) {
            Err(FAULT)
        } else {
            Ok(())
        }
    }
    fn checkpoint(&self, _: u64) -> Result<(), Error> {
        Ok(())
    }
}
fn parsed(svg: &str) -> (usvg::Tree, ResourcesRef, Arc<State>) {
    let state = Arc::new(State::default());
    let r = ResourcesRef::new_prepaid(
        Host(state.clone()),
        FirstFailureRef::new_prepaid(),
        Limits::default(),
    );
    let mut options = usvg::Options::default();
    options.processing = Arc::new(usvg::ProcessingContext::with_resources(r.clone()));
    let tree = usvg::Tree::from_data_controlled(svg.as_bytes(), &options).unwrap();
    (tree, r, state)
}
fn closed(r: &ResourcesRef, h: &State, fault: bool) {
    assert!(h.ledger.lock().unwrap().is_empty());
    assert_eq!(r.statistics().release_failures, u64::from(fault));
    assert_eq!(r.statistics().live_leases, u64::from(fault));
    if !fault {
        r.validate_retired().unwrap();
    }
}
fn scene() -> &'static str {
    r##"<svg xmlns="http://www.w3.org/2000/svg" width="64" height="48"><defs>
    <linearGradient id="l" gradientUnits="userSpaceOnUse" x1="0" x2="32" spreadMethod="reflect"><stop offset=".13" stop-color="red" stop-opacity=".73"/><stop offset=".41" stop-color="blue"/><stop offset=".77" stop-color="green" stop-opacity=".41"/></linearGradient>
    <radialGradient id="r"><stop stop-color="white"/><stop offset=".43" stop-color="orange" stop-opacity=".33"/><stop offset=".91" stop-color="purple"/></radialGradient>
    <pattern id="p" width="8" height="9" patternUnits="userSpaceOnUse"><rect width="5" height="6" fill="url(#r)"/></pattern>
    <clipPath id="c"><path d="M0 0H61V45H0Z"/></clipPath><mask id="m"><rect width="64" height="48" fill="white"/><circle cx="23" cy="19" r="7" fill="gray"/></mask></defs>
    <g clip-path="url(#c)" mask="url(#m)"><path d="M2 3H35V28H2Z" fill="url(#l)" fill-opacity=".713"/><path d="M30 11H58V43H30Z" fill="url(#p)"/><path d="M2 36C13 30 21 46 57 37" fill="none" stroke="url(#l)" stroke-width=".61" stroke-linecap="round"/></g></svg>"##
}
#[test]
fn actual_r_paid_gradients_pattern_clip_mask_and_aa_hairline_match_ordinary() {
    let ordinary = usvg::Tree::from_str(scene(), &usvg::Options::default()).unwrap();
    let mut a = tiny_skia::Pixmap::new(64, 48).unwrap();
    crate::render(&ordinary, tiny_skia::Transform::identity(), &mut a.as_mut()).unwrap();
    let (tree, r, h) = parsed(scene());
    let mut b = tiny_skia::Pixmap::new(64, 48).unwrap();
    crate::render_in_request(&tree, tiny_skia::Transform::identity(), &mut b.as_mut()).unwrap();
    assert_eq!(a.data(), b.data());
    assert!(b.data().iter().any(|v| *v != 0));
    drop(tree);
    closed(&r, &h, false);
}
const TWO: &str = r##"<svg xmlns="http://www.w3.org/2000/svg" width="64" height="32"><defs><linearGradient id="l" gradientUnits="userSpaceOnUse" x2="32"><stop offset=".13" stop-color="red" stop-opacity=".73"/><stop offset=".41" stop-color="blue"/><stop offset=".77" stop-color="green" stop-opacity=".41"/></linearGradient></defs><path d="M0 0H32V32H0Z" fill="url(#l)"/><path d="M40 0H64V32H40Z" fill="lime"/></svg>"##;
fn context(tree: &usvg::Tree) -> Context<'_> {
    Context {
        max_bbox: tiny_skia::IntRect::from_xywh(0, 0, 64, 32).unwrap(),
        processing: tree.processing_context().clone(),
        coverage: None,
    }
}
#[test]
fn actual_r_ready_transforms_shader_once_and_keeps_device_a8() {
    let (tree, r, h) = parsed(TWO);
    let ctx = context(&tree);
    let usvg::Node::Path(path) = &tree.root().children()[0] else {
        panic!("first path")
    };
    let transform = tiny_skia::Transform::from_row(0.93, 0.11, -0.04, 1.03, 1.33, -0.75);
    let coverage: Vec<u8> = (0..64 * 32)
        .map(|i| {
            if i % 64 < 32 {
                [0, 255, 255, 129, 17][i % 5]
            } else {
                0
            }
        })
        .collect();
    let mask = tiny_skia::MaskRef::from_bytes(&coverage, 64, 32).unwrap();
    let mut a = tiny_skia::Pixmap::new(64, 32).unwrap();
    let mut b = a.clone();
    let usvg::Paint::LinearGradient(g) = path.fill().unwrap().paint() else {
        panic!("linear")
    };
    let mut plain = tiny_skia::Paint {
        shader: convert_linear_gradient(g, path.fill().unwrap().opacity()).unwrap(),
        ..tiny_skia::Paint::default()
    };
    plain.shader.transform(transform);
    a.as_mut()
        .paint_coverage_checked::<Error>(mask, &plain, |_| Ok(()))
        .unwrap();
    assert!(fill_path_prepared(
        path,
        tiny_skia::BlendMode::SourceOver,
        &ctx,
        transform,
        &mut b.as_mut(),
        FillCoverage::Ready(mask)
    )
    .is_some());
    assert_eq!(a.data(), b.data());
    drop(ctx);
    drop(tree);
    closed(&r, &h, false);
}
#[test]
fn actual_r_each_first_paint_natural_refund_fault_prevents_following_shape() {
    let (tree, r, h) = parsed(TWO);
    let ctx = context(&tree);
    let usvg::Node::Path(path) = &tree.root().children()[0] else {
        panic!("first path")
    };
    let before = h.releases.load(Ordering::SeqCst);
    let mut pixels = tiny_skia::Pixmap::new(64, 32).unwrap();
    assert!(fill_path_prepared(
        path,
        tiny_skia::BlendMode::SourceOver,
        &ctx,
        tiny_skia::Transform::identity(),
        &mut pixels.as_mut(),
        FillCoverage::Original
    )
    .is_some());
    let count = h.releases.load(Ordering::SeqCst) - before;
    assert!(count >= 16);
    drop(ctx);
    drop(tree);
    closed(&r, &h, false);
    for failed in 1..=count {
        let (tree, r, h) = parsed(TWO);
        let ctx = context(&tree);
        let usvg::Node::Path(path) = &tree.root().children()[0] else {
            panic!("first path")
        };
        h.fail_release
            .store(h.releases.load(Ordering::SeqCst) + failed, Ordering::SeqCst);
        let mut pixels = tiny_skia::Pixmap::new(64, 32).unwrap();
        assert!(
            fill_path_prepared(
                path,
                tiny_skia::BlendMode::SourceOver,
                &ctx,
                tiny_skia::Transform::identity(),
                &mut pixels.as_mut(),
                FillCoverage::Original
            )
            .is_none(),
            "refund={failed}"
        );
        assert_eq!(r.failure(), Some(FAULT));
        let allocations = h.reserves.load(Ordering::SeqCst);
        crate::render::render_node(
            &tree.root().children()[1],
            &ctx,
            tiny_skia::Transform::identity(),
            &mut pixels.as_mut(),
        );
        assert_eq!(
            h.reserves.load(Ordering::SeqCst),
            allocations,
            "refund={failed}"
        );
        assert!(
            pixels
                .data()
                .chunks_exact(64 * 4)
                .all(|row| row[40 * 4..].iter().all(|v| *v == 0)),
            "refund={failed}"
        );
        drop(ctx);
        drop(tree);
        closed(&r, &h, true);
    }
}
