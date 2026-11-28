// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE file in the project root for license information.
// ---------------------------------------------------------------------------------------------
#pragma once
#include "notepads/markdown/core.h"

// Private preview ABI3. Error/read-result prefixes remain core ABI3.
// Kind4 is a paired-build semantic extension; never mix older Native consumers.
// One exclusive caller owns a layout and its current presentation cache. Only
// notepads_markdown_core_cancel_request on its separately live flag may run concurrently.
// Never overlap handles/buffers/descriptors/results. Failed bytes are invalid;
// semantic errors latch the first task fault. Destroy after all borrows end.
// Batch blob ranges, Text-local UTF16 and source UTF8 are distinct domains.
// Blob data is UTF8 text except the explicitly typed code-highlight ranges.
#define NOTEPADS_MARKDOWN_PREVIEW_ABI_VERSION 3u
#define NOTEPADS_MARKDOWN_PREVIEW_NONE UINT32_MAX
#define NOTEPADS_MARKDOWN_PREVIEW_BLOCKS 1u
#define NOTEPADS_MARKDOWN_PREVIEW_TEXTS 2u
#define NOTEPADS_MARKDOWN_PREVIEW_RUNS 3u
#define NOTEPADS_MARKDOWN_PREVIEW_CELLS 4u
#define NOTEPADS_MARKDOWN_PREVIEW_DECORATIONS 5u
#define NOTEPADS_MARKDOWN_PREVIEW_DECORATION_BULLET 1u
#define NOTEPADS_MARKDOWN_PREVIEW_DECORATION_ORDERED 2u
#define NOTEPADS_MARKDOWN_PREVIEW_DECORATION_TASK 3u
#define NOTEPADS_MARKDOWN_PREVIEW_DECORATION_STATIC_HTML_CHECKBOX 4u
#define NOTEPADS_MARKDOWN_PREVIEW_IMAGES 6u
#define NOTEPADS_MARKDOWN_PREVIEW_LINKS 7u
#define NOTEPADS_MARKDOWN_PREVIEW_BLOB 8u
#define NOTEPADS_MARKDOWN_PREVIEW_SCRIPT_SCOPES 10u
#define NOTEPADS_MARKDOWN_PREVIEW_BLOCK_GEOMETRY 11u
#define NOTEPADS_MARKDOWN_PREVIEW_HTML_CELLS 12u
#define NOTEPADS_MARKDOWN_PREVIEW_DISCLOSURES 13u
#define NOTEPADS_MARKDOWN_PREVIEW_HIDDEN_RANGES 14u
#define NOTEPADS_MARKDOWN_PREVIEW_RUBY_UNITS 15u
#define NOTEPADS_MARKDOWN_PREVIEW_LAYOUT_VISIBLE 0u
#define NOTEPADS_MARKDOWN_PREVIEW_LAYOUT_MARKERS 1u
#define NOTEPADS_MARKDOWN_PREVIEW_LAYOUT_SEMANTICS 2u
#define NOTEPADS_MARKDOWN_PREVIEW_HTML_CELL_HTML 1u
#define NOTEPADS_MARKDOWN_PREVIEW_HTML_CELL_COVERED 2u
#define NOTEPADS_MARKDOWN_PREVIEW_HTML_CELL_CAPTION 4u
#define NOTEPADS_MARKDOWN_PREVIEW_HTML_CELL_MASK 7u
#define NOTEPADS_MARKDOWN_PREVIEW_SCRIPT_ABI_VERSION 1u
#define NOTEPADS_MARKDOWN_PREVIEW_SCRIPT_FONT_MASK 11u // bold1 | italic2 | code font8
#define NOTEPADS_MARKDOWN_PREVIEW_SCRIPT_CODE_LINE_HEIGHT 16u // code element's 1.357em line box
#define NOTEPADS_MARKDOWN_PREVIEW_SCRIPT_SHAPE_ONLY 32u // real container boundary, no line box
#define NOTEPADS_MARKDOWN_PREVIEW_SCRIPT_SCOPE_MASK 59u // font mask | line box16 | shape-only32

