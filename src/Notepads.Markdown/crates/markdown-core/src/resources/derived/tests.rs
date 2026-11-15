// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE file in the project root for license information.
// ---------------------------------------------------------------------------------------------

use super::*;
use crate::config::{ParseConfig, Profile};
use crate::error::LimitKind;
use crate::identity::SourceKey;
use crate::policy::{CancellationToken, SourceLimits};
use crate::source::SourceDocument;
use std::sync::Barrier;

fn model(root: &SourceBudget, revision: u64) -> AcceptedModel {
    let mut op = root.begin(CancellationToken::new());
    let source = SourceDocument::new(
        SourceKey::new([193; 16], revision).unwrap(),
        b"one accepted paragraph",
        &mut op,
    )
    .unwrap();
    AcceptedModel::parse(source.snapshot(), ParseConfig::new(Profile::G3), 1, &mut op).unwrap()
}

fn assert_limit(error: DerivedError, kind: DerivedLimit) {
    assert!(
        matches!(error, DerivedError::Limit { kind: actual, .. } if actual == kind),
        "expected {kind:?}, received {error:?}"
    );
}

#[test]
fn resident_control_boundary_is_shared_and_refusal_is_transactional() {
    let metadata = SourceBudget::preview();
    let process = ProcessPolicy::new(&metadata).unwrap();
    let control = ControlPolicy::new(&process, &metadata).unwrap();
    let clone = control.clone();
    let process_base = process.usage();
    let control_base = control.usage();
    let metadata_base = metadata.usage().auxiliary_capacity;
    let residents: Vec<_> = (0..MAX_CONTROL_RESIDENTS)
        .map(|index| {
            if index % 2 == 0 {
                control.reserve_resident().unwrap()
            } else {
                clone.reserve_resident().unwrap()
            }
        })
        .collect();
    assert_eq!(control.usage().residents, 128);
    assert_eq!(process.usage().residents, 128);
    assert_eq!(
        control.usage(),
        Usage {
            residents: 128,
            ..control_base
        }
    );
    assert_eq!(
        process.usage(),
        Usage {
            residents: 128,
            ..process_base
        }
    );
    let process_before = process.usage();
    let control_before = control.usage();
    assert_eq!(
        clone.reserve_resident().unwrap_err(),
        DerivedError::Limit {
            kind: DerivedLimit::ControlResidents,
            requested: 129,
            limit: 128,
        }
    );
    assert_eq!(process.usage(), process_before);
    assert_eq!(control.usage(), control_before);
    assert_eq!(metadata.usage().auxiliary_capacity, metadata_base);
    drop(residents);
    assert_eq!(process.usage(), process_base);
    assert_eq!(control.usage(), control_base);
}

#[test]
fn resident_process_boundary_spans_controls_and_last_drop_reopens_capacity() {
    let metadata = SourceBudget::preview();
    let process = ProcessPolicy::new(&metadata).unwrap();
    let controls: [ControlPolicy; 3] =
        std::array::from_fn(|_| ControlPolicy::new(&process, &metadata).unwrap());
    let baseline = process.usage();
    let mut residents = Vec::new();
    for control in &controls[..2] {
        for _ in 0..MAX_CONTROL_RESIDENTS {
            residents.push(control.reserve_resident().unwrap());
        }
        assert_eq!(control.usage().residents, 128);
    }
    assert_eq!(process.usage().residents, 256);
    let before = process.usage();
    let third_before = controls[2].usage();
    assert_eq!(
        controls[2].reserve_resident().unwrap_err(),
        DerivedError::Limit {
            kind: DerivedLimit::ProcessResidents,
            requested: 257,
            limit: 256,
        }
    );
    assert_eq!(process.usage(), before);
    assert_eq!(controls[2].usage(), third_before);
    drop(residents.pop().unwrap());
    assert_eq!(controls[1].usage().residents, 127);
    assert_eq!(process.usage().residents, 255);
    let replacement = controls[2].reserve_resident().unwrap();
    assert_eq!(controls[2].usage().residents, 1);
    assert_eq!(process.usage().residents, MAX_PROCESS_RESIDENTS);
    drop(replacement);
    drop(residents);
    assert_eq!(process.usage(), baseline);
    for control in &controls {
        assert_eq!(control.usage().residents, 0);
    }
}

