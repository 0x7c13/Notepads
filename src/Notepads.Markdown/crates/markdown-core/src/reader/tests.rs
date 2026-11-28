// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE file in the project root for license information.
// ---------------------------------------------------------------------------------------------

use super::*;
use crate::config::{ParseConfig, Profile};
use crate::identity::SourceKey;
use crate::policy::{CancellationToken, SourceBudget};
use crate::source::SourceDocument;

fn actual(text: &str, root: &SourceBudget) -> AcceptedModel {
    let mut op = root.begin(CancellationToken::new());
    let document = SourceDocument::new(
        SourceKey::new([8; 16], 1).unwrap(),
        text.as_bytes(),
        &mut op,
    )
    .unwrap();
    AcceptedModel::parse(
        document.snapshot(),
        ParseConfig::new(Profile::C0),
        1,
        &mut op,
    )
    .unwrap()
}

#[test]
fn actual_main_keeps_scalar_boundaries_objects_and_alts_separate() {
    for (source, expected) in [
        ("**中&amp;😀**", "中&😀"),
        ("a\r\nb  \r\nc", "a b\nc"),
        ("![alt *中*](img)尾", "\u{fffc}尾"),
        ("&NotEqualTilde;\\*", "\u{2242}\u{338}*"),
        ("[r]: /dest\n\n[r]", "r"),
        ("```\nx", "x\n"),
        ("[r]: /dest", ""),
    ] {
        let root = SourceBudget::model_fixture();
        let model = actual(source, &root);
        let mut reader =
            MainReader::new(model.clone(), root.begin(CancellationToken::new())).unwrap();
        drop(model);
        let mut buffer = [0u8; 4];
        let mut output = String::new();
        loop {
            let batch = reader.read_into(&mut buffer).unwrap();
            output.push_str(std::str::from_utf8(&buffer[..batch.written]).unwrap());
            assert_eq!(batch.utf16, output.encode_utf16().count() as u64);
            if batch.done {
                break;
            }
            assert!(batch.written != 0);
        }
        assert_eq!(output, expected, "{source:?}");
        assert_eq!(
            reader.read_into(&mut buffer).unwrap(),
            ReadResult {
                written: 0,
                utf16: expected.encode_utf16().count() as u64,
                done: true
            }
        );
        drop(reader);
        assert_eq!(root.usage().source_capacity, 0);
        assert_eq!(root.usage().auxiliary_capacity, 0);
        assert_eq!(root.usage().source_owners, 0);
    }
}

#[test]
fn reader_continues_without_full_prefix_scan() {
    let root = SourceBudget::model_fixture();
    let source = "a😀中".repeat(20_000);
    let model = actual(&source, &root);
    assert_eq!(model.ir().flows()[0].atoms.len(), 1);
    let mut reader = MainReader::new(model, root.begin(CancellationToken::new())).unwrap();
    let mut buffer = [0; 65536];
    let mut total = 0;
    let mut batches = 0;
    loop {
        let batch = reader.read_into(&mut buffer).unwrap();
        batches += 1;
        assert_eq!(
            &buffer[..batch.written],
            &source.as_bytes()[total..total + batch.written]
        );
        total += batch.written;
        if batch.done {
            break;
        }
    }
    assert_eq!(total, source.len());
    // One immutable atom view per output batch, with no scalar-prefix reread.
    let views = batches;
    let read_fee = (size_of::<crate::ir::AtomFields>()
        + size_of::<std::mem::Discriminant<crate::ir::AuthorityBinding>>())
        as u64;
    assert!(
        reader
            .op
            .stats()
            .expect("statistics enabled")
            .scanned_bytes()
            <= source.len() as u64 + 6 * batches + views * read_fee
    );
    assert_eq!(
        reader
            .op
            .stats()
            .expect("statistics enabled")
            .copied_bytes(),
        source.len() as u64 + views * size_of::<crate::ir::AtomRow>() as u64
    );
}

#[test]
fn bounded_windows_preserve_every_utf8_tail_and_atom_transition() {
    let root = SourceBudget::preview();
    let source = format!(
        "{}**{}**&amp;![alt](image){}",
        "a😀中".repeat(150),
        "𐐀éz".repeat(80),
        "中😀x".repeat(150)
    );
    let expected = format!(
        "{}{}&\u{fffc}{}",
        "a😀中".repeat(150),
        "𐐀éz".repeat(80),
        "中😀x".repeat(150)
    );
    let model = actual(&source, &root);
    for size in [4, 5, 7, 255, 256, 257, 1023, 65536] {
        let mut reader =
            MainReader::new(model.clone(), root.begin(CancellationToken::new())).unwrap();
        let mut buffer = vec![0; size];
        let mut output = String::new();
        loop {
            let batch = reader.read_into(&mut buffer).unwrap();
            output.push_str(std::str::from_utf8(&buffer[..batch.written]).unwrap());
            assert_eq!(batch.utf16, output.encode_utf16().count() as u64);
            if batch.done {
                break;
            }
            assert_ne!(batch.written, 0);
        }
        assert_eq!(output, expected, "buffer {size}");
    }
}

#[test]
fn bounded_counting_deadline_rejects_uncommitted_batch() {
    let root = SourceBudget::task_fixture();
    let model = actual(&"中😀x".repeat(1024), &root);
    let mut reader = MainReader::new(model, root.begin(CancellationToken::new())).unwrap();
    reader.op.set_clock_samples_for_test(&[500_000_000]);
    assert!(matches!(
        reader.read_into(&mut [0; 65536]),
        Err(CoreError::DeadlineExceeded { .. })
    ));
    assert_eq!((reader.atom, reader.byte, reader.utf16), (0, 0, 0));
    let fault = reader.op.first_fault();
    assert_eq!(reader.read_into(&mut [0; 65536]).err(), fault);
}

