// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE file in the project root for license information.
// ---------------------------------------------------------------------------------------------
//! Background geometry regression only; not complete SVG or Native qualification.

use super::background_path;
use crate::geometry_test_support::{native, request, Call};
use crate::{Node, Options, Paint, Path, ProcessingContext, Rect, Tree};
use markdown_svg_budget::{Error, Limits, ResourceKind};
use std::sync::Arc;

const DOCUMENT: &str = "<svg xmlns='http://www.w3.org/2000/svg' width='32' height='32' background-color='#112233'><rect id='foreground' x='4' y='6' width='8' height='10' fill='#abcdef'/></svg>";

fn rectangle() -> Rect {
    Rect::from_xywh(0.0, 0.0, 32.0, 32.0).unwrap()
}

fn rect_bits(rect: Rect) -> [u32; 4] {
    [
        rect.x().to_bits(),
        rect.y().to_bits(),
        rect.width().to_bits(),
        rect.height().to_bits(),
    ]
}

#[derive(Debug, PartialEq)]
struct Observation {
    verbs: Vec<tiny_skia_path::PathVerb>,
    points: Vec<[u32; 2]>,
    bounds: [[u32; 4]; 5],
    fill_rgb: [u8; 3],
    fill_alpha: u32,
    fill_rule: crate::FillRule,
    transform: [u32; 6],
    visible: bool,
    paint_order: crate::PaintOrder,
    rendering: crate::ShapeRendering,
    stroke_absent: bool,
}

fn observe(path: &Path) -> Observation {
    let fill = path.fill().expect("background has a fill");
    let Paint::Color(color) = fill.paint() else {
        panic!("background is a solid color")
    };
    let ts = path.abs_transform();
    Observation {
        verbs: path.data().verbs().to_vec(),
        points: path
            .data()
            .points()
            .iter()
            .map(|p| [p.x.to_bits(), p.y.to_bits()])
            .collect(),
        bounds: [
            path.data().bounds(),
            path.bounding_box(),
            path.abs_bounding_box(),
            path.stroke_bounding_box(),
            path.abs_stroke_bounding_box(),
        ]
        .map(rect_bits),
        fill_rgb: [color.red, color.green, color.blue],
        fill_alpha: fill.opacity().get().to_bits(),
        fill_rule: fill.rule(),
        transform: [ts.sx, ts.ky, ts.kx, ts.sy, ts.tx, ts.ty].map(f32::to_bits),
        visible: path.is_visible(),
        paint_order: path.paint_order(),
        rendering: path.rendering_mode(),
        stroke_absent: path.stroke().is_none(),
    }
}

fn all_paths<'a>(group: &'a crate::Group, output: &mut Vec<&'a Path>) {
    for node in group.children() {
        match node {
            Node::Path(path) => output.push(path),
            Node::Group(group) => all_paths(group, output),
            _ => panic!("this small fixture contains only groups and paths"),
        }
    }
}

fn assert_paid(path: &Path, resources: &markdown_svg_budget::ResourcesRef) {
    assert!(
        path.data.belongs_to(resources),
        "outer SharedOwner is paid by the original request"
    );
    assert!(
        path.data().belongs_to(resources),
        "inner PPath storage is paid by the original request"
    );
    assert_eq!(path.data().retained_unpaid_capacity_bytes(), Some(0));
}

