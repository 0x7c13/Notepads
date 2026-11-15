// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE file in the project root for license information.
// ---------------------------------------------------------------------------------------------
//! One worker, one prepare, one frozen target, one caller-owned raster.
use crate::{
    abi,
    adapter::Adapter,
    memory::{Context, Host},
};
use markdown_core::resources::svg_size::{SvgNaturalSize, SvgRasterPlan};
use markdown_svg_budget::{Error, FailureKind, LimitKind, Limits, ResourcesRef, Statistics};
use std::{
    mem::ManuallyDrop,
    panic::{AssertUnwindSafe, catch_unwind},
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    thread::ThreadId,
};
use usvg::FontReplay;

/// Exact selected-toolchain Layout sizes, excluding the Host owner and its
/// canonical task. The ticket Vec is independently paid by its actual lease.
#[derive(Clone, Copy, Debug)]
pub struct BootstrapMetadata {
    pub shell_bytes: usize,
    pub shell_alignment: usize,
    pub cancel_arc_bytes: usize,
    pub diagnostics_arc_bytes: usize,
    pub resource_arc_bytes: usize,
    pub backend_box_bytes: usize,
    pub first_failure_arc_bytes: usize,
    pub adapter_arc_bytes: usize,
    pub retention_arc_bytes: usize,
    pub processing_arc_bytes: usize,
    pub font_database_arc_bytes: usize,
    pub font_database_sentinel_bytes: usize,
    pub database_family_bytes: [usize; 5],
    pub options_family_bytes: usize,
    pub languages_vector_bytes: usize,
    pub language_bytes: usize,
}
pub fn bootstrap_metadata() -> BootstrapMetadata {
    let resource =
        markdown_svg_budget::metadata_sizes::<crate::resource_backend::NativeBudgetBackend>();
    BootstrapMetadata {
        shell_bytes: size_of::<OwnedJob>(),
        shell_alignment: align_of::<OwnedJob>(),
        cancel_arc_bytes: crate::memory::arc_layout::<AtomicBool>().size(),
        diagnostics_arc_bytes: crate::memory::diagnostics_layout().size(),
        resource_arc_bytes: resource.resource_arc_bytes,
        backend_box_bytes: resource.backend_box_bytes,
        first_failure_arc_bytes: resource.failure_arc_bytes,
        adapter_arc_bytes: crate::memory::arc_layout::<Adapter>().size(),
        retention_arc_bytes: crate::memory::arc_layout::<crate::memory::RetainedPool>().size(),
        processing_arc_bytes: crate::memory::arc_layout::<usvg::ProcessingContext>().size(),
        font_database_arc_bytes: crate::memory::arc_layout::<fontdb::Database>().size(),
        font_database_sentinel_bytes: fontdb::Database::bootstrap_sentinel_layout().size(),
        database_family_bytes: [15, 5, 13, 6, 11],
        options_family_bytes: 15,
        languages_vector_bytes: size_of::<String>(),
        language_bytes: 2,
    }
}
impl BootstrapMetadata {
    pub fn total_bytes(self) -> Option<usize> {
        [
            self.shell_bytes,
            self.cancel_arc_bytes,
            self.diagnostics_arc_bytes,
            self.resource_arc_bytes,
            self.backend_box_bytes,
            self.first_failure_arc_bytes,
            self.adapter_arc_bytes,
            self.retention_arc_bytes,
            self.processing_arc_bytes,
            self.font_database_arc_bytes,
            self.font_database_sentinel_bytes,
            self.options_family_bytes,
            self.languages_vector_bytes,
            self.language_bytes,
        ]
        .into_iter()
        .chain(self.database_family_bytes)
        .try_fold(0usize, usize::checked_add)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u32)]
