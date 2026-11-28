// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE file in the project root for license information.
// ---------------------------------------------------------------------------------------------
extern crate std;

use super::*;

#[test]
fn ordinary_storage_moves_ranges_and_uses_stable_equal_key_order() {
    let mut cx = NoControl;
    let mut ranges = cx.buffer();
    ranges.try_push(2usize..5).unwrap();
    ranges.try_push(9..12).unwrap();
    let moved: Vec<_> = ranges.drain_all().rev().collect();
    assert!(ranges.is_empty());
    assert_eq!(moved, [9..12, 2..5]);

    for length in [0, 1, 2, 3, 17, 513] {
        let mut values: Vec<_> = (0..length).map(|i| ((length - i) % 7, i)).collect();
        let mut expected = values.clone();
        expected.sort_by_key(|value| value.0);
        stable_sort_by_key_in(&mut values, |value| value.0, &mut cx).unwrap();
        assert_eq!(values, expected);
    }
}

#[test]
fn ordinary_copy_and_resize_keep_original_values() {
    let mut cx = NoControl;
    let mut values = copy_slice_in(&[7u32, 11, 13], &mut cx).unwrap();
    resize_with_in(&mut values, 600, || 17, &mut cx).unwrap();
    assert_eq!(&values[..3], &[7, 11, 13]);
    assert!(values[3..].iter().all(|&value| value == 17));
    resize_with_in(&mut values, 2, || 0, &mut cx).unwrap();
    assert_eq!(values, [7, 11]);
}

#[cfg(feature = "notepads-markdown-resources")]
pub(crate) mod support {
    use super::std;
    use core::{
        alloc::Layout,
        cell::{Cell, RefCell},
    };
    use markdown_svg_budget::{
        Backend, Error, FailureKind, FirstFailureRef, LimitKind, Limits, ResourceKind,
        ResourcesRef, Ticket,
    };
    use std::{
        alloc::{GlobalAlloc, System},
        sync::{
            atomic::{AtomicU64, AtomicUsize, Ordering},
            Arc,
        },
    };

    const MAX_ALLOCATIONS: usize = 2048;

    #[derive(Clone, Copy, Default)]
    struct TraceSlot {
        bytes: usize,
        pointer: usize,
        reserve: usize,
        allocate: usize,
        commit: usize,
        free: usize,
        release: usize,
    }
    impl TraceSlot {
        const EMPTY: Self = Self {
            bytes: 0,
            pointer: 0,
            reserve: 0,
            allocate: 0,
            commit: 0,
            free: 0,
            release: 0,
        };
    }

    struct Trace {
        enabled: bool,
        bad: bool,
        sequence: usize,
        pending: Option<usize>,
        allocations: usize,
        frees: usize,
        unpaid: usize,
        slots: [TraceSlot; MAX_ALLOCATIONS],
    }
    impl Trace {
        const fn new() -> Self {
            Self {
                enabled: false,
                bad: false,
                sequence: 0,
                pending: None,
                allocations: 0,
                frees: 0,
                unpaid: 0,
                slots: [TraceSlot::EMPTY; MAX_ALLOCATIONS],
            }
        }
        fn next(&mut self) -> usize {
            self.sequence += 1;
            self.sequence
        }
    }

    std::thread_local! {
        // Const TLS and fixed slots keep allocator bookkeeping allocation-free.
        // Each libtest thread owns its own observation and fault fixture.
        static TRACE: RefCell<Trace> = const { RefCell::new(Trace::new()) };
        static REENTRANT: Cell<bool> = const { Cell::new(false) };
    }

    fn trace(f: impl FnOnce(&mut Trace)) {
        let _ = TRACE.try_with(|state| {
            match state.try_borrow_mut() {
                Ok(mut state) => {
                    if state.enabled {
                        f(&mut state);
                    }
                }
                Err(_) => {
                    // An unexpected reentrant observer cannot silently hide an
                    // allocation and still turn the ownership assertion green.
                    let _ = REENTRANT.try_with(|bad| bad.set(true));
                }
            }
        });
    }

    fn allocated(pointer: *mut u8, bytes: usize) {
        if pointer.is_null() {
            return;
        }
        trace(|state| {
            let Some(index) = state.pending.take() else {
                state.bad = true;
                state.unpaid += 1;
                return;
            };
            let sequence = state.next();
            let slot = &mut state.slots[index];
            if slot.bytes != bytes || slot.reserve == 0 || slot.pointer != 0 {
                state.bad = true;
            }
            slot.pointer = pointer as usize;
            slot.allocate = sequence;
            state.allocations += 1;
        });
    }

