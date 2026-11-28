// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE file in the project root for license information.
// ---------------------------------------------------------------------------------------------
#pragma once
#include "notepads/markdown/core.h"

// Private image admission ABI. Errors use the Core error POD.
// Prefixes are checked before full POD reads. All regions are valid, aligned,
// nonoverlapping and borrowed until return. Matching outputs are unchanged on
// error; successfully preflighted handle outputs are first set to null.
// Calls on a handle must finish before its one destruction. Distinct handles
// may run concurrently; the Rust coordinator serializes its short decisions.
// No callback/IO/decoder runs under a Rust lock. A panic poisons the coordinator;
// only destruction of its handles remains. Native memory is never owned here.
// Requests keep the global active slot until actual IO/decode termination,
// including late canceled requests. Free native bytes before destroying their
// bytes lease. Leases/pending requests may outlive policy and source handles.
// Image subquotas cover reserved native buffer capacities; root allocation
// funding, bounded WinRT transfer buffers, WIC/GPU internals are separate.

enum { NOTEPADS_MARKDOWN_IMAGE_ABI_VERSION = 2, NOTEPADS_MARKDOWN_IMAGE_MAX_ENTRIES = 256 };
enum notepads_markdown_image_state { NOTEPADS_MARKDOWN_IMAGE_UNREQUESTED = 0, NOTEPADS_MARKDOWN_IMAGE_PENDING = 1, NOTEPADS_MARKDOWN_IMAGE_READY = 2,
    NOTEPADS_MARKDOWN_IMAGE_UNAVAILABLE = 3, NOTEPADS_MARKDOWN_IMAGE_FAILED = 4, NOTEPADS_MARKDOWN_IMAGE_LIMITED = 5 };
enum notepads_markdown_image_failure { NOTEPADS_MARKDOWN_IMAGE_NO_FAILURE = 0, NOTEPADS_MARKDOWN_IMAGE_NOT_PROVIDED = 1, NOTEPADS_MARKDOWN_IMAGE_CANCELLED = 2,
    NOTEPADS_MARKDOWN_IMAGE_PROVIDER_ERROR = 3, NOTEPADS_MARKDOWN_IMAGE_DECODE_ERROR = 4, NOTEPADS_MARKDOWN_IMAGE_UNSUPPORTED = 5,
    NOTEPADS_MARKDOWN_IMAGE_QUOTA = 6, NOTEPADS_MARKDOWN_IMAGE_RELEASED = 7 };
enum notepads_markdown_image_byte_kind { NOTEPADS_MARKDOWN_IMAGE_ENCODED = 0, NOTEPADS_MARKDOWN_IMAGE_DECODED = 1 };

typedef struct notepads_markdown_image_policy notepads_markdown_image_policy;
typedef struct notepads_markdown_image_request notepads_markdown_image_request;
typedef struct notepads_markdown_image_bytes notepads_markdown_image_bytes;
typedef struct NOTEPADS_MARKDOWN_FFI_ALIGN8 notepads_markdown_image_header { uint32_t abi_version; uint32_t struct_size; } notepads_markdown_image_header;
typedef struct NOTEPADS_MARKDOWN_FFI_ALIGN8 notepads_markdown_image_epoch
{
    notepads_markdown_core_source source;
    uint64_t source_generation;
    uint64_t provider_generation;
} notepads_markdown_image_epoch;
typedef struct NOTEPADS_MARKDOWN_FFI_ALIGN8 notepads_markdown_image_epoch_descriptor
{
    notepads_markdown_image_header header;
    notepads_markdown_image_epoch epoch;
} notepads_markdown_image_epoch_descriptor;
typedef struct NOTEPADS_MARKDOWN_FFI_ALIGN8 notepads_markdown_image_key
{
    notepads_markdown_image_epoch epoch;
    uint32_t object;
    uint32_t reserved;
    uint64_t request;
} notepads_markdown_image_key;
typedef struct NOTEPADS_MARKDOWN_FFI_ALIGN8 notepads_markdown_image_snapshot
{
    notepads_markdown_image_header header;
    notepads_markdown_image_epoch epoch;
    uint64_t state_generation;
    uint32_t count;
    uint32_t active_requests;
    uint32_t reservations;
    uint32_t has_epoch;
    uint64_t encoded_bytes;
    uint64_t decoded_bytes;
} notepads_markdown_image_snapshot;

enum notepads_markdown_image_kind { NOTEPADS_MARKDOWN_IMAGE_RASTER = 0, NOTEPADS_MARKDOWN_IMAGE_SVG = 1, NOTEPADS_MARKDOWN_IMAGE_AUTO = 2 };
enum { NOTEPADS_MARKDOWN_IMAGE_RESOLUTION_LIMITED = 1 };
enum notepads_markdown_image_status { NOTEPADS_MARKDOWN_IMAGE_NEEDS_RENDER = 1, NOTEPADS_MARKDOWN_IMAGE_TARGET_SATISFIED = 2,
    NOTEPADS_MARKDOWN_IMAGE_TARGET_LIMITED = 4, NOTEPADS_MARKDOWN_IMAGE_HAS_METADATA = 8, NOTEPADS_MARKDOWN_IMAGE_CAPACITY_WAIT = 16 };

