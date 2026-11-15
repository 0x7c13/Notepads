// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE file in the project root for license information.
// ---------------------------------------------------------------------------------------------

//! Private SVG work funding. The exact-layout ticket arena lives at this unsafe
//! storage boundary; Core owns the existing model/control capacity decisions.

use super::*;
use markdown_core::resources::svg_work::{SvgHeaderCharge, SvgWorkAccount as WorkFunding};
use std::sync::atomic::AtomicU64;

pub const SVG_WORK_ABI_VERSION: u32 = 1;
mod access;
mod renderer_job;
mod resources_bridge;
pub use renderer_job::*;
pub use resources_bridge::*;
mod retained;
pub use retained::*;
const NO_SLOT: u32 = u32::MAX;
static NEXT_ACCOUNT: AtomicU64 = AtomicU64::new(1);

#[repr(C, align(8))]
#[derive(Clone, Copy, Default, Debug, Eq, PartialEq)]
pub struct SvgWorkToken {
    pub account_nonce: u64,
    pub slot: u32,
    pub generation: u32,
}

#[repr(C, align(8))]
#[derive(Clone, Copy, Default)]
pub struct SvgWorkHeader {
    pub abi_version: u32,
    pub struct_size: u32,
}

#[repr(C, align(8))]
#[derive(Clone, Copy, Default)]
pub struct SvgWorkSnapshot {
    pub header: SvgWorkHeader,
    pub account_nonce: u64,
    pub job_bytes: u64,
    pub job_peak: u64,
    pub root_work_bytes: u64,
    pub control_bytes: u64,
    pub header_bytes: u64,
    pub arena_bytes: u64,
    pub tickets: u32,
    pub pending: u32,
    pub closed: u32,
    pub reserved: u32,
}

const _: () = {
    assert!(size_of::<SvgWorkToken>() == 16 && align_of::<SvgWorkToken>() == 8);
    assert!(size_of::<SvgWorkSnapshot>() == 80 && align_of::<SvgWorkSnapshot>() == 8);
    assert!(std::mem::offset_of!(SvgWorkSnapshot, tickets) == 64);
};

#[derive(Clone, Copy, Eq, PartialEq)]
#[repr(u32)]
enum TicketState {
    Free,
    Pending,
    Admitted,
    Borrowed,
    Retired,
}

#[derive(Clone, Copy)]
struct TicketRow {
    bytes: u64,
    generation: u32,
    next: u32,
    kind: u32,
    state: TicketState,
    retained: *mut Owned<SvgRetained>,
}

/// All capacity rows are initialized, contain no destructor or borrowed owner,
/// and are exclusively accessible through one account. Payloads live elsewhere.
struct TicketArena {
    pointer: NonNull<TicketRow>,
    capacity: u32,
}

impl TicketArena {
    fn empty() -> Self {
        Self {
            pointer: NonNull::dangling(),
            capacity: 0,
        }
    }

    fn layout(capacity: u32) -> Result<Layout, CoreError> {
        if capacity == 0 || capacity == NO_SLOT {
            return Err(CoreError::ArithmeticOverflow);
        }
        Layout::array::<TicketRow>(capacity as usize).map_err(|_| CoreError::ArithmeticOverflow)
    }

    /// Caller has already funded the complete new Layout on both real ledgers.
    fn allocate(capacity: u32) -> Result<Self, CoreError> {
        let layout = Self::layout(capacity)?;
        #[cfg(test)]
        if tests::take_arena_failure() {
            return Err(CoreError::OutOfMemory);
        }
        // SAFETY: nonzero valid Layout; null is a recoverable allocation failure.
        let pointer = NonNull::new(unsafe { alloc(layout) }.cast::<TicketRow>())
            .ok_or(CoreError::OutOfMemory)?;
        for index in 0..capacity {
            // SAFETY: every slot in this fresh exact allocation is written once.
            unsafe {
                pointer.as_ptr().add(index as usize).write(TicketRow {
                    bytes: 0,
                    generation: 1,
                    next: NO_SLOT,
                    kind: 0,
                    state: TicketState::Free,
                    retained: ptr::null_mut(),
                });
            }
        }
        Ok(Self { pointer, capacity })
    }

