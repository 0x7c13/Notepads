// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE file in the project root for license information.
// ---------------------------------------------------------------------------------------------

use crate::identity::LogicalSpan;
use crate::ir::{Alignment, NodeId, ObjectId, Rows, TaskAction};
use crate::layout::BlockRequest;

pub const RUN_BOLD: u32 = 1;
pub const RUN_ITALIC: u32 = 2;
pub const RUN_STRIKE: u32 = 4;
pub const RUN_CODE: u32 = 8;
pub const RUN_INERT_HTML: u32 = 16;
pub const RUN_UNDERLINE: u32 = 32;
pub const RUN_MONOSPACE: u32 = 64;
pub const RUN_HIGHLIGHT: u32 = 128;

// ScriptScope has its own flag domain; bits 16 and 32 have separate meanings.
pub const SCRIPT_FONT_MASK: u32 = RUN_BOLD | RUN_ITALIC | RUN_CODE;
pub const SCRIPT_CODE_LINE_HEIGHT: u32 = 16;
pub const SCRIPT_SHAPE_ONLY: u32 = 32;
pub const SCRIPT_SCOPE_MASK: u32 = SCRIPT_FONT_MASK | SCRIPT_CODE_LINE_HEIGHT | SCRIPT_SHAPE_ONLY;

/// Bytes in this batch's blob, independently of any logical UTF-16 range.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct BlobSlice {
    pub start: u32,
    pub len: u32,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Block {
    pub request: BlockRequest,
    pub heading_level: u32,
    pub columns: u32,
    pub row_count: u32,
    pub texts: Rows<Text>,
    pub cells: Rows<Cell>,
    pub decorations: Rows<Decoration>,
    pub code_info: BlobSlice,
    pub code_first_line_number: u32,
    /// Pairs of little-endian u32 physical line numbers, inclusive and one-based.
    pub code_highlight_lines: BlobSlice,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Text {
    pub node: NodeId,
    pub logical: LogicalSpan,
    pub utf16_len: u32,
    pub value: BlobSlice,
    pub runs: Rows<Run>,
    pub images: Rows<Image>,
    pub derived: Rows<DerivedObject>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Run {
    pub start: u32,
    pub end: u32,
    pub flags: u32,
    pub link: Option<u32>,
}

/// Resolved ordered script nesting, relative to the owning text's parent font.
/// DIP displacement remains independent of the font size and display density.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ScriptStyle {
    pub size_scale: f64,
    pub relative_em: f64,
    pub absolute_dip: f64,
}

impl Default for ScriptStyle {
    fn default() -> Self {
        Self {
            size_scale: 1.0,
            relative_em: 0.0,
            absolute_dip: 0.0,
        }
    }
}

impl ScriptStyle {
    pub(crate) fn nested(self, kind: crate::ir::ScriptKind) -> Self {
        let (em, dip) = match kind {
            crate::ir::ScriptKind::Superscript => (-1.0 / 3.0, -1.0),
            crate::ir::ScriptKind::Subscript => (1.0 / 5.0, 1.0),
        };
        Self {
            size_scale: (self.size_scale * (5.0 / 6.0)).max(f64::MIN_POSITIVE),
            relative_em: self.relative_em + self.size_scale * em,
            absolute_dip: self.absolute_dip + dip,
        }
    }
}

/// Geometry for one non-default batch run, indexed independently of text rows.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ScriptRun {
    pub run: u32,
    pub style: ScriptStyle,
}

/// Accepted inline font boxes and real shaping containers, bound to their text.
/// Empty containers and intermediate ancestors retain their opening font state.
/// Scope flags use SCRIPT_SCOPE_MASK. SHAPE_ONLY contributes no font or line box;
/// its nonempty endpoints delimit shaping without splitting internal text atoms.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ScriptScope {
    pub text: u32,
    pub start: u32,
    pub end: u32,
    pub font_flags: u32,
    pub style: ScriptStyle,
}
/// One base and annotation pair in the owning text's local UTF-16 coordinates.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct RubyUnit {
    pub text: u32,
    pub start: u32,
    pub base_end: u32,
    pub end: u32,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Cell {
    pub node: NodeId,
    pub row_node: NodeId,
    pub row: u32,
    pub column: u32,
    pub header: bool,
    pub missing: bool,
    pub alignment: Alignment,
    pub text: u32,
    pub colspan: u32,
    pub rowspan: u32,
    pub html_flags: u32,
}

pub const HTML_CELL_HTML: u32 = 1;
pub const HTML_CELL_COVERED: u32 = 2;
pub const HTML_CELL_CAPTION: u32 = 4;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DecorationKind {
    Bullet,
    Ordered,
    Task,
    /// Disabled original HTML input. Never authorizes a GFM source edit.
    StaticHtmlCheckbox,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Decoration {
    pub item: NodeId,
    pub list: NodeId,
    pub kind: DecorationKind,
    pub depth: u32,
    pub ordinal: u64,
    pub marker: u8,
    pub checked: bool,
    pub task: Option<ObjectId>,
    pub action: Option<TaskAction>,
    /// Main UTF-16 object range; source action bytes remain a separate domain.
    pub logical: Option<LogicalSpan>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Image {
    pub object: ObjectId,
    pub node: NodeId,
    pub text: u32,
    pub offset_utf16: u32,
    pub destination: BlobSlice,
    pub title: BlobSlice,
    pub alt: BlobSlice,
    pub light_source: BlobSlice,
    pub dark_source: BlobSlice,
    pub wiki_size: Option<(u16, u16)>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct DerivedObject {
    pub object: ObjectId,
    pub node: NodeId,
    pub text: u32,
    pub offset_utf16: u32,
    pub kind: crate::ir::DerivedKind,
    pub input: BlobSlice,
    /// Original syntax from this batch's immutable model, not normalized input.
    pub export: BlobSlice,
    pub accessible_title: BlobSlice,
    pub accessible_description: BlobSlice,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Link {
    pub node: NodeId,
    pub local_position: Option<u64>,
    pub local_heading: Option<NodeId>,
    pub wiki: bool,
    pub destination: BlobSlice,
    pub title: BlobSlice,
}
