// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE file in the project root for license information.
// ---------------------------------------------------------------------------------------------
//! Natural font-owner refunds at the real U preload and bidi-run boundaries.

use super::*;
use crate::geometry_test_support::{native, request, Call, State};
use crate::{FallbackSelectionFn, FontSelectionFn, Options, ProcessingContext, Tree};
use markdown_svg_budget::{Error, Limits, ResourceKind, ResourcesRef};
use std::cell::RefCell;
use std::sync::atomic::{AtomicUsize, Ordering};

const TUFFY: &[u8] = include_bytes!("../../../../dependencies/fontdb-0.24.0/tests/fonts/Tuffy.ttf");
const TWELVE_AXES: &[u8] = include_bytes!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../../../tests/baselines/fontations-b373cdbc6043a791fb23b3e0f413649458f5e63a/font-test-data/test_data/ttf/Amstelvar-avar2.A.ttf"
));

struct OutputBoundary {
    event_start: usize,
    release_ordinal: usize,
    live_before: Vec<(u32, usize)>,
}

struct Trace {
    resources: ResourcesRef,
    state: Arc<State>,
    metrics: usize,
    runs: Vec<String>,
    outputs: Vec<OutputBoundary>,
    reject_first_output: bool,
}

std::thread_local! {
    // Other tests and their threads leave these observation hooks disabled.
    static TRACE: RefCell<Option<Trace>> = const { RefCell::new(None) };
}

struct TraceScope;
impl TraceScope {
    fn begin(resources: &ResourcesRef, state: &Arc<State>, reject_first_output: bool) -> Self {
        TRACE.with(|slot| {
            assert!(slot
                .replace(Some(Trace {
                    resources: resources.clone(),
                    state: state.clone(),
                    metrics: 0,
                    runs: Vec::new(),
                    outputs: Vec::new(),
                    reject_first_output,
                }))
                .is_none());
        });
        Self
    }

    fn finish(self) -> Trace {
        TRACE.with(|slot| slot.take().expect("active consumer observation"))
    }
}
impl Drop for TraceScope {
    fn drop(&mut self) {
        TRACE.with(|slot| slot.take());
    }
}

// Called only by the cfg(test) hooks at the actual consumer entries. These
// neither replace the consumer nor synthesize a failure in its request slot.
pub(super) fn metrics_enter() {
    TRACE.with(|slot| {
        if let Some(trace) = slot.borrow_mut().as_mut() {
            trace.metrics += 1;
        }
    });
}

pub(super) fn run_enter(text: &str) {
    TRACE.with(|slot| {
        if let Some(trace) = slot.borrow_mut().as_mut() {
            trace.runs.push(text.to_owned());
        }
    });
}

pub(super) fn before_output_drop() {
    TRACE.with(|slot| {
        let mut slot = slot.borrow_mut();
        let Some(trace) = slot.as_mut() else {
            return;
        };
        assert_eq!(trace.resources.failure(), None);
        let event_start = trace.state.lock().events.len();
        let release_ordinal = trace.state.counts().release + 1;
        trace.outputs.push(OutputBoundary {
            event_start,
            release_ordinal,
            live_before: trace.state.live_tickets(),
        });
        if trace.reject_first_output && trace.outputs.len() == 1 {
            trace.state.arm(Call::Release, 1, native(-8333));
        }
    });
}

fn database(bytes: &[u8]) -> (Arc<Database>, ID) {
    let mut db = Database::new();
    db.load_font_data(bytes.to_vec());
    let id = db.faces().next().expect("complete fixed font fixture").id;
    (Arc::new(db), id)
}

