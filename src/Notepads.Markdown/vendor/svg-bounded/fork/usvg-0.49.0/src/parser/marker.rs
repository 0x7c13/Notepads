// Copyright 2018 the Resvg Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT
// Modifications Copyright (c) 2026, Jiaqi (0x7c13) Liu.
// Experimental request-owned error propagation; original license terms retained.

use crate::tree::owner::TryCopy;
use markdown_svg_budget::{BudgetVec, Error, ResourceKind};
#[cfg(test)]
use std::sync::Arc;

use strict_num::NonZeroPositiveF32;
use svgtypes::Length;
use tiny_skia_path::Point;

use super::converter;
use super::svgtree::{AId, EId, SvgNode};
use crate::{
    ApproxEqUlps, ApproxZeroUlps, ClipPath, Fill, Group, Node, NonZeroRect, Transform, ViewBox,
};

// Similar to `tiny_skia_path::PathSegment`, but without the `QuadTo`.
#[derive(Copy, Clone, Debug)]
enum Segment {
    MoveTo(Point),
    LineTo(Point),
    CubicTo(Point, Point, Point),
    Close,
}

enum SegmentList {
    Compat(Vec<Segment>),
    Paid(BudgetVec<Segment>),
}
impl SegmentList {
    fn new(capacity: usize, processing: &crate::ProcessingContext) -> Result<Self, Error> {
        match processing.resources() {
            Some(resources) => {
                let mut list = BudgetVec::new(resources.clone(), ResourceKind::Scratch);
                list.try_reserve_exact(capacity)?;
                Ok(Self::Paid(list))
            }
            None => Ok(Self::Compat(Vec::with_capacity(capacity))),
        }
    }
    fn push(&mut self, segment: Segment) -> Result<(), Error> {
        match self {
            Self::Compat(values) => {
                values.push(segment);
                Ok(())
            }
            Self::Paid(values) => values.try_push(segment),
        }
    }
}
impl core::ops::Deref for SegmentList {
    type Target = [Segment];
    fn deref(&self) -> &[Segment] {
        match self {
            Self::Compat(values) => values,
            Self::Paid(values) => values,
        }
    }
}

pub(crate) fn is_valid(node: SvgNode) -> bool {
    // `marker-*` attributes cannot be set on shapes inside a `clipPath`.
    if node
        .ancestors()
        .any(|n| n.tag_name() == Some(EId::ClipPath))
    {
        return false;
    }

    let start = node.find_attribute::<SvgNode>(AId::MarkerStart);
    let mid = node.find_attribute::<SvgNode>(AId::MarkerMid);
    let end = node.find_attribute::<SvgNode>(AId::MarkerEnd);
    start.is_some() || mid.is_some() || end.is_some()
}

pub(crate) fn convert(
    node: SvgNode,
    path: &tiny_skia_path::Path,
    state: &converter::State,
    cache: &mut converter::Cache,
    parent: &mut Group,
) {
    let list = [
        (AId::MarkerStart, MarkerKind::Start),
        (AId::MarkerMid, MarkerKind::Middle),
        (AId::MarkerEnd, MarkerKind::End),
    ];

    for (aid, kind) in &list {
        if state.opt.processing.failed() {
            return;
        }
        let mut marker = None;
        if let Some(link) = node.find_attribute::<SvgNode>(*aid) {
            if link.tag_name() == Some(EId::Marker) {
                marker = Some(link);
            }
        }

        if let Some(marker) = marker {
            // TODO: move to svgtree
            // Check for recursive marker.
            let mut recursive = false;
            for parent_marker in &state.parent_markers {
                if super::shapes::checkpoint(&state.opt.processing, 1).is_err() {
                    return;
                }
                if *parent_marker == marker {
                    recursive = true;
                    break;
                }
            }
            if recursive && state.opt.processing.resources().is_some() {
                state.opt.processing.record_resource(Error::new(
                    markdown_svg_budget::FailureKind::Decode,
                    markdown_svg_budget::LimitKind::None,
                ));
                return;
            }
            if recursive {
                log::warn!("Recursive marker detected: {}", marker.element_id());
                continue;
            }

            let Some(result) = cache.with_reference(marker, state, |state, cache| {
                Some(resolve(node, path, marker, *kind, state, cache, parent))
            }) else {
                return;
            };
            if let Err(error) = result {
                state.opt.processing.record_resource(error);
                return;
            }
        }
    }
}

