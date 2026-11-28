// Notepads Markdown modifications Copyright (c) 2026, Jiaqi (0x7c13) Liu.
// Original third-party licenses remain applicable; see the crate COPYING file.

use std::borrow::Cow;
use std::char;
use std::cmp::min;
use std::str;

use crate::ctype::isdigit;

pub const ENTITY_MIN_LENGTH: usize = 2;
pub const ENTITY_MAX_LENGTH: usize = 32;

pub(crate) fn unescape_with_host<H: crate::native::Host>(
    text: &str,
    ctx: &mut crate::native::Context<'_, H>,
) -> Result<Option<(Cow<'static, str>, usize)>, H::Error> {
    let result = 'native: {
        let bytes = text.as_bytes();
        if text.len() >= 3 && bytes[0] == b'#' {
            let mut codepoint: u32 = 0;
            let mut i = 0;

            let num_digits = if isdigit(bytes[1]) {
                i = 1;
                while i < text.len() && isdigit(bytes[i]) {
                    ctx.scan_step()?;
                    codepoint = (codepoint * 10) + (bytes[i] as u32 - '0' as u32);
                    codepoint = min(codepoint, 0x11_0000);
                    i += 1;
                }
                i - 1
            } else if bytes[1] == b'x' || bytes[1] == b'X' {
                i = 2;
                while i < bytes.len() && bytes[i].is_ascii_hexdigit() {
                    ctx.scan_step()?;
                    codepoint = (codepoint * 16) + ((bytes[i] as u32 | 32) % 39 - 9);
                    codepoint = min(codepoint, 0x11_0000);
                    i += 1;
                }
                i - 2
            } else {
                0
            };

            let is_hex = bytes[1] == b'x' || bytes[1] == b'X';

            if i < bytes.len()
                && bytes[i] == b';'
                && ((is_hex && (1..=6).contains(&num_digits))
                    || (!is_hex && (1..=7).contains(&num_digits)))
            {
                if codepoint == 0 || (0xD800..=0xDFFF).contains(&codepoint) || codepoint >= 0x110000
                {
                    codepoint = 0xFFFD;
                }
                let scalar = char::from_u32(codepoint).unwrap_or('\u{FFFD}');
                let mut encoded = [0; 4];
                let value = ctx.string(scalar.encode_utf8(&mut encoded))?;
                break 'native Some((value.into(), i + 1));
            }
        }

        let size = min(text.len(), ENTITY_MAX_LENGTH);
        for i in ENTITY_MIN_LENGTH..size {
            ctx.scan_step()?;
            if bytes[i] == b' ' {
                break 'native None;
            }

            if bytes[i] == b';' {
                break 'native lookup(&text[..i]).map(|e| (e.into(), i + 1));
            }
        }

        None
    };
    ctx.flush_scan()?;
    Ok(result)
}

include!(concat!(env!("OUT_DIR"), "/entitydata.rs"));

fn lookup(text: &str) -> Option<&'static str> {
    entitydata::ENTITY_MAP.get(text).copied()
}

pub fn unescape_html(src: &str) -> Cow<'_, str> {
    let mut host = crate::native::LegacyHost;
    let mut ctx = crate::native::Context::new(&mut host, src.len());
    ctx.capture = false;
    match unescape_html_with_host(src, &mut ctx, None, 1) {
        Ok(value) => value.into(),
        Err(error) => match error {},
    }
}

pub(crate) fn unescape_html_with_host<H: crate::native::Host>(
    src: &str,
    ctx: &mut crate::native::Context<'_, H>,
    payload: Option<crate::native::PayloadId>,
    stage: u32,
) -> Result<String, H::Error> {
    unescape_html_with_targets(src, ctx, payload, stage, None)
}

pub(crate) fn unescape_html_with_targets<H: crate::native::Host>(
    src: &str,
    ctx: &mut crate::native::Context<'_, H>,
    payload: Option<crate::native::PayloadId>,
    stage: u32,
    inline: Option<(crate::native::NativeNodeId, crate::native::RSpan)>,
) -> Result<String, H::Error> {
    use crate::native::{AcceptedFact, DSpan, StageKind};
    let bytes = src.as_bytes();
    let mut output = String::new();
    ctx.grow_string(&mut output, src.len())?;
    let mut index = 0;
    let mut copied = 0;
    while index < bytes.len() {
        ctx.scan_step()?;
        if bytes[index] == b'&'
            && let Some((value, length)) = unescape_with_host(&src[index + 1..], ctx)?
        {
            ctx.decode_append(&mut output, &src[copied..index])?;
            let output_start = output.len();
            ctx.decode_append(&mut output, &value)?;
            if let Some(payload) = payload {
                ctx.fact(AcceptedFact::Stage {
                    payload,
                    stage,
                    kind: StageKind::Entity,
                    input: DSpan {
                        payload,
                        stage: stage - 1,
                        start: index,
                        end: index + length + 1,
                    },
                    output_start,
                    output_end: output.len(),
                    value: &value,
                    hidden_runs: &[],
                })?;
            }
            if let Some((node, raw)) = inline {
                ctx.fact(AcceptedFact::DecodeAtom {
                    node,
                    kind: crate::native::DecodeKind::Entity,
                    source: crate::native::RSpan {
                        buffer: raw.buffer,
                        start: raw.start + index,
                        end: raw.start + index + length + 1,
                    },
                    value: &value,
                })?;
            }
            index += length + 1;
            copied = index;
            if let Cow::Owned(value) = value {
                ctx.discard_string(value)?;
            }
            continue;
        }
        index += 1;
    }
    ctx.decode_append(&mut output, &src[copied..])?;
    ctx.flush_scan()?;
    Ok(output)
}
