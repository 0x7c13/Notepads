// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE file in the project root for license information.
// ---------------------------------------------------------------------------------------------
//! One-shot private renderer jobs. Work and cancellation borrow disjoint fields.
use super::super::transport::{distinct, region};
use super::access::AccountAccess;
use super::resources_bridge::*;
use super::*;
use crate::panic_boundary::drop_panic_payload;
use markdown_svg::{CleanupStatus, Identity, JobError as EngineError, JobState, OwnedJob};
use markdown_svg_budget::{Error as BudgetError, FailureKind, LimitKind as SvgLimit, Limits};
use std::{
    cell::UnsafeCell,
    mem::ManuallyDrop,
    panic::{AssertUnwindSafe, catch_unwind},
    sync::atomic::{AtomicBool, AtomicU32},
};

pub const SVG_JOB_ABI_VERSION: u32 = 1;
#[repr(C, align(8))]
#[derive(Clone, Copy, Default)]
pub struct SvgJobHeader {
    pub abi_version: u32,
    pub struct_size: u32,
}
impl SvgJobHeader {
    fn of<T>() -> Self {
        Self {
            abi_version: SVG_JOB_ABI_VERSION,
            struct_size: size_of::<T>() as u32,
        }
    }
}
#[repr(C, align(8))]
#[derive(Clone, Copy, Default, Debug, PartialEq, Eq)]
pub struct SvgJobIdentity {
    pub job_nonce: u64,
    pub source_generation: u64,
    pub provider_generation: u64,
    pub font_generation: u64,
    pub target_generation: u64,
}
impl From<SvgJobIdentity> for Identity {
    fn from(v: SvgJobIdentity) -> Self {
        Self {
            job_nonce: v.job_nonce,
            source_generation: v.source_generation,
            provider_generation: v.provider_generation,
            font_generation: v.font_generation,
            target_generation: v.target_generation,
        }
    }
}
#[repr(C, align(8))]
#[derive(Clone, Copy, Default)]
pub struct SvgJobError {
    pub header: SvgJobHeader,
    pub transport_status: i32,
    pub job_error_kind: u32,
    pub first_fault: SvgFault,
}
#[repr(C, align(8))]
#[derive(Clone, Copy, Default)]
pub struct SvgJobNatural {
    pub header: SvgJobHeader,
    pub identity: SvgJobIdentity,
    pub width_dip: f64,
    pub height_dip: f64,
    pub viewport_width_bits: u32,
    pub viewport_height_bits: u32,
}
#[repr(C, align(8))]
#[derive(Clone, Copy, Default)]
pub struct SvgJobTarget {
    pub header: SvgJobHeader,
    pub display_width_dip: f64,
    pub effective_physical_scale: f64,
    pub generation: u64,
}
#[repr(C, align(8))]
#[derive(Clone, Copy, Default)]
pub struct SvgJobPlan {
    pub header: SvgJobHeader,
    pub identity: SvgJobIdentity,
    pub natural_width_dip: f64,
    pub natural_height_dip: f64,
    pub requested_bucket: f64,
    pub ideal_scale: f64,
    pub extent_width: f64,
    pub extent_height: f64,
    pub viewport_width_bits: u32,
    pub viewport_height_bits: u32,
    pub scale_bits: u32,
    pub target_bucket: u32,
    pub width: u32,
    pub height: u32,
    pub stride: u32,
    pub resolution_limited: u32,
    pub capacity: u64,
}
#[repr(C, align(8))]
#[derive(Clone, Copy, Default)]
pub struct SvgJobRetirement {
    pub header: SvgJobHeader,
    pub identity: SvgJobIdentity,
    pub terminal_state: u32,
    pub cleanup_succeeded: u32,
    pub completed_stages: u32,
    pub job_error_kind: u32,
    pub first_fault: SvgFault,
    pub remaining_owners: u64,
    pub failed_releases: u64,
    pub cancelled: u32,
    pub reserved: u32,
}
#[repr(C, align(8))]
#[derive(Clone, Copy, Default)]
pub struct SvgJobDestroyed {
    pub header: SvgJobHeader,
    pub identity: SvgJobIdentity,
    pub consumed: u32,
    pub cleanup_succeeded: u32,
    pub cancelled: u32,
    pub reserved: u32,
    pub first_fault: SvgFault,
    pub unreleased_shell: SvgWorkToken,
    pub unreleased_engine: SvgWorkToken,
}
const _: () = {
    assert!(size_of::<SvgJobIdentity>() == 40);
    assert!(size_of::<SvgJobError>() == 32);
    assert!(size_of::<SvgJobNatural>() == 72);
    assert!(size_of::<SvgJobTarget>() == 32);
    assert!(size_of::<SvgJobPlan>() == 136);
    assert!(size_of::<SvgJobRetirement>() == 104);
    assert!(size_of::<SvgJobDestroyed>() == 112);
};