#[derive(Clone, Copy)]
enum MarkerKind {
    Start,
    Middle,
    End,
}

enum MarkerOrientation {
    Auto,
    AutoStartReverse,
    Angle(f32),
}

fn resolve(
    shape_node: SvgNode,
    path: &tiny_skia_path::Path,
    marker_node: SvgNode,
    marker_kind: MarkerKind,
    state: &converter::State,
    cache: &mut converter::Cache,
    parent: &mut Group,
) -> Result<Option<()>, Error> {
    let processing = &state.opt.processing;
    super::shapes::checkpoint(processing, 0)?;
    let candidate = (|| {
        let Some(stroke_scale) = stroke_scale(shape_node, marker_node, state) else {
            return Ok(None);
        };
        let stroke_scale = stroke_scale.get();

        let Some(r) = convert_rect(marker_node, state) else {
            return Ok(None);
        };

        let view_box_ts = marker_node
            .parse_viewbox()
            .map(|vb| {
                ViewBox {
                    rect: vb,
                    aspect: marker_node
                        .attribute(AId::PreserveAspectRatio)
                        .unwrap_or_default(),
                }
                .to_transform(r.size())
            })
            .unwrap_or_default();

        let has_overflow = {
            let overflow = marker_node.attribute(AId::Overflow);
            // `overflow` is `hidden` by default.
            overflow.is_none() || overflow == Some("hidden") || overflow == Some("scroll")
        };

        let clip_path = if has_overflow {
            let Some(inverse) = view_box_ts.invert() else {
                return Ok(None);
            };
            let Some(clip_rect) = r.size().to_non_zero_rect(0.0, 0.0).transform(inverse) else {
                return Ok(None);
            };

            let root = crate::scene::root(processing, || Group::empty_in(processing))?;
            let mut clip_path = ClipPath::empty(cache.gen_clip_path_id()?);
            clip_path.root = root;

            let Some(mut path) = super::shapes::rect_to_node_path(clip_rect.to_rect(), processing)?
            else {
                return Ok(None);
            };
            path.fill = Some(Fill::default());

            let cost = crate::scene::SceneCost::path(&path);
            let path = crate::scene::boxed_owned(processing, cost, path)?;
            clip_path
                .root
                .children
                .try_push(Node::Path(path), processing)?;
            clip_path.root.calculate_bounding_boxes();

            Some(crate::SharedOwner::try_new(
                clip_path,
                processing.resources(),
                ResourceKind::Scratch,
            )?)
        } else {
            None
        };

        let mut segments = SegmentList::new(path.len(), processing)?;
        let mut prev = Point::zero();
        let mut prev_move = Point::zero();
        let mut path_segments = path.segments();
        loop {
            super::shapes::checkpoint(processing, 1)?;
            let Some(seg) = path_segments.next() else {
                break;
            };
            match seg {
                tiny_skia_path::PathSegment::MoveTo(p) => {
                    segments.push(Segment::MoveTo(p))?;
                    prev = p;
                    prev_move = p;
                }
                tiny_skia_path::PathSegment::LineTo(p) => {
                    segments.push(Segment::LineTo(p))?;
                    prev = p;
                }
                tiny_skia_path::PathSegment::QuadTo(p1, p) => {
                    let (p1, p2, p) = quad_to_curve(prev, p1, p);
                    segments.push(Segment::CubicTo(p1, p2, p))?;
                    prev = p;
                }
                tiny_skia_path::PathSegment::CubicTo(p1, p2, p) => {
                    segments.push(Segment::CubicTo(p1, p2, p))?;
                    prev = p;
                }
                tiny_skia_path::PathSegment::Close => {
                    segments.push(Segment::Close)?;
                    prev = prev_move;
                }
            }
        }

        let draw_marker = |p: tiny_skia_path::Point, idx: usize| {
            super::shapes::checkpoint(processing, 0)?;
            let mut ts = Transform::from_translate(p.x, p.y);

            let angle = match convert_orientation(marker_node) {
                MarkerOrientation::AutoStartReverse if idx == 0 => {
                    (calc_vertex_angle(&segments, idx, processing)? + 180.0) % 360.0
                }
                MarkerOrientation::Auto | MarkerOrientation::AutoStartReverse => {
                    calc_vertex_angle(&segments, idx, processing)?
                }
                MarkerOrientation::Angle(angle) => angle,
            };

            if !angle.approx_zero_ulps(4) {
                ts = ts.pre_rotate(angle);
            }

            let (sx, sy) = view_box_ts.get_scale();
            ts = ts.pre_scale(sx * stroke_scale, sy * stroke_scale);
            ts = ts.pre_translate(-r.x(), -r.y());

            // TODO: do not create a group when no clipPath
            let mut g = Group {
                transform: ts,
                abs_transform: parent.abs_transform.pre_concat(ts),
                clip_path: clip_path.try_copy_in(processing)?,
                ..Group::empty_in(processing)
            };

            let mut marker_state = state.try_clone()?;
            marker_state
                .parent_markers
                .try_push(marker_node, processing)?;
            converter::convert_children(marker_node, &marker_state, cache, &mut g);
            super::shapes::checkpoint(processing, 0)?;
            state.opt.processing.observe_marker_bbox();
            g.calculate_bounding_boxes();

            if g.has_children() {
                let group =
                    crate::scene::boxed_owned(processing, crate::scene::SceneCost::NODE, g)?;
                parent.children.try_push(Node::Group(group), processing)?;
            }
            Ok(())
        };

        draw_markers(&segments, marker_kind, processing, draw_marker)?;

        Ok(Some(()))
    })();
    // The segment table and non-result clip/marker aliases have retired.
    super::shapes::checkpoint(processing, 0)?;
    candidate.map_err(|error| processing.record_resource(error))
}

