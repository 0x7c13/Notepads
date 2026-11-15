// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE file in the project root for license information.
// ---------------------------------------------------------------------------------------------

use super::*;
use crate::identity::SourceKey;
use crate::policy::SourceLimits;
use crate::resources::{ImageResourcePolicy, ImageTarget, SourceEpoch};

fn epoch(generation: u64) -> SourceEpoch {
    SourceEpoch {
        source: SourceKey::new([91; 16], generation).unwrap(),
        source_generation: generation,
        provider_generation: 1,
    }
}

fn policy() -> ImageResourcePolicy {
    let policy = ImageResourcePolicy::new();
    policy.begin_source(epoch(1)).unwrap();
    policy
}

fn request(policy: &ImageResourcePolicy, object: u32) -> RequestPermit {
    policy.desire_variant(object, target(1)).unwrap();
    policy.start_variant(object).unwrap()
}

fn target(generation: u64) -> ImageTarget {
    ImageTarget {
        font_generation: 1,
        generation,
        available_width_dip: 64.0,
        effective_physical_scale: 1.0,
    }
}

fn budget(bytes: u64) -> SourceBudget {
    SourceBudget::new(SourceLimits {
        auxiliary_capacity: bytes,
        ..SourceLimits::default()
    })
    .unwrap()
}

fn create(request: &RequestPermit, root: &SourceBudget) -> (SvgWorkAccount, SvgHeaderCharge) {
    request
        .begin_svg_work(root, CancellationToken::new(), 64)
        .unwrap()
        .commit()
}

fn replacement_request(policy: &ImageResourcePolicy, object: u32) -> RequestPermit {
    policy.desire_variant(object, target(2)).unwrap();
    policy.start_variant(object).unwrap()
}

#[test]
fn two_jobs_share_control_but_each_keeps_its_model_root() {
    let policy = policy();
    let first_request = request(&policy, 1);
    let second_request = request(&policy, 2);
    let first_root = budget(MAX_SVG_CONTROL_BYTES);
    let second_root = budget(MAX_SVG_CONTROL_BYTES);
    let (mut first, first_header) = create(&first_request, &first_root);
    let (mut second, second_header) = create(&second_request, &second_root);
    let available = MAX_SVG_CONTROL_BYTES - 128;
    first.reserve_work_bytes(available).unwrap();
    assert_eq!(first.stats().control_bytes, MAX_SVG_CONTROL_BYTES);
    assert_eq!(first_root.usage().auxiliary_capacity, available);
    assert_eq!(second_root.usage().auxiliary_capacity, 0);
    let before = second.stats();
    assert!(matches!(
        second.reserve_work_bytes(1),
        Err(SvgWorkError::ControlLimit { .. })
    ));
    assert_eq!(second.stats(), before);
    first.release_freed_work_bytes(available).unwrap();
    second.reserve_work_bytes(17).unwrap();
    assert_eq!(first_root.usage().auxiliary_capacity, 0);
    assert_eq!(second_root.usage().auxiliary_capacity, 17);
    assert_eq!(policy.usage().reservations, 0);
    second.release_freed_work_bytes(17).unwrap();
    first.close();
    second.close();
    drop((first, second, first_header, second_header));
    assert_eq!(policy.shared.lock().svg_work_live, 0);
}

#[test]
fn job_header_is_included_and_checked_overflow_never_changes_balance() {
    let policy = policy();
    let request = request(&policy, 1);
    let root = budget(MAX_SVG_JOB_BYTES);
    let (mut account, header) = create(&request, &root);
    let available = MAX_SVG_JOB_BYTES - 64;
    account.reserve_work_bytes(available).unwrap();
    let before = account.stats();
    assert!(matches!(
        account.reserve_work_bytes(1),
        Err(SvgWorkError::JobLimit { .. })
    ));
    assert_eq!(account.stats(), before);
    assert_eq!(
        account.reserve_work_bytes(u64::MAX),
        Err(SvgWorkError::ArithmeticOverflow)
    );
    assert_eq!(account.stats(), before);
    account.release_freed_work_bytes(available).unwrap();
    account.close();
    drop((account, header));
    assert_eq!(root.usage().auxiliary_capacity, 0);
}

