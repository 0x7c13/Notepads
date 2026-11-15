// Notepads Markdown modifications Copyright (c) 2026, Jiaqi (0x7c13) Liu.
// Original third-party licenses remain applicable; see the crate COPYING file.

use std::borrow::Cow;
use std::ptr;
use std::str;

use crate::ctype::{ispunct, isspace, isspace_char};
use crate::entity;
use crate::matchers::cr_or_lf_matcher;
use crate::parser::AutolinkType;
#[cfg(feature = "phoenix_heex")]
use crate::scanners;

#[derive(PartialEq, Eq)]
pub enum Case {
    Preserve,
    Fold,
}

pub fn unescape(s: &mut String) {
    let mut host = crate::native::LegacyHost;
    let mut ctx = crate::native::Context::new(&mut host, s.len());
    ctx.capture = false;
    *s = match unescape_with_host(s, &mut ctx, None, 1) {
        Ok(value) => value,
        Err(error) => match error {},
    };
}

pub(crate) fn clean_autolink_with_host<H: crate::native::Host>(
    raw: &str,
    kind: AutolinkType,
    ctx: &mut crate::native::Context<'_, H>,
    payload: Option<(crate::native::PayloadId, crate::native::RSpan)>,
) -> Result<String, H::Error> {
    use crate::native::{AcceptedFact, DSpan, RSpan, StageKind};
    let mut start = 0;
    let mut end = raw.len();
    while start < end && isspace(raw.as_bytes()[start]) {
        ctx.scan_step()?;
        start += 1;
    }
    while end > start && isspace(raw.as_bytes()[end - 1]) {
        ctx.scan_step()?;
        end -= 1;
    }
    if let Some((payload, _)) = payload {
        ctx.fact(AcceptedFact::Stage {
            payload,
            stage: 1,
            kind: StageKind::Trim,
            input: DSpan {
                payload,
                stage: 0,
                start,
                end,
            },
            output_start: 0,
            output_end: end - start,
            value: &raw[start..end],
            hidden_runs: &[],
        })?;
    }
    let decoded =
        entity::unescape_html_with_host(&raw[start..end], ctx, payload.map(|(id, _)| id), 2)?;
    let output = if kind == AutolinkType::Email && !decoded.is_empty() {
        let mut output = ctx.decoded_string("mailto:")?;
        ctx.append(&mut output, &decoded)?;
        ctx.discard_string(decoded)?;
        if let Some((payload, raw)) = payload {
            ctx.fact(AcceptedFact::PayloadPrefix {
                payload,
                kind: crate::native::GeneratedPrefix::Mailto,
                at: RSpan {
                    buffer: raw.buffer,
                    start: raw.start + start,
                    end: raw.start + start,
                },
                value: "mailto:",
            })?;
        }
        output
    } else {
        decoded
    };
    if let Some((payload, _)) = payload {
        ctx.fact(AcceptedFact::PayloadEnd {
            payload,
            value: &output,
        })?;
    }
    Ok(output)
}

#[cfg(test)]
pub fn normalize_code(v: &str) -> Cow<'_, str> {
    let mut host = crate::native::LegacyHost;
    let mut ctx = crate::native::Context::new(&mut host, v.len());
    ctx.capture = false;
    match normalize_code_with_host(v, &mut ctx, None) {
        Ok(value) => value.into(),
        Err(error) => match error {},
    }
}

