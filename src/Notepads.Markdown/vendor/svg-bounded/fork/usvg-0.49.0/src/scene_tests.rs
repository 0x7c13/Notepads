// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE file in the project root for license information.
// ---------------------------------------------------------------------------------------------
//! Private scene bridge tests. They do not qualify Clone allocations or SVG.

use super::*;
use crate::tree::owner::{Origin, PolicyValue, TreeString, TreeVec, TryCopy};
use markdown_svg_budget::{
    Backend, BudgetVec, FirstFailureRef, Limits, ResourceKind, ResourcesRef, Ticket,
};
use std::sync::atomic::{AtomicBool, AtomicU32, AtomicU64, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};

thread_local! {static SCAN_PHASE:std::cell::Cell<bool>=const{std::cell::Cell::new(false)};}
pub(super) struct ScanPhase(bool);
impl ScanPhase {
    pub(super) fn enter() -> Self {
        Self(SCAN_PHASE.with(|p| p.replace(true)))
    }
}
impl Drop for ScanPhase {
    fn drop(&mut self) {
        SCAN_PHASE.with(|p| p.set(self.0));
    }
}
#[derive(Default)]
struct State {
    checkpoints: AtomicUsize,
    work: AtomicU64,
    scan_checkpoints: AtomicUsize,
    scan_work: AtomicU64,
    scan_fault: Mutex<Option<(usize, Error)>>,
    next_ticket: AtomicU32,
    release_error: AtomicBool,
    fault: Mutex<Option<(usize, Error)>>,
}

struct Host(Arc<State>);

impl Backend for Host {
    fn reserve(&self, _: ResourceKind, _: usize) -> Result<Ticket, Error> {
        Ticket::new(1, self.0.next_ticket.fetch_add(1, Ordering::Relaxed), 1)
    }
    fn allocation_committed(&self, _: &Ticket, _: usize) -> Result<(), Error> {
        Ok(())
    }
    fn release_after_free(&self, _: Ticket) -> Result<(), Error> {
        if self.0.release_error.load(Ordering::Relaxed) {
            Err(Error::native(-9201))
        } else {
            Ok(())
        }
    }
    fn checkpoint(&self, work: u64) -> Result<(), Error> {
        let index = self.0.checkpoints.fetch_add(1, Ordering::Relaxed) + 1;
        self.0.work.fetch_add(work, Ordering::Relaxed);
        if SCAN_PHASE.with(|p| p.get()) {
            let index = self.0.scan_checkpoints.fetch_add(1, Ordering::Relaxed) + 1;
            self.0.scan_work.fetch_add(work, Ordering::Relaxed);
            if let Some((at, error)) = *self.0.scan_fault.lock().unwrap() {
                if at == index {
                    return Err(error);
                }
            }
        }
        match *self.0.fault.lock().unwrap() {
            Some((at, error)) if at == index => Err(error),
            _ => Ok(()),
        }
    }
}

fn request(limits: Limits) -> (ResourcesRef, ProcessingContext, Arc<State>) {
    let state = Arc::new(State::default());
    let r = ResourcesRef::new_prepaid(Host(state.clone()), FirstFailureRef::new_prepaid(), limits);
    let p = ProcessingContext::with_resources(r.clone());
    (r, p, state)
}

fn totals(r: &ResourcesRef) -> (u64, u64) {
    let s = r.statistics();
    (s.expanded_nodes, s.converted_segments)
}

