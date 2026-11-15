// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE file in the project root for license information.
// ---------------------------------------------------------------------------------------------
#pragma once
#include "notepads/markdown/ffi.h"

// Private synchronous font service: existing font POD version 3, independent
// callback-table version 1. Nonzero session is an opaque monotonic ticket, never
// an object address. All six callbacks run on the owning worker. Plan tables
// borrow through next plan entry; Rust copies their bounded actual data first.
// fill_font writes the final Rust owner; Native file fragments are <=64KiB and
// paired before callback return. Actual geometry stays paid until retirement.
// Native errors are recorded in the resources first-fault slot before return.
// Retire remains permitted after cancellation. No exception crosses callbacks.
enum { NOTEPADS_MARKDOWN_FR_ABI_VERSION = 3, NOTEPADS_MARKDOWN_SVG_FONT_CALLBACKS_VERSION = 1 };
struct MsFrAxis { uint32_t tag_be; float value; };
struct MsFrFile { uint32_t ticket, collection_faces; uint64_t byte_length; uint8_t sha256[32]; };
struct MsFrGlobalMetrics
{
    uint32_t units_per_em;
    int32_t ascent, descent, line_gap, cap_height, x_height;
    int32_t underline_position, underline_thickness, strike_position, strike_thickness;
};
struct MsFrFace
{
    uint32_t ticket, file_ticket, face_index, simulations;
    uint32_t units_per_em, glyph_count, axis_start, axis_count;
    MsFrGlobalMetrics metrics;
};
struct MsFrLogicalRun
{
    uint32_t utf8_start, utf8_length, face_ticket, script_be;
    uint32_t language_start, language_length, bidi_level, orientation;
    float fallback_scale;
};
// All draft byte ranges index valid UTF-8 at scalar boundaries. Styles and logical
// spans separately partition the entire draft text, in logical order, without gaps
// or overlap. Each count <=65535; text/strings <=1MiB; axes <=4096. Empty drafts
// are valid only with zero styles/spans. Strings contain UTF-8 WITHOUT NUL.
// Script values use ISO15924 big-endian u32 (for example Latn=0x4C61746E);
// variation axis tags use OpenType big-endian u32. Numeric meanings are unchanged.
// Language is nonempty BCP47 UTF-8; absent SVG language becomes und. Style family
// is nonempty CSS/SVG selected family text; it is NOT an actual-face guess.
// The controlled producer preserves the ordered family list: Named values are
// double-quoted, quote/backslash use simple escapes, ASCII controls use six
// uppercase hex digits plus one space; the five parsed generic kinds are bare
// serif/sans-serif/cursive/fantasy/monospace. Entries are separated by comma+space.
// Quoted generic-looking names remain Named. Source CSS has already been decoded;
// Native decodes only this transport grammar, then selects each actual face.
// Only the canonical grammar above is accepted.
// Weight 1..1000, stretch 1..9; style 0=normal,1=oblique,2=italic. Size finite >0.
// Direction 0=LTR,1=RTL; optical sizing 0=none,1=auto; orientation 0=upright,
// 1=sideways. Bidi levels 0..125 are resolved by Rust over each whole paragraph
// INCLUDING SVG isolate/override semantics. Script and language are Rust-resolved;
// Native may split only for actual font coverage, preserving supplied values.
// Axes are finite, unique-tag requested SVG variation values; axis ranges index
// Draft.axes. Native returns the actual selected face axes, including auto opsz.
struct MsFrStyle
{
    uint32_t utf8_start, utf8_length, family_start, family_length;
    uint32_t language_start, language_length, weight, style, stretch, direction;
    uint32_t axis_start, axis_count, optical_sizing;
    float size;
};
struct MsFrLogicalSpan
{
    uint32_t utf8_start, utf8_length, script_be, language_start, language_length;
    uint32_t bidi_level, orientation;
};
struct MsFrDraft
{
    uint32_t size, version; uint64_t draft_id;
    uint8_t const* text; uint32_t text_length;
    MsFrStyle const* styles; uint32_t style_count;
    MsFrAxis const* axes; uint32_t axis_count;
    MsFrLogicalSpan const* logical_spans; uint32_t logical_span_count;
    uint8_t const* strings; uint32_t strings_length;
    // Exact actual semantic SVG Text::id(), encoded in this strings pool. Empty
    // length means no source id; offset MUST then be zero. Otherwise <=4096 bytes,
    // valid UTF8/scalar boundaries and no NUL. No generated id, case id, draft-order
    // key, text guess or production font-selector override may be transported here.
    uint32_t element_id_start, element_id_length;
};
struct MsFrPlan
{
    uint32_t size, version; uint64_t generation;
    MsFrFile const* files; uint32_t file_count;
    MsFrFace const* faces; uint32_t face_count;
    MsFrAxis const* axes; uint32_t axis_count;
    MsFrLogicalRun const* runs; uint32_t run_count;
    uint8_t const* strings; uint32_t strings_length;
};
struct MsFrGlyphMetrics
{
    int32_t left_side_bearing; uint32_t advance_width;
    int32_t right_side_bearing, top_side_bearing;
    uint32_t advance_height; int32_t bottom_side_bearing, vertical_origin_y;
};
struct MsFrGlyphKey { uint32_t face_ticket, glyph_id, orientation, reserved; };
struct MsFrCommand { uint32_t kind; float x1,y1,x2,y2,x3,y3; };
struct MsFrGlyphPath { uint32_t key_index, command_start, command_count, fill_rule; };
struct MsFrGeometryReceipt
{
    uint32_t size, version; uint64_t generation;
    uint32_t glyph_count, command_count; uint64_t retained_native_bytes;
};
struct MsSvgFontCallbacks
{
    uint32_t size, version; uint64_t session;
    int32_t (NOTEPADS_MARKDOWN_FFI_CALL* plan)(uint64_t, MsFrDraft const*, MsFrPlan*);
    int32_t (NOTEPADS_MARKDOWN_FFI_CALL* fill_font)(uint64_t,uint32_t,uint8_t*,uint64_t,uint64_t*);
    int32_t (NOTEPADS_MARKDOWN_FFI_CALL* fill_metrics)(uint64_t,uint32_t,uint32_t,MsFrGlyphMetrics*,uint32_t,uint32_t*);
    int32_t (NOTEPADS_MARKDOWN_FFI_CALL* prepare_geometry)(uint64_t,MsFrGlyphKey const*,uint32_t,MsFrGeometryReceipt*);
    int32_t (NOTEPADS_MARKDOWN_FFI_CALL* fill_geometry)(uint64_t,uint64_t,MsFrGlyphPath*,uint32_t,MsFrCommand*,uint32_t,uint32_t*,uint32_t*);
    int32_t (NOTEPADS_MARKDOWN_FFI_CALL* retire_geometry)(uint64_t,uint64_t);
};
static_assert(sizeof(MsFrAxis)==8 && sizeof(MsFrFile)==48);
static_assert(sizeof(MsFrGlobalMetrics)==40 && sizeof(MsFrFace)==72);
static_assert(sizeof(MsFrLogicalRun)==36 && sizeof(MsFrLogicalSpan)==28);
static_assert(sizeof(MsFrStyle)==56 && sizeof(MsFrGlyphMetrics)==28);
static_assert(sizeof(MsFrGlyphKey)==16 && sizeof(MsFrCommand)==28);
static_assert(sizeof(MsFrGlyphPath)==16 && sizeof(MsFrGeometryReceipt)==32);
#ifdef _WIN64
static_assert(sizeof(MsFrDraft)==104 && sizeof(MsFrPlan)==96);
static_assert(sizeof(MsSvgFontCallbacks)==64);
#else
static_assert(sizeof(MsFrDraft)==64 && sizeof(MsFrPlan)==56);
static_assert(sizeof(MsSvgFontCallbacks)==40);
#endif
