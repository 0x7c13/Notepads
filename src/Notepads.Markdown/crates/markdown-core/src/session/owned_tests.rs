// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE file in the project root for license information.
// ---------------------------------------------------------------------------------------------

use super::*;
use crate::config::Profile;
use crate::error::LimitKind;
use crate::identity::{LogicalBoundary, LogicalSpan, SourceBoundary, SourceSpan};
use crate::ir::TextTarget;
use crate::mapping::{
    Affinity, Correspondence, PointInput, PointQuery, Position, RangeInput, RangeQuery, RangeResult,
};
use crate::policy::{ModelLimits, SourceLimits};
use crate::reader::MainReader;
use crate::source::Edit;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

const LIVE: u64 = 499_999_999;
const DEAD: u64 = 500_000_000;

fn c0() -> ParseConfig {
    ParseConfig::new(Profile::C0)
}

fn key(revision: u64) -> SourceKey {
    SourceKey::new([3; 16], revision).unwrap()
}

fn budgets() -> [SourceBudget; 2] {
    [SourceBudget::model_fixture(), SourceBudget::task_fixture()]
}

fn elapsed_budgets() -> impl Iterator<Item = SourceBudget> {
    budgets()
        .into_iter()
        .filter(|b| b.elapsed_limit_ns().is_some())
}

fn session(budget: &SourceBudget) -> SourceSession {
    SourceSession::transactional(c0(), budget.clone(), CancellationToken::new()).unwrap()
}

fn transaction(session: &mut SourceSession) -> SourceTransaction<'_> {
    let mut transaction = session.begin_transaction(CancellationToken::new());
    assert_eq!(transaction.op.first_fault(), None);
    transaction.op.set_clock_samples_for_test(&[LIVE]);
    transaction
}

fn attach(session: &mut SourceSession, text: &[u8]) {
    transaction(session).resync(key(1), text).unwrap();
}

fn complete(session: &mut SourceSession) -> OwnedCompletedParse {
    transaction(session).capture().unwrap().execute().unwrap()
}

fn query_operation(budget: &SourceBudget) -> Operation {
    let mut op = budget.begin(CancellationToken::new());
    op.set_clock_samples_for_test(&[LIVE]);
    op
}

fn released(budget: &SourceBudget) {
    let usage = budget.usage();
    assert_eq!(usage.source_capacity, 0);
    assert_eq!(usage.auxiliary_capacity, 0);
    assert_eq!(usage.source_owners, 0);
}

fn deadline() -> CoreError {
    CoreError::DeadlineExceeded {
        elapsed_ns: DEAD,
        limit_ns: DEAD,
    }
}

fn main_text(model: &AcceptedModel) -> String {
    let mut reader = MainReader::new(model.clone(), query_operation(model.budget())).unwrap();
    let mut output = String::new();
    let mut buffer = [0; 4];
    loop {
        let batch = reader.read_into(&mut buffer).unwrap();
        output.push_str(std::str::from_utf8(&buffer[..batch.written]).unwrap());
        assert_eq!(batch.utf16, output.encode_utf16().count() as u64);
        if batch.done {
            return output;
        }
        assert_ne!(batch.written, 0);
    }
}

#[test]
fn actual_queued_owners_are_send() {
    fn send<T: Send>() {}
    send::<OwnedParseRequest>();
    send::<OwnedCompletedParse>();
}

#[test]
fn control_creation_is_empty_charged_and_releases_on_each_failure() {
    for budget in budgets() {
        let token = CancellationToken::new();
        token.cancel();
        assert_eq!(
            SourceSession::transactional(c0(), budget.clone(), token).err(),
            Some(CoreError::Cancelled)
        );
        released(&budget);
        let s = session(&budget);
        assert!(s.snapshot().is_none());
        assert!(s.installed().is_none());
        assert!(s.needs_resync());
        assert_eq!(budget.usage().source_capacity, 0);
        assert_eq!(budget.usage().source_owners, 0);
        assert!(budget.usage().auxiliary_capacity > 0);
        drop(s);
        released(&budget);
    }
    for budget in elapsed_budgets() {
        let mut op = query_operation(&budget);
        // Expire after the real scope Arc has been allocated, before publication.
        op.set_clock_samples_for_test(&[LIVE, DEAD]);
        assert_eq!(
            SourceSession::unattached_inner(c0(), &mut op).err(),
            Some(deadline())
        );
        released(&budget);
    }
    let budget = SourceBudget::new_with_model_limits(
        SourceLimits {
            auxiliary_capacity: 0,
            ..SourceLimits::default()
        },
        ModelLimits::default(),
    )
    .unwrap();
    assert!(matches!(
        SourceSession::transactional(c0(), budget.clone(), CancellationToken::new()),
        Err(CoreError::ResourceLimit {
            kind: LimitKind::AuxiliaryCapacity,
            ..
        })
    ));
    released(&budget);
}

