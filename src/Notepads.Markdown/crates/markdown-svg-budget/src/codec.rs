// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE file in the project root for license information.
// ---------------------------------------------------------------------------------------------
//! Private storage policy shared by the controlled raster decoders.
//! Ordinary recoverable format errors remain in their decoder's own Copy type.
use crate::{BudgetVec, Error, FailureKind, LimitKind, ResourceKind, ResourcesRef};
use alloc::vec::Vec;
use core::{
    alloc::Layout,
    fmt,
    ops::{Deref, DerefMut},
};

#[derive(Clone, Default)]
pub struct Control {
    resources: Option<ResourcesRef>,
}

impl Control {
    pub const fn plain() -> Self {
        Self { resources: None }
    }
    pub fn paid(resources: ResourcesRef) -> Self {
        Self {
            resources: Some(resources),
        }
    }
    pub fn resources(&self) -> Option<&ResourcesRef> {
        self.resources.as_ref()
    }
    pub fn failure(&self) -> Option<Error> {
        self.resources.as_ref().and_then(ResourcesRef::failure)
    }
    pub fn step(&self, work: u64) -> Result<(), Error> {
        match &self.resources {
            Some(resources) => resources.checkpoint(work),
            None => Ok(()),
        }
    }
    /// Record a hard resource error, or an ordinary format error only AFTER
    /// its decoder has decided to terminate, before the first related cleanup.
    pub fn terminal(&self, error: Error) -> Error {
        self.resources.as_ref().map_or(error, |r| r.record(error))
    }
}

enum Storage<T> {
    Plain(Vec<T>),
    Paid(BudgetVec<T>),
}

/// No implicit Clone/Default and no mutable growable-storage escape.
pub struct Buffer<T> {
    storage: Storage<T>,
}

impl<T> Buffer<T> {
    pub fn new(control: &Control) -> Self {
        Self {
            storage: match control.resources() {
                Some(resources) => {
                    Storage::Paid(BudgetVec::new(resources.clone(), ResourceKind::Scratch))
                }
                None => Storage::Plain(Vec::new()),
            },
        }
    }
    fn control(&self) -> Control {
        match &self.storage {
            Storage::Plain(_) => Control::plain(),
            Storage::Paid(values) => Control::paid(values.resources.clone()),
        }
    }
    pub fn len(&self) -> usize {
        self.deref().len()
    }
    pub fn is_empty(&self) -> bool {
        self.deref().is_empty()
    }
    pub fn capacity(&self) -> usize {
        match &self.storage {
            Storage::Plain(values) => values.capacity(),
            Storage::Paid(values) => values.capacity(),
        }
    }
    fn reserve(&mut self, additional: usize, exact: bool) -> Result<(), Error> {
        let control = self.control();
        control.step(0)?;
        let required = self
            .len()
            .checked_add(additional)
            .ok_or_else(|| control.terminal(Error::new(FailureKind::Quota, LimitKind::Capacity)))?;
        Layout::array::<T>(required)
            .map_err(|_| control.terminal(Error::new(FailureKind::Quota, LimitKind::Capacity)))?;
        match &mut self.storage {
            Storage::Plain(values) => {
                let result = if exact {
                    values.try_reserve_exact(additional)
                } else {
                    values.try_reserve(additional)
                };
                result.map_err(|_| Error::new(FailureKind::Allocation, LimitKind::Capacity))
            }
            Storage::Paid(values) => {
                if exact {
                    values.try_reserve_exact(additional)
                } else {
                    values.try_reserve(additional)
                }
            }
        }
    }
    pub fn try_reserve(&mut self, additional: usize) -> Result<(), Error> {
        self.reserve(additional, false)
    }
    pub fn try_reserve_exact(&mut self, additional: usize) -> Result<(), Error> {
        self.reserve(additional, true)
    }
    /// Safe Vec::push cannot allocate when capacity is sufficient. Use this
    /// only after the whole batch has reserved its exact initialized extent.
    fn push_reserved(&mut self, value: T) -> Result<(), Error> {
        if self.len() == self.capacity() {
            return Err(self
                .control()
                .terminal(Error::new(FailureKind::Invariant, LimitKind::Capacity)));
        }
        match &mut self.storage {
            Storage::Plain(values) => values.push(value),
            Storage::Paid(values) => values.values.push(value),
        }
        Ok(())
    }
    pub fn try_push(&mut self, value: T) -> Result<(), Error> {
        self.try_reserve(1)?;
        self.push_reserved(value)
    }
    pub fn pop(&mut self) -> Option<T> {
        match &mut self.storage {
            Storage::Plain(values) => values.pop(),
            Storage::Paid(values) => values.pop(),
        }
    }
    pub fn truncate(&mut self, len: usize) {
        match &mut self.storage {
            Storage::Plain(values) => values.truncate(len),
            Storage::Paid(values) => values.values.truncate(len),
        }
    }
    pub fn clear(&mut self) {
        self.truncate(0);
    }
    /// Refund only after storage has physically freed. The representation and
    /// request owner remain unchanged, including for a subsequent reuse.
    pub fn release_storage(&mut self) {
        match &mut self.storage {
            Storage::Plain(values) => drop(core::mem::take(values)),
            Storage::Paid(values) => values.release_storage(),
        }
    }
    /// `make` performs resource-only construction. If it owns temporary paid
    /// values, it must record its resource error BEFORE those locals unwind.
    pub fn try_resize_with(
        &mut self,
        len: usize,
        mut make: impl FnMut(&Control) -> Result<T, Error>,
    ) -> Result<(), Error> {
        let control = self.control();
        control.step(0)?;
        let original = self.len();
        if len <= original {
            self.truncate(len);
            return control.step(0);
        }
        let result = (|| {
            self.try_reserve_exact(len - original)?;
            while self.len() < len {
                let count = batch_size::<T>().min(len - self.len());
                control.step(batch_work::<T>(count))?;
                for _ in 0..count {
                    let value = make(&control).map_err(|e| control.terminal(e))?;
                    self.push_reserved(value)?;
                }
            }
            control.step(0)
        })();
        if result.is_err() {
            self.truncate(original);
        }
        result
    }
    /// Clone nested owners explicitly; no T::clone call is hidden here.
    /// The callback obeys the same resource-only construction rule as resize.
    pub fn try_clone_with(
        &self,
        mut make: impl FnMut(&T, &Control) -> Result<T, Error>,
    ) -> Result<Self, Error> {
        let control = self.control();
        let mut result = Self::new(&control);
        result.try_reserve_exact(self.len())?;
        for part in self.chunks(batch_size::<T>()) {
            control.step(batch_work::<T>(part.len()))?;
            for value in part {
                result.push_reserved(make(value, &control).map_err(|e| control.terminal(e))?)?;
            }
        }
        control.step(0)?;
        Ok(result)
    }
    /// Preserve old immutable Vec getters only for actual Plain storage.
    pub fn as_plain_vec(&self) -> Option<&Vec<T>> {
        match &self.storage {
            Storage::Plain(values) => Some(values),
            Storage::Paid(_) => None,
        }
    }
    /// A rejected Paid conversion returns the entire owner without a refund,
    /// allocation, copy or failure mutation.
    pub fn try_into_plain(self) -> Result<Vec<T>, Self> {
        match self.storage {
            Storage::Plain(values) => Ok(values),
            Storage::Paid(values) => Err(Self {
                storage: Storage::Paid(values),
            }),
        }
    }
}

