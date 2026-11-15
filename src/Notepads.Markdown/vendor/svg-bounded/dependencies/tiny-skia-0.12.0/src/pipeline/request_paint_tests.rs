// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE file in the project root for license information.
// ---------------------------------------------------------------------------------------------
//! Real paint consumers, allocation lifecycle failures and pixel window guards.
use super::*;
use crate::blitter::Blitter;
use crate::shaders::PaintView;
use crate::{
    BlendMode, CheckedPaint, CheckedStops, ColorSpace, FillRule, GradientStop, LineCap, Paint,
    PaintOptions, PathBuilder, Pixmap, Point, Rect, SpreadMode, Stroke,
};
use alloc::{collections::BTreeMap, sync::Arc, vec, vec::Vec};
use core::num::NonZeroU16;
use core::sync::atomic::{AtomicU64, Ordering};
use markdown_svg_budget::{Backend, Error, FirstFailureRef, Limits, ResourcesRef, Ticket};
use std::sync::Mutex;

const FAULT: Error = Error::native(-2147024882);
#[derive(Default)]
struct Host {
    next: AtomicU64,
    ledger: Mutex<BTreeMap<u32, (ResourceKind, usize)>>,
    reserves: AtomicU64,
    commits: AtomicU64,
    releases: AtomicU64,
    fail_reserve: AtomicU64,
    fail_commit: AtomicU64,
    fail_release: AtomicU64,
    work_value: AtomicU64,
    work_count: AtomicU64,
    fail_work: AtomicU64,
    reserve_bytes: Mutex<Vec<usize>>,
    release_bytes: Mutex<Vec<usize>>,
    works: Mutex<Vec<u64>>,
}
struct HostBackend(Arc<Host>);
impl Backend for HostBackend {
    fn reserve(&self, kind: ResourceKind, bytes: usize) -> Result<Ticket, Error> {
        if kind == ResourceKind::Scratch {
            self.0.reserve_bytes.lock().unwrap().push(bytes);
            let n = self.0.reserves.fetch_add(1, Ordering::SeqCst) + 1;
            if n == self.0.fail_reserve.load(Ordering::SeqCst) {
                return Err(FAULT);
            }
        }
        let id = self.0.next.fetch_add(1, Ordering::SeqCst) as u32;
        assert!(self
            .0
            .ledger
            .lock()
            .unwrap()
            .insert(id, (kind, bytes))
            .is_none());
        Ticket::new(1, id, 1)
    }
    fn allocation_committed(&self, ticket: &Ticket, bytes: usize) -> Result<(), Error> {
        let (kind, actual) = self.0.ledger.lock().unwrap()[&ticket.slot()];
        assert_eq!(bytes, actual);
        if kind == ResourceKind::Scratch {
            let n = self.0.commits.fetch_add(1, Ordering::SeqCst) + 1;
            if n == self.0.fail_commit.load(Ordering::SeqCst) {
                return Err(FAULT);
            }
        }
        Ok(())
    }
    fn release_after_free(&self, ticket: Ticket) -> Result<(), Error> {
        let (kind, bytes) = self
            .0
            .ledger
            .lock()
            .unwrap()
            .remove(&ticket.slot())
            .unwrap();
        if kind == ResourceKind::Scratch {
            self.0.release_bytes.lock().unwrap().push(bytes);
            let n = self.0.releases.fetch_add(1, Ordering::SeqCst) + 1;
            if n == self.0.fail_release.load(Ordering::SeqCst) {
                return Err(FAULT);
            }
        }
        Ok(())
    }
    fn checkpoint(&self, work: u64) -> Result<(), Error> {
        self.0.works.lock().unwrap().push(work);
        if work == self.0.work_value.load(Ordering::SeqCst) {
            let n = self.0.work_count.fetch_add(1, Ordering::SeqCst) + 1;
            if n == self.0.fail_work.load(Ordering::SeqCst) {
                return Err(FAULT);
            }
        }
        Ok(())
    }
}
fn request() -> (ResourcesRef, Arc<Host>) {
    let h = Arc::new(Host::default());
    (
        ResourcesRef::new_prepaid(
            HostBackend(h.clone()),
            FirstFailureRef::new_prepaid(),
            Limits::default(),
        ),
        h,
    )
}
fn closed(r: &ResourcesRef, h: &Host, refund_failed: bool) {
    assert!(h.ledger.lock().unwrap().is_empty());
    assert_eq!(r.statistics().release_failures, u64::from(refund_failed));
    assert_eq!(r.statistics().live_leases, u64::from(refund_failed));
    if !refund_failed {
        assert_eq!(r.statistics().scratch_bytes, 0);
        r.validate_retired().unwrap();
    }
}
fn stops(count: usize) -> Vec<GradientStop> {
    (0..count)
        .map(|i| {
            let color = Color::from_rgba8(
                (i * 37) as u8,
                (i * 11) as u8,
                (i * 79) as u8,
                (51 + i * 23) as u8,
            );
            GradientStop::new(0.13 + 0.76 * i as f32 / (count - 1) as f32, color)
        })
        .collect()
}
fn paid_stops(points: &[GradientStop], r: &ResourcesRef) -> CheckedStops {
    let mut result = CheckedStops::new(r);
    result.try_reserve(points.len() + 2).unwrap();
    for &point in points {
        result.try_push(point).unwrap();
    }
    result
}
fn checked(r: &ResourcesRef, count: usize, options: PaintOptions) -> CheckedPaint<'static> {
    CheckedPaint::try_linear(
        Point::from_xy(0.0, 0.0),
        Point::from_xy(53.0, 0.0),
        paid_stops(&stops(count), r),
        SpreadMode::Reflect,
        Transform::identity(),
        options,
        r,
    )
    .unwrap()
    .unwrap()
}
fn rect(width: u32, height: u32) -> Rect {
    Rect::from_xywh(0.0, 0.0, width as f32, height as f32).unwrap()
}

