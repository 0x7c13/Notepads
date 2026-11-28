// Copyright 2006 The Android Open Source Project
// Copyright 2020 Yevhenii Reizner
// Modifications Copyright (c) 2026, Jiaqi (0x7c13) Liu.
//
// Use of this source code is governed by a BSD-style license that can be
// found in the LICENSE file.

// NOTE: this is not SkPathBuilder, but rather a reimplementation of SkPath.
use alloc::vec;
use alloc::vec::Vec;
use markdown_svg_budget::{ResourceKind, ResourcesRef};

use crate::path::PathVerb;
use crate::path_geometry;
use crate::scalar::{Scalar, SCALAR_ROOT_2_OVER_2};
use crate::storage::{GeometryStorage, Result};
use crate::{Path, Point, Rect};

#[derive(Copy, Clone, PartialEq, Debug)]
pub(crate) enum PathDirection {
    CW,
    CCW,
}

/// A path builder. Paid clones share both arrays until a checked mutation.
#[derive(Clone, Default, Debug)]
pub struct PathBuilder {
    pub(crate) storage: GeometryStorage,
    pub(crate) last_move_to_index: usize,
    pub(crate) move_to_required: bool,
}

impl PathBuilder {
    /// Creates a new builder.
    pub fn new() -> Self {
        Self {
            storage: GeometryStorage::default(),
            last_move_to_index: 0,
            move_to_required: true,
        }
    }

    /// Creates a new builder with specified verb and point capacities.
    pub fn with_capacity(verbs_capacity: usize, points_capacity: usize) -> Self {
        Self {
            storage: GeometryStorage::compat(
                Vec::with_capacity(verbs_capacity),
                Vec::with_capacity(points_capacity),
            ),
            last_move_to_index: 0,
            move_to_required: true,
        }
    }

    /// Creates an empty builder in the original request; no heap is allocated.
    pub fn with_resources(resources: &ResourcesRef, kind: ResourceKind) -> Self {
        #[cfg(test)]
        crate::storage::record_dash_builder_start();
        Self {
            storage: GeometryStorage::with_resources(resources, kind),
            last_move_to_index: 0,
            move_to_required: true,
        }
    }

    /// Creates paid empty storage with the original Default command state.
    /// Unlike with_resources, the first non-Move command does not inject Move.
    /// This only clones the request capability; no heap storage is allocated.
    pub fn with_resources_default(resources: &ResourcesRef, kind: ResourceKind) -> Self {
        let mut builder = Self::with_resources(resources, kind);
        builder.move_to_required = false;
        builder
    }

    /// Pays the actual header and both requested capacities before allocation.
    pub fn try_with_capacity(
        resources: &ResourcesRef,
        kind: ResourceKind,
        verbs: usize,
        points: usize,
    ) -> Result<Self> {
        let mut builder = Self::with_resources(resources, kind);
        builder.storage = GeometryStorage::try_with_capacities(resources, kind, verbs, points)?;
        Ok(builder)
    }

    /// Creates a clockwise closed rectangle path.
    pub fn from_rect(rect: Rect) -> Path {
        Path {
            bounds: rect,
            storage: GeometryStorage::compat(
                vec![
                    PathVerb::Move,
                    PathVerb::Line,
                    PathVerb::Line,
                    PathVerb::Line,
                    PathVerb::Close,
                ],
                vec![
                    Point::from_xy(rect.left(), rect.top()),
                    Point::from_xy(rect.right(), rect.top()),
                    Point::from_xy(rect.right(), rect.bottom()),
                    Point::from_xy(rect.left(), rect.bottom()),
                ],
            ),
        }
    }

    /// Creates the same rectangle with paid storage, without a temporary Vec.
    pub fn try_from_rect(rect: Rect, resources: &ResourcesRef, kind: ResourceKind) -> Result<Path> {
        let mut storage = GeometryStorage::with_resources(resources, kind);
        resources.checkpoint(5)?;
        storage.prepare(5, 4)?;
        storage.append_reserved(
            &[
                PathVerb::Move,
                PathVerb::Line,
                PathVerb::Line,
                PathVerb::Line,
                PathVerb::Close,
            ],
            &[
                Point::from_xy(rect.left(), rect.top()),
                Point::from_xy(rect.right(), rect.top()),
                Point::from_xy(rect.right(), rect.bottom()),
                Point::from_xy(rect.left(), rect.bottom()),
            ],
        );
        resources.checkpoint(0)?;
        Ok(Path {
            bounds: rect,
            storage,
        })
    }

    /// Creates a path from a circle.
    pub fn from_circle(cx: f32, cy: f32, radius: f32) -> Option<Path> {
        let mut builder = Self::new();
        builder.push_circle(cx, cy, radius);
        builder.finish()
    }