#[test]
fn root_limit_failure_and_real_growth_then_cancel_restore_actual_grant() {
    let policy = policy();
    let request = request(&policy, 1);
    let root = budget(32);
    let cancel = CancellationToken::new();
    let (mut account, header) = request
        .begin_svg_work(&root, cancel.clone(), 64)
        .unwrap()
        .commit();
    account.reserve_work_bytes(16).unwrap();
    let before = account.stats();
    assert!(matches!(
        account.reserve_work_bytes(17),
        Err(SvgWorkError::Source(CoreError::ResourceLimit { .. }))
    ));
    assert_eq!(account.stats(), before);
    let error = account.grow_with(8, true, |grant, total, op| {
        grant.admit_actual(total, op)?;
        assert_eq!(grant.reserved_bytes(), 24);
        assert_eq!(root.usage().auxiliary_capacity, 24);
        cancel.cancel();
        op.boundary_work().checkpoint_now()
    });
    assert_eq!(error, Err(SvgWorkError::Source(CoreError::Cancelled)));
    assert_eq!(account.stats(), before);
    assert_eq!(root.usage().auxiliary_capacity, 16);
    account.release_freed_work_bytes(16).unwrap();
    account.close();
    drop((account, header));
}

#[test]
fn cancellation_and_source_retirement_keep_live_bytes_until_free_ack() {
    let policy = policy();
    let request = request(&policy, 1);
    let root = budget(1024);
    let cancel = CancellationToken::new();
    let (mut account, header) = request
        .begin_svg_work(&root, cancel.clone(), 64)
        .unwrap()
        .commit();
    account.reserve_work_bytes(32).unwrap();
    let payload = vec![0_u8; 32];
    assert_eq!(payload.len(), 32);
    policy.begin_source(epoch(2)).unwrap();
    assert!(!account.is_current());
    assert!(matches!(
        account.reserve_work_bytes(1),
        Err(SvgWorkError::Resource(ResourceError::StaleResult))
    ));
    cancel.cancel();
    account.close();
    assert_eq!(root.usage().auxiliary_capacity, 32);
    assert_eq!(account.stats().control_bytes, 96);
    drop(payload);
    assert_eq!(account.stats().root_work_bytes, 32);
    account.release_freed_work_bytes(32).unwrap();
    assert_eq!(root.usage().auxiliary_capacity, 0);
    assert_eq!(account.stats().control_bytes, 64);
    drop(account);
    assert_eq!(policy.shared.lock().svg_work_live, 64);
    drop(header);
    assert_eq!(policy.shared.lock().svg_work_live, 0);
}

#[test]
fn failed_bootstrap_restores_eligibility_but_success_is_permanent() {
    let policy = policy();
    let request = request(&policy, 1);
    let root = budget(1024);
    let cancelled = CancellationToken::new();
    cancelled.cancel();
    assert!(matches!(
        request.begin_svg_work(&root, cancelled, 64),
        Err(SvgWorkError::Source(CoreError::Cancelled))
    ));
    assert_eq!(request.svg_account.load(Ordering::Acquire), 0);
    {
        let _failed_outer_slot = request
            .begin_svg_work(&root, CancellationToken::new(), 64)
            .unwrap();
        assert_eq!(policy.shared.lock().svg_work_live, 64);
    }
    assert_eq!(policy.shared.lock().svg_work_live, 0);
    assert_eq!(request.svg_account.load(Ordering::Acquire), 0);
    let (mut account, header) = create(&request, &root);
    assert!(matches!(
        request.begin_svg_work(&root, CancellationToken::new(), 64),
        Err(SvgWorkError::InvalidState)
    ));
    account.close();
    drop((account, header));
    assert!(matches!(
        request.begin_svg_work(&root, CancellationToken::new(), 64),
        Err(SvgWorkError::InvalidState)
    ));
}

#[test]
fn existing_allocation_reconciles_without_making_stale_source_current() {
    let policy = policy();
    let request = request(&policy, 1);
    let root = budget(128);
    let (mut account, header) = create(&request, &root);
    account.reserve_work_bytes(64).unwrap();
    policy.begin_source(epoch(2)).unwrap();
    account.reconcile_work_bytes(64, 80).unwrap();
    assert_eq!(root.usage().auxiliary_capacity, 80);
    assert!(!account.is_current());
    account.reconcile_work_bytes(80, 32).unwrap();
    assert_eq!(root.usage().auxiliary_capacity, 32);
    let before = account.stats();
    assert_eq!(
        account.release_freed_work_bytes(33),
        Err(SvgWorkError::InvalidSize)
    );
    assert_eq!(account.stats(), before);
    account.close();
    account.release_freed_work_bytes(32).unwrap();
    drop((account, header));
}

