// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE file in the project root for license information.
// ---------------------------------------------------------------------------------------------

//! Mechanism tests only: no Windows thread or production stack qualification is asserted.

use super::*;
use markdown_core::{
    config::{Extensions, ParseConfig, Profile},
    identity::SourceKey,
    policy::{CancellationToken, SourceBudget},
    resources::derived::{JOB_BYTES, ProcessPolicy},
    source::SourceDocument,
};
use ratex_types::{
    execution::ExecutionError,
    storage::{FAMILY_BYTES, StorageError},
};
use std::sync::atomic::Ordering;

fn model(root: &SourceBudget, source: &[u8]) -> AcceptedModel {
    let mut op = root.begin(CancellationToken::new());
    let source =
        SourceDocument::new(SourceKey::new([211; 16], 1).unwrap(), source, &mut op).unwrap();
    AcceptedModel::parse(
        source.snapshot(),
        ParseConfig::with_extensions(Profile::G3, Extensions::MATH),
        1,
        &mut op,
    )
    .unwrap()
}
fn stack() -> WorkerStackContract {
    WorkerStackContract {
        usable_headroom_bytes: 128 * 1024,
        platform_margin_bytes: 64 * 1024,
        audited_required_bytes: Some(1),
    }
}
fn deadline() -> Instant {
    Instant::now() + Duration::from_secs(5)
}

#[test]
fn computation_keeps_real_64m_and_worker_until_single_postjoin_finalization() {
    let root = SourceBudget::preview();
    let metadata = SourceBudget::preview();
    let process = ProcessPolicy::new(&metadata).unwrap();
    let policy = ControlPolicy::new(&process, &metadata).unwrap();
    let cancel = AtomicBool::new(false);
    let mut job = DeferredMathJob::prepare(
        model(&root, br"$$x^2$$"),
        &policy,
        0,
        128,
        &cancel,
        deadline(),
    )
    .unwrap();
    assert_eq!(job.state(), PendingMathState::Prepared);
    assert_eq!(job.data().err(), Some(invalid_state()));
    let seed = job.cumulative_snapshot();
    assert_eq!(
        seed.used_bytes()[StorageFamily::Metadata as usize],
        job.prepaid_metadata_bytes()
    );
    job.run(&cancel, &FontAssets::new(), &MathLimits, stack())
        .unwrap();
    assert!(!job.data().unwrap().commands.is_empty());
    assert_eq!(job.reserved_bytes(), JOB_BYTES);
    assert_eq!(policy.usage().jobs, 1);
    assert_eq!(policy.usage().retained_bytes, 0);
    assert!(
        job.cumulative_snapshot().used_bytes()[StorageFamily::Metadata as usize]
            >= seed.used_bytes()[StorageFamily::Metadata as usize]
    );
    assert_eq!(
        job.run(&cancel, &FontAssets::new(), &MathLimits, stack()),
        Err(invalid_state())
    );

    // The small stand-in copy is allocated after admission; actual native registry/DTO
    // allocation and real handle-signaled enforcement belong to the host integration.
    job.charge_native(&cancel, StorageFamily::Mirror, 256, 0)
        .unwrap();
    let mut mirror = Vec::<u8>::new();
    mirror.try_reserve_exact(256).unwrap();
    assert_eq!(mirror.capacity(), 256);
    mirror.resize(256, 0);
    let actual = job.prepaid_metadata_bytes() + mirror.capacity() as u64;
    let retained = job.finalize_native(&cancel, actual).unwrap();
    assert_eq!(job.state(), PendingMathState::Finalized);
    assert!(job.artifact.is_none());
    assert!(job.model.is_none());
    assert_eq!(job.reserved_bytes(), 0);
    assert_eq!(policy.usage().jobs, 0);
    assert_eq!(policy.usage().retained_bytes, actual);
    assert_eq!(root.usage().source_capacity, 0);
    assert_eq!(root.usage().auxiliary_capacity, actual);
    assert_eq!(
        job.finalize_native(&cancel, actual).err(),
        Some(invalid_state())
    );
    drop(job);
    assert_eq!(policy.usage().retained_bytes, actual);
    drop(mirror); // Real stand-in CPU buffer is freed before its final capacity grant.
    drop(retained);
    assert_eq!(root.usage().auxiliary_capacity, 0);
    assert_eq!(policy.usage().retained_bytes, 0);
}

