// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE file in the project root for license information.
// ---------------------------------------------------------------------------------------------

//! Independent cache ABI. Unsafe allocation and borrowed C spans stop at this boundary.

use markdown_core::measurement_cache::{
    CacheResult, CacheStats, ENTRY_BYTES_LIMIT, ENTRY_LIMIT, KEY_LIMIT, MeasurementCache,
};
use std::alloc::{Layout, alloc, alloc_zeroed, dealloc};
use std::ptr::NonNull;

pub const METRIC_CACHE_VERSION: u32 = 1;
const BAD_ABI: i32 = -1;
const INVALID_ARGUMENT: i32 = -2;
const INTERNAL_FAULT: i32 = -3;

#[repr(C, align(8))]
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct MetricCacheHeader {
    pub version: u32,
    pub struct_size: u32,
}

impl MetricCacheHeader {
    pub fn of<T>() -> Self {
        Self {
            version: METRIC_CACHE_VERSION,
            struct_size: size_of::<T>() as u32,
        }
    }
}

#[repr(C, align(8))]
#[derive(Clone, Copy, Debug, Default)]
pub struct MetricCacheLookupRequest {
    pub header: MetricCacheHeader,
    pub expected_epoch: u64,
    pub key_bytes: u32,
    pub output_capacity: u32,
}

#[repr(C, align(8))]
#[derive(Clone, Copy, Debug, Default)]
pub struct MetricCachePutRequest {
    pub header: MetricCacheHeader,
    pub expected_epoch: u64,
    pub key_bytes: u32,
    pub value_bytes: u32,
}

#[repr(C, align(8))]
#[derive(Clone, Copy, Debug, Default)]
pub struct MetricCacheResult {
    pub header: MetricCacheHeader,
    pub epoch: u64,
    pub disposition: u32,
    pub required_bytes: u32,
    pub copied_bytes: u32,
    pub reserved: u32,
}

impl From<CacheResult> for MetricCacheResult {
    fn from(value: CacheResult) -> Self {
        Self {
            header: MetricCacheHeader::of::<Self>(),
            epoch: value.epoch,
            disposition: value.disposition as u32,
            required_bytes: value.required_bytes,
            copied_bytes: value.copied_bytes,
            reserved: 0,
        }
    }
}

#[repr(C, align(8))]
#[derive(Clone, Copy, Debug, Default)]
pub struct MetricCacheStats {
    pub header: MetricCacheHeader,
    pub epoch: u64,
    pub allocated_bytes: u64,
    pub occupied_bytes: u64,
    pub hits: u64,
    pub misses: u64,
    pub probes: u64,
    pub puts: u64,
    pub evictions: u64,
    pub rejected_puts: u64,
    pub entries: u32,
    pub enabled: u32,
    pub entry_limit: u32,
    pub reserved: u32,
}

impl From<CacheStats> for MetricCacheStats {
    fn from(value: CacheStats) -> Self {
        Self {
            header: MetricCacheHeader::of::<Self>(),
            epoch: value.epoch,
            allocated_bytes: value.allocated_bytes,
            occupied_bytes: value.occupied_bytes,
            hits: value.hits,
            misses: value.misses,
            probes: value.probes,
            puts: value.puts,
            evictions: value.evictions,
            rejected_puts: value.rejected_puts,
            entries: value.entries,
            enabled: u32::from(value.enabled),
            entry_limit: ENTRY_LIMIT as u32,
            reserved: 0,
        }
    }
}

/// Caller-owned opaque handle. Only the exact allocation below may create one.
pub struct MetricCache {
    cache: MeasurementCache,
}

struct Allocation {
    pointer: NonNull<u8>,
    layout: Layout,
}

impl Allocation {
    fn new(layout: Layout, zeroed: bool) -> Option<Self> {
        #[cfg(test)]
        if tests::reject_allocation(layout) {
            return None;
        }
        // SAFETY: both fixed layouts have nonzero size and valid alignment.
        let pointer = unsafe {
            if zeroed {
                alloc_zeroed(layout)
            } else {
                alloc(layout)
            }
        };
        NonNull::new(pointer).map(|pointer| Self { pointer, layout })
    }

    fn into_raw(self) -> *mut u8 {
        let pointer = self.pointer.as_ptr();
        std::mem::forget(self);
        pointer
    }
}

impl Drop for Allocation {
    fn drop(&mut self) {
        #[cfg(test)]
        tests::record_guard_release(self.layout);
        // SAFETY: this guard exclusively owns the unchanged allocation and layout.
        unsafe { dealloc(self.pointer.as_ptr(), self.layout) };
    }
}

