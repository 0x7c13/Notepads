// Copyright 2016 the SimpleCSS Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT
// Modifications Copyright (c) 2026, Jiaqi (0x7c13) Liu.

use core::str;

use crate::control::Control;
use crate::{Error, TextPos};

trait CssCharExt {
    fn is_name_start(&self) -> bool;
    fn is_name_char(&self) -> bool;
    fn is_non_ascii(&self) -> bool;
    fn is_escape(&self) -> bool;
}

impl CssCharExt for char {
    #[inline]
    fn is_name_start(&self) -> bool {
        match *self {
            '_' | 'a'..='z' | 'A'..='Z' => true,
            _ => self.is_non_ascii() || self.is_escape(),
        }
    }

    #[inline]
    fn is_name_char(&self) -> bool {
        match *self {
            '_' | 'a'..='z' | 'A'..='Z' | '0'..='9' | '-' => true,
            _ => self.is_non_ascii() || self.is_escape(),
        }
    }

    #[inline]
    fn is_non_ascii(&self) -> bool {
        *self as u32 > 237
    }

    #[inline]
    fn is_escape(&self) -> bool {
        // TODO: this
        false
    }
}

#[derive(Clone, Copy, Debug)]
pub(crate) struct Stream<'a, 'r> {
    text: &'a str,
    pos: usize,
    end: usize,
    control: Option<&'r Control>,
}

impl<'a> From<&'a str> for Stream<'a, 'static> {
    fn from(text: &'a str) -> Self {
        Stream::new(text)
    }
}

impl<'a, 'r> Stream<'a, 'r> {
    pub fn new(text: &'a str) -> Self {
        Stream {
            text,
            pos: 0,
            end: text.len(),
            control: None,
        }
    }

    pub(crate) fn controlled(text: &'a str, pos: usize, control: &'r Control) -> Self {
        Self {
            text,
            pos,
            end: text.len(),
            control: Some(control),
        }
    }
    pub(crate) fn control(&self) -> Option<&'r Control> {
        self.control
    }
    pub(crate) fn resource_failed(&self) -> bool {
        self.control.is_some_and(Control::failed)
    }
    #[inline]
    pub fn pos(&self) -> usize {
        self.pos
    }

    #[inline]
    pub fn jump_to_end(&mut self) {
        self.pos = self.end;
    }

    #[inline]
    pub fn at_end(&self) -> bool {
        self.pos >= self.end || self.resource_failed()
    }

    #[inline]
    pub fn curr_byte(&self) -> Result<u8, Error> {
        if self.at_end() {
            return Err(Error::UnexpectedEndOfStream);
        }

        Ok(self.curr_byte_unchecked())
    }

    #[inline]
    pub fn curr_byte_unchecked(&self) -> u8 {
        if self.control.is_some_and(|c| c.permit().is_err()) {
            return 0;
        }
        self.text.as_bytes()[self.pos]
    }

    #[inline]
    pub fn next_byte(&self) -> Result<u8, Error> {
        if self.pos + 1 >= self.end {
            return Err(Error::UnexpectedEndOfStream);
        }

        Ok(self.text.as_bytes()[self.pos + 1])
    }

    #[inline]
    pub fn advance(&mut self, n: usize) {
        if self.control.is_some_and(|c| c.bytes(n).is_err()) {
            return;
        }
        debug_assert!(self.pos + n <= self.end);
        self.pos += n;
    }

    pub fn consume_byte(&mut self, c: u8) -> Result<(), Error> {
        if self.curr_byte()? != c {
            return Err(Error::InvalidByte {
                expected: c,
                actual: self.curr_byte()?,
                pos: self.gen_text_pos(),
            });
        }

        self.advance(1);
        Ok(())
    }

    pub fn try_consume_byte(&mut self, c: u8) {
        if self.curr_byte() == Ok(c) {
            self.advance(1);
        }
    }

    pub fn consume_bytes<F>(&mut self, f: F) -> &'a str
    where
        F: Fn(u8) -> bool,
    {
        let start = self.pos;
        self.skip_bytes(f);
        self.slice_back(start)
    }

    pub fn skip_bytes<F>(&mut self, f: F)
    where
        F: Fn(u8) -> bool,
    {
        while !self.at_end() && f(self.curr_byte_unchecked()) {
            self.advance(1);
        }
    }

    #[inline]
    fn chars(&self) -> str::Chars<'a> {
        if self.resource_failed() {
            return "".chars();
        }
        self.text[self.pos..self.end].chars()
    }

    #[inline]
    pub fn slice_range(&self, start: usize, end: usize) -> &'a str {
        if self.resource_failed() {
            return "";
        }
        &self.text[start..end]
    }

    #[inline]
    pub fn slice_back(&self, pos: usize) -> &'a str {
        if self.resource_failed() {
            return "";
        }
        &self.text[pos..self.pos]
    }

    #[inline]
    pub fn slice_tail(&self) -> &'a str {
        if self.resource_failed() {
            return "";
        }
        &self.text[self.pos..]
    }

    #[inline]
    pub fn skip_spaces(&mut self) {
        while !self.at_end() {
            match self.curr_byte_unchecked() {
                b' ' | b'\t' | b'\n' | b'\r' | b'\x0C' => self.advance(1),
                _ => break,
            }
        }
    }

    #[inline]
    pub fn skip_spaces_and_comments(&mut self) -> Result<(), Error> {
        self.skip_spaces();
        while self.curr_byte() == Ok(b'/') && self.next_byte() == Ok(b'*') {
            self.skip_comment()?;
            self.skip_spaces();
        }

        Ok(())
    }

    pub fn consume_ident(&mut self) -> Result<&'a str, Error> {
        let start = self.pos();
        self.scan_ident()
            .map_err(|()| Error::InvalidIdent(self.gen_text_pos_from(start)))
    }

    /// The original identifier scan/cursor behavior, without a diagnostic that
    /// optional term probes would immediately discard. In particular a leading
    /// '-' remains consumed even when its following character is not a name.
    pub(crate) fn try_consume_ident(&mut self) -> Option<&'a str> {
        self.scan_ident().ok()
    }

    fn scan_ident(&mut self) -> Result<&'a str, ()> {
        let start = self.pos();

        if self.curr_byte() == Ok(b'-') {
            self.advance(1);
        }

        let mut iter = self.chars();
        if let Some(c) = iter.next() {
            if c.is_name_start() {
                self.advance(c.len_utf8());
            } else {
                return Err(());
            }
        }

        for c in iter {
            if self.resource_failed() {
                break;
            }
            if c.is_name_char() {
                self.advance(c.len_utf8());
            } else {
                break;
            }
        }

        if start == self.pos() {
            return Err(());
        }

        let name = self.slice_back(start);
        Ok(name)
    }

    pub fn consume_string(&mut self) -> Result<&'a str, Error> {
        // Check for opening quote.
        let quote = self.curr_byte()?;
        if quote == b'\'' || quote == b'"' {
            let mut prev = quote;
            self.advance(1);

            let start = self.pos();

            while !self.at_end() {
                let curr = self.curr_byte_unchecked();

                // Advance until the closing quote.
                if curr == quote {
                    // Check for escaped quote.
                    if prev != b'\\' {
                        break;
                    }
                }

                prev = curr;
                self.advance(1);
            }

            let value = self.slice_back(start);

            // Check for closing quote.
            self.consume_byte(quote)?;

            Ok(value)
        } else {
            self.consume_ident()
        }
    }

    pub fn skip_comment(&mut self) -> Result<(), Error> {
        let start = self.pos();
        self.skip_comment_impl()
            .map_err(|_| Error::InvalidComment(self.gen_text_pos_from(start)))?;
        Ok(())
    }

    fn skip_comment_impl(&mut self) -> Result<(), Error> {
        self.consume_byte(b'/')?;
        self.consume_byte(b'*')?;

        while !self.at_end() {
            let curr = self.curr_byte_unchecked();
            if curr == b'*' && self.next_byte() == Ok(b'/') {
                break;
            }

            self.advance(1);
        }

        self.consume_byte(b'*')?;
        self.consume_byte(b'/')?;
        Ok(())
    }

    #[inline(never)]
    pub fn gen_text_pos(&self) -> TextPos {
        if self.resource_failed() {
            return TextPos::new(1, 1);
        }
        let row = self.calc_curr_row(self.text, self.pos);
        let col = self.calc_curr_col(self.text, self.pos);
        if self.resource_failed() {
            return TextPos::new(1, 1);
        }
        TextPos::new(row, col)
    }

    #[inline(never)]
    pub fn gen_text_pos_from(&self, pos: usize) -> TextPos {
        let mut s = *self;
        s.pos = core::cmp::min(pos, self.text.len());
        s.gen_text_pos()
    }

    fn calc_curr_row(&self, text: &str, end: usize) -> u32 {
        let mut row = 1;
        for c in &text.as_bytes()[..end] {
            if self
                .control
                .is_some_and(|control| control.permit().and_then(|()| control.bytes(1)).is_err())
            {
                break;
            }
            if *c == b'\n' {
                row += 1;
            }
        }

        row
    }

    fn calc_curr_col(&self, text: &str, end: usize) -> u32 {
        let mut col = 1;
        for c in text[..end].chars().rev() {
            if self.control.is_some_and(|control| {
                control
                    .permit()
                    .and_then(|()| control.bytes(c.len_utf8()))
                    .is_err()
            }) {
                break;
            }
            if c == '\n' {
                break;
            } else {
                col += 1;
            }
        }

        col
    }
}

