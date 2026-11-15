use crate::svg::pipeline::SvgPostprocessExecution;
use crate::{Error, Result};
use cssparser::{
    AtRuleParser, BasicParseErrorKind, CowRcStr, Delimiter, ParseError, ParseErrorKind, Parser,
    ParserState, QualifiedRuleParser, StyleSheetParser, Token,
};
use std::fmt;

const SCOPED_CSS_CHECKPOINT_BATCH: usize = 64;
const SCOPED_CSS_NESTING_HARD_LIMIT: u8 = 64;
const STYLE_END_ESCAPE_PATTERN: &[u8] = b"</style";
const STYLE_END_ESCAPE_REPLACEMENT: &str = "\\3c /style";

pub(super) fn projected_css_bytes(
    css: &str,
    scope: Option<&str>,
    execution: SvgPostprocessExecution<'_>,
) -> Result<usize> {
    let mut projected = ProjectedBytes::default();
    {
        let mut escaped = StyleEndEscaper::new(&mut projected);
        write_scoped_css(css, scope, &mut escaped, execution)?;
        escaped.finish().map_err(scoped_css_write_error)?;
    }
    projected
        .finish()
        .ok_or_else(|| execution.svg_byte_count_overflow())
}

pub(super) fn materialize_css(
    css: &str,
    scope: Option<&str>,
    output: &mut String,
    execution: SvgPostprocessExecution<'_>,
) -> Result<()> {
    let mut escaped = StyleEndEscaper::new(output);
    write_scoped_css(css, scope, &mut escaped, execution)?;
    escaped.finish().map_err(scoped_css_write_error)
}

#[derive(Default)]
struct ProjectedBytes {
    bytes: usize,
    overflowed: bool,
}

impl ProjectedBytes {
    fn finish(self) -> Option<usize> {
        (!self.overflowed).then_some(self.bytes)
    }
}

impl fmt::Write for ProjectedBytes {
    fn write_str(&mut self, value: &str) -> fmt::Result {
        if !self.overflowed {
            match self.bytes.checked_add(value.len()) {
                Some(bytes) => self.bytes = bytes,
                None => self.overflowed = true,
            }
        }
        Ok(())
    }
}

struct StyleEndEscaper<'a, W> {
    output: &'a mut W,
    pending: [u8; STYLE_END_ESCAPE_PATTERN.len()],
    pending_len: usize,
}

impl<'a, W: fmt::Write> StyleEndEscaper<'a, W> {
    fn new(output: &'a mut W) -> Self {
        Self {
            output,
            pending: [0; STYLE_END_ESCAPE_PATTERN.len()],
            pending_len: 0,
        }
    }

    fn finish(mut self) -> fmt::Result {
        self.flush_pending()
    }

    fn flush_pending(&mut self) -> fmt::Result {
        if self.pending_len != 0 {
            let pending = std::str::from_utf8(&self.pending[..self.pending_len])
                .expect("the style terminator prefix is ASCII");
            self.output.write_str(pending)?;
            self.pending_len = 0;
        }
        Ok(())
    }

    fn write_chunk(&mut self, value: &str) -> fmt::Result {
        let bytes = value.as_bytes();
        let mut cursor = 0usize;
        while cursor < bytes.len() {
            if self.pending_len == 0 {
                let Some(relative) = bytes[cursor..].iter().position(|byte| *byte == b'<') else {
                    self.output.write_str(&value[cursor..])?;
                    return Ok(());
                };
                let next = cursor + relative;
                self.output.write_str(&value[cursor..next])?;
                self.pending[0] = b'<';
                self.pending_len = 1;
                cursor = next + 1;
                continue;
            }

            let byte = bytes[cursor];
            if byte == STYLE_END_ESCAPE_PATTERN[self.pending_len] {
                self.pending[self.pending_len] = byte;
                self.pending_len += 1;
                cursor += 1;
                if self.pending_len == STYLE_END_ESCAPE_PATTERN.len() {
                    self.output.write_str(STYLE_END_ESCAPE_REPLACEMENT)?;
                    self.pending_len = 0;
                }
                continue;
            }

            self.flush_pending()?;
        }
        Ok(())
    }
}

impl<W: fmt::Write> fmt::Write for StyleEndEscaper<'_, W> {
    fn write_str(&mut self, value: &str) -> fmt::Result {
        self.write_chunk(value)
    }
}

