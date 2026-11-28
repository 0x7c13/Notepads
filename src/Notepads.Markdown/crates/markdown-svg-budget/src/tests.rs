// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE file in the project root for license information.
// ---------------------------------------------------------------------------------------------
use super::*;

#[path = "storage_tests.rs"]
mod storage_tests;

#[path = "callback_capture_tests.rs"]
mod callback_capture_tests;

#[test]
fn policy_borrows_original_payers_and_checks_static_without_allocation() {
    let _isolation = isolate_observer();
    let (r, _, state) = setup(Limits::default());
    let mut bytes = BudgetVec::new(r.clone(), ResourceKind::Scratch);
    bytes.try_extend_from_slice(&[1u8, 2]).unwrap();
    assert!(bytes.resources().is_same_request(&r));
    let list = FrozenList::try_freeze(&r, bytes).unwrap();
    let text = PaidUtf8::try_from_str(r.clone(), ResourceKind::Scratch, "中文").unwrap();
    let shared = PaidShared::try_new(&r, ResourceKind::Scratch, 7u32).unwrap();
    let allocator = BucketAllocator::new(r.clone(), ResourceKind::Scratch);
    let reserves = state.reserves.load(Ordering::Relaxed);
    assert!(text.resources().is_same_request(&r));
    assert!(shared.resources().is_same_request(&r));
    assert!(allocator.resources().is_same_request(&r));
    list.validate_policy(Some(&r)).unwrap();
    FrozenList::from_static(&[3u32])
        .validate_policy(Some(&r))
        .unwrap();
    FrozenList::from_static(&[3u32])
        .validate_policy(None)
        .unwrap();
    assert_eq!(state.reserves.load(Ordering::Relaxed), reserves);
    drop((list, text, shared, allocator));
    assert_eq!(state.live.load(Ordering::Relaxed), 0);
    r.validate_retired().unwrap();
}

#[test]
fn frozen_policy_checks_header_and_payload_independently() {
    let _isolation = isolate_observer();
    for foreign_header in [false, true] {
        let (a, _, a_state) = setup(Limits::default());
        let (b, _, b_state) = setup(Limits::default());
        let (header, buffer) = if foreign_header { (&b, &a) } else { (&a, &b) };
        let mut bytes = BudgetVec::new(buffer.clone(), ResourceKind::Scratch);
        bytes.try_push(7u8).unwrap();
        let list = crate::frozen::mismatched_owner_for_test(header, bytes).unwrap();
        let before = (
            a_state.reserves.load(Ordering::Relaxed),
            b_state.reserves.load(Ordering::Relaxed),
        );
        let invariant = Error::new(FailureKind::Invariant, LimitKind::None);
        assert_eq!(list.validate_policy(Some(&a)), Err(invariant));
        assert_eq!(a.failure(), Some(invariant));
        assert_eq!(b.failure(), Some(invariant));
        assert_eq!(
            (
                a_state.reserves.load(Ordering::Relaxed),
                b_state.reserves.load(Ordering::Relaxed)
            ),
            before
        );
        drop(list);
        assert_eq!(a_state.live.load(Ordering::Relaxed), 0);
        assert_eq!(b_state.live.load(Ordering::Relaxed), 0);
    }
}

#[test]
fn frozen_policy_rejects_demotion_and_plain_upgrade() {
    let _isolation = isolate_observer();
    let (r, _, state) = setup(Limits::default());
    let mut bytes = BudgetVec::new(r.clone(), ResourceKind::Scratch);
    bytes.try_push(1u8).unwrap();
    let list = FrozenList::try_freeze(&r, bytes).unwrap();
    let invariant = Error::new(FailureKind::Invariant, LimitKind::None);
    assert_eq!(list.validate_policy(None), Err(invariant));
    assert_eq!(r.failure(), Some(invariant));
    drop(list);
    assert_eq!(state.live.load(Ordering::Relaxed), 0);
    let (target, _, target_state) = setup(Limits::default());
    let plain = FrozenList::from_compat(vec![1u32]);
    assert_eq!(plain.validate_policy(Some(&target)), Err(invariant));
    assert_eq!(target_state.reserves.load(Ordering::Relaxed), 0);
    assert_eq!(target.failure(), Some(invariant));
}

#[test]
fn frozen_policy_preserves_existing_target_then_source_hresult() {
    let _isolation = isolate_observer();
    for target_failed in [false, true] {
        let (a, _, a_state) = setup(Limits::default());
        let (b, _, b_state) = setup(Limits::default());
        let (target, _, target_state) = setup(Limits::default());
        let mut bytes = BudgetVec::new(b.clone(), ResourceKind::Scratch);
        bytes.try_push(2u8).unwrap();
        let list = crate::frozen::mismatched_owner_for_test(&a, bytes).unwrap();
        let header_error = Error::native(-17);
        let payload_error = Error::native(-23);
        let target_error = Error::native(-31);
        a.record(header_error);
        b.record(payload_error);
        if target_failed {
            target.record(target_error);
        }
        let expected = if target_failed {
            target_error
        } else {
            header_error
        };
        assert_eq!(list.validate_policy(Some(&target)), Err(expected));
        assert_eq!(a.failure(), Some(header_error));
        assert_eq!(b.failure(), Some(payload_error));
        assert_eq!(target.failure(), Some(expected));
        assert_eq!(target_state.reserves.load(Ordering::Relaxed), 0);
        drop(list);
        assert_eq!(a_state.live.load(Ordering::Relaxed), 0);
        assert_eq!(b_state.live.load(Ordering::Relaxed), 0);
    }
}

#[test]
fn frozen_paid_clone_has_no_allocation_and_keeps_the_last_readers_fees_live() {
    let _isolation = isolate_observer();
    let (resources, _, state) = setup(Limits::default());
    let mut values = BudgetVec::new(resources.clone(), ResourceKind::Scratch);
    values.try_extend_from_slice(&[1.0f32, 2.0, 3.0]).unwrap();
    let original = FrozenList::try_freeze(&resources, values).unwrap();
    let paid = state.live.load(Ordering::Relaxed);
    let calls = state.reserves.load(Ordering::Relaxed);
    let before = ALLOCS.load(Ordering::Relaxed);
    ENABLED.store(true, Ordering::Relaxed);
    let other = original.clone();
    ENABLED.store(false, Ordering::Relaxed);
    assert_eq!(ALLOCS.load(Ordering::Relaxed), before);
    assert_eq!(state.reserves.load(Ordering::Relaxed), calls);
    assert!(original.belongs_to(&resources));
    assert_eq!(original.as_ptr(), other.as_ptr());
    assert_eq!(original.retained_unpaid_capacity(), 0);
    drop(original);
    assert_eq!(state.live.load(Ordering::Relaxed), paid);
    assert_eq!(other.as_slice(), &[1.0, 2.0, 3.0]);
    drop(other);
    assert_eq!(state.live.load(Ordering::Relaxed), 0);
    resources.validate_retired().unwrap();
}

#[test]
fn frozen_lists_keep_ieee_equality_and_compatibility_clone_semantics() {
    let _isolation = isolate_observer();
    let nans = FrozenList::from_static(&[f32::NAN]);
    assert_ne!(nans, nans.clone());
    assert_eq!(
        FrozenList::from_static(&[-0.0f32]),
        FrozenList::from_static(&[0.0f32])
    );
    let plain = FrozenList::from_compat(vec![1.0f32, 2.0]);
    let other = plain.clone();
    assert_eq!(plain, other);
    assert_ne!(plain.as_ptr(), other.as_ptr());
    assert!(plain.retained_unpaid_capacity() >= 2);
    assert!(nans.is_static());
}

#[test]
fn frozen_list_rejects_another_request_before_allocating_a_shared_header() {
    let _isolation = isolate_observer();
    let (first, _, first_state) = setup(Limits::default());
    let (second, _, second_state) = setup(Limits::default());
    let mut values = BudgetVec::new(first.clone(), ResourceKind::Scratch);
    values.try_push(7u32).unwrap();
    let error = FrozenList::try_freeze(&second, values).unwrap_err();
    assert_eq!(error, Error::new(FailureKind::Invariant, LimitKind::None));
    assert_eq!(second_state.reserves.load(Ordering::Relaxed), 0);
    assert_eq!(first_state.live.load(Ordering::Relaxed), 0);
    first.validate_retired().unwrap();
    second.validate_retired().unwrap();
}

