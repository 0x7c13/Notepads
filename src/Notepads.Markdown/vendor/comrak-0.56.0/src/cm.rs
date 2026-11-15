// Notepads Markdown modifications Copyright (c) 2026, Jiaqi (0x7c13) Liu.
// Original third-party licenses remain applicable; see the crate COPYING file.

use std::cmp::max;
use std::collections::HashSet;
use std::fmt::{self, Write};
use std::str;

use crate::Arena;
use crate::ctype::{isalpha, isdigit, ispunct, ispunct_char, isspace, isspace_char};
use crate::nodes::{
    ListDelimType, ListType, Node, NodeAlert, NodeBlockDirective, NodeCodeBlock, NodeHeading,
    NodeHtmlBlock, NodeLink, NodeList, NodeMath, NodeTaskItem, NodeValue, NodeWikiLink,
    TableAlignment,
};
use crate::parser::options::{Options, Plugins, WikiLinksMode};
#[cfg(feature = "phoenix_heex")]
use crate::parser::phoenix_heex::NodeHeexBlock;
#[cfg(feature = "shortcodes")]
use crate::parser::shortcodes::NodeShortCode;
use crate::scanners;
use crate::strings::trim_start_match;
use crate::{node_matches, strings};

/// Formats an AST as CommonMark, modified by the given options.
pub fn format_document(root: Node<'_>, options: &Options, output: &mut dyn Write) -> fmt::Result {
    crate::html::require_owned_ast(root)?;
    // Formatting an ill-formed AST might lead to invalid output. However, we don't want to pay for
    // validation in normal workflow. As a middleground, we validate the AST in debug builds. See
    // https://github.com/kivikakk/comrak/issues/371.
    #[cfg(debug_assertions)]
    root.validate().unwrap_or_else(|e| {
        panic!("The document to format is ill-formed: {:?}", e);
    });

    format_document_with_plugins(root, options, output, &Plugins::default())
}

/// Formats an AST as CommonMark, modified by the given options. Accepts custom plugins.
pub fn format_document_with_plugins(
    root: Node<'_>,
    options: &Options,
    output: &mut dyn Write,
    _plugins: &Plugins,
) -> fmt::Result {
    crate::html::require_owned_ast(root)?;
    if !options.render.experimental_minimize_commonmark {
        return format_internal(root, options, output);
    }

    let mut result = String::new();
    format_internal(root, options, &mut result)?;
    minimize_commonmark(&mut result, options);

    output.write_str(&result)
}

// Doesn't honour experimental_minimize_commonmark.
fn format_internal(root: Node<'_>, options: &Options, output: &mut dyn Write) -> fmt::Result {
    let mut f = CommonMarkFormatter::new(root, options, output);
    f.format(root)
}

struct CommonMarkFormatter<'a, 'o, 'c, 'w> {
    node: Node<'a>,
    options: &'o Options<'c>,
    output: &'w mut dyn Write,
    /// Buffer used by wrapping implementation; flushed on newline or wrapping
    /// event.
    wrap_buffer: String,
    /// The last two bytes written (to output or wrap_buffer).
    window: Vec<u8>,
    /// Contains some assortment of:
    ///
    /// * "> " when formatting within a blockquote.
    /// * " "*n when formatting within a list item (n≥2).
    /// * "    " when formatting within a fenced code block.
    /// * "    " when formatting within a footnote definition.
    /// * "> " when formatting within an alert.
    ///
    /// A prefix when non-empty is therefore guaranteed to have a length of at
    /// least two. This is relevant in write_prefix.
    prefix: String,
    column: usize,
    need_cr: u8,
    last_breakable: usize,
    begin_line: bool,
    begin_content: bool,
    no_linebreaks: bool,
    in_tight_list_item: bool,
    custom_escape: Option<fn(Node<'a>, char) -> bool>,
    footnote_ix: u32,
    ol_stack: Vec<usize>,
    allow_wrap: bool,
}

#[derive(PartialEq, Clone, Copy)]
enum Escaping {
    Literal,
    Normal,
    Url,
    Title,
}

impl Write for CommonMarkFormatter<'_, '_, '_, '_> {
    fn write_str(&mut self, s: &str) -> fmt::Result {
        self.output(s, false, Escaping::Literal)
    }
}

impl<'a, 'o, 'c, 'w> CommonMarkFormatter<'a, 'o, 'c, 'w> {
    fn new(node: Node<'a>, options: &'o Options<'c>, output: &'w mut dyn Write) -> Self {
        CommonMarkFormatter {
            node,
            options,
            output,
            wrap_buffer: String::new(),
            window: Vec::with_capacity(2),
            prefix: String::new(),
            column: 0,
            need_cr: 0,
            last_breakable: 0,
            begin_line: true,
            begin_content: true,
            no_linebreaks: false,
            in_tight_list_item: false,
            custom_escape: None,
            footnote_ix: 0,
            ol_stack: vec![],
            allow_wrap: options.render.width > 0 && !options.render.hardbreaks,
        }
    }