#[test]
fn checked_gradient_factories_match_plain_degenerate_and_normal_pixels() {
    for kind in 0..9 {
        for spread in [SpreadMode::Pad, SpreadMode::Reflect, SpreadMode::Repeat] {
            for cs in [
                ColorSpace::Linear,
                ColorSpace::Gamma2,
                ColorSpace::SimpleSRGB,
                ColorSpace::FullSRGBGamma,
            ] {
                let (r, h) = request();
                let points = stops(17);
                let paid = paid_stops(&points, &r);
                let ts = Transform::from_row(1.03, 0.07, -0.02, 0.97, 0.33, -0.66);
                let options = PaintOptions {
                    colorspace: cs,
                    anti_alias: false,
                    ..PaintOptions::default()
                };
                let (plain, paid) = match kind {
                    0..=2 => {
                        let end = if kind == 0 {
                            Point::from_xy(53.0, 9.0)
                        } else {
                            Point::from_xy(0.0, 0.0)
                        };
                        (
                            crate::LinearGradient::new(Point::zero(), end, points, spread, ts),
                            CheckedPaint::try_linear(
                                Point::zero(),
                                end,
                                paid,
                                spread,
                                ts,
                                options,
                                &r,
                            )
                            .unwrap(),
                        )
                    }
                    3..=5 => {
                        let (start, r0, end, r1) = match kind {
                            3 => (
                                Point::from_xy(13.0, 12.0),
                                2.0,
                                Point::from_xy(25.0, 27.0),
                                23.0,
                            ),
                            4 => (
                                Point::from_xy(25.0, 27.0),
                                23.0,
                                Point::from_xy(25.0, 27.0),
                                23.0,
                            ),
                            _ => (Point::zero(), 0.0, Point::zero(), 0.0),
                        };
                        (
                            crate::RadialGradient::new(start, r0, end, r1, points, spread, ts),
                            CheckedPaint::try_radial(
                                start, r0, end, r1, paid, spread, ts, options, &r,
                            )
                            .unwrap(),
                        )
                    }
                    _ => {
                        let (a, b) = match kind {
                            6 => (-45.0, 270.0),
                            7 => (90.0, 90.0),
                            _ => (0.0, 0.0),
                        };
                        (
                            crate::SweepGradient::new(
                                Point::from_xy(31.0, 29.0),
                                a,
                                b,
                                points,
                                spread,
                                ts,
                            ),
                            CheckedPaint::try_sweep(
                                Point::from_xy(31.0, 29.0),
                                a,
                                b,
                                paid,
                                spread,
                                ts,
                                options,
                                &r,
                            )
                            .unwrap(),
                        )
                    }
                };
                assert_eq!(
                    plain.is_some(),
                    paid.is_some(),
                    "kind={kind} spread={spread:?}"
                );
                if let (Some(shader), Some(paid)) = (plain, paid) {
                    let plain = Paint {
                        shader,
                        anti_alias: options.anti_alias,
                        colorspace: cs,
                        ..Paint::default()
                    };
                    let mut a = Pixmap::new(67, 61).unwrap();
                    let mut b = a.clone();
                    a.fill_rect(rect(67, 61), &plain, Transform::identity(), None);
                    b.as_mut()
                        .try_fill_rect_checked(rect(67, 61), &paid, Transform::identity(), None, &r)
                        .unwrap();
                    assert_eq!(
                        a.data(),
                        b.data(),
                        "kind={kind} spread={spread:?} cs={cs:?}"
                    );
                    drop(paid);
                }
                closed(&r, &h, false);
            }
        }
    }
}