struct ScopedCssCadence<'a> {
    execution: SvgPostprocessExecution<'a>,
    iterations: usize,
}

impl<'a> ScopedCssCadence<'a> {
    const fn new(execution: SvgPostprocessExecution<'a>) -> Self {
        Self {
            execution,
            iterations: 0,
        }
    }

    fn checkpoint(&self) -> Result<()> {
        self.execution.checkpoint()
    }

    fn tick(&mut self) -> Result<()> {
        if self.iterations.is_multiple_of(SCOPED_CSS_CHECKPOINT_BATCH) {
            self.checkpoint()?;
        }
        self.iterations = self.iterations.wrapping_add(1);
        Ok(())
    }

    fn step(&mut self) -> std::result::Result<(), ParseError<Error>> {
        self.tick().map_err(ParseError::custom)
    }
}

fn consume_css_component_values<'i>(
    input: &mut Parser<'i>,
    source: &'i str,
    expected_close: Option<u8>,
    depth: u8,
    cadence: &mut ScopedCssCadence<'_>,
) -> std::result::Result<(), ParseError<Error>> {
    loop {
        cadence.step()?;
        let token = match input.next_including_whitespace_and_comments() {
            Ok(token) => token.clone(),
            Err(error) if matches!(error.kind, BasicParseErrorKind::EndOfInput) => {
                if let Some(expected_close) = expected_close
                    && source
                        .as_bytes()
                        .get(input.position().byte_index())
                        .copied()
                        != Some(expected_close)
                {
                    return Err(ParseError::custom(Error::svg_postprocess(
                        "scoped-css",
                        "invalid scoped CSS: unclosed block or function",
                    )));
                }
                return Ok(());
            }
            Err(error) => return Err(error.into()),
        };

        let expected_close = match token {
            Token::Function(_) | Token::ParenthesisBlock => Some(b')'),
            Token::SquareBracketBlock => Some(b']'),
            Token::CurlyBracketBlock => Some(b'}'),
            Token::BadUrl(_)
            | Token::BadString(_)
            | Token::CloseParenthesis
            | Token::CloseSquareBracket
            | Token::CloseCurlyBracket => {
                return Err(ParseError::custom(Error::svg_postprocess(
                    "scoped-css",
                    "invalid scoped CSS token",
                )));
            }
            _ => None,
        };
        if let Some(expected_close) = expected_close {
            let nested_depth = descend_scoped_css(depth)?;
            input.parse_nested_block(|nested| {
                consume_css_component_values(
                    nested,
                    source,
                    Some(expected_close),
                    nested_depth,
                    cadence,
                )
            })?;
        }
    }
}

fn descend_scoped_css(depth: u8) -> std::result::Result<u8, ParseError<Error>> {
    depth
        .checked_add(1)
        .filter(|depth| *depth <= SCOPED_CSS_NESTING_HARD_LIMIT)
        .ok_or_else(|| {
            ParseError::custom(Error::svg_postprocess(
                "scoped-css",
                format!(
                    "scoped CSS nesting exceeds the hard limit of {SCOPED_CSS_NESTING_HARD_LIMIT}"
                ),
            ))
        })
}

fn write_scoped_css<W: fmt::Write>(
    css: &str,
    scope: Option<&str>,
    output: &mut W,
    execution: SvgPostprocessExecution<'_>,
) -> Result<()> {
    let Some(scope) = scope else {
        let mut input = Parser::new(css);
        let mut cadence = ScopedCssCadence::new(execution);
        consume_css_component_values(&mut input, css, None, 0, &mut cadence)
            .map_err(map_scoped_css_parse_error)?;
        cadence.checkpoint()?;
        execution.checkpoint()?;
        output.write_str(css).map_err(scoped_css_write_error)?;
        execution.checkpoint()?;
        return Ok(());
    };

    let mut input = Parser::new(css);
    let mut cadence = ScopedCssCadence::new(execution);
    write_scoped_rule_list(&mut input, css, scope, output, 0, &mut cadence)
        .map_err(map_scoped_css_parse_error)?;
    cadence.checkpoint()
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ScopedAtRuleKind {
    Forbidden,
    Group,
    Keyframes,
    Unsupported,
}

struct ScopedAtRulePrelude<'i> {
    name: CowRcStr<'i>,
    prelude: &'i str,
    kind: ScopedAtRuleKind,
}

