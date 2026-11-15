use super::ast::*;
use super::lexer::{self, Keyword, Token, TokenChannel, TokenKind};
use crate::{MAX_DIAGRAM_NESTING_DEPTH, OperationControl, OperationControlResult, SourceSpan};
use std::cell::Cell;
use std::collections::{HashSet, VecDeque};

const MISSING_PARTICIPANT: &str = "Missing `Participant`";
const MISSING_CONSTRUCTOR: &str = "Missing Constructor";
const MAX_HEAD_PARTICIPANT_PREDICTION_STATES: usize =
    MAX_DIAGRAM_NESTING_DEPTH * MAX_DIAGRAM_NESTING_DEPTH;

#[cfg(test)]
pub(super) fn parse(source: &str, raw_tokens: &[Token]) -> ParsedSyntax {
    parse_controlled(source, raw_tokens, &OperationControl::new())
        .expect("a private parse control cannot be cancelled")
}

pub(super) fn parse_controlled(
    source: &str,
    raw_tokens: &[Token],
    control: &OperationControl,
) -> OperationControlResult<ParsedSyntax> {
    control.checkpoint()?;
    let tokens = lexer::parser_tokens_controlled(raw_tokens, control)?;
    let comments = comments_before_default_tokens(raw_tokens, control)?;
    Parser::new(source, tokens, comments, control).parse()
}

struct Parser<'a> {
    source: &'a str,
    tokens: Vec<Token>,
    comments: Vec<Option<SpannedText>>,
    cursor: usize,
    diagnostics: Vec<SyntaxDiagnostic>,
    control: OperationControl,
}

impl<'a> Parser<'a> {
    fn new(
        source: &'a str,
        tokens: Vec<Token>,
        comments: Vec<Option<SpannedText>>,
        control: &OperationControl,
    ) -> Self {
        Self {
            source,
            tokens,
            comments,
            cursor: 0,
            diagnostics: Vec::new(),
            control: control.clone(),
        }
    }

    fn parse(mut self) -> OperationControlResult<ParsedSyntax> {
        self.control.checkpoint()?;
        match self.take_name() {
            Some(header)
                if header
                    .value
                    .get(.."zenuml".len())
                    .is_some_and(|prefix| prefix.eq_ignore_ascii_case("zenuml")) =>
            {
                self.restore_header_suffix(header)?;
            }
            Some(header) => self.error("expected `zenuml` header", header.span),
            None => self.error("expected `zenuml` header", self.insertion_span()),
        }

        let title = if self.at_keyword(Keyword::Title) {
            self.discard_current_comment();
            Some(self.parse_title())
        } else {
            None
        };

        let mut head = Vec::new();
        let mut starter = None;
        let mut pending_comment = None;
        let mut head_prediction = HeadParticipantPrediction::default();

        loop {
            if !self.parse_active() {
                break;
            }
            let comment = self.take_current_comment();
            if self.at_keyword(Keyword::Group) {
                head.push(HeadItemSyntax::Group(self.parse_group(comment)));
                continue;
            }
            if matches!(self.peek_kind(), TokenKind::StarterAnnotation) {
                starter = Some(self.parse_starter());
                break;
            }
            let participant = {
                let grammar = Grammar::with_control(self.source, &self.tokens, &self.control);
                head_prediction.select(&grammar, self.cursor)
            };
            if let Some(participant) = participant {
                head.push(HeadItemSyntax::Participant(Box::new(
                    self.consume_participant(participant, comment),
                )));
                continue;
            }
            pending_comment = comment;
            break;
        }

        let statements = self.parse_block(0, false, pending_comment, None).statements;
        let document = SyntaxDocument {
            title,
            head,
            starter,
            statements,
        };
        self.control.checkpoint()?;
        Ok(ParsedSyntax {
            document,
            diagnostics: self.diagnostics,
        })
    }

    fn restore_header_suffix(&mut self, header: SpannedText) -> OperationControlResult<()> {
        let Some(suffix) = header.value.get("zenuml".len()..) else {
            return Ok(());
        };
        if suffix.is_empty() {
            return Ok(());
        }

        let suffix_start = header.span.start + "zenuml".len();
        let raw_tokens = lexer::lex_controlled(suffix, &self.control)?;
        let suffix_tokens = lexer::parser_tokens_controlled(&raw_tokens, &self.control)?
            .into_iter()
            .filter(|token| !matches!(token.kind, TokenKind::Eof))
            .map(|mut token| {
                token.span = SourceSpan::new(
                    suffix_start + token.span.start,
                    suffix_start + token.span.end,
                );
                token
            })
            .collect::<Vec<_>>();
        self.comments.splice(
            self.cursor..self.cursor,
            std::iter::repeat_n(None, suffix_tokens.len()),
        );
        self.tokens.splice(self.cursor..self.cursor, suffix_tokens);
        self.control.checkpoint()
    }

    fn parse_title(&mut self) -> SpannedText {
        self.bump();
        let value = match self.peek().clone() {
            Token {
                kind: TokenKind::TitleContent(_),
                span,
                ..
            } => {
                self.bump();
                trimmed_text(self.source, span.start, span.end)
                    .unwrap_or_else(|| SpannedText::new(String::new(), span))
            }
            _ => SpannedText::new(String::new(), self.insertion_span()),
        };
        if matches!(self.peek_kind(), TokenKind::TitleEnd) {
            self.bump();
        }
        value
    }

    fn parse_group(&mut self, _comment: Option<SpannedText>) -> GroupSyntax {
        let start = self.bump().span.start;
        let name = self.take_name();
        let mut participants = Vec::new();
        if matches!(self.peek_kind(), TokenKind::OpenBrace) {
            self.bump();
            while self.parse_active()
                && !self.at_eof()
                && !matches!(self.peek_kind(), TokenKind::CloseBrace)
            {
                let comment = self.take_current_comment();
                let candidate = {
                    let grammar = Grammar::with_control(self.source, &self.tokens, &self.control);
                    grammar
                        .participant_candidates(self.cursor)
                        .into_iter()
                        .next()
                };
                if let Some(candidate) = candidate {
                    participants.push(self.consume_participant(candidate, comment));
                } else {
                    let span = self.bump().span;
                    self.error("expected participant declaration in ZenUML group", span);
                }
            }
            if matches!(self.peek_kind(), TokenKind::CloseBrace) {
                self.bump();
            }
        }
        GroupSyntax {
            name,
            participants,
            span: SourceSpan::new(start, self.previous_end().max(start)),
        }
    }

    fn parse_starter(&mut self) -> StarterSyntax {
        let annotation = self.bump().clone();
        if !matches!(self.peek_kind(), TokenKind::OpenParen) {
            return StarterSyntax {
                name: None,
                span: annotation.span,
            };
        }
        self.bump();
        let name = self.take_name();
        if matches!(self.peek_kind(), TokenKind::CloseParen) {
            self.bump();
        } else {
            self.error("expected `)` after ZenUML starter", self.insertion_span());
        }
        StarterSyntax {
            name,
            span: SourceSpan::new(annotation.span.start, self.previous_end()),
        }
    }

    fn consume_participant(
        &mut self,
        participant: ParticipantMatch,
        comment: Option<SpannedText>,
    ) -> ParticipantSyntax {
        self.cursor = participant.end;
        let name = participant
            .name
            .unwrap_or_else(|| SpannedText::new(MISSING_PARTICIPANT, participant.span));
        ParticipantSyntax {
            name,
            label: participant.label,
            participant_type: participant.participant_type,
            stereotype: participant.stereotype,
            emoji: participant.emoji,
            width: participant.width,
            color: participant.color,
            comment,
            span: participant.span,
        }
    }

    fn parse_block(
        &mut self,
        depth: usize,
        stop_at_close: bool,
        mut pending_comment: Option<SpannedText>,
        statement_limit: Option<usize>,
    ) -> ParsedBlock {
        if depth > MAX_DIAGRAM_NESTING_DEPTH {
            self.error(
                format!("ZenUML nesting depth exceeds {MAX_DIAGRAM_NESTING_DEPTH}"),
                self.peek().span,
            );
            self.skip_balanced_block();
            return ParsedBlock::default();
        }

        let mut statements = Vec::new();
        while self.parse_active() && !self.at_eof() {
            if statement_limit.is_some_and(|limit| statements.len() >= limit) {
                break;
            }
            if stop_at_close && matches!(self.peek_kind(), TokenKind::CloseBrace) {
                let closing_comment = self.take_current_comment();
                self.bump();
                return ParsedBlock {
                    statements,
                    closing_comment,
                };
            }
            if matches!(self.peek_kind(), TokenKind::CloseBrace) {
                let span = self.bump().span;
                self.error("unexpected `}` in ZenUML document", span);
                continue;
            }
            if pending_comment.is_none() {
                pending_comment = self.take_current_comment();
            }
            let start = self.cursor;
            if let Some(statement) = self.parse_statement(depth, pending_comment.take()) {
                statements.push(statement);
                continue;
            }
            if self.cursor == start {
                let span = self.bump().span;
                self.error("unsupported ZenUML statement", span);
            }
        }
        if stop_at_close {
            self.error(
                "unterminated ZenUML block; expected `}`",
                self.insertion_span(),
            );
        }
        ParsedBlock {
            statements,
            closing_comment: None,
        }
    }

    fn parse_statement(
        &mut self,
        depth: usize,
        comment: Option<SpannedText>,
    ) -> Option<StatementSyntax> {
        match self.peek_kind() {
            TokenKind::Keyword(Keyword::If) => self.parse_alternative(depth, comment),
            TokenKind::Keyword(Keyword::Try) => self.parse_try(depth, comment),
            TokenKind::Keyword(Keyword::Par) => self.parse_single_fragment(
                depth,
                comment,
                FragmentKindSyntax::Parallel,
                FragmentForm::RecoverRequiredBlock,
            ),
            TokenKind::Keyword(Keyword::Opt) => self.parse_single_fragment(
                depth,
                comment,
                FragmentKindSyntax::Optional,
                FragmentForm::RecoverRequiredBlock,
            ),
            TokenKind::Keyword(Keyword::Critical) => self.parse_single_fragment(
                depth,
                comment,
                FragmentKindSyntax::Critical,
                FragmentForm::RecoverRequiredBlock,
            ),
            TokenKind::Keyword(Keyword::Section) | TokenKind::OpenBrace => self
                .parse_single_fragment(
                    depth,
                    comment,
                    FragmentKindSyntax::Section,
                    FragmentForm::Section,
                ),
            TokenKind::Keyword(Keyword::While) => self.parse_single_fragment(
                depth,
                comment,
                FragmentKindSyntax::Loop,
                FragmentForm::OptionalBlock,
            ),
            TokenKind::Keyword(Keyword::Ref) => self.parse_reference(comment),
            TokenKind::Keyword(Keyword::Return) | TokenKind::ReturnAnnotation => {
                self.parse_return_statement(comment)
            }
            TokenKind::Divider(_) => self.parse_divider(comment),
            _ => self.parse_creation_message_async_or_return(depth, comment),
        }
    }

    fn parse_single_fragment(
        &mut self,
        depth: usize,
        comment: Option<SpannedText>,
        kind: FragmentKindSyntax,
        form: FragmentForm,
    ) -> Option<StatementSyntax> {
        let start = self.peek().span.start;
        let anonymous = matches!(self.peek_kind(), TokenKind::OpenBrace);
        if !anonymous {
            self.bump();
        }
        let had_par_expr = !anonymous && matches!(self.peek_kind(), TokenKind::OpenParen);
        let label = if had_par_expr {
            let (par, depth_error) = self.grammar_match(|grammar, start| grammar.par_expr(start));
            if let Some(span) = depth_error {
                self.report_expression_depth_error(span);
            }
            par.and_then(|par| {
                self.cursor = par.end;
                par.label
            })
        } else {
            None
        };

        let block = if matches!(self.peek_kind(), TokenKind::OpenBrace) {
            self.bump();
            self.parse_block(depth + 1, true, None, None)
        } else {
            let recover_missing_block = had_par_expr
                && matches!(
                    form,
                    FragmentForm::RecoverRequiredBlock | FragmentForm::Section
                );
            if anonymous || recover_missing_block {
                self.error("expected `{` after ZenUML fragment", self.insertion_span());
            }
            if recover_missing_block && {
                let grammar = Grammar::with_control(self.source, &self.tokens, &self.control);
                grammar.can_start_statement(self.cursor)
            } {
                self.parse_block(depth + 1, false, None, Some(1))
            } else {
                ParsedBlock::default()
            }
        };
        let end = self.previous_end().max(start);
        let section = FragmentSectionSyntax {
            label: label.clone(),
            statements: block.statements,
            body_comment: block.closing_comment,
            span: SourceSpan::new(start, end),
        };
        Some(StatementSyntax {
            kind: StatementKindSyntax::Fragment(FragmentSyntax {
                kind,
                label,
                sections: vec![section],
            }),
            comment,
            span: SourceSpan::new(start, end),
        })
    }