    /// Writes to self.output, or self.buffer if we're wrapping output, and
    /// update self.window.
    fn write(&mut self, s: &str) -> fmt::Result {
        if s.is_empty() {
            return Ok(());
        }
        // We maintain a window size of 2 bytes at most.
        if s.len() > 1 {
            self.window.clear();
            self.window.extend_from_slice(&s.as_bytes()[s.len() - 2..]);
        } else {
            assert_eq!(s.len(), 1);
            if self.window.len() == 2 {
                self.window.remove(0);
            } else {
                assert!(self.window.len() < 2);
            }
            self.window.push(s.as_bytes()[0]);
        }

        let last_was_cr = self.window.last() == Some(&b'\n');

        if self.options.render.width == 0 {
            self.output.write_str(s)?;
        } else if last_was_cr {
            // Can flush.
            self.output.write_str(&self.wrap_buffer)?;
            self.output.write_str(s)?;
            self.wrap_buffer.clear();
        } else {
            self.wrap_buffer.push_str(s);
        }

        if last_was_cr {
            self.column = 0;
            self.begin_line = true;
            self.begin_content = true;
            self.last_breakable = 0;
        }

        Ok(())
    }

    fn write_prefix(&mut self) -> fmt::Result {
        // A non-empty prefix is guaranteed to have a length of at least two.
        if self.prefix.is_empty() {
            return Ok(());
        }
        if self.options.render.width == 0 {
            self.output.write_str(&self.prefix)?;
        } else {
            self.wrap_buffer.push_str(&self.prefix);
        }
        self.window.clear();
        self.window
            .extend_from_slice(&self.prefix.as_bytes()[self.prefix.len() - 2..]);
        Ok(())
    }

    fn output(&mut self, s: &str, wrap: bool, escaping: Escaping) -> fmt::Result {
        let bytes = s.as_bytes();
        let wrap = self.allow_wrap && wrap && !self.no_linebreaks;

        if self.in_tight_list_item && self.need_cr > 1 {
            self.need_cr = 1;
        }

        let mut last_crs_consume = self
            .window
            .iter()
            .rev()
            .take_while(|&&b| b == b'\n')
            .count();
        while self.need_cr > 0 {
            if self.window.is_empty() {
                // nvm
            } else if last_crs_consume > 0 {
                last_crs_consume -= 1;
            } else {
                self.write("\n")?;
                if self.need_cr > 1 {
                    self.write_prefix()?;
                    self.column = self.prefix.len();
                }
            }
            self.need_cr -= 1;
        }

        let mut it = s.char_indices();

        while let Some((mut i, c)) = it.next() {
            if self.begin_line {
                self.write_prefix()?;
                self.column = self.prefix.len();
            }

            if self.custom_escape.is_some_and(|f| f(self.node, c)) {
                self.write("\\")?;
            }

            let nextb = bytes.get(i + 1);
            if c == ' ' && wrap {
                if !self.begin_line {
                    let last_nonspace = self.wrap_buffer.len();
                    self.write(" ")?;
                    self.column += 1;
                    self.begin_line = false;
                    self.begin_content = false;
                    while bytes.get(i + 1) == Some(&(b' ')) {
                        (i, _) = it.next().unwrap();
                    }
                    if !bytes.get(i + 1).is_some_and(|&c| isdigit(c)) {
                        self.last_breakable = last_nonspace;
                    }
                }
            } else if escaping == Escaping::Literal {
                if bytes[i] == b'\n' {
                    self.write("\n")?;
                } else {
                    let cs = char::to_string(&c);
                    self.write(&cs)?;
                    self.column += cs.len();
                    self.begin_line = false;
                    self.begin_content = self.begin_content && isdigit(bytes[i]);
                }
            } else {
                self.outc(c, escaping, nextb)?;
                self.begin_line = false;
                self.begin_content = self.begin_content && isdigit(bytes[i]);
            }

            if self.options.render.width > 0
                && self.column > self.options.render.width
                && !self.begin_line
                && self.last_breakable > 0
            {
                self.output
                    .write_str(&self.wrap_buffer[..self.last_breakable])?;
                self.output.write_str("\n")?;
                strings::remove_from_start(&mut self.wrap_buffer, self.last_breakable + 1);
                self.wrap_buffer.insert_str(0, &self.prefix);
                self.column = self.wrap_buffer.len();
                self.last_breakable = 0;
                self.begin_line = false;
                self.begin_content = false;
            }
        }
        Ok(())
    }

    fn outc(&mut self, c: char, escaping: Escaping, nextb: Option<&u8>) -> fmt::Result {
        // NOTE: nextb contains the byte *immediately following the first byte of c*.
        // If c is a multibyte character, nextb contains the second byte of c!
        // We rely on it here *only* where we know c to be a single byte
        // character, in which case it faithfully represents the first byte of the
        // following character (or is None if the string ends).
        // Any use of nextb must be conditional on asserting c is a single byte character.
        let follows_digit = self.window.last().is_some_and(|&b| isdigit(b));

        let nextb = nextb.map_or(0, |&c| c);

        let needs_escaping = (c as u64) < 0x80
            && escaping != Escaping::Literal
            && ((escaping == Escaping::Normal
                && ((c as u64) < 0x20
                    || c == '*'
                    || c == '_'
                    || c == '['
                    || c == ']'
                    || c == '#'
                    || c == '<'
                    || c == '>'
                    || c == '\\'
                    || c == '`'
                    || c == '~'
                    || c == '!'
                    || (self.options.extension.autolink && c == '@')
                    || (c == '&' && isalpha(nextb))
                    || (c == '!' && nextb == 0x5b)
                    || (self.begin_content
                        && self.options.extension.block_directive
                        && c == ':')
                    || (self.begin_content
                        && (c == '-' || c == '+' || c == '=')
                        && !follows_digit)
                    || (self.begin_content
                        && (c == '.' || c == ')')
                        && follows_digit
                        && (nextb == 0 || isspace(nextb)))))
                || (escaping == Escaping::Url
                    && (c == '`'
                        || c == '<'
                        || c == '>'
                        || isspace_char(c)
                        || c == '\\'
                        || c == ')'
                        || c == '('))
                || (escaping == Escaping::Title
                    && (c == '`' || c == '<' || c == '>' || c == '"' || c == '\\')));

        if needs_escaping {
            // (c as u64) < 256 is implied.
            let out = if escaping == Escaping::Url && isspace_char(c) {
                format!("%{:2X}", c as u8)
            } else if ispunct_char(c) {
                format!("\\{}", c)
            } else if c == '\0' {
                "\u{fffd}".to_string()
            } else {
                format!("&#{};", c as u8)
            };
            self.write(&out)?;
            self.column += out.len();
        } else {
            let cs = char::to_string(&c);
            self.write(&cs)?;
            self.column += cs.len();
        }

        Ok(())
    }

