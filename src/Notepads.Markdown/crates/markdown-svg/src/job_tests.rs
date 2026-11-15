// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE file in the project root for license information.
// ---------------------------------------------------------------------------------------------
use super::*;
#[path = "bidi_phase_cases.rs"]
mod bidi_phase_cases;
#[path = "bidi_release_cases.rs"]
mod bidi_release_cases;
#[path = "font_shape_tail_cases.rs"]
mod font_shape_tail_cases;
#[path = "search_phase_cases.rs"]
mod search_phase_cases;
use crate::memory::{AdmittedSlice, Host};
use markdown_svg_budget::{ResourceKind, Ticket};
use std::{
    collections::BTreeMap,
    sync::{
        Mutex,
        atomic::{AtomicU32, AtomicU64},
    },
};

struct ObservingAllocator;
#[global_allocator]
static ALLOCATOR: ObservingAllocator = ObservingAllocator;
#[derive(Clone, Copy, Debug, Default)]
struct Event {
    tag: u8,
    identity: usize,
    bytes: usize,
}
// Test observation storage only. These slots do not change request budgets.
const OBSERVER_CAPACITY: usize = 65_536;
struct Events {
    enabled: bool,
    length: usize,
    total: usize,
    overflow: usize,
    entries: [Event; OBSERVER_CAPACITY],
}
thread_local! {
    static EVENTS: std::cell::RefCell<Events> = const { std::cell::RefCell::new(Events {
        enabled: false, length: 0, total: 0, overflow: 0,
        entries: [Event { tag: 0, identity: 0, bytes: 0 }; OBSERVER_CAPACITY] }) };
}
fn event(tag: u8, identity: usize, bytes: usize) {
    let _ = EVENTS.try_with(|slot| {
        if let Ok(mut events) = slot.try_borrow_mut() {
            if !events.enabled {
                return;
            }
            events.total += 1;
            if events.length < events.entries.len() {
                let at = events.length;
                events.entries[at] = Event {
                    tag,
                    identity,
                    bytes,
                };
                events.length += 1;
            } else {
                events.overflow += 1;
            }
        }
    });
}
unsafe impl std::alloc::GlobalAlloc for ObservingAllocator {
    unsafe fn alloc(&self, layout: std::alloc::Layout) -> *mut u8 {
        // SAFETY: forward the exact allocator contract.
        let pointer = unsafe { std::alloc::GlobalAlloc::alloc(&std::alloc::System, layout) };
        event(1, pointer as usize, layout.size());
        pointer
    }
    unsafe fn alloc_zeroed(&self, layout: std::alloc::Layout) -> *mut u8 {
        let pointer = unsafe { std::alloc::GlobalAlloc::alloc_zeroed(&std::alloc::System, layout) };
        event(1, pointer as usize, layout.size());
        pointer
    }
    unsafe fn dealloc(&self, pointer: *mut u8, layout: std::alloc::Layout) {
        unsafe {
            std::alloc::GlobalAlloc::dealloc(&std::alloc::System, pointer, layout);
        }
        // This observation is after the real System free.
        event(2, pointer as usize, layout.size());
    }
    unsafe fn realloc(
        &self,
        pointer: *mut u8,
        layout: std::alloc::Layout,
        bytes: usize,
    ) -> *mut u8 {
        let fresh = unsafe {
            std::alloc::GlobalAlloc::realloc(&std::alloc::System, pointer, layout, bytes)
        };
        if !fresh.is_null() {
            event(2, pointer as usize, layout.size());
            event(1, fresh as usize, bytes);
        }
        fresh
    }
}
fn start_events() {
    EVENTS.with_borrow_mut(|events| {
        events.length = 0;
        events.total = 0;
        events.overflow = 0;
        events.enabled = true;
    });
}
fn stop_events() -> Vec<Event> {
    let (result, total, overflow) = EVENTS.with_borrow_mut(|events| {
        events.enabled = false;
        assert_eq!(events.total, events.length + events.overflow);
        assert!(
            events.overflow == 0,
            "allocator observer capacity exhausted: total={} recorded={} overflow={} capacity={}",
            events.total,
            events.length,
            events.overflow,
            events.entries.len()
        );
        (
            events.entries[..events.length].to_vec(),
            events.total,
            events.overflow,
        )
    });
    println!(
        "ALLOCATOR_OBSERVER test={:?} capacity={OBSERVER_CAPACITY} total={total} recorded={} overflow={overflow} old_4096_saturated_at={}",
        std::thread::current().name(),
        result.len(),
        total.min(4096)
    );
    result
}

