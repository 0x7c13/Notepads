// Notepads Markdown modifications Copyright (c) 2026, Jiaqi (0x7c13) Liu.
// Original third-party licenses remain applicable; see the crate COPYING file.

//! The CommonMark AST.

pub use crate::native::value::ValueText;
use std::cell::RefCell;
use std::convert::TryFrom;

#[cfg(feature = "phoenix_heex")]
pub use crate::parser::phoenix_heex::{HeexNode, NodeHeexBlock};
#[cfg(feature = "shortcodes")]
pub use crate::parser::shortcodes::NodeShortCode;
use crate::{arena_tree, options::AlertStyleType};

/// Shorthand for checking if a node's value matches the given expression.
///
/// Note this will call `node.data()`, which will fail if the node is already
/// mutably borrowed.
#[macro_export]
macro_rules! node_matches {
    ($node:expr, $( $pat:pat_param )|+) => {{
        matches!(
            $node.data().value,
            $( $pat )|+
        )
    }};
}

/// The core AST node enum.
#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(test, derive(strum::EnumDiscriminants))]
#[cfg_attr(
    test,
    strum_discriminants(vis(pub(crate)), derive(strum::VariantArray, Hash))
)]
pub enum NodeValue {
    /// The root of every CommonMark document.  Contains **blocks**.
    Document,

    /// Non-Markdown front matter.  Treated as an opaque blob.
    FrontMatter(String),

    /// **Block**. A [block quote](https://github.github.com/gfm/#block-quotes).  Contains other
    /// **blocks**.
    ///
    /// ```markdown
    /// > A block quote.
    /// ```
    BlockQuote,

    /// **Block**.  A [list](https://github.github.com/gfm/#lists).  Contains
    /// [list items](https://github.github.com/gfm/#list-items).
    ///
    /// ```markdown
    /// * An unordered list
    /// * Another item
    ///
    /// 1. An ordered list
    /// 2. Another item
    /// ```
    List(NodeList),

    /// **Block**.  A [list item](https://github.github.com/gfm/#list-items).  Contains other
    /// **blocks**.
    Item(NodeList),

    /// **Block**. A description list, enabled with `ext_description_lists` option.  Contains
    /// description items.
    ///
    /// It is required to put a blank line between terms and details.
    ///
    /// ```markdown
    /// Term 1
    ///
    /// : Details 1
    ///
    /// Term 2
    ///
    /// : Details 2
    /// ```
    DescriptionList,

    /// *Block**. An item of a description list.  Contains a term and one details block.
    DescriptionItem(NodeDescriptionItem),

    /// **Block**. Term of an item in a definition list.
    DescriptionTerm,

    /// **Block**. Details of an item in a definition list.
    DescriptionDetails,

    /// **Block**. A code block; may be [fenced](https://github.github.com/gfm/#fenced-code-blocks)
    /// or [indented](https://github.github.com/gfm/#indented-code-blocks).  Contains raw text
    /// which is not parsed as Markdown, although is HTML escaped.
    CodeBlock(Box<NodeCodeBlock>),

    /// **Block**. A [HTML block](https://github.github.com/gfm/#html-blocks).  Contains raw text
    /// which is neither parsed as Markdown nor HTML escaped.
    HtmlBlock(NodeHtmlBlock),

    #[cfg(feature = "phoenix_heex")]
    /// **Block**. A [Phoenix HEEx](https://hexdocs.pm/phoenix/components.html) template block.  Contains raw Phoenix HEEx text
    /// which is neither parsed as Markdown nor HTML escaped.
    HeexBlock(Box<NodeHeexBlock>),

    /// **Block**. A [paragraph](https://github.github.com/gfm/#paragraphs).  Contains **inlines**.
    Paragraph,

    /// **Block**. A heading; may be an [ATX heading](https://github.github.com/gfm/#atx-headings)
    /// or a [setext heading](https://github.github.com/gfm/#setext-headings). Contains
    /// **inlines**.
    Heading(NodeHeading),

    /// **Block**. A [thematic break](https://github.github.com/gfm/#thematic-breaks).  Has no
    /// children.
    ThematicBreak,

    /// A standalone native table-of-contents directive.
    TableOfContents,

    /// **Block**. A footnote definition.  The `String` is the footnote's name.
    /// Contains other **blocks**.
    FootnoteDefinition(NodeFootnoteDefinition),

    /// **Block**. A [table](https://github.github.com/gfm/#tables-extension-) per the GFM spec.
    /// Contains table rows.
    Table(Box<NodeTable>),

    /// **Block**. A table row.  The `bool` represents whether the row is the header row or not.
    /// Contains table cells.
    TableRow(bool),

    /// **Block**.  A table cell.  Contains **inlines**.
    TableCell,

    /// **Inline**.  [Textual content](https://github.github.com/gfm/#textual-content).  All text
    /// in a document will be contained in a `Text` node.
    Text(ValueText),

    /// **Block**. [Task list item](https://github.github.com/gfm/#task-list-items-extension-).
    /// The value is the symbol that was used in the brackets to mark a task item as checked, or
    /// `None` if the item is unchecked.
    TaskItem(NodeTaskItem),

    /// **Block leaf**. The captured task marker; the original Item owns its metadata.
    TaskCheckbox(NodeTaskCheckbox),

    /// **Inline**.  A [soft line break](https://github.github.com/gfm/#soft-line-breaks).  If
    /// the `hardbreaks` option is set in `Options` during formatting, it will be formatted
    /// as a `LineBreak`.
    SoftBreak,

