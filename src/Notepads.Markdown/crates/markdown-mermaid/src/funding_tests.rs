// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE file in the project root for license information.
// ---------------------------------------------------------------------------------------------

//! Real Core funding mechanism tests; no Windows thread/profile qualification.

use super::*;
use markdown_memory::{NativeRegion, OwnedBlock};
use markdown_core::{
    config::{ParseConfig, Profile},
    identity::SourceKey,
    policy::{CancellationToken, SourceBudget},
    resources::derived::{JOB_BYTES, ProcessPolicy},
    source::SourceDocument,
};

fn accepted(root: &SourceBudget) -> AcceptedModel {
    let mut operation = root.begin(CancellationToken::new());
    let document = SourceDocument::new(
        SourceKey::new([214; 16], 1).unwrap(),
        b"```mermaid\nflowchart TD\nA --> B\n```",
        &mut operation,
    )
    .unwrap();
    // The common resource authority needs an actual accepted source, independently
    // of the feature's not-yet-enabled semantic product entry. Fence mapping and
    // public opt-in require their own integration tests; this does not assert it.
    AcceptedModel::parse(
        document.snapshot(),
        ParseConfig::new(Profile::G3),
        1,
        &mut operation,
    )
    .unwrap()
}

#[test]
fn actual_job_cannot_shrink_with_live_rust_allocation_and_retains_real_native_buffer() {
    let source = SourceBudget::preview();
    let metadata = SourceBudget::preview();
    let model = accepted(&source);
    let process = ProcessPolicy::new(&metadata).unwrap();
    let policy = ControlPolicy::new(&process, &metadata).unwrap();
    let queued = policy.reserve_metadata(&model, 16 * 1024).unwrap();
    let base = OperationControl::new();
    let memory = JobMemory::activate(&model, &policy, 4096, MAX_STACK_RESERVATION, base).unwrap();
    let scope = memory.scope();
    let payload = OwnedBlock::try_array::<u8>(&scope, 4096, Family::Graph).unwrap();
    assert_eq!(policy.usage().jobs, 1);
    assert_eq!(
        memory.lock().grant.as_ref().unwrap().reserved_bytes(),
        JOB_BYTES
    );
    assert!(matches!(
        memory.finalize_after_thread_exit(8192),
        Err(DiagramError::InvalidFunding)
    ));
    assert_eq!(policy.usage().jobs, 1);
    memory.charge_native(4096).unwrap();
    // Host stand-in performs its actual allocation only after real admission.
    let mut native = Vec::<u8>::new();
    native.try_reserve_exact(4096).unwrap();
    native.resize(4096, 0);
    drop(payload);
    let retained = memory
        .finalize_after_thread_exit(4096 + native.capacity() as u64)
        .unwrap();
    assert_eq!(policy.usage().jobs, 0);
    assert_eq!(policy.usage().retained_bytes, 8192);
    assert!(matches!(
        memory.finalize_after_thread_exit(8192),
        Err(DiagramError::InvalidFunding)
    ));
    drop(scope);
    drop(memory);
    drop(native); // True native stand-in storage is freed before its grant.
    drop(retained);
    assert_eq!(policy.usage().retained_bytes, 0);
    drop(queued);
}

#[test]
fn region_producer_and_payload_guards_hold_real_allocations_until_final_drop() {
    let source = SourceBudget::preview();
    let metadata = SourceBudget::preview();
    let model = accepted(&source);
    let process = ProcessPolicy::new(&metadata).unwrap();
    let policy = ControlPolicy::new(&process, &metadata).unwrap();
    let queued = policy.reserve_metadata(&model, 16 * 1024).unwrap();
    let memory = JobMemory::activate(
        &model,
        &policy,
        4096,
        MAX_STACK_RESERVATION,
        OperationControl::new(),
    )
    .unwrap();
    let scope = memory.scope();
    let region = NativeRegion::try_new(&scope, 32).unwrap();
    let text = region
        .try_string("实际 source → AST → semantic", Family::Parse)
        .unwrap();
    let payload = markdown_memory::NativePayload::new(text, region.clone());
    drop(region);
    assert!(memory.usage().live_rust_bytes > 0);
    assert!(matches!(
        memory.finalize_after_thread_exit(4096),
        Err(DiagramError::InvalidFunding)
    ));
    drop(payload);
    assert_eq!(memory.usage().live_rust_bytes, 0);
    let retained = memory.finalize_after_thread_exit(4096).unwrap();
    assert_eq!(policy.usage().jobs, 0);
    drop(scope);
    drop(memory);
    drop(retained);
    drop(queued);
}

#[test]
fn shared_authority_records_one_fixed_work_failure_without_owning_scoped_control() {
    let source = SourceBudget::preview();
    let metadata = SourceBudget::preview();
    let model = accepted(&source);
    let process = ProcessPolicy::new(&metadata).unwrap();
    let policy = ControlPolicy::new(&process, &metadata).unwrap();
    let queued = policy.reserve_metadata(&model, 16 * 1024).unwrap();
    let base = OperationControl::new();
    let memory =
        JobMemory::activate(&model, &policy, 4096, MAX_STACK_RESERVATION, base.clone()).unwrap();
    let scope = memory.scope();
    scope.charge_work(1_000_000).unwrap();
    assert_eq!(scope.charge_work(1), Err(MemoryError::WorkLimited));
    assert_eq!(scope.failure(), Some(MemoryError::WorkLimited));
    assert!(base.checkpoint().is_err());
    assert!(matches!(
        scope.reserve_array::<u8>(1, Family::Graph),
        Err(MemoryError::WorkLimited)
    ));
    drop(scope);
    drop(memory);
    assert_eq!(policy.usage().jobs, 0);
    drop(queued);
}