    fn freed(pointer: *mut u8) {
        trace(|state| {
            let found = state.slots.iter().position(|slot| {
                slot.pointer == pointer as usize && slot.allocate != 0 && slot.free == 0
            });
            if let Some(index) = found {
                let sequence = state.next();
                let slot = &mut state.slots[index];
                if slot.release != 0 {
                    state.bad = true;
                }
                slot.free = sequence;
                state.frees += 1;
            }
        });
    }

    // Observation only: exact operations always forward to System. This never
    // refuses allocation, formats output, panics, or changes allocation sizes.
    struct Observer;
    unsafe impl GlobalAlloc for Observer {
        unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
            let pointer = unsafe { System.alloc(layout) };
            allocated(pointer, layout.size());
            pointer
        }
        unsafe fn alloc_zeroed(&self, layout: Layout) -> *mut u8 {
            let pointer = unsafe { System.alloc_zeroed(layout) };
            allocated(pointer, layout.size());
            pointer
        }
        unsafe fn dealloc(&self, pointer: *mut u8, layout: Layout) {
            unsafe { System.dealloc(pointer, layout) };
            // The physical deallocation has completed before recording free.
            freed(pointer);
        }
        unsafe fn realloc(&self, pointer: *mut u8, layout: Layout, bytes: usize) -> *mut u8 {
            let replacement = unsafe { System.realloc(pointer, layout, bytes) };
            if !replacement.is_null() {
                freed(pointer);
                allocated(replacement, bytes);
            }
            replacement
        }
    }
    #[global_allocator]
    static OBSERVER: Observer = Observer;

    pub(crate) struct Observation;
    #[derive(Debug)]
    pub(crate) struct Observed {
        pub(crate) allocations: usize,
        pub(crate) frees: usize,
        bad: bool,
        unpaid: usize,
        unfinished: usize,
    }
    impl Observation {
        pub(crate) fn start() -> Self {
            TRACE.with(|state| {
                let mut state = state.borrow_mut();
                assert!(!state.enabled);
                *state = Trace::new();
                state.enabled = true;
            });
            REENTRANT.with(|bad| bad.set(false));
            Self
        }
        pub(crate) fn finish(self) -> Observed {
            let result = TRACE.with(|state| {
                let mut state = state.borrow_mut();
                state.enabled = false;
                Observed {
                    allocations: state.allocations,
                    frees: state.frees,
                    bad: state.bad || REENTRANT.with(Cell::get),
                    unpaid: state.unpaid,
                    unfinished: state
                        .slots
                        .iter()
                        .filter(|slot| slot.reserve != 0 && slot.release == 0)
                        .count()
                        + usize::from(state.pending.is_some()),
                }
            });
            drop(self);
            result
        }
    }
    impl Drop for Observation {
        fn drop(&mut self) {
            let _ = TRACE.try_with(|state| {
                if let Ok(mut state) = state.try_borrow_mut() {
                    state.enabled = false;
                }
            });
        }
    }
    impl Observed {
        pub(crate) fn assert_freed_before_refund(&self) {
            assert!(
                !self.bad,
                "actual allocation/commit/free/release order changed: {:?}",
                self
            );
            assert_eq!(self.unpaid, 0);
            assert_eq!(self.unfinished, 0);
            assert_eq!(self.allocations, self.frees);
        }
    }

    #[derive(Clone, Copy, Default)]
    pub(crate) struct Faults {
        pub(crate) reserve: usize,
        pub(crate) commit: usize,
        pub(crate) checkpoint: usize,
    }
    pub(crate) fn reserve_error() -> Error {
        Error::new(FailureKind::Quota, LimitKind::Scratch)
    }
    pub(crate) fn commit_error() -> Error {
        Error::new(FailureKind::Allocation, LimitKind::Capacity)
    }
    pub(crate) fn checkpoint_error() -> Error {
        Error::new(FailureKind::Cancelled, LimitKind::None)
    }

    struct State {
        faults: Faults,
        reserves: AtomicUsize,
        commits: AtomicUsize,
        checkpoints: AtomicUsize,
        maximum_checkpoint: AtomicU64,
        work: AtomicU64,
        live: AtomicUsize,
        peak: AtomicUsize,
        sizes: [AtomicUsize; MAX_ALLOCATIONS],
    }
    struct TestBackend(Arc<State>);
    impl Backend for TestBackend {
        fn reserve(&self, kind: ResourceKind, bytes: usize) -> Result<Ticket, Error> {
            assert_eq!(kind, ResourceKind::Scratch);
            let number = self.0.reserves.fetch_add(1, Ordering::Relaxed) + 1;
            if number == self.0.faults.reserve {
                return Err(reserve_error());
            }
            if number > MAX_ALLOCATIONS {
                return Err(Error::new(FailureKind::Invariant, LimitKind::Capacity));
            }
            let index = number - 1;
            self.0.sizes[index].store(bytes, Ordering::Relaxed);
            let live = self.0.live.fetch_add(bytes, Ordering::Relaxed) + bytes;
            self.0.peak.fetch_max(live, Ordering::Relaxed);
            trace(|state| {
                if state.pending.is_some() || state.slots[index].reserve != 0 {
                    state.bad = true;
                }
                let sequence = state.next();
                state.slots[index].bytes = bytes;
                state.slots[index].reserve = sequence;
                state.pending = Some(index);
            });
            Ticket::new(1, index as u32, 1)
        }

        fn allocation_committed(&self, ticket: &Ticket, bytes: usize) -> Result<(), Error> {
            assert_eq!(ticket.account_nonce(), 1);
            assert_eq!(ticket.generation(), 1);
            let index = ticket.slot() as usize;
            trace(|state| {
                let sequence = state.next();
                let slot = &mut state.slots[index];
                if slot.bytes != bytes
                    || slot.allocate <= slot.reserve
                    || slot.free != 0
                    || slot.commit != 0
                {
                    state.bad = true;
                }
                slot.commit = sequence;
            });
            let number = self.0.commits.fetch_add(1, Ordering::Relaxed) + 1;
            if number == self.0.faults.commit {
                Err(commit_error())
            } else {
                Ok(())
            }
        }

        fn release_after_free(&self, ticket: Ticket) -> Result<(), Error> {
            assert_eq!(ticket.account_nonce(), 1);
            assert_eq!(ticket.generation(), 1);
            let index = ticket.slot() as usize;
            trace(|state| {
                let sequence = state.next();
                let slot = &mut state.slots[index];
                if slot.release != 0
                    || (slot.pointer != 0 && (slot.free <= slot.allocate || slot.free == 0))
                    || (slot.commit != 0 && slot.free <= slot.commit)
                {
                    state.bad = true;
                }
                slot.release = sequence;
                if state.pending == Some(index) {
                    state.pending = None;
                }
            });
            let bytes = self.0.sizes[index].swap(0, Ordering::Relaxed);
            assert_ne!(bytes, 0);
            self.0.live.fetch_sub(bytes, Ordering::Relaxed);
            Ok(())
        }

        fn checkpoint(&self, work: u64) -> Result<(), Error> {
            self.0.maximum_checkpoint.fetch_max(work, Ordering::Relaxed);
            self.0.work.fetch_add(work, Ordering::Relaxed);
            let number = self.0.checkpoints.fetch_add(1, Ordering::Relaxed) + 1;
            if number == self.0.faults.checkpoint {
                Err(checkpoint_error())
            } else {
                Ok(())
            }
        }
    }

    pub(crate) struct Fixture {
        pub(crate) resources: ResourcesRef,
        state: Arc<State>,
    }
    impl Fixture {
        pub(crate) fn new(faults: Faults) -> Self {
            Self::with_limits(faults, Limits::default())
        }
        pub(crate) fn with_limits(faults: Faults, limits: Limits) -> Self {
            let state = Arc::new(State {
                faults,
                reserves: AtomicUsize::new(0),
                commits: AtomicUsize::new(0),
                checkpoints: AtomicUsize::new(0),
                maximum_checkpoint: AtomicU64::new(0),
                work: AtomicU64::new(0),
                live: AtomicUsize::new(0),
                peak: AtomicUsize::new(0),
                sizes: core::array::from_fn(|_| AtomicUsize::new(0)),
            });
            let resources = ResourcesRef::new_prepaid(
                TestBackend(state.clone()),
                FirstFailureRef::new_prepaid(),
                limits,
            );
            Self { resources, state }
        }
        pub(crate) fn reserves(&self) -> usize {
            self.state.reserves.load(Ordering::Relaxed)
        }
        pub(crate) fn commits(&self) -> usize {
            self.state.commits.load(Ordering::Relaxed)
        }
        pub(crate) fn checkpoints(&self) -> usize {
            self.state.checkpoints.load(Ordering::Relaxed)
        }
        pub(crate) fn live(&self) -> usize {
            self.state.live.load(Ordering::Relaxed)
        }
        pub(crate) fn peak(&self) -> usize {
            self.state.peak.load(Ordering::Relaxed)
        }
        pub(crate) fn maximum_checkpoint(&self) -> u64 {
            self.state.maximum_checkpoint.load(Ordering::Relaxed)
        }
        pub(crate) fn assert_retired(&self) {
            assert_eq!(self.live(), 0);
            assert_eq!(self.resources.statistics().live_leases, 0);
            assert_eq!(self.resources.statistics().scratch_bytes, 0);
            assert_eq!(self.resources.statistics().release_failures, 0);
            self.resources.validate_retired().unwrap();
        }
    }
}

