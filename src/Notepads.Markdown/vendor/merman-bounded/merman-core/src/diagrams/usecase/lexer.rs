use super::ParseIssue;
use crate::{
    OperationControl, OperationControlResult, SourceSpan, diagrams::scan::is_ecmascript_whitespace,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum TokenKind {
    Identifier,
    NumberLiteral,
    Usecase,
    Actor,
    SystemBoundary,
    End,
    Direction,
    Td,
    Tb,
    Bt,
    Lr,
    Rl,
    Note,
    For,
    Json,
    ClassDef,
    Class,
    Style,
    Include,
    Extend,
    True,
    False,
    NewLine,
    Comment,
    PlainString,
    MarkdownString,
    AccTitleLine,
    AccDescrLine,
    AccDescrBlock,
    JsonDeclarationStart,
    JsonObjectLiteral,
    StereotypeStart,
    StereotypeText,
    StereotypeEnd,
    ClassSeparator,
    MetadataStart,
    At,
    LeftBrace,
    RightBrace,
    LeftBracket,
    RightBracket,
    LeftParen,
    RightParen,
    Comma,
    Colon,
    Generalization,
    DependencyArrow,
    ForwardSolid,
    BackwardSolid,
    ForwardCircle,
    BackwardCircle,
    ForwardCross,
    BackwardCross,
    MarkerlessSolid,
    CssIdentifier,
    HashColor,
    CssEscapedComma,
    Dash,
    Dot,
    Percent,
    CssPunctuation,
    LabelPunctuation,
    LabelSymbol,
    Eof,
}

impl TokenKind {
    pub(super) fn is_word(self) -> bool {
        matches!(
            self,
            Self::Identifier
                | Self::Usecase
                | Self::Actor
                | Self::SystemBoundary
                | Self::End
                | Self::Direction
                | Self::Td
                | Self::Tb
                | Self::Bt
                | Self::Lr
                | Self::Rl
                | Self::Note
                | Self::For
                | Self::Json
                | Self::ClassDef
                | Self::Class
                | Self::Style
                | Self::Include
                | Self::Extend
                | Self::True
                | Self::False
        )
    }

    pub(super) fn is_label_text(self) -> bool {
        self.is_word()
            || matches!(
                self,
                Self::NumberLiteral
                    | Self::At
                    | Self::Comma
                    | Self::Colon
                    | Self::HashColor
                    | Self::CssIdentifier
                    | Self::CssEscapedComma
                    | Self::Dash
                    | Self::Dot
                    | Self::Percent
                    | Self::CssPunctuation
                    | Self::LabelPunctuation
                    | Self::LabelSymbol
            )
    }

    pub(super) fn is_style_component(self) -> bool {
        self.is_word()
            || matches!(
                self,
                Self::PlainString
                    | Self::NumberLiteral
                    | Self::HashColor
                    | Self::CssIdentifier
                    | Self::CssEscapedComma
                    | Self::Dash
                    | Self::Dot
                    | Self::Percent
                    | Self::CssPunctuation
                    | Self::Colon
                    | Self::LeftParen
                    | Self::RightParen
                    | Self::LeftBracket
                    | Self::RightBracket
                    | Self::LeftBrace
                    | Self::RightBrace
                    | Self::At
                    | Self::MarkerlessSolid
            )
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct Token {
    pub kind: TokenKind,
    pub span: SourceSpan,
}

impl Token {
    pub(super) fn text(self, source: &str) -> &str {
        &source[self.span.start..self.span.end]
    }
}

fn word(byte: u8) -> bool {
    byte.is_ascii_alphanumeric() || byte == b'_'
}

fn hws(byte: u8) -> bool {
    matches!(byte, b' ' | b'\t')
}

fn scan_while(
    bytes: &[u8],
    mut offset: usize,
    predicate: impl Fn(u8) -> bool,
    control: &OperationControl,
) -> OperationControlResult<usize> {
    while bytes.get(offset).is_some_and(|byte| predicate(*byte)) {
        if offset & 1023 == 0 {
            control.checkpoint()?;
        }
        offset += 1;
    }
    Ok(offset)
}

fn issue(message: &str, start: usize, end: usize) -> ParseIssue {
    ParseIssue {
        message: message.to_owned(),
        span: SourceSpan { start, end },
    }
}

fn push(tokens: &mut Vec<Token>, kind: TokenKind, start: usize, end: usize) {
    tokens.push(Token {
        kind,
        span: SourceSpan { start, end },
    });
}

/// The order here follows the pinned Chevrotain token vocabulary, including its
/// keyword longer alternatives and the dedicated JSON/stereotype lexer modes.
pub(super) fn lex(
    source: &str,
    control: &OperationControl,
) -> OperationControlResult<Result<Vec<Token>, ParseIssue>> {
    use TokenKind::*;
    let bytes = source.as_bytes();
    let mut offset = 0;
    let mut tokens = Vec::new();
    let mut indented_line_start = true;
    while offset < bytes.len() {
        control.checkpoint()?;
        let start = offset;
        let remaining = &source[start..];
        let byte = bytes[start];
        if hws(byte) {
            offset = scan_while(bytes, start, hws, control)?;
            continue;
        }
        if matches!(byte, b'\n' | b'\r') {
            offset += if remaining.starts_with("\r\n") { 2 } else { 1 };
            push(&mut tokens, NewLine, start, offset);
            indented_line_start = true;
            continue;
        }
        if remaining.starts_with("\"`") {
            offset = start + 2;
            while offset + 1 < bytes.len() && !(bytes[offset] == b'`' && bytes[offset + 1] == b'"')
            {
                if offset & 1023 == 0 {
                    control.checkpoint()?;
                }
                offset += 1;
            }
            if offset + 1 >= bytes.len() {
                return Ok(Err(issue("Unclosed Markdown string", start, bytes.len())));
            }
            offset += 2;
            push(&mut tokens, MarkdownString, start, offset);
            indented_line_start = false;
            continue;
        }
        if indented_line_start && remaining.starts_with("%%") {
            offset = scan_while(bytes, start + 2, |b| !matches!(b, b'\n' | b'\r'), control)?;
            push(&mut tokens, Comment, start, offset);
            indented_line_start = false;
            continue;
        }
        if indented_line_start {
            let accessibility = if remaining.starts_with("accTitle") {
                Some((AccTitleLine, 8))
            } else if remaining.starts_with("accDescr") {
                Some((AccDescrLine, 8))
            } else {
                None
            };
            if let Some((kind, length)) = accessibility {
                let delimiter = scan_while(bytes, start + length, hws, control)?;
                if bytes.get(delimiter) == Some(&b':') {
                    offset = scan_while(
                        bytes,
                        delimiter + 1,
                        |b| !matches!(b, b'\n' | b'\r'),
                        control,
                    )?;
                    push(&mut tokens, kind, start, offset);
                    indented_line_start = false;
                    continue;
                }
                if kind == AccDescrLine && bytes.get(delimiter) == Some(&b'{') {
                    let end = scan_while(bytes, delimiter + 1, |b| b != b'}', control)?;
                    if end < bytes.len() {
                        offset = end + 1;
                        push(&mut tokens, AccDescrBlock, start, offset);
                        indented_line_start = false;
                        continue;
                    }
                }
            }
        }
        indented_line_start = false;
        if remaining.starts_with("json") && bytes.get(start + 4).is_some_and(|b| hws(*b)) {
            let id_start = scan_while(bytes, start + 4, hws, control)?;
            let id_end = scan_while(bytes, id_start, word, control)?;
            let at = scan_while(bytes, id_end, hws, control)?;
            if id_end > id_start && bytes.get(at) == Some(&b'@') {
                let brace = scan_while(bytes, at + 1, hws, control)?;
                if bytes.get(brace) == Some(&b'{') {
                    push(&mut tokens, JsonDeclarationStart, start, brace);
                    let mut depth = 0usize;
                    let mut quoted = false;
                    let mut escaped = false;
                    offset = brace;
                    while offset < bytes.len() {
                        if offset & 1023 == 0 {
                            control.checkpoint()?;
                        }
                        let byte = bytes[offset];
                        offset += 1;
                        if quoted {
                            if escaped {
                                escaped = false;
                            } else if byte == b'\\' {
                                escaped = true;
                            } else if byte == b'"' {
                                quoted = false;
                            }
                        } else if byte == b'"' {
                            quoted = true;
                        } else if byte == b'{' {
                            depth += 1;
                        } else if byte == b'}' {
                            depth -= 1;
                            if depth == 0 {
                                break;
                            }
                        }
                    }
                    if depth != 0 {
                        return Ok(Err(issue("Unclosed JSON object", brace, bytes.len())));
                    }
                    push(&mut tokens, JsonObjectLiteral, brace, offset);
                    continue;
                }
            }
        }
        let identifier_end = if word(byte) {
            scan_while(bytes, start, word, control)?
        } else {
            start
        };
        let keywords = [
            ("usecase-beta", Usecase),
            ("actor", Actor),
            ("systemBoundary", SystemBoundary),
            ("end", End),
            ("direction", Direction),
            ("TD", Td),
            ("TB", Tb),
            ("BT", Bt),
            ("LR", Lr),
            ("RL", Rl),
            ("note", Note),
            ("for", For),
            ("json", Json),
            ("classDef", ClassDef),
            ("class", Class),
            ("style", Style),
            ("include", Include),
            ("extend", Extend),
            ("true", True),
            ("false", False),
        ];
        if let Some((image, kind)) = keywords.into_iter().find(|(image, kind)| {
            if matches!(kind, Include | Extend) {
                remaining
                    .get(..image.len())
                    .is_some_and(|prefix| prefix.eq_ignore_ascii_case(image))
            } else {
                remaining.starts_with(image)
            }
        }) {
            if identifier_end > start + image.len() {
                offset = identifier_end;
                push(&mut tokens, Identifier, start, offset);
            } else {
                offset += image.len();
                push(&mut tokens, kind, start, offset);
            }
            continue;
        }
        if remaining.starts_with("<<") {
            push(&mut tokens, StereotypeStart, start, start + 2);
            offset = start + 2;
            while offset < bytes.len()
                && !matches!(bytes[offset], b'\n' | b'\r')
                && !(bytes[offset] == b'>' && bytes.get(offset + 1) == Some(&b'>'))
            {
                if offset & 1023 == 0 {
                    control.checkpoint()?;
                }
                offset += 1;
            }
            if bytes.get(offset..offset + 2) != Some(b">>") {
                return Ok(Err(issue("Unclosed stereotype", start, offset)));
            }
            let stereotype = &source[start + 2..offset];
            if stereotype.is_empty() || stereotype.chars().all(is_ecmascript_whitespace) {
                return Ok(Err(issue("Expected stereotype text", start + 2, offset)));
            }
            push(&mut tokens, StereotypeText, start + 2, offset);
            push(&mut tokens, StereotypeEnd, offset, offset + 2);
            offset += 2;
            continue;
        }
        let fixed = [
            ("--|>", Generalization),
            ("..>", DependencyArrow),
            (":::", ClassSeparator),
        ];
        if let Some((image, kind)) = fixed
            .into_iter()
            .find(|(image, _)| remaining.starts_with(image))
        {
            offset += image.len();
            push(&mut tokens, kind, start, offset);
            continue;
        }
        if remaining.starts_with("--") {
            let dash_end = scan_while(bytes, start + 2, |b| b == b'-', control)?;
            let (kind, end) = if bytes.get(dash_end) == Some(&b'>') {
                (ForwardSolid, dash_end + 1)
            } else if remaining.starts_with("--o") {
                (ForwardCircle, start + 3)
            } else if remaining.starts_with("--x") {
                (ForwardCross, start + 3)
            } else {
                (MarkerlessSolid, dash_end)
            };
            offset = end;
            push(&mut tokens, kind, start, offset);
            continue;
        }
        if remaining.starts_with("<--") {
            offset = scan_while(bytes, start + 3, |b| b == b'-', control)?;
            push(&mut tokens, BackwardSolid, start, offset);
            continue;
        }
        if remaining.starts_with("o--") || remaining.starts_with("x--") {
            offset += 3;
            push(
                &mut tokens,
                if byte == b'o' {
                    BackwardCircle
                } else {
                    BackwardCross
                },
                start,
                offset,
            );
            continue;
        }
        if remaining.starts_with("@{") {
            offset += 2;
            push(&mut tokens, MetadataStart, start, offset);
            continue;
        }
        if matches!(byte, b'"' | b'\'') {
            offset = scan_while(
                bytes,
                start + 1,
                |b| b != byte && !matches!(b, b'\n' | b'\r'),
                control,
            )?;
            if bytes.get(offset) != Some(&byte) {
                return Ok(Err(issue("Unclosed quoted string", start, offset)));
            }
            offset += 1;
            push(&mut tokens, PlainString, start, offset);
            continue;
        }
        if byte == b'#' {
            let end = scan_while(bytes, start + 1, |b| b.is_ascii_hexdigit(), control)?;
            if end > start + 1 {
                offset = end;
                push(&mut tokens, HashColor, start, offset);
                continue;
            }
        }
        if byte.is_ascii_alphabetic() || byte == b'_' {
            let mut css_end = identifier_end;
            while bytes.get(css_end) == Some(&b'-')
                && bytes.get(css_end + 1).is_some_and(|b| word(*b))
            {
                css_end = scan_while(bytes, css_end + 1, word, control)?;
            }
            if css_end > identifier_end {
                offset = css_end;
                push(&mut tokens, CssIdentifier, start, offset);
                continue;
            }
        }
        let mut number_end = start;
        if byte.is_ascii_digit()
            || (byte == b'.' && bytes.get(start + 1).is_some_and(u8::is_ascii_digit))
        {
            number_end = scan_while(bytes, start, |b| b.is_ascii_digit(), control)?;
            if bytes.get(number_end) == Some(&b'.')
                && bytes.get(number_end + 1).is_some_and(u8::is_ascii_digit)
            {
                number_end = scan_while(bytes, number_end + 1, |b| b.is_ascii_digit(), control)?;
            }
            number_end = scan_while(bytes, number_end, |b| b.is_ascii_alphabetic(), control)?;
        }
        if identifier_end > start || number_end > start {
            offset = identifier_end.max(number_end);
            push(
                &mut tokens,
                if number_end > identifier_end {
                    NumberLiteral
                } else {
                    Identifier
                },
                start,
                offset,
            );
            continue;
        }
        if remaining.starts_with("\\,") {
            offset += 2;
            push(&mut tokens, CssEscapedComma, start, offset);
            continue;
        }
        let kind = match byte {
            b'@' => At,
            b'{' => LeftBrace,
            b'}' => RightBrace,
            b'[' => LeftBracket,
            b']' => RightBracket,
            b'(' => LeftParen,
            b')' => RightParen,
            b',' => Comma,
            b':' => Colon,
            b'-' => Dash,
            b'.' => Dot,
            b'%' => Percent,
            b'!' | b'#' | b'$' | b'&' | b'*' | b'+' | b'/' | b'=' | b'?' | b'^' | b'_' | b'|'
            | b'~' => CssPunctuation,
            b';' | b'<' | b'>' | b'\\' | b'`' => LabelPunctuation,
            _ => LabelSymbol,
        };
        offset = if kind == LabelSymbol {
            scan_while(
                bytes,
                start,
                |b| !matches!(b, b'\t' | b'\n' | b'\r' | b' '..=b'~'),
                control,
            )?
        } else {
            start + 1
        };
        push(&mut tokens, kind, start, offset);
    }
    push(&mut tokens, Eof, bytes.len(), bytes.len());
    Ok(Ok(tokens))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn images(source: &str) -> Vec<(TokenKind, &str)> {
        lex(source, &OperationControl::default())
            .expect("operation remains active")
            .expect("valid source")
            .into_iter()
            .filter(|token| token.kind != TokenKind::Eof)
            .map(|token| (token.kind, token.text(source)))
            .collect()
    }

    #[test]
    fn upstream_operator_and_keyword_precedence() {
        use TokenKind::*;
        assert_eq!(
            images("--|> ..> <<Human>> ::: ---> <--- --o o-- --x x-- --"),
            vec![
                (Generalization, "--|>"),
                (DependencyArrow, "..>"),
                (StereotypeStart, "<<"),
                (StereotypeText, "Human"),
                (StereotypeEnd, ">>"),
                (ClassSeparator, ":::"),
                (ForwardSolid, "--->"),
                (BackwardSolid, "<---"),
                (ForwardCircle, "--o"),
                (BackwardCircle, "o--"),
                (ForwardCross, "--x"),
                (BackwardCross, "x--"),
                (MarkerlessSolid, "--"),
            ]
        );
        assert_eq!(
            images("actor actorName include INCLUDE included 1mg 1.5px .5 red-blue actorName-red"),
            vec![
                (Actor, "actor"),
                (Identifier, "actorName"),
                (Include, "include"),
                (Include, "INCLUDE"),
                (Identifier, "included"),
                (Identifier, "1mg"),
                (NumberLiteral, "1.5px"),
                (NumberLiteral, ".5"),
                (CssIdentifier, "red-blue"),
                (Identifier, "actorName"),
                (Dash, "-"),
                (Identifier, "red"),
            ]
        );
    }

    #[test]
    fn multiline_modes_preserve_byte_spans_and_line_only_tokens() {
        use TokenKind::*;
        assert_eq!(
            images("  %% 日本語\r\nA %% data\n\"`多行\n%% literal`\"\n accDescr {first\nsecond}"),
            vec![
                (Comment, "%% 日本語"),
                (NewLine, "\r\n"),
                (Identifier, "A"),
                (Percent, "%"),
                (Percent, "%"),
                (Identifier, "data"),
                (NewLine, "\n"),
                (MarkdownString, "\"`多行\n%% literal`\""),
                (NewLine, "\n"),
                (AccDescrBlock, "accDescr {first\nsecond}"),
            ]
        );
        let source = "json Payload@{\"text\":\"} \\\" {\",\"nested\":{\"日本語\":[1,2]}}:::data";
        let tokens = images(source);
        assert_eq!(tokens[0], (JsonDeclarationStart, "json Payload@"));
        assert_eq!(tokens[1].0, JsonObjectLiteral);
        assert_eq!(tokens[2], (ClassSeparator, ":::"));
        assert_eq!(tokens[3], (Identifier, "data"));
    }

    #[test]
    fn malformed_string_and_mode_tokens_fail_without_partial_acceptance() {
        for source in [
            "\"unterminated",
            "'line\nbreak'",
            "\"`unterminated",
            "json Payload@{\"nested\":{}",
            "<<unclosed",
            "<< >>",
            "<<line\nbreak>>",
        ] {
            assert!(
                lex(source, &OperationControl::default())
                    .expect("operation remains active")
                    .is_err(),
                "{source}"
            );
        }
    }
}
