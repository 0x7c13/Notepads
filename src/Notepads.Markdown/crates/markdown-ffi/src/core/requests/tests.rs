// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE file in the project root for license information.
// ---------------------------------------------------------------------------------------------

use super::*;

struct Fixture {
    cancel: *mut Owned<CoreCancel>,
    session: *mut Owned<CoreSession>,
    root: SourceBudget,
}

#[test]
fn extension_snapshot_transport_is_transactional_and_can_clear_flags() {
    let fixture = Fixture::new();
    let text = b"$x$";
    for (revision, extensions) in [
        (1, 0),
        (2, 1),
        (3, 3),
        (4, 4),
        (5, 7),
        (6, 8),
        (7, 32),
        (8, 127),
    ] {
        let request = fixture.start();
        let mut state = result();
        let mut error = WireError::default();
        // SAFETY: serial fixture, unique handles and fully initialized borrowed PODs.
        unsafe {
            assert_eq!(
                notepads_markdown_core_job_replace_utf8(
                    fixture.session,
                    request.job,
                    text.as_ptr(),
                    text.len() as u64,
                    &descriptor(revision),
                    extensions,
                    &mut state,
                    &mut error,
                ),
                0
            );
            assert_eq!(state.source_accepted, 1);
            assert_eq!(state.ready.extensions, extensions);
            assert_eq!(
                identity(state.ready).unwrap().config().extensions().bits(),
                extensions
            );
            let JobState::Ready(captured) = &(*request.job).value.state else {
                panic!("the source transaction must capture its exact full configuration");
            };
            assert_eq!(wire_identity(captured.identity()).extensions, extensions);
        }
    }
    let request = fixture.start();
    let mut state = result();
    let mut error = WireError::default();
    // SAFETY: an invalid mask is rejected before touching the same reusable request.
    unsafe {
        assert_eq!(
            notepads_markdown_core_job_replace_utf8(
                fixture.session,
                request.job,
                text.as_ptr(),
                text.len() as u64,
                &descriptor(99),
                0x8000_0000,
                &mut state,
                &mut error,
            ),
            STATUS_INVALID_ARGUMENT
        );
        assert!(matches!((*request.job).value.state, JobState::Preparing(_)));
        assert_eq!(
            (*fixture.session)
                .value
                .session
                .snapshot()
                .unwrap()
                .key()
                .revision(),
            8
        );
        assert_eq!(
            notepads_markdown_core_job_replace_utf16(
                fixture.session,
                request.job,
                [0x24_u16, 0x78, 0x24].as_ptr(),
                3,
                &descriptor(9),
                0,
                &mut state,
                &mut error,
            ),
            0
        );
        assert_eq!(state.ready.extensions, 0);
    }
}
impl Fixture {
    fn new() -> Self {
        let mut cancel = ptr::null_mut();
        let mut session = ptr::null_mut();
        let mut error = WireError::default();
        // SAFETY: initialized nonoverlapping outputs, serial live handles.
        unsafe {
            assert_eq!(
                notepads_markdown_core_cancel_create(ptr::null(), &mut cancel, &mut error),
                0
            );
            assert_eq!(
                notepads_markdown_core_session_create(cancel, 1, 0, &mut session, &mut error),
                0
            );
            Self {
                cancel,
                session,
                root: (*cancel).value.budget.clone(),
            }
        }
    }
    fn start(&self) -> Request {
        let mut job = ptr::null_mut();
        let mut cancel = ptr::null_mut();
        let mut error = WireError::default();
        // SAFETY: fixture session is used serially on this test thread.
        unsafe {
            assert_eq!(
                notepads_markdown_core_session_start_request(
                    self.session,
                    0,
                    &mut job,
                    &mut cancel,
                    &mut error
                ),
                0
            );
        }
        Request { job, cancel }
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        // SAFETY: lexical request/model guards end before this fixture.
        unsafe {
            assert_eq!(notepads_markdown_core_session_destroy(self.session), 0);
            assert_eq!(notepads_markdown_core_cancel_destroy(self.cancel), 0);
        }
        let usage = self.root.usage();
        assert_eq!(
            (
                usage.source_capacity,
                usage.auxiliary_capacity,
                usage.source_owners
            ),
            (0, 0, 0)
        );
    }
}
struct Request {
    job: *mut Owned<CoreJob>,
    cancel: *mut Owned<CoreCancel>,
}
impl Drop for Request {
    fn drop(&mut self) {
        // SAFETY: synchronous calls have ended; these are uniquely owned handles.
        unsafe {
            assert_eq!(notepads_markdown_core_job_destroy(self.job), 0);
            assert_eq!(notepads_markdown_core_cancel_destroy(self.cancel), 0);
        }
    }
}
fn result() -> WireSubmitResult {
    WireSubmitResult {
        header: WireHeader::of::<WireSubmitResult>(),
        ..Default::default()
    }
}
fn descriptor(revision: u64) -> WireSnapshot {
    WireSnapshot {
        header: WireHeader::of::<WireSnapshot>(),
        source: WireSource {
            document: [37; 16],
            revision,
        },
        profile: 1,
        reserved: 0,
    }
}
unsafe fn replace(f: &Fixture, r: &Request, text: &[u8], revision: u64) -> WireSubmitResult {
    let mut state = result();
    let mut error = WireError::default();
    // SAFETY: borrowed text and descriptor are valid through this synchronous call.
    unsafe {
        assert_eq!(
            notepads_markdown_core_job_replace_utf8(
                f.session,
                r.job,
                text.as_ptr(),
                text.len() as u64,
                &descriptor(revision),
                0,
                &mut state,
                &mut error
            ),
            0,
            "{error:?}"
        );
    }
    state
}

