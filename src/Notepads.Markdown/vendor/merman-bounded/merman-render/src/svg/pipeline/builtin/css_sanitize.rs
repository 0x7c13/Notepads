use crate::Result;
use cssparser::{
    AtRuleParser, BasicParseErrorKind, CowRcStr, ParseError, Parser, ParserState,
    QualifiedRuleParser, SourcePosition, StyleSheetParser, ToCss, Token,
};
use std::borrow::Cow;
use std::fmt;

use super::attr_sanitize::is_unsafe_render_resource_url_value;
use super::util::{checkpoint_loop, find_tag_end_with_checkpoints, find_with_checkpoints};
use crate::svg::pipeline::{SvgPostprocessContext, SvgPostprocessor};

const CSS_NESTING_HARD_LIMIT: u8 = 64;

#[derive(Debug, Clone, Copy, Default)]
pub struct SanitizeCssPostprocessor;

impl SvgPostprocessor for SanitizeCssPostprocessor {
    fn name(&self) -> &'static str {
        "sanitize-css"
    }

    fn process<'a>(
        &self,
        svg: Cow<'a, str>,
        ctx: &SvgPostprocessContext<'_>,
    ) -> Result<Cow<'a, str>> {
        apply_sanitize_style_elements(svg, || ctx.checkpoint())
    }
}

pub(crate) fn apply_sanitize_style_elements<'a>(
    svg: Cow<'a, str>,
    mut checkpoint: impl FnMut() -> Result<()>,
) -> Result<Cow<'a, str>> {
    checkpoint()?;
    if find_with_checkpoints(&svg, "<style", &mut checkpoint)?.is_none() {
        return Ok(svg);
    }
    sanitize_style_elements_with_checkpoints(&svg, &mut checkpoint).map(Cow::Owned)
}

pub(crate) fn sanitize_style_elements_with_checkpoints<E>(
    svg: &str,
    checkpoint: &mut impl FnMut() -> std::result::Result<(), E>,
) -> std::result::Result<String, E> {
    if find_with_checkpoints(svg, "<style", checkpoint)?.is_none() {
        return Ok(svg.to_string());
    }
    let mut out = String::with_capacity(svg.len());
    let mut cursor = 0;

    while let Some(rel_start) = find_with_checkpoints(&svg[cursor..], "<style", checkpoint)? {
        let start = cursor + rel_start;
        out.push_str(&svg[cursor..start]);

        let Some(open_end) = find_tag_end_with_checkpoints(svg, start, checkpoint)? else {
            out.push_str(&svg[start..]);
            return Ok(out);
        };

        let content_start = open_end + 1;
        let Some(rel_close_start) =
            find_with_checkpoints(&svg[content_start..], "</style", checkpoint)?
        else {
            out.push_str(&svg[start..]);
            return Ok(out);
        };
        let close_start = content_start + rel_close_start;
        let Some(close_end) = find_tag_end_with_checkpoints(svg, close_start, checkpoint)? else {
            out.push_str(&svg[start..]);
            return Ok(out);
        };

        out.push_str(&svg[start..=open_end]);
        out.push_str(&sanitize_css_with_checkpoints(
            &svg[content_start..close_start],
            checkpoint,
        )?);
        out.push_str(&svg[close_start..=close_end]);
        cursor = close_end + 1;
    }

    out.push_str(&svg[cursor..]);
    checkpoint()?;
    Ok(out)
}

#[cfg(test)]
pub(crate) fn sanitize_css(css: &str) -> String {
    process_stylesheet(css, CssProcessingMode::Sanitize).unwrap_or_default()
}

pub(super) fn sanitize_css_with_checkpoints<E>(
    css: &str,
    checkpoint: &mut impl FnMut() -> std::result::Result<(), E>,
) -> std::result::Result<String, E> {
    Ok(run_with_css_control(checkpoint, |control| {
        process_stylesheet_with_control(css, CssProcessingMode::Sanitize, control)
    })?
    .unwrap_or_default())
}

#[cfg(test)]
pub(super) fn sanitize_css_value(value: &str) -> Option<String> {
    let mut parser = Parser::new(value);
    let mut probe = || true;
    let mut control = CssParseControl::new(&mut probe);
    rewrite_component_values(
        &mut parser,
        CssProcessingMode::Sanitize,
        CssNestingDepth::default(),
        &mut control,
    )
    .ok()
}

pub(super) fn sanitize_css_value_with_checkpoints<E>(
    value: &str,
    checkpoint: &mut impl FnMut() -> std::result::Result<(), E>,
) -> std::result::Result<Option<String>, E> {
    Ok(run_with_css_control(checkpoint, |control| {
        let mut parser = Parser::new(value);
        rewrite_component_values(
            &mut parser,
            CssProcessingMode::Sanitize,
            CssNestingDepth::default(),
            control,
        )
    })?
    .ok())
}

#[cfg(test)]
pub(in crate::svg::pipeline) fn validate_resvg_css_stylesheet(
    css: &str,
) -> std::result::Result<(), String> {
    process_stylesheet(css, CssProcessingMode::Validate).map(|_| ())
}

