// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE file in the project root for license information.
// ---------------------------------------------------------------------------------------------

//! Canonical retained allocation state. Core owns funding; this boundary owns
//! exact record Layouts, intrusive refs, and acknowledgement of physical events.

use super::*;
use crate::core::preview::PreviewAllocation;
use markdown_core::resources::svg_work::CoreRetainedFunding;

pub const SVG_RETAINED_ABI_VERSION: u32 = 1;
const PAYLOAD: u32 = 0;
const METADATA: u32 = 1;

#[derive(Clone, Copy, Eq, PartialEq)]
enum Phase {
    NeverAllocated,
    AwaitExistingBind,
    Bound,
    Admitted,
    Retiring,
    Retired,
}

impl Phase {
    fn wire(self) -> u32 {
        match self {
            Self::NeverAllocated | Self::AwaitExistingBind => 0,
            Self::Bound => 1,
            Self::Admitted => 2,
            Self::Retired => 3,
            Self::Retiring => 4,
        }
    }
}

struct RetainedState {
    funding: CoreRetainedFunding,
    header_charge: Option<SvgHeaderCharge>,
    // Borrow only, never a separate owner. Read solely before physical free
    // under canonical Native ownership; cleanup never dereferences this lease.
    preview_allocation: *mut Owned<PreviewAllocation>,
    phase: Phase,
}

pub struct SvgRetained {
    refs: AtomicU64,
    state: Mutex<RetainedState>,
    owner_nonce: u64,
    payload_capacity: u64,
    kind: u32,
}

#[repr(C, align(8))]
#[derive(Clone, Copy, Default)]
pub struct SvgRetainedSnapshot {
    pub header: SvgWorkHeader,
    pub owner_nonce: u64,
    pub native_bytes: u64,
    pub preview_bytes: u64,
    pub header_bytes: u64,
    pub job_borrow_bytes: u64,
    pub refs: u64,
    pub root_live_bytes: u64,
    pub control_bytes: u64,
    pub phase: u32,
    pub kind: u32,
}

const _: () = {
    assert!(size_of::<SvgRetainedSnapshot>() == 80);
    assert!(align_of::<SvgRetainedSnapshot>() == 8);
    assert!(std::mem::offset_of!(SvgRetainedSnapshot, phase) == 72);
};

fn publish_row(
    account: &mut SvgWork,
    slot: u32,
    previous: TicketRow,
    retained: *mut Owned<SvgRetained>,
    bytes: u64,
) -> SvgWorkToken {
    // The row was validated before the funding transaction, with exclusive
    // access to this account. No fallible operation follows publication.
    let row = account.arena.get_mut(slot).expect("prepared free row");
    row.bytes = bytes;
    row.kind = 0;
    row.state = TicketState::Borrowed;
    row.retained = retained;
    account.free_head = previous.next;
    account.tickets += 1;
    SvgWorkToken {
        account_nonce: account.nonce,
        slot,
        generation: previous.generation,
    }
}

/// # Safety
/// Exclusive account and independent writable output/in-out storage. The image
/// and existing lease, if any, remain pinned and cannot be admitted/free'd
/// concurrently. Null lease requests a real captured-root preview allocation.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn notepads_markdown_svg_retained_prepare(
    abi_version: u32,
    account: *mut Owned<SvgWork>,
    kind: u32,
    image_bytes: *const Owned<ImageBytes>,
    root_allocation: *mut *mut Owned<PreviewAllocation>,
    requested_total: u64,
    payload_capacity: u64,
    output: *mut *mut Owned<SvgRetained>,
    initial_borrow: *mut SvgWorkToken,
    error: *mut WireError,
) -> i32 {
    handle_output!(output);
    if abi_version != SVG_RETAINED_ABI_VERSION {
        return STATUS_ABI_MISMATCH;
    }
    preflight!(
        valid_handle(account)
            && !root_allocation.is_null()
            && (root_allocation as usize).is_multiple_of(align_of::<usize>())
            && !initial_borrow.is_null()
            && (initial_borrow as usize).is_multiple_of(align_of::<SvgWorkToken>())
            && (image_bytes.is_null() || valid_handle(image_bytes))
    );
    // SAFETY: preflight covers pointer storage; validity/lifetime is the caller's
    // ABI obligation. No canonical output is published before complete success.
    unsafe {
        work_action(account, error, |account| {
            prepare_internal(
                account,
                kind,
                image_bytes,
                root_allocation,
                requested_total,
                payload_capacity,
                output,
                initial_borrow,
            )
        })
    }
}

