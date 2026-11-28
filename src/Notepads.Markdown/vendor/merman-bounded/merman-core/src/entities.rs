use std::borrow::Cow;

/// Reverts only the placeholder spelling introduced by Mermaid's `encodeEntities` preprocessing.
///
/// This is the exact representation transition used by Mermaid's final `cleanUpSvgCode` pass. It
/// deliberately does not parse the resulting HTML character references; callers that operate on
/// serialized markup must leave that work to the browser parser.
pub(crate) fn decode_mermaid_entity_placeholders(input: &str) -> Cow<'_, str> {
    if !input.contains('\u{fb02}') && !input.contains('\u{00b6}') {
        return Cow::Borrowed(input);
    }

    Cow::Owned(
        input
            .replace("ﬂ°°", "&#")
            .replace("ﬂ°", "&")
            .replace("¶ß", ";"),
    )
}

/// Decodes Mermaid's `encodeEntities` placeholders and shorthand `#...;` sequences into Unicode.
///
/// Upstream Mermaid runs `encodeEntities(text)` before parsing, and later uses `decodeEntities`
/// + browser `entityDecode(...)` to turn placeholders into actual characters.
///
/// In `merman` we decode these into Unicode as part of headless parsing so that:
/// - layout measurements operate on the same final text
/// - SVG output matches upstream DOM output
pub fn decode_mermaid_entities_to_unicode(input: &str) -> Cow<'_, str> {
    let entities = restore_mermaid_entity_spelling(input);
    if !entities.contains('&') {
        return entities;
    }

    Cow::Owned(decode_html_entities_to_unicode(entities.as_ref()).into_owned())
}

/// Restores Mermaid preprocessor placeholders and `#...;` shorthand to HTML entity spelling.
///
/// This intentionally stops before browser entity decoding. Flowchart SVG text labels need the
/// literal `&nbsp;` spelling, while HTML labels decode the same spelling when inserted as markup.
pub fn restore_mermaid_entity_spelling(input: &str) -> Cow<'_, str> {
    if !input.contains('#') && !input.contains('ﬂ') && !input.contains('¶') {
        return Cow::Borrowed(input);
    }

    // Step 1: Mermaid placeholders -> `&...;` / `&#...;`
    let mut s = decode_mermaid_entity_placeholders(input).into_owned();

    // Step 2 (shorthand): `#...;` -> `&...;` / `&#...;`
    //
    // This is primarily for older headless code paths / fixtures that bypass upstream-like
    // preprocessing. It is intentionally conservative and only rewrites `#\w+;` patterns.
    if s.contains('#') {
        let mut out = String::with_capacity(s.len());
        let mut it = s.chars().peekable();
        let mut prev: Option<char> = None;
        while let Some(ch) = it.next() {
            if ch != '#' {
                out.push(ch);
                prev = Some(ch);
                continue;
            }

            // Do not treat `&#...;` as Mermaid shorthand `#...;`.
            if prev == Some('&') {
                out.push('#');
                prev = Some('#');
                continue;
            }

            let mut entity = String::new();
            let mut ok = false;
            for _ in 0..64 {
                match it.peek().copied() {
                    Some(';') => {
                        it.next();
                        ok = true;
                        break;
                    }
                    Some(c) if c.is_ascii_alphanumeric() || c == '_' || c == '+' => {
                        entity.push(c);
                        it.next();
                    }
                    _ => break,
                }
            }

            if !ok {
                out.push('#');
                out.push_str(&entity);
                continue;
            }

            let is_int = entity.chars().all(|c| c.is_ascii_digit() || c == '+')
                && entity.chars().any(|c| c.is_ascii_digit());
            if is_int {
                out.push('&');
                out.push('#');
                out.push_str(&entity);
                out.push(';');
            } else {
                out.push('&');
                out.push_str(&entity);
                out.push(';');
            }
            prev = Some(';');
        }
        s = out;
    }

    Cow::Owned(s)
}

/// Decodes browser-facing HTML entities into Unicode without Mermaid shorthand handling.
pub fn decode_html_entities_to_unicode(input: &str) -> Cow<'_, str> {
    if !input.contains('&') {
        return Cow::Borrowed(input);
    }

    htmlize::unescape(input)
}