pub(crate) fn normalize_code_with_host<H: crate::native::Host>(
    v: &str,
    ctx: &mut crate::native::Context<'_, H>,
    source: Option<(crate::native::NativeNodeId, crate::native::RSpan)>,
) -> Result<String, H::Error> {
    use crate::native::{AcceptedFact, DecodeKind, RSpan};
    let bytes = v.as_bytes();
    let mut output = String::new();
    ctx.grow_string(&mut output, v.len())?;
    let mut offset = 0;
    let mut index = 0;
    let mut contains_nonspace = false;
    let mut first_unit_end = 1;
    let mut last_unit_start = v.len().saturating_sub(1);
    while index < bytes.len() {
        ctx.scan_step()?;
        let start = index;
        match bytes[index] {
            b'\r' | b'\n' => {
                index += 1;
                if bytes[start] == b'\r' && bytes.get(index) == Some(&b'\n') {
                    index += 1;
                }
                ctx.decode_append(&mut output, &v[offset..start])?;
                ctx.decode_append(&mut output, " ")?;
                if let Some((node, raw)) = source {
                    ctx.fact(AcceptedFact::DecodeAtom {
                        node,
                        kind: DecodeKind::CodeLineEndingToSpace,
                        source: RSpan {
                            buffer: raw.buffer,
                            start: raw.start + start,
                            end: raw.start + index,
                        },
                        value: " ",
                    })?;
                }
                offset = index;
                if start == 0 {
                    first_unit_end = index;
                }
                if index == bytes.len() {
                    last_unit_start = start;
                }
            }
            b' ' => index += 1,
            _ => {
                contains_nonspace = true;
                index += 1;
            }
        }
    }
    ctx.decode_append(&mut output, &v[offset..])?;
    if contains_nonspace && output.starts_with(' ') && output.ends_with(' ') {
        if let Some((node, raw)) = source {
            ctx.fact(AcceptedFact::DecodeAtom {
                node,
                kind: DecodeKind::CodeTrim,
                source: RSpan {
                    buffer: raw.buffer,
                    start: raw.start,
                    end: raw.start + first_unit_end,
                },
                value: "",
            })?;
            ctx.fact(AcceptedFact::DecodeAtom {
                node,
                kind: DecodeKind::CodeTrim,
                source: RSpan {
                    buffer: raw.buffer,
                    start: raw.start + last_unit_start,
                    end: raw.end,
                },
                value: "",
            })?;
        }
        let trimmed = ctx.decoded_string(&output[1..output.len() - 1])?;
        ctx.discard_string(output)?;
        output = trimmed;
    }
    ctx.flush_scan()?;
    Ok(output)
}

pub fn remove_trailing_blank_lines_slice(line: &str) -> &str {
    &line[..remove_trailing_blank_lines_ix(line)]
}

fn remove_trailing_blank_lines_ix(line: &str) -> usize {
    let mut host = crate::native::LegacyHost;
    let mut ctx = crate::native::Context::new(&mut host, line.len());
    ctx.capture = false;
    match remove_trailing_blank_lines_ix_with_host(line, &mut ctx) {
        Ok(value) => value,
        Err(error) => match error {},
    }
}

pub(crate) fn remove_trailing_blank_lines_ix_with_host<H: crate::native::Host>(
    line: &str,
    ctx: &mut crate::native::Context<'_, H>,
) -> Result<usize, H::Error> {
    let result = 'native: {
        let line_bytes = line.as_bytes();
        if line.is_empty() {
            break 'native 0;
        }
        let mut i = line.len() - 1;
        loop {
            ctx.scan_step()?;
            let c = line_bytes[i];

            if c != b' ' && c != b'\t' && !is_line_end_char(c) {
                break;
            }

            if i == 0 {
                break 'native 0;
            }

            i -= 1;
        }

        for (i, c) in line_bytes.iter().enumerate().take(line.len()).skip(i) {
            ctx.scan_step()?;
            if !is_line_end_char(*c) {
                continue;
            }

            break 'native i;
        }

        line.len()
    };
    ctx.flush_scan()?;
    Ok(result)
}

pub fn is_line_end_char(ch: u8) -> bool {
    matches!(ch, 10 | 13)
}

pub fn is_space_or_tab(ch: u8) -> bool {
    matches!(ch, 9 | 32)
}

/// Chop any trailing sequence of `#` characters from an ATX heading line.
///
/// Returns the possibly-chopped line and a boolean indicating whether
/// trailing hashes were removed (i.e. the heading had a closing sequence).
pub(crate) fn chop_trailing_hashes_with_host<'i, H: crate::native::Host>(
    line: &'i str,
    ctx: &mut crate::native::Context<'_, H>,
) -> Result<(&'i str, bool), H::Error> {
    let line = rtrim_slice_with_host(line, ctx)?;
    if line.is_empty() {
        return Ok((line, false));
    }
    let orig_n = line.len() - 1;
    let mut n = orig_n;
    let bytes = line.as_bytes();
    while bytes[n] == b'#' {
        ctx.scan_step()?;
        if n == 0 {
            ctx.flush_scan()?;
            return Ok((line, false));
        }
        n -= 1;
    }
    let result = if n != orig_n && is_space_or_tab(bytes[n]) {
        (rtrim_slice_with_host(&line[..n], ctx)?, true)
    } else {
        (line, false)
    };
    ctx.flush_scan()?;
    Ok(result)
}

pub fn rtrim(line: &mut String) -> usize {
    let mut host = crate::native::LegacyHost;
    let mut ctx = crate::native::Context::new(&mut host, line.len());
    ctx.capture = false;
    match rtrim_with_host(line, &mut ctx) {
        Ok(value) => value,
        Err(error) => match error {},
    }
}

