// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE file in the project root for license information.
// ---------------------------------------------------------------------------------------------

//! Fixed private platform observations. Not a product input or public API.

use crate::{STATUS_ABI_MISMATCH, STATUS_INVALID_ARGUMENT, STATUS_OK, contain_panic};
use std::collections::HashMap;
use std::sync::Arc;
use std::sync::atomic::{AtomicU32, Ordering};

pub const PLATFORM_VALIDATION_ABI_VERSION: u32 = 1;

#[repr(C, align(8))]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct PlatformValidationResult {
    pub abi_version: u32,
    pub struct_size: u32,
    pub rng_checked: u32,
    pub payload_status: i32,
    pub payload_drops: u32,
    pub payload_live: u32,
    pub payload_control_owners: u32,
    pub reserved: u32,
}

const _: () = {
    assert!(std::mem::size_of::<PlatformValidationResult>() == 32);
    assert!(std::mem::align_of::<PlatformValidationResult>() == 8);
    assert!(std::mem::offset_of!(PlatformValidationResult, abi_version) == 0);
    assert!(std::mem::offset_of!(PlatformValidationResult, struct_size) == 4);
    assert!(std::mem::offset_of!(PlatformValidationResult, rng_checked) == 8);
    assert!(std::mem::offset_of!(PlatformValidationResult, payload_status) == 12);
    assert!(std::mem::offset_of!(PlatformValidationResult, payload_drops) == 16);
    assert!(std::mem::offset_of!(PlatformValidationResult, payload_live) == 20);
    assert!(std::mem::offset_of!(PlatformValidationResult, payload_control_owners) == 24);
    assert!(std::mem::offset_of!(PlatformValidationResult, reserved) == 28);
};

struct DropWitness {
    drops: AtomicU32,
    live: AtomicU32,
}

struct NormalPayload(Arc<DropWitness>);

impl Drop for NormalPayload {
    fn drop(&mut self) {
        self.0.drops.fetch_add(1, Ordering::AcqRel);
        self.0.live.fetch_sub(1, Ordering::AcqRel);
    }
}

fn observe() -> PlatformValidationResult {
    // The default RandomState is intentional. No fixed seed or custom hasher.
    let mut map = HashMap::<u32, u32>::new();
    for (key, value) in [(1, 11), (2, 22), (3, 33), (4, 44)] {
        assert_eq!(map.insert(key, value), None);
    }
    assert_eq!(map.len(), 4);
    for (key, value) in [(1, 11), (2, 22), (3, 33), (4, 44)] {
        assert_eq!(map.get(&key), Some(&value));
    }
    assert_eq!(map.remove(&2), Some(22));
    assert_eq!(map.insert(3, 333), Some(33));
    assert_eq!(map.len(), 3);
    for (key, value) in [(1, 11), (3, 333), (4, 44)] {
        assert_eq!(map.get(&key), Some(&value));
    }
    assert_eq!(map.get(&2), None);
    drop(map);

    let control = Arc::new(DropWitness {
        drops: AtomicU32::new(0),
        live: AtomicU32::new(1),
    });
    let payload = NormalPayload(Arc::clone(&control));
    let payload_status = contain_panic(|| std::panic::panic_any(payload));
    let observed = PlatformValidationResult {
        abi_version: PLATFORM_VALIDATION_ABI_VERSION,
        struct_size: std::mem::size_of::<PlatformValidationResult>() as u32,
        rng_checked: 1,
        payload_status,
        payload_drops: control.drops.load(Ordering::Acquire),
        payload_live: control.live.load(Ordering::Acquire),
        payload_control_owners: Arc::strong_count(&control) as u32,
        reserved: 0,
    };
    // The payload and final control owner have retired before publication.
    drop(control);
    observed
}

/// # Safety
/// An aligned, non-null pointer must provide a readable eight-byte prefix.
/// A matching version/size commits the caller to a full, exclusive,
/// writable 32-byte result. Success initializes every field; failure preserves
/// storage and callers must not consume success fields. No pointer is retained.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn notepads_markdown_platform_validation_run(
    result: *mut PlatformValidationResult,
) -> i32 {
    if result.is_null() || !result.is_aligned() {
        return STATUS_INVALID_ARGUMENT;
    }
    contain_panic(|| {
        // SAFETY: only the caller-guaranteed prefix is read before acceptance.
        let version = unsafe { std::ptr::addr_of!((*result).abi_version).read() };
        // SAFETY: this is the second u32 of that same readable prefix.
        let size = unsafe { std::ptr::addr_of!((*result).struct_size).read() };
        if version != PLATFORM_VALIDATION_ABI_VERSION
            || size != std::mem::size_of::<PlatformValidationResult>() as u32
        {
            return STATUS_ABI_MISMATCH;
        }
        let observed = observe();
        // SAFETY: matching prefix commits to full writable storage. All work
        // and resources have retired; only a Copy POD remains to publish.
        unsafe { result.write(observed) };
        STATUS_OK
    })
}

#[cfg(test)]
mod tests;
