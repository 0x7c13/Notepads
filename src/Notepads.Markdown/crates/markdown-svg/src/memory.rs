// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE file in the project root for license information.
// ---------------------------------------------------------------------------------------------
//! Final owned font-service buffers. Reservation precedes allocation, admission
//! follows actual success, and actual free precedes capability release.
use crate::abi::{self, Callbacks, Failure, Result};
use core::{alloc::Layout, ptr::NonNull};
use markdown_svg_budget::{
    Error as ResourceError, FailureKind, FirstFailureRef, LimitKind, ResourceKind, Ticket,
};
use std::sync::{
    Arc, Weak,
    atomic::{AtomicBool, AtomicU64, Ordering},
};

/// Only this coarse job's callbacks and diagnostics. This owner deliberately has
/// no Adapter/fontdb reference, so FontBytes -> Lease cannot create an owner cycle.
#[derive(Clone)]
pub struct Context {
    callbacks: Callbacks,
    diagnostics: Arc<Diagnostics>,
    first_failure: FirstFailureRef,
    host: Arc<dyn Host>,
    cancel: Arc<AtomicBool>,
    expected_scope_owners: usize,
}
#[derive(Default)]
struct Diagnostics {
    release_failures: AtomicU64,
    created: [AtomicU64; 3],
    retired: [AtomicU64; 3],
}
#[derive(Clone, Copy)]
pub enum OwnerKind {
    Font = 0,
    Metrics = 1,
    Geometry = 2,
}
/// Declare this field AFTER the actual storage and its leases. Rust field-drop
/// order then records retirement only after physical storage and admission retire.
pub struct OwnerWitness {
    context: Context,
    kind: OwnerKind,
}
impl OwnerWitness {
    pub fn new(context: &Context, kind: OwnerKind) -> Self {
        context.diagnostics.created[kind as usize].fetch_add(1, Ordering::Relaxed);
        Self {
            context: context.clone(),
            kind,
        }
    }
}
impl Drop for OwnerWitness {
    fn drop(&mut self) {
        self.context.diagnostics.retired[self.kind as usize].fetch_add(1, Ordering::Relaxed);
    }
}
impl Context {
    pub fn new(callbacks: Callbacks, host: Arc<dyn Host>, cancel: Arc<AtomicBool>) -> Result<Self> {
        callbacks.validate()?;
        let mut context =
            Self::new_with_first_failure(callbacks, host, cancel, FirstFailureRef::new_prepaid())?;
        context.expected_scope_owners = 1;
        Ok(context)
    }
    /// The canonical resource owner prepaid this same slot before Native work.
    /// This constructor does not create a second error owner or replace a fault.
    pub fn new_with_first_failure(
        callbacks: Callbacks,
        host: Arc<dyn Host>,
        cancel: Arc<AtomicBool>,
        first_failure: FirstFailureRef,
    ) -> Result<Self> {
        Ok(Self {
            callbacks: callbacks.validate()?,
            diagnostics: Arc::new(Diagnostics::default()),
            first_failure,
            host,
            cancel,
            expected_scope_owners: 2,
        })
    }
    pub fn release_failures(&self) -> u64 {
        self.diagnostics.release_failures.load(Ordering::Relaxed)
    }
    pub fn first_failure_ref(&self) -> FirstFailureRef {
        self.first_failure.clone()
    }
    pub fn validate_scope_retired(&self) -> Result<()> {
        // Do not clone before this check: Context is the final lexical owner.
        self.first_failure
            .validate_owners(self.expected_scope_owners)
            .map_err(failure_from_resource)
    }
    pub fn resource_failure(&self) -> Option<ResourceError> {
        self.first_failure.get()
    }
    pub fn failure(&self) -> Option<Failure> {
        self.resource_failure().map(failure_from_resource)
    }
    pub fn record_resource(&self, error: ResourceError) -> ResourceError {
        self.first_failure.record(error)
    }
    pub fn record_failure(&self, failure: Failure) -> Failure {
        failure_from_resource(self.record_resource(resource_from_failure(failure)))
    }
    pub fn native_success(&self, hr: i32) -> Result<()> {
        if hr == 0 {
            Ok(())
        } else {
            Err(self.record_failure(Failure::Native(hr)))
        }
    }
    pub(crate) fn allocation_committed(
        &self,
        ticket: &Ticket,
        bytes: usize,
    ) -> core::result::Result<(), ResourceError> {
        self.host
            .allocation_committed(ticket, bytes)
            .map_err(|e| self.record_resource(e))
    }
    pub fn checkpoint(&self, work: u64) -> Result<()> {
        if self.cancel.load(Ordering::Acquire) {
            return Err(failure_from_resource(self.record_resource(
                ResourceError::new(FailureKind::Cancelled, LimitKind::None),
            )));
        }
        self.host
            .checkpoint(work)
            .map_err(|error| failure_from_resource(self.record_resource(error)))
    }
    pub(crate) fn coverage_provider(&self) -> Option<&dyn resvg::CoverageProvider> {
        self.host.coverage_provider()
    }
    pub fn make_resources(
        &self,
        limits: markdown_svg_budget::Limits,
    ) -> markdown_svg_budget::ResourcesRef {
        markdown_svg_budget::ResourcesRef::new_prepaid(
            crate::resource_backend::NativeBudgetBackend::new(self),
            self.first_failure_ref(),
            limits,
        )
    }
    pub fn owner_counts(&self) -> ([u64; 3], [u64; 3]) {
        (
            core::array::from_fn(|i| self.diagnostics.created[i].load(Ordering::Relaxed)),
            core::array::from_fn(|i| self.diagnostics.retired[i].load(Ordering::Relaxed)),
        )
    }
    /// Called after every owner in this job has physically dropped, including on
    /// an error/unwind path. Another job cannot change this diagnostic owner.
    pub fn validate_released(&self) -> Result<()> {
        let (created, retired) = self.owner_counts();
        if self.release_failures() == 0 && created == retired {
            Ok(())
        } else {
            self.record_failure(Failure::Contract);
            Err(Failure::Contract)
        }
    }
}
impl core::ops::Deref for Context {
    type Target = Callbacks;
    fn deref(&self) -> &Self::Target {
        &self.callbacks
    }
}

