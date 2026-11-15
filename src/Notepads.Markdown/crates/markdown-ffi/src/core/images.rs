// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE file in the project root for license information.
// ---------------------------------------------------------------------------------------------

//! Private image admission ABI. Policy leases account buffer capacities, not Windows objects.

use super::*;
use markdown_core::resources::{
    self, ByteReservation, EntrySnapshot, FailureKind, ImageFailure, ImageMetadata,
    ImageResourcePolicy, ImageVariantMetadata, RequestPermit, ResourceError, ResourceLimit,
    SourceEpoch,
};
use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, Ordering};

mod svg_work;
pub use svg_work::*;
mod transport;
pub use transport::*;

pub const IMAGE_ABI_VERSION: u32 = 2;

#[repr(C, align(8))]
#[derive(Clone, Copy, Default)]
pub struct ImageHeader {
    pub abi_version: u32,
    pub struct_size: u32,
}
impl ImageHeader {
    fn of<T>() -> Self {
        Self {
            abi_version: IMAGE_ABI_VERSION,
            struct_size: size_of::<T>() as u32,
        }
    }
}
#[repr(C, align(8))]
#[derive(Clone, Copy, Default)]
pub struct ImageEpoch {
    pub source: WireSource,
    pub source_generation: u64,
    pub provider_generation: u64,
}
#[repr(C, align(8))]
#[derive(Clone, Copy, Default)]
pub struct ImageEpochDescriptor {
    pub header: ImageHeader,
    pub epoch: ImageEpoch,
}
#[repr(C, align(8))]
#[derive(Clone, Copy, Default)]
pub struct ImageKey {
    pub epoch: ImageEpoch,
    pub object: u32,
    pub reserved: u32,
    pub request: u64,
}
#[repr(C, align(8))]
#[derive(Clone, Copy, Default)]
pub struct ImageSnapshot {
    pub header: ImageHeader,
    pub epoch: ImageEpoch,
    pub state_generation: u64,
    pub count: u32,
    pub active_requests: u32,
    pub reservations: u32,
    pub has_epoch: u32,
    pub encoded_bytes: u64,
    pub decoded_bytes: u64,
}
const _: () = {
    assert!(size_of::<ImageEpoch>() == 40 && size_of::<ImageEpochDescriptor>() == 48);
    assert!(size_of::<ImageKey>() == 56);
    assert!(size_of::<ImageSnapshot>() == 88);
    assert!(std::mem::offset_of!(ImageSnapshot, decoded_bytes) == 80);
    assert!(align_of::<ImageSnapshot>() == 8);
};

struct ImageOwner {
    // The policy/table and every permit/byte wrapper retire before the table's funding.
    policy: ImageResourcePolicy,
    _table: BoundaryReservation,
    budget: SourceBudget,
    poisoned: AtomicBool,
    // Includes panic containment and poisoning, not just each policy decision.
    // No user callback, IO or decoder runs in this bounded ABI critical section.
    call_gate: Mutex<()>,
}
pub struct ImagePolicy {
    owner: Arc<ImageOwner>,
}
pub struct ImageRequest {
    permit: RequestPermit,
    owner: Arc<ImageOwner>,
}
pub struct ImageBytes {
    bytes: ByteReservation,
    owner: Arc<ImageOwner>,
}

