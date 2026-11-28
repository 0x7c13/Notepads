// Notepads Markdown modifications: Copyright (c) 2026, Jiaqi (0x7c13) Liu.
// Original upstream MIT/Apache-2.0 licenses and notices are preserved.
//! Target-neutral lifecycle primitives for CPU-bound Merman operations.
//!
//! Parsing, analysis, SVG, ASCII, and export all need the same operation-scoped cancellation and
//! deadline semantics. Their layout and output budgets remain adapter-owned.

use std::fmt;
#[cfg(any(test, feature = "test-support"))]
use std::sync::atomic::AtomicU64;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, OnceLock};

use crate::resources::ResourceProfile;
#[cfg(any(
    not(all(target_arch = "wasm32", target_os = "unknown")),
    feature = "operation-deadlines",
    feature = "system-timing",
    test
))]
use std::time::Duration;

#[cfg(not(all(
    target_arch = "wasm32",
    target_os = "unknown",
    any(feature = "operation-deadlines", feature = "system-timing")
)))]
use std::time::Instant;
#[cfg(all(
    target_arch = "wasm32",
    target_os = "unknown",
    any(feature = "operation-deadlines", feature = "system-timing")
))]
use web_time::Instant;

/// The broad phase in which an operation observes a terminal condition.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum OperationPhase {
    Admission,
    Parse,
    Semantic,
    Analysis,
    Layout,
    Emit,
    Postprocess,
    Export,
    Unknown,
}

impl OperationPhase {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Admission => "admission",
            Self::Parse => "parse",
            Self::Semantic => "semantic",
            Self::Analysis => "analysis",
            Self::Layout => "layout",
            Self::Emit => "emit",
            Self::Postprocess => "postprocess",
            Self::Export => "export",
            Self::Unknown => "unknown",
        }
    }
}

impl fmt::Display for OperationPhase {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// Why an operation stopped at a cooperative checkpoint.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum CancelReason {
    Requested,
    DeadlineExceeded,
}

impl CancelReason {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Requested => "requested",
            Self::DeadlineExceeded => "deadline_exceeded",
        }
    }
}

impl fmt::Display for CancelReason {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// Structured cooperative cancellation returned by an operation checkpoint.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
#[error("operation cancelled during {phase}: {reason}")]
pub struct OperationCancelled {
    pub phase: OperationPhase,
    pub reason: CancelReason,
}

/// Result channel for a controlled operation stage.
pub type OperationControlResult<T> = std::result::Result<T, OperationCancelled>;

type Clock = Arc<dyn Fn() -> Instant + Send + Sync + 'static>;

struct OperationState {
    attention_required: AtomicBool,
    cancelled: AtomicBool,
    terminal: OnceLock<OperationLedgerError>,
    parent: Option<Arc<OperationState>>,
    deadline: OnceLock<Instant>,
    clock: Clock,
    #[cfg(any(test, feature = "test-support"))]
    successful_checkpoints_before_cancellation: AtomicU64,
}

impl fmt::Debug for OperationState {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("OperationState")
            .field("cancelled", &self.cancelled.load(Ordering::Acquire))
            .field("terminal", &self.terminal.get())
            .field("deadline", &self.deadline.get())
            .finish_non_exhaustive()
    }
}

impl OperationState {
    fn new(parent: Option<Arc<OperationState>>, clock: Clock) -> Self {
        let attention_required = parent.is_some();
        Self {
            attention_required: AtomicBool::new(attention_required),
            cancelled: AtomicBool::new(false),
            terminal: OnceLock::new(),
            parent,
            deadline: OnceLock::new(),
            clock,
            #[cfg(any(test, feature = "test-support"))]
            successful_checkpoints_before_cancellation: AtomicU64::new(u64::MAX),
        }
    }

    fn latch_terminal(&self, error: OperationLedgerError) -> OperationLedgerError {
        self.attention_required.store(true, Ordering::Release);
        self.terminal.get_or_init(|| error).clone()
    }

    fn attention_required(&self) -> bool {
        self.attention_required.load(Ordering::Acquire)
    }
}

/// Cloneable operation-scoped cooperative cancellation and deadline control.
///
/// A synchronous callback cannot be forcefully interrupted. It observes a request when it returns
/// to a checkpoint. Cancellation, deadlines, resource ceilings, and resource arithmetic failures
/// compete for one sticky terminal outcome per operation scope.
#[derive(Clone, Debug)]
pub struct OperationControl {
    state: Arc<OperationState>,
    default_phase: OperationPhase,
    memory_scope: Option<markdown_memory::MemoryScope>,
    memory_region: Option<markdown_memory::RegionHandle>,
}

impl Default for OperationControl {
    fn default() -> Self {
        Self::new()
    }
}

impl OperationControl {
    /// Attaches the enclosing native diagram job's already prepaid allocation authority.
    /// Clones share cancellation and preserve the exact authority; no allocation is made here.
    /// Legacy controls have no authority. A production native entry must reject that state
    /// before entering parser/layout rather than treating absence as an unlimited budget.
    pub fn with_memory_scope(&self, scope: markdown_memory::MemoryScope) -> Self {
        Self {
            state: Arc::clone(&self.state),
            default_phase: self.default_phase,
            memory_region: self
                .memory_region
                .as_ref()
                .filter(|region| region.scope().same_root(&scope))
                .cloned(),
            memory_scope: Some(scope),
        }
    }

