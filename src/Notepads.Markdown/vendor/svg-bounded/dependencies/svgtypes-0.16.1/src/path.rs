// Copyright 2021 the SVG Types Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT
// Modifications Copyright (c) 2026, Jiaqi (0x7c13) Liu.

use crate::numeric_control::{Cursor, NumericParseError};
use crate::{Error, Stream};
use alloc::vec::Vec;
use core::convert::Infallible;
use markdown_svg_budget::{BudgetVec, Error as ResourceError, ResourceKind, ResourcesRef};

// A single command/flag grammar over the ordinary Stream and prepaid Cursor.
// Decimal syntax is already shared by Stream::parse_number and Cursor.
pub(crate) trait PathSource {
    type ControlError;
    fn pos(&self) -> usize;
    fn char_pos(&self, position: usize) -> usize;
    fn curr_byte(&mut self) -> Result<u8, NumericParseError<Self::ControlError>>;
    fn byte_is(&mut self, byte: u8) -> Result<bool, NumericParseError<Self::ControlError>>;
    fn advance(&mut self, count: usize);
    fn skip_spaces(&mut self) -> Result<(), NumericParseError<Self::ControlError>>;
    fn parse_list_number(&mut self) -> Result<f64, NumericParseError<Self::ControlError>>;
    fn charge(&mut self, work: usize) -> Result<(), NumericParseError<Self::ControlError>>;
}

impl PathSource for Stream<'_> {
    type ControlError = Infallible;
    fn pos(&self) -> usize {
        Stream::pos(self)
    }
    fn char_pos(&self, position: usize) -> usize {
        self.calc_char_pos_at(position)
    }
    fn curr_byte(&mut self) -> Result<u8, NumericParseError<Infallible>> {
        Stream::curr_byte(self).map_err(NumericParseError::Syntax)
    }
    fn byte_is(&mut self, byte: u8) -> Result<bool, NumericParseError<Infallible>> {
        Ok(self.is_curr_byte_eq(byte))
    }
    fn advance(&mut self, count: usize) {
        Stream::advance(self, count);
    }
    fn skip_spaces(&mut self) -> Result<(), NumericParseError<Infallible>> {
        Stream::skip_spaces(self);
        Ok(())
    }
    fn parse_list_number(&mut self) -> Result<f64, NumericParseError<Infallible>> {
        Stream::parse_list_number(self).map_err(NumericParseError::Syntax)
    }
    fn charge(&mut self, _work: usize) -> Result<(), NumericParseError<Infallible>> {
        Ok(())
    }
}

impl<E, F: FnMut(usize) -> Result<(), E>> PathSource for Cursor<'_, F> {
    type ControlError = E;
    fn pos(&self) -> usize {
        self.position()
    }
    fn char_pos(&self, position: usize) -> usize {
        // A path has only consumed ASCII grammar before its first error.
        // Do not rescan that potentially long prefix to report its position.
        position + 1
    }
    fn curr_byte(&mut self) -> Result<u8, NumericParseError<E>> {
        self.required_byte(0)
    }
    fn byte_is(&mut self, byte: u8) -> Result<bool, NumericParseError<E>> {
        Ok(self.byte(0)? == Some(byte))
    }
    fn advance(&mut self, count: usize) {
        debug_assert_eq!(count, 1);
        self.advance_ascii();
    }
    fn skip_spaces(&mut self) -> Result<(), NumericParseError<E>> {
        Cursor::skip_spaces(self)
    }
    fn parse_list_number(&mut self) -> Result<f64, NumericParseError<E>> {
        self.list_number()
    }
    fn charge(&mut self, work: usize) -> Result<(), NumericParseError<E>> {
        self.work(work)
    }
}

/// Representation of a path segment.
///
/// If you want to change the segment type (for example `MoveTo` to `LineTo`)
/// you should create a new segment.
/// But you still can change points or make segment relative or absolute.
#[allow(missing_docs)]
#[derive(Clone, Copy, PartialEq, Debug)]
pub enum PathSegment {
    MoveTo {
        abs: bool,
        x: f64,
        y: f64,
    },
    LineTo {
        abs: bool,
        x: f64,
        y: f64,
    },
    HorizontalLineTo {
        abs: bool,
        x: f64,
    },
    VerticalLineTo {
        abs: bool,
        y: f64,
    },
    CurveTo {
        abs: bool,
        x1: f64,
        y1: f64,
        x2: f64,
        y2: f64,
        x: f64,
        y: f64,
    },
    SmoothCurveTo {
        abs: bool,
        x2: f64,
        y2: f64,
        x: f64,
        y: f64,
    },
    Quadratic {
        abs: bool,
        x1: f64,
        y1: f64,
        x: f64,
        y: f64,
    },
    SmoothQuadratic {
        abs: bool,
        x: f64,
        y: f64,
    },
    EllipticalArc {
        abs: bool,
        rx: f64,
        ry: f64,
        x_axis_rotation: f64,
        large_arc: bool,
        sweep: bool,
        x: f64,
        y: f64,
    },
    ClosePath {
        abs: bool,
    },
}

impl PathSegment {
    /// Returns `true` if the segment is absolute.
    ///
    /// # Examples
    ///
    /// ```
    /// use svgtypes::{PathParser, PathSegment};
    ///
    /// let mut segments = Vec::new();
    /// for segment in PathParser::from("M10-20l30.1.5.1-20z") {
    ///     segments.push(segment.unwrap());
    /// }
    ///
    /// assert_eq!(segments[0].is_abs(), true);
    /// assert_eq!(segments[1].is_abs(), false);
    /// assert_eq!(segments[2].is_abs(), false);
    /// assert_eq!(segments[3].is_abs(), false);
    /// ```
    ///
    pub fn is_abs(&self) -> bool {
        match *self {
            Self::MoveTo { abs, .. } => abs,
            Self::LineTo { abs, .. } => abs,
            Self::HorizontalLineTo { abs, .. } => abs,
            Self::VerticalLineTo { abs, .. } => abs,
            Self::CurveTo { abs, .. } => abs,
            Self::SmoothCurveTo { abs, .. } => abs,
            Self::Quadratic { abs, .. } => abs,
            Self::SmoothQuadratic { abs, .. } => abs,
            Self::EllipticalArc { abs, .. } => abs,
            Self::ClosePath { abs } => abs,
        }
    }

    /// Returns the segment letter.
    ///
    /// # Examples
    ///
    /// ```
    /// use svgtypes::{PathParser, PathSegment};
    ///
    /// let mut segments = Vec::new();
    /// for segment in PathParser::from("M10-20l30.1.5.1-20z") {
    ///     segments.push(segment.unwrap());
    /// }
    ///
    /// assert_eq!(segments[0].command(), b'M');
    /// assert_eq!(segments[1].command(), b'l');
    /// assert_eq!(segments[2].command(), b'l');
    /// assert_eq!(segments[3].command(), b'z');
    /// ```
    ///
    pub fn command(&self) -> u8 {
        match self.is_abs() {
            true => match *self {
                Self::MoveTo { .. } => b'M',
                Self::LineTo { .. } => b'L',
                Self::HorizontalLineTo { .. } => b'H',
                Self::VerticalLineTo { .. } => b'V',
                Self::CurveTo { .. } => b'C',
                Self::SmoothCurveTo { .. } => b'S',
                Self::Quadratic { .. } => b'Q',
                Self::SmoothQuadratic { .. } => b'T',
                Self::EllipticalArc { .. } => b'A',
                Self::ClosePath { .. } => b'Z',
            },
            false => match *self {
                Self::MoveTo { .. } => b'm',
                Self::LineTo { .. } => b'l',
                Self::HorizontalLineTo { .. } => b'h',
                Self::VerticalLineTo { .. } => b'v',
                Self::CurveTo { .. } => b'c',
                Self::SmoothCurveTo { .. } => b's',
                Self::Quadratic { .. } => b'q',
                Self::SmoothQuadratic { .. } => b't',
                Self::EllipticalArc { .. } => b'a',
                Self::ClosePath { .. } => b'z',
            },
        }
    }
}

