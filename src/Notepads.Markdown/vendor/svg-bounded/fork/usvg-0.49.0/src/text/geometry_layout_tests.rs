// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE file in the project root for license information.
// ---------------------------------------------------------------------------------------------
//! Request-owned text geometry. The original archived normal-bit tests remain separate.

use super::*;
use crate::geometry_test_support::{cancelled, native, request, Call, State};
use crate::tree::owner::{Origin, TreeString, TreeVec, TryCopy};
use markdown_svg_budget::{Error, FailureKind, LimitKind, ResourceKind, ResourcesRef};

fn text() -> Text {
    let bounds = tiny_skia_path::Rect::from_xywh(0.0, 0.0, 1.0, 1.0).unwrap();
    Text {
        origin: Origin::Plain,
        id: TreeString::plain("controlled-normal-test".into()),
        rendering_mode: crate::TextRendering::default(),
        dx: crate::NumberList::from_static(&[]),
        dy: crate::NumberList::from_static(&[]),
        rotate: crate::NumberList::from_static(&[]),
        writing_mode: WritingMode::LeftToRight,
        direction: 0,
        chunks: TreeVec::plain(Vec::new()),
        abs_transform: Transform::identity(),
        bounding_box: bounds,
        abs_bounding_box: bounds,
        stroke_bounding_box: bounds,
        abs_stroke_bounding_box: bounds,
        flattened: crate::SceneBox::try_new_with(&crate::ProcessingContext::default(), || {
            Ok(crate::Group::empty())
        })
        .unwrap(),
        layouted: TreeVec::plain(Vec::new()),
    }
}

fn clusters() -> Vec<GlyphCluster> {
    (0..9)
        .map(|i| GlyphCluster {
            byte_idx: ByteIndex::new(i),
            source_end: i + 1,
            logical_parent: None,
            resolved_script: None,
            spacing_eligible: true,
            spacing_after: 0.0,
            inline_scale: 1.0,
            codepoint: 'a',
            width: 0.0,
            advance: 2.0,
            text_length_spacing: 0.0,
            ascent: 7.0,
            descent: -2.0,
            has_relative_shift: i == 3,
            glyphs: TreeVec::plain(Vec::new()),
            transform: Transform::from_translate(i as f32 * 2.0, 3.0),
            path_transform: Transform::identity(),
            visible: true,
        })
        .collect()
}

fn curve() -> tiny_skia_path::Path {
    let mut builder = tiny_skia_path::PathBuilder::new();
    builder.move_to(0.0, 0.0);
    builder.line_to(0.0, 0.0);
    builder.line_to(3.0, 0.0);
    builder.quad_to(6.0, 8.0, 9.0, 0.0);
    builder.cubic_to(12.0, 8.0, 15.0, 8.0, 18.0, 0.0);
    builder.close();
    builder.move_to(100.0, 100.0);
    builder.line_to(110.0, 100.0);
    builder.finish().unwrap()
}

fn bits(values: &[Option<PathNormal>]) -> Vec<Option<[u32; 3]>> {
    values
        .iter()
        .map(|value| value.map(|v| [v.x.to_bits(), v.y.to_bits(), v.angle.to_bits()]))
        .collect()
}

fn retired(resources: &ResourcesRef, state: &State) {
    assert_eq!(state.live_len(), 0);
    assert_eq!(resources.statistics().scratch_bytes, 0);
    assert_eq!(resources.statistics().live_leases, 0);
    assert_eq!(resources.statistics().release_failures, 0);
    resources.validate_retired().unwrap();
}

fn span() -> TextSpan {
    TextSpan {
        computed_color: svgtypes::Color::black(),
        source_owner: 1,
        source_ancestors: TreeVec::plain(vec![1]),
        length_directives: TreeVec::plain(Vec::new()),
        decoration_owners: [None; 3],
        decoration_sources: [None, None, None],
        start: 0,
        end: 9,
        fill: Some(Fill::default()),
        stroke: None,
        paint_order: PaintOrder::default(),
        font: crate::Font {
            families: TreeVec::plain(vec![crate::FontFamily::Named(TreeString::plain(
                "Tuffy".into(),
            ))]),
            style: crate::FontStyle::default(),
            stretch: crate::FontStretch::default(),
            weight: 400,
            variations: TreeVec::plain(Vec::new()),
        },
        font_size: NonZeroPositiveF32::new(16.0).unwrap(),
        small_caps: false,
        apply_kerning: true,
        font_optical_sizing: crate::FontOpticalSizing::default(),
        language: TreeString::plain("und".into()),
        direction: 0,
        bidi_scopes: TreeVec::plain(Vec::new()),
        decoration: crate::TextDecoration {
            underline: None,
            overline: None,
            line_through: None,
        },
        dominant_baseline: DominantBaseline::default(),
        alignment_baseline: AlignmentBaseline::default(),
        baseline_shift: TreeVec::plain(Vec::new()),
        visible: true,
        letter_spacing: 0.0,
        word_spacing: 0.0,
        text_length: None,
        length_adjust: LengthAdjust::default(),
    }
}

