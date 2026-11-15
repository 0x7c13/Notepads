// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE file in the project root for license information.
// ---------------------------------------------------------------------------------------------

//! A joint receipt owner for upstream plain values that move through lexer,
//! AST and semantic construction. This is the existing authority's owner, not
//! another budget. Retired temporary blocks remain conservatively charged
//! until the complete native payload and all scoped producers are gone.

use crate::{
    CloneGroup, ClonePlan, Family, FundedShared, FundedString, FundedVec, MemoryError, MemoryScope,
    OwnerTicket, PlanClone,
};
use std::{
    fmt,
    ops::Deref,
    sync::{Mutex, MutexGuard},
};

pub type RegionHandle = FundedShared<NativeRegion>;

pub struct NativeRegion {
    owners: Mutex<FundedVec<OwnerTicket>>,
    scope: MemoryScope,
}

impl NativeRegion {
    pub fn try_new(
        scope: &MemoryScope,
        receipt_capacity: usize,
    ) -> Result<RegionHandle, MemoryError> {
        let owners = FundedVec::try_with_capacity(scope, Family::Metadata, receipt_capacity)?;
        FundedShared::try_new_in(
            scope,
            Family::Metadata,
            Self {
                owners: Mutex::new(owners),
                scope: scope.clone(),
            },
        )
    }

    pub fn scope(&self) -> &MemoryScope {
        &self.scope
    }

    fn fail<T>(&self, failure: MemoryError) -> Result<T, MemoryError> {
        self.scope.record_failure(failure);
        Err(failure)
    }