    /// **Inline**.  A [hard line break](https://github.github.com/gfm/#hard-line-breaks).
    LineBreak,

    /// **Inline**.  A [code span](https://github.github.com/gfm/#code-spans).
    Code(NodeCode),

    /// **Inline**.  [Raw HTML](https://github.github.com/gfm/#raw-html) contained inline.
    HtmlInline(ValueText),

    #[cfg(feature = "phoenix_heex")]
    /// **Inline**. A [Phoenix HEEx](https://hexdocs.pm/phoenix/components.html) inline template element.  Contains raw Phoenix HEEx text
    /// which is neither parsed as Markdown nor HTML escaped.
    HeexInline(String),

    /// **Block/Inline**.  A Raw output node. This will be inserted verbatim into CommonMark and
    /// HTML output. It can only be created programmatically, and is never parsed from input.
    Raw(String),

    /// **Inline**.  [Emphasized](https://github.github.com/gfm/#emphasis-and-strong-emphasis)
    /// text.
    Emph,

    /// **Inline**.  [Strong](https://github.github.com/gfm/#emphasis-and-strong-emphasis) text.
    Strong,

    /// **Inline**.  [Strikethrough](https://github.github.com/gfm/#strikethrough-extension-) text
    /// per the GFM spec.
    Strikethrough,

    /// ***Inline**. [Highlight](https://www.markdownlang.com/extended/highlight.html) / mark text
    Highlight,

    /// **Inline**. Inserted text, rendered as `<ins>`.
    Insert,

    /// **Inline**.  Superscript.  Enabled with `ext_superscript` option.
    Superscript,

    /// **Inline**.  A [link](https://github.github.com/gfm/#links) to some URL, with possible
    /// title.
    Link(Box<NodeLink>),

    /// **Inline**.  An [image](https://github.github.com/gfm/#images).
    Image(Box<NodeLink>),

    /// A native embed of a separately owned local Markdown document.
    NoteEmbed(Box<NodeLink>),

    /// **Inline**.  A footnote reference.
    FootnoteReference(Box<NodeFootnoteReference>),

    #[cfg(feature = "shortcodes")]
    /// **Inline**. An Emoji character generated from a shortcode. Enable with feature "shortcodes".
    ShortCode(Box<NodeShortCode>),

    /// **Inline**. A math span. Contains raw text which is not parsed as Markdown.
    /// Dollar math or code math
    ///
    /// Inline math $1 + 2$ and $`1 + 2`$
    ///
    /// Display math $$1 + 2$$ and
    /// $$
    /// 1 + 2
    /// $$
    ///
    Math(NodeMath),

    /// **Block**. A [multiline block quote](https://github.github.com/gfm/#block-quotes).  Spans multiple
    /// lines and contains other **blocks**.
    ///
    /// ```markdown
    /// >>>
    /// A paragraph.
    ///
    /// - item one
    /// - item two
    /// >>>
    /// ```
    MultilineBlockQuote(NodeMultilineBlockQuote),

    /// **Inline**.  A character that has been [escaped](https://github.github.com/gfm/#backslash-escapes)
    ///
    /// Included in the document tree only if the `escaped_char_spans` [parse
    /// option](crate::options::ParseBuilder::escaped_char_spans) and/or [render
    /// option](crate::options::RenderBuilder::escaped_char_spans) is enabled.
    ///
    /// If the render option is enabled, these are wrapped in distinct `<span>`
    /// tags in output HTML and XML.
    Escaped,

    /// **Inline**.  A wikilink to some URL.
    WikiLink(NodeWikiLink),

    /// **Inline**.  Underline. Enabled with `underline` option.
    Underline,

    /// **Inline**.  Subscript. Enabled with `subscript` options.
    Subscript,

    /// **Inline**.  Spoilered text.  Enabled with `spoiler` option.
    SpoileredText,

    /// **Inline**. Text surrounded by escaped markup. Enabled with `spoiler` option.
    /// The `&'static str` is the tag to be escaped.
    EscapedTag(&'static str),

    /// **Block**. GitHub style alert boxes which uses a modified blockquote syntax.
    /// Enabled with the `alerts` option.
    Alert(Box<NodeAlert>),

    /// **Block**. Block scoped subscript that acts similar to a header.
    /// Enabled with `subtext` option.
    Subtext,

    /// **Block**. A container directive block which can be used for custom extensions.
    /// Contains other **blocks**.
    ///
    /// ```markdown
    /// :::warning
    /// A paragraph.
    ///
    /// - item one
    /// - item two
    /// :::
    /// ```
    BlockDirective(Box<NodeBlockDirective>),
}

/// Alignment of a single table cell.
#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub enum TableAlignment {
    /// Cell content is unaligned.
    None,

    /// Cell content is aligned left.
    Left,

    /// Cell content is centered.
    Center,

    /// Cell content is aligned right.
    Right,
}

impl TableAlignment {
    pub(crate) fn xml_name(&self) -> Option<&'static str> {
        match *self {
            TableAlignment::None => None,
            TableAlignment::Left => Some("left"),
            TableAlignment::Center => Some("center"),
            TableAlignment::Right => Some("right"),
        }
    }
}

/// The metadata of a table
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct NodeTable {
    /// The table alignments
    pub alignments: Vec<TableAlignment>,

    /// Number of columns of the table
    pub num_columns: usize,

    /// Number of rows of the table
    pub num_rows: usize,

    /// Number of non-empty, non-autocompleted cells
    pub num_nonempty_cells: usize,
}

