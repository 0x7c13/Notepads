// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE file in the project root for license information.
// ---------------------------------------------------------------------------------------------
#pragma once
#include "notepads/markdown/core.h"

// Private immutable highlighting ABI1, independent of Core ABI3/Preview ABI2.
// No input text or third-party types cross the boundary. Build uses a dedicated
// same-root Operation and a source cancellation flag, not a layout Operation.
// The caller serializes calls on each result, including reads and destruction.
// A panic poisons that result. Matching POD prefixes authorize full reads;
// prefix rejection is nonpoisoning. Handle outputs are nulled after preflight.
// Summary remains unchanged on failure. Copy buffers may be partially written
// after semantic entry: discard ALL output on any nonzero status. All caller
// regions must be aligned, valid, nonoverlapping, and borrowed until return.
// A result owns bounded tokens and its accepted-model pin; it can outlive source
// handles, but never grants permission to publish an obsolete source/layout.
// A lexical cache retains its budget root and bounded exact text/tokens, never a
// model or cancel flag. Serialize cache calls, including destruction, on one worker.
// A panic poisons the cache; ordinary validation/cancellation failures do not.
// Selected builds accept 1..64 sorted, unique, nonzero main-flow CodeBlock node
// IDs, including IDs beyond the first 64 blocks. All handles must share a budget
// root. The cache can be reused across accepted models on that root. A successful
// build writes the number of reused complete blocks separately; failures leave
// that count unchanged and publish no snapshot. Existing summary layout is fixed.
enum { NOTEPADS_MARKDOWN_HIGHLIGHT_ABI_VERSION = 1, NOTEPADS_MARKDOWN_HIGHLIGHT_MAX_BLOCKS = 64, NOTEPADS_MARKDOWN_HIGHLIGHT_MAX_SPANS = 131072 };
enum { NOTEPADS_MARKDOWN_HIGHLIGHT_MAX_LANGUAGE = 50 };
enum notepads_markdown_highlight_role { NOTEPADS_MARKDOWN_HIGHLIGHT_PLAIN = 0, NOTEPADS_MARKDOWN_HIGHLIGHT_COMMENT = 1, NOTEPADS_MARKDOWN_HIGHLIGHT_STRING = 2,
    NOTEPADS_MARKDOWN_HIGHLIGHT_NUMBER = 3, NOTEPADS_MARKDOWN_HIGHLIGHT_KEYWORD = 4, NOTEPADS_MARKDOWN_HIGHLIGHT_TYPE = 5, NOTEPADS_MARKDOWN_HIGHLIGHT_FUNCTION = 6,
    NOTEPADS_MARKDOWN_HIGHLIGHT_ADDED = 7, NOTEPADS_MARKDOWN_HIGHLIGHT_REMOVED = 8, NOTEPADS_MARKDOWN_HIGHLIGHT_ROLE_COUNT = 9 };
enum notepads_markdown_highlight_state { NOTEPADS_MARKDOWN_HIGHLIGHT_PENDING = 0, NOTEPADS_MARKDOWN_HIGHLIGHT_COMPLETE = 1,
    NOTEPADS_MARKDOWN_HIGHLIGHT_SKIPPED = 2, NOTEPADS_MARKDOWN_HIGHLIGHT_FAILED = 3 };
