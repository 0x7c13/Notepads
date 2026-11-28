// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE file in the project root for license information.
// ---------------------------------------------------------------------------------------------

use super::controls::StaticCheckbox;
use super::tests::{parse, preview};
use super::{HtmlProjection, Segment};
use crate::config::{Extensions, ParseConfig, Profile};
use crate::error::CoreError;
use crate::identity::{LogicalBoundary, LogicalSpan, SourceBoundary, SourceKey, SourceSpan};
use crate::ir::{ObjectId, ObjectRow, TextTarget};
use crate::mapping::{
    Affinity, Correspondence, PointInput, PointQuery, Position, RangeInput, RangeQuery,
};
use crate::model::AcceptedModel;
use crate::policy::{CancellationToken, SourceBudget};
use crate::source::SourceDocument;

#[test]
fn disabled_html_checkboxes_preserve_boolean_presence_and_inline_label_text() {
    for (attributes, checked) in [
        ("type='checkbox' checked disabled", true),
        ("type='checkbox' disabled", false),
        ("TYPE='ChEcKbOx' CHECKED='false' DISABLED='false'", true),
        ("type='ch&#101;ckbox' checked='' disabled=''", true),
    ] {
        let source = format!(
            "<label><input {attributes}> 中😀 <b>bold</b> <code>*literal*</code> &amp; tail</label>\r\n"
        );
        let model = parse(&source, true);
        assert_eq!(
            preview(&model),
            "\u{fffc} 中😀 bold *literal* & tail",
            "{source}"
        );
        assert_eq!(model.original_source().text(), source);
        let tasks = model
            .ir()
            .objects()
            .iter()
            .enumerate()
            .filter_map(|(index, object)| {
                if let ObjectRow::Task { checked, .. } = object {
                    Some((ObjectId::from_index(index).unwrap(), *checked))
                } else {
                    None
                }
            })
            .collect::<Vec<_>>();
        assert_eq!(tasks.len(), 1, "{source}");
        assert_eq!(tasks[0].1, checked, "{source}");
        assert!(model.is_static_html_checkbox(tasks[0].0));
    }
}

#[test]
fn disabled_html_checkbox_keeps_empty_and_multiline_label_contracts() {
    for (source, expected) in [
        ("<label><input type=checkbox disabled></label>", "\u{fffc}"),
        (
            "<label>\r\n  <input type=checkbox disabled>\r\n  中😀\r\n <strong>line</strong> \r\n</label>",
            "\u{fffc} 中😀 line",
        ),
    ] {
        let model = parse(source, true);
        assert_eq!(preview(&model), expected, "{source:?}");
        assert_eq!(model.original_source().text(), source);
        assert_eq!(
            model
                .ir()
                .objects()
                .iter()
                .enumerate()
                .filter(|(index, _)| {
                    model.is_static_html_checkbox(ObjectId::from_index(*index).unwrap())
                })
                .count(),
            1
        );
    }
}

#[test]
fn unsupported_checkbox_labels_fall_back_as_complete_original_blocks() {
    for source in [
        "<label><input type=checkbox checked> enabled</label>",
        "<label><input type=radio disabled> radio</label>",
        "<label><input disabled> default text input</label>",
        "<label><input type='ch&amp;#101;ckbox' disabled> decode once</label>",
        "<label for='elsewhere'><input type=checkbox disabled> external</label>",
        "<label>not leading<input type=checkbox disabled></label>",
        "<label><b></b><input type=checkbox disabled> not leading</label>",
        "<label><input type=checkbox disabled><input type=checkbox disabled> two</label>",
        "<label><input type=checkbox disabled><b>mismatch</i></label>",
        "<label><input type=checkbox disabled></input> stray close</label>",
        "<label><input type=checkbox disabled> stray</b></label>",
        "<label><input type=checkbox disabled> missing close",
        "<label><input type=checkbox disabled><p>block child</p></label>",
        "<div><label><input type=checkbox disabled> nested</label></div>",
        "<label><input type=checkbox disabled> one</label><label><input type=checkbox disabled> two</label>",
        "<label><input type=checkbox disabled> one</label><p>extra</p>",
        "<label><input type=checkbox disabled> one</label>extra text",
        "prefix <label><input type=checkbox disabled> inline</label>",
    ] {
        let model = parse(source, true);
        // Rejected labels retain the pre-H1 safe-inline behavior. Some safe
        // delimiters (for example an empty <b></b>) already disappear there.
        let baseline = parse_config(source, Profile::G3, Extensions::HTML_INLINE);
        assert_eq!(preview(&model), preview(&baseline), "{source:?}");
        assert!(preview(&model).contains("<label"), "{source:?}");
        assert!(preview(&model).contains("<input"), "{source:?}");
        assert_eq!(model.original_source().text(), source);
        assert_eq!(baseline.original_source().text(), source);
        assert!(
            !model
                .ir()
                .objects()
                .iter()
                .any(|object| matches!(object, ObjectRow::Task { .. }))
        );
        assert!(!model.ir().objects().iter().enumerate().any(|(index, _)| {
            model.is_static_html_checkbox(ObjectId::from_index(index).unwrap())
        }));
    }
    let source = "<label><input type=checkbox disabled> literal</label>";
    assert!(preview(&parse(source, false)).contains(source));
    let oversized = format!(
        "<label><input type=checkbox disabled>{}</label>",
        "x".repeat(super::fragment::MAX_FRAGMENT_BYTES)
    );
    assert!(super::fragment::lower(&oversized, false).is_none());
}

