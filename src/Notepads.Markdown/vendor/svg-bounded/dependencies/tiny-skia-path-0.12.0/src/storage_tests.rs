// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE file in the project root for license information.
// ---------------------------------------------------------------------------------------------
use super::*;
use crate::{LineCap, LineJoin, Path, PathBuilder, PathStroker, Stroke, StrokeDash, Transform};
use markdown_svg_budget::{Backend, FirstFailureRef, Limits, Ticket};
use std::{
    alloc::{GlobalAlloc, Layout, System},
    cell::Cell,
    sync::{
        atomic::{AtomicBool, AtomicUsize, Ordering},
        Arc, Mutex,
    },
    vec,
    vec::Vec,
};

static ISOLATION: Mutex<()> = Mutex::new(());
static LAST_ALLOCATED_PTR: AtomicUsize = AtomicUsize::new(0);
static LAST_ALLOCATED_SIZE: AtomicUsize = AtomicUsize::new(0);
static LAST_ALLOCATED_ALIGN: AtomicUsize = AtomicUsize::new(0);
static WATCHED_PTR: AtomicUsize = AtomicUsize::new(0);
static WATCHED_FREE: AtomicBool = AtomicBool::new(false);
static COPY_ALLOCATION_SIZE: AtomicUsize = AtomicUsize::new(0);
static COPY_ALLOCATION_PTR: AtomicUsize = AtomicUsize::new(0);
static COPY_PREFIX_VALID: AtomicBool = AtomicBool::new(false);
static COPY_TAIL_UNTOUCHED: AtomicBool = AtomicBool::new(false);
std::thread_local! { static OBSERVE: Cell<bool> = const { Cell::new(false) }; }
struct Observer;
unsafe impl GlobalAlloc for Observer {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        let pointer = unsafe { System.alloc(layout) };
        let copy_size = COPY_ALLOCATION_SIZE.load(Ordering::Relaxed);
        if copy_size != 0
            && !pointer.is_null()
            && layout.size() == copy_size
            && COPY_ALLOCATION_PTR
                .compare_exchange(0, pointer as usize, Ordering::AcqRel, Ordering::Relaxed)
                .is_ok()
        {
            // Only the chosen test allocation is initialized as raw bytes.
            // Its Vec length is still zero; this does not create invalid enum
            // values. It lets deallocation observe the untouched Copy tail.
            unsafe {
                core::ptr::write_bytes(pointer, 0xA5, layout.size());
            }
        }
        if OBSERVE.try_with(Cell::get).unwrap_or(false) {
            LAST_ALLOCATED_PTR.store(pointer as usize, Ordering::Relaxed);
            LAST_ALLOCATED_SIZE.store(layout.size(), Ordering::Relaxed);
            LAST_ALLOCATED_ALIGN.store(layout.align(), Ordering::Relaxed);
        }
        pointer
    }
    unsafe fn dealloc(&self, pointer: *mut u8, layout: Layout) {
        let watched = pointer as usize == WATCHED_PTR.load(Ordering::Acquire);
        if pointer as usize == COPY_ALLOCATION_PTR.load(Ordering::Acquire) {
            // SAFETY: this particular allocation was fully initialized to A5
            // above. Read as u8 before System frees it, not as uninitialized T.
            let bytes = unsafe { core::slice::from_raw_parts(pointer, layout.size()) };
            COPY_PREFIX_VALID.store(
                bytes[0] == PathVerb::Move as u8
                    && bytes[1..256]
                        .iter()
                        .all(|byte| *byte == PathVerb::Line as u8),
                Ordering::Release,
            );
            COPY_TAIL_UNTOUCHED.store(
                bytes[256..].iter().all(|byte| *byte == 0xA5),
                Ordering::Release,
            );
        }
        unsafe {
            System.dealloc(pointer, layout);
        }
        if watched {
            WATCHED_FREE.store(true, Ordering::Release);
        }
    }
}
#[global_allocator]
static ALLOCATOR: Observer = Observer;

struct ObserveGuard;
impl ObserveGuard {
    fn new() -> Self {
        OBSERVE.with(|value| value.set(true));
        Self
    }
}
impl Drop for ObserveGuard {
    fn drop(&mut self) {
        OBSERVE.with(|value| value.set(false));
    }
}

