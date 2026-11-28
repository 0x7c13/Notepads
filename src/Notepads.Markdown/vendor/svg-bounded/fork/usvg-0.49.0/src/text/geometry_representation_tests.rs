// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE file in the project root for license information.
// ---------------------------------------------------------------------------------------------
//! Multi-ticket projection/retention checks, separate from the header-only mock.

use super::*;
use crate::geometry_test_support::{native, outline, request, Call};
use markdown_svg_budget::ResourcesRef;

#[derive(Debug)]
struct Guard;
impl ReplayRetention for Guard {}
#[derive(Debug, Default)]
struct Owner {
    reservations: std::sync::atomic::AtomicUsize,
}
impl crate::FontReplay for Owner {
    fn prepare(
        &self,
        _: &crate::Text,
        _: &mut Arc<fontdb::Database>,
    ) -> Result<crate::ReplayText, ()> {
        panic!("projection cannot select again")
    }
    fn fontdb(&self) -> Result<Arc<fontdb::Database>, ()> {
        panic!("projection cannot read font data")
    }
    fn variations(&self, _: fontdb::ID) -> Result<Arc<[crate::FontVariation]>, ()> {
        panic!("projection cannot select axes")
    }
    fn outline(&self, _: fontdb::ID, _: u32) -> Result<ReplayOutline, ()> {
        panic!("projection cannot select an outline")
    }
    fn reserve_replay_bytes(&self, _: usize) -> Result<Arc<dyn ReplayRetention>, ()> {
        self.reservations
            .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        Ok(Arc::new(Guard))
    }
}

fn cache(resources: &ResourcesRef) -> Cache {
    let mut cache = Cache::with_processing(
        Arc::new(crate::ProcessingContext::with_resources(resources.clone())),
        Arc::new(fontdb::Database::new()),
    );
    cache.font_replay = Some(Arc::new(Owner::default()));
    cache
}

#[test]
fn paid_representation_header_uses_its_own_request_before_projection_or_unwrap() {
    use markdown_svg_budget::{FailureKind, ResourceKind};
    for aliased in [false, true] {
        // Both negative forms start with fresh requests. The inner path is
        // deliberately paid by B: inspecting that path cannot establish that
        // the representation's separate header, paid by A, belongs to B.
        let (source_resources, source_state) = request(Default::default());
        let (resources, state) = request(Default::default());
        let inner = outline()
            .try_copy_in(&resources, ResourceKind::Scratch)
            .unwrap();
        let shared = RepresentationRef::test_with_header_unchecked(
            value(inner),
            &source_resources,
            ResourceKind::Scratch,
        )
        .unwrap();
        let alias = aliased.then(|| shared.test_alias_header_only());
        let owner = Arc::new(Owner::default());
        let mut cache = cache(&resources);
        cache.font_replay = Some(owner.clone());
        let before = state.counts();
        assert!(LocalRepresentation::into_payload(shared, &mut cache).is_none());
        assert_eq!(resources.failure().unwrap().kind, FailureKind::Invariant);
        assert_eq!(
            owner
                .reservations
                .load(std::sync::atomic::Ordering::Relaxed),
            0
        );
        assert_eq!(state.counts().reserve, before.reserve);
        assert_eq!(state.counts().commit, before.commit);
        assert_eq!(state.counts().checkpoint, before.checkpoint);
        assert_eq!(
            source_resources.failure().unwrap().kind,
            FailureKind::Invariant
        );
        if aliased {
            assert_eq!(source_state.live_len(), 1);
        }
        drop(alias);
        drop(cache);
        assert_eq!(source_state.live_len(), 0);
        assert_eq!(state.live_len(), 0);
        source_resources.validate_retired().unwrap();
        resources.validate_retired().unwrap();
    }

    let (resources, state) = request(Default::default());
    let inner = outline()
        .try_copy_in(&resources, ResourceKind::Scratch)
        .unwrap();
    let shared =
        RepresentationRef::try_new(value(inner), Some(&resources), ResourceKind::Scratch).unwrap();
    let mut cache = cache(&resources);
    let alias = shared.try_alias_in(&cache.processing).unwrap();
    let first = LocalRepresentation::into_payload(shared, &mut cache).unwrap();
    let second = LocalRepresentation::into_payload(alias, &mut cache).unwrap();
    for payload in [&first, &second] {
        assert!(
            matches!(payload, LocalPayload::Ordinary(Some(path)) if path.belongs_to(&resources))
        );
    }
    drop((first, second, cache));
    assert_eq!(state.live_len(), 0);
    resources.validate_retired().unwrap();
}

fn value(path: tiny_skia_path::Path) -> LocalRepresentation {
    LocalRepresentation {
        projection_bytes: path_bytes(&path).unwrap(),
        payload: LocalPayload::Ordinary(Some(path)),
        ink: LocalInk::NonEmpty,
        _metadata_retention: None,
        _payload_retention: Arc::new(Guard),
    }
}

