// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE file in the project root for license information.
// ---------------------------------------------------------------------------------------------

//! One finite root policy. Reservation gauges are not allocation-identity peaks.

use crate::error::{CoreError, LimitKind};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};

mod construction;
mod elapsed;
mod leaf;
#[cfg(test)]
mod task;
pub(crate) use construction::{
    ConstructionCharges, ConstructionId, NativeLease, OwnedWork, construction_scope,
};
use elapsed::ElapsedState;
pub(crate) use leaf::{ScalarBatchEnd, ScalarChargeOrder};
#[cfg(test)]
pub(crate) use task::TaskOperation;

#[cfg(test)]
mod bootstrap_tests;
#[cfg(test)]
mod collection_tests;
#[cfg(test)]
mod leaf_tests;
#[cfg(test)]
mod preview_tests;
#[cfg(test)]
mod task_tests;

pub const CHECKPOINT_BYTES: u64 = 65_536;
pub const CHECKPOINT_EVENTS: u64 = 256;
pub(crate) const MAX_METADATA_VIEW_BYTES: u64 = 512;
const ELAPSED_LIMIT_NS: u64 = 500_000_000;

/// Immutable diagnostic collection choice; it never selects a protection rule.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum WorkStatsCollection {
    #[default]
    Enabled,
    Disabled,
}

/// Production task access and deterministic cumulative work fixtures.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum ResourceAdmissionMode {
    CumulativeWork,
    #[default]
    TaskAccess,
}

impl ResourceAdmissionMode {
    pub fn label(self) -> &'static str {
        match self {
            Self::CumulativeWork => "cumulative-work",
            Self::TaskAccess => "task-access",
        }
    }
}

#[derive(Clone, Copy, Debug)]
pub struct SourceLimits {
    pub max_source_bytes: u64,
    pub retained_source_capacity: u64,
    pub retained_source_owners: u64,
    pub max_parse_bytes: u64,
    pub max_input_pieces: u64,
    pub max_edits: u64,
    pub max_insert_bytes: u64,
    pub auxiliary_capacity: u64,
    pub max_work: u64,
}

impl Default for SourceLimits {
    fn default() -> Self {
        Self {
            max_source_bytes: 16_777_216,
            retained_source_capacity: 67_108_864,
            retained_source_owners: 64,
            max_parse_bytes: 50_331_648,
            max_input_pieces: 1_048_576,
            max_edits: 65_536,
            max_insert_bytes: 16_777_216,
            auxiliary_capacity: 134_217_728,
            max_work: 268_435_456,
        }
    }
}

/// Model resource ceilings. Source-only fixtures do not enable model admission.
#[derive(Clone, Copy, Debug)]
pub struct ModelLimits {
    pub native_records: u64,
    pub model_records: u64,
    pub source_pieces: u64,
    pub proof_records: u64,
    pub depth: u64,
    pub decoded_bytes: u64,
    pub reference_lookups: u64,
    pub reference_bytes: u64,
    pub index_records: u64,
    pub query_requests: u64,
    pub query_records: u64,
    pub query_capacity: u64,
}

impl Default for ModelLimits {
    fn default() -> Self {
        Self {
            native_records: 1_048_576,
            model_records: 1_048_576,
            source_pieces: 2_097_152,
            proof_records: 1_048_576,
            depth: 256,
            decoded_bytes: 33_554_432,
            reference_lookups: 1_048_576,
            reference_bytes: 33_554_432,
            index_records: 4_194_304,
            query_requests: 256,
            query_records: 65_536,
            query_capacity: 8_388_608,
        }
    }
}

impl ModelLimits {
    fn limit(self, kind: LimitKind) -> Option<(usize, u64)> {
        Some(match kind {
            LimitKind::NativeRecords => (0, self.native_records),
            LimitKind::ModelRecords => (1, self.model_records),
            LimitKind::SourcePieces => (2, self.source_pieces),
            LimitKind::ProofRecords => (3, self.proof_records),
            LimitKind::DecodedBytes => (4, self.decoded_bytes),
            LimitKind::ReferenceLookups => (5, self.reference_lookups),
            LimitKind::ReferenceBytes => (6, self.reference_bytes),
            LimitKind::IndexRecords => (7, self.index_records),
            LimitKind::QueryRequests => (8, self.query_requests),
            LimitKind::QueryRecords => (9, self.query_records),
            LimitKind::QueryCapacity => (10, self.query_capacity),
            LimitKind::Depth => (11, self.depth),
            _ => return None,
        })
    }
}