#[test]
fn abi3_layouts_have_exact_offsets_on_the_current_target() {
    use std::mem::offset_of;
    assert_eq!(CORE_ABI_VERSION, 3);
    assert_eq!(
        (
            size_of::<WireSubmitResult>(),
            align_of::<WireSubmitResult>()
        ),
        (96, 8)
    );
    assert_eq!(offset_of!(WireSubmitResult, header), 0);
    assert_eq!(offset_of!(WireSubmitResult, source), 8);
    assert_eq!(offset_of!(WireSubmitResult, state_valid), 32);
    assert_eq!(offset_of!(WireSubmitResult, has_source), 36);
    assert_eq!(offset_of!(WireSubmitResult, needs_resync), 40);
    assert_eq!(offset_of!(WireSubmitResult, source_accepted), 44);
    assert_eq!(offset_of!(WireSubmitResult, has_ready), 48);
    assert_eq!(offset_of!(WireSubmitResult, reserved), 52);
    assert_eq!(offset_of!(WireSubmitResult, ready), 56);
    assert_eq!(
        (size_of::<WireEditBatch>(), align_of::<WireEditBatch>()),
        (72, 8)
    );
    assert_eq!(offset_of!(WireEditBatch, header), 0);
    assert_eq!(offset_of!(WireEditBatch, before), 8);
    assert_eq!(offset_of!(WireEditBatch, after), 32);
    assert_eq!(offset_of!(WireEditBatch, count), 56);
    assert_eq!(offset_of!(WireEditBatch, row_size), 64);
    assert_eq!(offset_of!(WireEditBatch, reserved), 68);
    assert_eq!(
        (size_of::<WireEditRow>(), align_of::<WireEditRow>()),
        (32, 8)
    );
    assert_eq!(offset_of!(WireEditRow, start), 0);
    assert_eq!(offset_of!(WireEditRow, end), 8);
    assert_eq!(offset_of!(WireEditRow, replacement_offset), 16);
    assert_eq!(offset_of!(WireEditRow, replacement_length), 24);
}

#[test]
fn initially_cancelled_start_has_no_outputs_and_no_request_capacity() {
    let f = Fixture::new();
    let before = f.root.usage();
    let mut job = ptr::dangling_mut();
    let mut cancel = ptr::dangling_mut();
    let mut error = WireError::default();
    // SAFETY: sentinels are output values, never dereferenced as handles.
    unsafe {
        assert_eq!(
            notepads_markdown_core_session_start_request(
                f.session,
                1,
                &mut job,
                &mut cancel,
                &mut error
            ),
            4
        );
    }
    assert!(job.is_null() && cancel.is_null());
    assert_eq!(f.root.usage().auxiliary_capacity, before.auxiliary_capacity);
}