#[test]
fn child_admission_is_exact_and_bound_to_the_real_request_owner() {
    let _isolation = isolate_observer();
    let (resources, fault, state) = setup(Limits::default());
    let same = resources.clone();
    let other = ResourcesRef::new_prepaid(TestBackend(state.clone()), fault, Limits::default());
    assert!(resources.is_same_request(&same));
    assert!(!resources.is_same_request(&other));
    let admission = resources.admit_child_encoded(17).unwrap();
    assert!(admission.matches(&same, 17));
    assert!(!admission.matches(&other, 17));
    assert!(!admission.matches(&resources, 18));
    let bytes = BudgetVec::<u8>::new(resources.clone(), ResourceKind::Scratch);
    assert!(bytes.belongs_to(&same));
    assert!(!bytes.belongs_to(&other));
    assert_eq!(resources.statistics().children, 1);
    assert_eq!(resources.statistics().child_encoded_bytes, 17);
    assert_eq!(resources.statistics().depth, 0);
    drop((bytes, admission, same, other));
    resources.validate_retired().unwrap();
}

#[test]
fn admitted_raster_does_not_use_svg_depth_and_admission_never_refunds_totals() {
    let _isolation = isolate_observer();
    let mut limits = Limits::default();
    limits.children = 2;
    limits.child_encoded_bytes = 8;
    limits.svg_depth = 2;
    let (resources, _, _) = setup(limits);
    let a = resources.admit_child_encoded(4).unwrap();
    let b = resources.admit_child_encoded(4).unwrap();
    assert_eq!(resources.statistics().children, 2);
    assert_eq!(resources.statistics().child_encoded_bytes, 8);
    assert_eq!(resources.statistics().depth, 0);
    drop((a, b));
    let a = resources.enter_svg_depth().unwrap();
    let b = resources.enter_svg_depth().unwrap();
    assert_eq!(resources.statistics().depth, 2);
    let error = resources.enter_svg_depth().err().unwrap();
    assert_eq!(error, Error::new(FailureKind::Quota, LimitKind::SvgDepth));
    drop((a, b));
    assert_eq!(resources.statistics().depth, 0);
    assert_eq!(resources.statistics().children, 2);
    assert_eq!(resources.statistics().child_encoded_bytes, 8);
    resources.validate_retired().unwrap();
}
use std::{
    alloc::{GlobalAlloc, System},
    sync::{
        Mutex,
        atomic::{AtomicBool, AtomicUsize},
    },
    vec,
};

// Every test that can allocate or change fault/observer globals participates.
// Keep the shared fault fixture serialized, while the tests that explicitly
// spawn worker threads still exercise their original concurrent operations.
static TEST_OBSERVER_LOCK: Mutex<()> = Mutex::new(());
#[path = "bucket_allocator_tests.rs"]
mod bucket_allocator_tests;
mod codec;
#[path = "scene_tests.rs"]
mod scene_tests;
std::thread_local! {
    // Const TLS has no allocated state or destructor. The allocator only reads
    // it, excluding libtest and waiting test threads from this test's counters.
    static OBSERVER_TEST_THREAD: core::cell::Cell<bool> = const { core::cell::Cell::new(false) };
}
fn observing_test_thread() -> bool {
    OBSERVER_TEST_THREAD
        .try_with(core::cell::Cell::get)
        .unwrap_or(false)
}
struct ObserverIsolation {
    _lock: std::sync::MutexGuard<'static, ()>,
}
fn isolate_observer() -> ObserverIsolation {
    let guard = TEST_OBSERVER_LOCK
        .lock()
        .unwrap_or_else(|error| error.into_inner());
    OBSERVER_TEST_THREAD.with(|enabled| enabled.set(true));
    ObserverIsolation { _lock: guard }
}
impl Drop for ObserverIsolation {
    fn drop(&mut self) {
        // Also run after an assertion unwind, before unlocking the next test.
        ENABLED.store(false, Ordering::Relaxed);
        TRACK_METADATA_ORDER.store(false, Ordering::Relaxed);
        TRACE_BYTES.store(0, Ordering::Relaxed);
        TRACE_PTR.store(0, Ordering::Relaxed);
        WATCH_PTR.store(0, Ordering::Relaxed);
        FAIL_NEXT_GROWTH.store(false, Ordering::Relaxed);
        OBSERVER_TEST_THREAD.with(|enabled| enabled.set(false));
    }
}

// Observation only. Always forward the exact operation to System; no allocation,
// refusal, panic, formatted logging or simulated OOM in the allocator.
struct Observer;
static ENABLED: AtomicBool = AtomicBool::new(false);
static LAST_ALLOC_PTR: AtomicUsize = AtomicUsize::new(0);
static LAST_ALLOC_SIZE: AtomicUsize = AtomicUsize::new(0);
static LAST_ALLOC_ALIGN: AtomicUsize = AtomicUsize::new(0);
static WATCH_PTR: AtomicUsize = AtomicUsize::new(0);
static WATCH_FREED: AtomicBool = AtomicBool::new(false);
static ALLOCS: AtomicUsize = AtomicUsize::new(0);
static FREES: AtomicUsize = AtomicUsize::new(0);
static LIVE: AtomicUsize = AtomicUsize::new(0);
static PEAK: AtomicUsize = AtomicUsize::new(0);
static MAX: AtomicUsize = AtomicUsize::new(0);
static TRACK_METADATA_ORDER: AtomicBool = AtomicBool::new(false);
static ORDER: AtomicUsize = AtomicUsize::new(0);
static OWNER_DROP: AtomicUsize = AtomicUsize::new(0);
static LAST_PHYSICAL_FREE: AtomicUsize = AtomicUsize::new(0);
static OUTER_REFUND: AtomicUsize = AtomicUsize::new(0);
static OUTER_PAID: AtomicUsize = AtomicUsize::new(0);
// A single exact Layout is watched at a time, with no observer allocation.
// All tests hold ObserverIsolation; no runner thread can change these records.
static TRACE_BYTES: AtomicUsize = AtomicUsize::new(0);
static TRACE_PTR: AtomicUsize = AtomicUsize::new(0);
static TRACE_PAID: AtomicUsize = AtomicUsize::new(0);
static TRACE_SEQUENCE: AtomicUsize = AtomicUsize::new(0);
static TRACE_RESERVED: AtomicUsize = AtomicUsize::new(0);
static TRACE_ALLOCATED: AtomicUsize = AtomicUsize::new(0);
static TRACE_ADMITTED: AtomicUsize = AtomicUsize::new(0);
static TRACE_FREED: AtomicUsize = AtomicUsize::new(0);
static TRACE_RELEASED: AtomicUsize = AtomicUsize::new(0);
static TRACE_BAD: AtomicBool = AtomicBool::new(false);
fn trace_allocated(ptr: *mut u8, layout: Layout) {
    if layout.size() == TRACE_BYTES.load(Ordering::Relaxed) && !ptr.is_null() {
        if TRACE_PAID.load(Ordering::Relaxed) != layout.size()
            || TRACE_RESERVED.load(Ordering::Relaxed) == 0
        {
            TRACE_BAD.store(true, Ordering::Relaxed);
        }
        TRACE_PTR.store(ptr as usize, Ordering::Relaxed);
        TRACE_ALLOCATED.store(
            TRACE_SEQUENCE.fetch_add(1, Ordering::Relaxed) + 1,
            Ordering::Relaxed,
        );
    }
}
unsafe impl GlobalAlloc for Observer {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        let ptr = unsafe { System.alloc(layout) };
        if !observing_test_thread() {
            return ptr;
        }
        trace_allocated(ptr, layout);
        if ENABLED.load(Ordering::Relaxed) && !ptr.is_null() {
            LAST_ALLOC_PTR.store(ptr as usize, Ordering::Relaxed);
            LAST_ALLOC_SIZE.store(layout.size(), Ordering::Relaxed);
            LAST_ALLOC_ALIGN.store(layout.align(), Ordering::Relaxed);
            ALLOCS.fetch_add(1, Ordering::Relaxed);
            let live = LIVE.fetch_add(layout.size(), Ordering::Relaxed) + layout.size();
            PEAK.fetch_max(live, Ordering::Relaxed);
            MAX.fetch_max(layout.size(), Ordering::Relaxed);
        }
        ptr
    }
    unsafe fn alloc_zeroed(&self, layout: Layout) -> *mut u8 {
        let ptr = unsafe { System.alloc_zeroed(layout) };
        if !observing_test_thread() {
            return ptr;
        }
        trace_allocated(ptr, layout);
        if ENABLED.load(Ordering::Relaxed) && !ptr.is_null() {
            LAST_ALLOC_PTR.store(ptr as usize, Ordering::Relaxed);
            LAST_ALLOC_SIZE.store(layout.size(), Ordering::Relaxed);
            LAST_ALLOC_ALIGN.store(layout.align(), Ordering::Relaxed);
            ALLOCS.fetch_add(1, Ordering::Relaxed);
            let live = LIVE.fetch_add(layout.size(), Ordering::Relaxed) + layout.size();
            PEAK.fetch_max(live, Ordering::Relaxed);
            MAX.fetch_max(layout.size(), Ordering::Relaxed);
        }
        ptr
    }
    unsafe fn realloc(&self, ptr: *mut u8, layout: Layout, size: usize) -> *mut u8 {
        let next = unsafe { System.realloc(ptr, layout, size) };
        if !observing_test_thread() {
            return next;
        }
        if ENABLED.load(Ordering::Relaxed) && !next.is_null() {
            ALLOCS.fetch_add(1, Ordering::Relaxed);
            FREES.fetch_add(1, Ordering::Relaxed);
            let live = LIVE.fetch_add(size, Ordering::Relaxed) + size;
            PEAK.fetch_max(live, Ordering::Relaxed);
            LIVE.fetch_sub(layout.size(), Ordering::Relaxed);
            MAX.fetch_max(size, Ordering::Relaxed);
        }
        next
    }
    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        let observing = observing_test_thread();
        let traced = observing && ptr as usize == TRACE_PTR.load(Ordering::Relaxed);
        if traced && TRACE_PAID.load(Ordering::Relaxed) != layout.size() {
            TRACE_BAD.store(true, Ordering::Relaxed);
        }
        unsafe {
            System.dealloc(ptr, layout);
        }
        if !observing {
            return;
        }
        if traced {
            TRACE_FREED.store(
                TRACE_SEQUENCE.fetch_add(1, Ordering::Relaxed) + 1,
                Ordering::Relaxed,
            );
            TRACE_PTR.store(0, Ordering::Relaxed);
        }
        if TRACK_METADATA_ORDER.load(Ordering::Relaxed) {
            LAST_PHYSICAL_FREE.store(ORDER.fetch_add(1, Ordering::Relaxed) + 1, Ordering::Relaxed);
        }
        if ENABLED.load(Ordering::Relaxed) {
            FREES.fetch_add(1, Ordering::Relaxed);
            LIVE.fetch_sub(layout.size(), Ordering::Relaxed);
        }
        if ptr as usize == WATCH_PTR.load(Ordering::Relaxed) {
            WATCH_FREED.store(true, Ordering::Release);
        }
    }
}
#[global_allocator]
static OBSERVER: Observer = Observer;

