// Copyright 2021 the SVG Types Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT
// Modifications Copyright (c) 2026, Jiaqi (0x7c13) Liu.

use crate::Stream;
use crate::numeric_control::{Cursor, NumericParseError};
use crate::path::PathSource;

fn next_pair<I: PathSource>(
    source: &mut I,
) -> Result<(f64, f64), NumericParseError<I::ControlError>> {
    source.charge(1)?;
    let x = source.parse_list_number()?;
    let y = source.parse_list_number()?;
    Ok((x, y))
}

/// A pull-based [`<list-of-points>`] parser.
///
/// Use it for the `points` attribute of the `polygon` and `polyline` elements.
///
/// # Errors
///
/// - Stops on a first invalid character. Follows the same rules as paths parser.
///
/// # Notes
///
/// - If data contains an odd number of coordinates - the last one will be ignored.
///   As SVG spec states.
/// - It doesn't validate that there are more than two coordinate pairs,
///   which is required by the SVG spec.
///
/// # Examples
///
/// ```
/// use svgtypes::PointsParser;
///
/// let mut p = PointsParser::from("10 20 30 40");
/// assert_eq!(p.next(), Some((10.0, 20.0)));
/// assert_eq!(p.next(), Some((30.0, 40.0)));
/// assert_eq!(p.next(), None);
/// ```
///
/// [`<list-of-points>`]: https://www.w3.org/TR/SVG11/shapes.html#PointsBNF
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct PointsParser<'a>(Stream<'a>);

impl<'a> From<&'a str> for PointsParser<'a> {
    #[inline]
    fn from(v: &'a str) -> Self {
        PointsParser(Stream::from(v))
    }
}

impl Iterator for PointsParser<'_> {
    type Item = (f64, f64);

    fn next(&mut self) -> Option<Self::Item> {
        if self.0.at_end() {
            None
        } else {
            next_pair(&mut self.0).ok()
        }
    }
}

/// A controlled points pull parser. Syntax stops and an odd tail is ignored,
/// while caller control failure is transported unchanged and never ignored.
pub struct ControlledPointsParser<'a, F> {
    cursor: Cursor<'a, F>,
    terminal: bool,
}

impl<F> core::fmt::Debug for ControlledPointsParser<'_, F> {
    fn fmt(&self, formatter: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        formatter
            .debug_struct("ControlledPointsParser")
            .field("position", &self.cursor.position())
            .field("terminal", &self.terminal)
            .finish()
    }
}

impl<'a, E, F: FnMut(usize) -> Result<(), E>> ControlledPointsParser<'a, F> {
    /// Construct with persistent source byte-window credit.
    pub fn new(text: &'a str, checkpoint: F) -> Result<Self, NumericParseError<E>> {
        Ok(Self {
            cursor: Cursor::new(text, checkpoint)?,
            terminal: false,
        })
    }

    /// Current byte position, including an ignored final unpaired coordinate.
    pub fn position(&self) -> usize {
        self.cursor.position()
    }

    /// Pull one complete pair. No later pair is visited after Syntax or Control.
    pub fn try_next(&mut self) -> Result<Option<(f64, f64)>, NumericParseError<E>> {
        if self.terminal {
            return Ok(None);
        }
        let result = (|| {
            self.cursor.work(0)?;
            if self.cursor.at_end() {
                return Ok(None);
            }
            let pair = next_pair(&mut self.cursor)?;
            self.cursor.work(0)?;
            Ok(Some(pair))
        })();
        match result {
            Err(NumericParseError::Syntax(_)) | Ok(None) => {
                self.terminal = true;
                Ok(None)
            }
            Err(NumericParseError::Control(error)) => {
                self.terminal = true;
                Err(NumericParseError::Control(error))
            }
            value => value,
        }
    }
}

#[rustfmt::skip]
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_1() {
        let mut parser = PointsParser::from("10 20 30 40");
        assert_eq!(parser.next().unwrap(), (10.0, 20.0));
        assert_eq!(parser.next().unwrap(), (30.0, 40.0));
        assert!(parser.next().is_none());
    }

    #[test]
    fn parse_2() {
        let mut parser = PointsParser::from("10 20 30 40 50");
        assert_eq!(parser.next().unwrap(), (10.0, 20.0));
        assert_eq!(parser.next().unwrap(), (30.0, 40.0));
        assert!(parser.next().is_none());
    }
}

#[cfg(test)]
mod controlled_tests {
    use super::*;
    use alloc::{format, vec::Vec};

    #[test]
    fn controlled_points_preserve_bits_odd_tail_syntax_stop_and_consumption() {
        for text in [
            "",
            " ",
            "1 2",
            "1 2 3",
            "-0 +0 1e-300 2e300",
            "1,2,3,4,",
            "1 2 3 ,",
            "1 2 3,",
            "1 2 3,é",
            "1 2é 4",
            "é",
            "1em 2",
            "1 2 + 4",
        ] {
            let mut ordinary = PointsParser::from(text);
            let expected = ordinary
                .by_ref()
                .map(|(x, y)| (x.to_bits(), y.to_bits()))
                .collect::<Vec<_>>();
            let mut parser =
                ControlledPointsParser::new(text, |_| Ok::<(), &'static str>(())).unwrap();
            let mut actual = Vec::new();
            while let Some((x, y)) = parser.try_next().unwrap() {
                actual.push((x.to_bits(), y.to_bits()));
            }
            assert_eq!(actual, expected, "{text}");
            assert_eq!(parser.position(), ordinary.0.pos(), "{text}");
        }
    }

    #[test]
    fn controlled_points_do_not_swallow_cancel_or_read_the_second_source_window() {
        let text = format!("1{}2 3 4", " ".repeat(8193));
        let mut windows = 0;
        let mut parser = ControlledPointsParser::new(&text, |work| {
            if work > 256 {
                windows += 1;
                if windows == 2 {
                    return Err("cancel-second-window");
                }
            }
            Ok(())
        })
        .unwrap();
        assert_eq!(
            parser.try_next(),
            Err(NumericParseError::Control("cancel-second-window"))
        );
        assert_eq!(parser.position(), 4096);
        assert_eq!(parser.cursor.lexical_read_end(), 4096);
        assert!(parser.try_next().unwrap().is_none());
    }
}
