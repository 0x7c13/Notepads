// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE file in the project root for license information.
// ---------------------------------------------------------------------------------------------
//! Original SFNT fixtures exercise the actual replay/layout/flatten chain.
//! These tests prove provenance mechanics, not Native shaping or pixels.

use super::*;
use crate::*;
use markdown_svg_budget::{
    Backend, Error as ResourceError, FailureKind, FirstFailureRef, LimitKind, Limits, Ticket,
};
use std::cell::RefCell;
use std::collections::{BTreeMap, BTreeSet};
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum CapturePhase {
    Record,
    Bind,
    Shared,
}

struct Observation {
    request: ResourcesRef,
    state: Arc<State>,
    phase: Option<CapturePhase>,
}
thread_local! {
    static OBSERVATION: RefCell<Option<Observation>> = const { RefCell::new(None) };
}

struct ObserveRequest;
impl ObserveRequest {
    fn enter(request: &ResourcesRef, state: &Arc<State>) -> Self {
        OBSERVATION.with_borrow_mut(|slot| {
            assert!(slot.is_none());
            *slot = Some(Observation {
                request: request.clone(),
                state: state.clone(),
                phase: None,
            });
        });
        Self
    }
}
impl Drop for ObserveRequest {
    fn drop(&mut self) {
        OBSERVATION.with_borrow_mut(|slot| {
            let observation = slot.take().unwrap();
            assert!(observation.phase.is_none());
        });
    }
}

pub(super) struct CapturePhaseGuard {
    active: bool,
    previous: Option<CapturePhase>,
}
impl CapturePhaseGuard {
    pub(super) fn enter(request: &ResourcesRef, phase: CapturePhase) -> Self {
        OBSERVATION.with_borrow_mut(|slot| {
            let Some(observation) = slot.as_mut() else {
                return Self {
                    active: false,
                    previous: None,
                };
            };
            assert!(observation.request.is_same_request(request));
            Self {
                active: true,
                previous: observation.phase.replace(phase),
            }
        })
    }
}
impl Drop for CapturePhaseGuard {
    fn drop(&mut self) {
        if self.active {
            OBSERVATION.with_borrow_mut(|slot| slot.as_mut().unwrap().phase = self.previous);
        }
    }
}

#[derive(Clone, Copy, Debug)]
struct Live {
    kind: ResourceKind,
    bytes: usize,
    capture: Option<CapturePhase>,
    committed: bool,
}

#[derive(Debug, Default)]
struct State {
    next: AtomicUsize,
    // Every allocation remains in this ledger, including unrelated new Geometry.
    live: Mutex<BTreeMap<u32, Live>>,
    // Physical storage is freed even when its canonical ticket refund fails.
    failed_refunds: Mutex<Vec<(Ticket, Live)>>,
    all_geometry_reserves: AtomicUsize,
    all_geometry_admits: AtomicUsize,
    all_geometry_releases: AtomicUsize,
    all_geometry_live: AtomicUsize,
    capture_reserves: AtomicUsize,
    capture_admits: AtomicUsize,
    capture_live: AtomicUsize,
    reject_reserve: AtomicUsize,
    reject_admit: AtomicUsize,
    rejected_reserve_phase: Mutex<Option<CapturePhase>>,
    rejected_admit_phase: Mutex<Option<CapturePhase>>,
    reject_release: AtomicBool,
    refused_capture_refunds: AtomicUsize,
    cancel: AtomicBool,
    capture_batches: AtomicUsize,
    synthetic_observations: AtomicUsize,
    growth_reserves: AtomicUsize,
    growth_admits: AtomicUsize,
    reject_growth_reserve: AtomicBool,
    reject_growth_admit: AtomicBool,
    reject_shared_after_growth: AtomicBool,
    reject_shared_admit_after_growth: AtomicBool,
    shared_after_growth_attempts: AtomicUsize,
    cancel_growth_admit: AtomicBool,
    empty_evenodd_observations: AtomicUsize,
    empty_evenodd_growth_reserves: AtomicUsize,
    empty_evenodd_growth_admits: AtomicUsize,
    reject_empty_evenodd_growth_reserve: AtomicBool,
    reject_empty_evenodd_growth_admit: AtomicBool,
    cancel_empty_evenodd_growth_admit: AtomicBool,
    reject_empty_evenodd_observation: AtomicBool,
    empty_evenodd_observation_rejections: AtomicUsize,
}
impl Drop for State {
    fn drop(&mut self) {
        if std::thread::panicking() {
            return;
        }
        assert!(
            self.live.get_mut().unwrap().is_empty(),
            "all Capture and other request storage must physically retire"
        );
        assert_eq!(*self.capture_live.get_mut(), 0);
        assert_eq!(*self.all_geometry_live.get_mut(), 0);
        assert_eq!(
            *self.all_geometry_reserves.get_mut(),
            *self.all_geometry_releases.get_mut()
        );
        let failed = self.failed_refunds.get_mut().unwrap();
        assert_eq!(failed.len(), *self.refused_capture_refunds.get_mut());
        let mut identities = BTreeSet::new();
        for (ticket, live) in failed.iter() {
            assert_eq!(live.kind, ResourceKind::Geometry);
            assert!(matches!(
                live.capture,
                Some(CapturePhase::Record | CapturePhase::Shared)
            ));
            assert!(live.bytes > 0);
            assert!(identities.insert((
                ticket.account_nonce(),
                ticket.slot(),
                ticket.generation()
            )));
            println!(
                "CAPTURE_FAILED_CANONICAL account={} slot={} generation={} phase={:?} bytes={} admitted={}",
                ticket.account_nonce(),
                ticket.slot(),
                ticket.generation(),
                live.capture,
                live.bytes,
                live.committed
            );
        }
        println!(
            "CAPTURE_LEDGER capture_reserve_attempts={} capture_admit_attempts={} all_geometry_accepted_reserves={} all_geometry_admit_attempts={} all_geometry_physical_releases={} all_geometry_live=0 failed_canonical_tickets={} failed_committed_tickets={}",
            self.capture_reserves.get_mut(),
            self.capture_admits.get_mut(),
            self.all_geometry_reserves.get_mut(),
            self.all_geometry_admits.get_mut(),
            self.all_geometry_releases.get_mut(),
            failed.len(),
            failed.iter().filter(|(_, live)| live.committed).count()
        );
    }
}
struct Host(Arc<State>);
impl Host {
    fn capture_phase(&self) -> Option<CapturePhase> {
        OBSERVATION.with_borrow(|slot| {
            slot.as_ref()
                .filter(|value| Arc::ptr_eq(&value.state, &self.0))
                .and_then(|value| value.phase)
        })
    }

    fn is_second_empty_evenodd_record_growth(&self, kind: ResourceKind, bytes: usize) -> bool {
        // Both observed Synthetic inputs must actually be None/EvenOdd. The
        // original one-record allocation must still be live while its own
        // replacement is reserved/admitted, before the following ordinary A.
        self.0.empty_evenodd_observations.load(Ordering::Relaxed) == 2
            && self.is_second_synthetic_record_growth(kind, bytes)
    }

    fn is_second_synthetic_record_growth(&self, kind: ResourceKind, bytes: usize) -> bool {
        let record = std::mem::size_of::<PlacedGlyphSource>();
        kind == ResourceKind::Geometry
            && self.capture_phase() == Some(CapturePhase::Record)
            && bytes == 4 * record
            && self.0.synthetic_observations.load(Ordering::Relaxed) == 2
            && self
                .0
                .live
                .lock()
                .unwrap()
                .values()
                .any(|live| live.capture == Some(CapturePhase::Record) && live.bytes == record)
    }
}
impl Backend for Host {
    fn reserve(&self, kind: ResourceKind, bytes: usize) -> Result<Ticket, ResourceError> {
        let capture = self
            .capture_phase()
            .filter(|phase| matches!(phase, CapturePhase::Record | CapturePhase::Shared));
        if matches!(capture, Some(CapturePhase::Record | CapturePhase::Shared)) {
            assert_eq!(kind, ResourceKind::Geometry);
            let ordinal = self.0.capture_reserves.fetch_add(1, Ordering::Relaxed) + 1;
            if self.is_second_empty_evenodd_record_growth(kind, bytes) {
                self.0
                    .empty_evenodd_growth_reserves
                    .fetch_add(1, Ordering::Relaxed);
                if self
                    .0
                    .reject_empty_evenodd_growth_reserve
                    .load(Ordering::Relaxed)
                {
                    return Err(ResourceError::native(-521));
                }
            }
            if self.is_second_synthetic_record_growth(kind, bytes) {
                self.0.growth_reserves.fetch_add(1, Ordering::Relaxed);
                if self.0.reject_growth_reserve.load(Ordering::Relaxed) {
                    return Err(ResourceError::native(-511));
                }
            }
            let record = std::mem::size_of::<PlacedGlyphSource>();
            let shared_after_growth = self.0.growth_admits.load(Ordering::Relaxed) == 1
                && capture == Some(CapturePhase::Shared)
                && self.0.live.lock().unwrap().values().any(|live| {
                    live.capture == Some(CapturePhase::Record) && live.bytes == 4 * record
                });
            if shared_after_growth {
                self.0
                    .shared_after_growth_attempts
                    .fetch_add(1, Ordering::Relaxed);
                if self.0.reject_shared_after_growth.load(Ordering::Relaxed) {
                    return Err(ResourceError::native(-513));
                }
            }
            if self.0.reject_reserve.load(Ordering::Relaxed) == ordinal {
                assert!(self
                    .0
                    .rejected_reserve_phase
                    .lock()
                    .unwrap()
                    .replace(capture.unwrap())
                    .is_none());
                return Err(ResourceError::native(-501));
            }
        }
        let slot = self.0.next.fetch_add(1, Ordering::Relaxed) as u32 + 1;
        assert!(self
            .0
            .live
            .lock()
            .unwrap()
            .insert(
                slot,
                Live {
                    kind,
                    bytes,
                    capture,
                    committed: false,
                }
            )
            .is_none());
        if kind == ResourceKind::Geometry {
            self.0.all_geometry_reserves.fetch_add(1, Ordering::Relaxed);
            self.0.all_geometry_live.fetch_add(bytes, Ordering::Relaxed);
        }
        if capture.is_some() {
            self.0.capture_live.fetch_add(bytes, Ordering::Relaxed);
        }
        Ticket::new(1, slot, 1)
    }
    fn allocation_committed(&self, ticket: &Ticket, bytes: usize) -> Result<(), ResourceError> {
        let live = self.0.live.lock().unwrap()[&ticket.slot()];
        assert_eq!(live.bytes, bytes);
        let kind = live.kind;
        let capture = live.capture;
        if kind == ResourceKind::Geometry {
            self.0.all_geometry_admits.fetch_add(1, Ordering::Relaxed);
        }
        if capture.is_some() {
            assert_eq!(capture, self.capture_phase());
            let ordinal = self.0.capture_admits.fetch_add(1, Ordering::Relaxed) + 1;
            if self.is_second_empty_evenodd_record_growth(kind, bytes) {
                self.0
                    .empty_evenodd_growth_admits
                    .fetch_add(1, Ordering::Relaxed);
                if self
                    .0
                    .reject_empty_evenodd_growth_admit
                    .load(Ordering::Relaxed)
                {
                    return Err(ResourceError::native(-522));
                }
                if self
                    .0
                    .cancel_empty_evenodd_growth_admit
                    .load(Ordering::Relaxed)
                {
                    self.0.cancel.store(true, Ordering::Relaxed);
                }
            }
            if self.is_second_synthetic_record_growth(kind, bytes) {
                self.0.growth_admits.fetch_add(1, Ordering::Relaxed);
                if self.0.reject_growth_admit.load(Ordering::Relaxed) {
                    return Err(ResourceError::native(-512));
                }
                if self.0.cancel_growth_admit.load(Ordering::Relaxed) {
                    self.0.cancel.store(true, Ordering::Relaxed);
                }
            }
            let record = std::mem::size_of::<PlacedGlyphSource>();
            let shared_after_growth = self.0.growth_admits.load(Ordering::Relaxed) == 1
                && capture == Some(CapturePhase::Shared)
                && self.0.live.lock().unwrap().values().any(|live| {
                    live.capture == Some(CapturePhase::Record) && live.bytes == 4 * record
                });
            if shared_after_growth
                && self
                    .0
                    .reject_shared_admit_after_growth
                    .load(Ordering::Relaxed)
            {
                return Err(ResourceError::native(-514));
            }
            if self.0.reject_admit.load(Ordering::Relaxed) == ordinal {
                assert!(self
                    .0
                    .rejected_admit_phase
                    .lock()
                    .unwrap()
                    .replace(capture.unwrap())
                    .is_none());
                return Err(ResourceError::native(-502));
            }
        }
        self.0
            .live
            .lock()
            .unwrap()
            .get_mut(&ticket.slot())
            .unwrap()
            .committed = true;
        Ok(())
    }
    fn release_after_free(&self, ticket: Ticket) -> Result<(), ResourceError> {
        let live = self.0.live.lock().unwrap().remove(&ticket.slot()).unwrap();
        if live.kind == ResourceKind::Geometry {
            self.0
                .all_geometry_live
                .fetch_sub(live.bytes, Ordering::Relaxed);
            self.0.all_geometry_releases.fetch_add(1, Ordering::Relaxed);
        }
        if live.capture.is_some() {
            self.0.capture_live.fetch_sub(live.bytes, Ordering::Relaxed);
            if self.0.reject_release.load(Ordering::Relaxed) {
                self.0
                    .refused_capture_refunds
                    .fetch_add(1, Ordering::Relaxed);
                self.0.failed_refunds.lock().unwrap().push((ticket, live));
                return Err(ResourceError::native(-503));
            }
        }
        Ok(())
    }
    fn checkpoint(&self, work: u64) -> Result<(), ResourceError> {
        if work == 256 && self.capture_phase() == Some(CapturePhase::Record) {
            self.0.capture_batches.fetch_add(1, Ordering::Relaxed);
        }
        if self.0.cancel.load(Ordering::Relaxed) {
            Err(ResourceError::new(FailureKind::Cancelled, LimitKind::None))
        } else {
            Ok(())
        }
    }
}
fn make_resources() -> (ResourcesRef, Arc<State>) {
    let state = Arc::new(State::default());
    let resources = ResourcesRef::new_prepaid(
        Host(state.clone()),
        FirstFailureRef::new_prepaid(),
        Limits::default(),
    );
    (resources, state)
}