#[test]
fn controlled_normals_match_every_ordinary_field_for_compat_and_same_request_paths() {
    let source = curve();
    let clusters = clusters();
    let shifts = vec![(0.0, 0.0); clusters.len()];
    for transform in [
        Transform::identity(),
        Transform::from_scale(0.25, 0.75),
        Transform::from_scale(3.0, 7.0),
        Transform::from_row(2.0, 0.25, -0.5, 3.0, 1.0, 2.0),
    ] {
        let mut text = text();
        text.abs_transform = transform;
        let expected = collect_normals(&text, &clusters, &shifts, &source, -1.0);
        for paid_input in [false, true] {
            let (resources, state) = request(Default::default());
            let processing = crate::ProcessingContext::with_resources(resources.clone());
            let path = if paid_input {
                source
                    .try_copy_in(&resources, ResourceKind::Geometry)
                    .unwrap()
            } else {
                source.clone()
            };
            let values =
                collect_normals_with(&text, &clusters, &shifts, &path, -1.0, &processing).unwrap();
            assert!(matches!(values, GeometryVec::Paid(_)));
            assert_eq!(bits(&values), bits(&expected));
            assert!(state.counts().checkpoint > 30);
            drop(values);
            drop(path);
            drop(processing);
            retired(&resources, &state);
        }
    }
}

#[test]
fn text_path_accuracy_keeps_valid_f32_bits_and_recovers_finite_extremes() {
    let processing = crate::ProcessingContext::default();
    for transform in [
        Transform::identity(),
        Transform::from_scale(0.0, 0.0),
        Transform::from_scale(1.0, 8.0),
        Transform::from_row(3.0, 4.0, 12.0, 5.0, 0.0, 0.0),
    ] {
        let (sx, sy) = transform.get_scale();
        let expected = (0.5f32 / (sx * sy).sqrt().max(1.0)) as f64;
        assert_eq!(
            text_path_accuracy(transform, &processing)
                .unwrap()
                .to_bits(),
            expected.to_bits()
        );
    }
    for transform in [
        Transform::from_scale(f32::MAX, f32::MAX),
        Transform::from_scale(1.0e20, 1.0e20),
        Transform::from_scale(f32::MAX, 1.0e-30),
        Transform::from_scale(f32::from_bits(1), f32::MAX),
        Transform::from_row(f32::MAX, f32::MAX, f32::MAX, f32::MAX, 0.0, 0.0),
    ] {
        let value = text_path_accuracy(transform, &processing).unwrap();
        let nx = (transform.sx as f64).hypot(transform.kx as f64);
        let ny = (transform.ky as f64).hypot(transform.sy as f64);
        let expected = 0.5 / (nx.sqrt() * ny.sqrt()).max(1.0);
        assert_eq!(value.to_bits(), expected.to_bits());
        assert!(value.is_finite() && value > 1.0e-40 && value <= 0.5);
    }
    let (resources, state) = request(Default::default());
    let processing = crate::ProcessingContext::with_resources(resources.clone());
    let error =
        text_path_accuracy(Transform::from_scale(f32::INFINITY, 1.0), &processing).unwrap_err();
    assert_eq!(error.kind, FailureKind::Decode);
    assert_eq!(resources.failure(), Some(error));
    drop(processing);
    retired(&resources, &state);
}

#[test]
fn foreign_paid_text_path_stops_before_output_allocation_and_preserves_other_request() {
    let (source_resources, source_state) = request(Default::default());
    let source = curve()
        .try_copy_in(&source_resources, ResourceKind::Geometry)
        .unwrap();
    let (resources, state) = request(Default::default());
    let processing = crate::ProcessingContext::with_resources(resources.clone());
    let error = collect_normals_with(
        &text(),
        &clusters(),
        &[(0.0, 0.0); 9],
        &source,
        0.0,
        &processing,
    )
    .err()
    .expect("foreign paid path must fail");
    assert_eq!(error.kind, FailureKind::Invariant);
    assert_eq!(resources.failure(), Some(error));
    assert_eq!(state.counts().reserve, 0);
    assert!(source_resources.failure().is_none());
    drop(source);
    drop(processing);
    retired(&resources, &state);
    retired(&source_resources, &source_state);
}