// Legal, same-policy owner fixtures. These private factories deliberately do
// not admit scene identities; each test still measures its original copy gate.
fn fixture_box<T: PolicyValue>(value: T, p: &ProcessingContext) -> crate::SceneBox<T> {
    crate::SceneBox::try_new_with(p, || Ok(value)).unwrap()
}
fn path_with_verbs(length: usize, p: &ProcessingContext) -> Path {
    assert!(length >= 2);
    let mut builder = match p.resources() {
        Some(r) => tiny_skia_path::PathBuilder::try_with_capacity(
            r,
            ResourceKind::Geometry,
            length,
            length,
        )
        .unwrap(),
        None => tiny_skia_path::PathBuilder::with_capacity(length, length),
    };
    builder.move_to(-0.0, 1.0);
    for i in 1..length {
        builder.line_to(i as f32, (i & 1) as f32);
    }
    let data = builder.try_finish().unwrap().unwrap();
    let data = crate::SharedOwner::try_new(data, p.resources(), ResourceKind::Geometry).unwrap();
    Path::new_shared(
        "",
        true,
        None,
        None,
        crate::PaintOrder::default(),
        crate::ShapeRendering::default(),
        data,
        crate::Transform::default(),
        p.resources(),
    )
    .unwrap()
    .unwrap()
}
fn group_with_paths(count: usize, p: &ProcessingContext) -> Group {
    let source = path_with_verbs(3, p);
    let mut group = Group::empty_in(p);
    for _ in 0..count {
        let path = source.try_copy_in(p).unwrap();
        group
            .children
            .try_push(Node::Path(fixture_box(path, p)), p)
            .unwrap();
    }
    group
}
fn tree_with_root(root: Group, p: &ProcessingContext) -> Tree {
    Tree {
        source_kind: crate::parser::source_kind::SourceKind::GeneralImage,
        size: crate::Size::from_wh(32.0, 32.0).unwrap(),
        natural_size: (32.0, 32.0),
        processing: Arc::new(match p.resources() {
            Some(r) => ProcessingContext::with_resources(r.clone()),
            None => ProcessingContext::default(),
        }),
        root,
        linear_gradients: TreeVec::new(p),
        radial_gradients: TreeVec::new(p),
        patterns: TreeVec::new(p),
        clip_paths: TreeVec::new(p),
        masks: TreeVec::new(p),
        filters: TreeVec::new(p),
        #[cfg(feature = "text")]
        fontdb: Arc::new(fontdb::Database::new()),
        #[cfg(feature = "text")]
        font_replay: None,
    }
}
fn text_with_root(flattened: Group, p: &ProcessingContext) -> Text {
    let bounds = crate::Rect::from_xywh(0.0, 0.0, 1.0, 1.0).unwrap();
    Text {
        origin: Origin::new(p),
        id: TreeString::new(p),
        rendering_mode: crate::TextRendering::default(),
        dx: crate::NumberList::from_static(&[]),
        dy: crate::NumberList::from_static(&[]),
        rotate: crate::NumberList::from_static(&[]),
        writing_mode: crate::WritingMode::LeftToRight,
        direction: 0,
        chunks: TreeVec::new(p),
        abs_transform: crate::Transform::default(),
        bounding_box: bounds,
        abs_bounding_box: bounds,
        stroke_bounding_box: bounds,
        abs_stroke_bounding_box: bounds,
        flattened: fixture_box(flattened, p),
        #[cfg(feature = "text")]
        layouted: TreeVec::new(p),
    }
}

struct ObservedClone {
    group: Group,
    calls: Arc<AtomicUsize>,
}

impl PolicyValue for ObservedClone {
    fn validate_policy(&self, p: &ProcessingContext) -> Result<(), Error> {
        self.group.validate_policy(p)
    }
}
impl TryCopy for ObservedClone {
    fn try_copy_in(&self, p: &ProcessingContext) -> Result<Self, Error> {
        self.calls.fetch_add(1, Ordering::Relaxed);
        Ok(Self {
            group: self.group.try_copy_in(p)?,
            calls: self.calls.clone(),
        })
    }
}

impl SceneOwned for ObservedClone {
    fn scene_cost(&self, p: &ProcessingContext) -> Result<SceneCost, Error> {
        self.group.scene_cost(p)
    }
}

#[test]
fn node_and_segment_limits_gate_factories_before_construction_and_never_refund() {
    let (_, zero, _) = request(Limits {
        expanded_nodes: 0,
        ..Limits::default()
    });
    let first_factory = AtomicUsize::new(0);
    let rejected = boxed(&zero, SceneCost::NODE, || {
        first_factory.fetch_add(1, Ordering::Relaxed);
        Group::empty_in(&zero)
    });
    println!(
        "scene-first-factory-at-zero-limit={}",
        first_factory.load(Ordering::Relaxed)
    );
    assert_eq!(first_factory.load(Ordering::Relaxed), 0);
    assert_eq!(
        rejected.unwrap_err(),
        Error::new(FailureKind::Quota, LimitKind::ExpandedNodes)
    );
    let (r, p, _) = request(Limits {
        expanded_nodes: 2,
        converted_segments: 6,
        ..Limits::default()
    });
    let source = path_with_verbs(3, &p);
    let calls = AtomicUsize::new(0);
    for _ in 0..2 {
        drop(
            boxed(&p, SceneCost::path(&source), || {
                calls.fetch_add(1, Ordering::Relaxed);
                source.try_copy_in(&p).unwrap()
            })
            .unwrap(),
        );
    }
    assert_eq!(totals(&r), (2, 6));
    let error = boxed(&p, SceneCost::path(&source), || {
        calls.fetch_add(1, Ordering::Relaxed);
        source.try_copy_in(&p).unwrap()
    })
    .unwrap_err();
    assert_eq!(
        error,
        Error::new(FailureKind::Quota, LimitKind::ExpandedNodes)
    );
    assert_eq!(calls.load(Ordering::Relaxed), 2);
    assert_eq!(totals(&r), (2, 6));
    drop(source);
    drop(p);
    r.validate_retired().unwrap();

    let (r, p, _) = request(Limits {
        converted_segments: 2,
        ..Limits::default()
    });
    let source = path_with_verbs(3, &p);
    let mut entered = false;
    assert_eq!(
        boxed(&p, SceneCost::path(&source), || {
            entered = true;
            source.try_copy_in(&p).unwrap()
        })
        .unwrap_err(),
        Error::new(FailureKind::Quota, LimitKind::ConvertedSegments)
    );
    assert!(!entered);
    assert_eq!(totals(&r), (0, 0));
}