typedef struct NOTEPADS_MARKDOWN_FFI_ALIGN8 notepads_markdown_image_target
{
    uint64_t font_generation, target_generation;
    double available_width_dip, effective_physical_scale;
} notepads_markdown_image_target;
typedef struct NOTEPADS_MARKDOWN_FFI_ALIGN8 notepads_markdown_image_desired
{
    notepads_markdown_image_header header;
    notepads_markdown_image_target target;
} notepads_markdown_image_desired;
typedef struct NOTEPADS_MARKDOWN_FFI_ALIGN8 notepads_markdown_image_request_info
{
    notepads_markdown_image_header header;
    notepads_markdown_image_key key;
    notepads_markdown_image_target target;
} notepads_markdown_image_request_info;
typedef struct NOTEPADS_MARKDOWN_FFI_ALIGN8 notepads_markdown_image_metadata
{
    uint32_t kind, flags;
    uint32_t intrinsic_width, intrinsic_height;
    uint32_t decoded_width, decoded_height, orientation, stride;
    double natural_width_dip, natural_height_dip;
    // Actual f32 renderer extents widened to double. SourceRect=(0,0,w,h).
    double source_width, source_height;
    uint32_t viewport_width_bits, viewport_height_bits, scale_bits, reserved;
} notepads_markdown_image_metadata;
typedef struct NOTEPADS_MARKDOWN_FFI_ALIGN8 notepads_markdown_image_entry
{
    uint32_t object, state;
    uint64_t published_request;
    notepads_markdown_image_metadata metadata;
    notepads_markdown_image_target desired_target, published_target;
    uint64_t pending_request;
    uint32_t failure;
    int32_t error;
    uint32_t status_flags, reserved, detail, padding;
} notepads_markdown_image_entry;
typedef struct NOTEPADS_MARKDOWN_FFI_ALIGN8 notepads_markdown_image_completion
{
    notepads_markdown_image_header header;
    notepads_markdown_image_metadata metadata;
    notepads_markdown_image_target target;
    uint32_t ready, failure;
    int32_t error;
    // Typed detail=(SVG fault kind<<16)|limit; or0. Kind1..7/16..24,
    // limit0..15. Native HRESULT remains in error. No user string.
    uint32_t detail;
} notepads_markdown_image_completion;

typedef struct NOTEPADS_MARKDOWN_FFI_ALIGN8 notepads_markdown_image_process_capacity
{
    notepads_markdown_image_header header;
    uint64_t generation, encoded_bytes, decoded_bytes;
} notepads_markdown_image_process_capacity;
typedef struct NOTEPADS_MARKDOWN_FFI_ALIGN8 notepads_markdown_image_raster_plan
{
    notepads_markdown_image_header header;
    uint32_t width, height, stride, maximum_side;
    uint64_t bytes;
} notepads_markdown_image_raster_plan;
typedef void (NOTEPADS_MARKDOWN_FFI_CALL *notepads_markdown_image_capacity_notify)(uint64_t generation);

