// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE file in the project root for license information.
// ---------------------------------------------------------------------------------------------

use super::*;
use crate::core::preview::{
    notepads_markdown_preview_allocation_destroy, notepads_markdown_preview_cancel_create, notepads_markdown_preview_model_admit,
    notepads_markdown_preview_model_reserve,
};
use markdown_core::source::SourceDocument;

thread_local! {
    static RETIREMENT_PANIC: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
}
pub(super) fn take_retirement_panic() -> bool {
    RETIREMENT_PANIC.with(|slot| slot.replace(false))
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
        unsafe {
            let mut cancel = ptr::null_mut();
            assert_eq!(notepads_markdown_preview_cancel_create(&mut cancel, &mut error()), 0);
            let budget = (*cancel).value.budget.clone();
            let mut op = budget.begin((*cancel).value.token.clone());
            let document = SourceDocument::new(
                SourceKey::new([17; 16], 1).unwrap(),
                b"![svg](image)",
                &mut op,
            )
            .unwrap();
            let accepted = AcceptedModel::parse(
                document.snapshot(),
                ParseConfig::new(Profile::G3),
                1,
                &mut op,
            )
            .unwrap();
            let model = Slot::new(op.boundary_work())
                .unwrap()
                .commit(CoreModel { model: accepted });
            let mut policy = ptr::null_mut();
            assert_eq!(notepads_markdown_image_policy_create(&mut policy, &mut error()), 0);
            let epoch = ImageEpochDescriptor {
                header: ImageHeader::of::<ImageEpochDescriptor>(),
                epoch: ImageEpoch {
                    source: WireSource {
                        document: [17; 16],
                        revision: 1,
                    },
                    source_generation: 1,
                    provider_generation: 1,
                },
            };
            assert_eq!(notepads_markdown_image_policy_begin(policy, &epoch, &mut error()), 0);
            let mut requests = [ptr::null_mut(); 2];
            let mut accounts = [ptr::null_mut(); 2];
            for i in 0..2 {
                desire_image_for_test(policy, i as u32, &mut error());
                let mut info = ImageRequestInfo {
                    header: ImageHeader::of::<ImageRequestInfo>(),
                    ..Default::default()
                };
                assert_eq!(
                    notepads_markdown_image_policy_start(
                        policy,
                        i as u32,
                        2,
                        &mut requests[i],
                        &mut info,
                        &mut error()
                    ),
                    0
                );
                assert_eq!(
                    notepads_markdown_svg_work_create(
                        1,
                        requests[i],
                        model,
                        cancel,
                        &mut accounts[i],
                        &mut error()
                    ),
                    0
                );
            }
            Self {
                accounts,
                requests,
                policy,
                model,
                cancel,
                budget,
            }
        }
    }

    fn prepare(
        &self,
        image: *const Owned<ImageBytes>,
        p: u64,
        b: u64,
        preview: &mut *mut Owned<PreviewAllocation>,
    ) -> (*mut Owned<SvgRetained>, SvgWorkToken) {
        let mut retained = ptr::null_mut();
        let mut token = SvgWorkToken::default();
        unsafe {
            assert_eq!(
                notepads_markdown_svg_retained_prepare(
                    1,
                    self.accounts[0],
                    if image.is_null() { METADATA } else { PAYLOAD },
                    image,
                    preview,
                    p,
                    b,
                    &mut retained,
                    &mut token,
                    &mut error(),
                ),
                0
            );
        }
        (retained, token)
    }

    fn image(&self, bytes: u64) -> *mut Owned<ImageBytes> {
        let mut image = ptr::null_mut();
        unsafe {
            assert_eq!(
                notepads_markdown_image_request_reserve(self.requests[0], 0, bytes, &mut image, &mut error()),
                0
            );
        }
        image
    }

    fn snapshot(&self, retained: *const Owned<SvgRetained>) -> SvgRetainedSnapshot {
        let mut output = SvgRetainedSnapshot {
            header: SvgWorkHeader {
                abi_version: 1,
                struct_size: size_of::<SvgRetainedSnapshot>() as u32,
            },
            ..Default::default()
        };
        unsafe {
            assert_eq!(notepads_markdown_svg_retained_read(retained, &mut output, &mut error()), 0);
        }
        output
    }

    fn admit_preview(&self, preview: *mut Owned<PreviewAllocation>, bytes: u64) {
        unsafe {
            assert_eq!(
                notepads_markdown_preview_model_admit(self.model, self.cancel, preview, bytes, &mut error()),
                0
            );
        }
    }

    fn return_borrow(&self, token: SvgWorkToken) {
        unsafe {
            assert_eq!(
                notepads_markdown_svg_work_return_retained(self.accounts[0], &token, &mut error()),
                0
            );
        }
    }

    fn close_account(&mut self, index: usize) {
        unsafe {
            assert_eq!(notepads_markdown_svg_work_close(self.accounts[index], &mut error()), 0);
            assert_eq!(notepads_markdown_svg_work_destroy(self.accounts[index], &mut error()), 0);
        }
        self.accounts[index] = ptr::null_mut();
    }

    fn control(&self) -> u64 {
        unsafe { (&(*self.policy).value.owner).policy.svg_work_bytes() }
    }

    fn balances(&self, index: usize) -> (u64, u64, u64, u64, u64, u32, u64, u64) {
        unsafe {
            let account = &(*self.accounts[index]).value;
            let stats = account.funding.stats();
            (
                self.budget.usage().auxiliary_capacity,
                self.control(),
                stats.job_bytes,
                stats.job_peak_bytes,
                stats.root_work_bytes,
                account.tickets,
                account.arena.bytes(),
                account.funding.retained_borrow_bytes(),
            )
        }
    }

    fn reject_prepare_unchanged(
        &self,
        index: usize,
        image: *const Owned<ImageBytes>,
        p: u64,
        b: u64,
        preview: &mut *mut Owned<PreviewAllocation>,
    ) {
        let before = self.balances(index);
        let original = *preview;
        let mut output = ptr::null_mut();
        let mut token = SvgWorkToken {
            account_nonce: 33,
            slot: 44,
            generation: 55,
        };
        let sentinel = token;
        unsafe {
            assert_ne!(
                notepads_markdown_svg_retained_prepare(
                    1,
                    self.accounts[index],
                    if image.is_null() { METADATA } else { PAYLOAD },
                    image,
                    preview,
                    p,
                    b,
                    &mut output,
                    &mut token,
                    &mut error()
                ),
                0
            );
        }
        assert_eq!(self.balances(index), before);
        assert_eq!(*preview, original);
        assert!(output.is_null());
        assert_eq!(token, sentinel);
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        // Preserve the original assertion when a test fails. Its still-live
        // canonical tokens are deliberately left to process teardown rather
        // than making up physical frees while unwinding a failed test.
        if std::thread::panicking() {
            return;
        }
        unsafe {
            for i in 0..2 {
                if !self.accounts[i].is_null() {
                    self.close_account(i);
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
fn retained_metadata_owns_exact_root_records_without_pinning_the_job() {
    let _process = crate::core::images::serialize_process_image_tests();
    let mut f = Fixture::new();
    let baseline = f.budget.usage().auxiliary_capacity;
    let control = f.control();
    let mut preview = ptr::null_mut();
    let (retained, token) = f.prepare(ptr::null(), 192, 0, &mut preview);
    let initial = f.snapshot(retained);
    assert_eq!(
        (initial.phase, initial.kind, initial.refs),
        (0, METADATA, 2)
    );
    assert_eq!(
        initial.preview_bytes,
        size_of::<Owned<PreviewAllocation>>() as u64
    );
    assert_eq!(initial.header_bytes, size_of::<Owned<SvgRetained>>() as u64);
    assert_eq!(
        initial.job_borrow_bytes,
        192 + initial.preview_bytes + initial.header_bytes
    );
    assert_eq!(
        f.control() - control,
        initial.job_borrow_bytes + unsafe { (*f.accounts[0]).value.arena.bytes() }
    );
    unsafe {
        let native_layout = Layout::from_size_align(192, 8).unwrap();
        let native = alloc(native_layout);
        assert!(!native.is_null());
        notepads_markdown_svg_retained_bind_physical(retained);
        assert_eq!(f.snapshot(retained).phase, 1);
        assert_ne!(notepads_markdown_svg_retained_admit(retained, &mut error()), 0);
        f.admit_preview(preview, 192);
        assert_eq!(notepads_markdown_svg_retained_admit(retained, &mut error()), 0);
        f.return_borrow(token);
        assert_eq!(f.snapshot(retained).refs, 1);
        notepads_markdown_svg_retained_abort_unallocated(retained);
        assert_eq!(f.snapshot(retained).phase, 2);
        let h0 = size_of::<Owned<SvgWork>>() as u64;
        let a0 = (*f.accounts[0]).value.arena.bytes();
        f.close_account(0);
        assert_eq!(
            f.budget.usage().auxiliary_capacity,
            baseline + initial.job_borrow_bytes - h0
        );
        assert_eq!(f.control(), control + initial.job_borrow_bytes - h0);
        assert!(a0 > 0);
        dealloc(native, native_layout);
        assert_eq!(
            f.snapshot(retained).control_bytes,
            control + initial.job_borrow_bytes - h0
        );
        assert_eq!(notepads_markdown_preview_allocation_destroy(preview), 0);
        let after_l = f.snapshot(retained);
        assert_eq!(
            after_l.root_live_bytes,
            baseline + initial.header_bytes - h0
        );
        assert_eq!(
            after_l.control_bytes,
            control + initial.job_borrow_bytes - h0
        );
        assert_eq!(
            notepads_markdown_svg_retained_after_physical_free_checked(retained),
            0
        );
    }
    assert_eq!(
        f.budget.usage().auxiliary_capacity,
        baseline - size_of::<Owned<SvgWork>>() as u64
    );
    assert_eq!(f.control(), control - size_of::<Owned<SvgWork>>() as u64);
}

#[test]
fn retained_payload_adopt_compares_image_to_b_and_keeps_h_until_last_borrow() {
    let _process = crate::core::images::serialize_process_image_tests();
    let f = Fixture::new();
    let b = 8 * 1024 * 1024;
    let p = b + 160;
    let image = f.image(b);
    let mut preview = ptr::null_mut();
    unsafe {
        assert_eq!(notepads_markdown_image_bytes_admit(image, b, &mut error()), 0);
        assert_eq!(
            notepads_markdown_preview_model_reserve(f.model, f.cancel, p, &mut preview, &mut error()),
            0
        );
        f.admit_preview(preview, p);
        let native_layout = Layout::from_size_align(p as usize, 8).unwrap();
        let native = alloc(native_layout);
        assert!(!native.is_null());
        let before = f.budget.usage().auxiliary_capacity;
        let control = f.control();
        let (retained, token) = f.prepare(image, p, b, &mut preview);
        let mut duplicate = ptr::null_mut();
        let mut duplicate_token = SvgWorkToken::default();
        assert_ne!(
            notepads_markdown_svg_retained_prepare(
                1,
                f.accounts[0],
                PAYLOAD,
                image,
                &mut preview,
                p,
                b,
                &mut duplicate,
                &mut duplicate_token,
                &mut error()
            ),
            0
        );
        assert!(duplicate.is_null());
        notepads_markdown_svg_retained_abort_unallocated(retained); // AwaitExistingBind rejects.
        assert_eq!(f.snapshot(retained).refs, 2);
        notepads_markdown_svg_retained_bind_physical(retained);
        assert_eq!(notepads_markdown_svg_retained_admit(retained, &mut error()), 0);
        assert_ne!(notepads_markdown_svg_retained_admit(retained, &mut error()), 0);
        let paid = f.snapshot(retained);
        assert_eq!(paid.native_bytes, p);
        assert_eq!(
            paid.job_borrow_bytes,
            p + paid.preview_bytes + paid.header_bytes
        );
        let arena = (*f.accounts[0]).value.arena.bytes();
        assert_eq!(
            f.budget.usage().auxiliary_capacity - before,
            arena + paid.header_bytes
        );
        assert_eq!(f.control() - control, arena + paid.job_borrow_bytes);
        assert_eq!(notepads_markdown_image_bytes_destroy(image), 0);
        assert_eq!((&(*f.policy).value.owner).policy.usage().encoded_bytes, b);
        dealloc(native, native_layout);
        assert_eq!(notepads_markdown_preview_allocation_destroy(preview), 0);
        assert_eq!(
            notepads_markdown_svg_retained_after_physical_free_checked(retained),
            0
        );
        let retired = f.snapshot(retained);
        assert_eq!((retired.phase, retired.refs), (3, 1));
        assert_eq!(retired.control_bytes, control + arena + paid.header_bytes);
        assert_eq!((&(*f.policy).value.owner).policy.usage().encoded_bytes, 0);
        let mut no_borrow = SvgWorkToken::default();
        assert_ne!(
            notepads_markdown_svg_work_borrow_retained(f.accounts[1], retained, &mut no_borrow, &mut error()),
            0
        );
        f.return_borrow(token);
        assert_eq!(f.control(), control + arena);
    }
}

#[test]
fn retained_existing_metadata_adopts_same_root_without_repaying_native_or_preview_bytes() {
    let _process = crate::core::images::serialize_process_image_tests();
    let f = Fixture::new();
    let p = 384;
    let mut preview = ptr::null_mut();
    unsafe {
        assert_eq!(
            notepads_markdown_preview_model_reserve(f.model, f.cancel, p, &mut preview, &mut error()),
            0
        );
        let native_layout = Layout::from_size_align(p as usize, 8).unwrap();
        let native = alloc(native_layout);
        assert!(!native.is_null());
        f.admit_preview(preview, p);
        let original_preview = preview;
        let before_root = f.budget.usage().auxiliary_capacity;
        let before_control = f.control();
        let (retained, token) = f.prepare(ptr::null(), p, 0, &mut preview);
        assert_eq!(preview, original_preview);
        assert_eq!(f.snapshot(retained).phase, 0); // Wire 0 includes existing allocations awaiting bind.
        notepads_markdown_svg_retained_bind_physical(retained);
        assert_eq!(notepads_markdown_svg_retained_admit(retained, &mut error()), 0);
        let paid = f.snapshot(retained);
        assert_eq!((paid.kind, paid.phase, paid.refs), (METADATA, 2, 2));
        assert_eq!(paid.native_bytes, p);
        assert_eq!(
            paid.preview_bytes,
            size_of::<Owned<PreviewAllocation>>() as u64
        );
        let arena = (*f.accounts[0]).value.arena.bytes();
        assert_eq!(
            f.budget.usage().auxiliary_capacity - before_root,
            arena + paid.header_bytes
        );
        assert_eq!(f.control() - before_control, arena + paid.job_borrow_bytes);
        dealloc(native, native_layout);
        assert_eq!(notepads_markdown_preview_allocation_destroy(preview), 0);
        assert_eq!(
            notepads_markdown_svg_retained_after_physical_free_checked(retained),
            0
        );
        let retired = f.snapshot(retained);
        assert_eq!((retired.phase, retired.refs), (3, 1));
        assert_eq!(
            retired.control_bytes,
            before_control + arena + paid.header_bytes
        );
        f.return_borrow(token);
        assert_eq!(f.control(), before_control + arena);
        assert_eq!(
            f.budget.usage().auxiliary_capacity,
            before_root + arena - p - paid.preview_bytes
        );
    }
}

#[test]
fn retained_new_payload_requires_pending_image_and_real_root_admission() {
    let _process = crate::core::images::serialize_process_image_tests();
    let f = Fixture::new();
    let image = f.image(256);
    let mut preview = ptr::null_mut();
    let (retained, token) = f.prepare(image, 416, 256, &mut preview);
    unsafe {
        let layout = Layout::from_size_align(416, 8).unwrap();
        let native = alloc(layout);
        assert!(!native.is_null());
        notepads_markdown_svg_retained_bind_physical(retained);
        // A reserved but unadmitted preview and image are independent failures.
        assert_ne!(notepads_markdown_svg_retained_admit(retained, &mut error()), 0);
        f.admit_preview(preview, 416);
        assert_ne!(notepads_markdown_svg_retained_admit(retained, &mut error()), 0);
        assert_eq!(notepads_markdown_image_bytes_admit(image, 256, &mut error()), 0);
        assert_eq!(notepads_markdown_svg_retained_admit(retained, &mut error()), 0);
        let before = f.snapshot(retained);
        notepads_markdown_svg_retained_abort_unallocated(retained);
        assert_eq!(f.snapshot(retained).control_bytes, before.control_bytes);
        assert_eq!(f.snapshot(retained).phase, 2);
        dealloc(native, layout);
        assert_eq!(notepads_markdown_preview_allocation_destroy(preview), 0);
        assert_eq!(
            notepads_markdown_svg_retained_after_physical_free_checked(retained),
            0
        );
        assert_eq!(notepads_markdown_image_bytes_destroy(image), 0);
        f.return_borrow(token);
    }
}

#[test]
fn retained_abort_only_after_creator_return_preserves_bound_and_duplicate_tokens() {
    let _process = crate::core::images::serialize_process_image_tests();
    let f = Fixture::new();
    let mut preview = ptr::null_mut();
    let (retained, token) = f.prepare(ptr::null(), 32, 0, &mut preview);
    unsafe {
        let paid = f.snapshot(retained);
        notepads_markdown_svg_retained_abort_unallocated(retained); // creator still outstanding
        assert_eq!(f.snapshot(retained).refs, 2);
        assert_ne!(
            notepads_markdown_svg_work_release_after_free(f.accounts[0], &token, &mut error()),
            0
        );
        assert_ne!(
            notepads_markdown_svg_work_return_retained(f.accounts[1], &token, &mut error()),
            0
        );
        f.return_borrow(token);
        assert_eq!(f.snapshot(retained).refs, 1);
        assert_ne!(
            notepads_markdown_svg_work_return_retained(f.accounts[0], &token, &mut error()),
            0
        );
        assert_eq!(notepads_markdown_preview_allocation_destroy(preview), 0);
        notepads_markdown_svg_retained_abort_unallocated(retained);
        assert_eq!(f.control(), paid.control_bytes - paid.job_borrow_bytes);
    }
}

#[test]
fn retained_same_job_deduplicates_and_reborrows_without_recharging_control_or_root() {
    let _process = crate::core::images::serialize_process_image_tests();
    let f = Fixture::new();
    let mut preview = ptr::null_mut();
    let (retained, creator) = f.prepare(ptr::null(), 96, 0, &mut preview);
    unsafe {
        let layout = Layout::from_size_align(96, 8).unwrap();
        let native = alloc(layout);
        assert!(!native.is_null());
        notepads_markdown_svg_retained_bind_physical(retained);
        f.admit_preview(preview, 96);
        assert_eq!(notepads_markdown_svg_retained_admit(retained, &mut error()), 0);
        let before = f.snapshot(retained);
        let mut token = SvgWorkToken {
            account_nonce: 99,
            slot: 88,
            generation: 77,
        };
        let sentinel = token;
        assert_ne!(
            notepads_markdown_svg_work_borrow_retained(f.accounts[0], retained, &mut token, &mut error()),
            0
        );
        assert_eq!(token, sentinel);
        // Different object in the same real policy/model is also forbidden.
        let foreign_before = f.balances(1);
        assert_ne!(
            notepads_markdown_svg_work_borrow_retained(f.accounts[1], retained, &mut token, &mut error()),
            0
        );
        // Rejection is pure, including a previously empty real ticket arena.
        assert_eq!(f.balances(1), foreign_before);
        let foreign = Fixture::new();
        let foreign_policy_before = foreign.balances(0);
        assert_ne!(
            notepads_markdown_svg_work_borrow_retained(foreign.accounts[0], retained, &mut token, &mut error()),
            0
        );
        assert_eq!(foreign.balances(0), foreign_policy_before);
        let reborrow_baseline = f.snapshot(retained);
        assert_eq!(reborrow_baseline.refs, before.refs);
        assert_eq!(reborrow_baseline.control_bytes, before.control_bytes);
        assert_eq!(reborrow_baseline.root_live_bytes, before.root_live_bytes);
        f.return_borrow(creator);
        assert_eq!(
            notepads_markdown_svg_work_borrow_retained(f.accounts[0], retained, &mut token, &mut error()),
            0
        );
        let after = f.snapshot(retained);
        assert_eq!(
            (after.refs, after.control_bytes, after.root_live_bytes),
            (
                reborrow_baseline.refs,
                reborrow_baseline.control_bytes,
                reborrow_baseline.root_live_bytes
            )
        );
        assert_eq!(token.slot, creator.slot);
        assert_ne!(token.generation, creator.generation);
        assert_ne!(
            notepads_markdown_svg_work_return_retained(f.accounts[0], &creator, &mut error()),
            0
        );
        // Arena relocation copies the one row reference, rather than retaining.
        let mut ordinary = Vec::new();
        for _ in 0..17 {
            let mut t = SvgWorkToken::default();
            assert_eq!(
                notepads_markdown_svg_work_reserve(f.accounts[0], 0, 1, &mut t, &mut error()),
                0
            );
            ordinary.push(t);
        }
        assert_eq!(f.snapshot(retained).refs, 2);
        for t in ordinary {
            assert_eq!(
                notepads_markdown_svg_work_release_after_free(f.accounts[0], &t, &mut error()),
                0
            );
        }
        f.return_borrow(token);
        dealloc(native, layout);
        assert_eq!(notepads_markdown_preview_allocation_destroy(preview), 0);
        assert_eq!(
            notepads_markdown_svg_retained_after_physical_free_checked(retained),
            0
        );
    }
}

#[test]
fn retained_source_cancel_and_account_poison_allow_only_existing_cleanup() {
    let _process = crate::core::images::serialize_process_image_tests();
    let mut f = Fixture::new();
    let mut preview = ptr::null_mut();
    let (retained, token) = f.prepare(ptr::null(), 128, 0, &mut preview);
    unsafe {
        let layout = Layout::from_size_align(128, 8).unwrap();
        let native = alloc(layout);
        assert!(!native.is_null());
        notepads_markdown_svg_retained_bind_physical(retained);
        f.admit_preview(preview, 128);
        assert_eq!(notepads_markdown_svg_retained_admit(retained, &mut error()), 0);
        let epoch = ImageEpochDescriptor {
            header: ImageHeader::of::<ImageEpochDescriptor>(),
            epoch: ImageEpoch {
                source: WireSource {
                    document: [17; 16],
                    revision: 2,
                },
                source_generation: 2,
                provider_generation: 1,
            },
        };
        assert_eq!(notepads_markdown_image_policy_begin(f.policy, &epoch, &mut error()), 0);
        assert_eq!(notepads_markdown_core_cancel_request(f.cancel), 0);
        (*f.accounts[0]).value.poisoned = true;
        (&(*f.policy).value.owner)
            .poisoned
            .store(true, Ordering::Release);
        let mut fresh = SvgWorkToken::default();
        assert_ne!(
            notepads_markdown_svg_work_borrow_retained(f.accounts[0], retained, &mut fresh, &mut error()),
            0
        );
        f.return_borrow(token);
        f.close_account(0);
        // The old model root is still captured after the whole job dies.
        assert_eq!(f.snapshot(retained).refs, 1);
        dealloc(native, layout);
        assert_eq!(notepads_markdown_preview_allocation_destroy(preview), 0);
        assert_eq!(
            notepads_markdown_svg_retained_after_physical_free_checked(retained),
            0
        );
    }
}

#[test]
fn retained_prepare_failure_preserves_existing_lease_image_and_output_token() {
    let _process = crate::core::images::serialize_process_image_tests();
    let f = Fixture::new();
    let image = f.image(32);
    let mut preview = ptr::null_mut();
    unsafe {
        assert_eq!(notepads_markdown_image_bytes_admit(image, 32, &mut error()), 0);
        assert_eq!(
            notepads_markdown_preview_model_reserve(f.model, f.cancel, 192, &mut preview, &mut error()),
            0
        );
        // Existing pending preview cannot be adopted, even when P is covered.
        let mut retained = ptr::null_mut();
        let mut token = SvgWorkToken {
            account_nonce: 9,
            slot: 8,
            generation: 7,
        };
        let sentinel = token;
        let original = preview;
        let control = f.control();
        let root = f.budget.usage().auxiliary_capacity;
        assert_ne!(
            notepads_markdown_svg_retained_prepare(
                1,
                f.accounts[0],
                PAYLOAD,
                image,
                &mut preview,
                192,
                32,
                &mut retained,
                &mut token,
                &mut error()
            ),
            0
        );
        assert_eq!(preview, original);
        assert!(retained.is_null());
        assert_eq!(token, sentinel);
        assert_eq!(f.control(), control);
        assert_eq!(f.budget.usage().auxiliary_capacity, root);
        f.admit_preview(preview, 192);
        // A different policy/model root is not adopted into the captured root.
        let foreign = Fixture::new();
        assert_ne!(
            notepads_markdown_svg_retained_prepare(
                1,
                foreign.accounts[0],
                PAYLOAD,
                image,
                &mut preview,
                192,
                32,
                &mut retained,
                &mut token,
                &mut error()
            ),
            0
        );
        assert_eq!(preview, original);
        assert_eq!((*preview).value.svg_attribution, 0);
        let mut mixed_root = ptr::null_mut();
        assert_eq!(
            notepads_markdown_preview_model_reserve(
                foreign.model,
                foreign.cancel,
                192,
                &mut mixed_root,
                &mut error()
            ),
            0
        );
        foreign.admit_preview(mixed_root, 192);
        let mixed_original = mixed_root;
        // Image and account match, but the original preview comes from another
        // real model root with the same source key. Root proof must still fail.
        assert_ne!(
            notepads_markdown_svg_retained_prepare(
                1,
                f.accounts[0],
                PAYLOAD,
                image,
                &mut mixed_root,
                192,
                32,
                &mut retained,
                &mut token,
                &mut error()
            ),
            0
        );
        assert_eq!(mixed_root, mixed_original);
        assert_eq!((*mixed_root).value.svg_attribution, 0);
        assert_eq!(notepads_markdown_preview_allocation_destroy(mixed_root), 0);
        assert_eq!(notepads_markdown_preview_allocation_destroy(preview), 0);
        assert_eq!(notepads_markdown_image_bytes_destroy(image), 0);
    }
}

#[test]
fn retained_real_root_exhaustion_rolls_back_slots_control_and_creator_borrow() {
    let _process = crate::core::images::serialize_process_image_tests();
    let f = Fixture::new();
    unsafe {
        // Prime the real arena so a later failure has no retained arena delta.
        let mut ordinary = SvgWorkToken::default();
        assert_eq!(
            notepads_markdown_svg_work_reserve(f.accounts[0], 0, 1, &mut ordinary, &mut error()),
            0
        );
        assert_eq!(
            notepads_markdown_svg_work_release_after_free(f.accounts[0], &ordinary, &mut error()),
            0
        );
        let available = f.budget.limits().auxiliary_capacity - f.budget.usage().auxiliary_capacity;
        let l = size_of::<Owned<PreviewAllocation>>() as u64;
        // P and L fit, but the real H Slot does not. Its failed transaction must
        // free the already-created preview Slot before refunding control/job.
        let filler = f
            .budget
            .reserve_resource_capacity(available - l - 128)
            .unwrap();
        let before_root = f.budget.usage().auxiliary_capacity;
        let before_control = f.control();
        let before_job = (*f.accounts[0]).value.funding.stats().job_bytes;
        let mut preview = ptr::null_mut();
        let mut retained = ptr::null_mut();
        let mut token = SvgWorkToken {
            account_nonce: 2,
            slot: 3,
            generation: 4,
        };
        let sentinel = token;
        assert_ne!(
            notepads_markdown_svg_retained_prepare(
                1,
                f.accounts[0],
                METADATA,
                ptr::null(),
                &mut preview,
                128,
                0,
                &mut retained,
                &mut token,
                &mut error()
            ),
            0
        );
        assert!(preview.is_null() && retained.is_null());
        assert_eq!(token, sentinel);
        assert_eq!(f.budget.usage().auxiliary_capacity, before_root);
        assert_eq!(f.control(), before_control);
        assert_eq!((*f.accounts[0]).value.funding.stats().job_bytes, before_job);
        assert_eq!((*f.accounts[0]).value.funding.retained_borrow_bytes(), 0);
        drop(filler);
    }
}

#[test]
fn retained_read_rejects_bad_version_without_touching_output() {
    let _process = crate::core::images::serialize_process_image_tests();
    let f = Fixture::new();
    let mut preview = ptr::null_mut();
    let (retained, token) = f.prepare(ptr::null(), 32, 0, &mut preview);
    let mut snapshot = SvgRetainedSnapshot {
        owner_nonce: 999,
        ..Default::default()
    };
    unsafe {
        assert_eq!(
            notepads_markdown_svg_retained_read(retained, &mut snapshot, &mut error()),
            STATUS_ABI_MISMATCH
        );
        assert_eq!(snapshot.owner_nonce, 999);
        f.return_borrow(token);
        assert_eq!(notepads_markdown_preview_allocation_destroy(preview), 0);
        notepads_markdown_svg_retained_abort_unallocated(retained);
    }
    assert_eq!(size_of::<ImageSnapshot>(), 88);
    assert_eq!(IMAGE_ABI_VERSION, 2);
}

#[test]
fn retained_foreign_prepare_identity_and_each_root_reject_before_arena_growth() {
    let _process = crate::core::images::serialize_process_image_tests();
    let f = Fixture::new();
    let foreign = Fixture::new();
    let image = f.image(32);
    let mut preview = ptr::null_mut();
    // Pure image provenance checks must run even with no existing root lease.
    f.reject_prepare_unchanged(1, image, 192, 32, &mut preview);
    foreign.reject_prepare_unchanged(0, image, 192, 32, &mut preview);
    unsafe {
        assert_eq!(notepads_markdown_image_bytes_admit(image, 32, &mut error()), 0);
        assert_eq!(
            notepads_markdown_preview_model_reserve(f.model, f.cancel, 192, &mut preview, &mut error()),
            0
        );
        f.admit_preview(preview, 192);
        let mut other = ptr::null_mut();
        assert_eq!(
            notepads_markdown_preview_model_reserve(foreign.model, foreign.cancel, 192, &mut other, &mut error()),
            0
        );
        foreign.admit_preview(other, 192);
        let foreign_before = foreign.balances(0);
        f.reject_prepare_unchanged(0, image, 192, 32, &mut other);
        assert_eq!(foreign.balances(0), foreign_before);
        // Each original allocation stays physically paid. Swap one equal-sized
        // root lease at a time solely to exercise the two independent proofs,
        // then restore it before either real allocation is destroyed.
        std::mem::swap(
            &mut (*preview).value.reservation,
            &mut (*other).value.reservation,
        );
        f.reject_prepare_unchanged(0, image, 192, 32, &mut preview);
        assert_eq!(foreign.balances(0), foreign_before);
        std::mem::swap(
            &mut (*preview).value.reservation,
            &mut (*other).value.reservation,
        );
        std::mem::swap(&mut (*preview).lease, &mut (*other).lease);
        f.reject_prepare_unchanged(0, image, 192, 32, &mut preview);
        assert_eq!(foreign.balances(0), foreign_before);
        std::mem::swap(&mut (*preview).lease, &mut (*other).lease);
        assert_eq!((*preview).value.svg_attribution, 0);
        assert_eq!((*other).value.svg_attribution, 0);
        assert_eq!(notepads_markdown_preview_allocation_destroy(preview), 0);
        assert_eq!(notepads_markdown_preview_allocation_destroy(other), 0);
        assert_eq!(notepads_markdown_image_bytes_destroy(image), 0);
    }
}

#[test]
fn retained_canceled_or_retired_prepare_rejects_before_first_arena() {
    let _process = crate::core::images::serialize_process_image_tests();
    let canceled = Fixture::new();
    let mut preview = ptr::null_mut();
    unsafe {
        assert_eq!(notepads_markdown_core_cancel_request(canceled.cancel), 0);
    }
    canceled.reject_prepare_unchanged(0, ptr::null(), 192, 0, &mut preview);
    let retired = Fixture::new();
    let epoch = ImageEpochDescriptor {
        header: ImageHeader::of::<ImageEpochDescriptor>(),
        epoch: ImageEpoch {
            source: WireSource {
                document: [17; 16],
                revision: 2,
            },
            source_generation: 2,
            provider_generation: 1,
        },
    };
    unsafe {
        assert_eq!(
            notepads_markdown_image_policy_begin(retired.policy, &epoch, &mut error()),
            0
        );
    }
    retired.reject_prepare_unchanged(0, ptr::null(), 192, 0, &mut preview);
}

#[test]
fn retained_real_system_p_l_h_deallocation_observes_each_still_paid_stage() {
    let _process = crate::core::images::serialize_process_image_tests();
    let f = Fixture::new();
    let mut preview = ptr::null_mut();
    let (retained, token) = f.prepare(ptr::null(), 160, 0, &mut preview);
    unsafe {
        let layout = Layout::from_size_align(160, 8).unwrap();
        let native = alloc(layout);
        assert!(!native.is_null());
        notepads_markdown_svg_retained_bind_physical(retained);
        f.admit_preview(preview, 160);
        assert_eq!(notepads_markdown_svg_retained_admit(retained, &mut error()), 0);
        let paid = f.snapshot(retained);
        let watched = [native as usize, preview as usize, retained as usize];
        super::super::tests::arm_retained_probe(
            &f.budget,
            &(&(*f.policy).value.owner).policy,
            watched,
        );
        dealloc(native, layout);
        assert_eq!(notepads_markdown_preview_allocation_destroy(preview), 0);
        assert_eq!(
            notepads_markdown_svg_retained_after_physical_free_checked(retained),
            0
        );
        let retired = f.snapshot(retained);
        assert_eq!((retired.phase, retired.refs), (3, 1));
        f.return_borrow(token);
        let (written, events) = super::super::tests::take_retained_probe();
        assert_eq!(written, 3);
        assert_eq!(
            events[0],
            (watched[0], 160, paid.root_live_bytes, paid.control_bytes)
        );
        assert_eq!(
            events[1],
            (
                watched[1],
                paid.preview_bytes as usize,
                paid.root_live_bytes - paid.native_bytes,
                paid.control_bytes,
            )
        );
        assert_eq!(
            events[2],
            (
                watched[2],
                paid.header_bytes as usize,
                paid.root_live_bytes - paid.native_bytes - paid.preview_bytes,
                paid.control_bytes - paid.native_bytes - paid.preview_bytes,
            )
        );
        assert_eq!(
            f.budget.usage().auxiliary_capacity,
            paid.root_live_bytes - paid.job_borrow_bytes
        );
        assert_eq!(f.control(), paid.control_bytes - paid.job_borrow_bytes);
    }
}

#[test]
fn retained_refund_unwind_never_publishes_success_or_allows_another_borrow() {
    let _process = crate::core::images::serialize_process_image_tests();
    let f = Fixture::new();
    let mut preview = ptr::null_mut();
    let (retained, token) = f.prepare(ptr::null(), 192, 0, &mut preview);
    unsafe {
        let layout = Layout::from_size_align(192, 8).unwrap();
        let native = alloc(layout);
        assert!(!native.is_null());
        notepads_markdown_svg_retained_bind_physical(retained);
        f.admit_preview(preview, 192);
        assert_eq!(notepads_markdown_svg_retained_admit(retained, &mut error()), 0);
        dealloc(native, layout);
        assert_eq!(notepads_markdown_preview_allocation_destroy(preview), 0);
        RETIREMENT_PANIC.with(|slot| slot.set(true));
        assert_eq!(
            notepads_markdown_svg_retained_after_physical_free_checked(retained),
            STATUS_PANIC
        );
        let failed = f.snapshot(retained);
        assert_eq!((failed.phase, failed.refs), (4, 2));
        assert_eq!(failed.native_bytes, 192); // Immutable Layout, not a live gauge.
        let mut rejected = SvgWorkToken {
            account_nonce: 55,
            slot: 66,
            generation: 77,
        };
        let sentinel = rejected;
        assert_ne!(
            notepads_markdown_svg_work_borrow_retained(f.accounts[0], retained, &mut rejected, &mut error()),
            0
        );
        assert_eq!(rejected, sentinel);
        assert!((*f.accounts[0]).value.token_row(token).unwrap().state == TicketState::Borrowed);
    }
    // The actual native/preview storage is gone. Keep the original canonical
    // funding/record/physical reference and creator row; never retry this free.
    std::mem::forget(f);
}

#[test]
fn checked_physical_event_can_destroy_last_record_without_a_post_return_read() {
    let _process = crate::core::images::serialize_process_image_tests();
    let f = Fixture::new();
    let mut preview = ptr::null_mut();
    let (retained, token) = f.prepare(ptr::null(), 224, 0, &mut preview);
    // SAFETY: real allocation/free followed by actual original preview record
    // destruction. No access to retained after its final checked event.
    unsafe {
        let layout = Layout::from_size_align(224, 8).unwrap();
        let native = alloc(layout);
        assert!(!native.is_null());
        notepads_markdown_svg_retained_bind_physical(retained);
        f.admit_preview(preview, 224);
        assert_eq!(notepads_markdown_svg_retained_admit(retained, &mut error()), 0);
        let paid = f.snapshot(retained);
        f.return_borrow(token);
        assert_eq!(f.snapshot(retained).refs, 1);
        dealloc(native, layout);
        assert_eq!(notepads_markdown_preview_allocation_destroy(preview), 0);
        assert_eq!(notepads_markdown_svg_retained_after_physical_free_checked(retained), 0);
        assert_eq!(
            f.budget.usage().auxiliary_capacity,
            paid.root_live_bytes - paid.job_borrow_bytes
        );
        assert_eq!(f.control(), paid.control_bytes - paid.job_borrow_bytes);
    }
}

#[test]
fn checked_physical_event_rejects_unbound_state_without_refund_or_ref_change() {
    let _process = crate::core::images::serialize_process_image_tests();
    let f = Fixture::new();
    let mut preview = ptr::null_mut();
    let (retained, token) = f.prepare(ptr::null(), 208, 0, &mut preview);
    let before = f.snapshot(retained);
    // SAFETY: live retained record; negative phase/pointer preflight returns
    // before any physical acknowledgement or cleanup mutation.
    unsafe {
        assert_eq!(
            notepads_markdown_svg_retained_after_physical_free_checked(ptr::null_mut()),
            STATUS_INVALID_ARGUMENT
        );
        assert_eq!(
            notepads_markdown_svg_retained_after_physical_free_checked(retained),
            crate::STATUS_INVALID_STATE
        );
        let after = f.snapshot(retained);
        assert_eq!(
            (
                after.phase,
                after.refs,
                after.control_bytes,
                after.root_live_bytes
            ),
            (
                before.phase,
                before.refs,
                before.control_bytes,
                before.root_live_bytes
            )
        );
        f.return_borrow(token);
        assert_eq!(notepads_markdown_preview_allocation_destroy(preview), 0);
        notepads_markdown_svg_retained_abort_unallocated(retained);
    }
}

#[test]
fn checked_physical_event_reports_refund_panic_and_keeps_failed_record_paid() {
    let _process = crate::core::images::serialize_process_image_tests();
    let f = Fixture::new();
    let mut preview = ptr::null_mut();
    let (retained, token) = f.prepare(ptr::null(), 240, 0, &mut preview);
    // SAFETY: physical storage is actually freed; creator borrow pins the
    // phase4 record while checking the injected refund failure.
    unsafe {
        let layout = Layout::from_size_align(240, 8).unwrap();
        let native = alloc(layout);
        assert!(!native.is_null());
        notepads_markdown_svg_retained_bind_physical(retained);
        f.admit_preview(preview, 240);
        assert_eq!(notepads_markdown_svg_retained_admit(retained, &mut error()), 0);
        let paid = f.snapshot(retained);
        dealloc(native, layout);
        assert_eq!(notepads_markdown_preview_allocation_destroy(preview), 0);
        RETIREMENT_PANIC.with(|slot| slot.set(true));
        assert_eq!(
            notepads_markdown_svg_retained_after_physical_free_checked(retained),
            STATUS_PANIC
        );
        let failed = f.snapshot(retained);
        // Unwind drops the already-taken physical retirement guard: P/L
        // really refunded after actual native/root free. The phase4 record,
        // its H and original job's full borrow claim remain, not a fake phase3.
        assert_eq!(
            (failed.phase, failed.refs, failed.control_bytes),
            (
                4,
                2,
                paid.control_bytes - paid.native_bytes - paid.preview_bytes
            )
        );
        assert_eq!(failed.job_borrow_bytes, paid.job_borrow_bytes);
        assert!((*f.accounts[0]).value.token_row(token).unwrap().state == TicketState::Borrowed);
    }
    // Uncertain refund intentionally preserves the original canonical owner,
    // account, creator row and fees; never retry the acknowledged free.
    std::mem::forget(f);
}
