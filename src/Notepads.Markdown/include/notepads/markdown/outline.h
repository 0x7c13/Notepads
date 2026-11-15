// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE file in the project root for license information.
// ---------------------------------------------------------------------------------------------
#pragma once
#include "notepads/markdown/core.h"

// Independent private outline ABI1; errors/read results retain core ABI3.
// One exclusive caller owns a directory. Its original Operation, work and
// first fault survive every summary/read call; destroy after all borrows end.
// Only cancellation on its separately live cancel handle may run concurrently.
// Failed output bytes are invalid. No handles/buffers/outputs may overlap.
#define NOTEPADS_MARKDOWN_OUTLINE_ABI_VERSION 1u
#define NOTEPADS_MARKDOWN_OUTLINE_HEADINGS 1u
#define NOTEPADS_MARKDOWN_OUTLINE_BLOB 2u
#define NOTEPADS_MARKDOWN_OUTLINE_ANCHORS 3u

typedef struct notepads_markdown_outline notepads_markdown_outline;

typedef struct NOTEPADS_MARKDOWN_FFI_ALIGN8 notepads_markdown_outline_blob
{
    uint64_t start;
    uint64_t length;
} notepads_markdown_outline_blob;

typedef struct NOTEPADS_MARKDOWN_FFI_ALIGN8 notepads_markdown_outline_heading
{
    uint32_t node; // Internal same-model block key; never a permanent public ID.
    uint32_t level;
    uint64_t logical_start; // Main UTF16, including valid empty ranges.
    uint64_t logical_end;
    notepads_markdown_outline_blob title;
    notepads_markdown_outline_blob anchor;
} notepads_markdown_outline_heading;

typedef struct NOTEPADS_MARKDOWN_FFI_ALIGN8 notepads_markdown_outline_anchor
{
    uint64_t position; // Main UTF16 boundary.
    uint32_t legacy; // An HTML a[name] target.
    uint32_t node; // Block key, or zero for an inline target.
    notepads_markdown_outline_blob name;
} notepads_markdown_outline_anchor;

typedef struct NOTEPADS_MARKDOWN_FFI_ALIGN8 notepads_markdown_outline_summary
{
    notepads_markdown_core_header header; // NOTEPADS_MARKDOWN_OUTLINE_ABI_VERSION, sizeof(notepads_markdown_outline_summary)
    notepads_markdown_core_identity model;
    uint64_t headings;
    uint64_t blob_bytes;
    uint64_t anchors;
} notepads_markdown_outline_summary;

#ifdef __cplusplus
static_assert(sizeof(notepads_markdown_outline_blob) == 16 && alignof(notepads_markdown_outline_blob) == 8);
static_assert(sizeof(notepads_markdown_outline_heading) == 56 && alignof(notepads_markdown_outline_heading) == 8);
static_assert(sizeof(notepads_markdown_outline_anchor) == 32 && alignof(notepads_markdown_outline_anchor) == 8);
static_assert(sizeof(notepads_markdown_outline_summary) == 72 && alignof(notepads_markdown_outline_summary) == 8);
extern "C"
{
#endif
    uint32_t NOTEPADS_MARKDOWN_FFI_CALL notepads_markdown_outline_abi_version(void);
    int32_t NOTEPADS_MARKDOWN_FFI_CALL notepads_markdown_outline_create(const notepads_markdown_core_model* model, const notepads_markdown_core_cancel* cancel,
        notepads_markdown_outline** output, notepads_markdown_core_error* error);
    int32_t NOTEPADS_MARKDOWN_FFI_CALL notepads_markdown_outline_get_summary(notepads_markdown_outline* outline,
        notepads_markdown_outline_summary* output, notepads_markdown_core_error* error);
    // At most 256 headings/anchors or 65536 blob bytes. Exact row_size: sizeof(row)/1.
    // Blob pages may divide UTF8 scalars; reassemble a full slice before decoding.
    // start+count must stay within the table; count0 permits a null buffer.
    int32_t NOTEPADS_MARKDOWN_FFI_CALL notepads_markdown_outline_read(notepads_markdown_outline* outline, uint32_t table, uint64_t start,
        uint64_t count, uint32_t row_size, void* buffer, notepads_markdown_core_read_result* output, notepads_markdown_core_error* error);
    int32_t NOTEPADS_MARKDOWN_FFI_CALL notepads_markdown_outline_destroy(notepads_markdown_outline* outline);
#ifdef __cplusplus
}
#endif