pub(in crate::svg::pipeline) fn validate_resvg_css_stylesheet_with_checkpoints<E>(
    css: &str,
    checkpoint: &mut impl FnMut() -> std::result::Result<(), E>,
) -> std::result::Result<(), CssValidationError<E>> {
    run_with_css_control(checkpoint, |control| {
        process_stylesheet_with_control(css, CssProcessingMode::Validate, control).map(|_| ())
    })
    .map_err(CssValidationError::Checkpoint)?
    .map_err(CssValidationError::Invalid)
}

#[cfg(test)]
pub(in crate::svg::pipeline) fn validate_resvg_css_declaration_list(
    css: &str,
) -> std::result::Result<(), String> {
    let mut parser = Parser::new(css);
    let mut probe = || true;
    let mut control = CssParseControl::new(&mut probe);
    rewrite_declaration_list(
        &mut parser,
        CssProcessingMode::Validate,
        CssNestingDepth::default(),
        &mut control,
    )
    .map(|_| ())
    .map_err(format_parse_error)
}

pub(in crate::svg::pipeline) fn validate_resvg_css_declaration_list_with_checkpoints<E>(
    css: &str,
    checkpoint: &mut impl FnMut() -> std::result::Result<(), E>,
) -> std::result::Result<(), CssValidationError<E>> {
    run_with_css_control(checkpoint, |control| {
        let mut parser = Parser::new(css);
        rewrite_declaration_list(
            &mut parser,
            CssProcessingMode::Validate,
            CssNestingDepth::default(),
            control,
        )
        .map(|_| ())
        .map_err(format_parse_error)
    })
    .map_err(CssValidationError::Checkpoint)?
    .map_err(CssValidationError::Invalid)
}

pub(in crate::svg::pipeline) enum CssValidationError<E> {
    Invalid(String),
    Checkpoint(E),
}