#[test]
fn normals_transport_every_reserve_commit_and_j_work_failure_without_next_consumer() {
    let source = curve();
    let clusters = clusters();
    let shifts = [(0.0, 0.0); 9];
    let (resources, state) = request(Default::default());
    let processing = crate::ProcessingContext::with_resources(resources.clone());
    let normal =
        collect_normals_with(&text(), &clusters, &shifts, &source, 0.0, &processing).unwrap();
    let baseline = state.counts();
    drop(normal);
    drop(processing);
    retired(&resources, &state);
    assert_eq!((baseline.reserve, baseline.commit), (2, 2));
    assert!(
        baseline.checkpoint > 40,
        "actual curve solver work must be controlled"
    );
    let mut cases = 0;
    for (call, count, errors) in [
        (Call::Reserve, baseline.reserve, vec![native(-810)]),
        (Call::Commit, baseline.commit, vec![native(-811)]),
        (
            Call::Checkpoint,
            baseline.checkpoint,
            vec![cancelled(), Error::new(FailureKind::Quota, LimitKind::Work)],
        ),
    ] {
        for at in 1..=count {
            for error in &errors {
                let (resources, state) = request(Default::default());
                state.arm(call, at, *error);
                let processing = crate::ProcessingContext::with_resources(resources.clone());
                let actual =
                    collect_normals_with(&text(), &clusters, &shifts, &source, 0.0, &processing)
                        .err()
                        .expect("armed real callback must stop normals");
                assert_eq!(actual, *error, "{call:?} {at}");
                assert_eq!(resources.failure(), Some(*error));
                let ledger = state.lock();
                let index = ledger
                    .events
                    .iter()
                    .position(|event| event.call == call && event.ordinal == at)
                    .unwrap();
                assert!(ledger.events[index + 1..]
                    .iter()
                    .all(|event| event.call == Call::Release));
                drop(ledger);
                drop(processing);
                retired(&resources, &state);
                cases += 1;
            }
        }
    }
    println!(
        "TEXT_NORMAL_FAULTS reserves={} commits={} checkpoints={} actual_cases={cases}",
        baseline.reserve, baseline.commit, baseline.checkpoint
    );
}

#[test]
fn offsets_cleanup_failure_prevents_normals_publication_and_keeps_failed_ticket() {
    let (resources, state) = request(Default::default());
    state.arm(Call::Release, 1, native(-812));
    let processing = crate::ProcessingContext::with_resources(resources.clone());
    assert!(collect_normals_with(
        &text(),
        &clusters(),
        &[(0.0, 0.0); 9],
        &curve(),
        0.0,
        &processing
    )
    .is_err());
    assert_eq!(resources.failure(), Some(native(-812)));
    assert_eq!(state.live_len(), 1);
    assert_eq!(resources.statistics().release_failures, 1);
    assert_eq!(resources.statistics().live_leases, 1);
    drop(processing);
    assert!(resources.validate_retired().is_err());
    assert_eq!(
        state.counts().release,
        2,
        "unpublished normals physically retire too"
    );
}