fn assert_failed_refund(
    resources: &ResourcesRef,
    state: &State,
    release_ordinal: usize,
    expected_bytes: usize,
    error: Error,
) -> u32 {
    assert_eq!(resources.failure(), Some(error));
    assert_eq!(resources.statistics().release_failures, 1);
    assert_eq!(resources.statistics().live_leases, 1);
    assert_eq!(resources.statistics().scratch_bytes, expected_bytes as u64);
    assert_eq!(state.kind_bytes(ResourceKind::Scratch), expected_bytes);
    state.assert_account(resources);
    let retained = state.live_tickets();
    assert_eq!(retained.len(), 1, "one canonical failed ticket remains");
    let (slot, bytes) = retained[0];
    assert_eq!(bytes, expected_bytes);
    let inner = state.lock();
    let event_index = inner
        .events
        .iter()
        .position(|event| event.call == Call::Release && event.ordinal == release_ordinal)
        .expect("the real natural Drop reached the armed release");
    assert_eq!(inner.events[event_index].bytes, expected_bytes);
    // This fixture injects only Release errors. Every successful reserve gets
    // the next canonical slot; the spy itself checks nonce/generation on refund.
    let reservation = inner
        .events
        .iter()
        .find(|event| event.call == Call::Reserve && event.ordinal == slot as usize)
        .expect("the retained canonical ticket's actual reserve");
    assert_eq!(reservation.bytes, expected_bytes);
    assert!(inner.events[event_index + 1..]
        .iter()
        .all(|event| event.call == Call::Release));
    drop(inner);
    assert_eq!(resources.record(native(-8399)), error, "first error wins");
    assert_eq!(resources.validate_retired(), Err(error));
    slot
}

// Probe the exact unchanged load_font input, including RF's own normalization
// scratch. In the 12-axis case the final release, rather than the first RF
// temporary release, is the Location spill owned by FontInstance.
fn successful_load_probe(
    bytes: &[u8],
    variations: &[crate::FontVariation],
    expected_last_bytes: usize,
) -> usize {
    let (db, id) = database(bytes);
    let (resources, state) = request(Limits::default());
    let processing = ProcessingContext::with_resources(resources.clone());
    let observation = TraceScope::begin(&resources, &state, false);
    assert!(db.load_font(id, variations, None, &processing).is_some());
    let trace = observation.finish();
    assert_eq!(trace.metrics, 1);
    assert!(trace.runs.is_empty());
    let inner = state.lock();
    let release = inner
        .events
        .iter()
        .rev()
        .find(|event| event.call == Call::Release)
        .expect("the chosen owner really has paid storage");
    assert_eq!(release.bytes, expected_last_bytes);
    let ordinal = release.ordinal;
    drop(inner);
    drop(trace);
    drop(processing);
    assert_eq!(state.live_len(), 0);
    resources.validate_retired().unwrap();
    ordinal
}

fn failed_load_probe(
    bytes: &[u8],
    variations: &[crate::FontVariation],
    release_ordinal: usize,
    expected_bytes: usize,
    expected_metrics: usize,
    error: Error,
) {
    let (db, id) = database(bytes);
    let (resources, state) = request(Limits::default());
    let processing = ProcessingContext::with_resources(resources.clone());
    let observation = TraceScope::begin(&resources, &state, false);
    assert_eq!(resources.failure(), None);
    state.arm(Call::Release, release_ordinal, error);
    assert!(db.load_font(id, variations, None, &processing).is_none());
    let trace = observation.finish();
    assert_eq!(trace.metrics, expected_metrics);
    assert!(trace.runs.is_empty());
    drop(trace);
    drop(processing);
    let slot = assert_failed_refund(&resources, &state, release_ordinal, expected_bytes, error);
    println!(
        "U_LOAD_FONT_NATURAL_REFUND release={release_ordinal} ticket_slot={slot} bytes={expected_bytes} metrics={expected_metrics} result_none=true first_error_preserved=true retired=false"
    );
}