pub(crate) fn rtrim_with_host<H: crate::native::Host>(
    line: &mut String,
    ctx: &mut crate::native::Context<'_, H>,
) -> Result<usize, H::Error> {
    let mut new_len = line.len();
    while new_len != 0 && isspace(line.as_bytes()[new_len - 1]) {
        ctx.scan_step()?;
        new_len -= 1;
    }
    let spaces = line.len() - new_len;
    line.truncate(new_len);
    ctx.flush_scan()?;
    Ok(spaces)
}

pub(crate) fn rtrim_slice_with_host<'i, H: crate::native::Host>(
    line: &'i str,
    ctx: &mut crate::native::Context<'_, H>,
) -> Result<&'i str, H::Error> {
    let mut end = line.len();
    for (at, ch) in line.char_indices().rev() {
        ctx.scan_step()?;
        if !isspace_char(ch) {
            break;
        }
        end = at;
    }
    ctx.flush_scan()?;
    Ok(&line[..end])
}

pub fn ltrim(line: &mut String) -> usize {
    let spaces = ltrim_count(line);
    remove_from_start(line, spaces);
    spaces
}

#[inline]
pub fn ltrim_count(line: &str) -> usize {
    line.as_bytes().iter().take_while(|&&c| isspace(c)).count()
}

pub fn remove_from_start(s: &mut String, n: usize) {
    if n == 0 {
        return;
    }

    if !s.is_char_boundary(n) {
        panic!("remove_from_start results in non UTF-8 string");
    }

    // SAFETY: we've asserted s[n] is a valid UTF-8 boundary, and we truncate
    // any duplicated continuation characters.
    let bytes = unsafe { s.as_bytes_mut() };
    shift_buf_left(bytes, n);
    let new_len = bytes.len() - n;
    // HACK: see shift_buf_left.
    bytes[new_len] = b'\0';
    s.truncate(new_len);
}

pub fn trim(line: &mut String) {
    ltrim(line);
    rtrim(line);
}

pub fn ltrim_slice(i: &str) -> &str {
    // Compared to upstream, this additionally trims U+000C FORM FEED.
    i.trim_start_matches(isspace_char)
}

pub fn rtrim_slice(i: &str) -> &str {
    i.trim_end_matches(isspace_char)
}

pub fn trim_slice(i: &str) -> &str {
    rtrim_slice(ltrim_slice(i))
}

pub fn trim_cow(s: &mut Cow<str>) {
    match s {
        Cow::Borrowed(str) => *str = trim_slice(str),
        Cow::Owned(string) => trim(string),
    }
}

// HACK: Using this function safely on a buffer obtained from
// String::as_bytes_mut() requires care when truncating it.
//
// Say the string ends in a multibyte character, i.e. the last byte is a UTF-8
// continuation byte. That byte will be repeated at the end of the buffer when
// it's shifted left by one at a time; therefore the truncation point won't be
// a valid UTF-8 character boundary, and String::truncate will panic. In such
// cases, set the byte immediately after the retained portion to b'\0' (or any
// non-continuation byte!).
fn shift_buf_left(buf: &mut [u8], n: usize) {
    if n == 0 {
        return;
    }
    assert!(n <= buf.len());
    let keep = buf.len() - n;
    // SAFETY: we can copy `keep` bytes from `dst+n` to `dst`, as the full size
    // of the `dst` buffer is `keep+n`.
    unsafe {
        let dst = buf.as_mut_ptr();
        let src = dst.add(n);
        ptr::copy(src, dst, keep);
    }
}

pub fn clean_url(url: &str) -> Cow<'static, str> {
    let mut host = crate::native::LegacyHost;
    let mut ctx = crate::native::Context::new(&mut host, url.len());
    ctx.capture = false;
    match clean_payload_with_host(url, crate::native::PayloadKind::Destination, &mut ctx, None) {
        Ok(value) => value.into(),
        Err(error) => match error {},
    }
}

pub fn clean_title(title: &str) -> Cow<'static, str> {
    let mut host = crate::native::LegacyHost;
    let mut ctx = crate::native::Context::new(&mut host, title.len());
    ctx.capture = false;
    match clean_payload_with_host(title, crate::native::PayloadKind::Title, &mut ctx, None) {
        Ok(value) => value.into(),
        Err(error) => match error {},
    }
}

pub(crate) fn clean_payload_with_host<H: crate::native::Host>(
    raw: &str,
    role: crate::native::PayloadKind,
    ctx: &mut crate::native::Context<'_, H>,
    payload: Option<crate::native::PayloadId>,
) -> Result<String, H::Error> {
    clean_payload_impl(raw, role, ctx, payload, PayloadContext::Ordinary)
}

pub(crate) fn clean_reference_title(title: &str) -> Cow<'static, str> {
    let mut host = crate::native::LegacyHost;
    let mut ctx = crate::native::Context::new(&mut host, title.len());
    ctx.capture = false;
    match clean_reference_title_with_host(title, &mut ctx, None) {
        Ok(value) => value.into(),
        Err(error) => match error {},
    }
}

