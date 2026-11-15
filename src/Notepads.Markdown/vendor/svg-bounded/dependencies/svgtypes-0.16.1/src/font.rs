// Copyright 2024 the SVG Types Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT
// Modifications Copyright (c) 2026, Jiaqi (0x7c13) Liu.

use crate::stream::{ByteExt, Stream};
use crate::Error;
use alloc::format;
use alloc::string::{String, ToString};
#[cfg(test)]
use alloc::vec;
use alloc::vec::Vec;
use core::fmt::Display;

#[cfg(test)]
#[path = "font_sink_tests.rs"]
mod font_sink_tests;

/// Parses a list of font families and generic families from a string.
pub fn parse_font_families(text: &str) -> Result<Vec<FontFamily>, Error> {
    match parse_font_families_with_control(text, |_| Ok::<_, core::convert::Infallible>(())) {
        Ok(families) => Ok(families),
        Err(FontFamilyParseError::Syntax(error)) => Err(error),
        Err(FontFamilyParseError::Control(never)) => match never {},
    }
}

/// A font-family syntax error or an unchanged caller checkpoint error.
#[derive(Debug, PartialEq, Eq)]
pub enum FontFamilyParseError<E> {
    /// An invalid CSS family list.
    Syntax(Error),
    /// The caller rejected further work. No syntax fallback is implied.
    Control(E),
}

impl<E> From<Error> for FontFamilyParseError<E> {
    fn from(error: Error) -> Self {
        Self::Syntax(error)
    }
}

/// Classification of a fully decoded family, without an owned string.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FontFamilyKind {
    /// A serif family.
    Serif,
    /// A sans-serif family.
    SansSerif,
    /// A cursive family.
    Cursive,
    /// A fantasy family.
    Fantasy,
    /// A monospace family.
    Monospace,
    /// A named family, including quoted generic spellings.
    Named,
}

/// Caller-owned output for the shared CSS family grammar.
///
/// `Name::as_ref` must be an O(1), allocation-free borrow. A sink must record
/// resource errors before destroying its own temporary owners. Partial output
/// stays with the caller and must be retired before syntax fallback/publication.
pub trait FontFamilySink<E> {
    /// The caller's ordinary or request-owned UTF-8 name.
    type Name: AsRef<str>;
    /// Construct the name in the original output policy.
    fn new_name(&mut self) -> Result<Self::Name, E>;
    /// Append one decoded scalar, without an intermediate ordinary string.
    fn push_char(&mut self, name: &mut Self::Name, ch: char) -> Result<(), E>;
    /// Consume a completed name. Generic classification may discard its name;
    /// resource failure during that cleanup must remain visible to the caller.
    fn finish_name(&mut self, name: Self::Name, kind: FontFamilyKind) -> Result<(), E>;
}

/// Parse with caller-owned output. Unlike the scan-only compatibility API,
/// this entry checks the original slot after each sink/cleanup boundary.
/// The zero-work callback must check the same original request; it is not a
/// substitute for source-work prepayment. Callbacks and sink are borrowed.
pub fn parse_font_families_into_with_control<E, S: FontFamilySink<E>>(
    text: &str,
    checkpoint: &mut impl FnMut(usize) -> Result<(), E>,
    sink: &mut S,
) -> Result<(), FontFamilyParseError<E>> {
    checkpoint(0).map_err(FontFamilyParseError::Control)?;
    let result = FamilyParser {
        text,
        pos: 0,
        char_pos: 1,
        prepaid: 0,
        checkpoint: &mut *checkpoint,
        check_sink: true,
    }
    .parse_into(sink);
    match result {
        Err(FontFamilyParseError::Control(error)) => Err(FontFamilyParseError::Control(error)),
        candidate => {
            // All local Name owners have exited before this tail, including
            // on Syntax. A refund error must not turn into syntax fallback.
            checkpoint(0).map_err(FontFamilyParseError::Control)?;
            candidate
        }
    }
}