/// A task list item's contents, and where it was found
#[derive(Debug, Clone, PartialEq, Eq, Copy)]
pub struct NodeTaskItem {
    /// The symbol within the brackets used to mark the task item as checked,
    /// or `None` if unchecked.
    pub symbol: Option<char>,

    /// The source position of the symbol itself.
    pub symbol_sourcepos: Sourcepos,
}

/// A captured task marker, kept separate from its containing list item.
#[derive(Debug, Clone, PartialEq, Eq, Copy)]
pub struct NodeTaskCheckbox {
    /// Whether the accepted raw marker was x or X.
    pub checked: bool,
}

/// An inline [code span](https://github.github.com/gfm/#code-spans).
#[derive(Default, Debug, Clone, PartialEq, Eq)]
pub struct NodeCode {
    /// The number of backticks
    pub num_backticks: usize,

    /// The content of the inline code span.
    /// As the contents are not interpreted as Markdown at all,
    /// they are contained within this structure,
    /// rather than inserted into a child inline of any kind.
    pub literal: ValueText,
}

/// The details of a link's destination, or an image's source.
#[derive(Default, Debug, Clone, PartialEq, Eq)]
pub struct NodeLink {
    /// The URL for the link destination or image source.
    pub url: ValueText,

    /// The title for the link or image.
    ///
    /// Note this field is used for the `title` attribute by the HTML formatter even for images;
    /// `alt` text is supplied in the image inline text.
    pub title: ValueText,
}

/// The details of a wikilink's destination.
#[derive(Default, Debug, Clone, PartialEq, Eq)]
pub struct NodeWikiLink {
    /// The URL for the link destination.
    pub url: String,
}

/// The metadata of a list; the kind of list, the delimiter used and so on.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct NodeList {
    /// The kind of list (bullet (unordered) or ordered).
    pub list_type: ListType,

    /// Number of spaces before the list marker.
    pub marker_offset: usize,

    /// Number of characters between the start of the list marker and the item text (including the list marker(s)).
    pub padding: usize,

    /// For ordered lists, the ordinal the list starts at.
    pub start: usize,

    /// For ordered lists, the delimiter after each number.
    pub delimiter: ListDelimType,

    /// For bullet lists, the character used for each bullet.
    pub bullet_char: u8,

    /// Whether the list is [tight](https://github.github.com/gfm/#tight), i.e. whether the
    /// paragraphs are wrapped in `<p>` tags when formatted as HTML.
    pub tight: bool,

    /// Whether the list contains tasks (checkbox items)
    pub is_task_list: bool,
}

/// The metadata of a description list
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct NodeDescriptionItem {
    /// Number of spaces before the list marker.
    pub marker_offset: usize,

    /// Number of characters between the start of the list marker and the item text (including the list marker(s)).
    pub padding: usize,

    /// Whether the list is [tight](https://github.github.com/gfm/#tight), i.e. whether the
    /// paragraphs are wrapped in `<p>` tags when formatted as HTML.
    pub tight: bool,
}

/// The type of list.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ListType {
    /// A bullet list, i.e. an unordered list.
    #[default]
    Bullet,

    /// An ordered list.
    Ordered,
}

/// The delimiter for ordered lists, i.e. the character which appears after each number.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ListDelimType {
    /// A period character `.`.
    #[default]
    Period,

    /// A paren character `)`.
    Paren,
}

impl ListDelimType {
    pub(crate) fn xml_name(&self) -> &'static str {
        match *self {
            ListDelimType::Period => "period",
            ListDelimType::Paren => "paren",
        }
    }
}

/// The metadata and data of a code block (fenced or indented).
#[derive(Default, Debug, Clone, PartialEq, Eq)]
pub struct NodeCodeBlock {
    /// Whether the code block is fenced.
    pub fenced: bool,

    /// For fenced code blocks, the fence character itself (`` ` `` or `~`).
    pub fence_char: u8,

    /// For fenced code blocks, the length of the fence.
    pub fence_length: usize,

    /// For fenced code blocks, the indentation level of the code within the block.
    pub fence_offset: usize,

    /// For fenced code blocks, the [info string](https://github.github.com/gfm/#info-string) after
    /// the opening fence, if any.
    pub info: ValueText,

    /// The literal contents of the code block.  As the contents are not interpreted as Markdown at
    /// all, they are contained within this structure, rather than inserted into a child inline of
    /// any kind.
    pub literal: ValueText,

    /// Whether the code block was explicitly closed by a closing fence. This is only meaningful
    /// for fenced code blocks; indented code blocks are treated as closed (true).
    pub closed: bool,
}

/// The metadata of a heading.
#[derive(Default, Debug, Clone, Copy, PartialEq, Eq)]
pub struct NodeHeading {
    /// The level of the header; from 1 to 6 for ATX headings, 1 or 2 for setext headings.
    pub level: u8,

    /// Whether the heading is setext (if not, ATX).
    pub setext: bool,

    /// Whether this ATX heading had a closing sequence of trailing hashes.
    /// Only meaningful for ATX headings (i.e. when `setext` is false).
    pub closed: bool,
}

/// The metadata of an included HTML block.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct NodeHtmlBlock {
    /// The HTML block's type
    pub block_type: u8,

    /// The literal contents of the HTML block.  Per NodeCodeBlock, the content is included here
    /// rather than in any inline.
    pub literal: ValueText,
}

/// The metadata of a footnote definition.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct NodeFootnoteDefinition {
    /// The name of the footnote.
    pub name: String,

    /// Total number of references to this footnote
    pub total_references: u32,
}

