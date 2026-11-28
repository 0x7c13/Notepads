// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE file in the project root for license information.
// ---------------------------------------------------------------------------------------------

use super::*;
use crate::config::{ParseConfig, Profile};
use crate::identity::{LogicalSpan, SourceKey};
use crate::ir::{Alignment, NodeKind};
use crate::layout::{BlockKind, LayoutDraft, LayoutStyle};
use crate::policy::{CancellationToken, SourceBudget};
use crate::source::SourceDocument;

fn parse(budget: &SourceBudget, input: &str) -> AcceptedModel {
    parse_config(budget, input, ParseConfig::new(Profile::G3))
}

fn parse_config(budget: &SourceBudget, input: &str, config: ParseConfig) -> AcceptedModel {
    let mut op = budget.begin(CancellationToken::new());
    let source = SourceDocument::new(
        SourceKey::new([128; 16], 1).unwrap(),
        input.as_bytes(),
        &mut op,
    )
    .unwrap();
    AcceptedModel::parse(source.snapshot(), config, 1, &mut op).unwrap()
}

#[test]
fn citation_links_resolve_to_private_local_bibliography_entries() {
    use crate::config::Extensions;
    let source = "---\nreferences: [{id: work, title: Title, author: [{family: Name}]}]\n---\n\n<a id=\"notepads-reference-&#48;00\"></a>\n\n[Ordinary](#notepads-reference-000) and [@work].";
    let budget = SourceBudget::preview();
    let model = parse_config(
        &budget,
        source,
        ParseConfig::with_extensions(
            Profile::G3,
            Extensions::from_bits(
                Extensions::FRONT_MATTER_HIDE.bits()
                    | Extensions::CITATIONS.bits()
                    | Extensions::HTML_BLOCKS.bits(),
            )
            .unwrap(),
        ),
    );
    let mut op = budget.begin(CancellationToken::new());
    let batch = project(&model, &mut op);
    let [ordinary, citation] = batch.links() else {
        panic!("two native links expected");
    };
    assert_eq!(
        value(&batch, ordinary.destination),
        "#notepads-reference-000"
    );
    assert_eq!(ordinary.local_position, None);
    assert_eq!(citation.destination.len, 0);
    assert_eq!(value(&batch, citation.title), "Name n.d.");
    let entry = batch
        .texts()
        .iter()
        .find(|text| value(&batch, text.value) == "Name (n.d.). Title")
        .unwrap();
    assert_eq!(citation.local_position, Some(entry.logical.start()));
    assert_eq!(model.html_anchors().len(), 1);
}

#[test]
fn citation_in_excerpt_keeps_text_when_its_bibliography_is_outside_scope() {
    use crate::config::Extensions;
    let source = "---\nreferences: [{id: work, title: Title, author: [{family: Name}]}]\n---\n\n## Excerpt\n\n[@work]\n\n## Next\n\nOutside";
    let budget = SourceBudget::preview();
    let config = ParseConfig::with_extensions(
        Profile::G3,
        Extensions::from_bits(Extensions::FRONT_MATTER_HIDE.bits() | Extensions::CITATIONS.bits())
            .unwrap(),
    );
    let model = parse_config(&budget, source, config);
    let mut op = budget.begin(CancellationToken::new());
    let key = draft(&model, &mut op).key();
    let outline = crate::outline::Outline::build(model.clone(), &mut op).unwrap();
    let heading = outline.headings()[0].node;
    let mut section =
        LayoutDraft::section(model.clone(), key, heading.value(), None, &mut op).unwrap();
    let requests = section.measurement_batch(256, &mut op).unwrap().unwrap();
    let presentation = PresentationBatch::build(model, &requests, &mut op).unwrap();
    assert!(presentation.links().is_empty());
    assert!(
        presentation
            .texts()
            .iter()
            .any(|text| value(&presentation, text.value) == "(Name n.d.)")
    );
}

#[test]
fn code_fence_metadata_marks_physical_lines_without_changing_text() {
    let budget = SourceBudget::preview();
    let authored = "js:line-numbers=98 {2,4-5}";
    let markdown = format!(
        "```{authored}\nconst note = '会议';\n\nfoo();\nbar();\n```\n\n```text\nplain\n```"
    );
    let model = parse(&budget, &markdown);
    let mut op = budget.begin(CancellationToken::new());
    let batch = project(&model, &mut op);
    let [code, ordinary] = batch.blocks() else {
        panic!("two fenced blocks expected")
    };
    assert_eq!(value(&batch, code.code_info), authored);
    let text = batch.texts()[code.texts.start()];
    assert_eq!(
        value(&batch, text.value),
        "const note = '会议';\n\nfoo();\nbar();\n"
    );
    assert_eq!(code.code_first_line_number, 98);
    let from = code.code_highlight_lines.start as usize;
    let to = from + code.code_highlight_lines.len as usize;
    let ranges = batch.blob()[from..to]
        .chunks_exact(4)
        .map(|chunk| u32::from_le_bytes(chunk.try_into().unwrap()))
        .collect::<Vec<_>>();
    assert_eq!(ranges, [2, 2, 4, 5]);
    assert_eq!(ordinary.code_first_line_number, 0);
    assert_eq!(ordinary.code_highlight_lines, BlobSlice::default());
}

#[test]
fn review_regression_large_code_info_remains_presentable() {
    let budget = SourceBudget::model_fixture();
    let info = "x".repeat(crate::policy::CHECKPOINT_BYTES as usize + 1);
    let model = parse(&budget, &format!("```{info}\nbody\n```\n"));
    let mut op = budget.begin(CancellationToken::new());
    let batch = project(&model, &mut op);
    assert_eq!(value(&batch, batch.blocks()[0].code_info), info);
    assert_eq!(value(&batch, batch.texts()[0].value), "body\n");
}

#[test]
fn review_regression_list_marker_belongs_to_first_toc_entry() {
    use crate::config::Extensions;
    for (marker, kind) in [
        ("-", DecorationKind::Bullet),
        ("1.", DecorationKind::Ordered),
    ] {
        let budget = SourceBudget::model_fixture();
        let model = parse_config(
            &budget,
            &format!("# Heading\n\n{marker} [TOC]\n\n   Later paragraph\n"),
            ParseConfig::with_extensions(Profile::G3, Extensions::TABLE_OF_CONTENTS),
        );
        let mut op = budget.begin(CancellationToken::new());
        let batch = project(&model, &mut op);
        let entry = batch
            .blocks()
            .iter()
            .find(|block| {
                model.ir().nodes()[block.request.node.index()].kind
                    == NodeKind::TableOfContentsEntry
            })
            .unwrap();
        let decorations = entry.decorations.slice(batch.decorations()).unwrap();
        assert_eq!(decorations.len(), 1);
        assert_eq!(decorations[0].kind, kind);
        assert_eq!(batch.decorations().len(), 1);
    }
}