    fn parse_alternative(
        &mut self,
        depth: usize,
        comment: Option<SpannedText>,
    ) -> Option<StatementSyntax> {
        let start = self.bump().span.start;
        let label = self.parse_required_par_expr("expected `(condition)` after ZenUML `if`");
        let first = self.parse_optional_alt_section(depth, label.clone(), start);
        let mut sections = vec![first];

        while self.at_keyword(Keyword::Else) {
            let branch_start = self.bump().span.start;
            let branch_label = if self.at_keyword(Keyword::If) {
                self.bump();
                self.parse_required_par_expr("expected `(condition)` after ZenUML `else if`")
            } else {
                Some(SpannedText::new(
                    "else",
                    SourceSpan::new(branch_start, self.previous_end()),
                ))
            };
            sections.push(self.parse_optional_alt_section(depth, branch_label, branch_start));
        }

        let end = self.previous_end().max(start);
        Some(StatementSyntax {
            kind: StatementKindSyntax::Fragment(FragmentSyntax {
                kind: FragmentKindSyntax::Alternative,
                label,
                sections,
            }),
            comment,
            span: SourceSpan::new(start, end),
        })
    }

    fn parse_required_par_expr(&mut self, diagnostic: &str) -> Option<SpannedText> {
        let (par, depth_error) = self.grammar_match(|grammar, start| grammar.par_expr(start));
        if let Some(span) = depth_error {
            self.report_expression_depth_error(span);
            return None;
        }
        if let Some(par) = par {
            self.cursor = par.end;
            par.label
        } else {
            self.error(diagnostic, self.insertion_span());
            None
        }
    }

    fn parse_optional_alt_section(
        &mut self,
        depth: usize,
        label: Option<SpannedText>,
        start: usize,
    ) -> FragmentSectionSyntax {
        let block = if matches!(self.peek_kind(), TokenKind::OpenBrace) {
            self.bump();
            self.parse_block(depth + 1, true, None, None)
        } else {
            ParsedBlock::default()
        };
        FragmentSectionSyntax {
            label,
            statements: block.statements,
            body_comment: block.closing_comment,
            span: SourceSpan::new(start, self.previous_end().max(start)),
        }
    }

    fn parse_try(&mut self, depth: usize, comment: Option<SpannedText>) -> Option<StatementSyntax> {
        let start = self.bump().span.start;
        let try_label = SpannedText::new("try", SourceSpan::new(start, self.previous_end()));
        let first = self.parse_required_fragment_section(depth, Some(try_label), "try", start);
        let mut sections = vec![first];

        while self.at_keyword(Keyword::Catch) || self.at_keyword(Keyword::Finally) {
            let is_catch = self.at_keyword(Keyword::Catch);
            let name = if is_catch { "catch" } else { "finally" };
            let branch_start = self.bump().span.start;
            let details = if is_catch && matches!(self.peek_kind(), TokenKind::OpenParen) {
                let (invocation, depth_error) =
                    self.grammar_match(|grammar, start| grammar.invocation(start));
                if let Some(span) = depth_error {
                    self.report_expression_depth_error(span);
                }
                invocation.map(|invocation| {
                    self.cursor = invocation.end;
                    trimmed_text(
                        self.source,
                        invocation.span.start + 1,
                        invocation.span.end.saturating_sub(1),
                    )
                    .unwrap_or_else(|| SpannedText::new(String::new(), invocation.span))
                })
            } else {
                None
            };
            let label = details.map_or_else(
                || SpannedText::new(name, SourceSpan::new(branch_start, self.previous_end())),
                |details| {
                    SpannedText::new(
                        format!("{name} {}", details.value),
                        SourceSpan::new(branch_start, details.span.end),
                    )
                },
            );
            sections.push(self.parse_required_fragment_section(
                depth,
                Some(label),
                name,
                branch_start,
            ));
            if !is_catch {
                break;
            }
        }
        let end = self.previous_end().max(start);
        Some(StatementSyntax {
            kind: StatementKindSyntax::Fragment(FragmentSyntax {
                kind: FragmentKindSyntax::TryCatchFinally,
                label: None,
                sections,
            }),
            comment,
            span: SourceSpan::new(start, end),
        })
    }

    fn parse_required_fragment_section(
        &mut self,
        depth: usize,
        label: Option<SpannedText>,
        context: &str,
        start: usize,
    ) -> FragmentSectionSyntax {
        let block = if matches!(self.peek_kind(), TokenKind::OpenBrace) {
            self.bump();
            self.parse_block(depth + 1, true, None, None)
        } else {
            self.error(
                format!("expected `{{` after ZenUML {context}"),
                self.insertion_span(),
            );
            ParsedBlock::default()
        };
        FragmentSectionSyntax {
            label,
            statements: block.statements,
            body_comment: block.closing_comment,
            span: SourceSpan::new(start, self.previous_end().max(start)),
        }
    }

    fn parse_reference(&mut self, comment: Option<SpannedText>) -> Option<StatementSyntax> {
        let start = self.bump().span.start;
        let mut participants = Vec::new();
        if matches!(self.peek_kind(), TokenKind::OpenParen) {
            self.bump();
            if let Some(name) = self.take_name() {
                participants.push(name);
                while matches!(self.peek_kind(), TokenKind::Comma) {
                    self.bump();
                    while let Some(name) = self.take_name() {
                        participants.push(name);
                    }
                }
            }
            if matches!(self.peek_kind(), TokenKind::CloseParen) {
                self.bump();
            } else {
                self.error("expected `)` after ZenUML reference", self.insertion_span());
            }
        } else {
            self.error("expected `(` after ZenUML `ref`", self.insertion_span());
        }
        if matches!(self.peek_kind(), TokenKind::Semicolon) {
            self.bump();
        }
        let end = self.previous_end().max(start);
        Some(StatementSyntax {
            kind: StatementKindSyntax::Reference(ReferenceSyntax {
                participants,
                span: SourceSpan::new(start, end),
            }),
            comment,
            span: SourceSpan::new(start, end),
        })
    }

    fn parse_return_statement(&mut self, comment: Option<SpannedText>) -> Option<StatementSyntax> {
        let start = self.bump().span.start;
        if matches!(
            self.tokens[self.cursor.saturating_sub(1)].kind,
            TokenKind::Keyword(Keyword::Return)
        ) {
            let (expr, depth_error) =
                self.grammar_match(|grammar, start| grammar.expression(start));
            if let Some(span) = depth_error {
                self.report_expression_depth_error(span);
            }
            let value = expr.map(|expr| {
                self.cursor = expr.end;
                trimmed_text(self.source, expr.span.start, expr.span.end)
                    .unwrap_or_else(|| SpannedText::new(String::new(), expr.span))
            });
            if matches!(self.peek_kind(), TokenKind::Semicolon) {
                self.bump();
            }
            let end = self.previous_end().max(start);
            return Some(StatementSyntax {
                kind: StatementKindSyntax::Return(ReturnSyntax {
                    from: None,
                    from_emoji: None,
                    to: None,
                    to_emoji: None,
                    value,
                }),
                comment,
                span: SourceSpan::new(start, end),
            });
        }

        let event = {
            let grammar = Grammar::with_control(self.source, &self.tokens, &self.control);
            grammar.async_message(self.cursor)
        };
        let Some(event) = event else {
            self.error(
                "expected async message after ZenUML return annotation",
                self.insertion_span(),
            );
            return None;
        };
        self.cursor = event.end;
        let end = self.previous_end().max(start);
        Some(StatementSyntax {
            kind: StatementKindSyntax::Return(ReturnSyntax {
                from: event.from.as_ref().map(|endpoint| endpoint.name.clone()),
                from_emoji: event.from.and_then(|endpoint| endpoint.emoji),
                to: event.to.as_ref().map(|endpoint| endpoint.name.clone()),
                to_emoji: event.to.and_then(|endpoint| endpoint.emoji),
                value: event.content,
            }),
            comment,
            span: SourceSpan::new(start, end),
        })
    }

    fn parse_divider(&mut self, comment: Option<SpannedText>) -> Option<StatementSyntax> {
        let token = self.bump().clone();
        let TokenKind::Divider(value) = token.kind else {
            return None;
        };
        let label = SpannedText::new(value, token.span);
        Some(StatementSyntax {
            kind: StatementKindSyntax::Divider(label),
            comment,
            span: token.span,
        })
    }

    fn parse_creation_message_async_or_return(
        &mut self,
        depth: usize,
        comment: Option<SpannedText>,
    ) -> Option<StatementSyntax> {
        let (creation, depth_error) =
            self.grammar_match(|grammar, start| grammar.creation_body(start));
        if let Some(span) = depth_error {
            self.report_expression_depth_error(span);
            return None;
        }
        if let Some(creation) = creation {
            return self.consume_creation(depth, comment, creation);
        }

        let (message, depth_error) =
            self.grammar_match(|grammar, start| grammar.message_body(start));
        if let Some(span) = depth_error {
            self.report_expression_depth_error(span);
            return None;
        }
        if let Some(message) = message.filter(|message| {
            let grammar = Grammar::with_control(self.source, &self.tokens, &self.control);
            grammar.message_candidate_is_complete(message.end)
        }) {
            return self.consume_message(depth, comment, message);
        }

        let event = {
            let grammar = Grammar::with_control(self.source, &self.tokens, &self.control);
            grammar.async_message(self.cursor)
        };
        if let Some(event) = event {
            return self.consume_async_message(comment, event);
        }

        let returned = {
            let grammar = Grammar::with_control(self.source, &self.tokens, &self.control);
            grammar.return_async_message(self.cursor)
        };
        returned.map(|returned| self.consume_return_async(comment, returned))
    }

    fn consume_creation(
        &mut self,
        depth: usize,
        comment: Option<SpannedText>,
        creation: CreationBodyMatch,
    ) -> Option<StatementSyntax> {
        let start = creation.span.start;
        self.cursor = creation.end;
        let body = self.consume_optional_statement_body(depth);
        let end = self.previous_end().max(start);
        let constructor = creation
            .constructor
            .unwrap_or_else(|| SpannedText::new(MISSING_CONSTRUCTOR, creation.new_span));
        Some(StatementSyntax {
            kind: StatementKindSyntax::Creation(CreationSyntax {
                constructor: constructor.clone(),
                assignment: creation.assignment,
                parameters: creation.parameters,
                body: body.statements,
                body_comment: body.closing_comment,
            }),
            comment,
            span: SourceSpan::new(start, end),
        })
    }

    fn consume_message(
        &mut self,
        depth: usize,
        comment: Option<SpannedText>,
        message: MessageBodyMatch,
    ) -> Option<StatementSyntax> {
        let start = message.span.start;
        self.cursor = message.end;
        let body = self.consume_optional_statement_body(depth);
        let end = self.previous_end().max(start);
        Some(StatementSyntax {
            kind: StatementKindSyntax::Message(MessageSyntax {
                from: message.from.as_ref().map(|endpoint| endpoint.name.clone()),
                from_emoji: message.from.and_then(|endpoint| endpoint.emoji),
                to: message.to.as_ref().map(|endpoint| endpoint.name.clone()),
                to_emoji: message.to.and_then(|endpoint| endpoint.emoji),
                signature: SpannedText::new(message.formatted, message.signature_span),
                assignment: message.assignment,
                style: MessageStyleSyntax::Synchronous,
                body: body.statements,
                body_comment: body.closing_comment,
            }),
            comment,
            span: SourceSpan::new(start, end),
        })
    }

