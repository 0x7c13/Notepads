// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE file in the project root for license information.
// ---------------------------------------------------------------------------------------------

//! Private whole-diagram owner ABI. No application-selected profile or budget.
//! The trusted native owner observes its real thread signaled before touching
//! computed fields, retiring storage, or returning a worker slot. An atomic
//! state alone is never proof of exit. Raw capabilities never come from input.

use super::*;
use markdown_core::{
    ir::{DerivedKind, ObjectRow},
    resources::derived::{
        ControlPolicy, DerivedError, DerivedLimit, MetadataGrant, ResidentGrant, RetainedGrant,
    },
};
use markdown_memory::MemoryError;
use markdown_mermaid::{
    DiagramError,
    canonical::CanonicalDocument,
    funding::{JobMemory, MAX_STACK_RESERVATION},
};
use merman_core::{CancelReason, OperationControl};
use std::{
    cell::UnsafeCell,
    sync::atomic::{AtomicBool, AtomicU32, AtomicUsize, Ordering},
    time::{Duration, Instant},
};

pub const DIAGRAM_ABI_VERSION: u32 = 1;
const METADATA_BYTES: u64 = 16 * 1024;
const NATIVE_COORDINATOR_BYTES: u64 = 1024;
const QUEUED: u32 = 1;
const ACTIVATED: u32 = 2;
const RUNNING: u32 = 3;
const COMPUTED: u32 = 4;
const FINALIZED: u32 = 5;
const FINISH_FAILED: u32 = 6;
const POISONED: u32 = 7;
const METADATA_ONLY: u32 = 8;
const STATUS_STACK_UNAVAILABLE: i32 = 100;
const STATUS_DIAGRAM_INVALID_INPUT: i32 = 104;
const STATUS_FONT_UNAVAILABLE: i32 = 105;

#[repr(C, align(8))]
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct DiagramSummary {
    pub header: WireHeader,
    pub identity: WireIdentity,
    pub object: u32,
    pub state: u32,
    pub width: f64,
    pub height: f64,
    pub bytes: u64,
    pub path_commands: u64,
    pub text_runs: u64,
    pub live_rust_bytes: u64,
    pub peak_requested_bytes: u64,
    pub native_charged_bytes: u64,
    pub work_units: u64,
    pub prepaid_metadata_bytes: u64,
}

#[repr(C, align(8))]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct DiagramReadResult {
    pub header: WireHeader,
    pub written: u64,
    pub next: u64,
    pub done: u32,
    pub reserved: u32,
}

#[repr(C, align(8))]
#[derive(Clone, Copy, Debug, Default)]
pub struct DiagramStyle {
    pub header: WireHeader,
    pub dark: u32,
    pub reserved: u32,
    pub foreground: [u8; 4],
    pub background: [u8; 4],
    pub border: [u8; 4],
    pub panel: [u8; 4],
    pub link: [u8; 4],
}
const _: () = {
    use std::mem::{align_of, offset_of};
    assert!(size_of::<DiagramStyle>() == 40 && align_of::<DiagramStyle>() == 8);
    assert!(size_of::<DiagramSummary>() == 136 && align_of::<DiagramSummary>() == 8);
    assert!(offset_of!(DiagramSummary, width) == 56);
    assert!(size_of::<DiagramReadResult>() == 32 && align_of::<DiagramReadResult>() == 8);
    assert!(offset_of!(DiagramReadResult, done) == 24);

    #[cfg(windows)]
    {
        let wide = size_of::<*const ()>() == 8;
        assert!(size_of::<DiagramFontCapability>() == if wide { 216 } else { 208 });
        assert!(align_of::<DiagramFontCapability>() == 8);
        assert!(offset_of!(DiagramFontCapability, font_epoch) == if wide { 32 } else { 24 });
        assert!(offset_of!(DiagramFontCapability, locale_name) == if wide { 40 } else { 32 });
    }
};