#[test]
fn ordinary_and_controlled_background_keep_full_geometry_fill_and_last_alias_funding() {
    for area in [
        rectangle(),
        Rect::from_xywh(-2.5, 3.25, 10.5, 20.75).unwrap(),
    ] {
        for alpha in [0, 91, 255] {
            let color = svgtypes::Color::new_rgba(17, 34, 51, alpha);
            let ordinary = background_path(color, area, &ProcessingContext::default())
                .unwrap()
                .unwrap();
            let (r, state) = request(Limits::default());
            let processing = ProcessingContext::with_resources(r.clone());
            let paid = background_path(color, area, &processing).unwrap().unwrap();
            let actual = observe(&paid);
            assert_eq!(actual, observe(&ordinary));
            assert_eq!(
                paid.data().verbs(),
                &[
                    tiny_skia_path::PathVerb::Move,
                    tiny_skia_path::PathVerb::Line,
                    tiny_skia_path::PathVerb::Line,
                    tiny_skia_path::PathVerb::Line,
                    tiny_skia_path::PathVerb::Close,
                ]
            );
            assert_eq!(
                actual.points,
                [
                    [area.left().to_bits(), area.top().to_bits()],
                    [area.right().to_bits(), area.top().to_bits()],
                    [area.right().to_bits(), area.bottom().to_bits()],
                    [area.left().to_bits(), area.bottom().to_bits()],
                ]
            );
            assert_eq!(actual.bounds, [rect_bits(area); 5]);
            assert_eq!(actual.fill_rgb, [17, 34, 51]);
            assert_eq!(actual.fill_alpha, (alpha as f32 / 255.0).to_bits());
            assert_paid(&paid, &r);
            let fee = state.kind_bytes(ResourceKind::Geometry);
            let leases = r.statistics().live_leases;
            assert!(fee > 0 && leases >= 4);
            assert_eq!(r.statistics().scratch_bytes, 0);
            state.assert_account(&r);
            let alias = paid
                .try_clone_in(&ProcessingContext::with_resources(r.clone()))
                .unwrap();
            let releases = state.counts().release;
            drop(paid);
            assert_eq!(state.counts().release, releases);
            assert_eq!(state.kind_bytes(ResourceKind::Geometry), fee);
            assert_eq!(r.statistics().live_leases, leases);
            assert_paid(&alias, &r);
            drop(alias);
            state.assert_account(&r);
            assert_eq!(state.live_len(), 0);
            drop(processing);
            r.validate_retired().unwrap();
        }
    }
    println!(
        "BACKGROUND_DIRECT cases=6 completeVerbsAndPoints=true allBoundsAndFill=true outerAndInnerSameRequest=true lastAliasPaid=true"
    );
}

#[test]
fn actual_tree_background_matches_ordinary_and_both_paths_keep_paid_geometry() {
    let ordinary = Tree::from_str(DOCUMENT, &Options::default()).unwrap();
    let mut original_paths = vec![];
    all_paths(ordinary.root(), &mut original_paths);
    assert_eq!(original_paths.len(), 2);
    let expected: Vec<_> = original_paths.iter().map(|path| observe(path)).collect();
    let (r, state) = request(Limits::default());
    let mut options = Options::default();
    options.processing = Arc::new(ProcessingContext::with_resources(r.clone()));
    let tree = Tree::from_data_controlled(DOCUMENT.as_bytes(), &options).unwrap();
    let mut paths = vec![];
    all_paths(tree.root(), &mut paths);
    assert_eq!(paths.len(), 2);
    assert_eq!(paths[0].id(), "");
    assert_eq!(paths[1].id(), "foreground");
    let actual: Vec<_> = paths.iter().map(|path| observe(path)).collect();
    assert_eq!(actual, expected);
    assert_eq!(actual[0].bounds, [rect_bits(rectangle()); 5]);
    assert_eq!(actual[0].fill_rgb, [17, 34, 51]);
    assert_eq!(actual[0].fill_alpha, 1.0f32.to_bits());
    for path in paths {
        assert_paid(path, &r);
    }
    assert!(state.kind_bytes(ResourceKind::Geometry) > 0);
    state.assert_account(&r);
    drop(tree);
    drop(options);
    assert_eq!(state.live_len(), 0);
    state.assert_account(&r);
    r.validate_retired().unwrap();
    println!(
        "BACKGROUND_TREE paths=2 fullOrdinaryControlledObservationsEqual=true backgroundAttribute=#112233"
    );
}

#[derive(Clone, Copy, Debug)]
enum Entry {
    Direct,
    Tree,
}

enum Candidate {
    Direct(Path),
    Tree(Tree),
}

