// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE file in the project root for license information.
// ---------------------------------------------------------------------------------------------
//! Private geometry-cache table tests; no renderer or product qualification.
use hashbrown::{ControlError, HashMap};
use markdown_svg_budget::{
    Backend, BucketAllocator, Error, FailureKind, FirstFailureRef, LimitKind, Limits, ResourceKind,
    ResourcesRef, Ticket, WorkControl,
};
use std::alloc::{GlobalAlloc, Layout, System};
use std::cell::Cell;
use std::hash::{BuildHasherDefault, Hash, Hasher};
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex, MutexGuard};

static LOCK: Mutex<()> = Mutex::new(());
static NEXT_BLOCK_BYTES: AtomicUsize = AtomicUsize::new(0);
static LAST_BLOCK: AtomicUsize = AtomicUsize::new(0);
static WATCH_BLOCK: AtomicUsize = AtomicUsize::new(0);
static WATCH_FREED: AtomicBool = AtomicBool::new(false);
thread_local! {
    static OBSERVING: Cell<bool> = const { Cell::new(false) };
}
struct Observation(MutexGuard<'static, ()>);
impl Observation {
    fn new() -> Self {
        let guard = LOCK.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
        NEXT_BLOCK_BYTES.store(0, Ordering::Relaxed);
        LAST_BLOCK.store(0, Ordering::Relaxed);
        WATCH_BLOCK.store(0, Ordering::Relaxed);
        WATCH_FREED.store(false, Ordering::Relaxed);
        OBSERVING.with(|observing| observing.set(true));
        Self(guard)
    }
}
impl Drop for Observation {
    fn drop(&mut self) {
        OBSERVING.with(|observing| observing.set(false));
        NEXT_BLOCK_BYTES.store(0, Ordering::Relaxed);
        WATCH_BLOCK.store(0, Ordering::Relaxed);
        let _ = &self.0;
    }
}
fn observing() -> bool {
    OBSERVING.try_with(Cell::get).unwrap_or(false)
}
// This observer never refuses, allocates, initializes payload or panics. It
// forwards System operations exactly and follows one real managed block only.
struct Observer;
unsafe impl GlobalAlloc for Observer {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        let pointer = unsafe { System.alloc(layout) };
        if observing()
            && !pointer.is_null()
            && NEXT_BLOCK_BYTES
                .compare_exchange(layout.size(), 0, Ordering::Relaxed, Ordering::Relaxed)
                .is_ok()
        {
            LAST_BLOCK.store(pointer as usize, Ordering::Relaxed);
        }
        pointer
    }
    unsafe fn alloc_zeroed(&self, layout: Layout) -> *mut u8 {
        let pointer = unsafe { System.alloc_zeroed(layout) };
        if observing()
            && !pointer.is_null()
            && NEXT_BLOCK_BYTES
                .compare_exchange(layout.size(), 0, Ordering::Relaxed, Ordering::Relaxed)
                .is_ok()
        {
            LAST_BLOCK.store(pointer as usize, Ordering::Relaxed);
        }
        pointer
    }
    unsafe fn realloc(&self, pointer: *mut u8, old: Layout, size: usize) -> *mut u8 {
        unsafe { System.realloc(pointer, old, size) }
    }
    unsafe fn dealloc(&self, pointer: *mut u8, layout: Layout) {
        let watched = observing() && WATCH_BLOCK.load(Ordering::Relaxed) == pointer as usize;
        unsafe { System.dealloc(pointer, layout) };
        if watched {
            WATCH_FREED.store(true, Ordering::Release);
        }
    }
}
#[global_allocator]
static OBSERVER: Observer = Observer;