    pub fn memory_scope(&self) -> Option<&markdown_memory::MemoryScope> {
        self.memory_scope.as_ref()
    }

    /// Retains the producer's joint receipt owner through token/AST/model moves.
    /// A new deep clone still requires a new complete nested allocation plan.
    pub fn with_memory_region(&self, region: markdown_memory::RegionHandle) -> Self {
        Self {
            state: Arc::clone(&self.state),
            default_phase: self.default_phase,
            memory_scope: Some(region.scope().clone()),
            memory_region: Some(region),
        }
    }

    pub fn memory_region(&self) -> Option<&markdown_memory::RegionHandle> {
        self.memory_region.as_ref()
    }

    /// Creates active control using the platform monotonic clock.
    pub fn new() -> Self {
        Self::with_clock(default_clock())
    }

    /// Creates active control with a supplied monotonic clock.
    ///
    /// This remains crate-private; public callers use [`Self::with_deadline`]. Tests use the hook
    /// to advance a fake monotonic clock without sleeping.
    fn with_clock(clock: Clock) -> Self {
        Self {
            state: Arc::new(OperationState::new(None, clock)),
            default_phase: OperationPhase::Unknown,
            memory_scope: None,
            memory_region: None,
        }
    }

    /// Creates an independently cancellable child that observes this control's cancellation and
    /// deadline state.
    pub fn child(&self) -> Self {
        Self {
            state: Arc::new(OperationState::new(
                Some(Arc::clone(&self.state)),
                Arc::clone(&self.state.clock),
            )),
            default_phase: self.default_phase,
            memory_scope: self.memory_scope.clone(),
            memory_region: self.memory_region.clone(),
        }
    }

    /// Creates a shared view whose unnamed checkpoints report the supplied operation phase.
    pub fn for_phase(&self, phase: OperationPhase) -> Self {
        Self {
            state: Arc::clone(&self.state),
            default_phase: phase,
            memory_scope: self.memory_scope.clone(),
            memory_region: self.memory_region.clone(),
        }
    }

