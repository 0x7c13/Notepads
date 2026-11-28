// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE file in the project root for license information.
// ---------------------------------------------------------------------------------------------
//! Dedicated geometry helper tests: canonical mock tickets and TLS allocation observation.

use std::alloc::{GlobalAlloc, Layout, System};
use std::cell::Cell;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, MutexGuard};

use markdown_svg_budget::{
    Backend, Error, FailureKind, FirstFailureRef, LimitKind, Limits, ResourceKind, ResourcesRef,
    Ticket,
};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct Allocations {
    pub alloc: usize,
    pub zeroed: usize,
    pub realloc: usize,
    pub dealloc: usize,
}

std::thread_local! {
    // Constant TLS has neither a heap allocation nor a destructor. All allocator
    // callbacks only update this Cell; logging/assertions happen outside them.
    static ALLOCATIONS: Cell<Option<Allocations>> = const { Cell::new(None) };
}

struct CounterAllocator;

fn observe(update: impl FnOnce(&mut Allocations)) {
    let _ = ALLOCATIONS.try_with(|slot| {
        if let Some(mut value) = slot.get() {
            update(&mut value);
            slot.set(Some(value));
        }
    });
}

// SAFETY: every allocation operation is forwarded once, with unchanged pointer,
// layout and size, to System. TLS observation neither allocates nor dereferences
// allocation pointers. Other test threads have independent disabled counters.
unsafe impl GlobalAlloc for CounterAllocator {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        observe(|s| s.alloc += 1);
        unsafe { System.alloc(layout) }
    }
    unsafe fn alloc_zeroed(&self, layout: Layout) -> *mut u8 {
        observe(|s| s.zeroed += 1);
        unsafe { System.alloc_zeroed(layout) }
    }
    unsafe fn realloc(&self, ptr: *mut u8, layout: Layout, size: usize) -> *mut u8 {
        observe(|s| s.realloc += 1);
        unsafe { System.realloc(ptr, layout, size) }
    }
    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        observe(|s| s.dealloc += 1);
        unsafe { System.dealloc(ptr, layout) }
    }
}

#[global_allocator]
static ALLOCATOR: CounterAllocator = CounterAllocator;

static OBSERVER_LOCK: Mutex<()> = Mutex::new(());

pub(crate) struct AllocationObservation {
    _lock: MutexGuard<'static, ()>,
}

impl AllocationObservation {
    pub fn begin() -> Self {
        let lock = OBSERVER_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        ALLOCATIONS.with(|slot| {
            assert!(slot.replace(Some(Allocations::default())).is_none());
        });
        Self { _lock: lock }
    }
    pub fn finish(self) -> Allocations {
        ALLOCATIONS.with(|slot| slot.take().expect("active allocation observation"))
    }
}