typedef struct notepads_markdown_preview_layout notepads_markdown_preview_layout;
typedef struct notepads_markdown_preview_allocation notepads_markdown_preview_allocation;

typedef struct NOTEPADS_MARKDOWN_FFI_ALIGN8 notepads_markdown_preview_blob
{
    uint64_t start;
    uint64_t length;
} notepads_markdown_preview_blob;

typedef struct NOTEPADS_MARKDOWN_FFI_ALIGN8 notepads_markdown_preview_key
{
    notepads_markdown_core_header header;
    notepads_markdown_core_identity model;
    uint64_t layout_generation;
    uint64_t theme_generation;
    uint64_t measurement_generation;
    double width;
    double indent;
    double gap;
} notepads_markdown_preview_key;

typedef struct NOTEPADS_MARKDOWN_FFI_ALIGN8 notepads_markdown_preview_summary
{
    notepads_markdown_core_header header;
    notepads_markdown_preview_key key;
    uint64_t start;
    uint64_t blocks;
    uint64_t texts;
    uint64_t runs;
    uint64_t cells;
    uint64_t decorations;
    uint64_t images;
    uint64_t links;
    uint64_t blob_bytes;
    uint64_t ruby_units;
} notepads_markdown_preview_summary;

typedef struct NOTEPADS_MARKDOWN_FFI_ALIGN8 notepads_markdown_preview_disclosure_override
{
    uint32_t node;
    uint32_t expanded;
} notepads_markdown_preview_disclosure_override;

typedef struct NOTEPADS_MARKDOWN_FFI_ALIGN8 notepads_markdown_preview_visibility_result
{
    notepads_markdown_core_header header;
    notepads_markdown_preview_key key;
    uint64_t disclosures;
    uint64_t hidden_ranges;
    uint64_t blocks;
} notepads_markdown_preview_visibility_result;

typedef struct NOTEPADS_MARKDOWN_FFI_ALIGN8 notepads_markdown_preview_disclosure
{
    uint32_t node;
    uint32_t summary;
    uint32_t expanded;
    uint32_t first_body_node;
    uint32_t next_node;
    uint32_t reserved;
    uint64_t summary_start;
    uint64_t body_start;
    uint64_t body_end;
} notepads_markdown_preview_disclosure;

typedef struct NOTEPADS_MARKDOWN_FFI_ALIGN8 notepads_markdown_preview_hidden_range
{
    uint64_t start;
    uint64_t end;
} notepads_markdown_preview_hidden_range;

typedef struct NOTEPADS_MARKDOWN_FFI_ALIGN8 notepads_markdown_preview_block
{
    uint32_t node;
    uint32_t kind; // paragraph1/heading2/code3/html4/rule5/table6/empty-item7
    uint64_t logical_start;
    uint64_t logical_end;
    double x;
    double width;
    double y;
    double height;
    uint32_t quote_depth;
    uint32_t item_depth;
    uint32_t heading_level;
    uint32_t columns;
    uint32_t row_count;
    uint32_t alignment; // none0/left1/center2/right3
    notepads_markdown_core_rows texts;
    notepads_markdown_core_rows cells;
    notepads_markdown_core_rows decorations;
    notepads_markdown_preview_blob code_info;
    uint32_t code_first_line_number; // Zero hides the numbering gutter.
    uint32_t code_reserved;
    notepads_markdown_preview_blob code_highlight_lines; // Pairs of one-based inclusive little-endian u32s.
} notepads_markdown_preview_block;

// Additive batch-local marker origin. Same indices/count as BLOCKS. Original
// block rows stay ABI3; never derive a list gutter from a deeper quote's x.
typedef struct NOTEPADS_MARKDOWN_FFI_ALIGN8 notepads_markdown_preview_block_geometry
{
    uint32_t node;
    uint32_t reserved;
    double marker_x;
} notepads_markdown_preview_block_geometry;

