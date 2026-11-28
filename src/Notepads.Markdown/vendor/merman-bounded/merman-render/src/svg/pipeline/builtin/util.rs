const POSTPROCESS_CHECKPOINT_BATCH: usize = 64;

pub(crate) fn checkpoint_loop<E>(
    iteration: usize,
    checkpoint: &mut impl FnMut() -> Result<(), E>,
) -> Result<(), E> {
    if iteration.is_multiple_of(POSTPROCESS_CHECKPOINT_BATCH) {
        checkpoint()?;
    }
    Ok(())
}

fn pattern_prefix_table<E>(
    pattern: &[u8],
    checkpoint: &mut impl FnMut() -> Result<(), E>,
) -> Result<Vec<usize>, E> {
    let mut prefix = vec![0usize; pattern.len()];
    let mut matched = 0usize;
    for index in 1..pattern.len() {
        checkpoint_loop(index, checkpoint)?;
        while matched > 0 && pattern[index] != pattern[matched] {
            matched = prefix[matched - 1];
        }
        if pattern[index] == pattern[matched] {
            matched += 1;
            prefix[index] = matched;
        }
    }
    Ok(prefix)
}

fn find_pattern_with_checkpoints<E>(
    haystack: &str,
    needle: &str,
    find_last: bool,
    checkpoint: &mut impl FnMut() -> Result<(), E>,
) -> Result<Option<usize>, E> {
    if needle.is_empty() {
        checkpoint()?;
        return Ok(Some(if find_last { haystack.len() } else { 0 }));
    }

    let needle = needle.as_bytes();
    let prefix = pattern_prefix_table(needle, checkpoint)?;
    let mut matched = 0usize;
    let mut last_match = None;
    for (index, byte) in haystack.bytes().enumerate() {
        checkpoint_loop(index, checkpoint)?;
        while matched > 0 && byte != needle[matched] {
            matched = prefix[matched - 1];
        }
        if byte != needle[matched] {
            continue;
        }
        matched += 1;
        if matched != needle.len() {
            continue;
        }

        let start = index + 1 - needle.len();
        if !find_last {
            return Ok(Some(start));
        }
        last_match = Some(start);
        matched = prefix[matched - 1];
    }
    checkpoint()?;
    Ok(last_match)
}

/// Finds a UTF-8 substring in linear time while observing a cooperative checkpoint cadence.
pub(crate) fn find_with_checkpoints<E>(
    haystack: &str,
    needle: &str,
    checkpoint: &mut impl FnMut() -> Result<(), E>,
) -> Result<Option<usize>, E> {
    find_pattern_with_checkpoints(haystack, needle, false, checkpoint)
}

/// Finds the last UTF-8 substring in linear time while observing a cooperative checkpoint cadence.
pub(crate) fn rfind_with_checkpoints<E>(
    haystack: &str,
    needle: &str,
    checkpoint: &mut impl FnMut() -> Result<(), E>,
) -> Result<Option<usize>, E> {
    find_pattern_with_checkpoints(haystack, needle, true, checkpoint)
}

pub(crate) fn trim_with_checkpoints<'a, E>(
    value: &'a str,
    checkpoint: &mut impl FnMut() -> Result<(), E>,
) -> Result<&'a str, E> {
    let mut start = None;
    for (iteration, (index, character)) in value.char_indices().enumerate() {
        checkpoint_loop(iteration, checkpoint)?;
        if !character.is_whitespace() {
            start = Some(index);
            break;
        }
    }
    let Some(start) = start else {
        checkpoint()?;
        return Ok(&value[value.len()..]);
    };

    let mut end = value.len();
    for (iteration, (index, character)) in value[start..].char_indices().rev().enumerate() {
        checkpoint_loop(iteration, checkpoint)?;
        if !character.is_whitespace() {
            end = start + index + character.len_utf8();
            break;
        }
    }
    checkpoint()?;
    Ok(&value[start..end])
}

pub(crate) fn extract_exact_double_quoted_attr_with_checkpoints<'a, E>(
    tag: &'a str,
    name: &str,
    checkpoint: &mut impl FnMut() -> Result<(), E>,
) -> Result<Option<&'a str>, E> {
    let needle = format!(r#"{name}=""#);
    let Some(start) = find_with_checkpoints(tag, &needle, checkpoint)? else {
        return Ok(None);
    };
    let value_start = start + needle.len();
    let Some(value_end) = find_with_checkpoints(&tag[value_start..], "\"", checkpoint)? else {
        return Ok(None);
    };
    trim_with_checkpoints(&tag[value_start..value_start + value_end], checkpoint).map(Some)
}

