// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE file in the project root for license information.
// ---------------------------------------------------------------------------------------------

#pragma once
#include "notepads/markdown/ffi.h"

// Private native table extents; verified against the Core policy in FFI tests.
#define NOTEPADS_MARKDOWN_DERIVED_CONTROL_RESIDENT_LIMIT 128u
#define NOTEPADS_MARKDOWN_DERIVED_PROCESS_RESIDENT_LIMIT 256u

// Private model ABI.
// Caller pointers must be valid and aligned. Every input POD has an initialized
// readable 8-byte version/size prefix; only a matching header permits full reads.
// Output PODs need only that initialized prefix and full writable storage.
// Preflight rejection does not consume the job or write a success POD. Handle
// outputs are nulled once their own preflight succeeds. A matching error POD may
// receive the error; an invalid error header is never written.
// Prepare's submit_result is also a state diagnostic: semantic entry writes
// canonical NoState, or paid same-scope State even when the source/capture fails.
// source_accepted reports this call's successful source kernel; it can be 1 with
// has_ready=0 when capture fails. State alone never grants model permission.
// Other success PODs (including query/summary) retain their own unchanged-on-error
// rules. Failed create/clone/install publishes no handle.
// Pointer storage, input, buffer, handle and output/error regions must not overlap.
// Input bytes borrow until return; Rust owns snapshots and immutable model pins.
// Mutable session/job/stream/query calls are exclusive; model reads may be shared.
// Only cancel_request may run concurrently with work, on its separate live token.
// Destroy once, after every call on that same handle has ended. Null destroy is OK.
// Model, stream and query pins can outlive session/job. Create fresh same-root
// tokens from model_cancel_create after destroying session/canceling an old token.
// Panic poisons involved mutable handles; only destruction remains. Cancel token
// is independent. No unwind crosses the ABI. Large/control allocations here are
// fallible and root-accounted; existing small core Arc/global OOM may still abort.
// Install reserves/allocates/accounts its return handle before committing the
// model; no fallible step follows core install's last Acquire checkpoint.

typedef struct notepads_markdown_core_cancel notepads_markdown_core_cancel;
typedef struct notepads_markdown_core_session notepads_markdown_core_session;
typedef struct notepads_markdown_core_job notepads_markdown_core_job;
typedef struct notepads_markdown_core_model notepads_markdown_core_model;
typedef struct notepads_markdown_core_stream notepads_markdown_core_stream;
typedef struct notepads_markdown_core_query notepads_markdown_core_query;

