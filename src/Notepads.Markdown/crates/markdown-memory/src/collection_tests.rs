// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE file in the project root for license information.
// ---------------------------------------------------------------------------------------------

use super::*;
use std::sync::atomic::{AtomicUsize, Ordering};

struct Observed {
    used: AtomicUsize,
    allocations: AtomicUsize,
    limit: usize,
    fail_site: usize,
}

impl AllocationAuthority for Observed {
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
    fn before_allocate(&self, _: Layout, _: Family) -> Result<(), MemoryError> {
        let site = self.allocations.fetch_add(1, Ordering::SeqCst) + 1;
        if site == self.fail_site {
            Err(MemoryError::OutOfMemory)
        } else {
            Ok(())
        }
    }
}

fn observed(limit: usize, fail_site: usize) -> (Arc<Observed>, MemoryScope) {
    let state = Arc::new(Observed {
        used: AtomicUsize::new(0),
        allocations: AtomicUsize::new(0),
        limit,
        fail_site,
    });
    let scope = MemoryScope::new(state.clone());
    (state, scope)
}

#[test]
fn rejected_growth_keeps_real_vector_and_does_not_invoke_allocator() {
    let (state, scope) = observed(40, 0);
    let mut data = FundedVec::try_with_capacity(&scope, Family::Graph, 4).unwrap();
    for value in [1u64, 2, 3, 4] {
        data.push_reserved(value).unwrap();
    }
    assert_eq!(data.try_push(5), Err(MemoryError::BudgetLimited));
    assert_eq!(data.as_slice(), &[1, 2, 3, 4]);
    assert_eq!(data.capacity(), 4);
    assert_eq!(state.allocations.load(Ordering::SeqCst), 1);
    assert_eq!(state.used.load(Ordering::SeqCst), 32);
    drop(data);
    assert_eq!(state.used.load(Ordering::SeqCst), 0);
}

#[test]
fn all_group_admission_precedes_first_actual_buffer_allocation() {
    let (state, scope) = observed(95, 0);
    let result = (|| {
        let first = scope.reserve_array::<u64>(8, Family::Parse)?;
        let second = scope.reserve_array::<u32>(8, Family::Parse)?;
        let first = FundedVec::<u64>::try_with_prepared(first, 8)?;
        let second = FundedVec::<u32>::try_with_prepared(second, 8)?;
        Ok::<_, MemoryError>((first, second))
    })();
    assert!(matches!(result, Err(MemoryError::BudgetLimited)));
    assert_eq!(state.allocations.load(Ordering::SeqCst), 0);
    assert_eq!(state.used.load(Ordering::SeqCst), 0);
}

#[test]
fn second_group_allocation_failure_frees_materialized_first_buffer_and_both_receipts() {
    let (state, scope) = observed(96, 2);
    let result = (|| {
        let first = scope.reserve_array::<u64>(8, Family::Parse)?;
        let second = scope.reserve_array::<u32>(8, Family::Parse)?;
        let first = FundedVec::<u64>::try_with_prepared(first, 8)?;
        let second = FundedVec::<u32>::try_with_prepared(second, 8)?;
        Ok::<_, MemoryError>((first, second))
    })();
    assert!(matches!(result, Err(MemoryError::OutOfMemory)));
    assert_eq!(state.allocations.load(Ordering::SeqCst), 2);
    assert_eq!(state.used.load(Ordering::SeqCst), 0);
}

#[test]
fn nested_clone_rejection_preserves_old_strings_and_releases_partial_destination() {
    let (old_state, old_scope) = observed(4096, 0);
    let mut old = FundedVec::try_with_capacity(&old_scope, Family::Model, 2).unwrap();
    for text in ["abcdefgh", "ijklmnop"] {
        old.push_reserved(FundedString::try_from_str(&old_scope, Family::Model, text).unwrap())
            .unwrap();
    }
    let old_bytes = old_state.used.load(Ordering::SeqCst);
    let (new_state, new_scope) = observed(2 * size_of::<FundedString>() + 8, 0);
    assert!(matches!(
        old.try_clone_in(&new_scope, Family::Model),
        Err(MemoryError::BudgetLimited)
    ));
    assert_eq!(old[0].as_str(), "abcdefgh");
    assert_eq!(old[1].as_str(), "ijklmnop");
    assert_eq!(old_state.used.load(Ordering::SeqCst), old_bytes);
    assert_eq!(new_state.used.load(Ordering::SeqCst), 0);
    drop(old);
    assert_eq!(old_state.used.load(Ordering::SeqCst), 0);
}

#[test]
fn string_rejection_preserves_valid_utf8_and_raw_parts_hold_the_real_owner() {
    let (state, scope) = observed(20, 0);
    let mut text = FundedString::try_from_str(&scope, Family::Text, "中文🦀").unwrap();
    assert_eq!(text.try_push('x'), Err(MemoryError::BudgetLimited));
    assert_eq!(text.as_str(), "中文🦀");
    let (text, owner) = text.into_parts();
    assert_eq!(state.used.load(Ordering::SeqCst), 10);
    drop(text);
    assert_eq!(state.used.load(Ordering::SeqCst), 10);
    drop(owner);
    assert_eq!(state.used.load(Ordering::SeqCst), 0);
}
