// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE file in the project root for license information.
// ---------------------------------------------------------------------------------------------

use super::*;
use markdown_core::measurement_cache::CACHE_TOTAL_BYTES;
use std::cell::RefCell;
use std::mem::offset_of;

#[derive(Default)]
struct Allocations {
    fail: usize,
    requested: Vec<Layout>,
    released_guards: Vec<Layout>,
}
thread_local! { static ALLOCATIONS: RefCell<Allocations> = RefCell::default(); }

pub(super) fn reject_allocation(layout: Layout) -> bool {
    ALLOCATIONS.with_borrow_mut(|state| {
        state.requested.push(layout);
        state.fail == state.requested.len()
    })
}

pub(super) fn record_guard_release(layout: Layout) {
    ALLOCATIONS.with_borrow_mut(|state| state.released_guards.push(layout));
}

fn reset(fail: usize) {
    ALLOCATIONS.with_borrow_mut(|state| {
        *state = Allocations {
            fail,
            ..Allocations::default()
        }
    });
}

fn stats() -> MetricCacheStats {
    MetricCacheStats {
        header: MetricCacheHeader::of::<MetricCacheStats>(),
        ..MetricCacheStats::default()
    }
}
fn result() -> MetricCacheResult {
    MetricCacheResult {
        header: MetricCacheHeader::of::<MetricCacheResult>(),
        ..MetricCacheResult::default()
    }
}

struct Owner(*mut MetricCache);
impl Drop for Owner {
    fn drop(&mut self) {
        // SAFETY: test owns the one handle, and all calls have returned.
        unsafe { notepads_markdown_metric_cache_destroy(self.0) };
    }
}

fn create() -> (Owner, MetricCacheStats) {
    let request = MetricCacheHeader::of::<MetricCacheHeader>();
    let mut pointer = std::ptr::null_mut();
    let mut stats = stats();
    // SAFETY: independent, initialized PODs and output pointer.
    assert_eq!(
        unsafe { notepads_markdown_metric_cache_create(&request, &mut pointer, &mut stats) },
        0
    );
    (Owner(pointer), stats)
}

#[test]
fn frozen_wire_layouts_and_offsets_match_header() {
    assert_eq!(
        (
            size_of::<MetricCacheHeader>(),
            align_of::<MetricCacheHeader>()
        ),
        (8, 8)
    );
    assert_eq!(
        (
            size_of::<MetricCacheLookupRequest>(),
            align_of::<MetricCacheLookupRequest>()
        ),
        (24, 8)
    );
    assert_eq!(
        (
            size_of::<MetricCachePutRequest>(),
            align_of::<MetricCachePutRequest>()
        ),
        (24, 8)
    );
    assert_eq!(
        (
            size_of::<MetricCacheResult>(),
            align_of::<MetricCacheResult>()
        ),
        (32, 8)
    );
    assert_eq!(
        (
            size_of::<MetricCacheStats>(),
            align_of::<MetricCacheStats>()
        ),
        (96, 8)
    );
    assert_eq!(offset_of!(MetricCacheLookupRequest, expected_epoch), 8);
    assert_eq!(offset_of!(MetricCacheLookupRequest, output_capacity), 20);
    assert_eq!(offset_of!(MetricCachePutRequest, value_bytes), 20);
    assert_eq!(offset_of!(MetricCacheResult, disposition), 16);
    assert_eq!(offset_of!(MetricCacheResult, reserved), 28);
    assert_eq!(offset_of!(MetricCacheStats, rejected_puts), 72);
    assert_eq!(offset_of!(MetricCacheStats, entries), 80);
}