#[cfg(windows)]
pub type DiagramFontCapability = markdown_diagram_platform::FontCapability;
#[cfg(not(windows))]
#[repr(C)]
#[derive(Clone, Copy)]
pub struct DiagramFontCapability {
    pub version: u32,
    pub size: u32,
    pub factory: *mut core::ffi::c_void,
    pub collection: *mut core::ffi::c_void,
    pub fallback: *mut core::ffi::c_void,
    pub font_epoch: u64,
    pub locale_name: [u16; 85],
}

struct Data {
    artifact: Option<CanonicalDocument>,
    model: Option<AcceptedModel>,
    identity: WireIdentity,
    object: u32,
    policy: ControlPolicy,
    first: Option<DiagramError>,
    memory: Option<Arc<JobMemory>>,
    retained: Option<RetainedGrant>,
    // Original queued funding and resident survive every state. free_context
    // moves this whole value out and truly deallocates the enclosing record
    // before dropping its grants. Native intrusive refs share one FFI ref.
    _queued: MetadataGrant,
    _resident: ResidentGrant,
}

pub struct DiagramJob {
    references: AtomicUsize,
    state: AtomicU32,
    start_claimed: AtomicBool,
    // Never scoped/regioned. Concurrent cancel borrows only this immutable
    // sibling; the worker exclusively borrows UnsafeCell<Data>.
    control: OperationControl,
    data: UnsafeCell<Data>,
}

/// One independently funded native control header. It deliberately has no job
/// or resident permit and does not pin the accepted source buffer/model.
pub struct DiagramCoordinatorFunding {
    _metadata: MetadataGrant,
}

const _: () = assert!(size_of::<DiagramJob>() + 4096 + 4096 < METADATA_BYTES as usize);

