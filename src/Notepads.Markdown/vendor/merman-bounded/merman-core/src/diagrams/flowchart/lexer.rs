// Notepads Markdown modifications: Copyright (c) 2026, Jiaqi (0x7c13) Liu.
// Original upstream MIT/Apache-2.0 licenses and notices are preserved.
use super::{
    ArrowToken, ClickAction, ClickStmt, DirectionStatementToken, LabeledText, LexError, LinkToken,
    NodeLabelToken, SubgraphHeader, TitleKind, Tok,
    ast::{FlowchartClickEditorEvidence, FlowchartDirectiveEditorEvidence},
    destruct_end_link, destruct_labeled_end_link, destruct_start_link, is_ecmascript_trim_char,
    lex, parse_label_text,
};
use crate::diagrams::jison_unicode::is_mermaid_unicode_text;
use crate::{
    EditorExpectedSyntax, EditorExpectedSyntaxKind, SourceSpan, editor::source_value_span,
};

fn directive_argument_spans(
    rest: &str,
    rest_start: usize,
) -> (Option<SourceSpan>, Option<SourceSpan>) {
    let leading = rest
        .char_indices()
        .take_while(|(_, ch)| is_ecmascript_trim_char(*ch))
        .map(|(index, ch)| index + ch.len_utf8())
        .last()
        .unwrap_or(0);
    let body = &rest[leading..];
    if body.is_empty() {
        return (None, None);
    }

    let first_len = body
        .char_indices()
        .find(|(_, ch)| is_ecmascript_trim_char(*ch))
        .map_or(body.len(), |(index, _)| index);
    let first_start = rest_start + leading;
    let first_end = first_start + first_len;
    let first = SourceSpan::new(first_start, first_end);
    let remainder = &body[first_len..];
    if remainder.is_empty() {
        return (Some(first), None);
    }

    let remainder_leading = remainder
        .char_indices()
        .take_while(|(_, ch)| is_ecmascript_trim_char(*ch))
        .map(|(index, ch)| index + ch.len_utf8())
        .last()
        .unwrap_or(0);
    let value_start = first_end + remainder_leading;
    let value_end = rest_start + rest.len();
    (Some(first), Some(SourceSpan::new(value_start, value_end)))
}

fn directive_error<I>(
    mut error: LexError,
    prefix: &'static str,
    statement_span: SourceSpan,
    expected_syntax: I,
) -> LexError
where
    I: IntoIterator<Item = (EditorExpectedSyntaxKind, SourceSpan)>,
{
    for (kind, span) in expected_syntax {
        error = error.expecting(kind, span);
    }
    error.in_directive(prefix, statement_span)
}

fn active_following_span(
    rest: &str,
    rest_start: usize,
    following: Option<SourceSpan>,
) -> Option<SourceSpan> {
    let following = following?;
    let local_start = following.start.checked_sub(rest_start)?;
    let raw = rest.get(local_start..)?;
    let trailing = raw
        .char_indices()
        .rev()
        .take_while(|(_, ch)| is_ecmascript_trim_char(*ch))
        .map(|(index, _)| raw.len() - index)
        .last()
        .unwrap_or(0);
    if raw.is_empty() || trailing > 0 {
        return Some(SourceSpan::new(following.end, following.end));
    }
    Some(following)
}

fn trailing_value_slot(
    rest: &str,
    rest_start: usize,
    following: Option<SourceSpan>,
) -> Option<SourceSpan> {
    let active = active_following_span(rest, rest_start, following)?;
    (active.start == active.end).then_some(active)
}

fn directive_editor_evidence(
    statement_span: SourceSpan,
    first: Option<(EditorExpectedSyntaxKind, SourceSpan)>,
    following: Option<(EditorExpectedSyntaxKind, SourceSpan)>,
) -> FlowchartDirectiveEditorEvidence {
    FlowchartDirectiveEditorEvidence::new(
        EditorExpectedSyntax::new(EditorExpectedSyntaxKind::Directive, statement_span),
        first.map(|(kind, span)| EditorExpectedSyntax::new(kind, span)),
        following.map(|(kind, span)| EditorExpectedSyntax::new(kind, span)),
    )
}

fn skip_ecmascript_whitespace(input: &str, mut pos: usize) -> usize {
    debug_assert!(input.is_char_boundary(pos));
    while pos < input.len() {
        let ch = input[pos..]
            .chars()
            .next()
            .expect("position before input end must contain a character");
        if !is_ecmascript_trim_char(ch) {
            break;
        }
        pos += ch.len_utf8();
    }
    pos
}

fn find_pipe_label_end(input: &str, mut pos: usize) -> Option<usize> {
    #[derive(Clone, Copy)]
    enum State {
        Text,
        String,
        MarkdownString,
    }

    let mut state = State::Text;
    while pos < input.len() {
        let rest = &input[pos..];
        match state {
            State::Text => {
                if rest.starts_with('|') {
                    return Some(pos);
                }
                if rest.starts_with("\"`") {
                    state = State::MarkdownString;
                    pos += 2;
                    continue;
                }
                if rest.starts_with('"') {
                    state = State::String;
                    pos += 1;
                    continue;
                }
            }
            State::String => {
                if rest.starts_with('"') {
                    state = State::Text;
                    pos += 1;
                    continue;
                }
            }
            State::MarkdownString => {
                if rest.starts_with("`\"") {
                    state = State::Text;
                    pos += 2;
                    continue;
                }
            }
        }

        let ch = rest
            .chars()
            .next()
            .expect("pipe label scan position must contain a character");
        pos += ch.len_utf8();
    }
    None
}

fn non_ascii_id_char_len(input: &str, pos: usize) -> Option<usize> {
    if !input.is_char_boundary(pos) {
        return None;
    }
    let ch = input[pos..].chars().next()?;
    (!ch.is_ascii() && is_mermaid_unicode_text(ch)).then(|| ch.len_utf8())
}

pub(super) struct Lexer<'input> {
    pub(super) input: &'input str,
    pub(super) pos: usize,
    // Every producer returns immediately after queuing exactly one follow-up.
    pub(super) pending: Option<std::result::Result<(usize, Tok, usize), LexError>>,
    pub(super) allow_header_direction: bool,
    pub(super) recover_partial_node_labels: bool,
    pub(super) region: Option<markdown_memory::RegionHandle>,
    pub(super) native_subgraph_depth: usize,
}

impl<'input> Lexer<'input> {
    pub(super) fn normalize_direction_token(dir: &str) -> &str {
        if dir == "TD" { "TB" } else { dir }
    }

    pub(super) fn new(input: &'input str) -> Self {
        Self {
            input,
            pos: 0,
            pending: None,
            allow_header_direction: false,
            recover_partial_node_labels: false,
            region: None,
            native_subgraph_depth: 0,
        }
    }

    pub(super) fn controlled(input: &'input str, control: &crate::OperationControl) -> Self {
        Self {
            region: control.memory_region().cloned(),
            ..Self::new(input)
        }
    }

    fn owned(&self, text: &str) -> std::result::Result<String, LexError> {
        lex::owned_text(self.region.as_ref(), text)
    }

    pub(super) fn native_limit(&self) -> LexError {
        if let Some(region) = &self.region {
            region
                .scope()
                .record_failure(markdown_memory::MemoryError::BudgetLimited);
        }
        LexError::terminal()
    }

    fn push_text(&self, output: &mut String, text: &str) -> std::result::Result<(), LexError> {
        match self.region.as_ref() {
            Some(region) => region
                .try_push_str(output, text, markdown_memory::Family::Parse)
                .map_err(|_| LexError::terminal()),
            None => {
                output.push_str(text);
                Ok(())
            }
        }
    }

    fn error_span(&self, message: impl AsRef<str>, span: SourceSpan) -> LexError {
        if self.region.is_some() {
            LexError::terminal().at_span(span)
        } else {
            LexError::with_span(message.as_ref(), span)
        }
    }

    #[cfg(any(feature = "diagram-flowchart", feature = "diagram-swimlane"))]
    pub(super) fn recovering(input: &'input str) -> Self {
        Self {
            recover_partial_node_labels: true,
            ..Self::new(input)
        }
    }

    pub(super) fn peek(&self) -> Option<u8> {
        self.input.as_bytes().get(self.pos).copied()
    }