struct PlainFamilySink {
    families: Vec<FontFamily>,
}
impl<E> FontFamilySink<E> for PlainFamilySink {
    type Name = String;
    fn new_name(&mut self) -> Result<String, E> {
        Ok(String::new())
    }
    fn push_char(&mut self, name: &mut String, ch: char) -> Result<(), E> {
        name.push(ch);
        Ok(())
    }
    fn finish_name(&mut self, name: String, kind: FontFamilyKind) -> Result<(), E> {
        self.families.push(match kind {
            FontFamilyKind::Serif => FontFamily::Serif,
            FontFamilyKind::SansSerif => FontFamily::SansSerif,
            FontFamilyKind::Cursive => FontFamily::Cursive,
            FontFamilyKind::Fantasy => FontFamily::Fantasy,
            FontFamilyKind::Monospace => FontFamily::Monospace,
            FontFamilyKind::Named => FontFamily::Named(name),
        });
        Ok(())
    }
}

/// Parses a CSS family list while prepaying source scanning in at most 4096-byte batches.
///
/// A zero-work checkpoint precedes parsing, including empty input. Each source byte is
/// then prepaid before it is inspected. The original result containers are retained;
/// this callback controls scanning, not allocation accounting for those containers.
pub fn parse_font_families_with_control<E>(
    text: &str,
    mut checkpoint: impl FnMut(usize) -> Result<(), E>,
) -> Result<Vec<FontFamily>, FontFamilyParseError<E>> {
    checkpoint(0).map_err(FontFamilyParseError::Control)?;
    let mut sink = PlainFamilySink {
        families: Vec::new(),
    };
    FamilyParser {
        text,
        pos: 0,
        char_pos: 1,
        prepaid: 0,
        checkpoint,
        check_sink: false,
    }
    .parse_into(&mut sink)?;
    Ok(sink.families)
}

#[cfg(test)]
mod font_family_source_tests {
    use super::*;

    fn named(value: &str) -> FontFamily {
        FontFamily::Named(value.to_string())
    }

    #[test]
    fn generic_keywords_are_ascii_case_insensitive() {
        assert_eq!(
            parse_font_families("SeRiF, SANS-SERIF, Cursive, FANTASY, MonoSpace").unwrap(),
            vec![
                FontFamily::Serif,
                FontFamily::SansSerif,
                FontFamily::Cursive,
                FontFamily::Fantasy,
                FontFamily::Monospace,
            ]
        );
    }