/// Compatibility projection only. The typed record remains the authority.
pub(crate) fn failure_from_resource(error: ResourceError) -> Failure {
    match error.kind {
        FailureKind::Native => error
            .native_hr
            .map(Failure::Native)
            .unwrap_or(Failure::Contract),
        FailureKind::Allocation | FailureKind::RasterAllocation => Failure::Allocation,
        FailureKind::Quota => Failure::Native(0x8007000eu32 as i32),
        FailureKind::Cancelled => Failure::Native(0x800704c7u32 as i32),
        FailureKind::Unsupported => Failure::Native(0x80070032u32 as i32),
        FailureKind::Invariant => Failure::Contract,
        FailureKind::Decode
        | FailureKind::ExternalResource
        | FailureKind::UnsupportedImageData
        | FailureKind::InvalidImageHeader
        | FailureKind::ImageDecode
        | FailureKind::MalformedImageData
        | FailureKind::NestedImageDecode
        | FailureKind::InvalidFilterRegion
        | FailureKind::FontReplay => Failure::Native(0x80070057u32 as i32),
    }
}
fn resource_from_failure(failure: Failure) -> ResourceError {
    match failure {
        Failure::Native(hr) => ResourceError::native(hr),
        Failure::Contract => ResourceError::new(FailureKind::Invariant, LimitKind::None),
        Failure::Allocation => ResourceError::new(FailureKind::Allocation, LimitKind::None),
    }
}

/// The embedding owner must account the exact bootstrap allocations before
/// calling Context::new; it retains that payment through the last Arc free.
/// No legacy probe allowance is accepted by this crate.
pub(crate) fn diagnostics_layout() -> Layout {
    arc_layout::<Diagnostics>()
}
pub(crate) fn arc_layout<T>() -> Layout {
    Layout::new::<(usize, usize)>()
        .extend(Layout::new::<T>())
        .unwrap()
        .0
        .pad_to_align()
}