fn valid<T>(pointer: *const T) -> bool {
    !pointer.is_null() && (pointer as usize).is_multiple_of(align_of::<T>())
}
fn invalid() -> DiagramError {
    DiagramError::InvalidFunding
}
fn diagram_header<T>() -> WireHeader {
    WireHeader {
        abi_version: DIAGRAM_ABI_VERSION,
        struct_size: size_of::<T>() as u32,
    }
}
unsafe fn check_header<T>(pointer: *const T) -> Result<(), i32> {
    if !valid(pointer) {
        return Err(STATUS_INVALID_ARGUMENT);
    }
    // SAFETY: caller supplies a readable initialized prefix.
    if unsafe { pointer.cast::<WireHeader>().read() } != diagram_header::<T>() {
        return Err(STATUS_ABI_MISMATCH);
    }
    Ok(())
}
fn fault(error: DiagramError) -> WireError {
    let mut out = WireError::default();
    out.status = match error {
        DiagramError::Funding(DerivedError::Source(error)) => return wire_error(error),
        DiagramError::Funding(DerivedError::Limit {
            kind,
            requested,
            limit,
        }) => {
            out.requested = requested;
            out.limit = limit;
            out.limit_kind = 100
                + match kind {
                    DerivedLimit::ProcessBytes => 1,
                    DerivedLimit::ControlBytes => 2,
                    DerivedLimit::ProcessJobs => 3,
                    DerivedLimit::ControlJobs => 4,
                    DerivedLimit::RetainedBytes => 5,
                    DerivedLimit::MetadataBytes => 6,
                    DerivedLimit::ProcessResidents => 7,
                    DerivedLimit::ControlResidents => 8,
                };
            9
        }
        DiagramError::Funding(DerivedError::ArithmeticOverflow) => 20,
        DiagramError::Funding(_) | DiagramError::InvalidFunding => 102,
        DiagramError::Memory(MemoryError::BudgetLimited | MemoryError::WorkLimited)
        | DiagramError::WorkLimited => 9,
        DiagramError::Memory(MemoryError::OutOfMemory) => 7,
        DiagramError::Memory(MemoryError::ArithmeticOverflow) => 20,
        DiagramError::Memory(MemoryError::Cancelled) | DiagramError::Cancelled => 4,
        DiagramError::Memory(_) => 102,
        DiagramError::InvalidInput => STATUS_DIAGRAM_INVALID_INPUT,
        DiagramError::Unsupported => 8,
        DiagramError::Deadline => 24,
        DiagramError::InvalidGeometry => 101,
        DiagramError::FontUnavailable => STATUS_FONT_UNAVAILABLE,
        DiagramError::Backend => 22,
        DiagramError::StackUnavailable => STATUS_STACK_UNAVAILABLE,
    };
    out
}
unsafe fn action_diagram(
    error: *mut WireError,
    body: impl FnOnce() -> Result<(), DiagramError>,
) -> i32 {
    // SAFETY: caller supplies initialized Core ABI3 error prefix.
    if let Err(code) = unsafe { header(error) } {
        return code;
    }
    let mut diagnostic = WireError::default();
    let code = contain_panic(|| match body() {
        Ok(()) => 0,
        Err(error) => {
            diagnostic = fault(error);
            diagnostic.status
        }
    });
    diagnostic.status = code;
    // SAFETY: full output storage follows matching prefix; no output aliasing.
    unsafe { error.write(diagnostic) };
    code
}
fn checkpoint(control: &OperationControl) -> Result<(), DiagramError> {
    control.checkpoint().map_err(|error| match error.reason {
        CancelReason::DeadlineExceeded => DiagramError::Deadline,
        CancelReason::Requested => DiagramError::Cancelled,
    })
}
fn input(model: &AcceptedModel, object: u32) -> Result<&str, DiagramError> {
    let index = object.checked_sub(1).ok_or(DiagramError::InvalidInput)? as usize;
    let Some(ObjectRow::Derived {
        kind: DerivedKind::Diagram,
        input,
        ..
    }) = model.ir().objects().get(index)
    else {
        return Err(DiagramError::Unsupported);
    };
    let payload = model
        .ir()
        .payloads()
        .get(input.index())
        .ok_or(DiagramError::InvalidInput)?;
    model
        .ir()
        .text(payload.value)
        .map_err(|_| DiagramError::InvalidInput)
}
unsafe fn poison(handle: *mut DiagramJob, code: i32) -> i32 {
    if code == STATUS_PANIC {
        // SAFETY: caller retains live context, no mutable data borrow needed.
        unsafe { (*ptr::addr_of!((*handle).state)).store(POISONED, Ordering::Release) };
    }
    code
}

#[unsafe(no_mangle)]
pub extern "C" fn notepads_markdown_diagram_abi_version() -> u32 {
    DIAGRAM_ABI_VERSION
}

/// # Safety
/// Live shared control/model; independent writable handle/error. Native caller
/// allocates at most the fixed 1024-byte header only after success.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn notepads_markdown_diagram_coordinator_funding_prepare(
    control: *mut MathControl,
    model: *const Owned<CoreModel>,
    output: *mut *mut DiagramCoordinatorFunding,
    error: *mut WireError,
) -> i32 {
    if !valid(output) {
        return STATUS_INVALID_ARGUMENT;
    }
    unsafe { output.write(ptr::null_mut()) };
    preflight!(valid(control) && valid_handle(model));
    unsafe {
        action_diagram(error, || {
            let grant = (*control)
                .derived_policy()
                .reserve_metadata(
                    &(*model).value.model,
                    size_of::<DiagramCoordinatorFunding>() as u64 + NATIVE_COORDINATOR_BYTES,
                )
                .map_err(DiagramError::Funding)?;
            let pointer = alloc(Layout::new::<DiagramCoordinatorFunding>())
                .cast::<DiagramCoordinatorFunding>();
            if pointer.is_null() {
                return Err(DiagramError::Memory(MemoryError::OutOfMemory));
            }
            pointer.write(DiagramCoordinatorFunding { _metadata: grant });
            output.write(pointer);
            Ok(())
        })
    }
}

