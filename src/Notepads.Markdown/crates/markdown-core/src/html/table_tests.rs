// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE file in the project root for license information.
// ---------------------------------------------------------------------------------------------

use super::HtmlProjection;
use super::table::{self, CAPTION, HTML};
use super::tests::{parse, preview};
use crate::config::{Extensions, ParseConfig, Profile};
use crate::error::CoreError;
use crate::identity::SourceKey;
use crate::ir::{Alignment, NodeId, NodeKind};
use crate::model::AcceptedModel;
use crate::policy::{CancellationToken, SourceBudget, SourceLimits};
use crate::source::SourceDocument;

pub(super) const CASE: &str = "<table>\n<caption>HTML 表格与跨列</caption>\n<thead><tr><th>列一</th><th>列二</th></tr></thead>\n<tbody>\n<tr><td colspan=\"2\">跨两列，&amp; 实体测试</td></tr>\n<tr><td>R2C1</td><td><code>x &lt; y</code></td></tr>\n</tbody>\n</table>";

fn configured(source: &str, profile: Profile, extensions: Extensions) -> AcceptedModel {
    let budget = SourceBudget::preview();
    let mut op = budget.begin(CancellationToken::new());
    let input = SourceDocument::new(
        SourceKey::new([7; 16], 1).unwrap(),
        source.as_bytes(),
        &mut op,
    )
    .unwrap();
    let config = ParseConfig::with_extensions(profile, extensions);
    AcceptedModel::parse(input.snapshot(), config, 1, &mut op).unwrap()
}

fn facts(model: &AcceptedModel) -> Vec<table::CellFacts> {
    let mut op = model.budget().begin(CancellationToken::new());
    let mut result = Vec::new();
    for (index, node) in model.ir().nodes().iter().enumerate() {
        if node.kind != NodeKind::TableCell {
            continue;
        }
        let row = model.ir().nodes()[node.parent.unwrap().index()];
        if let Some(value) = model
            .html_table_cell(
                row.parent.unwrap(),
                NodeId::from_index(index).unwrap(),
                &mut op,
            )
            .unwrap()
        {
            result.push(value);
        }
    }
    result
}

#[test]
fn html_table_actual_case_has_real_rectangular_slots_and_bound_html_facts() {
    for source in [CASE.to_owned(), CASE.replace('\n', "\r\n")] {
        let model = configured(&source, Profile::G3, Extensions::ALL);
        assert_eq!(model.original_source().text(), source);
        assert_eq!(
            preview(&model),
            "HTML 表格与跨列\t\n列一\t列二\n跨两列，& 实体测试\t\nR2C1\tx < y"
        );
        let rows = facts(&model);
        assert_eq!(
            rows.iter()
                .map(|cell| (cell.colspan, cell.flags))
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
            rows.iter().map(|cell| cell.header).collect::<Vec<_>>(),
            [false, false, true, true, false, false, false, false]
        );
        assert_eq!(rows[0].alignment, Alignment::Center);
        assert!(
            rows[1..]
                .iter()
                .all(|cell| cell.alignment == Alignment::None)
        );
        assert_eq!(
            model
                .ir()
                .nodes()
                .iter()
                .filter(|node| node.kind == NodeKind::Table)
                .count(),
            1
        );
        assert_eq!(
            model
                .ir()
                .nodes()
                .iter()
                .filter(|node| node.kind == NodeKind::TableCell)
                .count(),
            8
        );
    }
}

