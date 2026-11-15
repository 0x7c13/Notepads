// Notepads Markdown bounded elementary-layout modifications: Copyright (c) 2026, Jiaqi (0x7c13) Liu.
use ratex_types::color::Color;
use ratex_types::execution::{Phase, Resource};
use ratex_types::path_command::PathCommand;
use ratex_types::storage::{BoundedClone, StorageError, StorageFamily, StorageLedger};

/// A TeX box: the fundamental unit of layout.
///
/// Every mathematical element is represented as a box with three dimensions:
/// - `width`: horizontal extent
/// - `height`: ascent above baseline
/// - `depth`: descent below baseline
///
/// All values are in **em** units relative to the current font size.
#[derive(Debug, Clone)]
pub struct LayoutBox {
    pub width: f64,
    pub height: f64,
    pub depth: f64,
    pub content: BoxContent,
    pub color: Color,
}

/// What a LayoutBox contains.
#[derive(Debug, Clone)]
pub enum BoxContent {
    /// Horizontal list of child boxes laid out left-to-right.
    HBox(Vec<LayoutBox>),

    /// Vertical list of child boxes laid out top-to-bottom.
    VBox(Vec<VBoxChild>),

    /// A single glyph character.
    Glyph {
        font_id: ratex_font::FontId,
        char_code: u32,
    },

    /// A shaped text run. Glyph positions are relative to the run origin.
    ///
    /// This is an internal layout primitive; `to_display_list` continues to
    /// flatten it to protocol-compatible `GlyphPath` items.
    GlyphRun { glyphs: Vec<RunGlyph> },

    /// Filled rectangle from `\rule[<raise>]{width}{height}`.
    /// `thickness` is the ink height; `raise` is the distance (in em) from the baseline
    /// to the bottom edge of the rectangle, positive toward the top of the line.
    Rule { thickness: f64, raise: f64 },

    /// Empty space (kern).
    Kern,

    /// A fraction: numerator over denominator with optional bar.
    Fraction {
        numer: Box<LayoutBox>,
        denom: Box<LayoutBox>,
        numer_shift: f64,
        denom_shift: f64,
        bar_thickness: f64,
        numer_scale: f64,
        denom_scale: f64,
    },

    /// Superscript/subscript layout.
    SupSub {
        base: Box<LayoutBox>,
        sup: Option<Box<LayoutBox>>,
        sub: Option<Box<LayoutBox>>,
        sup_shift: f64,
        sub_shift: f64,
        sup_scale: f64,
        sub_scale: f64,
        /// When true, place scripts centered on the base width (e.g. `\overbrace` / `\underbrace`).
        center_scripts: bool,
        /// Italic correction of the base character (em). Superscript x is offset by this amount
        /// beyond base.width, matching KaTeX's margin-right on italic math symbols.
        italic_correction: f64,
        /// Horizontal kern (em) applied to the subscript: KaTeX uses `margin-left: -base.italic` on
        /// `SymbolNode` bases so subscripts are not pushed out by the base's italic correction.
        sub_h_kern: f64,
    },

    /// A radical (square root).
    Radical {
        body: Box<LayoutBox>,
        index: Option<Box<LayoutBox>>,
        /// Horizontal offset (in em) of the surd/body from the left edge when index is present.
        index_offset: f64,
        /// `scriptscript` size relative to the surrounding math style (for drawing the index).
        index_scale: f64,
        rule_thickness: f64,
        inner_height: f64,
    },

    /// An operator with limits above/below (e.g. \sum_{i=0}^{n}).
    OpLimits {
        base: Box<LayoutBox>,
        sup: Option<Box<LayoutBox>>,
        sub: Option<Box<LayoutBox>>,
        base_shift: f64,
        sup_kern: f64,
        sub_kern: f64,
        slant: f64,
        sup_scale: f64,
        sub_scale: f64,
    },

    /// An accent above or below its base.
    Accent {
        base: Box<LayoutBox>,
        accent: Box<LayoutBox>,
        clearance: f64,
        skew: f64,
        is_below: bool,
        /// KaTeX `accentunder.js`: extra em gap between base bottom and under-accent (e.g. 0.12 for `\\utilde`).
        under_gap_em: f64,
    },

    /// A stretchy delimiter (\left, \right) wrapping inner content.
    LeftRight {
        left: Box<LayoutBox>,
        right: Box<LayoutBox>,
        inner: Box<LayoutBox>,
    },

