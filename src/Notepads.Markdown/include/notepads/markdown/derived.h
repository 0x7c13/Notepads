// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE file in the project root for license information.
// ---------------------------------------------------------------------------------------------

#pragma once
#include "notepads/markdown/preview.h"

// Additive private ABI1, independent of unchanged Preview ABI2 text/image rows.
// Descriptors and both UTF8 slices belong to the same exclusive current preview
// batch and its immutable model. Copy blob bytes with notepads_markdown_preview_read(BLOB).
// Do not interleave measure/present/submit between summary/rows/blob copies.
// The native mirror must acquire its own model-funded allocation before copying.
// Main offsets still count a derived object once, regardless of input/export size.
enum { NOTEPADS_MARKDOWN_DERIVED_ABI_VERSION = 1 };
enum notepads_markdown_derived_kind {
    NOTEPADS_MARKDOWN_DERIVED_MATH = 1,
    NOTEPADS_MARKDOWN_DERIVED_DIAGRAM = 2,
    NOTEPADS_MARKDOWN_DERIVED_NOTE_EMBED = 3
};
enum notepads_markdown_math_syntax
{
    NOTEPADS_MARKDOWN_MATH_DOLLAR_INLINE = 1,
    NOTEPADS_MARKDOWN_MATH_DOLLAR_DISPLAY = 2,
    NOTEPADS_MARKDOWN_MATH_LATEX_INLINE = 3,
    NOTEPADS_MARKDOWN_MATH_LATEX_DISPLAY = 4,
    NOTEPADS_MARKDOWN_MATH_CODE_INLINE = 5,
    NOTEPADS_MARKDOWN_MATH_FENCED_DISPLAY = 6
};

typedef struct NOTEPADS_MARKDOWN_FFI_ALIGN8 notepads_markdown_derived_summary_result
{
    notepads_markdown_core_header header;
    notepads_markdown_preview_key key;
    uint64_t start;
    uint64_t rows;
    uint64_t blob_bytes;
} notepads_markdown_derived_summary_result;

typedef struct NOTEPADS_MARKDOWN_FFI_ALIGN8 notepads_markdown_derived_row
{
    uint32_t object;
    uint32_t node;
    uint32_t text;
    uint32_t offset_utf16;
    uint32_t kind;
    uint32_t syntax;
    notepads_markdown_preview_blob input;
    notepads_markdown_preview_blob export_source;
    notepads_markdown_preview_blob accessible_title;
    notepads_markdown_preview_blob accessible_description;
} notepads_markdown_derived_row;

#ifdef __cplusplus
static_assert(sizeof(notepads_markdown_derived_summary_result) == 128 && alignof(notepads_markdown_derived_summary_result) == 8);
static_assert(sizeof(notepads_markdown_derived_row) == 88 && alignof(notepads_markdown_derived_row) == 8);
static_assert(offsetof(notepads_markdown_derived_row, input) == 24);
extern "C"
{
#endif
    uint32_t NOTEPADS_MARKDOWN_FFI_CALL notepads_markdown_derived_abi_version(void);
    int32_t NOTEPADS_MARKDOWN_FFI_CALL notepads_markdown_derived_summary(notepads_markdown_preview_layout* layout,
        notepads_markdown_derived_summary_result* output, notepads_markdown_core_error* error);
    // At most256 rows; Core ABI3 read-result prefix. Discard partial buffer on failure.
    int32_t NOTEPADS_MARKDOWN_FFI_CALL notepads_markdown_derived_read(notepads_markdown_preview_layout* layout,
        uint64_t start, uint64_t count, uint32_t row_size, notepads_markdown_derived_row* buffer,
        notepads_markdown_core_read_result* output, notepads_markdown_core_error* error);
#ifdef __cplusplus
}
#endif
