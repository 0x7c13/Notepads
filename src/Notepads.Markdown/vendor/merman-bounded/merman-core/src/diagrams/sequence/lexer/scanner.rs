//! Mutable lexical state machine for Sequence diagrams.
//!
//! The parent module owns the stable token iterator; this module owns mode transitions, scanning,
//! and token-state transitions as one atomic protocol.

use super::{
    LexError, Tok,
    actor::{
        ActorBoundary, config_followed_by_alias, is_ecmascript_whitespace, scan_actor,
        signal_type_at, trim_ecmascript, trim_end_ecmascript,
    },
};
use crate::{SourceSpan, diagrams::scan::ScanStep};
use std::collections::VecDeque;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Mode {
    Default,
    Line,
    AccDescrMultiline,
}

pub(super) struct SequenceScanner<'input> {
    input: &'input str,
    pos: usize,
    pending: VecDeque<(usize, Tok, usize)>,
    mode: Mode,
    // Keywords are also legal participant ids in Mermaid. The boundary records both that parser
    // context must win over keyword lexing and where this particular actor id ends.
    actor_boundary: Option<ActorBoundary>,
    declaration_alias_pending: bool,
    declaration_config_allowed: bool,
    after_signal_type: bool,
}

impl<'input> SequenceScanner<'input> {
    pub(super) fn new(input: &'input str) -> Self {
        Self {
            input,
            pos: 0,
            pending: VecDeque::new(),
            mode: Mode::Default,
            actor_boundary: None,
            declaration_alias_pending: false,
            declaration_config_allowed: false,
            after_signal_type: false,
        }
    }

    fn peek(&self) -> Option<u8> {
        self.input.as_bytes().get(self.pos).copied()
    }

    fn peek2(&self) -> Option<[u8; 2]> {
        if self.pos + 1 >= self.input.len() {
            return None;
        }
        Some([
            self.input.as_bytes()[self.pos],
            self.input.as_bytes()[self.pos + 1],
        ])
    }

    fn bump(&mut self) -> Option<u8> {
        if self.pos >= self.input.len() {
            return None;
        }
        let bytes = self.input.as_bytes();
        let b = bytes[self.pos];

        // Keep `self.pos` on a UTF-8 char boundary. Mermaid input can contain arbitrary Unicode
        // (including `encodeEntities(...)` placeholders), and this lexer is otherwise byte-based.
        if b.is_ascii() {
            self.pos += 1;
        } else {
            // If we're already in the middle of a codepoint (continuation byte), resync by
            // skipping continuation bytes.
            if (b & 0b1100_0000) == 0b1000_0000 {
                self.pos += 1;
                while self.pos < bytes.len() && (bytes[self.pos] & 0b1100_0000) == 0b1000_0000 {
                    self.pos += 1;
                }
            } else {
                let len = if (b & 0b1110_0000) == 0b1100_0000 {
                    2
                } else if (b & 0b1111_0000) == 0b1110_0000 {
                    3
                } else if (b & 0b1111_1000) == 0b1111_0000 {
                    4
                } else {
                    1
                };
                self.pos = (self.pos + len).min(bytes.len());
                while self.pos < bytes.len() && (bytes[self.pos] & 0b1100_0000) == 0b1000_0000 {
                    self.pos += 1;
                }
            }
        }
        Some(b)
    }

    fn skip_ws(&mut self) {
        while let Some(ch) = self.input[self.pos..].chars().next() {
            if ch == '\n' || !is_ecmascript_whitespace(ch) {
                break;
            }
            self.pos += ch.len_utf8();
        }
    }

    fn starts_with_ci(&self, kw: &str) -> bool {
        let rest = self.input.as_bytes().get(self.pos..).unwrap_or_default();
        let kwb = kw.as_bytes();
        if rest.len() < kwb.len() {
            return false;
        }
        for i in 0..kwb.len() {
            let a = rest[i];
            let b = kwb[i];
            if !a.eq_ignore_ascii_case(&b) {
                return false;
            }
        }
        true
    }

