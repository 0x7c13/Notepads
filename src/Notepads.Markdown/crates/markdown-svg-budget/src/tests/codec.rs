// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE file in the project root for license information.
// ---------------------------------------------------------------------------------------------
use super::*;
use crate::codec::{Buffer, Control, One};
use core::cell::Cell;

#[test]
fn codec_copy_storage_never_calls_a_user_clone_or_hides_an_allocation() {
    let _isolation = isolate_observer();
    #[derive(Debug, PartialEq, Eq)]
    struct CopyOnly(u8);
    impl Copy for CopyOnly {}
    impl Clone for CopyOnly {
        fn clone(&self) -> Self {
            panic!("Copy path called Clone")
        }
    }
    let (resources, _, state) = setup(Limits::default());
    let control = Control::paid(resources.clone());
    let mut values = Buffer::new(&control);
    start_alloc_observation();
    values.try_reserve_exact(2048).unwrap();
    values.try_resize(1024, CopyOnly(9)).unwrap();
    values.try_extend_from_slice(&[CopyOnly(7); 16]).unwrap();
    let other = values.try_clone().unwrap();
    assert_eq!(&*values, &*other);
    assert_ne!(values.as_ptr(), other.as_ptr());
    drop((values, other));
    ENABLED.store(false, Ordering::Relaxed);
    assert_eq!(ALLOCS.load(Ordering::Relaxed), 2);
    assert_eq!(FREES.load(Ordering::Relaxed), 2);
    assert_eq!(PEAK.load(Ordering::Relaxed), 2048 + 1040);
    assert_eq!(LIVE.load(Ordering::Relaxed), 0);
    assert_eq!(state.live.load(Ordering::Relaxed), 0);
    drop(control);
    resources.validate_retired().unwrap();
}

#[test]
fn codec_clear_retains_the_fee_and_release_storage_keeps_paid_identity() {
    let _isolation = isolate_observer();
    let (resources, _, state) = setup(Limits::default());
    let control = Control::paid(resources.clone());
    let mut values = Buffer::<u8>::new(&control);
    begin_allocation_trace(16_389);
    values.try_resize(16_389, 17).unwrap();
    values.clear();
    assert_eq!(TRACE_FREED.load(Ordering::Relaxed), 0);
    assert_eq!(state.live.load(Ordering::Relaxed), 16_389);
    values.release_storage();
    finish_allocation_trace();
    assert!(values.as_plain_vec().is_none());
    values.try_push(33).unwrap();
    assert!(values.try_into_plain().is_err());
    assert_eq!(state.live.load(Ordering::Relaxed), 0);
    drop(control);
    resources.validate_retired().unwrap();
}

#[test]
fn codec_paid_conversion_refusal_does_not_allocate_refund_or_poison() {
    let _isolation = isolate_observer();
    let (resources, _, state) = setup(Limits::default());
    let control = Control::paid(resources.clone());
    let mut values = Buffer::new(&control);
    values.try_resize(71, 8u8).unwrap();
    let ptr = values.as_ptr();
    let paid = state.live.load(Ordering::Relaxed);
    let releases = state.releases.load(Ordering::Relaxed);
    start_alloc_observation();
    let values = values.try_into_plain().unwrap_err();
    ENABLED.store(false, Ordering::Relaxed);
    assert_eq!(ALLOCS.load(Ordering::Relaxed), 0);
    assert_eq!(FREES.load(Ordering::Relaxed), 0);
    assert_eq!(values.as_ptr(), ptr);
    assert_eq!(state.live.load(Ordering::Relaxed), paid);
    assert_eq!(state.releases.load(Ordering::Relaxed), releases);
    assert_eq!(control.failure(), None);
    drop(values);
    drop(control);
    resources.validate_retired().unwrap();
    let mut plain = Buffer::new(&Control::plain());
    plain.try_extend_from_slice(&[7u8, 8, 9]).unwrap();
    let original = plain.as_plain_vec().unwrap().as_ptr();
    let plain = plain.try_into_plain().unwrap();
    assert_eq!(plain.as_ptr(), original);
    assert_eq!(plain, [7, 8, 9]);
}

