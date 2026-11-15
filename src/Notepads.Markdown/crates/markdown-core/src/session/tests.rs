// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE file in the project root for license information.
// ---------------------------------------------------------------------------------------------

use super::*;
use crate::config::Profile;
use crate::identity::{LogicalBoundary, SourceSpan};
use crate::mapping::{Affinity, Correspondence, PointInput, PointQuery, Position};
use crate::policy::ResourceAdmissionMode;
use crate::source::Edit;

fn key(revision: u64) -> SourceKey {
    SourceKey::new([1; 16], revision).unwrap()
}
fn root() -> SourceBudget {
    SourceBudget::model_fixture()
}

#[test]
fn elapsed_session_construction_rejects_before_allocating_and_keeps_first_fault() {
    for budget in elapsed_candidate_budgets() {
        let token = CancellationToken::new();
        let mut op = elapsed_candidate_operation(&budget, token.clone());
        assert_eq!(
            SourceSession::new(key(1), b"abc", ParseConfig::new(Profile::C0), &mut op).err(),
            Some(CoreError::UnsupportedConfiguration)
        );
        assert_eq!(budget.usage().source_capacity, 0);
        assert_eq!(budget.usage().source_owners, 0);
        assert_eq!(budget.usage().auxiliary_capacity, 0);

        let mut cancelled = elapsed_candidate_operation(&budget, token.clone());
        cancelled.set_clock_samples_for_test(&[500_000_000]);
        token.cancel();
        assert_eq!(
            SourceSession::unattached(ParseConfig::new(Profile::C0), &mut cancelled).err(),
            Some(CoreError::Cancelled)
        );
        // An earlier mode failure cannot become a later cancellation/deadline.
        assert_eq!(
            op.checkpoint_now(),
            Err(CoreError::UnsupportedConfiguration)
        );
    }
}

#[test]
fn elapsed_mode_cannot_bypass_capture_execute_or_install_by_renewing_an_operation() {
    for budget in elapsed_candidate_budgets() {
        let token = CancellationToken::new();
        let mut op = elapsed_candidate_operation(&budget, token.clone());
        let document = SourceDocument::new(key(1), b"x", &mut op).unwrap();
        let control_bytes =
            size_of::<SessionScope>() + 2 * size_of::<usize>() + align_of::<SessionScope>() - 1;
        // Adversarial internal fixture: legacy construction refuses this mode.
        // Give it valid owners/tickets so each later guard is independently needed.
        let mut s = SourceSession {
            document: Some(document),
            budget: budget.clone(),
            config: ParseConfig::new(Profile::C0),
            epoch: 1,
            generation: 1,
            needs_resync: false,
            installed: None,
            scope: Arc::new(SessionScope {
                _control: budget.reserve_auxiliary(control_bytes as u64).unwrap(),
            }),
        };
        assert_eq!(
            s.capture(token.clone(), &mut op).err(),
            Some(CoreError::UnsupportedConfiguration)
        );
        assert_eq!(s.generation, 1);

        let request = || ParseRequest {
            source: s.snapshot().unwrap(),
            ticket: Ticket {
                identity: ModelIdentity::new(key(1), 1, s.config).unwrap(),
                epoch: s.epoch,
                scope: s.scope.clone(),
            },
            cancel: token.clone(),
        };
        assert_eq!(
            request().execute().err(),
            Some(CoreError::UnsupportedConfiguration)
        );
        let mut continued = elapsed_candidate_operation(&budget, token.clone());
        assert_eq!(
            request().execute_with_operation(&mut continued).err(),
            Some(CoreError::UnsupportedConfiguration)
        );

        let mut parse_op = elapsed_candidate_operation(&budget, token.clone());
        let model =
            AcceptedModel::parse(s.snapshot().unwrap(), s.config, 1, &mut parse_op).unwrap();
        let complete = CompletedParse {
            ticket: Ticket {
                identity: model.identity(),
                epoch: s.epoch,
                scope: s.scope.clone(),
            },
            model,
            cancel: token,
        };
        assert_eq!(
            s.install(complete).err(),
            Some(CoreError::UnsupportedConfiguration)
        );
        assert_eq!(s.generation, 1);
        assert_eq!(s.epoch, 1);
        assert!(s.installed().is_none());
        assert_eq!(s.snapshot().unwrap().text(), "x");
        drop(s);
        assert_eq!(budget.usage().source_capacity, 0);
        assert_eq!(budget.usage().source_owners, 0);
        assert_eq!(budget.usage().auxiliary_capacity, 0);
    }
}
fn elapsed_candidate_budgets() -> [SourceBudget; 1] {
    [SourceBudget::task_fixture()]
}