    fn starts_with_ci_word(&self, kw: &str) -> bool {
        if !self.starts_with_ci(kw) {
            return false;
        }
        let after = self.pos + kw.len();
        if after >= self.input.len() {
            return true;
        }
        let b = self.input.as_bytes()[after];
        !b.is_ascii_alphanumeric() && b != b'_'
    }

    fn lex_newline(&mut self) -> Option<(usize, Tok, usize)> {
        let start = self.pos;
        match self.peek()? {
            b'\n' | b';' => {
                self.pos += 1;
                self.mode = Mode::Default;
                self.actor_boundary = None;
                self.after_signal_type = false;
                Some((start, Tok::Newline, self.pos))
            }
            _ => None,
        }
    }

    fn lex_comment(&mut self) -> bool {
        let Some(b) = self.peek() else {
            return false;
        };

        let initial_percent_comment =
            if self.mode != Mode::Default || self.forced_actor_uses_id_rules() {
                false
            } else if matches!(self.peek2(), Some([b'%', b'%'])) {
                true
            } else {
                self.input[self.pos..].chars().next().is_some_and(|first| {
                    first != '}'
                        && first != '\n'
                        && first.len_utf16() == 1
                        && self.input[self.pos + first.len_utf8()..].starts_with("%%")
                })
            };
        if b != b'#' && !initial_percent_comment {
            return false;
        }

        // Consume the recognized prefix before scanning its body. Jison's NEWLINE rule precedes
        // the permissive [^}]%% rule; a comment must never begin on that newline.
        self.pos += if b == b'#' {
            1
        } else if matches!(self.peek2(), Some([b'%', b'%'])) {
            2
        } else {
            self.input[self.pos..].chars().next().unwrap().len_utf8() + 2
        };
        while let Some(b2) = self.peek() {
            if b2 == b'\n' {
                break;
            }
            self.pos += 1;
        }
        true
    }

    fn lex_multiline_acc_descr(&mut self) -> Option<(usize, Tok, usize)> {
        if self.mode != Mode::AccDescrMultiline {
            return None;
        }
        let start = self.pos;
        let Some(rel_end) = self.input[self.pos..].find('}') else {
            self.pos = self.input.len();
            self.mode = Mode::Default;
            // The pinned Jison lexer reaches EOF in its exclusive accessibility state without
            // returning `acc_descr_multiline_value`; the incomplete directive is therefore
            // ignored semantically while consuming the remainder of the document.
            return None;
        };
        let end = self.pos + rel_end;
        let s = self.input[self.pos..end].to_string();
        self.pos = end + 1;
        self.mode = Mode::Default;
        // In the upstream Jison grammar the multiline value is a complete statement and the
        // closing brace returns the lexer to INITIAL, where a same-line statement may begin.
        // The local grammar uses Newline as its statement boundary, so preserve that token-level
        // contract only when no physical line boundary will provide it.
        if self.multiline_acc_descr_needs_boundary() {
            self.pending.push_back((self.pos, Tok::Newline, self.pos));
        }
        Some((start, Tok::AccDescrMultiline(s), self.pos))
    }

    fn multiline_acc_descr_needs_boundary(&self) -> bool {
        let bytes = self.input.as_bytes();
        let mut cursor = self.pos;
        while matches!(bytes.get(cursor), Some(b' ' | b'\t')) {
            cursor += 1;
        }

        match bytes.get(cursor).copied() {
            None => true,
            Some(b'\n' | b';') => false,
            Some(b'\r') if bytes.get(cursor + 1).is_none_or(|next| *next == b'\n') => false,
            Some(b'#') => self.comment_reaches_eof(cursor),
            Some(b'%') if bytes.get(cursor + 1) == Some(&b'%') => self.comment_reaches_eof(cursor),
            Some(_) => true,
        }
    }

    fn comment_reaches_eof(&self, start: usize) -> bool {
        !self.input.as_bytes()[start..]
            .iter()
            .any(|byte| matches!(byte, b'\r' | b'\n'))
    }

