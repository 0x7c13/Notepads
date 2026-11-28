// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE file in the project root for license information.
// ---------------------------------------------------------------------------------------------

//! Ordered native records. Page buffers own their original host permits;
//! neither pages nor the directory are also entered in Context's address ledger.

use super::{Host, NativeViolation, WorkDelta};

struct Page<T, L> {
    values: Vec<T>,
    _lease: L,
}
struct Directory<T, L> {
    pages: Vec<Option<Page<T, L>>>,
    lease: L,
}
pub(super) struct NativeRows<T, L> {
    pages: Vec<Option<Page<T, L>>>,
    directory_lease: Option<L>,
    pending: Option<Directory<T, L>>,
    len: usize,
}
impl<T, L> Default for NativeRows<T, L> {
    fn default() -> Self {
        Self {
            pages: Vec::new(),
            directory_lease: None,
            pending: None,
            len: 0,
        }
    }
}
impl<T, L> std::fmt::Debug for NativeRows<T, L> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("NativeRows")
            .field("len", &self.len)
            .finish_non_exhaustive()
    }
}

/// Covers the *whole* mutation, including instructions after a successful Host
/// call. An unwind caught by the caller cannot resume this same Host/Work.
struct Mutation<'h, H: Host> {
    host: &'h mut H,
    closed: bool,
}
impl<H: Host> Drop for Mutation<'_, H> {
    fn drop(&mut self) {
        if !self.closed {
            self.host.reject(NativeViolation::InternalInvariant);
        }
    }
}

impl<T, L> NativeRows<T, L> {
    pub(super) fn len(&self) -> usize {
        self.len
    }
    pub(super) fn width() -> usize {
        let size = std::mem::size_of::<T>();
        if size == 0 || size > 16_384 {
            0
        } else {
            (16_384 / size).min(1024)
        }
    }
    pub(super) fn descriptor_bytes() -> usize {
        std::mem::size_of::<Option<Page<T, L>>>()
    }

    // Charging is supplied by the existing Host or read-only NativeReads. It
    // grants no allocation capability to a reader and exposes no permit.
    pub(super) fn get<E>(
        &self,
        index: usize,
        charge: impl FnOnce(usize) -> Result<(), E>,
    ) -> Result<Option<&T>, E> {
        charge(Self::descriptor_bytes())?;
        let width = Self::width();
        Ok(if width == 0 || index >= self.len {
            None
        } else {
            self.pages
                .get(index / width)
                .and_then(Option::as_ref)
                .and_then(|p| p.values.get(index % width))
        })
    }
    pub(super) fn get_mut<E>(
        &mut self,
        index: usize,
        charge: impl FnOnce(usize) -> Result<(), E>,
    ) -> Result<Option<&mut T>, E> {
        charge(Self::descriptor_bytes())?;
        let width = Self::width();
        Ok(if width == 0 || index >= self.len {
            None
        } else {
            self.pages
                .get_mut(index / width)
                .and_then(Option::as_mut)
                .and_then(|p| p.values.get_mut(index % width))
        })
    }
    /// Borrow only the initialized suffix of one physical page. Internal
    /// sequential consumers charge each row read while holding this descriptor.
    pub(super) fn page_tail<E>(
        &self,
        index: usize,
        charge: impl FnOnce(usize) -> Result<(), E>,
    ) -> Result<Option<&[T]>, E> {
        charge(Self::descriptor_bytes())?;
        let width = Self::width();
        Ok(if width == 0 || index >= self.len {
            None
        } else {
            self.pages
                .get(index / width)
                .and_then(Option::as_ref)
                .and_then(|p| p.values.get(index % width..))
        })
    }

    pub(super) fn page_tail_mut<E>(
        &mut self,
        index: usize,
        charge: impl FnOnce(usize) -> Result<(), E>,
    ) -> Result<Option<&mut [T]>, E> {
        charge(Self::descriptor_bytes())?;
        let width = Self::width();
        Ok(if width == 0 || index >= self.len {
            None
        } else {
            self.pages
                .get_mut(index / width)
                .and_then(Option::as_mut)
                .and_then(|p| p.values.get_mut(index % width..))
        })
    }

    /// Keep one physical descriptor borrowed across a sequential owner walk.
    /// The existing event for each record remains; the callback performs the
    /// original field/physical-owner work on that actual row.
    pub(super) fn visit<H: Host<Lease = L>>(
        &self,
        host: &mut H,
        mut visit: impl FnMut(&T, &mut H) -> Result<(), H::Error>,
    ) -> Result<(), H::Error> {
        let mut index = 0;
        while index < self.len {
            let page = self
                .page_tail(index, |n| charge_host(host, n))?
                .ok_or_else(|| host.reject(NativeViolation::InvalidProvenance))?;
            for (offset, row) in page.iter().enumerate() {
                if offset != 0 {
                    host.work(WorkDelta::Events(1))?;
                }
                visit(row, host)?;
            }
            index += page.len();
        }
        Ok(())
    }

    /// Initialize repeated records without repeatedly locating the same page.
    /// No resize may allocate: every page has already obtained its own permit.
    pub(super) fn extend_repeated<H: Host<Lease = L>>(
        &mut self,
        value: T,
        count: usize,
        host: &mut H,
    ) -> Result<(), H::Error>
    where
        T: Copy,
    {
        let mut guard = Mutation {
            host,
            closed: false,
        };
        let result = self.extend_repeated_inner(value, count, guard.host);
        guard.closed = true;
        result
    }

