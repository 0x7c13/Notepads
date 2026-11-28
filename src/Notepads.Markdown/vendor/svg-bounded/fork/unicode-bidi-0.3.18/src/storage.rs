// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE file in the project root for license information.
// ---------------------------------------------------------------------------------------------
//! Storage and prepaid work shared by ordinary and request-controlled algorithms.

use alloc::vec::{Drain, Vec};
use core::{
    convert::Infallible,
    mem::size_of,
    ops::{Deref, DerefMut},
};

#[cfg(feature = "notepads-markdown-resources")]
use markdown_svg_budget::{
    BudgetVec, Error, FailureKind, LimitKind, ResourceKind, ResourcesRef,
};

const MAX_EVENTS: usize = 256;
const MAX_COPY_BYTES: usize = 65_536;

pub(crate) trait Storage {
    type Error;
    type Buffer<T>: BufferOps<T, Error = Self::Error>;

    fn buffer<T>(&self) -> Self::Buffer<T>;
    fn before_events(&mut self, count: usize) -> Result<(), Self::Error>;
    fn finish(&mut self) -> Result<(), Self::Error>;
}

/// Deliberately excludes Clone, a growable Vec borrow, and unpaired storage escape.
pub(crate) trait BufferOps<T>: Deref<Target = [T]> + DerefMut {
    type Error;
    type Drain<'a>: DoubleEndedIterator<Item = T> + ExactSizeIterator
    where
        Self: 'a,
        T: 'a;

    fn try_reserve(&mut self, additional: usize) -> Result<(), Self::Error>;
    fn try_reserve_exact(&mut self, additional: usize) -> Result<(), Self::Error>;
    fn try_push(&mut self, value: T) -> Result<(), Self::Error>;
    fn pop(&mut self) -> Option<T>;
    fn drain_all(&mut self) -> Self::Drain<'_>;
}

pub(crate) struct NoControl;

impl Storage for NoControl {
    type Error = Infallible;
    type Buffer<T> = Vec<T>;

    fn buffer<T>(&self) -> Vec<T> {
        Vec::new()
    }

    fn before_events(&mut self, _count: usize) -> Result<(), Infallible> {
        Ok(())
    }

    fn finish(&mut self) -> Result<(), Infallible> {
        Ok(())
    }
}

impl<T> BufferOps<T> for Vec<T> {
    type Error = Infallible;
    type Drain<'a>
        = Drain<'a, T>
    where
        Self: 'a,
        T: 'a;

    fn try_reserve(&mut self, additional: usize) -> Result<(), Infallible> {
        // Preserve ordinary Vec allocation/panic boundaries. Infallible is never
        // fabricated from a std fallible allocation error.
        Vec::reserve(self, additional);
        Ok(())
    }

    fn try_reserve_exact(&mut self, additional: usize) -> Result<(), Infallible> {
        Vec::reserve_exact(self, additional);
        Ok(())
    }

    fn try_push(&mut self, value: T) -> Result<(), Infallible> {
        Vec::push(self, value);
        Ok(())
    }

    fn pop(&mut self) -> Option<T> {
        Vec::pop(self)
    }

    fn drain_all(&mut self) -> Drain<'_, T> {
        Vec::drain(self, ..)
    }
}

#[cfg(feature = "notepads-markdown-resources")]
pub(crate) struct RequestStorage {
    request: ResourcesRef,
}

#[cfg(feature = "notepads-markdown-resources")]
impl RequestStorage {
    pub(crate) fn new(request: &ResourcesRef) -> Result<Self, Error> {
        request.checkpoint(0)?;
        Ok(Self {
            request: request.clone(),
        })
    }
}

#[cfg(feature = "notepads-markdown-resources")]
impl Storage for RequestStorage {
    type Error = Error;
    type Buffer<T> = BudgetVec<T>;

    fn buffer<T>(&self) -> BudgetVec<T> {
        BudgetVec::new(self.request.clone(), ResourceKind::Scratch)
    }

    fn before_events(&mut self, count: usize) -> Result<(), Error> {
        if count > MAX_EVENTS {
            return Err(self
                .request
                .record(Error::new(FailureKind::Invariant, LimitKind::Work)));
        }
        // Exact known work, paid before its bounded window. No future credit.
        self.request.checkpoint(count as u64)
    }

    fn finish(&mut self) -> Result<(), Error> {
        self.request.checkpoint(0)
    }
}

