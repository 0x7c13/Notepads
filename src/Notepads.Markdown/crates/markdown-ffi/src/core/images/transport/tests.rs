// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE file in the project root for license information.
// ---------------------------------------------------------------------------------------------

use super::*;

#[test]
fn generation_only_matches_snapshot_before_and_after_publication_changes() {
    let _process = crate::core::images::serialize_process_image_tests();
    let mut fixture = Fixture::new();
    let read = |policy| {
        let mut generation = u64::MAX;
        // SAFETY: fixture owns the policy and both outputs are disjoint.
        let status = unsafe {
            notepads_markdown_image_policy_generation(policy, &mut generation, &mut error())
        };
        assert_eq!(status, 0);
        generation
    };
    let before = read(fixture.policy);
    assert_eq!(before, fixture.read().0.state_generation);
    fixture.desire(1, 1, 1.0);
    let _ = fixture.start(1, 2);
    let after = read(fixture.policy);
    assert_eq!(after, fixture.read().0.state_generation);
    assert!(after > before);
}

#[test]
fn reused_wire_requires_fresh_authority_and_retires_every_duplicate_binding() {
    let _process = crate::core::images::serialize_process_image_tests();
    let mut fixture = Fixture::new();
    // SAFETY: fixture owns all policy/request/byte handles and full disjoint PODs.
    unsafe {
        fixture.desire(1, 1, 1.0);
        let (birth_request, birth_info) = fixture.start(1, 2);
        let metadata = wire_metadata(
            ImageMetadata {
                intrinsic_width: 4096,
                intrinsic_height: 4096,
                decoded_width: 128,
                decoded_height: 128,
                orientation: 1,
                stride: 512,
            }
            .into(),
        );
        let bytes = fixture.decoded(birth_request, 128 * 128 * 4);
        let mut completion = ImageCompletion {
            header: ImageHeader::of::<ImageCompletion>(),
            metadata,
            target: birth_info.target,
            ready: 1,
            ..Default::default()
        };
        assert_eq!(
            notepads_markdown_image_policy_complete(
                fixture.policy,
                birth_request,
                &completion,
                bytes,
                &mut error()
            ),
            0
        );
        for request in fixture.requests.drain(..) {
            assert_eq!(notepads_markdown_image_request_destroy(request), 0);
        }
        let mut next_epoch = epoch(2);
        next_epoch.epoch.source.revision = 2;
        next_epoch.epoch.source_generation = 2;
        assert_eq!(
            notepads_markdown_image_policy_begin(fixture.policy, &next_epoch, &mut error()),
            0
        );
        for object in [7, 8] {
            fixture.desire(object, 2, 1.0);
            let (request, info) = fixture.start(object, 2);
            completion.target = info.target;
            assert_ne!(
                notepads_markdown_image_policy_complete(
                    fixture.policy,
                    request,
                    &completion,
                    bytes,
                    &mut error()
                ),
                0
            );
            completion.target.target_generation += 1;
            assert_ne!(
                notepads_markdown_image_policy_complete_reused(
                    fixture.policy,
                    request,
                    &completion,
                    bytes,
                    &mut error()
                ),
                0
            );
            completion.target = info.target;
            assert_eq!(
                notepads_markdown_image_policy_complete_reused(
                    fixture.policy,
                    request,
                    &completion,
                    bytes,
                    &mut error()
                ),
                0
            );
            let (snapshot, rows) = fixture.read();
            assert_eq!(snapshot.decoded_bytes, 128 * 128 * 4);
            assert_eq!(
                rows.iter()
                    .find(|row| row.object == object)
                    .unwrap()
                    .published_request,
                info.key.request
            );
        }
        assert_eq!(
            fixture.read().1.iter().filter(|row| row.state == 2).count(),
            2
        );
        for request in fixture.requests.drain(..) {
            assert_eq!(notepads_markdown_image_request_destroy(request), 0);
        }
        for (pixels, lease) in fixture.pixels.drain(..) {
            drop(pixels);
            assert_eq!(notepads_markdown_image_bytes_destroy(lease), 0);
        }
        let (snapshot, rows) = fixture.read();
        assert_eq!(snapshot.decoded_bytes, 0);
        assert!(
            rows.iter().all(|row| row.state == 0
                && row.published_request == 0
                && row.status_flags & 8 != 0)
        );
        for object in [7, 8] {
            assert_eq!(
                notepads_markdown_image_policy_forget(fixture.policy, object, &mut error()),
                0
            );
        }
    }
}