fn run_with_css_control<T, I, E>(
    checkpoint: &mut impl FnMut() -> std::result::Result<(), E>,
    run: impl FnOnce(&mut CssParseControl<'_>) -> std::result::Result<T, I>,
) -> std::result::Result<std::result::Result<T, I>, E> {
    checkpoint()?;
    let mut checkpoint_error = None;
    let result = {
        let mut probe = || match checkpoint() {
            Ok(()) => true,
            Err(error) => {
                checkpoint_error = Some(error);
                false
            }
        };
        let mut control = CssParseControl::new(&mut probe);
        run(&mut control)
    };
    if let Some(error) = checkpoint_error {
        return Err(error);
    }
    checkpoint()?;
    Ok(result)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum CssProcessingMode {
    Sanitize,
    Validate,
}

struct CssParseControl<'a> {
    checkpoint: &'a mut dyn FnMut() -> bool,
    iterations: usize,
    cancelled: bool,
}

impl<'a> CssParseControl<'a> {
    fn new(checkpoint: &'a mut dyn FnMut() -> bool) -> Self {
        Self {
            checkpoint,
            iterations: 0,
            cancelled: false,
        }
    }

    fn step(&mut self) -> std::result::Result<(), ParseError<CssViolation>> {
        self.observe().map_err(ParseError::custom)
    }

    fn observe(&mut self) -> std::result::Result<(), CssViolation> {
        if self.cancelled {
            return Err(CssViolation::Cancelled);
        }

        self.iterations = self.iterations.saturating_add(1);
        let checkpoint_result = checkpoint_loop(self.iterations, &mut || {
            if (self.checkpoint)() { Ok(()) } else { Err(()) }
        });
        if checkpoint_result.is_err() {
            self.cancelled = true;
            return Err(CssViolation::Cancelled);
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
struct CssNestingDepth(u8);

impl CssNestingDepth {
    fn descend(self) -> std::result::Result<Self, ParseError<CssViolation>> {
        if self.0 >= CSS_NESTING_HARD_LIMIT {
            return Err(ParseError::custom(CssViolation::NestingLimit));
        }
        Ok(Self(self.0 + 1))
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum CssViolation {
    Animation,
    BadToken,
    Cancelled,
    Degrees,
    EmptyDeclaration,
    ExternalImageFunction,
    MarkerReference,
    NestingLimit,
    RootSelector,
    UnclosedBlock,
    UnclosedToken,
    UnsafeUrl,
    UnsupportedAtRule(String),
}

impl fmt::Display for CssViolation {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Animation => f.write_str("CSS animation is not part of the resvg-safe contract"),
            Self::BadToken => f.write_str("malformed CSS token"),
            Self::Cancelled => f.write_str("CSS processing was cancelled"),
            Self::Degrees => {
                f.write_str("CSS angle units are not accepted by the resvg-safe contract")
            }
            Self::EmptyDeclaration => f.write_str("empty CSS declaration"),
            Self::ExternalImageFunction => {
                f.write_str("CSS image-set functions can reference external resources")
            }
            Self::MarkerReference => {
                f.write_str("CSS marker references are not part of the resvg-safe contract")
            }
            Self::NestingLimit => write!(
                f,
                "CSS nesting exceeds the resvg-safe hard limit of {CSS_NESTING_HARD_LIMIT}"
            ),
            Self::RootSelector => {
                f.write_str(":root rules are not part of the resvg-safe contract")
            }
            Self::UnclosedBlock => f.write_str("unclosed CSS block or function"),
            Self::UnclosedToken => f.write_str("unclosed CSS string, comment, or URL"),
            Self::UnsafeUrl => f.write_str("unsafe CSS URL"),
            Self::UnsupportedAtRule(name) => {
                write!(f, "@{name} is not part of the resvg-safe contract")
            }
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum AtRuleBody {
    Declarations,
    RuleList,
}

#[derive(Debug)]
enum AtRulePrelude {
    Drop,
    Keep {
        name: String,
        prelude: String,
        body: AtRuleBody,
    },
}

struct ResvgCssRuleParser<'control, 'checkpoint> {
    mode: CssProcessingMode,
    depth: CssNestingDepth,
    control: &'control mut CssParseControl<'checkpoint>,
}

impl<'i> AtRuleParser<'i> for ResvgCssRuleParser<'_, '_> {
    type Prelude = AtRulePrelude;
    type AtRule = String;
    type Error = CssViolation;

    fn parse_prelude(
        &mut self,
        name: CowRcStr<'i>,
        input: &mut Parser<'i>,
    ) -> std::result::Result<Self::Prelude, ParseError<Self::Error>> {
        let prelude = rewrite_component_values(input, self.mode, self.depth, self.control)?;
        let normalized_name = name.to_ascii_lowercase();
        let body = match normalized_name.as_str() {
            "font-face" | "page" => Some(AtRuleBody::Declarations),
            "container" | "document" | "layer" | "media" | "scope" | "supports" => {
                Some(AtRuleBody::RuleList)
            }
            _ => None,
        };

        let explicitly_removed = matches!(
            normalized_name.as_str(),
            "import" | "keyframes" | "-webkit-keyframes"
        );
        if explicitly_removed || body.is_none() {
            if self.mode == CssProcessingMode::Validate {
                return Err(ParseError::custom(CssViolation::UnsupportedAtRule(
                    normalized_name,
                )));
            }
            return Ok(AtRulePrelude::Drop);
        }

        Ok(AtRulePrelude::Keep {
            name: name.to_string(),
            prelude,
            body: body.expect("checked above"),
        })
    }

    fn rule_without_block(
        &mut self,
        prelude: Self::Prelude,
        _start: &ParserState,
    ) -> std::result::Result<Self::AtRule, ()> {
        match prelude {
            AtRulePrelude::Drop => Ok(String::new()),
            AtRulePrelude::Keep { .. } => Err(()),
        }
    }

    fn parse_block(
        &mut self,
        prelude: Self::Prelude,
        _start: &ParserState,
        input: &mut Parser<'i>,
    ) -> std::result::Result<Self::AtRule, ParseError<Self::Error>> {
        let depth = self.depth.descend()?;
        let AtRulePrelude::Keep {
            name,
            prelude,
            body,
        } = prelude
        else {
            consume_component_values(input, depth, self.control)?;
            return Ok(String::new());
        };

        let body = match body {
            AtRuleBody::Declarations => {
                rewrite_declaration_list(input, self.mode, depth, self.control)?
            }
            AtRuleBody::RuleList => rewrite_rule_list(input, self.mode, depth, self.control)?,
        };
        Ok(format!("@{name}{prelude}{{{body}}}"))
    }
}

impl<'i> QualifiedRuleParser<'i> for ResvgCssRuleParser<'_, '_> {
    type Prelude = Option<String>;
    type QualifiedRule = String;
    type Error = CssViolation;

    fn parse_prelude(
        &mut self,
        input: &mut Parser<'i>,
    ) -> std::result::Result<Self::Prelude, ParseError<Self::Error>> {
        let prelude = rewrite_component_values(input, self.mode, self.depth, self.control)?;
        if selector_contains_root(&prelude, self.depth, self.control).map_err(ParseError::custom)? {
            if self.mode == CssProcessingMode::Validate {
                return Err(ParseError::custom(CssViolation::RootSelector));
            }
            return Ok(None);
        }
        Ok(Some(prelude))
    }

    fn parse_block(
        &mut self,
        prelude: Self::Prelude,
        _start: &ParserState,
        input: &mut Parser<'i>,
    ) -> std::result::Result<Self::QualifiedRule, ParseError<Self::Error>> {
        let depth = self.depth.descend()?;
        let Some(prelude) = prelude else {
            consume_component_values(input, depth, self.control)?;
            return Ok(String::new());
        };
        let declarations = rewrite_declaration_list(input, self.mode, depth, self.control)?;
        Ok(format!("{prelude}{{{declarations}}}"))
    }
}

#[cfg(test)]
fn process_stylesheet(css: &str, mode: CssProcessingMode) -> std::result::Result<String, String> {
    let mut probe = || true;
    let mut control = CssParseControl::new(&mut probe);
    process_stylesheet_with_control(css, mode, &mut control)
}

fn process_stylesheet_with_control(
    css: &str,
    mode: CssProcessingMode,
    control: &mut CssParseControl<'_>,
) -> std::result::Result<String, String> {
    let mut input = Parser::new(css);
    rewrite_rule_list(&mut input, mode, CssNestingDepth::default(), control)
        .map_err(format_parse_error)
}

fn rewrite_rule_list(
    input: &mut Parser<'_>,
    mode: CssProcessingMode,
    depth: CssNestingDepth,
    control: &mut CssParseControl<'_>,
) -> std::result::Result<String, ParseError<CssViolation>> {
    let mut parser = ResvgCssRuleParser {
        mode,
        depth,
        control,
    };
    let mut output = String::new();

    for rule in StyleSheetParser::new(input, &mut parser) {
        match rule {
            Ok(rule) => output.push_str(&rule),
            Err((error, _, _)) if is_cancelled_parse_error(&error) => return Err(error),
            Err((error, _, _)) if mode == CssProcessingMode::Validate => return Err(error),
            Err(_) => {}
        }
    }

    Ok(output)
}

fn rewrite_declaration_list(
    input: &mut Parser<'_>,
    mode: CssProcessingMode,
    depth: CssNestingDepth,
    control: &mut CssParseControl<'_>,
) -> std::result::Result<String, ParseError<CssViolation>> {
    let mut output = String::new();

    loop {
        control.step()?;
        let declaration_start = input.position();
        if input.is_exhausted() {
            output.push_str(input.slice_from(declaration_start));
            return Ok(output);
        }

        let declaration = input.parse_until_after(cssparser::Delimiter::Semicolon, |declaration| {
            let prefix_start = declaration.position();
            let property = declaration.expect_ident_cloned()?;
            declaration.expect_colon()?;
            let value_start = declaration.position();
            let prefix = declaration.slice(prefix_start..value_start).to_string();

            if is_animation_property(&property) {
                consume_component_values(declaration, depth, control)?;
                if mode == CssProcessingMode::Validate {
                    return Err(ParseError::custom(CssViolation::Animation));
                }
                return Ok(None);
            }

            if is_marker_reference_property(&property) {
                consume_component_values(declaration, depth, control)?;
                if mode == CssProcessingMode::Validate {
                    return Err(ParseError::custom(CssViolation::MarkerReference));
                }
                return Ok(None);
            }

            let value = rewrite_component_values(declaration, mode, depth, control)?;
            if value.trim().is_empty() {
                return Err(ParseError::custom(CssViolation::EmptyDeclaration));
            }
            Ok(Some((prefix, value)))
        });
        let declaration_end = input.position();
        let had_semicolon = input
            .slice(declaration_start..declaration_end)
            .trim_end()
            .ends_with(';');

        match declaration {
            Ok(Some((prefix, value))) => {
                output.push_str(&prefix);
                output.push_str(&value);
                if had_semicolon {
                    output.push(';');
                }
            }
            Ok(None) => {}
            Err(error) if is_cancelled_parse_error(&error) => return Err(error),
            Err(error) if mode == CssProcessingMode::Validate => return Err(error),
            Err(_) => {}
        }
    }
}

fn rewrite_component_values(
    input: &mut Parser<'_>,
    mode: CssProcessingMode,
    depth: CssNestingDepth,
    control: &mut CssParseControl<'_>,
) -> std::result::Result<String, ParseError<CssViolation>> {
    let mut output = String::new();

    loop {
        control.step()?;
        let token_start = input.position();
        let token = match input.next_including_whitespace_and_comments() {
            Ok(token) => token.clone(),
            Err(error) if matches!(error.kind, BasicParseErrorKind::EndOfInput) => {
                output.push_str(input.slice_from(token_start));
                return Ok(output);
            }
            Err(error) => return Err(error.into()),
        };
        let token_end = input.position();

        match token {
            Token::QuotedString(_) | Token::Comment(_) => {
                let raw = input.slice(token_start..token_end);
                ensure_source_closed_token(raw, control)?;
                output.push_str(raw);
            }
            Token::Dimension {
                value,
                int_value,
                has_sign,
                unit,
            } if unit.eq_ignore_ascii_case("deg") => {
                if mode == CssProcessingMode::Validate {
                    return Err(ParseError::custom(CssViolation::Degrees));
                }
                output.push_str(
                    &Token::Number {
                        value,
                        int_value,
                        has_sign,
                    }
                    .to_css_string(),
                );
            }
            Token::UnquotedUrl(url) => {
                ensure_source_closed_token(input.slice(token_start..token_end), control)?;
                if is_unsafe_render_resource_url_value(&url) {
                    return Err(ParseError::custom(CssViolation::UnsafeUrl));
                }
                output.push_str(input.slice(token_start..token_end));
            }
            Token::Function(name) => {
                if matches_external_image_function(&name) {
                    return Err(ParseError::custom(CssViolation::ExternalImageFunction));
                }
                let nested_depth = depth.descend()?;
                output.push_str(input.slice(token_start..token_end));
                let nested = input.parse_nested_block(|nested| {
                    if name.eq_ignore_ascii_case("url") {
                        rewrite_quoted_url(nested, mode, nested_depth, control)
                    } else {
                        rewrite_component_values(nested, mode, nested_depth, control)
                    }
                })?;
                ensure_source_closed_block(input, token_start, ')', control)?;
                output.push_str(&nested);
                output.push(')');
            }
            Token::ParenthesisBlock | Token::SquareBracketBlock | Token::CurlyBracketBlock => {
                let nested_depth = depth.descend()?;
                output.push_str(input.slice(token_start..token_end));
                let nested = input.parse_nested_block(|nested| {
                    rewrite_component_values(nested, mode, nested_depth, control)
                })?;
                let close = match token {
                    Token::ParenthesisBlock => ')',
                    Token::SquareBracketBlock => ']',
                    Token::CurlyBracketBlock => '}',
                    _ => unreachable!(),
                };
                ensure_source_closed_block(input, token_start, close, control)?;
                output.push_str(&nested);
                output.push(close);
            }
            Token::BadUrl(_) | Token::BadString(_) => {
                return Err(ParseError::custom(CssViolation::BadToken));
            }
            _ => output.push_str(input.slice(token_start..token_end)),
        }
    }
}

pub(in crate::svg::pipeline) fn matches_external_image_function(name: &str) -> bool {
    name.eq_ignore_ascii_case("image-set") || name.eq_ignore_ascii_case("-webkit-image-set")
}

fn ensure_source_closed_token(
    raw_token: &str,
    control: &mut CssParseControl<'_>,
) -> std::result::Result<(), ParseError<CssViolation>> {
    control.step()?;
    // cssparser accepts EOF-terminated strings, comments, and URLs. Only an explicitly
    // closed token ends at the original boundary when another space follows it.
    let mut probe = String::with_capacity(raw_token.len() + 1);
    probe.push_str(raw_token);
    probe.push(' ');

    let mut parser = Parser::new(&probe);
    parser.next_including_whitespace_and_comments()?;
    if parser.position().byte_index() == raw_token.len() {
        Ok(())
    } else {
        Err(ParseError::custom(CssViolation::UnclosedToken))
    }
}

fn ensure_source_closed_block(
    input: &Parser<'_>,
    token_start: SourcePosition,
    close: char,
    control: &mut CssParseControl<'_>,
) -> std::result::Result<(), ParseError<CssViolation>> {
    let raw_block = input.slice(token_start..input.position());
    if source_ends_with_close(raw_block, close, control).map_err(ParseError::custom)?
        && source_closes_initial_block(raw_block, close, control).map_err(ParseError::custom)?
    {
        return Ok(());
    }

    Err(ParseError::custom(CssViolation::UnclosedBlock))
}

fn source_ends_with_close(
    raw_block: &str,
    close: char,
    control: &mut CssParseControl<'_>,
) -> std::result::Result<bool, CssViolation> {
    for character in raw_block.chars().rev() {
        control.observe()?;
        if character.is_whitespace() {
            continue;
        }
        return Ok(character == close);
    }
    Ok(false)
}

fn source_closes_initial_block(
    raw_block: &str,
    close: char,
    control: &mut CssParseControl<'_>,
) -> std::result::Result<bool, CssViolation> {
    const SENTINEL: &str = "__merman_css_closed_block_sentinel__";

    control.observe()?;
    let mut probe = String::with_capacity(raw_block.len() + SENTINEL.len() + 1);
    probe.push_str(raw_block);
    probe.push(' ');
    probe.push_str(SENTINEL);

    let mut parser = Parser::new(&probe);
    let Ok(token) = parser.next_including_whitespace().cloned() else {
        return Ok(false);
    };
    if !opening_token_matches_close(&token, close) {
        return Ok(false);
    }

    let nested_result = parser.parse_nested_block(|nested| {
        loop {
            control.step()?;
            if nested.next_including_whitespace().is_err() {
                break;
            }
        }
        Ok::<_, ParseError<CssViolation>>(())
    });
    if let Err(error) = nested_result {
        return if is_cancelled_parse_error(&error) {
            Err(CssViolation::Cancelled)
        } else {
            Ok(false)
        };
    }

    control.observe()?;
    Ok(matches!(
        parser.next(),
        Ok(Token::Ident(name)) if name.as_ref() == SENTINEL
    ))
}

fn opening_token_matches_close(token: &Token<'_>, close: char) -> bool {
    match token {
        Token::Function(_) | Token::ParenthesisBlock => close == ')',
        Token::SquareBracketBlock => close == ']',
        Token::CurlyBracketBlock => close == '}',
        _ => false,
    }
}

fn rewrite_quoted_url(
    input: &mut Parser<'_>,
    mode: CssProcessingMode,
    depth: CssNestingDepth,
    control: &mut CssParseControl<'_>,
) -> std::result::Result<String, ParseError<CssViolation>> {
    control.step()?;
    let url_start = input.position();
    let url = input.expect_string_cloned()?;
    input.expect_exhausted()?;
    if is_unsafe_render_resource_url_value(&url) {
        return Err(ParseError::custom(CssViolation::UnsafeUrl));
    }

    let raw = input.slice_from(url_start);
    let mut raw_parser = Parser::new(raw);
    rewrite_component_values(&mut raw_parser, mode, depth, control)
}

fn consume_component_values(
    input: &mut Parser<'_>,
    depth: CssNestingDepth,
    control: &mut CssParseControl<'_>,
) -> std::result::Result<(), ParseError<CssViolation>> {
    loop {
        control.step()?;
        let token = match input.next_including_whitespace() {
            Ok(token) => token.clone(),
            Err(error) if matches!(error.kind, BasicParseErrorKind::EndOfInput) => return Ok(()),
            Err(error) => return Err(error.into()),
        };
        if matches!(
            token,
            Token::Function(_)
                | Token::ParenthesisBlock
                | Token::SquareBracketBlock
                | Token::CurlyBracketBlock
        ) {
            let nested_depth = depth.descend()?;
            input.parse_nested_block(|nested| {
                consume_component_values(nested, nested_depth, control)
            })?;
        }
    }
}

fn is_animation_property(property: &str) -> bool {
    let property = property.to_ascii_lowercase();
    property == "animation" || property.starts_with("animation-")
}

fn is_marker_reference_property(property: &str) -> bool {
    matches!(
        property.to_ascii_lowercase().as_str(),
        "marker" | "marker-start" | "marker-mid" | "marker-end"
    )
}

fn selector_contains_root(
    selector: &str,
    depth: CssNestingDepth,
    control: &mut CssParseControl<'_>,
) -> std::result::Result<bool, CssViolation> {
    let mut parser = Parser::new(selector);
    parser_contains_root_selector(&mut parser, depth, control).map_err(|error| match error.kind {
        cssparser::ParseErrorKind::Custom(violation) => violation,
        cssparser::ParseErrorKind::Basic(_) => CssViolation::BadToken,
    })
}

fn parser_contains_root_selector(
    input: &mut Parser<'_>,
    depth: CssNestingDepth,
    control: &mut CssParseControl<'_>,
) -> std::result::Result<bool, ParseError<CssViolation>> {
    let mut after_colon = false;
    loop {
        control.step()?;
        let token = match input.next_including_whitespace() {
            Ok(token) => token.clone(),
            Err(error) if matches!(error.kind, BasicParseErrorKind::EndOfInput) => {
                return Ok(false);
            }
            Err(error) => return Err(error.into()),
        };
        match token {
            Token::Colon => after_colon = true,
            Token::Ident(name) if after_colon && name.eq_ignore_ascii_case("root") => {
                return Ok(true);
            }
            Token::Function(name) => {
                if after_colon && name.eq_ignore_ascii_case("root") {
                    return Ok(true);
                }
                let nested_depth = depth.descend()?;
                if input.parse_nested_block(|nested| {
                    parser_contains_root_selector(nested, nested_depth, control)
                })? {
                    return Ok(true);
                }
                after_colon = false;
            }
            Token::ParenthesisBlock | Token::SquareBracketBlock | Token::CurlyBracketBlock => {
                let nested_depth = depth.descend()?;
                if input.parse_nested_block(|nested| {
                    parser_contains_root_selector(nested, nested_depth, control)
                })? {
                    return Ok(true);
                }
                after_colon = false;
            }
            Token::WhiteSpace(_) => {}
            _ => after_colon = false,
        }
    }
}

fn is_cancelled_parse_error(error: &ParseError<CssViolation>) -> bool {
    matches!(
        &error.kind,
        cssparser::ParseErrorKind::Custom(CssViolation::Cancelled)
    )
}

fn format_parse_error(error: ParseError<CssViolation>) -> String {
    error.to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn nested_function(depth: usize, leaf: &str) -> String {
        format!("{}{}{}", "f(".repeat(depth), leaf, ")".repeat(depth))
    }

    fn nested_media(depth: usize, rule: &str) -> String {
        format!(
            "{}{}{}",
            "@media all{".repeat(depth),
            rule,
            "}".repeat(depth)
        )
    }

    #[test]
    fn css_sanitize_removes_active_rules_and_declarations_after_tokenization() {
        let css = r#"@\69mport url(\"https://example.com/a.css\");@k\65yframes spin{to{opacity:1}}:r\6fot{--x:red}.a{a\6eimation:spin 1s;fill:red}"#;
        let out = sanitize_css(css);

        assert_eq!(out, ".a{fill:red}");
    }

    #[test]
    fn css_sanitize_rewrites_only_dimension_tokens() {
        let css = r##".a{transform:rotate(45deg) rotate(-10.5DEG);content:\"45deg\";background:url(#a45deg);--name:foo45deg;--near:90deg-foo}"##;
        let out = sanitize_css(css);

        assert!(out.contains("rotate(45) rotate(-10.5)"), "{out}");
        assert!(out.contains(r#"content:\"45deg\""#), "{out}");
        assert!(out.contains("url(#a45deg)"), "{out}");
        assert!(out.contains("--name:foo45deg"), "{out}");
        assert!(out.contains("--near:90deg-foo"), "{out}");
    }

    #[test]
    fn css_sanitize_drops_unsafe_urls_and_preserves_safe_image_sources() {
        let css = r##".bad{fill:u\72l(\"j\61vascript:alert(1)\");stroke:red}.fragment{fill:url(#paint)}.image{background:url(data:image/png;base64,AAAA)}"##;
        let out = sanitize_css(css);

        assert_eq!(
            out,
            r##".bad{stroke:red}.fragment{fill:url(#paint)}.image{background:url(data:image/png;base64,AAAA)}"##
        );
    }

    #[test]
    fn css_sanitize_drops_escaped_external_image_functions() {
        let css = r#".bad{background-image:im\61ge-set(\"https://example.test/x.png\" 1x);stroke:red}.also-bad{content:-webkit-image-set(\"https://example.test/y.png\" 1x);fill:blue}"#;
        let out = sanitize_css(css);

        assert_eq!(out, ".bad{stroke:red}.also-bad{fill:blue}");
        assert!(validate_resvg_css_stylesheet(css).is_err());
    }

    #[test]
    fn css_sanitize_removes_marker_references_that_cannot_be_preflighted() {
        let css = r##".edge{marker:url(#all);marker-start:url(#start);marker-mid:url(#mid);marker-end:url(#end);stroke:red}"##;

        assert_eq!(sanitize_css(css), ".edge{stroke:red}");
        assert!(validate_resvg_css_stylesheet(css).is_err());
        assert!(validate_resvg_css_declaration_list("marker-end:url(#end);stroke:red").is_err());
    }

    #[test]
    fn css_sanitize_drops_external_render_resources() {
        let css = r##".local{background:url(../image.png);stroke:red}.root{cursor:url("/tmp/cursor.svg"),auto;fill:blue}.remote{filter:url(https://example.com/filter.svg#blur);color:black}.fragment{fill:url(#paint)}.embedded{background:url(data:image/png;base64,AAAA)}.missing-comma{background:url(data:image/png);opacity:.5}.spaced{background:url("d a t a:image/png;base64,BBBB");opacity:.75}"##;

        let out = sanitize_css(css);

        assert!(!out.contains("../image.png"), "{out}");
        assert!(!out.contains("/tmp/cursor.svg"), "{out}");
        assert!(!out.contains("https://example.com"), "{out}");
        assert!(out.contains(".local{stroke:red}"), "{out}");
        assert!(out.contains(".root{fill:blue}"), "{out}");
        assert!(out.contains(".remote{color:black}"), "{out}");
        assert!(out.contains(".fragment{fill:url(#paint)}"), "{out}");
        assert!(
            out.contains(".embedded{background:url(data:image/png;base64,AAAA)}"),
            "{out}"
        );
        assert!(out.contains(".missing-comma{opacity:.5}"), "{out}");
        assert!(out.contains(".spaced{opacity:.75}"), "{out}");
        assert!(!out.contains("url(data:image/png)"), "{out}");
        assert!(!out.contains("d a t a:"), "{out}");
    }

    #[test]
    fn css_validation_rejects_non_terminal_constructs() {
        assert!(validate_resvg_css_stylesheet("@import 'a.css';").is_err());
        assert!(validate_resvg_css_stylesheet(".a{animation:spin 1s}").is_err());
        assert!(validate_resvg_css_stylesheet(".a{transform:rotate(45deg)}").is_err());
        assert!(validate_resvg_css_stylesheet(".a{fill:url(javascript:x)}").is_err());
        assert!(validate_resvg_css_stylesheet(".a{content:\"45deg\"}").is_ok());
    }

    #[test]
    fn deg_component_rewrite_does_not_touch_strings_or_urls() {
        assert_eq!(
            sanitize_css_value(r##"rotate(.5deg) \"45deg\" url(#a45deg) foo45deg 90deg-foo"##)
                .as_deref(),
            Some(r##"rotate(0.5) \"45deg\" url(#a45deg) foo45deg 90deg-foo"##)
        );
    }

    #[test]
    fn css_sanitize_rejects_unclosed_component_value_blocks() {
        assert_eq!(sanitize_css_value("5rl('file:///{animatiEtroke:#333"), None);
        assert_eq!(sanitize_css_value("rotate(45deg"), None);
        assert_eq!(sanitize_css_value("outer((red)"), None);
    }

    #[test]
    fn css_sanitize_rejects_unclosed_component_value_tokens() {
        for value in [
            r#"""#,
            "'",
            r#""text}"#,
            "'text}",
            r#""text\""#,
            r#"'text\'"#,
            r#""text\"#,
            r#"'text\"#,
            r#""text\\"#,
            r#"'text\\"#,
            r#""text\22"#,
            r#"'text\27"#,
            "\"text\\\n",
            "red/*",
            "red/*/",
            "red/*comment}",
            "url(#paint",
            "url(#paint ",
            r#"url(#paint\)"#,
            r#"u\72l(#paint"#,
        ] {
            assert_eq!(sanitize_css_value(value), None, "{value:?}");
            let declaration = format!("content:{value}");
            let error = validate_resvg_css_declaration_list(&declaration)
                .expect_err("EOF-terminated tokens must fail validation");
            assert!(error.contains("unclosed CSS"), "{value:?}: {error}");

            let css = format!(".safe{{fill:red}}.a{{stroke:blue;{declaration}");
            let sanitized = sanitize_css(&css);
            assert_eq!(sanitized, ".safe{fill:red}.a{stroke:blue;}", "{value:?}");
            assert!(validate_resvg_css_stylesheet(&css).is_err(), "{value:?}");
            assert!(
                validate_resvg_css_stylesheet(&sanitized).is_ok(),
                "{value:?}"
            );
            assert_eq!(sanitize_css(&sanitized), sanitized, "{value:?}");
        }
        assert_eq!(sanitize_css(r#".a{b:"}"#), ".a{}");
    }

    #[test]
    fn css_sanitize_preserves_explicitly_closed_component_value_tokens() {
        for value in [
            r#""""#,
            "''",
            r#""A\"B""#,
            r#"'A\'B'"#,
            r#""A\\""#,
            r#"'A\\'"#,
            r#""A\"""#,
            r#"'A\''"#,
            r#""A\22 B""#,
            r#"'A\27 B'"#,
            r#""Unicode \1F642 ""#,
            "\"A\\\r\nB\"",
            r#"/*before*/"A"/*after*/"#,
            "red/**/",
            "red/*comment*/",
            "url(#paint)",
            r#"url(#paint\))"#,
            r#"u\72l(#paint)"#,
            r##"url("#paint")"##,
            "url('#paint')",
        ] {
            assert_eq!(
                sanitize_css_value(value).as_deref(),
                Some(value),
                "{value:?}"
            );
            let css = format!(".a{{content:{value}}}");
            assert_eq!(sanitize_css(&css), css, "{value:?}");
            assert!(validate_resvg_css_stylesheet(&css).is_ok(), "{value:?}");
            assert!(
                validate_resvg_css_declaration_list(&format!("content:{value}")).is_ok(),
                "{value:?}"
            );
        }
    }

    #[test]
    fn css_sanitize_drops_rules_with_unclosed_string_preludes() {
        for rule in [
            r#".a[title="}]{}"#,
            ".a[title='}]{}",
            r#"@media "all{}"#,
            "@supports (font-family:'a){}",
        ] {
            let css = format!(".safe{{fill:red}}{rule}");
            assert_eq!(sanitize_css(&css), ".safe{fill:red}", "{rule}");
            assert!(validate_resvg_css_stylesheet(&css).is_err(), "{rule}");
        }
    }

    #[test]
    fn controlled_css_sanitize_stops_during_declaration_traversal() {
        let css = format!(".a{{{}}}", "fill:red;".repeat(512));
        let mut checkpoints = 0usize;
        let result = sanitize_css_with_checkpoints(&css, &mut || {
            checkpoints += 1;
            if checkpoints == 3 {
                Err("cancelled")
            } else {
                Ok(())
            }
        });

        assert_eq!(result, Err("cancelled"));
        assert_eq!(checkpoints, 3);
    }

    #[test]
    fn css_nesting_limit_is_inclusive_and_validation_reports_the_limit() {
        let exact = nested_function(CSS_NESTING_HARD_LIMIT.into(), "red");
        let over = nested_function(usize::from(CSS_NESTING_HARD_LIMIT) + 1, "red");

        assert_eq!(sanitize_css_value(&exact).as_deref(), Some(exact.as_str()));
        assert_eq!(sanitize_css_value(&over), None);
        assert!(validate_resvg_css_declaration_list(&format!("fill:{exact}")).is_ok());

        let error = validate_resvg_css_declaration_list(&format!("fill:{over}"))
            .expect_err("one nesting level past the hard limit must be rejected");
        assert!(error.contains("CSS nesting exceeds"), "{error}");
        assert!(
            error.contains(&CSS_NESTING_HARD_LIMIT.to_string()),
            "{error}"
        );
    }

    #[test]
    fn css_nesting_limit_bounds_rules_selectors_and_drop_traversal() {
        let exact_rule_value = nested_function(usize::from(CSS_NESTING_HARD_LIMIT) - 1, "red");
        let over_rule_value = nested_function(CSS_NESTING_HARD_LIMIT.into(), "red");
        let declarations =
            format!(".exact{{fill:{exact_rule_value}}}.over{{fill:{over_rule_value};stroke:blue}}");
        assert_eq!(
            sanitize_css(&declarations),
            format!(".exact{{fill:{exact_rule_value}}}.over{{stroke:blue}}")
        );

        let selector_exact = format!(
            "{}.exact-selector{}{{fill:red}}",
            ":is(".repeat(CSS_NESTING_HARD_LIMIT.into()),
            ")".repeat(CSS_NESTING_HARD_LIMIT.into())
        );
        let selector_over = format!(
            "{}.over-selector{}{{fill:red}}",
            ":is(".repeat(usize::from(CSS_NESTING_HARD_LIMIT) + 1),
            ")".repeat(usize::from(CSS_NESTING_HARD_LIMIT) + 1)
        );
        assert!(validate_resvg_css_stylesheet(&selector_exact).is_ok());
        let selector_error = validate_resvg_css_stylesheet(&selector_over)
            .expect_err("an over-limit selector function must be rejected");
        assert!(selector_error.contains("CSS nesting exceeds"));
        assert!(sanitize_css(&selector_over).is_empty());

        let media_exact = nested_media(CSS_NESTING_HARD_LIMIT.into(), "");
        let media_over = nested_media(usize::from(CSS_NESTING_HARD_LIMIT) + 1, "");
        assert!(validate_resvg_css_stylesheet(&media_exact).is_ok());
        let media_error = validate_resvg_css_stylesheet(&media_over)
            .expect_err("an over-limit grouping rule must be rejected");
        assert!(media_error.contains("CSS nesting exceeds"));
        assert_eq!(
            sanitize_css(&media_over).matches("@media").count(),
            usize::from(CSS_NESTING_HARD_LIMIT)
        );

        let deeply_nested = nested_function(4_096, "spin");
        let dropped = format!(
            ".animation{{animation:{deeply_nested};stroke:blue}}:root{{--x:{deeply_nested}}}@unknown x{{value:{deeply_nested}}}.safe{{fill:red}}"
        );
        assert_eq!(
            sanitize_css(&dropped),
            ".animation{stroke:blue}.safe{fill:red}"
        );
    }
}
