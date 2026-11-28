// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE file in the project root for license information.
// ---------------------------------------------------------------------------------------------

//! Explicit operation-local cumulative allocation admission. No ambient allocator or current job.
//! Layout requests, including old/new overlap, are charged before allocation. Charges are never
//! refunded during the operation. This is payload/layout accounting, not allocator/RSS accounting.

use crate::execution::{ExecutionControl, ExecutionError, Phase};
use std::alloc::{Layout, alloc};
use std::cell::Cell;
use std::marker::PhantomData;
use std::ptr;
use std::ptr::NonNull;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(usize)]
pub enum StorageFamily {
    Ast,
    Tokens,
    Strings,
    Layout,
    Auxiliary,
    Display,
    Path,
    Workspace,
    Metadata,
    Mirror,
    Assets,
}

pub const FAMILY_COUNT: usize = 11;
pub const JOB_BYTES: u64 = 64 * 1024 * 1024;
const MIB: u64 = 1024 * 1024;
pub const FAMILY_BYTES: [u64; FAMILY_COUNT] = [
    8 * MIB,
    MIB,
    4 * MIB,
    16 * MIB,
    2 * MIB,
    MIB,
    16 * MIB,
    4 * MIB,
    2 * MIB,
    4 * MIB,
    4 * MIB,
];
pub const FAMILY_SLOTS: [u64; FAMILY_COUNT] = [
    8192,
    16384,
    u64::MAX,
    16384,
    u64::MAX,
    4096,
    262144,
    u64::MAX,
    u64::MAX,
    u64::MAX,
    u64::MAX,
];

