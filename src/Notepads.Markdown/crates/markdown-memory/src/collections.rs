// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE file in the project root for license information.
// ---------------------------------------------------------------------------------------------

//! Small explicit-owner collections for the native flowchart execution path.
//! There is deliberately no infallible growth or implicit allocator context.

use crate::{Family, GrowthTicket, MemoryError, MemoryScope, OwnerTicket};
use std::{
    alloc::Layout,
    borrow::Borrow,
    fmt,
    hash::{Hash, Hasher},
    ops::{Deref, DerefMut},
};

pub trait BoundedClone: Sized {
    fn try_clone_in(&self, scope: &MemoryScope, family: Family) -> Result<Self, MemoryError>;
}

macro_rules! copy_clone {
    ($($kind:ty),* $(,)?) => { $(impl BoundedClone for $kind {
        fn try_clone_in(&self, _: &MemoryScope, _: Family) -> Result<Self, MemoryError> { Ok(*self) }
    })* };
}
copy_clone!(
    bool,
    char,
    u8,
    u16,
    u32,
    u64,
    u128,
    usize,
    i8,
    i16,
    i32,
    i64,
    i128,
    isize,
    f32,
    f64,
    ()
);

impl<T: BoundedClone> BoundedClone for Option<T> {
    fn try_clone_in(&self, scope: &MemoryScope, family: Family) -> Result<Self, MemoryError> {
        self.as_ref()
            .map(|value| value.try_clone_in(scope, family))
            .transpose()
    }
}

impl<A: BoundedClone, B: BoundedClone> BoundedClone for (A, B) {
    fn try_clone_in(&self, scope: &MemoryScope, family: Family) -> Result<Self, MemoryError> {
        Ok((
            self.0.try_clone_in(scope, family)?,
            self.1.try_clone_in(scope, family)?,
        ))
    }
}

pub struct FundedVec<T> {
    // Rust drops fields in this order: data's actual allocation before receipt.
    values: Vec<T>,
    ticket: Option<OwnerTicket>,
    scope: MemoryScope,
    family: Family,
}

impl<T> FundedVec<T> {
    pub fn new_in(scope: MemoryScope, family: Family) -> Self {
        Self {
            values: Vec::new(),
            ticket: None,
            scope,
            family,
        }
    }

    pub fn try_with_capacity(
        scope: &MemoryScope,
        family: Family,
        capacity: usize,
    ) -> Result<Self, MemoryError> {
        let layout = Layout::array::<T>(capacity).map_err(|_| MemoryError::ArithmeticOverflow)?;
        Self::try_with_prepared(scope.reserve(layout, family)?, capacity)
    }

    /// Materializes an already reserved exact block. This allows every ticket
    /// in a multi-buffer group to be acquired before the first allocator call.
    pub fn try_with_prepared(ticket: GrowthTicket, capacity: usize) -> Result<Self, MemoryError> {
        let layout = Layout::array::<T>(capacity).map_err(|_| MemoryError::ArithmeticOverflow)?;
        if ticket.layout() != layout {
            return Err(MemoryError::InvalidState);
        }
        let scope = ticket.scope().clone();
        let family = ticket.family();
        if layout.size() == 0 {
            drop(ticket);
            return Ok(Self::new_in(scope, family));
        }
        let values = {
            scope.before_allocate(layout, family)?;
            // SAFETY: this exact validated Layout is completely reserved. It
            // uses Global's alloc/dealloc pair, matching Vec::from_raw_parts.
            let pointer = unsafe { std::alloc::alloc(layout) }.cast::<T>();
            if pointer.is_null() {
                scope.record_failure(MemoryError::OutOfMemory);
                return Err(MemoryError::OutOfMemory);
            }
            // SAFETY: unique allocation for capacity T, currently empty.
            unsafe { Vec::from_raw_parts(pointer, 0, capacity) }
        };
        Ok(Self {
            values,
            ticket: Some(ticket.commit()),
            scope,
            family,
        })
    }

    fn overflow(&self) -> MemoryError {
        self.scope.record_failure(MemoryError::ArithmeticOverflow);
        MemoryError::ArithmeticOverflow
    }

    fn grow(&mut self, capacity: usize) -> Result<(), MemoryError> {
        if capacity <= self.values.capacity() {
            return Ok(());
        }
        let layout = Layout::array::<T>(capacity).map_err(|_| self.overflow())?;
        if layout.size() == 0 {
            return Ok(());
        }
        // The new backing moves every live element as well as admitting the
        // full old/new overlap. Charge that actual work before allocating.
        self.scope.charge_work(self.values.len())?;
        let ticket = self.scope.reserve(layout, self.family)?;
        let (mut replacement, next_ticket) =
            Self::try_with_prepared(ticket, capacity)?.into_parts();
        replacement.append(&mut self.values);
        let old = std::mem::replace(&mut self.values, replacement);
        let old_ticket = std::mem::replace(&mut self.ticket, next_ticket);
        drop(old); // True deallocation precedes the old receipt's return.
        drop(old_ticket);
        Ok(())
    }

