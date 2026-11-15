// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE file in the project root for license information.
// ---------------------------------------------------------------------------------------------

use super::*;
use markdown_core::source::SourceDocument;
use std::cell::Cell;

#[test]
fn null_destroy_obeys_the_error_buffer_protocol() {
    let mut diagnostic = WireError {
        status: crate::STATUS_INVALID_ARGUMENT,
        ..WireError::default()
    };
    // SAFETY: null owns nothing; the error prefix and full storage are valid.
    unsafe {
        assert_eq!(
            notepads_markdown_svg_work_destroy(ptr::null_mut(), &mut diagnostic),
            crate::STATUS_OK
        );
        assert_eq!(diagnostic, WireError::default());
        diagnostic.header.struct_size = 8;
        let before = diagnostic;
        assert_eq!(
            notepads_markdown_svg_work_destroy(ptr::null_mut(), &mut diagnostic),
            crate::STATUS_ABI_MISMATCH
        );
        assert_eq!(diagnostic, before);
    }
}

thread_local! {
    static FAIL_ARENA: Cell<bool> = const { Cell::new(false) };
    static PROBE: Cell<FreeProbe> = const { Cell::new(FreeProbe::empty()) };
    static ALLOCATION_TRACE_ACTIVE: Cell<bool> = const { Cell::new(false) };
    static ALLOCATION_TRACE: Cell<AllocationTrace> = const { Cell::new(AllocationTrace::empty()) };
}

#[derive(Clone, Copy)]
struct AllocationTrace {
    events: [(usize, usize, usize, u64); 128],
    written: usize,
    overflow: bool,
}
impl AllocationTrace {
    const fn empty() -> Self {
        Self {
            events: [(0, 0, 0, 0); 128],
            written: 0,
            overflow: false,
        }
    }
}

/// Only this thread is observed. No ledger, mutex or allocating container is
/// accessed from the allocation callback; these records prove allocation
/// identity/order, not payment at the instant allocation begins.
pub(super) fn arm_system_allocation_trace() {
    ALLOCATION_TRACE.with(|trace| trace.set(AllocationTrace::empty()));
    ALLOCATION_TRACE_ACTIVE.with(|active| active.set(true));
}

pub(super) fn take_system_allocation_trace() -> (usize, bool, [(usize, usize, usize, u64); 128]) {
    ALLOCATION_TRACE_ACTIVE.with(|active| active.set(false));
    let trace = ALLOCATION_TRACE.with(|trace| trace.replace(AllocationTrace::empty()));
    (trace.written, trace.overflow, trace.events)
}

fn record_system_allocation(pointer: *mut u8, bytes: usize, alignment: usize) {
    if pointer.is_null() {
        return;
    }
    let _ = ALLOCATION_TRACE_ACTIVE.try_with(|active| {
        if !active.get() {
            return;
        }
        let _ = ALLOCATION_TRACE.try_with(|record| {
            let mut trace = record.get();
            if trace.written < trace.events.len() {
                trace.events[trace.written] =
                    (pointer as usize, bytes, alignment, trace.written as u64);
                trace.written += 1;
            } else {
                trace.overflow = true;
            }
            record.set(trace);
        });
    });
}

pub(super) fn take_arena_failure() -> bool {
    FAIL_ARENA.with(|value| value.replace(false))
}

#[derive(Clone, Copy)]
struct FreeEvent {
    address: usize,
    bytes: usize,
    root: u64,
    control: u64,
}
impl FreeEvent {
    const EMPTY: Self = Self {
        address: 0,
        bytes: 0,
        root: 0,
        control: 0,
    };
}
#[derive(Clone, Copy)]
struct FreeProbe {
    root: *const SourceBudget,
    policy: *const ImageResourcePolicy,
    watched: [usize; 3],
    events: [FreeEvent; 3],
    written: usize,
}
impl FreeProbe {
    const fn empty() -> Self {
        Self {
            root: ptr::null(),
            policy: ptr::null(),
            watched: [0; 3],
            events: [FreeEvent::EMPTY; 3],
            written: 0,
        }
    }
}
struct ProbeGuard;
impl Drop for ProbeGuard {
    fn drop(&mut self) {
        let _ = PROBE.try_with(|value| value.set(FreeProbe::empty()));
    }
}

