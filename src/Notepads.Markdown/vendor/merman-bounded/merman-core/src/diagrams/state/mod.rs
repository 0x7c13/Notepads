use std::collections::VecDeque;

use crate::diagrams::scan::{
    ScanStep, TokenScanner, consume_line_ending, is_ecmascript_inline_whitespace,
    is_ecmascript_whitespace,
};

mod ast;
mod db;
#[cfg(test)]
#[path = "tests.rs"]
mod lifecycle_tests;
mod parse;
mod render_model;

pub use render_model::{
    StateDiagramRenderEdge, StateDiagramRenderLink, StateDiagramRenderLinks,
    StateDiagramRenderModel, StateDiagramRenderNode, StateDiagramRenderNote,
    StateDiagramRenderRelation, StateDiagramRenderState, StateDiagramRenderStyleClass,
};
pub(crate) use render_model::{
    render_model_to_compat_json, render_model_to_compat_json_controlled,
};

pub(crate) use parse::{parse_state, parse_state_model_for_render_controlled};

pub(crate) use parse::parse_state_json_and_editor_facts;
#[cfg(test)]
pub(crate) use parse::{reset_state_syntax_construction_count, state_syntax_construction_count};

pub use ast::{
    StateDocument, StateDocumentId, StateStatement, StateStatementClick, StateStatementNote,
    StateStatementRelation, StateStatementState,
};

include_checked_in_lalrpop_parser!(
    #[allow(clippy::empty_line_after_outer_attr, clippy::filter_map_identity)]
    state_grammar,
    "state_grammar.rs"
);

#[derive(Debug, Clone)]
pub(crate) struct SpannedDirection {
    pub value: String,
    pub selection: crate::SourceSpan,
}

#[derive(Debug, Clone)]
pub(crate) enum Tok {
    Newline,
    Sd,
    Id(String),
    StyledId((String, String)),
    EdgeState,
    Descr(String),
    Arrow,
    StructStart,
    StructStop,
    As,
    Note,
    LeftOf,
    RightOf,
    NoteText(String),
    StateDescr(String),
    CompositState(String),
    Fork(String),
    Join(String),
    Choice(String),
    Concurrent,
    HideEmptyDescription,
    ScaleWidth(usize),
    ClassDef,
    ClassDefId(String),
    ClassDefStyleOpts(String),
    Class,
    ClassEntityIds(String),
    StyleClass(String),
    Style,
    StyleIds(String),
    StyleDefStyleOpts(String),
    Direction(SpannedDirection),
    AccTitle(String),
    AccDescr(String),
    AccDescrMultiline(String),
    Click,
    Href,
    StringLit(String),
}

#[derive(Debug, Clone, thiserror::Error)]
#[error("{message}")]
pub(crate) struct LexError {
    pub message: String,
    pub span: Option<crate::SourceSpan>,
    pub expected_syntax: Option<crate::EditorExpectedSyntax>,
}

impl LexError {
    fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
            span: None,
            expected_syntax: None,
        }
    }

    fn with_span(message: impl Into<String>, span: crate::SourceSpan) -> Self {
        Self {
            message: message.into(),
            span: Some(span),
            expected_syntax: None,
        }
    }
}

impl crate::error::ParseErrorSourceSpan for LexError {
    fn source_span(&self) -> Option<crate::SourceSpan> {
        self.span
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Mode {
    Default,
    Struct,
    State,
    StateId,
}

fn note_block_terminator_range(input: &str) -> Option<(usize, usize)> {
    let mut line_start = 0usize;
    for line_with_ending in input.split_inclusive('\n') {
        let line_end = line_start + line_with_ending.trim_end_matches(['\r', '\n']).len();
        if line_start > 0 {
            let line = &input[line_start..line_end];
            let leading = line.len() - line.trim_start_matches(char::is_whitespace).len();
            let content = &line[leading..];
            if content
                .trim_end_matches(char::is_whitespace)
                .eq_ignore_ascii_case("end note")
            {
                let marker_start = line_start + leading;
                return Some((marker_start, marker_start + "end note".len()));
            }
        }
        line_start += line_with_ending.len();
    }
    None
}

struct Lexer<'input> {
    input: &'input str,
    pos: usize,
    pending: VecDeque<(usize, Tok, usize)>,
    modes: Vec<Mode>,
    emitted_eof_newline: bool,
    direction_scan_end: usize,
}

impl<'input> Lexer<'input> {
    fn new(input: &'input str) -> Self {
        Self {
            input,
            pos: 0,
            pending: VecDeque::new(),
            modes: vec![Mode::Default],
            emitted_eof_newline: false,
            direction_scan_end: 0,
        }
    }