#[test]
fn all_four_modes_borrow_their_actual_same_model_payload() {
    for source in [br"$x^2$".as_slice(), br"$$x^2$$", br"\(x^2\)", br"\[x^2\]"] {
        let root = SourceBudget::preview();
        let metadata = SourceBudget::preview();
        let process = ProcessPolicy::new(&metadata).unwrap();
        let policy = ControlPolicy::new(&process, &metadata).unwrap();
        let model = model(&root, source);
        assert_eq!(model.ir().objects().len(), 1);
        assert_eq!(input(&model, 0).unwrap().0, "x^2");
        let cancel = AtomicBool::new(false);
        let mut job = DeferredMathJob::prepare(model, &policy, 0, 0, &cancel, deadline()).unwrap();
        job.run(&cancel, &FontAssets::new(), &MathLimits, stack())
            .unwrap();
        assert!(!job.data().unwrap().commands.is_empty());
        assert_eq!(policy.usage().jobs, 1);
        drop(job);
        assert_eq!(policy.usage().jobs, 0);
        assert_eq!(root.usage().auxiliary_capacity, 0);
    }
}

#[test]
fn cancellation_unknown_stack_and_parse_error_remain_funded_until_outer_drop() {
    for (source, cancel_after_prepare, admitted_stack) in [
        (br"$$x$$".as_slice(), true, stack()),
        (br"$$x$$".as_slice(), false, WorkerStackContract::unknown()),
        (br"$$\def\x{y}$$".as_slice(), false, stack()),
    ] {
        let root = SourceBudget::preview();
        let metadata = SourceBudget::preview();
        let process = ProcessPolicy::new(&metadata).unwrap();
        let policy = ControlPolicy::new(&process, &metadata).unwrap();
        let cancel = AtomicBool::new(false);
        let mut job =
            DeferredMathJob::prepare(model(&root, source), &policy, 0, 0, &cancel, deadline())
                .unwrap();
        cancel.store(cancel_after_prepare, Ordering::Release);
        let error = job
            .run(&cancel, &FontAssets::new(), &MathLimits, admitted_stack)
            .unwrap_err();
        assert_eq!(job.state(), PendingMathState::Computed);
        assert_eq!(job.data().err(), Some(error));
        assert_eq!(job.reserved_bytes(), JOB_BYTES);
        assert_eq!(policy.usage().jobs, 1);
        assert_eq!(policy.usage().retained_bytes, 0);
        assert_eq!(
            job.finalize_native(&cancel, job.prepaid_metadata_bytes())
                .err(),
            Some(error)
        );
        assert_eq!(job.reserved_bytes(), JOB_BYTES);
        assert_eq!(policy.usage().jobs, 1);
        drop(job);
        assert_eq!(policy.usage().jobs, 0);
        assert_eq!(root.usage().auxiliary_capacity, 0);
    }
}