    /// A rectangular matrix/array: every row has `col_widths.len()` cells,
    /// with missing trailing cells padded by empty boxes.
    Array {
        cells: Vec<Vec<LayoutBox>>,
        col_widths: Vec<f64>,
        /// Per-column alignment: b'l', b'c', or b'r'.
        col_aligns: Vec<u8>,
        row_heights: Vec<f64>,
        row_depths: Vec<f64>,
        /// Horizontal gap after each column. Length is `col_widths.len() - 1`.
        col_gaps: Box<[f64]>,
        offset: f64,
        /// Extra x padding before the first column (= arraycolsep when hskip_before_and_after is true).
        content_x_offset: f64,
        /// Extra x padding after the last column.
        content_x_end_offset: f64,
        /// For each column boundary (0 = before col 0, ..., num_cols = after last col),
        /// the vertical rule separator type: None = no rule, Some(false) = solid '|', Some(true) = dashed ':'.
        col_separators: Vec<Option<bool>>,
        /// For each row boundary (0 = before row 0, ..., num_rows = after last row),
        /// the list of hlines: false = solid, true = dashed.
        hlines_before_row: Vec<Vec<bool>>,
        /// Thickness of array rules in em.
        rule_thickness: f64,
        /// Gap between consecutive \hline or \hdashline rules (= \doublerulesep, in em).
        double_rule_sep: f64,
        /// Width of the cell grid including `content_x_offset` padding (em); excludes tag column.
        array_inner_width: f64,
        /// Horizontal gap between grid and tag column (em).
        tag_gap_em: f64,
        /// Width reserved for tags; tags are right-aligned in this column (em).
        tag_col_width: f64,
        /// Per-row tag layout; length matches number of rows.
        row_tags: Vec<Option<LayoutBox>>,
        /// When true, tags sit left of the grid (leqno-style).
        tags_left: bool,
    },

    /// An SVG-style path (arrows, braces, etc.).
    SvgPath {
        commands: Vec<PathCommand>,
        fill: bool,
    },

    /// A framed/colored box (fbox, colorbox, fcolorbox).
    /// body is the inner content; padding and border add to the outer dimensions.
    Framed {
        body: Box<LayoutBox>,
        padding: f64,
        border_thickness: f64,
        has_border: bool,
        bg_color: Option<Color>,
        border_color: Color,
    },

    /// A raised/lowered box (raisebox).
    /// shift > 0 moves content up, shift < 0 moves content down.
    RaiseBox { body: Box<LayoutBox>, shift: f64 },

    /// A scaled box (for \scriptstyle, \scriptscriptstyle in inline context).
    /// The child is rendered at child_scale relative to the parent.
    Scaled {
        body: Box<LayoutBox>,
        child_scale: f64,
    },

    /// Actuarial angle \angl{body}: path (horizontal roof + vertical bar) and body share the same baseline.
    Angl {
        path_commands: Vec<PathCommand>,
        body: Box<LayoutBox>,
    },

    /// \overline{body}: body with a horizontal rule drawn above it.
    Overline {
        body: Box<LayoutBox>,
        rule_thickness: f64,
        /// Distance from the body's top to the rule center.
        offset: f64,
    },

    /// \underline{body}: body with a horizontal rule drawn below it.
    Underline {
        body: Box<LayoutBox>,
        rule_thickness: f64,
        offset: f64,
    },

    /// Bussproofs-style proof tree with absolutely placed child boxes and inference rules.
    ProofTree {
        children: Vec<PlacedBox>,
        rules: Vec<ProofRule>,
    },

    /// Empty placeholder.
    Empty,
}

#[derive(Debug, Clone)]
pub struct PlacedBox {
    pub box_: LayoutBox,
    pub x: f64,
    pub baseline_y: f64,
}

#[derive(Debug, Clone)]
pub struct RunGlyph {
    pub x: f64,
    pub font_id: ratex_font::FontId,
    pub char_code: u32,
}

#[derive(Debug, Clone)]
pub struct ProofRule {
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub thickness: f64,
    pub dashed: bool,
}

/// A child element in a vertical box.
#[derive(Debug, Clone)]
pub struct VBoxChild {
    pub kind: VBoxChildKind,
    pub shift: f64,
}

#[derive(Debug, Clone)]
pub enum VBoxChildKind {
    Box(Box<LayoutBox>),
    Kern(f64),
}