/// The metadata of a footnote reference.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct NodeFootnoteReference {
    /// The name of the footnote.
    pub name: String,

    /// The original text elements of the footnote, including their source position spans.
    pub texts: Vec<(String, usize)>,

    /// The index of reference to the same footnote
    pub ref_num: u32,

    /// The index of the footnote in the document.
    pub ix: u32,
}

/// The metadata of a multiline blockquote.
#[derive(Default, Debug, Clone, Copy, PartialEq, Eq)]
pub struct NodeMultilineBlockQuote {
    /// The length of the fence.
    pub fence_length: usize,

    /// The indentation level of the fence marker.
    pub fence_offset: usize,
}

/// An inline math span
#[derive(Default, Debug, Clone, PartialEq, Eq)]
pub struct NodeMath {
    /// Whether this is dollar math (`$` or `$$`).
    /// `false` indicates it is code math
    pub dollar_math: bool,

    /// Whether this is display math (using `$$`)
    pub display_math: bool,

    /// The literal contents of the math span.
    /// As the contents are not interpreted as Markdown at all,
    /// they are contained within this structure,
    /// rather than inserted into a child inline of any kind.
    pub literal: ValueText,
}

/// The metadata of an Alert node.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NodeAlert {
    /// Type of alert
    pub alert_type: AlertType,

    /// Overridden title. If `None`, then use the default title.
    pub title: Option<String>,

    /// Originated from a multiline blockquote.
    pub multiline: bool,

    /// The length of the fence (multiline only).
    pub fence_length: usize,

    /// The indentation level of the fence marker (multiline only)
    pub fence_offset: usize,
}

/// The type of alert.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum AlertType {
    /// Useful information that users should know, even when skimming content
    #[default]
    Note,

    /// Helpful advice for doing things better or more easily
    Tip,

    /// Key information users need to know to achieve their goal
    Important,

    /// Urgent info that needs immediate user attention to avoid problems
    Warning,

    /// Advises about risks or negative outcomes of certain actions
    Caution,
}

impl AlertType {
    /// Returns the default title for an alert type
    pub fn default_title(&self) -> &'static str {
        match *self {
            AlertType::Note => "Note",
            AlertType::Tip => "Tip",
            AlertType::Important => "Important",
            AlertType::Warning => "Warning",
            AlertType::Caution => "Caution",
        }
    }

    /// Returns the CSS class to use for an alert type
    pub fn css_class(&self, style: AlertStyleType) -> &'static str {
        match (*self, style) {
            (AlertType::Note, AlertStyleType::Specific) => "markdown-alert-note",
            (AlertType::Note, AlertStyleType::Semantic) => "note",
            (AlertType::Tip, AlertStyleType::Specific) => "markdown-alert-tip",
            (AlertType::Tip, AlertStyleType::Semantic) => "tip",
            (AlertType::Important, AlertStyleType::Specific) => "markdown-alert-important",
            (AlertType::Important, AlertStyleType::Semantic) => "important",
            (AlertType::Warning, AlertStyleType::Specific) => "markdown-alert-warning",
            (AlertType::Warning, AlertStyleType::Semantic) => "warning",
            (AlertType::Caution, AlertStyleType::Specific) => "markdown-alert-caution",
            (AlertType::Caution, AlertStyleType::Semantic) => "caution",
        }
    }
}

/// The metadata of a container block directive node.
#[derive(Default, Debug, Clone, PartialEq, Eq)]
pub struct NodeBlockDirective {
    /// The length of the fence.
    pub fence_length: usize,

    /// The indentation level of the fence marker.
    pub fence_offset: usize,

    /// The info string after the opening fence, if any.
    pub info: String,
}

/// Attributes on the node, if any.
///
/// These are somewhat HTML-specific in nature, though none are formatted
/// automatically to HTML.  There may be an `id` (specified like "#xyz" in the
/// source document; last ID-like attribute wins), zero or more `classes`
/// (specified like ".abc" in the source document), and zero or more `pairs`
/// (specified like "a=b" in the source document).
///
/// Attribute values may be quoted as follows:
///
/// * `#"id with spaces in it"`
/// * `."class with \"double quotes\" in it"`
/// * `pair="with spaces and such in the value"`
///
/// Pair keys may not be quoted.  You can't open quotes late or close them early.
/// Backslashes escape whatever character follows them within quotes.
/// Newlines are not permitted in quoted values, but they are permitted around
/// attribute values when attached to an inline, e.g.:
///
/// ```markdown
/// Some `code`{
///   a="hello"
///   b="privyet"
/// }
/// ```
///
/// Take care that you don't lead such lines with four or more spaces relative to
/// the block offset, or you'll get an unwanted code block!
#[cfg(feature = "attributes")]
#[derive(Default, Debug, Clone, PartialEq, Eq)]
pub struct Attributes {
    /// The id attribute, if any; specified like `#id` (last wins).
    pub id: Option<String>,

    /// Class attributes, if any; specified like `.a .b`.
    pub classes: Vec<String>,

    /// Attribute pairs, if any; specified like `a=b c=d`.
    pub pairs: Vec<(String, String)>,
}