#[test]
fn resident_keeps_policy_headers_without_source_funding_or_model_pins() {
    let metadata = SourceBudget::preview();
    let source = SourceBudget::preview();
    let model = model(&source, 1);
    let process = ProcessPolicy::new(&metadata).unwrap();
    let control = ControlPolicy::new(&process, &metadata).unwrap();
    let process_base = process.usage();
    let source_base = source.usage().auxiliary_capacity;
    let first = control.reserve_resident().unwrap();
    assert_eq!(source.usage().auxiliary_capacity, source_base);
    let blocker = source
        .reserve_resource_capacity(source.limits().auxiliary_capacity - source_base)
        .unwrap();
    let second = control.reserve_resident().unwrap();
    assert_eq!(
        process.usage(),
        Usage {
            residents: 2,
            ..process_base
        }
    );
    drop(blocker);
    drop(model);
    assert_eq!(source.usage().auxiliary_capacity, 0);
    drop(control);
    assert_eq!(process.usage().residents, 2);
    assert_eq!(
        metadata.usage().auxiliary_capacity,
        PROCESS_METADATA_BYTES + CONTROL_METADATA_BYTES
    );
    drop(first);
    assert_eq!(process.usage().residents, 1);
    drop(second);
    assert_eq!(process.usage().residents, 0);
    assert_eq!(process.usage().bytes, PROCESS_METADATA_BYTES);
    assert_eq!(metadata.usage().auxiliary_capacity, PROCESS_METADATA_BYTES);
    drop(process);
    assert_eq!(metadata.usage().auxiliary_capacity, 0);
}

#[test]
fn resident_survives_worker_retirement_and_independent_metadata_or_result_drops() {
    let metadata = SourceBudget::preview();
    let source = SourceBudget::preview();
    let model = model(&source, 1);
    let process = ProcessPolicy::new(&metadata).unwrap();
    let control = ControlPolicy::new(&process, &metadata).unwrap();
    let baseline = process.usage();
    let resident = control.reserve_resident().unwrap();
    let queued = control.reserve_metadata(&model, 4096).unwrap();
    let mut job = control.acquire_job(&model).unwrap();
    assert_eq!(control.usage().jobs, 1);
    assert_eq!(control.usage().residents, 1);
    let retained = job.retain_result(8192).unwrap();
    drop(job);
    assert_eq!(control.usage().jobs, 0);
    assert_eq!(control.usage().residents, 1);
    assert_eq!(control.usage().retained_bytes, 8192);
    drop(retained);
    drop(queued);
    drop(model);
    assert_eq!(source.usage().auxiliary_capacity, 0);
    assert_eq!(
        process.usage(),
        Usage {
            residents: 1,
            ..baseline
        }
    );
    drop(resident);
    assert_eq!(process.usage(), baseline);
    assert_eq!(control.usage().residents, 0);
}

#[test]
fn resident_concurrent_claims_cannot_oversubscribe_the_last_control_slot() {
    let metadata = SourceBudget::preview();
    let process = ProcessPolicy::new(&metadata).unwrap();
    let control = ControlPolicy::new(&process, &metadata).unwrap();
    let residents: Vec<_> = (0..MAX_CONTROL_RESIDENTS - 1)
        .map(|_| control.reserve_resident().unwrap())
        .collect();
    let start = Barrier::new(3);
    let results = std::thread::scope(|scope| {
        let first = scope.spawn(|| {
            start.wait();
            control.reserve_resident()
        });
        let second = scope.spawn(|| {
            start.wait();
            control.reserve_resident()
        });
        start.wait();
        [first.join().unwrap(), second.join().unwrap()]
    });
    assert_eq!(results.iter().filter(|result| result.is_ok()).count(), 1);
    assert_eq!(
        results
            .iter()
            .filter_map(|result| result.as_ref().err())
            .collect::<Vec<_>>(),
        [&DerivedError::Limit {
            kind: DerivedLimit::ControlResidents,
            requested: 129,
            limit: 128,
        }]
    );
    assert_eq!(control.usage().residents, 128);
    assert_eq!(process.usage().residents, 128);
    drop(results);
    assert_eq!(control.usage().residents, 127);
    assert_eq!(process.usage().residents, 127);
    drop(residents);
    assert_eq!(control.usage().residents, 0);
    assert_eq!(process.usage().residents, 0);
}

