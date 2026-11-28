// Copyright 2018 the SVG Types Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT
// Modifications Copyright (c) 2026, Jiaqi (0x7c13) Liu.
//! Source-window control for number/length list lexing. Decimal conversion still
//! uses the unchanged, opaque `f64::from_str` call after the token is scanned.

use crate::{ByteExt, Error, Length, Stream};
use core::convert::Infallible;
use core::str::FromStr;

/// A numeric-list syntax error or an unchanged caller control error.
#[derive(Debug, PartialEq, Eq)]
pub enum NumericParseError<E> {
    /// The SVG number or list syntax is invalid.
    Syntax(Error),
    /// The caller rejected scanning or a value. No syntax recovery is implied.
    Control(E),
}

impl<E> From<Error> for NumericParseError<E> {
    fn from(error: Error) -> Self {
        Self::Syntax(error)
    }
}

/// A request-controlled pull parser for `<list-of-numbers>`.
///
/// Uses the ordinary number and separator grammar without reading later values.
/// One cursor retains its 4096-byte prepaid windows across all calls. A value is
/// returned only after its separator has also been scanned successfully.
/// Decimal conversion remains the unchanged opaque `f64::from_str` operation.
pub struct ControlledNumberListParser<'a, F> {
    cursor: Cursor<'a, F>,
    terminal: bool,
}

impl<F> core::fmt::Debug for ControlledNumberListParser<'_, F> {
    fn fmt(&self, formatter: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        formatter
            .debug_struct("ControlledNumberListParser")
            .field("position", &self.cursor.pos)
            .field("prepaid", &self.cursor.prepaid)
            .field("terminal", &self.terminal)
            .finish()
    }
}

impl<'a, E, F: FnMut(usize) -> Result<(), E>> ControlledNumberListParser<'a, F> {
    /// Checks the caller with zero work before constructing the parser.
    pub fn new(text: &'a str, checkpoint: F) -> Result<Self, NumericParseError<E>> {
        Ok(Self {
            cursor: Cursor::new(text, checkpoint)?,
            terminal: false,
        })
    }

    /// Current byte offset. Syntax advances directly to the input end, as in
    /// `NumberListParser`; Control retains the last consumed position.
    pub fn position(&self) -> usize {
        self.cursor.pos
    }
}

impl<E, F: FnMut(usize) -> Result<(), E>> Iterator for ControlledNumberListParser<'_, F> {
    type Item = Result<f64, NumericParseError<E>>;

    fn next(&mut self) -> Option<Self::Item> {
        if self.terminal || self.cursor.pos == self.cursor.text.len() {
            return None;
        }
        let result = self.cursor.number().and_then(|number| {
            self.cursor.separator()?;
            Ok(number)
        });
        match &result {
            Err(NumericParseError::Syntax(_)) => {
                // Position-only jump: never scan a tail after the first error.
                self.cursor.pos = self.cursor.text.len();
                self.terminal = true;
            }
            Err(NumericParseError::Control(_)) => self.terminal = true,
            Ok(_) => {}
        }
        Some(result)
    }
}

impl<E, F: FnMut(usize) -> Result<(), E>> core::iter::FusedIterator
    for ControlledNumberListParser<'_, F>
{
}

#[cfg(test)]
mod pull_tests {
    use super::*;
    use alloc::{format, vec::Vec};
    use core::cell::Cell;

    #[test]
    fn pull_matches_each_ordinary_result_bits_position_and_fused_error() {
        for input in [
            "",
            " ",
            "1",
            "1,",
            "1, ",
            "0 -0 +1 -.5 1.e2 2e-2",
            "1 2 bad 4",
            "1 bad 4",
            "1,,2",
            "1, ,2",
            "1-2+3",
            ".5.25",
            "1e",
            "1E",
            "1e+",
            "1e-2",
            "1em",
            "1ex",
            "1e9999",
            "1 č",
            "1\u{a0}2",
            "č1",
            "1 2 3 4",
        ] {
            let calls = Cell::new(0usize);
            let mut ordinary = crate::NumberListParser::from(input);
            let mut controlled = ControlledNumberListParser::new(input, |_| {
                calls.set(calls.get() + 1);
                Ok::<(), Infallible>(())
            })
            .unwrap();
            loop {
                let expected = ordinary.next();
                let actual = controlled.next();
                match (expected, actual) {
                    (Some(Ok(expected)), Some(Ok(actual))) => {
                        assert_eq!(actual.to_bits(), expected.to_bits(), "{input}");
                    }
                    (Some(Err(expected)), Some(Err(NumericParseError::Syntax(actual)))) => {
                        assert_eq!(actual, expected, "{input}");
                    }
                    (None, None) => break,
                    other => panic!("different next result for {input:?}: {other:?}"),
                }
                assert_eq!(controlled.position(), ordinary.position(), "{input}");
            }
            assert_eq!(controlled.position(), ordinary.position(), "{input}");
            let before = calls.get();
            for _ in 0..3 {
                assert!(controlled.next().is_none());
            }
            assert_eq!(calls.get(), before);
        }
    }

