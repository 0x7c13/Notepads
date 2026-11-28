// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE file in the project root for license information.
// ---------------------------------------------------------------------------------------------

//! Real model/root/policy integration. Mechanism-only stack values below are
//! local to cfg(test); they are not an audited profile or production defaults.

use super::*;
use crate::{MathPathCommand, Primitive};
use markdown_core::{
    config::{Extensions, ParseConfig, Profile},
    error::{CoreError, LimitKind},
    identity::SourceKey,
    ir::{DerivedKind, MathSyntax, ObjectRow},
    policy::{CancellationToken, SourceBudget},
    resources::derived::{DerivedError, DerivedLimit, JOB_BYTES, ProcessPolicy, Usage},
    source::SourceDocument,
};
use ratex_types::execution::{ExecutionError, Observer};
use std::{
    mem::size_of,
    sync::{Arc, atomic::AtomicBool},
    time::{Duration, Instant},
};

fn accepted_model(root: &SourceBudget) -> AcceptedModel {
    let mut operation = root.begin(CancellationToken::new());
    let source = SourceDocument::new(
        SourceKey::new([203; 16], 1).unwrap(),
        br"$$x^2$$",
        &mut operation,
    )
    .unwrap();
    let model = AcceptedModel::parse(
        source.snapshot(),
        ParseConfig::with_extensions(Profile::G3, Extensions::MATH),
        1,
        &mut operation,
    )
    .unwrap();
    assert_eq!(model.ir().objects().len(), 1);
    assert_eq!(math_input(&model), "x^2");
    model
}

fn math_input(model: &AcceptedModel) -> &str {
    let ObjectRow::Derived { kind, input, .. } = model.ir().objects()[0] else {
        panic!("expected a real accepted B0 math object");
    };
    assert_eq!(kind, DerivedKind::Math(MathSyntax::DollarDisplay));
    model
        .ir()
        .text(model.ir().payloads()[input.index()].value)
        .unwrap()
}

fn mechanism_only_stack() -> WorkerStackContract {
    WorkerStackContract {
        usable_headroom_bytes: 128 * 1024,
        platform_margin_bytes: 64 * 1024,
        audited_required_bytes: Some(1),
    }
}

fn live_usage(
    root: &SourceBudget,
    process: &ProcessPolicy,
    control: &ControlPolicy,
) -> (u64, u64, u64, Usage, Usage) {
    let source = root.usage();
    (
        source.source_capacity,
        source.auxiliary_capacity,
        source.source_owners,
        process.usage(),
        control.usage(),
    )
}

#[test]
fn real_formula_retains_exact_buffers_and_the_last_payload_owner_releases_all_grants() {
    let root = SourceBudget::preview();
    let metadata = SourceBudget::preview();
    let model = accepted_model(&root);
    let process = ProcessPolicy::new(&metadata).unwrap();
    let policy = ControlPolicy::new(&process, &metadata).unwrap();
    let base = live_usage(&root, &process, &policy);
    let cancel = AtomicBool::new(false);
    let control = ExecutionControl::new(
        &cancel,
        Some(Instant::now() + Duration::from_secs(5)),
        MathLimits.execution(),
    );
    let artifact = experimental_render_funded(
        &model,
        &policy,
        math_input(&model),
        MathMode::Display,
        &control,
        &MathLimits,
        &FontAssets::new(),
        mechanism_only_stack(),
    )
    .unwrap();
    assert!(!artifact.data().commands.is_empty());
    assert!((artifact.data().metrics.ascent - 0.86411).abs() < 0.002);
    let actual = (artifact.data().commands.capacity() * size_of::<MathPathCommand>()
        + artifact.data().primitives.capacity() * size_of::<Primitive>()
        + size_of::<OwnedMathArtifact<RetainedGrant>>()) as u64;
    assert_eq!(root.usage().auxiliary_capacity, base.1 + actual);
    assert!(root.usage().auxiliary_capacity_peak >= base.1 + JOB_BYTES);
    assert_eq!(process.usage().bytes, base.3.bytes + actual);
    assert_eq!(policy.usage().bytes, base.4.bytes + actual);
    assert_eq!(policy.usage().retained_bytes, actual);
    assert_eq!(process.usage().retained_bytes, actual);
    assert_eq!(process.usage().jobs, 0);
    assert_eq!(policy.usage().jobs, 0);

    let first = Arc::new(artifact);
    let last = Arc::clone(&first);
    drop(first);
    drop(model); // RetainedGrant holds the source capacity lease, never an AcceptedModel pin.
    assert_eq!(root.usage().source_capacity, 0);
    assert_eq!(root.usage().source_owners, 0);
    assert_eq!(root.usage().auxiliary_capacity, actual);
    drop(policy); // Its shared quota owner remains alive behind the numerical payload.
    assert_eq!(process.usage().retained_bytes, actual);
    drop(last); // OwnedMathArtifact declares data before its retained grant.
    assert_eq!(root.usage().auxiliary_capacity, 0);
    assert_eq!(process.usage().retained_bytes, 0);
    assert_eq!(process.usage().jobs, 0);
    assert_eq!(process.usage().bytes, process.usage().metadata_bytes);
    assert_eq!(
        metadata.usage().auxiliary_capacity,
        process.usage().metadata_bytes
    );
    drop(process);
    assert_eq!(metadata.usage().auxiliary_capacity, 0);
}

