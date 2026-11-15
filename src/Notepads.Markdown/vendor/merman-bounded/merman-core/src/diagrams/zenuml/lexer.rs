use crate::{OperationControl, OperationControlResult, SourceSpan};
use unicode_general_category::{GeneralCategory, get_general_category};

const KNOWN_UNITS: &[&str] = &[
    "milliseconds",
    "millisecond",
    "seconds",
    "second",
    "minutes",
    "minute",
    "hours",
    "hour",
    "weeks",
    "week",
    "secs",
    "mins",
    "hrs",
    "days",
    "KiB",
    "MiB",
    "GiB",
    "TiB",
    "rem",
    "sec",
    "min",
    "day",
    "KB",
    "MB",
    "GB",
    "TB",
    "kb",
    "mb",
    "gb",
    "tb",
    "px",
    "mm",
    "cm",
    "km",
    "mg",
    "kg",
    "ms",
    "hr",
    "B",
    "em",
    "s",
    "h",
    "d",
    "w",
    "m",
    "g",
];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Keyword {
    As,
    Catch,
    Critical,
    Else,
    False,
    Finally,
    Group,
    If,
    In,
    New,
    Nil,
    Opt,
    Par,
    Ref,
    Return,
    Section,
    Title,
    True,
    Try,
    While,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) enum TokenKind {
    Keyword(Keyword),
    Identifier(String),
    DigitLeadingName(String),
    StringLiteral { value: String, closed: bool },
    Integer(String),
    Float(String),
    Money(String),
    NumberUnit(String),
    Color(String),
    Annotation(String),
    ReturnAnnotation,
    StarterAnnotation,
    Comment(String),
    Divider(String),
    EventPayload(String),
    EventEnd,
    TitleContent(String),
    TitleEnd,
    Modifier,
    LineBreak,
    Colon,
    Semicolon,
    Comma,
    Assign,
    Dot,
    OpenParen,
    CloseParen,
    OpenBrace,
    CloseBrace,
    OpenBracket,
    CloseBracket,
    StereotypeOpen,
    StereotypeClose,
    Arrow,
    ReturnArrow,
    Operator(String),
    Other(char),
    Eof,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum TokenChannel {
    Default,
    Comment,
    Modifier,
    Hidden,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct Token {
    pub(super) kind: TokenKind,
    pub(super) span: SourceSpan,
    pub(super) channel: TokenChannel,
}

impl Token {
    fn new(kind: TokenKind, start: usize, end: usize) -> Self {
        Self {
            kind,
            span: SourceSpan::new(start, end),
            channel: TokenChannel::Default,
        }
    }

    fn on_channel(mut self, channel: TokenChannel) -> Self {
        self.channel = channel;
        self
    }
}

#[cfg(test)]
pub(super) fn lex(source: &str) -> Vec<Token> {
    lex_controlled(source, &OperationControl::new())
        .expect("a private parse control cannot be cancelled")
}

pub(super) fn lex_controlled(
    source: &str,
    control: &OperationControl,
) -> OperationControlResult<Vec<Token>> {
    let mut tokens = Vec::new();
    let lexer = Lexer::new(source);
    for token in lexer {
        if tokens.len() % 128 == 0 {
            control.checkpoint()?;
        }
        tokens.push(token);
    }
    control.checkpoint()?;
    Ok(tokens)
}

#[cfg(test)]
pub(super) fn parser_tokens(tokens: &[Token]) -> Vec<Token> {
    parser_tokens_controlled(tokens, &OperationControl::new())
        .expect("a private parse control cannot be cancelled")
}

pub(super) fn parser_tokens_controlled(
    tokens: &[Token],
    control: &OperationControl,
) -> OperationControlResult<Vec<Token>> {
    let mut parser_tokens = Vec::new();
    for (index, token) in tokens.iter().enumerate() {
        if index % 128 == 0 {
            control.checkpoint()?;
        }
        if token.channel == TokenChannel::Default {
            parser_tokens.push(token.clone());
        }
    }
    control.checkpoint()?;
    Ok(parser_tokens)
}

struct Lexer<'a> {
    source: &'a str,
    offset: usize,
    line_start: usize,
    title_allowed: bool,
    mode: LexerMode,
    emitted_eof: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum LexerMode {
    Default,
    Event,
    Title,
}

impl<'a> Lexer<'a> {
    fn new(source: &'a str) -> Self {
        Self {
            source,
            offset: 0,
            line_start: 0,
            title_allowed: true,
            mode: LexerMode::Default,
            emitted_eof: false,
        }
    }

    fn rest(&self) -> &'a str {
        &self.source[self.offset..]
    }

    fn starts_with(&self, value: &str) -> bool {
        self.rest().starts_with(value)
    }

    fn bump_char(&mut self) -> Option<char> {
        let ch = self.rest().chars().next()?;
        self.offset += ch.len_utf8();
        Some(ch)
    }

    fn skip_horizontal_space(&mut self) {
        while self
            .rest()
            .chars()
            .next()
            .is_some_and(|ch| matches!(ch, ' ' | '\t'))
        {
            self.bump_char();
        }
    }

    fn word(&mut self) -> Token {
        let start = self.offset;
        self.bump_char();
        while self
            .rest()
            .chars()
            .next()
            .is_some_and(is_identifier_continue)
        {
            self.bump_char();
        }
        let value = &self.source[start..self.offset];
        let kind = if matches!(value, "const" | "readonly" | "static" | "await") {
            TokenKind::Modifier
        } else {
            keyword(value, self.title_allowed, self.source, self.offset)
                .map(TokenKind::Keyword)
                .unwrap_or_else(|| TokenKind::Identifier(value.to_string()))
        };
        if !matches!(kind, TokenKind::Keyword(Keyword::Title)) && value != "zenuml" {
            self.title_allowed = false;
        }
        let channel = if matches!(kind, TokenKind::Modifier) {
            TokenChannel::Modifier
        } else {
            TokenChannel::Default
        };
        if matches!(kind, TokenKind::Keyword(Keyword::Title)) {
            self.mode = LexerMode::Title;
        }
        Token::new(kind, start, self.offset).on_channel(channel)
    }

    fn number(&mut self) -> Token {
        let start = self.offset;
        self.offset = scan_ascii_digits(self.source, self.offset);
        let digits_end = self.offset;
        let mut has_dot = false;
        if self.starts_with(".") {
            has_dot = true;
            self.offset += 1;
            self.offset = scan_ascii_digits(self.source, self.offset);
        }
        let numeric_end = self.offset;
        let unit_end = known_unit_end(self.source, numeric_end);
        let digit_name_end = (!has_dot)
            .then(|| digit_leading_name_end(self.source, digits_end))
            .flatten();

        let kind = match (unit_end, digit_name_end) {
            (Some(unit_end), Some(name_end)) if name_end > unit_end => {
                self.offset = name_end;
                TokenKind::DigitLeadingName(self.source[start..name_end].to_string())
            }
            (Some(unit_end), _) => {
                self.offset = unit_end;
                TokenKind::NumberUnit(self.source[start..unit_end].to_string())
            }
            (None, Some(name_end)) => {
                self.offset = name_end;
                TokenKind::DigitLeadingName(self.source[start..name_end].to_string())
            }
            (None, None) if has_dot => {
                TokenKind::Float(self.source[start..numeric_end].to_string())
            }
            (None, None) => TokenKind::Integer(self.source[start..digits_end].to_string()),
        };
        self.title_allowed = false;
        Token::new(kind, start, self.offset)
    }

    fn dot_or_number(&mut self) -> Token {
        let start = self.offset;
        let digit_start = start + 1;
        let digits_end = scan_ascii_digits(self.source, digit_start);
        if digits_end == digit_start {
            return self.fixed(1, TokenKind::Dot);
        }

        if let Some(unit_end) = known_unit_end(self.source, digits_end) {
            self.offset = unit_end;
            self.title_allowed = false;
            return Token::new(
                TokenKind::NumberUnit(self.source[start..unit_end].to_string()),
                start,
                unit_end,
            );
        }

        if self.source[digits_end..]
            .chars()
            .next()
            .is_some_and(is_identifier_start)
        {
            return self.fixed(1, TokenKind::Dot);
        }

        self.offset = digits_end;
        self.title_allowed = false;
        Token::new(
            TokenKind::Float(self.source[start..digits_end].to_string()),
            start,
            digits_end,
        )
    }

    fn money(&mut self) -> Option<Token> {
        let start = self.offset;
        let number_start = start + 1;
        let mut end = scan_ascii_digits(self.source, number_start);
        if end > number_start {
            if self.source[end..].starts_with('.') {
                end = scan_ascii_digits(self.source, end + 1);
            }
        } else if self.source[number_start..].starts_with('.') {
            let digits_start = number_start + 1;
            end = scan_ascii_digits(self.source, digits_start);
            if end == digits_start {
                return None;
            }
        } else {
            return None;
        }
        self.offset = end;
        self.title_allowed = false;
        Some(Token::new(
            TokenKind::Money(self.source[start..end].to_string()),
            start,
            end,
        ))
    }

    fn string(&mut self) -> Token {
        let start = self.offset;
        self.offset += 1;
        let mut value = String::new();
        let mut closed = false;
        while self.offset < self.source.len() {
            if self.starts_with("\"\"") {
                value.push('"');
                self.offset += 2;
                continue;
            }
            let Some(ch) = self.rest().chars().next() else {
                break;
            };
            if ch == '"' {
                self.offset += 1;
                closed = true;
                break;
            }
            if matches!(ch, '\r' | '\n') {
                break;
            }
            value.push(ch);
            self.offset += ch.len_utf8();
        }
        self.title_allowed = false;
        Token::new(
            TokenKind::StringLiteral { value, closed },
            start,
            self.offset,
        )
    }

    fn annotation(&mut self) -> Token {
        let start = self.offset;
        self.offset += 1;
        while self
            .rest()
            .bytes()
            .next()
            .is_some_and(|byte| byte.is_ascii_alphanumeric() || byte == b'_')
        {
            self.offset += 1;
        }
        let value = &self.source[start..self.offset];
        let kind = if value.eq_ignore_ascii_case("@starter") {
            TokenKind::StarterAnnotation
        } else if value.eq_ignore_ascii_case("@return") || value.eq_ignore_ascii_case("@reply") {
            TokenKind::ReturnAnnotation
        } else {
            TokenKind::Annotation(value.trim_start_matches('@').to_string())
        };
        self.title_allowed = false;
        Token::new(kind, start, self.offset)
    }

    fn color(&mut self) -> Option<Token> {
        let start = self.offset;
        self.offset += 1;
        while self
            .rest()
            .bytes()
            .next()
            .is_some_and(|byte| byte.is_ascii_hexdigit())
        {
            self.offset += 1;
        }
        if self.offset == start + 1 {
            self.offset = start;
            return None;
        }
        self.title_allowed = false;
        Some(Token::new(
            TokenKind::Color(self.source[start..self.offset].to_string()),
            start,
            self.offset,
        ))
    }

    fn comment(&mut self) -> Token {
        let start = self.offset;
        self.offset += 2;
        let body_start = self.offset;
        while self
            .rest()
            .chars()
            .next()
            .is_some_and(|ch| !matches!(ch, '\r' | '\n'))
        {
            self.bump_char();
        }
        let value = self.source[body_start..self.offset].to_string();
        Token::new(TokenKind::Comment(value), start, self.offset).on_channel(TokenChannel::Comment)
    }

    fn divider(&mut self, start: usize) -> Token {
        while self
            .rest()
            .chars()
            .next()
            .is_some_and(|ch| !matches!(ch, '\r' | '\n'))
        {
            self.bump_char();
        }
        self.title_allowed = false;
        Token::new(
            TokenKind::Divider(self.source[start..self.offset].trim().to_string()),
            start,
            self.offset,
        )
    }

    fn event_payload(&mut self) -> Option<Token> {
        let start = self.offset;
        while self
            .rest()
            .chars()
            .next()
            .is_some_and(|ch| !matches!(ch, '\r' | '\n'))
        {
            self.bump_char();
        }
        (start < self.offset).then(|| {
            Token::new(
                TokenKind::EventPayload(self.source[start..self.offset].trim().to_string()),
                start,
                self.offset,
            )
        })
    }

    fn title_content(&mut self) -> Option<Token> {
        let start = self.offset;
        while self
            .rest()
            .chars()
            .next()
            .is_some_and(|ch| !matches!(ch, '\r' | '\n'))
        {
            self.bump_char();
        }
        (start < self.offset).then(|| {
            Token::new(
                TokenKind::TitleContent(self.source[start..self.offset].to_string()),
                start,
                self.offset,
            )
        })
    }

    fn mode_end(&mut self, kind: TokenKind) -> Token {
        let start = self.offset;
        self.bump_char();
        self.line_start = self.offset;
        self.mode = LexerMode::Default;
        Token::new(kind, start, self.offset)
    }

    fn fixed(&mut self, width: usize, kind: TokenKind) -> Token {
        let start = self.offset;
        self.offset += width;
        self.title_allowed = false;
        Token::new(kind, start, self.offset)
    }
}

impl Iterator for Lexer<'_> {
    type Item = Token;

    fn next(&mut self) -> Option<Self::Item> {
        match self.mode {
            LexerMode::Event if matches!(self.rest().chars().next(), Some('\r' | '\n')) => {
                return Some(self.mode_end(TokenKind::EventEnd));
            }
            LexerMode::Event => {
                if let Some(payload) = self.event_payload() {
                    return Some(payload);
                }
            }
            LexerMode::Title if matches!(self.rest().chars().next(), Some('\r' | '\n')) => {
                return Some(self.mode_end(TokenKind::TitleEnd));
            }
            LexerMode::Title => {
                if let Some(content) = self.title_content() {
                    return Some(content);
                }
            }
            LexerMode::Default => {}
        }
        if self.offset >= self.source.len() {
            if self.emitted_eof {
                return None;
            }
            self.emitted_eof = true;
            return Some(Token::new(TokenKind::Eof, self.offset, self.offset));
        }

        let before_space = self.offset;
        self.skip_horizontal_space();
        if self.offset >= self.source.len() {
            return self.next();
        }
        if before_space == self.line_start && self.starts_with("==") {
            return Some(self.divider(before_space));
        }

        let start = self.offset;
        let ch = self.rest().chars().next()?;
        if matches!(ch, '\r' | '\n') {
            if ch == '\r' && self.starts_with("\r\n") {
                self.offset += 2;
            } else {
                self.bump_char();
            }
            self.line_start = self.offset;
            return Some(
                Token::new(TokenKind::LineBreak, start, self.offset)
                    .on_channel(TokenChannel::Hidden),
            );
        }
        if self.starts_with("//") {
            return Some(self.comment());
        }
        if is_identifier_start(ch) {
            return Some(self.word());
        }
        if ch.is_ascii_digit() {
            return Some(self.number());
        }
        if ch == '.' {
            return Some(self.dot_or_number());
        }
        if ch == '$'
            && let Some(money) = self.money()
        {
            return Some(money);
        }
        if ch == '"' {
            return Some(self.string());
        }
        if ch == '@' {
            return Some(self.annotation());
        }
        if ch == '#'
            && let Some(color) = self.color()
        {
            return Some(color);
        }

        for (text, kind) in [
            ("-->", TokenKind::ReturnArrow),
            ("->", TokenKind::Arrow),
            ("<<", TokenKind::StereotypeOpen),
            (">>", TokenKind::StereotypeClose),
            (">=", TokenKind::Operator(">=".to_string())),
            ("<=", TokenKind::Operator("<=".to_string())),
            ("==", TokenKind::Operator("==".to_string())),
            ("!=", TokenKind::Operator("!=".to_string())),
            ("&&", TokenKind::Operator("&&".to_string())),
            ("||", TokenKind::Operator("||".to_string())),
        ] {
            if self.starts_with(text) {
                return Some(self.fixed(text.len(), kind));
            }
        }

        let token = match ch {
            ':' => {
                let token = self.fixed(1, TokenKind::Colon);
                self.mode = LexerMode::Event;
                return Some(token);
            }
            ';' => self.fixed(1, TokenKind::Semicolon),
            ',' => self.fixed(1, TokenKind::Comma),
            '=' => self.fixed(1, TokenKind::Assign),
            '(' => self.fixed(1, TokenKind::OpenParen),
            ')' => self.fixed(1, TokenKind::CloseParen),
            '{' => self.fixed(1, TokenKind::OpenBrace),
            '}' => self.fixed(1, TokenKind::CloseBrace),
            '[' => self.fixed(1, TokenKind::OpenBracket),
            ']' => self.fixed(1, TokenKind::CloseBracket),
            '+' | '-' | '*' | '/' | '%' | '^' | '!' | '>' | '<' => {
                self.bump_char();
                self.title_allowed = false;
                Token::new(TokenKind::Operator(ch.to_string()), start, self.offset)
            }
            _ => {
                self.bump_char();
                self.title_allowed = false;
                Token::new(TokenKind::Other(ch), start, self.offset)
            }
        };
        Some(token)
    }
}