#[test]
fn c0_html_checkbox_has_inert_fallback_instead_of_an_unbound_gfm_object() {
    let source = "<label><input type=checkbox disabled> C0</label>";
    let model = parse_config(source, Profile::C0, Extensions::HTML_BLOCKS);
    assert!(preview(&model).contains(source));
}

fn parse_config(source: &str, profile: Profile, extensions: Extensions) -> AcceptedModel {
    let budget = SourceBudget::preview();
    let mut op = budget.begin(CancellationToken::new());
    let document = SourceDocument::new(
        SourceKey::new([77; 16], 1).unwrap(),
        source.as_bytes(),
        &mut op,
    )
    .unwrap();
    AcceptedModel::parse(
        document.snapshot(),
        ParseConfig::with_extensions(profile, extensions),
        1,
        &mut op,
    )
    .unwrap()
}

#[test]
fn checkbox_input_is_atomic_while_neighboring_label_unicode_is_exact() {
    let source = "Before中😀\r\n\r\n<label><input type='checkbox' checked disabled> 标签中😀tail A&amp;B</label>\r\n\r\nAfter";
    let model = parse(source, true);
    let text = preview(&model);
    let flow = model.main_flow_id();
    let input_start = source.find("<input").unwrap() as u64;
    let input_end = input_start + source[input_start as usize..].find('>').unwrap() as u64 + 1;
    let atom = model
        .ir()
        .objects()
        .iter()
        .find_map(|object| {
            if let ObjectRow::Task { atom, .. } = object {
                Some(*atom)
            } else {
                None
            }
        })
        .unwrap();
    let mut op = model.budget().begin(CancellationToken::new());
    let atom = model.ir().atom_at(atom.index(), &mut op).unwrap();
    let object = LogicalSpan::new(atom.target.start, atom.target.end).unwrap();
    let object_in_main = text[..text.find('\u{fffc}').unwrap()]
        .encode_utf16()
        .count() as u64;
    assert_eq!(object.start(), object_in_main, "{text:?}");
    assert_eq!(object.len(), 1);
    for (affinity, logical) in [
        (Affinity::Before, object.start()),
        (Affinity::After, object.end()),
    ] {
        let response = model
            .map_point(
                &PointQuery {
                    model: model.identity(),
                    input: PointInput::Source {
                        target: TextTarget::Flow(flow),
                        byte: SourceBoundary::new(input_start + 4),
                    },
                    affinity,
                },
                &mut op,
            )
            .unwrap();
        assert_eq!(
            response.results()[0].chosen,
            Position::Flow(LogicalBoundary::new(logical)),
            "{affinity:?}, object={object:?}, atom={atom:?}, main={text:?}, projected={:?}",
            model.source().text()
        );
        assert!(matches!(
            response.results()[0].correspondence,
            Correspondence::Atomic | Correspondence::Object
        ));
        assert!(response.results()[0].effects.atomic || response.results()[0].effects.object);
    }
    let selection = model
        .map_range(
            &RangeQuery {
                model: model.identity(),
                input: RangeInput::Flow { flow, span: object },
                empty_affinity: Affinity::After,
            },
            &mut op,
        )
        .unwrap();
    assert_eq!(selection.contributions().len(), 1);
    assert_eq!(
        selection.contributions()[0].source,
        SourceSpan::new(input_start, input_end).unwrap()
    );
    assert_eq!(
        selection.contributions()[0].kind,
        crate::ir::AtomKind::Object
    );
    assert!(selection.selections()[0].effects.object);
    for needle in ["😀tail", "tail", "fter"] {
        let byte = source.find(needle).unwrap() as u64;
        let logical = text[..text.find(needle).unwrap()].encode_utf16().count() as u64;
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
                "{needle}"
            );
            assert_eq!(
                response.results()[0].correspondence,
                Correspondence::Exact,
                "{needle}"
            );
            let reverse = model
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
                reverse.results()[0].chosen,
                Position::Source(SourceBoundary::new(byte)),
                "{needle}"
            );
            assert_eq!(
                reverse.results()[0].correspondence,
                Correspondence::Exact,
                "{needle}"
            );
        }
    }
    // A paragraph's leading source point also owns its existing GFM block
    // separator under Before affinity. Compare that seam to real authored
    // GFM with the same Main instead of claiming every boundary is exact.
    let baseline_source = "Before中😀\r\n\r\n- [x] 标签中😀tail A&amp;B\r\n\r\nAfter";
    let baseline = parse_config(baseline_source, Profile::G3, Extensions::HTML_INLINE);
    assert_eq!(preview(&baseline), text);
    for affinity in [Affinity::Before, Affinity::After] {
        let query = PointQuery {
            model: model.identity(),
            input: PointInput::Source {
                target: TextTarget::Flow(flow),
                byte: SourceBoundary::new(source.find("After").unwrap() as u64),
            },
            affinity,
        };
        let actual = model.map_point(&query, &mut op).unwrap();
        let mut baseline_op = baseline.budget().begin(CancellationToken::new());
        let expected = baseline
            .map_point(
                &PointQuery {
                    model: baseline.identity(),
                    input: PointInput::Source {
                        target: TextTarget::Flow(baseline.main_flow_id()),
                        byte: SourceBoundary::new(baseline_source.find("After").unwrap() as u64),
                    },
                    affinity,
                },
                &mut baseline_op,
            )
            .unwrap();
        assert_eq!(
            actual.results()[0].chosen.value(),
            expected.results()[0].chosen.value()
        );
        assert_eq!(
            actual.results()[0].correspondence,
            expected.results()[0].correspondence
        );
    }
    let amp_start = source.find("&amp;").unwrap() as u64;
    let amp_logical = text[..text.find('&').unwrap()].encode_utf16().count() as u64;
    for (affinity, logical) in [
        (Affinity::Before, amp_logical),
        (Affinity::After, amp_logical + 1),
    ] {
        let response = model
            .map_point(
                &PointQuery {
                    model: model.identity(),
                    input: PointInput::Source {
                        target: TextTarget::Flow(flow),
                        byte: SourceBoundary::new(amp_start + 2),
                    },
                    affinity,
                },
                &mut op,
            )
            .unwrap();
        assert_eq!(
            response.results()[0].chosen,
            Position::Flow(LogicalBoundary::new(logical))
        );
        assert_eq!(response.results()[0].correspondence, Correspondence::Atomic);
    }
}