    pub(super) fn peek2(&self) -> Option<[u8; 2]> {
        if self.pos + 1 >= self.input.len() {
            return None;
        }
        Some([
            self.input.as_bytes()[self.pos],
            self.input.as_bytes()[self.pos + 1],
        ])
    }

    pub(super) fn starts_with_kw(&self, kw: &str) -> bool {
        let rest = &self.input[self.pos..];
        if !rest.starts_with(kw) {
            return false;
        }
        let after = self.pos + kw.len();
        if after >= self.input.len() {
            return true;
        }
        let b = self.input.as_bytes()[after];
        !b.is_ascii_alphanumeric() && b != b'_' && b != b'-'
    }

    pub(super) fn skip_ws(&mut self) {
        while let Some(ch) = self.input[self.pos..].chars().next() {
            if !is_ecmascript_trim_char(ch) || ch == '\n' {
                break;
            }
            self.pos += ch.len_utf8();
        }
    }

    pub(super) fn lex_sep(&mut self) -> Option<(usize, Tok, usize)> {
        let start = self.pos;
        match self.peek()? {
            b'\n' => {
                let bytes = self.input.as_bytes();
                let mut look = self.pos + 1;
                while let Some(ch) = self.input[look..].chars().next() {
                    if ch == '\n' || !is_ecmascript_trim_char(ch) {
                        break;
                    }
                    look += ch.len_utf8();
                }
                if look < bytes.len() {
                    let is_linkish = match bytes[look] {
                        b'~' => {
                            look + 2 < bytes.len()
                                && bytes[look + 1] == b'~'
                                && bytes[look + 2] == b'~'
                        }
                        b'=' => look + 1 < bytes.len() && bytes[look + 1] == b'=',
                        b'-' => {
                            look + 1 < bytes.len()
                                && (bytes[look + 1] == b'-' || bytes[look + 1] == b'.')
                        }
                        b'o' | b'x' | b'<' => {
                            look + 2 < bytes.len()
                                && ((bytes[look + 1] == b'-'
                                    && (bytes[look + 2] == b'-' || bytes[look + 2] == b'.'))
                                    || (bytes[look + 1] == b'=' && bytes[look + 2] == b'='))
                        }
                        _ => false,
                    };
                    if is_linkish {
                        self.pos = look;
                        return None;
                    }
                }

                self.pos += 1;
                Some((start, Tok::Sep, self.pos))
            }
            b';' => {
                self.pos += 1;
                Some((start, Tok::Sep, self.pos))
            }
            _ => None,
        }
    }

    pub(super) fn lex_comment(&mut self) -> Option<(usize, Tok, usize)> {
        let start = self.pos;
        let Some([b'%', b'%']) = self.peek2() else {
            return None;
        };
        // Consume until newline or EOF. If newline exists, emit Sep to keep statement boundaries.
        self.pos += 2;
        while let Some(b) = self.peek() {
            if b == b'\n' {
                self.pos += 1;
                return Some((start, Tok::Sep, self.pos));
            }
            self.pos += 1;
        }
        None
    }

    pub(super) fn lex_direction(
        &mut self,
    ) -> Option<std::result::Result<(usize, Tok, usize), LexError>> {
        if !self.allow_header_direction {
            return None;
        }
        let start = self.pos;
        let rest = &self.input[self.pos..];
        for d in ["TB", "TD", "BT", "LR", "RL"] {
            if rest.starts_with(d) {
                let after = self.pos + d.len();
                if after < self.input.len() {
                    let b = self.input.as_bytes()[after];
                    if b.is_ascii_alphanumeric() || b == b'_' || b == b'-' {
                        continue;
                    }
                }
                self.pos = after;
                self.allow_header_direction = false;
                let d = Self::normalize_direction_token(d);
                return Some(self.owned(d).map(|d| (start, Tok::Direction(d), self.pos)));
            }
        }

        if let Some(&b) = rest.as_bytes().first() {
            let mapped = match b {
                b'>' => Some("LR"),
                b'<' => Some("RL"),
                b'^' => Some("BT"),
                b'v' => Some("TB"),
                _ => None,
            };
            if let Some(d) = mapped {
                let after = self.pos + 1;
                if after < self.input.len() {
                    let next = self.input.as_bytes()[after];
                    if next.is_ascii_alphanumeric() || next == b'_' || next == b'-' {
                        return None;
                    }
                }
                self.pos = after;
                self.allow_header_direction = false;
                return Some(self.owned(d).map(|d| (start, Tok::Direction(d), self.pos)));
            }
        }

        None
    }

    pub(super) fn lex_direction_stmt(
        &mut self,
    ) -> Option<std::result::Result<(usize, Tok, usize), LexError>> {
        let start = self.pos;
        if !self.starts_with_kw("direction") {
            return None;
        }
        self.pos += "direction".len();
        self.skip_ws();

        let direction_start = self.pos;
        while let Some(ch) = self.input[self.pos..].chars().next() {
            if is_ecmascript_trim_char(ch) || ch == ';' {
                break;
            }
            self.pos += ch.len_utf8();
        }
        let direction_end = self.pos;
        while let Some(b) = self.peek() {
            if b == b'\n' || b == b';' {
                break;
            }
            self.pos += 1;
        }
        let statement_end = self.pos;
        let direction = &self.input[direction_start..direction_end];
        let selection = SourceSpan::new(direction_start, direction_end);

        let Some(dir) = ["TB", "TD", "BT", "LR", "RL"]
            .into_iter()
            .find(|candidate| *candidate == direction)
        else {
            let error = self
                .error_span("invalid flowchart direction", selection)
                .expecting(
                    crate::EditorExpectedSyntaxKind::FlowchartDirectionValue,
                    selection,
                );
            if self.recover_partial_node_labels {
                return Some(Ok((
                    start,
                    Tok::DirectionStmt(DirectionStatementToken {
                        direction: String::new(),
                        selection,
                        recovery_error: Some(error),
                    }),
                    statement_end,
                )));
            }
            return Some(Err(error));
        };

        let direction = match self.owned(dir) {
            Ok(value) => value,
            Err(error) => return Some(Err(error)),
        };
        Some(Ok((
            start,
            Tok::DirectionStmt(DirectionStatementToken {
                direction,
                selection,
                recovery_error: None,
            }),
            statement_end,
        )))
    }

    pub(super) fn capture_to_stmt_end(&mut self) -> (usize, &'input str, usize) {
        let start = self.pos;
        let mut in_double_quote = false;
        let mut in_single_quote = false;
        let mut escaped = false;
        while self.pos < self.input.len() {
            let b = self.input.as_bytes()[self.pos];
            if in_double_quote {
                if escaped {
                    escaped = false;
                } else if b == b'\\' {
                    escaped = true;
                } else if b == b'"' {
                    in_double_quote = false;
                }
                self.pos += 1;
                continue;
            }
            if in_single_quote {
                if escaped {
                    escaped = false;
                } else if b == b'\\' {
                    escaped = true;
                } else if b == b'\'' {
                    in_single_quote = false;
                }
                self.pos += 1;
                continue;
            }

            if b == b'"' {
                in_double_quote = true;
                self.pos += 1;
                continue;
            }
            if b == b'\'' {
                in_single_quote = true;
                self.pos += 1;
                continue;
            }

            if b == b'\n' || b == b';' {
                break;
            }
            self.pos += 1;
        }
        (start, &self.input[start..self.pos], self.pos)
    }

    pub(super) fn capture_to_stmt_end_from(&mut self, start: usize) -> (usize, &'input str, usize) {
        self.pos = start;
        self.capture_to_stmt_end()
    }

    pub(super) fn capture_recovery_to_stmt_end_from(
        &mut self,
        start: usize,
    ) -> (usize, &'input str, usize) {
        self.pos = start;
        while self.pos < self.input.len() {
            match self.input.as_bytes()[self.pos] {
                b'\n' | b';' => break,
                _ => self.pos += 1,
            }
        }
        (start, &self.input[start..self.pos], self.pos)
    }

    pub(super) fn lex_style_sep(&mut self) -> Option<(usize, Tok, usize)> {
        let start = self.pos;
        if self.input[self.pos..].starts_with(":::") {
            self.pos += 3;
            return Some((start, Tok::StyleSep, self.pos));
        }
        None
    }

