// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE file in the project root for license information.
// ---------------------------------------------------------------------------------------------

use super::CanonicalError;

#[derive(Clone, Copy, Debug, Default)]
pub struct Rect {
    pub left: f64,
    pub top: f64,
    pub right: f64,
    pub bottom: f64,
}

#[derive(Clone, Copy, Debug)]
pub struct Matrix {
    pub a: f64,
    pub b: f64,
    pub c: f64,
    pub d: f64,
    pub e: f64,
    pub f: f64,
}

impl Default for Matrix {
    fn default() -> Self {
        Self {
            a: 1.0,
            b: 0.0,
            c: 0.0,
            d: 1.0,
            e: 0.0,
            f: 0.0,
        }
    }
}

impl Matrix {
    pub fn then(self, rhs: Self) -> Result<Self, CanonicalError> {
        let out = Self {
            a: self.a * rhs.a + self.c * rhs.b,
            b: self.b * rhs.a + self.d * rhs.b,
            c: self.a * rhs.c + self.c * rhs.d,
            d: self.b * rhs.c + self.d * rhs.d,
            e: self.a * rhs.e + self.c * rhs.f + self.e,
            f: self.b * rhs.e + self.d * rhs.f + self.f,
        };
        if [out.a, out.b, out.c, out.d, out.e, out.f]
            .into_iter()
            .all(valid)
        {
            Ok(out)
        } else {
            Err(CanonicalError::InvalidGeometry)
        }
    }

    pub fn translate(x: f64, y: f64) -> Self {
        Self {
            e: x,
            f: y,
            ..Self::default()
        }
    }
    pub fn scale(x: f64, y: f64) -> Self {
        Self {
            a: x,
            d: y,
            ..Self::default()
        }
    }
    pub fn rotate(angle: f64) -> Self {
        let (s, c) = angle.sin_cos();
        Self {
            a: c,
            b: s,
            c: -s,
            d: c,
            ..Self::default()
        }
    }
}

pub(crate) fn valid(value: f64) -> bool {
    value.is_finite() && value.abs() <= 1.0e9
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PathVerb {
    Move,
    Line,
    Quad,
    Cubic,
    Close,
}

#[derive(Clone, Copy, Debug)]
pub struct PathCommand {
    pub verb: PathVerb,
    pub x1: f64,
    pub y1: f64,
    pub x2: f64,
    pub y2: f64,
    pub x3: f64,
    pub y3: f64,
}

impl PathCommand {
    pub fn move_to(x: f64, y: f64) -> Self {
        Self {
            verb: PathVerb::Move,
            x1: x,
            y1: y,
            x2: 0.0,
            y2: 0.0,
            x3: 0.0,
            y3: 0.0,
        }
    }
    pub fn line_to(x: f64, y: f64) -> Self {
        Self {
            verb: PathVerb::Line,
            ..Self::move_to(x, y)
        }
    }
    pub fn cubic(x1: f64, y1: f64, x2: f64, y2: f64, x3: f64, y3: f64) -> Self {
        Self {
            verb: PathVerb::Cubic,
            x1,
            y1,
            x2,
            y2,
            x3,
            y3,
        }
    }
    pub fn close() -> Self {
        Self {
            verb: PathVerb::Close,
            ..Self::move_to(0.0, 0.0)
        }
    }
}

pub(crate) struct Numbers<'a> {
    pub rest: &'a str,
}

