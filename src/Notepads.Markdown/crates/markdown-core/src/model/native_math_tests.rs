// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE file in the project root for license information.
// ---------------------------------------------------------------------------------------------

use super::*;
use crate::config::{Extensions, Profile};
use crate::identity::SourceKey;
use crate::ir::{DerivedKind, MathSyntax, NodeKind, ObjectRow};
use crate::policy::CancellationToken;
use crate::reader::MainReader;
use crate::source::SourceDocument;

fn parse(budget: &SourceBudget, input: &str, extensions: Extensions) -> AcceptedModel {
    let mut op = budget.begin(CancellationToken::new());
    let source = SourceDocument::new(
        SourceKey::new([134; 16], 1).unwrap(),
        input.as_bytes(),
        &mut op,
    )
    .unwrap();
    let mut progress = ParseProgress::default();
    AcceptedModel::experimental_parse_with_progress(
        source.snapshot(),
        ParseConfig::with_extensions(Profile::G3, extensions),
        1,
        &mut op,
        &mut progress,
    )
    .unwrap_or_else(|error| panic!("{input:?}: {error:?}; {progress:?}"))
}

fn main(model: &AcceptedModel) -> String {
    let mut reader = MainReader::new(
        model.clone(),
        model.budget().begin(CancellationToken::new()),
    )
    .unwrap();
    let mut result = String::new();
    let mut bytes = [0; 7];
    loop {
        let read = reader.read_into(&mut bytes).unwrap();
        result.push_str(std::str::from_utf8(&bytes[..read.written]).unwrap());
        assert_eq!(read.utf16, result.encode_utf16().count() as u64);
        if read.done {
            return result;
        }
    }
}

#[test]
fn native_math_fences_have_display_geometry_and_exact_authored_export() {
    use crate::layout::{BlockKind, LayoutDraft, LayoutKey, LayoutStyle};
    use crate::presentation::PresentationBatch;

    for (source, expected) in [
        ("```math\r\n\\frac{a}{b}\r\n```", "\\frac{a}{b}\n"),
        ("~~~math\na^2+b^2\n~~~", "a^2+b^2\n"),
    ] {
        let budget = SourceBudget::preview();
        let model = parse(&budget, source, Extensions::MATH);
        assert_eq!(main(&model), "\u{fffc}");
        let mut op = budget.begin(CancellationToken::new());
        let key = LayoutKey::new(
            model.identity(),
            1,
            1,
            1,
            500.0,
            LayoutStyle::new(18.0, 6.0).unwrap(),
        )
        .unwrap();
        let mut draft = LayoutDraft::new(model.clone(), key, &mut op).unwrap();
        let batch = draft.measurement_batch(8, &mut op).unwrap().unwrap();
        assert_eq!(batch.requests.len(), 1);
        assert_eq!(batch.requests[0].kind, BlockKind::Paragraph);
        assert_eq!(
            model.ir().nodes()[batch.requests[0].node.index()].kind,
            NodeKind::MathBlock
        );
        let presentation = PresentationBatch::build(model, &batch, &mut op).unwrap();
        assert_eq!(presentation.derived().len(), 1);
        let math = presentation.derived()[0];
        assert_eq!(math.kind, DerivedKind::Math(MathSyntax::FencedDisplay));
        let value = |blob: crate::presentation::BlobSlice| {
            std::str::from_utf8(
                &presentation.blob()[blob.start as usize..(blob.start + blob.len) as usize],
            )
            .unwrap()
        };
        assert_eq!(value(math.input), expected);
        assert_eq!(value(math.export), source);
    }

    let budget = SourceBudget::preview();
    let disabled = parse(&budget, "```math\nx\n```", Extensions::NONE);
    assert!(disabled.ir().objects().is_empty());
    assert_eq!(main(&disabled), "x\n");
    let unknown = parse(&budget, "```mathematics\nx\n```", Extensions::MATH);
    assert!(unknown.ir().objects().is_empty());
    assert_eq!(main(&unknown), "x\n");
}