    fn cr(&mut self) {
        self.need_cr = max(self.need_cr, 1);
    }

    fn blankline(&mut self) {
        self.need_cr = max(self.need_cr, 2);
    }

    fn format(&mut self, root: Node<'a>) -> fmt::Result {
        enum Phase {
            Pre,
            Post,
        }
        let mut stack = vec![(root, Phase::Pre)];

        while let Some((node, phase)) = stack.pop() {
            match phase {
                Phase::Pre => {
                    if self.format_node(node, true)? {
                        stack.push((node, Phase::Post));
                        for ch in node.reverse_children() {
                            stack.push((ch, Phase::Pre));
                        }
                    }
                }
                Phase::Post => {
                    self.format_node(node, false)?;
                }
            }
        }

        if !self.wrap_buffer.is_empty() {
            self.output.write_str(&self.wrap_buffer)?;
        }
        if !self.window.is_empty() && self.window.last() != Some(&b'\n') {
            self.output.write_str("\n")?;
        }
        Ok(())
    }

    fn get_in_tight_list_item(&self, node: Node<'a>) -> bool {
        let tmp = match node.containing_block() {
            Some(tmp) => tmp,
            None => return false,
        };

        match tmp.data().value {
            NodeValue::Item(..) | NodeValue::TaskItem(..) => {
                if let NodeValue::List(ref nl) = tmp.parent().unwrap().data().value {
                    return nl.tight;
                }
                return false;
            }
            _ => {}
        }

        let parent = match tmp.parent() {
            Some(parent) => parent,
            None => return false,
        };

        match parent.data().value {
            NodeValue::Item(..) | NodeValue::TaskItem(..) => {
                if let NodeValue::List(ref nl) = parent.parent().unwrap().data().value {
                    return nl.tight;
                }
            }
            _ => {}
        }

        false
    }

