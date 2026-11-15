// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE file in the project root for license information.
// ---------------------------------------------------------------------------------------------

use super::*;
use crate::config::{Extensions, ParseConfig, Profile};
use crate::identity::SourceKey;
use crate::policy::{CancellationToken, SourceBudget, SourceLimits};
use crate::source::SourceDocument;

fn parse(budget: &SourceBudget, input: &str, extensions: Extensions) -> AcceptedModel {
    let mut op = budget.begin(CancellationToken::new());
    let source = SourceDocument::new(
        SourceKey::new([110; 16], 1).unwrap(),
        input.as_bytes(),
        &mut op,
    )
    .unwrap();
    AcceptedModel::parse(
        source.snapshot(),
        ParseConfig::with_extensions(Profile::G3, extensions),
        1,
        &mut op,
    )
    .unwrap()
}

fn build(input: &str) -> Outline {
    let budget = SourceBudget::model_fixture();
    let model = parse(&budget, input, Extensions::NONE);
    let mut op = budget.begin(CancellationToken::new());
    Outline::build(model, &mut op).unwrap()
}

fn value(outline: &Outline, range: TextRange) -> &str {
    std::str::from_utf8(
        &outline.blob()[range.start as usize..range.start as usize + range.len as usize],
    )
    .unwrap()
}

fn titles(outline: &Outline) -> Vec<&str> {
    outline
        .headings()
        .iter()
        .map(|h| value(outline, h.title))
        .collect()
}
fn anchors(outline: &Outline) -> Vec<&str> {
    outline
        .headings()
        .iter()
        .map(|h| value(outline, h.anchor))
        .collect()
}

#[test]
fn html_targets_keep_exact_names_and_do_not_add_headings() {
    let source = "# Duplicate\r\n\r\nBefore 😀\r\n\r\n<A NAME='Legacy'></A>\r\n\r\n\
        <p id='duplicate'>Next paragraph</p>\r\n\r\n\
        <h2 id='中文&amp;Exact'>Literal {#safe}</h2>\r\n\r\n\
        `<a id='code'></a>`\r\n\r\n```html\r\n<a id='fenced'></a>\r\n```\r\n";
    let budget = SourceBudget::preview();
    let extensions =
        Extensions::from_bits(Extensions::HTML_BLOCKS.bits() | Extensions::HTML_INLINE.bits())
            .unwrap();
    let model = parse(&budget, source, extensions);
    let mut op = budget.begin(CancellationToken::new());
    let outline = Outline::build(model.clone(), &mut op).unwrap();

    assert_eq!(model.original_source().text(), source);
    assert_eq!(titles(&outline), ["Duplicate", "Literal {#safe}"]);
    assert_eq!(anchors(&outline), ["duplicate-1", "中文&Exact"]);
    assert_eq!(outline.anchors()[2].node, Some(outline.headings()[1].node));
    let targets = outline
        .anchors()
        .iter()
        .map(|row| (value(&outline, row.name), row.legacy))
        .collect::<Vec<_>>();
    assert_eq!(
        targets,
        [
            ("Legacy", true),
            ("duplicate", false),
            ("中文&Exact", false)
        ]
    );

    let flow = model.main_flow_id();
    let end = model.ir().flows()[flow.index()].logical_utf16;
    let mut visible = String::new();
    crate::reader::text::visit(
        &model,
        flow,
        LogicalSpan::new(0, end).unwrap(),
        &mut op,
        |text, _| {
            visible.push_str(text);
            Ok(())
        },
    )
    .unwrap();
    assert!(!visible.contains("NAME='Legacy'"));
    for (row, text) in outline
        .anchors()
        .iter()
        .skip(1)
        .zip(["Next paragraph", "Literal {#safe}"])
    {
        let offset = visible[..visible.find(text).unwrap()]
            .encode_utf16()
            .count();
        assert_eq!(row.position as usize, offset, "{text:?} in {visible:?}");
    }
}

