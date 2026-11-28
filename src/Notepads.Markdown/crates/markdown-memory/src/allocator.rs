// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE file in the project root for license information.
// ---------------------------------------------------------------------------------------------

//! Exact-layout backing for the controlled graph fork.
//!
//! This is an allocator boundary, not a process-global allocator or a second
//! budget. Every allocation delegates admission to the enclosing MemoryScope.
//! Container callers must use fallible reserve/insert paths and complete their
//! prepared mutation before changing graph semantics.

use allocator_api2::alloc::{AllocError, Allocator};
use crate::{Family, MemoryError, MemoryScope, OwnerTicket};
use std::alloc::Layout;
use std::cell::Cell;
use std::sync::Mutex;
use std::ptr::NonNull;


/// The receipt is physically colocated with the actual table allocation and
/// remains alive even after all allocator clones except the container disappear.
struct AllocationHeader {
    layout: Layout,
    _ticket: Option<OwnerTicket>,
}

/// Clones share the already prepaid authority reference; cloning allocates no
/// metadata. `None` is the upstream compatibility path, never a production grant.
#[derive(Debug)]
pub struct ExactAllocator {
    scope: Option<MemoryScope>,
    family: Family,
    prepared_only: bool,
    prepared: Mutex<Option<crate::GrowthTicket>>,
}

impl Default for ExactAllocator {
    fn default() -> Self { Self { scope: None, family: Family::Graph, prepared_only: false, prepared: Mutex::new(None) } }
}

impl Clone for ExactAllocator {
    fn clone(&self) -> Self {
        Self {
            scope: self.scope.clone(),
            family: self.family,
            prepared_only: self.prepared_only,
            // A complete block receipt cannot be duplicated. Clones can free
            // existing blocks, but a prepared-only clone has no new grant.
            prepared: Mutex::new(None),
        }
    }
}

impl ExactAllocator {
    pub fn new(scope: MemoryScope) -> Self {
        Self::new_in(scope, Family::Graph)
    }

    pub fn with_prepared(ticket: crate::GrowthTicket) -> Self {
        Self {
            scope: Some(ticket.scope().clone()),
            family: ticket.family(),
            prepared_only: true,
            prepared: Mutex::new(Some(ticket)),
        }
    }

    pub fn new_in(scope: MemoryScope, family: Family) -> Self {
        Self { scope: Some(scope), family, prepared_only: false, prepared: Mutex::new(None) }
    }

    pub fn scope(&self) -> Option<&MemoryScope> {
        self.scope.as_ref()
    }

    pub fn combined_layout(payload: Layout) -> Result<(Layout, usize), MemoryError> {
        Layout::new::<AllocationHeader>()
            .extend(payload)
            .map(|(layout, offset)| (layout.pad_to_align(), offset))
            .map_err(|_| MemoryError::ArithmeticOverflow)
    }

    fn reject(&self, failure: MemoryError) -> AllocError {
        if let Some(scope) = &self.scope {
            scope.record_failure(failure);
        }
        AllocError
    }
}