#[test]
fn cancellation_before_result_admission_returns_no_state_without_source_work() {
    let f = Fixture::new();
    let r = f.start();
    let mut state = result();
    let mut error = WireError::default();
    // SAFETY: the same fresh flag is canceled before the first prepare charge.
    unsafe {
        assert_eq!(notepads_markdown_core_cancel_request(r.cancel), 0);
        assert_eq!(
            notepads_markdown_core_job_replace_utf8(
                f.session,
                r.job,
                b"never".as_ptr(),
                5,
                &descriptor(1),
                0,
                &mut state,
                &mut error
            ),
            4
        );
        assert_eq!(state, result());
        assert!((*f.session).value.session.snapshot().is_none());
        assert_eq!(notepads_markdown_core_job_run(r.job, &mut error), 4);
    }
}

#[test]
fn foreign_scope_has_no_state_but_same_session_stale_origin_reports_current_state() {
    let f = Fixture::new();
    let wrong = f.start();
    let mut other = ptr::null_mut();
    let mut error = WireError::default();
    let mut state = result();
    // SAFETY: two distinct same-root sessions, no overlapping mutable borrows.
    unsafe {
        assert_eq!(
            notepads_markdown_core_session_create(f.cancel, 1, 0, &mut other, &mut error),
            0
        );
        assert_eq!(
            notepads_markdown_core_job_replace_utf8(
                other,
                wrong.job,
                b"wrong".as_ptr(),
                5,
                &descriptor(1),
                0,
                &mut state,
                &mut error
            ),
            10
        );
        assert_eq!(state, result());
        assert_eq!(notepads_markdown_core_session_destroy(other), 0);
    }
    let stale = f.start();
    let accepted = f.start();
    unsafe {
        replace(&f, &accepted, b"current", 1);
        assert_eq!(
            notepads_markdown_core_job_replace_utf8(
                f.session,
                stale.job,
                b"old".as_ptr(),
                3,
                &descriptor(2),
                0,
                &mut state,
                &mut error
            ),
            10
        );
        assert_eq!(
            (
                state.state_valid,
                state.has_source,
                state.source_accepted,
                state.has_ready
            ),
            (1, 1, 0, 0)
        );
        assert_eq!(state.source, descriptor(1).source);
        assert_eq!(
            (*f.session).value.session.snapshot().unwrap().text(),
            "current"
        );
    }
}

#[test]
fn invalid_prefix_and_short_error_leave_the_same_preparing_request_retriable() {
    let f = Fixture::new();
    let r = f.start();
    let mut state = result();
    state.source_accepted = 42;
    let before = state;
    let mut error = WireError::default();
    let prefix = WireHeader {
        abi_version: 2,
        struct_size: 8,
    };
    // SAFETY: rejection can read only the prefix, not the missing Snapshot payload.
    unsafe {
        assert_eq!(
            notepads_markdown_core_job_replace_utf8(
                f.session,
                r.job,
                ptr::null(),
                0,
                (&prefix as *const WireHeader).cast(),
                0,
                &mut state,
                &mut error
            ),
            2
        );
        assert_eq!(state, before);
        let mut short = WireHeader {
            abi_version: 3,
            struct_size: 8,
        };
        assert_eq!(
            notepads_markdown_core_job_capture_current(
                f.session,
                r.job,
                &mut state,
                (&mut short as *mut WireHeader).cast()
            ),
            2
        );
        assert_eq!(state, before);
        let state = replace(&f, &r, b"ok", 1);
        assert_eq!(
            (state.state_valid, state.source_accepted, state.has_ready),
            (1, 1, 1)
        );
    }
}