#[derive(Default)]
struct Bank {
    reserves: AtomicUsize,
    admissions: AtomicUsize,
    releases: AtomicUsize,
    bytes: AtomicUsize,
    fail_reserve: AtomicUsize,
    fail_admission: AtomicUsize,
    fail_release_slot: AtomicUsize,
    watch_slot: AtomicUsize,
    saw_free_before_release: AtomicBool,
    entries: Mutex<Vec<(usize, usize)>>,
    release_log: Mutex<Vec<usize>>,
}
struct Host(Arc<Bank>);
impl Backend for Host {
    fn reserve(&self, _: ResourceKind, bytes: usize) -> Result<Ticket, Error> {
        let call = self.0.reserves.fetch_add(1, Ordering::Relaxed) + 1;
        if self.0.fail_reserve.load(Ordering::Relaxed) == call {
            return Err(Error::native(-901));
        }
        self.0.entries.lock().unwrap().push((call, bytes));
        self.0.bytes.fetch_add(bytes, Ordering::Relaxed);
        NEXT_BLOCK_BYTES.store(bytes, Ordering::Relaxed);
        Ticket::new(1, call as u32, 1)
    }
    fn allocation_committed(&self, ticket: &Ticket, bytes: usize) -> Result<(), Error> {
        assert_eq!(ticket.account_nonce(), 1);
        assert!(
            self.0
                .entries
                .lock()
                .unwrap()
                .contains(&(ticket.slot() as usize, bytes))
        );
        let call = self.0.admissions.fetch_add(1, Ordering::Relaxed) + 1;
        assert_eq!(NEXT_BLOCK_BYTES.load(Ordering::Relaxed), 0);
        assert_ne!(LAST_BLOCK.load(Ordering::Relaxed), 0);
        if self.0.watch_slot.load(Ordering::Relaxed) == ticket.slot() as usize {
            WATCH_BLOCK.store(LAST_BLOCK.load(Ordering::Relaxed), Ordering::Relaxed);
            WATCH_FREED.store(false, Ordering::Relaxed);
        }
        if self.0.fail_admission.load(Ordering::Relaxed) == call {
            return Err(Error::native(-902));
        }
        Ok(())
    }
    fn release_after_free(&self, ticket: Ticket) -> Result<(), Error> {
        let slot = ticket.slot() as usize;
        self.0.release_log.lock().unwrap().push(slot);
        if self.0.watch_slot.load(Ordering::Relaxed) == slot {
            self.0
                .saw_free_before_release
                .store(WATCH_FREED.load(Ordering::Acquire), Ordering::Relaxed);
        }
        if self.0.fail_release_slot.load(Ordering::Relaxed) == slot {
            return Err(Error::native(-903));
        }
        let mut entries = self.0.entries.lock().unwrap();
        let index = entries.iter().position(|entry| entry.0 == slot).unwrap();
        let (_, bytes) = entries.swap_remove(index);
        self.0.bytes.fetch_sub(bytes, Ordering::Relaxed);
        self.0.releases.fetch_add(1, Ordering::Relaxed);
        Ok(())
    }
    fn checkpoint(&self, _: u64) -> Result<(), Error> {
        Ok(())
    }
}
fn request() -> (ResourcesRef, Arc<Bank>) {
    request_with_limits(Limits::default())
}
fn request_with_limits(limits: Limits) -> (ResourcesRef, Arc<Bank>) {
    let bank = Arc::new(Bank {
        entries: Mutex::new(Vec::with_capacity(128)),
        release_log: Mutex::new(Vec::with_capacity(128)),
        ..Bank::default()
    });
    let resources =
        ResourcesRef::new_prepaid(Host(bank.clone()), FirstFailureRef::new_prepaid(), limits);
    (resources, bank)
}

#[derive(Default)]
struct DirectHasher(u64);
impl Hasher for DirectHasher {
    fn finish(&self) -> u64 {
        self.0
    }
    fn write(&mut self, bytes: &[u8]) {
        let mut value = 0u64;
        for (index, byte) in bytes.iter().take(8).enumerate() {
            value |= u64::from(*byte) << (index * 8);
        }
        self.0 = value;
    }
    fn write_u64(&mut self, value: u64) {
        self.0 = value;
    }
    fn write_usize(&mut self, value: usize) {
        self.0 = value as u64;
    }
}
type Seed = BuildHasherDefault<DirectHasher>;