    fn lex_keyword_lines(&mut self) -> Option<(usize, Tok, usize)> {
        let start = self.pos;

        if self.starts_with_ci_word("title:") {
            self.pos += "title:".len();
            self.skip_ws();
            let s = self.read_to_line_end();
            return Some((
                start,
                Tok::CompatTitle(trim_ecmascript(&s).to_string()),
                self.pos,
            ));
        }

        if self.starts_with_ci_word("title") {
            let after = self.pos + "title".len();
            if after < self.input.len() && self.char_at_is_inline_whitespace(after) {
                self.pos = after;
                self.skip_ws();
                let s = self.read_to_line_end();
                return Some((start, Tok::Title(trim_ecmascript(&s).to_string()), self.pos));
            }
        }

        if self.starts_with_ci_word("accTitle") {
            let after = self.pos + "accTitle".len();
            let rest = &self.input[after..];
            let colon_pos = rest.find(':')?;
            if rest[..colon_pos].chars().any(|c| c == '\n' || c == ';') {
                return None;
            }
            let colon = after + colon_pos;
            self.pos = colon + 1;
            self.skip_ws();
            let s = self.read_to_line_end();
            return Some((
                start,
                Tok::AccTitle(trim_ecmascript(&s).to_string()),
                self.pos,
            ));
        }

        if self.starts_with_ci_word("accDescr") {
            let after = self.pos + "accDescr".len();
            let rest = &self.input[after..];
            let non_ws = rest.find(|c: char| !is_ecmascript_whitespace(c))?;
            match rest[non_ws..].chars().next() {
                Some(':') => {
                    let colon = after + non_ws;
                    self.pos = colon + 1;
                    self.skip_ws();
                    let s = self.read_to_line_end();
                    return Some((
                        start,
                        Tok::AccDescr(trim_ecmascript(&s).to_string()),
                        self.pos,
                    ));
                }
                Some('{') => {
                    let opening = after + non_ws;
                    self.pos = opening + 1;
                    self.mode = Mode::AccDescrMultiline;
                    return self.lex_multiline_acc_descr();
                }
                _ => {}
            }
        }

        None
    }

    fn read_to_line_end(&mut self) -> String {
        let start = self.pos;
        while let Some(b) = self.peek() {
            if b == b'\n' || b == b';' {
                break;
            }
            if b == b'#' {
                break;
            }
            self.pos += 1;
        }
        self.input[start..self.pos].to_string()
    }