#[cfg(feature = "notepads-markdown-resources")]
impl<T> BufferOps<T> for BudgetVec<T> {
    type Error = Error;
    type Drain<'a>
        = Drain<'a, T>
    where
        Self: 'a,
        T: 'a;

    fn try_reserve(&mut self, additional: usize) -> Result<(), Error> {
        BudgetVec::try_reserve(self, additional)
    }

    fn try_reserve_exact(&mut self, additional: usize) -> Result<(), Error> {
        BudgetVec::try_reserve_exact(self, additional)
    }

    fn try_push(&mut self, value: T) -> Result<(), Error> {
        BudgetVec::try_push(self, value)
    }

    fn pop(&mut self) -> Option<T> {
        BudgetVec::pop(self)
    }

    fn drain_all(&mut self) -> Drain<'_, T> {
        // Drain borrows the original allocation owner. Its capacity and lease
        // stay paired even if the iterator exits before yielding every value.
        BudgetVec::drain(self, ..)
    }
}

fn copy_window<T>() -> usize {
    // Algorithm callers use scalar/range/inline records, never a giant element
    // or an allocating Clone. Each actual copy is bounded by these two limits.
    debug_assert!(size_of::<T>() <= MAX_COPY_BYTES);
    (MAX_COPY_BYTES / size_of::<T>().max(1))
        .max(1)
        .min(MAX_EVENTS)
}

pub(crate) fn copy_slice_in<C: Storage, T: Copy>(
    source: &[T],
    cx: &mut C,
) -> Result<C::Buffer<T>, C::Error> {
    let mut result = cx.buffer();
    result.try_reserve_exact(source.len())?;
    for chunk in source.chunks(copy_window::<T>()) {
        cx.before_events(chunk.len())?;
        for value in chunk {
            result.try_push(*value)?;
        }
    }
    Ok(result)
}

pub(crate) fn resize_with_in<C: Storage, T>(
    buffer: &mut C::Buffer<T>,
    length: usize,
    mut make: impl FnMut() -> T,
    cx: &mut C,
) -> Result<(), C::Error> {
    let window = copy_window::<T>();
    if length > buffer.len() {
        buffer.try_reserve_exact(length - buffer.len())?;
        while buffer.len() < length {
            let count = (length - buffer.len()).min(window);
            cx.before_events(count)?;
            for _ in 0..count {
                // Only the closed set of no-allocation scalar/range/inline
                // constructors inside the bidi algorithm calls this helper.
                buffer.try_push(make())?;
            }
        }
    } else {
        while buffer.len() > length {
            let count = (buffer.len() - length).min(window);
            cx.before_events(count)?;
            for _ in 0..count {
                drop(buffer.pop());
            }
        }
    }
    Ok(())
}

pub(crate) fn stable_sort_by_key_in<C: Storage, T: Copy, K: Ord>(
    values: &mut [T],
    key: impl Fn(&T) -> K,
    cx: &mut C,
) -> Result<(), C::Error> {
    let length = values.len();
    if length < 2 {
        return Ok(());
    }
    let mut scratch = copy_slice_in(values, cx)?;
    let mut width = 1usize;
    while width < length {
        cx.before_events(1)?;
        let mut start = 0;
        while start < length {
            cx.before_events(1)?;
            let middle = start.saturating_add(width).min(length);
            let end = middle.saturating_add(width).min(length);
            let (mut left, mut right, mut write) = (start, middle, start);
            while left < middle && right < end {
                cx.before_events(1)?;
                // Equal keys select the left record: stability is intentional.
                // Bidi callers supply a pure scalar field lookup as the key.
                let selected = if key(&values[left]) <= key(&values[right]) {
                    let index = left;
                    left += 1;
                    index
                } else {
                    let index = right;
                    right += 1;
                    index
                };
                cx.before_events(1)?;
                scratch[write] = values[selected];
                write += 1;
            }
            while left < middle {
                cx.before_events(1)?;
                scratch[write] = values[left];
                left += 1;
                write += 1;
            }
            while right < end {
                cx.before_events(1)?;
                scratch[write] = values[right];
                right += 1;
                write += 1;
            }
            start = end;
        }
        for (target, source) in values
            .chunks_mut(copy_window::<T>())
            .zip(scratch.chunks(copy_window::<T>()))
        {
            cx.before_events(target.len())?;
            target.copy_from_slice(source);
        }
        width = width.saturating_mul(2);
    }
    Ok(())
}

#[cfg(test)]
#[path = "storage_tests.rs"]
pub(crate) mod tests;