    fn emit_token(
        &mut self,
        token: (usize, Tok, usize),
    ) -> std::result::Result<(usize, Tok, usize), LexError> {
        Ok(token)
    }

    fn emit_result(
        &mut self,
        result: std::result::Result<(usize, Tok, usize), LexError>,
    ) -> std::result::Result<(usize, Tok, usize), LexError> {
        result
    }

    fn normalize_note_block_text(raw: &'input str) -> String {
        // Mermaid's state diagram note blocks do not preserve leading indentation for each line.
        // The upstream SVG baselines reflect line-wise trimming.
        let lines: Vec<&str> = raw
            .lines()
            .map(|l| l.trim_end_matches('\r').trim())
            .collect();

        let mut start = 0usize;
        let mut end = lines.len();
        while start < end && lines[start].is_empty() {
            start += 1;
        }
        while end > start && lines[end - 1].is_empty() {
            end -= 1;
        }

        lines[start..end].join("\n")
    }

    fn mode(&self) -> Mode {
        *self.modes.last().unwrap_or(&Mode::Default)
    }

    fn push_mode(&mut self, m: Mode) {
        self.modes.push(m);
    }

    fn pop_mode(&mut self) {
        if self.modes.len() > 1 {
            self.modes.pop();
        }
    }

    fn peek(&self) -> Option<u8> {
        self.input.as_bytes().get(self.pos).copied()
    }

    fn starts_with(&self, s: &str) -> bool {
        let hay = self.input.as_bytes();
        let pat = s.as_bytes();
        hay.get(self.pos..)
            .is_some_and(|tail| tail.starts_with(pat))
    }

    fn starts_with_ci(&self, s: &str) -> bool {
        let hay = self.input.as_bytes();
        let pat = s.as_bytes();
        hay.len() >= self.pos + pat.len()
            && hay[self.pos..self.pos + pat.len()].eq_ignore_ascii_case(pat)
    }

    fn starts_with_word_ci(&self, s: &str) -> bool {
        if !self.starts_with_ci(s) {
            return false;
        }
        let after = self.pos + s.len();
        if after >= self.input.len() {
            return true;
        }
        let next = self.input[after..].chars().next();
        next.is_some_and(|ch| {
            is_ecmascript_whitespace(ch) || matches!(ch, '{' | '}' | '[' | ']' | '"' | ':' | ';')
        })
    }

    fn skip_ws(&mut self) {
        while let Some(ch) = self.input[self.pos..].chars().next() {
            if !is_ecmascript_inline_whitespace(ch) {
                break;
            }
            self.pos += ch.len_utf8();
        }
    }

    fn read_to_newline(&mut self) -> String {
        let start = self.pos;
        while let Some(b) = self.peek() {
            if matches!(b, b'\r' | b'\n') {
                break;
            }
            self.pos += 1;
        }
        self.input[start..self.pos].to_string()
    }

    fn lex_newline(&mut self) -> Option<(usize, Tok, usize)> {
        let start = self.pos;
        self.pos = consume_line_ending(self.input, self.pos)?;
        while let Some(end) = consume_line_ending(self.input, self.pos) {
            self.pos = end;
        }
        if matches!(self.mode(), Mode::State | Mode::StateId) {
            self.pop_mode();
        }
        Some((start, Tok::Newline, self.pos))
    }

    fn skip_comment(&mut self) -> bool {
        if self.starts_with("%%") {
            let _ = self.read_to_newline();
            return true;
        }
        if self.peek() == Some(b'#') {
            let _ = self.read_to_newline();
            return true;
        }
        false
    }

    fn lex_string_lit(&mut self) -> Option<std::result::Result<(usize, Tok, usize), LexError>> {
        if self.peek()? != b'"' {
            return None;
        }
        let start = self.pos;
        self.pos += 1;
        let body_start = self.pos;
        while let Some(b) = self.peek() {
            if b == b'"' {
                break;
            }
            self.pos += 1;
        }
        if self.peek() != Some(b'"') {
            return Some(Err(LexError::new(
                "Unterminated string literal; missing '\"'",
            )));
        }
        let body = self.input[body_start..self.pos].to_string();
        self.pos += 1;
        Some(Ok((start, Tok::StringLit(body), self.pos)))
    }

