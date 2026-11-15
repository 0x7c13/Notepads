// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE file in the project root for license information.
// ---------------------------------------------------------------------------------------------

//! Process image authority. Every operation takes the control lock
//! before this lock; callbacks run after both locks have been released.

use super::*;
use std::sync::{Condvar, OnceLock};

pub type CapacityNotify = extern "C" fn(u64);

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct ProcessImageUsage {
    pub generation: u64,
    pub encoded_bytes: u64,
    pub decoded_bytes: u64,
}

pub(super) struct ProcessImageAuthority {
    pub(super) state: Mutex<ProcessImageUsage>,
}

static PROCESS: OnceLock<Arc<ProcessImageAuthority>> = OnceLock::new();
struct NotificationState {
    callback: Option<CapacityNotify>,
    active: u32,
}
static NOTIFY: Mutex<NotificationState> = Mutex::new(NotificationState {
    callback: None,
    active: 0,
});
static NOTIFY_IDLE: Condvar = Condvar::new();

impl ProcessImageAuthority {
    pub(super) fn shared() -> Arc<Self> {
        PROCESS
            .get_or_init(|| {
                Arc::new(Self {
                    state: Mutex::new(ProcessImageUsage::default()),
                })
            })
            .clone()
    }

    pub(super) fn notify_capacity(&self, generation: u64) {
        // Unit tests may inject an isolated process ledger. Their releases
        // must not notify observers of the real process ledger.
        #[cfg(test)]
        if !std::ptr::eq(self, Arc::as_ptr(&Self::shared())) {
            return;
        }
        notify_capacity(generation);
    }

    pub(super) fn lock(&self) -> MutexGuard<'_, ProcessImageUsage> {
        self.state
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
    }
}

/// The native integration installs one static module callback, never a pointer
/// to a control. Original native reservation owners pin module code throughout
/// their actual destroy call. The callback owns its teardown-safe weak registry.
pub fn set_process_image_capacity_notify(notify: Option<CapacityNotify>) {
    let mut state = NOTIFY
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    // Native registration/teardown is serialized. Clear the function pointer
    // before waiting so no new callback may race module unload. The callback
    // must not register/unregister itself; it only schedules weak UI waiters.
    state.callback = None;
    while state.active != 0 {
        state = NOTIFY_IDLE
            .wait(state)
            .unwrap_or_else(std::sync::PoisonError::into_inner);
    }
    state.callback = notify;
}

pub fn process_image_usage() -> ProcessImageUsage {
    *ProcessImageAuthority::shared().lock()
}

pub(super) fn notify_capacity(generation: u64) {
    let callback = {
        let mut state = NOTIFY
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let callback = state.callback;
        if callback.is_some() {
            state.active = state
                .active
                .checked_add(1)
                .expect("bounded image capacity notifications");
        }
        callback
    };
    if let Some(callback) = callback {
        callback(generation);
        let mut state = NOTIFY
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        state.active -= 1;
        if state.active == 0 {
            NOTIFY_IDLE.notify_all();
        }
    }
}

#[derive(Clone, Copy)]
pub(super) struct CapacityFailure {
    pub(super) kind: ByteKind,
    pub(super) bytes: u64,
    pub(super) generation: u64,
}

impl ImageResourcePolicy {
    /// Logical withdrawal never returns a physical byte or active IO charge.
    /// Intrinsic metadata remains available to layout in a non-Ready row.
    pub fn evict_variant(&self, object: u32) -> Result<(), ResourceError> {
        let mut state = self.shared.lock();
        let index = state.find(object).ok_or(ResourceError::UnknownObject)?;
        let generation = state.next_generation()?;
        let row = state.entries[index].as_mut().expect("occupied prefix");
        let capacity_limited = row.capacity_failure.is_some()
            && row
                .snapshot
                .failure
                .is_some_and(|failure| failure.kind == FailureKind::Limited);
        row.candidate = None;
        row.snapshot.pending_request = None;
        row.snapshot.request = None;
        row.snapshot.published_target = None;
        row.snapshot.state = ImageState::Unrequested;
        if !capacity_limited {
            row.snapshot.failure = None;
            row.snapshot.failure_detail = 0;
            row.snapshot.last_attempt_generation = 0;
            row.capacity_failure = None;
        }
        row.ready_reservation = 0;
        state.generation = generation;
        Ok(())
    }