#[test]
fn inline_html_scripts_shape_boundaries_keep_equal_geometry_siblings_distinct() {
    use crate::config::Extensions;
    for profile in [Profile::C0, Profile::G3] {
        for tag in ["sup", "sub"] {
            for (left, right) in [("a", "\u{301}"), ("👩", "\u{200d}💻")] {
                for siblings in [false, true] {
                    let budget = SourceBudget::preview();
                    let input = if siblings {
                        format!("A <{tag}>{left}</{tag}><{tag}>{right}</{tag}> Z")
                    } else {
                        format!("A <{tag}>{left}{right}</{tag}> Z")
                    };
                    let model = parse_config(
                        &budget,
                        &input,
                        ParseConfig::with_extensions(profile, Extensions::HTML_INLINE),
                    );
                    let mut op = budget.begin(CancellationToken::new());
                    let batch = project(&model, &mut op);
                    let scopes = batch.script_scopes();
                    assert_eq!(scopes.len(), if siblings { 2 } else { 1 });
                    let end =
                        2 + (left.encode_utf16().count() + right.encode_utf16().count()) as u32;
                    assert_eq!((scopes[0].start, scopes.last().unwrap().end), (2, end));
                    if siblings {
                        assert_eq!(scopes[0].style, scopes[1].style);
                        assert_eq!(scopes[0].end, 2 + left.encode_utf16().count() as u32);
                        assert_eq!(scopes[0].end, scopes[1].start);
                    }
                    drop((batch, op, model));
                    released(&budget);
                }
            }
        }
    }
}
#[test]
fn inline_html_scripts_shape_boundaries_keep_real_markdown_ancestors_and_owners() {
    use crate::config::Extensions;
    use crate::ir::DetailRow;
    for profile in [Profile::C0, Profile::G3] {
        let budget = SourceBudget::preview();
        let input = "A <sup>**a**__\u{301}__ *b* [c](#same)[\u{301}](#same) `d`</sup> Z\n\n**ffi &amp; \\* 👩\u{200d}💻** *e* [f](#other) `g` ~~h~~ [***nested***](#nested)\n\n# *head*\n\n| col |\n| --- |\n| **cell** |\n\n```\nplain\n```\n\n<div>plain</div>";
        let model = parse_config(
            &budget,
            input,
            ParseConfig::with_extensions(profile, Extensions::HTML_INLINE),
        );
        let mut op = budget.begin(CancellationToken::new());
        let batch = project(&model, &mut op);
        bounds(&batch);
        let mut expected = Vec::new();
        for node in model.ir().nodes() {
            if node.logical.is_empty()
                || !matches!(
                    node.kind,
                    NodeKind::Emphasis
                        | NodeKind::Strong
                        | NodeKind::Strike
                        | NodeKind::Link
                        | NodeKind::InlineCode
                )
            {
                continue;
            }
            if node.detail.is_some_and(|id| {
                matches!(model.ir().details()[id.index()], DetailRow::Html { .. })
            }) {
                continue;
            }
            let parent = node.parent.unwrap();
            let mut owner = parent;
            while !matches!(
                model.ir().nodes()[owner.index()].kind,
                NodeKind::Paragraph | NodeKind::Heading | NodeKind::TableCell
            ) {
                owner = model.ir().nodes()[owner.index()].parent.unwrap();
            }
            expected.push((
                owner.value(),
                parent.value(),
                node.logical.start(),
                node.logical.end(),
            ));
        }
        assert!(expected.iter().any(|(owner, parent, _, _)| owner != parent));
        assert!(expected.len() >= 10, "{profile:?}");
        let mut actual = Vec::new();
        for text in batch.texts() {
            for scope in model.inline_script_scopes(text.node, &mut op).unwrap() {
                if scope.font_flags & 32 != 0 {
                    assert_eq!(scope.font_flags, 32);
                    assert_eq!(scope.style, ScriptStyle::default());
                    actual.push((
                        scope.owner.value(),
                        scope.parent.value(),
                        scope.start,
                        scope.end,
                    ));
                }
            }
        }
        expected.sort_unstable();
        actual.sort_unstable();
        assert_eq!(actual, expected, "{profile:?}");
        assert!(
            batch
                .script_scopes()
                .iter()
                .any(|scope| scope.text > 0 && scope.font_flags == 32)
        );
        assert!(
            batch
                .script_scopes()
                .iter()
                .filter(|scope| scope.font_flags & 32 != 0)
                .all(|scope| scope.font_flags == 32
                    && scope.start < scope.end
                    && scope.style == ScriptStyle::default())
        );
        // A single strong container retains a single range despite entity, escape,
        // surrogate and ZWJ atoms. Its internal atom edges are not shape barriers.
        let second = batch.texts()[1];
        let strong = batch
            .script_scopes()
            .iter()
            .filter(|scope| scope.text == 1 && scope.font_flags == 32 && scope.start == 0)
            .collect::<Vec<_>>();
        assert_eq!(strong.len(), 1);
        assert_eq!(
            strong[0].end,
            "ffi & * 👩\u{200d}💻".encode_utf16().count() as u32
        );
        assert!(second.runs.len() > 1);
        drop((batch, op, model));
        released(&budget);
    }
}

#[test]
fn inline_html_scripts_shape_boundaries_preserve_sibling_html_containers() {
    use crate::config::Extensions;
    for profile in [Profile::C0, Profile::G3] {
        for tag in [
            "b", "strong", "i", "em", "del", "s", "strike", "code", "kbd", "samp",
        ] {
            let budget = SourceBudget::preview();
            let pair = format!("<{tag}>a</{tag}><{tag}>\u{301}</{tag}>");
            let input = format!("A {pair} <sup>{pair}</sup> Z");
            let model = parse_config(
                &budget,
                &input,
                ParseConfig::with_extensions(profile, Extensions::HTML_INLINE),
            );
            let mut op = budget.begin(CancellationToken::new());
            let batch = project(&model, &mut op);
            assert_eq!(
                value(&batch, batch.texts()[0].value),
                "A a\u{301} a\u{301} Z"
            );
            let ranges = batch
                .script_scopes()
                .iter()
                .map(|scope| (scope.start, scope.end))
                .collect::<Vec<_>>();
            assert_eq!(
                ranges,
                [(2, 3), (3, 4), (5, 6), (5, 7), (6, 7)],
                "{profile:?} {tag}"
            );
            for scope in batch.script_scopes().iter().filter(|scope| scope.start < 5) {
                assert_eq!(scope.font_flags, 32);
                assert_eq!(scope.style, ScriptStyle::default());
            }
            let shape_count = batch
                .script_scopes()
                .iter()
                .filter(|scope| scope.font_flags & 32 != 0)
                .count();
            assert_eq!(
                shape_count,
                if matches!(tag, "del" | "s" | "strike") {
                    4
                } else {
                    2
                }
            );
            drop((batch, op, model));
            released(&budget);
        }
    }
}