    fn owner_slots(
        &self,
        count: usize,
    ) -> Result<MutexGuard<'_, FundedVec<OwnerTicket>>, MemoryError> {
        if let Some(failure) = self.scope.failure() {
            return Err(failure);
        }
        let owners = self.owners.lock().map_err(|_| {
            self.scope.record_failure(MemoryError::InvalidState);
            MemoryError::InvalidState
        })?;
        if count > owners.capacity().saturating_sub(owners.len()) {
            return self.fail(MemoryError::BudgetLimited);
        }
        Ok(owners)
    }

    pub fn owner_blocks(&self) -> Result<usize, MemoryError> {
        Ok(self.owner_slots(0)?.len())
    }

    pub fn try_string(&self, source: &str, family: Family) -> Result<String, MemoryError> {
        self.scope.charge_work(source.len())?;
        let mut owners = self.owner_slots(usize::from(!source.is_empty()))?;
        let (value, receipt) =
            FundedString::try_from_str(&self.scope, family, source)?.into_parts();
        if let Some(receipt) = receipt {
            if let Err((receipt, failure)) = owners.push_reserved(receipt) {
                drop(value);
                drop(receipt);
                return self.fail(failure);
            }
        }
        Ok(value)
    }

    /// The caller may fill only this prepared capacity, without std growth.
    pub fn try_string_capacity(
        &self,
        capacity: usize,
        family: Family,
    ) -> Result<String, MemoryError> {
        let mut owners = self.owner_slots(usize::from(capacity != 0))?;
        let (value, receipt) =
            FundedString::try_with_capacity(&self.scope, family, capacity)?.into_parts();
        if let Some(receipt) = receipt {
            if let Err((receipt, failure)) = owners.push_reserved(receipt) {
                drop(value);
                drop(receipt);
                return self.fail(failure);
            }
        }
        Ok(value)
    }

    /// All nested payload blocks are reserved together before the first
    /// payload allocation, and every resulting receipt transfers to this owner.
    pub fn try_clone_recipe<T: PlanClone>(
        &self,
        source: &T,
        family: Family,
    ) -> Result<T, MemoryError> {
        self.try_build_recipe(
            family,
            |plan| source.plan_clone(plan),
            |group| source.clone_prepared(group),
        )
    }

    /// Plans a transformed value as one group, including all nested arrays and
    /// strings, before materializing any payload backing. Both closures must
    /// describe and consume exactly the same ordered layouts.
    pub fn try_build_recipe<T>(
        &self,
        family: Family,
        plan_value: impl FnOnce(&mut ClonePlan) -> Result<(), MemoryError>,
        materialize: impl FnOnce(&mut CloneGroup) -> Result<T, MemoryError>,
    ) -> Result<T, MemoryError> {
        let mut plan = ClonePlan::new(self.scope.clone(), family);
        plan_value(&mut plan)?;
        let mut owners = self.owner_slots(plan.blocks())?;
        let mut group = plan.prepare()?;
        let value = materialize(&mut group)?;
        let (value, mut receipts) = group.finish(value)?.into_parts();
        while let Some(receipt) = receipts.pop() {
            if let Err((receipt, failure)) = owners.push_reserved(receipt) {
                drop(value);
                drop(receipt);
                drop(receipts);
                return self.fail(failure);
            }
        }
        Ok(value)
    }

    /// Only containers created empty or already grown through this region may
    /// enter here. Full new backing is admitted while the old backing remains
    /// live; its retired receipt conservatively stays in the joint region.
    pub fn try_reserve_vec<T>(
        &self,
        values: &mut Vec<T>,
        additional: usize,
        family: Family,
    ) -> Result<(), MemoryError> {
        let required = values.len().checked_add(additional).ok_or_else(|| {
            self.scope.record_failure(MemoryError::ArithmeticOverflow);
            MemoryError::ArithmeticOverflow
        })?;
        if required <= values.capacity() {
            return Ok(());
        }
        self.scope.charge_work(values.len())?;
        let capacity = values
            .capacity()
            .checked_mul(2)
            .map(|n| n.max(required).max(4))
            .ok_or_else(|| {
                self.scope.record_failure(MemoryError::ArithmeticOverflow);
                MemoryError::ArithmeticOverflow
            })?;
        let mut owners = self.owner_slots(usize::from(size_of::<T>() != 0))?;
        let (mut prepared, receipt) =
            FundedVec::try_with_capacity(&self.scope, family, capacity)?.into_parts();
        prepared.append(values); // Exact prepared capacity; moves initialized values.
        std::mem::swap(values, &mut prepared);
        drop(prepared); // Actual retired backing disappears before any receipt can return.
        if let Some(receipt) = receipt {
            if let Err((receipt, failure)) = owners.push_reserved(receipt) {
                // Slot admission happened before allocation under this same lock.
                // Preserve ownership even on an internal invariant violation.
                let retired = std::mem::take(values);
                drop(retired);
                drop(receipt);
                return self.fail(failure);
            }
        }
        Ok(())
    }

    pub fn try_push<T>(
        &self,
        values: &mut Vec<T>,
        value: T,
        family: Family,
    ) -> Result<(), MemoryError> {
        self.try_reserve_vec(values, 1, family)?;
        values.push(value); // Prepared, exact live storage has a retained receipt.
        Ok(())
    }

    pub fn try_push_str(
        &self,
        value: &mut String,
        extra: &str,
        family: Family,
    ) -> Result<(), MemoryError> {
        if extra.is_empty() {
            return Ok(());
        }
        let length = value.len().checked_add(extra.len()).ok_or_else(|| {
            self.scope.record_failure(MemoryError::ArithmeticOverflow);
            MemoryError::ArithmeticOverflow
        })?;
        if length <= value.capacity() {
            self.scope.charge_work(extra.len())?;
            value.push_str(extra);
            return Ok(());
        }
        self.scope.charge_work(length)?;
        let capacity = value
            .capacity()
            .checked_mul(2)
            .map(|capacity| capacity.max(length).max(8))
            .ok_or_else(|| {
                self.scope.record_failure(MemoryError::ArithmeticOverflow);
                MemoryError::ArithmeticOverflow
            })?;
        let mut owners = self.owner_slots(1)?;
        let mut prepared = FundedString::try_with_capacity(&self.scope, family, capacity)?;
        prepared.try_push_str(value)?;
        prepared.try_push_str(extra)?;
        let (prepared, receipt) = prepared.into_parts();
        let retired = std::mem::replace(value, prepared);
        drop(retired);
        if let Some(receipt) = receipt {
            if let Err((receipt, failure)) = owners.push_reserved(receipt) {
                drop(std::mem::take(value));
                drop(receipt);
                return self.fail(failure);
            }
        }
        Ok(())
    }
}

impl fmt::Debug for NativeRegion {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("NativeRegion")
            .finish_non_exhaustive()
    }
}

/// The plain upstream data must be destroyed before its last joint region
/// handle. Read-only borrowing and moving this whole owner preserve that rule.
/// There is deliberately no unguarded data extraction or infallible deep clone.
pub struct NativePayload<T> {
    value: T,
    region: RegionHandle,
}