    /// Sets a relative monotonic deadline if one is not already configured.
    #[cfg(any(
        not(all(target_arch = "wasm32", target_os = "unknown")),
        feature = "operation-deadlines",
        feature = "system-timing"
    ))]
    pub fn with_deadline(self, timeout: Duration) -> Self {
        self.set_deadline(timeout);
        self
    }

    /// Preserves the request's original monotonic deadline after its metadata
    /// admission. This sets the existing cell once and allocates nothing.
    #[cfg(not(all(target_arch = "wasm32", target_os = "unknown")))]
    pub fn with_absolute_deadline(self, deadline: std::time::Instant) -> Self {
        if self.state.deadline.set(deadline).is_ok() {
            self.state.attention_required.store(true, Ordering::Release);
        }
        self
    }

    /// Sets a relative monotonic deadline, returning whether this call installed it.
    #[cfg(any(
        not(all(target_arch = "wasm32", target_os = "unknown")),
        feature = "operation-deadlines",
        feature = "system-timing"
    ))]
    pub fn set_deadline(&self, timeout: Duration) -> bool {
        let now = (self.state.clock)();
        self.state.attention_required.store(true, Ordering::Release);
        self.state
            .deadline
            .set(deadline_after(now, timeout))
            .is_ok()
    }

    /// Requests cancellation for this control and its clones.
    pub fn cancel(&self) {
        self.state.attention_required.store(true, Ordering::Release);
        self.state.cancelled.store(true, Ordering::Release);
    }

    /// Returns whether this control or an ancestor has received a cancellation request.
    pub fn is_cancelled(&self) -> bool {
        let mut state = Some(self.state.as_ref());
        while let Some(current) = state {
            if current.cancelled.load(Ordering::Acquire) {
                return true;
            }
            state = current.parent.as_deref();
        }
        false
    }

    /// Checks cancellation/deadline at an unspecified phase.
    pub fn checkpoint(&self) -> Result<(), OperationCancelled> {
        self.checkpoint_at(self.default_phase)
    }

    /// Replays this scope's observed cancellation without checking for a new terminal.
    pub(crate) fn observed_cancellation(&self) -> Option<OperationCancelled> {
        match self.state.terminal.get() {
            Some(OperationLedgerError::Cancelled(error)) => Some(*error),
            _ => None,
        }
    }

    /// Checks cancellation/deadline at a named phase.
    ///
    /// This cancellation-only projection is used by parsing and analysis code that cannot produce
    /// resource terminals. If a target adapter has already recorded a non-cancellation terminal,
    /// this method does not replace it with a later cancellation. Target adapters use
    /// [`Self::terminal_checkpoint_at`] to replay the complete terminal value.
    pub fn checkpoint_at(&self, phase: OperationPhase) -> Result<(), OperationCancelled> {
        if self
            .memory_scope
            .as_ref()
            .is_some_and(|scope| scope.failure().is_some())
        {
            // The complete fixed memory reason remains in the native authority.
            // This cancellation projection makes existing neutral-unwind paths
            // stop before consuming a failed value, without diagnostic allocation.
            self.cancel();
            return Err(OperationCancelled {
                phase,
                reason: CancelReason::Requested,
            });
        }
        if !self.state.attention_required() {
            return Ok(());
        }
        self.observe_cancellation_at(phase).map_or(Ok(()), Err)
    }

    /// Checks cancellation, deadlines, and any previously observed operation terminal.
    ///
    /// Target adapters call this before formal charges and controlled work. Speculative estimates
    /// that may be discarded may use [`Self::checkpoint_at`], but they must not record a resource
    /// terminal.
    pub fn terminal_checkpoint_at(
        &self,
        phase: OperationPhase,
    ) -> Result<(), OperationLedgerError> {
        if !self.state.attention_required() {
            return Ok(());
        }
        if let Some(error) = self.state.terminal.get() {
            return Err(error.clone());
        }
        let Some(cancellation) = self.observe_cancellation_at(phase) else {
            return self.state.terminal.get().cloned().map_or(Ok(()), Err);
        };
        Err(self.latch_terminal_error(OperationLedgerError::Cancelled(cancellation)))
    }

    fn observe_cancellation_at(&self, phase: OperationPhase) -> Option<OperationCancelled> {
        #[cfg(any(test, feature = "test-support"))]
        if self.consume_scheduled_checkpoint() {
            self.cancel();
        }

        self.resolve_cancellation_at(phase)
    }

    fn resolve_cancellation_at(&self, phase: OperationPhase) -> Option<OperationCancelled> {
        let mut state = Some(self.state.as_ref());
        let mut inherited = false;
        while let Some(current) = state {
            let terminal = if let Some(error) = current.terminal.get() {
                match error {
                    OperationLedgerError::Cancelled(error) if inherited => {
                        Some(OperationCancelled {
                            phase,
                            reason: error.reason,
                        })
                    }
                    OperationLedgerError::Cancelled(error) => Some(*error),
                    OperationLedgerError::ResourceLimitExceeded(_)
                    | OperationLedgerError::ArithmeticOverflow { .. }
                        if !inherited =>
                    {
                        return None;
                    }
                    OperationLedgerError::ResourceLimitExceeded(_)
                    | OperationLedgerError::ArithmeticOverflow { .. }
                        if current.cancelled.load(Ordering::Acquire) =>
                    {
                        Some(OperationCancelled {
                            phase,
                            reason: CancelReason::Requested,
                        })
                    }
                    OperationLedgerError::ResourceLimitExceeded(_)
                    | OperationLedgerError::ArithmeticOverflow { .. }
                        if current
                            .deadline
                            .get()
                            .is_some_and(|deadline| (current.clock)() >= *deadline) =>
                    {
                        Some(OperationCancelled {
                            phase,
                            reason: CancelReason::DeadlineExceeded,
                        })
                    }
                    OperationLedgerError::ResourceLimitExceeded(_)
                    | OperationLedgerError::ArithmeticOverflow { .. } => None,
                }
            } else if current.cancelled.load(Ordering::Acquire) {
                let cancellation = OperationCancelled {
                    phase,
                    reason: CancelReason::Requested,
                };
                match Self::resolve_latched_cancellation(current, inherited, cancellation) {
                    Some(error) => Some(error),
                    None => return None,
                }
            } else if current
                .deadline
                .get()
                .is_some_and(|deadline| (current.clock)() >= *deadline)
            {
                let cancellation = OperationCancelled {
                    phase,
                    reason: CancelReason::DeadlineExceeded,
                };
                match Self::resolve_latched_cancellation(current, inherited, cancellation) {
                    Some(error) => Some(error),
                    None => return None,
                }
            } else {
                None
            };

            if let Some(error) = terminal {
                if !inherited {
                    return Some(error);
                }
                return match self
                    .state
                    .latch_terminal(OperationLedgerError::Cancelled(error))
                {
                    OperationLedgerError::Cancelled(error) => Some(error),
                    OperationLedgerError::ResourceLimitExceeded(_)
                    | OperationLedgerError::ArithmeticOverflow { .. } => None,
                };
            }

            state = current.parent.as_deref();
            inherited = true;
        }
        None
    }

    fn resolve_latched_cancellation(
        state: &OperationState,
        inherited: bool,
        cancellation: OperationCancelled,
    ) -> Option<OperationCancelled> {
        match state.latch_terminal(OperationLedgerError::Cancelled(cancellation)) {
            OperationLedgerError::Cancelled(error) => Some(error),
            OperationLedgerError::ResourceLimitExceeded(_)
            | OperationLedgerError::ArithmeticOverflow { .. }
                if inherited =>
            {
                Some(cancellation)
            }
            OperationLedgerError::ResourceLimitExceeded(_)
            | OperationLedgerError::ArithmeticOverflow { .. } => None,
        }
    }

    fn latch_terminal_error(&self, error: OperationLedgerError) -> OperationLedgerError {
        self.state.latch_terminal(error)
    }

    /// Records a target-owned resource ceiling as this operation's first terminal outcome.
    pub fn terminate_resource_limit(
        &self,
        error: OperationResourceLimitExceeded,
    ) -> OperationLedgerError {
        self.latch_terminal_error(OperationLedgerError::ResourceLimitExceeded(error))
    }

    /// Records target-owned resource accounting overflow as this operation's first terminal outcome.
    pub fn terminate_resource_overflow(
        &self,
        id: &'static str,
        phase: OperationPhase,
        resource_phase: &'static str,
        actual: u64,
        maximum: u64,
        provenance: OperationResourceProvenance,
    ) -> OperationLedgerError {
        self.latch_terminal_error(OperationLedgerError::ArithmeticOverflow {
            id,
            phase,
            resource_phase,
            actual,
            maximum,
            provenance,
        })
    }

    #[cfg(any(test, feature = "test-support"))]
    fn consume_scheduled_checkpoint(&self) -> bool {
        self.state
            .successful_checkpoints_before_cancellation
            .try_update(Ordering::Relaxed, Ordering::Relaxed, |remaining| {
                (remaining != u64::MAX).then(|| remaining.saturating_sub(1))
            })
            .is_ok_and(|remaining| remaining == 0)
    }

    /// Schedules deterministic cancellation for tests after the requested successful checkpoints.
    #[cfg(any(test, feature = "test-support"))]
    pub fn cancel_after_checkpoints(&self, successful_checkpoints: usize) {
        self.state.attention_required.store(true, Ordering::Release);
        self.state
            .successful_checkpoints_before_cancellation
            .store(successful_checkpoints as u64, Ordering::Relaxed);
    }
}