/// Local work credit, prepaid in bounded coarse quanta. It is deliberately
/// not a replacement for Context::checkpoint on any pre-existing pipeline.
/// Failed/error paths cannot leave performed work unpaid, and unused credit is
/// not refunded or transferred to another source/job.
#[derive(Default)]
pub struct WorkCredit {
    credit: u64,
}
impl WorkCredit {
    pub fn charge(&mut self, context: &Context, mut work: u64) -> Result<()> {
        while work != 0 {
            if self.credit == 0 {
                context.checkpoint(4096)?;
                self.credit = 4096;
            }
            let consumed = work.min(self.credit);
            self.credit -= consumed;
            work -= consumed;
        }
        Ok(())
    }
}
pub struct WorkBatch<'a> {
    context: &'a Context,
    credit: WorkCredit,
}
impl<'a> WorkBatch<'a> {
    pub fn new(context: &'a Context) -> Self {
        Self {
            context,
            credit: WorkCredit::default(),
        }
    }
    pub fn charge(&mut self, work: u64) -> Result<()> {
        self.credit.charge(self.context, work)
    }
}

/// One account capability; no Native callback or exported FFI is involved.
/// Implementations enforce the single worker and do not retain engine owners.
pub trait Host: Send + Sync + 'static {
    /// Borrow the same worker's optional coarse geometry-coverage service.
    /// The canonical host keeps it alive through the synchronous render call.
    /// No provider retains the borrowed path, mask arena, or renderer context.
    fn coverage_provider(&self) -> Option<&dyn resvg::CoverageProvider> {
        None
    }
    fn reserve(
        &self,
        kind: ResourceKind,
        bytes: usize,
    ) -> core::result::Result<Ticket, ResourceError>;
    fn allocation_committed(
        &self,
        ticket: &Ticket,
        actual_layout_bytes: usize,
    ) -> core::result::Result<(), ResourceError>;
    fn release_after_free(&self, ticket: Ticket) -> core::result::Result<(), ResourceError>;
    fn checkpoint(&self, work: u64) -> core::result::Result<(), ResourceError>;
}
pub struct Lease {
    context: Context,
    ticket: Option<Ticket>,
    bytes: usize,
}
pub(crate) fn reserve_ticket(
    context: &Context,
    kind: ResourceKind,
    bytes: usize,
) -> Result<Ticket> {
    context
        .host
        .reserve(kind, bytes)
        .map_err(|e| failure_from_resource(context.record_resource(e)))
}
pub(crate) fn release_ticket(context: &Context, ticket: Ticket) -> Result<()> {
    context.host.release_after_free(ticket).map_err(|e| {
        context
            .diagnostics
            .release_failures
            .fetch_add(1, Ordering::Relaxed);
        failure_from_resource(context.record_resource(e))
    })
}
impl Lease {
    pub fn reserve(context: &Context, category: u32, bytes: usize) -> Result<Self> {
        let kind = match category {
            abi::FONT => ResourceKind::Font,
            abi::METRICS => ResourceKind::Metrics,
            abi::GEOMETRY => ResourceKind::Geometry,
            abi::PIXELS => ResourceKind::Pixels,
            abi::SCRATCH => ResourceKind::Scratch,
            _ => return Err(context.record_failure(Failure::Contract)),
        };
        Ok(Self {
            context: context.clone(),
            ticket: if bytes == 0 {
                None
            } else {
                Some(reserve_ticket(context, kind, bytes)?)
            },
            bytes,
        })
    }
    pub fn allocation_committed(&self, actual_layout_bytes: usize) -> Result<()> {
        if actual_layout_bytes > self.bytes {
            return Err(self.context.record_failure(Failure::Contract));
        }
        if let Some(ticket) = &self.ticket {
            self.context
                .host
                .allocation_committed(ticket, actual_layout_bytes)
                .map_err(|e| failure_from_resource(self.context.record_resource(e)))?;
        } else if actual_layout_bytes != 0 {
            return Err(self.context.record_failure(Failure::Contract));
        }
        Ok(())
    }
}
impl Drop for Lease {
    fn drop(&mut self) {
        if let Some(ticket) = self.ticket.take() {
            let _ = release_ticket(&self.context, ticket);
        }
    }
}