impl NodeValue {
    /// Indicates whether this node is a block node or inline node.
    pub fn block(&self) -> bool {
        match *self {
            NodeValue::Document
            | NodeValue::BlockQuote
            | NodeValue::FootnoteDefinition(_)
            | NodeValue::List(..)
            | NodeValue::DescriptionList
            | NodeValue::DescriptionItem(_)
            | NodeValue::DescriptionTerm
            | NodeValue::DescriptionDetails
            | NodeValue::Item(..)
            | NodeValue::CodeBlock(..)
            | NodeValue::HtmlBlock(..)
            | NodeValue::Paragraph
            | NodeValue::Heading(..)
            | NodeValue::ThematicBreak
            | NodeValue::TableOfContents
            | NodeValue::Table(..)
            | NodeValue::TableRow(..)
            | NodeValue::TableCell
            | NodeValue::TaskItem(..)
            | NodeValue::TaskCheckbox(..)
            | NodeValue::MultilineBlockQuote(_)
            | NodeValue::Alert(_)
            | NodeValue::Subtext => true,
            NodeValue::BlockDirective(_) => true,
            #[cfg(feature = "phoenix_heex")]
            NodeValue::HeexBlock(..) => true,
            _ => false,
        }
    }

    /// Whether the type the node is of can contain inline nodes.
    pub fn contains_inlines(&self) -> bool {
        matches!(
            *self,
            NodeValue::Paragraph
                | NodeValue::Heading(..)
                | NodeValue::TableCell
                | NodeValue::Subtext
        )
    }

    /// Return a reference to the text of a `Text` inline, if this node is one.
    ///
    /// Convenience method.
    pub fn text(&self) -> Option<&ValueText> {
        match *self {
            NodeValue::Text(ref t) => Some(t),
            _ => None,
        }
    }

    /// Return a mutable reference to the text of a `Text` inline, if this node is one.
    ///
    /// Convenience method.
    pub fn text_mut(&mut self) -> Option<&mut ValueText> {
        match *self {
            NodeValue::Text(ref mut t) => Some(t),
            _ => None,
        }
    }

    /// Returns the name this kind of node gets in an XML document.  Follows
    /// the CommonMark DTD for nodes from the spec, otherwise we just vibe in a
    /// stable way.
    pub fn xml_node_name(&self) -> &'static str {
        match *self {
            NodeValue::Document => "document",
            NodeValue::BlockQuote => "block_quote",
            NodeValue::FootnoteDefinition(_) => "footnote_definition",
            NodeValue::List(..) => "list",
            NodeValue::DescriptionList => "description_list",
            NodeValue::DescriptionItem(_) => "description_item",
            NodeValue::DescriptionTerm => "description_term",
            NodeValue::DescriptionDetails => "description_details",
            NodeValue::Item(..) => "item",
            NodeValue::CodeBlock(..) => "code_block",
            NodeValue::HtmlBlock(..) => "html_block",
            #[cfg(feature = "phoenix_heex")]
            NodeValue::HeexBlock(..) => "heex_block",
            NodeValue::Paragraph => "paragraph",
            NodeValue::Heading(..) => "heading",
            NodeValue::ThematicBreak => "thematic_break",
            NodeValue::TableOfContents => "table_of_contents",
            NodeValue::Table(..) => "table",
            NodeValue::TableRow(..) => "table_row",
            NodeValue::TableCell => "table_cell",
            NodeValue::Text(..) => "text",
            NodeValue::SoftBreak => "softbreak",
            NodeValue::LineBreak => "linebreak",
            NodeValue::Image(..) => "image",
            NodeValue::NoteEmbed(..) => "note_embed",
            NodeValue::Link(..) => "link",
            NodeValue::Emph => "emph",
            NodeValue::Strong => "strong",
            NodeValue::Code(..) => "code",
            NodeValue::HtmlInline(..) => "html_inline",
            #[cfg(feature = "phoenix_heex")]
            NodeValue::HeexInline(..) => "heex_inline",
            NodeValue::Raw(..) => "raw",
            NodeValue::Strikethrough => "strikethrough",
            NodeValue::Highlight => "highlight",
            NodeValue::Insert => "insert",
            NodeValue::FrontMatter(_) => "frontmatter",
            NodeValue::TaskItem { .. } => "taskitem",
            NodeValue::TaskCheckbox(..) => "task_checkbox",
            NodeValue::Superscript => "superscript",
            NodeValue::FootnoteReference(..) => "footnote_reference",
            #[cfg(feature = "shortcodes")]
            NodeValue::ShortCode(_) => "shortcode",
            NodeValue::MultilineBlockQuote(_) => "multiline_block_quote",
            NodeValue::Escaped => "escaped",
            NodeValue::Math(..) => "math",
            NodeValue::WikiLink(..) => "wikilink",
            NodeValue::Underline => "underline",
            NodeValue::Subscript => "subscript",
            NodeValue::SpoileredText => "spoiler",
            NodeValue::EscapedTag(_) => "escaped_tag",
            NodeValue::Alert(_) => "alert",
            NodeValue::Subtext => "subtext",
            NodeValue::BlockDirective(_) => "block_directive",
        }
    }

    pub(crate) fn has_block_lines(&self) -> bool {
        self.accepts_lines() || matches!(*self, NodeValue::HtmlBlock(..) | NodeValue::TableCell)
    }

    pub(crate) fn accepts_lines(&self) -> bool {
        match *self {
            NodeValue::Paragraph
            | NodeValue::Heading(..)
            | NodeValue::CodeBlock(..)
            | NodeValue::Subtext => true,
            #[cfg(feature = "phoenix_heex")]
            NodeValue::HeexBlock(..) => true,
            _ => false,
        }
    }
}

