// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE file in the project root for license information.
// ---------------------------------------------------------------------------------------------

use crate::{BoundedClone, Family, GrowthTicket, MemoryError, MemoryScope, OwnerTicket};
use std::{
    alloc::Layout,
    ops::{Deref, DerefMut},
    ptr::NonNull,
};

/// Exact-layout, fallible owned box for the controlled parser's AST nodes.
pub struct FundedBox<T> {
    pointer: NonNull<T>,
    _ticket: OwnerTicket,
}

// SAFETY: same ownership/access rules as Box<T>; the authority is Send + Sync.
unsafe impl<T: Send> Send for FundedBox<T> {}
unsafe impl<T: Sync> Sync for FundedBox<T> {}

impl<T> FundedBox<T> {
    pub fn try_new_in(scope: &MemoryScope, family: Family, value: T) -> Result<Self, MemoryError> {
        let layout = Layout::new::<T>();
        let ticket = scope.reserve(layout, family)?;
        Self::try_with_prepared(ticket, value)
    }

    pub fn try_with_prepared(ticket: GrowthTicket, value: T) -> Result<Self, MemoryError> {
        let layout = Layout::new::<T>();
        if ticket.layout() != layout { return Err(MemoryError::InvalidState); }
        let scope = ticket.scope().clone();
        let family = ticket.family();
        let pointer = if layout.size() == 0 {
            NonNull::dangling()
        } else {
            scope.before_allocate(layout, family)?;
            // SAFETY: the complete exact Layout was reserved before allocation.
            match NonNull::new(unsafe { std::alloc::alloc(layout) }.cast::<T>()) {
                Some(pointer) => pointer,
                None => {
                    scope.record_failure(MemoryError::OutOfMemory);
                    return Err(MemoryError::OutOfMemory);
                }
            }
        };
        // SAFETY: unique, aligned storage for T (or valid aligned ZST pointer).
        unsafe { pointer.as_ptr().write(value) };
        Ok(Self {
            pointer,
            _ticket: ticket.commit(),
        })
    }

    pub fn try_clone_in(&self, scope: &MemoryScope, family: Family) -> Result<Self, MemoryError>
    where
        T: BoundedClone,
    {
        Self::try_new_in(scope, family, self.deref().try_clone_in(scope, family)?)
    }
}

impl<T> Deref for FundedBox<T> {
    type Target = T;
    fn deref(&self) -> &T {
        // SAFETY: constructor initialized T and this owner retains the block.
        unsafe { self.pointer.as_ref() }
    }
}

impl<T> DerefMut for FundedBox<T> {
    fn deref_mut(&mut self) -> &mut T {
        // SAFETY: unique mutable ownership, as for Box<T>.
        unsafe { self.pointer.as_mut() }
    }
}

impl<T> AsRef<T> for FundedBox<T> {
    fn as_ref(&self) -> &T {
        self
    }
}
impl<T: std::fmt::Debug> std::fmt::Debug for FundedBox<T> {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.deref().fmt(formatter)
    }
}

struct FreeBlock {
    pointer: NonNull<u8>,
    layout: Layout,
}
impl Drop for FreeBlock {
    fn drop(&mut self) {
        if self.layout.size() != 0 {
            // SAFETY: exact matching allocation, uniquely held by this guard.
            unsafe { std::alloc::dealloc(self.pointer.as_ptr(), self.layout) };
        }
    }
}

impl<T> Drop for FundedBox<T> {
    fn drop(&mut self) {
        let free = FreeBlock {
            pointer: self.pointer.cast(),
            layout: Layout::new::<T>(),
        };
        // SAFETY: this is the unique initialized T. The guard frees its block
        // even during unwinding, before the enclosing receipt field is dropped.
        unsafe { std::ptr::drop_in_place(self.pointer.as_ptr()) };
        drop(free);
    }
}

impl<T: BoundedClone> BoundedClone for FundedBox<T> {
    fn try_clone_in(&self, scope: &MemoryScope, family: Family) -> Result<Self, MemoryError> {
        FundedBox::try_clone_in(self, scope, family)
    }
}