#[derive(Clone)]
pub struct CancellationToken {
    state: Arc<CancellationState>,
}

struct CancellationState {
    flag: AtomicBool,
    _control: Option<Lease>,
}

impl Default for CancellationToken {
    fn default() -> Self {
        Self {
            state: Arc::new(CancellationState {
                flag: AtomicBool::new(false),
                _control: None,
            }),
        }
    }
}

impl std::fmt::Debug for CancellationToken {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("CancellationToken")
            .field("cancelled", &self.state.flag.load(Ordering::Acquire))
            .finish_non_exhaustive()
    }
}

impl CancellationToken {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn cancel(&self) {
        self.state.flag.store(true, Ordering::Release);
    }
    /// Observe the same original flag without starting/resetting an operation.
    pub fn is_cancelled(&self) -> bool {
        self.state.flag.load(Ordering::Acquire)
    }

    pub(crate) fn same_flag(&self, other: &Self) -> bool {
        Arc::ptr_eq(&self.state, &other.state)
    }
}

#[derive(Clone)]
pub struct SourceBudget {
    inner: Arc<BudgetInner>,
}

struct BudgetInner {
    limits: SourceLimits,
    model_limits: Option<ModelLimits>,
    admission_mode: ResourceAdmissionMode,
    work_stats_collection: WorkStatsCollection,
    elapsed_limit_ns: Option<u64>,
    source: Arc<Quota>,
    auxiliary: Arc<Quota>,
    owners: Arc<Quota>,
}

/// Fields are independently sampled, not an atomic joint-peak observation.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct BudgetUsage {
    pub source_capacity: u64,
    pub source_capacity_peak: u64,
    pub auxiliary_capacity: u64,
    pub auxiliary_capacity_peak: u64,
    pub source_owners: u64,
}

impl SourceBudget {
    /// Deterministic source quota fixture with bounded-view work units.
    pub fn new(limits: SourceLimits) -> Result<Self, CoreError> {
        Self::create(limits, None)
    }

    /// Deterministic source/model quota fixture with bounded-view work units.
    pub fn new_with_model_limits(
        limits: SourceLimits,
        model_limits: ModelLimits,
    ) -> Result<Self, CoreError> {
        Self::create(limits, Some(model_limits))
    }

    fn create(limits: SourceLimits, model_limits: Option<ModelLimits>) -> Result<Self, CoreError> {
        Self::create_with_admission(
            limits,
            model_limits,
            ResourceAdmissionMode::CumulativeWork,
            WorkStatsCollection::Enabled,
        )
    }

    fn create_with_admission(
        limits: SourceLimits,
        model_limits: Option<ModelLimits>,
        admission_mode: ResourceAdmissionMode,
        work_stats_collection: WorkStatsCollection,
    ) -> Result<Self, CoreError> {
        Self::create_with_admission_window(
            limits,
            model_limits,
            admission_mode,
            work_stats_collection,
            ELAPSED_LIMIT_NS,
        )
    }

    fn create_with_admission_window(
        limits: SourceLimits,
        model_limits: Option<ModelLimits>,
        admission_mode: ResourceAdmissionMode,
        work_stats_collection: WorkStatsCollection,
        elapsed_limit_ns: u64,
    ) -> Result<Self, CoreError> {
        if admission_mode == ResourceAdmissionMode::CumulativeWork
            && work_stats_collection == WorkStatsCollection::Disabled
        {
            return Err(CoreError::UnsupportedConfiguration);
        }
        if matches!(admission_mode, ResourceAdmissionMode::TaskAccess)
            && (work_stats_collection != WorkStatsCollection::Disabled || model_limits.is_none())
        {
            return Err(CoreError::UnsupportedConfiguration);
        }
        // Model identifiers and flat ranges use checked u32 indices on all targets.
        if let Some(model) = model_limits {
            for count in [
                model.native_records,
                model.model_records,
                model.source_pieces,
                model.proof_records,
                model.index_records,
            ] {
                u32::try_from(count).map_err(|_| CoreError::ArithmeticOverflow)?;
            }
            for capacity in [model.decoded_bytes, model.query_capacity] {
                usize::try_from(capacity).map_err(|_| CoreError::ArithmeticOverflow)?;
            }
        }
        for capacity in [
            limits.max_source_bytes,
            limits.retained_source_capacity,
            limits.max_parse_bytes,
            limits.auxiliary_capacity,
        ] {
            usize::try_from(capacity).map_err(|_| CoreError::ArithmeticOverflow)?;
        }
        Ok(Self {
            inner: Arc::new(BudgetInner {
                source: Quota::new(limits.retained_source_capacity, LimitKind::SourceCapacity),
                auxiliary: Quota::new(limits.auxiliary_capacity, LimitKind::AuxiliaryCapacity),
                owners: Quota::new(limits.retained_source_owners, LimitKind::SourceOwners),
                limits,
                model_limits,
                admission_mode,
                work_stats_collection,
                elapsed_limit_ns: (admission_mode != ResourceAdmissionMode::CumulativeWork)
                    .then_some(elapsed_limit_ns),
            }),
        })
    }