pub(crate) fn clean_reference_title_with_host<H: crate::native::Host>(
    raw: &str,
    ctx: &mut crate::native::Context<'_, H>,
    payload: Option<crate::native::PayloadId>,
) -> Result<String, H::Error> {
    clean_payload_impl(
        raw,
        crate::native::PayloadKind::Title,
        ctx,
        payload,
        PayloadContext::AcceptedReferenceTitle,
    )
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum PayloadContext {
    Ordinary,
    AcceptedReferenceTitle,
}

fn clean_payload_impl<H: crate::native::Host>(
    raw: &str,
    role: crate::native::PayloadKind,
    ctx: &mut crate::native::Context<'_, H>,
    payload: Option<crate::native::PayloadId>,
    context: PayloadContext,
) -> Result<String, H::Error> {
    use crate::native::{AcceptedFact, DSpan, PayloadKind, StageKind};
    if role == PayloadKind::FenceInfo {
        // The accepted fence-info pipeline is Entity -> Trim -> Escape.
        let entity = entity::unescape_html_with_host(raw, ctx, payload, 1)?;
        let mut start = 0;
        let mut end = entity.len();
        while start < end && isspace(entity.as_bytes()[start]) {
            ctx.scan_step()?;
            start += 1;
        }
        while end > start && isspace(entity.as_bytes()[end - 1]) {
            ctx.scan_step()?;
            end -= 1;
        }
        if let Some(payload) = payload {
            ctx.fact(AcceptedFact::Stage {
                payload,
                stage: 2,
                kind: StageKind::Trim,
                input: DSpan {
                    payload,
                    stage: 1,
                    start,
                    end,
                },
                output_start: 0,
                output_end: end - start,
                value: &entity[start..end],
                hidden_runs: &[],
            })?;
        }
        let output = unescape_with_host(&entity[start..end], ctx, payload, 3)?;
        ctx.discard_string(entity)?;
        if let Some(payload) = payload {
            ctx.fact(AcceptedFact::PayloadEnd {
                payload,
                value: &output,
            })?;
        }
        return Ok(output);
    }
    let mut start = 0;
    let mut end = raw.len();
    if role == PayloadKind::Title {
        let bytes = raw.as_bytes();
        if end >= 2
            && matches!(
                (bytes[0], bytes[end - 1]),
                (b'\'', b'\'') | (b'"', b'"') | (b'(', b')')
            )
        {
            start = 1;
            end -= 1;
        }
    } else {
        while start < end && isspace(raw.as_bytes()[start]) {
            ctx.scan_step()?;
            start += 1;
        }
        while end > start && isspace(raw.as_bytes()[end - 1]) {
            ctx.scan_step()?;
            end -= 1;
        }
    }
    if let Some(payload) = payload {
        ctx.fact(AcceptedFact::Stage {
            payload,
            stage: 1,
            kind: StageKind::Trim,
            input: DSpan {
                payload,
                stage: 0,
                start,
                end,
            },
            output_start: 0,
            output_end: end - start,
            value: &raw[start..end],
            hidden_runs: &[],
        })?;
    }
    let reference = context == PayloadContext::AcceptedReferenceTitle;
    let body = if reference {
        reference_title_body(&raw[start..end], ctx, payload)?
    } else {
        Cow::Borrowed(&raw[start..end])
    };
    let entity =
        entity::unescape_html_with_host(&body, ctx, payload, if reference { 3 } else { 2 })?;
    if let Cow::Owned(value) = body {
        ctx.discard_string(value)?;
    }
    let output = unescape_with_host(&entity, ctx, payload, if reference { 4 } else { 3 })?;
    ctx.discard_string(entity)?;
    if let Some(payload) = payload {
        ctx.fact(AcceptedFact::PayloadEnd {
            payload,
            value: &output,
        })?;
    }
    Ok(output)
}

/// The accepting operation still holds the actual pre-Entity title bytes.
/// Numeric descriptors alone are not permission to hide source text.
fn validate_title_runs<H: crate::native::Host>(
    body: &str,
    payload: crate::native::PayloadId,
    runs: &[crate::native::DSpan],
    ctx: &mut crate::native::Context<'_, H>,
) -> Result<usize, H::Error> {
    use crate::native::NativeViolation;
    let bytes = body.as_bytes();
    let mut previous = 0;
    let mut removed = 0usize;
    for run in runs {
        ctx.event()?;
        if run.payload != payload
            || run.stage != 1
            || run.start == 0
            || run.start < previous
            || run.start >= run.end
            || run.end > bytes.len()
        {
            return Err(ctx.host.reject(NativeViolation::InvalidProvenance));
        }
        ctx.scan_step()?;
        if bytes[run.start - 1] != b'\n' {
            return Err(ctx.host.reject(NativeViolation::InvalidProvenance));
        }
        for byte in &bytes[run.start..run.end] {
            ctx.scan_step()?;
            if !matches!(byte, b' ' | b'\t') {
                return Err(ctx.host.reject(NativeViolation::InvalidProvenance));
            }
        }
        if let Some(byte) = bytes.get(run.end) {
            ctx.scan_step()?;
            if matches!(byte, b' ' | b'\t') {
                return Err(ctx.host.reject(NativeViolation::InvalidProvenance));
            }
        }
        removed = removed
            .checked_add(run.end - run.start)
            .ok_or_else(|| ctx.host.reject(NativeViolation::ArithmeticOverflow))?;
        previous = run.end;
    }
    Ok(removed)
}

fn reference_title_body<'a, H: crate::native::Host>(
    body: &'a str,
    ctx: &mut crate::native::Context<'_, H>,
    payload: Option<crate::native::PayloadId>,
) -> Result<Cow<'a, str>, H::Error> {
    use crate::native::{AcceptedFact, DSpan, NativeViolation, StageKind};
    let bytes = body.as_bytes();
    let mut at = 0;
    let mut copied = 0;
    let mut output: Option<String> = None;
    let mut runs = Vec::new();
    while at < bytes.len() {
        ctx.scan_step()?;
        let newline = bytes[at] == b'\n';
        at += 1;
        if !newline {
            continue;
        }
        let start = at;
        while let Some(byte) = bytes.get(at) {
            ctx.scan_step()?;
            if !matches!(byte, b' ' | b'\t') {
                break;
            }
            at += 1;
        }
        if start == at {
            continue;
        }
        let text = output.get_or_insert_with(String::new);
        ctx.decode_append(text, &body[copied..start])?;
        copied = at;
        if let Some(payload) = payload {
            ctx.push(
                &mut runs,
                DSpan {
                    payload,
                    stage: 1,
                    start,
                    end: at,
                },
            )?;
        }
    }
    let output = match output {
        Some(mut text) => {
            ctx.decode_append(&mut text, &body[copied..])?;
            Cow::Owned(text)
        }
        None => Cow::Borrowed(body),
    };
    if let Some(payload) = payload {
        let removed = validate_title_runs(body, payload, &runs, ctx)?;
        if body.len().checked_sub(removed) != Some(output.len()) {
            return Err(ctx.host.reject(NativeViolation::InvalidProvenance));
        }
        ctx.fact(AcceptedFact::Stage {
            payload,
            stage: 2,
            kind: StageKind::ReferenceTitleIndent,
            input: DSpan {
                payload,
                stage: 1,
                start: 0,
                end: body.len(),
            },
            output_start: 0,
            output_end: output.len(),
            value: &output,
            hidden_runs: &runs,
        })?;
    }
    ctx.discard_vec(runs)?;
    ctx.flush_scan()?;
    Ok(output)
}

