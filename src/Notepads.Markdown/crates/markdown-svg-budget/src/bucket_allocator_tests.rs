// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE file in the project root for license information.
// ---------------------------------------------------------------------------------------------
use super::*;
use allocator_api2::alloc::Allocator;
use std::panic::{AssertUnwindSafe, catch_unwind};

#[repr(C)]
struct ObservedHeader {
    lease: Lease,
}

fn block_layout(payload: Layout) -> (Layout, usize) {
    let (layout, offset) = Layout::new::<ObservedHeader>().extend(payload).unwrap();
    (layout.pad_to_align(), offset)
}

fn trace_block(bytes: usize) {
    TRACE_BYTES.store(bytes, Ordering::Relaxed);
    TRACE_PTR.store(0, Ordering::Relaxed);
    TRACE_PAID.store(0, Ordering::Relaxed);
    TRACE_SEQUENCE.store(0, Ordering::Relaxed);
    TRACE_RESERVED.store(0, Ordering::Relaxed);
    TRACE_ALLOCATED.store(0, Ordering::Relaxed);
    TRACE_ADMITTED.store(0, Ordering::Relaxed);
    TRACE_FREED.store(0, Ordering::Relaxed);
    TRACE_RELEASED.store(0, Ordering::Relaxed);
    TRACE_BAD.store(false, Ordering::Relaxed);
}

#[test]
fn bucket_layout_header_padding_zero_size_and_clone_free_are_exact() {
    let _isolation = isolate_observer();
    for (bytes, align) in [(0, 1), (0, 256), (1, 1), (17, 256), (65_537, 16)] {
        let (resources, _, state) = setup(Limits::default());
        let allocator = BucketAllocator::new(resources.clone(), ResourceKind::Scratch);
        let other = allocator.clone();
        let payload = Layout::from_size_align(bytes, align).unwrap();
        let (physical, offset) = block_layout(payload);
        trace_block(physical.size());
        let pointer = allocator.allocate(payload).unwrap();
        assert_eq!(pointer.len(), bytes);
        assert_eq!(pointer.cast::<u8>().as_ptr() as usize % align, 0);
        assert_eq!(state.live.load(Ordering::Relaxed), physical.size() as u64);
        assert_eq!(resources.statistics().scratch_bytes, physical.size() as u64);
        let base = unsafe { pointer.cast::<u8>().as_ptr().sub(offset) };
        assert_eq!(TRACE_PTR.load(Ordering::Relaxed), base as usize);
        WATCH_PTR.store(base as usize, Ordering::Relaxed);
        WATCH_FREED.store(false, Ordering::Relaxed);
        state.watched_ticket.store(1, Ordering::Relaxed);
        unsafe { other.deallocate(pointer.cast(), payload) };
        assert!(state.physical_free_before_release.load(Ordering::Acquire));
        assert!(!TRACE_BAD.load(Ordering::Relaxed));
        let order = [
            TRACE_RESERVED.load(Ordering::Relaxed),
            TRACE_ALLOCATED.load(Ordering::Relaxed),
            TRACE_ADMITTED.load(Ordering::Relaxed),
            TRACE_FREED.load(Ordering::Relaxed),
            TRACE_RELEASED.load(Ordering::Relaxed),
        ];
        assert!(order[0] != 0 && order.windows(2).all(|pair| pair[0] < pair[1]));
        assert_eq!(state.live.load(Ordering::Relaxed), 0);
        drop((allocator, other));
        resources.validate_retired().unwrap();
    }
}

struct ZeroWitness {
    inner: TestBackend,
    payload: Layout,
    saw_prefix: Arc<AtomicBool>,
}

impl Backend for ZeroWitness {
    fn reserve(&self, kind: ResourceKind, bytes: usize) -> Result<Ticket, Error> {
        self.inner.reserve(kind, bytes)
    }

    fn allocation_committed(&self, ticket: &Ticket, bytes: usize) -> Result<(), Error> {
        self.inner.allocation_committed(ticket, bytes)?;
        let (_, offset) = block_layout(self.payload);
        let base = TRACE_PTR.load(Ordering::Relaxed) as *mut u8;
        assert!(!base.is_null());
        // Test-only admission fixture initializes this real private payload to
        // a known value, before allocate_zeroed has initialized any byte. The
        // observer binds this exact physical block; no pointer is retained.
        unsafe { base.add(offset).write_bytes(0xa5, self.payload.size()) };
        Ok(())
    }

    fn release_after_free(&self, ticket: Ticket) -> Result<(), Error> {
        self.inner.release_after_free(ticket)
    }

