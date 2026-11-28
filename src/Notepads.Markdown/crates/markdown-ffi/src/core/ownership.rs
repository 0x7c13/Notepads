// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE file in the project root for license information.
// ---------------------------------------------------------------------------------------------

//! Boundary owner allocation, atomic publication and lease-preserving release.

use markdown_core::error::CoreError;
use markdown_core::policy::{BoundaryReservation, BoundaryWork};
use std::alloc::{Layout, alloc, dealloc};
use std::marker::PhantomData;
use std::ptr::{self, NonNull};

pub struct Owned<T> {
    pub(super) value: T,
    pub(super) lease: BoundaryReservation,
}

/// Allocation remains uninitialized until an infallible post-commit placement.
pub(super) struct Slot<T> {
    pointer: NonNull<Owned<T>>,
    lease: Option<BoundaryReservation>,
    _type: PhantomData<T>,
}

impl<T> Slot<T> {
    pub(super) fn new(mut op: BoundaryWork<'_>) -> Result<Self, CoreError> {
        let layout = Layout::new::<Owned<T>>();
        let lease = op.reserve_boundary(layout.size() as u64)?;
        op.external_write(layout.size() as u64)?;
        // SAFETY: nonzero valid Layout; allocation is owned by this slot until commit.
        let pointer = NonNull::new(unsafe { alloc(layout) }.cast::<Owned<T>>())
            .ok_or_else(|| op.boundary_failure(CoreError::OutOfMemory))?;
        let mut slot = Self {
            pointer,
            lease: Some(lease),
            _type: PhantomData,
        };
        op.admit_actual(
            slot.lease.as_mut().expect("fresh slot reservation"),
            layout.size() as u64,
        )?;
        op.checkpoint_now()?;
        Ok(slot)
    }

    pub(super) fn commit(mut self, value: T) -> *mut Owned<T> {
        let owned = Owned {
            value,
            lease: self.lease.take().expect("uncommitted slot"),
        };
        let pointer = self.pointer.as_ptr();
        // SAFETY: exact aligned writable allocation, initialized once, ownership transferred.
        unsafe { pointer.write(owned) };
        std::mem::forget(self);
        pointer
    }
}

impl<T> Drop for Slot<T> {
    fn drop(&mut self) {
        // SAFETY: slot has not committed, so only raw allocation (no T) is live.
        unsafe { dealloc(self.pointer.as_ptr().cast(), Layout::new::<Owned<T>>()) };
        // Its reservation is a field and drops after the actual deallocation.
    }
}

/// # Safety
/// Live uniquely owned handle; no concurrent calls and no subsequent use.
pub(super) unsafe fn release<T>(handle: *mut Owned<T>) {
    // SAFETY: caller transfers a live initialized allocation; move lease out before freeing it.
    let lease = unsafe { ptr::addr_of!((*handle).lease).read() };
    let dropped = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        // SAFETY: unique value, destroyed once; lease is already moved out.
        unsafe { ptr::drop_in_place(ptr::addr_of_mut!((*handle).value)) };
    }));
    // SAFETY: value has been dropped even on unwind; exact original Layout.
    unsafe { dealloc(handle.cast(), Layout::new::<Owned<T>>()) };
    drop(lease);
    if let Err(payload) = dropped {
        std::panic::resume_unwind(payload);
    }
}