/// Owned cumulative admission state. Moving between worker and post-join stages never
/// refunds prior requests, and the first failure survives the handoff.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct StorageSnapshot {
    bytes: [u64; FAMILY_COUNT],
    slots: [u64; FAMILY_COUNT],
    failure: Option<StorageError>,
}
impl Default for StorageSnapshot {
    fn default() -> Self {
        Self {
            bytes: [0; FAMILY_COUNT],
            slots: [0; FAMILY_COUNT],
            failure: None,
        }
    }
}
impl StorageSnapshot {
    pub fn used_bytes(&self) -> &[u64; FAMILY_COUNT] {
        &self.bytes
    }
    pub fn used_slots(&self) -> &[u64; FAMILY_COUNT] {
        &self.slots
    }
    pub fn failure(&self) -> Option<StorageError> {
        self.failure
    }
    pub fn charge(
        &mut self,
        family: StorageFamily,
        bytes: u64,
        slots: u64,
    ) -> Result<(), StorageError> {
        if let Some(error) = self.failure {
            return Err(error);
        }
        let i = family as usize;
        let result = (|| {
            let next_bytes = self.bytes[i]
                .checked_add(bytes)
                .ok_or(StorageError::Arithmetic)?;
            let next_slots = self.slots[i]
                .checked_add(slots)
                .ok_or(StorageError::Arithmetic)?;
            if next_bytes > FAMILY_BYTES[i] {
                return Err(StorageError::FamilyBytes {
                    family,
                    limit: FAMILY_BYTES[i],
                    requested: next_bytes,
                });
            }
            if next_slots > FAMILY_SLOTS[i] {
                return Err(StorageError::FamilySlots {
                    family,
                    limit: FAMILY_SLOTS[i],
                    requested: next_slots,
                });
            }
            self.bytes[i] = next_bytes;
            self.slots[i] = next_slots;
            Ok(())
        })();
        if let Err(error) = result {
            self.failure = Some(error);
        }
        result
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum StorageError {
    DepthLimit {
        phase: Phase,
        limit: u32,
        requested: u32,
    },
    Execution(ExecutionError),
    Arithmetic,
    FamilyBytes {
        family: StorageFamily,
        limit: u64,
        requested: u64,
    },
    FamilySlots {
        family: StorageFamily,
        limit: u64,
        requested: u64,
    },
    AllocFailed {
        bytes: usize,
    },
    InvalidElement,
    InvalidUtf8,
    UnsupportedObject,
}

/// Explicit fallible clone. Every nested owned container participates in the same job ledger.
pub trait BoundedClone: Sized {
    fn bounded_clone(
        &self,
        storage: &StorageLedger<'_>,
        phase: Phase,
        family: StorageFamily,
    ) -> Result<Self, StorageError>;
}
macro_rules! copy_clone {
    ($($ty:ty),* $(,)?) => { $(impl BoundedClone for $ty {
        fn bounded_clone(&self, storage: &StorageLedger<'_>, phase: Phase, _family: StorageFamily)
            -> Result<Self, StorageError> { storage.check(phase)?; Ok(*self) }
    })* };
}
copy_clone!(bool, u8, u16, u32, u64, usize, i32, i64, f32, f64, char);
impl BoundedClone for String {
    fn bounded_clone(
        &self,
        storage: &StorageLedger<'_>,
        phase: Phase,
        _family: StorageFamily,
    ) -> Result<Self, StorageError> {
        storage.try_string(phase, self)
    }
}
impl<T: BoundedClone> BoundedClone for Option<T> {
    fn bounded_clone(
        &self,
        storage: &StorageLedger<'_>,
        phase: Phase,
        family: StorageFamily,
    ) -> Result<Self, StorageError> {
        storage.check(phase)?;
        self.as_ref()
            .map(|value| value.bounded_clone(storage, phase, family))
            .transpose()
    }
}
impl<T: BoundedClone> BoundedClone for Vec<T> {
    fn bounded_clone(
        &self,
        storage: &StorageLedger<'_>,
        phase: Phase,
        family: StorageFamily,
    ) -> Result<Self, StorageError> {
        let mut copy = storage.try_vec_with_capacity(phase, family, self.len())?;
        for value in self {
            copy.push(value.bounded_clone(storage, phase, family)?);
        }
        Ok(copy)
    }
}
impl<T: BoundedClone> BoundedClone for Box<T> {
    fn bounded_clone(
        &self,
        storage: &StorageLedger<'_>,
        phase: Phase,
        family: StorageFamily,
    ) -> Result<Self, StorageError> {
        storage.check(phase)?;
        let value = self.as_ref().bounded_clone(storage, phase, family)?;
        storage.try_box(phase, family, value)
    }
}

pub fn clone_controlled<T: BoundedClone + Clone>(
    control: Option<&ExecutionControl<'_>>,
    phase: Phase,
    family: StorageFamily,
    value: &T,
) -> Result<T, StorageError> {
    match control.and_then(ExecutionControl::storage) {
        Some(storage) => value.bounded_clone(&storage, phase, family),
        None => Ok(value.clone()),
    }
}

/// A builder for legacy internal functions that report failures through their explicit control.
/// A failed append stores the typed failure and drops its value; the controlled outer entry
/// checks that failure before returning any result. Slice access cannot trigger Vec growth.
pub struct WorkVec<'a, T> {
    values: Vec<T>,
    control: Option<&'a ExecutionControl<'a>>,
    phase: Phase,
    family: StorageFamily,
}
impl<'a, T> WorkVec<'a, T> {
    pub fn new(
        control: Option<&'a ExecutionControl<'a>>,
        phase: Phase,
        family: StorageFamily,
    ) -> Self {
        Self {
            values: Vec::new(),
            control,
            phase,
            family,
        }
    }
    pub fn push(&mut self, value: T) {
        if let Some(storage) = self.control.and_then(ExecutionControl::storage) {
            let _ = storage.try_push(self.phase, self.family, &mut self.values, value);
        } else {
            self.values.push(value);
        }
    }
    pub fn pop(&mut self) -> Option<T> {
        if self
            .control
            .is_some_and(|control| control.checkpoint(self.phase).is_err())
        {
            return None;
        }
        self.values.pop()
    }
    pub fn into_vec(self) -> Vec<T> {
        self.values
    }
}
impl<T> std::ops::Deref for WorkVec<'_, T> {
    type Target = [T];
    fn deref(&self) -> &[T] {
        &self.values
    }
}
impl<T> std::ops::DerefMut for WorkVec<'_, T> {
    fn deref_mut(&mut self) -> &mut [T] {
        &mut self.values
    }
}