struct ScopedCssRuleParser<'scope, 'source, 'output, 'cadence, 'execution, W> {
    source: &'source str,
    scope: &'scope str,
    output: &'output mut W,
    depth: u8,
    cadence: &'cadence mut ScopedCssCadence<'execution>,
}

impl<'i, 'execution, W: fmt::Write> AtRuleParser<'i>
    for ScopedCssRuleParser<'_, 'i, '_, '_, 'execution, W>
{
    type Prelude = ScopedAtRulePrelude<'i>;
    type AtRule = ();
    type Error = Error;

    fn parse_prelude(
        &mut self,
        name: CowRcStr<'i>,
        input: &mut Parser<'i>,
    ) -> std::result::Result<Self::Prelude, ParseError<Self::Error>> {
        let start = input.position();
        consume_css_component_values(input, self.source, None, self.depth, self.cadence)?;
        let normalized_name = name.to_ascii_lowercase();
        let kind = if matches!(normalized_name.as_str(), "keyframes" | "-webkit-keyframes") {
            ScopedAtRuleKind::Keyframes
        } else if matches!(
            normalized_name.as_str(),
            "media" | "supports" | "layer" | "scope" | "container" | "starting-style"
        ) {
            ScopedAtRuleKind::Group
        } else if matches!(normalized_name.as_str(), "import" | "namespace" | "charset") {
            ScopedAtRuleKind::Forbidden
        } else {
            ScopedAtRuleKind::Unsupported
        };
        Ok(ScopedAtRulePrelude {
            name,
            prelude: input.slice_from(start),
            kind,
        })
    }

    fn rule_without_block(
        &mut self,
        prelude: Self::Prelude,
        _start: &ParserState,
    ) -> std::result::Result<Self::AtRule, ()> {
        match prelude.kind {
            ScopedAtRuleKind::Forbidden => Ok(()),
            ScopedAtRuleKind::Group if prelude.name.eq_ignore_ascii_case("layer") => {
                write!(self.output, "@{}{};", prelude.name, prelude.prelude).map_err(|_| ())
            }
            ScopedAtRuleKind::Unsupported => {
                write!(self.output, "@{}{};", prelude.name, prelude.prelude).map_err(|_| ())
            }
            ScopedAtRuleKind::Group | ScopedAtRuleKind::Keyframes => Err(()),
        }
    }

    fn parse_block(
        &mut self,
        prelude: Self::Prelude,
        _start: &ParserState,
        input: &mut Parser<'i>,
    ) -> std::result::Result<Self::AtRule, ParseError<Self::Error>> {
        match prelude.kind {
            ScopedAtRuleKind::Forbidden | ScopedAtRuleKind::Unsupported => {
                consume_css_component_values(input, self.source, None, self.depth, self.cadence)?;
                require_source_close(input, self.source, b'}')
            }
            ScopedAtRuleKind::Keyframes => {
                write_css(self.output, "@")?;
                write_css(self.output, &prelude.name)?;
                write_css(self.output, prelude.prelude)?;
                write_css(self.output, "{")?;
                let body_start = input.position();
                consume_css_component_values(input, self.source, None, self.depth, self.cadence)?;
                write_css(self.output, input.slice_from(body_start))?;
                require_source_close(input, self.source, b'}')?;
                write_css(self.output, "}")
            }
            ScopedAtRuleKind::Group => {
                write_css(self.output, "@")?;
                write_css(self.output, &prelude.name)?;
                write_css(self.output, prelude.prelude)?;
                write_css(self.output, "{")?;
                let nested_depth = descend_scoped_css(self.depth)?;
                write_scoped_rule_list(
                    input,
                    self.source,
                    self.scope,
                    self.output,
                    nested_depth,
                    self.cadence,
                )?;
                require_source_close(input, self.source, b'}')?;
                write_css(self.output, "}")
            }
        }
    }
}

