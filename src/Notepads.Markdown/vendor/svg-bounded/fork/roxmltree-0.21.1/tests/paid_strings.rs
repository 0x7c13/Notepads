// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE file in the project root for license information.
// ---------------------------------------------------------------------------------------------
//! Pure actual-allocation observer. This allocator only forwards to System;
//! request admission remains the production ResourcesRef/paid storage path.
#![cfg(feature = "notepads-markdown-resources")]

use markdown_svg_budget::{
    Backend, Error, FailureKind, FirstFailureRef, LimitKind, Limits, ResourceKind, ResourcesRef,
    Ticket,
};
use roxmltree::{Document, ParsingOptions, StringStorage};
use std::alloc::{GlobalAlloc, Layout, System};
use std::cell::Cell;
use std::sync::atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering};
use std::sync::Arc;

const ALLOC: usize = 1;
const FREE: usize = 2;
const RESERVE: usize = 3;
const RELEASE: usize = 4;
const REALLOC: usize = 5;
const MAX_EVENTS: usize = 1024;
const MAX_TICKETS: usize = 128;

thread_local! {
    static TRACKING: Cell<bool> = const { Cell::new(false) };
}
struct Event {
    operation: AtomicUsize,
    address: AtomicUsize,
    bytes: AtomicUsize,
}
impl Event {
    const fn new() -> Self {
        Self {
            operation: AtomicUsize::new(0),
            address: AtomicUsize::new(0),
            bytes: AtomicUsize::new(0),
        }
    }
}
static EVENTS: [Event; MAX_EVENTS] = [const { Event::new() }; MAX_EVENTS];
static NEXT: AtomicUsize = AtomicUsize::new(0);
static OVERFLOW: AtomicBool = AtomicBool::new(false);
fn record(operation: usize, address: usize, bytes: usize) {
    if !TRACKING.try_with(Cell::get).unwrap_or(false) {
        return;
    }
    let index = NEXT.fetch_add(1, Ordering::Relaxed);
    if index >= MAX_EVENTS {
        OVERFLOW.store(true, Ordering::Relaxed);
        return;
    }
    EVENTS[index].address.store(address, Ordering::Relaxed);
    EVENTS[index].bytes.store(bytes, Ordering::Relaxed);
    EVENTS[index].operation.store(operation, Ordering::Release);
}
struct ObservedSystem;
// SAFETY: Every method forwards its unchanged pointer/Layout arguments to the
// same System allocator. Observation uses only preallocated atomics and a const
// thread-local Cell. It never changes allocation success, size or alignment.
unsafe impl GlobalAlloc for ObservedSystem {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        // SAFETY: GlobalAlloc supplied the valid allocation Layout unchanged.
        let value = unsafe { System.alloc(layout) };
        if !value.is_null() {
            record(ALLOC, value as usize, layout.size());
        }
        value
    }
    unsafe fn alloc_zeroed(&self, layout: Layout) -> *mut u8 {
        // SAFETY: The valid allocation Layout is forwarded unchanged.
        let value = unsafe { System.alloc_zeroed(layout) };
        if !value.is_null() {
            record(ALLOC, value as usize, layout.size());
        }
        value
    }
    unsafe fn dealloc(&self, pointer: *mut u8, layout: Layout) {
        // SAFETY: This exact System allocation is returned with its Layout.
        unsafe { System.dealloc(pointer, layout) };
        // The event is deliberately recorded after physical deallocation.
        record(FREE, pointer as usize, layout.size());
    }
    unsafe fn realloc(&self, pointer: *mut u8, layout: Layout, size: usize) -> *mut u8 {
        // SAFETY: GlobalAlloc's valid System pointer and Layout are unchanged.
        let value = unsafe { System.realloc(pointer, layout, size) };
        record(REALLOC, pointer as usize, size);
        value
    }
}
#[global_allocator]
static ALLOCATOR: ObservedSystem = ObservedSystem;