    pub(super) fn lex_shape_data(
        &mut self,
    ) -> Option<std::result::Result<(usize, Tok, usize), LexError>> {
        let start = self.pos;
        if !self.input[self.pos..].starts_with("@{") {
            return None;
        }
        if self.region.is_some() {
            return Some(self.lex_shape_data_native());
        }
        self.pos += 2;

        // Mermaid's Jison lexer has dedicated states for shapeData strings:
        // - it allows `}` inside double-quoted strings
        // - it rewrites `\n\s*` inside double-quoted strings to `<br/>`
        //
        // We mimic that behavior here while returning a single `ShapeData` token.
        let bytes = self.input.as_bytes();
        let mut out = String::new();
        let mut segment_start = self.pos;
        let mut in_string = false;

        while self.pos < self.input.len() {
            let b = bytes[self.pos];
            if !in_string {
                if b == b'"' {
                    if let Err(error) =
                        self.push_text(&mut out, &self.input[segment_start..self.pos + 1])
                    {
                        return Some(Err(error));
                    }
                    self.pos += 1;
                    segment_start = self.pos;
                    in_string = true;
                    continue;
                }
                if b == b'}' {
                    if let Err(error) =
                        self.push_text(&mut out, &self.input[segment_start..self.pos])
                    {
                        return Some(Err(error));
                    }
                    self.pos += 1;
                    return Some(Ok((start, Tok::ShapeData(out), self.pos)));
                }
                self.pos += 1;
                continue;
            }

            if b == b'"' {
                if let Err(error) =
                    self.push_text(&mut out, &self.input[segment_start..self.pos + 1])
                {
                    return Some(Err(error));
                }
                self.pos += 1;
                segment_start = self.pos;
                in_string = false;
                continue;
            }

            if b == b'\n' {
                if let Err(error) = self.push_text(&mut out, &self.input[segment_start..self.pos]) {
                    return Some(Err(error));
                }
                if let Err(error) = self.push_text(&mut out, "<br/>") {
                    return Some(Err(error));
                }
                self.pos += 1;
                while self.pos < self.input.len() {
                    match bytes[self.pos] {
                        b' ' | b'\t' | b'\r' => self.pos += 1,
                        _ => break,
                    }
                }
                segment_start = self.pos;
                continue;
            }

            self.pos += 1;
        }

        if let Err(error) = self.push_text(&mut out, &self.input[segment_start..self.pos]) {
            return Some(Err(error));
        }
        let span = SourceSpan::new(start, self.pos);
        let expected = super::shape_value_expected_span(self.input, start, self.pos)
            .unwrap_or(SourceSpan::new(self.pos, self.pos));
        Some(Err(self
            .error_span("Unterminated shape data (missing `}`)", span)
            .expecting(
                crate::EditorExpectedSyntaxKind::ShapeValue,
                expected,
            )))
    }

    fn lex_shape_data_native(&mut self) -> std::result::Result<(usize, Tok, usize), LexError> {
        let start = self.pos;
        self.pos += 2;
        let mut segment = self.pos;
        let mut depth = 1usize;
        let mut quote = None;
        let mut output = String::new();
        while self.pos < self.input.len() {
            let byte = self.input.as_bytes()[self.pos];
            if let Some(active_quote) = quote {
                if active_quote == b'"' && byte == b'\\' {
                    self.pos += 1;
                    if self.pos < self.input.len() {
                        let escaped = self.input[self.pos..]
                            .chars()
                            .next()
                            .ok_or_else(LexError::terminal)?;
                        self.pos += escaped.len_utf8();
                        if escaped == '\r' && self.input.as_bytes().get(self.pos) == Some(&b'\n') {
                            self.pos += 1;
                        }
                    }
                    continue;
                }
                if byte == active_quote {
                    if active_quote == b'\''
                        && self.input.as_bytes().get(self.pos + 1) == Some(&b'\'')
                    {
                        self.pos += 2;
                        continue;
                    }
                    quote = None;
                    self.pos += 1;
                    continue;
                }
                if active_quote == b'"' && byte == b'\n' {
                    self.push_text(&mut output, &self.input[segment..self.pos])?;
                    self.push_text(&mut output, "<br/>")?;
                    self.pos += 1;
                    while matches!(
                        self.input.as_bytes().get(self.pos),
                        Some(b' ' | b'\t' | b'\r')
                    ) {
                        self.pos += 1;
                    }
                    segment = self.pos;
                    continue;
                }
                let ch = self.input[self.pos..]
                    .chars()
                    .next()
                    .ok_or_else(LexError::terminal)?;
                self.pos += ch.len_utf8();
                continue;
            }
            match byte {
                b'"' | b'\'' => {
                    quote = Some(byte);
                    self.pos += 1;
                }
                b'{' => {
                    depth += 1;
                    self.pos += 1;
                }
                b'}' => {
                    depth -= 1;
                    if depth == 0 {
                        self.push_text(&mut output, &self.input[segment..self.pos])?;
                        self.pos += 1;
                        return Ok((start, Tok::ShapeData(output), self.pos));
                    }
                    self.pos += 1;
                }
                _ => {
                    let ch = self.input[self.pos..]
                        .chars()
                        .next()
                        .ok_or_else(LexError::terminal)?;
                    self.pos += ch.len_utf8();
                }
            }
        }
        Err(self.error_span("Unterminated shape data", SourceSpan::new(start, self.pos)))
    }

    pub(super) fn lex_edge_id(
        &mut self,
    ) -> Option<std::result::Result<(usize, Tok, usize), LexError>> {
        let start = self.pos;
        if start >= self.input.len() {
            return None;
        }

        // Mermaid's `[^\s"]+@(?=[^\{"])` LINK_ID rule runs before ordinary identifiers and
        // greedily captures the last eligible `@` in a non-whitespace run.
        let mut cursor = start;
        let mut at = None;
        while cursor < self.input.len() {
            let ch = self.input[cursor..]
                .chars()
                .next()
                .expect("edge id scan position must contain a character");
            if is_ecmascript_trim_char(ch) || ch == '"' {
                break;
            }
            if ch == '@' {
                let next = self.input[cursor + 1..].chars().next();
                if cursor > start && matches!(next, Some(next) if next != '{' && next != '"') {
                    at = Some(cursor);
                }
            }
            cursor += ch.len_utf8();
        }

        let at = at?;
        if self.region.is_some() && at - start > 128 {
            return Some(Err(self.native_limit()));
        }
        self.pos = at + 1;
        Some(
            self.owned(&self.input[start..at])
                .map(|id| (start, Tok::EdgeId(id), self.pos)),
        )
    }

    pub(super) fn lex_style_stmt(
        &mut self,
    ) -> Option<std::result::Result<(usize, Tok, usize), LexError>> {
        let start = self.pos;
        if !self.starts_with_kw("style") {
            return None;
        }
        self.pos += "style".len();
        self.skip_ws();
        let (rest_start, rest, end) = self.capture_to_stmt_end();
        let statement_span = SourceSpan::new(start, end);
        let (target, style) = directive_argument_spans(&rest, rest_start);
        let style_slot = trailing_value_slot(&rest, rest_start, style);
        match lex::parse_style_stmt_controlled(&rest, self.region.as_ref()) {
            Ok(mut stmt) => {
                if let Err(error) =
                    lex::attach_style_stmt_spans(&mut stmt, &rest, rest_start, self.region.as_ref())
                {
                    return Some(Err(error));
                }
                stmt.editor_evidence = directive_editor_evidence(
                    statement_span,
                    target.map(|span| (EditorExpectedSyntaxKind::NodeIdentifier, span)),
                    style_slot.map(|span| (EditorExpectedSyntaxKind::StyleValue, span)),
                );
                Some(Ok((start, Tok::StyleStmt(stmt), end)))
            }
            Err(error) => Some(Err(directive_error(
                error,
                "style",
                statement_span,
                target
                    .map(|span| (EditorExpectedSyntaxKind::NodeIdentifier, span))
                    .into_iter()
                    .chain((target.is_none()).then_some((
                        EditorExpectedSyntaxKind::NodeIdentifier,
                        SourceSpan::new(end, end),
                    ))),
            ))),
        }
    }