// SAFETY: each pointer is produced by std::alloc for a combined validated Layout.
// Header placement and the returned payload offset account for both alignments.
// Each deallocation reconstructs the same base and layout from its allocation;
// allocator clones preserve the root and allocation contract. No allocation is
// exposed to a caller before its complete layout, including this header, is paid.
unsafe impl Allocator for ExactAllocator {
    fn allocate(&self, layout: Layout) -> Result<NonNull<[u8]>, AllocError> {
        if layout.size() == 0 {
            let pointer = NonNull::new(std::ptr::without_provenance_mut(layout.align()))
                .ok_or_else(|| self.reject(MemoryError::InvalidState))?;
            return Ok(NonNull::slice_from_raw_parts(pointer, 0));
        }
        let (combined, offset) = Self::combined_layout(layout)
            .map_err(|failure| self.reject(failure))?;
        let reservation = if self.prepared_only {
            let mut prepared = self.prepared.try_lock()
                .map_err(|_| self.reject(MemoryError::InvalidState))?;
            let ticket = prepared.take().ok_or_else(|| self.reject(MemoryError::Unfunded))?;
            if ticket.layout() != combined {
                return Err(self.reject(MemoryError::InvalidState));
            }
            Some(ticket)
        } else {
            self.scope.as_ref().map(|scope| scope.reserve(combined, self.family))
                .transpose().map_err(|failure| self.reject(failure))?
        };
        if let Some(scope) = &self.scope {
            scope.before_allocate(combined, reservation.as_ref().map_or(self.family, |ticket| ticket.family()))
                .map_err(|failure| self.reject(failure))?;
        }
        // SAFETY: complete requested Layout was validated and admitted above.
        let raw = unsafe { std::alloc::alloc(combined) };
        let base = NonNull::new(raw).ok_or_else(|| self.reject(MemoryError::OutOfMemory))?;
        let header = AllocationHeader {
            layout: combined,
            _ticket: reservation.map(|ticket| ticket.commit()),
        };
        // SAFETY: `combined` includes a properly aligned header and the complete
        // payload. Header initialization and pointer arithmetic cannot fail.
        unsafe {
            base.cast::<AllocationHeader>().as_ptr().write(header);
            let payload = NonNull::new_unchecked(base.as_ptr().add(offset));
            Ok(NonNull::slice_from_raw_parts(payload, layout.size()))
        }
    }

    unsafe fn deallocate(&self, pointer: NonNull<u8>, layout: Layout) {
        if layout.size() == 0 {
            return;
        }
        // The same payload Layout must be supplied under Allocator's contract.
        // Recomputing it allocates nothing and cannot fail for a valid pointer.
        let Ok((_, offset)) = Self::combined_layout(layout) else {
            // A caller violating the unsafe allocator contract cannot safely be
            // repaired by freeing a guessed address. Preserve the receipt/block.
            if let Some(scope) = &self.scope {
                scope.record_failure(MemoryError::InvalidState);
            }
            return;
        };
        // SAFETY: caller guarantees a live allocation with this payload Layout.
        // Move the header out without dropping it, free the actual backing, then
        // drop its receipt. Returning credit before dealloc would be incorrect.
        unsafe {
            let base = pointer.as_ptr().sub(offset);
            let header = base.cast::<AllocationHeader>().read();
            std::alloc::dealloc(base, header.layout);
            drop(header);
        }
    }
}

#[derive(Clone, Copy)]
struct LayoutProbe<'a> {
    requested: &'a Cell<Option<Layout>>,
    calls: &'a Cell<usize>,
}

// SAFETY: this allocator never returns a pointer or calls a system allocator.
// It is used only behind hashbrown::try_reserve, which handles AllocError.
unsafe impl Allocator for LayoutProbe<'_> {
    fn allocate(&self, layout: Layout) -> Result<NonNull<[u8]>, AllocError> {
        self.calls.set(self.calls.get().saturating_add(1));
        self.requested.set(Some(layout));
        Err(AllocError)
    }

    unsafe fn deallocate(&self, _: NonNull<u8>, _: Layout) {
        // No successful allocation exists for this planning-only allocator.
    }
}