/// # Safety
/// Both real ledgers remain live until take_retained_probe; no watched
/// allocation may be accessed after its real deallocation.
pub(super) unsafe fn arm_retained_probe(
    root: *const SourceBudget,
    policy: *const ImageResourcePolicy,
    watched: [usize; 3],
) {
    PROBE.with(|record| {
        record.set(FreeProbe {
            root,
            policy,
            watched,
            ..FreeProbe::empty()
        })
    });
}

pub(super) fn take_retained_probe() -> (usize, [(usize, usize, u64, u64); 3]) {
    let value = PROBE.with(|record| record.replace(FreeProbe::empty()));
    (
        value.written,
        value
            .events
            .map(|event| (event.address, event.bytes, event.root, event.control)),
    )
}

struct ObservedSystem;
#[global_allocator]
static TEST_ALLOCATOR: ObservedSystem = ObservedSystem;
// SAFETY: all operations preserve System's allocation/deallocation semantics.
// Observation uses an allocation-free, thread-local Copy record only. Test
// references outlive the explicitly armed window; unrelated threads are idle.
unsafe impl std::alloc::GlobalAlloc for ObservedSystem {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        let pointer = unsafe { std::alloc::GlobalAlloc::alloc(&std::alloc::System, layout) };
        record_system_allocation(pointer, layout.size(), layout.align());
        pointer
    }
    unsafe fn alloc_zeroed(&self, layout: Layout) -> *mut u8 {
        let pointer = unsafe { std::alloc::GlobalAlloc::alloc_zeroed(&std::alloc::System, layout) };
        record_system_allocation(pointer, layout.size(), layout.align());
        pointer
    }
    unsafe fn realloc(&self, pointer: *mut u8, layout: Layout, size: usize) -> *mut u8 {
        let result =
            unsafe { std::alloc::GlobalAlloc::realloc(&std::alloc::System, pointer, layout, size) };
        record_system_allocation(result, size, layout.align());
        result
    }
    unsafe fn dealloc(&self, pointer: *mut u8, layout: Layout) {
        let _ = PROBE.try_with(|record| {
            let mut value = record.get();
            if !value.root.is_null()
                && value.watched.contains(&(pointer as usize))
                && value.written < value.events.len()
            {
                // SAFETY: armed test pins both immutable roots through disarm;
                // deallocation must run outside the policy's short ledger lock.
                value.events[value.written] = FreeEvent {
                    address: pointer as usize,
                    bytes: layout.size(),
                    root: unsafe { (*value.root).usage().auxiliary_capacity },
                    control: unsafe { (*value.policy).svg_work_bytes() },
                };
                value.written += 1;
                record.set(value);
            }
        });
        unsafe { std::alloc::GlobalAlloc::dealloc(&std::alloc::System, pointer, layout) }
    }
}

fn error() -> WireError {
    WireError {
        header: WireHeader::of::<WireError>(),
        ..Default::default()
    }
}

struct Fixture {
    accounts: [*mut Owned<SvgWork>; 2],
    requests: [*mut Owned<ImageRequest>; 2],
    policy: *mut Owned<ImagePolicy>,
    model: *mut Owned<CoreModel>,
    cancel: *mut Owned<CoreCancel>,
    budget: SourceBudget,
}

impl Fixture {
    fn new() -> Self {
        Self::with_revision(1)
    }

