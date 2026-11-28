// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE file in the project root for license information.
// ---------------------------------------------------------------------------------------------
#pragma once
#include "notepads/markdown/math.h"

// Private whole-job Diagram ABI1. MathControl is borrowed only to clone its
// existing Core Derived authority: both engines share process/control bytes,
// resident counts and the real 2/process, 1/control worker slots.
// No third-party AST, container, observer, budget or stack-profile input crosses
// this boundary. Raw COM capability is captured by native trusted code, never
// obtained from Markdown or application configuration.
//
// prepare reserves resident + 16KiB queued metadata before context/control
// initialization. Its single five-second deadline includes queue time.
// activate obtains the actual 64MiB job grant and 8MiB stack reservation charge.
// claim_start is one-shot before native node/thread allocation.
//
// CRITICAL: computed/read/charge/finalize/retire/final release require the native
// owner to observe the real thread handle signaled, or prove no thread existed.
// A computed flag does not prove exit. Only cancel/state/retain may overlap run.
// charge_native precedes each real allocation including failed/copy attempts;
// finalize frees the Rust candidate before retaining actual native capacity.
// Old Frames, native streams and COM callbacks retain their true owners.
// Late live Rust receipts refuse shrink/free. Caller must then preserve the
// node, grant and module pin; it must not discard a failed release obligation.
//
// ABI1 POD outputs require an initialized readable version/size prefix; errors
// use unchanged Core ABI3. Valid writable handle output is nulled on prepare
// failure. No panic/exception crosses this ABI. Poisoned contexts retain funding
// and permit only safe cancel/nonfinal release; never publish them as Ready.
typedef struct notepads_markdown_diagram_job notepads_markdown_diagram_job;
typedef struct notepads_markdown_diagram_coordinator_funding notepads_markdown_diagram_coordinator_funding;
enum { NOTEPADS_MARKDOWN_DIAGRAM_ABI_VERSION = 1, NOTEPADS_MARKDOWN_DIAGRAM_MAX_BATCH = 65536,
    NOTEPADS_MARKDOWN_DIAGRAM_QUEUED_METADATA_BYTES = 16384, NOTEPADS_MARKDOWN_DIAGRAM_NATIVE_NODE_BYTES = 4096,
    NOTEPADS_MARKDOWN_DIAGRAM_NATIVE_COORDINATOR_BYTES = 1024 };
enum notepads_markdown_diagram_job_state { NOTEPADS_MARKDOWN_DIAGRAM_QUEUED = 1, NOTEPADS_MARKDOWN_DIAGRAM_ACTIVATED = 2,
    NOTEPADS_MARKDOWN_DIAGRAM_RUNNING = 3, NOTEPADS_MARKDOWN_DIAGRAM_COMPUTED = 4, NOTEPADS_MARKDOWN_DIAGRAM_FINALIZED = 5,
    NOTEPADS_MARKDOWN_DIAGRAM_FINISH_FAILED = 6, NOTEPADS_MARKDOWN_DIAGRAM_POISONED = 7, NOTEPADS_MARKDOWN_DIAGRAM_METADATA_ONLY = 8 };

enum notepads_markdown_diagram_status
{
    NOTEPADS_MARKDOWN_DIAGRAM_STACK_UNAVAILABLE = 100,
    NOTEPADS_MARKDOWN_DIAGRAM_INVALID_INPUT = 104,
    NOTEPADS_MARKDOWN_DIAGRAM_FONT_UNAVAILABLE = 105
};

typedef struct NOTEPADS_MARKDOWN_FFI_ALIGN8 notepads_markdown_diagram_font_capability
{
    uint32_t version;
    uint32_t size;
    void *factory;
    void *collection;
    void *fallback;
    uint64_t font_epoch;
    uint16_t locale_name[85];
} notepads_markdown_diagram_font_capability;

typedef struct NOTEPADS_MARKDOWN_FFI_ALIGN8 notepads_markdown_diagram_style
{
    notepads_markdown_core_header header;
    uint32_t dark;
    uint32_t reserved;
    // Fixed RGBA channel order, independent of native integer byte order.
    uint8_t foreground[4];
    uint8_t background[4];
    uint8_t border[4];
    uint8_t panel[4];
    uint8_t link[4];
} notepads_markdown_diagram_style;

typedef struct NOTEPADS_MARKDOWN_FFI_ALIGN8 notepads_markdown_diagram_summary
{
    notepads_markdown_core_header header;
    notepads_markdown_core_identity identity;
    uint32_t object;
    uint32_t state;
    double width;
    double height;
    uint64_t bytes;
    uint64_t path_commands;
    uint64_t text_runs;
    uint64_t live_rust_bytes;
    uint64_t peak_requested_bytes;
    uint64_t native_charged_bytes;
    uint64_t work_units;
    uint64_t prepaid_metadata_bytes;
} notepads_markdown_diagram_summary;