#[test]
fn reused_wire_rejects_foreign_payload_and_overlapping_storage() {
    let _process = crate::core::images::serialize_process_image_tests();
    let mut foreign = Fixture::new();
    foreign.desire(1, 1, 1.0);
    let (foreign_request, foreign_info) = foreign.start(1, 2);
    let bytes = foreign.decoded(foreign_request, 64 * 32 * 4);
    let completion = ImageCompletion {
        header: ImageHeader::of::<ImageCompletion>(),
        metadata: svg_metadata(1),
        target: foreign_info.target,
        ready: 1,
        ..Default::default()
    };
    let mut current = Fixture::new();
    // SAFETY: all live handles and PODs are fixture-owned. The one deliberate
    // overlap is rejected before dereferencing or writing either typed value.
    unsafe {
        current.desire(1, 1, 1.0);
        let (request, _) = current.start(1, 2);
        let before = current.read().1[0].pending_request;
        assert_ne!(
            notepads_markdown_image_policy_complete_reused(
                current.policy,
                request,
                &completion,
                bytes,
                &mut error()
            ),
            0
        );
        let pointer = ptr::addr_of!(completion);
        assert_eq!(
            notepads_markdown_image_policy_complete_reused(
                current.policy,
                request,
                pointer,
                bytes,
                pointer.cast_mut().cast()
            ),
            STATUS_INVALID_ARGUMENT
        );
        assert_eq!(current.read().1[0].pending_request, before);
    }
}

#[test]
fn windowed_wire_metadata_is_not_ready_and_eviction_keeps_physical_charge_until_destroy() {
    let _process = crate::core::images::serialize_process_image_tests();
    let mut fixture = Fixture::new();
    // SAFETY: fixture owns live policy, requests and admitted buffers.
    unsafe {
        fixture.desire(7, 1, 1.0);
        let (request, info) = fixture.start(7, 2);
        let metadata = wire_metadata(
            ImageMetadata {
                intrinsic_width: 4096,
                intrinsic_height: 2048,
                decoded_width: 128,
                decoded_height: 64,
                orientation: 1,
                stride: 512,
            }
            .into(),
        );
        let mut plan = ImageRasterPlan {
            header: ImageHeader::of::<ImageRasterPlan>(),
            ..Default::default()
        };
        assert_eq!(
            notepads_markdown_image_plan_raster(&metadata, &info.target, &mut plan, &mut error()),
            0
        );
        assert_eq!(
            (
                plan.width,
                plan.height,
                plan.stride,
                plan.maximum_side,
                plan.bytes
            ),
            (128, 64, 512, 128, 32768)
        );
        assert_eq!(
            notepads_markdown_image_request_note_metadata(request, &metadata, &mut error()),
            0
        );
        let (_, rows) = fixture.read();
        assert_eq!(rows[0].state, 1);
        assert_eq!(rows[0].published_request, 0);
        assert_ne!(rows[0].status_flags & 8, 0);
        let bytes = fixture.decoded(request, plan.bytes as usize);
        let completion = ImageCompletion {
            header: ImageHeader::of::<ImageCompletion>(),
            metadata,
            target: info.target,
            ready: 1,
            ..Default::default()
        };
        assert_eq!(
            notepads_markdown_image_policy_complete(
                fixture.policy,
                request,
                &completion,
                bytes,
                &mut error()
            ),
            0
        );
        let before = fixture.read().0;
        assert_eq!(before.decoded_bytes, plan.bytes);
        assert_eq!(
            notepads_markdown_image_policy_evict(fixture.policy, 7, &mut error()),
            0
        );
        let (evicted, rows) = fixture.read();
        assert_eq!(evicted.decoded_bytes, plan.bytes);
        assert_eq!((rows[0].state, rows[0].published_request), (0, 0));
        assert_ne!(rows[0].status_flags & 8, 0);
        assert_ne!(
            notepads_markdown_image_policy_forget(fixture.policy, 7, &mut error()),
            0
        );
        for (pixels, lease) in fixture.pixels.drain(..) {
            drop(pixels);
            assert_eq!(notepads_markdown_image_bytes_destroy(lease), 0);
        }
        for request in fixture.requests.drain(..) {
            assert_eq!(notepads_markdown_image_request_destroy(request), 0);
        }
        assert_eq!(fixture.read().0.decoded_bytes, 0);
        assert_eq!(
            notepads_markdown_image_policy_forget(fixture.policy, 7, &mut error()),
            0
        );
        assert!(fixture.read().1.is_empty());
    }
}

