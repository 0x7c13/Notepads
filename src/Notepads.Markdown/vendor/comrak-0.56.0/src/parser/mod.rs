// Notepads Markdown modifications Copyright (c) 2026, Jiaqi (0x7c13) Liu.
// Original third-party licenses remain applicable; see the crate COPYING file.

#[cfg(feature = "attributes")]
mod attributes;
mod autolink;
mod g4_email;
pub(crate) mod inlines;
pub mod options;
#[cfg(feature = "phoenix_heex")]
pub mod phoenix_heex;
#[cfg(feature = "shortcodes")]
pub mod shortcodes;
mod table;
mod task;

use std::borrow::Cow;
use std::cmp::{Ordering, min};
use std::collections::HashMap;
use std::fmt::Debug;
use std::mem;
use std::str;

use smallvec::SmallVec;

use crate::Arena;
use crate::native::value::NodeValueRole;
use crate::native::{
    self, AcceptedFact, AppendKind, BufferId, Context, Host, NativeAdmission, NativeKind,
    NativeViolation, PSpan, RSpan, SyntaxRole,
};

macro_rules! native_some { ($label:lifetime, $value:expr) => { match $value { Some(value) => value, None => break $label None } }; }
use crate::ctype::{isdigit, isspace};
use crate::entity;
use crate::matchers::cr_or_lf_matcher;
use crate::node_matches;
use crate::nodes::{
    self, AlertType, Ast, BlockLines, ListDelimType, ListType, Node, NodeAlert, NodeBlockDirective,
    NodeCodeBlock, NodeDescriptionItem, NodeFootnoteDefinition, NodeHeading, NodeHtmlBlock,
    NodeList, NodeMultilineBlockQuote, NodeTaskItem, NodeValue, Sourcepos, ValueText,
};
use crate::parser::inlines::RefMap;
pub use crate::parser::options::Options;
use crate::scanners;
use crate::strings::{self, Case, split_off_front_matter};

pub(crate) const TAB_STOP: usize = 4;
const CODE_INDENT: usize = 4;

// Very deeply nested lists can cause quadratic performance issues.
// This constant is used in open_new_blocks() to limit the nesting
// depth. It is unlikely that a non-contrived markdown document will
// be nested this deeply.
const MAX_LIST_DEPTH: usize = 100;
type ReferenceParse = Option<(usize, Option<(String, ResolvedReference)>)>;

enum TableHandling {
    NotMatched,
    Handled,
    Split,
}

/// Parse a Markdown document to an AST.
///
/// See the documentation of the crate root for an example.
pub fn parse_document<'a>(arena: &'a Arena<'a>, md: &str, options: &Options) -> Node<'a> {
    let root = arena.alloc(
        Ast {
            value: NodeValue::Document,
            sourcepos: (1, 1, 1, 1).into(),
            #[cfg(feature = "attributes")]
            attrs: None,
            native_id: None,
            native_callout_title: false,
            open: true,
            last_line_blank: false,
            table_visited: false,
            lines: None,
        }
        .into(),
    );
    let delimiter_arena = native::FallibleArena::new();
    let mut host = native::LegacyHost;
    let mut ctx = Context::new(&mut host, md.len());
    ctx.capture = false;
    let document = match Parser::new(&mut ctx, arena, root, options, &delimiter_arena).parse(md) {
        Ok(document) => document,
        Err(error) => match error {},
    };
    if options.parse.sourcepos_chars {
        convert_sourcepos_columns_to_chars(document, md);
    }
    document
}

#[cfg(test)]
pub(crate) fn parse_with_host<H: Host>(
    md: &str,
    options: &Options,
    host: &mut H,
) -> Result<(), H::Error> {
    parse_with_finalizer(md, options, host, false, |root, ctx| {
        native::finish(root, ctx)
    })
}

#[cfg(test)]
pub(crate) fn parse_with_finalizer<H: Host, T>(
    md: &str,
    options: &Options,
    host: &mut H,
    direct: bool,
    finish: impl for<'arena> FnOnce(Node<'arena>, &mut Context<'_, H>) -> Result<T, H::Error>,
) -> Result<T, H::Error> {
    let mut ctx = if direct {
        Context::new_direct(host, md)
    } else {
        Context::new(host, md.len())
    };
    parse_in_context(md, options, &mut ctx, finish)
}

/// The Context outlives this helper; neither arena can escape its HRTB finalizer.
pub(crate) fn parse_in_context<H: Host, T>(
    md: &str,
    options: &Options,
    ctx: &mut Context<'_, H>,
    finish: impl for<'arena> FnOnce(Node<'arena>, &mut Context<'_, H>) -> Result<T, H::Error>,
) -> Result<T, H::Error> {
    // An extension reaches production only after it records the same accepted facts.
    let extension = &options.extension;
    let any_g3 = extension.strikethrough || extension.table || extension.tasklist;
    let all_g3 = extension.strikethrough && extension.table && extension.tasklist;
    if (any_g3 && !all_g3)
        || (ctx.inline_policy == native::InlinePolicy::StrictG4 && !all_g3)
        || extension.autolink
        || (extension.inline_footnotes && !extension.footnotes)
        || extension.front_matter_delimiter.is_some()
        || extension.multiline_block_quotes
        || extension.wikilinks_title_after_pipe
        || extension.wikilinks_title_before_pipe
        || extension.underline
        || extension.spoiler
        || extension.greentext
        || extension.image_url_rewriter.is_some()
        || extension.link_url_rewriter.is_some()
        || extension.cjk_friendly_emphasis
        || extension.subtext
        || extension.insert
        || extension.block_directive
        || options.parse.smart
        || options.parse.default_info_string.is_some()
        || options.parse.relaxed_tasklist_matching
        || options.parse.tasklist_in_table
        || options.parse.relaxed_autolinks
        || options.parse.ignore_setext
        || options.parse.broken_link_callback.is_some()
        || options.parse.sourcepos_chars
        || options.parse.escaped_char_spans
        || options.render.escaped_char_spans
        || options.render.ignore_empty_links
    {
        return Err(ctx.host.reject(NativeViolation::UnsupportedConfiguration));
    }
    let arena = native::FallibleArena::new();
    let delimiter_arena = native::FallibleArena::new();
    let mut ast = Ast::new(NodeValue::Document, (1, 1).into());
    ast.native_id = Some(ctx.node(None, NativeKind::Document)?);
    let root: Node<'_> = arena.try_alloc(ast.into(), ctx)?;
    let parser = Parser::new(ctx, &arena, root, options, &delimiter_arena);
    parser.parse(md)?;
    finish(root, ctx)
}

/// Convert all byte-based column values in the AST's sourcepos to char-based.
fn convert_sourcepos_columns_to_chars(document: Node<'_>, md: &str) {
    let lines: Vec<&str> = md.lines().collect();

    let convert = |lc: &mut nodes::LineColumn| {
        if lc.column == 0 {
            return;
        }
        if let Some(line) = lines.get(lc.line.wrapping_sub(1)) {
            lc.column = byte_col_to_char_col(line, lc.column);
        }
    };

    for node in document.descendants() {
        let mut ast = node.data_mut();
        convert(&mut ast.sourcepos.start);
        convert(&mut ast.sourcepos.end);
    }
}

/// Convert a 1-based byte column index to a 1-based char column index for the given line.
fn byte_col_to_char_col(line: &str, byte_col: usize) -> usize {
    // If the byte column points past the end of the line (e.g. position
    // after the final byte, such as softbreak/newline positions),
    // map it to the position after the last Unicode character.
    if byte_col > line.len() {
        return line.chars().count() + 1;
    }

    // Count all chars whose start byte offset is <= byte_idx.
    // Since `char_indices()` yields (byte_offset_of_char_start, char),
    // `take_while` includes exactly the chars that start at or before `byte_idx`.
    let byte_idx = byte_col - 1;
    line.char_indices()
        .take_while(|&(i, _)| i <= byte_idx)
        .count()
}

/// Return whether the byte at the given offset passes the callback.
///
/// Returns `false` if the offset is out of bounds.
fn byte_matches<F>(bytes: &[u8], offset: usize, predicate: F) -> bool
where
    F: Fn(u8) -> bool,
{
    bytes.get(offset).is_some_and(|&b| predicate(b))
}

pub struct Parser<'a: 'd, 'o, 'c, 'd, 'n, 'h, H: Host> {
    ctx: &'n mut Context<'h, H>,
    arena: &'a native::FallibleArena<crate::nodes::AstNode<'a>, H::Lease>,
    delimiter_arena: &'d native::FallibleArena<inlines::Delimiter<'a, 'd>, H::Lease>,
    options: &'o Options<'c>,
    refmap: RefMap,
    inline_tables: inlines::ByteTables,
    footnote_defs: inlines::FootnoteDefs<'a>,
    root: Node<'a>,
    current: Node<'a>,
    line_number: usize,
    offset: usize,
    column: usize,
    thematic_break_kill_pos: usize,
    first_nonspace: usize,
    first_nonspace_column: usize,
    indent: usize,
    blank: bool,
    partially_consumed_tab: bool,
    saw_list: bool,
    working_content: String,
    curline_len: usize,
    curline_end_col: usize,
    last_line_length: usize,
    total_size: usize,
    #[cfg(feature = "phoenix_heex")]
    heex_block_depth: usize,
}

/// A reference link's resolved details.
#[derive(Clone, Debug)]
pub struct ResolvedReference {
    /// The destination URL of the reference link.
    pub url: nodes::ValueText,

    /// The text of the link.
    pub title: nodes::ValueText,
    pub(crate) native: Option<native::ReferenceIdentity>,
}

impl ResolvedReference {
    /// Construct a reference supplied by the compatibility broken-link callback.
    pub fn new(url: String, title: String) -> Self {
        Self {
            url: url.into(),
            title: title.into(),
            native: None,
        }
    }
}

struct FootnoteDefinition<'a> {
    ix: Option<u32>,
    node: Node<'a>,
    name: String,
    total_references: u32,
}

