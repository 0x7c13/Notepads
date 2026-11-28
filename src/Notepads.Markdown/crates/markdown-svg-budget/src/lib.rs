// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE file in the project root for license information.
// ---------------------------------------------------------------------------------------------
//! Private request resources. Metadata is paid by the enclosing Native scope.
#![no_std]

extern crate alloc;

mod bucket_allocator;
pub mod codec;
mod control;
mod frozen;
mod paid;
pub mod slice;
mod storage;
pub use bucket_allocator::BucketAllocator;
pub use control::{NoControl, RequestControl, WorkControl};
pub use frozen::FrozenList;
pub use paid::{PaidEncoded, PaidShared, PaidUtf8};

use alloc::{boxed::Box, sync::Arc, vec::Vec};
use core::{
    alloc::Layout,
    fmt,
    ops::{Deref, DerefMut, RangeFull},
    sync::atomic::{AtomicU64, Ordering},
};

/// Fixed first-failure categories, also used by controlled SVG processing.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[repr(u8)]
pub enum FailureKind {
    Native = 1,
    Quota = 2,
    Allocation = 3,
    Cancelled = 4,
    Decode = 5,
    Unsupported = 6,
    Invariant = 7,
    ExternalResource = 16,
    UnsupportedImageData = 17,
    InvalidImageHeader = 18,
    ImageDecode = 19,
    MalformedImageData = 20,
    NestedImageDecode = 21,
    RasterAllocation = 22,
    InvalidFilterRegion = 23,
    FontReplay = 24,
}

/// The resource dimension which failed, without an allocated error message.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[repr(u16)]
pub enum LimitKind {
    None = 0,
    Bytes = 1,
    Scratch = 2,
    Children = 3,
    SvgDepth = 4,
    ChildEncoded = 5,
    XmlNodes = 6,
    XmlAttributes = 7,
    XmlElementAttributes = 8,
    XmlDepth = 9,
    Namespaces = 10,
    Work = 11,
    Capacity = 12,
    InputBytes = 13,
    ExpandedNodes = 14,
    ConvertedSegments = 15,
}

/// Copy-only typed failure; a Native HRESULT belongs only to its original failure.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Error {
    pub kind: FailureKind,
    pub limit: LimitKind,
    pub native_hr: Option<i32>,
}
impl Error {
    pub const fn new(kind: FailureKind, limit: LimitKind) -> Self {
        Self {
            kind,
            limit,
            native_hr: None,
        }
    }
    pub const fn native(hr: i32) -> Self {
        Self {
            kind: FailureKind::Native,
            limit: LimitKind::None,
            native_hr: Some(hr),
        }
    }
    fn packed(self) -> u64 {
        self.kind as u64
            | ((self.limit as u64) << 8)
            | self
                .native_hr
                .map_or(0, |hr| (1u64 << 24) | ((hr as u32 as u64) << 32))
    }
    fn unpack(value: u64) -> Option<Self> {
        let kind = match value as u8 {
            0 => return None,
            1 => FailureKind::Native,
            2 => FailureKind::Quota,
            3 => FailureKind::Allocation,
            4 => FailureKind::Cancelled,
            5 => FailureKind::Decode,
            6 => FailureKind::Unsupported,
            7 => FailureKind::Invariant,
            16 => FailureKind::ExternalResource,
            17 => FailureKind::UnsupportedImageData,
            18 => FailureKind::InvalidImageHeader,
            19 => FailureKind::ImageDecode,
            20 => FailureKind::MalformedImageData,
            21 => FailureKind::NestedImageDecode,
            22 => FailureKind::RasterAllocation,
            23 => FailureKind::InvalidFilterRegion,
            24 => FailureKind::FontReplay,
            _ => return Some(Self::new(FailureKind::Invariant, LimitKind::None)),
        };
        let limit = match ((value >> 8) & 0xffff) as u16 {
            0 => LimitKind::None,
            1 => LimitKind::Bytes,
            2 => LimitKind::Scratch,
            3 => LimitKind::Children,
            4 => LimitKind::SvgDepth,
            5 => LimitKind::ChildEncoded,
            6 => LimitKind::XmlNodes,
            7 => LimitKind::XmlAttributes,
            8 => LimitKind::XmlElementAttributes,
            9 => LimitKind::XmlDepth,
            10 => LimitKind::Namespaces,
            11 => LimitKind::Work,
            12 => LimitKind::Capacity,
            13 => LimitKind::InputBytes,
            14 => LimitKind::ExpandedNodes,
            15 => LimitKind::ConvertedSegments,
            _ => return Some(Self::new(FailureKind::Invariant, LimitKind::None)),
        };
        Some(Self {
            kind,
            limit,
            native_hr: (value & (1 << 24) != 0).then_some((value >> 32) as u32 as i32),
        })
    }
}
impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "resource failure {:?}/{:?}", self.kind, self.limit)
    }
}

