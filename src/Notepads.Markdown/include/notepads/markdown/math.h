// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE file in the project root for license information.
// ---------------------------------------------------------------------------------------------

#pragma once
#include "notepads/markdown/core.h"

// Private Math ABI2. Runtime is module RAII-owned, never an ambient
// static. All controls in that module share its one process authority.
// Native fixed runtime/control wrappers are each <=256 requested bytes, including
// allocation prefix/alignment, within the Core policy's prepaid header allowance.
// Native job fixed wrappers are each <=4096 requested bytes and are seeded in
// the same real job Metadata family. No application budget/profile input exists.
//
// Pointers are live/aligned/nonoverlapping by caller obligation. Math output PODs
// require only an initialized readable 8-byte ABI2/size prefix before preflight.
// Error PODs use unchanged Core ABI3. Success PODs remain unchanged on failure;
// create/prepare clear their handle output after validating its writable storage.
// No unwind crosses this ABI. A panic permits only cancellation and release.
//
// prepare captures the exact immutable model/object and a fixed <=5s deadline;
// queued metadata and a resident permit (128/control, 256/process, all states)
// are acquired before allocating the context. activate acquires its true
// 64MiB/actual-job from that same captured model. Original queued metadata stays
// funded through actual context free; the active job deliberately double counts
// its seeded metadata. The resident permit is returned after actual context free.
// Native Start must claim exactly once before any node/thread allocation; a failed
// creation consumes that claim. Release and reprepare instead of retrying this job.
// run has one exclusive mutable job-field borrow. Only cancel, state and retained
// references may overlap it; cancel touches only the disjoint atomic flag.
//
// CRITICAL: after run starts, summary/read/charge/finalize and final release are
// permitted only after the private Windows owner has observed the actual thread
// handle signaled. Computed state is an additional check, not proof of exit.
// Native parent/reaper retains a reference through signaled; a running last
// release is rejected and cannot substitute for reaping. headroom is supplied
// only by the trusted native measurement at the actual worker entry. The builtin
// profile is compiled for the three audited MSVC RIDs; other targets remain
// Unknown. The trusted native measurement must meet the fixed 1.5MiB total
// threshold. Applications cannot provide a profile or audited requirement.
//
// charge_native precedes each real native allocation; cumulative charges include
// failed allocations and old/new copies. finalize_native receives all still-live
// native requested capacity, including context/wrappers, after Rust copies and
// construction temporary buffers have been freed. Failure retains full funding.
// Native allocation prefixes retain this context; first free actual allocation,
// then release that reference. Final context release deallocates its own header
// before dropping its source/policy leases. A retired context never grants Ready.
//
// retire_metadata requires exclusive access with no concurrent retain/authority
// creation. Queued/Activated require proof no thread was created; Computed/
// FinishFailed require the actual handle signaled. Only the original Node's
// one FFI reference may remain. All native allocations/weak owners must really
// be freed first; a failed native HeapFree keeps its extra context reference,
// refuses retirement and retains funding. MetadataOnly preserves original
// queued metadata/identity/deadline/policy/resident, drops full worker payload and
// model pins, and permits only state/cancel/retain/release. Repeated retirement,
// Running, Finalized and Poisoned refuse. C++ Node refs share one FFI reference.

typedef struct notepads_markdown_math_runtime notepads_markdown_math_runtime;
typedef struct notepads_markdown_math_control notepads_markdown_math_control;
typedef struct notepads_markdown_math_job notepads_markdown_math_job;

enum { NOTEPADS_MARKDOWN_MATH_ABI_VERSION = 2, NOTEPADS_MARKDOWN_MATH_MAX_BATCH = 256,
    NOTEPADS_MARKDOWN_MATH_NATIVE_POLICY_HEADER_BYTES = 256, NOTEPADS_MARKDOWN_MATH_NATIVE_JOB_HEADER_BYTES = 4096 };
