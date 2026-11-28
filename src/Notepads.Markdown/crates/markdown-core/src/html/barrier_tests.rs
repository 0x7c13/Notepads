// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE file in the project root for license information.
// ---------------------------------------------------------------------------------------------

use super::tests::{parse, preview};
use crate::identity::{LogicalBoundary, SourceBoundary};
use crate::ir::{NodeKind, TextTarget};
use crate::mapping::{Affinity, Correspondence, PointInput, PointQuery, Position};
use crate::policy::CancellationToken;

#[test]
fn empty_html_barriers_hide_source_and_preserve_separate_blocks() {
    for barrier in [
        "<!-- note -->",
        "<p></p>",
        "<div> </div>",
        "<p align='center'> </p>",
    ] {
        for (before, after, lists, code, expected) in [
            ("- first", "- second", 2, 0, "first\nsecond"),
            ("1. first", "2. second", 2, 0, "first\nsecond"),
            ("    first", "    second", 0, 2, "first\n\nsecond\n"),
            ("- first", "    second", 1, 1, "first\nsecond\n"),
            ("    first", "- second", 1, 1, "first\n\nsecond"),
        ] {
            let source = format!("{before}\n\n{barrier}\n\n{after}");
            let model = parse(&source, true);
            let nodes = model.ir().nodes();
            assert_eq!(preview(&model), expected, "{source:?}");
            assert_eq!(model.original_source().text(), source);
            assert_eq!(
                nodes.iter().filter(|n| n.kind == NodeKind::List).count(),
                lists,
                "{source:?}"
            );
            assert_eq!(
                nodes
                    .iter()
                    .filter(|n| n.kind == NodeKind::CodeBlock)
                    .count(),
                code,
                "{source:?}"
            );
            let root = model.ir().flows()[model.main_flow_id().index()].root;
            assert!(
                nodes
                    .iter()
                    .filter(|n| n.kind == NodeKind::CodeBlock)
                    .all(|n| n.parent == Some(root)),
                "{source:?}"
            );
            assert!(
                !nodes
                    .iter()
                    .any(|n| matches!(n.kind, NodeKind::HtmlBlock | NodeKind::ThematicBreak)),
                "{source:?}"
            );
        }
    }
}

#[test]
fn consecutive_empty_html_barriers_stay_invisible() {
    for endings in ["\n", "\r\n", "\r"] {
        let source =
            "- first\n\n<!-- one -->\n\n<p></p>\n\n<!-- two -->\n\n- second".replace('\n', endings);
        let model = parse(&source, true);
        assert_eq!(preview(&model), "first\nsecond");
        assert_eq!(
            model
                .ir()
                .nodes()
                .iter()
                .filter(|n| n.kind == NodeKind::List)
                .count(),
            2
        );
    }
    assert_eq!(preview(&parse("<!-- one -->\n\n<p></p>", true)), "");
}

#[test]
fn hidden_html_barriers_keep_neighboring_unicode_source_points_exact() {
    let source = "- Hé😀\r\n\r\n<!-- hidden -->\r\n\r\n- 中é tail";
    let model = parse(source, true);
    let text = preview(&model);
    assert_eq!(text, "Hé😀\n中é tail");
    for needle in ["é", "😀", "ail"] {
        let byte = source.find(needle).unwrap() as u64;
        let logical = text[..text.find(needle).unwrap()].encode_utf16().count() as u64;
        for affinity in [Affinity::Before, Affinity::After] {
            let mut op = model.budget().begin(CancellationToken::new());
            let response = model
                .map_point(
                    &PointQuery {
                        model: model.identity(),
                        input: PointInput::Source {
                            target: TextTarget::Flow(model.main_flow_id()),
                            byte: SourceBoundary::new(byte),
                        },
                        affinity,
                    },
                    &mut op,
                )
                .unwrap();
            assert_eq!(
                response.results()[0].chosen,
                Position::Flow(LogicalBoundary::new(logical)),
                "{needle:?}"
            );
            assert_eq!(response.results()[0].correspondence, Correspondence::Exact);
            let response = model
                .map_point(
                    &PointQuery {
                        model: model.identity(),
                        input: PointInput::Flow {
                            flow: model.main_flow_id(),
                            utf16: LogicalBoundary::new(logical),
                        },
                        affinity,
                    },
                    &mut op,
                )
                .unwrap();
            assert_eq!(
                response.results()[0].chosen,
                Position::Source(SourceBoundary::new(byte))
            );
            assert_eq!(response.results()[0].correspondence, Correspondence::Exact);
        }
    }
    for affinity in [Affinity::Before, Affinity::After] {
        let mut op = model.budget().begin(CancellationToken::new());
        let result = model
            .map_point(
                &PointQuery {
                    model: model.identity(),
                    input: PointInput::Source {
                        target: TextTarget::Flow(model.main_flow_id()),
                        byte: SourceBoundary::new(source.find("hidden").unwrap() as u64 + 1),
                    },
                    affinity,
                },
                &mut op,
            )
            .unwrap();
        assert_eq!(result.results()[0].correspondence, Correspondence::Hidden);
        assert!(!result.results()[0].effects.atomic);
    }
}