#[test]
fn source_capture_parse_install_and_independent_queries_use_real_models() {
    for budget in budgets() {
        for (source, expected) in [
            ("**中&amp;😀**", "中&😀"),
            ("a\r\nb  \r\nc", "a b\nc"),
            ("[r]: /dest", ""),
            ("", ""),
        ] {
            let mut s = session(&budget);
            let mut tx = transaction(&mut s);
            tx.resync(key(1), source.as_bytes()).unwrap();
            let request = tx.capture().unwrap();
            let identity = request.identity();
            let completed = request.execute().unwrap();
            assert_eq!(completed.identity(), identity);
            let model = s.install_owned(completed).unwrap();
            assert_eq!(model.identity(), identity);
            assert_eq!(main_text(&model), expected);
            assert_eq!(model.source().text(), source);
            assert_eq!(s.installed().unwrap().identity(), identity);
            drop(model);
            drop(s);
            released(&budget);
        }
    }
}

#[test]
fn installed_model_obeys_independent_point_and_range_oracles() {
    for budget in budgets() {
        let mut s = session(&budget);
        attach(&mut s, b"**a&amp;b**");
        let completed = complete(&mut s);
        let model = s.install_owned(completed).unwrap();
        let flow = model.main_flow_id();
        let response = model
            .map_point(
                &PointQuery {
                    model: model.identity(),
                    input: PointInput::Flow {
                        flow,
                        utf16: LogicalBoundary::new(1),
                    },
                    affinity: Affinity::After,
                },
                &mut query_operation(&budget),
            )
            .unwrap();
        assert_eq!(
            response.results()[0].chosen,
            Position::Source(SourceBoundary::new(3))
        );
        assert_eq!(response.results()[0].correspondence, Correspondence::Exact);
        drop(response);
        drop(model);
        transaction(&mut s).resync(key(2), b"abc").unwrap();
        let completed = complete(&mut s);
        let model = s.install_owned(completed).unwrap();
        let flow = model.main_flow_id();
        let response = model
            .map_range(
                &RangeQuery {
                    model: model.identity(),
                    input: RangeInput::Flow {
                        flow,
                        span: LogicalSpan::new(1, 2).unwrap(),
                    },
                    empty_affinity: Affinity::After,
                },
                &mut query_operation(&budget),
            )
            .unwrap();
        let RangeResult::Selection(rows) = response.results()[0] else {
            panic!("nonempty range")
        };
        let selections = rows.slice(response.selections()).unwrap();
        assert_eq!(selections.len(), 1);
        assert_eq!(selections[0].target, TextTarget::Flow(flow));
        assert_eq!(
            selections[0].start.chosen,
            Position::Source(SourceBoundary::new(1))
        );
        assert_eq!(
            selections[0].end.chosen,
            Position::Source(SourceBoundary::new(2))
        );
        assert!(!selections[0].expanded);
        drop(response);
        drop(model);
        drop(s);
        released(&budget);
    }
}