#[test]
fn inline_html_scripts_shape_boundaries_leave_nonscript_documents_empty() {
    use crate::config::Extensions;
    for profile in [Profile::C0, Profile::G3] {
        let budget = SourceBudget::preview();
        let model = parse_config(
            &budget,
            "**a** *b* ~~c~~ [d](#x) `e` <b>f</b> <code>g</code>",
            ParseConfig::with_extensions(profile, Extensions::HTML_INLINE),
        );
        let mut op = budget.begin(CancellationToken::new());
        let batch = project(&model, &mut op);
        assert!(batch.script_scopes().is_empty());
        assert!(batch.script_runs().is_empty());
        drop((batch, op, model));
        released(&budget);
    }
}
#[test]
fn inline_html_scripts_distinguish_code_line_boxes_from_code_fonts() {
    use crate::config::Extensions;
    for profile in [Profile::C0, Profile::G3] {
        for (body, visible, box_flags) in [
            ("<code></code>", "", 24),
            ("<kbd></kbd>", "", 8),
            ("<samp></samp>", "", 8),
            ("<code>x</code>", "x", 24),
            ("<kbd>x</kbd>", "x", 8),
            ("<samp>x</samp>", "x", 8),
            ("`x`", "x", 24),
            ("` a\nb `", "a b", 24),
        ] {
            for (prefix, suffix, opening_flags, leaf_flags) in [
                ("<sub>", "</sub>", 0, 0),
                ("**<sub>", "</sub>**", RUN_BOLD, RUN_BOLD),
                ("*<sub>", "</sub>*", RUN_ITALIC, RUN_ITALIC),
                ("<b><sub>", "</sub></b>", RUN_BOLD, RUN_BOLD),
                ("<i><sub>", "</sub></i>", RUN_ITALIC, RUN_ITALIC),
                ("<sub>***", "***</sub>", 0, RUN_BOLD | RUN_ITALIC),
            ] {
                let budget = SourceBudget::preview();
                let input = format!("A {prefix}{body}{suffix} B");
                let model = parse_config(
                    &budget,
                    &input,
                    ParseConfig::with_extensions(profile, Extensions::HTML_INLINE),
                );
                let mut op = budget.begin(CancellationToken::new());
                let batch = project(&model, &mut op);
                let font_scopes = batch
                    .script_scopes()
                    .iter()
                    .filter(|scope| scope.font_flags & SCRIPT_SHAPE_ONLY == 0)
                    .copied()
                    .collect::<Vec<_>>();
                bounds(&batch);
                assert_eq!(
                    value(&batch, batch.texts()[0].value),
                    format!("A {visible} B")
                );
                assert_eq!(font_scopes.len(), 2, "{profile:?} {input}");
                let mut flags = font_scopes
                    .iter()
                    .map(|scope| {
                        assert_eq!(
                            (scope.text, scope.start, scope.end),
                            (0, 2, 2 + visible.len() as u32)
                        );
                        assert_eq!(scope.style.absolute_dip, 1.0);
                        assert!((scope.style.size_scale - 5.0 / 6.0).abs() < 1e-12);
                        scope.font_flags
                    })
                    .collect::<Vec<_>>();
                flags.sort_unstable();
                assert_eq!(
                    flags,
                    [opening_flags, box_flags | leaf_flags],
                    "{profile:?} {input}"
                );
                for run in batch
                    .runs()
                    .iter()
                    .filter(|run| run.start >= 2 && run.end <= 2 + visible.len() as u32)
                {
                    assert_eq!(run.flags, RUN_CODE | leaf_flags, "{profile:?} {input}");
                }
            }
        }
    }
}
#[test]
fn inline_html_scripts_capture_opening_fonts_and_empty_descendant_boxes() {
    use crate::config::Extensions;
    for profile in [Profile::C0, Profile::G3] {
        let budget = SourceBudget::preview();
        let input = "A <b><sup></sup></b> <sup><b></b></sup> <sup><i></i><code></code><kbd></kbd><samp></samp></sup> **<sub></sub>**";
        let model = parse_config(
            &budget,
            input,
            ParseConfig::with_extensions(profile, Extensions::HTML_INLINE),
        );
        let mut op = budget.begin(CancellationToken::new());
        let batch = project(&model, &mut op);
        let font_scopes = batch
            .script_scopes()
            .iter()
            .filter(|scope| scope.font_flags & SCRIPT_SHAPE_ONLY == 0)
            .copied()
            .collect::<Vec<_>>();
        bounds(&batch);
        assert_eq!(font_scopes.len(), 9);
        assert_eq!(batch.script_runs().len(), 0);
        for (position, expected) in [
            (2, vec![RUN_BOLD]),
            (3, vec![0, RUN_BOLD]),
            (
                4,
                vec![
                    0,
                    RUN_ITALIC,
                    RUN_CODE,
                    RUN_CODE,
                    RUN_CODE | SCRIPT_CODE_LINE_HEIGHT,
                ],
            ),
            (5, vec![RUN_BOLD]),
        ] {
            let mut actual = font_scopes
                .iter()
                .filter(|scope| scope.start == position)
                .map(|scope| {
                    assert_eq!(scope.text, 0);
                    assert_eq!(scope.end, position);
                    assert!((scope.style.size_scale - 5.0 / 6.0).abs() < 1e-12);
                    scope.font_flags
                })
                .collect::<Vec<_>>();
            actual.sort_unstable();
            assert_eq!(actual, expected, "{profile:?} {position}");
        }
    }
}
#[test]
fn inline_html_scripts_resolve_ordered_nesting_and_restore_sibling_geometry() {
    use crate::config::Extensions;
    for profile in [Profile::C0, Profile::G3] {
        let budget = SourceBudget::preview();
        let input = "Base <sup>[A<sup>B</sup><sub>C</sub>D](#target)</sup> / <sub>E<sup>F</sup><sub>G</sub>H</sub> Tail";
        let model = parse_config(
            &budget,
            input,
            ParseConfig::with_extensions(profile, Extensions::HTML_INLINE),
        );
        let mut op = budget.begin(CancellationToken::new());
        let batch = project(&model, &mut op);
        let font_scopes = batch
            .script_scopes()
            .iter()
            .filter(|scope| scope.font_flags & SCRIPT_SHAPE_ONLY == 0)
            .copied()
            .collect::<Vec<_>>();
        bounds(&batch);
        let text = batch.texts()[0];
        assert_eq!(value(&batch, text.value), "Base ABCD / EFGH Tail");
        assert_eq!(batch.links().len(), 1);
        assert_eq!(value(&batch, batch.links()[0].destination), "#target");
        assert!(batch.runs().iter().all(|run| run.start < run.end));
        assert_eq!(font_scopes.len(), 6);
        let expected = [
            ("Base ", (1.0, 0.0, 0.0)),
            ("A", (5.0 / 6.0, -1.0 / 3.0, -1.0)),
            ("B", (25.0 / 36.0, -11.0 / 18.0, -2.0)),
            ("C", (25.0 / 36.0, -1.0 / 6.0, 0.0)),
            ("D", (5.0 / 6.0, -1.0 / 3.0, -1.0)),
            (" / ", (1.0, 0.0, 0.0)),
            ("E", (5.0 / 6.0, 1.0 / 5.0, 1.0)),
            ("F", (25.0 / 36.0, -7.0 / 90.0, 0.0)),
            ("G", (25.0 / 36.0, 11.0 / 30.0, 2.0)),
            ("H", (5.0 / 6.0, 1.0 / 5.0, 1.0)),
            (" Tail", (1.0, 0.0, 0.0)),
        ]
        .into_iter()
        .flat_map(|(text, style)| std::iter::repeat_n(style, text.encode_utf16().count()))
        .collect::<Vec<_>>();
        let mut actual = vec![ScriptStyle::default(); text.utf16_len as usize];
        let mut previous = None;
        for script in batch.script_runs() {
            assert!(previous.is_none_or(|run| run < script.run));
            previous = Some(script.run);
            let run = batch.runs()[script.run as usize];
            actual[run.start as usize..run.end as usize].fill(script.style);
        }
        for (index, (actual, expected)) in actual.iter().zip(expected).enumerate() {
            assert!(
                (actual.size_scale - expected.0).abs() < 1e-12,
                "{profile:?} {index}"
            );
            assert!(
                (actual.relative_em - expected.1).abs() < 1e-12,
                "{profile:?} {index}"
            );
            assert_eq!(actual.absolute_dip, expected.2, "{profile:?} {index}");
        }
        assert_eq!(
            font_scopes
                .iter()
                .map(|scope| (scope.text, scope.start, scope.end))
                .collect::<Vec<_>>(),
            [
                (0, 5, 9),
                (0, 6, 7),
                (0, 7, 8),
                (0, 12, 16),
                (0, 13, 14),
                (0, 14, 15)
            ]
        );
    }
}

#[test]
fn inline_html_scripts_keep_empty_owners_ancestors_and_multiline_scopes() {
    use crate::config::Extensions;
    let budget = SourceBudget::preview();
    let input = format!(
        "{}{}\n\n{}{}\n\nA <sup>B<br>C</sup> D",
        "<sup>".repeat(16),
        "</sup>".repeat(16),
        "<sub>".repeat(256),
        "</sub>".repeat(256)
    );
    let model = parse_config(
        &budget,
        &input,
        ParseConfig::with_extensions(Profile::G3, Extensions::HTML_INLINE),
    );
    let mut op = budget.begin(CancellationToken::new());
    let batch = project(&model, &mut op);
    let font_scopes = batch
        .script_scopes()
        .iter()
        .filter(|scope| scope.font_flags & SCRIPT_SHAPE_ONLY == 0)
        .copied()
        .collect::<Vec<_>>();
    bounds(&batch);
    assert_eq!(batch.texts().len(), 3);
    assert_eq!(font_scopes.len(), 273);
    for text in &batch.texts()[..2] {
        assert_eq!(text.logical.len(), 0);
        assert_eq!(value(&batch, text.value), "");
        assert_eq!(text.runs.len(), 0);
    }
    assert_eq!(batch.texts()[0].logical, batch.texts()[1].logical);
    for (owner, count, deepest_dip) in [(0, 16, -16.0), (1, 256, 256.0)] {
        let scopes = font_scopes
            .iter()
            .filter(|scope| scope.text == owner)
            .collect::<Vec<_>>();
        assert_eq!(scopes.len(), count);
        assert!(scopes.iter().all(|scope| scope.start == 0
            && scope.end == 0
            && scope.style.size_scale > 0.0
            && scope.style.size_scale.is_finite()));
        assert!(
            scopes
                .iter()
                .any(|scope| scope.style.absolute_dip == deepest_dip)
        );
    }
    assert_eq!(value(&batch, batch.texts()[2].value), "A B\nC D");
    let scope = font_scopes.iter().find(|scope| scope.text == 2).unwrap();
    assert_eq!((scope.start, scope.end), (2, 5));
    assert_eq!(scope.style.absolute_dip, -1.0);
}
#[test]
fn inline_html_emphasis_composes_with_markdown_and_preserves_one_link() {
    use crate::config::Extensions;
    let budget = SourceBudget::preview();
    let mut op = budget.begin(CancellationToken::new());
    let markdown = "A <b>[X <strong>中😀</strong> <i>*Y*</i> <s>Z</s> tail](#target)</b> B";
    let source = SourceDocument::new(
        SourceKey::new([129; 16], 1).unwrap(),
        markdown.as_bytes(),
        &mut op,
    )
    .unwrap();
    let model = AcceptedModel::parse(
        source.snapshot(),
        ParseConfig::with_extensions(Profile::G3, Extensions::HTML_INLINE),
        1,
        &mut op,
    )
    .unwrap();
    let batch = project(&model, &mut op);
    bounds(&batch);
    let text = batch.texts()[0];
    assert_eq!(value(&batch, text.value), "A X 中😀 Y Z tail B");
    assert_eq!(batch.links().len(), 1);
    let link = batch.links()[0];
    assert_eq!(value(&batch, link.destination), "#target");
    assert_eq!(model.ir().nodes()[link.node.index()].kind, NodeKind::Link);
    let actual = batch
        .runs()
        .iter()
        .flat_map(|run| std::iter::repeat_n((run.flags, run.link), (run.end - run.start) as usize))
        .collect::<Vec<_>>();
    let expected = [
        ("A ", 0, None),
        ("X 中😀 ", RUN_BOLD, Some(0)),
        ("Y", RUN_BOLD | RUN_ITALIC, Some(0)),
        (" ", RUN_BOLD, Some(0)),
        ("Z", RUN_BOLD | RUN_STRIKE, Some(0)),
        (" tail", RUN_BOLD, Some(0)),
        (" B", 0, None),
    ]
    .into_iter()
    .flat_map(|(text, flags, link)| std::iter::repeat_n((flags, link), text.encode_utf16().count()))
    .collect::<Vec<_>>();
    assert_eq!(actual, expected);
}

