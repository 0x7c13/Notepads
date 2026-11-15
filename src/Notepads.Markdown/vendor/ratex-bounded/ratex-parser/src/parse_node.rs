// Notepads Markdown bounded modifications: Copyright (c) 2026, Jiaqi (0x7c13) Liu.
use ratex_lexer::token::SourceLocation;
use serde::{Deserialize, Serialize};

/// Mode of the parser: math or text. Matches KaTeX's Mode type.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Mode {
    Math,
    Text,
}

/// Style string for \displaystyle, \textstyle etc.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum StyleStr {
    Display,
    Text,
    Script,
    Scriptscript,
}

/// Atom family: determines spacing behavior. Matches KaTeX's Atom type.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum AtomFamily {
    Bin,
    Close,
    Inner,
    Open,
    Punct,
    Rel,
}

/// A measurement with number and unit (e.g., "3pt", "1em").
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Measurement {
    pub number: f64,
    pub unit: String,
}

/// Column alignment spec for array environments.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AlignSpec {
    #[serde(rename = "type")]
    pub align_type: AlignType,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub align: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub pregap: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub postgap: Option<f64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum AlignType {
    Align,
    Separator,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ProofLineStyle {
    Solid,
    Dashed,
    None,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProofBranch {
    pub conclusion: Vec<ParseNode>,
    pub premises: Vec<ProofBranch>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(rename = "leftLabel")]
    pub left_label: Option<Vec<ParseNode>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(rename = "rightLabel")]
    pub right_label: Option<Vec<ParseNode>>,
    #[serde(rename = "lineStyle")]
    pub line_style: ProofLineStyle,
    #[serde(rename = "rootAtTop", skip_serializing_if = "is_false")]
    pub root_at_top: bool,
}

fn is_false(b: &bool) -> bool {
    !*b
}