#[derive(Default)]
struct TestHost {
    coverage: Option<Arc<OrdinaryShapeCoverage>>,
    coverage_queries: AtomicU32,
    next: AtomicU32,
    live: Mutex<BTreeMap<u32, (usize, bool)>>,
    committed: AtomicU64,
    released: AtomicU64,
    release_hr: AtomicU32,
    commit_hr: AtomicU32,
    checkpoint_hr: AtomicU32,
    panic_release: AtomicBool,
}
impl Host for TestHost {
    fn coverage_provider(&self) -> Option<&dyn resvg::CoverageProvider> {
        self.coverage_queries.fetch_add(1, Ordering::Relaxed);
        self.coverage
            .as_deref()
            .map(|value| value as &dyn resvg::CoverageProvider)
    }
    fn reserve(&self, _: ResourceKind, bytes: usize) -> Result<Ticket, Error> {
        let slot = self.next.fetch_add(1, Ordering::Relaxed);
        self.live.lock().unwrap().insert(slot, (bytes, false));
        event(3, slot as usize, bytes);
        Ticket::new(1, slot, 1)
    }
    fn allocation_committed(&self, ticket: &Ticket, bytes: usize) -> Result<(), Error> {
        event(4, ticket.slot() as usize, bytes);
        let hr = self.commit_hr.load(Ordering::Relaxed);
        if hr != 0 {
            return Err(Error::native(hr as i32));
        }
        let mut live = self.live.lock().unwrap();
        let item = live.get_mut(&ticket.slot()).unwrap();
        assert!(bytes <= item.0);
        assert!(!item.1, "one successful allocation admission");
        item.1 = true;
        self.committed.fetch_add(1, Ordering::Relaxed);
        Ok(())
    }
    fn release_after_free(&self, ticket: Ticket) -> Result<(), Error> {
        event(5, ticket.slot() as usize, 0);
        assert!(
            !self.panic_release.swap(false, Ordering::Relaxed),
            "injected release callback panic"
        );
        let hr = self.release_hr.load(Ordering::Relaxed);
        if hr != 0 {
            return Err(Error::native(hr as i32));
        }
        assert_eq!(ticket.account_nonce(), 1);
        assert_eq!(ticket.generation(), 1);
        assert!(self.live.lock().unwrap().remove(&ticket.slot()).is_some());
        self.released.fetch_add(1, Ordering::Relaxed);
        Ok(())
    }
    fn checkpoint(&self, _: u64) -> Result<(), Error> {
        let hr = self.checkpoint_hr.load(Ordering::Relaxed);
        if hr == 0 {
            Ok(())
        } else {
            Err(Error::native(hr as i32))
        }
    }
}
unsafe extern "C" fn no_plan(_: u64, _: *const abi::Draft, _: *mut abi::Plan) -> i32 {
    -101
}
unsafe extern "C" fn no_font(_: u64, _: u32, _: *mut u8, _: u64, _: *mut u64) -> i32 {
    -102
}
unsafe extern "C" fn no_metrics(
    _: u64,
    _: u32,
    _: u32,
    _: *mut abi::GlyphMetrics,
    _: u32,
    _: *mut u32,
) -> i32 {
    -103
}
unsafe extern "C" fn no_geometry(
    _: u64,
    _: *const abi::GlyphKey,
    _: u32,
    _: *mut abi::GeometryReceipt,
) -> i32 {
    -104
}
unsafe extern "C" fn no_fill_geometry(
    _: u64,
    _: u64,
    _: *mut abi::GlyphPath,
    _: u32,
    _: *mut abi::Command,
    _: u32,
    _: *mut u32,
    _: *mut u32,
) -> i32 {
    -105
}
unsafe extern "C" fn no_retire(_: u64, _: u64) -> i32 {
    -106
}
fn callbacks() -> abi::Callbacks {
    abi::Callbacks {
        size: size_of::<abi::Callbacks>() as u32,
        version: abi::FONT_CALLBACKS_VERSION,
        session: 1,
        plan: Some(no_plan),
        fill_font: Some(no_font),
        fill_metrics: Some(no_metrics),
        prepare_geometry: Some(no_geometry),
        fill_geometry: Some(no_fill_geometry),
        retire_geometry: Some(no_retire),
    }
}
fn identity() -> Identity {
    Identity {
        job_nonce: 11,
        source_generation: 12,
        provider_generation: 13,
        font_generation: 14,
        target_generation: 15,
    }
}
fn job(host: &Arc<TestHost>) -> OwnedJob {
    OwnedJob::new(identity(), callbacks(), host.clone(), Limits::default()).unwrap()
}
fn target(width: f64) -> Target {
    Target {
        display_width_dip: width,
        effective_physical_scale: 1.0,
        generation: 15,
    }
}
fn finish(job: &mut OwnedJob, host: &TestHost) -> Retirement {
    let receipt = job.finish_on_worker().unwrap();
    assert_eq!(receipt.cleanup_status, CleanupStatus::Succeeded);
    assert_eq!(receipt.remaining_resource_owners, 0);
    assert_eq!(receipt.failed_release_count, 0);
    assert!(host.live.lock().unwrap().is_empty());
    receipt
}
const RECT: &[u8] = b"<svg xmlns='http://www.w3.org/2000/svg' width='3' height='2'><rect width='3' height='2' fill='#ff8040'/></svg>";