    #[test]
    fn pull_keeps_independent_known_bits_and_syntax_position() {
        let mut parser =
            ControlledNumberListParser::new("-0 1e-2 bad ignored", |_| Ok::<(), Infallible>(()))
                .unwrap();
        assert_eq!(
            parser.next().unwrap().unwrap().to_bits(),
            0x8000_0000_0000_0000
        );
        assert_eq!(
            parser.next().unwrap().unwrap().to_bits(),
            0x3f84_7ae1_47ae_147b
        );
        assert_eq!(
            parser.next(),
            Some(Err(NumericParseError::Syntax(Error::InvalidNumber(9))))
        );
        assert_eq!(parser.position(), 19);
        assert!(parser.next().is_none());
    }

    #[test]
    fn pull_does_not_scan_ignored_tokens_or_error_tail() {
        let input = format!("1 2 3{}", "9".repeat(131073));
        let calls = Cell::new(0usize);
        let charged = Cell::new(0usize);
        let mut parser = ControlledNumberListParser::new(&input, |bytes| {
            calls.set(calls.get() + 1);
            charged.set(charged.get() + bytes);
            Ok::<(), i32>(())
        })
        .unwrap();
        assert_eq!(parser.next(), Some(Ok(1.0)));
        assert_eq!(parser.next(), Some(Ok(2.0)));
        assert_eq!(parser.position(), 4);
        assert_eq!(calls.get(), 2 + 2 * 2);
        assert_eq!(charged.get(), 4096 + 2);
        assert!(parser.cursor.max_lexical_read_end <= 5);
        drop(parser);

        let input = format!("1 bad{}", "9".repeat(131073));
        let charged = Cell::new(0usize);
        let mut parser = ControlledNumberListParser::new(&input, |bytes| {
            charged.set(charged.get() + bytes);
            Ok::<(), i32>(())
        })
        .unwrap();
        assert_eq!(parser.next(), Some(Ok(1.0)));
        assert_eq!(
            parser.next(),
            Some(Err(NumericParseError::Syntax(Error::InvalidNumber(3))))
        );
        assert_eq!(parser.position(), input.len());
        assert!(parser.next().is_none());
        assert_eq!(charged.get(), 4096 + 1);
        assert!(parser.cursor.max_lexical_read_end <= 3);
    }

    #[test]
    fn pull_rejects_every_read_window_before_lexical_read_at_4095_4096_4097() {
        for length in [4095usize, 4096, 4097] {
            let input = format!("0.{}", "0".repeat(length - 2));
            for rejected in 0..length.div_ceil(4096) {
                let approved = Cell::new(0usize);
                let windows = Cell::new(0usize);
                let mut parser = ControlledNumberListParser::new(&input, |bytes| {
                    if bytes == 0 {
                        return Ok(());
                    }
                    let window = windows.get();
                    windows.set(window + 1);
                    assert!(bytes <= 4096);
                    if window == rejected {
                        return Err(-2718);
                    }
                    approved.set(approved.get() + bytes);
                    Ok(())
                })
                .unwrap();
                assert_eq!(parser.next(), Some(Err(NumericParseError::Control(-2718))));
                assert!(parser.cursor.max_lexical_read_end <= approved.get());
                assert_eq!(parser.cursor.prepaid, approved.get());
                assert_eq!(windows.get(), rejected + 1);
                let position = parser.position();
                assert!(parser.next().is_none());
                assert_eq!(parser.position(), position);
                assert_eq!(windows.get(), rejected + 1);
            }
        }
    }