/// A pull-based [path data] parser.
///
/// # Errors
///
/// - Most of the `Error` types can occur.
///
/// # Notes
///
/// The library does not support implicit commands, so they will be converted to an explicit one.
/// It mostly affects an implicit `MoveTo`, which will be converted, according to the spec,
/// into an explicit `LineTo`.
///
/// Example: `M 10 20 30 40 50 60` -> `M 10 20 L 30 40 L 50 60`
///
/// # Examples
///
/// ```
/// use svgtypes::{PathParser, PathSegment};
///
/// let mut segments = Vec::new();
/// for segment in PathParser::from("M10-20l30.1.5.1-20z") {
///     segments.push(segment.unwrap());
/// }
///
/// assert_eq!(segments, &[
///     PathSegment::MoveTo { abs: true, x: 10.0, y: -20.0 },
///     PathSegment::LineTo { abs: false, x: 30.1, y: 0.5 },
///     PathSegment::LineTo { abs: false, x: 0.1, y: -20.0 },
///     PathSegment::ClosePath { abs: false },
/// ]);
/// ```
///
/// [path data]: https://www.w3.org/TR/SVG2/paths.html#PathData
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct PathParser<'a> {
    stream: Stream<'a>,
    prev_cmd: Option<u8>,
}

impl<'a> From<&'a str> for PathParser<'a> {
    #[inline]
    fn from(v: &'a str) -> Self {
        PathParser {
            stream: Stream::from(v),
            prev_cmd: None,
        }
    }
}

impl Iterator for PathParser<'_> {
    type Item = Result<PathSegment, Error>;

    #[inline]
    fn next(&mut self) -> Option<Self::Item> {
        let s = &mut self.stream;

        s.skip_spaces();

        if s.at_end() {
            return None;
        }

        let res = next_impl(s, &mut self.prev_cmd);
        if res.is_err() {
            s.jump_to_end();
        }

        Some(match res {
            Ok(segment) => Ok(segment),
            Err(NumericParseError::Syntax(error)) => Err(error),
            Err(NumericParseError::Control(never)) => match never {},
        })
    }
}

fn next_impl<I: PathSource>(
    s: &mut I,
    prev_cmd: &mut Option<u8>,
) -> Result<PathSegment, NumericParseError<I::ControlError>> {
    s.charge(1)?;
    let start = s.pos();

    let has_prev_cmd = prev_cmd.is_some();
    let first_char = s.curr_byte()?;

    if !has_prev_cmd && !is_cmd(first_char) {
        return Err(Error::UnexpectedData(s.char_pos(start)).into());
    }

    if !has_prev_cmd && !matches!(first_char, b'M' | b'm') {
        // The first segment must be a MoveTo.
        return Err(Error::UnexpectedData(s.char_pos(start)).into());
    }

    // TODO: simplify
    let is_implicit_move_to;
    let cmd: u8;
    if is_cmd(first_char) {
        is_implicit_move_to = false;
        cmd = first_char;
        s.advance(1);
    } else if is_number_start(first_char) && has_prev_cmd {
        // unwrap is safe, because we checked 'has_prev_cmd'
        let p_cmd = prev_cmd.unwrap();

        if p_cmd == b'Z' || p_cmd == b'z' {
            // ClosePath cannot be followed by a number.
            return Err(Error::UnexpectedData(s.char_pos(start)).into());
        }

        if p_cmd == b'M' || p_cmd == b'm' {
            // 'If a moveto is followed by multiple pairs of coordinates,
            // the subsequent pairs are treated as implicit lineto commands.'
            // So we parse them as LineTo.
            is_implicit_move_to = true;
            cmd = if is_absolute(p_cmd) { b'L' } else { b'l' };
        } else {
            is_implicit_move_to = false;
            cmd = p_cmd;
        }
    } else {
        return Err(Error::UnexpectedData(s.char_pos(start)).into());
    }

    let cmdl = to_relative(cmd);
    let absolute = is_absolute(cmd);
    let token = match cmdl {
        b'm' => PathSegment::MoveTo {
            abs: absolute,
            x: s.parse_list_number()?,
            y: s.parse_list_number()?,
        },
        b'l' => PathSegment::LineTo {
            abs: absolute,
            x: s.parse_list_number()?,
            y: s.parse_list_number()?,
        },
        b'h' => PathSegment::HorizontalLineTo {
            abs: absolute,
            x: s.parse_list_number()?,
        },
        b'v' => PathSegment::VerticalLineTo {
            abs: absolute,
            y: s.parse_list_number()?,
        },
        b'c' => PathSegment::CurveTo {
            abs: absolute,
            x1: s.parse_list_number()?,
            y1: s.parse_list_number()?,
            x2: s.parse_list_number()?,
            y2: s.parse_list_number()?,
            x: s.parse_list_number()?,
            y: s.parse_list_number()?,
        },
        b's' => PathSegment::SmoothCurveTo {
            abs: absolute,
            x2: s.parse_list_number()?,
            y2: s.parse_list_number()?,
            x: s.parse_list_number()?,
            y: s.parse_list_number()?,
        },
        b'q' => PathSegment::Quadratic {
            abs: absolute,
            x1: s.parse_list_number()?,
            y1: s.parse_list_number()?,
            x: s.parse_list_number()?,
            y: s.parse_list_number()?,
        },
        b't' => PathSegment::SmoothQuadratic {
            abs: absolute,
            x: s.parse_list_number()?,
            y: s.parse_list_number()?,
        },
        b'a' => {
            // TODO: radius cannot be negative
            PathSegment::EllipticalArc {
                abs: absolute,
                rx: s.parse_list_number()?,
                ry: s.parse_list_number()?,
                x_axis_rotation: s.parse_list_number()?,
                large_arc: parse_flag(s)?,
                sweep: parse_flag(s)?,
                x: s.parse_list_number()?,
                y: s.parse_list_number()?,
            }
        }
        b'z' => PathSegment::ClosePath { abs: absolute },
        _ => unreachable!(),
    };

    *prev_cmd = Some(if is_implicit_move_to {
        if absolute { b'M' } else { b'm' }
    } else {
        cmd
    });

    Ok(token)
}

/// Returns `true` if the selected char is the command.
#[rustfmt::skip]
#[inline]
fn is_cmd(c: u8) -> bool {
    matches!(c,
          b'M' | b'm'
        | b'Z' | b'z'
        | b'L' | b'l'
        | b'H' | b'h'
        | b'V' | b'v'
        | b'C' | b'c'
        | b'S' | b's'
        | b'Q' | b'q'
        | b'T' | b't'
        | b'A' | b'a')
}

/// Returns `true` if the selected char is the absolute command.
#[inline]
fn is_absolute(c: u8) -> bool {
    debug_assert!(is_cmd(c));
    matches!(
        c,
        b'M' | b'Z' | b'L' | b'H' | b'V' | b'C' | b'S' | b'Q' | b'T' | b'A'
    )
}

/// Converts the selected command char into the relative command char.
#[inline]
fn to_relative(c: u8) -> u8 {
    debug_assert!(is_cmd(c));
    match c {
        b'M' => b'm',
        b'Z' => b'z',
        b'L' => b'l',
        b'H' => b'h',
        b'V' => b'v',
        b'C' => b'c',
        b'S' => b's',
        b'Q' => b'q',
        b'T' => b't',
        b'A' => b'a',
        _ => c,
    }
}

#[inline]
fn is_number_start(c: u8) -> bool {
    matches!(c, b'0'..=b'9' | b'.' | b'-' | b'+')
}

// By the SVG spec 'large-arc' and 'sweep' must contain only one char
// and can be written without any separators, e.g.: 10 20 30 01 10 20.
fn parse_flag<I: PathSource>(s: &mut I) -> Result<bool, NumericParseError<I::ControlError>> {
    s.charge(1)?;
    s.skip_spaces()?;

    let c = s.curr_byte()?;
    match c {
        b'0' | b'1' => {
            s.advance(1);
            if s.byte_is(b',')? {
                s.advance(1);
            }
            s.skip_spaces()?;

            Ok(c == b'1')
        }
        _ => Err(Error::UnexpectedData(s.char_pos(s.pos())).into()),
    }
}

/// A pull path parser whose byte windows persist across all `try_next` calls.
pub struct ControlledPathParser<'a, F> {
    cursor: Cursor<'a, F>,
    prev_cmd: Option<u8>,
    terminal: bool,
}