// Skipped reasons1..6: unknown language, block bytes, lines, line bytes,
// participating document bytes, span count. Failed reasons1..4: compile,
// regex search, grammar, scope. Language1..45: C++, Rust, JSON, JS, Python,
// YAML, Shell, C#, HTML, CSS, SQL, Diff, Go, Java, Ruby, XML, Lua, Makefile,
// C, R, Scala, Markdown, Batchfile, Perl, Haskell, Objective-C, Objective-C++.
// PHP, HTML+PHP, TeX, Clojure, Erlang, Groovy, OCaml, Common Lisp, Tcl,
// Java Properties, Graphviz DOT, MATLAB, Pascal, reStructuredText,
// AppleScript, D, Haml, HTML+ERB.
typedef struct notepads_markdown_highlight_snapshot notepads_markdown_highlight_snapshot;
typedef struct notepads_markdown_highlight_cache notepads_markdown_highlight_cache;
typedef struct NOTEPADS_MARKDOWN_FFI_ALIGN8 notepads_markdown_highlight_header { uint32_t abi_version; uint32_t struct_size; } notepads_markdown_highlight_header;
typedef struct NOTEPADS_MARKDOWN_FFI_ALIGN8 notepads_markdown_highlight_block
{
    uint32_t node;
    uint32_t state;
    uint32_t reason;
    uint32_t language;
    uint64_t logical_start;
    uint64_t logical_end;
    uint32_t spans_start;
    uint32_t spans_count;
} notepads_markdown_highlight_block;
typedef struct NOTEPADS_MARKDOWN_FFI_ALIGN8 notepads_markdown_highlight_span
{
    uint32_t start_utf16;
    uint32_t length_utf16;
    uint32_t role;
    uint32_t reserved;
} notepads_markdown_highlight_span;
typedef struct NOTEPADS_MARKDOWN_FFI_ALIGN8 notepads_markdown_highlight_summary
{
    notepads_markdown_highlight_header header;
    notepads_markdown_core_identity model;
    uint32_t blocks;
    uint32_t spans;
    uint32_t complete_blocks;
    uint32_t skipped_blocks;
    uint32_t failed_blocks;
    uint32_t entry_limited;
    uint64_t participating_utf8;
    uint64_t external_events;
} notepads_markdown_highlight_summary;
#ifdef __cplusplus
static_assert(sizeof(notepads_markdown_highlight_block) == 40 && sizeof(notepads_markdown_highlight_span) == 16);
static_assert(sizeof(notepads_markdown_highlight_summary) == 88 && offsetof(notepads_markdown_highlight_summary, external_events) == 80);
extern "C" {
#endif
uint32_t NOTEPADS_MARKDOWN_FFI_CALL notepads_markdown_highlight_abi_version(void);
int32_t NOTEPADS_MARKDOWN_FFI_CALL notepads_markdown_highlight_cache_create(const notepads_markdown_core_model *model, const notepads_markdown_core_cancel *cancel,
    notepads_markdown_highlight_cache **output, notepads_markdown_core_error *error);
int32_t NOTEPADS_MARKDOWN_FFI_CALL notepads_markdown_highlight_build_selected(const notepads_markdown_core_model *model, const notepads_markdown_core_cancel *cancel,
    const uint32_t *nodes, uint32_t count, notepads_markdown_highlight_cache *cache, notepads_markdown_highlight_snapshot **output,
    uint32_t *reused, notepads_markdown_core_error *error);
int32_t NOTEPADS_MARKDOWN_FFI_CALL notepads_markdown_highlight_cache_destroy(notepads_markdown_highlight_cache *cache);
int32_t NOTEPADS_MARKDOWN_FFI_CALL notepads_markdown_highlight_build(const notepads_markdown_core_model *model, const notepads_markdown_core_cancel *cancel,
    notepads_markdown_highlight_snapshot **output, notepads_markdown_core_error *error);
int32_t NOTEPADS_MARKDOWN_FFI_CALL notepads_markdown_highlight_get_summary(notepads_markdown_highlight_snapshot *snapshot,
    notepads_markdown_highlight_summary *output, notepads_markdown_core_error *error);
int32_t NOTEPADS_MARKDOWN_FFI_CALL notepads_markdown_highlight_copy(notepads_markdown_highlight_snapshot *snapshot,
    notepads_markdown_highlight_block *blocks, uint32_t block_capacity, uint32_t block_size,
    notepads_markdown_highlight_span *spans, uint32_t span_capacity, uint32_t span_size,
    notepads_markdown_highlight_summary *output, notepads_markdown_core_error *error);
int32_t NOTEPADS_MARKDOWN_FFI_CALL notepads_markdown_highlight_destroy(notepads_markdown_highlight_snapshot *snapshot);
#ifdef __cplusplus
}
#endif
