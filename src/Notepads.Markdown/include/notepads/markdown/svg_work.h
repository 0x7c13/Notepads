// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE file in the project root for license information.
// ---------------------------------------------------------------------------------------------
#pragma once
#include "notepads/markdown/images.h"

// Private work-capacity ABI1. This does not enable SVG decoding or change image
// ABI1. All operations, including read, require one exclusive live account;
// distinct accounts may operate concurrently. No borrowed pointer escapes a call.
// create captures the request, source, model-root and cancellation identity.
// The original request must remain alive through actual work and UI completion.
// Work allocations count on the original model and on the shared image policy.
// Already root-funded encoded/decoded buffers cannot use this interface.
enum { NOTEPADS_MARKDOWN_SVG_WORK_ABI_VERSION = 1 };
enum notepads_markdown_svg_work_kind
{
    NOTEPADS_MARKDOWN_SVG_WORK_ALLOCATION = 0,
    NOTEPADS_MARKDOWN_SVG_WORK_THREAD_STACK = 1,
    NOTEPADS_MARKDOWN_SVG_WORK_LIBRARY_ALLOWANCE = 2
};
typedef struct notepads_markdown_svg_work notepads_markdown_svg_work;
typedef struct NOTEPADS_MARKDOWN_FFI_ALIGN8 notepads_markdown_svg_work_token
{
    uint64_t account_nonce;
    uint32_t slot;
    uint32_t generation;
} notepads_markdown_svg_work_token;
typedef struct NOTEPADS_MARKDOWN_FFI_ALIGN8 notepads_markdown_svg_work_header
{
    uint32_t abi_version;
    uint32_t struct_size;
} notepads_markdown_svg_work_header;
typedef struct NOTEPADS_MARKDOWN_FFI_ALIGN8 notepads_markdown_svg_work_snapshot
{
    notepads_markdown_svg_work_header header;
    uint64_t account_nonce;
    uint64_t job_bytes;
    uint64_t job_peak;
    uint64_t root_work_bytes;
    uint64_t control_bytes;
    uint64_t header_bytes;
    uint64_t arena_bytes;
    uint32_t tickets;
    uint32_t pending;
    uint32_t closed;
    uint32_t reserved;
} notepads_markdown_svg_work_snapshot;

#ifdef __cplusplus
static_assert(sizeof(notepads_markdown_svg_work_token) == 16 && alignof(notepads_markdown_svg_work_token) == 8);
static_assert(sizeof(notepads_markdown_svg_work_snapshot) == 80 && alignof(notepads_markdown_svg_work_snapshot) == 8);
static_assert(offsetof(notepads_markdown_svg_work_snapshot, tickets) == 64);
extern "C"
{
#endif
    uint32_t NOTEPADS_MARKDOWN_FFI_CALL notepads_markdown_svg_work_abi_version(void);
    int32_t NOTEPADS_MARKDOWN_FFI_CALL notepads_markdown_svg_work_create(uint32_t abi_version,
        const notepads_markdown_image_request* request, const notepads_markdown_core_model* model,
        const notepads_markdown_core_cancel* cancel, notepads_markdown_svg_work** output, notepads_markdown_core_error* error);
    // Reserve before allocating. A failed allocation may release its pending
    // token directly. Tokens are account-specific; outputs are unchanged on error.
    int32_t NOTEPADS_MARKDOWN_FFI_CALL notepads_markdown_svg_work_reserve(notepads_markdown_svg_work* account, uint32_t kind,
        uint64_t bytes, notepads_markdown_svg_work_token* output, notepads_markdown_core_error* error);
    // Exactly once after allocation. Failure retains the original reservation;
    // free the failed allocation before release. Do not publish it after failure.
    int32_t NOTEPADS_MARKDOWN_FFI_CALL notepads_markdown_svg_work_admit(notepads_markdown_svg_work* account,
        const notepads_markdown_svg_work_token* token, uint64_t actual, notepads_markdown_core_error* error);
    // Caller guarantees the payload has actually been freed, or allocation never
    // happened. Failed free must keep its token/owner/account and all charges.
    int32_t NOTEPADS_MARKDOWN_FFI_CALL notepads_markdown_svg_work_release_after_free(notepads_markdown_svg_work* account,
        const notepads_markdown_svg_work_token* token, notepads_markdown_core_error* error);
    int32_t NOTEPADS_MARKDOWN_FFI_CALL notepads_markdown_svg_work_read(notepads_markdown_svg_work* account,
        notepads_markdown_svg_work_snapshot* output, notepads_markdown_core_error* error);
    // Close only forbids new reserves; it neither frees payloads nor clears fees.
    int32_t NOTEPADS_MARKDOWN_FFI_CALL notepads_markdown_svg_work_close(notepads_markdown_svg_work* account, notepads_markdown_core_error* error);
    // Requires closed and no pending/live tokens. Failure preserves the canonical
    // handle. Success frees arena then Owned then returns their matching fees.
    // Null succeeds. Otherwise call exactly once after all account calls finish.
    int32_t NOTEPADS_MARKDOWN_FFI_CALL notepads_markdown_svg_work_destroy(notepads_markdown_svg_work* account, notepads_markdown_core_error* error);
#ifdef __cplusplus
}
#endif
