// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE file in the project root for license information.
// ---------------------------------------------------------------------------------------------

use super::*;

#[test]
fn retained_budget_query_checks_prefix_and_survives_session_retirement_without_work() {
    assert_eq!(size_of::<WireBudgetUsage>(), 24);
    assert_eq!(align_of::<WireBudgetUsage>(), 8);
    assert_eq!(std::mem::offset_of!(WireBudgetUsage, source_capacity), 8);
    // SAFETY: fixture owns live model/cancel/session; PODs are independent full storage.
    unsafe {
        let (cancel, session, model) = setup("**retained 中😀**");
        let root = (*cancel).value.budget.clone();
        let mut output = WireBudgetUsage {
            header: WireHeader::of::<WireBudgetUsage>(),
            source_capacity: u64::MAX,
            auxiliary_capacity: u64::MAX,
        };
        let mut error = WireError::default();
        output.header.struct_size -= 8;
        assert_eq!(
            notepads_markdown_core_model_budget_usage(model, &mut output, &mut error),
            STATUS_ABI_MISMATCH
        );
        assert_eq!(output.source_capacity, u64::MAX);
        output.header = WireHeader::of::<WireBudgetUsage>();
        error.header.struct_size = 0;
        assert_eq!(
            notepads_markdown_core_model_budget_usage(model, &mut output, &mut error),
            STATUS_ABI_MISMATCH
        );
        assert_eq!(output.auxiliary_capacity, u64::MAX);
        error = WireError::default();
        assert_eq!(
            notepads_markdown_core_model_budget_usage(ptr::null(), &mut output, &mut error),
            STATUS_INVALID_ARGUMENT
        );
        assert_eq!(notepads_markdown_core_cancel_request(cancel), 0);
        assert_eq!(notepads_markdown_core_session_destroy(session), 0);
        let before = root.usage();
        assert_eq!(
            notepads_markdown_core_model_budget_usage(model, &mut output, &mut error),
            0
        );
        let after = root.usage();
        assert_eq!(output.source_capacity, before.source_capacity);
        assert_eq!(output.auxiliary_capacity, before.auxiliary_capacity);
        assert_eq!(after.source_capacity, before.source_capacity);
        assert_eq!(after.auxiliary_capacity, before.auxiliary_capacity);
        assert!(output.source_capacity > 0 && output.auxiliary_capacity > 0);
        assert_eq!(notepads_markdown_core_model_destroy(model), 0);
        assert_eq!(notepads_markdown_core_cancel_destroy(cancel), 0);
        assert_eq!(root.usage().source_capacity, 0);
        assert_eq!(root.usage().auxiliary_capacity, 0);
    }
}

#[test]
fn extension_identity_round_trips_flags_and_rejects_invalid_masks() {
    for base in [1, 2] {
        for mask in (0..=255)
            .chain([0x100, 0x300, 0x500, 0x1ff, 0x3ff, 0x5ff])
            .flat_map(|mask| (0..128).map(move |more| mask | (more << 11)))
        {
            let config = profile_with_extensions(base, mask).unwrap();
            let model =
                ModelIdentity::new(SourceKey::new([134; 16], 7).unwrap(), 9, config).unwrap();
            assert_eq!(identity(wire_identity(model)).unwrap(), model);
        }
        let model = ModelIdentity::new(
            SourceKey::new([134; 16], 7).unwrap(),
            9,
            profile(base).unwrap(),
        )
        .unwrap();
        for extensions in [
            0x200,
            0x400,
            0x600,
            0x700,
            0x40000,
            0x80000,
            0x8000_0001,
            u32::MAX,
        ] {
            let mut wire = wire_identity(model);
            wire.extensions = extensions;
            assert_eq!(identity(wire), Err(CoreError::UnsupportedConfiguration));
        }
    }
    for profile in [0, 3, 4] {
        assert!(profile_with_extensions(profile, 0).is_err());
    }
    assert!(profile_with_extensions(2, 0x8000_0000).is_err());
    for mask in [0x200, 0x400, 0x600, 0x700, 0x40000, 0x80000] {
        assert!(profile_with_extensions(2, mask).is_err());
    }
}