struct OrdinaryShapeCoverage;
impl resvg::CoverageProvider for OrdinaryShapeCoverage {
    fn epoch(&self) -> u64 {
        panic!("ordinary shapes must not query a text coverage capability")
    }
    fn cover_batch(
        &self,
        _: &[resvg::CoverageRequest<'_>],
        _: &mut [u8],
        _: &ResourcesRef,
    ) -> Result<resvg::CoverageBatchResult, Error> {
        panic!("ordinary shapes must not allocate or request Native text coverage")
    }
}

#[test]
fn optional_coverage_service_preserves_original_large_ordinary_image() {
    let host = Arc::new(TestHost {
        coverage: Some(Arc::new(OrdinaryShapeCoverage)),
        ..Default::default()
    });
    let mut job = job(&host);
    job.prepare(RECT).unwrap();
    // Keep the exact formerly regressed D=4000,p=1 input, with a provider present.
    let plan = job.set_target(target(4000.0)).unwrap();
    assert_eq!((plan.width, plan.height), (4032, 2688));
    let mut guarded = vec![0x5b; plan.capacity + 18];
    job.run_into(identity(), &mut guarded[9..9 + plan.capacity], plan.stride)
        .unwrap();
    assert_eq!(host.coverage_queries.load(Ordering::Relaxed), 1);
    assert!(guarded[..9].iter().all(|byte| *byte == 0x5b));
    assert!(
        guarded[9 + plan.capacity..]
            .iter()
            .all(|byte| *byte == 0x5b)
    );
    // Exact integer-aligned, opaque solid rectangle: independent BGRA oracle.
    assert!(
        guarded[9..9 + plan.capacity]
            .chunks_exact(4)
            .all(|pixel| pixel == [0x40, 0x80, 0xff, 0xff])
    );
    finish(&mut job, &host);
}

#[test]
fn caller_buffer_renders_single_scale_and_matches_same_engine_reference() {
    let host = Arc::new(TestHost::default());
    let mut job = job(&host);
    assert_eq!(job.prepare(RECT).unwrap().width_dip, 3.0);
    let plan = job.set_target(target(4000.0)).unwrap();
    assert_eq!(plan.width, 4032);
    let mut reference = tiny_skia::Pixmap::new(plan.width, plan.height).unwrap();
    let scale = f32::from_bits(plan.scale_bits);
    resvg::render_in_request(
        job.work.tree.as_ref().unwrap(),
        tiny_skia::Transform::from_scale(scale, scale),
        &mut reference.as_mut(),
    )
    .unwrap();
    let mut guarded = vec![0xA7; plan.capacity + 128];
    let rendered = job
        .run_into(
            identity(),
            &mut guarded[64..64 + plan.capacity],
            plan.stride,
        )
        .unwrap();
    assert_eq!(rendered.pixel_bytes, plan.capacity);
    assert!(
        guarded[..64]
            .iter()
            .chain(&guarded[64 + plan.capacity..])
            .all(|&v| v == 0xA7)
    );
    for (rgba, bgra) in reference
        .data()
        .chunks_exact(4)
        .zip(guarded[64..64 + plan.capacity].chunks_exact(4))
    {
        assert_eq!([rgba[2], rgba[1], rgba[0], rgba[3]], bgra);
    }
    assert!(
        guarded[64..64 + plan.capacity]
            .chunks_exact(4)
            .any(|p| p[3] != 0)
    );
    let receipt = finish(&mut job, &host);
    assert_eq!(receipt.terminal_state, JobState::Rendered);
    assert_eq!(receipt.first_fault, None);
    assert!(job.can_destroy_shell());
}

#[test]
fn source_can_be_poisoned_after_prepare_and_transparent_output_is_success() {
    for (source, nonempty) in [
        (RECT, true),
        (
            b"<svg xmlns='http://www.w3.org/2000/svg' width='7' height='3'/>".as_slice(),
            false,
        ),
    ] {
        let host = Arc::new(TestHost::default());
        let mut job = job(&host);
        let mut source = source.to_vec();
        job.prepare(&source).unwrap();
        source.fill(0xDA);
        drop(source);
        let plan = job.set_target(target(7.0)).unwrap();
        let mut pixels = vec![0xFF; plan.capacity];
        job.run_into(identity(), &mut pixels, plan.stride).unwrap();
        assert_eq!(pixels.chunks_exact(4).any(|p| p[3] != 0), nonempty);
        finish(&mut job, &host);
    }
}

#[test]
fn frozen_f32_rule_rejects_scale_overflow_and_avoids_ceil4097() {
    let make = |w: f64, h: f64| NaturalSize {
        width_dip: w,
        height_dip: h,
        viewport_width_bits: (w as f32).to_bits(),
        viewport_height_bits: (h as f32).to_bits(),
    };
    assert_eq!(
        freeze_plan(identity(), make(1e-40, 1e-40), target(64.0)),
        Err(JobError::GeometryUnrepresentable)
    );
    for (w, h) in [(3.0, 2.0), (7.0, 3.0), (100.5, 50.25), (4096.0, 4095.0)] {
        let plan = freeze_plan(identity(), make(w, h), target(4096.0)).unwrap();
        assert!(plan.width <= 4096 && plan.height <= 4096);
        assert!(plan.extent_width <= 4096.0 && plan.extent_height <= 4096.0);
        let scale = f32::from_bits(plan.scale_bits);
        assert!(w * f64::from(scale) <= 4096.0 && h * f64::from(scale) <= 4096.0);
        assert_eq!(plan.capacity, plan.stride as usize * plan.height as usize);
    }
    let p = freeze_plan(identity(), make(3.0, 2.0), target(4096.0)).unwrap();
    assert!(f64::from(f32::from_bits(p.scale_bits)) < 4096.0 / 3.0);
    assert_eq!(p.width, 4096);
}

#[test]
fn repeated_calls_do_not_reset_or_reprepare_the_job() {
    let host = Arc::new(TestHost::default());
    let mut job = job(&host);
    job.prepare(RECT).unwrap();
    assert_eq!(job.prepare(RECT), Err(JobError::InvalidState));
    let plan = job.set_target(target(64.0)).unwrap();
    assert_eq!(job.set_target(target(128.0)), Err(JobError::InvalidState));
    let mut pixels = vec![0; plan.capacity];
    job.run_into(identity(), &mut pixels, plan.stride).unwrap();
    assert_eq!(
        job.run_into(identity(), &mut pixels, plan.stride),
        Err(JobError::InvalidState)
    );
    finish(&mut job, &host);
    assert_eq!(job.finish_on_worker(), Err(JobError::InvalidState));
}

#[test]
fn stale_identity_and_wrong_capacity_fail_before_writing() {
    for variant in 0..3 {
        let host = Arc::new(TestHost::default());
        let mut job = job(&host);
        job.prepare(RECT).unwrap();
        let plan = job.set_target(target(64.0)).unwrap();
        let mut id = identity();
        if variant == 0 {
            id.source_generation += 1;
        }
        let mut pixels = vec![0x7A; plan.capacity + usize::from(variant == 1)];
        let stride = plan.stride + u32::from(variant == 2);
        assert_eq!(
            job.run_into(id, &mut pixels, stride),
            Err(JobError::InvalidArgument)
        );
        assert!(pixels.iter().all(|&v| v == 0x7A));
        assert_eq!(finish(&mut job, &host).terminal_state, JobState::Failed);
    }
}

#[test]
fn cancellation_never_blocks_retirement() {
    let host = Arc::new(TestHost::default());
    let mut job = job(&host);
    job.prepare(RECT).unwrap();
    let plan = job.set_target(target(64.0)).unwrap();
    let cancel = job.cancellation();
    std::thread::spawn(move || cancel.cancel()).join().unwrap();
    let mut pixels = vec![0x7A; plan.capacity];
    assert!(matches!(
        job.run_into(identity(), &mut pixels, plan.stride),
        Err(JobError::Failure(Error {
            kind: FailureKind::Cancelled,
            ..
        }))
    ));
    let receipt = finish(&mut job, &host);
    assert_eq!(receipt.first_fault.unwrap().kind, FailureKind::Cancelled);
}

#[test]
fn prepare_failure_preserves_original_native_font_hresult_through_cleanup() {
    let host = Arc::new(TestHost::default());
    let mut job = job(&host);
    let result = job.prepare(b"<svg xmlns='http://www.w3.org/2000/svg'><text>x</text></svg>");
    assert_eq!(result, Err(JobError::Failure(Error::native(-101))));
    let receipt = finish(&mut job, &host);
    assert_eq!(receipt.first_fault, Some(Error::native(-101)));
    assert_eq!(receipt.terminal_state, JobState::Failed);
}

#[test]
fn successful_render_with_failed_release_has_failed_retirement_and_no_destroy() {
    let host = Arc::new(TestHost::default());
    let mut job = job(&host);
    job.prepare(RECT).unwrap();
    let plan = job.set_target(target(64.0)).unwrap();
    let mut pixels = vec![0; plan.capacity];
    job.run_into(identity(), &mut pixels, plan.stride).unwrap();
    host.release_hr.store((-777i32) as u32, Ordering::Relaxed);
    let receipt = job.finish_on_worker().unwrap();
    assert_eq!(receipt.terminal_state, JobState::Rendered);
    assert_eq!(receipt.cleanup_status, CleanupStatus::Failed);
    assert!(receipt.failed_release_count > 0);
    assert_eq!(job.state(), JobState::RetirementFailed);
    assert!(!job.can_destroy_shell());
    assert_eq!(job.finish_on_worker(), Err(JobError::InvalidState));
    assert!(!host.live.lock().unwrap().is_empty());
}

#[test]
fn new_and_target_failure_also_require_worker_finish() {
    let host = Arc::new(TestHost::default());
    let mut first = job(&host);
    assert_eq!(finish(&mut first, &host).terminal_state, JobState::New);
    let mut second = job(&host);
    second.prepare(RECT).unwrap();
    assert_eq!(
        second.set_target(target(f64::NAN)),
        Err(JobError::GeometryUnrepresentable)
    );
    assert_eq!(finish(&mut second, &host).terminal_state, JobState::Failed);
}

#[test]
fn allocation_commit_failure_retires_real_owned_buffer_before_pending_release() {
    let host = Arc::new(TestHost::default());
    let mut job = job(&host);
    let initial_live = host.live.lock().unwrap().len();
    let initial_committed = host.committed.load(Ordering::Relaxed);
    host.commit_hr.store((-778i32) as u32, Ordering::Relaxed);
    start_events();
    let result = AdmittedSlice::<u8>::zeroed(&job.work.context, abi::FONT, 4096);
    let events = stop_events();
    assert!(matches!(result, Err(abi::Failure::Native(-778))));
    assert_eq!(host.live.lock().unwrap().len(), initial_live);
    assert_eq!(host.committed.load(Ordering::Relaxed), initial_committed);
    assert_eq!(host.released.load(Ordering::Relaxed), 1);
    let allocation = events
        .iter()
        .position(|e| e.tag == 1 && e.bytes == 4096)
        .unwrap();
    let pointer = events[allocation].identity;
    let reserve = events
        .iter()
        .position(|e| e.tag == 3 && e.bytes == 4096)
        .unwrap();
    let token = events[reserve].identity;
    let admit = events
        .iter()
        .position(|e| e.tag == 4 && e.identity == token)
        .unwrap();
    let free = events
        .iter()
        .position(|e| e.tag == 2 && e.identity == pointer)
        .unwrap();
    let release = events
        .iter()
        .position(|e| e.tag == 5 && e.identity == token)
        .unwrap();
    assert!(
        reserve < allocation && allocation < admit && admit < free && free < release,
        "{events:?}"
    );
    finish(&mut job, &host);
}

#[test]
fn filter_and_mask_use_the_same_request_and_caller_buffer() {
    let source=b"<svg xmlns='http://www.w3.org/2000/svg' width='7' height='3'><defs><filter id='f'><feGaussianBlur stdDeviation='.3'/></filter><mask id='m'><rect width='7' height='3' fill='white'/></mask></defs><rect x='1' y='1' width='5' height='1' fill='red' filter='url(#f)' mask='url(#m)'/></svg>";
    let host = Arc::new(TestHost::default());
    let mut job = job(&host);
    job.prepare(source).unwrap();
    let plan = job.set_target(target(64.0)).unwrap();
    let mut pixels = vec![0; plan.capacity];
    job.run_into(identity(), &mut pixels, plan.stride).unwrap();
    assert!(pixels.chunks_exact(4).any(|p| p[3] != 0));
    finish(&mut job, &host);
}

#[test]
fn actual_embedded_png_nested_svg_and_feimage_remain_nonempty_on_the_same_resources() {
    let png = "data:image/png;base64,iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR4nGP4z8DwHwAFAAH/iZk9HQAAAABJRU5ErkJggg==";
    let nested = "data:image/svg+xml;base64,PHN2ZyB4bWxucz0naHR0cDovL3d3dy53My5vcmcvMjAwMC9zdmcnIHdpZHRoPSczJyBoZWlnaHQ9JzInPjxyZWN0IHdpZHRoPSczJyBoZWlnaHQ9JzInIGZpbGw9J3JlZCcvPjwvc3ZnPg==";
    for body in [
        format!("<image href='{png}' width='3' height='2'/>"),
        format!("<image href='{nested}' width='3' height='2'/>"),
        format!(
            "<defs><filter id='f' x='0' y='0' width='3' height='2' filterUnits='userSpaceOnUse'><feImage href='{png}' width='3' height='2'/></filter></defs><rect width='3' height='2' filter='url(#f)'/>"
        ),
    ] {
        let source =
            format!("<svg xmlns='http://www.w3.org/2000/svg' width='3' height='2'>{body}</svg>");
        let host = Arc::new(TestHost::default());
        let mut job = job(&host);
        job.prepare(source.as_bytes()).unwrap();
        let plan = job.set_target(target(64.0)).unwrap();
        let mut pixels = vec![0; plan.capacity];
        job.run_into(identity(), &mut pixels, plan.stride).unwrap();
        assert!(
            pixels.chunks_exact(4).any(|p| p[2] != 0 && p[3] != 0),
            "{body}"
        );
        let stats = job.stats().unwrap();
        assert!(stats.resources.unwrap().children > 0);
        finish(&mut job, &host);
    }
}

struct FontFixture {
    bytes: Vec<u8>,
    files: [abi::File; 1],
    faces: [abi::Face; 1],
    runs: Vec<abi::LogicalRun>,
    strings: Vec<u8>,
    generation: u64,
    fills: u64,
}
thread_local! { static FONT: std::cell::RefCell<Option<FontFixture>> = const { std::cell::RefCell::new(None) }; }
unsafe extern "C" fn real_plan(_: u64, draft: *const abi::Draft, out: *mut abi::Plan) -> i32 {
    FONT.with_borrow_mut(|slot| {
        let fixture = slot.as_mut().unwrap();
        // SAFETY: the engine passes this initialized owned request synchronously.
        let draft = unsafe { &*draft };
        let spans = unsafe {
            core::slice::from_raw_parts(draft.logical_spans, draft.logical_span_count as usize)
        };
        fixture.runs.clear();
        for span in spans {
            fixture.runs.push(abi::LogicalRun {
                utf8_start: span.utf8_start,
                utf8_length: span.utf8_length,
                face_ticket: 1,
                script_be: span.script_be,
                language_start: span.language_start,
                language_length: span.language_length,
                bidi_level: span.bidi_level,
                orientation: span.orientation,
                fallback_scale: 1.0,
            });
        }
        fixture.strings =
            unsafe { core::slice::from_raw_parts(draft.strings, draft.strings_length as usize) }
                .to_vec();
        fixture.generation += 1;
        let plan = abi::Plan {
            size: size_of::<abi::Plan>() as u32,
            version: abi::ABI_VERSION,
            generation: fixture.generation,
            files: fixture.files.as_ptr(),
            file_count: 1,
            faces: fixture.faces.as_ptr(),
            face_count: 1,
            axes: core::ptr::null(),
            axis_count: 0,
            runs: fixture.runs.as_ptr(),
            run_count: fixture.runs.len() as u32,
            strings: fixture.strings.as_ptr(),
            strings_length: fixture.strings.len() as u32,
        };
        unsafe {
            out.write(plan);
        }
        0
    })
}
unsafe extern "C" fn real_fill(
    _: u64,
    ticket: u32,
    out: *mut u8,
    capacity: u64,
    written: *mut u64,
) -> i32 {
    FONT.with_borrow_mut(|slot| {
        let fixture = slot.as_mut().unwrap();
        assert_eq!(ticket, 1);
        assert_eq!(capacity as usize, fixture.bytes.len());
        // SAFETY: the engine supplies its exact exclusively owned FontBytes.
        unsafe {
            core::ptr::copy_nonoverlapping(fixture.bytes.as_ptr(), out, fixture.bytes.len());
            written.write(capacity);
        }
        fixture.fills += 1;
        0
    })
}
fn install_font() -> abi::Callbacks {
    use read_fonts::TableProvider;
    let bytes = std::fs::read(
        std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../crates/markdown-math/assets/fonts/KaTeX_Main-Regular.ttf"),
    )
    .unwrap();
    let font = harfrust::FontRef::from_index(&bytes, 0).unwrap();
    let upem = u32::from(font.head().unwrap().units_per_em());
    let count = u32::from(font.maxp().unwrap().num_glyphs());
    let files = [abi::File {
        ticket: 1,
        collection_faces: 1,
        byte_length: bytes.len() as u64,
        sha256: core::array::from_fn(|i| {
            u8::from_str_radix(
                &"D0332F52868370FD83AE7FA46470F90C8F2EAB2FCF12BC4F88080B340C95A830"
                    [i * 2..i * 2 + 2],
                16,
            )
            .unwrap()
        }),
    }];
    let faces = [abi::Face {
        ticket: 1,
        file_ticket: 1,
        face_index: 0,
        simulations: 0,
        units_per_em: upem,
        glyph_count: count,
        axis_start: 0,
        axis_count: 0,
        metrics: abi::GlobalMetrics {
            units_per_em: upem,
            ..Default::default()
        },
    }];
    FONT.with_borrow_mut(|slot| {
        *slot = Some(FontFixture {
            bytes,
            files,
            faces,
            runs: Vec::new(),
            strings: Vec::new(),
            generation: 0,
            fills: 0,
        })
    });
    let mut callbacks = callbacks();
    callbacks.plan = Some(real_plan);
    callbacks.fill_font = Some(real_fill);
    callbacks
}

#[test]
fn real_font_replay_shapes_whole_run_and_owned_font_survives_provider_retirement() {
    let host = Arc::new(TestHost::default());
    let mut job =
        OwnedJob::new(identity(), install_font(), host.clone(), Limits::default()).unwrap();
    job.prepare(b"<svg xmlns='http://www.w3.org/2000/svg' width='180' height='50'><text x='3' y='30' font-family='fixture' font-size='24'>office A&amp;B</text></svg>").unwrap();
    let stats = job.stats().unwrap();
    assert_eq!(stats.drafts, 1);
    assert!(stats.glyphs > 0);
    FONT.with_borrow_mut(|slot| {
        let fixture = slot.take().unwrap();
        assert_eq!(fixture.fills, 1);
    });
    let plan = job.set_target(target(180.0)).unwrap();
    let mut pixels = vec![0; plan.capacity];
    job.run_into(identity(), &mut pixels, plan.stride).unwrap();
    assert!(pixels.chunks_exact(4).any(|p| p[3] != 0));
    let receipt = finish(&mut job, &host);
    assert_eq!(receipt.first_fault, None);
    let stats = job.stats().unwrap();
    assert_eq!(stats.created_owners[0], 1);
    assert_eq!(stats.created_owners, stats.retired_owners);
}

#[cfg(feature = "quality-observation")]
#[test]
fn quality_copy_is_complete_and_observation_preserves_actual_rendering() {
    const SOURCE: &[u8] = b"<svg xmlns='http://www.w3.org/2000/svg' width='180' height='50'><text x='3' y='30' font-family='fixture' font-size='24'>office A&amp;B</text></svg>";
    let render = |observe: bool| {
        let host = Arc::new(TestHost::default());
        let mut job =
            OwnedJob::new(identity(), install_font(), host.clone(), Limits::default()).unwrap();
        if observe {
            job.enable_quality_observation(1024 * 1024).unwrap();
        }
        job.prepare(SOURCE).unwrap();
        if observe {
            let info = job.quality_trace_info().unwrap();
            assert!(info.bytes > 0 && info.events > 0);
            assert_eq!((info.flags, info.error), (1, 0));
            let mut short = vec![0xAA; info.bytes as usize - 1];
            assert_eq!(
                job.copy_quality_trace(&mut short),
                Err(JobError::InvalidArgument)
            );
            assert!(short.iter().all(|byte| *byte == 0xAA));
            let mut bytes = vec![0; info.bytes as usize];
            assert_eq!(job.copy_quality_trace(&mut bytes).unwrap(), bytes.len());
            let text = std::str::from_utf8(&bytes).unwrap();
            assert!(
                text.ends_with('\n')
                    && text.contains("actualGlyphRun")
                    && text.contains("actualFontFile")
            );
            let batches = text
                .lines()
                .filter(|line| line.starts_with("{\"kind\":\"glyphFillSource\""));
            let mut observed_paths = 0;
            for batch in batches {
                observed_paths += 1;
                assert!(batch.contains(
                    "\"flattenPath\":{\"absMatrixBits\":[1065353216,0,0,1065353216,0,0]"
                ));
                assert!(batch.contains("\"visible\":true,\"hasStroke\":false,\"fillRule\":1"));
                assert!(batch.contains("\"paintOrder\":\"fill-stroke\",\"commandsBits\":[[0,"));
                assert!(batch.ends_with("]}}"));
            }
            assert!(observed_paths > 0);
        }
        FONT.with_borrow_mut(|slot| {
            slot.take().unwrap();
        });
        let plan = job.set_target(target(180.0)).unwrap();
        let mut pixels = vec![0; plan.capacity];
        job.run_into(identity(), &mut pixels, plan.stride).unwrap();
        assert_eq!(finish(&mut job, &host).first_fault, None);
        assert_eq!(job.quality_trace_info(), Err(JobError::InvalidState));
        pixels
    };
    assert_eq!(render(true), render(false));
}

#[cfg(feature = "quality-observation")]
#[test]
fn quality_overflow_preserves_an_empty_committed_prefix_and_retires() {
    let host = Arc::new(TestHost::default());
    let mut job =
        OwnedJob::new(identity(), install_font(), host.clone(), Limits::default()).unwrap();
    job.enable_quality_observation(8).unwrap();
    assert!(job.prepare(b"<svg xmlns='http://www.w3.org/2000/svg'><text font-family='fixture'>office</text></svg>").is_err());
    let info = job.quality_trace_info().unwrap();
    assert_eq!(
        (info.bytes, info.events, info.flags, info.error),
        (0, 0, 3, 1)
    );
    let mut destination = [0xCC; 8];
    assert_eq!(job.copy_quality_trace(&mut destination).unwrap(), 0);
    assert_eq!(destination, [0xCC; 8]);
    FONT.with_borrow_mut(|slot| {
        slot.take().unwrap();
    });
    let receipt = finish(&mut job, &host);
    assert_eq!(receipt.first_fault.unwrap().kind, FailureKind::Quota);
}

#[test]
fn original_font_error_remains_first_when_worker_cleanup_release_also_fails() {
    let host = Arc::new(TestHost::default());
    let mut job = job(&host);
    assert_eq!(
        job.prepare(b"<svg xmlns='http://www.w3.org/2000/svg'><text>x</text></svg>"),
        Err(JobError::Failure(Error::native(-101)))
    );
    host.release_hr.store((-777i32) as u32, Ordering::Relaxed);
    let receipt = job.finish_on_worker().unwrap();
    assert_eq!(receipt.first_fault, Some(Error::native(-101)));
    assert_eq!(receipt.cleanup_status, CleanupStatus::Failed);
    assert!(!job.can_destroy_shell());
}

#[test]
fn cleanup_panic_takes_each_slot_once_and_permanently_blocks_publication() {
    let host = Arc::new(TestHost::default());
    let mut job = job(&host);
    job.prepare(RECT).unwrap();
    let plan = job.set_target(target(64.0)).unwrap();
    let mut pixels = vec![0; plan.capacity];
    job.run_into(identity(), &mut pixels, plan.stride).unwrap();
    host.panic_release.store(true, Ordering::Relaxed);
    let receipt = job.finish_on_worker().unwrap();
    assert_eq!(receipt.cleanup_status, CleanupStatus::Failed);
    assert_eq!(job.state(), JobState::RetirementFailed);
    let releases = host.released.load(Ordering::Relaxed);
    assert_eq!(job.finish_on_worker(), Err(JobError::InvalidState));
    drop(job);
    assert_eq!(host.released.load(Ordering::Relaxed), releases);
    assert!(!host.live.lock().unwrap().is_empty());
}

#[test]
fn exact_bootstrap_manifest_matches_real_system_allocations() {
    let host = Arc::new(TestHost::default());
    // Fund the test ledger's existing node before observation. This allocation
    // belongs to the test backend, not to the engine bootstrap manifest.
    host.live.lock().unwrap().insert(u32::MAX, (0, false));
    let _ = std::thread::current().id();
    start_events();
    let mut job = Box::new(job(&host));
    let events = stop_events();
    host.live.lock().unwrap().remove(&u32::MAX);
    let actual: usize = events
        .iter()
        .filter(|event| event.tag == 1)
        .map(|event| event.bytes)
        .sum();
    let paid_dynamic: usize = host.live.lock().unwrap().values().map(|item| item.0).sum();
    let metadata = bootstrap_metadata();
    eprintln!(
        "bootstrap actual={actual}B fixed={}B separately paid={paid_dynamic}B allocations={} manifest={metadata:?}",
        metadata.total_bytes().unwrap(),
        events.iter().filter(|e| e.tag == 1).count()
    );
    assert_eq!(
        actual,
        metadata.total_bytes().unwrap() + paid_dynamic,
        "{events:?}"
    );
    finish(&mut job, &host);
}

#[test]
fn bootstrap_storage_retires_before_embedding_batch_and_last_cancel_refund() {
    let host = Arc::new(TestHost::default());
    host.live.lock().unwrap().insert(u32::MAX, (0, false));
    let _ = std::thread::current().id();
    let bytes = bootstrap_metadata().total_bytes().unwrap();
    start_events();
    let batch = host.reserve(ResourceKind::Scratch, bytes).unwrap();
    let batch_slot = batch.slot();
    let mut job = Box::new(job(&host));
    let cancel = job.cancellation();
    let offset = std::alloc::Layout::new::<(usize, usize)>()
        .extend(std::alloc::Layout::new::<AtomicBool>())
        .unwrap()
        .1;
    let cancel_pointer = Arc::as_ptr(&cancel.0) as usize - offset;
    let mut separately_paid = [0usize; 8];
    let mut separate_count = 0;
    for (slot, (size, _)) in host.live.lock().unwrap().iter() {
        if *slot != batch_slot && *slot != u32::MAX {
            separately_paid[separate_count] = *size;
            separate_count += 1;
        }
    }
    let constructed = EVENTS.with_borrow(|events| events.length);
    host.allocation_committed(&batch, bytes).unwrap();
    let retired = job.finish_on_worker().unwrap();
    assert_eq!(retired.cleanup_status, CleanupStatus::Succeeded);
    assert!(job.can_destroy_shell());
    assert_eq!(host.live.lock().unwrap().len(), 2);
    drop(job);
    let shell_dropped = EVENTS.with_borrow(|events| events.length);
    assert!(host.live.lock().unwrap().contains_key(&batch_slot));
    drop(cancel);
    let cancel_dropped = EVENTS.with_borrow(|events| events.length);
    // This unit-test embedding owner follows the actual batch contract. It
    // does not stand in for Native reaper/Core-ledger execution evidence.
    host.release_after_free(batch).unwrap();
    let events = stop_events();
    host.live.lock().unwrap().remove(&u32::MAX);
    assert!(host.live.lock().unwrap().is_empty());
    let reserved = events
        .iter()
        .position(|e| e.tag == 3 && e.identity == batch_slot as usize)
        .unwrap();
    let admitted = events
        .iter()
        .position(|e| e.tag == 4 && e.identity == batch_slot as usize)
        .unwrap();
    let refunded = events
        .iter()
        .position(|e| e.tag == 5 && e.identity == batch_slot as usize)
        .unwrap();
    let mut observed_bootstrap = 0;
    for allocation in events[..constructed].iter().filter(|e| e.tag == 1) {
        if separately_paid[..separate_count].contains(&allocation.bytes) {
            assert_eq!(
                events[..constructed]
                    .iter()
                    .filter(|e| e.tag == 1 && e.bytes == allocation.bytes)
                    .count(),
                1,
                "a dynamic fee must identify one exact constructor allocation"
            );
            continue;
        }
        let allocated = events
            .iter()
            .position(|e| e.tag == 1 && e.identity == allocation.identity)
            .unwrap();
        let freed = events
            .iter()
            .position(|e| {
                e.tag == 2 && e.identity == allocation.identity && e.bytes == allocation.bytes
            })
            .unwrap();
        assert!(
            reserved < allocated && allocated < admitted && admitted < freed && freed < refunded
        );
        if allocation.identity == cancel_pointer {
            assert!(freed >= shell_dropped && freed < cancel_dropped);
        } else {
            assert!(freed < shell_dropped);
        }
        observed_bootstrap += allocation.bytes;
    }
    assert_eq!(observed_bootstrap, bytes);
    assert_eq!(
        events[..constructed]
            .iter()
            .filter(|e| e.tag == 1 && e.identity == cancel_pointer)
            .count(),
        1
    );
}

#[test]
fn worker_identity_is_enforced_and_successful_shell_can_retire_on_another_thread() {
    fn assert_send<T: Send>() {}
    assert_send::<OwnedJob>();
    let host = Arc::new(TestHost::default());
    let job = job(&host);
    let mut job = std::thread::spawn(move || {
        let mut job = job;
        assert_eq!(job.prepare(RECT), Err(JobError::InvalidState));
        assert_eq!(job.finish_on_worker(), Err(JobError::InvalidState));
        job
    })
    .join()
    .unwrap();
    job.prepare(RECT).unwrap();
    finish(&mut job, &host);
    let cancel = job.cancellation();
    let releases = host.released.load(Ordering::Relaxed);
    std::thread::spawn(move || drop(job)).join().unwrap();
    assert_eq!(host.released.load(Ordering::Relaxed), releases);
    cancel.cancel();
    assert!(cancel.is_cancelled());
}

#[path = "font_actual_tests.rs"]
mod font_actual_tests;

#[path = "family_transport_tests.rs"]
mod family_transport_tests;