#[test]
fn independent_policies_do_not_share_svg_amounts() {
    let first_policy = policy();
    let second_policy = policy();
    let first_request = request(&first_policy, 1);
    let second_request = request(&second_policy, 1);
    let root = budget(MAX_SVG_CONTROL_BYTES * 2);
    let (mut first, first_header) = create(&first_request, &root);
    let (mut second, second_header) = create(&second_request, &root);
    let bytes = MAX_SVG_JOB_BYTES - 64;
    first.reserve_work_bytes(bytes).unwrap();
    second.reserve_work_bytes(bytes).unwrap();
    assert_eq!(first.stats().control_bytes, MAX_SVG_CONTROL_BYTES);
    assert_eq!(second.stats().control_bytes, MAX_SVG_CONTROL_BYTES);
    first.release_freed_work_bytes(bytes).unwrap();
    second.release_freed_work_bytes(bytes).unwrap();
    first.close();
    second.close();
    drop((first, second, first_header, second_header));
    assert_eq!(root.usage().auxiliary_capacity, 0);
}

#[test]
fn retained_replacement_jobs_pay_borrow_each_but_control_and_root_once() {
    let policy = policy();
    let first_request = request(&policy, 1);
    let root = budget(4096);
    let (mut first, first_header) = create(&first_request, &root);
    let image = first_request.reserve_encoded(100).unwrap();
    image.admit_actual(100).unwrap();
    // P+L already belongs to the original real preview root, not work aggregate.
    let physical_root = root.reserve_resource_capacity(160).unwrap();
    let bootstrap = first.prepare_retained(Some(&image), 128, 32, 48).unwrap();
    assert!(bootstrap.budget().same_root(&root));
    assert_eq!(bootstrap.key(), first_request.key());
    let record_root = bootstrap.budget().reserve_resource_capacity(48).unwrap();
    let (mut funding, retained_header) = bootstrap.commit();
    assert!(funding.matches_image(&image));
    assert!(funding.belongs_to(&physical_root));
    assert_eq!(first.stats().job_bytes, 64 + 208);
    assert_eq!(first.retained_borrow_bytes(), 208);
    assert_eq!(first.stats().root_work_bytes, 0);
    assert_eq!(root.usage().auxiliary_capacity, 208);
    drop(first_request);
    let second_request = replacement_request(&policy, 1);
    assert_ne!(second_request.key().request, funding.key().request);
    let (mut second, second_header) = create(&second_request, &root);
    let control_before = policy.svg_work_bytes();
    let root_before = root.usage();
    let borrowed = second.borrow_retained(&funding).unwrap();
    assert_eq!(borrowed, 208);
    assert_eq!(second.stats().job_bytes, 64 + 208);
    assert_eq!(policy.svg_work_bytes(), control_before);
    assert_eq!(root.usage(), root_before);
    first.return_retained(208).unwrap();
    assert_eq!(second.retained_borrow_bytes(), 208);
    // Actual external owner and preview record are retired before this event.
    drop(physical_root);
    drop(funding.take_physical_retirement());
    assert_eq!(policy.svg_work_bytes(), 128 + 48);
    assert_eq!(second.retained_borrow_bytes(), 208);
    assert_eq!(
        second.borrow_retained(&funding),
        Err(SvgWorkError::InvalidState)
    );
    second.return_retained(borrowed).unwrap();
    drop(funding);
    drop(record_root);
    assert_eq!(policy.svg_work_bytes(), 128 + 48);
    drop(retained_header);
    drop((first, second, first_header, second_header, image));
    assert_eq!(policy.svg_work_bytes(), 0);
    assert_eq!(policy.usage().reservations, 0);
    assert_eq!(root.usage().auxiliary_capacity, 0);
}

