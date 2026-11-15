// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE file in the project root for license information.
// ---------------------------------------------------------------------------------------------
//! Private normalizer storage. Compatibility and controlled storage are explicit;
//! every controlled allocation retains the original request capability.
use core::{
    cell::Cell,
    cmp::Ordering,
    ops::{Deref, DerefMut},
};
use markdown_svg_budget::{
    BudgetVec, Error as ResourceError, FailureKind, LimitKind, PaidUtf8, ResourceKind, ResourcesRef,
};
use roxmltree::Error;

pub(super) fn failure(resources: Option<&ResourcesRef>, error: ResourceError) -> Error {
    Error::Resource(resources.map_or(error, |r| r.record(error)))
}

pub(super) struct Work {
    resources: Option<ResourcesRef>,
    logical_credit: Cell<u16>,
}
impl Work {
    pub(super) fn new(resources: Option<ResourcesRef>) -> Self {
        Self {
            resources,
            logical_credit: Cell::new(0),
        }
    }
    pub(super) fn resources(&self) -> Option<&ResourcesRef> {
        self.resources.as_ref()
    }
    pub(super) fn event(&self) -> Result<(), Error> {
        if let Some(r) = self.resources() {
            if let Some(error) = r.failure() {
                return Err(Error::Resource(error));
            }
            let credit = self.logical_credit.get();
            if credit == 0 {
                r.checkpoint(256).map_err(Error::Resource)?;
                self.logical_credit.set(255);
            } else {
                self.logical_credit.set(credit - 1);
            }
        }
        Ok(())
    }
    pub(super) fn bytes(&self, count: usize) -> Result<(), Error> {
        if let Some(r) = self.resources() {
            r.consume_scan_work(count).map_err(Error::Resource)?;
        }
        Ok(())
    }
    pub(super) fn compare(&self, a: &str, b: &str) -> Result<Ordering, Error> {
        self.event()?;
        let a = a.as_bytes();
        let b = b.as_bytes();
        let mut offset = 0;
        while offset < a.len().min(b.len()) {
            // Acquire scan permission before reading the next comparison chunk.
            // Consume only examined bytes, retaining unused shared credit.
            let allowance = match self.resources() {
                Some(r) => r.scan_allowance().map_err(Error::Resource)?,
                None => 4096,
            };
            let end = (offset + allowance.min(4096)).min(a.len().min(b.len()));
            // Charge only the bytes actually inspected, not the unexamined tail.
            let mut cursor = offset;
            while cursor < end && a[cursor] == b[cursor] {
                cursor += 1;
            }
            if cursor < end {
                self.bytes(cursor - offset + 1)?;
                return Ok(a[cursor].cmp(&b[cursor]));
            }
            self.bytes(end - offset)?;
            offset = end;
        }
        Ok(a.len().cmp(&b.len()))
    }
    pub(super) fn checked(&self) -> Result<(), Error> {
        if let Some(error) = self.resources().and_then(ResourcesRef::failure) {
            return Err(Error::Resource(error));
        }
        Ok(())
    }
    pub(super) fn semantic(&self, error: Error) -> Error {
        if let Some(r) = self.resources() {
            r.record(ResourceError::new(FailureKind::Decode, LimitKind::None));
        }
        error
    }
}

