// Copyright 2011 Google Inc.
// Copyright 2020 Yevhenii Reizner
// Modifications Copyright (c) 2026, Jiaqi (0x7c13) Liu.
//
// Use of this source code is governed by a BSD-style license that can be
// found in the LICENSE file.

use crate::painter::GeometryControl;
use alloc::vec::Vec;
use core::ops::{Deref, DerefMut};
use markdown_svg_budget::{BudgetVec, Error, ResourceKind};

type RasterResult<T> = Result<T, Error>;

/// Copy-only scratch storage. The request form never materializes an unpaid Vec.
pub(crate) enum RasterVec<T: Copy> {
    Plain(Vec<T>),
    Paid(BudgetVec<T>),
}

impl<T: Copy> Default for RasterVec<T> {
    fn default() -> Self {
        Self::Plain(Vec::new())
    }
}
impl<T: Copy + core::fmt::Debug> core::fmt::Debug for RasterVec<T> {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        self.deref().fmt(f)
    }
}

impl<T: Copy> RasterVec<T> {
    pub(crate) fn new(control: GeometryControl) -> Self {
        match control.resources() {
            Some(r) => Self::Paid(BudgetVec::new(r.clone(), ResourceKind::Scratch)),
            None => Self::Plain(Vec::new()),
        }
    }
    pub(crate) fn reserve(&mut self, additional: usize) -> RasterResult<()> {
        match self {
            Self::Plain(v) => {
                v.reserve(additional);
                Ok(())
            }
            Self::Paid(v) => v.try_reserve(additional),
        }
    }
    pub(crate) fn push(&mut self, value: T, control: GeometryControl) -> RasterResult<()> {
        self.reserve(1)?;
        control.check(core::mem::size_of::<T>().max(1) as u64)?;
        match self {
            Self::Plain(v) => v.push(value),
            Self::Paid(v) => v.push_reserved(value),
        }
        Ok(())
    }
    pub(crate) fn resize(&mut self, len: usize, value: T) -> RasterResult<()> {
        match self {
            Self::Plain(v) => {
                v.resize(len, value);
                Ok(())
            }
            Self::Paid(v) => v.try_resize_copy(len, value),
        }
    }
    fn pop(&mut self) -> Option<T> {
        match self {
            Self::Plain(v) => v.pop(),
            Self::Paid(v) => v.pop(),
        }
    }
    pub(crate) fn insert_first(
        &mut self,
        value: T,
        mut control: GeometryControl,
    ) -> RasterResult<()> {
        let len = self.len();
        self.push(value, control)?;
        let result =
            markdown_svg_budget::slice::copy_within_with_control(self, 0..len, 1, &mut control);
        // The borrowed helper cannot record a synthetic range error before this owner drops.
        control.finish(result)?;
        self[0] = value;
        Ok(())
    }
    pub(crate) fn sort_by(
        &mut self,
        mut control: GeometryControl,
        compare: &mut impl FnMut(&T, &T) -> core::cmp::Ordering,
    ) -> RasterResult<()> {
        if self.len() < 2 {
            return control.check(0);
        }
        let mut scratch = Self::new(control);
        let result = (|| {
            scratch.resize(self.len(), self[0])?;
            let result = markdown_svg_budget::slice::stable_sort_by_with_control(
                self,
                &mut scratch,
                &mut control,
                &mut |a, b, _| Ok(compare(a, b)),
            );
            control.finish(result)
        })();
        // Fix helper errors before scratch retirement, then observe refund errors before scanning.
        let result = control.finish(result);
        drop(scratch);
        control.finish(result)
    }
}
impl<T: Copy> Deref for RasterVec<T> {
    type Target = [T];
    fn deref(&self) -> &[T] {
        match self {
            Self::Plain(v) => v,
            Self::Paid(v) => v,
        }
    }
}
impl<T: Copy> DerefMut for RasterVec<T> {
    fn deref_mut(&mut self) -> &mut [T] {
        match self {
            Self::Plain(v) => v,
            Self::Paid(v) => v,
        }
    }
}

use tiny_skia_path::PathVerb;

use crate::{Path, Point};

use crate::edge::{CubicEdge, Edge, LineEdge, QuadraticEdge};

// These are the actual closed Copy records admitted by this scratch facade.
const _: () = {
    assert!(core::mem::size_of::<Edge>() <= 65_536);
    assert!(core::mem::size_of::<PathEdge>() * 18 <= 65_536);
};
use crate::edge_clipper::EdgeClipperIter;
use crate::geom::ScreenIntRect;
use crate::path_geometry;

#[derive(Copy, Clone, PartialEq, Debug)]
enum Combine {
    No,
    Partial,
    Total,
}

#[derive(Copy, Clone, Debug)]
pub struct ShiftedIntRect {
    shifted: ScreenIntRect,
    shift: i32,
}

impl ShiftedIntRect {
    pub fn new(rect: &ScreenIntRect, shift: i32) -> Option<Self> {
        let shifted = ScreenIntRect::from_xywh(
            rect.x() << shift,
            rect.y() << shift,
            rect.width() << shift,
            rect.height() << shift,
        )?;
        Some(ShiftedIntRect { shifted, shift })
    }

