// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE file in the project root for license information.
// ---------------------------------------------------------------------------------------------

use super::super::{WireHeader, WireIdentity, WireRows};

pub const PREVIEW_ABI_VERSION: u32 = 3;
pub const PREVIEW_SCRIPT_ABI_VERSION: u32 = 1;
pub const PREVIEW_HTML_CELLS: u32 = 12;
pub const PREVIEW_DISCLOSURES: u32 = 13;
pub const PREVIEW_HIDDEN_RANGES: u32 = 14;
pub const PREVIEW_RUBY_UNITS: u32 = 15;
pub const PREVIEW_LAYOUT_VISIBLE: u32 = 0;
pub const PREVIEW_LAYOUT_MARKERS: u32 = 1;
pub const PREVIEW_LAYOUT_SEMANTICS: u32 = 2;
pub const PREVIEW_HTML_CELL_HTML: u32 = 1;
pub const PREVIEW_HTML_CELL_COVERED: u32 = 2;
pub const PREVIEW_HTML_CELL_CAPTION: u32 = 4;
pub const PREVIEW_HTML_CELL_MASK: u32 = 7;

pub fn prefix<T>() -> WireHeader {
    WireHeader {
        abi_version: PREVIEW_ABI_VERSION,
        struct_size: size_of::<T>() as u32,
    }
}

macro_rules! pod {
    ($name:ident { $($field:ident : $ty:ty),* $(,)? }) => {
        #[repr(C, align(8))]
        #[derive(Clone, Copy, Debug, Default, PartialEq)]
        pub struct $name { $(pub $field: $ty),* }
    };
}

pod!(PreviewBlob {
    start: u64,
    length: u64
});
pod!(PreviewKey {
    header: WireHeader,
    model: WireIdentity,
    layout_generation: u64,
    theme_generation: u64,
    measurement_generation: u64,
    width: f64,
    indent: f64,
    gap: f64,
});
pod!(PreviewSummary {
    header: WireHeader,
    key: PreviewKey,
    start: u64,
    blocks: u64,
    texts: u64,
    runs: u64,
    cells: u64,
    decorations: u64,
    images: u64,
    links: u64,
    blob_bytes: u64,
    ruby_units: u64,
});
pod!(PreviewVisibility {
    header: WireHeader,
    key: PreviewKey,
    disclosures: u64,
    hidden_ranges: u64,
    blocks: u64,
});
pod!(PreviewDisclosure {
    node: u32,
    summary: u32,
    expanded: u32,
    first_body_node: u32,
    next_node: u32,
    reserved: u32,
    summary_start: u64,
    body_start: u64,
    body_end: u64,
});
pod!(PreviewHiddenRange {
    start: u64,
    end: u64
});
pod!(PreviewBlock {
    node: u32,
    kind: u32,
    logical_start: u64,
    logical_end: u64,
    x: f64,
    width: f64,
    y: f64,
    height: f64,
    quote_depth: u32,
    item_depth: u32,
    heading_level: u32,
    columns: u32,
    row_count: u32,
    alignment: u32,
    texts: WireRows,
    cells: WireRows,
    decorations: WireRows,
    code_info: PreviewBlob,
    code_first_line_number: u32,
    code_reserved: u32,
    code_highlight_lines: PreviewBlob,
});
pod!(PreviewText {
    node: u32,
    reserved: u32,
    logical_start: u64,
    logical_end: u64,
    utf16_length: u64,
    utf8: PreviewBlob,
    runs: WireRows,
    images: WireRows,
});
pod!(PreviewRun {
    start: u32,
    end: u32,
    flags: u32,
    link: u32,
    size_scale: f64,
    relative_em: f64,
    absolute_dip: f64,
});
pod!(PreviewRubyUnit {
    text: u32,
    start: u32,
    base_end: u32,
    end: u32,
});
pod!(PreviewScriptScope {
    text: u32,
    start: u32,
    end: u32,
    font_flags: u32,
    size_scale: f64,
    relative_em: f64,
    absolute_dip: f64,
});
pod!(PreviewScriptSummary {
    header: WireHeader,
    key: PreviewKey,
    start: u64,
    rows: u64,
});
pod!(PreviewCell {
    node: u32,
    row_node: u32,
    row: u32,
    column: u32,
    alignment: u32,
    header: u32,
    missing: u32,
    text: u32,
});
pod!(PreviewHtmlCell {
    node: u32,
    colspan: u32,
    flags: u32,
    reserved: u32,
    rowspan: u32,
    reserved_geometry: u32,
});
pod!(PreviewDecoration {
    item: u32,
    list: u32,
    kind: u32,
    depth: u32,
    ordinal: u64,
    checked: u32,
    object: u32,
    marker: u32,
    has_action: u32,
    action_start: u64,
    action_end: u64,
    expected: u32,
    reserved: u32,
    logical_start: u64,
    logical_end: u64,
});
pod!(PreviewImage {
    object: u32,
    node: u32,
    text: u32,
    offset_utf16: u32,
    width: u32,
    height: u32,
    destination: PreviewBlob,
    title: PreviewBlob,
    alt: PreviewBlob,
    light_source: PreviewBlob,
    dark_source: PreviewBlob,
});
pod!(PreviewLink {
    node: u32,
    local_position_plus_one: u32,
    local_heading_node: u32,
    wiki: u32,
    destination: PreviewBlob,
    title: PreviewBlob
});
pod!(PreviewMetric {
    node: u32,
    reserved: u32,
    height: f64
});
pod!(PreviewExtent {
    header: WireHeader,
    key: PreviewKey,
    blocks: u64,
    height: f64
});
pod!(PreviewViewport {
    header: WireHeader,
    key: PreviewKey,
    y: f64,
    height: f64,
    overscan: f64
});
pod!(PreviewRange {
    header: WireHeader,
    start: u64,
    count: u64
});