/// Visits browser-facing HTML text as decoded Unicode without retaining an intermediate string.
///
/// The matcher follows the same general-text rules as [`decode_html_entities_to_unicode`],
/// including legacy bare named references and WHATWG numeric-reference corrections. Borrowed
/// source ranges and static entity expansions are forwarded directly to `visit`; numeric scalar
/// expansions use one stack buffer whose lifetime is limited to the callback.
pub fn visit_decoded_html_entities<E>(
    input: &str,
    mut visit: impl FnMut(&str) -> Result<(), E>,
) -> Result<(), E> {
    visit_decoded_html_entity_fragments(input, |fragment| match fragment {
        DecodedHtmlFragment::Borrowed(value) => visit(value),
        DecodedHtmlFragment::Scalar(ch) => {
            let mut buffer = [0u8; 4];
            visit(ch.encode_utf8(&mut buffer))
        }
    })
}

/// One allocation-free fragment produced by [`visit_decoded_html_entity_fragments`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DecodedHtmlFragment<'a> {
    Borrowed(&'a str),
    Scalar(char),
}

/// Visits decoded HTML text while preserving scalar expansions as values rather than temporary
/// string slices. This form is intended for borrowed render plans that must retain the replay
/// recipe without copying authored text.
pub fn visit_decoded_html_entity_fragments<'a, E>(
    input: &'a str,
    visit: impl FnMut(DecodedHtmlFragment<'a>) -> Result<(), E>,
) -> Result<(), E> {
    visit_decoded_html_entity_fragments_impl(input, usize::MAX, || Ok(()), visit)
}

/// Visits decoded HTML text with bounded checkpoints while scanning and replaying source spans.
///
/// This is an internal cross-crate rendering seam rather than a separate entity implementation:
/// it follows the exact matcher used by [`visit_decoded_html_entity_fragments`] while preventing
/// a long plain-text span from becoming one opaque scan and callback.
#[doc(hidden)]
pub fn visit_decoded_html_entity_fragments_with_checkpoints<'a, E>(
    input: &'a str,
    checkpoint: impl FnMut() -> Result<(), E>,
    visit: impl FnMut(DecodedHtmlFragment<'a>) -> Result<(), E>,
) -> Result<(), E> {
    const CHECKPOINT_BYTES: usize = 64;

    visit_decoded_html_entity_fragments_impl(input, CHECKPOINT_BYTES, checkpoint, visit)
}

fn visit_decoded_html_entity_fragments_impl<'a, E>(
    input: &'a str,
    checkpoint_bytes: usize,
    mut checkpoint: impl FnMut() -> Result<(), E>,
    mut visit: impl FnMut(DecodedHtmlFragment<'a>) -> Result<(), E>,
) -> Result<(), E> {
    checkpoint()?;
    let mut scan = 0usize;
    let mut emitted = 0usize;
    let mut next_checkpoint = checkpoint_bytes;
    while scan < input.len() {
        if scan >= next_checkpoint {
            checkpoint()?;
            let mut chunk_end = scan;
            while chunk_end > emitted && !input.is_char_boundary(chunk_end) {
                chunk_end -= 1;
            }
            if emitted < chunk_end {
                visit(DecodedHtmlFragment::Borrowed(&input[emitted..chunk_end]))?;
                emitted = chunk_end;
            }
            next_checkpoint = scan.saturating_add(checkpoint_bytes);
        }

        if input.as_bytes()[scan] != b'&' {
            scan += 1;
            continue;
        }

        let amp = scan;
        let Some(entity) = match_html_entity(&input.as_bytes()[amp..]) else {
            scan = amp + 1;
            continue;
        };
        if emitted < amp {
            visit(DecodedHtmlFragment::Borrowed(&input[emitted..amp]))?;
        }
        match entity.expansion {
            HtmlEntityExpansion::Static(bytes) => {
                visit(DecodedHtmlFragment::Borrowed(
                    std::str::from_utf8(bytes).expect("HTML entities expand to UTF-8"),
                ))?;
            }
            HtmlEntityExpansion::Scalar(ch) => {
                visit(DecodedHtmlFragment::Scalar(ch))?;
            }
        }
        scan = amp + entity.consumed;
        emitted = scan;
    }
    if emitted < input.len() {
        checkpoint()?;
        visit(DecodedHtmlFragment::Borrowed(&input[emitted..]))?;
    }
    checkpoint()?;
    Ok(())
}

#[derive(Debug, Clone, Copy)]
struct HtmlEntityMatch {
    consumed: usize,
    expansion: HtmlEntityExpansion,
}