#[test]
fn windowed_additive_pods_reject_bad_prefix_and_aliases_without_writing_outputs() {
    let _process = crate::core::images::serialize_process_image_tests();
    assert_eq!(size_of::<ImageRasterPlan>(), 32);
    assert_eq!(size_of::<ImageProcessCapacity>(), 32);
    assert_eq!(std::mem::offset_of!(ImageRasterPlan, bytes), 24);
    let mut capacity = ImageProcessCapacity {
        header: ImageHeader::of::<ImageProcessCapacity>(),
        decoded_bytes: u64::MAX,
        ..Default::default()
    };
    let mut diagnostic = error();
    // SAFETY: full independent PODs; the intentionally overlapping error argument
    // is rejected by interval preflight before any typed dereference or write.
    unsafe {
        capacity.header.struct_size = 8;
        assert_eq!(
            notepads_markdown_image_get_process_capacity(&mut capacity, &mut diagnostic),
            STATUS_ABI_MISMATCH
        );
        assert_eq!(capacity.decoded_bytes, u64::MAX);
        capacity.header = ImageHeader::of::<ImageProcessCapacity>();
        let pointer = ptr::addr_of_mut!(capacity);
        assert_eq!(
            notepads_markdown_image_get_process_capacity(pointer, pointer.cast()),
            STATUS_INVALID_ARGUMENT
        );
        assert_eq!(capacity.decoded_bytes, u64::MAX);
        diagnostic.header.struct_size = 0;
        assert_eq!(
            notepads_markdown_image_get_process_capacity(&mut capacity, &mut diagnostic),
            STATUS_ABI_MISMATCH
        );
        assert_eq!(capacity.decoded_bytes, u64::MAX);
        assert_eq!(
            notepads_markdown_image_get_process_capacity(&mut capacity, &mut error()),
            0
        );
    }
}

#[test]
fn svg_structural_failure_detail_survives_completion_and_snapshot_while_unknown_is_rejected() {
    let _process = crate::core::images::serialize_process_image_tests();
    for limit in [14, 15] {
        let mut fixture = Fixture::new();
        fixture.desire(1, 1, 1.0);
        let (request, info) = fixture.start(1, 2);
        let mut completion = ImageCompletion {
            header: ImageHeader::of::<ImageCompletion>(),
            target: info.target,
            failure: 6,
            error: -9,
            detail: (2 << 16) | 16,
            ..Default::default()
        };
        // SAFETY: fixture retains live policy/permit handles; completion and
        // error are separate valid PODs. Failure has no decoded allocation.
        unsafe {
            assert_eq!(
                notepads_markdown_image_policy_complete(
                    fixture.policy,
                    request,
                    &completion,
                    ptr::null_mut(),
                    &mut error()
                ),
                wire_error(CoreError::InvalidRange).status
            );
            assert_ne!(fixture.read().1[0].pending_request, 0);
            completion.detail = (2 << 16) | limit;
            assert_eq!(
                notepads_markdown_image_policy_complete(
                    fixture.policy,
                    request,
                    &completion,
                    ptr::null_mut(),
                    &mut error()
                ),
                0
            );
        }
        let entry = fixture.read().1[0];
        assert_eq!(
            (entry.failure, entry.error, entry.detail),
            (completion.failure, -9, completion.detail)
        );
        assert_eq!(entry.pending_request, 0);
    }
}