fn contained(action: impl FnOnce() -> i32) -> i32 {
    let result = crate::contain_panic(action);
    if result == crate::STATUS_PANIC {
        INTERNAL_FAULT
    } else {
        result
    }
}

fn aligned<T>(pointer: *const T) -> bool {
    !pointer.is_null() && (pointer as usize).is_multiple_of(align_of::<T>())
}

/// Caller promises an aligned readable eight-byte prefix for every nonnull POD.
unsafe fn header<T>(pointer: *const T) -> Result<(), i32> {
    if !aligned(pointer) {
        return Err(INVALID_ARGUMENT);
    }
    // SAFETY: prefix validity is the external caller's obligation.
    let value = unsafe { pointer.cast::<MetricCacheHeader>().read() };
    if value != MetricCacheHeader::of::<T>() {
        Err(BAD_ABI)
    } else {
        Ok(())
    }
}

fn span(pointer: *const u8, length: u32) -> Result<(), i32> {
    if length as u64 > isize::MAX as u64 || (length != 0 && pointer.is_null()) {
        Err(INVALID_ARGUMENT)
    } else {
        Ok(())
    }
}

/// Shared calls borrow only while caller RAII guarantees the owner remains live.
unsafe fn cache<'a>(pointer: *mut MetricCache) -> Result<Option<&'a MeasurementCache>, i32> {
    if pointer.is_null() {
        return Ok(None);
    }
    if !aligned(pointer) {
        return Err(INVALID_ARGUMENT);
    }
    // SAFETY: caller supplies a live handle and synchronizes destruction.
    Ok(Some(unsafe { &(*pointer).cache }))
}

fn allocate_backing(bytes: usize) -> Option<Box<[u8]>> {
    let layout = Layout::array::<u8>(bytes).ok()?;
    let allocation = Allocation::new(layout, true)?;
    // SAFETY: the fallible boundary allocation initialized exactly this slice,
    // and Box returns it to the same global allocator with the identical layout.
    Some(unsafe {
        Box::from_raw(std::ptr::slice_from_raw_parts_mut(
            allocation.into_raw(),
            bytes,
        ))
    })
}

/// # Safety
/// Valid readable request/output prefixes, disjoint writable outputs; out_cache is pointer-aligned.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn notepads_markdown_metric_cache_create(
    request: *const MetricCacheHeader,
    out_cache: *mut *mut MetricCache,
    out_stats: *mut MetricCacheStats,
) -> i32 {
    contained(|| {
        // SAFETY: caller supplies the required live POD prefixes and output pointer.
        let preflight = unsafe { header(request).and_then(|()| header(out_stats)) };
        if let Err(code) = preflight {
            return code;
        }
        if !aligned(out_cache) {
            return INVALID_ARGUMENT;
        }
        // SAFETY: validated outputs are independent and writable.
        unsafe {
            out_cache.write(std::ptr::null_mut());
            out_stats.write(CacheStats::default().into());
        }
        let owner_layout = Layout::new::<MetricCache>();
        let Some(owner) = Allocation::new(owner_layout, false) else {
            return 0;
        };
        let Ok(cache) = MeasurementCache::lazy(owner_layout.size(), allocate_backing) else {
            return INTERNAL_FAULT;
        };
        let stats = cache.stats();
        let pointer = owner.pointer.as_ptr().cast::<MetricCache>();
        // SAFETY: owner has the exact MetricCache layout and has not yet been initialized.
        unsafe {
            pointer.write(MetricCache { cache });
        }
        let pointer = owner.into_raw().cast::<MetricCache>();
        // SAFETY: the initialized owner is published exactly once; outputs were checked above.
        unsafe {
            out_stats.write(stats.into());
            out_cache.write(pointer);
        }
        0
    })
}

/// # Safety
/// A null handle or one unique live owner with all shared calls already completed.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn notepads_markdown_metric_cache_destroy(pointer: *mut MetricCache) {
    let _ = contained(|| {
        if !pointer.is_null() {
            // SAFETY: matching exact Layout, initialized by create; unique final ownership.
            unsafe {
                drop(Box::from_raw(pointer));
            }
        }
        0
    });
}

/// # Safety
/// Live synchronized handle (or null), valid independent writable stats POD.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn notepads_markdown_metric_cache_clear(
    pointer: *mut MetricCache,
    out_stats: *mut MetricCacheStats,
) -> i32 {
    contained(|| {
        // SAFETY: caller guarantees POD and handle lifetime.
        let checked = unsafe { header(out_stats).and_then(|()| cache(pointer)) };
        match checked {
            Err(code) => code,
            Ok(cache) => {
                let stats = cache.map_or_else(CacheStats::default, MeasurementCache::clear);
                // SAFETY: preflight validated the output POD.
                unsafe {
                    out_stats.write(stats.into());
                }
                0
            }
        }
    })
}