impl LayoutBox {
    pub fn new_empty() -> Self {
        Self {
            width: 0.0,
            height: 0.0,
            depth: 0.0,
            content: BoxContent::Empty,
            color: Color::BLACK,
        }
    }

    pub fn new_kern(width: f64) -> Self {
        Self {
            width,
            height: 0.0,
            depth: 0.0,
            content: BoxContent::Kern,
            color: Color::BLACK,
        }
    }

    pub fn new_rule(width: f64, height: f64, depth: f64, thickness: f64, raise: f64) -> Self {
        Self {
            width,
            height,
            depth,
            content: BoxContent::Rule { thickness, raise },
            color: Color::BLACK,
        }
    }

    pub(crate) fn new_filled_rect(
        width: f64,
        height: f64,
        depth: f64,
        options: &crate::scoped_options::ScopedOptions<'_>,
    ) -> Result<Self, StorageError> {
        let commands = options.copy_values(
            StorageFamily::Layout,
            &Self::full_rect_path(width, height, depth),
        )?;
        Ok(Self {
            width,
            height,
            depth,
            content: BoxContent::SvgPath {
                commands,
                fill: true,
            },
            color: options.color,
        })
    }

    pub(crate) fn full_rect_path(width: f64, height: f64, depth: f64) -> [PathCommand; 5] {
        [
            PathCommand::MoveTo { x: 0.0, y: -height },
            PathCommand::LineTo {
                x: width,
                y: -height,
            },
            PathCommand::LineTo { x: width, y: depth },
            PathCommand::LineTo { x: 0.0, y: depth },
            PathCommand::Close,
        ]
    }

    pub fn total_height(&self) -> f64 {
        self.height + self.depth
    }

    pub fn with_color(mut self, color: Color) -> Self {
        self.color = color;
        self
    }

    /// Adjust height/depth for a delimiter to match a target size.
    pub fn with_adjusted_delim(mut self, height: f64, depth: f64) -> Self {
        self.height = height;
        self.depth = depth;
        self
    }
}

impl BoundedClone for LayoutBox {
    fn bounded_clone(
        &self,
        storage: &StorageLedger<'_>,
        phase: Phase,
        _family: StorageFamily,
    ) -> Result<Self, StorageError> {
        bounded_layout_clone(self, storage, phase, 1)
    }
}