    fn format_node(&mut self, node: Node<'a>, entering: bool) -> Result<bool, fmt::Error> {
        self.node = node;

        let parent_node = node.parent();
        if entering {
            if parent_node.is_some()
                && node_matches!(
                    parent_node.unwrap(),
                    NodeValue::Item(..) | NodeValue::TaskItem(..)
                )
            {
                self.in_tight_list_item = self.get_in_tight_list_item(node);
            }
        } else if node_matches!(node, NodeValue::List(..)) {
            self.in_tight_list_item = parent_node.is_some()
                && node_matches!(
                    parent_node.unwrap(),
                    NodeValue::Item(..) | NodeValue::TaskItem(..)
                )
                && self.get_in_tight_list_item(node);
        }
        let next_is_block = node
            .next_sibling()
            .is_none_or(|next| next.data().value.block());
        let text_in_cell = node_matches!(node, NodeValue::Text(..))
            && parent_node.is_some_and(|n| node_matches!(n, NodeValue::TableCell));

        match node.data().value {
            NodeValue::Document => (),
            NodeValue::FrontMatter(ref fm) => self.format_front_matter(fm, entering)?,
            NodeValue::BlockQuote => self.format_block_quote(entering)?,
            NodeValue::List(..) => self.format_list(node, entering)?,
            NodeValue::Item(..) => self.format_item(node, entering)?,
            NodeValue::DescriptionList => (),
            NodeValue::DescriptionItem(..) => (),
            NodeValue::DescriptionTerm => (),
            NodeValue::DescriptionDetails => self.format_description_details(entering)?,
            NodeValue::Heading(ref nh) => self.format_heading(nh, entering)?,
            NodeValue::CodeBlock(ref ncb) => self.format_code_block(node, ncb, entering)?,
            NodeValue::HtmlBlock(ref nhb) => self.format_html_block(nhb, entering)?,
            #[cfg(feature = "phoenix_heex")]
            NodeValue::HeexBlock(ref nhb) => self.format_heex_block(nhb, entering)?,
            NodeValue::ThematicBreak => self.format_thematic_break(entering)?,
            NodeValue::TableOfContents => {
                if entering {
                    self.blankline();
                    write!(self, "[TOC]")?;
                    self.blankline();
                }
            }
            NodeValue::Paragraph => self.format_paragraph(entering),
            NodeValue::Text(ref literal) => self.format_text(
                literal.owned_str().ok_or(fmt::Error)?,
                node,
                entering,
                !text_in_cell,
            )?,
            NodeValue::LineBreak => self.format_line_break(entering, next_is_block)?,
            NodeValue::SoftBreak => self.format_soft_break(entering)?,
            NodeValue::Code(ref code) => {
                self.format_code(code.literal.owned_str().ok_or(fmt::Error)?, entering)?
            }
            NodeValue::HtmlInline(ref literal) => {
                self.format_html_inline(literal.owned_str().ok_or(fmt::Error)?, entering)?
            }
            #[cfg(feature = "phoenix_heex")]
            NodeValue::HeexInline(ref literal) => self.format_heex_inline(literal, entering)?,
            NodeValue::Raw(ref literal) => self.format_raw(literal, entering)?,
            NodeValue::Strong => {
                if parent_node.is_none() || !node_matches!(parent_node.unwrap(), NodeValue::Strong)
                {
                    self.format_strong()?;
                }
            }
            NodeValue::Emph => self.format_emph(node)?,
            NodeValue::TaskItem(ref nti) => self.format_task_item(nti, node, entering)?,
            NodeValue::TaskCheckbox(marker) => {
                if entering {
                    write!(self, "[{}]", if marker.checked { 'x' } else { ' ' })?;
                }
            }
            NodeValue::Strikethrough => self.format_strikethrough()?,
            NodeValue::Highlight => self.format_highlight()?,
            NodeValue::Insert => self.format_insert()?,
            NodeValue::Superscript => self.format_superscript()?,
            NodeValue::Link(ref nl) => return self.format_link(node, nl, entering),
            NodeValue::Image(ref nl) => self.format_image(nl, entering)?,
            NodeValue::NoteEmbed(ref target) => {
                if entering {
                    write!(self, "![[{}]]", target.url.owned_str().ok_or(fmt::Error)?)?;
                }
            }
            #[cfg(feature = "shortcodes")]
            NodeValue::ShortCode(ref ne) => self.format_shortcode(ne, entering)?,
            NodeValue::Table(..) => self.format_table(entering),
            NodeValue::TableRow(..) => self.format_table_row(entering)?,
            NodeValue::TableCell => self.format_table_cell(node, entering)?,
            NodeValue::FootnoteDefinition(ref nfd) => {
                self.format_footnote_definition(&nfd.name, entering)?
            }
            NodeValue::FootnoteReference(ref nfr) => {
                self.format_footnote_reference(&nfr.name, entering)?
            }
            NodeValue::MultilineBlockQuote(..) => self.format_block_quote(entering)?,
            NodeValue::Escaped => {
                // Noop - the character gets escaped as usual, this is just an
                // AST marker created by escaped_char_spans.
            }
            NodeValue::Math(ref math) => self.format_math(math, entering)?,
            NodeValue::WikiLink(ref nl) => self.format_wikilink(nl, entering)?,
            NodeValue::Underline => self.format_underline()?,
            NodeValue::Subscript => self.format_subscript()?,
            NodeValue::SpoileredText => self.format_spoiler()?,
            NodeValue::EscapedTag(net) => self.format_escaped_tag(net)?,
            NodeValue::Alert(ref alert) => self.format_alert(alert, entering)?,
            NodeValue::Subtext => self.format_subtext(entering)?,
            NodeValue::BlockDirective(ref nbd) => self.format_block_directive(nbd, entering)?,
        };
        Ok(true)
    }

    fn format_front_matter(&mut self, front_matter: &str, entering: bool) -> fmt::Result {
        if entering {
            self.output(front_matter, false, Escaping::Literal)?;
        }
        Ok(())
    }

    fn format_block_quote(&mut self, entering: bool) -> fmt::Result {
        if entering {
            write!(self, "> ")?;
            self.begin_content = true;
            write!(self.prefix, "> ")?;
        } else {
            let new_len = self.prefix.len() - 2;
            self.prefix.truncate(new_len);
            self.blankline();
        }
        Ok(())
    }

    fn format_list(&mut self, node: Node<'a>, entering: bool) -> fmt::Result {
        let ol_start = match node.data().value {
            NodeValue::List(NodeList {
                list_type: ListType::Ordered,
                start,
                ..
            }) => Some(start),
            _ => None,
        };

        if entering {
            if let Some(start) = ol_start {
                self.ol_stack.push(start);
            }
        } else {
            if ol_start.is_some() {
                self.ol_stack.pop();
            }

            if match node.next_sibling() {
                Some(next_sibling) => {
                    node_matches!(next_sibling, NodeValue::CodeBlock(..) | NodeValue::List(..))
                }
                _ => false,
            } {
                self.cr();
                write!(self, "<!-- end list -->")?;
                self.blankline();
            }
        }
        Ok(())
    }

