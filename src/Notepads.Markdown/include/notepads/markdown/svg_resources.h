// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE file in the project root for license information.
// ---------------------------------------------------------------------------------------------
#pragma once
#include "notepads/markdown/svg_retained.h"

// Private owned resources ABI1. Create before Native font-session allocation.
// Successful create consumes *account (sets null). The original request remains
// owned by the canonical Native task. Native and engine use this same account.
// Each operation borrows only this capability, with internal serialization;
// no account borrow survives a call or spans a Native callback.
// The caller pins the resources handle through every call/callback. Reclaim is
// exclusive, after all calls, engine/shell/cancel owners and font objects retire.
// Before commit, failure preserves handle and output. Commit nulls *resources
// and publishes a consumed/failed receipt BEFORE any free. The receipt remains
// authoritative even on panic. Success returns a usable original account.
// Post-commit cleanup failure consumes the resources handle permanently and
// returns only a quarantine account pin: keep the original task/request/slot,
// fees and receipt; never reuse, destroy, or retry that quarantined account.
// No raw original account may be used between create and successful reclaim.
enum { NOTEPADS_MARKDOWN_SVG_RESOURCES_ABI_VERSION = 1 };
enum notepads_markdown_svg_resource_kind { NOTEPADS_MARKDOWN_SVG_RESOURCE_ALLOCATION=0, NOTEPADS_MARKDOWN_SVG_RESOURCE_FONT=1,
    NOTEPADS_MARKDOWN_SVG_RESOURCE_METRICS=2, NOTEPADS_MARKDOWN_SVG_RESOURCE_GEOMETRY=3, NOTEPADS_MARKDOWN_SVG_RESOURCE_TREE=4,
    NOTEPADS_MARKDOWN_SVG_RESOURCE_PIXELS=5, NOTEPADS_MARKDOWN_SVG_RESOURCE_SCRATCH=6, NOTEPADS_MARKDOWN_SVG_RESOURCE_THREAD_STACK=7,
    NOTEPADS_MARKDOWN_SVG_RESOURCE_LIBRARY_ALLOWANCE=8 };
typedef struct notepads_markdown_svg_resources notepads_markdown_svg_resources;
typedef struct NOTEPADS_MARKDOWN_FFI_ALIGN8 notepads_markdown_svg_fault
{
    uint32_t kind, limit;
    int32_t native_hr;
    uint32_t native_hr_present;
} notepads_markdown_svg_fault;
typedef struct NOTEPADS_MARKDOWN_FFI_ALIGN8 notepads_markdown_svg_resources_retirement
{
    notepads_markdown_svg_work_header header;
    uint32_t consumed, cleanup_succeeded;
    notepads_markdown_svg_work* account;
#ifndef _WIN64
    uint32_t pointer_padding;
#endif
    notepads_markdown_svg_fault first_fault;
    notepads_markdown_svg_work_token unreleased_header;
    notepads_markdown_svg_work_token unreleased_account;
} notepads_markdown_svg_resources_retirement;
#ifdef __cplusplus
static_assert(sizeof(notepads_markdown_svg_fault)==16);
static_assert(sizeof(notepads_markdown_svg_resources_retirement) == 72 &&
    alignof(notepads_markdown_svg_resources_retirement) == 8 &&
    offsetof(notepads_markdown_svg_resources_retirement, first_fault) == 24);
extern "C" {
#endif
    uint32_t NOTEPADS_MARKDOWN_FFI_CALL notepads_markdown_svg_resources_abi_version(void);
    int32_t NOTEPADS_MARKDOWN_FFI_CALL notepads_markdown_svg_resources_create(uint32_t abi_version,
        notepads_markdown_svg_work** account, notepads_markdown_svg_resources** output, notepads_markdown_core_error* error);
    int32_t NOTEPADS_MARKDOWN_FFI_CALL notepads_markdown_svg_resources_reserve(notepads_markdown_svg_resources* resources,
        uint32_t kind, uint64_t bytes, notepads_markdown_svg_work_token* output, notepads_markdown_core_error* error);
    int32_t NOTEPADS_MARKDOWN_FFI_CALL notepads_markdown_svg_resources_admit(notepads_markdown_svg_resources* resources,
        const notepads_markdown_svg_work_token* token, uint64_t actual, notepads_markdown_core_error* error);
    int32_t NOTEPADS_MARKDOWN_FFI_CALL notepads_markdown_svg_resources_release_after_free(notepads_markdown_svg_resources* resources,
        const notepads_markdown_svg_work_token* token, notepads_markdown_core_error* error);
    int32_t NOTEPADS_MARKDOWN_FFI_CALL notepads_markdown_svg_resources_checkpoint(notepads_markdown_svg_resources* resources,
        uint64_t work, notepads_markdown_core_error* error);
    // First nonzero typed record wins. Does not allocate, reset, or refund.
    int32_t NOTEPADS_MARKDOWN_FFI_CALL notepads_markdown_svg_resources_record_fault(notepads_markdown_svg_resources* resources,
        const notepads_markdown_svg_fault* fault, notepads_markdown_core_error* error);
    int32_t NOTEPADS_MARKDOWN_FFI_CALL notepads_markdown_svg_resources_read_fault(const notepads_markdown_svg_resources* resources,
        notepads_markdown_svg_fault* output, notepads_markdown_core_error* error);
    // Header-first diagnostic copy under the same original account transaction.
    // Available for cleanup diagnostics; does not expose the transferred handle
    // or authorize new work/publication. Header/alias rejection preserves output.
    int32_t NOTEPADS_MARKDOWN_FFI_CALL notepads_markdown_svg_resources_read_work(const notepads_markdown_svg_resources* resources,
        notepads_markdown_svg_work_snapshot* output, notepads_markdown_core_error* error);
    int32_t NOTEPADS_MARKDOWN_FFI_CALL notepads_markdown_svg_resources_retained_prepare(uint32_t abi_version,
        notepads_markdown_svg_resources* resources, uint32_t kind, const notepads_markdown_image_bytes* image,
        notepads_markdown_preview_allocation** root_allocation, uint64_t total, uint64_t payload,
        notepads_markdown_svg_retained** output, notepads_markdown_svg_work_token* initial, notepads_markdown_core_error* error);
    int32_t NOTEPADS_MARKDOWN_FFI_CALL notepads_markdown_svg_resources_borrow_retained(notepads_markdown_svg_resources* resources,
        notepads_markdown_svg_retained* retained, notepads_markdown_svg_work_token* output, notepads_markdown_core_error* error);
    int32_t NOTEPADS_MARKDOWN_FFI_CALL notepads_markdown_svg_resources_return_retained(notepads_markdown_svg_resources* resources,
        const notepads_markdown_svg_work_token* token, notepads_markdown_core_error* error);
    int32_t NOTEPADS_MARKDOWN_FFI_CALL notepads_markdown_svg_resources_reclaim(notepads_markdown_svg_resources** resources,
        notepads_markdown_svg_resources_retirement* output, notepads_markdown_core_error* error);
#ifdef __cplusplus
}
#endif