#[test]
fn original_clock_survives_source_capture_queue_parse_completed_and_install() {
    for budget in elapsed_budgets() {
        let mut s = session(&budget);
        let clock = Arc::new(AtomicU64::new(490_000_000));
        let mut tx = transaction(&mut s);
        tx.op.set_shared_clock_for_test(clock.clone());
        tx.resync(key(1), b"abc").unwrap();
        clock.store(491_000_000, Ordering::Relaxed);
        let request = tx.capture().unwrap();
        assert!(request.op.clock_read_count_for_test() > 0);
        clock.store(492_000_000, Ordering::Relaxed);
        let completed = request.execute().unwrap();
        assert!(completed.op.clock_read_count_for_test() > 0);
        clock.store(493_000_000, Ordering::Relaxed);
        let model = s.install_owned(completed).unwrap();
        // Later queries are not bound to the finished update operation.
        clock.store(DEAD, Ordering::Relaxed);
        assert_eq!(main_text(&model), "abc");
        drop(model);
        drop(s);
        released(&budget);
    }
}

#[test]
fn queued_expiry_keeps_accepted_source_and_never_starts_a_new_window() {
    for budget in elapsed_budgets() {
        let mut s = session(&budget);
        let clock = Arc::new(AtomicU64::new(LIVE));
        let mut tx = transaction(&mut s);
        tx.op.set_shared_clock_for_test(clock.clone());
        tx.resync(key(1), b"accepted source").unwrap();
        let request = tx.capture().unwrap();
        clock.store(DEAD, Ordering::Relaxed);
        assert_eq!(request.execute().err(), Some(deadline()));
        assert_eq!(s.snapshot().unwrap().text(), "accepted source");
        assert!(s.installed().is_none());
        drop(s);
        released(&budget);
    }
}

#[test]
fn completed_expiry_does_not_replace_the_previous_installed_model() {
    for budget in elapsed_budgets() {
        let mut s = session(&budget);
        attach(&mut s, b"old model");
        let first = complete(&mut s);
        let old = s.install_owned(first).unwrap();
        let clock = Arc::new(AtomicU64::new(LIVE));
        let mut tx = transaction(&mut s);
        tx.op.set_shared_clock_for_test(clock.clone());
        let completed = tx.capture().unwrap().execute().unwrap();
        assert_ne!(completed.identity(), old.identity());
        clock.store(DEAD, Ordering::Relaxed);
        assert_eq!(s.install_owned(completed).err(), Some(deadline()));
        assert_eq!(s.installed().unwrap().identity(), old.identity());
        assert_eq!(main_text(&old), "old model");
        drop(old);
        drop(s);
        released(&budget);
    }
}

#[test]
fn final_install_checkpoint_precedes_publication() {
    for budget in elapsed_budgets() {
        let mut s = session(&budget);
        attach(&mut s, b"abc");
        let mut completed = complete(&mut s);
        completed.op.set_clock_samples_for_test(&[LIVE, DEAD]);
        assert_eq!(s.install_owned(completed).err(), Some(deadline()));
        assert!(s.installed().is_none());
        drop(s);
        released(&budget);
    }
}

#[test]
fn ignored_source_error_remains_first_fault_through_capture() {
    for budget in budgets() {
        let mut s = session(&budget);
        attach(&mut s, b"unchanged");
        let token = CancellationToken::new();
        let mut tx = s.begin_transaction(token.clone());
        tx.op.set_clock_samples_for_test(&[LIVE]);
        let fault = tx.resync(key(2), &[0xff]).err().unwrap();
        assert_eq!(fault, CoreError::InvalidUtf8 { offset: 0 });
        token.cancel();
        tx.op.set_clock_samples_for_test(&[DEAD]);
        assert_eq!(tx.capture().err(), Some(fault));
        assert_eq!(s.snapshot().unwrap().key(), key(1));
        assert_eq!(s.snapshot().unwrap().text(), "unchanged");
        drop(s);
        released(&budget);
    }
}