/// The main AST node type. Each variant corresponds to a KaTeX ParseNode type.
///
/// Serializes to JSON with `"type": "variant_name"` to match KaTeX's format,
/// enabling direct structural comparison.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum ParseNode {
    // =========================================================================
    // Symbol-based nodes (from symbols.js lookup in Parser.parseSymbol)
    // =========================================================================
    #[serde(rename = "atom")]
    Atom {
        mode: Mode,
        family: AtomFamily,
        text: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        loc: Option<SourceLocation>,
    },

    #[serde(rename = "mathord")]
    MathOrd {
        mode: Mode,
        text: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        loc: Option<SourceLocation>,
    },

    #[serde(rename = "textord")]
    TextOrd {
        mode: Mode,
        text: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        loc: Option<SourceLocation>,
    },

    #[serde(rename = "op-token")]
    OpToken {
        mode: Mode,
        text: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        loc: Option<SourceLocation>,
    },

    #[serde(rename = "accent-token")]
    AccentToken {
        mode: Mode,
        text: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        loc: Option<SourceLocation>,
    },

    #[serde(rename = "spacing")]
    SpacingNode {
        mode: Mode,
        text: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        loc: Option<SourceLocation>,
    },

    // =========================================================================
    // Structural nodes
    // =========================================================================
    #[serde(rename = "ordgroup")]
    OrdGroup {
        mode: Mode,
        body: Vec<ParseNode>,
        #[serde(skip_serializing_if = "Option::is_none")]
        semisimple: Option<bool>,
        #[serde(skip_serializing_if = "Option::is_none")]
        loc: Option<SourceLocation>,
    },

    #[serde(rename = "supsub")]
    SupSub {
        mode: Mode,
        #[serde(skip_serializing_if = "Option::is_none")]
        base: Option<Box<ParseNode>>,
        #[serde(skip_serializing_if = "Option::is_none")]
        sup: Option<Box<ParseNode>>,
        #[serde(skip_serializing_if = "Option::is_none")]
        sub: Option<Box<ParseNode>>,
        #[serde(skip_serializing_if = "Option::is_none")]
        loc: Option<SourceLocation>,
    },

    // =========================================================================
    // Function-generated nodes
    // =========================================================================
    #[serde(rename = "genfrac")]
    GenFrac {
        mode: Mode,
        continued: bool,
        numer: Box<ParseNode>,
        denom: Box<ParseNode>,
        #[serde(rename = "hasBarLine")]
        has_bar_line: bool,
        #[serde(rename = "leftDelim")]
        left_delim: Option<String>,
        #[serde(rename = "rightDelim")]
        right_delim: Option<String>,
        #[serde(rename = "barSize")]
        bar_size: Option<Measurement>,
        #[serde(skip_serializing_if = "Option::is_none")]
        loc: Option<SourceLocation>,
    },

    #[serde(rename = "sqrt")]
    Sqrt {
        mode: Mode,
        body: Box<ParseNode>,
        #[serde(skip_serializing_if = "Option::is_none")]
        index: Option<Box<ParseNode>>,
        #[serde(skip_serializing_if = "Option::is_none")]
        loc: Option<SourceLocation>,
    },

    #[serde(rename = "accent")]
    Accent {
        mode: Mode,
        label: String,
        #[serde(rename = "isStretchy")]
        #[serde(skip_serializing_if = "Option::is_none")]
        is_stretchy: Option<bool>,
        #[serde(rename = "isShifty")]
        #[serde(skip_serializing_if = "Option::is_none")]
        is_shifty: Option<bool>,
        base: Box<ParseNode>,
        #[serde(skip_serializing_if = "Option::is_none")]
        loc: Option<SourceLocation>,
    },

    #[serde(rename = "accentUnder")]
    AccentUnder {
        mode: Mode,
        label: String,
        #[serde(rename = "isStretchy")]
        #[serde(skip_serializing_if = "Option::is_none")]
        is_stretchy: Option<bool>,
        #[serde(rename = "isShifty")]
        #[serde(skip_serializing_if = "Option::is_none")]
        is_shifty: Option<bool>,
        base: Box<ParseNode>,
        #[serde(skip_serializing_if = "Option::is_none")]
        loc: Option<SourceLocation>,
    },

    #[serde(rename = "op")]
    Op {
        mode: Mode,
        limits: bool,
        #[serde(rename = "alwaysHandleSupSub")]
        #[serde(skip_serializing_if = "Option::is_none")]
        always_handle_sup_sub: Option<bool>,
        #[serde(rename = "suppressBaseShift")]
        #[serde(skip_serializing_if = "Option::is_none")]
        suppress_base_shift: Option<bool>,
        #[serde(rename = "parentIsSupSub")]
        parent_is_sup_sub: bool,
        symbol: bool,
        #[serde(skip_serializing_if = "Option::is_none")]
        name: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        body: Option<Vec<ParseNode>>,
        #[serde(skip_serializing_if = "Option::is_none")]
        loc: Option<SourceLocation>,
    },

    #[serde(rename = "operatorname")]
    OperatorName {
        mode: Mode,
        body: Vec<ParseNode>,
        #[serde(rename = "alwaysHandleSupSub")]
        always_handle_sup_sub: bool,
        limits: bool,
        #[serde(rename = "parentIsSupSub")]
        parent_is_sup_sub: bool,
        #[serde(skip_serializing_if = "Option::is_none")]
        loc: Option<SourceLocation>,
    },

    #[serde(rename = "font")]
    Font {
        mode: Mode,
        font: String,
        body: Box<ParseNode>,
        #[serde(skip_serializing_if = "Option::is_none")]
        loc: Option<SourceLocation>,
    },

    #[serde(rename = "text")]
    Text {
        mode: Mode,
        body: Vec<ParseNode>,
        #[serde(skip_serializing_if = "Option::is_none")]
        font: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        loc: Option<SourceLocation>,
    },

    #[serde(rename = "color")]
    Color {
        mode: Mode,
        color: String,
        body: Vec<ParseNode>,
        #[serde(skip_serializing_if = "Option::is_none")]
        loc: Option<SourceLocation>,
    },

    #[serde(rename = "color-token")]
    ColorToken {
        mode: Mode,
        color: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        loc: Option<SourceLocation>,
    },

    #[serde(rename = "size")]
    Size {
        mode: Mode,
        value: Measurement,
        #[serde(rename = "isBlank")]
        is_blank: bool,
        #[serde(skip_serializing_if = "Option::is_none")]
        loc: Option<SourceLocation>,
    },

    #[serde(rename = "styling")]
    Styling {
        mode: Mode,
        style: StyleStr,
        body: Vec<ParseNode>,
        #[serde(skip_serializing_if = "Option::is_none")]
        loc: Option<SourceLocation>,
    },

    #[serde(rename = "sizing")]
    Sizing {
        mode: Mode,
        size: u8,
        body: Vec<ParseNode>,
        #[serde(skip_serializing_if = "Option::is_none")]
        loc: Option<SourceLocation>,
    },

    #[serde(rename = "delimsizing")]
    DelimSizing {
        mode: Mode,
        size: u8,
        mclass: String,
        delim: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        loc: Option<SourceLocation>,
    },

    #[serde(rename = "leftright")]
    LeftRight {
        mode: Mode,
        body: Vec<ParseNode>,
        left: String,
        right: String,
        #[serde(rename = "rightColor")]
        #[serde(skip_serializing_if = "Option::is_none")]
        right_color: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        loc: Option<SourceLocation>,
    },

    #[serde(rename = "leftright-right")]
    LeftRightRight {
        mode: Mode,
        delim: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        color: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        loc: Option<SourceLocation>,
    },

    #[serde(rename = "middle")]
    Middle {
        mode: Mode,
        delim: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        loc: Option<SourceLocation>,
    },

    #[serde(rename = "overline")]
    Overline {
        mode: Mode,
        body: Box<ParseNode>,
        #[serde(skip_serializing_if = "Option::is_none")]
        loc: Option<SourceLocation>,
    },

    #[serde(rename = "underline")]
    Underline {
        mode: Mode,
        body: Box<ParseNode>,
        #[serde(skip_serializing_if = "Option::is_none")]
        loc: Option<SourceLocation>,
    },

    #[serde(rename = "rule")]
    Rule {
        mode: Mode,
        #[serde(skip_serializing_if = "Option::is_none")]
        shift: Option<Measurement>,
        width: Measurement,
        height: Measurement,
        #[serde(skip_serializing_if = "Option::is_none")]
        loc: Option<SourceLocation>,
    },

    #[serde(rename = "kern")]
    Kern {
        mode: Mode,
        dimension: Measurement,
        #[serde(skip_serializing_if = "Option::is_none")]
        loc: Option<SourceLocation>,
    },

    #[serde(rename = "phantom")]
    Phantom {
        mode: Mode,
        body: Vec<ParseNode>,
        #[serde(skip_serializing_if = "Option::is_none")]
        loc: Option<SourceLocation>,
    },

    #[serde(rename = "vphantom")]
    VPhantom {
        mode: Mode,
        body: Box<ParseNode>,
        #[serde(skip_serializing_if = "Option::is_none")]
        loc: Option<SourceLocation>,
    },

    #[serde(rename = "smash")]
    Smash {
        mode: Mode,
        body: Box<ParseNode>,
        #[serde(rename = "smashHeight")]
        smash_height: bool,
        #[serde(rename = "smashDepth")]
        smash_depth: bool,
        #[serde(skip_serializing_if = "Option::is_none")]
        loc: Option<SourceLocation>,
    },

    #[serde(rename = "mclass")]
    MClass {
        mode: Mode,
        mclass: String,
        body: Vec<ParseNode>,
        #[serde(rename = "isCharacterBox")]
        is_character_box: bool,
        #[serde(skip_serializing_if = "Option::is_none")]
        loc: Option<SourceLocation>,
    },

    #[serde(rename = "array")]
    Array {
        mode: Mode,
        body: Vec<Vec<ParseNode>>,
        #[serde(rename = "rowGaps")]
        row_gaps: Vec<Option<Measurement>>,
        #[serde(rename = "hLinesBeforeRow")]
        hlines_before_row: Vec<Vec<bool>>,
        #[serde(skip_serializing_if = "Option::is_none")]
        cols: Option<Vec<AlignSpec>>,
        #[serde(skip_serializing_if = "Option::is_none")]
        #[serde(rename = "colSeparationType")]
        col_separation_type: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        #[serde(rename = "hskipBeforeAndAfter")]
        hskip_before_and_after: Option<bool>,
        #[serde(skip_serializing_if = "Option::is_none")]
        #[serde(rename = "addJot")]
        add_jot: Option<bool>,
        #[serde(default = "default_arraystretch")]
        arraystretch: f64,
        #[serde(skip_serializing_if = "Option::is_none")]
        tags: Option<Vec<ArrayTag>>,
        #[serde(skip_serializing_if = "Option::is_none")]
        leqno: Option<bool>,
        #[serde(skip_serializing_if = "Option::is_none")]
        #[serde(rename = "isCD")]
        is_cd: Option<bool>,
        #[serde(skip_serializing_if = "Option::is_none")]
        loc: Option<SourceLocation>,
    },

    #[serde(rename = "environment")]
    Environment {
        mode: Mode,
        name: String,
        #[serde(rename = "nameGroup")]
        name_group: Box<ParseNode>,
        #[serde(skip_serializing_if = "Option::is_none")]
        loc: Option<SourceLocation>,
    },

    #[serde(rename = "cr")]
    Cr {
        mode: Mode,
        #[serde(rename = "newLine")]
        new_line: bool,
        #[serde(skip_serializing_if = "Option::is_none")]
        size: Option<Measurement>,
        #[serde(skip_serializing_if = "Option::is_none")]
        loc: Option<SourceLocation>,
    },

    #[serde(rename = "infix")]
    Infix {
        mode: Mode,
        #[serde(rename = "replaceWith")]
        replace_with: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        size: Option<Measurement>,
        #[serde(skip_serializing_if = "Option::is_none")]
        loc: Option<SourceLocation>,
    },

    #[serde(rename = "internal")]
    Internal {
        mode: Mode,
        #[serde(skip_serializing_if = "Option::is_none")]
        loc: Option<SourceLocation>,
    },

    #[serde(rename = "verb")]
    Verb {
        mode: Mode,
        body: String,
        star: bool,
        #[serde(skip_serializing_if = "Option::is_none")]
        loc: Option<SourceLocation>,
    },

    #[serde(rename = "href")]
    Href {
        mode: Mode,
        href: String,
        body: Vec<ParseNode>,
        #[serde(skip_serializing_if = "Option::is_none")]
        loc: Option<SourceLocation>,
    },

    #[serde(rename = "url")]
    Url {
        mode: Mode,
        url: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        loc: Option<SourceLocation>,
    },

    #[serde(rename = "raw")]
    Raw {
        mode: Mode,
        string: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        loc: Option<SourceLocation>,
    },

    #[serde(rename = "hbox")]
    HBox {
        mode: Mode,
        body: Vec<ParseNode>,
        #[serde(skip_serializing_if = "Option::is_none")]
        loc: Option<SourceLocation>,
    },

    #[serde(rename = "horizBrace")]
    HorizBrace {
        mode: Mode,
        label: String,
        #[serde(rename = "isOver")]
        is_over: bool,
        base: Box<ParseNode>,
        #[serde(skip_serializing_if = "Option::is_none")]
        loc: Option<SourceLocation>,
    },

    #[serde(rename = "enclose")]
    Enclose {
        mode: Mode,
        label: String,
        #[serde(rename = "backgroundColor")]
        #[serde(skip_serializing_if = "Option::is_none")]
        background_color: Option<String>,
        #[serde(rename = "borderColor")]
        #[serde(skip_serializing_if = "Option::is_none")]
        border_color: Option<String>,
        body: Box<ParseNode>,
        #[serde(skip_serializing_if = "Option::is_none")]
        loc: Option<SourceLocation>,
    },

    #[serde(rename = "lap")]
    Lap {
        mode: Mode,
        alignment: String,
        body: Box<ParseNode>,
        #[serde(skip_serializing_if = "Option::is_none")]
        loc: Option<SourceLocation>,
    },

    #[serde(rename = "mathchoice")]
    MathChoice {
        mode: Mode,
        display: Vec<ParseNode>,
        text: Vec<ParseNode>,
        script: Vec<ParseNode>,
        scriptscript: Vec<ParseNode>,
        #[serde(skip_serializing_if = "Option::is_none")]
        loc: Option<SourceLocation>,
    },

    #[serde(rename = "raisebox")]
    RaiseBox {
        mode: Mode,
        dy: Measurement,
        body: Box<ParseNode>,
        #[serde(skip_serializing_if = "Option::is_none")]
        loc: Option<SourceLocation>,
    },

    #[serde(rename = "vcenter")]
    VCenter {
        mode: Mode,
        body: Box<ParseNode>,
        #[serde(skip_serializing_if = "Option::is_none")]
        loc: Option<SourceLocation>,
    },

    #[serde(rename = "xArrow")]
    XArrow {
        mode: Mode,
        label: String,
        body: Box<ParseNode>,
        #[serde(skip_serializing_if = "Option::is_none")]
        below: Option<Box<ParseNode>>,
        #[serde(skip_serializing_if = "Option::is_none")]
        loc: Option<SourceLocation>,
    },

    #[serde(rename = "pmb")]
    Pmb {
        mode: Mode,
        mclass: String,
        body: Vec<ParseNode>,
        #[serde(skip_serializing_if = "Option::is_none")]
        loc: Option<SourceLocation>,
    },

    #[serde(rename = "tag")]
    Tag {
        mode: Mode,
        body: Vec<ParseNode>,
        tag: Vec<ParseNode>,
        #[serde(skip_serializing_if = "Option::is_none")]
        loc: Option<SourceLocation>,
    },

    #[serde(rename = "nonumber")]
    NoNumber {
        mode: Mode,
        #[serde(skip_serializing_if = "Option::is_none")]
        loc: Option<SourceLocation>,
    },

    #[serde(rename = "html")]
    Html {
        mode: Mode,
        attributes: std::collections::HashMap<String, String>,
        body: Vec<ParseNode>,
        #[serde(skip_serializing_if = "Option::is_none")]
        loc: Option<SourceLocation>,
    },

    #[serde(rename = "htmlmathml")]
    HtmlMathMl {
        mode: Mode,
        html: Vec<ParseNode>,
        mathml: Vec<ParseNode>,
        #[serde(skip_serializing_if = "Option::is_none")]
        loc: Option<SourceLocation>,
    },

    #[serde(rename = "includegraphics")]
    IncludeGraphics {
        mode: Mode,
        alt: String,
        width: Measurement,
        height: Measurement,
        totalheight: Measurement,
        src: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        loc: Option<SourceLocation>,
    },

    #[serde(rename = "cdlabel")]
    CdLabel {
        mode: Mode,
        side: String,
        label: Box<ParseNode>,
        #[serde(skip_serializing_if = "Option::is_none")]
        loc: Option<SourceLocation>,
    },

    #[serde(rename = "cdlabelparent")]
    CdLabelParent {
        mode: Mode,
        fragment: Box<ParseNode>,
        #[serde(skip_serializing_if = "Option::is_none")]
        loc: Option<SourceLocation>,
    },

    #[serde(rename = "cdArrow")]
    CdArrow {
        mode: Mode,
        /// "right", "left", "up", "down", "horiz_eq", "vert_eq", "none"
        direction: String,
        /// For right/left arrows: label above the shaft.
        /// For up/down arrows: label to the left of the shaft.
        #[serde(skip_serializing_if = "Option::is_none")]
        label_above: Option<Box<ParseNode>>,
        /// For right/left arrows: label below the shaft.
        /// For up/down arrows: label to the right of the shaft.
        #[serde(skip_serializing_if = "Option::is_none")]
        label_below: Option<Box<ParseNode>>,
        #[serde(skip_serializing_if = "Option::is_none")]
        loc: Option<SourceLocation>,
    },

    #[serde(rename = "proofTree")]
    ProofTree {
        mode: Mode,
        tree: ProofBranch,
        #[serde(skip_serializing_if = "Option::is_none")]
        loc: Option<SourceLocation>,
    },
}

