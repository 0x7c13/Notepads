// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE file in the project root for license information.
// ---------------------------------------------------------------------------------------------
//! Shared allocations have one private exit: move the node out after Arc frees
//! its allocation, then destroy the payload before returning the allocation fee.
use crate::{BudgetVec, Error, Lease, ResourceKind, ResourcesRef};
use alloc::sync::Arc;
use core::{alloc::Layout, fmt, ops::Deref, sync::atomic::AtomicUsize};

#[repr(C)]
struct SharedNode<T> {
    value: T,
    fee: Lease,
}

// Matches the repr(C, align(2)) ArcInner of the fixed Rust 1.99.1 toolchain.
// This is a Layout calculation only; no std-private pointer is accessed.
// Toolchain upgrades must repeat the real allocator size/alignment observations.
#[repr(C, align(2))]
struct ArcAllocation<T> {
    strong: AtomicUsize,
    weak: AtomicUsize,
    value: T,
}

pub(crate) fn shared_layout<T>() -> Layout {
    arc_layout::<SharedNode<T>>()
}
pub(crate) fn arc_layout<T>() -> Layout {
    Layout::new::<ArcAllocation<T>>()
}

/// Immutable shared ownership, without an Arc, Weak, raw or Pin escape hatch.
/// The fee covers the complete Arc allocation, including inline T and Lease.
/// Independent allocations inside T must already have their own paid owners.
/// Standard Arc system-OOM and reference-count-overflow abort boundaries remain.
pub struct PaidShared<T> {
    inner: Option<Arc<SharedNode<T>>>,
}
impl<T> PaidShared<T> {
    /// Borrow the header's actual payer. Separately owned payload buffers must
    /// still be validated at their own boundary.
    pub fn resources(&self) -> &ResourcesRef {
        &self
            .inner
            .as_ref()
            .expect("live paid shared owner")
            .fee
            .resources
    }

    pub fn try_new(resources: &ResourcesRef, kind: ResourceKind, value: T) -> Result<Self, Error> {
        Self::try_new_with(resources, kind, || Ok(value))
    }
    /// Reserve the exact shared allocation before constructing its payload.
    /// Failed/unwinding factories retire their partial payload before this
    /// still-pending reservation. Admission follows the actual Arc allocation.
    pub fn try_new_with(
        resources: &ResourcesRef,
        kind: ResourceKind,
        make: impl FnOnce() -> Result<T, Error>,
    ) -> Result<Self, Error> {
        resources.checkpoint(1)?;
        let fee = resources.reserve(kind, shared_layout::<T>().size())?;
        let value = make().map_err(|error| resources.record(error))?;
        let mut owner = Self {
            inner: Some(Arc::new(SharedNode { value, fee })),
        };
        // Keep the complete wrapper live during admission: on callback failure
        // or unwind its Drop physically frees Arc before returning the fee.
        Arc::get_mut(owner.inner.as_mut().expect("new paid shared owner"))
            .expect("exclusive new paid shared owner")
            .fee
            .allocation_committed()?;
        Ok(owner)
    }
    pub fn as_ref(&self) -> &T {
        &self.inner.as_ref().expect("live paid shared owner").value
    }
    /// Check the header's original request without exposing its lease or Arc.
    /// The payload's separately owned allocations still require their own
    /// request checks at the consumer boundary.
    pub fn belongs_to(&self, resources: &ResourcesRef) -> bool {
        self.inner
            .as_ref()
            .expect("live paid shared owner")
            .fee
            .resources
            .is_same_request(resources)
    }
    /// Borrow the payload only while this is its unique strong owner. The
    /// mutable borrow prevents another handle from being cloned through self;
    /// no Arc, Weak or pointer escapes, and no allocation or callback occurs.
    pub fn get_mut_if_unique(&mut self) -> Option<&mut T> {
        Arc::get_mut(self.inner.as_mut().expect("live paid shared owner"))
            .map(|node| &mut node.value)
    }
    /// Recover an exclusively owned payload without cloning its storage.
    /// A failed unwrap returns the complete wrapper; it never drops an Err Arc.
    pub fn try_unwrap(mut self) -> Result<T, Self> {
        let arc = self.inner.take().expect("live paid shared owner");
        match Arc::try_unwrap(arc) {
            Ok(SharedNode { value, fee }) => {
                // No Weak escapes. The Arc allocation is already physically
                // freed; T moves to caller ownership with its own payload fees.
                drop(fee);
                Ok(value)
            }
            Err(arc) => {
                self.inner = Some(arc);
                Err(self)
            }
        }
    }
    pub fn ptr_eq(&self, other: &Self) -> bool {
        Arc::ptr_eq(
            self.inner.as_ref().expect("live paid shared owner"),
            other.inner.as_ref().expect("live paid shared owner"),
        )
    }
}
impl<T> Clone for PaidShared<T> {
    fn clone(&self) -> Self {
        // O(1), no new allocation. Real parsing/rendering loops charge work and
        // cancellation in batches; cloning a handle never calls a Native API.
        Self {
            inner: Some(self.inner.as_ref().expect("live paid shared owner").clone()),
        }
    }
}
impl<T> Deref for PaidShared<T> {
    type Target = T;
    fn deref(&self) -> &T {
        self.as_ref()
    }
}
impl<T: fmt::Debug> fmt::Debug for PaidShared<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.as_ref().fmt(f)
    }
}
impl<T> Drop for PaidShared<T> {
    fn drop(&mut self) {
        if let Some(arc) = self.inner.take() {
            // Every strong handle uses into_inner, including concurrent drops.
            // No Weak can escape. The unique Some therefore arrives only after
            // Arc's entire physical allocation has been deallocated.
            if let Some(SharedNode { value, fee }) = Arc::into_inner(arc) {
                drop(value);
                // This local also drops on unwind if the payload destructor
                // panics; Arc was physically freed before either local existed.
                drop(fee);
            }
        }
    }
}