    fn lex_sd_header(&mut self) -> Option<(usize, Tok, usize)> {
        let start = self.pos;
        for keyword in ["stateDiagram-v2", "stateDiagram"] {
            if !self.starts_with_ci(keyword) {
                continue;
            }
            let suffix = &self.input[start + keyword.len()..];
            let rest = suffix.trim_start_matches(is_ecmascript_whitespace);
            if rest.len() == suffix.len() {
                continue;
            }
            self.pos = self.input.len() - rest.len();
            return Some((start, Tok::Sd, self.pos));
        }
        None
    }

    fn lex_direction(&mut self) -> Option<std::result::Result<(usize, Tok, usize), LexError>> {
        let start = self.pos;
        if start < self.direction_scan_end || !matches!(self.mode(), Mode::Default | Mode::Struct) {
            return None;
        }
        let rest = &self.input[start..];
        // Match the complete ordered Jison rules before consuming an identifier.
        let prefix_end = rest
            .find(['\n', '\r', '\u{2028}', '\u{2029}'])
            .unwrap_or(rest.len());
        for direction in ["TB", "BT", "RL", "LR"] {
            let value_end = rest[..prefix_end]
                .char_indices()
                .rev()
                .find_map(|(offset, _)| {
                    let candidate = &rest[offset..];
                    if !candidate
                        .get(.."direction".len())?
                        .eq_ignore_ascii_case("direction")
                    {
                        return None;
                    }
                    let suffix = &candidate["direction".len()..];
                    let value = suffix.trim_start_matches(is_ecmascript_whitespace);
                    if value.len() == suffix.len()
                        || !value.get(..2)?.eq_ignore_ascii_case(direction)
                    {
                        return None;
                    }
                    Some(rest.len() - value.len() + 2)
                });
            if let Some(value_end) = value_end {
                self.pos = start + value_end;
                let tail = &self.input[self.pos..];
                self.pos += tail.find('\n').unwrap_or(tail.len());
                return Some(Ok((
                    start,
                    Tok::Direction(SpannedDirection {
                        value: direction.to_string(),
                        selection: crate::SourceSpan::new(start + value_end - 2, start + value_end),
                    }),
                    self.pos,
                )));
            }
        }
        // Avoid rescanning a line for each state identifier after a failed match.
        self.direction_scan_end = start + prefix_end;
        None
    }

    fn lex_accessibility(&mut self) -> Option<std::result::Result<(usize, Tok, usize), LexError>> {
        let start = self.pos;
        for keyword in ["accTitle", "accDescr"] {
            if !self.starts_with_ci(keyword) {
                continue;
            }
            let suffix =
                self.input[start + keyword.len()..].trim_start_matches(is_ecmascript_whitespace);
            if let Some(body) = suffix.strip_prefix(':') {
                let body = body.trim_start_matches(is_ecmascript_whitespace);
                self.pos = self.input.len() - body.len();
                let value = self.read_to_newline();
                let value = value.trim_matches(is_ecmascript_whitespace).to_string();
                let token = if keyword == "accTitle" {
                    Tok::AccTitle(value)
                } else {
                    Tok::AccDescr(value)
                };
                return Some(Ok((start, token, self.pos)));
            }
            if keyword == "accDescr"
                && let Some(body) = suffix.strip_prefix('{')
            {
                let body = body.trim_start_matches(is_ecmascript_whitespace);
                let Some(end_rel) = body.find('}') else {
                    self.pos = self.input.len();
                    // Jison emits the remaining body, then one empty value at EOF.
                    // Both assignments occur; the final empty description wins.
                    self.pending.push_back((
                        self.pos,
                        Tok::AccDescrMultiline(String::new()),
                        self.pos,
                    ));
                    return Some(Ok((
                        start,
                        Tok::AccDescrMultiline(body.to_string()),
                        self.pos,
                    )));
                };
                self.pos = self.input.len() - body.len() + end_rel + 1;
                return Some(Ok((
                    start,
                    Tok::AccDescrMultiline(body[..end_rel].to_string()),
                    self.pos,
                )));
            }
        }
        None
    }