enum notepads_markdown_math_status { NOTEPADS_MARKDOWN_MATH_STACK_UNAVAILABLE = 100, NOTEPADS_MARKDOWN_MATH_INVALID_GEOMETRY = 101,
    NOTEPADS_MARKDOWN_MATH_INVALID_FUNDING = 102, NOTEPADS_MARKDOWN_MATH_INVALID_LIMITS = 103,
    NOTEPADS_MARKDOWN_MATH_PARSE_FAILURE = 104, NOTEPADS_MARKDOWN_MATH_MISSING_FONT = 105,
    NOTEPADS_MARKDOWN_MATH_MISSING_GLYPH = 106, NOTEPADS_MARKDOWN_MATH_INVALID_FONT = 107 };
enum notepads_markdown_math_job_state { NOTEPADS_MARKDOWN_MATH_QUEUED = 1, NOTEPADS_MARKDOWN_MATH_ACTIVATED = 2,
    NOTEPADS_MARKDOWN_MATH_RUNNING = 3, NOTEPADS_MARKDOWN_MATH_COMPUTED = 4, NOTEPADS_MARKDOWN_MATH_FINALIZED = 5,
    NOTEPADS_MARKDOWN_MATH_FINISH_FAILED = 6, NOTEPADS_MARKDOWN_MATH_POISONED = 7, NOTEPADS_MARKDOWN_MATH_METADATA_ONLY = 8 };
enum notepads_markdown_math_table { NOTEPADS_MARKDOWN_MATH_COMMANDS = 1, NOTEPADS_MARKDOWN_MATH_PRIMITIVES = 2 };
enum notepads_markdown_math_path_kind { NOTEPADS_MARKDOWN_MATH_MOVE = 1, NOTEPADS_MARKDOWN_MATH_LINE = 2,
    NOTEPADS_MARKDOWN_MATH_QUAD = 3, NOTEPADS_MARKDOWN_MATH_CUBIC = 4, NOTEPADS_MARKDOWN_MATH_CLOSE = 5 };
enum notepads_markdown_math_native_family { NOTEPADS_MARKDOWN_MATH_NATIVE_PATH = 1,
    NOTEPADS_MARKDOWN_MATH_NATIVE_MIRROR = 2, NOTEPADS_MARKDOWN_MATH_NATIVE_METADATA = 3 };
enum notepads_markdown_math_primitive_flags { NOTEPADS_MARKDOWN_MATH_FILL = 1, NOTEPADS_MARKDOWN_MATH_THEME_FOREGROUND = 2 };

