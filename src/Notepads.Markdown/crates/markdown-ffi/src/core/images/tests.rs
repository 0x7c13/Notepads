// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE file in the project root for license information.
// ---------------------------------------------------------------------------------------------

use super::*;

fn error() -> WireError {
    WireError {
        header: WireHeader::of::<WireError>(),
        ..Default::default()
    }
}
fn epoch(source: u64, provider: u64) -> ImageEpochDescriptor {
    ImageEpochDescriptor {
        header: ImageHeader::of::<ImageEpochDescriptor>(),
        epoch: ImageEpoch {
            source: WireSource {
                document: [7; 16],
                revision: source,
            },
            source_generation: source,
            provider_generation: provider,
        },
    }
}
fn info() -> ImageRequestInfo {
    ImageRequestInfo {
        header: ImageHeader::of::<ImageRequestInfo>(),
        ..Default::default()
    }
}
fn snapshot() -> ImageSnapshot {
    ImageSnapshot {
        header: ImageHeader::of::<ImageSnapshot>(),
        ..Default::default()
    }
}
struct Handles {
    pixels: Option<Vec<u8>>,
    bytes: *mut Owned<ImageBytes>,
    requests: [*mut Owned<ImageRequest>; 3],
    policy: *mut Owned<ImagePolicy>,
}
impl Default for Handles {
    fn default() -> Self {
        Self {
            pixels: None,
            bytes: ptr::null_mut(),
            requests: [ptr::null_mut(); 3],
            policy: ptr::null_mut(),
        }
    }
}
impl Drop for Handles {
    fn drop(&mut self) {
        self.pixels.take();
        // SAFETY: every stored handle is unique, live or null; native pixels retired above.
        unsafe {
            assert_eq!(notepads_markdown_image_bytes_destroy(self.bytes), 0);
            for request in self.requests {
                assert_eq!(notepads_markdown_image_request_destroy(request), 0);
            }
            assert_eq!(notepads_markdown_image_policy_destroy(self.policy), 0);
        }
    }
}
#[test]
fn image_abi_prefix_rejection_preserves_success_storage_and_policy() {
    let _process = crate::core::images::serialize_process_image_tests();
    let mut handles = Handles::default();
    let mut error = error();
    // SAFETY: guards own all handles; prefixes, stack PODs and buffers remain independent and live.
    unsafe {
        assert_eq!(notepads_markdown_image_abi_version(), IMAGE_ABI_VERSION);
        assert_eq!(notepads_markdown_image_policy_create(&mut handles.policy, &mut error), 0);
        // The complete descriptor address range is disjoint from the error.
        // Only its incompatible header is initialized.
        let mut only_prefix = std::mem::MaybeUninit::<ImageEpochDescriptor>::uninit();
        only_prefix.as_mut_ptr().cast::<ImageHeader>().write(ImageHeader {
            abi_version: 0,
            struct_size: size_of::<ImageEpochDescriptor>() as u32,
        });
        assert_eq!(
            notepads_markdown_image_policy_begin(
                handles.policy,
                only_prefix.as_ptr(),
                &mut error
            ),
            STATUS_ABI_MISMATCH
        );
        assert_eq!(
            notepads_markdown_image_policy_begin(handles.policy, &epoch(1, 1), &mut error),
            0
        );
        let mut output = info();
        output.header.abi_version = 0;
        output.key.request = 77;
        assert_eq!(
            notepads_markdown_image_policy_start(
                handles.policy,
                1,
                0,
                &mut handles.requests[0],
                &mut output,
                &mut error
            ),
            STATUS_ABI_MISMATCH
        );
        assert!(handles.requests[0].is_null());
        assert_eq!(output.key.request, 77);
        desire_image_for_test(handles.policy, 1, &mut error);
        output.header.abi_version = IMAGE_ABI_VERSION;
        assert_eq!(
            notepads_markdown_image_policy_start(
                handles.policy,
                1,
                0,
                &mut handles.requests[0],
                &mut output,
                &mut error
            ),
            0
        );
        assert_eq!(output.key.object, 1);
        assert_eq!(output.key.request, 1);
        let mut state = snapshot();
        let mut row = ImageEntry::default();
        assert_eq!(
            notepads_markdown_image_policy_read(
                handles.policy,
                &mut row,
                1,
                size_of::<ImageEntry>() as u32,
                &mut state,
                &mut error
            ),
            0
        );
        assert_eq!(state.active_requests, 1);
        assert_eq!(row.state, 1);
    }
}
#[test]
fn image_abi_cross_epoch_slots_bytes_and_last_owner_remain_paid() {
    let _process = crate::core::images::serialize_process_image_tests();
    let mut handles = Handles::default();
    let mut error = error();
    let mut output = info();
    // SAFETY: all pointers refer to independent valid storage guarded to the last use.
    unsafe {
        assert_eq!(notepads_markdown_image_policy_create(&mut handles.policy, &mut error), 0);
        assert_eq!(
            notepads_markdown_image_policy_begin(handles.policy, &epoch(1, 1), &mut error),
            0
        );
        for index in 0..2 {
            desire_image_for_test(handles.policy, index as u32, &mut error);
            assert_eq!(
                notepads_markdown_image_policy_start(
                    handles.policy,
                    index as u32,
                    0,
                    &mut handles.requests[index],
                    &mut output,
                    &mut error
                ),
                0
            );
        }
        desire_image_for_test(handles.policy, 2, &mut error);
        output.key.request = 99;
        assert_eq!(
            notepads_markdown_image_policy_start(
                handles.policy,
                2,
                0,
                &mut handles.requests[2],
                &mut output,
                &mut error
            ),
            9
        );
        assert_eq!(error.limit_kind, 19);
        assert_eq!(output.key.request, 99);
        assert!(handles.requests[2].is_null());
        assert_eq!(
            notepads_markdown_image_request_reserve(handles.requests[0], 1, 16, &mut handles.bytes, &mut error),
            0
        );
        handles.pixels = Some(vec![0_u8; 16]);
        assert_eq!(
            notepads_markdown_image_bytes_admit(
                handles.bytes,
                handles.pixels.as_ref().unwrap().capacity() as u64,
                &mut error
            ),
            0
        );
        let ready = ImageCompletion {
            header: ImageHeader::of::<ImageCompletion>(),
            metadata: ImageMetadataWire {
                intrinsic_width: 2,
                intrinsic_height: 2,
                decoded_width: 2,
                decoded_height: 2,
                orientation: 1,
                stride: 8,
                natural_width_dip: 2.0,
                natural_height_dip: 2.0,
                source_width: 2.0,
                source_height: 2.0,
                ..Default::default()
            },
            target: image_test_target(),
            ready: 1,
            ..Default::default()
        };
        assert_eq!(
            notepads_markdown_image_policy_complete(
                handles.policy,
                handles.requests[0],
                &ready,
                handles.bytes,
                &mut error
            ),
            0
        );
        assert_eq!(
            notepads_markdown_image_policy_complete(
                handles.policy,
                handles.requests[0],
                &ready,
                handles.bytes,
                &mut error
            ),
            5
        );
        assert_eq!(
            notepads_markdown_image_policy_begin(handles.policy, &epoch(2, 1), &mut error),
            0
        );
        assert_eq!(
            notepads_markdown_image_policy_complete(
                handles.policy,
                handles.requests[0],
                &ready,
                handles.bytes,
                &mut error
            ),
            10
        );
        let mut no_bytes = ptr::null_mut();
        assert_eq!(
            notepads_markdown_image_request_reserve(handles.requests[0], 0, 10, &mut no_bytes, &mut error),
            10
        );
        assert!(no_bytes.is_null());
        let mut state = snapshot();
        assert_eq!(
            notepads_markdown_image_policy_read(
                handles.policy,
                ptr::null_mut(),
                0,
                size_of::<ImageEntry>() as u32,
                &mut state,
                &mut error
            ),
            0
        );
        assert_eq!(state.count, 0);
        assert_eq!(state.active_requests, 2);
        assert_eq!(state.decoded_bytes, 16);
        assert_eq!(notepads_markdown_image_policy_destroy(handles.policy), 0);
        handles.policy = ptr::null_mut();
        // The policy handle may retire first: both request and bytes owners still fund its table.
        let byte_owner = &(*handles.bytes).value.owner;
        let request_owner = &(*handles.requests[1]).value.owner;
        assert_eq!(byte_owner.policy.usage().decoded_bytes, 16);
        assert_eq!(request_owner.policy.usage().active_requests, 2);
    }
}
#[test]
fn image_abi_snapshot_and_bad_completion_are_atomic_and_panic_gate_is_shared() {
    let _process = crate::core::images::serialize_process_image_tests();
    let mut handles = Handles::default();
    let mut error = error();
    let mut output = info();
    // SAFETY: valid independent prefix/full buffers and uniquely guarded handles.
    unsafe {
        assert_eq!(notepads_markdown_image_policy_create(&mut handles.policy, &mut error), 0);
        assert_eq!(
            notepads_markdown_image_policy_begin(handles.policy, &epoch(1, 1), &mut error),
            0
        );
        desire_image_for_test(handles.policy, 7, &mut error);
        assert_eq!(
            notepads_markdown_image_policy_start(
                handles.policy,
                7,
                0,
                &mut handles.requests[0],
                &mut output,
                &mut error
            ),
            0
        );
        let mut state = snapshot();
        state.state_generation = 99;
        assert_eq!(
            notepads_markdown_image_policy_read(
                handles.policy,
                ptr::null_mut(),
                0,
                size_of::<ImageEntry>() as u32,
                &mut state,
                &mut error
            ),
            9
        );
        assert_eq!(state.state_generation, 99);
        assert_eq!(error.requested, 1);
        assert_eq!(error.limit, 0);
        let completion = ImageCompletion {
            header: ImageHeader::of::<ImageCompletion>(),
            target: output.target,
            ready: 0,
            failure: 99,
            ..Default::default()
        };
        assert_eq!(
            notepads_markdown_image_policy_complete(
                handles.policy,
                handles.requests[0],
                &completion,
                ptr::null(),
                &mut error
            ),
            16
        );
        let mut row = ImageEntry::default();
        assert_eq!(
            notepads_markdown_image_policy_read(
                handles.policy,
                &mut row,
                1,
                size_of::<ImageEntry>() as u32,
                &mut state,
                &mut error
            ),
            0
        );
        assert_eq!(row.state, 1);
        assert_eq!(state.active_requests, 1);
        let policy_owner = &(*handles.policy).value.owner;
        policy_owner.poisoned.store(true, Ordering::Release);
        assert_eq!(
            notepads_markdown_image_policy_begin(handles.policy, &epoch(2, 1), &mut error),
            5
        );
        let mut denied = ptr::null_mut();
        assert_eq!(
            notepads_markdown_image_request_reserve(handles.requests[0], 0, 1, &mut denied, &mut error),
            5
        );
        assert!(denied.is_null());
    }
}

