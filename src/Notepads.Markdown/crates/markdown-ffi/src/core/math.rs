// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE file in the project root for license information.
// ---------------------------------------------------------------------------------------------

//! Private math owner ABI. The Windows reaper's real handle-signaled wait is a
//! caller safety obligation; an atomic Computed flag alone never proves exit.

use super::*;
use markdown_core::ir::{DerivedKind, ObjectRow};
use markdown_core::resources::derived::{
    ControlPolicy, DerivedError, DerivedLimit, MAX_METADATA_BYTES, MetadataGrant,
    POLICY_WRAPPER_BYTES, ProcessPolicy, ResidentGrant, RetainedGrant, Usage,
};
use markdown_math::chinese_text::{
    ChineseTextCommand, ChineseTextFamily, ChineseTextMetrics, ChineseTextOutline,
    ChineseTextShaper, ChineseTextStyle,
};
use markdown_math::fonts::FontAssets;
use markdown_math::pending::{DeferredMathJob, ExecutionError, StorageError};
use markdown_math::{
    MathError, MathLimits, MathPathCommand, StorageFamily, WorkerStackContract,
};
use std::cell::UnsafeCell;
use std::sync::atomic::{AtomicBool, AtomicU32, AtomicUsize, Ordering};
use std::time::{Duration, Instant};

pub const MATH_ABI_VERSION: u32 = 2;
const NATIVE_POLICY_BYTES: u64 = 256;
const NATIVE_JOB_BYTES: u64 = 4096;

// Stack audit provenance: math-stack-native-delta-v1 and the V3 physical
// AST/layout/TTF lemmas cover the MSVC product call graph and its stack frames.
// Rust <=1.25MiB, trusted Windows/std/CRT U=128KiB and
// native H=64KiB, plus a separate 64KiB margin: total admission is 1.5MiB.
// The native entry supplies only its measured contiguous committed RW lower
// bound in that same activation, after subtracting 16KiB entry slack. Neither
// an application nor a parser callback can select a profile or change its cap.
// Any change to the parser/layout/font/call graph or compiler invalidates this
// source/build qualification and requires a finite audit before package use.
fn native_stack_contract(measured_headroom: u64) -> WorkerStackContract {
    let supported = cfg!(all(
        target_os = "windows",
        target_env = "msvc",
        any(
            target_arch = "x86",
            target_arch = "x86_64",
            target_arch = "aarch64"
        )
    ));
    WorkerStackContract {
        usable_headroom_bytes: measured_headroom,
        platform_margin_bytes: 65_536,
        audited_required_bytes: supported.then_some(1_507_328),
    }
}
const QUEUED: u32 = 1;
const ACTIVATED: u32 = 2;
const RUNNING: u32 = 3;
const COMPUTED: u32 = 4;
const FINALIZED: u32 = 5;
const FINISH_FAILED: u32 = 6;
const POISONED: u32 = 7;
const METADATA_ONLY: u32 = 8;

#[repr(C, align(8))]
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct MathUsageRow {
    pub bytes: u64,
    pub retained_bytes: u64,
    pub metadata_bytes: u64,
    pub jobs: u32,
    pub residents: u32,
}
#[repr(C, align(8))]
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct MathUsage {
    pub header: WireHeader,
    pub process: MathUsageRow,
    pub control: MathUsageRow,
}
#[repr(C, align(8))]
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct MathMetrics {
    pub advance: f64,
    pub ascent: f64,
    pub descent: f64,
    pub left: f64,
    pub top: f64,
    pub right: f64,
    pub bottom: f64,
}
#[repr(C, align(8))]
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct MathSummary {
    pub header: WireHeader,
    pub identity: WireIdentity,
    pub object: u32,
    pub state: u32,
    pub metrics: MathMetrics,
    pub commands: u64,
    pub primitives: u64,
    pub prepaid_metadata_bytes: u64,
    pub computed_rust_bytes: u64,
    pub reserved_bytes: u64,
    pub cumulative_bytes: [u64; 11],
    pub cumulative_slots: [u64; 11],
}
#[repr(C, align(8))]
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct MathCommand {
    pub kind: u32,
    pub reserved: u32,
    pub x1: f64,
    pub y1: f64,
    pub x2: f64,
    pub y2: f64,
    pub x3: f64,
    pub y3: f64,
}
#[repr(C, align(8))]
#[derive(Default)]
pub struct MathTextMetrics {
    advance: f64,
    ascent: f64,
    descent: f64,
}

pub type MathTextWrite = unsafe extern "C" fn(*mut std::ffi::c_void, *const MathCommand) -> i32;
pub type MathTextShape = unsafe extern "C" fn(
    *const u16,
    u32,
    u32,
    *mut std::ffi::c_void,
    MathTextWrite,
    *mut MathTextMetrics,
) -> i32;

struct NativeChineseTextShaper(MathTextShape);

struct MathTextSink<'o, 's, 'a> {
    outline: &'o mut ChineseTextOutline<'s, 'a>,
    error: Option<MathError>,
}

