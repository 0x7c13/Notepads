// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE file in the project root for license information.
// ---------------------------------------------------------------------------------------------

use super::*;

#[derive(Debug)]
enum Scalar {
    Null,
    Text(String),
    Bool(bool),
    Signed(i64),
    Unsigned(u64),
    Float(f64),
}

#[derive(Debug)]
struct Field {
    key: String,
    value: Scalar,
}

#[derive(Debug)]
pub(crate) struct FlatShapeData {
    fields: Vec<Field>,
}

impl FlatShapeData {
    pub(super) fn parse(input: &str, control: &OperationControl) -> NativeResult<Self> {
        let mut reader = Reader {
            input,
            position: 0,
            control,
        };
        let mut fields = Vec::new();
        loop {
            reader.whitespace();
            if reader.position == input.len() {
                break;
            }
            let key = reader.key()?;
            reader.horizontal();
            if reader.take() != Some(':') {
                return reader.invalid();
            }
            reader.horizontal();
            let value = reader.scalar()?;
            // Duplicate fields are interpreted by reverse lookup, so the
            // final scalar (including null) is the sole effective value.
            active(
                memory::push(&mut fields, Field { key, value }, control),
                control,
            )?;
            reader.horizontal();
            if reader.position == input.len() {
                break;
            }
            match reader.take() {
                Some(',') | Some('\n') | Some('\r') => {}
                _ => return reader.invalid(),
            }
        }
        Ok(Self { fields })
    }

    fn get<'a>(
        &'a self,
        key: &str,
        control: &OperationControl,
    ) -> NativeResult<Option<&'a Scalar>> {
        for field in self.fields.iter().rev() {
            active(control.checkpoint(), control)?;
            active(
                memory::result(
                    control
                        .memory_scope()
                        .ok_or(NativeParseError::Memory(MemoryError::Unfunded))?
                        .charge_work(1),
                    control,
                ),
                control,
            )?;
            if field.key == key {
                return Ok(Some(&field.value));
            }
        }
        Ok(None)
    }

    pub(crate) fn apply_node(
        &self,
        node: &mut Node,
        control: &OperationControl,
    ) -> NativeResult<()> {
        for field in &self.fields {
            if !matches!(field.key.as_str(), "shape" | "label" | "labelType") {
                return Err(NativeParseError::Unsupported {
                    feature: if matches!(
                        field.key.as_str(),
                        "icon" | "img" | "form" | "pos" | "w" | "h" | "constraint"
                    ) {
                        NativeFeature::Assets
                    } else {
                        NativeFeature::Metadata
                    },
                });
            }
        }
        if let Some(Scalar::Text(shape)) = self.get("shape", control)? {
            // Compare scalars without allocating a lowercase copy.
            if shape.chars().any(char::is_uppercase)
                || shape.contains('_')
                || !crate::diagrams::shapes::is_valid_pinned_shape(shape)
            {
                return Err(NativeParseError::Syntax { span: None });
            }
            if !supported_shape(shape) {
                return Err(NativeParseError::Unsupported {
                    feature: NativeFeature::Shape,
                });
            }
            node.shape = Some(active(memory::clone_owned(shape, control), control)?);
        }
        if let Some(value) = self.get("label", control)? {
            if let Some(label) = project(value, control)? {
                node.label = Some(label);
                node.label_span = None;
                node.label_selection = None;
                node.label_type = match self.get("labelType", control)? {
                    Some(Scalar::Text(kind)) if kind == "text" => TitleKind::Text,
                    Some(Scalar::Text(kind)) if kind == "string" => TitleKind::String,
                    _ => TitleKind::Markdown,
                };
            }
        }
        Ok(())
    }
}

pub(super) fn supported_shape(shape: &str) -> bool {
    matches!(
        shape,
        "rect"
            | "rectangle"
            | "square"
            | "rounded"
            | "round"
            | "stadium"
            | "pill"
            | "terminal"
            | "subroutine"
            | "fr-rect"
            | "framed-rectangle"
            | "cylinder"
            | "cyl"
            | "database"
            | "db"
            | "circle"
            | "circ"
            | "doublecircle"
            | "double-circle"
            | "dbl-circ"
            | "diamond"
            | "diam"
            | "decision"
            | "hexagon"
            | "hex"
            | "prepare"
            | "lean-r"
            | "parallelogram"
            | "in-out"
            | "lean-l"
            | "parallelogram-alt"
            | "out-in"
            | "trapezoid"
            | "trap-b"
            | "priority"
            | "trapezoid-alt"
            | "trap-t"
            | "manual"
            | "odd"
            | "asymmetric"
            | "notch-rect"
    )
}