typedef struct NOTEPADS_MARKDOWN_FFI_ALIGN8 notepads_markdown_math_usage_row
{
    uint64_t bytes;
    uint64_t retained_bytes;
    uint64_t metadata_bytes;
    uint32_t jobs;
    uint32_t residents;
} notepads_markdown_math_usage_row;
typedef struct NOTEPADS_MARKDOWN_FFI_ALIGN8 notepads_markdown_math_usage
{
    notepads_markdown_core_header header;
    notepads_markdown_math_usage_row process;
    notepads_markdown_math_usage_row control;
} notepads_markdown_math_usage;
// Commands and ink use em units, y down from the DisplayList top-left origin;
// the producer baseline is ascent. The native baseline-relative geometry DTO
// adapter subtracts ascent from every used y and both vertical ink bounds.
// Advance/ascent/descent and x coordinates are unchanged by that translation.
typedef struct NOTEPADS_MARKDOWN_FFI_ALIGN8 notepads_markdown_math_metrics
{
    double advance;
    double ascent;
    double descent;
    double left;
    double top;
    double right;
    double bottom;
} notepads_markdown_math_metrics;
typedef struct NOTEPADS_MARKDOWN_FFI_ALIGN8 notepads_markdown_math_summary
{
    notepads_markdown_core_header header;
    notepads_markdown_core_identity identity;
    uint32_t object;
    uint32_t state;
    notepads_markdown_math_metrics metrics;
    uint64_t commands;
    uint64_t primitives;
    uint64_t prepaid_metadata_bytes;
    uint64_t computed_rust_bytes;
    uint64_t reserved_bytes;
    uint64_t cumulative_bytes[11];
    uint64_t cumulative_slots[11];
} notepads_markdown_math_summary;
typedef struct NOTEPADS_MARKDOWN_FFI_ALIGN8 notepads_markdown_math_command
{
    uint32_t kind;
    uint32_t reserved;
    double x1;
    double y1;
    double x2;
    double y2;
    double x3;
    double y3;
} notepads_markdown_math_command;
// Private synchronous DirectWrite bridge. Outlines are in em, y down
// relative to the run baseline. A null command checks cancellation.
enum notepads_markdown_math_text_style {
    NOTEPADS_MARKDOWN_MATH_TEXT_SERIF = 0, NOTEPADS_MARKDOWN_MATH_TEXT_SANS = 1, NOTEPADS_MARKDOWN_MATH_TEXT_MONOSPACE = 2,
    NOTEPADS_MARKDOWN_MATH_TEXT_BOLD = 256, NOTEPADS_MARKDOWN_MATH_TEXT_ITALIC = 512
};
enum { NOTEPADS_MARKDOWN_MATH_TEXT_MAX_UNITS = 1024, NOTEPADS_MARKDOWN_MATH_TEXT_MAX_GLYPHS = 4096 };
typedef struct NOTEPADS_MARKDOWN_FFI_ALIGN8 notepads_markdown_math_text_metrics
{
    double advance;
    double ascent;
    double descent;
} notepads_markdown_math_text_metrics;
typedef int32_t (NOTEPADS_MARKDOWN_FFI_CALL *notepads_markdown_math_text_write)(void* sink, const notepads_markdown_math_command* command);
typedef int32_t (NOTEPADS_MARKDOWN_FFI_CALL *notepads_markdown_math_text_shape)(const uint16_t* utf16, uint32_t length,
    uint32_t style, void* sink, notepads_markdown_math_text_write write, notepads_markdown_math_text_metrics* metrics);
typedef struct NOTEPADS_MARKDOWN_FFI_ALIGN8 notepads_markdown_math_primitive
{
    uint32_t command_start;
    uint32_t command_count;
    uint32_t flags;
    uint32_t reserved;
    float r;
    float g;
    float b;
    float a;
    double stroke_width;
} notepads_markdown_math_primitive;
typedef struct NOTEPADS_MARKDOWN_FFI_ALIGN8 notepads_markdown_math_read_result
{
    notepads_markdown_core_header header;
    uint64_t written;
    uint64_t next;
    uint32_t done;
    uint32_t reserved;
} notepads_markdown_math_read_result;

