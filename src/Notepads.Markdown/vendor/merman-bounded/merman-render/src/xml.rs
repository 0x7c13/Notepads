use std::borrow::Cow;

use crate::svg::scanner::find_tag_end;

pub(crate) fn is_xml_1_0_char(ch: char) -> bool {
    matches!(ch, '\u{9}' | '\u{A}' | '\u{D}')
        || matches!(
            ch,
            '\u{20}'..='\u{D7FF}' | '\u{E000}'..='\u{FFFD}' | '\u{10000}'..='\u{10FFFF}'
        )
}

/// Removes scalar values that XML 1.0 cannot serialize, without allocating for ordinary text.
pub(crate) fn strip_forbidden_xml_1_0_chars(value: &str) -> Cow<'_, str> {
    let Some((first_invalid, invalid)) = value.char_indices().find(|(_, ch)| !is_xml_1_0_char(*ch))
    else {
        return Cow::Borrowed(value);
    };

    let mut out = String::with_capacity(value.len() - invalid.len_utf8());
    out.push_str(&value[..first_invalid]);
    out.extend(
        value[first_invalid + invalid.len_utf8()..]
            .chars()
            .filter(|ch| is_xml_1_0_char(*ch)),
    );
    Cow::Owned(out)
}

/// Enforces the XML 1.0 scalar-value contract while preserving an existing owned allocation when
/// no normalization is required.
#[cfg(test)]
pub(crate) fn strip_forbidden_xml_1_0_chars_cow<'a>(value: Cow<'a, str>) -> Cow<'a, str> {
    match strip_forbidden_xml_1_0_chars(value.as_ref()) {
        Cow::Borrowed(_) => value,
        Cow::Owned(normalized) => Cow::Owned(normalized),
    }
}

/// Enforces the XML 1.0 scalar contract with bounded cooperative checkpoints.
///
/// Valid borrowed and owned inputs retain their original storage. Allocation begins only after
/// the first forbidden scalar is observed.
pub(crate) fn strip_forbidden_xml_1_0_chars_cow_with_checkpoints<'a, E>(
    value: Cow<'a, str>,
    mut checkpoint: impl FnMut() -> Result<(), E>,
) -> Result<Cow<'a, str>, E> {
    const CHECKPOINT_SCALARS: usize = 64;

    let mut normalized = None;
    let mut retained_start = 0usize;
    for (iteration, (index, ch)) in value.char_indices().enumerate() {
        if iteration % CHECKPOINT_SCALARS == 0 {
            checkpoint()?;
        }
        if is_xml_1_0_char(ch) {
            continue;
        }

        let output = normalized.get_or_insert_with(|| String::with_capacity(value.len()));
        output.push_str(&value[retained_start..index]);
        retained_start = index + ch.len_utf8();
    }
    checkpoint()?;

    let Some(mut normalized) = normalized else {
        return Ok(value);
    };
    normalized.push_str(&value[retained_start..]);
    Ok(Cow::Owned(normalized))
}

pub(crate) fn is_valid_xml_entity_reference(entity: &str) -> bool {
    if entity.is_empty() {
        return false;
    }
    if let Some(hex) = entity.strip_prefix("#x") {
        return u32::from_str_radix(hex, 16)
            .ok()
            .and_then(char::from_u32)
            .is_some_and(is_xml_1_0_char);
    }
    if let Some(decimal) = entity.strip_prefix('#') {
        return decimal
            .parse::<u32>()
            .ok()
            .and_then(char::from_u32)
            .is_some_and(is_xml_1_0_char);
    }
    matches!(entity, "amp" | "apos" | "gt" | "lt" | "quot")
}

fn push_xml_escaped(out: &mut String, value: &str) {
    for ch in value.chars().filter(|ch| is_xml_1_0_char(*ch)) {
        match ch {
            '&' => out.push_str("&amp;"),
            '\'' => out.push_str("&apos;"),
            '>' => out.push_str("&gt;"),
            '<' => out.push_str("&lt;"),
            '"' => out.push_str("&quot;"),
            _ => out.push(ch),
        }
    }
}

fn html_entity_reference_end(value: &str, amp: usize) -> Option<usize> {
    let bytes = value.as_bytes();
    let mut cursor = amp.checked_add(1)?;
    let first = *bytes.get(cursor)?;

    if first == b'#' {
        cursor += 1;
        let hexadecimal = bytes
            .get(cursor)
            .is_some_and(|byte| matches!(byte, b'x' | b'X'));
        if hexadecimal {
            cursor += 1;
        }
        let digits_start = cursor;
        while cursor < bytes.len()
            && cursor - amp <= 64
            && if hexadecimal {
                bytes[cursor].is_ascii_hexdigit()
            } else {
                bytes[cursor].is_ascii_digit()
            }
        {
            cursor += 1;
        }
        return (cursor > digits_start && bytes.get(cursor) == Some(&b';')).then_some(cursor);
    }

    let name_start = cursor;
    while cursor < bytes.len() && cursor - amp <= 64 && bytes[cursor].is_ascii_alphanumeric() {
        cursor += 1;
    }
    (cursor > name_start && bytes.get(cursor) == Some(&b';')).then_some(cursor)
}