const RETAIN_QUANTUM: usize = 64 * 1024;
const RETAIN_LIMIT: usize = 128 * 1024 * 1024;
const RETAIN_TICKETS: usize = RETAIN_LIMIT / RETAIN_QUANTUM;
struct RetainedLedger {
    // Compact tickets share one Context, rather than copying the entire callback
    // record for each block. Fixed capacity is reserved before allocation.
    tickets: Vec<Ticket>,
    used: usize,
    capacity: usize,
    shared: markdown_svg_budget::BudgetVec<Option<TrackedArc>>,
}

/// A Weak keeps the real Arc control block physically alive until this record
/// drops. It never extends the lifetime of the payload's strong ownership.
enum TrackedWeak {
    Object(Weak<dyn std::any::Any + Send + Sync>),
    Variations(Weak<[usvg::FontVariation]>),
}
impl TrackedWeak {
    fn strong_count(&self) -> usize {
        match self {
            Self::Object(owner) => owner.strong_count(),
            Self::Variations(owner) => owner.strong_count(),
        }
    }
}
struct TrackedArc {
    owner: Option<TrackedWeak>,
    fee: Option<markdown_svg_budget::Lease>,
    resources: markdown_svg_budget::ResourcesRef,
    payload_fees: [Option<markdown_svg_budget::Lease>; 2],
}
impl Drop for TrackedArc {
    fn drop(&mut self) {
        if self
            .owner
            .as_ref()
            .is_some_and(|owner| owner.strong_count() != 0)
        {
            self.resources
                .record(ResourceError::new(FailureKind::Invariant, LimitKind::None));
            // A live strong owner gives no physical-free proof. Preserve the
            // original control block and fee; never make a false refund.
            core::mem::forget(self.owner.take());
            core::mem::forget(self.fee.take());
            for fee in &mut self.payload_fees {
                core::mem::forget(fee.take());
            }
            return;
        }
        drop(self.owner.take()); // Last real Weak frees the actual Arc block.
        for fee in &mut self.payload_fees {
            drop(fee.take());
        }
        drop(self.fee.take());
    }
}