#[test]
fn codec_growth_refusals_preserve_the_original_storage_and_initialized_values() {
    let _isolation = isolate_observer();
    for stage in 0..3 {
        let mut limits = Limits::default();
        if stage == 0 {
            limits.scratch_bytes = 16;
        }
        let (resources, _, state) = setup(limits);
        let control = Control::paid(resources.clone());
        let mut values = Buffer::new(&control);
        values.try_extend_from_slice(&[1u8, 2, 3, 4]).unwrap();
        let original = values.as_ptr();
        let capacity = values.capacity();
        if stage == 1 {
            FAIL_NEXT_GROWTH.store(true, Ordering::Relaxed);
        }
        if stage == 2 {
            state.fail_on_admission.store(
                state.admissions.load(Ordering::Relaxed) + 1,
                Ordering::Relaxed,
            );
            begin_allocation_trace(16_389);
        }
        let error = values.try_resize(16_389, 0).unwrap_err();
        if stage == 2 {
            finish_allocation_trace();
        }
        assert_eq!(
            error.kind,
            [
                FailureKind::Quota,
                FailureKind::Allocation,
                FailureKind::Native
            ][stage]
        );
        assert_eq!(&*values, [1, 2, 3, 4]);
        assert_eq!(values.as_ptr(), original);
        assert_eq!(values.capacity(), capacity);
        assert_eq!(state.live.load(Ordering::Relaxed), capacity as u64);
        drop(values);
        drop(control);
        resources.validate_retired().unwrap();
    }
}

#[test]
fn codec_resize_cancels_between_real_batches_and_rolls_back_new_elements() {
    let _isolation = isolate_observer();
    let (resources, _, state) = setup(Limits::default());
    let control = Control::paid(resources.clone());
    let mut values = Buffer::<u8>::new(&control);
    values.try_reserve_exact(4096).unwrap();
    let made = Cell::new(0);
    let error = values
        .try_resize_with(4096, |_| {
            made.set(made.get() + 1);
            if made.get() == 1024 {
                state.fail_on_checkpoint.store(
                    state.checkpoint_calls.load(Ordering::Relaxed) + 1,
                    Ordering::Relaxed,
                );
            }
            Ok(3)
        })
        .unwrap_err();
    assert_eq!(made.get(), 1024);
    assert_eq!(error.kind, FailureKind::Cancelled);
    assert!(values.is_empty());
    assert_eq!(values.capacity(), 4096);
    assert_eq!(state.live.load(Ordering::Relaxed), 4096);
    drop(values);
    drop(control);
    resources.validate_retired().unwrap();
}