#[test]
fn empty_html_heading_ids_resolve_to_distinct_blocks() {
    let source = "<h2 id='first'></h2>\n\n<h2 id='second'></h2>\n\nAfter.\n";
    let budget = SourceBudget::preview();
    let model = parse(&budget, source, Extensions::ALL);
    let mut op = budget.begin(CancellationToken::new());
    let outline = Outline::build(model, &mut op).unwrap();
    let headings = outline.headings();
    assert_eq!(anchors(&outline), ["first", "second"]);
    assert_ne!(headings[0].node, headings[1].node);
    assert_eq!(headings[0].logical, headings[1].logical);
    assert_eq!(outline.anchors()[0].node, Some(headings[0].node));
    assert_eq!(outline.anchors()[1].node, Some(headings[1].node));
}

#[test]
fn inline_anchor_entities_can_expand_beyond_their_authored_byte_length() {
    let budget = SourceBudget::preview();
    let source = "<span id='&nGt;&nGt;&nGt;'>x</span> <a name='&nLt;&nLt;&nLt;'>y</a>";
    let model = parse(&budget, source, Extensions::HTML_INLINE);
    let mut op = budget.begin(CancellationToken::new());
    let outline = Outline::build(model, &mut op).unwrap();
    assert!(outline.headings().is_empty());
    let names = outline
        .anchors()
        .iter()
        .map(|anchor| (value(&outline, anchor.name), anchor.legacy))
        .collect::<Vec<_>>();
    assert_eq!(
        names,
        [
            ("≫\u{20d2}≫\u{20d2}≫\u{20d2}", false),
            ("≪\u{20d2}≪\u{20d2}≪\u{20d2}", true)
        ]
    );
}

#[test]
fn inline_targets_preserve_markdown_and_keep_code_inert() {
    let source = "Before <a name='legacy' id='A&amp;中文'>**bold**</a> and \
        <span id=outer>span</span> with <strong id='long-id-abcdefghijklmnopqrstuvwxyz'>strong</strong>.\n\n\
        `<a id='inline-code'></a>` <code><a name='html-code'></a></code>\n\n\
        <a id='unmatched'>Literal malformed target.\n";
    let budget = SourceBudget::preview();
    let model = parse(&budget, source, Extensions::ALL);
    let mut op = budget.begin(CancellationToken::new());
    let outline = Outline::build(model.clone(), &mut op).unwrap();
    let selected = outline
        .anchors()
        .iter()
        .map(|anchor| (value(&outline, anchor.name), anchor.position, anchor.legacy))
        .collect::<Vec<_>>();
    assert_eq!(
        selected,
        [
            ("A&中文", 7, false),
            ("legacy", 7, true),
            ("outer", 16, false),
            ("long-id-abcdefghijklmnopqrstuvwxyz", 26, false)
        ]
    );
    let flow = model.main_flow_id();
    let end = model.ir().flows()[flow.index()].logical_utf16;
    let mut preview = String::new();
    crate::reader::text::visit(
        &model,
        flow,
        LogicalSpan::new(0, end).unwrap(),
        &mut op,
        |text, _| {
            preview.push_str(text);
            Ok(())
        },
    )
    .unwrap();
    assert!(preview.starts_with("Before bold and span with strong."));
    assert!(preview.contains("<a id='inline-code'></a>"));
    assert!(preview.contains("<a name='html-code'></a>"));
    assert!(preview.contains("<a id='unmatched'>"));
    assert_eq!(model.original_source().text(), source);
}

#[test]
fn html_table_and_rule_ids_keep_block_targets_and_cell_labels() {
    let source = "Intro\n\n<hr id='rule'>\n\n\
        <table id='table'><caption id='caption'>Caption</caption>\
        <thead id='group'><tr id='row'><th id='cell'><a name='inline'></a>\
        <strong id='emphasis'>Header</strong></th></tr></thead>\
        <tbody><tr><td>Body</td></tr></tbody></table>\n";
    let budget = SourceBudget::preview();
    let model = parse(&budget, source, Extensions::ALL);
    let mut op = budget.begin(CancellationToken::new());
    let outline = Outline::build(model.clone(), &mut op).unwrap();
    let targets: Vec<_> = outline
        .anchors()
        .iter()
        .map(|anchor| {
            (
                value(&outline, anchor.name),
                anchor
                    .node
                    .map(|node| model.ir().nodes()[node.index()].kind),
                anchor.legacy,
            )
        })
        .collect();
    assert_eq!(
        targets,
        [
            ("rule", Some(NodeKind::ThematicBreak), false),
            ("table", Some(NodeKind::Table), false),
            ("caption", None, false),
            ("group", None, false),
            ("row", None, false),
            ("cell", None, false),
            ("inline", None, true),
            ("emphasis", None, false),
        ]
    );
    assert!(outline.headings().is_empty());
}