#[test]
fn invisible_barriers_cannot_resolve_authored_reference_links() {
    for prefix in [
        "[0-0-0] [0-0-1] [0-0-2]",
        "[ 0-0-0 ] [0-0-01] [0-0-999999999999999999999999999999999999999999]",
        "[ſ] [K] [ﬀ]",
    ] {
        let source = format!("{prefix}\n\n- first\n\n<!-- hidden -->\n\n- second");
        let model = parse(&source, true);
        assert_eq!(preview(&model), format!("{prefix}\nfirst\nsecond"));
        assert!(!model.ir().nodes().iter().any(|n| n.kind == NodeKind::Link));
    }
    let source = "[0-0-0]: /authored\n\n[0-0-0]\n\n- first\n\n<!-- hidden -->\n\n- second";
    let model = parse(source, true);
    assert_eq!(preview(&model), "0-0-0\nfirst\nsecond");
    assert_eq!(
        model
            .ir()
            .nodes()
            .iter()
            .filter(|n| n.kind == NodeKind::Link)
            .count(),
        1
    );
}

#[test]
fn disabled_or_unsupported_html_keeps_literal_fallback() {
    let source = "- first\n\n<!-- hidden -->\n\n- second";
    assert!(preview(&parse(source, false)).contains("<!-- hidden -->"));
    let source = "- first\n\n<iframe></iframe>\n\n- second";
    assert!(preview(&parse(source, true)).contains("<iframe></iframe>"));
}

#[test]
fn barrier_label_scan_crosses_windows_and_large_documents() {
    let prefix = format!(
        "{}0-0-0 [0-0-1] {} [0-0-2]",
        "x".repeat(16_382),
        "y".repeat(50_000)
    );
    let source = format!("{prefix}\n\n- first\n\n<!-- hidden -->\n\n- second");
    let model = parse(&source, true);
    assert_eq!(preview(&model), format!("{prefix}\nfirst\nsecond"));
    assert!(!model.ir().nodes().iter().any(|n| n.kind == NodeKind::Link));
    assert_eq!(model.original_source().text(), source);
}

#[test]
fn hidden_barrier_ranges_have_no_visible_or_atomic_contribution() {
    use crate::identity::SourceSpan;
    use crate::mapping::{RangeInput, RangeQuery};
    let source = "- first\n\n<!-- hidden -->\n\n- second";
    let model = parse(source, true);
    let start = source.find("hidden").unwrap() as u64;
    let mut op = model.budget().begin(CancellationToken::new());
    let response = model
        .map_range(
            &RangeQuery {
                model: model.identity(),
                input: RangeInput::Source {
                    target: TextTarget::Flow(model.main_flow_id()),
                    span: SourceSpan::new(start, start + 6).unwrap(),
                },
                empty_affinity: Affinity::After,
            },
            &mut op,
        )
        .unwrap();
    assert!(response.selections().is_empty());
    assert!(response.contributions().is_empty());
}

#[test]
fn barrier_admission_and_cancellation_do_not_publish_partial_projection() {
    use crate::config::{Extensions, ParseConfig, Profile};
    use crate::error::CoreError;
    use crate::identity::SourceKey;
    use crate::model::AcceptedModel;
    use crate::policy::{ModelLimits, SourceBudget, SourceLimits};
    use crate::source::SourceDocument;
    let source = "- first\n\n<!-- -->\n\n- second";
    let limits = SourceLimits {
        max_source_bytes: source.len() as u64,
        ..SourceLimits::default()
    };
    let budget = SourceBudget::new_with_model_limits(limits, ModelLimits::default()).unwrap();
    let mut op = budget.begin(CancellationToken::new());
    let doc = SourceDocument::new(
        SourceKey::new([8; 16], 1).unwrap(),
        source.as_bytes(),
        &mut op,
    )
    .unwrap();
    let model = AcceptedModel::parse(
        doc.snapshot(),
        ParseConfig::with_extensions(Profile::G3, Extensions::ALL),
        1,
        &mut op,
    )
    .unwrap();
    assert_eq!(model.original_source().text(), source);
    assert!(
        preview(&model).contains("<!-- -->"),
        "Source admission must retain the existing inert fallback"
    );

    let budget = SourceBudget::new(SourceLimits {
        auxiliary_capacity: 0,
        ..SourceLimits::default()
    })
    .unwrap();
    let mut op = budget.begin(CancellationToken::new());
    assert!(super::empty_barrier(source, &mut op).unwrap().is_none());
    let budget = SourceBudget::preview();
    let cancellation = CancellationToken::new();
    cancellation.cancel();
    let mut op = budget.begin(cancellation);
    assert!(matches!(
        super::empty_barrier(source, &mut op),
        Err(CoreError::Cancelled)
    ));
}