pub(crate) fn find_tag_end_with_checkpoints<E>(
    input: &str,
    start: usize,
    checkpoint: &mut impl FnMut() -> Result<(), E>,
) -> Result<Option<usize>, E> {
    let Some(tail) = input.get(start..) else {
        checkpoint()?;
        return Ok(None);
    };
    let mut quote = None;
    for (iteration, (offset, character)) in tail.char_indices().enumerate() {
        checkpoint_loop(iteration, checkpoint)?;
        match character {
            '\'' | '"' if quote == Some(character) => quote = None,
            '\'' | '"' if quote.is_none() => quote = Some(character),
            '>' if quote.is_none() => return Ok(Some(start + offset)),
            _ => {}
        }
    }
    checkpoint()?;
    Ok(None)
}

fn find_declaration_end_with_checkpoints<E>(
    input: &str,
    start: usize,
    checkpoint: &mut impl FnMut() -> Result<(), E>,
) -> Result<Option<usize>, E> {
    let Some(tail) = input.get(start..) else {
        checkpoint()?;
        return Ok(None);
    };
    let mut quote = None;
    let mut subset_depth = 0usize;
    for (iteration, (offset, character)) in tail.char_indices().enumerate() {
        checkpoint_loop(iteration, checkpoint)?;
        match character {
            '\'' | '"' if quote == Some(character) => quote = None,
            '\'' | '"' if quote.is_none() => quote = Some(character),
            '[' if quote.is_none() => subset_depth = subset_depth.saturating_add(1),
            ']' if quote.is_none() => subset_depth = subset_depth.saturating_sub(1),
            '>' if quote.is_none() && subset_depth == 0 => return Ok(Some(start + offset)),
            _ => {}
        }
    }
    checkpoint()?;
    Ok(None)
}

fn find_delimited_markup_end_with_checkpoints<E>(
    input: &str,
    start: usize,
    prefix: &str,
    terminator: &str,
    checkpoint: &mut impl FnMut() -> Result<(), E>,
) -> Result<Option<usize>, E> {
    let Some(content) = input
        .get(start..)
        .and_then(|tail| tail.strip_prefix(prefix))
    else {
        return Ok(None);
    };
    find_with_checkpoints(content, terminator, checkpoint)
        .map(|end| end.map(|offset| start + prefix.len() + offset + terminator.len() - 1))
}

fn find_markup_end_with_checkpoints<E>(
    input: &str,
    start: usize,
    checkpoint: &mut impl FnMut() -> Result<(), E>,
) -> Result<Option<usize>, E> {
    if input
        .get(start..)
        .is_some_and(|tail| tail.starts_with("<!--"))
    {
        return find_delimited_markup_end_with_checkpoints(input, start, "<!--", "-->", checkpoint);
    }
    if input
        .get(start..)
        .is_some_and(|tail| tail.starts_with("<![CDATA["))
    {
        return find_delimited_markup_end_with_checkpoints(
            input,
            start,
            "<![CDATA[",
            "]]>",
            checkpoint,
        );
    }
    if input
        .get(start..)
        .is_some_and(|tail| tail.starts_with("<?"))
    {
        return find_delimited_markup_end_with_checkpoints(input, start, "<?", "?>", checkpoint);
    }
    if input
        .get(start..)
        .is_some_and(|tail| tail.starts_with("<!"))
    {
        return find_declaration_end_with_checkpoints(input, start, checkpoint);
    }
    find_tag_end_with_checkpoints(input, start, checkpoint)
}

pub(crate) use crate::svg::scanner::find_tag_end;

#[derive(Debug, Clone, Copy)]
pub(crate) struct SvgTag<'a> {
    source: &'a str,
    start: usize,
    end: usize,
}

impl<'a> SvgTag<'a> {
    pub(crate) fn raw(self) -> &'a str {
        &self.source[self.start..=self.end]
    }

    pub(crate) fn start(self) -> usize {
        self.start
    }

    pub(crate) fn is_self_closing(self) -> bool {
        self.raw().trim_end().ends_with("/>")
    }
}

pub(crate) struct SvgTagScanner<'a> {
    source: &'a str,
    cursor: usize,
}

impl<'a> SvgTagScanner<'a> {
    pub(crate) fn new(source: &'a str) -> Self {
        Self { source, cursor: 0 }
    }

    pub(crate) fn cursor(&self) -> usize {
        self.cursor
    }