#[cfg(feature = "notepads-markdown-resources")]
use support::{checkpoint_error, commit_error, reserve_error, Faults, Fixture, Observation};

#[cfg(feature = "notepads-markdown-resources")]
#[test]
fn request_storage_prepays_exact_events_and_preserves_the_first_record() {
    let fixture = Fixture::new(Faults::default());
    let mut cx = RequestStorage::new(&fixture.resources).unwrap();
    let before = fixture.resources.statistics().work;
    cx.before_events(256).unwrap();
    assert_eq!(fixture.resources.statistics().work - before, 256);
    assert_eq!(fixture.maximum_checkpoint(), 256);
    let first = Error::native(-2_147_024_809);
    assert_eq!(fixture.resources.record(first), first);
    assert_eq!(cx.before_events(1), Err(first));
    assert_eq!(cx.before_events(257), Err(first));
    assert_eq!(cx.finish(), Err(first));
    drop(cx);
    fixture.assert_retired();

    let fixture = Fixture::new(Faults::default());
    let mut cx = RequestStorage::new(&fixture.resources).unwrap();
    assert_eq!(
        cx.before_events(257),
        Err(Error::new(FailureKind::Invariant, LimitKind::Work))
    );
    assert_eq!(fixture.resources.statistics().work, 0);
    drop(cx);
    fixture.assert_retired();
}

