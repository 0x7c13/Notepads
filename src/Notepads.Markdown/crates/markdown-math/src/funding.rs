// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE file in the project root for license information.
// ---------------------------------------------------------------------------------------------

//! Direct funding harness for exercising the math pipeline with a real Core job grant.

use markdown_core::{
    model::AcceptedModel,
    resources::derived::{ControlPolicy, JobGrant, RetainedGrant},
};

use crate::{
    ExecutionControl, JobFunding, MathError, MathLimits, MathMode, OwnedMathArtifact, Phase,
    WorkerStackContract, fonts::FontAssets, render,
};

/// A real Core job scope, not a caller-provided reservation count. It holds the
/// worker permit through the complete synchronous render and funding teardown.
/// Splitting transfers only result bytes; the permit stays in JobGrant.
struct ProductionFunding {
    grant: JobGrant,
}

impl JobFunding for ProductionFunding {
    type RetainedLease = RetainedGrant;

    fn reserved_bytes(&self) -> u64 {
        self.grant.reserved_bytes()
    }

    fn split_result(&mut self, actual_retained_bytes: u64) -> Result<RetainedGrant, MathError> {
        self.grant
            .retain_result(actual_retained_bytes)
            .map_err(MathError::Funding)
    }
}

/// Internal qualification bridge, intentionally not a production WinRT/FFI API.
/// The host must never obtain a stack contract from application-supplied values.
/// An observer can execute arbitrary reentrant code, so it is rejected before
/// even the first checkpoint. Unknown/mismatched/insufficient stack is rejected
/// before the real 64-MiB admission and before entering the parser.
#[inline(never)]
#[expect(
    clippy::too_many_arguments,
    reason = "Keep accepted source, policy, and each approved execution input explicit at this narrow bridge"
)]
pub(crate) fn experimental_render_funded(
    model: &AcceptedModel,
    policy: &ControlPolicy,
    tex: &str,
    mode: MathMode,
    control: &ExecutionControl<'_>,
    limits: &MathLimits,
    fonts: &FontAssets,
    stack: WorkerStackContract,
) -> Result<OwnedMathArtifact<RetainedGrant>, MathError> {
    if control.has_observer() {
        return Err(MathError::ObserverNotAllowed);
    }
    control.checkpoint(Phase::Admission)?;
    stack.admit()?;
    let grant = policy.acquire_job(model).map_err(MathError::Funding)?;
    // Capacity admission does not reset or substitute the math operation's
    // cancel/deadline. On this error the real grant rolls back by ordinary Drop.
    control.checkpoint(Phase::Admission)?;
    render(
        tex,
        mode,
        control,
        limits,
        fonts,
        ProductionFunding { grant },
        stack,
    )
}

mod tests;