#[test]
fn html_ins_preserves_underline_with_links_scripts_and_html_blocks() {
    use crate::config::Extensions;
    let budget = SourceBudget::preview();
    let model = parse_config(
        &budget,
        "A [plain <ins>**中😀**<sub>x</sub></ins> tail](#target) B `<ins>`\n\n<div>A <ins>block</ins> B</div>",
        ParseConfig::with_extensions(Profile::G3, Extensions::ALL),
    );
    let mut op = budget.begin(CancellationToken::new());
    let batch = project(&model, &mut op);
    bounds(&batch);
    assert_eq!(
        value(&batch, batch.texts()[0].value),
        "A plain 中😀x tail B <ins>"
    );
    assert_eq!(value(&batch, batch.texts()[1].value), "A block B");
    assert_eq!(batch.links().len(), 1);
    assert_eq!(value(&batch, batch.links()[0].destination), "#target");
    let actual = batch
        .runs()
        .iter()
        .flat_map(|run| std::iter::repeat_n((run.flags, run.link), (run.end - run.start) as usize))
        .collect::<Vec<_>>();
    let expected = [
        ("A ", 0, None),
        ("plain ", 0, Some(0)),
        ("中😀", RUN_BOLD | RUN_UNDERLINE, Some(0)),
        ("x", RUN_UNDERLINE, Some(0)),
        (" tail", 0, Some(0)),
        (" B ", 0, None),
        ("<ins>", RUN_CODE, None),
        ("A ", 0, None),
        ("block", RUN_UNDERLINE, None),
        (" B", 0, None),
    ]
    .into_iter()
    .flat_map(|(text, flags, link)| std::iter::repeat_n((flags, link), text.encode_utf16().count()))
    .collect::<Vec<_>>();
    assert_eq!(actual, expected);
    assert!(batch.script_runs().iter().any(|script| {
        let run = batch.runs()[script.run as usize];
        run.flags == RUN_UNDERLINE && run.link == Some(0) && script.style.relative_em > 0.0
    }));
}

#[test]
fn html_semantic_emphasis_composes_with_markdown_and_safe_html_blocks() {
    use crate::config::Extensions;
    let budget = SourceBudget::preview();
    let model = parse_config(
        &budget,
        "A [<VAR>α</var> <cite>**Title**</cite> <dfn>term</dfn>](#target) B `<cite>`\n\n<div><var>x</var> / <cite>work</cite> / <dfn>word</dfn></div>",
        ParseConfig::with_extensions(Profile::G3, Extensions::ALL),
    );
    let mut op = budget.begin(CancellationToken::new());
    let batch = project(&model, &mut op);
    bounds(&batch);
    let actual = batch
        .runs()
        .iter()
        .flat_map(|run| std::iter::repeat_n((run.flags, run.link), (run.end - run.start) as usize))
        .collect::<Vec<_>>();
    let expected = [
        ("A ", 0, None),
        ("α", RUN_ITALIC, Some(0)),
        (" ", 0, Some(0)),
        ("Title", RUN_BOLD | RUN_ITALIC, Some(0)),
        (" ", 0, Some(0)),
        ("term", RUN_ITALIC, Some(0)),
        (" B ", 0, None),
        ("<cite>", RUN_CODE, None),
        ("x", RUN_ITALIC, None),
        (" / ", 0, None),
        ("work", RUN_ITALIC, None),
        (" / ", 0, None),
        ("word", RUN_ITALIC, None),
    ]
    .into_iter()
    .flat_map(|(text, flags, link)| std::iter::repeat_n((flags, link), text.encode_utf16().count()))
    .collect::<Vec<_>>();
    assert_eq!(
        value(&batch, batch.texts()[0].value),
        "A α Title term B <cite>"
    );
    assert_eq!(value(&batch, batch.texts()[1].value), "x / work / word");
    assert_eq!(actual, expected);
    assert_eq!(batch.links().len(), 1);
    assert_eq!(value(&batch, batch.links()[0].destination), "#target");
}

#[test]
fn plain_inline_span_preserves_markdown_and_source() {
    use crate::config::Extensions;
    let input = "A <SPAN>**中😀** [label](#target)<span></span></span> B `<span>`\n\nOther <span title=\"plain\">text</span> / <span>orphan";
    let budget = SourceBudget::preview();
    let model = parse_config(
        &budget,
        input,
        ParseConfig::with_extensions(Profile::G3, Extensions::HTML_INLINE),
    );
    let mut op = budget.begin(CancellationToken::new());
    let batch = project(&model, &mut op);
    bounds(&batch);
    assert_eq!(model.source().text(), input);
    assert_eq!(
        value(&batch, batch.texts()[0].value),
        "A 中😀 label B <span>"
    );
    assert_eq!(
        value(&batch, batch.texts()[1].value),
        "Other <span title=\"plain\">text</span> / <span>orphan"
    );
    assert_eq!(batch.links().len(), 1);
    assert_eq!(value(&batch, batch.links()[0].destination), "#target");
    let actual = batch
        .runs()
        .iter()
        .flat_map(|run| std::iter::repeat_n((run.flags, run.link), (run.end - run.start) as usize))
        .take(batch.texts()[0].utf16_len as usize)
        .collect::<Vec<_>>();
    let expected = [
        ("A ", 0, None),
        ("中😀", RUN_BOLD, None),
        (" ", 0, None),
        ("label", 0, Some(0)),
        (" B ", 0, None),
        ("<span>", RUN_CODE, None),
    ]
    .into_iter()
    .flat_map(|(text, flags, link)| std::iter::repeat_n((flags, link), text.encode_utf16().count()))
    .collect::<Vec<_>>();
    assert_eq!(actual, expected);
}

#[test]
fn html_tt_composes_with_markdown_code_links_and_script_fonts() {
    use crate::config::Extensions;
    let input = "A [<tt>**中😀** <sup>x</sup> `y`</tt> tail](#target) <sub><tt>q</tt></sub>\n\n<p>Start <tt>mono</tt> end</p>";
    let budget = SourceBudget::preview();
    let model = parse_config(
        &budget,
        input,
        ParseConfig::with_extensions(Profile::G3, Extensions::ALL),
    );
    let mut op = budget.begin(CancellationToken::new());
    let batch = project(&model, &mut op);
    bounds(&batch);
    assert_eq!(model.original_source().text(), input);
    assert_eq!(value(&batch, batch.texts()[0].value), "A 中😀 x y tail q");
    assert_eq!(value(&batch, batch.texts()[1].value), "Start mono end");
    assert_eq!(batch.links().len(), 1);
    assert_eq!(value(&batch, batch.links()[0].destination), "#target");
    let actual = batch
        .runs()
        .iter()
        .flat_map(|run| std::iter::repeat_n((run.flags, run.link), (run.end - run.start) as usize))
        .collect::<Vec<_>>();
    let expected = [
        ("A ", 0, None),
        ("中😀", RUN_MONOSPACE | RUN_BOLD, Some(0)),
        (" x ", RUN_MONOSPACE, Some(0)),
        ("y", RUN_MONOSPACE | RUN_CODE, Some(0)),
        (" tail", 0, Some(0)),
        (" ", 0, None),
        ("q", RUN_MONOSPACE, None),
        ("Start ", 0, None),
        ("mono", RUN_MONOSPACE, None),
        (" end", 0, None),
    ]
    .into_iter()
    .flat_map(|(text, flags, link)| std::iter::repeat_n((flags, link), text.encode_utf16().count()))
    .collect::<Vec<_>>();
    assert_eq!(actual, expected);
    assert_eq!(
        batch
            .script_scopes()
            .iter()
            .filter(|scope| scope.font_flags == RUN_CODE && scope.start < scope.end)
            .count(),
        2
    );
}