/// Converts browser-facing HTML entity references into XML-safe serialization.
///
/// XML only defines five named entities. HTML entities such as `&nbsp;` are decoded to their
/// Unicode value, while unknown references are preserved as literal text by escaping `&`.
pub(crate) fn normalize_html_entities_for_xml(value: &str) -> Cow<'_, str> {
    let value = strip_forbidden_xml_1_0_chars(value);
    if !value.as_bytes().contains(&b'&') {
        return value;
    }

    let value = value.as_ref();
    let mut out = String::with_capacity(value.len());
    let mut cursor = 0usize;
    while let Some(relative_amp) = value[cursor..].find('&') {
        let amp = cursor + relative_amp;
        out.push_str(&value[cursor..amp]);
        let Some(semicolon) = html_entity_reference_end(value, amp) else {
            out.push_str("&amp;");
            cursor = amp + 1;
            continue;
        };
        let entity = &value[amp + 1..semicolon];
        if is_valid_xml_entity_reference(entity) {
            out.push_str(&value[amp..=semicolon]);
            cursor = semicolon + 1;
            continue;
        }

        let reference = &value[amp..=semicolon];
        let decoded = merman_core::entities::decode_html_entities_to_unicode(reference);
        if decoded.as_ref() != reference {
            push_xml_escaped(&mut out, decoded.as_ref());
        } else {
            out.push_str("&amp;");
            out.push_str(entity);
            out.push(';');
        }
        cursor = semicolon + 1;
    }
    out.push_str(&value[cursor..]);
    Cow::Owned(out)
}

