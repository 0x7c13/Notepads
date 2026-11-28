// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE file in the project root for license information.
// ---------------------------------------------------------------------------------------------

//! Allocation identity and physical-free payment observations only. This test
//! does not render text, prove Native retirement/join, or observe payment while
//! allocating: reading a ledger from alloc would risk its existing lock.
use super::super::tests::{
    arm_retained_probe, arm_system_allocation_trace, take_retained_probe,
    take_system_allocation_trace,
};
use super::*;
use markdown_core::source::SourceDocument;
use markdown_svg::abi;

fn core_error() -> WireError {
    WireError {
        header: WireHeader::of::<WireError>(),
        ..Default::default()
    }
}

fn pod<T: Default>() -> T {
    let mut value = T::default();
    // SAFETY: this helper is used only for the renderer's header-first PODs.
    unsafe {
        (&raw mut value)
            .cast::<SvgJobHeader>()
            .write(SvgJobHeader::of::<T>());
    }
    value
}

struct ActualRoots {
    cancel: *mut Owned<CoreCancel>,
    model: *mut Owned<CoreModel>,
    policy: *mut Owned<ImagePolicy>,
    request: *mut Owned<ImageRequest>,
    account: *mut Owned<SvgWork>,
    budget: SourceBudget,
}

impl ActualRoots {
    fn new() -> Self {
        let mut error = core_error();
        let mut cancel = ptr::null_mut();
        // SAFETY: all handles and PODs below are unique, real Core objects;
        // their lifetime exceeds the explicit allocation-free probe windows.
        unsafe {
            assert_eq!(notepads_markdown_preview_cancel_create(&mut cancel, &mut error), 0);
            let budget = (*cancel).value.budget.clone();
            let mut operation = budget.begin((*cancel).value.token.clone());
            let key = SourceKey::new([0x79; 16], 1).unwrap();
            let document =
                SourceDocument::new(key, b"![observer](actual.svg)", &mut operation).unwrap();
            let accepted = AcceptedModel::parse(
                document.snapshot(),
                ParseConfig::new(Profile::G3),
                1,
                &mut operation,
            )
            .unwrap();
            let model = Slot::new(operation.boundary_work())
                .unwrap()
                .commit(CoreModel { model: accepted });
            let mut policy = ptr::null_mut();
            assert_eq!(notepads_markdown_image_policy_create(&mut policy, &mut error), 0);
            let epoch = ImageEpochDescriptor {
                header: ImageHeader::of::<ImageEpochDescriptor>(),
                epoch: ImageEpoch {
                    source: WireSource {
                        document: [0x79; 16],
                        revision: 1,
                    },
                    source_generation: 1,
                    provider_generation: 1,
                },
            };
            assert_eq!(notepads_markdown_image_policy_begin(policy, &epoch, &mut error), 0);
            let mut request = ptr::null_mut();
            desire_image_for_test(policy, 0, &mut error);
            let mut info = ImageRequestInfo {
                header: ImageHeader::of::<ImageRequestInfo>(),
                ..Default::default()
            };
            assert_eq!(
                notepads_markdown_image_policy_start(policy, 0, 1, &mut request, &mut info, &mut error),
                0
            );
            let mut account = ptr::null_mut();
            assert_eq!(
                notepads_markdown_svg_work_create(1, request, model, cancel, &mut account, &mut error),
                0
            );
            Self {
                cancel,
                model,
                policy,
                request,
                account,
                budget,
            }
        }
    }

    fn control(&self) -> u64 {
        // SAFETY: the original policy is pinned until this fixture drops.
        unsafe { (&(*self.policy).value.owner).policy.svg_work_bytes() }
    }
}

impl Drop for ActualRoots {
    fn drop(&mut self) {
        // SAFETY: successful test teardown reclaims the exact original account
        // after the job and resources have retired all independent owners.
        unsafe {
            if !self.account.is_null() {
                assert_eq!(notepads_markdown_svg_work_close(self.account, &mut core_error()), 0);
                assert_eq!(notepads_markdown_svg_work_destroy(self.account, &mut core_error()), 0);
            }
            assert_eq!(notepads_markdown_image_request_destroy(self.request), 0);
            assert_eq!(notepads_markdown_image_policy_destroy(self.policy), 0);
            assert_eq!(notepads_markdown_core_model_destroy(self.model), 0);
            assert_eq!(notepads_markdown_core_cancel_destroy(self.cancel), 0);
        }
    }
}