// Complete immutable quote directory, semantic preorder. Empty quotes have
// zero height. Coordinates share block document-space DIPs; no Canvas padding.
enum notepads_markdown_preview_callout
{
    NOTEPADS_MARKDOWN_CALLOUT_NONE = 0,
    NOTEPADS_MARKDOWN_CALLOUT_NOTE = 1,
    NOTEPADS_MARKDOWN_CALLOUT_TIP = 2,
    NOTEPADS_MARKDOWN_CALLOUT_IMPORTANT = 3,
    NOTEPADS_MARKDOWN_CALLOUT_WARNING = 4,
    NOTEPADS_MARKDOWN_CALLOUT_CAUTION = 5,
    NOTEPADS_MARKDOWN_CALLOUT_ABSTRACT = 6,
    NOTEPADS_MARKDOWN_CALLOUT_INFO = 7,
    NOTEPADS_MARKDOWN_CALLOUT_TODO = 8,
    NOTEPADS_MARKDOWN_CALLOUT_SUCCESS = 9,
    NOTEPADS_MARKDOWN_CALLOUT_QUESTION = 10,
    NOTEPADS_MARKDOWN_CALLOUT_FAILURE = 11,
    NOTEPADS_MARKDOWN_CALLOUT_DANGER = 12,
    NOTEPADS_MARKDOWN_CALLOUT_BUG = 13,
    NOTEPADS_MARKDOWN_CALLOUT_EXAMPLE = 14,
    NOTEPADS_MARKDOWN_CALLOUT_QUOTE = 15
};

typedef struct NOTEPADS_MARKDOWN_FFI_ALIGN8 notepads_markdown_preview_quote
{
    uint32_t node;
    uint32_t depth;
    uint32_t callout;
    uint32_t reserved;
    double x;
    double width;
    double y;
    double height;
    double border_width;
} notepads_markdown_preview_quote;

typedef struct NOTEPADS_MARKDOWN_FFI_ALIGN8 notepads_markdown_preview_quote_summary_result
{
    notepads_markdown_core_header header;
    notepads_markdown_preview_key key;
    uint64_t rows;
} notepads_markdown_preview_quote_summary_result;

typedef struct NOTEPADS_MARKDOWN_FFI_ALIGN8 notepads_markdown_preview_text
{
    uint32_t node;
    uint32_t reserved;
    uint64_t logical_start;
    uint64_t logical_end;
    uint64_t utf16_length;
    notepads_markdown_preview_blob utf8;
    notepads_markdown_core_rows runs;
    notepads_markdown_core_rows images;
} notepads_markdown_preview_text;

#define NOTEPADS_MARKDOWN_PREVIEW_RUN_UNDERLINE 32u
#define NOTEPADS_MARKDOWN_PREVIEW_RUN_MONOSPACE 64u // font family only
#define NOTEPADS_MARKDOWN_PREVIEW_RUN_HIGHLIGHT 128u

// displacement DIP = parent font size DIP * relative_em + absolute_dip.
// Scopes include empty intervals and preserve each container opening font state.
// Code fonts (8) include code/kbd/samp/tt; only HTML/Markdown code has line box16.
// ScriptScope bit16 is independent of Run.flags bit16 (inert HTML).
// Copy summary/scopes/runs from one exclusive current batch without intervening
// measure/present/submit calls; validate its key/start against notepads_markdown_preview_summary.
typedef struct NOTEPADS_MARKDOWN_FFI_ALIGN8 notepads_markdown_preview_run
{
    uint32_t start;
    uint32_t end;
    uint32_t flags;
    uint32_t link;
    double size_scale;
    double relative_em;
    double absolute_dip;
} notepads_markdown_preview_run;