#[test]
fn html_table_rowspans_keep_single_authored_owners_and_rectangular_text() {
    let source = "<table>\r\n<caption>跨度</caption>\r\n<thead><tr><th colspan=3>标题</th></tr></thead>\r\n\
                  <tbody><tr><td rowspan='&#48;' colspan=2>甲😀</td><td>一</td></tr>\r\n\
                  <tr><td>二</td></tr></tbody>\r\n<tbody><tr><td colspan=3>乙</td></tr></tbody>\r\n</table>";
    let model = parse(source, true);
    assert_eq!(model.original_source().text(), source);
    assert_eq!(
        preview(&model),
        "跨度\t\t\n标题\t\t\n甲😀\t\t一\n\t\t二\n乙\t\t"
    );
    assert_eq!(
        facts(&model)
            .iter()
            .map(|cell| (cell.colspan, cell.rowspan, cell.flags))
            .collect::<Vec<_>>(),
        [
            (3, 1, 5),
            (0, 0, 7),
            (0, 0, 7),
            (3, 1, 1),
            (0, 0, 3),
            (0, 0, 3),
            (2, 2, 1),
            (0, 0, 3),
            (1, 1, 1),
            (0, 0, 3),
            (0, 0, 3),
            (1, 1, 1),
            (3, 1, 1),
            (0, 0, 3),
            (0, 0, 3)
        ]
    );

    let entirely_covered = "<table><tr><td colspan=2 rowspan=2>span</td></tr><tr></tr></table>";
    let model = parse(entirely_covered, true);
    assert_eq!(preview(&model), "span\t\n\t");
    assert_eq!(
        facts(&model)
            .iter()
            .map(|cell| cell.rowspan)
            .collect::<Vec<_>>(),
        [2, 0, 0, 0]
    );
}

#[test]
fn html_table_rowspan_overlaps_or_cross_group_claims_fall_back() {
    for source in [
        "<table><tr><td>a</td><td rowspan=2>b</td></tr><tr><td colspan=2>overlap</td></tr></table>",
        "<table><thead><tr><td rowspan=2>a</td></tr></thead><tbody><tr><td>b</td></tr></tbody></table>",
        "<table><tr><td>a</td><td rowspan=2>b</td></tr><tr></tr></table>",
    ] {
        let model = parse(source, true);
        assert_eq!(preview(&model).trim_end(), source);
        assert!(facts(&model).is_empty());
    }
}

#[test]
fn html_table_empty_caption_and_empty_body_keep_independent_structural_positions() {
    let first = "<table><caption></caption><tbody><tr><td></td></tr></tbody></table>";
    let model = parse(first, true);
    assert_eq!(preview(&model), "\n");
    let rows = facts(&model);
    assert_eq!(
        rows.iter()
            .map(|cell| (cell.colspan, cell.flags, cell.header))
            .collect::<Vec<_>>(),
        [(1, HTML | CAPTION, false), (1, HTML, false)]
    );
    let cells = model
        .ir()
        .nodes()
        .iter()
        .filter(|node| node.kind == NodeKind::TableCell)
        .collect::<Vec<_>>();
    assert_eq!(cells.len(), 2);
    assert!(
        cells
            .iter()
            .all(|cell| cell.logical.is_empty() && cell.first_child.is_none())
    );
    assert_eq!(cells[0].logical.start(), 0);
    assert_eq!(cells[1].logical.start(), 1);
    let together = format!("{first}\n\n{CASE}");
    let model = parse(&together, true);
    assert_eq!(
        preview(&model),
        "\n\nHTML 表格与跨列\t\n列一\t列二\n跨两列，& 实体测试\t\nR2C1\tx < y"
    );
    assert_eq!(facts(&model).len(), 10);
    for source in [
        "<table><tr><td></td><td></td></tr></table>",
        "<table><tr><td> </td><td><b></b></td></tr><tr><td></td><td></td></tr></table>",
    ] {
        let model = parse(source, true);
        assert_eq!(
            facts(&model).len(),
            if source.contains("<b>") { 4 } else { 2 }
        );
        assert_eq!(
            preview(&model),
            if source.contains("<b>") {
                "\t\n\t"
            } else {
                "\t"
            }
        );
    }
}