// Test convenience only: use the actual start/prepare ABI, and retire temporary
// cancel/job outputs on error. Direct ABI3 state/output tests live in requests_tests.
unsafe fn replace_for_test(
    session: *mut Owned<CoreSession>,
    input: *const u8,
    length: u64,
    descriptor: *const WireSnapshot,
    output: *mut *mut Owned<CoreJob>,
    error: *mut WireError,
) -> i32 {
    unsafe {
        let mut cancel = ptr::null_mut();
        let status =
            notepads_markdown_core_session_start_request(session, 0, output, &mut cancel, error);
        if status != 0 {
            return status;
        }
        let mut result = WireSubmitResult {
            header: WireHeader::of::<WireSubmitResult>(),
            ..Default::default()
        };
        let status = notepads_markdown_core_job_replace_utf8(
            session,
            *output,
            input,
            length,
            descriptor,
            0,
            &mut result,
            error,
        );
        notepads_markdown_core_cancel_destroy(cancel);
        if status != 0 {
            notepads_markdown_core_job_destroy(*output);
            *output = ptr::null_mut();
        }
        status
    }
}
unsafe fn capture_for_test(
    session: *mut Owned<CoreSession>,
    output: *mut *mut Owned<CoreJob>,
    error: *mut WireError,
) -> i32 {
    unsafe {
        let mut cancel = ptr::null_mut();
        let status =
            notepads_markdown_core_session_start_request(session, 0, output, &mut cancel, error);
        if status != 0 {
            return status;
        }
        let mut result = WireSubmitResult {
            header: WireHeader::of::<WireSubmitResult>(),
            ..Default::default()
        };
        let status =
            notepads_markdown_core_job_capture_current(session, *output, &mut result, error);
        notepads_markdown_core_cancel_destroy(cancel);
        if status != 0 {
            notepads_markdown_core_job_destroy(*output);
            *output = ptr::null_mut();
        }
        status
    }
}

fn snapshot(revision: u64, profile: u32) -> WireSnapshot {
    WireSnapshot {
        header: WireHeader::of::<WireSnapshot>(),
        source: WireSource {
            document: [9; 16],
            revision,
        },
        profile,
        reserved: 0,
    }
}
fn result() -> WireReadResult {
    WireReadResult {
        header: WireHeader::of::<WireReadResult>(),
        ..Default::default()
    }
}

#[test]
fn stable_bootstrap_creates_fresh_same_root_flags_without_session_borrows() {
    let mut error = WireError::default();
    let mut bootstrap = ptr::null_mut();
    let mut first = ptr::null_mut();
    let mut second = ptr::null_mut();
    let mut session = ptr::null_mut();
    let mut job = ptr::null_mut();
    // SAFETY: bootstrap is immutable for all calls; independent flags and session are serially owned.
    unsafe {
        assert_eq!(
            notepads_markdown_core_cancel_create(ptr::null(), &mut bootstrap, &mut error),
            0
        );
        let root = (*bootstrap).value.budget.clone();
        assert_eq!(
            notepads_markdown_core_cancel_create_like(bootstrap, &mut first, &mut error),
            0
        );
        assert_eq!(
            notepads_markdown_core_session_create(first, 1, 0, &mut session, &mut error),
            0
        );
        assert_eq!(notepads_markdown_core_cancel_request(first), 0);
        assert_eq!(notepads_markdown_core_cancel_request(bootstrap), 0);
        assert_eq!(
            notepads_markdown_core_cancel_create_like(bootstrap, &mut second, &mut error),
            0
        );
        assert_eq!(
            replace_for_test(
                session,
                b"ok".as_ptr(),
                2,
                &snapshot(1, 1),
                &mut job,
                &mut error
            ),
            0
        );
        assert_eq!(notepads_markdown_core_job_run(job, &mut error), 0);
        assert!(root.same_root(&(*second).value.budget));
        notepads_markdown_core_job_destroy(job);
        notepads_markdown_core_session_destroy(session);
        notepads_markdown_core_cancel_destroy(first);
        notepads_markdown_core_cancel_destroy(second);
        notepads_markdown_core_cancel_destroy(bootstrap);
        assert_eq!(root.usage().source_capacity, 0);
        assert_eq!(root.usage().auxiliary_capacity, 0);
    }
}