#[test]
fn retained_rejects_other_object_policy_root_and_source_without_claims() {
    let policy = policy();
    let origin_request = request(&policy, 1);
    let root = budget(4096);
    let (mut origin, origin_header) = create(&origin_request, &root);
    let image = origin_request.reserve_encoded(100).unwrap();
    let (mut funding, retained_header) = origin
        .prepare_retained(Some(&image), 128, 32, 48)
        .unwrap()
        .commit();
    let other_request = request(&policy, 2);
    let (mut other_object, other_header) = create(&other_request, &root);
    let before = other_object.stats();
    assert_eq!(
        other_object.borrow_retained(&funding),
        Err(SvgWorkError::InvalidState)
    );
    assert!(matches!(
        other_object.prepare_retained(Some(&image), 128, 32, 48),
        Err(SvgWorkError::InvalidState)
    ));
    assert_eq!(other_object.stats(), before);
    drop((other_object, other_header, other_request, origin_request));
    let replacement = replacement_request(&policy, 1);
    let foreign_root = budget(4096);
    let (mut other_root, other_header) = create(&replacement, &foreign_root);
    let before = other_root.stats();
    assert_eq!(
        other_root.borrow_retained(&funding),
        Err(SvgWorkError::InvalidState)
    );
    assert_eq!(other_root.stats(), before);
    let foreign_policy = ImageResourcePolicy::new();
    foreign_policy.begin_source(epoch(1)).unwrap();
    let foreign_request = request(&foreign_policy, 1);
    let (mut other_policy, foreign_header) = create(&foreign_request, &root);
    let before = other_policy.stats();
    assert_eq!(
        other_policy.borrow_retained(&funding),
        Err(SvgWorkError::InvalidState)
    );
    assert!(matches!(
        other_policy.prepare_retained(Some(&image), 128, 32, 48),
        Err(SvgWorkError::InvalidState)
    ));
    assert_eq!(other_policy.stats(), before);
    drop((other_root, other_header, replacement));
    policy.begin_source(epoch(2)).unwrap();
    let new_source_request = request(&policy, 1);
    let (mut other_source, new_header) = create(&new_source_request, &root);
    let before = other_source.stats();
    assert_eq!(
        other_source.borrow_retained(&funding),
        Err(SvgWorkError::InvalidState)
    );
    assert_eq!(other_source.stats(), before);
    origin.return_retained(funding.total_bytes()).unwrap();
    drop(funding.take_physical_retirement());
    drop((
        funding,
        retained_header,
        origin,
        origin_header,
        other_policy,
        foreign_header,
        other_source,
        new_header,
        image,
    ));
    assert_eq!(policy.svg_work_bytes(), 0);
    assert_eq!(foreign_policy.svg_work_bytes(), 0);
}

#[test]
fn retained_bootstrap_preclaims_metadata_and_rolls_back_on_root_failure() {
    let policy = policy();
    let request = request(&policy, 1);
    let root = budget(207);
    let (mut account, header) = create(&request, &root);
    let before = account.stats();
    {
        let bootstrap = account.prepare_retained(None, 128, 32, 48).unwrap();
        assert_eq!(policy.svg_work_bytes(), 64 + 208);
        assert_eq!(root.usage().auxiliary_capacity, 0);
        let physical_root = bootstrap.budget().reserve_resource_capacity(160).unwrap();
        assert!(matches!(
            bootstrap.budget().reserve_resource_capacity(48),
            Err(CoreError::ResourceLimit { .. })
        ));
        drop(physical_root); // physical Slot is retired before bootstrap rollback.
        drop(bootstrap);
    }
    assert_eq!(account.stats(), before);
    assert_eq!(account.retained_borrow_bytes(), 0);
    assert_eq!(root.usage().auxiliary_capacity, 0);
    assert_eq!(policy.usage().reservations, 0);
    let (mut funding, retained_header) = account.prepare_retained(None, 16, 8, 8).unwrap().commit();
    assert!(funding.image().is_none());
    assert_eq!(funding.stats().native_bytes, 16);
    assert_eq!(funding.stats().preview_bytes, 8);
    assert_eq!(funding.stats().header_bytes, 8);
    account.return_retained(32).unwrap();
    drop(funding.take_physical_retirement());
    assert_eq!(policy.svg_work_bytes(), 64 + 8);
    drop((funding, retained_header, account, header));
    assert_eq!(policy.svg_work_bytes(), 0);
}

