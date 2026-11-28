// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE file in the project root for license information.
// ---------------------------------------------------------------------------------------------
//! Actual fill consumers plus narrowly named edge/AA lifecycle tests.

use super::*;
use crate::{Mask, Paint, PathBuilder, Pixmap, Transform};
use alloc::{collections::BTreeMap, sync::Arc, vec::Vec};
use core::sync::atomic::{AtomicU64, Ordering};
use markdown_svg_budget::{
    Backend, Error, FailureKind, FirstFailureRef, LimitKind, Limits, ResourceKind, ResourcesRef,
    Ticket,
};
use std::sync::Mutex;

const FAULT: Error = Error::native(-2147024882);
#[derive(Default)]
struct Host {
    next: AtomicU64,
    ledger: Mutex<BTreeMap<u32, (ResourceKind, usize)>>,
    scratch_reserves: AtomicU64,
    scratch_releases: AtomicU64,
    checks: AtomicU64,
    fail_reserve: AtomicU64,
    fail_release: AtomicU64,
    failed_release_bytes: AtomicU64,
    fail_check: AtomicU64,
    reserve_events: Mutex<Vec<(ResourceKind, usize)>>,
}
struct HostBackend(Arc<Host>);
impl core::ops::Deref for HostBackend {
    type Target = Host;
    fn deref(&self) -> &Host {
        &self.0
    }
}
impl Backend for HostBackend {
    fn reserve(&self, kind: ResourceKind, bytes: usize) -> Result<Ticket, Error> {
        self.reserve_events.lock().unwrap().push((kind, bytes));
        if kind == ResourceKind::Scratch {
            let event = self.scratch_reserves.fetch_add(1, Ordering::SeqCst) + 1;
            if event == self.fail_reserve.load(Ordering::SeqCst) {
                return Err(FAULT);
            }
        }
        let slot = self.next.fetch_add(1, Ordering::SeqCst) as u32;
        assert!(self
            .ledger
            .lock()
            .unwrap()
            .insert(slot, (kind, bytes))
            .is_none());
        Ticket::new(1, slot, 1)
    }
    fn allocation_committed(&self, ticket: &Ticket, bytes: usize) -> Result<(), Error> {
        assert_eq!(self.ledger.lock().unwrap()[&ticket.slot()].1, bytes);
        Ok(())
    }
    fn release_after_free(&self, ticket: Ticket) -> Result<(), Error> {
        let (kind, bytes) = self.ledger.lock().unwrap().remove(&ticket.slot()).unwrap();
        if kind == ResourceKind::Scratch {
            let event = self.scratch_releases.fetch_add(1, Ordering::SeqCst) + 1;
            if event == self.fail_release.load(Ordering::SeqCst) {
                self.failed_release_bytes
                    .store(bytes as u64, Ordering::SeqCst);
                return Err(FAULT);
            }
        }
        Ok(())
    }
    fn checkpoint(&self, _: u64) -> Result<(), Error> {
        let event = self.checks.fetch_add(1, Ordering::SeqCst) + 1;
        if event == self.fail_check.load(Ordering::SeqCst) {
            Err(FAULT)
        } else {
            Ok(())
        }
    }
}
fn resources(limits: Limits) -> (ResourcesRef, Arc<Host>) {
    let host = Arc::new(Host::default());
    (
        ResourcesRef::new_prepaid(
            HostBackend(host.clone()),
            FirstFailureRef::new_prepaid(),
            limits,
        ),
        host,
    )
}
fn closed(resources: &ResourcesRef, host: &Host, failed_release: bool) {
    let stats = resources.statistics();
    assert!(host.ledger.lock().unwrap().is_empty()); // every actual allocation was freed
    assert_eq!(stats.release_failures, u64::from(failed_release));
    if failed_release {
        // B intentionally retains the exact failed refund fee and ticket count;
        // physical deallocation does not turn backend Err into successful retirement.
        assert_eq!(stats.live_leases, 1);
        assert_eq!(
            stats.scratch_bytes,
            host.failed_release_bytes.load(Ordering::SeqCst)
        );
        assert!(stats.scratch_bytes > 0);
        assert_eq!(resources.failure(), Some(FAULT));
    } else {
        assert_eq!(stats.live_leases, 0);
        assert_eq!(stats.scratch_bytes, 0);
        resources.validate_retired().unwrap();
    }
}