    pub(super) fn lex_classdef_stmt(
        &mut self,
    ) -> Option<std::result::Result<(usize, Tok, usize), LexError>> {
        let start = self.pos;
        if !self.starts_with_kw("classDef") {
            return None;
        }
        self.pos += "classDef".len();
        self.skip_ws();
        let (rest_start, rest, end) = self.capture_to_stmt_end();
        let statement_span = SourceSpan::new(start, end);
        let (class_name, style) = directive_argument_spans(&rest, rest_start);
        let style_slot = trailing_value_slot(&rest, rest_start, style);
        match lex::parse_classdef_stmt_controlled(&rest, self.region.as_ref()) {
            Ok(mut stmt) => {
                if let Err(error) = lex::attach_classdef_stmt_spans(
                    &mut stmt,
                    &rest,
                    rest_start,
                    self.region.as_ref(),
                ) {
                    return Some(Err(error));
                }
                stmt.editor_evidence = directive_editor_evidence(
                    statement_span,
                    class_name.map(|span| (EditorExpectedSyntaxKind::ClassName, span)),
                    style_slot.map(|span| (EditorExpectedSyntaxKind::StyleValue, span)),
                );
                Some(Ok((start, Tok::ClassDefStmt(stmt), end)))
            }
            Err(error) => Some(Err(directive_error(
                error,
                "classDef",
                statement_span,
                class_name.map(|span| (EditorExpectedSyntaxKind::ClassName, span)),
            ))),
        }
    }

    pub(super) fn lex_class_assign_stmt(
        &mut self,
    ) -> Option<std::result::Result<(usize, Tok, usize), LexError>> {
        let start = self.pos;
        if !self.starts_with_kw("class") {
            return None;
        }
        self.pos += "class".len();
        self.skip_ws();
        let (rest_start, rest, end) = self.capture_to_stmt_end();
        let statement_span = SourceSpan::new(start, end);
        let (targets, class_name) = directive_argument_spans(&rest, rest_start);
        let class_name = active_following_span(&rest, rest_start, class_name);
        match lex::parse_class_assign_stmt_controlled(&rest, self.region.as_ref()) {
            Ok(mut stmt) => {
                if let Err(error) = lex::attach_class_assign_stmt_spans(
                    &mut stmt,
                    &rest,
                    rest_start,
                    self.region.as_ref(),
                ) {
                    return Some(Err(error));
                }
                stmt.editor_evidence = directive_editor_evidence(
                    statement_span,
                    targets.map(|span| (EditorExpectedSyntaxKind::IdList, span)),
                    class_name.map(|span| (EditorExpectedSyntaxKind::ClassName, span)),
                );
                Some(Ok((start, Tok::ClassAssignStmt(stmt), end)))
            }
            Err(error) => {
                let target = targets.unwrap_or_else(|| SourceSpan::new(end, end));
                Some(Err(directive_error(
                    error,
                    "class",
                    statement_span,
                    [(EditorExpectedSyntaxKind::IdList, target)]
                        .into_iter()
                        .chain(class_name.map(|span| (EditorExpectedSyntaxKind::ClassName, span))),
                )))
            }
        }
    }

    pub(super) fn lex_click_stmt(
        &mut self,
    ) -> Option<std::result::Result<(usize, Tok, usize), LexError>> {
        let start = self.pos;
        if !self.starts_with_kw("click") {
            return None;
        }
        self.pos += "click".len();
        self.skip_ws();
        let (rest_start, rest, end) = self.capture_to_stmt_end();
        let statement_span = SourceSpan::new(start, end);
        let (target, _) = directive_argument_spans(&rest, rest_start);
        match lex::parse_click_stmt_controlled(&rest, rest_start, self.region.as_ref()) {
            Ok(mut stmt) => {
                stmt.editor_evidence = directive_editor_evidence(
                    statement_span,
                    target.map(|span| (EditorExpectedSyntaxKind::NodeIdentifier, span)),
                    None,
                );
                Some(Ok((start, Tok::ClickStmt(stmt), end)))
            }
            Err(error) => {
                let target = target.unwrap_or_else(|| SourceSpan::new(end, end));
                let error = directive_error(
                    error,
                    "click",
                    statement_span,
                    [(EditorExpectedSyntaxKind::NodeIdentifier, target)],
                );
                if self.recover_partial_node_labels && target.start < target.end {
                    Some(Ok((
                        start,
                        Tok::ClickStmt(ClickStmt {
                            ids: vec![self.input[target.start..target.end].to_string()],
                            id_spans: vec![target],
                            tooltip: None,
                            action: ClickAction::Callback,
                            editor_evidence: directive_editor_evidence(
                                statement_span,
                                Some((EditorExpectedSyntaxKind::NodeIdentifier, target)),
                                None,
                            ),
                            interaction_evidence: FlowchartClickEditorEvidence::default(),
                            recovery_error: Some(error),
                        }),
                        end,
                    )))
                } else {
                    Some(Err(error))
                }
            }
        }
    }

    pub(super) fn lex_link_style_stmt(
        &mut self,
    ) -> Option<std::result::Result<(usize, Tok, usize), LexError>> {
        let start = self.pos;
        if !self.starts_with_kw("linkStyle") {
            return None;
        }
        self.pos += "linkStyle".len();
        self.skip_ws();
        let (rest_start, rest, end) = self.capture_to_stmt_end();
        match lex::parse_link_style_stmt_controlled(&rest, rest_start, self.region.as_ref()) {
            Ok(stmt) => Some(Ok((start, Tok::LinkStyleStmt(stmt), end))),
            Err(e) => Some(Err(e)),
        }
    }

    pub(super) fn lex_subgraph_header_after_keyword(
        &mut self,
        keyword_start: usize,
    ) -> Option<std::result::Result<(usize, Tok, usize), LexError>> {
        // Mermaid's current flowchart parser treats all horizontal whitespace after the
        // `subgraph` keyword as a separator. Extra spaces therefore do not become part of the
        // header token and must not force an otherwise named subgraph onto an auto-generated id.
        //
        // Example:
        // - `subgraph main`   -> id stays `main`
        // - `subgraph  main`  -> id also stays `main`
        let rest = &self.input[self.pos..];
        if rest.starts_with('\n') || rest.starts_with("\r\n") || rest.starts_with(';') {
            return None;
        }
        while let Some(ch) = self.input[self.pos..].chars().next()
            && ch != '\n'
            && ch != '\r'
            && is_ecmascript_trim_char(ch)
        {
            self.pos += ch.len_utf8();
        }

        let start = self.pos;
        if start >= self.input.len() {
            return None;
        }
        match self.input.as_bytes()[start] {
            b'\n' | b'\r' | b';' => return None,
            _ => {}
        }

        let mut in_quote = false;
        while self.pos < self.input.len() {
            let b = self.input.as_bytes()[self.pos];
            if in_quote {
                if b == b'"' {
                    in_quote = false;
                }
                self.pos += 1;
                continue;
            }
            if b == b'"' {
                in_quote = true;
                self.pos += 1;
                continue;
            }
            if b == b'\n' || b == b'\r' || b == b';' || b == b'[' {
                break;
            }
            self.pos += 1;
        }

        let raw_id_end = self.pos;
        let raw_id = match self.owned(&self.input[start..raw_id_end]) {
            Ok(value) => value,
            Err(error) => return Some(Err(error)),
        };
        let mut raw_title = match self.owned(&raw_id) {
            Ok(value) => value,
            Err(error) => return Some(Err(error)),
        };
        let mut title_kind = TitleKind::Text;
        let mut id_equals_title = true;

        if self.pos < self.input.len() && self.input.as_bytes()[self.pos] == b'[' {
            id_equals_title = false;
            self.pos += 1;
            let title_start = self.pos;
            in_quote = false;
            while self.pos < self.input.len() {
                let b = self.input.as_bytes()[self.pos];
                if in_quote {
                    if b == b'"' {
                        in_quote = false;
                    }
                    self.pos += 1;
                    continue;
                }
                if b == b'"' {
                    in_quote = true;
                    self.pos += 1;
                    continue;
                }
                if b == b']' {
                    break;
                }
                if b == b'\n' || b == b'\r' {
                    break;
                }
                self.pos += 1;
            }
            raw_title = match self.owned(&self.input[title_start..self.pos]) {
                Ok(value) => value,
                Err(error) => return Some(Err(error)),
            };
            let parsed_title =
                match lex::parse_node_label_text_controlled(&raw_title, self.region.as_ref()) {
                    Ok(parsed) => parsed,
                    Err(error) => {
                        return Some(Err(
                            self.error_span(error.message, SourceSpan::new(title_start, self.pos))
                        ));
                    }
                };
            title_kind = parsed_title.kind;
            if self.pos < self.input.len() && self.input.as_bytes()[self.pos] == b']' {
                self.pos += 1;
            }
        } else if raw_id.contains('"') && !(raw_id.starts_with('"') && raw_id.ends_with('"')) {
            return Some(Err(self.error_span(
                "Invalid subgraph header: quoted strings cannot be mixed with unquoted text",
                SourceSpan::new(start, raw_id_end),
            )));
        }

        Some(Ok((
            start,
            Tok::SubgraphHeader(SubgraphHeader {
                raw_id,
                header_span: Some(SourceSpan::new(keyword_start, self.pos)),
                raw_id_span: Some(SourceSpan::new(start, raw_id_end)),
                raw_title,
                title_kind,
                id_equals_title,
            }),
            self.pos,
        )))
    }

