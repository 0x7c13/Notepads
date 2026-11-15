// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE file in the project root for license information.
// ---------------------------------------------------------------------------------------------

use super::*;
use crate::config::Profile;
use crate::identity::SourceKey;
use crate::policy::{CancellationToken, ModelLimits, SourceLimits};
use crate::source::SourceDocument;

const STABLE: u64 = 499_999_999;
const LIMIT: u64 = 500_000_000;
const CONFIG: ParseConfig = ParseConfig::new(Profile::C0);

fn operation(budget: &SourceBudget) -> Operation {
    let mut op = budget.begin(CancellationToken::new());
    op.set_clock_samples_for_test(&[STABLE]);
    op
}

fn document(budget: &SourceBudget) -> SourceDocument {
    SourceDocument::new(
        SourceKey::new([0x38; 16], 1).unwrap(),
        b"a",
        &mut operation(budget),
    )
    .unwrap()
}

fn released(budget: &SourceBudget) {
    let usage = budget.usage();
    assert_eq!(
        (
            usage.source_capacity,
            usage.auxiliary_capacity,
            usage.source_owners
        ),
        (0, 0, 0)
    );
}

#[test]
fn typed_source_capture_and_prepare_failures_keep_actual_context() {
    let budget = SourceBudget::task_fixture();
    let mut progress = ParseProgress::default();
    progress.begin_source_capture();
    let mut op = operation(&budget);
    let source = SourceDocument::new(SourceKey::new([0x38; 16], 1).unwrap(), &[0xff], &mut op);
    assert!(source.is_err());
    assert_eq!(progress.stage(), ParseStage::SourceCapture);
    drop(source);
    released(&budget);

    let doc = document(&budget);
    let token = CancellationToken::new();
    token.cancel();
    let mut op = budget.begin(token);
    assert!(matches!(
        AcceptedModel::experimental_parse_with_progress(
            doc.snapshot(),
            CONFIG,
            1,
            &mut op,
            &mut progress
        ),
        Err(CoreError::Cancelled)
    ));
    assert_eq!(progress.stage(), ParseStage::Prepare);
    let mut op = operation(&budget);
    op.set_clock_samples_for_test(&[LIMIT]);
    assert!(matches!(
        AcceptedModel::experimental_parse_with_progress(
            doc.snapshot(),
            CONFIG,
            1,
            &mut op,
            &mut progress
        ),
        Err(CoreError::DeadlineExceeded { .. })
    ));
    assert_eq!(progress.stage(), ParseStage::Prepare);
    drop(doc);
    released(&budget);
}

// The native allocation ledger charges address-dependent probe work. A clock
// prefix from a different construction cannot identify this run's stage.
// These hooks change only the existing synthetic clock input, in this same run;
// they do not replace Work, admission, stage guards or the final finish check.
#[derive(Debug)]
pub(super) struct Deadline {
    stage: Option<ParseStage>,
    clock: Arc<std::sync::atomic::AtomicU64>,
    fired: bool,
}

impl Deadline {
    fn fire(&mut self) {
        self.clock
            .store(LIMIT, std::sync::atomic::Ordering::Relaxed);
        self.fired = true;
    }
}

impl ParseProgress {
    pub(super) fn deadline_at_stage_for_test(&mut self, stage: ParseStage) {
        if let Some(deadline) = &mut self.deadline
            && deadline.stage == Some(stage)
        {
            deadline.fire();
        }
    }

    pub(super) fn deadline_at_finish_for_test(&mut self) {
        if let Some(deadline) = &mut self.deadline
            && deadline.stage.is_none()
        {
            deadline.fire();
        }
    }
}