#ifdef __cplusplus
static_assert(sizeof(notepads_markdown_image_epoch) == 40 && sizeof(notepads_markdown_image_epoch_descriptor) == 48);
static_assert(sizeof(notepads_markdown_image_key) == 56);
static_assert(sizeof(notepads_markdown_image_snapshot) == 88 && offsetof(notepads_markdown_image_snapshot, decoded_bytes) == 80);
static_assert(sizeof(notepads_markdown_image_target) == 32 && sizeof(notepads_markdown_image_desired) == 40);
static_assert(sizeof(notepads_markdown_image_request_info) == 96 && sizeof(notepads_markdown_image_metadata) == 80);
static_assert(sizeof(notepads_markdown_image_entry) == 192 && sizeof(notepads_markdown_image_completion) == 136);
static_assert(offsetof(notepads_markdown_image_entry, detail) == 184 && alignof(notepads_markdown_image_entry) == 8);
static_assert(sizeof(notepads_markdown_image_process_capacity) == 32 && alignof(notepads_markdown_image_process_capacity) == 8);
static_assert(sizeof(notepads_markdown_image_raster_plan) == 32 && offsetof(notepads_markdown_image_raster_plan, bytes) == 24);
extern "C"
{
#endif
    uint32_t NOTEPADS_MARKDOWN_FFI_CALL notepads_markdown_image_abi_version(void);
    int32_t NOTEPADS_MARKDOWN_FFI_CALL notepads_markdown_image_policy_create(notepads_markdown_image_policy **output, notepads_markdown_core_error *error);
    int32_t NOTEPADS_MARKDOWN_FFI_CALL notepads_markdown_image_policy_begin(notepads_markdown_image_policy *policy,
        const notepads_markdown_image_epoch_descriptor *epoch, notepads_markdown_core_error *error);
    int32_t NOTEPADS_MARKDOWN_FFI_CALL notepads_markdown_image_policy_desire(notepads_markdown_image_policy *policy, uint32_t object,
        const notepads_markdown_image_desired *desired, notepads_markdown_core_error *error);
    // AUTO is first unclassified load with a captured desired target; one
    // request/slot admits either format after reading actual immutable bytes.
    int32_t NOTEPADS_MARKDOWN_FFI_CALL notepads_markdown_image_policy_start(notepads_markdown_image_policy *policy, uint32_t object, uint32_t kind,
        notepads_markdown_image_request **output, notepads_markdown_image_request_info *info, notepads_markdown_core_error *error);
    int32_t NOTEPADS_MARKDOWN_FFI_CALL notepads_markdown_image_policy_generation(const notepads_markdown_image_policy *policy,
        uint64_t *generation, notepads_markdown_core_error *error);
    int32_t NOTEPADS_MARKDOWN_FFI_CALL notepads_markdown_image_policy_read(const notepads_markdown_image_policy *policy,
        notepads_markdown_image_entry *rows, uint32_t capacity, uint32_t row_size,
        notepads_markdown_image_snapshot *output, notepads_markdown_core_error *error);
    int32_t NOTEPADS_MARKDOWN_FFI_CALL notepads_markdown_image_policy_complete(notepads_markdown_image_policy *policy, const notepads_markdown_image_request *request,
        const notepads_markdown_image_completion *completion, const notepads_markdown_image_bytes *decoded, notepads_markdown_core_error *error);
    // Caller verifies exact current content/render identity;
    // fresh request/target and this same policy's original admitted allocation
    // remain live. Adds a current binding without another physical byte charge.
    int32_t NOTEPADS_MARKDOWN_FFI_CALL notepads_markdown_image_policy_complete_reused(notepads_markdown_image_policy *policy, const notepads_markdown_image_request *request,
        const notepads_markdown_image_completion *completion, const notepads_markdown_image_bytes *decoded, notepads_markdown_core_error *error);
    // Keep intrinsic metadata, invalidate publication, and retain every physical charge.
    int32_t NOTEPADS_MARKDOWN_FFI_CALL notepads_markdown_image_policy_evict(notepads_markdown_image_policy *policy, uint32_t object, notepads_markdown_core_error *error);
    int32_t NOTEPADS_MARKDOWN_FFI_CALL notepads_markdown_image_policy_replace_source(notepads_markdown_image_policy *policy, uint32_t object, notepads_markdown_core_error *error);
    // Only a quiescent row with no outstanding request/reservation may be forgotten.
    int32_t NOTEPADS_MARKDOWN_FFI_CALL notepads_markdown_image_policy_forget(notepads_markdown_image_policy *policy, uint32_t object, notepads_markdown_core_error *error);
    int32_t NOTEPADS_MARKDOWN_FFI_CALL notepads_markdown_image_policy_retry(notepads_markdown_image_policy *policy, uint32_t object, uint32_t *eligible, notepads_markdown_core_error *error);
    int32_t NOTEPADS_MARKDOWN_FFI_CALL notepads_markdown_image_request_note_metadata(const notepads_markdown_image_request *request,
        const notepads_markdown_image_metadata *metadata, notepads_markdown_core_error *error);
    int32_t NOTEPADS_MARKDOWN_FFI_CALL notepads_markdown_image_get_process_capacity(notepads_markdown_image_process_capacity *output, notepads_markdown_core_error *error);
    int32_t NOTEPADS_MARKDOWN_FFI_CALL notepads_markdown_image_plan_raster(const notepads_markdown_image_metadata *metadata,
        const notepads_markdown_image_target *target, notepads_markdown_image_raster_plan *output, notepads_markdown_core_error *error);
    // One static module callback, no per-control context. Called outside all ledger locks.
    // The callback must not throw or register/unregister itself. Caller serializes
    // registration and pins code; null synchronously drains callbacks, outside native locks.
    void NOTEPADS_MARKDOWN_FFI_CALL notepads_markdown_image_set_process_capacity_notify(notepads_markdown_image_capacity_notify notify);
    int32_t NOTEPADS_MARKDOWN_FFI_CALL notepads_markdown_image_request_reserve(const notepads_markdown_image_request *request, uint32_t kind,
        uint64_t bytes, notepads_markdown_image_bytes **output, notepads_markdown_core_error *error);
    // Once per reserved allocation. Actual capacity still counts after retirement.
    int32_t NOTEPADS_MARKDOWN_FFI_CALL notepads_markdown_image_bytes_admit(const notepads_markdown_image_bytes *bytes, uint64_t actual, notepads_markdown_core_error *error);
    int32_t NOTEPADS_MARKDOWN_FFI_CALL notepads_markdown_image_policy_destroy(notepads_markdown_image_policy *policy);
    int32_t NOTEPADS_MARKDOWN_FFI_CALL notepads_markdown_image_request_destroy(notepads_markdown_image_request *request);
    int32_t NOTEPADS_MARKDOWN_FFI_CALL notepads_markdown_image_bytes_destroy(notepads_markdown_image_bytes *bytes);
#ifdef __cplusplus
}
#endif