unsafe extern "C" fn write_math_text(
    context: *mut std::ffi::c_void,
    command: *const MathCommand,
) -> i32 {
    if context.is_null() {
        return STATUS_INVALID_ARGUMENT;
    }
    // SAFETY: the synchronous native provider invokes this only with the
    // exclusive stack context supplied for this shape call.
    let sink = unsafe { &mut *context.cast::<MathTextSink<'_, '_, '_>>() };
    if let Some(error) = sink.error {
        return fault(error).status;
    }
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        if command.is_null() {
            return sink.outline.checkpoint();
        }
        if !valid(command) {
            return Err(MathError::InvalidGeometry);
        }
        // SAFETY: the provider owns this aligned initialized row through return.
        let row = unsafe { &*command };
        if row.reserved != 0 {
            return Err(MathError::InvalidGeometry);
        }
        let command = match row.kind {
            1 => ChineseTextCommand::MoveTo {
                x: row.x1,
                y: row.y1,
            },
            2 => ChineseTextCommand::LineTo {
                x: row.x1,
                y: row.y1,
            },
            3 => ChineseTextCommand::QuadTo {
                x1: row.x1,
                y1: row.y1,
                x: row.x2,
                y: row.y2,
            },
            4 => ChineseTextCommand::CubicTo {
                x1: row.x1,
                y1: row.y1,
                x2: row.x2,
                y2: row.y2,
                x: row.x3,
                y: row.y3,
            },
            5 => ChineseTextCommand::Close,
            _ => return Err(MathError::InvalidGeometry),
        };
        sink.outline.push(command)
    }));
    match result.unwrap_or(Err(MathError::InternalInvariant)) {
        Ok(()) => 0,
        Err(error) => {
            sink.error = Some(error);
            fault(error).status
        }
    }
}