/// # Safety
/// One canonical physical owner, immediately after successful allocation and
/// prefix installation, without an intervening fallible operation.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn notepads_markdown_svg_retained_bind_physical(
    retained: *mut Owned<SvgRetained>,
) {
    if !valid_handle(retained) {
        return;
    }
    let _ = contain_panic(|| {
        // SAFETY: the caller owns the physical reference throughout this call.
        let value = unsafe { &(*retained).value };
        let mut state = value.state.lock().unwrap_or_else(|e| e.into_inner());
        if matches!(
            state.phase,
            Phase::NeverAllocated | Phase::AwaitExistingBind
        ) {
            state.phase = Phase::Bound;
        }
        0
    });
}

/// # Safety
/// Live physical owner. Its original preview lease remains live and exclusively
/// admitted by Native; no physical free overlaps this admission.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn notepads_markdown_svg_retained_admit(
    retained: *mut Owned<SvgRetained>,
    error: *mut WireError,
) -> i32 {
    preflight!(valid_handle(retained));
    // SAFETY: physical owner pins the record and its original lease until free.
    unsafe {
        action(error, || {
            let value = &(*retained).value;
            let mut state = value.state.lock().unwrap_or_else(|e| e.into_inner());
            if state.phase != Phase::Bound {
                return Err(CoreError::InvalidState);
            }
            let preview = &*state.preview_allocation;
            let stats = state.funding.stats();
            if !preview.value.admitted
                || preview.value.svg_attribution != value.owner_nonce
                || preview.value.reservation.reserved_bytes() < stats.native_bytes
                || !state.funding.belongs_to(&preview.value.reservation)
                || !state.funding.belongs_to(&preview.lease)
                || (value.kind == PAYLOAD
                    && state
                        .funding
                        .image()
                        .and_then(ByteReservation::actual_bytes)
                        != Some(value.payload_capacity))
            {
                return Err(CoreError::InvalidState);
            }
            state.phase = Phase::Admitted;
            Ok(())
        })
    }
}

/// # Safety
/// Exclusive account and pinned live canonical physical owner. Native must also
/// pin its immutable payload owner before reading bytes; this token is funding.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn notepads_markdown_svg_work_borrow_retained(
    account: *mut Owned<SvgWork>,
    retained: *mut Owned<SvgRetained>,
    output: *mut SvgWorkToken,
    error: *mut WireError,
) -> i32 {
    preflight!(
        valid_handle(account)
            && valid_handle(retained)
            && !output.is_null()
            && (output as usize).is_multiple_of(align_of::<SvgWorkToken>())
    );
    // SAFETY: caller pins both handles. The record mutex linearizes admission,
    // physical retirement, and each new job borrow.
    unsafe {
        work_action(account, error, |account| {
            borrow_internal(account, retained, output)
        })
    }
}

/// Drop one already-owned reference. Every mutation/ref addition happens while
/// a physical or row reference pins this record. The final drop is exclusive.
unsafe fn release_ref(retained: *mut Owned<SvgRetained>) {
    // SAFETY: the caller transfers one owned, live intrusive reference.
    let value = unsafe { &(*retained).value };
    if value.refs.fetch_sub(1, Ordering::AcqRel) != 1 {
        return;
    }
    let header = {
        let mut state = value.state.lock().unwrap_or_else(|e| e.into_inner());
        debug_assert!(state.phase == Phase::Retired);
        state.header_charge.take()
    };
    // SAFETY: the final reference transferred unique ownership; release moves
    // the root H lease out, deallocates the record, then drops its root funding.
    unsafe { release(retained) };
    // H's control charge retires only after real Owned deallocation.
    drop(header);
}