// A deliberately small independently assembled binding fixture lets corrupted
// side rows exercise rejection without exposing mutable accepted models.
fn binding_fixture(projected: &str) -> (HtmlProjection, AcceptedModel) {
    let model = parse(projected, false);
    let mut op = model.budget().begin(CancellationToken::new());
    let original = "<input type=checkbox checked disabled>";
    let document =
        SourceDocument::new(model.original_source().key(), original.as_bytes(), &mut op).unwrap();
    let projection = HtmlProjection {
        original: document.snapshot(),
        segments: vec![Segment {
            original: 0..original.len(),
            preview: 2..5,
            exact: false,
        }],
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
        static_checkboxes: vec![StaticCheckbox {
            marker: 2..5,
            source: 0..original.len(),
            checked: true,
            object: None,
            atom: None,
            logical: None,
        }],
        front_matter_table: None,
        static_checkbox_objects: Vec::with_capacity(1),
        static_checkbox_flow: None,
        tables: Vec::new(),
        _storage: op.budget().reserve_auxiliary(1024).unwrap(),
    };
    (projection, model)
}

#[test]
fn static_checkbox_binding_rejects_missing_duplicate_or_inconsistent_facts() {
    for corruption in 0..5 {
        let (mut projection, model) = binding_fixture(if corruption == 4 {
            "  [x] no task"
        } else {
            "- [x] task"
        });
        match corruption {
            0 => projection.static_checkboxes[0].checked = false,
            1 => projection.static_checkboxes[0].source.end -= 1,
            2 => projection.static_checkboxes.push(StaticCheckbox {
                marker: 2..5,
                source: projection.static_checkboxes[0].source.clone(),
                checked: true,
                object: None,
                atom: None,
                logical: None,
            }),
            3 => projection.static_checkboxes[0].object = Some(ObjectId::from_index(0).unwrap()),
            _ => {}
        }
        let mut op = model.budget().begin(CancellationToken::new());
        assert!(
            matches!(
                projection.bind_static_checkboxes(&model, &mut op),
                Err(CoreError::InvalidProvenance)
            ),
            "corruption {corruption}"
        );
        assert_eq!(op.first_fault(), Some(CoreError::InvalidProvenance));
    }
    let (mut projection, model) = binding_fixture("- [x] task");
    let cancellation = CancellationToken::new();
    cancellation.cancel();
    let mut op = model.budget().begin(cancellation);
    assert!(matches!(
        projection.bind_static_checkboxes(&model, &mut op),
        Err(CoreError::Cancelled)
    ));
    assert!(projection.static_checkboxes[0].object.is_none());
}

#[test]
fn multiple_static_checkbox_objects_bind_once_in_sorted_directory() {
    let source = "<label><input type=checkbox checked disabled> one</label>\n\n<label><input type=checkbox disabled> two</label>\n\n- [x] editable";
    let model = parse(source, true);
    assert_eq!(
        preview(&model),
        "\u{fffc} one\n\u{fffc} two\n\u{fffc} editable"
    );
    let static_tasks = model
        .ir()
        .objects()
        .iter()
        .enumerate()
        .filter_map(|(index, object)| {
            matches!(object, ObjectRow::Task { .. })
                .then(|| model.is_static_html_checkbox(ObjectId::from_index(index).unwrap()))
        })
        .collect::<Vec<_>>();
    assert_eq!(static_tasks, [true, true, false]);
}