    /// Creates a path from an oval.
    pub fn from_oval(oval: Rect) -> Option<Path> {
        let mut builder = Self::new();
        builder.push_oval(oval);
        builder.finish()
    }

    pub(crate) fn verbs(&self) -> &[PathVerb] {
        self.storage.verbs()
    }
    pub(crate) fn points(&self) -> &[Point] {
        self.storage.points()
    }
    pub(crate) fn try_reserve(&mut self, verbs: usize, points: usize) -> Result<()> {
        self.storage.control().check(0)?;
        self.storage.prepare(verbs, points)
    }

    /// Returns the current number of verbs.
    pub fn len(&self) -> usize {
        self.verbs().len()
    }
    /// Checks whether any verbs have been added.
    pub fn is_empty(&self) -> bool {
        self.verbs().is_empty()
    }

    /// Begins a contour, replacing a preceding Move.
    pub fn move_to(&mut self, x: f32, y: f32) {
        let _ = self.try_move_to(x, y);
    }
    /// Checked variant of move_to.
    pub fn try_move_to(&mut self, x: f32, y: f32) -> Result<()> {
        let control = self.storage.control();
        control.check(1)?;
        if self.verbs().last() == Some(&PathVerb::Move) {
            self.storage.prepare(0, 0)?;
            let last = self.points().len() - 1;
            self.storage.points_mut()[last] = Point::from_xy(x, y);
        } else {
            self.storage.prepare(1, 1)?;
            self.last_move_to_index = self.points().len();
            self.move_to_required = false;
            self.storage
                .append_reserved(&[PathVerb::Move], &[Point::from_xy(x, y)]);
        }
        control.check(0)
    }

    fn try_command(&mut self, verb: PathVerb, points: &[Point]) -> Result<()> {
        let control = self.storage.control();
        control.check(1)?;
        let inject = self.move_to_required;
        let start = if inject {
            self.points()
                .get(self.last_move_to_index)
                .copied()
                .unwrap_or_default()
        } else {
            Point::zero()
        };
        self.storage
            .prepare(1 + usize::from(inject), points.len() + usize::from(inject))?;
        if inject {
            self.last_move_to_index = self.points().len();
            self.move_to_required = false;
            self.storage.append_reserved(&[PathVerb::Move], &[start]);
        }
        self.storage.append_reserved(&[verb], points);
        control.check(0)
    }

    /// Adds a line, injecting the original implicit Move when necessary.
    pub fn line_to(&mut self, x: f32, y: f32) {
        let _ = self.try_line_to(x, y);
    }
    /// Checked variant of line_to.
    pub fn try_line_to(&mut self, x: f32, y: f32) -> Result<()> {
        self.try_command(PathVerb::Line, &[Point::from_xy(x, y)])
    }
    /// Adds a quadratic curve.
    pub fn quad_to(&mut self, x1: f32, y1: f32, x: f32, y: f32) {
        let _ = self.try_quad_to(x1, y1, x, y);
    }
    /// Checked variant of quad_to.
    pub fn try_quad_to(&mut self, x1: f32, y1: f32, x: f32, y: f32) -> Result<()> {
        self.try_command(
            PathVerb::Quad,
            &[Point::from_xy(x1, y1), Point::from_xy(x, y)],
        )
    }
    pub(crate) fn try_quad_to_pt(&mut self, a: Point, b: Point) -> Result<()> {
        self.try_quad_to(a.x, a.y, b.x, b.y)
    }

    pub(crate) fn try_conic_to(
        &mut self,
        x1: f32,
        y1: f32,
        x: f32,
        y: f32,
        weight: f32,
    ) -> Result<()> {
        self.storage.control().check(1)?;
        if !(weight > 0.0) {
            self.try_line_to(x, y)?;
        } else if !weight.is_finite() {
            self.try_line_to(x1, y1)?;
            self.try_line_to(x, y)?;
        } else if weight == 1.0 {
            self.try_quad_to(x1, y1, x, y)?;
        } else {
            if self.move_to_required {
                let start = self
                    .points()
                    .get(self.last_move_to_index)
                    .copied()
                    .unwrap_or_default();
                self.try_move_to(start.x, start.y)?;
            }
            let last = self.last_point().expect("conic start");
            if let Some(quadder) = path_geometry::AutoConicToQuads::compute(
                last,
                Point::from_xy(x1, y1),
                Point::from_xy(x, y),
                weight,
            ) {
                // AutoConicToQuads uses a fixed stack array and pow2 <= 4.
                let mut offset = 1;
                for _ in 0..quadder.len {
                    self.storage.control().check(1)?;
                    let a = quadder.points[offset];
                    let b = quadder.points[offset + 1];
                    self.try_quad_to(a.x, a.y, b.x, b.y)?;
                    offset += 2;
                }
            }
        }
        self.storage.control().check(0)
    }
    pub(crate) fn try_conic_points_to(&mut self, a: Point, b: Point, weight: f32) -> Result<()> {
        self.try_conic_to(a.x, a.y, b.x, b.y, weight)
    }