    fn lex_word_keywords(&mut self) -> Option<(usize, Tok, usize)> {
        let start = self.pos;
        if self.starts_with_ci_word("sequenceDiagram") {
            self.pos += "sequenceDiagram".len();
            return Some((start, Tok::SequenceDiagram, self.pos));
        }
        if self.starts_with_ci_word("participant") {
            self.pos += "participant".len();
            return Some((start, Tok::Participant, self.pos));
        }
        if self.starts_with_ci_word("actor") {
            self.pos += "actor".len();
            return Some((start, Tok::ActorKw, self.pos));
        }
        if self.starts_with_ci_word("box") {
            self.pos += "box".len();
            self.mode = Mode::Line;
            return Some((start, Tok::Box, self.pos));
        }
        if self.starts_with_ci_word("end") {
            self.pos += "end".len();
            return Some((start, Tok::End, self.pos));
        }
        if self.starts_with_ci_word("loop") {
            self.pos += "loop".len();
            self.mode = Mode::Line;
            return Some((start, Tok::Loop, self.pos));
        }
        if self.starts_with_ci_word("rect") {
            self.pos += "rect".len();
            self.mode = Mode::Line;
            return Some((start, Tok::Rect, self.pos));
        }
        if self.starts_with_ci_word("opt") {
            self.pos += "opt".len();
            self.mode = Mode::Line;
            return Some((start, Tok::Opt, self.pos));
        }
        if self.starts_with_ci_word("alt") {
            self.pos += "alt".len();
            self.mode = Mode::Line;
            return Some((start, Tok::Alt, self.pos));
        }
        if self.starts_with_ci_word("else") {
            self.pos += "else".len();
            self.mode = Mode::Line;
            return Some((start, Tok::Else, self.pos));
        }
        if self.starts_with_ci_word("par_over") {
            self.pos += "par_over".len();
            self.mode = Mode::Line;
            return Some((start, Tok::ParOver, self.pos));
        }
        if self.starts_with_ci_word("par") {
            self.pos += "par".len();
            self.mode = Mode::Line;
            return Some((start, Tok::Par, self.pos));
        }
        if self.starts_with_ci_word("and") {
            self.pos += "and".len();
            self.mode = Mode::Line;
            return Some((start, Tok::And, self.pos));
        }
        if self.starts_with_ci_word("critical") {
            self.pos += "critical".len();
            self.mode = Mode::Line;
            return Some((start, Tok::Critical, self.pos));
        }
        if self.starts_with_ci_word("option") {
            self.pos += "option".len();
            self.mode = Mode::Line;
            return Some((start, Tok::Option, self.pos));
        }
        if self.starts_with_ci_word("break") {
            self.pos += "break".len();
            self.mode = Mode::Line;
            return Some((start, Tok::Break, self.pos));
        }
        if self.starts_with_ci_word("create") {
            self.pos += "create".len();
            return Some((start, Tok::Create, self.pos));
        }
        if self.starts_with_ci_word("destroy") {
            self.pos += "destroy".len();
            return Some((start, Tok::Destroy, self.pos));
        }
        if self.declaration_alias_pending && self.starts_with_ci_word("as") {
            self.pos += "as".len();
            self.mode = Mode::Line;
            return Some((start, Tok::As, self.pos));
        }
        if self.starts_with_ci_word("note") {
            self.pos += "note".len();
            return Some((start, Tok::Note, self.pos));
        }

        if self.starts_menu_statement("links") {
            self.pos += "links".len();
            return Some((start, Tok::Links, self.pos));
        }
        if self.starts_menu_statement("link") {
            self.pos += "link".len();
            return Some((start, Tok::Link, self.pos));
        }
        if self.starts_menu_statement("properties") {
            self.pos += "properties".len();
            return Some((start, Tok::Properties, self.pos));
        }
        if self.starts_menu_statement("details") {
            self.pos += "details".len();
            return Some((start, Tok::Details, self.pos));
        }

        if self.starts_with_ci("left of") {
            let after = self.pos + "left of".len();
            self.pos = after;
            return Some((start, Tok::LeftOf, self.pos));
        }
        if self.starts_with_ci("right of") {
            let after = self.pos + "right of".len();
            self.pos = after;
            return Some((start, Tok::RightOf, self.pos));
        }
        if self.starts_with_ci_word("over") {
            self.pos += "over".len();
            return Some((start, Tok::Over, self.pos));
        }

        if self.starts_with_ci_word("autonumber") {
            self.pos += "autonumber".len();
            return Some((start, Tok::Autonumber, self.pos));
        }
        if self.starts_with_ci_word("off") {
            self.pos += "off".len();
            return Some((start, Tok::Off, self.pos));
        }
        if self.starts_with_ci_word("activate") {
            self.pos += "activate".len();
            return Some((start, Tok::Activate, self.pos));
        }
        if self.starts_with_ci_word("deactivate") {
            self.pos += "deactivate".len();
            return Some((start, Tok::Deactivate, self.pos));
        }

        None
    }

    fn lex_punct(&mut self) -> Option<(usize, Tok, usize)> {
        let start = self.pos;
        match self.peek()? {
            b',' => {
                self.pos += 1;
                Some((start, Tok::Comma, self.pos))
            }
            b'+' => {
                self.pos += 1;
                Some((start, Tok::Plus, self.pos))
            }
            _ => None,
        }
    }

    fn lex_central_marker(&mut self) -> Option<(usize, Tok, usize)> {
        let start = self.pos;
        if self.input[self.pos..].starts_with("()") {
            self.pos += 2;
            return Some((start, Tok::Central, self.pos));
        }
        None
    }

    fn lex_signal_type(&mut self) -> Option<(usize, Tok, usize)> {
        let start = self.pos;
        let (len, ty) = signal_type_at(self.input, self.pos)?;

        self.pos += len;
        Some((start, Tok::SignalType(ty), self.pos))
    }

    fn lex_minus(&mut self) -> Option<(usize, Tok, usize)> {
        let start = self.pos;
        if self.peek()? != b'-' {
            return None;
        }
        self.pos += 1;
        Some((start, Tok::Minus, self.pos))
    }