typedef struct NOTEPADS_MARKDOWN_FFI_ALIGN8 notepads_markdown_diagram_read_result
{
    notepads_markdown_core_header header;
    uint64_t written;
    uint64_t next;
    uint32_t done;
    uint32_t reserved;
} notepads_markdown_diagram_read_result;

#ifdef __cplusplus
static_assert(sizeof(notepads_markdown_diagram_style) == 40 &&
    alignof(notepads_markdown_diagram_style) == 8);
static_assert(sizeof(notepads_markdown_diagram_summary) == 136 &&
    alignof(notepads_markdown_diagram_summary) == 8 &&
    offsetof(notepads_markdown_diagram_summary, width) == 56);
static_assert(sizeof(notepads_markdown_diagram_read_result) == 32 &&
    alignof(notepads_markdown_diagram_read_result) == 8 &&
    offsetof(notepads_markdown_diagram_read_result, done) == 24);
static_assert(sizeof(notepads_markdown_diagram_font_capability) == (sizeof(void *) == 8 ? 216 : 208) &&
    alignof(notepads_markdown_diagram_font_capability) == 8 &&
    offsetof(notepads_markdown_diagram_font_capability, font_epoch) == (sizeof(void *) == 8 ? 32 : 24) &&
    offsetof(notepads_markdown_diagram_font_capability, locale_name) == (sizeof(void *) == 8 ? 40 : 32));
extern "C" {
#endif
uint32_t NOTEPADS_MARKDOWN_FFI_CALL notepads_markdown_diagram_abi_version(void);
int32_t NOTEPADS_MARKDOWN_FFI_CALL notepads_markdown_diagram_coordinator_funding_prepare(notepads_markdown_math_control *control, notepads_markdown_core_model const *model,
    notepads_markdown_diagram_coordinator_funding **output, notepads_markdown_core_error *error);
int32_t NOTEPADS_MARKDOWN_FFI_CALL notepads_markdown_diagram_coordinator_funding_release(notepads_markdown_diagram_coordinator_funding *funding, notepads_markdown_core_error *error);
int32_t NOTEPADS_MARKDOWN_FFI_CALL notepads_markdown_diagram_job_prepare(notepads_markdown_math_control *control, notepads_markdown_core_model const *model, uint32_t object,
    notepads_markdown_diagram_job **output, notepads_markdown_core_error *error);
int32_t NOTEPADS_MARKDOWN_FFI_CALL notepads_markdown_diagram_job_activate(notepads_markdown_diagram_job *job, notepads_markdown_core_error *error);
int32_t NOTEPADS_MARKDOWN_FFI_CALL notepads_markdown_diagram_job_claim_start(notepads_markdown_diagram_job *job, notepads_markdown_core_error *error);
// Capability's original COM owners stay alive through real thread exit. The
// measured headroom belongs to this same ordinary worker activation. The
// builtin target profile is private; unknown profile returns StackUnavailable.
int32_t NOTEPADS_MARKDOWN_FFI_CALL notepads_markdown_diagram_job_run(notepads_markdown_diagram_job *job, notepads_markdown_diagram_font_capability const *fonts,
    notepads_markdown_diagram_style const *style, uint64_t measured_headroom, notepads_markdown_core_error *error);
int32_t NOTEPADS_MARKDOWN_FFI_CALL notepads_markdown_diagram_job_cancel(notepads_markdown_diagram_job *job, notepads_markdown_core_error *error);
uint32_t NOTEPADS_MARKDOWN_FFI_CALL notepads_markdown_diagram_job_state(notepads_markdown_diagram_job const *job);
int32_t NOTEPADS_MARKDOWN_FFI_CALL notepads_markdown_diagram_job_retain(notepads_markdown_diagram_job *job, notepads_markdown_core_error *error);
int32_t NOTEPADS_MARKDOWN_FFI_CALL notepads_markdown_diagram_job_release(notepads_markdown_diagram_job *job, notepads_markdown_core_error *error);
int32_t NOTEPADS_MARKDOWN_FFI_CALL notepads_markdown_diagram_job_retire_metadata(notepads_markdown_diagram_job *job, notepads_markdown_core_error *error);
int32_t NOTEPADS_MARKDOWN_FFI_CALL notepads_markdown_diagram_job_summary(notepads_markdown_diagram_job *job, notepads_markdown_diagram_summary *output, notepads_markdown_core_error *error);
int32_t NOTEPADS_MARKDOWN_FFI_CALL notepads_markdown_diagram_job_read(notepads_markdown_diagram_job *job, uint64_t start, uint8_t *buffer, uint64_t capacity,
    notepads_markdown_diagram_read_result *output, notepads_markdown_core_error *error);
int32_t NOTEPADS_MARKDOWN_FFI_CALL notepads_markdown_diagram_job_charge_native(notepads_markdown_diagram_job *job, uint64_t bytes, notepads_markdown_core_error *error);
int32_t NOTEPADS_MARKDOWN_FFI_CALL notepads_markdown_diagram_job_finalize_native(notepads_markdown_diagram_job *job, uint64_t actual_native_bytes, notepads_markdown_core_error *error);
#ifdef __cplusplus
}
#endif