    /// Adds a cubic curve.
    pub fn cubic_to(&mut self, x1: f32, y1: f32, x2: f32, y2: f32, x: f32, y: f32) {
        let _ = self.try_cubic_to(x1, y1, x2, y2, x, y);
    }
    /// Checked variant of cubic_to.
    pub fn try_cubic_to(
        &mut self,
        x1: f32,
        y1: f32,
        x2: f32,
        y2: f32,
        x: f32,
        y: f32,
    ) -> Result<()> {
        self.try_command(
            PathVerb::Cubic,
            &[
                Point::from_xy(x1, y1),
                Point::from_xy(x2, y2),
                Point::from_xy(x, y),
            ],
        )
    }
    pub(crate) fn try_cubic_to_pt(&mut self, a: Point, b: Point, c: Point) -> Result<()> {
        self.try_cubic_to(a.x, a.y, b.x, b.y, c.x, c.y)
    }

    /// Closes the current contour unless it is empty or already closed.
    pub fn close(&mut self) {
        let _ = self.try_close();
    }
    /// Checked variant of close.
    pub fn try_close(&mut self) -> Result<()> {
        let control = self.storage.control();
        control.check(1)?;
        if !self.is_empty() && self.verbs().last() != Some(&PathVerb::Close) {
            self.storage.prepare(1, 0)?;
            self.storage.append_reserved(&[PathVerb::Close], &[]);
        }
        self.move_to_required = true;
        control.check(0)
    }

    /// Returns the last point.
    pub fn last_point(&self) -> Option<Point> {
        self.points().last().copied()
    }
    pub(crate) fn try_set_last_point(&mut self, point: Point) -> Result<()> {
        self.storage.control().check(1)?;
        if self.points().is_empty() {
            self.try_move_to(point.x, point.y)
        } else {
            self.storage.prepare(0, 0)?;
            *self
                .storage
                .points_mut()
                .last_mut()
                .expect("last path point") = point;
            self.storage.control().check(0)
        }
    }
    pub(crate) fn try_is_zero_length_since_point(&self, start: usize) -> Result<bool> {
        let control = self.storage.control();
        control.check(0)?;
        let count = self.points().len() - start;
        if count < 2 {
            return Ok(true);
        }
        let first = self.points()[start];
        for index in 1..count {
            control.check(1)?;
            if first != self.points()[start + index] {
                return Ok(false);
            }
        }
        control.check(0)?;
        Ok(true)
    }

    /// Appends a clockwise rectangle.
    pub fn push_rect(&mut self, rect: Rect) {
        let _ = self.try_push_rect(rect);
    }
    /// Checked variant of push_rect.
    pub fn try_push_rect(&mut self, rect: Rect) -> Result<()> {
        self.try_move_to(rect.left(), rect.top())?;
        self.try_line_to(rect.right(), rect.top())?;
        self.try_line_to(rect.right(), rect.bottom())?;
        self.try_line_to(rect.left(), rect.bottom())?;
        self.try_close()
    }
    /// Appends a clockwise oval.
    pub fn push_oval(&mut self, oval: Rect) {
        let _ = self.try_push_oval(oval);
    }
    /// Checked variant of push_oval.
    pub fn try_push_oval(&mut self, oval: Rect) -> Result<()> {
        let cx = oval.left().half() + oval.right().half();
        let cy = oval.top().half() + oval.bottom().half();
        let oval_points = [
            Point::from_xy(cx, oval.bottom()),
            Point::from_xy(oval.left(), cy),
            Point::from_xy(cx, oval.top()),
            Point::from_xy(oval.right(), cy),
        ];
        let rect_points = [
            Point::from_xy(oval.right(), oval.bottom()),
            Point::from_xy(oval.left(), oval.bottom()),
            Point::from_xy(oval.left(), oval.top()),
            Point::from_xy(oval.right(), oval.top()),
        ];
        self.try_move_to(oval_points[3].x, oval_points[3].y)?;
        for (a, b) in rect_points.iter().zip(oval_points.iter()) {
            self.try_conic_points_to(*a, *b, SCALAR_ROOT_2_OVER_2)?;
        }
        self.try_close()
    }
    /// Appends a circle when its original bounds are valid.
    pub fn push_circle(&mut self, x: f32, y: f32, radius: f32) {
        let _ = self.try_push_circle(x, y, radius);
    }
    /// Checked variant of push_circle.
    pub fn try_push_circle(&mut self, x: f32, y: f32, radius: f32) -> Result<()> {
        self.storage.control().check(1)?;
        if let Some(rect) =
            Rect::from_xywh(x - radius, y - radius, radius + radius, radius + radius)
        {
            self.try_push_oval(rect)?;
        }
        self.storage.control().check(0)
    }