    fn with_revision(revision: u64) -> Self {
        let mut e = error();
        let mut cancel = ptr::null_mut();
        // SAFETY: unique handles/PODs owned by this test, all regions independent.
        unsafe {
            assert_eq!(
                notepads_markdown_preview_cancel_create(&mut cancel, &mut e),
                0
            );
            let budget = (*cancel).value.budget.clone();
            let mut op = budget.begin((*cancel).value.token.clone());
            let key = SourceKey::new([7; 16], revision).unwrap();
            let document = SourceDocument::new(key, b"![svg](image)", &mut op).unwrap();
            let accepted = AcceptedModel::parse(
                document.snapshot(),
                ParseConfig::new(Profile::G3),
                1,
                &mut op,
            )
            .unwrap();
            let slot = Slot::new(op.boundary_work()).unwrap();
            let model = slot.commit(CoreModel { model: accepted });
            let mut policy = ptr::null_mut();
            assert_eq!(
                notepads_markdown_image_policy_create(&mut policy, &mut e),
                0
            );
            let epoch = ImageEpochDescriptor {
                header: ImageHeader::of::<ImageEpochDescriptor>(),
                epoch: ImageEpoch {
                    source: WireSource {
                        document: [7; 16],
                        revision,
                    },
                    source_generation: revision,
                    provider_generation: 1,
                },
            };
            assert_eq!(
                notepads_markdown_image_policy_begin(policy, &epoch, &mut e),
                0
            );
            let mut requests = [ptr::null_mut(); 2];
            for (i, request) in requests.iter_mut().enumerate() {
                desire_image_for_test(policy, i as u32, &mut e);
                let mut info = ImageRequestInfo {
                    header: ImageHeader::of::<ImageRequestInfo>(),
                    ..Default::default()
                };
                assert_eq!(
                    notepads_markdown_image_policy_start(
                        policy, i as u32, 2, request, &mut info, &mut e
                    ),
                    0
                );
            }
            Self {
                accounts: [ptr::null_mut(); 2],
                requests,
                policy,
                model,
                cancel,
                budget,
            }
        }
    }

    fn create(&mut self, index: usize) {
        // SAFETY: fixture owns the original request/model/cancel and output.
        unsafe {
            assert_eq!(
                notepads_markdown_svg_work_create(
                    1,
                    self.requests[index],
                    self.model,
                    self.cancel,
                    &mut self.accounts[index],
                    &mut error()
                ),
                0
            );
        }
    }

    fn read(&self, index: usize) -> SvgWorkSnapshot {
        let mut output = SvgWorkSnapshot {
            header: SvgWorkHeader {
                abi_version: 1,
                struct_size: size_of::<SvgWorkSnapshot>() as u32,
            },
            ..Default::default()
        };
        // SAFETY: no test overlaps account operations.
        unsafe {
            assert_eq!(
                notepads_markdown_svg_work_read(self.accounts[index], &mut output, &mut error()),
                0
            );
        }
        output
    }

    fn reserve(&self, index: usize, bytes: u64) -> SvgWorkToken {
        let mut token = SvgWorkToken::default();
        // SAFETY: exclusive sequential access, independent full output/error.
        unsafe {
            assert_eq!(
                notepads_markdown_svg_work_reserve(
                    self.accounts[index],
                    0,
                    bytes,
                    &mut token,
                    &mut error()
                ),
                0
            );
        }
        token
    }

    fn free_token(&self, index: usize, token: SvgWorkToken) {
        // SAFETY: these tests either never allocate for this token or explicitly
        // physically deallocate its exact payload before reaching this helper.
        unsafe {
            assert_eq!(
                notepads_markdown_svg_work_release_after_free(
                    self.accounts[index],
                    &token,
                    &mut error()
                ),
                0
            );
        }
    }