impl<T: Copy> Buffer<T> {
    pub fn try_resize(&mut self, len: usize, value: T) -> Result<(), Error> {
        self.try_resize_with(len, |_| Ok(value))
    }
    pub fn try_extend_from_slice(&mut self, values: &[T]) -> Result<(), Error> {
        let control = self.control();
        let original = self.len();
        let result = (|| {
            self.try_reserve(values.len())?;
            for part in values.chunks(batch_size::<T>()) {
                control.step(batch_work::<T>(part.len()))?;
                for &value in part {
                    self.push_reserved(value)?;
                }
            }
            control.step(0)
        })();
        if result.is_err() {
            self.truncate(original);
        }
        result
    }
    pub fn try_clone(&self) -> Result<Self, Error> {
        self.try_clone_with(|value, _| Ok(*value))
    }
}

fn batch_size<T>() -> usize {
    (65_536 / size_of::<T>().max(1)).clamp(1, 1024)
}
fn batch_work<T>(count: usize) -> u64 {
    // For non-ZSTs, the previously checked/reserved Layout bounds this product.
    (count * size_of::<T>().max(1)) as u64
}

impl<T> Deref for Buffer<T> {
    type Target = [T];
    fn deref(&self) -> &[T] {
        match &self.storage {
            Storage::Plain(values) => values,
            Storage::Paid(values) => values,
        }
    }
}
impl<T> DerefMut for Buffer<T> {
    fn deref_mut(&mut self) -> &mut [T] {
        match &mut self.storage {
            Storage::Plain(values) => values,
            Storage::Paid(values) => values,
        }
    }
}
impl<T: fmt::Debug> fmt::Debug for Buffer<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.deref().fmt(f)
    }
}

/// One heap object, with its actual allocation admitted before nested owners
/// are constructed. It exposes neither the Vec nor its allocation lease.
pub struct One<T> {
    values: Buffer<T>,
}
impl<T> One<T> {
    pub fn try_new(
        control: &Control,
        make: impl FnOnce(&Control) -> Result<T, Error>,
    ) -> Result<Self, Error> {
        let mut values = Buffer::new(control);
        values.try_reserve_exact(1)?;
        let value = make(control).map_err(|e| control.terminal(e))?;
        values.push_reserved(value)?;
        control.step(0)?;
        Ok(Self { values })
    }
}
impl<T> Deref for One<T> {
    type Target = T;
    fn deref(&self) -> &T {
        &self.values[0]
    }
}
impl<T> DerefMut for One<T> {
    fn deref_mut(&mut self) -> &mut T {
        &mut self.values[0]
    }
}
impl<T: fmt::Debug> fmt::Debug for One<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.deref().fmt(f)
    }
}