struct SharedHeader<T> {
    refs: Cell<usize>,
    value: T,
}
/// Operation-local, fallibly allocated shared owner. It is deliberately neither Send nor Sync;
/// sharing only connects parser guards on the same worker, never distinct operation budgets.
pub struct LocalShared<T> {
    header: NonNull<SharedHeader<T>>,
    _single_thread: PhantomData<std::rc::Rc<()>>,
}
impl<T> LocalShared<T> {
    pub fn try_new(
        storage: &StorageLedger<'_>,
        phase: Phase,
        value: T,
    ) -> Result<Self, StorageError> {
        let owner = storage.try_box(
            phase,
            StorageFamily::Metadata,
            SharedHeader {
                refs: Cell::new(1),
                value,
            },
        )?;
        Ok(Self {
            header: NonNull::from(Box::leak(owner)),
            _single_thread: PhantomData,
        })
    }
}
impl<T> Clone for LocalShared<T> {
    fn clone(&self) -> Self {
        // SAFETY: a live owner guarantees a live header. All access is single-threaded and the
        // shared value is only read; mutation is provided by T's own Cell/Mutex contract.
        let header = unsafe { self.header.as_ref() };
        header.refs.set(
            header
                .refs
                .get()
                .checked_add(1)
                .expect("finite local shared references"),
        );
        Self {
            header: self.header,
            _single_thread: PhantomData,
        }
    }
}
impl<T> std::ops::Deref for LocalShared<T> {
    type Target = T;
    fn deref(&self) -> &T {
        // SAFETY: this borrow cannot outlive the owner. Header is unique to this refcount set.
        unsafe { &self.header.as_ref().value }
    }
}
impl<T: std::fmt::Debug> std::fmt::Debug for LocalShared<T> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        std::ops::Deref::deref(self).fmt(f)
    }
}
impl<T> Drop for LocalShared<T> {
    fn drop(&mut self) {
        // SAFETY: refs counts the live owners, including self, so it cannot be zero here.
        let header = unsafe { self.header.as_ref() };
        let remaining = header.refs.get() - 1;
        header.refs.set(remaining);
        if remaining == 0 {
            // SAFETY: the last owner reconstructs the original Box exactly once. The header
            // pointer is from Box::leak above and uses the same global allocator/layout.
            unsafe {
                drop(Box::from_raw(self.header.as_ptr()));
            }
        }
    }
}

impl From<ExecutionError> for StorageError {
    fn from(value: ExecutionError) -> Self {
        Self::Execution(value)
    }
}
impl std::fmt::Display for StorageError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(formatter, "{self:?}")
    }
}
impl std::error::Error for StorageError {}

/// A funding owner must outlive this ledger and every buffer created through it. Its 64-MiB
/// source/root and policy leases are supplied and retained by the adapter, never invented here.
/// No buffer is permitted to escape the adapter's final leased artifact or its owned workspace.
pub struct StorageLedger<'a> {
    control: &'a ExecutionControl<'a>,
}