impl<'a: 'd, 'o, 'c, 'd, 'n, 'h, H: Host> Parser<'a, 'o, 'c, 'd, 'n, 'h, H>
where
    'c: 'o,
{
    fn new(
        ctx: &'n mut Context<'h, H>,
        arena: &'a native::FallibleArena<crate::nodes::AstNode<'a>, H::Lease>,
        root: Node<'a>,
        options: &'o Options<'c>,
        delimiter_arena: &'d native::FallibleArena<inlines::Delimiter<'a, 'd>, H::Lease>,
    ) -> Self {
        Parser {
            ctx,
            arena,
            delimiter_arena,
            options,
            refmap: RefMap::new(),
            inline_tables: inlines::ByteTables::new(options),
            footnote_defs: inlines::FootnoteDefs::new(),
            root,
            current: root,
            line_number: 0,
            offset: 0,
            column: 0,
            thematic_break_kill_pos: 0,
            first_nonspace: 0,
            first_nonspace_column: 0,
            indent: 0,
            blank: false,
            partially_consumed_tab: false,
            saw_list: false,
            working_content: String::new(),
            curline_len: 0,
            curline_end_col: 0,
            last_line_length: 0,
            total_size: 0,
            #[cfg(feature = "phoenix_heex")]
            heex_block_depth: 0,
        }
    }

    fn parse(mut self, mut s: &str) -> Result<Node<'a>, H::Error> {
        if let Some(delimiter) = &self.options.extension.front_matter_delimiter
            && let Some((front_matter, rest)) = split_off_front_matter(s, delimiter)
        {
            self.handle_front_matter(front_matter, delimiter)?;
            s = rest;
        }

        let s = s;
        let sb = s.as_bytes();

        let end = s.len();
        self.total_size = end;

        let mut ix = 0;
        let matcher = cr_or_lf_matcher();
        while ix < end {
            let mut eol = self
                .ctx
                .find(&sb[ix..], |part| matcher.find(part))?
                .map_or(end, |at| ix + at);
            if eol < end {
                if sb[eol] == b'\r' {
                    eol += 1;
                    if eol < end && sb[eol] == b'\n' {
                        eol += 1;
                    }
                } else if sb[eol] == b'\n' {
                    eol += 1;
                }
            }

            self.ctx.line_start = ix;
            self.process_line(&s[ix..eol])?;

            ix = eol;
        }

        self.finalize_document()?;
        self.postprocess_text_nodes(self.root)?;
        // Email matching uses actual decoded text, including an @ from an
        // entity. Coalesce the ordinary text tree before looking for it.
        if self.ctx.inline_policy == native::InlinePolicy::StrictG4
            && self
                .ctx
                .find(sb, |part| {
                    part.iter().position(|&byte| matches!(byte, b'@' | b'&'))
                })?
                .is_some()
        {
            self.finish_g4_email(self.root)?;
        }
        let working = mem::take(&mut self.working_content);
        self.ctx.discard_string(working)?;
        self.refmap.release_owned(self.ctx)?;
        Ok(self.root)
    }

    fn handle_front_matter(&mut self, front_matter: &str, delimiter: &str) -> Result<(), H::Error> {
        {
            let (lines, _) = strings::count_newlines(front_matter);

            let stripped_front_matter = strings::remove_trailing_blank_lines_slice(front_matter);
            let (stripped_lines, _) = strings::count_newlines(stripped_front_matter);

            let node = self.add_child(
                self.root,
                NodeValue::FrontMatter(front_matter.to_string()),
                1,
            )?;
            self.finalize(node)?.unwrap();

            node.data_mut().sourcepos = Sourcepos {
                start: nodes::LineColumn { line: 1, column: 1 },
                end: nodes::LineColumn {
                    line: 1 + stripped_lines,
                    column: delimiter.len(),
                },
            };
            self.line_number += lines;
        };
        Ok(())
    }

    fn process_line(&mut self, line: &str) -> Result<(), H::Error> {
        {
            let bytes = line.as_bytes();

            self.curline_len = line.len();
            self.curline_end_col = line.len();
            if self.curline_end_col > 0 && bytes[self.curline_end_col - 1] == b'\n' {
                self.curline_end_col -= 1;
            }
            if self.curline_end_col > 0 && bytes[self.curline_end_col - 1] == b'\r' {
                self.curline_end_col -= 1;
            }

            self.offset = 0;
            self.column = 0;
            self.first_nonspace = 0;
            self.first_nonspace_column = 0;
            self.indent = 0;
            self.thematic_break_kill_pos = 0;
            self.blank = false;
            self.partially_consumed_tab = false;

            if self.line_number == 0 && line.len() >= 3 && line.starts_with('\u{feff}') {
                self.offset += 3;
                if let Some(node) = self.root.data().native_id {
                    self.ctx.fact(AcceptedFact::Syntax {
                        node,
                        role: SyntaxRole::LeadingBom,
                        span: PSpan {
                            start: self.ctx.line_start,
                            end: self.ctx.line_start + 3,
                        },
                    })?;
                }
            }

            self.line_number += 1;
            if let Some(node) = self.root.data().native_id {
                self.ctx.fact(AcceptedFact::BlockConsume {
                    node,
                    span: PSpan {
                        start: self.ctx.line_start,
                        end: self.ctx.line_start + line.len(),
                    },
                })?;
            }

            if let Some((last_matched_container, all_matched)) = self.check_open_blocks(line)? {
                let mut container = last_matched_container;
                let current = self.current;
                self.open_new_blocks(&mut container, line, all_matched)?;

                if current.same_node(self.current) {
                    self.add_text_to_container(container, last_matched_container, line)?;
                }
            }

            if self.blank
                && !node_matches!(
                    self.current,
                    NodeValue::CodeBlock(..) | NodeValue::HtmlBlock(..)
                )
                // A consumed table line has an empty remainder, not blank
                // source. Real blank lines close the Table/Row before here.
                && !(self.ctx.capture
                    && node_matches!(self.current, NodeValue::Table(..) | NodeValue::TableRow(..)))
                && let Some(node) = self.root.data().native_id
            {
                self.ctx.fact(AcceptedFact::Syntax {
                    node,
                    role: SyntaxRole::BlankLine,
                    span: PSpan {
                        start: self.ctx.line_start,
                        end: self.ctx.line_start + line.len(),
                    },
                })?;
            }
            self.last_line_length = self.curline_end_col;

            self.curline_len = 0;
            self.curline_end_col = 0;
        };
        Ok(())
    }

    ///////////////////////
    // Check open blocks //
    ///////////////////////

    fn check_open_blocks(&mut self, line: &str) -> Result<Option<(Node<'a>, bool)>, H::Error> {
        let result = 'native: {
            let (all_matched, mut container) =
                native_some!('native, self.check_open_blocks_inner(self.root, line)?);

            if !all_matched {
                container = container.parent().unwrap();
            }

            Some((container, all_matched))
        };
        Ok(result)
    }

    fn check_open_blocks_inner(
        &mut self,
        mut container: Node<'a>,
        line: &str,
    ) -> Result<Option<(bool, Node<'a>)>, H::Error> {
        let result = 'native: {
            let mut all_matched = false;

            loop {
                self.ctx.event()?;
                if !container.last_child_is_open() {
                    all_matched = true;
                    break;
                }
                container = container.last_child().unwrap();
                let ast = &mut container.data_mut();

                self.find_first_nonspace(line)?;
                let accepted_prefix_start = self.offset;

                match ast.value {
                    NodeValue::BlockQuote => {
                        if !self.parse_block_quote_prefix(line)? {
                            break;
                        }
                    }
                    NodeValue::Item(ref nl) => {
                        if !self.parse_node_item_prefix(line, container, nl)? {
                            break;
                        }
                    }
                    NodeValue::DescriptionItem(ref di) => {
                        if !self.parse_description_item_prefix(line, container, di)? {
                            break;
                        }
                    }
                    NodeValue::CodeBlock(..) => {
                        if !native_some!('native, self.parse_code_block_prefix(line, container, ast)?)
                        {
                            break;
                        }
                    }
                    NodeValue::HtmlBlock(ref nhb) => {
                        if !self.parse_html_block_prefix(nhb.block_type) {
                            break;
                        }
                    }
                    NodeValue::Paragraph => {
                        if self.blank {
                            break;
                        }
                    }
                    NodeValue::Table(..) => {
                        let matches = if self.ctx.capture {
                            table::matches_with_context(self.ctx, &line[self.first_nonspace..])?
                        } else {
                            table::matches(
                                &line[self.first_nonspace..],
                                self.options.extension.spoiler,
                            )
                        };
                        if !matches {
                            break;
                        }
                    }
                    NodeValue::Heading(..)
                    | NodeValue::TableRow(..)
                    | NodeValue::TableCell
                    | NodeValue::Subtext => {
                        break;
                    }
                    NodeValue::FootnoteDefinition(..) => {
                        if !self.parse_footnote_definition_block_prefix(line)? {
                            break;
                        }
                    }
                    NodeValue::MultilineBlockQuote(..) => {
                        native_some!('native, self.parse_multiline_block_quote_prefix(line, container, ast)?);
                    }
                    NodeValue::Alert(ref alert) => {
                        if alert.multiline {
                            native_some!('native, self.parse_multiline_block_quote_prefix(line, container, ast)?);
                        } else if !self.parse_block_quote_prefix(line)? {
                            break;
                        }
                    }
                    NodeValue::BlockDirective(..) => {
                        native_some!('native, self.parse_block_directive_prefix(line, container, ast)?);
                    }
                    _ => {}
                }
                if let Some(node) = ast.native_id {
                    self.ctx.fact(AcceptedFact::BlockConsume {
                        node,
                        span: PSpan {
                            start: self.ctx.line_start,
                            end: self.ctx.line_start + line.len(),
                        },
                    })?;
                    let role = match ast.value {
                        NodeValue::BlockQuote => Some(SyntaxRole::QuotePrefix),
                        NodeValue::Item(..) => Some(SyntaxRole::ListPrefix),
                        NodeValue::DescriptionItem(..) => Some(SyntaxRole::DescriptionIndent),
                        NodeValue::CodeBlock(..) => Some(SyntaxRole::CodeIndent),
                        NodeValue::FootnoteDefinition(..) => Some(SyntaxRole::FootnoteIndent),
                        _ => None,
                    };
                    if let Some(role) = role.filter(|_| self.offset != accepted_prefix_start) {
                        self.ctx.fact(AcceptedFact::Syntax {
                            node,
                            role,
                            span: PSpan {
                                start: self.ctx.line_start + accepted_prefix_start,
                                end: self.ctx.line_start + self.offset,
                            },
                        })?;
                    }
                    if matches!(ast.value, NodeValue::FootnoteDefinition(_))
                        && self.offset < line.len()
                    {
                        self.ctx.fact(AcceptedFact::Syntax {
                            node,
                            role: SyntaxRole::FootnoteContent,
                            span: PSpan {
                                start: self.ctx.line_start + self.offset,
                                end: self.ctx.line_start + line.len(),
                            },
                        })?;
                    }
                }
            }

            Some((all_matched, container))
        };
        Ok(result)
    }

    fn find_first_nonspace(&mut self, line: &str) -> Result<(), H::Error> {
        {
            let mut chars_to_tab = TAB_STOP - (self.column % TAB_STOP);
            let bytes = line.as_bytes();

            if self.first_nonspace <= self.offset {
                self.first_nonspace = self.offset;
                self.first_nonspace_column = self.column;

                loop {
                    self.ctx.scan_step()?;
                    match bytes.get(self.first_nonspace) {
                        Some(b' ') => {
                            self.first_nonspace += 1;
                            self.first_nonspace_column += 1;
                            chars_to_tab -= 1;
                            if chars_to_tab == 0 {
                                chars_to_tab = TAB_STOP;
                            }
                        }
                        Some(b'\t') => {
                            self.first_nonspace += 1;
                            self.first_nonspace_column += chars_to_tab;
                            chars_to_tab = TAB_STOP;
                        }
                        _ => break,
                    }
                }
            }

            self.indent = self.first_nonspace_column - self.column;
            self.blank = bytes
                .get(self.first_nonspace)
                .is_none_or(|&b| strings::is_line_end_char(b));
        };
        Ok(())
    }

    fn parse_block_quote_prefix(&mut self, line: &str) -> Result<bool, H::Error> {
        let result = 'native: {
            let bytes = line.as_bytes();
            let indent = self.indent;

            // When already inside a blockquote, always treat `>` as a blockquote marker,
            // even if it's a lone `>` (which represents a blank line in the blockquote).
            // This allows multi-paragraph blockquotes without requiring a space after `>`.
            if indent <= 3 && bytes.get(self.first_nonspace) == Some(&b'>') {
                self.advance_offset(line, indent + 1, true)?;

                if byte_matches(bytes, self.offset, strings::is_space_or_tab) {
                    self.advance_offset(line, 1, true)?;
                }

                break 'native true;
            }

            false
        };
        Ok(result)
    }

    fn is_greentext(&self, line: &str) -> bool {
        if !self.options.extension.greentext {
            return false;
        }

        !byte_matches(
            line.as_bytes(),
            self.first_nonspace + 1,
            strings::is_space_or_tab,
        )
    }

    fn parse_node_item_prefix(
        &mut self,
        line: &str,
        container: Node<'a>,
        nl: &NodeList,
    ) -> Result<bool, H::Error> {
        let result = {
            if self.indent >= nl.marker_offset + nl.padding {
                self.advance_offset(line, nl.marker_offset + nl.padding, true)?;
                true
            } else if self.blank && container.first_child().is_some() {
                let offset = self.first_nonspace - self.offset;
                self.advance_offset(line, offset, false)?;
                true
            } else {
                false
            }
        };
        Ok(result)
    }

    fn parse_description_item_prefix(
        &mut self,
        line: &str,
        container: Node<'a>,
        di: &NodeDescriptionItem,
    ) -> Result<bool, H::Error> {
        let result = {
            if self.indent >= di.marker_offset + di.padding {
                self.advance_offset(line, di.marker_offset + di.padding, true)?;
                true
            } else if self.blank && container.first_child().is_some() {
                let offset = self.first_nonspace - self.offset;
                self.advance_offset(line, offset, false)?;
                true
            } else {
                false
            }
        };
        Ok(result)
    }

    fn parse_code_block_prefix(
        &mut self,
        line: &str,
        container: Node<'a>,
        ast: &mut Ast,
    ) -> Result<Option<bool>, H::Error> {
        let result = 'native: {
            let (fenced, fence_char, fence_length, fence_offset) = match ast.value {
                NodeValue::CodeBlock(ref ncb) => (
                    ncb.fenced,
                    ncb.fence_char,
                    ncb.fence_length,
                    ncb.fence_offset,
                ),
                _ => unreachable!(),
            };

            if !fenced {
                if self.indent >= CODE_INDENT {
                    self.advance_offset(line, CODE_INDENT, true)?;
                    break 'native Some(true);
                } else if self.blank {
                    let offset = self.first_nonspace - self.offset;
                    self.advance_offset(line, offset, false)?;
                    break 'native Some(true);
                }
                break 'native Some(false);
            }

            let bytes = line.as_bytes();
            let matched = if self.indent <= 3 && bytes.get(self.first_nonspace) == Some(&fence_char)
            {
                scanners::close_code_fence_with_host(&line[self.first_nonspace..], self.ctx)?
                    .unwrap_or(0)
            } else {
                0
            };

            if matched >= fence_length {
                let closing_start = self.offset;
                let NodeValue::CodeBlock(ref mut ncb) = ast.value else {
                    unreachable!();
                };
                ncb.closed = true;
                self.advance_offset(line, matched, false)?;
                if let Some(node) = ast.native_id {
                    self.ctx.fact(AcceptedFact::Syntax {
                        node,
                        role: SyntaxRole::FenceClosing,
                        span: PSpan {
                            start: self.ctx.line_start + closing_start,
                            end: self.ctx.line_start + line.len(),
                        },
                    })?;
                    self.ctx.fact(AcceptedFact::BlockConsume {
                        node,
                        span: PSpan {
                            start: self.ctx.line_start,
                            end: self.ctx.line_start + line.len(),
                        },
                    })?;
                }
                self.current = self.finalize_borrowed(container, ast)?.unwrap();
                break 'native None;
            }

            let mut i = fence_offset;
            while i > 0 && byte_matches(bytes, self.offset, strings::is_space_or_tab) {
                self.advance_offset(line, 1, true)?;
                i -= 1;
            }
            Some(true)
        };
        Ok(result)
    }

    fn parse_html_block_prefix(&self, t: u8) -> bool {
        match t {
            1..=5 => true,
            6 | 7 => !self.blank,
            _ => unreachable!(),
        }
    }

    fn parse_footnote_definition_block_prefix(&mut self, line: &str) -> Result<bool, H::Error> {
        let result = {
            if self.indent >= 4 {
                self.advance_offset(line, 4, true)?;
                true
            } else {
                line == "\n" || line == "\r\n"
            }
        };
        Ok(result)
    }

    fn parse_multiline_block_quote_prefix(
        &mut self,
        line: &str,
        container: Node<'a>,
        ast: &mut Ast,
    ) -> Result<Option<()>, H::Error> {
        let result = 'native: {
            // XXX: refactoring revealed that, unlike parse_code_block_prefix, this
            // function never fails to match without signalling 'should_continue'
            // (which is a `Some(false)` in that function). Is that odd?

            let (fence_length, fence_offset) = match ast.value {
                NodeValue::MultilineBlockQuote(ref node_value) => {
                    (node_value.fence_length, node_value.fence_offset)
                }
                NodeValue::Alert(ref node_value) => {
                    (node_value.fence_length, node_value.fence_offset)
                }
                _ => unreachable!(),
            };

            let bytes = line.as_bytes();
            let matched = if self.indent <= 3 && bytes.get(self.first_nonspace) == Some(&b'>') {
                scanners::close_multiline_block_quote_fence(&line[self.first_nonspace..])
                    .unwrap_or(0)
            } else {
                0
            };

            if matched >= fence_length {
                self.advance_offset(line, matched, false)?;

                // The last child, like an indented codeblock, could be left open.
                // Make sure it's finalized.
                if container.last_child_is_open() {
                    let mut child = container.last_child().unwrap();
                    // Descend to the deepest-last open child before finalizing it.
                    // Stop descending when encountering a `List` node because
                    // list structure must be finalized at the item level. This
                    // ensures nested open children (e.g. indented code blocks)
                    // are closed first while avoiding descending into lists.
                    while child.last_child_is_open() && !node_matches!(child, NodeValue::List(..)) {
                        child = child.last_child().unwrap();
                    }
                    let child_ast = &mut child.data_mut();

                    self.finalize_borrowed(child, child_ast)?.unwrap();
                }

                self.fix_zero_end_columns(container)?;

                self.current = self.finalize_borrowed(container, ast)?.unwrap();
                break 'native None;
            }

            let mut i = fence_offset;
            while i > 0 && byte_matches(bytes, self.offset, strings::is_space_or_tab) {
                self.advance_offset(line, 1, true)?;
                i -= 1;
            }
            Some(())
        };
        Ok(result)
    }

    fn parse_block_directive_prefix(
        &mut self,
        line: &str,
        container: Node<'a>,
        ast: &mut Ast,
    ) -> Result<Option<()>, H::Error> {
        let result = 'native: {
            let (fence_length, fence_offset) = match ast.value {
                NodeValue::BlockDirective(ref nbd) => (nbd.fence_length, nbd.fence_offset),
                _ => unreachable!(),
            };

            let bytes = line.as_bytes();
            let matched = if self.indent <= 3 && bytes.get(self.first_nonspace) == Some(&b':') {
                scanners::close_block_directive_fence(&line[self.first_nonspace..]).unwrap_or(0)
            } else {
                0
            };

            if matched >= fence_length {
                self.advance_offset(line, matched, false)?;

                // The last child, like an indented codeblock, could be left open.
                // Make sure it's finalized.
                if container.last_child_is_open() {
                    let mut child = container.last_child().unwrap();
                    // Descend to the deepest-last open child before finalizing it.
                    // Stop descending when encountering a `List` node because
                    // list structure must be finalized at the item level. This
                    // ensures nested open children (e.g. indented code blocks)
                    // are closed first while avoiding descending into lists.
                    while child.last_child_is_open() && !node_matches!(child, NodeValue::List(..)) {
                        child = child.last_child().unwrap();
                    }
                    let child_ast = &mut child.data_mut();

                    self.finalize_borrowed(child, child_ast)?.unwrap();
                }

                self.fix_zero_end_columns(container)?;

                self.current = self.finalize_borrowed(container, ast)?.unwrap();
                break 'native None;
            }

            let mut i = fence_offset;
            while i > 0 && byte_matches(bytes, self.offset, strings::is_space_or_tab) {
                self.advance_offset(line, 1, true)?;
                i -= 1;
            }
            Some(())
        };
        Ok(result)
    }

    fn deepest_last_end(node: Node<'a>) -> Option<nodes::LineColumn> {
        let mut last = node.last_child()?;
        let mut best: Option<nodes::LineColumn> = None;

        loop {
            let end = last.data().sourcepos.end;
            if end.column != 0 {
                let wider = match best {
                    None => true,
                    Some(b) => end.line > b.line || (end.line == b.line && end.column > b.column),
                };
                if wider {
                    best = Some(end);
                }
            }
            match last.last_child() {
                Some(child) => last = child,
                None => break,
            }
        }

        best
    }

    // Walk the subtree rooted at each child of `container` in post-order
    // and, where a node's end column is zero, attempt to adopt the widest
    // non-zero end column along its deepest-last path; otherwise
    // fall back to the node's start position.
    // Returns a candidate end position for `container` if found.
    fn fix_zero_end_columns(
        &mut self,
        container: Node<'a>,
    ) -> Result<Option<nodes::LineColumn>, H::Error> {
        if self.ctx.capture {
            return Ok(None);
        }
        let result = 'native: {
            // explicit stack for post-order traversal: (node, visited)
            let mut stack: Vec<(Node<'a>, bool)> = Vec::new();

            for ch in container.children() {
                stack.push((ch, false));

                while let Some((node, visited)) = stack.pop() {
                    if !visited {
                        stack.push((node, true));
                        for c in node.children() {
                            stack.push((c, false));
                        }
                    } else {
                        let end_col = node.data().sourcepos.end.column;
                        if end_col == 0 {
                            if let Some(pos) = Self::deepest_last_end(node) {
                                node.data_mut().sourcepos.end = pos;
                            } else {
                                let mut ast = node.data_mut();
                                ast.sourcepos.end = ast.sourcepos.start;
                            }
                        }
                    }
                }
            }

            // Compute a candidate end position for the container by looking at
            // its last child. Return it only if it has a non-zero column; the
            // caller can fall back to the container's start position if desired.
            //
            // We originally looked at the deepest-last descendant, but there
            // may be intermediate containers that are larger than it, which we
            // should use instead. If looking just at the last child isn't
            // enough in some circumstances, we should consider using the widest
            // of the last descendants.
            if let Some(last_desc) = container.last_child() {
                let last_end = last_desc.data().sourcepos.end;
                if last_end.column != 0 {
                    break 'native Some(last_end);
                }
            }
            None
        };
        Ok(result)
    }

    /////////////////////
    // Open new blocks //
    /////////////////////

    fn open_new_blocks(
        &mut self,
        container: &mut Node<'a>,
        line: &str,
        all_matched: bool,
    ) -> Result<(), H::Error> {
        {
            let mut maybe_lazy = node_matches!(self.current, NodeValue::Paragraph);
            let mut depth = 0;

            while !node_matches!(
                container,
                NodeValue::CodeBlock(..) | NodeValue::HtmlBlock(..)
            ) && !{
                #[cfg(feature = "phoenix_heex")]
                {
                    node_matches!(container, NodeValue::HeexBlock(..))
                }
                #[cfg(not(feature = "phoenix_heex"))]
                {
                    false
                }
            } {
                depth += 1;
                self.find_first_nonspace(line)?;
                let indented = self.indent >= CODE_INDENT;
                let mut table_split = false;

                if !((!indented
                    && (self.handle_block_directive(container, line)?
                        || self.handle_alert(container, line)?
                        || self.handle_multiline_blockquote(container, line)?
                        || self.handle_blockquote(container, line)?
                        || self.handle_atx_heading(container, line)?
                        || self.handle_atx_subtext(container, line)?
                        || self.handle_code_fence(container, line)?
                        || self.handle_heex_directive_block(container, line)?
                        || self.handle_heex_expression_block(container, line)?
                        || self.handle_heex_block(container, line)?
                        || self.handle_html_block(container, line)?
                        || self.handle_setext_heading(container, line)?
                        || self.handle_thematic_break(container, line, all_matched)?
                        || self.handle_table_of_contents(container, line)?
                        || self.handle_footnote(container, line, depth)?
                        || self.handle_description_list(container, line)?))
                    || self.handle_list(container, line, indented, depth)?
                    || self.handle_code_block(container, line, indented, maybe_lazy)?
                    || match self.handle_table(container, line, indented)? {
                        TableHandling::NotMatched => false,
                        TableHandling::Handled => true,
                        TableHandling::Split => {
                            table_split = true;
                            true
                        }
                    })
                {
                    break;
                }

                if table_split || container.data().value.accepts_lines() {
                    break;
                }

                maybe_lazy = false;
            }
        };
        Ok(())
    }

    fn handle_block_directive(
        &mut self,
        container: &mut Node<'a>,
        line: &str,
    ) -> Result<bool, H::Error> {
        let result = 'native: {
            let Some(matched) = self.detect_block_directive(line)? else {
                break 'native false;
            };

            let first_nonspace = self.first_nonspace;
            let offset = self.offset;

            let info_start = first_nonspace + matched;
            let mut info_end = info_start;
            while info_end < line.len() && !strings::is_line_end_char(line.as_bytes()[info_end]) {
                info_end += 1;
            }

            let mut info = entity::unescape_html(&line[info_start..info_end]);
            strings::trim_cow(&mut info);
            let mut info = info.into_owned();
            strings::unescape(&mut info);

            let nbd = NodeBlockDirective {
                fence_length: matched,
                fence_offset: self.indent,
                info,
            };

            *container = self.add_child(
                container,
                NodeValue::BlockDirective(Box::new(nbd)),
                self.first_nonspace + 1,
            )?;

            let line_to_consume = line.len() - offset;
            self.advance_offset(line, line_to_consume, false)?;

            true
        };
        Ok(result)
    }

    fn detect_block_directive(&mut self, line: &str) -> Result<Option<usize>, H::Error> {
        let result = {
            if self.options.extension.block_directive {
                scanners::open_block_directive_fence(&line[self.first_nonspace..])
            } else {
                None
            }
        };
        Ok(result)
    }

    fn handle_alert(&mut self, container: &mut Node<'a>, line: &str) -> Result<bool, H::Error> {
        if self.ctx.capture {
            return self.handle_native_callout(container, line);
        }
        let result = 'native: {
            let Some(alert_type) = self.detect_alert(line)? else {
                break 'native false;
            };

            let alert_startpos = self.first_nonspace;
            let mut title_startpos = self.first_nonspace;
            let mut fence_length = 0;

            let bytes = line.as_bytes();
            // The checks made here co-operate with the alert_start scanner for soundness.
            while bytes[title_startpos] != b']' {
                if bytes[title_startpos] == b'>' {
                    fence_length += 1
                }
                title_startpos += 1;
            }
            title_startpos += 1;

            if fence_length == 2
                || (fence_length >= 3 && !self.options.extension.multiline_block_quotes)
            {
                break 'native false;
            }

            // anything remaining on this line is considered an alert title
            let mut title = entity::unescape_html(&line[title_startpos..]);
            strings::trim_cow(&mut title);
            let mut title = title.into_owned();
            strings::unescape(&mut title);

            let na = NodeAlert {
                alert_type,
                multiline: fence_length >= 3,
                fence_length,
                fence_offset: self.indent,
                title: if title.is_empty() { None } else { Some(title) },
            };

            let offset = self.curline_len - self.offset - strings::newlines_of(line);
            self.advance_offset(line, offset, false)?;

            *container = self.add_child(
                container,
                NodeValue::Alert(Box::new(na)),
                alert_startpos + 1,
            )?;

            true
        };
        Ok(result)
    }

    fn handle_native_callout(
        &mut self,
        container: &mut Node<'a>,
        line: &str,
    ) -> Result<bool, H::Error> {
        if !self.options.extension.alerts {
            return Ok(false);
        }
        let quote_start = self.first_nonspace;
        let Some(header) = native::callout::scan_line(
            line.as_bytes(),
            quote_start,
            self.first_nonspace_column,
            self.ctx,
        )?
        else {
            return Ok(false);
        };
        let whitespace = self.offset;
        *container = self.add_child(container, NodeValue::BlockQuote, quote_start + 1)?;
        self.block_whitespace(*container, whitespace, quote_start)?;
        self.syntax(
            *container,
            SyntaxRole::QuotePrefix,
            quote_start,
            header.marker.start,
        )?;
        self.block_whitespace(*container, header.marker.end, header.title_start)?;
        self.advance_offset(line, header.title_start - self.offset, false)?;
        let title = if header.has_title {
            let paragraph =
                self.add_child(*container, NodeValue::Paragraph, header.title_start + 1)?;
            self.add_line(paragraph, line)?;
            let mut ast = paragraph.data_mut();
            // A title has inline grammar. A reference-definition lookalike at
            // its start must not detach the title or create a block definition.
            ast.open = false;
            ast.native_callout_title = true;
            ast.sourcepos.end = (self.line_number, self.curline_end_col).into();
            let content = &mut ast.lines_mut().content;
            *content = self.take_content(content)?;
            let id = ast
                .native_id
                .ok_or_else(|| self.ctx.host.reject(NativeViolation::InvalidProvenance))?;
            Some(id)
        } else {
            None
        };
        let node = container
            .data()
            .native_id
            .ok_or_else(|| self.ctx.host.reject(NativeViolation::InvalidProvenance))?;
        self.ctx.fact(AcceptedFact::Callout {
            node,
            kind: header.kind,
            disclosure: header.disclosure,
            marker: PSpan {
                start: self.ctx.line_start + header.marker.start,
                end: self.ctx.line_start + header.marker.end,
            },
            title,
        })?;
        let end = self.curline_len - strings::newlines_of(line);
        self.advance_offset(line, end - self.offset, false)?;
        Ok(true)
    }

    fn detect_alert(&mut self, line: &str) -> Result<Option<AlertType>, H::Error> {
        let result = {
            if self.options.extension.alerts
                && line.as_bytes().get(self.first_nonspace) == Some(&b'>')
            {
                scanners::alert_start(&line[self.first_nonspace..])
            } else {
                None
            }
        };
        Ok(result)
    }

    fn handle_multiline_blockquote(
        &mut self,
        container: &mut Node<'a>,
        line: &str,
    ) -> Result<bool, H::Error> {
        let result = 'native: {
            let Some(matched) = self.detect_multiline_blockquote(line)? else {
                break 'native false;
            };

            let first_nonspace = self.first_nonspace;
            let offset = self.offset;
            let nmbc = NodeMultilineBlockQuote {
                fence_length: matched,
                fence_offset: self.indent,
            };

            *container = self.add_child(
                container,
                NodeValue::MultilineBlockQuote(nmbc),
                self.first_nonspace + 1,
            )?;

            self.advance_offset(line, first_nonspace + matched - offset, false)?;

            true
        };
        Ok(result)
    }

    fn detect_multiline_blockquote(&mut self, line: &str) -> Result<Option<usize>, H::Error> {
        let result = {
            if self.options.extension.multiline_block_quotes {
                scanners::open_multiline_block_quote_fence(&line[self.first_nonspace..])
            } else {
                None
            }
        };
        Ok(result)
    }

    fn handle_blockquote(
        &mut self,
        container: &mut Node<'a>,
        line: &str,
    ) -> Result<bool, H::Error> {
        let result = 'native: {
            if !self.detect_blockquote(line)? {
                break 'native false;
            }

            let blockquote_startpos = self.first_nonspace;
            let whitespace_start = self.offset;

            let offset = self.first_nonspace + 1 - self.offset;
            self.advance_offset(line, offset, false)?;
            if byte_matches(line.as_bytes(), self.offset, strings::is_space_or_tab) {
                self.advance_offset(line, 1, true)?;
            }
            *container =
                self.add_child(container, NodeValue::BlockQuote, blockquote_startpos + 1)?;
            self.block_whitespace(container, whitespace_start, blockquote_startpos)?;
            self.syntax(
                container,
                SyntaxRole::QuotePrefix,
                blockquote_startpos,
                self.offset,
            )?;

            true
        };
        Ok(result)
    }

    fn detect_blockquote(&mut self, line: &str) -> Result<bool, H::Error> {
        let result =
            { line.as_bytes().get(self.first_nonspace) == Some(&b'>') && !self.is_greentext(line) };
        Ok(result)
    }

    fn handle_atx_heading(
        &mut self,
        container: &mut Node<'a>,
        line: &str,
    ) -> Result<bool, H::Error> {
        let result = 'native: {
            let Some(matched) = self.detect_atx_heading(line)? else {
                break 'native false;
            };

            let heading_startpos = self.first_nonspace;
            let offset = self.offset;
            self.advance_offset(line, heading_startpos + matched - offset, false)?;
            *container = self.add_child(
                container,
                NodeValue::Heading(NodeHeading::default()),
                heading_startpos + 1,
            )?;
            self.block_whitespace(container, offset, heading_startpos)?;
            self.syntax(
                container,
                SyntaxRole::HeadingPrefix,
                heading_startpos,
                self.offset,
            )?;

            let bytes = line.as_bytes();
            let mut hashpos = bytes[self.first_nonspace..]
                .iter()
                .position(|&c| c == b'#')
                .unwrap()
                + self.first_nonspace;
            let mut level = 0;
            while hashpos < bytes.len() && bytes[hashpos] == b'#' {
                level += 1;
                hashpos += 1;
            }

            let container_ast = &mut container.data_mut();
            container_ast.value = NodeValue::Heading(NodeHeading {
                level,
                setext: false,
                closed: false,
            });

            true
        };
        Ok(result)
    }

    fn detect_atx_heading(&mut self, line: &str) -> Result<Option<usize>, H::Error> {
        let result =
            { scanners::atx_heading_start_with_host(&line[self.first_nonspace..], self.ctx)? };
        Ok(result)
    }

    fn handle_atx_subtext(
        &mut self,
        container: &mut Node<'a>,
        line: &str,
    ) -> Result<bool, H::Error> {
        let result = 'native: {
            let Some(matched) = self.detect_atx_subtext(line)? else {
                break 'native false;
            };

            let heading_startpos = self.first_nonspace;
            let offset = self.offset;
            self.advance_offset(line, heading_startpos + matched - offset, false)?;
            *container = self.add_child(container, NodeValue::Subtext, heading_startpos + 1)?;

            let container_ast = &mut container.data_mut();
            container_ast.value = NodeValue::Subtext;

            true
        };
        Ok(result)
    }

    fn detect_atx_subtext(&mut self, line: &str) -> Result<Option<usize>, H::Error> {
        let result = {
            if self.options.extension.subtext {
                scanners::atx_subtext_start(&line[self.first_nonspace..])
            } else {
                None
            }
        };
        Ok(result)
    }

    fn handle_code_fence(
        &mut self,
        container: &mut Node<'a>,
        line: &str,
    ) -> Result<bool, H::Error> {
        let result = 'native: {
            let Some(matched) = self.detect_code_fence(line)? else {
                break 'native false;
            };

            let first_nonspace = self.first_nonspace;
            let offset = self.offset;
            let ncb = NodeCodeBlock {
                fenced: true,
                fence_char: line.as_bytes()[first_nonspace],
                fence_length: matched,
                fence_offset: self.indent,
                info: self.ctx.empty_value(),
                literal: self.ctx.empty_value(),
                closed: false,
            };
            let ncb = self.ctx.boxed(ncb)?;
            *container = self.add_child(
                container,
                NodeValue::CodeBlock(ncb),
                self.first_nonspace + 1,
            )?;
            self.advance_offset(line, first_nonspace + matched - offset, false)?;
            self.block_whitespace(container, offset, first_nonspace)?;
            self.syntax(
                container,
                SyntaxRole::FenceOpening,
                first_nonspace,
                self.offset,
            )?;

            true
        };
        Ok(result)
    }

    fn detect_code_fence(&mut self, line: &str) -> Result<Option<usize>, H::Error> {
        let result =
            { scanners::open_code_fence_with_host(&line[self.first_nonspace..], self.ctx)? };
        Ok(result)
    }

    fn handle_html_block(
        &mut self,
        container: &mut Node<'a>,
        line: &str,
    ) -> Result<bool, H::Error> {
        let result = 'native: {
            let Some(matched) = self.detect_html_block(container, line)? else {
                break 'native false;
            };

            let nhb = NodeHtmlBlock {
                block_type: matched as u8,
                literal: self.ctx.empty_value(),
            };

            *container = self.add_child(
                container,
                NodeValue::HtmlBlock(nhb),
                self.first_nonspace + 1,
            )?;

            true
        };
        Ok(result)
    }

    fn detect_html_block(
        &mut self,
        container: Node<'a>,
        line: &str,
    ) -> Result<Option<usize>, H::Error> {
        let result = {
            let first =
                scanners::html_block_start_with_host(&line[self.first_nonspace..], self.ctx)?;
            if first.is_none() && !node_matches!(container, NodeValue::Paragraph) {
                scanners::html_block_start_7_with_host(&line[self.first_nonspace..], self.ctx)?
            } else {
                first
            }
        };
        Ok(result)
    }

    #[cfg(feature = "phoenix_heex")]
    fn handle_heex_block(
        &mut self,
        container: &mut Node<'a>,
        line: &str,
    ) -> Result<bool, H::Error> {
        let result = 'native: {
            let Some(tag_name_len) = self.detect_heex_block(line)? else {
                break 'native false;
            };

            let tag_name =
                line[self.first_nonspace + 1..=self.first_nonspace + tag_name_len].to_string();

            let nhb = phoenix_heex::NodeHeexBlock {
                literal: String::new(),
                node: phoenix_heex::HeexNode::Tag(tag_name),
            };

            self.heex_block_depth = 0;

            *container = self.add_child(
                container,
                NodeValue::HeexBlock(Box::new(nhb)),
                self.first_nonspace + 1,
            )?;

            true
        };
        Ok(result)
    }

    #[cfg(not(feature = "phoenix_heex"))]
    fn handle_heex_block(
        &mut self,
        _container: &mut Node<'a>,
        _line: &str,
    ) -> Result<bool, H::Error> {
        let result = { false };
        Ok(result)
    }

    #[cfg(feature = "phoenix_heex")]
    fn detect_heex_block(&mut self, line: &str) -> Result<Option<usize>, H::Error> {
        let result = 'native: {
            if !self.options.extension.phoenix_heex {
                break 'native None;
            }

            let tag_name_len =
                native_some!('native, scanners::phoenix_opening_tag(&line[self.first_nonspace..]));
            let tag_name = &line[self.first_nonspace + 1..=self.first_nonspace + tag_name_len];

            if let Some(end_pos) =
                scanners::phoenix_closing_tag_end(&line[self.first_nonspace..], tag_name)
            {
                let after_closing = &line[self.first_nonspace + end_pos..];
                if !after_closing.trim().is_empty() {
                    break 'native None;
                }
            }

            if let Some(end_pos) =
                scanners::phoenix_self_closing_tag_end(&line[self.first_nonspace..])
            {
                let after_closing = &line[self.first_nonspace + end_pos..];
                if !after_closing.trim().is_empty() {
                    break 'native None;
                }
            }

            Some(tag_name_len)
        };
        Ok(result)
    }

    #[cfg(feature = "phoenix_heex")]
    fn is_heex_opening_self_closing_tag(
        &mut self,
        line: &str,
        expected_tag_name: &str,
    ) -> Result<bool, H::Error> {
        let result = 'native: {
            let Some(tag_name_len) = scanners::phoenix_opening_tag(&line[self.first_nonspace..])
            else {
                break 'native false;
            };

            let line_tag_name = &line[self.first_nonspace + 1..=self.first_nonspace + tag_name_len];

            if line_tag_name != expected_tag_name {
                break 'native false;
            }

            let bytes = &line.as_bytes()[self.first_nonspace..];
            let mut end = bytes.len();
            while end > 0 && strings::is_line_end_char(bytes[end - 1]) {
                end -= 1;
            }

            end >= 2 && bytes[end - 2] == b'/' && bytes[end - 1] == b'>'
        };
        Ok(result)
    }

    #[cfg(feature = "phoenix_heex")]
    fn is_heex_opening_same_name_tag(
        &mut self,
        line: &str,
        tag_name: &str,
    ) -> Result<bool, H::Error> {
        let result = {
            if let Some(opening_len) = scanners::phoenix_opening_tag(&line[self.first_nonspace..]) {
                let opening_name =
                    &line[self.first_nonspace + 1..=self.first_nonspace + opening_len];
                opening_name == tag_name
                    && !self.is_heex_opening_self_closing_tag(line, tag_name)?
            } else {
                false
            }
        };
        Ok(result)
    }

    #[cfg(feature = "phoenix_heex")]
    fn heex_tag_matches_end(&mut self, line: &str, tag_name: &str) -> Result<bool, H::Error> {
        let result = {
            if self.is_heex_opening_same_name_tag(line, tag_name)? {
                self.heex_block_depth += 1;
            }

            let matched = if let Some(closing_tag_name_len) =
                scanners::phoenix_block_closing_tag(&line[self.first_nonspace..])
            {
                let closing_tag_name =
                    &line[self.first_nonspace + 2..self.first_nonspace + 2 + closing_tag_name_len];
                if closing_tag_name == tag_name {
                    self.heex_block_depth = self.heex_block_depth.saturating_sub(1);
                    true
                } else {
                    false
                }
            } else if self.is_heex_opening_self_closing_tag(line, tag_name)? {
                true
            } else if scanners::phoenix_closing_tag_end(&line[self.first_nonspace..], tag_name)
                .is_some()
            {
                self.heex_block_depth = self.heex_block_depth.saturating_sub(1);
                true
            } else {
                false
            };

            matched && self.heex_block_depth == 0
        };
        Ok(result)
    }

    #[cfg(feature = "phoenix_heex")]
    fn handle_heex_directive_block(
        &mut self,
        container: &mut Node<'a>,
        line: &str,
    ) -> Result<bool, H::Error> {
        let result = 'native: {
            if !self.detect_heex_directive_block(container, line)? {
                break 'native false;
            }

            let bytes = &line.as_bytes()[self.first_nonspace..];
            let node = if bytes.len() >= 4 && bytes[2] == b'!' && bytes[3] == b'-' {
                phoenix_heex::HeexNode::MultilineComment
            } else if bytes.len() >= 3 && bytes[2] == b'#' {
                phoenix_heex::HeexNode::Comment
            } else {
                phoenix_heex::HeexNode::Directive
            };

            let nhb = phoenix_heex::NodeHeexBlock {
                literal: String::new(),
                node,
            };

            *container = self.add_child(
                container,
                NodeValue::HeexBlock(Box::new(nhb)),
                self.first_nonspace + 1,
            )?;

            true
        };
        Ok(result)
    }

    #[cfg(not(feature = "phoenix_heex"))]
    fn handle_heex_directive_block(
        &mut self,
        _container: &mut Node<'a>,
        _line: &str,
    ) -> Result<bool, H::Error> {
        let result = { false };
        Ok(result)
    }

    #[cfg(feature = "phoenix_heex")]
    fn detect_heex_directive_block(
        &mut self,
        container: Node<'a>,
        line: &str,
    ) -> Result<bool, H::Error> {
        let result = 'native: {
            if !self.options.extension.phoenix_heex {
                break 'native false;
            }

            if node_matches!(container, NodeValue::HeexBlock(..)) {
                break 'native false;
            }

            if self.first_nonspace > 0 {
                let before = &line.as_bytes()[..self.first_nonspace];
                if before
                    .iter()
                    .any(|&b| !strings::is_space_or_tab(b) && !strings::is_line_end_char(b))
                {
                    break 'native false;
                }
            }

            if scanners::phoenix_block_directive_start(&line[self.first_nonspace..]).is_none() {
                break 'native false;
            }

            if node_matches!(container, NodeValue::Paragraph) {
                !scanners::phoenix_block_directive_end(&line[self.first_nonspace..])
            } else {
                true
            }
        };
        Ok(result)
    }

    #[cfg(feature = "phoenix_heex")]
    fn handle_heex_expression_block(
        &mut self,
        container: &mut Node<'a>,
        line: &str,
    ) -> Result<bool, H::Error> {
        let result = 'native: {
            if !self.detect_heex_expression_block(container, line)? {
                break 'native false;
            }

            let nhb = phoenix_heex::NodeHeexBlock {
                literal: String::new(),
                node: phoenix_heex::HeexNode::Expression,
            };

            *container = self.add_child(
                container,
                NodeValue::HeexBlock(Box::new(nhb)),
                self.first_nonspace + 1,
            )?;

            true
        };
        Ok(result)
    }

    #[cfg(not(feature = "phoenix_heex"))]
    fn handle_heex_expression_block(
        &mut self,
        _container: &mut Node<'a>,
        _line: &str,
    ) -> Result<bool, H::Error> {
        let result = { false };
        Ok(result)
    }

    #[cfg(feature = "phoenix_heex")]
    fn has_unclosed_heex_directive(&mut self, node: Node<'a>) -> Result<bool, H::Error> {
        let result = 'native: {
            let data = node.data();
            let content = &data.lines.as_ref().unwrap().content;

            if !content.as_bytes().contains(&b'<') {
                break 'native false;
            }

            let bytes = content.as_bytes();
            let len = bytes.len();

            let mut i = 0;
            let mut in_directive = false;
            let mut in_comment = false;

            while i < len {
                if i + 1 < len && bytes[i] == b'<' && bytes[i + 1] == b'%' {
                    in_directive = true;
                    if (i + 2 < len && bytes[i + 2] == b'#')
                        || (i + 3 < len && bytes[i + 2] == b'!' && bytes[i + 3] == b'-')
                    {
                        in_comment = true;
                    }
                    i += 2;
                } else if in_directive && i + 1 < len && bytes[i] == b'%' && bytes[i + 1] == b'>' {
                    in_directive = false;
                    in_comment = false;
                    i += 2;
                } else if in_directive
                    && in_comment
                    && i + 3 < len
                    && bytes[i] == b'-'
                    && bytes[i + 1] == b'-'
                    && bytes[i + 2] == b'%'
                    && bytes[i + 3] == b'>'
                {
                    in_directive = false;
                    in_comment = false;
                    i += 4;
                } else {
                    i += 1;
                }
            }

            in_directive
        };
        Ok(result)
    }

    #[cfg(feature = "phoenix_heex")]
    fn detect_heex_expression_block(
        &mut self,
        container: Node<'a>,
        line: &str,
    ) -> Result<bool, H::Error> {
        let result = 'native: {
            if !self.options.extension.phoenix_heex {
                break 'native false;
            }

            if node_matches!(container, NodeValue::HeexBlock(..)) {
                break 'native false;
            }

            if self.first_nonspace > 0 {
                let before = &line.as_bytes()[..self.first_nonspace];
                if before
                    .iter()
                    .any(|&b| !strings::is_space_or_tab(b) && !strings::is_line_end_char(b))
                {
                    break 'native false;
                }
            }

            if scanners::phoenix_block_expression_start(&line[self.first_nonspace..]).is_none() {
                break 'native false;
            }

            if node_matches!(container, NodeValue::Paragraph) {
                !scanners::phoenix_block_expression_end(&line[self.first_nonspace..])
            } else {
                true
            }
        };
        Ok(result)
    }

    fn handle_setext_heading(
        &mut self,
        container: &mut Node<'a>,
        line: &str,
    ) -> Result<bool, H::Error> {
        let result = 'native: {
            let Some(sc) = self.detect_setext_heading(container, line)? else {
                break 'native false;
            };

            let has_content = {
                let mut ast = container.data_mut();
                self.resolve_reference_link_definitions(ast.lines_mut())?
            };
            if has_content {
                container.data_mut().value = NodeValue::Heading(NodeHeading {
                    level: match sc {
                        scanners::SetextChar::Equals => 1,
                        scanners::SetextChar::Hyphen => 2,
                    },
                    setext: true,
                    closed: false,
                });
                let whitespace_start = self.offset;
                let adv = line.len() - strings::newlines_of(line) - self.offset;
                self.advance_offset(line, adv, false)?;
                self.block_whitespace(container, whitespace_start, self.first_nonspace)?;
                if let Some(node) = container.data().native_id {
                    self.ctx.fact(AcceptedFact::Retyped {
                        node,
                        kind: NativeKind::Heading,
                    })?;
                }
                self.syntax(
                    container,
                    SyntaxRole::SetextUnderline,
                    self.first_nonspace,
                    self.offset,
                )?;
            }

            true
        };
        Ok(result)
    }

    fn detect_setext_heading(
        &mut self,
        container: Node<'a>,
        line: &str,
    ) -> Result<Option<scanners::SetextChar>, H::Error> {
        let result = {
            if node_matches!(container, NodeValue::Paragraph) && !self.options.parse.ignore_setext {
                scanners::setext_heading_line_with_host(&line[self.first_nonspace..], self.ctx)?
            } else {
                None
            }
        };
        Ok(result)
    }

    fn handle_thematic_break(
        &mut self,
        container: &mut Node<'a>,
        line: &str,
        all_matched: bool,
    ) -> Result<bool, H::Error> {
        let result = 'native: {
            let Some(_matched) = self.detect_thematic_break(container, line, all_matched)? else {
                break 'native false;
            };

            *container =
                self.add_child(container, NodeValue::ThematicBreak, self.first_nonspace + 1)?;

            let whitespace_start = self.offset;
            let adv = line.len() - strings::newlines_of(line) - self.offset;
            container.data_mut().sourcepos.end = (self.line_number, adv).into();
            self.advance_offset(line, adv, false)?;
            self.block_whitespace(container, whitespace_start, self.first_nonspace)?;
            self.syntax(
                container,
                SyntaxRole::ThematicBreak,
                self.first_nonspace,
                self.offset,
            )?;

            true
        };
        Ok(result)
    }

    fn handle_table_of_contents(
        &mut self,
        container: &mut Node<'a>,
        line: &str,
    ) -> Result<bool, H::Error> {
        if !self.ctx.capture
            || !self.options.extension.native_table_of_contents
            || node_matches!(
                container,
                NodeValue::Table(..) | NodeValue::TableRow(..) | NodeValue::TableCell
            )
        {
            return Ok(false);
        }
        let start = self.first_nonspace;
        let rest = &line.as_bytes()[start..];
        if rest.first() != Some(&b'[') {
            return Ok(false);
        }
        let mut marker_len = None;
        for spelling in [b"[TOC]".as_slice(), b"[[_TOC_]]"] {
            let Some(candidate) = rest.get(..spelling.len()) else {
                continue;
            };
            self.ctx.scan(spelling.len())?;
            if candidate.eq_ignore_ascii_case(spelling) {
                marker_len = Some(spelling.len());
                break;
            }
        }
        let Some(marker_len) = marker_len else {
            return Ok(false);
        };
        let end = line.len() - strings::newlines_of(line);
        let marker_end = start + marker_len;
        if marker_end > end {
            return Ok(false);
        }
        for &byte in &line.as_bytes()[marker_end..end] {
            self.ctx.scan_step()?;
            if !matches!(byte, b' ' | b'\t') {
                return Ok(false);
            }
        }
        let offset = self.offset;
        *container = self.add_child(container, NodeValue::TableOfContents, start + 1)?;
        container.data_mut().sourcepos.end = (self.line_number, end).into();
        self.advance_offset(line, end - offset, false)?;
        self.block_whitespace(*container, offset, start)?;
        self.syntax(*container, SyntaxRole::TableOfContents, start, marker_end)?;
        self.block_whitespace(*container, marker_end, end)?;
        Ok(true)
    }

    fn detect_thematic_break(
        &mut self,
        container: Node<'a>,
        line: &str,
        all_matched: bool,
    ) -> Result<Option<usize>, H::Error> {
        let result = {
            if !matches!(
                (&container.data().value, all_matched),
                (&NodeValue::Paragraph, false)
            ) && self.thematic_break_kill_pos <= self.first_nonspace
            {
                let (offset, found) = self.scan_thematic_break_inner(line)?;
                if !found {
                    self.thematic_break_kill_pos = offset;
                    None
                } else {
                    Some(offset)
                }
            } else {
                None
            }
        };
        Ok(result)
    }

    fn scan_thematic_break_inner(&mut self, line: &str) -> Result<(usize, bool), H::Error> {
        let result = 'native: {
            let mut i = self.first_nonspace;

            if i >= line.len() {
                break 'native (i, false);
            }

            let bytes = line.as_bytes();
            let b = bytes[i];
            if b != b'*' && b != b'_' && b != b'-' {
                break 'native (i, false);
            }

            let mut count = 1;
            let mut nextb;
            loop {
                self.ctx.scan_step()?;
                i += 1;
                if i >= line.len() {
                    nextb = 255;
                    break;
                }
                nextb = bytes[i];

                if nextb == b {
                    count += 1;
                } else if nextb != b' ' && nextb != b'\t' {
                    break;
                }
            }

            if count >= 3 && (nextb == 255 || nextb == b'\r' || nextb == b'\n') {
                ((i - self.first_nonspace) + 1, true)
            } else {
                (i, false)
            }
        };
        Ok(result)
    }

    fn handle_footnote(
        &mut self,
        container: &mut Node<'a>,
        line: &str,
        depth: usize,
    ) -> Result<bool, H::Error> {
        let result = 'native: {
            let Some(matched) = self.detect_footnote(line, depth)? else {
                break 'native false;
            };
            if self.ctx.capture {
                // add_child closes incompatible leaf/list containers before
                // inserting this definition. Check that eventual parent, not
                // whichever previous block is still open on this line.
                let value = NodeValue::FootnoteDefinition(NodeFootnoteDefinition {
                    name: String::new(),
                    total_references: 0,
                });
                let mut parent = *container;
                while !parent.can_contain_type(&value) {
                    self.ctx.event()?;
                    parent = parent
                        .parent()
                        .ok_or_else(|| self.ctx.host.reject(NativeViolation::InvalidProvenance))?;
                }
                if !parent.same_node(self.root) {
                    break 'native false;
                }
            }
            let start = self.first_nonspace;
            let end = start + matched;
            let label = line[start + 2..end].split(']').next().unwrap();
            let offset_before = self.offset;
            let offset = end - offset_before;
            self.advance_offset(line, offset, false)?;
            *container = self.add_child(
                container,
                NodeValue::FootnoteDefinition(NodeFootnoteDefinition {
                    name: if self.ctx.capture {
                        String::new()
                    } else {
                        label.to_owned()
                    },
                    total_references: 0,
                }),
                start + 1,
            )?;
            if let Some(id) = container.data().native_id {
                self.refmap.insert_footnote(label, id, self.ctx)?;
                let marker_end = start + 2 + label.len() + 2;
                self.block_whitespace(*container, offset_before, start)?;
                self.syntax(*container, SyntaxRole::FootnoteMarker, start, marker_end)?;
                self.block_whitespace(*container, marker_end, end)?;
                if end < line.len() {
                    self.syntax(*container, SyntaxRole::FootnoteContent, end, line.len())?;
                }
            }

            true
        };
        Ok(result)
    }

    fn detect_footnote(&mut self, line: &str, depth: usize) -> Result<Option<usize>, H::Error> {
        if self.ctx.capture && self.options.extension.footnotes {
            let Some(rest) = line[self.first_nonspace..].strip_prefix("[^") else {
                return Ok(None);
            };
            let Some(end) = self.ctx.find(rest.as_bytes(), |bytes| {
                bytes.iter().copied().position(native::footnote_label_break)
            })?
            else {
                return Ok(None);
            };
            if end == 0 || !rest[end..].starts_with("]:") {
                return Ok(None);
            }
            let tail = &rest.as_bytes()[end + 2..];
            let space = self
                .ctx
                .find(tail, |bytes| {
                    bytes.iter().position(|&ch| !matches!(ch, b' ' | b'\t'))
                })?
                .unwrap_or(tail.len());
            return Ok(Some(4 + end + space));
        }
        let result = {
            if self.options.extension.footnotes && (self.ctx.capture || depth < MAX_LIST_DEPTH) {
                scanners::footnote_definition(&line[self.first_nonspace..])
            } else {
                None
            }
        };
        Ok(result)
    }

    fn handle_description_list(
        &mut self,
        container: &mut Node<'a>,
        line: &str,
    ) -> Result<bool, H::Error> {
        let result = 'native: {
            let start = self.first_nonspace;
            let preceding = self.offset;
            let Some(matched) = self.detect_description_list(container, line)? else {
                break 'native false;
            };

            let offset = self.first_nonspace + matched - self.offset;
            self.advance_offset(line, offset, false)?;
            if byte_matches(line.as_bytes(), self.offset, strings::is_space_or_tab) {
                self.advance_offset(line, 1, true)?;
            }

            let item = container
                .parent()
                .ok_or_else(|| self.ctx.host.reject(NativeViolation::InvalidProvenance))?;
            self.block_whitespace(item, preceding, start)?;
            self.syntax(item, SyntaxRole::DescriptionMarker, start, start + 1)?;
            self.block_whitespace(item, start + 1, self.offset)?;
            true
        };
        Ok(result)
    }

    fn detect_description_list(
        &mut self,
        container: &mut Node<'a>,
        line: &str,
    ) -> Result<Option<usize>, H::Error> {
        if !self.options.extension.description_lists {
            return Ok(None);
        }
        let raw = &line[self.first_nonspace..];
        let matched = if self.ctx.capture {
            let bytes = raw.as_bytes();
            if !matches!(bytes.first(), Some(b':' | b'~')) {
                return Ok(None);
            }
            self.ctx.scan_step()?;
            let mut end = 1;
            while matches!(bytes.get(end), Some(b' ' | b'\t')) {
                self.ctx.scan_step()?;
                end += 1;
            }
            (end > 1).then_some(end)
        } else {
            scanners::description_item_start(raw)
        };
        if let Some(matched) = matched
            && self.parse_desc_list_details(container, matched)?
        {
            return Ok(Some(matched));
        }
        Ok(None)
    }

    fn parse_desc_list_details(
        &mut self,
        container: &mut Node<'a>,
        matched: usize,
    ) -> Result<bool, H::Error> {
        let result = 'native: {
            let mut tight = false;
            let last_child = match container.last_child() {
                Some(lc) => lc,
                None => {
                    // Happens when the detail line is directly after the term,
                    // without a blank line between.
                    if !node_matches!(container, NodeValue::Paragraph) {
                        // If the container is not a paragraph, then this can't
                        // be a description list item.
                        break 'native false;
                    }

                    let Some(parent) = container.parent() else {
                        break 'native false;
                    };

                    tight = true;
                    *container = parent;
                    container.last_child().unwrap()
                }
            };

            if node_matches!(last_child, NodeValue::Paragraph) {
                if last_child.data().native_callout_title {
                    // The authored title is bound to its callout. It cannot
                    // become a description term on a subsequent body line.
                    break 'native false;
                }
                // We have found the details after the paragraph for the term.
                //
                // This paragraph is moved as a child of a new DescriptionTerm node.
                //
                // If the node before the paragraph is a description list, the item
                // is added to it. If not, create a new list.

                last_child.detach();
                let last_child_sourcepos = last_child.data().sourcepos;

                let list = match container.last_child() {
                    Some(lc) if node_matches!(lc, NodeValue::DescriptionList) => {
                        reopen_ast_nodes(lc, self.ctx)?;
                        lc
                    }
                    _ => {
                        let list = self.add_child(
                            container,
                            NodeValue::DescriptionList,
                            self.first_nonspace + 1,
                        )?;
                        list.data_mut().sourcepos.start = last_child_sourcepos.start;
                        list
                    }
                };

                let metadata = NodeDescriptionItem {
                    marker_offset: self.indent,
                    padding: matched,
                    tight,
                };

                let item = self.add_child(
                    list,
                    NodeValue::DescriptionItem(metadata),
                    self.first_nonspace + 1,
                )?;
                item.data_mut().sourcepos.start = last_child_sourcepos.start;
                let term =
                    self.add_child(item, NodeValue::DescriptionTerm, self.first_nonspace + 1)?;
                let details =
                    self.add_child(item, NodeValue::DescriptionDetails, self.first_nonspace + 1)?;

                term.data_mut().sourcepos.start = last_child_sourcepos.start;

                term.append(last_child);

                *container = details;

                true
            } else if node_matches!(last_child, NodeValue::DescriptionItem(..)) {
                let parent = last_child.parent().unwrap();
                let tight = match last_child.data().value {
                    NodeValue::DescriptionItem(ref ndi) => ndi.tight,
                    _ => false,
                };

                let metadata = NodeDescriptionItem {
                    marker_offset: self.indent,
                    padding: matched,
                    tight,
                };

                let item = self.add_child(
                    parent,
                    NodeValue::DescriptionItem(metadata),
                    self.first_nonspace + 1,
                )?;

                let details =
                    self.add_child(item, NodeValue::DescriptionDetails, self.first_nonspace + 1)?;

                *container = details;

                true
            } else {
                false
            }
        };
        Ok(result)
    }

    fn handle_list(
        &mut self,
        container: &mut Node<'a>,
        line: &str,
        indented: bool,
        depth: usize,
    ) -> Result<bool, H::Error> {
        let result = 'native: {
            let Some((matched, mut nl)) = self.detect_list(container, line, indented, depth)?
            else {
                break 'native false;
            };

            let whitespace_start = self.offset;
            let offset = self.first_nonspace + matched - self.offset;
            self.advance_offset(line, offset, false)?;
            let (save_partially_consumed_tab, save_offset, save_column) =
                (self.partially_consumed_tab, self.offset, self.column);

            let bytes = line.as_bytes();
            while self.column - save_column <= 5
                && byte_matches(bytes, self.offset, strings::is_space_or_tab)
            {
                self.advance_offset(line, 1, true)?;
            }

            let i = self.column - save_column;
            if !(1..5).contains(&i) || byte_matches(bytes, self.offset, strings::is_line_end_char) {
                nl.padding = matched + 1;
                self.offset = save_offset;
                self.column = save_column;
                self.partially_consumed_tab = save_partially_consumed_tab;
                if i > 0 {
                    self.advance_offset(line, 1, true)?;
                }
            } else {
                nl.padding = matched + i;
            }

            nl.marker_offset = self.indent;

            if match container.data().value {
                NodeValue::List(ref mnl) => !lists_match(&nl, mnl),
                _ => true,
            } {
                self.saw_list = true;
                *container =
                    self.add_child(container, NodeValue::List(nl), self.first_nonspace + 1)?;
            }

            self.syntax(
                container,
                SyntaxRole::ListPrefix,
                self.first_nonspace,
                self.offset,
            )?;
            *container = self.add_child(container, NodeValue::Item(nl), self.first_nonspace + 1)?;
            self.block_whitespace(container, whitespace_start, self.first_nonspace)?;
            self.syntax(
                container,
                SyntaxRole::ListPrefix,
                self.first_nonspace,
                self.offset,
            )?;

            true
        };
        Ok(result)
    }

    fn detect_list(
        &mut self,
        container: Node<'a>,
        line: &str,
        indented: bool,
        depth: usize,
    ) -> Result<Option<(usize, NodeList)>, H::Error> {
        let result = {
            if (!indented || node_matches!(container, NodeValue::List(..)))
                && self.indent < 4
                && (self.ctx.capture || depth < MAX_LIST_DEPTH)
            {
                parse_list_marker_with_host(
                    line,
                    self.first_nonspace,
                    node_matches!(container, NodeValue::Paragraph),
                    self.ctx,
                )?
            } else {
                None
            }
        };
        Ok(result)
    }

    fn handle_code_block(
        &mut self,
        container: &mut Node<'a>,
        line: &str,
        indented: bool,
        maybe_lazy: bool,
    ) -> Result<bool, H::Error> {
        let result = 'native: {
            if !self.detect_code_block(indented, maybe_lazy)? {
                break 'native false;
            }

            let code_indent_start = self.offset;
            self.advance_offset(line, CODE_INDENT, true)?;
            let ncb = NodeCodeBlock {
                fenced: false,
                fence_char: 0,
                fence_length: 0,
                fence_offset: 0,
                info: self.ctx.empty_value(),
                literal: self.ctx.empty_value(),
                closed: true,
            };
            let ncb = self.ctx.boxed(ncb)?;
            *container = self.add_child(container, NodeValue::CodeBlock(ncb), self.offset + 1)?;
            self.syntax(
                container,
                SyntaxRole::CodeIndent,
                code_indent_start,
                self.offset,
            )?;

            true
        };
        Ok(result)
    }

    fn detect_code_block(&mut self, indented: bool, maybe_lazy: bool) -> Result<bool, H::Error> {
        let result = { indented && !maybe_lazy && !self.blank };
        Ok(result)
    }

    fn handle_table(
        &mut self,
        container: &mut Node<'a>,
        line: &str,
        indented: bool,
    ) -> Result<TableHandling, H::Error> {
        let result = 'native: {
            let Some(table::TableBlockResult {
                container: new_container,
                replace,
                mark_visited,
                split,
            }) = self.detect_table(container, line, indented)?
            else {
                break 'native TableHandling::NotMatched;
            };

            if split {
                if replace
                    || mark_visited
                    || container.same_node(new_container)
                    || !node_matches!(container, NodeValue::Paragraph)
                    || !node_matches!(new_container, NodeValue::Table(..))
                {
                    return Err(self.ctx.host.reject(NativeViolation::InvalidProvenance));
                }
                *container = new_container;
                self.current = new_container;
                break 'native TableHandling::Split;
            }

            if replace {
                container.insert_after(new_container);
                container.detach();
                *container = new_container;
            } else {
                *container = new_container;
            }
            if mark_visited {
                container.data_mut().table_visited = true;
            }

            TableHandling::Handled
        };
        Ok(result)
    }

    fn detect_table(
        &mut self,
        container: Node<'a>,
        line: &str,
        indented: bool,
    ) -> Result<Option<table::TableBlockResult<'a>>, H::Error> {
        let result = {
            if !indented && self.options.extension.table {
                table::try_opening_block(self, container, line)?
            } else {
                None
            }
        };
        Ok(result)
    }

    //////////
    // Core //
    //////////

    fn advance_offset(
        &mut self,
        line: &str,
        mut count: usize,
        columns: bool,
    ) -> Result<(), H::Error> {
        {
            let bytes = line.as_bytes();
            while count > 0 {
                self.ctx.scan_step()?;
                match bytes[self.offset] {
                    b'\t' => {
                        let chars_to_tab = TAB_STOP - (self.column % TAB_STOP);
                        if columns {
                            self.partially_consumed_tab = chars_to_tab > count;
                            let chars_to_advance = min(count, chars_to_tab);
                            self.column += chars_to_advance;
                            if !self.partially_consumed_tab {
                                self.offset += 1;
                            };
                            count -= chars_to_advance;
                        } else {
                            self.partially_consumed_tab = false;
                            self.column += chars_to_tab;
                            self.offset += 1;
                            count -= 1;
                        }
                    }
                    _ => {
                        self.partially_consumed_tab = false;
                        self.offset += 1;
                        self.column += 1;
                        count -= 1;
                    }
                }
            }
        };
        Ok(())
    }

    fn syntax(
        &mut self,
        node: Node<'a>,
        role: SyntaxRole,
        start: usize,
        end: usize,
    ) -> Result<(), H::Error> {
        if let Some(node) = node.data().native_id {
            self.ctx.fact(AcceptedFact::Syntax {
                node,
                role,
                span: PSpan {
                    start: self.ctx.line_start + start,
                    end: self.ctx.line_start + end,
                },
            })?;
        }
        Ok(())
    }

    fn block_whitespace(
        &mut self,
        node: Node<'a>,
        start: usize,
        end: usize,
    ) -> Result<(), H::Error> {
        if start != end {
            self.syntax(node, SyntaxRole::BlockWhitespace, start, end)?;
        }
        Ok(())
    }

    fn add_child(
        &mut self,
        mut parent: Node<'a>,
        value: NodeValue,
        start_column: usize,
    ) -> Result<Node<'a>, H::Error> {
        let result = {
            while !parent.can_contain_type(&value) {
                self.ctx.event()?;
                parent = self.finalize(parent)?.unwrap();
            }

            assert!(start_column > 0);

            if self.ctx.capture {
                let mut depth = 1usize;
                for _ in parent.ancestors() {
                    self.ctx.event()?;
                    depth += 1;
                    self.ctx.host.admit(NativeAdmission::Depth(depth as u64))?;
                }
            }
            let kind = native::kind(&value);
            let mut child = Ast::new(value, (self.line_number, start_column).into());
            if self.ctx.capture {
                let kind = kind.ok_or_else(|| {
                    self.ctx
                        .host
                        .reject(NativeViolation::UnsupportedConfiguration)
                })?;
                let id = self.ctx.node(parent.data().native_id, kind)?;
                child.native_id = Some(id);
                self.ctx.fact(AcceptedFact::BlockStart {
                    node: id,
                    at: self.ctx.line_start + start_column - 1,
                })?;
                self.ctx.fact(AcceptedFact::BlockConsume {
                    node: id,
                    span: PSpan {
                        start: self.ctx.line_start,
                        end: self.ctx.line_start + self.curline_len,
                    },
                })?;
            }
            if child.value.has_block_lines() {
                let mut lines = BlockLines {
                    opened_line: self.line_number,
                    ..Default::default()
                };
                if let Some(id) = child.native_id {
                    lines.buffer = Some(self.ctx.buffer(id)?);
                }
                child.lines = Some(self.ctx.boxed(lines)?);
            }
            let node: Node<'a> = self.arena.try_alloc(child.into(), self.ctx)?;
            parent.append(node);
            node
        };
        Ok(result)
    }

    fn add_text_to_container(
        &mut self,
        mut container: Node<'a>,
        last_matched_container: Node<'a>,
        line: &str,
    ) -> Result<(), H::Error> {
        {
            self.find_first_nonspace(line)?;

            if self.blank
                && let Some(last_child) = container.last_child()
            {
                last_child.data_mut().last_line_blank = true;
            }

            container.data_mut().last_line_blank = self.blank
                && match container.data().value {
                    NodeValue::BlockQuote
                    | NodeValue::Heading(..)
                    | NodeValue::ThematicBreak
                    | NodeValue::TableOfContents
                    | NodeValue::Subtext => false,
                    NodeValue::CodeBlock(ref ncb) => !ncb.fenced,
                    NodeValue::Item(..) => {
                        container.first_child().is_some()
                            || container.data().sourcepos.start.line != self.line_number
                    }
                    NodeValue::MultilineBlockQuote(..) => false,
                    NodeValue::Alert(..) => false,
                    _ => true,
                };

            let mut tmp = container;
            while let Some(parent) = tmp.parent() {
                self.ctx.event()?;
                parent.data_mut().last_line_blank = false;
                tmp = parent;
            }

            if !self.current.same_node(last_matched_container)
                && container.same_node(last_matched_container)
                && !self.blank
                && (!self.options.extension.greentext
                    || !node_matches!(container, NodeValue::BlockQuote | NodeValue::Document))
                && node_matches!(self.current, NodeValue::Paragraph)
            {
                if self.ctx.capture && self.options.extension.footnotes && self.offset < line.len()
                {
                    // This line was accepted through lazy continuation after
                    // at least one container prefix failed to match.
                    let mut ancestor = self.current;
                    while !ancestor.same_node(last_matched_container) {
                        self.ctx.event()?;
                        if node_matches!(ancestor, NodeValue::FootnoteDefinition(..)) {
                            self.syntax(
                                ancestor,
                                SyntaxRole::FootnoteContent,
                                self.offset,
                                line.len(),
                            )?;
                        }
                        ancestor = ancestor.parent().ok_or_else(|| {
                            self.ctx.host.reject(NativeViolation::InvalidProvenance)
                        })?;
                    }
                }
                self.add_line(self.current, line)?;
            } else {
                while !self.current.same_node(last_matched_container) {
                    self.ctx.event()?;
                    self.current = self.finalize(self.current)?.unwrap();
                }

                let add_text_result = match container.data().value {
                    NodeValue::CodeBlock(..) => AddTextResult::LiteralText,
                    NodeValue::HtmlBlock(ref nhb) => AddTextResult::HtmlBlock(nhb.block_type),
                    #[cfg(feature = "phoenix_heex")]
                    NodeValue::HeexBlock(ref nhb) => AddTextResult::HeexBlock(nhb.clone()),
                    _ => AddTextResult::Otherwise,
                };

                match add_text_result {
                    AddTextResult::LiteralText => {
                        self.add_line(container, line)?;
                    }
                    AddTextResult::HtmlBlock(block_type) => {
                        self.add_line(container, line)?;

                        let matches_end_condition = match block_type {
                            1 => scanners::html_block_end_1_with_host(
                                &line[self.first_nonspace..],
                                self.ctx,
                            )?,
                            2 => scanners::html_block_end_2_with_host(
                                &line[self.first_nonspace..],
                                self.ctx,
                            )?,
                            3 => scanners::html_block_end_3_with_host(
                                &line[self.first_nonspace..],
                                self.ctx,
                            )?,
                            4 => scanners::html_block_end_4_with_host(
                                &line[self.first_nonspace..],
                                self.ctx,
                            )?,
                            5 => scanners::html_block_end_5_with_host(
                                &line[self.first_nonspace..],
                                self.ctx,
                            )?,
                            _ => false,
                        };

                        if matches_end_condition {
                            container = self.finalize(container)?.unwrap();
                        }
                    }
                    #[cfg(feature = "phoenix_heex")]
                    AddTextResult::HeexBlock(nhb) => {
                        self.add_line(container, line)?;

                        let matches_end = match nhb.node {
                            phoenix_heex::HeexNode::Tag(ref tag_name) => {
                                self.heex_tag_matches_end(line, tag_name)?
                            }
                            phoenix_heex::HeexNode::Directive => {
                                scanners::phoenix_block_directive_end(&line[self.first_nonspace..])
                            }
                            phoenix_heex::HeexNode::Comment => {
                                scanners::phoenix_block_comment_end(&line[self.first_nonspace..])
                            }
                            phoenix_heex::HeexNode::MultilineComment => {
                                scanners::phoenix_block_multiline_comment_end(
                                    &line[self.first_nonspace..],
                                )
                            }
                            phoenix_heex::HeexNode::Expression => {
                                scanners::phoenix_block_expression_end(&line[self.first_nonspace..])
                            }
                        };

                        if matches_end {
                            container = self.finalize(container)?.unwrap();
                        }
                    }
                    _ => {
                        if self.blank {
                            // do nothing
                        } else if container.data().value.accepts_lines() {
                            let mut line = line;
                            {
                                let mut data = container.data_mut();
                                let native_id = data.native_id;
                                if let NodeValue::Heading(ref mut nh) = data.value
                                    && !nh.setext
                                {
                                    let (new_line, closed) =
                                        strings::chop_trailing_hashes_with_host(line, self.ctx)?;
                                    if let Some(node) = native_id
                                        && new_line.len() != line.len()
                                    {
                                        self.ctx.fact(AcceptedFact::Syntax {
                                            node,
                                            role: SyntaxRole::HeadingSuffix,
                                            span: PSpan {
                                                start: self.ctx.line_start + new_line.len(),
                                                end: self.ctx.line_start + line.len(),
                                            },
                                        })?;
                                    }
                                    line = new_line;
                                    nh.closed = closed;
                                }
                            }

                            #[cfg(feature = "phoenix_heex")]
                            let preserve_heex_whitespace = if self.options.extension.phoenix_heex {
                                node_matches!(container, NodeValue::Paragraph)
                                    && self.has_unclosed_heex_directive(container)?
                            } else {
                                false
                            };
                            #[cfg(not(feature = "phoenix_heex"))]
                            let preserve_heex_whitespace = false;

                            let count = self.first_nonspace - self.offset;

                            // In some cases the `chop_trailing_hashes` above
                            // can leave the line shorter than the recorded
                            // `first_nonspace` This happens with ATX headers
                            // containing no header text, multiple spaces and
                            // trailing hashes, e.g
                            //
                            // ###     ###
                            //
                            // In this case `first_nonspace` indexes into the
                            // second set of hashes, while `chop_trailing_hashtags`
                            // truncates `line` to just `###` (the first three
                            // hashes). In this case there's no text to add, and no
                            // further processing to be done.
                            let have_line_text = self.first_nonspace <= line.len();

                            if have_line_text {
                                let paragraph_continuation =
                                    node_matches!(container, NodeValue::Paragraph)
                                        && container.data().lines.as_ref().unwrap().opened_line
                                            != self.line_number;
                                if !preserve_heex_whitespace && !paragraph_continuation {
                                    let whitespace_start = self.offset;
                                    self.advance_offset(line, count, false)?;
                                    self.block_whitespace(
                                        container,
                                        whitespace_start,
                                        self.offset,
                                    )?;
                                }
                                self.add_line(container, line)?;
                            }
                        } else {
                            container = self.add_child(
                                container,
                                NodeValue::Paragraph,
                                self.first_nonspace + 1,
                            )?;
                            let count = self.first_nonspace - self.offset;
                            let whitespace_start = self.offset;
                            self.advance_offset(line, count, false)?;
                            self.block_whitespace(container, whitespace_start, self.offset)?;
                            self.add_line(container, line)?;
                        }
                    }
                }

                self.current = container;
            }
        };
        Ok(())
    }

    fn add_line(&mut self, node: Node<'a>, line: &str) -> Result<(), H::Error> {
        {
            let mut ast = node.data_mut();
            assert!(ast.open);
            // Keep the original end column before expanding tabs or trimming trailing blanks.
            if !self.blank && matches!(ast.value, NodeValue::CodeBlock(ref ncb) if !ncb.fenced) {
                ast.sourcepos.end = (self.line_number, self.curline_end_col).into();
            }
            let lines = ast.lines_mut();
            if lines.content.capacity() == 0 {
                mem::swap(&mut lines.content, &mut self.working_content);
            }
            // Definitions can be consumed by a failed setext candidate before
            // another line is appended. Native buffer coordinates keep the
            // consumed prefix even though content now stores only the suffix.
            if self.partially_consumed_tab {
                let tab = self.offset;
                self.offset += 1;
                let chars_to_tab = TAB_STOP - (self.column % TAB_STOP);
                if let Some(buffer) = lines.buffer {
                    self.ctx.fact(AcceptedFact::Append {
                        span: RSpan {
                            buffer,
                            start: lines.base + lines.content.len(),
                            end: lines.base + lines.content.len() + chars_to_tab,
                        },
                        source: PSpan {
                            start: self.ctx.line_start + tab,
                            end: self.ctx.line_start + tab + 1,
                        },
                        kind: AppendKind::PartialTab {
                            spaces: chars_to_tab as u8,
                            column: self.column,
                        },
                    })?;
                }
                self.ctx.grow_string(&mut lines.content, chars_to_tab)?;
                for _ in 0..chars_to_tab {
                    lines.content.push(' ');
                }
            }
            if self.offset < line.len() {
                // Since whitespace is stripped off the beginning of lines, we need
                // to keep track of how much was stripped off. This allows us to
                // properly calculate inline sourcepos during inline processing.
                self.ctx.small_push(&mut lines.offsets, self.offset)?;
                if let Some(buffer) = lines.buffer {
                    self.ctx.fact(AcceptedFact::Append {
                        span: RSpan {
                            buffer,
                            start: lines.base + lines.content.len(),
                            end: lines.base + lines.content.len() + line.len() - self.offset,
                        },
                        source: PSpan {
                            start: self.ctx.line_start + self.offset,
                            end: self.ctx.line_start + line.len(),
                        },
                        kind: AppendKind::Copy,
                    })?;
                }
                self.ctx.append(&mut lines.content, &line[self.offset..])?;
            }
            if !line.is_empty() {
                self.ctx.scan_step()?;
                if self.ctx.line_start + line.len() == self.ctx.input_len
                    && !strings::is_line_end_char(*line.as_bytes().last().unwrap())
                {
                    // This is the actual accepted source line, before any code-info/body cuts.
                    lines.eof_anchor = Some(self.ctx.input_len);
                }
            }
        };
        Ok(())
    }

    fn finalize_document(&mut self) -> Result<(), H::Error> {
        {
            while !self.current.same_node(self.root) {
                self.ctx.event()?;
                self.current = self.finalize(self.current)?.unwrap();
            }

            self.finalize(self.root)?;

            self.refmap.max_ref_size = self.total_size.min(100000);

            self.process_inlines()?;

            if self.ctx.capture && self.options.extension.inline_footnotes {
                self.footnote_defs.append_native(self.root, self.ctx)?;
            }

            if self.options.extension.footnotes && !self.ctx.capture {
                // Append auto-generated inline footnote definitions
                if self.options.extension.inline_footnotes {
                    self.root.extend(self.footnote_defs.take());
                }

                self.process_footnotes()?;
            }

            self.propagate_list_sourcepos(self.root)?;
        };
        Ok(())
    }

    fn propagate_list_sourcepos(&mut self, root: Node<'a>) -> Result<(), H::Error> {
        if self.ctx.capture {
            return Ok(());
        }
        'native: {
            if !self.saw_list {
                break 'native;
            }
            for node in root.descendants() {
                if !matches!(node.data().value, NodeValue::List(..)) {
                    continue;
                }
                let mut max_end = node.data().sourcepos.end;
                for d in node.descendants() {
                    let de = d.data().sourcepos.end;
                    if de.column == 0 {
                        continue;
                    }
                    if de.line > max_end.line
                        || (de.line == max_end.line && de.column > max_end.column)
                    {
                        max_end = de;
                    }
                }
                if max_end.column != 0 {
                    node.data_mut().sourcepos.end = max_end;
                }
            }
        };
        Ok(())
    }

    fn finalize(&mut self, node: Node<'a>) -> Result<Option<Node<'a>>, H::Error> {
        let result = { self.finalize_borrowed(node, &mut node.data_mut())? };
        Ok(result)
    }

    fn resolve_reference_link_definitions(
        &mut self,
        lines: &mut BlockLines,
    ) -> Result<bool, H::Error> {
        let mut pos = 0;
        while pos < lines.content.len() && lines.content.as_bytes()[pos] == b'[' {
            self.ctx.event()?;
            let Some((offset, reference)) =
                self.parse_reference_inline(&lines.content[pos..], lines.buffer, lines.base + pos)?
            else {
                break;
            };
            pos += offset;
            if let Some((label, reference)) = reference {
                let native = reference.native;
                let accepted = self.refmap.map.insert(label, reference, self.ctx)?;
                if let Some(native) = native {
                    self.ctx.fact(AcceptedFact::Winner {
                        definition: native.definition,
                        accepted,
                    })?;
                }
            }
        }
        if pos != 0 {
            let definition_end = lines
                .base
                .checked_add(pos)
                .ok_or_else(|| self.ctx.host.reject(NativeViolation::ArithmeticOverflow))?;
            if let Some(buffer) = lines.buffer {
                self.ctx.fact(AcceptedFact::Cut {
                    span: RSpan {
                        buffer,
                        start: lines.base,
                        end: definition_end,
                    },
                    reason: SyntaxRole::ReferenceDefinition,
                })?;
            }
            // The accepted definition ends before this first residual line.
            // Its indentation is prose whitespace, not part of the definition.
            let mut end = pos;
            while end < lines.content.len() {
                self.ctx.scan_step()?;
                if !matches!(lines.content.as_bytes()[end], b' ' | b'\t') {
                    break;
                }
                end += 1;
            }
            let whitespace = end - pos;
            let residual_base = definition_end
                .checked_add(whitespace)
                .ok_or_else(|| self.ctx.host.reject(NativeViolation::ArithmeticOverflow))?;
            if whitespace != 0
                && let Some(buffer) = lines.buffer
            {
                self.ctx.fact(AcceptedFact::Cut {
                    span: RSpan {
                        buffer,
                        start: definition_end,
                        end: residual_base,
                    },
                    reason: SyntaxRole::BlockWhitespace,
                })?;
            }
            let (consumed_lines, _) =
                strings::count_newlines_with_host(&lines.content[..pos], self.ctx)?;
            let retained = lines
                .offsets
                .len()
                .checked_sub(consumed_lines)
                .ok_or_else(|| self.ctx.host.reject(NativeViolation::InvalidProvenance))?;
            for index in 0..retained {
                self.ctx.event()?;
                self.ctx
                    .host
                    .work(native::WorkDelta::Copy(std::mem::size_of::<usize>() as u64))?;
                lines.offsets[index] = lines.offsets[index + consumed_lines];
            }
            lines.offsets.truncate(retained);
            if let Some(first) = lines.offsets.first_mut() {
                *first = first
                    .checked_add(whitespace)
                    .ok_or_else(|| self.ctx.host.reject(NativeViolation::ArithmeticOverflow))?;
            }
            lines.opened_line = lines
                .opened_line
                .checked_add(consumed_lines)
                .ok_or_else(|| self.ctx.host.reject(NativeViolation::ArithmeticOverflow))?;
            let remaining = self.ctx.string(&lines.content[end..])?;
            let previous = mem::replace(&mut lines.content, remaining);
            self.ctx.discard_string(previous)?;
            lines.base = residual_base;
        }
        Ok(!strings::is_blank_with_host(&lines.content, self.ctx)?)
    }

    fn finalize_borrowed(
        &mut self,
        node: Node<'a>,
        ast: &mut Ast,
    ) -> Result<Option<Node<'a>>, H::Error> {
        let result = {
            assert!(ast.open);
            ast.open = false;

            let parent = node.parent();
            let last_content_end = ast.sourcepos.end;

            if self.curline_len == 0 {
                ast.sourcepos.end = (self.line_number, self.last_line_length).into();
            } else if match ast.value {
                NodeValue::Document => true,
                NodeValue::CodeBlock(ref ncb) => ncb.fenced && ncb.closed,
                NodeValue::MultilineBlockQuote(..) => true,
                NodeValue::BlockDirective(..) => true,
                _ => false,
            } {
                ast.sourcepos.end = (self.line_number, self.curline_end_col).into();
            } else if matches!(
                ast.value,
                NodeValue::ThematicBreak
                    | NodeValue::TableOfContents
                    | NodeValue::TableRow(..)
                    | NodeValue::Table(..)
            ) {
                // sourcepos.end set by itself or managed below.
            } else {
                ast.sourcepos.end = (self.line_number - 1, self.last_line_length).into();
            }

            match ast.value {
                NodeValue::DescriptionList => {}
                NodeValue::DescriptionItem(..)
                | NodeValue::DescriptionTerm
                | NodeValue::DescriptionDetails => {
                    self.fix_zero_end_columns(node)?;
                    if !self.ctx.capture && ast.sourcepos.end.column == 0 {
                        ast.sourcepos.end =
                            Self::deepest_last_end(node).unwrap_or(ast.sourcepos.start);
                    }
                }
                NodeValue::Paragraph => {
                    let has_content = self.resolve_reference_link_definitions(ast.lines_mut())?;
                    if has_content {
                        let content = &mut ast.lines_mut().content;
                        *content = self.take_content(content)?;
                    } else {
                        if let Some(buffer) = ast.lines_mut().buffer {
                            self.ctx.fact(AcceptedFact::Seal {
                                buffer,
                                len: ast.lines_mut().base + ast.lines_mut().content.len(),
                            })?;
                        }
                        let lines = self.ctx.unbox(ast.lines.take().unwrap())?;
                        let previous = mem::replace(&mut self.working_content, lines.content);
                        self.ctx.discard_string(previous)?;
                        self.ctx.discard_small(lines.offsets)?;
                        self.working_content.clear();
                        node.detach();
                    }
                }
                NodeValue::CodeBlock(ref mut ncb) => {
                    let mut lines = self.ctx.unbox(ast.lines.take().unwrap())?;
                    let original_len = lines.base + lines.content.len();
                    let content = &mut lines.content;
                    if !ncb.fenced {
                        ast.sourcepos.end = last_content_end;
                        let retained =
                            strings::remove_trailing_blank_lines_ix_with_host(content, self.ctx)?;
                        if let Some(buffer) = lines.buffer {
                            if retained != content.len() {
                                self.ctx.fact(AcceptedFact::Cut {
                                    span: RSpan {
                                        buffer,
                                        start: lines.base + retained,
                                        end: original_len,
                                    },
                                    reason: SyntaxRole::BlockTerminator,
                                })?;
                            }
                            if let Some(node) = ast.native_id {
                                self.ctx.fact(AcceptedFact::CodeTerminal {
                                    node,
                                    buffer,
                                    at: lines.base + retained,
                                    eof: (retained == content.len())
                                        .then(|| {
                                            lines.eof_anchor.map(|at| PSpan { start: at, end: at })
                                        })
                                        .flatten(),
                                })?;
                            }
                        }
                        content.truncate(retained);
                        self.ctx.append(content, "\n")?;
                    } else {
                        let mut pos = 0;
                        while pos < content.len() {
                            self.ctx.scan_step()?;
                            if strings::is_line_end_char(content.as_bytes()[pos]) {
                                break;
                            }
                            pos += 1;
                        }

                        let payload = if let Some(buffer) = lines.buffer
                            && pos != 0
                        {
                            let payload = self.ctx.payload(
                                native::PayloadKind::FenceInfo,
                                RSpan {
                                    buffer,
                                    start: lines.base,
                                    end: lines.base + pos,
                                },
                            )?;
                            if let Some(node) = ast.native_id {
                                self.ctx
                                    .fact(AcceptedFact::BlockPayload { node, payload })?;
                            }
                            Some(payload)
                        } else {
                            None
                        };
                        #[allow(unused_mut)]
                        // Mutable only when the original attributes feature is built.
                        let mut info = strings::clean_payload_with_host(
                            &content[..pos],
                            native::PayloadKind::FenceInfo,
                            self.ctx,
                            payload,
                        )?;

                        #[cfg(feature = "attributes")]
                        if self.options.extension.fenced_code_attributes
                            && let Some(attrs) = attributes::parse_off(&mut info)
                        {
                            ast.attrs = Some(Box::new(attrs));
                        }

                        let info = if info.is_empty() {
                            self.options
                                .parse
                                .default_info_string
                                .as_ref()
                                .map_or(info, |s| s.clone())
                        } else {
                            info
                        };
                        ncb.info = self.ctx.finish_payload_value(
                            info,
                            payload,
                            native::PayloadKind::FenceInfo,
                        )?;

                        if content.as_bytes().get(pos) == Some(&b'\r') {
                            pos += 1;
                        }
                        if content.as_bytes().get(pos) == Some(&b'\n') {
                            pos += 1;
                        }

                        if let Some(buffer) = lines.buffer {
                            self.ctx.fact(AcceptedFact::Cut {
                                span: RSpan {
                                    buffer,
                                    start: lines.base,
                                    end: lines.base + pos,
                                },
                                reason: SyntaxRole::CodeInfo,
                            })?;
                        }
                        let remaining = self.ctx.string(&content[pos..])?;
                        let previous = mem::replace(content, remaining);
                        self.ctx.discard_string(previous)?;
                        lines.base += pos;
                        if !content.is_empty() {
                            self.ctx.scan_step()?;
                            if let Some(eof) = lines.eof_anchor
                                && !strings::is_line_end_char(*content.as_bytes().last().unwrap())
                            {
                                // The accepted body reaches a real unterminated input EOF line.
                                // Its LF is new output; the original R buffer remains unchanged.
                                self.ctx.append(content, "\n")?;
                                if let (Some(node), Some(buffer)) = (ast.native_id, lines.buffer) {
                                    self.ctx.fact(AcceptedFact::CodeTerminal {
                                        node,
                                        buffer,
                                        at: original_len,
                                        eof: Some(PSpan {
                                            start: eof,
                                            end: eof,
                                        }),
                                    })?;
                                }
                            }
                        }
                    }
                    if let Some(buffer) = lines.buffer {
                        self.ctx.fact(AcceptedFact::Seal {
                            buffer,
                            len: original_len,
                        })?;
                        if let Some(node) = ast.native_id {
                            // Literal copy range is independent of the terminal projection separator.
                            let literal_end = if ncb.fenced {
                                original_len
                            } else {
                                lines.base + content.len() - 1
                            };
                            self.ctx.fact(AcceptedFact::TextAccepted {
                                node,
                                span: RSpan {
                                    buffer,
                                    start: lines.base,
                                    end: literal_end,
                                },
                            })?;
                        }
                    }
                    self.ctx
                        .host
                        .admit(NativeAdmission::DecodedBytes(content.len() as u64))?;
                    ncb.literal = if self.ctx.direct {
                        let literal = self.ctx.finish_node_value(
                            ValueText::pending(),
                            ast.native_id,
                            NodeValueRole::BlockCode,
                        )?;
                        self.recycle_content(content)?;
                        literal
                    } else {
                        self.take_content(content)?.into()
                    };
                    self.ctx.discard_small(lines.offsets)?;
                }
                NodeValue::HtmlBlock(ref mut nhb) => {
                    let mut lines = self.ctx.unbox(ast.lines.take().unwrap())?;
                    if let (Some(node), Some(buffer)) = (ast.native_id, lines.buffer) {
                        self.ctx.fact(AcceptedFact::Seal {
                            buffer,
                            len: lines.base + lines.content.len(),
                        })?;
                        self.ctx.fact(AcceptedFact::RawHtml {
                            node,
                            block: true,
                            span: RSpan {
                                buffer,
                                start: lines.base,
                                end: lines.base + lines.content.len(),
                            },
                        })?;
                    }
                    let trimmed_len = strings::remove_trailing_blank_lines_ix_with_host(
                        &lines.content,
                        self.ctx,
                    )?;
                    let (num_lines, last_line_len) =
                        strings::count_newlines_with_host(&lines.content[..trimmed_len], self.ctx)?;
                    let end_line = ast.sourcepos.start.line + num_lines;
                    let end_col =
                        lines.offsets.get(num_lines).copied().unwrap_or(0) + last_line_len;
                    ast.sourcepos.end = (end_line, end_col).into();

                    self.ctx
                        .host
                        .admit(NativeAdmission::DecodedBytes(lines.content.len() as u64))?;
                    nhb.literal = if self.ctx.direct {
                        let literal = self.ctx.finish_node_value(
                            ValueText::pending(),
                            ast.native_id,
                            NodeValueRole::Html,
                        )?;
                        self.recycle_content(&mut lines.content)?;
                        literal
                    } else {
                        self.take_content(&mut lines.content)?.into()
                    };
                    self.ctx.discard_small(lines.offsets)?;
                }
                #[cfg(feature = "phoenix_heex")]
                NodeValue::HeexBlock(ref mut nhb) => {
                    let mut lines = self.ctx.unbox(ast.lines.take().unwrap())?;
                    let trimmed = strings::remove_trailing_blank_lines_slice(&lines.content);
                    let (num_lines, last_line_len) = strings::count_newlines(trimmed);
                    let end_line = ast.sourcepos.start.line + num_lines;
                    let end_col =
                        lines.offsets.get(num_lines).copied().unwrap_or(0) + last_line_len;
                    ast.sourcepos.end = (end_line, end_col).into();

                    nhb.literal = self.take_content(&mut lines.content)?;
                }
                NodeValue::List(ref mut nl) => {
                    if let Some(candidate_end) = self.fix_zero_end_columns(node)? {
                        ast.sourcepos.end = candidate_end;
                    }
                    nl.tight = self.determine_list_tight(node)?;
                }
                NodeValue::FootnoteDefinition(_) => {
                    if let Some(candidate_end) = self.fix_zero_end_columns(node)? {
                        ast.sourcepos.end = candidate_end;
                    }
                }
                NodeValue::Heading(_) => {
                    #[cfg(feature = "attributes")]
                    if self.options.extension.header_attributes
                        && let Some(attrs) = attributes::parse_off(&mut ast.lines_mut().content)
                    {
                        ast.attrs = Some(Box::new(attrs));
                    }
                    let content = &mut ast.lines_mut().content;
                    *content = self.take_content(content)?;
                }
                NodeValue::Subtext => {
                    let content = &mut ast.lines_mut().content;
                    *content = self.take_content(content)?;
                }
                _ => (),
            }

            parent
        };
        Ok(result)
    }

    fn take_content(&mut self, content: &mut String) -> Result<String, H::Error> {
        let result = {
            let exact = self.ctx.string(content)?;
            self.recycle_content(content)?;
            exact
        };
        Ok(result)
    }

    fn recycle_content(&mut self, content: &mut String) -> Result<(), H::Error> {
        // Preserve the original working-buffer owner and its charged capacity.
        let previous = mem::replace(&mut self.working_content, mem::take(content));
        self.ctx.discard_string(previous)?;
        self.working_content.clear();
        Ok(())
    }

    fn determine_list_tight(&mut self, node: Node<'a>) -> Result<bool, H::Error> {
        let result = 'native: {
            let mut ch = node.first_child();

            while let Some(item) = ch {
                self.ctx.event()?;
                if item.data().last_line_blank && item.next_sibling().is_some() {
                    break 'native false;
                }

                let mut subch = item.first_child();
                while let Some(subitem) = subch {
                    self.ctx.event()?;
                    if (item.next_sibling().is_some() || subitem.next_sibling().is_some())
                        && subitem.ends_with_blank_line_with_host(self.ctx)?
                    {
                        break 'native false;
                    }
                    subch = subitem.next_sibling();
                }

                ch = item.next_sibling();
            }

            true
        };
        Ok(result)
    }

    fn process_inlines(&mut self) -> Result<(), H::Error> {
        if self.ctx.capture && self.options.extension.tasklist {
            // Save the original block successor before task acceptance or
            // inline insertion. Detaching an empty label then cannot lose its
            // later block or a following list item.
            let mut current = Some(self.root);
            while let Some(node) = current {
                self.ctx.event()?;
                current = if let Some(child) = node.first_child() {
                    Some(child)
                } else {
                    let mut cursor = node;
                    loop {
                        self.ctx.event()?;
                        if let Some(sibling) = cursor.next_sibling() {
                            break Some(sibling);
                        }
                        if let Some(parent) = cursor.parent() {
                            cursor = parent;
                        } else {
                            break None;
                        }
                    }
                };
                if !node.data().value.contains_inlines() {
                    continue;
                }
                let first_item_paragraph = matches!(node.data().value, NodeValue::Paragraph)
                    && node.previous_sibling().is_none()
                    && node.parent().is_some_and(|p| {
                        matches!(p.data().value, NodeValue::Item(_))
                            && p.parent()
                                .is_some_and(|list| matches!(list.data().value, NodeValue::List(_)))
                    });
                if first_item_paragraph {
                    let accepted = {
                        let data = node.data();
                        let lines = data.lines.as_ref().ok_or_else(|| {
                            self.ctx.host.reject(NativeViolation::InvalidProvenance)
                        })?;
                        let buffer = lines.buffer.ok_or_else(|| {
                            self.ctx.host.reject(NativeViolation::InvalidProvenance)
                        })?;
                        task::probe(self.ctx, buffer, lines.base, &lines.content)?
                    };
                    if let Some(accepted) = accepted {
                        task::accept(self, node, accepted)?;
                    }
                }
                self.parse_inlines(node)?;
                task::finish(node);
            }
            return Ok(());
        }
        {
            for node in self.root.descendants() {
                self.ctx.event()?;
                if node.data().value.contains_inlines() {
                    self.parse_inlines(node)?;
                }
            }
        };
        Ok(())
    }

    fn parse_inlines(&mut self, node: Node<'a>) -> Result<(), H::Error> {
        {
            let (lines, line) = {
                let mut node_data = node.data_mut();
                let lines = node_data.lines.take().unwrap();
                let line = if self.ctx.capture && lines.opened_line != 0 {
                    lines.opened_line
                } else {
                    node_data.sourcepos.start.line
                };
                (lines, line)
            };
            let BlockLines {
                mut content,
                offsets,
                buffer,
                base,
                opened_line: _,
                eof_anchor: _,
            } = self.ctx.unbox(lines)?;
            let previous_len = content.len();
            strings::rtrim_with_host(&mut content, self.ctx)?;
            if let Some(buffer) = buffer {
                if content.len() != previous_len {
                    self.ctx.fact(AcceptedFact::Cut {
                        span: RSpan {
                            buffer,
                            start: base + content.len(),
                            end: base + previous_len,
                        },
                        reason: SyntaxRole::InlineTrim,
                    })?;
                }
                if self.options.extension.native_heading_ids
                    && matches!(node.data().value, NodeValue::Heading(_))
                    && content.ends_with('}')
                {
                    self.ctx.scan(content.len())?;
                    if let Some(start) = content.rfind("{#")
                        && (start == 0 || content.as_bytes()[start - 1].is_ascii_whitespace())
                        && native::heading_identifier(&content[start..]).is_some()
                    {
                        self.ctx.fact(AcceptedFact::Cut {
                            span: RSpan {
                                buffer,
                                start: base + start,
                                end: base + content.len(),
                            },
                            reason: SyntaxRole::HeadingIdentifier,
                        })?;
                        content.truncate(start);
                        strings::rtrim_with_host(&mut content, self.ctx)?;
                        if content.len() != start {
                            self.ctx.fact(AcceptedFact::Cut {
                                span: RSpan {
                                    buffer,
                                    start: base + content.len(),
                                    end: base + start,
                                },
                                reason: SyntaxRole::InlineTrim,
                            })?;
                        }
                    }
                }
                self.ctx.fact(AcceptedFact::Seal {
                    buffer,
                    len: base + previous_len,
                })?;
            }

            if self.ctx.inline_policy == native::InlinePolicy::StrictG4
                && native::g4::has_raw_autolink_marker(content.as_bytes(), self.ctx)?
            {
                let node_id = node
                    .data()
                    .native_id
                    .ok_or_else(|| self.ctx.host.reject(NativeViolation::InvalidProvenance))?;
                let buffer_id = buffer
                    .ok_or_else(|| self.ctx.host.reject(NativeViolation::InvalidProvenance))?;
                native::g4::with_leaf(node_id, buffer_id, base, content.len(), |leaf| {
                    let released = native::g4::shadow::run(
                        leaf,
                        native::g4::shadow::Seed {
                            raw: &content,
                            offsets: &offsets,
                            line,
                            in_paragraph: matches!(node.data().value, NodeValue::Paragraph),
                            in_table_cell: matches!(node.data().value, NodeValue::TableCell),
                            options: self.options,
                            refmap: &self.refmap,
                            tables: &self.inline_tables,
                        },
                        self.ctx,
                    )?;
                    let selection = native::g4::select(&content, released, self.ctx)?;
                    let mut subj = inlines::Subject::new(
                        self.ctx,
                        buffer,
                        base,
                        self.arena,
                        self.options,
                        content,
                        line,
                        &self.refmap,
                        &self.inline_tables,
                        &mut self.footnote_defs,
                        self.delimiter_arena,
                        0,
                    );
                    subj.parse_selected(node, line, &offsets, selection)?;
                    subj.process_emphasis(0)?;
                    subj.clear_brackets()?;
                    subj.release_owned()
                })?;
            } else {
                let mut subj = inlines::Subject::new(
                    self.ctx,
                    buffer,
                    base,
                    self.arena,
                    self.options,
                    content,
                    line,
                    &self.refmap,
                    &self.inline_tables,
                    &mut self.footnote_defs,
                    self.delimiter_arena,
                    0,
                );

                while subj.parse_inline(node, line, &offsets)? {}
                subj.process_emphasis(0)?;
                subj.clear_brackets()?;
                subj.release_owned()?;
            }

            self.ctx.discard_small(offsets)?;
        };
        Ok(())
    }

    fn process_footnotes(&mut self) -> Result<(), H::Error> {
        {
            // TODO: combine find_footnote_definitions and find_footnote_references into one pass!
            let mut fd_map = HashMap::new();
            Self::find_footnote_definitions(self.root, &mut fd_map);

            let mut next_ix = 0;
            self.find_footnote_references(&mut fd_map, &mut next_ix)?;

            let mut fds = fd_map.into_values().collect::<Vec<_>>();
            fds.sort_unstable_by_key(|a| a.ix);
            for fd in fds {
                if fd.ix.is_some() {
                    let NodeValue::FootnoteDefinition(ref mut nfd) = fd.node.data_mut().value
                    else {
                        unreachable!()
                    };
                    nfd.name = fd.name.to_string();
                    nfd.total_references = fd.total_references;
                    if !self.options.parse.leave_footnote_definitions {
                        self.root.append(fd.node);
                    }
                } else if !self.options.parse.leave_footnote_definitions {
                    fd.node.detach();
                }
            }
        };
        Ok(())
    }

    fn find_footnote_definitions(
        root: Node<'a>,
        map: &mut HashMap<String, FootnoteDefinition<'a>>,
    ) {
        let mut stack = vec![root];
        while let Some(node) = stack.pop() {
            match node.data().value {
                NodeValue::FootnoteDefinition(ref nfd) => {
                    map.insert(
                        strings::normalize_label(&nfd.name, Case::Fold),
                        FootnoteDefinition {
                            ix: None,
                            node,
                            name: strings::normalize_label(&nfd.name, Case::Preserve),
                            total_references: 0,
                        },
                    );
                }
                _ => {
                    stack.extend(node.reverse_children());
                }
            }
        }
    }

    fn find_footnote_references(
        &mut self,
        map: &mut HashMap<String, FootnoteDefinition>,
        ixp: &mut u32,
    ) -> Result<(), H::Error> {
        {
            let mut stack = vec![self.root];
            while let Some(node) = stack.pop() {
                let mut ast = node.data_mut();
                let sp = ast.sourcepos;
                match ast.value {
                    NodeValue::FootnoteReference(ref mut nfr) => {
                        let normalized = strings::normalize_label(&nfr.name, Case::Fold);
                        if let Some(ref mut footnote) = map.get_mut(&normalized) {
                            let ix = match footnote.ix {
                                Some(ix) => ix,
                                None => {
                                    *ixp += 1;
                                    footnote.ix = Some(*ixp);
                                    *ixp
                                }
                            };
                            footnote.total_references += 1;
                            nfr.ref_num = footnote.total_references;
                            nfr.ix = ix;
                            nfr.name = strings::normalize_label(&footnote.name, Case::Preserve);
                        } else {
                            // Restore the nodes as they were-ish.  We restore each
                            // Text node as it was found, preserving the sourcepos
                            // spans.  This is important for accurate sourcepos
                            // tracking; we assert when 'consuming' sourcepos
                            // lengths in post-processing that either the span
                            // length matches the byte count of the string (meaning
                            // we can reliably subset them both), or that we're
                            // consuming a whole span.  Trying to consume part of a
                            // span without a matching length is undefined, and we
                            // will crash; see Spx::consume.
                            //
                            // See HACK comment in
                            // `inlines::Subject::handle_close_bracket` for the
                            // producer of these values.
                            assert!(!nfr.texts.is_empty());
                            let mut lc = sp.start;
                            let mut target = node;

                            let mut texts = mem::take(&mut nfr.texts);
                            texts.insert(0, ("[".into(), 1));
                            texts.push(("]".into(), 1));

                            for (text, span) in &mut texts {
                                let inl = self.arena.try_alloc(
                                    Ast::new_with_sourcepos(
                                        NodeValue::Text(self.ctx.string(text)?.into()),
                                        (lc, lc.column_add(*span as isize - 1)).into(),
                                    )
                                    .into(),
                                    self.ctx,
                                )?;
                                target.insert_after(inl);
                                target = inl;
                                lc = lc.column_add(*span as isize);
                            }
                            node.detach();
                        }
                    }
                    _ => {
                        stack.extend(node.reverse_children());
                    }
                }
            }
        };
        Ok(())
    }

    fn postprocess_text_nodes(&mut self, root: Node<'a>) -> Result<(), H::Error> {
        {
            let mut stack = Vec::new();
            self.ctx.push(&mut stack, (root, false))?;
            let mut children = vec![];
            let coalesce_escaped =
                !(self.options.parse.escaped_char_spans || self.options.render.escaped_char_spans);

            while let Some((parent, in_bracket_context)) = stack.pop() {
                self.ctx.event()?;
                let mut it = parent.first_child();
                let mut escaped_to_coalesce = vec![];

                while let Some(node) = it {
                    self.ctx.event()?;
                    let mut child_in_bracket_context = in_bracket_context;
                    let mut emptied = false;
                    let ast = &mut node.data_mut();

                    let sourcepos = ast.sourcepos;
                    let native_id = ast.native_id;
                    match ast.value {
                        NodeValue::Text(ref mut text) => {
                            let sourcepos = self.postprocess_text_node_with_context(
                                node,
                                native_id,
                                sourcepos,
                                text,
                                in_bracket_context,
                            )?;
                            emptied = self.ctx.value_is_empty(text)?;
                            ast.sourcepos = sourcepos;
                        }
                        NodeValue::Link(..) | NodeValue::Image(..) | NodeValue::WikiLink(..) => {
                            // Recurse into links, images, and wikilinks to join adjacent text nodes,
                            // but mark the context so autolinks won't be generated within them.
                            child_in_bracket_context = true;
                        }
                        NodeValue::Escaped if coalesce_escaped => {
                            self.ctx.push(&mut escaped_to_coalesce, node)?;
                        }
                        _ => {}
                    }

                    if !emptied {
                        self.ctx
                            .push(&mut children, (node, child_in_bracket_context))?;
                    }

                    it = node.next_sibling();

                    if emptied {
                        node.detach();
                    }
                }

                // Remove Escaped from the tree, coalescing with adjacent nodes.
                for node in escaped_to_coalesce.drain(..) {
                    self.ctx.event()?;
                    let escaped_text = node.first_child().unwrap();
                    node.insert_before(escaped_text);
                    node.detach();

                    let mut target = escaped_text;

                    // We only need look one left and one right, as all adjacent
                    // Text nodes are coalesced already.
                    if let Some(before) = target.previous_sibling() {
                        let mut before_mut = before.data_mut();
                        let before_id = before_mut.native_id;
                        if let Some(before_text) = before_mut.value.text_mut() {
                            let mut target_data = target.data_mut();
                            let target_id = target_data.native_id;
                            let target_end = target_data.sourcepos.end;
                            let target_text = target_data.value.text_mut().unwrap();
                            self.join_text_values(before_text, before_id, target_text, target_id)?;
                            let donor = mem::replace(target_text, self.ctx.empty_value());
                            before_mut.sourcepos.end = target_end;
                            self.ctx.discard_value(donor)?;
                            target.detach();

                            target = before;
                        }
                    }

                    if let Some(after) = target.next_sibling() {
                        let mut after_mut = after.data_mut();
                        let after_id = after_mut.native_id;
                        let after_end = after_mut.sourcepos.end;
                        if let Some(after_text) = after_mut.value.text_mut() {
                            let mut target_mut = target.data_mut();
                            let target_id = target_mut.native_id;
                            self.join_text_values(
                                target_mut.value.text_mut().unwrap(),
                                target_id,
                                after_text,
                                after_id,
                            )?;
                            let donor = mem::replace(after_text, self.ctx.empty_value());
                            target_mut.sourcepos.end = after_end;
                            self.ctx.discard_value(donor)?;
                            after.detach();
                        }
                    }
                }

                // Push children onto work stack in reverse order so they are
                // traversed in order
                self.ctx.discard_vec(escaped_to_coalesce)?;
                for child in children.drain(..).rev() {
                    self.ctx.push(&mut stack, child)?;
                }
            }
            self.ctx.discard_vec(stack)?;
            self.ctx.discard_vec(children)?;
        };
        Ok(())
    }

    fn join_text_values(
        &mut self,
        survivor: &mut ValueText,
        survivor_id: Option<native::NativeNodeId>,
        donor: &ValueText,
        donor_id: Option<native::NativeNodeId>,
    ) -> Result<usize, H::Error> {
        let before = self.ctx.value_len(survivor)?;
        let donor_bytes = self.ctx.value_len(donor)?;
        if let (Some(survivor), Some(donor)) = (survivor_id, donor_id) {
            self.ctx.fact(AcceptedFact::TextJoin {
                survivor,
                donor,
                survivor_bytes_before: before,
            })?;
        } else if self.ctx.direct {
            return Err(self.ctx.host.reject(NativeViolation::InvalidProvenance));
        }
        if self.ctx.direct {
            // TextJoin already transported the accepted atoms; no second body is appended.
            let joined = before
                .checked_add(donor_bytes)
                .ok_or_else(|| self.ctx.host.reject(NativeViolation::ArithmeticOverflow))?;
            if self.ctx.value_len(survivor)? != joined {
                return Err(self.ctx.host.reject(NativeViolation::InvalidProvenance));
            }
        } else {
            let survivor = survivor
                .owned_cow_mut()
                .ok_or_else(|| self.ctx.host.reject(NativeViolation::InvalidProvenance))?;
            let donor = donor
                .owned_str()
                .ok_or_else(|| self.ctx.host.reject(NativeViolation::InvalidProvenance))?;
            self.ctx.append_cow(survivor, donor)?;
        }
        Ok(donor_bytes)
    }

    fn postprocess_text_node_with_context(
        &mut self,
        node: Node<'a>,
        native_id: Option<native::NativeNodeId>,
        mut sourcepos: Sourcepos,
        root: &mut ValueText,
        in_bracket_context: bool,
    ) -> Result<Sourcepos, H::Error> {
        let result = {
            // Join adjacent text nodes together, then post-process.
            // Record the original list of sourcepos and bytecounts
            // for the post-processing step.

            let needs_spx = self.options.extension.tasklist
                || (self.options.extension.autolink && !in_bracket_context);

            let mut spxv = SmallVec::new();
            if needs_spx {
                spxv.push((sourcepos, self.ctx.value_len(root)?));
            }

            while let Some(ns) = node.next_sibling() {
                let mut ns_data = ns.data_mut();
                let sp = ns_data.sourcepos;
                let NodeValue::Text(ref mut adj) = ns_data.value else {
                    break;
                };
                let adj = mem::replace(adj, self.ctx.empty_value());
                let donor = ns_data.native_id;
                drop(ns_data);
                let donor_bytes = self.join_text_values(root, native_id, &adj, donor)?;
                if needs_spx {
                    spxv.push((sp, donor_bytes));
                }
                sourcepos.end.column = sp.end.column;
                self.ctx.discard_value(adj)?;
                ns.detach();
            }

            if needs_spx {
                self.postprocess_text_node_with_context_inner(
                    node,
                    root,
                    &mut sourcepos,
                    spxv,
                    in_bracket_context,
                )?;
            }

            sourcepos
        };
        Ok(result)
    }

    fn postprocess_text_node_with_context_inner(
        &mut self,
        node: Node<'a>,
        text: &mut ValueText,
        sourcepos: &mut Sourcepos,
        spxv: SmallVec<[(Sourcepos, usize); 4]>,
        in_bracket_context: bool,
    ) -> Result<(), H::Error> {
        {
            let mut spx = Spx {
                items: spxv,
                front: 0,
            };
            if self.options.extension.tasklist && !self.ctx.capture {
                let owned = text
                    .owned_cow_mut()
                    .ok_or_else(|| self.ctx.host.reject(NativeViolation::InvalidProvenance))?;
                self.process_tasklist(node, owned, sourcepos, &mut spx)?;
            }

            if self.options.extension.autolink && !in_bracket_context {
                autolink::process_email_autolinks(
                    self.ctx,
                    self.arena,
                    node,
                    text,
                    self.options.parse.relaxed_autolinks,
                    sourcepos,
                    &mut spx,
                )?;
            }
        };
        Ok(())
    }

    // Processes tasklist items in a text node.  This function
    // must not detach `node`, as we iterate through siblings in
    // `postprocess_text_nodes_with_context` and may end up relying on it
    // remaining in place.
    //
    // `text` is the mutably borrowed textual content of `node`.  If it is empty
    // after the call to `process_tasklist`, it will be properly cleaned up.
    fn process_tasklist(
        &mut self,
        node: Node<'a>,
        text: &mut Cow<'static, str>,
        sourcepos: &mut Sourcepos,
        spx: &mut Spx,
    ) -> Result<(), H::Error> {
        'native: {
            let (end, matched, symbol_range) = match scanners::tasklist(text) {
                Some(p) => p,
                None => break 'native,
            };

            let mut chars = matched.chars();
            let Some(symbol) = chars.next() else {
                break 'native;
            };

            // There must be at most one `char`'s worth of content in `matched`,
            // otherwise we ignore it.
            if chars.next().is_some() {
                break 'native;
            }

            if !self.options.parse.relaxed_tasklist_matching && !matches!(symbol, ' ' | 'x' | 'X') {
                break 'native;
            }

            let symbol_sourcepos: Sourcepos = (
                sourcepos.start.column_add(symbol_range.start as isize),
                // `- 1`: symbol_range is end-exclusive, but sourcepos are inclusive.
                sourcepos.start.column_add(symbol_range.end as isize - 1),
            )
                .into();

            let parent = node.parent().unwrap();

            if node_matches!(parent, NodeValue::TableCell) {
                if !self.options.parse.tasklist_in_table {
                    break 'native;
                }

                if node.previous_sibling().is_some() || node.next_sibling().is_some() {
                    break 'native;
                }

                // For now, require the task item is the only content of the table cell.
                // If we want to relax this later, we can.
                if end != text.len() {
                    break 'native;
                }

                strings::remove_from_start(self.ctx.cow_mut(text)?, end);
                parent.prepend(
                    self.arena.try_alloc(
                        Ast::new_with_sourcepos(
                            NodeValue::TaskItem(NodeTaskItem {
                                symbol: if symbol == ' ' { None } else { Some(symbol) },
                                symbol_sourcepos,
                            }),
                            *sourcepos,
                        )
                        .into(),
                        self.ctx,
                    )?,
                );
            } else if node_matches!(parent, NodeValue::Paragraph) {
                if node.previous_sibling().is_some() || parent.previous_sibling().is_some() {
                    break 'native;
                }

                let grandparent = parent.parent().unwrap();
                if !node_matches!(grandparent, NodeValue::Item(..)) {
                    break 'native;
                }

                let great_grandparent = grandparent.parent().unwrap();
                if !node_matches!(great_grandparent, NodeValue::List(..)) {
                    break 'native;
                }

                // These are sound only because the exact text that we've matched and
                // the count thereof (i.e. "end") will precisely map to characters in
                // the source document.
                strings::remove_from_start(self.ctx.cow_mut(text)?, end);

                let adjust = spx.consume(end) + 1;
                assert_eq!(sourcepos.start.column, parent.data().sourcepos.start.column);

                // See tests::fuzz::echaw9. The paragraph doesn't exist in the source,
                // so we remove it.
                if sourcepos.end.column < adjust && node.next_sibling().is_none() {
                    parent.detach();
                } else {
                    sourcepos.start.column = adjust;
                    parent.data_mut().sourcepos.start.column = adjust;
                }

                grandparent.data_mut().value = NodeValue::TaskItem(NodeTaskItem {
                    symbol: if symbol == ' ' { None } else { Some(symbol) },
                    symbol_sourcepos,
                });

                if let NodeValue::List(list) = &mut great_grandparent.data_mut().value {
                    list.is_task_list = true;
                }
            }
        };
        Ok(())
    }

    fn parse_reference_inline(
        &mut self,
        content: &str,
        buffer: Option<BufferId>,
        base: usize,
    ) -> Result<ReferenceParse, H::Error> {
        let result = 'native: {
            let mut scanner = inlines::Scanner::new();

            let lab = match scanner.link_label_with_host(content, self.ctx)? {
                Some(lab) if !lab.is_empty() => lab,
                _ => break 'native None,
            };

            let label_end = scanner.pos - 1;
            if scanner.peek_byte(content) != Some(b':') {
                break 'native None;
            }

            scanner.pos += 1;
            scanner.spnl_with_host(content, self.ctx)?;
            let url_start = scanner.pos;
            let (url, matchlen) =
                match inlines::manual_scan_link_url_with_host(&content[scanner.pos..], self.ctx)? {
                    Some((url, matchlen)) => (url, matchlen),
                    None => break 'native None,
                };
            scanner.pos += matchlen;

            let beforetitle = scanner.pos;
            scanner.spnl_with_host(content, self.ctx)?;
            let title_start = scanner.pos;
            let title_search = if scanner.pos == beforetitle {
                None
            } else {
                scanners::link_title_with_host(&content[scanner.pos..], self.ctx)?
            };
            let mut title = match title_search {
                Some(matchlen) => {
                    let t = &content[scanner.pos..scanner.pos + matchlen];
                    scanner.pos += matchlen;
                    t
                }
                _ => {
                    scanner.pos = beforetitle;
                    ""
                }
            };

            scanner.skip_spaces_with_host(content, self.ctx)?;
            if !scanner.skip_line_end_with_host(content, self.ctx)? {
                if !title.is_empty() {
                    title = "";
                    scanner.pos = beforetitle;
                    scanner.skip_spaces_with_host(content, self.ctx)?;
                    if !scanner.skip_line_end_with_host(content, self.ctx)? {
                        break 'native None;
                    }
                } else {
                    break 'native None;
                }
            }

            let lab = strings::normalize_label_with_host(lab, Case::Fold, self.ctx)?;
            let mut rr = None;
            if !lab.is_empty() {
                let (native, destination, decoded_title) = if let Some(buffer) = buffer {
                    let definition = self.ctx.definition()?;
                    let angle = content.as_bytes().get(url_start) == Some(&b'<');
                    let raw_url = RSpan {
                        buffer,
                        start: base + url_start + usize::from(angle),
                        end: base + url_start + matchlen - usize::from(angle),
                    };
                    let destination = self
                        .ctx
                        .payload(native::PayloadKind::Destination, raw_url)?;
                    let decoded_url = strings::clean_payload_with_host(
                        url,
                        native::PayloadKind::Destination,
                        self.ctx,
                        Some(destination),
                    )?;
                    let title_id = if title.is_empty() {
                        None
                    } else {
                        Some(self.ctx.payload(
                            native::PayloadKind::Title,
                            RSpan {
                                buffer,
                                start: base + title_start,
                                end: base + title_start + title.len(),
                            },
                        )?)
                    };
                    let decoded_title =
                        strings::clean_reference_title_with_host(title, self.ctx, title_id)?;
                    self.ctx.fact(AcceptedFact::Definition {
                        definition,
                        buffer,
                        syntax: RSpan {
                            buffer,
                            start: base,
                            end: base + scanner.pos,
                        },
                        label: RSpan {
                            buffer,
                            start: base + 1,
                            end: base + label_end,
                        },
                        normalized_label: &lab,
                        destination,
                        title: title_id,
                    })?;
                    (
                        Some(native::ReferenceIdentity {
                            definition,
                            destination,
                            title: title_id,
                        }),
                        self.ctx.finish_payload_value(
                            decoded_url,
                            Some(destination),
                            native::PayloadKind::Destination,
                        )?,
                        self.ctx.finish_payload_value(
                            decoded_title,
                            title_id,
                            native::PayloadKind::Title,
                        )?,
                    )
                } else {
                    (
                        None,
                        self.ctx.string(&strings::clean_url(url))?.into(),
                        self.ctx
                            .string(&strings::clean_reference_title(title))?
                            .into(),
                    )
                };
                rr = Some((
                    lab,
                    ResolvedReference {
                        url: destination,
                        title: decoded_title,
                        native,
                    },
                ));
            } else {
                self.ctx.discard_string(lab)?;
            }
            Some((scanner.pos, rr))
        };
        Ok(result)
    }
}