#[test]
fn owner_is_small_and_failed_first_backing_put_can_recover_without_changing_epoch() {
    reset(1);
    let (owner, failed) = create();
    assert!(owner.0.is_null());
    assert_eq!(failed.allocated_bytes, 0);
    assert_eq!(failed.enabled, 0);
    ALLOCATIONS.with_borrow(|state| {
        assert_eq!(state.requested.len(), 1);
        assert!(state.released_guards.is_empty());
    });
    reset(2);
    let (owner, initial) = create();
    assert!(!owner.0.is_null());
    assert_eq!(initial.allocated_bytes, size_of::<MetricCache>() as u64);
    assert_eq!(initial.entry_limit, 2048);
    ALLOCATIONS.with_borrow(|state| {
        assert_eq!(state.requested.len(), 1);
        assert_eq!(state.requested[0], Layout::new::<MetricCache>());
        assert!(state.released_guards.is_empty());
    });
    let put = MetricCachePutRequest {
        header: MetricCacheHeader::of::<MetricCachePutRequest>(),
        expected_epoch: 1,
        key_bytes: 1,
        value_bytes: 1,
    };
    let mut outcome = result();
    // SAFETY: independent live owner, request, result and byte spans.
    unsafe {
        assert_eq!(
            notepads_markdown_metric_cache_put(
                owner.0,
                &put,
                b"k".as_ptr(),
                b"v".as_ptr(),
                &mut outcome
            ),
            0
        );
        assert_eq!(outcome.disposition, 6); // Optional allocation failed.
        assert_eq!(
            notepads_markdown_metric_cache_put(
                owner.0,
                &put,
                b"k".as_ptr(),
                b"v".as_ptr(),
                &mut outcome
            ),
            0
        );
        assert_eq!(outcome.disposition, 3);
        assert_eq!(outcome.epoch, 1);
        let mut current = stats();
        assert_eq!(
            notepads_markdown_metric_cache_get_stats(owner.0, &mut current),
            0
        );
        assert!(current.allocated_bytes < CACHE_TOTAL_BYTES as u64);
        assert_eq!(current.entries, 1);
        assert_eq!(
            notepads_markdown_metric_cache_clear(owner.0, &mut current),
            0
        );
        assert_eq!(current.allocated_bytes, size_of::<MetricCache>() as u64);
        assert_eq!(current.entries, 0);
        assert_eq!(current.epoch, 2);
    }
    drop(owner);
    // A fresh owner has no data from its destroyed predecessor.
    let (owner, fresh) = create();
    assert_eq!(fresh.entries, 0);
    assert_eq!(fresh.epoch, 1);
    drop(owner);
}

#[test]
fn copy_out_query_clear_and_stale_put_round_trip() {
    reset(0);
    let (owner, _) = create();
    let mut result = result();
    let put = MetricCachePutRequest {
        header: MetricCacheHeader::of::<MetricCachePutRequest>(),
        expected_epoch: 1,
        key_bytes: 3,
        value_bytes: 5,
    };
    let mut query = MetricCacheLookupRequest {
        header: MetricCacheHeader::of::<MetricCacheLookupRequest>(),
        expected_epoch: 1,
        key_bytes: 3,
        output_capacity: 0,
    };
    // SAFETY: live owner and correctly sized, nonoverlapping spans/PODs.
    unsafe {
        assert_eq!(
            notepads_markdown_metric_cache_put(
                owner.0,
                &put,
                b"key".as_ptr(),
                b"value".as_ptr(),
                &mut result
            ),
            0
        );
        assert_eq!(result.disposition, 3);
        assert_eq!(
            notepads_markdown_metric_cache_lookup(
                owner.0,
                &query,
                b"key".as_ptr(),
                std::ptr::null_mut(),
                &mut result
            ),
            0
        );
        assert_eq!(
            (
                result.disposition,
                result.required_bytes,
                result.copied_bytes
            ),
            (2, 5, 0)
        );
        query.output_capacity = 4;
        let mut small = [0xa5; 4];
        assert_eq!(
            notepads_markdown_metric_cache_lookup(
                owner.0,
                &query,
                b"key".as_ptr(),
                small.as_mut_ptr(),
                &mut result
            ),
            0
        );
        assert_eq!(small, [0xa5; 4]);
        query.output_capacity = 5;
        let mut output = [0; 5];
        assert_eq!(
            notepads_markdown_metric_cache_lookup(
                owner.0,
                &query,
                b"key".as_ptr(),
                output.as_mut_ptr(),
                &mut result
            ),
            0
        );
        assert_eq!(output, *b"value");
        assert_eq!(result.disposition, 1);
        let mut stats = stats();
        assert_eq!(notepads_markdown_metric_cache_clear(owner.0, &mut stats), 0);
        assert_eq!(stats.epoch, 2);
        assert_eq!(
            notepads_markdown_metric_cache_lookup(
                owner.0,
                &query,
                b"key".as_ptr(),
                output.as_mut_ptr(),
                &mut result
            ),
            0
        );
        assert_eq!(result.disposition, 4);
        assert_eq!(
            notepads_markdown_metric_cache_put(
                owner.0,
                &put,
                b"key".as_ptr(),
                b"value".as_ptr(),
                &mut result
            ),
            0
        );
        assert_eq!(result.disposition, 4);
        assert_eq!(
            notepads_markdown_metric_cache_get_stats(owner.0, &mut stats),
            0
        );
        assert_eq!(stats.entries, 0);
        assert_eq!(stats.hits, 1);
        assert_eq!(stats.probes, 2);
        assert_eq!(stats.rejected_puts, 1);
    }
}