#[derive(Clone)]
struct Dropped {
    id: usize,
    counts: Arc<Vec<AtomicUsize>>,
}
impl Drop for Dropped {
    fn drop(&mut self) {
        self.counts[self.id].fetch_add(1, Ordering::Relaxed);
    }
}
impl PartialEq for Dropped {
    fn eq(&self, other: &Self) -> bool {
        self.id == other.id
    }
}
impl Eq for Dropped {}
impl Hash for Dropped {
    fn hash<H: Hasher>(&self, state: &mut H) {
        state.write_usize(self.id);
    }
}
type PaidMap = HashMap<Dropped, Dropped, Seed, BucketAllocator>;
fn counters(count: usize) -> Arc<Vec<AtomicUsize>> {
    Arc::new((0..count).map(|_| AtomicUsize::new(0)).collect())
}
fn item(id: usize, counts: &Arc<Vec<AtomicUsize>>) -> Dropped {
    Dropped {
        id,
        counts: counts.clone(),
    }
}
fn map(resources: &ResourcesRef) -> PaidMap {
    HashMap::with_hasher_in(
        Seed::default(),
        BucketAllocator::new(resources.clone(), ResourceKind::Scratch),
    )
}

#[derive(Clone, Copy)]
enum Stop {
    Never,
    Checkpoint(usize),
    CtrlChunk(usize),
    Copy(usize),
    Group(usize),
}
struct Policy<'a> {
    resources: &'a ResourcesRef,
    stop: Stop,
    calls: usize,
    ctrl_chunks: usize,
    copies: usize,
    groups: usize,
    hashes: usize,
    equalities: usize,
    callbacks: usize,
}
impl<'a> Policy<'a> {
    fn new(resources: &'a ResourcesRef) -> Self {
        Self {
            resources,
            stop: Stop::Never,
            calls: 0,
            ctrl_chunks: 0,
            copies: 0,
            groups: 0,
            hashes: 0,
            equalities: 0,
            callbacks: 0,
        }
    }
    fn reset(&mut self, stop: Stop) {
        self.stop = stop;
        self.calls = 0;
        self.ctrl_chunks = 0;
        self.copies = 0;
        self.groups = 0;
        self.hashes = 0;
        self.equalities = 0;
        self.callbacks = 0;
    }
}
impl WorkControl for Policy<'_> {
    type Error = Error;
    fn checkpoint(&mut self, work: u64) -> Result<(), Error> {
        self.calls += 1;
        if work == 256 {
            self.ctrl_chunks += 1;
        }
        if work == core::mem::size_of::<(Dropped, Dropped)>() as u64 {
            self.copies += 1;
        }
        if work == 16 {
            self.groups += 1;
        }
        let fail = match self.stop {
            Stop::Never => false,
            Stop::Checkpoint(at) => self.calls == at,
            Stop::CtrlChunk(at) => work == 256 && self.ctrl_chunks == at,
            Stop::Copy(at) => {
                work == core::mem::size_of::<(Dropped, Dropped)>() as u64 && self.copies == at
            }
            Stop::Group(at) => work == 16 && self.groups == at,
        };
        if fail {
            return Err(self
                .resources
                .record(Error::new(FailureKind::Cancelled, LimitKind::Work)));
        }
        self.resources.checkpoint(work)
    }
}
fn equal(a: &Dropped, b: &Dropped, policy: &mut Policy<'_>) -> Result<bool, Error> {
    policy.equalities += 1;
    Ok(a.id == b.id)
}
fn existing_hash(key: &Dropped, policy: &mut Policy<'_>) -> Result<u64, Error> {
    policy.hashes += 1;
    Ok(key.id as u64)
}
fn insert(
    target: &mut PaidMap,
    id: usize,
    counts: &Arc<Vec<AtomicUsize>>,
    policy: &mut Policy<'_>,
) -> Result<Option<Dropped>, ControlError<Error>> {
    target.try_insert_with_control(
        id as u64,
        item(id, counts),
        item(id, counts),
        policy,
        equal,
        existing_hash,
    )
}
fn count_drops(counts: &Arc<Vec<AtomicUsize>>) -> Vec<usize> {
    counts
        .iter()
        .map(|count| count.load(Ordering::Relaxed))
        .collect()
}