#[derive(Default)]
struct State {
    next: AtomicU64,
    live: AtomicU64,
    peak: AtomicU64,
    reserves: AtomicU64,
    releases: AtomicU64,
    admissions: AtomicU64,
    fail_on_admission: AtomicU64,
    cancel_on_admission: AtomicBool,
    failed_release: AtomicBool,
    panic_release_ticket: AtomicU64,
    failure: AtomicU64,
    checkpoint_calls: AtomicU64,
    fail_on_checkpoint: AtomicU64,
    watched_ticket: AtomicU64,
    physical_free_before_release: AtomicBool,
    entries: Mutex<Vec<(u64, usize)>>,
}
struct TestBackend(Arc<State>);
impl Drop for TestBackend {
    fn drop(&mut self) {
        if TRACK_METADATA_ORDER.load(Ordering::Relaxed) {
            OWNER_DROP.store(ORDER.fetch_add(1, Ordering::Relaxed) + 1, Ordering::Relaxed);
        }
    }
}
struct OuterEnvelope;
impl Drop for OuterEnvelope {
    fn drop(&mut self) {
        assert_eq!(LIVE.load(Ordering::Relaxed), 0);
        OUTER_PAID.store(0, Ordering::Relaxed);
        OUTER_REFUND.store(ORDER.fetch_add(1, Ordering::Relaxed) + 1, Ordering::Relaxed);
    }
}
impl Backend for TestBackend {
    fn reserve(&self, _: ResourceKind, bytes: usize) -> Result<Ticket, Error> {
        let failure = self.0.failure.load(Ordering::Relaxed);
        if failure != 0 {
            return Err(Error::native(failure as u32 as i32));
        }
        let ticket = self.0.next.fetch_add(1, Ordering::Relaxed) + 1;
        self.0.reserves.fetch_add(1, Ordering::Relaxed);
        let live = self.0.live.fetch_add(bytes as u64, Ordering::Relaxed) + bytes as u64;
        self.0.peak.fetch_max(live, Ordering::Relaxed);
        self.0.entries.lock().unwrap().push((ticket, bytes));
        if bytes == TRACE_BYTES.load(Ordering::Relaxed) {
            TRACE_PAID.store(bytes, Ordering::Relaxed);
            TRACE_RESERVED.store(
                TRACE_SEQUENCE.fetch_add(1, Ordering::Relaxed) + 1,
                Ordering::Relaxed,
            );
        }
        Ticket::new(1, ticket as u32, 1)
    }
    fn allocation_committed(&self, ticket: &Ticket, bytes: usize) -> Result<(), Error> {
        assert_eq!(ticket.account_nonce(), 1);
        assert_eq!(ticket.generation(), 1);
        assert!(
            self.0
                .entries
                .lock()
                .unwrap()
                .iter()
                .any(|entry| *entry == (u64::from(ticket.slot()), bytes))
        );
        let call = self.0.admissions.fetch_add(1, Ordering::Relaxed) + 1;
        if bytes == TRACE_BYTES.load(Ordering::Relaxed) {
            assert_ne!(TRACE_PTR.load(Ordering::Relaxed), 0);
            assert_eq!(TRACE_PAID.load(Ordering::Relaxed), bytes);
            assert_ne!(TRACE_ALLOCATED.load(Ordering::Relaxed), 0);
            TRACE_ADMITTED.store(
                TRACE_SEQUENCE.fetch_add(1, Ordering::Relaxed) + 1,
                Ordering::Relaxed,
            );
        }
        if self.0.fail_on_admission.load(Ordering::Relaxed) == call {
            return Err(if self.0.cancel_on_admission.load(Ordering::Relaxed) {
                Error::new(FailureKind::Cancelled, LimitKind::Work)
            } else {
                Error::native(-88)
            });
        }
        Ok(())
    }
    fn release_after_free(&self, ticket: Ticket) -> Result<(), Error> {
        if self.0.panic_release_ticket.load(Ordering::Relaxed) == u64::from(ticket.slot()) {
            self.0.panic_release_ticket.store(0, Ordering::Relaxed);
            panic!("intentional backend release fixture");
        }
        if self.0.failed_release.load(Ordering::Relaxed) {
            return Err(Error::native(-77));
        }
        if u64::from(ticket.slot()) == self.0.watched_ticket.load(Ordering::Relaxed) {
            self.0
                .physical_free_before_release
                .store(WATCH_FREED.load(Ordering::Acquire), Ordering::Release);
        }
        let mut entries = self.0.entries.lock().unwrap();
        let index = entries
            .iter()
            .position(|(id, _)| *id == u64::from(ticket.slot()))
            .unwrap();
        let (_, bytes) = entries.swap_remove(index);
        if bytes == TRACE_BYTES.load(Ordering::Relaxed) {
            assert_ne!(TRACE_FREED.load(Ordering::Relaxed), 0);
            TRACE_RELEASED.store(
                TRACE_SEQUENCE.fetch_add(1, Ordering::Relaxed) + 1,
                Ordering::Relaxed,
            );
            TRACE_PAID.store(0, Ordering::Relaxed);
        }
        self.0.live.fetch_sub(bytes as u64, Ordering::Relaxed);
        self.0.releases.fetch_add(1, Ordering::Relaxed);
        Ok(())
    }
    fn checkpoint(&self, _: u64) -> Result<(), Error> {
        let call = self.0.checkpoint_calls.fetch_add(1, Ordering::Relaxed) + 1;
        if self.0.fail_on_checkpoint.load(Ordering::Relaxed) == call {
            return Err(Error::new(FailureKind::Cancelled, LimitKind::Work));
        }
        let failure = self.0.failure.load(Ordering::Relaxed);
        if failure == 0 {
            Ok(())
        } else {
            Err(Error::native(failure as u32 as i32))
        }
    }
}
fn setup(limits: Limits) -> (ResourcesRef, FirstFailureRef, Arc<State>) {
    let state = Arc::new(State {
        entries: Mutex::new(Vec::with_capacity(128)),
        ..State::default()
    });
    let fault = FirstFailureRef::new_prepaid();
    let resources = ResourcesRef::new_prepaid(TestBackend(state.clone()), fault.clone(), limits);
    (resources, fault, state)
}