impl<'a> Numbers<'a> {
    pub fn new(value: &'a str) -> Self {
        Self { rest: value }
    }
    pub fn skip(&mut self) {
        self.rest = self
            .rest
            .trim_start_matches(|c: char| c.is_ascii_whitespace() || c == ',');
    }
    pub fn next(&mut self) -> Result<f64, CanonicalError> {
        self.skip();
        let bytes = self.rest.as_bytes();
        let mut n = 0;
        if matches!(bytes.first(), Some(b'+' | b'-')) {
            n += 1;
        }
        let mut digits = 0;
        while bytes.get(n).is_some_and(u8::is_ascii_digit) {
            n += 1;
            digits += 1;
        }
        if bytes.get(n) == Some(&b'.') {
            n += 1;
            while bytes.get(n).is_some_and(u8::is_ascii_digit) {
                n += 1;
                digits += 1;
            }
        }
        if digits == 0 {
            return Err(CanonicalError::InvalidGeometry);
        }
        if matches!(bytes.get(n), Some(b'e' | b'E')) {
            n += 1;
            if matches!(bytes.get(n), Some(b'+' | b'-')) {
                n += 1;
            }
            let start = n;
            while bytes.get(n).is_some_and(u8::is_ascii_digit) {
                n += 1;
            }
            if start == n {
                return Err(CanonicalError::InvalidGeometry);
            }
        }
        let value = self.rest[..n]
            .parse::<f64>()
            .map_err(|_| CanonicalError::InvalidGeometry)?;
        if !valid(value) {
            return Err(CanonicalError::InvalidGeometry);
        }
        self.rest = &self.rest[n..];
        Ok(value)
    }
    pub fn done(&mut self) -> bool {
        self.skip();
        self.rest.is_empty()
    }
}

pub(crate) fn number(value: &str) -> Result<f64, CanonicalError> {
    let mut n = Numbers::new(value);
    let v = n.next()?;
    if n.done() {
        Ok(v)
    } else {
        Err(CanonicalError::InvalidGeometry)
    }
}

pub(crate) fn length(value: &str, em: f64, ex: f64, percent: f64) -> Result<f64, CanonicalError> {
    let value = value.trim();
    let (value, scale) = if let Some(v) = value.strip_suffix("em") {
        (v, em)
    } else if let Some(v) = value.strip_suffix("ex") {
        (v, ex)
    } else if let Some(v) = value.strip_suffix("px") {
        (v, 1.0)
    } else if let Some(v) = value.strip_suffix("pt") {
        (v, 96.0 / 72.0)
    } else if let Some(v) = value.strip_suffix('%') {
        (v, percent / 100.0)
    } else {
        (value, 1.0)
    };
    let out = number(value)? * scale;
    if valid(out) {
        Ok(out)
    } else {
        Err(CanonicalError::InvalidGeometry)
    }
}

pub(crate) fn transform(value: &str) -> Result<Matrix, CanonicalError> {
    let mut rest = value.trim();
    let mut out = Matrix::default();
    let mut operations = 0;
    while !rest.is_empty() {
        operations += 1;
        if operations > 64 {
            return Err(CanonicalError::Limited);
        }
        let open = rest.find('(').ok_or(CanonicalError::InvalidGeometry)?;
        let name = rest[..open].trim();
        let close = rest.find(')').ok_or(CanonicalError::InvalidGeometry)?;
        if close < open {
            return Err(CanonicalError::InvalidGeometry);
        }
        let mut n = Numbers::new(&rest[open + 1..close]);
        let first = n.next()?;
        let matrix = match name {
            "matrix" => Matrix {
                a: first,
                b: n.next()?,
                c: n.next()?,
                d: n.next()?,
                e: n.next()?,
                f: n.next()?,
            },
            "translate" => Matrix::translate(first, if n.done() { 0.0 } else { n.next()? }),
            "scale" => Matrix::scale(first, if n.done() { first } else { n.next()? }),
            "rotate" => {
                let rotation = Matrix::rotate(first.to_radians());
                if n.done() {
                    rotation
                } else {
                    let x = n.next()?;
                    let y = n.next()?;
                    Matrix::translate(x, y)
                        .then(rotation)?
                        .then(Matrix::translate(-x, -y))?
                }
            }
            "skewX" => Matrix {
                c: first.to_radians().tan(),
                ..Matrix::default()
            },
            "skewY" => Matrix {
                b: first.to_radians().tan(),
                ..Matrix::default()
            },
            _ => return Err(CanonicalError::InvalidGeometry),
        };
        if !n.done() {
            return Err(CanonicalError::InvalidGeometry);
        }
        out = out.then(matrix)?;
        rest = rest[close + 1..].trim_start_matches(|c: char| c.is_ascii_whitespace() || c == ',');
    }
    Ok(out)
}