    /// Deterministic source-only quota fixture with cumulative work diagnostics.
    pub fn source_fixture() -> Self {
        Self::new(SourceLimits::default()).expect("source fixture fits all supported targets")
    }

    /// Deterministic model quota fixture with cumulative work diagnostics.
    pub fn model_fixture() -> Self {
        Self::new_with_model_limits(SourceLimits::default(), ModelLimits::default())
            .expect("model fixture fits all supported targets")
    }

    /// Current task access with a short window and standard quotas for regressions.
    pub fn task_fixture() -> Self {
        Self::create_with_admission(
            SourceLimits::default(),
            Some(ModelLimits::default()),
            ResourceAdmissionMode::TaskAccess,
            WorkStatsCollection::Disabled,
        )
        .expect("task fixture fits all supported targets")
    }

    pub fn admission_mode(&self) -> ResourceAdmissionMode {
        self.inner.admission_mode
    }

    /// Preview safety policy: a cooperative 5s operation deadline and 256MiB
    /// of tracked auxiliary capacity. Neither bounds whole-process memory.
    pub fn preview() -> Self {
        Self::create_with_admission_window(
            SourceLimits {
                auxiliary_capacity: 268_435_456,
                ..SourceLimits::default()
            },
            Some(ModelLimits::default()),
            ResourceAdmissionMode::TaskAccess,
            WorkStatsCollection::Disabled,
            5_000_000_000,
        )
        .expect("preview safety policy fits all supported targets")
    }

    #[cfg(test)]
    pub(crate) fn preview_without_deadline_for_test() -> Self {
        let mut budget = Self::preview();
        Arc::get_mut(&mut budget.inner).unwrap().elapsed_limit_ns = None;
        budget
    }

    #[cfg(test)]
    pub(crate) fn task_with_limits_for_test(limits: SourceLimits, model: ModelLimits) -> Self {
        Self::create_with_admission(
            limits,
            Some(model),
            ResourceAdmissionMode::TaskAccess,
            WorkStatsCollection::Disabled,
        )
        .expect("test limits fit all supported targets")
    }

    pub fn work_stats_collection(&self) -> WorkStatsCollection {
        self.inner.work_stats_collection
    }

    pub fn active_work_limit(&self) -> Option<u64> {
        match self.admission_mode() {
            ResourceAdmissionMode::CumulativeWork => Some(self.limits().max_work),
            ResourceAdmissionMode::TaskAccess => None,
        }
    }

    pub fn elapsed_limit_ns(&self) -> Option<u64> {
        self.inner.elapsed_limit_ns
    }

    pub fn limits(&self) -> &SourceLimits {
        &self.inner.limits
    }

    pub fn model_limits(&self) -> Option<&ModelLimits> {
        self.inner.model_limits.as_ref()
    }

    pub fn usage(&self) -> BudgetUsage {
        BudgetUsage {
            source_capacity: self.inner.source.live.load(Ordering::Acquire),
            source_capacity_peak: self.inner.source.peak.load(Ordering::Acquire),
            auxiliary_capacity: self.inner.auxiliary.live.load(Ordering::Acquire),
            auxiliary_capacity_peak: self.inner.auxiliary.peak.load(Ordering::Acquire),
            source_owners: self.inner.owners.live.load(Ordering::Acquire),
        }
    }

