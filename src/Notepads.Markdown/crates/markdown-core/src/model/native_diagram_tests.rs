// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE file in the project root for license information.
// ---------------------------------------------------------------------------------------------

use super::*;
use crate::{
    config::{Extensions, Profile},
    identity::SourceKey,
    ir::{DerivedKind, ObjectRow},
    policy::CancellationToken,
    reader::MainReader,
    source::SourceDocument,
};

fn parse(budget: &SourceBudget, input: &str, extensions: Extensions) -> AcceptedModel {
    let mut op = budget.begin(CancellationToken::new());
    let source = SourceDocument::new(
        SourceKey::new([215; 16], 1).unwrap(),
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
    let mut bytes = [0; 7];
    let mut result = String::new();
    loop {
        let read = reader.read_into(&mut bytes).unwrap();
        result.push_str(std::str::from_utf8(&bytes[..read.written]).unwrap());
        if read.done {
            return result;
        }
    }
}

#[test]
fn native_diagram_fenced_body_and_exact_original_raw_export_are_independent() {
    for (input, body, export) in [
        (
            "```mermaid\nflowchart TD\nA-->B\n```",
            "flowchart TD\nA-->B\n",
            None,
        ),
        (
            "~~~mermaid\r\nflowchart LR\r\nA[中文&amp;]-->B\r\n~~~~\r\n",
            "flowchart LR\nA[中文&amp;]-->B\n",
            None,
        ),
        (
            "> ```mermaid\n> flowchart TD\n> A --> B\n> ```\n",
            "flowchart TD\nA --> B\n",
            Some("```mermaid\nflowchart TD\nA --> B\n```\n"),
        ),
        (
            "- ```mermaid\n  flowchart TD\n  A --> B\n  ```\n",
            "flowchart TD\nA --> B\n",
            Some("```mermaid\nflowchart TD\nA --> B\n```\n"),
        ),
        ("```mermaid\n", "", None),
        (
            "```mermaid\nflowchart TD\nA-->B",
            "flowchart TD\nA-->B",
            None,
        ),
    ] {
        let budget = SourceBudget::preview();
        {
            let model = parse(&budget, input, Extensions::MERMAID);
            assert_eq!(main(&model), "\u{fffc}", "{input:?}");
            assert_eq!(model.ir().objects().len(), 1);
            let ObjectRow::Derived {
                kind,
                input: payload,
                export_sources,
                ..
            } = model.ir().objects()[0]
            else {
                panic!()
            };
            assert_eq!(kind, DerivedKind::Diagram);
            assert_eq!(
                model
                    .ir()
                    .text(model.ir().payloads()[payload.index()].value)
                    .unwrap(),
                body
            );
            let raw: String = export_sources
                .slice(model.ir().sources())
                .unwrap()
                .iter()
                .map(|span| &model.source().text()[span.start() as usize..span.end() as usize])
                .collect();
            // Like Math export, copy the complete original fenced object,
            // excluding its enclosing quote/list prefixes. Every byte remains
            // an actual accepted source fragment, preserving original CRLF.
            assert_eq!(raw, export.unwrap_or(input));
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
fn native_diagram_opt_in_preserves_other_fences_and_can_coexist_with_math() {
    let budget = SourceBudget::preview();
    for source in [
        "```mermaidX\nA\n```",
        "```rust\nA\n```",
        "    mermaid\n    A\n",
    ] {
        let model = parse(&budget, source, Extensions::MERMAID);
        assert!(model.ir().objects().is_empty());
        assert!(!main(&model).contains('\u{fffc}'));
    }
    let plain = parse(
        &budget,
        "```mermaid\nflowchart TD\nA-->B\n```",
        Extensions::NONE,
    );
    assert!(plain.ir().objects().is_empty());
    assert_eq!(main(&plain), "flowchart TD\nA-->B\n");
    let both = parse(
        &budget,
        "$x$\n\n```mermaid\nflowchart TD\nA-->B\n```\n\nend",
        Extensions::ALL,
    );
    assert_eq!(main(&both), "\u{fffc}\n\u{fffc}\nend");
    assert_eq!(both.ir().objects().len(), 2);
    assert!(matches!(
        both.ir().objects()[0],
        ObjectRow::Derived {
            kind: DerivedKind::Math(_),
            ..
        }
    ));
    assert!(matches!(
        both.ir().objects()[1],
        ObjectRow::Derived {
            kind: DerivedKind::Diagram,
            ..
        }
    ));
}

#[test]
fn derived_fence_languages_use_the_first_complete_decoded_word() {
    let budget = SourceBudget::preview();
    let parse_fence = |info: &str| parse(&budget, &format!("```{info}\nA\n```"), Extensions::ALL);
    for info in ["math\\-ext", "mermaid&amp;v2", "math&#120;"] {
        let model = parse_fence(info);
        assert!(model.ir().objects().is_empty(), "{info}");
        assert_eq!(main(&model), "A\n");
    }
    let math = parse_fence("&#109;ath&#32;option");
    assert!(matches!(
        math.ir().objects()[0],
        ObjectRow::Derived {
            kind: DerivedKind::Math(_),
            ..
        }
    ));
    let diagram = parse_fence("mer&#109;aid");
    assert!(matches!(
        diagram.ir().objects()[0],
        ObjectRow::Derived {
            kind: DerivedKind::Diagram,
            ..
        }
    ));
}

#[test]
fn native_diagram_enters_measurement_presentation_and_source_queries_in_containers() {
    use crate::identity::{LogicalBoundary, SourceBoundary};
    use crate::ir::{NodeKind, TextTarget};
    use crate::layout::{BlockKind, LayoutDraft, LayoutKey, LayoutStyle};
    use crate::mapping::{Affinity, PointInput, PointQuery, Position};
    use crate::presentation::PresentationBatch;

    for (source, raw, quote_depth, item_depth) in [
        (
            "```mermaid\r\nflowchart LR\r\nA-->B\r\n```\r\n",
            "```mermaid\r\nflowchart LR\r\nA-->B\r\n```\r\n",
            0,
            0,
        ),
        (
            "> ```mermaid\n> flowchart LR\n> A-->B\n> ```\n",
            "```mermaid\nflowchart LR\nA-->B\n```\n",
            1,
            0,
        ),
        (
            "- ```mermaid\n  flowchart LR\n  A-->B\n  ```\n",
            "```mermaid\nflowchart LR\nA-->B\n```\n",
            0,
            1,
        ),
    ] {
        let budget = SourceBudget::preview();
        let model = parse(&budget, source, Extensions::MERMAID);
        for (affinity, edge) in [(Affinity::Before, 0), (Affinity::After, 1)] {
            let mut op = budget.begin(CancellationToken::new());
            let mapped = model
                .map_point(
                    &PointQuery {
                        model: model.identity(),
                        input: PointInput::Source {
                            target: TextTarget::Flow(model.main_flow_id()),
                            byte: SourceBoundary::new((source.find("A-->B").unwrap() + 1) as u64),
                        },
                        affinity,
                    },
                    &mut op,
                )
                .unwrap();
            assert_eq!(
                mapped.results()[0].chosen,
                Position::Flow(LogicalBoundary::new(edge))
            );
            assert!(mapped.results()[0].effects.object);
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
        let measurement = draft
            .measurement_batch(256, &mut op)
            .unwrap()
            .expect("the diagram must enter viewport measurement");
        assert_eq!(measurement.requests.len(), 1);
        let request = measurement.requests[0];
        assert_eq!(request.kind, BlockKind::Paragraph);
        assert_eq!(
            model.ir().nodes()[request.node.index()].kind,
            NodeKind::Diagram
        );
        assert_eq!(
            (request.context.quote_depth, request.context.item_depth),
            (quote_depth, item_depth)
        );
        let batch = PresentationBatch::build(model.clone(), &measurement, &mut op)
            .unwrap_or_else(|error| panic!("{source:?}: {error:?}; request={request:?}"));
        assert_eq!(batch.blocks().len(), 1);
        assert_eq!(batch.texts().len(), 1);
        assert_eq!(batch.derived().len(), 1);
        assert_eq!(batch.decorations().len(), usize::from(item_depth != 0));
        let value = |slice: crate::presentation::BlobSlice| {
            std::str::from_utf8(
                &batch.blob()[slice.start as usize..(slice.start + slice.len) as usize],
            )
            .unwrap()
        };
        let derived = batch.derived()[0];
        assert_eq!(derived.kind, DerivedKind::Diagram);
        assert_eq!(derived.offset_utf16, 0);
        assert_eq!(value(derived.input), "flowchart LR\nA-->B\n");
        assert_eq!(value(derived.export), raw);
        assert_eq!(value(batch.texts()[0].value), "\u{fffc}");
    }
}