#[test]
fn checked_and_ordinary_use_identical_bucket_order_capacity_and_value_semantics() {
    let _observation = Observation::new();
    let (resources, bank) = request();
    let counts = counters(320);
    let mut paid = map(&resources);
    let mut ordinary: HashMap<usize, usize, Seed> = HashMap::with_hasher(Seed::default());
    let mut policy = Policy::new(&resources);
    for id in (0..300).map(|index| (index * 127) % 300) {
        assert!(
            insert(&mut paid, id, &counts, &mut policy)
                .unwrap()
                .is_none()
        );
        ordinary.insert(id, id);
        assert_eq!(paid.capacity(), ordinary.capacity());
        assert_eq!(
            paid.iter()
                .map(|(key, value)| (key.id, value.id))
                .collect::<Vec<_>>(),
            ordinary
                .iter()
                .map(|(key, value)| (*key, *value))
                .collect::<Vec<_>>()
        );
    }
    let old = insert(&mut paid, 43, &counts, &mut policy)
        .unwrap()
        .unwrap();
    assert_eq!(old.id, 43);
    drop(old);
    assert_eq!(counts[43].load(Ordering::Relaxed), 2);
    for id in 0..300 {
        let found = paid
            .try_get_with_control(id as u64, &id, &mut policy, |query, key, policy| {
                policy.equalities += 1;
                Ok(*query == key.id)
            })
            .unwrap()
            .unwrap();
        assert_eq!(found.id, id);
    }
    let mut visited = Vec::with_capacity(300);
    paid.try_for_each_with_control(&mut policy, |key, value, policy| {
        policy.callbacks += 1;
        visited.push((key.id, value.id));
        Ok(())
    })
    .unwrap();
    assert_eq!(
        visited,
        paid.iter()
            .map(|(key, value)| (key.id, value.id))
            .collect::<Vec<_>>()
    );
    drop(paid);
    assert_eq!(counts[43].load(Ordering::Relaxed), 4);
    for id in (0..300).filter(|id| *id != 43) {
        assert_eq!(counts[id].load(Ordering::Relaxed), 2);
    }
    assert_eq!(bank.bytes.load(Ordering::Relaxed), 0);
    resources.validate_retired().unwrap();
}

#[test]
fn cancelled_ctrl_initialization_never_hashes_or_drops_source_entries() {
    let _observation = Observation::new();
    let (resources, bank) = request();
    let counts = counters(16);
    let mut target = map(&resources);
    let mut policy = Policy::new(&resources);
    for id in 0..12 {
        insert(&mut target, id, &counts, &mut policy).unwrap();
    }
    let bytes = bank.bytes.load(Ordering::Relaxed);
    let capacity = target.capacity();
    policy.reset(Stop::CtrlChunk(2));
    let result = target.try_reserve_with_control(800, &mut policy, existing_hash);
    assert!(matches!(
        result,
        Err(ControlError::Control(Error {
            kind: FailureKind::Cancelled,
            ..
        }))
    ));
    assert_eq!(policy.ctrl_chunks, 2);
    assert_eq!(policy.hashes, 0);
    assert_eq!(target.len(), 12);
    assert_eq!(target.capacity(), capacity);
    assert_eq!(bank.bytes.load(Ordering::Relaxed), bytes);
    assert!(count_drops(&counts).iter().all(|count| *count == 0));
    for (key, value) in &target {
        assert_eq!(key.id, value.id);
    }
    drop(target);
    assert!(count_drops(&counts)[..12].iter().all(|count| *count == 2));
    assert_eq!(bank.bytes.load(Ordering::Relaxed), 0);
    resources.validate_retired().unwrap();
}