    /// Reserve capacity for a resource owned by this existing quota root.
    /// This does not create a producer, reset a clock, or authorize publication.
    /// The resource's own control must check cancellation/deadline around admission.
    pub fn reserve_resource_capacity(&self, bytes: u64) -> Result<BoundaryReservation, CoreError> {
        self.reserve_auxiliary(bytes).map(BoundaryReservation)
    }

    pub fn begin(&self, cancel: CancellationToken) -> Operation {
        self.begin_inner(Some(cancel))
    }

    /// Start before allocating this operation's fresh, independently cancelable
    /// flag. No source or boundary can observe the private bootstrap state.
    pub fn begin_cancellable(&self, initially_cancelled: bool) -> Result<Operation, CoreError> {
        let mut op = self.begin_inner(None);
        op.finish_bootstrap(initially_cancelled)?;
        Ok(op)
    }

    fn begin_inner(&self, cancel: Option<CancellationToken>) -> Operation {
        let elapsed = self.elapsed_limit_ns().map(ElapsedState::new);
        let mut op = Operation {
            budget: self.clone(),
            cancel,
            stats: (self.work_stats_collection() == WorkStatsCollection::Enabled)
                .then(WorkStats::default),
            fault: None,
            admitted: [0; 12],
            elapsed,
        };
        if matches!(self.admission_mode(), ResourceAdmissionMode::TaskAccess) {
            // checkpoint_now latches every failure in this same operation.
            // A failed begin returns a faulted operation, never a fresh timer.
            let _ = op.checkpoint_now();
        }
        op
    }

    pub fn same_root(&self, other: &Self) -> bool {
        Arc::ptr_eq(&self.inner, &other.inner)
    }

    pub(crate) fn reserve_source(&self, bytes: u64) -> Result<Lease, CoreError> {
        self.inner.source.reserve(bytes)
    }

    pub(crate) fn reserve_auxiliary(&self, bytes: u64) -> Result<Lease, CoreError> {
        self.inner.auxiliary.reserve(bytes)
    }

    pub(crate) fn reserve_owner(&self) -> Result<Lease, CoreError> {
        self.inner.owners.reserve(1)
    }
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct WorkStats {
    scanned_bytes: u64,
    copied_bytes: u64,
    events: u64,
    metadata_scanned_bytes: u64,
    metadata_copied_bytes: u64,
    metadata_actions: u64,
    charged_units: u64,
}

impl WorkStats {
    pub fn scanned_bytes(self) -> u64 {
        self.scanned_bytes
    }

    pub fn copied_bytes(self) -> u64 {
        self.copied_bytes
    }

    pub fn events(self) -> u64 {
        self.events
    }

    pub fn total(self) -> u64 {
        // Every charge checks this byte diagnostic sum, independently of work units.
        self.scanned_bytes + self.copied_bytes + self.events
    }

    pub fn metadata_scanned_bytes(self) -> u64 {
        self.metadata_scanned_bytes
    }

    pub fn metadata_copied_bytes(self) -> u64 {
        self.metadata_copied_bytes
    }

    pub fn metadata_actions(self) -> u64 {
        self.metadata_actions
    }

    pub fn charged_units(self) -> u64 {
        self.charged_units
    }
}

/// One exclusive operation; stages share its first fault and protection window.
pub struct Operation {
    budget: SourceBudget,
    cancel: Option<CancellationToken>,
    stats: Option<WorkStats>,
    fault: Option<CoreError>,
    admitted: [u64; 12],
    elapsed: Option<ElapsedState>,
}

impl Operation {
    fn finish_bootstrap(&mut self, initially_cancelled: bool) -> Result<(), CoreError> {
        self.checkpoint_now()?;
        let bytes = size_of::<CancellationState>()
            + 2 * size_of::<usize>()
            + align_of::<CancellationState>()
            - 1;
        self.events(1)?;
        let lease = self
            .budget
            .reserve_auxiliary(bytes as u64)
            .map_err(|error| self.fail(error))?;
        self.copy(bytes as u64)?;
        self.checkpoint_now()?;
        let token = CancellationToken {
            state: Arc::new(CancellationState {
                flag: AtomicBool::new(initially_cancelled),
                _control: Some(lease),
            }),
        };
        self.cancel = Some(token);
        self.checkpoint_now()
    }

    /// A handle to the original flag, never a replacement flag or clock.
    pub fn cancellation(&self) -> CancellationToken {
        self.cancel
            .as_ref()
            .expect("fully initialized operation")
            .clone()
    }