fn two_real_spans(
    bytes: &[u8],
    settings: bool,
    relative_release: usize,
    expected_bytes: usize,
    metrics_before_failure: usize,
    error: Error,
    reject: bool,
) {
    let (db, id) = database(bytes);
    let (resources, state) = request(Limits::default());
    let primary = Arc::new(AtomicUsize::new(0));
    let fallback = Arc::new(AtomicUsize::new(0));
    let release_base = Arc::new(AtomicUsize::new(0));
    let reserve_base = Arc::new(AtomicUsize::new(0));
    let options = Options {
        fontdb: db,
        processing: Arc::new(ProcessingContext::with_resources(resources.clone())),
        font_resolver: FontResolver {
            select_font: FontSelectionFn::Owned(Box::new({
                let primary = primary.clone();
                let state = state.clone();
                let release_base = release_base.clone();
                let reserve_base = reserve_base.clone();
                move |font, _| {
                    // convert_font always supplies CSS's default wght=400;
                    // an explicit setting replaces that inferred value.
                    assert_eq!(font.variations.len(), 1);
                    assert_eq!(font.variations[0].tag, *b"wght");
                    assert_eq!(
                        font.variations[0].value,
                        if settings { 700.0 } else { 400.0 }
                    );
                    if primary.fetch_add(1, Ordering::Relaxed) == 0 {
                        release_base.store(state.counts().release, Ordering::Relaxed);
                        reserve_base.store(state.counts().reserve, Ordering::Relaxed);
                        if reject {
                            state.arm(Call::Release, relative_release, error);
                        }
                    }
                    Some(id)
                }
            })),
            select_fallback: FallbackSelectionFn::Owned(Box::new({
                let fallback = fallback.clone();
                move |_, _, _| {
                    fallback.fetch_add(1, Ordering::Relaxed);
                    None
                }
            })),
        },
        ..Options::default()
    };
    let variation = if settings {
        " font-variation-settings='&quot;wght&quot; 700'"
    } else {
        ""
    };
    let source = format!(
        "<svg xmlns='http://www.w3.org/2000/svg' width='160' height='60'><text x='4' y='30' font-optical-sizing='none'{variation}><tspan font-family='First'>A</tspan><tspan font-family='Second'>A</tspan></text></svg>"
    );
    let observation = TraceScope::begin(&resources, &state, false);
    let result = Tree::from_data_controlled(source.as_bytes(), &options);
    let trace = observation.finish();
    if reject {
        assert!(result.is_err());
        assert_eq!(primary.load(Ordering::Relaxed), 1);
        assert_eq!(trace.metrics, metrics_before_failure);
        assert!(trace.runs.is_empty());
    } else {
        assert!(result.is_ok());
        assert_eq!(primary.load(Ordering::Relaxed), 2);
        assert_eq!(trace.metrics, 2);
        assert!(!trace.runs.is_empty(), "the positive Tree reached real HB");
    }
    assert_eq!(fallback.load(Ordering::Relaxed), 0);
    drop(result);
    drop(trace);
    drop(options);
    if reject {
        let ordinal = release_base.load(Ordering::Relaxed) + relative_release;
        let slot = assert_failed_refund(&resources, &state, ordinal, expected_bytes, error);
        assert!(slot as usize > reserve_base.load(Ordering::Relaxed));
        println!(
            "U_TREE_TWO_SPANS_NATURAL_REFUND settings={settings} release={ordinal} ticket_slot={slot} bytes={expected_bytes} select_font=1 metrics={metrics_before_failure} hb_runs=0 fallback=0 retired=false"
        );
    } else {
        assert_eq!(state.live_len(), 0);
        resources.validate_retired().unwrap();
        println!(
            "U_TREE_TWO_SPANS_POSITIVE settings={settings} select_font=2 metrics=2 real_hb=true fallback=0 retired=true"
        );
    }
}

#[test]
fn settings_natural_refund_stops_metrics_and_the_next_real_preload_span() {
    let variations = [crate::FontVariation::new(*b"wght", 700.0)];
    let (resources, state) = request(Limits::default());
    {
        let instance =
            super::super::font_instance::FontInstance::new(TUFFY, 0, &variations, Some(&resources))
                .unwrap()
                .unwrap();
        assert_eq!(instance.axes().len(), 0);
        assert!(instance.location().coords().is_empty());
    }
    let releases: Vec<_> = state
        .lock()
        .events
        .iter()
        .filter(|event| event.call == Call::Release)
        .map(|event| event.bytes)
        .collect();
    assert_eq!(releases, [8], "the actual one-setting Vec has 8 paid bytes");
    resources.validate_retired().unwrap();
    let release = successful_load_probe(TUFFY, &variations, 8);
    assert_eq!(release, 1);
    let error = native(-8331);
    failed_load_probe(TUFFY, &variations, release, 8, 0, error);
    for reject in [false, true] {
        two_real_spans(TUFFY, true, release, 8, 0, error, reject);
    }
}