    fn bytes(&self) -> u64 {
        u64::from(self.capacity) * size_of::<TicketRow>() as u64
    }

    fn get(&self, slot: u32) -> Result<&TicketRow, CoreError> {
        if slot >= self.capacity {
            return Err(CoreError::UnknownId);
        }
        // SAFETY: initialized row in the allocation, shared for this borrow only.
        Ok(unsafe { &*self.pointer.as_ptr().add(slot as usize) })
    }

    fn get_mut(&mut self, slot: u32) -> Result<&mut TicketRow, CoreError> {
        if slot >= self.capacity {
            return Err(CoreError::UnknownId);
        }
        // SAFETY: account has an exclusive arena borrow; row is initialized.
        Ok(unsafe { &mut *self.pointer.as_ptr().add(slot as usize) })
    }
}

impl Drop for TicketArena {
    fn drop(&mut self) {
        if self.capacity != 0 {
            // SAFETY: exact original allocation; rows are Copy without destructors.
            unsafe {
                dealloc(
                    self.pointer.as_ptr().cast(),
                    Self::layout(self.capacity).unwrap(),
                );
            }
        }
    }
}

fn next_nonce() -> Result<u64, CoreError> {
    NEXT_ACCOUNT
        .try_update(Ordering::AcqRel, Ordering::Acquire, |value| {
            value.checked_add(1)
        })
        .map_err(|_| CoreError::ArithmeticOverflow)
}

unsafe fn svg_header(pointer: *const SvgWorkSnapshot) -> Result<(), i32> {
    if pointer.is_null() || !(pointer as usize).is_multiple_of(align_of::<SvgWorkSnapshot>()) {
        return Err(STATUS_INVALID_ARGUMENT);
    }
    // SAFETY: caller supplies the readable, aligned versioned prefix.
    let prefix = unsafe { pointer.cast::<SvgWorkHeader>().read() };
    if prefix.abi_version != SVG_WORK_ABI_VERSION
        || prefix.struct_size as usize != size_of::<SvgWorkSnapshot>()
    {
        return Err(STATUS_ABI_MISMATCH);
    }
    Ok(())
}

#[unsafe(no_mangle)]
pub extern "C" fn notepads_markdown_svg_work_abi_version() -> u32 {
    SVG_WORK_ABI_VERSION
}

pub struct SvgWork {
    // Successful teardown frees the arena explicitly, then moves the header
    // claim to the destroyer's stack before freeing this Owned allocation.
    arena: TicketArena,
    funding: WorkFunding,
    header_charge: Option<SvgHeaderCharge>,
    nonce: u64,
    free_head: u32,
    tickets: u32,
    pending: u32,
    poisoned: bool,
    font_bytes: u64,
    renderer_phase: u32,
}

impl SvgWork {
    fn snapshot(&self) -> SvgWorkSnapshot {
        let stats = self.funding.stats();
        SvgWorkSnapshot {
            header: SvgWorkHeader {
                abi_version: SVG_WORK_ABI_VERSION,
                struct_size: size_of::<SvgWorkSnapshot>() as u32,
            },
            account_nonce: self.nonce,
            job_bytes: stats.job_bytes,
            job_peak: stats.job_peak_bytes,
            root_work_bytes: stats.root_work_bytes,
            control_bytes: stats.control_bytes,
            header_bytes: stats.header_bytes,
            arena_bytes: self.arena.bytes(),
            tickets: self.tickets,
            pending: self.pending,
            closed: u32::from(stats.closed),
            reserved: 0,
        }
    }