impl<'i, 'execution, W: fmt::Write> QualifiedRuleParser<'i>
    for ScopedCssRuleParser<'_, 'i, '_, '_, 'execution, W>
{
    type Prelude = &'i str;
    type QualifiedRule = ();
    type Error = Error;

    fn parse_prelude(
        &mut self,
        input: &mut Parser<'i>,
    ) -> std::result::Result<Self::Prelude, ParseError<Self::Error>> {
        let start = input.position();
        consume_css_component_values(input, self.source, None, self.depth, self.cadence)?;
        Ok(input.slice_from(start))
    }

    fn parse_block(
        &mut self,
        selector: Self::Prelude,
        _start: &ParserState,
        input: &mut Parser<'i>,
    ) -> std::result::Result<Self::QualifiedRule, ParseError<Self::Error>> {
        let body_start = input.position();
        consume_css_component_values(input, self.source, None, self.depth, self.cadence)?;
        let body = input.slice_from(body_start);
        write_selector_list(
            selector,
            body,
            self.scope,
            self.output,
            self.depth,
            self.cadence,
        )
        .map_err(ParseError::custom)?;
        write_css(self.output, " {")?;
        write_css(self.output, body)?;
        require_source_close(input, self.source, b'}')?;
        write_css(self.output, "}")
    }
}

fn write_scoped_rule_list<'i, W: fmt::Write>(
    input: &mut Parser<'i>,
    source: &'i str,
    scope: &str,
    output: &mut W,
    depth: u8,
    cadence: &mut ScopedCssCadence<'_>,
) -> std::result::Result<(), ParseError<Error>> {
    cadence.step()?;
    if depth >= SCOPED_CSS_NESTING_HARD_LIMIT {
        return Err(ParseError::custom(Error::svg_postprocess(
            "scoped-css",
            format!(
                "scoped CSS rule nesting exceeds the hard limit of {SCOPED_CSS_NESTING_HARD_LIMIT}"
            ),
        )));
    }
    let mut parser = ScopedCssRuleParser {
        source,
        scope,
        output,
        depth,
        cadence,
    };
    for rule in StyleSheetParser::new(input, &mut parser) {
        rule.map_err(|(error, _, _)| error)?;
    }
    Ok(())
}

fn require_source_close<'i>(
    input: &Parser<'i>,
    source: &'i str,
    expected_close: u8,
) -> std::result::Result<(), ParseError<Error>> {
    if source
        .as_bytes()
        .get(input.position().byte_index())
        .copied()
        == Some(expected_close)
    {
        Ok(())
    } else {
        Err(ParseError::custom(Error::svg_postprocess(
            "scoped-css",
            "invalid scoped CSS: unclosed rule block",
        )))
    }
}

fn write_selector_list<W: fmt::Write>(
    selector: &str,
    body: &str,
    scope: &str,
    output: &mut W,
    depth: u8,
    cadence: &mut ScopedCssCadence<'_>,
) -> Result<()> {
    let mut safe_root_declarations = None;
    let mut input = Parser::new(selector);
    let mut part_start = 0usize;
    loop {
        cadence.step().map_err(map_scoped_css_parse_error)?;
        let token_start = input.position().byte_index();
        let token = match input.next_including_whitespace_and_comments() {
            Ok(token) => token.clone(),
            Err(error) if matches!(error.kind, BasicParseErrorKind::EndOfInput) => {
                write_selector_part(
                    &selector[part_start..],
                    body,
                    scope,
                    &mut safe_root_declarations,
                    output,
                    depth,
                    cadence,
                )?;
                return Ok(());
            }
            Err(error) => return Err(map_scoped_css_parse_error(error.into())),
        };
        let token_end = input.position().byte_index();
        match token {
            Token::Comma => {
                write_selector_part(
                    &selector[part_start..token_start],
                    body,
                    scope,
                    &mut safe_root_declarations,
                    output,
                    depth,
                    cadence,
                )?;
                output.write_str(", ").map_err(scoped_css_write_error)?;
                part_start = token_end;
            }
            Token::Function(_) | Token::ParenthesisBlock => {
                let nested_depth = descend_scoped_css(depth).map_err(map_scoped_css_parse_error)?;
                input
                    .parse_nested_block(|nested| {
                        consume_css_component_values(
                            nested,
                            selector,
                            Some(b')'),
                            nested_depth,
                            cadence,
                        )
                    })
                    .map_err(map_scoped_css_parse_error)?;
            }
            Token::SquareBracketBlock => {
                let nested_depth = descend_scoped_css(depth).map_err(map_scoped_css_parse_error)?;
                input
                    .parse_nested_block(|nested| {
                        consume_css_component_values(
                            nested,
                            selector,
                            Some(b']'),
                            nested_depth,
                            cadence,
                        )
                    })
                    .map_err(map_scoped_css_parse_error)?;
            }
            Token::CurlyBracketBlock => {
                let nested_depth = descend_scoped_css(depth).map_err(map_scoped_css_parse_error)?;
                input
                    .parse_nested_block(|nested| {
                        consume_css_component_values(
                            nested,
                            selector,
                            Some(b'}'),
                            nested_depth,
                            cadence,
                        )
                    })
                    .map_err(map_scoped_css_parse_error)?;
            }
            Token::BadUrl(_)
            | Token::BadString(_)
            | Token::CloseParenthesis
            | Token::CloseSquareBracket
            | Token::CloseCurlyBracket => {
                return Err(Error::svg_postprocess(
                    "scoped-css",
                    "invalid scoped CSS selector token",
                ));
            }
            _ => {}
        }
    }
}

