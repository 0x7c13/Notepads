// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE file in the project root for license information.
// ---------------------------------------------------------------------------------------------

use super::*;
use finl_unicode::categories::CharacterCategories;

#[derive(Clone, Copy, PartialEq)]
enum Kind {
    Text,
    Break,
    Delimiter,
}
struct Token {
    start: usize,
    end: usize,
    kind: Kind,
    marker: u8,
    count: usize,
    open: bool,
    close: bool,
    active: bool,
    strong: bool,
    emphasis: bool,
    depth: u8,
}

pub(super) fn parse(
    raw: &str,
    markdown: bool,
    base: FontPlan,
    scope: &MemoryScope,
    control: &OperationControl,
) -> Result<(FundedString, FundedVec<Span>), CanonicalError> {
    let mut tokens = FundedVec::new_in(scope.clone(), Family::Text);
    let bytes = raw.as_bytes();
    let mut cursor = 0;
    let mut text_start = 0;
    while cursor < bytes.len() {
        check(scope, control, 1)?;
        let begin = cursor;
        let mut special = None;
        if bytes[cursor] == b'\r' || bytes[cursor] == b'\n' {
            cursor += 1;
            if bytes[begin] == b'\r' && bytes.get(cursor) == Some(&b'\n') {
                cursor += 1;
            }
            special = Some(token(begin, cursor, Kind::Break));
        } else if bytes[cursor] == b'<' {
            if let Some(end) = find_byte(&raw[cursor..], b'>', scope, control)? {
                let end = cursor + end + 1;
                // Pay before the bounded standard-library body scans: Unicode
                // trim, br/slash tests, tag name and autolink classification.
                // The whole body is at most the admitted 16-KiB label; eight
                // byte passes conservatively cover every classification arm.
                check(scope, control, (end - cursor).saturating_mul(8))?;
                let body = raw[cursor + 1..end - 1].trim_end();
                if body.eq_ignore_ascii_case("br")
                    || body
                        .strip_suffix('/')
                        .is_some_and(|v| v.trim_end().eq_ignore_ascii_case("br"))
                {
                    cursor = end;
                    special = Some(token(begin, end, Kind::Break));
                } else if html_tag(body)
                    || body.starts_with("!--")
                    || markdown
                        && !body.chars().any(char::is_whitespace)
                        && (body.contains("://") || body.contains('@'))
                {
                    return Err(CanonicalError::UnsupportedElement);
                }
            }
        } else if markdown
            && bytes[cursor] == b'\\'
            && bytes.get(cursor + 1).is_some_and(u8::is_ascii_punctuation)
        {
            flush(&mut tokens, text_start, begin)?;
            cursor += 2;
            tokens.try_push(token(begin + 1, cursor, Kind::Text))?;
            text_start = cursor;
            continue;
        } else if markdown && matches!(bytes[cursor], b'*' | b'_') {
            let marker = bytes[cursor];
            cursor += 1;
            while bytes.get(cursor) == Some(&marker) {
                cursor += 1;
            }
            let before = raw[..begin].chars().next_back();
            let after = raw[cursor..].chars().next();
            let before_space = before.is_none_or(char::is_whitespace);
            let after_space = after.is_none_or(char::is_whitespace);
            let before_punct = before.is_some_and(punctuation);
            let after_punct = after.is_some_and(punctuation);
            let left = !after_space && (!after_punct || before_space || before_punct);
            let right = !before_space && (!before_punct || after_space || after_punct);
            let mut t = token(begin, cursor, Kind::Delimiter);
            t.marker = marker;
            t.count = cursor - begin;
            t.open = left && (marker != b'_' || !right || before_punct);
            t.close = right && (marker != b'_' || !left || after_punct);
            t.active = true;
            special = Some(t);
        } else if markdown && bytes[cursor] == b'`' {
            let mut n = 1;
            while bytes.get(cursor + n) == Some(&b'`') {
                n += 1;
            }
            if matching_run(&raw[cursor + n..], b'`', n, scope, control)? {
                return Err(CanonicalError::UnsupportedElement);
            }
            cursor += n;
        } else if (markdown
            && bytes[cursor] == b'['
            && complete_link(&raw[cursor..], scope, control)?)
            || (raw[cursor..].starts_with("$$") && raw[cursor + 2..].contains("$$"))
        {
            return Err(CanonicalError::UnsupportedElement);
        }
        if let Some(t) = special {
            flush(&mut tokens, text_start, begin)?;
            tokens.try_push(t)?;
            text_start = cursor;
        } else if cursor == begin {
            cursor += raw[cursor..]
                .chars()
                .next()
                .ok_or(CanonicalError::MalformedXml)?
                .len_utf8();
        }
    }
    flush(&mut tokens, text_start, raw.len())?;
    if markdown {
        emphasis(&mut tokens, scope, control)?;
    }
    let mut text = FundedString::new_in(scope.clone(), Family::Text);
    let mut spans = FundedVec::new_in(scope.clone(), Family::Text);
    for t in &tokens {
        check(scope, control, 1)?;
        let mut font = base;
        if t.strong {
            font.weight = 700;
        }
        if t.emphasis {
            font.italic = true;
        }
        let start = text.len();
        match t.kind {
            Kind::Break => text.try_push('\n')?,
            Kind::Delimiter => {
                for _ in 0..t.count {
                    text.try_push(t.marker as char)?;
                }
            }
            Kind::Text => decode(&raw[t.start..t.end], &mut text, scope, control)?,
        }
        if text.len() > start {
            if spans.len() >= 4096 {
                return Err(CanonicalError::Limited);
            }
            if let Some(last) = spans
                .last_mut()
                .filter(|s: &&mut Span| s.range.end == start && same_font(s.font, font))
            {
                last.range.end = text.len();
            } else {
                spans.try_push(Span {
                    range: start..text.len(),
                    font,
                })?;
            }
        }
    }
    Ok((text, spans))
}