#[test]
fn cancel_after_257_real_migration_copies_frees_raw_duplicates_without_double_drop() {
    let _observation = Observation::new();
    let (resources, bank) = request();
    let counts = counters(400);
    let mut target = map(&resources);
    let mut policy = Policy::new(&resources);
    for id in 0..400 {
        insert(&mut target, id, &counts, &mut policy).unwrap();
    }
    let bytes = bank.bytes.load(Ordering::Relaxed);
    let capacity = target.capacity();
    policy.reset(Stop::Copy(258));
    let result = target.try_reserve_with_control(capacity + 1, &mut policy, existing_hash);
    assert!(matches!(
        result,
        Err(ControlError::Control(Error {
            kind: FailureKind::Cancelled,
            ..
        }))
    ));
    assert_eq!(policy.copies, 258);
    assert_eq!(policy.hashes, 258);
    assert_eq!(target.capacity(), capacity);
    assert_eq!(bank.bytes.load(Ordering::Relaxed), bytes);
    assert!(count_drops(&counts).iter().all(|count| *count == 0));
    assert_eq!(target.len(), 400);
    for (key, value) in &target {
        assert_eq!(key.id, value.id);
    }
    drop(target);
    assert!(count_drops(&counts).iter().all(|count| *count == 2));
    resources.validate_retired().unwrap();
}

#[test]
fn sparse_controlled_iteration_stops_before_group_257_and_before_first_callback() {
    let _observation = Observation::new();
    let (resources, _) = request();
    let counts = counters(8200);
    let mut target = map(&resources);
    let mut policy = Policy::new(&resources);
    target
        .try_reserve_with_control(4000, &mut policy, existing_hash)
        .unwrap();
    insert(&mut target, 8000, &counts, &mut policy).unwrap();
    policy.reset(Stop::Group(257));
    let result = target.try_for_each_with_control(&mut policy, |_, _, policy| {
        policy.callbacks += 1;
        Ok(())
    });
    assert_eq!(
        result,
        Err(Error::new(FailureKind::Cancelled, LimitKind::Work))
    );
    assert_eq!(policy.groups, 257);
    assert_eq!(policy.callbacks, 0);
    assert_eq!(counts[8000].load(Ordering::Relaxed), 0);
    drop(target);
    assert_eq!(counts[8000].load(Ordering::Relaxed), 2);
    resources.validate_retired().unwrap();
}

#[test]
fn old_bucket_release_error_occurs_after_swap_and_keeps_new_table_valid() {
    let _observation = Observation::new();
    let (resources, bank) = request();
    bank.watch_slot.store(1, Ordering::Relaxed);
    let counts = counters(40);
    let mut target = map(&resources);
    let mut policy = Policy::new(&resources);
    target
        .try_reserve_with_control(40, &mut policy, existing_hash)
        .unwrap();
    for id in 0..40 {
        insert(&mut target, id, &counts, &mut policy).unwrap();
    }
    let old_capacity = target.capacity();
    bank.fail_release_slot.store(1, Ordering::Relaxed);
    let result = target.try_reserve_with_control(200, &mut policy, existing_hash);
    assert!(matches!(result, Err(ControlError::Control(error)) if error == Error::native(-903)));
    assert!(bank.saw_free_before_release.load(Ordering::Relaxed));
    assert!(WATCH_FREED.load(Ordering::Acquire));
    assert!(target.capacity() > old_capacity);
    assert_eq!(target.len(), 40);
    assert!(count_drops(&counts).iter().all(|count| *count == 0));
    for (key, value) in &target {
        assert_eq!(key.id, value.id);
    }
    drop(target);
    assert!(count_drops(&counts).iter().all(|count| *count == 2));
    assert_eq!(resources.statistics().release_failures, 1);
    assert_eq!(resources.statistics().live_leases, 1);
    assert_eq!(bank.entries.lock().unwrap().len(), 1);
    assert_eq!(bank.entries.lock().unwrap()[0].0, 1);
    assert_eq!(
        bank.release_log
            .lock()
            .unwrap()
            .iter()
            .filter(|slot| **slot == 1)
            .count(),
        1
    );
    assert_eq!(resources.validate_retired(), Err(Error::native(-903)));
}