#[test]
fn explicit_heading_ids_use_the_native_heading_scope_and_preserve_the_original_source() {
    let source = "# exact\r\n\r\n## Styled *title* {#exact} ##\r\n\r\n\
        > Inside quote\r\n> with continuation {#中.Case}\r\n> =====\r\n\r\n\
        - ### Inside list {#list:anchor}\r\n\r\n\
        {#empty}\r\n------\r\n\r\n\
        ## `code {#code}`\r\n\r\n## Escaped \\{#escaped}\r\n\r\n\
        ## Duplicate manual {#exact}\r\n\r\n## Entity &#123;#entity}\r\n\r\n\
        <h2 id='native-html'>HTML {#literal}</h2>\r\n";
    let budget = SourceBudget::preview();
    let model = parse(&budget, source, Extensions::ALL);
    let mut op = budget.begin(CancellationToken::new());
    let outline = Outline::build(model.clone(), &mut op).unwrap();
    assert_eq!(model.original_source().text(), source);
    assert_eq!(
        titles(&outline),
        [
            "exact",
            "Styled title",
            "Inside quote with continuation",
            "Inside list",
            "",
            "code {#code}",
            "Escaped {#escaped}",
            "Duplicate manual",
            "Entity {#entity}",
            "HTML {#literal}",
        ]
    );
    assert_eq!(
        anchors(&outline),
        [
            "exact-1",
            "exact",
            "中.Case",
            "list:anchor",
            "empty",
            "code-code",
            "escaped-escaped",
            "exact",
            "entity-entity",
            "native-html",
        ]
    );
    assert_eq!(value(&outline, outline.anchors()[0].name), "native-html");
    let literal = build("## Unextended {#identifier}");
    assert_eq!(titles(&literal), ["Unextended {#identifier}"]);
}

#[test]
fn upstream_header_id_prefix_six_levels_exact_expected() {
    // Independent expected values from pinned Comrak src/tests/header_id_prefix.rs,
    // original hrefs (without user-content prefix), not generated by this builder.
    let outline =
        build("# Hi.\n## Hi 1.\n### Hi.\n#### Hello.\n##### Hi.\n###### Hello.\n# Isn't it grand?");
    assert_eq!(
        titles(&outline),
        [
            "Hi.",
            "Hi 1.",
            "Hi.",
            "Hello.",
            "Hi.",
            "Hello.",
            "Isn't it grand?"
        ]
    );
    assert_eq!(
        anchors(&outline),
        [
            "hi",
            "hi-1",
            "hi-2",
            "hello",
            "hi-3",
            "hello-1",
            "isnt-it-grand"
        ]
    );
    assert_eq!(
        outline
            .headings()
            .iter()
            .map(|h| h.level)
            .collect::<Vec<_>>(),
        [1, 2, 3, 4, 5, 6, 1]
    );
}

#[test]
fn upstream_literal_suffix_collisions_are_global() {
    let outline = build("# a\n# a\n# a-1\n# a\n");
    assert_eq!(anchors(&outline), ["a", "a-1", "a-1-1", "a-2"]);
    assert_eq!(titles(&outline), ["a", "a", "a-1", "a"]);
}

#[test]
fn heading_anchors_trim_rendered_edge_whitespace_before_deduplication() {
    let outline = build(
        "# &nbsp;&#32;Edge  *Space*&#32;&nbsp;&#32;\r\n\
         # Edge  Space\r\n\
         # Edge  Space-1\r\n\
         # &emsp;&#32;Edge  Space&#32;&thinsp;\r\n\
         # &nbsp;&#32;边缘空白&#32;\r\n\
         # &#32;! Keep &nbsp;Inner !&#32;\r\n\
         # &#32;&nbsp;&#32;\r\n\
         #\r\n",
    );
    assert_eq!(
        titles(&outline),
        [
            "\u{a0} Edge  Space \u{a0} ",
            "Edge  Space",
            "Edge  Space-1",
            "\u{2003} Edge  Space \u{2009}",
            "\u{a0} 边缘空白 ",
            " ! Keep \u{a0}Inner ! ",
            " \u{a0} ",
            "",
        ]
    );
    assert_eq!(
        anchors(&outline),
        [
            "edge--space",
            "edge--space-1",
            "edge--space-1-1",
            "edge--space-2",
            "边缘空白",
            "-keep-inner-",
            "",
            "-1",
        ]
    );
}