    pub(crate) fn skip_to(&mut self, cursor: usize) {
        self.cursor = cursor.min(self.source.len());
    }

    #[cfg(test)]
    pub(crate) fn next(&mut self) -> Option<SvgTag<'a>> {
        let mut checkpoint = || Ok::<(), std::convert::Infallible>(());
        match self.next_with_checkpoints(&mut checkpoint) {
            Ok(tag) => tag,
            Err(error) => match error {},
        }
    }

    pub(crate) fn next_with_checkpoints<E>(
        &mut self,
        checkpoint: &mut impl FnMut() -> Result<(), E>,
    ) -> Result<Option<SvgTag<'a>>, E> {
        let Some(rel_start) = find_with_checkpoints(&self.source[self.cursor..], "<", checkpoint)?
        else {
            return Ok(None);
        };
        let start = self.cursor + rel_start;
        let Some(end) = find_markup_end_with_checkpoints(self.source, start, checkpoint)? else {
            self.cursor = start;
            return Ok(None);
        };
        self.cursor = end + 1;
        Ok(Some(SvgTag {
            source: self.source,
            start,
            end,
        }))
    }
}

#[derive(Debug, Clone, Copy)]
pub(crate) struct SvgQuotedAttr {
    pub(crate) full_start: usize,
    pub(crate) full_end: usize,
    pub(crate) name_start: usize,
    pub(crate) name_end: usize,
    pub(crate) value_start: usize,
    pub(crate) value_end: usize,
}

pub(crate) fn next_svg_quoted_attr(tag: &str, from: usize) -> Option<SvgQuotedAttr> {
    let mut cursor = from;
    while cursor < tag.len() {
        let ch = tag.get(cursor..)?.chars().next()?;
        if ch.is_whitespace() {
            let full_start = cursor;
            let name_start = skip_svg_attr_whitespace(tag, cursor);
            if let Some(attr_match) = svg_quoted_attr_at(tag, full_start, name_start) {
                return Some(attr_match);
            }
            cursor = name_start;
        } else {
            cursor += ch.len_utf8();
        }
    }
    None
}

pub(crate) fn next_svg_quoted_attr_with_checkpoints<E>(
    tag: &str,
    from: usize,
    checkpoint: &mut impl FnMut() -> Result<(), E>,
) -> Result<Option<SvgQuotedAttr>, E> {
    let mut cursor = from;
    let mut iteration = 0usize;
    while cursor < tag.len() {
        checkpoint_loop(iteration, checkpoint)?;
        iteration = iteration.saturating_add(1);
        let Some(ch) = tag.get(cursor..).and_then(|tail| tail.chars().next()) else {
            return Ok(None);
        };
        if ch.is_whitespace() {
            let full_start = cursor;
            let name_start = skip_svg_attr_whitespace_with_checkpoints(tag, cursor, checkpoint)?;
            if let Some(attr_match) =
                svg_quoted_attr_at_with_checkpoints(tag, full_start, name_start, checkpoint)?
            {
                return Ok(Some(attr_match));
            }
            cursor = name_start;
        } else {
            cursor += ch.len_utf8();
        }
    }
    checkpoint()?;
    Ok(None)
}

pub(crate) fn start_tag_name(tag: &str) -> Option<&str> {
    let tag = tag.trim_start();
    if !tag.starts_with('<')
        || tag.starts_with("</")
        || tag.starts_with("<!--")
        || tag.starts_with("<!")
        || tag.starts_with("<?")
    {
        return None;
    }

    let start = 1;
    let end = start
        + tag[start..]
            .find(|ch: char| ch.is_whitespace() || ch == '/' || ch == '>')
            .unwrap_or(tag.len() - start);
    (start < end).then_some(&tag[start..end])
}

pub(crate) fn end_tag_name(tag: &str) -> Option<&str> {
    let tag = tag.trim_start().strip_prefix("</")?;
    let end = tag
        .find(|ch: char| ch.is_whitespace() || ch == '>')
        .unwrap_or(tag.len());
    (end > 0).then_some(&tag[..end])
}