#[test]
fn completed_source_failure_does_not_advertise_a_geometry_retry_on_wire() {
    let _process = crate::core::images::serialize_process_image_tests();
    let svg_decode = ((markdown_svg_budget::FailureKind::Decode as u32) << 16)
        | markdown_svg_budget::LimitKind::None as u32;
    for (failure, detail) in [(1, 0), (5, 0), (4, svg_decode)] {
        let mut fixture = Fixture::new();
        fixture.desire(1, 1, 1.0);
        let (request, info) = fixture.start(1, 2);
        let completion = ImageCompletion {
            header: ImageHeader::of::<ImageCompletion>(),
            target: info.target,
            failure,
            error: -9,
            detail,
            ..Default::default()
        };
        // SAFETY: Fixture owns the independent policy/permit; all PODs and
        // outputs are valid disjoint storage, with no decoded lease on failure.
        unsafe {
            assert_eq!(
                notepads_markdown_image_policy_complete(
                    fixture.policy,
                    request,
                    &completion,
                    ptr::null_mut(),
                    &mut error(),
                ),
                0
            );
        }
        let (before, rows) = fixture.read();
        let original = rows[0];
        fixture.desire(1, 2, 2.0);
        let (after, rows) = fixture.read();
        assert_eq!(before.state_generation, after.state_generation);
        assert_eq!(rows[0].desired_target, info.target);
        assert_eq!(rows[0].state, original.state);
        assert_eq!(rows[0].failure, failure);
        assert_eq!(rows[0].detail, detail);
        assert_eq!(rows[0].error, -9);
        assert_eq!(rows[0].pending_request, 0);
        assert_eq!(rows[0].status_flags & 1, 0, "NEEDS_RENDER must remain off");
        if failure == 4 {
            let mut font = desired(2, 2.0);
            font.target.font_generation = 2;
            // SAFETY: same live Fixture and complete independent POD storage.
            unsafe {
                assert_eq!(
                    notepads_markdown_image_policy_desire(fixture.policy, 1, &font, &mut error()),
                    0
                );
            }
            assert_eq!(fixture.read().1[0].status_flags & 1, 1);
        }
    }
}

