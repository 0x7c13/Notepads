// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE file in the project root for license information.
// ---------------------------------------------------------------------------------------------

//! Stable, owned account capability for Native font allocations and the engine.
//! Destruction publishes its consumed receipt before any irreversible free.
use super::super::transport::{distinct, region};
use super::access::{AccountAccess, AccountOwner};
use super::*;
use markdown_svg_budget::{
    Error, FailureKind, FirstFailureRef, FirstFailureSlot, LimitKind as SvgLimit,
};
use std::mem::ManuallyDrop;
use std::sync::Arc;

pub const SVG_RESOURCES_ABI_VERSION: u32 = 1;
#[repr(C, align(8))]
#[derive(Clone, Copy, Default, Debug, Eq, PartialEq)]
pub struct SvgFault {
    pub kind: u32,
    pub limit: u32,
    pub native_hr: i32,
    pub native_hr_present: u32,
}
#[repr(C, align(8))]
#[derive(Clone, Copy, Default)]
pub struct SvgResourcesRetirement {
    pub header: SvgWorkHeader,
    pub consumed: u32,
    pub cleanup_succeeded: u32,
    pub account: *mut Owned<SvgWork>,
    #[cfg(target_pointer_width = "32")]
    pub pointer_padding: u32,
    pub first_fault: SvgFault,
    pub unreleased_header: SvgWorkToken,
    pub unreleased_account: SvgWorkToken,
}
const _: () = {
    assert!(size_of::<SvgFault>() == 16);
    assert!(size_of::<SvgResourcesRetirement>() == 72);
    assert!(std::mem::offset_of!(SvgResourcesRetirement, first_fault) == 24);
};

pub struct SvgResources {
    owner: ManuallyDrop<AccountOwner>,
    host: ManuallyDrop<Arc<AccountAccess>>,
    fault: ManuallyDrop<FirstFailureRef>,
    payment: SvgWorkToken,
}
fn resource_layout() -> Layout {
    Layout::new::<SvgResources>()
}
fn fault_layout() -> Layout {
    Layout::new::<(usize, usize)>()
        .extend(Layout::new::<FirstFailureSlot>())
        .unwrap()
        .0
        .pad_to_align()
}
pub(super) fn valid_resource(p: *const SvgResources) -> bool {
    !p.is_null() && (p as usize).is_multiple_of(align_of::<SvgResources>())
}
pub(super) fn aligned<T>(p: *const T) -> bool {
    !p.is_null() && (p as usize).is_multiple_of(align_of::<T>())
}