    pub fn boundary_work(&mut self) -> BoundaryWork<'_> {
        BoundaryWork(self)
    }

    /// Account for an audited boundary's actual bounded read/write work.
    pub fn external_read(&mut self, bytes: u64) -> Result<(), CoreError> {
        self.scan(bytes)
    }

    pub fn external_write(&mut self, bytes: u64) -> Result<(), CoreError> {
        self.copy(bytes)
    }

    pub fn external_events(&mut self, count: u64) -> Result<(), CoreError> {
        self.events(count)
    }

    /// Latch the audited boundary's first typed failure; never clear a fault.
    pub fn boundary_failure(&mut self, error: CoreError) -> CoreError {
        self.fail(error)
    }

    /// Reserve a fixed external allocation before allocating it. The boundary
    /// must free its allocation before dropping the returned reservation.
    pub fn reserve_boundary(&mut self, bytes: u64) -> Result<BoundaryReservation, CoreError> {
        self.checkpoint_now()?;
        self.events(1)?;
        let lease = self
            .budget
            .reserve_auxiliary(bytes)
            .map_err(|e| self.fail(e))?;
        Ok(BoundaryReservation(lease))
    }

    pub(crate) fn uses_cancellation(&self, cancel: &CancellationToken) -> bool {
        self.cancel
            .as_ref()
            .is_some_and(|original| original.same_flag(cancel))
    }
    /// Cumulative record/output admission, shared by every stage of this operation.
    pub(crate) fn admit_model(&mut self, kind: LimitKind, delta: u64) -> Result<(), CoreError> {
        self.checkpoint()?;
        let limits = self
            .budget
            .inner
            .model_limits
            .ok_or_else(|| self.fail(CoreError::UnsupportedConfiguration))?;
        let (index, limit) = limits
            .limit(kind)
            .ok_or_else(|| self.fail(CoreError::InternalInvariant))?;
        if kind == LimitKind::Depth {
            return self.check_limit(kind, delta, limit);
        }
        let requested = self.admitted[index]
            .checked_add(delta)
            .ok_or_else(|| self.fail(CoreError::ArithmeticOverflow))?;
        self.check_limit(kind, requested, limit)?;
        self.admitted[index] = requested;
        Ok(())
    }

    pub fn checkpoint(&mut self) -> Result<(), CoreError> {
        if let Some(error) = self.fault {
            return Err(error);
        }
        if self
            .cancel
            .as_ref()
            .is_some_and(|cancel| cancel.state.flag.load(Ordering::Acquire))
        {
            return Err(self.fail(CoreError::Cancelled));
        }
        Ok(())
    }

    /// Check the original clock at an entry, allocation or publication boundary.
    pub(crate) fn checkpoint_now(&mut self) -> Result<(), CoreError> {
        self.checkpoint()?;
        if let Some(elapsed) = &mut self.elapsed {
            let result = elapsed.read_clock();
            result.map_err(|error| self.fail(error))?;
        }
        Ok(())
    }

    /// Synthetic elapsed nanos are only a policy negative-control facility.
    /// Installing samples never resets begin, the previous reading or the schedule.
    #[cfg(test)]
    pub(crate) fn set_clock_samples_for_test(&mut self, samples: &[u64]) {
        if let Some(elapsed) = &mut self.elapsed {
            elapsed.set_samples(samples);
        }
    }

    #[cfg(test)]
    pub(crate) fn set_shared_clock_for_test(&mut self, value: Arc<std::sync::atomic::AtomicU64>) {
        if let Some(elapsed) = &mut self.elapsed {
            elapsed.set_shared_sample(value);
        }
    }

    /// None means diagnostics were not collected from begin; it is never zero work.
    pub fn stats(&self) -> Option<WorkStats> {
        self.stats
    }

    pub fn first_fault(&self) -> Option<CoreError> {
        self.fault
    }

    pub(crate) fn fail(&mut self, error: CoreError) -> CoreError {
        *self.fault.get_or_insert(error)
    }

    pub(crate) fn budget(&self) -> &SourceBudget {
        &self.budget
    }

    pub(crate) fn uses_task_access(&self) -> bool {
        self.budget.admission_mode() == ResourceAdmissionMode::TaskAccess
    }

