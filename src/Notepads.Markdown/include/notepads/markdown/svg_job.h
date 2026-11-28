// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE file in the project root for license information.
// ---------------------------------------------------------------------------------------------
#pragma once
#include "notepads/markdown/svg_resources.h"
#include "notepads/markdown/svg_fonts.h"
#include "notepads/markdown/svg_coverage.h"

// Private one-shot renderer ABI1. Create/prepare/target/run/finish use exactly
// the creating worker. Inputs borrow only through the call; the model/encoded
// source, resources, Native font session and original request are task-pinned.
// Cancel uses the original request Core token in a separate sibling handle;
// it never borrows mutable engine data or a work-account operation.
// Retain under the Native task gate before publishing a cancel call lease.
// Revoke that entry gate before close_cancel; join the real worker, then wait
// for all retained calls before destroy. No arbitrary raw pointer may enter
// retain after gate revocation. A refcount alone does not prove caller entry.
// run returns only a candidate. Publish only after Rust+Native cleanup success,
// real thread signaled, revoked entry/zero cancel leases, destroy success and
// final source/provider/font/target identity validation. Keep the original two
// physical ImageLoadTask slots; failed retirement permanently quarantines one.
// No Rust container or Windows object crosses this ABI. Output PODs require a
// matching readable header; numeric overflow/known overlap rejected first.
// Job errors use Core transport codes plus explicit job_error_kind; Geometry
// has its own diagnostic kind and is never silently scaled/clipped to pass.

enum { NOTEPADS_MARKDOWN_SVG_JOB_ABI_VERSION=1 };
enum notepads_markdown_svg_job_error_kind { NOTEPADS_MARKDOWN_SVG_JOB_ERROR_NONE=0, NOTEPADS_MARKDOWN_SVG_JOB_ERROR_STATE=1,
    NOTEPADS_MARKDOWN_SVG_JOB_ERROR_ARGUMENT=2, NOTEPADS_MARKDOWN_SVG_JOB_ERROR_GEOMETRY=3,
    NOTEPADS_MARKDOWN_SVG_JOB_ERROR_RESOURCE=4, NOTEPADS_MARKDOWN_SVG_JOB_ERROR_PANIC=5 };