pub(super) fn fault_wire(error: Option<Error>) -> SvgFault {
    error.map_or(SvgFault::default(), |e| SvgFault {
        kind: e.kind as u32,
        limit: e.limit as u32,
        native_hr: e.native_hr.unwrap_or(0),
        native_hr_present: u32::from(e.native_hr.is_some()),
    })
}
fn fault_error(value: SvgFault) -> Result<Error, CoreError> {
    let kind = match value.kind {
        1 => FailureKind::Native,
        2 => FailureKind::Quota,
        3 => FailureKind::Allocation,
        4 => FailureKind::Cancelled,
        5 => FailureKind::Decode,
        6 => FailureKind::Unsupported,
        7 => FailureKind::Invariant,
        16 => FailureKind::ExternalResource,
        17 => FailureKind::UnsupportedImageData,
        18 => FailureKind::InvalidImageHeader,
        19 => FailureKind::ImageDecode,
        20 => FailureKind::MalformedImageData,
        21 => FailureKind::NestedImageDecode,
        22 => FailureKind::RasterAllocation,
        23 => FailureKind::InvalidFilterRegion,
        24 => FailureKind::FontReplay,
        _ => return Err(CoreError::InvalidRange),
    };
    let limit = match value.limit {
        0 => SvgLimit::None,
        1 => SvgLimit::Bytes,
        2 => SvgLimit::Scratch,
        3 => SvgLimit::Children,
        4 => SvgLimit::SvgDepth,
        5 => SvgLimit::ChildEncoded,
        6 => SvgLimit::XmlNodes,
        7 => SvgLimit::XmlAttributes,
        8 => SvgLimit::XmlElementAttributes,
        9 => SvgLimit::XmlDepth,
        10 => SvgLimit::Namespaces,
        11 => SvgLimit::Work,
        12 => SvgLimit::Capacity,
        13 => SvgLimit::InputBytes,
        14 => SvgLimit::ExpandedNodes,
        15 => SvgLimit::ConvertedSegments,
        _ => return Err(CoreError::InvalidRange),
    };
    if value.native_hr_present > 1
        || (kind == FailureKind::Native && (value.native_hr_present != 1 || value.native_hr == 0))
        || (kind != FailureKind::Native && (value.native_hr_present != 0 || value.native_hr != 0))
    {
        return Err(CoreError::InvalidRange);
    }
    Ok(Error {
        kind,
        limit,
        native_hr: (value.native_hr_present == 1).then_some(value.native_hr),
    })
}
unsafe fn resource_action(
    handle: *mut SvgResources,
    error: *mut WireError,
    cleanup: bool,
    work: impl FnOnce(&SvgResources) -> Result<(), CoreError>,
) -> i32 {
    // SAFETY: caller pins the immutable capability; only AccountAccess mutates its
    // disjoint account under its own short lock. No handle borrow spans a callback.
    unsafe {
        action(error, || {
            let resource = &*handle;
            if !cleanup && resource.fault.get().is_some() {
                return Err(CoreError::InvalidState);
            }
            let result = work(resource);
            if let Err(e) = result {
                resource.fault.record(access::resource_error(e));
            }
            result
        })
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn notepads_markdown_svg_resources_abi_version() -> u32 {
    SVG_RESOURCES_ABI_VERSION
}

/// # Safety
/// Exclusive canonical account, no live calls, distinct valid aligned in/out/error.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn notepads_markdown_svg_resources_create(
    abi_version: u32,
    account: *mut *mut Owned<SvgWork>,
    output: *mut *mut SvgResources,
    error: *mut WireError,
) -> i32 {
    preflight!(
        aligned(account)
            && aligned(output)
            && distinct(&[region(account), region(output), region(error)])
    );
    if abi_version != SVG_RESOURCES_ABI_VERSION {
        return STATUS_ABI_MISMATCH;
    }
    // SAFETY: ownership is transferred only after actual allocation/admission.
    unsafe {
        action(error, || {
            let original = account.read();
            if !valid_handle(original)
                || !distinct(&[
                    region(original),
                    region(account),
                    region(output),
                    region(error),
                ])
            {
                return Err(CoreError::InvalidBoundary);
            }
            let mut pending = original;
            let mut owner = AccountOwner::take(&mut pending)?;
            let host = owner.host();
            let bytes = resource_layout()
                .size()
                .checked_add(fault_layout().size())
                .ok_or(CoreError::ArithmeticOverflow)? as u64;
            let payment = match host.with(|a, _| a.reserve(0, bytes)) {
                Ok(t) => t,
                Err(e) => {
                    drop(host);
                    owner.reclaim()?;
                    return Err(e);
                }
            };
            let pointer = if injected(1) {
                ptr::null_mut()
            } else {
                std::alloc::alloc(resource_layout()).cast::<SvgResources>()
            };
            if pointer.is_null() {
                host.with_cleanup(true, |a, _| a.release_after_free(payment))?;
                drop(host);
                owner.reclaim()?;
                return Err(CoreError::OutOfMemory);
            }
            let fault = FirstFailureRef::new_prepaid();
            if let Err(e) = host.with(|a, _| a.admit(payment, bytes)) {
                drop(fault);
                std::alloc::dealloc(pointer.cast(), resource_layout());
                host.with_cleanup(true, |a, _| a.release_after_free(payment))?;
                drop(host);
                owner.reclaim()?;
                return Err(e);
            }
            pointer.write(SvgResources {
                owner: ManuallyDrop::new(owner),
                host: ManuallyDrop::new(host),
                fault: ManuallyDrop::new(fault),
                payment,
            });
            account.write(ptr::null_mut());
            output.write(pointer);
            Ok(())
        })
    }
}

/// # Safety
/// Live capability and independent readable token/writable error; actual storage
/// is not freed on admission failure. All output regions are independent.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn notepads_markdown_svg_resources_reserve(
    handle: *mut SvgResources,
    kind: u32,
    bytes: u64,
    output: *mut SvgWorkToken,
    error: *mut WireError,
) -> i32 {
    preflight!(
        valid_resource(handle)
            && aligned(output)
            && distinct(&[region(handle), region(output), region(error)])
    );
    unsafe {
        resource_action(handle, error, false, |r| {
            let token = r.host.with(|a, _| a.reserve_resource(kind, bytes))?;
            output.write(token);
            Ok(())
        })
    }
}
#[unsafe(no_mangle)]
pub unsafe extern "C" fn notepads_markdown_svg_resources_admit(
    handle: *mut SvgResources,
    token: *const SvgWorkToken,
    actual: u64,
    error: *mut WireError,
) -> i32 {
    preflight!(
        valid_resource(handle)
            && aligned(token)
            && distinct(&[region(handle), region(token), region(error)])
    );
    unsafe {
        resource_action(handle, error, false, |r| {
            r.host.with(|a, _| a.admit(token.read(), actual))
        })
    }
}
#[unsafe(no_mangle)]
pub unsafe extern "C" fn notepads_markdown_svg_resources_release_after_free(
    handle: *mut SvgResources,
    token: *const SvgWorkToken,
    error: *mut WireError,
) -> i32 {
    preflight!(
        valid_resource(handle)
            && aligned(token)
            && distinct(&[region(handle), region(token), region(error)])
    );
    unsafe {
        resource_action(handle, error, true, |r| {
            r.host
                .with_cleanup(true, |a, _| a.release_after_free(token.read()))
        })
    }
}
#[unsafe(no_mangle)]
pub unsafe extern "C" fn notepads_markdown_svg_resources_checkpoint(
    handle: *mut SvgResources,
    work: u64,
    error: *mut WireError,
) -> i32 {
    preflight!(valid_resource(handle) && distinct(&[region(handle), region(error)]));
    unsafe { resource_action(handle, error, false, |r| r.host.checkpoint_core(work)) }
}
#[unsafe(no_mangle)]
pub unsafe extern "C" fn notepads_markdown_svg_resources_record_fault(
    handle: *mut SvgResources,
    fault: *const SvgFault,
    error: *mut WireError,
) -> i32 {
    preflight!(
        valid_resource(handle)
            && aligned(fault)
            && distinct(&[region(handle), region(fault), region(error)])
    );
    unsafe {
        action(error, || {
            let value = fault_error(fault.read())?;
            (*handle).fault.record(value);
            Ok(())
        })
    }
}
#[unsafe(no_mangle)]
pub unsafe extern "C" fn notepads_markdown_svg_resources_read_fault(
    handle: *const SvgResources,
    output: *mut SvgFault,
    error: *mut WireError,
) -> i32 {
    preflight!(
        valid_resource(handle)
            && aligned(output)
            && distinct(&[region(handle), region(output), region(error)])
    );
    unsafe {
        action(error, || {
            output.write(fault_wire((*handle).fault.get()));
            Ok(())
        })
    }
}

