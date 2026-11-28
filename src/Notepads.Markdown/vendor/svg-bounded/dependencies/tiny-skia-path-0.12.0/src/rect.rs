// Copyright 2020 Yevhenii Reizner
// Modifications Copyright (c) 2026, Jiaqi (0x7c13) Liu.
//
// Use of this source code is governed by a BSD-style license that can be
// found in the LICENSE file.

use core::convert::TryFrom;

use crate::{FiniteF32, IntSize, LengthU32, Point, SaturateRound, Size, Transform};

#[cfg(all(not(feature = "std"), feature = "no-std-float"))]
use crate::NoStdFloat;

/// An integer rectangle.
///
/// # Guarantees
///
/// - Width and height are in 1..=i32::MAX range.
/// - x+width and y+height does not overflow.
#[allow(missing_docs)]
#[derive(Copy, Clone, PartialEq, Debug)]
pub struct IntRect {
    x: i32,
    y: i32,
    width: LengthU32,
    height: LengthU32,
}

impl IntRect {
    /// Creates a new `IntRect`.
    pub fn from_xywh(x: i32, y: i32, width: u32, height: u32) -> Option<Self> {
        x.checked_add(i32::try_from(width).ok()?)?;
        y.checked_add(i32::try_from(height).ok()?)?;

        Some(IntRect {
            x,
            y,
            width: LengthU32::new(width)?,
            height: LengthU32::new(height)?,
        })
    }

    /// Creates a new `IntRect`.
    pub fn from_ltrb(left: i32, top: i32, right: i32, bottom: i32) -> Option<Self> {
        let width = u32::try_from(right.checked_sub(left)?).ok()?;
        let height = u32::try_from(bottom.checked_sub(top)?).ok()?;
        IntRect::from_xywh(left, top, width, height)
    }

    /// Returns rect's X position.
    pub fn x(&self) -> i32 {
        self.x
    }

    /// Returns rect's Y position.
    pub fn y(&self) -> i32 {
        self.y
    }

    /// Returns rect's width.
    pub fn width(&self) -> u32 {
        self.width.get()
    }

    /// Returns rect's height.
    pub fn height(&self) -> u32 {
        self.height.get()
    }

    /// Returns rect's left edge.
    pub fn left(&self) -> i32 {
        self.x
    }

    /// Returns rect's top edge.
    pub fn top(&self) -> i32 {
        self.y
    }

    /// Returns rect's right edge.
    pub fn right(&self) -> i32 {
        // No overflow is guaranteed by constructors.
        self.x + self.width.get() as i32
    }

    /// Returns rect's bottom edge.
    pub fn bottom(&self) -> i32 {
        // No overflow is guaranteed by constructors.
        self.y + self.height.get() as i32
    }

    /// Returns rect's size.
    pub fn size(&self) -> IntSize {
        IntSize::from_wh_safe(self.width, self.height)
    }

    /// Checks that the rect is completely includes `other` Rect.
    pub fn contains(&self, other: &Self) -> bool {
        self.x <= other.x
            && self.y <= other.y
            && self.right() >= other.right()
            && self.bottom() >= other.bottom()
    }

    /// Returns an intersection of two rectangles.
    ///
    /// Returns `None` otherwise.
    pub fn intersect(&self, other: &Self) -> Option<Self> {
        let left = self.x.max(other.x);
        let top = self.y.max(other.y);

        let right = self.right().min(other.right());
        let bottom = self.bottom().min(other.bottom());

        let w = u32::try_from(right.checked_sub(left)?).ok()?;
        let h = u32::try_from(bottom.checked_sub(top)?).ok()?;

        IntRect::from_xywh(left, top, w, h)
    }

    /// Insets the rectangle.
    pub fn inset(&self, dx: i32, dy: i32) -> Option<Self> {
        IntRect::from_ltrb(
            self.left() + dx,
            self.top() + dy,
            self.right() - dx,
            self.bottom() - dy,
        )
    }