#[derive(Clone, Copy, Debug)]
struct Entry {
    ticket: u32,
    bytes: usize,
    kind: ResourceKind,
    pointer: usize,
    alignment: usize,
}
struct State {
    entries: Mutex<Vec<Entry>>,
    reserves: AtomicUsize,
    commits: AtomicUsize,
    checkpoints: AtomicUsize,
    peak: AtomicUsize,
    fail_reserve: AtomicUsize,
    fail_commit: AtomicUsize,
    fail_checkpoint: AtomicUsize,
    copy_windows: AtomicUsize,
    fail_copy_window: AtomicUsize,
    checkpoint_error: Error,
    fail_scratch_release: AtomicBool,
    failed_ticket: AtomicUsize,
    watched_ticket: AtomicUsize,
}
impl State {
    fn new(error: Error) -> Self {
        Self {
            entries: Mutex::new(Vec::with_capacity(4096)),
            reserves: AtomicUsize::new(0),
            commits: AtomicUsize::new(0),
            checkpoints: AtomicUsize::new(0),
            peak: AtomicUsize::new(0),
            fail_reserve: AtomicUsize::new(0),
            fail_commit: AtomicUsize::new(0),
            fail_checkpoint: AtomicUsize::new(0),
            copy_windows: AtomicUsize::new(0),
            fail_copy_window: AtomicUsize::new(0),
            checkpoint_error: error,
            fail_scratch_release: AtomicBool::new(false),
            failed_ticket: AtomicUsize::new(0),
            watched_ticket: AtomicUsize::new(0),
        }
    }
    fn live(&self) -> usize {
        self.entries
            .lock()
            .unwrap()
            .iter()
            .map(|entry| entry.bytes)
            .sum()
    }
}
struct TestBackend(Arc<State>);
impl Backend for TestBackend {
    fn reserve(&self, kind: ResourceKind, bytes: usize) -> Result<Ticket> {
        let call = self.0.reserves.fetch_add(1, Ordering::Relaxed) + 1;
        if call == self.0.fail_reserve.load(Ordering::Relaxed) {
            return Err(Error::native(-180));
        }
        let mut entries = self.0.entries.lock().unwrap();
        assert!(
            entries.len() < entries.capacity(),
            "fixture bookkeeping must not grow during observation"
        );
        entries.push(Entry {
            ticket: call as u32,
            bytes,
            kind,
            pointer: 0,
            alignment: 0,
        });
        self.0.peak.fetch_max(
            entries.iter().map(|entry| entry.bytes).sum(),
            Ordering::Relaxed,
        );
        Ticket::new(1, call as u32, 1)
    }
    fn allocation_committed(&self, ticket: &Ticket, bytes: usize) -> Result<()> {
        let mut entries = self.0.entries.lock().unwrap();
        let entry = entries
            .iter_mut()
            .find(|entry| entry.ticket == ticket.slot())
            .unwrap();
        assert_eq!(bytes, entry.bytes);
        if OBSERVE.try_with(Cell::get).unwrap_or(false) {
            assert_eq!(
                LAST_ALLOCATED_SIZE.load(Ordering::Relaxed),
                bytes,
                "admission must match the real allocation layout"
            );
            entry.pointer = LAST_ALLOCATED_PTR.load(Ordering::Relaxed);
            entry.alignment = LAST_ALLOCATED_ALIGN.load(Ordering::Relaxed);
            assert_ne!(entry.pointer, 0);
            assert_eq!(entry.pointer % entry.alignment, 0);
        }
        let call = self.0.commits.fetch_add(1, Ordering::Relaxed) + 1;
        if call == self.0.fail_commit.load(Ordering::Relaxed) {
            return Err(Error::native(-181));
        }
        Ok(())
    }
    fn release_after_free(&self, ticket: Ticket) -> Result<()> {
        let mut entries = self.0.entries.lock().unwrap();
        let index = entries
            .iter()
            .position(|entry| entry.ticket == ticket.slot())
            .unwrap();
        if self.0.watched_ticket.load(Ordering::Relaxed) == ticket.slot() as usize {
            assert!(
                WATCHED_FREE.load(Ordering::Acquire),
                "lease returned before its real allocation freed"
            );
        }
        if entries[index].kind == ResourceKind::Scratch
            && self.0.fail_scratch_release.swap(false, Ordering::Relaxed)
        {
            self.0
                .failed_ticket
                .store(ticket.slot() as usize, Ordering::Relaxed);
            return Err(Error::native(-193));
        }
        entries.swap_remove(index);
        Ok(())
    }
    fn checkpoint(&self, work: u64) -> Result<()> {
        let call = self.0.checkpoints.fetch_add(1, Ordering::Relaxed) + 1;
        if call == self.0.fail_checkpoint.load(Ordering::Relaxed) {
            return Err(self.0.checkpoint_error);
        }
        if work == 256 {
            let window = self.0.copy_windows.fetch_add(1, Ordering::Relaxed) + 1;
            if window == self.0.fail_copy_window.load(Ordering::Relaxed) {
                return Err(self.0.checkpoint_error);
            }
        }
        Ok(())
    }
}
fn setup(error: Error) -> (ResourcesRef, Arc<State>) {
    let state = Arc::new(State::new(error));
    let resources = ResourcesRef::new_prepaid(
        TestBackend(state.clone()),
        FirstFailureRef::new_prepaid(),
        Limits::default(),
    );
    (resources, state)
}
fn error() -> Error {
    Error::new(FailureKind::Cancelled, LimitKind::Work)
}
fn retired(resources: &ResourcesRef, state: &State) {
    assert_eq!(state.live(), 0);
    resources.validate_retired().unwrap();
}
fn paid_curve(resources: &ResourcesRef) -> Result<Path> {
    let mut builder = PathBuilder::with_resources(resources, ResourceKind::Geometry);
    builder.try_move_to(-2.0, 1.0)?;
    builder.try_line_to(3.0, 7.0)?;
    builder.try_quad_to(9.0, -3.0, 12.0, 4.0)?;
    builder.try_cubic_to(5.0, 8.0, -9.0, 7.0, 3.0, 2.0)?;
    builder.try_close()?;
    Ok(builder.try_finish()?.unwrap())
}

#[test]
fn scene_policy_checks_each_real_geometry_owner() {
    let _lock = ISOLATION.lock().unwrap_or_else(|error| error.into_inner());
    for foreign in 0..4 {
        let (target, target_state) = setup(error());
        let (other, other_state) = setup(error());
        let payer = |field| if field == foreign { &other } else { &target };
        let mut verbs = BudgetVec::new(payer(2).clone(), ResourceKind::Geometry);
        verbs
            .try_extend_from_slice(&[PathVerb::Move, PathVerb::Line])
            .unwrap();
        let mut points = BudgetVec::new(payer(3).clone(), ResourceKind::Geometry);
        points
            .try_extend_from_slice(&[Point::from_xy(0.0, 0.0), Point::from_xy(1.0, 1.0)])
            .unwrap();
        let owner = PaidShared::try_new(
            payer(1),
            ResourceKind::Geometry,
            PathBuffers { verbs, points },
        )
        .unwrap();
        let storage = GeometryStorage::Paid {
            resources: payer(0).clone(),
            kind: ResourceKind::Geometry,
            owner: Some(owner),
        };
        if foreign != 0 {
            assert!(storage.resources().unwrap().is_same_request(&target));
        }
        let reserves = (
            target_state.reserves.load(Ordering::Relaxed),
            other_state.reserves.load(Ordering::Relaxed),
        );
        let invariant = Error::new(FailureKind::Invariant, LimitKind::None);
        assert_eq!(storage.validate_policy(Some(&target)), Err(invariant));
        assert_eq!(target.failure(), Some(invariant));
        assert_eq!(other.failure(), Some(invariant));
        assert_eq!(
            (
                target_state.reserves.load(Ordering::Relaxed),
                other_state.reserves.load(Ordering::Relaxed)
            ),
            reserves
        );
        drop(storage);
        assert_eq!(target_state.live(), 0);
        assert_eq!(other_state.live(), 0);
    }
}

#[test]
fn scene_policy_rejects_paid_demotion_and_plain_promotion() {
    let _lock = ISOLATION.lock().unwrap_or_else(|error| error.into_inner());
    let (r, state) = setup(error());
    let path = paid_curve(&r).unwrap();
    path.validate_policy(Some(&r)).unwrap();
    let invariant = Error::new(FailureKind::Invariant, LimitKind::None);
    assert_eq!(path.validate_policy(None), Err(invariant));
    assert_eq!(r.failure(), Some(invariant));
    drop(path);
    assert_eq!(state.live(), 0);
    let (target, state) = setup(error());
    let mut plain = PathBuilder::new();
    plain.move_to(0.0, 0.0);
    plain.line_to(1.0, 1.0);
    let plain = plain.finish().unwrap();
    plain.validate_policy(None).unwrap();
    assert_eq!(plain.validate_policy(Some(&target)), Err(invariant));
    assert_eq!(target.failure(), Some(invariant));
    assert_eq!(state.reserves.load(Ordering::Relaxed), 0);
}