fn is_identifier_start(ch: char) -> bool {
    ch == '_' || is_unicode_letter(ch)
}

fn is_identifier_continue(ch: char) -> bool {
    ch == '_' || is_unicode_letter(ch) || is_unicode_decimal_number(ch)
}

fn is_unicode_letter(ch: char) -> bool {
    matches!(
        get_general_category(ch),
        GeneralCategory::UppercaseLetter
            | GeneralCategory::LowercaseLetter
            | GeneralCategory::TitlecaseLetter
            | GeneralCategory::ModifierLetter
            | GeneralCategory::OtherLetter
    )
}

fn is_unicode_decimal_number(ch: char) -> bool {
    get_general_category(ch) == GeneralCategory::DecimalNumber
}

fn scan_ascii_digits(source: &str, mut offset: usize) -> usize {
    while source
        .as_bytes()
        .get(offset)
        .is_some_and(u8::is_ascii_digit)
    {
        offset += 1;
    }
    offset
}

fn digit_leading_name_end(source: &str, digits_end: usize) -> Option<usize> {
    let first = source[digits_end..].chars().next()?;
    if !is_unicode_letter(first) {
        return None;
    }
    let mut end = digits_end + first.len_utf8();
    while let Some(ch) = source[end..].chars().next() {
        if !is_identifier_continue(ch) {
            break;
        }
        end += ch.len_utf8();
    }
    Some(end)
}