fn allocation_workflow(
    fail_reserve: usize,
    fail_admission: usize,
) -> (usize, usize, Option<Error>) {
    let (resources, bank) = request();
    bank.fail_reserve.store(fail_reserve, Ordering::Relaxed);
    bank.fail_admission.store(fail_admission, Ordering::Relaxed);
    let counts = counters(80);
    let mut target = map(&resources);
    let mut policy = Policy::new(&resources);
    let result = (|| {
        for id in 0..80 {
            insert(&mut target, id, &counts, &mut policy)?;
        }
        target.try_reserve_with_control(300, &mut policy, existing_hash)
    })();
    let first = resources.failure();
    assert_eq!(result.is_err(), first.is_some());
    drop(target);
    assert!(
        count_drops(&counts)
            .iter()
            .all(|count| *count == 0 || *count == 2)
    );
    assert_eq!(bank.bytes.load(Ordering::Relaxed), 0);
    assert_eq!(resources.statistics().live_leases, 0);
    resources.validate_retired().unwrap();
    (
        bank.reserves.load(Ordering::Relaxed),
        bank.admissions.load(Ordering::Relaxed),
        first,
    )
}

#[test]
fn every_real_reserve_and_admission_refusal_retires_once_and_keeps_native_error() {
    let _observation = Observation::new();
    let (reserves, admissions, first) = allocation_workflow(0, 0);
    assert!(reserves > 2 && admissions == reserves && first.is_none());
    for at in 1..=reserves {
        assert_eq!(allocation_workflow(at, 0).2, Some(Error::native(-901)));
    }
    for at in 1..=admissions {
        assert_eq!(allocation_workflow(0, at).2, Some(Error::native(-902)));
    }
}

#[test]
fn every_checkpoint_failure_before_or_after_live_insert_preserves_single_ownership() {
    let _observation = Observation::new();
    let (resources, _) = request();
    let counts = counters(8);
    let mut baseline = map(&resources);
    let mut policy = Policy::new(&resources);
    for id in 0..7 {
        insert(&mut baseline, id, &counts, &mut policy).unwrap();
    }
    policy.reset(Stop::Never);
    insert(&mut baseline, 7, &counts, &mut policy).unwrap();
    let calls = policy.calls;
    drop(baseline);
    resources.validate_retired().unwrap();
    assert!(calls > 20);
    for at in 1..=calls {
        let (resources, bank) = request();
        let counts = counters(8);
        let mut target = map(&resources);
        let mut policy = Policy::new(&resources);
        for id in 0..7 {
            insert(&mut target, id, &counts, &mut policy).unwrap();
        }
        policy.reset(Stop::Checkpoint(at));
        assert!(insert(&mut target, 7, &counts, &mut policy).is_err());
        assert_eq!(
            resources.failure(),
            Some(Error::new(FailureKind::Cancelled, LimitKind::Work))
        );
        let present = target.iter().any(|(key, _)| key.id == 7);
        assert_eq!(target.len(), if present { 8 } else { 7 });
        for (key, value) in &target {
            assert_eq!(key.id, value.id);
        }
        drop(target);
        assert!(count_drops(&counts).iter().all(|count| *count == 2));
        assert_eq!(bank.bytes.load(Ordering::Relaxed), 0);
        resources.validate_retired().unwrap();
    }
}