fn write_selector_part<W: fmt::Write>(
    selector: &str,
    body: &str,
    scope: &str,
    safe_root_declarations: &mut Option<bool>,
    output: &mut W,
    depth: u8,
    cadence: &mut ScopedCssCadence<'_>,
) -> Result<()> {
    let selector = trim_css_whitespace(selector, cadence)?;
    if selector.is_empty() {
        return Err(Error::svg_postprocess(
            "scoped-css",
            "invalid scoped CSS: empty selector",
        ));
    }
    if matches!(selector, ":root" | "svg") {
        output.write_str(scope).map_err(scoped_css_write_error)?;
        return Ok(());
    }

    let safe_root_declarations = if selector == "&" || selector == scope {
        match *safe_root_declarations {
            Some(safe) => safe,
            None => {
                let safe = has_only_safe_root_declarations(body, depth, cadence)?;
                *safe_root_declarations = Some(safe);
                safe
            }
        }
    } else {
        false
    };
    if !selector_is_already_namespaced(selector, scope, safe_root_declarations, cadence)? {
        output.write_str(scope).map_err(scoped_css_write_error)?;
        output.write_char(' ').map_err(scoped_css_write_error)?;
    }
    write_expanded_selector(selector, scope, output, depth, cadence)
}

fn selector_is_already_namespaced(
    selector: &str,
    scope: &str,
    safe_root_declarations: bool,
    cadence: &mut ScopedCssCadence<'_>,
) -> Result<bool> {
    if selector == "&" || selector == scope {
        return Ok(safe_root_declarations);
    }
    if let Some(suffix) = selector.strip_prefix(scope) {
        return Ok(is_namespaced_suffix(suffix));
    }

    let mut input = Parser::new(selector);
    cadence.step().map_err(map_scoped_css_parse_error)?;
    if !matches!(input.next(), Ok(Token::Delim('&'))) {
        return Ok(false);
    }
    let suffix = &selector[input.position().byte_index()..];
    Ok(is_namespaced_suffix(suffix))
}

fn is_namespaced_suffix(suffix: &str) -> bool {
    if suffix.starts_with('>') {
        return true;
    }
    let Some(first) = suffix.chars().next() else {
        return false;
    };
    if !is_css_whitespace(first) {
        return false;
    }
    let descendant = suffix.trim_start_matches(is_css_whitespace);
    !descendant.is_empty()
        && !descendant.starts_with('+')
        && !descendant.starts_with('~')
        && !descendant.starts_with("||")
}

fn is_css_whitespace(character: char) -> bool {
    matches!(character, ' ' | '\n' | '\r' | '\t' | '\u{000C}')
}

fn trim_css_whitespace<'a>(value: &'a str, cadence: &mut ScopedCssCadence<'_>) -> Result<&'a str> {
    let mut start = value.len();
    for (index, character) in value.char_indices() {
        cadence.tick()?;
        if !is_css_whitespace(character) {
            start = index;
            break;
        }
    }
    if start == value.len() {
        cadence.checkpoint()?;
        return Ok(&value[value.len()..]);
    }
    let mut end = value.len();
    for (index, character) in value[start..].char_indices().rev() {
        cadence.tick()?;
        if !is_css_whitespace(character) {
            end = start + index + character.len_utf8();
            break;
        }
    }
    Ok(&value[start..end])
}