/// # Safety
/// Live synchronized handle (or null), valid independent writable stats POD.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn notepads_markdown_metric_cache_get_stats(
    pointer: *mut MetricCache,
    out_stats: *mut MetricCacheStats,
) -> i32 {
    contained(|| {
        // SAFETY: caller guarantees POD and handle lifetime.
        let checked = unsafe { header(out_stats).and_then(|()| cache(pointer)) };
        match checked {
            Err(code) => code,
            Ok(cache) => {
                let stats = cache.map_or_else(CacheStats::default, MeasurementCache::stats);
                // SAFETY: preflight validated the output POD.
                unsafe {
                    out_stats.write(stats.into());
                }
                0
            }
        }
    })
}

/// # Safety
/// Live handle and PODs, disjoint key/output/result spans valid through return; no concurrent destroy.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn notepads_markdown_metric_cache_lookup(
    pointer: *mut MetricCache,
    request: *const MetricCacheLookupRequest,
    key: *const u8,
    output: *mut u8,
    result: *mut MetricCacheResult,
) -> i32 {
    contained(|| {
        // SAFETY: caller supplies valid prefixes and handle lifetime.
        let checked = unsafe {
            header(request)
                .and_then(|()| header(result))
                .and_then(|()| cache(pointer))
        };
        let cache = match checked {
            Ok(value) => value,
            Err(code) => return code,
        };
        // SAFETY: exact request header promises the full readable POD.
        let request = unsafe { request.read() };
        if let Err(code) =
            span(key, request.key_bytes).and_then(|()| span(output, request.output_capacity))
        {
            return code;
        }
        let found = if let Some(cache) = cache {
            if request.key_bytes == 0 || request.key_bytes as usize > KEY_LIMIT {
                cache.lookup(request.expected_epoch, &[], &mut [])
            } else {
                // SAFETY: caller spans are live and disjoint; lengths were checked for isize.
                let key = unsafe { std::slice::from_raw_parts(key, request.key_bytes as usize) };
                let output = if request.output_capacity == 0 {
                    &mut []
                } else {
                    // SAFETY: zero length/null case was handled separately.
                    unsafe {
                        std::slice::from_raw_parts_mut(output, request.output_capacity as usize)
                    }
                };
                cache.lookup(request.expected_epoch, key, output)
            }
        } else {
            CacheResult::disabled()
        };
        // SAFETY: validated independent output.
        unsafe {
            result.write(found.into());
        }
        0
    })
}

/// # Safety
/// Live handle and PODs, disjoint borrowed key/value/result spans; no concurrent destroy.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn notepads_markdown_metric_cache_put(
    pointer: *mut MetricCache,
    request: *const MetricCachePutRequest,
    key: *const u8,
    value: *const u8,
    result: *mut MetricCacheResult,
) -> i32 {
    contained(|| {
        // SAFETY: caller supplies valid prefixes and handle lifetime.
        let checked = unsafe {
            header(request)
                .and_then(|()| header(result))
                .and_then(|()| cache(pointer))
        };
        let cache = match checked {
            Ok(value) => value,
            Err(code) => return code,
        };
        // SAFETY: exact header promises full readable request.
        let request = unsafe { request.read() };
        if let Err(code) =
            span(key, request.key_bytes).and_then(|()| span(value, request.value_bytes))
        {
            return code;
        }
        let stored = if let Some(cache) = cache {
            if request.key_bytes == 0
                || request.key_bytes as usize > KEY_LIMIT
                || request.value_bytes == 0
                || u64::from(request.key_bytes) + u64::from(request.value_bytes)
                    > ENTRY_BYTES_LIMIT as u64
            {
                cache.put(request.expected_epoch, &[], &[])
            } else {
                // SAFETY: caller supplies live nonempty readable spans; sizes validated above.
                let key = unsafe { std::slice::from_raw_parts(key, request.key_bytes as usize) };
                // SAFETY: same nonempty span obligation for the payload.
                let value =
                    unsafe { std::slice::from_raw_parts(value, request.value_bytes as usize) };
                cache.put(request.expected_epoch, key, value)
            }
        } else {
            CacheResult::disabled()
        };
        // SAFETY: validated independent output.
        unsafe {
            result.write(stored.into());
        }
        0
    })
}

#[cfg(test)]
mod tests;