    #[test]
    fn pull_separator_control_cannot_publish_the_preceding_number() {
        let input = format!("1{}2", " ".repeat(8193));
        let approved = Cell::new(0usize);
        let mut parser = ControlledNumberListParser::new(&input, |bytes| {
            let next_window = (input.len() - approved.get()).min(4096);
            if bytes == 0 || bytes != next_window {
                // Conversion has its separate prepaid pass and zero tail.
                // This test rejects the second lexical source window.
                return Ok(());
            }
            if approved.get() != 0 {
                return Err(73);
            }
            approved.set(approved.get() + bytes);
            Ok(())
        })
        .unwrap();
        assert_eq!(parser.next(), Some(Err(NumericParseError::Control(73))));
        assert_eq!(parser.position(), 4096);
        assert!(parser.cursor.max_lexical_read_end <= 4096);
        assert!(parser.next().is_none());
    }

    #[test]
    fn pull_reuses_windows_across_values_and_consumes_required_separators() {
        let input = "1 ".repeat(3000);
        let calls = Cell::new(0usize);
        let charged = Cell::new(0usize);
        let parser = ControlledNumberListParser::new(&input, |bytes| {
            calls.set(calls.get() + 1);
            charged.set(charged.get() + bytes);
            Ok::<(), Infallible>(())
        })
        .unwrap();
        assert_eq!(parser.map(|value| value.unwrap()).sum::<f64>(), 3000.0);
        assert_eq!(calls.get(), 3 + 2 * 3000);
        assert_eq!(charged.get(), 6000 + 3000);

        let input = format!("1{}2 tail", " ".repeat(65537));
        let charged = Cell::new(0usize);
        let mut parser = ControlledNumberListParser::new(&input, |bytes| {
            charged.set(charged.get() + bytes);
            Ok::<(), Infallible>(())
        })
        .unwrap();
        assert_eq!(parser.next(), Some(Ok(1.0)));
        assert_eq!(parser.position(), 65538);
        assert!(charged.get() > 65536);
        assert_eq!(parser.next(), Some(Ok(2.0)));
    }

    #[test]
    fn pull_preserves_noncopy_typed_control_and_checks_even_empty_input() {
        #[derive(Debug, PartialEq)]
        enum Rejection {
            Cancelled,
            Work,
            Native(i32),
        }
        for error in [
            Rejection::Cancelled,
            Rejection::Work,
            Rejection::Native(-991),
        ] {
            let mut failure = Some(error);
            let result = ControlledNumberListParser::new("", |_| Err(failure.take().unwrap()));
            assert!(matches!(result, Err(NumericParseError::Control(_))));
            assert!(failure.is_none());
        }
        let input = "1 ".repeat(10000);
        let mut charges = Vec::new();
        let mut parser = ControlledNumberListParser::new(&input, |bytes| {
            charges.push(bytes);
            if bytes != 0 {
                Err(Rejection::Native(-992))
            } else {
                Ok(())
            }
        })
        .unwrap();
        assert_eq!(
            parser.next(),
            Some(Err(NumericParseError::Control(Rejection::Native(-992))))
        );
        assert!(parser.next().is_none());
        drop(parser);
        assert_eq!(charges, [0, 4096]);
    }
}

#[cfg(test)]
mod opaque_conversion_tests {
    use super::*;
    use alloc::format;
    use core::cell::Cell;

    #[test]
    fn opaque_conversion_prepays_the_whole_second_pass_before_a_value() {
        let token = format!("0.{}1", "0".repeat(65537));
        let source = format!("{token} 2");
        let hit = Cell::new(false);
        let mut parser = ControlledNumberListParser::new(&source, |work| {
            if work == token.len() {
                hit.set(true);
                return Err(701);
            }
            Ok(())
        })
        .unwrap();
        assert_eq!(parser.next(), Some(Err(NumericParseError::Control(701))));
        assert!(hit.get());
        assert!(parser.next().is_none());
    }

    #[test]
    fn opaque_conversion_postcheck_precedes_value_or_syntax_publication() {
        for source in ["+1.25", "1e9999"] {
            let full_slice_charges = Cell::new(0);
            let postcheck_hit = Cell::new(false);
            let mut parser = ControlledNumberListParser::new(source, |work| {
                if full_slice_charges.get() == 2 && work == 0 {
                    postcheck_hit.set(true);
                    return Err(702);
                }
                if work == source.len() {
                    // A short source has both a lexical window and a second
                    // pass of this size. Only the second one arms rejection.
                    full_slice_charges.set(full_slice_charges.get() + 1);
                }
                Ok(())
            })
            .unwrap();
            assert_eq!(parser.next(), Some(Err(NumericParseError::Control(702))));
            assert!(postcheck_hit.get());
            assert!(parser.next().is_none());
        }
    }
}

