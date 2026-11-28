// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE file in the project root for license information.
// ---------------------------------------------------------------------------------------------

use super::*;

#[test]
fn task_fixture_uses_current_access_with_bounded_test_limits() {
    let task = SourceBudget::task_fixture();
    assert_eq!(task.admission_mode(), ResourceAdmissionMode::TaskAccess);
    assert_eq!(task.admission_mode().label(), "task-access");
    assert_eq!(task.work_stats_collection(), WorkStatsCollection::Disabled);
    assert_eq!(task.elapsed_limit_ns(), Some(500_000_000));
    assert_eq!(task.active_work_limit(), None);
    let limits = task.limits();
    assert_eq!(limits.max_source_bytes, 16_777_216);
    assert_eq!(limits.retained_source_capacity, 67_108_864);
    assert_eq!(limits.retained_source_owners, 64);
    assert_eq!(limits.max_parse_bytes, 50_331_648);
    assert_eq!(limits.max_input_pieces, 1_048_576);
    assert_eq!(limits.max_edits, 65_536);
    assert_eq!(limits.max_insert_bytes, 16_777_216);
    assert_eq!(limits.auxiliary_capacity, 134_217_728);
    assert_eq!(limits.max_work, 268_435_456);
    let model = task.model_limits().unwrap();
    assert_eq!(model.native_records, 1_048_576);
    assert_eq!(model.model_records, 1_048_576);
    assert_eq!(model.source_pieces, 2_097_152);
    assert_eq!(model.proof_records, 1_048_576);
    assert_eq!(model.depth, 256);
    assert_eq!(model.decoded_bytes, 33_554_432);
    assert_eq!(model.reference_lookups, 1_048_576);
    assert_eq!(model.reference_bytes, 33_554_432);
    assert_eq!(model.index_records, 4_194_304);
    assert_eq!(model.query_requests, 256);
    assert_eq!(model.query_records, 65_536);
    assert_eq!(model.query_capacity, 8_388_608);

    let task_op = task.begin(CancellationToken::new());
    assert!(task_op.uses_task_access());
    assert!(task_op.uses_verified_index());
    assert!(
        !SourceBudget::model_fixture()
            .begin(CancellationToken::new())
            .uses_verified_index()
    );
    assert_eq!(task_op.stats(), None);
}

#[test]
fn task_mode_rejects_diagnostic_or_missing_model_variants() {
    for (stats, model) in [
        (
            WorkStatsCollection::Enabled,
            Some(ModelLimits::default()),
        ),
        (
            WorkStatsCollection::Disabled,
            None,
        ),
    ] {
        assert!(matches!(
            SourceBudget::create_with_admission(
                SourceLimits::default(),
                model,
                ResourceAdmissionMode::TaskAccess,
                stats
            ),
            Err(CoreError::UnsupportedConfiguration)
        ));
    }
}

#[test]
fn task_ordinary_fallback_retains_first_fault_cancel_and_deadline() {
    let budget = SourceBudget::task_fixture();
    let cancel = CancellationToken::new();
    cancel.cancel();
    let mut op = budget.begin(cancel);
    assert_eq!(op.first_fault(), Some(CoreError::Cancelled));
    assert_eq!(op.scan(1), Err(CoreError::Cancelled));
    assert_eq!(op.stats(), None);

    let mut op = budget.begin(CancellationToken::new());
    assert_eq!(
        op.scan(CHECKPOINT_BYTES + 1),
        Err(CoreError::InternalInvariant)
    );
    assert_eq!(op.checkpoint_now(), Err(CoreError::InternalInvariant));

    let mut op = budget.begin(CancellationToken::new());
    op.set_clock_samples_for_test(&[499_999_999, 500_000_000]);
    op.scan(0).unwrap();
    assert_eq!(
        op.events(0),
        Err(CoreError::DeadlineExceeded {
            elapsed_ns: 500_000_000,
            limit_ns: 500_000_000,
        })
    );
    assert_eq!(op.stats(), None);
    assert_eq!(budget.usage().source_capacity, 0);
    assert_eq!(budget.usage().auxiliary_capacity, 0);
    assert_eq!(budget.usage().source_owners, 0);
}