fn fixture(seed: u32) -> Path {
    let mut p = PathBuilder::new();
    let offset = seed as f32 * 0.137;
    p.move_to(-6.5 + offset, 8.3);
    p.cubic_to(15.25, -20.0, 75.75, 82.5, 58.625, 19.5);
    p.quad_to(85.5, 74.3, 24.25, 57.75);
    p.line_to(10.0, 30.5);
    p.close();
    p.move_to(18.5, 15.25);
    p.line_to(49.75, 49.125);
    p.line_to(19.25, 48.5);
    p.close();
    p.move_to(30.5, 25.25);
    p.quad_to(12.25, 63.5, 48.125, 17.75);
    p.finish().unwrap()
}
fn paint(aa: bool) -> Paint<'static> {
    let mut paint = Paint::default();
    paint.anti_alias = aa;
    paint.set_color_rgba8(17, 122, 232, 201);
    paint
}

#[test]
fn actual_pixmap_fill_and_mask_preserve_all_bytes_for_request_and_ordinary() {
    for seed in 0..8 {
        for aa in [false, true] {
            for rule in [FillRule::Winding, FillRule::EvenOdd] {
                let plain = fixture(seed);
                let (r, h) = resources(Limits::default());
                let paid = plain.try_copy_in(&r, ResourceKind::Geometry).unwrap();
                for transform in [Transform::identity(), Transform::from_translate(1.25, -2.5)] {
                    let mut ordinary = Pixmap::new(72, 64).unwrap();
                    let mut request = Pixmap::new(72, 64).unwrap();
                    ordinary.fill_path(&plain, &paint(aa), rule, transform, None);
                    request
                        .try_fill_path(&paid, &paint(aa), rule, transform, None, &r)
                        .unwrap();
                    assert_eq!(
                        ordinary.data(),
                        request.data(),
                        "seed={seed} aa={aa} rule={rule:?}"
                    );
                    assert!(request.data().iter().any(|v| *v != 0));
                    let mut ordinary = Mask::new(72, 64).unwrap();
                    let mut request = Mask::new(72, 64).unwrap();
                    ordinary.fill_path(&plain, rule, aa, transform);
                    request
                        .try_fill_path(&paid, rule, aa, transform, &r)
                        .unwrap();
                    assert_eq!(ordinary.data(), request.data());
                }
                assert!(h.scratch_reserves.load(Ordering::SeqCst) > 0);
                assert_eq!(r.statistics().scratch_bytes, 0);
                drop(paid);
                closed(&r, &h, false);
            }
        }
    }
}

#[test]
fn actual_sort_scratch_refund_failure_stops_before_first_pixel() {
    for aa in [false, true] {
        let (r, h) = resources(Limits::default());
        let path = fixture(0).try_copy_in(&r, ResourceKind::Geometry).unwrap();
        h.fail_release.store(1, Ordering::SeqCst);
        let mut pixels = Pixmap::new(72, 64).unwrap();
        assert_eq!(
            pixels.try_fill_path(
                &path,
                &paint(aa),
                FillRule::Winding,
                Transform::identity(),
                None,
                &r
            ),
            Err(FAULT)
        );
        assert_eq!(r.failure(), Some(FAULT));
        assert!(pixels.data().iter().all(|v| *v == 0));
        drop(path);
        closed(&r, &h, true);
    }
}