    fn append_storage(&mut self, other: &GeometryStorage) -> Result<()> {
        if self.storage.resources().is_none() {
            if let Some(resources) = other.resources() {
                self.storage = self
                    .storage
                    .try_copy_in(resources, other.kind().expect("paid path kind"))?;
            }
        }
        let control = self.storage.control();
        control.source(other)?;
        self.storage
            .prepare(other.verbs().len(), other.points().len())?;
        let mut offset = 0;
        for index in 0..other.verbs().len() {
            control.check(1)?;
            let verb = other.verbs()[index];
            let count = match verb {
                PathVerb::Move | PathVerb::Line => 1,
                PathVerb::Quad => 2,
                PathVerb::Cubic => 3,
                PathVerb::Close => 0,
            };
            self.storage
                .append_reserved(&[verb], &other.points()[offset..offset + count]);
            offset += count;
        }
        control.check(0)
    }

    /// Appends a path, keeping the original builder-state behavior.
    pub fn push_path(&mut self, path: &Path) {
        let _ = self.try_push_path(path);
    }
    /// Checked variant of push_path, rejecting a different paid request.
    pub fn try_push_path(&mut self, path: &Path) -> Result<()> {
        self.storage.control().source(&path.storage)?;
        self.last_move_to_index = self.points().len();
        self.append_storage(&path.storage)
    }
    pub(crate) fn try_push_path_builder(&mut self, other: &PathBuilder) -> Result<()> {
        self.storage.control().source(&other.storage)?;
        if other.is_empty() {
            return self.storage.control().check(0);
        }
        if self.last_move_to_index != 0 {
            self.last_move_to_index = self
                .points()
                .len()
                .checked_add(other.last_move_to_index)
                .ok_or_else(|| {
                    self.storage.control().fail(
                        markdown_svg_budget::FailureKind::Quota,
                        markdown_svg_budget::LimitKind::Capacity,
                    )
                })?;
        }
        self.append_storage(&other.storage)
    }
    pub(crate) fn try_reverse_path_to(&mut self, other: &PathBuilder) -> Result<()> {
        self.storage.control().source(&other.storage)?;
        if other.is_empty() {
            return Ok(());
        }
        debug_assert_eq!(other.verbs()[0], PathVerb::Move);
        let mut offset = other.points().len() - 1;
        for verb in other.verbs().iter().rev() {
            self.storage.control().check(1)?;
            match verb {
                PathVerb::Move => break,
                PathVerb::Line => {
                    let a = other.points()[offset - 1];
                    offset -= 1;
                    self.try_line_to(a.x, a.y)?;
                }
                PathVerb::Quad => {
                    let a = other.points()[offset - 1];
                    let b = other.points()[offset - 2];
                    offset -= 2;
                    self.try_quad_to_pt(a, b)?;
                }
                PathVerb::Cubic => {
                    let a = other.points()[offset - 1];
                    let b = other.points()[offset - 2];
                    let c = other.points()[offset - 3];
                    offset -= 3;
                    self.try_cubic_to_pt(a, b, c)?;
                }
                PathVerb::Close => {}
            }
        }
        self.storage.control().check(0)
    }

    /// Clears the builder, retaining exclusive storage and its payment.
    pub fn clear(&mut self) {
        let _ = self.try_clear();
    }
    /// Shared clear discards only this alias; it never creates an ordinary buffer.
    pub fn try_clear(&mut self) -> Result<()> {
        self.storage.clear()?;
        self.last_move_to_index = 0;
        self.move_to_required = true;
        self.storage.control().check(0)
    }

    /// Finishes the builder, retaining the original empty/invalid None behavior.
    pub fn finish(self) -> Option<Path> {
        self.try_finish().ok().flatten()
    }
    /// Moves both paid arrays into a Path; errors never become a successful None.
    pub fn try_finish(self) -> Result<Option<Path>> {
        let control = self.storage.control();
        let candidate = (|| {
            control.check(0)?;
            if self.len() < 2 {
                return Ok(None);
            }
            let Some(bounds) = Rect::from_points_controlled(self.points(), &control)? else {
                return Ok(None);
            };
            Ok(Some(Path {
                bounds,
                storage: self.storage,
            }))
        })();
        control.check(0)?;
        candidate
    }
}