    fn checkpoint(&self, work: u64) -> Result<(), Error> {
        if self.inner.0.checkpoint_calls.load(Ordering::Relaxed) + 1 == 3 {
            let (_, offset) = block_layout(self.payload);
            let base = TRACE_PTR.load(Ordering::Relaxed) as *const u8;
            let bytes =
                unsafe { std::slice::from_raw_parts(base.add(offset), self.payload.size()) };
            self.saw_prefix.store(
                bytes[..256].iter().all(|value| *value == 0)
                    && bytes[256..].iter().all(|value| *value == 0xa5),
                Ordering::Relaxed,
            );
        }
        self.inner.checkpoint(work)
    }
}

#[test]
fn bucket_zeroed_second_window_cancel_preserves_unvisited_tail_and_frees_block() {
    let _isolation = isolate_observer();
    let (old_resources, fault, state) = setup(Limits::default());
    let payload = Layout::from_size_align(513, 64).unwrap();
    let saw_prefix = Arc::new(AtomicBool::new(false));
    let resources = ResourcesRef::new_prepaid(
        ZeroWitness {
            inner: TestBackend(state.clone()),
            payload,
            saw_prefix: saw_prefix.clone(),
        },
        fault,
        Limits::default(),
    );
    drop(old_resources);
    trace_block(block_layout(payload).0.size());
    state.fail_on_checkpoint.store(3, Ordering::Relaxed);
    let allocator = BucketAllocator::new(resources.clone(), ResourceKind::Scratch);
    assert!(allocator.allocate_zeroed(payload).is_err());
    assert!(saw_prefix.load(Ordering::Relaxed));
    assert_eq!(state.checkpoint_calls.load(Ordering::Relaxed), 3);
    assert_eq!(
        resources.failure(),
        Some(Error::new(FailureKind::Cancelled, LimitKind::Work))
    );
    assert_eq!(state.live.load(Ordering::Relaxed), 0);
    assert!(!TRACE_BAD.load(Ordering::Relaxed));
    assert!(TRACE_FREED.load(Ordering::Relaxed) < TRACE_RELEASED.load(Ordering::Relaxed));
    drop(allocator);
    resources.validate_retired().unwrap();
}

#[test]
fn bucket_zeroed_success_and_resize_refusals_keep_original_contents_and_lease() {
    let _isolation = isolate_observer();
    let (resources, _, state) = setup(Limits::default());
    let allocator = BucketAllocator::new(resources.clone(), ResourceKind::Geometry);
    let payload = Layout::from_size_align(513, 128).unwrap();
    let pointer = allocator.allocate_zeroed(payload).unwrap();
    let contents =
        unsafe { std::slice::from_raw_parts(pointer.cast::<u8>().as_ptr(), payload.size()) };
    assert!(contents.iter().all(|byte| *byte == 0));
    unsafe {
        pointer
            .cast::<u8>()
            .as_ptr()
            .write_bytes(0x79, payload.size())
    };
    let physical_bytes = state.live.load(Ordering::Relaxed);
    let grow = Layout::from_size_align(1024, 256).unwrap();
    let shrink = Layout::from_size_align(32, 16).unwrap();
    unsafe {
        assert!(allocator.grow(pointer.cast(), payload, grow).is_err());
        assert!(
            allocator
                .grow_zeroed(pointer.cast(), payload, grow)
                .is_err()
        );
        assert!(allocator.shrink(pointer.cast(), payload, shrink).is_err());
    }
    assert_eq!(
        resources.failure(),
        Some(Error::new(FailureKind::Invariant, LimitKind::Capacity))
    );
    assert!(
        unsafe { std::slice::from_raw_parts(pointer.cast::<u8>().as_ptr(), payload.size()) }
            .iter()
            .all(|byte| *byte == 0x79)
    );
    assert_eq!(state.reserves.load(Ordering::Relaxed), 1);
    assert_eq!(state.admissions.load(Ordering::Relaxed), 1);
    assert_eq!(state.releases.load(Ordering::Relaxed), 0);
    assert_eq!(state.live.load(Ordering::Relaxed), physical_bytes);
    unsafe { allocator.deallocate(pointer.cast(), payload) };
    drop(allocator);
    resources.validate_retired().unwrap();
}

#[test]
fn bucket_admission_failure_frees_before_refund_and_preserves_typed_error() {
    let _isolation = isolate_observer();
    for cancelled in [false, true] {
        let (resources, _, state) = setup(Limits::default());
        let payload = Layout::from_size_align(33, 64).unwrap();
        trace_block(block_layout(payload).0.size());
        state.fail_on_admission.store(1, Ordering::Relaxed);
        state
            .cancel_on_admission
            .store(cancelled, Ordering::Relaxed);
        let allocator = BucketAllocator::new(resources.clone(), ResourceKind::Scratch);
        assert!(allocator.allocate(payload).is_err());
        assert_eq!(
            resources.failure(),
            Some(if cancelled {
                Error::new(FailureKind::Cancelled, LimitKind::Work)
            } else {
                Error::native(-88)
            })
        );
        assert!(TRACE_FREED.load(Ordering::Relaxed) != 0);
        assert!(TRACE_FREED.load(Ordering::Relaxed) < TRACE_RELEASED.load(Ordering::Relaxed));
        assert_eq!(state.live.load(Ordering::Relaxed), 0);
        assert!(!TRACE_BAD.load(Ordering::Relaxed));
        drop(allocator);
        resources.validate_retired().unwrap();
    }
}