    /// Independent validation products are a capability, not a scheduling identity.
    pub(crate) fn uses_verified_index(&self) -> bool {
        self.uses_task_access()
    }

    #[cfg(test)]
    pub(crate) fn clock_read_count_for_test(&self) -> usize {
        self.elapsed
            .as_ref()
            .map_or(0, ElapsedState::read_count_for_test)
    }

    pub(crate) fn ensure_budget(&mut self, budget: &SourceBudget) -> Result<(), CoreError> {
        self.checkpoint()?;
        if !self.budget.same_root(budget) {
            return Err(self.fail(CoreError::WrongBudget));
        }
        Ok(())
    }

    pub(crate) fn check_limit(
        &mut self,
        kind: LimitKind,
        requested: u64,
        limit: u64,
    ) -> Result<(), CoreError> {
        self.checkpoint()?;
        if requested > limit {
            return Err(self.fail(CoreError::ResourceLimit {
                kind,
                requested,
                limit,
            }));
        }
        Ok(())
    }

    pub(crate) fn scan(&mut self, bytes: u64) -> Result<(), CoreError> {
        self.charge::<ScanWork>(bytes)
    }

    /// Admit a complete scan through the original bounded checkpoint path.
    pub(crate) fn scan_chunked(&mut self, mut bytes: u64) -> Result<(), CoreError> {
        while bytes > CHECKPOINT_BYTES {
            self.scan(CHECKPOINT_BYTES)?;
            bytes -= CHECKPOINT_BYTES;
        }
        self.scan(bytes)
    }

    pub(crate) fn copy(&mut self, bytes: u64) -> Result<(), CoreError> {
        self.charge::<CopyWork>(bytes)
    }

    pub(crate) fn events(&mut self, events: u64) -> Result<(), CoreError> {
        self.charge::<EventWork>(events)
    }

    /// Audited fixed metadata only; never source/text, arrays or actual buffer moves.
    pub(crate) fn view_scan(&mut self, bytes: u64) -> Result<(), CoreError> {
        self.charge::<ViewScanWork>(bytes)
    }

    /// Logical by-value view construction, not a stored row write or ABI export.
    pub(crate) fn view_copy(&mut self, bytes: u64) -> Result<(), CoreError> {
        self.charge::<ViewCopyWork>(bytes)
    }

    #[inline(always)]
    fn charge<K: ChargeKind>(&mut self, count: u64) -> Result<(), CoreError> {
        let kind = K::KIND;
        self.checkpoint()?;
        let block_limit = match kind {
            WorkKind::Scan | WorkKind::Copy => CHECKPOINT_BYTES,
            WorkKind::Events => CHECKPOINT_EVENTS,
            WorkKind::ViewScan | WorkKind::ViewCopy => MAX_METADATA_VIEW_BYTES,
        };
        if count > block_limit {
            return Err(self.fail(CoreError::InternalInvariant));
        }
        let (clock_bytes, clock_events) = match kind {
            WorkKind::Scan | WorkKind::Copy => (count, 0),
            WorkKind::Events => (0, count),
            WorkKind::ViewScan | WorkKind::ViewCopy => (count, u64::from(count != 0)),
        };
        if matches!(
            self.budget.admission_mode(),
            ResourceAdmissionMode::TaskAccess
        ) && count == 0
        {
            self.checkpoint_now()?;
        }
        self.schedule_elapsed_charge(clock_bytes, clock_events)?;
        if self.stats.is_none() {
            // Collection is optional only for elapsed admission. Keep the same
            // final flag Acquire after real clock scheduling and block checks.
            return self.checkpoint();
        }
        self.charge_collected(count, kind)
    }

