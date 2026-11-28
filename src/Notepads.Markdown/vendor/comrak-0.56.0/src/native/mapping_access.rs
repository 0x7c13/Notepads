// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE file in the project root for license information.
// ---------------------------------------------------------------------------------------------

//! Short-lived mapping allocation access to the original Host and registry.
//! It cannot emit grammar facts or reach the Context/AST. The only source field
//! is the read-only input length already required by accepted block claims.

use super::{Host, NativeViolation, WorkDelta, allocation::Registry};

pub(super) struct MappingAccess<'a, H: Host> {
    pub(super) host: &'a mut H,
    pub(super) registry: &'a mut Registry<H::Lease>,
    pub(super) input_len: usize,
    pub(super) inline_policy: super::InlinePolicy,
}

impl<H: Host> MappingAccess<'_, H> {
    pub(crate) fn scan(&mut self, mut bytes: usize) -> Result<(), H::Error> {
        while bytes != 0 {
            let count = bytes.min(65_536);
            self.host.work(WorkDelta::Scan(count as u64))?;
            bytes -= count;
        }
        Ok(())
    }
    pub(crate) fn event(&mut self) -> Result<(), H::Error> {
        self.host.work(WorkDelta::Events(1))
    }
    pub(super) fn ensure_slot(&mut self) -> Result<(), H::Error> {
        self.registry.prepare(true, self.host)
    }
    pub(super) fn retain(&mut self, pointer: usize, lease: H::Lease) -> Result<(), H::Error> {
        self.registry
            .retain(pointer, lease, true, self.host)
            .map(|_| ())
    }
    pub(super) fn release(&mut self, pointer: usize) -> Result<(), H::Error> {
        self.registry.release_pointer(pointer, self.host)
    }
    pub(crate) fn discard_string(&mut self, value: String) -> Result<(), H::Error> {
        let pointer = if value.capacity() == 0 {
            0
        } else {
            value.as_ptr() as usize
        };
        drop(value);
        self.release(pointer)
    }
    pub(crate) fn grow_string(
        &mut self,
        value: &mut String,
        additional: usize,
    ) -> Result<(), H::Error> {
        self.registry.mutable(self.host)?;
        let target = value
            .len()
            .checked_add(additional)
            .ok_or_else(|| self.host.reject(NativeViolation::ArithmeticOverflow))?;
        if target <= value.capacity() {
            return Ok(());
        }
        let target = target.max(
            value
                .capacity()
                .max(16)
                .checked_mul(2)
                .ok_or_else(|| self.host.reject(NativeViolation::ArithmeticOverflow))?,
        );
        self.ensure_slot()?;
        let mut lease = self.host.reserve(target as u64)?;
        let mut grown = String::new();
        grown
            .try_reserve_exact(target)
            .map_err(|_| self.host.reject(NativeViolation::OutOfMemory))?;
        self.host
            .admit_actual(&mut lease, grown.capacity() as u64)?;
        self.retain(grown.as_ptr() as usize, lease)?;
        let mut offset = 0;
        while offset < value.len() {
            let mut end = (offset + 65_536).min(value.len());
            while !value.is_char_boundary(end) {
                end -= 1;
            }
            self.host.work(WorkDelta::Copy((end - offset) as u64))?;
            grown.push_str(&value[offset..end]);
            offset = end;
        }
        let old_pointer = if value.capacity() == 0 {
            0
        } else {
            value.as_ptr() as usize
        };
        *value = grown; // old allocation frees before its capacity lease.
        self.release(old_pointer)
    }
    pub(crate) fn append(&mut self, target: &mut String, value: &str) -> Result<(), H::Error> {
        self.grow_string(target, value.len())?;
        let mut offset = 0;
        while offset < value.len() {
            let mut end = (offset + 65_536).min(value.len());
            while !value.is_char_boundary(end) {
                end -= 1;
            }
            self.host.work(WorkDelta::Copy((end - offset) as u64))?;
            target.push_str(&value[offset..end]);
            offset = end;
        }
        Ok(())
    }
    pub(crate) fn string(&mut self, value: &str) -> Result<String, H::Error> {
        let mut target = String::new();
        self.append(&mut target, value)?;
        Ok(target)
    }
}