fn write_expanded_selector<W: fmt::Write>(
    selector: &str,
    scope: &str,
    output: &mut W,
    depth: u8,
    cadence: &mut ScopedCssCadence<'_>,
) -> Result<()> {
    let mut input = Parser::new(selector);
    write_expanded_selector_parser(&mut input, scope, output, depth, cadence)
        .map_err(map_scoped_css_parse_error)
}

fn write_expanded_selector_parser<'i, W: fmt::Write>(
    input: &mut Parser<'i>,
    scope: &str,
    output: &mut W,
    depth: u8,
    cadence: &mut ScopedCssCadence<'_>,
) -> std::result::Result<(), ParseError<Error>> {
    loop {
        cadence.step()?;
        let token_start = input.position();
        let token = match input.next_including_whitespace_and_comments() {
            Ok(token) => token.clone(),
            Err(error) if matches!(error.kind, BasicParseErrorKind::EndOfInput) => return Ok(()),
            Err(error) => return Err(error.into()),
        };
        let token_end = input.position();
        match token {
            Token::Delim('&') => write_css(output, scope)?,
            Token::Function(_) | Token::ParenthesisBlock => {
                write_css(output, input.slice(token_start..token_end))?;
                let nested_depth = descend_scoped_css(depth)?;
                input.parse_nested_block(|nested| {
                    write_expanded_selector_parser(nested, scope, output, nested_depth, cadence)
                })?;
                write_css(output, ")")?;
            }
            Token::SquareBracketBlock => {
                write_css(output, input.slice(token_start..token_end))?;
                let nested_depth = descend_scoped_css(depth)?;
                input.parse_nested_block(|nested| {
                    write_expanded_selector_parser(nested, scope, output, nested_depth, cadence)
                })?;
                write_css(output, "]")?;
            }
            Token::CurlyBracketBlock => {
                write_css(output, input.slice(token_start..token_end))?;
                let nested_depth = descend_scoped_css(depth)?;
                input.parse_nested_block(|nested| {
                    write_expanded_selector_parser(nested, scope, output, nested_depth, cadence)
                })?;
                write_css(output, "}")?;
            }
            Token::BadUrl(_)
            | Token::BadString(_)
            | Token::CloseParenthesis
            | Token::CloseSquareBracket
            | Token::CloseCurlyBracket => {
                return Err(ParseError::custom(Error::svg_postprocess(
                    "scoped-css",
                    "invalid scoped CSS selector token",
                )));
            }
            _ => write_css(output, input.slice(token_start..token_end))?,
        }
    }
}

fn has_only_safe_root_declarations(
    body: &str,
    depth: u8,
    cadence: &mut ScopedCssCadence<'_>,
) -> Result<bool> {
    let mut parser = Parser::new(body);
    while !parser.is_exhausted() {
        cadence.step().map_err(map_scoped_css_parse_error)?;
        if parser
            .try_parse(|declaration| declaration.expect_semicolon())
            .is_ok()
        {
            continue;
        }
        let allowed = parser.parse_until_after(Delimiter::Semicolon, |declaration| {
            cadence.step()?;
            let property = declaration.expect_ident_cloned()?;
            declaration.expect_colon()?;
            consume_css_component_values(declaration, body, None, depth, cadence)?;
            Ok(matches!(
                property.as_ref(),
                "font-family" | "font-size" | "fill"
            ))
        });
        match allowed {
            Ok(true) => {}
            Ok(false) => return Ok(false),
            Err(error) => return Err(map_scoped_css_parse_error(error)),
        }
    }
    Ok(true)
}

fn write_css(
    output: &mut impl fmt::Write,
    value: &str,
) -> std::result::Result<(), ParseError<Error>> {
    output
        .write_str(value)
        .map_err(|error| ParseError::custom(scoped_css_write_error(error)))
}

fn map_scoped_css_parse_error(error: ParseError<Error>) -> Error {
    match error.kind {
        ParseErrorKind::Custom(error) => error,
        ParseErrorKind::Basic(error) => {
            Error::svg_postprocess("scoped-css", format!("invalid scoped CSS: {error}"))
        }
    }
}

fn scoped_css_write_error(error: fmt::Error) -> Error {
    Error::svg_postprocess("scoped-css", format!("failed to write scoped CSS: {error}"))
}