typedef struct notepads_markdown_svg_job notepads_markdown_svg_job;
typedef struct notepads_markdown_svg_job_cancel notepads_markdown_svg_job_cancel;
typedef struct NOTEPADS_MARKDOWN_FFI_ALIGN8 notepads_markdown_svg_job_header { uint32_t abi_version, struct_size; } notepads_markdown_svg_job_header;
typedef struct NOTEPADS_MARKDOWN_FFI_ALIGN8 notepads_markdown_svg_job_identity
{
    uint64_t job_nonce, source_generation, provider_generation, font_generation, target_generation;
} notepads_markdown_svg_job_identity;
typedef struct NOTEPADS_MARKDOWN_FFI_ALIGN8 notepads_markdown_svg_job_error
{
    notepads_markdown_svg_job_header header;
    int32_t transport_status;
    uint32_t job_error_kind;
    notepads_markdown_svg_fault first_fault;
} notepads_markdown_svg_job_error;
typedef struct NOTEPADS_MARKDOWN_FFI_ALIGN8 notepads_markdown_svg_job_natural
{
    notepads_markdown_svg_job_header header;
    notepads_markdown_svg_job_identity identity;
    double width_dip, height_dip;
    uint32_t viewport_width_bits, viewport_height_bits;
} notepads_markdown_svg_job_natural;
typedef struct NOTEPADS_MARKDOWN_FFI_ALIGN8 notepads_markdown_svg_job_target
{
    notepads_markdown_svg_job_header header;
    double display_width_dip, effective_physical_scale;
    uint64_t generation;
} notepads_markdown_svg_job_target;
typedef struct NOTEPADS_MARKDOWN_FFI_ALIGN8 notepads_markdown_svg_job_plan
{
    notepads_markdown_svg_job_header header;
    notepads_markdown_svg_job_identity identity;
    double natural_width_dip, natural_height_dip;
    double requested_bucket, ideal_scale, extent_width, extent_height;
    uint32_t viewport_width_bits, viewport_height_bits, scale_bits, target_bucket;
    uint32_t width, height, stride, resolution_limited;
    uint64_t capacity;
} notepads_markdown_svg_job_plan;
typedef struct NOTEPADS_MARKDOWN_FFI_ALIGN8 notepads_markdown_svg_job_retirement
{
    notepads_markdown_svg_job_header header;
    notepads_markdown_svg_job_identity identity;
    uint32_t terminal_state, cleanup_succeeded, completed_stages, job_error_kind;
    notepads_markdown_svg_fault first_fault;
    uint64_t remaining_owners, failed_releases;
    uint32_t cancelled, reserved;
} notepads_markdown_svg_job_retirement;
typedef struct NOTEPADS_MARKDOWN_FFI_ALIGN8 notepads_markdown_svg_job_destroyed
{
    notepads_markdown_svg_job_header header;
    notepads_markdown_svg_job_identity identity;
    uint32_t consumed, cleanup_succeeded, cancelled, reserved;
    notepads_markdown_svg_fault first_fault;
    notepads_markdown_svg_work_token unreleased_shell, unreleased_engine;
} notepads_markdown_svg_job_destroyed;
#ifdef __cplusplus
static_assert(sizeof(notepads_markdown_svg_job_identity)==40 && sizeof(notepads_markdown_svg_job_error)==32);
static_assert(sizeof(notepads_markdown_svg_job_natural)==72 && sizeof(notepads_markdown_svg_job_target)==32);
static_assert(sizeof(notepads_markdown_svg_job_plan)==136 && sizeof(notepads_markdown_svg_job_retirement)==104);
static_assert(sizeof(notepads_markdown_svg_job_destroyed)==112);
extern "C" {
#endif
    uint32_t NOTEPADS_MARKDOWN_FFI_CALL notepads_markdown_svg_job_abi_version(void);
    // Native initializes both handle outputs to null before every call.
    // identity.job_nonce is the original work snapshot.account_nonce (capture
    // before resources_create); source/provider must match that original epoch.
    // Font/target generations are nonzero captured Native generations.
    // Font POD/table remain alive in the Native session. Outputs unchanged on
    // preflight failure. A semantic initialization failure may still return a
    // nonnull owned job/cancel pair: always finish/retire a returned job even
    // when this call's status is nonzero. No pointer means no published owner. A successful create is the only job in the original account/request
    // lifetime; resources reclaim/recreate never resets that qualification. Reserve exact fixed metadata before allocation, admit afterward.
    // Coverage is copied during creation and shares the font session and account.
    int32_t NOTEPADS_MARKDOWN_FFI_CALL notepads_markdown_svg_job_create(uint32_t abi_version,
        notepads_markdown_svg_resources* resources, const MsSvgFontCallbacks* fonts,
        const MsSvgCoverageCallbacks* coverage,
        const notepads_markdown_svg_job_identity* identity, notepads_markdown_svg_job** output,
        notepads_markdown_svg_job_cancel** cancel, notepads_markdown_svg_job_error* error);
    int32_t NOTEPADS_MARKDOWN_FFI_CALL notepads_markdown_svg_job_prepare(notepads_markdown_svg_job* job, const uint8_t* source,
        uint64_t length, notepads_markdown_svg_job_natural* output, notepads_markdown_svg_job_error* error);
    int32_t NOTEPADS_MARKDOWN_FFI_CALL notepads_markdown_svg_job_set_target(notepads_markdown_svg_job* job,
        const notepads_markdown_svg_job_target* target, notepads_markdown_svg_job_plan* output, notepads_markdown_svg_job_error* error);
    // Exact frozen identity/stride/capacity; no final pixel allocation or copy.
    // The PBGRA buffer is unpublished on any failure, including partial writes.
    // sourceRect is exactly (0,0,extent_width,extent_height), never full ceil box.
    int32_t NOTEPADS_MARKDOWN_FFI_CALL notepads_markdown_svg_job_run(notepads_markdown_svg_job* job,
        const notepads_markdown_svg_job_identity* identity, uint8_t* pbgra, uint64_t capacity,
        uint32_t stride, notepads_markdown_svg_job_error* error);
    // Valid receipt transport may report cleanup_succeeded=0. Never retry finish
    // or publish a failed receipt; already retired objects cannot be dropped twice.
    int32_t NOTEPADS_MARKDOWN_FFI_CALL notepads_markdown_svg_job_finish(notepads_markdown_svg_job* job,
        notepads_markdown_svg_job_retirement* output, notepads_markdown_svg_job_error* error);
    // One creator reference is owned by the canonical task. These methods act
    // solely on the cancel sibling and are safe with a pinned live work call.
    int32_t NOTEPADS_MARKDOWN_FFI_CALL notepads_markdown_svg_job_cancel_retain(notepads_markdown_svg_job_cancel* cancel);
    int32_t NOTEPADS_MARKDOWN_FFI_CALL notepads_markdown_svg_job_cancel_request(notepads_markdown_svg_job_cancel* cancel);
    int32_t NOTEPADS_MARKDOWN_FFI_CALL notepads_markdown_svg_job_cancel_release(notepads_markdown_svg_job_cancel* cancel);
    int32_t NOTEPADS_MARKDOWN_FFI_CALL notepads_markdown_svg_job_close_cancel(notepads_markdown_svg_job* job);
    // Reaper only: real worker joined, Native font capability retired, entry
    // gate revoked and cancel refs==1. Precommit failure preserves owner/output.
    // Commit nulls *job and writes consumed/failed receipt before any free.
    // Postcommit failure keeps canonical resources/task/request/slot and fees
    // permanently quarantined. Inspect consumed even when status reports panic.
    int32_t NOTEPADS_MARKDOWN_FFI_CALL notepads_markdown_svg_job_destroy(notepads_markdown_svg_job** job,
        notepads_markdown_svg_job_destroyed* output, notepads_markdown_svg_job_error* error);
#ifdef __cplusplus
}
#endif