#[test]
fn cancel_and_failed_partial_batch_do_not_commit_cursor_or_reset_work() {
    let root = SourceBudget::model_fixture();
    let model = actual("plain", &root);
    let cancel = CancellationToken::new();
    let mut reader = MainReader::new(model.clone(), root.begin(cancel.clone())).unwrap();
    reader.read_into(&mut [0; 4]).unwrap();
    let before = (
        reader.atom,
        reader.byte,
        reader.utf16,
        reader.op.stats().expect("statistics enabled"),
    );
    cancel.cancel();
    assert_eq!(
        reader.read_into(&mut [0; 4]).err(),
        Some(CoreError::Cancelled)
    );
    assert_eq!(
        (
            reader.atom,
            reader.byte,
            reader.utf16,
            reader.op.stats().expect("statistics enabled")
        ),
        before
    );
    let mut op = root.begin(CancellationToken::new());
    let stop = root.limits().max_work - 10;
    while op.stats().expect("statistics enabled").total() < stop {
        op.external_read((stop - op.stats().expect("statistics enabled").total()).min(65_536))
            .unwrap();
    }
    let mut reader = MainReader::new(model, op).unwrap();
    assert!(matches!(
        reader.read_into(&mut [0; 4]),
        Err(CoreError::ResourceLimit { .. })
    ));
    assert_eq!((reader.atom, reader.byte, reader.utf16), (0, 0, 0));
    let first = reader.op.first_fault();
    assert_eq!(reader.read_into(&mut [0; 4]).err(), first);
}

#[test]
fn reader_wrong_root_and_bounds_fail_before_exposing_text() {
    let root = SourceBudget::model_fixture();
    let model = actual("😀", &root);
    assert!(matches!(
        MainReader::new(
            model.clone(),
            SourceBudget::model_fixture().begin(CancellationToken::new())
        ),
        Err(CoreError::WrongBudget)
    ));
    assert_eq!(model.flow_id(0), Err(CoreError::UnknownId));
    assert_eq!(model.flow_id(2), Err(CoreError::UnknownId));
    assert_eq!(model.payload_id(1), Err(CoreError::UnknownId));
    let mut reader = MainReader::new(model, root.begin(CancellationToken::new())).unwrap();
    let mut too_small = [99; 3];
    assert_eq!(
        reader.read_into(&mut too_small).err(),
        Some(CoreError::InvalidRange)
    );
    assert_eq!(too_small, [99; 3]);
    assert_eq!(
        reader.read_into(&mut [0; 4]).err(),
        Some(CoreError::InvalidRange)
    );
}

#[test]
fn boundary_result_write_is_admitted_before_cursor_commit() {
    let root = SourceBudget::model_fixture();
    let model = actual("x", &root);
    let mut reader = MainReader::new(model.clone(), root.begin(CancellationToken::new())).unwrap();
    let before = reader
        .op
        .stats()
        .expect("statistics enabled")
        .copied_bytes();
    reader.boundary_result(32).unwrap();
    assert_eq!(
        reader
            .op
            .stats()
            .expect("statistics enabled")
            .copied_bytes()
            - before,
        32
    );
    assert_eq!(reader.read_into(&mut [0; 4]).unwrap().written, 1);
    reader.boundary_result(32).unwrap();
    assert!(reader.read_into(&mut [0; 4]).unwrap().done);
    let mut op = root.begin(CancellationToken::new());
    let stop = root.limits().max_work - 31;
    while op.stats().expect("statistics enabled").total() < stop {
        op.external_read((stop - op.stats().expect("statistics enabled").total()).min(65_536))
            .unwrap();
    }
    let mut reader = MainReader::new(model, op).unwrap();
    assert!(matches!(
        reader.boundary_result(32),
        Err(CoreError::ResourceLimit { .. })
    ));
    assert_eq!((reader.atom, reader.byte, reader.utf16), (0, 0, 0));
    let first = reader.op.first_fault();
    assert_eq!(reader.read_into(&mut [0; 4]).err(), first);
}

#[test]
fn accepted_multisegment_pool_keeps_real_main_bytes_across_reader_batches() {
    let root = SourceBudget::model_fixture();
    // Thousands of actual entity values force the packed final pool across
    // allocations; plain inline-code bodies may remain source-backed Copies.
    let source = "&amp;".repeat(5000);
    let expected = "&".repeat(5000);
    let model = actual(&source, &root);
    let mut segments = 0;
    model.ir().text.for_each_segment(|_| segments += 1);
    assert!(segments >= 2);
    let mut reader = MainReader::new(model, root.begin(CancellationToken::new())).unwrap();
    let mut buffer = [0; 4096];
    let mut output = Vec::new();
    loop {
        let batch = reader.read_into(&mut buffer).unwrap();
        output.extend_from_slice(&buffer[..batch.written]);
        if batch.done {
            break;
        }
    }
    assert_eq!(output, expected.as_bytes());
    drop(reader);
    assert_eq!(root.usage().source_capacity, 0);
    assert_eq!(root.usage().auxiliary_capacity, 0);
    assert_eq!(root.usage().source_owners, 0);
}