#[test]
fn static_checkbox_directory_crosses_event_windows() {
    for count in [42, 43, 128] {
        let source = "<label><input type=checkbox disabled>label</label>\n\n".repeat(count);
        let model = parse(&source, true);
        assert_eq!(
            preview(&model),
            vec!["\u{fffc} label"; count].join("\n"),
            "{count} static checkboxes"
        );
        assert_eq!(model.original_source().text(), source);
        assert_eq!(model.ir().objects().len(), count);
        for index in 0..count {
            assert!(model.is_static_html_checkbox(ObjectId::from_index(index).unwrap()));
        }
    }
}

#[test]
fn html_event_windows_preserve_work_limits_and_cancellation() {
    use crate::error::LimitKind;
    use crate::policy::{CHECKPOINT_EVENTS, SourceLimits};

    for count in [0, 255, 256, 257, 1024] {
        let budget = SourceBudget::model_fixture();
        let mut op = budget.begin(CancellationToken::new());
        let before = op.stats().unwrap().events();
        super::charge_events(&mut op, count).unwrap();
        assert_eq!(op.stats().unwrap().events() - before, count);
    }
    let budget = SourceBudget::new(SourceLimits {
        max_work: CHECKPOINT_EVENTS,
        ..SourceLimits::default()
    })
    .unwrap();
    let mut op = budget.begin(CancellationToken::new());
    let failure = super::charge_events(&mut op, CHECKPOINT_EVENTS + 1).unwrap_err();
    assert_eq!(
        failure,
        CoreError::ResourceLimit {
            kind: LimitKind::Work,
            requested: CHECKPOINT_EVENTS + 1,
            limit: CHECKPOINT_EVENTS,
        }
    );
    assert_eq!(op.stats().unwrap().events(), CHECKPOINT_EVENTS);
    assert_eq!(op.first_fault(), Some(failure));
    assert_eq!(super::charge_events(&mut op, 1), Err(failure));

    let cancellation = CancellationToken::new();
    let mut op = budget.begin(cancellation.clone());
    cancellation.cancel();
    assert_eq!(
        super::charge_events(&mut op, CHECKPOINT_EVENTS + 1),
        Err(CoreError::Cancelled)
    );
    assert_eq!(op.stats().unwrap().events(), 0);
    assert_eq!(op.first_fault(), Some(CoreError::Cancelled));
}