#[test]
fn shared_policy_metadata_is_funded_bounded_and_released_by_last_owner() {
    let metadata = SourceBudget::preview();
    let process = ProcessPolicy::new(&metadata).unwrap();
    assert_eq!(metadata.usage().auxiliary_capacity, PROCESS_METADATA_BYTES);
    assert_eq!(process.usage().bytes, PROCESS_METADATA_BYTES);
    let control = ControlPolicy::new(&process, &metadata).unwrap();
    let clone = control.clone();
    let process_clone = process.clone();
    assert_eq!(control.usage().bytes, CONTROL_METADATA_BYTES);
    assert_eq!(control.usage().metadata_bytes, CONTROL_METADATA_BYTES);
    assert_eq!(
        process.usage().metadata_bytes,
        PROCESS_METADATA_BYTES + CONTROL_METADATA_BYTES
    );
    assert_eq!(
        metadata.usage().auxiliary_capacity,
        PROCESS_METADATA_BYTES + CONTROL_METADATA_BYTES
    );
    drop(control);
    assert_eq!(clone.usage().metadata_bytes, CONTROL_METADATA_BYTES);
    drop(clone);
    assert_eq!(process.usage().metadata_bytes, PROCESS_METADATA_BYTES);
    drop(process);
    assert_eq!(metadata.usage().auxiliary_capacity, PROCESS_METADATA_BYTES);
    drop(process_clone);
    assert_eq!(metadata.usage().auxiliary_capacity, 0);

    let limits = Limits {
        metadata_bytes: PROCESS_METADATA_BYTES + CONTROL_METADATA_BYTES,
        ..Limits::PRODUCTION
    };
    let process = ProcessPolicy::create(&metadata, limits).unwrap();
    let control = ControlPolicy::new(&process, &metadata).unwrap();
    let before = process.usage();
    let source_before = metadata.usage().auxiliary_capacity;
    assert_limit(
        ControlPolicy::new(&process, &metadata).unwrap_err(),
        DerivedLimit::MetadataBytes,
    );
    assert_eq!(process.usage(), before);
    assert_eq!(metadata.usage().auxiliary_capacity, source_before);
    drop(control);
    drop(process);
    assert_eq!(metadata.usage().auxiliary_capacity, 0);
}

#[test]
fn header_source_or_policy_refusal_never_leaves_a_partial_header_charge() {
    let unfunded = SourceBudget::new(SourceLimits {
        auxiliary_capacity: 0,
        ..SourceLimits::default()
    })
    .unwrap();
    assert!(matches!(
        ProcessPolicy::new(&unfunded),
        Err(DerivedError::Source(CoreError::ResourceLimit {
            kind: LimitKind::AuxiliaryCapacity,
            ..
        }))
    ));
    assert_eq!(unfunded.usage().auxiliary_capacity, 0);

    let metadata = SourceBudget::preview();
    let process = ProcessPolicy::new(&metadata).unwrap();
    let before = process.usage();
    assert!(matches!(
        ControlPolicy::new(&process, &unfunded),
        Err(DerivedError::Source(CoreError::ResourceLimit {
            kind: LimitKind::AuxiliaryCapacity,
            ..
        }))
    ));
    assert_eq!(process.usage(), before);
    let process = ProcessPolicy::create(
        &metadata,
        Limits {
            process_bytes: PROCESS_METADATA_BYTES,
            ..Limits::PRODUCTION
        },
    )
    .unwrap();
    let before = process.usage();
    let source_before = metadata.usage().auxiliary_capacity;
    assert_limit(
        ControlPolicy::new(&process, &metadata).unwrap_err(),
        DerivedLimit::ProcessBytes,
    );
    assert_eq!(process.usage(), before);
    assert_eq!(metadata.usage().auxiliary_capacity, source_before);
}