impl ChineseTextShaper for NativeChineseTextShaper {
    fn shape(
        &mut self,
        utf16: &[u16],
        style: ChineseTextStyle,
        outline: &mut ChineseTextOutline<'_, '_>,
    ) -> Result<Option<ChineseTextMetrics>, MathError> {
        let mut flags = match style.family {
            ChineseTextFamily::Serif => 0,
            ChineseTextFamily::SansSerif => 1,
            ChineseTextFamily::Monospace => 2,
        };
        flags |= u32::from(style.bold) << 8;
        flags |= u32::from(style.italic) << 9;
        let mut sink = MathTextSink {
            outline,
            error: None,
        };
        let mut metrics = MathTextMetrics::default();
        let length = u32::try_from(utf16.len()).map_err(|_| MathError::InvalidLimits)?;
        // SAFETY: this is the worker's private synchronous provider. The
        // input, metrics and exclusive sink live until it returns.
        let status = unsafe {
            (self.0)(
                utf16.as_ptr(),
                length,
                flags,
                (&mut sink as *mut MathTextSink<'_, '_, '_>).cast(),
                write_math_text,
                &mut metrics,
            )
        };
        if let Some(error) = sink.error {
            return Err(error);
        }
        match status {
            0 => Ok(Some(ChineseTextMetrics {
                width: metrics.advance,
                height: metrics.ascent,
                depth: metrics.descent,
            })),
            105 => Ok(None),
            _ => Err(MathError::InvalidGeometry),
        }
    }
}

#[repr(C, align(8))]
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct MathPrimitive {
    pub command_start: u32,
    pub command_count: u32,
    pub flags: u32,
    pub reserved: u32,
    pub r: f32,
    pub g: f32,
    pub b: f32,
    pub a: f32,
    pub stroke_width: f64,
}
#[repr(C, align(8))]
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct MathReadResult {
    pub header: WireHeader,
    pub written: u64,
    pub next: u64,
    pub done: u32,
    pub reserved: u32,
}

pub struct MathRuntime {
    references: AtomicUsize,
    metadata: SourceBudget,
    policy: ProcessPolicy,
}
pub struct MathControl {
    references: AtomicUsize,
    process: ProcessPolicy,
    policy: ControlPolicy,
}
impl MathControl {
    /// Private sibling engines share this exact authority. The borrowing FFI
    /// caller keeps the control alive; this clone creates no new quota pool.
    pub(super) fn derived_policy(&self) -> ControlPolicy {
        self.policy.clone()
    }
}
struct JobData {
    model: Option<AcceptedModel>,
    policy: ControlPolicy,
    identity: WireIdentity,
    object: u32,
    deadline: Instant,
    queued: Option<MetadataGrant>,
    job: Option<DeferredMathJob>,
    retained: Option<RetainedGrant>,
    // Field order matters: all payload/funding fields drop before this final
    // permit, and free() deallocates the context itself before dropping data.
    _resident: ResidentGrant,
}
pub struct MathJob {
    references: AtomicUsize,
    state: AtomicU32,
    cancelled: AtomicBool,
    start_claimed: AtomicBool,
    data: UnsafeCell<JobData>,
}

const _: () = {
    assert!(size_of::<MathUsageRow>() == 32);
    assert!(size_of::<MathUsage>() == 72);
    assert!(size_of::<MathMetrics>() == 56);
    assert!(size_of::<MathSummary>() == 328);
    assert!(size_of::<MathCommand>() == 56);
    assert!(size_of::<MathTextMetrics>() == 24);
    assert!(size_of::<MathPrimitive>() == 40);
    assert!(size_of::<MathReadResult>() == 32);
    assert!(
        size_of::<MathRuntime>() + align_of::<MathRuntime>() + NATIVE_POLICY_BYTES as usize
            <= POLICY_WRAPPER_BYTES as usize
    );
    assert!(
        size_of::<MathControl>() + align_of::<MathControl>() + NATIVE_POLICY_BYTES as usize
            <= POLICY_WRAPPER_BYTES as usize
    );
    assert!(size_of::<MathJob>() >= size_of::<DeferredMathJob>());
};

fn invalid_state() -> MathError {
    MathError::Funding(DerivedError::InvalidState)
}
fn core_fault(error: CoreError) -> MathError {
    MathError::Funding(DerivedError::Source(error))
}
fn math_header_value<T>() -> WireHeader {
    WireHeader {
        abi_version: MATH_ABI_VERSION,
        struct_size: size_of::<T>() as u32,
    }
}
fn valid<T>(pointer: *const T) -> bool {
    !pointer.is_null() && (pointer as usize).is_multiple_of(align_of::<T>())
}
unsafe fn math_header<T>(pointer: *const T) -> Result<(), i32> {
    if !valid(pointer) {
        return Err(STATUS_INVALID_ARGUMENT);
    }
    // SAFETY: caller guarantees the initialized readable eight-byte prefix.
    let prefix = unsafe { pointer.cast::<WireHeader>().read() };
    if prefix != math_header_value::<T>() {
        return Err(STATUS_ABI_MISMATCH);
    }
    Ok(())
}
fn usage(value: Usage) -> MathUsageRow {
    MathUsageRow {
        bytes: value.bytes,
        retained_bytes: value.retained_bytes,
        metadata_bytes: value.metadata_bytes,
        jobs: value.jobs,
        residents: value.residents,
    }
}
fn fault(value: MathError) -> WireError {
    let mut result = WireError::default();
    result.status = match value {
        MathError::Funding(DerivedError::Source(error)) => return wire_error(error),
        MathError::Funding(DerivedError::InvalidState) => 5,
        MathError::Funding(DerivedError::ArithmeticOverflow) => 20,
        MathError::Funding(DerivedError::InvalidSize) => 1,
        MathError::Funding(DerivedError::Limit {
            kind,
            requested,
            limit,
        }) => {
            result.limit_kind = 100
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
            result.requested = requested;
            result.limit = limit;
            9
        }
        MathError::StackUnavailable {
            required,
            available,
        } => {
            result.requested = required;
            result.actual = available;
            100
        }
        MathError::Execution(error) => {
            let (phase, status) = match error {
                ExecutionError::Cancelled { phase } => (phase, 4),
                ExecutionError::Deadline { phase } => (phase, 24),
                ExecutionError::Unsupported { phase } => (phase, 8),
                ExecutionError::InternalInvariant { phase } => (phase, 22),
                ExecutionError::StorageFailure { phase } => (phase, 9),
                ExecutionError::ResourceLimit {
                    phase,
                    resource,
                    requested,
                    limit,
                } => {
                    result.limit_kind = 200 + resource as u32;
                    result.requested = requested;
                    result.limit = limit;
                    (phase, 9)
                }
            };
            result.offset = phase as u64;
            status
        }
        MathError::Storage(error) => match error {
            StorageError::Execution(error) => return fault(MathError::Execution(error)),
            StorageError::FamilyBytes {
                family,
                requested,
                limit,
            } => {
                result.limit_kind = 300 + family as u32;
                result.requested = requested;
                result.limit = limit;
                9
            }
            StorageError::FamilySlots {
                family,
                requested,
                limit,
            } => {
                result.limit_kind = 400 + family as u32;
                result.requested = requested;
                result.limit = limit;
                9
            }
            StorageError::DepthLimit {
                phase,
                requested,
                limit,
            } => {
                result.offset = phase as u64;
                result.requested = requested.into();
                result.limit = limit.into();
                9
            }
            StorageError::AllocFailed { bytes } => {
                result.requested = bytes as u64;
                7
            }
            StorageError::Arithmetic => 20,
            StorageError::UnsupportedObject => 8,
            StorageError::InvalidElement | StorageError::InvalidUtf8 => 22,
        },
        MathError::InvalidGeometry => 101,
        MathError::InvalidFunding => 102,
        MathError::InvalidLimits | MathError::ObserverNotAllowed => 103,
        MathError::ParseFailure => 104,
        MathError::MissingFont { font } => {
            result.actual = font.into();
            105
        }
        MathError::MissingGlyph { font, codepoint } => {
            result.actual = font.into();
            result.offset = codepoint.into();
            106
        }
        MathError::InvalidFont { font } => {
            result.actual = font.into();
            107
        }
        MathError::Unsupported => 8,
        MathError::InternalInvariant => 22,
    };
    result
}
unsafe fn math_action(error: *mut WireError, work: impl FnOnce() -> Result<(), MathError>) -> i32 {
    // SAFETY: ABI caller provides the readable Core ABI3 prefix.
    if let Err(code) = unsafe { header(error) } {
        return code;
    }
    let mut diagnostic = WireError::default();
    let code = contain_panic(|| match work() {
        Ok(()) => 0,
        Err(value) => {
            diagnostic = fault(value);
            diagnostic.status
        }
    });
    diagnostic.status = code;
    // SAFETY: matching error header guarantees full writable independent storage.
    unsafe { error.write(diagnostic) };
    code
}
fn allocate<T>(value: T) -> Result<*mut T, MathError> {
    // SAFETY: nonzero fixed Layout; policy/metadata grant already prepaid it.
    let pointer = unsafe { alloc(Layout::new::<T>()) }.cast::<T>();
    if pointer.is_null() {
        return Err(core_fault(CoreError::OutOfMemory));
    }
    // SAFETY: fresh aligned allocation is initialized once and transferred.
    unsafe { pointer.write(value) };
    Ok(pointer)
}
unsafe fn free<T>(pointer: *mut T) {
    // SAFETY: caller owns the final reference, with no concurrent operation.
    let value = unsafe { pointer.read() };
    // SAFETY: exact Layout, value has moved to the stack; no live allocation bytes remain.
    unsafe { dealloc(pointer.cast(), Layout::new::<T>()) };
    drop(value);
}
fn retain(counter: &AtomicUsize) -> Result<(), MathError> {
    counter
        .try_update(Ordering::Relaxed, Ordering::Relaxed, |count| {
            count
                .checked_add(1)
                .filter(|next| count != 0 && *next <= isize::MAX as usize)
        })
        .map(|_| ())
        .map_err(|_| invalid_state())
}
fn release_counter(counter: &AtomicUsize, running: impl Fn() -> bool) -> Result<bool, MathError> {
    let mut count = counter.load(Ordering::Acquire);
    loop {
        if count == 0 || (count == 1 && running()) {
            return Err(invalid_state());
        }
        match counter.compare_exchange_weak(count, count - 1, Ordering::AcqRel, Ordering::Acquire) {
            Ok(_) => return Ok(count == 1),
            Err(next) => count = next,
        }
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn notepads_markdown_math_abi_version() -> u32 {
    MATH_ABI_VERSION
}

/// # Safety
/// Independent writable handle/error storage, with a readable Core error prefix.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn notepads_markdown_math_runtime_create(
    output: *mut *mut MathRuntime,
    error: *mut WireError,
) -> i32 {
    handle_output!(output);
    // SAFETY: checked outputs satisfy ABI ownership and initialization rules.
    unsafe {
        math_action(error, || {
            let metadata = SourceBudget::new(markdown_core::policy::SourceLimits {
                auxiliary_capacity: MAX_METADATA_BYTES,
                ..Default::default()
            })
            .map_err(core_fault)?;
            let policy = ProcessPolicy::new(&metadata).map_err(MathError::Funding)?;
            output.write(allocate(MathRuntime {
                references: AtomicUsize::new(1),
                metadata,
                policy,
            })?);
            Ok(())
        })
    }
}

macro_rules! policy_refs {
    ($retain:ident, $release:ident, $ty:ty) => {
        /// # Safety
        /// Live handle; caller holds its own reference throughout this call.
        #[unsafe(no_mangle)]
        pub unsafe extern "C" fn $retain(handle: *mut $ty, error: *mut WireError) -> i32 {
            preflight!(valid(handle));
            // SAFETY: only the atomic reference counter is borrowed.
            unsafe { math_action(error, || retain(&*ptr::addr_of!((*handle).references))) }
        }
        /// # Safety
        /// Transfers one reference. Final release requires no concurrent calls.
        #[unsafe(no_mangle)]
        pub unsafe extern "C" fn $release(handle: *mut $ty, error: *mut WireError) -> i32 {
            preflight!(handle.is_null() || valid(handle));
            // SAFETY: caller transfers a valid reference; final value moves out before deallocation.
            unsafe {
                math_action(error, || {
                    if !handle.is_null()
                        && release_counter(&*ptr::addr_of!((*handle).references), || false)?
                    {
                        free(handle);
                    }
                    Ok(())
                })
            }
        }
    };
}
policy_refs!(notepads_markdown_math_runtime_retain, notepads_markdown_math_runtime_release, MathRuntime);
policy_refs!(notepads_markdown_math_control_retain, notepads_markdown_math_control_release, MathControl);

/// # Safety
/// Live runtime; independent writable handle/error storage.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn notepads_markdown_math_control_create(
    runtime: *mut MathRuntime,
    output: *mut *mut MathControl,
    error: *mut WireError,
) -> i32 {
    handle_output!(output);
    preflight!(valid(runtime));
    // SAFETY: immutable runtime borrow, exact newly owned handle allocation.
    unsafe {
        math_action(error, || {
            let runtime = &*runtime;
            let policy = ControlPolicy::new(&runtime.policy, &runtime.metadata)
                .map_err(MathError::Funding)?;
            output.write(allocate(MathControl {
                references: AtomicUsize::new(1),
                process: runtime.policy.clone(),
                policy,
            })?);
            Ok(())
        })
    }
}

/// # Safety
/// Live runtime and initialized independent Math output/Core error prefixes.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn notepads_markdown_math_runtime_usage(
    runtime: *mut MathRuntime,
    output: *mut MathUsage,
    error: *mut WireError,
) -> i32 {
    preflight!(valid(runtime));
    // SAFETY: initialized output prefix as required by the ABI.
    if let Err(code) = unsafe { math_header(output) } {
        return code;
    }
    // SAFETY: accepted full output storage, immutable policy access.
    unsafe {
        math_action(error, || {
            output.write(MathUsage {
                header: math_header_value::<MathUsage>(),
                process: usage((*runtime).policy.usage()),
                control: MathUsageRow::default(),
            });
            Ok(())
        })
    }
}
/// # Safety
/// Live control and initialized independent Math output/Core error prefixes.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn notepads_markdown_math_control_usage(
    control: *mut MathControl,
    output: *mut MathUsage,
    error: *mut WireError,
) -> i32 {
    preflight!(valid(control));
    // SAFETY: initialized output prefix as required by the ABI.
    if let Err(code) = unsafe { math_header(output) } {
        return code;
    }
    // SAFETY: accepted full output storage, immutable policy access.
    unsafe {
        math_action(error, || {
            output.write(MathUsage {
                header: math_header_value::<MathUsage>(),
                process: usage((*control).process.usage()),
                control: usage((*control).policy.usage()),
            });
            Ok(())
        })
    }
}