    fn destroy(&mut self, index: usize) {
        // SAFETY: no live payload/ticket remains in successful test teardown.
        unsafe {
            assert_eq!(
                notepads_markdown_svg_work_close(self.accounts[index], &mut error()),
                0
            );
            assert_eq!(
                notepads_markdown_svg_work_destroy(self.accounts[index], &mut error()),
                0
            );
        }
        self.accounts[index] = ptr::null_mut();
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        // SAFETY: handles are unique and independent; all test tokens retired.
        unsafe {
            for account in self.accounts {
                if !account.is_null() {
                    assert_eq!(notepads_markdown_svg_work_close(account, &mut error()), 0);
                    assert_eq!(notepads_markdown_svg_work_destroy(account, &mut error()), 0);
                }
            }
            for request in self.requests {
                assert_eq!(notepads_markdown_image_request_destroy(request), 0);
            }
            assert_eq!(notepads_markdown_image_policy_destroy(self.policy), 0);
            assert_eq!(notepads_markdown_core_model_destroy(self.model), 0);
            assert_eq!(notepads_markdown_core_cancel_destroy(self.cancel), 0);
        }
    }
}

#[test]
fn exact_owned_and_arena_use_the_captured_model_root_until_physical_teardown() {
    let _process = crate::core::images::serialize_process_image_tests();
    let mut f = Fixture::new();
    let baseline = f.budget.usage().auxiliary_capacity;
    f.create(0);
    let initial = f.read(0);
    assert_eq!(initial.header_bytes, size_of::<Owned<SvgWork>>() as u64);
    assert_eq!(initial.job_bytes, initial.header_bytes);
    assert_eq!(initial.root_work_bytes, 0);
    assert_eq!(
        f.budget.usage().auxiliary_capacity - baseline,
        initial.header_bytes
    );
    let token = f.reserve(0, 4096);
    let reserved = f.read(0);
    assert_eq!(reserved.root_work_bytes, reserved.arena_bytes + 4096);
    assert_eq!(
        f.budget.usage().auxiliary_capacity - baseline,
        reserved.job_bytes
    );
    // SAFETY: the full exact allocation is already paid by both real roots.
    unsafe {
        let layout = Layout::from_size_align(4096, 8).unwrap();
        let payload = alloc(layout);
        assert!(!payload.is_null());
        assert_eq!(
            notepads_markdown_svg_work_admit(f.accounts[0], &token, 4096, &mut error()),
            0
        );
        assert_eq!(f.read(0).job_bytes, reserved.job_bytes);
        assert_eq!(
            notepads_markdown_svg_work_close(f.accounts[0], &mut error()),
            0
        );
        // Still-live allocation models a failed free: don't call release.
        assert_ne!(
            notepads_markdown_svg_work_destroy(f.accounts[0], &mut error()),
            0
        );
        assert_eq!(f.read(0).job_bytes, reserved.job_bytes);
        assert_eq!(
            f.budget.usage().auxiliary_capacity - baseline,
            reserved.job_bytes
        );
        dealloc(payload, layout);
        assert_eq!(f.read(0).job_bytes, reserved.job_bytes);
    }
    f.free_token(0, token);
    let after_payload = f.read(0);
    assert_eq!(
        after_payload.job_bytes,
        after_payload.header_bytes + after_payload.arena_bytes
    );
    f.destroy(0);
    assert_eq!(f.budget.usage().auxiliary_capacity, baseline);
}

#[test]
fn many_work_tickets_do_not_consume_legacy_image_reservations_and_survive_growth() {
    let _process = crate::core::images::serialize_process_image_tests();
    let mut f = Fixture::new();
    f.create(0);
    let tokens: Vec<_> = (0..1025).map(|_| f.reserve(0, 1)).collect();
    assert_eq!(f.read(0).tickets, 1025);
    // SAFETY: the test owns this policy; no operation is concurrent.
    unsafe {
        assert_eq!((&(*f.policy).value.owner).policy.usage().reservations, 0);
    }
    for token in tokens {
        f.free_token(0, token);
    }
    assert_eq!(f.read(0).pending, 0);
    f.destroy(0);
}