    #[test]
    fn quoted_generic_keywords_remain_named() {
        assert_eq!(
            parse_font_families(r#""SERIF", 'sans-serif', "\73 erif""#).unwrap(),
            vec![named("SERIF"), named("sans-serif"), named("serif")]
        );
    }

    #[test]
    fn escaped_single_identifiers_can_resolve_to_generic() {
        assert_eq!(
            parse_font_families(r"\53 ERIF, \sans-serif, cur\73 ive, fan\tasy, mono\73 pace")
                .unwrap(),
            vec![
                FontFamily::Serif,
                FontFamily::SansSerif,
                FontFamily::Cursive,
                FontFamily::Fantasy,
                FontFamily::Monospace,
            ]
        );
    }

    #[test]
    fn quoted_simple_and_hex_escapes_are_decoded_once() {
        assert_eq!(
            parse_font_families(r#""A\"B\\C\,D", '\41 rial', "\4E2D \6587""#).unwrap(),
            vec![named("A\"B\\C,D"), named("Arial"), named("中文")]
        );
    }

    #[test]
    fn escaped_list_and_token_delimiters_stay_inside_names() {
        assert_eq!(
            parse_font_families(r"ACME\,Inc, ACME\ Sans, ACME\\Font").unwrap(),
            vec![named("ACME,Inc"), named("ACME Sans"), named("ACME\\Font")]
        );
    }

    #[test]
    fn even_and_odd_backslashes_before_quotes_have_distinct_meaning() {
        for (source, expected) in [
            (r#""A\\""#, "A\\"),
            (r#""A\\\"B""#, "A\\\"B"),
            (r"'A\\'", "A\\"),
            (r"'A\\\'B'", "A\\'B"),
        ] {
            assert_eq!(parse_font_families(source).unwrap(), vec![named(expected)]);
        }
    }

    #[test]
    fn all_non_ascii_codepoints_are_valid_identifier_characters() {
        assert_eq!(
            parse_font_families("\u{80}Face, \u{a0}Face, Ελληνικά, 𐐀Name").unwrap(),
            vec![
                named("\u{80}Face"),
                named("\u{a0}Face"),
                named("Ελληνικά"),
                named("𐐀Name"),
            ]
        );
    }

    #[test]
    fn multiple_identifiers_only_normalize_token_separators() {
        assert_eq!(
            parse_font_families("Noto\tSans \r\nCJK, sans-serif Extra, ser\\ if").unwrap(),
            vec![
                named("Noto Sans CJK"),
                named("sans-serif Extra"),
                named("ser if")
            ]
        );
    }

    #[test]
    fn comments_are_trivia_only_outside_tokens_and_strings() {
        assert_eq!(
            parse_font_families(
                r#"/*lead*/Noto/**/Sans/*tail*/,/*x*/SeRiF/**/, "A/*literal*/B"/*done*/"#
            )
            .unwrap(),
            vec![
                named("Noto Sans"),
                FontFamily::Serif,
                named("A/*literal*/B")
            ]
        );
    }

    #[test]
    fn invalid_hex_codepoints_use_css_source_replacement() {
        assert_eq!(
            parse_font_families(r#""\0 \D800 \110000", A\000000 B\00DFFF C\FFFFFF"#).unwrap(),
            vec![named("���"), named("A�B�C�")]
        );
    }

    #[test]
    fn hex_escapes_consume_at_most_six_digits_and_one_whitespace() {
        assert_eq!(
            parse_font_families(r#""\000041B", "\41 B", "\41  B", \000041B"#).unwrap(),
            vec![named("AB"), named("AB"), named("A B"), named("AB")]
        );
    }

    #[test]
    fn string_escaped_newlines_are_removed() {
        assert_eq!(
            parse_font_families("\"A\\\nB\", \"C\\\r\nD\", \"E\\\rF\", \"G\\\u{c}H\"").unwrap(),
            vec![named("AB"), named("CD"), named("EF"), named("GH")]
        );
    }

    #[test]
    fn bad_strings_are_rejected() {
        for source in [
            "\"bad\nstring\"",
            "\"bad\rstring\"",
            "\"bad\u{c}string\"",
            "'unterminated",
            "\"dangling\\",
        ] {
            assert!(parse_font_families(source).is_err(), "{source:?}");
        }
    }

    #[test]
    fn unescaped_number_starts_remain_invalid() {
        for source in ["123", "1Face", "-1Face", "+1Face", "Hawaii 5-0"] {
            assert!(parse_font_families(source).is_err(), "{source:?}");
        }
    }

    #[test]
    fn css_identifier_starts_support_double_hyphens_and_escaped_digits() {
        assert_eq!(
            parse_font_families(r"--Face, -\31 Face, \31 Face").unwrap(),
            vec![named("--Face"), named("-1Face"), named("1Face")]
        );
    }

    #[test]
    fn long_source_loops_prepay_every_byte_in_bounded_batches() {
        for (source, expected) in [
            (
                format!("/*{}*/ SeRiF", "中".repeat(5000)),
                vec![FontFamily::Serif],
            ),
            (
                format!("\"{}\"", "\\000041".repeat(4000)),
                vec![named(&"A".repeat(4000))],
            ),
            (
                format!("{} Serif", "A".repeat(12000)),
                vec![named(&format!("{} Serif", "A".repeat(12000)))],
            ),
            (
                format!("{}serif", " \t\r\n\u{c}".repeat(3000)),
                vec![FontFamily::Serif],
            ),
            (
                format!("\"{}\"", "A\\\r\n".repeat(5000)),
                vec![named(&"A".repeat(5000))],
            ),
        ] {
            let mut batches = Vec::new();
            let result = parse_font_families_with_control(&source, |bytes| {
                batches.push(bytes);
                Ok::<_, ()>(())
            })
            .unwrap();
            assert_eq!(result, expected);
            assert_eq!(batches.first(), Some(&0));
            assert!(batches.iter().all(|bytes| *bytes <= 4096));
            assert_eq!(batches.iter().sum::<usize>(), source.len());
        }
    }

    #[test]
    fn checkpoint_failure_preserves_caller_error_and_stops_scanning() {
        #[derive(Debug, PartialEq, Eq)]
        struct CallerError(i32);
        let source = format!("/*{}*/ \"Other\"", "中".repeat(6000));
        for reject_call in 1..=5 {
            let mut calls = 0;
            let result = parse_font_families_with_control(&source, |_| {
                calls += 1;
                if calls == reject_call {
                    Err(CallerError(-2718))
                } else {
                    Ok(())
                }
            });
            assert_eq!(
                result,
                Err(FontFamilyParseError::Control(CallerError(-2718)))
            );
            assert_eq!(calls, reject_call);
        }
    }

    #[test]
    fn syntax_and_control_errors_remain_distinct_including_empty_input() {
        assert_eq!(
            parse_font_families_with_control("123", |_| Ok::<_, i32>(())),
            Err(FontFamilyParseError::Syntax(Error::InvalidIdent))
        );
        assert_eq!(
            parse_font_families_with_control("", |_| Err::<(), _>(-314)),
            Err(FontFamilyParseError::Control(-314))
        );
    }
}

/// A type of font family.
#[derive(Clone, PartialEq, Eq, Debug, Hash)]
pub enum FontFamily {
    /// A serif font.
    Serif,
    /// A sans-serif font.
    SansSerif,
    /// A cursive font.
    Cursive,
    /// A fantasy font.
    Fantasy,
    /// A monospace font.
    Monospace,
    /// A custom named font.
    Named(String),
}

impl Display for FontFamily {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        let str = match self {
            Self::Monospace => "monospace".to_string(),
            Self::Serif => "serif".to_string(),
            Self::SansSerif => "sans-serif".to_string(),
            Self::Cursive => "cursive".to_string(),
            Self::Fantasy => "fantasy".to_string(),
            Self::Named(s) => format!("\"{s}\""),
        };
        write!(f, "{str}")
    }
}

// Deliberately local to font-family: the shared SVG Stream grammar is unchanged.
struct FamilyParser<'a, F> {
    text: &'a str,
    pos: usize,
    char_pos: usize,
    prepaid: usize,
    checkpoint: F,
    check_sink: bool,
}

impl<E, F: FnMut(usize) -> Result<(), E>> FamilyParser<'_, F> {
    fn after_sink(&mut self) -> Result<(), FontFamilyParseError<E>> {
        if self.check_sink {
            (self.checkpoint)(0).map_err(FontFamilyParseError::Control)?;
        }
        Ok(())
    }

    fn new_name<S: FontFamilySink<E>>(
        &mut self,
        sink: &mut S,
    ) -> Result<S::Name, FontFamilyParseError<E>> {
        let name = sink.new_name().map_err(FontFamilyParseError::Control)?;
        self.after_sink()?;
        Ok(name)
    }

    fn push_char<S: FontFamilySink<E>>(
        &mut self,
        sink: &mut S,
        name: &mut S::Name,
        ch: char,
    ) -> Result<(), FontFamilyParseError<E>> {
        sink.push_char(name, ch)
            .map_err(FontFamilyParseError::Control)?;
        self.after_sink()
    }
    fn prepay(&mut self, end: usize) -> Result<(), FontFamilyParseError<E>> {
        while self.prepaid < end {
            let bytes = (self.text.len() - self.prepaid).min(4096);
            (self.checkpoint)(bytes).map_err(FontFamilyParseError::Control)?;
            self.prepaid += bytes;
        }
        Ok(())
    }

    fn byte(&mut self, offset: usize) -> Result<Option<u8>, FontFamilyParseError<E>> {
        let index = self.pos + offset;
        if index >= self.text.len() {
            return Ok(None);
        }
        self.prepay(index + 1)?;
        Ok(Some(self.text.as_bytes()[index]))
    }

    fn bump_ascii(&mut self, count: usize) {
        self.pos += count;
        self.char_pos += count;
    }

    fn bump_char(&mut self) -> Result<char, FontFamilyParseError<E>> {
        // Reading a UTF-8 scalar can inspect four bytes. Prepay that lookahead
        // before decoding, including when a scalar crosses a batch boundary.
        self.prepay(self.pos.saturating_add(4).min(self.text.len()))?;
        let value = self.text[self.pos..]
            .chars()
            .next()
            .ok_or(Error::UnexpectedEndOfStream)?;
        self.pos += value.len_utf8();
        self.char_pos += 1;
        Ok(value)
    }

    fn is_space(byte: u8) -> bool {
        matches!(byte, b' ' | b'\t' | b'\n' | b'\r' | 0x0c)
    }

    fn is_newline(byte: u8) -> bool {
        matches!(byte, b'\n' | b'\r' | 0x0c)
    }

    fn is_name_start(byte: u8) -> bool {
        byte.is_ascii_alphabetic() || byte == b'_' || byte >= 0x80
    }

    fn valid_escape(&mut self, offset: usize) -> Result<bool, FontFamilyParseError<E>> {
        Ok(self.byte(offset)? == Some(b'\\')
            && !self.byte(offset + 1)?.is_some_and(Self::is_newline))
    }

    fn skip_trivia(&mut self) -> Result<(), FontFamilyParseError<E>> {
        while let Some(byte) = self.byte(0)? {
            if Self::is_space(byte) {
                self.bump_ascii(1);
            } else if byte == b'/' && self.byte(1)? == Some(b'*') {
                self.bump_ascii(2);
                while let Some(byte) = self.byte(0)? {
                    if byte == b'*' && self.byte(1)? == Some(b'/') {
                        self.bump_ascii(2);
                        break;
                    }
                    self.bump_char()?;
                }
            } else {
                break;
            }
        }
        Ok(())
    }

    fn escape(&mut self, in_string: bool) -> Result<Option<char>, FontFamilyParseError<E>> {
        self.bump_ascii(1); // The backslash was already inspected and prepaid.
        let Some(byte) = self.byte(0)? else {
            return if in_string {
                Err(Error::UnexpectedEndOfStream.into())
            } else {
                Ok(Some('\u{fffd}'))
            };
        };
        if Self::is_newline(byte) {
            if !in_string {
                return Err(Error::InvalidIdent.into());
            }
            self.bump_ascii(1);
            if byte == b'\r' && self.byte(0)? == Some(b'\n') {
                self.bump_ascii(1);
            }
            return Ok(None);
        }
        if byte.is_ascii_hexdigit() {
            let mut value = 0u32;
            for _ in 0..6 {
                let Some(byte) = self.byte(0)? else {
                    break;
                };
                let digit = match byte {
                    b'0'..=b'9' => byte - b'0',
                    b'a'..=b'f' => byte - b'a' + 10,
                    b'A'..=b'F' => byte - b'A' + 10,
                    _ => break,
                };
                value = value * 16 + u32::from(digit);
                self.bump_ascii(1);
            }
            if let Some(space) = self.byte(0)?.filter(|byte| Self::is_space(*byte)) {
                self.bump_ascii(1);
                if space == b'\r' && self.byte(0)? == Some(b'\n') {
                    self.bump_ascii(1);
                }
            }
            // CSS source escapes replace invalid scalars; private wire decoding
            // has a separate strict contract and must not reuse this recovery.
            return Ok(Some(
                char::from_u32(value)
                    .filter(|value| *value != '\0')
                    .unwrap_or('\u{fffd}'),
            ));
        }
        let value = self.bump_char()?;
        Ok(Some(if value == '\0' { '\u{fffd}' } else { value }))
    }

    fn quoted_name<S: FontFamilySink<E>>(
        &mut self,
        quote: u8,
        sink: &mut S,
    ) -> Result<S::Name, FontFamilyParseError<E>> {
        self.bump_ascii(1);
        let mut name = self.new_name(sink)?;
        loop {
            let byte = self.byte(0)?.ok_or(Error::UnexpectedEndOfStream)?;
            if byte == quote {
                self.bump_ascii(1);
                return Ok(name);
            }
            if Self::is_newline(byte) {
                return Err(Error::InvalidValue.into());
            }
            if byte == b'\\' {
                if let Some(value) = self.escape(true)? {
                    self.push_char(sink, &mut name, value)?;
                }
            } else {
                let value = self.bump_char()?;
                self.push_char(
                    sink,
                    &mut name,
                    if value == '\0' { '\u{fffd}' } else { value },
                )?;
            }
        }
    }

    fn ident_into<S: FontFamilySink<E>>(
        &mut self,
        name: &mut S::Name,
        sink: &mut S,
    ) -> Result<(), FontFamilyParseError<E>> {
        let start = match self.byte(0)? {
            Some(b'-') => match self.byte(1)? {
                Some(byte) if Self::is_name_start(byte) || byte == b'-' => true,
                Some(b'\\') => self.valid_escape(1)?,
                _ => false,
            },
            Some(byte) if Self::is_name_start(byte) => true,
            Some(b'\\') => self.valid_escape(0)?,
            _ => false,
        };
        if !start {
            return Err(Error::InvalidIdent.into());
        }
        while let Some(byte) = self.byte(0)? {
            if Self::is_name_start(byte) || byte.is_ascii_digit() || byte == b'-' {
                let ch = self.bump_char()?;
                self.push_char(sink, name, ch)?;
            } else if byte == b'\\' && self.valid_escape(0)? {
                if let Some(value) = self.escape(false)? {
                    self.push_char(sink, name, value)?;
                }
            } else {
                break;
            }
        }
        Ok(())
    }

    fn identifier_family<S: FontFamilySink<E>>(
        &mut self,
        sink: &mut S,
    ) -> Result<(S::Name, FontFamilyKind), FontFamilyParseError<E>> {
        let mut name = self.new_name(sink)?;
        self.ident_into(&mut name, sink)?;
        self.skip_trivia()?;
        let mut single_ident = true;
        while !matches!(self.byte(0)?, None | Some(b',')) {
            self.push_char(sink, &mut name, ' ')?;
            self.ident_into(&mut name, sink)?;
            single_ident = false;
            self.skip_trivia()?;
        }
        // Length checks bound keyword comparison even for a very long name.
        let text = name.as_ref();
        let family = if !single_ident {
            FontFamilyKind::Named
        } else if text.len() == 5 && text.eq_ignore_ascii_case("serif") {
            FontFamilyKind::Serif
        } else if text.len() == 10 && text.eq_ignore_ascii_case("sans-serif") {
            FontFamilyKind::SansSerif
        } else if text.len() == 7 && text.eq_ignore_ascii_case("cursive") {
            FontFamilyKind::Cursive
        } else if text.len() == 7 && text.eq_ignore_ascii_case("fantasy") {
            FontFamilyKind::Fantasy
        } else if text.len() == 9 && text.eq_ignore_ascii_case("monospace") {
            FontFamilyKind::Monospace
        } else {
            FontFamilyKind::Named
        };
        Ok((name, family))
    }

    fn parse_into<S: FontFamilySink<E>>(
        mut self,
        sink: &mut S,
    ) -> Result<(), FontFamilyParseError<E>> {
        self.skip_trivia()?;
        while let Some(byte) = self.byte(0)? {
            // Preserve the existing API's handling of empty comma entries.
            if byte == b',' {
                self.bump_ascii(1);
                self.skip_trivia()?;
                continue;
            }
            let (name, kind) = if byte == b'\'' || byte == b'"' {
                (self.quoted_name(byte, sink)?, FontFamilyKind::Named)
            } else {
                self.identifier_family(sink)?
            };
            if kind != FontFamilyKind::Named || !name.as_ref().is_empty() {
                sink.finish_name(name, kind)
                    .map_err(FontFamilyParseError::Control)?;
            } else {
                drop(name);
            }
            self.after_sink()?;
            self.skip_trivia()?;
            match self.byte(0)? {
                None => break,
                Some(b',') => {
                    self.bump_ascii(1);
                    self.skip_trivia()?;
                }
                _ => return Err(Error::UnexpectedData(self.char_pos).into()),
            }
        }
        Ok(())
    }
}

/// The values of a [`font` shorthand](https://www.w3.org/TR/css-fonts-3/#font-prop).
#[derive(Clone, PartialEq, Eq, Debug, Hash)]
pub struct FontShorthand<'a> {
    /// The font style.
    pub font_style: Option<&'a str>,
    /// The font variant.
    pub font_variant: Option<&'a str>,
    /// The font weight.
    pub font_weight: Option<&'a str>,
    /// The font stretch.
    pub font_stretch: Option<&'a str>,
    /// The font size.
    pub font_size: &'a str,
    /// The font family.
    pub font_family: &'a str,
}

impl<'a> FontShorthand<'a> {
    /// Parses the `font` shorthand from a string.
    ///
    /// We can't use the `FromStr` trait because it requires
    /// an owned value as a return type.
    ///
    /// [font]: https://www.w3.org/TR/css-fonts-3/#font-prop
    #[allow(clippy::should_implement_trait)] // We aren't changing public API yet.
    pub fn from_str(text: &'a str) -> Result<Self, Error> {
        let mut stream = Stream::from(text);
        stream.skip_spaces();

        let mut prev_pos = stream.pos();

        let mut font_style = None;
        let mut font_variant = None;
        let mut font_weight = None;
        let mut font_stretch = None;

        for _ in 0..4 {
            let ident = stream.consume_ascii_ident();

            match ident {
                // TODO: Reuse actual parsers to prevent duplication.
                // We ignore normal because it's ambiguous to which it belongs and all
                // other attributes need to be reset anyway.
                "normal" => {}
                "small-caps" => font_variant = Some(ident),
                "italic" | "oblique" => font_style = Some(ident),
                "bold" | "bolder" | "lighter" | "100" | "200" | "300" | "400" | "500" | "600"
                | "700" | "800" | "900" => font_weight = Some(ident),
                "ultra-condensed" | "extra-condensed" | "condensed" | "semi-condensed"
                | "semi-expanded" | "expanded" | "extra-expanded" | "ultra-expanded" => {
                    font_stretch = Some(ident);
                }
                _ => {
                    // Not one of the 4 properties, so we backtrack and then start
                    // passing font size and family.
                    stream = Stream::from(text);
                    stream.advance(prev_pos);
                    break;
                }
            }

            stream.skip_spaces();
            prev_pos = stream.pos();
        }

        prev_pos = stream.pos();
        if stream.curr_byte()?.is_digit() {
            // A font size such as '15pt'.
            let _ = stream.parse_length()?;
        } else {
            // A font size like 'xx-large'.
            let size = stream.consume_ascii_ident();

            if !matches!(
                size,
                "xx-small"
                    | "x-small"
                    | "small"
                    | "medium"
                    | "large"
                    | "x-large"
                    | "xx-large"
                    | "larger"
                    | "smaller"
            ) {
                return Err(Error::UnexpectedData(prev_pos));
            }
        }

        let font_size = stream.slice_back(prev_pos);
        stream.skip_spaces();

        if stream.curr_byte()? == b'/' {
            // We should ignore line height since it has no effect in SVG.
            stream.advance(1);
            stream.skip_spaces();
            let _ = stream.parse_length()?;
            stream.skip_spaces();
        }

        if stream.at_end() {
            return Err(Error::UnexpectedEndOfStream);
        }

        let font_family = stream.slice_tail();

        Ok(Self {
            font_style,
            font_variant,
            font_weight,
            font_stretch,
            font_size,
            font_family,
        })
    }
}

#[rustfmt::skip]
#[cfg(test)]
mod tests {
    use super::*;

    macro_rules! font_family {
        ($name:ident, $text:expr, $result:expr) => (
            #[test]
            fn $name() {
                assert_eq!(parse_font_families($text).unwrap(), $result);
            }
        )
    }

    macro_rules! named {
        ($text:expr) => (
            FontFamily::Named($text.to_string())
        )
    }

    const SERIF: FontFamily = FontFamily::Serif;
    const SANS_SERIF: FontFamily = FontFamily::SansSerif;
    const FANTASY: FontFamily = FontFamily::Fantasy;
    const MONOSPACE: FontFamily = FontFamily::Monospace;
    const CURSIVE: FontFamily = FontFamily::Cursive;

    font_family!(font_family_1, "Times New Roman", vec![named!("Times New Roman")]);
    font_family!(font_family_2, "serif", vec![SERIF]);
    font_family!(font_family_3, "sans-serif", vec![SANS_SERIF]);
    font_family!(font_family_4, "cursive", vec![CURSIVE]);
    font_family!(font_family_5, "fantasy", vec![FANTASY]);
    font_family!(font_family_6, "monospace", vec![MONOSPACE]);
    font_family!(font_family_7, "'Times New Roman'", vec![named!("Times New Roman")]);
    font_family!(font_family_8, "'Times New Roman', sans-serif", vec![named!("Times New Roman"), SANS_SERIF]);
    font_family!(font_family_9, "'Times New Roman', sans-serif", vec![named!("Times New Roman"), SANS_SERIF]);
    font_family!(font_family_10, "Arial, sans-serif, 'fantasy'", vec![named!("Arial"), SANS_SERIF, named!("fantasy")]);
    font_family!(font_family_11, "    Arial  , monospace  , 'fantasy'", vec![named!("Arial"), MONOSPACE, named!("fantasy")]);
    font_family!(font_family_12, "Times    New Roman", vec![named!("Times New Roman")]);
    font_family!(font_family_13, "\"Times New Roman\", sans-serif, sans-serif, \"Arial\"",
        vec![named!("Times New Roman"), SANS_SERIF, SANS_SERIF, named!("Arial")]
    );
    font_family!(font_family_14, "Times New Roman,,,Arial", vec![named!("Times New Roman"), named!("Arial")]);
    font_family!(font_family_15, "简体中文,sans-serif  , ,\"日本語フォント\",Arial",
        vec![named!("简体中文"), SANS_SERIF, named!("日本語フォント"), named!("Arial")]);

    font_family!(font_family_16, "", vec![]);

    macro_rules! font_family_err {
        ($name:ident, $text:expr, $result:expr) => (
            #[test]
            fn $name() {
                assert_eq!(parse_font_families($text).unwrap_err().to_string(), $result);
            }
        )
    }
    font_family_err!(font_family_err_1, "Red/Black, sans-serif", "invalid ident");
    font_family_err!(font_family_err_2, "\"Lucida\" Grande, sans-serif", "unexpected data at position 10");
    font_family_err!(font_family_err_3, "Ahem!, sans-serif", "invalid ident");
    font_family_err!(font_family_err_4, "test@foo, sans-serif", "invalid ident");
    font_family_err!(font_family_err_5, "#POUND, sans-serif", "invalid ident");
    font_family_err!(font_family_err_6, "Hawaii 5-0, sans-serif", "invalid ident");

    impl<'a> FontShorthand<'a> {
        fn new(font_style: Option<&'a str>, font_variant: Option<&'a str>, font_weight: Option<&'a str>,
                   font_stretch: Option<&'a str>, font_size: &'a str, font_family: &'a str) -> Self {
            Self {
                font_style, font_variant, font_weight, font_stretch, font_size, font_family
            }
        }
    }

    macro_rules! font_shorthand {
        ($name:ident, $text:expr, $result:expr) => (
            #[test]
            fn $name() {
                assert_eq!(FontShorthand::from_str($text).unwrap(), $result);
            }
        )
    }

    font_shorthand!(font_shorthand_1, "12pt/14pt sans-serif",
        FontShorthand::new(None, None, None, None, "12pt", "sans-serif"));
    font_shorthand!(font_shorthand_2, "80% sans-serif",
        FontShorthand::new(None, None, None, None, "80%", "sans-serif"));
    font_shorthand!(font_shorthand_3, "bold italic large Palatino, serif",
        FontShorthand::new(Some("italic"), None, Some("bold"), None, "large", "Palatino, serif"));
    font_shorthand!(font_shorthand_4, "x-large/110% \"new century schoolbook\", serif",
        FontShorthand::new(None, None, None, None, "x-large", "\"new century schoolbook\", serif"));
    font_shorthand!(font_shorthand_5, "normal small-caps 120%/120% fantasy",
        FontShorthand::new(None, Some("small-caps"), None, None, "120%", "fantasy"));
    font_shorthand!(font_shorthand_6, "condensed oblique 12pt \"Helvetica Neue\", serif",
        FontShorthand::new(Some("oblique"), None, None, Some("condensed"), "12pt", "\"Helvetica Neue\", serif"));
    font_shorthand!(font_shorthand_7, "italic 500 2em sans-serif, 'Noto Sans'",
        FontShorthand::new(Some("italic"), None, Some("500"), None, "2em", "sans-serif, 'Noto Sans'"));
    font_shorthand!(font_shorthand_8, "xx-large 'Noto Sans'",
        FontShorthand::new(None, None, None, None, "xx-large", "'Noto Sans'"));
    font_shorthand!(font_shorthand_9, "small-caps normal normal italic xx-small Times",
        FontShorthand::new(Some("italic"), Some("small-caps"), None, None, "xx-small", "Times"));


    macro_rules! font_shorthand_err {
        ($name:ident, $text:expr, $result:expr) => (
            #[test]
            fn $name() {
                assert_eq!(FontShorthand::from_str($text).unwrap_err(), $result);
            }
        )
    }

    font_shorthand_err!(font_shorthand_err_1, "", Error::UnexpectedEndOfStream);
    font_shorthand_err!(font_shorthand_err_2, "Noto Sans", Error::UnexpectedData(0));
    font_shorthand_err!(font_shorthand_err_3, "12pt  ", Error::UnexpectedEndOfStream);
    font_shorthand_err!(font_shorthand_err_4, "something 12pt 'Noto Sans'", Error::UnexpectedData(0));
    font_shorthand_err!(font_shorthand_err_5, "'Noto Sans' 13pt", Error::UnexpectedData(0));
    font_shorthand_err!(font_shorthand_err_6,
        "small-caps normal normal normal italic xx-large Times", Error::UnexpectedData(32));
}