/// # Safety
/// Sole funding handle, after true native coordinator header free. A native
/// HeapFree failure must keep this owner and its module pin, never release it.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn notepads_markdown_diagram_coordinator_funding_release(
    funding: *mut DiagramCoordinatorFunding,
    error: *mut WireError,
) -> i32 {
    preflight!(funding.is_null() || valid(funding));
    unsafe {
        action_diagram(error, || {
            if funding.is_null() {
                return Ok(());
            }
            let value = funding.read();
            dealloc(funding.cast(), Layout::new::<DiagramCoordinatorFunding>());
            drop(value);
            Ok(())
        })
    }
}

/// # Safety
/// Live immutable shared Core model/control, writable independent output/error.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn notepads_markdown_diagram_job_prepare(
    control: *mut MathControl,
    model: *const Owned<CoreModel>,
    object: u32,
    output: *mut *mut DiagramJob,
    error: *mut WireError,
) -> i32 {
    if !valid(output) {
        return STATUS_INVALID_ARGUMENT;
    }
    // SAFETY: caller provides writable independent handle storage.
    unsafe { output.write(ptr::null_mut()) };
    preflight!(valid(control) && valid_handle(model));
    // SAFETY: immutable input owners, new allocation initialized once.
    unsafe {
        action_diagram(error, || {
            let deadline = Instant::now()
                .checked_add(Duration::from_secs(5))
                .ok_or(DiagramError::InvalidFunding)?;
            let model = &(*model).value.model;
            let source = input(model, object)?;
            if source.len() > 16 * 1024 {
                return Err(DiagramError::WorkLimited);
            }
            let policy = (*control).derived_policy();
            let resident = policy.reserve_resident().map_err(DiagramError::Funding)?;
            let queued = policy
                .reserve_metadata(model, METADATA_BYTES)
                .map_err(DiagramError::Funding)?;
            // Fixed control/clock initialization is prepaid, before Arc creation.
            // Queue time consumes this one deadline; activate/run never restart it.
            let base = OperationControl::new().with_absolute_deadline(deadline);
            checkpoint(&base)?;
            let value = DiagramJob {
                references: AtomicUsize::new(1),
                state: AtomicU32::new(QUEUED),
                start_claimed: AtomicBool::new(false),
                control: base,
                data: UnsafeCell::new(Data {
                    artifact: None,
                    model: Some(model.clone()),
                    identity: wire_identity(model.identity()),
                    object,
                    policy,
                    first: None,
                    memory: None,
                    retained: None,
                    _queued: queued,
                    _resident: resident,
                }),
            };
            let pointer = alloc(Layout::new::<DiagramJob>()).cast::<DiagramJob>();
            if pointer.is_null() {
                return Err(DiagramError::Memory(MemoryError::OutOfMemory));
            }
            pointer.write(value);
            output.write(pointer);
            Ok(())
        })
    }
}

/// # Safety
/// Exclusive queued context; cancel/state/ref operations may overlap.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn notepads_markdown_diagram_job_activate(
    handle: *mut DiagramJob,
    error: *mut WireError,
) -> i32 {
    preflight!(valid(handle));
    // SAFETY: atomic/control siblings disjoint from mutable job data.
    let code = unsafe {
        action_diagram(error, || {
            if (*ptr::addr_of!((*handle).state)).load(Ordering::Acquire) != QUEUED {
                return Err(invalid());
            }
            let data = &mut *(*ptr::addr_of!((*handle).data)).get();
            checkpoint(&*ptr::addr_of!((*handle).control))?;
            let memory = JobMemory::activate(
                data.model.as_ref().ok_or_else(invalid)?,
                &data.policy,
                METADATA_BYTES,
                MAX_STACK_RESERVATION,
                (*ptr::addr_of!((*handle).control)).clone(),
            )?;
            data.memory = Some(memory);
            (*ptr::addr_of!((*handle).state)).store(ACTIVATED, Ordering::Release);
            Ok(())
        })
    };
    unsafe { poison(handle, code) }
}