#[cfg(test)]
mod reference_title_tests {
    use super::*;
    use crate::native::tests::MemoryHost;
    use crate::native::{Context, DSpan, PayloadId};

    #[test]
    fn title_run_acceptance_rejects_wrong_bytes_and_nonmaximal_descriptors() {
        let id = PayloadId::from_raw(1).unwrap();
        for (body, start, end) in [
            ("a\n x", 2, 4),
            ("a\n  x", 2, 3),
            ("a  x", 1, 3),
            ("a\n  x", 3, 4),
            ("a\n\u{a0}x", 2, 4),
        ] {
            let mut host = MemoryHost::new();
            let mut ctx = Context::new(&mut host, body.len());
            assert!(
                validate_title_runs(
                    body,
                    id,
                    &[DSpan {
                        payload: id,
                        stage: 1,
                        start,
                        end
                    }],
                    &mut ctx
                )
                .is_err()
            );
        }
    }

    #[test]
    fn title_run_acceptance_checks_old_stage_identity_order_and_zero_copy_path() {
        let id = PayloadId::from_raw(1).unwrap();
        let run = DSpan {
            payload: id,
            stage: 1,
            start: 2,
            end: 4,
        };
        for runs in [
            vec![DSpan { stage: 2, ..run }],
            vec![DSpan {
                payload: PayloadId::from_raw(2).unwrap(),
                ..run
            }],
            vec![run, run],
        ] {
            let mut host = MemoryHost::new();
            let mut ctx = Context::new(&mut host, 10);
            assert!(validate_title_runs("a\n  b\n\t c ", id, &runs, &mut ctx).is_err());
        }
        let mut host = MemoryHost::new();
        let mut ctx = Context::new(&mut host, 10);
        assert_eq!(
            validate_title_runs(
                "a\n  b\n\t c ",
                id,
                &[
                    run,
                    DSpan {
                        payload: id,
                        stage: 1,
                        start: 6,
                        end: 8
                    }
                ],
                &mut ctx
            )
            .unwrap(),
            4
        );
        let value = reference_title_body(" a\nb  ", &mut ctx, None).unwrap();
        assert!(matches!(value, Cow::Borrowed(" a\nb  ")));
    }
}