#[cfg(any(
    not(all(target_arch = "wasm32", target_os = "unknown")),
    feature = "operation-deadlines",
    feature = "system-timing"
))]
fn deadline_after(now: Instant, mut timeout: Duration) -> Instant {
    loop {
        if let Some(deadline) = now.checked_add(timeout) {
            return deadline;
        }
        timeout /= 2;
    }
}

fn default_clock() -> Clock {
    #[cfg(not(all(
        target_arch = "wasm32",
        target_os = "unknown",
        not(any(feature = "operation-deadlines", feature = "system-timing"))
    )))]
    {
        Arc::new(Instant::now)
    }

    #[cfg(all(
        target_arch = "wasm32",
        target_os = "unknown",
        not(any(feature = "operation-deadlines", feature = "system-timing"))
    ))]
    {
        Arc::new(|| unreachable!("deadline clock is unavailable in this artifact"))
    }
}

/// Stable adapter domain that owns an operation resource terminal.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum OperationResourceDomain {
    Input,
    Render,
    Ascii,
    Export,
}

impl OperationResourceDomain {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Input => "input",
            Self::Render => "render",
            Self::Ascii => "ascii",
            Self::Export => "export",
        }
    }
}

impl fmt::Display for OperationResourceDomain {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.as_str())
    }
}

/// One explicit policy override captured when a resource terminal is first recorded.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct OperationResourceOverride {
    pub id: &'static str,
    pub value: u64,
}

/// Immutable policy provenance attached to the first operation resource terminal.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OperationResourceProvenance {
    pub domain: OperationResourceDomain,
    pub profile: Option<ResourceProfile>,
    pub explicit_overrides: Arc<[OperationResourceOverride]>,
}

impl OperationResourceProvenance {
    pub fn new(
        domain: OperationResourceDomain,
        profile: Option<ResourceProfile>,
        explicit_overrides: impl IntoIterator<Item = OperationResourceOverride>,
    ) -> Self {
        Self {
            domain,
            profile,
            explicit_overrides: explicit_overrides.into_iter().collect::<Vec<_>>().into(),
        }
    }
}

/// Target-neutral description of a checked operation resource rejection.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error(
    "operation resource limit `{id}` exceeded during {phase}: {consumed} + {requested} > {limit}"
)]
pub struct OperationResourceLimitExceeded {
    pub id: &'static str,
    pub phase: OperationPhase,
    pub resource_phase: &'static str,
    pub limit: u64,
    pub consumed: u64,
    pub requested: u64,
    pub provenance: OperationResourceProvenance,
}

