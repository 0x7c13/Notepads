// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE file in the project root for license information.
// ---------------------------------------------------------------------------------------------
//! Read-only list ownership shared by semantic and geometry consumers.
use crate::{BudgetVec, Error, FailureKind, LimitKind, PaidShared, ResourceKind, ResourcesRef};
use alloc::vec::Vec;
use core::{fmt, ops::Deref};

enum Storage<T: Copy + 'static> {
    Compatibility(Vec<T>),
    Paid(PaidShared<BudgetVec<T>>),
    Static(&'static [T]),
}

#[cfg(test)]
pub(crate) fn mismatched_owner_for_test<T: Copy + 'static>(
    header: &ResourcesRef,
    buffer: BudgetVec<T>,
) -> Result<FrozenList<T>, Error> {
    // An intentionally inconsistent private fixture: real allocations still
    // have their original payers and destructors. No production constructor
    // permits this header/payload mismatch.
    Ok(FrozenList {
        storage: Storage::Paid(PaidShared::try_new(header, ResourceKind::Scratch, buffer)?),
    })
}

/// Immutable lists retain the real allocation fee until the last reader frees
/// the payload. Only the compatibility branch performs an ordinary deep clone.
pub struct FrozenList<T: Copy + 'static> {
    storage: Storage<T>,
}
impl<T: Copy + 'static> FrozenList<T> {
    pub fn from_compat(values: Vec<T>) -> Self {
        Self {
            storage: Storage::Compatibility(values),
        }
    }
    pub const fn from_static(values: &'static [T]) -> Self {
        Self {
            storage: Storage::Static(values),
        }
    }
    pub fn try_freeze(resources: &ResourcesRef, values: BudgetVec<T>) -> Result<Self, Error> {
        if !values.belongs_to(resources) {
            return Err(resources.record(Error::new(FailureKind::Invariant, LimitKind::None)));
        }
        let owner = PaidShared::try_new(resources, ResourceKind::Scratch, values)?;
        Ok(Self {
            storage: Storage::Paid(owner),
        })
    }
    pub fn as_slice(&self) -> &[T] {
        match &self.storage {
            Storage::Compatibility(values) => values,
            Storage::Paid(values) => values.as_ref(),
            Storage::Static(values) => values,
        }
    }
    pub fn is_static(&self) -> bool {
        matches!(self.storage, Storage::Static(_))
    }
    /// Validate both the shared header and its real buffer. Static borrowed
    /// lists have no allocation; compatibility lists cannot become paid by
    /// being borrowed from a request. Existing faults win over this mismatch.
    pub fn validate_policy(&self, target: Option<&ResourcesRef>) -> Result<(), Error> {
        let sources = match &self.storage {
            Storage::Static(_) => [None, None],
            Storage::Compatibility(_) => {
                return match target {
                    None => Ok(()),
                    Some(target) => {
                        Err(target.record(Error::new(FailureKind::Invariant, LimitKind::None)))
                    }
                };
            }
            Storage::Paid(owner) => [Some(owner.resources()), Some(owner.as_ref().resources())],
        };
        let existing = target
            .and_then(ResourcesRef::failure)
            .or_else(|| sources.iter().flatten().find_map(|source| source.failure()));
        let mismatch = sources
            .iter()
            .flatten()
            .any(|source| target.is_none_or(|target| !source.is_same_request(target)));
        if mismatch || existing.is_some() {
            let invariant = Error::new(FailureKind::Invariant, LimitKind::None);
            // Capture all pre-existing records before setting any new slot.
            // This ordering preserves a source HRESULT when the target was
            // still clean; independent accounts have no global event order.
            if mismatch {
                for source in sources.iter().flatten() {
                    source.record(invariant);
                }
            }
            let error = existing.unwrap_or(invariant);
            return Err(target.map_or(error, |target| target.record(error)));
        }
        Ok(())
    }
    pub fn belongs_to(&self, resources: &ResourcesRef) -> bool {
        match &self.storage {
            Storage::Paid(values) => values.as_ref().belongs_to(resources),
            _ => false,
        }
    }
    pub fn retained_unpaid_capacity(&self) -> usize {
        match &self.storage {
            Storage::Compatibility(values) => values.capacity(),
            _ => 0,
        }
    }
}
impl<T: Copy + 'static> Clone for FrozenList<T> {
    fn clone(&self) -> Self {
        let storage = match &self.storage {
            Storage::Compatibility(values) => Storage::Compatibility(values.clone()),
            Storage::Paid(values) => Storage::Paid(values.clone()),
            Storage::Static(values) => Storage::Static(values),
        };
        Self { storage }
    }
}
impl<T: Copy + 'static> From<Vec<T>> for FrozenList<T> {
    fn from(values: Vec<T>) -> Self {
        Self::from_compat(values)
    }
}
impl<T: Copy + 'static> Deref for FrozenList<T> {
    type Target = [T];
    fn deref(&self) -> &Self::Target {
        self.as_slice()
    }
}
impl<T: Copy + 'static> AsRef<[T]> for FrozenList<T> {
    fn as_ref(&self) -> &[T] {
        self.as_slice()
    }
}
impl<T: Copy + PartialEq + 'static> PartialEq for FrozenList<T> {
    fn eq(&self, other: &Self) -> bool {
        self.len() == other.len() && self.iter().zip(other.iter()).all(|(a, b)| a == b)
    }
}
impl<T: Copy + Eq + 'static> Eq for FrozenList<T> {}
impl<T: Copy + 'static> fmt::Debug for FrozenList<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("FrozenList")
            .field("length", &self.len())
            .field("paid", &matches!(self.storage, Storage::Paid(_)))
            .finish()
    }
}