#[test]
fn packed_slot_never_replaces_earlier_semantic_or_hresult() {
    let _isolation = isolate_observer();
    let slot = FirstFailureSlot::new();
    let first = Error::new(FailureKind::Decode, LimitKind::XmlNodes);
    assert_eq!(slot.record(first), first);
    assert_eq!(slot.record(Error::native(i32::MIN)), first);
    assert_eq!(slot.get(), Some(first));
    for hr in [i32::MIN, -1, 0, i32::MAX] {
        let slot = FirstFailureSlot::new();
        let first = Error::native(hr);
        assert_eq!(slot.record(first), first);
        assert_eq!(
            slot.record(Error::new(FailureKind::FontReplay, LimitKind::None)),
            first
        );
        assert_eq!(slot.get(), Some(first));
    }
}

#[test]
fn packed_slot_concurrent_writers_publish_only_a_complete_record() {
    let _isolation = isolate_observer();
    let slot = Arc::new(FirstFailureSlot::new());
    std::thread::scope(|scope| {
        for hr in [-11, -22, -33, -44] {
            let slot = slot.clone();
            scope.spawn(move || {
                for _ in 0..1000 {
                    slot.record(Error::native(hr));
                }
            });
        }
    });
    let first = slot.get().unwrap();
    assert_eq!(first.kind, FailureKind::Native);
    assert!([-11, -22, -33, -44].contains(&first.native_hr.unwrap()));
    assert_eq!(first.limit, LimitKind::None);
}

#[test]
fn growth_prepays_old_plus_new_and_refunds_after_physical_free() {
    let _isolation = isolate_observer();
    let (resources, fault, state) = setup(Limits::default());
    let mut values = BudgetVec::<u64>::new(resources.clone(), ResourceKind::Scratch);
    for i in 0..4 {
        values.try_push(i).unwrap();
    }
    assert_eq!(values.capacity(), 4);
    WATCH_PTR.store(values.as_ptr() as usize, Ordering::Relaxed);
    WATCH_FREED.store(false, Ordering::Relaxed);
    state.watched_ticket.store(1, Ordering::Relaxed);
    values.try_push(4).unwrap();
    assert_eq!(values.as_slice(), &[0, 1, 2, 3, 4]);
    assert_eq!(state.peak.load(Ordering::Relaxed), 32 + 64);
    assert_eq!(state.live.load(Ordering::Relaxed), 64);
    assert!(state.physical_free_before_release.load(Ordering::Acquire));
    drop(values);
    assert_eq!(state.live.load(Ordering::Relaxed), 0);
    resources.validate_retired().unwrap();
    drop(resources);
    fault.validate_unique().unwrap();
}

#[test]
fn refusal_keeps_old_buffer_and_first_native_hresult() {
    let _isolation = isolate_observer();
    let (resources, _, state) = setup(Limits::default());
    let mut values = BudgetVec::<u64>::new(resources.clone(), ResourceKind::Scratch);
    for i in 0..4 {
        values.try_push(i).unwrap();
    }
    let pointer = values.as_ptr();
    state.failure.store(0x8007000eu32 as u64, Ordering::Relaxed);
    let error = values.try_push(4).unwrap_err();
    assert_eq!(error.native_hr, Some(0x8007000eu32 as i32));
    assert_eq!(values.as_ptr(), pointer);
    assert_eq!(values.as_slice(), &[0, 1, 2, 3]);
    assert_eq!(state.live.load(Ordering::Relaxed), 32);
    assert_eq!(
        resources.record(Error::new(FailureKind::Decode, LimitKind::None)),
        error
    );
    drop(values);
    resources.validate_retired().unwrap();
}

#[test]
fn scratch_boundary_minus_exact_plus_and_fresh_after_failure() {
    let _isolation = isolate_observer();
    for bytes in [31, 32, 33] {
        let mut limits = Limits::default();
        limits.scratch_bytes = 32;
        let (resources, _, state) = setup(limits);
        let mut values = BudgetVec::<u8>::new(resources.clone(), ResourceKind::Scratch);
        let result = values.try_reserve_exact(bytes);
        assert_eq!(result.is_ok(), bytes <= 32);
        if bytes > 32 {
            assert_eq!(state.reserves.load(Ordering::Relaxed), 0);
        }
        drop(values);
        resources.validate_retired().unwrap();
    }
    let (resources, _, _) = setup(Limits::default());
    let mut values = BudgetVec::<u8>::new(resources.clone(), ResourceKind::Scratch);
    values.try_extend_from_slice(b"fresh").unwrap();
    drop(values);
    resources.validate_retired().unwrap();
}

#[test]
fn child_scope_shares_cumulative_limits_and_only_depth_retires() {
    let _isolation = isolate_observer();
    let mut limits = Limits::default();
    limits.children = 2;
    limits.svg_depth = 2;
    limits.child_encoded_bytes = 6;
    let (resources, _, _) = setup(limits);
    let one = resources.enter_child().unwrap();
    resources.add_child_encoded(3).unwrap();
    let two = resources.enter_child().unwrap();
    resources.add_child_encoded(3).unwrap();
    assert_eq!(resources.statistics().depth, 2);
    drop(two);
    drop(one);
    assert_eq!(resources.statistics().depth, 0);
    assert_eq!(resources.statistics().children, 2);
    assert_eq!(resources.statistics().child_encoded_bytes, 6);
    assert_eq!(
        resources.enter_child().err().unwrap().limit,
        LimitKind::Children
    );
    resources.validate_retired().unwrap();
}

#[test]
fn cancellation_and_release_failure_preserve_original_error_without_panic() {
    let _isolation = isolate_observer();
    let (resources, _, state) = setup(Limits::default());
    let lease = resources.reserve(ResourceKind::Scratch, 8).unwrap();
    state.failure.store(0x80004004u32 as u64, Ordering::Relaxed);
    let first = resources.checkpoint(1).unwrap_err();
    assert_eq!(first.native_hr, Some(0x80004004u32 as i32));
    state.failed_release.store(true, Ordering::Relaxed);
    drop(lease);
    assert_eq!(resources.failure(), Some(first));
    assert_eq!(resources.statistics().release_failures, 1);
    assert_eq!(resources.statistics().live_leases, 1);
    assert!(resources.validate_retired().is_err());
}

#[test]
fn checked_capacity_rejects_before_backend_or_allocation() {
    let _isolation = isolate_observer();
    let (resources, _, state) = setup(Limits::default());
    let mut values = BudgetVec::<u64>::new(resources.clone(), ResourceKind::Scratch);
    assert_eq!(
        values.try_reserve_exact(usize::MAX).unwrap_err().limit,
        LimitKind::Capacity
    );
    assert_eq!(state.reserves.load(Ordering::Relaxed), 0);
    assert_eq!(values.len(), 0);
    drop(values);
    resources.validate_retired().unwrap();
}