// Prepay bounded search windows before touching their bytes. A missing closing
// delimiter must consume the suffix search work just like a found delimiter.
fn find_byte(
    raw: &str,
    needle: u8,
    scope: &MemoryScope,
    control: &OperationControl,
) -> Result<Option<usize>, CanonicalError> {
    for (index, window) in raw.as_bytes().chunks(256).enumerate() {
        check(scope, control, window.len())?;
        if let Some(offset) = window.iter().position(|byte| *byte == needle) {
            check(scope, control, 0)?;
            return Ok(Some(index * 256 + offset));
        }
    }
    check(scope, control, 0)?;
    Ok(None)
}

fn token(start: usize, end: usize, kind: Kind) -> Token {
    Token {
        start,
        end,
        kind,
        marker: 0,
        count: 0,
        open: false,
        close: false,
        active: false,
        strong: false,
        emphasis: false,
        depth: 0,
    }
}
fn flush(tokens: &mut FundedVec<Token>, start: usize, end: usize) -> Result<(), CanonicalError> {
    if start < end {
        tokens.try_push(token(start, end, Kind::Text))?;
    }
    Ok(())
}
fn punctuation(c: char) -> bool {
    c.is_punctuation() || c.is_symbol()
}

fn emphasis(
    tokens: &mut FundedVec<Token>,
    scope: &MemoryScope,
    control: &OperationControl,
) -> Result<(), CanonicalError> {
    for closer in 0..tokens.len() {
        if tokens[closer].kind != Kind::Delimiter || !tokens[closer].close {
            continue;
        }
        while tokens[closer].count > 0 {
            let mut opener = None;
            for at in (0..closer).rev() {
                check(scope, control, 1)?;
                let a = &tokens[at];
                let b = &tokens[closer];
                if !a.active || !a.open || a.count == 0 || a.marker != b.marker {
                    continue;
                }
                if (a.close || b.open)
                    && (a.count + b.count).is_multiple_of(3)
                    && (!a.count.is_multiple_of(3) || !b.count.is_multiple_of(3))
                {
                    continue;
                }
                opener = Some(at);
                break;
            }
            let Some(opener) = opener else {
                break;
            };
            let count = if tokens[opener].count >= 2 && tokens[closer].count >= 2 {
                2
            } else {
                1
            };
            tokens[opener].count -= count;
            tokens[closer].count -= count;
            check(scope, control, closer - opener)?;
            for item in &mut tokens[opener + 1..closer] {
                item.depth += 1;
                if item.depth > 8 {
                    return Err(CanonicalError::Limited);
                }
                if count == 2 {
                    item.strong = true;
                } else {
                    item.emphasis = true;
                }
                if item.kind == Kind::Delimiter {
                    item.active = false;
                }
            }
        }
    }
    Ok(())
}

