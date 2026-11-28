// Copyright 2016 the SimpleCSS Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT
// Modifications Copyright (c) 2026, Jiaqi (0x7c13) Liu.

/*!
A simple [CSS 2.1](https://www.w3.org/TR/CSS21/) parser and selector.

This is not a browser-grade CSS parser. If you need one,
use [cssparser](https://crates.io/crates/cssparser) +
[selectors](https://crates.io/crates/selectors).

Since it's very simple we will start with limitations:

## Limitations

- [At-rules](https://www.w3.org/TR/CSS21/syndata.html#at-rules) are not supported.
  They will be skipped during parsing.
- Property values are not parsed.
  In CSS like `* { width: 5px }` you will get a `width` property with a `5px` value as a string.
- CDO/CDC comments are not supported.
- Parser is case sensitive. All keywords must be lowercase.
- Unicode escape, like `\26`, is not supported.

## Features

- Selector matching support.
- The rules are sorted by specificity.
- `!important` parsing support.
- Has a high-level parsers and low-level, zero-allocation tokenizers.
- No unsafe.
*/

// LINEBENDER LINT SET - lib.rs - v2
// See https://linebender.org/wiki/canonical-lints/
// These lints aren't included in Cargo.toml because they
// shouldn't apply to examples and tests
#![warn(unused_crate_dependencies)]
#![warn(clippy::print_stdout, clippy::print_stderr)]
// Targeting e.g. 32-bit means structs containing usize can give false positives for 64-bit.
#![cfg_attr(target_pointer_width = "64", warn(clippy::trivially_copy_pass_by_ref))]
// END LINEBENDER LINT SET
#![cfg_attr(docsrs, feature(doc_auto_cfg))]
#![cfg_attr(all(not(feature = "std"), not(test)), no_std)]
// The following lints are part of the Linebender standard set,
// but resolving them has been deferred for now.
// Feel free to send a PR that solves one or more of these.
#![allow(
    missing_debug_implementations,
    unreachable_pub,
    clippy::use_self,
    clippy::missing_assert_message,
    clippy::missing_panics_doc,
    clippy::exhaustive_enums,
    clippy::unseparated_literal_suffix
)]
#![cfg_attr(test, allow(unused_crate_dependencies))] // Some dev dependencies are only used in tests

extern crate alloc;

use alloc::vec::Vec;
use core::fmt;

use log::warn;

mod builder;
mod control;
mod matcher;
mod selector;
mod stream;

pub use selector::*;

#[cfg(feature = "notepads-markdown-resources")]
pub mod controlled;
use stream::Stream;

/// A list of possible errors.
#[derive(Clone, Copy, PartialEq, Debug)]
pub enum Error {
    /// The steam ended earlier than we expected.
    ///
    /// Should only appear on invalid input data.
    UnexpectedEndOfStream,

    /// An invalid ident.
    InvalidIdent(TextPos),

    /// An unclosed comment.
    InvalidComment(TextPos),

    /// An invalid declaration value.
    InvalidValue(TextPos),

    /// An invalid byte.
    #[allow(missing_docs)]
    InvalidByte {
        expected: u8,
        actual: u8,
        pos: TextPos,
    },

    /// A missing selector.
    SelectorMissing,

    /// An unexpected selector.
    UnexpectedSelector,

    /// An unexpected combinator.
    UnexpectedCombinator,

    /// An invalid or unsupported attribute selector.
    InvalidAttributeSelector,

    /// An invalid language pseudo-class.
    InvalidLanguagePseudoClass,
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match *self {
            Error::UnexpectedEndOfStream => {
                write!(f, "unexpected end of stream")
            }
            Error::InvalidIdent(pos) => {
                write!(f, "invalid ident at {}", pos)
            }
            Error::InvalidComment(pos) => {
                write!(f, "invalid comment at {}", pos)
            }
            Error::InvalidValue(pos) => {
                write!(f, "invalid value at {}", pos)
            }
            Error::InvalidByte {
                expected,
                actual,
                pos,
            } => {
                write!(
                    f,
                    "expected '{}' not '{}' at {}",
                    expected as char, actual as char, pos
                )
            }
            Error::SelectorMissing => {
                write!(f, "selector missing")
            }
            Error::UnexpectedSelector => {
                write!(f, "unexpected selector")
            }
            Error::UnexpectedCombinator => {
                write!(f, "unexpected combinator")
            }
            Error::InvalidAttributeSelector => {
                write!(f, "invalid or unsupported attribute selector")
            }
            Error::InvalidLanguagePseudoClass => {
                write!(f, "invalid language pseudo-class")
            }
        }
    }
}

