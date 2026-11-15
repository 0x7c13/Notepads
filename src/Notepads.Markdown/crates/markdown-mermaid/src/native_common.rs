// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE file in the project root for license information.
// ---------------------------------------------------------------------------------------------

//! Shared bounded primitives for native Mermaid diagram families.

use crate::{
    DiagramError,
    canonical::{CanonicalError, PathCommand, PathSink, Rect},
    labels::{CompiledLabel, LabelPolicy, compile_label},
    layout::LayoutPolicy,
    rich_text::{RichTextProvider, TextDirection},
    scene::{
        Color, Paint, Point, SceneBuilder,
        shapes::{self, Shape},
    },
};
use markdown_memory::{Family, FundedString, MemoryError, MemoryScope};
use merman_core::{CancelReason, OperationControl};

pub(crate) struct FlowchartEncoder<'a> {
    text: FundedString,
    control: &'a OperationControl,
}

impl<'a> FlowchartEncoder<'a> {
    pub fn new(scope: &MemoryScope, control: &'a OperationControl) -> Self {
        Self {
            text: FundedString::new_in(scope.clone(), Family::Parse),
            control,
        }
    }

    pub fn finish(self) -> FundedString {
        self.text
    }

    pub fn put(&mut self, text: &str) -> Result<(), DiagramError> {
        check(self.control, text.len().saturating_add(1))?;
        if self.text.len().saturating_add(text.len()) > 16 * 1024 {
            self.text.scope().record_failure(MemoryError::WorkLimited);
            return Err(DiagramError::WorkLimited);
        }
        self.text.try_push_str(text)?;
        Ok(())
    }

    fn number(&mut self, mut value: usize) -> Result<(), DiagramError> {
        let mut digits = [0u8; 20];
        let mut cursor = digits.len();
        loop {
            cursor -= 1;
            digits[cursor] = b'0' + (value % 10) as u8;
            value /= 10;
            if value == 0 {
                break;
            }
        }
        self.put(std::str::from_utf8(&digits[cursor..]).map_err(|_| DiagramError::InvalidInput)?)
    }

    pub fn id(&mut self, prefix: &str, index: usize) -> Result<(), DiagramError> {
        self.put(prefix)?;
        self.number(index)
    }

    pub fn color(&mut self, rgb: u32) -> Result<(), DiagramError> {
        if rgb > 0xffffff {
            return Err(DiagramError::InvalidGeometry);
        }
        let mut bytes = [b'#'; 7];
        for (index, byte) in bytes.iter_mut().enumerate().skip(1) {
            let shift = (6 - index) * 4;
            *byte = b"0123456789abcdef"[((rgb >> shift) & 15) as usize];
        }
        self.put(std::str::from_utf8(&bytes).map_err(|_| DiagramError::InvalidInput)?)
    }

    pub fn label(&mut self, label: &str) -> Result<(), DiagramError> {
        for value in label.chars() {
            check(self.control, 1)?;
            // This is only a grammar-transport encoding. Authored entities and
            // raw breaks were already decoded once into the typed label text.
            if matches!(
                value,
                '"' | '&'
                    | '#'
                    | '<'
                    | '>'
                    | '|'
                    | '`'
                    | '$'
                    | '['
                    | ']'
                    | '{'
                    | '}'
                    | '\\'
                    | ';'
                    | '%'
            ) {
                self.put("#")?;
                self.number(value as usize)?;
                self.put(";")?;
            } else {
                let mut bytes = [0; 4];
                self.put(value.encode_utf8(&mut bytes))?;
            }
        }
        Ok(())
    }
}

pub(crate) fn scope(control: &OperationControl) -> Result<MemoryScope, DiagramError> {
    let value = control.memory_scope().ok_or(DiagramError::InvalidFunding)?;
    if let Some(error) = value.failure() {
        return Err(DiagramError::Memory(error));
    }
    control.checkpoint().map_err(|error| {
        if error.reason == CancelReason::DeadlineExceeded {
            DiagramError::Deadline
        } else {
            DiagramError::Cancelled
        }
    })?;
    Ok(value.clone())
}

pub(crate) fn check(control: &OperationControl, work: usize) -> Result<(), DiagramError> {
    let funded = scope(control)?;
    funded.charge_work(work).map_err(DiagramError::Memory)?;
    control.checkpoint().map_err(|error| {
        if error.reason == CancelReason::DeadlineExceeded {
            DiagramError::Deadline
        } else {
            DiagramError::Cancelled
        }
    })
}

pub(crate) fn canonical(error: CanonicalError, control: &OperationControl) -> DiagramError {
    if let Some(error) = control.memory_scope().and_then(MemoryScope::failure) {
        return DiagramError::Memory(error);
    }
    let error = local_error(error);
    if matches!(error, DiagramError::Cancelled)
        && control
            .checkpoint()
            .is_err_and(|stop| stop.reason == CancelReason::DeadlineExceeded)
    {
        return DiagramError::Deadline;
    }
    error
}