    fn consume_optional_statement_body(&mut self, depth: usize) -> ParsedBlock {
        if matches!(self.peek_kind(), TokenKind::Semicolon) {
            self.bump();
            ParsedBlock::default()
        } else if matches!(self.peek_kind(), TokenKind::OpenBrace) {
            self.bump();
            self.parse_block(depth + 1, true, None, None)
        } else {
            ParsedBlock::default()
        }
    }

    fn consume_async_message(
        &mut self,
        comment: Option<SpannedText>,
        event: AsyncMessageMatch,
    ) -> Option<StatementSyntax> {
        let start = event.span.start;
        self.cursor = event.end;
        let end = self.previous_end().max(start);
        let signature = event
            .content
            .clone()
            .unwrap_or_else(|| SpannedText::new(String::new(), SourceSpan::new(end, end)));
        Some(StatementSyntax {
            kind: StatementKindSyntax::Message(MessageSyntax {
                from: event.from.as_ref().map(|endpoint| endpoint.name.clone()),
                from_emoji: event.from.and_then(|endpoint| endpoint.emoji),
                to: event.to.as_ref().map(|endpoint| endpoint.name.clone()),
                to_emoji: event.to.and_then(|endpoint| endpoint.emoji),
                signature,
                assignment: None,
                style: MessageStyleSyntax::Asynchronous,
                body: Vec::new(),
                body_comment: None,
            }),
            comment,
            span: SourceSpan::new(start, end),
        })
    }

    fn consume_return_async(
        &mut self,
        comment: Option<SpannedText>,
        returned: ReturnAsyncMatch,
    ) -> StatementSyntax {
        let start = returned.span.start;
        self.cursor = returned.end;
        let end = self.previous_end().max(start);
        StatementSyntax {
            kind: StatementKindSyntax::Return(ReturnSyntax {
                from: Some(returned.from.name),
                from_emoji: returned.from.emoji,
                to: returned.to.as_ref().map(|endpoint| endpoint.name.clone()),
                to_emoji: returned.to.and_then(|endpoint| endpoint.emoji),
                value: returned.content,
            }),
            comment,
            span: SourceSpan::new(start, end),
        }
    }

    fn take_current_comment(&mut self) -> Option<SpannedText> {
        self.comments.get_mut(self.cursor).and_then(Option::take)
    }

    fn discard_current_comment(&mut self) {
        if let Some(comment) = self.comments.get_mut(self.cursor) {
            *comment = None;
        }
    }

    fn grammar_match<T>(
        &self,
        parse: impl FnOnce(&Grammar<'_>, usize) -> Option<T>,
    ) -> (Option<T>, Option<SourceSpan>) {
        let grammar = Grammar::with_control(self.source, &self.tokens, &self.control);
        let value = parse(&grammar, self.cursor);
        (value, grammar.expression_depth_error())
    }

    fn report_expression_depth_error(&mut self, span: SourceSpan) {
        self.error(
            format!("ZenUML expression nesting depth exceeds {MAX_DIAGRAM_NESTING_DEPTH}"),
            span,
        );
    }

    fn take_name(&mut self) -> Option<SpannedText> {
        let name = token_name(self.peek())?;
        self.bump();
        Some(name)
    }

    fn skip_balanced_block(&mut self) {
        let mut depth = 0usize;
        while self.parse_active() && !self.at_eof() {
            match self.peek_kind() {
                TokenKind::OpenBrace => depth += 1,
                TokenKind::CloseBrace if depth == 0 => {
                    self.bump();
                    break;
                }
                TokenKind::CloseBrace => depth -= 1,
                _ => {}
            }
            self.bump();
        }
    }

    fn at_keyword(&self, keyword: Keyword) -> bool {
        matches!(self.peek_kind(), TokenKind::Keyword(found) if *found == keyword)
    }

    fn at_eof(&self) -> bool {
        matches!(self.peek_kind(), TokenKind::Eof)
    }

    fn parse_active(&self) -> bool {
        !self.control.is_cancelled()
    }

    fn peek(&self) -> &Token {
        &self.tokens[self.cursor.min(self.tokens.len().saturating_sub(1))]
    }

    fn peek_kind(&self) -> &TokenKind {
        &self.peek().kind
    }

    fn bump(&mut self) -> &Token {
        let index = self.cursor.min(self.tokens.len().saturating_sub(1));
        if self.cursor + 1 < self.tokens.len() {
            self.cursor += 1;
        }
        &self.tokens[index]
    }

    fn previous_end(&self) -> usize {
        self.tokens
            .get(self.cursor.saturating_sub(1))
            .map_or(0, |token| token.span.end)
    }

    fn insertion_span(&self) -> SourceSpan {
        SourceSpan::new(self.peek().span.start, self.peek().span.start)
    }

    fn error(&mut self, message: impl Into<String>, span: SourceSpan) {
        self.diagnostics.push(SyntaxDiagnostic {
            message: message.into(),
            span,
        });
    }
}

fn comments_before_default_tokens(
    raw_tokens: &[Token],
    control: &OperationControl,
) -> OperationControlResult<Vec<Option<SpannedText>>> {
    let mut slots = Vec::new();
    let mut pending = Vec::new();
    for (index, token) in raw_tokens.iter().enumerate() {
        if index % 128 == 0 {
            control.checkpoint()?;
        }
        match (&token.kind, token.channel) {
            (TokenKind::Comment(value), TokenChannel::Comment) => {
                pending.push(SpannedText::new(value.clone(), token.span));
            }
            (_, TokenChannel::Default) => {
                let comment = pending.first().map(|first| {
                    let end = pending.last().map_or(first.span.end, |last| last.span.end);
                    SpannedText::new(
                        pending
                            .iter()
                            .map(|comment| comment.value.as_str())
                            .collect::<Vec<_>>()
                            .join("\n"),
                        SourceSpan::new(first.span.start, end),
                    )
                });
                slots.push(comment);
                pending.clear();
            }
            _ => {}
        }
    }
    control.checkpoint()?;
    Ok(slots)
}

#[derive(Debug, Default)]
struct ParsedBlock {
    statements: Vec<StatementSyntax>,
    closing_comment: Option<SpannedText>,
}

#[derive(Debug, Clone, Copy)]
enum FragmentForm {
    OptionalBlock,
    RecoverRequiredBlock,
    Section,
}

#[derive(Debug, Clone)]
struct ParticipantMatch {
    end: usize,
    name: Option<SpannedText>,
    label: Option<SpannedText>,
    participant_type: Option<SpannedText>,
    stereotype: Option<SpannedText>,
    emoji: Option<SpannedText>,
    width: Option<SpannedText>,
    color: Option<SpannedText>,
    span: SourceSpan,
}

#[derive(Debug, Clone)]
struct StereotypeMatch {
    end: usize,
    value: Option<SpannedText>,
}

#[derive(Debug, Clone)]
struct EndpointMatch {
    end: usize,
    name: SpannedText,
    emoji: Option<SpannedText>,
}

#[derive(Debug, Clone)]
struct AssignmentMatch {
    end: usize,
    assignee: Option<SpannedText>,
}

#[derive(Debug, Clone)]
struct InvocationMatch {
    end: usize,
    formatted: String,
    parameters: String,
    parameters_span: SourceSpan,
    span: SourceSpan,
}

#[derive(Debug, Clone)]
struct SignatureMatch {
    end: usize,
    formatted: String,
    invoked: bool,
}

#[derive(Debug, Clone)]
struct FuncMatch {
    end: usize,
    formatted: String,
    span: SourceSpan,
    first_invoked: bool,
    first_emoji: bool,
}

#[derive(Debug, Clone)]
struct MessageBodyMatch {
    end: usize,
    assignment: Option<SpannedText>,
    from: Option<EndpointMatch>,
    to: Option<EndpointMatch>,
    formatted: String,
    signature_span: SourceSpan,
    span: SourceSpan,
}

#[derive(Debug, Clone)]
struct CreationBodyMatch {
    end: usize,
    assignment: Option<SpannedText>,
    constructor: Option<SpannedText>,
    parameters: Option<SpannedText>,
    new_span: SourceSpan,
    span: SourceSpan,
}

#[derive(Debug, Clone)]
struct AsyncMessageMatch {
    end: usize,
    from: Option<EndpointMatch>,
    to: Option<EndpointMatch>,
    content: Option<SpannedText>,
    span: SourceSpan,
}

#[derive(Debug, Clone)]
struct ReturnAsyncMatch {
    end: usize,
    from: EndpointMatch,
    to: Option<EndpointMatch>,
    content: Option<SpannedText>,
    span: SourceSpan,
}

#[derive(Debug, Clone)]
struct ExprMatch {
    end: usize,
    span: SourceSpan,
}

#[derive(Debug, Clone)]
struct ParameterMatch {
    end: usize,
    formatted: String,
}

#[derive(Debug, Clone)]
struct ParExprMatch {
    end: usize,
    label: Option<SpannedText>,
}

struct Grammar<'a> {
    source: &'a str,
    tokens: &'a [Token],
    control: OperationControl,
    expression_depth_error: Cell<Option<SourceSpan>>,
    #[cfg(test)]
    participant_candidate_evaluations: Cell<usize>,
}

enum GrammarValue {
    Expression(Option<ExprMatch>),
    Creation(Option<CreationBodyMatch>),
    Func(Option<FuncMatch>),
    Signature(Option<SignatureMatch>),
    Invocation(Option<InvocationMatch>),
    Parameter(Option<ParameterMatch>),
}

enum GrammarTask {
    Expression {
        start: usize,
        min_bp: u8,
        depth: usize,
    },
    ExpressionAfterPrefix {
        min_bp: u8,
        depth: usize,
    },
    ExpressionContinue {
        left: ExprMatch,
        min_bp: u8,
        depth: usize,
    },
    ExpressionAfterRight {
        left: ExprMatch,
        min_bp: u8,
        depth: usize,
    },
    Prefix {
        start: usize,
        depth: usize,
    },
    PrefixAfterUnary {
        start: usize,
    },
    PrefixAfterAssignment {
        start: usize,
        depth: usize,
    },
    PrefixTryParenthesized {
        start: usize,
        depth: usize,
    },
    PrefixAfterParenthesized {
        start: usize,
        depth: usize,
    },
    PrefixTryCreation {
        start: usize,
        depth: usize,
    },
    PrefixAfterCreation {
        start: usize,
        depth: usize,
    },
    PrefixTryMessagePath {
        start: usize,
        depth: usize,
    },
    PrefixAfterMessagePath {
        start: usize,
        depth: usize,
    },
    PrefixTryFunc {
        start: usize,
        depth: usize,
    },
    PrefixAfterFunc {
        start: usize,
    },
    Creation {
        start: usize,
        depth: usize,
    },
    CreationAfterInvocation {
        start: usize,
        assignment: Option<AssignmentMatch>,
        constructor: Option<SpannedText>,
        new_span: SourceSpan,
        invocation_start: usize,
    },
    Func {
        start: usize,
        depth: usize,
    },
    FuncAfterFirst {
        start: usize,
        depth: usize,
    },
    FuncContinue {
        start: usize,
        depth: usize,
        end: usize,
        formatted: String,
        first_invoked: bool,
        first_emoji: bool,
    },
    FuncAfterNext {
        start: usize,
        depth: usize,
        end: usize,
        formatted: String,
        first_invoked: bool,
        first_emoji: bool,
    },
    Signature {
        start: usize,
        depth: usize,
    },
    SignatureAfterInvocation {
        endpoint: EndpointMatch,
        formatted: String,
    },
    Invocation {
        start: usize,
        depth: usize,
    },
    InvocationNextParameter {
        start: usize,
        depth: usize,
        end: usize,
        parameters: Vec<ParameterMatch>,
    },
    InvocationAfterParameter {
        start: usize,
        depth: usize,
        parameter_start: usize,
        parameters: Vec<ParameterMatch>,
    },
    Parameter {
        start: usize,
        depth: usize,
    },
    ParameterAfterNamedExpression {
        start: usize,
        assignment_end: usize,
    },
    ParameterAfterExpression {
        start: usize,
    },
}

fn take_expression(result: &mut Option<GrammarValue>) -> Option<ExprMatch> {
    match result.take().expect("expression task result") {
        GrammarValue::Expression(value) => value,
        _ => unreachable!("expression continuation received a different grammar value"),
    }
}

