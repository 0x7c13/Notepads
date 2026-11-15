// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE file in the project root for license information.
// ---------------------------------------------------------------------------------------------
//! A checked glyph's one selected local representation. Selection precedes
//! spacing; final placement consumes this same object, without another lookup.
//! Retention accounting covers these retained owners and known scene copies.
//! Codec/parser temporary allocations remain a separate admission obligation.

use crate::tree::owner::{self, PolicyValue, TryCopy};
use std::mem::size_of;
use std::sync::Arc;

use super::flatten::{BitmapImage, DatabaseExt, OwnedOutline};
use super::layout::PositionedGlyph;
use crate::{
    Cache, FillRule, Group, Node, ProcessingFailure, ReplayOutline, ReplayRetention, TextSpan,
};

pub(crate) use crate::geometry::SharedOwner as RepresentationRef;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum LocalInk {
    NoInk,
    NonEmpty,
    Unknown,
}

#[derive(Debug)]
pub(crate) enum LocalPayload {
    Colr(Group),
    Svg(Node),
    Bitmap(BitmapImage),
    Ordinary(Option<tiny_skia_path::Path>),
    Synthetic {
        data: Option<crate::SharedOwner<tiny_skia_path::Path>>,
        rule: FillRule,
    },
}

#[derive(Debug)]
pub(crate) struct LocalRepresentation {
    pub(crate) payload: LocalPayload,
    ink: LocalInk,
    projection_bytes: usize,
    // Payload is declared first: admission must outlive its physical storage.
    _metadata_retention: Option<Arc<dyn ReplayRetention>>,
    _payload_retention: Arc<dyn ReplayRetention>,
}

impl LocalRepresentation {
    pub(crate) fn ink(&self) -> LocalInk {
        self.ink
    }

    /// Public only within text: a legitimate alias is another consumer of this
    /// immutable selection, not permission to select the glyph a second time.
    pub(super) fn into_payload(
        value: RepresentationRef<Self>,
        cache: &mut Cache,
    ) -> Option<LocalPayload> {
        let processing = cache.processing.clone();
        if value.validate_policy(&processing).is_err() {
            return None;
        }
        let candidate = (|| {
            let Some(projection_bytes) = value.projection_bytes.checked_add(4 * size_of::<Node>())
            else {
                return failed(cache);
            };
            // This reservation precedes either the alias clone or scene projection.
            // The adapter also keeps the guard until the entire request retires.
            reserve(cache, projection_bytes)?;
            Some(match value.try_unwrap_in(&processing).ok()? {
                Ok(value) => value.payload,
                Err(value) => match project_payload(&value.payload, &cache.processing) {
                    Ok(payload) => payload,
                    Err(error) => {
                        cache.processing.record_resource(error);
                        return None;
                    }
                },
            })
        })();
        // try_unwrap retires the shared header, and the aliased projection
        // retires its consumed reference after cloning. Neither may publish
        // another representation after a cleanup failure in the same slot.
        if let Some(resources) = processing.resources() {
            if let Err(error) = resources.checkpoint(0) {
                processing.record_resource(error);
                return None;
            }
        }
        if processing.failed() {
            None
        } else {
            candidate
        }
    }
}