fn error() -> WireError {
    WireError {
        header: WireHeader::of::<WireError>(),
        ..Default::default()
    }
}
fn desired(generation: u64, scale: f64) -> ImageDesired {
    ImageDesired {
        header: ImageHeader::of::<ImageDesired>(),
        target: ImageTargetWire {
            font_generation: 1,
            target_generation: generation,
            available_width_dip: 64.0,
            effective_physical_scale: scale,
        },
    }
}
fn epoch(version: u32) -> ImageEpochDescriptor {
    ImageEpochDescriptor {
        header: ImageHeader {
            abi_version: version,
            struct_size: size_of::<ImageEpochDescriptor>() as u32,
        },
        epoch: ImageEpoch {
            source: WireSource {
                document: [31; 16],
                revision: 1,
            },
            source_generation: 1,
            provider_generation: 1,
        },
    }
}
fn svg_metadata(scale: u32) -> ImageMetadataWire {
    ImageMetadataWire {
        kind: 1,
        decoded_width: 64 * scale,
        decoded_height: 32 * scale,
        orientation: 1,
        stride: 256 * scale,
        natural_width_dip: 64.0,
        natural_height_dip: 32.0,
        source_width: f64::from(64 * scale),
        source_height: f64::from(32 * scale),
        viewport_width_bits: 64f32.to_bits(),
        viewport_height_bits: 32f32.to_bits(),
        scale_bits: (scale as f32).to_bits(),
        ..Default::default()
    }
}
struct Fixture {
    policy: *mut Owned<ImagePolicy>,
    requests: Vec<*mut Owned<ImageRequest>>,
    pixels: Vec<(Vec<u8>, *mut Owned<ImageBytes>)>,
}
impl Fixture {
    fn new() -> Self {
        let mut result = Self {
            policy: ptr::null_mut(),
            requests: Vec::new(),
            pixels: Vec::new(),
        };
        // SAFETY: independent full initialized storage; Fixture owns returned handles.
        unsafe {
            assert_eq!(
                notepads_markdown_image_policy_create(&mut result.policy, &mut error()),
                0
            );
            assert_eq!(
                notepads_markdown_image_policy_begin(result.policy, &epoch(2), &mut error()),
                0
            );
        }
        result
    }
    fn desire(&self, object: u32, generation: u64, scale: f64) {
        // SAFETY: live independent policy, input and error storage.
        unsafe {
            assert_eq!(
                notepads_markdown_image_policy_desire(
                    self.policy,
                    object,
                    &desired(generation, scale),
                    &mut error()
                ),
                0
            );
        }
    }
    fn start(&mut self, object: u32, kind: u32) -> (*mut Owned<ImageRequest>, ImageRequestInfo) {
        let mut request = ptr::null_mut();
        let mut info = ImageRequestInfo {
            header: ImageHeader::of::<ImageRequestInfo>(),
            ..Default::default()
        };
        // SAFETY: live handles/valid nonoverlapping full outputs.
        unsafe {
            assert_eq!(
                notepads_markdown_image_policy_start(
                    self.policy,
                    object,
                    kind,
                    &mut request,
                    &mut info,
                    &mut error()
                ),
                0
            );
        }
        self.requests.push(request);
        (request, info)
    }
    fn decoded(
        &mut self,
        request: *mut Owned<ImageRequest>,
        size: usize,
    ) -> *mut Owned<ImageBytes> {
        let mut lease = ptr::null_mut();
        // SAFETY: Fixture keeps permit and actual initialized payload alive with its lease.
        unsafe {
            assert_eq!(
                notepads_markdown_image_request_reserve(
                    request,
                    1,
                    size as u64,
                    &mut lease,
                    &mut error()
                ),
                0
            );
            let pixels = vec![0; size];
            assert_eq!(
                notepads_markdown_image_bytes_admit(lease, pixels.capacity() as u64, &mut error()),
                0
            );
            self.pixels.push((pixels, lease));
        }
        lease
    }
    fn read(&self) -> (ImageSnapshot, Vec<ImageEntry>) {
        let mut result = ImageSnapshot {
            header: ImageHeader::of::<ImageSnapshot>(),
            ..Default::default()
        };
        let mut rows = vec![ImageEntry::default(); resources::MAX_ENTRIES];
        // SAFETY: live policy, aligned valid complete independent output buffers.
        unsafe {
            assert_eq!(
                notepads_markdown_image_policy_read(
                    self.policy,
                    rows.as_mut_ptr(),
                    rows.len() as u32,
                    size_of::<ImageEntry>() as u32,
                    &mut result,
                    &mut error()
                ),
                0
            );
        }
        rows.truncate(result.count as usize);
        (result, rows)
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        // SAFETY: actual payloads free before matching leases; handles are uniquely owned.
        unsafe {
            for (pixels, lease) in self.pixels.drain(..) {
                drop(pixels);
                assert_eq!(notepads_markdown_image_bytes_destroy(lease), 0);
            }
            for request in self.requests.drain(..) {
                assert_eq!(notepads_markdown_image_request_destroy(request), 0);
            }
            assert_eq!(notepads_markdown_image_policy_destroy(self.policy), 0);
        }
    }
}