fn stroke_scale(
    path_node: SvgNode,
    marker_node: SvgNode,
    state: &converter::State,
) -> Option<NonZeroPositiveF32> {
    match marker_node.attribute(AId::MarkerUnits) {
        Some("userSpaceOnUse") => NonZeroPositiveF32::new(1.0),
        _ => path_node.resolve_valid_length(AId::StrokeWidth, state, 1.0),
    }
}

fn draw_markers<P>(
    path: &[Segment],
    kind: MarkerKind,
    processing: &crate::ProcessingContext,
    mut draw_marker: P,
) -> Result<(), Error>
where
    P: FnMut(Point, usize) -> Result<(), Error>,
{
    super::shapes::checkpoint(processing, 1)?;
    match kind {
        MarkerKind::Start => {
            if let Some(Segment::MoveTo(p)) = path.first().copied() {
                draw_marker(p, 0)?;
            }
        }
        MarkerKind::Middle => {
            let total = path.len() - 1;
            let mut i = 1;
            while i < total {
                super::shapes::checkpoint(processing, 1)?;
                let p = match path[i] {
                    Segment::MoveTo(p) | Segment::LineTo(p) | Segment::CubicTo(_, _, p) => p,
                    _ => {
                        i += 1;
                        continue;
                    }
                };
                draw_marker(p, i)?;
                i += 1;
            }
        }
        MarkerKind::End => {
            let idx = path.len() - 1;
            match path.last().copied() {
                Some(Segment::LineTo(p)) | Some(Segment::CubicTo(_, _, p)) => draw_marker(p, idx)?,
                Some(Segment::Close) => {
                    draw_marker(get_subpath_start(path, idx, processing)?, idx)?
                }
                _ => {}
            }
        }
    }
    super::shapes::checkpoint(processing, 0)
}