fn context_bits(ctx: &GradientCtx) -> (usize, Vec<u32>, Vec<u32>, Vec<u32>) {
    let bits = |colors: &[GradientColor]| {
        colors
            .iter()
            .flat_map(|c| [c.r.to_bits(), c.g.to_bits(), c.b.to_bits(), c.a.to_bits()])
            .collect()
    };
    (
        ctx.len,
        bits(&ctx.factors),
        bits(&ctx.biases),
        ctx.t_values.iter().map(|t| t.get().to_bits()).collect(),
    )
}
#[test]
fn ordered_opacity_view_preserves_each_context_float_and_transparent_dummy_removal() {
    for ops in [&[0.713, 0.273, 0.923][..], &[0.0][..]] {
        let (r, h) = request();
        let mut paint = Paint {
            shader: crate::LinearGradient::new(
                Point::zero(),
                Point::from_xy(53.0, 0.0),
                stops(17),
                SpreadMode::Reflect,
                Transform::identity(),
            )
            .unwrap(),
            ..Paint::default()
        };
        let paid = CheckedPaint::try_from_plain(&paint, &r).unwrap();
        for &opacity in ops {
            paint.shader.apply_opacity(opacity);
        }
        let view = paid.view(&r).unwrap().with_opacity(ops);
        for _ in 0..3 {
            let mut a = RasterPipelineBuilder::new();
            let mut b =
                RasterPipelineBuilder::new_with_control(GeometryControl::request(&r)).unwrap();
            PaintView::plain(&paint).push_stages(&mut a).unwrap();
            view.push_stages(&mut b).unwrap();
            assert_eq!(
                context_bits(&a.context_mut().unwrap().gradient),
                context_bits(&b.context_mut().unwrap().gradient)
            );
        }
        let mut a = Pixmap::new(67, 2).unwrap();
        let mut b = a.clone();
        a.fill_rect(rect(67, 2), &paint, Transform::identity(), None);
        {
            let mut pix = b.as_mut();
            let mut sub = pix.as_subpixmap();
            let mut blitter = RasterPipelineBlitter::new_with_view(
                view,
                None,
                &mut sub,
                GeometryControl::request(&r),
            )
            .unwrap()
            .unwrap();
            blitter.blit_rect(&ScreenIntRect::from_xywh(0, 0, 67, 2).unwrap());
        }
        assert_eq!(a.data(), b.data());
        drop(paid);
        closed(&r, &h, false);
    }
}

#[test]
fn three_pipeline_headers_and_each_array_fail_reserve_and_commit_without_pixels() {
    for commit in [false, true] {
        for fail in 1..=12 {
            let (r, h) = request();
            let paint = checked(&r, 17, PaintOptions::default());
            let base_r = h.reserves.load(Ordering::SeqCst);
            let base_c = h.commits.load(Ordering::SeqCst);
            if commit {
                h.fail_commit.store(base_c + fail, Ordering::SeqCst);
            } else {
                h.fail_reserve.store(base_r + fail, Ordering::SeqCst);
            }
            let mut pixels = Pixmap::new(67, 3).unwrap();
            assert_eq!(
                pixels.as_mut().try_fill_rect_checked(
                    rect(67, 3),
                    &paint,
                    Transform::identity(),
                    None,
                    &r
                ),
                Err(FAULT),
                "commit={commit} fail={fail}"
            );
            assert!(pixels.data().iter().all(|v| *v == 0));
            drop(paint);
            closed(&r, &h, false);
        }
    }
    let (r, h) = request();
    let paint = checked(&r, 17, PaintOptions::default());
    h.reserve_bytes.lock().unwrap().clear();
    let base = h.reserves.load(Ordering::SeqCst);
    let mut pixels = Pixmap::new(67, 3).unwrap();
    pixels
        .as_mut()
        .try_fill_rect_checked(rect(67, 3), &paint, Transform::identity(), None, &r)
        .unwrap();
    assert_eq!(h.reserves.load(Ordering::SeqCst) - base, 12);
    let events = h.reserve_bytes.lock().unwrap();
    assert_eq!(&events[..4], &events[4..8]);
    assert_eq!(&events[..4], &events[8..12]);
    std::println!(
        "PipelineData={} align={} three header/array layouts={events:?}",
        core::mem::size_of::<PipelineData>(),
        core::mem::align_of::<PipelineData>()
    );
    drop(events);
    drop(paint);
    closed(&r, &h, false);
}

