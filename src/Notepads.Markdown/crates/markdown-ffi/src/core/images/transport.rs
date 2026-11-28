// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE file in the project root for license information.
// ---------------------------------------------------------------------------------------------

//! Image transport for target-bound requests and windowed process capacity.

use super::*;
use resources::{ImageTarget, SvgImageMetadata};

#[repr(C, align(8))]
#[derive(Clone, Copy, Default, Debug, PartialEq)]
pub struct ImageTargetWire {
    pub font_generation: u64,
    pub target_generation: u64,
    pub available_width_dip: f64,
    pub effective_physical_scale: f64,
}
#[repr(C, align(8))]
#[derive(Clone, Copy, Default)]
pub struct ImageDesired {
    pub header: ImageHeader,
    pub target: ImageTargetWire,
}
#[repr(C, align(8))]
#[derive(Clone, Copy, Default)]
pub struct ImageRequestInfo {
    pub header: ImageHeader,
    pub key: ImageKey,
    pub target: ImageTargetWire,
}
#[repr(C, align(8))]
#[derive(Clone, Copy, Default, Debug, PartialEq)]
pub struct ImageMetadataWire {
    pub kind: u32,
    pub flags: u32,
    pub intrinsic_width: u32,
    pub intrinsic_height: u32,
    pub decoded_width: u32,
    pub decoded_height: u32,
    pub orientation: u32,
    pub stride: u32,
    pub natural_width_dip: f64,
    pub natural_height_dip: f64,
    pub source_width: f64,
    pub source_height: f64,
    pub viewport_width_bits: u32,
    pub viewport_height_bits: u32,
    pub scale_bits: u32,
    pub reserved: u32,
}
#[repr(C, align(8))]
#[derive(Clone, Copy, Default)]
pub struct ImageEntry {
    pub object: u32,
    pub state: u32,
    pub published_request: u64,
    pub metadata: ImageMetadataWire,
    pub desired_target: ImageTargetWire,
    pub published_target: ImageTargetWire,
    pub pending_request: u64,
    pub failure: u32,
    pub error: i32,
    pub status_flags: u32,
    pub reserved: u32,
    pub detail: u32,
    pub padding: u32,
}
#[repr(C, align(8))]
#[derive(Clone, Copy, Default)]
pub struct ImageCompletion {
    pub header: ImageHeader,
    pub metadata: ImageMetadataWire,
    pub target: ImageTargetWire,
    pub ready: u32,
    pub failure: u32,
    pub error: i32,
    pub detail: u32,
}

#[repr(C, align(8))]
#[derive(Clone, Copy, Default)]
pub struct ImageProcessCapacity {
    pub header: ImageHeader,
    pub generation: u64,
    pub encoded_bytes: u64,
    pub decoded_bytes: u64,
}

#[repr(C, align(8))]
#[derive(Clone, Copy, Default)]
pub struct ImageRasterPlan {
    pub header: ImageHeader,
    pub width: u32,
    pub height: u32,
    pub stride: u32,
    pub maximum_side: u32,
    pub bytes: u64,
}

const _: () = {
    assert!(size_of::<ImageTargetWire>() == 32 && size_of::<ImageDesired>() == 40);
    assert!(size_of::<ImageRequestInfo>() == 96 && size_of::<ImageMetadataWire>() == 80);
    assert!(size_of::<ImageEntry>() == 192 && size_of::<ImageCompletion>() == 136);
    assert!(std::mem::offset_of!(ImageEntry, detail) == 184);
    assert!(align_of::<ImageEntry>() == 8);
    assert!(size_of::<ImageProcessCapacity>() == 32 && align_of::<ImageProcessCapacity>() == 8);
    assert!(
        size_of::<ImageRasterPlan>() == 32 && std::mem::offset_of!(ImageRasterPlan, bytes) == 24
    );
};