#[test]
fn html_table_generated_row_source_authority_includes_its_own_lf() {
    let model = parse(
        "<table><caption></caption><tr><td colspan=2>X</td></tr></table>",
        true,
    );
    assert_eq!(model.source().text(), "\n\n| | |\n|---|---|\n|X| |\n\n\n\n");
    let rows = model
        .ir()
        .nodes()
        .iter()
        .filter(|node| node.kind == NodeKind::TableRow)
        .collect::<Vec<_>>();
    assert_eq!(rows.len(), 2);
    assert_eq!(
        (
            rows[0].source_envelope.start(),
            rows[0].source_envelope.end()
        ),
        (2, 8)
    );
    assert_eq!(
        (
            rows[1].source_envelope.start(),
            rows[1].source_envelope.end()
        ),
        (18, 24)
    );
    assert_eq!(preview(&model), "\t\nX\t");
}

#[test]
fn html_table_inline_text_entities_breaks_code_and_image_have_closed_provenance() {
    let source = "<table><tr><td><b>粗😀</b><br><code>*x*</code><a href='#z'>link</a> &amp; &amp;lt;</td><th rowspan='&#49;' scope='ROW'><img src='x.png' alt='ALT中'></th></tr></table>";
    let model = parse(source, true);
    assert_eq!(preview(&model), "粗😀\n*x*link & &lt;\t\u{fffc}");
    let values = facts(&model);
    assert_eq!(values.len(), 2);
    assert!(!values[0].header);
    assert!(values[1].header);
    assert!(
        values
            .iter()
            .all(|cell| cell.colspan == 1 && cell.flags == HTML)
    );
    assert_eq!(model.original_source().text(), source);
    assert!(
        model
            .ir()
            .objects()
            .iter()
            .any(|row| matches!(row, crate::ir::ObjectRow::Image { .. }))
    );
}

#[test]
fn html_table_collapses_space_before_trailing_empty_inline_elements() {
    let source = "<table><tr><td>x <span></span><strong> </strong></td>\
        <td>y <span>visible</span></td></tr></table>";
    let model = parse(source, true);
    assert_eq!(preview(&model), "x\ty visible");
    assert_eq!(facts(&model).len(), 2);
    assert_eq!(model.original_source().text(), source);
}

#[test]
fn html_table_trims_space_before_empty_images_and_keeps_nonempty_images() {
    let source = "<table><tr><td>x <img></td><td>y <img alt=' '><img></td>\
        <td>z <img src='x.png' alt='Image'></td><td>w <img alt='Fallback'></td></tr></table>";
    let model = parse(source, true);
    assert_eq!(preview(&model), "x\ty\tz \u{fffc}\tw Fallback");
    assert_eq!(facts(&model).len(), 4);
    assert_eq!(model.ir().objects().len(), 1);
    assert_eq!(model.original_source().text(), source);
}

#[test]
fn html_table_keeps_literal_pipes_in_link_and_image_destinations() {
    let source = "<table><tr><td><a href='https://example.test/?a=1|2'>Link</a> \
        <img src='a|b.png' alt='Image'></td></tr></table>";
    let model = parse(source, true);
    assert_eq!(preview(&model), "Link \u{fffc}");
    assert_eq!(facts(&model).len(), 1);
    let values: Vec<_> = model
        .ir()
        .payloads()
        .iter()
        .filter(|row| row.role == crate::ir::PayloadRole::Destination)
        .map(|row| model.ir().text(row.value).unwrap())
        .collect();
    assert_eq!(values, ["https://example.test/?a=1|2", "a|b.png"]);
    assert_eq!(model.original_source().text(), source);
}