fn elapsed_candidate_operation(budget: &SourceBudget, token: CancellationToken) -> Operation {
    let mut op = budget.begin(token);
    if matches!(budget.admission_mode(), ResourceAdmissionMode::TaskAccess) {
        assert_eq!(op.first_fault(), None);
        op.set_clock_samples_for_test(&[499_999_999]);
    } else {
        op.set_clock_samples_for_test(&[100_000_000]);
    }
    op
}

fn operation(budget: &SourceBudget) -> Operation {
    budget.begin(CancellationToken::new())
}
fn session(budget: &SourceBudget, text: &[u8]) -> SourceSession {
    SourceSession::new(
        key(1),
        text,
        ParseConfig::new(Profile::C0),
        &mut operation(budget),
    )
    .unwrap()
}

#[test]
fn unattached_same_key_bytes_and_continued_operation_are_real() {
    let budget = root();
    let cancel = CancellationToken::new();
    let mut op = budget.begin(cancel.clone());
    let mut session = SourceSession::unattached(ParseConfig::new(Profile::C0), &mut op).unwrap();
    assert!(session.needs_resync());
    assert!(session.snapshot().is_none());
    session.resync(key(1), b"abc", &mut op).unwrap();
    let before = op.stats().expect("statistics enabled");
    let request = session.capture(cancel.clone(), &mut op).unwrap();
    let complete = request.execute_with_operation(&mut op).unwrap();
    assert!(op.stats().expect("statistics enabled").total() > before.total());
    let model = session.install(complete).unwrap();
    let generation = model.identity().generation();
    let mut invalid = operation(&budget);
    assert_eq!(
        session.resync(key(1), b"abd", &mut invalid).err(),
        Some(CoreError::WrongRevision {
            expected: 1,
            actual: 1
        })
    );
    assert_eq!(
        session.installed().unwrap().identity().generation(),
        generation
    );
    assert_eq!(session.snapshot().unwrap().text(), "abc");
    drop(model);
    drop(session);
    assert_eq!(budget.usage().source_capacity, 0);
    assert_eq!(budget.usage().auxiliary_capacity, 0);
}

#[test]
fn continued_execution_cannot_switch_root_flag_or_reset_budget() {
    let budget = root();
    let mut session = session(&budget, b"abc");
    let cancel = CancellationToken::new();
    let request = session
        .capture(cancel.clone(), &mut operation(&budget))
        .unwrap();
    assert!(matches!(
        request.execute_with_operation(&mut operation(&budget)),
        Err(CoreError::InvalidState)
    ));
    let request = session
        .capture(cancel.clone(), &mut operation(&budget))
        .unwrap();
    assert!(matches!(
        request.execute_with_operation(&mut root().begin(cancel.clone())),
        Err(CoreError::WrongBudget)
    ));
    let request = session
        .capture(cancel.clone(), &mut operation(&budget))
        .unwrap();
    let mut op = budget.begin(cancel);
    while op.stats().expect("statistics enabled").total() < budget.limits().max_work {
        let count = (budget.limits().max_work - op.stats().expect("statistics enabled").total())
            .min(65_536);
        op.external_read(count).unwrap();
    }
    // Use the public bounded meter instead of resetting counters for execution.
    assert!(matches!(
        request.execute_with_operation(&mut op),
        Err(CoreError::ResourceLimit {
            kind: crate::error::LimitKind::Work,
            ..
        })
    ));
}