#[test]
fn checkbox_source_points_ranges_and_mixed_expansion_use_the_bound_task_atom() {
    use crate::ir::AtomKind;
    let source = "Lead中😀\n\n<label><input type=checkbox checked disabled> X&amp;Y <img src='p.png' alt='alt中😀'> Z</label>\n\nAfter";
    let model = parse(source, true);
    let main = preview(&model);
    let flow = model.main_flow_id();
    let task = model
        .ir()
        .objects()
        .iter()
        .find_map(|object| {
            if let ObjectRow::Task { atom, .. } = object {
                Some(*atom)
            } else {
                None
            }
        })
        .unwrap();
    let mut op = model.budget().begin(CancellationToken::new());
    let atom = model.ir().atom_at(task.index(), &mut op).unwrap();
    let (start, end) = (atom.target.start, atom.target.end);
    let input_start = source.find("<input").unwrap() as u64;
    let input_end = input_start + source[input_start as usize..].find('>').unwrap() as u64 + 1;
    let ordinary = parse_config(model.source().text(), Profile::G3, Extensions::HTML_INLINE);
    assert_eq!(preview(&ordinary), main);
    let ordinary_body_start = model
        .ir()
        .objects()
        .iter()
        .find_map(|object| {
            if let ObjectRow::Task { spacer, .. } = object {
                Some(
                    spacer
                        .slice(model.ir().sources())
                        .unwrap()
                        .last()
                        .unwrap()
                        .end(),
                )
            } else {
                None
            }
        })
        .unwrap();
    for byte in [input_start, input_start + 4, input_end] {
        for affinity in [Affinity::Before, Affinity::After] {
            let expected = if byte == input_start {
                start
            } else if byte == input_end || affinity == Affinity::After {
                end
            } else {
                start
            };
            let query = PointQuery {
                model: model.identity(),
                input: PointInput::Source {
                    target: TextTarget::Flow(flow),
                    byte: SourceBoundary::new(byte),
                },
                affinity,
            };
            let response = model.map_point(&query, &mut op).unwrap();
            let point = response.results()[0];
            if byte == input_end && affinity == Affinity::After {
                let mut ordinary_op = ordinary.budget().begin(CancellationToken::new());
                let right_side = ordinary
                    .map_point(
                        &PointQuery {
                            model: ordinary.identity(),
                            input: PointInput::Source {
                                target: TextTarget::Flow(ordinary.main_flow_id()),
                                byte: SourceBoundary::new(ordinary_body_start),
                            },
                            affinity,
                        },
                        &mut ordinary_op,
                    )
                    .unwrap();
                assert_eq!(point.chosen.value(), right_side.results()[0].chosen.value());
                assert_eq!(point.correspondence, right_side.results()[0].correspondence);
                assert_eq!(point.effects, right_side.results()[0].effects);
            } else {
                assert_eq!(
                    point.chosen,
                    Position::Flow(LogicalBoundary::new(expected)),
                    "{byte}/{affinity:?}"
                );
                assert_eq!(
                    point.correspondence,
                    if byte == input_start || byte == input_end {
                        Correspondence::Exact
                    } else {
                        Correspondence::Object
                    }
                );
                let contributors = point.contributors.slice(response.contributors()).unwrap();
                assert_eq!(contributors.len(), 1);
                assert_eq!(contributors[0].atom, task);
                assert_eq!(
                    contributors[0].source,
                    SourceSpan::new(input_start, input_end).unwrap()
                );
            }
            let empty = model
                .map_range(
                    &RangeQuery {
                        model: model.identity(),
                        input: RangeInput::Source {
                            target: TextTarget::Flow(flow),
                            span: SourceSpan::new(byte, byte).unwrap(),
                        },
                        empty_affinity: affinity,
                    },
                    &mut op,
                )
                .unwrap();
            assert_eq!(empty.boundaries()[0].chosen, point.chosen);
            assert!(empty.contributions().is_empty());
        }
    }
    for (a, b, expanded) in [
        (input_start, input_end, false),
        (input_start + 2, input_end - 2, true),
        (input_start, input_end - 1, true),
        (input_start + 1, input_end, true),
    ] {
        let response = model
            .map_range(
                &RangeQuery {
                    model: model.identity(),
                    input: RangeInput::Source {
                        target: TextTarget::Flow(flow),
                        span: SourceSpan::new(a, b).unwrap(),
                    },
                    empty_affinity: Affinity::After,
                },
                &mut op,
            )
            .unwrap();
        let selection = response.selections()[0];
        assert_eq!(
            (selection.start.chosen.value(), selection.end.chosen.value()),
            (start, end)
        );
        assert_eq!(selection.expanded, expanded, "{a}..{b}");
        assert_eq!(response.contributions().len(), 1);
        let contribution = response.contributions()[0];
        assert_eq!(contribution.atom, task);
        assert_eq!(contribution.expanded, expanded);
        assert_eq!(contribution.source, SourceSpan::new(a, b).unwrap());
        assert_eq!(
            (contribution.target.start, contribution.target.end),
            (start, end)
        );
    }
    let x_end = source.find(" X&amp;").unwrap() as u64 + 2;
    let label_start_in_main = main[..main.find('X').unwrap()].encode_utf16().count() as u64;
    for (a, b, expected_start, expected_end) in [
        (
            input_end,
            x_end,
            label_start_in_main,
            label_start_in_main + 1,
        ),
        (1, input_start, 1, start),
        (1, input_end, 1, end),
    ] {
        let response = model
            .map_range(
                &RangeQuery {
                    model: model.identity(),
                    input: RangeInput::Source {
                        target: TextTarget::Flow(flow),
                        span: SourceSpan::new(a, b).unwrap(),
                    },
                    empty_affinity: Affinity::After,
                },
                &mut op,
            )
            .unwrap();
        let selection = response.selections()[0];
        assert_eq!(
            (selection.start.chosen.value(), selection.end.chosen.value()),
            (expected_start, expected_end)
        );
        let other_expansion = response
            .contributions()
            .iter()
            .any(|row| row.atom != task && row.expanded);
        assert_eq!(
            selection.expanded,
            other_expansion,
            "{a}..{b}: {:?}",
            response.contributions()
        );
        assert!(
            response
                .contributions()
                .iter()
                .filter(|row| row.atom == task || row.kind == AtomKind::Copy)
                .all(|row| !row.expanded)
        );
        if a == input_end {
            assert!(!selection.expanded);
        }
        if a == input_end || b == input_start {
            assert!(response.contributions().iter().all(|row| row.atom != task));
        }
    }
    let amp = source.find("&amp;").unwrap() as u64;
    let image = source.find("<img").unwrap() as u64;
    for b in [amp + 2, image + 3] {
        let response = model
            .map_range(
                &RangeQuery {
                    model: model.identity(),
                    input: RangeInput::Source {
                        target: TextTarget::Flow(flow),
                        span: SourceSpan::new(input_start, b).unwrap(),
                    },
                    empty_affinity: Affinity::After,
                },
                &mut op,
            )
            .unwrap();
        assert!(response.selections()[0].expanded);
        assert!(
            response
                .contributions()
                .iter()
                .any(|row| row.atom == task && !row.expanded)
        );
        assert!(
            response.contributions().iter().any(|row| row.atom != task
                && matches!(row.kind, AtomKind::Transform | AtomKind::Object)
                && row.expanded),
            "{b}: {:?}",
            response.contributions()
        );
    }
}