    fn lex_stmt_line(&mut self) -> Option<std::result::Result<(usize, Tok, usize), LexError>> {
        let start = self.pos;

        if self.starts_with_ci("hide empty description") {
            self.pos += "hide empty description".len();
            let _ = self.read_to_newline();
            return Some(Ok((start, Tok::HideEmptyDescription, self.pos)));
        }

        if self.starts_with_word_ci("scale") {
            self.pos += "scale".len();
            self.skip_ws();

            let width_start = self.pos;
            while let Some(b) = self.peek() {
                if b.is_ascii_digit() {
                    self.pos += 1;
                    continue;
                }
                break;
            }
            if self.pos == width_start {
                return Some(Err(LexError::new("Expected a width number after 'scale'")));
            }
            let width: usize = match self.input[width_start..self.pos].parse() {
                Ok(v) => v,
                Err(_) => {
                    return Some(Err(LexError::new("Invalid width number after 'scale'")));
                }
            };

            self.skip_ws();
            if !self.starts_with_word_ci("width") {
                return Some(Err(LexError::new("Expected 'width' after `scale <n>`")));
            }
            self.pos += "width".len();
            let _ = self.read_to_newline();
            return Some(Ok((start, Tok::ScaleWidth(width), self.pos)));
        }

        if self.starts_with_word_ci("click") {
            self.pos += "click".len();
            return Some(Ok((start, Tok::Click, self.pos)));
        }
        if self.starts_with_word_ci("href") {
            self.pos += "href".len();
            return Some(Ok((start, Tok::Href, self.pos)));
        }

        if self.starts_with_word_ci("note") {
            let kw_end = self.pos + "note".len();
            self.pos = kw_end;
            self.skip_ws();

            // Floating note: note "text" as id
            if self.peek() == Some(b'"') {
                let Some(Ok((_s, Tok::StringLit(text), _e))) = self.lex_string_lit() else {
                    return Some(Err(LexError::new("Unterminated note string; missing '\"'")));
                };
                self.skip_ws();
                if !self.starts_with_word_ci("as") {
                    return Some(Err(LexError::new(
                        "Expected 'as' in floating note statement",
                    )));
                }
                let as_start = self.pos;
                self.pos += "as".len();
                self.skip_ws();
                let id_start = self.pos;
                let id = self.read_to_newline().trim().to_string();

                self.pending.push_back((start, Tok::Note, kw_end));
                self.pending
                    .push_back((kw_end, Tok::NoteText(text), as_start));
                self.pending.push_back((as_start, Tok::As, id_start));
                self.pending.push_back((id_start, Tok::Id(id), self.pos));
                return self.pending.pop_front().map(Ok);
            }

            // Positioned note: note left of|right of ID : text
            let pos_start = self.pos;
            let pos_tok = if self.starts_with_ci("left of") {
                self.pos += "left of".len();
                Tok::LeftOf
            } else if self.starts_with_ci("right of") {
                self.pos += "right of".len();
                Tok::RightOf
            } else {
                return Some(Err(LexError::new(
                    "Expected 'left of' or 'right of' after 'note'",
                )));
            };

            self.skip_ws();
            let id_start = self.pos;
            while let Some(ch) = self.input[self.pos..].chars().next() {
                if ch == ':' || ch == '\n' || is_ecmascript_whitespace(ch) || ch == '-' {
                    break;
                }
                self.pos += ch.len_utf8();
            }
            let id_end = self.pos;
            let id = self.input[id_start..self.pos].trim().to_string();

            self.skip_ws();
            let text_start = self.pos;
            let text = if self.peek() == Some(b':') {
                self.pos += 1;

                self.read_to_newline().trim().to_string()
            } else {
                let Some(rest) = self.input.get(self.pos..) else {
                    return Some(Err(LexError::new(
                        "Internal lexer error: invalid UTF-8 boundary",
                    )));
                };
                let Some((marker_start, marker_end)) = note_block_terminator_range(rest) else {
                    return Some(Err(LexError::new(
                        "Unterminated note block; missing 'end note'",
                    )));
                };
                let t = Self::normalize_note_block_text(&rest[..marker_start]);
                self.pos += marker_end;
                t
            };

            self.pending.push_back((start, Tok::Note, kw_end));
            self.pending.push_back((pos_start, pos_tok, id_start));
            self.pending.push_back((id_start, Tok::Id(id), id_end));
            self.pending
                .push_back((text_start, Tok::NoteText(text), self.pos));
            return self.pending.pop_front().map(Ok);
        }

        if self.starts_with_word_ci("classDef") {
            let kw_end = self.pos + "classDef".len();
            self.pos = kw_end;
            self.skip_ws();
            let id_start = self.pos;
            while let Some(b) = self.peek() {
                if b.is_ascii_alphanumeric() || b == b'_' {
                    self.pos += 1;
                    continue;
                }
                break;
            }
            let id_end = self.pos;
            let id = self.input[id_start..id_end].trim().to_string();
            self.skip_ws();
            let raw = self.read_to_newline().trim().to_string();

            self.pending.push_back((start, Tok::ClassDef, kw_end));
            self.pending
                .push_back((id_start, Tok::ClassDefId(id), id_end));
            self.pending
                .push_back((id_end, Tok::ClassDefStyleOpts(raw), self.pos));
            return self.pending.pop_front().map(Ok);
        }

        if self.starts_with_word_ci("class") {
            let kw_end = self.pos + "class".len();
            self.pos = kw_end;
            self.skip_ws();
            let ids_start = self.pos;
            let mut ids_end = self.pos;
            loop {
                let word_start = self.pos;
                while let Some(b) = self.peek() {
                    if b.is_ascii_alphanumeric() || b == b'_' {
                        self.pos += 1;
                        continue;
                    }
                    break;
                }
                if self.pos == word_start {
                    break;
                }
                ids_end = self.pos;
                let after_word = self.pos;
                self.skip_ws();
                if self.peek() == Some(b',') {
                    self.pos += 1;
                    self.skip_ws();
                    continue;
                }
                self.pos = after_word;
                break;
            }
            let ids = self.input[ids_start..ids_end].trim().to_string();
            self.skip_ws();
            let style = self.read_to_newline().trim().to_string();

            self.pending.push_back((start, Tok::Class, kw_end));
            self.pending
                .push_back((ids_start, Tok::ClassEntityIds(ids), ids_end));
            self.pending
                .push_back((ids_end, Tok::StyleClass(style), self.pos));
            return self.pending.pop_front().map(Ok);
        }

        if self.starts_with_word_ci("style") {
            let kw_end = self.pos + "style".len();
            self.pos = kw_end;
            self.skip_ws();
            let ids_start = self.pos;
            while let Some(b) = self.peek() {
                if b.is_ascii_alphanumeric() || b == b'_' || b == b',' {
                    self.pos += 1;
                    continue;
                }
                break;
            }
            let ids_end = self.pos;
            let ids = self.input[ids_start..ids_end].trim().to_string();
            self.skip_ws();
            let raw = self.read_to_newline().trim().to_string();

            self.pending.push_back((start, Tok::Style, kw_end));
            if !ids.is_empty() {
                self.pending
                    .push_back((ids_start, Tok::StyleIds(ids), ids_end));
                self.pending
                    .push_back((ids_end, Tok::StyleDefStyleOpts(raw), self.pos));
            }
            return self.pending.pop_front().map(Ok);
        }

        if self.starts_with_word_ci("state") {
            self.pos += "state".len();
            self.push_mode(Mode::State);
            return None;
        }

        None
    }

