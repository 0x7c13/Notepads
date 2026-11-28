use crate::{
    EditorExpectedSyntax, EditorExpectedSyntaxKind, EditorSemanticFacts, EditorSemanticKind,
    EditorSemanticSymbol, SourceSpan,
};
#[cfg(test)]
use std::{cell::RefCell, collections::BTreeMap};

#[cfg(test)]
thread_local! {
    static FAMILY_SYNTAX_CONSTRUCTION_COUNTS: RefCell<BTreeMap<&'static str, usize>> =
        const { RefCell::new(BTreeMap::new()) };
}

#[cfg(test)]
pub(crate) fn record_family_syntax_construction(family: &'static str) {
    FAMILY_SYNTAX_CONSTRUCTION_COUNTS.with_borrow_mut(|counts| {
        *counts.entry(family).or_default() += 1;
    });
}

#[cfg(test)]
pub(crate) fn reset_family_syntax_construction_count(family: &'static str) {
    FAMILY_SYNTAX_CONSTRUCTION_COUNTS.with_borrow_mut(|counts| {
        counts.remove(family);
    });
}

#[cfg(test)]
pub(crate) fn family_syntax_construction_count(family: &'static str) -> usize {
    FAMILY_SYNTAX_CONSTRUCTION_COUNTS
        .with_borrow(|counts| counts.get(family).copied().unwrap_or_default())
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct LangiumString {
    pub(crate) value: String,
    pub(crate) raw_span: SourceSpan,
    pub(crate) value_span: SourceSpan,
    pub(crate) consumed: usize,
}

pub(crate) fn is_ecmascript_line_terminator(ch: char) -> bool {
    matches!(ch, '\n' | '\r' | '\u{2028}' | '\u{2029}')
}

/// Parses the shared Langium `STRING` terminal and applies `DefaultValueConverter` unescaping.
pub(crate) fn parse_langium_string(input: &str, input_start: usize) -> Option<LangiumString> {
    let mut chars = input.char_indices();
    let (_, quote) = chars.next()?;
    if !matches!(quote, '"' | '\'') {
        return None;
    }

    let mut value = String::new();
    let mut escaped = false;
    for (index, ch) in chars {
        if escaped {
            if is_ecmascript_line_terminator(ch) {
                return None;
            }
            value.push(ch);
            escaped = false;
            continue;
        }
        if ch == '\\' {
            escaped = true;
            continue;
        }
        if ch == quote {
            let consumed = index + ch.len_utf8();
            return Some(LangiumString {
                value,
                raw_span: SourceSpan::new(input_start, input_start + consumed),
                value_span: SourceSpan::new(input_start + quote.len_utf8(), input_start + index),
                consumed,
            });
        }
        value.push(ch);
    }
    None
}

/// Returns the visible prefix before a Langium `SINGLE_LINE_COMMENT` outside a STRING token.
pub(crate) fn strip_langium_inline_comment(line: &str) -> &str {
    let mut quote: Option<char> = None;
    let mut escaped = false;
    let mut chars = line.char_indices().peekable();
    while let Some((index, ch)) = chars.next() {
        if escaped {
            escaped = false;
            continue;
        }
        if quote.is_some() && ch == '\\' {
            escaped = true;
            continue;
        }
        if matches!(ch, '"' | '\'') {
            match quote {
                Some(open) if open == ch => quote = None,
                None => quote = Some(ch),
                _ => {}
            }
            continue;
        }
        if quote.is_none() && ch == '%' && chars.peek().is_some_and(|(_, next)| *next == '%') {
            return &line[..index];
        }
    }
    line
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum LangiumCommonField {
    Title,
    AccTitle,
    AccDescr,
}

impl LangiumCommonField {
    pub(crate) fn directive(self) -> &'static str {
        match self {
            Self::Title => "title",
            Self::AccTitle => "accTitle",
            Self::AccDescr => "accDescr",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct LangiumCommonFact {
    pub(crate) field: LangiumCommonField,
    pub(crate) value: String,
    /// Span of the complete common terminal, including its leading horizontal whitespace.
    pub(crate) raw_span: SourceSpan,
    /// Span of the unnormalized payload after trimming its outer whitespace.
    pub(crate) value_span: SourceSpan,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct LangiumCommonDiagnostic {
    pub(crate) message: String,
    pub(crate) span: SourceSpan,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct LangiumCommonParse {
    pub(crate) fact: LangiumCommonFact,
    /// Bytes consumed from the supplied offset, including the required EOL or EOF.
    pub(crate) consumed: usize,
    /// Last whitespace-only physical line consumed as trailing EOL trivia, if any.
    pub(crate) trailing_whitespace_span: Option<SourceSpan>,
    pub(crate) diagnostic: Option<LangiumCommonDiagnostic>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) struct LangiumCommonFacts {
    ordered: Vec<LangiumCommonFact>,
}

impl LangiumCommonFacts {
    pub(crate) fn push(&mut self, fact: LangiumCommonFact) {
        self.ordered.push(fact);
    }

    pub(crate) fn iter(&self) -> impl Iterator<Item = &LangiumCommonFact> {
        self.ordered.iter()
    }

    #[cfg(test)]
    fn ordered(&self) -> &[LangiumCommonFact] {
        &self.ordered
    }

    pub(crate) fn last(&self, field: LangiumCommonField) -> Option<&LangiumCommonFact> {
        self.ordered.iter().rev().find(|fact| fact.field == field)
    }
}

pub(crate) fn push_langium_common_editor_fact(
    facts: &mut EditorSemanticFacts,
    fact: &LangiumCommonFact,
    family_name: &str,
) {
    facts.push_directive_prefix(fact.field.directive());
    if fact.value.is_empty() {
        return;
    }

    let detail = match fact.field {
        LangiumCommonField::Title => format!("{family_name} title"),
        LangiumCommonField::AccTitle => format!("{family_name} accessibility title"),
        LangiumCommonField::AccDescr => format!("{family_name} accessibility description"),
    };
    facts.push_expected_syntax(EditorExpectedSyntax::new(
        EditorExpectedSyntaxKind::Payload,
        fact.value_span,
    ));
    facts.push_symbol(EditorSemanticSymbol::payload(
        fact.value.clone(),
        Some(detail),
        EditorSemanticKind::String,
        fact.raw_span,
        fact.value_span,
    ));
}

pub(crate) fn push_langium_common_recovery(
    facts: &mut EditorSemanticFacts,
    diagnostic: &LangiumCommonDiagnostic,
) {
    facts.mark_recovered_from_parse_error(&diagnostic.message, Some(diagnostic.span));
}

/// Parses one `TitleAndAccessibilities` terminal at `offset` in the complete parser input.
///
/// A successful result consumes the terminal and its required `EOL` (`NEWLINE+ | EOF`). An
/// unterminated block `accDescr` is returned as a recovered fact so editor projections can retain
/// its prefix and partial payload; strict family parsers must reject results with a diagnostic.
pub(crate) fn parse_langium_common(source: &str, offset: usize) -> Option<LangiumCommonParse> {
    let input = source.get(offset..)?;
    let leading = horizontal_whitespace_len(input);
    let token_start = offset + leading;
    let token = source.get(token_start..)?;

    if token.starts_with("accDescr") {
        return parse_acc_descr(source, offset, token_start);
    }
    if token.starts_with("accTitle") {
        return parse_acc_title(source, offset, token_start);
    }
    if token.starts_with("title") {
        return parse_title(source, offset, token_start);
    }
    None
}

fn parse_title(source: &str, offset: usize, token_start: usize) -> Option<LangiumCommonParse> {
    let keyword_end = token_start + "title".len();
    let next = source.get(keyword_end..)?.chars().next();
    let capture_start = keyword_end;
    let raw_end = if next.is_some_and(is_horizontal_whitespace) {
        inline_terminal_end(source, capture_start)
    } else {
        keyword_end
    };
    parsed_inline(
        source,
        offset,
        raw_end,
        LangiumCommonField::Title,
        capture_start,
    )
}

fn parse_acc_title(source: &str, offset: usize, token_start: usize) -> Option<LangiumCommonParse> {
    let keyword_end = token_start + "accTitle".len();
    let colon = keyword_end + horizontal_whitespace_len(source.get(keyword_end..)?);
    if source.as_bytes().get(colon) != Some(&b':') {
        return None;
    }
    let capture_start = colon + 1;
    let raw_end = inline_terminal_end(source, capture_start);
    parsed_inline(
        source,
        offset,
        raw_end,
        LangiumCommonField::AccTitle,
        capture_start,
    )
}

fn parse_acc_descr(source: &str, offset: usize, token_start: usize) -> Option<LangiumCommonParse> {
    let keyword_end = token_start + "accDescr".len();
    let horizontal_end = keyword_end + horizontal_whitespace_len(source.get(keyword_end..)?);
    if source.as_bytes().get(horizontal_end) == Some(&b':') {
        let capture_start = horizontal_end + 1;
        let raw_end = inline_terminal_end(source, capture_start);
        return parsed_inline(
            source,
            offset,
            raw_end,
            LangiumCommonField::AccDescr,
            capture_start,
        );
    }

    let opening = skip_javascript_whitespace(source, keyword_end);
    if source.as_bytes().get(opening) != Some(&b'{') {
        return None;
    }
    let content_start = opening + 1;
    let Some(relative_close) = source.get(content_start..)?.find('}') else {
        let raw_end = source.len();
        let (value, value_span) = normalized_block_value(source, content_start, raw_end);
        return Some(LangiumCommonParse {
            fact: LangiumCommonFact {
                field: LangiumCommonField::AccDescr,
                value,
                raw_span: SourceSpan::new(offset, raw_end),
                value_span,
            },
            consumed: raw_end - offset,
            trailing_whitespace_span: None,
            diagnostic: Some(LangiumCommonDiagnostic {
                message: "unterminated accDescr block; expected `}`".to_string(),
                span: SourceSpan::new(raw_end, raw_end),
            }),
        });
    };

    let close = content_start + relative_close;
    let raw_end = close + 1;
    let eol = consume_required_eol(source, raw_end)?;
    let (value, value_span) = normalized_block_value(source, content_start, close);
    Some(LangiumCommonParse {
        fact: LangiumCommonFact {
            field: LangiumCommonField::AccDescr,
            value,
            raw_span: SourceSpan::new(offset, raw_end),
            value_span,
        },
        consumed: eol.end - offset,
        trailing_whitespace_span: eol.trailing_whitespace_span,
        diagnostic: None,
    })
}

fn parsed_inline(
    source: &str,
    offset: usize,
    raw_end: usize,
    field: LangiumCommonField,
    capture_start: usize,
) -> Option<LangiumCommonParse> {
    let eol = consume_required_eol(source, raw_end)?;
    let capture = source.get(capture_start..raw_end)?;
    let trimmed = capture.trim();
    let leading = capture.len() - capture.trim_start().len();
    let value_start = capture_start + leading;
    let value_span = SourceSpan::new(value_start, value_start + trimmed.len());
    Some(LangiumCommonParse {
        fact: LangiumCommonFact {
            field,
            value: collapse_horizontal_runs(trimmed),
            raw_span: SourceSpan::new(offset, raw_end),
            value_span,
        },
        consumed: eol.end - offset,
        trailing_whitespace_span: eol.trailing_whitespace_span,
        diagnostic: None,
    })
}

fn inline_terminal_end(source: &str, start: usize) -> usize {
    let bytes = source.as_bytes();
    let mut cursor = start;
    while cursor < bytes.len() {
        if matches!(bytes[cursor], b'\n' | b'\r') || bytes.get(cursor..cursor + 2) == Some(b"%%") {
            break;
        }
        cursor += 1;
    }
    cursor
}

struct ConsumedEol {
    end: usize,
    trailing_whitespace_span: Option<SourceSpan>,
}

fn consume_required_eol(source: &str, raw_end: usize) -> Option<ConsumedEol> {
    let mut cursor = raw_end + horizontal_whitespace_len(source.get(raw_end..)?);
    if source.get(cursor..)?.starts_with("%%") {
        cursor = inline_terminal_end(source, cursor + 2);
    }
    if cursor == source.len() {
        return Some(ConsumedEol {
            end: cursor,
            trailing_whitespace_span: None,
        });
    }
    cursor = consume_newline(source, cursor)?;
    let mut trailing_whitespace_span = None;

    loop {
        let trivia_start = cursor;
        let mut next = cursor + horizontal_whitespace_len(source.get(cursor..)?);
        let comment = source.get(next..)?.starts_with("%%");
        if comment {
            next = inline_terminal_end(source, next + 2);
        } else if next > trivia_start
            && (next == source.len() || consume_newline(source, next).is_some())
        {
            trailing_whitespace_span = Some(SourceSpan::new(trivia_start, next));
        }
        if next == source.len() {
            return Some(ConsumedEol {
                end: next,
                trailing_whitespace_span,
            });
        }
        if let Some(after_newline) = consume_newline(source, next) {
            cursor = after_newline;
            continue;
        }
        return Some(ConsumedEol {
            end: trivia_start,
            trailing_whitespace_span,
        });
    }
}

fn consume_newline(source: &str, offset: usize) -> Option<usize> {
    if source.get(offset..)?.starts_with("\r\n") {
        return Some(offset + 2);
    }
    (source.as_bytes().get(offset) == Some(&b'\n')).then_some(offset + 1)
}

fn horizontal_whitespace_len(input: &str) -> usize {
    input
        .chars()
        .take_while(|ch| is_horizontal_whitespace(*ch))
        .map(char::len_utf8)
        .sum()
}

fn is_horizontal_whitespace(ch: char) -> bool {
    matches!(ch, ' ' | '\t')
}

fn skip_javascript_whitespace(source: &str, start: usize) -> usize {
    let mut cursor = start;
    let Some(rest) = source.get(start..) else {
        return cursor;
    };
    for ch in rest.chars() {
        if !is_javascript_whitespace(ch) {
            break;
        }
        cursor += ch.len_utf8();
    }
    cursor
}

fn is_javascript_whitespace(ch: char) -> bool {
    ch.is_whitespace() || matches!(ch, '\u{000B}' | '\u{000C}' | '\u{FEFF}')
}

fn normalized_block_value(source: &str, start: usize, end: usize) -> (String, SourceSpan) {
    let raw = &source[start..end];
    let leading = raw.len() - raw.trim_start().len();
    let trimmed = raw.trim();
    let value_start = start + leading;
    let value_span = SourceSpan::new(value_start, value_start + trimmed.len());
    let value = raw
        .lines()
        .map(str::trim)
        .map(collapse_horizontal_runs)
        .filter(|line| !line.is_empty())
        .collect::<Vec<_>>()
        .join("\n");
    (value, value_span)
}

fn collapse_horizontal_runs(value: &str) -> String {
    let mut output = String::with_capacity(value.len());
    let mut horizontal = String::new();
    for ch in value.chars() {
        if is_horizontal_whitespace(ch) {
            horizontal.push(ch);
            continue;
        }
        if horizontal.len() >= 2 {
            output.push(' ');
        } else {
            output.push_str(&horizontal);
        }
        horizontal.clear();
        output.push(ch);
    }
    if horizontal.len() >= 2 {
        output.push(' ');
    } else {
        output.push_str(&horizontal);
    }
    output
}

#[cfg(test)]
mod tests {
    use super::*;
    #[cfg(feature = "all-diagrams")]
    use crate::{EditorSemanticCompleteness, Error, MermaidConfig, ParseMetadata, Result};
    #[cfg(feature = "all-diagrams")]
    use serde_json::Value;

    #[test]
    fn langium_string_removes_escape_backslashes_without_decoding_control_characters() {
        for (source, expected) in [
            ("\"\\n\"", "n"),
            ("\"\\t\"", "t"),
            ("\"\\0\"", "0"),
            ("\"\\\"\"", "\""),
            ("\"\\\\\"", "\\"),
            ("'\\''", "'"),
        ] {
            let parsed = parse_langium_string(source, 10)
                .unwrap_or_else(|| panic!("STRING terminal did not parse: {source:?}"));
            assert_eq!(parsed.value, expected, "source: {source:?}");
            assert_eq!(parsed.consumed, source.len(), "source: {source:?}");
            assert_eq!(
                parsed.raw_span,
                SourceSpan::new(10, 10 + source.len()),
                "source: {source:?}"
            );
            assert_eq!(
                parsed.value_span,
                SourceSpan::new(11, 9 + source.len()),
                "source: {source:?}"
            );
        }
    }

    #[test]
    fn langium_string_rejects_escaped_ecmascript_line_terminators() {
        for line_terminator in ['\n', '\r', '\u{2028}', '\u{2029}'] {
            let source = format!("\"before\\{line_terminator}after\"");
            assert_eq!(
                parse_langium_string(&source, 0),
                None,
                "escaped line terminator: {line_terminator:?}"
            );
        }
    }

    #[test]
    fn common_terminal_conformance_matrix() {
        struct Case {
            source: &'static str,
            offset: usize,
            field: LangiumCommonField,
            value: &'static str,
            raw: &'static str,
            value_source: &'static str,
            remainder: &'static str,
        }

        let cases = [
            Case {
                source: "packet title  Packet   map %% note\r\n0: \"bit\"\r\n",
                offset: "packet".len(),
                field: LangiumCommonField::Title,
                value: "Packet map",
                raw: " title  Packet   map ",
                value_source: "Packet   map",
                remainder: "0: \"bit\"\r\n",
            },
            Case {
                source: "\taccTitle:\t Accessible   packet\nnext",
                offset: 0,
                field: LangiumCommonField::AccTitle,
                value: "Accessible packet",
                raw: "\taccTitle:\t Accessible   packet",
                value_source: "Accessible   packet",
                remainder: "next",
            },
            Case {
                source: "accDescr: value before comment %% hidden\nnext",
                offset: 0,
                field: LangiumCommonField::AccDescr,
                value: "value before comment",
                raw: "accDescr: value before comment ",
                value_source: "value before comment",
                remainder: "next",
            },
            Case {
                source: "accDescr { same   line } \t%% hidden\r\nnext",
                offset: 0,
                field: LangiumCommonField::AccDescr,
                value: "same line",
                raw: "accDescr { same   line }",
                value_source: "same   line",
                remainder: "next",
            },
            Case {
                source: "accDescr\r\n{\r\n  First   line\r\n\r\n\tSecond\t\tline  \r\n}\r\nnext",
                offset: 0,
                field: LangiumCommonField::AccDescr,
                value: "First line\nSecond line",
                raw: "accDescr\r\n{\r\n  First   line\r\n\r\n\tSecond\t\tline  \r\n}",
                value_source: "First   line\r\n\r\n\tSecond\t\tline",
                remainder: "next",
            },
            Case {
                source: "accDescr { text %% is payload }\n",
                offset: 0,
                field: LangiumCommonField::AccDescr,
                value: "text %% is payload",
                raw: "accDescr { text %% is payload }",
                value_source: "text %% is payload",
                remainder: "",
            },
            Case {
                source: "title alpha\tbeta\n\n  %% hidden blank\r\n\r\nnext",
                offset: 0,
                field: LangiumCommonField::Title,
                value: "alpha\tbeta",
                raw: "title alpha\tbeta",
                value_source: "alpha\tbeta",
                remainder: "next",
            },
            Case {
                source: "accDescr {\n  alpha\tbeta\n\n  gamma\t\tdelta\n}\n",
                offset: 0,
                field: LangiumCommonField::AccDescr,
                value: "alpha\tbeta\ngamma delta",
                raw: "accDescr {\n  alpha\tbeta\n\n  gamma\t\tdelta\n}",
                value_source: "alpha\tbeta\n\n  gamma\t\tdelta",
                remainder: "",
            },
            Case {
                source: "title\n",
                offset: 0,
                field: LangiumCommonField::Title,
                value: "",
                raw: "title",
                value_source: "",
                remainder: "",
            },
            Case {
                source: "accTitle:%% comment\n",
                offset: 0,
                field: LangiumCommonField::AccTitle,
                value: "",
                raw: "accTitle:",
                value_source: "",
                remainder: "",
            },
        ];

        for case in cases {
            let parsed = parse_langium_common(case.source, case.offset)
                .unwrap_or_else(|| panic!("common terminal did not parse: {:?}", case.source));
            assert_eq!(parsed.fact.field, case.field, "source: {:?}", case.source);
            assert_eq!(parsed.fact.value, case.value, "source: {:?}", case.source);
            assert_eq!(
                &case.source[parsed.fact.raw_span.start..parsed.fact.raw_span.end],
                case.raw,
                "source: {:?}",
                case.source
            );
            assert_eq!(
                &case.source[parsed.fact.value_span.start..parsed.fact.value_span.end],
                case.value_source,
                "source: {:?}",
                case.source
            );
            assert_eq!(
                &case.source[case.offset + parsed.consumed..],
                case.remainder,
                "source: {:?}",
                case.source
            );
            assert!(parsed.diagnostic.is_none());
        }
    }

    #[test]
    fn common_facts_are_ordered_and_last_assignment_wins() {
        let source = "title First\naccDescr: Description\ntitle Second\n";
        let mut offset = 0usize;
        let mut facts = LangiumCommonFacts::default();
        while offset < source.len() {
            let parsed = parse_langium_common(source, offset).unwrap();
            offset += parsed.consumed;
            facts.push(parsed.fact);
        }

        assert_eq!(facts.ordered().len(), 3);
        assert_eq!(
            facts
                .last(LangiumCommonField::Title)
                .map(|fact| fact.value.as_str()),
            Some("Second")
        );
        assert_eq!(
            facts
                .last(LangiumCommonField::AccDescr)
                .map(|fact| fact.value.as_str()),
            Some("Description")
        );
    }

    #[test]
    fn unterminated_block_returns_partial_fact_and_eof_diagnostic() {
        let source = "accDescr {\r\n  First   line\r\n  Second line";
        let parsed = parse_langium_common(source, 0).unwrap();

        assert!(parsed.diagnostic.is_some());
        assert_eq!(parsed.consumed, source.len());
        assert_eq!(parsed.fact.value, "First line\nSecond line");
        assert_eq!(parsed.fact.raw_span, SourceSpan::new(0, source.len()));
        assert_eq!(
            parsed.fact.value_span,
            SourceSpan::new(source.find("First").unwrap(), source.len())
        );
        assert_eq!(
            parsed.diagnostic.unwrap().span,
            SourceSpan::new(source.len(), source.len())
        );
    }

    #[test]
    fn common_terminals_require_exact_case_and_eol_or_eof() {
        for source in [
            "Title Wrong case\n",
            "accDescription: not a terminal\n",
            "title: not title syntax\n",
            "titlex\n",
            "accDescr { value } trailing\n",
            "%% title hidden\n",
        ] {
            assert!(
                parse_langium_common(source, 0).is_none(),
                "unexpected match for {source:?}"
            );
        }
    }

    #[cfg(feature = "all-diagrams")]
    type JsonParser = fn(&str, &ParseMetadata) -> Result<Value>;
    #[cfg(feature = "all-diagrams")]
    struct FamilyCase {
        id: &'static str,
        header: &'static str,
        tail: &'static str,
        parser: JsonParser,
        combined: crate::family::CombinedSemanticParser,
        retains_title: bool,
    }

    #[cfg(feature = "all-diagrams")]
    fn family_cases() -> [FamilyCase; 7] {
        [
            FamilyCase {
                id: "architecture",
                header: "architecture-beta",
                tail: "service api\r\n",
                parser: crate::diagrams::architecture::parse_architecture,
                combined: crate::diagrams::architecture::parse_architecture_json_and_editor_facts,
                retains_title: true,
            },
            FamilyCase {
                id: "cynefin",
                header: "cynefin-beta",
                tail: "clear\r\n",
                parser: crate::diagrams::cynefin::parse_cynefin,
                combined: crate::diagrams::cynefin::parse_cynefin_json_and_editor_facts,
                retains_title: true,
            },
            FamilyCase {
                id: "gitGraph",
                header: "gitGraph",
                tail: "commit id: \"c1\"\r\n",
                parser: crate::diagrams::git_graph::parse_git_graph,
                combined: crate::diagrams::git_graph::parse_git_graph_json_and_editor_facts,
                retains_title: true,
            },
            FamilyCase {
                id: "info",
                header: "info showInfo",
                tail: "",
                parser: crate::diagrams::info::parse_info,
                combined: crate::diagrams::info::parse_info_json_and_editor_facts,
                retains_title: false,
            },
            FamilyCase {
                id: "packet",
                header: "packet",
                tail: "0-7: \"byte\"\r\n",
                parser: crate::diagrams::packet::parse_packet,
                combined: crate::diagrams::packet::parse_packet_json_and_editor_facts,
                retains_title: true,
            },
            FamilyCase {
                id: "pie",
                header: "pie showData",
                tail: "\"A\": 1\r\n",
                parser: crate::diagrams::pie::parse_pie,
                combined: crate::diagrams::pie::parse_pie_json_and_editor_facts,
                retains_title: true,
            },
            FamilyCase {
                id: "radar",
                header: "radar-beta",
                tail: "axis a\r\ncurve c { 1 }\r\n",
                parser: crate::diagrams::radar::parse_radar,
                combined: crate::diagrams::radar::parse_radar_json_and_editor_facts,
                retains_title: true,
            },
        ]
    }

    #[cfg(feature = "all-diagrams")]
    fn metadata(id: &str) -> ParseMetadata {
        ParseMetadata {
            diagram_type: id.to_string(),
            config: MermaidConfig::default(),
            effective_config: MermaidConfig::default(),
            title: None,
        }
    }

    #[cfg(feature = "all-diagrams")]
    fn common_family_source(case: &FamilyCase) -> String {
        format!(
            "{}\r\n  title First\r\n  title Final   title %% hidden\r\n  accTitle:  Accessible   map\r\n  accDescr {{\r\n    First   line\r\n\r\n    \tSecond\t\tline  \r\n  }}\r\n{}",
            case.header, case.tail
        )
    }

    #[test]
    #[cfg(feature = "all-diagrams")]
    fn importing_families_share_common_values_spans_and_db_capabilities() {
        for case in family_cases() {
            let source = common_family_source(&case);
            let meta = metadata(case.id);
            let model = (case.parser)(&source, &meta)
                .unwrap_or_else(|error| panic!("{} common parse failed: {error}", case.id));

            if case.retains_title {
                assert_eq!(model["title"], "Final title", "family: {}", case.id);
            } else {
                assert!(model.get("title").is_none(), "family: {}", case.id);
            }
            if case.id == "info" {
                assert!(model.get("accTitle").is_none());
                assert!(model.get("accDescr").is_none());
            } else {
                assert_eq!(model["accTitle"], "Accessible map", "family: {}", case.id);
                assert_eq!(
                    model["accDescr"], "First line\nSecond line",
                    "family: {}",
                    case.id
                );
            }

            let editor = crate::family::test_support::editor_facts(case.combined, &source, &meta);
            assert_eq!(
                editor.completeness,
                EditorSemanticCompleteness::Complete,
                "family: {}",
                case.id
            );
            let detail = format!("{} accessibility description", case.id);
            let descr = editor
                .symbols
                .iter()
                .find(|symbol| symbol.detail.as_deref() == Some(detail.as_str()))
                .unwrap_or_else(|| panic!("{} missing common editor payload", case.id));
            assert_eq!(descr.name, "First line\nSecond line");
            let raw = &source[descr.span.start..descr.span.end];
            assert!(raw.starts_with("  accDescr {"), "family: {}", case.id);
            assert!(raw.ends_with("  }"), "family: {}", case.id);
            let selected = &source[descr.selection.start..descr.selection.end];
            assert!(selected.starts_with("First"), "family: {}", case.id);
            assert!(selected.contains("Second\t\tline"), "family: {}", case.id);
        }
    }

    #[test]
    #[cfg(feature = "all-diagrams")]
    fn importing_family_typed_models_receive_the_same_common_values() {
        let cases = family_cases();
        let source = |id: &str| {
            let case = cases.iter().find(|case| case.id == id).unwrap();
            common_family_source(case)
        };

        let architecture = crate::diagrams::architecture::parse_architecture_model_for_render(
            &source("architecture"),
            &metadata("architecture"),
        )
        .unwrap();
        assert_eq!(architecture.title.as_deref(), Some("Final title"));
        assert_eq!(
            architecture.acc_descr.as_deref(),
            Some("First line\nSecond line")
        );
        assert_eq!(architecture.nodes.len(), 1);

        let cynefin = crate::diagrams::cynefin::parse_cynefin_model_for_render(
            &source("cynefin"),
            &metadata("cynefin"),
        )
        .unwrap();
        assert_eq!(cynefin.title.as_deref(), Some("Final title"));
        assert_eq!(cynefin.domains.len(), 1);

        let git_graph = crate::diagrams::git_graph::parse_git_graph_model_for_render(
            &source("gitGraph"),
            &metadata("gitGraph"),
        )
        .unwrap();
        assert_eq!(git_graph.title.as_deref(), Some("Final title"));
        assert_eq!(git_graph.acc_title.as_deref(), Some("Accessible map"));
        assert_eq!(
            git_graph.acc_descr.as_deref(),
            Some("First line\nSecond line")
        );
        assert_eq!(git_graph.commits.len(), 1);

        let info =
            crate::diagrams::info::parse_info_model_for_render(&source("info"), &metadata("info"))
                .unwrap();
        assert!(info.show_info);

        let packet = crate::diagrams::packet::parse_packet_model_for_render(
            &source("packet"),
            &metadata("packet"),
        )
        .unwrap();
        assert_eq!(packet.title.as_deref(), Some("Final title"));
        assert_eq!(packet.packet.len(), 1);

        let pie =
            crate::diagrams::pie::parse_pie_model_for_render(&source("pie"), &metadata("pie"))
                .unwrap();
        assert_eq!(pie.title.as_deref(), Some("Final title"));
        assert!(pie.show_data);
        assert_eq!(pie.sections.len(), 1);

        let radar = crate::diagrams::radar::parse_radar_model_for_render(
            &source("radar"),
            &metadata("radar"),
        )
        .unwrap();
        assert_eq!(radar.title.as_deref(), Some("Final title"));
        assert_eq!(radar.axes.len(), 1);
        assert_eq!(radar.curves.len(), 1);
    }

    #[test]
    #[cfg(feature = "all-diagrams")]
    fn importing_families_reject_unterminated_blocks_and_recover_editor_payloads() {
        for case in family_cases() {
            let source = format!("{}\naccDescr {{\n  partial", case.header);
            let meta = metadata(case.id);
            let error = match (case.parser)(&source, &meta) {
                Ok(_) => panic!("{} accepted unterminated accDescr", case.id),
                Err(error) => error,
            };
            let Error::DiagramParse { diagnostic, .. } = error else {
                panic!("{} returned non-parser error", case.id);
            };
            assert_eq!(
                diagnostic.span(),
                Some(SourceSpan::new(source.len(), source.len()))
            );

            let editor = crate::family::test_support::editor_facts(case.combined, &source, &meta);
            assert_eq!(editor.completeness, EditorSemanticCompleteness::Recovered);
            assert!(
                editor
                    .directive_prefixes
                    .iter()
                    .any(|prefix| prefix == "accDescr")
            );
            assert!(editor.symbols.iter().any(|symbol| {
                symbol.name == "partial"
                    && symbol.selection
                        == SourceSpan::new(source.find("partial").unwrap(), source.len())
            }));
            assert!(editor.diagnostics.iter().any(|entry| {
                entry.message.contains("unterminated accDescr block")
                    && entry.span == Some(SourceSpan::new(source.len(), source.len()))
            }));
        }
    }
}