#[test]
fn checkbox_source_mapping_cannot_redirect_image_alt_or_payload_domains() {
    let source = "<label><input type=checkbox disabled> <img src='p.png' alt='alt中😀'></label>";
    let model = parse(source, true);
    let (alt, destination) = model
        .ir()
        .objects()
        .iter()
        .find_map(|object| {
            if let ObjectRow::Image {
                alt, destination, ..
            } = object
            {
                Some((*alt, *destination))
            } else {
                None
            }
        })
        .unwrap();
    let input = source.find("<input").unwrap() as u64 + 3;
    for target in [TextTarget::Flow(alt), TextTarget::Payload(destination)] {
        let mut op = model.budget().begin(CancellationToken::new());
        let point = model.map_point(
            &PointQuery {
                model: model.identity(),
                input: PointInput::Source {
                    target,
                    byte: SourceBoundary::new(input),
                },
                affinity: Affinity::After,
            },
            &mut op,
        );
        assert!(matches!(point, Err(CoreError::OutsideScope)), "{target:?}");
        let mut op = model.budget().begin(CancellationToken::new());
        let range = model.map_range(
            &RangeQuery {
                model: model.identity(),
                input: RangeInput::Source {
                    target,
                    span: SourceSpan::new(input, input + 2).unwrap(),
                },
                empty_affinity: Affinity::Before,
            },
            &mut op,
        );
        assert!(matches!(range, Err(CoreError::OutsideScope)), "{target:?}");
    }
}

#[test]
fn checkbox_unspaced_label_seam_preserves_both_sides_and_label_only_ranges() {
    use crate::ir::AtomKind;
    for label in ["Beta", "中文尾", "😀tail"] {
        let source = format!("<label><input type=checkbox disabled>{label}</label>");
        let model = parse(&source, true);
        let main = preview(&model);
        assert_eq!(main, format!("\u{fffc} {label}"));
        let label_start = source.find(label).unwrap() as u64;
        let label_end = label_start + label.len() as u64;
        let input_start = source.find("<input").unwrap() as u64;
        let input_end = input_start + source[input_start as usize..].find('>').unwrap() as u64 + 1;
        assert_eq!(
            input_end, label_start,
            "one original byte owns both seam sides"
        );
        let label_utf16 = label.encode_utf16().count() as u64;
        let flow = model.main_flow_id();
        let mut op = model.budget().begin(CancellationToken::new());
        for (affinity, expected) in [(Affinity::After, 2), (Affinity::Before, 1)] {
            let point = model
                .map_point(
                    &PointQuery {
                        model: model.identity(),
                        input: PointInput::Source {
                            target: TextTarget::Flow(flow),
                            byte: SourceBoundary::new(label_start),
                        },
                        affinity,
                    },
                    &mut op,
                )
                .unwrap();
            assert_eq!(
                point.results()[0].chosen,
                Position::Flow(LogicalBoundary::new(expected)),
                "{label:?}/{affinity:?}: {main:?}"
            );
            if affinity == Affinity::After {
                assert!(
                    point.results()[0].effects.synthetic || point.results()[0].effects.atomic,
                    "the generated gap at this shared byte must not claim unique Exact correspondence"
                );
            }
            let empty = model
                .map_range(
                    &RangeQuery {
                        model: model.identity(),
                        input: RangeInput::Source {
                            target: TextTarget::Flow(flow),
                            span: SourceSpan::new(label_start, label_start).unwrap(),
                        },
                        empty_affinity: affinity,
                    },
                    &mut op,
                )
                .unwrap();
            assert_eq!(empty.boundaries()[0].chosen, point.results()[0].chosen);
            assert!(empty.contributions().is_empty());
            let reverse = model
                .map_point(
                    &PointQuery {
                        model: model.identity(),
                        input: PointInput::Flow {
                            flow,
                            utf16: LogicalBoundary::new(expected),
                        },
                        affinity,
                    },
                    &mut op,
                )
                .unwrap();
            assert_eq!(
                reverse.results()[0].chosen,
                Position::Source(SourceBoundary::new(label_start)),
                "{label:?}/{affinity:?}"
            );
        }
        let label_range = model
            .map_range(
                &RangeQuery {
                    model: model.identity(),
                    input: RangeInput::Source {
                        target: TextTarget::Flow(flow),
                        span: SourceSpan::new(label_start, label_end).unwrap(),
                    },
                    empty_affinity: Affinity::After,
                },
                &mut op,
            )
            .unwrap();
        let selected = label_range.selections()[0];
        assert_eq!(
            (selected.start.chosen.value(), selected.end.chosen.value()),
            (2, 2 + label_utf16),
            "{label:?}"
        );
        let ordinary_source = format!("<p>{label}</p>");
        let ordinary = parse(&ordinary_source, true);
        assert_eq!(preview(&ordinary), label);
        let mut ordinary_op = ordinary.budget().begin(CancellationToken::new());
        let ordinary_range = ordinary
            .map_range(
                &RangeQuery {
                    model: ordinary.identity(),
                    input: RangeInput::Source {
                        target: TextTarget::Flow(ordinary.main_flow_id()),
                        span: SourceSpan::new(3, 3 + label.len() as u64).unwrap(),
                    },
                    empty_affinity: Affinity::After,
                },
                &mut ordinary_op,
            )
            .unwrap();
        assert_eq!(
            selected.expanded,
            ordinary_range.selections()[0].expanded,
            "{label:?}: label={:?}, ordinary={:?}",
            label_range.contributions(),
            ordinary_range.contributions()
        );
        let label_content = label_range
            .contributions()
            .iter()
            .filter(|row| row.kind != AtomKind::Hidden)
            .collect::<Vec<_>>();
        assert_eq!(label_content.len(), 1);
        assert_eq!(label_content[0].kind, AtomKind::Copy);
        assert_eq!(
            label_content[0].source,
            SourceSpan::new(label_start, label_end).unwrap()
        );
        let reverse_range = model
            .map_range(
                &RangeQuery {
                    model: model.identity(),
                    input: RangeInput::Flow {
                        flow,
                        span: LogicalSpan::new(2, 2 + label_utf16).unwrap(),
                    },
                    empty_affinity: Affinity::After,
                },
                &mut op,
            )
            .unwrap();
        assert_eq!(reverse_range.contributions().len(), 1);
        assert_eq!(reverse_range.contributions()[0].kind, AtomKind::Copy);
        assert_eq!(
            reverse_range.contributions()[0].source,
            SourceSpan::new(label_start, label_end).unwrap()
        );
        let input_range = model
            .map_range(
                &RangeQuery {
                    model: model.identity(),
                    input: RangeInput::Source {
                        target: TextTarget::Flow(flow),
                        span: SourceSpan::new(input_start, input_end).unwrap(),
                    },
                    empty_affinity: Affinity::After,
                },
                &mut op,
            )
            .unwrap();
        assert_eq!(
            (
                input_range.selections()[0].start.chosen.value(),
                input_range.selections()[0].end.chosen.value()
            ),
            (0, 1)
        );
        assert!(!input_range.selections()[0].expanded);
        assert_eq!(input_range.contributions().len(), 1);
        assert_eq!(input_range.contributions()[0].kind, AtomKind::Object);
    }
}