/// One atomic publication transports the entire immutable first record.
#[derive(Default)]
pub struct FirstFailureSlot {
    packed: AtomicU64,
}
impl FirstFailureSlot {
    pub const fn new() -> Self {
        Self {
            packed: AtomicU64::new(0),
        }
    }
    pub fn record(&self, error: Error) -> Error {
        match self
            .packed
            .compare_exchange(0, error.packed(), Ordering::AcqRel, Ordering::Acquire)
        {
            Ok(_) => error,
            Err(first) => {
                Error::unpack(first).unwrap_or(Error::new(FailureKind::Invariant, LimitKind::None))
            }
        }
    }
    pub fn get(&self) -> Option<Error> {
        Error::unpack(self.packed.load(Ordering::Acquire))
    }
}
impl fmt::Debug for FirstFailureSlot {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.get().fmt(f)
    }
}

/// Shared fault capability, with no Weak or raw Arc escape.
#[derive(Clone, Debug)]
pub struct FirstFailureRef(Arc<FirstFailureSlot>);
impl FirstFailureRef {
    /// Internal bootstrap contract.
    /// The outer Native bootstrap envelope is already paid and remains paid
    /// until every strong reference and its physical Arc allocation are gone.
    pub fn new_prepaid() -> Self {
        Self(Arc::new(FirstFailureSlot::new()))
    }
    pub fn record(&self, error: Error) -> Error {
        self.0.record(error)
    }
    pub fn get(&self) -> Option<Error> {
        self.0.get()
    }
    pub fn validate_unique(&self) -> Result<(), Error> {
        self.validate_owners(1)
    }
    /// Exact canonical owners at a proved retirement boundary; no Weak escape.
    /// Embedded jobs have one Context and one enclosing resources owner.
    /// Non-mutating preflight; a rejected close must not poison a live job.
    pub fn has_exact_owners(&self, expected: usize) -> bool {
        expected != 0 && Arc::strong_count(&self.0) == expected && Arc::weak_count(&self.0) == 0
    }
    pub fn validate_owners(&self, expected: usize) -> Result<(), Error> {
        if self.has_exact_owners(expected) {
            Ok(())
        } else {
            Err(self.record(Error::new(FailureKind::Invariant, LimitKind::None)))
        }
    }
}

/// Existing private Native resource categories.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u32)]
pub enum ResourceKind {
    Font = 1,
    Metrics = 2,
    Geometry = 3,
    Pixels = 5,
    Scratch = 6,
}

/// Unique capability returned by a backend. It cannot be copied or cloned.
#[repr(C)]
#[derive(Debug)]
pub struct Ticket {
    account_nonce: u64,
    slot: u32,
    generation: u32,
}
const _: () = {
    assert!(size_of::<Ticket>() == 16);
    assert!(core::mem::offset_of!(Ticket, account_nonce) == 0);
    assert!(core::mem::offset_of!(Ticket, slot) == 8);
    assert!(core::mem::offset_of!(Ticket, generation) == 12);
};
impl Ticket {
    pub fn new(account_nonce: u64, slot: u32, generation: u32) -> Result<Self, Error> {
        if account_nonce == 0 || generation == 0 {
            Err(Error::new(FailureKind::Invariant, LimitKind::None))
        } else {
            Ok(Self {
                account_nonce,
                slot,
                generation,
            })
        }
    }
    pub fn account_nonce(&self) -> u64 {
        self.account_nonce
    }
    pub fn slot(&self) -> u32 {
        self.slot
    }
    pub fn generation(&self) -> u32 {
        self.generation
    }
}
/// Callbacks must not retain resources, buffers, trees or adapters.
pub trait Backend: Send + Sync + 'static {
    fn reserve(&self, kind: ResourceKind, bytes: usize) -> Result<Ticket, Error>;
    /// Called only after the exact reserved allocation has actually succeeded.
    fn allocation_committed(&self, ticket: &Ticket, bytes: usize) -> Result<(), Error>;
    /// Pending reservations may roll back without allocation. Admitted storage
    /// must be physically freed before this callback.
    fn release_after_free(&self, ticket: Ticket) -> Result<(), Error>;
    fn checkpoint(&self, work: u64) -> Result<(), Error>;
}