typedef struct NOTEPADS_MARKDOWN_FFI_ALIGN8 notepads_markdown_preview_ruby_unit
{
    uint32_t text; // batch-local Text index
    uint32_t start; // Text-local UTF16 start of the ruby base
    uint32_t base_end; // end of base, start of annotation
    uint32_t end; // end of annotation
} notepads_markdown_preview_ruby_unit;

typedef struct NOTEPADS_MARKDOWN_FFI_ALIGN8 notepads_markdown_preview_script_scope
{
    uint32_t text; // batch-local Text index
    uint32_t start; // Text-local UTF16 half-open interval
    uint32_t end;
    uint32_t font_flags; // opening fonts/line box or SHAPE_ONLY; NOTEPADS_MARKDOWN_PREVIEW_SCRIPT_SCOPE_MASK
    double size_scale;
    double relative_em;
    double absolute_dip;
} notepads_markdown_preview_script_scope;

typedef struct NOTEPADS_MARKDOWN_FFI_ALIGN8 notepads_markdown_preview_script_summary_result
{
    notepads_markdown_core_header header; // independent NOTEPADS_MARKDOWN_PREVIEW_SCRIPT_ABI_VERSION prefix
    notepads_markdown_preview_key key;
    uint64_t start;
    uint64_t rows;
} notepads_markdown_preview_script_summary_result;
typedef struct NOTEPADS_MARKDOWN_FFI_ALIGN8 notepads_markdown_preview_cell
{
    uint32_t node;
    uint32_t row_node;
    uint32_t row;
    uint32_t column;
    uint32_t alignment; // none0/left1/center2/right3
    uint32_t header;
    uint32_t missing;
    uint32_t text;
} notepads_markdown_preview_cell;

// HTML topology, same count/order as CELLS in the same immutable
// batch. The old cell POD and summary stay ABI3. Paired-build consumers only.
// Ordinary cells: span1/flags0. HTML owners: span1..64; covered: span0 and
// empty payload. All caption slots carry CAPTION; reserved is always zero.
typedef struct NOTEPADS_MARKDOWN_FFI_ALIGN8 notepads_markdown_preview_html_cell
{
    uint32_t node;
    uint32_t colspan;
    uint32_t flags;
    uint32_t reserved;
    uint32_t rowspan;
    uint32_t reserved_geometry;
} notepads_markdown_preview_html_cell;

typedef struct NOTEPADS_MARKDOWN_FFI_ALIGN8 notepads_markdown_preview_decoration
{
    uint32_t item;
    uint32_t list;
    uint32_t kind; // bullet1/ordered2/task3/static disabled HTML checkbox4
    uint32_t depth;
    uint64_t ordinal;
    uint32_t checked;
    uint32_t object;
    uint32_t marker;
    uint32_t has_action; // static HTML checkbox requires this and all action fields to be zero
    uint64_t action_start;
    uint64_t action_end;
    uint32_t expected; // source bytes in low 24 bits, little-endian
    uint32_t reserved;
    uint64_t logical_start; // Main UTF-16 task object; UINT64_MAX for non-task decorations
    uint64_t logical_end;
} notepads_markdown_preview_decoration;

typedef struct NOTEPADS_MARKDOWN_FFI_ALIGN8 notepads_markdown_preview_image
{
    uint32_t object;
    uint32_t node;
    uint32_t text;
    uint32_t offset_utf16;
    uint32_t width; // HTML dimensions in CSS pixels, 0 for intrinsic
    uint32_t height;
    notepads_markdown_preview_blob destination;
    notepads_markdown_preview_blob title;
    notepads_markdown_preview_blob alt;
    notepads_markdown_preview_blob light_source;
    notepads_markdown_preview_blob dark_source;
} notepads_markdown_preview_image;