#[test]
fn actual_aba_stale_and_wrong_root_do_not_install_or_reinterpret_ids() {
    // SAFETY: valid independent handles and PODs, exclusive session and job access, destruction after calls.
    unsafe {
        let (cancel, session, model) = setup("A");
        let root = (*cancel).value.budget.clone();
        let mut error = WireError::default();
        let mut job = ptr::null_mut();
        let mut next = ptr::null_mut();
        let mut output = ptr::null_mut();
        assert_eq!(capture_for_test(session, &mut job, &mut error), 0);
        assert_eq!(notepads_markdown_core_job_run(job, &mut error), 0);
        assert_eq!(
            replace_for_test(
                session,
                b"B".as_ptr(),
                1,
                &snapshot(2, 1),
                &mut next,
                &mut error
            ),
            0
        );
        notepads_markdown_core_job_destroy(next);
        assert_eq!(
            replace_for_test(
                session,
                b"A".as_ptr(),
                1,
                &snapshot(1, 1),
                &mut next,
                &mut error
            ),
            0
        );
        notepads_markdown_core_job_destroy(next);
        assert_eq!(
            notepads_markdown_core_job_install(session, job, &mut output, &mut error),
            10
        );
        assert!(output.is_null());
        notepads_markdown_core_job_destroy(job);
        let mut foreign = ptr::null_mut();
        assert_eq!(
            notepads_markdown_core_cancel_create(ptr::null(), &mut foreign, &mut error),
            0
        );
        let mut stream = ptr::null_mut();
        assert_eq!(
            notepads_markdown_core_stream_create(model, foreign, &mut stream, &mut error),
            14
        );
        assert!(stream.is_null());
        notepads_markdown_core_cancel_destroy(foreign);
        let descriptor = WireQuery {
            header: WireHeader::of::<WireQuery>(),
            identity: WireIdentity {
                generation: 999,
                ..wire_identity((*model).value.model.identity())
            },
            domain: 2,
            target_kind: 1,
            target_id: (*model).value.model.main_flow_id().value(),
            affinity: 0,
            start: 0,
            end: 0,
        };
        let mut query = ptr::null_mut();
        assert_eq!(
            notepads_markdown_core_map_points(
                model,
                cancel,
                &descriptor,
                1,
                &mut query,
                &mut error
            ),
            23
        );
        assert_eq!(error.expected, 1);
        assert_eq!(error.actual, 999);
        notepads_markdown_core_model_destroy(model);
        notepads_markdown_core_session_destroy(session);
        notepads_markdown_core_cancel_destroy(cancel);
        assert_eq!(root.usage().source_capacity, 0);
        assert_eq!(root.usage().auxiliary_capacity, 0);
    }
}
unsafe fn setup(
    text: &str,
) -> (
    *mut Owned<CoreCancel>,
    *mut Owned<CoreSession>,
    *mut Owned<CoreModel>,
) {
    // SAFETY: delegated fixture establishes and retires the same actual ABI handles.
    unsafe { setup_with_budget(text, SourceBudget::task_fixture()) }
}
unsafe fn setup_with_budget(
    text: &str,
    budget: SourceBudget,
) -> (
    *mut Owned<CoreCancel>,
    *mut Owned<CoreSession>,
    *mut Owned<CoreModel>,
) {
    let cancel;
    let mut session = ptr::null_mut();
    let mut job = ptr::null_mut();
    let mut model = ptr::null_mut();
    let mut error = WireError::default();
    // SAFETY: initialized stack descriptors/output/error, valid borrowed UTF8; serial live handles.
    unsafe {
        cancel = new_control_cancel(budget).unwrap();
        assert_eq!(
            notepads_markdown_core_session_create(cancel, 1, 0, &mut session, &mut error),
            0
        );
        assert_eq!(
            replace_for_test(
                session,
                text.as_ptr(),
                text.len() as u64,
                &snapshot(1, 1),
                &mut job,
                &mut error
            ),
            0
        );
        assert_eq!(
            notepads_markdown_core_job_run(job, &mut error),
            0,
            "{error:?}"
        );
        assert_eq!(
            notepads_markdown_core_job_install(session, job, &mut model, &mut error),
            0,
            "{error:?}"
        );
        assert_eq!(notepads_markdown_core_job_destroy(job), 0);
    }
    (cancel, session, model)
}