#[test]
fn inline_html_code_reuses_code_runs_and_retains_only_the_enclosing_link() {
    use crate::config::Extensions;
    let budget = SourceBudget::preview();
    let mut op = budget.begin(CancellationToken::new());
    let input = "A <kbd>Ctrl+P</kbd> *<code>中😀</code>* <samp>[x](u)</samp> [Outer <code>x</code>](#target)";
    let source = SourceDocument::new(
        SourceKey::new([133; 16], 1).unwrap(),
        input.as_bytes(),
        &mut op,
    )
    .unwrap();
    let model = AcceptedModel::parse(
        source.snapshot(),
        ParseConfig::with_extensions(Profile::G3, Extensions::HTML_INLINE),
        1,
        &mut op,
    )
    .unwrap();
    let batch = project(&model, &mut op);
    bounds(&batch);
    assert_eq!(
        value(&batch, batch.texts()[0].value),
        "A Ctrl+P 中😀 [x](u) Outer x"
    );
    assert_eq!(batch.links().len(), 1);
    assert_eq!(value(&batch, batch.links()[0].destination), "#target");
    let actual = batch
        .runs()
        .iter()
        .flat_map(|run| std::iter::repeat_n((run.flags, run.link), (run.end - run.start) as usize))
        .collect::<Vec<_>>();
    let expected = [
        ("A ", 0, None),
        ("Ctrl+P", RUN_CODE, None),
        (" ", 0, None),
        ("中😀", RUN_CODE | RUN_ITALIC, None),
        (" ", 0, None),
        ("[x](u)", RUN_CODE, None),
        (" ", 0, None),
        ("Outer ", 0, Some(0)),
        ("x", RUN_CODE, Some(0)),
    ]
    .into_iter()
    .flat_map(|(text, flags, link)| std::iter::repeat_n((flags, link), text.encode_utf16().count()))
    .collect::<Vec<_>>();
    assert_eq!(actual, expected);
}

fn draft(model: &AcceptedModel, op: &mut Operation) -> LayoutDraft {
    let key = LayoutKey::new(
        model.identity(),
        1,
        2,
        3,
        800.0,
        LayoutStyle::new(16.0, 6.0).unwrap(),
    )
    .unwrap();
    LayoutDraft::new(model.clone(), key, op).unwrap()
}

fn project(model: &AcceptedModel, op: &mut Operation) -> PresentationBatch {
    let mut draft = draft(model, op);
    let key = draft.key();
    let batch = draft.measurement_batch(256, op).unwrap();
    let empty = MeasurementBatch {
        key,
        start: 0,
        requests: &[],
        scope: None,
    };
    PresentationBatch::build(model.clone(), batch.as_ref().unwrap_or(&empty), op).unwrap()
}

fn value(batch: &PresentationBatch, slice: BlobSlice) -> &str {
    let start = slice.start as usize;
    let end = start.checked_add(slice.len as usize).unwrap();
    std::str::from_utf8(batch.blob().get(start..end).unwrap()).unwrap()
}

fn bounds(batch: &PresentationBatch) {
    for block in batch.blocks() {
        assert!(block.texts.start() + block.texts.len() <= batch.texts().len());
        assert!(block.cells.start() + block.cells.len() <= batch.cells().len());
        assert!(block.decorations.start() + block.decorations.len() <= batch.decorations().len());
        value(batch, block.code_info);
        assert_eq!(
            batch.model().ir().nodes()[block.request.node.index()].flow,
            batch.model().main_flow_id()
        );
    }
    for text in batch.texts() {
        assert_eq!(
            value(batch, text.value).encode_utf16().count(),
            text.utf16_len as usize
        );
        assert_eq!(text.logical.len(), u64::from(text.utf16_len));
        let runs = &batch.runs()[text.runs.start()..text.runs.start() + text.runs.len()];
        let mut next = 0;
        for run in runs {
            assert_eq!(run.start, next, "runs cover text exactly once");
            assert!(run.end > run.start && run.end <= text.utf16_len);
            next = run.end;
            if let Some(link) = run.link {
                assert!((link as usize) < batch.links().len());
            }
        }
        assert_eq!(next, text.utf16_len);
        assert!(text.images.start() + text.images.len() <= batch.images().len());
    }
    for cell in batch.cells() {
        assert!((cell.text as usize) < batch.texts().len());
    }
    for image in batch.images() {
        let text = batch.texts()[image.text as usize];
        assert_eq!(
            value(batch, text.value)
                .encode_utf16()
                .nth(image.offset_utf16 as usize),
            Some(0xfffc)
        );
        value(batch, image.destination);
        value(batch, image.title);
        value(batch, image.alt);
    }
    for link in batch.links() {
        value(batch, link.destination);
        value(batch, link.title);
    }
}

fn released(budget: &SourceBudget) {
    let usage = budget.usage();
    assert_eq!(usage.source_capacity, 0);
    assert_eq!(usage.auxiliary_capacity, 0);
    assert_eq!(usage.source_owners, 0);
}

#[test]
fn html_table_metadata_preserves_caption_merges_headers_and_covered_empty_texts() {
    use crate::config::Extensions;
    let budget = SourceBudget::preview();
    let source = "<table><caption>HTML 表格与跨列</caption><thead><tr><th>列一</th><th>列二</th></tr></thead><tbody><tr><td colspan=2>跨两列，&amp; 实体测试</td></tr><tr><td>R2C1</td><td><code>x &lt; y</code></td></tr></tbody></table>";
    let model = parse_config(
        &budget,
        source,
        ParseConfig::with_extensions(Profile::G3, Extensions::ALL),
    );
    let mut op = budget.begin(CancellationToken::new());
    let batch = project(&model, &mut op);
    bounds(&batch);
    assert_eq!(batch.blocks().len(), 1);
    assert_eq!(
        (batch.blocks()[0].columns, batch.blocks()[0].row_count),
        (2, 4)
    );
    assert_eq!(
        batch
            .cells()
            .iter()
            .map(|cell| (cell.colspan, cell.html_flags))
            .collect::<Vec<_>>(),
        [
            (2, 5),
            (0, 7),
            (1, 1),
            (1, 1),
            (2, 1),
            (0, 3),
            (1, 1),
            (1, 1)
        ]
    );
    assert_eq!(
        batch
            .cells()
            .iter()
            .map(|cell| cell.header)
            .collect::<Vec<_>>(),
        [false, false, true, true, false, false, false, false]
    );
    assert_eq!(
        batch
            .cells()
            .iter()
            .map(|cell| value(&batch, batch.texts()[cell.text as usize].value))
            .collect::<Vec<_>>(),
        [
            "HTML 表格与跨列",
            "",
            "列一",
            "列二",
            "跨两列，& 实体测试",
            "",
            "R2C1",
            "x < y"
        ]
    );
    assert_eq!(batch.cells()[0].alignment, Alignment::Center);
    for covered in [1usize, 5] {
        let cell = batch.cells()[covered];
        let text = batch.texts()[cell.text as usize];
        assert!(!cell.missing && !cell.header);
        assert_eq!(cell.alignment, Alignment::None);
        assert!(text.logical.is_empty());
        assert_eq!(text.utf16_len, 0);
        assert!(text.runs.is_empty() && text.images.is_empty() && text.derived.is_empty());
        assert!(
            !batch
                .script_scopes()
                .iter()
                .any(|scope| scope.text == cell.text)
        );
    }
    let last = batch.texts()[batch.cells()[7].text as usize];
    assert!(
        batch.runs()[last.runs.start()..last.runs.start() + last.runs.len()]
            .iter()
            .all(|run| run.flags & RUN_CODE != 0)
    );
    drop((batch, op, model));
    released(&budget);
}

#[test]
fn html_table_headerless_mixed_cells_inline_object_and_real_empty_body_are_projected() {
    use crate::config::Extensions;
    let budget = SourceBudget::preview();
    let source = "<table><caption></caption><tr><td><b>粗😀</b><br><code>*x*</code><a href='#z'>link</a></td><th><img src='x.png' alt='ALT中'></th></tr><tr><th></th><td></td></tr></table>";
    let model = parse_config(
        &budget,
        source,
        ParseConfig::with_extensions(Profile::G3, Extensions::ALL),
    );
    let mut op = budget.begin(CancellationToken::new());
    let batch = project(&model, &mut op);
    bounds(&batch);
    assert_eq!(
        batch
            .cells()
            .iter()
            .map(|cell| cell.header)
            .collect::<Vec<_>>(),
        [false, false, false, true, true, false]
    );
    assert_eq!(
        batch.cells()[0].html_flags,
        HTML_CELL_HTML | HTML_CELL_CAPTION
    );
    assert_eq!(
        batch.cells()[1].html_flags,
        HTML_CELL_HTML | HTML_CELL_COVERED | HTML_CELL_CAPTION
    );
    assert_eq!(
        value(&batch, batch.texts()[batch.cells()[2].text as usize].value),
        "粗😀\n*x*link"
    );
    assert_eq!(batch.images().len(), 1);
    assert_eq!(value(&batch, batch.images()[0].alt), "ALT中");
    assert_eq!(batch.links().len(), 1);
    assert_eq!(value(&batch, batch.links()[0].destination), "#z");
    for cell in &batch.cells()[4..] {
        assert_eq!(cell.html_flags, HTML_CELL_HTML);
        assert_eq!(cell.colspan, 1);
        let text = batch.texts()[cell.text as usize];
        assert!(text.logical.is_empty());
        assert_eq!(value(&batch, text.value), "");
    }
    drop((batch, op, model));
    released(&budget);
}