#[test]
fn natural_pipeline_array_and_header_refunds_reject_success_and_stop_next_tile() {
    for failed in 1..=12 {
        let (r, h) = request();
        let paint = checked(
            &r,
            17,
            PaintOptions {
                anti_alias: false,
                ..PaintOptions::default()
            },
        );
        h.fail_release
            .store(h.releases.load(Ordering::SeqCst) + failed, Ordering::SeqCst);
        let mut pixels = Pixmap::new(67, 3).unwrap();
        assert_eq!(
            pixels.as_mut().try_fill_rect_checked(
                rect(67, 3),
                &paint,
                Transform::identity(),
                None,
                &r
            ),
            Err(FAULT)
        );
        assert!(pixels.data().iter().any(|v| *v != 0));
        drop(paint);
        closed(&r, &h, true);
    }
    // The first two scratch refunds are sort scratch and edges. The next 12
    // are the first tile's actual pipeline arrays and metadata headers.
    for failed in 3..=14 {
        let (r, h) = request();
        let paint = checked(
            &r,
            17,
            PaintOptions {
                anti_alias: false,
                ..PaintOptions::default()
            },
        );
        let path = PathBuilder::from_rect(rect(16000, 2))
            .try_copy_in(&r, ResourceKind::Geometry)
            .unwrap();
        h.fail_release
            .store(h.releases.load(Ordering::SeqCst) + failed, Ordering::SeqCst);
        let mut pixels = Pixmap::new(16000, 2).unwrap();
        assert_eq!(
            pixels.as_mut().try_fill_path_checked(
                &path,
                &paint,
                FillRule::Winding,
                Transform::identity(),
                None,
                &r
            ),
            Err(FAULT)
        );
        assert!(
            pixels
                .data()
                .chunks_exact(16000 * 4)
                .all(|row| row[8191 * 4..].iter().all(|v| *v == 0)),
            "refund={failed}"
        );
        drop(path);
        drop(paint);
        closed(&r, &h, true);
    }
}

#[test]
fn second_memset_window_preserves_prefix_and_stops_suffix() {
    for a8 in [false, true] {
        let (r, h) = request();
        h.work_value.store(256, Ordering::SeqCst);
        h.fail_work.store(2, Ordering::SeqCst);
        if a8 {
            let mut data = vec![0u8; 1024];
            let mut sub = SubPixmapMut {
                data: &mut data,
                size: crate::IntSize::from_wh(1024, 1).unwrap(),
                real_width: 1024,
            };
            {
                let mut b = RasterPipelineBlitter::new_mask_with_control(
                    &mut sub,
                    GeometryControl::request(&r),
                )
                .unwrap()
                .unwrap();
                b.blit_rect(&ScreenIntRect::from_xywh(0, 0, 1024, 1).unwrap());
            }
            assert!(data[..256].iter().all(|v| *v == 255));
            assert!(data[256..].iter().all(|v| *v == 0));
        } else {
            let paint = CheckedPaint::try_solid(
                Color::WHITE,
                PaintOptions {
                    anti_alias: false,
                    blend_mode: BlendMode::Source,
                    ..PaintOptions::default()
                },
                &r,
            )
            .unwrap();
            let mut pix = Pixmap::new(1024, 1).unwrap();
            assert_eq!(
                pix.as_mut().try_fill_rect_checked(
                    rect(1024, 1),
                    &paint,
                    Transform::identity(),
                    None,
                    &r
                ),
                Err(FAULT)
            );
            assert!(pix.data()[..256 * 4].iter().all(|v| *v == 255));
            assert!(pix.data()[256 * 4..].iter().all(|v| *v == 0));
            drop(paint);
        }
        assert_eq!(r.failure(), Some(FAULT));
        closed(&r, &h, false);
    }
}