#[test]
fn scene_policy_preserves_target_and_source_native_failures() {
    let _lock = ISOLATION.lock().unwrap_or_else(|error| error.into_inner());
    for target_failed in [false, true] {
        let (source, source_state) = setup(error());
        let (target, target_state) = setup(error());
        let path = paid_curve(&source).unwrap();
        let source_error = Error::native(-401);
        let target_error = Error::native(-402);
        source.record(source_error);
        if target_failed {
            target.record(target_error);
        }
        let expected = if target_failed {
            target_error
        } else {
            source_error
        };
        assert_eq!(path.validate_policy(Some(&target)), Err(expected));
        assert_eq!(source.failure(), Some(source_error));
        assert_eq!(target.failure(), Some(expected));
        assert_eq!(target_state.reserves.load(Ordering::Relaxed), 0);
        drop(path);
        assert_eq!(source_state.live(), 0);
    }
}

#[test]
fn scene_policy_validates_paid_empty_without_new_allocation() {
    let _lock = ISOLATION.lock().unwrap_or_else(|error| error.into_inner());
    let (r, state) = setup(error());
    let empty = GeometryStorage::with_resources(&r, ResourceKind::Geometry);
    empty.validate_policy(Some(&r)).unwrap();
    assert_eq!(state.reserves.load(Ordering::Relaxed), 0);
    assert_eq!(
        empty.validate_policy(None),
        Err(Error::new(FailureKind::Invariant, LimitKind::None))
    );
    assert_eq!(state.reserves.load(Ordering::Relaxed), 0);
}
fn assert_geometry(left: &Path, right: &Path) {
    assert_eq!(left.verbs(), right.verbs());
    assert_eq!(left.points().len(), right.points().len());
    for (left, right) in left.points().iter().zip(right.points()) {
        assert_eq!(
            (left.x.to_bits(), left.y.to_bits()),
            (right.x.to_bits(), right.y.to_bits())
        );
    }
    let bits = |rect: crate::Rect| {
        [
            rect.left().to_bits(),
            rect.top().to_bits(),
            rect.right().to_bits(),
            rect.bottom().to_bits(),
        ]
    };
    assert_eq!(bits(left.bounds()), bits(right.bounds()));
    assert_eq!(
        left.compute_tight_bounds().map(bits),
        right.compute_tight_bounds().map(bits)
    );
}

#[test]
fn paid_explicit_capacity_layouts_are_exact_including_zero_and_partial_zero() {
    let _lock = ISOLATION.lock().unwrap_or_else(|error| error.into_inner());
    #[repr(C)]
    struct BuffersNode {
        value: PathBuffers,
        fee: markdown_svg_budget::Lease,
    }
    #[repr(C, align(2))]
    struct ArcAllocation {
        strong: AtomicUsize,
        weak: AtomicUsize,
        value: BuffersNode,
    }
    let header = Layout::new::<ArcAllocation>();
    for kind in [ResourceKind::Geometry, ResourceKind::Scratch] {
        for (verbs, points) in [(3, 9), (1, 1), (0, 9), (3, 0), (0, 0)] {
            let (resources, state) = setup(error());
            let gate = DashGateScope::new();
            let observer = ObserveGuard::new();
            LAST_ALLOCATED_PTR.store(0, Ordering::Relaxed);
            LAST_ALLOCATED_SIZE.store(0, Ordering::Relaxed);
            let builder = PathBuilder::try_with_capacity(&resources, kind, verbs, points).unwrap();
            assert_eq!(
                gate.starts(),
                (1, 0),
                "the actual builder constructor is observed once"
            );
            assert!(builder.move_to_required);
            assert_eq!(builder.last_move_to_index, 0);
            assert!(builder.verbs().is_empty() && builder.points().is_empty());
            assert_eq!(builder.storage.kind(), Some(kind));
            assert!(builder
                .storage
                .resources()
                .unwrap()
                .is_same_request(&resources));
            if verbs == 0 && points == 0 {
                assert_eq!(LAST_ALLOCATED_PTR.load(Ordering::Relaxed), 0);
                assert_eq!(LAST_ALLOCATED_SIZE.load(Ordering::Relaxed), 0);
                assert!(matches!(
                    &builder.storage,
                    GeometryStorage::Paid { owner: None, .. }
                ));
                assert_eq!(state.reserves.load(Ordering::Relaxed), 0);
                assert_eq!(state.commits.load(Ordering::Relaxed), 0);
            } else {
                let GeometryStorage::Paid {
                    owner: Some(owner), ..
                } = &builder.storage
                else {
                    panic!("nonzero exact capacity has a paid owner")
                };
                std::println!(
                    "explicit-capacity kind={kind:?} requested={verbs}/{points} actual={}/{}",
                    owner.verbs.capacity(),
                    owner.points.capacity()
                );
                assert_eq!(
                    (owner.verbs.capacity(), owner.points.capacity()),
                    (verbs, points)
                );
                let mut entries = state.entries.lock().unwrap().clone();
                entries.sort_by_key(|entry| entry.ticket);
                assert_eq!(
                    entries.len(),
                    1 + usize::from(verbs != 0) + usize::from(points != 0)
                );
                assert!(entries
                    .iter()
                    .all(|entry| entry.kind == kind && entry.pointer != 0));
                assert_eq!(
                    (entries[0].bytes, entries[0].alignment),
                    (header.size(), header.align())
                );
                let mut at = 1;
                if verbs != 0 {
                    let layout = Layout::array::<PathVerb>(verbs).unwrap();
                    assert_eq!(
                        (entries[at].bytes, entries[at].alignment),
                        (layout.size(), layout.align())
                    );
                    assert_eq!(
                        entries[at].pointer,
                        owner.verbs.as_slice().as_ptr() as usize
                    );
                    at += 1;
                }
                if points != 0 {
                    let layout = Layout::array::<Point>(points).unwrap();
                    assert_eq!(
                        (entries[at].bytes, entries[at].alignment),
                        (layout.size(), layout.align())
                    );
                    assert_eq!(
                        entries[at].pointer,
                        owner.points.as_slice().as_ptr() as usize
                    );
                }
                assert_eq!(state.reserves.load(Ordering::Relaxed), entries.len());
                assert_eq!(state.commits.load(Ordering::Relaxed), entries.len());
                assert_eq!(resources.statistics().live_leases as usize, entries.len());
            }
            drop((observer, gate, builder));
            retired(&resources, &state);
        }
    }
}