#[test]
fn token_identity_generation_and_exhaustion_reject_stale_foreign_and_duplicate_releases() {
    let _process = crate::core::images::serialize_process_image_tests();
    let mut f = Fixture::new();
    f.create(0);
    f.create(1);
    let token = f.reserve(0, 16);
    let other = f.reserve(1, 16);
    // SAFETY: readable tokens and exclusive handles, failed calls preserve state.
    unsafe {
        assert_ne!(
            notepads_markdown_svg_work_admit(f.accounts[1], &token, 16, &mut error()),
            0
        );
        assert_eq!(f.read(1).tickets, 1);
    }
    f.free_token(0, token);
    let second = f.reserve(0, 16);
    assert_eq!(second.slot, token.slot);
    assert_ne!(second.generation, token.generation);
    unsafe {
        assert_ne!(
            notepads_markdown_svg_work_release_after_free(f.accounts[0], &token, &mut error()),
            0
        );
        // Force the otherwise unreachable 2^32 reuse boundary in the actual row.
        (*f.accounts[0])
            .value
            .arena
            .get_mut(second.slot)
            .unwrap()
            .generation = u32::MAX;
    }
    let final_token = SvgWorkToken {
        generation: u32::MAX,
        ..second
    };
    f.free_token(0, final_token);
    let next = f.reserve(0, 1);
    assert_ne!(next.slot, final_token.slot);
    f.free_token(0, next);
    f.free_token(1, other);
}

#[test]
fn shared_control_limit_and_failed_admit_keep_original_ticket_and_success_outputs() {
    let _process = crate::core::images::serialize_process_image_tests();
    let mut f = Fixture::new();
    f.create(0);
    f.create(1);
    let a = f.reserve(0, 80 * 1024 * 1024);
    let b = f.reserve(1, 32 * 1024 * 1024);
    let before = f.read(1);
    let sentinel = SvgWorkToken {
        account_nonce: 999,
        slot: 777,
        generation: 888,
    };
    let mut output = sentinel;
    // SAFETY: no payload is allocated for these reservation-only boundary cases.
    unsafe {
        assert_ne!(
            notepads_markdown_svg_work_reserve(
                f.accounts[1],
                0,
                17 * 1024 * 1024,
                &mut output,
                &mut error()
            ),
            0
        );
        assert_eq!(output, sentinel);
        assert_eq!(f.read(1).job_bytes, before.job_bytes);
        assert_ne!(
            notepads_markdown_svg_work_admit(f.accounts[1], &b, 64 * 1024 * 1024, &mut error()),
            0
        );
        assert_eq!(f.read(1).pending, 1);
        assert_eq!(f.read(1).root_work_bytes, before.root_work_bytes);
    }
    f.free_token(0, a);
    f.free_token(1, b);
}

#[test]
fn cancellation_and_source_retirement_preserve_existing_tickets_but_deny_new_work() {
    let _process = crate::core::images::serialize_process_image_tests();
    let mut f = Fixture::new();
    f.create(0);
    let token = f.reserve(0, 128);
    let before = f.read(0);
    // SAFETY: source/cancel calls have independent PODs, no work overlaps.
    unsafe {
        let epoch = ImageEpochDescriptor {
            header: ImageHeader::of::<ImageEpochDescriptor>(),
            epoch: ImageEpoch {
                source: WireSource {
                    document: [7; 16],
                    revision: 2,
                },
                source_generation: 2,
                provider_generation: 1,
            },
        };
        assert_eq!(
            notepads_markdown_image_policy_begin(f.policy, &epoch, &mut error()),
            0
        );
        assert_eq!(notepads_markdown_core_cancel_request(f.cancel), 0);
        let mut output = SvgWorkToken::default();
        assert_ne!(
            notepads_markdown_svg_work_reserve(f.accounts[0], 0, 1, &mut output, &mut error()),
            0
        );
        assert_eq!(f.read(0).job_bytes, before.job_bytes);
        assert_eq!(
            notepads_markdown_svg_work_close(f.accounts[0], &mut error()),
            0
        );
    }
    f.free_token(0, token);
    f.destroy(0);
}

