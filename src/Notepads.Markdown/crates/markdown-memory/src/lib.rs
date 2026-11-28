// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE file in the project root for license information.
// ---------------------------------------------------------------------------------------------

//! Narrow, platform-neutral allocation receipts for the controlled flowchart fork.
//!
//! Authorities and their reference headers are prepaid by the enclosing job.
//! Acquiring, committing and returning a receipt allocate nothing. Each owner
//! must free its actual block before dropping its receipt. A growth operation
//! retains the old owner while reserving the complete new Layout, not a delta.
//! There is no process-global allocator, implicit current scope or budget panic.

use std::alloc::Layout;
use std::fmt;
use std::sync::Arc;

mod allocator;
mod boxed;
mod clone_group;
mod collections;
mod region;
mod shared;
pub use allocator::{ExactAllocator, clone_hashmap_prepared, clone_hashset_prepared, plan_hashmap, plan_hashset, table_layout};
pub use boxed::FundedBox;
pub use clone_group::{CloneGroup, ClonePlan, OwnedPayload, PlanClone};
pub use collections::{BoundedClone, FundedString, FundedVec};
pub use region::{NativePayload, NativeRegion, RegionHandle};
pub use shared::FundedShared;
pub type OwnedVec<T> = FundedVec<T>;
pub type OwnedString = FundedString;

#[cfg(test)]
mod collection_tests;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u8)]
pub enum Family {
    Parse,
    Model,
    Graph,
    Work,
    Svg,
    Canonical,
    Text,
    Cache,
    Metadata,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MemoryError {
    BudgetLimited,
    OutOfMemory,
    ArithmeticOverflow,
    InvalidState,
    Cancelled,
    Unfunded,
    WorkLimited,
}

impl fmt::Display for MemoryError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{self:?}")
    }
}

impl std::error::Error for MemoryError {}

/// Implemented by the enclosing diagram job's real source/process/control grant.
/// Methods must not allocate, invoke client callbacks, or panic. A rejection
/// records a fixed terminal reason before returning; it never calls an allocator.
/// `release` follows real free, except for an unmaterialized reservation rollback.
pub trait AllocationAuthority: Send + Sync {
    fn reserve(&self, layout: Layout, family: Family) -> Result<(), MemoryError>;
    fn release(&self, layout: Layout, family: Family);
    fn failure(&self) -> Option<MemoryError>;
    fn record_failure(&self, _error: MemoryError) {}
    /// Allocation-site cancellation/OOM injection, after complete admission.
    /// Production authorities normally return Ok; this cannot add a charge.
    fn before_allocate(&self, _layout: Layout, _family: Family) -> Result<(), MemoryError> {
        Ok(())
    }
    fn charge_work(&self, _units: usize) -> Result<(), MemoryError> {
        Ok(())
    }
}

#[derive(Clone)]
pub struct MemoryScope {
    authority: Arc<dyn AllocationAuthority>,
}

impl MemoryScope {
    /// The supplied Arc header must already be funded. This merely moves it.
    pub fn new(authority: Arc<dyn AllocationAuthority>) -> Self {
        Self { authority }
    }

    pub fn reserve(&self, layout: Layout, family: Family) -> Result<GrowthTicket, MemoryError> {
        if let Some(error) = self.authority.failure() {
            return Err(error);
        }
        self.authority.reserve(layout, family)?;
        Ok(GrowthTicket {
            scope: self.clone(),
            layout,
            family,
            armed: true,
        })
    }

    pub fn reserve_array<T>(
        &self,
        count: usize,
        family: Family,
    ) -> Result<GrowthTicket, MemoryError> {
        let layout = Layout::array::<T>(count).map_err(|_| MemoryError::ArithmeticOverflow)?;
        self.reserve(layout, family)
    }

    pub fn failure(&self) -> Option<MemoryError> {
        self.authority.failure()
    }

    pub fn same_root(&self, other: &Self) -> bool {
        Arc::ptr_eq(&self.authority, &other.authority)
    }

    pub fn record_failure(&self, error: MemoryError) {
        self.authority.record_failure(error);
    }