pub(super) fn region<T>(pointer: *const T) -> (usize, usize) {
    (pointer as usize, size_of::<T>())
}
/// Numerical preflight only: actual storage validity and lifetime remain the caller's obligation.
pub(super) fn distinct(regions: &[(usize, usize)]) -> bool {
    for (index, &(start, size)) in regions.iter().enumerate() {
        if size == 0 {
            continue;
        }
        let Some(end) = start.checked_add(size) else {
            return false;
        };
        if start == 0 || size > isize::MAX as usize {
            return false;
        }
        for &(other, length) in &regions[..index] {
            if length != 0 && start < other.saturating_add(length) && other < end {
                return false;
            }
        }
    }
    true
}
fn wire_target(value: Option<ImageTarget>) -> ImageTargetWire {
    value.map_or_else(ImageTargetWire::default, |value| ImageTargetWire {
        font_generation: value.font_generation,
        target_generation: value.generation,
        available_width_dip: value.available_width_dip,
        effective_physical_scale: value.effective_physical_scale,
    })
}
fn target(value: ImageTargetWire) -> Result<ImageTarget, CoreError> {
    let result = ImageTarget {
        font_generation: value.font_generation,
        generation: value.target_generation,
        available_width_dip: value.available_width_dip,
        effective_physical_scale: value.effective_physical_scale,
    };
    result.validate().map_err(resource_error)?;
    Ok(result)
}
fn wire_metadata(value: ImageVariantMetadata) -> ImageMetadataWire {
    match value {
        ImageVariantMetadata::Raster(value) => {
            let (width, height) = if value.orientation >= 5 {
                (value.intrinsic_height, value.intrinsic_width)
            } else {
                (value.intrinsic_width, value.intrinsic_height)
            };
            ImageMetadataWire {
                intrinsic_width: value.intrinsic_width,
                intrinsic_height: value.intrinsic_height,
                decoded_width: value.decoded_width,
                decoded_height: value.decoded_height,
                orientation: value.orientation,
                stride: value.stride,
                natural_width_dip: f64::from(width),
                natural_height_dip: f64::from(height),
                source_width: f64::from(value.decoded_width),
                source_height: f64::from(value.decoded_height),
                ..ImageMetadataWire::default()
            }
        }
        ImageVariantMetadata::Svg(value) => ImageMetadataWire {
            kind: 1,
            flags: u32::from(value.resolution_limited),
            decoded_width: value.decoded_width,
            decoded_height: value.decoded_height,
            orientation: 1,
            stride: value.stride,
            natural_width_dip: value.natural_width_dip,
            natural_height_dip: value.natural_height_dip,
            source_width: value.source_width,
            source_height: value.source_height,
            viewport_width_bits: value.viewport_width_bits,
            viewport_height_bits: value.viewport_height_bits,
            scale_bits: value.scale_bits,
            ..ImageMetadataWire::default()
        },
    }
}
fn read_metadata(value: ImageMetadataWire) -> Result<ImageVariantMetadata, CoreError> {
    if value.reserved != 0 {
        return Err(CoreError::InvalidRange);
    }
    match value.kind {
        0 => {
            let raster = ImageMetadata {
                intrinsic_width: value.intrinsic_width,
                intrinsic_height: value.intrinsic_height,
                decoded_width: value.decoded_width,
                decoded_height: value.decoded_height,
                orientation: value.orientation,
                stride: value.stride,
            };
            raster.required_capacity().map_err(resource_error)?;
            if value != wire_metadata(raster.into()) {
                return Err(CoreError::InvalidRange);
            }
            Ok(raster.into())
        }
        1 if value.flags <= 1
            && value.intrinsic_width == 0
            && value.intrinsic_height == 0
            && value.orientation == 1 =>
        {
            Ok(SvgImageMetadata {
                natural_width_dip: value.natural_width_dip,
                natural_height_dip: value.natural_height_dip,
                decoded_width: value.decoded_width,
                decoded_height: value.decoded_height,
                stride: value.stride,
                source_width: value.source_width,
                source_height: value.source_height,
                viewport_width_bits: value.viewport_width_bits,
                viewport_height_bits: value.viewport_height_bits,
                scale_bits: value.scale_bits,
                resolution_limited: value.flags != 0,
            }
            .into())
        }
        _ => Err(CoreError::InvalidRange),
    }
}
fn wire_state(state: resources::ImageState) -> u32 {
    match state {
        resources::ImageState::Unrequested => 0,
        resources::ImageState::Pending => 1,
        resources::ImageState::Ready => 2,
        resources::ImageState::Unavailable => 3,
        resources::ImageState::Failed => 4,
        resources::ImageState::Limited => 5,
    }
}
fn wire_entry(value: EntrySnapshot) -> ImageEntry {
    let satisfied = value.target_satisfied();
    let needs_render = value.desired_target.is_some_and(|target| {
        value.pending_request.is_none()
            && !satisfied
            && (value.last_attempt_generation < target.generation || value.retry_eligible)
    });
    let limited = match (value.metadata, value.desired_target) {
        (Some(ImageVariantMetadata::Svg(metadata)), Some(target)) => {
            resources::svg_size::SvgNaturalSize::new(
                metadata.natural_width_dip,
                metadata.natural_height_dip,
            )
            .and_then(|natural| {
                resources::svg_size::SvgRasterPlan::for_display(
                    natural,
                    metadata.natural_width_dip.min(target.available_width_dip),
                    target.effective_physical_scale,
                )
            })
            .is_ok_and(|plan| plan.resolution_limited)
        }
        _ => false,
    };
    ImageEntry {
        object: value.object,
        state: wire_state(value.state),
        published_request: if value.state == resources::ImageState::Ready {
            value.request.map_or(0, |key| key.request)
        } else {
            0
        },
        metadata: value
            .metadata
            .map_or_else(ImageMetadataWire::default, wire_metadata),
        desired_target: wire_target(value.desired_target),
        published_target: wire_target(value.published_target),
        pending_request: value.pending_request.map_or(0, |key| key.request),
        failure: value.failure.map_or(0, |value| wire_failure(value.kind)),
        error: value.failure.map_or(0, |value| value.code),
        status_flags: u32::from(needs_render)
            | (u32::from(satisfied) << 1)
            | (u32::from(limited) << 2)
            | (u32::from(value.metadata.is_some()) << 3)
            | (u32::from(value.capacity_wait) << 4),
        detail: value.failure_detail,
        ..ImageEntry::default()
    }
}