pub struct RetainedPool {
    ledger: std::sync::Mutex<RetainedLedger>,
    context: Context,
    // After ledger/ticket storage. The retained payload has already physically
    // dropped when its last guard (and Adapter's request guard) lets this retire.
    metadata: Option<Lease>,
    resources: markdown_svg_budget::ResourcesRef,
}
impl std::fmt::Debug for RetainedPool {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("RetainedReplayPool").finish_non_exhaustive()
    }
}
impl usvg::ReplayRetention for RetainedPool {}
impl RetainedPool {
    pub fn new(
        context: &Context,
        resources: &markdown_svg_budget::ResourcesRef,
    ) -> Result<Arc<Self>> {
        let bytes = RETAIN_TICKETS
            .checked_mul(size_of::<Ticket>())
            .ok_or_else(|| context.record_failure(Failure::Contract))?;
        let metadata = Lease::reserve(context, abi::SCRATCH, bytes)?;
        let mut tickets = Vec::new();
        tickets
            .try_reserve_exact(RETAIN_TICKETS)
            .map_err(|_| context.record_failure(Failure::Allocation))?;
        if tickets.capacity() != RETAIN_TICKETS {
            return Err(context.record_failure(Failure::Allocation));
        }
        metadata.allocation_committed(tickets.capacity() * size_of::<Ticket>())?;
        Ok(Arc::new(Self {
            ledger: std::sync::Mutex::new(RetainedLedger {
                tickets,
                used: 0,
                capacity: 0,
                shared: markdown_svg_budget::BudgetVec::new(
                    resources.clone(),
                    ResourceKind::Scratch,
                ),
            }),
            context: context.clone(),
            metadata: Some(metadata),
            resources: resources.clone(),
        }))
    }
    /// Build one actual standard Arc required by the font/renderer contracts.
    /// Registry capacity is paid first; the final record assignment cannot fail.
    pub fn share<T: std::any::Any + Send + Sync>(self: &Arc<Self>, value: T) -> Result<Arc<T>> {
        self.share_with_storage(value, [None, None])
    }
    /// Paths use two ordinary upstream Vec allocations. Their exact paid leases
    /// follow the same last-owner/Weak physical-free proof as the Arc metadata.
    pub fn share_with_storage<T: std::any::Any + Send + Sync>(
        self: &Arc<Self>,
        value: T,
        payload_fees: [Option<markdown_svg_budget::Lease>; 2],
    ) -> Result<Arc<T>> {
        struct Pending<T> {
            value: Option<T>,
            fees: [Option<markdown_svg_budget::Lease>; 2],
        }
        // Field order also covers a reserve/cancellation failure before Arc exists.
        let mut pending = Pending {
            value: Some(value),
            fees: payload_fees,
        };
        let mut ledger = self
            .ledger
            .lock()
            .map_err(|_| self.context.record_failure(Failure::Contract))?;
        let index = ledger.shared.len();
        ledger
            .shared
            .try_push(None)
            .map_err(failure_from_resource)?;
        let resources = self.resources.clone();
        let mut fee = resources
            .reserve(ResourceKind::Scratch, arc_layout::<T>().size())
            .map_err(failure_from_resource)?;
        let owner = Arc::new(pending.value.take().expect("unpublished shared payload"));
        if let Err(error) = fee.allocation_committed() {
            drop(owner); // Actual complete Arc free before its pending fee.
            return Err(failure_from_resource(error));
        }
        let erased: Arc<dyn std::any::Any + Send + Sync> = owner.clone();
        let weak = Arc::downgrade(&erased);
        drop(erased);
        ledger.shared.as_mut_slice()[index] = Some(TrackedArc {
            owner: Some(TrackedWeak::Object(weak)),
            fee: Some(fee),
            resources,
            payload_fees: core::mem::replace(&mut pending.fees, [None, None]),
        });
        Ok(owner)
    }
    /// Exact one-allocation Arc slice required by the unchanged font contract.
    /// Fill directly in uninitialized Copy storage; no temporary Vec or clone.
    pub fn variations(self: &Arc<Self>, axes: &[abi::Axis]) -> Result<Arc<[usvg::FontVariation]>> {
        let mut ledger = self
            .ledger
            .lock()
            .map_err(|_| self.context.record_failure(Failure::Contract))?;
        let index = ledger.shared.len();
        ledger
            .shared
            .try_push(None)
            .map_err(failure_from_resource)?;
        let layout = Layout::new::<(usize, usize)>()
            .extend(
                Layout::array::<usvg::FontVariation>(axes.len())
                    .map_err(|_| self.context.record_failure(Failure::Contract))?,
            )
            .map_err(|_| self.context.record_failure(Failure::Contract))?
            .0
            .pad_to_align();
        let resources = self.resources.clone();
        let mut fee = resources
            .reserve(ResourceKind::Scratch, layout.size())
            .map_err(failure_from_resource)?;
        let mut owner = Arc::<[usvg::FontVariation]>::new_uninit_slice(axes.len());
        if let Err(error) = fee.allocation_committed() {
            drop(owner);
            return Err(failure_from_resource(error));
        }
        let values = Arc::get_mut(&mut owner).expect("unpublished Arc slice");
        for (index, axis) in axes.iter().enumerate() {
            if index % 256 == 0 {
                self.context.checkpoint(256)?;
            }
            values[index].write(usvg::FontVariation::new(
                axis.tag_be.to_be_bytes(),
                axis.value,
            ));
        }
        // SAFETY: every initialized slice element is written exactly once above.
        // Cancellation before this point drops only MaybeUninit<Copy> storage.
        let owner = unsafe { owner.assume_init() };
        ledger.shared.as_mut_slice()[index] = Some(TrackedArc {
            owner: Some(TrackedWeak::Variations(Arc::downgrade(&owner))),
            fee: Some(fee),
            resources,
            payload_fees: [None, None],
        });
        Ok(owner)
    }
    pub fn retain(self: &Arc<Self>, bytes: usize) -> Result<Arc<dyn usvg::ReplayRetention>> {
        if let Some(error) = self.context.failure() {
            return Err(error);
        }
        let mut ledger = self
            .ledger
            .lock()
            .map_err(|_| self.context.record_failure(Failure::Contract))?;
        let used = ledger
            .used
            .checked_add(bytes)
            .filter(|n| *n <= RETAIN_LIMIT)
            .ok_or_else(|| self.context.record_failure(Failure::Allocation))?;
        let capacity = used
            .checked_add(RETAIN_QUANTUM - 1)
            .ok_or_else(|| self.context.record_failure(Failure::Allocation))?
            / RETAIN_QUANTUM
            * RETAIN_QUANTUM;
        if capacity > ledger.capacity {
            if ledger.tickets.len() == RETAIN_TICKETS {
                return Err(self.context.record_failure(Failure::Allocation));
            }
            let ticket = reserve_ticket(
                &self.context,
                ResourceKind::Scratch,
                capacity - ledger.capacity,
            )?;
            // This rounded allowance also covers estimates reserved before
            // upstream allocates. It stays Pending: there is no exact physical
            // allocation here to admit with allocation_committed.
            ledger.tickets.push(ticket);
            ledger.capacity = capacity;
        }
        ledger.used = used;
        Ok(self.clone())
    }
}
impl Drop for RetainedPool {
    fn drop(&mut self) {
        let ledger = self
            .ledger
            .get_mut()
            .unwrap_or_else(|poison| poison.into_inner());
        // All payload caches are dropped before the Adapter's final pool field.
        // Weak retirement then frees each real Arc block before its own fee.
        ledger.shared.release_storage();
        for ticket in ledger.tickets.drain(..) {
            let _ = release_ticket(&self.context, ticket);
        }
        // The fixed Vec allocation is the only storage paid by this lease.
        // Pool Self/Arc header are paid by the enclosing Native bootstrap, which
        // remains alive until this destructor and the physical Arc free finish.
        drop(core::mem::take(&mut ledger.tickets));
        drop(self.metadata.take());
    }
}
mod sealed {
    pub trait Sealed {}
}
/// Every bit pattern is valid; initialized zero storage and FFI overwrite are safe.
pub trait Pod: Copy + sealed::Sealed {}
macro_rules! pod { ($($ty:ty),*)=>{$(impl sealed::Sealed for $ty{} impl Pod for $ty{})*}; }
pod!(
    u8,
    abi::Axis,
    abi::File,
    abi::Face,
    abi::LogicalRun,
    abi::Style,
    abi::LogicalSpan,
    abi::GlyphMetrics,
    abi::GlyphKey,
    abi::Command,
    abi::GlyphPath
);