fn project_payload(
    payload: &LocalPayload,
    processing: &crate::ProcessingContext,
) -> Result<LocalPayload, markdown_svg_budget::Error> {
    if let Some(resources) = processing.resources() {
        resources.checkpoint(0)?;
        if let LocalPayload::Ordinary(Some(path)) = payload {
            // Ordinary callback outlines are the explicit deep-import boundary.
            // Paid inputs must validate every header/array in this same request.
            path.validate_policy(if path.request_resources().is_some() {
                Some(resources)
            } else {
                None
            })
            .map_err(|e| processing.record_resource(e))?;
            return path
                .try_copy_in(resources, markdown_svg_budget::ResourceKind::Scratch)
                .map(|path| LocalPayload::Ordinary(Some(path)));
        }
    }
    // An aliased selected scene produces a real owned copy. Its shared Path
    // arrays retain their existing geometry leases, but structural admission
    // counts every new scene Path/Group identity before the deep Clone.
    match payload {
        LocalPayload::Colr(root) => {
            crate::scene::clone_owned(processing, root).map(LocalPayload::Colr)
        }
        LocalPayload::Svg(node) => {
            crate::scene::clone_owned(processing, node).map(LocalPayload::Svg)
        }
        // Bitmap metadata is not yet an owned scene Node; its placement admits
        // that Node. Ordinary and Synthetic P geometry is also not a scene.
        LocalPayload::Bitmap(v) => v.try_copy_in(processing).map(LocalPayload::Bitmap),
        LocalPayload::Ordinary(v) => v.try_copy_in(processing).map(LocalPayload::Ordinary),
        LocalPayload::Synthetic { data, rule } => Ok(LocalPayload::Synthetic {
            data: data.try_copy_in(processing)?,
            rule: *rule,
        }),
    }
}

fn failed<T>(cache: &Cache) -> Option<T> {
    cache.processing.fail(ProcessingFailure::FontReplay);
    None
}

fn reserve(cache: &Cache, bytes: usize) -> Option<Arc<dyn ReplayRetention>> {
    if cache.processing.failed() {
        return None;
    }
    let Some(owner) = cache.font_replay.as_ref() else {
        return failed(cache);
    };
    match owner.reserve_replay_bytes(bytes) {
        Ok(guard) => Some(guard),
        Err(()) => failed(cache),
    }
}

/// Select only after the caller has admitted the complete CheckedPlan. The
/// effective source paint is retained by PositionedGlyph/Span; local payloads
/// stay in their original font/bitmap units until the existing final transform.
pub(crate) fn select_local(
    cache: &mut Cache,
    glyph: &PositionedGlyph,
    source_span: &TextSpan,
) -> Option<RepresentationRef<LocalRepresentation>> {
    // Preserve the original reserve helper's pre-selection failure barrier,
    // including when controlled metadata uses the exact shared-owner path.
    if cache.processing.failed() {
        return None;
    }
    share_local(cache, |cache| {
        let metadata = if cache.processing.resources().is_some() {
            None
        } else {
            let metadata_bytes =
                size_of::<LocalRepresentation>().checked_add(2 * size_of::<usize>());
            let Some(metadata_bytes) = metadata_bytes else {
                return failed(cache);
            };
            Some(reserve(cache, metadata_bytes)?)
        };
        select_value(cache, glyph, source_span, metadata)
    })
}