enum { NOTEPADS_MARKDOWN_CORE_ABI_VERSION = 3, NOTEPADS_MARKDOWN_CORE_MAX_BATCH = 256 };
enum notepads_markdown_core_profile { NOTEPADS_MARKDOWN_CORE_C0 = 1, NOTEPADS_MARKDOWN_CORE_G3 = 2 };
enum notepads_markdown_core_extensions {
    NOTEPADS_MARKDOWN_CORE_EXTENSIONS_NONE = 0,
    NOTEPADS_MARKDOWN_CORE_EXTENSIONS_MATH = 1,
    NOTEPADS_MARKDOWN_CORE_EXTENSIONS_MERMAID = 2,
    NOTEPADS_MARKDOWN_CORE_EXTENSIONS_HTML_BLOCKS = 4,
    NOTEPADS_MARKDOWN_CORE_EXTENSIONS_HTML_INLINE = 8,
    NOTEPADS_MARKDOWN_CORE_EXTENSIONS_HEADING_IDS = 16,
    NOTEPADS_MARKDOWN_CORE_EXTENSIONS_FOOTNOTES = 32,
    NOTEPADS_MARKDOWN_CORE_EXTENSIONS_CALLOUTS = 64,
    NOTEPADS_MARKDOWN_CORE_EXTENSIONS_DESCRIPTION_LISTS = 128,
    NOTEPADS_MARKDOWN_CORE_EXTENSIONS_FRONT_MATTER = 0x100,
    NOTEPADS_MARKDOWN_CORE_EXTENSIONS_FRONT_MATTER_CODE = 0x300,
    NOTEPADS_MARKDOWN_CORE_EXTENSIONS_FRONT_MATTER_HIDE = 0x500,
    NOTEPADS_MARKDOWN_CORE_EXTENSIONS_HIGHLIGHT = 0x800,
    NOTEPADS_MARKDOWN_CORE_EXTENSIONS_INLINE_SCRIPTS = 0x1000,
    NOTEPADS_MARKDOWN_CORE_EXTENSIONS_EMOJI_SHORTCODES = 0x2000,
    NOTEPADS_MARKDOWN_CORE_EXTENSIONS_TABLE_OF_CONTENTS = 0x4000,
    NOTEPADS_MARKDOWN_CORE_EXTENSIONS_LOCAL_WIKI_LINKS = 0x8000,
    NOTEPADS_MARKDOWN_CORE_EXTENSIONS_NOTE_EMBEDS = 0x10000,
    NOTEPADS_MARKDOWN_CORE_EXTENSIONS_CITATIONS = 0x20000
};
enum notepads_markdown_core_status
{
    NOTEPADS_MARKDOWN_CORE_OK = 0, NOTEPADS_MARKDOWN_CORE_INVALID_ARGUMENT = 1, NOTEPADS_MARKDOWN_CORE_ABI_MISMATCH = 2,
    NOTEPADS_MARKDOWN_CORE_INVALID_UTF8 = 3, NOTEPADS_MARKDOWN_CORE_CANCELLED = 4, NOTEPADS_MARKDOWN_CORE_INVALID_STATE = 5,
    NOTEPADS_MARKDOWN_CORE_PANIC = 6, NOTEPADS_MARKDOWN_CORE_OUT_OF_MEMORY = 7, NOTEPADS_MARKDOWN_CORE_UNSUPPORTED = 8,
    NOTEPADS_MARKDOWN_CORE_RESOURCE_LIMIT = 9, NOTEPADS_MARKDOWN_CORE_STALE_RESULT = 10, NOTEPADS_MARKDOWN_CORE_RESYNC_REQUIRED = 11,
    NOTEPADS_MARKDOWN_CORE_WRONG_REVISION = 12, NOTEPADS_MARKDOWN_CORE_WRONG_DOCUMENT = 13, NOTEPADS_MARKDOWN_CORE_WRONG_BUDGET = 14,
    NOTEPADS_MARKDOWN_CORE_INVALID_BOUNDARY = 15, NOTEPADS_MARKDOWN_CORE_INVALID_RANGE = 16, NOTEPADS_MARKDOWN_CORE_UNKNOWN_ID = 17,
    NOTEPADS_MARKDOWN_CORE_OUTSIDE_SCOPE = 18, NOTEPADS_MARKDOWN_CORE_INVALID_PROVENANCE = 19,
    NOTEPADS_MARKDOWN_CORE_ARITHMETIC_OVERFLOW = 20, NOTEPADS_MARKDOWN_CORE_INVALID_EDITS = 21,
    NOTEPADS_MARKDOWN_CORE_INTERNAL_INVARIANT = 22, NOTEPADS_MARKDOWN_CORE_WRONG_GENERATION = 23,
    NOTEPADS_MARKDOWN_CORE_DEADLINE = 24, NOTEPADS_MARKDOWN_CORE_CLOCK_FAILURE = 25, NOTEPADS_MARKDOWN_CORE_INVALID_UTF16 = 26
};
enum notepads_markdown_core_limit_kind
{
    NOTEPADS_MARKDOWN_CORE_SOURCE_BYTES = 1, NOTEPADS_MARKDOWN_CORE_SOURCE_CAPACITY = 2, NOTEPADS_MARKDOWN_CORE_SOURCE_OWNERS = 3,
    NOTEPADS_MARKDOWN_CORE_PARSE_BYTES = 4, NOTEPADS_MARKDOWN_CORE_INPUT_PIECES = 5, NOTEPADS_MARKDOWN_CORE_EDITS = 6,
    NOTEPADS_MARKDOWN_CORE_INSERT_BYTES = 7, NOTEPADS_MARKDOWN_CORE_AUXILIARY_CAPACITY = 8, NOTEPADS_MARKDOWN_CORE_WORK = 9,
    NOTEPADS_MARKDOWN_CORE_NATIVE_RECORDS = 10, NOTEPADS_MARKDOWN_CORE_MODEL_RECORDS = 11, NOTEPADS_MARKDOWN_CORE_SOURCE_PIECES = 12,
    NOTEPADS_MARKDOWN_CORE_PROOF_RECORDS = 13, NOTEPADS_MARKDOWN_CORE_DEPTH = 14, NOTEPADS_MARKDOWN_CORE_DECODED_BYTES = 15,
    NOTEPADS_MARKDOWN_CORE_REFERENCE_LOOKUPS = 16, NOTEPADS_MARKDOWN_CORE_REFERENCE_BYTES = 17,
    NOTEPADS_MARKDOWN_CORE_INDEX_RECORDS = 18, NOTEPADS_MARKDOWN_CORE_QUERY_REQUESTS = 19,
    NOTEPADS_MARKDOWN_CORE_QUERY_RECORDS = 20, NOTEPADS_MARKDOWN_CORE_QUERY_CAPACITY = 21
};
enum notepads_markdown_core_domain { NOTEPADS_MARKDOWN_CORE_SOURCE_UTF8 = 1, NOTEPADS_MARKDOWN_CORE_FLOW_UTF16 = 2, NOTEPADS_MARKDOWN_CORE_PAYLOAD_UTF8 = 3 };
enum notepads_markdown_core_target_kind { NOTEPADS_MARKDOWN_CORE_FLOW = 1, NOTEPADS_MARKDOWN_CORE_PAYLOAD = 2 };
enum notepads_markdown_core_affinity { NOTEPADS_MARKDOWN_CORE_BEFORE = 0, NOTEPADS_MARKDOWN_CORE_AFTER = 1 };
enum notepads_markdown_core_direction { NOTEPADS_MARKDOWN_CORE_SOURCE_TO_TARGET = 1, NOTEPADS_MARKDOWN_CORE_TARGET_TO_SOURCE = 2 };
enum notepads_markdown_core_correspondence { NOTEPADS_MARKDOWN_CORE_EXACT = 0, NOTEPADS_MARKDOWN_CORE_ATOMIC = 1, NOTEPADS_MARKDOWN_CORE_HIDDEN = 2, NOTEPADS_MARKDOWN_CORE_SYNTHETIC = 3, NOTEPADS_MARKDOWN_CORE_OBJECT = 4 };
enum notepads_markdown_core_atom_kind { NOTEPADS_MARKDOWN_CORE_COPY_ATOM = 0, NOTEPADS_MARKDOWN_CORE_TRANSFORM_ATOM = 1, NOTEPADS_MARKDOWN_CORE_HIDDEN_ATOM = 2, NOTEPADS_MARKDOWN_CORE_SYNTHETIC_ATOM = 3, NOTEPADS_MARKDOWN_CORE_OBJECT_ATOM = 4 };
enum notepads_markdown_core_effect { NOTEPADS_MARKDOWN_CORE_EFFECT_ATOMIC = 1, NOTEPADS_MARKDOWN_CORE_EFFECT_HIDDEN = 2, NOTEPADS_MARKDOWN_CORE_EFFECT_SYNTHETIC = 4, NOTEPADS_MARKDOWN_CORE_EFFECT_OBJECT = 8 };
enum notepads_markdown_core_table
{
    NOTEPADS_MARKDOWN_CORE_POINTS = 1, NOTEPADS_MARKDOWN_CORE_RANGES = 2, NOTEPADS_MARKDOWN_CORE_SELECTIONS = 3,
    NOTEPADS_MARKDOWN_CORE_SPANS = 4, NOTEPADS_MARKDOWN_CORE_ANCHORS = 5, NOTEPADS_MARKDOWN_CORE_CONTRIBUTORS = 6, NOTEPADS_MARKDOWN_CORE_CONTRIBUTIONS = 7
};