    /// The same semantic image selected a different authored source. Revoke
    /// old candidates and metadata while preserving physical owner accounting.
    pub fn replace_variant_source(&self, object: u32) -> Result<(), ResourceError> {
        let mut state = self.shared.lock();
        let Some(index) = state.find(object) else {
            return Ok(());
        };
        let generation = state.next_generation()?;
        let row = state.entries[index].as_mut().expect("occupied prefix");
        row.snapshot = EntrySnapshot {
            object,
            desired_target: row.snapshot.desired_target,
            ..EntrySnapshot::default()
        };
        row.known_kind = None;
        row.candidate = None;
        row.ready_reservation = 0;
        row.capacity_failure = None;
        state.generation = generation;
        Ok(())
    }

    /// WIC has validated intrinsic dimensions and planned physical output,
    /// but no pixel allocation/publication is authorized by this metadata.
    pub fn note_variant_metadata(
        &self,
        key: RequestKey,
        metadata: ImageVariantMetadata,
    ) -> Result<(), ResourceError> {
        let mut state = self.shared.lock();
        let index = state.pending(key)?;
        let row = state.entries[index].as_ref().expect("occupied prefix");
        if !matches!(metadata, ImageVariantMetadata::Raster(_))
            || row
                .candidate
                .as_ref()
                .is_none_or(|candidate| candidate.kind == AdmissionKind::Svg)
        {
            return Err(ResourceError::InvalidMetadata);
        }
        metadata.required_capacity(
            row.snapshot
                .desired_target
                .ok_or(ResourceError::InvalidState)?,
        )?;
        let generation = state.next_generation()?;
        let row = state.entries[index].as_mut().expect("occupied prefix");
        row.known_kind = Some(KnownKind::Raster);
        if row.ready_reservation == 0 {
            row.snapshot.metadata = Some(metadata);
        }
        state.generation = generation;
        Ok(())
    }

    /// Bounded row recycling is allowed only after all actual current-epoch
    /// requests/reservations have retired. Request IDs never restart on reuse.
    pub fn forget_variant(&self, object: u32) -> Result<(), ResourceError> {
        let mut state = self.shared.lock();
        let index = state.find(object).ok_or(ResourceError::UnknownObject)?;
        let row = state.entries[index].as_ref().expect("occupied prefix");
        if row.candidate.is_some()
            || row.live_requests != 0
            || row.live_reservations != 0
            || row.ready_reservation != 0
        {
            return Err(ResourceError::InvalidState);
        }
        let generation = state.next_generation()?;
        for cursor in index..state.count - 1 {
            state.entries[cursor] = state.entries[cursor + 1].take();
        }
        state.count -= 1;
        let count = state.count;
        state.entries[count] = None;
        state.generation = generation;
        Ok(())
    }

    pub fn retry_eligible(&self, object: u32) -> Result<bool, ResourceError> {
        let state = self.shared.lock();
        let index = state.find(object).ok_or(ResourceError::UnknownObject)?;
        Ok(self.shared.retry_eligible(
            &state,
            state.entries[index].as_ref().expect("occupied prefix"),
        ))
    }
}

impl Shared {
    pub(super) fn retry_eligible(&self, state: &State, row: &Entry) -> bool {
        let Some(failed) = row.capacity_failure else {
            return false;
        };
        if row.candidate.is_some()
            || !row
                .snapshot
                .failure
                .is_some_and(|failure| failure.kind == FailureKind::Limited)
        {
            return false;
        }
        let process = self.process.lock();
        let local = match failed.kind {
            ByteKind::Encoded => state.usage.encoded_bytes,
            ByteKind::Decoded => state.usage.decoded_bytes,
        };
        let global = match failed.kind {
            ByteKind::Encoded => process.encoded_bytes,
            ByteKind::Decoded => process.decoded_bytes,
        };
        process.generation > failed.generation
            && local
                .checked_add(failed.bytes)
                .is_some_and(|total| check_bytes(failed.kind, total).is_ok())
            && global
                .checked_add(failed.bytes)
                .is_some_and(|total| check_process_bytes(failed.kind, total).is_ok())
    }
}

#[cfg(test)]
mod tests;