#[test]
fn svg_replacement_preserves_old_pixels_and_exact_target_identity() {
    let _process = crate::core::images::serialize_process_image_tests();
    let mut f = Fixture::new();
    f.desire(1, 1, 1.0);
    let (old, info) = f.start(1, 1);
    let published_request = info.key.request;
    let bytes = f.decoded(old, 8192);
    let first = ImageCompletion {
        header: ImageHeader::of::<ImageCompletion>(),
        metadata: svg_metadata(1),
        target: info.target,
        ready: 1,
        ..Default::default()
    };
    // SAFETY: the Fixture owns independently live policy/request/lease and valid PODs.
    unsafe {
        assert_eq!(
            notepads_markdown_image_policy_complete(f.policy, old, &first, bytes, &mut error()),
            0
        );
    }
    f.desire(1, 2, 2.0);
    let (new, info) = f.start(1, 1);
    let pending = f.read();
    assert_eq!(pending.1[0].state, 2);
    assert_eq!(pending.1[0].published_request, published_request);
    assert_eq!(pending.1[0].pending_request, info.key.request);
    let fail = ImageCompletion {
        header: ImageHeader::of::<ImageCompletion>(),
        target: info.target,
        failure: 6,
        error: -9,
        detail: (2 << 16) | 1,
        ..Default::default()
    };
    unsafe {
        assert_eq!(
            notepads_markdown_image_policy_complete(
                f.policy,
                new,
                &fail,
                ptr::null(),
                &mut error()
            ),
            0
        );
    }
    let failed = f.read();
    assert_eq!(failed.1[0].state, 2);
    assert_eq!(failed.1[0].metadata, first.metadata);
    assert_eq!(failed.1[0].failure, 6);
    assert_eq!(failed.1[0].detail, (2 << 16) | 1);
    assert_eq!(failed.1[0].status_flags & 1, 0);
}

#[test]
fn short_and_incompatible_prefixes_never_mutate_the_table() {
    let _process = crate::core::images::serialize_process_image_tests();
    let f = Fixture::new();
    let before = f.read().0.state_generation;
    // Reserve disjoint storage for the descriptor range used in alias preflight.
    // SAFETY: only the short prefix is initialized, and it cannot authorize a full read.
    unsafe {
        let mut short = std::mem::MaybeUninit::<ImageDesired>::uninit();
        short
            .as_mut_ptr()
            .cast::<ImageHeader>()
            .write(ImageHeader::of::<ImageHeader>());
        assert_eq!(
            notepads_markdown_image_policy_desire(
                f.policy,
                1,
                short.as_ptr(),
                &mut error()
            ),
            STATUS_ABI_MISMATCH
        );
        let mut info = ImageRequestInfo {
            header: ImageHeader {
                abi_version: 0,
                struct_size: size_of::<ImageRequestInfo>() as u32,
            },
            ..Default::default()
        };
        let sentinel = 1usize as *mut Owned<ImageRequest>;
        let mut request = sentinel;
        assert_eq!(
            notepads_markdown_image_policy_start(
                f.policy,
                1,
                1,
                &mut request,
                &mut info,
                &mut error()
            ),
            STATUS_ABI_MISMATCH
        );
        assert_eq!(request, sentinel);
    }
    assert_eq!(f.read().0.state_generation, before);
}

fn resource_status(value: CoreError) -> i32 {
    let mut output = error();
    // SAFETY: fully initialized independent error POD.
    unsafe { action(&mut output, || Err(value)) }
}

#[test]
fn alias_preflight_rejects_before_error_handle_or_success_writes() {
    let _process = crate::core::images::serialize_process_image_tests();
    let mut create_error = error();
    let original = create_error.header;
    let shared: *mut WireError = &mut create_error;
    // SAFETY: the creator rejects overlapping outputs before accessing either one.
    assert_eq!(
        unsafe { notepads_markdown_image_policy_create(shared.cast(), shared) },
        STATUS_INVALID_ARGUMENT
    );
    assert_eq!(create_error.header.abi_version, original.abi_version);
    assert_eq!(create_error.header.struct_size, original.struct_size);

    let mut f = Fixture::new();
    f.desire(1, 1, 1.0);
    let (request, info) = f.start(1, 1);
    let mut completion = ImageCompletion {
        header: ImageHeader::of::<ImageCompletion>(),
        target: info.target,
        failure: 2,
        ..Default::default()
    };
    let original = completion.header;
    // SAFETY: numerical alias preflight rejects before any dereference/write.
    unsafe {
        assert_eq!(
            notepads_markdown_image_policy_complete(
                f.policy,
                request,
                &completion,
                ptr::null(),
                (&raw mut completion).cast()
            ),
            STATUS_INVALID_ARGUMENT
        );
        assert_eq!(completion.header.abi_version, original.abi_version);
        let mut snapshot = ImageSnapshot {
            header: ImageHeader::of::<ImageSnapshot>(),
            ..Default::default()
        };
        assert_eq!(
            notepads_markdown_image_policy_read(
                f.policy,
                (&raw mut snapshot).cast(),
                1,
                192,
                &mut snapshot,
                &mut error()
            ),
            STATUS_INVALID_ARGUMENT
        );
        let old = f.policy;
        assert_eq!(
            notepads_markdown_image_policy_start(
                f.policy,
                2,
                0,
                &mut f.policy as *mut *mut Owned<ImagePolicy> as *mut *mut Owned<ImageRequest>,
                (&raw mut f.policy).cast(),
                &mut error()
            ),
            STATUS_INVALID_ARGUMENT
        );
        assert_eq!(f.policy, old);
    }
    assert_eq!(f.read().1[0].pending_request, info.key.request);
}