enum AddTextResult {
    LiteralText,
    HtmlBlock(u8),
    #[cfg(feature = "phoenix_heex")]
    HeexBlock(Box<phoenix_heex::NodeHeexBlock>),
    Otherwise,
}

fn parse_list_marker_with_host<H: Host>(
    line: &str,
    mut pos: usize,
    interrupts_paragraph: bool,
    ctx: &mut Context<'_, H>,
) -> Result<Option<(usize, NodeList)>, H::Error> {
    let result = 'native: {
        let bytes = line.as_bytes();
        if pos >= line.len() {
            break 'native None;
        }
        let mut c = bytes[pos];
        let startpos = pos;

        if c == b'*' || c == b'-' || c == b'+' {
            pos += 1;
            if !bytes.get(pos).is_none_or(|&b| isspace(b)) {
                break 'native None;
            }

            if interrupts_paragraph {
                // "However, an empty list item cannot interrupt a paragraph:"
                let mut i = pos;
                if i == bytes.len() {
                    break 'native None;
                }

                while strings::is_space_or_tab(bytes[i]) {
                    ctx.scan_step()?;
                    i += 1;
                    if i == bytes.len() {
                        break 'native None;
                    }
                }
                if strings::is_line_end_char(bytes[i]) {
                    break 'native None;
                }
            }

            break 'native Some((
                pos - startpos,
                NodeList {
                    list_type: ListType::Bullet,
                    marker_offset: 0,
                    padding: 0,
                    start: 1,
                    delimiter: ListDelimType::Period,
                    bullet_char: c,
                    tight: false,
                    is_task_list: false,
                },
            ));
        } else if isdigit(c) {
            let mut start: usize = 0;
            let mut digits = 0;

            loop {
                ctx.scan_step()?;
                start = (10 * start) + (bytes[pos] - b'0') as usize;
                pos += 1;
                digits += 1;

                if pos == bytes.len() {
                    break 'native None;
                }

                if !(digits < 9 && isdigit(bytes[pos])) {
                    break;
                }
            }

            if interrupts_paragraph && start != 1 {
                break 'native None;
            }

            c = bytes[pos];
            if c != b'.' && c != b')' {
                break 'native None;
            }

            pos += 1;

            if pos == bytes.len() || !isspace(bytes[pos]) {
                break 'native None;
            }

            if interrupts_paragraph {
                let mut i = pos;
                while strings::is_space_or_tab(bytes[i]) {
                    ctx.scan_step()?;
                    i += 1;
                    if i == bytes.len() {
                        break 'native None;
                    }
                }
                if strings::is_line_end_char(bytes[i]) {
                    break 'native None;
                }
            }

            break 'native Some((
                pos - startpos,
                NodeList {
                    list_type: ListType::Ordered,
                    marker_offset: 0,
                    padding: 0,
                    start,
                    delimiter: if c == b'.' {
                        ListDelimType::Period
                    } else {
                        ListDelimType::Paren
                    },
                    bullet_char: 0,
                    tight: false,
                    is_task_list: false,
                },
            ));
        }

        None
    };
    ctx.flush_scan()?;
    Ok(result)
}