/// # Safety
/// Live queued/activated context; one-shot claim precedes native allocation.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn notepads_markdown_diagram_job_claim_start(
    handle: *mut DiagramJob,
    error: *mut WireError,
) -> i32 {
    preflight!(valid(handle));
    unsafe {
        action_diagram(error, || {
            if !matches!(
                (*ptr::addr_of!((*handle).state)).load(Ordering::Acquire),
                QUEUED | ACTIVATED
            ) {
                return Err(invalid());
            }
            (*ptr::addr_of!((*handle).start_claimed))
                .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
                .map_err(|_| invalid())?;
            Ok(())
        })
    }
}

/// Private native flowchart admission for the three audited MSVC targets.
/// The 6 MiB engineering profile includes a named aggregate 4 MiB allowance
/// for the closed Windows/runtime chain, not an observed OS stack maximum.
/// Native code must still measure committed RW, non-guard headroom in this
/// ordinary activation. No caller can provide a profile or bypass that gate.
/// Keep the barrier so final compiler-artifact deltas retain the compute path.
#[inline(never)]
fn native_required_stack() -> Option<u64> {
    let required = std::hint::black_box(6 * 1024 * 1024u64);
    (cfg!(all(
        target_os = "windows",
        target_env = "msvc",
        any(
            target_arch = "x86",
            target_arch = "x86_64",
            target_arch = "aarch64"
        )
    )) && required > 0)
        .then_some(required)
}

/// # Safety
/// One trusted ordinary worker activation owns this call exclusively. Only
/// cancel/state/retain overlap it. Original native COM objects and input PODs
/// stay live throughout; no Markdown/application callback supplies capability
/// pointers. Native owner measures committed RW non-guard headroom in this
/// same activation and keeps context/node/module until the real handle signals.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn notepads_markdown_diagram_job_run(
    handle: *mut DiagramJob,
    fonts: *const DiagramFontCapability,
    style: *const DiagramStyle,
    measured_headroom: u64,
    error: *mut WireError,
) -> i32 {
    preflight!(valid(handle) && valid(fonts));
    if let Err(code) = unsafe { check_header(style) } {
        return code;
    }
    let mut entered = false;
    let code = unsafe {
        action_diagram(error, || {
            if !(*ptr::addr_of!((*handle).start_claimed)).load(Ordering::Acquire) {
                return Err(invalid());
            }
            (*ptr::addr_of!((*handle).state))
                .compare_exchange(ACTIVATED, RUNNING, Ordering::AcqRel, Ordering::Acquire)
                .map_err(|_| invalid())?;
            entered = true;
            let data = &mut *(*ptr::addr_of!((*handle).data)).get();
            let control = &*ptr::addr_of!((*handle).control);
            let result = (|| {
                checkpoint(control)?;
                let required = native_required_stack().ok_or(DiagramError::StackUnavailable)?;
                if measured_headroom < required || measured_headroom > MAX_STACK_RESERVATION {
                    return Err(DiagramError::StackUnavailable);
                }
                let style = style.read();
                if style.dark > 1 || style.reserved != 0 {
                    return Err(DiagramError::InvalidInput);
                }
                compute_windows(data, fonts.read(), style, control)
            })()
            .map_err(|error| {
                data.memory
                    .as_ref()
                    .map_or(error, |memory| memory.resolve_error(error))
            });
            if let Err(error) = result {
                data.first.get_or_insert(error);
            }
            result
        })
    };
    if entered {
        unsafe {
            (*ptr::addr_of!((*handle).state)).store(
                if code == STATUS_PANIC {
                    POISONED
                } else {
                    COMPUTED
                },
                Ordering::Release,
            )
        };
    }
    code
}

