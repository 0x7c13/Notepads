// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE file in the project root for license information.
// ---------------------------------------------------------------------------------------------

//! Shared requested-capacity admission for native derived objects.
//!
//! The host supplies one process authority to all controls. Cloning either
//! policy shares its existing ledger; a new document revision does not reset
//! outstanding jobs or results. These gauges are not physical memory or RSS.
//! Payload owners must free their buffers before releasing the matching grant.
//!
//! Policy headers are prepaid from a real source budget. As elsewhere in Core,
//! ordinary `Arc::new` retains the small initialization OOM boundary; the last
//! Arc allocation header can outlive its value's Drop by the final deallocation
//! instructions. There are no Weak owners or dynamically allocated job headers.

use crate::error::CoreError;
use crate::model::AcceptedModel;
use crate::policy::{BoundaryReservation, SourceBudget};
use std::fmt;
use std::sync::{Arc, Mutex, MutexGuard};

pub const JOB_BYTES: u64 = 64 * 1024 * 1024;
pub const MAX_RETAINED_BYTES: u64 = 32 * 1024 * 1024;
pub const MAX_PROCESS_BYTES: u64 = 256 * 1024 * 1024;
pub const MAX_CONTROL_BYTES: u64 = 128 * 1024 * 1024;
pub const MAX_METADATA_BYTES: u64 = 2 * 1024 * 1024;
pub const MAX_PROCESS_RESIDENTS: u32 = 256;
// Context-count ceiling for long documents; byte and concurrent-job budgets stay separate.
pub const MAX_CONTROL_RESIDENTS: u32 = 128;
/// The private FFI/native policy wrappers share this already prepaid header
/// allowance. They must assert their combined Layout fits; it is not a second
/// reservation or permission to allocate unbounded platform state.
pub const POLICY_WRAPPER_BYTES: u64 = 512;
const PROCESS_METADATA_BYTES: u64 = 1024;
const CONTROL_METADATA_BYTES: u64 = 1024;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DerivedLimit {
    ProcessBytes,
    ControlBytes,
    ProcessJobs,
    ControlJobs,
    ProcessResidents,
    ControlResidents,
    RetainedBytes,
    MetadataBytes,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DerivedError {
    Source(CoreError),
    InvalidSize,
    InvalidState,
    ArithmeticOverflow,
    Limit {
        kind: DerivedLimit,
        requested: u64,
        limit: u64,
    },
}

impl From<CoreError> for DerivedError {
    fn from(value: CoreError) -> Self {
        Self::Source(value)
    }
}

impl fmt::Display for DerivedError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{self:?}")
    }
}

impl std::error::Error for DerivedError {}

/// `bytes` includes payload reservations and metadata. `metadata_bytes` is a
/// subset of it; `retained_bytes` is a subset of payload reservations. Resident
/// contexts remain counted independently of their worker and payload states.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct Usage {
    pub bytes: u64,
    pub jobs: u32,
    pub residents: u32,
    pub retained_bytes: u64,
    pub metadata_bytes: u64,
}

#[derive(Clone, Copy)]
struct Limits {
    process_bytes: u64,
    control_bytes: u64,
    process_jobs: u32,
    control_jobs: u32,
    process_residents: u32,
    control_residents: u32,
    retained_bytes: u64,
    metadata_bytes: u64,
}

impl Limits {
    const PRODUCTION: Self = Self {
        process_bytes: MAX_PROCESS_BYTES,
        control_bytes: MAX_CONTROL_BYTES,
        process_jobs: 2,
        control_jobs: 1,
        process_residents: MAX_PROCESS_RESIDENTS,
        control_residents: MAX_CONTROL_RESIDENTS,
        retained_bytes: MAX_RETAINED_BYTES,
        metadata_bytes: MAX_METADATA_BYTES,
    };
}

struct ProcessShared {
    usage: Mutex<Usage>,
    limits: Limits,
    _metadata: BoundaryReservation,
}