    /// Outsets the rectangle.
    pub fn make_outset(&self, dx: i32, dy: i32) -> Option<Self> {
        IntRect::from_ltrb(
            self.left().saturating_sub(dx),
            self.top().saturating_sub(dy),
            self.right().saturating_add(dx),
            self.bottom().saturating_add(dy),
        )
    }

    /// Translates the rect by the specified offset.
    pub fn translate(&self, tx: i32, ty: i32) -> Option<Self> {
        IntRect::from_xywh(self.x() + tx, self.y() + ty, self.width(), self.height())
    }

    /// Translates the rect to the specified position.
    pub fn translate_to(&self, x: i32, y: i32) -> Option<Self> {
        IntRect::from_xywh(x, y, self.width(), self.height())
    }

    /// Converts into `Rect`.
    pub fn to_rect(&self) -> Rect {
        // Can't fail, because `IntRect` is always valid.
        Rect::from_ltrb(
            self.x as f32,
            self.y as f32,
            self.x as f32 + self.width.get() as f32,
            self.y as f32 + self.height.get() as f32,
        )
        .unwrap()
    }
}

#[cfg(test)]
mod int_rect_tests {
    use super::*;

    #[test]
    fn tests() {
        assert_eq!(IntRect::from_xywh(0, 0, 0, 0), None);
        assert_eq!(IntRect::from_xywh(0, 0, 1, 0), None);
        assert_eq!(IntRect::from_xywh(0, 0, 0, 1), None);

        assert_eq!(IntRect::from_xywh(0, 0, u32::MAX, u32::MAX), None);
        assert_eq!(IntRect::from_xywh(0, 0, 1, u32::MAX), None);
        assert_eq!(IntRect::from_xywh(0, 0, u32::MAX, 1), None);

        assert_eq!(IntRect::from_xywh(i32::MAX, 0, 1, 1), None);
        assert_eq!(IntRect::from_xywh(0, i32::MAX, 1, 1), None);

        {
            // No intersection.
            let r1 = IntRect::from_xywh(1, 2, 3, 4).unwrap();
            let r2 = IntRect::from_xywh(11, 12, 13, 14).unwrap();
            assert_eq!(r1.intersect(&r2), None);
        }

        {
            // Second inside the first one.
            let r1 = IntRect::from_xywh(1, 2, 30, 40).unwrap();
            let r2 = IntRect::from_xywh(11, 12, 13, 14).unwrap();
            assert_eq!(r1.intersect(&r2), IntRect::from_xywh(11, 12, 13, 14));
        }

        {
            // Partial overlap.
            let r1 = IntRect::from_xywh(1, 2, 30, 40).unwrap();
            let r2 = IntRect::from_xywh(11, 12, 50, 60).unwrap();
            assert_eq!(r1.intersect(&r2), IntRect::from_xywh(11, 12, 20, 30));
        }
    }
}

/// A rectangle defined by left, top, right and bottom edges.
///
/// Can have zero width and/or height. But not a negative one.
///
/// # Guarantees
///
/// - All values are finite.
/// - Left edge is <= right.
/// - Top edge is <= bottom.
/// - Width and height are <= f32::MAX.
#[allow(missing_docs)]
#[derive(Copy, Clone, PartialEq, Hash, Eq)]
pub struct Rect {
    left: FiniteF32,
    top: FiniteF32,
    right: FiniteF32,
    bottom: FiniteF32,
}

impl core::fmt::Debug for Rect {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("Rect")
            .field("left", &self.left.get())
            .field("top", &self.top.get())
            .field("right", &self.right.get())
            .field("bottom", &self.bottom.get())
            .finish()
    }
}