#[test]
fn ordinary_tombstones_mixed_with_checked_reserve_rebuild_safely_without_losing_owners() {
    let _observation = Observation::new();
    let (resources, bank) = request();
    let counts = counters(120);
    let mut target = map(&resources);
    let mut policy = Policy::new(&resources);
    target
        .try_reserve_with_control(100, &mut policy, existing_hash)
        .unwrap();
    for id in 0..100 {
        insert(&mut target, id, &counts, &mut policy).unwrap();
    }
    for id in 0..90 {
        // This ordinary deletion deliberately leaves the production monotone
        // facade. It tests the safe conservative mixed-use fallback only.
        let query = item(id, &counts);
        drop(target.remove_entry(&query).unwrap());
        drop(query);
    }
    let before = bank.reserves.load(Ordering::Relaxed);
    target
        .try_reserve_with_control(100, &mut policy, existing_hash)
        .unwrap();
    assert!(bank.reserves.load(Ordering::Relaxed) > before);
    assert_eq!(target.len(), 10);
    for (key, value) in &target {
        assert_eq!(key.id, value.id);
        assert!(key.id >= 90);
    }
    drop(target);
    for id in 0..90 {
        assert_eq!(counts[id].load(Ordering::Relaxed), 3);
    }
    for id in 90..100 {
        assert_eq!(counts[id].load(Ordering::Relaxed), 2);
    }
    assert_eq!(bank.bytes.load(Ordering::Relaxed), 0);
    resources.validate_retired().unwrap();
}

#[test]
fn colliding_lookup_stops_before_group_257_without_comparing_unvisited_keys() {
    let _observation = Observation::new();
    let (resources, _) = request();
    let counts = counters(4200);
    let mut target = map(&resources);
    let mut policy = Policy::new(&resources);
    target
        .try_reserve_with_control(4200, &mut policy, |_, _| Ok(0))
        .unwrap();
    for id in 0..4200 {
        // Fixed all-collision setup. No resize occurs after the explicit paid
        // allocation; the controlled operations still charge every probe/eq.
        target
            .try_insert_with_control(
                0,
                item(id, &counts),
                item(id, &counts),
                &mut policy,
                equal,
                |_, _| Ok(0),
            )
            .unwrap();
    }
    let mut visited = Vec::with_capacity(4200);
    policy.reset(Stop::Group(257));
    let result = target.try_get_with_control(0, &usize::MAX, &mut policy, |query, key, policy| {
        policy.equalities += 1;
        visited.push(key.id);
        Ok(*query == key.id)
    });
    assert!(
        matches!(result, Err(error) if error == Error::new(FailureKind::Cancelled, LimitKind::Work))
    );
    assert_eq!(policy.groups, 257);
    assert_eq!(policy.equalities, 4096);
    assert_eq!(visited, (0..4096).collect::<Vec<_>>());
    assert!(count_drops(&counts).iter().all(|count| *count == 0));
    drop(target);
    assert!(count_drops(&counts).iter().all(|count| *count == 2));
    resources.validate_retired().unwrap();
}

#[test]
fn controlled_clone_visit_failure_retires_partial_target_without_mutating_source() {
    let _observation = Observation::new();
    let (resources, bank) = request();
    let counts = counters(600);
    let mut source = map(&resources);
    let mut policy = Policy::new(&resources);
    for id in 0..600 {
        insert(&mut source, id, &counts, &mut policy).unwrap();
    }
    let bytes = bank.bytes.load(Ordering::Relaxed);
    let mut target = map(&resources);
    policy.reset(Stop::Never);
    let result = source.try_for_each_with_control(&mut policy, |key, value, policy| {
        policy.callbacks += 1;
        if policy.callbacks == 257 {
            return Err(policy
                .resources
                .record(Error::new(FailureKind::Cancelled, LimitKind::Work)));
        }
        target
            .try_insert_with_control(
                key.id as u64,
                key.clone(),
                value.clone(),
                policy,
                equal,
                existing_hash,
            )
            .map_err(|error| match error {
                ControlError::Control(error) => error,
                ControlError::Reserve(_) => policy.resources.failure().unwrap(),
            })?;
        Ok(())
    });
    assert_eq!(
        result,
        Err(Error::new(FailureKind::Cancelled, LimitKind::Work))
    );
    assert_eq!(policy.callbacks, 257);
    assert_eq!(target.len(), 256);
    assert_eq!(source.len(), 600);
    assert!(count_drops(&counts).iter().all(|count| *count == 0));
    drop(target);
    assert_eq!(bank.bytes.load(Ordering::Relaxed), bytes);
    for id in 0..256 {
        assert_eq!(counts[id].load(Ordering::Relaxed), 2);
    }
    for (key, value) in &source {
        assert_eq!(key.id, value.id);
    }
    drop(source);
    for id in 0..256 {
        assert_eq!(counts[id].load(Ordering::Relaxed), 4);
    }
    for id in 256..600 {
        assert_eq!(counts[id].load(Ordering::Relaxed), 2);
    }
    resources.validate_retired().unwrap();
}