struct ControlShared {
    // Always lock process before control; no client code runs under either.
    process: Arc<ProcessShared>,
    usage: Mutex<Usage>,
    _metadata: BoundaryReservation,
}

// Conservative fixed prepayment covers the value, Arc's two counters and
// alignment padding on all three supported pointer widths.
const _: () = assert!(
    size_of::<ProcessShared>()
        + 2 * size_of::<usize>()
        + 2 * align_of::<ProcessShared>()
        + POLICY_WRAPPER_BYTES as usize
        <= PROCESS_METADATA_BYTES as usize
);
const _: () = assert!(
    size_of::<ControlShared>()
        + 2 * size_of::<usize>()
        + 2 * align_of::<ControlShared>()
        + POLICY_WRAPPER_BYTES as usize
        <= CONTROL_METADATA_BYTES as usize
);

fn lock(usage: &Mutex<Usage>) -> MutexGuard<'_, Usage> {
    // Recovery lets existing RAII owners return their capacity after an
    // unrelated internal panic. There is no callback or fallible work in a
    // committed ledger mutation and no user code can access these locks.
    usage
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
}

fn add(current: u64, amount: u64) -> Result<u64, DerivedError> {
    current
        .checked_add(amount)
        .ok_or(DerivedError::ArithmeticOverflow)
}

fn check(kind: DerivedLimit, requested: u64, limit: u64) -> Result<(), DerivedError> {
    if requested > limit {
        Err(DerivedError::Limit {
            kind,
            requested,
            limit,
        })
    } else {
        Ok(())
    }
}

/// Explicit authority: the production host must share the same instance across
/// controls. Independent `new` calls intentionally create independent pools.
#[derive(Clone)]
pub struct ProcessPolicy {
    shared: Arc<ProcessShared>,
}

impl ProcessPolicy {
    pub fn new(metadata: &SourceBudget) -> Result<Self, DerivedError> {
        Self::create(metadata, Limits::PRODUCTION)
    }

    fn create(metadata: &SourceBudget, limits: Limits) -> Result<Self, DerivedError> {
        check(
            DerivedLimit::MetadataBytes,
            PROCESS_METADATA_BYTES,
            limits.metadata_bytes,
        )?;
        check(
            DerivedLimit::ProcessBytes,
            PROCESS_METADATA_BYTES,
            limits.process_bytes,
        )?;
        let reservation = metadata.reserve_resource_capacity(PROCESS_METADATA_BYTES)?;
        Ok(Self {
            shared: Arc::new(ProcessShared {
                usage: Mutex::new(Usage {
                    bytes: PROCESS_METADATA_BYTES,
                    metadata_bytes: PROCESS_METADATA_BYTES,
                    ..Usage::default()
                }),
                limits,
                _metadata: reservation,
            }),
        })
    }

    pub fn usage(&self) -> Usage {
        *lock(&self.shared.usage)
    }
}

impl fmt::Debug for ProcessPolicy {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("ProcessPolicy")
            .field("usage", &self.usage())
            .finish_non_exhaustive()
    }
}

/// Clones share one control, including its cross-revision retained total and
/// actual worker permit. The policy owns no AcceptedModel or source snapshot.
#[derive(Clone)]
pub struct ControlPolicy {
    shared: Arc<ControlShared>,
}