fn calc_vertex_angle(
    path: &[Segment],
    idx: usize,
    processing: &crate::ProcessingContext,
) -> Result<f32, Error> {
    super::shapes::checkpoint(processing, 1)?;
    let angle = if idx == 0 {
        // First segment.

        debug_assert!(path.len() > 1);

        let seg1 = path[0];
        let seg2 = path[1];

        match (seg1, seg2) {
            (Segment::MoveTo(pm), Segment::LineTo(p)) => calc_line_angle(pm.x, pm.y, p.x, p.y),
            (Segment::MoveTo(pm), Segment::CubicTo(p1, _, p)) => {
                if pm.x.approx_eq_ulps(&p1.x, 4) && pm.y.approx_eq_ulps(&p1.y, 4) {
                    calc_line_angle(pm.x, pm.y, p.x, p.y)
                } else {
                    calc_line_angle(pm.x, pm.y, p1.x, p1.y)
                }
            }
            _ => 0.0,
        }
    } else if idx == path.len() - 1 {
        // Last segment.

        let seg1 = path[idx - 1];
        let seg2 = path[idx];

        match (seg1, seg2) {
            (_, Segment::MoveTo(_)) => 0.0, // unreachable
            (_, Segment::LineTo(p)) => {
                let prev = get_prev_vertex(path, idx, processing)?;
                calc_line_angle(prev.x, prev.y, p.x, p.y)
            }
            (_, Segment::CubicTo(p1, p2, p)) => {
                if p2.x.approx_eq_ulps(&p.x, 4) && p2.y.approx_eq_ulps(&p.y, 4) {
                    calc_line_angle(p1.x, p1.y, p.x, p.y)
                } else {
                    calc_line_angle(p2.x, p2.y, p.x, p.y)
                }
            }
            (Segment::LineTo(p), Segment::Close) => {
                let next = get_subpath_start(path, idx, processing)?;
                calc_line_angle(p.x, p.y, next.x, next.y)
            }
            (Segment::CubicTo(_, p2, p), Segment::Close) => {
                let prev = get_prev_vertex(path, idx, processing)?;
                let next = get_subpath_start(path, idx, processing)?;
                calc_curves_angle(
                    prev.x, prev.y, p2.x, p2.y, p.x, p.y, next.x, next.y, next.x, next.y,
                )
            }
            (_, Segment::Close) => 0.0,
        }
    } else {
        // Middle segments.

        let seg1 = path[idx];
        let seg2 = path[idx + 1];

        // TODO: Not sure if there is a better way.
        match (seg1, seg2) {
            (Segment::MoveTo(pm), Segment::LineTo(p)) => calc_line_angle(pm.x, pm.y, p.x, p.y),
            (Segment::MoveTo(pm), Segment::CubicTo(p1, _, _)) => {
                calc_line_angle(pm.x, pm.y, p1.x, p1.y)
            }
            (Segment::LineTo(p1), Segment::LineTo(p2)) => {
                let prev = get_prev_vertex(path, idx, processing)?;
                calc_angle(prev.x, prev.y, p1.x, p1.y, p1.x, p1.y, p2.x, p2.y)
            }
            (Segment::CubicTo(_, c1_p2, c1_p), Segment::CubicTo(c2_p1, _, c2_p)) => {
                let prev = get_prev_vertex(path, idx, processing)?;
                calc_curves_angle(
                    prev.x, prev.y, c1_p2.x, c1_p2.y, c1_p.x, c1_p.y, c2_p1.x, c2_p1.y, c2_p.x,
                    c2_p.y,
                )
            }
            (Segment::LineTo(pl), Segment::CubicTo(p1, _, p)) => {
                let prev = get_prev_vertex(path, idx, processing)?;
                calc_curves_angle(
                    prev.x, prev.y, prev.x, prev.y, pl.x, pl.y, p1.x, p1.y, p.x, p.y,
                )
            }
            (Segment::CubicTo(_, p2, p), Segment::LineTo(pl)) => {
                let prev = get_prev_vertex(path, idx, processing)?;
                calc_curves_angle(prev.x, prev.y, p2.x, p2.y, p.x, p.y, pl.x, pl.y, pl.x, pl.y)
            }
            (Segment::LineTo(p), Segment::MoveTo(_)) => {
                let prev = get_prev_vertex(path, idx, processing)?;
                calc_line_angle(prev.x, prev.y, p.x, p.y)
            }
            (Segment::CubicTo(_, p2, p), Segment::MoveTo(_)) => {
                if p.x.approx_eq_ulps(&p2.x, 4) && p.y.approx_eq_ulps(&p2.y, 4) {
                    let prev = get_prev_vertex(path, idx, processing)?;
                    calc_line_angle(prev.x, prev.y, p.x, p.y)
                } else {
                    calc_line_angle(p2.x, p2.y, p.x, p.y)
                }
            }
            (Segment::LineTo(p), Segment::Close) => {
                let prev = get_prev_vertex(path, idx, processing)?;
                let next = get_subpath_start(path, idx, processing)?;
                calc_angle(prev.x, prev.y, p.x, p.y, p.x, p.y, next.x, next.y)
            }
            (_, Segment::Close) => {
                let prev = get_prev_vertex(path, idx, processing)?;
                let next = get_subpath_start(path, idx, processing)?;
                calc_line_angle(prev.x, prev.y, next.x, next.y)
            }
            (_, Segment::MoveTo(_)) | (Segment::Close, _) => 0.0,
        }
    };
    Ok(angle)
}

