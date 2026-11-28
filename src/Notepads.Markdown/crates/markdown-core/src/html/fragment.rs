// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE file in the project root for license information.
// ---------------------------------------------------------------------------------------------

//! Lower a restricted HTML fragment to the native Markdown reader's syntax.
//! Only recognized tags and attributes can produce reader syntax. Everything
//! else either becomes escaped text or leaves the original block inert.

use super::controls::StaticCheckbox;
use super::ruby::{self, DisplayUnit};
use crate::presentation::{
    RUN_BOLD, RUN_CODE, RUN_HIGHLIGHT, RUN_ITALIC, RUN_MONOSPACE, RUN_STRIKE, RUN_UNDERLINE,
};
use lol_html::{HtmlRewriter, MemorySettings, Settings, doc_comments, doc_text, element, end_tag};
use std::cell::RefCell;
use std::ops::Range;
use std::rc::Rc;

pub(super) const MAX_FRAGMENT_BYTES: usize = 64 * 1024;
const MAX_DEPTH: usize = 32;
pub(super) const MAX_PARTS: usize = 4096;
const MAX_IMAGE_DIMENSION: u32 = 8192;

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub(super) struct ImageSize {
    pub width: u32,
    pub height: u32,
}

#[derive(Debug)]
pub(super) struct Part {
    pub source: Range<usize>,
    pub output: Range<usize>,
}

#[derive(Debug)]
pub(super) struct Destination {
    pub link_start: usize,
    pub source: Range<usize>,
    pub output: Range<usize>,
    pub exact: bool,
}

impl Destination {
    fn attribute(
        el: &lol_html::html_content::Element<'_, '_>,
        name: &str,
        value: &str,
        link_start: usize,
    ) -> Option<Self> {
        let attribute = el
            .attributes()
            .iter()
            .find(|attribute| attribute.name() == name)?;
        Some(Self {
            link_start,
            source: attribute.value_source_location()?.bytes(),
            output: 0..0,
            exact: attribute.value() == value[1..value.len() - 1],
        })
    }
}

#[derive(Debug, Default)]
pub(super) struct Fragment {
    pub markdown: String,
    pub parts: Vec<Part>,
    pub alignments: Vec<(Range<usize>, u32)>,
    pub images: Vec<(Range<usize>, ImageSize)>,
    pub styles: Vec<(Range<u64>, u32)>,
    pub ruby: Vec<DisplayUnit>,
    pub pictures: Vec<super::picture::Picture>,
    pub destinations: Vec<Destination>,
    pub anchors: Vec<Anchor>,
    pub static_checkboxes: Vec<StaticCheckbox>,
    pub disclosures: Vec<super::details::Projected>,
    pub html_table: Option<super::table::Table>,
    pub citation: bool,
    pub suppress_email_autolinks: bool,
    // Populated only by inline-only lowering, independently of GFM syntax.
    pub inline_text: String,
}