#[derive(Debug)]
struct Proof(Vec<ReplaySourceCell>);
impl ReplayLogicalSource for Proof {
    fn cells(&self) -> &[ReplaySourceCell] {
        &self.0
    }
    fn checkpoint(&self, _: u64) -> Result<(), ()> {
        Ok(())
    }
    fn source_bidi_level(&self, _: usize) -> Option<u8> {
        Some(0)
    }
}
#[derive(Debug)]
struct Guard;
impl ReplayRetention for Guard {}
#[derive(Clone, Copy, Debug)]
struct GeometryObservation {
    glyph: u32,
    representation: u32,
    local_present: bool,
    fill_rule: FillRule,
    glyph_transform: Transform,
    cluster_transform: Transform,
    span_transform: Transform,
    transform: Transform,
}
// Test-only scalar snapshots do not retain a Path, its data Arc or its source.
// Paint flags describe the flatten callback boundary, before paint-server updates.
#[derive(Debug, PartialEq, Eq)]
struct FlattenPathObservation {
    verbs: Vec<u8>,
    points_bits: Vec<[u32; 2]>,
    abs_matrix_bits: [u32; 6],
    fill_rule: Option<u8>,
    visible: bool,
    has_stroke: bool,
    stroke_first: bool,
}
#[derive(Debug)]
struct Owner {
    db: Arc<fontdb::Database>,
    font: fontdb::ID,
    resources: Option<ResourcesRef>,
    state: Arc<State>,
    epoch: u64,
    compat_replay: bool,
    color: bool,
    synthetic: bool,
    synthetic_rule: FillRule,
    empty_synthetic: bool,
    zero_advance: bool,
    cancel_empty: bool,
    fail_source: bool,
    reject_release_on_source: bool,
    observed: Mutex<Vec<GeometryObservation>>,
    published: Mutex<Vec<(usize, u64, usize)>>,
    flatten_paths: Mutex<Vec<FlattenPathObservation>>,
    empty_evenodd_sources: Mutex<Vec<Vec<PlacedGlyphSource>>>,
}
impl FontReplay for Owner {
    fn coverage_epoch(&self) -> u64 {
        self.epoch
    }
    fn request_resources(&self) -> Option<ResourcesRef> {
        self.resources.clone()
    }
    fn reserve_replay_bytes(&self, _: usize) -> Result<Arc<dyn ReplayRetention>, ()> {
        Ok(Arc::new(Guard))
    }
    fn fontdb(&self) -> Result<Arc<fontdb::Database>, ()> {
        Ok(self.db.clone())
    }
    fn variations(&self, _: fontdb::ID) -> Result<Arc<[FontVariation]>, ()> {
        Ok(Arc::from([]))
    }
    fn prepare(&self, text: &Text, database: &mut Arc<fontdb::Database>) -> Result<ReplayText, ()> {
        *database = self.db.clone();
        let face = ReplayFace {
            id: self.font,
            synthetic_metrics: None,
        };
        let mut cells = Vec::new();
        let mut chunks = Vec::new();
        let mut styles = Vec::new();
        let mut base = 0;
        for (domain, chunk) in text.chunks().iter().enumerate() {
            let mut glyphs = Vec::new();
            for (at, ch) in chunk.text().char_indices() {
                let end = at + ch.len_utf8();
                let parent = cells.len();
                let visual = chunk
                    .spans()
                    .iter()
                    .position(|span| at >= span.start() && at < span.end())
                    .ok_or(())?;
                cells.push(ReplaySourceCell {
                    start: base + at,
                    end: base + end,
                });
                let glyph = match ch {
                    '_' => 0,
                    'S' => 2,
                    'C' => 1,
                    _ => {
                        if self.color {
                            2
                        } else {
                            1
                        }
                    }
                };
                glyphs.push(ReplayGlyph {
                    identity: (!self.compat_replay).then_some(ReplayGlyphIdentity {
                        parent,
                        part: parent,
                        part_count: 1,
                        position_domain: domain,
                        visual_span: visual,
                        semantic_em: chunk.spans()[visual].font_size().get(),
                        native_orientation: 0,
                        script_be: u32::from_be_bytes(*b"Latn"),
                    }),
                    start: at,
                    end,
                    glyph,
                    face,
                    geometry_origin: ReplayGeometryOrigin::FontDesignOriginYUp,
                    x_offset: 0,
                    y_offset: 0,
                    advance: if self.zero_advance { 0 } else { 600 },
                    y_advance: 0,
                    fallback_scale: if ch == 'B' { 0.75 } else { 1.0 },
                });
            }
            chunks.push(glyphs);
            styles.push(vec![face; chunk.spans().len()]);
            base += chunk.text().len();
        }
        Ok(ReplayText {
            logical: if self.compat_replay {
                None
            } else {
                Some(Arc::new(Proof(cells)))
            },
            chunks: chunks
                .into_iter()
                .map(Into::into)
                .collect::<Vec<_>>()
                .into(),
            styles: styles
                .into_iter()
                .map(Into::into)
                .collect::<Vec<_>>()
                .into(),
        })
    }
    fn outline(&self, _: fontdb::ID, glyph: u32) -> Result<ReplayOutline, ()> {
        Ok(if self.synthetic && glyph == 2 {
            let data = if self.empty_synthetic {
                None
            } else {
                let mut builder = tiny_skia_path::PathBuilder::new();
                builder.move_to(0.0, 0.0);
                builder.line_to(500.0, 0.0);
                builder.line_to(0.0, 700.0);
                builder.close();
                Some(Arc::new(builder.finish().unwrap()))
            };
            ReplayOutline::Synthetic {
                data,
                rule: self.synthetic_rule,
            }
        } else {
            ReplayOutline::Ordinary
        })
    }
    fn observe_geometry(&self, value: ReplayGeometryObservation<'_>) -> Result<(), ()> {
        let empty_evenodd = value.representation == 1
            && value.path.is_none()
            && value.fill_rule == FillRule::EvenOdd;
        if value.representation == 1 {
            self.state
                .synthetic_observations
                .fetch_add(1, Ordering::Relaxed);
        }
        if empty_evenodd {
            self.state
                .empty_evenodd_observations
                .fetch_add(1, Ordering::Relaxed);
        }
        self.observed.lock().unwrap().push(GeometryObservation {
            glyph: value.glyph,
            representation: value.representation,
            local_present: value.path.is_some(),
            fill_rule: value.fill_rule,
            transform: value.representation_transform,
            glyph_transform: value.glyph_transform,
            cluster_transform: value.cluster_transform,
            span_transform: value.span_transform,
        });
        if self.cancel_empty && (value.glyph == 0 || empty_evenodd) {
            self.state.cancel.store(true, Ordering::Relaxed);
        }
        if empty_evenodd
            && self
                .state
                .reject_empty_evenodd_observation
                .load(Ordering::Relaxed)
        {
            self.state
                .empty_evenodd_observation_rejections
                .fetch_add(1, Ordering::Relaxed);
            return Err(());
        }
        Ok(())
    }
    fn observe_glyph_fill_source(&self, source: &GlyphFillSource, path: &Path) -> Result<(), ()> {
        assert!(std::ptr::eq(source, path.glyph_fill_source().unwrap()));
        self.flatten_paths
            .lock()
            .unwrap()
            .push(flatten_path_observation(path));
        self.published.lock().unwrap().push((
            source.glyphs().as_ptr() as usize,
            source.epoch(),
            source.glyphs().len(),
        ));
        if self.empty_synthetic && self.synthetic_rule == FillRule::EvenOdd {
            self.empty_evenodd_sources
                .lock()
                .unwrap()
                .push(source.glyphs().to_vec());
        }
        if self.fail_source {
            if self.reject_release_on_source {
                self.state.reject_release.store(true, Ordering::Relaxed);
            }
            Err(())
        } else {
            Ok(())
        }
    }
}

fn owner(colr: bool, svg: bool, bitmap: bool) -> (Owner, ResourcesRef, Arc<State>) {
    let (resources, state) = make_resources();
    let mut db = fontdb::Database::new();
    let mut data = crate::parser::glyph_coverage_sfnt(colr, svg, bitmap);
    if svg {
        // Keep the original SFNT table layout. Use the same compliant relative
        // outline counterpart as the existing font-source profile tests.
        let before = b"M0 0H500V700H0Z";
        let at = data
            .windows(before.len())
            .position(|value| value == before)
            .unwrap();
        data[at..at + before.len()].copy_from_slice(b"M0 0h500v700H0z");
    }
    let font = db.push_face_info(fontdb::FaceInfo {
        id: fontdb::ID::dummy(),
        source: fontdb::Source::Binary(Arc::new(data)),
        index: 0,
        families: vec![("Mock".into(), fontdb::Language::English_UnitedStates)],
        post_script_name: String::new(),
        style: fontdb::Style::Normal,
        weight: fontdb::Weight::NORMAL,
        stretch: fontdb::Stretch::Normal,
        monospaced: false,
    });
    (
        Owner {
            db: Arc::new(db),
            font,
            resources: Some(resources.clone()),
            state: state.clone(),
            epoch: 77,
            compat_replay: false,
            color: colr || svg || bitmap,
            synthetic: false,
            synthetic_rule: FillRule::EvenOdd,
            empty_synthetic: false,
            zero_advance: false,
            cancel_empty: false,
            fail_source: false,
            reject_release_on_source: false,
            observed: Mutex::new(Vec::new()),
            published: Mutex::new(Vec::new()),
            flatten_paths: Mutex::new(Vec::new()),
            empty_evenodd_sources: Mutex::new(Vec::new()),
        },
        resources,
        state,
    )
}
fn options(owner: &Arc<Owner>, resources: Option<&ResourcesRef>) -> Options<'static> {
    Options {
        fontdb: owner.db.clone(),
        font_replay: Some(owner.clone()),
        processing: Arc::new(match resources {
            Some(resources) => ProcessingContext::with_resources(resources.clone()),
            None => ProcessingContext::default(),
        }),
        ..Options::default()
    }
}
fn parse(owner: &Arc<Owner>, resources: &ResourcesRef, text: &str) -> Result<Tree, crate::Error> {
    let _observation = ObserveRequest::enter(resources, &owner.state);
    let source = format!(
        r#"<svg xmlns="http://www.w3.org/2000/svg" width="240" height="100"><path d="M0 0H5V5Z"/><text id="t" x="10" y="60" font-family="Mock" font-size="20">{text}</text></svg>"#
    );
    Tree::from_data_in_request(source.as_bytes(), &options(owner, Some(resources)))
}
fn text(group: &Group) -> &Text {
    for node in group.children() {
        match node {
            Node::Text(text) => return text,
            Node::Group(group) => {
                if let Some(value) = text_option(group) {
                    return value;
                }
            }
            _ => (),
        }
    }
    panic!("expected actual Text node")
}
fn text_option(group: &Group) -> Option<&Text> {
    for node in group.children() {
        match node {
            Node::Text(text) => return Some(text),
            Node::Group(group) => {
                if let Some(value) = text_option(group) {
                    return Some(value);
                }
            }
            _ => (),
        }
    }
    None
}
fn paths<'a>(group: &'a Group, output: &mut Vec<&'a Path>) {
    for node in group.children() {
        match node {
            Node::Path(path) => output.push(path),
            Node::Group(group) => paths(group, output),
            Node::Text(text) => paths(text.flattened(), output),
            Node::Image(_) => (),
        }
    }
}
fn sources(tree: &Tree) -> Vec<&Path> {
    let mut result = Vec::new();
    paths(tree.root(), &mut result);
    result
        .into_iter()
        .filter(|path| path.glyph_fill_source().is_some())
        .collect()
}
fn matrix_bits(transform: Transform) -> [u32; 6] {
    [
        transform.sx,
        transform.ky,
        transform.kx,
        transform.sy,
        transform.tx,
        transform.ty,
    ]
    .map(f32::to_bits)
}