#[test]
fn html_table_unicode_and_entity_mapping_retains_tight_original_cell_sources() {
    use crate::identity::{LogicalBoundary, LogicalSpan, SourceBoundary, SourceSpan};
    use crate::ir::TextTarget;
    use crate::mapping::{
        Affinity, Correspondence, PointInput, PointQuery, Position, RangeInput, RangeQuery,
    };
    let source = "Before\r\n\r\n<table>\r\n<caption>Caption中😀</caption>\r\n<tr><td colspan=2>RAW 中😀tail A&amp;B</td></tr>\r\n<tr><td>last</td><td>z</td></tr>\r\n</table>\r\n\r\nAfter";
    let model = parse(source, true);
    let main = preview(&model);
    assert_eq!(
        main,
        "Before\nCaption中😀\t\nRAW 中😀tail A&B\t\nlast\tz\nAfter"
    );
    let flow = model.main_flow_id();
    let mut op = model.budget().begin(CancellationToken::new());
    for needle in ["tion中😀", "😀tail", "tail", "ast", "fter"] {
        let byte = source.find(needle).unwrap() as u64;
        let logical = main[..main.find(needle).unwrap()].encode_utf16().count() as u64;
        for affinity in [Affinity::Before, Affinity::After] {
            let response = model
                .map_point(
                    &PointQuery {
                        model: model.identity(),
                        input: PointInput::Source {
                            target: TextTarget::Flow(flow),
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
                "{needle}/{affinity:?}"
            );
            assert_eq!(response.results()[0].correspondence, Correspondence::Exact);
            let response = model
                .map_point(
                    &PointQuery {
                        model: model.identity(),
                        input: PointInput::Flow {
                            flow,
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
    let amp = main[..main.find('&').unwrap()].encode_utf16().count() as u64;
    let start = source.find("&amp;").unwrap() as u64;
    let response = model
        .map_range(
            &RangeQuery {
                model: model.identity(),
                input: RangeInput::Flow {
                    flow,
                    span: LogicalSpan::new(amp, amp + 1).unwrap(),
                },
                empty_affinity: Affinity::After,
            },
            &mut op,
        )
        .unwrap();
    assert!(
        response
            .contributions()
            .iter()
            .any(|part| part.source == SourceSpan::new(start, start + 5).unwrap())
    );
    assert!(response.selections()[0].effects.atomic);
    assert_eq!(model.original_source().text(), source);
}

#[test]
fn html_table_first_cell_text_seam_matches_independent_gfm_and_keeps_tight_copy() {
    use crate::identity::{SourceBoundary, SourceSpan};
    use crate::ir::{AtomKind, TextTarget};
    use crate::mapping::{Affinity, PointInput, PointQuery, RangeInput, RangeQuery};
    let html = "<table><caption>h</caption><tr><td colspan=2>RAW</td></tr><tr><td>last</td><td>z</td></tr></table>";
    // Existing GFM syntax supplies the same row/first-cell boundary without
    // HTML projection, colspan metadata, or the new table binder.
    let gfm = "|h| |\n|---|---|\n|RAW| |\n|last|z|";
    let model = parse(html, true);
    let ordinary = parse(gfm, false);
    assert_eq!(preview(&model), "h\t\nRAW\t\nlast\tz");
    assert_eq!(preview(&ordinary), preview(&model));
    let point = |model: &AcceptedModel, source: &str, affinity| {
        let byte = source.find("last").unwrap() as u64;
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
        let result = response.results()[0];
        let empty = model
            .map_range(
                &RangeQuery {
                    model: model.identity(),
                    input: RangeInput::Source {
                        target: TextTarget::Flow(model.main_flow_id()),
                        span: SourceSpan::new(byte, byte).unwrap(),
                    },
                    empty_affinity: affinity,
                },
                &mut op,
            )
            .unwrap();
        assert_eq!(empty.boundaries()[0].chosen, result.chosen);
        (result.chosen, result.correspondence, result.effects)
    };
    for affinity in [Affinity::Before, Affinity::After] {
        let actual = point(&model, html, affinity);
        let baseline = point(&ordinary, gfm, affinity);
        assert_eq!(
            actual, baseline,
            "first-cell shared row-LF seam {affinity:?}"
        );
        if affinity == Affinity::Before {
            assert!(actual.2.synthetic);
        }
    }
    let mut ranges = Vec::new();
    for (model, source) in [(&ordinary, gfm), (&model, html)] {
        let main = preview(model);
        let logical = main[..main.find("last").unwrap()].encode_utf16().count() as u64;
        let byte = source.find("last").unwrap() as u64;
        let mut op = model.budget().begin(CancellationToken::new());
        let response = model
            .map_range(
                &RangeQuery {
                    model: model.identity(),
                    input: RangeInput::Source {
                        target: TextTarget::Flow(model.main_flow_id()),
                        span: SourceSpan::new(byte, byte + 4).unwrap(),
                    },
                    empty_affinity: Affinity::Before,
                },
                &mut op,
            )
            .unwrap();
        let selected = response.selections()[0];
        let related = |related: crate::mapping::Related| {
            (
                related.envelope,
                related.spans.slice(response.spans()).unwrap().to_vec(),
                related.anchors.slice(response.anchors()).unwrap().to_vec(),
            )
        };
        ranges.push((
            selected.start.chosen,
            selected.end.chosen,
            selected.start.correspondence,
            selected.end.correspondence,
            selected.start.effects,
            selected.end.effects,
            selected.effects,
            selected.expanded,
            related(selected.start.related),
            related(selected.end.related),
            related(selected.related),
            response
                .contributions()
                .iter()
                .map(|part| {
                    (
                        part.kind,
                        part.source.start() as i64 - byte as i64,
                        part.source.end() as i64 - byte as i64,
                        part.target,
                        part.expanded,
                    )
                })
                .collect::<Vec<_>>(),
        ));
        let copies = response
            .contributions()
            .iter()
            .filter(|part| part.kind == AtomKind::Copy)
            .collect::<Vec<_>>();
        assert_eq!(copies.len(), 1);
        assert_eq!(copies[0].source, SourceSpan::new(byte, byte + 4).unwrap());
        assert_eq!(
            (copies[0].target.start, copies[0].target.end),
            (logical, logical + 4)
        );
        assert!(
            response
                .contributions()
                .iter()
                .all(|part| part.kind != AtomKind::Object)
        );
        let mut read = String::new();
        crate::reader::text::visit(
            model,
            model.main_flow_id(),
            crate::identity::LogicalSpan::new(copies[0].target.start, copies[0].target.end)
                .unwrap(),
            &mut op,
            |text, _| {
                read.push_str(text);
                Ok(())
            },
        )
        .unwrap();
        assert_eq!(read, "last");
    }
    // Existing mapping::tests::original_multi_hidden_synthetic_keeps_endpoints_out_of_content
    // pins GFM's content-only aggregate. The existing projected range path
    // conservatively includes endpoint Effects as well. Its first boundary
    // is the synthetic row LF on both models; preserve that extra truthful
    // aggregate flag, while comparing every other fact without exceptions.
    assert!(!ranges[0].6.synthetic);
    assert!(ranges[0].4.synthetic);
    assert!(ranges[1].6.synthetic);
    let endpoint_synthetic = ranges[0].4.synthetic || ranges[0].5.synthetic;
    ranges[0].6.synthetic |= endpoint_synthetic;
    assert_eq!(
        ranges[1], ranges[0],
        "HTML shared-seam selection differs beyond the established projected endpoint-effect union"
    );
}

#[test]
fn html_table_raw_nul_expansion_keeps_original_and_uses_bounded_scan_charges() {
    let raw_body = format!("{}{}", "a".repeat(59_000), "\0".repeat(3_000));
    let expected = format!("{}{}", "a".repeat(59_000), "\u{fffd}".repeat(3_000));
    let source = format!("<table><tr><td>{raw_body}</td></tr></table>");
    assert!(source.len() < super::fragment::MAX_FRAGMENT_BYTES);
    assert_eq!(expected.len(), 68_000);
    // Independently establish that the current ordinary GFM parser keeps the
    // expanded text in a slice larger than one permitted work-charge block.
    let baseline = configured(
        &format!("|{expected}|\n|---|\n"),
        Profile::G3,
        Extensions::NONE,
    );
    let cell = baseline
        .ir()
        .nodes()
        .iter()
        .find(|node| node.kind == NodeKind::TableCell)
        .unwrap();
    let mut largest = 0;
    let mut op = baseline.budget().begin(CancellationToken::new());
    crate::reader::text::atoms(
        &baseline,
        baseline.main_flow_id(),
        cell.logical,
        &mut op,
        |atom, part, op| {
            largest = largest.max(crate::reader::text::atom_part(&baseline, atom, part, op)?.len());
            Ok(())
        },
    )
    .unwrap();
    assert!(
        largest > crate::policy::CHECKPOINT_BYTES as usize,
        "fixture must exercise a large accepted atom: {largest}"
    );
    let model = configured(&source, Profile::G3, Extensions::ALL);
    assert!(model.original_source().text() == source);
    assert_eq!(facts(&model).len(), 1);
    assert!(preview(&model) == expected);
}

#[test]
fn html_table_unsupported_whole_fragment_preserves_existing_fallback() {
    for source in [
        "<table><tr><td rowspan=2>x</td></tr></table>",
        "<table><tr><td colspan=0>x</td></tr></table>",
        "<table><tr><td colspan=65>x</td></tr></table>",
        "<table><tr><td colspan=2 colspan=1>x</td></tr></table>",
        "<table><tr><td colspan='&amp;#50;'>x</td></tr></table>",
        "<table><tr><td scope=bad>x</td></tr></table>",
        "<table><tr><td>a</td><td>b</td></tr><tr><td>c</td></tr></table>",
        "<table><tr><td><table><tr><td>x</td></tr></table></td></tr></table>",
        "<table><tr><td><input type=checkbox disabled></td></tr></table>",
        "<table><tr><td><p>x</p></td></tr></table>",
        "<table><tr><td><ruby>x<rt><b>y</b></rt></ruby></td></tr></table>",
        "<table><tr><td>x</th></tr></table>",
        "<table><tr><td>x</td></tr>",
        "<table><tr><td>x</td></tr></table><p>other</p>",
        "<table><tr><td>x</td></tr>\n\n<tr><td>y</td></tr></table>",
    ] {
        let model = parse(source, true);
        let baseline = configured(source, Profile::G3, Extensions::HTML_INLINE);
        assert_eq!(model.original_source().text(), source);
        assert_eq!(preview(&model), preview(&baseline), "{source}");
        assert!(facts(&model).is_empty(), "{source}");
        assert!(
            !model
                .ir()
                .objects()
                .iter()
                .any(|row| matches!(row, crate::ir::ObjectRow::Task { .. }))
        );
    }
    let commonmark = configured(CASE, Profile::C0, Extensions::from_bits(12).unwrap());
    let baseline = configured(CASE, Profile::C0, Extensions::HTML_INLINE);
    assert_eq!(preview(&commonmark), preview(&baseline));
    assert!(facts(&commonmark).is_empty());
}

#[test]
fn html_table_gfm_and_yaml_defaults_remain_unmarked() {
    let model = parse(
        "---\na: b\nc: d\n---\n\n| a | b |\n| --- | --- |\n| c | d |",
        true,
    );
    assert!(facts(&model).is_empty());
    assert!(
        model
            .ir()
            .nodes()
            .iter()
            .any(|node| node.kind == NodeKind::Table)
    );
}

#[test]
fn html_table_amplification_is_reserved_before_slots_and_limits_include_caption() {
    let budget = SourceBudget::new(SourceLimits {
        auxiliary_capacity: 1024,
        ..SourceLimits::default()
    })
    .unwrap();
    let mut op = budget.begin(CancellationToken::new());
    assert!(
        table::lower("<table><tr><td colspan=64>x</td></tr></table>", &mut op)
            .unwrap()
            .is_none()
    );
    drop(op);
    assert_eq!(budget.usage().auxiliary_capacity, 0);
    let budget = SourceBudget::preview();
    let mut op = budget.begin(CancellationToken::new());
    let at_limit = format!(
        "<table><caption></caption>{}</table>",
        "<tr><td></td></tr>".repeat(255)
    );
    let prepared = table::lower(&at_limit, &mut op).unwrap().unwrap();
    assert_eq!(
        prepared.fragment.html_table.as_ref().unwrap().rows.len(),
        256
    );
    assert!(budget.usage().auxiliary_capacity > 0);
    drop(prepared);
    assert_eq!(budget.usage().auxiliary_capacity, 0);
    let too_many = format!(
        "<table><caption></caption>{}</table>",
        "<tr><td></td></tr>".repeat(256)
    );
    assert!(table::lower(&too_many, &mut op).unwrap().is_none());
    let too_many_slots = format!(
        "<table>{}</table>",
        "<tr><td colspan=64>x</td></tr>".repeat(65)
    );
    assert!(table::lower(&too_many_slots, &mut op).unwrap().is_none());
    drop(op);
    assert_eq!(budget.usage().auxiliary_capacity, 0);
}

fn binding_fixture() -> (HtmlProjection, AcceptedModel, crate::policy::Lease) {
    let raw = "<table><caption>C</caption><tr><td colspan=2>A&amp;B</td></tr></table>";
    let budget = SourceBudget::preview();
    let mut op = budget.begin(CancellationToken::new());
    let prepared = table::lower(raw, &mut op).unwrap().unwrap();
    let input = SourceDocument::new(
        SourceKey::new([7; 16], 1).unwrap(),
        prepared.fragment.markdown.as_bytes(),
        &mut op,
    )
    .unwrap();
    let original = SourceDocument::new(input.snapshot().key(), raw.as_bytes(), &mut op).unwrap();
    let model =
        AcceptedModel::parse(input.snapshot(), ParseConfig::new(Profile::G3), 1, &mut op).unwrap();
    let projection = HtmlProjection {
        original: original.snapshot(),
        segments: Vec::new(),
        alignments: Vec::new(),
        images: Vec::new(),
        styles: Vec::new(),
        literal_regions: Vec::new(),
        ruby: Vec::new(),
        pictures: Vec::new(),
        destinations: Vec::new(),
        anchors: Vec::new(),
        citation_anchors_start: 0,
        citation_regions: Vec::new(),
        bibliography: None,
        disclosures: Vec::new(),
        static_checkboxes: Vec::new(),
        static_checkbox_objects: Vec::new(),
        static_checkbox_flow: None,
        tables: vec![prepared.fragment.html_table.unwrap()],
        front_matter_table: None,
        _storage: budget.reserve_auxiliary(1024).unwrap(),
    };
    (projection, model, prepared.storage)
}

#[test]
fn html_table_binding_rejects_corrupt_text_source_span_coverage_and_duplicate_authority() {
    for corruption in 0..6 {
        let (mut projection, model, _stage) = binding_fixture();
        let cell = &mut projection.tables[0].cells[2];
        match corruption {
            0 => cell.expected.push('X'),
            1 => cell.source = 0..0,
            2 => cell.facts.colspan = 1,
            3 => projection.tables[0].cells[3].expected.push('X'),
            4 => projection.tables[0].projected.start += 1,
            _ => projection.tables[0].rows.pop().map(|_| ()).unwrap(),
        }
        let mut op = model.budget().begin(CancellationToken::new());
        assert_eq!(
            projection.bind_tables(&model, &mut op),
            Err(CoreError::InvalidProvenance),
            "corruption {corruption}"
        );
    }
    let (mut projection, model, _stage) = binding_fixture();
    let mut op = model.budget().begin(CancellationToken::new());
    projection.bind_tables(&model, &mut op).unwrap();
    assert_eq!(
        projection.bind_tables(&model, &mut op),
        Err(CoreError::InvalidProvenance)
    );
}

#[test]
fn html_table_cancellation_never_becomes_raw_fallback() {
    let budget = SourceBudget::preview();
    let token = CancellationToken::new();
    token.cancel();
    let mut op = budget.begin(token);
    assert!(matches!(
        table::lower(CASE, &mut op),
        Err(CoreError::Cancelled)
    ));
    let (mut projection, model, _stage) = binding_fixture();
    let token = CancellationToken::new();
    token.cancel();
    let mut op = model.budget().begin(token);
    assert_eq!(
        projection.bind_tables(&model, &mut op),
        Err(CoreError::Cancelled)
    );
}