#[test]
fn paid_explicit_capacity_reuses_exact_arrays_and_preserves_normal_prepare_growth() {
    let _lock = ISOLATION.lock().unwrap_or_else(|error| error.into_inner());
    let (resources, state) = setup(error());
    let mut paid =
        PathBuilder::try_with_capacity(&resources, ResourceKind::Geometry, 3, 9).unwrap();
    let mut ordinary = PathBuilder::with_capacity(3, 9);
    let baseline = state.reserves.load(Ordering::Relaxed);
    for builder in [&mut paid, &mut ordinary] {
        builder.try_move_to(1.0, 2.0).unwrap();
        builder.try_cubic_to(3.0, 4.0, 5.0, 6.0, 7.0, 8.0).unwrap();
        builder
            .try_cubic_to(9.0, 10.0, 11.0, 12.0, 13.0, 14.0)
            .unwrap();
    }
    assert_eq!(state.reserves.load(Ordering::Relaxed), baseline);
    let paid = paid.try_finish().unwrap().unwrap();
    let ordinary = ordinary.try_finish().unwrap().unwrap();
    assert_geometry(&paid, &ordinary);
    drop(paid);
    retired(&resources, &state);

    let (resources, state) = setup(error());
    let mut growing = PathBuilder::with_resources(&resources, ResourceKind::Geometry);
    growing.try_move_to(1.0, 2.0).unwrap();
    let GeometryStorage::Paid {
        owner: Some(owner), ..
    } = &growing.storage
    else {
        panic!("normal prepare still creates its shared owner")
    };
    assert_eq!((owner.verbs.capacity(), owner.points.capacity()), (4, 4));
    drop(growing);
    retired(&resources, &state);
}

#[test]
fn paid_explicit_capacity_reserve_commit_and_cleanup_failures_keep_typed_first_error() {
    let _lock = ISOLATION.lock().unwrap_or_else(|error| error.into_inner());
    let mut executed = 0;
    for (verbs, points) in [(3, 9), (0, 9), (3, 0)] {
        let owners = 1 + usize::from(verbs != 0) + usize::from(points != 0);
        for admission in [false, true] {
            for at in 1..=owners {
                let (resources, state) = setup(error());
                if admission {
                    state.fail_commit.store(at, Ordering::Relaxed);
                } else {
                    state.fail_reserve.store(at, Ordering::Relaxed);
                }
                let observer = ObserveGuard::new();
                let result = PathBuilder::try_with_capacity(
                    &resources,
                    ResourceKind::Scratch,
                    verbs,
                    points,
                );
                let expected = Error::native(if admission { -181 } else { -180 });
                assert_eq!(
                    result.unwrap_err(),
                    expected,
                    "{verbs}/{points} commit={admission} at={at}"
                );
                assert_eq!(resources.failure(), Some(expected));
                assert_eq!(resources.statistics().release_failures, 0);
                drop(observer);
                retired(&resources, &state);
                executed += 1;
            }
        }
    }
    for admission in [false, true] {
        let (resources, state) = setup(error());
        if admission {
            state.fail_commit.store(3, Ordering::Relaxed);
        } else {
            state.fail_reserve.store(3, Ordering::Relaxed);
        }
        state.fail_scratch_release.store(true, Ordering::Relaxed);
        let expected = Error::native(if admission { -181 } else { -180 });
        assert_eq!(
            PathBuilder::try_with_capacity(&resources, ResourceKind::Scratch, 3, 9).unwrap_err(),
            expected
        );
        assert_eq!(
            resources.failure(),
            Some(expected),
            "the original admission failure precedes cleanup"
        );
        assert_eq!(resources.statistics().release_failures, 1);
        assert_eq!(resources.statistics().live_leases, 1);
        assert_eq!(state.entries.lock().unwrap().len(), 1);
        assert_ne!(state.failed_ticket.load(Ordering::Relaxed), 0);
        assert!(resources.validate_retired().is_err());
        executed += 1;
    }
    std::println!("explicit-capacity-faults actual-cases={executed} fully-retired-cases=14 failed-refund-retained=2");
    assert_eq!(executed, 16);
}

#[test]
fn paid_layout_last_alias_and_free_before_refund_cover_both_arrays_and_header() {
    let _lock = ISOLATION.lock().unwrap_or_else(|error| error.into_inner());
    for watched in 0..3 {
        let (resources, state) = setup(error());
        let _observer = ObserveGuard::new();
        let mut builder =
            PathBuilder::try_with_capacity(&resources, ResourceKind::Geometry, 7, 129).unwrap();
        builder.try_move_to(-0.0, 0.0).unwrap();
        builder.try_line_to(3.0, 4.0).unwrap();
        let path = builder.try_finish().unwrap().unwrap();
        let mut entries = state.entries.lock().unwrap().clone();
        entries.sort_by_key(|entry| entry.ticket);
        assert_eq!(entries.len(), 3);
        assert_eq!(entries[1].bytes, 7 * core::mem::size_of::<PathVerb>());
        assert_eq!(entries[2].bytes, 129 * core::mem::size_of::<Point>());
        assert_eq!(entries[2].pointer, path.points().as_ptr() as usize);
        let entry = entries[watched];
        WATCHED_FREE.store(false, Ordering::Relaxed);
        WATCHED_PTR.store(entry.pointer, Ordering::Release);
        state
            .watched_ticket
            .store(entry.ticket as usize, Ordering::Relaxed);
        let baseline = state.live();
        let reserves = state.reserves.load(Ordering::Relaxed);
        let clone = path.clone();
        assert_eq!(clone.points().as_ptr(), path.points().as_ptr());
        assert_eq!(state.reserves.load(Ordering::Relaxed), reserves);
        drop(path);
        assert_eq!(state.live(), baseline);
        assert!(!WATCHED_FREE.load(Ordering::Relaxed));
        drop(clone);
        assert!(WATCHED_FREE.load(Ordering::Acquire));
        WATCHED_PTR.store(0, Ordering::Release);
        retired(&resources, &state);
    }
}