#[test]
fn malformed_geometry_and_target_do_not_publish_or_lose_candidate() {
    let _process = crate::core::images::serialize_process_image_tests();
    let mut f = Fixture::new();
    f.desire(1, 1, 2.0);
    let (request, info) = f.start(1, 1);
    let bytes = f.decoded(request, 32768);
    let valid = ImageCompletion {
        header: ImageHeader::of::<ImageCompletion>(),
        metadata: svg_metadata(2),
        target: info.target,
        ready: 1,
        ..Default::default()
    };
    let generation = f.read().0.state_generation;
    let mut cases = Vec::new();
    for bad in [f64::NAN, f64::INFINITY, 0.0, -1.0, 127.5] {
        let mut value = valid;
        value.metadata.source_width = bad;
        cases.push(value);
    }
    let mut value = valid;
    value.metadata.flags = 2;
    cases.push(value);
    let mut value = valid;
    value.metadata.reserved = 1;
    cases.push(value);
    let mut value = valid;
    value.metadata.orientation = 2;
    cases.push(value);
    let mut value = valid;
    value.metadata.decoded_width += 1;
    cases.push(value);
    let mut value = valid;
    value.target.target_generation += 1;
    cases.push(value);
    for bad in cases {
        // SAFETY: valid independent full PODs and live pinned handles.
        unsafe {
            assert_ne!(
                notepads_markdown_image_policy_complete(
                    f.policy,
                    request,
                    &bad,
                    bytes,
                    &mut error()
                ),
                0
            );
        }
        assert_eq!(f.read().0.state_generation, generation);
        assert_eq!(f.read().1[0].pending_request, info.key.request);
    }
    unsafe {
        assert_eq!(
            notepads_markdown_image_policy_complete(f.policy, request, &valid, bytes, &mut error()),
            0
        );
    }
    assert_eq!(f.read().1[0].status_flags & 2, 2);
}

#[test]
fn raster_wire_validates_exif_geometry_and_captured_target() {
    let _process = crate::core::images::serialize_process_image_tests();
    let mut f = Fixture::new();
    f.desire(1, 1, 1.0);
    let (request, info) = f.start(1, 0);
    let bytes = f.decoded(request, 48);
    let raster = ImageMetadata {
        intrinsic_width: 4,
        intrinsic_height: 3,
        decoded_width: 3,
        decoded_height: 4,
        orientation: 6,
        stride: 12,
    };
    let mut completion = ImageCompletion {
        header: ImageHeader::of::<ImageCompletion>(),
        metadata: wire_metadata(raster.into()),
        target: info.target,
        ready: 1,
        ..Default::default()
    };
    assert_eq!(info.target, desired(1, 1.0).target);
    assert_eq!(completion.metadata.natural_width_dip, 3.0);
    assert_eq!(completion.metadata.natural_height_dip, 4.0);
    // SAFETY: independent initialized PODs and properly admitted matching actual pixels.
    unsafe {
        completion.metadata.flags = 1;
        assert_ne!(
            notepads_markdown_image_policy_complete(
                f.policy,
                request,
                &completion,
                bytes,
                &mut error()
            ),
            0
        );
        completion.metadata.flags = 0;
        assert_eq!(
            notepads_markdown_image_policy_complete(
                f.policy,
                request,
                &completion,
                bytes,
                &mut error()
            ),
            0
        );
    }
    assert_eq!(f.read().1[0].metadata.orientation, 6);
}