#[test]
fn original_edit_sequence_distinguishes_stale_and_lost_chain() {
    // Fixed E-EDIT-01 project.edits.atomic-old-coordinate-resync.
    let budget = root();
    let mut s = session(&budget, b"abcdef");
    let edits = [
        Edit {
            range: SourceSpan::new(1, 3).unwrap(),
            replacement: b"X",
        },
        Edit {
            range: SourceSpan::new(4, 4).unwrap(),
            replacement: b"YZ",
        },
    ];
    let first = || EditBatch {
        before: key(1),
        after: key(7),
        edits: &edits,
    };
    assert_eq!(
        s.apply(first(), &mut operation(&budget)).unwrap().text(),
        "aXdYZef"
    );
    assert_eq!(
        s.apply(first(), &mut operation(&budget)).err(),
        Some(CoreError::WrongRevision {
            expected: 7,
            actual: 7
        })
    );
    assert!(!s.needs_resync());
    assert_eq!(
        s.apply(
            EditBatch {
                before: key(6),
                after: key(8),
                edits: &[]
            },
            &mut operation(&budget)
        )
        .err(),
        Some(CoreError::ResyncRequired {
            expected: 7,
            actual: 6
        })
    );
    assert!(s.needs_resync());
    assert_eq!(s.snapshot().unwrap().text(), "aXdYZef");
    assert!(
        s.capture(CancellationToken::new(), &mut operation(&budget))
            .is_err()
    );
    assert_eq!(
        s.resync(key(9), "中😀\r\n".as_bytes(), &mut operation(&budget))
            .unwrap()
            .text(),
        "中😀\r\n"
    );
    let edits = [Edit {
        range: SourceSpan::new(8, 9).unwrap(),
        replacement: b"",
    }];
    assert_eq!(
        s.apply(
            EditBatch {
                before: key(9),
                after: key(10),
                edits: &edits
            },
            &mut operation(&budget)
        )
        .unwrap()
        .text(),
        "中😀\r"
    );
    let edits = [Edit {
        range: SourceSpan::new(4, 5).unwrap(),
        replacement: b"",
    }];
    assert_eq!(
        s.apply(
            EditBatch {
                before: key(10),
                after: key(11),
                edits: &edits
            },
            &mut operation(&budget)
        )
        .err(),
        Some(CoreError::InvalidBoundary)
    );
    assert_eq!(s.snapshot().unwrap().key(), key(10));
    assert_eq!(
        s.apply(
            EditBatch {
                before: key(10),
                after: key(20),
                edits: &[]
            },
            &mut operation(&budget)
        )
        .unwrap()
        .key(),
        key(20)
    );
}

#[test]
fn actual_model_maps_atomic_entity_and_releases_last_pin() {
    let budget = root();
    let mut s = session(&budget, b"**a&amp;b**");
    let request = s
        .capture(CancellationToken::new(), &mut operation(&budget))
        .unwrap();
    let model = s.install(request.execute().unwrap()).unwrap();
    let flow = model.ir().nodes()[0].flow;
    let query = PointQuery {
        model: model.identity(),
        input: PointInput::Flow {
            flow,
            utf16: LogicalBoundary::new(1),
        },
        affinity: Affinity::After,
    };
    let response = model.map_point(&query, &mut operation(&budget)).unwrap();
    assert_eq!(
        response.results()[0].chosen,
        Position::Source(crate::identity::SourceBoundary::new(3))
    );
    assert_eq!(response.results()[0].correspondence, Correspondence::Exact);
    let retained = model.clone();
    s.detach(&mut operation(&budget)).unwrap();
    drop(model);
    assert!(budget.usage().source_capacity > 0);
    assert!(retained.map_point(&query, &mut operation(&budget)).is_ok());
    drop(retained);
    assert_eq!(budget.usage().source_capacity, 0);
    drop(response);
    drop(s);
    assert_eq!(budget.usage().auxiliary_capacity, 0);
    assert_eq!(budget.usage().source_owners, 0);
}

