// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE file in the project root for license information.
// ---------------------------------------------------------------------------------------------

use super::*;
use crate::config::Profile;
use crate::error::LimitKind;
use crate::identity::SourceSpan;
use crate::policy::SourceLimits;
use crate::source::{Edit, SourceDocument};
use std::sync::atomic::{AtomicU64, Ordering};

fn key(revision: u64) -> SourceKey {
    SourceKey::new([71; 16], revision).unwrap()
}
fn config() -> ParseConfig {
    ParseConfig::new(Profile::C0)
}
fn session(root: &SourceBudget) -> SourceSession {
    SourceSession::transactional(config(), root.clone(), CancellationToken::new()).unwrap()
}
fn released(root: &SourceBudget) {
    let usage = root.usage();
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
fn pending_request_is_send_and_owns_fresh_independent_cancellation() {
    fn send<T: Send>() {}
    send::<PendingSourceRequest>();
    let root = SourceBudget::task_fixture();
    let mut session = session(&root);
    let first = session.start_request(false).unwrap();
    let second = session.start_request(false).unwrap();
    first.cancellation().cancel();
    assert!(matches!(
        first.bind(&mut session),
        Err(CoreError::Cancelled)
    ));
    let mut tx = second.bind(&mut session).unwrap();
    tx.resync(key(1), b"accepted").unwrap();
    let completed = tx.capture().unwrap().execute().unwrap();
    let model = session.install_owned(completed).unwrap();
    assert_eq!(model.source().text(), "accepted");
    drop(model);
    drop(session);
    released(&root);
}

#[test]
fn pending_bind_checks_scope_and_entire_origin_without_restarting() {
    let root = SourceBudget::task_fixture();
    let mut session = session(&root);
    let stale = session.start_request(false).unwrap();
    let mut current = session
        .start_request(false)
        .unwrap()
        .bind(&mut session)
        .unwrap();
    current.resync(key(1), b"new").unwrap();
    drop(current.capture().unwrap());
    assert!(matches!(
        stale.bind(&mut session),
        Err(CoreError::StaleResult)
    ));
    let config_stale = session.start_request(false).unwrap();
    session
        .begin_transaction(CancellationToken::new())
        .set_config(ParseConfig::new(Profile::G3))
        .unwrap();
    assert!(matches!(
        config_stale.bind(&mut session),
        Err(CoreError::StaleResult)
    ));
    let wrong_scope = session.start_request(false).unwrap();
    let mut other = super::boundary_tests::session(&root);
    assert!(!wrong_scope.matches_scope(&other));
    assert!(matches!(
        wrong_scope.bind(&mut other),
        Err(CoreError::StaleResult)
    ));
    assert_eq!(session.snapshot().unwrap().text(), "new");
    drop(other);
    drop(session);
    released(&root);
}

#[test]
fn pending_queue_and_boundary_reservation_keep_the_original_clock_and_pins() {
    let root = SourceBudget::task_fixture();
    let mut session = session(&root);
    let clock = Arc::new(AtomicU64::new(499_999_900));
    let mut pending = session.start_request(false).unwrap();
    pending.op.set_shared_clock_for_test(clock.clone());
    let lease = pending.boundary_work().reserve_boundary(4096).unwrap();
    clock.store(500_000_000, Ordering::Relaxed);
    assert!(matches!(
        pending.bind(&mut session),
        Err(CoreError::DeadlineExceeded { .. })
    ));
    assert!(session.snapshot().is_none());
    drop(session);
    assert_eq!(root.usage().auxiliary_capacity, 4096);
    drop(lease);
    released(&root);
}

#[test]
fn utf16_real_model_matches_utf8_source_and_independent_main_oracle() {
    use crate::reader::MainReader;
    let root = SourceBudget::task_fixture();
    let mut session = session(&root);
    let original = "**中&amp;😀**\r\n";
    let input: Vec<u16> = original.encode_utf16().collect();
    let mut tx = session
        .start_request(false)
        .unwrap()
        .bind(&mut session)
        .unwrap();
    tx.resync_utf16_with_config(key(1), &input, config())
        .unwrap();
    let completed = tx.capture().unwrap().execute().unwrap();
    let model = session.install_owned(completed).unwrap();
    assert_eq!(model.source().text(), original);
    let op = root.begin_cancellable(false).unwrap();
    let mut reader = MainReader::new(model.clone(), op).unwrap();
    let mut buffer = [0_u8; 128];
    let read = reader.read_into(&mut buffer).unwrap();
    assert_eq!(&buffer[..read.written], "中&😀".as_bytes());
    drop(reader);
    drop(model);
    drop(session);
    released(&root);
}

#[test]
fn utf16_decoder_carries_surrogates_across_quantum_and_preserves_nul_crlf() {
    let root = SourceBudget::task_fixture();
    let mut op = root.begin_cancellable(false).unwrap();
    let original = format!("{}😀中\0\r\n", "a".repeat(255));
    let input: Vec<u16> = original.encode_utf16().collect();
    let document = SourceDocument::new_utf16(key(1), &input, &mut op).unwrap();
    assert_eq!(document.snapshot().text(), original);
    drop(document);
    drop(op);
    released(&root);
}

#[test]
fn invalid_utf16_reports_u16_offset_is_sticky_and_preserves_installed() {
    let root = SourceBudget::task_fixture();
    let mut session = session(&root);
    let mut tx = session
        .start_request(false)
        .unwrap()
        .bind(&mut session)
        .unwrap();
    tx.resync(key(1), b"old").unwrap();
    let completed = tx.capture().unwrap().execute().unwrap();
    let installed = session.install_owned(completed).unwrap();
    let mut pending = session.start_request(false).unwrap();
    let cancel = pending.cancellation();
    pending.op.set_clock_samples_for_test(&[499_999_900]);
    let mut tx = pending.bind(&mut session).unwrap();
    assert!(matches!(
        tx.resync_utf16_with_config(key(2), &[0x4e2d, 0xd800], config()),
        Err(CoreError::InvalidUtf16 { offset: 1 })
    ));
    cancel.cancel();
    assert!(matches!(
        tx.resync(key(2), b"valid"),
        Err(CoreError::InvalidUtf16 { offset: 1 })
    ));
    drop(tx);
    assert_eq!(session.snapshot().unwrap().text(), "old");
    assert_eq!(
        session.installed().unwrap().identity(),
        installed.identity()
    );
    drop(cancel);
    drop(installed);
    drop(session);
    released(&root);
}

#[test]
fn utf16_same_key_compare_and_encoding_failure_do_not_commit_new_config() {
    let root = SourceBudget::task_fixture();
    let mut session = session(&root);
    let mut first = session
        .start_request(false)
        .unwrap()
        .bind(&mut session)
        .unwrap();
    first.resync(key(1), b"same").unwrap();
    drop(first);
    let mut second = session
        .start_request(false)
        .unwrap()
        .bind(&mut session)
        .unwrap();
    assert!(matches!(
        second.resync_utf16_with_config(key(1), &[b'x' as u16], ParseConfig::new(Profile::G3)),
        Err(CoreError::WrongRevision { .. })
    ));
    drop(second);
    assert_eq!(session.config, config());
    let mut third = session
        .start_request(false)
        .unwrap()
        .bind(&mut session)
        .unwrap();
    assert!(matches!(
        third.resync_utf16_with_config(key(1), &[b'x' as u16, 0xdc00], config()),
        Err(CoreError::InvalidUtf16 { offset: 1 })
    ));
    drop(third);
    let utf16: Vec<_> = "same".encode_utf16().collect();
    session
        .start_request(false)
        .unwrap()
        .bind(&mut session)
        .unwrap()
        .resync_utf16_with_config(key(1), &utf16, config())
        .unwrap();
    assert_eq!(session.snapshot().unwrap().text(), "same");
    drop(session);
    released(&root);
}

#[test]
fn utf16_output_expansion_is_checked_before_allocating_source() {
    let root = SourceBudget::new(SourceLimits {
        max_source_bytes: 3,
        ..SourceLimits::default()
    })
    .unwrap();
    let mut op = root.begin_cancellable(false).unwrap();
    assert!(matches!(
        SourceDocument::new_utf16(key(1), &[0x4e2d, 0x6587], &mut op),
        Err(CoreError::ResourceLimit {
            kind: LimitKind::SourceBytes,
            requested: 6,
            limit: 3
        })
    ));
    assert_eq!(root.usage().source_capacity, 0);
    assert_eq!(root.usage().source_owners, 0);
    drop(op);
    released(&root);
}

#[test]
fn replacing_parse_ticket_never_discards_already_committed_edits() {
    let root = SourceBudget::task_fixture();
    let mut session = session(&root);
    let mut a = session
        .start_request(false)
        .unwrap()
        .bind(&mut session)
        .unwrap();
    a.resync(key(1), b"A").unwrap();
    let a = a.capture().unwrap();
    let mut b = session
        .start_request(false)
        .unwrap()
        .bind(&mut session)
        .unwrap();
    b.apply(EditBatch {
        before: key(1),
        after: key(2),
        edits: &[Edit {
            range: SourceSpan::new(1, 1).unwrap(),
            replacement: b"B",
        }],
    })
    .unwrap();
    let b = b.capture().unwrap();
    drop(b);
    let mut c = session
        .start_request(false)
        .unwrap()
        .bind(&mut session)
        .unwrap();
    c.apply(EditBatch {
        before: key(2),
        after: key(3),
        edits: &[Edit {
            range: SourceSpan::new(2, 2).unwrap(),
            replacement: b"C",
        }],
    })
    .unwrap();
    let completed = c.capture().unwrap().execute().unwrap();
    let installed = session.install_owned(completed).unwrap();
    assert_eq!(installed.source().text(), "ABC");
    assert!(matches!(
        session.install_owned(a.execute().unwrap()),
        Err(CoreError::StaleResult)
    ));
    drop(installed);
    drop(session);
    released(&root);
}