#[test]
fn retained_image_at_encoded_limit_is_independent_of_larger_native_layout() {
    let policy = policy();
    let request = request(&policy, 1);
    let root = budget(16 * 1024 * 1024);
    let (mut account, header) = create(&request, &root);
    let image = request
        .reserve_encoded(super::super::MAX_ENCODED_IMAGE_BYTES)
        .unwrap();
    image
        .admit_actual(super::super::MAX_ENCODED_IMAGE_BYTES)
        .unwrap();
    let native_bytes = image.reserved_bytes() + 256;
    let (mut funding, retained_header) = account
        .prepare_retained(Some(&image), native_bytes, 32, 48)
        .unwrap()
        .commit();
    assert_eq!(
        funding.image().unwrap().actual_bytes(),
        Some(super::super::MAX_ENCODED_IMAGE_BYTES)
    );
    let different = request.reserve_encoded(1).unwrap();
    assert!(!funding.matches_image(&different));
    account.return_retained(funding.total_bytes()).unwrap();
    drop(image);
    assert_eq!(
        policy.usage().encoded_bytes,
        super::super::MAX_ENCODED_IMAGE_BYTES + 1
    );
    let retirement = funding.take_physical_retirement();
    assert_eq!(
        policy.usage().encoded_bytes,
        super::super::MAX_ENCODED_IMAGE_BYTES + 1
    );
    drop(retirement);
    assert_eq!(policy.usage().encoded_bytes, 1);
    drop((funding, retained_header, different, account, header));
    assert_eq!(policy.usage().reservations, 0);
    assert_eq!(policy.svg_work_bytes(), 0);
}

#[test]
fn retained_cleanup_survives_poison_cancel_source_close_and_account_death() {
    let policy = policy();
    let request = request(&policy, 1);
    let root = budget(4096);
    let cancel = CancellationToken::new();
    let (mut account, header) = request
        .begin_svg_work(&root, cancel.clone(), 64)
        .unwrap()
        .commit();
    let image = request.reserve_decoded(128).unwrap();
    let (mut funding, retained_header) = account
        .prepare_retained(Some(&image), 160, 32, 48)
        .unwrap()
        .commit();
    drop(image);
    policy.begin_source(epoch(2)).unwrap();
    cancel.cancel();
    account.close();
    let poison_shared = policy.shared.clone();
    let poisoned = std::thread::spawn(move || {
        let _lock = poison_shared.state.lock().unwrap();
        panic!("intentional policy poison before canonical cleanup");
    })
    .join();
    assert!(poisoned.is_err());
    assert!(policy.shared.state.is_poisoned());
    account.return_retained(240).unwrap();
    assert_eq!(account.stats().job_bytes, 64);
    drop((account, header, request));
    assert_eq!(policy.svg_work_bytes(), 240);
    assert_eq!(policy.usage().decoded_bytes, 128);
    drop(funding.take_physical_retirement());
    assert_eq!(policy.svg_work_bytes(), 48);
    assert_eq!(policy.usage().decoded_bytes, 0);
    drop(funding);
    assert_eq!(policy.svg_work_bytes(), 48);
    drop(retained_header);
    assert_eq!(policy.svg_work_bytes(), 0);
}

#[test]
fn retained_borrow_counts_against_work_growth_and_old_work_cannot_return_it() {
    let policy = policy();
    let request = request(&policy, 1);
    let root = budget(MAX_SVG_JOB_BYTES);
    let (mut account, header) = create(&request, &root);
    let p = MAX_SVG_JOB_BYTES - 64 - 32 - 48;
    let (mut funding, retained_header) =
        account.prepare_retained(None, p, 32, 48).unwrap().commit();
    let before = account.stats();
    assert_eq!(before.job_bytes, MAX_SVG_JOB_BYTES);
    assert_eq!(before.root_work_bytes, 0);
    assert!(matches!(
        account.reserve_work_bytes(1),
        Err(SvgWorkError::JobLimit { .. })
    ));
    assert!(matches!(
        account.borrow_retained(&funding),
        Err(SvgWorkError::JobLimit { .. })
    ));
    assert_eq!(
        account.release_freed_work_bytes(1),
        Err(SvgWorkError::InvalidSize)
    );
    assert_eq!(account.stats(), before);
    assert_eq!(account.return_retained(0), Err(SvgWorkError::InvalidSize));
    assert_eq!(
        account.return_retained(MAX_SVG_JOB_BYTES),
        Err(SvgWorkError::InvalidSize)
    );
    account.return_retained(funding.total_bytes()).unwrap();
    // Control still contains physical owner, even though the creator job ended.
    assert!(matches!(
        account.reserve_work_bytes(1),
        Err(SvgWorkError::ControlLimit { .. })
    ));
    drop(funding.take_physical_retirement());
    drop((funding, retained_header));
    account.reserve_work_bytes(1).unwrap();
    assert_eq!(account.stats().job_bytes, 65);
    assert_eq!(account.stats().root_work_bytes, 1);
    account.release_freed_work_bytes(1).unwrap();
    drop((account, header));
    assert_eq!(policy.svg_work_bytes(), 0);
}