/// # Safety
/// Live policy and independent valid error output. No bytes are released here.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn notepads_markdown_image_policy_evict(
    policy: *const Owned<ImagePolicy>,
    object: u32,
    error: *mut WireError,
) -> i32 {
    preflight!(valid_handle(policy) && distinct(&[region(policy), region(error)]));
    // SAFETY: the caller retains policy through the synchronous call.
    unsafe {
        image_action(&(*policy).value.owner, error, |owner, _| {
            owner.policy.evict_variant(object).map_err(resource_error)
        })
    }
}

/// # Safety
/// Live policy and independent valid error output; physical owners remain charged.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn notepads_markdown_image_policy_replace_source(
    policy: *const Owned<ImagePolicy>,
    object: u32,
    error: *mut WireError,
) -> i32 {
    preflight!(valid_handle(policy) && distinct(&[region(policy), region(error)]));
    // SAFETY: caller provides a live independent owner and error POD.
    unsafe {
        image_action(&(*policy).value.owner, error, |owner, _| {
            owner
                .policy
                .replace_variant_source(object)
                .map_err(resource_error)
        })
    }
}

/// # Safety
/// Live policy and independent valid error output; physical owners remain charged.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn notepads_markdown_image_policy_forget(
    policy: *const Owned<ImagePolicy>,
    object: u32,
    error: *mut WireError,
) -> i32 {
    preflight!(valid_handle(policy) && distinct(&[region(policy), region(error)]));
    // SAFETY: caller provides a live independent owner and error POD.
    unsafe {
        image_action(&(*policy).value.owner, error, |owner, _| {
            owner.policy.forget_variant(object).map_err(resource_error)
        })
    }
}

/// # Safety
/// Live policy, independent aligned writable u32 and Core error output.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn notepads_markdown_image_policy_retry(
    policy: *const Owned<ImagePolicy>,
    object: u32,
    eligible: *mut u32,
    error: *mut WireError,
) -> i32 {
    preflight!(
        valid_handle(policy)
            && !eligible.is_null()
            && (eligible as usize).is_multiple_of(align_of::<u32>())
            && distinct(&[region(policy), region(eligible), region(error)])
    );
    // SAFETY: all fallible validation precedes the independent output write.
    unsafe {
        image_action(&(*policy).value.owner, error, |owner, _| {
            let result = owner
                .policy
                .retry_eligible(object)
                .map_err(resource_error)?;
            eligible.write(u32::from(result));
            Ok(())
        })
    }
}