fn default_arraystretch() -> f64 {
    1.0
}

/// Tag variant for array rows: either auto-numbered (bool) or explicit tag.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum ArrayTag {
    Auto(bool),
    Explicit(Vec<ParseNode>),
}

// ── Helper methods ──────────────────────────────────────────────────────────

impl ParseNode {
    pub fn mode(&self) -> Mode {
        match self {
            Self::Atom { mode, .. }
            | Self::MathOrd { mode, .. }
            | Self::TextOrd { mode, .. }
            | Self::OpToken { mode, .. }
            | Self::AccentToken { mode, .. }
            | Self::SpacingNode { mode, .. }
            | Self::OrdGroup { mode, .. }
            | Self::SupSub { mode, .. }
            | Self::GenFrac { mode, .. }
            | Self::Sqrt { mode, .. }
            | Self::Accent { mode, .. }
            | Self::AccentUnder { mode, .. }
            | Self::Op { mode, .. }
            | Self::OperatorName { mode, .. }
            | Self::Font { mode, .. }
            | Self::Text { mode, .. }
            | Self::Color { mode, .. }
            | Self::ColorToken { mode, .. }
            | Self::Size { mode, .. }
            | Self::Styling { mode, .. }
            | Self::Sizing { mode, .. }
            | Self::DelimSizing { mode, .. }
            | Self::LeftRight { mode, .. }
            | Self::LeftRightRight { mode, .. }
            | Self::Middle { mode, .. }
            | Self::Overline { mode, .. }
            | Self::Underline { mode, .. }
            | Self::Rule { mode, .. }
            | Self::Kern { mode, .. }
            | Self::Phantom { mode, .. }
            | Self::VPhantom { mode, .. }
            | Self::Smash { mode, .. }
            | Self::MClass { mode, .. }
            | Self::Array { mode, .. }
            | Self::Environment { mode, .. }
            | Self::Cr { mode, .. }
            | Self::Infix { mode, .. }
            | Self::Internal { mode, .. }
            | Self::Verb { mode, .. }
            | Self::Href { mode, .. }
            | Self::Url { mode, .. }
            | Self::Raw { mode, .. }
            | Self::HBox { mode, .. }
            | Self::HorizBrace { mode, .. }
            | Self::Enclose { mode, .. }
            | Self::Lap { mode, .. }
            | Self::MathChoice { mode, .. }
            | Self::RaiseBox { mode, .. }
            | Self::VCenter { mode, .. }
            | Self::XArrow { mode, .. }
            | Self::Pmb { mode, .. }
            | Self::Tag { mode, .. }
            | Self::NoNumber { mode, .. }
            | Self::Html { mode, .. }
            | Self::HtmlMathMl { mode, .. }
            | Self::IncludeGraphics { mode, .. }
            | Self::CdLabel { mode, .. }
            | Self::CdLabelParent { mode, .. }
            | Self::CdArrow { mode, .. }
            | Self::ProofTree { mode, .. } => *mode,
        }
    }