fn select_value(
    cache: &mut Cache,
    glyph: &PositionedGlyph,
    source_span: &TextSpan,
    metadata: Option<Arc<dyn ReplayRetention>>,
) -> Option<LocalRepresentation> {
    let Some(owner) = cache.font_replay.clone() else {
        return failed(cache);
    };
    let variations = match owner.variations(glyph.font) {
        Ok(value) => value,
        Err(()) => return failed(cache),
    };
    let replay_outline = match owner.outline(glyph.font, glyph.id.0) {
        Ok(value) => value,
        Err(()) => return failed(cache),
    };

    // Preserve the existing synthetic colour restriction, including real BGRA
    // glyphs that the ordinary raster reader cannot yet consume. A colour table
    // on a different glyph does not reject this glyph's synthetic outline.
    if matches!(replay_outline, ReplayOutline::Synthetic { .. })
        && cache
            .fontdb
            .has_color_glyph(glyph.font, glyph.id, &cache.processing)
            .map_err(|e| cache.processing.record_resource(e))
            .ok()?
            != Some(false)
    {
        return failed(cache);
    }

    let options = cache.font_options()?;
    // No new cache: each positioned occurrence owns one selection. Existing
    // normal-mode caches and their representation priority remain unchanged.
    let payload = if let Some(tree) = cache
        .fontdb
        .colr(
            glyph.font,
            glyph.id,
            &variations,
            source_span.computed_color,
            &options,
        )
        .map_err(|e| cache.processing.record_resource(e))
        .ok()?
    {
        LocalPayload::Colr(tree.into_root_in(&cache.processing).ok()?)
    } else if cache.processing.failed() {
        return None;
    } else if let Some(node) = cache
        .fontdb
        .svg(glyph.font, glyph.id, source_span.computed_color, &options)
        .map_err(|e| cache.processing.record_resource(e))
        .ok()?
    {
        LocalPayload::Svg(node)
    } else if cache.processing.failed() {
        return None;
    } else if let Some(image) = cache
        .fontdb
        .raster(glyph.font, glyph.id, &cache.processing)
        .map_err(|e| cache.processing.record_resource(e))
        .ok()?
    {
        LocalPayload::Bitmap(image)
    } else {
        if cache.processing.failed() {
            return None;
        }
        // A present colour representation which failed to decode is not an
        // empty outline and cannot be converted into a successful blank glyph.
        match cache
            .fontdb
            .has_color_glyph(glyph.font, glyph.id, &cache.processing)
            .map_err(|e| cache.processing.record_resource(e))
            .ok()?
        {
            Some(false) => (),
            _ => return failed(cache),
        }
        match replay_outline {
            ReplayOutline::Synthetic { data, rule } => {
                let data = crate::geometry::import_outline(data, &cache.processing).ok()?;
                LocalPayload::Synthetic { data, rule }
            }
            ReplayOutline::Ordinary => {
                match cache.fontdb.owned_outline(
                    glyph.font,
                    glyph.id,
                    &variations,
                    Some(owner.as_ref()),
                    &cache.processing,
                ) {
                    Ok(OwnedOutline::Empty) => LocalPayload::Ordinary(None),
                    Ok(OwnedOutline::NonEmpty(path)) => LocalPayload::Ordinary(Some(path)),
                    Ok(OwnedOutline::Unknown) => return failed(cache),
                    Err(error) => {
                        cache.processing.record_resource(error);
                        return None;
                    }
                }
            }
        }
    };
    if cache.processing.failed() {
        return None;
    }

    let ink = match &payload {
        LocalPayload::Ordinary(None) | LocalPayload::Synthetic { data: None, .. } => {
            LocalInk::NoInk
        }
        // Empty converted colour trees may result from paint/clip processing.
        // They are not affirmative evidence of an empty local representation.
        LocalPayload::Colr(group) if group.children.is_empty() => LocalInk::Unknown,
        LocalPayload::Svg(Node::Group(group)) if group.children.is_empty() => LocalInk::Unknown,
        _ => LocalInk::NonEmpty,
    };
    let Some(retained_bytes) = retention::payload_bytes(&payload, &cache.processing)
        .map_err(|e| cache.processing.record_resource(e))
        .ok()?
    else {
        let _ = owner.unsupported("UnsupportedLocalRepresentationRetention");
        return failed(cache);
    };
    let projection_bytes = match &payload {
        LocalPayload::Synthetic {
            data: Some(path), ..
        } => {
            let Some(bytes) = path_bytes(path) else {
                return failed(cache);
            };
            bytes
        }
        _ => retained_bytes,
    };
    let retained = reserve(cache, retained_bytes)?;
    Some(LocalRepresentation {
        payload,
        ink,
        projection_bytes,
        _metadata_retention: metadata,
        _payload_retention: retained,
    })
}

fn share_local(
    cache: &mut Cache,
    make: impl FnOnce(&mut Cache) -> Option<LocalRepresentation>,
) -> Option<RepresentationRef<LocalRepresentation>> {
    let p = cache.processing.clone();
    crate::SharedOwner::try_new_with(&p, markdown_svg_budget::ResourceKind::Scratch, || {
        make(cache).ok_or_else(|| {
            p.resource_failure().unwrap_or_else(|| {
                failed::<()>(cache);
                p.resource_failure().expect("failed selection recorded")
            })
        })
    })
    .map_err(|e| p.record_resource(e))
    .ok()
}