#[repr(C, align(8))]
pub struct SvgJobCancel {
    token: CancellationToken,
    refs: AtomicU32,
    closed: AtomicBool,
}
struct WorkerData {
    engine: Option<ManuallyDrop<OwnedJob>>,
    initial_error: Option<EngineError>,
    engine_fee: Option<SvgWorkToken>,
    baseline_tickets: u32,
    baseline_pending: u32,
    finished: bool,
    cleanup_ok: bool,
}
#[repr(C, align(8))]
pub struct SvgRendererJob {
    cancel: SvgJobCancel,
    data: UnsafeCell<WorkerData>,
    host: ManuallyDrop<Arc<AccountAccess>>,
    resources: *mut SvgResources,
    identity: SvgJobIdentity,
    shell_fee: SvgWorkToken,
    worker: std::thread::ThreadId,
}
fn valid_job(p: *const SvgRendererJob) -> bool {
    aligned(p) && distinct(&[region(p)])
}
unsafe fn job_header<T>(p: *const T) -> Result<(), i32> {
    if !aligned(p) {
        return Err(STATUS_INVALID_ARGUMENT);
    }
    let h = unsafe { p.cast::<SvgJobHeader>().read() };
    if h.abi_version != SVG_JOB_ABI_VERSION || h.struct_size as usize != size_of::<T>() {
        Err(STATUS_ABI_MISMATCH)
    } else {
        Ok(())
    }
}
fn error_kind(e: EngineError) -> u32 {
    match e {
        EngineError::InvalidState => 1,
        EngineError::InvalidArgument => 2,
        EngineError::GeometryUnrepresentable => 3,
        EngineError::Failure(_) => 4,
        EngineError::Panic => 5,
    }
}
fn error_status(e: EngineError) -> i32 {
    match e {
        EngineError::InvalidState => 5,
        EngineError::InvalidArgument => 1,
        EngineError::GeometryUnrepresentable => 8,
        EngineError::Panic => 6,
        EngineError::Failure(v) => match v.kind {
            FailureKind::Cancelled => 4,
            FailureKind::Quota => 9,
            FailureKind::Allocation | FailureKind::RasterAllocation => 7,
            FailureKind::Unsupported => 8,
            _ => 16,
        },
    }
}
fn core_failure(e: CoreError) -> EngineError {
    match e {
        CoreError::InvalidState => EngineError::InvalidState,
        CoreError::InvalidRange | CoreError::InvalidBoundary => EngineError::InvalidArgument,
        _ => EngineError::Failure(access::resource_error(e)),
    }
}
unsafe fn invoke(
    error: *mut SvgJobError,
    resources: *const SvgResources,
    work: impl FnOnce() -> Result<(), EngineError>,
) -> i32 {
    if let Err(s) = unsafe { job_header(error) } {
        return s;
    }
    let status = contain_panic(|| match work() {
        Ok(()) => {
            unsafe {
                error.write(SvgJobError {
                    header: SvgJobHeader::of::<SvgJobError>(),
                    ..Default::default()
                })
            };
            0
        }
        Err(e) => {
            if let EngineError::Failure(v) = e {
                unsafe { record_renderer_fault(resources, v) };
            }
            if e == EngineError::Panic {
                unsafe {
                    record_renderer_fault(
                        resources,
                        BudgetError::new(FailureKind::Invariant, SvgLimit::None),
                    )
                };
            }
            let status = error_status(e);
            unsafe {
                error.write(SvgJobError {
                    header: SvgJobHeader::of::<SvgJobError>(),
                    transport_status: status,
                    job_error_kind: error_kind(e),
                    first_fault: fault_wire(renderer_fault(resources)),
                })
            };
            status
        }
    });
    if status == STATUS_PANIC {
        unsafe {
            record_renderer_fault(
                resources,
                BudgetError::new(FailureKind::Invariant, SvgLimit::None),
            );
            error.write(SvgJobError {
                header: SvgJobHeader::of::<SvgJobError>(),
                transport_status: 6,
                job_error_kind: 5,
                first_fault: fault_wire(renderer_fault(resources)),
            })
        };
    }
    status
}
#[unsafe(no_mangle)]
pub extern "C" fn notepads_markdown_svg_job_abi_version() -> u32 {
    1
}

