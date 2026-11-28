// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE file in the project root for license information.
// ---------------------------------------------------------------------------------------------

use crate::{Family, MemoryError, MemoryScope, OwnerTicket};
use std::{
    alloc::Layout,
    mem::ManuallyDrop,
    ops::Deref,
    ptr::NonNull,
    sync::atomic::{AtomicUsize, Ordering, fence},
};

struct Inner<T> {
    references: AtomicUsize,
    value: ManuallyDrop<T>,
    ticket: ManuallyDrop<OwnerTicket>,
}

/// A narrow strong owner for receipt regions, with an exact prepaid allocation.
/// Clone does not allocate. There are no Weak records or ambient roots.
pub struct FundedShared<T> {
    pointer: NonNull<Inner<T>>,
}

// SAFETY: identical sharing conditions to Arc<T>; atomic count and immutable
// access, with user interior mutability constrained by T: Send + Sync.
unsafe impl<T: Send + Sync> Send for FundedShared<T> {}
unsafe impl<T: Send + Sync> Sync for FundedShared<T> {}

impl<T> FundedShared<T> {
    pub fn try_new_in(scope: &MemoryScope, family: Family, value: T) -> Result<Self, MemoryError> {
        let layout = Layout::new::<Inner<T>>();
        let ticket = scope.reserve(layout, family)?;
        scope.before_allocate(layout, family)?;
        // SAFETY: complete exact Inner Layout was admitted before allocation.
        let pointer = NonNull::new(unsafe { std::alloc::alloc(layout) }.cast::<Inner<T>>())
            .ok_or_else(|| {
                scope.record_failure(MemoryError::OutOfMemory);
                MemoryError::OutOfMemory
            })?;
        // SAFETY: unique correctly aligned storage, initialized exactly once.
        unsafe {
            pointer.as_ptr().write(Inner {
                references: AtomicUsize::new(1),
                value: ManuallyDrop::new(value),
                ticket: ManuallyDrop::new(ticket.commit()),
            })
        };
        Ok(Self { pointer })
    }

    pub fn strong_count(&self) -> usize {
        // SAFETY: self owns one reference, keeping the count alive.
        unsafe { self.pointer.as_ref() }
            .references
            .load(Ordering::Acquire)
    }

    pub fn ptr_eq(left: &Self, right: &Self) -> bool {
        left.pointer == right.pointer
    }
}

impl<T> Clone for FundedShared<T> {
    fn clone(&self) -> Self {
        // SAFETY: self retains a live reference through this increment.
        let previous = unsafe { self.pointer.as_ref() }
            .references
            .fetch_add(1, Ordering::Relaxed);
        // Same unreachable refcount-overflow defence as Arc; this is not a
        // budget or input rejection path and never converts OOM into a panic.
        if previous >= isize::MAX as usize {
            std::process::abort();
        }
        Self {
            pointer: self.pointer,
        }
    }
}

impl<T> Deref for FundedShared<T> {
    type Target = T;
    fn deref(&self) -> &T {
        // SAFETY: this strong owner keeps the initialized immutable value live.
        unsafe { &self.pointer.as_ref().value }
    }
}

struct FreeShared<T> {
    pointer: NonNull<Inner<T>>,
    ticket: ManuallyDrop<OwnerTicket>,
}
impl<T> Drop for FreeShared<T> {
    fn drop(&mut self) {
        // SAFETY: last reference, value already dropped (or unwinding). Move
        // receipt was removed before freeing its old inline storage.
        unsafe {
            std::alloc::dealloc(self.pointer.as_ptr().cast(), Layout::new::<Inner<T>>());
            ManuallyDrop::drop(&mut self.ticket);
        }
    }
}

impl<T> Drop for FundedShared<T> {
    fn drop(&mut self) {
        // SAFETY: self owns a live count; only last owner accesses value mutably.
        let inner = unsafe { self.pointer.as_ref() };
        if inner.references.fetch_sub(1, Ordering::Release) != 1 {
            return;
        }
        fence(Ordering::Acquire);
        // SAFETY: no other reference remains; ticket is moved exactly once.
        let inner = unsafe { self.pointer.as_mut() };
        let cleanup = FreeShared {
            pointer: self.pointer,
            ticket: ManuallyDrop::new(unsafe { ManuallyDrop::take(&mut inner.ticket) }),
        };
        // SAFETY: initialized T is dropped once; cleanup also runs on unwind.
        unsafe { ManuallyDrop::drop(&mut inner.value) };
        drop(cleanup);
    }
}

impl<T: std::fmt::Debug> std::fmt::Debug for FundedShared<T> {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.deref().fmt(formatter)
    }
}