fn take_creation(result: &mut Option<GrammarValue>) -> Option<CreationBodyMatch> {
    match result.take().expect("creation task result") {
        GrammarValue::Creation(value) => value,
        _ => unreachable!("creation continuation received a different grammar value"),
    }
}

fn take_func(result: &mut Option<GrammarValue>) -> Option<FuncMatch> {
    match result.take().expect("func task result") {
        GrammarValue::Func(value) => value,
        _ => unreachable!("func continuation received a different grammar value"),
    }
}

fn take_signature(result: &mut Option<GrammarValue>) -> Option<SignatureMatch> {
    match result.take().expect("signature task result") {
        GrammarValue::Signature(value) => value,
        _ => unreachable!("signature continuation received a different grammar value"),
    }
}

fn take_invocation(result: &mut Option<GrammarValue>) -> Option<InvocationMatch> {
    match result.take().expect("invocation task result") {
        GrammarValue::Invocation(value) => value,
        _ => unreachable!("invocation continuation received a different grammar value"),
    }
}

fn take_parameter(result: &mut Option<GrammarValue>) -> Option<ParameterMatch> {
    match result.take().expect("parameter task result") {
        GrammarValue::Parameter(value) => value,
        _ => unreachable!("parameter continuation received a different grammar value"),
    }
}

impl<'a> Grammar<'a> {
    #[cfg(test)]
    fn new(source: &'a str, tokens: &'a [Token]) -> Self {
        Self::with_control(source, tokens, &OperationControl::new())
    }

    fn with_control(source: &'a str, tokens: &'a [Token], control: &OperationControl) -> Self {
        Self {
            source,
            tokens,
            control: control.clone(),
            expression_depth_error: Cell::new(None),
            #[cfg(test)]
            participant_candidate_evaluations: Cell::new(0),
        }
    }

    fn expression_depth_error(&self) -> Option<SourceSpan> {
        self.expression_depth_error.get()
    }

    #[cfg(test)]
    fn expression_depth_exceeded(&self) -> bool {
        self.expression_depth_error.get().is_some()
    }

    #[cfg(test)]
    fn participant_candidate_evaluations(&self) -> usize {
        self.participant_candidate_evaluations.get()
    }

    fn run_grammar_task(&self, initial: GrammarTask) -> GrammarValue {
        let cancellation_kind = match &initial {
            GrammarTask::Expression { .. } => 0,
            GrammarTask::Creation { .. } => 1,
            GrammarTask::Func { .. } => 2,
            GrammarTask::Invocation { .. } => 3,
            _ => unreachable!("only grammar roots may start a grammar task"),
        };
        self.expression_depth_error.set(None);
        let mut tasks = vec![initial];
        let mut result = None;
        let mut inspected = 0usize;

        while let Some(task) = tasks.pop() {
            if inspected.is_multiple_of(128) && self.control.checkpoint().is_err() {
                return match cancellation_kind {
                    0 => GrammarValue::Expression(None),
                    1 => GrammarValue::Creation(None),
                    2 => GrammarValue::Func(None),
                    3 => GrammarValue::Invocation(None),
                    _ => unreachable!("known cancellation projection"),
                };
            }
            inspected = inspected.saturating_add(1);
            match task {
                GrammarTask::Expression {
                    start,
                    min_bp,
                    depth,
                } => {
                    if depth > MAX_DIAGRAM_NESTING_DEPTH {
                        if self.expression_depth_error.get().is_none() {
                            self.expression_depth_error
                                .set(Some(self.span(start, start.saturating_add(1))));
                        }
                        result = Some(GrammarValue::Expression(None));
                    } else {
                        tasks.push(GrammarTask::ExpressionAfterPrefix { min_bp, depth });
                        tasks.push(GrammarTask::Prefix { start, depth });
                    }
                }
                GrammarTask::ExpressionAfterPrefix { min_bp, depth } => {
                    if let Some(left) = take_expression(&mut result) {
                        tasks.push(GrammarTask::ExpressionContinue {
                            left,
                            min_bp,
                            depth,
                        });
                    } else {
                        result = Some(GrammarValue::Expression(None));
                    }
                }
                GrammarTask::ExpressionContinue {
                    left,
                    min_bp,
                    depth,
                } => {
                    let Some((left_bp, right_bp)) = self.binary_binding_power(left.end) else {
                        result = Some(GrammarValue::Expression(Some(left)));
                        continue;
                    };
                    if left_bp < min_bp {
                        result = Some(GrammarValue::Expression(Some(left)));
                        continue;
                    }
                    let right_start = left.end + 1;
                    tasks.push(GrammarTask::ExpressionAfterRight {
                        left,
                        min_bp,
                        depth,
                    });
                    tasks.push(GrammarTask::Expression {
                        start: right_start,
                        min_bp: right_bp,
                        depth: depth + 1,
                    });
                }
                GrammarTask::ExpressionAfterRight {
                    left,
                    min_bp,
                    depth,
                } => {
                    let Some(right) = take_expression(&mut result) else {
                        result = Some(GrammarValue::Expression(
                            self.expression_depth_error.get().is_none().then_some(left),
                        ));
                        continue;
                    };
                    let combined = ExprMatch {
                        end: right.end,
                        span: SourceSpan::new(left.span.start, right.span.end),
                    };
                    tasks.push(GrammarTask::ExpressionContinue {
                        left: combined,
                        min_bp,
                        depth,
                    });
                }
                GrammarTask::Prefix { start, depth } => {
                    if matches!(self.kind(start), Some(TokenKind::Operator(value)) if value == "-" || value == "!")
                    {
                        tasks.push(GrammarTask::PrefixAfterUnary { start });
                        tasks.push(GrammarTask::Expression {
                            start: start + 1,
                            min_bp: 13,
                            depth: depth + 1,
                        });
                    } else if let Some(assignment) = self.assignment(start) {
                        tasks.push(GrammarTask::PrefixAfterAssignment { start, depth });
                        tasks.push(GrammarTask::Expression {
                            start: assignment.end,
                            min_bp: 0,
                            depth: depth + 1,
                        });
                    } else {
                        tasks.push(GrammarTask::PrefixTryParenthesized { start, depth });
                    }
                }
                GrammarTask::PrefixAfterUnary { start } => {
                    result = Some(GrammarValue::Expression(take_expression(&mut result).map(
                        |inner| ExprMatch {
                            end: inner.end,
                            span: self.span(start, inner.end),
                        },
                    )));
                }
                GrammarTask::PrefixAfterAssignment { start, depth } => {
                    if let Some(expr) = take_expression(&mut result) {
                        result = Some(GrammarValue::Expression(Some(ExprMatch {
                            end: expr.end,
                            span: self.span(start, expr.end),
                        })));
                    } else if self.expression_depth_error.get().is_some() {
                        result = Some(GrammarValue::Expression(None));
                    } else {
                        tasks.push(GrammarTask::PrefixTryParenthesized { start, depth });
                    }
                }
                GrammarTask::PrefixTryParenthesized { start, depth } => {
                    if matches!(self.kind(start), Some(TokenKind::OpenParen)) {
                        tasks.push(GrammarTask::PrefixAfterParenthesized { start, depth });
                        tasks.push(GrammarTask::Expression {
                            start: start + 1,
                            min_bp: 0,
                            depth: depth + 1,
                        });
                    } else if self.expression_depth_error.get().is_some() {
                        result = Some(GrammarValue::Expression(None));
                    } else {
                        tasks.push(GrammarTask::PrefixTryCreation { start, depth });
                    }
                }
                GrammarTask::PrefixAfterParenthesized { start, depth } => {
                    let expr = take_expression(&mut result);
                    if let Some(expr) = expr
                        && matches!(self.kind(expr.end), Some(TokenKind::CloseParen))
                    {
                        let end = expr.end + 1;
                        result = Some(GrammarValue::Expression(Some(ExprMatch {
                            end,
                            span: self.span(start, end),
                        })));
                    } else {
                        tasks.push(GrammarTask::PrefixTryCreation { start, depth });
                    }
                }
                GrammarTask::PrefixTryCreation { start, depth } => {
                    tasks.push(GrammarTask::PrefixAfterCreation { start, depth });
                    tasks.push(GrammarTask::Creation { start, depth });
                }
                GrammarTask::PrefixAfterCreation { start, depth } => {
                    if let Some(creation) = take_creation(&mut result) {
                        result = Some(GrammarValue::Expression(Some(ExprMatch {
                            end: creation.end,
                            span: creation.span,
                        })));
                    } else if self.expression_depth_error.get().is_some() {
                        result = Some(GrammarValue::Expression(None));
                    } else {
                        tasks.push(GrammarTask::PrefixTryMessagePath { start, depth });
                    }
                }
                GrammarTask::PrefixTryMessagePath { start, depth } => {
                    if let Some(from_to) = self.message_path(start) {
                        tasks.push(GrammarTask::PrefixAfterMessagePath { start, depth });
                        tasks.push(GrammarTask::Func {
                            start: from_to.end,
                            depth,
                        });
                    } else if self.expression_depth_error.get().is_some() {
                        result = Some(GrammarValue::Expression(None));
                    } else {
                        tasks.push(GrammarTask::PrefixTryFunc { start, depth });
                    }
                }
                GrammarTask::PrefixAfterMessagePath { start, depth } => {
                    if let Some(func) = take_func(&mut result) {
                        result = Some(GrammarValue::Expression(Some(ExprMatch {
                            end: func.end,
                            span: self.span(start, func.end),
                        })));
                    } else {
                        tasks.push(GrammarTask::PrefixTryFunc { start, depth });
                    }
                }
                GrammarTask::PrefixTryFunc { start, depth } => {
                    tasks.push(GrammarTask::PrefixAfterFunc { start });
                    tasks.push(GrammarTask::Func { start, depth });
                }
                GrammarTask::PrefixAfterFunc { start } => {
                    let func = take_func(&mut result);
                    match func {
                        Some(func) if func.first_invoked || func.first_emoji => {
                            result = Some(GrammarValue::Expression(Some(ExprMatch {
                                end: func.end,
                                span: func.span,
                            })));
                        }
                        _ if self.expression_depth_error.get().is_some() => {
                            result = Some(GrammarValue::Expression(None));
                        }
                        _ if self.is_atom(start) => {
                            result = Some(GrammarValue::Expression(Some(ExprMatch {
                                end: start + 1,
                                span: self.span(start, start + 1),
                            })));
                        }
                        _ => result = Some(GrammarValue::Expression(None)),
                    }
                }
                GrammarTask::Creation { start, depth } => {
                    let assignment = self.assignment(start);
                    let cursor = assignment
                        .as_ref()
                        .map_or(start, |assignment| assignment.end);
                    if !matches!(self.kind(cursor), Some(TokenKind::Keyword(Keyword::New))) {
                        result = Some(GrammarValue::Creation(None));
                        continue;
                    }
                    let new_span = self.tokens[cursor].span;
                    let mut invocation_start = cursor + 1;
                    let constructor = self.name(invocation_start);
                    if constructor.is_some() {
                        invocation_start += 1;
                    }
                    if constructor.is_some()
                        && matches!(self.kind(invocation_start), Some(TokenKind::OpenParen))
                    {
                        tasks.push(GrammarTask::CreationAfterInvocation {
                            start,
                            assignment,
                            constructor,
                            new_span,
                            invocation_start,
                        });
                        tasks.push(GrammarTask::Invocation {
                            start: invocation_start,
                            depth,
                        });
                    } else {
                        let span = self.span(start, invocation_start);
                        result = Some(GrammarValue::Creation(Some(CreationBodyMatch {
                            end: invocation_start,
                            assignment: assignment.and_then(|assignment| assignment.assignee),
                            constructor,
                            parameters: None,
                            new_span,
                            span,
                        })));
                    }
                }
                GrammarTask::CreationAfterInvocation {
                    start,
                    assignment,
                    constructor,
                    new_span,
                    invocation_start,
                } => {
                    let invocation = take_invocation(&mut result);
                    if invocation.is_none() && self.expression_depth_error.get().is_some() {
                        result = Some(GrammarValue::Creation(None));
                        continue;
                    }
                    let end = invocation
                        .as_ref()
                        .map_or(invocation_start, |invocation| invocation.end);
                    let parameters = invocation.map(|invocation| {
                        SpannedText::new(invocation.parameters, invocation.parameters_span)
                    });
                    let span = self.span(start, end);
                    result = Some(GrammarValue::Creation(Some(CreationBodyMatch {
                        end,
                        assignment: assignment.and_then(|assignment| assignment.assignee),
                        constructor,
                        parameters,
                        new_span,
                        span,
                    })));
                }
                GrammarTask::Func { start, depth } => {
                    tasks.push(GrammarTask::FuncAfterFirst { start, depth });
                    tasks.push(GrammarTask::Signature { start, depth });
                }
                GrammarTask::FuncAfterFirst { start, depth } => {
                    let Some(first) = take_signature(&mut result) else {
                        result = Some(GrammarValue::Func(None));
                        continue;
                    };
                    let first_emoji = matches!(self.kind(start), Some(TokenKind::OpenBracket));
                    tasks.push(GrammarTask::FuncContinue {
                        start,
                        depth,
                        end: first.end,
                        formatted: first.formatted,
                        first_invoked: first.invoked,
                        first_emoji,
                    });
                }
                GrammarTask::FuncContinue {
                    start,
                    depth,
                    end,
                    formatted,
                    first_invoked,
                    first_emoji,
                } => {
                    if matches!(self.kind(end), Some(TokenKind::Dot)) {
                        tasks.push(GrammarTask::FuncAfterNext {
                            start,
                            depth,
                            end,
                            formatted,
                            first_invoked,
                            first_emoji,
                        });
                        tasks.push(GrammarTask::Signature {
                            start: end + 1,
                            depth,
                        });
                    } else {
                        let span = self.span(start, end);
                        result = Some(GrammarValue::Func(Some(FuncMatch {
                            end,
                            formatted,
                            span,
                            first_invoked,
                            first_emoji,
                        })));
                    }
                }
                GrammarTask::FuncAfterNext {
                    start,
                    depth,
                    end,
                    mut formatted,
                    first_invoked,
                    first_emoji,
                } => {
                    let Some(signature) = take_signature(&mut result) else {
                        if self.expression_depth_error.get().is_some() {
                            result = Some(GrammarValue::Func(None));
                        } else {
                            let span = self.span(start, end);
                            result = Some(GrammarValue::Func(Some(FuncMatch {
                                end,
                                formatted,
                                span,
                                first_invoked,
                                first_emoji,
                            })));
                        }
                        continue;
                    };
                    formatted.push('.');
                    formatted.push_str(&signature.formatted);
                    tasks.push(GrammarTask::FuncContinue {
                        start,
                        depth,
                        end: signature.end,
                        formatted,
                        first_invoked,
                        first_emoji,
                    });
                }
                GrammarTask::Signature { start, depth } => {
                    let Some(endpoint) = self.endpoint(start) else {
                        result = Some(GrammarValue::Signature(None));
                        continue;
                    };
                    let mut formatted = String::new();
                    if let Some(emoji) = &endpoint.emoji {
                        formatted.push('[');
                        formatted.push_str(&emoji.value);
                        formatted.push(']');
                    }
                    formatted.push_str(&endpoint.name.value);
                    if matches!(self.kind(endpoint.end), Some(TokenKind::OpenParen)) {
                        let invocation_start = endpoint.end;
                        tasks.push(GrammarTask::SignatureAfterInvocation {
                            endpoint,
                            formatted,
                        });
                        tasks.push(GrammarTask::Invocation {
                            start: invocation_start,
                            depth,
                        });
                    } else {
                        result = Some(GrammarValue::Signature(Some(SignatureMatch {
                            end: endpoint.end,
                            formatted,
                            invoked: false,
                        })));
                    }
                }
                GrammarTask::SignatureAfterInvocation {
                    endpoint,
                    mut formatted,
                } => {
                    if let Some(invocation) = take_invocation(&mut result) {
                        formatted.push_str(&invocation.formatted);
                        result = Some(GrammarValue::Signature(Some(SignatureMatch {
                            end: invocation.end,
                            formatted,
                            invoked: true,
                        })));
                    } else if self.expression_depth_error.get().is_some() {
                        result = Some(GrammarValue::Signature(None));
                    } else {
                        result = Some(GrammarValue::Signature(Some(SignatureMatch {
                            end: endpoint.end,
                            formatted,
                            invoked: false,
                        })));
                    }
                }
                GrammarTask::Invocation { start, depth } => {
                    if !matches!(self.kind(start), Some(TokenKind::OpenParen)) {
                        result = Some(GrammarValue::Invocation(None));
                        continue;
                    }
                    let end = start + 1;
                    if matches!(self.kind(end), Some(TokenKind::CloseParen)) {
                        result = Some(GrammarValue::Invocation(Some(self.finish_invocation(
                            start,
                            end + 1,
                            Vec::new(),
                        ))));
                    } else {
                        tasks.push(GrammarTask::InvocationNextParameter {
                            start,
                            depth,
                            end,
                            parameters: Vec::new(),
                        });
                    }
                }
                GrammarTask::InvocationNextParameter {
                    start,
                    depth,
                    end,
                    parameters,
                } => {
                    tasks.push(GrammarTask::InvocationAfterParameter {
                        start,
                        depth,
                        parameter_start: end,
                        parameters,
                    });
                    tasks.push(GrammarTask::Parameter { start: end, depth });
                }
                GrammarTask::InvocationAfterParameter {
                    start,
                    depth,
                    parameter_start,
                    mut parameters,
                } => {
                    let Some(parameter) = take_parameter(&mut result) else {
                        result = Some(GrammarValue::Invocation(None));
                        continue;
                    };
                    if parameter.end == parameter_start {
                        result = Some(GrammarValue::Invocation(None));
                        continue;
                    }
                    let mut end = parameter.end;
                    parameters.push(parameter);
                    if matches!(self.kind(end), Some(TokenKind::Comma)) {
                        end += 1;
                        if matches!(self.kind(end), Some(TokenKind::CloseParen)) {
                            result = Some(GrammarValue::Invocation(Some(self.finish_invocation(
                                start,
                                end + 1,
                                parameters,
                            ))));
                        } else {
                            tasks.push(GrammarTask::InvocationNextParameter {
                                start,
                                depth,
                                end,
                                parameters,
                            });
                        }
                    } else if matches!(self.kind(end), Some(TokenKind::CloseParen)) {
                        result = Some(GrammarValue::Invocation(Some(self.finish_invocation(
                            start,
                            end + 1,
                            parameters,
                        ))));
                    } else {
                        result = Some(GrammarValue::Invocation(None));
                    }
                }
                GrammarTask::Parameter { start, depth } => {
                    if self.is_identifier_name(start)
                        && matches!(self.kind(start + 1), Some(TokenKind::Assign))
                    {
                        let assignment_end = start + 2;
                        tasks.push(GrammarTask::ParameterAfterNamedExpression {
                            start,
                            assignment_end,
                        });
                        tasks.push(GrammarTask::Expression {
                            start: assignment_end,
                            min_bp: 0,
                            depth: depth + 1,
                        });
                    } else if self.name(start).is_some() && self.is_identifier_name(start + 1) {
                        let end = start + 2;
                        let formatted =
                            format!("{} {}", self.token_text(start), self.token_text(start + 1));
                        result = Some(GrammarValue::Parameter(Some(ParameterMatch {
                            end,
                            formatted,
                        })));
                    } else {
                        tasks.push(GrammarTask::ParameterAfterExpression { start });
                        tasks.push(GrammarTask::Expression {
                            start,
                            min_bp: 0,
                            depth: depth + 1,
                        });
                    }
                }
                GrammarTask::ParameterAfterNamedExpression {
                    start,
                    assignment_end,
                } => {
                    let expression = take_expression(&mut result);
                    if expression.is_none() && self.expression_depth_error.get().is_some() {
                        result = Some(GrammarValue::Parameter(None));
                        continue;
                    }
                    let end = expression.map_or(assignment_end, |expression| expression.end);
                    let formatted = self.compact(start, end);
                    result = Some(GrammarValue::Parameter(Some(ParameterMatch {
                        end,
                        formatted,
                    })));
                }
                GrammarTask::ParameterAfterExpression { start } => {
                    result = Some(GrammarValue::Parameter(take_expression(&mut result).map(
                        |expression| ParameterMatch {
                            end: expression.end,
                            formatted: self.compact(start, expression.end),
                        },
                    )));
                }
            }
        }

        result.expect("grammar task must produce a value")
    }