/// # Safety
/// Live resources and distinct valid inputs and outputs. The coverage table is
/// copied during creation and must refer to the same session as the font table.
/// A nonnull returned job owns cleanup even if semantic initialization fails.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn notepads_markdown_svg_job_create(
    version: u32,
    resources: *mut SvgResources,
    fonts: *const markdown_svg::abi::Callbacks,
    coverage: *const markdown_svg::coverage::Callbacks,
    identity: *const SvgJobIdentity,
    output: *mut *mut SvgRendererJob,
    cancel: *mut *mut SvgJobCancel,
    error: *mut SvgJobError,
) -> i32 {
    // Validate borrowed regions before reading inputs or writing outputs.
    preflight!(
        valid_resource(resources)
            && aligned(fonts)
            && aligned(coverage)
            && aligned(identity)
            && aligned(output)
            && aligned(cancel)
            && distinct(&[
                region(resources),
                region(fonts),
                region(coverage),
                region(identity),
                region(output),
                region(cancel),
                region(error)
            ])
    );
    if version != SVG_JOB_ABI_VERSION {
        return STATUS_ABI_MISMATCH;
    }
    unsafe {
        invoke(error, resources, || {
            let id = identity.read();
            let callbacks = fonts.read();
            if id.job_nonce == 0
                || id.source_generation == 0
                || id.provider_generation == 0
                || id.font_generation == 0
                || id.target_generation == 0
                || callbacks.validate().is_err()
            {
                return Err(EngineError::InvalidArgument);
            }
            let coverage = markdown_svg::coverage::NativeCoverageProvider::new(
                coverage.read(),
                callbacks.session,
            )
            .map_err(|_| EngineError::InvalidArgument)?;
            if !renderer_binding_matches(
                resources,
                id.job_nonce,
                id.source_generation,
                id.provider_generation,
            )
            .map_err(core_failure)?
            {
                return Err(EngineError::InvalidArgument);
            }
            let metadata = markdown_svg::bootstrap_metadata();
            let inner = metadata
                .total_bytes()
                .and_then(|v| v.checked_sub(metadata.shell_bytes))
                .and_then(|v| v.checked_sub(metadata.first_failure_arc_bytes))
                .ok_or(EngineError::InvalidArgument)?;
            let worker = std::thread::current().id();
            let (host, fault, token) = claim_renderer(resources).map_err(core_failure)?;
            let shell = match host.with(|a, _| a.reserve(0, size_of::<SvgRendererJob>() as u64)) {
                Ok(v) => v,
                Err(e) => {
                    complete_renderer_claim(resources, false).map_err(core_failure)?;
                    return Err(core_failure(e));
                }
            };
            let pointer =
                std::alloc::alloc(Layout::new::<SvgRendererJob>()).cast::<SvgRendererJob>();
            if pointer.is_null() {
                let released = host.with_cleanup(true, |a, _| a.release_after_free(shell));
                let rolled_back = complete_renderer_claim(resources, false);
                released.map_err(core_failure)?;
                rolled_back.map_err(core_failure)?;
                return Err(EngineError::Failure(BudgetError::new(
                    FailureKind::Allocation,
                    SvgLimit::None,
                )));
            }
            let (tickets, pending) = match host.with(|a, _| {
                a.admit(shell, size_of::<SvgRendererJob>() as u64)?;
                if injected(6) {
                    return Err(CoreError::OutOfMemory);
                }
                if a.renderer_phase != 1 {
                    return Err(CoreError::InvalidState);
                }
                let baseline = (a.tickets, a.pending);
                a.renderer_phase = 2;
                Ok(baseline)
            }) {
                Ok(baseline) => baseline,
                Err(e) => {
                    std::alloc::dealloc(pointer.cast(), Layout::new::<SvgRendererJob>());
                    let released = host.with_cleanup(true, |a, _| a.release_after_free(shell));
                    let rolled_back = complete_renderer_claim(resources, false);
                    released.map_err(core_failure)?;
                    rolled_back.map_err(core_failure)?;
                    return Err(core_failure(e));
                }
            };
            pointer.write(SvgRendererJob {
                cancel: SvgJobCancel {
                    token,
                    refs: AtomicU32::new(1),
                    closed: AtomicBool::new(false),
                },
                data: UnsafeCell::new(WorkerData {
                    engine: None,
                    initial_error: None,
                    engine_fee: None,
                    baseline_tickets: tickets,
                    baseline_pending: pending,
                    finished: false,
                    cleanup_ok: false,
                }),
                host: ManuallyDrop::new(host),
                resources,
                identity: id,
                shell_fee: shell,
                worker,
            });
            // This canonical shell now owns any partial initialization. Publish it
            // before initialization, so every semantic failure has a cleanup owner.
            output.write(pointer);
            cancel.write(&raw mut (*pointer).cancel);
            let data = &mut *(*pointer).data.get();
            let init = catch_unwind(AssertUnwindSafe(|| -> Result<(), EngineError> {
                (*pointer)
                    .host
                    .bind_coverage(coverage)
                    .map_err(core_failure)?;
                if injected(1) {
                    return Err(EngineError::Failure(BudgetError::new(
                        FailureKind::Allocation,
                        SvgLimit::None,
                    )));
                }
                let fee = (*pointer)
                    .host
                    .with(|a, _| a.reserve(0, inner as u64))
                    .map_err(core_failure)?;
                data.engine_fee = Some(fee);
                let engine = OwnedJob::new_with_first_failure(
                    id.into(),
                    callbacks,
                    Arc::clone(&(*pointer).host) as Arc<dyn markdown_svg::Host>,
                    Limits::default(),
                    fault,
                )?;
                data.engine = Some(ManuallyDrop::new(engine));
                if injected(2) {
                    return Err(EngineError::Failure(BudgetError::native(-778)));
                }
                (*pointer)
                    .host
                    .with(|a, _| a.admit(fee, inner as u64))
                    .map_err(core_failure)?;
                Ok(())
            }));
            let result = match init {
                Ok(v) => v,
                Err(payload) => {
                    drop_panic_payload(payload);
                    Err(EngineError::Panic)
                }
            };
            if let Err(e) = result {
                data.initial_error = Some(e);
                return Err(e);
            }
            Ok(())
        })
    }
}
unsafe fn worker_data<'a>(job: *mut SvgRendererJob) -> Result<&'a mut WorkerData, EngineError> {
    unsafe {
        if (*job).worker != std::thread::current().id() {
            return Err(EngineError::InvalidState);
        }
        Ok(&mut *(*job).data.get())
    }
}
#[unsafe(no_mangle)]
pub unsafe extern "C" fn notepads_markdown_svg_job_prepare(
    job: *mut SvgRendererJob,
    source: *const u8,
    length: u64,
    output: *mut SvgJobNatural,
    error: *mut SvgJobError,
) -> i32 {
    preflight!(
        valid_job(job)
            && length <= isize::MAX as u64
            && (length == 0 || !source.is_null())
            && distinct(&[
                region(job),
                region(output),
                region(error),
                (source as usize, length as usize)
            ])
    );
    if let Err(s) = unsafe { job_header(output) } {
        return s;
    }
    unsafe {
        invoke(error, (*job).resources, || {
            let data = worker_data(job)?;
            if data.finished {
                return Err(EngineError::InvalidState);
            }
            if let Some(e) = data.initial_error {
                return Err(e);
            }
            let bytes = if length == 0 {
                &[]
            } else {
                std::slice::from_raw_parts(source, length as usize)
            };
            let natural = data
                .engine
                .as_mut()
                .ok_or(EngineError::InvalidState)?
                .prepare(bytes)?;
            output.write(SvgJobNatural {
                header: SvgJobHeader::of::<SvgJobNatural>(),
                identity: (*job).identity,
                width_dip: natural.width_dip,
                height_dip: natural.height_dip,
                viewport_width_bits: natural.viewport_width_bits,
                viewport_height_bits: natural.viewport_height_bits,
            });
            Ok(())
        })
    }
}
#[unsafe(no_mangle)]
pub unsafe extern "C" fn notepads_markdown_svg_job_set_target(
    job: *mut SvgRendererJob,
    target: *const SvgJobTarget,
    output: *mut SvgJobPlan,
    error: *mut SvgJobError,
) -> i32 {
    preflight!(
        valid_job(job) && distinct(&[region(job), region(target), region(output), region(error)])
    );
    if let Err(s) = unsafe { job_header(target).and_then(|_| job_header(output)) } {
        return s;
    }
    unsafe {
        invoke(error, (*job).resources, || {
            let data = worker_data(job)?;
            if data.finished {
                return Err(EngineError::InvalidState);
            }
            if let Some(e) = data.initial_error {
                return Err(e);
            }
            let t = target.read();
            let p = data
                .engine
                .as_mut()
                .ok_or(EngineError::InvalidState)?
                .set_target(markdown_svg::Target {
                    display_width_dip: t.display_width_dip,
                    effective_physical_scale: t.effective_physical_scale,
                    generation: t.generation,
                })?;
            output.write(SvgJobPlan {
                header: SvgJobHeader::of::<SvgJobPlan>(),
                identity: (*job).identity,
                natural_width_dip: p.natural.width_dip,
                natural_height_dip: p.natural.height_dip,
                requested_bucket: p.requested_bucket,
                ideal_scale: p.ideal_scale,
                extent_width: p.extent_width,
                extent_height: p.extent_height,
                viewport_width_bits: p.natural.viewport_width_bits,
                viewport_height_bits: p.natural.viewport_height_bits,
                scale_bits: p.scale_bits,
                target_bucket: p.target_bucket,
                width: p.width,
                height: p.height,
                stride: p.stride,
                resolution_limited: u32::from(p.resolution_limited),
                capacity: p.capacity as u64,
            });
            Ok(())
        })
    }
}
#[unsafe(no_mangle)]
pub unsafe extern "C" fn notepads_markdown_svg_job_run(
    job: *mut SvgRendererJob,
    identity: *const SvgJobIdentity,
    pixels: *mut u8,
    capacity: u64,
    stride: u32,
    error: *mut SvgJobError,
) -> i32 {
    preflight!(
        valid_job(job)
            && aligned(identity)
            && !pixels.is_null()
            && capacity <= isize::MAX as u64
            && distinct(&[
                region(job),
                region(identity),
                region(error),
                (pixels as usize, capacity as usize)
            ])
    );
    unsafe {
        invoke(error, (*job).resources, || {
            let data = worker_data(job)?;
            if data.finished {
                return Err(EngineError::InvalidState);
            }
            if let Some(e) = data.initial_error {
                return Err(e);
            }
            data.engine
                .as_mut()
                .ok_or(EngineError::InvalidState)?
                .run_into(
                    identity.read().into(),
                    std::slice::from_raw_parts_mut(pixels, capacity as usize),
                    stride,
                )?;
            Ok(())
        })
    }
}