impl<F> core::fmt::Debug for ControlledPathParser<'_, F> {
    fn fmt(&self, formatter: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        formatter
            .debug_struct("ControlledPathParser")
            .field("position", &self.cursor.position())
            .field("prev_cmd", &self.prev_cmd)
            .field("terminal", &self.terminal)
            .finish()
    }
}

impl<'a, E, F: FnMut(usize) -> Result<(), E>> ControlledPathParser<'a, F> {
    /// Construct after checking the original caller control capability.
    pub fn new(text: &'a str, checkpoint: F) -> Result<Self, NumericParseError<E>> {
        Ok(Self {
            cursor: Cursor::new(text, checkpoint)?,
            prev_cmd: None,
            terminal: false,
        })
    }

    /// Current byte position; Syntax jumps to the end, Control preserves it.
    pub fn position(&self) -> usize {
        self.cursor.position()
    }

    /// Pull one command without visiting a later command after failure.
    pub fn try_next(&mut self) -> Result<Option<PathSegment>, NumericParseError<E>> {
        if self.terminal {
            return Ok(None);
        }
        let result = (|| {
            self.cursor.work(0)?;
            self.cursor.skip_spaces()?;
            if self.cursor.at_end() {
                return Ok(None);
            }
            let value = next_impl(&mut self.cursor, &mut self.prev_cmd)?;
            self.cursor.work(0)?;
            Ok(Some(value))
        })();
        match &result {
            Err(NumericParseError::Syntax(_)) => {
                self.cursor.jump_to_end();
                self.terminal = true;
            }
            Err(NumericParseError::Control(_)) => self.terminal = true,
            Ok(None) => self.terminal = true,
            Ok(Some(_)) => {}
        }
        result
    }
}

#[rustfmt::skip]
#[cfg(test)]
mod tests {
    use super::*;

    macro_rules! test {
        ($name:ident, $text:expr, $( $seg:expr ),*) => (
            #[test]
            fn $name() {
                let mut s = PathParser::from($text);
                $(
                    assert_eq!(s.next().unwrap().unwrap(), $seg);
                )*

                if let Some(res) = s.next() {
                    assert!(res.is_err());
                }
            }
        )
    }

    test!(null, "", );
    test!(not_a_path, "q", );
    test!(not_a_move_to, "L 20 30", );
    test!(stop_on_err_1, "M 10 20 L 30 40 L 50",
        PathSegment::MoveTo { abs: true, x: 10.0, y: 20.0 },
        PathSegment::LineTo { abs: true, x: 30.0, y: 40.0 }
    );

    test!(move_to_1, "M 10 20", PathSegment::MoveTo { abs: true, x: 10.0, y: 20.0 });
    test!(move_to_2, "m 10 20", PathSegment::MoveTo { abs: false, x: 10.0, y: 20.0 });
    test!(move_to_3, "M 10 20 30 40 50 60",
        PathSegment::MoveTo { abs: true, x: 10.0, y: 20.0 },
        PathSegment::LineTo { abs: true, x: 30.0, y: 40.0 },
        PathSegment::LineTo { abs: true, x: 50.0, y: 60.0 }
    );
    test!(move_to_4, "M 10 20 30 40 50 60 M 70 80 90 100 110 120",
        PathSegment::MoveTo { abs: true, x: 10.0, y: 20.0 },
        PathSegment::LineTo { abs: true, x: 30.0, y: 40.0 },
        PathSegment::LineTo { abs: true, x: 50.0, y: 60.0 },
        PathSegment::MoveTo { abs: true, x: 70.0, y: 80.0 },
        PathSegment::LineTo { abs: true, x: 90.0, y: 100.0 },
        PathSegment::LineTo { abs: true, x: 110.0, y: 120.0 }
    );

    test!(arc_to_1, "M 10 20 A 5 5 30 1 1 20 20",
        PathSegment::MoveTo { abs: true, x: 10.0, y: 20.0 },
        PathSegment::EllipticalArc {
            abs: true,
            rx: 5.0, ry: 5.0,
            x_axis_rotation: 30.0,
            large_arc: true, sweep: true,
            x: 20.0, y: 20.0
        }
    );

    test!(arc_to_2, "M 10 20 a 5 5 30 0 0 20 20",
        PathSegment::MoveTo { abs: true, x: 10.0, y: 20.0 },
        PathSegment::EllipticalArc {
            abs: false,
            rx: 5.0, ry: 5.0,
            x_axis_rotation: 30.0,
            large_arc: false, sweep: false,
            x: 20.0, y: 20.0
        }
    );

    test!(arc_to_10, "M10-20A5.5.3-4 010-.1",
        PathSegment::MoveTo { abs: true, x: 10.0, y: -20.0 },
        PathSegment::EllipticalArc {
            abs: true,
            rx: 5.5, ry: 0.3,
            x_axis_rotation: -4.0,
            large_arc: false, sweep: true,
            x: 0.0, y: -0.1
        }
    );

    test!(separator_1, "M 10 20 L 5 15 C 10 20 30 40 50 60",
        PathSegment::MoveTo { abs: true, x: 10.0, y: 20.0 },
        PathSegment::LineTo { abs: true, x: 5.0, y: 15.0 },
        PathSegment::CurveTo {
            abs: true,
            x1: 10.0, y1: 20.0,
            x2: 30.0, y2: 40.0,
            x:  50.0, y:  60.0,
        }
    );

    test!(separator_2, "M 10, 20 L 5, 15 C 10, 20 30, 40 50, 60",
        PathSegment::MoveTo { abs: true, x: 10.0, y: 20.0 },
        PathSegment::LineTo { abs: true, x: 5.0, y: 15.0 },
        PathSegment::CurveTo {
            abs: true,
            x1: 10.0, y1: 20.0,
            x2: 30.0, y2: 40.0,
            x:  50.0, y:  60.0,
        }
    );

    test!(separator_3, "M 10,20 L 5,15 C 10,20 30,40 50,60",
        PathSegment::MoveTo { abs: true, x: 10.0, y: 20.0 },
        PathSegment::LineTo { abs: true, x: 5.0, y: 15.0 },
        PathSegment::CurveTo {
            abs: true,
            x1: 10.0, y1: 20.0,
            x2: 30.0, y2: 40.0,
            x:  50.0, y:  60.0,
        }
    );

    test!(separator_4, "M10, 20 L5, 15 C10, 20 30 40 50 60",
        PathSegment::MoveTo { abs: true, x: 10.0, y: 20.0 },
        PathSegment::LineTo { abs: true, x: 5.0, y: 15.0 },
        PathSegment::CurveTo {
            abs: true,
            x1: 10.0, y1: 20.0,
            x2: 30.0, y2: 40.0,
            x:  50.0, y:  60.0,
        }
    );

    test!(separator_5, "M10 20V30H40V50H60Z",
        PathSegment::MoveTo { abs: true, x: 10.0, y: 20.0 },
        PathSegment::VerticalLineTo { abs: true, y: 30.0 },
        PathSegment::HorizontalLineTo { abs: true, x: 40.0 },
        PathSegment::VerticalLineTo { abs: true, y: 50.0 },
        PathSegment::HorizontalLineTo { abs: true, x: 60.0 },
        PathSegment::ClosePath { abs: true }
    );

    test!(all_segments_1, "M 10 20 L 30 40 H 50 V 60 C 70 80 90 100 110 120 S 130 140 150 160
        Q 170 180 190 200 T 210 220 A 50 50 30 1 1 230 240 Z",
        PathSegment::MoveTo { abs: true, x: 10.0, y: 20.0 },
        PathSegment::LineTo { abs: true, x: 30.0, y: 40.0 },
        PathSegment::HorizontalLineTo { abs: true, x: 50.0 },
        PathSegment::VerticalLineTo { abs: true, y: 60.0 },
        PathSegment::CurveTo {
            abs: true,
            x1:  70.0, y1:  80.0,
            x2:  90.0, y2: 100.0,
            x:  110.0, y:  120.0,
        },
        PathSegment::SmoothCurveTo {
            abs: true,
            x2: 130.0, y2: 140.0,
            x:  150.0, y:  160.0,
        },
        PathSegment::Quadratic {
            abs: true,
            x1: 170.0, y1: 180.0,
            x:  190.0, y:  200.0,
        },
        PathSegment::SmoothQuadratic { abs: true, x: 210.0, y: 220.0 },
        PathSegment::EllipticalArc {
            abs: true,
            rx: 50.0, ry: 50.0,
            x_axis_rotation: 30.0,
            large_arc: true, sweep: true,
            x: 230.0, y: 240.0
        },
        PathSegment::ClosePath { abs: true }
    );