/// # Safety
/// Exclusive account, readable original token, independent writable error. This
/// cleanup remains available after poison, source retirement, close or cancel.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn notepads_markdown_svg_work_return_retained(
    account: *mut Owned<SvgWork>,
    token: *const SvgWorkToken,
    error: *mut WireError,
) -> i32 {
    preflight!(
        valid_handle(account)
            && !token.is_null()
            && (token as usize).is_multiple_of(align_of::<SvgWorkToken>())
    );
    // SAFETY: this path intentionally bypasses work_action's poisoned rejection;
    // action still validates independent Core error storage and contains panic.
    let status = unsafe {
        action(error, || {
            return_internal(&mut (*account).value, token.read())
        })
    };
    if status == STATUS_PANIC {
        unsafe { (*account).value.poisoned = true };
    }
    status
}

unsafe fn retire_physical(retained: *mut Owned<SvgRetained>, unallocated: bool) -> bool {
    // SAFETY: canonical caller pins the physical reference through this call.
    let value = unsafe { &(*retained).value };
    let retirement = {
        let mut state = value.state.lock().unwrap_or_else(|e| e.into_inner());
        if if unallocated {
            state.phase != Phase::NeverAllocated
        } else {
            !matches!(state.phase, Phase::Bound | Phase::Admitted)
        } {
            return false;
        }
        // Abort requires the creator borrow returned first. No further borrow
        // can exist before admission, so physical must be the last reference.
        if unallocated && value.refs.load(Ordering::Acquire) != 1 {
            return false;
        }
        // Revoke admission before moving the refund guard out of the lock.
        // A failed/unwound retirement must never look successfully retired.
        state.phase = Phase::Retiring;
        state.preview_allocation = ptr::null_mut();
        state.funding.take_physical_retirement()
    };
    // Image destruction may reenter its policy, so drop outside record mutex.
    #[cfg(test)]
    if tests::take_retirement_panic() {
        panic!("injected retained refund unwind before phase commit");
    }
    drop(retirement);
    {
        let mut state = value.state.lock().unwrap_or_else(|e| e.into_inner());
        state.phase = Phase::Retired;
    }
    unsafe { release_ref(retained) };
    true
}

/// # Safety
/// Canonical proof: HeapFree succeeded, and the original preview lease has been
/// destroyed/deallocated. No access to its old address occurs in this function.
/// This call may deallocate the record. Retain an independent borrow before
/// reading the record after return.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn notepads_markdown_svg_retained_after_physical_free_checked(
    retained: *mut Owned<SvgRetained>,
) -> i32 {
    if !valid_handle(retained) {
        return STATUS_INVALID_ARGUMENT;
    }
    contain_panic(|| {
        // SAFETY: caller transfers the uniquely acknowledged physical reference.
        if unsafe { retire_physical(retained, false) } {
            crate::STATUS_OK
        } else {
            crate::STATUS_INVALID_STATE
        }
    })
}

/// # Safety
/// Never allocated by Native. Return creator token, then physically destroy the
/// returned preview lease, then call this function. Bound/adopted owners reject.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn notepads_markdown_svg_retained_abort_unallocated(
    retained: *mut Owned<SvgRetained>,
) {
    if !valid_handle(retained) {
        return;
    }
    let _ = contain_panic(|| {
        unsafe { retire_physical(retained, true) };
        0
    });
}

/// # Safety
/// A physical or borrow reference pins this handle; output/error are independent
/// writable storage. Retired snapshots require an outstanding borrow reference.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn notepads_markdown_svg_retained_read(
    retained: *const Owned<SvgRetained>,
    output: *mut SvgRetainedSnapshot,
    error: *mut WireError,
) -> i32 {
    preflight!(valid_handle(retained));
    if output.is_null() || !(output as usize).is_multiple_of(align_of::<SvgRetainedSnapshot>()) {
        return STATUS_INVALID_ARGUMENT;
    }
    // SAFETY: caller supplies the readable versioned prefix before full write.
    let prefix = unsafe { output.cast::<SvgWorkHeader>().read() };
    if prefix.abi_version != SVG_RETAINED_ABI_VERSION
        || prefix.struct_size as usize != size_of::<SvgRetainedSnapshot>()
    {
        return STATUS_ABI_MISMATCH;
    }
    unsafe {
        action(error, || {
            let value = &(*retained).value;
            let state = value.state.lock().unwrap_or_else(|e| e.into_inner());
            let stats = state.funding.stats();
            output.write(SvgRetainedSnapshot {
                header: SvgWorkHeader {
                    abi_version: SVG_RETAINED_ABI_VERSION,
                    struct_size: size_of::<SvgRetainedSnapshot>() as u32,
                },
                owner_nonce: value.owner_nonce,
                native_bytes: stats.native_bytes,
                preview_bytes: stats.preview_bytes,
                header_bytes: stats.header_bytes,
                job_borrow_bytes: state.funding.total_bytes(),
                refs: value.refs.load(Ordering::Acquire),
                root_live_bytes: stats.root_live_bytes,
                control_bytes: stats.control_bytes,
                phase: state.phase.wire(),
                kind: value.kind,
            });
            Ok(())
        })
    }
}