typedef struct NOTEPADS_MARKDOWN_FFI_ALIGN8 notepads_markdown_preview_link
{
    uint32_t node;
    uint32_t local_position_plus_one; // Main UTF-16 position plus one, or zero.
    uint32_t local_heading_node;     // Main heading NodeId, or zero.
    uint32_t wiki;                   // Destination uses local wiki syntax.
    notepads_markdown_preview_blob destination;
    notepads_markdown_preview_blob title;
} notepads_markdown_preview_link;

typedef struct NOTEPADS_MARKDOWN_FFI_ALIGN8 notepads_markdown_preview_metric
{
    uint32_t node;
    uint32_t reserved;
    double height;
} notepads_markdown_preview_metric;

typedef struct NOTEPADS_MARKDOWN_FFI_ALIGN8 notepads_markdown_preview_extent
{
    notepads_markdown_core_header header;
    notepads_markdown_preview_key key;
    uint64_t blocks;
    double height;
} notepads_markdown_preview_extent;

typedef struct NOTEPADS_MARKDOWN_FFI_ALIGN8 notepads_markdown_preview_viewport
{
    notepads_markdown_core_header header;
    notepads_markdown_preview_key key;
    double y;
    double height;
    double overscan;
} notepads_markdown_preview_viewport;

typedef struct NOTEPADS_MARKDOWN_FFI_ALIGN8 notepads_markdown_preview_range
{
    notepads_markdown_core_header header;
    uint64_t start;
    uint64_t count;
} notepads_markdown_preview_range;