pub struct AdmittedSlice<T: Pod> {
    pointer: NonNull<T>,
    length: usize,
    layout: Layout,
    lease: Option<Lease>,
}
impl<T: Pod> AdmittedSlice<T> {
    pub fn zeroed(callbacks: &Context, category: u32, length: usize) -> Result<Self> {
        let layout =
            Layout::array::<T>(length).map_err(|_| callbacks.record_failure(Failure::Contract))?;
        let lease = Lease::reserve(callbacks, category, layout.size())?;
        let pointer = if length == 0 {
            NonNull::dangling()
        } else {
            // SAFETY: nonzero checked Layout, all-zero representation is valid Pod.
            NonNull::new(unsafe { std::alloc::alloc_zeroed(layout) }.cast::<T>())
                .ok_or_else(|| callbacks.record_failure(Failure::Allocation))?
        };
        if let Err(error) = lease.allocation_committed(layout.size()) {
            if length != 0 {
                // SAFETY: the successful exact allocation has not escaped.
                unsafe {
                    std::alloc::dealloc(pointer.as_ptr().cast(), layout);
                }
            }
            return Err(error);
        }
        Ok(Self {
            pointer,
            length,
            layout,
            lease: Some(lease),
        })
    }
    pub fn len(&self) -> usize {
        self.length
    }
    pub fn as_slice(&self) -> &[T] {
        // SAFETY: this owner uniquely allocated exactly length initialized Pod values.
        unsafe { core::slice::from_raw_parts(self.pointer.as_ptr(), self.length) }
    }
    pub fn as_mut_slice(&mut self) -> &mut [T] {
        // SAFETY: mutable borrow prevents all other Rust access during a synchronous fill.
        unsafe { core::slice::from_raw_parts_mut(self.pointer.as_ptr(), self.length) }
    }
    pub fn as_mut_ptr(&mut self) -> *mut T {
        self.pointer.as_ptr()
    }
    /// Copy a validated Native metadata view before the earlier of next-plan entry
    /// or coarse return. Other callbacks keep metadata views valid by ABI2 contract.
    pub unsafe fn copy_native(
        callbacks: &Context,
        source: *const T,
        length: usize,
    ) -> Result<Self> {
        if length != 0 && (source.is_null() || (source as usize) % align_of::<T>() != 0) {
            return Err(callbacks.record_failure(Failure::Contract));
        }
        let mut owner = Self::zeroed(callbacks, abi::SCRATCH, length)?;
        if length != 0 {
            // SAFETY: caller guarantees bounded readable metadata for this callback
            // epoch. Destination is distinct final Rust storage, allocated above.
            unsafe {
                core::ptr::copy_nonoverlapping(source, owner.as_mut_ptr(), length);
            }
        }
        Ok(owner)
    }
}
impl<T: Pod> Drop for AdmittedSlice<T> {
    fn drop(&mut self) {
        if self.length != 0 {
            // SAFETY: exact pointer/Layout pair from this owner's successful allocation.
            unsafe {
                std::alloc::dealloc(self.pointer.as_ptr().cast(), self.layout);
            }
        }
        drop(self.lease.take()); // Actual storage is gone BEFORE the shared lease.
    }
}
// Immutable fontdb Binary owners may be held by Arc. Mutability still requires
// &mut self, and callbacks contain no borrowed Rust/Native object pointers. A wrong
// worker release is safely rejected/diagnosed by Native; coarse exit must reject
// a nonzero per-job release-failure count before destroying the registry.
unsafe impl<T: Pod + Send> Send for AdmittedSlice<T> {}
unsafe impl<T: Pod + Sync> Sync for AdmittedSlice<T> {}