/// # Safety
/// Live request and complete aligned readable metadata, independent error POD.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn notepads_markdown_image_request_note_metadata(
    request: *const Owned<ImageRequest>,
    metadata: *const ImageMetadataWire,
    error: *mut WireError,
) -> i32 {
    preflight!(
        valid_handle(request)
            && !metadata.is_null()
            && (metadata as usize).is_multiple_of(align_of::<ImageMetadataWire>())
            && distinct(&[region(request), region(metadata), region(error)])
    );
    // SAFETY: caller supplies full fixed-layout metadata and retains the request.
    unsafe {
        image_action(&(*request).value.owner, error, |owner, _| {
            owner
                .policy
                .note_variant_metadata((*request).value.permit.key(), read_metadata(*metadata)?)
                .map_err(resource_error)
        })
    }
}

/// # Safety
/// Initialized image-v2 output prefix and independent valid Core error POD.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn notepads_markdown_image_get_process_capacity(
    output: *mut ImageProcessCapacity,
    error: *mut WireError,
) -> i32 {
    preflight!(distinct(&[region(output), region(error)]));
    // SAFETY: caller guarantees a readable initialized prefix.
    if let Err(status) = unsafe { image_header(output) } {
        return status;
    }
    // SAFETY: action validates the independent error before the output write.
    unsafe {
        action(error, || {
            let usage = resources::process_image_usage();
            output.write(ImageProcessCapacity {
                header: ImageHeader::of::<ImageProcessCapacity>(),
                generation: usage.generation,
                encoded_bytes: usage.encoded_bytes,
                decoded_bytes: usage.decoded_bytes,
            });
            Ok(())
        })
    }
}

/// # Safety
/// Complete aligned metadata/target, initialized output prefix; independent error.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn notepads_markdown_image_plan_raster(
    metadata: *const ImageMetadataWire,
    desired: *const ImageTargetWire,
    output: *mut ImageRasterPlan,
    error: *mut WireError,
) -> i32 {
    preflight!(
        !metadata.is_null()
            && !desired.is_null()
            && (metadata as usize).is_multiple_of(align_of::<ImageMetadataWire>())
            && (desired as usize).is_multiple_of(align_of::<ImageTargetWire>())
            && distinct(&[
                region(metadata),
                region(desired),
                region(output),
                region(error)
            ])
    );
    // SAFETY: caller guarantees an initialized output prefix.
    if let Err(status) = unsafe { image_header(output) } {
        return status;
    }
    // SAFETY: readable fixed inputs and independent output/error storage.
    unsafe {
        action(error, || {
            let ImageVariantMetadata::Raster(metadata) = read_metadata(*metadata)? else {
                return Err(CoreError::InvalidRange);
            };
            let plan = metadata
                .raster_plan(target(*desired)?)
                .map_err(resource_error)?;
            output.write(ImageRasterPlan {
                header: ImageHeader::of::<ImageRasterPlan>(),
                width: plan.width,
                height: plan.height,
                stride: plan.stride,
                maximum_side: plan.maximum_side,
                bytes: plan.bytes,
            });
            Ok(())
        })
    }
}

/// Register one static module callback, called outside ledger locks. The caller
/// serializes registration and pins callback code until synchronous unregister
/// returns. Callback must not throw or register/unregister itself.
#[unsafe(no_mangle)]
pub extern "C" fn notepads_markdown_image_set_process_capacity_notify(
    notify: Option<resources::CapacityNotify>,
) {
    let _ = crate::contain_panic(|| {
        resources::set_process_image_capacity_notify(notify);
        crate::STATUS_OK
    });
}

/// # Safety
/// Live policy and initialized readable prefix/full epoch, separate error storage.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn notepads_markdown_image_policy_begin(
    policy: *const Owned<ImagePolicy>,
    epoch: *const ImageEpochDescriptor,
    error: *mut WireError,
) -> i32 {
    preflight!(valid_handle(policy) && distinct(&[region(policy), region(epoch), region(error)]));
    if let Err(status) = unsafe { image_header(epoch) } {
        return status;
    }
    // SAFETY: live owner and fully authorized, independent epoch storage.
    unsafe {
        image_action(&(*policy).value.owner, error, |owner, _| {
            owner
                .policy
                .begin_source(from_epoch((*epoch).epoch)?)
                .map_err(resource_error)
        })
    }
}

/// # Safety
/// Shared live policy, readable versioned desired target, independent error.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn notepads_markdown_image_policy_desire(
    policy: *const Owned<ImagePolicy>,
    object: u32,
    desired: *const ImageDesired,
    error: *mut WireError,
) -> i32 {
    preflight!(valid_handle(policy) && distinct(&[region(policy), region(desired), region(error)]));
    if let Err(status) = unsafe { image_header(desired) } {
        return status;
    }
    // SAFETY: complete descriptor authorized by prefix; no borrowed data escapes.
    unsafe {
        image_action(&(*policy).value.owner, error, |owner, _| {
            owner
                .policy
                .desire_variant(object, target((*desired).target)?)
                .map_err(resource_error)
        })
    }
}