    test!(all_segments_2, "m 10 20 l 30 40 h 50 v 60 c 70 80 90 100 110 120 s 130 140 150 160
        q 170 180 190 200 t 210 220 a 50 50 30 1 1 230 240 z",
        PathSegment::MoveTo { abs: false, x: 10.0, y: 20.0 },
        PathSegment::LineTo { abs: false, x: 30.0, y: 40.0 },
        PathSegment::HorizontalLineTo { abs: false, x: 50.0 },
        PathSegment::VerticalLineTo { abs: false, y: 60.0 },
        PathSegment::CurveTo {
            abs: false,
            x1:  70.0, y1:  80.0,
            x2:  90.0, y2: 100.0,
            x:  110.0, y:  120.0,
        },
        PathSegment::SmoothCurveTo {
            abs: false,
            x2: 130.0, y2: 140.0,
            x:  150.0, y:  160.0,
        },
        PathSegment::Quadratic {
            abs: false,
            x1: 170.0, y1: 180.0,
            x:  190.0, y:  200.0,
        },
        PathSegment::SmoothQuadratic { abs: false, x: 210.0, y: 220.0 },
        PathSegment::EllipticalArc {
            abs: false,
            rx: 50.0, ry: 50.0,
            x_axis_rotation: 30.0,
            large_arc: true, sweep: true,
            x: 230.0, y: 240.0
        },
        PathSegment::ClosePath { abs: false }
    );

    test!(close_path_1, "M10 20 L 30 40 ZM 100 200 L 300 400",
        PathSegment::MoveTo { abs: true, x: 10.0, y: 20.0 },
        PathSegment::LineTo { abs: true, x: 30.0, y: 40.0 },
        PathSegment::ClosePath { abs: true },
        PathSegment::MoveTo { abs: true, x: 100.0, y: 200.0 },
        PathSegment::LineTo { abs: true, x: 300.0, y: 400.0 }
    );

    test!(close_path_2, "M10 20 L 30 40 zM 100 200 L 300 400",
        PathSegment::MoveTo { abs: true, x: 10.0, y: 20.0 },
        PathSegment::LineTo { abs: true, x: 30.0, y: 40.0 },
        PathSegment::ClosePath { abs: false },
        PathSegment::MoveTo { abs: true, x: 100.0, y: 200.0 },
        PathSegment::LineTo { abs: true, x: 300.0, y: 400.0 }
    );

    test!(close_path_3, "M10 20 L 30 40 Z Z Z",
        PathSegment::MoveTo { abs: true, x: 10.0, y: 20.0 },
        PathSegment::LineTo { abs: true, x: 30.0, y: 40.0 },
        PathSegment::ClosePath { abs: true },
        PathSegment::ClosePath { abs: true },
        PathSegment::ClosePath { abs: true }
    );

    // first token should be EndOfStream
    test!(invalid_1, "M\t.", );

    // ClosePath can't be followed by a number
    test!(invalid_2, "M 0 0 Z 2",
        PathSegment::MoveTo { abs: true, x: 0.0, y: 0.0 },
        PathSegment::ClosePath { abs: true }
    );

    // ClosePath can be followed by any command
    test!(invalid_3, "M 0 0 Z H 10",
        PathSegment::MoveTo { abs: true, x: 0.0, y: 0.0 },
        PathSegment::ClosePath { abs: true },
        PathSegment::HorizontalLineTo { abs: true, x: 10.0 }
    );

    #[test]
    fn test_helper_functions(){
        assert!(PathSegment::MoveTo { abs: true, x: 0.0, y: 0.0 }.is_abs());
        assert!(!PathSegment::MoveTo { abs: false, x: 0.0, y: 0.0 }.is_abs()); // asserting false
        assert_eq!(PathSegment::MoveTo { abs: true, x: 0.0, y: 0.0 }.command(), b'M');
    }
}

/// Representation of a simple path segment.
#[allow(missing_docs)]
#[derive(Clone, Copy, PartialEq, Debug)]
pub enum SimplePathSegment {
    MoveTo {
        x: f64,
        y: f64,
    },
    LineTo {
        x: f64,
        y: f64,
    },
    CurveTo {
        x1: f64,
        y1: f64,
        x2: f64,
        y2: f64,
        x: f64,
        y: f64,
    },
    Quadratic {
        x1: f64,
        y1: f64,
        x: f64,
        y: f64,
    },
    ClosePath,
}

/// A simplifying Path Data parser.
///
/// A more high-level Path Data parser on top of [`PathParser`] that provides:
///
/// - Relative to absolute segment coordinates conversion
/// - `ArcTo` to `CurveTo`s conversion
/// - `SmoothCurveTo` and `SmoothQuadratic` conversion
/// - `HorizontalLineTo` and `VerticalLineTo` to `LineTo` conversion
/// - Implicit `MoveTo` after `ClosePath` handling
///
/// In the end, only absolute `MoveTo`, `LineTo`, `CurveTo`, `Quadratic` and `ClosePath` segments
/// will be produced.
#[derive(Clone, Debug)]
pub struct SimplifyingPathParser<'a> {
    parser: PathParser<'a>,
    state: SimplifyState,
    buffer: CompatPending,
}

#[derive(Clone, Debug)]
struct SimplifyState {
    prev_mx: f64,
    prev_my: f64,
    prev_tx: f64,
    prev_ty: f64,
    prev_x: f64,
    prev_y: f64,
    prev_seg: PathSegment,
    prev_simple_seg: Option<SimplePathSegment>,
}

impl Default for SimplifyState {
    fn default() -> Self {
        Self {
            prev_mx: 0.0,
            prev_my: 0.0,
            prev_tx: 0.0,
            prev_ty: 0.0,
            prev_x: 0.0,
            prev_y: 0.0,
            prev_seg: PathSegment::MoveTo {
                abs: true,
                x: 0.0,
                y: 0.0,
            },
            prev_simple_seg: None,
        }
    }
}

trait PendingQueue<E> {
    fn push(&mut self, segment: SimplePathSegment) -> Result<(), E>;
    fn last(&self) -> Option<SimplePathSegment>;
}

#[derive(Clone, Debug, Default)]
struct CompatPending {
    values: Vec<SimplePathSegment>,
    head: usize,
}

impl CompatPending {
    fn pop(&mut self) -> Option<SimplePathSegment> {
        let value = self.values.get(self.head).copied()?;
        self.head += 1;
        if self.head == self.values.len() {
            self.values.clear();
            self.head = 0;
        }
        Some(value)
    }
}

impl PendingQueue<Infallible> for CompatPending {
    fn push(&mut self, segment: SimplePathSegment) -> Result<(), Infallible> {
        self.values.push(segment);
        Ok(())
    }
    fn last(&self) -> Option<SimplePathSegment> {
        self.values.last().copied()
    }
}

struct PaidPending {
    values: BudgetVec<SimplePathSegment>,
    head: usize,
}

impl PaidPending {
    fn pop(&mut self) -> Option<SimplePathSegment> {
        let value = self.values.get(self.head).copied()?;
        self.head += 1;
        if self.head == self.values.len() {
            self.values.clear();
            self.head = 0;
        }
        Some(value)
    }
}

impl PendingQueue<ResourceError> for PaidPending {
    fn push(&mut self, segment: SimplePathSegment) -> Result<(), ResourceError> {
        self.values.try_push(segment)
    }
    fn last(&self) -> Option<SimplePathSegment> {
        self.values.last().copied()
    }
}