#[test]
fn native_family_limits_and_first_failure_cannot_be_refunded_or_reset() {
    let root = SourceBudget::preview();
    let metadata = SourceBudget::preview();
    let process = ProcessPolicy::new(&metadata).unwrap();
    let policy = ControlPolicy::new(&process, &metadata).unwrap();
    let cancel = AtomicBool::new(false);
    let mut job =
        DeferredMathJob::prepare(model(&root, br"$$x$$"), &policy, 0, 0, &cancel, deadline())
            .unwrap();
    job.run(&cancel, &FontAssets::new(), &MathLimits, stack())
        .unwrap();
    let before = job.cumulative_snapshot();
    assert_eq!(
        job.charge_native(&cancel, StorageFamily::Ast, 1, 0),
        Err(MathError::InvalidFunding)
    );
    assert_eq!(job.cumulative_snapshot(), before);
    job.charge_native(&cancel, StorageFamily::Mirror, 64, 0)
        .unwrap();
    let charged = job.cumulative_snapshot();
    assert_eq!(
        charged.used_bytes()[StorageFamily::Mirror as usize],
        before.used_bytes()[StorageFamily::Mirror as usize] + 64
    );
    let error = job
        .charge_native(
            &cancel,
            StorageFamily::Mirror,
            FAMILY_BYTES[StorageFamily::Mirror as usize],
            0,
        )
        .unwrap_err();
    assert!(matches!(
        error,
        MathError::Storage(StorageError::FamilyBytes {
            family: StorageFamily::Mirror,
            ..
        })
    ));
    assert_eq!(
        job.charge_native(&cancel, StorageFamily::Metadata, 0, 0),
        Err(error)
    );
    assert_eq!(job.cumulative_snapshot().used_bytes(), charged.used_bytes());
    assert_eq!(
        job.finalize_native(&cancel, job.prepaid_metadata_bytes())
            .err(),
        Some(error)
    );
    assert_eq!(job.reserved_bytes(), JOB_BYTES);
    drop(job);
    assert_eq!(policy.usage().jobs, 0);
}

#[test]
fn invalid_native_actual_and_repeated_finish_keep_full_grant_on_first_failure() {
    let root = SourceBudget::preview();
    let metadata = SourceBudget::preview();
    let process = ProcessPolicy::new(&metadata).unwrap();
    let policy = ControlPolicy::new(&process, &metadata).unwrap();
    let cancel = AtomicBool::new(false);
    let mut job =
        DeferredMathJob::prepare(model(&root, br"$$x$$"), &policy, 0, 0, &cancel, deadline())
            .unwrap();
    job.run(&cancel, &FontAssets::new(), &MathLimits, stack())
        .unwrap();
    assert_eq!(
        job.finalize_native(&cancel, job.prepaid_metadata_bytes() + 1)
            .err(),
        Some(MathError::InvalidFunding)
    );
    assert!(job.artifact.is_none());
    assert!(job.model.is_none());
    assert_eq!(job.reserved_bytes(), JOB_BYTES);
    assert_eq!(policy.usage().jobs, 1);
    assert_eq!(
        job.finalize_native(&cancel, job.prepaid_metadata_bytes())
            .err(),
        Some(invalid_state())
    );
    drop(job);
    assert_eq!(policy.usage().jobs, 0);
    assert_eq!(root.usage().auxiliary_capacity, 0);
}

#[test]
fn precancelled_and_oversized_header_admission_never_leave_a_job_permit() {
    let root = SourceBudget::preview();
    let metadata = SourceBudget::preview();
    let process = ProcessPolicy::new(&metadata).unwrap();
    let policy = ControlPolicy::new(&process, &metadata).unwrap();
    let cancel = AtomicBool::new(true);
    assert!(matches!(
        DeferredMathJob::prepare(model(&root, br"$$x$$"), &policy, 0, 0, &cancel, deadline()).err(),
        Some(MathError::Execution(ExecutionError::Cancelled {
            phase: Phase::Admission
        }))
    ));
    assert_eq!(policy.usage().jobs, 0);
    cancel.store(false, Ordering::Release);
    assert!(matches!(
        DeferredMathJob::prepare(
            model(&root, br"$$x$$"),
            &policy,
            0,
            FAMILY_BYTES[StorageFamily::Metadata as usize],
            &cancel,
            deadline()
        )
        .err(),
        Some(MathError::Storage(StorageError::FamilyBytes {
            family: StorageFamily::Metadata,
            ..
        }))
    ));
    assert_eq!(policy.usage().jobs, 0);
    assert_eq!(root.usage().auxiliary_capacity, 0);
}