    fn lex_num(&mut self) -> Option<(usize, Tok, usize)> {
        let start = self.pos;
        let mut end = self.pos;
        let mut saw_digit = false;
        while let Some(b) = self.input.as_bytes().get(end) {
            if b.is_ascii_digit() {
                end += 1;
                saw_digit = true;
                continue;
            }
            break;
        }

        if self.input.as_bytes().get(end) == Some(&b'.') {
            let decimal_start = end + 1;
            let mut decimal_end = decimal_start;
            while let Some(b) = self.input.as_bytes().get(decimal_end) {
                if b.is_ascii_digit() {
                    decimal_end += 1;
                    continue;
                }
                break;
            }
            let decimal_places = decimal_end - decimal_start;
            if decimal_places == 0 || decimal_places > 2 {
                return None;
            }
            end = decimal_end;
        } else if !saw_digit {
            return None;
        }

        let n: f64 = self.input[start..end].parse().ok()?;
        if !n.is_finite() {
            return None;
        }
        self.pos = end;
        Some((start, Tok::Num(n), self.pos))
    }

    fn lex_rest_of_line(&mut self) -> Option<(usize, Tok, usize)> {
        if self.mode != Mode::Line {
            return None;
        }
        let start = self.pos;
        let s = self.read_to_line_end();
        self.mode = Mode::Default;
        Some((
            start,
            Tok::RestOfLine(trim_ecmascript(&s).to_string()),
            self.pos,
        ))
    }

    fn lex_config(&mut self) -> Option<std::result::Result<(usize, Tok, usize), LexError>> {
        let start = self.pos;
        if !self.input[self.pos..].starts_with("@{") {
            return None;
        }
        if !self.declaration_config_allowed {
            self.pos += 2;
            return Some(Err(LexError {
                message: "Config objects require a whitespace-free actor id".to_string(),
                span: SourceSpan::new(start, self.pos),
            }));
        }
        self.declaration_config_allowed = false;
        self.pos += 2;
        let Some(rel_end) = self.input[self.pos..].find('}') else {
            self.pos = self.input.len();
            return Some(Err(LexError {
                message: "Unterminated config object; missing '}'".to_string(),
                span: SourceSpan::new(start, self.input.len()),
            }));
        };
        let end = self.pos + rel_end;
        let s = trim_ecmascript(&self.input[self.pos..end]).to_string();
        self.pos = end + 1;
        self.declaration_alias_pending = config_followed_by_alias(self.input, self.pos);
        Some(Ok((start, Tok::Config(s), self.pos)))
    }

    fn lex_text(&mut self) -> Option<(usize, Tok, usize)> {
        let start = self.pos;
        if self.peek()? != b':' {
            return None;
        }
        self.pos += 1;
        let s = self.read_to_line_end();
        Some((start, Tok::Text(trim_ecmascript(&s).to_string()), self.pos))
    }

    fn lex_actor_with_boundary(&mut self, boundary: ActorBoundary) -> Option<(usize, Tok, usize)> {
        let start = self.pos;
        let scan = scan_actor(self.input, start, boundary)?;
        let actor = scan.text.to_string();
        self.pos = scan.scan_end;
        if boundary == ActorBoundary::Declaration {
            self.declaration_alias_pending = true;
            self.declaration_config_allowed = scan.config_allowed;
        }
        Some((start, Tok::Actor(actor), scan.token_end))
    }

    fn char_at_is_inline_whitespace(&self, position: usize) -> bool {
        self.input
            .get(position..)
            .and_then(|rest| rest.chars().next())
            .is_some_and(|ch| ch != '\n' && is_ecmascript_whitespace(ch))
    }

    fn char_at_is_ecmascript_whitespace(&self, position: usize) -> bool {
        self.input
            .get(position..)
            .and_then(|rest| rest.chars().next())
            .is_some_and(is_ecmascript_whitespace)
    }

    fn starts_with_ci_at(&self, position: usize, expected: &str) -> bool {
        let Some(actual) = self
            .input
            .as_bytes()
            .get(position..position.saturating_add(expected.len()))
        else {
            return false;
        };
        actual.eq_ignore_ascii_case(expected.as_bytes())
    }

    fn lex_actor(&mut self) -> Option<(usize, Tok, usize)> {
        self.lex_actor_with_boundary(ActorBoundary::StatementEnd)
    }