pub(crate) fn label(
    raw: &str,
    policy: LayoutPolicy,
    provider: &dyn RichTextProvider,
    control: &OperationControl,
) -> Result<CompiledLabel, DiagramError> {
    let funded = scope(control)?;
    compile_label(
        raw,
        false,
        LabelPolicy {
            font: policy.font,
            wrap_width: policy.wrap_width,
            direction: TextDirection::Auto,
            line_gap: 0.0,
        },
        &funded,
        control,
        provider,
    )
    .map_err(|error| canonical(error, control))
}

pub(crate) fn text(
    scene: &mut SceneBuilder,
    provider: &dyn RichTextProvider,
    label: &CompiledLabel,
    color: Color,
    x: f64,
    y: f64,
) -> Result<(), DiagramError> {
    scene
        .begin_path(
            Paint {
                fill: Some(color),
                stroke: None,
                ..Paint::default()
            },
            Point::default(),
        )
        .map_err(local_error)?;
    label
        .outline_at(provider, x, y, scene)
        .map_err(local_error)?;
    scene.end_path().map_err(local_error)
}

pub(crate) fn shape(
    scene: &mut SceneBuilder,
    shape: Shape,
    rect: Rect,
    paint: Paint,
) -> Result<(), DiagramError> {
    scene
        .begin_path(paint, Point::default())
        .map_err(local_error)?;
    shapes::outline(shape, rect, scene).map_err(local_error)?;
    scene.end_path().map_err(local_error)
}

pub(crate) fn line(
    scene: &mut SceneBuilder,
    from: Point,
    to: Point,
    mut paint: Paint,
) -> Result<(), DiagramError> {
    paint.fill = None;
    scene
        .begin_path(paint, Point::default())
        .map_err(local_error)?;
    scene
        .push(PathCommand::move_to(from.x, from.y))
        .map_err(local_error)?;
    scene
        .push(PathCommand::line_to(to.x, to.y))
        .map_err(local_error)?;
    scene.end_path().map_err(local_error)
}

pub(crate) fn local_error(error: CanonicalError) -> DiagramError {
    match error {
        CanonicalError::Memory(error) => DiagramError::Memory(error),
        CanonicalError::Cancelled => DiagramError::Cancelled,
        CanonicalError::Limited => DiagramError::WorkLimited,
        CanonicalError::FontUnavailable => DiagramError::FontUnavailable,
        CanonicalError::UnsupportedElement
        | CanonicalError::UnsupportedProperty
        | CanonicalError::UnsupportedCss => DiagramError::Unsupported,
        CanonicalError::InvalidGeometry => DiagramError::InvalidGeometry,
        _ => DiagramError::Backend,
    }
}

/// Convert the supported authored text layer exactly once, before it enters
/// generated flowchart grammar. Raw breaks are semantic; entity-produced `<br>`
/// remains literal text. Callers choose whether other angle text is literal.
pub(crate) fn decode_label_text(
    raw: &str,
    scope: &MemoryScope,
    control: &OperationControl,
    reject_other_markup: bool,
) -> Result<FundedString, DiagramError> {
    let mut text = FundedString::new_in(scope.clone(), Family::Text);
    let mut candidate = FundedString::new_in(scope.clone(), Family::Text);
    let mut cursor = 0;
    let mut start = 0;
    while cursor < raw.len() {
        check(control, 1)?;
        let rest = &raw[cursor..];
        if rest.starts_with('<') {
            let after = &rest[1..];
            if let Some(end) = raw_label_break(after) {
                check(control, end + 1)?;
                decode_html(&raw[start..cursor], &mut text, control)?;
                text.try_push('\n')?;
                cursor += end + 1;
                start = cursor;
                continue;
            }
            if reject_other_markup
                && (after
                    .as_bytes()
                    .first()
                    .is_some_and(u8::is_ascii_alphabetic)
                    || after.starts_with(['/', '!']))
            {
                return Err(DiagramError::Unsupported);
            }
        } else if rest.starts_with('#') && (cursor == 0 || raw.as_bytes()[cursor - 1] != b'&') {
            // A fixed-size lookahead prevents repeated unterminated hash text
            // from turning the 16 KiB source scan into quadratic work.
            if let Some(end) = rest.as_bytes()[1..]
                .iter()
                .take(65)
                .position(|byte| *byte == b';')
            {
                let name = &rest[1..end + 1];
                if !name.is_empty()
                    && name
                        .bytes()
                        .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'+'))
                {
                    decode_html(&raw[start..cursor], &mut text, control)?;
                    let mut spelling = [0u8; 68];
                    spelling[0] = b'&';
                    let numeric = name.bytes().any(|byte| byte.is_ascii_digit())
                        && name
                            .bytes()
                            .all(|byte| byte.is_ascii_digit() || byte == b'+');
                    let prefix = if numeric {
                        spelling[1] = b'#';
                        2
                    } else {
                        1
                    };
                    spelling[prefix..prefix + name.len()].copy_from_slice(name.as_bytes());
                    spelling[prefix + name.len()] = b';';
                    let spelling = std::str::from_utf8(&spelling[..prefix + name.len() + 1])
                        .map_err(|_| DiagramError::InvalidInput)?;
                    candidate.clear();
                    decode_html(spelling, &mut candidate, control)?;
                    if candidate.as_str() == spelling {
                        text.try_push_str(&rest[..end + 2])?;
                    } else {
                        text.try_push_str(&candidate)?;
                    }
                    cursor += end + 2;
                    start = cursor;
                    continue;
                }
            }
        }
        cursor += rest
            .chars()
            .next()
            .ok_or(DiagramError::InvalidInput)?
            .len_utf8();
    }
    decode_html(&raw[start..], &mut text, control)?;
    Ok(text)
}