/// Frozen encoded bytes. No mutable bytes, Arc or growable Vec is exposed.
pub type PaidEncoded = PaidShared<BudgetVec<u8>>;
impl BudgetVec<u8> {
    /// Grow and initialize in bounded chunks after reserving the full storage.
    /// On error the valid partial buffer remains paid and must not be published.
    pub fn try_resize_zeroed(&mut self, length: usize) -> Result<(), Error> {
        self.try_reserve_exact(length.saturating_sub(self.len()))?;
        if length <= self.len() {
            self.values.truncate(length);
            return Ok(());
        }
        while self.len() < length {
            let count = (length - self.len()).min(4096);
            self.resources.checkpoint(count as u64)?;
            self.values.resize(self.len() + count, 0);
        }
        Ok(())
    }
    pub fn into_shared(self) -> Result<PaidEncoded, Error> {
        let resources = self.resources.clone();
        let kind = self.kind;
        PaidShared::try_new(&resources, kind, self)
    }
}

/// Always-valid UTF-8, with paid storage and no mutable-byte escape.
pub struct PaidUtf8 {
    bytes: BudgetVec<u8>,
}
impl PaidUtf8 {
    /// Return the actual retained byte capacity without exposing its storage.
    pub fn capacity(&self) -> usize {
        self.bytes.capacity()
    }

    /// Reserve actual UTF-8 capacity before producing bytes. This preserves
    /// callers' explicit capacity choices without a temporary ordinary String.
    pub fn try_reserve_exact(&mut self, additional: usize) -> Result<(), Error> {
        self.bytes.try_reserve_exact(additional)
    }

    /// Borrow the original UTF-8 buffer owner without exposing mutable bytes.
    pub fn resources(&self) -> &ResourcesRef {
        self.bytes.resources()
    }

    pub fn new(resources: ResourcesRef, kind: ResourceKind) -> Self {
        Self {
            bytes: BudgetVec::new(resources, kind),
        }
    }
    pub fn try_from_str(
        resources: ResourcesRef,
        kind: ResourceKind,
        text: &str,
    ) -> Result<Self, Error> {
        let mut value = Self::new(resources, kind);
        value.bytes.try_reserve_exact(text.len())?;
        value.try_push_str(text)?;
        Ok(value)
    }
    pub fn try_push(&mut self, ch: char) -> Result<(), Error> {
        let mut buffer = [0; 4];
        self.try_push_str(ch.encode_utf8(&mut buffer))
    }
    pub fn try_push_str(&mut self, mut text: &str) -> Result<(), Error> {
        self.bytes.try_reserve(text.len())?;
        while !text.is_empty() {
            let mut end = text.len().min(64 * 1024);
            while !text.is_char_boundary(end) {
                end -= 1;
            }
            self.bytes.resources.checkpoint(end as u64)?;
            // The complete capacity was reserved above. No allocation occurs
            // here, and each appended prefix ends on a UTF-8 boundary.
            self.bytes.values.extend_from_slice(&text.as_bytes()[..end]);
            text = &text[end..];
        }
        Ok(())
    }
    pub fn as_str(&self) -> &str {
        // SAFETY: bytes is private; only valid str/char prefixes are appended,
        // including on cancellation. clear preserves validity. No mutable bytes
        // or unvalidated constructor is exposed. Reading is therefore O(1).
        unsafe { core::str::from_utf8_unchecked(self.bytes.as_slice()) }
    }
    pub fn len(&self) -> usize {
        self.bytes.len()
    }
    pub fn is_empty(&self) -> bool {
        self.bytes.is_empty()
    }
    pub fn clear(&mut self) {
        self.bytes.clear();
    }
    pub fn into_shared(self) -> Result<PaidShared<Self>, Error> {
        let resources = self.bytes.resources.clone();
        let kind = self.bytes.kind;
        PaidShared::try_new(&resources, kind, self)
    }
}
impl fmt::Debug for PaidUtf8 {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.as_str().fmt(f)
    }
}
impl Deref for PaidUtf8 {
    type Target = str;
    fn deref(&self) -> &str {
        self.as_str()
    }
}