fn calc_line_angle(x1: f32, y1: f32, x2: f32, y2: f32) -> f32 {
    calc_angle(x1, y1, x2, y2, x1, y1, x2, y2)
}

fn calc_curves_angle(
    px: f32,
    py: f32, // previous vertex
    cx1: f32,
    cy1: f32, // previous control point
    x: f32,
    y: f32, // current vertex
    cx2: f32,
    cy2: f32, // next control point
    nx: f32,
    ny: f32, // next vertex
) -> f32 {
    if cx1.approx_eq_ulps(&x, 4) && cy1.approx_eq_ulps(&y, 4) {
        calc_angle(px, py, x, y, x, y, cx2, cy2)
    } else if x.approx_eq_ulps(&cx2, 4) && y.approx_eq_ulps(&cy2, 4) {
        calc_angle(cx1, cy1, x, y, x, y, nx, ny)
    } else {
        calc_angle(cx1, cy1, x, y, x, y, cx2, cy2)
    }
}

fn calc_angle(x1: f32, y1: f32, x2: f32, y2: f32, x3: f32, y3: f32, x4: f32, y4: f32) -> f32 {
    use std::f32::consts::*;

    fn normalize(rad: f32) -> f32 {
        let v = rad % (PI * 2.0);
        if v < 0.0 { v + PI * 2.0 } else { v }
    }

    fn vector_angle(vx: f32, vy: f32) -> f32 {
        let rad = vy.atan2(vx);
        if rad.is_nan() { 0.0 } else { normalize(rad) }
    }

    let in_a = vector_angle(x2 - x1, y2 - y1);
    let out_a = vector_angle(x4 - x3, y4 - y3);
    let d = (out_a - in_a) * 0.5;

    let mut angle = in_a + d;
    if FRAC_PI_2 < d.abs() {
        angle -= PI;
    }

    normalize(angle).to_degrees()
}

fn get_subpath_start(
    segments: &[Segment],
    idx: usize,
    processing: &crate::ProcessingContext,
) -> Result<Point, Error> {
    let offset = segments.len() - idx;
    let mut reversed = segments.iter().rev().skip(offset);
    loop {
        super::shapes::checkpoint(processing, 1)?;
        let Some(seg) = reversed.next() else {
            return Ok(Point::zero());
        };
        if let Segment::MoveTo(p) = *seg {
            return Ok(p);
        }
    }
}
fn get_prev_vertex(
    segments: &[Segment],
    idx: usize,
    processing: &crate::ProcessingContext,
) -> Result<Point, Error> {
    super::shapes::checkpoint(processing, 1)?;
    match segments[idx - 1] {
        Segment::MoveTo(p) | Segment::LineTo(p) | Segment::CubicTo(_, _, p) => Ok(p),
        Segment::Close => get_subpath_start(segments, idx, processing),
    }
}

fn convert_rect(node: SvgNode, state: &converter::State) -> Option<NonZeroRect> {
    NonZeroRect::from_xywh(
        node.convert_user_length(AId::RefX, state, Length::zero()),
        node.convert_user_length(AId::RefY, state, Length::zero()),
        node.convert_user_length(AId::MarkerWidth, state, Length::new_number(3.0)),
        node.convert_user_length(AId::MarkerHeight, state, Length::new_number(3.0)),
    )
}

fn convert_orientation(node: SvgNode) -> MarkerOrientation {
    match node.attribute(AId::Orient) {
        Some("auto") => MarkerOrientation::Auto,
        Some("auto-start-reverse") => MarkerOrientation::AutoStartReverse,
        _ => match node.attribute::<svgtypes::Angle>(AId::Orient) {
            Some(angle) => MarkerOrientation::Angle(angle.to_degrees() as f32),
            None => MarkerOrientation::Angle(0.0),
        },
    }
}