#[test]
fn image_abi_concurrent_call_cannot_publish_after_contained_panic() {
    let _process = crate::core::images::serialize_process_image_tests();
    use std::sync::{Barrier, mpsc};
    let mut handles = Handles::default();
    let mut wire_error = error();
    // SAFETY: uniquely guarded handle output and independent full error buffer.
    assert_eq!(
        unsafe { notepads_markdown_image_policy_create(&mut handles.policy, &mut wire_error) },
        0
    );
    // SAFETY: live policy, cloned owner outlives all scoped worker calls.
    let owner = unsafe { (*handles.policy).value.owner.clone() };
    let entered = Barrier::new(2);
    let release_panic = Barrier::new(2);
    let (attempting, attempted) = mpsc::channel();
    let ran = AtomicBool::new(false);
    std::thread::scope(|scope| {
        let first = scope.spawn(|| {
            let mut output = error();
            // SAFETY: independent writable error buffer; closure uses no FFI pointers.
            unsafe {
                image_action(&owner, &mut output, |_, _| {
                    entered.wait();
                    release_panic.wait();
                    panic!("controlled image action panic");
                })
            }
        });
        entered.wait();
        assert!(matches!(
            owner.call_gate.try_lock(),
            Err(std::sync::TryLockError::WouldBlock)
        ));
        let second = scope.spawn(|| {
            let mut output = error();
            attempting.send(()).unwrap();
            // SAFETY: independent writable error buffer, same valid shared owner.
            unsafe {
                image_action(&owner, &mut output, |owner, _| {
                    ran.store(true, Ordering::Release);
                    owner
                        .policy
                        .begin_source(from_epoch(epoch(1, 1).epoch)?)
                        .map_err(resource_error)
                })
            }
        });
        attempted.recv().unwrap();
        release_panic.wait();
        assert_eq!(first.join().unwrap(), STATUS_PANIC);
        assert_eq!(second.join().unwrap(), 5);
    });
    assert!(!ran.load(Ordering::Acquire));
    assert!(owner.poisoned.load(Ordering::Acquire));
    let mut rows = [EntrySnapshot::default(); 1];
    let state = owner.policy.snapshot(&mut rows).unwrap();
    assert!(state.epoch.is_none());
}