    pub fn type_name(&self) -> &'static str {
        match self {
            Self::Atom { .. } => "atom",
            Self::MathOrd { .. } => "mathord",
            Self::TextOrd { .. } => "textord",
            Self::OpToken { .. } => "op-token",
            Self::AccentToken { .. } => "accent-token",
            Self::SpacingNode { .. } => "spacing",
            Self::OrdGroup { .. } => "ordgroup",
            Self::SupSub { .. } => "supsub",
            Self::GenFrac { .. } => "genfrac",
            Self::Sqrt { .. } => "sqrt",
            Self::Accent { .. } => "accent",
            Self::AccentUnder { .. } => "accentUnder",
            Self::Op { .. } => "op",
            Self::OperatorName { .. } => "operatorname",
            Self::Font { .. } => "font",
            Self::Text { .. } => "text",
            Self::Color { .. } => "color",
            Self::ColorToken { .. } => "color-token",
            Self::Size { .. } => "size",
            Self::Styling { .. } => "styling",
            Self::Sizing { .. } => "sizing",
            Self::DelimSizing { .. } => "delimsizing",
            Self::LeftRight { .. } => "leftright",
            Self::LeftRightRight { .. } => "leftright-right",
            Self::Middle { .. } => "middle",
            Self::Overline { .. } => "overline",
            Self::Underline { .. } => "underline",
            Self::Rule { .. } => "rule",
            Self::Kern { .. } => "kern",
            Self::Phantom { .. } => "phantom",
            Self::VPhantom { .. } => "vphantom",
            Self::Smash { .. } => "smash",
            Self::MClass { .. } => "mclass",
            Self::Array { .. } => "array",
            Self::Environment { .. } => "environment",
            Self::Cr { .. } => "cr",
            Self::Infix { .. } => "infix",
            Self::Internal { .. } => "internal",
            Self::Verb { .. } => "verb",
            Self::Href { .. } => "href",
            Self::Url { .. } => "url",
            Self::Raw { .. } => "raw",
            Self::HBox { .. } => "hbox",
            Self::HorizBrace { .. } => "horizBrace",
            Self::Enclose { .. } => "enclose",
            Self::Lap { .. } => "lap",
            Self::MathChoice { .. } => "mathchoice",
            Self::RaiseBox { .. } => "raisebox",
            Self::VCenter { .. } => "vcenter",
            Self::XArrow { .. } => "xArrow",
            Self::Pmb { .. } => "pmb",
            Self::Tag { .. } => "tag",
            Self::NoNumber { .. } => "nonumber",
            Self::Html { .. } => "html",
            Self::HtmlMathMl { .. } => "htmlmathml",
            Self::IncludeGraphics { .. } => "includegraphics",
            Self::CdLabel { .. } => "cdlabel",
            Self::CdLabelParent { .. } => "cdlabelparent",
            Self::CdArrow { .. } => "cdArrow",
            Self::ProofTree { .. } => "proofTree",
        }
    }

    /// Check if this node is a symbol node (atom or non-atom symbol).
    pub fn is_symbol_node(&self) -> bool {
        matches!(
            self,
            Self::Atom { .. }
                | Self::MathOrd { .. }
                | Self::TextOrd { .. }
                | Self::OpToken { .. }
                | Self::AccentToken { .. }
                | Self::SpacingNode { .. }
        )
    }

    /// Get the text of a symbol node.
    pub fn symbol_text(&self) -> Option<&str> {
        match self {
            Self::Atom { text, .. }
            | Self::MathOrd { text, .. }
            | Self::TextOrd { text, .. }
            | Self::OpToken { text, .. }
            | Self::AccentToken { text, .. }
            | Self::SpacingNode { text, .. } => Some(text),
            _ => None,
        }
    }

    /// Normalize an argument: if it's an ordgroup with a single element, unwrap it.
    pub fn normalize_argument(arg: ParseNode) -> ParseNode {
        match arg {
            Self::OrdGroup { mut body, .. } if body.len() == 1 => body.pop().unwrap(),
            arg => arg,
        }
    }

    /// Convert an argument to a list: if ordgroup, return body; otherwise wrap in vec.
    pub fn ord_argument(arg: ParseNode) -> Vec<ParseNode> {
        if let ParseNode::OrdGroup { body, .. } = arg {
            body
        } else {
            vec![arg]
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_serialize_mathord() {
        let node = ParseNode::MathOrd {
            mode: Mode::Math,
            text: "x".to_string(),
            loc: None,
        };
        let json = serde_json::to_string(&node).unwrap();
        assert!(json.contains(r#""type":"mathord""#));
        assert!(json.contains(r#""mode":"math""#));
        assert!(json.contains(r#""text":"x""#));
    }

    #[test]
    fn test_serialize_ordgroup() {
        let node = ParseNode::OrdGroup {
            mode: Mode::Math,
            body: vec![ParseNode::MathOrd {
                mode: Mode::Math,
                text: "a".to_string(),
                loc: None,
            }],
            semisimple: None,
            loc: None,
        };
        let json = serde_json::to_string(&node).unwrap();
        assert!(json.contains(r#""type":"ordgroup""#));
    }

    #[test]
    fn test_serialize_supsub() {
        let node = ParseNode::SupSub {
            mode: Mode::Math,
            base: Some(Box::new(ParseNode::MathOrd {
                mode: Mode::Math,
                text: "x".to_string(),
                loc: None,
            })),
            sup: Some(Box::new(ParseNode::TextOrd {
                mode: Mode::Math,
                text: "2".to_string(),
                loc: None,
            })),
            sub: None,
            loc: None,
        };
        let json = serde_json::to_string(&node).unwrap();
        assert!(json.contains(r#""type":"supsub""#));
    }

    #[test]
    fn test_serialize_genfrac() {
        let node = ParseNode::GenFrac {
            mode: Mode::Math,
            continued: false,
            numer: Box::new(ParseNode::MathOrd {
                mode: Mode::Math,
                text: "a".to_string(),
                loc: None,
            }),
            denom: Box::new(ParseNode::MathOrd {
                mode: Mode::Math,
                text: "b".to_string(),
                loc: None,
            }),
            has_bar_line: true,
            left_delim: None,
            right_delim: None,
            bar_size: None,
            loc: None,
        };
        let json = serde_json::to_string(&node).unwrap();
        assert!(json.contains(r#""type":"genfrac""#));
        assert!(json.contains(r#""hasBarLine":true"#));
    }

    #[test]
    fn test_serialize_atom() {
        let node = ParseNode::Atom {
            mode: Mode::Math,
            family: AtomFamily::Bin,
            text: "+".to_string(),
            loc: None,
        };
        let json = serde_json::to_string(&node).unwrap();
        assert!(json.contains(r#""type":"atom""#));
        assert!(json.contains(r#""family":"bin""#));
    }

    #[test]
    fn test_roundtrip() {
        let node = ParseNode::MathOrd {
            mode: Mode::Math,
            text: "x".to_string(),
            loc: Some(SourceLocation { start: 0, end: 1 }),
        };
        let json = serde_json::to_string(&node).unwrap();
        let parsed: ParseNode = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.type_name(), "mathord");
        assert_eq!(parsed.symbol_text(), Some("x"));
    }

    #[test]
    fn test_mode_accessor() {
        let node = ParseNode::Atom {
            mode: Mode::Math,
            family: AtomFamily::Rel,
            text: "=".to_string(),
            loc: None,
        };
        assert_eq!(node.mode(), Mode::Math);
    }

    #[test]
    fn test_normalize_argument() {
        let group = ParseNode::OrdGroup {
            mode: Mode::Math,
            body: vec![ParseNode::MathOrd {
                mode: Mode::Math,
                text: "x".to_string(),
                loc: None,
            }],
            semisimple: None,
            loc: None,
        };
        let normalized = ParseNode::normalize_argument(group);
        assert_eq!(normalized.type_name(), "mathord");
    }
}