#[test]
fn coarse_entry_guards_and_final_publication_are_in_the_attempted_stage() {
    for budget in [SourceBudget::task_fixture()] {
        let doc = document(&budget);
        let retained = budget.usage();
        for requested in [
            Some(ParseStage::Build),
            Some(ParseStage::Validate),
            Some(ParseStage::Index),
            Some(ParseStage::Publish),
            None, // After private model assembly, before the real final finish.
        ] {
            let clock = Arc::new(std::sync::atomic::AtomicU64::new(STABLE));
            let mut op = operation(&budget);
            op.set_shared_clock_for_test(clock.clone());
            let mut progress = ParseProgress {
                stage: ParseStage::NotStarted,
                deadline: Some(Deadline {
                    stage: requested,
                    clock,
                    fired: false,
                }),
            };
            let result = AcceptedModel::experimental_parse_with_progress(
                doc.snapshot(),
                CONFIG,
                1,
                &mut op,
                &mut progress,
            );
            let error = CoreError::DeadlineExceeded {
                elapsed_ns: LIMIT,
                limit_ns: LIMIT,
            };
            assert!(matches!(&result, Err(actual) if *actual == error));
            assert_eq!(progress.stage(), requested.unwrap_or(ParseStage::Publish));
            assert!(progress.deadline.as_ref().unwrap().fired);
            assert_eq!(op.first_fault(), Some(error));
            assert_eq!(op.checkpoint(), Err(error));
            drop(result);
            let after = budget.usage();
            assert_eq!(after.source_capacity, retained.source_capacity);
            assert_eq!(after.source_owners, retained.source_owners);
            assert_eq!(after.auxiliary_capacity, retained.auxiliary_capacity);
        }
        let mut op = operation(&budget);
        let mut progress = ParseProgress::default();
        let model = AcceptedModel::experimental_parse_with_progress(
            doc.snapshot(),
            CONFIG,
            1,
            &mut op,
            &mut progress,
        )
        .unwrap();
        assert_eq!(progress.stage(), ParseStage::Complete);
        drop(model);
        drop(doc);
        released(&budget);
    }
}

#[test]
fn validation_resource_failure_never_seals_or_publishes_facts() {
    let budget = SourceBudget::task_with_limits_for_test(
        SourceLimits::default(),
        ModelLimits {
            index_records: 0,
            ..ModelLimits::default()
        },
    );
    let doc = document(&budget);
    let mut op = operation(&budget);
    let mut progress = ParseProgress::default();
    let result = AcceptedModel::experimental_parse_with_progress(
        doc.snapshot(),
        CONFIG,
        1,
        &mut op,
        &mut progress,
    );
    assert!(matches!(
        result,
        Err(CoreError::ResourceLimit {
            kind: crate::error::LimitKind::IndexRecords,
            ..
        })
    ));
    assert_eq!(progress.stage(), ParseStage::Validate);
    drop(result);
    drop(doc);
    released(&budget);
}

#[test]
fn progress_keeps_semantics_exact_pre_capture_fees_and_first_fault() {
    for budget in [SourceBudget::model_fixture(), SourceBudget::task_fixture()] {
        let doc = document(&budget);
        let mut plain = operation(&budget);
        let a = AcceptedModel::parse(doc.snapshot(), CONFIG, 1, &mut plain).unwrap();
        let mut observed = operation(&budget);
        let mut progress = ParseProgress::default();
        let b = AcceptedModel::experimental_parse_with_progress(
            doc.snapshot(),
            CONFIG,
            1,
            &mut observed,
            &mut progress,
        )
        .unwrap();
        assert_eq!(progress.stage(), ParseStage::Complete);
        assert_eq!(plain.first_fault(), observed.first_fault());
        assert_eq!(a.ir().atoms_len(), b.ir().atoms_len());
        assert_eq!(a.ir().nodes(), b.ir().nodes());
        drop((a, b));
        // Native allocation ledger hashing depends on actual allocation
        // addresses (vendor native.rs allocation_hash). Separate captures can
        // do different probe work even for empty input. Compare exact fees at
        // the invalid-generation boundary before those allocations.
        let mut plain = operation(&budget);
        let a = AcceptedModel::parse(doc.snapshot(), CONFIG, 0, &mut plain);
        let mut observed = operation(&budget);
        let b = AcceptedModel::experimental_parse_with_progress(
            doc.snapshot(),
            CONFIG,
            0,
            &mut observed,
            &mut progress,
        );
        assert!(matches!(a, Err(CoreError::InvalidRevision)));
        assert!(matches!(b, Err(CoreError::InvalidRevision)));
        assert_eq!(progress.stage(), ParseStage::Prepare);
        assert_eq!(plain.stats(), observed.stats());
        assert_eq!(
            plain.clock_read_count_for_test(),
            observed.clock_read_count_for_test()
        );
        assert_eq!(plain.first_fault(), observed.first_fault());
        for observe in [false, true] {
            let mut op = operation(&budget);
            op.fail(CoreError::InvalidProvenance);
            let result = if observe {
                AcceptedModel::experimental_parse_with_progress(
                    doc.snapshot(),
                    CONFIG,
                    1,
                    &mut op,
                    &mut progress,
                )
            } else {
                AcceptedModel::parse(doc.snapshot(), CONFIG, 1, &mut op)
            };
            assert!(matches!(result, Err(CoreError::InvalidProvenance)));
            assert_eq!(op.first_fault(), Some(CoreError::InvalidProvenance));
        }
        assert_eq!(progress.stage(), ParseStage::Prepare);
        drop(doc);
        released(&budget);
    }
}