#[test]
fn short_snapshot_reports_required_capacity_without_partial_output() {
    let _process = crate::core::images::serialize_process_image_tests();
    let f = Fixture::new();
    f.desire(1, 1, 1.0);
    f.desire(2, 1, 1.0);
    let mut output = ImageSnapshot {
        header: ImageHeader::of::<ImageSnapshot>(),
        state_generation: 0xfeed,
        ..Default::default()
    };
    let mut row = ImageEntry {
        object: 0xbeef,
        ..Default::default()
    };
    let mut err = error();
    // SAFETY: complete independent output PODs; row capacity really is one.
    let status = unsafe {
        notepads_markdown_image_policy_read(f.policy, &mut row, 1, 192, &mut output, &mut err)
    };
    assert_eq!(
        status,
        resource_status(CoreError::ResourceLimit {
            kind: LimitKind::QueryCapacity,
            requested: 2,
            limit: 1
        })
    );
    assert_eq!(err.requested, 2);
    assert_eq!(err.limit, 1);
    assert_eq!(row.object, 0xbeef);
    assert_eq!(output.state_generation, 0xfeed);
    assert_eq!(f.read().1.len(), 2);
}

#[test]
fn auto_uses_one_request_for_real_raster_or_svg_and_explicit_kinds_stay_strict() {
    let _process = crate::core::images::serialize_process_image_tests();
    for svg in [false, true] {
        let mut f = Fixture::new();
        f.desire(1, 1, 1.0);
        let (request, info) = f.start(1, 2);
        let bytes = f.decoded(request, if svg { 8192 } else { 48 });
        let metadata = if svg {
            svg_metadata(1)
        } else {
            wire_metadata(
                ImageMetadata {
                    intrinsic_width: 4,
                    intrinsic_height: 3,
                    decoded_width: 3,
                    decoded_height: 4,
                    orientation: 6,
                    stride: 12,
                }
                .into(),
            )
        };
        let completion = ImageCompletion {
            header: ImageHeader::of::<ImageCompletion>(),
            metadata,
            target: info.target,
            ready: 1,
            ..Default::default()
        };
        // SAFETY: same live request and admitted real payload, independent inputs/outputs.
        unsafe {
            assert_eq!(
                notepads_markdown_image_policy_complete(
                    f.policy,
                    request,
                    &completion,
                    bytes,
                    &mut error()
                ),
                0
            );
        }
        let state = f.read();
        assert_eq!(state.0.active_requests, 1);
        assert_eq!(state.1[0].published_request, info.key.request);
        assert_eq!(state.1[0].metadata.kind, u32::from(svg));
        assert_eq!(state.1[0].desired_target.target_generation, 1);
    }
    for kind in [0, 1] {
        let mut f = Fixture::new();
        f.desire(1, 1, 1.0);
        let (request, info) = f.start(1, kind);
        let bytes = f.decoded(request, 8192);
        let wrong = if kind == 0 {
            svg_metadata(1)
        } else {
            wire_metadata(
                ImageMetadata {
                    intrinsic_width: 4,
                    intrinsic_height: 3,
                    decoded_width: 4,
                    decoded_height: 3,
                    orientation: 1,
                    stride: 16,
                }
                .into(),
            )
        };
        let completion = ImageCompletion {
            header: ImageHeader::of::<ImageCompletion>(),
            metadata: wrong,
            target: info.target,
            ready: 1,
            ..Default::default()
        };
        let generation = f.read().0.state_generation;
        unsafe {
            assert_ne!(
                notepads_markdown_image_policy_complete(
                    f.policy,
                    request,
                    &completion,
                    bytes,
                    &mut error()
                ),
                0
            );
        }
        assert_eq!(f.read().0.state_generation, generation);
        assert_eq!(f.read().1[0].pending_request, info.key.request);
    }
}