typedef struct NOTEPADS_MARKDOWN_FFI_ALIGN8 notepads_markdown_core_header { uint32_t abi_version; uint32_t struct_size; } notepads_markdown_core_header;
typedef struct NOTEPADS_MARKDOWN_FFI_ALIGN8 notepads_markdown_core_source { uint8_t document[16]; uint64_t revision; } notepads_markdown_core_source;
typedef struct NOTEPADS_MARKDOWN_FFI_ALIGN8 notepads_markdown_core_identity
{
    uint8_t document[16];
    uint64_t revision;
    uint64_t generation;
    uint32_t profile;
    uint32_t extensions;
} notepads_markdown_core_identity;
typedef struct NOTEPADS_MARKDOWN_FFI_ALIGN8 notepads_markdown_core_error
{
    notepads_markdown_core_header header;
    int32_t status;
    uint32_t limit_kind;
    uint64_t requested;
    uint64_t limit;
    uint64_t expected;
    uint64_t actual;
    uint64_t offset;
    uint64_t edit_index;
} notepads_markdown_core_error;
typedef struct NOTEPADS_MARKDOWN_FFI_ALIGN8 notepads_markdown_core_snapshot
{
    notepads_markdown_core_header header;
    notepads_markdown_core_source source;
    uint32_t profile;
    uint32_t reserved;
} notepads_markdown_core_snapshot;
typedef struct NOTEPADS_MARKDOWN_FFI_ALIGN8 notepads_markdown_core_model_summary_result
{
    notepads_markdown_core_header header;
    notepads_markdown_core_identity identity;
    uint32_t main_flow_id;
    uint32_t has_footnotes;
    uint64_t nodes;
    uint64_t flows;
    uint64_t objects;
    uint64_t payloads;
    uint64_t atoms;
    uint64_t main_utf16;
} notepads_markdown_core_model_summary_result;
// Independently sampled tracked capacities; excludes SDK/OS/GPU allocations.
typedef struct NOTEPADS_MARKDOWN_FFI_ALIGN8 notepads_markdown_core_budget_usage_result
{
    notepads_markdown_core_header header;
    uint64_t source_capacity;
    uint64_t auxiliary_capacity;
} notepads_markdown_core_budget_usage_result;
typedef struct NOTEPADS_MARKDOWN_FFI_ALIGN8 notepads_markdown_core_read_result
{
    notepads_markdown_core_header header;
    uint64_t written;
    uint64_t next;
    uint32_t done;
    uint32_t reserved;
} notepads_markdown_core_read_result;
typedef struct NOTEPADS_MARKDOWN_FFI_ALIGN8 notepads_markdown_core_query_descriptor
{
    notepads_markdown_core_header header;
    notepads_markdown_core_identity identity;
    uint32_t domain;
    uint32_t target_kind;
    uint32_t target_id;
    uint32_t affinity;
    uint64_t start;
    uint64_t end;
} notepads_markdown_core_query_descriptor;
typedef struct NOTEPADS_MARKDOWN_FFI_ALIGN8 notepads_markdown_core_rows { uint32_t start; uint32_t count; } notepads_markdown_core_rows;
typedef struct NOTEPADS_MARKDOWN_FFI_ALIGN8 notepads_markdown_core_span
{
    uint32_t domain;
    uint32_t reserved;
    uint64_t start;
    uint64_t end;
} notepads_markdown_core_span;
typedef struct NOTEPADS_MARKDOWN_FFI_ALIGN8 notepads_markdown_core_position { uint32_t domain; uint32_t reserved; uint64_t value; } notepads_markdown_core_position;
typedef struct NOTEPADS_MARKDOWN_FFI_ALIGN8 notepads_markdown_core_related
{
    notepads_markdown_core_rows spans;
    notepads_markdown_core_rows anchors;
    uint32_t has_envelope;
    uint32_t reserved;
    notepads_markdown_core_span envelope;
} notepads_markdown_core_related;
typedef struct NOTEPADS_MARKDOWN_FFI_ALIGN8 notepads_markdown_core_point
{
    uint32_t target_kind;
    uint32_t target_id;
    uint32_t direction;
    uint32_t effects;
    notepads_markdown_core_position chosen;
    notepads_markdown_core_related related;
    uint32_t correspondence;
    uint32_t reserved;
    notepads_markdown_core_rows contributors;
} notepads_markdown_core_point;
typedef struct NOTEPADS_MARKDOWN_FFI_ALIGN8 notepads_markdown_core_range { uint32_t kind; uint32_t reserved; notepads_markdown_core_rows rows; } notepads_markdown_core_range;
typedef struct NOTEPADS_MARKDOWN_FFI_ALIGN8 notepads_markdown_core_selection
{
    uint32_t target_kind;
    uint32_t target_id;
    uint32_t direction;
    uint32_t effects;
    uint32_t expanded;
    uint32_t reserved;
    notepads_markdown_core_point start;
    notepads_markdown_core_point end;
    notepads_markdown_core_related related;
    notepads_markdown_core_rows contributions;
} notepads_markdown_core_selection;
typedef struct NOTEPADS_MARKDOWN_FFI_ALIGN8 notepads_markdown_core_contributor
{
    uint32_t atom;
    uint32_t source_part;
    uint32_t kind;
    uint32_t correspondence;
    uint64_t source_start;
    uint64_t source_end;
    uint64_t target_start;
    uint64_t target_end;
} notepads_markdown_core_contributor;
typedef struct NOTEPADS_MARKDOWN_FFI_ALIGN8 notepads_markdown_core_contribution
{
    uint32_t atom;
    uint32_t source_part;
    uint32_t kind;
    uint32_t expanded;
    uint64_t source_start;
    uint64_t source_end;
    uint64_t target_start;
    uint64_t target_end;
} notepads_markdown_core_contribution;
typedef struct NOTEPADS_MARKDOWN_FFI_ALIGN8 notepads_markdown_core_query_summary_result
{
    notepads_markdown_core_header header;
    notepads_markdown_core_identity identity;
    uint32_t kind;
    uint32_t reserved;
    uint64_t points;
    uint64_t ranges;
    uint64_t selections;
    uint64_t spans;
    uint64_t anchors;
    uint64_t contributors;
    uint64_t contributions;
} notepads_markdown_core_query_summary_result;