    fn format_item(&mut self, node: Node<'a>, entering: bool) -> fmt::Result {
        let parent = match node.parent().unwrap().data().value {
            NodeValue::List(ref nl) => *nl,
            _ => unreachable!(),
        };

        let mut listmarker = String::new();

        let marker_width = if parent.list_type == ListType::Bullet {
            2
        } else {
            let list_number = if let Some(last_stack) = self.ol_stack.last_mut() {
                let list_number = *last_stack;
                if entering {
                    *last_stack += 1;
                    list_number
                } else {
                    list_number - 1
                }
            } else {
                match node.data().value {
                    NodeValue::Item(ref ni) => ni.start,
                    NodeValue::TaskItem(_) => parent.start,
                    _ => unreachable!(),
                }
            };
            let list_delim = parent.delimiter;
            write!(
                listmarker,
                "{}{} ",
                list_number,
                if list_delim == ListDelimType::Paren {
                    ")"
                } else {
                    "."
                }
            )?;
            let mut current_len = listmarker.len();

            while current_len < self.options.render.ol_width.min(12) {
                write!(listmarker, " ").unwrap();
                current_len += 1;
            }

            listmarker.len()
        };

        if entering {
            if parent.list_type == ListType::Bullet {
                let bullet = char::from(self.options.render.list_style as u8);
                write!(self, "{} ", bullet).unwrap();
            } else {
                self.write_str(&listmarker)?;
            }
            self.begin_content = true;
            for _ in 0..marker_width {
                write!(self.prefix, " ")?;
            }
        } else {
            let new_len = if self.prefix.len() > marker_width {
                self.prefix.len() - marker_width
            } else {
                0
            };
            self.prefix.truncate(new_len);
            self.cr();
        }

        Ok(())
    }

    fn format_description_details(&mut self, entering: bool) -> fmt::Result {
        if entering {
            write!(self, ": ")?
        }
        Ok(())
    }

    fn format_heading(&mut self, nh: &NodeHeading, entering: bool) -> fmt::Result {
        if entering {
            for _ in 0..nh.level {
                write!(self, "#")?;
            }
            write!(self, " ")?;
            self.begin_content = true;
            self.no_linebreaks = true;
        } else {
            self.no_linebreaks = false;
            self.blankline();
        }
        Ok(())
    }

    fn format_code_block(
        &mut self,
        node: Node<'a>,
        ncb: &NodeCodeBlock,
        entering: bool,
    ) -> fmt::Result {
        if !entering {
            return Ok(());
        }

        let first_in_list_item = node.previous_sibling().is_none()
            && match node.parent() {
                Some(parent) => {
                    node_matches!(parent, NodeValue::Item(..) | NodeValue::TaskItem(..))
                }
                _ => false,
            };

        if !first_in_list_item {
            self.blankline();
        }

        let info_text = ncb.info.owned_str().ok_or(fmt::Error)?;
        let literal_text = ncb.literal.owned_str().ok_or(fmt::Error)?;
        let info = info_text.as_bytes();
        let literal = literal_text.as_bytes();

        #[allow(clippy::len_zero)]
        if !(info.len() > 0
            || literal.len() <= 2
            || isspace(literal[0])
            || first_in_list_item
            || self.options.render.prefer_fenced
            || isspace(literal[literal.len() - 1]) && isspace(literal[literal.len() - 2]))
        {
            write!(self, "    ")?;
            write!(self.prefix, "    ")?;
            self.write_str(literal_text)?;
            let new_len = self.prefix.len() - 4;
            self.prefix.truncate(new_len);
        } else {
            let fence_byte = if info.contains(&b'`') { b'~' } else { b'`' };
            let numticks = max(3, longest_byte_sequence(literal, fence_byte) + 1);
            for _ in 0..numticks {
                write!(self, "{}", fence_byte as char)?;
            }
            if !info.is_empty() {
                self.write_str(info_text)?;
            }
            self.cr();
            self.write_str(literal_text)?;
            self.cr();
            for _ in 0..numticks {
                write!(self, "{}", fence_byte as char)?;
            }
        }
        self.blankline();

        Ok(())
    }

    fn format_html_block(&mut self, nhb: &NodeHtmlBlock, entering: bool) -> fmt::Result {
        if entering {
            self.blankline();
            self.write_str(nhb.literal.owned_str().ok_or(fmt::Error)?)?;
            self.blankline();
        }
        Ok(())
    }

    #[cfg(feature = "phoenix_heex")]
    fn format_heex_block(&mut self, nhb: &NodeHeexBlock, entering: bool) -> fmt::Result {
        if entering {
            self.blankline();
            self.write_str(&nhb.literal)?;
            self.blankline();
        }
        Ok(())
    }

    fn format_thematic_break(&mut self, entering: bool) -> fmt::Result {
        if entering {
            self.blankline();
            write!(self, "-----")?;
            self.blankline();
        }
        Ok(())
    }

    fn format_paragraph(&mut self, entering: bool) {
        if !entering {
            self.blankline();
        }
    }

    fn format_text(
        &mut self,
        literal: &str,
        node: Node<'a>,
        entering: bool,
        wrap: bool,
    ) -> fmt::Result {
        if entering {
            // Entity encode a leading or trailing whitespace character if this
            // is the first or last child of an emphasis node respectively,
            // otherwise flankingness rules will cause the emph to not be
            // parsed! (Note that both is possible.)
            let (leading, trailing) = if node
                .parent()
                .is_some_and(|n| is_emphasis_delimiter_node(&n.data().value))
            {
                (
                    if node.previous_sibling().is_none() {
                        literal.chars().next().filter(|c| c.is_whitespace())
                    } else {
                        None
                    },
                    if node.next_sibling().is_none() {
                        literal.chars().next_back().filter(|c| c.is_whitespace())
                    } else {
                        None
                    },
                )
            } else {
                (None, None)
            };

            if let Some(leading) = leading {
                write!(self, "&#{};", leading as u32)?;

                if leading.len_utf8() == literal.len() {
                    // Deleterious case: the emphasis contains a text node which
                    // is a single whitespace character. Then,leading and trailing
                    // are actually the same (only) character, and the string slice
                    // below will fail.
                    // We can just stop!
                    return Ok(());
                }
            }

            self.output(
                &literal[leading.map_or(0, char::len_utf8)
                    ..literal.len() - trailing.map_or(0, char::len_utf8)],
                wrap,
                Escaping::Normal,
            )?;

            if let Some(trailing) = trailing {
                write!(self, "&#{};", trailing as u32)?;
            }
        }
        Ok(())
    }