/// Visits a number list while prepaying lexing in at most 4096-byte windows.
///
/// A zero-work check precedes even empty input. One cursor retains prepaid
/// lookahead across all values. The final window may include unconsumed syntax
/// after an error; this is conservative prepayment, not an exact read count.
/// Visitor errors and scan errors are returned unchanged as `Control`.
/// No values are buffered here. `f64::from_str` remains an opaque conversion of
/// a complete token; these checkpoints do not make that call interruptible.
pub fn parse_number_list_with_control<E>(
    text: &str,
    checkpoint: impl FnMut(usize) -> Result<(), E>,
    emit: impl FnMut(f64) -> Result<(), E>,
) -> Result<(), NumericParseError<E>> {
    Cursor::new(text, checkpoint)?.numbers(emit)
}

/// Visits a length list using the same prepaid numeric lexer and SVG units.
///
/// The caller may retain visited values on `Syntax` to reproduce the original
/// length-list recovery. A `Control` error must never take that recovery path.
/// The decimal-conversion limitation documented on the number visitor applies.
pub fn parse_length_list_with_control<E>(
    text: &str,
    checkpoint: impl FnMut(usize) -> Result<(), E>,
    emit: impl FnMut(Length) -> Result<(), E>,
) -> Result<(), NumericParseError<E>> {
    Cursor::new(text, checkpoint)?.lengths(emit)
}

// The ordinary Stream wrapper retains its whitespace handling and character
// error position. Always transfer consumption, including a syntax failure.
pub(crate) fn number_from_stream(stream: &mut Stream<'_>) -> Result<f64, Error> {
    let mut cursor = match Cursor::new(stream.slice_tail(), |_| Ok::<(), Infallible>(())) {
        Ok(cursor) => cursor,
        Err(NumericParseError::Syntax(error)) => return Err(error),
        Err(NumericParseError::Control(never)) => match never {},
    };
    let result = cursor.number_impl();
    stream.advance(cursor.pos);
    match result {
        Ok(number) => Ok(number),
        Err(NumericParseError::Syntax(error)) => Err(error),
        Err(NumericParseError::Control(never)) => match never {},
    }
}

pub(crate) struct Cursor<'a, F> {
    text: &'a str,
    pos: usize,
    prepaid: usize,
    checkpoint: F,
    #[cfg(test)]
    max_lexical_read_end: usize,
}

impl<F> Cursor<'_, F> {
    pub(crate) fn position(&self) -> usize {
        self.pos
    }
    #[cfg(test)]
    pub(crate) fn lexical_read_end(&self) -> usize {
        self.max_lexical_read_end
    }
}

impl<'a, E, F: FnMut(usize) -> Result<(), E>> Cursor<'a, F> {
    pub(crate) fn new(text: &'a str, mut checkpoint: F) -> Result<Self, NumericParseError<E>> {
        checkpoint(0).map_err(NumericParseError::Control)?;
        Ok(Cursor {
            text,
            pos: 0,
            prepaid: 0,
            checkpoint,
            #[cfg(test)]
            max_lexical_read_end: 0,
        })
    }

    pub(crate) fn byte(&mut self, offset: usize) -> Result<Option<u8>, NumericParseError<E>> {
        // Every caller has offset 0 or 1 and pos <= len; checked_add also keeps
        // lookahead well-defined at the platform's maximum possible slice size.
        let Some(index) = self.pos.checked_add(offset) else {
            return Ok(None);
        };
        if index >= self.text.len() {
            return Ok(None);
        }
        while index >= self.prepaid {
            let bytes = (self.text.len() - self.prepaid).min(4096);
            (self.checkpoint)(bytes).map_err(NumericParseError::Control)?;
            self.prepaid += bytes;
        }
        #[cfg(test)]
        {
            self.max_lexical_read_end = self.max_lexical_read_end.max(index + 1);
        }
        Ok(Some(self.text.as_bytes()[index]))
    }