    fn token_row(&self, token: SvgWorkToken) -> Result<&TicketRow, CoreError> {
        if token.account_nonce != self.nonce {
            return Err(CoreError::WrongBudget);
        }
        let row = self.arena.get(token.slot)?;
        if row.generation != token.generation
            || !matches!(
                row.state,
                TicketState::Pending | TicketState::Admitted | TicketState::Borrowed
            )
        {
            return Err(CoreError::UnknownId);
        }
        Ok(row)
    }

    fn ensure_free(&mut self) -> Result<(), CoreError> {
        if self.free_head != NO_SLOT {
            return Ok(());
        }
        let next_capacity = if self.arena.capacity == 0 {
            8
        } else {
            self.arena
                .capacity
                .checked_mul(2)
                .ok_or(CoreError::ArithmeticOverflow)?
        };
        let new_bytes = TicketArena::layout(next_capacity)?.size() as u64;
        self.funding
            .reserve_work_bytes(new_bytes)
            .map_err(CoreError::from)?;
        let mut next = match TicketArena::allocate(next_capacity) {
            Ok(value) => value,
            Err(error) => {
                self.funding
                    .release_freed_work_bytes(new_bytes)
                    .map_err(CoreError::from)?;
                return Err(error);
            }
        };
        let old_capacity = self.arena.capacity;
        if old_capacity != 0 {
            // SAFETY: disjoint exact allocations; all old rows are Copy and initialized.
            unsafe {
                ptr::copy_nonoverlapping(
                    self.arena.pointer.as_ptr(),
                    next.pointer.as_ptr(),
                    old_capacity as usize,
                );
            }
        }
        // Only the new rows enter the free chain. Exhausted generations never
        // become free again, including across an arena relocation.
        let mut head = NO_SLOT;
        for index in (old_capacity..next_capacity).rev() {
            let row = next.get_mut(index)?;
            row.next = head;
            head = index;
        }
        let old_bytes = self.arena.bytes();
        let old = std::mem::replace(&mut self.arena, next);
        self.free_head = head;
        drop(old);
        self.funding
            .release_freed_work_bytes(old_bytes)
            .map_err(CoreError::from)?;
        Ok(())
    }
}

unsafe fn work_action(
    handle: *mut Owned<SvgWork>,
    error: *mut WireError,
    work: impl FnOnce(&mut SvgWork) -> Result<(), CoreError>,
) -> i32 {
    // SAFETY: the caller guarantees an exclusive live handle; action checks
    // independent error storage before entering the closure.
    let status = unsafe {
        action(error, || {
            let value = &mut (*handle).value;
            if value.poisoned {
                return Err(CoreError::InvalidState);
            }
            work(value)
        })
    };
    if status == STATUS_PANIC {
        // SAFETY: the exclusive canonical handle is still allocated on error.
        unsafe {
            (*handle).value.poisoned = true;
        }
    }
    status
}

