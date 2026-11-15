// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE file in the project root for license information.
// ---------------------------------------------------------------------------------------------
//! Private font-service ABI3 mirror. ISO15924 script tags and OpenType axis tags are
//! distinct domains. All pointers are borrowed only within a validated callback.
use core::ffi::c_void;

pub const ABI_VERSION: u32 = 3;
/// New function table; the contained font PODs retain their own version 3.
pub const FONT_CALLBACKS_VERSION: u32 = 1;
pub const FONT: u32 = 1;
pub const METRICS: u32 = 2;
pub const GEOMETRY: u32 = 3;
pub const PIXELS: u32 = 5;
pub const SCRATCH: u32 = 6;

#[repr(C)]
#[derive(Clone, Copy, Debug, Default)]
pub struct Axis {
    pub tag_be: u32,
    pub value: f32,
}
#[repr(C)]
#[derive(Clone, Copy, Debug, Default)]
pub struct File {
    pub ticket: u32,
    pub collection_faces: u32,
    pub byte_length: u64,
    pub sha256: [u8; 32],
}
#[repr(C)]
#[derive(Clone, Copy, Debug, Default)]
pub struct GlobalMetrics {
    pub units_per_em: u32,
    pub ascent: i32,
    pub descent: i32,
    pub line_gap: i32,
    pub cap_height: i32,
    pub x_height: i32,
    pub underline_position: i32,
    pub underline_thickness: i32,
    pub strike_position: i32,
    pub strike_thickness: i32,
}
#[repr(C)]
#[derive(Clone, Copy, Debug, Default)]
pub struct Face {
    pub ticket: u32,
    pub file_ticket: u32,
    pub face_index: u32,
    pub simulations: u32,
    pub units_per_em: u32,
    pub glyph_count: u32,
    pub axis_start: u32,
    pub axis_count: u32,
    pub metrics: GlobalMetrics,
}
#[repr(C)]
#[derive(Clone, Copy, Debug, Default)]
pub struct LogicalRun {
    pub utf8_start: u32,
    pub utf8_length: u32,
    pub face_ticket: u32,
    pub script_be: u32,
    pub language_start: u32,
    pub language_length: u32,
    pub bidi_level: u32,
    pub orientation: u32,
    pub fallback_scale: f32,
}
#[repr(C)]
#[derive(Clone, Copy, Debug, Default)]
pub struct Style {
    pub utf8_start: u32,
    pub utf8_length: u32,
    pub family_start: u32,
    pub family_length: u32,
    pub language_start: u32,
    pub language_length: u32,
    pub weight: u32,
    pub style: u32,
    pub stretch: u32,
    pub direction: u32,
    pub axis_start: u32,
    pub axis_count: u32,
    pub optical_sizing: u32,
    pub size: f32,
}
#[repr(C)]
#[derive(Clone, Copy, Debug, Default)]
pub struct LogicalSpan {
    pub utf8_start: u32,
    pub utf8_length: u32,
    pub script_be: u32,
    pub language_start: u32,
    pub language_length: u32,
    pub bidi_level: u32,
    pub orientation: u32,
}
#[repr(C)]
#[derive(Clone, Copy, Debug)]
pub struct Draft {
    pub size: u32,
    pub version: u32,
    pub draft_id: u64,
    pub text: *const u8,
    pub text_length: u32,
    pub styles: *const Style,
    pub style_count: u32,
    pub axes: *const Axis,
    pub axis_count: u32,
    pub logical_spans: *const LogicalSpan,
    pub logical_span_count: u32,
    pub strings: *const u8,
    pub strings_length: u32,
    pub element_id_start: u32,
    pub element_id_length: u32,
}
#[repr(C)]
#[derive(Clone, Copy, Debug)]
pub struct Plan {
    pub size: u32,
    pub version: u32,
    pub generation: u64,
    pub files: *const File,
    pub file_count: u32,
    pub faces: *const Face,
    pub face_count: u32,
    pub axes: *const Axis,
    pub axis_count: u32,
    pub runs: *const LogicalRun,
    pub run_count: u32,
    pub strings: *const u8,
    pub strings_length: u32,
}
#[repr(C)]
#[derive(Clone, Copy, Debug, Default)]
pub struct GlyphMetrics {
    pub left_side_bearing: i32,
    pub advance_width: u32,
    pub right_side_bearing: i32,
    pub top_side_bearing: i32,
    pub advance_height: u32,
    pub bottom_side_bearing: i32,
    pub vertical_origin_y: i32,
}
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Hash)]
pub struct GlyphKey {
    pub face_ticket: u32,
    pub glyph_id: u32,
    pub orientation: u32,
    pub reserved: u32,
}
#[repr(C)]
#[derive(Clone, Copy, Debug, Default)]
pub struct Command {
    pub kind: u32,
    pub x1: f32,
    pub y1: f32,
    pub x2: f32,
    pub y2: f32,
    pub x3: f32,
    pub y3: f32,
}
#[repr(C)]
#[derive(Clone, Copy, Debug, Default)]
pub struct GlyphPath {
    pub key_index: u32,
    pub command_start: u32,
    pub command_count: u32,
    pub fill_rule: u32,
}
#[repr(C)]
#[derive(Clone, Copy, Debug, Default)]
pub struct GeometryReceipt {
    pub size: u32,
    pub version: u32,
    pub generation: u64,
    pub glyph_count: u32,
    pub command_count: u32,
    pub retained_native_bytes: u64,
}
#[repr(C)]
#[derive(Clone, Copy)]
pub struct Callbacks {
    pub size: u32,
    pub version: u32,
    pub session: u64,
    pub plan: Option<unsafe extern "C" fn(u64, *const Draft, *mut Plan) -> i32>,
    pub fill_font: Option<unsafe extern "C" fn(u64, u32, *mut u8, u64, *mut u64) -> i32>,
    pub fill_metrics:
        Option<unsafe extern "C" fn(u64, u32, u32, *mut GlyphMetrics, u32, *mut u32) -> i32>,
    pub prepare_geometry:
        Option<unsafe extern "C" fn(u64, *const GlyphKey, u32, *mut GeometryReceipt) -> i32>,
    pub fill_geometry: Option<
        unsafe extern "C" fn(
            u64,
            u64,
            *mut GlyphPath,
            u32,
            *mut Command,
            u32,
            *mut u32,
            *mut u32,
        ) -> i32,
    >,
    pub retire_geometry: Option<unsafe extern "C" fn(u64, u64) -> i32>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Failure {
    Contract,
    Allocation,
    Native(i32),
}
pub type Result<T> = core::result::Result<T, Failure>;
impl Callbacks {
    pub fn validate(self) -> Result<Self> {
        if self.size as usize != size_of::<Self>()
            || self.version != FONT_CALLBACKS_VERSION
            || self.session == 0
            || self.plan.is_none()
            || self.fill_font.is_none()
            || self.fill_metrics.is_none()
            || self.prepare_geometry.is_none()
            || self.fill_geometry.is_none()
            || self.retire_geometry.is_none()
        {
            return Err(Failure::Contract);
        }
        Ok(self)
    }
}
const _: () = {
    assert!(size_of::<Axis>() == 8);
    assert!(size_of::<File>() == 48);
    assert!(size_of::<GlobalMetrics>() == 40);
    assert!(size_of::<Face>() == 72);
    assert!(size_of::<LogicalRun>() == 36);
    assert!(size_of::<Style>() == 56);
    assert!(size_of::<LogicalSpan>() == 28);

    assert!(size_of::<GlyphMetrics>() == 28);
    assert!(size_of::<Command>() == 28);
    assert!(size_of::<GlyphKey>() == 16);
    assert!(size_of::<GlyphPath>() == 16);
    assert!(size_of::<GeometryReceipt>() == 32);
};
#[cfg(target_pointer_width = "64")]
const _: () = {
    assert!(size_of::<*const c_void>() == 8);
    assert!(size_of::<Draft>() == 104);
    assert!(align_of::<Draft>() == 8);
    assert!(size_of::<Plan>() == 96);
    assert!(align_of::<Plan>() == 8);
    assert!(size_of::<Callbacks>() == 64);
    assert!(align_of::<Callbacks>() == 8);
    assert!(core::mem::offset_of!(Draft, element_id_start) == 92);
    assert!(core::mem::offset_of!(Draft, element_id_length) == 96);
};
#[cfg(target_pointer_width = "32")]
const _: () = {
    assert!(size_of::<*const c_void>() == 4);
    assert!(size_of::<Draft>() == 64);
    assert!(align_of::<Draft>() == 8);
    assert!(size_of::<Plan>() == 56);
    assert!(align_of::<Plan>() == 8);
    assert!(size_of::<Callbacks>() == 40);
    assert!(align_of::<Callbacks>() == 8);
    assert!(core::mem::offset_of!(Draft, element_id_start) == 56);
    assert!(core::mem::offset_of!(Draft, element_id_length) == 60);
};