#[derive(Debug, Clone, Copy)]
enum HtmlEntityExpansion {
    Static(&'static [u8]),
    Scalar(char),
}

fn match_html_entity(input: &[u8]) -> Option<HtmlEntityMatch> {
    debug_assert_eq!(input.first(), Some(&b'&'));
    if input.get(1) == Some(&b'#') {
        return match_numeric_html_entity(input);
    }

    let mut candidate_end = 1usize;
    while candidate_end < input.len()
        && candidate_end < htmlize::ENTITY_MAX_LENGTH
        && input[candidate_end].is_ascii_alphanumeric()
    {
        candidate_end += 1;
    }
    let has_semicolon = input.get(candidate_end) == Some(&b';');
    let closed_end = candidate_end + usize::from(has_semicolon);
    if has_semicolon && let Some(&expansion) = htmlize::ENTITIES.get(&input[..closed_end]) {
        return Some(HtmlEntityMatch {
            consumed: closed_end,
            expansion: HtmlEntityExpansion::Static(expansion),
        });
    }

    let candidate = &input[..closed_end.min(input.len())];
    let max_bare = candidate.len().min(htmlize::BARE_ENTITY_MAX_LENGTH);
    for length in htmlize::ENTITY_MIN_LENGTH..=max_bare {
        if let Some(&expansion) = htmlize::ENTITIES.get(&candidate[..length]) {
            return Some(HtmlEntityMatch {
                consumed: length,
                expansion: HtmlEntityExpansion::Static(expansion),
            });
        }
    }
    None
}

fn match_numeric_html_entity(input: &[u8]) -> Option<HtmlEntityMatch> {
    let (radix, digits_start) = match input.get(2) {
        Some(b'x' | b'X') => (16, 3),
        Some(_) => (10, 2),
        None => return None,
    };
    let mut digits_end = digits_start;
    while digits_end < input.len()
        && match radix {
            16 => input[digits_end].is_ascii_hexdigit(),
            _ => input[digits_end].is_ascii_digit(),
        }
    {
        digits_end += 1;
    }
    if digits_end == digits_start {
        return None;
    }
    let consumed = digits_end + usize::from(input.get(digits_end) == Some(&b';'));
    let digits = std::str::from_utf8(&input[digits_start..digits_end])
        .expect("numeric HTML entities contain only ASCII digits");
    let expansion = match u32::from_str_radix(digits, radix) {
        Ok(number) => corrected_numeric_html_entity(number),
        Err(error) if matches!(error.kind(), std::num::IntErrorKind::PosOverflow) => {
            HtmlEntityExpansion::Scalar('\u{fffd}')
        }
        Err(_) => return None,
    };
    Some(HtmlEntityMatch {
        consumed,
        expansion,
    })
}

fn corrected_numeric_html_entity(number: u32) -> HtmlEntityExpansion {
    let corrected = match number {
        0 | 0x11_0000.. | 0xD800..=0xDFFF => '\u{fffd}',
        0x80 => '\u{20ac}',
        0x82 => '\u{201a}',
        0x83 => '\u{0192}',
        0x84 => '\u{201e}',
        0x85 => '\u{2026}',
        0x86 => '\u{2020}',
        0x87 => '\u{2021}',
        0x88 => '\u{02c6}',
        0x89 => '\u{2030}',
        0x8a => '\u{0160}',
        0x8b => '\u{2039}',
        0x8c => '\u{0152}',
        0x8e => '\u{017d}',
        0x91 => '\u{2018}',
        0x92 => '\u{2019}',
        0x93 => '\u{201c}',
        0x94 => '\u{201d}',
        0x95 => '\u{2022}',
        0x96 => '\u{2013}',
        0x97 => '\u{2014}',
        0x98 => '\u{02dc}',
        0x99 => '\u{2122}',
        0x9a => '\u{0161}',
        0x9b => '\u{203a}',
        0x9c => '\u{0153}',
        0x9e => '\u{017e}',
        0x9f => '\u{0178}',
        value => char::from_u32(value).unwrap_or('\u{fffd}'),
    };
    HtmlEntityExpansion::Scalar(corrected)
}

#[cfg(test)]
mod tests {
    use super::{
        decode_html_entities_to_unicode, decode_mermaid_entities_to_unicode,
        decode_mermaid_entity_placeholders, restore_mermaid_entity_spelling,
        visit_decoded_html_entities, visit_decoded_html_entity_fragments_with_checkpoints,
    };

    fn assert_streaming_html_decode_matches_owned(input: &str) {
        let mut streamed = String::new();
        visit_decoded_html_entities(input, |fragment| {
            streamed.push_str(fragment);
            Ok::<(), std::convert::Infallible>(())
        })
        .expect("infallible visitor should complete");
        assert_eq!(
            streamed,
            decode_html_entities_to_unicode(input),
            "{input:?}"
        );
    }