impl Rect {
    /// Creates new `Rect`.
    pub fn from_ltrb(left: f32, top: f32, right: f32, bottom: f32) -> Option<Self> {
        let left = FiniteF32::new(left)?;
        let top = FiniteF32::new(top)?;
        let right = FiniteF32::new(right)?;
        let bottom = FiniteF32::new(bottom)?;

        if left.get() <= right.get() && top.get() <= bottom.get() {
            // Width and height must not overflow.
            checked_f32_sub(right.get(), left.get())?;
            checked_f32_sub(bottom.get(), top.get())?;

            Some(Rect {
                left,
                top,
                right,
                bottom,
            })
        } else {
            None
        }
    }

    /// Creates new `Rect`.
    pub fn from_xywh(x: f32, y: f32, w: f32, h: f32) -> Option<Self> {
        Rect::from_ltrb(x, y, w + x, h + y)
    }

    /// Returns the left edge.
    pub fn left(&self) -> f32 {
        self.left.get()
    }

    /// Returns the top edge.
    pub fn top(&self) -> f32 {
        self.top.get()
    }

    /// Returns the right edge.
    pub fn right(&self) -> f32 {
        self.right.get()
    }

    /// Returns the bottom edge.
    pub fn bottom(&self) -> f32 {
        self.bottom.get()
    }

    /// Returns rect's X position.
    pub fn x(&self) -> f32 {
        self.left.get()
    }

    /// Returns rect's Y position.
    pub fn y(&self) -> f32 {
        self.top.get()
    }

    /// Returns rect's width.
    #[inline]
    pub fn width(&self) -> f32 {
        self.right.get() - self.left.get()
    }

    /// Returns rect's height.
    #[inline]
    pub fn height(&self) -> f32 {
        self.bottom.get() - self.top.get()
    }

    /// Returns true if the rect is empty.
    #[inline]
    pub fn is_empty(&self) -> bool {
        self.left == self.right || self.top == self.bottom
    }

    /// Converts into an `IntRect` by adding 0.5 and discarding the fractional portion.
    ///
    /// Width and height are guarantee to be >= 1.
    pub fn round(&self) -> Option<IntRect> {
        let left = i32::saturate_round(self.left());
        let top = i32::saturate_round(self.top());
        let right = i32::saturate_round(self.right());
        let bottom = i32::saturate_round(self.bottom());
        IntRect::from_xywh(
            left,
            top,
            core::cmp::max(1, right.saturating_sub(left)) as u32,
            core::cmp::max(1, bottom.saturating_sub(top)) as u32,
        )
    }

    /// Converts into an `IntRect` rounding outwards.
    ///
    /// Width and height are guarantee to be >= 1.
    pub fn round_out(&self) -> Option<IntRect> {
        let left = i32::saturate_floor(self.left());
        let top = i32::saturate_floor(self.top());
        let right = i32::saturate_ceil(self.right());
        let bottom = i32::saturate_ceil(self.bottom());
        IntRect::from_xywh(
            left,
            top,
            core::cmp::max(1, right.saturating_sub(left)) as u32,
            core::cmp::max(1, bottom.saturating_sub(top)) as u32,
        )
    }

    /// Returns an intersection of two rectangles.
    ///
    /// Returns `None` otherwise.
    pub fn intersect(&self, other: &Self) -> Option<Self> {
        let left = self.x().max(other.x());
        let top = self.y().max(other.y());

        let right = self.right().min(other.right());
        let bottom = self.bottom().min(other.bottom());

        Rect::from_ltrb(left, top, right, bottom)
    }

    /// Returns the union of two rectangles.
    ///
    /// Returns `None` if the width or height would overflow.
    pub fn join(&self, other: &Self) -> Option<Self> {
        if other.is_empty() {
            return Some(*self);
        }
        if self.is_empty() {
            return Some(*other);
        }

        let left = self.x().min(other.x());
        let top = self.y().min(other.y());

        let right = self.right().max(other.right());
        let bottom = self.bottom().max(other.bottom());

        Rect::from_ltrb(left, top, right, bottom)
    }

    /// Creates a Rect from Point array.
    ///
    /// Returns None if count is zero or if Point array contains an infinity or NaN.
    pub fn from_points(points: &[Point]) -> Option<Self> {
        Self::from_points_controlled(points, &crate::storage::Control::default())
            .ok()
            .flatten()
    }