#[test]
fn html_table_gfm_and_front_matter_cells_default_to_span_one_without_html_flags() {
    use crate::config::Extensions;
    let budget = SourceBudget::preview();
    let source = "---\na: b\nc: d\n---\n\n| a | b |\n| --- | --- |\n| c | d |\n\n<table><tr><td>x</td><th>y</th></tr></table>";
    let model = parse_config(
        &budget,
        source,
        ParseConfig::with_extensions(Profile::G3, Extensions::ALL),
    );
    let mut op = budget.begin(CancellationToken::new());
    let batch = project(&model, &mut op);
    bounds(&batch);
    assert!(
        batch
            .cells()
            .iter()
            .filter(|cell| cell.html_flags == 0)
            .count()
            >= 4
    );
    for cell in batch.cells().iter().filter(|cell| cell.html_flags == 0) {
        assert_eq!(cell.colspan, 1);
    }
    assert_eq!(
        batch
            .cells()
            .iter()
            .filter(|cell| cell.html_flags == HTML_CELL_HTML)
            .count(),
        2
    );
    drop((batch, op, model));
    released(&budget);
}

#[test]
fn accepted_inline_styles_links_html_and_image_keep_exact_text_and_utf16_ranges() {
    let budget = SourceBudget::preview();
    let model = parse(
        &budget,
        "**bold *中😀*** ~~gone~~ `a  b` [link](https://example.test \"tip\") <i>x</i> ![alt *中*](img \"pic\")",
    );
    let mut op = budget.begin(CancellationToken::new());
    let batch = project(&model, &mut op);
    assert_eq!(batch.blocks().len(), 1);
    assert_eq!(batch.texts().len(), 1);
    let text = batch.texts()[0];
    assert_eq!(
        value(&batch, text.value),
        "bold 中😀 gone a  b link <i>x</i> \u{fffc}"
    );
    let mut actual = vec![None; text.utf16_len as usize];
    for run in batch.runs() {
        for slot in &mut actual[run.start as usize..run.end as usize] {
            assert!(slot.replace((run.flags, run.link.is_some())).is_none());
        }
    }
    let expected = [
        ("bold ", RUN_BOLD, false),
        ("中😀", RUN_BOLD | RUN_ITALIC, false),
        (" ", 0, false),
        ("gone", RUN_STRIKE, false),
        (" ", 0, false),
        ("a  b", RUN_CODE, false),
        (" ", 0, false),
        ("link", 0, true),
        (" ", 0, false),
        ("<i>", RUN_INERT_HTML, false),
        ("x", 0, false),
        ("</i>", RUN_INERT_HTML, false),
        (" \u{fffc}", 0, false),
    ]
    .into_iter()
    .flat_map(|(text, flags, link)| {
        std::iter::repeat_n(Some((flags, link)), text.encode_utf16().count())
    })
    .collect::<Vec<_>>();
    assert_eq!(actual, expected);
    assert!(
        batch.links().iter().all(
            |link| value(&batch, link.destination) == "https://example.test"
                && value(&batch, link.title) == "tip"
        )
    );
    assert_eq!(batch.images().len(), 1);
    let image = batch.images()[0];
    assert_eq!(value(&batch, image.destination), "img");
    assert_eq!(value(&batch, image.title), "pic");
    assert_eq!(value(&batch, image.alt), "alt 中");
    assert_eq!(
        image.offset_utf16 as usize,
        "bold 中😀 gone a  b link <i>x</i> ".encode_utf16().count()
    );
    bounds(&batch);
    drop((batch, op, model));
    released(&budget);
}

#[test]
fn link_atoms_share_one_payload_without_changing_utf16_styles_or_ranges() {
    let budget = SourceBudget::preview();
    let model = parse(
        &budget,
        "[中&amp;**文😀**](https://example.test/shared \"标题\")",
    );
    let mut op = budget.begin(CancellationToken::new());
    let batch = project(&model, &mut op);
    assert_eq!(batch.links().len(), 1);
    assert_eq!(batch.texts().len(), 1);
    let text = batch.texts()[0];
    assert_eq!(value(&batch, text.value), "中&文😀");
    let link = batch.links()[0];
    assert_eq!(
        value(&batch, link.destination),
        "https://example.test/shared"
    );
    assert_eq!(value(&batch, link.title), "标题");
    assert_eq!(
        batch.blob().len(),
        "中&文😀".len() + "https://example.test/shared".len() + "标题".len()
    );
    assert!(batch.runs().len() >= 3);
    let mut actual = vec![None; 5];
    for run in batch.runs() {
        assert_eq!(run.link, Some(0));
        for slot in &mut actual[run.start as usize..run.end as usize] {
            assert!(slot.replace(run.flags).is_none());
        }
    }
    assert_eq!(
        actual,
        [
            Some(0),
            Some(0),
            Some(RUN_BOLD),
            Some(RUN_BOLD),
            Some(RUN_BOLD)
        ]
    );
    bounds(&batch);
    drop((op, model));
    // The projected batch remains the final model owner, including its link.
    assert_eq!(value(&batch, batch.links()[0].title), "标题");
    drop(batch);
    released(&budget);
}

#[test]
fn link_memo_keeps_distinct_nodes_texts_and_batches_independent() {
    let budget = SourceBudget::preview();
    let model = parse(
        &budget,
        "[a&amp;*b*](same \"t\") [中**文**](same \"t\")\n\n[last](same \"t\")",
    );
    let mut op = budget.begin(CancellationToken::new());
    let whole = project(&model, &mut op);
    assert_eq!(whole.links().len(), 3);
    assert_eq!(whole.texts().len(), 2);
    for (index, link) in whole.links().iter().enumerate() {
        assert_eq!(value(&whole, link.destination), "same");
        assert_eq!(value(&whole, link.title), "t");
        assert!(
            whole.links()[..index]
                .iter()
                .all(|other| other.node != link.node)
        );
    }
    let first_text = whole.texts()[0];
    let second_text = whole.texts()[1];
    let first_runs = first_text.runs.slice(whole.runs()).unwrap();
    let second_runs = second_text.runs.slice(whole.runs()).unwrap();
    assert!(first_runs.iter().any(|run| run.link == Some(0)));
    assert!(first_runs.iter().any(|run| run.link == Some(1)));
    assert!(first_runs.iter().any(|run| run.link.is_none()));
    assert!(second_runs.iter().all(|run| run.link == Some(2)));
    assert_eq!(whole.blob().len(), "a&b 中文".len() + "last".len() + 3 * 5);
    bounds(&whole);

    let mut draft = draft(&model, &mut op);
    let key = draft.key();
    let source = draft.measurement_batch(1, &mut op).unwrap().unwrap();
    let node = source.requests[0].node;
    let first = PresentationBatch::build(model.clone(), &source, &mut op).unwrap();
    assert_eq!(first.links().len(), 2);
    draft
        .submit_measurements(
            key,
            0,
            &[crate::layout::MeasuredBlock { node, height: 20.0 }],
            &mut op,
        )
        .unwrap();
    let source = draft.measurement_batch(1, &mut op).unwrap().unwrap();
    let second = PresentationBatch::build(model.clone(), &source, &mut op).unwrap();
    assert_eq!(second.links().len(), 1);
    assert!(second.runs().iter().all(|run| run.link == Some(0)));
    assert_eq!(second.links()[0].node, whole.links()[2].node);
    assert_eq!(second.blob().len(), "last".len() + 5);
    bounds(&first);
    bounds(&second);
    drop((whole, first, second, draft, op, model));
    released(&budget);
}