/// # Safety
/// Live shared model/control; independent writable handle/error storage.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn notepads_markdown_math_job_prepare(
    control: *mut MathControl,
    model: *const Owned<CoreModel>,
    object: u32,
    output: *mut *mut MathJob,
    error: *mut WireError,
) -> i32 {
    handle_output!(output);
    preflight!(valid(control) && valid_handle(model));
    // SAFETY: model/control are immutable for this capture; new context is uniquely initialized.
    unsafe {
        math_action(error, || {
            // Start once before admission/allocation; queue or lock time must
            // never restart the job's absolute execution deadline.
            let deadline = Instant::now()
                .checked_add(Duration::from_secs(5))
                .ok_or(MathError::InvalidLimits)?;
            let model = &(*model).value.model;
            let index = object
                .checked_sub(1)
                .ok_or_else(|| core_fault(CoreError::UnknownId))? as usize;
            match model.ir().objects().get(index) {
                Some(ObjectRow::Derived {
                    kind: DerivedKind::Math(_),
                    ..
                }) => {}
                None => return Err(core_fault(CoreError::UnknownId)),
                _ => return Err(MathError::Unsupported),
            }
            let policy = (*control).policy.clone();
            let resident = policy.reserve_resident().map_err(MathError::Funding)?;
            let bytes = size_of::<MathJob>() as u64 + NATIVE_JOB_BYTES;
            let queued = policy
                .reserve_metadata(model, bytes)
                .map_err(MathError::Funding)?;
            output.write(allocate(MathJob {
                references: AtomicUsize::new(1),
                state: AtomicU32::new(QUEUED),
                cancelled: AtomicBool::new(false),
                start_claimed: AtomicBool::new(false),
                data: UnsafeCell::new(JobData {
                    model: Some(model.clone()),
                    policy,
                    identity: wire_identity(model.identity()),
                    object,
                    deadline,
                    queued: Some(queued),
                    job: None,
                    retained: None,
                    _resident: resident,
                }),
            })?);
            Ok(())
        })
    }
}