#[test]
fn source_capacity_refusal_and_same_control_worker_refusal_roll_back() {
    let metadata = SourceBudget::preview();
    let source = SourceBudget::preview();
    let model = model(&source, 1);
    let process = ProcessPolicy::new(&metadata).unwrap();
    let control = ControlPolicy::new(&process, &metadata).unwrap();
    let before_process = process.usage();
    let before_control = control.usage();
    let base = source.usage().auxiliary_capacity;
    let blocker = source
        .reserve_resource_capacity(source.limits().auxiliary_capacity - base - JOB_BYTES + 1)
        .unwrap();
    let blocked = source.usage().auxiliary_capacity;
    assert!(matches!(
        control.acquire_job(&model),
        Err(DerivedError::Source(CoreError::ResourceLimit {
            kind: LimitKind::AuxiliaryCapacity,
            ..
        }))
    ));
    assert_eq!(source.usage().auxiliary_capacity, blocked);
    assert_eq!(process.usage(), before_process);
    assert_eq!(control.usage(), before_control);
    drop(blocker);
    let job = control.acquire_job(&model).unwrap();
    assert_eq!(job.reserved_bytes(), JOB_BYTES);
    assert_eq!(source.usage().auxiliary_capacity, base + JOB_BYTES);
    let running_process = process.usage();
    let running_control = control.usage();
    assert_limit(
        control.clone().acquire_job(&model).unwrap_err(),
        DerivedLimit::ControlJobs,
    );
    assert_eq!(source.usage().auxiliary_capacity, base + JOB_BYTES);
    assert_eq!(process.usage(), running_process);
    assert_eq!(control.usage(), running_control);
    drop(job);
    assert_eq!(source.usage().auxiliary_capacity, base);
    assert_eq!(process.usage(), before_process);
    assert_eq!(control.usage(), before_control);
}

#[test]
fn two_actual_concurrent_workers_share_process_and_third_is_rejected() {
    let metadata = SourceBudget::preview();
    let source = SourceBudget::preview();
    let model = model(&source, 1);
    let process = ProcessPolicy::new(&metadata).unwrap();
    let first = ControlPolicy::new(&process, &metadata).unwrap();
    let second = ControlPolicy::new(&process.clone(), &metadata).unwrap();
    let third = ControlPolicy::new(&process, &metadata).unwrap();
    let baseline = process.usage();
    let source_base = source.usage().auxiliary_capacity;
    let entered = Barrier::new(3);
    let leave = Barrier::new(3);
    std::thread::scope(|scope| {
        for control in [&first, &second] {
            let model = &model;
            let entered = &entered;
            let leave = &leave;
            scope.spawn(move || {
                let job = control.acquire_job(model).unwrap();
                entered.wait();
                leave.wait();
                drop(job);
            });
        }
        entered.wait();
        let before = process.usage();
        let first_jobs = first.usage().jobs;
        let second_jobs = second.usage().jobs;
        let source_before = source.usage().auxiliary_capacity;
        let rejected = third.acquire_job(&model);
        let after = process.usage();
        let third_jobs = third.usage().jobs;
        let source_after = source.usage().auxiliary_capacity;
        // Unblock both real workers before assertions: a failing assertion
        // must not leave thread::scope waiting for a barrier forever.
        leave.wait();
        assert_eq!(before.jobs, 2);
        assert_eq!(first_jobs, 1);
        assert_eq!(second_jobs, 1);
        assert_eq!(source_before, source_base + 2 * JOB_BYTES);
        assert_limit(rejected.unwrap_err(), DerivedLimit::ProcessJobs);
        assert_eq!(after, before);
        assert_eq!(third_jobs, 0);
        assert_eq!(source_after, source_base + 2 * JOB_BYTES);
    });
    assert_eq!(source.usage().auxiliary_capacity, source_base);
    assert_eq!(process.usage(), baseline);
}

#[test]
fn byte_policy_refusals_restore_source_and_all_preexisting_charges() {
    for expected in [DerivedLimit::ControlBytes, DerivedLimit::ProcessBytes] {
        let metadata = SourceBudget::preview();
        let source = SourceBudget::preview();
        let model = model(&source, 1);
        let limits = if expected == DerivedLimit::ControlBytes {
            Limits {
                control_bytes: CONTROL_METADATA_BYTES + JOB_BYTES,
                ..Limits::PRODUCTION
            }
        } else {
            Limits {
                process_bytes: PROCESS_METADATA_BYTES + 2 * CONTROL_METADATA_BYTES + JOB_BYTES,
                ..Limits::PRODUCTION
            }
        };
        let process = ProcessPolicy::create(&metadata, limits).unwrap();
        let control = ControlPolicy::new(&process, &metadata).unwrap();
        let other = ControlPolicy::new(&process, &metadata).unwrap();
        let mut job = control.acquire_job(&model).unwrap();
        let retained = job.retain_result(4096).unwrap();
        drop(job);
        let process_before = process.usage();
        let control_before = control.usage();
        let other_before = other.usage();
        let source_before = source.usage().auxiliary_capacity;
        let target = if expected == DerivedLimit::ControlBytes {
            &control
        } else {
            &other
        };
        assert_limit(target.acquire_job(&model).unwrap_err(), expected);
        assert_eq!(process.usage(), process_before);
        assert_eq!(control.usage(), control_before);
        assert_eq!(other.usage(), other_before);
        assert_eq!(source.usage().auxiliary_capacity, source_before);
        drop(retained);
        assert_eq!(process.usage().retained_bytes, 0);
    }
}