fn quad_to_curve(prev: Point, p1: Point, p: Point) -> (Point, Point, Point) {
    #[inline]
    fn calc(n1: f32, n2: f32) -> f32 {
        (n1 + n2 * 2.0) / 3.0
    }

    (
        Point::from_xy(calc(prev.x, p1.x), calc(prev.y, p1.y)),
        Point::from_xy(calc(p.x, p1.x), calc(p.y, p1.y)),
        p,
    )
}

#[cfg(test)]
mod geometry_control_tests {
    use super::*;
    use markdown_svg_budget::{Backend, FirstFailureRef, Limits, ResourcesRef, Ticket};
    use std::sync::{
        Mutex,
        atomic::{AtomicUsize, Ordering},
    };

    #[derive(Default)]
    struct Track {
        next: AtomicUsize,
        live: AtomicUsize,
        calls: AtomicUsize,
        stop: AtomicUsize,
        target_bytes: usize,
        target_slot: AtomicUsize,
        target_count: AtomicUsize,
        failed: Mutex<Option<Ticket>>,
    }
    struct TestBackend(Arc<Track>);
    impl Backend for TestBackend {
        fn reserve(&self, kind: ResourceKind, bytes: usize) -> Result<Ticket, Error> {
            let slot = self.0.next.fetch_add(1, Ordering::Relaxed) + 1;
            self.0.live.fetch_add(bytes, Ordering::Relaxed);
            if kind == ResourceKind::Scratch && bytes == self.0.target_bytes {
                self.0.target_count.fetch_add(1, Ordering::Relaxed);
                self.0.target_slot.store(slot, Ordering::Relaxed);
            }
            Ticket::new(bytes as u64, slot as u32, 1)
        }
        fn allocation_committed(&self, _: &Ticket, _: usize) -> Result<(), Error> {
            Ok(())
        }
        fn release_after_free(&self, ticket: Ticket) -> Result<(), Error> {
            if ticket.slot() as usize == self.0.target_slot.load(Ordering::Relaxed) {
                assert!(self.0.failed.lock().unwrap().replace(ticket).is_none());
                return Err(Error::native(-973));
            }
            self.0
                .live
                .fetch_sub(ticket.account_nonce() as usize, Ordering::Relaxed);
            Ok(())
        }
        fn checkpoint(&self, _: u64) -> Result<(), Error> {
            let call = self.0.calls.fetch_add(1, Ordering::Relaxed) + 1;
            if call == self.0.stop.load(Ordering::Relaxed) {
                Err(Error::native(-972))
            } else {
                Ok(())
            }
        }
    }
    fn request(target_bytes: usize) -> (ResourcesRef, Arc<Track>) {
        let track = Arc::new(Track {
            target_bytes,
            ..Track::default()
        });
        let resources = ResourcesRef::new_prepaid(
            TestBackend(track.clone()),
            FirstFailureRef::new_prepaid(),
            Limits::default(),
        );
        (resources, track)
    }

    #[test]
    fn marker_segment_storage_is_paid_and_reverse_scan_rejects_before_the_257th_payload() {
        let (resources, track) = request(0);
        {
            let processing = crate::ProcessingContext::with_resources(resources.clone());
            let mut list = SegmentList::new(1002, &processing).unwrap();
            list.push(Segment::MoveTo(Point::from_xy(3.0, 4.0)))
                .unwrap();
            for index in 1..=1000 {
                list.push(Segment::LineTo(Point::from_xy(index as f32, 8.0)))
                    .unwrap();
            }
            list.push(Segment::Close).unwrap();
            assert_eq!(
                resources.statistics().scratch_bytes,
                (1002 * size_of::<Segment>()) as u64
            );
            track.calls.store(0, Ordering::Relaxed);
            assert_eq!(
                get_subpath_start(&list, 1001, &processing).unwrap(),
                Point::from_xy(3.0, 4.0)
            );
            assert_eq!(track.calls.load(Ordering::Relaxed), 1001);
            track.calls.store(0, Ordering::Relaxed);
            track.stop.store(257, Ordering::Relaxed);
            assert_eq!(
                get_subpath_start(&list, 1001, &processing),
                Err(Error::native(-972))
            );
            assert_eq!(track.calls.load(Ordering::Relaxed), 257);
        }
        assert_eq!(track.live.load(Ordering::Relaxed), 0);
        resources.validate_retired().unwrap();
    }