/// Obtains the fixed hashbrown implementation's actual requested table Layout
/// without allocating a table or estimating bytes from element capacity.
pub fn table_layout<K: Eq + std::hash::Hash, V>(entries: usize) -> Result<Option<Layout>, MemoryError> {
    let requested = Cell::new(None);
    let calls = Cell::new(0);
    let mut table = hashbrown::HashMap::<K, V, _, _>::with_hasher_in(
        rustc_hash::FxBuildHasher,
        LayoutProbe { requested: &requested, calls: &calls },
    );
    let result = table.try_reserve(entries);
    match (result, calls.get(), requested.get()) {
        (Ok(()), 0, None) => Ok(None),
        (Err(_), 1, Some(layout)) => ExactAllocator::combined_layout(layout).map(|pair| Some(pair.0)),
        (Err(_), 0, None) => Err(MemoryError::ArithmeticOverflow),
        _ => Err(MemoryError::InvalidState),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::AllocationAuthority;
    use rustc_hash::FxBuildHasher;
    use std::sync::Arc;
    use std::sync::atomic::{AtomicUsize, Ordering};

    struct Authority {
        used: AtomicUsize,
        peak: AtomicUsize,
        failure: AtomicUsize,
        limit: usize,
    }

    impl AllocationAuthority for Authority {
        fn reserve(&self, layout: Layout, _: Family) -> Result<(), MemoryError> {
            let result = self.used.try_update(Ordering::SeqCst, Ordering::SeqCst, |old| {
                old.checked_add(layout.size()).filter(|next| *next <= self.limit)
            });
            match result {
                Ok(old) => {
                    self.peak.fetch_max(old + layout.size(), Ordering::SeqCst);
                    Ok(())
                }
                Err(_) => {
                    self.record_failure(MemoryError::BudgetLimited);
                    Err(MemoryError::BudgetLimited)
                }
            }
        }

        fn release(&self, layout: Layout, _: Family) {
            self.used.fetch_sub(layout.size(), Ordering::SeqCst);
        }

        fn failure(&self) -> Option<MemoryError> {
            (self.failure.load(Ordering::SeqCst) != 0).then_some(MemoryError::BudgetLimited)
        }

        fn record_failure(&self, _: MemoryError) {
            self.failure.store(1, Ordering::SeqCst);
        }
    }

    fn allocator(limit: usize) -> (ExactAllocator, Arc<Authority>) {
        let authority = Arc::new(Authority {
            used: AtomicUsize::new(0),
            peak: AtomicUsize::new(0),
            failure: AtomicUsize::new(0),
            limit,
        });
        (ExactAllocator::new(MemoryScope::new(authority.clone())), authority)
    }

    #[test]
    fn table_reserve_rejects_before_semantic_insert_and_preserves_fixed_reason() {
        let (allocator, authority) = allocator(0);
        let mut table = hashbrown::HashMap::<u32, u32, _, _>::with_hasher_in(
            FxBuildHasher,
            allocator,
        );
        assert!(table.try_reserve(1).is_err());
        assert!(table.is_empty());
        assert_eq!(authority.used.load(Ordering::SeqCst), 0);
        assert_eq!(authority.failure(), Some(MemoryError::BudgetLimited));
    }

    #[test]
    fn complete_headers_and_old_new_backings_remain_charged_during_growth() {
        let old_layout = Layout::from_size_align(24, 8).unwrap();
        let new_layout = Layout::from_size_align(96, 16).unwrap();
        let (old_combined, _) = ExactAllocator::combined_layout(old_layout).unwrap();
        let (new_combined, _) = ExactAllocator::combined_layout(new_layout).unwrap();
        let total = old_combined.size() + new_combined.size();
        let (allocator, authority) = allocator(total);
        let old = allocator.allocate(old_layout).unwrap().cast::<u8>();
        // SAFETY: old is this allocator's allocation with exactly old_layout.
        let next = unsafe { allocator.grow(old, old_layout, new_layout) }.unwrap().cast::<u8>();
        assert_eq!(authority.peak.load(Ordering::SeqCst), total);
        assert_eq!(authority.used.load(Ordering::SeqCst), new_combined.size());
        // SAFETY: next is the successful grow result with new_layout.
        unsafe { allocator.deallocate(next, new_layout) };
        assert_eq!(authority.used.load(Ordering::SeqCst), 0);
    }

    #[test]
    fn payload_only_quota_cannot_hide_the_receipt_and_alignment_header() {
        let payload = Layout::from_size_align(32, 64).unwrap();
        let (allocator, authority) = allocator(payload.size());
        assert!(allocator.allocate(payload).is_err());
        assert_eq!(authority.used.load(Ordering::SeqCst), 0);
        assert_eq!(authority.failure(), Some(MemoryError::BudgetLimited));
    }
}

use crate::{ClonePlan, CloneGroup, PlanClone};
type Table<K, V> = hashbrown::HashMap<K, V, rustc_hash::FxBuildHasher, ExactAllocator>;

pub fn plan_hashmap<K, V>(source: &Table<K, V>, plan: &mut ClonePlan) -> Result<(), MemoryError>
where K: PlanClone + Eq + std::hash::Hash, V: PlanClone {
    plan.scope().charge_work(source.len().checked_mul(2).ok_or(MemoryError::ArithmeticOverflow)?)?;
    if let Some(layout) = table_layout::<K, V>(source.len())? { plan.layout(layout)?; }
    for (key, value) in source { key.plan_clone(plan)?; value.plan_clone(plan)?; }
    Ok(())
}

pub fn clone_hashmap_prepared<K, V>(source: &Table<K, V>, group: &mut CloneGroup) -> Result<Table<K, V>, MemoryError>
where K: PlanClone + Eq + std::hash::Hash, V: PlanClone {
    let allocator = match table_layout::<K, V>(source.len())? {
        Some(layout) => ExactAllocator::with_prepared(group.take_layout(layout)?),
        None => ExactAllocator::new_in(group.scope().clone(), group.family()),
    };
    let mut table = Table::with_hasher_in(rustc_hash::FxBuildHasher, allocator);
    table.try_reserve(source.len()).map_err(|_| group.scope().failure().unwrap_or(MemoryError::InvalidState))?;
    for (key, value) in source {
        let key = key.clone_prepared(group)?;
        let value = value.clone_prepared(group)?;
        // Every source key is unique and exact capacity was already prepared.
        table.insert(key, value);
    }
    Ok(table)
}


impl<K, V> crate::PlanClone for Table<K, V>
where K: crate::PlanClone + Eq + std::hash::Hash, V: crate::PlanClone {
    fn plan_clone(&self, plan: &mut crate::ClonePlan) -> Result<(), MemoryError> { plan_hashmap(self, plan) }
    fn clone_prepared(&self, group: &mut crate::CloneGroup) -> Result<Self, MemoryError> { clone_hashmap_prepared(self, group) }
}

type Set<K> = hashbrown::HashSet<K, rustc_hash::FxBuildHasher, ExactAllocator>;

pub fn plan_hashset<K>(source: &Set<K>, plan: &mut ClonePlan) -> Result<(), MemoryError>
where K: PlanClone + Eq + std::hash::Hash {
    plan.scope().charge_work(source.len().checked_mul(2).ok_or(MemoryError::ArithmeticOverflow)?)?;
    // Pinned hashbrown HashSet is a HashMap<K, (), S, A>; its reserve requests
    // exactly that table layout, including control bytes and our receipt header.
    if let Some(layout) = table_layout::<K, ()>(source.len())? { plan.layout(layout)?; }
    for key in source { key.plan_clone(plan)?; }
    Ok(())
}

pub fn clone_hashset_prepared<K>(source: &Set<K>, group: &mut CloneGroup) -> Result<Set<K>, MemoryError>
where K: PlanClone + Eq + std::hash::Hash {
    let allocator = match table_layout::<K, ()>(source.len())? {
        Some(layout) => ExactAllocator::with_prepared(group.take_layout(layout)?),
        None => ExactAllocator::new_in(group.scope().clone(), group.family()),
    };
    let mut result = Set::with_hasher_in(rustc_hash::FxBuildHasher, allocator);
    result.try_reserve(source.len()).map_err(|_| group.scope().failure().unwrap_or(MemoryError::InvalidState))?;
    for key in source { result.insert(key.clone_prepared(group)?); }
    Ok(result)
}

impl<K> crate::PlanClone for Set<K>
where K: crate::PlanClone + Eq + std::hash::Hash {
    fn plan_clone(&self, plan: &mut crate::ClonePlan) -> Result<(), MemoryError> { plan_hashset(self, plan) }
    fn clone_prepared(&self, group: &mut crate::CloneGroup) -> Result<Self, MemoryError> { clone_hashset_prepared(self, group) }
}