    fn lex_state_mode_token(
        &mut self,
    ) -> Option<std::result::Result<(usize, Tok, usize), LexError>> {
        self.skip_ws();
        let start = self.pos;

        self.peek()?;

        if self.mode() == Mode::StateId {
            let body_start = self.pos;
            while let Some(b) = self.peek() {
                if matches!(b, b'\r' | b'\n' | b'{') {
                    break;
                }
                self.pos += 1;
            }
            let raw = &self.input[body_start..self.pos];
            let leading = raw.len() - raw.trim_start_matches(is_ecmascript_whitespace).len();
            let trailing = raw.trim_end_matches(is_ecmascript_whitespace).len();
            let id_start = body_start + leading;
            let id_end = body_start + trailing;
            let id = self.input[id_start..id_end].to_string();
            self.pop_mode(); // StateId
            self.pop_mode(); // State
            return Some(Ok((id_start, Tok::Id(id), id_end)));
        }

        if self.peek() == Some(b'"') {
            self.pos += 1;
            let body_start = self.pos;
            while let Some(b) = self.peek() {
                if b == b'"' {
                    break;
                }
                self.pos += 1;
            }
            if self.peek() != Some(b'"') {
                return Some(Err(LexError::new(
                    "Unterminated state description string; missing '\"'",
                )));
            }
            let body = self.input[body_start..self.pos].to_string();
            self.pos += 1;
            return Some(Ok((start, Tok::StateDescr(body), self.pos)));
        }

        if self.starts_with_word_ci("as") {
            self.pos += "as".len();
            self.push_mode(Mode::StateId);
            return Some(Ok((start, Tok::As, self.pos)));
        }

        // Fork/join/choice markers are recognized using the rest of the line.
        let Some(rel) = self.input.get(self.pos..) else {
            return Some(Err(LexError::new(
                "Internal lexer error: invalid UTF-8 boundary",
            )));
        };
        let eol = rel.find(['\r', '\n']).unwrap_or(rel.len());
        let line = &rel[..eol];
        let trimmed = line.trim().to_string();
        let lower = trimmed.to_ascii_lowercase();
        for marker in ["<<fork>>", "[[fork]]"] {
            if lower.ends_with(marker) {
                let base = trimmed[..trimmed.len() - marker.len()].trim().to_string();
                self.pos += eol;
                self.pop_mode();
                return Some(Ok((start, Tok::Fork(base), self.pos)));
            }
        }
        for marker in ["<<join>>", "[[join]]"] {
            if lower.ends_with(marker) {
                let base = trimmed[..trimmed.len() - marker.len()].trim().to_string();
                self.pos += eol;
                self.pop_mode();
                return Some(Ok((start, Tok::Join(base), self.pos)));
            }
        }
        for marker in ["<<choice>>", "[[choice]]"] {
            if lower.ends_with(marker) {
                let base = trimmed[..trimmed.len() - marker.len()].trim().to_string();
                self.pos += eol;
                self.pop_mode();
                return Some(Ok((start, Tok::Choice(base), self.pos)));
            }
        }

        // Otherwise treat it as a composite state ID: read only the identifier token.
        let Some(id) = self.read_plain_id() else {
            self.pop_mode();
            return Some(Err(LexError::new("Expected a state id")));
        };
        self.pop_mode();

        // Mermaid accepts `state <id>` with a `{ ... }` block that starts on the next line:
        //
        //   state Foo
        //   {
        //     ...
        //   }
        //
        // Treat the intervening whitespace/newlines as insignificant and advance to the `{` so the
        // parser sees `CompositState` followed immediately by a `Block`.
        let end = self.pos;
        let mut look = self.pos;
        while let Some(ch) = self.input[look..].chars().next() {
            if !is_ecmascript_inline_whitespace(ch) {
                break;
            }
            look += ch.len_utf8();
        }
        let same_line_end = self.input[look..]
            .find(['\r', '\n'])
            .map(|rel| look + rel)
            .unwrap_or(self.input.len());
        let same_line_tail = &self.input[look..same_line_end];
        let tail_leading = same_line_tail
            .len()
            .saturating_sub(same_line_tail.trim_start().len());
        let same_line_value_start = look + tail_leading;
        let same_line_value = &self.input[same_line_value_start..same_line_end];
        if !same_line_value.starts_with('{')
            && let Some(brace_rel) = same_line_value.find('{')
        {
            let bad_start = same_line_value_start;
            let bad_end_untrimmed = same_line_value_start + brace_rel;
            let bad = &self.input[bad_start..bad_end_untrimmed];
            let bad_end = bad_start + bad.trim_end().len();
            return Some(Err(LexError::with_span(
                "State name must be a single word",
                crate::SourceSpan::new(bad_start, bad_end.max(bad_start)),
            )));
        }
        if consume_line_ending(self.input, look).is_some() {
            let mut scan = look;
            loop {
                if let Some(end) = consume_line_ending(self.input, scan) {
                    scan = end;
                    continue;
                }
                let Some(ch) = self.input[scan..].chars().next() else {
                    break;
                };
                if is_ecmascript_inline_whitespace(ch) {
                    scan += ch.len_utf8();
                    continue;
                }
                break;
            }
            if self.input.as_bytes().get(scan) == Some(&b'{') {
                self.pos = scan;
            }
        }

        Some(Ok((start, Tok::CompositState(id), end)))
    }