    pub fn shifted(&self) -> &ScreenIntRect {
        &self.shifted
    }

    pub fn recover(&self) -> ScreenIntRect {
        ScreenIntRect::from_xywh(
            self.shifted.x() >> self.shift,
            self.shifted.y() >> self.shift,
            self.shifted.width() >> self.shift,
            self.shifted.height() >> self.shift,
        )
        .unwrap() // cannot fail, because the original rect was valid
    }
}

pub struct BasicEdgeBuilder {
    edges: RasterVec<Edge>,
    clip_shift: i32,
}

impl BasicEdgeBuilder {
    fn new(clip_shift: i32, control: GeometryControl) -> RasterResult<Self> {
        let mut edges = RasterVec::new(control);
        edges.reserve(64)?;
        Ok(BasicEdgeBuilder { edges, clip_shift })
    }

    // Skia returns a linked list here, but it's a nightmare to use in Rust,
    // so we're mimicking it with Vec.
    pub(crate) fn build_edges(
        path: &Path,
        clip: Option<&ShiftedIntRect>,
        clip_shift: i32,
        control: GeometryControl,
    ) -> RasterResult<Option<RasterVec<Edge>>> {
        // If we're convex, then we need both edges, even if the right edge is past the clip.
        // let can_cull_to_the_right = !path.isConvex();
        let can_cull_to_the_right = false; // TODO: this

        control.check(0)?;
        let mut builder = BasicEdgeBuilder::new(clip_shift, control)?;
        let candidate = builder.build(path, clip, can_cull_to_the_right, control);
        let candidate = control.finish(candidate);
        match candidate {
            Ok(true) if builder.edges.len() >= 2 => Ok(Some(builder.edges)),
            Ok(valid) => {
                if !valid {
                    log::warn!("infinite or NaN segments detected during edges building");
                }
                drop(builder);
                control.finish(Ok(None))
            }
            Err(error) => {
                drop(builder);
                control.finish(Err(error))
            }
        }
    }

    // TODO: build_poly
    pub fn build(
        &mut self,
        path: &Path,
        clip: Option<&ShiftedIntRect>,
        can_cull_to_the_right: bool,
        control: GeometryControl,
    ) -> RasterResult<bool> {
        if let Some(clip) = clip {
            let clip = clip.recover().to_rect();
            let mut iter = EdgeClipperIter::new(path, clip, can_cull_to_the_right);
            while let Some(edges) = iter.try_next(control)? {
                for edge in edges {
                    control.check(1)?;
                    match edge {
                        PathEdge::LineTo(p0, p1) => {
                            if !p0.is_finite() || !p1.is_finite() {
                                return Ok(false);
                            }

                            self.push_line(&[p0, p1], control)?
                        }
                        PathEdge::QuadTo(p0, p1, p2) => {
                            if !p0.is_finite() || !p1.is_finite() || !p2.is_finite() {
                                return Ok(false);
                            }

                            self.push_quad(&[p0, p1, p2], control)?
                        }
                        PathEdge::CubicTo(p0, p1, p2, p3) => {
                            if !p0.is_finite()
                                || !p1.is_finite()
                                || !p2.is_finite()
                                || !p3.is_finite()
                            {
                                return Ok(false);
                            }

                            self.push_cubic(&[p0, p1, p2, p3], control)?
                        }
                    }
                }
            }
        } else {
            let mut iter = edge_iter(path);
            while let Some(edge) = iter.try_next(control)? {
                match edge {
                    PathEdge::LineTo(p0, p1) => {
                        self.push_line(&[p0, p1], control)?;
                    }
                    PathEdge::QuadTo(p0, p1, p2) => {
                        let points = [p0, p1, p2];
                        let mut mono_x = [Point::zero(); 5];
                        let n = path_geometry::chop_quad_at_y_extrema(&points, &mut mono_x);
                        for i in 0..=n {
                            self.push_quad(&mono_x[i * 2..], control)?;
                        }
                    }
                    PathEdge::CubicTo(p0, p1, p2, p3) => {
                        let points = [p0, p1, p2, p3];
                        let mut mono_y = [Point::zero(); 10];
                        let n = path_geometry::chop_cubic_at_y_extrema(&points, &mut mono_y);
                        for i in 0..=n {
                            self.push_cubic(&mono_y[i * 3..], control)?;
                        }
                    }
                }
            }
        }

        control.finish(Ok(true))
    }

    fn push_line(&mut self, points: &[Point; 2], control: GeometryControl) -> RasterResult<()> {
        control.check(1)?;
        if let Some(edge) = LineEdge::new(points[0], points[1], self.clip_shift) {
            let combine = if edge.is_vertical() && !self.edges.is_empty() {
                if let Some(Edge::Line(last)) = self.edges.last_mut() {
                    combine_vertical(&edge, last)
                } else {
                    Combine::No
                }
            } else {
                Combine::No
            };

            match combine {
                Combine::Total => {
                    self.edges.pop();
                }
                Combine::Partial => {}
                Combine::No => self.edges.push(Edge::Line(edge), control)?,
            }
        }
        Ok(())
    }