    pub fn before_allocate(&self, layout: Layout, family: Family) -> Result<(), MemoryError> {
        self.authority
            .before_allocate(layout, family)
            .inspect_err(|error| self.record_failure(*error))
    }

    pub fn charge_work(&self, units: usize) -> Result<(), MemoryError> {
        self.authority
            .charge_work(units)
            .inspect_err(|error| self.record_failure(*error))
    }
}

impl fmt::Debug for MemoryScope {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("MemoryScope")
            .finish_non_exhaustive()
    }
}

/// Nonclone, complete Layout reservation. Dropping before materialization
/// returns unused admission. After materialization the caller must either
/// commit it to the actual owner or free the block before dropping this value.
#[must_use]
pub struct GrowthTicket {
    scope: MemoryScope,
    layout: Layout,
    family: Family,
    armed: bool,
}

impl GrowthTicket {
    pub fn scope(&self) -> &MemoryScope {
        &self.scope
    }
    pub fn layout(&self) -> Layout {
        self.layout
    }

    pub fn family(&self) -> Family {
        self.family
    }

    /// No allocation or fallible operation. The actual block's owner receives
    /// this value and must retain it until after real deallocation.
    pub fn commit(mut self) -> OwnerTicket {
        self.armed = false;
        OwnerTicket {
            scope: self.scope.clone(),
            layout: self.layout,
            family: self.family,
        }
    }
}

impl Drop for GrowthTicket {
    fn drop(&mut self) {
        if self.armed {
            self.scope.authority.release(self.layout, self.family);
        }
    }
}

impl fmt::Debug for GrowthTicket {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("GrowthTicket")
            .field("layout", &self.layout)
            .field("family", &self.family)
            .finish()
    }
}

/// Actual block receipt. This does not free a block by itself. Field/drop order
/// must make the actual data disappear first, including every nested owner.
#[must_use]
pub struct OwnerTicket {
    scope: MemoryScope,
    layout: Layout,
    family: Family,
}

impl OwnerTicket {
    pub fn layout(&self) -> Layout {
        self.layout
    }

    pub fn family(&self) -> Family {
        self.family
    }

    pub fn scope(&self) -> &MemoryScope {
        &self.scope
    }
}

impl Drop for OwnerTicket {
    fn drop(&mut self) {
        self.scope.authority.release(self.layout, self.family);
    }
}

impl fmt::Debug for OwnerTicket {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("OwnerTicket")
            .field("layout", &self.layout)
            .field("family", &self.family)
            .finish()
    }
}

/// Exact-layout, zero-initialized temporary storage. The allocator receives
/// exactly the Layout paid before its invocation; there is no post-admission
/// capacity guess. The receipt is declared after data and returned after free.
pub struct OwnedBlock {
    pointer: std::ptr::NonNull<u8>,
    layout: Layout,
    _ticket: OwnerTicket,
}

// SAFETY: this is uniquely owned ordinary byte storage. Safe mutable access
// requires &mut Self, and the authority is Send + Sync. No platform object or
// thread-affine pointer is stored here.
unsafe impl Send for OwnedBlock {}
unsafe impl Sync for OwnedBlock {}

impl OwnedBlock {
    pub fn try_new(
        scope: &MemoryScope,
        layout: Layout,
        family: Family,
    ) -> Result<Self, MemoryError> {
        let ticket = scope.reserve(layout, family)?;
        let pointer = if layout.size() == 0 {
            // A zero-length slice needs a nonnull aligned pointer, not storage.
            std::ptr::NonNull::new(std::ptr::without_provenance_mut(layout.align()))
                .ok_or(MemoryError::InvalidState)?
        } else {
            scope.before_allocate(layout, family)?;
            // SAFETY: Layout is validated by std and completely reserved above.
            let raw = unsafe { std::alloc::alloc_zeroed(layout) };
            match std::ptr::NonNull::new(raw) {
                Some(pointer) => pointer,
                None => {
                    scope.record_failure(MemoryError::OutOfMemory);
                    return Err(MemoryError::OutOfMemory);
                }
            }
        };
        Ok(Self {
            pointer,
            layout,
            _ticket: ticket.commit(),
        })
    }

