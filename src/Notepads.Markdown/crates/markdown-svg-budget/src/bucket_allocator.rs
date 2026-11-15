// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE file in the project root for license information.
// ---------------------------------------------------------------------------------------------
//! Exact paid blocks for the private outline table. A block owns its lease in
//! an inline header, so cloning the allocator does not require a pointer map.
use crate::{Error, FailureKind, Lease, LimitKind, ResourceKind, ResourcesRef};
use allocator_api2::alloc::{AllocError, Allocator, Global};
use core::{alloc::Layout, mem, ptr::NonNull};

#[repr(C)]
struct Header {
    lease: Lease,
}

/// Allocator for one request and one resource category. The complete requested
/// block, including the header and alignment gaps, is paid before allocation.
#[derive(Clone, Debug)]
pub struct BucketAllocator {
    resources: ResourcesRef,
    kind: ResourceKind,
}

/// Own a physical block before any admission or initialization callback. Its
/// payload is not yet public and its header does not contain an initialized T.
struct PendingBlock {
    base: NonNull<u8>,
    layout: Layout,
    lease: Option<Lease>,
}

impl Drop for PendingBlock {
    fn drop(&mut self) {
        // SAFETY: this guard exclusively owns the allocation from Global with
        // this exact layout. No header has taken its lease while the guard is
        // armed; no caller can access its payload on this failure/unwind path.
        unsafe { Global.deallocate(self.base, self.layout) };
        drop(self.lease.take());
    }
}

impl BucketAllocator {
    /// Borrow the original payer without exposing its block header or lease.
    pub fn resources(&self) -> &ResourcesRef {
        &self.resources
    }

    pub fn new(resources: ResourcesRef, kind: ResourceKind) -> Self {
        Self { resources, kind }
    }

    pub fn belongs_to(&self, resources: &ResourcesRef) -> bool {
        self.resources.is_same_request(resources)
    }

    fn layout(&self, payload: Layout) -> Result<(Layout, usize), AllocError> {
        Layout::new::<Header>()
            .extend(payload)
            .map(|(layout, offset)| (layout.pad_to_align(), offset))
            .map_err(|_| {
                self.resources
                    .record(Error::new(FailureKind::Quota, LimitKind::Capacity));
                AllocError
            })
    }

    fn allocate_block(&self, payload: Layout, zeroed: bool) -> Result<NonNull<[u8]>, AllocError> {
        self.resources.checkpoint(1).map_err(|_| AllocError)?;
        let (layout, offset) = self.layout(payload)?;
        let lease = self
            .resources
            .reserve(self.kind, layout.size())
            .map_err(|_| AllocError)?;
        let block = Global.allocate(layout).map_err(|_| {
            self.resources
                .record(Error::new(FailureKind::Allocation, LimitKind::Capacity));
            AllocError
        })?;
        let mut pending = PendingBlock {
            base: block.cast(),
            layout,
            lease: Some(lease),
        };
        // Fixed Global returns precisely the requested layout. Never expose an
        // oversized physical result as extra table buckets or uncharged space.
        if block.len() != layout.size() {
            self.resources
                .record(Error::new(FailureKind::Invariant, LimitKind::Capacity));
            return Err(AllocError);
        }
        pending
            .lease
            .as_mut()
            .expect("pending bucket lease")
            .allocation_committed()
            .map_err(|_| AllocError)?;
        // SAFETY: extend guarantees this offset is within the new allocation,
        // aligned for payload. It also covers the one-past zero-sized payload.
        let pointer = unsafe { NonNull::new_unchecked(pending.base.as_ptr().add(offset)) };
        if zeroed {
            let mut written = 0;
            while written < payload.size() {
                let count = (payload.size() - written).min(256);
                self.resources
                    .checkpoint(count as u64)
                    .map_err(|_| AllocError)?;
                // SAFETY: only the requested payload suffix is initialized;
                // the header and alignment padding are never overwritten.
                unsafe { pointer.as_ptr().add(written).write_bytes(0, count) };
                written += count;
            }
        }
        self.resources.checkpoint(0).map_err(|_| AllocError)?;
        let lease = pending.lease.take().expect("pending bucket lease");
        // SAFETY: the base has Header alignment and enough space. This short
        // no-callback commit moves the unique lease into its header, and then
        // disarms the guard. No fallible operation separates those actions.
        unsafe {
            pending
                .base
                .cast::<Header>()
                .as_ptr()
                .write(Header { lease })
        };
        mem::forget(pending);
        Ok(NonNull::slice_from_raw_parts(pointer, payload.size()))
    }

    fn unsupported_resize(&self) -> AllocError {
        self.resources
            .record(Error::new(FailureKind::Invariant, LimitKind::Capacity));
        AllocError
    }
}

// SAFETY: successful allocate returns a fresh block whose public size is
// exactly the requested payload size/alignment. Its inline lease and physical
// block survive until deallocate by any clone of the same allocator. The API2
// fit contract therefore fixes the original payload size/alignment, permitting
// the same header offset/layout to be recomputed. Unsupported resizing returns
// Err without writing or freeing the original block. No borrowed pointer or
// external pointer map is stored in the allocator.
unsafe impl Allocator for BucketAllocator {
    fn allocate(&self, layout: Layout) -> Result<NonNull<[u8]>, AllocError> {
        self.allocate_block(layout, false)
    }

    fn allocate_zeroed(&self, layout: Layout) -> Result<NonNull<[u8]>, AllocError> {
        self.allocate_block(layout, true)
    }

    unsafe fn deallocate(&self, pointer: NonNull<u8>, payload: Layout) {
        let (layout, offset) = self.layout(payload).expect("live bucket layout");
        // SAFETY: Allocator's caller supplies a currently allocated payload and
        // fitting layout. Exact public length forces the original size, and
        // fitting alignment is the original one. Zero-sized payloads also came
        // from a real header block. Move the lease before physically freeing
        // its storage, then refund only after Global has freed that block.
        let base = unsafe { NonNull::new_unchecked(pointer.as_ptr().sub(offset)) };
        let Header { lease } = unsafe { base.cast::<Header>().as_ptr().read() };
        unsafe { Global.deallocate(base, layout) };
        drop(lease);
    }

    unsafe fn grow(
        &self,
        _pointer: NonNull<u8>,
        _old: Layout,
        _new: Layout,
    ) -> Result<NonNull<[u8]>, AllocError> {
        Err(self.unsupported_resize())
    }

    unsafe fn grow_zeroed(
        &self,
        _pointer: NonNull<u8>,
        _old: Layout,
        _new: Layout,
    ) -> Result<NonNull<[u8]>, AllocError> {
        Err(self.unsupported_resize())
    }

    unsafe fn shrink(
        &self,
        _pointer: NonNull<u8>,
        _old: Layout,
        _new: Layout,
    ) -> Result<NonNull<[u8]>, AllocError> {
        Err(self.unsupported_resize())
    }
}