#[test]
fn actual_tiled_fill_edge_refund_error_stops_next_tile() {
    for aa in [false, true] {
        for failed_release in 2..=if aa { 4 } else { 2 } {
            let (r, h) = resources(Limits::default());
            let plain = PathBuilder::from_rect(Rect::from_xywh(0.25, 0.25, 16000.0, 6.5).unwrap());
            let paid = plain.try_copy_in(&r, ResourceKind::Geometry).unwrap();
            h.fail_release.store(failed_release, Ordering::SeqCst); // sort1, edges2, AA arrays3/4
            let mut pixels = Pixmap::new(16000, 8).unwrap();
            assert_eq!(
                pixels.try_fill_path(
                    &paid,
                    &paint(aa),
                    FillRule::Winding,
                    Transform::identity(),
                    None,
                    &r
                ),
                Err(FAULT)
            );
            assert!(pixels
                .data()
                .chunks_exact(16000 * 4)
                .all(|row| row[8191 * 4..].iter().all(|b| *b == 0)));
            assert!(pixels
                .data()
                .chunks_exact(16000 * 4)
                .any(|row| row[..8191 * 4].iter().any(|b| *b != 0)));
            assert_eq!(r.failure(), Some(FAULT));
            drop(paid);
            closed(&r, &h, true);
        }
    }
}
#[test]
fn actual_fill_every_scratch_allocation_failure_returns_original_error_and_retires() {
    for aa in [false, true] {
        let (r, h) = resources(Limits::default());
        let paid = fixture(0).try_copy_in(&r, ResourceKind::Geometry).unwrap();
        let mut pixels = Pixmap::new(72, 64).unwrap();
        pixels
            .try_fill_path(
                &paid,
                &paint(aa),
                FillRule::Winding,
                Transform::identity(),
                None,
                &r,
            )
            .unwrap();
        let count = h.scratch_reserves.load(Ordering::SeqCst);
        assert!(count >= 2);
        drop(paid);
        closed(&r, &h, false);
        for event in 1..=count {
            let (r, h) = resources(Limits::default());
            let paid = fixture(0).try_copy_in(&r, ResourceKind::Geometry).unwrap();
            h.fail_reserve.store(event, Ordering::SeqCst);
            let mut pixels = Pixmap::new(72, 64).unwrap();
            assert_eq!(
                pixels.try_fill_path(
                    &paid,
                    &paint(aa),
                    FillRule::Winding,
                    Transform::identity(),
                    None,
                    &r
                ),
                Err(FAULT),
                "aa={aa} event={event}"
            );
            assert_eq!(r.failure(), Some(FAULT));
            drop(paid);
            closed(&r, &h, false);
        }
    }
}

#[derive(Default)]
struct BlitCount {
    horizontal: usize,
    rows: usize,
}
impl Blitter for BlitCount {
    fn blit_h(&mut self, _: u32, _: u32, _: LengthU32) {
        self.horizontal += 1;
    }
    fn blit_anti_h(&mut self, _: u32, _: u32, _: &mut [u8], _: &mut [crate::alpha_runs::AlphaRun]) {
        self.rows += 1;
    }
}
#[test]
fn aa_explicit_finish_flushes_once_and_unfinished_request_drop_never_paints() {
    for mode in 0..3 {
        let (r, h) = resources(Limits::default());
        let mut count = BlitCount::default();
        let bounds = IntRect::from_xywh(0, 0, 8, 8).unwrap();
        let clip = ScreenIntRect::from_xywh(0, 0, 8, 8).unwrap();
        let mut aa = SuperBlitter::new(&bounds, &clip, &mut count, GeometryControl::request(&r))
            .unwrap()
            .unwrap();
        aa.blit_h(0, 0, LengthU32::new(8).unwrap());
        if mode == 0 {
            aa.finish().unwrap();
        }
        if mode == 1 {
            r.record(FAULT);
            assert_eq!(aa.finish(), Err(FAULT));
        }
        drop(aa);
        assert_eq!(count.rows, usize::from(mode == 0));
        closed(&r, &h, false);
    }
}
#[test]
fn clipped_no_output_and_non_yielding_verbs_observe_work_quota() {
    let mut p = PathBuilder::new();
    for i in 0..512 {
        p.move_to(1000.0 + i as f32, 1000.0);
        p.close();
    }
    // Ensure finite nonzero bounds while the preceding Move/Close pairs yield no edge.
    p.move_to(1000.0, 1000.0);
    p.line_to(1005.0, 1005.0);
    p.close();
    let path = p.finish().unwrap();
    assert!(path.len() > 1000);
    for aa in [false, true] {
        let mut limits = Limits::default();
        limits.work = 16;
        let (r, h) = resources(limits);
        let mut count = BlitCount::default();
        let clip = ScreenIntRect::from_xywh(0, 0, 8, 8).unwrap();
        let result = if aa {
            fill_path_with_control(
                &path,
                FillRule::Winding,
                &clip,
                &mut count,
                GeometryControl::request(&r),
            )
        } else {
            super::super::path::fill_path_with_control(
                &path,
                FillRule::Winding,
                &clip,
                &mut count,
                GeometryControl::request(&r),
            )
        };
        assert_eq!(result, Err(Error::new(FailureKind::Quota, LimitKind::Work)));
        assert_eq!(count.horizontal + count.rows, 0);
        closed(&r, &h, false);
    }
}