#[cfg(feature = "std")]
impl std::error::Error for Error {}

/// A position in text.
///
/// Position indicates a row/line and a column in the original text. Starting from 1:1.
#[derive(Clone, Copy, PartialEq, Debug)]
#[allow(missing_docs)]
pub struct TextPos {
    pub row: u32,
    pub col: u32,
}

impl TextPos {
    /// Constructs a new `TextPos`.
    ///
    /// Should not be invoked manually, but rather via `Stream::gen_text_pos`.
    pub fn new(row: u32, col: u32) -> TextPos {
        TextPos { row, col }
    }
}

impl fmt::Display for TextPos {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}:{}", self.row, self.col)
    }
}

/// A declaration.
#[derive(Clone, Copy, PartialEq, Debug)]
#[allow(missing_docs)]
pub struct Declaration<'a> {
    pub name: &'a str,
    pub value: &'a str,
    pub important: bool,
}

/// A rule.
#[derive(Clone, Debug)]
pub struct Rule<'a> {
    /// A rule selector.
    pub selector: Selector<'a>,
    /// A rule declarations.
    pub declarations: Vec<Declaration<'a>>,
}

/// A style sheet.
#[derive(Clone, Debug)]
pub struct StyleSheet<'a> {
    /// A list of rules.
    pub rules: Vec<Rule<'a>>,
}

impl<'a> StyleSheet<'a> {
    /// Creates an empty style sheet.
    pub fn new() -> Self {
        StyleSheet { rules: Vec::new() }
    }

    /// Parses a style sheet from text.
    ///
    /// At-rules are not supported and will be skipped.
    ///
    /// # Errors
    ///
    /// Doesn't produce any errors. In worst case scenario will return an empty stylesheet.
    ///
    /// All warnings will be logged.
    pub fn parse(text: &'a str) -> Self {
        let mut sheet = StyleSheet::new();
        sheet.parse_more(text);
        sheet
    }

    /// Parses a style sheet from a text to the current style sheet.
    pub fn parse_more(&mut self, text: &'a str) {
        let mut sink = builder::PlainSheet {
            rules: &mut self.rules,
            declarations: Vec::new(),
        };
        let _ = builder::parse_more(text, &mut sink, None);

        // Remove empty rules.
        self.rules.retain(|rule| !rule.declarations.is_empty());

        // Sort the rules by specificity.
        self.rules
            .sort_by_cached_key(|rule| rule.selector.specificity());
    }
}

impl fmt::Display for StyleSheet<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        for (i, rule) in self.rules.iter().enumerate() {
            write!(f, "{} {{ ", rule.selector)?;
            for dec in &rule.declarations {
                write!(f, "{}:{}", dec.name, dec.value)?;
                if dec.important {
                    write!(f, " !important")?;
                }
                write!(f, ";")?;
            }
            write!(f, " }}")?;

            if i != self.rules.len() - 1 {
                writeln!(f)?;
            }
        }

        Ok(())
    }
}

impl Default for StyleSheet<'_> {
    fn default() -> Self {
        Self::new()
    }
}

pub(crate) fn consume_at_rule(s: &mut Stream<'_, '_>) -> Result<(), Error> {
    let ident = s.consume_ident()?;
    warn!("The @{} rule is not supported. Skipped.", ident);

    s.skip_bytes(|c| c != b';' && c != b'{');

    match s.curr_byte()? {
        b';' => s.advance(1),
        b'{' => consume_block(s),
        _ => {}
    }

    Ok(())
}

fn consume_block(s: &mut Stream<'_, '_>) {
    s.try_consume_byte(b'{');
    consume_until_block_end(s);
}