pub struct FontBytes {
    bytes: AdmittedSlice<u8>,
    _lifetime: OwnerWitness,
}
impl FontBytes {
    pub fn fill(callbacks: &Context, file: &abi::File) -> Result<Self> {
        let len = usize::try_from(file.byte_length)
            .map_err(|_| callbacks.record_failure(Failure::Contract))?;
        if !(12..=64 * 1024 * 1024).contains(&len) || file.ticket == 0 || file.collection_faces == 0
        {
            return Err(callbacks.record_failure(Failure::Contract));
        }
        let mut bytes = AdmittedSlice::zeroed(callbacks, abi::FONT, len)?;
        let mut written = 0;
        // SAFETY: final exclusively borrowed Rust owner has exact admitted capacity.
        // Native directly copies <=64KiB fragments; no intermediate font payload.
        let hr = unsafe {
            callbacks.fill_font.unwrap()(
                callbacks.session,
                file.ticket,
                bytes.as_mut_ptr(),
                file.byte_length,
                &mut written,
            )
        };
        callbacks.native_success(hr)?;
        if written != file.byte_length {
            return Err(callbacks.record_failure(Failure::Contract));
        }
        callbacks.checkpoint(file.byte_length)?;
        Ok(Self {
            bytes,
            _lifetime: OwnerWitness::new(callbacks, OwnerKind::Font),
        })
    }
}
impl AsRef<[u8]> for FontBytes {
    fn as_ref(&self) -> &[u8] {
        self.bytes.as_slice()
    }
}
impl core::fmt::Debug for FontBytes {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("FontBytes")
            .field("length", &self.bytes.len())
            .finish_non_exhaustive()
    }
}