#[test]
fn table_cells_preserve_alignment_missing_cells_and_independent_inline_styles() {
    let budget = SourceBudget::preview();
    let model = parse(
        &budget,
        "| **L** | R | C |\n| :--- | ---: | :---: |\n| *x* | y |\n",
    );
    let mut op = budget.begin(CancellationToken::new());
    let batch = project(&model, &mut op);
    assert_eq!(batch.blocks().len(), 1);
    let block = batch.blocks()[0];
    assert_eq!(block.request.kind, BlockKind::Table);
    assert_eq!((block.columns, block.row_count), (3, 2));
    assert_eq!(batch.cells().len(), 6);
    assert_eq!(
        batch
            .texts()
            .iter()
            .map(|t| value(&batch, t.value))
            .collect::<Vec<_>>(),
        ["L", "R", "C", "x", "y", ""]
    );
    assert_eq!(
        batch
            .cells()
            .iter()
            .map(|c| c.alignment)
            .collect::<Vec<_>>(),
        [
            Alignment::Left,
            Alignment::Right,
            Alignment::Center,
            Alignment::Left,
            Alignment::Right,
            Alignment::Center
        ]
    );
    assert_eq!(
        batch
            .cells()
            .iter()
            .map(|c| (c.row, c.column, c.header, c.missing))
            .collect::<Vec<_>>(),
        [
            (0, 0, true, false),
            (0, 1, true, false),
            (0, 2, true, false),
            (1, 0, false, false),
            (1, 1, false, false),
            (1, 2, false, true),
        ]
    );
    let first = batch.texts()[0];
    assert!(
        batch.runs()[first.runs.start()..first.runs.start() + first.runs.len()]
            .iter()
            .all(|r| r.flags == RUN_BOLD)
    );
    let x = batch.texts()[3];
    assert!(
        batch.runs()[x.runs.start()..x.runs.start() + x.runs.len()]
            .iter()
            .all(|r| r.flags == RUN_ITALIC)
    );
    for cell in batch.cells() {
        assert_eq!(
            batch.model().ir().nodes()[cell.node.index()].kind,
            NodeKind::TableCell
        );
        assert_eq!(
            batch.model().ir().nodes()[cell.node.index()].parent,
            Some(cell.row_node)
        );
    }
    bounds(&batch);
    drop((batch, op, model));
    released(&budget);
}

#[test]
fn ordered_numbers_task_objects_and_nested_empty_items_are_not_flattened() {
    let budget = SourceBudget::preview();
    let model = parse(&budget, "3. first\n9. second\n\n- [x] \n\n  - child\n");
    let mut op = budget.begin(CancellationToken::new());
    let batch = project(&model, &mut op);
    assert_eq!(
        batch
            .blocks()
            .iter()
            .map(|b| b.request.kind)
            .collect::<Vec<_>>(),
        [
            BlockKind::Paragraph,
            BlockKind::Paragraph,
            BlockKind::EmptyListItem,
            BlockKind::Paragraph
        ]
    );
    assert_eq!(
        batch
            .texts()
            .iter()
            .map(|t| value(&batch, t.value))
            .collect::<Vec<_>>(),
        ["first", "second", "child"]
    );
    assert_eq!(
        batch
            .decorations()
            .iter()
            .map(|d| (d.kind, d.ordinal, d.depth))
            .collect::<Vec<_>>(),
        [
            (DecorationKind::Ordered, 3, 1),
            (DecorationKind::Ordered, 4, 1),
            (DecorationKind::Task, 0, 1),
            (DecorationKind::Bullet, 0, 2),
        ]
    );
    let task = batch.decorations()[2];
    assert!(task.checked && task.task.is_some());
    assert_eq!(task.action.unwrap().expected, *b"[x]");
    assert!(batch.blocks()[2].texts.is_empty());
    assert!(
        !batch.blob().contains(&0xef),
        "no duplicate U+FFFC task text"
    );
    bounds(&batch);
    drop((batch, op, model));
    released(&budget);
}

#[test]
fn item_decoration_is_once_even_when_visible_batch_starts_on_later_paragraph() {
    let budget = SourceBudget::preview();
    let model = parse(&budget, "- [x] first\n\n  second\n");
    let mut op = budget.begin(CancellationToken::new());
    let mut draft = draft(&model, &mut op);
    let source = draft.measurement_batch(256, &mut op).unwrap().unwrap();
    assert_eq!(source.requests.len(), 2);
    let batch = PresentationBatch::build(model.clone(), &source, &mut op).unwrap();
    assert_eq!(batch.decorations().len(), 1);
    assert_eq!(batch.blocks()[0].decorations.len(), 1);
    assert!(batch.blocks()[1].decorations.is_empty());
    let later = MeasurementBatch {
        key: source.key,
        start: 1,
        requests: &source.requests[1..],
        scope: source.scope,
    };
    let later = PresentationBatch::build(model.clone(), &later, &mut op).unwrap();
    assert_eq!(later.start(), 1);
    assert!(later.decorations().is_empty());
    assert_eq!(value(&later, later.texts()[0].value), "second");
    bounds(&batch);
    bounds(&later);
    drop((later, batch, draft, op, model));
    released(&budget);
}

#[test]
fn static_html_checkboxes_never_export_synthetic_gfm_edit_actions() {
    use crate::config::Extensions;
    use crate::ir::ObjectRow;
    let budget = SourceBudget::preview();
    let input = "<label><input type=checkbox checked disabled> 中😀 <b>bold</b> <code>*literal*</code></label>\n\n<label><input type=checkbox disabled> unchecked</label>\n\n- [x] editable\n";
    let model = parse_config(
        &budget,
        input,
        ParseConfig::with_extensions(Profile::G3, Extensions::HTML_BLOCKS),
    );
    // The independently valid synthetic marker is allowed to exist in IR;
    // only an original authored GFM marker authorizes the presentation action.
    assert!(
        model
            .ir()
            .objects()
            .iter()
            .filter(|object| matches!(
                object,
                ObjectRow::Task {
                    action: Some(_),
                    ..
                }
            ))
            .count()
            == 3
    );
    let mut op = budget.begin(CancellationToken::new());
    let batch = project(&model, &mut op);
    let decorations = batch.decorations();
    assert_eq!(decorations.len(), 3);
    assert_eq!(
        decorations
            .iter()
            .map(|value| value.kind)
            .collect::<Vec<_>>(),
        [
            DecorationKind::StaticHtmlCheckbox,
            DecorationKind::StaticHtmlCheckbox,
            DecorationKind::Task
        ]
    );
    assert!(decorations[0].checked);
    assert!(!decorations[1].checked);
    for value in &decorations[..2] {
        assert!(value.task.is_some());
        assert!(value.action.is_none());
        assert_eq!(value.logical.unwrap().len(), 1);
    }
    assert_eq!(decorations[2].action.unwrap().expected, *b"[x]");
    let marker = decorations[2].action.unwrap().source;
    let original_start =
        model.original_source_byte(marker.start(), crate::mapping::Affinity::After);
    let original_end = model.original_source_byte(marker.end(), crate::mapping::Affinity::Before);
    assert_eq!(
        &input[original_start as usize..original_end as usize],
        "[x]"
    );
    assert_eq!(
        batch
            .texts()
            .iter()
            .map(|text| value(&batch, text.value))
            .collect::<Vec<_>>(),
        ["中😀 bold *literal*", "unchecked", "editable"]
    );
    assert!(batch.runs().iter().any(|run| run.flags & RUN_BOLD != 0));
    assert!(batch.runs().iter().any(|run| run.flags & RUN_CODE != 0));
    bounds(&batch);
    drop((batch, op, model));
    released(&budget);
}

#[test]
fn standalone_empty_task_before_later_body_is_still_an_external_decoration() {
    let budget = SourceBudget::preview();
    let model = parse(&budget, "- [x] \n\n  later\n");
    let mut op = budget.begin(CancellationToken::new());
    let batch = project(&model, &mut op);
    assert_eq!(batch.blocks().len(), 1);
    assert_eq!(value(&batch, batch.texts()[0].value), "later");
    assert_eq!(batch.decorations()[0].kind, DecorationKind::Task);
    assert!(batch.decorations()[0].checked);
    bounds(&batch);
    drop((batch, op, model));
    released(&budget);
}