/// Sticky terminal failure shared by operation controls and target-owned resource adapters.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum OperationLedgerError {
    #[error(transparent)]
    Cancelled(#[from] OperationCancelled),
    #[error(transparent)]
    ResourceLimitExceeded(#[from] OperationResourceLimitExceeded),
    #[error(
        "operation resource `{id}` arithmetic overflow during {phase} ({resource_phase}; actual {actual}, maximum {maximum})"
    )]
    ArithmeticOverflow {
        id: &'static str,
        phase: OperationPhase,
        resource_phase: &'static str,
        actual: u64,
        maximum: u64,
        provenance: OperationResourceProvenance,
    },
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Barrier;
    use std::sync::atomic::AtomicU64;
    use std::thread;

    fn test_resource_provenance() -> OperationResourceProvenance {
        OperationResourceProvenance::new(
            OperationResourceDomain::Render,
            Some(ResourceProfile::Interactive),
            [],
        )
    }

    #[test]
    fn clones_and_children_observe_shared_cancellation() {
        let control = OperationControl::new();
        let clone = control.clone();
        let child = control.child();
        child.cancel();
        assert!(child.is_cancelled());
        assert!(!control.is_cancelled());
        control.cancel();
        assert!(clone.is_cancelled());
        assert!(child.checkpoint_at(OperationPhase::Parse).is_err());
    }

    #[test]
    fn phase_views_share_state_and_label_unnamed_checkpoints() {
        let control = OperationControl::new();
        let parse = control.for_phase(OperationPhase::Parse);

        control.cancel();

        let error = parse.checkpoint().unwrap_err();
        assert_eq!(error.phase, OperationPhase::Parse);
        assert_eq!(error.reason, CancelReason::Requested);
    }

    #[cfg(not(all(target_arch = "wasm32", target_os = "unknown")))]
    #[test]
    fn absolute_prepare_deadline_does_not_call_clock_or_restart_an_existing_deadline() {
        let calls = Arc::new(std::sync::atomic::AtomicUsize::new(0));
        let observed = calls.clone();
        let now = std::time::Instant::now();
        let control = OperationControl::with_clock(Arc::new(move || {
            observed.fetch_add(1, Ordering::SeqCst);
            now
        }))
        .with_absolute_deadline(now - Duration::from_millis(1));
        assert_eq!(calls.load(Ordering::SeqCst), 0);
        let control = control.with_absolute_deadline(now + Duration::from_secs(60));
        assert_eq!(calls.load(Ordering::SeqCst), 0);
        assert_eq!(
            control.checkpoint().unwrap_err().reason,
            CancelReason::DeadlineExceeded
        );
    }

    #[test]
    fn deadline_is_sticky_and_distinct_from_requested_cancellation() {
        let now = Arc::new(AtomicU64::new(0));
        let source = Arc::clone(&now);
        let base = Instant::now();
        let clock = Arc::new(move || base + Duration::from_millis(source.load(Ordering::Relaxed)));
        let control = OperationControl::with_clock(clock).with_deadline(Duration::from_millis(0));
        let error = control.checkpoint_at(OperationPhase::Layout).unwrap_err();
        assert_eq!(error.reason, CancelReason::DeadlineExceeded);
        assert_eq!(control.checkpoint().unwrap_err(), error);
        assert_eq!(
            control
                .terminal_checkpoint_at(OperationPhase::Emit)
                .unwrap_err(),
            OperationLedgerError::Cancelled(error)
        );
    }

    #[test]
    fn child_latches_the_first_reason_observed_across_its_parent_chain() {
        let now = Arc::new(AtomicU64::new(0));
        let source = Arc::clone(&now);
        let base = Instant::now();
        let clock = Arc::new(move || base + Duration::from_millis(source.load(Ordering::Relaxed)));
        let parent = OperationControl::with_clock(clock);
        let child = parent.child().with_deadline(Duration::from_millis(1));

        parent.cancel();
        assert_eq!(
            child
                .checkpoint_at(OperationPhase::Parse)
                .unwrap_err()
                .reason,
            CancelReason::Requested
        );
        now.store(2, Ordering::Relaxed);
        assert_eq!(
            child
                .checkpoint_at(OperationPhase::Layout)
                .unwrap_err()
                .reason,
            CancelReason::Requested
        );
    }

    #[test]
    fn child_rebinds_an_observed_parent_cancellation_to_its_checkpoint_phase() {
        let parent = OperationControl::new();
        parent.cancel();
        let parent_error = parent
            .checkpoint_at(OperationPhase::Parse)
            .expect_err("the parent cancellation must be observed");
        let child = parent.child();

        let child_error = child
            .checkpoint_at(OperationPhase::Layout)
            .expect_err("the child must observe the parent cancellation");

        assert_eq!(parent_error.reason, CancelReason::Requested);
        assert_eq!(parent_error.phase, OperationPhase::Parse);
        assert_eq!(child_error.reason, parent_error.reason);
        assert_eq!(child_error.phase, OperationPhase::Layout);
        assert_eq!(
            parent.checkpoint_at(OperationPhase::Emit).unwrap_err(),
            parent_error
        );
        assert_eq!(
            child.checkpoint_at(OperationPhase::Emit).unwrap_err(),
            child_error
        );
    }

    #[test]
    fn child_rebinds_an_observed_parent_deadline_to_its_checkpoint_phase() {
        let parent = OperationControl::new().with_deadline(Duration::ZERO);
        let parent_error = parent
            .checkpoint_at(OperationPhase::Parse)
            .expect_err("the parent deadline must be observed");
        let child = parent.child();

        let child_error = child
            .checkpoint_at(OperationPhase::Layout)
            .expect_err("the child must observe the parent deadline");

        assert_eq!(parent_error.reason, CancelReason::DeadlineExceeded);
        assert_eq!(parent_error.phase, OperationPhase::Parse);
        assert_eq!(child_error.reason, parent_error.reason);
        assert_eq!(child_error.phase, OperationPhase::Layout);
        assert_eq!(
            parent.checkpoint_at(OperationPhase::Emit).unwrap_err(),
            parent_error
        );
        assert_eq!(
            child.checkpoint_at(OperationPhase::Emit).unwrap_err(),
            child_error
        );
    }

    #[test]
    fn child_keeps_its_requested_reason_after_parent_deadline_expires() {
        let now = Arc::new(AtomicU64::new(0));
        let source = Arc::clone(&now);
        let base = Instant::now();
        let clock = Arc::new(move || base + Duration::from_millis(source.load(Ordering::Relaxed)));
        let parent = OperationControl::with_clock(clock).with_deadline(Duration::from_millis(1));
        let child = parent.child();

        child.cancel();
        assert_eq!(
            child
                .checkpoint_at(OperationPhase::Parse)
                .unwrap_err()
                .reason,
            CancelReason::Requested
        );
        now.store(2, Ordering::Relaxed);
        assert_eq!(
            child
                .checkpoint_at(OperationPhase::Layout)
                .unwrap_err()
                .reason,
            CancelReason::Requested
        );
    }

    #[test]
    fn child_latches_an_expired_parent_deadline() {
        let now = Arc::new(AtomicU64::new(0));
        let source = Arc::clone(&now);
        let base = Instant::now();
        let clock = Arc::new(move || base + Duration::from_millis(source.load(Ordering::Relaxed)));
        let parent = OperationControl::with_clock(clock).with_deadline(Duration::from_millis(1));
        let child = parent.child();

        now.store(2, Ordering::Relaxed);
        assert_eq!(
            child
                .checkpoint_at(OperationPhase::Parse)
                .unwrap_err()
                .reason,
            CancelReason::DeadlineExceeded
        );
        parent.cancel();
        assert_eq!(
            child
                .checkpoint_at(OperationPhase::Layout)
                .unwrap_err()
                .reason,
            CancelReason::DeadlineExceeded
        );
    }

    #[test]
    fn child_keeps_an_observed_parent_cancellation_when_parent_resource_wins_the_latch() {
        let parent = OperationControl::new();
        let child = parent.child();
        let cancellation = OperationCancelled {
            phase: OperationPhase::Parse,
            reason: CancelReason::Requested,
        };
        let parent_terminal = OperationLedgerError::ArithmeticOverflow {
            id: "parent_work",
            phase: OperationPhase::Layout,
            resource_phase: "layout",
            actual: u64::MAX,
            maximum: u64::MAX,
            provenance: test_resource_provenance(),
        };

        parent.cancel();
        assert_eq!(
            parent.latch_terminal_error(parent_terminal.clone()),
            parent_terminal
        );
        let observed = OperationControl::resolve_latched_cancellation(
            parent.state.as_ref(),
            true,
            cancellation,
        )
        .expect("an ancestor resource terminal must not hide an observed cancellation");
        assert_eq!(observed, cancellation);
        assert_eq!(
            child.latch_terminal_error(OperationLedgerError::Cancelled(observed)),
            OperationLedgerError::Cancelled(cancellation)
        );
        assert_eq!(
            child
                .terminal_checkpoint_at(OperationPhase::Emit)
                .expect_err("the child must replay its cancellation"),
            OperationLedgerError::Cancelled(cancellation)
        );
        assert_eq!(
            parent
                .terminal_checkpoint_at(OperationPhase::Emit)
                .expect_err("the parent must retain its resource terminal"),
            parent_terminal
        );
    }

    #[test]
    fn child_latches_parent_deadline_when_parent_resource_wins_during_clock_read() {
        let now = Arc::new(AtomicU64::new(0));
        let clock_now = Arc::clone(&now);
        let parent_slot = Arc::new(OnceLock::<OperationControl>::new());
        let clock_parent = Arc::clone(&parent_slot);
        let base = Instant::now();
        let parent_terminal = OperationLedgerError::ArithmeticOverflow {
            id: "parent_work",
            phase: OperationPhase::Layout,
            resource_phase: "layout",
            actual: u64::MAX,
            maximum: u64::MAX,
            provenance: test_resource_provenance(),
        };
        let clock_terminal = parent_terminal.clone();
        let clock = Arc::new(move || {
            if let Some(parent) = clock_parent.get() {
                parent.latch_terminal_error(clock_terminal.clone());
            }
            base + Duration::from_millis(clock_now.load(Ordering::Relaxed))
        });
        let parent = OperationControl::with_clock(clock).with_deadline(Duration::from_millis(1));
        assert!(parent_slot.set(parent.clone()).is_ok());
        let child = parent.child();

        now.store(2, Ordering::Relaxed);
        let cancellation = child
            .terminal_checkpoint_at(OperationPhase::Parse)
            .expect_err("the child must observe the expired parent deadline");
        assert_eq!(
            cancellation,
            OperationLedgerError::Cancelled(OperationCancelled {
                phase: OperationPhase::Parse,
                reason: CancelReason::DeadlineExceeded,
            })
        );
        assert_eq!(
            child
                .terminal_checkpoint_at(OperationPhase::Emit)
                .expect_err("the child must replay its cancellation"),
            cancellation
        );
        assert_eq!(
            parent
                .terminal_checkpoint_at(OperationPhase::Emit)
                .expect_err("the parent must retain its resource terminal"),
            parent_terminal
        );
    }

    #[test]
    fn oversized_deadline_clamps_without_panicking_or_expiring_immediately() {
        let control = OperationControl::new().with_deadline(Duration::MAX);
        assert!(control.state.deadline.get().is_some());
        assert_eq!(control.checkpoint(), Ok(()));
    }

    #[test]
    fn concurrent_checkpoints_replay_one_complete_cancellation() {
        const THREADS: usize = 16;
        for _ in 0..32 {
            let control = OperationControl::new();
            control.cancel();
            let barrier = Arc::new(Barrier::new(THREADS));
            let handles = (0..THREADS)
                .map(|index| {
                    let control = control.clone();
                    let barrier = Arc::clone(&barrier);
                    thread::spawn(move || {
                        barrier.wait();
                        let phase = if index % 2 == 0 {
                            OperationPhase::Parse
                        } else {
                            OperationPhase::Layout
                        };
                        control.checkpoint_at(phase)
                    })
                })
                .collect::<Vec<_>>();

            let mut first = None;
            for handle in handles {
                let cancelled = handle
                    .join()
                    .expect("checkpoint thread should not panic")
                    .expect_err("every concurrent observer must see cancellation");
                assert_eq!(cancelled.reason, CancelReason::Requested);
                if let Some(first) = first {
                    assert_eq!(cancelled, first);
                } else {
                    first = Some(cancelled);
                }
            }
        }
    }

    #[test]
    fn operation_replays_the_first_resource_rejection_after_later_cancellation() {
        let control = OperationControl::new();
        let first = control.terminate_resource_limit(OperationResourceLimitExceeded {
            id: "work",
            phase: OperationPhase::Layout,
            resource_phase: "layout",
            limit: 2,
            consumed: 2,
            requested: 1,
            provenance: test_resource_provenance(),
        });
        assert_eq!(
            first,
            OperationLedgerError::ResourceLimitExceeded(OperationResourceLimitExceeded {
                id: "work",
                phase: OperationPhase::Layout,
                resource_phase: "layout",
                limit: 2,
                consumed: 2,
                requested: 1,
                provenance: test_resource_provenance(),
            })
        );

        let clone = control.clone();
        let child = control.child();
        control.cancel();
        assert_eq!(clone.checkpoint_at(OperationPhase::Emit), Ok(()));
        assert_eq!(
            clone
                .terminal_checkpoint_at(OperationPhase::Emit)
                .expect_err("the complete checkpoint must replay the resource terminal"),
            first
        );
        let cancellation = child
            .checkpoint_at(OperationPhase::Postprocess)
            .expect_err("a child has a distinct terminal scope and observes parent cancellation");
        assert_eq!(cancellation.reason, CancelReason::Requested);
        assert_eq!(cancellation.phase, OperationPhase::Postprocess);
        assert_eq!(
            child
                .terminal_checkpoint_at(OperationPhase::Emit)
                .expect_err("the child's complete checkpoint replays its cancellation"),
            OperationLedgerError::Cancelled(cancellation)
        );
        assert_eq!(
            control.terminate_resource_overflow(
                "later_work",
                OperationPhase::Emit,
                "emit",
                u64::MAX,
                u64::MAX,
                test_resource_provenance(),
            ),
            first
        );
    }

    #[test]
    fn child_control_starts_a_distinct_terminal_scope() {
        let parent = OperationControl::new();
        let parent_terminal = parent.terminate_resource_limit(OperationResourceLimitExceeded {
            id: "parent_work",
            phase: OperationPhase::Layout,
            resource_phase: "layout",
            limit: 0,
            consumed: 0,
            requested: 1,
            provenance: test_resource_provenance(),
        });

        let child = parent.child();
        assert_eq!(child.terminal_checkpoint_at(OperationPhase::Emit), Ok(()));
        let child_terminal = child.terminate_resource_limit(OperationResourceLimitExceeded {
            id: "child_work",
            phase: OperationPhase::Emit,
            resource_phase: "emit",
            limit: 0,
            consumed: 0,
            requested: 1,
            provenance: test_resource_provenance(),
        });
        assert_ne!(child_terminal, parent_terminal);
        assert_eq!(
            parent
                .terminal_checkpoint_at(OperationPhase::Layout)
                .unwrap_err(),
            parent_terminal
        );
        assert_eq!(
            child
                .terminal_checkpoint_at(OperationPhase::Emit)
                .unwrap_err(),
            child_terminal
        );
    }

    #[test]
    fn operation_replays_cancellation_before_later_resource_failure() {
        let control = OperationControl::new();
        control.cancel();

        let first = control
            .terminal_checkpoint_at(OperationPhase::Parse)
            .unwrap_err();
        assert_eq!(
            first,
            OperationLedgerError::Cancelled(OperationCancelled {
                phase: OperationPhase::Parse,
                reason: CancelReason::Requested,
            })
        );
        assert_eq!(
            control.terminate_resource_limit(OperationResourceLimitExceeded {
                id: "work",
                phase: OperationPhase::Emit,
                resource_phase: "emit",
                limit: 0,
                consumed: 0,
                requested: 1,
                provenance: test_resource_provenance(),
            }),
            first
        );
        assert_eq!(
            control
                .terminal_checkpoint_at(OperationPhase::Postprocess)
                .unwrap_err(),
            first
        );
    }

    #[test]
    fn operation_replays_the_first_arithmetic_overflow() {
        let control = OperationControl::new();
        let first = control.terminate_resource_overflow(
            "layout_work",
            OperationPhase::Layout,
            "layout",
            u64::MAX,
            u64::MAX,
            test_resource_provenance(),
        );
        assert_eq!(
            first,
            OperationLedgerError::ArithmeticOverflow {
                id: "layout_work",
                phase: OperationPhase::Layout,
                resource_phase: "layout",
                actual: u64::MAX,
                maximum: u64::MAX,
                provenance: test_resource_provenance(),
            }
        );
        assert_eq!(
            control.terminate_resource_limit(OperationResourceLimitExceeded {
                id: "output_bytes",
                phase: OperationPhase::Emit,
                resource_phase: "emit",
                limit: 0,
                consumed: 0,
                requested: 1,
                provenance: test_resource_provenance(),
            }),
            first
        );
        assert_eq!(
            control
                .terminal_checkpoint_at(OperationPhase::Postprocess)
                .unwrap_err(),
            first
        );
    }

    #[test]
    fn concurrent_resource_and_cancellation_observers_replay_one_terminal_error() {
        for _ in 0..32 {
            let control = OperationControl::new();
            let barrier = Arc::new(Barrier::new(2));

            let resource = {
                let control = control.clone();
                let barrier = Arc::clone(&barrier);
                thread::spawn(move || {
                    barrier.wait();
                    control.terminate_resource_limit(OperationResourceLimitExceeded {
                        id: "race_work",
                        phase: OperationPhase::Layout,
                        resource_phase: "layout",
                        limit: 0,
                        consumed: 0,
                        requested: 1,
                        provenance: test_resource_provenance(),
                    })
                })
            };
            let cancellation = {
                let control = control.clone();
                let barrier = Arc::clone(&barrier);
                thread::spawn(move || {
                    barrier.wait();
                    control.cancel();
                    control.terminal_checkpoint_at(OperationPhase::Emit)
                })
            };

            let resource = resource.join().expect("resource thread should not panic");
            let cancellation = cancellation
                .join()
                .expect("cancellation thread should not panic")
                .unwrap_err();
            assert_eq!(resource, cancellation);
            assert_eq!(
                control
                    .terminal_checkpoint_at(OperationPhase::Postprocess)
                    .unwrap_err(),
                resource
            );
        }
    }
}