fn bounded_layout_clone(
    value: &LayoutBox,
    storage: &StorageLedger<'_>,
    phase: Phase,
    depth: u32,
) -> Result<LayoutBox, StorageError> {
    storage.check(phase)?;
    if depth > 16 {
        return Err(storage.fail(StorageError::DepthLimit {
            phase,
            limit: 16,
            requested: depth,
        }));
    }
    storage.control().consume(phase, Resource::LayoutNodes, 1)?;
    let boxed = |value: &LayoutBox| -> Result<Box<LayoutBox>, StorageError> {
        let result = bounded_layout_clone(value, storage, phase, depth + 1)?;
        storage.try_box(phase, StorageFamily::Layout, result)
    };
    let optional = |value: &Option<Box<LayoutBox>>| value.as_deref().map(boxed).transpose();
    let content = match &value.content {
        BoxContent::HBox(children) => {
            let mut result =
                storage.try_vec_with_capacity(phase, StorageFamily::Layout, children.len())?;
            for child in children {
                result.push(bounded_layout_clone(child, storage, phase, depth + 1)?);
            }
            BoxContent::HBox(result)
        }
        BoxContent::VBox(children) => {
            let mut result =
                storage.try_vec_with_capacity(phase, StorageFamily::Layout, children.len())?;
            for child in children {
                storage.check(phase)?;
                let kind = match &child.kind {
                    VBoxChildKind::Box(value) => VBoxChildKind::Box(boxed(value)?),
                    VBoxChildKind::Kern(value) => {
                        storage.control().consume(phase, Resource::LayoutNodes, 1)?;
                        VBoxChildKind::Kern(*value)
                    }
                };
                result.push(VBoxChild {
                    kind,
                    shift: child.shift,
                });
            }
            BoxContent::VBox(result)
        }
        BoxContent::Glyph { font_id, char_code } => BoxContent::Glyph {
            font_id: *font_id,
            char_code: *char_code,
        },
        BoxContent::GlyphRun { glyphs } => {
            storage
                .control()
                .consume(phase, Resource::LayoutNodes, glyphs.len() as u64)?;
            let mut result =
                storage.try_vec_with_capacity(phase, StorageFamily::Layout, glyphs.len())?;
            for glyph in glyphs {
                storage.check(phase)?;
                result.push(RunGlyph {
                    x: glyph.x,
                    font_id: glyph.font_id,
                    char_code: glyph.char_code,
                });
            }
            BoxContent::GlyphRun { glyphs: result }
        }
        BoxContent::Rule { thickness, raise } => BoxContent::Rule {
            thickness: *thickness,
            raise: *raise,
        },
        BoxContent::Kern => BoxContent::Kern,
        BoxContent::Empty => BoxContent::Empty,
        BoxContent::Fraction {
            numer,
            denom,
            numer_shift,
            denom_shift,
            bar_thickness,
            numer_scale,
            denom_scale,
        } => BoxContent::Fraction {
            numer: boxed(numer)?,
            denom: boxed(denom)?,
            numer_shift: *numer_shift,
            denom_shift: *denom_shift,
            bar_thickness: *bar_thickness,
            numer_scale: *numer_scale,
            denom_scale: *denom_scale,
        },
        BoxContent::SupSub {
            base,
            sup,
            sub,
            sup_shift,
            sub_shift,
            sup_scale,
            sub_scale,
            center_scripts,
            italic_correction,
            sub_h_kern,
        } => BoxContent::SupSub {
            base: boxed(base)?,
            sup: optional(sup)?,
            sub: optional(sub)?,
            sup_shift: *sup_shift,
            sub_shift: *sub_shift,
            sup_scale: *sup_scale,
            sub_scale: *sub_scale,
            center_scripts: *center_scripts,
            italic_correction: *italic_correction,
            sub_h_kern: *sub_h_kern,
        },
        BoxContent::Radical {
            body,
            index,
            index_offset,
            index_scale,
            rule_thickness,
            inner_height,
        } => BoxContent::Radical {
            body: boxed(body)?,
            index: optional(index)?,
            index_offset: *index_offset,
            index_scale: *index_scale,
            rule_thickness: *rule_thickness,
            inner_height: *inner_height,
        },
        BoxContent::OpLimits {
            base,
            sup,
            sub,
            base_shift,
            sup_kern,
            sub_kern,
            slant,
            sup_scale,
            sub_scale,
        } => BoxContent::OpLimits {
            base: boxed(base)?,
            sup: optional(sup)?,
            sub: optional(sub)?,
            base_shift: *base_shift,
            sup_kern: *sup_kern,
            sub_kern: *sub_kern,
            slant: *slant,
            sup_scale: *sup_scale,
            sub_scale: *sub_scale,
        },
        BoxContent::LeftRight { left, right, inner } => BoxContent::LeftRight {
            left: boxed(left)?,
            right: boxed(right)?,
            inner: boxed(inner)?,
        },
        BoxContent::RaiseBox { body, shift } => BoxContent::RaiseBox {
            body: boxed(body)?,
            shift: *shift,
        },
        BoxContent::SvgPath { commands, fill }
            if crate::resource_limits::is_bounded_fill_path(commands, *fill) =>
        {
            let mut copy =
                storage.try_vec_with_capacity(phase, StorageFamily::Layout, commands.len())?;
            copy.extend_from_slice(commands);
            BoxContent::SvgPath {
                commands: copy,
                fill: *fill,
            }
        }
        BoxContent::Scaled { body, child_scale } => BoxContent::Scaled {
            body: boxed(body)?,
            child_scale: *child_scale,
        },
        BoxContent::Overline {
            body,
            rule_thickness,
            offset,
        } => BoxContent::Overline {
            body: boxed(body)?,
            rule_thickness: *rule_thickness,
            offset: *offset,
        },
        BoxContent::Underline {
            body,
            rule_thickness,
            offset,
        } => BoxContent::Underline {
            body: boxed(body)?,
            rule_thickness: *rule_thickness,
            offset: *offset,
        },
        _ => return Err(storage.fail(StorageError::UnsupportedObject)),
    };
    Ok(LayoutBox {
        width: value.width,
        height: value.height,
        depth: value.depth,
        content,
        color: value.color,
    })
}

const _: () = {
    assert!(std::mem::size_of::<LayoutBox>() <= 1_024);
    assert!(std::mem::align_of::<LayoutBox>() <= 1_024);
    assert!(std::mem::size_of::<VBoxChild>() <= 1_024);
    assert!(std::mem::size_of::<RunGlyph>() <= 1_024);
};