#[test]
fn invalid_or_repeated_split_preserves_reservation_and_actual_worker_scope() {
    let metadata = SourceBudget::preview();
    let source = SourceBudget::preview();
    let model = model(&source, 1);
    let process = ProcessPolicy::new(&metadata).unwrap();
    let control = ControlPolicy::new(&process, &metadata).unwrap();
    let base = source.usage().auxiliary_capacity;
    let mut job = control.acquire_job(&model).unwrap();
    let before_process = process.usage();
    let before_control = control.usage();
    for invalid in [0, MAX_RETAINED_BYTES + 1, JOB_BYTES + 1, u64::MAX] {
        assert_eq!(
            job.retain_result(invalid).unwrap_err(),
            DerivedError::InvalidSize
        );
        assert_eq!(job.reserved_bytes(), JOB_BYTES);
        assert_eq!(source.usage().auxiliary_capacity, base + JOB_BYTES);
        assert_eq!(process.usage(), before_process);
        assert_eq!(control.usage(), before_control);
    }
    let result = job.retain_result(8192).unwrap();
    assert_eq!(job.reserved_bytes(), 0);
    assert_eq!(result.reserved_bytes(), 8192);
    assert_eq!(source.usage().auxiliary_capacity, base + 8192);
    assert_eq!(control.usage().jobs, 1);
    assert_eq!(process.usage().jobs, 1);
    let before_process = process.usage();
    let before_control = control.usage();
    assert_eq!(
        job.retain_result(1).unwrap_err(),
        DerivedError::InvalidState
    );
    assert_eq!(process.usage(), before_process);
    assert_eq!(control.usage(), before_control);
    assert_eq!(source.usage().auxiliary_capacity, base + 8192);
    assert_limit(
        control.acquire_job(&model).unwrap_err(),
        DerivedLimit::ControlJobs,
    );
    drop(result);
    assert_eq!(source.usage().auxiliary_capacity, base);
    assert_eq!(control.usage().jobs, 1);
    assert_eq!(process.usage().jobs, 1);
    drop(job);
    assert_eq!(control.usage().jobs, 0);
    assert_eq!(process.usage().jobs, 0);
}

#[test]
fn cumulative_retained_limit_spans_revisions_and_failed_split_keeps_full_job() {
    let metadata = SourceBudget::preview();
    let source = SourceBudget::preview();
    let old_model = model(&source, 1);
    let process = ProcessPolicy::new(&metadata).unwrap();
    let control = ControlPolicy::new(&process, &metadata).unwrap();
    let mut first = control.acquire_job(&old_model).unwrap();
    let old_bytes = MAX_RETAINED_BYTES / 2 + 1;
    let old_result = first.retain_result(old_bytes).unwrap();
    drop(first);
    drop(old_model);
    // A grant retains the actual source quota, without pinning the model.
    assert_eq!(source.usage().auxiliary_capacity, old_bytes);
    let new_model = model(&source, 2);
    let model_base = source.usage().auxiliary_capacity - old_bytes;
    let mut next = control.clone().acquire_job(&new_model).unwrap();
    let before_process = process.usage();
    let before_control = control.usage();
    assert_limit(
        next.retain_result(MAX_RETAINED_BYTES / 2).unwrap_err(),
        DerivedLimit::RetainedBytes,
    );
    assert_eq!(next.reserved_bytes(), JOB_BYTES);
    assert_eq!(
        source.usage().auxiliary_capacity,
        model_base + old_bytes + JOB_BYTES
    );
    assert_eq!(process.usage(), before_process);
    assert_eq!(control.usage(), before_control);
    drop(old_result);
    let result = next.retain_result(MAX_RETAINED_BYTES / 2).unwrap();
    assert_eq!(control.usage().jobs, 1);
    assert_eq!(
        source.usage().auxiliary_capacity,
        model_base + MAX_RETAINED_BYTES / 2
    );
    drop(next);
    assert_eq!(control.usage().jobs, 0);
    drop(new_model);
    assert_eq!(source.usage().auxiliary_capacity, MAX_RETAINED_BYTES / 2);
    drop(result);
    assert_eq!(source.usage().auxiliary_capacity, 0);
    assert_eq!(control.usage().retained_bytes, 0);
    assert_eq!(process.usage().retained_bytes, 0);
}