impl ControlPolicy {
    pub fn new(process: &ProcessPolicy, metadata: &SourceBudget) -> Result<Self, DerivedError> {
        let reservation = metadata.reserve_resource_capacity(CONTROL_METADATA_BYTES)?;
        let mut usage = lock(&process.shared.usage);
        let bytes = add(usage.bytes, CONTROL_METADATA_BYTES)?;
        let metadata_bytes = add(usage.metadata_bytes, CONTROL_METADATA_BYTES)?;
        check(
            DerivedLimit::ProcessBytes,
            bytes,
            process.shared.limits.process_bytes,
        )?;
        check(
            DerivedLimit::ControlBytes,
            CONTROL_METADATA_BYTES,
            process.shared.limits.control_bytes,
        )?;
        check(
            DerivedLimit::MetadataBytes,
            metadata_bytes,
            process.shared.limits.metadata_bytes,
        )?;
        let shared = Arc::new(ControlShared {
            process: process.shared.clone(),
            usage: Mutex::new(Usage {
                bytes: CONTROL_METADATA_BYTES,
                metadata_bytes: CONTROL_METADATA_BYTES,
                ..Usage::default()
            }),
            _metadata: reservation,
        });
        usage.bytes = bytes;
        usage.metadata_bytes = metadata_bytes;
        Ok(Self { shared })
    }

    /// Admit one resident context before allocating its metadata. Queued,
    /// running, ready, failed and old-frame contexts all keep this permit until
    /// the actual context has been freed. This reserves no bytes or worker
    /// slot and retains neither a model nor a source snapshot.
    pub fn reserve_resident(&self) -> Result<ResidentGrant, DerivedError> {
        let mut process = lock(&self.shared.process.usage);
        let mut control = lock(&self.shared.usage);
        let process_residents = process
            .residents
            .checked_add(1)
            .ok_or(DerivedError::ArithmeticOverflow)?;
        let control_residents = control
            .residents
            .checked_add(1)
            .ok_or(DerivedError::ArithmeticOverflow)?;
        let limits = self.shared.process.limits;
        check(
            DerivedLimit::ProcessResidents,
            u64::from(process_residents),
            u64::from(limits.process_residents),
        )?;
        check(
            DerivedLimit::ControlResidents,
            u64::from(control_residents),
            u64::from(limits.control_residents),
        )?;
        process.residents = process_residents;
        control.residents = control_residents;
        Ok(ResidentGrant {
            control: self.shared.clone(),
        })
    }

    /// Source funding comes only from the supplied accepted model's real root.
    /// This is capacity admission, not cancellation or publication permission.
    pub fn acquire_job(&self, model: &AcceptedModel) -> Result<JobGrant, DerivedError> {
        let source = model.budget().reserve_resource_capacity(JOB_BYTES)?;
        let mut process = lock(&self.shared.process.usage);
        let mut control = lock(&self.shared.usage);
        let process_bytes = add(process.bytes, JOB_BYTES)?;
        let control_bytes = add(control.bytes, JOB_BYTES)?;
        let process_jobs = process
            .jobs
            .checked_add(1)
            .ok_or(DerivedError::ArithmeticOverflow)?;
        let control_jobs = control
            .jobs
            .checked_add(1)
            .ok_or(DerivedError::ArithmeticOverflow)?;
        let limits = self.shared.process.limits;
        check(
            DerivedLimit::ProcessJobs,
            u64::from(process_jobs),
            u64::from(limits.process_jobs),
        )?;
        check(
            DerivedLimit::ControlJobs,
            u64::from(control_jobs),
            u64::from(limits.control_jobs),
        )?;
        check(
            DerivedLimit::ProcessBytes,
            process_bytes,
            limits.process_bytes,
        )?;
        check(
            DerivedLimit::ControlBytes,
            control_bytes,
            limits.control_bytes,
        )?;
        process.bytes = process_bytes;
        process.jobs = process_jobs;
        control.bytes = control_bytes;
        control.jobs = control_jobs;
        Ok(JobGrant {
            control: self.shared.clone(),
            source: Some(source),
            bytes: JOB_BYTES,
        })
    }