    pub(crate) fn required_byte(&mut self, offset: usize) -> Result<u8, NumericParseError<E>> {
        self.byte(offset)?
            .ok_or_else(|| Error::UnexpectedEndOfStream.into())
    }

    pub(crate) fn skip_spaces(&mut self) -> Result<(), NumericParseError<E>> {
        while self.byte(0)?.is_some_and(|byte| byte.is_space()) {
            self.pos += 1;
        }
        Ok(())
    }

    fn skip_digits(&mut self) -> Result<(), NumericParseError<E>> {
        while self.byte(0)?.is_some_and(|byte| byte.is_digit()) {
            self.pos += 1;
        }
        Ok(())
    }

    fn number_impl(&mut self) -> Result<f64, NumericParseError<E>> {
        let start = self.pos;
        let mut byte = self.required_byte(0)?;
        if byte.is_sign() {
            self.pos += 1;
            byte = self.required_byte(0)?;
        }
        match byte {
            b'0'..=b'9' => self.skip_digits()?,
            b'.' => {}
            _ => return Err(Error::InvalidNumber(0).into()),
        }
        if self.byte(0)? == Some(b'.') {
            self.pos += 1;
            self.skip_digits()?;
        }
        if let Some(b'e' | b'E') = self.byte(0)? {
            let next = self.required_byte(1)?;
            // The legacy SVG grammar leaves em/ex for length-unit parsing.
            if next != b'm' && next != b'x' {
                self.pos += 1;
                match self.required_byte(0)? {
                    b'+' | b'-' => {
                        self.pos += 1;
                        self.skip_digits()?;
                    }
                    b'0'..=b'9' => self.skip_digits()?,
                    _ => return Err(Error::InvalidNumber(0).into()),
                }
            }
        }
        // StdFloatOpaqueV1: retain the ordinary complete-token conversion,
        // separately prepay its second input pass, and check after it before
        // publishing either a value or a syntax result. The std call itself
        // remains opaque; lexical windows do not make it interruptible.
        (self.checkpoint)(self.pos - start).map_err(NumericParseError::Control)?;
        let parsed = f64::from_str(&self.text[start..self.pos]);
        (self.checkpoint)(0).map_err(NumericParseError::Control)?;
        if let Ok(number) = parsed {
            if number.is_finite() {
                return Ok(number);
            }
        }
        Err(Error::InvalidNumber(0).into())
    }

    fn number(&mut self) -> Result<f64, NumericParseError<E>> {
        self.skip_spaces()?;
        let start = self.pos;
        match self.number_impl() {
            Ok(number) => Ok(number),
            // A controlled list starts at zero and only consumes ASCII syntax.
            // Thus start+1 is the original character position without a second,
            // uncontrolled scan over a potentially very long valid prefix.
            Err(NumericParseError::Syntax(_)) => Err(Error::InvalidNumber(start + 1).into()),
            Err(NumericParseError::Control(error)) => Err(NumericParseError::Control(error)),
        }
    }

    pub(crate) fn at_end(&self) -> bool {
        self.pos >= self.text.len()
    }

    pub(crate) fn advance_ascii(&mut self) {
        // The caller has successfully read this one ASCII grammar byte.
        self.pos += 1;
    }

    pub(crate) fn jump_to_end(&mut self) {
        // A syntax failure terminates without reading the remaining tail.
        self.pos = self.text.len();
    }

    pub(crate) fn work(&mut self, work: usize) -> Result<(), NumericParseError<E>> {
        (self.checkpoint)(work).map_err(NumericParseError::Control)
    }

    pub(crate) fn list_number(&mut self) -> Result<f64, NumericParseError<E>> {
        if self.at_end() {
            return Err(Error::UnexpectedEndOfStream.into());
        }
        let value = self.number()?;
        self.separator()?;
        Ok(value)
    }

    fn separator(&mut self) -> Result<(), NumericParseError<E>> {
        self.skip_spaces()?;
        if self.byte(0)? == Some(b',') {
            self.pos += 1;
        }
        Ok(())
    }

    fn numbers(
        &mut self,
        mut emit: impl FnMut(f64) -> Result<(), E>,
    ) -> Result<(), NumericParseError<E>> {
        while self.pos < self.text.len() {
            let number = self.number()?;
            self.separator()?;
            emit(number).map_err(NumericParseError::Control)?;
        }
        Ok(())
    }