/// # Safety
/// Live policy and complete independent initialized info, handle/error outputs.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn notepads_markdown_image_policy_start(
    policy: *const Owned<ImagePolicy>,
    object: u32,
    kind: u32,
    output: *mut *mut Owned<ImageRequest>,
    info: *mut ImageRequestInfo,
    error: *mut WireError,
) -> i32 {
    preflight!(
        valid_handle(policy)
            && kind <= 2
            && distinct(&[region(policy), region(output), region(info), region(error)])
    );
    if let Err(status) = unsafe { image_header(info) } {
        return status;
    }
    handle_output!(output);
    // SAFETY: all outputs are independent. All fallible work precedes start commit.
    unsafe {
        image_action(&(*policy).value.owner, error, |owner, op| {
            let slot = Slot::new(op.boundary_work())?;
            op.boundary_work().checkpoint_now()?;
            let permit = match kind {
                0 => owner.policy.start(object),
                1 => owner.policy.start_variant(object),
                _ => owner.policy.start_unknown(object),
            }
            .map_err(resource_error)?;
            let key = wire_key(permit.key());
            let target = wire_target(Some(permit.target()));
            output.write(slot.commit(ImageRequest {
                permit,
                owner: (*policy).value.owner.clone(),
            }));
            info.write(ImageRequestInfo {
                header: ImageHeader::of::<ImageRequestInfo>(),
                key,
                target,
            });
            Ok(())
        })
    }
}

/// # Safety
/// Live shared policy and disjoint writable generation/error values.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn notepads_markdown_image_policy_generation(
    policy: *const Owned<ImagePolicy>,
    generation: *mut u64,
    error: *mut WireError,
) -> i32 {
    preflight!(
        valid_handle(policy)
            && (generation as usize).is_multiple_of(align_of::<u64>())
            && distinct(&[region(policy), region(generation), region(error)])
    );
    // SAFETY: the policy and both outputs meet the ABI requirements above.
    unsafe {
        image_action(&(*policy).value.owner, error, |owner, op| {
            let value = owner.policy.state_generation().map_err(resource_error)?;
            op.boundary_work().checkpoint_now()?;
            generation.write(value);
            Ok(())
        })
    }
}

/// # Safety
/// Shared live policy, valid aligned full output regions and initialized snapshot prefix.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn notepads_markdown_image_policy_read(
    policy: *const Owned<ImagePolicy>,
    rows: *mut ImageEntry,
    capacity: u32,
    row_size: u32,
    output: *mut ImageSnapshot,
    error: *mut WireError,
) -> i32 {
    let bytes = (capacity as usize).checked_mul(size_of::<ImageEntry>());
    preflight!(
        valid_handle(policy)
            && capacity as usize <= resources::MAX_ENTRIES
            && row_size as usize == size_of::<ImageEntry>()
            && bytes.is_some()
            && (capacity == 0
                || (!rows.is_null() && (rows as usize).is_multiple_of(align_of::<ImageEntry>())))
            && distinct(&[
                region(policy),
                (rows as usize, bytes.unwrap_or(0)),
                region(output),
                region(error)
            ])
    );
    if let Err(status) = unsafe { image_header(output) } {
        return status;
    }
    // SAFETY: complete independent output buffers; all fallible work precedes writes.
    unsafe {
        image_action(&(*policy).value.owner, error, |owner, op| {
            let mut native = [EntrySnapshot::default(); resources::MAX_ENTRIES];
            let snapshot = owner
                .policy
                .snapshot(&mut native[..capacity as usize])
                .map_err(|error| match error {
                    ResourceError::SnapshotTooSmall { required } => CoreError::ResourceLimit {
                        kind: LimitKind::QueryCapacity,
                        requested: required as u64,
                        limit: u64::from(capacity),
                    },
                    error => resource_error(error),
                })?;
            op.boundary_work().checkpoint_now()?;
            for (index, entry) in native[..snapshot.written].iter().enumerate() {
                rows.add(index).write(wire_entry(*entry));
            }
            output.write(ImageSnapshot {
                header: ImageHeader::of::<ImageSnapshot>(),
                epoch: snapshot.epoch.map_or_else(ImageEpoch::default, wire_epoch),
                state_generation: snapshot.state_generation,
                count: snapshot.written as u32,
                active_requests: snapshot.usage.active_requests,
                reservations: snapshot.usage.reservations,
                has_epoch: u32::from(snapshot.epoch.is_some()),
                encoded_bytes: snapshot.usage.encoded_bytes,
                decoded_bytes: snapshot.usage.decoded_bytes,
            });
            Ok(())
        })
    }
}