fn value_in(path: tiny_skia_path::Path, p: &crate::ProcessingContext) -> LocalRepresentation {
    let LocalPayload::Ordinary(Some(path)) =
        project_payload(&LocalPayload::Ordinary(Some(path)), p).unwrap()
    else {
        unreachable!()
    };
    value(path)
}

#[test]
fn selected_scene_alias_admits_copy_but_unique_projection_only_moves_existing_graph() {
    use markdown_svg_budget::{FailureKind, LimitKind, Limits};
    for svg_node in [false, true] {
        for (aliased, allowed, success) in [
            (false, false, true),
            (true, false, false),
            (true, true, true),
        ] {
            let (resources, state) = request(Limits {
                expanded_nodes: if allowed { 4 } else { 2 },
                converted_segments: if allowed { 8 } else { 4 },
                ..Limits::default()
            });
            let mut cache = cache(&resources);
            let options = crate::Options {
                processing: cache.processing.clone(),
                ..crate::Options::default()
            };
            let tree = crate::Tree::from_data_controlled(
                b"<svg xmlns='http://www.w3.org/2000/svg' width='32' height='32'><path d='M1 1L20 1L20 20Z'/></svg>",
                &options,
            ).unwrap();
            assert_eq!(resources.statistics().expanded_nodes, 2);
            assert_eq!(resources.statistics().converted_segments, 4);
            // Drop the parsed Tree's remaining request owner explicitly. A
            // partial move of tree.root would retain tree.processing until
            // this test iteration ended, past validate_retired below.
            let root = tree.into_root_in(&cache.processing).unwrap();
            let payload = if svg_node {
                // Same-request parsed root, already admitted by converter.
                LocalPayload::Svg(crate::Node::Group(
                    crate::scene::moved_root(&cache.processing, root).unwrap(),
                ))
            } else {
                LocalPayload::Colr(root)
            };
            let shared = RepresentationRef::try_new(
                LocalRepresentation {
                    payload,
                    ink: LocalInk::NonEmpty,
                    projection_bytes: 0,
                    _metadata_retention: None,
                    _payload_retention: Arc::new(Guard),
                },
                Some(&resources),
                markdown_svg_budget::ResourceKind::Scratch,
            )
            .unwrap();
            let alias = aliased.then(|| shared.try_alias_in(&cache.processing).unwrap());
            let candidate = LocalRepresentation::into_payload(shared, &mut cache);
            assert_eq!(candidate.is_some(), success);
            let actual = resources.statistics();
            let expected = if aliased && success { (4, 8) } else { (2, 4) };
            assert_eq!((actual.expanded_nodes, actual.converted_segments), expected);
            if !success {
                let error = resources.failure().unwrap();
                assert_eq!(
                    (error.kind, error.limit),
                    (FailureKind::Quota, LimitKind::ExpandedNodes)
                );
                assert_eq!(alias.as_ref().unwrap().ink(), LocalInk::NonEmpty);
            }
            drop((candidate, alias, options, cache));
            assert_eq!(state.live_len(), 0);
            resources.validate_retired().unwrap();
        }
    }
}

#[test]
fn ordinary_alias_projection_pays_real_arrays_before_copy_and_retires_all_owners() {
    let (resources, state) = request(Default::default());
    let mut cache = cache(&resources);
    let source = outline();
    let source_pointer = source.points().as_ptr() as usize;
    let shared = share_local(&mut cache, |cache| {
        Some(value_in(source, &cache.processing))
    })
    .unwrap();
    let alias = shared.try_alias_in(&cache.processing).unwrap();
    let first = LocalRepresentation::into_payload(shared, &mut cache).unwrap();
    assert!(
        matches!(&first, LocalPayload::Ordinary(Some(path)) if path.belongs_to(&resources) && path.retained_unpaid_capacity_bytes() == Some(0) && path.points().as_ptr() as usize != source_pointer)
    );
    assert!(
        state.live_len() >= 4,
        "metadata plus the paid geometry header and two arrays coexist"
    );
    let second = LocalRepresentation::into_payload(alias, &mut cache).unwrap();
    match (&first, &second) {
        (LocalPayload::Ordinary(Some(a)), LocalPayload::Ordinary(Some(b))) => {
            assert_eq!(a.verbs(), b.verbs());
            assert_eq!(a.points(), b.points());
            assert_eq!(a.bounds(), b.bounds());
        }
        _ => panic!("the same selected ordinary outline must be projected"),
    }
    drop(first);
    drop(second);
    drop(cache);
    assert_eq!(state.live_len(), 0);
    resources.validate_retired().unwrap();
}