impl SimplifyState {
    fn consume<E, Q: PendingQueue<E>>(
        &mut self,
        segment: PathSegment,
        buffer: &mut Q,
        checkpoint: &mut impl FnMut(usize) -> Result<(), E>,
    ) -> Result<(), E> {
        checkpoint(1)?;
        // If a ClosePath segment is followed by any command other than MoveTo or ClosePath
        // then MoveTo is implicit.
        if let Some(SimplePathSegment::ClosePath) = self.prev_simple_seg {
            match segment {
                PathSegment::MoveTo { .. } | PathSegment::ClosePath { .. } => {}
                _ => {
                    let new_seg = SimplePathSegment::MoveTo {
                        x: self.prev_mx,
                        y: self.prev_my,
                    };
                    buffer.push(new_seg)?;
                    self.prev_simple_seg = Some(new_seg);
                }
            }
        }

        match segment {
            PathSegment::MoveTo { abs, mut x, mut y } => {
                if !abs {
                    // When we get 'm'(relative) segment, which is not first segment - then it's
                    // relative to a previous 'M'(absolute) segment, not to the first segment.
                    if let Some(SimplePathSegment::ClosePath) = self.prev_simple_seg {
                        x += self.prev_mx;
                        y += self.prev_my;
                    } else {
                        x += self.prev_x;
                        y += self.prev_y;
                    }
                }

                buffer.push(SimplePathSegment::MoveTo { x, y })?;
                self.prev_seg = segment;
            }
            PathSegment::LineTo { abs, mut x, mut y } => {
                if !abs {
                    x += self.prev_x;
                    y += self.prev_y;
                }

                buffer.push(SimplePathSegment::LineTo { x, y })?;
                self.prev_seg = segment;
            }
            PathSegment::HorizontalLineTo { abs, mut x } => {
                if !abs {
                    x += self.prev_x;
                }

                buffer.push(SimplePathSegment::LineTo { x, y: self.prev_y })?;
                self.prev_seg = segment;
            }
            PathSegment::VerticalLineTo { abs, mut y } => {
                if !abs {
                    y += self.prev_y;
                }

                buffer.push(SimplePathSegment::LineTo { x: self.prev_x, y })?;
                self.prev_seg = segment;
            }
            PathSegment::CurveTo {
                abs,
                mut x1,
                mut y1,
                mut x2,
                mut y2,
                mut x,
                mut y,
            } => {
                if !abs {
                    x1 += self.prev_x;
                    y1 += self.prev_y;
                    x2 += self.prev_x;
                    y2 += self.prev_y;
                    x += self.prev_x;
                    y += self.prev_y;
                }

                buffer.push(SimplePathSegment::CurveTo {
                    x1,
                    y1,
                    x2,
                    y2,
                    x,
                    y,
                })?;

                // Remember as absolute.
                self.prev_seg = PathSegment::CurveTo {
                    abs: true,
                    x1,
                    y1,
                    x2,
                    y2,
                    x,
                    y,
                };
            }
            PathSegment::SmoothCurveTo {
                abs,
                mut x2,
                mut y2,
                mut x,
                mut y,
            } => {
                // 'The first control point is assumed to be the reflection of the second control
                // point on the previous command relative to the current point.
                // (If there is no previous command or if the previous command
                // was not an C, c, S or s, assume the first control point is
                // coincident with the current point.)'
                let (x1, y1) = match self.prev_seg {
                    PathSegment::CurveTo { x2, y2, x, y, .. }
                    | PathSegment::SmoothCurveTo { x2, y2, x, y, .. } => {
                        (x * 2.0 - x2, y * 2.0 - y2)
                    }
                    _ => (self.prev_x, self.prev_y),
                };

                if !abs {
                    x2 += self.prev_x;
                    y2 += self.prev_y;
                    x += self.prev_x;
                    y += self.prev_y;
                }

                buffer.push(SimplePathSegment::CurveTo {
                    x1,
                    y1,
                    x2,
                    y2,
                    x,
                    y,
                })?;

                // Remember as absolute.
                self.prev_seg = PathSegment::SmoothCurveTo {
                    abs: true,
                    x2,
                    y2,
                    x,
                    y,
                };
            }
            PathSegment::Quadratic {
                abs,
                mut x1,
                mut y1,
                mut x,
                mut y,
            } => {
                if !abs {
                    x1 += self.prev_x;
                    y1 += self.prev_y;
                    x += self.prev_x;
                    y += self.prev_y;
                }

                buffer.push(SimplePathSegment::Quadratic { x1, y1, x, y })?;

                // Remember as absolute.
                self.prev_seg = PathSegment::Quadratic {
                    abs: true,
                    x1,
                    y1,
                    x,
                    y,
                };
            }
            PathSegment::SmoothQuadratic { abs, mut x, mut y } => {
                // 'The control point is assumed to be the reflection of
                // the control point on the previous command relative to
                // the current point. (If there is no previous command or
                // if the previous command was not a Q, q, T or t, assume
                // the control point is coincident with the current point.)'
                let (x1, y1) = match self.prev_seg {
                    PathSegment::Quadratic { x1, y1, x, y, .. } => (x * 2.0 - x1, y * 2.0 - y1),
                    PathSegment::SmoothQuadratic { x, y, .. } => {
                        (x * 2.0 - self.prev_tx, y * 2.0 - self.prev_ty)
                    }
                    _ => (self.prev_x, self.prev_y),
                };

                self.prev_tx = x1;
                self.prev_ty = y1;

                if !abs {
                    x += self.prev_x;
                    y += self.prev_y;
                }

                buffer.push(SimplePathSegment::Quadratic { x1, y1, x, y })?;

                // Remember as absolute.
                self.prev_seg = PathSegment::SmoothQuadratic { abs: true, x, y };
            }
            PathSegment::EllipticalArc {
                abs,
                rx,
                ry,
                x_axis_rotation,
                large_arc,
                sweep,
                mut x,
                mut y,
            } => {
                if !abs {
                    x += self.prev_x;
                    y += self.prev_y;
                }

                let svg_arc = kurbo::SvgArc {
                    from: kurbo::Point::new(self.prev_x, self.prev_y),
                    to: kurbo::Point::new(x, y),
                    radii: kurbo::Vec2::new(rx, ry),
                    x_rotation: x_axis_rotation.to_radians(),
                    large_arc,
                    sweep,
                };

                match kurbo::Arc::from_svg_arc(&svg_arc) {
                    Some(arc) => {
                        let mut segments = arc.append_iter(0.1);
                        loop {
                            checkpoint(1)?;
                            let Some(kurbo::PathEl::CurveTo(p1, p2, p)) = segments.next() else {
                                break;
                            };
                            buffer.push(SimplePathSegment::CurveTo {
                                x1: p1.x,
                                y1: p1.y,
                                x2: p2.x,
                                y2: p2.y,
                                x: p.x,
                                y: p.y,
                            })?;
                        }
                    }
                    None => {
                        buffer.push(SimplePathSegment::LineTo { x, y })?;
                    }
                }

                self.prev_seg = segment;
            }
            PathSegment::ClosePath { .. } => {
                if let Some(SimplePathSegment::ClosePath) = self.prev_simple_seg {
                    // Do not add sequential ClosePath segments.
                    // Otherwise it will break markers rendering.
                } else {
                    buffer.push(SimplePathSegment::ClosePath)?;
                }

                self.prev_seg = segment;
            }
        }

        // Remember last position.
        if let Some(new_segment) = buffer.last() {
            self.prev_simple_seg = Some(new_segment);

            match new_segment {
                SimplePathSegment::MoveTo { x, y } => {
                    self.prev_x = x;
                    self.prev_y = y;
                    self.prev_mx = self.prev_x;
                    self.prev_my = self.prev_y;
                }
                SimplePathSegment::LineTo { x, y } => {
                    self.prev_x = x;
                    self.prev_y = y;
                }
                SimplePathSegment::CurveTo { x, y, .. } => {
                    self.prev_x = x;
                    self.prev_y = y;
                }
                SimplePathSegment::Quadratic { x, y, .. } => {
                    self.prev_x = x;
                    self.prev_y = y;
                }
                SimplePathSegment::ClosePath => {
                    // ClosePath moves us to the last MoveTo coordinate.
                    self.prev_x = self.prev_mx;
                    self.prev_y = self.prev_my;
                }
            }
        }

        Ok(())
    }
}

impl<'a> From<&'a str> for SimplifyingPathParser<'a> {
    fn from(text: &'a str) -> Self {
        Self {
            parser: PathParser::from(text),
            state: SimplifyState::default(),
            buffer: CompatPending::default(),
        }
    }
}