#[test]
fn all_nested_atx_and_setext_headings_in_document_order() {
    let outline = build("# first\n\n> ### quoted\n\n- ## listed\n\nsetext\n======\n\nlast\n----\n");
    assert_eq!(
        titles(&outline),
        ["first", "quoted", "listed", "setext", "last"]
    );
    assert_eq!(
        outline
            .headings()
            .iter()
            .map(|h| h.level)
            .collect::<Vec<_>>(),
        [1, 3, 2, 1, 2]
    );
    for h in outline.headings() {
        let node = outline.model().ir().nodes()[h.node.index()];
        assert_eq!(node.kind, NodeKind::Heading);
        assert_eq!(node.flow, outline.model().main_flow_id());
        assert_eq!(node.logical, h.logical);
    }
}

#[test]
fn label_is_plain_semantics_with_entity_code_link_alt_and_html() {
    let outline = build("# A &amp; *B* `C` [D](url) ![E *F*](img) <i>G</i>\n");
    assert_eq!(titles(&outline), ["A & B C D E F G"]);
    assert_eq!(anchors(&outline), ["a--b-c-d-e-f-g"]);
    assert!(!value(&outline, outline.headings()[0].title).contains('\u{fffc}'));
}

#[test]
fn alt_flow_bridges_include_nested_images_without_recursion() {
    let outline = build("# ![outer ![inner](i) tail](o)\n");
    assert_eq!(titles(&outline), ["outer inner tail"]);
    assert_eq!(anchors(&outline), ["outer-inner-tail"]);
}

#[test]
fn math_title_uses_input_and_disabled_math_preserves_text() {
    let budget = SourceBudget::model_fixture();
    let model = parse(&budget, "# Math $x^2$ and \\(y+1\\)\n", Extensions::MATH);
    let mut op = budget.begin(CancellationToken::new());
    let outline = Outline::build(model, &mut op).unwrap();
    assert_eq!(titles(&outline), ["Math x^2 and y+1"]);
    assert_eq!(anchors(&outline), ["math-x2-and-y1"]);
    let plain = build("# Math $x^2$\n");
    assert_eq!(titles(&plain), ["Math $x^2$"]);
}

#[test]
fn soft_and_hard_break_labels_are_spaces() {
    let soft = build("a\nb\n---\n");
    assert_eq!(titles(&soft), ["a b"]);
    let hard = build("a  \nb\n---\n");
    assert_eq!(titles(&hard), ["a b"]);
}

#[test]
fn empty_headings_keep_distinct_nodes_and_empty_anchor_rules() {
    let outline = build("#\n\n##\n\n# !!!\n\n# -1\n");
    assert_eq!(titles(&outline), ["", "", "!!!", "-1"]);
    assert_eq!(anchors(&outline), ["", "-1", "-2", "-1-1"]);
    let first = outline.headings()[0];
    let second = outline.headings()[1];
    assert!(first.logical.is_empty() && second.logical.is_empty());
    assert_ne!(first.node, second.node);
    assert_eq!(first.logical, second.logical);
}

#[test]
fn unicode_main_coordinates_are_utf16_and_newlines_are_equivalent() {
    for separator in ["\n", "\r", "\r\n"] {
        let outline = build(&format!("# A😀中{separator}{separator}## B{separator}"));
        assert_eq!(titles(&outline), ["A😀中", "B"]);
        assert_eq!(anchors(&outline), ["a中", "b"]);
        assert_eq!(
            outline.headings()[0].logical,
            LogicalSpan::new(0, 4).unwrap()
        );
        assert_eq!(outline.headings()[1].logical.len(), 1);
    }
}