pub(crate) fn consume_until_block_end(s: &mut Stream<'_, '_>) {
    // Block can have nested blocks, so we have to check for matching braces.
    // We simply counting the number of opening braces, which is incorrect,
    // since `{` can be inside a string, but it's fine for majority of the cases.

    let mut braces = 0;
    while !s.at_end() {
        match s.curr_byte_unchecked() {
            b'{' => {
                braces += 1;
            }
            b'}' => {
                if braces == 0 {
                    break;
                } else {
                    braces -= 1;
                }
            }
            _ => {}
        }

        s.advance(1);
    }

    s.try_consume_byte(b'}');
}

/// A declaration tokenizer.
///
/// Tokenizer will stop at the first invalid token.
///
/// # Example
///
/// ```
/// use simplecss::{DeclarationTokenizer, Declaration};
///
/// let mut t = DeclarationTokenizer::from("background: url(\"img.png\"); color:red !important");
/// assert_eq!(t.next().unwrap(), Declaration { name: "background", value: "url(\"img.png\")", important: false });
/// assert_eq!(t.next().unwrap(), Declaration { name: "color", value: "red", important: true });
/// ```
pub struct DeclarationTokenizer<'a> {
    stream: Stream<'a, 'static>,
}

impl<'a> From<&'a str> for DeclarationTokenizer<'a> {
    fn from(text: &'a str) -> Self {
        DeclarationTokenizer {
            stream: Stream::from(text),
        }
    }
}

impl<'a> Iterator for DeclarationTokenizer<'a> {
    type Item = Declaration<'a>;

    fn next(&mut self) -> Option<Self::Item> {
        let _ = self.stream.skip_spaces_and_comments();

        if self.stream.at_end() {
            return None;
        }

        match consume_declaration(&mut self.stream) {
            Ok(v) => Some(v),
            Err(_) => {
                self.stream.jump_to_end();
                None
            }
        }
    }
}

pub(crate) fn consume_declaration<'a>(s: &mut Stream<'a, '_>) -> Result<Declaration<'a>, Error> {
    s.skip_spaces_and_comments()?;

    // Parse name.

    // https://snook.ca/archives/html_and_css/targetting_ie7
    if s.curr_byte() == Ok(b'*') {
        s.advance(1);
    }

    let name = s.consume_ident()?;

    s.skip_spaces_and_comments()?;
    s.consume_byte(b':')?;
    s.skip_spaces_and_comments()?;

    // Parse value.
    let start = s.pos();
    let value = if matches!(name, "font-family" | "font") {
        let end = consume_font_value_extent(s)?;
        s.slice_range(start, end)
    } else {
        let mut end = s.pos();
        // These normal terminators previously constructed an InvalidIdent position
        // which was immediately discarded. Position calculation rescans the entire
        // prefix, making an ordinary multi-declaration sheet quadratic.
        while !matches!(s.curr_byte(), Ok(b';' | b'}' | b'!') | Err(_)) && consume_term(s).is_ok() {
            end = s.pos();
            s.skip_spaces_and_comments()?;
        }
        stream::trim(s.slice_range(start, end), s.control())
    };

    s.skip_spaces_and_comments()?;

    // Check for `important`.
    let mut important = false;
    if s.curr_byte() == Ok(b'!') {
        s.advance(1);
        s.skip_spaces_and_comments()?;
        if s.slice_tail().starts_with("important") {
            s.advance(9);
            important = true;
        }
    }

    s.skip_spaces_and_comments()?;

    while s.curr_byte() == Ok(b';') {
        s.advance(1);
        s.skip_spaces_and_comments()?;
    }

    s.skip_spaces_and_comments()?;

    if value.is_empty() {
        return Err(Error::InvalidValue(s.gen_text_pos_from(start)));
    }

    Ok(Declaration {
        name,
        value,
        important,
    })
}