fn lists_match(list_data: &NodeList, item_data: &NodeList) -> bool {
    list_data.list_type == item_data.list_type
        && list_data.delimiter == item_data.delimiter
        && list_data.bullet_char == item_data.bullet_char
}

fn reopen_ast_nodes<H: Host>(mut ast: Node<'_>, ctx: &mut Context<'_, H>) -> Result<(), H::Error> {
    loop {
        ctx.event()?;
        ast.data_mut().open = true;
        ast = match ast.parent() {
            Some(p) => p,
            None => return Ok(()),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AutolinkType {
    Uri,
    Email,
}

pub(crate) struct Spx {
    items: SmallVec<[(Sourcepos, usize); 4]>,
    front: usize,
}

impl Spx {
    // Sourcepos end column `e` of a node determined by advancing through `spx`
    // until `i` bytes of input are seen.
    //
    // For each element `(sp, x)` in `spx`:
    // - if remaining `i` is greater than the byte count `x`,
    //     set `i -= x` and continue.
    // - if remaining `i` is equal to the byte count `x`,
    //     set `e = sp.end.column` and finish.
    // - if remaining `i` is less than the byte count `x`,
    //     assert `sp.end.column - sp.start.column + 1 == x || i == 0` (1),
    //     set `e = sp.start.column + i - 1` and finish.
    //
    // (1) If `x` doesn't equal the range covered between the start and end column,
    //     there's no way to determine sourcepos within the range. This is a bug if
    //     it happens; it suggests we've matched an email autolink with some smart
    //     punctuation in it, or worse.
    //
    //     The one exception is if `i == 0`. Given nothing to consume, we can
    //     happily restore what we read, returning `sp.start.column - 1` for the
    //     end column of the original node.
    pub(crate) fn consume(&mut self, mut rem: usize) -> usize {
        loop {
            let Some(&(sp, x)) = self.items.get(self.front) else {
                unreachable!();
            };
            match rem.cmp(&x) {
                Ordering::Greater => {
                    self.front += 1;
                    rem -= x;
                }
                Ordering::Equal => {
                    self.front += 1;
                    return sp.end.column;
                }
                Ordering::Less => {
                    assert!((sp.end.column - sp.start.column + 1 == x) || rem == 0);
                    self.items[self.front] = (
                        (
                            sp.start.line,
                            sp.start.column + rem,
                            sp.end.line,
                            sp.end.column,
                        )
                            .into(),
                        x - rem,
                    );
                    return sp.start.column + rem - 1;
                }
            }
        }
    }
}