fn project(value: &Scalar, control: &OperationControl) -> NativeResult<Option<String>> {
    let value = match value {
        Scalar::Null => return Ok(None),
        Scalar::Text(value) => active(memory::clone_owned(value, control), control)?,
        Scalar::Bool(value) => active(
            memory::owned_string(if *value { "true" } else { "false" }, control),
            control,
        )?,
        Scalar::Signed(value) => active(
            memory::format_owned(format_args!("{value}"), control),
            control,
        )?,
        Scalar::Unsigned(value) => active(
            memory::format_owned(format_args!("{value}"), control),
            control,
        )?,
        Scalar::Float(value) => {
            let number = serde_json::Number::from_f64(*value)
                .ok_or(NativeParseError::Syntax { span: None })?;
            active(
                memory::format_owned(format_args!("{number}"), control),
                control,
            )?
        }
    };
    Ok(Some(value))
}

struct Reader<'a> {
    input: &'a str,
    position: usize,
    control: &'a OperationControl,
}
impl Reader<'_> {
    fn invalid<T>(&self) -> NativeResult<T> {
        Err(NativeParseError::Syntax { span: None })
    }
    fn peek(&self) -> Option<char> {
        self.input[self.position..].chars().next()
    }
    fn take(&mut self) -> Option<char> {
        let ch = self.peek()?;
        self.position += ch.len_utf8();
        Some(ch)
    }
    fn horizontal(&mut self) {
        while self.peek().is_some_and(|ch| matches!(ch, ' ' | '\t')) {
            self.take();
        }
    }
    fn whitespace(&mut self) {
        while self.peek().is_some_and(char::is_whitespace) {
            self.take();
        }
    }
    fn key(&mut self) -> NativeResult<String> {
        if self.peek().is_some_and(|ch| matches!(ch, '\'' | '"')) {
            return self.quoted();
        }
        let start = self.position;
        while self
            .peek()
            .is_some_and(|ch| ch != ':' && !ch.is_whitespace() && ch != ',')
        {
            self.take();
        }
        if self.position == start {
            return self.invalid();
        }
        let raw = &self.input[start..self.position];
        if raw.starts_with(['[', '{', '&', '*', '!', '|', '>']) {
            return Err(NativeParseError::Unsupported {
                feature: NativeFeature::ShapeData,
            });
        }
        active(memory::owned_string(raw, self.control), self.control)
    }
    fn scalar(&mut self) -> NativeResult<Scalar> {
        if self.peek().is_some_and(|ch| matches!(ch, '\'' | '"')) {
            return self.quoted().map(Scalar::Text);
        }
        let start = self.position;
        while self
            .peek()
            .is_some_and(|ch| !matches!(ch, ',' | '\r' | '\n'))
        {
            self.take();
        }
        let raw = self.input[start..self.position].trim();
        if raw.starts_with(['[', '{', '&', '*', '!', '|', '>']) {
            return Err(NativeParseError::Unsupported {
                feature: NativeFeature::ShapeData,
            });
        }
        if raw.contains(": ") || raw.contains(" #") {
            return Err(NativeParseError::Unsupported {
                feature: NativeFeature::ShapeData,
            });
        }
        if raw.is_empty() || matches!(raw, "null" | "Null" | "NULL" | "~") {
            return Ok(Scalar::Null);
        }
        if matches!(raw, "true" | "True" | "TRUE") {
            return Ok(Scalar::Bool(true));
        }
        if matches!(raw, "false" | "False" | "FALSE") {
            return Ok(Scalar::Bool(false));
        }
        if let Some(number) = number(raw, self.control)? {
            return Ok(number);
        }
        Ok(Scalar::Text(active(
            memory::owned_string(raw, self.control),
            self.control,
        )?))
    }
    fn quoted(&mut self) -> NativeResult<String> {
        let quote = self.take().ok_or(NativeParseError::Syntax { span: None })?;
        let region = self
            .control
            .memory_region()
            .ok_or(NativeParseError::Memory(MemoryError::Unfunded))?;
        let mut output = String::new();
        loop {
            active(self.control.checkpoint(), self.control)?;
            let ch = self.take().ok_or(NativeParseError::Syntax { span: None })?;
            if ch == quote {
                if quote == '\'' && self.peek() == Some('\'') {
                    self.take();
                } else {
                    return Ok(output);
                }
            } else if quote == '"' && ch == '\\' {
                let escaped = self.take().ok_or(NativeParseError::Syntax { span: None })?;
                let scalar = match escaped {
                    '0' => '\0',
                    'a' => '\u{7}',
                    'b' => '\u{8}',
                    't' | '\t' => '\t',
                    'n' => '\n',
                    'v' => '\u{b}',
                    'f' => '\u{c}',
                    'r' => '\r',
                    'e' => '\u{1b}',
                    ' ' => ' ',
                    '"' => '"',
                    '/' => '/',
                    '\\' => '\\',
                    'N' => '\u{85}',
                    '_' => '\u{a0}',
                    'L' => '\u{2028}',
                    'P' => '\u{2029}',
                    'x' | 'u' | 'U' => {
                        let digits = match escaped {
                            'x' => 2,
                            'u' => 4,
                            _ => 8,
                        };
                        let mut value = 0u32;
                        for _ in 0..digits {
                            let digit = self
                                .take()
                                .and_then(|ch| ch.to_digit(16))
                                .ok_or(NativeParseError::Syntax { span: None })?;
                            value = value
                                .checked_mul(16)
                                .and_then(|value| value.checked_add(digit))
                                .ok_or(NativeParseError::Syntax { span: None })?;
                        }
                        char::from_u32(value).ok_or(NativeParseError::Syntax { span: None })?
                    }
                    '\r' | '\n' => {
                        if escaped == '\r' && self.peek() == Some('\n') {
                            self.take();
                        }
                        self.whitespace();
                        continue;
                    }
                    _ => return self.invalid(),
                };
                let mut buffer = [0u8; 4];
                region
                    .try_push_str(
                        &mut output,
                        scalar.encode_utf8(&mut buffer),
                        markdown_memory::Family::Parse,
                    )
                    .map_err(NativeParseError::Memory)?;
                continue;
            }
            let mut buffer = [0u8; 4];
            region
                .try_push_str(
                    &mut output,
                    ch.encode_utf8(&mut buffer),
                    markdown_memory::Family::Parse,
                )
                .map_err(NativeParseError::Memory)?;
        }
    }
}