#[test]
fn actual_model_pin_outlives_session_and_cancelled_old_token() {
    // SAFETY: all calls follow exact live-handle ownership, output prefix and exclusive-call contracts.
    unsafe {
        let (cancel, session, model) = setup("**中&amp;😀**");
        let root = (*cancel).value.budget.clone();
        let mut error = WireError::default();
        let mut summary = WireModelSummary {
            header: WireHeader::of::<WireModelSummary>(),
            ..Default::default()
        };
        assert_eq!(
            notepads_markdown_core_model_summary(model, &mut summary, &mut error),
            0
        );
        assert_eq!(summary.main_utf16, 4);
        assert_eq!(summary.identity.profile, 1);
        assert_eq!(notepads_markdown_core_cancel_request(cancel), 0);
        assert_eq!(notepads_markdown_core_session_destroy(session), 0);
        let mut fresh = ptr::null_mut();
        let mut stream = ptr::null_mut();
        assert_eq!(
            notepads_markdown_core_model_cancel_create(model, &mut fresh, &mut error),
            0
        );
        assert_eq!(
            notepads_markdown_core_stream_create(model, fresh, &mut stream, &mut error),
            0
        );
        assert_eq!(notepads_markdown_core_model_destroy(model), 0);
        let mut bytes = [0u8; 4];
        let mut output = result();
        let mut text = String::new();
        loop {
            assert_eq!(
                notepads_markdown_core_stream_read(
                    stream,
                    bytes.as_mut_ptr(),
                    4,
                    &mut output,
                    &mut error
                ),
                0
            );
            text.push_str(std::str::from_utf8(&bytes[..output.written as usize]).unwrap());
            if output.done != 0 {
                break;
            }
        }
        assert_eq!(text, "中&😀");
        assert_eq!(output.next, 4);
        assert_eq!(notepads_markdown_core_stream_destroy(stream), 0);
        assert_eq!(notepads_markdown_core_cancel_destroy(fresh), 0);
        assert_eq!(notepads_markdown_core_cancel_destroy(cancel), 0);
        assert_eq!(root.usage().source_capacity, 0);
        assert_eq!(root.usage().auxiliary_capacity, 0);
        assert_eq!(root.usage().source_owners, 0);
    }
}

#[test]
fn original_model_point_and_range_batch_are_exported_with_full_relations() {
    // SAFETY: exact initialized POD arrays, matching live handles, serialized calls.
    unsafe {
        let (cancel, session, model) = setup("**中&amp;😀**");
        let root = (*cancel).value.budget.clone();
        let mut error = WireError::default();
        let model_value = &(*model).value.model;
        let identity = wire_identity(model_value.identity());
        let mut descriptor = WireQuery {
            header: WireHeader::of::<WireQuery>(),
            identity,
            domain: 1,
            target_kind: 1,
            target_id: 1,
            affinity: 0,
            start: 6,
            end: 6,
        };
        let mut query = ptr::null_mut();
        assert_eq!(
            notepads_markdown_core_map_points(
                model,
                cancel,
                &descriptor,
                1,
                &mut query,
                &mut error
            ),
            0,
            "{error:?}"
        );
        let mut summary = WireQuerySummary {
            header: WireHeader::of::<WireQuerySummary>(),
            ..Default::default()
        };
        assert_eq!(
            notepads_markdown_core_query_summary(query, &mut summary, &mut error),
            0
        );
        assert_eq!(summary.points, 1);
        let mut points = [WirePoint::default(); 1];
        let mut output = result();
        assert_eq!(
            notepads_markdown_core_query_read(
                query,
                1,
                0,
                1,
                size_of::<WirePoint>() as u32,
                points.as_mut_ptr().cast(),
                &mut output,
                &mut error
            ),
            0
        );
        let core_query = point_query(model_value, descriptor).unwrap();
        let mut op = root.begin(CancellationToken::new());
        let expected = model_value.map_point(&core_query, &mut op).unwrap();
        assert_eq!(points[0], wire_point(expected.results()[0]));
        assert_eq!(points[0].chosen.value, 1);
        assert_eq!(points[0].correspondence, 1);
        let mut contributors = [WireContributor::default(); 256];
        assert_eq!(
            notepads_markdown_core_query_read(
                query,
                6,
                0,
                summary.contributors,
                size_of::<WireContributor>() as u32,
                contributors.as_mut_ptr().cast(),
                &mut output,
                &mut error
            ),
            0
        );
        for (i, value) in expected.contributors().iter().enumerate() {
            assert_eq!(contributors[i], wire_contributor(*value));
        }
        drop(expected);
        assert_eq!(notepads_markdown_core_query_destroy(query), 0);
        descriptor.domain = 2;
        descriptor.start = 1;
        descriptor.end = 2;
        assert_eq!(
            notepads_markdown_core_map_ranges(
                model,
                cancel,
                &descriptor,
                1,
                &mut query,
                &mut error
            ),
            0
        );
        let mut range = [WireRange::default(); 1];
        assert_eq!(
            notepads_markdown_core_query_read(
                query,
                2,
                0,
                1,
                size_of::<WireRange>() as u32,
                range.as_mut_ptr().cast(),
                &mut output,
                &mut error
            ),
            0
        );
        assert_eq!(range[0].kind, 2);
        let mut selections = [WireSelection::default(); 1];
        assert_eq!(
            notepads_markdown_core_query_read(
                query,
                3,
                0,
                1,
                size_of::<WireSelection>() as u32,
                selections.as_mut_ptr().cast(),
                &mut output,
                &mut error
            ),
            0
        );
        let expected = model_value
            .map_range(&range_query(model_value, descriptor).unwrap(), &mut op)
            .unwrap();
        assert_eq!(selections[0], wire_selection(expected.selections()[0]));
        drop(expected);
        assert_eq!(notepads_markdown_core_model_destroy(model), 0);
        assert_eq!(notepads_markdown_core_session_destroy(session), 0);
        // Response itself still owns the model after both preceding owners are gone.
        assert_eq!(
            notepads_markdown_core_query_summary(query, &mut summary, &mut error),
            0
        );
        assert_eq!(notepads_markdown_core_query_destroy(query), 0);
        assert_eq!(notepads_markdown_core_cancel_destroy(cancel), 0);
        assert_eq!(root.usage().source_capacity, 0);
        assert_eq!(root.usage().auxiliary_capacity, 0);
    }
}