    fn format_line_break(&mut self, entering: bool, next_is_block: bool) -> fmt::Result {
        if entering {
            if !self.options.render.hardbreaks && !next_is_block {
                // If the next element is a block, a backslash means a
                // literal backslash instead of a line break. In this case
                // we can just skip the line break since it's meaningless
                // before a block.
                write!(self, "\\")?;
            }
            self.cr();
        }
        Ok(())
    }

    fn format_soft_break(&mut self, entering: bool) -> fmt::Result {
        if entering {
            if !self.no_linebreaks
                && self.options.render.width == 0
                && !self.options.render.hardbreaks
            {
                self.cr();
            } else if self.options.render.hardbreaks {
                self.output("\n", true, Escaping::Literal)?;
            } else {
                self.output(" ", true, Escaping::Literal)?;
            }
        }
        Ok(())
    }

    fn format_code(&mut self, literal: &str, entering: bool) -> fmt::Result {
        if entering {
            let literal_bytes = literal.as_bytes();
            let numticks = shortest_unused_sequence(literal_bytes, b'`');
            for _ in 0..numticks {
                write!(self, "`")?;
            }

            let all_space = literal_bytes
                .iter()
                .all(|&c| c == b' ' || c == b'\r' || c == b'\n');
            let has_edge_space =
                literal_bytes[0] == b' ' || literal_bytes[literal_bytes.len() - 1] == b' ';
            let has_edge_backtick =
                literal_bytes[0] == b'`' || literal_bytes[literal_bytes.len() - 1] == b'`';

            let pad = literal.is_empty() || has_edge_backtick || (!all_space && has_edge_space);
            if pad {
                write!(self, " ")?
            }
            self.output(literal, true, Escaping::Literal)?;
            if pad {
                write!(self, " ")?;
            }
            for _ in 0..numticks {
                write!(self, "`")?;
            }
        }

        Ok(())
    }

    fn format_html_inline(&mut self, literal: &str, entering: bool) -> fmt::Result {
        if entering {
            self.write_str(literal)?;
        }
        Ok(())
    }

    #[cfg(feature = "phoenix_heex")]
    fn format_heex_inline(&mut self, literal: &str, entering: bool) -> fmt::Result {
        if entering {
            self.write_str(literal)?;
        }
        Ok(())
    }

    fn format_raw(&mut self, literal: &str, entering: bool) -> fmt::Result {
        if entering {
            self.write_str(literal)?;
        }
        Ok(())
    }

    fn format_strong(&mut self) -> fmt::Result {
        write!(self, "**")
    }

    fn format_emph(&mut self, node: Node<'a>) -> fmt::Result {
        let emph_delim = if match node.parent() {
            Some(parent) => matches!(parent.data().value, NodeValue::Emph),
            _ => false,
        } && node.next_sibling().is_none()
            && node.previous_sibling().is_none()
        {
            "_"
        } else {
            "*"
        };

        self.write_str(emph_delim)?;
        Ok(())
    }

    fn format_task_item(
        &mut self,
        nti: &NodeTaskItem,
        node: Node<'a>,
        entering: bool,
    ) -> fmt::Result {
        if node
            .parent()
            .map(|p| node_matches!(p, NodeValue::List(_)))
            .unwrap_or_default()
        {
            self.format_item(node, entering)?;
        }
        if entering {
            write!(self, "[{}] ", nti.symbol.unwrap_or(' '))?;
        }
        Ok(())
    }

    fn format_strikethrough(&mut self) -> fmt::Result {
        write!(self, "~~")?;
        Ok(())
    }

    fn format_highlight(&mut self) -> fmt::Result {
        write!(self, "==")?;
        Ok(())
    }

    fn format_insert(&mut self) -> fmt::Result {
        write!(self, "++")?;
        Ok(())
    }

    fn format_superscript(&mut self) -> fmt::Result {
        write!(self, "^")?;
        Ok(())
    }

    fn format_underline(&mut self) -> fmt::Result {
        write!(self, "__")?;
        Ok(())
    }

    fn format_subscript(&mut self) -> fmt::Result {
        write!(self, "~")?;
        Ok(())
    }

    fn format_spoiler(&mut self) -> fmt::Result {
        write!(self, "||")?;
        Ok(())
    }

    fn format_escaped_tag(&mut self, net: &str) -> fmt::Result {
        self.output(net, false, Escaping::Literal)
    }

    fn format_link(
        &mut self,
        node: Node<'a>,
        nl: &NodeLink,
        entering: bool,
    ) -> Result<bool, fmt::Error> {
        let url = nl.url.owned_str().ok_or(fmt::Error)?;
        let title = nl.title.owned_str().ok_or(fmt::Error)?;
        if is_autolink(node, nl)? {
            if entering {
                write!(self, "<{}>", trim_start_match(url, "mailto:"))?;
                return Ok(false);
            }
        } else if entering {
            write!(self, "[")?;
        } else {
            write!(self, "](")?;
            self.output(url, false, Escaping::Url)?;
            if !title.is_empty() {
                write!(self, " \"")?;
                self.output(title, false, Escaping::Title)?;
                write!(self, "\"")?;
            }
            write!(self, ")")?;
        }

        Ok(true)
    }

