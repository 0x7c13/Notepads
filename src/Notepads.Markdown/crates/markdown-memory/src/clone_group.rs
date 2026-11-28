// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE file in the project root for license information.
// ---------------------------------------------------------------------------------------------

//! A complete nested clone plan shared by parser, graph and emission. All payload tickets are acquired before the
//! first payload allocator call. Planning/storage themselves use the same root.

use crate::{Family, FundedVec, GrowthTicket, MemoryError, MemoryScope, OwnerTicket};
use std::{alloc::Layout, ops::Deref};

pub trait PlanClone: Sized {
    fn plan_clone(&self, plan: &mut ClonePlan) -> Result<(), MemoryError>;
    fn clone_prepared(&self, group: &mut CloneGroup) -> Result<Self, MemoryError>;
}

pub struct ClonePlan {
    layouts: FundedVec<Layout>,
    family: Family,
    depth: usize,
}

impl ClonePlan {
    pub fn new(scope: MemoryScope, family: Family) -> Self {
        Self { layouts: FundedVec::new_in(scope, Family::Metadata), family, depth: 0 }
    }

    pub fn scope(&self) -> &MemoryScope { self.layouts.scope() }
    pub fn blocks(&self) -> usize { self.layouts.len() }
    pub fn family(&self) -> Family { self.family }

    pub fn array<T>(&mut self, count: usize) -> Result<(), MemoryError> {
        // Bound both the read-only planning traversal and the later element
        // copy/move by the same operation's cumulative work authority.
        let work = count.checked_mul(2).and_then(|work| work.checked_add(1))
            .ok_or(MemoryError::ArithmeticOverflow)?;
        self.scope().charge_work(work)?;
        let layout = Layout::array::<T>(count).map_err(|_| MemoryError::ArithmeticOverflow)?;
        self.layout(layout)
    }

    pub fn layout(&mut self, layout: Layout) -> Result<(), MemoryError> { self.layouts.try_push(layout) }

    pub fn enter(&mut self) -> Result<(), MemoryError> {
        self.scope().charge_work(1)?;
        if self.depth >= 32 { return Err(MemoryError::BudgetLimited); }
        self.depth += 1;
        Ok(())
    }

    pub fn leave(&mut self) { self.depth -= 1; }

    pub fn prepare(self) -> Result<CloneGroup, MemoryError> {
        let scope = self.layouts.scope().clone();
        // These two metadata buffers are paid before allocation as well. They
        // contain no cloned payload and may safely be freed on group rejection.
        let ticket_layout = Layout::array::<GrowthTicket>(self.layouts.len())
            .map_err(|_| MemoryError::ArithmeticOverflow)?;
        let owner_layout = Layout::array::<OwnerTicket>(self.layouts.len())
            .map_err(|_| MemoryError::ArithmeticOverflow)?;
        let ticket_storage = scope.reserve(ticket_layout, Family::Metadata)?;
        let owner_storage = scope.reserve(owner_layout, Family::Metadata)?;
        let mut tickets = FundedVec::try_with_prepared(ticket_storage, self.layouts.len())?;
        let owners = FundedVec::try_with_prepared(owner_storage, self.layouts.len())?;
        for layout in &self.layouts {
            let ticket = scope.reserve(*layout, self.family)?;
            // Complete metadata capacity was prepared above; on an impossible
            // mismatch the still-unmaterialized receipt can be returned safely.
            tickets.push_reserved(ticket).map_err(|(_, failure)| failure)?;
        }
        tickets.reverse();
        Ok(CloneGroup { tickets, owners, scope, family: self.family })
    }
}

pub struct CloneGroup {
    tickets: FundedVec<GrowthTicket>,
    owners: FundedVec<OwnerTicket>,
    scope: MemoryScope,
    family: Family,
}

impl CloneGroup {
    pub fn take_array<T>(&mut self, count: usize) -> Result<GrowthTicket, MemoryError> {
        let layout = Layout::array::<T>(count).map_err(|_| MemoryError::ArithmeticOverflow)?;
        self.take_layout(layout)
    }

    pub fn scope(&self) -> &MemoryScope { &self.scope }
    pub fn family(&self) -> Family { self.family }

    pub fn take_layout(&mut self, layout: Layout) -> Result<GrowthTicket, MemoryError> {
        let ticket = self.tickets.pop().ok_or(MemoryError::InvalidState)?;
        if ticket.layout() != layout { return Err(MemoryError::InvalidState); }
        Ok(ticket)
    }