pub(crate) fn unescape_with_host<H: crate::native::Host>(
    raw: &str,
    ctx: &mut crate::native::Context<'_, H>,
    payload: Option<crate::native::PayloadId>,
    stage: u32,
) -> Result<String, H::Error> {
    use crate::native::{AcceptedFact, DSpan, StageKind};
    let bytes = raw.as_bytes();
    let mut output = String::new();
    ctx.grow_string(&mut output, raw.len())?;
    let mut index = 0;
    let mut copied = 0;
    while index < bytes.len() {
        ctx.scan_step()?;
        if bytes[index] == b'\\' && bytes.get(index + 1).is_some_and(|byte| ispunct(*byte)) {
            ctx.decode_append(&mut output, &raw[copied..index])?;
            let output_start = output.len();
            ctx.decode_append(&mut output, &raw[index + 1..index + 2])?;
            if let Some(payload) = payload {
                ctx.fact(AcceptedFact::Stage {
                    payload,
                    stage,
                    kind: StageKind::Escape,
                    input: DSpan {
                        payload,
                        stage: stage - 1,
                        start: index,
                        end: index + 2,
                    },
                    output_start,
                    output_end: output.len(),
                    value: &raw[index + 1..index + 2],
                    hidden_runs: &[],
                })?;
            }
            index += 2;
            copied = index;
        } else {
            index += 1;
        }
    }
    ctx.decode_append(&mut output, &raw[copied..])?;
    ctx.flush_scan()?;
    Ok(output)
}

pub fn is_blank(s: &str) -> bool {
    let mut host = crate::native::LegacyHost;
    let mut ctx = crate::native::Context::new(&mut host, s.len());
    ctx.capture = false;
    match is_blank_with_host(s, &mut ctx) {
        Ok(value) => value,
        Err(error) => match error {},
    }
}
pub(crate) fn is_blank_with_host<H: crate::native::Host>(
    s: &str,
    ctx: &mut crate::native::Context<'_, H>,
) -> Result<bool, H::Error> {
    let result = 'native: {
        for c in s.as_bytes() {
            ctx.scan_step()?;
            match c {
                10 | 13 => break 'native true,
                32 | 9 => (),
                _ => break 'native false,
            }
        }
        true
    };
    ctx.flush_scan()?;
    Ok(result)
}

pub fn normalize_label(input: &str, casing: Case) -> String {
    let mut host = crate::native::LegacyHost;
    let mut ctx = crate::native::Context::new(&mut host, input.len());
    ctx.capture = false;
    match normalize_label_with_host(input, casing, &mut ctx) {
        Ok(value) => value,
        Err(error) => match error {},
    }
}

pub(crate) fn normalize_label_with_host<H: crate::native::Host>(
    input: &str,
    casing: Case,
    ctx: &mut crate::native::Context<'_, H>,
) -> Result<String, H::Error> {
    use caseless::Caseless;
    let mut start = 0;
    let mut end = input.len();
    for (at, ch) in input.char_indices() {
        ctx.scan_step()?;
        if !isspace_char(ch) {
            start = at;
            break;
        }
        start = at + ch.len_utf8();
    }
    for (at, ch) in input[start..].char_indices().rev() {
        ctx.scan_step()?;
        if !isspace_char(ch) {
            break;
        }
        end = start + at;
    }
    let mut value = String::new();
    let mut whitespace = false;
    for ch in input[start..end].chars() {
        ctx.scan_step()?;
        if ch.is_whitespace() {
            if !whitespace {
                ctx.decode_append(&mut value, " ")?;
                whitespace = true;
            }
        } else {
            whitespace = false;
            let mut encoded = [0; 4];
            ctx.decode_append(&mut value, ch.encode_utf8(&mut encoded))?;
        }
    }
    if casing == Case::Fold {
        let mut folded = String::new();
        for ch in value.chars().default_case_fold() {
            ctx.scan_step()?;
            let mut encoded = [0; 4];
            ctx.decode_append(&mut folded, ch.encode_utf8(&mut encoded))?;
        }
        ctx.discard_string(value)?;
        value = folded;
    }
    ctx.flush_scan()?;
    Ok(value)
}