struct Trace;
impl Trace {
    fn begin() -> Self {
        NEXT.store(0, Ordering::Relaxed);
        OVERFLOW.store(false, Ordering::Relaxed);
        TRACKING.with(|tracking| tracking.set(true));
        Self
    }
}
impl Drop for Trace {
    fn drop(&mut self) {
        TRACKING.with(|tracking| tracking.set(false));
    }
}
#[derive(Clone, Copy, Debug)]
struct Observed {
    operation: usize,
    address: usize,
    bytes: usize,
    sequence: usize,
}
fn observed() -> Vec<Observed> {
    assert!(!OVERFLOW.load(Ordering::Relaxed));
    (0..NEXT.load(Ordering::Relaxed))
        .map(|index| Observed {
            operation: EVENTS[index].operation.load(Ordering::Acquire),
            address: EVENTS[index].address.load(Ordering::Relaxed),
            bytes: EVENTS[index].bytes.load(Ordering::Relaxed),
            sequence: index,
        })
        .collect()
}
fn require_funded_allocations(events: &[Observed]) {
    let mut used = Vec::new();
    for allocation in events.iter().filter(|event| event.operation == ALLOC) {
        let reservation = events
            .iter()
            .rev()
            .find(|event| {
                event.operation == RESERVE
                    && event.sequence < allocation.sequence
                    && event.bytes == allocation.bytes
                    && !used.contains(&event.address)
                    && !events.iter().any(|release| {
                        release.operation == RELEASE
                            && release.address == event.address
                            && release.sequence > event.sequence
                            && release.sequence < allocation.sequence
                    })
            })
            .expect("actual controlled allocation lacks an earlier matching reservation");
        used.push(reservation.address);
    }
    assert!(!events.iter().any(|event| event.operation == REALLOC));
}
struct State {
    next: AtomicU64,
    live: AtomicU64,
    peak: AtomicU64,
    bytes: [AtomicU64; MAX_TICKETS],
}
struct ObserverBackend(Arc<State>);
impl Backend for ObserverBackend {
    fn reserve(&self, _: ResourceKind, bytes: usize) -> Result<Ticket, Error> {
        let ticket = self.0.next.fetch_add(1, Ordering::Relaxed) + 1;
        assert!((ticket as usize) < MAX_TICKETS);
        self.0.bytes[ticket as usize].store(bytes as u64, Ordering::Relaxed);
        let live = self.0.live.fetch_add(bytes as u64, Ordering::Relaxed) + bytes as u64;
        self.0.peak.fetch_max(live, Ordering::Relaxed);
        record(RESERVE, ticket as usize, bytes);
        Ticket::new(1, ticket as u32, 1)
    }
    fn allocation_committed(&self, _: &Ticket, _: usize) -> Result<(), Error> {
        Ok(())
    }
    fn release_after_free(&self, ticket: Ticket) -> Result<(), Error> {
        let index = u64::from(ticket.slot()) as usize;
        let bytes = self.0.bytes[index].swap(0, Ordering::Relaxed);
        self.0.live.fetch_sub(bytes, Ordering::Relaxed);
        record(RELEASE, index, bytes as usize);
        Ok(())
    }
    fn checkpoint(&self, _: u64) -> Result<(), Error> {
        Ok(())
    }
}
fn setup(limits: Limits) -> (ResourcesRef, Arc<State>) {
    let state = Arc::new(State {
        next: AtomicU64::new(0),
        live: AtomicU64::new(0),
        peak: AtomicU64::new(0),
        bytes: [const { AtomicU64::new(0) }; MAX_TICKETS],
    });
    let resources = ResourcesRef::new_prepaid(
        ObserverBackend(state.clone()),
        FirstFailureRef::new_prepaid(),
        limits,
    );
    (resources, state)
}
fn parse<'input>(
    source: &'input str,
    resources: &ResourcesRef,
) -> Result<Document<'input>, roxmltree::Error> {
    Document::parse_with_options(
        source,
        ParsingOptions {
            resources: Some(resources.clone()),
            ..Default::default()
        },
    )
}