#[derive(Clone, Copy, Debug)]
pub struct Limits {
    pub scratch_bytes: u64,
    pub child_encoded_bytes: u64,
    pub children: u64,
    pub svg_depth: u64,
    pub xml_nodes: u64,
    pub xml_attributes: u64,
    pub xml_element_attributes: u64,
    pub xml_depth: u64,
    pub work: u64,
    pub input_bytes: u64,
    pub expanded_nodes: u32,
    pub converted_segments: u32,
}
impl Default for Limits {
    fn default() -> Self {
        Self {
            scratch_bytes: 64 * 1024 * 1024,
            child_encoded_bytes: 8 * 1024 * 1024,
            children: 64,
            svg_depth: 8,
            xml_nodes: 65_536,
            xml_attributes: 262_144,
            xml_element_attributes: 256,
            xml_depth: 128,
            work: 1 << 28,
            input_bytes: 8 * 1024 * 1024,
            expanded_nodes: 131_072,
            converted_segments: 1_048_576,
        }
    }
}
struct ResourceInner {
    backend: Box<dyn Backend>,
    fault: FirstFailureRef,
    limits: Limits,
    scratch: AtomicU64,
    leases: AtomicU64,
    release_failures: AtomicU64,
    children: AtomicU64,
    depth: AtomicU64,
    child_encoded: AtomicU64,
    work: AtomicU64,
    scan_credit: AtomicU64,
    // One atomic pair prevents a failed two-dimensional scene admission from
    // consuming either counter. Both fixed structural limits fit in u32.
    scene: AtomicU64,
}
#[derive(Clone)]
pub struct ResourcesRef(Arc<ResourceInner>);
impl fmt::Debug for ResourcesRef {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ResourcesRef")
            .field("statistics", &self.statistics())
            .finish()
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Statistics {
    pub scratch_bytes: u64,
    pub live_leases: u64,
    pub release_failures: u64,
    pub children: u64,
    pub depth: u64,
    pub child_encoded_bytes: u64,
    pub work: u64,
    pub expanded_nodes: u64,
    pub converted_segments: u64,
}
impl ResourcesRef {
    /// Internal bootstrap contract.
    /// One externally paid bootstrap envelope covers the resource Arc, backend
    /// Box and fault Arc until their physical allocations have been freed.
    pub fn new_prepaid<B: Backend>(backend: B, fault: FirstFailureRef, limits: Limits) -> Self {
        Self(Arc::new(ResourceInner {
            backend: Box::new(backend),
            fault,
            limits,
            scratch: AtomicU64::new(0),
            leases: AtomicU64::new(0),
            release_failures: AtomicU64::new(0),
            children: AtomicU64::new(0),
            depth: AtomicU64::new(0),
            child_encoded: AtomicU64::new(0),
            work: AtomicU64::new(0),
            scan_credit: AtomicU64::new(0),
            scene: AtomicU64::new(0),
        }))
    }
    pub fn fault_slot(&self) -> &FirstFailureRef {
        &self.0.fault
    }
    pub fn failure(&self) -> Option<Error> {
        self.0.fault.get()
    }
    pub fn record(&self, error: Error) -> Error {
        self.0.fault.record(error)
    }
    pub fn limits(&self) -> Limits {
        self.0.limits
    }
    /// True only for the same prepaid request owner, without an allocation.
    pub fn is_same_request(&self, other: &Self) -> bool {
        Arc::ptr_eq(&self.0, &other.0)
    }
    /// Admit newly materialized scene nodes and normalized path verbs together.
    /// Successful totals are cumulative and are never refunded when a scene is
    /// moved, discarded or retired. This does not account its allocation bytes.
    pub fn admit_scene(&self, nodes: u64, segments: u64) -> Result<(), Error> {
        self.checkpoint(0)?;
        let mut previous = self.0.scene.load(Ordering::Acquire);
        let mut retries = 0u16;
        loop {
            if let Some(error) = self.failure() {
                return Err(error);
            }
            let old_nodes = previous >> 32;
            let old_segments = previous & u64::from(u32::MAX);
            let Some(next_nodes) = old_nodes
                .checked_add(nodes)
                .filter(|value| *value <= u64::from(self.0.limits.expanded_nodes))
            else {
                return Err(self.record(Error::new(FailureKind::Quota, LimitKind::ExpandedNodes)));
            };
            let Some(next_segments) = old_segments
                .checked_add(segments)
                .filter(|value| *value <= u64::from(self.0.limits.converted_segments))
            else {
                return Err(
                    self.record(Error::new(FailureKind::Quota, LimitKind::ConvertedSegments))
                );
            };
            let next = (next_nodes << 32) | next_segments;
            match self.0.scene.compare_exchange_weak(
                previous,
                next,
                Ordering::AcqRel,
                Ordering::Acquire,
            ) {
                Ok(_) => return Ok(()),
                Err(observed) => previous = observed,
            }
            retries += 1;
            if retries == 256 {
                // No counter state or lock is held across the backend call.
                self.checkpoint(0)?;
                retries = 0;
            }
        }
    }
    /// Admit one encoded child once. These request totals are never refunded.
    pub fn admit_child_encoded(&self, bytes: usize) -> Result<ChildAdmission, Error> {
        if let Some(error) = self.failure() {
            return Err(error);
        }
        self.add(
            &self.0.children,
            1,
            self.0.limits.children,
            LimitKind::Children,
        )?;
        self.add(
            &self.0.child_encoded,
            bytes as u64,
            self.0.limits.child_encoded_bytes,
            LimitKind::ChildEncoded,
        )?;
        Ok(ChildAdmission {
            resources: self.clone(),
            bytes,
        })
    }
    /// Enter SVG recursion for an already admitted child. Raster children do
    /// not enter this scope. The legacy combined entry retains its semantics.
    pub fn enter_svg_depth(&self) -> Result<NestedScope, Error> {
        if let Some(error) = self.failure() {
            return Err(error);
        }
        self.add(
            &self.0.depth,
            1,
            self.0.limits.svg_depth,
            LimitKind::SvgDepth,
        )?;
        Ok(NestedScope {
            resources: self.clone(),
        })
    }
    pub fn check_limit(&self, kind: LimitKind, value: u64, maximum: u64) -> Result<(), Error> {
        if let Some(error) = self.failure() {
            return Err(error);
        }
        if value > maximum {
            Err(self.record(Error::new(FailureKind::Quota, kind)))
        } else {
            Ok(())
        }
    }
    fn add(
        &self,
        counter: &AtomicU64,
        value: u64,
        maximum: u64,
        kind: LimitKind,
    ) -> Result<(), Error> {
        counter
            .try_update(Ordering::AcqRel, Ordering::Acquire, |old| {
                old.checked_add(value).filter(|n| *n <= maximum)
            })
            .map(|_| ())
            .map_err(|_| self.record(Error::new(FailureKind::Quota, kind)))
    }
    pub fn reserve(&self, kind: ResourceKind, bytes: usize) -> Result<Lease, Error> {
        if let Some(error) = self.failure() {
            return Err(error);
        }
        if bytes == 0 {
            return Ok(Lease {
                resources: self.clone(),
                ticket: None,
                kind,
                bytes: 0,
                admitted: false,
            });
        }
        if kind == ResourceKind::Scratch {
            self.add(
                &self.0.scratch,
                bytes as u64,
                self.0.limits.scratch_bytes,
                LimitKind::Scratch,
            )?;
        }
        // No counter lock or CAS operation is held while invoking a backend.
        let ticket = match self.0.backend.reserve(kind, bytes) {
            Ok(ticket) => ticket,
            Err(error) => {
                if kind == ResourceKind::Scratch {
                    self.0.scratch.fetch_sub(bytes as u64, Ordering::AcqRel);
                }
                return Err(self.record(error));
            }
        };
        self.0.leases.fetch_add(1, Ordering::AcqRel);
        Ok(Lease {
            resources: self.clone(),
            ticket: Some(ticket),
            kind,
            bytes,
            admitted: false,
        })
    }
    /// Confirm an actual allocation owned by this request. Failure leaves its
    /// reservation live until the caller frees the storage and drops the lease.
    pub fn allocation_committed(&self, lease: &mut Lease) -> Result<(), Error> {
        if !self.is_same_request(&lease.resources) {
            return Err(self.record(Error::new(FailureKind::Invariant, LimitKind::None)));
        }
        lease.allocation_committed()
    }
    pub fn checkpoint(&self, work: u64) -> Result<(), Error> {
        if let Some(error) = self.failure() {
            return Err(error);
        }
        self.add(&self.0.work, work, self.0.limits.work, LimitKind::Work)?;
        self.0.backend.checkpoint(work).map_err(|e| self.record(e))
    }
    /// Prepaid request-wide scanning credit. All child streams share the same
    /// remainder; no per-token rounding, extra allocation or second work ledger.
    pub fn scan_allowance(&self) -> Result<usize, Error> {
        if let Some(error) = self.failure() {
            return Err(error);
        }
        if self.0.scan_credit.load(Ordering::Acquire) == 0 {
            let remaining = self
                .0
                .limits
                .work
                .saturating_sub(self.0.work.load(Ordering::Acquire));
            // Exhaustion still goes through checkpoint so it records the same
            // Work failure; a final partial refill may consume the remainder.
            let grant = remaining.clamp(1, 65_536);
            self.checkpoint(grant)?;
            self.0.scan_credit.fetch_add(grant, Ordering::AcqRel);
        }
        Ok(self.0.scan_credit.load(Ordering::Acquire).min(65_536) as usize)
    }
    /// Charge work before consuming a byte/character scan. A bounded search may
    /// first obtain scan_allowance, then consume only bytes actually examined.
    pub fn consume_scan_work(&self, mut bytes: usize) -> Result<(), Error> {
        while bytes != 0 {
            let available = self.scan_allowance()?;
            if available == 0 {
                continue;
            }
            let used = bytes.min(available) as u64;
            if self
                .0
                .scan_credit
                .try_update(Ordering::AcqRel, Ordering::Acquire, |old| {
                    old.checked_sub(used)
                })
                .is_ok()
            {
                bytes -= used as usize;
            }
        }
        Ok(())
    }
    pub fn enter_child(&self) -> Result<NestedScope, Error> {
        if let Some(error) = self.failure() {
            return Err(error);
        }
        self.add(
            &self.0.children,
            1,
            self.0.limits.children,
            LimitKind::Children,
        )?;
        self.add(
            &self.0.depth,
            1,
            self.0.limits.svg_depth,
            LimitKind::SvgDepth,
        )?;
        Ok(NestedScope {
            resources: self.clone(),
        })
    }
    pub fn add_child_encoded(&self, bytes: usize) -> Result<(), Error> {
        if let Some(error) = self.failure() {
            return Err(error);
        }
        self.add(
            &self.0.child_encoded,
            bytes as u64,
            self.0.limits.child_encoded_bytes,
            LimitKind::ChildEncoded,
        )
    }
    pub fn statistics(&self) -> Statistics {
        let scene = self.0.scene.load(Ordering::Acquire);
        Statistics {
            scratch_bytes: self.0.scratch.load(Ordering::Acquire),
            live_leases: self.0.leases.load(Ordering::Acquire),
            release_failures: self.0.release_failures.load(Ordering::Acquire),
            children: self.0.children.load(Ordering::Acquire),
            depth: self.0.depth.load(Ordering::Acquire),
            child_encoded_bytes: self.0.child_encoded.load(Ordering::Acquire),
            work: self.0.work.load(Ordering::Acquire),
            expanded_nodes: scene >> 32,
            converted_segments: scene & u64::from(u32::MAX),
        }
    }
    pub fn validate_retired(&self) -> Result<(), Error> {
        let s = self.statistics();
        if Arc::strong_count(&self.0) == 1
            && Arc::weak_count(&self.0) == 0
            && s.live_leases == 0
            && s.scratch_bytes == 0
            && s.depth == 0
            && s.release_failures == 0
        {
            Ok(())
        } else {
            Err(self.record(Error::new(FailureKind::Invariant, LimitKind::None)))
        }
    }
}

/// Non-cloneable proof that one exact child was admitted in this request.
/// This owns no bytes or allocation fee and never refunds cumulative totals.
#[derive(Debug)]
pub struct ChildAdmission {
    resources: ResourcesRef,
    bytes: usize,
}
impl ChildAdmission {
    pub fn matches(&self, resources: &ResourcesRef, bytes: usize) -> bool {
        self.bytes == bytes && self.resources.is_same_request(resources)
    }
}

/// Stack lease. The payload must physically free before this value is dropped.
pub struct Lease {
    resources: ResourcesRef,
    ticket: Option<Ticket>,
    kind: ResourceKind,
    bytes: usize,
    admitted: bool,
}
impl Lease {
    /// Call exactly once after successful allocation/capacity validation, while
    /// its storage and this reservation are both live. No callback is issued
    /// for zero bytes. Never call this to admit an allowance or an estimate.
    pub fn allocation_committed(&mut self) -> Result<(), Error> {
        if self.admitted {
            return Err(self
                .resources
                .record(Error::new(FailureKind::Invariant, LimitKind::None)));
        }
        if let Some(error) = self.resources.failure() {
            return Err(error);
        }
        if let Some(ticket) = self.ticket.as_ref() {
            self.resources
                .0
                .backend
                .allocation_committed(ticket, self.bytes)
                .map_err(|error| self.resources.record(error))?;
        }
        self.admitted = true;
        Ok(())
    }
}
impl fmt::Debug for Lease {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Lease")
            .field("kind", &self.kind)
            .field("bytes", &self.bytes)
            .finish()
    }
}
impl Drop for Lease {
    fn drop(&mut self) {
        if let Some(ticket) = self.ticket.take() {
            if let Err(error) = self.resources.0.backend.release_after_free(ticket) {
                self.resources
                    .0
                    .release_failures
                    .fetch_add(1, Ordering::AcqRel);
                self.resources.record(error);
                // Failed release is not a successful refund, even diagnostically.
                return;
            }
            if self.kind == ResourceKind::Scratch {
                self.resources
                    .0
                    .scratch
                    .fetch_sub(self.bytes as u64, Ordering::AcqRel);
            }
            self.resources.0.leases.fetch_sub(1, Ordering::AcqRel);
        }
    }
}
pub struct NestedScope {
    resources: ResourcesRef,
}
impl Drop for NestedScope {
    fn drop(&mut self) {
        self.resources.0.depth.fetch_sub(1, Ordering::AcqRel);
    }
}