fn resource_error(error: ResourceError) -> CoreError {
    match error {
        ResourceError::OutOfMemory => CoreError::OutOfMemory,
        ResourceError::StaleResult => CoreError::StaleResult,
        ResourceError::UnknownObject => CoreError::UnknownId,
        ResourceError::ArithmeticOverflow => CoreError::ArithmeticOverflow,
        ResourceError::SnapshotTooSmall { .. }
        | ResourceError::InvalidSize
        | ResourceError::InvalidMetadata => CoreError::InvalidRange,
        ResourceError::WrongReservation => CoreError::WrongBudget,
        ResourceError::InvalidEpoch | ResourceError::InvalidState | ResourceError::NotAdmitted => {
            CoreError::InvalidState
        }
        ResourceError::Limit {
            kind,
            requested,
            limit,
        } => CoreError::ResourceLimit {
            kind: match kind {
                ResourceLimit::Entries => LimitKind::ModelRecords,
                ResourceLimit::ActiveRequests => LimitKind::QueryRequests,
                ResourceLimit::Reservations => LimitKind::IndexRecords,
                ResourceLimit::DecodedBytes => LimitKind::DecodedBytes,
                ResourceLimit::EncodedBytes | ResourceLimit::EncodedImageBytes => {
                    LimitKind::AuxiliaryCapacity
                }
            },
            requested,
            limit,
        },
    }
}
fn from_epoch(epoch: ImageEpoch) -> Result<SourceEpoch, CoreError> {
    Ok(SourceEpoch {
        source: key(epoch.source)?,
        source_generation: epoch.source_generation,
        provider_generation: epoch.provider_generation,
    })
}
fn wire_epoch(epoch: SourceEpoch) -> ImageEpoch {
    ImageEpoch {
        source: WireSource {
            document: epoch.source.document(),
            revision: epoch.source.revision(),
        },
        source_generation: epoch.source_generation,
        provider_generation: epoch.provider_generation,
    }
}
fn wire_key(key: resources::RequestKey) -> ImageKey {
    ImageKey {
        epoch: wire_epoch(key.epoch),
        object: key.object,
        reserved: 0,
        request: key.request,
    }
}
fn failure(value: u32) -> Result<FailureKind, CoreError> {
    Ok(match value {
        1 => FailureKind::Unavailable,
        2 => FailureKind::Cancelled,
        3 => FailureKind::Provider,
        4 => FailureKind::Decode,
        5 => FailureKind::Unsupported,
        6 => FailureKind::Limited,
        7 => FailureKind::Released,
        _ => return Err(CoreError::InvalidRange),
    })
}
fn wire_failure(value: FailureKind) -> u32 {
    match value {
        FailureKind::Unavailable => 1,
        FailureKind::Cancelled => 2,
        FailureKind::Provider => 3,
        FailureKind::Decode => 4,
        FailureKind::Unsupported => 5,
        FailureKind::Limited => 6,
        FailureKind::Released => 7,
    }
}
unsafe fn image_header<T>(pointer: *const T) -> Result<(), i32> {
    if pointer.is_null() || !(pointer as usize).is_multiple_of(align_of::<T>()) {
        return Err(STATUS_INVALID_ARGUMENT);
    }
    // SAFETY: caller provides the readable aligned 8-byte image prefix.
    let prefix = unsafe { pointer.cast::<ImageHeader>().read() };
    if prefix.abi_version != IMAGE_ABI_VERSION || prefix.struct_size as usize != size_of::<T>() {
        return Err(STATUS_ABI_MISMATCH);
    }
    Ok(())
}
unsafe fn image_action(
    owner: &ImageOwner,
    error: *mut WireError,
    work: impl FnOnce(&ImageOwner, &mut Operation) -> Result<(), CoreError>,
) -> i32 {
    // Serialize the check, bounded work, panic containment AND poison store.
    // A concurrent call cannot pass the gate before a panic and publish after it.
    let _gate = owner.call_gate.lock().unwrap_or_else(|poison| {
        owner.poisoned.store(true, Ordering::Release);
        poison.into_inner()
    });
    // SAFETY: caller supplies independent writable ABI3 error storage.
    let status = unsafe {
        action(error, || {
            if owner.poisoned.load(Ordering::Acquire) {
                return Err(CoreError::InvalidState);
            }
            let mut op = owner.budget.begin_cancellable(false)?;
            op.boundary_work().checkpoint_now()?;
            work(owner, &mut op)
        })
    };
    if status == STATUS_PANIC {
        owner.poisoned.store(true, Ordering::Release);
    }
    status
}

#[unsafe(no_mangle)]
pub extern "C" fn notepads_markdown_image_abi_version() -> u32 {
    IMAGE_ABI_VERSION
}