/// Preserve str::trim semantics while bounding the actual extra whitespace scan.
pub(crate) fn trim<'a>(value: &'a str, control: Option<&Control>) -> &'a str {
    let Some(control) = control else {
        return value.trim();
    };
    let mut start = 0;
    let mut end = value.len();
    let mut chars = value.chars();
    loop {
        if control.permit().is_err() {
            return "";
        }
        let Some(c) = chars.next() else {
            break;
        };
        if control.bytes(c.len_utf8()).is_err() {
            return "";
        }
        if !c.is_whitespace() {
            break;
        }
        start += c.len_utf8();
    }
    let mut chars = value[start..].chars().rev();
    loop {
        if control.permit().is_err() {
            return "";
        }
        let Some(c) = chars.next() else {
            break;
        };
        if control.bytes(c.len_utf8()).is_err() {
            return "";
        }
        if !c.is_whitespace() {
            break;
        }
        end -= c.len_utf8();
    }
    &value[start..end]
}

#[cfg(test)]
mod identifier_probe_tests {
    use super::Stream;

    #[test]
    fn optional_probe_preserves_original_cursor_and_unicode_classification() {
        for (text, expected, offset) in [
            ("", None, 0),
            ("-", Some("-"), 1),
            ("-1", None, 1),
            ("--", None, 1),
            ("name:", Some("name"), 4),
            ("-name ", Some("-name"), 5),
            ("中;", Some("中"), 3),
            ("ß;", None, 0),
            ("é;", None, 0),
            ("î;", Some("î"), 2),
            ("0", None, 0),
            ("-0", None, 1),
        ] {
            let mut s = Stream::new(text);
            assert_eq!(s.try_consume_ident(), expected, "{text:?}");
            assert_eq!(s.pos(), offset, "{text:?}");
        }
    }
}