#[test]
fn codec_one_admits_its_shell_before_factory_and_frees_nested_storage_first() {
    let _isolation = isolate_observer();
    struct Nested<'a> {
        bytes: Buffer<u8>,
        state: &'a State,
    }
    impl Drop for Nested<'_> {
        fn drop(&mut self) {
            assert_eq!(self.bytes.len(), 16_389);
            assert_eq!(
                self.state.live.load(Ordering::Relaxed),
                (size_of::<Self>() + 16_389) as u64
            );
        }
    }
    let (resources, _, state) = setup(Limits::default());
    let control = Control::paid(resources.clone());
    begin_allocation_trace(16_389);
    let value = One::try_new(&control, |control| {
        assert_eq!(state.admissions.load(Ordering::Relaxed), 1);
        assert_eq!(
            state.live.load(Ordering::Relaxed),
            size_of::<Nested<'_>>() as u64
        );
        let mut bytes = Buffer::new(control);
        bytes.try_resize(16_389, 3)?;
        Ok(Nested {
            bytes,
            state: &state,
        })
    })
    .unwrap();
    WATCH_PTR.store((&*value as *const Nested<'_>) as usize, Ordering::Relaxed);
    WATCH_FREED.store(false, Ordering::Relaxed);
    state.watched_ticket.store(1, Ordering::Relaxed);
    drop(value);
    finish_allocation_trace();
    assert!(state.physical_free_before_release.load(Ordering::Acquire));
    assert_eq!(state.live.load(Ordering::Relaxed), 0);
    assert_eq!(state.reserves.load(Ordering::Relaxed), 2);
    assert_eq!(state.releases.load(Ordering::Relaxed), 2);
    drop(control);
    resources.validate_retired().unwrap();
}

#[test]
fn codec_one_factory_error_and_unwind_keep_partial_owners_paid_until_free() {
    let _isolation = isolate_observer();
    for unwind in [false, true] {
        let (resources, _, state) = setup(Limits::default());
        let control = Control::paid(resources.clone());
        begin_allocation_trace(16_389);
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            One::<[u64; 17]>::try_new(&control, |control| {
                assert_eq!(state.admissions.load(Ordering::Relaxed), 1);
                let mut partial = Buffer::new(control);
                partial.try_resize(16_389, 1u8)?;
                let error = control.terminal(Error::native(-371));
                if unwind {
                    panic!("intentional codec factory unwind");
                }
                Err(error)
            })
        }));
        assert_eq!(result.is_err(), unwind);
        if !unwind {
            assert_eq!(result.unwrap().unwrap_err(), Error::native(-371));
        }
        assert_eq!(control.failure(), Some(Error::native(-371)));
        finish_allocation_trace();
        assert_eq!(state.live.load(Ordering::Relaxed), 0);
        assert_eq!(state.reserves.load(Ordering::Relaxed), 2);
        assert_eq!(state.releases.load(Ordering::Relaxed), 2);
        drop(control);
        resources.validate_retired().unwrap();
    }
}

#[test]
fn codec_one_refuses_before_factory_and_zero_sized_values_still_poll() {
    let _isolation = isolate_observer();
    for stage in 0..4 {
        let mut limits = Limits::default();
        if stage == 0 {
            limits.scratch_bytes = 1;
        }
        let (resources, _, state) = setup(limits);
        let control = Control::paid(resources.clone());
        if stage == 1 {
            FAIL_NEXT_GROWTH.store(true, Ordering::Relaxed);
        }
        if stage == 2 {
            state.fail_on_admission.store(1, Ordering::Relaxed);
        }
        if stage == 3 {
            state.fail_on_checkpoint.store(1, Ordering::Relaxed);
        }
        let called = Cell::new(false);
        let result = if stage == 3 {
            One::try_new(&control, |_| {
                called.set(true);
                Ok(())
            })
            .map(|_| ())
        } else {
            One::try_new(&control, |_| {
                called.set(true);
                Ok([0u8; 73])
            })
            .map(|_| ())
        };
        assert!(result.is_err());
        assert!(!called.get());
        assert_eq!(state.live.load(Ordering::Relaxed), 0);
        drop(control);
        resources.validate_retired().unwrap();
    }
    let (resources, _, state) = setup(Limits::default());
    let control = Control::paid(resources.clone());
    drop(One::try_new(&control, |_| Ok(())).unwrap());
    assert_eq!(state.reserves.load(Ordering::Relaxed), 0);
    assert_eq!(state.admissions.load(Ordering::Relaxed), 0);
    assert!(state.checkpoint_calls.load(Ordering::Relaxed) >= 2);
    drop(control);
    resources.validate_retired().unwrap();
}

#[test]
fn codec_terminal_semantic_failure_precedes_a_real_free_and_failed_refund() {
    let _isolation = isolate_observer();
    let (resources, _, state) = setup(Limits::default());
    let control = Control::paid(resources.clone());
    let mut bytes = Buffer::new(&control);
    bytes.try_resize(101, 3u8).unwrap();
    WATCH_PTR.store(bytes.as_ptr() as usize, Ordering::Relaxed);
    WATCH_FREED.store(false, Ordering::Relaxed);
    let semantic = control.terminal(Error::new(FailureKind::ImageDecode, LimitKind::None));
    state.failed_release.store(true, Ordering::Relaxed);
    drop(bytes);
    assert!(WATCH_FREED.load(Ordering::Acquire));
    assert_eq!(state.releases.load(Ordering::Relaxed), 0);
    assert_eq!(state.live.load(Ordering::Relaxed), 101);
    assert_eq!(resources.statistics().release_failures, 1);
    assert_eq!(control.failure(), Some(semantic));
    assert_eq!(resources.validate_retired(), Err(semantic));
}