/// A single node in the CommonMark AST.
///
/// The struct contains metadata about the node's position in the original document, and the core
/// enum, `NodeValue`.
#[derive(Clone, PartialEq, Eq)]
pub struct Ast {
    /// The node value itself.
    pub value: NodeValue,

    /// The positions in the source document this node comes from.
    pub sourcepos: Sourcepos,

    #[cfg(feature = "attributes")]
    /// [Attributes] on this node, if any.
    pub attrs: Option<Box<Attributes>>,

    pub(crate) native_id: Option<crate::native::NativeNodeId>,
    pub(crate) native_callout_title: bool,
    pub(crate) open: bool,
    pub(crate) last_line_blank: bool,
    pub(crate) table_visited: bool,
    pub(crate) lines: Option<Box<BlockLines>>,
}

#[derive(Clone, Default, PartialEq, Eq)]
pub(crate) struct BlockLines {
    pub(crate) buffer: Option<crate::native::BufferId>,
    pub(crate) base: usize,
    pub(crate) opened_line: usize,
    pub(crate) eof_anchor: Option<usize>,
    pub(crate) content: String,
    pub(crate) offsets: smallvec::SmallVec<[usize; 4]>,
}

impl Ast {
    pub(crate) fn lines_mut(&mut self) -> &mut BlockLines {
        self.lines.as_mut().unwrap()
    }
}

impl std::fmt::Debug for Ast {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "<{:?} ({})>", self.value, self.sourcepos)
    }
}

#[allow(dead_code)]
#[cfg(all(target_pointer_width = "64", not(feature = "attributes")))]
/// Assert the size of Ast is 88 bytes. It's big enough; no más.
const AST_SIZE_ASSERTION: [u8; 88] = [0; std::mem::size_of::<Ast>()];

#[allow(dead_code)]
#[cfg(all(target_pointer_width = "64", not(feature = "attributes")))]
/// Assert the total size of what we allocate in the Arena, for reference.
///
/// Note that the size adds to Ast:
/// * 8 bytes for RefCell.
/// * 40 bytes for arena_tree::Node's 5 pointers.
const AST_NODE_SIZE_ASSERTION: [u8; 136] = [0; std::mem::size_of::<AstNode<'_>>()];

/// Represents the position in the source Markdown this node was rendered from.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct Sourcepos {
    /// The line and column of the first character of this node.
    pub start: LineColumn,
    /// The line and column of the last character of this node.
    pub end: LineColumn,
}

impl std::fmt::Debug for Sourcepos {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        std::fmt::Display::fmt(self, f)
    }
}

impl std::fmt::Display for Sourcepos {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "{}:{}-{}:{}",
            self.start.line, self.start.column, self.end.line, self.end.column,
        )
    }
}

impl From<(LineColumn, LineColumn)> for Sourcepos {
    fn from((start, end): (LineColumn, LineColumn)) -> Sourcepos {
        Sourcepos { start, end }
    }
}

impl From<(usize, usize, usize, usize)> for Sourcepos {
    fn from(sp: (usize, usize, usize, usize)) -> Sourcepos {
        Sourcepos {
            start: LineColumn {
                line: sp.0,
                column: sp.1,
            },
            end: LineColumn {
                line: sp.2,
                column: sp.3,
            },
        }
    }
}

/// Represents the 1-based line and column positions of a given character.
///
/// By default, the `column` value is measured in UTF-8 byte offsets (1-based),
/// matching cmark behavior. This means multi-byte UTF-8 characters
/// (for example, `ö` or `好`) increase the column count by their byte
/// length rather than by Rust's `char` count.
///
/// Enable [`parse.sourcepos_chars`][crate::options::Parse#structfield.sourcepos_chars] to have
/// column values reported as a Unicode character count instead.
#[derive(Default, Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct LineColumn {
    /// The 1-based line number of the character.
    pub line: usize,
    /// The 1-based column number of the character.
    ///
    /// By default this is counted in UTF-8 bytes (so a 3-byte character
    /// increments the column by 3).  Enable
    /// [`parse.sourcepos_chars`][crate::options::Parse#structfield.sourcepos_chars] to have
    /// it reported as a Unicode character count instead.
    pub column: usize,
}

impl From<(usize, usize)> for LineColumn {
    fn from(lc: (usize, usize)) -> LineColumn {
        LineColumn {
            line: lc.0,
            column: lc.1,
        }
    }
}

impl LineColumn {
    /// Return a new LineColumn based on this one, with the column adjusted by offset.
    pub fn column_add(&self, offset: isize) -> LineColumn {
        LineColumn {
            line: self.line,
            column: usize::try_from((self.column as isize) + offset).unwrap(),
        }
    }
}

impl Ast {
    /// Create a new AST node with the given value and starting sourcepos. The
    /// end column is set to zero; it is expected this will be set manually
    /// or later in the parse.  Use [`Ast::new_with_sourcepos`] if you have full
    /// sourcepos.
    pub fn new(value: NodeValue, start: LineColumn) -> Self {
        Ast {
            value,
            sourcepos: (start.line, start.column, start.line, 0).into(),
            #[cfg(feature = "attributes")]
            attrs: None,
            native_id: None,
            native_callout_title: false,
            open: true,
            last_line_blank: false,
            table_visited: false,
            lines: None,
        }
    }

    /// Create a new AST node with the given value and sourcepos.
    pub fn new_with_sourcepos(value: NodeValue, sourcepos: Sourcepos) -> Self {
        Ast {
            value,
            sourcepos,
            #[cfg(feature = "attributes")]
            attrs: None,
            native_id: None,
            native_callout_title: false,
            open: true,
            last_line_blank: false,
            table_visited: false,
            lines: None,
        }
    }
}