#[test]
fn cumulative_work_and_admissions_are_not_reset_at_capture_or_execute() {
    for budget in [SourceBudget::model_fixture()] {
        let mut s = session(&budget);
        let mut tx = transaction(&mut s);
        tx.resync(key(1), b"abc").unwrap();
        let source_work = tx.op.stats().unwrap().total();
        tx.apply(EditBatch {
            before: key(1),
            after: key(2),
            edits: &[],
        })
        .unwrap();
        let edited_work = tx.op.stats().unwrap().total();
        assert!(edited_work > source_work);
        let request = tx.capture().unwrap();
        assert_eq!(request.op.stats().unwrap().total(), edited_work);
        let completed = request.execute().unwrap();
        assert!(completed.op.stats().unwrap().total() > edited_work);
        let model = s.install_owned(completed).unwrap();
        drop(model);
        drop(s);
        released(&budget);
    }
    for budget in budgets() {
        let mut s = session(&budget);
        let mut tx = transaction(&mut s);
        tx.resync(key(1), b"abc").unwrap();
        // A finite admission control, not a fabricated model or measured workload.
        tx.op
            .admit_model(
                LimitKind::NativeRecords,
                budget.model_limits().unwrap().native_records,
            )
            .unwrap();
        let request = tx.capture().unwrap();
        assert!(matches!(
            request.execute(),
            Err(CoreError::ResourceLimit {
                kind: LimitKind::NativeRecords,
                ..
            })
        ));
        assert!(s.installed().is_none());
        drop(s);
        released(&budget);
    }
}

#[test]
fn cancellation_of_actual_request_or_completion_prevents_installation() {
    for budget in budgets() {
        let mut s = session(&budget);
        attach(&mut s, b"abc");
        let request = transaction(&mut s).capture().unwrap();
        request.cancellation().cancel();
        assert_eq!(request.execute().err(), Some(CoreError::Cancelled));
        let completed = complete(&mut s);
        completed.cancellation().cancel();
        assert_eq!(s.install_owned(completed).err(), Some(CoreError::Cancelled));
        let token = CancellationToken::new();
        token.cancel();
        let mut tx = s.begin_transaction(token);
        assert_eq!(
            tx.resync(key(2), b"other").err(),
            Some(CoreError::Cancelled)
        );
        assert_eq!(tx.capture().err(), Some(CoreError::Cancelled));
        assert_eq!(s.snapshot().unwrap().key(), key(1));
        assert!(s.installed().is_none());
        drop(s);
        released(&budget);
    }
}

#[test]
fn atomic_edit_chain_resync_and_same_key_rules_preserve_state() {
    for budget in budgets() {
        let mut s = session(&budget);
        attach(&mut s, "中😀\r\n".as_bytes());
        let edits = [Edit {
            range: SourceSpan::new(4, 5).unwrap(),
            replacement: b"x",
        }];
        assert_eq!(
            transaction(&mut s)
                .apply(EditBatch {
                    before: key(1),
                    after: key(2),
                    edits: &edits
                })
                .err(),
            Some(CoreError::InvalidBoundary)
        );
        assert_eq!(s.snapshot().unwrap().key(), key(1));
        assert!(!s.needs_resync());
        let edits = [Edit {
            range: SourceSpan::new(7, 9).unwrap(),
            replacement: b"!",
        }];
        let mut tx = transaction(&mut s);
        assert_eq!(
            tx.apply(EditBatch {
                before: key(1),
                after: key(7),
                edits: &edits
            })
            .unwrap()
            .text(),
            "中😀!"
        );
        let request = tx.capture().unwrap();
        assert_eq!(request.identity().source(), key(7));
        let old = request.execute().unwrap();
        assert_eq!(
            transaction(&mut s).resync(key(7), b"different").err(),
            Some(CoreError::WrongRevision {
                expected: 7,
                actual: 7
            })
        );
        assert_eq!(s.snapshot().unwrap().text(), "中😀!");
        assert_eq!(
            transaction(&mut s)
                .apply(EditBatch {
                    before: key(7),
                    after: key(7),
                    edits: &[]
                })
                .err(),
            Some(CoreError::WrongRevision {
                expected: 7,
                actual: 7
            })
        );
        assert!(!s.needs_resync());
        assert_eq!(
            transaction(&mut s)
                .apply(EditBatch {
                    before: key(6),
                    after: key(8),
                    edits: &[]
                })
                .err(),
            Some(CoreError::ResyncRequired {
                expected: 7,
                actual: 6
            })
        );
        assert!(s.needs_resync());
        assert_eq!(s.install_owned(old).err(), Some(CoreError::StaleResult));
        assert_eq!(
            transaction(&mut s).capture().err(),
            Some(CoreError::InvalidState)
        );
        transaction(&mut s).resync(key(9), b"resynced").unwrap();
        assert!(!s.needs_resync());
        let completed = complete(&mut s);
        let model = s.install_owned(completed).unwrap();
        assert_eq!(main_text(&model), "resynced");
        drop(model);
        drop(s);
        released(&budget);
    }
}