    /// Attaches an already materialized plain container to the joint payload
    /// owner. A rejected receipt is returned, so actual data can be freed first.
    pub fn attach(&mut self, ticket: OwnerTicket) -> Result<(), (OwnerTicket, MemoryError)> {
        self.owners.push_reserved(ticket)
    }

    pub fn vector<T>(&mut self, count: usize) -> Result<Vec<T>, MemoryError> {
        let ticket = self.take_array::<T>(count)?;
        let (values, owner) = FundedVec::try_with_prepared(ticket, count)?.into_parts();
        if let Some(owner) = owner {
            if let Err((owner, failure)) = self.attach(owner) {
                drop(values);
                drop(owner);
                return Err(failure);
            }
        }
        Ok(values)
    }

    pub fn string(&mut self, source: &str) -> Result<String, MemoryError> {
        let mut bytes = self.vector::<u8>(source.len())?;
        bytes.extend_from_slice(source.as_bytes()); // Exact prepared capacity.
        // SAFETY: source is valid UTF-8 and its bytes were copied unchanged.
        Ok(unsafe { String::from_utf8_unchecked(bytes) })
    }

    pub fn finish<T>(self, value: T) -> Result<OwnedPayload<T>, MemoryError> {
        if !self.tickets.is_empty() {
            drop(value); // Real payload disappears before the receipts in self.
            return Err(MemoryError::InvalidState);
        }
        let Self { tickets, owners, scope, .. } = self;
        drop(tickets);
        Ok(OwnedPayload { value, owners, scope })
    }
}


/// The plain upstream data and every nested receipt form one owner. A caller
/// must not extract T while dropping its receipts, nor grow it through std APIs.
pub struct OwnedPayload<T> {
    value: T,
    owners: FundedVec<OwnerTicket>,
    scope: MemoryScope,
}

impl<T: PlanClone> OwnedPayload<T> {
    pub fn try_clone(scope: &MemoryScope, family: Family, source: &T) -> Result<Self, MemoryError> {
        let mut plan = ClonePlan::new(scope.clone(), family);
        source.plan_clone(&mut plan)?;
        let mut group = plan.prepare()?;
        let value = source.clone_prepared(&mut group)?;
        group.finish(value)
    }
}

impl<T> OwnedPayload<T> {
    pub fn scope(&self) -> &MemoryScope { &self.scope }
    pub fn owner_blocks(&self) -> usize { self.owners.len() }

    /// Receipts must be stored after the data or merged into an enclosing graph
    /// owner that outlives it. This is not an untracked std-container conversion.
    pub fn into_parts(self) -> (T, FundedVec<OwnerTicket>) {
        let Self { value, owners, .. } = self;
        (value, owners)
    }
}
impl<T> Deref for OwnedPayload<T> { type Target = T; fn deref(&self) -> &T { &self.value } }

macro_rules! copy_plan {
    ($($kind:ty),* $(,)?) => { $(impl PlanClone for $kind {
        fn plan_clone(&self, _: &mut ClonePlan) -> Result<(), MemoryError> { Ok(()) }
        fn clone_prepared(&self, _: &mut CloneGroup) -> Result<Self, MemoryError> { Ok(*self) }
    })* };
}
copy_plan!((), bool, char, u8, u16, u32, u64, u128, usize, i8, i16, i32, i64, i128, isize, f32, f64);