#[test]
fn twelve_axis_location_final_refund_stops_load_publication_and_next_real_span() {
    let (resources, state) = request(Limits::default());
    let instance =
        super::super::font_instance::FontInstance::new(TWELVE_AXES, 0, &[], Some(&resources))
            .unwrap()
            .unwrap();
    assert_eq!(instance.axes().len(), 12, "fixed official 12-axis fixture");
    assert_eq!(instance.location().coords().len(), 12);
    assert_eq!(state.live_tickets().len(), 1);
    assert_eq!(state.kind_bytes(ResourceKind::Scratch), 24);
    drop(instance);
    assert_eq!(state.live_len(), 0);
    resources.validate_retired().unwrap();
    let release = successful_load_probe(TWELVE_AXES, &[], 24);
    let error = native(-8332);
    failed_load_probe(TWELVE_AXES, &[], release, 24, 1, error);
    let tree_variations = [crate::FontVariation::new(*b"wght", 400.0)];
    let tree_release = successful_load_probe(TWELVE_AXES, &tree_variations, 24);
    for reject in [false, true] {
        two_real_spans(TWELVE_AXES, false, tree_release, 24, 1, error, reject);
    }
}

#[test]
fn first_bidi_run_output_natural_refund_stops_the_second_run_and_fallback() {
    const TEXT: &str = "abc אב";
    let (db, id) = database(TUFFY);
    let font = db
        .load_font(id, &[], None, &ProcessingContext::default())
        .unwrap();
    // Establish the unchanged real input has exactly two visual runs.
    let (levels, runs) = bidi_visual_runs_for_shape(TEXT, None).unwrap();
    assert_eq!(runs.len(), 2);
    let expected_runs: Vec<_> = runs
        .iter()
        .map(|run| TEXT[run.clone()].to_owned())
        .collect();
    drop(runs);
    drop(levels);
    for reject in [false, true] {
        let (resources, state) = request(Limits::default());
        let processing = ProcessingContext::with_resources(resources.clone());
        let fallback = AtomicUsize::new(0);
        let resolver = FontResolver {
            select_font: FontSelectionFn::Owned(Box::new(move |_, _| Some(id))),
            select_fallback: FallbackSelectionFn::Borrowed(&|_, _, _| {
                fallback.fetch_add(1, Ordering::Relaxed);
                None
            }),
        };
        let mut fontdb = db.clone();
        let observation = TraceScope::begin(&resources, &state, reject);
        let result = shape_text(
            TEXT,
            font,
            false,
            true,
            &[],
            16.0,
            crate::FontOpticalSizing::None,
            &resolver,
            &mut fontdb,
            &processing,
        );
        let trace = observation.finish();
        assert_eq!(trace.metrics, 0);
        if reject {
            assert_eq!(result.err(), Some(native(-8333)));
            assert_eq!(trace.runs, expected_runs[..1]);
            assert_eq!(trace.outputs.len(), 1);
            assert_eq!(fallback.load(Ordering::Relaxed), 0);
            let boundary = &trace.outputs[0];
            let inner = state.lock();
            let event = inner.events[boundary.event_start];
            assert_eq!(
                event.call,
                Call::Release,
                "the next action is real Output Drop"
            );
            assert_eq!(event.ordinal, boundary.release_ordinal);
            let expected_bytes = event.bytes;
            assert!(expected_bytes > 0);
            let ordinal = boundary.release_ordinal;
            drop(inner);
            let retained = state.live_tickets();
            assert_eq!(retained.len(), 1);
            assert!(boundary.live_before.contains(&retained[0]));
            drop(trace);
            drop(resolver);
            drop(processing);
            let slot =
                assert_failed_refund(&resources, &state, ordinal, expected_bytes, native(-8333));
            println!(
                "U_BIDI_OUTPUT_NATURAL_REFUND visual_runs=2 actual_runs=1 output_drops=1 fallback=0 release={ordinal} ticket_slot={slot} bytes={expected_bytes} result_error=true retired=false"
            );
        } else {
            let glyphs = result.unwrap();
            assert!(!glyphs.is_empty());
            assert!(glyphs.iter().any(Glyph::is_missing));
            assert_eq!(trace.runs, expected_runs);
            assert_eq!(trace.outputs.len(), 2);
            assert_eq!(fallback.load(Ordering::Relaxed), 1);
            drop(glyphs);
            drop(trace);
            drop(resolver);
            drop(processing);
            assert_eq!(state.live_len(), 0);
            resources.validate_retired().unwrap();
            println!(
                "U_BIDI_OUTPUT_POSITIVE visual_runs=2 actual_runs=2 output_drops=2 fallback=1 retired=true"
            );
        }
    }
}