#[test]
fn real_request_work_quota_rejects_ctrl_fill_without_turning_it_into_cancel_or_allocation() {
    let _observation = Observation::new();
    let before_work = {
        let (resources, _) = request();
        let counts = counters(40);
        let mut target = map(&resources);
        let mut policy = Policy::new(&resources);
        target
            .try_reserve_with_control(40, &mut policy, existing_hash)
            .unwrap();
        for id in 0..40 {
            insert(&mut target, id, &counts, &mut policy).unwrap();
        }
        let before = resources.statistics().work;
        target
            .try_reserve_with_control(200, &mut policy, existing_hash)
            .unwrap();
        assert!(resources.statistics().work > before + 64);
        drop(target);
        resources.validate_retired().unwrap();
        before
    };
    let mut limits = Limits::default();
    limits.work = before_work + 64;
    let (resources, bank) = request_with_limits(limits);
    let counts = counters(40);
    let mut target = map(&resources);
    let mut policy = Policy::new(&resources);
    target
        .try_reserve_with_control(40, &mut policy, existing_hash)
        .unwrap();
    for id in 0..40 {
        insert(&mut target, id, &counts, &mut policy).unwrap();
    }
    assert_eq!(resources.statistics().work, before_work);
    let bytes = bank.bytes.load(Ordering::Relaxed);
    let capacity = target.capacity();
    policy.reset(Stop::Never);
    let result = target.try_reserve_with_control(200, &mut policy, existing_hash);
    assert!(
        matches!(result, Err(ControlError::Control(error)) if error == Error::new(FailureKind::Quota, LimitKind::Work))
    );
    assert_eq!(policy.hashes, 0);
    assert_eq!(policy.ctrl_chunks, 1);
    assert_eq!(target.capacity(), capacity);
    assert_eq!(target.len(), 40);
    assert_eq!(bank.bytes.load(Ordering::Relaxed), bytes);
    assert_eq!(resources.statistics().work, before_work + 1);
    drop(target);
    assert!(count_drops(&counts).iter().all(|count| *count == 2));
    resources.validate_retired().unwrap();
}

#[test]
fn checked_capacity_overflow_preserves_entries_and_is_distinct_from_control_failure() {
    let _observation = Observation::new();
    let (resources, bank) = request();
    let counts = counters(1);
    let mut target = map(&resources);
    let mut policy = Policy::new(&resources);
    insert(&mut target, 0, &counts, &mut policy).unwrap();
    let reserves = bank.reserves.load(Ordering::Relaxed);
    let result = target.try_reserve_with_control(usize::MAX, &mut policy, existing_hash);
    assert!(matches!(
        result,
        Err(ControlError::Reserve(
            hashbrown::TryReserveError::CapacityOverflow
        ))
    ));
    assert_eq!(resources.failure(), None);
    assert_eq!(target.len(), 1);
    assert_eq!(bank.reserves.load(Ordering::Relaxed), reserves);
    assert!(count_drops(&counts).iter().all(|count| *count == 0));
    drop(target);
    assert!(count_drops(&counts).iter().all(|count| *count == 2));
    resources.validate_retired().unwrap();
}
