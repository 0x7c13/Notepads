// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE file in the project root for license information.
// ---------------------------------------------------------------------------------------------
#pragma once
#include "notepads/markdown/svg_work.h"
#include "notepads/markdown/preview.h"
enum { NOTEPADS_MARKDOWN_SVG_RETAINED_ABI_VERSION = 1 };
enum notepads_markdown_svg_retained_kind { NOTEPADS_MARKDOWN_SVG_RETAINED_PAYLOAD = 0, NOTEPADS_MARKDOWN_SVG_RETAINED_METADATA = 1 };
typedef struct notepads_markdown_svg_retained notepads_markdown_svg_retained;
typedef struct NOTEPADS_MARKDOWN_FFI_ALIGN8 notepads_markdown_svg_retained_snapshot
{
    notepads_markdown_svg_work_header header;
    uint64_t owner_nonce;
    uint64_t native_bytes;
    uint64_t preview_bytes;
    uint64_t header_bytes;
    uint64_t job_borrow_bytes;
    uint64_t refs;
    uint64_t root_live_bytes;
    uint64_t control_bytes;
    uint32_t phase;
    uint32_t kind;
} notepads_markdown_svg_retained_snapshot;
#ifdef __cplusplus
static_assert(sizeof(notepads_markdown_svg_retained_snapshot) == 80 && alignof(notepads_markdown_svg_retained_snapshot) == 8);
static_assert(offsetof(notepads_markdown_svg_retained_snapshot, phase) == 72);
extern "C"
{
#endif
    // P=requested_total; B=payload_capacity; P includes prefix/owner/alignment.
    // FFI computes L=Owned<PreviewAllocation> and H=Owned<Retained> exactly.
    // *root_allocation != null: existing admitted payload/root; adopt P+L.
    // *root_allocation == null: prepay P+L+H before creating the real root lease
    // and retained record. Returns that lease; Native must not reserve twice.
    // Payload requires image_bytes and B>0. Metadata requires null and B==0.
    // Error preserves original input lease, output null and no published token.
    int32_t NOTEPADS_MARKDOWN_FFI_CALL notepads_markdown_svg_retained_prepare(uint32_t abi_version,
        notepads_markdown_svg_work* account, uint32_t kind, const notepads_markdown_image_bytes* image_bytes,
        notepads_markdown_preview_allocation** root_allocation, uint64_t requested_total,
        uint64_t payload_capacity, notepads_markdown_svg_retained** output,
        notepads_markdown_svg_work_token* initial_borrow, notepads_markdown_core_error* error);
    // Unsafe physical event. Exactly once, immediately after successful HeapAlloc
    // and prefix installation, before any fallible operation. Existing adopted
    // allocation binds immediately after prepare's infallible prefix install.
    void NOTEPADS_MARKDOWN_FFI_CALL notepads_markdown_svg_retained_bind_physical(notepads_markdown_svg_retained* retained);
    // Validates physical binding, actual root admitted flag/reserved P, actual
    // image admission B (payload kind); no caller-supplied admitted boolean.
    int32_t NOTEPADS_MARKDOWN_FFI_CALL notepads_markdown_svg_retained_admit(notepads_markdown_svg_retained* retained,
        notepads_markdown_core_error* error);
    int32_t NOTEPADS_MARKDOWN_FFI_CALL notepads_markdown_svg_work_borrow_retained(notepads_markdown_svg_work* account,
        notepads_markdown_svg_retained* retained, notepads_markdown_svg_work_token* output, notepads_markdown_core_error* error);
    // Cleanup allowed after cancel/source retirement/account poison.
    int32_t NOTEPADS_MARKDOWN_FFI_CALL notepads_markdown_svg_work_return_retained(notepads_markdown_svg_work* account,
        const notepads_markdown_svg_work_token* token, notepads_markdown_core_error* error);
    // Keep a physical or borrow reference live throughout this read. The root
    // gauge is the captured model's actual ledger, not an independent policy root.
    // phase: 0 NeverAllocated, 1 Bound, 2 Admitted, 3 Retired.
    int32_t NOTEPADS_MARKDOWN_FFI_CALL notepads_markdown_svg_retained_read(const notepads_markdown_svg_retained* retained,
        notepads_markdown_svg_retained_snapshot* output, notepads_markdown_core_error* error);
    // Unsafe caller proof: HeapFree succeeded AND the original preview lease
    // was destroyed/deallocated. Must not dereference the old lease or prefix.
    // No-fail valid canonical cleanup; retires P+L, then physical ref; H only
    // after record's last ref and actual deallocation. Never call on failed free.
    // Diagnostic snapshot phase 4 means retirement is in progress or failed;
    // it forbids a new borrow. Only phase 3 proves refund completion. Layout
    // byte fields remain immutable sizes, including after successful retirement.
    // Checked physical event: OK only after phase3 refund commit and reference/
    // final record cleanup. Invalid phase or panic returns non-OK. The record
    // may be gone on return; no post-return read without an independent borrow.
    int32_t NOTEPADS_MARKDOWN_FFI_CALL notepads_markdown_svg_retained_after_physical_free_checked(notepads_markdown_svg_retained* retained);
    // Only NeverAllocated; creator borrow returned and preview lease destroyed.
    // Forbidden for adopted-existing/bound/admitted owner or failed HeapFree.
    void NOTEPADS_MARKDOWN_FFI_CALL notepads_markdown_svg_retained_abort_unallocated(notepads_markdown_svg_retained* retained);
#ifdef __cplusplus
}
#endif