impl Iterator for SimplifyingPathParser<'_> {
    type Item = Result<SimplePathSegment, Error>;

    fn next(&mut self) -> Option<Self::Item> {
        loop {
            if let Some(value) = self.buffer.pop() {
                return Some(Ok(value));
            }
            let segment = match self.parser.next()? {
                Ok(value) => value,
                Err(error) => return Some(Err(error)),
            };
            match self
                .state
                .consume(segment, &mut self.buffer, &mut |_| Ok(()))
            {
                Ok(()) => {}
                Err(never) => match never {},
            }
            // Repeated Close commands remain the same state transition; loop
            // rather than recurse through an input-sized chain of next calls.
        }
    }
}

/// A controlled simplifier with the same state machine and a request-paid FIFO.
/// Its queue retains capacity between pulls and never falls back to ordinary Vec.
pub struct ControlledSimplifyingPathParser<'a, F> {
    parser: ControlledPathParser<'a, F>,
    state: SimplifyState,
    buffer: PaidPending,
    resources: ResourcesRef,
    terminal: bool,
}

impl<F> core::fmt::Debug for ControlledSimplifyingPathParser<'_, F> {
    fn fmt(&self, formatter: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        formatter
            .debug_struct("ControlledSimplifyingPathParser")
            .field("parser", &self.parser)
            .field("state", &self.state)
            .field("pending", &(self.buffer.values.len() - self.buffer.head))
            .field("terminal", &self.terminal)
            .finish()
    }
}

impl<'a, F: FnMut(usize) -> Result<(), ResourceError>> ControlledSimplifyingPathParser<'a, F> {
    /// Construct the Scratch queue from the same request used by the caller.
    pub fn new(
        text: &'a str,
        checkpoint: F,
        resources: &ResourcesRef,
    ) -> Result<Self, NumericParseError<ResourceError>> {
        resources
            .checkpoint(0)
            .map_err(NumericParseError::Control)?;
        let parser = ControlledPathParser::new(text, checkpoint).map_err(|error| match error {
            NumericParseError::Control(error) => {
                NumericParseError::Control(resources.record(error))
            }
            error => error,
        })?;
        Ok(Self {
            parser,
            state: SimplifyState::default(),
            buffer: PaidPending {
                values: BudgetVec::new(resources.clone(), ResourceKind::Scratch),
                head: 0,
            },
            resources: resources.clone(),
            terminal: false,
        })
    }

    /// Current lexical byte position, including a failed command's consumption.
    pub fn position(&self) -> usize {
        self.parser.position()
    }

    /// Pull one simplified segment. Callers must retire this queue before their
    /// final request check, so its last-storage release fault cannot be hidden.
    pub fn try_next(
        &mut self,
    ) -> Result<Option<SimplePathSegment>, NumericParseError<ResourceError>> {
        if self.terminal {
            return Ok(None);
        }
        let result = (|| {
            self.resources
                .checkpoint(0)
                .map_err(NumericParseError::Control)?;
            loop {
                self.parser.cursor.work(1)?;
                if let Some(value) = self.buffer.pop() {
                    self.resources
                        .checkpoint(0)
                        .map_err(NumericParseError::Control)?;
                    return Ok(Some(value));
                }
                let Some(segment) = self.parser.try_next()? else {
                    self.resources
                        .checkpoint(0)
                        .map_err(NumericParseError::Control)?;
                    return Ok(None);
                };
                let parser = &mut self.parser;
                self.state
                    .consume(
                        segment,
                        &mut self.buffer,
                        &mut |work| match parser.cursor.work(work) {
                            Ok(()) => Ok(()),
                            Err(NumericParseError::Control(error)) => Err(error),
                            Err(NumericParseError::Syntax(_)) => {
                                unreachable!("a control checkpoint does not parse syntax")
                            }
                        },
                    )
                    .map_err(NumericParseError::Control)?;
            }
        })();
        match result {
            Err(NumericParseError::Control(error)) => {
                self.terminal = true;
                Err(NumericParseError::Control(self.resources.record(error)))
            }
            Err(error) => {
                self.terminal = true;
                Err(error)
            }
            Ok(None) => {
                self.terminal = true;
                Ok(None)
            }
            value => value,
        }
    }
}

#[cfg(test)]
mod controlled_path_tests {
    extern crate std;
    use self::std::sync::Mutex;
    use super::*;
    use alloc::{format, sync::Arc, vec, vec::Vec};
    use core::sync::atomic::{AtomicUsize, Ordering};
    use markdown_svg_budget::{Backend, FirstFailureRef, Limits, Ticket};

    #[derive(Default)]
    struct State {
        reserves: AtomicUsize,
        commits: AtomicUsize,
        fail_reserve: usize,
        fail_commit: usize,
        fail_release: usize,
        tickets: Mutex<Vec<(u32, usize)>>,
    }
    struct TestBackend(Arc<State>);
    impl Backend for TestBackend {
        fn reserve(&self, _: ResourceKind, bytes: usize) -> Result<Ticket, ResourceError> {
            let call = self.0.reserves.fetch_add(1, Ordering::Relaxed) + 1;
            if call == self.0.fail_reserve {
                return Err(ResourceError::native(-731));
            }
            self.0.tickets.lock().unwrap().push((call as u32, bytes));
            Ticket::new(1, call as u32, 1)
        }
        fn allocation_committed(&self, ticket: &Ticket, bytes: usize) -> Result<(), ResourceError> {
            assert!(
                self.0
                    .tickets
                    .lock()
                    .unwrap()
                    .contains(&(ticket.slot(), bytes))
            );
            let call = self.0.commits.fetch_add(1, Ordering::Relaxed) + 1;
            if call == self.0.fail_commit {
                Err(ResourceError::native(-732))
            } else {
                Ok(())
            }
        }
        fn release_after_free(&self, ticket: Ticket) -> Result<(), ResourceError> {
            if ticket.slot() as usize == self.0.fail_release {
                return Err(ResourceError::native(-733));
            }
            let mut tickets = self.0.tickets.lock().unwrap();
            let index = tickets
                .iter()
                .position(|entry| entry.0 == ticket.slot())
                .unwrap();
            tickets.swap_remove(index);
            Ok(())
        }
        fn checkpoint(&self, _: u64) -> Result<(), ResourceError> {
            // Release errors deliberately do not poison this host callback.
            Ok(())
        }
    }

    fn resources(
        fail_reserve: usize,
        fail_commit: usize,
        fail_release: usize,
    ) -> (ResourcesRef, Arc<State>) {
        let state = Arc::new(State {
            fail_reserve,
            fail_commit,
            fail_release,
            ..State::default()
        });
        let resources = ResourcesRef::new_prepaid(
            TestBackend(state.clone()),
            FirstFailureRef::new_prepaid(),
            Limits::default(),
        );
        (resources, state)
    }

    fn simple_bits(segment: SimplePathSegment) -> Vec<u64> {
        match segment {
            SimplePathSegment::MoveTo { x, y } => vec![0, x.to_bits(), y.to_bits()],
            SimplePathSegment::LineTo { x, y } => vec![1, x.to_bits(), y.to_bits()],
            SimplePathSegment::CurveTo {
                x1,
                y1,
                x2,
                y2,
                x,
                y,
            } => vec![
                2,
                x1.to_bits(),
                y1.to_bits(),
                x2.to_bits(),
                y2.to_bits(),
                x.to_bits(),
                y.to_bits(),
            ],
            SimplePathSegment::Quadratic { x1, y1, x, y } => {
                vec![3, x1.to_bits(), y1.to_bits(), x.to_bits(), y.to_bits()]
            }
            SimplePathSegment::ClosePath => vec![4],
        }
    }

    fn paid_simple(
        text: &str,
        resources: &ResourcesRef,
    ) -> Result<Vec<Result<Vec<u64>, Error>>, ResourceError> {
        let result = {
            let mut parser = ControlledSimplifyingPathParser::new(
                text,
                |work| resources.checkpoint(work as u64),
                resources,
            )
            .map_err(|error| match error {
                NumericParseError::Control(error) => error,
                _ => panic!("constructor syntax"),
            })?;
            let mut values = Vec::new();
            loop {
                match parser.try_next() {
                    Ok(Some(value)) => values.push(Ok(simple_bits(value))),
                    Ok(None) => break Ok(values),
                    Err(NumericParseError::Syntax(error)) => {
                        values.push(Err(error));
                        break Ok(values);
                    }
                    Err(NumericParseError::Control(error)) => break Err(error),
                }
            }
        };
        // This is a real caller boundary after the pending queue is destroyed.
        resources.checkpoint(0)?;
        result
    }