#[test]
fn checkbox_styled_code_and_image_label_sides_match_ordinary_html_projection() {
    use crate::ir::AtomKind;
    for (body, rendered) in [
        ("<b>Beta</b>", "Beta"),
        ("<code>中</code>", "中"),
        ("<img src='p.png' alt='alt'>", "\u{fffc}"),
    ] {
        let source = format!("<label><input type=checkbox disabled>{body}</label>");
        let ordinary_source = format!("<p>{body}</p>");
        let model = parse(&source, true);
        let ordinary = parse(&ordinary_source, true);
        assert_eq!(preview(&model), format!("\u{fffc} {rendered}"));
        assert_eq!(preview(&ordinary), rendered);
        let body_start = source.find(body).unwrap() as u64;
        let body_end = body_start + body.len() as u64;
        let flow = model.main_flow_id();
        let mut op = model.budget().begin(CancellationToken::new());
        let mut ordinary_op = ordinary.budget().begin(CancellationToken::new());
        let task = model
            .ir()
            .objects()
            .iter()
            .find_map(|object| {
                if let ObjectRow::Task { atom, .. } = object {
                    Some(*atom)
                } else {
                    None
                }
            })
            .unwrap();
        for (affinity, expected) in [(Affinity::Before, 1), (Affinity::After, 2)] {
            let point = model
                .map_point(
                    &PointQuery {
                        model: model.identity(),
                        input: PointInput::Source {
                            target: TextTarget::Flow(flow),
                            byte: SourceBoundary::new(body_start),
                        },
                        affinity,
                    },
                    &mut op,
                )
                .unwrap();
            assert_eq!(
                point.results()[0].chosen.value(),
                expected,
                "{body}/{affinity:?}"
            );
            if affinity == Affinity::After {
                assert!(point.results()[0].effects.synthetic || point.results()[0].effects.atomic);
            }
            let empty = model
                .map_range(
                    &RangeQuery {
                        model: model.identity(),
                        input: RangeInput::Source {
                            target: TextTarget::Flow(flow),
                            span: SourceSpan::new(body_start, body_start).unwrap(),
                        },
                        empty_affinity: affinity,
                    },
                    &mut op,
                )
                .unwrap();
            assert_eq!(empty.boundaries()[0].chosen, point.results()[0].chosen);
        }
        let label_range = model
            .map_range(
                &RangeQuery {
                    model: model.identity(),
                    input: RangeInput::Source {
                        target: TextTarget::Flow(flow),
                        span: SourceSpan::new(body_start, body_end).unwrap(),
                    },
                    empty_affinity: Affinity::After,
                },
                &mut op,
            )
            .unwrap();
        let ordinary_range = ordinary
            .map_range(
                &RangeQuery {
                    model: ordinary.identity(),
                    input: RangeInput::Source {
                        target: TextTarget::Flow(ordinary.main_flow_id()),
                        span: SourceSpan::new(3, 3 + body.len() as u64).unwrap(),
                    },
                    empty_affinity: Affinity::After,
                },
                &mut ordinary_op,
            )
            .unwrap();
        let selected = label_range.selections()[0];
        assert_eq!(
            (selected.start.chosen.value(), selected.end.chosen.value()),
            (2, 2 + rendered.encode_utf16().count() as u64)
        );
        assert_eq!(
            selected.expanded,
            ordinary_range.selections()[0].expanded,
            "{body}: label={:?}, ordinary={:?}",
            label_range.contributions(),
            ordinary_range.contributions()
        );
        assert!(
            label_range
                .contributions()
                .iter()
                .all(|row| row.atom != task && row.kind != AtomKind::Synthetic)
        );
        let visible = label_range
            .contributions()
            .iter()
            .filter(|row| row.kind != AtomKind::Hidden)
            .map(|row| {
                assert!(row.target.start >= 2);
                (
                    row.kind,
                    row.source.start() - body_start,
                    row.source.end() - body_start,
                    row.target.start - 2,
                    row.target.end - 2,
                    row.expanded,
                )
            })
            .collect::<Vec<_>>();
        let ordinary_visible = ordinary_range
            .contributions()
            .iter()
            .filter(|row| row.kind != AtomKind::Hidden)
            .map(|row| {
                (
                    row.kind,
                    row.source.start() - 3,
                    row.source.end() - 3,
                    row.target.start,
                    row.target.end,
                    row.expanded,
                )
            })
            .collect::<Vec<_>>();
        assert_eq!(visible, ordinary_visible, "{body}");
        assert!(!visible.is_empty());
        let reverse = model
            .map_point(
                &PointQuery {
                    model: model.identity(),
                    input: PointInput::Flow {
                        flow,
                        utf16: LogicalBoundary::new(2),
                    },
                    affinity: Affinity::After,
                },
                &mut op,
            )
            .unwrap();
        let ordinary_reverse = ordinary
            .map_point(
                &PointQuery {
                    model: ordinary.identity(),
                    input: PointInput::Flow {
                        flow: ordinary.main_flow_id(),
                        utf16: LogicalBoundary::new(0),
                    },
                    affinity: Affinity::After,
                },
                &mut ordinary_op,
            )
            .unwrap();
        let source_byte = reverse.results()[0].chosen.value();
        assert_eq!(
            source_byte - body_start,
            ordinary_reverse.results()[0].chosen.value() - 3,
            "{body}"
        );
        let roundtrip = model
            .map_point(
                &PointQuery {
                    model: model.identity(),
                    input: PointInput::Source {
                        target: TextTarget::Flow(flow),
                        byte: SourceBoundary::new(source_byte),
                    },
                    affinity: Affinity::After,
                },
                &mut op,
            )
            .unwrap();
        assert_eq!(roundtrip.results()[0].chosen.value(), 2, "{body}");
        let input_start = source.find("<input").unwrap() as u64;
        let input_range = model
            .map_range(
                &RangeQuery {
                    model: model.identity(),
                    input: RangeInput::Source {
                        target: TextTarget::Flow(flow),
                        span: SourceSpan::new(input_start, body_start).unwrap(),
                    },
                    empty_affinity: Affinity::After,
                },
                &mut op,
            )
            .unwrap();
        assert_eq!(
            (
                input_range.selections()[0].start.chosen.value(),
                input_range.selections()[0].end.chosen.value()
            ),
            (0, 1)
        );
        assert!(!input_range.selections()[0].expanded);
        assert_eq!(input_range.contributions().len(), 1);
        assert_eq!(input_range.contributions()[0].atom, task);
    }
}