fn flatten_path_observation(path: &Path) -> FlattenPathObservation {
    use tiny_skia_path::PathVerb;
    FlattenPathObservation {
        verbs: path
            .data()
            .verbs()
            .iter()
            .map(|verb| match verb {
                PathVerb::Move => 0,
                PathVerb::Line => 1,
                PathVerb::Quad => 2,
                PathVerb::Cubic => 3,
                PathVerb::Close => 4,
            })
            .collect(),
        points_bits: path
            .data()
            .points()
            .iter()
            .map(|point| [point.x.to_bits(), point.y.to_bits()])
            .collect(),
        abs_matrix_bits: matrix_bits(path.abs_transform()),
        fill_rule: path.fill().map(|fill| match fill.rule() {
            FillRule::EvenOdd => 0,
            FillRule::NonZero => 1,
        }),
        visible: path.is_visible(),
        has_stroke: path.stroke().is_some(),
        stroke_first: path.paint_order() == PaintOrder::StrokeAndFill,
    }
}

fn assert_flatten_paths_match_solid_tree(owner: &Owner, tree: &Tree) {
    // Geometry and the abs matrix are compared with the returned Tree. These
    // selected fixtures use solid paints, so their paint flags also survive
    // paint-server updates; this is not a final-paint claim for other inputs.
    let found = sources(tree);
    let observed = owner.flatten_paths.lock().unwrap();
    assert_eq!(observed.len(), found.len());
    for (snapshot, path) in observed.iter().zip(found) {
        assert!(path
            .fill()
            .is_none_or(|fill| matches!(fill.paint(), Paint::Color(_))));
        assert!(path
            .stroke()
            .is_none_or(|stroke| matches!(stroke.paint(), Paint::Color(_))));
        assert_eq!(snapshot, &flatten_path_observation(path));
    }
}