    fn format_wikilink(&mut self, nl: &NodeWikiLink, entering: bool) -> fmt::Result {
        if entering {
            write!(self, "[[")?;
            if self.options.extension.wikilinks() == Some(WikiLinksMode::UrlFirst) {
                self.output(&nl.url, false, Escaping::Url)?;
                write!(self, "|").unwrap();
            }
        } else {
            if self.options.extension.wikilinks() == Some(WikiLinksMode::TitleFirst) {
                write!(self, "|")?;
                self.output(&nl.url, false, Escaping::Url)?;
            }
            write!(self, "]]")?;
        }

        Ok(())
    }

    fn format_image(&mut self, nl: &NodeLink, entering: bool) -> fmt::Result {
        let url = nl.url.owned_str().ok_or(fmt::Error)?;
        let title = nl.title.owned_str().ok_or(fmt::Error)?;
        if entering {
            write!(self, "![")?;
        } else {
            write!(self, "](")?;
            self.output(url, false, Escaping::Url)?;
            if !title.is_empty() {
                self.output(" \"", true, Escaping::Literal)?;
                self.output(title, false, Escaping::Title)?;
                write!(self, "\"")?;
            }
            write!(self, ")")?;
        }
        Ok(())
    }

    #[cfg(feature = "shortcodes")]
    fn format_shortcode(&mut self, ne: &NodeShortCode, entering: bool) -> fmt::Result {
        if entering {
            write!(self, ":")?;
            self.output(&ne.code, false, Escaping::Literal)?;
            write!(self, ":")?;
        }
        Ok(())
    }

    fn format_table(&mut self, entering: bool) {
        if entering {
            self.custom_escape = Some(table_escape);
        } else {
            self.custom_escape = None;
        }
        self.blankline();
    }

    fn format_table_row(&mut self, entering: bool) -> fmt::Result {
        if entering {
            self.cr();
            write!(self, "|")?;
        }
        Ok(())
    }

    fn format_table_cell(&mut self, node: Node<'a>, entering: bool) -> fmt::Result {
        if entering {
            write!(self, " ")?;
        } else {
            write!(self, " |")?;

            let row = &node.parent().unwrap().data().value;
            let in_header = match *row {
                NodeValue::TableRow(header) => header,
                _ => panic!(),
            };

            if in_header && node.next_sibling().is_none() {
                let table = &node.parent().unwrap().parent().unwrap().data().value;
                let alignments = match table {
                    NodeValue::Table(nt) => &nt.alignments,
                    _ => panic!(),
                };

                self.cr();
                write!(self, "|")?;
                for a in alignments {
                    write!(
                        self,
                        " {} |",
                        match *a {
                            TableAlignment::Left => ":--",
                            TableAlignment::Center => ":-:",
                            TableAlignment::Right => "--:",
                            TableAlignment::None => "---",
                        }
                    )?;
                }
                self.cr();
            }
        }
        Ok(())
    }

    fn format_footnote_definition(&mut self, name: &str, entering: bool) -> fmt::Result {
        if entering {
            self.footnote_ix += 1;
            writeln!(self, "[^{}]:", name)?;
            write!(self.prefix, "    ")?;
        } else {
            let new_len = self.prefix.len() - 4;
            self.prefix.truncate(new_len);
        }
        Ok(())
    }

    fn format_footnote_reference(&mut self, r: &str, entering: bool) -> fmt::Result {
        if entering {
            self.write_str("[^")?;
            self.write_str(r)?;
            self.write_str("]")?;
        }
        Ok(())
    }

    fn format_math(&mut self, math: &NodeMath, entering: bool) -> fmt::Result {
        if entering {
            let literal = math.literal.owned_str().ok_or(fmt::Error)?;
            let (start_fence, end_fence) = match (
                math.dollar_math,
                math.display_math,
                self.options.extension.math_latex,
                self.options.extension.math_dollars,
            ) {
                (false, _, _, _) => ("$`", "`$"),
                (true, true, true, false) => ("\\[", "\\]"),
                (true, false, true, false) => ("\\(", "\\)"),
                (true, true, _, _) => ("$$", "$$"),
                (true, false, _, _) => ("$", "$"),
            };

            self.output(start_fence, false, Escaping::Literal)?;
            self.output(literal, true, Escaping::Literal)?;
            self.output(end_fence, false, Escaping::Literal)?;
        }
        Ok(())
    }

    fn format_alert(&mut self, alert: &NodeAlert, entering: bool) -> fmt::Result {
        if entering {
            write!(
                self,
                "> [!{}]",
                alert.alert_type.default_title().to_uppercase()
            )?;
            if let Some(ref title) = alert.title {
                write!(self, " {title}")?;
            }
            writeln!(self)?;
            write!(self, "> ")?;
            self.begin_content = true;
            write!(self.prefix, "> ")?;
        } else {
            let new_len = self.prefix.len() - 2;
            self.prefix.truncate(new_len);
            self.blankline();
        }
        Ok(())
    }

    fn format_subtext(&mut self, entering: bool) -> fmt::Result {
        if entering {
            write!(self, "-# ")?;
            self.begin_content = true;
            self.no_linebreaks = true;
        } else {
            self.no_linebreaks = false;
            self.blankline();
        }
        Ok(())
    }