    pub(crate) fn from_points_controlled(
        points: &[Point],
        control: &crate::storage::Control,
    ) -> crate::storage::Result<Option<Self>> {
        control.check(1)?;
        use crate::f32x4_t::f32x4;

        let mut offset;
        let mut min;
        let mut max;

        // Handle trivial-size lists without creating vectors, since they won't
        // actually use the vector min/max code
        match points.len() {
            0 => return Ok(None),
            1 => {
                let Point { x, y } = points[0];
                return Ok(Rect::from_xywh(x, y, 0.0, 0.0));
            }
            2 => {
                let Point { x: x0, y: y0 } = points[0];
                let Point { x: x1, y: y1 } = points[1];
                // Avoid min/max to preserve a NaN for rejection by from_ltrb
                let (l, r) = if x0 < x1 { (x0, x1) } else { (x1, x0) };
                let (t, b) = if y0 < y1 { (y0, y1) } else { (y1, y0) };
                return Ok(Rect::from_ltrb(l, t, r, b));
            }
            i if i & 1 != 0 => {
                let pt = points[0];
                min = f32x4([pt.x, pt.y, pt.x, pt.y]);
                max = min;
                offset = 1;
            }
            _ => {
                let pt0 = points[0];
                let pt1 = points[1];
                min = f32x4([pt0.x, pt0.y, pt1.x, pt1.y]);
                max = min;
                offset = 2;
            }
        }

        let mut accum = f32x4::default();
        while offset != points.len() {
            control.check(2)?;
            let pt0 = points[offset + 0];
            let pt1 = points[offset + 1];
            let xy = f32x4([pt0.x, pt0.y, pt1.x, pt1.y]);

            accum *= xy;
            min = min.min(xy);
            max = max.max(xy);
            offset += 2;
        }

        let all_finite = accum * f32x4::default() == f32x4::default();
        let min: [f32; 4] = min.0;
        let max: [f32; 4] = max.0;
        control.check(0)?;
        Ok(if all_finite {
            Rect::from_ltrb(
                min[0].min(min[2]),
                min[1].min(min[3]),
                max[0].max(max[2]),
                max[1].max(max[3]),
            )
        } else {
            None
        })
    }

    /// Insets the rectangle by the specified offset.
    pub fn inset(&self, dx: f32, dy: f32) -> Option<Self> {
        Rect::from_ltrb(
            self.left() + dx,
            self.top() + dy,
            self.right() - dx,
            self.bottom() - dy,
        )
    }

    /// Outsets the rectangle by the specified offset.
    pub fn outset(&self, dx: f32, dy: f32) -> Option<Self> {
        self.inset(-dx, -dy)
    }

    /// Transforms the rect using the provided `Transform`.
    ///
    /// If the transform is a skew, the result will be a bounding box around the skewed rectangle.
    pub fn transform(&self, ts: Transform) -> Option<Self> {
        if ts.is_identity() {
            Some(*self)
        } else if ts.has_skew() {
            // we need to transform all 4 corners
            let lt = Point::from_xy(self.left(), self.top());
            let rt = Point::from_xy(self.right(), self.top());
            let lb = Point::from_xy(self.left(), self.bottom());
            let rb = Point::from_xy(self.right(), self.bottom());
            let mut pts = [lt, rt, lb, rb];
            ts.map_points(&mut pts);
            Self::from_points(&pts)
        } else {
            // Faster (more common) case
            let lt = Point::from_xy(self.left(), self.top());
            let rb = Point::from_xy(self.right(), self.bottom());
            let mut pts = [lt, rb];
            ts.map_points(&mut pts);
            Self::from_points(&pts)
        }
    }