/// # Safety
/// Exclusive queued context, except concurrent cancel/reference operations.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn notepads_markdown_math_job_activate(handle: *mut MathJob, error: *mut WireError) -> i32 {
    preflight!(valid(handle));
    // SAFETY: data and independent cancel/state siblings have disjoint storage.
    let code = unsafe {
        math_action(error, || {
            if (*ptr::addr_of!((*handle).state)).load(Ordering::Acquire) != QUEUED {
                return Err(invalid_state());
            }
            let cancelled = &*ptr::addr_of!((*handle).cancelled);
            let data = &mut *(*ptr::addr_of!((*handle).data)).get();
            let model = data.model.as_ref().ok_or_else(invalid_state)?.clone();
            let host = size_of::<MathJob>() as u64 - size_of::<DeferredMathJob>() as u64
                + NATIVE_JOB_BYTES;
            let job = DeferredMathJob::prepare(
                model,
                &data.policy,
                data.object - 1,
                host,
                cancelled,
                data.deadline,
            )?;
            data.job = Some(job);
            drop(data.model.take());
            // Keep the original independent context/Node metadata lease until
            // actual final context free. The 64MiB seed deliberately double
            // counts it while active and after successful finalization.
            (*ptr::addr_of!((*handle).state)).store(ACTIVATED, Ordering::Release);
            Ok(())
        })
    };
    // SAFETY: the validated context remains live; no worker starts before activation succeeds.
    unsafe { poison_on_panic(handle, code) }
}