fn build(entry: Entry, processing: &Arc<ProcessingContext>) -> Result<Candidate, Error> {
    match entry {
        Entry::Direct => background_path(
            svgtypes::Color::new_rgba(17, 34, 51, 91),
            rectangle(),
            processing,
        )
        .map(|candidate| {
            Candidate::Direct(candidate.expect("positive finite background rectangle"))
        }),
        Entry::Tree => {
            let mut options = Options::default();
            options.processing = processing.clone();
            match Tree::from_data_controlled(DOCUMENT.as_bytes(), &options) {
                Ok(tree) => Ok(Candidate::Tree(tree)),
                Err(crate::Error::Resource(error)) => Err(error),
                Err(crate::Error::ParsingFailed(roxmltree::Error::Resource(error))) => Err(error),
                Err(crate::Error::ProcessingFailure(error)) => Err(error.typed_failure),
                Err(error) => {
                    panic!("unexpected syntax/origin error in the fixed valid fixture: {error:?}")
                }
            }
        }
    }
}

#[test]
fn every_real_reserve_commit_and_last_lease_fault_keeps_typed_failure_and_rejects_candidate() {
    for entry in [Entry::Direct, Entry::Tree] {
        let (r, state) = request(Limits::default());
        let processing = Arc::new(ProcessingContext::with_resources(r.clone()));
        let candidate = build(entry, &processing)
            .ok()
            .expect("baseline background succeeds");
        match &candidate {
            Candidate::Direct(path) => assert_paid(path, &r),
            Candidate::Tree(tree) => assert_eq!(tree.size().width(), 32.0),
        }
        let baseline = state.counts();
        drop(candidate);
        let retired = state.counts();
        assert!(
            retired.release > baseline.release,
            "last retained lease retires after successful candidate delivery"
        );
        assert_eq!(state.live_len(), 0);
        drop(processing);
        r.validate_retired().unwrap();
        for (call, count, error) in [
            (Call::Reserve, baseline.reserve, native(-951)),
            (Call::Commit, baseline.commit, native(-952)),
        ] {
            for ordinal in 1..=count {
                let (r, state) = request(Limits::default());
                let processing = Arc::new(ProcessingContext::with_resources(r.clone()));
                state.arm(call, ordinal, error);
                assert_eq!(
                    build(entry, &processing).err(),
                    Some(error),
                    "{entry:?} {call:?} ordinal={ordinal}"
                );
                assert_eq!(r.failure(), Some(error));
                assert_eq!(processing.resource_failure(), Some(error));
                assert_eq!(state.live_len(), 0);
                state.assert_account(&r);
                drop(processing);
                r.validate_retired().unwrap();
            }
            println!("BACKGROUND_FAULT {entry:?} {call:?} count={count}");
        }
        let (r, state) = request(Limits::default());
        let processing = Arc::new(ProcessingContext::with_resources(r.clone()));
        let error = native(-953);
        state.arm(Call::Release, retired.release, error);
        let candidate = build(entry, &processing)
            .ok()
            .expect("retained owner not retired yet");
        assert_eq!(r.failure(), None);
        drop(candidate);
        assert_eq!(r.failure(), Some(error));
        assert_eq!(processing.resource_failure(), Some(error));
        assert_eq!(
            state.live_len(),
            1,
            "failed final refund retains the canonical lease"
        );
        assert_eq!(r.statistics().release_failures, 1);
        state.assert_account(&r);
        let counts = state.counts();
        assert_eq!(
            build(entry, &processing).err(),
            Some(error),
            "no new successful candidate after retirement fault"
        );
        assert_eq!(state.counts().reserve, counts.reserve);
        assert_eq!(state.counts().commit, counts.commit);
        assert_eq!(state.counts().checkpoint, counts.checkpoint);
        assert_eq!(r.record(native(-954)), error);
        drop(processing);
        assert_eq!(r.validate_retired(), Err(error));
        println!(
            "BACKGROUND_LAST_RELEASE {entry:?} ordinal={} typedFirstRetained=true nextCandidateRejected=true",
            retired.release
        );
    }
}