#[test]
fn malformed_headers_and_null_spans_are_rejected_before_access() {
    reset(0);
    let (owner, _) = create();
    let mut result = result();
    let mut request = MetricCacheLookupRequest {
        header: MetricCacheHeader::of::<MetricCacheLookupRequest>(),
        expected_epoch: 1,
        key_bytes: 1,
        output_capacity: 0,
    };
    // SAFETY: live PODs; null spans are deliberately rejected before dereference.
    unsafe {
        assert_eq!(
            notepads_markdown_metric_cache_lookup(
                owner.0,
                &request,
                std::ptr::null(),
                std::ptr::null_mut(),
                &mut result
            ),
            INVALID_ARGUMENT
        );
        request.header.version = 3;
        assert_eq!(
            notepads_markdown_metric_cache_lookup(
                owner.0,
                &request,
                b"k".as_ptr(),
                std::ptr::null_mut(),
                &mut result
            ),
            BAD_ABI
        );
        request.header = MetricCacheHeader::of::<MetricCacheHeader>();
        assert_eq!(
            notepads_markdown_metric_cache_lookup(
                owner.0,
                &request,
                b"k".as_ptr(),
                std::ptr::null_mut(),
                &mut result
            ),
            BAD_ABI
        );
        request.header = MetricCacheHeader::of::<MetricCacheLookupRequest>();
        request.output_capacity = 1;
        assert_eq!(
            notepads_markdown_metric_cache_lookup(
                owner.0,
                &request,
                b"k".as_ptr(),
                std::ptr::null_mut(),
                &mut result
            ),
            INVALID_ARGUMENT
        );
        assert_eq!(
            notepads_markdown_metric_cache_get_stats(owner.0, std::ptr::null_mut()),
            INVALID_ARGUMENT
        );
    }
    assert_eq!(span(std::ptr::null(), 1), Err(INVALID_ARGUMENT));
    #[cfg(target_pointer_width = "32")]
    assert_eq!(span(b"x".as_ptr(), u32::MAX), Err(INVALID_ARGUMENT));
}

#[test]
fn disabled_null_handle_and_oversize_admission_do_not_invoke_document_operations() {
    reset(0);
    let (owner, _) = create();
    let mut result = result();
    let key = vec![0; KEY_LIMIT + 1];
    let put = MetricCachePutRequest {
        header: MetricCacheHeader::of::<MetricCachePutRequest>(),
        expected_epoch: 1,
        key_bytes: key.len() as u32,
        value_bytes: 1,
    };
    // SAFETY: actual key buffer matches its declared length; all outputs independent.
    unsafe {
        assert_eq!(
            notepads_markdown_metric_cache_put(
                owner.0,
                &put,
                key.as_ptr(),
                b"v".as_ptr(),
                &mut result
            ),
            0
        );
        assert_eq!(result.disposition, 5);
        assert_eq!(
            notepads_markdown_metric_cache_put(
                std::ptr::null_mut(),
                &put,
                key.as_ptr(),
                b"v".as_ptr(),
                &mut result
            ),
            0
        );
        assert_eq!(result.disposition, 6);
        let mut stats = stats();
        assert_eq!(
            notepads_markdown_metric_cache_clear(std::ptr::null_mut(), &mut stats),
            0
        );
        assert_eq!(
            (stats.epoch, stats.enabled, stats.allocated_bytes),
            (0, 0, 0)
        );
        notepads_markdown_metric_cache_destroy(std::ptr::null_mut());
    }
}

#[test]
fn panic_is_contained_with_cache_specific_status() {
    assert_eq!(contained(|| panic!("cache boundary test")), INTERNAL_FAULT);
}