#[test]
fn actual_metadata_layouts_are_exact_and_physically_free_before_refund() {
    let _isolation = isolate_observer();
    // State and test harness capacities are outside the observed metadata scope.
    let state = Arc::new(State {
        entries: Mutex::new(vec![]),
        ..State::default()
    });
    ALLOCS.store(0, Ordering::Relaxed);
    FREES.store(0, Ordering::Relaxed);
    LIVE.store(0, Ordering::Relaxed);
    PEAK.store(0, Ordering::Relaxed);
    MAX.store(0, Ordering::Relaxed);
    ORDER.store(0, Ordering::Relaxed);
    OUTER_PAID.store(
        metadata_sizes::<TestBackend>().total_bytes(),
        Ordering::Relaxed,
    );
    let envelope = OuterEnvelope;
    TRACK_METADATA_ORDER.store(true, Ordering::SeqCst);
    ENABLED.store(true, Ordering::SeqCst);
    let fault = FirstFailureRef::new_prepaid();
    let resources =
        ResourcesRef::new_prepaid(TestBackend(state.clone()), fault.clone(), Limits::default());
    let actual = LIVE.load(Ordering::Relaxed);
    let count = ALLOCS.load(Ordering::Relaxed);
    resources.validate_retired().unwrap();
    drop(resources);
    fault.validate_unique().unwrap();
    drop(fault);
    let retired = LIVE.load(Ordering::Relaxed);
    let frees = FREES.load(Ordering::Relaxed);
    assert_ne!(OUTER_PAID.load(Ordering::Relaxed), 0);
    drop(envelope);
    let owner_drop = OWNER_DROP.load(Ordering::Relaxed);
    let physical_free = LAST_PHYSICAL_FREE.load(Ordering::Relaxed);
    let refund = OUTER_REFUND.load(Ordering::Relaxed);
    TRACK_METADATA_ORDER.store(false, Ordering::SeqCst);
    ENABLED.store(false, Ordering::SeqCst);
    // The modeled outer fee owner has now actually retired, after all three
    // physical metadata allocations. Native combined-envelope proof is separate.
    assert_eq!(count, 3);
    assert_eq!(frees, 3);
    assert_eq!(retired, 0);
    let envelope = metadata_sizes::<TestBackend>();
    assert_eq!(actual, envelope.total_bytes());
    assert_eq!(count, envelope.allocation_count);
    assert!(owner_drop < physical_free && physical_free < refund);
    assert_eq!(OUTER_PAID.load(Ordering::Relaxed), 0);
    std::println!(
        "metadata actualRequested={} envelope={} allocations={} ownerDrop={} lastPhysicalFree={} actualOuterFeeRefund={}",
        actual,
        envelope.total_bytes(),
        count,
        owner_drop,
        physical_free,
        refund
    );
}

#[test]
fn injected_preallocation_error_keeps_old_storage_and_rolls_pending_fee_back() {
    let _isolation = isolate_observer();
    let (resources, _, state) = setup(Limits::default());
    let mut values = BudgetVec::<u64>::new(resources.clone(), ResourceKind::Scratch);
    for i in 0..4 {
        values.try_push(i).unwrap();
    }
    let pointer = values.as_ptr();
    FAIL_NEXT_GROWTH.store(true, Ordering::Release);
    let error = values.try_push(4).unwrap_err();
    assert_eq!(error.kind, FailureKind::Allocation);
    assert_eq!(values.as_ptr(), pointer);
    assert_eq!(values.as_slice(), &[0, 1, 2, 3]);
    assert_eq!(state.peak.load(Ordering::Relaxed), 96);
    assert_eq!(state.live.load(Ordering::Relaxed), 32);
    assert_eq!(state.reserves.load(Ordering::Relaxed), 2);
    assert_eq!(state.releases.load(Ordering::Relaxed), 1);
    drop(values);
    resources.validate_retired().unwrap();
}

#[test]
fn nested_depth_and_encoded_boundaries_do_not_reset_with_guard_drop() {
    let _isolation = isolate_observer();
    for limit in [1, 2, 3] {
        let mut limits = Limits::default();
        limits.svg_depth = limit;
        let (resources, _, _) = setup(limits);
        let one = resources.enter_child().unwrap();
        let two = resources.enter_child();
        assert_eq!(two.is_ok(), limit >= 2);
        if limit < 2 {
            assert_eq!(resources.failure().unwrap().limit, LimitKind::SvgDepth);
        }
        drop(two);
        drop(one);
        resources.validate_retired().unwrap();
    }
    for limit in [5, 6, 7] {
        let mut limits = Limits::default();
        limits.child_encoded_bytes = limit;
        let (resources, _, _) = setup(limits);
        resources.add_child_encoded(3).unwrap();
        assert_eq!(resources.add_child_encoded(3).is_ok(), limit >= 6);
        if limit < 6 {
            assert_eq!(resources.failure().unwrap().limit, LimitKind::ChildEncoded);
        }
        resources.validate_retired().unwrap();
    }
}

#[test]
fn work_boundary_is_checked_before_backend_work() {
    let _isolation = isolate_observer();
    for limit in [255, 256, 257] {
        let mut limits = Limits::default();
        limits.work = limit;
        let (resources, _, _) = setup(limits);
        assert_eq!(resources.checkpoint(256).is_ok(), limit >= 256);
        assert_eq!(
            resources.statistics().work,
            if limit >= 256 { 256 } else { 0 }
        );
        resources.validate_retired().unwrap();
    }
}

#[test]
fn scan_credit_can_use_a_small_limit_and_the_final_work_remainder() {
    let _isolation = isolate_observer();
    for limit in [1, 65_535, 65_536, 65_537] {
        let (resources, _, state) = setup(Limits {
            work: limit,
            ..Limits::default()
        });
        assert_eq!(
            resources.scan_allowance().unwrap(),
            limit.min(65_536) as usize
        );
        resources.consume_scan_work(limit as usize).unwrap();
        assert_eq!(resources.statistics().work, limit);
        let calls = state.checkpoint_calls.load(Ordering::Relaxed);
        let error = Error::new(FailureKind::Quota, LimitKind::Work);
        assert_eq!(resources.consume_scan_work(1), Err(error));
        assert_eq!(resources.failure(), Some(error));
        assert_eq!(state.checkpoint_calls.load(Ordering::Relaxed), calls);
    }
    let (resources, _, _) = setup(Limits {
        work: 65_537,
        ..Limits::default()
    });
    resources.checkpoint(65_530).unwrap();
    assert_eq!(resources.scan_allowance().unwrap(), 7);
    resources.consume_scan_work(7).unwrap();
    assert_eq!(resources.statistics().work, 65_537);
}

#[test]
fn old_release_failure_aborts_growth_before_publishing_new_item() {
    let _isolation = isolate_observer();
    let (resources, _, state) = setup(Limits::default());
    let mut values = BudgetVec::<u64>::new(resources.clone(), ResourceKind::Scratch);
    for i in 0..4 {
        values.try_push(i).unwrap();
    }
    state.failed_release.store(true, Ordering::Release);
    assert_eq!(values.try_push(4).unwrap_err().native_hr, Some(-77));
    assert_eq!(values.as_slice(), &[0, 1, 2, 3]);
    assert_eq!(resources.statistics().release_failures, 1);
    drop(values);
    assert!(resources.validate_retired().is_err());
}