#[test]
fn successful_source_actions_survive_dropping_transaction_and_parse_failure() {
    for budget in budgets() {
        let mut s = session(&budget);
        transaction(&mut s).resync(key(1), b"accepted").unwrap();
        assert_eq!(s.snapshot().unwrap().text(), "accepted");
        let mut tx = transaction(&mut s);
        tx.resync_with_config(key(2), b"updated", ParseConfig::new(Profile::G3))
            .unwrap();
        let request = tx.capture().unwrap();
        request.cancellation().cancel();
        assert_eq!(request.execute().err(), Some(CoreError::Cancelled));
        assert_eq!(s.snapshot().unwrap().key(), key(2));
        assert_eq!(s.snapshot().unwrap().text(), "updated");
        assert_eq!(s.config, ParseConfig::new(Profile::G3));
        assert!(s.installed().is_none());
        drop(s);
        released(&budget);
    }
}

#[test]
fn supersession_aba_config_and_detach_each_reject_old_completion() {
    for budget in budgets() {
        for change in 0..5 {
            let mut s = session(&budget);
            attach(&mut s, b"old");
            let old = complete(&mut s);
            match change {
                0 => {
                    drop(transaction(&mut s).capture().unwrap());
                }
                1 => {
                    transaction(&mut s).resync(key(2), b"new").unwrap();
                }
                2 => {
                    transaction(&mut s).resync(key(2), b"new").unwrap();
                    transaction(&mut s).resync(key(1), b"old").unwrap();
                }
                3 => {
                    transaction(&mut s)
                        .set_config(ParseConfig::new(Profile::G3))
                        .unwrap();
                }
                4 => {
                    transaction(&mut s).detach().unwrap();
                }
                _ => unreachable!(),
            }
            assert_eq!(s.install_owned(old).err(), Some(CoreError::StaleResult));
            assert!(s.installed().is_none());
            drop(s);
            released(&budget);
        }
    }
}

#[test]
fn completion_cannot_cross_scope_or_root_even_with_identical_identity() {
    for budget in budgets() {
        let mut a = session(&budget);
        let mut b = session(&budget);
        attach(&mut a, b"AAAA");
        attach(&mut b, b"BBBB");
        let completed_a = complete(&mut a);
        let completed_b = complete(&mut b);
        assert_eq!(completed_a.identity(), completed_b.identity());
        assert_eq!(
            b.install_owned(completed_a).err(),
            Some(CoreError::StaleResult)
        );
        let model = b.install_owned(completed_b).unwrap();
        assert_eq!(main_text(&model), "BBBB");
        drop(model);
        let completed_a = complete(&mut a);
        let foreign_budget = SourceBudget::model_fixture();
        let mut foreign = session(&foreign_budget);
        attach(&mut foreign, b"AAAA");
        assert_eq!(
            foreign.install_owned(completed_a).err(),
            Some(CoreError::WrongBudget)
        );
        assert!(foreign.installed().is_none());
        drop(foreign);
        released(&foreign_budget);
        drop(a);
        drop(b);
        released(&budget);
    }
}

#[test]
fn generation_and_epoch_exhaustion_do_not_change_the_accepted_source() {
    for budget in budgets() {
        let mut s = session(&budget);
        attach(&mut s, b"old");
        s.generation = u64::MAX;
        assert_eq!(
            transaction(&mut s).capture().err(),
            Some(CoreError::ArithmeticOverflow)
        );
        assert_eq!(s.generation, u64::MAX);
        s.epoch = u64::MAX;
        assert_eq!(
            transaction(&mut s).resync(key(2), b"new").err(),
            Some(CoreError::ArithmeticOverflow)
        );
        assert_eq!(
            transaction(&mut s).detach().err(),
            Some(CoreError::ArithmeticOverflow)
        );
        assert_eq!(
            transaction(&mut s)
                .set_config(ParseConfig::new(Profile::G3))
                .err(),
            Some(CoreError::ArithmeticOverflow)
        );
        assert_eq!(s.snapshot().unwrap().key(), key(1));
        assert_eq!(s.snapshot().unwrap().text(), "old");
        assert_eq!(s.config, c0());
        drop(s);
        released(&budget);
    }
}