    /// Prepay an unstarted record or shared queue capacity from its captured
    /// model's real root. Headers and all such records share the process's
    /// persistent metadata limit. This acquires neither a worker permit nor a
    /// retained-result allowance; per-job metadata remains inside JOB_BYTES.
    ///
    /// Reserve before allocating the complete capacity, including its owner
    /// and alignment. On allocation failure free the actual partial allocation
    /// before dropping this grant. Admission does not authorize publication or
    /// replace the caller's cancellation/deadline checks.
    pub fn reserve_metadata(
        &self,
        model: &AcceptedModel,
        bytes: u64,
    ) -> Result<MetadataGrant, DerivedError> {
        if bytes == 0 || bytes > MAX_METADATA_BYTES {
            return Err(DerivedError::InvalidSize);
        }
        let source = model.budget().reserve_resource_capacity(bytes)?;
        let mut process = lock(&self.shared.process.usage);
        let mut control = lock(&self.shared.usage);
        let process_bytes = add(process.bytes, bytes)?;
        let control_bytes = add(control.bytes, bytes)?;
        let process_metadata = add(process.metadata_bytes, bytes)?;
        let control_metadata = add(control.metadata_bytes, bytes)?;
        let limits = self.shared.process.limits;
        check(
            DerivedLimit::ProcessBytes,
            process_bytes,
            limits.process_bytes,
        )?;
        check(
            DerivedLimit::ControlBytes,
            control_bytes,
            limits.control_bytes,
        )?;
        check(
            DerivedLimit::MetadataBytes,
            process_metadata,
            limits.metadata_bytes,
        )?;
        check(
            DerivedLimit::MetadataBytes,
            control_metadata,
            limits.metadata_bytes,
        )?;
        process.bytes = process_bytes;
        process.metadata_bytes = process_metadata;
        control.bytes = control_bytes;
        control.metadata_bytes = control_metadata;
        Ok(MetadataGrant {
            control: self.shared.clone(),
            source,
        })
    }

    pub fn usage(&self) -> Usage {
        *lock(&self.shared.usage)
    }
}

impl fmt::Debug for ControlPolicy {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("ControlPolicy")
            .field("usage", &self.usage())
            .finish_non_exhaustive()
    }
}

impl Drop for ControlShared {
    fn drop(&mut self) {
        // No grants survive: each would still own a strong Arc to this value.
        let mut process = lock(&self.process.usage);
        process.bytes -= CONTROL_METADATA_BYTES;
        process.metadata_bytes -= CONTROL_METADATA_BYTES;
    }
}

/// Nonclone permit for one real context, independent of its rendering state.
/// The owner must free that context before dropping this permit. Only the
/// existing policy Arc is retained; admission allocates no additional header.
#[must_use]
pub struct ResidentGrant {
    control: Arc<ControlShared>,
}

impl fmt::Debug for ResidentGrant {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("ResidentGrant")
            .finish_non_exhaustive()
    }
}

impl Drop for ResidentGrant {
    fn drop(&mut self) {
        let mut process = lock(&self.control.process.usage);
        let mut control = lock(&self.control.usage);
        process.residents -= 1;
        control.residents -= 1;
    }
}

/// Nonclone reservation for an external record or capacity owner. The owner
/// frees the actual allocation first, then drops this grant. It holds only the
/// quota authority, so replacing/dropping its model cannot retire the charge.
#[must_use]
pub struct MetadataGrant {
    control: Arc<ControlShared>,
    source: BoundaryReservation,
}

impl MetadataGrant {
    pub fn reserved_bytes(&self) -> u64 {
        self.source.reserved_bytes()
    }
}

impl fmt::Debug for MetadataGrant {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("MetadataGrant")
            .field("reserved_bytes", &self.reserved_bytes())
            .finish_non_exhaustive()
    }
}

impl Drop for MetadataGrant {
    fn drop(&mut self) {
        let bytes = self.reserved_bytes();
        let mut process = lock(&self.control.process.usage);
        let mut control = lock(&self.control.usage);
        process.bytes -= bytes;
        process.metadata_bytes -= bytes;
        control.bytes -= bytes;
        control.metadata_bytes -= bytes;
        // The original source reservation releases after these locks. No
        // model, source generation, worker or cancellation callback is needed.
    }
}