typedef struct notepads_markdown_core_allocation notepads_markdown_core_allocation; /* opaque */
typedef struct NOTEPADS_MARKDOWN_FFI_ALIGN8 notepads_markdown_core_submit_result {
    notepads_markdown_core_header header;           /* 0,8 */
    notepads_markdown_core_source source;           /* 8,24 */
    uint32_t state_valid;            /* 32 */
    uint32_t has_source;             /* 36 */
    uint32_t needs_resync;           /* 40 */
    uint32_t source_accepted;        /* 44 */
    uint32_t has_ready;              /* 48 */
    uint32_t reserved;               /* 52, always 0 */
    notepads_markdown_core_identity ready;          /* 56,40; total96 */
} notepads_markdown_core_submit_result;
typedef struct NOTEPADS_MARKDOWN_FFI_ALIGN8 notepads_markdown_core_edit_batch {
    notepads_markdown_core_header header;           /* 0,8 */
    notepads_markdown_core_source before;           /* 8,24 */
    notepads_markdown_core_source after;            /* 32,24 */
    uint64_t count;                  /* 56 */
    uint32_t row_size;               /* 64, must32 */
    uint32_t reserved;               /* 68, must0; total72 */
} notepads_markdown_core_edit_batch;
typedef struct NOTEPADS_MARKDOWN_FFI_ALIGN8 notepads_markdown_core_edit_row {
    uint64_t start;                  /* 0, original S UTF8 bytes */
    uint64_t end;                    /* 8, exclusive */
    uint64_t replacement_offset;     /* 16, blob byte index */
    uint64_t replacement_length;     /* 24; total32 */
} notepads_markdown_core_edit_row;