    pub fn try_reserve_exact(&mut self, additional: usize) -> Result<(), MemoryError> {
        let needed = self
            .len()
            .checked_add(additional)
            .ok_or_else(|| self.overflow())?;
        self.grow(needed)
    }

    pub fn try_reserve(&mut self, additional: usize) -> Result<(), MemoryError> {
        let needed = self
            .len()
            .checked_add(additional)
            .ok_or_else(|| self.overflow())?;
        if needed <= self.capacity() {
            return Ok(());
        }
        let doubled = self.capacity().checked_mul(2).unwrap_or(needed);
        self.grow(needed.max(doubled).max(4))
    }

    pub fn try_push(&mut self, value: T) -> Result<(), MemoryError> {
        self.try_reserve(1)?;
        self.values.push(value); // Capacity was proved above; cannot allocate.
        Ok(())
    }

    /// No allocation and no destruction of a rejected value. Useful when the
    /// value is a receipt for a separately owned block being atomically linked.
    pub fn push_reserved(&mut self, value: T) -> Result<(), (T, MemoryError)> {
        if self.len() == self.capacity() {
            return Err((value, MemoryError::InvalidState));
        }
        self.values.push(value);
        Ok(())
    }

    pub fn try_insert(&mut self, index: usize, value: T) -> Result<(), MemoryError> {
        if index > self.len() {
            return Err(MemoryError::InvalidState);
        }
        self.try_reserve(1)?;
        self.values.insert(index, value);
        Ok(())
    }

    pub fn try_extend_from_slice(&mut self, source: &[T]) -> Result<(), MemoryError>
    where
        T: Copy,
    {
        self.try_reserve(source.len())?;
        self.values.extend_from_slice(source);
        Ok(())
    }

    pub fn try_resize(&mut self, length: usize, value: T) -> Result<(), MemoryError>
    where
        T: Copy,
    {
        if length > self.len() {
            self.try_reserve(length - self.len())?;
        }
        self.values.resize(length, value);
        Ok(())
    }

    pub fn try_append(&mut self, other: &mut Self) -> Result<(), MemoryError> {
        if !self.scope.same_root(&other.scope) {
            return Err(MemoryError::InvalidState);
        }
        self.try_reserve(other.len())?;
        self.values.append(&mut other.values);
        Ok(())
    }

    pub fn len(&self) -> usize {
        self.values.len()
    }
    pub fn is_empty(&self) -> bool {
        self.values.is_empty()
    }
    pub fn capacity(&self) -> usize {
        self.values.capacity()
    }
    pub fn as_slice(&self) -> &[T] {
        &self.values
    }
    pub fn as_mut_slice(&mut self) -> &mut [T] {
        &mut self.values
    }
    pub fn pop(&mut self) -> Option<T> {
        self.values.pop()
    }
    pub fn clear(&mut self) {
        self.values.clear();
    }
    pub fn truncate(&mut self, length: usize) {
        self.values.truncate(length);
    }
    pub fn remove(&mut self, index: usize) -> T {
        self.values.remove(index)
    }
    pub fn swap_remove(&mut self, index: usize) -> T {
        self.values.swap_remove(index)
    }
    pub fn scope(&self) -> &MemoryScope {
        &self.scope
    }
    pub fn family(&self) -> Family {
        self.family
    }

    /// For a controlled foreign data structure whose ABI requires std Vec.
    /// The caller must store data before its receipt, retain both together,
    /// and perform every later capacity change through a prepared paid block.
    /// Dropping the receipt while this Vec still owns storage violates funding.
    pub fn into_parts(self) -> (Vec<T>, Option<OwnerTicket>) {
        let Self { values, ticket, .. } = self;
        (values, ticket)
    }

    pub fn try_clone_in(&self, scope: &MemoryScope, family: Family) -> Result<Self, MemoryError>
    where
        T: BoundedClone,
    {
        let mut result = Self::try_with_capacity(scope, family, self.len())?;
        for value in &self.values {
            let cloned = value.try_clone_in(scope, family)?;
            result.values.push(cloned); // Exact len capacity already prepared.
        }
        Ok(result)
    }
}