#[test]
fn strict_utf16_errors_are_typed_sticky_and_do_not_accept_source() {
    let f = Fixture::new();
    let r = f.start();
    let mut state = result();
    let mut error = WireError::default();
    let input = [0x4e2d, 0xd800];
    // SAFETY: live nonoverlapping handles, aligned borrowed u16 input.
    unsafe {
        assert_eq!(
            notepads_markdown_core_job_replace_utf16(
                f.session,
                r.job,
                input.as_ptr(),
                2,
                &descriptor(1),
                0,
                &mut state,
                &mut error
            ),
            26
        );
        assert_eq!(error.offset, 1);
        assert_eq!(
            (
                state.state_valid,
                state.has_source,
                state.source_accepted,
                state.has_ready
            ),
            (1, 0, 0, 0)
        );
        notepads_markdown_core_cancel_request(r.cancel);
        assert_eq!(notepads_markdown_core_job_run(r.job, &mut error), 26);
        assert_eq!(
            notepads_markdown_core_job_capture_current(f.session, r.job, &mut state, &mut error),
            26
        );
        assert_eq!(state, result());
        assert_eq!(error.offset, 1);
    }
}

#[test]
fn utf16_count_overflow_is_preflight_and_unicode_nul_crlf_are_preserved() {
    let f = Fixture::new();
    let r = f.start();
    let mut state = result();
    let mut error = WireError::default();
    // SAFETY: overflowing spans must be rejected before dereferencing the sentinel.
    unsafe {
        assert_eq!(
            notepads_markdown_core_job_replace_utf16(
                f.session,
                r.job,
                ptr::dangling(),
                u64::MAX,
                &descriptor(1),
                0,
                &mut state,
                &mut error
            ),
            1
        );
        let original = "中😀\0\r\n";
        let units: Vec<_> = original.encode_utf16().collect();
        assert_eq!(
            notepads_markdown_core_job_replace_utf16(
                f.session,
                r.job,
                units.as_ptr(),
                units.len() as u64,
                &descriptor(1),
                0,
                &mut state,
                &mut error
            ),
            0
        );
        assert_eq!(
            (*f.session).value.session.snapshot().unwrap().text(),
            original
        );
        assert_eq!(notepads_markdown_core_job_run(r.job, &mut error), 0);
        let mut model = ptr::null_mut();
        assert_eq!(
            notepads_markdown_core_job_install(f.session, r.job, &mut model, &mut error),
            0
        );
        assert_eq!((*model).value.model.source().text(), original);
        assert_eq!(notepads_markdown_core_model_destroy(model), 0);
    }
}

#[test]
fn source_commit_then_capture_failure_returns_truthful_source_accepted_state() {
    let f = Fixture::new();
    let r = f.start();
    let mut state = result();
    let mut error = WireError::default();
    // SAFETY: TLS control cancels only this call's actual flag after source success.
    unsafe {
        CANCEL_AFTER_SOURCE.with(|slot| *slot.borrow_mut() = Some((*r.cancel).value.token.clone()));
        assert_eq!(
            notepads_markdown_core_job_replace_utf8(
                f.session,
                r.job,
                b"accepted".as_ptr(),
                8,
                &descriptor(1),
                0,
                &mut state,
                &mut error
            ),
            4
        );
        assert_eq!(
            (
                state.state_valid,
                state.has_source,
                state.source_accepted,
                state.has_ready
            ),
            (1, 1, 1, 0)
        );
        assert_eq!(
            (*f.session).value.session.snapshot().unwrap().text(),
            "accepted"
        );
        assert_eq!(notepads_markdown_core_job_run(r.job, &mut error), 4);
    }
    let next = f.start();
    let mut state = result();
    unsafe {
        assert_eq!(
            notepads_markdown_core_job_capture_current(f.session, next.job, &mut state, &mut error),
            0
        );
    }
    assert_eq!((state.source_accepted, state.has_ready), (0, 1));
}

#[test]
fn output_admission_failure_returns_no_state_and_preserves_old_source() {
    let f = Fixture::new();
    let r = f.start();
    let mut state = result();
    let mut error = WireError::default();
    unsafe {
        replace(&f, &r, b"old", 1);
    }
    let new = f.start();
    // Reserve remaining capacity and force an original operation fault before prepare.
    // This is an actual resource rejection, not a replacement/fake operation.
    let mut blocker = f.root.begin_cancellable(false).unwrap();
    let hold = blocker
        .reserve_boundary(f.root.limits().auxiliary_capacity - f.root.usage().auxiliary_capacity)
        .unwrap();
    let mut allocation = ptr::null_mut();
    unsafe {
        assert_eq!(
            notepads_markdown_core_job_reserve_allocation(new.job, 1, &mut allocation, &mut error),
            9
        );
        assert!(allocation.is_null());
        assert_eq!(
            notepads_markdown_core_job_replace_utf8(
                f.session,
                new.job,
                b"new".as_ptr(),
                3,
                &descriptor(2),
                0,
                &mut state,
                &mut error
            ),
            9
        );
        assert_eq!(state, result());
        assert_eq!((*f.session).value.session.snapshot().unwrap().text(), "old");
    }
    drop(hold);
    drop(blocker);
}