    #[test]
    fn controlled_commands_and_simplification_match_original_grammar_and_float_bits() {
        for text in [
            "",
            "  ",
            "M-0 +0",
            "M1 2 3 4 5 6",
            "m1 2 l3 4 h5 v6z h2 m4 5",
            "M1 2C3 4 5 6 7 8S9 10 11 12Q13 14 15 16T17 18z",
            "m1 2c3 4 5 6 7 8s9 10 11 12q13 14 15 16t17 18z",
            "M1 2A5 7 20 0112 15z",
            "M1 2a5 7 20 1 0 12 15z",
            "M1 2A0 7 0 0 1 12 15",
            "M1 2zzzzzL3 4",
            "M1 2z m3 4",
            "L1 2",
            "M1",
            "M1 2L3",
            "M1 2Z1",
            "M1 2A3 4 0 2 1 5 6",
            "M1 2L3e 4",
            "M1 2é",
            "M1 2Lé 4",
            "M1 2L1e999 4",
            "M1,2 L3.5.25",
            "M\t. ",
            "M1 2,",
            "M1 2 L3,4,",
            "M1 2 L3 4,,",
        ] {
            let ordinary = PathParser::from(text).collect::<Vec<_>>();
            let mut parser =
                ControlledPathParser::new(text, |_| Ok::<(), &'static str>(())).unwrap();
            let mut controlled = Vec::new();
            loop {
                match parser.try_next() {
                    Ok(Some(value)) => controlled.push(Ok(value)),
                    Ok(None) => break,
                    Err(NumericParseError::Syntax(error)) => {
                        controlled.push(Err(error));
                        break;
                    }
                    Err(NumericParseError::Control(error)) => panic!("unexpected {error}"),
                }
            }
            // Finite number Debug is round-tripping and retains the sign of zero.
            assert_eq!(format!("{ordinary:?}"), format!("{controlled:?}"), "{text}");
            let ordinary = SimplifyingPathParser::from(text)
                .map(|result| result.map(simple_bits))
                .collect::<Vec<_>>();
            let (resources, state) = resources(0, 0, 0);
            assert_eq!(paid_simple(text, &resources).unwrap(), ordinary, "{text}");
            assert!(state.tickets.lock().unwrap().is_empty());
            resources.validate_retired().unwrap();
        }
    }

    #[test]
    fn path_lexing_stops_before_reading_the_second_4096_byte_window() {
        let text = format!("M{}1 2", " ".repeat(8193));
        let mut windows = 0;
        let mut parser = ControlledPathParser::new(&text, |work| {
            if work > 256 {
                windows += 1;
                if windows == 2 {
                    return Err("second-window");
                }
            }
            Ok(())
        })
        .unwrap();
        assert_eq!(
            parser.try_next(),
            Err(NumericParseError::Control("second-window"))
        );
        assert_eq!(parser.cursor.position(), 4096);
        assert_eq!(parser.cursor.lexical_read_end(), 4096);
        assert!(parser.try_next().unwrap().is_none());
    }

    #[test]
    fn source_window_credit_survives_many_separate_command_pulls() {
        let text = format!("M0 0{}", "L1 1".repeat(2000));
        let mut windows = 0;
        let mut parser = ControlledPathParser::new(&text, |work| {
            if work > 256 {
                windows += 1;
                if windows == 2 {
                    return Err("second-window");
                }
            }
            Ok(())
        })
        .unwrap();
        let mut commands = 0;
        loop {
            match parser.try_next() {
                Ok(Some(_)) => commands += 1,
                Err(NumericParseError::Control("second-window")) => break,
                other => panic!("unexpected {other:?}"),
            }
        }
        assert!(commands > 500 && commands < 2001);
        assert_eq!(parser.cursor.lexical_read_end(), 4096);
        assert_eq!(windows, 2);
    }

    #[test]
    fn repeated_close_commands_use_bounded_stack_and_can_be_cancelled_without_output() {
        let text = format!("M0 0Z{}L1 1", "Z".repeat(100_000));
        assert_eq!(SimplifyingPathParser::from(text.as_str()).count(), 4);
        let (resources, state) = resources(0, 0, 0);
        let mut events = 0;
        {
            let mut parser = ControlledSimplifyingPathParser::new(
                &text,
                |work| {
                    if work == 1 {
                        events += 1;
                        if events == 512 {
                            return Err(ResourceError::native(-734));
                        }
                    }
                    Ok(())
                },
                &resources,
            )
            .unwrap();
            assert!(parser.try_next().unwrap().is_some());
            assert!(parser.try_next().unwrap().is_some());
            assert_eq!(
                parser.try_next(),
                Err(NumericParseError::Control(ResourceError::native(-734)))
            );
            assert!(parser.position() < 1000);
        }
        assert_eq!(events, 512);
        assert!(state.tickets.lock().unwrap().is_empty());
        assert_eq!(resources.failure(), Some(ResourceError::native(-734)));
        resources.validate_retired().unwrap();
    }

    #[test]
    fn paid_arc_queue_transports_every_actual_reserve_and_commit_fault_without_fallback() {
        let text = "M0 0A1000000000000000000 1000000000000000000 0 1 1 1000000000000000000 0";
        let (baseline, state) = resources(0, 0, 0);
        let values = paid_simple(text, &baseline).unwrap();
        assert!(values.len() > 256);
        let reserves = state.reserves.load(Ordering::Relaxed);
        let commits = state.commits.load(Ordering::Relaxed);
        assert!(reserves > 2 && commits == reserves);
        baseline.validate_retired().unwrap();
        for reserve in 1..=reserves {
            let (resources, state) = resources(reserve, 0, 0);
            assert_eq!(
                paid_simple(text, &resources),
                Err(ResourceError::native(-731))
            );
            assert_eq!(state.reserves.load(Ordering::Relaxed), reserve);
            assert!(state.tickets.lock().unwrap().is_empty());
            resources.validate_retired().unwrap();
        }
        for commit in 1..=commits {
            let (resources, state) = resources(0, commit, 0);
            assert_eq!(
                paid_simple(text, &resources),
                Err(ResourceError::native(-732))
            );
            assert_eq!(state.commits.load(Ordering::Relaxed), commit);
            assert!(state.tickets.lock().unwrap().is_empty());
            resources.validate_retired().unwrap();
        }
    }

    #[test]
    fn queue_growth_release_failure_stops_with_original_slot_even_if_host_checkpoint_is_ok() {
        let (resources, state) = resources(0, 0, 1);
        let result = paid_simple(
            "M0 0A1000000000000000000 1000000000000000000 0 1 1 1000000000000000000 0",
            &resources,
        );
        assert_eq!(result, Err(ResourceError::native(-733)));
        assert_eq!(resources.failure(), Some(ResourceError::native(-733)));
        assert_eq!(resources.statistics().release_failures, 1);
        let tickets = state.tickets.lock().unwrap();
        assert_eq!(tickets.len(), 1);
        assert_eq!(tickets[0].0, 1);
        assert!(resources.validate_retired().is_err());
    }
}

#[rustfmt::skip]
#[cfg(test)]
mod simple_tests {
    use super::*;

    macro_rules! test {
        ($name:ident, $text:expr, $( $seg:expr ),*) => (
            #[test]
            fn $name() {
                let mut s = SimplifyingPathParser::from($text);
                $(
                    assert_eq!(s.next().unwrap().unwrap(), $seg);
                )*

                if let Some(res) = s.next() {
                    assert!(res.is_err());
                }
            }
        )
    }

    test!(ignore_duplicated_close_paths, "M 10 20 L 30 40 Z Z Z Z",
        SimplePathSegment::MoveTo { x: 10.0, y: 20.0 },
        SimplePathSegment::LineTo { x: 30.0, y: 40.0 },
        SimplePathSegment::ClosePath
    );