    /// Applies a bounding box transform.
    pub fn bbox_transform(&self, bbox: NonZeroRect) -> Self {
        let x = self.x() * bbox.width() + bbox.x();
        let y = self.y() * bbox.height() + bbox.y();
        let w = self.width() * bbox.width();
        let h = self.height() * bbox.height();
        Self::from_xywh(x, y, w, h).unwrap()
    }

    /// Converts into [`NonZeroRect`].
    pub fn to_non_zero_rect(&self) -> Option<NonZeroRect> {
        NonZeroRect::from_xywh(self.x(), self.y(), self.width(), self.height())
    }
}

fn checked_f32_sub(a: f32, b: f32) -> Option<f32> {
    debug_assert!(a.is_finite());
    debug_assert!(b.is_finite());

    let n = a as f64 - b as f64;
    // Not sure if this is perfectly correct.
    if n > f32::MIN as f64 && n < f32::MAX as f64 {
        Some(n as f32)
    } else {
        None
    }
}

#[cfg(test)]
mod rect_tests {
    use super::*;

    #[test]
    fn tests() {
        assert_eq!(Rect::from_ltrb(10.0, 10.0, 5.0, 10.0), None);
        assert_eq!(Rect::from_ltrb(10.0, 10.0, 10.0, 5.0), None);
        assert_eq!(Rect::from_ltrb(f32::NAN, 10.0, 10.0, 10.0), None);
        assert_eq!(Rect::from_ltrb(10.0, f32::NAN, 10.0, 10.0), None);
        assert_eq!(Rect::from_ltrb(10.0, 10.0, f32::NAN, 10.0), None);
        assert_eq!(Rect::from_ltrb(10.0, 10.0, 10.0, f32::NAN), None);
        assert_eq!(Rect::from_ltrb(10.0, 10.0, 10.0, f32::INFINITY), None);

        let rect = Rect::from_ltrb(10.0, 20.0, 30.0, 40.0).unwrap();
        assert_eq!(rect.left(), 10.0);
        assert_eq!(rect.top(), 20.0);
        assert_eq!(rect.right(), 30.0);
        assert_eq!(rect.bottom(), 40.0);
        assert_eq!(rect.width(), 20.0);
        assert_eq!(rect.height(), 20.0);

        let rect = Rect::from_ltrb(-30.0, 20.0, -10.0, 40.0).unwrap();
        assert_eq!(rect.width(), 20.0);
        assert_eq!(rect.height(), 20.0);
    }

    #[test]
    fn transform() {
        // Tests based on 2x2 rectangle
        let rect = Rect::from_ltrb(1.0, 2.0, 3.0, 4.0).unwrap();

        let ts = Transform::identity();
        assert_eq!(rect.transform(ts).unwrap(), rect);

        // Scale x by 1x, y by 2x
        let ts = Transform::from_scale(1.0, 2.0);
        let rect_ts: Rect = Rect::from_ltrb(1.0, 4.0, 3.0, 8.0).unwrap();
        assert_eq!(rect.transform(ts).unwrap(), rect_ts);

        // Skew box along x-axis - vertical lines at y=c go to y=c+x and
        // horizontal lines stay put. Result is bounding box
        let ts = Transform::from_skew(1.0, 0.0);
        let bounding_rect: Rect = Rect::from_ltrb(3.0, 2.0, 7.0, 4.0).unwrap();
        assert_eq!(rect.transform(ts).unwrap(), bounding_rect);

        // Skew box along y-axis - horizontal lines at x=c go to x=c+2y
        let ts = Transform::from_skew(0.0, 2.0);
        let bounding_rect: Rect = Rect::from_ltrb(1.0, 4.0, 3.0, 10.0).unwrap();
        assert_eq!(rect.transform(ts).unwrap(), bounding_rect);
    }
}

/// A rectangle defined by left, top, right and bottom edges.
///
/// Similar to [`Rect`], but width and height guarantee to be non-zero and positive.
///
/// # Guarantees
///
/// - All values are finite.
/// - Left edge is < right.
/// - Top edge is < bottom.
/// - Width and height are <= f32::MAX.
/// - Width and height are > 0.0
#[allow(missing_docs)]
#[derive(Copy, Clone, PartialEq)]
pub struct NonZeroRect {
    left: FiniteF32,
    top: FiniteF32,
    right: FiniteF32,
    bottom: FiniteF32,
}