#[derive(Clone, Copy, Debug, Default)]
pub(crate) struct PathInfo {
    pub start: (f64, f64),
    pub end: (f64, f64),
    pub start_tangent: (f64, f64),
    pub end_tangent: (f64, f64),
    pub commands: usize,
}

pub(crate) fn path_info(value: &str, limit: usize) -> Result<PathInfo, CanonicalError> {
    let mut n = Numbers::new(value);
    let mut command = ' ';
    let mut p = (0.0, 0.0);
    let mut begin = p;
    let mut out = PathInfo::default();
    let mut has_start = false;
    let mut prior_control = p;
    let mut prior = ' ';
    while !n.done() {
        if n.rest.as_bytes()[0].is_ascii_alphabetic() {
            command = n.rest.as_bytes()[0] as char;
            n.rest = &n.rest[1..];
        }
        let relative = command.is_ascii_lowercase();
        let upper = command.to_ascii_uppercase();
        let old = p;
        let mut tangent = None;
        let point = |n: &mut Numbers<'_>| -> Result<(f64, f64), CanonicalError> {
            let x = n.next()?;
            let y = n.next()?;
            Ok(if relative {
                (x + old.0, y + old.1)
            } else {
                (x, y)
            })
        };
        match upper {
            'M' => {
                p = point(&mut n)?;
                begin = p;
                if !has_start {
                    out.start = p;
                    has_start = true;
                }
                command = if relative { 'l' } else { 'L' };
            }
            'L' => {
                p = point(&mut n)?;
                tangent = Some((p.0 - old.0, p.1 - old.1));
            }
            'H' => {
                let x = n.next()?;
                p = (if relative { x + old.0 } else { x }, old.1);
                tangent = Some((p.0 - old.0, 0.0));
            }
            'V' => {
                let y = n.next()?;
                p = (old.0, if relative { y + old.1 } else { y });
                tangent = Some((0.0, p.1 - old.1));
            }
            'C' => {
                let c1 = point(&mut n)?;
                let c2 = point(&mut n)?;
                p = point(&mut n)?;
                if out.start_tangent == (0.0, 0.0) {
                    out.start_tangent =
                        nonzero((c1.0 - old.0, c1.1 - old.1), (p.0 - old.0, p.1 - old.1));
                }
                tangent = Some(nonzero(
                    (p.0 - c2.0, p.1 - c2.1),
                    (p.0 - old.0, p.1 - old.1),
                ));
                prior_control = c2;
            }
            'S' => {
                let c1 = if matches!(prior, 'C' | 'S') {
                    (2.0 * old.0 - prior_control.0, 2.0 * old.1 - prior_control.1)
                } else {
                    old
                };
                let c2 = point(&mut n)?;
                p = point(&mut n)?;
                if out.start_tangent == (0.0, 0.0) {
                    out.start_tangent =
                        nonzero((c1.0 - old.0, c1.1 - old.1), (p.0 - old.0, p.1 - old.1));
                }
                tangent = Some(nonzero(
                    (p.0 - c2.0, p.1 - c2.1),
                    (p.0 - old.0, p.1 - old.1),
                ));
                prior_control = c2;
            }
            'Q' => {
                let c = point(&mut n)?;
                p = point(&mut n)?;
                if out.start_tangent == (0.0, 0.0) {
                    out.start_tangent =
                        nonzero((c.0 - old.0, c.1 - old.1), (p.0 - old.0, p.1 - old.1));
                }
                tangent = Some(nonzero((p.0 - c.0, p.1 - c.1), (p.0 - old.0, p.1 - old.1)));
                prior_control = c;
            }
            'T' => {
                let c = if matches!(prior, 'Q' | 'T') {
                    (2.0 * old.0 - prior_control.0, 2.0 * old.1 - prior_control.1)
                } else {
                    old
                };
                p = point(&mut n)?;
                tangent = Some(nonzero((p.0 - c.0, p.1 - c.1), (p.0 - old.0, p.1 - old.1)));
                prior_control = c;
            }
            'A' => {
                let rx = n.next()?.abs();
                let ry = n.next()?.abs();
                let angle = n.next()?.to_radians();
                let large = n.next()?;
                let sweep = n.next()?;
                if !matches!(large, 0.0 | 1.0) || !matches!(sweep, 0.0 | 1.0) {
                    return Err(CanonicalError::InvalidGeometry);
                }
                p = point(&mut n)?;
                let (start, end) = arc_tangents(old, p, rx, ry, angle, large != 0.0, sweep != 0.0);
                if out.start_tangent == (0.0, 0.0) {
                    out.start_tangent = start;
                }
                tangent = Some(end);
            }
            'Z' => {
                p = begin;
                tangent = Some((p.0 - old.0, p.1 - old.1));
                command = ' ';
            }
            _ => return Err(CanonicalError::InvalidGeometry),
        }
        if let Some(t) = tangent {
            if out.start_tangent == (0.0, 0.0) {
                out.start_tangent = t;
            }
            if t != (0.0, 0.0) {
                out.end_tangent = t;
            }
        }
        if ![p.0, p.1, prior_control.0, prior_control.1]
            .into_iter()
            .all(valid)
        {
            return Err(CanonicalError::InvalidGeometry);
        }
        out.end = p;
        out.commands += 1;
        if out.commands > limit {
            return Err(CanonicalError::Limited);
        }
        prior = upper;
    }
    Ok(out)
}