    fn finish_invocation(
        &self,
        start: usize,
        end: usize,
        parameters: Vec<ParameterMatch>,
    ) -> InvocationMatch {
        let parameters = parameters
            .iter()
            .map(|parameter| parameter.formatted.as_str())
            .collect::<Vec<_>>()
            .join(",");
        InvocationMatch {
            end,
            formatted: format!("({parameters})"),
            parameters: parameters.clone(),
            parameters_span: SourceSpan::new(
                self.tokens[start].span.end,
                self.tokens[end - 1].span.start,
            ),
            span: self.span(start, end),
        }
    }

    fn head_suffix_terminal(&self, start: usize) -> bool {
        self.is_eof(start)
            || matches!(self.kind(start), Some(TokenKind::StarterAnnotation))
            || matches!(self.kind(start), Some(TokenKind::Keyword(Keyword::Group)))
            || self.can_start_statement(start)
    }

    fn participant_candidates(&self, start: usize) -> Vec<ParticipantMatch> {
        #[cfg(test)]
        self.participant_candidate_evaluations.set(
            self.participant_candidate_evaluations
                .get()
                .saturating_add(1),
        );
        let mut out = Vec::new();
        match self.kind(start) {
            Some(TokenKind::Annotation(value)) => {
                let participant_type =
                    Some(SpannedText::new(value.clone(), self.tokens[start].span));
                for stereotype in self.stereotype_candidates(start + 1) {
                    self.push_decorated_participant_candidates(
                        start,
                        stereotype.end,
                        participant_type.clone(),
                        stereotype.value,
                        &mut out,
                    );
                }
                self.push_decorated_participant_candidates(
                    start,
                    start + 1,
                    participant_type.clone(),
                    None,
                    &mut out,
                );
                out.push(self.decorator_only(start, start + 1, participant_type, None));
            }
            kind if matches!(kind, Some(TokenKind::StereotypeOpen))
                || matches!(kind, Some(TokenKind::Operator(value)) if value == "<") =>
            {
                for stereotype in self.stereotype_candidates(start) {
                    self.push_decorated_participant_candidates(
                        start,
                        stereotype.end,
                        None,
                        stereotype.value.clone(),
                        &mut out,
                    );
                    out.push(self.decorator_only(start, stereotype.end, None, stereotype.value));
                }
            }
            _ => {
                if let Some(candidate) = self.finish_participant(start, start, None, None) {
                    out.push(candidate);
                }
            }
        }
        out.sort_by_key(|candidate| std::cmp::Reverse(candidate.end));
        out.dedup_by(|left, right| left.end == right.end && left.name == right.name);
        out
    }

    fn push_decorated_participant_candidates(
        &self,
        start: usize,
        cursor: usize,
        participant_type: Option<SpannedText>,
        stereotype: Option<SpannedText>,
        out: &mut Vec<ParticipantMatch>,
    ) {
        if let Some(candidate) =
            self.finish_participant(start, cursor, participant_type, stereotype)
        {
            out.push(candidate);
        }
    }

    fn finish_participant(
        &self,
        start: usize,
        mut cursor: usize,
        participant_type: Option<SpannedText>,
        stereotype: Option<SpannedText>,
    ) -> Option<ParticipantMatch> {
        let (emoji, after_emoji) = self.emoji(cursor).map_or((None, cursor), |emoji| {
            let end = emoji.end;
            (Some(emoji.name), end)
        });
        cursor = after_emoji;
        let name = self.name(cursor)?;
        cursor += 1;
        let width = match self.kind(cursor) {
            Some(TokenKind::Integer(value)) => {
                let width = SpannedText::new(value.clone(), self.tokens[cursor].span);
                cursor += 1;
                Some(width)
            }
            _ => None,
        };
        let label = if matches!(self.kind(cursor), Some(TokenKind::Keyword(Keyword::As))) {
            cursor += 1;
            let label = self.name(cursor);
            if label.is_some() {
                cursor += 1;
            }
            label
        } else {
            None
        };
        let color = match self.kind(cursor) {
            Some(TokenKind::Color(value)) => {
                let color = SpannedText::new(value.clone(), self.tokens[cursor].span);
                cursor += 1;
                Some(color)
            }
            _ => None,
        };
        let span = self.span(start, cursor);
        Some(ParticipantMatch {
            end: cursor,
            name: Some(name),
            label,
            participant_type,
            stereotype,
            emoji,
            width,
            color,
            span,
        })
    }