#[test]
fn same_root_different_job_cannot_admit_an_older_allocation() {
    let f = Fixture::new();
    let first = f.start();
    let other = f.start();
    let mut error = WireError::default();
    let mut allocation = ptr::null_mut();
    unsafe {
        assert_eq!(
            notepads_markdown_core_job_reserve_allocation(
                first.job,
                64,
                &mut allocation,
                &mut error
            ),
            0
        );
        assert_eq!(
            notepads_markdown_core_job_admit_allocation(other.job, allocation, 80, &mut error),
            18
        );
        assert_eq!(notepads_markdown_core_job_run(other.job, &mut error), 18);
        assert_eq!(
            notepads_markdown_core_job_admit_allocation(first.job, allocation, 80, &mut error),
            0
        );
        drop(first); // Lease independently pins quota/scope past its job.
        assert_eq!(notepads_markdown_core_allocation_destroy(allocation), 0);
    }
}

#[test]
fn duplicate_admission_is_terminal_and_does_not_release_caller_capacity() {
    let f = Fixture::new();
    let r = f.start();
    let mut allocation = ptr::null_mut();
    let mut error = WireError::default();
    unsafe {
        assert_eq!(
            notepads_markdown_core_job_reserve_allocation(r.job, 64, &mut allocation, &mut error),
            0
        );
        assert_eq!(
            notepads_markdown_core_job_admit_allocation(r.job, allocation, 128, &mut error),
            0
        );
        let before = f.root.usage().auxiliary_capacity;
        assert_eq!(
            notepads_markdown_core_job_admit_allocation(r.job, allocation, 128, &mut error),
            5
        );
        assert!(f.root.usage().auxiliary_capacity >= 128);
        assert!(f.root.usage().auxiliary_capacity <= before); // Terminal retires the original flag/op.
        assert_eq!(notepads_markdown_core_allocation_destroy(allocation), 0);
    }
}

#[test]
fn insufficient_actual_capacity_and_native_allocation_failure_keep_first_fault() {
    let f = Fixture::new();
    let r = f.start();
    let mut allocation = ptr::null_mut();
    let mut error = WireError::default();
    unsafe {
        assert_eq!(
            notepads_markdown_core_job_reserve_allocation(r.job, 64, &mut allocation, &mut error),
            0
        );
        assert_eq!(
            notepads_markdown_core_job_admit_allocation(r.job, allocation, 63, &mut error),
            16
        );
        assert_eq!(
            notepads_markdown_core_job_allocation_failed(r.job, 64, &mut error),
            16
        );
        assert_eq!(notepads_markdown_core_allocation_destroy(allocation), 0);
    }
    let oom = f.start();
    unsafe {
        assert_eq!(
            notepads_markdown_core_job_allocation_failed(oom.job, 64, &mut error),
            7
        );
        notepads_markdown_core_cancel_request(oom.cancel);
        assert_eq!(notepads_markdown_core_job_run(oom.job, &mut error), 7);
    }
}