impl PlanClone for String {
    fn plan_clone(&self, plan: &mut ClonePlan) -> Result<(), MemoryError> { plan.array::<u8>(self.len()) }
    fn clone_prepared(&self, group: &mut CloneGroup) -> Result<Self, MemoryError> { group.string(self) }
}
impl<T: PlanClone> PlanClone for Option<T> {
    fn plan_clone(&self, plan: &mut ClonePlan) -> Result<(), MemoryError> {
        match self { Some(value) => value.plan_clone(plan), None => Ok(()) }
    }
    fn clone_prepared(&self, group: &mut CloneGroup) -> Result<Self, MemoryError> {
        self.as_ref().map(|value| value.clone_prepared(group)).transpose()
    }
}
impl<T: PlanClone> PlanClone for Vec<T> {
    fn plan_clone(&self, plan: &mut ClonePlan) -> Result<(), MemoryError> {
        plan.array::<T>(self.len())?;
        for value in self { value.plan_clone(plan)?; }
        Ok(())
    }
    fn clone_prepared(&self, group: &mut CloneGroup) -> Result<Self, MemoryError> {
        let mut values = group.vector::<T>(self.len())?;
        for value in self { values.push(value.clone_prepared(group)?); }
        Ok(values)
    }
}
impl<A: PlanClone, B: PlanClone> PlanClone for (A, B) {
    fn plan_clone(&self, plan: &mut ClonePlan) -> Result<(), MemoryError> {
        self.0.plan_clone(plan)?;
        self.1.plan_clone(plan)
    }
    fn clone_prepared(&self, group: &mut CloneGroup) -> Result<Self, MemoryError> {
        Ok((self.0.clone_prepared(group)?, self.1.clone_prepared(group)?))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::AllocationAuthority;
    use std::sync::{Arc, atomic::{AtomicUsize, Ordering}};

    struct Authority {
        used: AtomicUsize,
        graph_reserves: AtomicUsize,
        graph_allocates: AtomicUsize,
        fail_reserve: usize,
        fail_allocate: usize,
        expected_reserves: usize,
    }
    impl AllocationAuthority for Authority {
        fn reserve(&self, layout: Layout, family: Family) -> Result<(), MemoryError> {
            if family == Family::Graph {
                let site = self.graph_reserves.fetch_add(1, Ordering::SeqCst) + 1;
                if site == self.fail_reserve { return Err(MemoryError::BudgetLimited); }
            }
            self.used.fetch_add(layout.size(), Ordering::SeqCst);
            Ok(())
        }
        fn release(&self, layout: Layout, _: Family) { self.used.fetch_sub(layout.size(), Ordering::SeqCst); }
        fn failure(&self) -> Option<MemoryError> { None }
        fn before_allocate(&self, _: Layout, family: Family) -> Result<(), MemoryError> {
            if family == Family::Graph {
                assert_eq!(self.graph_reserves.load(Ordering::SeqCst), self.expected_reserves);
                let site = self.graph_allocates.fetch_add(1, Ordering::SeqCst) + 1;
                if site == self.fail_allocate { return Err(MemoryError::OutOfMemory); }
            }
            Ok(())
        }
    }
    fn scope(fail_reserve: usize, fail_allocate: usize) -> (MemoryScope, Arc<Authority>) {
        let authority = Arc::new(Authority {
            used: AtomicUsize::new(0), graph_reserves: AtomicUsize::new(0), graph_allocates: AtomicUsize::new(0),
            fail_reserve, fail_allocate, expected_reserves: 4,
        });
        (MemoryScope::new(authority.clone()), authority)
    }
    fn source() -> Vec<String> { vec!["one".into(), "two".into(), "three".into()] }

    #[test]
    fn nested_group_reserves_every_payload_block_before_the_first_payload_allocation() {
        let (scope, authority) = scope(0, 0);
        let original = source();
        let cloned = OwnedPayload::try_clone(&scope, Family::Graph, &original).unwrap();
        assert_eq!(&*cloned, &original);
        assert_eq!(cloned.owner_blocks(), 4);
        assert!(authority.used.load(Ordering::SeqCst) > 0);
        drop(cloned);
        assert_eq!(authority.used.load(Ordering::SeqCst), 0);
    }

    #[test]
    fn nth_nested_reservation_rejection_never_calls_a_payload_allocator() {
        for site in 1..=4 {
            let (scope, authority) = scope(site, 0);
            let original = source();
            assert!(matches!(OwnedPayload::try_clone(&scope, Family::Graph, &original), Err(MemoryError::BudgetLimited)));
            assert_eq!(original, source());
            assert_eq!(authority.graph_allocates.load(Ordering::SeqCst), 0);
            assert_eq!(authority.used.load(Ordering::SeqCst), 0);
        }
    }

    #[test]
    fn nth_nested_allocator_failure_preserves_original_and_returns_all_real_owners() {
        for site in 1..=4 {
            let (scope, authority) = scope(0, site);
            let original = source();
            assert!(matches!(OwnedPayload::try_clone(&scope, Family::Graph, &original), Err(MemoryError::OutOfMemory)));
            assert_eq!(original, source());
            assert_eq!(authority.used.load(Ordering::SeqCst), 0);
        }
    }
}