/// Keep this scope alive until the real worker has stopped and all transient
/// buffers are gone. A successful split transfers bytes, never this permit.
#[must_use]
pub struct JobGrant {
    control: Arc<ControlShared>,
    source: Option<BoundaryReservation>,
    bytes: u64,
}

impl JobGrant {
    pub fn reserved_bytes(&self) -> u64 {
        self.bytes
    }

    /// The caller has already freed transient buffers. On failure the entire
    /// 64 MiB reservation and worker permit remain unchanged. On success actual
    /// result buffers must be freed before dropping the returned grant.
    pub fn retain_result(&mut self, actual: u64) -> Result<RetainedGrant, DerivedError> {
        let source = self.source.as_mut().ok_or(DerivedError::InvalidState)?;
        if actual == 0 {
            return Err(DerivedError::InvalidSize);
        }
        if actual > MAX_RETAINED_BYTES || actual > self.bytes {
            return Err(DerivedError::InvalidSize);
        }
        let mut process = lock(&self.control.process.usage);
        let mut control = lock(&self.control.usage);
        let process_retained = add(process.retained_bytes, actual)?;
        let control_retained = add(control.retained_bytes, actual)?;
        check(
            DerivedLimit::RetainedBytes,
            control_retained,
            self.control.process.limits.retained_bytes,
        )?;
        let reduction = self.bytes - actual;
        let process_bytes = process
            .bytes
            .checked_sub(reduction)
            .ok_or(DerivedError::ArithmeticOverflow)?;
        let control_bytes = control
            .bytes
            .checked_sub(reduction)
            .ok_or(DerivedError::ArithmeticOverflow)?;
        // This cannot increase or allocate. Keep the old policies conservative
        // until the existing source reservation has successfully shrunk.
        source.shrink_to(actual)?;
        process.bytes = process_bytes;
        process.retained_bytes = process_retained;
        control.bytes = control_bytes;
        control.retained_bytes = control_retained;
        let result = RetainedGrant {
            control: self.control.clone(),
            _source: self.source.take(),
            bytes: actual,
        };
        self.bytes = 0;
        Ok(result)
    }
}

impl fmt::Debug for JobGrant {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("JobGrant")
            .field("reserved_bytes", &self.reserved_bytes())
            .finish_non_exhaustive()
    }
}

impl Drop for JobGrant {
    fn drop(&mut self) {
        let mut process = lock(&self.control.process.usage);
        let mut control = lock(&self.control.usage);
        process.bytes -= self.bytes;
        control.bytes -= self.bytes;
        process.jobs -= 1;
        control.jobs -= 1;
        // The source reservation field drops after these guards, without
        // checking a retired model, cancellation flag or worker context.
    }
}

/// A nonclone capacity lease. An upper layer may share its complete payload
/// owner using Arc; the last payload owner must free buffers before this field.
#[must_use]
pub struct RetainedGrant {
    control: Arc<ControlShared>,
    // Option permits moving the exact existing reservation from JobGrant,
    // without a replacement allocation, release/reacquire gap or unwrap.
    _source: Option<BoundaryReservation>,
    bytes: u64,
}

impl RetainedGrant {
    pub fn reserved_bytes(&self) -> u64 {
        self.bytes
    }
}

impl fmt::Debug for RetainedGrant {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("RetainedGrant")
            .field("reserved_bytes", &self.reserved_bytes())
            .finish_non_exhaustive()
    }
}

impl Drop for RetainedGrant {
    fn drop(&mut self) {
        let mut process = lock(&self.control.process.usage);
        let mut control = lock(&self.control.usage);
        process.bytes -= self.bytes;
        control.bytes -= self.bytes;
        process.retained_bytes -= self.bytes;
        control.retained_bytes -= self.bytes;
        // The owned source lease releases after these ledger locks leave this
        // scope. Its drop needs no source/model/cancellation callback.
    }
}

#[cfg(test)]
mod tests;