#[test]
fn native_math_delimiters_have_atomic_main_and_exact_raw_source() {
    // Inputs/literals follow the fixed Comrak 0.56.0 MIT math baseline;
    // expected Main and raw export are independently specified here.
    for (input, literal, syntax) in [
        ("$2+2$", "2+2", MathSyntax::DollarInline),
        ("$` x\r\n*y* `$", "x *y*", MathSyntax::CodeInline),
        ("$$   2+2  $$", "   2+2  ", MathSyntax::DollarDisplay),
        (
            "\\(\\alpha + \\beta\\)",
            "\\alpha + \\beta",
            MathSyntax::LatexInline,
        ),
        ("\\[\r\n2+2\r\n\\]", "\n2+2\n", MathSyntax::LatexDisplay),
    ] {
        let budget = SourceBudget::preview();
        {
            let model = parse(&budget, input, Extensions::MATH);
            assert_eq!(main(&model), "\u{fffc}");
            assert_eq!(model.ir().objects().len(), 1);
            let ObjectRow::Derived {
                kind,
                input: payload,
                export_sources,
                ..
            } = model.ir().objects()[0]
            else {
                panic!("expected owned math object")
            };
            assert_eq!(kind, DerivedKind::Math(syntax));
            assert_eq!(
                model
                    .ir()
                    .text(model.ir().payloads()[payload.index()].value)
                    .unwrap(),
                literal
            );
            let mut exported = String::new();
            for span in export_sources.slice(model.ir().sources()).unwrap() {
                exported
                    .push_str(&model.source().text()[span.start() as usize..span.end() as usize]);
            }
            assert_eq!(exported, input);
        }
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
}

#[test]
fn native_math_extension_is_independent_of_the_old_g3_dialect() {
    let budget = SourceBudget::preview();
    let old = parse(&budget, "A $x$ B `\\(y\\)`", Extensions::NONE);
    assert_eq!(main(&old), "A $x$ B \\(y\\)");
    assert!(old.ir().objects().is_empty());
    let extended = parse(&budget, "A $x$ B `\\(y\\)`", Extensions::MATH);
    assert_eq!(main(&extended), "A \u{fffc} B \\(y\\)");
    assert_eq!(extended.ir().objects().len(), 1);
    assert_ne!(old.identity().config(), extended.identity().config());
}

#[test]
fn native_math_display_latex_does_not_consume_markdown_bracket_escapes() {
    use crate::identity::{LogicalBoundary, SourceBoundary};
    use crate::ir::TextTarget;
    use crate::mapping::{Affinity, PointInput, PointQuery, Position};
    let budget = SourceBudget::preview();
    for (input, expected) in [
        (r"[转义 \[a\] 完整](#target)", "转义 [a] 完整"),
        (r"Text \[a\] tail", "Text [a] tail"),
        (r"\[a\] tail", "[a] tail"),
        (r"\[a\] and \[b\]", "[a] and [b]"),
        ("Text\n\\[a\\]", "Text [a]"),
        (r"# \[a\]", "[a]"),
        ("| cell |\n| --- |\n| \\[a\\] |", "cell\n[a]"),
    ] {
        let model = parse(&budget, input, Extensions::MATH);
        assert_eq!(main(&model), expected, "{input:?}");
        assert!(model.ir().objects().is_empty(), "{input:?}");
        if input.starts_with('[') {
            assert_eq!(
                model
                    .ir()
                    .nodes()
                    .iter()
                    .filter(|row| row.kind == NodeKind::Link)
                    .count(),
                1,
                "{input:?}",
            );
            assert_eq!(
                model
                    .ir()
                    .stages()
                    .iter()
                    .filter(|stage| stage.kind == crate::ir::StageKind::AsciiEscape)
                    .count(),
                2,
                "both visible brackets retain their escaped source ownership",
            );
            for (escape, before) in [(r"\[", 3), (r"\]", 5)] {
                let byte = SourceBoundary::new((input.find(escape).unwrap() + 1) as u64);
                for (affinity, logical) in
                    [(Affinity::Before, before), (Affinity::After, before + 1)]
                {
                    let result = model
                        .map_point(
                            &PointQuery {
                                model: model.identity(),
                                input: PointInput::Source {
                                    target: TextTarget::Flow(model.main_flow_id()),
                                    byte,
                                },
                                affinity,
                            },
                            &mut budget.begin(CancellationToken::new()),
                        )
                        .unwrap();
                    assert_eq!(
                        result.results()[0].chosen,
                        Position::Flow(LogicalBoundary::new(logical))
                    );
                }
            }
        }
    }
    let model = parse(&budget, r"![\[a\]](image.png)", Extensions::MATH);
    assert_eq!(model.ir().objects().len(), 1);
    let ObjectRow::Image { alt, .. } = model.ir().objects()[0] else {
        panic!("only the image should be an object");
    };
    assert_eq!(model.ir().flows()[alt.index()].logical_utf16, 3);
}

#[test]
fn native_math_display_latex_accepts_standalone_paragraphs_in_containers() {
    let budget = SourceBudget::preview();
    for input in [r"\[a\]", r"  \[a\]  ", "> \\[a\n> b\\]", r"- \[a\]"] {
        let model = parse(&budget, input, Extensions::MATH);
        assert_eq!(main(&model), "\u{fffc}", "{input:?}");
        assert_eq!(model.ir().objects().len(), 1);
        assert!(
            matches!(
                model.ir().objects()[0],
                ObjectRow::Derived {
                    kind: DerivedKind::Math(MathSyntax::LatexDisplay),
                    ..
                }
            ),
            "{input:?}"
        );
    }
}

#[test]
fn native_math_display_latex_preserves_github_link_ownership_in_headings() {
    let budget = SourceBudget::preview();
    let mut op = budget.begin(CancellationToken::new());
    let source = SourceDocument::new(
        SourceKey::new([134; 16], 1).unwrap(),
        br"# \[ text https://example.com/path tail \]",
        &mut op,
    )
    .unwrap();
    let model = AcceptedModel::parse(
        source.snapshot(),
        ParseConfig::with_extensions(Profile::G3, Extensions::MATH),
        1,
        &mut op,
    )
    .unwrap();
    assert_eq!(main(&model), "[ text https://example.com/path tail ]");
    assert!(model.ir().objects().is_empty());
    assert_eq!(
        model
            .ir()
            .nodes()
            .iter()
            .filter(|row| row.kind == NodeKind::Link)
            .count(),
        1,
    );
}

#[test]
fn native_math_transport_preserves_container_and_table_source_without_markdown_cleaning() {
    for (input, expected_main, expected_input, expected_export) in [
        ("$a\r\nb$", "\u{fffc}", "a b", "$a\r\nb$"),
        (
            "> $$\r\n> a+b\r\n> $$",
            "\u{fffc}",
            "\na+b\n",
            "$$\r\na+b\r\n$$",
        ),
        (
            "- value \\(x&amp;y\\)",
            "value \u{fffc}",
            "x&amp;y",
            "\\(x&amp;y\\)",
        ),
        (
            "| formula |\r\n| --- |\r\n| $a\\|b$ |",
            "formula\n\u{fffc}",
            "a|b",
            "$a\\|b$",
        ),
    ] {
        let budget = SourceBudget::preview();
        let model = parse(&budget, input, Extensions::MATH);
        assert_eq!(main(&model), expected_main, "{input:?}");
        let object = model
            .ir()
            .objects()
            .iter()
            .find(|row| matches!(row, ObjectRow::Derived { .. }))
            .unwrap();
        let ObjectRow::Derived {
            input: payload,
            export_sources,
            ..
        } = *object
        else {
            unreachable!()
        };
        assert_eq!(
            model
                .ir()
                .text(model.ir().payloads()[payload.index()].value)
                .unwrap(),
            expected_input
        );
        let exported: String = export_sources
            .slice(model.ir().sources())
            .unwrap()
            .iter()
            .map(|span| &model.source().text()[span.start() as usize..span.end() as usize])
            .collect();
        assert_eq!(exported, expected_export);
    }
}

#[test]
fn native_math_source_queries_choose_object_edges_and_batch_exports_the_same_snapshot() {
    use crate::identity::{LogicalBoundary, SourceBoundary};
    use crate::ir::TextTarget;
    use crate::layout::{LayoutDraft, LayoutKey, LayoutStyle};
    use crate::mapping::{Affinity, PointInput, PointQuery, Position};
    use crate::presentation::PresentationBatch;
    let budget = SourceBudget::preview();
    let model = parse(&budget, "A $x^2$ B", Extensions::MATH);
    for (affinity, edge) in [(Affinity::Before, 2), (Affinity::After, 3)] {
        let mut op = budget.begin(CancellationToken::new());
        let result = model
            .map_point(
                &PointQuery {
                    model: model.identity(),
                    input: PointInput::Source {
                        target: TextTarget::Flow(model.main_flow_id()),
                        byte: SourceBoundary::new(4),
                    },
                    affinity,
                },
                &mut op,
            )
            .unwrap();
        assert_eq!(
            result.results()[0].chosen,
            Position::Flow(LogicalBoundary::new(edge))
        );
        assert!(result.results()[0].effects.object);
    }
    let mut op = budget.begin(CancellationToken::new());
    let key = LayoutKey::new(
        model.identity(),
        1,
        1,
        1,
        800.0,
        LayoutStyle::new(16.0, 6.0).unwrap(),
    )
    .unwrap();
    let mut draft = LayoutDraft::new(model.clone(), key, &mut op).unwrap();
    let measurement = draft.measurement_batch(256, &mut op).unwrap().unwrap();
    let batch = PresentationBatch::build(model.clone(), &measurement, &mut op).unwrap();
    assert_eq!(batch.derived().len(), 1);
    assert_eq!(batch.texts()[0].derived.len(), 1);
    let derived = batch.derived()[0];
    assert_eq!(derived.offset_utf16, 2);
    let value = |slice: crate::presentation::BlobSlice| {
        std::str::from_utf8(
            &batch.blob()[slice.start as usize..slice.start as usize + slice.len as usize],
        )
        .unwrap()
    };
    assert_eq!(value(derived.input), "x^2");
    assert_eq!(value(derived.export), "$x^2$");
    assert_eq!(value(batch.texts()[0].value), "A \u{fffc} B");
}

#[test]
fn native_math_in_image_alt_has_readable_raw_export_and_keeps_main_atomic() {
    use crate::layout::{LayoutDraft, LayoutKey, LayoutStyle};
    use crate::presentation::PresentationBatch;
    let budget = SourceBudget::preview();
    let model = parse(&budget, "![value $x$](img)", Extensions::MATH);
    assert_eq!(main(&model), "\u{fffc}");
    assert_eq!(model.ir().objects().len(), 2);
    let mut op = budget.begin(CancellationToken::new());
    let key = LayoutKey::new(
        model.identity(),
        1,
        1,
        1,
        800.0,
        LayoutStyle::new(16.0, 6.0).unwrap(),
    )
    .unwrap();
    let mut draft = LayoutDraft::new(model.clone(), key, &mut op).unwrap();
    let measurement = draft.measurement_batch(256, &mut op).unwrap().unwrap();
    let batch = PresentationBatch::build(model, &measurement, &mut op).unwrap();
    assert!(
        batch.derived().is_empty(),
        "the alt formula is not a second Main object"
    );
    let alt = batch.images()[0].alt;
    assert_eq!(
        std::str::from_utf8(&batch.blob()[alt.start as usize..(alt.start + alt.len) as usize])
            .unwrap(),
        "value $x$"
    );
}

#[test]
fn native_math_tabbed_containers_keep_only_content_in_the_owned_input() {
    for (input, expected) in [
        ("> $$\n>\tx+y\n> $$", "\n  x+y\n"),
        ("- $$\n\tx+y\n  $$", "\n  x+y\n"),
    ] {
        let budget = SourceBudget::preview();
        let model = parse(&budget, input, Extensions::MATH);
        assert_eq!(main(&model), "\u{fffc}");
        let ObjectRow::Derived { input: payload, .. } = model.ir().objects()[0] else {
            panic!()
        };
        assert_eq!(
            model
                .ir()
                .text(model.ir().payloads()[payload.index()].value)
                .unwrap(),
            expected,
            "{input:?}"
        );
    }
}