#[test]
fn unknown_zero_and_insufficient_stack_cannot_touch_source_quota_or_parser() {
    let root = SourceBudget::preview();
    let metadata = SourceBudget::preview();
    let model = accepted_model(&root);
    let process = ProcessPolicy::new(&metadata).unwrap();
    let policy = ControlPolicy::new(&process, &metadata).unwrap();
    for stack in [
        WorkerStackContract::unknown(),
        WorkerStackContract {
            audited_required_bytes: Some(0),
            ..mechanism_only_stack()
        },
        WorkerStackContract {
            usable_headroom_bytes: 64 * 1024,
            ..mechanism_only_stack()
        },
    ] {
        let before = live_usage(&root, &process, &policy);
        let source_before = root.usage(); // Includes peaks: no transient 64-MiB acquisition.
        let cancel = AtomicBool::new(false);
        let control = ExecutionControl::new(
            &cancel,
            Some(Instant::now() + Duration::from_secs(5)),
            MathLimits.execution(),
        );
        let error = experimental_render_funded(
            &model,
            &policy,
            "x",
            MathMode::Display,
            &control,
            &MathLimits,
            &FontAssets::new(),
            stack,
        )
        .err()
        .unwrap();
        assert!(matches!(error, MathError::StackUnavailable { .. }));
        assert_eq!(live_usage(&root, &process, &policy), before);
        assert_eq!(root.usage(), source_before);
        assert_eq!(control.consumed_counts(), [0; 8]);
        assert_eq!(control.checkpoint_counts()[Phase::Parser as usize], 0);
        assert!(control.storage().is_none());
    }
}

#[test]
fn an_observer_that_would_panic_is_rejected_before_every_checkpoint_and_quota() {
    struct PanicObserver;
    impl Observer for PanicObserver {
        fn checkpoint(&self, _: Phase, _: u64) {
            panic!("The concrete funding bridge must never call this observer");
        }
    }
    let root = SourceBudget::preview();
    let metadata = SourceBudget::preview();
    let model = accepted_model(&root);
    let process = ProcessPolicy::new(&metadata).unwrap();
    let policy = ControlPolicy::new(&process, &metadata).unwrap();
    let before = live_usage(&root, &process, &policy);
    let source_before = root.usage();
    let cancel = AtomicBool::new(true); // Observer rejection still precedes cancel/deadline checks.
    let observer = PanicObserver;
    let control = ExecutionControl::new(&cancel, Some(Instant::now()), MathLimits.execution())
        .with_observer(&observer);
    let error = experimental_render_funded(
        &model,
        &policy,
        "x",
        MathMode::Display,
        &control,
        &MathLimits,
        &FontAssets::new(),
        WorkerStackContract::unknown(),
    )
    .err()
    .unwrap();
    assert_eq!(error, MathError::ObserverNotAllowed);
    assert_eq!(control.checkpoint_counts(), [0; 8]);
    assert_eq!(control.consumed_counts(), [0; 8]);
    assert_eq!(live_usage(&root, &process, &policy), before);
    assert_eq!(root.usage(), source_before);
}

#[test]
fn math_cancellation_and_deadline_before_admission_leave_no_quota_or_worker() {
    let root = SourceBudget::preview();
    let metadata = SourceBudget::preview();
    let model = accepted_model(&root);
    let process = ProcessPolicy::new(&metadata).unwrap();
    let policy = ControlPolicy::new(&process, &metadata).unwrap();
    for cancelled in [true, false] {
        let cancel = AtomicBool::new(cancelled);
        let deadline = if cancelled {
            Instant::now() + Duration::from_secs(5)
        } else {
            Instant::now()
        };
        let control = ExecutionControl::new(&cancel, Some(deadline), MathLimits.execution());
        let before = live_usage(&root, &process, &policy);
        let source_before = root.usage();
        let error = experimental_render_funded(
            &model,
            &policy,
            "x",
            MathMode::Display,
            &control,
            &MathLimits,
            &FontAssets::new(),
            mechanism_only_stack(),
        )
        .err()
        .unwrap();
        assert!(if cancelled {
            matches!(
                error,
                MathError::Execution(ExecutionError::Cancelled { .. })
            )
        } else {
            matches!(error, MathError::Execution(ExecutionError::Deadline { .. }))
        });
        assert_eq!(live_usage(&root, &process, &policy), before);
        assert_eq!(root.usage(), source_before);
    }
}