    fn push_quad(&mut self, points: &[Point], control: GeometryControl) -> RasterResult<()> {
        control.check(1)?;
        if let Some(edge) = QuadraticEdge::new(points, self.clip_shift, control)? {
            self.edges.push(Edge::Quadratic(edge), control)?;
        }
        Ok(())
    }

    fn push_cubic(&mut self, points: &[Point], control: GeometryControl) -> RasterResult<()> {
        control.check(1)?;
        if let Some(edge) = CubicEdge::new(points, self.clip_shift, control)? {
            self.edges.push(Edge::Cubic(edge), control)?;
        }
        Ok(())
    }
}

fn combine_vertical(edge: &LineEdge, last: &mut LineEdge) -> Combine {
    if last.dx != 0 || edge.x != last.x {
        return Combine::No;
    }

    if edge.winding == last.winding {
        return if edge.last_y + 1 == last.first_y {
            last.first_y = edge.first_y;
            Combine::Partial
        } else if edge.first_y == last.last_y + 1 {
            last.last_y = edge.last_y;
            Combine::Partial
        } else {
            Combine::No
        };
    }

    if edge.first_y == last.first_y {
        return if edge.last_y == last.last_y {
            Combine::Total
        } else if edge.last_y < last.last_y {
            last.first_y = edge.last_y + 1;
            Combine::Partial
        } else {
            last.first_y = last.last_y + 1;
            last.last_y = edge.last_y;
            last.winding = edge.winding;
            Combine::Partial
        };
    }

    if edge.last_y == last.last_y {
        if edge.first_y > last.first_y {
            last.last_y = edge.first_y - 1;
        } else {
            last.last_y = last.first_y - 1;
            last.first_y = edge.first_y;
            last.winding = edge.winding;
        }

        return Combine::Partial;
    }

    Combine::No
}

pub fn edge_iter(path: &Path) -> PathEdgeIter<'_> {
    PathEdgeIter {
        path,
        verb_index: 0,
        points_index: 0,
        move_to: Point::zero(),
        needs_close_line: false,
    }
}

#[derive(Copy, Clone, PartialEq, Debug)]
pub enum PathEdge {
    LineTo(Point, Point),
    QuadTo(Point, Point, Point),
    CubicTo(Point, Point, Point, Point),
}

/// Lightweight variant of PathIter that only returns segments (e.g. lines/quads).
///
/// Does not return Move or Close. Always "auto-closes" each contour.
pub struct PathEdgeIter<'a> {
    path: &'a Path,
    verb_index: usize,
    points_index: usize,
    move_to: Point,
    needs_close_line: bool,
}

impl PathEdgeIter<'_> {
    fn close_line(&mut self) -> Option<PathEdge> {
        self.needs_close_line = false;

        let edge = PathEdge::LineTo(self.path.points()[self.points_index - 1], self.move_to);
        Some(edge)
    }
}

impl PathEdgeIter<'_> {
    pub(crate) fn try_next(&mut self, control: GeometryControl) -> RasterResult<Option<PathEdge>> {
        loop {
            control.check(1)?;
            if self.verb_index >= self.path.verbs().len() {
                return Ok(if self.needs_close_line {
                    self.close_line()
                } else {
                    None
                });
            }
            let verb = self.path.verbs()[self.verb_index];
            self.verb_index += 1;
            match verb {
                PathVerb::Move => {
                    let previous = if self.needs_close_line {
                        self.close_line()
                    } else {
                        None
                    };
                    self.move_to = self.path.points()[self.points_index];
                    self.points_index += 1;
                    if previous.is_some() {
                        return Ok(previous);
                    }
                }
                PathVerb::Close => {
                    if self.needs_close_line {
                        return Ok(self.close_line());
                    }
                }
                _ => {
                    self.needs_close_line = true;
                    let p = self.path.points();
                    let i = self.points_index;
                    let (edge, count) = match verb {
                        PathVerb::Line => (PathEdge::LineTo(p[i - 1], p[i]), 1),
                        PathVerb::Quad => (PathEdge::QuadTo(p[i - 1], p[i], p[i + 1]), 2),
                        PathVerb::Cubic => {
                            (PathEdge::CubicTo(p[i - 1], p[i], p[i + 1], p[i + 2]), 3)
                        }
                        _ => unreachable!(),
                    };
                    self.points_index += count;
                    return Ok(Some(edge));
                }
            }
        }
    }
}

impl Iterator for PathEdgeIter<'_> {
    type Item = PathEdge;
    fn next(&mut self) -> Option<PathEdge> {
        // This kernel only fails at a checkpoint; ordinary checkpoints cannot fail.
        match self.try_next(GeometryControl::ordinary()) {
            Ok(value) => value,
            Err(_) => unreachable!(),
        }
    }
}