#[test]
fn normalize_label_fold_test() {
    assert_eq!(normalize_label("Abc   \t\ndef", Case::Preserve), "Abc def");
    assert_eq!(normalize_label("Abc   \t\ndef", Case::Fold), "abc def");
    assert_eq!(normalize_label("Straẞe", Case::Preserve), "Straẞe");
    assert_eq!(normalize_label("Straẞe", Case::Fold), "strasse");
}

pub fn split_off_front_matter<'s>(mut s: &'s str, delimiter: &str) -> Option<(&'s str, &'s str)> {
    s = trim_start_match(s, "\u{feff}");

    if !s.starts_with(delimiter) {
        return None;
    }
    let mut start = delimiter.len();
    if s[start..].starts_with('\n') {
        start += 1;
    } else if s[start..].starts_with("\r\n") {
        start += 2;
    } else {
        return None;
    }

    start += {
        let n = s[start..]
            .find(&("\n".to_string() + delimiter + "\r\n"))
            .or_else(|| s[start..].find(&("\n".to_string() + delimiter + "\n")))
            .or_else(|| s[start..].find(&("\n".to_string() + delimiter)))?;
        n + 1 + delimiter.len()
    };

    if start == s.len() {
        return Some((s, ""));
    }

    start += if s[start..].starts_with('\n') {
        1
    } else if s[start..].starts_with("\r\n") {
        2
    } else {
        return None;
    };

    start += if s[start..].starts_with('\n') {
        1
    } else if s[start..].starts_with("\r\n") {
        2
    } else {
        0
    };

    Some((&s[..start], &s[start..]))
}

pub fn trim_start_match<'s>(s: &'s str, pat: &str) -> &'s str {
    s.strip_prefix(pat).unwrap_or(s)
}

pub fn count_newlines(input: &str) -> (usize, usize) {
    let mut host = crate::native::LegacyHost;
    let mut ctx = crate::native::Context::new(&mut host, input.len());
    ctx.capture = false;
    match count_newlines_with_host(input, &mut ctx) {
        Ok(value) => value,
        Err(error) => match error {},
    }
}

pub(crate) fn count_newlines_with_host<H: crate::native::Host>(
    input: &str,
    ctx: &mut crate::native::Context<'_, H>,
) -> Result<(usize, usize), H::Error> {
    let bytes = input.as_bytes();
    let matcher = cr_or_lf_matcher();
    let mut num_lines = 0;
    let mut last_line_start = 0;
    let mut offset = 0;
    while offset < bytes.len() {
        let Some(found) = ctx.find(&bytes[offset..], |part| matcher.find(part))? else {
            break;
        };
        let at = offset + found;
        num_lines += 1;
        last_line_start = at + 1;
        if bytes[at] == b'\r' && bytes.get(at + 1) == Some(&b'\n') {
            last_line_start += 1;
        }
        offset = last_line_start;
    }
    Ok((num_lines, input.len() - last_line_start))
}

pub fn newlines_of(s: &str) -> usize {
    if s.ends_with("\r\n") {
        2
    } else if s.ends_with('\r') || s.ends_with('\n') {
        1
    } else {
        0
    }
}

#[cfg(feature = "phoenix_heex")]
pub fn skip_quoted_string(s: &str, mut cursor: usize, quote_byte: u8) -> usize {
    let bytes = s.as_bytes();
    let len = s.len();
    let mut escaped = false;

    while cursor < len {
        let byte = bytes[cursor];
        if escaped {
            escaped = false;
        } else if byte == b'\\' {
            escaped = true;
        } else if byte == quote_byte {
            return cursor + 1;
        }
        cursor += 1;
    }

    cursor
}

#[cfg(feature = "phoenix_heex")]
pub fn find_matching_brace(s: &str, start: usize) -> Option<usize> {
    let bytes = s.as_bytes();
    let mut cursor = start;
    let mut depth = 1;
    let len = s.len();

    while cursor < len {
        match bytes[cursor] {
            b'"' | b'\'' => {
                cursor = skip_quoted_string(s, cursor + 1, bytes[cursor]);
            }
            b'{' => {
                depth += 1;
                cursor += 1;
            }
            b'}' => {
                depth -= 1;
                if depth == 0 {
                    return Some(cursor + 1);
                }
                cursor += 1;
            }
            _ => cursor += 1,
        }
    }
    None
}

