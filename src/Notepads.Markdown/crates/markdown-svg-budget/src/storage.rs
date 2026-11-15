// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE file in the project root for license information.
// ---------------------------------------------------------------------------------------------
//! Checked storage operations used by font and raster owners. The existing
//! codec facade retains its separate accepted allocation/window contract.

use crate::{BudgetVec, Error, FailureKind, LimitKind, ResourcesRef};
use core::mem::size_of;

const MAX_EVENTS: usize = 256;
const MAX_BYTES: usize = 65_536;

pub(crate) fn copy_window<T>() -> Result<usize, Error> {
    let bytes = size_of::<T>();
    if bytes > MAX_BYTES {
        return Err(Error::new(FailureKind::Quota, LimitKind::Capacity));
    }
    Ok(MAX_EVENTS.min(MAX_BYTES / bytes.max(1)))
}

pub(crate) fn copy_work<T>(count: usize) -> u64 {
    // All callers use copy_window first. Count and product therefore fit
    // 256 events / 65536 bytes, including nonzero logical work for ZSTs.
    (count * size_of::<T>()).max(count) as u64
}

impl<T> BudgetVec<T> {
    /// Commit one previously reserved element with no allocation, Clone,
    /// resource callback or failure check. The caller must preflight capacity,
    /// bounded work and the first slot before this short ownership transfer.
    pub fn push_reserved(&mut self, value: T) {
        assert!(self.values.len() < self.values.capacity());
        self.values.push(value);
    }

    /// Resize Copy records in bounded initialized windows. An error may leave
    /// a valid partial new length; this owner retains every value and fee.
    /// Capacity survives shrinking. Records larger than 64KiB are unsupported
    /// by this operation and return typed Capacity before storage changes.
    pub fn try_resize_copy(&mut self, length: usize, value: T) -> Result<(), Error>
    where
        T: Copy,
    {
        self.resources.checkpoint(0)?;
        let window = copy_window::<T>().map_err(|error| self.resources.record(error))?;
        if length <= self.len() {
            self.values.truncate(length);
            return self.resources.checkpoint(0);
        }
        self.try_reserve_exact(length - self.len())?;
        while self.len() < length {
            let count = window.min(length - self.len());
            self.resources.checkpoint(copy_work::<T>(count))?;
            for _ in 0..count {
                self.push_reserved(value);
            }
        }
        self.resources.checkpoint(0)
    }

    /// Explicitly copy nested owners into a separately paid same-request,
    /// same-kind target. No T::Clone is hidden here. The callback must record
    /// any resource error before its own local owners unwind; this method can
    /// only fix the first slot before cleaning the values it receives.
    pub fn try_clone_with(
        &self,
        mut copy: impl FnMut(&T, &ResourcesRef) -> Result<T, Error>,
    ) -> Result<Self, Error> {
        self.resources.checkpoint(0)?;
        let window = copy_window::<T>().map_err(|error| self.resources.record(error))?;
        let mut target = Self::new(self.resources.clone(), self.kind);
        target.try_reserve_exact(self.len())?;
        for chunk in self.as_slice().chunks(window) {
            self.resources.checkpoint(copy_work::<T>(chunk.len()))?;
            for original in chunk {
                let value = copy(original, &self.resources)
                    .map_err(|error| self.resources.record(error))?;
                // A callback may return a valid owner after a nested cleanup
                // has fixed the first slot. Reject it before the commit, and
                // only then let value/target cleanup run.
                self.resources.checkpoint(0)?;
                target.push_reserved(value);
            }
        }
        // The callback itself can own paid captures, independently of the
        // values it returns. Retire it before the success tail, so its last
        // refund cannot turn a returned Ok into a latent first-slot failure.
        drop(copy);
        self.resources.checkpoint(0)?;
        Ok(target)
    }
}