#[cfg(feature = "notepads-markdown-resources")]
#[test]
fn nested_drain_moves_owners_and_early_drop_frees_storage_before_refund() {
    let fixture = Fixture::new(Faults::default());
    let observed = Observation::start();
    let cx = RequestStorage::new(&fixture.resources).unwrap();
    let mut outer = cx.buffer();
    for i in 0..12 {
        let mut inner = cx.buffer();
        inner.try_reserve_exact(1).unwrap();
        inner.try_push(i..i + 2).unwrap();
        outer.try_push(inner).unwrap();
    }
    let paid = fixture.live();
    let reserves = fixture.reserves();
    {
        let mut drain = outer.drain_all();
        let first = drain.next().unwrap();
        assert_eq!(&first[..], &[0..2]);
        assert_eq!(fixture.reserves(), reserves);
        assert_eq!(fixture.live(), paid);
        drop(first);
        // Dropping an incomplete drain destroys every remaining inner owner.
    }
    assert!(outer.is_empty());
    assert!(fixture.live() > 0, "empty outer still owns its allocation");
    drop(outer);
    drop(cx);
    let observed = observed.finish();
    assert!(observed.allocations >= 13);
    observed.assert_freed_before_refund();
    fixture.assert_retired();
}

#[cfg(feature = "notepads-markdown-resources")]
fn sorting_request(fixture: &Fixture) -> Result<(), Error> {
    let mut cx = RequestStorage::new(&fixture.resources)?;
    let mut values = cx.buffer();
    for i in 0..777usize {
        cx.before_events(1)?;
        values.try_push(((776 - i) % 13, i))?;
    }
    stable_sort_by_key_in(&mut values, |value| value.0, &mut cx)?;
    assert!(values.windows(2).all(|pair| {
        pair[0].0 < pair[1].0 || (pair[0].0 == pair[1].0 && pair[0].1 < pair[1].1)
    }));
    cx.finish()?;
    Ok(())
}