/// Retain font source bytes for the downstream font parser, without decoding or
/// validating names here. Other declarations keep their original term grammar.
fn consume_font_value_extent(s: &mut Stream<'_, '_>) -> Result<usize, Error> {
    let mut end = s.pos();
    let mut quote = None;
    let mut escaped = false;
    let mut escape_hex_digits = 0u8;
    let mut escape_cr = false;
    let mut comment = false;
    let mut comment_star = false;
    let mut slash = false;
    let mut scanned = 0usize;
    loop {
        if scanned == 0 {
            #[cfg(feature = "notepads-markdown-resources")]
            if let Some(control) = s.control() {
                // Stream::advance still pays all scanning work. This explicit
                // checkpoint bounds cancellation even with shared scan credit.
                control
                    .resources()
                    .checkpoint(0)
                    .map_err(|_| Error::UnexpectedEndOfStream)?;
            }
        }
        if s.resource_failed() {
            return Err(Error::UnexpectedEndOfStream);
        }
        let byte = match s.curr_byte() {
            Ok(byte) => byte,
            Err(error) if s.resource_failed() => return Err(error),
            Err(_) => break,
        };
        let mut keep_end = true;
        let mut escape_byte = false;
        if !comment {
            if escaped {
                escaped = false;
                escape_hex_digits = u8::from(byte.is_ascii_hexdigit());
                escape_cr = byte == b'\r';
                escape_byte = true;
            } else {
                if escape_cr {
                    escape_cr = false;
                    escape_byte = byte == b'\n';
                }
                if !escape_byte && escape_hex_digits != 0 {
                    if byte.is_ascii_hexdigit() && escape_hex_digits < 6 {
                        escape_hex_digits += 1;
                        escape_byte = true;
                    } else {
                        escape_hex_digits = 0;
                        if matches!(byte, b' ' | b'\t' | b'\r' | b'\n' | 0x0c) {
                            escape_cr = byte == b'\r';
                            escape_byte = true;
                        }
                    }
                }
            }
        }
        if comment {
            if comment_star && byte == b'/' {
                comment = false;
                comment_star = false;
            } else {
                comment_star = byte == b'*';
            }
        } else if escape_byte {
            // Keep the original escape, including its optional hex terminator
            // or CRLF continuation. No scalar is decoded in this intake.
        } else if let Some(mark) = quote {
            if byte == b'\\' {
                escaped = true;
            } else if byte == mark {
                quote = None;
            }
        } else if slash && byte == b'*' {
            slash = false;
            comment = true;
        } else {
            slash = false;
            match byte {
                b';' | b'}' | b'!' => break,
                b'\'' | b'"' => quote = Some(byte),
                b'\\' => escaped = true,
                b'/' => slash = true,
                b' ' | b'\t' | b'\r' | b'\n' | 0x0c => keep_end = false,
                _ => {}
            }
        }
        // Byte scanning cannot end a successful slice within a UTF-8 scalar:
        // only ASCII delimiters or true EOF stop it. A resource stop returns
        // an error before any slice is made, including inside a scalar.
        s.advance(1);
        if s.resource_failed() {
            return Err(Error::UnexpectedEndOfStream);
        }
        scanned = (scanned + 1) % 4096;
        if keep_end {
            end = s.pos();
        }
    }
    Ok(end)
}

fn consume_term(s: &mut Stream<'_, '_>) -> Result<(), Error> {
    fn consume_digits(s: &mut Stream<'_, '_>) {
        while let Ok(b'0'..=b'9') = s.curr_byte() {
            s.advance(1);
        }
    }

    match s.curr_byte()? {
        b'#' => {
            s.advance(1);
            match s.try_consume_ident() {
                Some(_) => {}
                None => {
                    // Try consume as a hex color.
                    while let Ok(c) = s.curr_byte() {
                        match c {
                            b'0'..=b'9' | b'a'..=b'f' | b'A'..=b'F' => s.advance(1),
                            _ => break,
                        }
                    }
                }
            }
        }
        b'+' | b'-' | b'0'..=b'9' | b'.' => {
            // Consume number.

            s.advance(1);
            consume_digits(s);
            if s.curr_byte() == Ok(b'.') {
                s.advance(1);
                consume_digits(s);
            }

            if s.curr_byte() == Ok(b'%') {
                s.advance(1);
            } else {
                // Absence of a suffix is ordinary syntax, not a diagnostic.
                let _ = s.try_consume_ident();
            }
        }
        b'\'' | b'"' => {
            s.consume_string()?;
        }
        b',' => {
            s.advance(1);
        }
        _ => {
            let _ = s.consume_ident()?;

            // Consume function.
            if s.curr_byte() == Ok(b'(') {
                s.skip_bytes(|c| c != b')');
                s.consume_byte(b')')?;
            }
        }
    }

    Ok(())
}