#[cfg(feature = "phoenix_heex")]
pub fn phoenix_inline_tag(s: &str) -> Option<usize> {
    let tag_name_len = scanners::phoenix_opening_tag(s)?;
    let bytes = s.as_bytes();
    let mut cursor = 1 + tag_name_len;
    let len = s.len();

    while cursor < len {
        match bytes[cursor] {
            b'>' => return Some(cursor + 1),
            b'/' if cursor + 1 < len && bytes[cursor + 1] == b'>' => return Some(cursor + 2),
            b'"' | b'\'' => {
                cursor = skip_quoted_string(s, cursor + 1, bytes[cursor]);
            }
            b'{' => {
                cursor = find_matching_brace(s, cursor + 1)?;
            }
            _ => cursor += 1,
        }
    }

    None
}

#[cfg(feature = "phoenix_heex")]
pub fn phoenix_inline_expression(s: &str) -> Option<usize> {
    if !s.starts_with('{') {
        return None;
    }
    find_matching_brace(s, 1)
}

#[cfg(test)]
pub mod tests {
    use super::{
        count_newlines, ltrim, normalize_code, normalize_label, shift_buf_left,
        split_off_front_matter,
    };
    use crate::strings::Case;

    #[test]
    fn normalize_code_handles_lone_newline() {
        assert_eq!(normalize_code("\n"), " ");
    }

    #[test]
    fn normalize_code_handles_lone_space() {
        assert_eq!(normalize_code(" "), " ");
    }

    #[test]
    fn front_matter() {
        assert_eq!(
            split_off_front_matter("---\nfoo: bar\n---\nHiiii", "---"),
            Some(("---\nfoo: bar\n---\n", "Hiiii"))
        );
        assert_eq!(
            split_off_front_matter(
                "\u{feff}!@#\r\n\r\nfoo: !@# \r\nquux\n!@#\r\n\n\nYes!\n",
                "!@#"
            ),
            Some(("!@#\r\n\r\nfoo: !@# \r\nquux\n!@#\r\n\n", "\nYes!\n"))
        );
        assert_eq!(
            split_off_front_matter(
                "\u{feff}!@#\r\n\r\nfoo: \n!@# \r\nquux\n!@#\r\n\n\nYes!\n",
                "!@#"
            ),
            Some(("!@#\r\n\r\nfoo: \n!@# \r\nquux\n!@#\r\n\n", "\nYes!\n"))
        );
    }

    #[test]
    fn normalize_label_lowercase() {
        assert_eq!(normalize_label("  Foo\u{A0}BAR  ", Case::Fold), "foo bar");
        assert_eq!(normalize_label("  FooİBAR  ", Case::Fold), "fooi\u{307}bar");
    }

    #[test]
    fn normalize_label_preserve() {
        assert_eq!(
            normalize_label("  Foo\u{A0}BAR  ", Case::Preserve),
            "Foo BAR"
        );
        assert_eq!(normalize_label("  FooİBAR  ", Case::Preserve), "FooİBAR");
    }

    #[test]
    fn shift_buf_left_ok() {
        let mut b: Vec<u8>;

        b = vec![1, 2, 3, 4, 5, 6];
        shift_buf_left(&mut b, 1);
        assert_eq!(b, vec![2, 3, 4, 5, 6, 6]);

        shift_buf_left(&mut b, 2);
        assert_eq!(b, vec![4, 5, 6, 6, 6, 6]);
    }

    #[test]
    fn ltrim_ok() {
        let mut s: String;

        s = "okay".to_string();
        ltrim(&mut s);
        assert_eq!(s, "okay");

        s = "   okay".to_string();
        ltrim(&mut s);
        assert_eq!(s, "okay");
    }

    #[test]
    fn count_newlines_ok() {
        assert_eq!((0, 7), count_newlines("abcdefg"));
        assert_eq!((2, 0), count_newlines("abc\ndefg\n"));
        assert_eq!((3, 2), count_newlines("abc\rde\nfg\nhi"));
    }

    #[test]
    fn count_newlines_handles_crlf_and_utf8() {
        for (input, expected) in [
            ("", (0, 0)),
            ("\r", (1, 0)),
            ("\n", (1, 0)),
            ("\r\n", (1, 0)),
            ("\r\r\n\n\r", (4, 0)),
            ("é\r\n中文", (1, 6)),
            ("é\r\n中文\r\n", (2, 0)),
        ] {
            assert_eq!(count_newlines(input), expected, "{input:?}");
        }
    }

    #[test]
    fn count_newlines_handles_crlf_at_scan_boundaries() {
        for prefix_len in 0..128 {
            let input = format!("{}\r\n\r\n\né", "x".repeat(prefix_len));
            assert_eq!(count_newlines(&input), (3, 2), "{prefix_len}");
        }
    }
}