#[test]
fn create_validates_model_cancel_source_one_shot_and_prefix_before_allocating() {
    let _process = crate::core::images::serialize_process_image_tests();
    let mut f = Fixture::new();
    let foreign = Fixture::new();
    let baseline = f.budget.usage().auxiliary_capacity;
    let mut out = ptr::null_mut();
    // SAFETY: all fixtures keep separate immutable roots/handles and outputs live.
    unsafe {
        assert_eq!(
            notepads_markdown_svg_work_create(
                0,
                f.requests[0],
                f.model,
                f.cancel,
                &mut out,
                &mut error()
            ),
            STATUS_ABI_MISMATCH
        );
        assert!(out.is_null());
        assert_ne!(
            notepads_markdown_svg_work_create(
                1,
                f.requests[0],
                f.model,
                foreign.cancel,
                &mut out,
                &mut error()
            ),
            0
        );
        assert!(out.is_null());
        assert_eq!(f.budget.usage().auxiliary_capacity, baseline);
    }
    f.create(0);
    unsafe {
        assert_ne!(
            notepads_markdown_svg_work_create(
                1,
                f.requests[0],
                f.model,
                f.cancel,
                &mut out,
                &mut error()
            ),
            0
        );
        assert!(out.is_null());
        let mut bad = SvgWorkSnapshot {
            job_bytes: 42,
            ..Default::default()
        };
        assert_eq!(
            notepads_markdown_svg_work_read(f.accounts[0], &mut bad, &mut error()),
            STATUS_ABI_MISMATCH
        );
        assert_eq!(bad.job_bytes, 42);
    }
    f.destroy(0);
    unsafe {
        assert_ne!(
            notepads_markdown_svg_work_create(
                1,
                f.requests[0],
                f.model,
                f.cancel,
                &mut out,
                &mut error()
            ),
            0
        );
        assert!(out.is_null());
    }
}

#[test]
fn actual_system_deallocation_observes_payload_arena_and_owned_still_fully_funded() {
    let _process = crate::core::images::serialize_process_image_tests();
    let mut f = Fixture::new();
    let baseline = f.budget.usage().auxiliary_capacity;
    f.create(0);
    let token = f.reserve(0, 32);
    let before = f.read(0);
    // SAFETY: exact payload paid before allocation; fixture roots remain pinned
    // throughout the allocation-free observer window.
    unsafe {
        let layout = Layout::from_size_align(32, 8).unwrap();
        let payload = alloc(layout);
        assert!(!payload.is_null());
        assert_eq!(
            notepads_markdown_svg_work_admit(f.accounts[0], &token, 32, &mut error()),
            0
        );
        let watched = [
            payload as usize,
            (*f.accounts[0]).value.arena.pointer.as_ptr() as usize,
            f.accounts[0] as usize,
        ];
        let _probe_guard = ProbeGuard;
        PROBE.with(|record| {
            record.set(FreeProbe {
                root: &f.budget,
                policy: &(&(*f.policy).value.owner).policy,
                watched,
                ..FreeProbe::empty()
            })
        });
        dealloc(payload, layout);
        f.free_token(0, token);
        f.destroy(0);
        let observed = PROBE.with(|record| record.replace(FreeProbe::empty()));
        assert_eq!(observed.written, 3);
        for (event, address) in observed.events.iter().zip(watched) {
            assert_eq!(event.address, address);
        }
        assert_eq!(observed.events[0].bytes, 32);
        assert_eq!(observed.events[0].control, before.job_bytes);
        assert_eq!(observed.events[0].root, baseline + before.job_bytes);
        assert_eq!(observed.events[1].bytes as u64, before.arena_bytes);
        assert_eq!(
            observed.events[1].control,
            before.header_bytes + before.arena_bytes
        );
        assert_eq!(
            observed.events[1].root,
            baseline + before.header_bytes + before.arena_bytes
        );
        assert_eq!(observed.events[2].bytes as u64, before.header_bytes);
        assert_eq!(observed.events[2].control, before.header_bytes);
        assert_eq!(observed.events[2].root, baseline + before.header_bytes);
        assert_eq!((&(*f.policy).value.owner).policy.svg_work_bytes(), 0);
        assert_eq!(f.budget.usage().auxiliary_capacity, baseline);
    }
}