// Run this observer suite with --test-threads=1. The concurrency test below
// creates real threads inside its one observed ownership lifetime.
fn start_alloc_observation() {
    ALLOCS.store(0, Ordering::Relaxed);
    FREES.store(0, Ordering::Relaxed);
    LIVE.store(0, Ordering::Relaxed);
    PEAK.store(0, Ordering::Relaxed);
    MAX.store(0, Ordering::Relaxed);
    ENABLED.store(true, Ordering::Relaxed);
}
fn check_shared_layout<T>(value: T) {
    let (resources, fault, state) = setup(Limits::default());
    let layout = crate::paid::shared_layout::<T>();
    start_alloc_observation();
    let shared = PaidShared::try_new(&resources, ResourceKind::Scratch, value).unwrap();
    assert_eq!(ALLOCS.load(Ordering::Relaxed), 1);
    assert_eq!(LAST_ALLOC_SIZE.load(Ordering::Relaxed), layout.size());
    assert_eq!(LAST_ALLOC_ALIGN.load(Ordering::Relaxed), layout.align());
    assert_eq!(state.live.load(Ordering::Relaxed), layout.size() as u64);
    WATCH_PTR.store(LAST_ALLOC_PTR.load(Ordering::Relaxed), Ordering::Relaxed);
    WATCH_FREED.store(false, Ordering::Relaxed);
    state.watched_ticket.store(1, Ordering::Relaxed);
    let other = shared.clone();
    assert!(shared.ptr_eq(&other));
    assert_eq!(ALLOCS.load(Ordering::Relaxed), 1);
    drop(shared);
    assert_eq!(state.releases.load(Ordering::Relaxed), 0);
    drop(other);
    ENABLED.store(false, Ordering::Relaxed);
    assert_eq!(LIVE.load(Ordering::Relaxed), 0);
    assert!(state.physical_free_before_release.load(Ordering::Acquire));
    resources.validate_retired().unwrap();
    drop(resources);
    fault.validate_unique().unwrap();
}
#[test]
fn shared_real_arc_layout_and_refund_cover_zst_and_overalignment() {
    let _isolation = isolate_observer();
    #[repr(align(64))]
    struct Aligned([u8; 3]);
    check_shared_layout(());
    check_shared_layout(7u8);
    let aligned = Aligned([1, 2, 3]);
    assert_eq!(aligned.0, [1, 2, 3]);
    check_shared_layout(aligned);
}
#[test]
fn shared_header_quota_is_rejected_before_actual_allocation() {
    let _isolation = isolate_observer();
    let size = crate::paid::shared_layout::<()>();
    let (resources, _, state) = setup(Limits {
        scratch_bytes: size.size() as u64 - 1,
        ..Limits::default()
    });
    start_alloc_observation();
    let error = PaidShared::try_new(&resources, ResourceKind::Scratch, ()).unwrap_err();
    ENABLED.store(false, Ordering::Relaxed);
    assert_eq!(error.kind, FailureKind::Quota);
    assert_eq!(ALLOCS.load(Ordering::Relaxed), 0);
    assert_eq!(state.reserves.load(Ordering::Relaxed), 0);
    resources.validate_retired().unwrap();
}
#[test]
fn shared_encoded_payload_and_header_both_physically_free_before_refund() {
    let _isolation = isolate_observer();
    let (resources, _, state) = setup(Limits::default());
    start_alloc_observation();
    let mut buffer = BudgetVec::new(resources.clone(), ResourceKind::Scratch);
    buffer.try_extend_from_slice(b"encoded").unwrap();
    let shared = buffer.into_shared().unwrap();
    WATCH_PTR.store(LAST_ALLOC_PTR.load(Ordering::Relaxed), Ordering::Relaxed);
    WATCH_FREED.store(false, Ordering::Relaxed);
    state.watched_ticket.store(2, Ordering::Relaxed);
    let other = shared.clone();
    assert_eq!(shared.as_slice(), b"encoded");
    assert_eq!(ALLOCS.load(Ordering::Relaxed), 2);
    drop(shared);
    assert_eq!(state.releases.load(Ordering::Relaxed), 0);
    drop(other);
    ENABLED.store(false, Ordering::Relaxed);
    assert_eq!(LIVE.load(Ordering::Relaxed), 0);
    assert_eq!(state.releases.load(Ordering::Relaxed), 2);
    assert!(state.physical_free_before_release.load(Ordering::Acquire));
    resources.validate_retired().unwrap();
}
#[test]
fn shared_concurrent_last_drops_destroy_exactly_one_payload() {
    let _isolation = isolate_observer();
    struct Payload<'a>(&'a AtomicUsize);
    impl Drop for Payload<'_> {
        fn drop(&mut self) {
            self.0.fetch_add(1, Ordering::AcqRel);
        }
    }
    let (resources, _, state) = setup(Limits::default());
    let drops = AtomicUsize::new(0);
    let shared = PaidShared::try_new(&resources, ResourceKind::Scratch, Payload(&drops)).unwrap();
    std::thread::scope(|scope| {
        for _ in 0..16 {
            let owner = shared.clone();
            scope.spawn(move || drop(owner));
        }
        drop(shared);
    });
    assert_eq!(drops.load(Ordering::Acquire), 1);
    assert_eq!(state.reserves.load(Ordering::Relaxed), 1);
    assert_eq!(state.releases.load(Ordering::Relaxed), 1);
    resources.validate_retired().unwrap();
}
#[test]
fn shared_payload_unwind_still_refunds_after_arc_deallocation() {
    let _isolation = isolate_observer();
    struct PanicPayload;
    impl Drop for PanicPayload {
        fn drop(&mut self) {
            panic!("intentional destructor fixture");
        }
    }
    let (resources, _, state) = setup(Limits::default());
    start_alloc_observation();
    let shared = PaidShared::try_new(&resources, ResourceKind::Scratch, PanicPayload).unwrap();
    ENABLED.store(false, Ordering::Relaxed);
    WATCH_PTR.store(LAST_ALLOC_PTR.load(Ordering::Relaxed), Ordering::Relaxed);
    WATCH_FREED.store(false, Ordering::Relaxed);
    state.watched_ticket.store(1, Ordering::Relaxed);
    assert!(std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| drop(shared))).is_err());
    assert!(state.physical_free_before_release.load(Ordering::Acquire));
    assert_eq!(state.live.load(Ordering::Relaxed), 0);
    LIVE.store(0, Ordering::Relaxed);
    resources.validate_retired().unwrap();
}
#[test]
fn paid_utf8_preserves_unicode_and_shared_storage_identity() {
    let _isolation = isolate_observer();
    let (resources, _, state) = setup(Limits::default());
    let mut text = PaidUtf8::new(resources.clone(), ResourceKind::Scratch);
    for ch in "中Á👩‍💻".chars() {
        text.try_push(ch).unwrap();
    }
    assert_eq!(text.as_str(), "中Á👩‍💻");
    let shared = text.into_shared().unwrap();
    let other = shared.clone();
    assert_eq!(shared.as_str(), "中Á👩‍💻");
    assert!(shared.ptr_eq(&other));
    drop(shared);
    drop(other);
    assert_eq!(state.live.load(Ordering::Relaxed), 0);
    resources.validate_retired().unwrap();
}
#[test]
fn paid_utf8_cancellation_leaves_a_valid_partial_prefix() {
    let _isolation = isolate_observer();
    let source = "中".repeat(30000);
    let (resources, _, state) = setup(Limits::default());
    state.fail_on_checkpoint.store(2, Ordering::Relaxed);
    let mut text = PaidUtf8::new(resources.clone(), ResourceKind::Scratch);
    let error = text.try_push_str(&source).unwrap_err();
    assert_eq!(error.kind, FailureKind::Cancelled);
    assert_eq!(text.len(), 65535);
    assert_eq!(text.as_str(), &source[..text.len()]);
    drop(text);
    assert_eq!(state.live.load(Ordering::Relaxed), 0);
    resources.validate_retired().unwrap();
}
#[test]
fn zeroed_growth_checks_cancellation_within_initialization() {
    let _isolation = isolate_observer();
    let (resources, _, state) = setup(Limits::default());
    state.fail_on_checkpoint.store(2, Ordering::Relaxed);
    let mut bytes = BudgetVec::new(resources.clone(), ResourceKind::Scratch);
    let error = bytes.try_resize_zeroed(10000).unwrap_err();
    assert_eq!(error.kind, FailureKind::Cancelled);
    assert_eq!(bytes.len(), 4096);
    assert!(bytes.iter().all(|byte| *byte == 0));
    assert_eq!(state.live.load(Ordering::Relaxed), 10000);
    drop(bytes);
    resources.validate_retired().unwrap();
}

#[test]
fn shared_unique_unwrap_preserves_payload_and_refunds_only_freed_header() {
    let _isolation = isolate_observer();
    let (resources, _, state) = setup(Limits::default());
    start_alloc_observation();
    let mut bytes = BudgetVec::new(resources.clone(), ResourceKind::Scratch);
    bytes.try_extend_from_slice(b"pixels").unwrap();
    let shared = bytes.into_shared().unwrap();
    WATCH_PTR.store(LAST_ALLOC_PTR.load(Ordering::Relaxed), Ordering::Relaxed);
    WATCH_FREED.store(false, Ordering::Relaxed);
    state.watched_ticket.store(2, Ordering::Relaxed);
    let other = shared.clone();
    let shared = match shared.try_unwrap() {
        Err(owner) => owner,
        Ok(_) => panic!("shared payload escaped"),
    };
    assert_eq!(state.releases.load(Ordering::Relaxed), 0);
    drop(other);
    let bytes = shared.try_unwrap().unwrap();
    assert_eq!(bytes.as_slice(), b"pixels");
    assert_eq!(state.live.load(Ordering::Relaxed), 6);
    assert!(state.physical_free_before_release.load(Ordering::Acquire));
    assert_eq!(ALLOCS.load(Ordering::Relaxed), 2);
    drop(bytes);
    ENABLED.store(false, Ordering::Relaxed);
    assert_eq!(LIVE.load(Ordering::Relaxed), 0);
    resources.validate_retired().unwrap();
}