#[test]
fn second_long_gradient_scan_window_stops_before_first_store_in_both_precisions() {
    for hq in [false, true] {
        let (r, h) = request();
        let paint = checked(
            &r,
            601,
            PaintOptions {
                force_hq_pipeline: hq,
                ..PaintOptions::default()
            },
        );
        h.work_value.store(256, Ordering::SeqCst);
        h.work_count.store(0, Ordering::SeqCst);
        h.fail_work.store(2, Ordering::SeqCst);
        let mut pixels = Pixmap::new(19, 2).unwrap();
        assert_eq!(
            pixels.as_mut().try_fill_rect_checked(
                rect(19, 2),
                &paint,
                Transform::identity(),
                None,
                &r
            ),
            Err(FAULT)
        );
        assert!(pixels.data().iter().all(|v| *v == 0));
        drop(paint);
        closed(&r, &h, false);
    }
}

#[test]
fn transparent_rle_second_run_and_a8_second_read_window_observe_original_slot() {
    let (r, h) = request();
    let paint = CheckedPaint::try_solid(Color::WHITE, PaintOptions::default(), &r).unwrap();
    let mut pixels = Pixmap::new(6, 1).unwrap();
    {
        let mut pix = pixels.as_mut();
        let mut sub = pix.as_subpixmap();
        let mut b = RasterPipelineBlitter::new_with_view(
            paint.view(&r).unwrap(),
            None,
            &mut sub,
            GeometryControl::request(&r),
        )
        .unwrap()
        .unwrap();
        h.work_value.store(1, Ordering::SeqCst);
        h.work_count.store(0, Ordering::SeqCst);
        h.fail_work.store(2, Ordering::SeqCst);
        let mut alpha = [0, 0, 255, 0, 255, 0];
        let mut runs = [
            NonZeroU16::new(2),
            None,
            NonZeroU16::new(2),
            None,
            NonZeroU16::new(2),
            None,
            None,
        ];
        b.blit_anti_h(0, 0, &mut alpha, &mut runs);
    }
    assert_eq!(r.failure(), Some(FAULT));
    assert!(pixels.data().iter().all(|v| *v == 0));
    drop(paint);
    closed(&r, &h, false);
    let (r, h) = request();
    let paint = CheckedPaint::try_solid(Color::WHITE, PaintOptions::default(), &r).unwrap();
    h.work_value.store(256, Ordering::SeqCst);
    h.work_count.store(0, Ordering::SeqCst);
    h.fail_work.store(2, Ordering::SeqCst);
    let coverage = vec![0; 1024];
    let mask = crate::MaskRef::from_bytes(&coverage, 1024, 1).unwrap();
    let mut pixels = Pixmap::new(1024, 1).unwrap();
    assert_eq!(
        pixels
            .as_mut()
            .try_paint_coverage(mask, &paint, Transform::identity(), &r),
        Err(FAULT)
    );
    assert!(pixels.data().iter().all(|v| *v == 0));
    drop(paint);
    closed(&r, &h, false);
}

#[test]
fn checked_a8_matches_callback_coverage_and_clear_skips_invalid_gradient() {
    for width in [1, 19, 4097] {
        let (r, h) = request();
        let mut paint = Paint {
            shader: crate::LinearGradient::new(
                Point::zero(),
                Point::from_xy(53.0, 0.0),
                stops(17),
                SpreadMode::Reflect,
                Transform::identity(),
            )
            .unwrap(),
            ..Paint::default()
        };
        let checked = CheckedPaint::try_from_plain(&paint, &r).unwrap();
        let transform = Transform::from_row(0.93, 0.12, -0.08, 1.03, -2.25, 0.5);
        paint.shader.transform(transform);
        let coverage: Vec<u8> = (0..width * 3)
            .map(|i| [0, 0, 255, 255, 1, 127, 201][i as usize % 7])
            .collect();
        let mask = crate::MaskRef::from_bytes(&coverage, width, 3).unwrap();
        let mut a = Pixmap::new(width, 3).unwrap();
        let mut b = a.clone();
        a.as_mut()
            .paint_coverage_checked::<Error>(mask, &paint, |_| Ok(()))
            .unwrap();
        b.as_mut()
            .try_paint_coverage(mask, &checked, transform, &r)
            .unwrap();
        assert_eq!(a.data(), b.data(), "width={width}");
        drop(checked);
        closed(&r, &h, false);
    }
    let (r, h) = request();
    let mut plain = Paint {
        shader: crate::LinearGradient::new(
            Point::zero(),
            Point::from_xy(53.0, 0.0),
            stops(17),
            SpreadMode::Pad,
            Transform::identity(),
        )
        .unwrap(),
        blend_mode: BlendMode::Clear,
        ..Paint::default()
    };
    plain.shader.transform(Transform::from_scale(0.0, 0.0));
    let checked = CheckedPaint::try_from_plain(&plain, &r).unwrap();
    let coverage = [255; 19];
    let mask = crate::MaskRef::from_bytes(&coverage, 19, 1).unwrap();
    let mut pixels = Pixmap::new(19, 1).unwrap();
    pixels.fill(Color::WHITE);
    let before = h.reserves.load(Ordering::SeqCst);
    pixels
        .as_mut()
        .try_paint_coverage(mask, &checked, Transform::identity(), &r)
        .unwrap();
    assert_eq!(h.reserves.load(Ordering::SeqCst) - before, 3); // headers only; no gradient arrays
    assert!(pixels.data().iter().all(|v| *v == 0));
    drop(checked);
    closed(&r, &h, false);
}