#[test]
fn retained_invalid_amounts_overflow_control_and_cancel_never_change_balances() {
    let policy = policy();
    let first_request = request(&policy, 1);
    let root = budget(1024);
    let cancel = CancellationToken::new();
    let (mut account, header) = first_request
        .begin_svg_work(&root, cancel.clone(), 64)
        .unwrap()
        .commit();
    let before = account.stats();
    assert!(matches!(
        account.prepare_retained(None, 0, 1, 1),
        Err(SvgWorkError::InvalidSize)
    ));
    assert!(matches!(
        account.prepare_retained(None, u64::MAX, 1, 1),
        Err(SvgWorkError::ArithmeticOverflow)
    ));
    assert!(matches!(
        account.prepare_retained(None, MAX_SVG_JOB_BYTES, 1, 1),
        Err(SvgWorkError::JobLimit { .. })
    ));
    assert_eq!(account.stats(), before);
    let other_request = request(&policy, 2);
    let (other, other_header) = create(&other_request, &root);
    assert!(matches!(
        account.prepare_retained(None, MAX_SVG_JOB_BYTES - 64 - 2, 1, 1),
        Err(SvgWorkError::ControlLimit { .. })
    ));
    assert_eq!(account.retained_borrow_bytes(), 0);
    cancel.cancel();
    assert!(matches!(
        account.prepare_retained(None, 1, 1, 1),
        Err(SvgWorkError::Source(CoreError::Cancelled))
    ));
    assert_eq!(account.retained_borrow_bytes(), 0);
    drop((account, header, other, other_header));
    assert_eq!(policy.svg_work_bytes(), 0);
}

#[test]
fn retained_root_proof_is_read_only_and_never_admits_wrong_root() {
    let root = budget(1024);
    let foreign_root = budget(1024);
    let lease = root.reserve_resource_capacity(32).unwrap();
    let before = root.usage();
    assert!(lease.belongs_to(&root));
    assert!(lease.belongs_to(&root.clone()));
    assert!(!lease.belongs_to(&foreign_root));
    assert_eq!(root.usage(), before);
    assert_eq!(foreign_root.usage().auxiliary_capacity, 0);
    drop(lease);
    assert_eq!(root.usage().auxiliary_capacity, 0);
}

#[test]
fn retained_new_borrow_checks_current_cancel_and_close_before_any_claim() {
    for gate in 0..3 {
        let policy = policy();
        let request = request(&policy, 1);
        let root = budget(4096);
        let cancel = CancellationToken::new();
        let (mut account, header) = request
            .begin_svg_work(&root, cancel.clone(), 64)
            .unwrap()
            .commit();
        let (mut funding, retained_header) = account
            .prepare_retained(None, 128, 32, 48)
            .unwrap()
            .commit();
        account.return_retained(funding.total_bytes()).unwrap();
        let expected = match gate {
            0 => {
                cancel.cancel();
                SvgWorkError::Source(CoreError::Cancelled)
            }
            1 => {
                policy.begin_source(epoch(2)).unwrap();
                SvgWorkError::Resource(ResourceError::StaleResult)
            }
            _ => {
                account.close();
                SvgWorkError::InvalidState
            }
        };
        let before = account.stats();
        assert_eq!(account.borrow_retained(&funding), Err(expected));
        assert_eq!(account.stats(), before);
        assert_eq!(account.retained_borrow_bytes(), 0);
        drop(funding.take_physical_retirement());
        drop((funding, retained_header, account, header));
        assert_eq!(policy.svg_work_bytes(), 0);
    }
}