    fn decorator_only(
        &self,
        start: usize,
        end: usize,
        participant_type: Option<SpannedText>,
        stereotype: Option<SpannedText>,
    ) -> ParticipantMatch {
        let span = self.span(start, end);
        ParticipantMatch {
            end,
            name: None,
            label: None,
            participant_type,
            stereotype,
            emoji: None,
            width: None,
            color: None,
            span,
        }
    }

    fn stereotype_candidates(&self, start: usize) -> Vec<StereotypeMatch> {
        let mut out = Vec::new();
        let Some(open) = self.kind(start) else {
            return out;
        };
        if !matches!(open, TokenKind::StereotypeOpen)
            && !matches!(open, TokenKind::Operator(value) if value == "<")
        {
            return out;
        }

        if matches!(open, TokenKind::StereotypeOpen)
            && let Some(name) = self.name(start + 1)
        {
            let mut end = start + 2;
            if matches!(self.kind(end), Some(TokenKind::StereotypeClose))
                || matches!(self.kind(end), Some(TokenKind::Operator(value)) if value == ">")
            {
                end += 1;
            }
            out.push(StereotypeMatch {
                end,
                value: Some(name),
            });
        }

        let mut bare_end = start + 1;
        if matches!(self.kind(bare_end), Some(TokenKind::StereotypeClose))
            || matches!(self.kind(bare_end), Some(TokenKind::Operator(value)) if value == ">")
        {
            bare_end += 1;
        }
        out.push(StereotypeMatch {
            end: bare_end,
            value: None,
        });
        out.sort_by_key(|candidate| std::cmp::Reverse(candidate.end));
        out
    }

    fn message_body(&self, start: usize) -> Option<MessageBodyMatch> {
        if let Some(assignment) = self.assignment(start) {
            let mut from = None;
            let mut to = None;
            let mut formatted = String::new();
            let mut signature_span = SourceSpan::new(
                self.token_end(assignment.end.saturating_sub(1)),
                self.token_end(assignment.end.saturating_sub(1)),
            );
            let mut end = assignment.end;
            if let Some(from_to) = self.message_path(end) {
                from = from_to.from;
                to = Some(from_to.to);
                end = from_to.end;
                if let Some(func) = self.func(end) {
                    formatted = func.formatted;
                    signature_span = func.span;
                    end = func.end;
                }
            } else if let Some(func) = self.bare_func(end) {
                formatted = func.formatted;
                signature_span = func.span;
                end = func.end;
            }
            let span = self.span(start, end);
            return Some(MessageBodyMatch {
                end,
                assignment: assignment.assignee,
                from,
                to,
                formatted,
                signature_span,
                span,
            });
        }

        if let Some(from_to) = self.message_path(start) {
            let mut end = from_to.end;
            let (formatted, signature_span) = if let Some(func) = self.func(end) {
                end = func.end;
                (func.formatted.clone(), func.span)
            } else {
                let offset = self.token_end(end.saturating_sub(1));
                (String::new(), SourceSpan::new(offset, offset))
            };
            let span = self.span(start, end);
            return Some(MessageBodyMatch {
                end,
                assignment: None,
                from: from_to.from,
                to: Some(from_to.to),
                formatted,
                signature_span,
                span,
            });
        }

        let func = self.bare_func(start)?;
        let span = self.span(start, func.end);
        Some(MessageBodyMatch {
            end: func.end,
            assignment: None,
            from: None,
            to: None,
            formatted: func.formatted.clone(),
            signature_span: func.span,
            span,
        })
    }

    fn creation_body(&self, start: usize) -> Option<CreationBodyMatch> {
        match self.run_grammar_task(GrammarTask::Creation { start, depth: 0 }) {
            GrammarValue::Creation(value) => value,
            _ => unreachable!("creation task returned a different grammar value"),
        }
    }

    fn async_message(&self, start: usize) -> Option<AsyncMessageMatch> {
        let first = self.endpoint(start)?;
        if matches!(self.kind(first.end), Some(TokenKind::Arrow)) {
            if let Some(to) = self.endpoint(first.end + 1) {
                if matches!(self.kind(to.end), Some(TokenKind::Colon)) {
                    let event_start = to.end + 1;
                    return Some(self.finish_event(start, Some(first), Some(to), event_start));
                }
                let end = to.end;
                return Some(AsyncMessageMatch {
                    end,
                    from: Some(first),
                    to: Some(to),
                    content: None,
                    span: self.span(start, end),
                });
            }
            let end = first.end + 1;
            return Some(AsyncMessageMatch {
                end,
                from: Some(first),
                to: None,
                content: None,
                span: self.span(start, end),
            });
        }
        if matches!(self.kind(first.end), Some(TokenKind::Colon)) {
            return Some(self.finish_event(start, None, Some(first.clone()), first.end + 1));
        }
        if matches!(self.kind(first.end), Some(TokenKind::Operator(value)) if value == "-") {
            let to = self.endpoint(first.end + 1);
            let end = to.as_ref().map_or(first.end + 1, |to| to.end);
            return Some(AsyncMessageMatch {
                end,
                from: Some(first),
                to,
                content: None,
                span: self.span(start, end),
            });
        }
        None
    }

    fn finish_event(
        &self,
        start: usize,
        from: Option<EndpointMatch>,
        to: Option<EndpointMatch>,
        mut end: usize,
    ) -> AsyncMessageMatch {
        let content = match self.kind(end) {
            Some(TokenKind::EventPayload(value)) => {
                let token = &self.tokens[end];
                end += 1;
                trimmed_text(self.source, token.span.start, token.span.end)
                    .or_else(|| Some(SpannedText::new(value.clone(), token.span)))
            }
            _ => None,
        };
        if matches!(self.kind(end), Some(TokenKind::EventEnd)) {
            end += 1;
        }
        AsyncMessageMatch {
            end,
            from,
            to,
            content,
            span: self.span(start, end),
        }
    }

    fn return_async_message(&self, start: usize) -> Option<ReturnAsyncMatch> {
        let from = self.endpoint(start)?;
        if !matches!(self.kind(from.end), Some(TokenKind::ReturnArrow)) {
            return None;
        }
        let mut end = from.end + 1;
        let to = self.endpoint(end);
        if let Some(to) = &to {
            end = to.end;
        }
        let content = if matches!(self.kind(end), Some(TokenKind::Colon)) {
            end += 1;
            let value = match self.kind(end) {
                Some(TokenKind::EventPayload(value)) => {
                    let token = &self.tokens[end];
                    end += 1;
                    trimmed_text(self.source, token.span.start, token.span.end)
                        .or_else(|| Some(SpannedText::new(value.clone(), token.span)))
                }
                _ => None,
            };
            if matches!(self.kind(end), Some(TokenKind::EventEnd)) {
                end += 1;
            }
            value
        } else {
            None
        };
        Some(ReturnAsyncMatch {
            end,
            from,
            to,
            content,
            span: self.span(start, end),
        })
    }

    fn message_path(&self, start: usize) -> Option<FromToMatch> {
        let first = self.endpoint(start)?;
        if matches!(self.kind(first.end), Some(TokenKind::Arrow)) {
            let to = self.endpoint(first.end + 1)?;
            if !matches!(self.kind(to.end), Some(TokenKind::Dot)) {
                return None;
            }
            return Some(FromToMatch {
                end: to.end + 1,
                from: Some(first),
                to,
            });
        }
        if matches!(self.kind(first.end), Some(TokenKind::Dot)) {
            return Some(FromToMatch {
                end: first.end + 1,
                from: None,
                to: first,
            });
        }
        None
    }

    fn endpoint(&self, start: usize) -> Option<EndpointMatch> {
        let (emoji, cursor) = self.emoji(start).map_or((None, start), |emoji| {
            let end = emoji.end;
            (Some(emoji.name), end)
        });
        let name = self.name(cursor)?;
        Some(EndpointMatch {
            end: cursor + 1,
            name,
            emoji,
        })
    }

    fn emoji(&self, start: usize) -> Option<EmojiMatch> {
        if !matches!(self.kind(start), Some(TokenKind::OpenBracket)) {
            return None;
        }
        let name = self.name(start + 1)?;
        if !matches!(self.kind(start + 2), Some(TokenKind::CloseBracket)) {
            return None;
        }
        Some(EmojiMatch {
            end: start + 3,
            name,
        })
    }

    fn func(&self, start: usize) -> Option<FuncMatch> {
        match self.run_grammar_task(GrammarTask::Func { start, depth: 0 }) {
            GrammarValue::Func(value) => value,
            _ => unreachable!("func task returned a different grammar value"),
        }
    }

    fn bare_func(&self, start: usize) -> Option<FuncMatch> {
        self.func(start)
    }

    fn invocation(&self, start: usize) -> Option<InvocationMatch> {
        match self.run_grammar_task(GrammarTask::Invocation { start, depth: 0 }) {
            GrammarValue::Invocation(value) => value,
            _ => unreachable!("invocation task returned a different grammar value"),
        }
    }

    fn assignment(&self, start: usize) -> Option<AssignmentMatch> {
        if self.name(start).is_some()
            && let Some((assignee, assignee_end)) = self.assignee(start + 1)
            && matches!(self.kind(assignee_end), Some(TokenKind::Assign))
        {
            let end = assignee_end + 1;
            return Some(AssignmentMatch { end, assignee });
        }
        let (assignee, assignee_end) = self.assignee(start)?;
        if !matches!(self.kind(assignee_end), Some(TokenKind::Assign)) {
            return None;
        }
        let end = assignee_end + 1;
        Some(AssignmentMatch { end, assignee })
    }

    fn assignee(&self, start: usize) -> Option<(Option<SpannedText>, usize)> {
        if self.is_identifier_name(start) && matches!(self.kind(start + 1), Some(TokenKind::Comma))
        {
            let mut end = start + 1;
            let mut last = self.name(start);
            while matches!(self.kind(end), Some(TokenKind::Comma))
                && self.is_identifier_name(end + 1)
            {
                last = self.name(end + 1);
                end += 2;
            }
            return Some((last, end));
        }
        if matches!(self.kind(start), Some(TokenKind::Keyword(Keyword::New))) {
            return Some((
                Some(SpannedText::new("new", self.tokens[start].span)),
                start + 1,
            ));
        }
        if self.is_atom(start) {
            return Some((self.name(start), start + 1));
        }
        None
    }

    fn expression(&self, start: usize) -> Option<ExprMatch> {
        match self.run_grammar_task(GrammarTask::Expression {
            start,
            min_bp: 0,
            depth: 0,
        }) {
            GrammarValue::Expression(value) => value,
            _ => unreachable!("expression task returned a different grammar value"),
        }
    }

    fn binary_binding_power(&self, index: usize) -> Option<(u8, u8)> {
        let precedence = match self.kind(index) {
            Some(TokenKind::Operator(value)) if value == "||" => 1,
            Some(TokenKind::Operator(value)) if value == "&&" => 3,
            Some(TokenKind::Operator(value)) if value == "==" || value == "!=" => 5,
            Some(TokenKind::Operator(value))
                if matches!(value.as_str(), "<" | ">" | "<=" | ">=") =>
            {
                7
            }
            Some(TokenKind::Operator(value)) if value == "+" || value == "-" => 9,
            Some(TokenKind::Operator(value)) if matches!(value.as_str(), "*" | "/" | "%") => 11,
            _ => return None,
        };
        Some((precedence, precedence + 1))
    }

    fn par_expr(&self, start: usize) -> Option<ParExprMatch> {
        if !matches!(self.kind(start), Some(TokenKind::OpenParen)) {
            return None;
        }
        let mut end = start + 1;
        let label = if matches!(self.kind(end), Some(TokenKind::CloseParen)) {
            None
        } else if let Some(condition) = self.condition(end) {
            end = condition.end;
            let span = condition.span;
            trimmed_text(self.source, span.start, span.end)
        } else {
            None
        };
        if matches!(self.kind(end), Some(TokenKind::CloseParen)) {
            end += 1;
        }
        Some(ParExprMatch { end, label })
    }

