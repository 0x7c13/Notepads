// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE file in the project root for license information.
// ---------------------------------------------------------------------------------------------
#pragma once
#include "notepads/markdown/ffi.h"

// Private synchronous whole-fill coverage ABI. All pointers are borrowed for
// this call only. Native must not retain commands, paths, arena, or result.
enum
{
    NOTEPADS_MARKDOWN_SVG_COVERAGE_ABI = 1,
    NOTEPADS_MARKDOWN_SVG_COVERAGE_MAX_PATHS = 32,
    NOTEPADS_MARKDOWN_SVG_COVERAGE_MOVE = 0,
    NOTEPADS_MARKDOWN_SVG_COVERAGE_LINE = 1,
    NOTEPADS_MARKDOWN_SVG_COVERAGE_QUAD = 2,
    NOTEPADS_MARKDOWN_SVG_COVERAGE_CUBIC = 3,
    NOTEPADS_MARKDOWN_SVG_COVERAGE_CLOSE = 4,
    NOTEPADS_MARKDOWN_SVG_COVERAGE_COORDINATE_DOMAIN = 1,
    NOTEPADS_MARKDOWN_SVG_COVERAGE_PRECISION_DOMAIN = 2,
    NOTEPADS_MARKDOWN_SVG_COVERAGE_FONT_FORMAT_DOMAIN = 3
};

struct NOTEPADS_MARKDOWN_FFI_ALIGN8 MsSvgCoverageCommand
{
    uint32_t verb, reserved;
    // Original f32 bits. Move/Line use 2, Quad 4, Cubic 6, Close 0 entries.
    // All unused coordinates and reserved fields are zero.
    uint32_t xy_bits[6];
};
struct NOTEPADS_MARKDOWN_FFI_ALIGN8 MsSvgCoveragePath
{
    uint32_t command_start, command_count, fill_rule, reserved;
    // M: local geometry to the actual current t surface; sx,ky,kx,sy,tx,ty.
    uint32_t matrix_bits[6];
    // Positive finite original f32 values, s>=t. Native uses the exact s/t.
    uint32_t s_bits, t_bits;
    // Coverage uses the entire current surface, with target_x/y=0.
    int32_t target_x, target_y;
    uint32_t width, height;
    uint64_t a8_offset, stride, length;
};
struct NOTEPADS_MARKDOWN_FFI_ALIGN8 MsSvgCoverageBatch
{
    uint32_t size, version, path_count, command_count;
    MsSvgCoveragePath const *paths;
#if INTPTR_MAX == INT32_MAX
    uint32_t paths_padding;
#endif
    MsSvgCoverageCommand const *commands;
#if INTPTR_MAX == INT32_MAX
    uint32_t commands_padding;
#endif
    uint8_t *a8;
#if INTPTR_MAX == INT32_MAX
    uint32_t a8_padding;
#endif
    uint64_t a8_capacity;
};
struct NOTEPADS_MARKDOWN_FFI_ALIGN8 MsSvgCoverageResult
{
    uint32_t size, version, ready_mask, ineligible_mask;
    uint32_t reasons[NOTEPADS_MARKDOWN_SVG_COVERAGE_MAX_PATHS];
    uint32_t completed, reserved;
};
struct NOTEPADS_MARKDOWN_FFI_ALIGN8 MsSvgCoverageCallbacks
{
    uint32_t size, version;
    uint64_t session;
    int32_t(NOTEPADS_MARKDOWN_FFI_CALL *cover_batch)(uint64_t, MsSvgCoverageBatch const *, MsSvgCoverageResult *);
#if INTPTR_MAX == INT32_MAX
    uint32_t callback_padding;
#endif
};

// Callback.session is nonzero, equals the original font callback session, and
// resolves on the same worker using the same pinned resource account. Every
// path in one batch has the same s_bits/t_bits. Strict S_OK is the only success;
// the original account's typed first fault takes precedence over derived HRs.
// Cleanup failure is independent and must never overwrite an earlier work fault.
// NonZero=0, EvenOdd=1. Count 0 is valid; nonempty spans must be valid, checked
// and nonaliasing. Output spans are pairwise disjoint, stride>=width, checked
// needed=(width&&height)?stride*(height-1)+width:0; needed<=length and checked
// offset+length<=capacity. Do not evaluate height-1 for an empty target. Whole
// output spans [offset,offset+length) are disjoint, not merely their live rows.
// Native writes every logical A8 pixel on Ready; padding is not read or written.
// Success: completed=1; Ready/Ineligible disjoint and partition all path bits;
// reasons are one of the three domains only on Ineligible, otherwise zero.
// The caller initializes Result.size/version and zeros its remaining payload.
// Failure after a valid header never commits; masks/reasons/completed stay zero.
// A malformed header/span may be rejected without touching untrusted output.
// No failed output may be consumed.
// Cancel/Quota/OS/writer/cleanup errors never become representation fallback.
// Command spans may share immutable input; unused fields/padding must be zero.
// All work and all physical allocations use the original same-worker account.
static_assert(sizeof(MsSvgCoverageCommand) == 32 && alignof(MsSvgCoverageCommand) == 8);
static_assert(sizeof(MsSvgCoveragePath) == 88 && alignof(MsSvgCoveragePath) == 8);
static_assert(sizeof(MsSvgCoverageBatch) == 48 && alignof(MsSvgCoverageBatch) == 8);
static_assert(sizeof(MsSvgCoverageResult) == 152 && alignof(MsSvgCoverageResult) == 8);
static_assert(sizeof(MsSvgCoverageCallbacks) == 24 && alignof(MsSvgCoverageCallbacks) == 8);
static_assert(offsetof(MsSvgCoveragePath, matrix_bits) == 16);
static_assert(offsetof(MsSvgCoveragePath, s_bits) == 40);
static_assert(offsetof(MsSvgCoveragePath, a8_offset) == 64);
static_assert(offsetof(MsSvgCoverageBatch, paths) == 16);
static_assert(offsetof(MsSvgCoverageBatch, commands) == 24);
static_assert(offsetof(MsSvgCoverageBatch, a8) == 32);
static_assert(offsetof(MsSvgCoverageBatch, a8_capacity) == 40);
static_assert(offsetof(MsSvgCoverageResult, completed) == 144);
static_assert(offsetof(MsSvgCoverageCallbacks, cover_batch) == 16);