#[cfg(test)]
mod tests;

/// Internal transaction; external pointer validity is established by its ABI caller.
pub(super) unsafe fn prepare_internal(
    account: &mut SvgWork,
    kind: u32,
    image_bytes: *const Owned<ImageBytes>,
    root_allocation: *mut *mut Owned<PreviewAllocation>,
    requested_total: u64,
    payload_capacity: u64,
    output: *mut *mut Owned<SvgRetained>,
    initial_borrow: *mut SvgWorkToken,
) -> Result<(), CoreError> {
    unsafe {
        if requested_total == 0
            || requested_total < payload_capacity
            || !matches!(kind, PAYLOAD | METADATA)
            || (kind == PAYLOAD && (image_bytes.is_null() || payload_capacity == 0))
            || (kind == METADATA && (!image_bytes.is_null() || payload_capacity != 0))
        {
            return Err(CoreError::InvalidRange);
        }
        usize::try_from(requested_total).map_err(|_| CoreError::ArithmeticOverflow)?;
        let original = root_allocation.read();
        // Both payload and metadata may already have an admitted allocation
        // from this same model root. Attribution adds the control/job claim;
        // it must not reserve those existing native/root-record bytes again.
        if !original.is_null() && !valid_handle(original) {
            return Err(CoreError::InvalidBoundary);
        }
        let image = (!image_bytes.is_null()).then(|| &(*image_bytes).value.bytes);
        if let Some(image) = image {
            let image_valid = if original.is_null() {
                image.actual_bytes().is_none() && image.reserved_bytes() >= payload_capacity
            } else {
                image.actual_bytes() == Some(payload_capacity)
            };
            if !image_valid {
                return Err(CoreError::InvalidState);
            }
        }
        if !original.is_null()
            && (!(*original).value.admitted
                || (*original).value.svg_attribution != 0
                || (*original).value.reservation.reserved_bytes() < requested_total)
        {
            return Err(CoreError::InvalidState);
        }
        // Identity/root rejection is pure: a foreign caller must not grow
        // this account's arena before its allocation provenance is known.
        if !original.is_null()
            && (!account
                .funding
                .retained_root_matches(&(*original).value.reservation)
                || !account.funding.retained_root_matches(&(*original).lease))
        {
            return Err(CoreError::WrongBudget);
        }
        account
            .funding
            .validate_retained_prepare(
                image,
                requested_total,
                size_of::<Owned<PreviewAllocation>>() as u64,
                size_of::<Owned<SvgRetained>>() as u64,
            )
            .map_err(CoreError::from)?;
        account
            .tickets
            .checked_add(1)
            .ok_or(CoreError::ArithmeticOverflow)?;
        account.ensure_free()?;
        let slot_index = account.free_head;
        let previous = *account.arena.get(slot_index)?;
        if previous.state != TicketState::Free {
            return Err(CoreError::InternalInvariant);
        }
        let nonce = next_nonce()?;
        // Declare bootstrap before both exact Slots/reservation. Their
        // failed physical allocations retire before control/job rollback.
        let bootstrap = account
            .funding
            .prepare_retained(
                image,
                requested_total,
                size_of::<Owned<PreviewAllocation>>() as u64,
                size_of::<Owned<SvgRetained>>() as u64,
            )
            .map_err(CoreError::from)?;
        if !original.is_null()
            && (!(*original).value.reservation.belongs_to(bootstrap.budget())
                || !(*original).lease.belongs_to(bootstrap.budget()))
        {
            return Err(CoreError::WrongBudget);
        }
        let mut op = bootstrap.operation();
        let preview_slot = if original.is_null() {
            Some(Slot::<PreviewAllocation>::new(op.boundary_work())?)
        } else {
            None
        };
        let native_reservation = if original.is_null() {
            Some(op.reserve_boundary(requested_total)?)
        } else {
            None
        };
        let retained_slot = Slot::<SvgRetained>::new(op.boundary_work())?;
        op.boundary_work().checkpoint_now()?;
        let (funding, header_charge) = bootstrap.commit();
        let bytes = funding.total_bytes();
        let preview = match (preview_slot, native_reservation) {
            (Some(slot), Some(reservation)) => slot.commit(PreviewAllocation {
                reservation,
                admitted: false,
                svg_attribution: nonce,
            }),
            (None, None) => original,
            _ => unreachable!("paired new preview reservation"),
        };
        (*preview).value.svg_attribution = nonce;
        let retained = retained_slot.commit(SvgRetained {
            refs: AtomicU64::new(2), // physical owner plus creator row
            state: Mutex::new(RetainedState {
                funding,
                header_charge: Some(header_charge),
                preview_allocation: preview,
                phase: if original.is_null() {
                    Phase::NeverAllocated
                } else {
                    Phase::AwaitExistingBind
                },
            }),
            owner_nonce: nonce,
            payload_capacity,
            kind,
        });
        let token = publish_row(account, slot_index, previous, retained, bytes);
        root_allocation.write(preview);
        initial_borrow.write(token);
        output.write(retained);
        Ok(())
    }
}