    pub(super) fn lex_amp(&mut self) -> Option<(usize, Tok, usize)> {
        let start = self.pos;
        if self.peek()? != b'&' {
            return None;
        }
        self.pos += 1;
        Some((start, Tok::Amp, self.pos))
    }

    pub(super) fn lex_id(&mut self) -> Option<std::result::Result<(usize, Tok, usize), LexError>> {
        let start = self.pos;
        let bytes = self.input.as_bytes();
        if start >= bytes.len() {
            return None;
        }
        let first = bytes[start];
        if first.is_ascii_alphanumeric() || first == b'_' {
            self.pos += 1;
        } else if let Some(len) = non_ascii_id_char_len(self.input, start) {
            self.pos += len;
        } else {
            return None;
        }

        while self.pos < bytes.len() {
            if self.pos + 1 < bytes.len()
                && (bytes[self.pos] == b'-' && bytes[self.pos + 1] == b'-'
                    || bytes[self.pos] == b'=' && bytes[self.pos + 1] == b'=')
            {
                break;
            }
            let b = bytes[self.pos];
            if b.is_ascii_alphanumeric() || b == b'_' {
                self.pos += 1;
                continue;
            }
            if let Some(len) = non_ascii_id_char_len(self.input, self.pos) {
                self.pos += len;
                continue;
            }
            if b == b'-' {
                if self.pos + 1 < bytes.len() && bytes[self.pos + 1] == b'-' {
                    break;
                }
                // Dotted edges start with `-.` (e.g. `A-.->B`). Avoid consuming the link start as
                // part of the id while still allowing ids like `subcontainer-child`.
                if self.pos + 1 < bytes.len() && bytes[self.pos + 1] == b'.' {
                    break;
                }
                self.pos += 1;
                continue;
            }
            if b == b'.' {
                // Allow dots inside ids (Mermaid supports nodes like `P1.5`), but avoid consuming
                // the `.` that starts a dotted link token like `.->` when it is directly adjacent
                // to an id (e.g. `A.->B`).
                if self.pos + 1 < bytes.len() && bytes[self.pos + 1] == b'-' {
                    break;
                }
                self.pos += 1;
                continue;
            }
            break;
        }

        if self.pos <= start {
            return None;
        }

        if self.region.is_some() && self.pos - start > 128 {
            return Some(Err(self.native_limit()));
        }

        Some(
            self.owned(&self.input[start..self.pos])
                .map(|id| (start, Tok::Id(id), self.pos)),
        )
    }

    pub(super) fn lex_arrow_and_label(
        &mut self,
    ) -> Option<std::result::Result<(usize, Tok, usize), LexError>> {
        let start = self.pos;
        let bytes = self.input.as_bytes();

        #[derive(Debug, Clone, Copy, PartialEq, Eq)]
        enum LinkFamily {
            Normal,
            Thick,
            Dotted,
            Invisible,
        }

        struct LinkEndMatch<'a> {
            label_end: usize,
            match_end: usize,
            operator: &'a str,
            operator_span: SourceSpan,
        }