/// # Safety
/// Shared live request/model/cancel, independent writable output and Core error.
/// The request must cover the complete physical worker and subsequent UI commit.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn notepads_markdown_svg_work_create(
    abi_version: u32,
    request: *const Owned<ImageRequest>,
    model: *const Owned<CoreModel>,
    cancel: *const Owned<CoreCancel>,
    output: *mut *mut Owned<SvgWork>,
    error: *mut WireError,
) -> i32 {
    handle_output!(output);
    if abi_version != SVG_WORK_ABI_VERSION {
        return STATUS_ABI_MISMATCH;
    }
    preflight!(valid_handle(request) && valid_handle(model) && valid_handle(cancel));
    // SAFETY: caller pins the immutable inputs; output is written only after all
    // fallible steps, including the exact Owned allocation, have succeeded.
    unsafe {
        let owner = &(*request).value.owner;
        let _gate = owner.call_gate.lock().unwrap_or_else(|poison| {
            owner.poisoned.store(true, Ordering::Release);
            poison.into_inner()
        });
        let code = action(error, || {
            let request = &(*request).value;
            let model = &(*model).value.model;
            let cancel = &(*cancel).value;
            if !cancel.budget.same_root(model.budget()) {
                return Err(CoreError::WrongBudget);
            }
            if model.identity().source() != request.permit.key().epoch.source {
                return Err(CoreError::WrongDocument);
            }
            if request.owner.poisoned.load(Ordering::Acquire) {
                return Err(CoreError::InvalidState);
            }
            let nonce = next_nonce()?;
            let bootstrap = request
                .permit
                .begin_svg_work(
                    model.budget(),
                    cancel.token.clone(),
                    size_of::<Owned<SvgWork>>() as u64,
                )
                .map_err(CoreError::from)?;
            // Bootstrap predates Slot: failed Slot allocation is physically freed
            // before bootstrap rolls back its policy header claim and one-shot state.
            let mut op = model.budget().begin(cancel.token.clone());
            let slot = Slot::new(op.boundary_work())?;
            let (funding, header_charge) = bootstrap.commit();
            output.write(slot.commit(SvgWork {
                arena: TicketArena::empty(),
                funding,
                header_charge: Some(header_charge),
                nonce,
                free_head: NO_SLOT,
                tickets: 0,
                pending: 0,
                poisoned: false,
                font_bytes: 0,
                renderer_phase: 0,
            }));
            Ok(())
        });
        if code == STATUS_PANIC {
            owner.poisoned.store(true, Ordering::Release);
        }
        code
    }
}

/// # Safety
/// Exclusive live account; output/error are aligned, writable and independent.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn notepads_markdown_svg_work_reserve(
    handle: *mut Owned<SvgWork>,
    kind: u32,
    bytes: u64,
    output: *mut SvgWorkToken,
    error: *mut WireError,
) -> i32 {
    preflight!(
        valid_handle(handle)
            && !output.is_null()
            && (output as usize).is_multiple_of(align_of::<SvgWorkToken>())
    );
    // SAFETY: caller supplies exclusive handle and independent success storage.
    unsafe {
        work_action(handle, error, |value| {
            let token = value.reserve(kind, bytes)?;
            output.write(token);
            Ok(())
        })
    }
}

/// # Safety
/// Exclusive live account, readable token; admit once after allocation. On error
/// the caller frees the actual allocation before returning the retained token.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn notepads_markdown_svg_work_admit(
    handle: *mut Owned<SvgWork>,
    token: *const SvgWorkToken,
    actual: u64,
    error: *mut WireError,
) -> i32 {
    preflight!(
        valid_handle(handle)
            && !token.is_null()
            && (token as usize).is_multiple_of(align_of::<SvgWorkToken>())
    );
    // SAFETY: readable full token and exclusive account have independent storage.
    unsafe { work_action(handle, error, |value| value.admit(token.read(), actual)) }
}

/// # Safety
/// The caller has actually freed the allocation, or it never allocated it.
/// Failed free preserves this token and handle. No concurrent account operation.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn notepads_markdown_svg_work_release_after_free(
    handle: *mut Owned<SvgWork>,
    token: *const SvgWorkToken,
    error: *mut WireError,
) -> i32 {
    preflight!(
        valid_handle(handle)
            && !token.is_null()
            && (token as usize).is_multiple_of(align_of::<SvgWorkToken>())
    );
    // SAFETY: physical free is the caller's obligation; token is readable.
    unsafe {
        work_action(handle, error, |value| {
            value.release_after_free(token.read())
        })
    }
}

/// # Safety
/// Exclusive live account; writable prefix/full output and error independent.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn notepads_markdown_svg_work_read(
    handle: *mut Owned<SvgWork>,
    output: *mut SvgWorkSnapshot,
    error: *mut WireError,
) -> i32 {
    preflight!(valid_handle(handle));
    if let Err(status) = unsafe { svg_header(output) } {
        return status;
    }
    // SAFETY: matching prefix authorizes full output, written only on success.
    unsafe {
        work_action(handle, error, |value| {
            output.write(value.snapshot());
            Ok(())
        })
    }
}