/// Normalizes sanitized browser HTML into a fragment that can be embedded in SVG XML.
pub(crate) fn normalize_html_fragment_for_xhtml(input: &str) -> String {
    let input = input
        .replace("<br>", "<br />")
        .replace("<br/>", "<br />")
        .replace("<br >", "<br />")
        .replace("</br>", "<br />")
        .replace("</br/>", "<br />")
        .replace("</br />", "<br />")
        .replace("</br >", "<br />");
    let input = normalize_html_entities_for_xml(&input);

    fn is_xhtml_void_tag(name: &str) -> bool {
        matches!(
            name,
            "br" | "img"
                | "hr"
                | "input"
                | "meta"
                | "link"
                | "source"
                | "area"
                | "base"
                | "col"
                | "embed"
                | "param"
                | "track"
                | "wbr"
        )
    }

    fn self_close_xhtml_void_tag(tag: &str) -> String {
        if !tag.ends_with('>') {
            return tag.to_string();
        }
        let mut inner = tag[..tag.len() - 1].to_string();
        while inner.ends_with(|character: char| character.is_whitespace()) {
            inner.pop();
        }
        if inner.ends_with('/') {
            while inner.ends_with('/') {
                inner.pop();
            }
            while inner.ends_with(|character: char| character.is_whitespace()) {
                inner.pop();
            }
        }
        inner.push_str(" /");
        inner.push('>');
        inner
    }

    let mut out = String::with_capacity(input.len());
    let mut characters = input.char_indices().peekable();

    while let Some((offset, character)) = characters.next() {
        match character {
            '<' => {
                let next = characters.peek().map(|(_, character)| *character);
                if !matches!(
                    next,
                    Some(next) if next.is_ascii_alphabetic() || matches!(next, '/' | '!' | '?')
                ) {
                    out.push_str("&lt;");
                    continue;
                }

                let Some(end) = find_tag_end(input.as_ref(), offset + character.len_utf8()) else {
                    out.push_str("&lt;");
                    continue;
                };
                while characters
                    .peek()
                    .is_some_and(|(character_offset, _)| *character_offset <= end)
                {
                    characters.next();
                }

                let tag = &input[offset..=end];
                let tag = tag.trim();
                let inner = tag.trim_start_matches('<').trim_end_matches('>').trim();
                let is_closing = inner.starts_with('/');
                let name = inner
                    .trim_start_matches('/')
                    .trim_end_matches('/')
                    .split_whitespace()
                    .next()
                    .unwrap_or("")
                    .to_ascii_lowercase();
                if !is_closing && is_xhtml_void_tag(&name) {
                    out.push_str(&self_close_xhtml_void_tag(tag));
                } else {
                    out.push_str(tag);
                }
            }
            '>' => out.push_str("&gt;"),
            '&' => {
                let mut tail = String::new();
                let mut valid_terminator = false;
                for _ in 0..32 {
                    match characters.peek().map(|(_, character)| *character) {
                        Some(';') => {
                            characters.next();
                            tail.push(';');
                            valid_terminator = true;
                            break;
                        }
                        Some(character)
                            if character.is_ascii_alphanumeric()
                                || matches!(character, '#' | 'x' | 'X') =>
                        {
                            characters.next();
                            tail.push(character);
                        }
                        _ => break,
                    }
                }
                let entity = tail.strip_suffix(';').unwrap_or(&tail);
                if valid_terminator && is_valid_xml_entity_reference(entity) {
                    out.push('&');
                    out.push_str(&tail);
                } else {
                    out.push_str("&amp;");
                    out.push_str(&tail);
                }
            }
            _ => out.push(character),
        }
    }

    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stripping_is_borrowed_for_valid_xml_and_removes_every_forbidden_range() {
        let valid = "tab\tline\ncarriage\rUnicode \u{10000}";
        assert!(matches!(
            strip_forbidden_xml_1_0_chars(valid),
            Cow::Borrowed(_)
        ));

        assert_eq!(
            strip_forbidden_xml_1_0_chars("A\u{0}B\u{1c}C\u{fffe}D"),
            "ABCD"
        );
    }

    #[test]
    fn cow_normalization_preserves_valid_owned_storage() {
        let value = String::from("<svg><text>valid</text></svg>");
        let allocation = value.as_ptr();

        let normalized = strip_forbidden_xml_1_0_chars_cow(Cow::Owned(value));

        assert!(matches!(normalized, Cow::Owned(_)));
        assert_eq!(normalized.as_ptr(), allocation);
    }

    #[test]
    fn controlled_cow_normalization_stops_during_a_long_valid_span() {
        let value = "valid".repeat(1_024);
        let mut checkpoints = 0usize;

        let result =
            strip_forbidden_xml_1_0_chars_cow_with_checkpoints(Cow::Borrowed(&value), || {
                checkpoints += 1;
                if checkpoints == 2 {
                    Err("cancelled")
                } else {
                    Ok(())
                }
            });

        assert_eq!(result, Err("cancelled"));
    }

    #[test]
    fn html_entities_are_projected_to_xml_without_changing_text_semantics() {
        assert_eq!(
            normalize_html_entities_for_xml("known=&amp; html=&nbsp; unknown=&x41;"),
            "known=&amp; html=\u{a0} unknown=&amp;x41;"
        );
        assert_eq!(
            normalize_html_entities_for_xml("&#65; &#x41; &#X41; &#0;"),
            "&#65; &#x41; A \u{fffd}"
        );
        assert_eq!(
            normalize_html_entities_for_xml("&&x41; &&X41;"),
            "&amp;&amp;x41; &amp;&amp;X41;"
        );
    }

    #[test]
    fn sanitized_html_is_normalized_for_svg_xml_embedding() {
        assert_eq!(
            normalize_html_fragment_for_xhtml("<p>A<br><img src=\"x\"> 1 < 2 &amp;</p>"),
            "<p>A<br /><img src=\"x\" /> 1 &lt; 2 &amp;</p>"
        );
    }

    #[test]
    fn xhtml_normalization_preserves_greater_than_in_double_quoted_attributes() {
        let normalized =
            normalize_html_fragment_for_xhtml(r#"<img title="left > right" src="diagram.svg">"#);
        assert_eq!(
            normalized,
            r#"<img title="left > right" src="diagram.svg" />"#
        );

        let rooted = format!("<root>{normalized}</root>");
        let document =
            roxmltree::Document::parse(&rooted).expect("normalized XHTML must remain valid XML");
        let image = document
            .descendants()
            .find(|node| node.has_tag_name("img"))
            .expect("normalized fragment must contain the image");
        assert_eq!(image.attribute("title"), Some("left > right"));
        assert_eq!(image.attribute("src"), Some("diagram.svg"));
    }

    #[test]
    fn xhtml_normalization_preserves_greater_than_in_single_quoted_attributes() {
        let normalized =
            normalize_html_fragment_for_xhtml("<input data-rule='score > 10' value='ready'>");
        assert_eq!(normalized, "<input data-rule='score > 10' value='ready' />");

        let rooted = format!("<root>{normalized}</root>");
        let document =
            roxmltree::Document::parse(&rooted).expect("normalized XHTML must remain valid XML");
        let input = document
            .descendants()
            .find(|node| node.has_tag_name("input"))
            .expect("normalized fragment must contain the input");
        assert_eq!(input.attribute("data-rule"), Some("score > 10"));
        assert_eq!(input.attribute("value"), Some("ready"));
    }
}
