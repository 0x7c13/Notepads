/// One rule dispatch. A skipped rule must consume input or leave its lexical state.
/// Empty tokens are valid when they drain pending output or complete a state transition.
pub(crate) enum ScanStep<T> {
    Emit(T),
    Skip,
    End,
}

pub(crate) trait TokenScanner {
    type Event;

    fn scan_step(&mut self) -> ScanStep<Self::Event>;

    fn next_token(&mut self) -> Option<Self::Event> {
        loop {
            match self.scan_step() {
                ScanStep::Emit(event) => return Some(event),
                ScanStep::Skip => {}
                ScanStep::End => return None,
            }
        }
    }
}

pub(crate) fn collect_lexical_events<S: TokenScanner>(
    scanner: &mut S,
    control: &crate::OperationControl,
) -> crate::OperationControlResult<Vec<S::Event>> {
    let mut events = Vec::new();
    let mut steps = 0usize;
    loop {
        if steps.is_multiple_of(128) {
            control.checkpoint()?;
        }
        match scanner.scan_step() {
            ScanStep::Emit(event) => events.push(event),
            ScanStep::Skip => {}
            ScanStep::End => break,
        }
        steps = steps.wrapping_add(1);
    }
    control.checkpoint()?;
    Ok(events)
}

pub(crate) fn strip_line_ending(segment: &str) -> &str {
    let segment = segment.strip_suffix('\n').unwrap_or(segment);
    segment.strip_suffix('\r').unwrap_or(segment)
}

pub(crate) fn consume_line_ending(source: &str, offset: usize) -> Option<usize> {
    match source.as_bytes().get(offset).copied()? {
        b'\r' if source.as_bytes().get(offset + 1) == Some(&b'\n') => Some(offset + 2),
        b'\r' | b'\n' => Some(offset + 1),
        _ => None,
    }
}

pub(crate) struct LineCursor<'a> {
    source: &'a str,
    segments: std::str::SplitInclusive<'a, char>,
    same_line_remainder: Option<(usize, usize)>,
    offset: usize,
}

impl<'a> LineCursor<'a> {
    pub(crate) fn new(code: &'a str) -> Self {
        Self {
            source: code,
            segments: code.split_inclusive('\n'),
            same_line_remainder: None,
            offset: 0,
        }
    }

    pub(crate) fn next_line(&mut self) -> Option<(&'a str, usize)> {
        if let Some((start, end)) = self.same_line_remainder.take() {
            return Some((strip_line_ending(&self.source[start..end]), start));
        }
        let segment = self.segments.next()?;
        let line_start = self.offset;
        self.offset += segment.len();
        Some((strip_line_ending(segment), line_start))
    }

    /// Replays the unconsumed suffix of the current physical line before advancing to the next.
    ///
    /// Stateful lexers resume their default mode immediately after a closing delimiter. Manual
    /// line parsers use this operation to preserve the same source-backed behavior without
    /// rescanning or manufacturing a synthetic line.
    pub(crate) fn resume_same_line_at(&mut self, start: usize) {
        debug_assert!(self.same_line_remainder.is_none());
        debug_assert!(start <= self.source.len());
        debug_assert!(self.source.is_char_boundary(start));
        let end = self.source[start..]
            .find('\n')
            .map_or(self.source.len(), |relative| start + relative);
        self.same_line_remainder = Some((start, end));
    }

    pub(crate) fn offset(&self) -> usize {
        self.offset
    }
}

pub(crate) fn physical_line_at(source: &str, offset: usize) -> (&str, usize) {
    let rest = &source[offset..];
    let (segment, next_offset) = match rest.find('\n') {
        Some(relative_newline) => (&rest[..=relative_newline], offset + relative_newline + 1),
        None => (rest, source.len()),
    };
    (strip_line_ending(segment), next_offset)
}

pub(crate) fn starts_with_case_insensitive(haystack: &str, needle: &str) -> bool {
    if haystack.len() < needle.len() {
        return false;
    }
    haystack
        .as_bytes()
        .iter()
        .take(needle.len())
        .copied()
        .map(|b| b.to_ascii_lowercase())
        .eq(needle
            .as_bytes()
            .iter()
            .copied()
            .map(|b| b.to_ascii_lowercase()))
}

pub(crate) fn split_indent_by<F>(line: &str, mut is_indent_char: F) -> (usize, &str)
where
    F: FnMut(char) -> bool,
{
    let mut indent_chars = 0usize;
    let mut byte_idx = line.len();
    for (idx, ch) in line.char_indices() {
        if is_indent_char(ch) {
            indent_chars += 1;
            continue;
        }
        byte_idx = idx;
        break;
    }
    if indent_chars == 0 {
        byte_idx = 0;
    } else if byte_idx == line.len() {
        byte_idx = line.len();
    }
    (indent_chars, &line[byte_idx..])
}

pub(crate) fn split_indent(line: &str) -> (usize, &str) {
    split_indent_by(line, char::is_whitespace)
}

pub(crate) fn split_ascii_indent(line: &str) -> (usize, &str) {
    split_indent_by(line, |ch| matches!(ch, ' ' | '\t'))
}

pub(crate) fn leading_whitespace_len(s: &str) -> usize {
    s.chars()
        .take_while(|ch| ch.is_whitespace())
        .map(char::len_utf8)
        .sum()
}