fn svg_quoted_attr_at(tag: &str, full_start: usize, name_start: usize) -> Option<SvgQuotedAttr> {
    let first = *tag.as_bytes().get(name_start)?;
    if !is_svg_attr_name_start_byte(first) {
        return None;
    }

    let name_end = consume_svg_attr_name(tag, name_start);
    let mut cursor = skip_svg_attr_whitespace(tag, name_end);
    if !tag.get(cursor..)?.starts_with('=') {
        return None;
    }
    cursor += 1;
    cursor = skip_svg_attr_whitespace(tag, cursor);

    let quote = tag.get(cursor..)?.chars().next()?;
    if !matches!(quote, '"' | '\'') {
        return None;
    }

    let value_start = cursor + quote.len_utf8();
    let value_end = value_start + tag.get(value_start..)?.find(quote)?;
    Some(SvgQuotedAttr {
        full_start,
        full_end: value_end + quote.len_utf8(),
        name_start,
        name_end,
        value_start,
        value_end,
    })
}

fn svg_quoted_attr_at_with_checkpoints<E>(
    tag: &str,
    full_start: usize,
    name_start: usize,
    checkpoint: &mut impl FnMut() -> Result<(), E>,
) -> Result<Option<SvgQuotedAttr>, E> {
    let Some(first) = tag.as_bytes().get(name_start).copied() else {
        return Ok(None);
    };
    if !is_svg_attr_name_start_byte(first) {
        return Ok(None);
    }

    let name_end = consume_svg_attr_name_with_checkpoints(tag, name_start, checkpoint)?;
    let mut cursor = skip_svg_attr_whitespace_with_checkpoints(tag, name_end, checkpoint)?;
    if !tag.get(cursor..).is_some_and(|tail| tail.starts_with('=')) {
        return Ok(None);
    }
    cursor += 1;
    cursor = skip_svg_attr_whitespace_with_checkpoints(tag, cursor, checkpoint)?;

    let Some(quote) = tag.get(cursor..).and_then(|tail| tail.chars().next()) else {
        return Ok(None);
    };
    if !matches!(quote, '"' | '\'') {
        return Ok(None);
    }

    let value_start = cursor + quote.len_utf8();
    let quote = if quote == '"' { "\"" } else { "'" };
    let Some(relative_end) = find_with_checkpoints(
        tag.get(value_start..).unwrap_or_default(),
        quote,
        checkpoint,
    )?
    else {
        return Ok(None);
    };
    let value_end = value_start + relative_end;
    Ok(Some(SvgQuotedAttr {
        full_start,
        full_end: value_end + quote.len(),
        name_start,
        name_end,
        value_start,
        value_end,
    }))
}

fn skip_svg_attr_whitespace(tag: &str, mut cursor: usize) -> usize {
    while let Some(ch) = tag.get(cursor..).and_then(|tail| tail.chars().next()) {
        if !ch.is_whitespace() {
            break;
        }
        cursor += ch.len_utf8();
    }
    cursor
}

fn skip_svg_attr_whitespace_with_checkpoints<E>(
    tag: &str,
    mut cursor: usize,
    checkpoint: &mut impl FnMut() -> Result<(), E>,
) -> Result<usize, E> {
    let mut iteration = 0usize;
    while let Some(ch) = tag.get(cursor..).and_then(|tail| tail.chars().next()) {
        checkpoint_loop(iteration, checkpoint)?;
        iteration = iteration.saturating_add(1);
        if !ch.is_whitespace() {
            break;
        }
        cursor += ch.len_utf8();
    }
    Ok(cursor)
}

fn consume_svg_attr_name(tag: &str, mut cursor: usize) -> usize {
    while let Some(b) = tag.as_bytes().get(cursor) {
        if !is_svg_attr_name_continue_byte(*b) {
            break;
        }
        cursor += 1;
    }
    cursor
}

fn consume_svg_attr_name_with_checkpoints<E>(
    tag: &str,
    mut cursor: usize,
    checkpoint: &mut impl FnMut() -> Result<(), E>,
) -> Result<usize, E> {
    let mut iteration = 0usize;
    while let Some(byte) = tag.as_bytes().get(cursor) {
        checkpoint_loop(iteration, checkpoint)?;
        iteration = iteration.saturating_add(1);
        if !is_svg_attr_name_continue_byte(*byte) {
            break;
        }
        cursor += 1;
    }
    Ok(cursor)
}

fn is_svg_attr_name_start_byte(b: u8) -> bool {
    b.is_ascii_alphabetic() || matches!(b, b'_' | b':')
}

fn is_svg_attr_name_continue_byte(b: u8) -> bool {
    b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_' | b':' | b'.')
}

pub(crate) fn extract_quoted_attr<'a>(tag: &'a str, name: &str) -> Option<&'a str> {
    let (start, end) = find_quoted_attr_value_span(tag, name)?;
    Some(tag[start..end].trim())
}