impl Drop for AllocationObservation {
    fn drop(&mut self) {
        let _ = ALLOCATIONS.try_with(|slot| slot.set(None));
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Call {
    Reserve,
    Commit,
    Checkpoint,
    Release,
}

#[derive(Clone, Copy, Debug)]
pub(crate) struct Event {
    pub call: Call,
    pub ordinal: usize,
    pub work: u64,
    pub bytes: usize,
    pub live_bytes: usize,
    pub live_leases: usize,
}

#[derive(Clone, Copy, Debug)]
struct Live {
    slot: u32,
    bytes: usize,
    kind: ResourceKind,
    admitted: bool,
}

#[derive(Clone, Copy, Debug, Default)]
pub(crate) struct Counts {
    pub reserve: usize,
    pub commit: usize,
    pub checkpoint: usize,
    pub release: usize,
}

pub(crate) struct Inner {
    nonce: u64,
    next: u32,
    live: Vec<Live>,
    pub events: Vec<Event>,
    pub counts: Counts,
    fault: Option<(Call, usize, Error)>,
    pub peak_bytes: usize,
    pub peak_leases: usize,
}

impl Inner {
    fn live_bytes(&self) -> usize {
        self.live.iter().map(|v| v.bytes).sum()
    }
    fn event(&mut self, call: Call, ordinal: usize, work: u64, bytes: usize) -> Result<(), Error> {
        // Preallocated logs make measured cache hits observable without a mock
        // backend allocation. Exceeding the fixed fixture capacity is a failure.
        assert!(self.events.len() < self.events.capacity());
        let live_bytes = self.live_bytes();
        self.peak_bytes = self.peak_bytes.max(live_bytes);
        self.peak_leases = self.peak_leases.max(self.live.len());
        self.events.push(Event {
            call,
            ordinal,
            work,
            bytes,
            live_bytes,
            live_leases: self.live.len(),
        });
        match self.fault {
            Some((kind, at, error)) if call == kind && ordinal == at => Err(error),
            _ => Ok(()),
        }
    }
}

pub(crate) struct State(pub Mutex<Inner>);

impl State {
    pub fn live_tickets(&self) -> Vec<(u32, usize)> {
        self.lock().live.iter().map(|v| (v.slot, v.bytes)).collect()
    }
    pub fn lock(&self) -> MutexGuard<'_, Inner> {
        self.0.lock().unwrap()
    }
    pub fn counts(&self) -> Counts {
        self.lock().counts
    }
    pub fn arm(&self, call: Call, relative: usize, error: Error) {
        let mut s = self.lock();
        let base = match call {
            Call::Reserve => s.counts.reserve,
            Call::Commit => s.counts.commit,
            Call::Checkpoint => s.counts.checkpoint,
            Call::Release => s.counts.release,
        };
        s.fault = Some((call, base + relative, error));
    }
    pub fn assert_live(&self, r: &ResourcesRef, leases: usize) {
        let s = self.lock();
        assert_eq!(s.live.len(), leases);
        assert_eq!(s.live_bytes() as u64, r.statistics().scratch_bytes);
        assert_eq!(s.live.len() as u64, r.statistics().live_leases);
        assert!(s.live.iter().all(|v| v.kind == ResourceKind::Scratch));
    }
    pub fn kind_bytes(&self, kind: ResourceKind) -> usize {
        self.lock()
            .live
            .iter()
            .filter(|v| v.kind == kind)
            .map(|v| v.bytes)
            .sum()
    }
    pub fn assert_account(&self, r: &ResourcesRef) {
        let s = self.lock();
        let scratch: usize = s
            .live
            .iter()
            .filter(|v| v.kind == ResourceKind::Scratch)
            .map(|v| v.bytes)
            .sum();
        assert_eq!(scratch as u64, r.statistics().scratch_bytes);
        assert_eq!(s.live.len() as u64, r.statistics().live_leases);
    }
    pub fn live_len(&self) -> usize {
        self.lock().live.len()
    }
}

struct Host(Arc<State>);

impl Backend for Host {
    fn reserve(&self, kind: ResourceKind, bytes: usize) -> Result<Ticket, Error> {
        let mut s = self.0.lock();
        s.counts.reserve += 1;
        let ordinal = s.counts.reserve;
        s.event(Call::Reserve, ordinal, 0, bytes)?;
        s.next += 1;
        let slot = s.next;
        assert!(s.live.len() < s.live.capacity());
        s.live.push(Live {
            slot,
            bytes,
            kind,
            admitted: false,
        });
        s.peak_bytes = s.peak_bytes.max(s.live_bytes());
        s.peak_leases = s.peak_leases.max(s.live.len());
        Ticket::new(s.nonce, slot, 1)
    }
    fn allocation_committed(&self, ticket: &Ticket, bytes: usize) -> Result<(), Error> {
        let mut s = self.0.lock();
        let v = s.live.iter_mut().find(|v| v.slot == ticket.slot()).unwrap();
        assert_eq!(v.bytes, bytes);
        assert!(!v.admitted);
        v.admitted = true;
        s.counts.commit += 1;
        let ordinal = s.counts.commit;
        s.event(Call::Commit, ordinal, 0, bytes)
    }
    fn release_after_free(&self, ticket: Ticket) -> Result<(), Error> {
        let mut s = self.0.lock();
        let index = s.live.iter().position(|v| v.slot == ticket.slot()).unwrap();
        assert_eq!(ticket.account_nonce(), s.nonce);
        assert_eq!(ticket.generation(), 1);
        s.counts.release += 1;
        let ordinal = s.counts.release;
        let bytes = s.live[index].bytes;
        // On a failed refund, retain the canonical ticket and charged bytes.
        s.event(Call::Release, ordinal, 0, bytes)?;
        s.live.swap_remove(index);
        Ok(())
    }
    fn checkpoint(&self, work: u64) -> Result<(), Error> {
        let mut s = self.0.lock();
        s.counts.checkpoint += 1;
        let ordinal = s.counts.checkpoint;
        s.event(Call::Checkpoint, ordinal, work, 0)
    }
}

pub(crate) fn request(limits: Limits) -> (ResourcesRef, Arc<State>) {
    static NEXT_ACCOUNT: AtomicU64 = AtomicU64::new(0x2026_1102);
    let state = Arc::new(State(Mutex::new(Inner {
        nonce: NEXT_ACCOUNT.fetch_add(1, Ordering::Relaxed),
        next: 0,
        live: Vec::with_capacity(4096),
        events: Vec::with_capacity(65536),
        counts: Counts::default(),
        fault: None,
        peak_bytes: 0,
        peak_leases: 0,
    })));
    let r = ResourcesRef::new_prepaid(Host(state.clone()), FirstFailureRef::new_prepaid(), limits);
    (r, state)
}

pub(crate) fn native(code: i32) -> Error {
    Error::native(code)
}

pub(crate) fn cancelled() -> Error {
    Error::new(FailureKind::Cancelled, LimitKind::None)
}

pub(crate) fn outline() -> tiny_skia_path::Path {
    let mut p = tiny_skia_path::PathBuilder::new();
    p.move_to(0.0, 0.0);
    p.cubic_to(1.0, 2.0, 3.0, 4.0, 5.0, 6.0);
    p.line_to(7.0, 2.0);
    p.close();
    p.finish().unwrap()
}
