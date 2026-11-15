// Copyright 2006 The Android Open Source Project
// Copyright 2020 Yevhenii Reizner
// Modifications Copyright (c) 2026, Jiaqi (0x7c13) Liu.
//
// Use of this source code is governed by a BSD-style license that can be
// found in the LICENSE file.

use crate::path_builder::PathBuilder;
use crate::storage::{Control, GeometryStorage, Result};
use crate::transform::Transform;
use crate::{Point, Rect};
use markdown_svg_budget::{ResourceKind, ResourcesRef};

/// A path verb.
#[allow(missing_docs)]
#[derive(Copy, Clone, Eq, PartialEq, Ord, PartialOrd, Debug, Hash)]
pub enum PathVerb {
    Move,
    Line,
    Quad,
    Cubic,
    Close,
}

/// An immutable finite Bezier path with precomputed bounds.
/// Paid clones share both arrays; checked mutation preserves value semantics.
#[derive(Clone, PartialEq)]
pub struct Path {
    pub(crate) storage: GeometryStorage,
    pub(crate) bounds: Rect,
}

impl Path {
    /// Borrow the primary source policy only. Call `validate_policy` before
    /// import: this marker alone does not validate header or array ownership.
    pub fn request_resources(&self) -> Option<&ResourcesRef> {
        self.storage.resources()
    }

    /// Validate every private array owner against a scene's source policy.
    /// This is stricter than the external compatibility geometry adapter:
    /// plain paths cannot silently become a request-owned scene field.
    pub fn validate_policy(
        &self,
        target: Option<&ResourcesRef>,
    ) -> core::result::Result<(), markdown_svg_budget::Error> {
        self.storage.validate_policy(target)
    }

    /// Returns the number of path verbs.
    pub fn len(&self) -> usize {
        self.verbs().len()
    }
    /// Reports whether the path has no verbs.
    pub fn is_empty(&self) -> bool {
        self.verbs().is_empty()
    }
    /// Returns the precomputed bounds of all control points.
    pub fn bounds(&self) -> Rect {
        self.bounds
    }
    /// Returns the immutable verbs.
    pub fn verbs(&self) -> &[PathVerb] {
        self.storage.verbs()
    }
    /// Returns the immutable points.
    pub fn points(&self) -> &[Point] {
        self.storage.points()
    }
    /// Checks the actual request owner without exposing an allocation handle.
    pub fn belongs_to(&self, resources: &ResourcesRef) -> bool {
        self.storage
            .resources()
            .is_some_and(|source| source.is_same_request(resources))
    }