    fn condition(&self, start: usize) -> Option<ExprMatch> {
        if self.is_identifier_name(start)
            && matches!(self.kind(start + 1), Some(TokenKind::Keyword(Keyword::In)))
            && self.is_identifier_name(start + 2)
        {
            let end = start + 3;
            return Some(ExprMatch {
                end,
                span: self.span(start, end),
            });
        }

        if let Some(expr) = self.expression(start)
            && matches!(self.kind(expr.end), Some(TokenKind::CloseParen))
        {
            return Some(expr);
        }

        let mut end = start;
        while self.is_text_word(end) {
            end += 1;
        }
        if end >= start + 2 {
            return Some(ExprMatch {
                end,
                span: self.span(start, end),
            });
        }
        self.expression(start)
    }

    fn can_start_statement(&self, start: usize) -> bool {
        match self.kind(start) {
            Some(TokenKind::Keyword(
                Keyword::If
                | Keyword::Try
                | Keyword::Par
                | Keyword::Opt
                | Keyword::Critical
                | Keyword::Section
                | Keyword::While
                | Keyword::Ref
                | Keyword::Return,
            ))
            | Some(TokenKind::ReturnAnnotation)
            | Some(TokenKind::Divider(_))
            | Some(TokenKind::OpenBrace) => true,
            _ => {
                self.creation_body(start).is_some()
                    || self.message_body(start).is_some()
                    || self.async_message(start).is_some()
                    || self.return_async_message(start).is_some()
            }
        }
    }

    fn message_candidate_is_complete(&self, end: usize) -> bool {
        !matches!(
            self.kind(end),
            Some(TokenKind::Colon | TokenKind::Arrow | TokenKind::ReturnArrow)
        ) && !matches!(self.kind(end), Some(TokenKind::Operator(value)) if value == "-")
    }

    fn is_atom(&self, index: usize) -> bool {
        matches!(
            self.kind(index),
            Some(
                TokenKind::Integer(_)
                    | TokenKind::Float(_)
                    | TokenKind::NumberUnit(_)
                    | TokenKind::Money(_)
                    | TokenKind::Identifier(_)
                    | TokenKind::DigitLeadingName(_)
                    | TokenKind::StringLiteral { .. }
                    | TokenKind::Keyword(Keyword::True | Keyword::False | Keyword::Nil)
            )
        )
    }

    fn is_identifier_name(&self, index: usize) -> bool {
        matches!(
            self.kind(index),
            Some(TokenKind::Identifier(_) | TokenKind::DigitLeadingName(_))
        )
    }

    fn is_text_word(&self, index: usize) -> bool {
        matches!(
            self.kind(index),
            Some(
                TokenKind::Identifier(_)
                    | TokenKind::DigitLeadingName(_)
                    | TokenKind::NumberUnit(_)
            )
        )
    }

    fn name(&self, index: usize) -> Option<SpannedText> {
        token_name(self.tokens.get(index)?)
    }

    fn kind(&self, index: usize) -> Option<&TokenKind> {
        self.tokens.get(index).map(|token| &token.kind)
    }

    fn is_eof(&self, index: usize) -> bool {
        matches!(self.kind(index), Some(TokenKind::Eof) | None)
    }

    fn span(&self, start: usize, end: usize) -> SourceSpan {
        let start_offset = self
            .tokens
            .get(start)
            .map_or(self.source.len(), |token| token.span.start);
        let end_offset = if end > start {
            self.tokens
                .get(end - 1)
                .map_or(start_offset, |token| token.span.end)
        } else {
            start_offset
        };
        SourceSpan::new(start_offset, end_offset)
    }

    fn token_end(&self, index: usize) -> usize {
        self.tokens
            .get(index)
            .map_or(self.source.len(), |token| token.span.end)
    }

    fn token_text(&self, index: usize) -> &str {
        self.tokens
            .get(index)
            .and_then(|token| self.source.get(token.span.start..token.span.end))
            .unwrap_or("")
    }

    fn compact(&self, start: usize, end: usize) -> String {
        (start..end).map(|index| self.token_text(index)).collect()
    }
}

/// Caches one bounded proof path through the flat ZenUML declaration prefix.
///
/// Participant declarations are not nested syntax, but their optional decorators make their
/// boundary ambiguous with the first statement. The old predictor recursed across every later
/// declaration on each head-loop iteration. This keeps at most one parser-depth-sized path and
/// extends it only after the parser has consumed that path. Dead `(token, depth)` states are
/// memoized within a fixed parser-depth-squared budget. The parser consumes each fixed-size plan
/// before extending it, so long flat declaration lists remain iterative and linear.
#[derive(Default)]
struct HeadParticipantPrediction {
    planned: VecDeque<PlannedHeadParticipant>,
}

#[derive(Clone)]
struct PlannedHeadParticipant {
    start: usize,
    candidate: ParticipantMatch,
}

impl HeadParticipantPrediction {
    fn select(&mut self, grammar: &Grammar<'_>, start: usize) -> Option<ParticipantMatch> {
        if self
            .planned
            .front()
            .is_some_and(|planned| planned.start == start)
        {
            return self.planned.pop_front().map(|planned| planned.candidate);
        }
        self.planned.clear();
        self.planned.extend(Self::plan(grammar, start));
        self.planned.pop_front().map(|planned| planned.candidate)
    }

    fn plan(grammar: &Grammar<'_>, start: usize) -> Vec<PlannedHeadParticipant> {
        let mut frames = Vec::with_capacity(MAX_DIAGRAM_NESTING_DEPTH + 1);
        let mut exhausted = HashSet::new();
        frames.push(HeadParticipantFrame::new(grammar, start));

        'search: loop {
            if grammar.control.is_cancelled() {
                return Vec::new();
            }
            let depth = frames.len().saturating_sub(1);
            // The first token can itself start a message, but head selection must first try its
            // participant candidates. Every later frame is a suffix of an already-selected
            // candidate and can use the normal statement boundary.
            let terminal = depth > 0 && grammar.head_suffix_terminal(frames[depth].start);
            if terminal || (depth == MAX_DIAGRAM_NESTING_DEPTH && frames[depth].has_continuation())
            {
                return frames
                    .into_iter()
                    .filter_map(|frame| {
                        frame.chosen.map(|candidate| PlannedHeadParticipant {
                            start: frame.start,
                            candidate,
                        })
                    })
                    .collect();
            }

            if depth < MAX_DIAGRAM_NESTING_DEPTH {
                while let Some(candidate) = frames
                    .last_mut()
                    .and_then(HeadParticipantFrame::next_candidate)
                {
                    if grammar.control.is_cancelled() {
                        return Vec::new();
                    }
                    if exhausted.contains(&(candidate.end, depth + 1)) {
                        continue;
                    }
                    frames[depth].chosen = Some(candidate.clone());
                    frames.push(HeadParticipantFrame::new(grammar, candidate.end));
                    continue 'search;
                }
            }

            if let Some(frame) = frames.pop() {
                if exhausted.len() == MAX_HEAD_PARTICIPANT_PREDICTION_STATES {
                    return Vec::new();
                }
                exhausted.insert((frame.start, depth));
            }
            if frames.is_empty() {
                return Vec::new();
            }
        }
    }
}

struct HeadParticipantFrame {
    start: usize,
    candidates: Vec<ParticipantMatch>,
    next_candidate: usize,
    chosen: Option<ParticipantMatch>,
}

impl HeadParticipantFrame {
    fn new(grammar: &Grammar<'_>, start: usize) -> Self {
        Self {
            start,
            candidates: grammar.participant_candidates(start),
            next_candidate: 0,
            chosen: None,
        }
    }

    fn next_candidate(&mut self) -> Option<ParticipantMatch> {
        while let Some(candidate) = self.candidates.get(self.next_candidate).cloned() {
            self.next_candidate += 1;
            if candidate.end > self.start {
                return Some(candidate);
            }
        }
        None
    }

    fn has_continuation(&self) -> bool {
        self.candidates
            .iter()
            .any(|candidate| candidate.end > self.start)
    }
}

#[derive(Debug, Clone)]
struct FromToMatch {
    end: usize,
    from: Option<EndpointMatch>,
    to: EndpointMatch,
}

#[derive(Debug, Clone)]
struct EmojiMatch {
    end: usize,
    name: SpannedText,
}

fn token_name(token: &Token) -> Option<SpannedText> {
    match &token.kind {
        TokenKind::Identifier(value)
        | TokenKind::DigitLeadingName(value)
        | TokenKind::StringLiteral { value, .. } => {
            Some(SpannedText::new(value.clone(), token.span))
        }
        _ => None,
    }
}