/// # Safety
/// Live queued or activated context. The private native owner claims before allocating its
/// prepaid node or thread. A claim is consumed even if native creation fails:
/// retries require releasing this context and preparing a new funded job.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn notepads_markdown_math_job_claim_start(
    handle: *mut MathJob,
    error: *mut WireError,
) -> i32 {
    preflight!(valid(handle));
    // SAFETY: only independent atomic siblings are read/modified, no data borrow.
    unsafe {
        math_action(error, || {
            if !matches!(
                (*ptr::addr_of!((*handle).state)).load(Ordering::Acquire),
                QUEUED | ACTIVATED
            ) {
                return Err(invalid_state());
            }
            (*ptr::addr_of!((*handle).start_claimed))
                .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
                .map_err(|_| invalid_state())?;
            Ok(())
        })
    }
}

/// # Safety
/// Live owned reference. May overlap run; only the atomic counter is accessed.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn notepads_markdown_math_job_retain(handle: *mut MathJob, error: *mut WireError) -> i32 {
    preflight!(valid(handle));
    // SAFETY: independent atomic sibling, no job-data borrow.
    unsafe { math_action(error, || retain(&*ptr::addr_of!((*handle).references))) }
}
/// # Safety
/// Transfer one reference. Final release requires thread signaled and all native
/// payloads already freed. It must not race run, activation or post-join calls.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn notepads_markdown_math_job_release(handle: *mut MathJob, error: *mut WireError) -> i32 {
    preflight!(handle.is_null() || valid(handle));
    // SAFETY: caller transfers a live reference; atomic state is an extra early-release guard.
    unsafe {
        math_action(error, || {
            if !handle.is_null()
                && release_counter(&*ptr::addr_of!((*handle).references), || {
                    (*ptr::addr_of!((*handle).state)).load(Ordering::Acquire) == RUNNING
                })?
            {
                free(handle);
            }
            Ok(())
        })
    }
}
/// # Safety
/// Live reference through return; error storage is independent of concurrent run.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn notepads_markdown_math_job_cancel(handle: *mut MathJob, error: *mut WireError) -> i32 {
    preflight!(valid(handle));
    // SAFETY: touches only the independent stable atomic flag, never the whole context/job.
    unsafe {
        math_action(error, || {
            (*ptr::addr_of!((*handle).cancelled)).store(true, Ordering::Release);
            Ok(())
        })
    }
}
/// # Safety
/// Live reference through return. This atomic diagnostic is not proof of thread exit.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn notepads_markdown_math_job_state(handle: *const MathJob) -> u32 {
    if !valid(handle) {
        return 0;
    }
    // SAFETY: shared access only to the atomic state sibling.
    unsafe { (*ptr::addr_of!((*handle).state)).load(Ordering::Acquire) }
}

/// # Safety
/// Unique native worker invocation; parent owns a reference until actual handle
/// signaled. Concurrent access is limited to independent cancel/ref/state fields.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn notepads_markdown_math_job_run(
    handle: *mut MathJob,
    measured_headroom: u64,
    error: *mut WireError,
) -> i32 {
    // SAFETY: this entry inherits the worker/exclusive lifetime contract.
    unsafe { run_math_job(handle, measured_headroom, None, error) }
}

/// # Safety
/// Same worker contract as notepads_markdown_math_job_run. The provider is a synchronous
/// private platform function. It borrows each request and sink only until its
/// call returns and never invokes the sink concurrently.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn notepads_markdown_math_job_run_with_text(
    handle: *mut MathJob,
    measured_headroom: u64,
    shape: Option<MathTextShape>,
    error: *mut WireError,
) -> i32 {
    let Some(shape) = shape else {
        return STATUS_INVALID_ARGUMENT;
    };
    // SAFETY: this entry inherits the worker/exclusive lifetime contract.
    unsafe {
        run_math_job(
            handle,
            measured_headroom,
            Some(NativeChineseTextShaper(shape)),
            error,
        )
    }
}