#[test]
fn paid_builder_clone_cow_clear_finish_and_role_copy_keep_value_and_real_fees() {
    let _lock = ISOLATION.lock().unwrap_or_else(|error| error.into_inner());
    let (resources, state) = setup(error());
    let mut builder = PathBuilder::with_resources(&resources, ResourceKind::Geometry);
    builder.try_move_to(0.0, 0.0).unwrap();
    builder.try_line_to(2.0, 3.0).unwrap();
    let mut other = builder.clone();
    assert_eq!(builder.points().as_ptr(), other.points().as_ptr());
    other.try_line_to(7.0, 8.0).unwrap();
    assert_ne!(builder.points().as_ptr(), other.points().as_ptr());
    assert_eq!(builder.points().len(), 2);
    let source = builder.try_finish().unwrap().unwrap();
    let path = other.try_finish().unwrap().unwrap();
    let same = path
        .try_copy_in(&resources, ResourceKind::Geometry)
        .unwrap();
    assert_eq!(same.points().as_ptr(), path.points().as_ptr());
    let copied = path.try_copy_in(&resources, ResourceKind::Scratch).unwrap();
    assert_ne!(copied.points().as_ptr(), path.points().as_ptr());
    assert_geometry(&path, &copied);
    assert_eq!(copied.retained_unpaid_capacity_bytes(), Some(0));
    let before_clear = state.live();
    let mut shared_empty = same.try_clear().unwrap();
    assert_eq!(shared_empty.storage.kind(), Some(ResourceKind::Geometry));
    assert_eq!(state.live(), before_clear);
    assert!(shared_empty.is_empty());
    shared_empty.try_move_to(19.0, 22.0).unwrap();
    shared_empty.try_line_to(23.0, 31.0).unwrap();
    let unique = source.try_clear().unwrap();
    assert!(unique.is_empty());
    let peak = state.peak.load(Ordering::Relaxed);
    assert!(peak >= before_clear);
    drop((path, copied, shared_empty, unique));
    retired(&resources, &state);
}

#[test]
fn path_controls_reject_other_request_before_copy_or_mutation() {
    let _lock = ISOLATION.lock().unwrap_or_else(|error| error.into_inner());
    let (first, first_state) = setup(error());
    let (second, second_state) = setup(error());
    let source = paid_curve(&first).unwrap();
    assert_eq!(
        source
            .try_copy_in(&second, ResourceKind::Geometry)
            .unwrap_err(),
        Error::new(FailureKind::Invariant, LimitKind::None)
    );
    assert_eq!(second_state.reserves.load(Ordering::Relaxed), 0);
    assert_eq!(first.failure(), None);
    drop(source);
    retired(&first, &first_state);
    retired(&second, &second_state);
}

#[test]
fn controlled_transform_stroke_all_cap_join_and_dash_fields_match_ordinary() {
    let _lock = ISOLATION.lock().unwrap_or_else(|error| error.into_inner());
    let (resources, state) = setup(error());
    let source = paid_curve(&resources).unwrap();
    let ordinary = {
        let mut builder = PathBuilder::new();
        builder.move_to(-2.0, 1.0);
        builder.line_to(3.0, 7.0);
        builder.quad_to(9.0, -3.0, 12.0, 4.0);
        builder.cubic_to(5.0, 8.0, -9.0, 7.0, 3.0, 2.0);
        builder.close();
        builder.finish().unwrap()
    };
    assert_geometry(&ordinary, &source);
    assert!(ordinary.retained_unpaid_capacity_bytes().unwrap() >= ordinary.points().len() * 8);
    for transform in [
        Transform::identity(),
        Transform::from_translate(-1.0, 3.0),
        Transform::from_scale(-2.0, 0.125),
        Transform::from_row(1.0, 0.25, -0.75, 3.0, 2.0, -5.0),
    ] {
        let old = ordinary.clone().transform(transform);
        let new = source
            .clone()
            .try_transform(transform, &resources, ResourceKind::Geometry)
            .unwrap();
        assert_eq!(old.is_some(), new.is_some());
        if let (Some(old), Some(new)) = (old, new) {
            assert_geometry(&old, &new);
        }
    }
    for cap in [LineCap::Butt, LineCap::Round, LineCap::Square] {
        for join in [
            LineJoin::Miter,
            LineJoin::MiterClip,
            LineJoin::Round,
            LineJoin::Bevel,
        ] {
            for width in [0.125, 2.0, 35.0] {
                let style = Stroke {
                    width,
                    line_cap: cap,
                    line_join: join,
                    ..Stroke::default()
                };
                let old = ordinary.stroke(&style, 1.0);
                let new = source
                    .try_stroke(&style, 1.0, &resources, ResourceKind::Geometry)
                    .unwrap();
                assert_eq!(old.is_some(), new.is_some());
                if let (Some(old), Some(new)) = (old, new) {
                    assert_geometry(&old, &new);
                }
            }
        }
    }
    for pattern in [
        vec![1.0, 2.0],
        vec![0.0, 2.0],
        vec![3.0, 0.0],
        vec![0.125, 0.75, 2.0, 3.0],
    ] {
        let dash = StrokeDash::new(pattern, -0.125).unwrap();
        let old = ordinary.dash(&dash, 1.0);
        let new = source
            .try_dash(&dash, 1.0, &resources, ResourceKind::Geometry)
            .unwrap();
        assert_eq!(old.is_some(), new.is_some());
        if let (Some(old), Some(new)) = (old, new) {
            assert_geometry(&old, &new);
        }
    }
    drop(source);
    retired(&resources, &state);
}

#[test]
fn reusable_paid_stroker_clone_has_no_allocation_and_keeps_cow_buffers_paid() {
    let _lock = ISOLATION.lock().unwrap_or_else(|error| error.into_inner());
    let (resources, state) = setup(error());
    let source = paid_curve(&resources).unwrap();
    let mut stroker = PathStroker::with_resources(&resources);
    let first = stroker
        .try_stroke(
            &source,
            &Stroke::default(),
            1.0,
            &resources,
            ResourceKind::Geometry,
        )
        .unwrap()
        .unwrap();
    let before = state.reserves.load(Ordering::Relaxed);
    let mut clone = stroker.clone();
    assert_eq!(state.reserves.load(Ordering::Relaxed), before);
    let second = clone
        .try_stroke(
            &source,
            &Stroke::default(),
            1.0,
            &resources,
            ResourceKind::Geometry,
        )
        .unwrap()
        .unwrap();
    assert_geometry(&first, &second);
    drop((first, second, source, stroker, clone));
    retired(&resources, &state);
}