fn trimmed_text(source: &str, start: usize, end: usize) -> Option<SpannedText> {
    let raw = source.get(start..end)?;
    let trimmed_start = raw.len().saturating_sub(raw.trim_start().len());
    let trimmed_end = raw.trim_end().len();
    (trimmed_start <= trimmed_end).then(|| {
        SpannedText::new(
            raw[trimmed_start..trimmed_end].to_string(),
            SourceSpan::new(start + trimmed_start, start + trimmed_end),
        )
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse_source(source: &str) -> ParsedSyntax {
        let tokens = super::super::lexer::lex(source);
        parse(source, &tokens)
    }

    fn head_participants(document: &SyntaxDocument) -> Vec<&ParticipantSyntax> {
        document
            .head
            .iter()
            .filter_map(|item| match item {
                HeadItemSyntax::Participant(participant) => Some(participant.as_ref()),
                HeadItemSyntax::Group(_) => None,
            })
            .collect()
    }

    fn flat_head_participants(count: usize) -> String {
        let mut source = String::from("zenuml\n");
        for index in 0..count {
            source.push_str("@Actor P");
            source.push_str(&index.to_string());
            source.push(' ');
        }
        source.push_str("@Starter(P0) P0.m()");
        source
    }

    #[test]
    fn parses_nested_official_grammar_without_line_translation() {
        let parsed = parse_source(
            "zenuml\n@Actor Client #FFEBE6\n@Starter(Client)\nService.call(x) {\n  if(x != null) {\n    Worker.run(x)\n  }\n}\n",
        );
        assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
        assert_eq!(head_participants(&parsed.document).len(), 1);
        assert_eq!(parsed.document.statements.len(), 1);
    }

    #[test]
    fn header_prefix_matches_the_mermaid_adapter_replacement() {
        let parsed = parse_source("zenumlA.m()");
        assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
        assert_eq!(parsed.document.statements.len(), 1);
        assert!(matches!(
            &parsed.document.statements[0].kind,
            StatementKindSyntax::Message(message)
                if message.to.as_ref().map(|name| name.value.as_str()) == Some("A")
                    && message.signature.value == "m()"
        ));

        let upper_case = parse_source("ZenUMLA.m()");
        assert!(
            upper_case.diagnostics.is_empty(),
            "{:?}",
            upper_case.diagnostics
        );
    }

    #[test]
    fn mermaid_adapter_leaves_acc_title_to_zenuml_core() {
        let source = "zenuml\naccTitle: Accessible\n";
        let parsed = parse_source(source);

        assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
        assert!(parsed.document.head.is_empty());
        assert!(matches!(
            &parsed.document.statements[0].kind,
            StatementKindSyntax::Message(message)
                if message.style == MessageStyleSyntax::Asynchronous
                    && message.to.as_ref().map(|name| name.value.as_str()) == Some("accTitle")
                    && message.signature.value == "Accessible"
        ));
    }

    #[test]
    fn same_line_statements_are_delimited_by_grammar_rules() {
        for source in ["zenuml\nA.m() B.m()", "zenuml\nA.m();B.m()"] {
            let parsed = parse_source(source);
            assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
            assert_eq!(parsed.document.statements.len(), 2);
        }
    }

    #[test]
    fn head_prediction_rolls_back_before_message_receivers() {
        for source in ["zenuml\n@Actor A B.m()", "zenuml\nA B.m()"] {
            let parsed = parse_source(source);
            assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
            let participants = head_participants(&parsed.document);
            assert_eq!(participants.len(), 1);
            assert_eq!(participants[0].name.value, "A");
            assert_eq!(parsed.document.statements.len(), 1);
        }
    }

    #[test]
    fn head_prediction_limits_each_plan_and_streams_long_flat_declarations() {
        for count in [
            MAX_DIAGRAM_NESTING_DEPTH,
            MAX_DIAGRAM_NESTING_DEPTH + 1,
            MAX_DIAGRAM_NESTING_DEPTH * 2 + 1,
        ] {
            let source = flat_head_participants(count);
            let raw_tokens = super::super::lexer::lex(&source);
            let tokens = super::super::lexer::parser_tokens(&raw_tokens);
            let grammar = Grammar::new(&source, &tokens);
            let mut prediction = HeadParticipantPrediction::default();
            let mut start = 1;

            for index in 0..count {
                let participant = prediction
                    .select(&grammar, start)
                    .expect("flat declaration must remain selectable");
                let expected = format!("P{index}");
                assert_eq!(
                    participant.name.as_ref().map(|name| name.value.as_str()),
                    Some(expected.as_str())
                );
                assert!(prediction.planned.len() < MAX_DIAGRAM_NESTING_DEPTH);
                start = participant.end;
            }

            assert!(grammar.head_suffix_terminal(start));
            assert!(prediction.select(&grammar, start).is_none());
            let chunk_count = count.div_ceil(MAX_DIAGRAM_NESTING_DEPTH);
            assert!(
                grammar.participant_candidate_evaluations() <= count + chunk_count + 1,
                "head prediction must evaluate declarations linearly with one lookahead per chunk"
            );
        }
    }

    #[test]
    fn head_prediction_avoids_stack_growth_on_long_flat_declarations() {
        let count = MAX_DIAGRAM_NESTING_DEPTH * 16;
        let source = flat_head_participants(count);
        let handle = std::thread::Builder::new()
            .name("zenuml-head-prediction-small-stack".to_string())
            .stack_size(128 * 1024)
            .spawn(move || {
                let parsed = parse_source(&source);
                assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
                assert_eq!(head_participants(&parsed.document).len(), count);
                assert!(parsed.document.starter.is_some());
                assert_eq!(parsed.document.statements.len(), 1);
            })
            .expect("spawn small-stack ZenUML parser thread");
        handle
            .join()
            .expect("long flat ZenUML declaration parse must not overflow the stack");
    }

    fn nested_expression_cases(depth: usize) -> [String; 4] {
        [
            format!("{}x", "!".repeat(depth)),
            format!("{}x{}", "(".repeat(depth), ")".repeat(depth)),
            format!("{}x", "value=".repeat(depth)),
            format!("{}x{}", "F(".repeat(depth), ")".repeat(depth)),
        ]
    }

    fn assert_expression_depth_is_bounded(source: String) {
        let raw_tokens = super::super::lexer::lex(&source);
        let tokens = super::super::lexer::parser_tokens(&raw_tokens);
        let grammar = Grammar::new(&source, &tokens);
        assert!(grammar.expression(0).is_none());
        assert!(grammar.expression_depth_exceeded(), "{source}");
    }

    #[test]
    fn expression_grammar_bounds_every_recursive_form() {
        let excessive = MAX_DIAGRAM_NESTING_DEPTH + 1;
        let cases = nested_expression_cases(excessive);

        let handle = std::thread::Builder::new()
            .name("zenuml-expression-depth-small-stack".to_string())
            .stack_size(512 * 1024)
            .spawn(move || {
                for source in cases {
                    assert_expression_depth_is_bounded(source);
                }
            })
            .expect("spawn small-stack ZenUML expression parser thread");
        handle
            .join()
            .expect("bounded ZenUML expressions must not overflow the stack");
    }

    #[test]
    fn expression_depth_boundary_is_accepted_and_excess_is_a_parse_diagnostic() {
        for expression in nested_expression_cases(MAX_DIAGRAM_NESTING_DEPTH) {
            let source = format!("zenuml\nreturn {expression}");
            let parsed = parse_source(&source);
            assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
            assert_eq!(parsed.document.statements.len(), 1, "{source}");
        }

        for expression in nested_expression_cases(MAX_DIAGRAM_NESTING_DEPTH + 1) {
            let source = format!("zenuml\nreturn {expression}\nAfter.call()");
            let parsed = parse_source(&source);
            assert!(
                parsed.diagnostics.iter().any(|diagnostic| diagnostic
                    .message
                    .contains("ZenUML expression nesting depth exceeds")),
                "{source}: {:?}",
                parsed.diagnostics
            );
            assert!(
                parsed.document.statements.iter().any(|statement| matches!(
                    &statement.kind,
                    StatementKindSyntax::Message(message) if message.signature.value == "call()"
                )),
                "parser did not recover after the bounded expression: {source}"
            );
        }
    }

    #[test]
    fn decorator_only_participants_keep_the_upstream_synthetic_name() {
        for source in ["zenuml\n@Actor\nA.m()", "zenuml\n<<Service>>\nA.m()"] {
            let parsed = parse_source(source);
            assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
            let participants = head_participants(&parsed.document);
            assert_eq!(participants.len(), 1);
            assert_eq!(participants[0].name.value, MISSING_PARTICIPANT);
            assert_eq!(parsed.document.statements.len(), 1);
        }
    }

    #[test]
    fn participant_prediction_composes_type_stereotype_and_emoji() {
        let parsed = parse_source("zenuml\n@Actor <<Boundary>> [rocket] A A.m()");
        assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
        let participants = head_participants(&parsed.document);
        assert_eq!(participants.len(), 1);
        let participant = participants[0];
        assert_eq!(participant.name.value, "A");
        assert_eq!(
            participant
                .participant_type
                .as_ref()
                .map(|value| value.value.as_str()),
            Some("Actor")
        );
        assert_eq!(
            participant
                .stereotype
                .as_ref()
                .map(|value| value.value.as_str()),
            Some("Boundary")
        );
        assert_eq!(
            participant.emoji.as_ref().map(|value| value.value.as_str()),
            Some("rocket")
        );
        assert_eq!(parsed.document.statements.len(), 1);
    }

    #[test]
    fn optional_if_block_does_not_capture_the_following_statement() {
        let optional = parse_source("zenuml\nif(x) A.m()");
        let StatementKindSyntax::Fragment(fragment) = &optional.document.statements[0].kind else {
            panic!("expected alternative");
        };
        assert!(fragment.sections[0].statements.is_empty());
        assert_eq!(optional.document.statements.len(), 2);

        let braced = parse_source("zenuml\nif(x){A.m() B.m()}");
        let StatementKindSyntax::Fragment(fragment) = &braced.document.statements[0].kind else {
            panic!("expected alternative");
        };
        assert_eq!(fragment.sections[0].statements.len(), 2);
    }

    #[test]
    fn parameter_and_condition_ast_nodes_retain_exact_spans() {
        let source = concat!(
            "zenuml\n",
            "A.m(x=1, Type value, B.call(), 10ms) ",
            "if(item in items){A.m()} ",
            "if(status pending 10ms){A.m()}",
        );
        let parsed = parse_source(source);
        assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
        let StatementKindSyntax::Message(message) = &parsed.document.statements[0].kind else {
            panic!("expected message");
        };
        assert_eq!(message.signature.value, "m(x=1,Type value,B.call(),10ms)");
        assert_eq!(
            &source[message.signature.span.start..message.signature.span.end],
            "m(x=1, Type value, B.call(), 10ms)"
        );
        for (statement, expected) in parsed.document.statements[1..]
            .iter()
            .zip(["item in items", "status pending 10ms"])
        {
            let StatementKindSyntax::Fragment(fragment) = &statement.kind else {
                panic!("expected fragment");
            };
            let label = fragment.label.as_ref().expect("condition label");
            assert_eq!(label.value, expected);
            assert_eq!(&source[label.span.start..label.span.end], expected);
        }
    }

    #[test]
    fn title_and_event_modes_have_independent_grammar_boundaries() {
        let parsed = parse_source("zenuml\ntitle Order Service\nA: ready\nB.m()");
        assert_eq!(
            parsed
                .document
                .title
                .as_ref()
                .map(|title| title.value.as_str()),
            Some("Order Service")
        );
        assert_eq!(parsed.document.statements.len(), 2);
        assert_eq!(
            parsed.document.title.as_ref().unwrap().value,
            "Order Service"
        );
        assert!(matches!(
            parsed.document.statements[0].kind,
            StatementKindSyntax::Message(MessageSyntax {
                style: MessageStyleSyntax::Asynchronous,
                ..
            })
        ));

        let method = parse_source("zenuml\ntitle.m()");
        assert!(method.document.title.is_none());
        assert_eq!(method.document.statements.len(), 1);
    }

    #[test]
    fn empty_starter_parentheses_are_valid_and_remain_name_optional() {
        let parsed = parse_source("zenuml\n@Starter() A.m()");
        assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
        let starter = parsed.document.starter.as_ref().expect("starter syntax");
        assert!(starter.name.is_none());
        assert_eq!(parsed.document.statements.len(), 1);
    }

    #[test]
    fn message_context_accepts_dotted_func_without_invocation() {
        let parsed = parse_source("zenuml\n@Starter(S) A.B");
        assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
        assert_eq!(parsed.document.statements.len(), 1);
        let StatementKindSyntax::Message(message) = &parsed.document.statements[0].kind else {
            panic!("expected message");
        };
        assert_eq!(
            message
                .to
                .as_ref()
                .map(|participant| participant.value.as_str()),
            Some("A")
        );
        assert_eq!(message.signature.value, "B");
    }

    #[test]
    fn missing_required_fragment_braces_recover_without_changing_optional_rules() {
        for source in ["zenuml\nif(x) A.m()", "zenuml\nwhile(x) A.m()"] {
            let parsed = parse_source(source);
            assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
            let StatementKindSyntax::Fragment(fragment) = &parsed.document.statements[0].kind
            else {
                panic!("expected fragment");
            };
            assert!(fragment.sections[0].statements.is_empty());
            assert_eq!(parsed.document.statements.len(), 2);
        }

        for source in [
            "zenuml\npar(x) A.m()",
            "zenuml\nopt(x) A.m()",
            "zenuml\ncritical(x) A.m()",
            "zenuml\nsection(x) A.m()",
        ] {
            let parsed = parse_source(source);
            assert!(!parsed.diagnostics.is_empty(), "{source}");
            let StatementKindSyntax::Fragment(fragment) = &parsed.document.statements[0].kind
            else {
                panic!("expected fragment: {source}");
            };
            assert_eq!(fragment.sections[0].statements.len(), 1, "{source}");
            assert_eq!(parsed.document.statements.len(), 1, "{source}");
        }
    }

    #[test]
    fn recovers_after_invalid_token_without_line_synchronization() {
        let parsed = parse_source("zenuml\n@Starter(A) A.call() ? B.call()");
        assert!(!parsed.diagnostics.is_empty());
        assert_eq!(parsed.document.statements.len(), 2);
    }

    #[test]
    fn comment_channel_preserves_text_and_binds_to_the_next_default_token() {
        let parsed = parse_source(concat!(
            "zenuml\n",
            "// first comment \n",
            "// second comment  \n",
            "const A.m() ",
            "// next message \n",
            "B.m()",
        ));
        assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
        assert_eq!(parsed.document.statements.len(), 2);
        assert_eq!(
            parsed.document.statements[0]
                .comment
                .as_ref()
                .map(|comment| comment.value.as_str()),
            Some(" first comment \n second comment  ")
        );
        assert_eq!(
            parsed.document.statements[1]
                .comment
                .as_ref()
                .map(|comment| comment.value.as_str()),
            Some(" next message ")
        );
    }

    #[test]
    fn block_close_comment_is_owned_by_the_brace_block() {
        let parsed = parse_source("zenuml\nA.m() { internal()\n// block close \n}");
        assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
        let StatementKindSyntax::Message(message) = &parsed.document.statements[0].kind else {
            panic!("expected message");
        };
        assert_eq!(message.body.len(), 1);
        assert_eq!(
            message
                .body_comment
                .as_ref()
                .map(|comment| comment.value.as_str()),
            Some(" block close ")
        );
    }
}