#[test]
fn edits_survive_ticket_replacement_and_missing_chain_is_explicit() {
    let f = Fixture::new();
    let a = f.start();
    let mut error = WireError::default();
    unsafe {
        replace(&f, &a, b"A", 1);
    }
    let batch = |before, after| WireEditBatch {
        header: WireHeader::of::<WireEditBatch>(),
        before: descriptor(before).source,
        after: descriptor(after).source,
        count: 1,
        row_size: 32,
        reserved: 0,
    };
    let mut state = result();
    let b = f.start();
    let row = WireEditRow {
        start: 1,
        end: 1,
        replacement_offset: 0,
        replacement_length: 1,
    };
    unsafe {
        assert_eq!(
            notepads_markdown_core_job_apply(
                f.session,
                b.job,
                &batch(1, 2),
                &row,
                b"B".as_ptr(),
                1,
                &mut state,
                &mut error
            ),
            0
        );
    }
    drop(b);
    let c = f.start();
    let row = WireEditRow {
        start: 2,
        end: 2,
        ..row
    };
    unsafe {
        assert_eq!(
            notepads_markdown_core_job_apply(
                f.session,
                c.job,
                &batch(2, 3),
                &row,
                b"C".as_ptr(),
                1,
                &mut state,
                &mut error
            ),
            0
        );
        assert_eq!((*f.session).value.session.snapshot().unwrap().text(), "ABC");
        assert_eq!(notepads_markdown_core_job_run(c.job, &mut error), 0);
        let mut model = ptr::null_mut();
        assert_eq!(
            notepads_markdown_core_job_install(f.session, c.job, &mut model, &mut error),
            0
        );
        assert_eq!((*model).value.model.source().text(), "ABC");
        notepads_markdown_core_model_destroy(model);
    }
    let missing = f.start();
    unsafe {
        assert_eq!(
            notepads_markdown_core_job_apply(
                f.session,
                missing.job,
                &batch(1, 4),
                &row,
                b"D".as_ptr(),
                1,
                &mut state,
                &mut error
            ),
            11
        );
        assert_eq!(
            (state.source_accepted, state.has_ready, state.needs_resync),
            (0, 0, 1)
        );
        assert_eq!((*f.session).value.session.snapshot().unwrap().text(), "ABC");
    }
}

#[test]
fn invalid_edit_blob_is_atomic_and_abi_preflight_does_not_consume_job() {
    let f = Fixture::new();
    let a = f.start();
    unsafe {
        replace(&f, &a, b"A", 1);
    }
    let r = f.start();
    let mut error = WireError::default();
    let mut state = result();
    let mut batch = WireEditBatch {
        header: WireHeader::of::<WireEditBatch>(),
        before: descriptor(1).source,
        after: descriptor(2).source,
        count: u64::MAX,
        row_size: 32,
        reserved: 0,
    };
    unsafe {
        assert_eq!(
            notepads_markdown_core_job_apply(
                f.session,
                r.job,
                &batch,
                ptr::dangling(),
                ptr::null(),
                0,
                &mut state,
                &mut error
            ),
            1
        );
        batch.count = 1;
        let row = WireEditRow {
            start: 0,
            end: 1,
            replacement_offset: u64::MAX,
            replacement_length: 2,
        };
        assert_eq!(
            notepads_markdown_core_job_apply(
                f.session,
                r.job,
                &batch,
                &row,
                b"X".as_ptr(),
                1,
                &mut state,
                &mut error
            ),
            16
        );
        assert_eq!(
            (state.state_valid, state.source_accepted, state.has_ready),
            (1, 0, 0)
        );
        assert_eq!((*f.session).value.session.snapshot().unwrap().text(), "A");
    }
}

#[test]
fn used_install_cannot_repeat_and_fixed_typed_errors_are_not_internal_invariant() {
    let f = Fixture::new();
    let r = f.start();
    let mut error = WireError::default();
    let mut model = ptr::null_mut();
    unsafe {
        replace(&f, &r, b"x", 1);
        assert_eq!(notepads_markdown_core_job_run(r.job, &mut error), 0);
        assert_eq!(
            notepads_markdown_core_job_install(f.session, r.job, &mut model, &mut error),
            0
        );
        let installed = model;
        assert_eq!(
            notepads_markdown_core_job_install(f.session, r.job, &mut model, &mut error),
            5
        );
        assert!(model.is_null());
        assert_eq!(notepads_markdown_core_model_destroy(installed), 0);
    }
    let deadline = wire_error(CoreError::DeadlineExceeded {
        elapsed_ns: 500_000_007,
        limit_ns: 500_000_000,
    });
    assert_eq!(
        (deadline.status, deadline.requested, deadline.limit),
        (24, 500_000_007, 500_000_000)
    );
    assert_eq!(wire_error(CoreError::ClockFailure).status, 25);
}