fn number(raw: &str, control: &OperationControl) -> NativeResult<Option<Scalar>> {
    if !raw
        .as_bytes()
        .first()
        .is_some_and(|first| first.is_ascii_digit() || matches!(first, b'+' | b'-' | b'.'))
    {
        return Ok(None);
    }
    let cleaned = if raw.contains('_') {
        let region = control
            .memory_region()
            .ok_or(NativeParseError::Memory(MemoryError::Unfunded))?;
        let mut value = region
            .try_string_capacity(raw.len(), markdown_memory::Family::Parse)
            .map_err(NativeParseError::Memory)?;
        for ch in raw.chars().filter(|ch| *ch != '_') {
            value.push(ch);
        }
        Some(value)
    } else {
        None
    };
    let clean = cleaned.as_deref().unwrap_or(raw);
    let (negative, body) = if let Some(rest) = clean.strip_prefix('-') {
        (true, rest)
    } else {
        (false, clean.strip_prefix('+').unwrap_or(clean))
    };
    let integer = if let Some(digits) = body.strip_prefix("0b").or_else(|| body.strip_prefix("0B"))
    {
        u128::from_str_radix(digits, 2).ok()
    } else if let Some(digits) = body.strip_prefix("0o").or_else(|| body.strip_prefix("0O")) {
        u128::from_str_radix(digits, 8).ok()
    } else if let Some(digits) = body.strip_prefix("0x").or_else(|| body.strip_prefix("0X")) {
        u128::from_str_radix(digits, 16).ok()
    } else if !body.is_empty() && body.bytes().all(|byte| byte.is_ascii_digit()) {
        body.parse::<u128>().ok()
    } else {
        None
    };
    if let Some(unsigned) = integer {
        if negative {
            if let Some(signed) = i128::try_from(unsigned).ok().and_then(i128::checked_neg) {
                return Ok(Some(if let Ok(value) = i64::try_from(signed) {
                    Scalar::Signed(value)
                } else {
                    Scalar::Float(signed as f64)
                }));
            }
        } else {
            return Ok(Some(if let Ok(value) = u64::try_from(unsigned) {
                Scalar::Unsigned(value)
            } else {
                Scalar::Float(unsigned as f64)
            }));
        }
    }
    if !body.is_empty()
        && body.bytes().any(|byte| byte.is_ascii_digit())
        && body
            .bytes()
            .all(|byte| byte.is_ascii_digit() || matches!(byte, b'.' | b'e' | b'E' | b'+' | b'-'))
    {
        if let Ok(mut value) = body.parse::<f64>() {
            if negative {
                value = -value;
            }
            if value.is_finite() {
                return Ok(Some(Scalar::Float(value)));
            }
        }
    }
    Ok(None)
}