#ifdef __cplusplus
#define NOTEPADS_MARKDOWN_PREVIEW_SIZE(type, bytes) static_assert(sizeof(type) == bytes && alignof(type) == 8)
NOTEPADS_MARKDOWN_PREVIEW_SIZE(notepads_markdown_preview_blob, 16);
NOTEPADS_MARKDOWN_PREVIEW_SIZE(notepads_markdown_preview_key, 96);
NOTEPADS_MARKDOWN_PREVIEW_SIZE(notepads_markdown_preview_summary, 184);
NOTEPADS_MARKDOWN_PREVIEW_SIZE(notepads_markdown_preview_ruby_unit, 16);
NOTEPADS_MARKDOWN_PREVIEW_SIZE(notepads_markdown_preview_disclosure_override, 8);
NOTEPADS_MARKDOWN_PREVIEW_SIZE(notepads_markdown_preview_visibility_result, 128);
NOTEPADS_MARKDOWN_PREVIEW_SIZE(notepads_markdown_preview_disclosure, 48);
NOTEPADS_MARKDOWN_PREVIEW_SIZE(notepads_markdown_preview_hidden_range, 16);
NOTEPADS_MARKDOWN_PREVIEW_SIZE(notepads_markdown_preview_block, 144);
NOTEPADS_MARKDOWN_PREVIEW_SIZE(notepads_markdown_preview_block_geometry, 16);
NOTEPADS_MARKDOWN_PREVIEW_SIZE(notepads_markdown_preview_quote, 56);
NOTEPADS_MARKDOWN_PREVIEW_SIZE(notepads_markdown_preview_quote_summary_result, 112);
NOTEPADS_MARKDOWN_PREVIEW_SIZE(notepads_markdown_preview_text, 64);
NOTEPADS_MARKDOWN_PREVIEW_SIZE(notepads_markdown_preview_run, 40);
NOTEPADS_MARKDOWN_PREVIEW_SIZE(notepads_markdown_preview_script_scope, 40);
NOTEPADS_MARKDOWN_PREVIEW_SIZE(notepads_markdown_preview_script_summary_result, 120);
static_assert(offsetof(notepads_markdown_preview_run, size_scale) == 16);
static_assert(offsetof(notepads_markdown_preview_run, absolute_dip) == 32);
static_assert(offsetof(notepads_markdown_preview_script_scope, font_flags) == 12);
NOTEPADS_MARKDOWN_PREVIEW_SIZE(notepads_markdown_preview_cell, 32);
NOTEPADS_MARKDOWN_PREVIEW_SIZE(notepads_markdown_preview_html_cell, 24);
static_assert(offsetof(notepads_markdown_preview_html_cell, colspan) == 4);
static_assert(offsetof(notepads_markdown_preview_html_cell, flags) == 8);
static_assert(offsetof(notepads_markdown_preview_html_cell, reserved) == 12);
NOTEPADS_MARKDOWN_PREVIEW_SIZE(notepads_markdown_preview_decoration, 80);
NOTEPADS_MARKDOWN_PREVIEW_SIZE(notepads_markdown_preview_image, 104);
NOTEPADS_MARKDOWN_PREVIEW_SIZE(notepads_markdown_preview_link, 48);
NOTEPADS_MARKDOWN_PREVIEW_SIZE(notepads_markdown_preview_metric, 16);
NOTEPADS_MARKDOWN_PREVIEW_SIZE(notepads_markdown_preview_extent, 120);
NOTEPADS_MARKDOWN_PREVIEW_SIZE(notepads_markdown_preview_viewport, 128);
NOTEPADS_MARKDOWN_PREVIEW_SIZE(notepads_markdown_preview_range, 24);
#undef NOTEPADS_MARKDOWN_PREVIEW_SIZE
extern "C"
{
#endif
    uint32_t NOTEPADS_MARKDOWN_FFI_CALL notepads_markdown_preview_abi_version(void);
    // Complete-state only; independent of the current measure/present batch.
    // Read at most 256 rows, exact row_size. Exclusive layout and nonoverlapping
    // initialized result/error PODs. Failed bytes invalid; original cancellation
    // and first-fault rules apply. Zero-row reads still require a nonnull buffer.
    int32_t NOTEPADS_MARKDOWN_FFI_CALL notepads_markdown_preview_quote_summary(notepads_markdown_preview_layout* layout,
        notepads_markdown_preview_quote_summary_result* output, notepads_markdown_core_error* error);
    int32_t NOTEPADS_MARKDOWN_FFI_CALL notepads_markdown_preview_quote_read(notepads_markdown_preview_layout* layout, uint64_t start,
        uint64_t count, uint32_t row_size, notepads_markdown_preview_quote* buffer,
        notepads_markdown_core_read_result* output, notepads_markdown_core_error* error);
    int32_t NOTEPADS_MARKDOWN_FFI_CALL notepads_markdown_preview_script_summary(notepads_markdown_preview_layout* layout,
        notepads_markdown_preview_script_summary_result* output, notepads_markdown_core_error* error);
    int32_t NOTEPADS_MARKDOWN_FFI_CALL notepads_markdown_preview_cancel_create(notepads_markdown_core_cancel** output, notepads_markdown_core_error* error);
    // Visible layout overrides are ascending by node, with unique IDs and expanded 0 or 1.
    // A nonzero section_heading limits projection to that accepted heading's section.
    int32_t NOTEPADS_MARKDOWN_FFI_CALL notepads_markdown_preview_layout_create(const notepads_markdown_core_model* model, const notepads_markdown_core_cancel* cancel,
        const notepads_markdown_preview_key* key, const notepads_markdown_preview_disclosure_override* overrides, uint32_t override_count,
        uint32_t purpose, uint32_t section_heading, notepads_markdown_preview_layout** output, notepads_markdown_core_error* error);
    int32_t NOTEPADS_MARKDOWN_FFI_CALL notepads_markdown_preview_visibility(notepads_markdown_preview_layout* layout,
        notepads_markdown_preview_visibility_result* output, notepads_markdown_core_error* error);
    int32_t NOTEPADS_MARKDOWN_FFI_CALL notepads_markdown_preview_semantic_batch(notepads_markdown_preview_layout* layout, uint64_t start,
        uint32_t count, notepads_markdown_preview_summary* output, notepads_markdown_core_error* error);
    int32_t NOTEPADS_MARKDOWN_FFI_CALL notepads_markdown_preview_measure_begin(notepads_markdown_preview_layout* layout, uint32_t maximum,
        notepads_markdown_preview_summary* output, notepads_markdown_core_error* error);
    // At most 256 fixed rows or 65536 blob bytes. Exact row_size; byte blob uses 1.
    // read_result.written/next are row counts/indices, or bytes for BLOB.
    // DISCLOSURES and HIDDEN_RANGES are independent of the current batch.
    int32_t NOTEPADS_MARKDOWN_FFI_CALL notepads_markdown_preview_read(notepads_markdown_preview_layout* layout, uint32_t table, uint64_t start,
        uint64_t count, uint32_t row_size, void* buffer, notepads_markdown_core_read_result* output, notepads_markdown_core_error* error);
    int32_t NOTEPADS_MARKDOWN_FFI_CALL notepads_markdown_preview_submit(notepads_markdown_preview_layout* layout, const notepads_markdown_preview_key* key,
        uint64_t start, const notepads_markdown_preview_metric* metrics, uint32_t count, notepads_markdown_core_error* error);
    int32_t NOTEPADS_MARKDOWN_FFI_CALL notepads_markdown_preview_finish(notepads_markdown_preview_layout* layout, notepads_markdown_preview_extent* output, notepads_markdown_core_error* error);
    /* Opt into draft presentation until the next measure call. Zero blocks means no prefix covers bottom. */
    int32_t NOTEPADS_MARKDOWN_FFI_CALL notepads_markdown_preview_begin_prefix(notepads_markdown_preview_layout* layout,
                                                                              double bottom, notepads_markdown_preview_extent* output,
                                                                              notepads_markdown_core_error* error);
    int32_t NOTEPADS_MARKDOWN_FFI_CALL notepads_markdown_preview_visible(notepads_markdown_preview_layout* layout, const notepads_markdown_preview_viewport* viewport,
        notepads_markdown_preview_range* output, notepads_markdown_core_error* error);
    int32_t NOTEPADS_MARKDOWN_FFI_CALL notepads_markdown_preview_present(notepads_markdown_preview_layout* layout, uint64_t start, uint32_t count,
        notepads_markdown_preview_summary* output, notepads_markdown_core_error* error);
    int32_t NOTEPADS_MARKDOWN_FFI_CALL notepads_markdown_preview_reserve(notepads_markdown_preview_layout* layout, uint64_t bytes,
        notepads_markdown_preview_allocation** output, notepads_markdown_core_error* error);
    int32_t NOTEPADS_MARKDOWN_FFI_CALL notepads_markdown_preview_admit(notepads_markdown_preview_layout* layout, notepads_markdown_preview_allocation* allocation,
        uint64_t capacity_bytes, notepads_markdown_core_error* error);
    int32_t NOTEPADS_MARKDOWN_FFI_CALL notepads_markdown_preview_allocation_destroy(notepads_markdown_preview_allocation* allocation);
    // Source-independent native funding in the accepted model's budget root.
    // Model/cancel must share a root; allocation remains owned until destroy,
    // even after model/cancel/layout destruction. Free native bytes before lease.
    int32_t NOTEPADS_MARKDOWN_FFI_CALL notepads_markdown_preview_model_reserve(const notepads_markdown_core_model* model, const notepads_markdown_core_cancel* cancel,
        uint64_t bytes, notepads_markdown_preview_allocation** output, notepads_markdown_core_error* error);
    int32_t NOTEPADS_MARKDOWN_FFI_CALL notepads_markdown_preview_model_admit(const notepads_markdown_core_model* model, const notepads_markdown_core_cancel* cancel,
        notepads_markdown_preview_allocation* allocation, uint64_t capacity_bytes, notepads_markdown_core_error* error);
    int32_t NOTEPADS_MARKDOWN_FFI_CALL notepads_markdown_preview_layout_destroy(notepads_markdown_preview_layout* layout);
#ifdef __cplusplus
}
#endif