#[test]
fn retired_facades_do_not_clear_a_shared_payload_or_keep_worker_occupied() {
    struct Payload {
        // Rust field order frees the actual bytes before the capacity lease.
        bytes: Vec<u8>,
        _grant: RetainedGrant,
    }
    let metadata = SourceBudget::preview();
    let source = SourceBudget::preview();
    let model = model(&source, 1);
    let process = ProcessPolicy::new(&metadata).unwrap();
    let control = ControlPolicy::new(&process, &metadata).unwrap();
    let mut job = control.acquire_job(&model).unwrap();
    let bytes = vec![19; 4096];
    let actual = bytes.capacity() as u64
        + size_of::<Payload>() as u64
        + (2 * size_of::<usize>() + align_of::<Payload>()) as u64;
    let payload = Arc::new(Payload {
        bytes,
        _grant: job.retain_result(actual).unwrap(),
    });
    drop(job);
    drop(model);
    drop(control);
    assert_eq!(source.usage().auxiliary_capacity, actual);
    assert_eq!(process.usage().jobs, 0);
    assert_eq!(process.usage().retained_bytes, actual);
    assert_eq!(
        metadata.usage().auxiliary_capacity,
        PROCESS_METADATA_BYTES + CONTROL_METADATA_BYTES
    );
    let other_owner = payload.clone();
    assert_eq!(payload.bytes[0], 19);
    drop(payload);
    assert_eq!(source.usage().auxiliary_capacity, actual);
    assert_eq!(process.usage().retained_bytes, actual);
    drop(other_owner);
    assert_eq!(source.usage().auxiliary_capacity, 0);
    assert_eq!(process.usage().retained_bytes, 0);
    assert_eq!(process.usage().bytes, PROCESS_METADATA_BYTES);
    assert_eq!(metadata.usage().auxiliary_capacity, PROCESS_METADATA_BYTES);
    drop(process);
    assert_eq!(metadata.usage().auxiliary_capacity, 0);
}

#[test]
fn metadata_rejects_invalid_sizes_and_source_exhaustion_without_policy_changes() {
    let metadata = SourceBudget::preview();
    let source = SourceBudget::preview();
    let model = model(&source, 1);
    let process = ProcessPolicy::new(&metadata).unwrap();
    let control = ControlPolicy::new(&process, &metadata).unwrap();
    let process_before = process.usage();
    let control_before = control.usage();
    let source_before = source.usage().auxiliary_capacity;
    for bytes in [0, MAX_METADATA_BYTES + 1, u64::MAX] {
        assert_eq!(
            control.reserve_metadata(&model, bytes).unwrap_err(),
            DerivedError::InvalidSize
        );
        assert_eq!(source.usage().auxiliary_capacity, source_before);
        assert_eq!(process.usage(), process_before);
        assert_eq!(control.usage(), control_before);
    }
    let blocker = source
        .reserve_resource_capacity(source.limits().auxiliary_capacity - source_before - 63)
        .unwrap();
    let blocked = source.usage().auxiliary_capacity;
    assert!(matches!(
        control.reserve_metadata(&model, 64),
        Err(DerivedError::Source(CoreError::ResourceLimit {
            kind: LimitKind::AuxiliaryCapacity,
            ..
        }))
    ));
    assert_eq!(source.usage().auxiliary_capacity, blocked);
    assert_eq!(process.usage(), process_before);
    assert_eq!(control.usage(), control_before);
    drop(blocker);
    assert_eq!(source.usage().auxiliary_capacity, source_before);
}