#[test]
fn expired_activation_preserves_the_deadline_reason() {
    let source = SourceBudget::preview();
    let metadata = SourceBudget::preview();
    let model = accepted(&source);
    let process = ProcessPolicy::new(&metadata).unwrap();
    let policy = ControlPolicy::new(&process, &metadata).unwrap();
    let base = OperationControl::new().with_deadline(std::time::Duration::ZERO);
    assert!(matches!(
        JobMemory::activate(&model, &policy, 4096, MAX_STACK_RESERVATION, base),
        Err(DiagramError::Deadline)
    ));
    assert_eq!(policy.usage().jobs, 0);
}

#[test]
fn native_admission_and_finalization_preserve_the_expired_deadline() {
    let source = SourceBudget::preview();
    let metadata = SourceBudget::preview();
    let model = accepted(&source);
    let process = ProcessPolicy::new(&metadata).unwrap();
    let policy = ControlPolicy::new(&process, &metadata).unwrap();
    let base = OperationControl::new();
    let memory =
        JobMemory::activate(&model, &policy, 4096, MAX_STACK_RESERVATION, base.clone()).unwrap();
    // Shared original deadline; no sleep or fake worker exit. This checks
    // admission/finalization mechanics, not Windows thread qualification.
    assert!(base.set_deadline(std::time::Duration::ZERO));
    let admission = memory.charge_native(1);
    let finalization = memory.finalize_after_thread_exit(4096).err();
    assert_eq!(
        (admission, finalization),
        (Err(DiagramError::Deadline), Some(DiagramError::Deadline))
    );
    assert_eq!(policy.usage().jobs, 1);
    drop(memory);
    assert_eq!(policy.usage().jobs, 0);
}

#[test]
fn native_admission_keeps_requested_cancellation_and_funded_failure_priority() {
    for scenario in 0..3 {
        let source = SourceBudget::preview();
        let metadata = SourceBudget::preview();
        let model = accepted(&source);
        let process = ProcessPolicy::new(&metadata).unwrap();
        let policy = ControlPolicy::new(&process, &metadata).unwrap();
        let base = OperationControl::new();
        let memory =
            JobMemory::activate(&model, &policy, 4096, MAX_STACK_RESERVATION, base.clone())
                .unwrap();
        let expected = if scenario == 0 {
            base.cancel();
            assert_eq!(
                base.checkpoint().unwrap_err().reason,
                merman_core::CancelReason::Requested
            );
            DiagramError::Cancelled
        } else if scenario == 1 {
            let scope = memory.scope();
            scope.charge_work(1_000_000).unwrap();
            assert_eq!(scope.charge_work(1), Err(MemoryError::WorkLimited));
            DiagramError::Memory(MemoryError::WorkLimited)
        } else {
            assert_eq!(
                memory.charge_native(JOB_BYTES),
                Err(DiagramError::Memory(MemoryError::BudgetLimited))
            );
            DiagramError::Memory(MemoryError::BudgetLimited)
        };
        assert!(base.set_deadline(std::time::Duration::ZERO));
        assert_eq!(memory.charge_native(1), Err(expected));
        assert!(matches!(
            memory.finalize_after_thread_exit(4096),
            Err(error) if error == expected
        ));
        assert_eq!(policy.usage().jobs, 1);
        drop(memory);
        assert_eq!(policy.usage().jobs, 0);
    }
}

#[test]
fn allocation_cancellation_recovers_the_original_job_reason() {
    for requested in [true, false] {
        for stage in 0..3 {
            let source = SourceBudget::preview();
            let metadata = SourceBudget::preview();
            let model = accepted(&source);
            let process = ProcessPolicy::new(&metadata).unwrap();
            let policy = ControlPolicy::new(&process, &metadata).unwrap();
            let base = OperationControl::new();
            let memory =
                JobMemory::activate(&model, &policy, 4096, MAX_STACK_RESERVATION, base.clone())
                    .unwrap();
            let scope = memory.scope();
            let expected = if requested {
                base.cancel();
                assert_eq!(
                    base.checkpoint().unwrap_err().reason,
                    merman_core::CancelReason::Requested
                );
                DiagramError::Cancelled
            } else {
                DiagramError::Deadline
            };
            assert!(base.set_deadline(std::time::Duration::ZERO));
            let error = match stage {
                0 => OwnedBlock::try_array::<u8>(&scope, 64, Family::Text)
                    .err()
                    .unwrap(),
                1 => scope
                    .before_allocate(Layout::new::<u8>(), Family::Text)
                    .unwrap_err(),
                _ => scope.charge_work(1).unwrap_err(),
            };
            assert_eq!(error, MemoryError::Cancelled);
            assert_eq!(memory.resolve_error(DiagramError::Memory(error)), expected);
            assert_eq!(memory.resolve_error(DiagramError::Cancelled), expected);
            assert_eq!(memory.usage().live_rust_bytes, 0);
            assert_eq!(policy.usage().jobs, 1);
            drop(scope);
            drop(memory);
            assert_eq!(policy.usage().jobs, 0);
        }
    }
}