    /// Actual unpaid array capacity, excluding the inline Path value. Paid
    /// storage and its shared header are already charged to their request.
    pub fn retained_unpaid_capacity_bytes(&self) -> Option<usize> {
        self.storage.retained_unpaid_capacity_bytes()
    }
    /// Shares same-kind storage or copies to paid storage while the source lives.
    pub fn try_copy_in(&self, resources: &ResourcesRef, kind: ResourceKind) -> Result<Self> {
        let storage = self.storage.try_copy_in(resources, kind)?;
        resources.checkpoint(0)?;
        Ok(Self {
            storage,
            bounds: self.bounds,
        })
    }
    /// Computes exact curve extrema using the original floating-point order.
    pub fn compute_tight_bounds(&self) -> Option<Rect> {
        self.compute_tight_bounds_with(&self.storage.control())
            .ok()
            .flatten()
    }
    /// Checked variant of compute_tight_bounds.
    pub fn try_compute_tight_bounds(&self, resources: &ResourcesRef) -> Result<Option<Rect>> {
        let control = Control::request(resources);
        control.source(&self.storage)?;
        self.compute_tight_bounds_with(&control)
    }
    fn compute_tight_bounds_with(&self, control: &Control) -> Result<Option<Rect>> {
        control.check(0)?;
        let mut extremas = [Point::zero(); 5];
        let mut min = self.points()[0];
        let mut max = self.points()[0];
        let mut iter = self.segments();
        let mut last_point = Point::zero();
        loop {
            control.check(1)?;
            let Some(segment) = iter.next() else {
                break;
            };
            let mut count = 0;
            match segment {
                PathSegment::MoveTo(p) | PathSegment::LineTo(p) => {
                    extremas[0] = p;
                    count = 1;
                }
                PathSegment::QuadTo(p0, p1) => {
                    count = compute_quad_extremas(last_point, p0, p1, &mut extremas);
                }
                PathSegment::CubicTo(p0, p1, p2) => {
                    count = compute_cubic_extremas(last_point, p0, p1, p2, &mut extremas);
                }
                PathSegment::Close => {}
            }
            last_point = iter.last_point;
            for tmp in &extremas[..count] {
                min.x = min.x.min(tmp.x);
                min.y = min.y.min(tmp.y);
                max.x = max.x.max(tmp.x);
                max.y = max.y.max(tmp.y);
            }
        }
        control.check(0)?;
        Ok(Rect::from_ltrb(min.x, min.y, max.x, max.y))
    }
    /// Transforms in place, returning None for the original invalid bounds case.
    pub fn transform(self, transform: Transform) -> Option<Self> {
        let control = self.storage.control();
        self.transform_with(transform, &control, None)
            .ok()
            .flatten()
    }
    /// Checked transform. A category change makes a genuinely paid copy.
    pub fn try_transform(
        self,
        transform: Transform,
        resources: &ResourcesRef,
        kind: ResourceKind,
    ) -> Result<Option<Self>> {
        self.transform_with(transform, &Control::request(resources), Some(kind))
    }
    fn transform_with(
        mut self,
        transform: Transform,
        control: &Control,
        kind: Option<ResourceKind>,
    ) -> Result<Option<Self>> {
        let candidate = (|| {
            control.source(&self.storage)?;
            if let (Some(resources), Some(kind)) = (&control.0, kind) {
                self.storage = self.storage.try_copy_in(resources, kind)?;
                control.check(0)?;
            }
            if transform.is_identity() {
                return Ok(Some(self));
            }
            self.storage.prepare(0, 0)?;
            for chunk in self.storage.points_mut().chunks_mut(256) {
                control.check(chunk.len() as u64)?;
                transform.map_points(chunk);
            }
            let Some(bounds) = Rect::from_points_controlled(self.points(), control)? else {
                return Ok(None);
            };
            self.bounds = bounds;
            Ok(Some(self))
        })();
        control.check(0)?;
        candidate
    }
    /// Iterates the original segment sequence, without allocating.
    pub fn segments(&self) -> PathSegmentsIter<'_> {
        PathSegmentsIter {
            path: self,
            verb_index: 0,
            points_index: 0,
            is_auto_close: false,
            last_move_to: Point::zero(),
            last_point: Point::zero(),
        }
    }
    /// Clears into a builder; a shared paid alias becomes an empty paid builder.
    pub fn clear(self) -> PathBuilder {
        let control = self.storage.control();
        let kind = self.storage.kind();
        match self.try_clear() {
            Ok(builder) => builder,
            Err(_) => match (control.0.as_ref(), kind) {
                (Some(resources), Some(kind)) => PathBuilder::with_resources(resources, kind),
                _ => PathBuilder::new(),
            },
        }
    }
    /// Checked variant of clear.
    pub fn try_clear(mut self) -> Result<PathBuilder> {
        let control = self.storage.control();
        let candidate = (|| {
            self.storage.clear()?;
            Ok(PathBuilder {
                storage: self.storage,
                last_move_to_index: 0,
                move_to_required: true,
            })
        })();
        control.check(0)?;
        candidate
    }
}

impl core::fmt::Debug for Path {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        use core::fmt::Write;

        let mut s = alloc::string::String::new();
        for segment in self.segments() {
            match segment {
                PathSegment::MoveTo(p) => s.write_fmt(format_args!("M {} {} ", p.x, p.y))?,
                PathSegment::LineTo(p) => s.write_fmt(format_args!("L {} {} ", p.x, p.y))?,
                PathSegment::QuadTo(p0, p1) => {
                    s.write_fmt(format_args!("Q {} {} {} {} ", p0.x, p0.y, p1.x, p1.y))?
                }
                PathSegment::CubicTo(p0, p1, p2) => s.write_fmt(format_args!(
                    "C {} {} {} {} {} {} ",
                    p0.x, p0.y, p1.x, p1.y, p2.x, p2.y
                ))?,
                PathSegment::Close => s.write_fmt(format_args!("Z "))?,
            }
        }

        s.pop(); // ' '

        f.debug_struct("Path")
            .field("segments", &s)
            .field("bounds", &self.bounds)
            .finish()
    }
}

fn compute_quad_extremas(p0: Point, p1: Point, p2: Point, extremas: &mut [Point; 5]) -> usize {
    use crate::path_geometry;

    let src = [p0, p1, p2];
    let mut extrema_idx = 0;
    if let Some(t) = path_geometry::find_quad_extrema(p0.x, p1.x, p2.x) {
        extremas[extrema_idx] = path_geometry::eval_quad_at(&src, t.to_normalized());
        extrema_idx += 1;
    }
    if let Some(t) = path_geometry::find_quad_extrema(p0.y, p1.y, p2.y) {
        extremas[extrema_idx] = path_geometry::eval_quad_at(&src, t.to_normalized());
        extrema_idx += 1;
    }
    extremas[extrema_idx] = p2;
    extrema_idx + 1
}