    fn lengths(
        &mut self,
        mut emit: impl FnMut(Length) -> Result<(), E>,
    ) -> Result<(), NumericParseError<E>> {
        while self.pos < self.text.len() {
            let number = self.number()?;
            // Do not inspect beyond the finite unit lookahead. In particular,
            // '%' needs one byte, while the remaining original units need two.
            let first = self.byte(0)?;
            let second = if first.is_some() && first != Some(b'%') {
                self.byte(1)?
            } else {
                None
            };
            let bytes = [first.unwrap_or(0), second.unwrap_or(0)];
            let available = usize::from(first.is_some()) + usize::from(second.is_some());
            let (unit, consumed) = crate::length::length_unit(&bytes[..available]);
            self.pos += consumed;
            self.separator()?;
            emit(Length::new(number, unit)).map_err(NumericParseError::Control)?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use alloc::{format, vec, vec::Vec};
    use core::cell::Cell;

    fn number_values(input: &str) -> Result<Vec<u64>, NumericParseError<Infallible>> {
        let mut result = Vec::new();
        parse_number_list_with_control(
            input,
            |_| Ok(()),
            |number| {
                result.push(number.to_bits());
                Ok(())
            },
        )?;
        Ok(result)
    }

    #[test]
    fn independent_number_bits_and_syntax_match_original_contract() {
        assert_eq!(
            number_values("0 -0 +1 -.5 1.e2 2e-2").unwrap(),
            vec![
                0,
                0x8000_0000_0000_0000,
                0x3ff0_0000_0000_0000,
                0xbfe0_0000_0000_0000,
                0x4059_0000_0000_0000,
                0x3f94_7ae1_47ae_147b
            ]
        );
        assert_eq!(number_values("").unwrap(), vec![]);
        assert_eq!(number_values("1,").unwrap(), vec![1.0f64.to_bits()]);
        for (input, position) in [
            (" ", 2),
            ("1, ", 4),
            ("1em", 2),
            ("1e", 1),
            ("1e+", 1),
            ("1E", 1),
            ("1 č", 3),
            ("1e9999", 1),
        ] {
            assert_eq!(
                number_values(input),
                Err(NumericParseError::Syntax(Error::InvalidNumber(position))),
                "{input}"
            );
        }
    }

    #[test]
    fn ordinary_stream_keeps_error_consumption_and_unicode_character_position() {
        for (input, expected_position) in [("1e", 1), ("1E", 1), ("1e+", 3), ("-q", 1), (".", 1)] {
            let mut stream = Stream::from(input);
            assert_eq!(stream.parse_number(), Err(Error::InvalidNumber(1)));
            assert_eq!(stream.pos(), expected_position, "{input}");
        }
        let mut stream = Stream::from("é1e+tail");
        stream.advance(2);
        assert_eq!(stream.parse_number(), Err(Error::InvalidNumber(2)));
        assert_eq!(stream.pos(), 5);
    }

    #[test]
    fn long_number_boundaries_have_independent_zero_and_one_oracles() {
        for count in [4095, 4096, 4097, 65535, 65536, 65537, 131073] {
            let zeros = "0".repeat(count);
            for (input, expected) in [
                (format!("0.{zeros}1 1"), vec![0, 0x3ff0_0000_0000_0000]),
                (format!("1.{zeros}"), vec![0x3ff0_0000_0000_0000]),
                (format!("{zeros}1"), vec![0x3ff0_0000_0000_0000]),
                (format!("1e+{zeros}0"), vec![0x3ff0_0000_0000_0000]),
                (
                    format!("{}1", " ".repeat(count)),
                    vec![0x3ff0_0000_0000_0000],
                ),
            ] {
                assert_eq!(number_values(&input).unwrap(), expected);
            }
            let mut values = Vec::new();
            let source = format!("0.{zeros}1px,1%");
            parse_length_list_with_control(
                &source,
                |_| Ok::<(), Infallible>(()),
                |value| {
                    values.push((value.number.to_bits(), value.unit));
                    Ok(())
                },
            )
            .unwrap();
            assert_eq!(
                values,
                vec![
                    (0, crate::LengthUnit::Px),
                    (0x3ff0_0000_0000_0000, crate::LengthUnit::Percent)
                ]
            );
        }
    }

    #[test]
    fn lexical_reads_stop_before_every_rejected_source_window() {
        let zeros = "0".repeat(131073);
        let sources = [
            format!("0.{zeros}1 1"),
            format!("{zeros}1"),
            format!("1e+{zeros}0"),
            format!("{}1", " ".repeat(131073)),
            format!("1 {}č", " ".repeat(131073)),
        ];
        for source in sources {
            let windows = source.len().div_ceil(4096);
            for rejected in 0..windows {
                let calls = Cell::new(0usize);
                let approved = Cell::new(0usize);
                let mut cursor = Cursor::new(&source, |bytes| {
                    if bytes == 0 {
                        return Ok(());
                    }
                    if bytes != (source.len() - approved.get()).min(4096) {
                        // Numeric conversion is charged independently from
                        // the lexical windows exercised by this test.
                        return Ok(());
                    }
                    let call = calls.get();
                    calls.set(call + 1);
                    assert!(bytes <= 4096);
                    if call == rejected {
                        return Err(-2718);
                    }
                    approved.set(approved.get() + bytes);
                    Ok(())
                })
                .unwrap();
                assert_eq!(
                    cursor.numbers(|_| Ok(())),
                    Err(NumericParseError::Control(-2718))
                );
                assert!(cursor.max_lexical_read_end <= approved.get());
                assert_eq!(cursor.prepaid, approved.get());
                assert_eq!(calls.get(), rejected + 1);
            }
        }
    }

    #[test]
    fn control_survives_fraction_exponent_unit_and_separator_lookahead() {
        for tail in [".0", "e0", "e+0", "em", "px", "% 1", ",1", " č"] {
            let source = format!("{}1{tail}", " ".repeat(4094));
            let approved = Cell::new(0usize);
            let mut cursor = Cursor::new(&source, |bytes| {
                if bytes != 0 && approved.get() != 0 {
                    return Err(19);
                }
                approved.set(approved.get() + bytes);
                Ok(())
            })
            .unwrap();
            let result = cursor.lengths(|_| Ok(()));
            assert_eq!(result, Err(NumericParseError::Control(19)), "{tail}");
            assert!(cursor.max_lexical_read_end <= approved.get());
        }
    }

    #[test]
    fn credit_is_shared_across_values_and_visitor_errors_are_control() {
        let source = "1 ".repeat(10000);
        let mut charged = Vec::new();
        let mut emitted = 0;
        parse_number_list_with_control(
            &source,
            |bytes| {
                charged.push(bytes);
                Ok::<_, i32>(())
            },
            |_| {
                emitted += 1;
                Ok(())
            },
        )
        .unwrap();
        assert_eq!(emitted, 10000);
        assert_eq!(charged.iter().sum::<usize>(), source.len() + emitted);
        assert_eq!(charged.len(), 1 + source.len().div_ceil(4096) + 2 * emitted);
        assert_eq!(
            parse_number_list_with_control("1 2", |_| Ok(()), |_| Err(73)),
            Err(NumericParseError::Control(73))
        );
        assert_eq!(
            parse_length_list_with_control("1px 2px", |_| Ok(()), |_| Err(74)),
            Err(NumericParseError::Control(74))
        );
        assert_eq!(
            parse_number_list_with_control("", |_| Err(75), |_| Ok(())),
            Err(NumericParseError::Control(75))
        );
    }

    #[test]
    fn length_syntax_preserves_emitted_prefix_without_converting_control() {
        for source in ["1 px", "1PX", "1px garbage", "1px, "] {
            let mut values = Vec::new();
            let result = parse_length_list_with_control(
                source,
                |_| Ok::<(), Infallible>(()),
                |value| {
                    values.push((value.number.to_bits(), value.unit));
                    Ok(())
                },
            );
            assert!(matches!(result, Err(NumericParseError::Syntax(_))));
            assert_eq!(values.len(), 1, "{source}");
        }
        let mut values = Vec::new();
        parse_length_list_with_control(
            "1em 1ex 1% -0cm",
            |_| Ok::<(), Infallible>(()),
            |value| {
                values.push(value);
                Ok(())
            },
        )
        .unwrap();
        assert_eq!(values.len(), 4);
        assert_eq!(values[0].unit, crate::LengthUnit::Em);
        assert_eq!(values[1].unit, crate::LengthUnit::Ex);
        assert_eq!(values[2].unit, crate::LengthUnit::Percent);
        assert_eq!(values[3].number.to_bits(), 0x8000_0000_0000_0000);
    }
}