/// Returns whether a character belongs to JavaScript's ECMAScript whitespace set.
///
/// Mermaid's Jison lexers use `\s`, whose behavior differs from Rust's `char::is_whitespace`
/// for U+FEFF and a few control characters. Keep this predicate source-compatible with the
/// JavaScript lexer while advancing UTF-8 cursors by complete characters.
pub(crate) fn is_ecmascript_whitespace(ch: char) -> bool {
    matches!(
        ch,
        '\u{0009}'
            | '\u{000A}'
            | '\u{000B}'
            | '\u{000C}'
            | '\u{000D}'
            | '\u{0020}'
            | '\u{00A0}'
            | '\u{1680}'
            | '\u{2000}'
            ..='\u{200A}'
                | '\u{2028}'
                | '\u{2029}'
                | '\u{202F}'
                | '\u{205F}'
                | '\u{3000}'
                | '\u{FEFF}'
    )
}

pub(crate) fn is_ecmascript_inline_whitespace(ch: char) -> bool {
    is_ecmascript_whitespace(ch) && !matches!(ch, '\r' | '\n')
}

pub(crate) fn split_statement_suffix_hash_or_semi(s: &str) -> &str {
    let mut end = s.len();
    for (i, c) in s.char_indices() {
        if c == '#' || c == ';' {
            end = i;
            break;
        }
    }
    &s[..end]
}

#[cfg(test)]
mod tests {
    use super::{
        LineCursor, consume_line_ending, leading_whitespace_len, physical_line_at,
        split_ascii_indent, split_indent, split_indent_by, split_statement_suffix_hash_or_semi,
        starts_with_case_insensitive, strip_line_ending,
    };

    #[test]
    fn strip_line_ending_removes_lf_and_crlf() {
        assert_eq!(strip_line_ending("line\n"), "line");
        assert_eq!(strip_line_ending("line\r\n"), "line");
        assert_eq!(strip_line_ending("line"), "line");
    }

    #[test]
    fn consume_line_ending_accepts_cr_lf_and_crlf() {
        assert_eq!(consume_line_ending("\r", 0), Some(1));
        assert_eq!(consume_line_ending("\n", 0), Some(1));
        assert_eq!(consume_line_ending("\r\n", 0), Some(2));
        assert_eq!(consume_line_ending("x", 0), None);
    }

    #[test]
    fn line_cursor_tracks_utf8_byte_offsets_and_strips_endings() {
        let mut cursor = LineCursor::new("alpha\r\n\u{03b2}eta");

        assert_eq!(cursor.next_line(), Some(("alpha", 0)));
        assert_eq!(cursor.offset(), "alpha\r\n".len());
        assert_eq!(cursor.next_line(), Some(("\u{03b2}eta", "alpha\r\n".len())));
        assert_eq!(cursor.offset(), "alpha\r\n\u{03b2}eta".len());
        assert_eq!(cursor.next_line(), None);
    }

    #[test]
    fn line_cursor_replays_a_utf8_same_line_remainder_before_the_next_line() {
        let source = "first } \u{03b2}eta\r\nnext";
        let mut cursor = LineCursor::new(source);

        assert_eq!(cursor.next_line(), Some(("first } \u{03b2}eta", 0)));
        let remainder_start = source.find('\u{03b2}').unwrap();
        cursor.resume_same_line_at(remainder_start);

        assert_eq!(cursor.next_line(), Some(("\u{03b2}eta", remainder_start)));
        assert_eq!(
            cursor.next_line(),
            Some(("next", "first } \u{03b2}eta\r\n".len()))
        );
        assert_eq!(cursor.next_line(), None);
    }

    #[test]
    fn physical_line_at_tracks_next_offset_and_strips_endings() {
        let source = "first\r\nsecond\r";
        let second_start = "first\r\n".len();

        assert_eq!(physical_line_at(source, 0), ("first", second_start));
        assert_eq!(
            physical_line_at(source, second_start),
            ("second", source.len())
        );
        assert_eq!(physical_line_at(source, source.len()), ("", source.len()));
    }

    #[test]
    fn starts_with_case_insensitive_handles_ascii_prefixes() {
        assert!(starts_with_case_insensitive("MindMap", "mindmap"));
        assert!(!starts_with_case_insensitive("diagram", "mindmap"));
    }

    #[test]
    fn split_indent_counts_leading_whitespace() {
        let (indent, rest) = split_indent(" \troot");
        assert_eq!(indent, 2);
        assert_eq!(rest, "root");
    }

    #[test]
    fn split_ascii_indent_counts_only_spaces_and_tabs() {
        let (indent, rest) = split_ascii_indent(" \t\u{00A0}root");
        assert_eq!(indent, 2);
        assert_eq!(rest, "\u{00A0}root");
    }

    #[test]
    fn split_indent_by_honors_custom_predicate() {
        let (indent, rest) = split_indent_by(" \troot", |ch| ch == ' ' || ch == '\t');
        assert_eq!(indent, 2);
        assert_eq!(rest, "root");
    }

    #[test]
    fn leading_whitespace_len_tracks_utf8_width() {
        assert_eq!(leading_whitespace_len(" \troot"), 2);
    }

    #[test]
    fn split_statement_suffix_hash_or_semi_stops_before_comment_markers() {
        assert_eq!(split_statement_suffix_hash_or_semi("task # note"), "task ");
        assert_eq!(split_statement_suffix_hash_or_semi("task; note"), "task");
        assert_eq!(split_statement_suffix_hash_or_semi("task"), "task");
    }
}