/// # Safety
/// Exclusive live handle and independent writable Core error storage.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn notepads_markdown_svg_work_close(
    handle: *mut Owned<SvgWork>,
    error: *mut WireError,
) -> i32 {
    preflight!(valid_handle(handle));
    // SAFETY: caller has an exclusive account borrow; close never clears tickets.
    let status = unsafe {
        action(error, || {
            (*handle).value.funding.close();
            Ok(())
        })
    };
    if status == STATUS_PANIC {
        unsafe { (*handle).value.poisoned = true };
    }
    status
}

/// # Safety
/// Null or the unique live canonical account; no calls overlap. Error keeps the
/// complete handle live. Success invalidates it after actual arena/Owned free.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn notepads_markdown_svg_work_destroy(
    handle: *mut Owned<SvgWork>,
    error: *mut WireError,
) -> i32 {
    preflight!(handle.is_null() || valid_handle(handle));
    // SAFETY: caller transfers ownership only on success; action validates error
    // storage before examining/mutating the canonical account.
    unsafe {
        action(error, || {
            if handle.is_null() {
                return Ok(());
            }
            let value = &mut (*handle).value;
            let stats = value.funding.stats();
            if !stats.closed
                || value.tickets != 0
                || value.pending != 0
                || value.funding.retained_borrow_bytes() != 0
                || stats.root_work_bytes != value.arena.bytes()
            {
                return Err(CoreError::InvalidState);
            }
            let old = std::mem::replace(&mut value.arena, TicketArena::empty());
            let bytes = old.bytes();
            drop(old);
            value
                .funding
                .release_freed_work_bytes(bytes)
                .map_err(CoreError::from)?;
            let header = value
                .header_charge
                .take()
                .ok_or(CoreError::InternalInvariant)?;
            release(handle);
            // Normal Owned release has now freed value, allocation and root header
            // funding. Only now may its policy header claim be returned.
            drop(header);
            Ok(())
        })
    }
}

#[cfg(test)]
mod tests;

impl SvgWork {
    fn reserve(&mut self, kind: u32, bytes: u64) -> Result<SvgWorkToken, CoreError> {
        if kind > 2 || bytes == 0 {
            return Err(CoreError::InvalidRange);
        }
        usize::try_from(bytes).map_err(|_| CoreError::ArithmeticOverflow)?;
        let tickets = self
            .tickets
            .checked_add(1)
            .ok_or(CoreError::ArithmeticOverflow)?;
        let pending = self
            .pending
            .checked_add(1)
            .ok_or(CoreError::ArithmeticOverflow)?;
        self.ensure_free()?;
        let slot = self.free_head;
        let row = *self.arena.get(slot)?;
        if row.state != TicketState::Free {
            return Err(CoreError::InternalInvariant);
        }
        self.funding
            .reserve_work_bytes(bytes)
            .map_err(CoreError::from)?;
        let target = self.arena.get_mut(slot)?;
        target.bytes = bytes;
        target.kind = kind;
        target.state = TicketState::Pending;
        self.free_head = row.next;
        self.tickets = tickets;
        self.pending = pending;
        Ok(SvgWorkToken {
            account_nonce: self.nonce,
            slot,
            generation: row.generation,
        })
    }
}