// Creation/finish need the validated table, but this observer deliberately
// does no prepare/text work. Any unexpected font call is rejected explicitly.
unsafe extern "C" fn no_plan(_: u64, _: *const abi::Draft, _: *mut abi::Plan) -> i32 {
    -101
}
unsafe extern "C" fn no_font(_: u64, _: u32, _: *mut u8, _: u64, _: *mut u64) -> i32 {
    -102
}
unsafe extern "C" fn no_metrics(
    _: u64,
    _: u32,
    _: u32,
    _: *mut abi::GlyphMetrics,
    _: u32,
    _: *mut u32,
) -> i32 {
    -103
}
unsafe extern "C" fn no_geometry(
    _: u64,
    _: *const abi::GlyphKey,
    _: u32,
    _: *mut abi::GeometryReceipt,
) -> i32 {
    -104
}
unsafe extern "C" fn no_fill_geometry(
    _: u64,
    _: u64,
    _: *mut abi::GlyphPath,
    _: u32,
    _: *mut abi::Command,
    _: u32,
    _: *mut u32,
    _: *mut u32,
) -> i32 {
    -105
}
unsafe extern "C" fn no_retire(_: u64, _: u64) -> i32 {
    -106
}

struct TraceGuard;
impl Drop for TraceGuard {
    fn drop(&mut self) {
        let _ = take_system_allocation_trace();
        let _ = take_retained_probe();
    }
}

