use super::config::EPSILON;
use crate::model::LayoutPoint;

#[derive(Debug, Clone, Copy, PartialEq)]
pub(super) struct Rect {
    pub left: f64,
    pub right: f64,
    pub top: f64,
    pub bottom: f64,
}

impl Rect {
    pub fn from_center(x: f64, y: f64, width: f64, height: f64) -> Self {
        Self {
            left: x - width / 2.0,
            right: x + width / 2.0,
            top: y - height / 2.0,
            bottom: y + height / 2.0,
        }
    }

    pub fn width(self) -> f64 {
        (self.right - self.left).max(0.0)
    }

    pub fn height(self) -> f64 {
        (self.bottom - self.top).max(0.0)
    }

    pub fn center(self) -> LayoutPoint {
        LayoutPoint {
            x: (self.left + self.right) / 2.0,
            y: (self.top + self.bottom) / 2.0,
        }
    }

    pub fn union(&mut self, other: Self) {
        self.left = self.left.min(other.left);
        self.right = self.right.max(other.right);
        self.top = self.top.min(other.top);
        self.bottom = self.bottom.max(other.bottom);
    }

    pub fn inflated(self, amount: f64) -> Self {
        Self {
            left: self.left - amount,
            right: self.right + amount,
            top: self.top - amount,
            bottom: self.bottom + amount,
        }
    }

    pub fn contains_point(self, point: &LayoutPoint, inset: f64) -> bool {
        point.x > self.left + inset
            && point.x < self.right - inset
            && point.y > self.top + inset
            && point.y < self.bottom - inset
    }
}

pub(super) fn segment_blocked(
    from: &LayoutPoint,
    to: &LayoutPoint,
    obstacles: &[(String, Rect)],
    excluded: &[&str],
) -> bool {
    let min_x = from.x.min(to.x);
    let max_x = from.x.max(to.x);
    let min_y = from.y.min(to.y);
    let max_y = from.y.max(to.y);
    obstacles.iter().any(|(id, rect)| {
        if excluded.contains(&id.as_str()) {
            return false;
        }
        if (from.y - to.y).abs() <= EPSILON {
            rect.top < from.y && rect.bottom > from.y && rect.right > min_x && rect.left < max_x
        } else if (from.x - to.x).abs() <= EPSILON {
            rect.left < from.x && rect.right > from.x && rect.bottom > min_y && rect.top < max_y
        } else {
            true
        }
    })
}