/// Exact requested bootstrap Layout sizes for the fixed toolchain. Allocator
/// bookkeeping/RSS are separate. Validate again on a toolchain change.
#[derive(Clone, Copy, Debug)]
pub struct MetadataSizes {
    pub resource_arc_bytes: usize,
    pub backend_box_bytes: usize,
    pub failure_arc_bytes: usize,
    pub allocation_count: usize,
}
impl MetadataSizes {
    pub fn total_bytes(self) -> usize {
        self.resource_arc_bytes + self.backend_box_bytes + self.failure_arc_bytes
    }
}
pub fn metadata_sizes<B: Backend>() -> MetadataSizes {
    MetadataSizes {
        resource_arc_bytes: paid::arc_layout::<ResourceInner>().size(),
        backend_box_bytes: size_of::<B>(),
        failure_arc_bytes: paid::arc_layout::<FirstFailureSlot>().size(),
        allocation_count: 2 + usize::from(size_of::<B>() != 0),
    }
}

/// An owned paid vector. No API exposes a growable Vec or unpaired storage.
pub struct BudgetVec<T> {
    values: Vec<T>,
    lease: Option<Lease>,
    resources: ResourcesRef,
    kind: ResourceKind,
}
impl<T> BudgetVec<T> {
    /// Borrow the actual request that owns this vector, including paid-empty
    /// storage. No allocation, lease or mutable storage escapes.
    pub fn resources(&self) -> &ResourcesRef {
        &self.resources
    }