/// Paid geometry owns its own exact arrays and header. Only compatibility
/// storage remains in this outer retention estimate; other scene metadata and
/// its owners stay in their original accounting categories.
fn path_bytes(path: &tiny_skia_path::Path) -> Option<usize> {
    path.retained_unpaid_capacity_bytes()
}

#[cfg(test)]
#[path = "geometry_representation_tests.rs"]
mod geometry_representation_tests;

fn path_owner_bytes(path: &crate::geometry::SharedOwner<tiny_skia_path::Path>) -> Option<usize> {
    let arrays = path_bytes(path)?;
    let metadata = if path.is_paid() {
        0
    } else {
        let (layout, _) = std::alloc::Layout::new::<[std::sync::atomic::AtomicUsize; 2]>()
            .extend(std::alloc::Layout::new::<tiny_skia_path::Path>())
            .ok()?;
        layout.pad_to_align().size()
    };
    arrays.checked_add(metadata)
}

#[path = "representation_retention.rs"]
mod retention;

#[cfg(test)]
fn node_bytes(node: &Node, depth: usize) -> Option<usize> {
    retention::test_node_bytes(node, depth)
}

#[cfg(test)]
mod representation_tests {
    use super::*;
    use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
    use std::sync::Mutex;

    #[derive(Debug, Default)]
    struct Stats {
        bytes: AtomicUsize,
        live: AtomicUsize,
    }
    #[derive(Debug)]
    struct Guard(Arc<Stats>);
    impl ReplayRetention for Guard {}
    impl Drop for Guard {
        fn drop(&mut self) {
            self.0.live.fetch_sub(1, Ordering::Relaxed);
        }
    }
    #[derive(Debug, Default)]
    struct Owner {
        stats: Arc<Stats>,
        reject: AtomicBool,
        guards: Mutex<Vec<Arc<dyn ReplayRetention>>>,
    }
    impl crate::FontReplay for Owner {
        fn prepare(
            &self,
            _: &crate::Text,
            _: &mut Arc<fontdb::Database>,
        ) -> Result<crate::ReplayText, ()> {
            panic!("projection must not prepare or select again")
        }
        fn fontdb(&self) -> Result<Arc<fontdb::Database>, ()> {
            panic!("projection must not read font data")
        }
        fn variations(&self, _: fontdb::ID) -> Result<Arc<[crate::FontVariation]>, ()> {
            panic!("projection must not read variations again")
        }
        fn outline(&self, _: fontdb::ID, _: u32) -> Result<ReplayOutline, ()> {
            panic!("projection must not select again")
        }
        fn reserve_replay_bytes(&self, bytes: usize) -> Result<Arc<dyn ReplayRetention>, ()> {
            if self.reject.load(Ordering::Relaxed) {
                return Err(());
            }
            self.stats.bytes.fetch_add(bytes, Ordering::Relaxed);
            self.stats.live.fetch_add(1, Ordering::Relaxed);
            let guard: Arc<dyn ReplayRetention> = Arc::new(Guard(self.stats.clone()));
            self.guards.lock().unwrap().push(guard.clone());
            Ok(guard)
        }
    }
    fn fixture() -> (Cache, Arc<Owner>, RepresentationRef<LocalRepresentation>) {
        let owner = Arc::new(Owner::default());
        let mut cache = Cache::new(Arc::new(fontdb::Database::new()));
        cache.font_replay = Some(owner.clone());
        let mut builder = tiny_skia_path::PathBuilder::new();
        builder.move_to(0.0, 0.0);
        builder.line_to(20.0, 0.0);
        builder.line_to(0.0, 20.0);
        builder.close();
        let path = builder.finish().unwrap();
        let bytes = path_bytes(&path).unwrap();
        let value = Arc::new(LocalRepresentation {
            payload: LocalPayload::Ordinary(Some(path)),
            ink: LocalInk::NonEmpty,
            projection_bytes: bytes,
            _metadata_retention: Some(reserve(&cache, size_of::<LocalRepresentation>()).unwrap()),
            _payload_retention: reserve(&cache, bytes).unwrap(),
        });
        (cache, owner, RepresentationRef::from_compat(value))
    }
    #[test]
    fn legitimate_alias_projects_same_selection_with_paid_copy_and_physical_retirement() {
        let (mut cache, owner, value) = fixture();
        let stats = owner.stats.clone();
        let mut alias = value.try_alias_in(&cache.processing).unwrap();
        let before = stats.bytes.load(Ordering::Relaxed);
        let first = LocalRepresentation::into_payload(value, &mut cache).unwrap();
        assert!(stats.bytes.load(Ordering::Relaxed) > before);
        assert!(alias
            .try_get_mut_if_unique(&cache.processing)
            .unwrap()
            .is_some());
        let second = LocalRepresentation::into_payload(alias, &mut cache).unwrap();
        match (&first, &second) {
            (LocalPayload::Ordinary(Some(a)), LocalPayload::Ordinary(Some(b))) => {
                assert_eq!(a.points(), b.points())
            }
            _ => panic!("the original payload must be reused"),
        }
        drop(first);
        drop(second);
        assert!(stats.live.load(Ordering::Relaxed) > 0);
        drop(cache);
        drop(owner);
        assert_eq!(stats.live.load(Ordering::Relaxed), 0);
    }
    #[test]
    fn alias_copy_reservation_failure_stops_before_copy_and_preserves_other_owner() {
        let (mut cache, owner, value) = fixture();
        let alias = value.try_alias_in(&cache.processing).unwrap();
        owner.reject.store(true, Ordering::Relaxed);
        assert!(LocalRepresentation::into_payload(value, &mut cache).is_none());
        assert_eq!(
            cache.processing.failure(),
            Some(ProcessingFailure::FontReplay)
        );
        assert_eq!(alias.ink(), LocalInk::NonEmpty);
    }