impl<'a> StorageLedger<'a> {
    pub fn new(control: &'a ExecutionControl<'a>) -> Self {
        control.storage_enabled.set(true);
        Self { control }
    }
    pub(crate) fn view(control: &'a ExecutionControl<'a>) -> Self {
        Self { control }
    }
    pub fn control(&self) -> &'a ExecutionControl<'a> {
        self.control
    }
    pub fn failure(&self) -> Option<StorageError> {
        self.control.storage_failure.get()
    }
    pub fn check(&self, phase: Phase) -> Result<(), StorageError> {
        self.control
            .checkpoint(phase)
            .map_err(|error| self.fail(error.into()))?;
        match self.control.storage_failure.get() {
            Some(error) => Err(error),
            None => Ok(()),
        }
    }
    pub fn used_bytes(&self) -> [u64; FAMILY_COUNT] {
        std::array::from_fn(|i| self.control.storage_bytes[i].get())
    }
    pub fn used_slots(&self) -> [u64; FAMILY_COUNT] {
        std::array::from_fn(|i| self.control.storage_slots[i].get())
    }
    pub fn snapshot(&self) -> StorageSnapshot {
        StorageSnapshot {
            bytes: self.used_bytes(),
            slots: self.used_slots(),
            failure: self.failure(),
        }
    }
    /// Add already-funded outer-owner requests before any worker allocation. A zero seed
    /// keeps the legacy operation's existing cumulative counters unchanged.
    pub fn seed(&self, seed: StorageSnapshot) -> Result<(), StorageError> {
        if let Some(error) = seed.failure {
            return Err(self.fail(error));
        }
        const FAMILIES: [StorageFamily; FAMILY_COUNT] = [
            StorageFamily::Ast,
            StorageFamily::Tokens,
            StorageFamily::Strings,
            StorageFamily::Layout,
            StorageFamily::Auxiliary,
            StorageFamily::Display,
            StorageFamily::Path,
            StorageFamily::Workspace,
            StorageFamily::Metadata,
            StorageFamily::Mirror,
            StorageFamily::Assets,
        ];
        let mut merged = self.snapshot();
        for (i, family) in FAMILIES.into_iter().enumerate() {
            if seed.bytes[i] != 0 || seed.slots[i] != 0 {
                merged
                    .charge(family, seed.bytes[i], seed.slots[i])
                    .map_err(|error| self.fail(error))?;
            }
        }
        // Seed records allocations which already exist. Cancellation cannot erase their
        // admission. No new allocation/work occurs here; normal producer checkpoints follow.
        for i in 0..FAMILY_COUNT {
            self.control.storage_bytes[i].set(merged.bytes[i]);
            self.control.storage_slots[i].set(merged.slots[i]);
        }
        Ok(())
    }
    pub fn fail(&self, error: StorageError) -> StorageError {
        if self.control.storage_failure.get().is_none() {
            self.control.storage_failure.set(Some(error));
        }
        self.control.storage_failure.get().unwrap()
    }
    pub fn charge(
        &self,
        phase: Phase,
        family: StorageFamily,
        bytes: u64,
        slots: u64,
    ) -> Result<(), StorageError> {
        self.control
            .checkpoint(phase)
            .map_err(|error| self.fail(error.into()))?;
        if let Some(error) = self.control.storage_failure.get() {
            return Err(error);
        }
        let i = family as usize;
        let next_bytes = self.control.storage_bytes[i]
            .get()
            .checked_add(bytes)
            .ok_or_else(|| self.fail(StorageError::Arithmetic))?;
        let next_slots = self.control.storage_slots[i]
            .get()
            .checked_add(slots)
            .ok_or_else(|| self.fail(StorageError::Arithmetic))?;
        if next_bytes > FAMILY_BYTES[i] {
            return Err(self.fail(StorageError::FamilyBytes {
                family,
                limit: FAMILY_BYTES[i],
                requested: next_bytes,
            }));
        }
        if next_slots > FAMILY_SLOTS[i] {
            return Err(self.fail(StorageError::FamilySlots {
                family,
                limit: FAMILY_SLOTS[i],
                requested: next_slots,
            }));
        }
        self.control.storage_bytes[i].set(next_bytes);
        self.control.storage_slots[i].set(next_slots);
        Ok(())
    }
    pub fn try_vec_with_capacity<T>(
        &self,
        phase: Phase,
        family: StorageFamily,
        cap: usize,
    ) -> Result<Vec<T>, StorageError> {
        self.check(phase)?;
        if std::mem::size_of::<T>() == 0 {
            return Err(self.fail(StorageError::InvalidElement));
        }
        if cap == 0 {
            return Ok(Vec::new());
        }
        let layout = Layout::array::<T>(cap).map_err(|_| self.fail(StorageError::Arithmetic))?;
        self.charge(phase, family, layout.size() as u64, cap as u64)?;
        // SAFETY: Layout::array validates size/alignment and cap is nonzero for non-ZST T.
        // Global alloc is paired with Vec's global allocator. The returned Vec owns exactly
        // this allocation and has no initialized elements. A null allocation is never used.
        let raw = unsafe { alloc(layout) }.cast::<T>();
        if raw.is_null() {
            return Err(self.fail(StorageError::AllocFailed {
                bytes: layout.size(),
            }));
        }
        // SAFETY: pointer/layout/capacity match the preceding global allocation, len is zero.
        Ok(unsafe { Vec::from_raw_parts(raw, 0, cap) })
    }
    pub fn try_reserve<T>(
        &self,
        phase: Phase,
        family: StorageFamily,
        out: &mut Vec<T>,
        additional: usize,
    ) -> Result<(), StorageError> {
        self.check(phase)?;
        if let Some(error) = self.control.storage_failure.get() {
            return Err(error);
        }
        let required = out
            .len()
            .checked_add(additional)
            .ok_or_else(|| self.fail(StorageError::Arithmetic))?;
        if required <= out.capacity() {
            return Ok(());
        }
        let capacity = out
            .capacity()
            .checked_mul(2)
            .ok_or_else(|| self.fail(StorageError::Arithmetic))?
            .max(required)
            .max(1);
        let mut replacement = self.try_vec_with_capacity::<T>(phase, family, capacity)?;
        let len = out.len();
        // SAFETY: replacement's capacity is at least required >= len. Allocations do not
        // overlap. All initialized values move exactly once; neither copying nor dropping T
        // invokes user code here. Old Vec is set to len zero before its allocation is freed.
        unsafe {
            ptr::copy_nonoverlapping(out.as_ptr(), replacement.as_mut_ptr(), len);
            replacement.set_len(len);
            out.set_len(0);
        }
        *out = replacement;
        Ok(())
    }
    pub fn try_push<T>(
        &self,
        phase: Phase,
        family: StorageFamily,
        out: &mut Vec<T>,
        value: T,
    ) -> Result<(), StorageError> {
        self.try_reserve(phase, family, out, 1)?;
        out.push(value); // capacity was admitted before this append; cannot grow.
        Ok(())
    }
    pub fn try_extend_copy<T: Copy>(
        &self,
        phase: Phase,
        family: StorageFamily,
        out: &mut Vec<T>,
        values: &[T],
    ) -> Result<(), StorageError> {
        self.try_reserve(phase, family, out, values.len())?;
        out.extend_from_slice(values); // exact known count and sufficient capacity.
        Ok(())
    }
    pub fn try_box<T>(
        &self,
        phase: Phase,
        family: StorageFamily,
        value: T,
    ) -> Result<Box<T>, StorageError> {
        self.check(phase)?;
        if std::mem::size_of::<T>() == 0 {
            return Err(self.fail(StorageError::InvalidElement));
        }
        let layout = Layout::new::<T>();
        self.charge(phase, family, layout.size() as u64, 1)?;
        // SAFETY: valid nonzero Layout, global allocator paired with Box, initialized once.
        let raw = unsafe { alloc(layout) }.cast::<T>();
        if raw.is_null() {
            return Err(self.fail(StorageError::AllocFailed {
                bytes: layout.size(),
            }));
        }
        // SAFETY: aligned unique allocation for T. The write cannot panic; Box takes ownership.
        unsafe {
            raw.write(value);
            Ok(Box::from_raw(raw))
        }
    }
    pub fn try_string(&self, phase: Phase, value: &str) -> Result<String, StorageError> {
        self.check(phase)?;
        if value.len() > 65536 {
            return Err(self.fail(StorageError::FamilyBytes {
                family: StorageFamily::Strings,
                limit: 65536,
                requested: value.len() as u64,
            }));
        }
        let mut bytes =
            self.try_vec_with_capacity::<u8>(phase, StorageFamily::Strings, value.len())?;
        bytes.extend_from_slice(value.as_bytes());
        // SAFETY: exact copy of a valid str, with no mutation after copy.
        Ok(unsafe { String::from_utf8_unchecked(bytes) })
    }
    pub fn try_format(
        &self,
        phase: Phase,
        args: std::fmt::Arguments<'_>,
    ) -> Result<String, StorageError> {
        use std::fmt::Write;
        struct Counter(usize);
        impl Write for Counter {
            fn write_str(&mut self, text: &str) -> std::fmt::Result {
                self.0 = self.0.checked_add(text.len()).ok_or(std::fmt::Error)?;
                Ok(())
            }
        }
        self.check(phase)?;
        let mut counter = Counter(0);
        counter
            .write_fmt(args)
            .map_err(|_| self.fail(StorageError::Arithmetic))?;
        if counter.0 > 65536 {
            return Err(self.fail(StorageError::FamilyBytes {
                family: StorageFamily::Strings,
                limit: 65536,
                requested: counter.0 as u64,
            }));
        }
        self.control
            .consume(
                phase,
                crate::execution::Resource::GeneratedBytes,
                counter.0 as u64,
            )
            .map_err(|error| self.fail(error.into()))?;
        let bytes = self.try_vec_with_capacity::<u8>(phase, StorageFamily::Strings, counter.0)?;
        // SAFETY: an empty buffer is valid UTF-8, regardless of its admitted capacity.
        let text = unsafe { String::from_utf8_unchecked(bytes) };
        struct Writer<'a, 'b> {
            storage: &'a StorageLedger<'b>,
            phase: Phase,
            text: String,
        }
        impl Write for Writer<'_, '_> {
            fn write_str(&mut self, text: &str) -> std::fmt::Result {
                self.storage
                    .try_append(self.phase, &mut self.text, text)
                    .map_err(|_| std::fmt::Error)
            }
        }
        let mut writer = Writer {
            storage: self,
            phase,
            text,
        };
        writer.write_fmt(args).map_err(|_| {
            self.failure()
                .unwrap_or_else(|| self.fail(StorageError::InvalidElement))
        })?;
        Ok(writer.text)
    }
    pub fn try_append(
        &self,
        phase: Phase,
        out: &mut String,
        value: &str,
    ) -> Result<(), StorageError> {
        self.check(phase)?;
        let required = out
            .len()
            .checked_add(value.len())
            .ok_or_else(|| self.fail(StorageError::Arithmetic))?;
        if required > 65536 {
            return Err(self.fail(StorageError::FamilyBytes {
                family: StorageFamily::Strings,
                limit: 65536,
                requested: required as u64,
            }));
        }
        if required > out.capacity() {
            let capacity = out
                .capacity()
                .checked_mul(2)
                .ok_or_else(|| self.fail(StorageError::Arithmetic))?
                .max(required)
                .max(1);
            let mut bytes =
                self.try_vec_with_capacity::<u8>(phase, StorageFamily::Strings, capacity)?;
            bytes.extend_from_slice(out.as_bytes());
            bytes.extend_from_slice(value.as_bytes());
            // SAFETY: concatenation of two valid UTF-8 strings.
            *out = unsafe { String::from_utf8_unchecked(bytes) };
        } else {
            out.push_str(value);
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::execution::Limits;
    use std::cell::Cell;
    use std::sync::atomic::AtomicBool;

    #[test]
    fn cumulative_growth_accounts_old_and_new_and_moves_owners() {
        let cancelled = AtomicBool::new(false);
        let control = ExecutionControl::new(&cancelled, None, Limits::default());
        let ledger = StorageLedger::new(&control);
        let dropped = Cell::new(0);
        struct Owner<'a>(&'a Cell<usize>);
        impl Drop for Owner<'_> {
            fn drop(&mut self) {
                self.0.set(self.0.get() + 1);
            }
        }
        let mut values = Vec::new();
        for _ in 0..3 {
            ledger
                .try_push(
                    Phase::Parser,
                    StorageFamily::Ast,
                    &mut values,
                    Owner(&dropped),
                )
                .unwrap();
        }
        assert_eq!(ledger.used_slots()[StorageFamily::Ast as usize], 1 + 2 + 4);
        assert_eq!(dropped.get(), 0);
        drop(values);
        assert_eq!(dropped.get(), 3);
        assert_eq!(ledger.used_slots()[StorageFamily::Ast as usize], 7);
    }
    #[test]
    fn quota_and_zero_work_cancel_are_checked_before_allocation() {
        let cancelled = AtomicBool::new(false);
        let control = ExecutionControl::new(&cancelled, None, Limits::default());
        let ledger = StorageLedger::new(&control);
        let result =
            ledger.try_vec_with_capacity::<u64>(Phase::Parser, StorageFamily::Tokens, 16385);
        assert!(matches!(result, Err(StorageError::FamilySlots { .. })));
        assert_eq!(ledger.used_bytes()[StorageFamily::Tokens as usize], 0);
        let cancelled = AtomicBool::new(true);
        let control = ExecutionControl::new(&cancelled, None, Limits::default());
        let ledger = StorageLedger::new(&control);
        assert!(matches!(
            ledger.try_vec_with_capacity::<u8>(Phase::Lexer, StorageFamily::Strings, 0),
            Err(StorageError::Execution(_))
        ));
    }
    #[test]
    fn utf8_append_keeps_valid_bytes_and_never_uses_implicit_growth() {
        let cancelled = AtomicBool::new(false);
        let control = ExecutionControl::new(&cancelled, None, Limits::default());
        let ledger = StorageLedger::new(&control);
        let mut text = ledger.try_string(Phase::Parser, "≠∉©").unwrap();
        ledger.try_append(Phase::Parser, &mut text, "😀").unwrap();
        assert_eq!(text, "≠∉©😀");
        assert_eq!(ledger.used_bytes()[StorageFamily::Strings as usize], 8 + 16);
    }
    #[test]
    fn a_second_view_cannot_reset_the_same_job_and_last_owner_drops_once() {
        let cancelled = AtomicBool::new(false);
        let control = ExecutionControl::new(&cancelled, None, Limits::default());
        let first = StorageLedger::new(&control);
        let dropped = Cell::new(0usize);
        struct Owner<'a>(&'a Cell<usize>);
        impl Drop for Owner<'_> {
            fn drop(&mut self) {
                self.0.set(self.0.get() + 1);
            }
        }
        let shared = LocalShared::try_new(&first, Phase::Parser, Owner(&dropped)).unwrap();
        let counted = first.used_bytes();
        let second = StorageLedger::new(&control);
        assert_eq!(second.used_bytes(), counted);
        let last = shared.clone();
        drop(shared);
        assert_eq!(dropped.get(), 0);
        drop(last);
        assert_eq!(dropped.get(), 1);
        assert_eq!(second.used_bytes(), counted);
    }
}