#[test]
fn root_move_and_pattern_style_wrapper_charge_only_new_identity() {
    let (r, p, _) = request(Limits::default());
    let child = boxed(&p, SceneCost::NODE, || Group::empty_in(&p)).unwrap();
    let owned = root(&p, || {
        let mut value = Group::empty_in(&p);
        value.children.try_push(Node::Group(child), &p).unwrap();
        value
    })
    .unwrap();
    assert_eq!(totals(&r), (2, 0));
    // Provenance: `owned` was admitted by root immediately above.
    let mut moved = moved_root(&p, owned).unwrap();
    assert_eq!(totals(&r), (2, 0));
    let outer = root(&p, || Group::empty_in(&p)).unwrap();
    let old = core::mem::replace(moved.as_mut(), outer);
    moved
        .as_mut()
        .children
        .try_push(Node::Group(fixture_box(old, &p)), &p)
        .unwrap();
    assert_eq!(totals(&r), (3, 0));
    drop(moved);
    assert_eq!(totals(&r), (3, 0));
}

#[test]
fn owned_clone_is_prepaid_once_and_quotas_reject_before_real_clone() {
    let calls = Arc::new(AtomicUsize::new(0));
    let (_, zero, _) = request(Limits {
        expanded_nodes: 0,
        ..Limits::default()
    });
    let source = ObservedClone {
        group: group_with_paths(2, &zero),
        calls: calls.clone(),
    };
    assert!(clone_owned(&zero, &source).is_err());
    assert_eq!(calls.load(Ordering::Relaxed), 0);
    drop(source);
    let (r, p, _) = request(Limits {
        expanded_nodes: 3,
        converted_segments: 6,
        ..Limits::default()
    });
    let source = ObservedClone {
        group: group_with_paths(2, &p),
        calls: calls.clone(),
    };
    let cloned = clone_boxed(&p, &source).unwrap();
    assert_eq!(calls.load(Ordering::Relaxed), 1);
    assert_eq!(totals(&r), (3, 6));
    for (a, b) in source.group.children().iter().zip(cloned.group.children()) {
        let (Node::Path(a), Node::Path(b)) = (a, b) else {
            unreachable!()
        };
        assert_eq!(a.data().verbs(), b.data().verbs());
        assert_eq!(a.data().points(), b.data().points());
        assert_eq!(a.data().points().as_ptr(), b.data().points().as_ptr());
    }
    assert!(clone_owned(&p, &source).is_err());
    assert_eq!(calls.load(Ordering::Relaxed), 1);
    assert_eq!(totals(&r), (3, 6));
}