#[test]
fn span_metrics_paid_bboxes_match_ordinary_and_cleanup_cannot_publish_fragments() {
    let span = span();
    let clusters = clusters();
    let transform = Transform::from_row(1.3, 0.2, -0.7, 2.0, 8.0, 9.0);
    let expected = convert_span_inner(
        &span,
        &clusters,
        transform,
        None,
        &crate::ProcessingContext::default(),
    )
    .unwrap()
    .unwrap();
    let (resources, state) = request(Default::default());
    let processing = crate::ProcessingContext::with_resources(resources.clone());
    let clusters = clusters
        .iter()
        .map(|c| {
            let mut v = c.try_copy_in(&crate::ProcessingContext::default()).unwrap();
            v.glyphs = TreeVec::new(&processing);
            v
        })
        .collect::<Vec<_>>();
    let actual = convert_span_inner(&span, &clusters, transform, None, &processing)
        .unwrap()
        .unwrap();
    assert_eq!(actual.0.len(), expected.0.len());
    assert_eq!(actual.1, expected.1);
    let baseline = state.counts();
    drop(actual);
    drop(clusters);
    assert!(baseline.reserve >= 3 && baseline.release >= 3);
    assert_eq!(state.live_len(), 0);
    drop(processing);
    retired(&resources, &state);
    let clusters = super::geometry_layout_tests::clusters();
    for call in [Call::Reserve, Call::Commit, Call::Release] {
        let count = match call {
            Call::Reserve => baseline.reserve,
            Call::Commit => baseline.commit,
            Call::Release => baseline.release,
            _ => unreachable!(),
        };
        for at in 1..=count {
            let (resources, state) = request(Default::default());
            let processing = crate::ProcessingContext::with_resources(resources.clone());
            let clusters = clusters
                .iter()
                .map(|c| GlyphCluster {
                    byte_idx: c.byte_idx,
                    source_end: c.source_end,
                    logical_parent: c.logical_parent,
                    resolved_script: c.resolved_script,
                    spacing_eligible: c.spacing_eligible,
                    spacing_after: c.spacing_after,
                    inline_scale: c.inline_scale,
                    codepoint: c.codepoint,
                    width: c.width,
                    advance: c.advance,
                    text_length_spacing: c.text_length_spacing,
                    ascent: c.ascent,
                    descent: c.descent,
                    has_relative_shift: c.has_relative_shift,
                    glyphs: TreeVec::new(&processing),
                    transform: c.transform,
                    path_transform: c.path_transform,
                    visible: c.visible,
                })
                .collect::<Vec<_>>();
            state.arm(call, at, native(-813));
            let result = convert_span_inner(&span, &clusters, transform, None, &processing);
            assert!(
                matches!(result, Err(error) if error == native(-813)),
                "{call:?} {at}"
            );
            assert_eq!(resources.failure(), Some(native(-813)));
            drop(clusters);
            drop(processing);
            if call == Call::Release {
                assert_eq!(state.live_len(), 1);
                assert_eq!(resources.statistics().release_failures, 1);
                assert!(resources.validate_retired().is_err());
            } else {
                retired(&resources, &state);
            }
        }
    }
    let (resources, state) = request(Default::default());
    let processing = crate::ProcessingContext::with_resources(resources.clone());
    let intervals = collect_decoration_spans(&span, &clusters, &processing).unwrap();
    assert!(matches!(intervals, GeometryVec::Paid(_)));
    assert_eq!(
        intervals.len(),
        2,
        "the original relative-shift split stays intact"
    );
    assert_eq!(
        intervals.iter().map(|v| v.width).collect::<Vec<_>>(),
        [6.0, 12.0]
    );
    drop(intervals);
    drop(processing);
    retired(&resources, &state);
}

#[test]
fn decoration_intervals_and_geometry_use_paid_storage_and_keep_original_geometry() {
    let mut db = fontdb::Database::new();
    db.load_font_data(
        include_bytes!("../../../../dependencies/fontdb-0.24.0/tests/fonts/Tuffy.ttf").to_vec(),
    );
    let id = db.faces().next().unwrap().id;
    let font = db
        .load_font(id, &[], None, &crate::ProcessingContext::default())
        .unwrap();
    let intervals = [
        DecorationSpan {
            width: 40.0,
            transform: Transform::from_translate(1.0, 2.0),
        },
        DecorationSpan {
            width: 10.0,
            transform: Transform::from_rotate(15.0),
        },
    ];
    let decoration = TextDecorationStyle {
        fill: Some(Fill::default()),
        stroke: None,
    };
    let transform = Transform::from_row(1.5, 0.2, -0.3, 1.2, 2.0, 7.0);
    let expected = convert_decoration_values(
        3.0,
        16.0,
        true,
        &font,
        decoration
            .try_copy_in(&crate::ProcessingContext::default())
            .unwrap(),
        &intervals,
        transform,
        &crate::ProcessingContext::default(),
    )
    .unwrap()
    .unwrap();
    let (resources, state) = request(Default::default());
    let processing = crate::ProcessingContext::with_resources(resources.clone());
    let path = convert_decoration_values(
        3.0,
        16.0,
        true,
        &font,
        decoration,
        &intervals,
        transform,
        &processing,
    )
    .unwrap()
    .unwrap();
    assert!(path.data.is_paid());
    assert!(path.data.belongs_to(&resources));
    assert_eq!(path.data.retained_unpaid_capacity_bytes(), Some(0));
    assert_eq!(path.data.verbs(), expected.data.verbs());
    assert_eq!(path.data.points(), expected.data.points());
    assert_eq!(path.bounding_box, expected.bounding_box);
    let counts = state.counts();
    assert!(counts.reserve > 5 && counts.checkpoint > 20);
    drop(path);
    drop(processing);
    retired(&resources, &state);
    println!(
        "TEXT_DECORATION_BASELINE reserves={} commits={} checkpoints={}",
        counts.reserve, counts.commit, counts.checkpoint
    );
}