fn compute_cubic_extremas(
    p0: Point,
    p1: Point,
    p2: Point,
    p3: Point,
    extremas: &mut [Point; 5],
) -> usize {
    use crate::path_geometry;

    let mut ts0 = path_geometry::new_t_values();
    let mut ts1 = path_geometry::new_t_values();
    let n0 = path_geometry::find_cubic_extrema(p0.x, p1.x, p2.x, p3.x, &mut ts0);
    let n1 = path_geometry::find_cubic_extrema(p0.y, p1.y, p2.y, p3.y, &mut ts1);
    let total_len = n0 + n1;
    debug_assert!(total_len <= 4);

    let src = [p0, p1, p2, p3];
    let mut extrema_idx = 0;
    for t in &ts0[0..n0] {
        extremas[extrema_idx] = path_geometry::eval_cubic_pos_at(&src, t.to_normalized());
        extrema_idx += 1;
    }
    for t in &ts1[0..n1] {
        extremas[extrema_idx] = path_geometry::eval_cubic_pos_at(&src, t.to_normalized());
        extrema_idx += 1;
    }
    extremas[total_len] = p3;
    total_len + 1
}

/// A path segment.
#[allow(missing_docs)]
#[derive(Copy, Clone, PartialEq, Debug)]
pub enum PathSegment {
    MoveTo(Point),
    LineTo(Point),
    QuadTo(Point, Point),
    CubicTo(Point, Point, Point),
    Close,
}

/// A path segments iterator.
#[allow(missing_debug_implementations)]
#[derive(Clone)]
pub struct PathSegmentsIter<'a> {
    path: &'a Path,
    verb_index: usize,
    points_index: usize,

    is_auto_close: bool,
    last_move_to: Point,
    last_point: Point,
}

impl PathSegmentsIter<'_> {
    /// Sets the auto closing mode. Off by default.
    ///
    /// When enabled, emits an additional `PathSegment::Line` from the current position
    /// to the previous `PathSegment::Move`. And only then emits `PathSegment::Close`.
    pub fn set_auto_close(&mut self, flag: bool) {
        self.is_auto_close = flag;
    }

    pub(crate) fn auto_close(&mut self) -> PathSegment {
        if self.is_auto_close && self.last_point != self.last_move_to {
            self.verb_index -= 1;
            PathSegment::LineTo(self.last_move_to)
        } else {
            PathSegment::Close
        }
    }

    pub(crate) fn try_has_valid_tangent(&self, control: &Control) -> Result<bool> {
        let mut iter = self.clone();
        loop {
            control.check(1)?;
            let Some(segment) = iter.next() else {
                break;
            };
            match segment {
                PathSegment::MoveTo(_) => {
                    return Ok(false);
                }
                PathSegment::LineTo(p) => {
                    if iter.last_point == p {
                        continue;
                    }

                    return Ok(true);
                }
                PathSegment::QuadTo(p1, p2) => {
                    if iter.last_point == p1 && iter.last_point == p2 {
                        continue;
                    }

                    return Ok(true);
                }
                PathSegment::CubicTo(p1, p2, p3) => {
                    if iter.last_point == p1 && iter.last_point == p2 && iter.last_point == p3 {
                        continue;
                    }

                    return Ok(true);
                }
                PathSegment::Close => {
                    return Ok(false);
                }
            }
        }

        control.check(0)?;
        Ok(false)
    }

    /// Returns the current verb.
    pub fn curr_verb(&self) -> PathVerb {
        self.path.storage.verbs()[self.verb_index - 1]
    }

    /// Returns the next verb.
    pub fn next_verb(&self) -> Option<PathVerb> {
        self.path.storage.verbs().get(self.verb_index).cloned()
    }
}

impl Iterator for PathSegmentsIter<'_> {
    type Item = PathSegment;

    fn next(&mut self) -> Option<Self::Item> {
        if self.verb_index < self.path.storage.verbs().len() {
            let verb = self.path.storage.verbs()[self.verb_index];
            self.verb_index += 1;

            match verb {
                PathVerb::Move => {
                    self.points_index += 1;
                    self.last_move_to = self.path.storage.points()[self.points_index - 1];
                    self.last_point = self.last_move_to;
                    Some(PathSegment::MoveTo(self.last_move_to))
                }
                PathVerb::Line => {
                    self.points_index += 1;
                    self.last_point = self.path.storage.points()[self.points_index - 1];
                    Some(PathSegment::LineTo(self.last_point))
                }
                PathVerb::Quad => {
                    self.points_index += 2;
                    self.last_point = self.path.storage.points()[self.points_index - 1];
                    Some(PathSegment::QuadTo(
                        self.path.storage.points()[self.points_index - 2],
                        self.last_point,
                    ))
                }
                PathVerb::Cubic => {
                    self.points_index += 3;
                    self.last_point = self.path.storage.points()[self.points_index - 1];
                    Some(PathSegment::CubicTo(
                        self.path.storage.points()[self.points_index - 3],
                        self.path.storage.points()[self.points_index - 2],
                        self.last_point,
                    ))
                }
                PathVerb::Close => {
                    let seg = self.auto_close();
                    self.last_point = self.last_move_to;
                    Some(seg)
                }
            }
        } else {
            None
        }
    }
}
