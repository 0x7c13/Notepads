// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE file in the project root for license information.
// ---------------------------------------------------------------------------------------------

use super::*;
use crate::config::{Extensions, ParseConfig, Profile};
use crate::identity::{LogicalBoundary, SourceKey};
use crate::mapping::Correspondence;
use crate::policy::{CancellationToken, ModelLimits, SourceBudget, SourceLimits};
use crate::reader::MainReader;
use crate::source::SourceDocument;

fn budget() -> SourceBudget {
    SourceBudget::new_with_model_limits(SourceLimits::default(), ModelLimits::default()).unwrap()
}

fn builder(source: &str) -> ProjectionBuilder<'_> {
    ProjectionBuilder {
        original: source,
        preview: String::with_capacity(source.len()),
        segments: Vec::with_capacity(1),
        cursor: 0,
    }
}

#[test]
fn projection_builder_meters_whole_source_copy() {
    let source = format!("{}😀{}", "a".repeat(65_535), "b".repeat(1_048_576));
    let budget = budget();
    let mut op = budget.begin(CancellationToken::new());
    let before = op.stats().unwrap();
    let mut builder = builder(&source);
    builder.copy_to(source.len(), &mut op).unwrap();
    let after = op.stats().unwrap();
    assert_eq!(builder.preview, source);
    assert_eq!(
        after.copied_bytes() - before.copied_bytes(),
        source.len() as u64
    );
    assert!(builder.segments[0].exact);
    assert_eq!(builder.segments[0].original, 0..source.len());
    assert_eq!(builder.segments[0].preview, 0..source.len());
}

#[test]
fn projection_builder_meters_nonidentical_address_exact_comparison() {
    let source = format!("{}中😀{}", "a".repeat(4095), "b".repeat(8192));
    let text = source.clone();
    assert_ne!(source.as_ptr(), text.as_ptr());
    let budget = budget();
    let mut op = budget.begin(CancellationToken::new());
    let mut builder = builder(&source);
    builder.emit(0..source.len(), &text, &mut op).unwrap();
    let stats = op.stats().unwrap();
    assert!(stats.scanned_bytes() >= 2 * source.len() as u64);
    assert_eq!(stats.copied_bytes(), text.len() as u64);
    assert_eq!(builder.preview, text);
    assert!(builder.segments[0].exact);
}

#[test]
fn projection_builder_rejects_cancelled_and_unfunded_copy_before_writes() {
    let source = "a".repeat(8192);
    let budget = budget();
    let flag = CancellationToken::new();
    flag.cancel();
    let mut op = budget.begin(flag);
    let mut cancelled = builder(&source);
    assert_eq!(
        cancelled.copy_to(source.len(), &mut op),
        Err(CoreError::Cancelled)
    );
    assert!(cancelled.preview.is_empty());

    let budget = SourceBudget::new_with_model_limits(
        SourceLimits {
            max_work: 256,
            ..SourceLimits::default()
        },
        ModelLimits::default(),
    )
    .unwrap();
    let mut op = budget.begin(CancellationToken::new());
    let mut denied = builder(&source);
    let error = denied.copy_to(source.len(), &mut op).unwrap_err();
    assert!(matches!(
        error,
        CoreError::ResourceLimit {
            kind: crate::error::LimitKind::Work,
            ..
        }
    ));
    assert_eq!(op.first_fault(), Some(error));
    assert!(denied.preview.is_empty());
}

#[test]
fn projection_builder_observes_mid_copy_and_completion_deadlines() {
    const LIMIT: u64 = 500_000_000;
    for length in [7, 131_072] {
        for budget in [SourceBudget::task_fixture()] {
            let source = "a".repeat(length);
            let mut op = budget.begin(CancellationToken::new());
            op.set_clock_samples_for_test(&[LIMIT - 1, LIMIT]);
            let mut builder = builder(&source);
            let error = builder.copy_to(source.len(), &mut op).unwrap_err();
            assert_eq!(
                error,
                CoreError::DeadlineExceeded {
                    elapsed_ns: LIMIT,
                    limit_ns: LIMIT
                }
            );
            assert_eq!(op.first_fault(), Some(error));
            if length > 65_536 {
                assert!(builder.preview.len() <= 65_536);
            }
        }
    }
}

#[test]
fn projected_parse_preserves_long_unicode_prefix_tail_and_original_mapping() {
    let prefix = format!("{}😀", "a".repeat(4095));
    let tail = format!("{}中😀", "z".repeat(8191));
    let source = format!("{prefix}\n\n<p>A &amp; 中😀</p>\n\n{tail}");
    let budget = budget();
    let source_document = SourceDocument::new(
        SourceKey::new([0x73; 16], 1).unwrap(),
        source.as_bytes(),
        &mut budget.begin(CancellationToken::new()),
    )
    .unwrap();
    let model = AcceptedModel::parse(
        source_document.snapshot(),
        ParseConfig::with_extensions(Profile::C0, Extensions::HTML_BLOCKS),
        1,
        &mut budget.begin(CancellationToken::new()),
    )
    .unwrap();
    assert_eq!(model.original_source().text(), source);
    let mut reader =
        MainReader::new(model.clone(), budget.begin(CancellationToken::new())).unwrap();
    let mut output = Vec::new();
    let mut buffer = [0; 4096];
    loop {
        let result = reader.read_into(&mut buffer).unwrap();
        output.extend_from_slice(&buffer[..result.written]);
        if result.done {
            break;
        }
    }
    let output = String::from_utf8(output).unwrap();
    assert!(output.starts_with(&prefix));
    assert!(output.contains("A & 中😀"));
    assert!(output.ends_with(&tail));
    let byte = source.find(&tail).unwrap() as u64 + 4096;
    for affinity in [Affinity::Before, Affinity::After] {
        let point = model
            .map_point(
                &PointQuery {
                    model: model.identity(),
                    input: PointInput::Source {
                        target: TextTarget::Flow(model.main_flow_id()),
                        byte: SourceBoundary::new(byte),
                    },
                    affinity,
                },
                &mut budget.begin(CancellationToken::new()),
            )
            .unwrap();
        assert_eq!(point.results()[0].correspondence, Correspondence::Exact);
        let Position::Flow(at) = point.results()[0].chosen else {
            panic!("expected flow")
        };
        let reverse = model
            .map_point(
                &PointQuery {
                    model: model.identity(),
                    input: PointInput::Flow {
                        flow: model.main_flow_id(),
                        utf16: LogicalBoundary::new(at.get()),
                    },
                    affinity,
                },
                &mut budget.begin(CancellationToken::new()),
            )
            .unwrap();
        assert_eq!(
            reverse.results()[0].chosen,
            Position::Source(SourceBoundary::new(byte))
        );
    }
}