/// Internal transaction; external pointer validity is established by its ABI caller.
pub(super) unsafe fn borrow_internal(
    account: &mut SvgWork,
    retained: *mut Owned<SvgRetained>,
    output: *mut SvgWorkToken,
) -> Result<(), CoreError> {
    unsafe {
        let value = &(*retained).value;
        let state = value.state.lock().unwrap_or_else(|e| e.into_inner());
        if state.phase != Phase::Admitted {
            return Err(CoreError::InvalidState);
        }
        for slot in 0..account.arena.capacity {
            let row = account.arena.get(slot)?;
            if row.state == TicketState::Borrowed && row.retained == retained {
                return Err(CoreError::InvalidState);
            }
        }
        account
            .tickets
            .checked_add(1)
            .ok_or(CoreError::ArithmeticOverflow)?;
        if value.refs.load(Ordering::Acquire) == u64::MAX {
            return Err(CoreError::ArithmeticOverflow);
        }
        account
            .funding
            .validate_retained_borrow(&state.funding)
            .map_err(CoreError::from)?;
        account.ensure_free()?;
        let slot = account.free_head;
        let previous = *account.arena.get(slot)?;
        if previous.state != TicketState::Free {
            return Err(CoreError::InternalInvariant);
        }
        let bytes = account
            .funding
            .borrow_retained(&state.funding)
            .map_err(CoreError::from)?;
        value.refs.fetch_add(1, Ordering::Relaxed);
        output.write(publish_row(account, slot, previous, retained, bytes));
        Ok(())
    }
}

/// Cleanup transaction; caller pins the retained row and exclusively borrows account.
pub(super) unsafe fn return_internal(
    value: &mut SvgWork,
    token: SvgWorkToken,
) -> Result<(), CoreError> {
    unsafe {
        let row = *value.token_row(token)?;
        if row.state != TicketState::Borrowed || row.retained.is_null() {
            return Err(CoreError::InvalidState);
        }
        value
            .funding
            .return_retained(row.bytes)
            .map_err(CoreError::from)?;
        let target = value.arena.get_mut(token.slot)?;
        target.bytes = 0;
        target.retained = ptr::null_mut();
        if let Some(generation) = row.generation.checked_add(1) {
            target.generation = generation;
            target.state = TicketState::Free;
            target.next = value.free_head;
            value.free_head = token.slot;
        } else {
            target.state = TicketState::Retired;
            target.next = NO_SLOT;
        }
        value.tickets -= 1;
        release_ref(row.retained);
        Ok(())
    }
}