#[cfg(windows)]
#[inline(never)]
unsafe fn compute_windows(
    data: &mut Data,
    fonts: DiagramFontCapability,
    style: DiagramStyle,
    control: &OperationControl,
) -> Result<(), DiagramError> {
    use markdown_diagram_platform::{NativeTheme, PlatformError, WindowsText, render_native};
    let memory = data.memory.as_ref().ok_or_else(invalid)?;
    let scope = memory.scope();
    let scoped = control.with_memory_scope(scope.clone());
    // SAFETY: the private native worker retains the captured free-threaded
    // factory/collection/fallback through real exit; QI takes our strong refs.
    let text = unsafe { WindowsText::from_capability(fonts, scope.clone(), scoped.clone()) }
        .map_err(|error| match error {
            PlatformError::Memory(error) => DiagramError::Memory(error),
            PlatformError::Cancelled => DiagramError::Cancelled,
            PlatformError::DirectWrite(code) if code == 0x8007_000Eu32 as i32 => {
                DiagramError::Memory(MemoryError::OutOfMemory)
            }
            _ => DiagramError::FontUnavailable,
        })?;
    let artifact = render_native(
        input(data.model.as_ref().ok_or_else(invalid)?, data.object)?,
        &scope,
        &scoped,
        &text,
        NativeTheme {
            dark: style.dark != 0,
            foreground: style.foreground,
            background: style.background,
            border: style.border,
            panel: style.panel,
            link: style.link,
        },
    )?;
    checkpoint(control)?;
    data.artifact = Some(artifact);
    // text/scoped/region producers really release before the ordinary thread
    // returns. Late COM receipts, if any, remain visible to the same JobMemory.
    Ok(())
}
#[cfg(not(windows))]
unsafe fn compute_windows(
    _: &mut Data,
    _: DiagramFontCapability,
    _: DiagramStyle,
    _: &OperationControl,
) -> Result<(), DiagramError> {
    Err(DiagramError::StackUnavailable)
}

/// # Safety
/// Live shared context. Only the immutable control sibling is touched.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn notepads_markdown_diagram_job_cancel(
    handle: *mut DiagramJob,
    error: *mut WireError,
) -> i32 {
    preflight!(valid(handle));
    unsafe {
        action_diagram(error, || {
            (*ptr::addr_of!((*handle).control)).cancel();
            Ok(())
        })
    }
}
/// # Safety
/// Live shared context.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn notepads_markdown_diagram_job_state(handle: *const DiagramJob) -> u32 {
    if !valid(handle) {
        return 0;
    }
    unsafe { (*ptr::addr_of!((*handle).state)).load(Ordering::Acquire) }
}
/// # Safety
/// Caller already owns a live reference. No resurrection from final free.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn notepads_markdown_diagram_job_retain(
    handle: *mut DiagramJob,
    error: *mut WireError,
) -> i32 {
    preflight!(valid(handle));
    unsafe {
        action_diagram(error, || {
            (*ptr::addr_of!((*handle).references))
                .try_update(Ordering::Relaxed, Ordering::Relaxed, |count| {
                    count
                        .checked_add(1)
                        .filter(|next| count > 0 && *next <= isize::MAX as usize)
                })
                .map_err(|_| invalid())?;
            Ok(())
        })
    }
}
/// # Safety
/// Caller transfers a reference; final owner proves no thread or signaled real
/// thread and exclusive access. Late COM owners refuse final free and keep all
/// funding plus the native node/module pin; caller must not discard that owner.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn notepads_markdown_diagram_job_release(
    handle: *mut DiagramJob,
    error: *mut WireError,
) -> i32 {
    preflight!(handle.is_null() || valid(handle));
    unsafe {
        action_diagram(error, || {
            if handle.is_null() {
                return Ok(());
            }
            let references = &*ptr::addr_of!((*handle).references);
            let mut count = references.load(Ordering::Acquire);
            loop {
                if count == 0 {
                    return Err(invalid());
                }
                if count == 1 {
                    if matches!(
                        (*ptr::addr_of!((*handle).state)).load(Ordering::Acquire),
                        RUNNING | POISONED
                    ) {
                        return Err(invalid());
                    }
                    let data = &mut *(*ptr::addr_of!((*handle).data)).get();
                    drop(data.artifact.take());
                    if data.memory.as_ref().is_some_and(|memory| {
                        memory.usage().live_rust_bytes != 0 || Arc::strong_count(memory) != 1
                    }) {
                        return Err(invalid());
                    }
                }
                match references.compare_exchange_weak(
                    count,
                    count - 1,
                    Ordering::AcqRel,
                    Ordering::Acquire,
                ) {
                    Ok(_) => break,
                    Err(next) => count = next,
                }
            }
            if count == 1 {
                let value = handle.read();
                dealloc(handle.cast(), Layout::new::<DiagramJob>());
                drop(value);
            }
            Ok(())
        })
    }
}