/// # Safety
/// Independent writable handle output and Core ABI3 error storage.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn notepads_markdown_image_policy_create(
    output: *mut *mut Owned<ImagePolicy>,
    error: *mut WireError,
) -> i32 {
    preflight!(transport::distinct(&[transport::region(output), transport::region(error)]));
    handle_output!(output);
    // SAFETY: preflighted handle storage; action validates error before any work.
    unsafe {
        action(error, || {
            let budget = SourceBudget::preview();
            let mut op = budget.begin_cancellable(false)?;
            let slot = Slot::new(op.boundary_work())?;
            let mut table =
                op.reserve_boundary(ImageResourcePolicy::table_request_bytes() as u64)?;
            let policy = ImageResourcePolicy::try_new().map_err(resource_error)?;
            table.admit_actual(policy.table_capacity_bytes() as u64, &mut op)?;
            op.boundary_work().checkpoint_now()?;
            output.write(slot.commit(ImagePolicy {
                owner: Arc::new(ImageOwner {
                    policy,
                    _table: table,
                    budget,
                    poisoned: AtomicBool::new(false),
                    call_gate: Mutex::new(()),
                }),
            }));
            Ok(())
        })
    }
}

/// # Safety
/// Shared live request, independent writable bytes handle and Core error storage.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn notepads_markdown_image_request_reserve(
    request: *const Owned<ImageRequest>,
    kind: u32,
    bytes: u64,
    output: *mut *mut Owned<ImageBytes>,
    error: *mut WireError,
) -> i32 {
    handle_output!(output);
    preflight!(valid_handle(request) && kind <= 1);
    // SAFETY: request remains live; output is separate from request/error storage.
    unsafe {
        image_action(&(*request).value.owner, error, |_, op| {
            let slot = Slot::new(op.boundary_work())?;
            let request = &(*request).value;
            let charge = if kind == 0 {
                request.permit.reserve_encoded(bytes)
            } else {
                request.permit.reserve_decoded(bytes)
            }
            .map_err(resource_error)?;
            output.write(slot.commit(ImageBytes {
                bytes: charge,
                owner: request.owner.clone(),
            }));
            Ok(())
        })
    }
}

/// # Safety
/// Live shared bytes lease, actual already allocated capacity, independent error storage.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn notepads_markdown_image_bytes_admit(
    bytes: *const Owned<ImageBytes>,
    actual: u64,
    error: *mut WireError,
) -> i32 {
    preflight!(valid_handle(bytes));
    // SAFETY: independently owned lease remains live throughout the atomic admission.
    unsafe {
        image_action(&(*bytes).value.owner, error, |_, _| {
            (*bytes)
                .value
                .bytes
                .admit_actual(actual)
                .map_err(resource_error)
        })
    }
}

destroy!(notepads_markdown_image_policy_destroy, ImagePolicy);
destroy!(notepads_markdown_image_request_destroy, ImageRequest);
destroy!(notepads_markdown_image_bytes_destroy, ImageBytes);

#[cfg(test)]
mod tests;

#[cfg(test)]
fn serialize_process_image_tests() -> std::sync::MutexGuard<'static, ()> {
    static SERIAL: Mutex<()> = Mutex::new(());
    SERIAL.lock().unwrap_or_else(|poisoned| poisoned.into_inner())
}

#[cfg(test)]
fn desire_image_for_test(policy: *const Owned<ImagePolicy>, object: u32, error: &mut WireError) {
    let desired = ImageDesired {
        header: ImageHeader::of::<ImageDesired>(),
        target: image_test_target(),
    };
    // SAFETY: fixtures own the live policy and independent writable error.
    assert_eq!(unsafe { notepads_markdown_image_policy_desire(policy, object, &desired, error) }, 0);
}

#[cfg(test)]
fn image_test_target() -> ImageTargetWire {
    ImageTargetWire {
        font_generation: 1,
        target_generation: 1,
        available_width_dip: 256.0,
        effective_physical_scale: 1.0,
    }
}