#[derive(Clone, Copy, Debug)]
enum Scenario {
    Builder,
    Cow,
    Transform,
    Stroke,
    Dash,
}
fn scenario(which: Scenario, resources: &ResourcesRef) -> Result<Option<Path>> {
    let source = paid_curve(resources)?;
    match which {
        Scenario::Builder => {
            let mut builder = PathBuilder::with_resources(resources, ResourceKind::Geometry);
            builder.try_push_path(&source)?;
            for index in 0..17 {
                builder.try_cubic_to(1.0, -3.0, 5.0, 9.0, index as f32, 4.0)?;
            }
            builder.try_finish()
        }
        Scenario::Cow => {
            let clone = source.clone();
            let result = clone.try_transform(
                Transform::from_translate(3.0, 4.0),
                resources,
                ResourceKind::Geometry,
            );
            drop(source);
            resources.checkpoint(0)?;
            result
        }
        Scenario::Transform => source.try_transform(
            Transform::from_row(1.0, 0.25, 0.75, 2.0, 3.0, 4.0),
            resources,
            ResourceKind::Scratch,
        ),
        Scenario::Stroke => source.try_stroke(
            &Stroke {
                line_cap: LineCap::Round,
                line_join: LineJoin::Round,
                ..Stroke::default()
            },
            1.0,
            resources,
            ResourceKind::Geometry,
        ),
        Scenario::Dash => source.try_dash(
            &StrokeDash::new(vec![2.0, 3.0], 0.25).unwrap(),
            1.0,
            resources,
            ResourceKind::Geometry,
        ),
    }
}

#[test]
fn every_real_reserve_commit_and_checkpoint_failure_stops_and_retires_each_path_family() {
    let _lock = ISOLATION.lock().unwrap_or_else(|error| error.into_inner());
    for which in [
        Scenario::Builder,
        Scenario::Cow,
        Scenario::Transform,
        Scenario::Stroke,
        Scenario::Dash,
    ] {
        let (resources, state) = setup(error());
        drop(scenario(which, &resources).unwrap());
        let counts = [
            state.reserves.load(Ordering::Relaxed),
            state.commits.load(Ordering::Relaxed),
            state.checkpoints.load(Ordering::Relaxed),
        ];
        std::println!(
            "path-fault-family={which:?} reserve={} commit={} checkpoint={}",
            counts[0],
            counts[1],
            counts[2]
        );
        retired(&resources, &state);
        for (category, count) in counts.into_iter().enumerate() {
            for work_mode in 0..if category == 2 { 2 } else { 1 } {
                for at in 1..=count {
                    let control_error = if work_mode == 0 {
                        error()
                    } else {
                        Error::new(FailureKind::Quota, LimitKind::Work)
                    };
                    let (resources, state) = setup(control_error);
                    match category {
                        0 => state.fail_reserve.store(at, Ordering::Relaxed),
                        1 => state.fail_commit.store(at, Ordering::Relaxed),
                        _ => state.fail_checkpoint.store(at, Ordering::Relaxed),
                    }
                    let result = scenario(which, &resources);
                    let expected = match category {
                        0 => Error::native(-180),
                        1 => Error::native(-181),
                        _ => control_error,
                    };
                    assert_eq!(
                        result.unwrap_err(),
                        expected,
                        "{which:?} category={category} at={at}"
                    );
                    assert_eq!(resources.failure(), Some(expected));
                    let reached = match category {
                        0 => state.reserves.load(Ordering::Relaxed),
                        1 => state.commits.load(Ordering::Relaxed),
                        _ => state.checkpoints.load(Ordering::Relaxed),
                    };
                    assert_eq!(
                        reached, at,
                        "continued backend calls after the first failure"
                    );
                    retired(&resources, &state);
                }
            }
        }
    }
}

#[test]
fn work_failure_keeps_typed_quota_distinct_from_cancellation() {
    let _lock = ISOLATION.lock().unwrap_or_else(|error| error.into_inner());
    let expected = Error::new(FailureKind::Quota, LimitKind::Work);
    let (resources, state) = setup(expected);
    state.fail_checkpoint.store(17, Ordering::Relaxed);
    assert_eq!(
        scenario(Scenario::Stroke, &resources).unwrap_err(),
        expected
    );
    retired(&resources, &state);
}

#[test]
fn stroker_temporary_drop_failure_cannot_publish_successful_path() {
    let _lock = ISOLATION.lock().unwrap_or_else(|error| error.into_inner());
    let (resources, state) = setup(error());
    let mut builder = PathBuilder::with_resources(&resources, ResourceKind::Geometry);
    builder.try_move_to(0.0, 0.0).unwrap();
    builder.try_line_to(10.0, 0.0).unwrap();
    let source = builder.try_finish().unwrap().unwrap();
    state.fail_scratch_release.store(true, Ordering::Relaxed);
    let result = source.try_stroke(&Stroke::default(), 1.0, &resources, ResourceKind::Geometry);
    assert_eq!(result.unwrap_err(), Error::native(-193));
    assert_ne!(state.failed_ticket.load(Ordering::Relaxed), 0);
    drop(source);
    assert_eq!(resources.statistics().release_failures, 1);
    assert_eq!(resources.statistics().live_leases, 1);
    assert_eq!(state.entries.lock().unwrap().len(), 1);
    assert_eq!(resources.validate_retired(), Err(Error::native(-193)));
}

#[test]
fn empty_finish_cleanup_failure_is_typed_error_instead_of_successful_none() {
    let _lock = ISOLATION.lock().unwrap_or_else(|error| error.into_inner());
    let (resources, state) = setup(error());
    let mut builder = PathBuilder::with_resources(&resources, ResourceKind::Scratch);
    builder.try_move_to(2.0, 3.0).unwrap();
    state.fail_scratch_release.store(true, Ordering::Relaxed);
    assert_eq!(builder.try_finish().unwrap_err(), Error::native(-193));
    assert_eq!(resources.statistics().release_failures, 1);
    assert_eq!(state.entries.lock().unwrap().len(), 1);
}

#[test]
fn invalid_transform_cleanup_failure_is_typed_error_instead_of_successful_none() {
    let _lock = ISOLATION.lock().unwrap_or_else(|error| error.into_inner());
    let (resources, state) = setup(error());
    let source = paid_curve(&resources).unwrap();
    state.fail_scratch_release.store(true, Ordering::Relaxed);
    let result = source.try_transform(
        Transform::from_scale(f32::INFINITY, 1.0),
        &resources,
        ResourceKind::Scratch,
    );
    assert_eq!(result.unwrap_err(), Error::native(-193));
    assert_eq!(resources.statistics().release_failures, 1);
    assert_eq!(state.entries.lock().unwrap().len(), 1);
}