fn nonzero(a: (f64, f64), b: (f64, f64)) -> (f64, f64) {
    if a == (0.0, 0.0) { b } else { a }
}

fn arc_tangents(
    p0: (f64, f64),
    p1: (f64, f64),
    mut rx: f64,
    mut ry: f64,
    phi: f64,
    large: bool,
    sweep: bool,
) -> ((f64, f64), (f64, f64)) {
    if rx == 0.0 || ry == 0.0 || p0 == p1 {
        return ((p1.0 - p0.0, p1.1 - p0.1), (p1.0 - p0.0, p1.1 - p0.1));
    }
    let (s, c) = phi.sin_cos();
    let dx = (p0.0 - p1.0) / 2.0;
    let dy = (p0.1 - p1.1) / 2.0;
    let x = c * dx + s * dy;
    let y = -s * dx + c * dy;
    let lambda = x * x / (rx * rx) + y * y / (ry * ry);
    if lambda > 1.0 {
        let k = lambda.sqrt();
        rx *= k;
        ry *= k;
    }
    let numerator = (rx * rx * ry * ry - rx * rx * y * y - ry * ry * x * x).max(0.0);
    let denominator = rx * rx * y * y + ry * ry * x * x;
    let factor = if denominator == 0.0 {
        0.0
    } else {
        (numerator / denominator).sqrt() * if large == sweep { -1.0 } else { 1.0 }
    };
    let cx = factor * rx * y / ry;
    let cy = -factor * ry * x / rx;
    let tangent = |px: f64, py: f64| {
        let ux = (px - cx) / rx;
        let uy = (py - cy) / ry;
        let sign = if sweep { 1.0 } else { -1.0 };
        (
            sign * (-c * rx * uy - s * ry * ux),
            sign * (-s * rx * uy + c * ry * ux),
        )
    };
    (tangent(x, y), tangent(-x, -y))
}