#[cfg(test)]
mod native_memory_transport_tests {
    use super::*;
    use markdown_memory::{AllocationAuthority, Family, MemoryError, MemoryScope};
    use std::alloc::Layout;
    use std::sync::atomic::AtomicBool;

    struct Authority(AtomicBool);
    impl AllocationAuthority for Authority {
        fn reserve(&self, _: Layout, _: Family) -> Result<(), MemoryError> {
            Ok(())
        }
        fn release(&self, _: Layout, _: Family) {}
        fn failure(&self) -> Option<MemoryError> {
            self.0
                .load(Ordering::Acquire)
                .then_some(MemoryError::BudgetLimited)
        }
        fn record_failure(&self, _: MemoryError) {
            self.0.store(true, Ordering::Release);
        }
    }

    #[test]
    fn clone_phase_and_child_keep_the_exact_memory_authority() {
        let scope = MemoryScope::new(Arc::new(Authority(AtomicBool::new(false))));
        let root = OperationControl::new().with_memory_scope(scope.clone());
        for control in [
            root.clone(),
            root.child(),
            root.for_phase(OperationPhase::Parse),
        ] {
            assert!(control.memory_scope().unwrap().same_root(&scope));
        }
        assert!(OperationControl::new().memory_scope().is_none());
    }

    #[test]
    fn fixed_memory_failure_is_observed_even_when_attention_fast_path_was_clear() {
        let scope = MemoryScope::new(Arc::new(Authority(AtomicBool::new(false))));
        let root = OperationControl::new().with_memory_scope(scope.clone());
        let phase = root.for_phase(OperationPhase::Semantic);
        assert!(phase.checkpoint().is_ok());
        scope.record_failure(MemoryError::BudgetLimited);
        assert_eq!(
            phase.checkpoint(),
            Err(OperationCancelled {
                phase: OperationPhase::Semantic,
                reason: CancelReason::Requested
            })
        );
        assert!(root.is_cancelled());
        assert_eq!(scope.failure(), Some(MemoryError::BudgetLimited));
    }
}