    pub fn try_array<T>(
        scope: &MemoryScope,
        count: usize,
        family: Family,
    ) -> Result<Self, MemoryError> {
        let layout = Layout::array::<T>(count).map_err(|_| MemoryError::ArithmeticOverflow)?;
        Self::try_new(scope, layout, family)
    }

    pub fn layout(&self) -> Layout {
        self.layout
    }
    pub fn as_ptr(&self) -> *const u8 {
        self.pointer.as_ptr()
    }
    pub fn as_mut_ptr(&mut self) -> *mut u8 {
        self.pointer.as_ptr()
    }

    pub fn as_slice(&self) -> &[u8] {
        // SAFETY: storage is initialized, live and exactly layout.size bytes.
        unsafe { std::slice::from_raw_parts(self.pointer.as_ptr(), self.layout.size()) }
    }

    pub fn as_mut_slice(&mut self) -> &mut [u8] {
        // SAFETY: unique access to the same fully initialized byte storage.
        unsafe { std::slice::from_raw_parts_mut(self.pointer.as_ptr(), self.layout.size()) }
    }
}

impl Drop for OwnedBlock {
    fn drop(&mut self) {
        if self.layout.size() != 0 {
            // SAFETY: pointer came from alloc_zeroed with this exact Layout and
            // is uniquely owned. The _ticket field drops only after this body.
            unsafe { std::alloc::dealloc(self.pointer.as_ptr(), self.layout) };
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};

    struct Authority {
        used: AtomicUsize,
        limit: usize,
    }

    impl AllocationAuthority for Authority {
        fn reserve(&self, layout: Layout, _: Family) -> Result<(), MemoryError> {
            self.used
                .try_update(Ordering::SeqCst, Ordering::SeqCst, |old| {
                    old.checked_add(layout.size())
                        .filter(|next| *next <= self.limit)
                })
                .map(|_| ())
                .map_err(|_| MemoryError::BudgetLimited)
        }

        fn release(&self, layout: Layout, _: Family) {
            self.used.fetch_sub(layout.size(), Ordering::SeqCst);
        }

        fn failure(&self) -> Option<MemoryError> {
            None
        }
    }

    #[test]
    fn complete_old_and_new_layouts_remain_funded_until_each_owner_frees() {
        let authority = Arc::new(Authority {
            used: AtomicUsize::new(0),
            limit: 96,
        });
        let scope = MemoryScope::new(authority.clone());
        let old = scope
            .reserve_array::<u8>(32, Family::Graph)
            .unwrap()
            .commit();
        let next = scope.reserve_array::<u8>(64, Family::Graph).unwrap();
        assert_eq!(authority.used.load(Ordering::SeqCst), 96);
        assert!(matches!(
            scope.reserve_array::<u8>(1, Family::Graph),
            Err(MemoryError::BudgetLimited)
        ));
        drop(old);
        assert_eq!(authority.used.load(Ordering::SeqCst), 64);
        let next = next.commit();
        assert_eq!(authority.used.load(Ordering::SeqCst), 64);
        drop(next);
        assert_eq!(authority.used.load(Ordering::SeqCst), 0);
    }

    #[test]
    fn unmaterialized_group_rejection_rolls_back_without_spending_the_old_owner() {
        let authority = Arc::new(Authority {
            used: AtomicUsize::new(0),
            limit: 80,
        });
        let scope = MemoryScope::new(authority.clone());
        let old = scope
            .reserve_array::<u8>(24, Family::Model)
            .unwrap()
            .commit();
        let first = scope.reserve_array::<u8>(40, Family::Model).unwrap();
        assert!(scope.reserve_array::<u8>(24, Family::Graph).is_err());
        drop(first);
        assert_eq!(authority.used.load(Ordering::SeqCst), 24);
        drop(old);
        assert_eq!(authority.used.load(Ordering::SeqCst), 0);
    }
}
