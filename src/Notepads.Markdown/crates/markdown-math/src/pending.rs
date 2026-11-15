// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE file in the project root for license information.
// ---------------------------------------------------------------------------------------------

//! Private production-integration mechanism. No publication authority or stack profile is
//! supplied here. The Windows owner must observe the real thread handle signaled before
//! accessing computed data, charging native storage, finalizing, or destroying the job.

use markdown_core::{
    ir::{DerivedKind, ObjectRow},
    model::AcceptedModel,
    resources::derived::{ControlPolicy, DerivedError, JobGrant, RetainedGrant},
};
use ratex_types::storage::StorageFamily;
pub use ratex_types::{
    execution::ExecutionError,
    storage::{StorageError, StorageSnapshot},
};
use std::{
    sync::atomic::AtomicBool,
    time::{Duration, Instant},
};

use crate::{
    ExecutionControl, JobFunding, MathArtifactData, MathError, MathLimits, MathMode,
    OwnedMathArtifact, Phase, WorkerStackContract, fonts::FontAssets, render::render_seeded,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PendingMathState {
    Prepared,
    Computed,
    FinishAttempted,
    Finalized,
}

struct PendingReceipt;
struct PendingFunding<'a> {
    grant: &'a JobGrant,
    reported_actual: &'a mut u64,
}
impl JobFunding for PendingFunding<'_> {
    type RetainedLease = PendingReceipt;
    fn reserved_bytes(&self) -> u64 {
        self.grant.reserved_bytes()
    }
    fn split_result(&mut self, actual: u64) -> Result<PendingReceipt, MathError> {
        if actual == 0 || actual > 32 * 1024 * 1024 || actual > self.grant.reserved_bytes() {
            return Err(MathError::InvalidFunding);
        }
        // This receipt cannot release or resize the outer owner's real lease or worker slot.
        *self.reported_actual = actual;
        Ok(PendingReceipt)
    }
}

/// The value is admitted before its caller allocates its enclosing private FFI/native Slot.
/// All actual result/model owners are declared before the grant, which is dropped last.
/// Methods after `run` require exclusive post-join access supplied by the private host.
pub struct DeferredMathJob {
    artifact: Option<OwnedMathArtifact<PendingReceipt>>,
    model: Option<AcceptedModel>,
    object_index: usize,
    mode: MathMode,
    state: PendingMathState,
    error: Option<MathError>,
    cumulative: StorageSnapshot,
    paid_metadata: u64,
    native_charged_bytes: u64,
    computed_rust_bytes: u64,
    deadline: Instant,
    grant: Option<JobGrant>,
}

impl DeferredMathJob {
    /// `host_metadata_bytes` is the bounded Slot/native header Layout, excluding Self.
    /// It must be reported by the private allocation authority before actual allocation.
    pub fn prepare(
        model: AcceptedModel,
        policy: &ControlPolicy,
        object_index: u32,
        host_metadata_bytes: u64,
        cancelled: &AtomicBool,
        deadline: Instant,
    ) -> Result<Self, MathError> {
        let control = ExecutionControl::new(cancelled, Some(deadline), MathLimits.execution());
        control.checkpoint(Phase::Admission)?;
        let now = Instant::now();
        if deadline <= now {
            return Err(MathError::Execution(
                ratex_types::execution::ExecutionError::Deadline {
                    phase: Phase::Admission,
                },
            ));
        }
        if deadline
            > now
                .checked_add(Duration::from_secs(5))
                .ok_or(MathError::InvalidLimits)?
        {
            return Err(MathError::InvalidLimits);
        }
        let (_, mode) = input(&model, object_index as usize)?;
        let paid_metadata = (size_of::<Self>() as u64)
            .checked_add(host_metadata_bytes)
            .ok_or(MathError::InvalidFunding)?;
        let grant = policy.acquire_job(&model)?;
        let mut cumulative = StorageSnapshot::default();
        cumulative.charge(StorageFamily::Metadata, paid_metadata, 0)?;
        control.checkpoint(Phase::Admission)?;
        Ok(Self {
            artifact: None,
            model: Some(model),
            object_index: object_index as usize,
            mode,
            state: PendingMathState::Prepared,
            error: None,
            cumulative,
            paid_metadata,
            native_charged_bytes: paid_metadata,
            computed_rust_bytes: 0,
            deadline,
            grant: Some(grant),
        })
    }

    pub fn state(&self) -> PendingMathState {
        self.state
    }
    pub fn reserved_bytes(&self) -> u64 {
        self.grant.as_ref().map_or(0, JobGrant::reserved_bytes)
    }
    pub fn error(&self) -> Option<MathError> {
        self.error
    }
    pub fn cumulative_snapshot(&self) -> StorageSnapshot {
        self.cumulative
    }
    pub fn prepaid_metadata_bytes(&self) -> u64 {
        self.paid_metadata
    }
    pub fn computed_rust_bytes(&self) -> u64 {
        self.computed_rust_bytes
    }

    /// Unique synchronous worker invocation. Control is constructed here, with the same
    /// external stable flag and fixed absolute deadline; there is no observer/callback input.
    /// The private FFI fixes flag identity and keeps its allocation as a disjoint sibling,
    /// so concurrent cancellation never borrows this entire mutably borrowed job.
    #[inline(never)]
    pub fn run(
        &mut self,
        cancelled: &AtomicBool,
        fonts: &FontAssets,
        limits: &MathLimits,
        stack: WorkerStackContract,
    ) -> Result<(), MathError> {
        self.run_with_chinese_text(cancelled, fonts, limits, stack, None)
    }

