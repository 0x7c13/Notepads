// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE file in the project root for license information.
// ---------------------------------------------------------------------------------------------

//! Typed producer. Only validated scalar geometry reaches the SVG writer.
//! Every live byte buffer owns its exact receipt; no input SVG is accepted.

pub mod shapes;
pub mod style;

use crate::canonical::CanonicalError;
pub use crate::canonical::{PathCommand, PathSink, PathVerb, Rect};
use markdown_memory::{Family, FundedString, MemoryScope};
use std::fmt::{self, Write};

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Point {
    pub x: f64,
    pub y: f64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Color {
    pub rgb: u32,
    pub alpha: u8,
}
impl Color {
    pub const BLACK: Self = Self::rgb(0x000000);
    pub const WHITE: Self = Self::rgb(0xffffff);
    pub const fn rgb(rgb: u32) -> Self {
        Self { rgb, alpha: 255 }
    }
    pub const fn rgba(rgb: u32, alpha: u8) -> Self {
        Self { rgb, alpha }
    }
}

#[derive(Clone, Copy, Debug)]
pub struct Paint {
    pub fill: Option<Color>,
    pub stroke: Option<Color>,
    pub stroke_width: f64,
    pub opacity: f64,
    pub dash: [f64; 4],
    pub dash_len: usize,
}
impl Default for Paint {
    fn default() -> Self {
        Self {
            fill: Some(Color::WHITE),
            stroke: Some(Color::BLACK),
            stroke_width: 1.5,
            opacity: 1.0,
            dash: [0.0; 4],
            dash_len: 0,
        }
    }
}

pub struct OwnedScene {
    pub svg: FundedString,
    pub bounds: Rect,
    pub paths: usize,
    pub commands: usize,
}

pub struct SceneBuilder {
    body: FundedString,
    scope: MemoryScope,
    bounds: Rect,
    has_ink: bool,
    active: bool,
    stroke_pad: f64,
    offset: Point,
    commands: usize,
    paths: usize,
    byte_limit: usize,
    command_limit: usize,
    failure: Option<CanonicalError>,
}

impl SceneBuilder {
    pub fn new(scope: &MemoryScope, byte_limit: usize, command_limit: usize) -> Self {
        Self {
            body: FundedString::new_in(scope.clone(), Family::Svg),
            scope: scope.clone(),
            bounds: Rect::default(),
            has_ink: false,
            active: false,
            stroke_pad: 0.0,
            offset: Point::default(),
            commands: 0,
            paths: 0,
            byte_limit,
            command_limit,
            failure: None,
        }
    }
    pub fn scope(&self) -> &MemoryScope {
        &self.scope
    }
    fn put(&mut self, text: &str) -> Result<(), CanonicalError> {
        self.scope.charge_work(text.len())?;
        if self
            .body
            .len()
            .checked_add(text.len())
            .is_none_or(|len| len > self.byte_limit)
        {
            return Err(CanonicalError::Limited);
        }
        self.body.try_push_str(text)?;
        Ok(())
    }
    fn formatted(&mut self, args: fmt::Arguments<'_>) -> Result<(), CanonicalError> {
        self.write_fmt(args)
            .map_err(|_| self.failure.unwrap_or(CanonicalError::InvalidGeometry))
    }
    pub fn begin_path(&mut self, paint: Paint, offset: Point) -> Result<(), CanonicalError> {
        if self.active
            || !valid(offset.x)
            || !valid(offset.y)
            || !valid(paint.stroke_width)
            || paint.stroke_width < 0.0
            || !(0.0..=1.0).contains(&paint.opacity)
            || paint.dash_len > 4
            || paint.dash[..paint.dash_len]
                .iter()
                .any(|x| !valid(*x) || *x < 0.0)
        {
            return Err(CanonicalError::InvalidGeometry);
        }
        self.put("<path fill=\"")?;
        self.color(paint.fill)?;
        self.put("\" stroke=\"")?;
        self.color(paint.stroke)?;
        self.formatted(format_args!("\" stroke-width=\"{}\" opacity=\"{}\" fill-opacity=\"{}\" stroke-opacity=\"{}\" stroke-linejoin=\"round\" stroke-linecap=\"round\"", paint.stroke_width, paint.opacity,paint.fill.map_or(1.0,|c|c.alpha as f64/255.0),paint.stroke.map_or(1.0,|c|c.alpha as f64/255.0)))?;
        if paint.dash_len > 0 {
            self.put(" stroke-dasharray=\"")?;
            for (index, value) in paint.dash[..paint.dash_len].iter().enumerate() {
                if index > 0 {
                    self.put(" ")?;
                }
                self.formatted(format_args!("{value}"))?;
            }
            self.put("\"")?;
        }
        self.put(" d=\"")?;
        self.active = true;
        self.offset = offset;
        self.stroke_pad = if paint.stroke.is_some() {
            paint.stroke_width / 2.0
        } else {
            0.0
        };
        Ok(())
    }
    fn color(&mut self, color: Option<Color>) -> Result<(), CanonicalError> {
        match color {
            None => self.put("none"),
            Some(color) if color.rgb <= 0xffffff => {
                self.formatted(format_args!("#{:06x}", color.rgb))
            }
            Some(_) => Err(CanonicalError::InvalidGeometry),
        }
    }
    pub fn end_path(&mut self) -> Result<(), CanonicalError> {
        if !self.active {
            return Err(CanonicalError::InvalidGeometry);
        }
        self.put("\"/>")?;
        self.active = false;
        self.paths += 1;
        Ok(())
    }
    fn ink(&mut self, x: f64, y: f64) -> Result<(), CanonicalError> {
        if !valid(x) || !valid(y) {
            return Err(CanonicalError::InvalidGeometry);
        }
        let r = Rect {
            left: x - self.stroke_pad,
            top: y - self.stroke_pad,
            right: x + self.stroke_pad,
            bottom: y + self.stroke_pad,
        };
        if self.has_ink {
            self.bounds.left = self.bounds.left.min(r.left);
            self.bounds.top = self.bounds.top.min(r.top);
            self.bounds.right = self.bounds.right.max(r.right);
            self.bounds.bottom = self.bounds.bottom.max(r.bottom);
        } else {
            self.bounds = r;
            self.has_ink = true;
        }
        Ok(())
    }
    pub fn finish(self, margin: f64) -> Result<OwnedScene, CanonicalError> {
        if self.active || !valid(margin) || margin < 0.0 {
            return Err(CanonicalError::InvalidGeometry);
        }
        let mut bounds = if self.has_ink {
            self.bounds
        } else {
            Rect {
                left: 0.0,
                top: 0.0,
                right: 1.0,
                bottom: 1.0,
            }
        };
        bounds.left -= margin;
        bounds.top -= margin;
        bounds.right += margin;
        bounds.bottom += margin;
        let width = bounds.right - bounds.left;
        let height = bounds.bottom - bounds.top;
        if !valid(width) || !valid(height) || width <= 0.0 || height <= 0.0 {
            return Err(CanonicalError::InvalidGeometry);
        }
        // The complete body stays paid while the final owner is allocated.
        let capacity = self
            .body
            .len()
            .checked_add(512)
            .ok_or(CanonicalError::Limited)?;
        if capacity > self.byte_limit {
            return Err(CanonicalError::Limited);
        }
        let mut svg = FundedString::try_with_capacity(&self.scope, Family::Svg, capacity)?;
        write!(svg, "<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"{width}\" height=\"{height}\" viewBox=\"{} {} {width} {height}\">", bounds.left, bounds.top)
            .map_err(|_| CanonicalError::Memory(self.scope.failure().unwrap_or(markdown_memory::MemoryError::InvalidState)))?;
        self.scope.charge_work(self.body.len())?;
        svg.try_push_str(&self.body)?;
        svg.try_push_str("</svg>")?;
        Ok(OwnedScene {
            svg,
            bounds,
            paths: self.paths,
            commands: self.commands,
        })
    }
}

impl Write for SceneBuilder {
    fn write_str(&mut self, text: &str) -> fmt::Result {
        self.put(text).map_err(|error| {
            self.failure = Some(error);
            fmt::Error
        })
    }
}

impl PathSink for SceneBuilder {
    fn push(&mut self, c: PathCommand) -> Result<(), CanonicalError> {
        if !self.active || self.commands >= self.command_limit {
            return Err(CanonicalError::Limited);
        }
        self.scope.charge_work(1)?;
        self.commands += 1;
        let (x, y) = (c.x1 + self.offset.x, c.y1 + self.offset.y);
        match c.verb {
            PathVerb::Move | PathVerb::Line => {
                self.ink(x, y)?;
                let verb = if c.verb == PathVerb::Move { 'M' } else { 'L' };
                self.formatted(format_args!("{verb}{x} {y} "))
            }
            PathVerb::Quad => {
                let (x2, y2) = (c.x2 + self.offset.x, c.y2 + self.offset.y);
                self.ink(x, y)?;
                self.ink(x2, y2)?;
                self.formatted(format_args!("Q{x} {y} {x2} {y2} "))
            }
            PathVerb::Cubic => {
                let (x2, y2, x3, y3) = (
                    c.x2 + self.offset.x,
                    c.y2 + self.offset.y,
                    c.x3 + self.offset.x,
                    c.y3 + self.offset.y,
                );
                self.ink(x, y)?;
                self.ink(x2, y2)?;
                self.ink(x3, y3)?;
                self.formatted(format_args!("C{x} {y} {x2} {y2} {x3} {y3} "))
            }
            PathVerb::Close => self.put("Z "),
        }
    }
}

pub(crate) fn valid(value: f64) -> bool {
    value.is_finite() && value.abs() <= 1.0e7
}
pub fn translated(rect: Rect, point: Point) -> Rect {
    Rect {
        left: rect.left + point.x,
        right: rect.right + point.x,
        top: rect.top + point.y,
        bottom: rect.bottom + point.y,
    }
}
pub fn intersects(a: Rect, b: Rect) -> bool {
    a.left < b.right && a.right > b.left && a.top < b.bottom && a.bottom > b.top
}