struct CopyWatch;
impl Drop for CopyWatch {
    fn drop(&mut self) {
        COPY_ALLOCATION_SIZE.store(0, Ordering::Release);
        COPY_ALLOCATION_PTR.store(0, Ordering::Release);
    }
}

#[test]
fn cow_second_256_verb_window_stops_before_copy_and_preserves_untouched_tail() {
    let _lock = ISOLATION.lock().unwrap_or_else(|error| error.into_inner());
    assert_eq!(core::mem::size_of::<PathVerb>(), 1);
    let (resources, state) = setup(error());
    let mut builder =
        PathBuilder::try_with_capacity(&resources, ResourceKind::Geometry, 1024, 1024).unwrap();
    builder.try_move_to(0.0, 0.0).unwrap();
    for index in 1..600 {
        builder
            .try_line_to(index as f32, (index % 11) as f32)
            .unwrap();
    }
    let source = builder.try_finish().unwrap().unwrap();
    let clone = source.clone();
    let original_pointer = source.points().as_ptr();
    state.copy_windows.store(0, Ordering::Relaxed);
    state.fail_copy_window.store(2, Ordering::Relaxed);
    COPY_PREFIX_VALID.store(false, Ordering::Relaxed);
    COPY_TAIL_UNTOUCHED.store(false, Ordering::Relaxed);
    COPY_ALLOCATION_PTR.store(0, Ordering::Relaxed);
    COPY_ALLOCATION_SIZE.store(1024, Ordering::Release);
    let watch = CopyWatch;
    let result = clone.try_transform(
        Transform::from_translate(3.0, 4.0),
        &resources,
        ResourceKind::Geometry,
    );
    assert_eq!(result.unwrap_err(), error());
    assert_eq!(state.copy_windows.load(Ordering::Relaxed), 2);
    assert!(
        COPY_PREFIX_VALID.load(Ordering::Acquire),
        "first paid window was not copied as predicted"
    );
    assert!(
        COPY_TAIL_UNTOUCHED.load(Ordering::Acquire),
        "copy crossed the rejected second window"
    );
    assert_eq!(source.points().as_ptr(), original_pointer);
    assert_eq!(source.points()[599], Point::from_xy(599.0, 5.0));
    drop(watch);
    drop(source);
    retired(&resources, &state);
}

// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE file in the project root for license information.
// ---------------------------------------------------------------------------------------------
// Appended to the private P test module for the directed review repair.
std::thread_local! {
    static DASH_GATE_STARTS: Cell<Option<(usize, usize)>> = const { Cell::new(None) };
}

pub(super) fn record_dash_builder_start() {
    DASH_GATE_STARTS.with(|counts| {
        if let Some((builders, contours)) = counts.get() {
            counts.set(Some((builders + 1, contours)));
        }
    });
}

pub(super) fn record_dash_contour_start() {
    DASH_GATE_STARTS.with(|counts| {
        if let Some((builders, contours)) = counts.get() {
            counts.set(Some((builders, contours + 1)));
        }
    });
}

struct DashGateScope;
impl DashGateScope {
    fn new() -> Self {
        DASH_GATE_STARTS.with(|counts| {
            assert_eq!(counts.get(), None);
            counts.set(Some((0, 0)));
        });
        Self
    }

    fn starts(&self) -> (usize, usize) {
        DASH_GATE_STARTS.with(Cell::get).unwrap()
    }
}
impl Drop for DashGateScope {
    fn drop(&mut self) {
        DASH_GATE_STARTS.with(|counts| counts.set(None));
    }
}

fn paid_dash(resources: &ResourcesRef) -> StrokeDash {
    let mut values = BudgetVec::new(resources.clone(), ResourceKind::Scratch);
    values.try_extend_from_slice(&[2.0f32, 3.0]).unwrap();
    let values = markdown_svg_budget::FrozenList::try_freeze(resources, values).unwrap();
    StrokeDash::try_new_frozen(values, 0.125, resources)
        .unwrap()
        .unwrap()
}

fn reject_foreign_paid_dash(checked: bool) {
    // Each entrypoint gets genuinely fresh requests. A successful explicit
    // rejection must never poison A before testing the old Option wrapper.
    let (first, first_state) = setup(error());
    let (second, second_state) = setup(error());
    let path = paid_curve(&first).unwrap();
    let dash = paid_dash(&second);
    let first_reserves = first_state.reserves.load(Ordering::Relaxed);
    let second_reserves = second_state.reserves.load(Ordering::Relaxed);
    let checkpoints = first_state.checkpoints.load(Ordering::Relaxed);
    assert_eq!(first.failure(), None);
    assert_eq!(second.failure(), None);
    let gate = DashGateScope::new();
    let expected = Error::new(FailureKind::Invariant, LimitKind::None);
    if checked {
        assert_eq!(
            path.try_dash(&dash, 1.0, &first, ResourceKind::Geometry)
                .unwrap_err(),
            expected
        );
    } else {
        let candidate = path.dash(&dash, 1.0);
        assert!(
            candidate.is_none(),
            "foreign paid dash reached PB/Contour {:?}",
            gate.starts()
        );
    }
    assert_eq!(first.failure(), Some(expected));
    assert_eq!(second.failure(), None);
    assert_eq!(gate.starts(), (0, 0));
    assert_eq!(first_state.reserves.load(Ordering::Relaxed), first_reserves);
    assert_eq!(
        second_state.reserves.load(Ordering::Relaxed),
        second_reserves
    );
    // Exactly the original source-owner gate ran, before any contour work.
    assert_eq!(
        first_state.checkpoints.load(Ordering::Relaxed),
        checkpoints + 1
    );
    drop(gate);
    drop((path, dash));
    retired(&first, &first_state);
    retired(&second, &second_state);
}

#[test]
fn foreign_paid_dash_option_entry_rejects_before_builder_and_contour() {
    let _lock = ISOLATION.lock().unwrap_or_else(|error| error.into_inner());
    reject_foreign_paid_dash(false);
}

#[test]
fn foreign_paid_dash_checked_entry_rejects_before_builder_and_contour() {
    let _lock = ISOLATION.lock().unwrap_or_else(|error| error.into_inner());
    reject_foreign_paid_dash(true);
}