#[test]
fn actual_renderer_shell_and_private_cancel_free_before_both_bootstrap_fees_return() {
    let _process = crate::core::images::serialize_process_image_tests();
    // Initialize std's current-thread state before identifying the first engine
    // allocation after the actual renderer shell allocation.
    let _ = std::thread::current().id();
    let mut roots = ActualRoots::new();
    let nonce = unsafe { (*roots.account).value.nonce };
    let mut resources = ptr::null_mut();
    assert_eq!(
        unsafe {
            notepads_markdown_svg_resources_create(1, &mut roots.account, &mut resources, &mut core_error())
        },
        0
    );
    assert!(roots.account.is_null());
    let callbacks = abi::Callbacks {
        size: size_of::<abi::Callbacks>() as u32,
        version: abi::FONT_CALLBACKS_VERSION,
        session: 1,
        plan: Some(no_plan),
        fill_font: Some(no_font),
        fill_metrics: Some(no_metrics),
        prepare_geometry: Some(no_geometry),
        fill_geometry: Some(no_fill_geometry),
        retire_geometry: Some(no_retire),
    };
    let identity = SvgJobIdentity {
        job_nonce: nonce,
        source_generation: 1,
        provider_generation: 1,
        font_generation: 1,
        target_generation: 1,
    };
    let mut job = ptr::null_mut();
    let mut cancel = ptr::null_mut();
    let _guard = TraceGuard;
    arm_system_allocation_trace();
    assert_eq!(
        unsafe {
            notepads_markdown_svg_job_create(
                1,
                resources,
                &callbacks,
                &super::shape_coverage(),
                &identity,
                &mut job,
                &mut cancel,
                &mut pod(),
            )
        },
        0
    );
    let (count, overflow, allocations) = take_system_allocation_trace();
    assert!(!overflow, "fixed System allocation trace overflowed");
    assert!(!job.is_null() && !cancel.is_null());
    let shell_index = allocations[..count]
        .iter()
        .position(|event| event.0 == job as usize)
        .unwrap();
    assert_eq!(allocations[shell_index].1, size_of::<SvgRendererJob>());
    assert_eq!(allocations[shell_index].2, align_of::<SvgRendererJob>());
    // With std thread initialized, OwnedJob::new_inner's first real allocation
    // immediately follows this shell: Arc<AtomicBool> for its private cancel.
    let private_cancel = allocations[shell_index + 1];
    let metadata = markdown_svg::bootstrap_metadata();
    assert_eq!(private_cancel.1, metadata.cancel_arc_bytes);
    assert_eq!(private_cancel.2, align_of::<usize>());
    assert_eq!(private_cancel.3, allocations[shell_index].3 + 1);
    assert_ne!(private_cancel.0, 0);
    assert_ne!(private_cancel.0, job as usize);

    // SAFETY: same creating worker; only immutable/private inspection occurs
    // between public ABI calls. The observer pins the same real Host explicitly.
    let (host, shell_fee, engine_fee) = unsafe {
        let data = &*(*job).data.get();
        (
            Arc::clone(&(*job).host),
            (*job).shell_fee,
            data.engine_fee.unwrap(),
        )
    };
    let shell_bytes = size_of::<SvgRendererJob>() as u64;
    let engine_bytes = (metadata.total_bytes().unwrap()
        - metadata.shell_bytes
        - metadata.first_failure_arc_bytes) as u64;
    let mut retired: SvgJobRetirement = pod();
    assert_eq!(
        unsafe { notepads_markdown_svg_job_finish(job, &mut retired, &mut pod()) },
        0
    );
    assert_eq!(retired.cleanup_succeeded, 1);
    assert_eq!(retired.remaining_owners, 0);
    assert_eq!(retired.failed_releases, 0);
    // Worker finish retires its independent ticket Vec; the two fixed
    // bootstrap fees below must remain admitted until physical shell teardown.
    host.with(|account, _| {
        let shell = account.token_row(shell_fee)?;
        let engine = account.token_row(engine_fee)?;
        assert!(shell.state == TicketState::Admitted);
        assert!(engine.state == TicketState::Admitted);
        assert_eq!(shell.bytes, shell_bytes);
        assert_eq!(engine.bytes, engine_bytes);
        assert_eq!(account.pending, 0);
        Ok(())
    })
    .unwrap();
    assert_eq!(unsafe { notepads_markdown_svg_job_close_cancel(job) }, 0);
    let before_root = roots.budget.usage().auxiliary_capacity;
    let before_control = roots.control();
    let before_retire = host.snapshot();
    // SAFETY: both real ledgers and both watched allocation identities remain
    // pinned; the exact shell/private Arc frees occur outside account locks.
    unsafe {
        arm_retained_probe(
            &roots.budget,
            &(&(*roots.policy).value.owner).policy,
            [job as usize, private_cancel.0, 0],
        );
    }
    let mut destroyed: SvgJobDestroyed = pod();
    assert_eq!(
        unsafe { notepads_markdown_svg_job_destroy(&mut job, &mut destroyed, &mut pod()) },
        0
    );
    let (free_count, frees) = take_retained_probe();
    assert_eq!(free_count, 2);
    assert_eq!(frees[0].0, allocations[shell_index].0);
    assert_eq!(frees[0].1 as u64, shell_bytes);
    assert_eq!(frees[1].0, private_cancel.0);
    assert_eq!(frees[1].1, metadata.cancel_arc_bytes);
    for event in &frees[..2] {
        assert_eq!(event.2, before_root);
        assert_eq!(event.3, before_control);
    }
    assert!(job.is_null());
    assert_eq!((destroyed.consumed, destroyed.cleanup_succeeded), (1, 1));
    assert_eq!(destroyed.unreleased_shell, SvgWorkToken::default());
    assert_eq!(destroyed.unreleased_engine, SvgWorkToken::default());
    let after_retire = host.snapshot();
    assert_eq!(after_retire.0 + 2, before_retire.0);
    assert_eq!(after_retire.1, 0);
    assert_eq!(after_retire.2 + shell_bytes + engine_bytes, before_retire.2);
    assert_eq!(
        roots.budget.usage().auxiliary_capacity + shell_bytes + engine_bytes,
        before_root
    );
    assert_eq!(roots.control() + shell_bytes + engine_bytes, before_control);
    host.with(|account, _| {
        assert!(account.token_row(shell_fee).is_err());
        assert!(account.token_row(engine_fee).is_err());
        Ok(())
    })
    .unwrap();
    drop(host);
    let mut reclaimed = SvgResourcesRetirement {
        header: SvgWorkHeader {
            abi_version: 1,
            struct_size: size_of::<SvgResourcesRetirement>() as u32,
        },
        ..Default::default()
    };
    assert_eq!(
        unsafe { notepads_markdown_svg_resources_reclaim(&mut resources, &mut reclaimed, &mut core_error()) },
        0
    );
    assert_eq!((reclaimed.consumed, reclaimed.cleanup_succeeded), (1, 1));
    assert!(resources.is_null());
    roots.account = reclaimed.account;
}
