// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE file in the project root for license information.
// ---------------------------------------------------------------------------------------------

use super::*;
use crate::{STATUS_INVALID_ARGUMENT, STATUS_OK, STATUS_PANIC};
use std::mem::{MaybeUninit, align_of, size_of};

#[repr(C, align(8))]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct Prefix {
    version: u32,
    size: u32,
}

fn observe_once() -> PlatformValidationResult {
    let mut storage = MaybeUninit::<PlatformValidationResult>::uninit();
    let pointer = storage.as_mut_ptr();
    // SAFETY: the complete aligned allocation exists. Only its prefix is read
    // by Rust until the successful call writes every field.
    unsafe {
        std::ptr::addr_of_mut!((*pointer).abi_version).write(PLATFORM_VALIDATION_ABI_VERSION);
        std::ptr::addr_of_mut!((*pointer).struct_size).write(32);
        assert_eq!(
            notepads_markdown_platform_validation_run(pointer),
            STATUS_OK
        );
        storage.assume_init()
    }
}

fn require_observations(result: PlatformValidationResult) {
    assert_eq!(result.abi_version, 1);
    assert_eq!(result.struct_size, 32);
    assert_eq!(result.rng_checked, 1);
    assert_eq!(result.payload_status, STATUS_PANIC);
    assert_eq!(result.payload_drops, 1);
    assert_eq!(result.payload_live, 0);
    assert_eq!(result.payload_control_owners, 1);
    assert_eq!(result.reserved, 0);
}

#[test]
fn layout() {
    assert_eq!(
        (
            size_of::<PlatformValidationResult>(),
            align_of::<PlatformValidationResult>()
        ),
        (32, 8)
    );
    assert_eq!((size_of::<Prefix>(), align_of::<Prefix>()), (8, 8));
    let observed = observe_once();
    // Eight initialized wire fields occupy exactly the documented 32 bytes.
    require_observations(observed);
}

#[test]
fn null() {
    // SAFETY: null is an explicitly handled argument, not dereferenced.
    assert_eq!(
        unsafe { notepads_markdown_platform_validation_run(std::ptr::null_mut()) },
        STATUS_INVALID_ARGUMENT
    );
}

#[test]
fn rejects_misaligned_result() {
    let mut storage = [0xa5a5a5a5a5a5a5a5u64; 5];
    let before = storage;
    // SAFETY: this pointer is within writable storage, but misaligned. The
    // entry point must reject it before any read or write.
    let pointer = unsafe { storage.as_mut_ptr().cast::<u8>().add(4).cast() };
    assert_eq!(
        unsafe { notepads_markdown_platform_validation_run(pointer) },
        STATUS_INVALID_ARGUMENT
    );
    assert_eq!(storage, before);
}

#[test]
fn rejected_version_short_prefix() {
    let mut prefix = Prefix {
        version: 0,
        size: 32,
    };
    let before = prefix;
    // SAFETY: only an aligned readable 8B prefix is promised for rejected ABI.
    assert_eq!(
        unsafe { notepads_markdown_platform_validation_run((&mut prefix as *mut Prefix).cast()) },
        STATUS_ABI_MISMATCH
    );
    assert_eq!(prefix, before);
}

#[test]
fn rejected_size_short_prefix() {
    let mut prefix = Prefix {
        version: 1,
        size: 8,
    };
    let before = prefix;
    // SAFETY: a wrong size cannot authorize reading/writing the missing tail.
    assert_eq!(
        unsafe { notepads_markdown_platform_validation_run((&mut prefix as *mut Prefix).cast()) },
        STATUS_ABI_MISMATCH
    );
    assert_eq!(prefix, before);
}

#[test]
fn rejected_header_leaves_sentinel() {
    for (version, size) in [(0, 32), (1, 31)] {
        let mut result = PlatformValidationResult {
            abi_version: version,
            struct_size: size,
            rng_checked: 0xa5a5a5a5,
            payload_status: -12345,
            payload_drops: 12,
            payload_live: 13,
            payload_control_owners: 14,
            reserved: 15,
        };
        let before = result;
        // SAFETY: valid full storage; rejection must preserve every field.
        assert_eq!(
            unsafe { notepads_markdown_platform_validation_run(&mut result) },
            STATUS_ABI_MISMATCH
        );
        assert_eq!(result, before);
    }
}

#[test]
fn initialized_prefix_only_success() {
    require_observations(observe_once());
}

#[test]
fn repeated_calls_are_independent() {
    require_observations(observe_once());
    require_observations(observe_once());
}

#[test]
fn concurrent_calls_are_independent() {
    let left = std::thread::spawn(observe_once);
    let right = std::thread::spawn(observe_once);
    require_observations(left.join().expect("first independent FFI call"));
    require_observations(right.join().expect("second independent FFI call"));
}