#[test]
fn final_config_detach_and_capture_checks_leave_all_state_unchanged() {
    for budget in elapsed_budgets() {
        for action in 0..3 {
            let mut s = session(&budget);
            attach(&mut s, b"old");
            let completed = complete(&mut s);
            let old = s.install_owned(completed).unwrap();
            let epoch = s.epoch;
            let generation = s.generation;
            let mut tx = transaction(&mut s);
            tx.op.set_clock_samples_for_test(&[LIVE, DEAD]);
            let fault = match action {
                0 => tx.set_config(ParseConfig::new(Profile::G3)).err(),
                1 => tx.detach().err(),
                2 => tx.capture().err(),
                _ => unreachable!(),
            };
            assert_eq!(fault, Some(deadline()));
            assert_eq!(s.epoch, epoch);
            assert_eq!(s.generation, generation);
            assert_eq!(s.config, c0());
            assert_eq!(s.snapshot().unwrap().text(), "old");
            assert_eq!(s.installed().unwrap().identity(), old.identity());
            drop(old);
            drop(s);
            released(&budget);
        }
    }
}

#[test]
fn final_source_commit_checks_preserve_source_and_joint_config() {
    for budget in elapsed_budgets() {
        for edit in [false, true] {
            // Observe the real final checkpoint for this tiny deterministic action.
            let mut reference = session(&budget);
            attach(&mut reference, b"old");
            let action = |tx: &mut SourceTransaction<'_>| {
                if edit {
                    tx.apply(EditBatch {
                        before: key(1),
                        after: key(2),
                        edits: &[],
                    })
                } else {
                    tx.resync_with_config(key(2), b"new", ParseConfig::new(Profile::G3))
                }
            };
            let mut tx = transaction(&mut reference);
            action(&mut tx).unwrap();
            let reads = tx.op.clock_read_count_for_test();
            assert!((2..64).contains(&reads));
            drop(tx);
            drop(reference);
            released(&budget);

            let mut s = session(&budget);
            attach(&mut s, b"old");
            let completed = complete(&mut s);
            let old = s.install_owned(completed).unwrap();
            let epoch = s.epoch;
            let mut samples = vec![LIVE; reads];
            samples[reads - 1] = DEAD;
            let mut tx = transaction(&mut s);
            tx.op.set_clock_samples_for_test(&samples);
            assert_eq!(action(&mut tx).err(), Some(deadline()));
            drop(tx);
            assert_eq!(s.epoch, epoch);
            assert_eq!(s.config, c0());
            assert_eq!(s.snapshot().unwrap().key(), key(1));
            assert_eq!(s.snapshot().unwrap().text(), "old");
            assert_eq!(s.installed().unwrap().identity(), old.identity());
            drop(old);
            drop(s);
            released(&budget);
        }
    }
}

#[test]
fn dropped_request_completion_session_and_retained_pins_release_only_at_last_owner() {
    for budget in budgets() {
        let mut s = session(&budget);
        attach(&mut s, b"old");
        let retained_source = s.snapshot().unwrap();
        let completed = complete(&mut s);
        let retained_model = s.install_owned(completed).unwrap();
        let request = transaction(&mut s).capture().unwrap();
        transaction(&mut s).resync(key(2), b"new").unwrap();
        drop(s);
        assert_eq!(retained_source.text(), "old");
        assert_eq!(main_text(&retained_model), "old");
        let completed = request.execute().unwrap();
        drop(retained_source);
        drop(retained_model);
        assert!(budget.usage().source_capacity > 0);
        assert!(budget.usage().auxiliary_capacity > 0);
        assert_eq!(main_text(&completed.completed.model), "old");
        drop(completed);
        released(&budget);

        let mut s = session(&budget);
        attach(&mut s, b"queued");
        let request = transaction(&mut s).capture().unwrap();
        drop(s);
        assert!(budget.usage().source_capacity > 0);
        drop(request);
        released(&budget);
    }
}