#[test]
fn metadata_global_boundary_includes_headers_and_is_independent_of_worker_family() {
    let metadata = SourceBudget::new(SourceLimits {
        auxiliary_capacity: MAX_METADATA_BYTES,
        ..SourceLimits::default()
    })
    .unwrap();
    let first_source = SourceBudget::preview();
    let second_source = SourceBudget::preview();
    let first_model = model(&first_source, 1);
    let second_model = model(&second_source, 2);
    let process = ProcessPolicy::new(&metadata).unwrap();
    let first = ControlPolicy::new(&process, &metadata).unwrap();
    let second = ControlPolicy::new(&process, &metadata).unwrap();
    let headers = PROCESS_METADATA_BYTES + 2 * CONTROL_METADATA_BYTES;
    let first_base = first_source.usage().auxiliary_capacity;
    let second_base = second_source.usage().auxiliary_capacity;
    let first_bytes = MAX_METADATA_BYTES - headers - 1;
    let first_record = first.reserve_metadata(&first_model, first_bytes).unwrap();
    assert_eq!(first_record.reserved_bytes(), first_bytes);
    assert_eq!(process.usage().metadata_bytes, MAX_METADATA_BYTES - 1);
    assert_eq!(process.usage().jobs, 0);
    assert_eq!(process.usage().retained_bytes, 0);
    let mut job = second.acquire_job(&second_model).unwrap();
    // Metadata is admissible while this same control has its actual worker.
    let second_record = second.reserve_metadata(&second_model, 1).unwrap();
    assert_eq!(process.usage().metadata_bytes, MAX_METADATA_BYTES);
    assert_eq!(process.usage().bytes, MAX_METADATA_BYTES + JOB_BYTES);
    assert_eq!(process.usage().jobs, 1);
    assert_eq!(first.usage().jobs, 0);
    assert_eq!(second.usage().jobs, 1);
    assert_eq!(
        first.usage().metadata_bytes,
        CONTROL_METADATA_BYTES + first_bytes
    );
    assert_eq!(second.usage().metadata_bytes, CONTROL_METADATA_BYTES + 1);
    let process_before = process.usage();
    let control_before = second.usage();
    let second_before = second_source.usage().auxiliary_capacity;
    assert_limit(
        second.reserve_metadata(&second_model, 1).unwrap_err(),
        DerivedLimit::MetadataBytes,
    );
    assert_eq!(process.usage(), process_before);
    assert_eq!(second.usage(), control_before);
    assert_eq!(second_source.usage().auxiliary_capacity, second_before);
    assert_limit(
        ControlPolicy::new(&process, &metadata).unwrap_err(),
        DerivedLimit::MetadataBytes,
    );
    assert_eq!(metadata.usage().auxiliary_capacity, headers);
    assert_eq!(process.usage(), process_before);

    // The worker's own Metadata family is already inside its 64 MiB. Neither
    // that job nor its retained payload acquires this persistent metadata cap.
    let retained = job.retain_result(MAX_RETAINED_BYTES).unwrap();
    assert_eq!(process.usage().metadata_bytes, MAX_METADATA_BYTES);
    assert_eq!(process.usage().retained_bytes, MAX_RETAINED_BYTES);
    assert_eq!(process.usage().jobs, 1);
    drop(job);
    assert_eq!(process.usage().jobs, 0);
    assert_eq!(
        first_source.usage().auxiliary_capacity,
        first_base + first_bytes
    );
    assert_eq!(
        second_source.usage().auxiliary_capacity,
        second_base + 1 + MAX_RETAINED_BYTES
    );
    drop(retained);
    drop(second_record);
    drop(first_record);
    assert_eq!(first_source.usage().auxiliary_capacity, first_base);
    assert_eq!(second_source.usage().auxiliary_capacity, second_base);
    assert_eq!(process.usage().bytes, headers);
    assert_eq!(process.usage().metadata_bytes, headers);
    assert_eq!(process.usage().retained_bytes, 0);
}

#[test]
fn metadata_policy_refusal_with_a_live_job_rolls_back_the_real_source_reservation() {
    for expected in [DerivedLimit::ProcessBytes, DerivedLimit::ControlBytes] {
        let metadata = SourceBudget::preview();
        let source = SourceBudget::preview();
        let model = model(&source, 1);
        let limits = if expected == DerivedLimit::ProcessBytes {
            Limits {
                process_bytes: PROCESS_METADATA_BYTES + CONTROL_METADATA_BYTES + JOB_BYTES + 4095,
                ..Limits::PRODUCTION
            }
        } else {
            Limits {
                control_bytes: CONTROL_METADATA_BYTES + JOB_BYTES + 4095,
                ..Limits::PRODUCTION
            }
        };
        let process = ProcessPolicy::create(&metadata, limits).unwrap();
        let control = ControlPolicy::new(&process, &metadata).unwrap();
        let job = control.acquire_job(&model).unwrap();
        let process_before = process.usage();
        let control_before = control.usage();
        let source_before = source.usage().auxiliary_capacity;
        assert_limit(
            control.reserve_metadata(&model, 4096).unwrap_err(),
            expected,
        );
        assert_eq!(process.usage(), process_before);
        assert_eq!(control.usage(), control_before);
        assert_eq!(source.usage().auxiliary_capacity, source_before);
        assert_eq!(job.reserved_bytes(), JOB_BYTES);
        drop(job);
        assert_eq!(process.usage().jobs, 0);
    }
}