#[test]
fn aba_and_superseded_tickets_never_install() {
    let budget = root();
    let mut s = session(&budget, b"old");
    let old = s
        .capture(CancellationToken::new(), &mut operation(&budget))
        .unwrap()
        .execute()
        .unwrap();
    s.resync(key(2), b"other", &mut operation(&budget)).unwrap();
    s.resync(key(1), b"old", &mut operation(&budget)).unwrap();
    assert!(matches!(s.install(old), Err(CoreError::StaleResult)));
    assert!(s.installed().is_none());
    let first = s
        .capture(CancellationToken::new(), &mut operation(&budget))
        .unwrap();
    let last = s
        .capture(CancellationToken::new(), &mut operation(&budget))
        .unwrap();
    assert!(matches!(
        s.install(first.execute().unwrap()),
        Err(CoreError::StaleResult)
    ));
    assert!(s.install(last.execute().unwrap()).is_ok());
}

#[test]
fn cancellation_before_execute_and_after_acceptance_prevents_publish() {
    let budget = root();
    let mut s = session(&budget, b"hello");
    let request = s
        .capture(CancellationToken::new(), &mut operation(&budget))
        .unwrap();
    request.cancellation().cancel();
    assert!(matches!(request.execute(), Err(CoreError::Cancelled)));
    let request = s
        .capture(CancellationToken::new(), &mut operation(&budget))
        .unwrap();
    let cancel = request.cancellation();
    let completed = request.execute().unwrap();
    cancel.cancel();
    assert!(matches!(s.install(completed), Err(CoreError::Cancelled)));
    assert!(s.installed().is_none());
    drop(s);
    assert_eq!(budget.usage().auxiliary_capacity, 0);
}

#[test]
fn generation_and_epoch_exhaustion_preserve_current_source() {
    let budget = root();
    let mut s = session(&budget, b"hello");
    s.generation = u64::MAX;
    assert!(matches!(
        s.capture(CancellationToken::new(), &mut operation(&budget)),
        Err(CoreError::ArithmeticOverflow)
    ));
    s.epoch = u64::MAX;
    assert!(matches!(
        s.resync(key(2), b"changed", &mut operation(&budget)),
        Err(CoreError::ArithmeticOverflow)
    ));
    assert_eq!(s.snapshot().unwrap().text(), "hello");
    // Final destruction remains possible even when no new epoch can be issued.
    drop(s);
    assert_eq!(budget.usage().source_capacity, 0);
}

#[test]
fn wrong_budget_does_not_publish() {
    let budget = root();
    let mut s = session(&budget, b"hello");
    assert!(matches!(
        s.capture(CancellationToken::new(), &mut operation(&root())),
        Err(CoreError::WrongBudget)
    ));
    assert!(s.installed().is_none());
    drop(s);
    assert_eq!(budget.usage().auxiliary_capacity, 0);
}

#[test]
fn distinct_sessions_with_identical_ticket_fields_cannot_install_each_other() {
    let budget = root();
    let mut a = session(&budget, b"AAAA");
    let mut b = session(&budget, b"BBBB");
    let request_a = a
        .capture(CancellationToken::new(), &mut operation(&budget))
        .unwrap();
    let request_b = b
        .capture(CancellationToken::new(), &mut operation(&budget))
        .unwrap();
    assert_eq!(request_a.identity(), request_b.identity());
    assert!(matches!(
        b.install(request_a.execute().unwrap()),
        Err(CoreError::StaleResult)
    ));
    assert_eq!(b.snapshot().unwrap().text(), "BBBB");
    assert!(b.installed().is_none());
    assert_eq!(
        b.install(request_b.execute().unwrap())
            .unwrap()
            .source()
            .text(),
        "BBBB"
    );
    drop(a);
    drop(b);
    assert_eq!(budget.usage().source_capacity, 0);
    assert_eq!(budget.usage().auxiliary_capacity, 0);
}

#[test]
fn detached_session_scope_lives_until_last_request_is_released() {
    let budget = root();
    let mut s = session(&budget, b"old");
    let request = s
        .capture(CancellationToken::new(), &mut operation(&budget))
        .unwrap();
    drop(s);
    assert!(budget.usage().auxiliary_capacity > 0);
    assert!(budget.usage().source_capacity > 0);
    drop(request);
    assert_eq!(budget.usage().source_capacity, 0);
    assert_eq!(budget.usage().auxiliary_capacity, 0);
}