#[test]
fn failed_exact_arena_growth_keeps_old_rows_and_balances_intact() {
    let _process = crate::core::images::serialize_process_image_tests();
    let mut f = Fixture::new();
    f.create(0);
    let tokens: Vec<_> = (0..8).map(|_| f.reserve(0, 4)).collect();
    let before = f.read(0);
    let root = f.budget.usage().auxiliary_capacity;
    FAIL_ARENA.with(|value| value.set(true));
    let mut output = SvgWorkToken {
        account_nonce: 12,
        slot: 13,
        generation: 14,
    };
    let sentinel = output;
    // SAFETY: allocator seam rejects before actual allocation, after real dual
    // reservation; this is not a fabricated System allocator null/OOM guarantee.
    unsafe {
        assert_ne!(
            notepads_markdown_svg_work_reserve(f.accounts[0], 0, 1, &mut output, &mut error()),
            0
        );
    }
    assert_eq!(output, sentinel);
    let after = f.read(0);
    assert_eq!(
        (
            after.job_bytes,
            after.root_work_bytes,
            after.arena_bytes,
            after.tickets
        ),
        (
            before.job_bytes,
            before.root_work_bytes,
            before.arena_bytes,
            before.tickets
        )
    );
    assert_eq!(f.budget.usage().auxiliary_capacity, root);
    for token in tokens {
        f.free_token(0, token);
    }
}

#[test]
fn wrong_source_and_real_header_root_failure_leave_creation_retryable_and_layout_intact() {
    let _process = crate::core::images::serialize_process_image_tests();
    let mut f = Fixture::new();
    let foreign_source = Fixture::with_revision(2);
    let baseline = f.budget.usage().auxiliary_capacity;
    let mut output = ptr::null_mut();
    // SAFETY: two live models with distinct source revisions; each model has
    // its correct root cancel. The request remains tied to the first source.
    unsafe {
        assert_ne!(
            notepads_markdown_svg_work_create(
                1,
                f.requests[0],
                foreign_source.model,
                foreign_source.cancel,
                &mut output,
                &mut error()
            ),
            0
        );
        assert!(output.is_null());
        assert_eq!((&(*f.policy).value.owner).policy.svg_work_bytes(), 0);
    }
    let header = size_of::<Owned<SvgWork>>() as u64;
    let filler = f
        .budget
        .reserve_resource_capacity(f.budget.limits().auxiliary_capacity - baseline - (header - 1))
        .unwrap();
    let before_failure = f.budget.usage().auxiliary_capacity;
    // SAFETY: real model-root capacity is one byte too small for the exact
    // Owned allocation; bootstrap's policy/header and Creating flag must roll back.
    unsafe {
        assert_ne!(
            notepads_markdown_svg_work_create(
                1,
                f.requests[0],
                f.model,
                f.cancel,
                &mut output,
                &mut error()
            ),
            0
        );
        assert!(output.is_null());
        assert_eq!((&(*f.policy).value.owner).policy.svg_work_bytes(), 0);
    }
    assert_eq!(f.budget.usage().auxiliary_capacity, before_failure);
    drop(filler);
    f.create(0);
    f.destroy(0);
    assert_eq!(f.budget.usage().auxiliary_capacity, baseline);
    assert_eq!(IMAGE_ABI_VERSION, 2);
    assert_eq!(size_of::<ImageMetadataWire>(), 80);
    assert_eq!(size_of::<ImageSnapshot>(), 88);
    assert_eq!(size_of::<ImageEntry>(), 192);
    assert_eq!(size_of::<ImageCompletion>(), 136);
}

mod access_tests;

mod resources_bridge_tests;

mod renderer_job_tests;