#[test]
fn metadata_keeps_the_captured_revision_root_and_survives_facade_retirement() {
    let metadata = SourceBudget::preview();
    let old_source = SourceBudget::preview();
    let new_source = SourceBudget::preview();
    let old_model = model(&old_source, 1);
    let new_model = model(&new_source, 2);
    let process = ProcessPolicy::new(&metadata).unwrap();
    let control = ControlPolicy::new(&process, &metadata).unwrap();
    let old_record = control.reserve_metadata(&old_model, 4096).unwrap();
    drop(old_model);
    assert_eq!(old_source.usage().auxiliary_capacity, 4096);
    let new_record = control.reserve_metadata(&new_model, 8192).unwrap();
    drop(new_model);
    assert_eq!(new_source.usage().auxiliary_capacity, 8192);
    drop(control);
    assert_eq!(old_source.usage().auxiliary_capacity, 4096);
    assert_eq!(new_source.usage().auxiliary_capacity, 8192);
    assert_eq!(
        process.usage().metadata_bytes,
        PROCESS_METADATA_BYTES + CONTROL_METADATA_BYTES + 4096 + 8192
    );
    assert_eq!(process.usage().jobs, 0);
    drop(new_record);
    assert_eq!(old_source.usage().auxiliary_capacity, 4096);
    assert_eq!(new_source.usage().auxiliary_capacity, 0);
    drop(old_record);
    assert_eq!(old_source.usage().auxiliary_capacity, 0);
    assert_eq!(process.usage().bytes, PROCESS_METADATA_BYTES);
    assert_eq!(metadata.usage().auxiliary_capacity, PROCESS_METADATA_BYTES);
    drop(process);
    assert_eq!(metadata.usage().auxiliary_capacity, 0);
}

#[test]
fn shared_metadata_record_frees_actual_allocation_before_its_last_lease() {
    use std::sync::atomic::{AtomicBool, Ordering};

    struct Allocation {
        bytes: Option<Vec<u8>>,
        source: SourceBudget,
        expected_charge: u64,
        freed: Arc<AtomicBool>,
    }
    impl Drop for Allocation {
        fn drop(&mut self) {
            drop(self.bytes.take());
            // The real Vec has just freed; the matching lease is still live.
            assert_eq!(self.source.usage().auxiliary_capacity, self.expected_charge);
            self.freed.store(true, Ordering::Release);
        }
    }
    struct Record {
        allocation: Allocation,
    }
    let metadata = SourceBudget::preview();
    let source = SourceBudget::preview();
    let model = model(&source, 1);
    let process = ProcessPolicy::new(&metadata).unwrap();
    let control = ControlPolicy::new(&process, &metadata).unwrap();
    let prepaid =
        8192 + (size_of::<Record>() + 2 * size_of::<usize>() + align_of::<Record>()) as u64;
    let grant = control.reserve_metadata(&model, prepaid).unwrap();
    // Funding precedes both the real array allocation and its shared owner.
    let bytes = vec![47; 4096];
    assert!(bytes.capacity() as u64 <= 8192);
    let freed = Arc::new(AtomicBool::new(false));
    let record = Arc::new(Record {
        allocation: Allocation {
            bytes: Some(bytes),
            source: source.clone(),
            expected_charge: prepaid,
            freed: freed.clone(),
        },
    });
    drop(model);
    drop(control);
    let last = record.clone();
    assert_eq!(record.allocation.bytes.as_ref().unwrap()[0], 47);
    drop(record);
    assert!(!freed.load(Ordering::Acquire));
    assert_eq!(source.usage().auxiliary_capacity, prepaid);
    assert_eq!(process.usage().jobs, 0);
    drop(last);
    assert!(freed.load(Ordering::Acquire));
    // Arc::drop has now also deallocated the actual Record/Arc header. Keep
    // its grant outside that allocation, so header reclamation precedes the
    // source/policy release as well as the Vec reclamation observed above.
    assert_eq!(source.usage().auxiliary_capacity, prepaid);
    drop(grant);
    assert_eq!(source.usage().auxiliary_capacity, 0);
    assert_eq!(process.usage().bytes, PROCESS_METADATA_BYTES);
    assert_eq!(process.usage().metadata_bytes, PROCESS_METADATA_BYTES);
}