#[derive(Debug)]
pub(super) struct Anchor {
    pub name: String,
    pub offset: usize,
    pub legacy: bool,
    pub kind: AnchorKind,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum AnchorKind {
    Text,
    Heading,
    Rule,
    Table,
}

pub(super) fn anchor_attribute(
    element: &lol_html::html_content::Element<'_, '_>,
    attribute: &str,
) -> Option<String> {
    let raw = element.get_attribute(attribute)?;
    let value = htmlize::unescape_attribute(raw).into_owned();
    if value.is_empty()
        || value.contains('\0')
        || (attribute == "id" && value.bytes().any(crate::ir::inline_html_tag_space))
    {
        return None;
    }
    Some(value)
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum Tag {
    Container,
    Details,
    Summary,
    Paragraph,
    Quote,
    Heading(u8),
    Inline,
    Strong,
    Emphasis,
    Strike,
    Highlight,
    Ruby,
    RubyAnnotation,
    RubyFallback,
    Underline,
    Monospace,
    Code,
    Link,
    Image,
    Picture,
    PictureSource,
    Break,
    Rule,
    Label,
    Input,
}

impl Tag {
    fn parse(name: &str) -> Option<Self> {
        Some(match name {
            "div" | "section" | "article" | "header" | "footer" | "main" | "figure" | "center" => {
                Self::Container
            }
            "p" | "figcaption" => Self::Paragraph,
            "blockquote" => Self::Quote,
            "details" => Self::Details,
            "summary" => Self::Summary,
            "h1" => Self::Heading(1),
            "h2" => Self::Heading(2),
            "h3" => Self::Heading(3),
            "h4" => Self::Heading(4),
            "h5" => Self::Heading(5),
            "h6" => Self::Heading(6),
            "span" => Self::Inline,
            "b" | "strong" => Self::Strong,
            "i" | "em" | "var" | "cite" | "dfn" => Self::Emphasis,
            "del" | "s" | "strike" => Self::Strike,
            "mark" => Self::Highlight,
            "ruby" => Self::Ruby,
            "rt" => Self::RubyAnnotation,
            "rp" => Self::RubyFallback,
            "ins" => Self::Underline,
            "tt" => Self::Monospace,
            "code" | "kbd" | "samp" => Self::Code,
            "a" => Self::Link,
            "img" => Self::Image,
            "picture" => Self::Picture,
            "source" => Self::PictureSource,
            "br" => Self::Break,
            "hr" => Self::Rule,
            "label" => Self::Label,
            "input" => Self::Input,
            _ => return None,
        })
    }

    fn is_block(self) -> bool {
        matches!(
            self,
            Self::Container
                | Self::Details
                | Self::Summary
                | Self::Paragraph
                | Self::Quote
                | Self::Heading(_)
                | Self::Rule
        )
    }

    fn style(self) -> u32 {
        match self {
            Self::Strong => RUN_BOLD,
            Self::Emphasis => RUN_ITALIC,
            Self::Strike => RUN_STRIKE,
            Self::Highlight => RUN_HIGHLIGHT,
            Self::Underline => RUN_UNDERLINE,
            Self::Monospace => RUN_MONOSPACE,
            Self::Code => RUN_CODE,
            _ => 0,
        }
    }
}

fn alignment(value: &str) -> Option<u32> {
    match value.trim().to_ascii_lowercase().as_str() {
        "left" => Some(1),
        "center" => Some(2),
        "right" => Some(3),
        _ => None,
    }
}

fn dimension(value: &str) -> u32 {
    value
        .trim()
        .parse::<u32>()
        .ok()
        .filter(|&n| n > 0 && n <= MAX_IMAGE_DIMENSION)
        .unwrap_or_default()
}

fn relative_path(value: &str) -> bool {
    let path = value.split(['?', '#']).next().unwrap_or_default();
    let mut bytes = path.as_bytes();
    let mut leading = true;
    while let Some((&byte, rest)) = bytes.split_first() {
        bytes = rest;
        let decoded = if byte == b'%' && rest.len() >= 2 {
            std::str::from_utf8(&rest[..2])
                .ok()
                .and_then(|hex| u8::from_str_radix(hex, 16).ok())
        } else {
            None
        };
        let byte = if let Some(decoded) = decoded {
            bytes = &rest[2..];
            decoded
        } else {
            byte
        };
        // The image provider unescapes paths before asking Windows to open
        // them. Apply the same guard to encoded drive and UNC spellings.
        if matches!(byte, b':' | b'\\') || byte.is_ascii_control() || (leading && byte == b'/') {
            return false;
        }
        leading &= byte == b' ';
    }
    true
}

pub(super) fn safe_destination(value: &str, link: bool) -> Option<&str> {
    let value = value.trim_matches(|c: char| c.is_ascii_whitespace());
    if value.is_empty()
        || value.starts_with("//")
        || value.chars().any(|c| c.is_control() || c == '\\')
    {
        return None;
    }
    let end = value.find(['/', '?', '#']).unwrap_or(value.len());
    let allowed = if let Some((scheme, _)) = value[..end].split_once(':') {
        scheme.eq_ignore_ascii_case("http")
            || scheme.eq_ignore_ascii_case("https")
            || (link && scheme.eq_ignore_ascii_case("mailto"))
    } else {
        relative_path(value)
    };
    allowed.then_some(value)
}

fn destination(value: &str, link: bool) -> Option<String> {
    let value = safe_destination(value, link)?;
    let mut escaped = String::with_capacity(value.len() + 2);
    escaped.push('<');
    for c in value.chars() {
        match c {
            '<' => escaped.push_str("&lt;"),
            '>' => escaped.push_str("&gt;"),
            '&' => escaped.push_str("&amp;"),
            ' ' => escaped.push_str("&#32;"),
            '|' => escaped.push_str("&#124;"),
            _ => escaped.push(c),
        }
    }
    escaped.push('>');
    Some(escaped)
}

fn escape_text(value: &str, protect_start: bool) -> String {
    use std::fmt::Write;
    let mut output = String::with_capacity(value.len());
    for (index, c) in value.chars().enumerate() {
        if html_space(c) {
            output.push_str("&#32;");
        } else if c == '\0' {
            output.push('\u{fffd}');
        } else if c.is_ascii_punctuation()
            || (protect_start && index == 0 && c.is_ascii_alphanumeric())
        {
            // Entities are parsed as text even when adjacent to other syntax.
            write!(output, "&#{};", u32::from(c)).expect("writing to String");
        } else {
            output.push(c);
        }
    }
    output
}

fn html_space(c: char) -> bool {
    matches!(c, '\t' | '\n' | '\x0c' | '\r' | ' ')
}

struct Frame {
    start: usize,
    tag: Tag,
    align: u32,
    style: u32,
    closing: String,
    destination: Option<usize>,
    disclosure: Option<DisclosureFrame>,
}

#[derive(Clone, Copy)]
enum DisclosureFrame {
    Details(usize),
    Summary(usize),
}

struct Label {
    input: bool,
    closed: bool,
    // Account for every original token, including end tags the HTML parser
    // otherwise ignores. A repaired/mismatched label must remain literal.
    coverage: Vec<Range<usize>>,
}

#[derive(Default)]
struct Renderer {
    output: Fragment,
    stack: Vec<Frame>,
    block_start: usize,
    block_align: u32,
    space: Option<Range<usize>>,
    has_content: bool,
    last_space: bool,
    recognized: bool,
    label: Option<Label>,
    ruby_close: Option<Range<usize>>,
    picture: Option<super::picture::Pending>,
    inline_only: bool,
    inline_coverage: Vec<Range<usize>>,
}

impl Renderer {
    fn details_parent(&self) -> Option<usize> {
        match self.stack.last()?.disclosure? {
            DisclosureFrame::Details(index) => Some(index),
            DisclosureFrame::Summary(_) => None,
        }
    }

    fn default_details_label(&mut self, index: usize, at: usize) -> lol_html::HandlerResult {
        if self.output.disclosures[index].summary.is_none() {
            let start = self.output.markdown.len();
            self.emit(at..at, "Details")?;
            self.output.disclosures[index].summary = Some((start, self.output.markdown.len()));
            self.end_block(at)?;
        }
        Ok(())
    }

    fn cover_label(&mut self, span: Range<usize>) -> lol_html::HandlerResult {
        if self.inline_only && !span.is_empty() {
            if self.inline_coverage.len() >= MAX_PARTS {
                return Err("HTML inline fragment is too complex".into());
            }
            self.inline_coverage.push(span.clone());
        }
        if let Some(label) = &mut self.label {
            if label.coverage.len() >= MAX_PARTS {
                return Err("HTML checkbox label is too complex".into());
            }
            if !span.is_empty() {
                label.coverage.push(span);
            }
        }
        Ok(())
    }

    fn comment(&mut self, span: Range<usize>) -> lol_html::HandlerResult {
        if self.label.as_ref().is_some_and(|label| label.closed) {
            return Err("Content after HTML checkbox label".into());
        }
        self.cover_label(span)?;
        self.recognized = true;
        Ok(())
    }

    fn emit(&mut self, source: Range<usize>, text: &str) -> lol_html::HandlerResult {
        if self.output.parts.len() >= MAX_PARTS
            || self.output.markdown.len() + text.len() > 6 * MAX_FRAGMENT_BYTES
        {
            return Err("HTML preview fragment is too complex".into());
        }
        let start = self.output.markdown.len();
        self.output.markdown.push_str(text);
        self.output.parts.push(Part {
            source,
            output: start..self.output.markdown.len(),
        });
        Ok(())
    }

    fn emit_text(&mut self, source: Range<usize>, text: &str) -> lol_html::HandlerResult {
        let start = self.output.markdown.len() as u64;
        self.emit(source, text)?;
        if self.inline_only {
            self.output.inline_text.push_str(&htmlize::unescape(text));
        }
        let end = self.output.markdown.len() as u64;
        let style = self.stack.last().map_or(0, |frame| frame.style);
        if style != 0 && start < end {
            if let Some((last, flags)) = self.output.styles.last_mut()
                && *flags == style
                && last.end == start
            {
                last.end = end;
            } else {
                self.output.styles.push((start..end, style));
            }
        }
        Ok(())
    }

    fn flush_space(&mut self) -> lol_html::HandlerResult {
        if let Some(span) = self.space.take()
            && self.has_content
            && !self.last_space
        {
            self.emit_text(span, " ")?;
            self.last_space = true;
        }
        Ok(())
    }

    fn word(
        &mut self,
        start: usize,
        mut raw: &str,
        mut protect_start: bool,
    ) -> lol_html::HandlerResult {
        let mut at = start;
        while !raw.is_empty() {
            if protect_start && raw.as_bytes()[0].is_ascii_alphanumeric() {
                self.visible_text(at..at + 1, &escape_text(&raw[..1], true))?;
                at += 1;
                raw = &raw[1..];
                protect_start = false;
                continue;
            }
            let ordinary = raw
                .find(|c: char| c.is_ascii_punctuation() || c == '\0')
                .unwrap_or(raw.len());
            if ordinary != 0 {
                self.visible_text(at..at + ordinary, &raw[..ordinary])?;
                at += ordinary;
                raw = &raw[ordinary..];
                protect_start = false;
                continue;
            }
            if let Some(entity) = raw.strip_prefix('&') {
                let end = 1 + entity
                    .find(|c: char| !c.is_ascii_alphanumeric() && c != '#')
                    .unwrap_or(entity.len());
                let end = end + usize::from(raw.as_bytes().get(end) == Some(&b';'));
                let decoded = htmlize::unescape(&raw[..end]);
                if decoded != raw[..end] {
                    if decoded.chars().all(html_space) {
                        self.space.get_or_insert(at..at + end);
                    } else {
                        self.visible_text(at..at + end, &escape_text(&decoded, protect_start))?;
                    }
                    at += end;
                    raw = &raw[end..];
                    protect_start = false;
                    continue;
                }
            }
            self.visible_text(at..at + 1, &escape_text(&raw[..1], false))?;
            at += 1;
            raw = &raw[1..];
            protect_start = false;
        }
        Ok(())
    }

    fn visible_text(&mut self, source: Range<usize>, text: &str) -> lol_html::HandlerResult {
        self.flush_space()?;
        self.emit_text(source, text)?;
        self.has_content = true;
        self.last_space = false;
        Ok(())
    }

    fn text(&mut self, span: Range<usize>, raw: &str) -> lol_html::HandlerResult {
        if self.picture.is_some() {
            if raw.chars().all(html_space) {
                return self.cover_label(span);
            }
            return Err("Unexpected text inside HTML picture".into());
        }
        self.cover_label(span.clone())?;
        if let Some(label) = &self.label
            && (label.closed || !label.input)
            && !raw.chars().all(html_space)
        {
            return Err("Text outside HTML checkbox label body".into());
        }
        self.text_content(span, raw, false)
    }

    fn text_content(
        &mut self,
        span: Range<usize>,
        raw: &str,
        protect_start: bool,
    ) -> lol_html::HandlerResult {
        let leading = raw.len() - raw.trim_start_matches(html_space).len();
        let trimmed = raw.trim_matches(html_space);
        if trimmed.is_empty() {
            if !span.is_empty() {
                self.space.get_or_insert(span);
            }
            return Ok(());
        }
        if let Some(index) = self.details_parent() {
            self.default_details_label(index, span.start)?;
        }
        if leading > 0 {
            self.space.get_or_insert(span.start..span.start + leading);
        }
        self.flush_space()?;
        // Keep exact mappings for ordinary text and for each unchanged word.
        let mut start = 0;
        for (end, c) in trimmed.char_indices().filter(|(_, c)| html_space(*c)) {
            if start != end {
                self.word(
                    span.start + leading + start,
                    &trimmed[start..end],
                    protect_start && start == 0,
                )?;
            }
            let at = span.start + leading + end;
            self.space.get_or_insert(at..at + c.len_utf8());
            start = end + c.len_utf8();
            if trimmed[start..]
                .chars()
                .next()
                .is_some_and(|c| !html_space(c))
            {
                self.flush_space()?;
            }
        }
        self.word(
            span.start + leading + start,
            &trimmed[start..],
            protect_start && start == 0,
        )?;
        if leading + trimmed.len() < raw.len() {
            self.space = Some(span.start + leading + trimmed.len()..span.end);
        }
        Ok(())
    }

    fn open_ruby(&mut self, source: &str, start: usize) -> lol_html::HandlerResult {
        if self.stack.iter().any(|frame| frame.tag == Tag::Code)
            || self
                .label
                .as_ref()
                .is_some_and(|label| label.closed || !label.input)
        {
            return Err("Unsupported HTML ruby context".into());
        }
        let parsed = ruby::parse_at(source, start).ok_or("Unsupported HTML ruby group")?;
        self.cover_label(start..parsed.closing.end)?;
        if let Some(index) = self.details_parent() {
            self.default_details_label(index, start)?;
        }
        self.flush_space()?;
        self.recognized = true;
        for unit in &parsed.units {
            let start = self.output.markdown.len() as u64;
            // Keep Markdown's inline lexer from joining projected ruby text
            // and adjacent source into an emoji shortcode or other token.
            self.text_content(unit.base.clone(), &source[unit.base.clone()], true)?;
            let base_end = self.output.markdown.len() as u64;
            self.text_content(
                unit.annotation.clone(),
                &source[unit.annotation.clone()],
                true,
            )?;
            let end = self.output.markdown.len() as u64;
            if !(start < base_end && base_end < end) || self.output.ruby.len() >= MAX_PARTS {
                return Err("Invalid HTML ruby text".into());
            }
            self.output.ruby.push(DisplayUnit {
                start,
                base_end,
                end,
            });
        }
        self.ruby_close = Some(parsed.closing);
        Ok(())
    }

    fn end_block(&mut self, at: usize) -> lol_html::HandlerResult {
        self.space = None;
        let end = self.output.markdown.len();
        if end > self.block_start {
            if self.block_align != 0 {
                self.output
                    .alignments
                    .push((self.block_start..end, self.block_align));
            }
            self.emit(at..at, "\n\n")?;
        }
        self.has_content = false;
        self.last_space = false;
        self.block_start = self.output.markdown.len();
        Ok(())
    }

    fn open(
        &mut self,
        el: &lol_html::html_content::Element<'_, '_>,
        tag: Tag,
    ) -> lol_html::HandlerResult {
        if self.inline_only && (tag.is_block() || matches!(tag, Tag::Label | Tag::Input)) {
            return Err("Unsupported HTML table inline element".into());
        }
        let span = el.source_location().bytes();
        if tag == Tag::Label {
            if self.recognized
                || !self.stack.is_empty()
                || !self.output.markdown.is_empty()
                || el.has_attribute("for")
            {
                return Err("HTML checkbox label is not standalone".into());
            }
            self.label = Some(Label {
                input: false,
                closed: false,
                coverage: Vec::new(),
            });
            self.space = None;
        } else if self.label.as_ref().is_some_and(|label| label.closed) {
            return Err("Element after HTML checkbox label".into());
        }
        self.cover_label(span.clone())?;
        self.recognized = true;
        let attr = |name| {
            el.get_attribute(name)
                .map(|value| htmlize::unescape_attribute(value).into_owned())
        };
        if tag == Tag::PictureSource {
            let picture = self.picture.as_mut().ok_or("Unowned HTML picture source")?;
            if !picture.saw_image {
                picture.candidate(el);
            }
            return Ok(());
        }
        if tag == Tag::Picture {
            self.picture = Some(super::picture::Pending::default());
        }
        if tag == Tag::Input {
            let valid_parent = self
                .stack
                .last()
                .is_some_and(|frame| frame.tag == Tag::Label);
            let first = self
                .label
                .as_ref()
                .is_some_and(|label| !label.input && !label.closed);
            if !valid_parent
                || !first
                || !self.output.markdown.is_empty()
                || !el.has_attribute("disabled")
                || !attr("type").is_some_and(|value| value.eq_ignore_ascii_case("checkbox"))
            {
                return Err("Unsupported HTML checkbox input".into());
            }
            let checked = el.has_attribute("checked");
            self.space = None;
            self.emit(span.start..span.start, "- ")?;
            let start = self.output.markdown.len();
            self.emit(span.clone(), if checked { "[x]" } else { "[ ]" })?;
            self.output.static_checkboxes.push(StaticCheckbox {
                marker: start..start + 3,
                source: span.clone(),
                checked,
                object: None,
                atom: None,
                logical: None,
            });
            self.emit(span.end..span.end, " ")?;
            self.label.as_mut().unwrap().input = true;
            self.has_content = true;
            self.last_space = true;
            return Ok(());
        }
        if self.label.is_some()
            && tag != Tag::Label
            && (tag.is_block() || !self.label.as_ref().unwrap().input)
        {
            return Err("Unsupported HTML checkbox label content".into());
        }
        let mut disclosure = None;
        if let Some(parent) = self.details_parent() {
            if tag == Tag::Summary && self.output.disclosures[parent].summary.is_none() {
                disclosure = Some(DisclosureFrame::Summary(parent));
            } else {
                self.default_details_label(parent, span.start)?;
            }
        }
        if tag == Tag::Details {
            let index = self.output.disclosures.len();
            if index >= super::details::MAX_DISCLOSURES {
                return Err("HTML disclosure limit".into());
            }
            self.output.disclosures.push(super::details::Projected {
                initially_expanded: el.has_attribute("open"),
                ..super::details::Projected::default()
            });
            disclosure = Some(DisclosureFrame::Details(index));
        }
        let inherited = self.stack.last().map_or(0, |frame| frame.align);
        let align = if el.tag_name() == "center" {
            2
        } else if tag.is_block() {
            attr("align")
                .as_deref()
                .and_then(alignment)
                .unwrap_or(inherited)
        } else {
            inherited
        };
        if tag.is_block() {
            if self
                .stack
                .last()
                .is_some_and(|frame| !matches!(frame.tag, Tag::Container | Tag::Details))
            {
                return Err("HTML preview block nested inside inline content".into());
            }
            self.end_block(span.start)?;
            self.block_align = align;
        } else if tag == Tag::Break {
            self.space = None;
        } else {
            self.flush_space()?;
        }
        let offset = self.output.markdown.len();
        for (attribute, legacy) in [("id", false), ("name", true)] {
            if legacy && tag != Tag::Link {
                continue;
            }
            if let Some(name) = anchor_attribute(el, attribute) {
                self.output.anchors.push(Anchor {
                    name,
                    offset,
                    legacy,
                    kind: match tag {
                        Tag::Heading(_) => AnchorKind::Heading,
                        Tag::Rule => AnchorKind::Rule,
                        _ => AnchorKind::Text,
                    },
                });
            }
        }
        let mut closing = String::new();
        let mut destination_origin = None;
        match tag {
            Tag::Heading(level) => {
                self.emit(span.clone(), &format!("{} ", "#".repeat(level.into())))?
            }
            Tag::Quote => self.emit(span.clone(), "> ")?,
            Tag::Rule => {
                self.emit(span.clone(), "---")?;
                self.has_content = true;
                return self.end_block(span.end);
            }
            Tag::Break => {
                // Keep the break in the same native text block, including
                // headings and terminal breaks.
                self.emit(span, "&#10;")?;
                if self.inline_only {
                    self.output.inline_text.push('\n');
                }
                self.has_content = true;
                self.last_space = true;
                return Ok(());
            }
            Tag::Image => {
                if let Some(picture) = &mut self.picture {
                    if picture.saw_image {
                        return Err("Multiple fallback images in an HTML picture".into());
                    }
                    picture.saw_image = true;
                }
                let before = self.output.markdown.len();
                let alt = escape_text(
                    &attr("alt")
                        .unwrap_or_default()
                        .split(html_space)
                        .filter(|s| !s.is_empty())
                        .collect::<Vec<_>>()
                        .join(" "),
                    false,
                );
                if let Some(src) = attr("src")
                    .as_deref()
                    .and_then(|value| destination(value, false))
                {
                    let start = self.output.markdown.len();
                    let width = attr("width").as_deref().map_or(0, dimension);
                    let height = attr("height").as_deref().map_or(0, dimension);
                    let mut origin = Destination::attribute(el, "src", &src, start)
                        .ok_or("Missing HTML image attribute source")?;
                    let destination_start = start + alt.len() + 5;
                    origin.output = destination_start..destination_start + src.len() - 2;
                    self.emit(span, &format!("![{alt}]({src})"))?;
                    if self.inline_only {
                        self.output.inline_text.push('\u{fffc}');
                    }
                    self.output.destinations.push(origin);
                    if let Some(picture) = &mut self.picture {
                        picture.image = Some(start..self.output.markdown.len());
                    }
                    if width != 0 || height != 0 {
                        self.output.images.push((
                            start..self.output.markdown.len(),
                            ImageSize { width, height },
                        ));
                    }
                } else {
                    self.emit_text(span, &alt)?;
                }
                if self.output.markdown.len() > before {
                    self.has_content = true;
                    self.last_space = false;
                }
                return Ok(());
            }
            Tag::Link => {
                if self.stack.iter().any(|frame| frame.tag == Tag::Link) {
                    return Err("Nested HTML links".into());
                }
                if let Some(href) = attr("href")
                    .as_deref()
                    .and_then(|value| destination(value, true))
                {
                    let origin =
                        Destination::attribute(el, "href", &href, self.output.markdown.len())
                            .ok_or("Missing HTML link attribute source")?;
                    self.emit(span.clone(), "[")?;
                    // Record before nested images so syntax starts stay sorted.
                    destination_origin = Some(self.output.destinations.len());
                    self.output.destinations.push(origin);
                    closing = format!("]({href})");
                }
            }
            _ => {}
        }
        if self.stack.len() >= MAX_DEPTH {
            return Err("HTML preview nesting limit".into());
        }
        let style = self.stack.last().map_or(0, |frame| frame.style) | tag.style();
        self.stack.push(Frame {
            start: span.start,
            tag,
            align,
            style,
            closing,
            destination: destination_origin,
            disclosure,
        });
        Ok(())
    }

    fn close(&mut self, start: usize, span: Range<usize>) -> lol_html::HandlerResult {
        let frame = self.stack.pop().ok_or("Unexpected HTML preview end tag")?;
        if frame.start != start {
            return Err("Unbalanced HTML preview block".into());
        }
        self.cover_label(span.clone())?;
        if frame.tag == Tag::Picture {
            let picture = self.picture.take().ok_or("Missing HTML picture")?;
            if !picture.saw_image {
                return Err("HTML picture has no fallback image".into());
            }
            if let Some(picture) = picture.finish() {
                if self.output.pictures.len() >= super::picture::MAX_PICTURES {
                    return Err("HTML picture limit".into());
                }
                self.output.pictures.push(picture);
            }
        }
        match frame.disclosure {
            Some(DisclosureFrame::Summary(index)) => {
                let end = self.output.markdown.len();
                if self.block_start < end {
                    self.output.disclosures[index].summary = Some((self.block_start, end));
                } else {
                    self.default_details_label(index, span.start)?;
                }
            }
            Some(DisclosureFrame::Details(index)) => {
                self.default_details_label(index, span.start)?;
                self.end_block(span.start)?;
                self.output.disclosures[index].close = Some(self.output.markdown.len());
            }
            None => {}
        }
        if frame.tag == Tag::Label {
            let label = self.label.as_mut().ok_or("Missing HTML checkbox label")?;
            if !label.input || label.closed || !self.stack.is_empty() {
                return Err("Incomplete HTML checkbox label".into());
            }
            label.closed = true;
            self.end_block(span.start)?;
        } else if frame.tag.is_block() {
            self.end_block(span.start)?;
            self.block_align = self.stack.last().map_or(0, |frame| frame.align);
        } else if !frame.closing.is_empty() {
            // Trailing whitespace belongs outside Markdown delimiters.
            self.flush_space()?;
            if let Some(index) = frame.destination {
                let start = self.output.markdown.len();
                self.output.destinations[index].output = start + 3..start + frame.closing.len() - 2;
            }
            self.emit(span, &frame.closing)?;
        }
        Ok(())
    }
}

pub(super) fn lower(source: &str, allow_plain_text: bool) -> Option<Fragment> {
    lower_impl(source, false, allow_plain_text, false)
}

pub(super) fn lower_inline(source: &str) -> Option<Fragment> {
    lower_impl(source, true, true, false)
}

pub(super) fn lower_summary(source: &str, allow_plain_text: bool) -> Option<Fragment> {
    lower_impl(source, false, allow_plain_text, true)
}

pub(super) fn safe_inline(name: &str) -> bool {
    Tag::parse(name).is_some_and(|tag| !tag.is_block() && !matches!(tag, Tag::Label | Tag::Input))
}

fn lower_impl(
    source: &str,
    inline_only: bool,
    allow_plain_text: bool,
    standalone_summary: bool,
) -> Option<Fragment> {
    if source.len() > MAX_FRAGMENT_BYTES {
        return None;
    }
    let renderer = Rc::new(RefCell::new(Renderer {
        inline_only,
        ..Renderer::default()
    }));
    let elements = renderer.clone();
    let text = renderer.clone();
    let comments = renderer.clone();
    let settings = Settings::new()
        .with_memory_settings(MemorySettings::new().with_max_allowed_memory_usage(2 * 1024 * 1024))
        .append_document_content_handler(doc_text!(move |chunk| {
            let span = chunk.source_location().bytes();
            let raw = source
                .get(span.clone())
                .ok_or("Invalid HTML preview text span")?;
            let mut renderer = text.borrow_mut();
            if let Some(close) = &renderer.ruby_close {
                if span.end > close.start {
                    return Err("HTML ruby text passed its authored delimiter".into());
                }
                return Ok(());
            }
            renderer.text(span, raw)
        }))
        .append_document_content_handler(doc_comments!(move |comment| {
            comments
                .borrow_mut()
                .comment(comment.source_location().bytes())
        }))
        .append_element_content_handler(element!("*", move |el| {
            if el.namespace_uri() != "http://www.w3.org/1999/xhtml" {
                return Err("Foreign HTML preview element".into());
            }
            let name = el.tag_name();
            let span = el.source_location().bytes();
            if standalone_summary {
                let root = elements.borrow().stack.is_empty();
                if root && (span.start != 0 || name != "summary")
                    || !root && Tag::parse(&name).is_some_and(Tag::is_block)
                {
                    return Err("HTML disclosure label must be a single summary".into());
                }
            }
            if let Some(close) = &elements.borrow().ruby_close {
                if span.end > close.start || !matches!(name.as_str(), "rp" | "rt") {
                    return Err("Unexpected element inside HTML ruby".into());
                }
                return Ok(());
            }
            let tag = Tag::parse(&name).ok_or("Unsupported HTML preview element")?;
            if elements.borrow().picture.is_some()
                && !matches!(tag, Tag::Image | Tag::PictureSource)
            {
                return Err("Unsupported HTML picture child".into());
            }
            if tag == Tag::Ruby {
                elements.borrow_mut().open_ruby(source, span.start)?;
                let elements = elements.clone();
                el.on_end_tag(end_tag!(move |end| {
                    let mut renderer = elements.borrow_mut();
                    if !end.name().eq_ignore_ascii_case("ruby")
                        || renderer.ruby_close.take() != Some(end.source_location().bytes())
                    {
                        return Err("Mismatched HTML ruby end tag".into());
                    }
                    Ok(())
                }))?;
                return Ok(());
            }
            if matches!(tag, Tag::RubyAnnotation | Tag::RubyFallback) {
                return Err("Unowned HTML ruby annotation".into());
            }
            elements.borrow_mut().open(el, tag)?;
            if el.can_have_content() {
                let start = el.source_location().bytes().start;
                let source_end = source.len();
                let elements = elements.clone();
                el.on_end_tag(end_tag!(move |end| {
                    if !end.name().eq_ignore_ascii_case(&name) {
                        return Err("Mismatched HTML preview end tag".into());
                    }
                    if standalone_summary
                        && start == 0
                        && end.source_location().bytes().end != source_end
                    {
                        return Err("Content after HTML disclosure summary".into());
                    }
                    elements
                        .borrow_mut()
                        .close(start, end.source_location().bytes())
                }))?;
            }
            Ok(())
        }));
    let mut parser = HtmlRewriter::new(settings, |_: &[u8]| {});
    parser.write(source.as_bytes()).ok()?;
    parser.end().ok()?;
    let mut renderer = Rc::try_unwrap(renderer).ok()?.into_inner();
    if (!renderer.recognized && !allow_plain_text)
        || !renderer.stack.is_empty()
        || renderer.ruby_close.is_some()
        || renderer.picture.is_some()
    {
        return None;
    }
    if let Some(label) = &renderer.label {
        if !label.closed || renderer.output.static_checkboxes.len() != 1 {
            return None;
        }
        let mut at = 0;
        for span in &label.coverage {
            if span.start < at
                || !source.get(at..span.start)?.chars().all(html_space)
                || source.get(span.clone()).is_none()
            {
                return None;
            }
            at = span.end;
        }
        if !source.get(at..)?.chars().all(html_space) {
            return None;
        }
    }
    if inline_only {
        let mut at = 0;
        for span in &renderer.inline_coverage {
            if span.start < at
                || !source.get(at..span.start)?.chars().all(html_space)
                || source.get(span.clone()).is_none()
            {
                return None;
            }
            at = span.end;
        }
        if !source.get(at..)?.chars().all(html_space) {
            return None;
        }
        // Opening an empty inline element can flush a pending collapsed
        // space. The generated GFM cell trims it at the cell boundary.
        let output = &mut renderer.output;
        if output.markdown.ends_with(' ') {
            let end = output.markdown.len() - 1;
            let space = output
                .parts
                .iter()
                .rposition(|part| !part.output.is_empty())?;
            if output.parts[space].output != (end..end + 1) {
                return None;
            }
            output.parts.remove(space);
            // Some tags, such as images with no usable source or alt text,
            // account for source bytes without emitting any text.
            for part in &mut output.parts[space..] {
                part.output = end..end;
            }
            output.markdown.pop();
            output.inline_text.pop();
            for anchor in &mut output.anchors {
                anchor.offset = anchor.offset.min(end);
            }
            if let Some((style, _)) = output.styles.last_mut()
                && style.end > end as u64
            {
                style.end = end as u64;
                if style.is_empty() {
                    output.styles.pop();
                }
            }
        }
    } else {
        renderer.end_block(source.len()).ok()?;
    }
    let mut last = 0;
    for part in &renderer.output.parts {
        if part.source.start < last || source.get(part.source.clone()).is_none() {
            return None;
        }
        last = part.source.end;
    }
    for destination in &renderer.output.destinations {
        if source.get(destination.source.clone()).is_none()
            || renderer
                .output
                .markdown
                .get(destination.output.clone())
                .is_none()
        {
            return None;
        }
    }
    for picture in &mut renderer.output.pictures {
        for variant in picture.variants() {
            source.get(variant.source.clone())?;
        }
    }
    Some(renderer.output)
}