#[test]
fn checked_paint_cross_request_rejects_before_allocation_and_keeps_foreign_refund_origin() {
    let (a, ah) = request();
    let (b, bh) = request();
    let stops = paid_stops(&stops(17), &a);
    assert!(CheckedPaint::try_linear(
        Point::zero(),
        Point::from_xy(1.0, 0.0),
        stops,
        SpreadMode::Pad,
        Transform::identity(),
        PaintOptions::default(),
        &b
    )
    .is_err());
    assert_eq!(bh.reserves.load(Ordering::SeqCst), 0);
    assert!(a.failure().is_none());
    closed(&a, &ah, false);
    closed(&b, &bh, false);
    let (a, ah) = request();
    let (b, bh) = request();
    let paint = checked(&a, 17, PaintOptions::default());
    assert!(paint.try_clone_in(&b).is_err());
    let mut pixels = Pixmap::new(2, 2).unwrap();
    assert!(pixels
        .as_mut()
        .try_fill_rect_checked(rect(2, 2), &paint, Transform::identity(), None, &b)
        .is_err());
    assert_eq!(bh.reserves.load(Ordering::SeqCst), 0);
    assert!(pixels.data().iter().all(|v| *v == 0));
    drop(paint);
    closed(&a, &ah, false);
    closed(&b, &bh, false);
}

#[test]
fn actual_aa_hairline_gradient_and_tiled_transform_match_all_bytes() {
    for tiled in [false, true] {
        for cap in [LineCap::Butt, LineCap::Round, LineCap::Square] {
            let (r, h) = request();
            let width = if tiled { 16000 } else { 71 };
            let mut p = PathBuilder::new();
            p.move_to(0.2, 1.3);
            p.line_to(width as f32 - 1.2, 6.7);
            p.move_to(5.2, 9.3);
            p.cubic_to(13.7, 2.3, 45.7, 15.5, 63.2, 8.5);
            let path = p.finish().unwrap();
            let paid_path = path.try_copy_in(&r, ResourceKind::Geometry).unwrap();
            let paint = Paint {
                shader: crate::LinearGradient::new(
                    Point::zero(),
                    Point::from_xy(53.0, 0.0),
                    stops(17),
                    SpreadMode::Repeat,
                    Transform::identity(),
                )
                .unwrap(),
                ..Paint::default()
            };
            let checked = CheckedPaint::try_from_plain(&paint, &r).unwrap();
            let stroke = Stroke {
                width: 0.61,
                line_cap: cap,
                ..Stroke::default()
            };
            let transform = Transform::from_row(0.99, 0.00001, -0.00002, 1.01, 0.33, -0.11);
            let mut a = Pixmap::new(width, 17).unwrap();
            let mut b = a.clone();
            a.stroke_path(&path, &paint, &stroke, transform, None);
            b.as_mut()
                .try_stroke_path_checked(&paid_path, &checked, &stroke, transform, None, &r)
                .unwrap();
            assert_eq!(a.data(), b.data(), "tiled={tiled} cap={cap:?}");
            assert!(a.data().iter().any(|v| *v != 0));
            drop(checked);
            drop(paid_path);
            closed(&r, &h, false);
        }
    }
}