    fn lex_id(&mut self) -> Option<(usize, Tok, usize)> {
        let start = self.pos;
        let mut end = self.pos;
        while let Some(ch) = self.input[end..].chars().next() {
            if ch == ':' || ch == '\n' || is_ecmascript_whitespace(ch) || ch == '-' || ch == '{' {
                break;
            }
            end += ch.len_utf8();
        }
        if end == start {
            return None;
        }
        self.pos = end;
        Some((start, Tok::Id(self.input[start..end].to_string()), self.pos))
    }

    fn lex_descr(&mut self) -> Option<(usize, Tok, usize)> {
        let start = self.pos;
        if self.peek()? != b':' {
            return None;
        }
        self.pos += 1;
        let body_start = self.pos;
        while let Some(b) = self.peek() {
            if matches!(b, b'\r' | b'\n' | b';') {
                break;
            }
            self.pos += 1;
        }
        let body = self.input[body_start..self.pos].trim().to_string();
        Some((start, Tok::Descr(body), self.pos))
    }

    fn read_plain_id(&mut self) -> Option<String> {
        let start = self.pos;
        let mut end = self.pos;
        while let Some(ch) = self.input[end..].chars().next() {
            if ch == ':' || ch == '\n' || is_ecmascript_whitespace(ch) || ch == '-' || ch == '{' {
                break;
            }
            end += ch.len_utf8();
        }
        if end == start {
            return None;
        }
        self.pos = end;
        Some(self.input[start..end].to_string())
    }