pub(super) enum Storage<T> {
    Plain(Vec<T>),
    Paid(BudgetVec<T>),
}
impl<T> Storage<T> {
    pub(super) fn new(resources: Option<&ResourcesRef>) -> Self {
        match resources {
            Some(r) => Self::Paid(BudgetVec::new(r.clone(), ResourceKind::Scratch)),
            None => Self::Plain(Vec::new()),
        }
    }
    pub(super) fn try_reserve(&mut self, additional: usize) -> Result<(), Error> {
        match self {
            Self::Paid(v) => v.try_reserve(additional).map_err(Error::Resource),
            Self::Plain(v) => v.try_reserve(additional).map_err(|_| {
                failure(
                    None,
                    ResourceError::new(FailureKind::Allocation, LimitKind::Capacity),
                )
            }),
        }
    }
    pub(super) fn try_push(&mut self, value: T) -> Result<(), Error> {
        match self {
            Self::Paid(v) => v.try_push(value).map_err(Error::Resource),
            Self::Plain(v) => {
                v.try_reserve(1).map_err(|_| {
                    failure(
                        None,
                        ResourceError::new(FailureKind::Allocation, LimitKind::Capacity),
                    )
                })?;
                v.push(value);
                Ok(())
            }
        }
    }
    pub(super) fn pop(&mut self) -> Option<T> {
        match self {
            Self::Plain(v) => v.pop(),
            Self::Paid(v) => v.pop(),
        }
    }
}
impl<T> Deref for Storage<T> {
    type Target = [T];
    fn deref(&self) -> &[T] {
        match self {
            Self::Plain(v) => v,
            Self::Paid(v) => v.as_slice(),
        }
    }
}
impl<T> DerefMut for Storage<T> {
    fn deref_mut(&mut self) -> &mut [T] {
        match self {
            Self::Plain(v) => v,
            Self::Paid(v) => v.as_mut_slice(),
        }
    }
}

/// In-place heapsort with fallible comparisons; failure is never an Ordering.
pub(super) fn try_sort<T>(
    values: &mut [T],
    mut compare: impl FnMut(&T, &T) -> Result<Ordering, Error>,
) -> Result<(), Error> {
    fn sift<T>(
        v: &mut [T],
        mut root: usize,
        end: usize,
        c: &mut impl FnMut(&T, &T) -> Result<Ordering, Error>,
    ) -> Result<(), Error> {
        loop {
            let Some(mut child) = root.checked_mul(2).and_then(|x| x.checked_add(1)) else {
                return Ok(());
            };
            if child >= end {
                return Ok(());
            }
            if child + 1 < end && c(&v[child], &v[child + 1])? == Ordering::Less {
                child += 1;
            }
            if c(&v[root], &v[child])? != Ordering::Less {
                return Ok(());
            }
            v.swap(root, child);
            root = child;
        }
    }
    for root in (0..values.len() / 2).rev() {
        sift(values, root, values.len(), &mut compare)?;
    }
    for end in (1..values.len()).rev() {
        values.swap(0, end);
        sift(values, 0, end, &mut compare)?;
    }
    Ok(())
}

pub(super) enum TextBytes {
    Plain(String),
    Paid(PaidUtf8),
}
pub(crate) struct TextStorage {
    bytes: TextBytes,
    start: usize,
    end: usize,
}
impl TextStorage {
    pub(super) fn new(resources: Option<&ResourcesRef>) -> Self {
        Self {
            bytes: match resources {
                Some(r) => TextBytes::Paid(PaidUtf8::new(r.clone(), ResourceKind::Scratch)),
                None => TextBytes::Plain(String::new()),
            },
            start: 0,
            end: 0,
        }
    }
    pub(super) fn try_push_str(&mut self, value: &str) -> Result<(), Error> {
        debug_assert_eq!(self.start, 0);
        match &mut self.bytes {
            TextBytes::Plain(s) => s.push_str(value),
            TextBytes::Paid(s) => s.try_push_str(value).map_err(Error::Resource)?,
        }
        self.end += value.len();
        Ok(())
    }
    pub(super) fn clear(&mut self) {
        self.start = 0;
        self.end = 0;
        match &mut self.bytes {
            TextBytes::Plain(s) => s.clear(),
            TextBytes::Paid(s) => s.clear(),
        }
    }
    pub(super) fn remove_first_space(&mut self) {
        debug_assert!(self.starts_with(' '));
        self.start += 1;
    }
    pub(super) fn remove_last_space(&mut self) {
        debug_assert!(self.ends_with(' '));
        self.end -= 1;
    }
}
impl Deref for TextStorage {
    type Target = str;
    fn deref(&self) -> &str {
        let s = match &self.bytes {
            TextBytes::Plain(s) => s.as_str(),
            TextBytes::Paid(s) => s.as_str(),
        };
        &s[self.start..self.end]
    }
}
impl core::fmt::Debug for TextStorage {
    fn fmt(&self, f: &mut core::fmt::Formatter) -> core::fmt::Result {
        core::fmt::Debug::fmt(&**self, f)
    }
}