#[test]
fn cps_next_stage_peeks_without_backend_dispatch_after_sticky_failure() {
    for hq in [false, true] {
        let (r, h) = request();
        h.work_value.store(1, Ordering::SeqCst);
        h.fail_work.store(1, Ordering::SeqCst);
        let mut pixels = Pixmap::new(19, 1).unwrap();
        let mut pix = pixels.as_mut();
        let mut sub = pix.as_subpixmap();
        let mut ctx = Context::default();
        let rect = ScreenIntRect::from_xywh(0, 0, 19, 1).unwrap();
        let result = if hq {
            let functions: &[highp::StageFn] = &[
                highp::fault_then_next,
                highp::after_fault_sentinel,
                highp::store,
                highp::just_return,
            ];
            highp::start(
                functions,
                functions,
                &rect,
                AAMaskCtx::default(),
                MaskCtx::default(),
                &mut ctx,
                PixmapRef::from_bytes(&[0, 0, 0, 0], 1, 1).unwrap(),
                &mut sub,
                GeometryControl::request(&r),
            )
        } else {
            let functions: &[lowp::StageFn] = &[
                lowp::fault_then_next,
                lowp::after_fault_sentinel,
                lowp::store,
                lowp::just_return,
            ];
            lowp::start(
                functions,
                functions,
                &rect,
                AAMaskCtx::default(),
                MaskCtx::default(),
                &mut ctx,
                &mut sub,
                GeometryControl::request(&r),
            )
        };
        assert_eq!(result, Err(FAULT));
        assert_eq!(ctx.current_coverage, 0.0);
        assert_eq!(&*h.works.lock().unwrap(), &[0, if hq { 12 } else { 20 }, 1]);
        assert!(pixels.data().iter().all(|v| *v == 0));
        closed(&r, &h, false);
    }
}

#[test]
fn full_and_tail_vectors_pay_complete_lane_width_before_dispatch() {
    for hq in [false, true] {
        let (r, h) = request();
        let width = if hq {
            highp::STAGE_WIDTH
        } else {
            lowp::STAGE_WIDTH
        };
        let mut pixels = Pixmap::new((width * 2 + 3) as u32, 2).unwrap();
        let mut p = RasterPipelineBuilder::new_with_control(GeometryControl::request(&r)).unwrap();
        p.set_force_hq_pipeline(hq);
        p.push_uniform_color(Color::WHITE.premultiply());
        p.push(Stage::Store);
        let mut p = p.compile().unwrap();
        h.works.lock().unwrap().clear();
        p.run(
            &ScreenIntRect::from_xywh(0, 0, (width * 2 + 3) as u32, 2).unwrap(),
            AAMaskCtx::default(),
            MaskCtx::default(),
            PixmapRef::from_bytes(&[0, 0, 0, 0], 1, 1).unwrap(),
            &mut pixels.as_mut().as_subpixmap(),
        )
        .unwrap();
        let paid: Vec<u64> = h
            .works
            .lock()
            .unwrap()
            .iter()
            .copied()
            .filter(|v| *v != 0)
            .collect();
        assert_eq!(paid, vec![(3 + width) as u64; 6]);
        assert!(pixels.data().iter().all(|v| *v == 255));
        drop(p);
        closed(&r, &h, false);
    }
}

#[test]
fn empty_compile_and_degenerate_stop_retirement_keep_natural_refund_error() {
    let (r, h) = request();
    let mut p = RasterPipelineBuilder::new_with_control(GeometryControl::request(&r)).unwrap();
    p.context_mut()
        .unwrap()
        .gradient
        .factors
        .reserve(17)
        .unwrap();
    h.fail_release.store(1, Ordering::SeqCst);
    assert!(matches!(p.compile(), Err(FAULT)));
    closed(&r, &h, true);
    for points in [vec![], vec![GradientStop::new(0.0, Color::WHITE)]] {
        let (r, h) = request();
        let stops = paid_stops(&points, &r);
        h.fail_release.store(1, Ordering::SeqCst);
        assert!(matches!(
            CheckedPaint::try_linear(
                Point::zero(),
                Point::from_xy(1.0, 0.0),
                stops,
                SpreadMode::Pad,
                Transform::identity(),
                PaintOptions::default(),
                &r
            ),
            Err(FAULT)
        ));
        closed(&r, &h, true);
    }
}