/// # Safety
/// A live pinned capability and independent writable header-first snapshot/error.
/// This copies diagnostics under the original account's short transaction; it
/// never exposes its transferred raw handle or authorizes new work/publication.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn notepads_markdown_svg_resources_read_work(
    handle: *const SvgResources,
    output: *mut SvgWorkSnapshot,
    error: *mut WireError,
) -> i32 {
    preflight!(
        valid_resource(handle)
            && aligned(output)
            && distinct(&[region(handle), region(output), region(error)])
    );
    if let Err(status) = unsafe { svg_header(output) } {
        return status;
    }
    unsafe {
        action(error, || {
            let snapshot = (*handle)
                .host
                .with_cleanup(true, |account, _| Ok(account.snapshot()))?;
            output.write(snapshot);
            Ok(())
        })
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn notepads_markdown_svg_resources_retained_prepare(
    abi_version: u32,
    handle: *mut SvgResources,
    kind: u32,
    image: *const Owned<ImageBytes>,
    allocation: *mut *mut Owned<crate::core::preview::PreviewAllocation>,
    total: u64,
    payload: u64,
    output: *mut *mut Owned<SvgRetained>,
    initial: *mut SvgWorkToken,
    error: *mut WireError,
) -> i32 {
    preflight!(
        valid_resource(handle)
            && aligned(allocation)
            && aligned(output)
            && aligned(initial)
            && (image.is_null() || valid_handle(image))
    );
    preflight!(distinct(&[
        region(handle),
        region(allocation),
        region(output),
        region(initial),
        region(error),
        (
            image as usize,
            if image.is_null() {
                0
            } else {
                size_of::<Owned<ImageBytes>>()
            }
        )
    ]));
    unsafe { output.write(ptr::null_mut()) };
    if abi_version != SVG_RETAINED_ABI_VERSION {
        return STATUS_ABI_MISMATCH;
    }
    unsafe {
        resource_action(handle, error, false, |r| {
            r.host.with(|a, _| {
                retained::prepare_internal(
                    a, kind, image, allocation, total, payload, output, initial,
                )
            })
        })
    }
}
#[unsafe(no_mangle)]
pub unsafe extern "C" fn notepads_markdown_svg_resources_borrow_retained(
    handle: *mut SvgResources,
    retained: *mut Owned<SvgRetained>,
    output: *mut SvgWorkToken,
    error: *mut WireError,
) -> i32 {
    preflight!(
        valid_resource(handle)
            && valid_handle(retained)
            && aligned(output)
            && distinct(&[
                region(handle),
                region(retained),
                region(output),
                region(error)
            ])
    );
    unsafe {
        resource_action(handle, error, false, |r| {
            r.host
                .with(|a, _| retained::borrow_internal(a, retained, output))
        })
    }
}
#[unsafe(no_mangle)]
pub unsafe extern "C" fn notepads_markdown_svg_resources_return_retained(
    handle: *mut SvgResources,
    token: *const SvgWorkToken,
    error: *mut WireError,
) -> i32 {
    preflight!(
        valid_resource(handle)
            && aligned(token)
            && distinct(&[region(handle), region(token), region(error)])
    );
    unsafe {
        resource_action(handle, error, true, |r| {
            r.host
                .with_cleanup(true, |a, _| retained::return_internal(a, token.read()))
        })
    }
}

/// # Safety
/// Exclusive canonical capability owner. All calls/Native callbacks/engine owners
/// retired. A consumed failed receipt is a permanent quarantine pin, never a
/// reusable account. Inspect consumed even if this function reports panic.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn notepads_markdown_svg_resources_reclaim(
    owner_pointer: *mut *mut SvgResources,
    output: *mut SvgResourcesRetirement,
    error: *mut WireError,
) -> i32 {
    preflight!(
        aligned(owner_pointer)
            && aligned(output)
            && distinct(&[region(owner_pointer), region(output), region(error)])
    );
    let handle = unsafe { owner_pointer.read() };
    preflight!(
        valid_resource(handle)
            && distinct(&[
                region(handle),
                region(owner_pointer),
                region(output),
                region(error)
            ])
    );
    let header = unsafe { output.cast::<SvgWorkHeader>().read() };
    if header.abi_version != SVG_RESOURCES_ABI_VERSION
        || header.struct_size as usize != size_of::<SvgResourcesRetirement>()
    {
        return STATUS_ABI_MISMATCH;
    }
    let mut committed = false;
    let status = unsafe {
        action(error, || {
            let resource = &*handle;
            if !resource.owner.has_exact_references(2) || !resource.fault.has_exact_owners(1) {
                return Err(CoreError::InvalidState);
            }
            if resource.fault.get().is_some() {
                resource.host.with_cleanup(true, |a, _| {
                    a.funding.close();
                    Ok(())
                })?;
            }
            let mut receipt = SvgResourcesRetirement {
                header,
                consumed: 1,
                cleanup_succeeded: 0,
                account: resource.owner.canonical_pin(),
                #[cfg(target_pointer_width = "32")]
                pointer_padding: 0,
                first_fault: fault_wire(resource.fault.get()),
                unreleased_header: resource.payment,
                unreleased_account: resource.owner.bootstrap_token(),
            };
            // Publish the stable Native-owned receipt before the first irreversible
            // action. It remains valid when any subsequent step fails or panics.
            output.write(receipt);
            owner_pointer.write(ptr::null_mut());
            committed = true;
            let mut moved = handle.read();
            std::alloc::dealloc(handle.cast(), resource_layout());
            drop(ManuallyDrop::take(&mut moved.fault));
            let host = ManuallyDrop::take(&mut moved.host);
            let mut owner = ManuallyDrop::take(&mut moved.owner);
            if injected(3) {
                panic!("injected post-consumption resource retirement panic");
            }
            let header_result = if injected(2) {
                Err(CoreError::InternalInvariant)
            } else {
                host.with_cleanup(true, |a, _| a.release_after_free(moved.payment))
            };
            if header_result.is_ok() {
                receipt.unreleased_header = SvgWorkToken::default();
                output.write(receipt);
            }
            drop(host);
            let account_result = owner.reclaim();
            if let Ok(account) = account_result {
                receipt.account = account;
                receipt.unreleased_account = SvgWorkToken::default();
                output.write(receipt);
            }
            if let Err(e) = header_result.and(account_result.map(|_| ())) {
                // No external capability/reference survives this exclusive
                // retirement. Keep the recovered canonical account poisoned.
                (*receipt.account).value.poisoned = true;
                if receipt.first_fault.kind == 0 {
                    receipt.first_fault = fault_wire(Some(access::resource_error(e)));
                }
                output.write(receipt);
                return Err(e);
            }
            receipt.cleanup_succeeded = 1;
            output.write(receipt);
            Ok(())
        })
    };
    if status == STATUS_PANIC && committed {
        unsafe {
            // The failed consumed receipt pins the original account. Forgotten
            // AccountOwner references have no remaining callable entry point.
            (*(*output).account).value.poisoned = true;
            if (*output).first_fault.kind == 0 {
                (*output).first_fault =
                    fault_wire(Some(Error::new(FailureKind::Invariant, SvgLimit::None)));
            }
        }
    }
    status
}

#[cfg(test)]
pub(super) unsafe fn engine_resources(
    handle: *const SvgResources,
) -> (Arc<AccountAccess>, FirstFailureRef) {
    // SAFETY: embedding worker pins the capability, no reclaim overlaps.
    unsafe {
        let resource = &*handle;
        (
            Arc::clone(&resource.host),
            FirstFailureRef::clone(&resource.fault),
        )
    }
}

#[cfg(test)]
thread_local! { static INJECTION: std::cell::Cell<u8> = const { std::cell::Cell::new(0) }; }
fn injected(point: u8) -> bool {
    #[cfg(test)]
    {
        return INJECTION.with(|slot| {
            if slot.get() == point {
                slot.set(0);
                true
            } else {
                false
            }
        });
    }
    #[cfg(not(test))]
    {
        let _ = point;
        false
    }
}
#[cfg(test)]
pub(super) fn inject(point: u8) {
    INJECTION.with(|slot| slot.set(point));
}

/// The single renderer claim belongs to the original account, not this shell.
pub(super) unsafe fn claim_renderer(
    handle: *mut SvgResources,
) -> Result<(Arc<AccountAccess>, FirstFailureRef, CancellationToken), CoreError> {
    unsafe {
        let r = &*handle;
        if r.fault.get().is_some() {
            return Err(CoreError::InvalidState);
        }
        let token = r.host.with(|a, op| {
            if a.renderer_phase != 0 || a.funding.stats().closed || !a.funding.is_current() {
                return Err(CoreError::InvalidState);
            }
            op.boundary_work().checkpoint_now()?;
            a.renderer_phase = 1;
            Ok(op.cancellation())
        })?;
        Ok((Arc::clone(&r.host), FirstFailureRef::clone(&r.fault), token))
    }
}
pub(super) unsafe fn complete_renderer_claim(
    handle: *mut SvgResources,
    commit: bool,
) -> Result<(), CoreError> {
    unsafe {
        (*handle).host.with_cleanup(true, |a, _| {
            if a.renderer_phase != 1 {
                return Err(CoreError::InvalidState);
            }
            a.renderer_phase = if commit { 2 } else { 0 };
            Ok(())
        })
    }
}
pub(super) unsafe fn renderer_fault(handle: *const SvgResources) -> Option<Error> {
    unsafe { (*handle).fault.get() }
}
pub(super) unsafe fn record_renderer_fault(handle: *const SvgResources, error: Error) -> Error {
    unsafe { (*handle).fault.record(error) }
}
pub(super) unsafe fn renderer_owners_clear(handle: *const SvgResources, host_refs: usize) -> bool {
    unsafe {
        let r = &*handle;
        r.owner.has_exact_references(host_refs) && r.fault.has_exact_owners(1)
    }
}

pub(super) unsafe fn renderer_binding_matches(
    handle: *mut SvgResources,
    nonce: u64,
    source: u64,
    provider: u64,
) -> Result<bool, CoreError> {
    unsafe {
        (*handle).host.with(|a, _| {
            let key = a.funding.request_key();
            Ok(a.nonce == nonce
                && key.epoch.source_generation == source
                && key.epoch.provider_generation == provider)
        })
    }
}