#[test]
fn unicode_anchorizer_independent_differential_including_final_sigma() {
    let names = [
        "ΟΣ",
        "Σ",
        "Ο'Σ",
        "İ",
        "AΣ\u{10fc}Σ",
        "中文 Text",
        "e\u{301}",
        "É",
        "٣_Ⅳ",
        "שלום العربية",
        "😀 ☃",
        "a\u{200d}b",
        "𐐀𐐁",
        "A-1",
        "ΟΣ",
        "İ",
    ];
    let source = names.iter().map(|v| format!("# {v}\n")).collect::<String>();
    let outline = build(&source);
    let mut reference = comrak::Anchorizer::new();
    assert_eq!(outline.headings().len(), names.len());
    for (h, expected_title) in outline.headings().iter().zip(names) {
        assert_eq!(value(&outline, h.title), expected_title);
        assert_eq!(
            value(&outline, h.anchor),
            reference.anchorize(expected_title)
        );
    }
    assert_eq!(value(&outline, outline.headings()[0].anchor), "ος");
}

#[test]
fn numbering_does_not_restart_at_32_or_256_heading_boundaries() {
    let mut source = "# a\n".repeat(600);
    source.push_str("# a-1\n# a\n");
    let outline = build(&source);
    assert_eq!(outline.headings().len(), 602);
    for i in 1..600 {
        assert_eq!(
            value(&outline, outline.headings()[i].anchor),
            format!("a-{i}")
        );
    }
    assert_eq!(value(&outline, outline.headings()[600].anchor), "a-1-1");
    assert_eq!(value(&outline, outline.headings()[601].anchor), "a-600");
}

#[test]
fn empty_document_and_no_heading_document_have_empty_catalogs() {
    for source in ["", "a plain paragraph\n", "```\n# not a heading\n```\n"] {
        let outline = build(source);
        assert!(outline.headings().is_empty());
        assert!(outline.blob().is_empty());
    }
}

#[test]
fn temporary_and_final_storage_refund_to_original_model_usage() {
    let budget = SourceBudget::model_fixture();
    let model = parse(&budget, &"# same\n".repeat(80), Extensions::NONE);
    let before = budget.usage().auxiliary_capacity;
    let mut op = budget.begin(CancellationToken::new());
    let outline = Outline::build(model.clone(), &mut op).unwrap();
    // Only the two final paid containers remain, never the two hash tables,
    // lower scratch, base pool or candidate scratch.
    assert_eq!(
        budget.usage().auxiliary_capacity - before,
        (outline.headings.values.capacity() * size_of::<Heading>() + outline.blob.values.capacity())
            as u64
    );
    drop(outline);
    assert_eq!(budget.usage().auxiliary_capacity, before);
}

#[test]
fn quota_failure_is_atomic_and_new_operation_can_recover() {
    let budget = SourceBudget::model_fixture();
    let model = parse(&budget, "# Heading\n", Extensions::NONE);
    let before = budget.usage().auxiliary_capacity;
    let held = budget
        .reserve_resource_capacity(budget.limits().auxiliary_capacity - before - 1)
        .unwrap();
    let occupied = budget.usage().auxiliary_capacity;
    let mut op = budget.begin(CancellationToken::new());
    let error = Outline::build(model.clone(), &mut op).err().unwrap();
    assert!(matches!(error, CoreError::ResourceLimit { .. }));
    assert_eq!(budget.usage().auxiliary_capacity, occupied);
    assert_eq!(op.first_fault(), Some(error));
    drop(held);
    assert_eq!(budget.usage().auxiliary_capacity, before);
    let mut next = budget.begin(CancellationToken::new());
    assert_eq!(
        titles(&Outline::build(model, &mut next).unwrap()),
        ["Heading"]
    );
}

#[test]
fn cancelled_and_expired_operations_cannot_publish_partial_catalogs() {
    let budget = SourceBudget::preview();
    let model = parse(&budget, "# One\n# Two\n", Extensions::NONE);
    let before = budget.usage().auxiliary_capacity;
    let flag = CancellationToken::new();
    flag.cancel();
    let mut cancelled = budget.begin(flag);
    assert_eq!(
        Outline::build(model.clone(), &mut cancelled).err(),
        Some(CoreError::Cancelled)
    );
    assert_eq!(budget.usage().auxiliary_capacity, before);
    let mut expired = budget.begin(CancellationToken::new());
    expired.set_clock_samples_for_test(&[5_000_000_000]);
    assert!(matches!(
        Outline::build(model.clone(), &mut expired),
        Err(CoreError::DeadlineExceeded { .. })
    ));
    assert_eq!(budget.usage().auxiliary_capacity, before);
}