#[test]
fn same_request_and_compat_dash_keep_both_entrypoints_usable() {
    let _lock = ISOLATION.lock().unwrap_or_else(|error| error.into_inner());
    for checked in [false, true] {
        for owned in [false, true] {
            let (resources, state) = setup(error());
            let path = paid_curve(&resources).unwrap();
            let dash = if owned {
                paid_dash(&resources)
            } else {
                StrokeDash::new(vec![2.0, 3.0], 0.125).unwrap()
            };
            let reference = path
                .try_dash(&dash, 1.0, &resources, ResourceKind::Geometry)
                .unwrap()
                .unwrap();
            let gate = DashGateScope::new();
            let output = if checked {
                path.try_dash(&dash, 1.0, &resources, ResourceKind::Geometry)
                    .unwrap()
            } else {
                path.dash(&dash, 1.0)
            }
            .unwrap();
            assert_geometry(&reference, &output);
            assert_eq!(gate.starts(), (1, 1));
            assert_eq!(resources.failure(), None);
            drop(gate);
            drop((path, dash, output, reference));
            retired(&resources, &state);
        }
    }
}

#[test]
fn foreign_paid_stroker_and_source_reject_in_both_entrypoints() {
    let _lock = ISOLATION.lock().unwrap_or_else(|error| error.into_inner());
    for checked in [false, true] {
        let (first, first_state) = setup(error());
        let (second, second_state) = setup(error());
        let path = paid_curve(&second).unwrap();
        let mut stroker = PathStroker::with_resources(&first);
        let reserves = first_state.reserves.load(Ordering::Relaxed);
        let expected = Error::new(FailureKind::Invariant, LimitKind::None);
        if checked {
            assert_eq!(
                stroker
                    .try_stroke(
                        &path,
                        &Stroke::default(),
                        1.0,
                        &first,
                        ResourceKind::Geometry
                    )
                    .unwrap_err(),
                expected
            );
        } else {
            assert!(stroker.stroke(&path, &Stroke::default(), 1.0).is_none());
        }
        assert_eq!(first.failure(), Some(expected));
        assert_eq!(second.failure(), None);
        assert_eq!(first_state.reserves.load(Ordering::Relaxed), reserves);
        drop((stroker, path));
        retired(&first, &first_state);
        retired(&second, &second_state);
    }
}

#[test]
fn paid_default_constructor_is_allocation_free_and_keeps_original_command_state() {
    let _lock = ISOLATION.lock().unwrap_or_else(|error| error.into_inner());
    #[derive(Clone, Copy)]
    enum Command {
        Move(f32, f32),
        Line(f32, f32),
        Quad(f32, f32, f32, f32),
        Cubic(f32, f32, f32, f32, f32, f32),
        Close,
    }
    use Command::*;
    let cases: &[(&str, &[Command])] = &[
        ("empty", &[]),
        ("close-only", &[Close]),
        ("line-first-single", &[Line(1.0, 2.0)]),
        ("line-first-two", &[Line(1.0, 2.0), Line(3.0, 4.0)]),
        ("quad-first", &[Quad(1.0, 2.0, 3.0, 4.0), Line(7.0, 8.0)]),
        (
            "cubic-first",
            &[Cubic(1.0, 2.0, 3.0, 4.0, 5.0, 6.0), Line(9.0, 8.0)],
        ),
        ("move-only", &[Move(-0.0, 0.0)]),
        ("valid-move", &[Move(-0.0, 0.0), Line(3.0, -0.0), Close]),
        ("close-then-lines", &[Close, Line(1.0, 2.0), Line(3.0, 4.0)]),
    ];
    for &(name, commands) in cases {
        std::println!("paid-default-case={name}");
        let (resources, state) = setup(error());
        let mut ordinary = PathBuilder::default();
        let (mut paid, original_new) = {
            let _observer = ObserveGuard::new();
            LAST_ALLOCATED_PTR.store(0, Ordering::Relaxed);
            LAST_ALLOCATED_SIZE.store(0, Ordering::Relaxed);
            let paid = PathBuilder::with_resources_default(&resources, ResourceKind::Geometry);
            let original_new = PathBuilder::with_resources(&resources, ResourceKind::Geometry);
            assert_eq!(
                LAST_ALLOCATED_PTR.load(Ordering::Relaxed),
                0,
                "constructor allocated for {name}"
            );
            assert_eq!(LAST_ALLOCATED_SIZE.load(Ordering::Relaxed), 0);
            (paid, original_new)
        };
        assert_eq!(state.reserves.load(Ordering::Relaxed), 0);
        assert_eq!(state.checkpoints.load(Ordering::Relaxed), 0);
        assert!(
            original_new.move_to_required,
            "with_resources keeps new semantics"
        );
        assert!(!paid.move_to_required);
        assert_eq!(paid.last_move_to_index, 0);
        assert_eq!(paid.storage.kind(), Some(ResourceKind::Geometry));
        assert!(paid
            .storage
            .resources()
            .unwrap()
            .is_same_request(&resources));
        drop(original_new);
        for command in commands {
            match *command {
                Move(x, y) => {
                    ordinary.move_to(x, y);
                    paid.try_move_to(x, y).unwrap();
                }
                Line(x, y) => {
                    ordinary.line_to(x, y);
                    paid.try_line_to(x, y).unwrap();
                }
                Quad(x1, y1, x, y) => {
                    ordinary.quad_to(x1, y1, x, y);
                    paid.try_quad_to(x1, y1, x, y).unwrap();
                }
                Cubic(x1, y1, x2, y2, x, y) => {
                    ordinary.cubic_to(x1, y1, x2, y2, x, y);
                    paid.try_cubic_to(x1, y1, x2, y2, x, y).unwrap();
                }
                Close => {
                    ordinary.close();
                    paid.try_close().unwrap();
                }
            }
            assert_eq!(ordinary.verbs(), paid.verbs(), "verbs for {name}");
            assert_eq!(ordinary.points(), paid.points(), "points for {name}");
            assert_eq!(
                ordinary.last_move_to_index, paid.last_move_to_index,
                "move index for {name}"
            );
            assert_eq!(
                ordinary.move_to_required, paid.move_to_required,
                "move state for {name}"
            );
        }
        let before = ordinary.finish();
        let after = paid.try_finish().unwrap();
        assert_eq!(
            before.is_some(),
            after.is_some(),
            "None behavior for {name}"
        );
        if let (Some(before), Some(after)) = (before, after) {
            assert_geometry(&before, &after);
        }
        assert_eq!(resources.failure(), None);
        retired(&resources, &state);
    }
}