        struct StartLinkMatch<'a> {
            family: LinkFamily,
            operator: &'a str,
            operator_span: SourceSpan,
            match_end: usize,
        }

        let input = self.input;
        let region = self.region.as_ref();
        let match_link_end = |pos: usize, family: LinkFamily| -> Option<LinkEndMatch<'input>> {
            let len = bytes.len();
            let label_end = pos;
            let operator_start = skip_ecmascript_whitespace(self.input, pos);
            if operator_start >= len {
                return None;
            }

            let mut cur = operator_start;
            let start_marker = bytes[cur];
            if matches!(start_marker, b'x' | b'o' | b'<') {
                cur += 1;
                if cur >= len {
                    return None;
                }
            }

            match family {
                LinkFamily::Invisible => {
                    cur = operator_start;
                    let mut tildes = 0usize;
                    while cur < len && bytes[cur] == b'~' {
                        tildes += 1;
                        cur += 1;
                    }
                    if tildes < 3 {
                        return None;
                    }
                }
                LinkFamily::Normal => {
                    let hyphen_start = cur;
                    while cur < len && bytes[cur] == b'-' {
                        cur += 1;
                    }
                    let hyphens = cur - hyphen_start;
                    if hyphens < 2 {
                        return None;
                    }
                    if cur < len {
                        match bytes[cur] {
                            b'x' | b'o' | b'>' => {
                                cur += 1;
                            }
                            _ => {
                                // Open-ended edge: `--+` + `-` requires at least 3 hyphens total.
                                if hyphens < 3 {
                                    return None;
                                }
                            }
                        }
                    } else if hyphens < 3 {
                        return None;
                    }
                }
                LinkFamily::Thick => {
                    let eq_start = cur;
                    while cur < len && bytes[cur] == b'=' {
                        cur += 1;
                    }
                    let eqs = cur - eq_start;
                    if eqs < 2 {
                        return None;
                    }
                    if cur < len {
                        match bytes[cur] {
                            b'x' | b'o' | b'>' => {
                                cur += 1;
                            }
                            _ => {
                                // Open-ended edge: `==+` + `=` requires at least 3 '=' total.
                                if eqs < 3 {
                                    return None;
                                }
                            }
                        }
                    } else if eqs < 3 {
                        return None;
                    }
                }
                LinkFamily::Dotted => {
                    if cur < len && bytes[cur] == b'-' {
                        cur += 1;
                    }
                    let mut dots = 0usize;
                    while cur < len && bytes[cur] == b'.' {
                        dots += 1;
                        cur += 1;
                    }
                    if dots == 0 {
                        return None;
                    }
                    if cur >= len || bytes[cur] != b'-' {
                        return None;
                    }
                    cur += 1;
                    if cur < len && matches!(bytes[cur], b'x' | b'o' | b'>') {
                        cur += 1;
                    }
                }
            }

            let operator_end = cur;
            let match_end = skip_ecmascript_whitespace(self.input, operator_end);
            Some(LinkEndMatch {
                label_end,
                match_end,
                operator: &input[operator_start..operator_end],
                operator_span: SourceSpan::new(operator_start, operator_end),
            })
        };

        let compute_link =
            |end: &str, start: Option<&str>| -> std::result::Result<LinkToken, LexError> {
                let end_semantics = if start.is_some() {
                    destruct_labeled_end_link(end)
                } else {
                    destruct_end_link(end)
                };
                let mut start_marker = end_semantics.start_marker;

                if let Some(start_str) = start {
                    let start_semantics = destruct_start_link(start_str);
                    if start_semantics.stroke_kind != end_semantics.stroke_kind {
                        if region.is_some() {
                            return Err(LexError::terminal());
                        }
                        return Err(LexError::new(
                            "Invalid link: stroke mismatch between start and end".to_string(),
                        ));
                    }

                    start_marker = start_semantics.marker;
                }

                Ok(LinkToken {
                    end: lex::owned_text(region, end)?,
                    start_marker,
                    end_marker: end_semantics.end_marker,
                    stroke_kind: end_semantics.stroke_kind,
                    visibility: end_semantics.visibility,
                    length: end_semantics.length,
                })
            };

        // 1) Prefer full LINK tokens, matching their source-order priority before START_LINK.
        let families = [
            LinkFamily::Invisible,
            LinkFamily::Thick,
            LinkFamily::Normal,
            LinkFamily::Dotted,
        ];
        for family in families {
            if let Some(link_match) = match_link_end(self.pos, family) {
                self.pos = link_match.match_end;
                let arrow_end = link_match.match_end;
                let link = match compute_link(link_match.operator, None) {
                    Ok(v) => v,
                    Err(e) => return Some(Err(e)),
                };
                let arrow = ArrowToken {
                    link,
                    recovery_error: None,
                };

                // Optional pipe label: `A--x|label|B` or `A --> |label| B`.
                let pipe_pos = self.pos;
                if pipe_pos < self.input.len() && bytes[pipe_pos] == b'|' {
                    self.pos = pipe_pos + 1;
                    let label_start = self.pos;
                    if let Some(label_end) = find_pipe_label_end(self.input, label_start) {
                        self.pos = label_end;
                        let raw = &self.input[label_start..self.pos];
                        let raw_span = SourceSpan::new(label_start, self.pos);
                        let parsed = match lex::parse_node_label_text_controlled(
                            raw,
                            self.region.as_ref(),
                        ) {
                            Ok(parsed) => parsed,
                            Err(error) => {
                                self.pos += 1;
                                return Some(Err(self.error_span(error.message, raw_span)));
                            }
                        };
                        self.pos += 1;
                        let token_span = SourceSpan::new(pipe_pos, self.pos);
                        let label = labeled_text_with_spans(
                            self.input,
                            LabeledText {
                                text: parsed.text,
                                kind: parsed.kind,
                                span: None,
                                selection: None,
                            },
                            token_span,
                            raw_span,
                        );
                        self.pending = Some(Ok((pipe_pos, Tok::EdgeLabel(label), self.pos)));
                    } else {
                        self.pos = self.input.len();
                        let error = self
                            .error_span(
                                "Unterminated flowchart pipe edge label",
                                SourceSpan::new(pipe_pos, self.pos),
                            )
                            .expecting(
                                crate::EditorExpectedSyntaxKind::Payload,
                                SourceSpan::new(self.pos, self.pos),
                            );
                        if self.recover_partial_node_labels {
                            let mut arrow = arrow;
                            arrow.recovery_error = Some(error);
                            return Some(Ok((start, Tok::Arrow(arrow), self.pos)));
                        }
                        return Some(Err(error));
                    }
                }

                return Some(Ok((start, Tok::Arrow(arrow), arrow_end)));
            }
        }

        // 2) START_LINK + edgeText + LINK (new notation): A-- text -->B
        let parse_start_link = |pos: usize| -> Option<StartLinkMatch<'input>> {
            let len = bytes.len();
            let operator_start = skip_ecmascript_whitespace(self.input, pos);
            if operator_start >= len {
                return None;
            }
            let mut cur = operator_start;
            if matches!(bytes[cur], b'x' | b'o' | b'<') {
                cur += 1;
                if cur >= len {
                    return None;
                }
            }

            if cur + 1 < len && bytes[cur] == b'-' && bytes[cur + 1] == b'-' {
                cur += 2;
                return Some(StartLinkMatch {
                    family: LinkFamily::Normal,
                    operator: &input[operator_start..cur],
                    operator_span: SourceSpan::new(operator_start, cur),
                    match_end: skip_ecmascript_whitespace(self.input, cur),
                });
            }
            if cur + 1 < len && bytes[cur] == b'=' && bytes[cur + 1] == b'=' {
                cur += 2;
                return Some(StartLinkMatch {
                    family: LinkFamily::Thick,
                    operator: &input[operator_start..cur],
                    operator_span: SourceSpan::new(operator_start, cur),
                    match_end: skip_ecmascript_whitespace(self.input, cur),
                });
            }
            if cur + 1 < len && bytes[cur] == b'-' && bytes[cur + 1] == b'.' {
                cur += 2;
                return Some(StartLinkMatch {
                    family: LinkFamily::Dotted,
                    operator: &input[operator_start..cur],
                    operator_span: SourceSpan::new(operator_start, cur),
                    match_end: skip_ecmascript_whitespace(self.input, cur),
                });
            }
            None
        };

        let Some(start_match) = parse_start_link(self.pos) else {
            let operator_start = skip_ecmascript_whitespace(self.input, self.pos);
            if self.input[operator_start..].starts_with("->") {
                self.pos = operator_start + 2;
                let selection = SourceSpan::new(operator_start, self.pos);
                return Some(Err(self
                    .error_span("incomplete flowchart edge operator", selection)
                    .expecting(
                        crate::EditorExpectedSyntaxKind::FlowchartOperator,
                        selection,
                    )));
            }
            return None;
        };
        let family = start_match.family;
        let after_start = start_match.match_end;
        let edge_text_start = after_start;

        #[derive(Debug, Clone, Copy, PartialEq, Eq)]
        enum EdgeTextState {
            Plain,
            String,
            MarkdownString,
        }

        let arrow_token =
            |link: LinkToken, _end_span: SourceSpan, recovery_error: Option<LexError>| ArrowToken {
                link,
                recovery_error,
            };

        let mut scan = edge_text_start;
        let mut state = EdgeTextState::Plain;
        while scan < self.input.len() {
            let rest = &self.input[scan..];
            match state {
                EdgeTextState::Plain => {
                    if rest.starts_with("\"`") {
                        state = EdgeTextState::MarkdownString;
                        scan += 2;
                        continue;
                    }
                    if rest.starts_with('"') {
                        state = EdgeTextState::String;
                        scan += 1;
                        continue;
                    }
                    let whitespace_end = skip_ecmascript_whitespace(self.input, scan);
                    if let Some(link_match) = match_link_end(scan, family) {
                        let match_start = link_match.label_end;
                        let match_end = link_match.match_end;
                        let raw_text = &self.input[edge_text_start..match_start];
                        let raw_span = SourceSpan::new(edge_text_start, match_start);
                        self.pos = match_end;

                        let parsed =
                            match lex::parse_edge_text_controlled(raw_text, self.region.as_ref()) {
                                Ok(parsed) => parsed,
                                Err(error) => {
                                    let error = self.error_span(error.message, raw_span).expecting(
                                        crate::EditorExpectedSyntaxKind::Payload,
                                        raw_span,
                                    );
                                    if self.recover_partial_node_labels
                                        && let Ok(link) = compute_link(
                                            link_match.operator,
                                            Some(start_match.operator),
                                        )
                                    {
                                        let arrow = arrow_token(
                                            link,
                                            link_match.operator_span,
                                            Some(error),
                                        );
                                        return Some(Ok((start, Tok::Arrow(arrow), match_end)));
                                    }
                                    return Some(Err(error));
                                }
                            };
                        let link =
                            match compute_link(link_match.operator, Some(start_match.operator)) {
                                Ok(v) => v,
                                Err(e) => return Some(Err(e)),
                            };
                        let arrow = arrow_token(link, link_match.operator_span, None);

                        let label = labeled_text_with_spans(
                            self.input,
                            LabeledText {
                                text: parsed.text,
                                kind: parsed.kind,
                                span: None,
                                selection: None,
                            },
                            SourceSpan::new(edge_text_start, match_start),
                            raw_span,
                        );
                        self.pending =
                            Some(Ok((edge_text_start, Tok::EdgeLabel(label), match_end)));
                        return Some(Ok((start, Tok::Arrow(arrow), after_start)));
                    }

                    if whitespace_end != scan {
                        // The terminator probe already inspected this whole whitespace run. Skip
                        // it once instead of probing every suffix, which would make long internal
                        // whitespace in an edge label quadratic.
                        scan = whitespace_end;
                        continue;
                    }

                    let invalid_edge_text = match family {
                        LinkFamily::Normal => rest.starts_with("--"),
                        LinkFamily::Thick => rest.starts_with('='),
                        LinkFamily::Dotted => rest.starts_with('.'),
                        LinkFamily::Invisible => false,
                    };
                    if invalid_edge_text {
                        let ch = rest
                            .chars()
                            .next()
                            .expect("edge label scan position must contain a character");
                        let error_end = scan + ch.len_utf8();
                        self.pos = error_end;
                        return Some(Err(self.error_span(
                            "Invalid character sequence in flowchart edge label",
                            SourceSpan::new(scan, error_end),
                        )));
                    }
                }
                EdgeTextState::String => {
                    if rest.starts_with('"') {
                        state = EdgeTextState::Plain;
                        scan += 1;
                        continue;
                    }
                }
                EdgeTextState::MarkdownString => {
                    if rest.starts_with("`\"") {
                        state = EdgeTextState::Plain;
                        scan += 2;
                        continue;
                    }
                    if rest.starts_with(['`', '"']) {
                        let ch = rest
                            .chars()
                            .next()
                            .expect("edge label scan position must contain a character");
                        let error_end = scan + ch.len_utf8();
                        self.pos = error_end;
                        return Some(Err(self.error_span(
                            "Invalid Markdown string in flowchart edge label",
                            SourceSpan::new(scan, error_end),
                        )));
                    }
                }
            }
            let ch = self.input[scan..]
                .chars()
                .next()
                .expect("edge label scan position must contain a character");
            scan += ch.len_utf8();
        }

        self.pos = self.input.len();
        Some(Err(self
            .error_span(
                "Unterminated edge label (missing link terminator)",
                SourceSpan::new(edge_text_start, self.pos),
            )
            .expecting(
                crate::EditorExpectedSyntaxKind::FlowchartOperator,
                SourceSpan::new(start_match.operator_span.start, after_start),
            )))
    }

    pub(super) fn lex_node_label(
        &mut self,
    ) -> Option<std::result::Result<(usize, Tok, usize), LexError>> {
        let start = self.pos;
        let rest = &self.input[self.pos..];

        if rest.starts_with("[\\") {
            let open = "[\\";
            let content_start = self.pos + open.len();
            let end_slash = lex::find_unquoted_delim(self.input, content_start, "/]");
            let end_backslash = lex::find_unquoted_delim(self.input, content_start, "\\]");

            let (end_start, close, shape) = match (end_slash, end_backslash) {
                (None, None) => {
                    if self.recover_partial_node_labels {
                        let (raw_start, raw, token_end) =
                            self.capture_recovery_to_stmt_end_from(content_start);
                        let token = build_partial_node_label_token_from_raw(
                            self.input,
                            "inv_trapezoid",
                            SourceSpan::new(start, token_end),
                            SourceSpan::new(content_start, token_end),
                            &raw,
                            SourceSpan::new(raw_start, token_end),
                            PartialNodeLabelRecovery {
                                trigger_span: Some(SourceSpan::new(start, content_start)),
                                error: self.error_span(
                                    "Unterminated node label (missing `/]` or `\\]`)",
                                    SourceSpan::new(start, token_end),
                                ),
                            },
                        );
                        self.pos = token_end;
                        return Some(Ok((start, token, self.pos)));
                    }
                    let (_, _, token_end) = self.capture_to_stmt_end_from(content_start);
                    self.pos = token_end;
                    return Some(Err(self.error_span(
                        "Unterminated node label (missing `/]` or `\\]`)",
                        SourceSpan::new(start, token_end),
                    )));
                }
                (Some(p), None) => (p, "/]", "inv_trapezoid"),
                (None, Some(p)) => (p, "\\]", "lean_left"),
                (Some(a), Some(b)) => {
                    if a <= b {
                        (a, "/]", "inv_trapezoid")
                    } else {
                        (b, "\\]", "lean_left")
                    }
                }
            };

            let token_end = end_start + close.len();
            let token = match build_node_label_token(
                self.region.as_ref(),
                self.input,
                shape,
                SourceSpan::new(start, token_end),
                SourceSpan::new(content_start, end_start),
                None,
            ) {
                Ok(v) => v,
                Err(e) => {
                    self.pos = token_end;
                    return Some(Err(e));
                }
            };
            self.pos = token_end;
            return Some(Ok((start, token, self.pos)));
        }

        if rest.starts_with("[/") {
            let open = "[/";
            let content_start = self.pos + open.len();
            let end_slash = lex::find_unquoted_delim(self.input, content_start, "/]");
            let end_backslash = lex::find_unquoted_delim(self.input, content_start, "\\]");

            let (end_start, close, shape) = match (end_slash, end_backslash) {
                (None, None) => {
                    if self.recover_partial_node_labels {
                        let (raw_start, raw, token_end) =
                            self.capture_recovery_to_stmt_end_from(content_start);
                        let token = build_partial_node_label_token_from_raw(
                            self.input,
                            "lean_right",
                            SourceSpan::new(start, token_end),
                            SourceSpan::new(content_start, token_end),
                            &raw,
                            SourceSpan::new(raw_start, token_end),
                            PartialNodeLabelRecovery {
                                trigger_span: Some(SourceSpan::new(start, content_start)),
                                error: self.error_span(
                                    "Unterminated node label (missing `/]` or `\\]`)",
                                    SourceSpan::new(start, token_end),
                                ),
                            },
                        );
                        self.pos = token_end;
                        return Some(Ok((start, token, self.pos)));
                    }
                    let (_, _, token_end) = self.capture_to_stmt_end_from(content_start);
                    self.pos = token_end;
                    return Some(Err(self.error_span(
                        "Unterminated node label (missing `/]` or `\\]`)",
                        SourceSpan::new(start, token_end),
                    )));
                }
                (Some(p), None) => (p, "/]", "lean_right"),
                (None, Some(p)) => (p, "\\]", "trapezoid"),
                (Some(a), Some(b)) => {
                    if a <= b {
                        (a, "/]", "lean_right")
                    } else {
                        (b, "\\]", "trapezoid")
                    }
                }
            };

            let token_end = end_start + close.len();
            let token = match build_node_label_token(
                self.region.as_ref(),
                self.input,
                shape,
                SourceSpan::new(start, token_end),
                SourceSpan::new(content_start, end_start),
                None,
            ) {
                Ok(v) => v,
                Err(e) => {
                    self.pos = token_end;
                    return Some(Err(e));
                }
            };
            self.pos = token_end;
            return Some(Ok((start, token, self.pos)));
        }

        let candidates: [(&str, &str, &str); 8] = [
            ("(((", ")))", "doublecircle"),
            ("{{", "}}", "hexagon"),
            ("[[", "]]", "subroutine"),
            ("(-", "-)", "ellipse"),
            ("([", "])", "stadium"),
            ("[(", ")]", "cylinder"),
            ("((", "))", "circle"),
            (">", "]", "odd"),
        ];

        for (open, close, shape) in candidates {
            if !rest.starts_with(open) {
                continue;
            }
            let content_start = self.pos + open.len();
            let token = if let Some(end_start) =
                lex::find_unquoted_delim(self.input, content_start, close)
            {
                let token_end = end_start + close.len();
                let token = match build_node_label_token(
                    self.region.as_ref(),
                    self.input,
                    shape,
                    SourceSpan::new(start, token_end),
                    SourceSpan::new(content_start, end_start),
                    None,
                ) {
                    Ok(v) => v,
                    Err(e) => {
                        self.pos = token_end;
                        return Some(Err(e));
                    }
                };
                self.pos = token_end;
                token
            } else {
                if !self.recover_partial_node_labels {
                    let (_, _, token_end) = self.capture_to_stmt_end_from(content_start);
                    self.pos = token_end;
                    return Some(Err(self.error_span(
                        if self.region.is_some() {
                            String::new()
                        } else {
                            format!("Unterminated node label (missing `{close}`)")
                        },
                        SourceSpan::new(start, token_end),
                    )));
                }
                let (raw_start, raw, token_end) =
                    self.capture_recovery_to_stmt_end_from(content_start);
                let token = build_partial_node_label_token_from_raw(
                    self.input,
                    shape,
                    SourceSpan::new(start, token_end),
                    SourceSpan::new(content_start, token_end),
                    &raw,
                    SourceSpan::new(raw_start, token_end),
                    PartialNodeLabelRecovery {
                        trigger_span: Some(SourceSpan::new(start, content_start)),
                        error: self.error_span(
                            if self.region.is_some() {
                                String::new()
                            } else {
                                format!("Unterminated node label (missing `{close}`)")
                            },
                            SourceSpan::new(start, token_end),
                        ),
                    },
                );
                self.pos = token_end;
                token
            };
            return Some(Ok((start, token, self.pos)));
        }

        if rest.starts_with("[") {
            let content_start = self.pos + 1;
            let token =
                if let Some(end_start) = lex::find_unquoted_delim(self.input, content_start, "]") {
                    let token_end = end_start + 1;
                    let raw = &self.input[content_start..end_start];
                    let raw_span = SourceSpan::new(content_start, end_start);
                    let (shape, label_raw, label_offset) = lex::parse_rect_border_label(raw);
                    let label_span = SourceSpan::new(
                        raw_span.start + label_offset,
                        raw_span.start + label_offset + label_raw.len(),
                    );
                    let token = match build_node_label_token_from_raw(
                        self.region.as_ref(),
                        self.input,
                        shape,
                        SourceSpan::new(start, token_end),
                        SourceSpan::new(content_start, end_start),
                        label_raw,
                        label_span,
                        None,
                    ) {
                        Ok(v) => v,
                        Err(e) => {
                            self.pos = token_end;
                            return Some(Err(e));
                        }
                    };
                    self.pos = token_end;
                    token
                } else {
                    if !self.recover_partial_node_labels {
                        let (_, _, token_end) = self.capture_to_stmt_end_from(content_start);
                        self.pos = token_end;
                        return Some(Err(self.error_span(
                            "Unterminated node label (missing `]`)",
                            SourceSpan::new(start, token_end),
                        )));
                    }
                    let (raw_start, raw, token_end) =
                        self.capture_recovery_to_stmt_end_from(content_start);
                    let (shape, label_raw, label_offset) = lex::parse_rect_border_label(&raw);
                    let label_span = SourceSpan::new(
                        raw_start + label_offset,
                        raw_start + label_offset + label_raw.len(),
                    );
                    let token = build_partial_node_label_token_from_raw(
                        self.input,
                        shape,
                        SourceSpan::new(start, token_end),
                        SourceSpan::new(content_start, token_end),
                        label_raw,
                        label_span,
                        PartialNodeLabelRecovery {
                            trigger_span: Some(SourceSpan::new(start, content_start)),
                            error: self.error_span(
                                "Unterminated node label (missing `]`)",
                                SourceSpan::new(start, token_end),
                            ),
                        },
                    );
                    self.pos = token_end;
                    token
                };
            return Some(Ok((start, token, self.pos)));
        }

        if rest.starts_with("{") {
            let content_start = self.pos + 1;
            let token =
                if let Some(end_start) = lex::find_unquoted_delim(self.input, content_start, "}") {
                    let token_end = end_start + 1;
                    let token = match build_node_label_token(
                        self.region.as_ref(),
                        self.input,
                        "diamond",
                        SourceSpan::new(start, token_end),
                        SourceSpan::new(content_start, end_start),
                        None,
                    ) {
                        Ok(v) => v,
                        Err(e) => {
                            self.pos = token_end;
                            return Some(Err(e));
                        }
                    };
                    self.pos = token_end;
                    token
                } else {
                    if !self.recover_partial_node_labels {
                        let (_, _, token_end) = self.capture_to_stmt_end_from(content_start);
                        self.pos = token_end;
                        return Some(Err(self.error_span(
                            "Unterminated node label (missing `}`)",
                            SourceSpan::new(start, token_end),
                        )));
                    }
                    let (raw_start, raw, token_end) =
                        self.capture_recovery_to_stmt_end_from(content_start);
                    let token = build_partial_node_label_token_from_raw(
                        self.input,
                        "diamond",
                        SourceSpan::new(start, token_end),
                        SourceSpan::new(content_start, token_end),
                        &raw,
                        SourceSpan::new(raw_start, token_end),
                        PartialNodeLabelRecovery {
                            trigger_span: Some(SourceSpan::new(start, content_start)),
                            error: self.error_span(
                                "Unterminated node label (missing `}`)",
                                SourceSpan::new(start, token_end),
                            ),
                        },
                    );
                    self.pos = token_end;
                    token
                };
            return Some(Ok((start, token, self.pos)));
        }

        if rest.starts_with("(") {
            let content_start = self.pos + 1;
            let token =
                if let Some(end_start) = lex::find_unquoted_delim(self.input, content_start, ")") {
                    let token_end = end_start + 1;
                    let token = match build_node_label_token(
                        self.region.as_ref(),
                        self.input,
                        "round",
                        SourceSpan::new(start, token_end),
                        SourceSpan::new(content_start, end_start),
                        None,
                    ) {
                        Ok(v) => v,
                        Err(e) => {
                            self.pos = token_end;
                            return Some(Err(e));
                        }
                    };
                    self.pos = token_end;
                    token
                } else {
                    if !self.recover_partial_node_labels {
                        let (_, _, token_end) = self.capture_to_stmt_end_from(content_start);
                        self.pos = token_end;
                        return Some(Err(self.error_span(
                            "Unterminated node label (missing `)`)",
                            SourceSpan::new(start, token_end),
                        )));
                    }
                    let (raw_start, raw, token_end) =
                        self.capture_recovery_to_stmt_end_from(content_start);
                    let token = build_partial_node_label_token_from_raw(
                        self.input,
                        "round",
                        SourceSpan::new(start, token_end),
                        SourceSpan::new(content_start, token_end),
                        &raw,
                        SourceSpan::new(raw_start, token_end),
                        PartialNodeLabelRecovery {
                            trigger_span: Some(SourceSpan::new(start, content_start)),
                            error: self.error_span(
                                "Unterminated node label (missing `)`)",
                                SourceSpan::new(start, token_end),
                            ),
                        },
                    );
                    self.pos = token_end;
                    token
                };
            return Some(Ok((start, token, self.pos)));
        }

        None
    }
}