    test!(relative_move_to, "m 30 40 110 120 -20 -130",
        SimplePathSegment::MoveTo { x: 30.0, y: 40.0 },
        SimplePathSegment::LineTo { x: 140.0, y: 160.0 },
        SimplePathSegment::LineTo { x: 120.0, y: 30.0 }
    );

    test!(smooth_curve_to_after_move_to, "M 30 40 S 171 45 180 155",
        SimplePathSegment::MoveTo { x: 30.0, y: 40.0 },
        SimplePathSegment::CurveTo { x1: 30.0, y1: 40.0, x2: 171.0, y2: 45.0, x: 180.0, y: 155.0 }
    );

    test!(smooth_curve_to_after_curve_to, "M 30 40 C 16 137 171 45 100 90 S 171 45 180 155",
        SimplePathSegment::MoveTo { x: 30.0, y: 40.0 },
        SimplePathSegment::CurveTo { x1: 16.0, y1: 137.0, x2: 171.0, y2: 45.0, x: 100.0, y: 90.0 },
        SimplePathSegment::CurveTo { x1: 29.0, y1: 135.0, x2: 171.0, y2: 45.0, x: 180.0, y: 155.0 }
    );

    test!(relative_smooth_curve_to_after_arc_to, "M 1 5 A 5 5 0 0 1 3 1 s 3 2 8 2",
        SimplePathSegment::MoveTo { x: 1.0, y: 5.0 },
        SimplePathSegment::CurveTo {
            x1: 1.0, y1: 3.4262134833347355,
            x2: 1.7409707866677877, y2: 1.9442719099991592,
            x: 2.9999999999999996, y: 1.0000000000000004
        },
        SimplePathSegment::CurveTo {
            x1: 2.9999999999999996, y1: 1.0000000000000004,
            x2: 6.0, y2: 3.0000000000000004,
            x: 11.0, y: 3.0000000000000004
        }
    );

    test!(smooth_quadratic_after_move_to, "M 30 40 T 180 155",
        SimplePathSegment::MoveTo { x: 30.0, y: 40.0 },
        SimplePathSegment::Quadratic { x1: 30.0, y1: 40.0, x: 180.0, y: 155.0 }
    );

    test!(smooth_quadratic_after_quadratic, "M 30 40 Q 171 45 100 90 T 160 180",
        SimplePathSegment::MoveTo { x: 30.0, y: 40.0 },
        SimplePathSegment::Quadratic { x1: 171.0, y1: 45.0, x: 100.0, y: 90.0 },
        SimplePathSegment::Quadratic { x1: 29.0, y1: 135.0, x: 160.0, y: 180.0 }
    );

    test!(relative_smooth_quadratic_after_quadratic, "M 30 40 Q 171 45 100 90 t 60 80",
        SimplePathSegment::MoveTo { x: 30.0, y: 40.0 },
        SimplePathSegment::Quadratic { x1: 171.0, y1: 45.0, x: 100.0, y: 90.0 },
        SimplePathSegment::Quadratic { x1: 29.0, y1: 135.0, x: 160.0, y: 170.0 }
    );

    test!(relative_smooth_quadratic_after_relative_quadratic, "M 30 40 q 171 45 50 40 t 60 80",
        SimplePathSegment::MoveTo { x: 30.0, y: 40.0 },
        SimplePathSegment::Quadratic { x1: 201.0, y1: 85.0, x: 80.0, y: 80.0 },
        SimplePathSegment::Quadratic { x1: -41.0, y1: 75.0, x: 140.0, y: 160.0 }
    );

    test!(smooth_quadratic_after_smooth_quadratic, "M 30 30 T 40 140 T 170 30",
        SimplePathSegment::MoveTo { x: 30.0, y: 30.0 },
        SimplePathSegment::Quadratic { x1: 30.0, y1: 30.0, x: 40.0, y: 140.0 },
        SimplePathSegment::Quadratic { x1: 50.0, y1: 250.0, x: 170.0, y: 30.0 }
    );

    test!(smooth_quadratic_after_relative_smooth_quadratic, "M 30 30 T 40 140 t 100 -30",
        SimplePathSegment::MoveTo { x: 30.0, y: 30.0 },
        SimplePathSegment::Quadratic { x1: 30.0, y1: 30.0, x: 40.0, y: 140.0 },
        SimplePathSegment::Quadratic { x1: 50.0, y1: 250.0, x: 140.0, y: 110.0 }
    );

    test!(smooth_quadratic_after_relative_quadratic, "M 30 30 T 40 140 q 30 100 120 -30",
        SimplePathSegment::MoveTo { x: 30.0, y: 30.0 },
        SimplePathSegment::Quadratic { x1: 30.0, y1: 30.0, x: 40.0, y: 140.0 },
        SimplePathSegment::Quadratic { x1: 70.0, y1: 240.0, x: 160.0, y: 110.0 }
    );

    test!(smooth_quadratic_after_relative_smooth_curve_to, "M 30 30 T 40 170 s 90 -20 90 -90",
        SimplePathSegment::MoveTo { x: 30.0, y: 30.0 },
        SimplePathSegment::Quadratic { x1: 30.0, y1: 30.0, x: 40.0, y: 170.0 },
        SimplePathSegment::CurveTo { x1: 40.0, y1: 170.0, x2: 130.0, y2: 150.0, x: 130.0, y: 80.0 }
    );

    test!(quadratic_after_smooth_quadratic, "M 30 30 T 40 140 Q 80 180 170 30",
        SimplePathSegment::MoveTo { x: 30.0, y: 30.0 },
        SimplePathSegment::Quadratic { x1: 30.0, y1: 30.0, x: 40.0, y: 140.0 },
        SimplePathSegment::Quadratic { x1: 80.0, y1: 180.0, x: 170.0, y: 30.0 }
    );

    test!(relative_smooth_quadratic_to_after_arc_to, "M 1 5 A 5 5 0 0 1 3 1 t 8 2",
        SimplePathSegment::MoveTo { x: 1.0, y: 5.0 },
        SimplePathSegment::CurveTo {
            x1: 1.0, y1: 3.4262134833347355,
            x2: 1.7409707866677877, y2: 1.9442719099991592,
            x: 2.9999999999999996, y: 1.0000000000000004
        },
        SimplePathSegment::Quadratic {
            x1: 2.9999999999999996, y1: 1.0000000000000004,
            x: 11.0, y: 3.0000000000000004
        }
    );

    test!(implicit_move_to_after_close_path, "M 10 20 L 30 40 Z L 50 60",
        SimplePathSegment::MoveTo { x: 10.0, y: 20.0 },
        SimplePathSegment::LineTo { x: 30.0, y: 40.0 },
        SimplePathSegment::ClosePath,
        SimplePathSegment::MoveTo { x: 10.0, y: 20.0 },
        SimplePathSegment::LineTo { x: 50.0, y: 60.0 }
    );

    #[test]
    fn arc_to() {
        let mut s = SimplifyingPathParser::from("M 30 40 A 40 30 20 1 1 150 100");
        assert_eq!(s.next().unwrap().unwrap(), SimplePathSegment::MoveTo { x: 30.0, y: 40.0 });
        let curve1 = s.next().unwrap().unwrap();
        let curve2 = s.next().unwrap().unwrap();
        if let Some(res) = s.next() {
            assert!(res.is_err());
        }

        if let SimplePathSegment::CurveTo { x1, y1, x2, y2, x, y } = curve1 {
            assert_eq!(x1.round(), 45.0);
            assert_eq!(y1.round(), 16.0);
            assert_eq!(x2.round(), 84.0);
            assert_eq!(y2.round(), 10.0);
            assert_eq!(x.round(), 117.0);
            assert_eq!(y.round(), 27.0);
        } else {
            panic!("invalid type");
        }

        if let SimplePathSegment::CurveTo { x1, y1, x2, y2, x, y } = curve2 {
            assert_eq!(x1.round(), 150.0);
            assert_eq!(y1.round(), 43.0);
            assert_eq!(x2.round(), 165.0);
            assert_eq!(y2.round(), 76.0);
            assert_eq!(x.round(), 150.0);
            assert_eq!(y.round(), 100.0);
        } else {
            panic!("invalid type");
        }
    }
}
