// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE file in the project root for license information.
// ---------------------------------------------------------------------------------------------

//! Independent admission for the typed native producer. This deliberately
//! accepts its svg/path vocabulary only, with absolute commands and no CSS,
//! text, transforms, clips, references or externally supplied SVG input.

use super::{
    geometry::{Numbers, number},
    *,
};

#[derive(Clone, Copy, Debug)]
pub struct ArtifactFacts {
    pub bounds: Rect,
    pub paths: usize,
    pub commands: usize,
}

pub fn accept_artifact(
    bytes: FundedString,
    facts: ArtifactFacts,
    scope: &MemoryScope,
    control: &OperationControl,
    limits: CanonicalLimits,
) -> Result<CanonicalDocument, CanonicalError> {
    if !bytes.scope().same_root(scope) || control.memory_scope().is_none_or(|s| !s.same_root(scope))
    {
        return Err(CanonicalError::Memory(MemoryError::Unfunded));
    }
    let hard = CanonicalLimits::default();
    if bytes.len() > limits.output_bytes
        || limits.output_bytes > hard.output_bytes
        || limits.commands > hard.commands
        || limits.elements > hard.elements
        || facts.commands > limits.commands
        || facts.paths > limits.elements
    {
        return Err(CanonicalError::Limited);
    }
    checkpoint(scope, control)?;
    scope.charge_work(bytes.len())?;
    let value = bytes.as_str();
    if !value.chars().all(|c| matches!(c,'\t'|'\r'|'\n'|'\u{20}'..='\u{d7ff}'|'\u{e000}'..='\u{fffd}'|'\u{10000}'..='\u{10ffff}')) {
        return Err(CanonicalError::MalformedXml);
    }
    let mut reader = Reader { value, cursor: 0 };
    let root = reader.open()?;
    if root.tag != "svg" || root.closed {
        return Err(CanonicalError::MalformedXml);
    }
    for (key, _) in root.iter() {
        if !matches!(key, "xmlns" | "width" | "height" | "viewBox") {
            return Err(CanonicalError::UnsupportedProperty);
        }
    }
    if root.get("xmlns") != Some("http://www.w3.org/2000/svg") {
        return Err(CanonicalError::MalformedXml);
    }
    let width = number(root.required("width")?)?;
    let height = number(root.required("height")?)?;
    if width <= 0.0 || height <= 0.0 {
        return Err(CanonicalError::InvalidGeometry);
    }
    let mut numbers = Numbers::new(root.required("viewBox")?);
    let view = [
        numbers.next()?,
        numbers.next()?,
        numbers.next()?,
        numbers.next()?,
    ];
    if !numbers.done()
        || view[2] <= 0.0
        || view[3] <= 0.0
        || !same(width, view[2])
        || !same(height, view[3])
    {
        return Err(CanonicalError::InvalidGeometry);
    }
    let bounds = Rect {
        left: view[0],
        top: view[1],
        right: view[0] + view[2],
        bottom: view[1] + view[3],
    };
    if ![
        bounds.left,
        bounds.right,
        bounds.top,
        bounds.bottom,
        facts.bounds.left,
        facts.bounds.top,
        facts.bounds.right,
        facts.bounds.bottom,
    ]
    .into_iter()
    .all(super::geometry::valid)
        || !same(bounds.left, facts.bounds.left)
        || !same(bounds.top, facts.bounds.top)
        || !same(bounds.right, facts.bounds.right)
        || !same(bounds.bottom, facts.bounds.bottom)
    {
        return Err(CanonicalError::InvalidGeometry);
    }
    let mut paths = 0;
    let mut commands = 0;
    loop {
        checkpoint(scope, control)?;
        reader.whitespace();
        if reader.value[reader.cursor..].starts_with("</svg>") {
            reader.cursor += 6;
            break;
        }
        let path = reader.open()?;
        if path.tag != "path" || !path.closed {
            return Err(CanonicalError::UnsupportedElement);
        }
        paths += 1;
        if paths > limits.elements {
            return Err(CanonicalError::Limited);
        }
        for (key, _) in path.iter() {
            if !matches!(
                key,
                "fill"
                    | "stroke"
                    | "stroke-width"
                    | "opacity"
                    | "fill-opacity"
                    | "stroke-opacity"
                    | "stroke-linejoin"
                    | "stroke-linecap"
                    | "stroke-dasharray"
                    | "d"
            ) {
                return Err(CanonicalError::UnsupportedProperty);
            }
        }
        let _ = paint(path.required("fill")?)?;
        let stroke = paint(path.required("stroke")?)?;
        let stroke_width = number(path.required("stroke-width")?)?;
        let opacity = number(path.required("opacity")?)?;
        for name in ["fill-opacity", "stroke-opacity"] {
            if let Some(value) = path.get(name)
                && !(0.0..=1.0).contains(&number(value)?)
            {
                return Err(CanonicalError::InvalidGeometry);
            }
        }
        if stroke_width < 0.0
            || !(0.0..=1.0).contains(&opacity)
            || path.get("stroke-linejoin") != Some("round")
            || path.get("stroke-linecap") != Some("round")
        {
            return Err(CanonicalError::InvalidGeometry);
        }
        if let Some(dash) = path.get("stroke-dasharray") {
            let mut n = Numbers::new(dash);
            let mut count = 0;
            while !n.done() {
                if n.next()? < 0.0 {
                    return Err(CanonicalError::InvalidGeometry);
                }
                count += 1;
                if count > 4 {
                    return Err(CanonicalError::Limited);
                }
            }
            if count == 0 {
                return Err(CanonicalError::InvalidGeometry);
            }
        }
        let pad = if stroke { stroke_width / 2.0 } else { 0.0 };
        let count = validate_path(
            path.required("d")?,
            bounds,
            pad,
            scope,
            control,
            limits.commands.saturating_sub(commands),
        )?;
        commands += count;
    }
    reader.whitespace();
    if reader.cursor != value.len() || paths != facts.paths || commands != facts.commands {
        return Err(CanonicalError::MalformedXml);
    }
    checkpoint(scope, control)?;
    Ok(CanonicalDocument {
        bytes,
        width,
        height,
        path_commands: commands,
        text_runs: 0,
    })
}