/// # Safety
/// Exclusive context; no thread created or actual handle signaled; all native
/// result/weak owners gone. Original one FFI ref only, as in Math retirement.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn notepads_markdown_diagram_job_retire_metadata(
    handle: *mut DiagramJob,
    error: *mut WireError,
) -> i32 {
    preflight!(valid(handle));
    let code = unsafe {
        action_diagram(error, || {
            if !matches!(
                (*ptr::addr_of!((*handle).state)).load(Ordering::Acquire),
                QUEUED | ACTIVATED | COMPUTED | FINISH_FAILED
            ) || (*ptr::addr_of!((*handle).references)).load(Ordering::Acquire) != 1
            {
                return Err(invalid());
            }
            let data = &mut *(*ptr::addr_of!((*handle).data)).get();
            if data.retained.is_some() {
                return Err(invalid());
            }
            drop(data.artifact.take());
            if let Some(memory) = &data.memory {
                if Arc::strong_count(memory) != 1 {
                    return Err(invalid());
                }
                memory.retire_after_thread_exit()?;
            }
            drop(data.memory.take());
            drop(data.model.take());
            (*ptr::addr_of!((*handle).state)).store(METADATA_ONLY, Ordering::Release);
            Ok(())
        })
    };
    unsafe { poison(handle, code) }
}

unsafe fn computed<'a>(handle: *mut DiagramJob) -> Result<&'a mut Data, DiagramError> {
    // SAFETY: the private ABI caller has observed the actual thread signaled
    // and supplies exclusive post-exit access. The state is an extra check.
    unsafe {
        if (*ptr::addr_of!((*handle).state)).load(Ordering::Acquire) != COMPUTED {
            return Err(invalid());
        }
        let data = &mut *(*ptr::addr_of!((*handle).data)).get();
        if let Some(error) = data.first {
            return Err(error);
        }
        Ok(data)
    }
}

/// # Safety
/// Real thread signaled; exclusive context, writable independent ABI1 summary.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn notepads_markdown_diagram_job_summary(
    handle: *mut DiagramJob,
    output: *mut DiagramSummary,
    error: *mut WireError,
) -> i32 {
    preflight!(valid(handle));
    if let Err(code) = unsafe { check_header(output) } {
        return code;
    }
    let code = unsafe {
        action_diagram(error, || {
            let data = computed(handle)?;
            checkpoint(&*ptr::addr_of!((*handle).control))?;
            let artifact = data.artifact.as_ref().ok_or_else(invalid)?;
            let usage = data.memory.as_ref().ok_or_else(invalid)?.usage();
            output.write(DiagramSummary {
                header: diagram_header::<DiagramSummary>(),
                identity: data.identity,
                object: data.object,
                state: COMPUTED,
                width: artifact.width,
                height: artifact.height,
                bytes: artifact.as_bytes().len() as u64,
                path_commands: artifact.path_commands as u64,
                text_runs: artifact.text_runs as u64,
                live_rust_bytes: usage.live_rust_bytes,
                peak_requested_bytes: usage.peak_requested_bytes,
                native_charged_bytes: usage.native_charged_bytes,
                work_units: usage.work_units,
                prepaid_metadata_bytes: METADATA_BYTES,
            });
            Ok(())
        })
    };
    unsafe { poison(handle, code) }
}

