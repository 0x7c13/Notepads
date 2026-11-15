// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE file in the project root for license information.
// ---------------------------------------------------------------------------------------------

use super::*;
use crate::identity::ParseSpan;
use crate::policy::{CHECKPOINT_BYTES, CancellationToken, SourceLimits};

fn key(revision: u64) -> SourceKey {
    SourceKey::new([7; 16], revision).unwrap()
}

fn range(start: u64, end: u64) -> SourceSpan {
    SourceSpan::new(start, end).unwrap()
}

fn operation(budget: &SourceBudget) -> Operation {
    budget.begin(CancellationToken::new())
}

fn document(bytes: &[u8], budget: &SourceBudget) -> SourceDocument {
    SourceDocument::new(key(1), bytes, &mut operation(budget)).unwrap()
}

fn apply(
    document: &mut SourceDocument,
    revision: u64,
    edits: &[Edit<'_>],
    budget: &SourceBudget,
) -> Result<SourceSnapshot, CoreError> {
    document.apply(
        EditBatch {
            before: document.snapshot().key(),
            after: key(revision),
            edits,
        },
        &mut operation(budget),
    )
}

fn assert_released(budget: &SourceBudget) {
    let usage = budget.usage();
    assert_eq!(usage.source_capacity, 0);
    assert_eq!(usage.source_owners, 0);
    assert_eq!(usage.auxiliary_capacity, 0);
}

#[test]
fn snapshot_allocation_deadline_drops_private_source_and_preserves_old_pin() {
    let budget = SourceBudget::task_fixture();
    let mut preparation = operation(&budget);
    preparation.set_clock_samples_for_test(&[100_000_000]);
    let doc = SourceDocument::new(key(1), b"old", &mut preparation).unwrap();
    let old = doc.snapshot();
    let owner = OwnerLease::new(&mut preparation).unwrap();
    let mut text = ChargedString::source(3, &mut preparation).unwrap();
    text.append("new", &mut preparation).unwrap();
    assert_eq!(budget.usage().source_capacity, 6);
    assert_eq!(budget.usage().source_owners, 2);

    let mut op = operation(&budget);
    op.set_clock_samples_for_test(&[100_000_000, 500_000_000]);
    let deadline = CoreError::DeadlineExceeded {
        elapsed_ns: 500_000_000,
        limit_ns: 500_000_000,
    };
    assert_eq!(snapshot(key(2), text, owner, &mut op).err(), Some(deadline));
    assert_eq!(op.first_fault(), Some(deadline));
    assert_eq!(doc.snapshot().key(), key(1));
    assert_eq!(old.key(), key(1));
    assert_eq!(old.text(), "old");
    assert_eq!(budget.usage().source_capacity, 3);
    assert_eq!(budget.usage().source_owners, 1);
    drop(doc);
    assert_eq!(budget.usage().source_owners, 1);
    drop(old);
    assert_released(&budget);
}

#[test]
fn apply_final_deadline_does_not_publish_the_fully_built_after_source() {
    let budget = SourceBudget::task_fixture();
    let mut preparation = operation(&budget);
    preparation.set_clock_samples_for_test(&[100_000_000]);
    let mut doc = SourceDocument::new(key(1), b"old", &mut preparation).unwrap();
    let old = doc.snapshot();
    let mut op = operation(&budget);
    // Entry, buffer reserve pair and snapshot allocation pair remain in time;
    // the final transaction check expires before swapping current.
    op.set_clock_samples_for_test(&[
        100_000_000,
        100_000_000,
        100_000_000,
        100_000_000,
        100_000_000,
        500_000_000,
    ]);
    let deadline = CoreError::DeadlineExceeded {
        elapsed_ns: 500_000_000,
        limit_ns: 500_000_000,
    };
    let edits = [Edit {
        range: range(0, 3),
        replacement: b"new",
    }];
    assert_eq!(
        doc.apply(
            EditBatch {
                before: key(1),
                after: key(2),
                edits: &edits,
            },
            &mut op,
        )
        .err(),
        Some(deadline)
    );
    assert_eq!(op.first_fault(), Some(deadline));
    assert_eq!(doc.snapshot().key(), key(1));
    assert_eq!(doc.snapshot().text(), "old");
    assert_eq!(old.text(), "old");
    assert_eq!(budget.usage().source_capacity, 3);
    assert!(budget.usage().source_capacity_peak >= 6);
    assert_eq!(budget.usage().source_owners, 1);

    let mut later = operation(&budget);
    later.set_clock_samples_for_test(&[100_000_000]);
    let after = doc
        .apply(
            EditBatch {
                before: key(1),
                after: key(2),
                edits: &edits,
            },
            &mut later,
        )
        .unwrap();
    assert_eq!(after.text(), "new");
    assert_eq!(old.text(), "old");
    assert_eq!(budget.usage().source_owners, 2);
    drop(after);
    drop(doc);
    assert_eq!(budget.usage().source_owners, 1);
    drop(old);
    assert_released(&budget);
}

#[test]
fn original_source_is_owned_and_scalar_boundaries_are_checked() {
    let budget = SourceBudget::source_fixture();
    let original = "\u{feff}中\r\n\0😀\rZ";
    let mut input = original.as_bytes().to_vec();
    let source = document(&input, &budget).snapshot();
    input.fill(b'x');
    assert_eq!(source.text(), original);
    assert_eq!(source.key(), key(1));
    assert_eq!(source.validate_span(range(0, 15)), Ok(()));
    assert_eq!(source.validate_span(range(7, 7)), Ok(())); // inside CRLF, legal S scalar boundary
    assert_eq!(
        source.validate_span(range(1, 1)),
        Err(CoreError::InvalidBoundary)
    );
    assert_eq!(
        source.validate_span(range(10, 11)),
        Err(CoreError::InvalidBoundary)
    );
    assert_eq!(
        source.validate_span(range(0, 16)),
        Err(CoreError::InvalidBoundary)
    );
    drop(source);
    assert_released(&budget);
}

#[test]
fn independent_normalization_fixture_has_exact_bytes_and_six_atoms() {
    let budget = SourceBudget::source_fixture();
    let input = include_bytes!("../../../../tests/fixtures/m1-range/inputs/normalization.bin");
    assert_eq!(input, b"\xe4\xb8\xad\r\n\0\xf0\x9f\x98\x80\rZ");
    let source = document(input, &budget).snapshot();
    let prepared = ParseInput::prepare(source.clone(), &mut operation(&budget)).unwrap();
    assert_eq!(prepared.source().text().as_bytes(), input);
    assert_eq!(prepared.text(), "中\n�😀\nZ");
    assert_eq!(prepared.text().len(), 13);
    assert!(!prepared.is_identity());
    let expected = [
        (0, 3, 0, 3, InputPieceKind::Copy),
        (3, 5, 3, 4, InputPieceKind::CrLf),
        (5, 6, 4, 7, InputPieceKind::Nul),
        (6, 10, 7, 11, InputPieceKind::Copy),
        (10, 11, 11, 12, InputPieceKind::Cr),
        (11, 12, 12, 13, InputPieceKind::Copy),
    ];
    for (piece, &(s0, s1, p0, p1, kind)) in prepared.pieces().iter().zip(&expected) {
        assert_eq!(piece.source, range(s0, s1));
        assert_eq!(piece.parse, ParseSpan::new(p0, p1).unwrap());
        assert_eq!(piece.kind, kind);
    }
    assert_eq!(prepared.pieces().len(), expected.len());
    assert_eq!(budget.usage().source_owners, 1);
    assert!(budget.usage().auxiliary_capacity >= 13 + 6 * size_of::<InputPiece>() as u64);
    drop(source);
    assert_eq!(budget.usage().source_owners, 1);
    drop(prepared);
    assert_released(&budget);
}

#[test]
fn identity_input_borrows_same_source_and_keeps_bom_tabs_entities_literal() {
    let budget = SourceBudget::source_fixture();
    let source = document("\u{feff}\t&amp;😀\n".as_bytes(), &budget).snapshot();
    let prepared = ParseInput::prepare(source.clone(), &mut operation(&budget)).unwrap();
    assert!(prepared.is_identity());
    assert_eq!(prepared.text().as_ptr(), source.text().as_ptr());
    assert_eq!(prepared.text(), source.text());
    assert_eq!(prepared.pieces().len(), 1);
    assert_eq!(prepared.pieces()[0].kind, InputPieceKind::Copy);
    assert_eq!(
        budget.usage().auxiliary_capacity,
        size_of::<InputPiece>() as u64
    );
    drop(prepared);
    drop(source);
    assert_released(&budget);

    let source = document(b"", &budget).snapshot();
    let prepared = ParseInput::prepare(source, &mut operation(&budget)).unwrap();
    assert!(prepared.is_identity());
    assert!(prepared.pieces().is_empty());
    assert_eq!(prepared.text(), "");
    assert_eq!(budget.usage().auxiliary_capacity, 0);
    drop(prepared);
    assert_released(&budget);
}

#[test]
fn n01_preserves_atoms_at_byte_windows_and_merges_copy_runs() {
    let budget = SourceBudget::source_fixture();
    let boundary = CHECKPOINT_BYTES as usize;
    for padding in boundary - 4..=boundary + 4 {
        let prefix = "x".repeat(padding);
        let input = format!("{prefix}中\r\n\0😀\rZ");
        let expected = format!("{prefix}中\n�😀\nZ");
        let source = document(input.as_bytes(), &budget).snapshot();
        let prepared = ParseInput::prepare(source, &mut operation(&budget)).unwrap();
        assert_eq!(prepared.text(), expected);
        assert_eq!(prepared.pieces().len(), 6);
        assert_eq!(prepared.pieces()[0].source, range(0, (padding + 3) as u64));
        for pair in prepared.pieces().windows(2) {
            assert_eq!(pair[0].source.end(), pair[1].source.start());
            assert_eq!(pair[0].parse.end(), pair[1].parse.start());
        }
    }
    // CRLF exactly straddles the nominal block boundary.
    let input = format!("{}\r\nY", "x".repeat(boundary - 1));
    let prepared = ParseInput::prepare(
        document(input.as_bytes(), &budget).snapshot(),
        &mut operation(&budget),
    )
    .unwrap();
    assert_eq!(prepared.pieces().len(), 3);
    assert_eq!(prepared.pieces()[1].kind, InputPieceKind::CrLf);
    assert_eq!(prepared.pieces()[1].source.len(), 2);
    drop(prepared);

    let input = "😀".repeat(boundary);
    let prepared = ParseInput::prepare(
        document(input.as_bytes(), &budget).snapshot(),
        &mut operation(&budget),
    )
    .unwrap();
    assert!(prepared.is_identity());
    assert_eq!(prepared.pieces().len(), 1);
    drop(prepared);
    assert_released(&budget);
}

#[test]
fn adjacent_n01_transforms_are_not_coalesced() {
    let budget = SourceBudget::source_fixture();
    let prepared = ParseInput::prepare(
        document(b"\r\n\r\0\0", &budget).snapshot(),
        &mut operation(&budget),
    )
    .unwrap();
    assert_eq!(prepared.text(), "\n\n��");
    assert_eq!(prepared.pieces().len(), 4);
    assert_eq!(prepared.pieces()[0].kind, InputPieceKind::CrLf);
    assert_eq!(prepared.pieces()[1].kind, InputPieceKind::Cr);
    assert_eq!(prepared.pieces()[2].kind, InputPieceKind::Nul);
    assert_eq!(prepared.pieces()[3].kind, InputPieceKind::Nul);
}

#[test]
fn invalid_utf8_reports_original_offset_and_releases_private_source() {
    let budget = SourceBudget::source_fixture();
    for (bytes, offset) in [
        (&b"a\xff"[..], 1),
        (&b"\xed\xa0\x80"[..], 0),
        (&b"abc\xf0\x9f"[..], 3),
    ] {
        let mut op = operation(&budget);
        let result = SourceDocument::new(key(1), bytes, &mut op);
        assert_eq!(result.err(), Some(CoreError::InvalidUtf8 { offset }));
        assert_eq!(op.first_fault(), Some(CoreError::InvalidUtf8 { offset }));
        assert_released(&budget);
    }
    for padding in CHECKPOINT_BYTES as usize - 3..=CHECKPOINT_BYTES as usize {
        let mut bytes = vec![b'a'; padding];
        bytes.extend_from_slice(b"\xf0\x9f\x98\x80\xff");
        assert_eq!(
            SourceDocument::new(key(1), &bytes, &mut operation(&budget)).err(),
            Some(CoreError::InvalidUtf8 {
                offset: padding as u64 + 4
            })
        );
        assert_released(&budget);
    }
}

#[test]
fn validation_prefix_rechecks_are_metered_and_copy_is_once() {
    let budget = SourceBudget::source_fixture();
    let bytes = format!("{}😀Z", "x".repeat(CHECKPOINT_BYTES as usize - 1));
    let mut op = operation(&budget);
    let source = SourceDocument::new(key(1), bytes.as_bytes(), &mut op).unwrap();
    assert_eq!(
        op.stats().expect("statistics enabled").scanned_bytes(),
        65_536 + 65_535 + 5
    );
    assert_eq!(
        op.stats().expect("statistics enabled").copied_bytes(),
        bytes.len() as u64
    );
    assert_eq!(source.snapshot().text(), bytes);
}

#[test]
fn edits_use_one_before_coordinate_system_and_retained_snapshots_stay_immutable() {
    let budget = SourceBudget::source_fixture();
    let mut doc = document("a中b😀c".as_bytes(), &budget);
    let old = doc.snapshot();
    let edits = [
        Edit {
            range: range(1, 4),
            replacement: b"Q",
        },
        Edit {
            range: range(5, 9),
            replacement: "界".as_bytes(),
        },
    ];
    let after = apply(&mut doc, 7, &edits, &budget).unwrap();
    assert_eq!(after.text(), "aQb界c");
    assert_eq!(after.key(), key(7));
    assert_eq!(old.text(), "a中b😀c");
    assert_eq!(old.key(), key(1));
    assert_eq!(budget.usage().source_owners, 2);
    drop(old);
    assert_eq!(budget.usage().source_owners, 1);
    drop(doc);
    assert_eq!(after.text(), "aQb界c");
    drop(after);
    assert_released(&budget);
}

#[test]
fn edit_key_failures_are_typed_and_do_not_poison_new_operations() {
    let budget = SourceBudget::source_fixture();
    let mut doc = document(b"before", &budget);
    for (before, after, expected) in [
        (
            SourceKey::new([8; 16], 1).unwrap(),
            key(2),
            CoreError::WrongDocument,
        ),
        (
            key(1),
            SourceKey::new([8; 16], 2).unwrap(),
            CoreError::WrongDocument,
        ),
        (
            key(3),
            key(4),
            CoreError::ResyncRequired {
                expected: 1,
                actual: 3,
            },
        ),
        (key(1), key(1), CoreError::InvalidRevision),
    ] {
        let result = doc.apply(
            EditBatch {
                before,
                after,
                edits: &[],
            },
            &mut operation(&budget),
        );
        assert_eq!(result.err(), Some(expected));
        assert_eq!(doc.snapshot().text(), "before");
        assert_eq!(doc.snapshot().key(), key(1));
    }
    apply(&mut doc, 3, &[], &budget).unwrap();
    let result = doc.apply(
        EditBatch {
            before: key(1),
            after: key(4),
            edits: &[],
        },
        &mut operation(&budget),
    );
    assert_eq!(
        result.err(),
        Some(CoreError::WrongRevision {
            expected: 3,
            actual: 1
        })
    );
    assert_eq!(doc.snapshot().key(), key(3));
}

#[test]
fn reordered_overlapping_and_same_start_edits_are_rejected() {
    let budget = SourceBudget::source_fixture();
    let mut doc = document(b"abcdef", &budget);
    for ranges in [[(3, 4), (1, 2)], [(1, 3), (2, 4)], [(1, 1), (1, 2)]] {
        let edits = ranges.map(|(start, end)| Edit {
            range: range(start, end),
            replacement: b"Q",
        });
        assert_eq!(
            apply(&mut doc, 2, &edits, &budget).err(),
            Some(CoreError::InvalidEdits)
        );
        assert_eq!(doc.snapshot().text(), "abcdef");
        assert_eq!(budget.usage().source_owners, 1);
    }
    let adjacent = [
        Edit {
            range: range(1, 2),
            replacement: b"X",
        },
        Edit {
            range: range(2, 3),
            replacement: b"Y",
        },
    ];
    assert_eq!(
        apply(&mut doc, 2, &adjacent, &budget).unwrap().text(),
        "aXYdef"
    );
}

#[test]
fn invalid_edit_boundaries_leave_original_source_unchanged() {
    let budget = SourceBudget::source_fixture();
    let mut doc = document("中😀".as_bytes(), &budget);
    for (start, end) in [(1, 3), (3, 5), (0, 8)] {
        let edits = [Edit {
            range: range(start, end),
            replacement: b"x",
        }];
        assert_eq!(
            apply(&mut doc, 2, &edits, &budget).err(),
            Some(CoreError::InvalidBoundary)
        );
        assert_eq!(doc.snapshot().text(), "中😀");
    }
}

#[test]
fn replacement_errors_report_original_edit_index_and_local_byte_offset() {
    let budget = SourceBudget::source_fixture();
    let mut empty = document(b"", &budget);
    let edits = [Edit {
        range: range(0, 0),
        replacement: b"a\xff",
    }];
    assert_eq!(
        apply(&mut empty, 2, &edits, &budget).err(),
        Some(CoreError::InvalidEditUtf8 {
            edit_index: 0,
            offset: 1
        })
    );
    assert_eq!(empty.snapshot().text(), "");
    let mut doc = document(b"a", &budget);
    let mut replacement = vec![b'z'; CHECKPOINT_BYTES as usize - 1];
    replacement.extend_from_slice(b"\xf0\xff");
    let edits = [
        Edit {
            range: range(0, 0),
            replacement: b"first",
        },
        Edit {
            range: range(1, 1),
            replacement: &replacement,
        },
    ];
    assert_eq!(
        apply(&mut doc, 2, &edits, &budget).err(),
        Some(CoreError::InvalidEditUtf8 {
            edit_index: 1,
            offset: CHECKPOINT_BYTES - 1
        })
    );
    assert_eq!(doc.snapshot().text(), "a");
    assert_eq!(doc.snapshot().key(), key(1));
    assert_eq!(budget.usage().source_owners, 2);
}

#[test]
fn source_byte_edit_and_insert_limits_are_inclusive() {
    let budget = SourceBudget::new(SourceLimits {
        max_source_bytes: 4,
        max_edits: 1,
        max_insert_bytes: 2,
        ..SourceLimits::default()
    })
    .unwrap();
    let mut doc = document(b"abcd", &budget);
    assert!(matches!(
        SourceDocument::new(key(1), b"abcde", &mut operation(&budget)).err(),
        Some(CoreError::ResourceLimit {
            kind: LimitKind::SourceBytes,
            requested: 5,
            limit: 4
        })
    ));
    let replace = [Edit {
        range: range(1, 3),
        replacement: b"XY",
    }];
    assert_eq!(
        apply(&mut doc, 2, &replace, &budget).unwrap().text(),
        "aXYd"
    );
    let oversized = [Edit {
        range: range(1, 3),
        replacement: b"XYZ",
    }];
    assert!(matches!(
        apply(&mut doc, 3, &oversized, &budget).err(),
        Some(CoreError::ResourceLimit {
            kind: LimitKind::InsertBytes,
            ..
        })
    ));
    let too_many = [
        Edit {
            range: range(0, 0),
            replacement: b"",
        },
        Edit {
            range: range(1, 1),
            replacement: b"",
        },
    ];
    assert!(matches!(
        apply(&mut doc, 3, &too_many, &budget).err(),
        Some(CoreError::ResourceLimit {
            kind: LimitKind::Edits,
            ..
        })
    ));
    assert_eq!(doc.snapshot().key(), key(2));
}

#[test]
fn old_pins_count_distinct_owners_until_last_clone_drops() {
    let budget = SourceBudget::new(SourceLimits {
        retained_source_owners: 2,
        retained_source_capacity: 8,
        ..SourceLimits::default()
    })
    .unwrap();
    let mut doc = document(b"data", &budget);
    let old = doc.snapshot();
    let old_clone = old.clone();
    assert_eq!(budget.usage().source_owners, 1);
    let current = apply(&mut doc, 2, &[], &budget).unwrap();
    assert_eq!(budget.usage().source_owners, 2);
    assert!(matches!(
        apply(&mut doc, 3, &[], &budget).err(),
        Some(CoreError::ResourceLimit {
            kind: LimitKind::SourceOwners,
            ..
        })
    ));
    drop(old);
    assert_eq!(budget.usage().source_owners, 2);
    drop(old_clone);
    assert_eq!(budget.usage().source_owners, 1);
    drop(current);
    apply(&mut doc, 3, &[], &budget).unwrap();
    drop(doc);
    assert_released(&budget);
}

#[test]
fn simultaneous_source_capacity_rejection_retires_private_owner() {
    let budget = SourceBudget::new(SourceLimits {
        retained_source_capacity: 7,
        ..SourceLimits::default()
    })
    .unwrap();
    let mut doc = document(b"data", &budget);
    let usage = budget.usage();
    assert!(matches!(
        apply(&mut doc, 2, &[], &budget).err(),
        Some(CoreError::ResourceLimit {
            kind: LimitKind::SourceCapacity,
            ..
        })
    ));
    assert_eq!(budget.usage().source_capacity, usage.source_capacity);
    assert_eq!(budget.usage().source_owners, 1);
    assert_eq!(doc.snapshot().key(), key(1));
    drop(doc);
    assert_released(&budget);
}

#[test]
fn prepare_limits_and_allocated_fill_failure_release_all_private_auxiliary() {
    for limits in [
        SourceLimits {
            max_parse_bytes: 2,
            ..SourceLimits::default()
        },
        SourceLimits {
            max_input_pieces: 0,
            ..SourceLimits::default()
        },
        SourceLimits {
            auxiliary_capacity: size_of::<InputPiece>() as u64,
            ..SourceLimits::default()
        },
    ] {
        let budget = SourceBudget::new(limits).unwrap();
        let source = document(b"\0", &budget).snapshot();
        assert!(matches!(
            ParseInput::prepare(source.clone(), &mut operation(&budget)).err(),
            Some(CoreError::ResourceLimit { .. })
        ));
        assert_eq!(budget.usage().auxiliary_capacity, 0);
        assert_eq!(budget.usage().source_owners, 1);
        drop(source);
        assert_released(&budget);
    }
    let budget = SourceBudget::new(SourceLimits {
        max_work: 30,
        ..SourceLimits::default()
    })
    .unwrap();
    let source = document("中\r\n\0😀\rZ".as_bytes(), &budget).snapshot();
    let mut op = operation(&budget);
    assert!(matches!(
        ParseInput::prepare(source.clone(), &mut op).err(),
        Some(CoreError::ResourceLimit {
            kind: LimitKind::Work,
            ..
        })
    ));
    assert!(budget.usage().auxiliary_capacity_peak > 0); // failure after private allocation
    assert_eq!(budget.usage().auxiliary_capacity, 0);
    drop(source);
    assert_released(&budget);
}

#[test]
fn budget_roots_cannot_be_replaced_to_escape_old_pin_charges() {
    let budget = SourceBudget::source_fixture();
    let other = SourceBudget::source_fixture();
    let mut doc = document(b"original", &budget);
    let source = doc.snapshot();
    assert_eq!(
        ParseInput::prepare(source.clone(), &mut operation(&other)).err(),
        Some(CoreError::WrongBudget)
    );
    assert_eq!(
        apply(&mut doc, 2, &[], &other).err(),
        Some(CoreError::WrongBudget)
    );
    assert_eq!(doc.snapshot().key(), key(1));
    assert_released(&other);
    let same_root = budget.clone();
    let prepared = ParseInput::prepare(source, &mut operation(&same_root)).unwrap();
    drop(doc);
    assert_eq!(budget.usage().source_owners, 1);
    drop(prepared);
    assert_released(&budget);
}

#[test]
fn one_chain_keeps_first_fault_under_later_cancel_and_cannot_restart_work() {
    let budget = SourceBudget::new(SourceLimits {
        max_work: 30,
        ..SourceLimits::default()
    })
    .unwrap();
    let token = CancellationToken::new();
    let mut op = budget.begin(token.clone());
    let doc = SourceDocument::new(key(1), "中\r\n\0😀\rZ".as_bytes(), &mut op).unwrap();
    assert!(matches!(
        ParseInput::prepare(doc.snapshot(), &mut op).err(),
        Some(CoreError::ResourceLimit {
            kind: LimitKind::Work,
            ..
        })
    ));
    let first = op.first_fault().unwrap();
    token.cancel();
    assert_eq!(op.checkpoint(), Err(first));
    assert_eq!(
        SourceDocument::new(key(2), b"new", &mut op).err(),
        Some(first)
    );
    assert_eq!(doc.snapshot().key(), key(1));
}

#[test]
fn cross_thread_token_clone_cancels_an_active_bounded_validator_and_cleans_up() {
    // Deterministic same-flag correctness control only, not a natural-stage latency sample.
    let budget = SourceBudget::source_fixture();
    let token = CancellationToken::new();
    let bytes = vec![b'x'; 3 * CHECKPOINT_BYTES as usize];
    let (started_tx, started_rx) = std::sync::mpsc::channel();
    let (cancelled_tx, cancelled_rx) = std::sync::mpsc::channel();
    std::thread::scope(|scope| {
        let worker_token = token.clone();
        let worker_budget = budget.clone();
        let worker_bytes = &bytes;
        let worker = scope.spawn(move || {
            let mut op = worker_budget.begin(worker_token);
            let mut text = ChargedString::source(worker_bytes.len(), &mut op).unwrap();
            let mut first_chunk = true;
            let result = utf8::visit(worker_bytes, None, &mut op, |chunk, op| {
                text.append(chunk, op)?;
                if first_chunk {
                    first_chunk = false;
                    started_tx.send(()).unwrap();
                    cancelled_rx.recv().unwrap();
                }
                Ok(())
            });
            assert!(op.stats().expect("statistics enabled").copied_bytes() > 0);
            result
        });
        started_rx.recv().unwrap();
        token.cancel();
        cancelled_tx.send(()).unwrap();
        assert_eq!(worker.join().unwrap(), Err(CoreError::Cancelled));
    });
    assert_released(&budget);
}

#[test]
fn immutable_source_and_parse_input_are_send_sync_and_piece_size_is_bounded() {
    fn send_sync<T: Send + Sync>() {}
    send_sync::<SourceSnapshot>();
    send_sync::<ParseInput>();
    assert!(size_of::<InputPiece>() <= 40);
    let budget = SourceBudget::source_fixture();
    let source = document("中文😀".as_bytes(), &budget).snapshot();
    let prepared = ParseInput::prepare(source.clone(), &mut operation(&budget)).unwrap();
    std::thread::scope(|scope| {
        scope
            .spawn(|| assert_eq!(source.text(), "中文😀"))
            .join()
            .unwrap();
        scope
            .spawn(|| assert_eq!(prepared.text(), "中文😀"))
            .join()
            .unwrap();
    });
}
