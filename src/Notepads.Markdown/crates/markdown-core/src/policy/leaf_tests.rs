// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE file in the project root for license information.
// ---------------------------------------------------------------------------------------------

use super::*;
use std::cell::Cell;
use std::ops::ControlFlow;

#[test]
fn metadata_reads_keep_bounded_view_charges_and_values() {
    let budget = SourceBudget::model_fixture();
    let mut op = budget.begin(CancellationToken::new());
    assert_eq!(op.read_metadata(8, || Ok(42u64)), Ok(42));
    let stats = op.stats().unwrap();
    assert_eq!(stats.scanned_bytes(), 8);
    assert_eq!(stats.copied_bytes(), 8);
    assert_eq!(stats.metadata_actions(), 2);
    assert_eq!(stats.charged_units(), 2);
}

#[test]
fn metadata_second_view_refusal_keeps_the_first_charge_and_does_not_read() {
    let budget = SourceBudget::new_with_model_limits(
        SourceLimits {
            max_work: 1,
            ..SourceLimits::default()
        },
        ModelLimits::default(),
    )
    .unwrap();
    let mut op = budget.begin(CancellationToken::new());
    let called = Cell::new(false);
    let error = CoreError::ResourceLimit {
        kind: LimitKind::Work,
        requested: 2,
        limit: 1,
    };
    assert_eq!(
        op.read_metadata(8, || {
            called.set(true);
            Ok(42u64)
        }),
        Err(error)
    );
    assert!(!called.get());
    let stats = op.stats().unwrap();
    assert_eq!(stats.scanned_bytes(), 8);
    assert_eq!(stats.copied_bytes(), 0);
    assert_eq!(stats.charged_units(), 1);
    assert_eq!(op.checkpoint(), Err(error));
}

#[test]
fn metadata_width_and_cancel_reject_before_actual_read() {
    let budget = SourceBudget::task_fixture();
    for cancelled in [false, true] {
        let cancel = CancellationToken::new();
        let mut op = budget.begin(cancel.clone());
        if cancelled {
            cancel.cancel();
        }
        let called = Cell::new(false);
        assert_eq!(
            op.read_metadata(513, || {
                called.set(true);
                Ok(1u64)
            }),
            Err(if cancelled {
                CoreError::Cancelled
            } else {
                CoreError::InternalInvariant
            })
        );
        assert!(!called.get());
        assert_eq!(op.stats(), None);
    }
}

#[test]
fn scalar_batches_bound_actual_steps_and_resume_the_same_iterator() {
    let budget = SourceBudget::task_fixture();
    let mut op = budget.begin(CancellationToken::new());
    op.set_clock_samples_for_test(&[ELAPSED_LIMIT_NS - 1]);
    let text = "中".repeat(257);
    let mut input = text.chars();
    let mut seen = 0;
    assert_eq!(
        op.visit_scalar_batch(&mut input, ScalarChargeOrder::ScanThenEvent, |_| {
            seen += 1;
            Ok(ControlFlow::Continue(()))
        }),
        Ok(ScalarBatchEnd::Full)
    );
    assert_eq!(seen, 256);
    assert_eq!(input.as_str(), "中");
    assert_eq!(
        op.visit_scalar_batch(&mut input, ScalarChargeOrder::ScanThenEvent, |_| {
            seen += 1;
            Ok(ControlFlow::Continue(()))
        }),
        Ok(ScalarBatchEnd::Exhausted)
    );
    assert_eq!(seen, 257);
    assert_eq!(op.stats(), None);
}

#[test]
fn scalar_charge_order_keeps_partial_fee_and_first_fault() {
    for order in [
        ScalarChargeOrder::ScanThenEvent,
        ScalarChargeOrder::EventThenScan,
    ] {
        let budget = SourceBudget::new(SourceLimits {
            max_work: 1,
            ..SourceLimits::default()
        })
        .unwrap();
        let mut op = budget.begin(CancellationToken::new());
        let called = Cell::new(false);
        let mut input = "中".chars();
        let error = CoreError::ResourceLimit {
            kind: LimitKind::Work,
            requested: if order == ScalarChargeOrder::ScanThenEvent {
                3
            } else {
                4
            },
            limit: 1,
        };
        assert_eq!(
            op.visit_scalar_batch(&mut input, order, |_| {
                called.set(true);
                Ok(ControlFlow::Continue(()))
            }),
            Err(error)
        );
        assert!(!called.get());
        assert_eq!(
            op.stats().unwrap().events(),
            u64::from(order == ScalarChargeOrder::EventThenScan)
        );
        assert_eq!(op.checkpoint_now(), Err(error));
    }
}

#[test]
fn scalar_break_and_typed_error_do_not_run_later_actions() {
    let budget = SourceBudget::task_fixture();
    let mut op = budget.begin(CancellationToken::new());
    let mut input = "abc".chars();
    assert_eq!(
        op.visit_scalar_batch(&mut input, ScalarChargeOrder::ScanThenEvent, |_| Ok(
            ControlFlow::Break(())
        )),
        Ok(ScalarBatchEnd::Break)
    );
    assert_eq!(input.as_str(), "bc");
    let error = CoreError::InvalidProvenance;
    assert_eq!(
        op.visit_scalar_batch(&mut input, ScalarChargeOrder::EventThenScan, |_| Err(error)),
        Err(error)
    );
    assert_eq!(input.as_str(), "c");
    let called = Cell::new(false);
    assert_eq!(
        op.visit_scalar_batch(&mut input, ScalarChargeOrder::EventThenScan, |_| {
            called.set(true);
            Ok(ControlFlow::Continue(()))
        }),
        Err(error)
    );
    assert!(!called.get());
}