#[test]
fn codec_non_copy_deep_clone_failure_frees_partial_copies_and_preserves_originals() {
    let _isolation = isolate_observer();
    let (resources, _, state) = setup(Limits::default());
    let control = Control::paid(resources.clone());
    let mut original = Buffer::new(&control);
    original.try_reserve_exact(3).unwrap();
    for size in [17, 23, 29] {
        let mut bytes = Buffer::new(&control);
        bytes.try_resize(size, size as u8).unwrap();
        original.try_push(bytes).unwrap();
    }
    let pointers = [
        original[0].as_ptr(),
        original[1].as_ptr(),
        original[2].as_ptr(),
    ];
    let paid = state.live.load(Ordering::Relaxed);
    state.fail_on_admission.store(
        state.admissions.load(Ordering::Relaxed) + 3,
        Ordering::Relaxed,
    );
    let copied = Cell::new(0);
    let error = original
        .try_clone_with(|bytes, _| {
            let copy = bytes.try_clone()?;
            copied.set(copied.get() + 1);
            if copied.get() == 1 {
                WATCH_PTR.store(copy.as_ptr() as usize, Ordering::Relaxed);
                WATCH_FREED.store(false, Ordering::Relaxed);
                state
                    .watched_ticket
                    .store(state.next.load(Ordering::Relaxed), Ordering::Relaxed);
            }
            Ok(copy)
        })
        .unwrap_err();
    assert_eq!(error, Error::native(-88));
    assert_eq!(copied.get(), 1);
    assert!(state.physical_free_before_release.load(Ordering::Acquire));
    assert_eq!(state.live.load(Ordering::Relaxed), paid);
    for index in 0..3 {
        assert_eq!(original[index].as_ptr(), pointers[index]);
        assert!(
            original[index]
                .iter()
                .all(|byte| *byte == original[index].len() as u8)
        );
    }
    drop(original);
    drop(control);
    resources.validate_retired().unwrap();
}

#[test]
fn codec_non_copy_resize_rolls_back_suffix_after_a_factory_records_its_error() {
    let _isolation = isolate_observer();
    let (resources, _, state) = setup(Limits::default());
    let control = Control::paid(resources.clone());
    let mut values = Buffer::new(&control);
    values.try_reserve_exact(4).unwrap();
    let mut first = Buffer::new(&control);
    first.try_resize(7, 99u8).unwrap();
    let ptr = first.as_ptr();
    values.try_push(first).unwrap();
    let paid = state.live.load(Ordering::Relaxed);
    let made = Cell::new(0);
    let error = values
        .try_resize_with(4, |control| {
            let mut bytes = Buffer::new(control);
            bytes.try_resize(11, 3u8)?;
            made.set(made.get() + 1);
            if made.get() == 2 {
                WATCH_PTR.store(bytes.as_ptr() as usize, Ordering::Relaxed);
                WATCH_FREED.store(false, Ordering::Relaxed);
                state
                    .watched_ticket
                    .store(state.next.load(Ordering::Relaxed), Ordering::Relaxed);
                // Record before this local owner frees, then let the outer resize
                // roll back its already constructed prefix using the same fault.
                return Err(control.terminal(Error::native(-434)));
            }
            Ok(bytes)
        })
        .unwrap_err();
    assert_eq!(error, Error::native(-434));
    assert_eq!(made.get(), 2);
    assert_eq!(values.len(), 1);
    assert_eq!(values[0].as_ptr(), ptr);
    assert_eq!(&*values[0], [99; 7]);
    assert!(state.physical_free_before_release.load(Ordering::Acquire));
    assert_eq!(state.live.load(Ordering::Relaxed), paid);
    drop(values);
    drop(control);
    resources.validate_retired().unwrap();
}
