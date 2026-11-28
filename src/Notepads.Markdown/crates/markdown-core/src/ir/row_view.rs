// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE file in the project root for license information.
// ---------------------------------------------------------------------------------------------

//! Borrowed immutable rows. A range never promises cross-page contiguity.

use super::storage::FinalSlot;
use crate::memory::FrozenPagedRows;

#[derive(Clone, Copy)]
enum Backing<'a, T: Copy> {
    Slice(&'a [T]),
    Paged(&'a FrozenPagedRows<FinalSlot<T>>),
    CompletedPaged(&'a FrozenPagedRows<T>),
}

#[derive(Clone, Copy)]
pub struct RowView<'a, T: Copy> {
    backing: Backing<'a, T>,
    start: usize,
    len: usize,
}

impl<'a, T: Copy> RowView<'a, T> {
    pub(in crate::ir) fn completed_paged(rows: &'a FrozenPagedRows<T>) -> Self {
        Self {
            backing: Backing::CompletedPaged(rows),
            start: 0,
            len: rows.len(),
        }
    }
    pub(crate) fn paged(rows: &'a FrozenPagedRows<FinalSlot<T>>) -> Self {
        Self {
            backing: Backing::Paged(rows),
            start: 0,
            len: rows.len(),
        }
    }
    pub fn len(self) -> usize {
        self.len
    }
    pub fn is_empty(self) -> bool {
        self.len == 0
    }
    pub fn get(self, index: usize) -> Option<&'a T> {
        if index >= self.len {
            return None;
        }
        let index = self.start.checked_add(index)?;
        match self.backing {
            Backing::Slice(rows) => rows.get(index),
            Backing::CompletedPaged(rows) => rows.get(index),
            Backing::Paged(rows) => match rows.get(index)? {
                FinalSlot::Reserved => None,
                FinalSlot::Written(value) => Some(value),
            },
        }
    }
    pub fn first(self) -> Option<&'a T> {
        self.get(0)
    }
    pub fn last(self) -> Option<&'a T> {
        self.len.checked_sub(1).and_then(|i| self.get(i))
    }
    pub fn range(self, start: usize, len: usize) -> Option<Self> {
        if start.checked_add(len)? > self.len {
            return None;
        }
        Some(Self {
            backing: self.backing,
            start: self.start.checked_add(start)?,
            len,
        })
    }
    pub fn iter(self) -> RowIter<'a, T> {
        RowIter {
            rows: self,
            front: 0,
            back: self.len,
        }
    }
}
impl<'a, T: Copy> From<&'a [T]> for RowView<'a, T> {
    fn from(rows: &'a [T]) -> Self {
        Self {
            backing: Backing::Slice(rows),
            start: 0,
            len: rows.len(),
        }
    }
}
impl<T: Copy> std::ops::Index<usize> for RowView<'_, T> {
    type Output = T;
    fn index(&self, index: usize) -> &T {
        self.get(index).expect("complete frozen row index")
    }
}
impl<T: Copy + std::fmt::Debug> std::fmt::Debug for RowView<'_, T> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_list().entries(self.iter()).finish()
    }
}
impl<T: Copy + PartialEq> PartialEq for RowView<'_, T> {
    fn eq(&self, other: &Self) -> bool {
        self.len == other.len && self.iter().eq(other.iter())
    }
}
impl<T: Copy + Eq> Eq for RowView<'_, T> {}

pub struct RowIter<'a, T: Copy> {
    rows: RowView<'a, T>,
    front: usize,
    back: usize,
}
impl<'a, T: Copy> Iterator for RowIter<'a, T> {
    type Item = &'a T;
    fn next(&mut self) -> Option<Self::Item> {
        if self.front == self.back {
            return None;
        }
        let at = self.front;
        self.front += 1;
        self.rows.get(at)
    }
    fn size_hint(&self) -> (usize, Option<usize>) {
        let n = self.back - self.front;
        (n, Some(n))
    }
}
impl<T: Copy> DoubleEndedIterator for RowIter<'_, T> {
    fn next_back(&mut self) -> Option<Self::Item> {
        if self.front == self.back {
            return None;
        }
        self.back -= 1;
        self.rows.get(self.back)
    }
}
impl<T: Copy> ExactSizeIterator for RowIter<'_, T> {}
impl<'a, T: Copy> IntoIterator for RowView<'a, T> {
    type Item = &'a T;
    type IntoIter = RowIter<'a, T>;
    fn into_iter(self) -> Self::IntoIter {
        self.iter()
    }
}

pub(crate) trait RowRange<T: Copy> {
    type View;
    fn bounded(self, start: usize, len: usize) -> Option<Self::View>;
}
impl<'a, T: Copy> RowRange<T> for &'a [T] {
    type View = &'a [T];
    fn bounded(self, start: usize, len: usize) -> Option<Self::View> {
        self.get(start..start.checked_add(len)?)
    }
}
impl<'a, T: Copy> RowRange<T> for RowView<'a, T> {
    type View = Self;
    fn bounded(self, start: usize, len: usize) -> Option<Self> {
        self.range(start, len)
    }
}