/// The type of a node within the document.
///
/// It is bound by the lifetime `'a`, which corresponds to the `Arena` nodes are
/// allocated in. Child `Ast`s are wrapped in `RefCell` for interior mutability.
///
/// You can construct a new `AstNode` from a `NodeValue` using the `From` trait:
///
/// ```rust
/// # use comrak::nodes::{AstNode, NodeValue};
/// let root = AstNode::from(NodeValue::Document);
/// ```
///
/// Note that no sourcepos information is given to the created node. If you wish
/// to assign sourcepos information, use the `From` trait to create an `AstNode`
/// from an `Ast`:
///
/// ```rust
/// # use comrak::nodes::{Ast, AstNode, NodeValue};
/// let root = AstNode::from(Ast::new_with_sourcepos(
///     NodeValue::Paragraph,
///     (4, 1, 4, 10).into(),
/// ));
/// ```
///
/// For practical use, you'll probably need it allocated in an `Arena`, in which
/// case you can use `.into()` to simplify creation:
///
/// ```rust
/// # use comrak::{nodes::{AstNode, NodeValue}, Arena};
/// # let arena = Arena::new();
/// let node_in_arena = arena.alloc(NodeValue::Document.into());
/// ```
pub type AstNode<'a> = arena_tree::Node<'a, RefCell<Ast>>;

/// A reference to a node in an arena.  Unless you are manually creating nodes
/// before the arena, this is the type you will see most often.
pub type Node<'a> = &'a AstNode<'a>;

impl From<NodeValue> for AstNode<'_> {
    /// Create a new AST node with the given value. The sourcepos is set to (0,0)-(0,0).
    fn from(value: NodeValue) -> Self {
        arena_tree::Node::new(RefCell::new(Ast::new(value, LineColumn::default())))
    }
}

impl From<Ast> for AstNode<'_> {
    /// Create a new AST node with the given Ast.
    fn from(ast: Ast) -> Self {
        arena_tree::Node::new(RefCell::new(ast))
    }
}

/// Validation errors produced by [arena_tree::Node::validate].
#[derive(Debug, Clone)]
pub enum ValidationError<'a> {
    /// The type of a child node is not allowed in the parent node. This can happen when an inline
    /// node is found in a block container, a block is found in an inline node, etc.
    InvalidChildType {
        /// The parent node.
        parent: Node<'a>,
        /// The child node.
        child: Node<'a>,
    },
}

impl<'a> arena_tree::Node<'a, RefCell<Ast>> {
    /// Returns true if the given node can contain a node with the given value.
    pub fn can_contain_type(&self, child: &NodeValue) -> bool {
        if matches!(self.data().value, NodeValue::TaskCheckbox(..)) {
            return false;
        }
        match *child {
            NodeValue::TaskCheckbox(..) => {
                return node_matches!(self, NodeValue::Item(..));
            }
            NodeValue::Raw(_) => {
                return true;
            }
            NodeValue::Document => {
                return false;
            }
            NodeValue::FrontMatter(_) => {
                return node_matches!(self, NodeValue::Document);
            }
            _ => {}
        }

        match self.data().value {
            NodeValue::Document
            | NodeValue::BlockQuote
            | NodeValue::FootnoteDefinition(_)
            | NodeValue::DescriptionTerm
            | NodeValue::DescriptionDetails
            | NodeValue::Item(..)
            | NodeValue::TaskItem(..) => {
                child.block() && !matches!(*child, NodeValue::Item(..) | NodeValue::TaskItem(..))
            }
            NodeValue::List(..) => matches!(*child, NodeValue::Item(..) | NodeValue::TaskItem(..)),
            NodeValue::DescriptionList => matches!(*child, NodeValue::DescriptionItem(_)),
            NodeValue::DescriptionItem(_) => matches!(
                *child,
                NodeValue::DescriptionTerm | NodeValue::DescriptionDetails
            ),
            #[cfg(feature = "shortcodes")]
            NodeValue::ShortCode(..) => !child.block(),
            NodeValue::Paragraph
            | NodeValue::Heading(..)
            | NodeValue::Emph
            | NodeValue::Strong
            | NodeValue::Link(..)
            | NodeValue::Image(..)
            | NodeValue::WikiLink(..)
            | NodeValue::Strikethrough
            | NodeValue::Highlight
            | NodeValue::Insert
            | NodeValue::Superscript
            | NodeValue::SpoileredText
            | NodeValue::Underline
            | NodeValue::Subscript
            | NodeValue::Subtext
            // XXX: this is quite a hack: the EscapedTag _contains_ whatever was
            // possibly going to fall into the spoiler. This should be fixed in
            // inlines.
            | NodeValue::EscapedTag(_)
            => !child.block(),
            NodeValue::Table(..) => matches!(*child, NodeValue::TableRow(..)),
            NodeValue::TableRow(..) => matches!(*child, NodeValue::TableCell),
            NodeValue::TableCell => {
                #[cfg(feature = "shortcodes")]
                if matches!(*child, NodeValue::ShortCode(..)) {
                    return true;
                }

                #[cfg(feature = "phoenix_heex")]
                if matches!(*child, NodeValue::HeexInline(_)) {
                    return true;
                }

                matches!(
                    *child,
                    NodeValue::Text(..)
                    | NodeValue::Code(..)
                    | NodeValue::Emph
                    | NodeValue::Strong
                    | NodeValue::Link(..)
                    | NodeValue::Image(..)
                    | NodeValue::NoteEmbed(..)
                    | NodeValue::Strikethrough
                    | NodeValue::Highlight
                    | NodeValue::Insert
                    | NodeValue::HtmlInline(..)
                    | NodeValue::Math(..)
                    | NodeValue::WikiLink(..)
                    | NodeValue::FootnoteReference(..)
                    | NodeValue::Superscript
                    | NodeValue::SpoileredText
                    | NodeValue::Underline
                    | NodeValue::Subscript
                    | NodeValue::TaskItem(_)
                    | NodeValue::Escaped
                    | NodeValue::EscapedTag(_)
                )
            }
            NodeValue::MultilineBlockQuote(_) => {
                child.block() && !matches!(*child, NodeValue::Item(..) | NodeValue::TaskItem(..))
            }
            NodeValue::Alert(_) => {
                child.block() && !matches!(*child, NodeValue::Item(..) | NodeValue::TaskItem(..))
            }
            NodeValue::BlockDirective(_) => {
                child.block() && !matches!(*child, NodeValue::Item(..) | NodeValue::TaskItem(..))
            }

            NodeValue::Escaped => matches!(*child, NodeValue::Text(_)),

            NodeValue::FrontMatter(_)
            | NodeValue::CodeBlock(_)
            | NodeValue::HtmlBlock(_)
            | NodeValue::ThematicBreak
            | NodeValue::TableOfContents
            | NodeValue::NoteEmbed(_)
            | NodeValue::Text(_)
            | NodeValue::SoftBreak
            | NodeValue::LineBreak
            | NodeValue::Code(_)
            | NodeValue::HtmlInline(_)
            | NodeValue::Raw(_)
            | NodeValue::FootnoteReference(_)
            | NodeValue::Math(_) => false,
            NodeValue::TaskCheckbox(..) => false,

            #[cfg(feature = "phoenix_heex")]
            NodeValue::HeexBlock(_) | NodeValue::HeexInline(_) => false,
        }
    }