#[test]
fn scan_counts_embedded_svg_and_text_flattened_but_not_arc_resources() {
    let (r, p, _) = request(Limits::default());
    let mut shared_root = group_with_paths(5, &p);
    let pattern = crate::SharedOwner::try_new(
        crate::Pattern {
            id: crate::NonEmptyString::try_new("shared-pattern", &p)
                .unwrap()
                .unwrap(),
            units: crate::Units::UserSpaceOnUse,
            content_units: crate::Units::UserSpaceOnUse,
            transform: crate::Transform::default(),
            rect: crate::NonZeroRect::from_xywh(0.0, 0.0, 2.0, 2.0).unwrap(),
            view_box: None,
            root: group_with_paths(7, &p),
        },
        p.resources(),
        ResourceKind::Scratch,
    )
    .unwrap();
    let Node::Path(path) = &mut shared_root.children[0] else {
        unreachable!()
    };
    path.as_mut().fill = Some(crate::Fill {
        paint: crate::Paint::Pattern(pattern.try_alias_in(&p).unwrap()),
        opacity: crate::Opacity::ONE,
        rule: crate::FillRule::default(),
        context_element: None,
    });
    let tree = tree_with_root(shared_root, &p);
    let mut root = Group::empty_in(&p);
    root.children
        .try_push(
            Node::Image(fixture_box(
                Image {
                    origin: Origin::new(&p),
                    id: TreeString::new(&p),
                    visible: true,
                    size: crate::Size::from_wh(32.0, 32.0).unwrap(),
                    rendering_mode: crate::ImageRendering::default(),
                    kind: ImageKind::SVG(tree),
                    abs_transform: crate::Transform::default(),
                    abs_bounding_box: crate::NonZeroRect::from_xywh(0.0, 0.0, 32.0, 32.0).unwrap(),
                },
                &p,
            )),
            &p,
        )
        .unwrap();
    root.children
        .try_push(
            Node::Text(fixture_box(text_with_root(group_with_paths(2, &p), &p), &p)),
            &p,
        )
        .unwrap();
    // outer root1 + image1 + SVGroot1 + paths5 + text1 + flattened1 + paths2.
    assert_eq!(
        root.scene_cost(&p).unwrap(),
        SceneCost {
            nodes: 12,
            segments: 21
        }
    );
    let cloned = clone_owned(&p, &root).unwrap();
    assert_eq!(totals(&r), (12, 21));
    assert_eq!(cloned.scene_cost(&p).unwrap().nodes, 12);
    // Explicitly cloning a resource root is different from aliasing its Arc.
    assert_eq!(pattern.scene_cost(&p).unwrap().nodes, 8);
    assert_eq!(pattern.scene_cost(&p).unwrap().segments, 21);
    let missing: Option<Node> = None;
    assert_eq!(missing.scene_cost(&p).unwrap(), SceneCost::default());
    assert!(clone_owned(&p, &missing).unwrap().is_none());
    assert_eq!(totals(&r), (12, 21));
}

fn nested(depth: usize, p: &ProcessingContext) -> Group {
    let mut value = Group::empty_in(&p);
    for _ in 1..depth {
        let mut parent = Group::empty_in(&p);
        parent
            .children
            .try_push(Node::Group(fixture_box(value, p)), p)
            .unwrap();
        value = parent;
    }
    value
}

#[test]
fn semantic_depth_128_accepts_without_counting_node_box_and_group_frames_twice() {
    let (r, p, _) = request(Limits::default());
    let valid = nested(128, &p);
    assert_eq!(valid.scene_cost(&p).unwrap().nodes, 128);
    drop(clone_owned(&p, &valid).unwrap());
    assert_eq!(totals(&r), (128, 0));
    let invalid = nested(129, &p);
    assert_eq!(
        clone_owned(&p, &invalid).unwrap_err(),
        Error::new(FailureKind::Quota, LimitKind::XmlDepth)
    );
    assert_eq!(totals(&r), (128, 0));
    // The ordinary facade does not acquire a new scene-depth limit.
    let ordinary = ProcessingContext::default();
    let ordinary_source = nested(129, &ordinary);
    drop(clone_owned(&ordinary, &ordinary_source).unwrap());
}

#[test]
fn second_256_scan_window_fails_before_clone_and_before_any_scene_commit() {
    for error in [
        Error::new(FailureKind::Cancelled, LimitKind::None),
        Error::new(FailureKind::Quota, LimitKind::Work),
        Error::native(-9202),
    ] {
        let (r, p, state) = request(Limits::default());
        let calls = Arc::new(AtomicUsize::new(0));
        let source = ObservedClone {
            group: group_with_paths(600, &p),
            calls: calls.clone(),
        };
        // clone_owned entry is the first zero checkpoint, followed by one
        // pre-action event for root and every child. Reject child256's event.
        *state.scan_fault.lock().unwrap() = Some((258, error));
        assert!(matches!(clone_owned(&p, &source), Err(value) if value == error));
        assert_eq!(calls.load(Ordering::Relaxed), 0);
        assert_eq!(state.scan_work.load(Ordering::Relaxed), 257);
        assert_eq!(state.scan_checkpoints.load(Ordering::Relaxed), 258);
        assert_eq!(totals(&r), (0, 0));
        assert_eq!(source.group.children().len(), 600);
        assert_eq!(p.resource_failure(), Some(error));
        assert!(boxed::<Group>(&p, SceneCost::NODE, || panic!("no next producer")).is_err());
        assert_eq!(state.scan_checkpoints.load(Ordering::Relaxed), 258);

        // Inspect the actual scanner's accepted prefix using the same policy
        // window, independently of the wrapper's final error projection.
        let (_, direct, direct_state) = request(Limits::default());
        *direct_state.scan_fault.lock().unwrap() = Some((258, error));
        let _phase = ScanPhase::enter();
        check(&direct).unwrap();
        let mut scan = Scan::new(&direct);
        assert_eq!(scan.group(&source.group, 1), Err(error));
        assert_eq!(scan.cost.nodes, 256); // root + first255 children
        assert_eq!(scan.cost.segments, 255 * 3);
    }
}