#[test]
fn pattern_draw_and_checked_apply_mask_preserve_all_bytes_and_refund_tail() {
    let mut source = Pixmap::new(3, 3).unwrap();
    for (i, p) in source.pixels_mut().iter_mut().enumerate() {
        *p = Color::from_rgba8((i * 27) as u8, 101, (i * 39) as u8, 191)
            .premultiply()
            .to_color_u8();
    }
    for quality in [
        crate::FilterQuality::Nearest,
        crate::FilterQuality::Bilinear,
        crate::FilterQuality::Bicubic,
    ] {
        let (r, h) = request();
        let mut a = Pixmap::new(67, 31).unwrap();
        let mut b = a.clone();
        let paint = crate::PixmapPaint {
            opacity: 0.713,
            quality,
            ..crate::PixmapPaint::default()
        };
        let transform = Transform::from_row(9.13, 0.12, -0.17, 8.77, 1.23, 0.73);
        a.draw_pixmap(0, 0, source.as_ref(), &paint, transform, None);
        b.try_draw_pixmap(0, 0, source.as_ref(), &paint, transform, None, &r)
            .unwrap();
        assert_eq!(a.data(), b.data());
        let values: Vec<u8> = (0..67 * 31).map(|i| (i * 13) as u8).collect();
        let mask = crate::MaskRef::from_bytes(&values, 67, 31).unwrap();
        a.as_mut().apply_mask_ref(mask);
        b.as_mut().try_apply_mask_ref(mask, &r).unwrap();
        assert_eq!(a.data(), b.data());
        closed(&r, &h, false);
    }
    let (r, h) = request();
    let mask_values = [127; 19];
    let mask = crate::MaskRef::from_bytes(&mask_values, 19, 1).unwrap();
    let mut pixels = Pixmap::new(19, 1).unwrap();
    pixels.fill(Color::WHITE);
    h.fail_release.store(1, Ordering::SeqCst);
    assert_eq!(pixels.as_mut().try_apply_mask_ref(mask, &r), Err(FAULT));
    assert!(pixels.data().iter().all(|v| *v == 127));
    closed(&r, &h, true);
}

struct FaultingBlitter<'a> {
    resources: &'a ResourcesRef,
    calls: usize,
    calls_after_fault: usize,
}
impl FaultingBlitter<'_> {
    fn visit(&mut self) {
        if self.resources.failure().is_some() {
            self.calls_after_fault += 1;
        }
        self.calls += 1;
        self.resources.record(FAULT);
    }
}
impl Blitter for FaultingBlitter<'_> {
    fn blit_h(&mut self, _: u32, _: u32, _: crate::LengthU32) {
        self.visit();
    }
    fn blit_anti_h(&mut self, _: u32, _: u32, _: &mut [u8], _: &mut [crate::alpha_runs::AlphaRun]) {
        self.visit();
    }
    fn blit_v(&mut self, _: u32, _: u32, _: crate::LengthU32, _: u8) {
        self.visit();
    }
    fn blit_anti_h2(&mut self, _: u32, _: u32, _: u8, _: u8) {
        self.visit();
    }
    fn blit_anti_v2(&mut self, _: u32, _: u32, _: u8, _: u8) {
        self.visit();
    }
    fn blit_rect(&mut self, _: &ScreenIntRect) {
        self.visit();
    }
}
#[test]
fn all_four_aa_hairline_adapters_stop_after_nested_blit_failure() {
    for (start, end) in [
        (Point::from_xy(2.3, 12.3), Point::from_xy(61.7, 12.3)),
        (Point::from_xy(2.3, 12.3), Point::from_xy(61.7, 26.7)),
        (Point::from_xy(12.3, 2.3), Point::from_xy(12.3, 61.7)),
        (Point::from_xy(12.3, 2.3), Point::from_xy(26.7, 61.7)),
    ] {
        let (r, h) = request();
        let mut path = PathBuilder::new();
        path.move_to(start.x, start.y);
        path.line_to(end.x, end.y);
        let path = path.finish().unwrap();
        let mut blitter = FaultingBlitter {
            resources: &r,
            calls: 0,
            calls_after_fault: 0,
        };
        assert_eq!(
            crate::scan::hairline_aa::stroke_path_with_control(
                &path,
                LineCap::Butt,
                &ScreenIntRect::from_xywh(0, 0, 64, 64).unwrap(),
                &mut blitter,
                GeometryControl::request(&r)
            ),
            Err(FAULT)
        );
        assert_eq!(blitter.calls, 1);
        assert_eq!(blitter.calls_after_fault, 0);
        closed(&r, &h, false);
    }
}