impl<T> NativePayload<T> {
    pub fn new(value: T, region: RegionHandle) -> Self {
        Self { value, region }
    }
    pub fn region(&self) -> &RegionHandle {
        &self.region
    }
}
impl<T: PlanClone> NativePayload<T> {
    pub fn try_clone_in(&self, region: &RegionHandle, family: Family) -> Result<Self, MemoryError> {
        let value = region.try_clone_recipe(&self.value, family)?;
        Ok(Self::new(value, region.clone()))
    }
}
impl<T> Deref for NativePayload<T> {
    type Target = T;
    fn deref(&self) -> &T {
        &self.value
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::AllocationAuthority;
    use std::{
        alloc::Layout,
        sync::{
            Arc,
            atomic::{AtomicUsize, Ordering},
        },
    };

    struct Authority {
        used: AtomicUsize,
        payload_allocations: AtomicUsize,
    }
    impl AllocationAuthority for Authority {
        fn reserve(&self, layout: Layout, _: Family) -> Result<(), MemoryError> {
            self.used.fetch_add(layout.size(), Ordering::SeqCst);
            Ok(())
        }
        fn release(&self, layout: Layout, _: Family) {
            self.used.fetch_sub(layout.size(), Ordering::SeqCst);
        }
        fn failure(&self) -> Option<MemoryError> {
            None
        }
        fn before_allocate(&self, _: Layout, family: Family) -> Result<(), MemoryError> {
            if family != Family::Metadata {
                self.payload_allocations.fetch_add(1, Ordering::SeqCst);
            }
            Ok(())
        }
    }
    fn scope() -> (MemoryScope, Arc<Authority>) {
        let authority = Arc::new(Authority {
            used: AtomicUsize::new(0),
            payload_allocations: AtomicUsize::new(0),
        });
        (MemoryScope::new(authority.clone()), authority)
    }

    #[test]
    fn full_receipt_region_refuses_before_any_string_allocator() {
        let (scope, authority) = scope();
        let region = NativeRegion::try_new(&scope, 0).unwrap();
        assert_eq!(
            region.try_string("payload", Family::Parse),
            Err(MemoryError::BudgetLimited)
        );
        assert_eq!(authority.payload_allocations.load(Ordering::SeqCst), 0);
        drop(region);
        assert_eq!(authority.used.load(Ordering::SeqCst), 0);
    }

    #[test]
    fn moved_nested_plain_values_keep_real_receipts_until_last_payload_owner() {
        let (scope, authority) = scope();
        let region = NativeRegion::try_new(&scope, 16).unwrap();
        let value = region
            .try_clone_recipe(&vec!["one".to_owned(), "two".to_owned()], Family::Parse)
            .unwrap();
        let payload = NativePayload::new(value, region.clone());
        drop(region);
        assert_eq!(payload.as_slice(), ["one", "two"]);
        assert!(authority.used.load(Ordering::SeqCst) > 0);
        drop(payload);
        assert_eq!(authority.used.load(Ordering::SeqCst), 0);
    }

    #[test]
    fn vector_growth_moves_noncopy_values_and_retains_old_plus_new_receipts() {
        let (scope, authority) = scope();
        let region = NativeRegion::try_new(&scope, 16).unwrap();
        let mut values = Vec::new();
        for n in 0..9 {
            region.try_push(&mut values, n, Family::Parse).unwrap();
        }
        assert_eq!(values, [0, 1, 2, 3, 4, 5, 6, 7, 8]);
        assert_eq!(region.owner_blocks().unwrap(), 3);
        let payload = NativePayload::new(values, region.clone());
        drop(region);
        assert!(authority.used.load(Ordering::SeqCst) > 0);
        drop(payload);
        assert_eq!(authority.used.load(Ordering::SeqCst), 0);
    }

    #[test]
    fn payload_values_drop_while_their_joint_receipts_are_still_charged() {
        struct Probe(Arc<Authority>);
        impl Drop for Probe {
            fn drop(&mut self) {
                assert!(self.0.used.load(Ordering::SeqCst) > 0);
            }
        }
        let (scope, authority) = scope();
        let region = NativeRegion::try_new(&scope, 1).unwrap();
        let payload = NativePayload::new(Probe(authority.clone()), region);
        drop(payload);
        assert_eq!(authority.used.load(Ordering::SeqCst), 0);
    }
}