#[cfg(feature = "quality-observation")]
#[repr(C, align(8))]
#[derive(Clone, Copy, Default)]
pub struct SvgQualityTraceInfo {
    pub header: SvgJobHeader,
    pub identity: SvgJobIdentity,
    pub bytes: u64,
    pub capacity: u64,
    pub events: u64,
    pub flags: u32,
    pub trace_error: u32,
}
#[cfg(feature = "quality-observation")]
const _: () = assert!(size_of::<SvgQualityTraceInfo>() == 80);

/// Test-feature-only batch observation; no record or borrowed pointer escapes.
#[cfg(feature = "quality-observation")]
#[unsafe(no_mangle)]
pub unsafe extern "C" fn notepads_markdown_svg_job_quality_enable(
    job: *mut SvgRendererJob,
    capacity: u64,
    error: *mut SvgJobError,
) -> i32 {
    preflight!(
        valid_job(job)
            && (1..=8 * 1024 * 1024).contains(&capacity)
            && distinct(&[region(job), region(error)])
    );
    unsafe {
        invoke(error, (*job).resources, || {
            let data = worker_data(job)?;
            if data.finished {
                return Err(EngineError::InvalidState);
            }
            if let Some(error) = data.initial_error {
                return Err(error);
            }
            data.engine
                .as_mut()
                .ok_or(EngineError::InvalidState)?
                .enable_quality_observation(capacity as usize)
        })
    }
}