fn known_unit_end(source: &str, number_end: usize) -> Option<usize> {
    KNOWN_UNITS
        .iter()
        .filter(|unit| source[number_end..].starts_with(**unit))
        .max_by_key(|unit| unit.len())
        .map(|unit| number_end + unit.len())
}

fn keyword(value: &str, title_allowed: bool, source: &str, after_word: usize) -> Option<Keyword> {
    Some(match value {
        "as" => Keyword::As,
        "catch" => Keyword::Catch,
        "critical" => Keyword::Critical,
        "else" => Keyword::Else,
        "false" => Keyword::False,
        "finally" => Keyword::Finally,
        "group" => Keyword::Group,
        "if" => Keyword::If,
        "in" => Keyword::In,
        "new" => Keyword::New,
        "nil" | "null" => Keyword::Nil,
        "opt" => Keyword::Opt,
        "par" => Keyword::Par,
        "ref" => Keyword::Ref,
        "return" => Keyword::Return,
        "section" | "frame" => Keyword::Section,
        "true" => Keyword::True,
        "try" => Keyword::Try,
        "while" | "for" | "foreach" | "forEach" | "loop" => Keyword::While,
        "title" if title_allowed && title_right_edge_is_directive(source, after_word) => {
            Keyword::Title
        }
        _ => return None,
    })
}