#[test]
fn lowercase_low_work_never_enters_std_and_keeps_first_fault() {
    let budget = SourceBudget::new(SourceLimits {
        max_work: 100,
        ..SourceLimits::default()
    })
    .unwrap();
    let mut op = budget.begin(CancellationToken::new());
    let calls = lowercase::calls().0;
    let error = Lowercase::new("long single title", false, &mut op)
        .err()
        .unwrap();
    assert!(matches!(
        error,
        CoreError::ResourceLimit {
            kind: LimitKind::Work,
            ..
        }
    ));
    assert_eq!(lowercase::calls().0, calls);
    assert_eq!(op.first_fault(), Some(error));
    assert_eq!(budget.usage().auxiliary_capacity, 0);
}

#[test]
fn lowercase_precharges_both_branches_with_long_sigma_counterexample() {
    for (input, has_sigma, factor, fixed) in [
        ("Long中İTitle".repeat(128), false, 64_u64, 128_u64),
        (format!("aΣ{}Σ", "\u{10fc}".repeat(128)), true, 4096, 256),
    ] {
        let budget = SourceBudget::model_fixture();
        let mut op = budget.begin(CancellationToken::new());
        let before = op.stats().unwrap();
        let calls = lowercase::calls().0;
        let lower = Lowercase::new(&input, has_sigma, &mut op).unwrap();
        let after = op.stats().unwrap();
        let n = input.len() as u64;
        assert!(after.scanned_bytes() - before.scanned_bytes() >= factor * n + fixed);
        assert!(after.events() - before.events() >= factor * n + fixed);
        assert!(after.copied_bytes() - before.copied_bytes() >= 16 * n + 128);
        assert_eq!(lowercase::calls(), (calls + 1, has_sigma));
        assert_eq!(lower.text, input.to_lowercase());
        assert_eq!(
            budget.usage().auxiliary_capacity,
            lower.text.capacity() as u64
        );
        drop(lower);
        assert_eq!(budget.usage().auxiliary_capacity, 0);
    }
}

#[test]
fn cancellation_and_time_change_inside_std_discard_paid_output() {
    fn cancel(op: &mut Operation) {
        op.cancellation().cancel();
    }
    fn expire(op: &mut Operation) {
        op.set_clock_samples_for_test(&[5_000_000_000]);
    }
    for (hook, cancelled) in [
        (cancel as fn(&mut Operation), true),
        (expire as fn(&mut Operation), false),
    ] {
        let budget = SourceBudget::preview();
        let mut op = budget.begin(CancellationToken::new());
        let before = lowercase::calls().0;
        lowercase::after_std(Some(hook));
        let error = Lowercase::new("AΣ\u{10fc}Σ", true, &mut op).err().unwrap();
        lowercase::after_std(None);
        assert_eq!(lowercase::calls().0, before + 1);
        if cancelled {
            assert_eq!(error, CoreError::Cancelled);
        } else {
            assert!(matches!(error, CoreError::DeadlineExceeded { .. }));
        }
        assert_eq!(op.first_fault(), Some(error));
        assert_eq!(budget.usage().auxiliary_capacity, 0);
    }
}

#[test]
fn table_collision_probing_compares_actual_keys_and_charges_work() {
    let budget = SourceBudget::model_fixture();
    let mut op = budget.begin(CancellationToken::new());
    let mut table = Table::new(2, &mut op).unwrap();
    let (a, taken) = table.find(b"ab", b"a", 7, &mut op).unwrap();
    assert!(!taken);
    table
        .put(a, 7, TextRange { start: 0, len: 1 }, 1, &mut op)
        .unwrap();
    let (b, taken) = table.find(b"ab", b"b", 7, &mut op).unwrap();
    assert!(!taken);
    assert_ne!(a, b);
    table
        .put(b, 7, TextRange { start: 1, len: 1 }, 1, &mut op)
        .unwrap();
    assert_eq!(table.find(b"ab", b"a", 7, &mut op).unwrap(), (a, true));
    assert_eq!(table.find(b"ab", b"b", 7, &mut op).unwrap(), (b, true));
    assert!(op.stats().unwrap().scanned_bytes() > 0);
}