#[test]
fn rejected_headers_do_not_consume_job_or_write_success_output() {
    #[repr(C, align(8))]
    struct Prefix {
        version: u32,
        size: u32,
    }
    let mut error = WireError::default();
    let mut cancel = ptr::null_mut();
    let mut session = ptr::null_mut();
    let mut job = ptr::null_mut();
    let mut model = ptr::null_mut();
    // SAFETY: rejected descriptors expose only aligned readable prefixes; full matching PODs otherwise.
    unsafe {
        assert_eq!(
            notepads_markdown_core_cancel_create(ptr::null(), &mut cancel, &mut error),
            0
        );
        assert_eq!(
            notepads_markdown_core_session_create(cancel, 1, 0, &mut session, &mut error),
            0
        );
        let prefix = Prefix {
            version: CORE_ABI_VERSION + 1,
            size: 8,
        };
        assert_eq!(
            replace_for_test(
                session,
                ptr::null(),
                0,
                (&prefix as *const Prefix).cast(),
                &mut job,
                &mut error
            ),
            STATUS_ABI_MISMATCH
        );
        assert!(job.is_null());
        assert_eq!(
            replace_for_test(
                session,
                b"x".as_ptr(),
                1,
                &snapshot(1, 1),
                &mut job,
                &mut error
            ),
            0
        );
        let mut wrong = Prefix {
            version: CORE_ABI_VERSION,
            size: 8,
        };
        assert_eq!(
            notepads_markdown_core_job_run(job, (&mut wrong as *mut Prefix).cast()),
            STATUS_ABI_MISMATCH
        );
        assert_eq!(notepads_markdown_core_job_run(job, &mut error), 0);
        assert_eq!(
            notepads_markdown_core_job_install(session, job, &mut model, &mut error),
            0
        );
        let mut output = WireModelSummary {
            header: WireHeader::of::<WireModelSummary>(),
            ..Default::default()
        };
        output.header.struct_size = 8;
        let before = output;
        assert_eq!(
            notepads_markdown_core_model_summary(model, &mut output, &mut error),
            STATUS_ABI_MISMATCH
        );
        assert_eq!(output, before);
        notepads_markdown_core_model_destroy(model);
        notepads_markdown_core_job_destroy(job);
        notepads_markdown_core_session_destroy(session);
        notepads_markdown_core_cancel_destroy(cancel);
    }
}