fn title_right_edge_is_directive(source: &str, mut offset: usize) -> bool {
    while source[offset..]
        .chars()
        .next()
        .is_some_and(|ch| matches!(ch, ' ' | '\t'))
    {
        offset += source[offset..].chars().next().map_or(0, char::len_utf8);
    }
    !source[offset..]
        .chars()
        .next()
        .is_some_and(|ch| matches!(ch, '.' | '(' | '='))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn title_predicate_distinguishes_directive_from_method() {
        let directive = lex("title Order Service\n");
        assert!(matches!(
            directive[0].kind,
            TokenKind::Keyword(Keyword::Title)
        ));

        let method = lex("title.render()\n");
        assert!(matches!(method[0].kind, TokenKind::Identifier(_)));
    }

    #[test]
    fn unicode_names_and_unclosed_strings_follow_the_companion_lexer_rules() {
        let tokens = lex("客户.创建(\"订单\n");
        assert!(matches!(tokens[0].kind, TokenKind::Identifier(ref v) if v == "客户"));
        assert!(
            tokens
                .iter()
                .any(|token| matches!(token.kind, TokenKind::StringLiteral { closed: false, .. }))
        );
    }

    #[test]
    fn oracle_modifier_channel_is_preserved_for_editor_and_hidden_from_parser() {
        let tokens = lex("const result = await Service.call()\n");
        assert!(
            tokens
                .iter()
                .any(|token| matches!(token.kind, TokenKind::Modifier)
                    && token.channel == TokenChannel::Modifier)
        );
        let parser_tokens = parser_tokens(&tokens);
        assert!(
            !parser_tokens
                .iter()
                .any(|token| matches!(token.kind, TokenKind::Modifier))
        );
        assert!(
            matches!(parser_tokens[0].kind, TokenKind::Identifier(ref value) if value == "result")
        );
    }

    #[test]
    fn companion_matrix_distinguishes_units_digit_names_and_float_right_edges() {
        let tokens = lex("1kg 100day 0.5h .5m 10ms 5xx 2FAService 404Page 1_000 .5abc");
        let visible = tokens
            .iter()
            .filter(|token| !matches!(token.kind, TokenKind::Eof))
            .map(|token| &token.kind)
            .collect::<Vec<_>>();

        assert!(matches!(visible[0], TokenKind::NumberUnit(value) if value == "1kg"));
        assert!(matches!(visible[1], TokenKind::NumberUnit(value) if value == "100day"));
        assert!(matches!(visible[2], TokenKind::NumberUnit(value) if value == "0.5h"));
        assert!(matches!(visible[3], TokenKind::NumberUnit(value) if value == ".5m"));
        assert!(matches!(visible[4], TokenKind::NumberUnit(value) if value == "10ms"));
        assert!(matches!(visible[5], TokenKind::DigitLeadingName(value) if value == "5xx"));
        assert!(matches!(visible[6], TokenKind::DigitLeadingName(value) if value == "2FAService"));
        assert!(matches!(visible[7], TokenKind::DigitLeadingName(value) if value == "404Page"));
        assert!(matches!(visible[8], TokenKind::Integer(value) if value == "1"));
        assert!(matches!(visible[9], TokenKind::Identifier(value) if value == "_000"));
        assert!(matches!(visible[10], TokenKind::Dot));
        assert!(matches!(visible[11], TokenKind::DigitLeadingName(value) if value == "5abc"));
    }

    #[test]
    fn companion_matrix_known_unit_corpus_uses_the_number_unit_token() {
        for unit in KNOWN_UNITS {
            let source = format!("1{unit}");
            let tokens = lex(&source);
            assert!(
                matches!(&tokens[0].kind, TokenKind::NumberUnit(value) if value == &source),
                "{source}: {:?}",
                tokens
            );
        }
    }

    #[test]
    fn bracket_emoji_uses_real_lexer_tokens_and_colon_override_enters_event_mode() {
        let tokens = lex("[rocket] Production\n[:red:] Alert\n");
        assert!(matches!(tokens[0].kind, TokenKind::OpenBracket));
        assert!(matches!(tokens[1].kind, TokenKind::Identifier(ref value) if value == "rocket"));
        assert!(matches!(tokens[2].kind, TokenKind::CloseBracket));
        let colon = tokens
            .iter()
            .position(|token| matches!(token.kind, TokenKind::Colon))
            .expect("colon override token");
        assert!(matches!(tokens[colon + 1].kind, TokenKind::EventPayload(_)));
    }

    #[test]
    fn identifiers_use_unicode_letter_and_decimal_number_categories_exactly() {
        let tokens = lex("A\u{0345} A١ ʰName ١A AⅫ");
        let visible = tokens
            .iter()
            .filter(|token| !matches!(token.kind, TokenKind::Eof))
            .map(|token| &token.kind)
            .collect::<Vec<_>>();

        assert!(matches!(visible[0], TokenKind::Identifier(value) if value == "A"));
        assert!(matches!(visible[1], TokenKind::Other('\u{0345}')));
        assert!(matches!(visible[2], TokenKind::Identifier(value) if value == "A١"));
        assert!(matches!(visible[3], TokenKind::Identifier(value) if value == "ʰName"));
        assert!(matches!(visible[4], TokenKind::Other('١')));
        assert!(matches!(visible[5], TokenKind::Identifier(value) if value == "A"));
        assert!(matches!(visible[6], TokenKind::Identifier(value) if value == "A"));
        assert!(matches!(visible[7], TokenKind::Other('Ⅻ')));
    }

    #[test]
    fn color_requires_at_least_one_hex_digit() {
        let tokens = lex("# #abc");
        assert!(matches!(tokens[0].kind, TokenKind::Other('#')));
        assert!(matches!(tokens[1].kind, TokenKind::Color(ref value) if value == "#abc"));
    }
}