#[cfg(feature = "quality-observation")]
#[unsafe(no_mangle)]
pub unsafe extern "C" fn notepads_markdown_svg_job_quality_info(
    job: *mut SvgRendererJob,
    output: *mut SvgQualityTraceInfo,
    error: *mut SvgJobError,
) -> i32 {
    preflight!(valid_job(job) && distinct(&[region(job), region(output), region(error)]));
    if let Err(status) = unsafe { job_header(output) } {
        return status;
    }
    unsafe {
        invoke(error, (*job).resources, || {
            let data = worker_data(job)?;
            if data.finished {
                return Err(EngineError::InvalidState);
            }
            let value = data
                .engine
                .as_ref()
                .ok_or(EngineError::InvalidState)?
                .quality_trace_info()?;
            output.write(SvgQualityTraceInfo {
                header: SvgJobHeader::of::<SvgQualityTraceInfo>(),
                identity: (*job).identity,
                bytes: value.bytes,
                capacity: value.capacity,
                events: value.events,
                flags: value.flags,
                trace_error: value.error,
            });
            Ok(())
        })
    }
}

#[cfg(feature = "quality-observation")]
#[unsafe(no_mangle)]
pub unsafe extern "C" fn notepads_markdown_svg_job_quality_copy(
    job: *mut SvgRendererJob,
    output: *mut u8,
    capacity: u64,
    written: *mut u64,
    error: *mut SvgJobError,
) -> i32 {
    preflight!(
        valid_job(job)
            && aligned(written)
            && capacity <= isize::MAX as u64
            && (capacity == 0 || !output.is_null())
            && distinct(&[
                region(job),
                region(written),
                region(error),
                (output as usize, capacity as usize)
            ])
    );
    if let Err(status) = unsafe { job_header(error) } {
        return status;
    }
    // Only valid, disjoint outputs are touched. A rejected copy exposes no prefix.
    unsafe { written.write(0) };
    unsafe {
        invoke(error, (*job).resources, || {
            let data = worker_data(job)?;
            if data.finished {
                return Err(EngineError::InvalidState);
            }
            let destination = if capacity == 0 {
                &mut []
            } else {
                std::slice::from_raw_parts_mut(output, capacity as usize)
            };
            let count = data
                .engine
                .as_ref()
                .ok_or(EngineError::InvalidState)?
                .copy_quality_trace(destination)?;
            written.write(count as u64);
            Ok(())
        })
    }
}
#[unsafe(no_mangle)]
pub unsafe extern "C" fn notepads_markdown_svg_job_finish(
    job: *mut SvgRendererJob,
    output: *mut SvgJobRetirement,
    error: *mut SvgJobError,
) -> i32 {
    preflight!(valid_job(job) && distinct(&[region(job), region(output), region(error)]));
    if let Err(s) = unsafe { job_header(output) } {
        return s;
    }
    unsafe {
        invoke(error, (*job).resources, || {
            let data = worker_data(job)?;
            if data.finished {
                return Err(EngineError::InvalidState);
            }
            let (terminal, ok, stages, kind, first, owners, releases) = if let Some(engine) =
                data.engine.as_mut()
            {
                let r = engine.finish_on_worker()?;
                (
                    r.terminal_state as u32,
                    r.cleanup_status == CleanupStatus::Succeeded,
                    r.completed_stage_bits,
                    data.initial_error
                        .or(r.first_job_error)
                        .map_or(0, error_kind),
                    r.first_fault,
                    r.remaining_resource_owners,
                    r.failed_release_count,
                )
            } else {
                let expected_tickets = data.baseline_tickets + u32::from(data.engine_fee.is_some());
                let expected_pending = data.baseline_pending + u32::from(data.engine_fee.is_some());
                let counts = (*job)
                    .host
                    .with_cleanup(true, |a, _| Ok((a.tickets, a.pending)))
                    .map_err(core_failure)?;
                let clean = counts == (expected_tickets, expected_pending)
                    && renderer_owners_clear((*job).resources, 3);
                (
                    JobState::Failed as u32,
                    clean,
                    0,
                    data.initial_error.map_or(0, error_kind),
                    renderer_fault((*job).resources),
                    u64::from(!clean),
                    0,
                )
            };
            data.finished = true;
            data.cleanup_ok = ok;
            output.write(SvgJobRetirement {
                header: SvgJobHeader::of::<SvgJobRetirement>(),
                identity: (*job).identity,
                terminal_state: terminal,
                cleanup_succeeded: u32::from(ok),
                completed_stages: stages,
                job_error_kind: kind,
                first_fault: fault_wire(first),
                remaining_owners: owners,
                failed_releases: releases,
                cancelled: u32::from((*job).cancel.token.is_cancelled()),
                reserved: 0,
            });
            Ok(())
        })
    }
}
#[unsafe(no_mangle)]
pub unsafe extern "C" fn notepads_markdown_svg_job_cancel_retain(cancel: *mut SvgJobCancel) -> i32 {
    if !aligned(cancel) || !distinct(&[region(cancel)]) {
        return STATUS_INVALID_ARGUMENT;
    }
    unsafe {
        let c = &*cancel;
        if c.closed.load(Ordering::Acquire) {
            return 5;
        }
        if c.refs
            .try_update(Ordering::AcqRel, Ordering::Acquire, |n| {
                if n == 0 || n == u32::MAX {
                    None
                } else {
                    Some(n + 1)
                }
            })
            .is_err()
        {
            return 20;
        }
        if c.closed.load(Ordering::Acquire) {
            c.refs.fetch_sub(1, Ordering::AcqRel);
            return 5;
        }
        0
    }
}
#[unsafe(no_mangle)]
pub unsafe extern "C" fn notepads_markdown_svg_job_cancel_request(
    cancel: *mut SvgJobCancel,
) -> i32 {
    if !aligned(cancel) || !distinct(&[region(cancel)]) {
        return STATUS_INVALID_ARGUMENT;
    }
    unsafe {
        if (*cancel).refs.load(Ordering::Acquire) < 2 {
            return 5;
        }
        (*cancel).token.cancel();
        0
    }
}
#[unsafe(no_mangle)]
pub unsafe extern "C" fn notepads_markdown_svg_job_cancel_release(
    cancel: *mut SvgJobCancel,
) -> i32 {
    if !aligned(cancel) || !distinct(&[region(cancel)]) {
        return STATUS_INVALID_ARGUMENT;
    }
    unsafe {
        if (*cancel)
            .refs
            .try_update(Ordering::AcqRel, Ordering::Acquire, |n| {
                if n > 1 { Some(n - 1) } else { None }
            })
            .is_ok()
        {
            0
        } else {
            5
        }
    }
}
#[unsafe(no_mangle)]
pub unsafe extern "C" fn notepads_markdown_svg_job_close_cancel(job: *mut SvgRendererJob) -> i32 {
    if !valid_job(job) {
        return STATUS_INVALID_ARGUMENT;
    }
    unsafe {
        (*job).cancel.closed.store(true, Ordering::Release);
    }
    0
}
#[unsafe(no_mangle)]
pub unsafe extern "C" fn notepads_markdown_svg_job_destroy(
    owner: *mut *mut SvgRendererJob,
    output: *mut SvgJobDestroyed,
    error: *mut SvgJobError,
) -> i32 {
    preflight!(aligned(owner) && distinct(&[region(owner), region(output), region(error)]));
    let job = unsafe { owner.read() };
    preflight!(
        valid_job(job) && distinct(&[region(job), region(owner), region(output), region(error)])
    );
    if let Err(s) = unsafe { job_header(output) } {
        return s;
    }
    let resources = unsafe { (*job).resources };
    let mut committed = false;
    let status = unsafe {
        invoke(error, resources, || {
            // The Native caller has already joined the real worker and revoked its
            // entry gate. Only this post-join path may access data off the worker.
            if !(*job).cancel.closed.load(Ordering::Acquire)
                || (*job).cancel.refs.load(Ordering::Acquire) != 1
            {
                return Err(EngineError::InvalidState);
            }
            let data = &*(*job).data.get();
            if !data.finished || !data.cleanup_ok {
                return Err(EngineError::InvalidState);
            }
            let mut receipt = SvgJobDestroyed {
                header: SvgJobHeader::of::<SvgJobDestroyed>(),
                identity: (*job).identity,
                consumed: 1,
                cleanup_succeeded: 0,
                cancelled: u32::from((*job).cancel.token.is_cancelled()),
                reserved: 0,
                first_fault: fault_wire(renderer_fault(resources)),
                unreleased_shell: (*job).shell_fee,
                unreleased_engine: data.engine_fee.unwrap_or_default(),
            };
            output.write(receipt);
            owner.write(ptr::null_mut());
            committed = true;
            let mut moved = job.read();
            std::alloc::dealloc(job.cast(), Layout::new::<SvgRendererJob>());
            let mut data = moved.data.into_inner();
            if injected(3) {
                panic!("injected renderer destruction after shell free");
            }
            if let Some(mut engine) = data.engine.take() {
                ManuallyDrop::drop(&mut engine);
            }
            drop(moved.cancel);
            let host = ManuallyDrop::take(&mut moved.host);
            if let Some(fee) = data.engine_fee {
                let released = if injected(4) {
                    Err(CoreError::InternalInvariant)
                } else {
                    host.with_cleanup(true, |a, _| a.release_after_free(fee))
                };
                if let Err(e) = released {
                    let fault = access::resource_error(e);
                    record_renderer_fault(resources, fault);
                    receipt.first_fault = fault_wire(renderer_fault(resources));
                    output.write(receipt);
                    return Err(EngineError::Failure(fault));
                }
                receipt.unreleased_engine = SvgWorkToken::default();
                output.write(receipt);
            }
            let released = if injected(5) {
                Err(CoreError::InternalInvariant)
            } else {
                host.with_cleanup(true, |a, _| a.release_after_free(moved.shell_fee))
            };
            if let Err(e) = released {
                let fault = access::resource_error(e);
                record_renderer_fault(resources, fault);
                receipt.first_fault = fault_wire(renderer_fault(resources));
                output.write(receipt);
                return Err(EngineError::Failure(fault));
            }
            receipt.unreleased_shell = SvgWorkToken::default();
            receipt.cleanup_succeeded = 1;
            output.write(receipt);
            drop(host);
            Ok(())
        })
    };
    if status == STATUS_PANIC && committed {
        unsafe {
            (*output).first_fault = fault_wire(renderer_fault(resources));
        }
    }
    status
}

#[cfg(test)]
thread_local! { static INJECT:std::cell::Cell<u8> = const {std::cell::Cell::new(0)}; }
fn injected(point: u8) -> bool {
    #[cfg(test)]
    {
        return INJECT.with(|s| {
            if s.get() == point {
                s.set(0);
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
    INJECT.with(|s| s.set(point));
}

#[cfg(test)]
#[path = "renderer_job_observer_tests.rs"]
mod observer_tests;