#[test]
fn heading_code_info_raw_whitespace_and_inert_html_are_kept() {
    let budget = SourceBudget::preview();
    let model = parse(
        &budget,
        "### title\n\n```rust\na  b\nc\n```\n\n<div>raw</div>\n\n---\n",
    );
    let mut op = budget.begin(CancellationToken::new());
    let batch = project(&model, &mut op);
    assert_eq!(
        batch
            .blocks()
            .iter()
            .map(|b| b.request.kind)
            .collect::<Vec<_>>(),
        [
            BlockKind::Heading,
            BlockKind::CodeBlock,
            BlockKind::HtmlBlock,
            BlockKind::ThematicBreak
        ]
    );
    assert_eq!(batch.blocks()[0].heading_level, 3);
    assert_eq!(value(&batch, batch.blocks()[1].code_info), "rust");
    assert_eq!(
        batch
            .texts()
            .iter()
            .map(|t| value(&batch, t.value))
            .collect::<Vec<_>>(),
        ["title", "a  b\nc\n", "<div>raw</div>\n"]
    );
    let code = batch.texts()[1];
    let html = batch.texts()[2];
    assert!(
        batch.runs()[code.runs.start()..code.runs.start() + code.runs.len()]
            .iter()
            .all(|r| r.flags == RUN_CODE)
    );
    assert!(
        batch.runs()[html.runs.start()..html.runs.start() + html.runs.len()]
            .iter()
            .all(|r| r.flags == RUN_INERT_HTML)
    );
    assert!(batch.blocks()[3].texts.is_empty());
    bounds(&batch);
    drop((batch, op, model));
    released(&budget);
}

#[test]
fn logical_range_reader_uses_utf16_not_source_bytes_and_rejects_surrogate_cuts() {
    let budget = SourceBudget::preview();
    let model = parse(&budget, "A😀中 &amp; e\u{301}");
    for (start, end, expected) in [(1, 3, "😀"), (3, 4, "中"), (4, 6, " &"), (0, 0, "")] {
        let mut op = budget.begin(CancellationToken::new());
        let mut actual = String::new();
        crate::reader::text::visit(
            &model,
            model.main_flow_id(),
            LogicalSpan::new(start, end).unwrap(),
            &mut op,
            |part, _| {
                actual.push_str(part);
                Ok(())
            },
        )
        .unwrap();
        assert_eq!(actual, expected);
    }
    for (start, end) in [(2, 3), (1, 2), (2, 2), (0, 100)] {
        let mut op = budget.begin(CancellationToken::new());
        let result = crate::reader::text::visit(
            &model,
            model.main_flow_id(),
            LogicalSpan::new(start, end).unwrap(),
            &mut op,
            |_, _| Ok(()),
        );
        assert!(matches!(
            result,
            Err(CoreError::InvalidBoundary | CoreError::InvalidRange)
        ));
    }
    drop(model);
    released(&budget);
}

#[test]
fn later_main_range_has_local_runs_and_batch_start_without_prefix_text() {
    let budget = SourceBudget::preview();
    let model = parse(&budget, "first\n\nlater **😀**\n");
    let mut op = budget.begin(CancellationToken::new());
    let mut draft = draft(&model, &mut op);
    let source = draft.measurement_batch(256, &mut op).unwrap().unwrap();
    let later = MeasurementBatch {
        key: source.key,
        start: 1,
        requests: &source.requests[1..],
        scope: source.scope,
    };
    let batch = PresentationBatch::build(model.clone(), &later, &mut op).unwrap();
    assert_eq!(batch.start(), 1);
    assert_eq!(value(&batch, batch.texts()[0].value), "later 😀");
    assert!(batch.texts()[0].logical.start() > 0);
    assert_eq!(
        batch
            .runs()
            .iter()
            .map(|r| (r.start, r.end, r.flags))
            .collect::<Vec<_>>(),
        [(0, 6, 0), (6, 8, RUN_BOLD)]
    );
    bounds(&batch);
    drop((batch, draft, op, model));
    released(&budget);
}

#[test]
fn empty_batch_empty_item_and_definition_only_source_are_valid() {
    for input in ["", "[r]: /dest\n", "-\n"] {
        let budget = SourceBudget::preview();
        let model = parse(&budget, input);
        let mut op = budget.begin(CancellationToken::new());
        let batch = project(&model, &mut op);
        assert!(batch.texts().is_empty());
        assert!(batch.blob().is_empty());
        assert_eq!(batch.decorations().len(), usize::from(input == "-\n"));
        bounds(&batch);
        drop((batch, op, model));
        released(&budget);
    }
}

#[test]
fn wrong_model_budget_oversized_batch_and_cancellation_never_return_projection() {
    let budget = SourceBudget::preview();
    let model = parse(&budget, "body");
    let cancel = CancellationToken::new();
    let mut op = budget.begin(cancel.clone());
    let mut draft = draft(&model, &mut op);
    let source = draft.measurement_batch(256, &mut op).unwrap().unwrap();
    let other = SourceBudget::preview();
    let mut wrong = other.begin(CancellationToken::new());
    assert!(matches!(
        PresentationBatch::build(model.clone(), &source, &mut wrong),
        Err(CoreError::WrongBudget)
    ));
    let many = vec![source.requests[0]; 257];
    let many = MeasurementBatch {
        key: source.key,
        start: 0,
        requests: &many,
        scope: None,
    };
    let mut invalid = budget.begin(cancel.clone());
    assert!(matches!(
        PresentationBatch::build(model.clone(), &many, &mut invalid),
        Err(CoreError::InvalidRange)
    ));
    let stale_key = LayoutKey::new(
        crate::identity::ModelIdentity::new(
            SourceKey::new([1; 16], 2).unwrap(),
            9,
            ParseConfig::new(Profile::G3),
        )
        .unwrap(),
        1,
        2,
        3,
        800.0,
        source.key.style(),
    )
    .unwrap();
    let stale = MeasurementBatch {
        key: stale_key,
        start: 0,
        requests: source.requests,
        scope: source.scope,
    };
    let mut invalid = budget.begin(cancel.clone());
    assert!(matches!(
        PresentationBatch::build(model.clone(), &stale, &mut invalid),
        Err(CoreError::StaleResult)
    ));
    op.fail(CoreError::InvalidRange);
    cancel.cancel();
    assert!(matches!(
        PresentationBatch::build(model.clone(), &source, &mut op),
        Err(CoreError::InvalidRange)
    ));
    let mut canceled = budget.begin(cancel);
    assert!(matches!(
        PresentationBatch::build(model.clone(), &source, &mut canceled),
        Err(CoreError::Cancelled)
    ));
    drop((canceled, invalid, wrong, draft, op, model));
    released(&budget);
    released(&other);
}

#[test]
fn quota_failure_unwinds_partial_tables_and_batch_pin_releases_last_model() {
    let budget = SourceBudget::preview();
    let model = parse(&budget, "body");
    let mut op = budget.begin(CancellationToken::new());
    let mut draft = draft(&model, &mut op);
    let source = draft.measurement_batch(256, &mut op).unwrap().unwrap();
    let spare = budget.limits().auxiliary_capacity - budget.usage().auxiliary_capacity;
    let held = op
        .reserve_boundary(spare - (size_of::<Block>() + size_of::<Text>()) as u64)
        .unwrap();
    let before = budget.usage().auxiliary_capacity;
    assert!(matches!(
        PresentationBatch::build(model.clone(), &source, &mut op),
        Err(CoreError::ResourceLimit { .. })
    ));
    assert_eq!(budget.usage().auxiliary_capacity, before);
    drop((held, draft, op));
    let mut op = budget.begin(CancellationToken::new());
    let batch = project(&model, &mut op);
    drop(model);
    assert_eq!(batch.model().source().text(), "body");
    assert_eq!(value(&batch, batch.texts()[0].value), "body");
    drop((batch, op));
    released(&budget);
}

#[test]
fn final_build_checkpoint_failure_releases_every_unpublished_table() {
    let budget = SourceBudget::preview();
    let model = parse(&budget, "plain **bold**\n");
    let cancel = CancellationToken::new();
    let mut setup = budget.begin(cancel.clone());
    let mut draft = draft(&model, &mut setup);
    let source = draft.measurement_batch(256, &mut setup).unwrap().unwrap();
    let mut reference = budget.begin(cancel.clone());
    reference.set_clock_samples_for_test(&[4_000_000_000]);
    let batch = PresentationBatch::build(model.clone(), &source, &mut reference).unwrap();
    let checks = reference.clock_read_count_for_test();
    assert!(checks > 8);
    drop((batch, reference));
    let mut failure = budget.begin(cancel);
    let mut samples = vec![4_000_000_000; checks];
    samples[checks - 1] = 5_000_000_000;
    failure.set_clock_samples_for_test(&samples);
    let before = budget.usage().auxiliary_capacity;
    assert!(matches!(
        PresentationBatch::build(model.clone(), &source, &mut failure),
        Err(CoreError::DeadlineExceeded { .. })
    ));
    assert_eq!(failure.clock_read_count_for_test(), checks);
    assert_eq!(budget.usage().auxiliary_capacity, before);
    drop((failure, draft, setup, model));
    released(&budget);
}