#[test]
fn flatten_observer_keeps_actual_scene_matrix_and_solid_paint_flags() {
    for (attributes, fill_rule, visible, has_stroke, stroke_first) in [
        (r#"fill="red""#, Some(1), true, false, false),
        (
            r#"fill="red" stroke="blue" paint-order="stroke fill""#,
            Some(1),
            true,
            true,
            true,
        ),
        (r#"fill="none" stroke="blue""#, None, true, true, false),
        (
            r#"fill="red" visibility="hidden""#,
            Some(1),
            false,
            false,
            false,
        ),
    ] {
        let (owner, resources, state) = owner(false, false, false);
        let owner = Arc::new(owner);
        let source = format!(
            r#"<svg xmlns="http://www.w3.org/2000/svg" width="240" height="100"><g transform="matrix(1.25 0.125 -0.25 0.75 7.5 -3.25)"><text x="10" y="60" font-family="Mock" font-size="20" {attributes}>A_B</text></g></svg>"#
        );
        let tree = {
            let _observation = ObserveRequest::enter(&resources, &state);
            Tree::from_data_in_request(source.as_bytes(), &options(&owner, Some(&resources)))
                .unwrap()
        };
        assert_flatten_paths_match_solid_tree(&owner, &tree);
        let observed = owner.flatten_paths.lock().unwrap();
        assert_eq!(observed.len(), 1, "attributes={attributes}");
        let snapshot = &observed[0];
        assert_eq!(
            snapshot.abs_matrix_bits,
            [1.25f32, 0.125, -0.25, 0.75, 7.5, -3.25].map(f32::to_bits)
        );
        assert_eq!(
            (
                snapshot.fill_rule,
                snapshot.visible,
                snapshot.has_stroke,
                snapshot.stroke_first,
            ),
            (fill_rule, visible, has_stroke, stroke_first),
            "attributes={attributes}"
        );
        drop(observed);
        drop(tree);
        assert_eq!(state.capture_live.load(Ordering::Relaxed), 0);
        drop(owner);
        assert!(state.live.lock().unwrap().is_empty());
        resources.validate_retired().unwrap();
    }
}

#[test]
fn actual_ordinary_batch_keeps_empty_member_fallback_em_and_consumed_matrices() {
    let (owner, resources, state) = owner(false, false, false);
    let owner = Arc::new(owner);
    let tree = parse(&owner, &resources, "A_B").unwrap();
    let found = sources(&tree);
    assert_eq!(found.len(), 1);
    let source = found[0].glyph_fill_source().unwrap();
    assert_eq!(source.epoch(), 77);
    assert!(source.belongs_to(&resources));
    let members = source.glyphs();
    assert_eq!(members.len(), 3);
    assert_eq!(
        members
            .iter()
            .map(|value| value.layout_source_range)
            .collect::<Vec<_>>(),
        [[0, 1], [1, 2], [2, 3]]
    );
    assert_eq!(
        members
            .iter()
            .map(|value| (value.local_outline_present, value.contributed_to_path))
            .collect::<Vec<_>>(),
        [(true, true), (false, false), (true, true)]
    );
    assert_eq!(members[2].font_size.to_bits(), 15.0f32.to_bits());
    for (member, observed) in members.iter().zip(owner.observed.lock().unwrap().iter()) {
        assert_eq!(u32::from(member.glyph), observed.glyph);
        assert_eq!(member.font, owner.font);
        assert_eq!(member.units_per_em, 1000);
        assert_eq!(member.representation, GlyphOutlineKind::Ordinary);
        assert_eq!(observed.representation, 0);
        assert_eq!(member.local_outline_present, observed.local_present);
        assert_eq!(
            matrix_bits(member.outline_transform),
            matrix_bits(observed.transform)
        );
        assert_eq!(
            matrix_bits(member.glyph_transform),
            matrix_bits(observed.glyph_transform)
        );
        assert_eq!(
            matrix_bits(member.cluster_transform),
            matrix_bits(observed.cluster_transform)
        );
        assert_eq!(
            matrix_bits(member.span_transform),
            matrix_bits(observed.span_transform)
        );
        assert!(member.source_span.is_some());
        assert!(member.source_identity.is_some());
    }
    let actual = text(tree.root());
    for (member, glyph) in members.iter().zip(
        actual
            .layouted()
            .iter()
            .flat_map(|span| &span.positioned_glyphs),
    ) {
        assert_eq!(
            matrix_bits(member.outline_transform),
            matrix_bits(glyph.outline_transform())
        );
    }
    assert_eq!(
        owner.published.lock().unwrap().as_slice(),
        [(members.as_ptr() as usize, 77, 3)]
    );
    let mut all_paths = Vec::new();
    paths(tree.root(), &mut all_paths);
    assert!(all_paths
        .iter()
        .any(|path| path.glyph_fill_source().is_none()));
    assert_flatten_paths_match_solid_tree(&owner, &tree);
    drop(tree);
    assert_eq!(state.capture_live.load(Ordering::Relaxed), 0);
    drop(owner);
    resources.validate_retired().unwrap();
}

#[test]
fn same_origin_repeated_glyphs_are_not_deduplicated() {
    let (mut owner, resources, _) = owner(false, false, false);
    owner.zero_advance = true;
    let owner = Arc::new(owner);
    let tree = parse(&owner, &resources, "AA").unwrap();
    let found = sources(&tree);
    let members = found[0].glyph_fill_source().unwrap().glyphs();
    assert_eq!(members.len(), 2);
    assert_eq!(members[0].glyph, members[1].glyph);
    assert_eq!(
        matrix_bits(members[0].outline_transform),
        matrix_bits(members[1].outline_transform)
    );
    assert_ne!(
        members[0].source_identity.unwrap().parent,
        members[1].source_identity.unwrap().parent
    );
    drop(tree);
    drop(owner);
    resources.validate_retired().unwrap();
}

#[test]
fn synthetic_remains_a_single_member_path_between_ordinary_batches() {
    let (mut owner, resources, _) = owner(false, false, false);
    owner.synthetic = true;
    let owner = Arc::new(owner);
    let tree = parse(&owner, &resources, "ASA").unwrap();
    let found = sources(&tree);
    assert_eq!(found.len(), 3);
    for (index, path) in found.iter().enumerate() {
        let members = path.glyph_fill_source().unwrap().glyphs();
        assert_eq!(members.len(), 1);
        assert_eq!(
            members[0].representation,
            if index == 1 {
                GlyphOutlineKind::Synthetic
            } else {
                GlyphOutlineKind::Ordinary
            }
        );
        assert!(members[0].local_outline_present && members[0].contributed_to_path);
        if index == 1 {
            assert_eq!(
                path.glyph_fill_source().unwrap().retained_record_bytes(),
                std::mem::size_of::<PlacedGlyphSource>(),
                "the known single synthetic member must not retain a four-record buffer"
            );
        }
        assert_eq!(
            path.fill().unwrap().rule(),
            if index == 1 {
                FillRule::EvenOdd
            } else {
                FillRule::NonZero
            }
        );
    }
    assert_flatten_paths_match_solid_tree(&owner, &tree);
    drop(tree);
    drop(owner);
    resources.validate_retired().unwrap();
}

#[test]
fn color_boundaries_flush_ordinary_before_colr_svg_and_bitmap() {
    for representation in 0..3 {
        let (owner, resources, _) = owner(
            representation == 0,
            representation == 1,
            representation == 2,
        );
        let owner = Arc::new(owner);
        let tree = parse(&owner, &resources, "ACA").unwrap();
        let actual = text(tree.root());
        let found = sources(&tree);
        let counts = found
            .iter()
            .map(|path| path.glyph_fill_source().unwrap().glyphs().len())
            .collect::<Vec<_>>();
        assert_eq!(counts, vec![1, 1]);
        let kinds = actual
            .flattened()
            .children()
            .iter()
            .map(|node| match node {
                Node::Path(_) => 'P',
                Node::Group(_) => 'G',
                _ => '?',
            })
            .collect::<String>();
        assert_eq!(kinds, "PGP");
        for path in found {
            assert!(path
                .glyph_fill_source()
                .unwrap()
                .glyphs()
                .iter()
                .all(|member| member.glyph == 2
                    && member.representation == GlyphOutlineKind::Ordinary));
        }
        let mut color_paths = Vec::new();
        for node in actual.flattened().children() {
            if let Node::Group(group) = node {
                paths(group, &mut color_paths);
            }
        }
        assert!(color_paths
            .iter()
            .all(|path| path.glyph_fill_source().is_none()));
        drop(tree);
        drop(owner);
        resources.validate_retired().unwrap();
    }
}

// Correct painter-order regressions. The old GP/merged assertion and the
// original implementation's red runs are retained in the evidence snapshots.
fn assert_colr_painter_order(content: &str, expected: &str, compat_replay: bool) {
    let (mut owner, resources, _) = owner(true, false, false);
    owner.compat_replay = compat_replay;
    if compat_replay {
        owner.epoch = 0;
    }
    let owner = Arc::new(owner);
    let tree = parse(&owner, &resources, content).unwrap();
    let actual = text(tree.root());
    assert_eq!(actual.chunks().len(), 1);
    assert_eq!(actual.layouted().len(), 1);
    assert_eq!(actual.layouted()[0].positioned_glyphs.len(), content.len());
    assert!(actual.layouted()[0]
        .positioned_glyphs
        .iter()
        .all(|glyph| { glyph.source_span().is_none() == compat_replay }));
    let kinds = actual
        .flattened()
        .children()
        .iter()
        .map(|node| match node {
            Node::Path(_) => 'P',
            Node::Group(_) => 'G',
            _ => '?',
        })
        .collect::<String>();
    assert_eq!(kinds, expected, "content={content}, compat={compat_replay}");

    let ordinary_ranges = content
        .char_indices()
        .filter_map(|(at, ch)| (ch == 'A').then_some([at, at + ch.len_utf8()]))
        .collect::<Vec<_>>();
    let ordinary_paths = actual
        .flattened()
        .children()
        .iter()
        .filter_map(|node| {
            if let Node::Path(path) = node {
                Some(path.as_ref())
            } else {
                None
            }
        })
        .collect::<Vec<_>>();
    assert_eq!(ordinary_paths.len(), ordinary_ranges.len());
    let observations = owner.observed.lock().unwrap();
    assert_eq!(
        observations
            .iter()
            .map(|value| value.representation)
            .collect::<Vec<_>>(),
        content
            .chars()
            .map(|ch| if ch == 'C' { 2 } else { 0 })
            .collect::<Vec<_>>()
    );
    let ordinary_observations = observations
        .iter()
        .filter(|value| value.representation == 0)
        .collect::<Vec<_>>();
    assert_eq!(ordinary_observations.len(), ordinary_ranges.len());
    for ((path, range), observed) in ordinary_paths
        .iter()
        .zip(&ordinary_ranges)
        .zip(ordinary_observations)
    {
        if compat_replay {
            assert!(path.glyph_fill_source().is_none());
            continue;
        }
        let source = path.glyph_fill_source().unwrap();
        assert_eq!(source.epoch(), 77);
        assert!(source.belongs_to(&resources));
        assert_eq!(
            source.glyphs().len(),
            1,
            "ordinary members cannot span a COLR group"
        );
        let member = &source.glyphs()[0];
        assert_eq!(member.font, owner.font);
        assert_eq!(member.glyph, 2);
        assert_eq!(member.units_per_em, 1000);
        assert_eq!(member.font_size.to_bits(), 20.0f32.to_bits());
        assert_eq!(member.layout_source_range, *range);
        assert_eq!(member.source_span, Some(0));
        assert_eq!(member.source_identity.unwrap().parent, range[0]);
        assert_eq!(member.representation, GlyphOutlineKind::Ordinary);
        assert!(member.local_outline_present && member.contributed_to_path);
        assert_eq!(
            matrix_bits(member.glyph_transform),
            matrix_bits(observed.glyph_transform)
        );
        assert_eq!(
            matrix_bits(member.cluster_transform),
            matrix_bits(observed.cluster_transform)
        );
        assert_eq!(
            matrix_bits(member.span_transform),
            matrix_bits(observed.span_transform)
        );
        assert_eq!(
            matrix_bits(member.outline_transform),
            matrix_bits(observed.transform)
        );
    }
    for node in actual.flattened().children() {
        if let Node::Group(group) = node {
            let mut color_paths = Vec::new();
            paths(group, &mut color_paths);
            assert!(
                !color_paths.is_empty(),
                "fixture must contain an actual COLR layer"
            );
            assert!(color_paths
                .iter()
                .all(|path| path.glyph_fill_source().is_none()));
            assert!(color_paths.iter().any(|path| matches!(path.fill().map(|fill| fill.paint()),
                Some(Paint::Color(color)) if color.red == 255 && color.green == 0 && color.blue == 0)));
        }
    }
    assert_eq!(
        owner.published.lock().unwrap().len(),
        if compat_replay {
            0
        } else {
            ordinary_ranges.len()
        }
    );
    drop(observations);
    assert_flatten_paths_match_solid_tree(&owner, &tree);
    drop(tree);
    drop(owner);
    resources.validate_retired().unwrap();
}

#[test]
fn colr_order_selected_ordinary_then_color_is_pg() {
    assert_colr_painter_order("AC", "PG", false);
}

#[test]
fn colr_order_selected_color_then_ordinary_is_gp() {
    assert_colr_painter_order("CA", "GP", false);
}

#[test]
fn colr_order_selected_ordinary_color_ordinary_is_pgp_with_separate_sources() {
    assert_colr_painter_order("ACA", "PGP", false);
}

#[test]
fn colr_order_compat_ordinary_then_color_is_pg() {
    assert_colr_painter_order("AC", "PG", true);
}

#[test]
fn colr_order_compat_color_then_ordinary_is_gp() {
    assert_colr_painter_order("CA", "GP", true);
}

#[test]
fn colr_order_compat_ordinary_color_ordinary_is_pgp() {
    assert_colr_painter_order("ACA", "PGP", true);
}

fn assert_colr_flush_funding_failure(admission: bool) {
    let (owner, resources, state) = owner(true, false, false);
    // Reservation 1 is the first ordinary record block. Reservation/admission
    // 2 freezes that prefix at the newly corrected COLR boundary, before the
    // final A can be flattened. It is not an unrelated end-of-span failure.
    if admission {
        state.reject_admit.store(2, Ordering::Relaxed);
    } else {
        state.reject_reserve.store(2, Ordering::Relaxed);
    }
    let owner = Arc::new(owner);
    assert!(parse(&owner, &resources, "ACA").is_err());
    assert_eq!(
        *if admission {
            &state.rejected_admit_phase
        } else {
            &state.rejected_reserve_phase
        }
        .lock()
        .unwrap(),
        Some(CapturePhase::Shared),
        "the original prefix fault must hit its actual immutable Capture owner"
    );
    assert_eq!(
        resources.failure().unwrap().native_hr,
        Some(if admission { -502 } else { -501 })
    );
    assert_eq!(
        owner
            .observed
            .lock()
            .unwrap()
            .iter()
            .map(|value| value.representation)
            .collect::<Vec<_>>(),
        [0, 2]
    );
    assert!(owner.published.lock().unwrap().is_empty());
    assert_eq!(state.capture_reserves.load(Ordering::Relaxed), 2);
    assert_eq!(state.capture_live.load(Ordering::Relaxed), 0);
    drop(owner);
    resources.validate_retired().unwrap();
}

#[test]
fn colr_order_prefix_freeze_reserve_failure_aborts_before_trailing_ordinary() {
    assert_colr_flush_funding_failure(false);
}

#[test]
fn colr_order_prefix_freeze_admit_failure_aborts_before_trailing_ordinary() {
    assert_colr_flush_funding_failure(true);
}

#[test]
fn colr_order_prefix_observer_failure_retires_without_returning_partial_tree() {
    let (mut owner, resources, state) = owner(true, false, false);
    owner.fail_source = true;
    let owner = Arc::new(owner);
    assert!(parse(&owner, &resources, "ACA").is_err());
    assert_eq!(resources.failure().unwrap().kind, FailureKind::FontReplay);
    assert_eq!(
        owner
            .observed
            .lock()
            .unwrap()
            .iter()
            .map(|value| value.representation)
            .collect::<Vec<_>>(),
        [0, 2]
    );
    {
        let published = owner.published.lock().unwrap();
        assert_eq!(published.len(), 1);
        assert_eq!(published[0].1, 77);
        assert_eq!(
            published[0].2, 1,
            "the failed source contains only the preceding A"
        );
    }
    assert_eq!(state.capture_live.load(Ordering::Relaxed), 0);
    drop(owner);
    assert_eq!(resources.statistics().release_failures, 0);
    resources.validate_retired().unwrap();
}

#[test]
fn colr_order_empty_ordinary_prefix_adds_no_path_or_cross_color_member() {
    for compat_replay in [false, true] {
        for (content, expected) in [("_C", "G"), ("_CA", "GP")] {
            let (mut owner, resources, state) = owner(true, false, false);
            owner.compat_replay = compat_replay;
            if compat_replay {
                owner.epoch = 0;
            }
            let owner = Arc::new(owner);
            let tree = parse(&owner, &resources, content).unwrap();
            let actual = text(tree.root());
            let kinds = actual
                .flattened()
                .children()
                .iter()
                .map(|node| match node {
                    Node::Path(_) => 'P',
                    Node::Group(_) => 'G',
                    _ => '?',
                })
                .collect::<String>();
            assert_eq!(kinds, expected, "content={content}, compat={compat_replay}");
            let observed = owner.observed.lock().unwrap();
            assert_eq!(observed[0].glyph, 0);
            assert_eq!(observed[0].representation, 0);
            assert!(!observed[0].local_present);
            assert_eq!(observed[1].representation, 2);
            drop(observed);
            let found = sources(&tree);
            let expected_sources = usize::from(!compat_replay && content == "_CA");
            assert_eq!(found.len(), expected_sources);
            if let Some(path) = found.first() {
                let members = path.glyph_fill_source().unwrap().glyphs();
                assert_eq!(members.len(), 1);
                assert_eq!(members[0].glyph, 2);
                assert_eq!(members[0].layout_source_range, [2, 3]);
                assert!(members[0].local_outline_present && members[0].contributed_to_path);
            }
            assert_eq!(owner.published.lock().unwrap().len(), expected_sources);
            drop(tree);
            assert_eq!(state.capture_live.load(Ordering::Relaxed), 0);
            drop(owner);
            resources.validate_retired().unwrap();
        }
    }
}

#[test]
fn empty_and_empty_synthetic_batches_create_no_fill_source_or_extra_path() {
    for synthetic in [false, true] {
        let (mut baseline, baseline_resources, _) = owner(false, false, false);
        baseline.epoch = 0;
        baseline.synthetic = synthetic;
        baseline.empty_synthetic = synthetic;
        let baseline = Arc::new(baseline);
        let (mut active, resources, state) = owner(false, false, false);
        active.synthetic = synthetic;
        active.empty_synthetic = synthetic;
        let active = Arc::new(active);
        let content = if synthetic { "S" } else { "_" };
        // Observe the original epoch-zero behavior of this exact SFNT fixture;
        // an empty local outline does not predetermine whether Text is retained.
        let before = parse(&baseline, &baseline_resources, content);
        let after = parse(&active, &resources, content);
        match (before, after) {
            (Ok(before), Ok(after)) => {
                assert_eq!(
                    before.to_string(&WriteOptions::default()),
                    after.to_string(&WriteOptions::default())
                );
                assert!(sources(&after).is_empty());
                eprintln!(
                    "empty coverage fixture synthetic={synthetic}: epoch0=Ok, epoch77=Ok, observed={}",
                    active.observed.lock().unwrap().len()
                );
                drop((before, after));
            }
            (Err(_), Err(_)) => {
                assert_eq!(baseline_resources.failure(), resources.failure());
                eprintln!(
                    "empty coverage fixture synthetic={synthetic}: both original and captured parse failed"
                );
            }
            _ => panic!("capture changed the original empty fixture result"),
        }
        assert!(active.published.lock().unwrap().is_empty());
        assert!(active.flatten_paths.lock().unwrap().is_empty());
        assert!(baseline.flatten_paths.lock().unwrap().is_empty());
        assert_eq!(state.capture_live.load(Ordering::Relaxed), 0);
        assert!(
            state.capture_reserves.load(Ordering::Relaxed) <= 1,
            "an attempted empty batch may own records but cannot freeze a shared Path source"
        );
        drop((baseline, active));
        baseline_resources.validate_retired().unwrap();
        resources.validate_retired().unwrap();
    }
}

#[test]
fn epoch_zero_preserves_serialized_geometry_paint_and_decorations_have_no_source() {
    let (mut legacy, legacy_resources, legacy_state) = owner(false, false, false);
    legacy.epoch = 0;
    let legacy = Arc::new(legacy);
    let (active, active_resources, _) = owner(false, false, false);
    let active = Arc::new(active);
    let content = r#"<tspan text-decoration="underline overline line-through" fill="red" stroke="blue" paint-order="stroke fill">AB</tspan>"#;
    let before = parse(&legacy, &legacy_resources, content).unwrap();
    let after = parse(&active, &active_resources, content).unwrap();
    assert_eq!(
        before.to_string(&WriteOptions::default()),
        after.to_string(&WriteOptions::default())
    );
    assert!(sources(&before).is_empty());
    assert!(legacy.flatten_paths.lock().unwrap().is_empty());
    assert_flatten_paths_match_solid_tree(&active, &after);
    assert_eq!(legacy_state.capture_reserves.load(Ordering::Relaxed), 0);
    let mut actual = Vec::new();
    paths(text(after.root()).flattened(), &mut actual);
    assert_eq!(actual.len(), 4);
    assert_eq!(
        actual
            .iter()
            .filter(|path| path.glyph_fill_source().is_some())
            .count(),
        1
    );
    drop((before, after));
    drop((legacy, active));
    legacy_resources.validate_retired().unwrap();
    active_resources.validate_retired().unwrap();
}

#[test]
fn capture_rejects_missing_or_different_real_resources_without_compat_fallback() {
    for mismatch in 0..3 {
        let (mut owner, resources, state) = owner(false, false, false);
        let (other, _) = make_resources();
        if mismatch == 0 {
            owner.resources = None;
        }
        if mismatch == 2 {
            owner.resources = Some(other.clone());
        }
        let owner = Arc::new(owner);
        let mut cache = Cache::new(owner.db.clone());
        cache.font_replay = Some(owner);
        if mismatch != 1 {
            cache.processing = Arc::new(ProcessingContext::with_resources(resources.clone()));
        }
        assert!(Capture::new(&cache).is_none());
        assert_eq!(
            cache.processing.failure(),
            Some(ProcessingFailure::FontReplay)
        );
        assert_eq!(state.capture_reserves.load(Ordering::Relaxed), 0);
        drop(cache);
        resources.validate_retired().unwrap();
        other.validate_retired().unwrap();
    }
}

#[test]
fn no_owner_or_explicit_epoch_zero_needs_no_capture_resources() {
    let mut cache = Cache::new(Arc::new(fontdb::Database::new()));
    assert!(Capture::new(&cache).unwrap().enabled.is_none());
    let (mut owner, resources, state) = owner(false, false, false);
    owner.epoch = 0;
    owner.resources = None;
    cache.font_replay = Some(Arc::new(owner));
    assert!(Capture::new(&cache).unwrap().enabled.is_none());
    assert!(!cache.processing.failed());
    assert_eq!(state.capture_reserves.load(Ordering::Relaxed), 0);
    drop(cache);
    resources.validate_retired().unwrap();
}

#[test]
fn an_existing_first_fault_prevents_capture_without_replacing_it() {
    let (owner, resources, state) = owner(false, false, false);
    let mut cache = Cache::new(owner.db.clone());
    cache.font_replay = Some(Arc::new(owner));
    cache.processing = Arc::new(ProcessingContext::with_resources(resources.clone()));
    cache
        .processing
        .record_resource(ResourceError::native(-504));
    assert!(Capture::new(&cache).is_none());
    assert_eq!(
        cache.processing.resource_failure().unwrap().native_hr,
        Some(-504)
    );
    assert_eq!(state.capture_reserves.load(Ordering::Relaxed), 0);
    drop(cache);
    resources.validate_retired().unwrap();
}

#[test]
fn a_present_outline_with_nonfinite_transformed_points_is_not_claimed_as_appended() {
    let (mut owner, resources, _) = owner(false, false, false);
    let mut data = crate::parser::glyph_coverage_sfnt(false, false, false);
    let count = u16::from_be_bytes(data[4..6].try_into().unwrap()) as usize;
    let record = (0..count)
        .map(|index| 12 + index * 16)
        .find(|&at| &data[at..at + 4] == b"glyf")
        .unwrap();
    let offset =
        u32::from_be_bytes(data[record + 8..record + 12].try_into().unwrap()) as usize + 34;
    // Only glyph 2's original rectangle becomes large in design units. At the
    // same finite huge em, glyph 1 still produces a finite real Path.
    for at in [6, 8, 20, 30] {
        data[offset + at..offset + at + 2].copy_from_slice(&32767u16.to_be_bytes());
    }
    data[offset + 24..offset + 26].copy_from_slice(&(-32767i16).to_be_bytes());
    let mut db = fontdb::Database::new();
    owner.font = db.push_face_info(fontdb::FaceInfo {
        id: fontdb::ID::dummy(),
        source: fontdb::Source::Binary(Arc::new(data)),
        index: 0,
        families: vec![("Mock".into(), fontdb::Language::English_UnitedStates)],
        post_script_name: String::new(),
        style: fontdb::Style::Normal,
        weight: fontdb::Weight::NORMAL,
        stretch: fontdb::Stretch::Normal,
        monospaced: false,
    });
    owner.db = Arc::new(db);
    let owner = Arc::new(owner);
    let document = br#"<svg xmlns="http://www.w3.org/2000/svg" width="240" height="100"><text x="0" y="0" font-family="Mock" font-size="1e38">AS</text></svg>"#;
    let tree = {
        let _observation = ObserveRequest::enter(&resources, &owner.state);
        Tree::from_data_in_request(document, &options(&owner, Some(&resources))).unwrap()
    };
    let found = sources(&tree);
    assert_eq!(found.len(), 1);
    let members = found[0].glyph_fill_source().unwrap().glyphs();
    assert_eq!(members.len(), 2);
    assert!(members.iter().all(|member| member.local_outline_present));
    assert!(members[0].contributed_to_path);
    assert!(!members[1].contributed_to_path);
    assert!(members[1].outline_transform.is_finite());
    drop(tree);
    drop(owner);
    resources.validate_retired().unwrap();
}

#[test]
fn cancellation_at_a_short_empty_tail_or_all_empty_batch_is_observed() {
    for content in ["A_", "_"] {
        let (mut owner, resources, state) = owner(false, false, false);
        owner.cancel_empty = true;
        let owner = Arc::new(owner);
        assert!(parse(&owner, &resources, content).is_err());
        assert_eq!(resources.failure().unwrap().kind, FailureKind::Cancelled);
        assert_eq!(state.capture_live.load(Ordering::Relaxed), 0);
        assert!(owner.published.lock().unwrap().is_empty());
        drop(owner);
        resources.validate_retired().unwrap();
    }
}

#[test]
fn record_and_shared_owner_reserve_or_admit_failures_abort_actual_parse_and_retire() {
    for admission in [false, true] {
        for ordinal in [1, 2] {
            let (owner, resources, state) = owner(false, false, false);
            if admission {
                state.reject_admit.store(ordinal, Ordering::Relaxed);
            } else {
                state.reject_reserve.store(ordinal, Ordering::Relaxed);
            }
            let owner = Arc::new(owner);
            assert!(parse(&owner, &resources, "A").is_err());
            assert_eq!(
                *if admission {
                    &state.rejected_admit_phase
                } else {
                    &state.rejected_reserve_phase
                }
                .lock()
                .unwrap(),
                Some(if ordinal == 1 {
                    CapturePhase::Record
                } else {
                    CapturePhase::Shared
                }),
                "the original two fault ordinals must remain actual Capture owners"
            );
            assert_eq!(
                resources.failure().unwrap().native_hr,
                Some(if admission { -502 } else { -501 })
            );
            assert_eq!(state.capture_live.load(Ordering::Relaxed), 0);
            assert!(owner.published.lock().unwrap().is_empty());
            drop(owner);
            resources.validate_retired().unwrap();
        }
    }
}

#[test]
fn observer_rejection_discards_attached_source_and_preserves_first_fault() {
    let (mut owner, resources, state) = owner(false, false, false);
    owner.fail_source = true;
    state.reject_release.store(true, Ordering::Relaxed);
    let owner = Arc::new(owner);
    assert!(parse(&owner, &resources, "A").is_err());
    assert_eq!(owner.published.lock().unwrap().len(), 1);
    assert_eq!(resources.failure().unwrap().kind, FailureKind::FontReplay);
    assert_eq!(state.capture_live.load(Ordering::Relaxed), 0);
    drop(owner);
    let refused = state.refused_capture_refunds.load(Ordering::Relaxed);
    assert_eq!(
        refused, 2,
        "Host explicitly rejected both actual Capture ticket refunds"
    );
    assert_eq!(resources.statistics().release_failures, refused as u64);
    assert_eq!(resources.statistics().live_leases, refused as u64);
    assert_eq!(resources.statistics().scratch_bytes, 0);
    assert_eq!(resources.statistics().depth, 0);
    assert!(
        resources.validate_retired().is_err(),
        "real failed refunds cannot be called retired"
    );
}

#[test]
fn path_and_tree_clones_keep_the_last_shared_owner_fees_live() {
    let (owner, resources, state) = owner(false, false, false);
    let owner = Arc::new(owner);
    let tree = parse(&owner, &resources, "A").unwrap();
    let cloned_tree = tree.try_clone_in(tree.processing_context()).unwrap();
    let cloned_path = sources(&tree)[0]
        .try_clone_in(tree.processing_context())
        .unwrap();
    let retained = state.capture_live.load(Ordering::Relaxed);
    let reserves = state.capture_reserves.load(Ordering::Relaxed);
    assert_eq!(
        reserves, 2,
        "one actual record block and one shared batch block"
    );
    let source = cloned_path.glyph_fill_source().unwrap();
    eprintln!(
        "glyph coverage sizes: Path={}, Node={}, PlacedGlyphSource={}, retained record capacity bytes={}, retained Capture bytes={retained}",
        std::mem::size_of::<Path>(),
        std::mem::size_of::<Node>(),
        std::mem::size_of::<PlacedGlyphSource>(),
        source.retained_record_bytes()
    );
    drop(tree);
    assert_eq!(state.capture_live.load(Ordering::Relaxed), retained);
    drop(cloned_tree);
    assert_eq!(state.capture_live.load(Ordering::Relaxed), retained);
    assert_eq!(cloned_path.glyph_fill_source().unwrap().glyphs().len(), 1);
    drop(cloned_path);
    assert_eq!(state.capture_live.load(Ordering::Relaxed), 0);
    drop(owner);
    resources.validate_retired().unwrap();
}

fn nonzero_owner(compat_replay: bool) -> (Owner, ResourcesRef, Arc<State>) {
    let (mut owner, resources, state) = owner(false, false, false);
    owner.synthetic = true;
    owner.synthetic_rule = FillRule::NonZero;
    owner.compat_replay = compat_replay;
    (owner, resources, state)
}

fn flat_paths(tree: &Tree) -> Vec<&Path> {
    text(tree.root())
        .flattened()
        .children()
        .iter()
        .filter_map(|node| {
            if let Node::Path(path) = node {
                Some(path.as_ref())
            } else {
                None
            }
        })
        .collect()
}

fn assert_nonzero_members(
    path: &Path,
    owner: &Owner,
    resources: &ResourcesRef,
    expected: &[GlyphOutlineKind],
) {
    let source = path.glyph_fill_source().unwrap();
    assert_eq!(source.epoch(), owner.epoch);
    assert!(source.belongs_to(resources));
    assert_eq!(source.glyphs().len(), expected.len());
    let observations = owner.observed.lock().unwrap();
    assert_eq!(observations.len(), expected.len());
    for (index, ((member, representation), observed)) in source
        .glyphs()
        .iter()
        .zip(expected)
        .zip(observations.iter())
        .enumerate()
    {
        assert_eq!(member.representation, *representation);
        assert_eq!(u32::from(member.glyph), observed.glyph);
        assert_eq!(member.font, owner.font);
        assert_eq!(member.units_per_em, 1000);
        assert_eq!(member.layout_source_range, [index, index + 1]);
        assert_eq!(member.source_span, (!owner.compat_replay).then_some(0));
        assert_eq!(member.source_identity.is_none(), owner.compat_replay);
        assert_eq!(member.local_outline_present, observed.local_present);
        assert_eq!(
            matrix_bits(member.glyph_transform),
            matrix_bits(observed.glyph_transform)
        );
        assert_eq!(
            matrix_bits(member.cluster_transform),
            matrix_bits(observed.cluster_transform)
        );
        assert_eq!(
            matrix_bits(member.span_transform),
            matrix_bits(observed.span_transform)
        );
        assert_eq!(
            matrix_bits(member.outline_transform),
            matrix_bits(observed.transform)
        );
    }
}

#[test]
fn synthetic_nonzero_repeated_same_chunk_selected_and_true_compat_share_one_path() {
    for (compat_replay, epoch) in [(false, 77), (true, 77), (true, 0)] {
        let (mut owner, resources, state) = nonzero_owner(compat_replay);
        owner.epoch = epoch;
        owner.zero_advance = true;
        let owner = Arc::new(owner);
        let tree = parse(&owner, &resources, "SS").unwrap();
        let actual = text(tree.root());
        assert_eq!(actual.chunks().len(), 1);
        assert_eq!(actual.layouted().len(), 1);
        let placed = &actual.layouted()[0].positioned_glyphs;
        assert_eq!(placed.len(), 2);
        assert!(placed
            .iter()
            .all(|glyph| glyph.source_span().is_none() == compat_replay));
        assert_eq!(
            matrix_bits(placed[0].outline_transform()),
            matrix_bits(placed[1].outline_transform())
        );
        let found = flat_paths(&tree);
        assert_eq!(
            found.len(),
            1,
            "same-chunk NonZero Synthetic must be one fill, compat={compat_replay}, epoch={epoch}"
        );
        assert_eq!(found[0].fill().unwrap().rule(), FillRule::NonZero);
        let points = found[0].data().points();
        assert!(!points.is_empty());
        assert_eq!(points.len() % 2, 0);
        assert_eq!(&points[..points.len() / 2], &points[points.len() / 2..]);
        if epoch != 0 {
            assert_nonzero_members(
                found[0],
                &owner,
                &resources,
                &[GlyphOutlineKind::Synthetic; 2],
            );
            assert_eq!(owner.published.lock().unwrap().len(), 1);
            assert_eq!(state.growth_reserves.load(Ordering::Relaxed), 1);
            assert_eq!(state.growth_admits.load(Ordering::Relaxed), 1);
        } else {
            assert!(found[0].glyph_fill_source().is_none());
            assert!(owner.published.lock().unwrap().is_empty());
            assert_eq!(state.capture_reserves.load(Ordering::Relaxed), 0);
        }
        assert_eq!(state.synthetic_observations.load(Ordering::Relaxed), 2);
        assert_flatten_paths_match_solid_tree(&owner, &tree);
        drop(tree);
        assert_eq!(state.capture_live.load(Ordering::Relaxed), 0);
        drop(owner);
        resources.validate_retired().unwrap();
    }
}

#[test]
fn synthetic_nonzero_mixed_ordinary_members_keep_real_order_and_all_four_matrices() {
    for compat_replay in [false, true] {
        for content in ["ASA", "SA", "AS"] {
            let (mut owner, resources, state) = nonzero_owner(compat_replay);
            owner.zero_advance = true;
            let owner = Arc::new(owner);
            let tree = parse(&owner, &resources, content).unwrap();
            assert_eq!(text(tree.root()).chunks().len(), 1);
            assert_eq!(text(tree.root()).layouted().len(), 1);
            let found = flat_paths(&tree);
            assert_eq!(found.len(), 1, "content={content}, compat={compat_replay}");
            let expected = content
                .chars()
                .map(|ch| {
                    if ch == 'S' {
                        GlyphOutlineKind::Synthetic
                    } else {
                        GlyphOutlineKind::Ordinary
                    }
                })
                .collect::<Vec<_>>();
            assert_nonzero_members(found[0], &owner, &resources, &expected);
            assert!(found[0]
                .glyph_fill_source()
                .unwrap()
                .glyphs()
                .iter()
                .all(|member| member.local_outline_present && member.contributed_to_path));
            assert_eq!(owner.published.lock().unwrap().len(), 1);
            assert_flatten_paths_match_solid_tree(&owner, &tree);
            drop(tree);
            assert_eq!(state.capture_live.load(Ordering::Relaxed), 0);
            drop(owner);
            resources.validate_retired().unwrap();
        }
    }
}

#[test]
fn synthetic_nonzero_nonoverlapping_members_still_share_the_original_span_path() {
    let (owner, resources, state) = nonzero_owner(false);
    let owner = Arc::new(owner);
    let tree = parse(&owner, &resources, "SS").unwrap();
    let found = flat_paths(&tree);
    assert_eq!(found.len(), 1);
    assert_nonzero_members(
        found[0],
        &owner,
        &resources,
        &[GlyphOutlineKind::Synthetic; 2],
    );
    let members = found[0].glyph_fill_source().unwrap().glyphs();
    assert_ne!(
        matrix_bits(members[0].outline_transform),
        matrix_bits(members[1].outline_transform)
    );
    assert_eq!(
        members[1].outline_transform.tx - members[0].outline_transform.tx,
        12.0
    );
    assert_eq!(state.growth_reserves.load(Ordering::Relaxed), 1);
    drop(tree);
    drop(owner);
    resources.validate_retired().unwrap();
}

#[test]
fn synthetic_nonzero_empty_members_remain_in_the_actual_nonempty_batch() {
    for content in ["ASSA", "SSA"] {
        let (mut owner, resources, state) = nonzero_owner(false);
        owner.empty_synthetic = true;
        let owner = Arc::new(owner);
        let tree = parse(&owner, &resources, content).unwrap();
        let found = flat_paths(&tree);
        assert_eq!(found.len(), 1);
        let members = found[0].glyph_fill_source().unwrap().glyphs();
        assert_eq!(members.len(), content.len());
        for (member, ch) in members.iter().zip(content.chars()) {
            assert_eq!(
                member.representation,
                if ch == 'S' {
                    GlyphOutlineKind::Synthetic
                } else {
                    GlyphOutlineKind::Ordinary
                }
            );
            assert_eq!(member.local_outline_present, ch != 'S');
            assert_eq!(member.contributed_to_path, ch != 'S');
        }
        assert_eq!(owner.published.lock().unwrap().len(), 1);
        drop(tree);
        assert_eq!(state.capture_live.load(Ordering::Relaxed), 0);
        drop(owner);
        resources.validate_retired().unwrap();
    }
}

#[test]
fn synthetic_nonzero_all_empty_batch_adds_no_source_or_path_and_retires() {
    for compat_replay in [false, true] {
        let (mut baseline, baseline_resources, _) = nonzero_owner(compat_replay);
        baseline.empty_synthetic = true;
        baseline.epoch = 0;
        let baseline = Arc::new(baseline);
        let (mut owner, resources, state) = nonzero_owner(compat_replay);
        owner.empty_synthetic = true;
        let owner = Arc::new(owner);
        let before = parse(&baseline, &baseline_resources, "SS");
        let after = parse(&owner, &resources, "SS");
        match (before, after) {
            (Ok(before), Ok(after)) => {
                assert_eq!(
                    before.to_string(&WriteOptions::default()),
                    after.to_string(&WriteOptions::default())
                );
                assert!(sources(&after).is_empty());
                let mut actual_paths = Vec::new();
                paths(after.root(), &mut actual_paths);
                assert_eq!(
                    actual_paths.len(),
                    1,
                    "only the fixture's unrelated root shape remains"
                );
                drop((before, after));
            }
            (Err(_), Err(_)) => assert_eq!(baseline_resources.failure(), resources.failure()),
            _ => panic!("capture changed this all-empty fixture's actual parse result"),
        }
        assert!(owner.published.lock().unwrap().is_empty());
        assert!(owner.flatten_paths.lock().unwrap().is_empty());
        assert!(baseline.flatten_paths.lock().unwrap().is_empty());
        assert_eq!(
            state.synthetic_observations.load(Ordering::Relaxed),
            baseline
                .state
                .synthetic_observations
                .load(Ordering::Relaxed)
        );
        assert!(owner
            .observed
            .lock()
            .unwrap()
            .iter()
            .all(|observed| !observed.local_present));
        assert_eq!(state.capture_live.load(Ordering::Relaxed), 0);
        drop((baseline, owner));
        baseline_resources.validate_retired().unwrap();
        resources.validate_retired().unwrap();
    }
}

#[test]
fn synthetic_nonzero_different_style_spans_keep_separate_paths_and_sources() {
    let (owner, resources, state) = nonzero_owner(false);
    let owner = Arc::new(owner);
    let tree = parse(&owner, &resources, r#"SS<tspan fill="red">SS</tspan>SS"#).unwrap();
    let actual = text(tree.root());
    assert_eq!(actual.chunks().len(), 1);
    assert_eq!(actual.layouted().len(), 3);
    let found = flat_paths(&tree);
    assert_eq!(found.len(), 3);
    for (index, path) in found.iter().enumerate() {
        let source = path.glyph_fill_source().unwrap();
        assert!(source.belongs_to(&resources));
        assert_eq!(source.glyphs().len(), 2);
        assert!(source
            .glyphs()
            .iter()
            .all(|member| member.source_span == Some(index)
                && member.representation == GlyphOutlineKind::Synthetic));
    }
    assert!(
        matches!(found[1].fill().unwrap().paint(), Paint::Color(color) if color.red == 255 && color.green == 0 && color.blue == 0)
    );
    assert_eq!(owner.published.lock().unwrap().len(), 3);
    assert_flatten_paths_match_solid_tree(&owner, &tree);
    drop(tree);
    assert_eq!(state.capture_live.load(Ordering::Relaxed), 0);
    drop(owner);
    resources.validate_retired().unwrap();
}

#[test]
fn synthetic_nonzero_colr_svg_and_bitmap_boundaries_keep_synthetic_on_both_sides() {
    for representation in 0..3 {
        for compat_replay in [false, true] {
            let (mut owner, resources, state) = owner(
                representation == 0,
                representation == 1,
                representation == 2,
            );
            owner.synthetic = true;
            owner.synthetic_rule = FillRule::NonZero;
            owner.compat_replay = compat_replay;
            let owner = Arc::new(owner);
            let tree = parse(&owner, &resources, "SCS").unwrap();
            let actual = text(tree.root());
            assert_eq!(actual.chunks().len(), 1);
            assert_eq!(actual.layouted().len(), 1);
            let kinds = actual
                .flattened()
                .children()
                .iter()
                .map(|node| match node {
                    Node::Path(_) => 'P',
                    Node::Group(_) => 'G',
                    _ => '?',
                })
                .collect::<String>();
            assert_eq!(
                kinds, "PGP",
                "representation={representation}, compat={compat_replay}"
            );
            let found = sources(&tree);
            assert_eq!(found.len(), 2);
            for (path, range) in found.iter().zip([[0, 1], [2, 3]]) {
                let source = path.glyph_fill_source().unwrap();
                assert!(source.belongs_to(&resources));
                assert_eq!(source.glyphs().len(), 1);
                assert_eq!(
                    source.glyphs()[0].representation,
                    GlyphOutlineKind::Synthetic
                );
                assert_eq!(source.glyphs()[0].layout_source_range, range);
                assert_eq!(
                    source.retained_record_bytes(),
                    std::mem::size_of::<PlacedGlyphSource>()
                );
            }
            for node in actual.flattened().children() {
                if let Node::Group(group) = node {
                    let mut color_paths = Vec::new();
                    paths(group, &mut color_paths);
                    assert!(color_paths
                        .iter()
                        .all(|path| path.glyph_fill_source().is_none()));
                }
            }
            assert_flatten_paths_match_solid_tree(&owner, &tree);
            drop(tree);
            assert_eq!(state.capture_live.load(Ordering::Relaxed), 0);
            drop(owner);
            resources.validate_retired().unwrap();
        }
    }
}

fn assert_synthetic_nonzero_growth_failure(admission: bool, reject_release: bool) {
    let (owner, resources, state) = nonzero_owner(false);
    if admission {
        state.reject_growth_admit.store(true, Ordering::Relaxed);
    } else {
        state.reject_growth_reserve.store(true, Ordering::Relaxed);
    }
    state
        .reject_release
        .store(reject_release, Ordering::Relaxed);
    let owner = Arc::new(owner);
    assert!(
        parse(&owner, &resources, "SS").is_err(),
        "must hit the second Synthetic exact1-to-four-record growth"
    );
    assert_eq!(state.synthetic_observations.load(Ordering::Relaxed), 2);
    assert_eq!(state.growth_reserves.load(Ordering::Relaxed), 1);
    assert_eq!(
        state.growth_admits.load(Ordering::Relaxed),
        usize::from(admission)
    );
    assert_eq!(
        resources.failure().unwrap().native_hr,
        Some(if admission { -512 } else { -511 })
    );
    assert!(owner.published.lock().unwrap().is_empty());
    assert_eq!(state.capture_live.load(Ordering::Relaxed), 0);
    drop(owner);
    if reject_release {
        let refused = state.refused_capture_refunds.load(Ordering::Relaxed);
        assert_eq!(refused, if admission { 2 } else { 1 });
        assert_eq!(resources.statistics().release_failures, refused as u64);
        assert_eq!(resources.statistics().live_leases, refused as u64);
        assert!(resources.validate_retired().is_err());
    } else {
        assert_eq!(resources.statistics().release_failures, 0);
        resources.validate_retired().unwrap();
    }
}

#[test]
fn synthetic_nonzero_second_member_real_growth_reserve_and_admit_failures_abort_and_retire() {
    for admission in [false, true] {
        assert_synthetic_nonzero_growth_failure(admission, false);
    }
}

#[test]
fn synthetic_nonzero_growth_first_fault_survives_actual_refund_rejections() {
    for admission in [false, true] {
        assert_synthetic_nonzero_growth_failure(admission, true);
    }
}

#[test]
fn synthetic_nonzero_shared_owner_reserve_failure_is_after_real_member_growth() {
    for admission in [false, true] {
        let (owner, resources, state) = nonzero_owner(false);
        if admission {
            state
                .reject_shared_admit_after_growth
                .store(true, Ordering::Relaxed);
        } else {
            state
                .reject_shared_after_growth
                .store(true, Ordering::Relaxed);
        }
        let owner = Arc::new(owner);
        assert!(parse(&owner, &resources, "SS").is_err());
        assert_eq!(state.synthetic_observations.load(Ordering::Relaxed), 2);
        assert_eq!(state.growth_reserves.load(Ordering::Relaxed), 1);
        assert_eq!(state.growth_admits.load(Ordering::Relaxed), 1);
        assert_eq!(
            state.shared_after_growth_attempts.load(Ordering::Relaxed),
            1
        );
        assert_eq!(
            resources.failure().unwrap().native_hr,
            Some(if admission { -514 } else { -513 })
        );
        assert!(owner.published.lock().unwrap().is_empty());
        assert_eq!(state.capture_live.load(Ordering::Relaxed), 0);
        drop(owner);
        resources.validate_retired().unwrap();
    }
}

#[test]
fn synthetic_nonzero_cancellation_at_real_growth_retires_without_publishing_a_prefix() {
    let (owner, resources, state) = nonzero_owner(false);
    state.cancel_growth_admit.store(true, Ordering::Relaxed);
    let owner = Arc::new(owner);
    assert!(parse(&owner, &resources, "SS").is_err());
    assert_eq!(state.synthetic_observations.load(Ordering::Relaxed), 2);
    assert_eq!(state.growth_reserves.load(Ordering::Relaxed), 1);
    assert_eq!(state.growth_admits.load(Ordering::Relaxed), 1);
    assert_eq!(resources.failure().unwrap().kind, FailureKind::Cancelled);
    assert!(owner.published.lock().unwrap().is_empty());
    assert_eq!(state.capture_live.load(Ordering::Relaxed), 0);
    drop(owner);
    resources.validate_retired().unwrap();
}

#[test]
fn synthetic_nonzero_observer_rejection_sees_complete_batch_and_no_successful_tree() {
    for reject_release in [false, true] {
        let (mut owner, resources, state) = nonzero_owner(false);
        owner.fail_source = true;
        owner.reject_release_on_source = reject_release;
        let owner = Arc::new(owner);
        assert!(parse(&owner, &resources, "SS").is_err());
        assert_eq!(state.synthetic_observations.load(Ordering::Relaxed), 2);
        assert_eq!(state.growth_admits.load(Ordering::Relaxed), 1);
        let published = owner.published.lock().unwrap();
        assert_eq!(published.len(), 1);
        assert_eq!(published[0].1, 77);
        assert_eq!(
            published[0].2, 2,
            "observer receives both members, never an earlier one-member prefix"
        );
        drop(published);
        assert_eq!(resources.failure().unwrap().kind, FailureKind::FontReplay);
        assert_eq!(state.capture_live.load(Ordering::Relaxed), 0);
        drop(owner);
        if reject_release {
            assert_eq!(state.refused_capture_refunds.load(Ordering::Relaxed), 2);
            assert_eq!(resources.statistics().release_failures, 2);
            assert_eq!(resources.statistics().live_leases, 2);
            assert!(resources.validate_retired().is_err());
        } else {
            resources.validate_retired().unwrap();
        }
    }
}

#[test]
fn synthetic_nonzero_grown_batch_clones_retain_same_request_until_last_alias_drops() {
    let (owner, resources, state) = nonzero_owner(false);
    let owner = Arc::new(owner);
    let tree = parse(&owner, &resources, "SS").unwrap();
    let found = sources(&tree);
    assert_eq!(found.len(), 1);
    let source = found[0].glyph_fill_source().unwrap();
    assert!(source.belongs_to(&resources));
    assert_eq!(source.glyphs().len(), 2);
    assert_eq!(
        source.retained_record_bytes(),
        4 * std::mem::size_of::<PlacedGlyphSource>()
    );
    assert_eq!(state.growth_reserves.load(Ordering::Relaxed), 1);
    assert_eq!(state.growth_admits.load(Ordering::Relaxed), 1);
    assert_eq!(
        state.capture_reserves.load(Ordering::Relaxed),
        3,
        "exact1, real growth, final shared owner"
    );
    let alias = found[0].try_clone_in(tree.processing_context()).unwrap();
    let tree_alias = tree.try_clone_in(tree.processing_context()).unwrap();
    let retained = state.capture_live.load(Ordering::Relaxed);
    drop(tree);
    assert_eq!(state.capture_live.load(Ordering::Relaxed), retained);
    drop(tree_alias);
    assert_eq!(state.capture_live.load(Ordering::Relaxed), retained);
    assert!(alias.glyph_fill_source().unwrap().belongs_to(&resources));
    drop(alias);
    assert_eq!(state.capture_live.load(Ordering::Relaxed), 0);
    drop(owner);
    resources.validate_retired().unwrap();
}

fn empty_evenodd_owner(compat_replay: bool) -> (Owner, ResourcesRef, Arc<State>) {
    let (mut owner, resources, state) = owner(false, false, false);
    owner.synthetic = true;
    owner.synthetic_rule = FillRule::EvenOdd;
    owner.empty_synthetic = true;
    owner.compat_replay = compat_replay;
    (owner, resources, state)
}

fn assert_empty_evenodd_members(
    path: &Path,
    owner: &Owner,
    resources: &ResourcesRef,
    content: &str,
) {
    let expected = content
        .chars()
        .map(|ch| {
            if ch == 'S' {
                GlyphOutlineKind::Synthetic
            } else {
                GlyphOutlineKind::Ordinary
            }
        })
        .collect::<Vec<_>>();
    // The existing checker compares every source range and all four matrices
    // with actual geometry observations, including the real None/None branch.
    assert_nonzero_members(path, owner, resources, &expected);
    let source = path.glyph_fill_source().unwrap();
    let observations = owner.observed.lock().unwrap();
    for (index, ((member, observed), ch)) in source
        .glyphs()
        .iter()
        .zip(observations.iter())
        .zip(content.chars())
        .enumerate()
    {
        let ordinary = ch == 'A';
        assert_eq!(member.glyph, if ordinary { 1 } else { 2 });
        assert_eq!(member.local_outline_present, ordinary);
        assert_eq!(member.contributed_to_path, ordinary);
        assert_eq!(observed.representation, u32::from(!ordinary));
        assert_eq!(
            observed.fill_rule,
            if ordinary {
                FillRule::NonZero
            } else {
                FillRule::EvenOdd
            }
        );
        if let Some(identity) = member.source_identity {
            assert_eq!(identity.parent, index);
            assert_eq!(identity.part, index);
            assert_eq!(identity.part_count, 1);
            assert_eq!(identity.position_domain, 0);
            assert_eq!(identity.visual_span, 0);
        }
    }
    assert_eq!(
        owner.published.lock().unwrap().as_slice(),
        [(source.glyphs().as_ptr() as usize, 77, content.len())]
    );
}

#[test]
fn empty_evenodd_members_keep_one_actual_path_selected_and_true_compat_epoch_zero_and_77() {
    for compat_replay in [false, true] {
        for epoch in [0, 77] {
            for content in ["ASA", "ASSA", "SSA", "ASS"] {
                let (mut owner, resources, state) = empty_evenodd_owner(compat_replay);
                owner.epoch = epoch;
                owner.zero_advance = true;
                let owner = Arc::new(owner);
                let tree = parse(&owner, &resources, content).unwrap();
                let actual = text(tree.root());
                assert_eq!(actual.chunks().len(), 1);
                assert_eq!(actual.layouted().len(), 1);
                let placed = &actual.layouted()[0].positioned_glyphs;
                assert_eq!(placed.len(), content.len());
                for glyph in placed {
                    assert_eq!(glyph.source_span().is_none(), compat_replay);
                    assert_eq!(glyph.outline_transform().tx, 10.0);
                    assert_eq!(glyph.outline_transform().ty, 60.0);
                }
                let found = flat_paths(&tree);
                assert_eq!(
                    found.len(),
                    1,
                    "empty EvenOdd cannot split a painted span: {content}, compat={compat_replay}, epoch={epoch}"
                );
                assert_eq!(actual.flattened().children().len(), 1);
                assert_eq!(found[0].fill().unwrap().rule(), FillRule::NonZero);
                assert!(!found[0].data().points().is_empty());
                let observed = owner.observed.lock().unwrap();
                assert_eq!(observed.len(), content.len());
                for (value, ch) in observed.iter().zip(content.chars()) {
                    assert_eq!(value.local_present, ch == 'A');
                    assert_eq!(value.representation, u32::from(ch == 'S'));
                    assert_eq!(
                        value.fill_rule,
                        if ch == 'S' {
                            FillRule::EvenOdd
                        } else {
                            FillRule::NonZero
                        }
                    );
                }
                drop(observed);
                assert_eq!(
                    state.empty_evenodd_observations.load(Ordering::Relaxed),
                    content.bytes().filter(|&ch| ch == b'S').count()
                );
                if epoch == 77 {
                    assert_empty_evenodd_members(found[0], &owner, &resources, content);
                } else {
                    assert!(found[0].glyph_fill_source().is_none());
                    assert!(owner.published.lock().unwrap().is_empty());
                    assert_eq!(state.capture_reserves.load(Ordering::Relaxed), 0);
                }
                assert_flatten_paths_match_solid_tree(&owner, &tree);
                drop(tree);
                assert_eq!(state.capture_live.load(Ordering::Relaxed), 0);
                drop(owner);
                assert!(state.live.lock().unwrap().is_empty());
                resources.validate_retired().unwrap();
            }
        }
    }
}

#[test]
fn empty_evenodd_all_empty_batches_create_no_path_source_or_remaining_lease() {
    for compat_replay in [false, true] {
        for epoch in [0, 77] {
            for content in ["S", "SS"] {
                let (mut owner, resources, state) = empty_evenodd_owner(compat_replay);
                owner.epoch = epoch;
                let owner = Arc::new(owner);
                let tree = parse(&owner, &resources, content).unwrap();
                assert!(sources(&tree).is_empty());
                let mut found = Vec::new();
                paths(tree.root(), &mut found);
                assert_eq!(found.len(), 1, "only the unrelated root shape remains");
                assert!(owner.published.lock().unwrap().is_empty());
                assert!(owner.flatten_paths.lock().unwrap().is_empty());
                assert_eq!(
                    state.empty_evenodd_observations.load(Ordering::Relaxed),
                    content.len()
                );
                assert_eq!(
                    state.empty_evenodd_growth_admits.load(Ordering::Relaxed),
                    usize::from(epoch == 77 && content == "SS")
                );
                assert_eq!(state.capture_live.load(Ordering::Relaxed), 0);
                drop(tree);
                drop(owner);
                assert!(state.live.lock().unwrap().is_empty());
                resources.validate_retired().unwrap();
            }
        }
    }
}

#[test]
fn empty_evenodd_style_boundaries_do_not_transfer_empty_members_between_spans() {
    for compat_replay in [false, true] {
        let (owner, resources, state) = empty_evenodd_owner(compat_replay);
        let owner = Arc::new(owner);
        let tree = parse(&owner, &resources, r#"AS<tspan fill="red">SA</tspan>S"#).unwrap();
        assert_eq!(text(tree.root()).chunks().len(), 1);
        let found = flat_paths(&tree);
        assert_eq!(found.len(), 2);
        for (span, (path, ranges)) in found
            .iter()
            .zip([[[0, 1], [1, 2]], [[2, 3], [3, 4]]])
            .enumerate()
        {
            let source = path.glyph_fill_source().unwrap();
            assert!(source.belongs_to(&resources));
            assert_eq!(source.epoch(), 77);
            assert_eq!(source.glyphs().len(), 2);
            for (member, range) in source.glyphs().iter().zip(ranges) {
                assert_eq!(member.layout_source_range, range);
                assert_eq!(member.source_span, (!compat_replay).then_some(span));
                let ordinary = matches!(range[0], 0 | 3);
                assert_eq!(member.local_outline_present, ordinary);
                assert_eq!(member.contributed_to_path, ordinary);
                if let Some(identity) = member.source_identity {
                    assert_eq!(identity.visual_span, span);
                }
            }
        }
        assert!(
            matches!(found[1].fill().unwrap().paint(), Paint::Color(color) if color.red == 255 && color.green == 0 && color.blue == 0)
        );
        assert_eq!(owner.observed.lock().unwrap().len(), 5);
        assert_eq!(state.empty_evenodd_observations.load(Ordering::Relaxed), 3);
        assert_eq!(owner.published.lock().unwrap().len(), 2);
        assert_flatten_paths_match_solid_tree(&owner, &tree);
        drop(tree);
        assert_eq!(state.capture_live.load(Ordering::Relaxed), 0);
        drop(owner);
        resources.validate_retired().unwrap();
    }
}

#[test]
fn empty_evenodd_color_boundaries_drop_empty_prefix_and_tail_without_creating_sources() {
    for representation in 0..3 {
        for compat_replay in [false, true] {
            let (mut owner, resources, state) = owner(
                representation == 0,
                representation == 1,
                representation == 2,
            );
            owner.synthetic = true;
            owner.synthetic_rule = FillRule::EvenOdd;
            owner.empty_synthetic = true;
            owner.compat_replay = compat_replay;
            let owner = Arc::new(owner);
            let tree = parse(&owner, &resources, "SCS").unwrap();
            let actual = text(tree.root());
            assert_eq!(actual.flattened().children().len(), 1);
            assert!(matches!(actual.flattened().children()[0], Node::Group(_)));
            assert!(sources(&tree).is_empty());
            assert_eq!(state.empty_evenodd_observations.load(Ordering::Relaxed), 2);
            assert!(owner.published.lock().unwrap().is_empty());
            assert!(owner.flatten_paths.lock().unwrap().is_empty());
            drop(tree);
            assert_eq!(state.capture_live.load(Ordering::Relaxed), 0);
            drop(owner);
            resources.validate_retired().unwrap();
        }
    }
}

fn assert_empty_evenodd_growth_failure(compat_replay: bool, admission: bool, reject_release: bool) {
    let (owner, resources, state) = empty_evenodd_owner(compat_replay);
    if admission {
        state
            .reject_empty_evenodd_growth_admit
            .store(true, Ordering::Relaxed);
    } else {
        state
            .reject_empty_evenodd_growth_reserve
            .store(true, Ordering::Relaxed);
    }
    state
        .reject_release
        .store(reject_release, Ordering::Relaxed);
    let owner = Arc::new(owner);
    assert!(
        parse(&owner, &resources, "SSA").is_err(),
        "the second None/EvenOdd member must grow its own still-live exact1 records"
    );
    assert_eq!(state.empty_evenodd_observations.load(Ordering::Relaxed), 2);
    assert_eq!(
        state.empty_evenodd_growth_reserves.load(Ordering::Relaxed),
        1
    );
    assert_eq!(
        state.empty_evenodd_growth_admits.load(Ordering::Relaxed),
        usize::from(admission)
    );
    let observed = owner.observed.lock().unwrap();
    assert_eq!(observed.len(), 2, "growth refusal precedes the ordinary A");
    assert!(observed.iter().all(|value| value.representation == 1
        && !value.local_present
        && value.fill_rule == FillRule::EvenOdd));
    drop(observed);
    let first = resources.failure().unwrap();
    assert_eq!(first.native_hr, Some(if admission { -522 } else { -521 }));
    assert!(owner.published.lock().unwrap().is_empty());
    assert_eq!(state.capture_live.load(Ordering::Relaxed), 0);
    drop(owner);
    assert!(state.live.lock().unwrap().is_empty());
    assert_eq!(resources.failure(), Some(first));
    if reject_release {
        let refused = state.refused_capture_refunds.load(Ordering::Relaxed);
        assert_eq!(refused, if admission { 2 } else { 1 });
        assert_eq!(resources.statistics().release_failures, refused as u64);
        assert_eq!(resources.statistics().live_leases, refused as u64);
        assert!(resources.validate_retired().is_err());
    } else {
        assert_eq!(resources.statistics().release_failures, 0);
        resources.validate_retired().unwrap();
    }
}

#[test]
fn empty_evenodd_second_member_real_growth_reserve_and_admit_failures_abort_and_retire() {
    for compat_replay in [false, true] {
        for admission in [false, true] {
            assert_empty_evenodd_growth_failure(compat_replay, admission, false);
        }
    }
}

#[test]
fn empty_evenodd_growth_first_fault_survives_actual_refund_rejections() {
    for compat_replay in [false, true] {
        for admission in [false, true] {
            assert_empty_evenodd_growth_failure(compat_replay, admission, true);
        }
    }
}

#[test]
fn empty_evenodd_source_observer_rejection_sees_all_empty_members_and_keeps_first_fault() {
    for compat_replay in [false, true] {
        for reject_release in [false, true] {
            let (mut owner, resources, state) = empty_evenodd_owner(compat_replay);
            owner.fail_source = true;
            owner.reject_release_on_source = reject_release;
            let owner = Arc::new(owner);
            assert!(parse(&owner, &resources, "SSA").is_err());
            assert_eq!(state.empty_evenodd_observations.load(Ordering::Relaxed), 2);
            assert_eq!(state.empty_evenodd_growth_admits.load(Ordering::Relaxed), 1);
            assert_eq!(owner.published.lock().unwrap().len(), 1);
            let snapshots = owner.empty_evenodd_sources.lock().unwrap();
            assert_eq!(snapshots.len(), 1);
            assert_eq!(snapshots[0].len(), 3);
            for (index, member) in snapshots[0].iter().enumerate() {
                assert_eq!(member.layout_source_range, [index, index + 1]);
                assert_eq!(member.local_outline_present, index == 2);
                assert_eq!(member.contributed_to_path, index == 2);
                assert_eq!(member.source_identity.is_none(), compat_replay);
            }
            drop(snapshots);
            assert_eq!(resources.failure().unwrap().kind, FailureKind::FontReplay);
            assert_eq!(state.capture_live.load(Ordering::Relaxed), 0);
            drop(owner);
            assert!(state.live.lock().unwrap().is_empty());
            assert_eq!(resources.failure().unwrap().kind, FailureKind::FontReplay);
            if reject_release {
                assert_eq!(state.refused_capture_refunds.load(Ordering::Relaxed), 2);
                assert_eq!(resources.statistics().release_failures, 2);
                assert_eq!(resources.statistics().live_leases, 2);
                assert!(resources.validate_retired().is_err());
            } else {
                resources.validate_retired().unwrap();
            }
        }
    }
}

#[test]
fn empty_evenodd_geometry_observer_rejection_aborts_before_record_or_prefix_publication() {
    for compat_replay in [false, true] {
        for epoch in [0, 77] {
            let (mut owner, resources, state) = empty_evenodd_owner(compat_replay);
            owner.epoch = epoch;
            state
                .reject_empty_evenodd_observation
                .store(true, Ordering::Relaxed);
            let owner = Arc::new(owner);
            assert!(parse(&owner, &resources, "ASA").is_err());
            assert_eq!(
                state
                    .empty_evenodd_observation_rejections
                    .load(Ordering::Relaxed),
                1
            );
            assert_eq!(state.empty_evenodd_observations.load(Ordering::Relaxed), 1);
            let observed = owner.observed.lock().unwrap();
            assert_eq!(
                observed.len(),
                2,
                "the rejected empty observer stops before the trailing A"
            );
            assert_eq!(observed[0].representation, 0);
            assert_eq!(observed[1].fill_rule, FillRule::EvenOdd);
            assert!(!observed[1].local_present);
            drop(observed);
            assert!(owner.published.lock().unwrap().is_empty());
            assert_eq!(resources.failure().unwrap().kind, FailureKind::FontReplay);
            assert_eq!(state.capture_live.load(Ordering::Relaxed), 0);
            drop(owner);
            assert!(state.live.lock().unwrap().is_empty());
            resources.validate_retired().unwrap();
        }
    }
}

#[test]
fn empty_evenodd_cancellation_after_real_second_member_growth_retires_without_source() {
    for compat_replay in [false, true] {
        let (owner, resources, state) = empty_evenodd_owner(compat_replay);
        state
            .cancel_empty_evenodd_growth_admit
            .store(true, Ordering::Relaxed);
        let owner = Arc::new(owner);
        assert!(parse(&owner, &resources, "SS").is_err());
        assert_eq!(state.empty_evenodd_observations.load(Ordering::Relaxed), 2);
        assert_eq!(
            state.empty_evenodd_growth_reserves.load(Ordering::Relaxed),
            1
        );
        assert_eq!(state.empty_evenodd_growth_admits.load(Ordering::Relaxed), 1);
        assert_eq!(resources.failure().unwrap().kind, FailureKind::Cancelled);
        assert!(owner.published.lock().unwrap().is_empty());
        assert_eq!(state.capture_live.load(Ordering::Relaxed), 0);
        drop(owner);
        assert!(state.live.lock().unwrap().is_empty());
        resources.validate_retired().unwrap();
    }
}

#[test]
fn empty_evenodd_cancellation_at_short_empty_tail_or_all_empty_batch_is_observed() {
    for compat_replay in [false, true] {
        for content in ["AS", "S"] {
            let (mut owner, resources, state) = empty_evenodd_owner(compat_replay);
            owner.cancel_empty = true;
            let owner = Arc::new(owner);
            assert!(parse(&owner, &resources, content).is_err());
            assert_eq!(state.empty_evenodd_observations.load(Ordering::Relaxed), 1);
            assert_eq!(resources.failure().unwrap().kind, FailureKind::Cancelled);
            assert!(owner.published.lock().unwrap().is_empty());
            assert_eq!(state.capture_live.load(Ordering::Relaxed), 0);
            drop(owner);
            assert!(state.live.lock().unwrap().is_empty());
            resources.validate_retired().unwrap();
        }
    }
}

#[test]
fn empty_evenodd_batch_aliases_keep_same_request_records_until_last_drop() {
    for compat_replay in [false, true] {
        let (owner, resources, state) = empty_evenodd_owner(compat_replay);
        let owner = Arc::new(owner);
        let tree = parse(&owner, &resources, "SSA").unwrap();
        let found = flat_paths(&tree);
        assert_eq!(found.len(), 1);
        assert_empty_evenodd_members(found[0], &owner, &resources, "SSA");
        assert_eq!(state.empty_evenodd_growth_admits.load(Ordering::Relaxed), 1);
        let alias = found[0].try_clone_in(tree.processing_context()).unwrap();
        let tree_alias = tree.try_clone_in(tree.processing_context()).unwrap();
        let retained = state.capture_live.load(Ordering::Relaxed);
        assert!(retained > 0);
        drop(tree);
        assert_eq!(state.capture_live.load(Ordering::Relaxed), retained);
        drop(tree_alias);
        assert_eq!(state.capture_live.load(Ordering::Relaxed), retained);
        assert!(alias.glyph_fill_source().unwrap().belongs_to(&resources));
        assert_eq!(alias.glyph_fill_source().unwrap().glyphs().len(), 3);
        drop(alias);
        assert_eq!(state.capture_live.load(Ordering::Relaxed), 0);
        drop(owner);
        assert!(state.live.lock().unwrap().is_empty());
        resources.validate_retired().unwrap();
    }
}