#[test]
fn invalid_profile_cancel_stale_cross_session_and_single_use_are_typed() {
    // SAFETY: all pointers are live and correctly aligned, calls serial, handles destroyed once.
    unsafe {
        let (cancel, session, model) = setup("old");
        let root = (*cancel).value.budget.clone();
        let mut error = WireError::default();
        let mut job = ptr::null_mut();
        let mut output = ptr::null_mut();
        assert_eq!(
            replace_for_test(
                session,
                b"new".as_ptr(),
                3,
                &snapshot(2, 0),
                &mut job,
                &mut error
            ),
            STATUS_INVALID_ARGUMENT
        );
        assert!(job.is_null());
        assert_eq!(
            replace_for_test(
                session,
                b"new".as_ptr(),
                3,
                &snapshot(3, 1),
                &mut job,
                &mut error
            ),
            0
        );
        assert_eq!(notepads_markdown_core_job_run(job, &mut error), 0);
        let mut other = ptr::null_mut();
        assert_eq!(
            notepads_markdown_core_session_create(cancel, 1, 0, &mut other, &mut error),
            0
        );
        assert_eq!(
            notepads_markdown_core_job_install(other, job, &mut output, &mut error),
            10
        );
        assert!(output.is_null());
        assert_eq!(
            notepads_markdown_core_job_install(session, job, &mut output, &mut error),
            10
        );
        notepads_markdown_core_job_destroy(job);
        notepads_markdown_core_session_destroy(other);
        let mut fresh_cancel = ptr::null_mut();
        assert_eq!(
            notepads_markdown_core_session_start_request(
                session,
                1,
                &mut job,
                &mut fresh_cancel,
                &mut error
            ),
            4
        );
        assert!(job.is_null() && fresh_cancel.is_null());
        notepads_markdown_core_session_destroy(session);
        notepads_markdown_core_model_destroy(model);
        notepads_markdown_core_cancel_destroy(cancel);
        assert_eq!(root.usage().source_capacity, 0);
        assert_eq!(root.usage().auxiliary_capacity, 0);
    }
}

#[test]
fn install_output_allocation_failure_keeps_existing_installed_model() {
    // SAFETY: test obtains exclusive handles and inspects their private safe data, no aliasing calls.
    unsafe {
        let (cancel, session, model) = setup("old");
        let root = (*cancel).value.budget.clone();
        let mut error = WireError::default();
        let mut job = ptr::null_mut();
        let mut output = ptr::null_mut();
        assert_eq!(capture_for_test(session, &mut job, &mut error), 0);
        assert_eq!(notepads_markdown_core_job_run(job, &mut error), 0);
        let before = (*session).value.session.installed().unwrap().identity();
        let mut blocker = root.begin(CancellationToken::new());
        let hold = blocker
            .reserve_boundary(root.limits().auxiliary_capacity - root.usage().auxiliary_capacity)
            .unwrap();
        assert_eq!(
            notepads_markdown_core_job_install(session, job, &mut output, &mut error),
            9
        );
        assert_eq!(error.limit_kind, 8);
        assert!(output.is_null());
        assert_eq!(
            (*session).value.session.installed().unwrap().identity(),
            before
        );
        assert!(matches!((*job).value.state, JobState::Terminal(_)));
        drop(hold);
        // The first fault is retained; freeing capacity cannot turn this failed job into a success.
        assert_eq!(
            notepads_markdown_core_job_install(session, job, &mut output, &mut error),
            9
        );
        notepads_markdown_core_job_destroy(job);
        notepads_markdown_core_model_destroy(model);
        notepads_markdown_core_session_destroy(session);
        notepads_markdown_core_cancel_destroy(cancel);
        assert_eq!(root.usage().source_capacity, 0);
        assert_eq!(root.usage().auxiliary_capacity, 0);
    }
}

