// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE file in the project root for license information.
// ---------------------------------------------------------------------------------------------

//! A single inline record, or one explicitly admitted backing and physical ID.
use super::{
    Host, MappingAccess, NativeViolation, WorkDelta,
    allocation::{AllocationId, Registry},
};
use smallvec::SmallVec;

#[derive(Debug)]
pub(super) struct SmallValue<T: Copy> {
    values: SmallVec<[T; 1]>,
    id: Option<AllocationId>,
}
impl<T: Copy> Default for SmallValue<T> {
    fn default() -> Self {
        Self {
            values: SmallVec::new(),
            id: None,
        }
    }
}
impl<T: Copy> std::ops::Deref for SmallValue<T> {
    type Target = [T];
    fn deref(&self) -> &[T] {
        &self.values
    }
}
impl<T: Copy> std::ops::DerefMut for SmallValue<T> {
    fn deref_mut(&mut self) -> &mut [T] {
        &mut self.values
    }
}
impl<'a, T: Copy> IntoIterator for &'a SmallValue<T> {
    type Item = &'a T;
    type IntoIter = std::slice::Iter<'a, T>;
    fn into_iter(self) -> Self::IntoIter {
        self.values.iter()
    }
}
impl<T: Copy> SmallValue<T> {
    pub(super) fn new() -> Self {
        Self::default()
    }
    /// Moves the *whole* owner, including its ID; never extracts only backing.
    pub(super) fn take(&mut self) -> Self {
        std::mem::take(self)
    }
    pub(super) fn truncate(&mut self, len: usize) {
        self.values.truncate(len);
    }
    pub(super) fn ensure<H: Host>(
        &mut self,
        additional: usize,
        ctx: &mut MappingAccess<'_, H>,
    ) -> Result<(), H::Error> {
        ctx.registry.mutable(ctx.host)?;
        let needed = self
            .len()
            .checked_add(additional)
            .ok_or_else(|| ctx.host.reject(NativeViolation::ArithmeticOverflow))?;
        if needed <= self.values.capacity() {
            return Ok(());
        }
        let capacity = self
            .values
            .capacity()
            .checked_mul(2)
            .map(|n| n.max(needed))
            .ok_or_else(|| ctx.host.reject(NativeViolation::ArithmeticOverflow))?;
        let size = std::mem::size_of::<T>();
        let bytes = capacity
            .checked_mul(size)
            .ok_or_else(|| ctx.host.reject(NativeViolation::ArithmeticOverflow))?;
        if size == 0 {
            return Err(ctx.host.reject(NativeViolation::InvalidProvenance));
        }
        ctx.registry.prepare(false, ctx.host)?;
        let mut lease = ctx.host.reserve(bytes as u64)?;
        let mut values = SmallVec::<[T; 1]>::new();
        values
            .try_reserve_exact(capacity)
            .map_err(|_| ctx.host.reject(NativeViolation::OutOfMemory))?;
        ctx.host
            .admit_actual(&mut lease, (values.capacity() * size) as u64)?;
        let id = ctx
            .registry
            .retain(values.as_ptr() as usize, lease, false, ctx.host)?;
        for value in &self.values {
            ctx.host.work(WorkDelta::Copy(size as u64))?;
            values.push(*value);
        }
        let old = std::mem::replace(
            self,
            Self {
                values,
                id: Some(id),
            },
        );
        old.discard(ctx)
    }
    pub(super) fn push<H: Host>(
        &mut self,
        value: T,
        ctx: &mut MappingAccess<'_, H>,
    ) -> Result<(), H::Error> {
        ctx.event()?;
        self.ensure(1, ctx)?;
        ctx.host
            .work(WorkDelta::Copy(std::mem::size_of::<T>() as u64))?;
        self.values.push(value);
        Ok(())
    }
    /// Append only real initialized records after explicit capacity admission.
    pub(super) fn extend_from_slice<H: Host>(
        &mut self,
        values: &[T],
        ctx: &mut MappingAccess<'_, H>,
    ) -> Result<(), H::Error> {
        self.ensure(values.len(), ctx)?;
        let size = std::mem::size_of::<T>();
        if size == 0 || size > 65_536 {
            return Err(ctx.host.reject(NativeViolation::InvalidProvenance));
        }
        // Empty replacement still performs the original mutation checkpoint.
        if values.is_empty() {
            ctx.event()?;
        }
        for chunk in values.chunks(65_536 / size) {
            ctx.event()?;
            ctx.host
                .work(WorkDelta::Copy(std::mem::size_of_val(chunk) as u64))?;
            self.values.extend_from_slice(chunk);
        }
        Ok(())
    }

    pub(super) fn discard<H: Host>(self, ctx: &mut MappingAccess<'_, H>) -> Result<(), H::Error> {
        let Self { values, id } = self;
        let pointer = values.as_ptr() as usize;
        let spilled = values.spilled();
        drop(values);
        match (spilled, id) {
            (true, Some(id)) => ctx.registry.release(id, pointer, ctx.host),
            (false, None) => Ok(()),
            _ => Err(ctx.host.reject(NativeViolation::InvalidProvenance)),
        }
    }
    pub(super) fn mark<H: Host>(
        &self,
        registry: &mut Registry<H::Lease>,
        host: &mut H,
    ) -> Result<(), H::Error> {
        let Self { values, id } = self;
        match (values.spilled(), *id) {
            (true, Some(id)) => registry.mark(id, values.as_ptr() as usize, host),
            (false, None) => Ok(()),
            _ => Err(host.reject(NativeViolation::InvalidProvenance)),
        }
    }
}