impl<T> Deref for FundedVec<T> {
    type Target = [T];
    fn deref(&self) -> &[T] {
        self.as_slice()
    }
}
impl<T> DerefMut for FundedVec<T> {
    fn deref_mut(&mut self) -> &mut [T] {
        self.as_mut_slice()
    }
}
impl<T: fmt::Debug> fmt::Debug for FundedVec<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.values.fmt(f)
    }
}
impl<T: BoundedClone> BoundedClone for FundedVec<T> {
    fn try_clone_in(&self, scope: &MemoryScope, family: Family) -> Result<Self, MemoryError> {
        FundedVec::try_clone_in(self, scope, family)
    }
}
impl<'a, T> IntoIterator for &'a FundedVec<T> {
    type Item = &'a T;
    type IntoIter = std::slice::Iter<'a, T>;
    fn into_iter(self) -> Self::IntoIter {
        self.values.iter()
    }
}
impl<'a, T> IntoIterator for &'a mut FundedVec<T> {
    type Item = &'a mut T;
    type IntoIter = std::slice::IterMut<'a, T>;
    fn into_iter(self) -> Self::IntoIter {
        self.values.iter_mut()
    }
}

pub struct FundedString {
    bytes: FundedVec<u8>,
}

impl FundedString {
    pub fn new_in(scope: MemoryScope, family: Family) -> Self {
        Self {
            bytes: FundedVec::new_in(scope, family),
        }
    }
    pub fn try_with_capacity(
        scope: &MemoryScope,
        family: Family,
        capacity: usize,
    ) -> Result<Self, MemoryError> {
        Ok(Self {
            bytes: FundedVec::try_with_capacity(scope, family, capacity)?,
        })
    }
    pub fn try_from_str(
        scope: &MemoryScope,
        family: Family,
        source: &str,
    ) -> Result<Self, MemoryError> {
        let mut result = Self::try_with_capacity(scope, family, source.len())?;
        result.try_push_str(source)?;
        Ok(result)
    }
    pub fn try_push_str(&mut self, source: &str) -> Result<(), MemoryError> {
        self.bytes.try_extend_from_slice(source.as_bytes())
    }
    pub fn try_push(&mut self, value: char) -> Result<(), MemoryError> {
        let mut bytes = [0; 4];
        self.try_push_str(value.encode_utf8(&mut bytes))
    }
    pub fn as_str(&self) -> &str {
        // SAFETY: only valid str and char encodings can enter private bytes.
        unsafe { std::str::from_utf8_unchecked(self.bytes.as_slice()) }
    }
    pub fn as_bytes(&self) -> &[u8] {
        self.bytes.as_slice()
    }
    pub fn len(&self) -> usize {
        self.bytes.len()
    }
    pub fn is_empty(&self) -> bool {
        self.bytes.is_empty()
    }
    pub fn capacity(&self) -> usize {
        self.bytes.capacity()
    }
    pub fn clear(&mut self) {
        self.bytes.clear();
    }
    pub fn scope(&self) -> &MemoryScope {
        self.bytes.scope()
    }
    pub fn try_reserve(&mut self, additional: usize) -> Result<(), MemoryError> {
        self.bytes.try_reserve(additional)
    }
    pub fn try_clone_in(&self, scope: &MemoryScope, family: Family) -> Result<Self, MemoryError> {
        Self::try_from_str(scope, family, self.as_str())
    }

    /// Same joint-owner obligation as FundedVec::into_parts. String consumes
    /// the identical Global allocation without copying or changing its Layout.
    pub fn into_parts(self) -> (String, Option<OwnerTicket>) {
        let (bytes, ticket) = self.bytes.into_parts();
        // SAFETY: private bytes contain only appended valid UTF8 encodings.
        (unsafe { String::from_utf8_unchecked(bytes) }, ticket)
    }
}

impl Deref for FundedString {
    type Target = str;
    fn deref(&self) -> &str {
        self.as_str()
    }
}
impl AsRef<str> for FundedString {
    fn as_ref(&self) -> &str {
        self.as_str()
    }
}
impl Borrow<str> for FundedString {
    fn borrow(&self) -> &str {
        self.as_str()
    }
}
impl fmt::Debug for FundedString {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.as_str().fmt(f)
    }
}
impl fmt::Display for FundedString {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}
impl fmt::Write for FundedString {
    fn write_str(&mut self, source: &str) -> fmt::Result {
        self.try_push_str(source).map_err(|error| {
            self.scope().record_failure(error);
            fmt::Error
        })
    }
}
impl PartialEq for FundedString {
    fn eq(&self, other: &Self) -> bool {
        self.as_str() == other.as_str()
    }
}
impl Eq for FundedString {}
impl PartialEq<str> for FundedString {
    fn eq(&self, other: &str) -> bool {
        self.as_str() == other
    }
}
impl PartialEq<&str> for FundedString {
    fn eq(&self, other: &&str) -> bool {
        self.as_str() == *other
    }
}
impl PartialOrd for FundedString {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}
impl Ord for FundedString {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        self.as_str().cmp(other.as_str())
    }
}
impl Hash for FundedString {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.as_str().hash(state);
    }
}
impl BoundedClone for FundedString {
    fn try_clone_in(&self, scope: &MemoryScope, family: Family) -> Result<Self, MemoryError> {
        FundedString::try_clone_in(self, scope, family)
    }
}