pub(crate) fn find_quoted_attr_value_span(tag: &str, name: &str) -> Option<(usize, usize)> {
    let mut cursor = 0usize;
    while let Some(attr) = next_svg_quoted_attr(tag, cursor) {
        if tag[attr.name_start..attr.name_end].eq_ignore_ascii_case(name) {
            return Some((attr.value_start, attr.value_end));
        }
        cursor = attr.full_end;
    }
    None
}

pub(crate) fn set_or_insert_quoted_attr(tag: &str, name: &str, value: &str) -> String {
    if let Some((value_start, value_end)) = find_quoted_attr_value_span(tag, name) {
        let mut out = String::with_capacity(tag.len() + value.len());
        out.push_str(&tag[..value_start]);
        out.push_str(value);
        out.push_str(&tag[value_end..]);
        return out;
    }

    let insert_at = tag
        .trim_end()
        .strip_suffix("/>")
        .map(|prefix| prefix.trim_end().len())
        .unwrap_or_else(|| tag.rfind('>').unwrap_or(tag.len()));
    let mut out = String::with_capacity(tag.len() + name.len() + value.len() + 4);
    out.push_str(&tag[..insert_at]);
    out.push(' ');
    out.push_str(name);
    out.push_str(r#"=""#);
    out.push_str(value);
    out.push('"');
    out.push_str(&tag[insert_at..]);
    out
}

pub(crate) fn escape_xml_attr(value: &str) -> String {
    let mut checkpoint = || Ok::<(), std::convert::Infallible>(());
    match escape_xml_attr_with_checkpoints(value, &mut checkpoint) {
        Ok(value) => value,
        Err(error) => match error {},
    }
}

pub(crate) fn escape_xml_text_with_checkpoints<E>(
    value: &str,
    checkpoint: &mut impl FnMut() -> Result<(), E>,
) -> Result<String, E> {
    let mut out = String::with_capacity(value.len());
    for (iteration, ch) in value.chars().enumerate() {
        checkpoint_loop(iteration, checkpoint)?;
        if !crate::xml::is_xml_1_0_char(ch) {
            continue;
        }
        match ch {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            _ => out.push(ch),
        }
    }
    checkpoint()?;
    Ok(out)
}

pub(crate) fn escape_xml_attr_with_checkpoints<E>(
    value: &str,
    checkpoint: &mut impl FnMut() -> Result<(), E>,
) -> Result<String, E> {
    let mut out = String::with_capacity(value.len());
    for (iteration, ch) in value.chars().enumerate() {
        checkpoint_loop(iteration, checkpoint)?;
        if !crate::xml::is_xml_1_0_char(ch) {
            continue;
        }
        match ch {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&apos;"),
            _ => out.push(ch),
        }
    }
    checkpoint()?;
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn quoted_attr_helpers_handle_spacing_case_and_quote_style() {
        let tag = r#"<path DATA-note = 'keep' style = "fill:red" />"#;

        assert_eq!(extract_quoted_attr(tag, "data-note"), Some("keep"));
        assert_eq!(extract_quoted_attr(tag, "STYLE"), Some("fill:red"));

        let rewritten = set_or_insert_quoted_attr(tag, "style", "stroke:blue");
        assert!(
            rewritten.contains(r#"style = "stroke:blue""#),
            "{rewritten}"
        );

        let inserted = set_or_insert_quoted_attr(tag, "x", "10");
        assert!(inserted.contains(r#" x="10" />"#), "{inserted}");
    }

    #[test]
    fn svg_tag_scanner_reports_tag_spans_and_names() {
        let svg = r#"<svg><g class="a > b"><rect width="1"/></g><!-- > <style>x</style> --><![CDATA[</style>]]></svg>"#;
        let mut scanner = SvgTagScanner::new(svg);

        let svg_tag = scanner.next().unwrap();
        assert_eq!(svg_tag.raw(), "<svg>");
        assert_eq!(svg_tag.start(), 0);
        assert!(!svg_tag.is_self_closing());

        let g_tag = scanner.next().unwrap();
        assert_eq!(g_tag.raw(), r#"<g class="a > b">"#);

        let rect_tag = scanner.next().unwrap();
        assert!(rect_tag.is_self_closing());

        let g_close = scanner.next().unwrap();
        assert_eq!(g_close.raw(), "</g>");

        let comment = scanner.next().unwrap();
        assert_eq!(comment.raw(), "<!-- > <style>x</style> -->");

        let cdata = scanner.next().unwrap();
        assert_eq!(cdata.raw(), "<![CDATA[</style>]]>");
    }
}