#[cfg(feature = "notepads-markdown-resources")]
#[test]
fn stable_sort_rejects_each_actual_reserve_and_commit_and_frees_every_owner() {
    let baseline = Fixture::new(Faults::default());
    let observed = Observation::start();
    sorting_request(&baseline).unwrap();
    observed.finish().assert_freed_before_refund();
    baseline.assert_retired();
    let (reserves, commits) = (baseline.reserves(), baseline.commits());
    assert!(reserves > 2);
    assert_eq!(reserves, commits);
    assert!(baseline.peak() > 777 * core::mem::size_of::<(usize, usize)>());

    for number in 1..=reserves {
        let fixture = Fixture::new(Faults {
            reserve: number,
            ..Faults::default()
        });
        let observed = Observation::start();
        assert_eq!(sorting_request(&fixture), Err(reserve_error()));
        observed.finish().assert_freed_before_refund();
        assert_eq!(fixture.reserves(), number);
        assert_eq!(fixture.resources.failure(), Some(reserve_error()));
        fixture.assert_retired();
    }
    for number in 1..=commits {
        let fixture = Fixture::new(Faults {
            commit: number,
            ..Faults::default()
        });
        let observed = Observation::start();
        assert_eq!(sorting_request(&fixture), Err(commit_error()));
        observed.finish().assert_freed_before_refund();
        assert_eq!(fixture.commits(), number);
        assert_eq!(fixture.resources.failure(), Some(commit_error()));
        fixture.assert_retired();
    }
    std::eprintln!(
        "BIDI_FAULT_MATRIX family=sort reserve_cases={} commit_cases={} peak_requested_live={} actual_free_before_refund=true",
        reserves, commits, baseline.peak()
    );
}

#[cfg(feature = "notepads-markdown-resources")]
#[test]
fn sorter_stops_in_copy_comparison_and_later_pass_windows_without_an_output() {
    let baseline = Fixture::new(Faults::default());
    sorting_request(&baseline).unwrap();
    let last = baseline.checkpoints();
    assert!(last > 1000);
    for checkpoint in [1, 2, 257, 778, 800, last / 2, last - 1, last] {
        let fixture = Fixture::new(Faults {
            checkpoint,
            ..Faults::default()
        });
        let observed = Observation::start();
        assert_eq!(sorting_request(&fixture), Err(checkpoint_error()));
        observed.finish().assert_freed_before_refund();
        assert_eq!(fixture.checkpoints(), checkpoint);
        fixture.assert_retired();
    }
    std::eprintln!(
        "BIDI_FAULT_MATRIX family=sort checkpoint_cases=8 actual_checkpoint_denominator={} selected_windows={:?} actual_free_before_refund=true",
        last, [1, 2, 257, 778, 800, last / 2, last - 1, last]
    );
}

#[cfg(feature = "notepads-markdown-resources")]
#[test]
fn copy_and_resize_second_window_fail_before_performing_unpaid_changes() {
    let fixture = Fixture::new(Faults {
        checkpoint: 3,
        ..Faults::default()
    });
    let observed = Observation::start();
    let mut cx = RequestStorage::new(&fixture.resources).unwrap();
    let result = copy_slice_in(&[7u8; 600], &mut cx);
    assert_eq!(result.err(), Some(checkpoint_error()));
    drop(cx);
    observed.finish().assert_freed_before_refund();
    fixture.assert_retired();

    let fixture = Fixture::new(Faults {
        checkpoint: 3,
        ..Faults::default()
    });
    let observed = Observation::start();
    let mut cx = RequestStorage::new(&fixture.resources).unwrap();
    let mut buffer = cx.buffer();
    let mut made = 0;
    let result = resize_with_in(
        &mut buffer,
        600,
        || {
            made += 1;
            7u8
        },
        &mut cx,
    );
    assert_eq!(result, Err(checkpoint_error()));
    assert_eq!(made, 256);
    assert_eq!(buffer.len(), 256);
    drop(buffer);
    drop(cx);
    observed.finish().assert_freed_before_refund();
    fixture.assert_retired();
}