    fn lex_styled_id(&mut self) -> Option<(usize, Tok, usize)> {
        let start = self.pos;

        if self.starts_with("[*]:::") {
            self.pos += "[*]:::".len();
            let class_id = self.read_plain_id()?;
            return Some((
                start,
                Tok::StyledId(("[*]".to_string(), class_id)),
                self.pos,
            ));
        }

        // Look ahead: <id>:::<classId>
        let save = self.pos;
        let Some(id) = self.read_plain_id() else {
            self.pos = save;
            return None;
        };
        if !self.starts_with(":::") {
            self.pos = save;
            return None;
        }
        self.pos += 3;
        let Some(class_id) = self.read_plain_id() else {
            self.pos = save;
            return None;
        };
        Some((start, Tok::StyledId((id, class_id)), self.pos))
    }
}

impl TokenScanner for Lexer<'_> {
    type Event = std::result::Result<(usize, Tok, usize), LexError>;

    fn scan_step(&mut self) -> ScanStep<Self::Event> {
        if let Some(item) = self.pending.pop_front() {
            return ScanStep::Emit(self.emit_token(item));
        }

        if self.pos >= self.input.len() {
            if self.emitted_eof_newline {
                return ScanStep::End;
            }
            self.emitted_eof_newline = true;
            return ScanStep::Emit(self.emit_token((self.pos, Tok::Newline, self.pos)));
        }
        if self.mode() == Mode::Default {
            // INITIAL keyword and quoted-string rules precede greedy directions.
            for (keyword, token) in [("click", Tok::Click), ("href", Tok::Href)] {
                if self.starts_with_ci(keyword)
                    && self
                        .input
                        .as_bytes()
                        .get(self.pos + keyword.len())
                        .is_none_or(|byte| !byte.is_ascii_alphanumeric() && *byte != b'_')
                {
                    let start = self.pos;
                    self.pos += keyword.len();
                    return ScanStep::Emit(self.emit_token((start, token, self.pos)));
                }
            }
            if self.peek() == Some(b'"')
                && self.input[self.pos + 1..].contains('"')
                && let Some(token) = self.lex_string_lit()
            {
                return ScanStep::Emit(self.emit_result(token));
            }
            if self.starts_with_ci("default")
                && self
                    .input
                    .as_bytes()
                    .get(self.pos + 7)
                    .is_none_or(|byte| !byte.is_ascii_alphanumeric() && *byte != b'_')
            {
                let start = self.pos;
                self.pos += 7;
                return ScanStep::Emit(Err(LexError::with_span(
                    "Unexpected reserved state keyword default",
                    crate::SourceSpan::new(start, self.pos),
                )));
            }
        }

        if self.mode() == Mode::Default
            && let Some(direction) = self.lex_direction()
        {
            return ScanStep::Emit(self.emit_result(direction));
        }

        // Whitespace is a lexer rule, not preprocessing: INITIAL directions see
        // its original cursor, while struct whitespace precedes directions.
        let before_whitespace = self.pos;
        self.skip_ws();
        if self.pos != before_whitespace {
            return ScanStep::Skip;
        }
        if let Some(nl) = self.lex_newline() {
            return ScanStep::Emit(self.emit_token(nl));
        }
        if self.skip_comment() {
            return ScanStep::Skip;
        }

        if self.mode() == Mode::StateId
            && let Some(tok) = self.lex_state_mode_token()
        {
            return ScanStep::Emit(self.emit_result(tok));
        }

        if let Some(sd) = self.lex_sd_header() {
            return ScanStep::Emit(self.emit_token(sd));
        }

        // In struct mode these explicit rules precede directions; in INITIAL they follow them.
        // Require the upstream whitespace suffix so punctuation does not change rule priority.
        if self.mode() == Mode::Struct
            && ["classDef", "class", "style", "state"]
                .into_iter()
                .any(|keyword| {
                    self.starts_with_ci(keyword)
                        && self.input[self.pos + keyword.len()..]
                            .chars()
                            .next()
                            .is_some_and(is_ecmascript_whitespace)
                })
            && let Some(token) = self.lex_stmt_line()
        {
            return ScanStep::Emit(self.emit_result(token));
        }

        if self.mode() == Mode::Struct
            && let Some(dir) = self.lex_direction()
        {
            return ScanStep::Emit(self.emit_result(dir));
        }

        if let Some(acc) = self.lex_accessibility() {
            return ScanStep::Emit(self.emit_result(acc));
        }
        if self.pos >= self.input.len() {
            return ScanStep::Skip;
        }

        if !matches!(self.mode(), Mode::State | Mode::StateId)
            && let Some(tok) = self.lex_stmt_line()
        {
            return ScanStep::Emit(self.emit_result(tok));
        }

        if self.mode() == Mode::State
            && let Some(tok) = self.lex_state_mode_token()
        {
            return ScanStep::Emit(self.emit_result(tok));
        }

        let start = self.pos;
        if self.starts_with("-->") {
            self.pos += 3;
            return ScanStep::Emit(self.emit_token((start, Tok::Arrow, self.pos)));
        }

        if self.mode() == Mode::Struct && self.starts_with("--") {
            self.pos += 2;
            return ScanStep::Emit(self.emit_token((start, Tok::Concurrent, self.pos)));
        }

        if self.starts_with("[*]") && !self.starts_with("[*]:::") {
            self.pos += 3;
            return ScanStep::Emit(self.emit_token((start, Tok::EdgeState, self.pos)));
        }

        if self.peek() == Some(b'{') {
            self.pos += 1;
            if self.mode() == Mode::State {
                self.pop_mode();
            }
            self.push_mode(Mode::Struct);
            return ScanStep::Emit(self.emit_token((start, Tok::StructStart, self.pos)));
        }

        if self.peek() == Some(b'}') {
            self.pos += 1;
            if self.mode() == Mode::Struct {
                self.pop_mode();
            }
            return ScanStep::Emit(self.emit_token((start, Tok::StructStop, self.pos)));
        }

        if let Some(tok) = self.lex_string_lit() {
            return ScanStep::Emit(self.emit_result(tok));
        }

        if let Some(tok) = self.lex_styled_id() {
            return ScanStep::Emit(self.emit_token(tok));
        }

        if let Some(tok) = self.lex_descr() {
            return ScanStep::Emit(self.emit_token(tok));
        }

        if let Some(tok) = self.lex_id() {
            return ScanStep::Emit(self.emit_token(tok));
        }

        let bad = self
            .input
            .get(self.pos..)
            .and_then(|s| s.chars().next())
            .unwrap_or('?');
        self.pos += bad.len_utf8().max(1);
        ScanStep::Emit(Err(LexError::new(format!("Unexpected character '{bad}'"))))
    }
}