#[test]
fn checkbox_empty_label_after_edge_retains_ordinary_gfm_right_side() {
    let source = "<label><input type=checkbox disabled></label>";
    let model = parse(source, true);
    let ordinary = parse_config(model.source().text(), Profile::G3, Extensions::HTML_INLINE);
    assert_eq!(preview(&model), "\u{fffc}");
    assert_eq!(preview(&ordinary), preview(&model));
    let fragment = super::fragment::lower(source, false).unwrap();
    let right_byte =
        model.source().text().find("- [ ] ").unwrap() as u64 + fragment.markdown.len() as u64;
    let input_end = source.find("</label>").unwrap() as u64;
    let mut op = model.budget().begin(CancellationToken::new());
    let mut ordinary_op = ordinary.budget().begin(CancellationToken::new());
    let actual = model
        .map_point(
            &PointQuery {
                model: model.identity(),
                input: PointInput::Source {
                    target: TextTarget::Flow(model.main_flow_id()),
                    byte: SourceBoundary::new(input_end),
                },
                affinity: Affinity::After,
            },
            &mut op,
        )
        .unwrap();
    let expected = ordinary
        .map_point(
            &PointQuery {
                model: ordinary.identity(),
                input: PointInput::Source {
                    target: TextTarget::Flow(ordinary.main_flow_id()),
                    byte: SourceBoundary::new(right_byte),
                },
                affinity: Affinity::After,
            },
            &mut ordinary_op,
        )
        .unwrap();
    assert_eq!(
        actual.results()[0].chosen.value(),
        expected.results()[0].chosen.value()
    );
    assert_eq!(
        actual.results()[0].correspondence,
        expected.results()[0].correspondence
    );
    assert_eq!(actual.results()[0].effects, expected.results()[0].effects);
    let empty = model
        .map_range(
            &RangeQuery {
                model: model.identity(),
                input: RangeInput::Source {
                    target: TextTarget::Flow(model.main_flow_id()),
                    span: SourceSpan::new(input_end, input_end).unwrap(),
                },
                empty_affinity: Affinity::After,
            },
            &mut op,
        )
        .unwrap();
    assert_eq!(empty.boundaries()[0].chosen, actual.results()[0].chosen);
}