pub enum JobState {
    New,
    Preparing,
    Prepared,
    Targeted,
    Running,
    Rendered,
    Failed,
    EngineFinished,
    RetirementFailed,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Identity {
    pub job_nonce: u64,
    pub source_generation: u64,
    pub provider_generation: u64,
    pub font_generation: u64,
    pub target_generation: u64,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct NaturalSize {
    pub width_dip: f64,
    pub height_dip: f64,
    pub viewport_width_bits: u32,
    pub viewport_height_bits: u32,
}
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Target {
    pub display_width_dip: f64,
    pub effective_physical_scale: f64,
    pub generation: u64,
}

/// This entire value is immutable after set_target. A caller returns only its
/// identity; run consumes the stored geometry and never re-derives a scale.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct RasterPlan {
    pub identity: Identity,
    pub natural: NaturalSize,
    pub requested_bucket: f64,
    pub target_bucket: u32,
    pub ideal_scale: f64,
    pub scale_bits: u32,
    pub extent_width: f64,
    pub extent_height: f64,
    pub width: u32,
    pub height: u32,
    pub stride: u32,
    pub capacity: usize,
    pub resolution_limited: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum JobError {
    InvalidState,
    InvalidArgument,
    GeometryUnrepresentable,
    Failure(Error),
    Panic,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CleanupStatus {
    Succeeded,
    Failed,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Rendered {
    pub identity: Identity,
    pub pixel_bytes: usize,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Retirement {
    pub identity: Identity,
    pub terminal_state: JobState,
    pub first_fault: Option<Error>,
    pub first_job_error: Option<JobError>,
    pub cleanup_status: CleanupStatus,
    pub completed_stage_bits: u32,
    pub remaining_resource_owners: u64,
    pub failed_release_count: u64,
}
#[derive(Clone, Copy, Debug)]
pub struct JobStats {
    pub state: JobState,
    pub first_fault: Option<Error>,
    pub first_job_error: Option<JobError>,
    pub resources: Option<Statistics>,
    pub created_owners: [u64; 3],
    pub retired_owners: [u64; 3],
    pub failed_release_count: u64,
    pub drafts: u64,
    pub glyphs: u64,
    pub target: Option<RasterPlan>,
}

/// Clone this value before sending work to the worker. Cancellation never
/// borrows JobWork, enters an account or calls a font capability.
#[derive(Clone)]
pub struct CancelHandle(Arc<AtomicBool>);
impl CancelHandle {
    pub fn cancel(&self) {
        self.0.store(true, Ordering::Release);
    }
    pub fn is_cancelled(&self) -> bool {
        self.0.load(Ordering::Acquire)
    }
}

struct JobWork {
    context: Context,
    resources: Option<ResourcesRef>,
    tree: Option<usvg::Tree>,
    options: Option<usvg::Options<'static>>,
    adapter: Option<Arc<Adapter>>,
}

/// Drop is deliberately fail-closed. Without a successful finish receipt the
/// work slots and their account capabilities remain quarantined, including if
/// an embedding caller accidentally drops this value on a reaper thread.
pub struct OwnedJob {
    identity: Identity,
    worker: ThreadId,
    cancel: CancelHandle,
    state: JobState,
    terminal_state: JobState,
    natural: Option<NaturalSize>,
    plan: Option<RasterPlan>,
    display_width_dip: f64,
    drafts: u64,
    glyphs: u64,
    retirement: Option<Retirement>,
    first_job_error: Option<JobError>,
    #[cfg(feature = "quality-observation")]
    quality_enabled: bool,
    work: ManuallyDrop<JobWork>,
}

impl OwnedJob {
    /// The embedding owner reserves the exact bootstrap batch before this call.
    /// After construction and the real shell allocation succeed, it admits the
    /// actual batch. Payment remains until the shell and last cancel Arc free.
    /// Font callbacks and Host refer to the same live canonical task/account.
    pub fn new(
        identity: Identity,
        fonts: abi::Callbacks,
        host: Arc<dyn Host>,
        limits: Limits,
    ) -> Result<Self, JobError> {
        Self::new_inner(identity, fonts, host, limits, None)
    }
    /// Uses the canonical resources fault owner; its allocation is already paid.
    /// The caller subtracts first_failure_arc_bytes from the engine batch and
    /// keeps that canonical owner through worker retirement and shell free.
    pub fn new_with_first_failure(
        identity: Identity,
        fonts: abi::Callbacks,
        host: Arc<dyn Host>,
        limits: Limits,
        first_failure: markdown_svg_budget::FirstFailureRef,
    ) -> Result<Self, JobError> {
        Self::new_inner(identity, fonts, host, limits, Some(first_failure))
    }
    fn new_inner(
        identity: Identity,
        fonts: abi::Callbacks,
        host: Arc<dyn Host>,
        limits: Limits,
        first_failure: Option<markdown_svg_budget::FirstFailureRef>,
    ) -> Result<Self, JobError> {
        if identity.job_nonce == 0 {
            return Err(JobError::InvalidArgument);
        }
        fonts.validate().map_err(|_| JobError::InvalidArgument)?;
        let cancel = CancelHandle(Arc::new(AtomicBool::new(false)));
        let context = match first_failure {
            Some(fault) => Context::new_with_first_failure(fonts, host, cancel.0.clone(), fault),
            None => Context::new(fonts, host, cancel.0.clone()),
        }
        .map_err(|_| JobError::InvalidArgument)?;
        let resources = context.make_resources(limits);
        let adapter = Arc::new(Adapter::new(&context, &resources).map_err(|_| {
            JobError::Failure(
                context
                    .resource_failure()
                    .unwrap_or(Error::new(FailureKind::Invariant, LimitKind::None)),
            )
        })?);
        let processing = Arc::new(usvg::ProcessingContext::with_resources(resources.clone()));
        let mut options = usvg::Options::with_resources_and_fontdb(
            processing,
            adapter.fontdb().map_err(|_| {
                JobError::Failure(
                    context
                        .resource_failure()
                        .unwrap_or(Error::new(FailureKind::Invariant, LimitKind::None)),
                )
            })?,
            "Times New Roman".to_owned(),
            vec!["en".to_owned()],
        );
        options.font_replay = Some(adapter.clone());
        Ok(Self {
            identity,
            worker: std::thread::current().id(),
            cancel,
            state: JobState::New,
            terminal_state: JobState::New,
            natural: None,
            plan: None,
            display_width_dip: 0.0,
            drafts: 0,
            glyphs: 0,
            retirement: None,
            first_job_error: None,
            #[cfg(feature = "quality-observation")]
            quality_enabled: false,
            work: ManuallyDrop::new(JobWork {
                context,
                resources: Some(resources),
                tree: None,
                options: Some(options),
                adapter: Some(adapter),
            }),
        })
    }
    pub fn cancellation(&self) -> CancelHandle {
        self.cancel.clone()
    }
    pub fn state(&self) -> JobState {
        self.state
    }
    #[cfg(feature = "quality-observation")]
    pub fn enable_quality_observation(&mut self, capacity: usize) -> Result<(), JobError> {
        self.require(JobState::New)?;
        if self.quality_enabled {
            return Err(JobError::InvalidState);
        }
        if !(1..=8 * 1024 * 1024).contains(&capacity) {
            return Err(JobError::InvalidArgument);
        }
        let result = self
            .work
            .adapter
            .as_ref()
            .ok_or(JobError::InvalidState)?
            .enable_quality_observation(capacity);
        match result {
            Ok(()) => {
                self.quality_enabled = true;
                Ok(())
            }
            Err(_) => {
                let fault = self
                    .work
                    .context
                    .resource_failure()
                    .unwrap_or(Error::new(FailureKind::Invariant, LimitKind::None));
                Err(self.failure(JobError::Failure(fault)))
            }
        }
    }
    #[cfg(feature = "quality-observation")]
    fn quality_access(&self) -> Result<&Adapter, JobError> {
        if self.worker != std::thread::current().id()
            || !self.quality_enabled
            || matches!(
                self.state,
                JobState::Preparing
                    | JobState::Running
                    | JobState::EngineFinished
                    | JobState::RetirementFailed
            )
        {
            return Err(JobError::InvalidState);
        }
        self.work.adapter.as_deref().ok_or(JobError::InvalidState)
    }
    #[cfg(feature = "quality-observation")]
    pub fn quality_trace_info(&self) -> Result<crate::QualityTraceInfo, JobError> {
        self.quality_access()?
            .quality_trace_info()
            .map_err(|_| JobError::InvalidState)
    }
    #[cfg(feature = "quality-observation")]
    pub fn copy_quality_trace(&self, destination: &mut [u8]) -> Result<usize, JobError> {
        let owner = self.quality_access()?;
        let required = owner
            .quality_trace_info()
            .map_err(|_| JobError::InvalidState)?
            .bytes;
        if required > destination.len() as u64 {
            return Err(JobError::InvalidArgument);
        }
        owner
            .copy_quality_trace(destination)
            .map_err(|_| JobError::InvalidState)
    }
    fn require(&self, state: JobState) -> Result<(), JobError> {
        if self.worker != std::thread::current().id() || self.state != state {
            Err(JobError::InvalidState)
        } else {
            Ok(())
        }
    }
    fn failure(&mut self, error: JobError) -> JobError {
        self.state = JobState::Failed;
        if self.first_job_error.is_none() {
            self.first_job_error = Some(error);
        }
        match error {
            JobError::Failure(e) => JobError::Failure(self.work.context.record_resource(e)),
            JobError::InvalidState => error,
            JobError::GeometryUnrepresentable => {
                self.work
                    .context
                    .record_resource(Error::new(FailureKind::Unsupported, LimitKind::Capacity));
                error
            }
            other => {
                self.work
                    .context
                    .record_resource(Error::new(FailureKind::Invariant, LimitKind::None));
                other
            }
        }
    }
    pub fn prepare(&mut self, source: &[u8]) -> Result<NaturalSize, JobError> {
        self.require(JobState::New)?;
        self.state = JobState::Preparing;
        let outcome = catch_unwind(AssertUnwindSafe(|| self.prepare_inner(source)));
        match outcome {
            Ok(Ok(size)) => {
                if let Err(error) = self.work.resources.as_ref().unwrap().checkpoint(0) {
                    // No failed candidate may remain installed after a
                    // temporary's last release faults during prepare exit.
                    let candidate = self.work.tree.take();
                    drop(candidate);
                    return Err(self.failure(JobError::Failure(error)));
                }
                self.natural = Some(size);
                self.state = JobState::Prepared;
                Ok(size)
            }
            Ok(Err(error)) => Err(self.failure(error)),
            Err(payload) => {
                std::mem::forget(payload);
                Err(self.failure(JobError::Panic))
            }
        }
    }
    fn prepare_inner(&mut self, source: &[u8]) -> Result<NaturalSize, JobError> {
        let work = &mut *self.work;
        let error = |_: abi::Failure| {
            JobError::Failure(
                work.context
                    .resource_failure()
                    .unwrap_or(Error::new(FailureKind::Invariant, LimitKind::None)),
            )
        };
        work.context.checkpoint(1).map_err(error)?;
        let owner = work.adapter.as_ref().unwrap();
        let tree = usvg::Tree::from_data_controlled(source, work.options.as_ref().unwrap())
            .map_err(|error| {
                let typed = work
                    .context
                    .resource_failure()
                    .unwrap_or_else(|| match error {
                        usvg::Error::Resource(e) => e,
                        usvg::Error::ProcessingFailure(e) => e.typed_failure,
                        _ => Error::new(FailureKind::Decode, LimitKind::None),
                    });
                JobError::Failure(work.context.record_resource(typed))
            })?;
        // The same root parse computed natural DIP and used this exact f32
        // viewport. They have separate meanings and are never substituted.
        let (width_dip, height_dip) = tree.natural_size();
        let vw = tree.size().width();
        let vh = tree.size().height();
        if SvgNaturalSize::new(width_dip, height_dip).is_err() || !positive(vw) || !positive(vh) {
            return Err(JobError::GeometryUnrepresentable);
        }
        let size = NaturalSize {
            width_dip,
            height_dip,
            viewport_width_bits: vw.to_bits(),
            viewport_height_bits: vh.to_bits(),
        };
        let (drafts, glyphs) = owner.counts().map_err(error)?;
        self.drafts = drafts;
        self.glyphs = glyphs;
        work.context.checkpoint(1).map_err(error)?;
        work.resources
            .as_ref()
            .unwrap()
            .checkpoint(0)
            .map_err(JobError::Failure)?;
        work.tree = Some(tree);
        Ok(size)
    }
    pub fn set_target(&mut self, target: Target) -> Result<RasterPlan, JobError> {
        self.require(JobState::Prepared)?;
        if let Err(error) = self.work.resources.as_ref().unwrap().checkpoint(0) {
            return Err(self.failure(JobError::Failure(error)));
        }
        if target.generation != self.identity.target_generation {
            return Err(self.failure(JobError::InvalidArgument));
        }
        if let Err(_) = self.work.context.checkpoint(1) {
            return Err(self.failure(JobError::Failure(
                self.work.context.resource_failure().unwrap(),
            )));
        }
        match freeze_plan(self.identity, self.natural.unwrap(), target) {
            Ok(plan) => {
                self.plan = Some(plan);
                self.display_width_dip = target.display_width_dip;
                self.state = JobState::Targeted;
                Ok(plan)
            }
            Err(error) => Err(self.failure(error)),
        }
    }
    pub fn run_into(
        &mut self,
        identity: Identity,
        pixels: &mut [u8],
        stride: u32,
    ) -> Result<Rendered, JobError> {
        self.require(JobState::Targeted)?;
        let plan = self.plan.unwrap();
        if identity != plan.identity || stride != plan.stride || pixels.len() != plan.capacity {
            return Err(self.failure(JobError::InvalidArgument));
        }
        self.state = JobState::Running;
        let display_width_dip = self.display_width_dip;
        let outcome = catch_unwind(AssertUnwindSafe(|| {
            let work = &mut *self.work;
            let failure =
                |_: abi::Failure| JobError::Failure(work.context.resource_failure().unwrap());
            for chunk in pixels.chunks_mut(4096 * 4) {
                work.context
                    .checkpoint(chunk.len() as u64)
                    .map_err(failure)?;
                chunk.fill(0);
            }
            {
                let mut pixmap = tiny_skia::PixmapMut::from_bytes(pixels, plan.width, plan.height)
                    .ok_or(JobError::InvalidArgument)?;
                let scale = f32::from_bits(plan.scale_bits);
                let tree = work.tree.as_ref().unwrap();
                let coverage = work.context.coverage_provider().map(|provider| {
                    resvg::CoverageConfig::new(
                        provider,
                        display_width_dip,
                        tree.size().width(),
                        scale,
                    )
                });
                resvg::render_in_request_with_coverage(
                    tree,
                    tiny_skia::Transform::from_scale(scale, scale),
                    &mut pixmap,
                    coverage,
                )
                .map_err(|e| JobError::Failure(work.context.record_resource(e.typed_failure)))?;
            }
            if let Some(error) = work.context.resource_failure() {
                return Err(JobError::Failure(error));
            }
            for chunk in pixels.chunks_mut(4096 * 4) {
                work.context
                    .checkpoint((chunk.len() / 4) as u64)
                    .map_err(failure)?;
                for pixel in chunk.chunks_exact_mut(4) {
                    pixel.swap(0, 2);
                }
            }
            work.context.checkpoint(1).map_err(failure)?;
            work.resources
                .as_ref()
                .unwrap()
                .checkpoint(0)
                .map_err(JobError::Failure)?;
            Ok(Rendered {
                identity: plan.identity,
                pixel_bytes: plan.capacity,
            })
        }));
        match outcome {
            Ok(Ok(value)) => {
                self.state = JobState::Rendered;
                Ok(value)
            }
            Ok(Err(error)) => Err(self.failure(error)),
            Err(payload) => {
                std::mem::forget(payload);
                Err(self.failure(JobError::Panic))
            }
        }
    }
    pub fn stats(&self) -> Result<JobStats, JobError> {
        if self.worker != std::thread::current().id() {
            return Err(JobError::InvalidState);
        }
        let (created, retired) = self.work.context.owner_counts();
        Ok(JobStats {
            state: self.state,
            first_fault: self.work.context.resource_failure(),
            first_job_error: self.first_job_error,
            resources: self.work.resources.as_ref().map(ResourcesRef::statistics),
            created_owners: created,
            retired_owners: retired,
            failed_release_count: self.work.context.release_failures(),
            drafts: self.drafts,
            glyphs: self.glyphs,
            target: self.plan,
        })
    }
    /// Receipt transport success includes cleanup failure. A failed receipt is
    /// permanent quarantine; it never authorizes candidate publication.
    pub fn finish_on_worker(&mut self) -> Result<Retirement, JobError> {
        if self.worker != std::thread::current().id()
            || matches!(
                self.state,
                JobState::Preparing
                    | JobState::Running
                    | JobState::EngineFinished
                    | JobState::RetirementFailed
            )
        {
            return Err(JobError::InvalidState);
        }
        self.terminal_state = self.state;
        let work = &mut *self.work;
        let mut bits = 0;
        let mut clean = true;
        // Each slot is taken before its destructor. A panic can never cause a
        // later finish, Drop or reaper to call that destructor again.
        macro_rules! retire {
            ($slot:expr, $bit:expr) => {{
                let owner = $slot.take();
                match catch_unwind(AssertUnwindSafe(|| drop(owner))) {
                    Ok(()) => bits |= $bit,
                    Err(payload) => {
                        std::mem::forget(payload);
                        clean = false;
                    }
                }
            }};
        }
        retire!(work.tree, 1);
        retire!(work.options, 2);
        retire!(work.adapter, 4);
        let remaining = work
            .resources
            .as_ref()
            .map_or(0, |r| r.statistics().live_leases);
        let resource_release_failures = work
            .resources
            .as_ref()
            .map_or(0, |r| r.statistics().release_failures);
        if work
            .resources
            .as_ref()
            .is_some_and(|r| r.validate_retired().is_err())
        {
            clean = false;
        }
        retire!(work.resources, 8);
        if work.context.validate_released().is_err()
            || work.context.validate_scope_retired().is_err()
        {
            clean = false;
        }
        if clean {
            bits |= 16;
        }
        let receipt = Retirement {
            identity: self.identity,
            terminal_state: self.terminal_state,
            first_fault: work.context.resource_failure(),
            first_job_error: self.first_job_error,
            cleanup_status: if clean {
                CleanupStatus::Succeeded
            } else {
                CleanupStatus::Failed
            },
            completed_stage_bits: bits,
            remaining_resource_owners: remaining,
            failed_release_count: work
                .context
                .release_failures()
                .saturating_add(resource_release_failures),
        };
        self.state = if clean {
            JobState::EngineFinished
        } else {
            JobState::RetirementFailed
        };
        self.retirement = Some(receipt);
        Ok(receipt)
    }
    /// The embedding reaper additionally proves native font retirement,
    /// signaled worker, revoked entry points and zero cancel call leases.
    pub fn can_destroy_shell(&self) -> bool {
        self.state == JobState::EngineFinished
    }
}
impl Drop for OwnedJob {
    fn drop(&mut self) {
        if self.state == JobState::EngineFinished {
            // SAFETY: successful worker retirement removed every thread-bound
            // owner. Remaining Context/Arc/atomics are thread independent.
            unsafe {
                ManuallyDrop::drop(&mut self.work);
            }
        }
    }
}

fn positive(value: f32) -> bool {
    value.is_finite() && value > 0.0
}

fn freeze_plan(
    identity: Identity,
    natural: NaturalSize,
    target: Target,
) -> Result<RasterPlan, JobError> {
    let fail = JobError::GeometryUnrepresentable;
    let n = SvgNaturalSize::new(natural.width_dip, natural.height_dip).map_err(|_| fail)?;
    let core =
        SvgRasterPlan::for_display(n, target.display_width_dip, target.effective_physical_scale)
            .map_err(|_| fail)?;
    let frozen = core
        .freeze_renderer(natural.viewport_width_bits, natural.viewport_height_bits)
        .map_err(|_| fail)?;
    let capacity = usize::try_from(frozen.capacity)
        .ok()
        .filter(|&n| n <= isize::MAX as usize)
        .ok_or(fail)?;
    Ok(RasterPlan {
        identity,
        natural,
        requested_bucket: core.requested_bucket_pixels,
        target_bucket: core.target_bucket,
        ideal_scale: core.uniform_scale,
        scale_bits: frozen.scale_bits,
        extent_width: frozen.extent_width,
        extent_height: frozen.extent_height,
        width: frozen.width,
        height: frozen.height,
        stride: frozen.stride,
        capacity,
        resolution_limited: core.resolution_limited,
    })
}

#[cfg(test)]
#[path = "job_tests.rs"]
mod tests;