impl Iterator for Lexer<'_> {
    type Item = <Self as TokenScanner>::Event;

    fn next(&mut self) -> Option<Self::Item> {
        self.next_token()
    }
}

#[cfg(test)]
mod tests {
    use super::{Lexer, Tok};

    #[test]
    fn missing_state_id_leaves_the_error_state_before_recovery() {
        let events: Vec<_> = Lexer::new("stateDiagram-v2\nstate {\nA --> B\n}\n")
            .take(32)
            .collect();
        assert!(
            events.len() < 32,
            "recovery must leave the missing-id state"
        );
        assert_eq!(events.iter().filter(|event| event.is_err()).count(), 1);
        assert!(
            events
                .iter()
                .any(|event| { matches!(event, Ok((_, Tok::Id(id), _)) if id == "B") })
        );
    }

    #[test]
    fn state_direction_rules_match_only_complete_statements_and_preserve_value_spans() {
        for (source, expected, offset) in [
            ("direction\tLR", "LR", 10),
            ("prefixdirection rlignored", "RL", 16),
            ("direction LR direction TB", "TB", 23),
            ("direction RL\r direction TB", "RL", 10),
            ("direction RL\u{2028} direction TB", "RL", 10),
            ("direction\nBT", "BT", 10),
        ] {
            let (start, token, end) = Lexer::new(source).next().unwrap().unwrap();
            let Tok::Direction(direction) = token else {
                panic!("expected direction for {source:?}")
            };
            assert_eq!(direction.value, expected, "{source:?}");
            assert_eq!(
                direction.selection,
                crate::SourceSpan::new(offset, offset + 2),
                "{source:?}"
            );
            assert_eq!((start, end), (0, source.len()), "{source:?}");
        }
        for source in [
            "direction",
            "direction --> X",
            "direction_name",
            "direction XX",
        ] {
            assert!(
                matches!(Lexer::new(source).next().unwrap().unwrap().1, Tok::Id(_)),
                "{source}"
            );
        }
    }

    #[test]
    fn state_descriptions_preserve_following_colons() {
        let input = "stateDiagram-v2\nmyState : status: active\n";
        let descriptions: Vec<_> = Lexer::new(input)
            .map(|event| event.expect("state token").1)
            .filter_map(|token| match token {
                Tok::Descr(description) => Some(description),
                _ => None,
            })
            .collect();

        assert_eq!(descriptions, vec!["status: active"]);
    }
}