#[test]
fn rejected_replacement_preserves_config_installed_model_and_completed_ticket() {
    // SAFETY: serial exclusive session/job access and live, independent input/output storage.
    unsafe {
        let (cancel, session, model) = setup("old");
        let root = (*cancel).value.budget.clone();
        let mut error = WireError::default();
        let mut ticket = ptr::null_mut();
        let mut rejected = ptr::null_mut();
        let mut installed = ptr::null_mut();
        assert_eq!(capture_for_test(session, &mut ticket, &mut error), 0);
        assert_eq!(notepads_markdown_core_job_run(ticket, &mut error), 0);
        let before = (*session).value.session.installed().unwrap().identity();
        assert_eq!(
            replace_for_test(
                session,
                b"new".as_ptr(),
                3,
                &snapshot(1, 2),
                &mut rejected,
                &mut error
            ),
            12
        );
        assert!(rejected.is_null());
        assert_eq!(
            (*session).value.session.installed().unwrap().identity(),
            before
        );
        assert_eq!(
            notepads_markdown_core_job_install(session, ticket, &mut installed, &mut error),
            0
        );
        assert_eq!(
            (*installed).value.model.identity().config().profile(),
            Profile::C0
        );
        notepads_markdown_core_model_destroy(installed);
        notepads_markdown_core_job_destroy(ticket);
        notepads_markdown_core_model_destroy(model);
        notepads_markdown_core_session_destroy(session);
        notepads_markdown_core_cancel_destroy(cancel);
        assert_eq!(root.usage().source_capacity, 0);
        assert_eq!(root.usage().auxiliary_capacity, 0);
    }
}

#[test]
fn empty_query_export_charges_result_write_and_preserves_output_on_work_failure() {
    // SAFETY: empty input batch, aligned initialized POD and exclusive query operations.
    unsafe {
        let (cancel, session, model) = setup_with_budget("x", SourceBudget::model_fixture());
        let root = (*cancel).value.budget.clone();
        let mut error = WireError::default();
        let mut query = ptr::null_mut();
        assert_eq!(
            notepads_markdown_core_map_points(
                model,
                cancel,
                ptr::null(),
                0,
                &mut query,
                &mut error
            ),
            0
        );
        let mut point = WirePoint::default();
        let mut output = result();
        let before = (*query)
            .value
            .op
            .stats()
            .expect("statistics enabled")
            .copied_bytes();
        assert_eq!(
            notepads_markdown_core_query_read(
                query,
                1,
                0,
                0,
                size_of::<WirePoint>() as u32,
                (&mut point as *mut WirePoint).cast(),
                &mut output,
                &mut error
            ),
            0
        );
        assert_eq!(
            (*query)
                .value
                .op
                .stats()
                .expect("statistics enabled")
                .copied_bytes()
                - before,
            size_of::<WireReadResult>() as u64
        );
        assert_eq!(output.written, 0);
        assert_eq!(output.done, 1);
        let op = &mut (*query).value.op;
        let stop = root.limits().max_work - size_of::<WireReadResult>() as u64 + 1;
        while op.stats().expect("statistics enabled").total() < stop {
            op.external_read((stop - op.stats().expect("statistics enabled").total()).min(65_536))
                .unwrap();
        }
        let before = output;
        assert_eq!(
            notepads_markdown_core_query_read(
                query,
                1,
                0,
                0,
                size_of::<WirePoint>() as u32,
                (&mut point as *mut WirePoint).cast(),
                &mut output,
                &mut error
            ),
            9
        );
        assert_eq!(output, before);
        assert_eq!(error.limit_kind, 9);
        notepads_markdown_core_query_destroy(query);
        notepads_markdown_core_model_destroy(model);
        notepads_markdown_core_session_destroy(session);
        notepads_markdown_core_cancel_destroy(cancel);
        assert_eq!(root.usage().source_capacity, 0);
        assert_eq!(root.usage().auxiliary_capacity, 0);
    }
}