    #[doc(hidden)]
    #[inline(never)]
    pub fn run_with_chinese_text(
        &mut self,
        cancelled: &AtomicBool,
        fonts: &FontAssets,
        limits: &MathLimits,
        stack: WorkerStackContract,
        shaper: Option<&mut dyn crate::chinese_text::ChineseTextShaper>,
    ) -> Result<(), MathError> {
        if self.state != PendingMathState::Prepared {
            return Err(invalid_state());
        }
        // Mark before any fallible action so all failure paths are terminal for execution.
        self.state = PendingMathState::Computed;
        let result = (|| {
            let model = self.model.as_ref().ok_or(MathError::InternalInvariant)?;
            let (tex, mode) = input(model, self.object_index)?;
            if mode != self.mode {
                return Err(MathError::InternalInvariant);
            }
            let control = ExecutionControl::new(cancelled, Some(self.deadline), limits.execution());
            let grant = self.grant.as_ref().ok_or(MathError::InternalInvariant)?;
            render_seeded(
                tex,
                mode,
                &control,
                limits,
                fonts,
                PendingFunding {
                    grant,
                    reported_actual: &mut self.computed_rust_bytes,
                },
                stack,
                shaper,
                &mut self.cumulative,
            )
        })();
        match result {
            Ok(artifact) => {
                self.artifact = Some(artifact);
                Ok(())
            }
            Err(error) => {
                self.error = Some(error);
                Err(error)
            }
        }
    }

    pub fn data(&self) -> Result<&MathArtifactData, MathError> {
        if self.state != PendingMathState::Computed {
            return Err(invalid_state());
        }
        if let Some(error) = self.error {
            return Err(error);
        }
        self.artifact
            .as_ref()
            .map(OwnedMathArtifact::data)
            .ok_or(MathError::InternalInvariant)
    }

    /// Post-join native allocation admission, before the actual allocation. No refund API:
    /// freed copies and failed allocations still consumed cumulative request capacity.
    pub fn charge_native(
        &mut self,
        cancelled: &AtomicBool,
        family: StorageFamily,
        bytes: u64,
        slots: u64,
    ) -> Result<(), MathError> {
        if self.state != PendingMathState::Computed {
            return Err(invalid_state());
        }
        if let Some(error) = self.error {
            return Err(error);
        }
        if !matches!(
            family,
            StorageFamily::Path | StorageFamily::Mirror | StorageFamily::Metadata
        ) {
            return Err(MathError::InvalidFunding);
        }
        let control = ExecutionControl::new(cancelled, Some(self.deadline), MathLimits.execution());
        control.checkpoint(Phase::Admission)?;
        let total = self
            .native_charged_bytes
            .checked_add(bytes)
            .ok_or(MathError::InvalidFunding)?;
        self.cumulative.charge(family, bytes, slots)?;
        self.native_charged_bytes = total;
        Ok(())
    }

    /// Exactly one post-join finalization attempt. Native caller owns all still-live native
    /// buffers and includes their full known capacity plus surviving headers in `actual`.
    /// This method cannot verify those external allocations or prove thread termination.
    /// On failure the caller must free native buffers before dropping this still-funded job.
    pub fn finalize_native(
        &mut self,
        cancelled: &AtomicBool,
        actual: u64,
    ) -> Result<RetainedGrant, MathError> {
        if self.state != PendingMathState::Computed {
            return Err(invalid_state());
        }
        self.state = PendingMathState::FinishAttempted;
        // Actual Rust payloads and their model pin are destroyed BEFORE any lease shrink.
        drop(self.artifact.take());
        drop(self.model.take());
        if let Some(error) = self.error {
            return Err(error);
        }
        if let Some(error) = self.cumulative.failure() {
            return Err(MathError::Storage(error));
        }
        let control = ExecutionControl::new(cancelled, Some(self.deadline), MathLimits.execution());
        control.checkpoint(Phase::Admission)?;
        if actual < self.paid_metadata || actual > self.native_charged_bytes {
            return Err(MathError::InvalidFunding);
        }
        let retained = self
            .grant
            .as_mut()
            .ok_or(MathError::InternalInvariant)?
            .retain_result(actual)?;
        // Host called only after signaled; native construction is now complete. This is
        // the first permitted point at which the real actual-job permit can be released.
        drop(self.grant.take());
        self.state = PendingMathState::Finalized;
        Ok(retained)
    }
}

fn invalid_state() -> MathError {
    MathError::Funding(DerivedError::InvalidState)
}
fn input(model: &AcceptedModel, object_index: usize) -> Result<(&str, MathMode), MathError> {
    let Some(ObjectRow::Derived {
        kind: DerivedKind::Math(syntax),
        input,
        ..
    }) = model.ir().objects().get(object_index)
    else {
        return Err(MathError::Unsupported);
    };
    let mode = if syntax.is_display() {
        MathMode::Display
    } else {
        MathMode::Inline
    };
    let payload = model
        .ir()
        .payloads()
        .get(input.index())
        .ok_or(MathError::InternalInvariant)?;
    let tex = model
        .ir()
        .text(payload.value)
        .map_err(|_| MathError::InternalInvariant)?;
    Ok((tex, mode))
}

#[cfg(test)]
mod tests;
