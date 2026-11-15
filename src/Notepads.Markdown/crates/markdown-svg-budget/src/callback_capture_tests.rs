// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE file in the project root for license information.
// ---------------------------------------------------------------------------------------------
//! Captured-owner regressions: before two real red tests, after borrowed sort
//! comparison and explicit clone-callback retirement. Expectations unchanged.
use super::*;
use crate::slice::stable_sort_by_with_control;

#[test]
fn by_value_sort_capture_refund_must_preserve_comparator_error() {
    let _isolation = isolate_observer();
    let (resources, _, state) = setup(Limits::default());
    let mut held = BudgetVec::new(resources.clone(), ResourceKind::Scratch);
    held.try_resize_copy(17, 9u8).unwrap();
    WATCH_PTR.store(held.as_ptr() as usize, Ordering::Relaxed);
    WATCH_FREED.store(false, Ordering::Relaxed);
    let state_in_callback = state.clone();
    let original = Error::native(-902);
    let raw_error = core::cell::Cell::new(None::<Error>);
    let slot_before_record = core::cell::Cell::new(None::<Error>);
    let mut values = [2u32, 1];
    let mut scratch = [0u32; 2];
    let result = stable_sort_by_with_control(
        &mut values,
        &mut scratch,
        &mut RequestControl(&resources),
        &mut move |_, _, _| {
            core::hint::black_box(held.as_slice()[0]);
            state_in_callback
                .failed_release
                .store(true, Ordering::Relaxed);
            Err(original)
        },
    )
    .map_err(|error| {
        // This wrapper records immediately when the borrowed algorithm returns.
        raw_error.set(Some(error));
        slot_before_record.set(resources.failure());
        resources.record(error)
    });
    let freed = WATCH_FREED.load(Ordering::Acquire);
    std::println!(
        "CAPTURE_OBSERVATION {{\"case\":\"sort\",\"rawNative\":{},\"slotBeforeCallerRecord\":{},\"returnedNative\":{},\"finalSlotNative\":{},\"heldPhysicallyFreed\":{},\"releaseFailures\":{},\"liveLeases\":{}}}",
        raw_error.get().and_then(|e| e.native_hr).unwrap_or(0),
        slot_before_record
            .get()
            .and_then(|e| e.native_hr)
            .unwrap_or(0),
        result.as_ref().err().and_then(|e| e.native_hr).unwrap_or(0),
        resources.failure().and_then(|e| e.native_hr).unwrap_or(0),
        freed,
        resources.statistics().release_failures,
        resources.statistics().live_leases,
    );
    assert!(
        freed,
        "the comparator's captured paid buffer must retire naturally inside the call"
    );
    assert_eq!(
        result,
        Err(original),
        "original comparator error must own the first slot"
    );
    assert_eq!(resources.failure(), Some(original));
}

fn consume_clone(value: &BudgetVec<u8>, calls: &AtomicUsize) {
    calls.fetch_add(1, Ordering::Relaxed);
    core::hint::black_box(value.as_slice()[0]);
}

#[test]
fn by_value_clone_capture_refund_must_reject_success_and_next_consumer() {
    let _isolation = isolate_observer();
    let (resources, _, state) = setup(Limits::default());
    let mut source = BudgetVec::new(resources.clone(), ResourceKind::Scratch);
    source.try_resize_copy(3, 5u8).unwrap();
    let mut held = BudgetVec::new(resources.clone(), ResourceKind::Geometry);
    held.try_resize_copy(17, 9u8).unwrap();
    WATCH_PTR.store(held.as_ptr() as usize, Ordering::Relaxed);
    WATCH_FREED.store(false, Ordering::Relaxed);
    let state_in_callback = state.clone();
    let callback_calls = AtomicUsize::new(0);
    let callback_calls_ref = &callback_calls;
    let result = source.try_clone_with(move |value, _| {
        core::hint::black_box(held.as_slice()[0]);
        callback_calls_ref.fetch_add(1, Ordering::Relaxed);
        state_in_callback
            .failed_release
            .store(true, Ordering::Relaxed);
        Ok(*value)
    });
    let consumer_calls = AtomicUsize::new(0);
    if let Ok(value) = &result {
        consume_clone(value, &consumer_calls);
    }
    let freed = WATCH_FREED.load(Ordering::Acquire);
    std::println!(
        "CAPTURE_OBSERVATION {{\"case\":\"clone\",\"returnedOk\":{},\"returnedNative\":{},\"finalSlotNative\":{},\"heldPhysicallyFreed\":{},\"callbackCalls\":{},\"consumerCalls\":{},\"releaseFailures\":{},\"liveLeases\":{}}}",
        result.is_ok(),
        result.as_ref().err().and_then(|e| e.native_hr).unwrap_or(0),
        resources.failure().and_then(|e| e.native_hr).unwrap_or(0),
        freed,
        callback_calls.load(Ordering::Relaxed),
        consumer_calls.load(Ordering::Relaxed),
        resources.statistics().release_failures,
        resources.statistics().live_leases,
    );
    assert!(
        freed,
        "the callback's captured paid buffer must retire naturally inside the call"
    );
    assert!(
        result.is_err(),
        "capture cleanup fault must reject clone success"
    );
    assert_eq!(result.as_ref().err(), Some(&Error::native(-77)));
    assert_eq!(
        consumer_calls.load(Ordering::Relaxed),
        0,
        "next consumer must not enter"
    );
}