#[cfg(feature = "notepads-markdown-resources")]
#[test]
fn sorter_cancels_before_the_second_copyback_window_and_keeps_the_unwritten_suffix() {
    struct CopybackWindowGate {
        inner: RequestStorage,
        resources: ResourcesRef,
        full_windows: usize,
        rejected: bool,
        work_at_rejection: Option<u64>,
    }

    impl Storage for CopybackWindowGate {
        type Error = Error;
        type Buffer<T> = BudgetVec<T>;

        fn buffer<T>(&self) -> BudgetVec<T> {
            self.inner.buffer()
        }

        fn before_events(&mut self, count: usize) -> Result<(), Error> {
            if count == 256 {
                self.full_windows += 1;
                if self.full_windows == 5 {
                    self.rejected = true;
                    self.work_at_rejection = Some(self.resources.statistics().work);
                    // Reject before granting the second copyback block. The
                    // original request records and transports this exact error.
                    return Err(self.resources.record(checkpoint_error()));
                }
            }
            self.inner.before_events(count)
        }

        fn finish(&mut self) -> Result<(), Error> {
            self.inner.finish()
        }
    }

    let fixture = Fixture::new(Faults::default());
    let observed = Observation::start();
    let mut prepare = RequestStorage::new(&fixture.resources).unwrap();
    let mut values = prepare.buffer();
    values.try_reserve_exact(777).unwrap();
    for index in 0..777usize {
        prepare.before_events(1).unwrap();
        values.try_push((777 - index, index)).unwrap();
    }
    assert_eq!(fixture.reserves(), 1);
    assert_eq!(fixture.commits(), 1);
    let input_paid = fixture.live();
    assert!(input_paid > 0);
    // On both supported pointer widths, the pair's bounded copy block is 256.
    assert_eq!(
        (65_536 / core::mem::size_of::<(usize, usize)>()).min(256),
        256
    );

    // Only sorting uses the wrapper. Preparing the paid input did not consume
    // any of its window counters and uses the same original request owner.
    let mut gate = CopybackWindowGate {
        inner: prepare,
        resources: fixture.resources.clone(),
        full_windows: 0,
        rejected: false,
        work_at_rejection: None,
    };
    let result = stable_sort_by_key_in(&mut values, |value| value.0, &mut gate);
    assert_eq!(result, Err(checkpoint_error()));
    assert!(gate.rejected);
    assert_eq!(gate.full_windows, 5);
    assert_eq!(fixture.resources.failure(), Some(checkpoint_error()));
    assert_eq!(
        gate.work_at_rejection,
        Some(fixture.resources.statistics().work)
    );

    // Three 256-item calls initialized scratch (plus its nine-item tail).
    // The fourth granted the first first-round copyback block; the fifth
    // rejected the next block. These values prove the actual write frontier:
    // only the first 256 records contain the first adjacent-pair merge.
    for (index, &value) in values[..256].iter().enumerate() {
        let original_index = if index % 2 == 0 { index + 1 } else { index - 1 };
        assert_eq!(value, (777 - original_index, original_index));
    }
    for index in 256..777usize {
        assert_eq!(values[index], (777 - index, index));
    }
    assert_eq!(values[0], (776, 1));
    assert_eq!(values[255], (523, 254));
    assert_eq!(values[256], (521, 256));
    assert_eq!(values[776], (1, 776));

    // Scratch was actually dropped on the early return; input is still paid.
    assert_eq!(fixture.reserves(), 2);
    assert_eq!(fixture.commits(), 2);
    assert_eq!(fixture.live(), input_paid);
    assert!(fixture.peak() > input_paid);
    let checkpoints_after_rejection = fixture.checkpoints();
    assert_eq!(gate.finish(), Err(checkpoint_error()));
    assert_eq!(fixture.checkpoints(), checkpoints_after_rejection);
    assert_eq!(
        fixture.resources.record(Error::native(-2_147_024_809)),
        checkpoint_error()
    );

    drop(values);
    drop(gate);
    observed.finish().assert_freed_before_refund();
    fixture.assert_retired();
    std::eprintln!(
        "BIDI_SORT_COPYBACK_SECOND_WINDOW items=777 scratch_full_windows=3 copyback_granted_full_windows=1 copyback_rejected_full_window=2 rejected_256_call=5 rewritten_prefix=256 untouched_suffix=521 typed_cancel=true actual_free_before_refund=true retired=true"
    );
}