unsafe fn run_math_job(
    handle: *mut MathJob,
    measured_headroom: u64,
    mut shaper: Option<NativeChineseTextShaper>,
    error: *mut WireError,
) -> i32 {
    preflight!(valid(handle));
    let mut entered = false;
    // SAFETY: only the UnsafeCell job field is mutably borrowed, disjoint from cancel/ref/state.
    let code = unsafe {
        math_action(error, || {
            if !(*ptr::addr_of!((*handle).start_claimed)).load(Ordering::Acquire) {
                return Err(invalid_state());
            }
            (*ptr::addr_of!((*handle).state))
                .compare_exchange(ACTIVATED, RUNNING, Ordering::AcqRel, Ordering::Acquire)
                .map_err(|_| invalid_state())?;
            entered = true;
            let data = &mut *(*ptr::addr_of!((*handle).data)).get();
            let cancelled = &*ptr::addr_of!((*handle).cancelled);
            let stack = native_stack_contract(measured_headroom);
            data.job
                .as_mut()
                .ok_or_else(invalid_state)?
                .run_with_chinese_text(
                    cancelled,
                    &FontAssets::new(),
                    &MathLimits,
                    stack,
                    shaper
                        .as_mut()
                        .map(|shape| shape as &mut dyn ChineseTextShaper),
                )
        })
    };
    if entered {
        // SAFETY: atomic sibling remains live under the parent's retained reference.
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

unsafe fn computed_data<'a>(handle: *mut MathJob) -> Result<&'a mut JobData, MathError> {
    // SAFETY: private caller has observed handle signaled and supplies exclusive post-join access.
    if unsafe { (*ptr::addr_of!((*handle).state)).load(Ordering::Acquire) } != COMPUTED {
        return Err(invalid_state());
    }
    // SAFETY: actual worker terminated; no remaining concurrent access to this field.
    Ok(unsafe { &mut *(*ptr::addr_of!((*handle).data)).get() })
}

unsafe fn poison_on_panic(handle: *mut MathJob, code: i32) -> i32 {
    if code == STATUS_PANIC {
        // SAFETY: caller owns a live context; only the independent atomic sibling is touched.
        unsafe { (*ptr::addr_of!((*handle).state)).store(POISONED, Ordering::Release) };
    }
    code
}
/// # Safety
/// Thread handle actually signaled; exclusive context and initialized output/error prefixes.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn notepads_markdown_math_job_summary(
    handle: *mut MathJob,
    output: *mut MathSummary,
    error: *mut WireError,
) -> i32 {
    preflight!(valid(handle));
    // SAFETY: readable output prefix.
    if let Err(code) = unsafe { math_header(output) } {
        return code;
    }
    // SAFETY: signaled/exclusive caller guarantee and accepted writable output.
    let code = unsafe {
        math_action(error, || {
            let data = computed_data(handle)?;
            let job = data.job.as_ref().ok_or_else(invalid_state)?;
            let artifact = job.data()?;
            let metrics = artifact.metrics;
            let cumulative = job.cumulative_snapshot();
            output.write(MathSummary {
                header: math_header_value::<MathSummary>(),
                identity: data.identity,
                object: data.object,
                state: COMPUTED,
                metrics: MathMetrics {
                    advance: metrics.advance,
                    ascent: metrics.ascent,
                    descent: metrics.descent,
                    left: metrics.ink.left,
                    top: metrics.ink.top,
                    right: metrics.ink.right,
                    bottom: metrics.ink.bottom,
                },
                commands: artifact.commands.len() as u64,
                primitives: artifact.primitives.len() as u64,
                prepaid_metadata_bytes: job.prepaid_metadata_bytes(),
                computed_rust_bytes: job.computed_rust_bytes(),
                reserved_bytes: job.reserved_bytes(),
                cumulative_bytes: *cumulative.used_bytes(),
                cumulative_slots: *cumulative.used_slots(),
            });
            Ok(())
        })
    };
    // SAFETY: this exclusively owned post-join context remains live on failure.
    unsafe { poison_on_panic(handle, code) }
}

fn command(value: MathPathCommand) -> MathCommand {
    let mut row = MathCommand::default();
    match value {
        MathPathCommand::MoveTo { x, y } => {
            row.kind = 1;
            row.x1 = x;
            row.y1 = y;
        }
        MathPathCommand::LineTo { x, y } => {
            row.kind = 2;
            row.x1 = x;
            row.y1 = y;
        }
        MathPathCommand::QuadTo { x1, y1, x, y } => {
            row.kind = 3;
            row.x1 = x1;
            row.y1 = y1;
            row.x2 = x;
            row.y2 = y;
        }
        MathPathCommand::CubicTo {
            x1,
            y1,
            x2,
            y2,
            x,
            y,
        } => {
            row.kind = 4;
            row.x1 = x1;
            row.y1 = y1;
            row.x2 = x2;
            row.y2 = y2;
            row.x3 = x;
            row.y3 = y;
        }
        MathPathCommand::Close => row.kind = 5,
    }
    row
}
/// # Safety
/// Thread signaled, exclusive context; full independent buffer and initialized
/// result/error prefixes. Discard entire buffer if any read fails.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn notepads_markdown_math_job_read(
    handle: *mut MathJob,
    table: u32,
    start: u64,
    count: u64,
    row_size: u32,
    buffer: *mut std::ffi::c_void,
    output: *mut MathReadResult,
    error: *mut WireError,
) -> i32 {
    preflight!(valid(handle) && valid(buffer.cast::<MathCommand>()) && count <= 256);
    let expected = match table {
        1 => size_of::<MathCommand>(),
        2 => size_of::<MathPrimitive>(),
        _ => return STATUS_INVALID_ARGUMENT,
    };
    preflight!(row_size as usize == expected);
    // SAFETY: initialized output prefix.
    if let Err(code) = unsafe { math_header(output) } {
        return code;
    }
    // SAFETY: accepted independent buffer/output, actual post-join exclusive ownership.
    let code = unsafe {
        math_action(error, || {
            let data = computed_data(handle)?;
            let artifact = data.job.as_ref().ok_or_else(invalid_state)?.data()?;
            let total = if table == 1 {
                artifact.commands.len()
            } else {
                artifact.primitives.len()
            };
            let start =
                usize::try_from(start).map_err(|_| core_fault(CoreError::ArithmeticOverflow))?;
            let end = start
                .checked_add(count as usize)
                .filter(|end| *end <= total)
                .ok_or_else(|| core_fault(CoreError::InvalidRange))?;
            for index in start..end {
                if table == 1 {
                    buffer
                        .cast::<MathCommand>()
                        .add(index - start)
                        .write(command(artifact.commands[index]));
                } else {
                    let value = artifact.primitives[index];
                    // The bounded producer accepts monochrome elementary TeX;
                    // color handlers are rejected before their arguments. Its
                    // paint contract is foreground, independent of RGBA values.
                    // Supporting explicit colors later requires real provenance
                    // from the engine, never inferring it from numeric black.
                    buffer
                        .cast::<MathPrimitive>()
                        .add(index - start)
                        .write(MathPrimitive {
                            command_start: value.command_start,
                            command_count: value.command_len,
                            flags: u32::from(value.fill) | 2,
                            reserved: 0,
                            r: value.color.r,
                            g: value.color.g,
                            b: value.color.b,
                            a: value.color.a,
                            stroke_width: value.stroke_width,
                        });
                }
            }
            output.write(MathReadResult {
                header: math_header_value::<MathReadResult>(),
                written: count,
                next: end as u64,
                done: u32::from(end == total),
                reserved: 0,
            });
            Ok(())
        })
    };
    // SAFETY: this exclusively owned post-join context remains live on failure.
    unsafe { poison_on_panic(handle, code) }
}
/// # Safety
/// Actual thread signaled and exclusive job; request precedes the native allocation.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn notepads_markdown_math_job_charge_native(
    handle: *mut MathJob,
    family: u32,
    bytes: u64,
    slots: u64,
    error: *mut WireError,
) -> i32 {
    preflight!(valid(handle));
    let family = match family {
        1 => StorageFamily::Path,
        2 => StorageFamily::Mirror,
        3 => StorageFamily::Metadata,
        _ => return STATUS_INVALID_ARGUMENT,
    };
    // SAFETY: exclusive post-join job data, disjoint stable flag.
    let code = unsafe {
        math_action(error, || {
            let data = computed_data(handle)?;
            data.job.as_mut().ok_or_else(invalid_state)?.charge_native(
                &*ptr::addr_of!((*handle).cancelled),
                family,
                bytes,
                slots,
            )
        })
    };
    // SAFETY: this exclusively owned post-join context remains live on failure.
    unsafe { poison_on_panic(handle, code) }
}
/// # Safety
/// Actual thread signaled; all transient/native copies freed; actual includes
/// all live capacities and headers. Native keeps a context ref until its last free.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn notepads_markdown_math_job_finalize_native(
    handle: *mut MathJob,
    actual_native_bytes: u64,
    error: *mut WireError,
) -> i32 {
    preflight!(valid(handle));
    let mut attempted = false;
    // SAFETY: private caller provides unique post-join access and truthful native capacities.
    let code = unsafe {
        math_action(error, || {
            let data = computed_data(handle)?;
            attempted = true;
            let retained = data
                .job
                .as_mut()
                .ok_or_else(invalid_state)?
                .finalize_native(&*ptr::addr_of!((*handle).cancelled), actual_native_bytes)?;
            data.retained = Some(retained);
            drop(data.job.take());
            Ok(())
        })
    };
    if attempted {
        // SAFETY: same exclusive live context; terminal state has no further math mutation.
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

/// Retires a failed/cancelled attempt without retaining its full worker grant.
///
/// # Safety
/// The caller has exclusive access, with no concurrent retain, authority
/// creation or mutable operation. For Queued/Activated it proves that no thread
/// was created. For Computed/FinishFailed the actual thread handle must already
/// be signaled. All native payloads/authorities must have been actually freed.
/// Only the PreparedNode's original context reference may remain. A leaked
/// NativeMathMemory reference (including actual HeapFree failure) therefore
/// refuses retirement and preserves full funding. C++ intrusive Node owners
/// share that original reference rather than retaining this FFI context.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn notepads_markdown_math_job_retire_metadata(
    handle: *mut MathJob,
    error: *mut WireError,
) -> i32 {
    preflight!(valid(handle));
    // SAFETY: the private caller proves nonconcurrency and actual thread exit.
    let code = unsafe {
        math_action(error, || {
            if !matches!(
                (*ptr::addr_of!((*handle).state)).load(Ordering::Acquire),
                QUEUED | ACTIVATED | COMPUTED | FINISH_FAILED
            ) || (*ptr::addr_of!((*handle).references)).load(Ordering::Acquire) != 1
            {
                return Err(invalid_state());
            }
            let data = &mut *(*ptr::addr_of!((*handle).data)).get();
            // All allowed states still own the independent prepare lease.
            if data.queued.is_none() || data.retained.is_some() {
                return Err(invalid_state());
            }
            drop(data.job.take());
            drop(data.model.take());
            (*ptr::addr_of!((*handle).state)).store(METADATA_ONLY, Ordering::Release);
            Ok(())
        })
    };
    // SAFETY: live context, stable atomic sibling only.
    unsafe { poison_on_panic(handle, code) }
}

#[cfg(test)]
mod tests;