#[test]
fn moving_non_copy_values_in_batches_keeps_single_ownership() {
    let _isolation = isolate_observer();
    struct Value<'a> {
        id: usize,
        drops: &'a AtomicUsize,
    }
    impl Drop for Value<'_> {
        fn drop(&mut self) {
            self.drops.fetch_add(1, Ordering::Relaxed);
        }
    }
    let drops = AtomicUsize::new(0);
    let (resources, _, state) = setup(Limits::default());
    let mut values = BudgetVec::new(resources.clone(), ResourceKind::Scratch);
    for id in 0..8192 {
        values.try_push(Value { id, drops: &drops }).unwrap();
    }
    let old = values.as_ptr();
    WATCH_PTR.store(old as usize, Ordering::Relaxed);
    WATCH_FREED.store(false, Ordering::Relaxed);
    state
        .watched_ticket
        .store(state.next.load(Ordering::Relaxed), Ordering::Relaxed);
    values.try_reserve_exact(1).unwrap();
    assert_ne!(old, values.as_ptr());
    assert!(values.iter().enumerate().all(|(id, value)| id == value.id));
    assert_eq!(drops.load(Ordering::Relaxed), 0);
    assert!(state.physical_free_before_release.load(Ordering::Acquire));
    drop(values);
    assert_eq!(drops.load(Ordering::Relaxed), 8192);
    resources.validate_retired().unwrap();
}
#[test]
fn cancelled_batched_move_keeps_original_pointer_values_and_owners() {
    let _isolation = isolate_observer();
    struct Value<'a> {
        id: usize,
        drops: &'a AtomicUsize,
    }
    impl Drop for Value<'_> {
        fn drop(&mut self) {
            self.drops.fetch_add(1, Ordering::Relaxed);
        }
    }
    let drops = AtomicUsize::new(0);
    let (resources, _, state) = setup(Limits::default());
    let mut values = BudgetVec::new(resources.clone(), ResourceKind::Scratch);
    for id in 0..8192 {
        values.try_push(Value { id, drops: &drops }).unwrap();
    }
    let pointer = values.as_ptr();
    let capacity = values.capacity();
    let live = state.live.load(Ordering::Relaxed);
    let call = state.checkpoint_calls.load(Ordering::Relaxed);
    state.fail_on_checkpoint.store(call + 2, Ordering::Relaxed);
    let error = values.try_reserve_exact(1).unwrap_err();
    assert_eq!(error.kind, FailureKind::Cancelled);
    assert_eq!(values.as_ptr(), pointer);
    assert_eq!(values.capacity(), capacity);
    assert_eq!(values.len(), 8192);
    assert!(values.iter().enumerate().all(|(id, value)| id == value.id));
    assert_eq!(drops.load(Ordering::Relaxed), 0);
    assert_eq!(state.live.load(Ordering::Relaxed), live);
    drop(values);
    assert_eq!(drops.load(Ordering::Relaxed), 8192);
    resources.validate_retired().unwrap();
}
#[test]
fn release_unwind_never_refunds_a_new_buffer_still_owned_by_self() {
    let _isolation = isolate_observer();
    let (resources, _, state) = setup(Limits::default());
    let mut values = BudgetVec::<u64>::new(resources.clone(), ResourceKind::Scratch);
    for id in 0..4 {
        values.try_push(id).unwrap();
    }
    let old_ticket = state.next.load(Ordering::Relaxed);
    state
        .panic_release_ticket
        .store(old_ticket, Ordering::Relaxed);
    assert!(std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| values.try_push(4))).is_err());
    assert_eq!(values.as_slice(), &[0, 1, 2, 3]);
    assert_eq!(values.capacity(), 8);
    assert!(values.lease.is_some());
    assert_eq!(state.live.load(Ordering::Relaxed), 32 + 64);
    assert_eq!(resources.statistics().scratch_bytes, 32 + 64);
    let new_ticket = values
        .lease
        .as_ref()
        .unwrap()
        .ticket
        .as_ref()
        .unwrap()
        .slot() as u64;
    state.watched_ticket.store(new_ticket, Ordering::Relaxed);
    WATCH_PTR.store(values.as_ptr() as usize, Ordering::Relaxed);
    WATCH_FREED.store(false, Ordering::Relaxed);
    drop(values);
    assert!(state.physical_free_before_release.load(Ordering::Acquire));
    assert_eq!(state.live.load(Ordering::Relaxed), 32); // failed old callback never refunded
    assert!(resources.validate_retired().is_err());
}
#[test]
fn zero_sized_drop_values_remain_owned_without_a_heap_allocation() {
    let _isolation = isolate_observer();
    static DROPS: AtomicUsize = AtomicUsize::new(0);
    struct Empty;
    impl Drop for Empty {
        fn drop(&mut self) {
            DROPS.fetch_add(1, Ordering::Relaxed);
        }
    }
    DROPS.store(0, Ordering::Relaxed);
    let (resources, _, state) = setup(Limits::default());
    let mut values = BudgetVec::new(resources.clone(), ResourceKind::Scratch);
    for _ in 0..20 {
        values.try_push(Empty).unwrap();
    }
    assert_eq!(state.reserves.load(Ordering::Relaxed), 0);
    drop(values);
    assert_eq!(DROPS.load(Ordering::Relaxed), 20);
    assert_eq!(state.admissions.load(Ordering::Relaxed), 0);
    resources.validate_retired().unwrap();
}

#[test]
fn batched_move_preserves_overaligned_values_and_nested_paid_owners() {
    let _isolation = isolate_observer();
    #[repr(align(64))]
    struct Aligned(u64);
    let (resources, _, state) = setup(Limits::default());
    let mut aligned = BudgetVec::new(resources.clone(), ResourceKind::Scratch);
    for id in 0..2048 {
        aligned.try_push(Aligned(id)).unwrap();
    }
    aligned.try_reserve_exact(1).unwrap();
    assert_eq!((aligned.as_ptr() as usize) % 64, 0);
    assert!(
        aligned
            .iter()
            .enumerate()
            .all(|(id, value)| id as u64 == value.0)
    );
    drop(aligned);
    let mut owners = BudgetVec::new(resources.clone(), ResourceKind::Scratch);
    for id in 0u32..2048 {
        let mut bytes = BudgetVec::new(resources.clone(), ResourceKind::Scratch);
        bytes.try_extend_from_slice(&id.to_le_bytes()).unwrap();
        owners.try_push(bytes.into_shared().unwrap()).unwrap();
    }
    owners.try_reserve_exact(1).unwrap();
    assert!(
        owners
            .iter()
            .enumerate()
            .all(|(id, value)| value.as_slice() == (id as u32).to_le_bytes())
    );
    drop(owners);
    assert_eq!(state.live.load(Ordering::Relaxed), 0);
    resources.validate_retired().unwrap();
}

fn begin_allocation_trace(bytes: usize) {
    assert_ne!(bytes, 0);
    TRACE_BYTES.store(0, Ordering::Relaxed);
    for field in [
        &TRACE_PTR,
        &TRACE_PAID,
        &TRACE_SEQUENCE,
        &TRACE_RESERVED,
        &TRACE_ALLOCATED,
        &TRACE_ADMITTED,
        &TRACE_FREED,
        &TRACE_RELEASED,
    ] {
        field.store(0, Ordering::Relaxed);
    }
    TRACE_BAD.store(false, Ordering::Relaxed);
    TRACE_BYTES.store(bytes, Ordering::Relaxed);
}
fn finish_allocation_trace() {
    TRACE_BYTES.store(0, Ordering::Relaxed);
    let orders = [
        TRACE_RESERVED.load(Ordering::Relaxed),
        TRACE_ALLOCATED.load(Ordering::Relaxed),
        TRACE_ADMITTED.load(Ordering::Relaxed),
        TRACE_FREED.load(Ordering::Relaxed),
        TRACE_RELEASED.load(Ordering::Relaxed),
    ];
    assert_eq!(orders, [1, 2, 3, 4, 5]);
    assert!(!TRACE_BAD.load(Ordering::Relaxed));
    assert_eq!(TRACE_PAID.load(Ordering::Relaxed), 0);
    std::println!(
        "real System allocation order: reserve={} alloc={} admit={} free={} release={}",
        orders[0],
        orders[1],
        orders[2],
        orders[3],
        orders[4]
    );
}

#[test]
fn ticket_is_native_sixteen_bytes_and_keeps_all_identity_bits() {
    let _isolation = isolate_observer();
    assert_eq!(size_of::<Ticket>(), 16);
    assert_eq!(core::mem::offset_of!(Ticket, account_nonce), 0);
    assert_eq!(core::mem::offset_of!(Ticket, slot), 8);
    assert_eq!(core::mem::offset_of!(Ticket, generation), 12);
    let t = Ticket::new(0xFEDC_BA98_7654_3210, u32::MAX, u32::MAX).unwrap();
    assert_eq!(t.account_nonce(), 0xFEDC_BA98_7654_3210);
    assert_eq!(t.slot(), u32::MAX);
    assert_eq!(t.generation(), u32::MAX);
    assert!(Ticket::new(0, 1, 1).is_err());
    assert!(Ticket::new(1, 1, 0).is_err());
    assert_eq!(Ticket::new(1, 0, 1).unwrap().slot(), 0);
}