/// # Safety
/// Real thread signaled; exclusive context, distinct writable byte buffer and
/// ABI1 result. Native allocation of buffer precedes this read and is prepaid
/// by charge_native. Reads copy a bounded raw UTF8 byte segment, not text units.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn notepads_markdown_diagram_job_read(
    handle: *mut DiagramJob,
    start: u64,
    buffer: *mut u8,
    capacity: u64,
    output: *mut DiagramReadResult,
    error: *mut WireError,
) -> i32 {
    preflight!(valid(handle) && !buffer.is_null() && (1..=65_536).contains(&capacity));
    if let Err(code) = unsafe { check_header(output) } {
        return code;
    }
    let code = unsafe {
        action_diagram(error, || {
            let data = computed(handle)?;
            checkpoint(&*ptr::addr_of!((*handle).control))?;
            let bytes = data.artifact.as_ref().ok_or_else(invalid)?.as_bytes();
            let start = usize::try_from(start).map_err(|_| DiagramError::InvalidInput)?;
            if start > bytes.len() {
                return Err(DiagramError::InvalidInput);
            }
            let count = (bytes.len() - start).min(capacity as usize);
            ptr::copy_nonoverlapping(bytes.as_ptr().add(start), buffer, count);
            output.write(DiagramReadResult {
                header: diagram_header::<DiagramReadResult>(),
                written: count as u64,
                next: (start + count) as u64,
                done: u32::from(start + count == bytes.len()),
                reserved: 0,
            });
            Ok(())
        })
    };
    unsafe { poison(handle, code) }
}

/// # Safety
/// Actual thread signaled; unique computed context. Call before native alloc,
/// including failed attempts and complete old/new copies, never after realloc.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn notepads_markdown_diagram_job_charge_native(
    handle: *mut DiagramJob,
    bytes: u64,
    error: *mut WireError,
) -> i32 {
    preflight!(valid(handle));
    let code = unsafe {
        action_diagram(error, || {
            computed(handle)?
                .memory
                .as_ref()
                .ok_or_else(invalid)?
                .charge_native(bytes)
        })
    };
    unsafe { poison(handle, code) }
}

/// # Safety
/// Real thread handle signaled. Caller reports all still-live native requested
/// bytes plus the full prepaid metadata, after freeing temporary/copy owners.
/// Rust candidate storage is really freed here before retaining the native
/// result grant. Late callback owners refuse shrink rather than returning it.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn notepads_markdown_diagram_job_finalize_native(
    handle: *mut DiagramJob,
    actual_native_bytes: u64,
    error: *mut WireError,
) -> i32 {
    preflight!(valid(handle));
    let mut attempted = false;
    let code = unsafe {
        action_diagram(error, || {
            let data = computed(handle)?;
            attempted = true;
            drop(data.artifact.take());
            if data
                .memory
                .as_ref()
                .is_none_or(|memory| Arc::strong_count(memory) != 1)
            {
                return Err(invalid());
            }
            let retained = data
                .memory
                .as_ref()
                .ok_or_else(invalid)?
                .finalize_after_thread_exit(actual_native_bytes)?;
            data.retained = Some(retained);
            drop(data.memory.take());
            drop(data.model.take());
            Ok(())
        })
    };
    if attempted {
        unsafe {
            (*ptr::addr_of!((*handle).state)).store(
                if code == 0 {
                    FINALIZED
                } else if code == STATUS_PANIC {
                    POISONED
                } else {
                    FINISH_FAILED
                },
                Ordering::Release,
            )
        };
    }
    code
}

#[cfg(test)]
mod tests;
