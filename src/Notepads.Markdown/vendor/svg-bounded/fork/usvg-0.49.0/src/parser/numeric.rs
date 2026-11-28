// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE file in the project root for license information.
// ---------------------------------------------------------------------------------------------
//! Mutable numeric/position construction, frozen before semantic publication.
use markdown_svg_budget::{BudgetVec, Error, FrozenList, ResourceKind, ResourcesRef};
use std::ops::{Deref, DerefMut};

pub(crate) enum ListBuilder<T: Copy + 'static> {
    Plain(Vec<T>),
    Paid(BudgetVec<T>),
}
impl<T: Copy + 'static> ListBuilder<T> {
    pub(crate) fn new(resources: Option<&ResourcesRef>) -> Self {
        match resources {
            Some(r) => Self::Paid(BudgetVec::new(r.clone(), ResourceKind::Scratch)),
            None => Self::Plain(Vec::new()),
        }
    }
    pub(crate) fn push(&mut self, value: T) -> Result<(), Error> {
        #[cfg(test)]
        let _phase = tests::NumericPhase::enter(1);
        match self {
            Self::Plain(v) => {
                v.push(value);
                Ok(())
            }
            Self::Paid(v) => v.try_push(value),
        }
    }
    pub(crate) fn reserve_exact(&mut self, count: usize) -> Result<(), Error> {
        #[cfg(test)]
        let _phase = tests::NumericPhase::enter(1);
        match self {
            Self::Plain(v) => {
                v.reserve_exact(count);
                Ok(())
            }
            Self::Paid(v) => v.try_reserve_exact(count),
        }
    }
    pub(crate) fn filled(
        resources: Option<&ResourcesRef>,
        count: usize,
        value: T,
    ) -> Result<Self, Error> {
        let mut output = Self::new(resources);
        output.reserve_exact(count)?;
        for index in 0..count {
            if index % 256 == 0 {
                step(resources, (count - index).min(256))?;
            }
            output.push(value)?;
        }
        Ok(output)
    }
    pub(crate) fn freeze(self, resources: Option<&ResourcesRef>) -> Result<FrozenList<T>, Error> {
        #[cfg(test)]
        let _phase = tests::NumericPhase::enter(2);
        match self {
            Self::Plain(v) => Ok(v.into()),
            Self::Paid(v) => FrozenList::try_freeze(resources.expect("paid numeric owner"), v),
        }
    }
}
impl<T: Copy + 'static> Deref for ListBuilder<T> {
    type Target = [T];
    fn deref(&self) -> &[T] {
        match self {
            Self::Plain(v) => v,
            Self::Paid(v) => v,
        }
    }
}
impl<T: Copy + 'static> DerefMut for ListBuilder<T> {
    fn deref_mut(&mut self) -> &mut [T] {
        match self {
            Self::Plain(v) => v,
            Self::Paid(v) => v,
        }
    }
}
pub(crate) fn step(resources: Option<&ResourcesRef>, work: usize) -> Result<(), Error> {
    if let Some(r) = resources {
        r.checkpoint(work as u64)?;
    }
    Ok(())
}
pub(crate) fn checked(resources: Option<&ResourcesRef>) -> Result<(), Error> {
    if let Some(error) = resources.and_then(ResourcesRef::failure) {
        Err(error)
    } else {
        Ok(())
    }
}

#[cfg(test)]
#[path = "numeric_tests.rs"]
pub(super) mod tests;