    #[test]
    fn marker_angles_keep_original_order_bits_and_basic_geometric_directions() {
        let segments = [
            Segment::MoveTo(Point::from_xy(0.0, 0.0)),
            Segment::LineTo(Point::from_xy(10.0, 0.0)),
            Segment::LineTo(Point::from_xy(10.0, 10.0)),
            Segment::Close,
        ];
        let plain = crate::ProcessingContext::default();
        let (resources, track) = request(0);
        {
            let controlled = crate::ProcessingContext::with_resources(resources.clone());
            for (index, target) in [0.0f32, 45.0, 157.5, 225.0].into_iter().enumerate() {
                let ordinary = calc_vertex_angle(&segments, index, &plain).unwrap();
                let paid = calc_vertex_angle(&segments, index, &controlled).unwrap();
                assert_eq!(ordinary.to_bits(), paid.to_bits());
                assert!((ordinary - target).abs() < 0.001);
            }
        }
        assert!(track.calls.load(Ordering::Relaxed) > 4);
        resources.validate_retired().unwrap();
    }

    #[test]
    fn marker_resolve_cannot_return_a_candidate_after_its_segment_table_release_fails() {
        let mut d = String::from("M0 0");
        for index in 1..=1000 {
            use std::fmt::Write;
            write!(d, "L{} {}", index, index % 7).unwrap();
        }
        d.push('z');
        let source = format!(
            "<svg xmlns='http://www.w3.org/2000/svg'><defs><marker id='m' markerWidth='4' markerHeight='4'><path d='M0 0L4 2L0 4z'/></marker></defs><path id='s' d='{d}'/></svg>"
        );
        let bytes = 1002 * size_of::<Segment>();
        let (resources, track) = request(bytes);
        let xml = roxmltree::Document::parse(&source).unwrap();
        let document = super::super::svgtree::Document::parse_tree_for_source(
            &xml,
            None,
            super::super::source_kind::SourceKind::GeneralImage,
            Some(resources.clone()),
        )
        .unwrap();
        let shape = document
            .root()
            .descendants()
            .find(|node| node.element_id() == "s")
            .unwrap();
        let marker = document
            .root()
            .descendants()
            .find(|node| node.element_id() == "m")
            .unwrap();
        {
            let opt = super::super::Options {
                processing: Arc::new(crate::ProcessingContext::with_resources(resources.clone())),
                ..super::super::Options::default()
            };
            let state = converter::State {
                refs: crate::parser::reference::RefPath::default(),
                context_proofs: [crate::parser::reference::PaintProof::default(); 2],
                opt: &opt,
                parent_clip_path: None,
                parent_markers: crate::tree::owner::TreeVec::new(&opt.processing),
                context_element: None,
                fe_image_link: false,
                view_box: NonZeroRect::from_xywh(0.0, 0.0, 100.0, 100.0).unwrap(),
                use_size: (None, None),
            };
            let path = super::super::shapes::convert(shape, &state)
                .unwrap()
                .unwrap();
            assert_eq!(path.len(), 1002);
            #[cfg(feature = "text")]
            let mut cache =
                converter::Cache::with_processing(opt.processing.clone(), opt.fontdb.clone());
            #[cfg(not(feature = "text"))]
            let mut cache = converter::Cache::with_processing(opt.processing.clone());

            let mut parent = Group::empty_in(&opt.processing);
            let mut candidate_consumed = false;
            let result = resolve(
                shape,
                &path,
                marker,
                MarkerKind::Start,
                &state,
                &mut cache,
                &mut parent,
            );
            if matches!(result, Ok(Some(()))) {
                candidate_consumed = true;
            }
            assert!(!candidate_consumed);
            assert_eq!(result, Err(Error::native(-973)));
            assert_eq!(opt.processing.marker_post_counts(), [1, 0]);
        }
        drop(document);
        assert_eq!(track.target_count.load(Ordering::Relaxed), 1);
        assert_eq!(resources.failure(), Some(Error::native(-973)));
        assert_eq!(resources.statistics().release_failures, 1);
        assert_eq!(track.live.load(Ordering::Relaxed), bytes);
        assert!(track.failed.lock().unwrap().is_some());
        assert!(resources.validate_retired().is_err());
    }
}