#[test]
fn checked_cost_overflow_preserves_first_typed_error_and_commits_neither_dimension() {
    for (prior, addition, limit) in [
        (
            SceneCost {
                nodes: u64::MAX,
                segments: 0,
            },
            SceneCost::NODE,
            LimitKind::ExpandedNodes,
        ),
        (
            SceneCost {
                nodes: 0,
                segments: u64::MAX,
            },
            SceneCost {
                nodes: 1,
                segments: 1,
            },
            LimitKind::ConvertedSegments,
        ),
    ] {
        let (r, p, _) = request(Limits::default());
        let mut scan = Scan {
            processing: &p,
            cost: prior,
        };
        assert_eq!(
            scan.identity(1, addition),
            Err(Error::new(FailureKind::Quota, limit))
        );
        assert_eq!(scan.cost, prior);
        assert_eq!(totals(&r), (0, 0));
        assert_eq!(p.record_resource(Error::native(-9203)).limit, limit);
    }
}

#[test]
fn factory_local_real_paid_retirement_fault_is_rejected_by_tail_with_backend_checkpoint_ok() {
    let (r, p, state) = request(Limits::default());
    let mut temporary = BudgetVec::<u8>::new(r.clone(), ResourceKind::Scratch);
    temporary.try_push(7).unwrap();
    state.release_error.store(true, Ordering::Relaxed);
    let result = root(&p, || {
        drop(temporary);
        Group::empty_in(&p)
    });
    assert_eq!(result.unwrap_err(), Error::native(-9201));
    assert_eq!(totals(&r), (1, 0));
    assert_eq!(p.resource_failure(), Some(Error::native(-9201)));
    assert_eq!(r.statistics().release_failures, 1);
    assert_eq!(r.statistics().live_leases, 1);
    assert!(r.validate_retired().is_err());
    assert!(boxed::<Group>(&p, SceneCost::NODE, || panic!("no fallback")).is_err());
}

#[test]
fn default_node_limit_is_reached_by_actual_independent_group_factories() {
    let limits = Limits::default();
    let expected = limits.expanded_nodes as u64;
    let (r, p, _) = request(limits);
    for index in 0..expected {
        drop(boxed(&p, SceneCost::NODE, || Group::empty_in(&p)).unwrap());
        if index + 2 == expected {
            assert_eq!(totals(&r), (expected - 1, 0));
        }
    }
    assert_eq!(totals(&r), (expected, 0));
    assert_eq!(
        boxed::<Group>(&p, SceneCost::NODE, || panic!("past default node limit")).unwrap_err(),
        Error::new(FailureKind::Quota, LimitKind::ExpandedNodes)
    );
    assert_eq!(totals(&r), (expected, 0));
}

#[test]
fn default_segment_limit_uses_complete_real_path_verbs_and_shared_copy_counts_again() {
    let limits = Limits::default();
    let expected = limits.converted_segments as usize;
    let (r, p, _) = request(limits);
    let source = path_with_verbs(expected, &p);
    assert_eq!(source.data().len(), expected);
    let first = clone_boxed(&p, &source).unwrap();
    assert_eq!(totals(&r), (1, expected as u64));
    assert_eq!(
        first.data().points().as_ptr(),
        source.data().points().as_ptr()
    );
    assert_eq!(
        clone_boxed(&p, &source).unwrap_err(),
        Error::new(FailureKind::Quota, LimitKind::ConvertedSegments)
    );
    assert_eq!(totals(&r), (1, expected as u64));
}

#[cfg(feature = "text")]
#[test]
fn cost_scan_kernel_has_no_system_allocation_after_paid_policy_validation() {
    use crate::geometry_test_support::AllocationObservation;
    let (_, p, _) = request(Limits::default());
    let source = group_with_paths(600, &p);
    source.validate_policy(&p).unwrap();
    let observation = AllocationObservation::begin();
    let mut scan = Scan::new(&p);
    let result = scan.group(&source, 1).map(|()| scan.cost);
    let calls = observation.finish();
    assert_eq!(result.unwrap().nodes, 601);
    assert_eq!(calls.alloc + calls.zeroed + calls.realloc, 0);
}