fn same(a: f64, b: f64) -> bool {
    (a - b).abs() <= 1e-5
}
fn paint(value: &str) -> Result<bool, CanonicalError> {
    if value == "none" {
        return Ok(false);
    }
    if value.len() == 7
        && value.starts_with('#')
        && value.as_bytes()[1..].iter().all(u8::is_ascii_hexdigit)
    {
        return Ok(true);
    }
    Err(CanonicalError::UnsupportedProperty)
}

fn validate_path(
    value: &str,
    bounds: Rect,
    pad: f64,
    scope: &MemoryScope,
    control: &OperationControl,
    limit: usize,
) -> Result<usize, CanonicalError> {
    let mut n = Numbers::new(value);
    let mut commands = 0;
    let mut moved = false;
    while !n.done() {
        checkpoint(scope, control)?;
        if commands >= limit {
            return Err(CanonicalError::Limited);
        }
        let command = *n
            .rest
            .as_bytes()
            .first()
            .ok_or(CanonicalError::InvalidGeometry)?;
        if !matches!(command, b'M' | b'L' | b'Q' | b'C' | b'Z') {
            return Err(CanonicalError::InvalidGeometry);
        }
        n.rest = &n.rest[1..];
        let points = match command {
            b'M' => {
                moved = true;
                1
            }
            b'L' if moved => 1,
            b'Q' if moved => 2,
            b'C' if moved => 3,
            b'Z' if moved => 0,
            _ => return Err(CanonicalError::InvalidGeometry),
        };
        for _ in 0..points {
            let x = n.next()?;
            let y = n.next()?;
            // A Bezier lies in its control hull. With round joins/caps, padding
            // every hull point by half the stroke is a conservative ink bound.
            // This is independent of the producer's accumulator, not an exact
            // pixel or tightly fitted glyph-box claim.
            if x - pad < bounds.left - 1e-5
                || x + pad > bounds.right + 1e-5
                || y - pad < bounds.top - 1e-5
                || y + pad > bounds.bottom + 1e-5
            {
                return Err(CanonicalError::InvalidGeometry);
            }
        }
        commands += 1;
    }
    Ok(commands)
}

struct Element<'a> {
    tag: &'a str,
    attributes: [(&'a str, &'a str); 12],
    count: usize,
    closed: bool,
}
impl<'a> Element<'a> {
    fn iter(&self) -> impl Iterator<Item = (&'a str, &'a str)> + '_ {
        self.attributes[..self.count].iter().copied()
    }
    fn get(&self, key: &str) -> Option<&'a str> {
        self.iter().find(|(k, _)| *k == key).map(|(_, v)| v)
    }
    fn required(&self, key: &str) -> Result<&'a str, CanonicalError> {
        self.get(key).ok_or(CanonicalError::MalformedXml)
    }
}
struct Reader<'a> {
    value: &'a str,
    cursor: usize,
}
impl<'a> Reader<'a> {
    fn whitespace(&mut self) {
        while self
            .value
            .as_bytes()
            .get(self.cursor)
            .is_some_and(u8::is_ascii_whitespace)
        {
            self.cursor += 1;
        }
    }
    fn open(&mut self) -> Result<Element<'a>, CanonicalError> {
        self.whitespace();
        let b = self.value.as_bytes();
        if b.get(self.cursor) != Some(&b'<') {
            return Err(CanonicalError::MalformedXml);
        }
        self.cursor += 1;
        let start = self.cursor;
        while b.get(self.cursor).is_some_and(u8::is_ascii_alphanumeric) {
            self.cursor += 1;
        }
        if start == self.cursor {
            return Err(CanonicalError::MalformedXml);
        }
        let mut result = Element {
            tag: &self.value[start..self.cursor],
            attributes: [("", ""); 12],
            count: 0,
            closed: false,
        };
        loop {
            self.whitespace();
            if b.get(self.cursor) == Some(&b'>') {
                self.cursor += 1;
                return Ok(result);
            }
            if b.get(self.cursor) == Some(&b'/') && b.get(self.cursor + 1) == Some(&b'>') {
                self.cursor += 2;
                result.closed = true;
                return Ok(result);
            }
            let start = self.cursor;
            while b
                .get(self.cursor)
                .is_some_and(|b| b.is_ascii_alphanumeric() || *b == b'-')
            {
                self.cursor += 1;
            }
            if start == self.cursor {
                return Err(CanonicalError::MalformedXml);
            }
            let key = &self.value[start..self.cursor];
            if result.count == result.attributes.len() || result.get(key).is_some() {
                return Err(CanonicalError::MalformedXml);
            }
            if b.get(self.cursor) != Some(&b'=') || b.get(self.cursor + 1) != Some(&b'"') {
                return Err(CanonicalError::MalformedXml);
            }
            self.cursor += 2;
            let start = self.cursor;
            while b.get(self.cursor).is_some_and(|b| *b != b'"') {
                if matches!(b[self.cursor], b'<' | b'&') {
                    return Err(CanonicalError::MalformedXml);
                }
                self.cursor += 1;
            }
            if b.get(self.cursor) != Some(&b'"') {
                return Err(CanonicalError::MalformedXml);
            }
            let value = &self.value[start..self.cursor];
            self.cursor += 1;
            result.attributes[result.count] = (key, value);
            result.count += 1;
        }
    }
}