fn build_node_label_token(
    region: Option<&markdown_memory::RegionHandle>,
    input: &str,
    shape: &str,
    token_span: SourceSpan,
    content_span: SourceSpan,
    trigger_span: Option<SourceSpan>,
) -> std::result::Result<Tok, LexError> {
    let raw = &input[content_span.start..content_span.end];
    let raw_span = content_span;
    build_node_label_token_from_raw(
        region,
        input,
        shape,
        token_span,
        content_span,
        raw,
        raw_span,
        trigger_span,
    )
}

fn build_node_label_token_from_raw(
    region: Option<&markdown_memory::RegionHandle>,
    input: &str,
    shape: &str,
    token_span: SourceSpan,
    _content_span: SourceSpan,
    raw: &str,
    raw_span: SourceSpan,
    trigger_span: Option<SourceSpan>,
) -> std::result::Result<Tok, LexError> {
    let text = lex::parse_node_label_text_controlled(raw, region)?;
    Ok(Tok::NodeLabel(NodeLabelToken {
        shape: lex::owned_text(region, shape)?,
        text: labeled_text_with_spans(input, text, token_span, raw_span),
        trigger_span,
        recovery_error: None,
    }))
}

struct PartialNodeLabelRecovery {
    trigger_span: Option<SourceSpan>,
    error: LexError,
}

fn build_partial_node_label_token_from_raw(
    input: &str,
    shape: &str,
    token_span: SourceSpan,
    _content_span: SourceSpan,
    raw: &str,
    raw_span: SourceSpan,
    recovery: PartialNodeLabelRecovery,
) -> Tok {
    let (text, kind) = parse_label_text(raw);
    Tok::NodeLabel(NodeLabelToken {
        shape: shape.to_string(),
        text: labeled_text_with_spans(
            input,
            LabeledText {
                text,
                kind,
                span: None,
                selection: None,
            },
            token_span,
            raw_span,
        ),
        trigger_span: recovery.trigger_span,
        recovery_error: Some(recovery.error),
    })
}

fn labeled_text_with_spans(
    input: &str,
    mut text: LabeledText,
    token_span: SourceSpan,
    content_span: SourceSpan,
) -> LabeledText {
    text.span = Some(token_span);
    text.selection = label_value_selection(input, content_span, &text.text).or(Some(content_span));
    text
}

fn label_value_selection(input: &str, content_span: SourceSpan, value: &str) -> Option<SourceSpan> {
    if value.is_empty() {
        return None;
    }
    source_value_span(input, content_span, value)
}