fn matching_run(
    raw: &str,
    marker: u8,
    count: usize,
    scope: &MemoryScope,
    control: &OperationControl,
) -> Result<bool, CanonicalError> {
    let mut at = 0;
    let bytes = raw.as_bytes();
    while at < bytes.len() {
        check(scope, control, 1)?;
        if bytes[at] != marker {
            at += 1;
            continue;
        }
        let start = at;
        while bytes.get(at) == Some(&marker) {
            at += 1;
        }
        if at - start == count {
            return Ok(true);
        }
    }
    Ok(false)
}
fn complete_link(
    raw: &str,
    scope: &MemoryScope,
    control: &OperationControl,
) -> Result<bool, CanonicalError> {
    let Some(close) = balanced_link(raw, b'[', b']', scope, control)? else {
        return Ok(false);
    };
    let tail = &raw[close + 1..];
    if tail.starts_with('(') {
        return balanced_link(tail, b'(', b')', scope, control).map(|v| v.is_some());
    }
    if tail.starts_with('[') {
        return balanced_link(tail, b'[', b']', scope, control).map(|v| v.is_some());
    }
    Ok(false)
}
fn balanced_link(
    raw: &str,
    open: u8,
    close: u8,
    scope: &MemoryScope,
    control: &OperationControl,
) -> Result<Option<usize>, CanonicalError> {
    let bytes = raw.as_bytes();
    let mut depth = 0usize;
    let mut cursor = 0;
    while cursor < bytes.len() {
        check(scope, control, 1)?;
        if bytes[cursor] == b'\\' && bytes.get(cursor + 1).is_some_and(u8::is_ascii_punctuation) {
            cursor += 2;
            continue;
        }
        if bytes[cursor] == open {
            depth += 1;
        } else if bytes[cursor] == close {
            depth = depth.saturating_sub(1);
            if depth == 0 {
                return Ok(Some(cursor));
            }
        }
        cursor += 1;
    }
    Ok(None)
}
fn html_tag(body: &str) -> bool {
    let body = body.strip_prefix('/').unwrap_or(body);
    if !body.as_bytes().first().is_some_and(u8::is_ascii_alphabetic) {
        return false;
    }
    let n = body
        .bytes()
        .take_while(|b| b.is_ascii_alphanumeric() || matches!(b, b'-' | b':'))
        .count();
    let tail = &body[n..];
    tail.is_empty() || tail.starts_with(char::is_whitespace) || tail == "/"
}