    fn extend_repeated_inner<H: Host<Lease = L>>(
        &mut self,
        value: T,
        mut count: usize,
        host: &mut H,
    ) -> Result<(), H::Error>
    where
        T: Copy,
    {
        let width = Self::width();
        if width == 0 || self.pending.is_some() {
            return Err(host.reject(NativeViolation::InvalidProvenance));
        }
        self.len
            .checked_add(count)
            .ok_or_else(|| host.reject(NativeViolation::ArithmeticOverflow))?;
        while count != 0 {
            if self.len.is_multiple_of(width) {
                host.work(WorkDelta::Copy(std::mem::size_of::<T>() as u64))?;
                self.push_inner(value, host)?;
                count -= 1;
                if count == 0 {
                    break;
                }
                if self.len.is_multiple_of(width) {
                    continue;
                }
            }
            charge_host(host, Self::descriptor_bytes())?;
            let page = self
                .pages
                .last_mut()
                .and_then(Option::as_mut)
                .ok_or_else(|| host.reject(NativeViolation::InvalidProvenance))?;
            let available = width - page.values.len();
            if page.values.capacity() < width || available == 0 {
                return Err(host.reject(NativeViolation::InvalidProvenance));
            }
            let mut remaining = available.min(count);
            while remaining != 0 {
                // Bounded original work units, with a real page borrow retained.
                let batch = remaining.min(256);
                host.work(WorkDelta::Events(batch as u64))?;
                host.work(WorkDelta::Copy((batch * std::mem::size_of::<T>()) as u64))?;
                page.values.resize(page.values.len() + batch, value);
                self.len += batch;
                remaining -= batch;
                count -= batch;
            }
        }
        Ok(())
    }

    pub(super) fn push<H: Host<Lease = L>>(
        &mut self,
        value: T,
        host: &mut H,
    ) -> Result<(), H::Error> {
        let mut guard = Mutation {
            host,
            closed: false,
        };
        let result = self.push_inner(value, guard.host);
        // All typed errors originate in this same Host (including reject).
        // No independent error/fault is stored in this physical owner.
        guard.closed = true;
        result
    }
    fn push_inner<H: Host<Lease = L>>(&mut self, value: T, host: &mut H) -> Result<(), H::Error> {
        host.work(WorkDelta::Events(1))?;
        let width = Self::width();
        if width == 0 || self.pending.is_some() {
            return Err(host.reject(NativeViolation::InvalidProvenance));
        }
        let next = self
            .len
            .checked_add(1)
            .ok_or_else(|| host.reject(NativeViolation::ArithmeticOverflow))?;
        if self.len.is_multiple_of(width) {
            if self.pages.len() == self.pages.capacity() {
                let count = self
                    .pages
                    .capacity()
                    .max(2)
                    .checked_mul(2)
                    .ok_or_else(|| host.reject(NativeViolation::ArithmeticOverflow))?;
                let bytes = count
                    .checked_mul(Self::descriptor_bytes())
                    .ok_or_else(|| host.reject(NativeViolation::ArithmeticOverflow))?;
                let mut lease = host.reserve(bytes as u64)?;
                let mut pages = Vec::new();
                pages
                    .try_reserve_exact(count)
                    .map_err(|_| host.reject(NativeViolation::OutOfMemory))?;
                let actual = pages
                    .capacity()
                    .checked_mul(Self::descriptor_bytes())
                    .ok_or_else(|| host.reject(NativeViolation::ArithmeticOverflow))?;
                host.admit_actual(&mut lease, actual as u64)?;
                self.pending = Some(Directory { pages, lease });
                for _ in 0..self.pages.len() {
                    host.work(WorkDelta::Copy(Self::descriptor_bytes() as u64))?;
                    self.pending.as_mut().unwrap().pages.push(None);
                }
                for index in 0..self.pages.len() {
                    host.work(WorkDelta::Copy(Self::descriptor_bytes() as u64))?;
                    // Stable ordinal; never reverse descriptors as FallibleArena may.
                    self.pending.as_mut().unwrap().pages[index] = self.pages[index].take();
                }
                let Directory { pages, lease } = self.pending.take().unwrap();
                let old = std::mem::replace(&mut self.pages, pages);
                drop(old);
                self.directory_lease = Some(lease);
            }
            let bytes = width
                .checked_mul(std::mem::size_of::<T>())
                .ok_or_else(|| host.reject(NativeViolation::ArithmeticOverflow))?;
            let mut lease = host.reserve(bytes as u64)?;
            let mut values = Vec::new();
            values
                .try_reserve_exact(width)
                .map_err(|_| host.reject(NativeViolation::OutOfMemory))?;
            let actual = values
                .capacity()
                .checked_mul(std::mem::size_of::<T>())
                .ok_or_else(|| host.reject(NativeViolation::ArithmeticOverflow))?;
            host.admit_actual(&mut lease, actual as u64)?;
            host.work(WorkDelta::Copy(Self::descriptor_bytes() as u64))?;
            self.pages.push(Some(Page {
                values,
                _lease: lease,
            }));
        }
        charge_host(host, Self::descriptor_bytes())?;
        let page = self
            .pages
            .last_mut()
            .and_then(Option::as_mut)
            .ok_or_else(|| host.reject(NativeViolation::InvalidProvenance))?;
        page.values.push(value);
        self.len = next;
        Ok(())
    }
}

pub(super) fn charge_host<H: Host>(host: &mut H, bytes: usize) -> Result<(), H::Error> {
    host.work(WorkDelta::Events(1))?;
    host.work(WorkDelta::Scan(bytes as u64))
}

#[cfg(test)]
#[path = "rows_tests.rs"]
mod tests;