    #[test]
    fn html_entity_decode_does_not_apply_mermaid_shorthand() {
        assert_eq!(
            decode_html_entities_to_unicode("Tom &amp; Jerry &lt;ok&gt; &#39;x&#39;"),
            "Tom & Jerry <ok> 'x'"
        );
        assert_eq!(decode_html_entities_to_unicode("#quot;"), "#quot;");
    }

    #[test]
    fn streaming_html_entity_decode_matches_the_owned_decoder() {
        for input in [
            "plain",
            "Tom &amp; Jerry &lt;ok&gt; &#39;x&#39;",
            "&times &times; &timesb; &timesbar; &timesd;",
            "&#0; &#x80; &#x110000; &unknown;",
            "&nGg; and &nLl;",
        ] {
            assert_streaming_html_decode_matches_owned(input);
        }

        for (entity, _) in htmlize::ENTITIES.entries() {
            let entity = std::str::from_utf8(entity).expect("HTML entity keys are ASCII");
            assert_streaming_html_decode_matches_owned(entity);
            assert_streaming_html_decode_matches_owned(&format!("before{entity}after"));
            assert_streaming_html_decode_matches_owned(&format!("{entity}suffix"));

            if let Some(bare) = entity.strip_suffix(';') {
                assert_streaming_html_decode_matches_owned(bare);
                assert_streaming_html_decode_matches_owned(&format!("{bare}suffix"));
            }
        }

        for value in 0..=0xffu32 {
            assert_streaming_html_decode_matches_owned(&format!("&#{value};"));
            assert_streaming_html_decode_matches_owned(&format!("&#{value}suffix"));
            assert_streaming_html_decode_matches_owned(&format!("&#x{value:x};"));
            assert_streaming_html_decode_matches_owned(&format!("&#X{value:X}suffix"));
        }
        for value in [
            0x7ff,
            0x800,
            0xd7ff,
            0xd800,
            0xdfff,
            0xe000,
            0x10ffff,
            0x110000,
            u32::MAX,
        ] {
            assert_streaming_html_decode_matches_owned(&format!("&#{value};"));
            assert_streaming_html_decode_matches_owned(&format!("&#x{value:x};"));
        }
        for input in [
            "&#999999999999999999999999999999;",
            "&#xFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFF;",
            "&#;",
            "&#x;",
        ] {
            assert_streaming_html_decode_matches_owned(input);
        }
    }

    #[test]
    fn controlled_html_entity_scan_stops_before_copying_the_full_plain_span() {
        let input = "A".repeat(4_096);
        let mut checkpoints = 0usize;
        let mut copied = 0usize;

        let result = visit_decoded_html_entity_fragments_with_checkpoints(
            &input,
            || {
                checkpoints += 1;
                if checkpoints == 2 {
                    Err("cancelled")
                } else {
                    Ok(())
                }
            },
            |fragment| {
                copied += match fragment {
                    super::DecodedHtmlFragment::Borrowed(value) => value.len(),
                    super::DecodedHtmlFragment::Scalar(value) => value.len_utf8(),
                };
                Ok(())
            },
        );

        assert_eq!(result, Err("cancelled"));
        assert_eq!(copied, 0);
    }

    #[test]
    fn mermaid_entity_decode_keeps_shorthand_and_placeholder_semantics() {
        assert_eq!(decode_mermaid_entities_to_unicode("#quot;"), "\"");
        assert_eq!(decode_mermaid_entities_to_unicode("ﬂ°quot¶ß"), "\"");
        assert_eq!(decode_mermaid_entities_to_unicode("ﬂ°°39¶ß"), "'");
    }

    #[test]
    fn mermaid_entity_spelling_restoration_stops_before_browser_decode() {
        assert_eq!(restore_mermaid_entity_spelling("#nbsp;"), "&nbsp;");
        assert_eq!(restore_mermaid_entity_spelling("ﬂ°nbsp¶ß"), "&nbsp;");
        assert_eq!(restore_mermaid_entity_spelling("ﬂ°°160¶ß"), "&#160;");
        assert_eq!(restore_mermaid_entity_spelling("&nbsp;"), "&nbsp;");
    }

    #[test]
    fn placeholder_decode_preserves_the_serialized_entity_layer() {
        assert_eq!(
            decode_mermaid_entity_placeholders("javascriptﬂ°colon¶ßalert(1)"),
            "javascript&colon;alert(1)"
        );
        assert_eq!(
            decode_mermaid_entity_placeholders("ticket&amp;value"),
            "ticket&amp;value"
        );
    }
}