const _: () = {
    assert!(size_of::<PreviewBlob>() == 16);
    assert!(size_of::<PreviewKey>() == 96);
    assert!(size_of::<PreviewSummary>() == 184);
    assert!(size_of::<PreviewRubyUnit>() == 16 && align_of::<PreviewRubyUnit>() == 8);
    assert!(size_of::<PreviewVisibility>() == 128);
    assert!(size_of::<PreviewDisclosure>() == 48);
    assert!(size_of::<PreviewHiddenRange>() == 16);
    assert!(size_of::<PreviewBlock>() == 144);
    assert!(size_of::<PreviewText>() == 64);
    assert!(size_of::<PreviewRun>() == 40 && align_of::<PreviewRun>() == 8);
    assert!(size_of::<PreviewScriptScope>() == 40 && align_of::<PreviewScriptScope>() == 8);
    assert!(size_of::<PreviewScriptSummary>() == 120 && align_of::<PreviewScriptSummary>() == 8);
    assert!(std::mem::offset_of!(PreviewRun, size_scale) == 16);
    assert!(std::mem::offset_of!(PreviewRun, absolute_dip) == 32);
    assert!(std::mem::offset_of!(PreviewScriptScope, font_flags) == 12);
    assert!(size_of::<PreviewCell>() == 32);
    assert!(size_of::<PreviewHtmlCell>() == 24 && align_of::<PreviewHtmlCell>() == 8);
    assert!(std::mem::offset_of!(PreviewHtmlCell, colspan) == 4);
    assert!(std::mem::offset_of!(PreviewHtmlCell, flags) == 8);
    assert!(std::mem::offset_of!(PreviewHtmlCell, reserved) == 12);
    assert!(size_of::<PreviewDecoration>() == 80);
    assert!(size_of::<PreviewImage>() == 104);
    assert!(size_of::<PreviewLink>() == 48);
    assert!(size_of::<PreviewMetric>() == 16);
    assert!(size_of::<PreviewExtent>() == 120);
    assert!(size_of::<PreviewViewport>() == 128);
    assert!(size_of::<PreviewRange>() == 24);
    assert!(align_of::<PreviewBlock>() == 8);
};