#[test]
fn actual_normalized_allocations_are_paid_and_physically_free_before_refund() {
    // Input and fake-backend/bootstrap bookkeeping are outside the observed
    // region. Only the actual production parse/string allocations are sampled.
    let source = "<e a='中&#xA;文'>&amp;😀中文</e>";
    let (resources, state) = setup(Limits::default());
    let trace = Trace::begin();
    let doc = parse(source, &resources).unwrap();
    let storage = doc.root_element().text_storage().unwrap();
    let shared = match storage {
        StringStorage::Paid(value) => value,
        _ => panic!("normalization did not return paid storage"),
    };
    let header_value = shared.as_ref() as *const _ as usize;
    let buffer = shared.as_ref().as_str().as_ptr() as usize;
    let clone_begin = NEXT.load(Ordering::Relaxed);
    let cloned = storage.clone();
    let clone_end = NEXT.load(Ordering::Relaxed);
    drop(doc);
    let document_gone = NEXT.load(Ordering::Relaxed);
    assert!(state.live.load(Ordering::Relaxed) > 0);
    drop(cloned);
    drop(trace);
    let events = observed();
    require_funded_allocations(&events);
    assert!(!events[clone_begin..clone_end]
        .iter()
        .any(|event| event.operation == ALLOC));
    let header = events
        .iter()
        .rev()
        .find(|event| {
            event.operation == ALLOC
                && event.sequence < clone_begin
                && event.address <= header_value
                && header_value < event.address + event.bytes
        })
        .unwrap();
    let payload = events
        .iter()
        .rev()
        .find(|event| {
            event.operation == ALLOC && event.sequence < clone_begin && event.address == buffer
        })
        .unwrap();
    let freed_header = events
        .iter()
        .find(|event| {
            event.operation == FREE
                && event.sequence > header.sequence
                && event.address == header.address
        })
        .unwrap();
    let freed_payload = events
        .iter()
        .find(|event| {
            event.operation == FREE
                && event.sequence > payload.sequence
                && event.address == payload.address
        })
        .unwrap();
    assert!(freed_header.sequence >= document_gone);
    assert!(freed_payload.sequence >= document_gone);
    assert!(
        freed_header.sequence < freed_payload.sequence,
        "last into_inner did not free the shared block first"
    );
    let header_reserve = events
        .iter()
        .rev()
        .find(|event| {
            event.operation == RESERVE
                && event.sequence < header.sequence
                && event.bytes == header.bytes
        })
        .unwrap();
    let payload_reserve = events
        .iter()
        .rev()
        .find(|event| {
            event.operation == RESERVE
                && event.sequence < payload.sequence
                && event.bytes == payload.bytes
        })
        .unwrap();
    let header_refund = events
        .iter()
        .find(|event| event.operation == RELEASE && event.address == header_reserve.address)
        .unwrap();
    let payload_refund = events
        .iter()
        .find(|event| event.operation == RELEASE && event.address == payload_reserve.address)
        .unwrap();
    assert!(freed_header.sequence < header_refund.sequence);
    assert!(freed_payload.sequence < payload_refund.sequence);
    assert!(payload_refund.sequence < header_refund.sequence);
    assert_eq!(state.live.load(Ordering::Relaxed), 0);
    resources.validate_retired().unwrap();

    println!("{{\"positive_actual_allocations\":{},\"one_text_shared_block_bytes\":{},\"one_text_payload_bytes\":{},\"positive_tracked_peak_bytes\":{}}}",
        events.iter().filter(|event| event.operation == ALLOC).count(),
        header.bytes, payload.bytes, state.peak.load(Ordering::Relaxed));

    let name = "a".repeat(8192);
    let invalid = [
        format!("<e>&{name};</e>"),
        format!("<e a='&{name};'/>"),
        format!("<e {name}='a' {name}='b'/>"),
        format!("<e xmlns:{name}='a' xmlns:{name}='b'/>"),
        format!("<{name}></different>"),
        format!("<{name}:e/>"),
    ];
    for source in &invalid {
        let (resources, state) = setup(Limits::default());
        let trace = Trace::begin();
        let result = parse(source, &resources);
        assert!(result.is_err());
        drop(result);
        drop(trace);
        let events = observed();
        require_funded_allocations(&events);
        assert!(!events
            .iter()
            .any(|event| event.operation == ALLOC && event.bytes >= 8192));
        assert_eq!(resources.failure().unwrap().kind, FailureKind::Decode);
        assert_eq!(state.live.load(Ordering::Relaxed), 0);
        resources.validate_retired().unwrap();
    }

    let source = format!("<e a='{}\r中文&amp;尾'/>", "x".repeat(12_000));
    let (baseline, state) = setup(Limits::default());
    drop(parse(&source, &baseline).unwrap());
    let peak = state.peak.load(Ordering::Relaxed);
    baseline.validate_retired().unwrap();
    let mut limits = Limits::default();
    limits.scratch_bytes = peak - 1;
    let (resources, state) = setup(limits);
    let trace = Trace::begin();
    let result = parse(&source, &resources);
    assert!(result.is_err());
    drop(result);
    drop(trace);
    let events = observed();
    require_funded_allocations(&events);
    assert_eq!(resources.failure().unwrap().limit, LimitKind::Scratch);
    assert_eq!(state.live.load(Ordering::Relaxed), 0);
    resources.validate_retired().unwrap();
    println!("{{\"actual_allocation_funding\":true,\"clone_allocations\":0,\"header_and_payload_free_before_refund\":true,\"large_named_error_cases\":6,\"real_quota_refusal\":true,\"native_or_font_calls\":0}}");
}