#[test]
fn real_ordinary_projection_allocation_faults_never_return_an_unpaid_clone() {
    let source = outline();
    let payload = LocalPayload::Ordinary(Some(source.clone()));
    let (resources, state) = request(Default::default());
    drop(
        project_payload(
            &payload,
            &crate::ProcessingContext::with_resources(resources.clone()),
        )
        .unwrap(),
    );
    let baseline = state.counts();
    assert_eq!(baseline.reserve, 3);
    assert_eq!(baseline.commit, 3);
    resources.validate_retired().unwrap();
    for call in [Call::Reserve, Call::Commit] {
        for at in 1..=3 {
            let (resources, state) = request(Default::default());
            state.arm(call, at, native(-830));
            assert_eq!(
                project_payload(
                    &payload,
                    &crate::ProcessingContext::with_resources(resources.clone()),
                )
                .unwrap_err(),
                native(-830)
            );
            assert_eq!(resources.failure(), Some(native(-830)));
            assert_eq!(state.live_len(), 0);
            resources.validate_retired().unwrap();
            assert!(
                matches!(&payload, LocalPayload::Ordinary(Some(path)) if path.points() == source.points())
            );
        }
    }
}

#[test]
fn shared_representation_header_release_failure_stops_before_payload_consumer() {
    let (resources, state) = request(Default::default());
    let mut cache = cache(&resources);
    let shared = share_local(&mut cache, |cache| {
        Some(value_in(outline(), &cache.processing))
    })
    .unwrap();
    state.arm(Call::Release, 1, native(-831));
    let mut consumer = 0;
    if LocalRepresentation::into_payload(shared, &mut cache).is_some() {
        consumer += 1;
    }
    assert_eq!(consumer, 0);
    assert_eq!(resources.failure(), Some(native(-831)));
    assert_eq!(cache.processing.resource_failure(), Some(native(-831)));
    assert_eq!(state.live_len(), 1);
    assert_eq!(resources.statistics().release_failures, 1);
    drop(cache);
    assert!(resources.validate_retired().is_err());
}

#[test]
fn paid_paths_remove_only_their_exact_geometry_storage_and_outer_header_fee() {
    let original = outline();
    let mut ordinary = crate::Path::new_simple(Arc::new(original.clone())).unwrap();
    ordinary.id =
        crate::TreeString::try_with_capacity(23, &crate::ProcessingContext::default()).unwrap();
    ordinary
        .id
        .try_push_char('p', &crate::ProcessingContext::default())
        .unwrap();
    let ordinary = Node::Path(
        crate::SceneBox::try_new_with(&crate::ProcessingContext::default(), || Ok(ordinary))
            .unwrap(),
    );
    let ordinary_bytes = node_bytes(&ordinary, 0).unwrap();
    let Node::Path(ordinary_path) = &ordinary else {
        unreachable!()
    };
    let (layout, _) = std::alloc::Layout::new::<[std::sync::atomic::AtomicUsize; 2]>()
        .extend(std::alloc::Layout::new::<tiny_skia_path::Path>())
        .unwrap();
    // Compat Vec::clone can shrink capacity. Account the actual retained
    // clone's two capacities, rather than its source's growth allowance.
    let capacity = ordinary_path.data.retained_unpaid_capacity_bytes().unwrap();
    assert!(capacity > 0);
    let geometry = capacity + layout.pad_to_align().size();
    assert_eq!(path_owner_bytes(&ordinary_path.data), Some(geometry));
    assert_eq!(
        ordinary_bytes,
        size_of::<crate::Path>() + ordinary_path.id.capacity() + geometry
    );

    let (resources, state) = request(Default::default());
    let mut paid = crate::Path::try_new_simple(original, &resources)
        .unwrap()
        .unwrap();
    let p = crate::ProcessingContext::with_resources(resources.clone());
    paid.id = crate::TreeString::try_with_capacity(23, &p).unwrap();
    paid.id.try_push_char('p', &p).unwrap();
    let paid = Node::Path(crate::SceneBox::try_new_with(&p, || Ok(paid)).unwrap());
    let Node::Path(paid_path) = &paid else {
        unreachable!()
    };
    assert!(paid_path.data.is_paid());
    assert!(paid_path.data.belongs_to(&resources));
    assert_eq!(path_owner_bytes(&paid_path.data), Some(0));
    assert_eq!(
        node_bytes(&paid, 0),
        Some(size_of::<crate::Path>() + paid_path.id.capacity())
    );
    assert_eq!(ordinary_bytes - node_bytes(&paid, 0).unwrap(), geometry);
    assert_eq!(path_bytes(&paid_path.data), Some(0));
    let alias = paid_path.data.try_alias_in(&p).unwrap();
    drop(paid);
    assert!(
        state.live_len() > 0,
        "external path aliases retain the exact paid owners"
    );
    drop(alias);
    drop(p);
    assert_eq!(state.live_len(), 0);
    resources.validate_retired().unwrap();
}