fn raw_label_break(after_angle: &str) -> Option<usize> {
    if !after_angle.get(..2)?.eq_ignore_ascii_case("br") {
        return None;
    }
    let rest = after_angle[2..].trim_start();
    let rest = rest.strip_prefix('/').map_or(rest, str::trim_start);
    let rest = rest.strip_prefix('>')?;
    Some(after_angle.len() - rest.len())
}

fn decode_html(
    raw: &str,
    text: &mut FundedString,
    control: &OperationControl,
) -> Result<(), DiagramError> {
    check(control, raw.len().saturating_add(1))?;
    // The shared visitor uses Borrowed for both authored spans and static
    // named-entity expansions. Preserve raw separators before visiting chunks
    // that cannot themselves contain them, then normalize every expansion.
    let mut start = 0;
    for (index, value) in raw.char_indices() {
        if matches!(value, '\r' | '\n' | '\t' | '\u{c}') {
            decode_html_chunk(&raw[start..index], text, control)?;
            text.try_push(value)?;
            start = index + value.len_utf8();
        }
    }
    decode_html_chunk(&raw[start..], text, control)
}

fn decoded_scalar(value: char, text: &mut FundedString) -> Result<(), DiagramError> {
    let value = if matches!(value, '\r' | '\n' | '\t' | '\u{c}') {
        ' '
    } else {
        value
    };
    if value < '\u{20}' || matches!(value, '\u{fffe}' | '\u{ffff}') {
        return Err(DiagramError::InvalidInput);
    }
    text.try_push(value)?;
    Ok(())
}

fn decode_html_chunk(
    raw: &str,
    text: &mut FundedString,
    control: &OperationControl,
) -> Result<(), DiagramError> {
    merman_core::entities::visit_decoded_html_entity_fragments_with_checkpoints(
        raw,
        || check(control, 64),
        |fragment| {
            match fragment {
                merman_core::entities::DecodedHtmlFragment::Borrowed(value) => {
                    for character in value.chars() {
                        decoded_scalar(character, text)?;
                    }
                }
                merman_core::entities::DecodedHtmlFragment::Scalar(value) => {
                    // Entity whitespace is ordinary text, distinct from raw
                    // line separators and supported raw <br> markup.
                    decoded_scalar(value, text)?;
                }
            }
            Ok(())
        },
    )
}

pub(crate) fn quoted_prefix(raw: &str) -> Result<(Option<&str>, &str), DiagramError> {
    let raw = raw.trim_start();
    if !raw.starts_with('"') {
        return Ok((None, raw));
    }
    let mut escaped = false;
    for (index, character) in raw.char_indices().skip(1) {
        if escaped {
            escaped = false;
        } else if character == '\\' {
            escaped = true;
        } else if character == '"' {
            let end = index + 1;
            return Ok((Some(&raw[..end]), raw[end..].trim_start()));
        }
    }
    Err(DiagramError::InvalidInput)
}

pub(crate) fn decode_label(raw: &str, scope: &MemoryScope) -> Result<FundedString, DiagramError> {
    if raw.len() > 16 * 1024 {
        return Err(DiagramError::WorkLimited);
    }
    scope
        .charge_work(raw.len().saturating_add(1))
        .map_err(DiagramError::Memory)?;
    let raw = raw.trim();
    let value = match quoted_prefix(raw)? {
        (Some(whole), "") => &whole[1..whole.len() - 1],
        _ => raw,
    };
    if raw.starts_with('"') {
        // The prepaid source scan found the closing quote. Fund decoding separately.
        scope
            .charge_work(value.len())
            .map_err(DiagramError::Memory)?;
    }
    let mut result = FundedString::new_in(scope.clone(), Family::Text);
    let mut chars = value.chars();
    while let Some(character) = chars.next() {
        let character = if character == '\\' {
            match chars.next() {
                Some('n') => '\n',
                Some('t') => '\t',
                Some('r') => '\n',
                Some('"') => '"',
                Some('\\') => '\\',
                Some(other) => {
                    result.try_push_str("\\").map_err(DiagramError::Memory)?;
                    other
                }
                None => '\\',
            }
        } else {
            character
        };
        if character == '\0' || (character.is_control() && !matches!(character, '\n' | '\t')) {
            return Err(DiagramError::InvalidInput);
        }
        let mut encoded = [0u8; 4];
        result
            .try_push_str(character.encode_utf8(&mut encoded))
            .map_err(DiagramError::Memory)?;
    }
    Ok(result)
}