#ifdef __cplusplus
static_assert(sizeof(notepads_markdown_math_usage_row) == 32 && sizeof(notepads_markdown_math_usage) == 72);
static_assert(sizeof(notepads_markdown_math_metrics) == 56 && sizeof(notepads_markdown_math_summary) == 328);
static_assert(sizeof(notepads_markdown_math_command) == 56 && sizeof(notepads_markdown_math_primitive) == 40);
static_assert(sizeof(notepads_markdown_math_read_result) == 32);
extern "C"
{
#endif
    uint32_t NOTEPADS_MARKDOWN_FFI_CALL notepads_markdown_math_abi_version(void);
    int32_t NOTEPADS_MARKDOWN_FFI_CALL notepads_markdown_math_runtime_create(notepads_markdown_math_runtime** output, notepads_markdown_core_error* error);
    int32_t NOTEPADS_MARKDOWN_FFI_CALL notepads_markdown_math_runtime_retain(notepads_markdown_math_runtime* runtime, notepads_markdown_core_error* error);
    int32_t NOTEPADS_MARKDOWN_FFI_CALL notepads_markdown_math_runtime_release(notepads_markdown_math_runtime* runtime, notepads_markdown_core_error* error);
    int32_t NOTEPADS_MARKDOWN_FFI_CALL notepads_markdown_math_runtime_usage(notepads_markdown_math_runtime* runtime, notepads_markdown_math_usage* output, notepads_markdown_core_error* error);
    int32_t NOTEPADS_MARKDOWN_FFI_CALL notepads_markdown_math_control_create(notepads_markdown_math_runtime* runtime, notepads_markdown_math_control** output, notepads_markdown_core_error* error);
    int32_t NOTEPADS_MARKDOWN_FFI_CALL notepads_markdown_math_control_retain(notepads_markdown_math_control* control, notepads_markdown_core_error* error);
    int32_t NOTEPADS_MARKDOWN_FFI_CALL notepads_markdown_math_control_release(notepads_markdown_math_control* control, notepads_markdown_core_error* error);
    int32_t NOTEPADS_MARKDOWN_FFI_CALL notepads_markdown_math_control_usage(notepads_markdown_math_control* control, notepads_markdown_math_usage* output, notepads_markdown_core_error* error);
    // object is the 1-based object ID from the same model's derived descriptor.
    int32_t NOTEPADS_MARKDOWN_FFI_CALL notepads_markdown_math_job_prepare(notepads_markdown_math_control* control, const notepads_markdown_core_model* model,
        uint32_t object, notepads_markdown_math_job** output, notepads_markdown_core_error* error);
    int32_t NOTEPADS_MARKDOWN_FFI_CALL notepads_markdown_math_job_activate(notepads_markdown_math_job* job, notepads_markdown_core_error* error);
    int32_t NOTEPADS_MARKDOWN_FFI_CALL notepads_markdown_math_job_claim_start(notepads_markdown_math_job* job, notepads_markdown_core_error* error);
    int32_t NOTEPADS_MARKDOWN_FFI_CALL notepads_markdown_math_job_retain(notepads_markdown_math_job* job, notepads_markdown_core_error* error);
    int32_t NOTEPADS_MARKDOWN_FFI_CALL notepads_markdown_math_job_release(notepads_markdown_math_job* job, notepads_markdown_core_error* error);
    int32_t NOTEPADS_MARKDOWN_FFI_CALL notepads_markdown_math_job_cancel(notepads_markdown_math_job* job, notepads_markdown_core_error* error);
    uint32_t NOTEPADS_MARKDOWN_FFI_CALL notepads_markdown_math_job_state(const notepads_markdown_math_job* job);
    int32_t NOTEPADS_MARKDOWN_FFI_CALL notepads_markdown_math_job_run(notepads_markdown_math_job* job, uint64_t measured_headroom, notepads_markdown_core_error* error);
    int32_t NOTEPADS_MARKDOWN_FFI_CALL notepads_markdown_math_job_run_with_text(notepads_markdown_math_job* job, uint64_t measured_headroom,
        notepads_markdown_math_text_shape shape, notepads_markdown_core_error* error);
    int32_t NOTEPADS_MARKDOWN_FFI_CALL notepads_markdown_math_job_summary(notepads_markdown_math_job* job, notepads_markdown_math_summary* output, notepads_markdown_core_error* error);
    int32_t NOTEPADS_MARKDOWN_FFI_CALL notepads_markdown_math_job_read(notepads_markdown_math_job* job, uint32_t table,
        uint64_t start, uint64_t count, uint32_t row_size, void* buffer,
        notepads_markdown_math_read_result* output, notepads_markdown_core_error* error);
    int32_t NOTEPADS_MARKDOWN_FFI_CALL notepads_markdown_math_job_charge_native(notepads_markdown_math_job* job, uint32_t family,
        uint64_t bytes, uint64_t slots, notepads_markdown_core_error* error);
    int32_t NOTEPADS_MARKDOWN_FFI_CALL notepads_markdown_math_job_finalize_native(notepads_markdown_math_job* job,
        uint64_t actual_native_bytes, notepads_markdown_core_error* error);
    int32_t NOTEPADS_MARKDOWN_FFI_CALL notepads_markdown_math_job_retire_metadata(notepads_markdown_math_job* job, notepads_markdown_core_error* error);
#ifdef __cplusplus
}
#endif