    /// The Comrak representation of a markdown node in Rust isn't strict enough to rule out
    /// invalid trees according to the CommonMark specification. One simple example is that block
    /// containers, such as lists, should only contain blocks, but it's possible to put naked
    /// inline text in a list item. Such invalid trees can lead Comrak to generate incorrect output
    /// if rendered.
    ///
    /// This method performs additional structural checks to ensure that a markdown AST is valid
    /// according to the CommonMark specification.
    ///
    /// Note that those invalid trees can only be generated programmatically. Parsing markdown with
    /// Comrak, on the other hand, should always produce a valid tree.
    pub fn validate(&'a self) -> Result<(), ValidationError<'a>> {
        let mut stack = vec![self];

        while let Some(node) = stack.pop() {
            // Check that this node type is valid wrt to the type of its parent.
            if let Some(parent) = node.parent()
                && !parent.can_contain_type(&node.data().value)
            {
                return Err(ValidationError::InvalidChildType {
                    parent,
                    child: node,
                });
            }

            stack.extend(node.children());
        }

        Ok(())
    }

    /// Collects this node and all of its children in depth-first
    /// (document) order, returning the concatenated literal contents of text,
    /// code and math blocks. Line breaks and soft breaks are represented as a
    /// single whitespace character.
    pub fn collect_text(&'a self) -> String {
        let mut text = String::with_capacity(20);
        self.collect_text_append(&mut text);
        text
    }

    /// Collects this node and all of its children in depth-first
    /// (document) order, appending the literal contents of text, code and math
    /// blocks to an output buffer. Line breaks and soft breaks are represented
    /// as a single whitespace character.
    pub fn collect_text_append(&'a self, output: &mut String) {
        let mut stack = vec![self];
        while let Some(node) = stack.pop() {
            match node.data().value {
                NodeValue::Text(ref literal) => output.push_str(
                    literal
                        .owned_str()
                        .expect("collect_text requires a legacy owned AST"),
                ),
                NodeValue::Code(NodeCode { ref literal, .. }) => output.push_str(
                    literal
                        .owned_str()
                        .expect("collect_text requires a legacy owned AST"),
                ),
                NodeValue::LineBreak | NodeValue::SoftBreak => output.push(' '),
                NodeValue::Math(NodeMath { ref literal, .. }) => output.push_str(
                    literal
                        .owned_str()
                        .expect("collect_text requires a legacy owned AST"),
                ),
                _ => stack.extend(node.reverse_children()),
            }
        }
    }

    pub(crate) fn last_child_is_open(&self) -> bool {
        self.last_child().is_some_and(|n| n.data().open)
    }

    pub(crate) fn ends_with_blank_line_with_host<H: crate::native::Host>(
        &self,
        ctx: &mut crate::native::Context<'_, H>,
    ) -> Result<bool, H::Error> {
        let mut it = Some(self);
        while let Some(cur) = it {
            ctx.event()?;
            if cur.data().last_line_blank {
                return Ok(true);
            }
            match cur.data().value {
                NodeValue::List(..) | NodeValue::Item(..) | NodeValue::TaskItem(..) => {
                    it = cur.last_child()
                }
                _ => it = None,
            };
        }
        Ok(false)
    }

    pub(crate) fn containing_block(&'a self) -> Option<Node<'a>> {
        let mut ch = Some(self);
        while let Some(n) = ch {
            if n.data().value.block() {
                return Some(n);
            }
            ch = n.parent();
        }
        None
    }
}