impl core::fmt::Debug for NonZeroRect {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("NonZeroRect")
            .field("left", &self.left.get())
            .field("top", &self.top.get())
            .field("right", &self.right.get())
            .field("bottom", &self.bottom.get())
            .finish()
    }
}

impl NonZeroRect {
    /// Creates new `NonZeroRect`.
    pub fn from_ltrb(left: f32, top: f32, right: f32, bottom: f32) -> Option<Self> {
        let left = FiniteF32::new(left)?;
        let top = FiniteF32::new(top)?;
        let right = FiniteF32::new(right)?;
        let bottom = FiniteF32::new(bottom)?;

        if left.get() < right.get() && top.get() < bottom.get() {
            // Width and height must not overflow.
            checked_f32_sub(right.get(), left.get())?;
            checked_f32_sub(bottom.get(), top.get())?;

            Some(Self {
                left,
                top,
                right,
                bottom,
            })
        } else {
            None
        }
    }

    /// Creates new `NonZeroRect`.
    pub fn from_xywh(x: f32, y: f32, w: f32, h: f32) -> Option<Self> {
        Self::from_ltrb(x, y, w + x, h + y)
    }

    /// Returns the left edge.
    pub fn left(&self) -> f32 {
        self.left.get()
    }

    /// Returns the top edge.
    pub fn top(&self) -> f32 {
        self.top.get()
    }

    /// Returns the right edge.
    pub fn right(&self) -> f32 {
        self.right.get()
    }

    /// Returns the bottom edge.
    pub fn bottom(&self) -> f32 {
        self.bottom.get()
    }

    /// Returns rect's X position.
    pub fn x(&self) -> f32 {
        self.left.get()
    }

    /// Returns rect's Y position.
    pub fn y(&self) -> f32 {
        self.top.get()
    }

    /// Returns rect's width.
    pub fn width(&self) -> f32 {
        self.right.get() - self.left.get()
    }

    /// Returns rect's height.
    pub fn height(&self) -> f32 {
        self.bottom.get() - self.top.get()
    }

    /// Returns rect's size.
    pub fn size(&self) -> Size {
        Size::from_wh(self.width(), self.height()).unwrap()
    }

    /// Translates the rect to the specified position.
    pub fn translate_to(&self, x: f32, y: f32) -> Option<Self> {
        Self::from_xywh(x, y, self.width(), self.height())
    }

    /// Transforms the rect using the provided `Transform`.
    ///
    /// If the transform is a skew, the result will be a bounding box around the skewed rectangle.
    pub fn transform(&self, ts: Transform) -> Option<Self> {
        if ts.is_identity() {
            Some(*self)
        } else {
            self.to_rect().transform(ts)?.to_non_zero_rect()
        }
    }

    /// Applies a bounding box transform.
    pub fn bbox_transform(&self, bbox: NonZeroRect) -> Self {
        let x = self.x() * bbox.width() + bbox.x();
        let y = self.y() * bbox.height() + bbox.y();
        let w = self.width() * bbox.width();
        let h = self.height() * bbox.height();
        Self::from_xywh(x, y, w, h).unwrap()
    }

    /// Converts into [`Rect`].
    pub fn to_rect(&self) -> Rect {
        Rect::from_xywh(self.x(), self.y(), self.width(), self.height()).unwrap()
    }

    /// Converts into [`IntRect`].
    pub fn to_int_rect(&self) -> IntRect {
        IntRect::from_xywh(
            self.x().floor() as i32,
            self.y().floor() as i32,
            core::cmp::max(1, self.width().ceil() as u32),
            core::cmp::max(1, self.height().ceil() as u32),
        )
        .unwrap()
    }
}