    fn format_block_directive(&mut self, nbd: &NodeBlockDirective, entering: bool) -> fmt::Result {
        if entering {
            let fence = ":".repeat(nbd.fence_length.max(3));
            if nbd.info.is_empty() {
                writeln!(self, "{fence}")?;
            } else {
                writeln!(self, "{fence}{}", nbd.info)?;
            }
            self.begin_content = true;
        } else {
            let fence = ":".repeat(nbd.fence_length.max(3));
            write!(self, "{fence}")?;
            self.blankline();
        }
        Ok(())
    }
}

fn longest_byte_sequence(buffer: &[u8], ch: u8) -> usize {
    let mut longest = 0;
    let mut current = 0;
    for c in buffer {
        if *c == ch {
            current += 1;
        } else {
            if current > longest {
                longest = current;
            }
            current = 0;
        }
    }
    if current > longest {
        longest = current;
    }
    longest
}

fn shortest_unused_sequence(buffer: &[u8], f: u8) -> usize {
    let mut used = HashSet::<usize>::new();
    let mut current = 0;
    for c in buffer {
        if *c == f {
            current += 1;
        } else {
            if current > 0 {
                used.insert(current);
            }
            current = 0;
        }
    }

    if current > 0 {
        used.insert(current);
    }

    let mut i = 1;
    while used.contains(&i) {
        i += 1;
    }
    i
}

fn is_autolink(node: Node<'_>, nl: &NodeLink) -> Result<bool, fmt::Error> {
    let url = nl.url.owned_str().ok_or(fmt::Error)?;
    let title = nl.title.owned_str().ok_or(fmt::Error)?;
    if url.is_empty() || scanners::scheme(url).is_none() {
        return Ok(false);
    }

    if !title.is_empty() {
        return Ok(false);
    }

    let Some(child) = node.first_child() else {
        return Ok(false);
    };
    let data = child.data();
    let link_text = match &data.value {
        NodeValue::Text(text) => text.owned_str().ok_or(fmt::Error)?,
        _ => return Ok(false),
    };

    Ok(trim_start_match(url, "mailto:") == link_text)
}

fn table_escape(node: Node<'_>, c: char) -> bool {
    match node.data().value {
        NodeValue::Table(..) | NodeValue::TableRow(..) | NodeValue::TableCell => false,
        _ => c == '|',
    }
}

fn minimize_commonmark(text: &mut String, original_options: &Options) {
    let mut options_without = original_options.clone();
    options_without.render.experimental_minimize_commonmark = false;

    let ixs: Vec<usize> = text
        .as_bytes()
        .iter()
        .enumerate()
        .filter_map(|(ix, &c)| if c == b'\\' { Some(ix) } else { None })
        .collect();
    let original = text.clone();

    let mut adjust = 0;
    for ix in ixs {
        text.remove(ix - adjust);

        let arena = Arena::new();
        let root = crate::parse_document(&arena, text, &options_without);

        let mut out = String::new();
        format_document(root, &options_without, &mut out).unwrap();

        if original == out {
            // Removed character is guaranteed to be 1 byte wide, since it's
            // always '\\'.
            adjust += 1;
        } else {
            text.insert(ix - adjust, '\\');
        }
    }
}

/// Escapes the input, rendering it suitable for inclusion in a CommonMark
/// document in a place where regular inline parsing is occurring. Note that
/// this is not minimal --- there will be more escaping backslashes in the
/// output than is strictly necessary. The rendering will not be affected,
/// however.
pub fn escape_inline(text: &str) -> String {
    let mut result = String::with_capacity(text.len() * 3 / 2);

    for c in text.chars() {
        if c < '\x20'
            || c == '*'
            || c == '_'
            || c == '['
            || c == ']'
            || c == '#'
            || c == '<'
            || c == '>'
            || c == '\\'
            || c == '`'
            || c == '~'
            || c == '!'
            || c == '&'
            || c == '!'
            || c == '-'
            || c == '+'
            || c == '='
            || c == '.'
            || c == '('
            || c == ')'
            || c == '"'
        {
            if ispunct(c as u8) {
                write!(&mut result, "\\{}", c).unwrap();
            } else {
                write!(&mut result, "&#{};", c as u8).unwrap();
            }
        } else {
            result.push(c);
        }
    }

    result
}

/// Escapes the input URL, rendering it suitable for inclusion as a [link
/// destination] per the CommonMark spec.
///
/// [link destination]: https://spec.commonmark.org/0.31.2/#link-destination
pub fn escape_link_destination(url: &str) -> String {
    let mut result = String::with_capacity(url.len() * 3 / 2);

    result.push('<');
    for c in url.chars() {
        match c {
            '<' | '>' => {
                result.push('\\');
                result.push(c);
            }
            '\n' => result.push_str("%0A"),
            '\r' => result.push_str("%0D"),
            _ => result.push(c),
        }
    }
    result.push('>');

    result
}

#[inline]
fn is_emphasis_delimiter_node(value: &NodeValue) -> bool {
    matches!(
        value,
        NodeValue::Emph
            | NodeValue::Strong
            | NodeValue::Strikethrough
            | NodeValue::Highlight
            | NodeValue::Superscript
            | NodeValue::Subscript
            | NodeValue::SpoileredText
            | NodeValue::Underline
    )
}