/// # Safety
/// Live policy/request/optional bytes, initialized readable completion prefix/full POD,
/// with all pointers and error storage valid, aligned and mutually independent.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn notepads_markdown_image_policy_complete(
    policy: *const Owned<ImagePolicy>,
    request: *const Owned<ImageRequest>,
    completion: *const ImageCompletion,
    decoded: *const Owned<ImageBytes>,
    error: *mut WireError,
) -> i32 {
    // SAFETY: forwards this function's complete independent input contract.
    unsafe { complete(policy, request, completion, decoded, error, false) }
}

/// Bind immutable decoded bytes to a new current request without another charge.
/// # Safety
/// As complete; native additionally verifies exact freshly authorized content
/// and render identity. Only the same policy may reuse its own admitted bytes.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn notepads_markdown_image_policy_complete_reused(
    policy: *const Owned<ImagePolicy>,
    request: *const Owned<ImageRequest>,
    completion: *const ImageCompletion,
    decoded: *const Owned<ImageBytes>,
    error: *mut WireError,
) -> i32 {
    // SAFETY: forwards this function's complete independent input contract.
    unsafe { complete(policy, request, completion, decoded, error, true) }
}

unsafe fn complete(
    policy: *const Owned<ImagePolicy>,
    request: *const Owned<ImageRequest>,
    completion: *const ImageCompletion,
    decoded: *const Owned<ImageBytes>,
    error: *mut WireError,
    reused: bool,
) -> i32 {
    preflight!(
        valid_handle(policy)
            && valid_handle(request)
            && (decoded.is_null() || valid_handle(decoded))
            && distinct(&[
                region(policy),
                region(request),
                region(completion),
                (
                    decoded as usize,
                    if decoded.is_null() {
                        0
                    } else {
                        size_of::<Owned<ImageBytes>>()
                    }
                ),
                region(error)
            ])
    );
    if let Err(status) = unsafe { image_header(completion) } {
        return status;
    }
    // SAFETY: complete inputs preflighted, independently pinned handles, no native callbacks.
    unsafe {
        image_action(&(*policy).value.owner, error, |owner, _| {
            if !Arc::ptr_eq(&(*policy).value.owner, &(*request).value.owner) {
                return Err(CoreError::WrongBudget);
            }
            let input = completion.read();
            if input.ready > 1
                || (reused && input.ready != 1)
                || input.target != wire_target(Some((*request).value.permit.target()))
            {
                return Err(CoreError::InvalidRange);
            }
            if input.ready == 1 {
                if decoded.is_null() || input.failure != 0 || input.error != 0 || input.detail != 0
                {
                    return Err(CoreError::InvalidRange);
                }
                if !Arc::ptr_eq(&(*policy).value.owner, &(*decoded).value.owner) {
                    return Err(CoreError::WrongBudget);
                }
                let key = (*request).value.permit.key();
                let metadata = read_metadata(input.metadata)?;
                let bytes = &(*decoded).value.bytes;
                if reused {
                    owner.policy.complete_reused_variant(key, metadata, bytes)
                } else {
                    owner.policy.complete_variant_ready(key, metadata, bytes)
                }
                .map_err(resource_error)
            } else {
                let kind = input.detail >> 16;
                let limit = input.detail & 0xffff;
                if !decoded.is_null()
                    || input.metadata != ImageMetadataWire::default()
                    || (input.detail != 0
                        && (!(matches!(kind, 1..=7 | 16..=24))
                            || limit > markdown_svg_budget::LimitKind::ConvertedSegments as u32))
                {
                    return Err(CoreError::InvalidRange);
                }
                owner
                    .policy
                    .complete_variant_failure(
                        (*request).value.permit.key(),
                        ImageFailure {
                            kind: failure(input.failure)?,
                            code: input.error,
                        },
                        input.detail,
                    )
                    .map_err(resource_error)
            }
        })
    }
}

#[cfg(test)]
mod tests;