#[test]
fn real_vector_allocation_is_admitted_after_success_and_freed_before_release() {
    let _isolation = isolate_observer();
    let (resources, _, state) = setup(Limits::default());
    let mut values = BudgetVec::<u8>::new(resources.clone(), ResourceKind::Scratch);
    begin_allocation_trace(16_389);
    values.try_reserve_exact(16_389).unwrap();
    assert_eq!(state.admissions.load(Ordering::Relaxed), 1);
    assert_eq!(TRACE_FREED.load(Ordering::Relaxed), 0);
    drop(values);
    finish_allocation_trace();
    resources.validate_retired().unwrap();
}

#[test]
fn real_growth_admission_failure_frees_new_block_and_preserves_old_values() {
    let _isolation = isolate_observer();
    for cancelled in [false, true] {
        let (resources, _, state) = setup(Limits::default());
        let mut values = BudgetVec::<u8>::new(resources.clone(), ResourceKind::Scratch);
        values.try_extend_from_slice(&[7, 8, 9]).unwrap();
        let old_ptr = values.as_ptr();
        let old_capacity = values.capacity();
        let old_live = state.live.load(Ordering::Relaxed);
        state.fail_on_admission.store(2, Ordering::Relaxed);
        state
            .cancel_on_admission
            .store(cancelled, Ordering::Relaxed);
        begin_allocation_trace(16_392);
        let error = values.try_reserve_exact(16_389).unwrap_err();
        finish_allocation_trace();
        assert_eq!(
            error,
            if cancelled {
                Error::new(FailureKind::Cancelled, LimitKind::Work)
            } else {
                Error::native(-88)
            }
        );
        assert_eq!(values.as_ptr(), old_ptr);
        assert_eq!(values.capacity(), old_capacity);
        assert_eq!(values.as_slice(), &[7, 8, 9]);
        assert_eq!(state.live.load(Ordering::Relaxed), old_live);
        drop(values);
        resources.validate_retired().unwrap();
        assert_eq!(resources.failure(), Some(error));
    }
}

#[test]
fn real_shared_header_admission_and_last_reader_keep_physical_order() {
    let _isolation = isolate_observer();
    let (resources, _, state) = setup(Limits::default());
    begin_allocation_trace(crate::paid::shared_layout::<[u8; 16_389]>().size());
    let value = PaidShared::try_new(&resources, ResourceKind::Scratch, [3u8; 16_389]).unwrap();
    let clone = value.clone();
    drop(value);
    assert_eq!(TRACE_FREED.load(Ordering::Relaxed), 0);
    assert_ne!(state.live.load(Ordering::Relaxed), 0);
    assert_eq!(clone[0], 3);
    drop(clone);
    finish_allocation_trace();
    resources.validate_retired().unwrap();
}

#[test]
fn real_shared_header_admission_failure_frees_header_before_returning_fee() {
    let _isolation = isolate_observer();
    for cancelled in [false, true] {
        let (resources, _, state) = setup(Limits::default());
        state.fail_on_admission.store(1, Ordering::Relaxed);
        state
            .cancel_on_admission
            .store(cancelled, Ordering::Relaxed);
        begin_allocation_trace(crate::paid::shared_layout::<[u8; 16_389]>().size());
        let error =
            PaidShared::try_new(&resources, ResourceKind::Scratch, [3u8; 16_389]).unwrap_err();
        finish_allocation_trace();
        assert_eq!(
            error,
            if cancelled {
                Error::new(FailureKind::Cancelled, LimitKind::Work)
            } else {
                Error::native(-88)
            }
        );
        assert_eq!(state.live.load(Ordering::Relaxed), 0);
        resources.validate_retired().unwrap();
    }
}

#[test]
fn pending_failure_does_not_claim_allocation_admission() {
    let _isolation = isolate_observer();
    let (resources, _, state) = setup(Limits::default());
    let mut values = BudgetVec::<u8>::new(resources.clone(), ResourceKind::Scratch);
    FAIL_NEXT_GROWTH.store(true, Ordering::Release);
    assert_eq!(
        values.try_reserve_exact(16_389).unwrap_err().kind,
        FailureKind::Allocation
    );
    assert_eq!(state.reserves.load(Ordering::Relaxed), 1);
    assert_eq!(state.admissions.load(Ordering::Relaxed), 0);
    assert_eq!(state.releases.load(Ordering::Relaxed), 1);
    drop(values);
    resources.validate_retired().unwrap();
}

#[test]
fn duplicate_and_foreign_lease_admission_never_reach_backend() {
    let _isolation = isolate_observer();
    let (first, _, state) = setup(Limits::default());
    let (other, _, _) = setup(Limits::default());
    let mut pending = first.reserve(ResourceKind::Scratch, 8).unwrap();
    assert_eq!(
        other.allocation_committed(&mut pending).unwrap_err().kind,
        FailureKind::Invariant
    );
    assert_eq!(state.admissions.load(Ordering::Relaxed), 0);
    drop(pending);
    let mut values = BudgetVec::<u8>::new(first.clone(), ResourceKind::Scratch);
    values.try_push(1).unwrap();
    assert_eq!(
        first
            .allocation_committed(values.lease.as_mut().unwrap())
            .unwrap_err()
            .kind,
        FailureKind::Invariant
    );
    assert_eq!(state.admissions.load(Ordering::Relaxed), 1);
    drop(values);
    first.validate_retired().unwrap();
    other.validate_retired().unwrap();
}

#[test]
fn admission_failure_survives_cleanup_failure_and_retains_unreturned_fee() {
    let _isolation = isolate_observer();
    let (resources, _, state) = setup(Limits::default());
    let size = crate::paid::shared_layout::<[u8; 16_389]>().size();
    state.fail_on_admission.store(1, Ordering::Relaxed);
    state.failed_release.store(true, Ordering::Relaxed);
    begin_allocation_trace(size);
    let error = PaidShared::try_new(&resources, ResourceKind::Scratch, [3u8; 16_389]).unwrap_err();
    TRACE_BYTES.store(0, Ordering::Relaxed);
    assert_eq!(error, Error::native(-88));
    assert_eq!(resources.failure(), Some(error));
    assert_eq!(TRACE_FREED.load(Ordering::Relaxed), 4);
    assert_eq!(TRACE_RELEASED.load(Ordering::Relaxed), 0);
    assert!(!TRACE_BAD.load(Ordering::Relaxed));
    assert_eq!(state.live.load(Ordering::Relaxed), size as u64);
    assert_eq!(resources.statistics().live_leases, 1);
    assert_eq!(resources.statistics().release_failures, 1);
    assert!(resources.validate_retired().is_err());
    // This is an intentionally failed cleanup, not a successful refund.
    TRACE_PAID.store(0, Ordering::Relaxed);
}

#[test]
fn observer_excludes_competing_thread_allocations_with_the_same_layout() {
    let _isolation = isolate_observer();
    let (resources, _, _) = setup(Limits::default());
    let start = AtomicBool::new(false);
    let finished = AtomicBool::new(false);
    std::thread::scope(|scope| {
        scope.spawn(|| {
            while !start.load(Ordering::Acquire) {
                core::hint::spin_loop();
            }
            // Real allocations through this same global System-forwarding
            // observer, but from a thread outside its owning test scope.
            for _ in 0..128 {
                let mut unrelated = vec![0u8; 16_389];
                core::hint::black_box(unrelated.as_mut_slice());
                drop(unrelated);
            }
            finished.store(true, Ordering::Release);
        });
        begin_allocation_trace(16_389);
        start_alloc_observation();
        start.store(true, Ordering::Release);
        while !finished.load(Ordering::Acquire) {
            core::hint::spin_loop();
        }
        assert_eq!(ALLOCS.load(Ordering::Relaxed), 0);
        assert_eq!(FREES.load(Ordering::Relaxed), 0);
        assert_eq!(TRACE_ALLOCATED.load(Ordering::Relaxed), 0);
        assert!(!TRACE_BAD.load(Ordering::Relaxed));
        let mut values = BudgetVec::<u8>::new(resources.clone(), ResourceKind::Scratch);
        values.try_reserve_exact(16_389).unwrap();
        drop(values);
        ENABLED.store(false, Ordering::Relaxed);
        assert_eq!(ALLOCS.load(Ordering::Relaxed), 1);
        assert_eq!(FREES.load(Ordering::Relaxed), 1);
        assert_eq!(LIVE.load(Ordering::Relaxed), 0);
        finish_allocation_trace();
    });
    resources.validate_retired().unwrap();
}
