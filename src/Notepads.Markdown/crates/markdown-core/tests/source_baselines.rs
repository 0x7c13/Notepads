// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE file in the project root for license information.
// ---------------------------------------------------------------------------------------------

//! Source-only replay; this grants no Markdown grammar/model/mapping coverage.

use markdown_core::error::CoreError;
use markdown_core::identity::SourceKey;
use markdown_core::policy::{CancellationToken, SourceBudget};
use markdown_core::source::{ParseInput, SourceDocument};
use std::path::Path;

fn check_source(bytes: &[u8]) {
    let budget = SourceBudget::source_fixture();
    let mut op = budget.begin(CancellationToken::new());
    if let Err(error) = std::str::from_utf8(bytes) {
        assert!(matches!(
            SourceDocument::new(SourceKey::new([7; 16], 1).unwrap(), bytes, &mut op),
            Err(CoreError::InvalidUtf8 { offset }) if offset == error.valid_up_to() as u64
        ));
        assert_eq!(budget.usage().source_capacity, 0);
        assert_eq!(budget.usage().source_owners, 0);
        return;
    }
    let document = SourceDocument::new(SourceKey::new([7; 16], 1).unwrap(), bytes, &mut op)
        .expect("ordinary baseline source must be admitted");
    let snapshot = document.snapshot();
    let prepared = ParseInput::prepare(snapshot, &mut op).unwrap();
    assert_eq!(prepared.source().text().as_bytes(), bytes);
    // Independent three-rule oracle in the harness; its allocations are not engine gauges.
    let original = std::str::from_utf8(bytes).unwrap();
    let expected = original
        .replace("\r\n", "\n")
        .replace('\r', "\n")
        .replace('\0', "\u{fffd}");
    assert_eq!(prepared.text(), expected);
    if !bytes.contains(&b'\r') && !bytes.contains(&0) {
        assert!(prepared.is_identity());
        assert!(std::ptr::eq(
            prepared.text().as_ptr(),
            prepared.source().text().as_ptr()
        ));
    }
    let mut source_end = 0;
    let mut parse_end = 0;
    for piece in prepared.pieces() {
        assert_eq!(piece.source.start(), source_end);
        assert_eq!(piece.parse.start(), parse_end);
        source_end = piece.source.end();
        parse_end = piece.parse.end();
        assert!(original.is_char_boundary(source_end as usize));
        assert!(prepared.text().is_char_boundary(parse_end as usize));
    }
    assert_eq!(source_end, bytes.len() as u64);
    assert_eq!(parse_end, expected.len() as u64);
    assert!(op.stats().expect("statistics enabled").total() <= budget.limits().max_work);
    drop(prepared);
    drop(document);
    assert_eq!(budget.usage().source_capacity, 0);
    assert_eq!(budget.usage().auxiliary_capacity, 0);
    assert_eq!(budget.usage().source_owners, 0);
}

fn fixtures() -> &'static Path {
    Path::new(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../tests/fixtures/m1"
    ))
}

#[test]
fn required_913_sources_are_admitted_and_normalized() {
    let mut count = 0;
    for line in include_str!("source-baseline-index.tsv").lines() {
        if line.starts_with('#') {
            continue;
        }
        let fields: Vec<_> = line.split('\t').collect();
        if fields[0] != "required" {
            continue;
        }
        let bytes = std::fs::read(fixtures().join(fields[2])).unwrap();
        assert_eq!(
            bytes.len(),
            fields[3].parse::<usize>().unwrap(),
            "{}",
            fields[1]
        );
        assert_eq!(std::str::from_utf8(&bytes).is_ok(), fields[5] == "true");
        check_source(&bytes);
        count += 1;
    }
    assert_eq!(count, 913);
}

#[test]
fn five_large_source_shapes_and_ordinary_windows_line_endings_fit() {
    let mut count = 0;
    for line in include_str!("source-baseline-index.tsv").lines() {
        if line.starts_with('#') {
            continue;
        }
        let fields: Vec<_> = line.split('\t').collect();
        if fields[0] != "workload" {
            continue;
        }
        let bytes = std::fs::read(fixtures().join(fields[2])).unwrap();
        assert_eq!(bytes.len(), fields[3].parse::<usize>().unwrap());
        check_source(&bytes);
        if bytes.contains(&b'\n') {
            let original = std::str::from_utf8(&bytes).unwrap();
            // These are source-layer test variants, never replacements for the frozen baseline.
            for newline in ["\r\n", "\r"] {
                check_source(original.replace('\n', newline).as_bytes());
            }
        }
        count += 1;
    }
    assert_eq!(count, 5);
}