    /// Check the actual allocation owner without exposing storage or a lease.
    pub fn belongs_to(&self, resources: &ResourcesRef) -> bool {
        self.resources.is_same_request(resources)
    }
    pub fn new(resources: ResourcesRef, kind: ResourceKind) -> Self {
        Self {
            values: Vec::new(),
            lease: None,
            resources,
            kind,
        }
    }
    pub fn len(&self) -> usize {
        self.values.len()
    }
    pub fn capacity(&self) -> usize {
        self.values.capacity()
    }
    pub fn is_empty(&self) -> bool {
        self.values.is_empty()
    }
    pub fn as_slice(&self) -> &[T] {
        &self.values
    }
    pub fn as_mut_slice(&mut self) -> &mut [T] {
        &mut self.values
    }
    pub fn try_reserve(&mut self, additional: usize) -> Result<(), Error> {
        if let Some(error) = self.resources.failure() {
            return Err(error);
        }
        let required = self.values.len().checked_add(additional).ok_or_else(|| {
            self.resources
                .record(Error::new(FailureKind::Quota, LimitKind::Capacity))
        })?;
        if required <= self.capacity() {
            return Ok(());
        }
        let doubled = self.capacity().checked_mul(2).unwrap_or(required);
        self.grow(required.max(doubled).max(4))
    }
    pub fn try_reserve_exact(&mut self, additional: usize) -> Result<(), Error> {
        if let Some(error) = self.resources.failure() {
            return Err(error);
        }
        let required = self.values.len().checked_add(additional).ok_or_else(|| {
            self.resources
                .record(Error::new(FailureKind::Quota, LimitKind::Capacity))
        })?;
        if required <= self.capacity() {
            return Ok(());
        }
        self.grow(required)
    }
    fn grow(&mut self, capacity: usize) -> Result<(), Error> {
        if capacity < self.values.len() {
            return Err(self
                .resources
                .record(Error::new(FailureKind::Invariant, LimitKind::Capacity)));
        }
        let layout = Layout::array::<T>(capacity).map_err(|_| {
            self.resources
                .record(Error::new(FailureKind::Quota, LimitKind::Capacity))
        })?;
        let mut lease = self.resources.reserve(self.kind, layout.size())?;
        // Declaration order guarantees the fresh Vec frees before its new lease
        // on allocation/capacity validation failure or unwinding.
        #[cfg(test)]
        if FAIL_NEXT_GROWTH.swap(false, Ordering::AcqRel) {
            return Err(self
                .resources
                .record(Error::new(FailureKind::Allocation, LimitKind::Capacity)));
        }
        let mut values: Vec<T> = Vec::new();
        values.try_reserve_exact(capacity).map_err(|_| {
            self.resources
                .record(Error::new(FailureKind::Allocation, LimitKind::Capacity))
        })?;
        if size_of::<T>() != 0 && values.capacity() != capacity {
            return Err(self
                .resources
                .record(Error::new(FailureKind::Invariant, LimitKind::Capacity)));
        }
        lease.allocation_committed()?;
        let old_length = self.values.len();
        let element_bytes = size_of::<T>();
        if element_bytes != 0 {
            let batch = (65_536 / element_bytes).max(1);
            let mut copied = 0;
            while copied < old_length {
                let count = batch.min(old_length - copied);
                let bytes = count.checked_mul(element_bytes).ok_or_else(|| {
                    self.resources
                        .record(Error::new(FailureKind::Quota, LimitKind::Capacity))
                })?;
                self.resources.consume_scan_work(bytes)?;
                // SAFETY: old_length initialized T values still belong solely
                // to the old Vec. The distinct new allocation has enough
                // correctly aligned capacity, but length remains zero until
                // every batch succeeds. These raw copies are never exposed or
                // dropped as T on error, so even non-Copy owners remain owned
                // exactly once by the unchanged old Vec during cancellation.
                unsafe {
                    core::ptr::copy_nonoverlapping(
                        self.values.as_ptr().add(copied),
                        values.as_mut_ptr().add(copied),
                        count,
                    );
                }
                copied += count;
            }
        }
        if let Some(error) = self.resources.failure() {
            return Err(error);
        }
        // SAFETY: all initialized values have been copied (ZSTs need no bytes).
        // Transfer ownership without a fallible call between the two lengths.
        // Old values are no longer dropped; new values now each have one owner.
        unsafe {
            self.values.set_len(0);
            values.set_len(old_length);
        }
        let old_values = core::mem::replace(&mut self.values, values);
        // Pair the new block and fee before any callback can unwind. The old
        // allocation is freed before returning its fee, including on panic.
        let old_lease = self.lease.replace(lease);
        drop(old_values);
        drop(old_lease);
        if let Some(error) = self.resources.failure() {
            return Err(error);
        }
        Ok(())
    }
    pub fn try_push(&mut self, value: T) -> Result<(), Error> {
        self.try_reserve(1)?;
        self.values.push(value);
        Ok(())
    }
    pub fn try_insert(&mut self, index: usize, value: T) -> Result<(), Error> {
        if index > self.len() {
            return Err(self
                .resources
                .record(Error::new(FailureKind::Invariant, LimitKind::Capacity)));
        }
        self.try_reserve(1)?;
        self.values.insert(index, value);
        Ok(())
    }
    pub fn try_extend_from_slice(&mut self, values: &[T]) -> Result<(), Error>
    where
        T: Clone,
    {
        self.try_reserve(values.len())?;
        self.values.extend_from_slice(values);
        Ok(())
    }
    /// Commit an already paid Copy suffix without allocating, calling Clone,
    /// consulting the first-failure slot or entering the resource backend.
    /// The caller must reserve capacity and pay bounded work before entering
    /// its no-failure command commit, then check its request before publishing.
    pub fn extend_reserved_copy(&mut self, values: &[T])
    where
        T: Copy,
    {
        assert!(values.len() <= self.values.capacity() - self.values.len());
        for &value in values {
            self.values.push(value);
        }
    }
    pub fn clear(&mut self) {
        self.values.clear();
    }
    /// Free the complete allocation before returning its lease.
    pub fn release_storage(&mut self) {
        drop(core::mem::take(&mut self.values));
        drop(self.lease.take());
    }
    pub fn pop(&mut self) -> Option<T> {
        self.values.pop()
    }
    /// Borrow a forward-draining tail while retaining this vector's capacity
    /// and fee. The internal caller validates start, policy and work before
    /// consuming it; error cleanup must still drop every remaining value.
    pub fn drain_tail(&mut self, start: usize) -> alloc::vec::Drain<'_, T> {
        self.values.drain(start..)
    }
    pub fn drain(&mut self, range: RangeFull) -> alloc::vec::Drain<'_, T> {
        self.values.drain(range)
    }
}
impl<T> Deref for BudgetVec<T> {
    type Target = [T];
    fn deref(&self) -> &[T] {
        &self.values
    }
}
impl<T> DerefMut for BudgetVec<T> {
    fn deref_mut(&mut self) -> &mut [T] {
        &mut self.values
    }
}
impl<T> Drop for BudgetVec<T> {
    fn drop(&mut self) {
        drop(core::mem::take(&mut self.values));
        drop(self.lease.take());
    }
}
impl<T: fmt::Debug> fmt::Debug for BudgetVec<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.values.fmt(f)
    }
}

#[cfg(test)]
static FAIL_NEXT_GROWTH: core::sync::atomic::AtomicBool =
    core::sync::atomic::AtomicBool::new(false);
#[cfg(test)]
extern crate std;
#[cfg(test)]
mod tests;