#[test]
fn query_rejects_unknown_ids_halfsurrogates_and_retains_first_fault() {
    // SAFETY: valid complete descriptor, aligned output tables, exclusive query operations.
    unsafe {
        let (cancel, session, model) = setup("😀");
        let mut error = WireError::default();
        let mut query = ptr::null_mut();
        let mut descriptor = WireQuery {
            header: WireHeader::of::<WireQuery>(),
            identity: wire_identity((*model).value.model.identity()),
            domain: 2,
            target_kind: 1,
            target_id: 1,
            affinity: 0,
            start: 1,
            end: 1,
        };
        assert_eq!(
            notepads_markdown_core_map_points(
                model,
                cancel,
                &descriptor,
                1,
                &mut query,
                &mut error
            ),
            15
        );
        assert!(query.is_null());
        descriptor.start = 0;
        descriptor.end = 0;
        descriptor.target_id = 999;
        assert_eq!(
            notepads_markdown_core_map_points(
                model,
                cancel,
                &descriptor,
                1,
                &mut query,
                &mut error
            ),
            17
        );
        descriptor.target_id = 1;
        assert_eq!(
            notepads_markdown_core_map_points(
                model,
                cancel,
                &descriptor,
                1,
                &mut query,
                &mut error
            ),
            0
        );
        let mut point = WirePoint::default();
        let mut output = result();
        let before = output;
        assert_eq!(
            notepads_markdown_core_query_read(
                query,
                1,
                2,
                1,
                size_of::<WirePoint>() as u32,
                (&mut point as *mut WirePoint).cast(),
                &mut output,
                &mut error
            ),
            16
        );
        assert_eq!(output, before);
        assert_eq!(
            notepads_markdown_core_query_read(
                query,
                1,
                0,
                1,
                size_of::<WirePoint>() as u32,
                (&mut point as *mut WirePoint).cast(),
                &mut output,
                &mut error
            ),
            16
        );
        notepads_markdown_core_query_destroy(query);
        notepads_markdown_core_model_destroy(model);
        notepads_markdown_core_session_destroy(session);
        notepads_markdown_core_cancel_destroy(cancel);
    }
}

#[test]
fn cancellation_during_real_parser_run_uses_shared_token_and_drops_owners() {
    use std::time::{Duration, Instant};
    let source = vec![b'a'; 16_777_216];
    let mut error = WireError::default();
    let mut cancel = ptr::null_mut();
    let mut session = ptr::null_mut();
    let mut job = ptr::null_mut();
    let mut request_cancel = ptr::null_mut();
    // SAFETY: setup is serial, source/descriptor/output all valid through capture.
    unsafe {
        assert_eq!(
            notepads_markdown_core_cancel_create(ptr::null(), &mut cancel, &mut error),
            0
        );
        assert_eq!(
            notepads_markdown_core_session_create(cancel, 1, 0, &mut session, &mut error),
            0
        );
        assert_eq!(
            notepads_markdown_core_session_start_request(
                session,
                0,
                &mut job,
                &mut request_cancel,
                &mut error
            ),
            0
        );
        let mut result = WireSubmitResult {
            header: WireHeader::of::<WireSubmitResult>(),
            ..Default::default()
        };
        assert_eq!(
            notepads_markdown_core_job_replace_utf8(
                session,
                job,
                source.as_ptr(),
                source.len() as u64,
                &snapshot(1, 1),
                0,
                &mut result,
                &mut error
            ),
            0
        );
    }
    let job_address = job as usize;
    let root = unsafe { (*cancel).value.budget.clone() };
    std::thread::scope(|scope| {
        let running = scope.spawn(move || {
            let mut error = WireError::default();
            // SAFETY: job is exclusively moved to worker; no destroy until join.
            unsafe {
                notepads_markdown_core_job_run(job_address as *mut Owned<CoreJob>, &mut error)
            }
        });
        let deadline = Instant::now() + Duration::from_secs(10);
        while root.usage().auxiliary_capacity < 100_000 && Instant::now() < deadline {
            std::thread::yield_now();
        }
        assert!(root.usage().auxiliary_capacity >= 100_000);
        // SAFETY: cancel handle lives until after join and shares only an atomic flag.
        unsafe {
            assert_eq!(notepads_markdown_core_cancel_request(request_cancel), 0);
        }
        assert_eq!(running.join().unwrap(), 4);
    });
    // SAFETY: worker finished; all handles now exclusively destroyed.
    unsafe {
        notepads_markdown_core_job_destroy(job);
        notepads_markdown_core_session_destroy(session);
        notepads_markdown_core_cancel_destroy(request_cancel);
        notepads_markdown_core_cancel_destroy(cancel);
    }
    assert_eq!(root.usage().source_capacity, 0);
    assert_eq!(root.usage().auxiliary_capacity, 0);
    assert_eq!(root.usage().source_owners, 0);
}