fn decode(
    raw: &str,
    out: &mut FundedString,
    scope: &MemoryScope,
    control: &OperationControl,
) -> Result<(), CanonicalError> {
    // Hash references are recognized before invoking the same allocation-free
    // pinned WHATWG entity visitor as Core. Decoded text never re-enters markup.
    let mut start = 0;
    let mut at = 0;
    let mut decoded = FundedString::new_in(scope.clone(), Family::Text);
    while at < raw.len() {
        check(scope, control, 1)?;
        if raw.as_bytes()[at] == b'#' && (at == 0 || raw.as_bytes()[at - 1] != b'&') {
            let rest = &raw[at + 1..];
            // A reference beyond byte 64 cannot be accepted. Do not search
            // the remaining label for a semicolon that will be discarded.
            let candidate = &rest.as_bytes()[..rest.len().min(65)];
            check(scope, control, candidate.len())?;
            if let Some(end) = candidate.iter().position(|byte| *byte == b';') {
                let name = &rest[..end];
                if !name.is_empty() && name.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'+')
                {
                    html(&raw[start..at], out, scope, control)?;
                    let mut candidate = [0u8; 68];
                    candidate[0] = b'&';
                    let numeric = name.bytes().all(|b| b.is_ascii_digit() || b == b'+');
                    let prefix = if numeric {
                        candidate[1] = b'#';
                        2
                    } else {
                        1
                    };
                    candidate[prefix..prefix + name.len()].copy_from_slice(name.as_bytes());
                    candidate[prefix + name.len()] = b';';
                    let value = std::str::from_utf8(&candidate[..prefix + name.len() + 1])
                        .map_err(|_| CanonicalError::MalformedXml)?;
                    decoded.clear();
                    html(value, &mut decoded, scope, control)?;
                    if decoded.as_str() == value {
                        // Unknown hash reference retains its authored spelling.
                        out.try_push_str(&raw[at..at + end + 2])?;
                    } else {
                        for c in decoded.as_str().chars() {
                            append(out, c)?;
                        }
                    }
                    at += end + 2;
                    start = at;
                    continue;
                }
            }
        }
        at += raw[at..]
            .chars()
            .next()
            .ok_or(CanonicalError::MalformedXml)?
            .len_utf8();
    }
    html(&raw[start..], out, scope, control)
}
fn html(
    raw: &str,
    out: &mut FundedString,
    scope: &MemoryScope,
    control: &OperationControl,
) -> Result<(), CanonicalError> {
    let mut start = 0;
    let mut at = 0;
    while let Some(offset) = raw[at..].find("&#") {
        let begin = at + offset;
        let mut end = begin + 2;
        let hex = matches!(raw.as_bytes().get(end), Some(b'x' | b'X'));
        if hex {
            end += 1;
        }
        let digits = end;
        while raw.as_bytes().get(end).is_some_and(|b| {
            if hex {
                b.is_ascii_hexdigit()
            } else {
                b.is_ascii_digit()
            }
        }) {
            end += 1;
        }
        if end == digits {
            at = end;
            continue;
        }
        let value = u32::from_str_radix(&raw[digits..end], if hex { 16 } else { 10 })
            .ok()
            .and_then(char::from_u32);
        if raw.as_bytes().get(end) == Some(&b';') {
            end += 1;
        }
        if value.is_none_or(|c| c == '\0' || matches!(c, '\u{fffe}' | '\u{ffff}')) {
            html_chunk(&raw[start..begin], out, scope, control)?;
            out.try_push_str(&raw[begin..end])?;
            start = end;
        }
        check(scope, control, end - begin)?;
        at = end;
    }
    html_chunk(&raw[start..], out, scope, control)
}

fn html_chunk(
    raw: &str,
    out: &mut FundedString,
    scope: &MemoryScope,
    control: &OperationControl,
) -> Result<(), CanonicalError> {
    merman_core::entities::visit_decoded_html_entity_fragments_with_checkpoints(
        raw,
        || check(scope, control, 64),
        |fragment| {
            match fragment {
                merman_core::entities::DecodedHtmlFragment::Borrowed(value) => {
                    for c in value.chars() {
                        append(out, c)?;
                    }
                }
                merman_core::entities::DecodedHtmlFragment::Scalar(c) => append(out, c)?,
            }
            Ok(())
        },
    )
}
fn append(out: &mut FundedString, c: char) -> Result<(), CanonicalError> {
    let c = if matches!(c, '\r' | '\n' | '\t' | '\u{c}') {
        ' '
    } else {
        c
    };
    if c == ' ' && out.as_str().ends_with(' ') {
        return Ok(());
    }
    if c < '\u{20}' || matches!(c, '\u{fffe}' | '\u{ffff}') {
        return Err(CanonicalError::MalformedXml);
    }
    out.try_push(c).map_err(Into::into)
}
