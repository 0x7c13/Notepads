// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE file in the project root for license information.
// ---------------------------------------------------------------------------------------------
#pragma once
#include "notepads/markdown/ffi.h"
#include <stddef.h>
#include <stdint.h>

#define NOTEPADS_MARKDOWN_METRIC_CACHE_VERSION 1u
#define NOTEPADS_MARKDOWN_METRIC_CACHE_TOTAL_BYTES 8388608u
#define NOTEPADS_MARKDOWN_METRIC_CACHE_ENTRY_LIMIT 2048u
#define NOTEPADS_MARKDOWN_METRIC_CACHE_KEY_LIMIT 65536u
#define NOTEPADS_MARKDOWN_METRIC_CACHE_ENTRY_BYTES_LIMIT 524288u

// TOTAL_BYTES is an allocation ceiling, including owner and growth overlap.
// Backing grows fallibly on put; Clear releases it and advances the epoch.
// A valid call returns 0, including optional allocation failure at create or put.
// Negative values diagnose ABI/argument/internal faults, never a document failure.
#define NOTEPADS_MARKDOWN_METRIC_CACHE_BAD_ABI (-1)
#define NOTEPADS_MARKDOWN_METRIC_CACHE_INVALID_ARGUMENT (-2)
#define NOTEPADS_MARKDOWN_METRIC_CACHE_INTERNAL_FAULT (-3)
#define NOTEPADS_MARKDOWN_METRIC_CACHE_MISS 0u
#define NOTEPADS_MARKDOWN_METRIC_CACHE_HIT 1u
#define NOTEPADS_MARKDOWN_METRIC_CACHE_NEED_BUFFER 2u
#define NOTEPADS_MARKDOWN_METRIC_CACHE_STORED 3u
#define NOTEPADS_MARKDOWN_METRIC_CACHE_STALE_EPOCH 4u
#define NOTEPADS_MARKDOWN_METRIC_CACHE_TOO_LARGE 5u
#define NOTEPADS_MARKDOWN_METRIC_CACHE_DISABLED 6u

typedef struct notepads_markdown_metric_cache notepads_markdown_metric_cache;

typedef struct NOTEPADS_MARKDOWN_FFI_ALIGN8 notepads_markdown_metric_cache_header
{
    uint32_t version;
    uint32_t struct_size;
} notepads_markdown_metric_cache_header;

typedef struct NOTEPADS_MARKDOWN_FFI_ALIGN8 notepads_markdown_metric_cache_lookup_request
{
    notepads_markdown_metric_cache_header header;
    uint64_t expected_epoch;
    uint32_t key_bytes;
    uint32_t output_capacity;
} notepads_markdown_metric_cache_lookup_request;

typedef struct NOTEPADS_MARKDOWN_FFI_ALIGN8 notepads_markdown_metric_cache_put_request
{
    notepads_markdown_metric_cache_header header;
    uint64_t expected_epoch;
    uint32_t key_bytes;
    uint32_t value_bytes;
} notepads_markdown_metric_cache_put_request;

typedef struct NOTEPADS_MARKDOWN_FFI_ALIGN8 notepads_markdown_metric_cache_result
{
    notepads_markdown_metric_cache_header header;
    uint64_t epoch;
    uint32_t disposition;
    uint32_t required_bytes;
    uint32_t copied_bytes;
    uint32_t reserved;
} notepads_markdown_metric_cache_result;

typedef struct NOTEPADS_MARKDOWN_FFI_ALIGN8 notepads_markdown_metric_cache_stats
{
    notepads_markdown_metric_cache_header header;
    uint64_t epoch;
    uint64_t allocated_bytes;
    uint64_t occupied_bytes;
    uint64_t hits;
    uint64_t misses;
    uint64_t probes;
    uint64_t puts;
    uint64_t evictions;
    uint64_t rejected_puts;
    uint32_t entries;
    uint32_t enabled;
    uint32_t entry_limit;
    uint32_t reserved;
} notepads_markdown_metric_cache_stats;

#ifdef __cplusplus
static_assert(sizeof(notepads_markdown_metric_cache_header) == 8 && alignof(notepads_markdown_metric_cache_header) == 8);
static_assert(sizeof(notepads_markdown_metric_cache_lookup_request) == 24 && alignof(notepads_markdown_metric_cache_lookup_request) == 8);
static_assert(sizeof(notepads_markdown_metric_cache_put_request) == 24 && alignof(notepads_markdown_metric_cache_put_request) == 8);
static_assert(sizeof(notepads_markdown_metric_cache_result) == 32 && alignof(notepads_markdown_metric_cache_result) == 8);
static_assert(sizeof(notepads_markdown_metric_cache_stats) == 96 && alignof(notepads_markdown_metric_cache_stats) == 8);
static_assert(offsetof(notepads_markdown_metric_cache_lookup_request, expected_epoch) == 8);
static_assert(offsetof(notepads_markdown_metric_cache_lookup_request, key_bytes) == 16);
static_assert(offsetof(notepads_markdown_metric_cache_result, disposition) == 16);
static_assert(offsetof(notepads_markdown_metric_cache_stats, rejected_puts) == 72);
static_assert(offsetof(notepads_markdown_metric_cache_stats, entries) == 80);
extern "C"
{
#endif

// Every input/output POD must begin with version1 and its exact full struct_size.
// All pointers must be live, correctly aligned and valid for their declared spans.
// Borrowed input/output spans must not overlap each other, POD outputs, or cache memory.
// Ownership: success returns either an opaque owner or null (cache disabled
// because the owner could not be allocated). Backing storage is allocated lazily.
// Destroy must not race any call; caller RAII keeps the owner alive through every
// worker. Destroy(null) is harmless.
// Other null-cache calls return disabled, with epoch/allocated_bytes/entries zero.
// Key must be 1..65536 bytes; value must be nonempty; key+value <=524288.
// Cache errors never set the document Operation's first_fault.
int32_t NOTEPADS_MARKDOWN_FFI_CALL notepads_markdown_metric_cache_create(
    const notepads_markdown_metric_cache_header* request,
    notepads_markdown_metric_cache** out_cache,
    notepads_markdown_metric_cache_stats* out_stats);
void NOTEPADS_MARKDOWN_FFI_CALL notepads_markdown_metric_cache_destroy(notepads_markdown_metric_cache* cache);
int32_t NOTEPADS_MARKDOWN_FFI_CALL notepads_markdown_metric_cache_clear(
    notepads_markdown_metric_cache* cache,
    notepads_markdown_metric_cache_stats* out_stats);
int32_t NOTEPADS_MARKDOWN_FFI_CALL notepads_markdown_metric_cache_get_stats(
    notepads_markdown_metric_cache* cache,
    notepads_markdown_metric_cache_stats* out_stats);

// Capacity0 permits output=null. NeedBuffer reports required and writes no bytes.
// Every call performs a new complete key lookup. No pin/reader survives return.
// Pass the miss epoch to put; a later clear invalidates that writer.
int32_t NOTEPADS_MARKDOWN_FFI_CALL notepads_markdown_metric_cache_lookup(
    notepads_markdown_metric_cache* cache,
    const notepads_markdown_metric_cache_lookup_request* request,
    const uint8_t* key,
    uint8_t* output,
    notepads_markdown_metric_cache_result* result);
int32_t NOTEPADS_MARKDOWN_FFI_CALL notepads_markdown_metric_cache_put(
    notepads_markdown_metric_cache* cache,
    const notepads_markdown_metric_cache_put_request* request,
    const uint8_t* key,
    const uint8_t* value,
    notepads_markdown_metric_cache_result* result);

#ifdef __cplusplus
}
#endif