    // Keep the cumulative transaction out of the shared protection frame.
    #[inline(never)]
    fn charge_collected(&mut self, count: u64, kind: WorkKind) -> Result<(), CoreError> {
        let Some(mut next) = self.stats else {
            return Err(self.fail(CoreError::InternalInvariant));
        };
        // Check and commit all diagnostics and the guard as one charge transaction.
        // Earlier successful calls in a compound getter are never rolled back.
        let increment = |value: u64, delta: u64| value.checked_add(delta);
        let overflow = CoreError::ArithmeticOverflow;
        match kind {
            WorkKind::Scan | WorkKind::ViewScan => {
                next.scanned_bytes =
                    increment(next.scanned_bytes, count).ok_or_else(|| self.fail(overflow))?;
            }
            WorkKind::Copy | WorkKind::ViewCopy => {
                next.copied_bytes =
                    increment(next.copied_bytes, count).ok_or_else(|| self.fail(overflow))?;
            }
            WorkKind::Events => {
                next.events = increment(next.events, count).ok_or_else(|| self.fail(overflow))?;
            }
        }
        let view = matches!(kind, WorkKind::ViewScan | WorkKind::ViewCopy);
        if view {
            let field = match kind {
                WorkKind::ViewScan => &mut next.metadata_scanned_bytes,
                WorkKind::ViewCopy => &mut next.metadata_copied_bytes,
                _ => unreachable!(),
            };
            *field = increment(*field, count).ok_or_else(|| self.fail(overflow))?;
            next.metadata_actions = increment(next.metadata_actions, u64::from(count != 0))
                .ok_or_else(|| self.fail(overflow))?;
        }
        // Check the byte diagnostic sum as well as the charged fixed-view units.
        next.scanned_bytes
            .checked_add(next.copied_bytes)
            .and_then(|value| value.checked_add(next.events))
            .ok_or_else(|| self.fail(overflow))?;
        let requested = increment(
            next.charged_units,
            if view { u64::from(count != 0) } else { count },
        )
        .ok_or_else(|| self.fail(overflow))?;
        if let Some(limit) = self.budget.active_work_limit() {
            self.check_limit(LimitKind::Work, requested, limit)?;
        } else {
            // Preserve the original charge's final same-flag Acquire even when
            // cumulative work is diagnostic rather than an admission criterion.
            self.checkpoint()?;
        }
        next.charged_units = requested;
        self.stats = Some(next);
        Ok(())
    }

    fn schedule_elapsed_charge(&mut self, bytes: u64, events: u64) -> Result<(), CoreError> {
        let Some(elapsed) = &self.elapsed else {
            return Ok(());
        };
        let due = elapsed
            .reaches_boundary(bytes, events)
            .map_err(|error| self.fail(error))?;
        if due {
            self.checkpoint_now()?;
        }
        // The complete current block belongs to the new window after a read;
        // discarding its crossed-boundary portion would skip future checkpoints.
        let result = match &mut self.elapsed {
            Some(elapsed) => elapsed.record_charge(bytes, events),
            None => Err(CoreError::InternalInvariant),
        };
        result.map_err(|error| self.fail(error))
    }
}

/// Finite access for the audited platform boundary. This does not expose an
/// operation, accept a replacement, or permit moving an operation out of a ticket.
///
/// ```compile_fail,E0616
/// use markdown_core::policy::BoundaryWork;
/// fn extract(work: BoundaryWork<'_>) { let _ = work.0; }
/// ```
pub struct BoundaryWork<'op>(&'op mut Operation);

impl BoundaryWork<'_> {
    pub fn checkpoint_now(&mut self) -> Result<(), CoreError> {
        self.0.checkpoint_now()
    }

    pub fn external_read(&mut self, bytes: u64) -> Result<(), CoreError> {
        self.0.external_read(bytes)
    }

    pub fn external_write(&mut self, bytes: u64) -> Result<(), CoreError> {
        self.0.external_write(bytes)
    }

    pub fn external_events(&mut self, count: u64) -> Result<(), CoreError> {
        self.0.external_events(count)
    }

    pub fn reserve_boundary(&mut self, bytes: u64) -> Result<BoundaryReservation, CoreError> {
        self.0.reserve_boundary(bytes)
    }

    pub fn admit_actual(
        &mut self,
        reservation: &mut BoundaryReservation,
        bytes: u64,
    ) -> Result<(), CoreError> {
        reservation.admit_actual(bytes, self.0)
    }

    pub fn boundary_failure(&mut self, error: CoreError) -> CoreError {
        self.0.boundary_failure(error)
    }

    pub fn first_fault(&self) -> Option<CoreError> {
        self.0.first_fault()
    }

    pub fn limits(&self) -> SourceLimits {
        *self.0.budget.limits()
    }
}

enum WorkKind {
    Scan,
    Copy,
    Events,
    ViewScan,
    ViewCopy,
}

trait ChargeKind {
    const KIND: WorkKind;
}

struct ScanWork;
struct CopyWork;
struct EventWork;
struct ViewScanWork;
struct ViewCopyWork;