#[test]
fn concrete_source_and_policy_refusals_preserve_the_typed_funding_reason() {
    let root = SourceBudget::preview();
    let metadata = SourceBudget::preview();
    let model = accepted_model(&root);
    let process = ProcessPolicy::new(&metadata).unwrap();
    let policy = ControlPolicy::new(&process, &metadata).unwrap();
    let blocker = root
        .reserve_resource_capacity(
            root.limits().auxiliary_capacity - root.usage().auxiliary_capacity - JOB_BYTES + 1,
        )
        .unwrap();
    let cancel = AtomicBool::new(false);
    let control = ExecutionControl::new(
        &cancel,
        Some(Instant::now() + Duration::from_secs(5)),
        MathLimits.execution(),
    );
    let before = live_usage(&root, &process, &policy);
    let error = experimental_render_funded(
        &model,
        &policy,
        "x",
        MathMode::Display,
        &control,
        &MathLimits,
        &FontAssets::new(),
        mechanism_only_stack(),
    )
    .err()
    .unwrap();
    assert!(matches!(
        error,
        MathError::Funding(DerivedError::Source(CoreError::ResourceLimit {
            kind: LimitKind::AuxiliaryCapacity,
            ..
        }))
    ));
    assert_eq!(live_usage(&root, &process, &policy), before);
    drop(blocker);

    let existing = policy.acquire_job(&model).unwrap();
    let before = live_usage(&root, &process, &policy);
    let error = experimental_render_funded(
        &model,
        &policy,
        "x",
        MathMode::Display,
        &control,
        &MathLimits,
        &FontAssets::new(),
        mechanism_only_stack(),
    )
    .err()
    .unwrap();
    assert!(matches!(
        error,
        MathError::Funding(DerivedError::Limit {
            kind: DerivedLimit::ControlJobs,
            ..
        })
    ));
    assert_eq!(live_usage(&root, &process, &policy), before);
    drop(existing);
    assert_eq!(policy.usage().jobs, 0);
}

#[test]
fn parse_and_execution_errors_after_real_admission_roll_back_full_grants() {
    for input in [r"\def\a{x}\a", "x"] {
        let root = SourceBudget::preview();
        let metadata = SourceBudget::preview();
        let model = accepted_model(&root);
        let process = ProcessPolicy::new(&metadata).unwrap();
        let policy = ControlPolicy::new(&process, &metadata).unwrap();
        let cancel = AtomicBool::new(false);
        let mut limits = MathLimits.execution();
        if input == "x" {
            limits.values[crate::Resource::OutlineSegments as usize] = 0;
        }
        let control = ExecutionControl::new(
            &cancel,
            Some(Instant::now() + Duration::from_secs(5)),
            limits,
        );
        let before = live_usage(&root, &process, &policy);
        let error = experimental_render_funded(
            &model,
            &policy,
            input,
            MathMode::Display,
            &control,
            &MathLimits,
            &FontAssets::new(),
            mechanism_only_stack(),
        )
        .err()
        .unwrap();
        assert!(!matches!(error, MathError::Funding(_)));
        assert_eq!(live_usage(&root, &process, &policy), before);
        assert!(root.usage().auxiliary_capacity_peak >= before.1 + JOB_BYTES);
        assert_eq!(policy.usage().jobs, 0);
        assert_eq!(policy.usage().retained_bytes, 0);
    }
}

#[test]
fn concrete_split_keeps_the_original_scope_on_failure_and_render_observes_its_same_cancel() {
    let root = SourceBudget::preview();
    let metadata = SourceBudget::preview();
    let model = accepted_model(&root);
    let process = ProcessPolicy::new(&metadata).unwrap();
    let policy = ControlPolicy::new(&process, &metadata).unwrap();
    let base = live_usage(&root, &process, &policy);
    let mut funding = ProductionFunding {
        grant: policy.acquire_job(&model).unwrap(),
    };
    assert_eq!(funding.reserved_bytes(), JOB_BYTES);
    assert_eq!(
        funding.split_result(0).err().unwrap(),
        MathError::Funding(DerivedError::InvalidSize)
    );
    assert_eq!(funding.reserved_bytes(), JOB_BYTES);
    assert_eq!(root.usage().auxiliary_capacity, base.1 + JOB_BYTES);
    assert_eq!(policy.usage().jobs, 1);
    let cancel = AtomicBool::new(true); // The real grant exists before this math control is checked.
    let control = ExecutionControl::new(
        &cancel,
        Some(Instant::now() + Duration::from_secs(5)),
        MathLimits.execution(),
    );
    let error = render(
        "x",
        MathMode::Display,
        &control,
        &MathLimits,
        &FontAssets::new(),
        funding,
        mechanism_only_stack(),
    )
    .err()
    .unwrap();
    assert!(matches!(
        error,
        MathError::Execution(ExecutionError::Cancelled { .. })
    ));
    assert_eq!(live_usage(&root, &process, &policy), base);
}