struct AdmissionPanic(TestBackend);
impl Backend for AdmissionPanic {
    fn reserve(&self, kind: ResourceKind, bytes: usize) -> Result<Ticket, Error> {
        self.0.reserve(kind, bytes)
    }
    fn allocation_committed(&self, ticket: &Ticket, bytes: usize) -> Result<(), Error> {
        self.0.allocation_committed(ticket, bytes)?;
        WATCH_PTR.store(TRACE_PTR.load(Ordering::Relaxed), Ordering::Relaxed);
        WATCH_FREED.store(false, Ordering::Relaxed);
        self.0
            .0
            .watched_ticket
            .store(u64::from(ticket.slot()), Ordering::Relaxed);
        // Panic formatting/unwind may make unrelated allocations. Follow the
        // exact already observed pointer/ticket rather than their byte sizes.
        TRACE_BYTES.store(0, Ordering::Relaxed);
        panic!("intentional bucket admission unwind")
    }
    fn release_after_free(&self, ticket: Ticket) -> Result<(), Error> {
        self.0.release_after_free(ticket)
    }
    fn checkpoint(&self, work: u64) -> Result<(), Error> {
        self.0.checkpoint(work)
    }
}

#[test]
fn bucket_admission_unwind_retires_the_already_allocated_block_before_its_lease() {
    let _isolation = isolate_observer();
    let (old_resources, fault, state) = setup(Limits::default());
    let resources = ResourcesRef::new_prepaid(
        AdmissionPanic(TestBackend(state.clone())),
        fault,
        Limits::default(),
    );
    drop(old_resources);
    let payload = Layout::from_size_align(35, 128).unwrap();
    trace_block(block_layout(payload).0.size());
    let allocator = BucketAllocator::new(resources.clone(), ResourceKind::Scratch);
    let actual = catch_unwind(AssertUnwindSafe(|| allocator.allocate(payload)));
    assert!(actual.is_err());
    assert!(WATCH_FREED.load(Ordering::Acquire));
    assert!(state.physical_free_before_release.load(Ordering::Acquire));
    assert_eq!(state.live.load(Ordering::Relaxed), 0);
    drop(allocator);
    resources.validate_retired().unwrap();
}

#[test]
fn bucket_release_error_frees_physical_block_once_but_does_not_refund_ticket() {
    let _isolation = isolate_observer();
    let (resources, _, state) = setup(Limits::default());
    let allocator = BucketAllocator::new(resources.clone(), ResourceKind::Scratch);
    let payload = Layout::from_size_align(29, 32).unwrap();
    let pointer = allocator.allocate(payload).unwrap();
    let (layout, offset) = block_layout(payload);
    WATCH_PTR.store(
        unsafe { pointer.cast::<u8>().as_ptr().sub(offset) } as usize,
        Ordering::Relaxed,
    );
    WATCH_FREED.store(false, Ordering::Relaxed);
    state.failed_release.store(true, Ordering::Relaxed);
    unsafe { allocator.deallocate(pointer.cast(), payload) };
    assert!(WATCH_FREED.load(Ordering::Acquire));
    assert_eq!(state.live.load(Ordering::Relaxed), layout.size() as u64);
    assert_eq!(state.entries.lock().unwrap().len(), 1);
    assert_eq!(resources.statistics().live_leases, 1);
    assert_eq!(resources.statistics().release_failures, 1);
    assert_eq!(resources.failure(), Some(Error::native(-77)));
    drop(allocator);
    assert_eq!(resources.validate_retired(), Err(Error::native(-77)));
}

#[test]
fn bucket_layout_overflow_is_rejected_before_reservation_or_physical_allocation() {
    let _isolation = isolate_observer();
    let (resources, _, state) = setup(Limits::default());
    let allocator = BucketAllocator::new(resources.clone(), ResourceKind::Scratch);
    let payload = Layout::from_size_align((isize::MAX as usize) & !7, 8).unwrap();
    assert!(allocator.allocate(payload).is_err());
    assert_eq!(state.reserves.load(Ordering::Relaxed), 0);
    assert_eq!(state.admissions.load(Ordering::Relaxed), 0);
    assert_eq!(
        resources.failure(),
        Some(Error::new(FailureKind::Quota, LimitKind::Capacity))
    );
    drop(allocator);
    resources.validate_retired().unwrap();
}