impl SvgWork {
    fn admit(&mut self, token: SvgWorkToken, actual: u64) -> Result<(), CoreError> {
        let row = *self.token_row(token)?;
        if row.state != TicketState::Pending || actual == 0 {
            return Err(CoreError::InvalidState);
        }
        usize::try_from(actual).map_err(|_| CoreError::ArithmeticOverflow)?;
        let pending = self
            .pending
            .checked_sub(1)
            .ok_or(CoreError::InternalInvariant)?;
        let next_font = if row.kind == 0x10001 {
            let next = self
                .font_bytes
                .checked_sub(row.bytes)
                .and_then(|v| v.checked_add(actual))
                .ok_or(CoreError::ArithmeticOverflow)?;
            Self::check_font_bytes(next)?;
            Some(next)
        } else {
            None
        };
        self.funding
            .reconcile_work_bytes(row.bytes, actual)
            .map_err(CoreError::from)?;
        let row = self.arena.get_mut(token.slot)?;
        row.bytes = actual;
        row.state = TicketState::Admitted;
        if let Some(next) = next_font {
            self.font_bytes = next;
        }
        self.pending = pending;
        Ok(())
    }
}

impl SvgWork {
    fn release_after_free(&mut self, token: SvgWorkToken) -> Result<(), CoreError> {
        let row = *self.token_row(token)?;
        if row.state == TicketState::Borrowed {
            return Err(CoreError::InvalidState);
        }
        let tickets = self
            .tickets
            .checked_sub(1)
            .ok_or(CoreError::InternalInvariant)?;
        let pending = if row.state == TicketState::Pending {
            self.pending
                .checked_sub(1)
                .ok_or(CoreError::InternalInvariant)?
        } else {
            self.pending
        };
        let next_font = if row.kind == 0x10001 {
            Some(
                self.font_bytes
                    .checked_sub(row.bytes)
                    .ok_or(CoreError::InternalInvariant)?,
            )
        } else {
            None
        };
        self.funding
            .release_freed_work_bytes(row.bytes)
            .map_err(CoreError::from)?;
        let target = self.arena.get_mut(token.slot)?;
        target.bytes = 0;
        if let Some(next) = row.generation.checked_add(1) {
            target.generation = next;
            target.state = TicketState::Free;
            target.next = self.free_head;
            self.free_head = token.slot;
        } else {
            target.state = TicketState::Retired;
            target.next = NO_SLOT;
        }
        self.tickets = tickets;
        self.pending = pending;
        if let Some(next) = next_font {
            self.font_bytes = next;
        }
        Ok(())
    }
}

impl SvgWork {
    fn check_font_bytes(bytes: u64) -> Result<(), CoreError> {
        const LIMIT: u64 = 64 * 1024 * 1024;
        if bytes > LIMIT {
            Err(CoreError::ResourceLimit {
                kind: LimitKind::AuxiliaryCapacity,
                requested: bytes,
                limit: LIMIT,
            })
        } else {
            Ok(())
        }
    }
    fn reserve_resource(&mut self, kind: u32, bytes: u64) -> Result<SvgWorkToken, CoreError> {
        if kind > 8 {
            return Err(CoreError::InvalidRange);
        }
        let next_font = if kind == 1 {
            let next = self
                .font_bytes
                .checked_add(bytes)
                .ok_or(CoreError::ArithmeticOverflow)?;
            Self::check_font_bytes(next)?;
            Some(next)
        } else {
            None
        };
        let token = self.reserve(
            match kind {
                7 => 1,
                8 => 2,
                _ => 0,
            },
            bytes,
        )?;
        if kind < 7 {
            self.arena.get_mut(token.slot)?.kind = 0x10000 | kind;
        }
        if let Some(next) = next_font {
            self.font_bytes = next;
        }
        Ok(token)
    }
}

#[cfg(test)]
fn shape_coverage() -> markdown_svg::coverage::Callbacks {
    unsafe extern "C" fn unexpected(
        _: u64,
        _: *const markdown_svg::coverage::Batch,
        _: *mut markdown_svg::coverage::Output,
    ) -> i32 {
        0x8000_4005_u32 as i32
    }

    markdown_svg::coverage::Callbacks {
        size: size_of::<markdown_svg::coverage::Callbacks>() as u32,
        version: markdown_svg::coverage::VERSION,
        session: 1,
        cover_batch: Some(unexpected),
        ..Default::default()
    }
}