    fn lex_forced_actor_id(&mut self) -> Option<(usize, Tok, usize)> {
        let boundary = self.actor_boundary?;
        self.lex_actor_with_boundary(boundary)
    }

    fn lex_actor_before_signal(&mut self) -> Option<(usize, Tok, usize)> {
        if self.initial_keyword_precedes_actor() {
            return None;
        }
        let saved = self.pos;
        let tok = self.lex_actor_with_boundary(ActorBoundary::SignalSource)?;
        let after_actor = self.pos;

        if let Tok::Actor(actor) = &tok.1
            && let Some(stripped) = actor.strip_suffix("()")
            && self.peek_signal_type_at(after_actor)
        {
            let central_start = tok.2.checked_sub(2)?;
            self.pending.push_back((central_start, Tok::Central, tok.2));
            let actor_end = trim_end_ecmascript(&self.input[tok.0..central_start]).len() + tok.0;
            return Some((
                tok.0,
                Tok::Actor(trim_end_ecmascript(stripped).to_string()),
                actor_end,
            ));
        }

        if self.peek_signal_type_at_current() {
            Some(tok)
        } else {
            self.pos = saved;
            None
        }
    }

    fn initial_keyword_precedes_actor(&self) -> bool {
        if self.declaration_alias_pending
            && self.starts_with_ci_at(self.pos, "as")
            && self.char_at_is_ecmascript_whitespace(self.pos + "as".len())
        {
            return true;
        }

        const KEYWORDS: [&str; 24] = [
            "sequenceDiagram",
            "participant",
            "actor",
            "create",
            "destroy",
            "box",
            "loop",
            "rect",
            "opt",
            "alt",
            "else",
            "par",
            "par_over",
            "and",
            "critical",
            "option",
            "break",
            "end",
            "note",
            "autonumber",
            "off",
            "activate",
            "deactivate",
            "over",
        ];
        KEYWORDS
            .iter()
            .any(|keyword| self.starts_with_ci_word(keyword))
            || ["links", "link", "properties", "details"]
                .iter()
                .any(|keyword| self.starts_menu_statement(keyword))
            || self.starts_initial_relative_note_keyword("left of")
            || self.starts_initial_relative_note_keyword("right of")
    }

    fn starts_menu_statement(&self, keyword: &str) -> bool {
        if !self.starts_with_ci(keyword) {
            return false;
        }
        let mut cursor = self.pos + keyword.len();
        let start = cursor;
        while matches!(self.input.as_bytes().get(cursor), Some(b' ' | b'\t')) {
            cursor += 1;
        }
        cursor > start
            && self.input[cursor..].chars().next().is_some_and(|ch| {
                !is_ecmascript_whitespace(ch)
                    && !matches!(
                        ch,
                        '/' | '\\' | '+' | '(' | ')' | '<' | '>' | ':' | ',' | ';' | '-'
                    )
            })
    }

    fn starts_initial_relative_note_keyword(&self, keyword: &str) -> bool {
        self.starts_with_ci_at(self.pos, keyword)
    }

    fn forced_actor_uses_id_rules(&self) -> bool {
        matches!(
            self.actor_boundary,
            Some(ActorBoundary::Declaration | ActorBoundary::StatementEnd)
        )
    }

    fn peek_signal_type_at_current(&self) -> bool {
        self.peek_signal_type_at(self.pos)
    }

    fn peek_signal_type_at(&self, pos: usize) -> bool {
        signal_type_at(self.input, pos).is_some()
    }

    fn note_emitted_token(&mut self, tok: &Tok) {
        match tok {
            Tok::Newline | Tok::Participant | Tok::ActorKw | Tok::As => {
                self.declaration_alias_pending = false;
                self.declaration_config_allowed = false;
            }
            Tok::Config(_) => self.declaration_config_allowed = false,
            _ => {}
        }
        self.after_signal_type = matches!(tok, Tok::SignalType(_));
        self.actor_boundary = match tok {
            Tok::Participant | Tok::ActorKw => Some(ActorBoundary::Declaration),
            Tok::Destroy | Tok::Activate | Tok::Deactivate => Some(ActorBoundary::StatementEnd),
            Tok::Links
            | Tok::Link
            | Tok::Properties
            | Tok::Details
            | Tok::SignalType(_)
            | Tok::Plus
            | Tok::Minus
            | Tok::Central => Some(ActorBoundary::Text),
            Tok::LeftOf | Tok::RightOf | Tok::Over | Tok::Comma => Some(ActorBoundary::TextOrComma),
            _ => None,
        };
    }