#[test]
fn actual_fill_sampled_checkpoint_failures_keep_typed_error_and_free_all_scratch() {
    for aa in [false, true] {
        let (r, h) = resources(Limits::default());
        let paid = fixture(0).try_copy_in(&r, ResourceKind::Geometry).unwrap();
        let start = h.checks.load(Ordering::SeqCst);
        let mut pixels = Pixmap::new(72, 64).unwrap();
        pixels
            .try_fill_path(
                &paid,
                &paint(aa),
                FillRule::Winding,
                Transform::identity(),
                None,
                &r,
            )
            .unwrap();
        let total = h.checks.load(Ordering::SeqCst) - start;
        drop(paid);
        closed(&r, &h, false);
        assert!(total > 256);
        for relative in (1..=total)
            .step_by((total / 32).max(1) as usize)
            .chain(core::iter::once(total))
        {
            let (r, h) = resources(Limits::default());
            let paid = fixture(0).try_copy_in(&r, ResourceKind::Geometry).unwrap();
            h.fail_check
                .store(h.checks.load(Ordering::SeqCst) + relative, Ordering::SeqCst);
            let mut pixels = Pixmap::new(72, 64).unwrap();
            assert_eq!(
                pixels.try_fill_path(
                    &paid,
                    &paint(aa),
                    FillRule::Winding,
                    Transform::identity(),
                    None,
                    &r
                ),
                Err(FAULT),
                "aa={aa} relative={relative} total={total}"
            );
            assert_eq!(r.failure(), Some(FAULT));
            drop(paid);
            closed(&r, &h, false);
        }
    }
}

#[test]
fn actual_non_aa_hairline_stroke_preserves_bytes_and_stops_on_cancel() {
    let plain = fixture(1);
    let stroke = crate::Stroke {
        width: 0.5,
        ..crate::Stroke::default()
    };
    let (r, h) = resources(Limits::default());
    let paid = plain.try_copy_in(&r, ResourceKind::Geometry).unwrap();
    let mut ordinary = Pixmap::new(72, 64).unwrap();
    let mut request = Pixmap::new(72, 64).unwrap();
    ordinary.stroke_path(&plain, &paint(false), &stroke, Transform::identity(), None);
    let start = h.checks.load(Ordering::SeqCst);
    request
        .try_stroke_path(
            &paid,
            &paint(false),
            &stroke,
            Transform::identity(),
            None,
            &r,
        )
        .unwrap();
    let total = h.checks.load(Ordering::SeqCst) - start;
    assert_eq!(ordinary.data(), request.data());
    assert!(request.data().iter().any(|v| *v != 0));
    assert!(
        total > 256,
        "actual non-AA hairline loop must poll the original request"
    );
    drop(paid);
    closed(&r, &h, false);
    for relative in [1, total / 2, total] {
        let (r, h) = resources(Limits::default());
        let paid = plain.try_copy_in(&r, ResourceKind::Geometry).unwrap();
        h.fail_check
            .store(h.checks.load(Ordering::SeqCst) + relative, Ordering::SeqCst);
        let mut pixels = Pixmap::new(72, 64).unwrap();
        assert_eq!(
            pixels.try_stroke_path(
                &paid,
                &paint(false),
                &stroke,
                Transform::identity(),
                None,
                &r
            ),
            Err(FAULT)
        );
        assert_eq!(r.failure(), Some(FAULT));
        drop(paid);
        closed(&r, &h, false);
    }
}

#[test]
fn raster_sort_owner_tail_refund_error_rejects_returned_candidate() {
    use crate::edge_builder::RasterVec;
    let (r, h) = resources(Limits::default());
    let c = GeometryControl::request(&r);
    let mut edges = RasterVec::new(c);
    edges.push(3u32, c).unwrap();
    edges.push(1, c).unwrap();
    edges.push(2, c).unwrap();
    h.fail_release.store(1, Ordering::SeqCst);
    let candidate = edges.sort_by(c, &mut |a, b| a.cmp(b));
    // This assertion is specifically the helper's return boundary, not a
    // claim that every outer stock consumer lacks an additional guard.
    assert_eq!(candidate, Err(FAULT));
    assert_eq!(r.failure(), Some(FAULT));
    drop(edges);
    closed(&r, &h, true);
}

#[test]
fn lowp_wide_gradient_ordinary_and_request_geometry_share_pixels() {
    // Paint/stops are explicit external Plain input to this compatibility API.
    // This checks the shared pixel kernel, not complete paid gradient storage.
    for stop_count in [65_535, 65_536, 65_537] {
        for width in [1, 16, 17] {
            let (r, h) = resources(Limits::default());
            crate::pipeline::gradient_index_tests::assert_last_stop_color(
                stop_count,
                width,
                Some(&r),
            );
            closed(&r, &h, false);
        }
    }
}