#ifdef __cplusplus
#define NOTEPADS_MARKDOWN_CORE_LAYOUT(type, bytes, field, offset) \
    static_assert(sizeof(type) == bytes); \
    static_assert(alignof(type) == 8); \
    static_assert(offsetof(type, field) == offset)
NOTEPADS_MARKDOWN_CORE_LAYOUT(notepads_markdown_core_header, 8, struct_size, 4);
NOTEPADS_MARKDOWN_CORE_LAYOUT(notepads_markdown_core_source, 24, revision, 16);
NOTEPADS_MARKDOWN_CORE_LAYOUT(notepads_markdown_core_identity, 40, profile, 32);
NOTEPADS_MARKDOWN_CORE_LAYOUT(notepads_markdown_core_error, 64, edit_index, 56);
NOTEPADS_MARKDOWN_CORE_LAYOUT(notepads_markdown_core_snapshot, 40, profile, 32);
NOTEPADS_MARKDOWN_CORE_LAYOUT(notepads_markdown_core_model_summary_result, 104, main_utf16, 96);
NOTEPADS_MARKDOWN_CORE_LAYOUT(notepads_markdown_core_budget_usage_result, 24, auxiliary_capacity, 16);
NOTEPADS_MARKDOWN_CORE_LAYOUT(notepads_markdown_core_read_result, 32, done, 24);
NOTEPADS_MARKDOWN_CORE_LAYOUT(notepads_markdown_core_query_descriptor, 80, start, 64);
NOTEPADS_MARKDOWN_CORE_LAYOUT(notepads_markdown_core_rows, 8, count, 4);
NOTEPADS_MARKDOWN_CORE_LAYOUT(notepads_markdown_core_span, 24, end, 16);
NOTEPADS_MARKDOWN_CORE_LAYOUT(notepads_markdown_core_position, 16, value, 8);
NOTEPADS_MARKDOWN_CORE_LAYOUT(notepads_markdown_core_related, 48, envelope, 24);
NOTEPADS_MARKDOWN_CORE_LAYOUT(notepads_markdown_core_point, 96, contributors, 88);
NOTEPADS_MARKDOWN_CORE_LAYOUT(notepads_markdown_core_range, 16, rows, 8);
NOTEPADS_MARKDOWN_CORE_LAYOUT(notepads_markdown_core_selection, 272, contributions, 264);
NOTEPADS_MARKDOWN_CORE_LAYOUT(notepads_markdown_core_contributor, 48, target_end, 40);
NOTEPADS_MARKDOWN_CORE_LAYOUT(notepads_markdown_core_contribution, 48, target_end, 40);
NOTEPADS_MARKDOWN_CORE_LAYOUT(notepads_markdown_core_query_summary_result, 112, contributions, 104);
NOTEPADS_MARKDOWN_CORE_LAYOUT(notepads_markdown_core_submit_result, 96, ready, 56);
NOTEPADS_MARKDOWN_CORE_LAYOUT(notepads_markdown_core_edit_batch, 72, row_size, 64);
NOTEPADS_MARKDOWN_CORE_LAYOUT(notepads_markdown_core_edit_row, 32, replacement_length, 24);
static_assert(offsetof(notepads_markdown_core_submit_result, header) == 0);
static_assert(offsetof(notepads_markdown_core_submit_result, source) == 8);
static_assert(offsetof(notepads_markdown_core_submit_result, state_valid) == 32);
static_assert(offsetof(notepads_markdown_core_submit_result, has_source) == 36);
static_assert(offsetof(notepads_markdown_core_submit_result, needs_resync) == 40);
static_assert(offsetof(notepads_markdown_core_submit_result, source_accepted) == 44);
static_assert(offsetof(notepads_markdown_core_submit_result, has_ready) == 48);
static_assert(offsetof(notepads_markdown_core_submit_result, reserved) == 52);
static_assert(offsetof(notepads_markdown_core_submit_result, ready) == 56);
static_assert(offsetof(notepads_markdown_core_edit_batch, header) == 0);
static_assert(offsetof(notepads_markdown_core_edit_batch, before) == 8);
static_assert(offsetof(notepads_markdown_core_edit_batch, after) == 32);
static_assert(offsetof(notepads_markdown_core_edit_batch, count) == 56);
static_assert(offsetof(notepads_markdown_core_edit_batch, row_size) == 64);
static_assert(offsetof(notepads_markdown_core_edit_batch, reserved) == 68);
static_assert(offsetof(notepads_markdown_core_edit_row, start) == 0);
static_assert(offsetof(notepads_markdown_core_edit_row, end) == 8);
static_assert(offsetof(notepads_markdown_core_edit_row, replacement_offset) == 16);
static_assert(offsetof(notepads_markdown_core_edit_row, replacement_length) == 24);
#undef NOTEPADS_MARKDOWN_CORE_LAYOUT
extern "C"
{
#endif
    uint32_t NOTEPADS_MARKDOWN_FFI_CALL notepads_markdown_core_abi_version(void);
    int32_t NOTEPADS_MARKDOWN_FFI_CALL notepads_markdown_core_cancel_create(const notepads_markdown_core_session* session_or_null, notepads_markdown_core_cancel** output, notepads_markdown_core_error* error);
    // Fresh flag from stable controller bootstrap; no mutable session access.
    int32_t NOTEPADS_MARKDOWN_FFI_CALL notepads_markdown_core_cancel_create_like(const notepads_markdown_core_cancel* prototype, notepads_markdown_core_cancel** output, notepads_markdown_core_error* error);
    int32_t NOTEPADS_MARKDOWN_FFI_CALL notepads_markdown_core_model_cancel_create(const notepads_markdown_core_model* model, notepads_markdown_core_cancel** output, notepads_markdown_core_error* error);
    int32_t NOTEPADS_MARKDOWN_FFI_CALL notepads_markdown_core_cancel_request(const notepads_markdown_core_cancel* cancel);
    int32_t NOTEPADS_MARKDOWN_FFI_CALL notepads_markdown_core_cancel_destroy(notepads_markdown_core_cancel* cancel);
    // Unknown extension bits fail preflight without accepting source or a generation.
    int32_t NOTEPADS_MARKDOWN_FFI_CALL notepads_markdown_core_session_create(const notepads_markdown_core_cancel* cancel,
        uint32_t profile, uint32_t extensions, notepads_markdown_core_session** output, notepads_markdown_core_error* error);
    // ABI3 source preparation uses one original job, never a precreated update flag.
    int32_t NOTEPADS_MARKDOWN_FFI_CALL notepads_markdown_core_session_start_request(notepads_markdown_core_session* session,
        uint32_t initially_cancelled, notepads_markdown_core_job** job, notepads_markdown_core_cancel** cancel,
        notepads_markdown_core_error* error);
    int32_t NOTEPADS_MARKDOWN_FFI_CALL notepads_markdown_core_job_replace_utf8(notepads_markdown_core_session* session,
        notepads_markdown_core_job* job, const uint8_t* input, uint64_t bytes,
        const notepads_markdown_core_snapshot* descriptor, uint32_t extensions,
        notepads_markdown_core_submit_result* result, notepads_markdown_core_error* error);
    int32_t NOTEPADS_MARKDOWN_FFI_CALL notepads_markdown_core_job_replace_utf16(notepads_markdown_core_session* session,
        notepads_markdown_core_job* job, const uint16_t* input, uint64_t units,
        const notepads_markdown_core_snapshot* descriptor, uint32_t extensions,
        notepads_markdown_core_submit_result* result, notepads_markdown_core_error* error);
    int32_t NOTEPADS_MARKDOWN_FFI_CALL notepads_markdown_core_job_apply(notepads_markdown_core_session* session,
        notepads_markdown_core_job* job, const notepads_markdown_core_edit_batch* descriptor,
        const notepads_markdown_core_edit_row* rows, const uint8_t* replacements, uint64_t replacement_bytes,
        notepads_markdown_core_submit_result* result, notepads_markdown_core_error* error);
    int32_t NOTEPADS_MARKDOWN_FFI_CALL notepads_markdown_core_job_capture_current(notepads_markdown_core_session* session,
        notepads_markdown_core_job* job, notepads_markdown_core_submit_result* result, notepads_markdown_core_error* error);
    int32_t NOTEPADS_MARKDOWN_FFI_CALL notepads_markdown_core_job_reserve_allocation(notepads_markdown_core_job* job,
        uint64_t requested, notepads_markdown_core_allocation** output, notepads_markdown_core_error* error);
    int32_t NOTEPADS_MARKDOWN_FFI_CALL notepads_markdown_core_job_admit_allocation(notepads_markdown_core_job* job,
        notepads_markdown_core_allocation* allocation, uint64_t actual_capacity, notepads_markdown_core_error* error);
    int32_t NOTEPADS_MARKDOWN_FFI_CALL notepads_markdown_core_job_allocation_failed(notepads_markdown_core_job* job,
        uint64_t requested, notepads_markdown_core_error* error);
    int32_t NOTEPADS_MARKDOWN_FFI_CALL notepads_markdown_core_allocation_destroy(notepads_markdown_core_allocation* allocation);
    int32_t NOTEPADS_MARKDOWN_FFI_CALL notepads_markdown_core_session_detach(notepads_markdown_core_session* session, const notepads_markdown_core_cancel* cancel, notepads_markdown_core_error* error);
    int32_t NOTEPADS_MARKDOWN_FFI_CALL notepads_markdown_core_session_destroy(notepads_markdown_core_session* session);
    int32_t NOTEPADS_MARKDOWN_FFI_CALL notepads_markdown_core_job_run(notepads_markdown_core_job* job, notepads_markdown_core_error* error);
    int32_t NOTEPADS_MARKDOWN_FFI_CALL notepads_markdown_core_job_install(notepads_markdown_core_session* session, notepads_markdown_core_job* job, notepads_markdown_core_model** output, notepads_markdown_core_error* error);
    int32_t NOTEPADS_MARKDOWN_FFI_CALL notepads_markdown_core_job_destroy(notepads_markdown_core_job* job);
    int32_t NOTEPADS_MARKDOWN_FFI_CALL notepads_markdown_core_model_summary(const notepads_markdown_core_model* model, notepads_markdown_core_model_summary_result* output, notepads_markdown_core_error* error);
    int32_t NOTEPADS_MARKDOWN_FFI_CALL notepads_markdown_core_model_budget_usage(const notepads_markdown_core_model* model, notepads_markdown_core_budget_usage_result* output, notepads_markdown_core_error* error);
    int32_t NOTEPADS_MARKDOWN_FFI_CALL notepads_markdown_core_model_clone(const notepads_markdown_core_model* model, const notepads_markdown_core_cancel* cancel, notepads_markdown_core_model** output, notepads_markdown_core_error* error);
    int32_t NOTEPADS_MARKDOWN_FFI_CALL notepads_markdown_core_model_destroy(notepads_markdown_core_model* model);
    int32_t NOTEPADS_MARKDOWN_FFI_CALL notepads_markdown_core_stream_create(const notepads_markdown_core_model* model, const notepads_markdown_core_cancel* cancel, notepads_markdown_core_stream** output, notepads_markdown_core_error* error);
    // 4..65536 bytes; written is UTF8 bytes, next is Main UTF16. Complete scalars
    // only. Failure invalidates the whole buffer batch and latches stream fault.
    int32_t NOTEPADS_MARKDOWN_FFI_CALL notepads_markdown_core_stream_read(notepads_markdown_core_stream* stream, uint8_t* buffer, uint64_t capacity, notepads_markdown_core_read_result* output, notepads_markdown_core_error* error);
    int32_t NOTEPADS_MARKDOWN_FFI_CALL notepads_markdown_core_stream_destroy(notepads_markdown_core_stream* stream);
    // Point descriptors have start==end. Source queries identify a flow/payload
    // target; reverse descriptors' domain must match that target. IDs belong to
    // the supplied complete model identity, which is checked by core.
    int32_t NOTEPADS_MARKDOWN_FFI_CALL notepads_markdown_core_map_points(const notepads_markdown_core_model* model, const notepads_markdown_core_cancel* cancel,
        const notepads_markdown_core_query_descriptor* queries, uint64_t count, notepads_markdown_core_query** output, notepads_markdown_core_error* error);
    int32_t NOTEPADS_MARKDOWN_FFI_CALL notepads_markdown_core_map_ranges(const notepads_markdown_core_model* model, const notepads_markdown_core_cancel* cancel,
        const notepads_markdown_core_query_descriptor* queries, uint64_t count, notepads_markdown_core_query** output, notepads_markdown_core_error* error);
    int32_t NOTEPADS_MARKDOWN_FFI_CALL notepads_markdown_core_query_summary(notepads_markdown_core_query* query, notepads_markdown_core_query_summary_result* output, notepads_markdown_core_error* error);
    // Exact wire row_size, <=256 rows, buffer aligned8 and count*row_size bytes.
    // next is a row index. No internal borrowed pointer is returned. Point batch
    // POINTS=results; range batch POINTS=boundaries, RANGES tags 1=boundary/2=selection.
    // Embedded Rows refer to this response's corresponding complete table.
    // An envelope is diagnostic and does not authorize its interior. Failed
    // buffer contents are invalid and must be discarded; query fault is latched.
    int32_t NOTEPADS_MARKDOWN_FFI_CALL notepads_markdown_core_query_read(notepads_markdown_core_query* query, uint32_t table, uint64_t start, uint64_t count,
        uint32_t row_size, void* buffer, notepads_markdown_core_read_result* output, notepads_markdown_core_error* error);
    int32_t NOTEPADS_MARKDOWN_FFI_CALL notepads_markdown_core_query_destroy(notepads_markdown_core_query* query);
#ifdef __cplusplus
}
#endif