    fn emit(
        &mut self,
        tok: (usize, Tok, usize),
    ) -> ScanStep<std::result::Result<(usize, Tok, usize), LexError>> {
        self.note_emitted_token(&tok.1);
        ScanStep::Emit(Ok(tok))
    }

    fn emit_result(
        &mut self,
        token: std::result::Result<(usize, Tok, usize), LexError>,
    ) -> ScanStep<std::result::Result<(usize, Tok, usize), LexError>> {
        if let Ok((_, token, _)) = &token {
            self.note_emitted_token(token);
        } else {
            self.actor_boundary = None;
            self.declaration_alias_pending = false;
            self.declaration_config_allowed = false;
        }
        ScanStep::Emit(token)
    }
}

impl<'input> SequenceScanner<'input> {
    pub(super) fn scan_step(
        &mut self,
    ) -> ScanStep<std::result::Result<(usize, Tok, usize), LexError>> {
        if let Some(tok) = self.pending.pop_front() {
            return self.emit(tok);
        }

        let start = self.pos;
        self.skip_ws();

        if self.pos >= self.input.len() {
            return ScanStep::End;
        }

        if self.lex_comment() {
            return ScanStep::Skip;
        }

        if let Some(tok) = self.lex_multiline_acc_descr() {
            return self.emit(tok);
        }

        if let Some(tok) = self.lex_rest_of_line() {
            return self.emit(tok);
        }

        if let Some(tok) = self.lex_newline() {
            return self.emit(tok);
        }

        if self.forced_actor_uses_id_rules()
            && let Some(tok) = self.lex_forced_actor_id()
        {
            return self.emit(tok);
        }

        if let Some(tok) = self.lex_keyword_lines() {
            return self.emit(tok);
        }
        if self.pos >= self.input.len() {
            return ScanStep::End;
        }

        if self.actor_boundary.is_some()
            && self.initial_keyword_precedes_actor()
            && let Some(tok) = self.lex_word_keywords()
        {
            return self.emit(tok);
        }

        if self.actor_boundary.is_some()
            && let Some(tok) = self.lex_forced_actor_id()
        {
            return self.emit(tok);
        }

        if self.after_signal_type {
            if let Some(tok) = self.lex_central_marker() {
                return self.emit(tok);
            }
            if let Some(tok) = self.lex_punct() {
                return self.emit(tok);
            }
            if let Some(tok) = self.lex_minus() {
                return self.emit(tok);
            }
            if let Some(tok) = self.lex_forced_actor_id() {
                return self.emit(tok);
            }
        }

        if let Some(tok) = self.lex_central_marker() {
            return self.emit(tok);
        }

        if let Some(tok) = self.lex_actor_before_signal() {
            return self.emit(tok);
        }

        if let Some(tok) = self.lex_word_keywords() {
            return self.emit(tok);
        }

        if let Some(tok) = self.lex_signal_type() {
            return self.emit(tok);
        }

        if let Some(tok) = self.lex_config() {
            return self.emit_result(tok);
        }

        if let Some(tok) = self.lex_text() {
            return self.emit(tok);
        }

        if let Some(tok) = self.lex_num() {
            return self.emit(tok);
        }

        if let Some(tok) = self.lex_punct() {
            return self.emit(tok);
        }

        if let Some(tok) = self.lex_central_marker() {
            return self.emit(tok);
        }

        if let Some(tok) = self.lex_minus() {
            return self.emit(tok);
        }

        if let Some(tok) = self.lex_actor() {
            return self.emit(tok);
        }

        let _ = self.bump();
        ScanStep::Emit(Err(LexError {
            message: format!("Unexpected character at {start}"),
            span: SourceSpan::new(start, self.pos),
        }))
    }
}