impl ChargeKind for ScanWork {
    const KIND: WorkKind = WorkKind::Scan;
}

impl ChargeKind for CopyWork {
    const KIND: WorkKind = WorkKind::Copy;
}

impl ChargeKind for EventWork {
    const KIND: WorkKind = WorkKind::Events;
}

impl ChargeKind for ViewScanWork {
    const KIND: WorkKind = WorkKind::ViewScan;
}

impl ChargeKind for ViewCopyWork {
    const KIND: WorkKind = WorkKind::ViewCopy;
}

struct Quota {
    live: AtomicU64,
    peak: AtomicU64,
    limit: u64,
    kind: LimitKind,
}

impl Quota {
    fn new(limit: u64, kind: LimitKind) -> Arc<Self> {
        Arc::new(Self {
            live: AtomicU64::new(0),
            peak: AtomicU64::new(0),
            limit,
            kind,
        })
    }

    fn release(&self, amount: u64) -> Result<(), CoreError> {
        if amount == 0 {
            return Ok(());
        }
        self.live
            .try_update(Ordering::AcqRel, Ordering::Acquire, |live| {
                live.checked_sub(amount)
            })
            .map(|_| ())
            .map_err(|_| CoreError::InternalInvariant)
    }

    fn reserve(self: &Arc<Self>, amount: u64) -> Result<Lease, CoreError> {
        let mut live = self.live.load(Ordering::Acquire);
        loop {
            let requested = live
                .checked_add(amount)
                .ok_or(CoreError::ArithmeticOverflow)?;
            if requested > self.limit {
                return Err(CoreError::ResourceLimit {
                    kind: self.kind,
                    requested,
                    limit: self.limit,
                });
            }
            match self.live.compare_exchange_weak(
                live,
                requested,
                Ordering::AcqRel,
                Ordering::Acquire,
            ) {
                Ok(_) => {
                    self.peak.fetch_max(requested, Ordering::AcqRel);
                    return Ok(Lease {
                        quota: self.clone(),
                        amount,
                    });
                }
                Err(current) => live = current,
            }
        }
    }
}

/// Nonclone reservation. Releasing never checks cancellation or charges work.
pub(crate) struct Lease {
    quota: Arc<Quota>,
    amount: u64,
}

/// Allocation owner releases this only after its allocation has been freed.
/// This is requested-capacity accounting, not allocator usable bytes or RSS.
pub struct BoundaryReservation(Lease);

impl BoundaryReservation {
    /// Read-only proof that this lease belongs to the captured auxiliary root.
    /// This neither admits capacity nor checks cancellation or current source.
    pub fn belongs_to(&self, budget: &SourceBudget) -> bool {
        Arc::ptr_eq(&self.0.quota, &budget.inner.auxiliary)
    }

    pub fn reserved_bytes(&self) -> u64 {
        self.0.amount
    }

    /// Reduce an existing reservation without releasing and reacquiring it.
    /// The owner must first free any payload no longer covered by this amount.
    /// Cleanup is independent of cancellation and never allocates.
    pub fn shrink_to(&mut self, bytes: u64) -> Result<(), CoreError> {
        if bytes > self.0.amount {
            return Err(CoreError::InvalidRange);
        }
        let released = self.0.amount - bytes;
        self.0.quota.release(released)?;
        self.0.amount = bytes;
        Ok(())
    }

    pub fn admit_actual(&mut self, bytes: u64, op: &mut Operation) -> Result<(), CoreError> {
        op.checkpoint_now()?;
        if !Arc::ptr_eq(&self.0.quota, &op.budget.inner.auxiliary) {
            return Err(op.fail(CoreError::WrongBudget));
        }
        self.0.admit_actual(bytes).map_err(|e| op.fail(e))?;
        op.checkpoint_now()
    }
}

impl Lease {
    pub(crate) fn admit_actual(&mut self, actual: u64) -> Result<(), CoreError> {
        if actual > self.amount {
            let mut extra = self.quota.reserve(actual - self.amount)?;
            self.amount = actual;
            // Transfer this reservation without freeing/reacquiring it.
            extra.amount = 0;
        }
        Ok(())
    }
}

impl Drop for Lease {
    fn drop(&mut self) {
        // Preserve the charged amount if accounting is already inconsistent.
        let _ = self.quota.release(self.amount);
    }
}

#[cfg(test)]
mod tests;