    #[derive(Clone, Copy, Default, Debug)]
    struct AllocationEvent {
        tag: u8,
        bytes: usize,
    }
    #[derive(Clone, Copy)]
    struct AllocationEvents {
        enabled: bool,
        count: usize,
        events: [AllocationEvent; 32],
    }
    thread_local! {
        static ALLOCATION_EVENTS: std::cell::Cell<AllocationEvents> = const { std::cell::Cell::new(AllocationEvents {
            enabled: false, count: 0, events: [AllocationEvent { tag: 0, bytes: 0 }; 32]
        }) };
    }
    fn allocation_event(tag: u8, bytes: usize) {
        let _ = ALLOCATION_EVENTS.try_with(|slot| {
            let mut events = slot.get();
            if events.enabled && events.count < events.events.len() {
                events.events[events.count] = AllocationEvent { tag, bytes };
                events.count += 1;
                slot.set(events);
            }
        });
    }
    #[derive(Default)]
    struct MetadataState {
        // Header-only observer metrics; every request ticket, including paid
        // ordinary projection arrays, remains in the canonical map below.
        live: AtomicUsize,
        next: AtomicUsize,
        tickets: Mutex<std::collections::BTreeMap<u32, (usize, bool, bool)>>,
        failed_tickets: Mutex<Vec<markdown_svg_budget::Ticket>>,
        reserves: AtomicUsize,
        admits: AtomicUsize,
        releases: AtomicUsize,
        reject_reserve: AtomicBool,
        reject_admit: AtomicBool,
        reject_release: AtomicBool,
        cancelled: AtomicBool,
    }
    struct MetadataBackend(Arc<MetadataState>);
    impl markdown_svg_budget::Backend for MetadataBackend {
        fn reserve(
            &self,
            _: markdown_svg_budget::ResourceKind,
            bytes: usize,
        ) -> Result<markdown_svg_budget::Ticket, markdown_svg_budget::Error> {
            let slot = self.0.next.fetch_add(1, Ordering::Relaxed) as u32 + 1;
            // The fixture starts with ordinary storage. share_local's first
            // reservation is the representation header, before its factory.
            // Later reservations belong to the actual paid geometry projection.
            let header = slot == 1;
            if header {
                allocation_event(1, bytes);
                if self.0.reject_reserve.load(Ordering::Relaxed) {
                    return Err(markdown_svg_budget::Error::native(-776));
                }
                self.0.live.fetch_add(bytes, Ordering::Relaxed);
                self.0.reserves.fetch_add(1, Ordering::Relaxed);
            }
            assert!(self
                .0
                .tickets
                .lock()
                .unwrap()
                .insert(slot, (bytes, header, false))
                .is_none());
            markdown_svg_budget::Ticket::new(1, slot, 1)
        }
        fn allocation_committed(
            &self,
            ticket: &markdown_svg_budget::Ticket,
            bytes: usize,
        ) -> Result<(), markdown_svg_budget::Error> {
            let mut tickets = self.0.tickets.lock().unwrap();
            let (reserved, header, admitted) = tickets.get_mut(&ticket.slot()).unwrap();
            assert_eq!(*reserved, bytes);
            assert!(!*admitted);
            *admitted = true;
            if *header {
                allocation_event(3, bytes);
                self.0.admits.fetch_add(1, Ordering::Relaxed);
                if self.0.reject_admit.load(Ordering::Relaxed) {
                    return Err(markdown_svg_budget::Error::native(-778));
                }
            }
            Ok(())
        }
        fn release_after_free(
            &self,
            ticket: markdown_svg_budget::Ticket,
        ) -> Result<(), markdown_svg_budget::Error> {
            assert_eq!(ticket.account_nonce(), 1);
            assert_eq!(ticket.generation(), 1);
            let mut tickets = self.0.tickets.lock().unwrap();
            let (bytes, header, _) = *tickets.get(&ticket.slot()).unwrap();
            if header {
                allocation_event(5, bytes);
                if self.0.reject_release.load(Ordering::Relaxed) {
                    self.0.failed_tickets.lock().unwrap().push(ticket);
                    return Err(markdown_svg_budget::Error::native(-777));
                }
                self.0.live.fetch_sub(bytes, Ordering::Relaxed);
                self.0.releases.fetch_add(1, Ordering::Relaxed);
            }
            assert!(tickets.remove(&ticket.slot()).is_some());
            Ok(())
        }
        fn checkpoint(&self, _: u64) -> Result<(), markdown_svg_budget::Error> {
            if self.0.cancelled.load(Ordering::Relaxed) {
                Err(markdown_svg_budget::Error::new(
                    markdown_svg_budget::FailureKind::Cancelled,
                    markdown_svg_budget::LimitKind::None,
                ))
            } else {
                Ok(())
            }
        }
    }
    fn funded_fixture(
        mut value: LocalRepresentation,
        p: &crate::ProcessingContext,
    ) -> Option<LocalRepresentation> {
        value.payload = project_payload(&value.payload, p)
            .map_err(|e| p.record_resource(e))
            .ok()?;
        Some(value)
    }
    fn metadata_fixture() -> (
        Cache,
        Arc<Owner>,
        Arc<MetadataState>,
        markdown_svg_budget::ResourcesRef,
        LocalRepresentation,
    ) {
        let (mut cache, owner, value) = fixture();
        let mut value = value
            .try_unwrap_in(&cache.processing)
            .unwrap()
            .expect("unique legacy fixture");
        value._metadata_retention = None;
        let state = Arc::new(MetadataState::default());
        let resources = markdown_svg_budget::ResourcesRef::new_prepaid(
            MetadataBackend(state.clone()),
            markdown_svg_budget::FirstFailureRef::new_prepaid(),
            Default::default(),
        );
        let replay = cache.font_replay.take();
        cache = Cache::with_processing(
            Arc::new(crate::ProcessingContext::with_resources(resources.clone())),
            cache.fontdb.clone(),
        );
        cache.font_replay = replay;
        (cache, owner, state, resources, value)
    }
    fn observe_metadata() {
        ALLOCATION_EVENTS.with(|slot| {
            slot.set(AllocationEvents {
                enabled: true,
                count: 0,
                events: [AllocationEvent::default(); 32],
            })
        });
    }
    fn metadata_events() -> Vec<AllocationEvent> {
        ALLOCATION_EVENTS.with(|slot| {
            let events = slot.get();
            slot.set(AllocationEvents {
                enabled: false,
                ..events
            });
            events.events[..events.count].to_vec()
        })
    }
    fn assert_metadata_sequence(events: &[AllocationEvent]) {
        // Safe protocol observation only. Physical System free is verified by
        // the shared owner's existing allocator suite and the engine observer.
        assert_eq!(events.iter().map(|e| e.tag).collect::<Vec<_>>(), [1, 3, 5]);
        assert_eq!(events[0].bytes, events[1].bytes);
        assert_eq!(events[0].bytes, events[2].bytes);
        assert!(events[0].bytes > size_of::<LocalRepresentation>());
    }
    #[test]
    fn controlled_metadata_alias_keeps_the_admitted_fee_until_last_owner() {
        let (mut cache, owner, state, resources, value) = metadata_fixture();
        let coarse_before = owner.stats.bytes.load(Ordering::Relaxed);
        observe_metadata();
        let value =
            share_local(&mut cache, |cache| funded_fixture(value, &cache.processing)).unwrap();
        assert!(value.is_paid());
        let alias = value.try_alias_in(&cache.processing).unwrap();
        assert_eq!(owner.stats.bytes.load(Ordering::Relaxed), coarse_before);
        assert_eq!(state.reserves.load(Ordering::Relaxed), 1);
        assert_eq!(state.admits.load(Ordering::Relaxed), 1);
        let first = LocalRepresentation::into_payload(value, &mut cache).unwrap();
        assert_eq!(state.releases.load(Ordering::Relaxed), 0);
        assert!(state.live.load(Ordering::Relaxed) > 0);
        let second = LocalRepresentation::into_payload(alias, &mut cache).unwrap();
        let events = metadata_events();
        assert_metadata_sequence(&events);
        assert_eq!(state.live.load(Ordering::Relaxed), 0);
        drop(cache);
        match (&first, &second) {
            (LocalPayload::Ordinary(Some(a)), LocalPayload::Ordinary(Some(b))) => {
                assert_eq!(a.points(), b.points());
                assert_eq!(a.verbs(), b.verbs());
            }
            _ => panic!("selection and alias projection changed"),
        }
        drop(first);
        drop(second);
        assert!(state.tickets.lock().unwrap().is_empty());
        resources.validate_retired().unwrap();
    }
    #[test]
    fn controlled_metadata_admit_failure_discards_owner_and_refunds_pending_ticket() {
        let (mut cache, _owner, state, resources, value) = metadata_fixture();
        state.reject_admit.store(true, Ordering::Relaxed);
        observe_metadata();
        assert!(
            share_local(&mut cache, |cache| funded_fixture(value, &cache.processing)).is_none()
        );
        let events = metadata_events();
        assert_metadata_sequence(&events);
        assert_eq!(state.live.load(Ordering::Relaxed), 0);
        assert_eq!(resources.failure().unwrap().native_hr, Some(-778));
        drop(cache);
        resources.validate_retired().unwrap();
    }
    #[test]
    fn controlled_metadata_cancel_before_reserve_allocates_no_shared_storage() {
        let (mut cache, _owner, state, resources, value) = metadata_fixture();
        state.cancelled.store(true, Ordering::Relaxed);
        observe_metadata();
        assert!(
            share_local(&mut cache, |cache| funded_fixture(value, &cache.processing)).is_none()
        );
        assert!(metadata_events().is_empty());
        assert_eq!(state.reserves.load(Ordering::Relaxed), 0);
        assert_eq!(
            resources.failure().unwrap().kind,
            markdown_svg_budget::FailureKind::Cancelled
        );
        drop(cache);
        resources.validate_retired().unwrap();
    }
    #[test]
    fn controlled_metadata_cleanup_failure_keeps_the_original_native_first_fault() {
        let (mut cache, _owner, state, resources, value) = metadata_fixture();
        state.reject_admit.store(true, Ordering::Relaxed);
        state.reject_release.store(true, Ordering::Relaxed);
        observe_metadata();
        assert!(
            share_local(&mut cache, |cache| funded_fixture(value, &cache.processing)).is_none()
        );
        let events = metadata_events();
        assert_metadata_sequence(&events);
        assert!(state.live.load(Ordering::Relaxed) > 0);
        assert_eq!(resources.failure().unwrap().native_hr, Some(-778));
        assert_eq!(resources.statistics().release_failures, 1);
        drop(cache);
        assert!(resources.validate_retired().is_err());
    }
    #[test]
    fn controlled_metadata_is_reserved_before_selection_factory_and_admitted_after_it() {
        let (mut cache, _owner, state, resources, value) = metadata_fixture();
        observe_metadata();
        let value = share_local(&mut cache, |cache| {
            assert_eq!(state.reserves.load(Ordering::Relaxed), 1);
            assert!(state.live.load(Ordering::Relaxed) > 0);
            assert_eq!(state.admits.load(Ordering::Relaxed), 0);
            funded_fixture(value, &cache.processing)
        })
        .unwrap();
        drop(value);
        assert_metadata_sequence(&metadata_events());
        drop(cache);
        resources.validate_retired().unwrap();
    }
    #[test]
    fn controlled_metadata_rejection_stops_before_selection_factory() {
        let (mut cache, _owner, state, resources, value) = metadata_fixture();
        state.reject_reserve.store(true, Ordering::Relaxed);
        let selected = std::cell::Cell::new(false);
        observe_metadata();
        assert!(share_local(&mut cache, |_| {
            selected.set(true);
            Some(value)
        })
        .is_none());
        assert!(!selected.get());
        assert_eq!(
            metadata_events().iter().map(|e| e.tag).collect::<Vec<_>>(),
            [1]
        );
        assert_eq!(state.admits.load(Ordering::Relaxed), 0);
        assert_eq!(state.live.load(Ordering::Relaxed), 0);
        assert_eq!(resources.failure().unwrap().native_hr, Some(-776));
        drop(cache);
        resources.validate_retired().unwrap();
    }
    #[test]
    fn controlled_factory_failure_precedes_pending_metadata_cleanup_error() {
        let (mut cache, _owner, state, resources, value) = metadata_fixture();
        state.reject_release.store(true, Ordering::Relaxed);
        observe_metadata();
        assert!(share_local(&mut cache, |cache| {
            drop(value);
            failed(cache)
        })
        .is_none());
        assert_eq!(
            metadata_events().iter().map(|e| e.tag).collect::<Vec<_>>(),
            [1, 5]
        );
        assert_eq!(state.admits.load(Ordering::Relaxed), 0);
        assert!(state.live.load(Ordering::Relaxed) > 0);
        assert_eq!(
            resources.failure().unwrap().kind,
            markdown_svg_budget::FailureKind::FontReplay
        );
        assert_eq!(resources.failure().unwrap().native_hr, None);
        assert_eq!(resources.statistics().release_failures, 1);
        drop(cache);
        assert!(resources.validate_retired().is_err());
    }
}

impl PolicyValue for LocalPayload {
    fn validate_policy(
        &self,
        p: &crate::ProcessingContext,
    ) -> Result<(), markdown_svg_budget::Error> {
        owner::check(p, 0)?;
        match self {
            Self::Colr(v) => v.validate_policy(p),
            Self::Svg(v) => v.validate_policy(p),
            Self::Bitmap(v) => v.validate_policy(p),
            Self::Ordinary(v) => v.validate_policy(p),
            Self::Synthetic { data, .. } => data.validate_policy(p),
        }
    }
}
impl PolicyValue for LocalRepresentation {
    fn validate_policy(
        &self,
        p: &crate::ProcessingContext,
    ) -> Result<(), markdown_svg_budget::Error> {
        self.payload.validate_policy(p)
    }
}
