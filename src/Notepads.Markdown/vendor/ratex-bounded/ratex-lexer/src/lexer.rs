// Notepads Markdown execution-control modifications: Copyright (c) 2026, Jiaqi (0x7c13) Liu.
use crate::token::{SourceLocation, Token};
use ratex_types::execution::{ExecutionControl, Phase, Resource};

/// A token scanned without allocating its text. Normalized whitespace borrows
/// static text; other non-EOF spans borrow the lexer's original input.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TokenSpan<'a> {
    pub text: &'a str,
    pub loc: SourceLocation,
    pub is_eof: bool,
}
impl<'a> TokenSpan<'a> {
    fn new(text: &'a str, start: usize, end: usize) -> Self {
        Self {
            text,
            loc: SourceLocation { start, end },
            is_eof: false,
        }
    }
    fn eof(pos: usize) -> Self {
        Self {
            text: "EOF",
            loc: SourceLocation {
                start: pos,
                end: pos,
            },
            is_eof: true,
        }
    }
    /// Allocate an owned token after the caller has checked its resource limits.
    pub fn into_token(self) -> Token {
        Token::new(self.text, self.loc.start, self.loc.end)
    }
}

/// Lexer for LaTeX math input.
///
/// Follows KaTeX's lexing strategy:
/// - Control words: `\` followed by one or more ASCII letters or `@`
/// - Control symbols: `\` followed by a single non-letter character
/// - Control space: `\` followed by whitespace
/// - Verb: `\verb*<delim>...<delim>` or `\verb<delim>...<delim>`
/// - Comments: `%` through end of line (skipped)
/// - Whitespace: collapsed to a single space token
/// - Regular characters: character + any trailing combining diacritical marks
pub struct Lexer<'a> {
    execution: Option<&'a ExecutionControl<'a>>,
    input: &'a str,
    bytes: &'a [u8],
    pos: usize,
    /// Category codes. Currently only comment (14) and active (13) are used.
    /// `%` defaults to catcode 14 (comment), `~` to catcode 13 (active).
    /// The MacroExpander may change catcodes (e.g., `\makeatletter` sets `@` to 13).
    catcodes: [(char, u8); 16],
    catcode_count: usize,
    legacy_catcode_overflow: Vec<(char, u8)>,
}

impl<'a> Lexer<'a> {
    pub fn new(input: &'a str) -> Self {
        Self {
            input,
            bytes: input.as_bytes(),
            pos: 0,
            execution: None,
            catcodes: {
                let mut values = [('\0', 0); 16];
                values[0] = ('%', 14);
                values[1] = ('~', 13);
                values
            },
            catcode_count: 2,
            legacy_catcode_overflow: Vec::new(),
        }
    }

    pub fn set_execution(&mut self, execution: &'a ExecutionControl<'a>) {
        self.execution = Some(execution);
    }

    fn checkpoint(&self) -> bool {
        self.execution
            .is_none_or(|control| control.checkpoint(Phase::Lexer).is_ok())
    }

    /// Set the category code for a character.
    /// Used by MacroExpander for commands like `\makeatletter`.
    pub fn set_catcode(&mut self, ch: char, code: u8) {
        for entry in &mut self.catcodes[..self.catcode_count] {
            if entry.0 == ch {
                entry.1 = code;
                return;
            }
        }
        if self.catcode_count < self.catcodes.len() {
            self.catcodes[self.catcode_count] = (ch, code);
            self.catcode_count += 1;
        } else if let Some(control) = self.execution.filter(|control| control.storage().is_some()) {
            control.fail(ratex_types::execution::ExecutionError::Unsupported {
                phase: Phase::Lexer,
            });
        } else if let Some(entry) = self
            .legacy_catcode_overflow
            .iter_mut()
            .find(|entry| entry.0 == ch)
        {
            entry.1 = code;
        } else {
            self.legacy_catcode_overflow.push((ch, code));
        }
    }

    /// Get the current position in the input.
    pub fn pos(&self) -> usize {
        self.pos
    }

    fn at_end(&self) -> bool {
        self.pos >= self.input.len()
    }

    fn peek_byte(&self) -> Option<u8> {
        self.bytes.get(self.pos).copied()
    }

    fn current_char(&self) -> Option<char> {
        self.input[self.pos..].chars().next()
    }

    fn advance_char(&mut self) -> Option<char> {
        let ch = self.input[self.pos..].chars().next()?;
        self.pos += ch.len_utf8();
        Some(ch)
    }

    fn catcode(&self, ch: char) -> u8 {
        self.get_catcode(ch)
    }

    /// Get the catcode for a character (public for MacroExpander).
    pub fn get_catcode(&self, ch: char) -> u8 {
        for &(c, code) in &self.catcodes[..self.catcode_count] {
            if c == ch {
                return code;
            }
        }
        self.legacy_catcode_overflow
            .iter()
            .find(|entry| entry.0 == ch)
            .map_or(0, |entry| entry.1)
    }

    fn is_whitespace(b: u8) -> bool {
        matches!(b, b' ' | b'\t' | b'\r' | b'\n')
    }

    /// KaTeX includes `@` in control word characters (`\\[a-zA-Z@]+`).
    fn is_control_word_char(b: u8) -> bool {
        b.is_ascii_alphabetic() || b == b'@'
    }

    fn is_combining_diacritical(ch: char) -> bool {
        ('\u{0300}'..='\u{036F}').contains(&ch)
    }

    fn skip_whitespace(&mut self) {
        while self.pos < self.bytes.len() && Self::is_whitespace(self.bytes[self.pos]) {
            if !self.checkpoint() {
                return;
            }
            self.pos += 1;
        }
    }

    /// Lex `\verb*<d>...<d>` or `\verb<d>...<d>` after `\verb` has been consumed.
    fn lex_verb(&mut self, start: usize, _: bool) -> TokenSpan<'a> {
        if self.at_end() {
            return TokenSpan::new("\\verb", start, self.pos);
        }
        // Check for \verb* variant
        let starred = self.peek_byte() == Some(b'*');
        if starred {
            self.pos += 1;
        }
        if self.at_end() {
            let text = &self.input[start..self.pos];
            return TokenSpan::new(text, start, self.pos);
        }
        // The next character is the delimiter
        let delim = self.advance_char().unwrap();
        // Scan until matching delimiter or end of input
        while let Some(ch) = self.advance_char() {
            if !self.checkpoint() {
                return TokenSpan::eof(self.pos);
            }
            if ch == delim {
                let text = &self.input[start..self.pos];
                return TokenSpan::new(text, start, self.pos);
            }
        }
        // Unterminated \verb — return what we have
        let text = &self.input[start..self.pos];
        TokenSpan::new(text, start, self.pos)
    }

    /// Lex a single token from the current position.
    pub fn next_span(&mut self) -> TokenSpan<'a> {
        if self
            .execution
            .is_some_and(|control| control.consume(Phase::Lexer, Resource::Tokens, 1).is_err())
        {
            return TokenSpan::eof(self.pos);
        }
        loop {
            if !self.checkpoint() {
                return TokenSpan::eof(self.pos);
            }
            // Skip whitespace, returning a space token if any was found
            if self.at_end() {
                return TokenSpan::eof(self.pos);
            }

            let start = self.pos;
            let ch = match self.current_char() {
                Some(c) => c,
                None => return TokenSpan::eof(self.pos),
            };

            // Whitespace → single space token
            if let Some(b) = self.peek_byte() {
                if Self::is_whitespace(b) {
                    self.skip_whitespace();
                    return TokenSpan::new(" ", start, self.pos);
                }
            }

            // Comment character (catcode 14, default `%`)
            if self.catcode(ch) == 14 {
                self.advance_char();
                // Skip to end of line
                while !self.at_end() {
                    if !self.checkpoint() {
                        return TokenSpan::eof(self.pos);
                    }
                    if self.peek_byte() == Some(b'\n') {
                        self.pos += 1;
                        break;
                    }
                    self.pos += 1;
                }
                continue;
            }

            // Backslash → control sequence
            if ch == '\\' {
                self.pos += 1; // consume the backslash
                if self.at_end() {
                    return TokenSpan::new("\\", start, self.pos);
                }

                let next_byte = self.bytes[self.pos];

                if Self::is_whitespace(next_byte) {
                    // Control space: \<whitespace> → "\\ " token, then skip trailing whitespace
                    self.pos += 1;
                    self.skip_whitespace();
                    return TokenSpan::new("\\ ", start, self.pos);
                }

                if Self::is_control_word_char(next_byte) {
                    // Control word: \<letters or @>
                    while self.pos < self.bytes.len()
                        && Self::is_control_word_char(self.bytes[self.pos])
                    {
                        if !self.checkpoint() {
                            return TokenSpan::eof(self.pos);
                        }
                        self.pos += 1;
                    }
                    let text = &self.input[start..self.pos];

                    // Handle \verb and \verb* — verbatim content delimited by next char
                    if text == "\\verb" {
                        return self.lex_verb(start, false);
                    }

                    let end = self.pos;
                    // Skip trailing whitespace after control word (KaTeX behavior)
                    self.skip_whitespace();
                    return TokenSpan::new(text, start, end);
                }

                // Control symbol: \<single non-letter char>
                let sym_char = self.current_char().unwrap();
                self.pos += sym_char.len_utf8();
                let text = &self.input[start..self.pos];
                return TokenSpan::new(text, start, self.pos);
            }

            // Active character (catcode 13, default `~`) → rendered as a space
            if self.catcode(ch) == 13 {
                self.advance_char();
                return TokenSpan::new(&self.input[start..self.pos], start, self.pos);
            }

            // Regular character (including {, }, ^, _, &, etc.)
            // Consume trailing combining diacritical marks (U+0300–U+036F) to form
            // a single token, matching KaTeX's regex behavior.
            self.advance_char();
            while let Some(next) = self.current_char() {
                if !self.checkpoint() {
                    return TokenSpan::eof(self.pos);
                }
                if Self::is_combining_diacritical(next) {
                    self.advance_char();
                } else {
                    break;
                }
            }
            return TokenSpan::new(&self.input[start..self.pos], start, self.pos);
        }
    }

    /// Lex a single owned token, preserving the original lexer API.
    pub fn lex(&mut self) -> Token {
        let span = self.next_span();
        if let Some(storage) = self.execution.and_then(ExecutionControl::storage) {
            let text = storage
                .try_string(Phase::Lexer, span.text)
                .unwrap_or_default();
            Token {
                is_builtin_literal: false,
                text,
                loc: span.loc,
                noexpand: false,
                treat_as_relax: false,
            }
        } else {
            span.into_token()
        }
    }

    /// Lex all remaining tokens until EOF.
    pub fn lex_all(&mut self) -> Vec<Token> {
        let mut tokens = Vec::new();
        loop {
            let tok = self.lex();
            if tok.is_eof() {
                tokens.push(tok);
                break;
            }
            tokens.push(tok);
        }
        tokens
    }
}

impl Iterator for Lexer<'_> {
    type Item = Token;

    fn next(&mut self) -> Option<Token> {
        let tok = self.lex();
        if tok.is_eof() { None } else { Some(tok) }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn lex_texts(input: &str) -> Vec<String> {
        let mut lexer = Lexer::new(input);
        lexer.lex_all().into_iter().map(|t| t.text).collect()
    }

    // === Basic character tokens ===

    #[test]
    fn test_single_letter() {
        assert_eq!(lex_texts("a"), vec!["a", "EOF"]);
    }

    #[test]
    fn test_multiple_letters() {
        assert_eq!(lex_texts("abc"), vec!["a", "b", "c", "EOF"]);
    }

    #[test]
    fn test_digit() {
        assert_eq!(lex_texts("123"), vec!["1", "2", "3", "EOF"]);
    }

    #[test]
    fn test_empty_input() {
        assert_eq!(lex_texts(""), vec!["EOF"]);
    }

    // === Control sequences ===

    #[test]
    fn test_control_word() {
        assert_eq!(lex_texts("\\frac"), vec!["\\frac", "EOF"]);
    }

    #[test]
    fn test_control_word_alpha() {
        assert_eq!(lex_texts("\\alpha"), vec!["\\alpha", "EOF"]);
    }

    #[test]
    fn test_control_word_followed_by_letter() {
        // \frac followed by letter: control word consumes trailing space then letter is separate
        assert_eq!(lex_texts("\\frac x"), vec!["\\frac", "x", "EOF"]);
    }

    #[test]
    fn test_control_word_no_space() {
        // \fracx → \fracx as one control word
        assert_eq!(lex_texts("\\fracx"), vec!["\\fracx", "EOF"]);
    }

    #[test]
    fn test_control_word_brace() {
        // \frac{} → \frac, {, }
        assert_eq!(lex_texts("\\frac{}"), vec!["\\frac", "{", "}", "EOF"]);
    }

    #[test]
    fn test_control_symbol() {
        // \, is a control symbol (comma is not a letter)
        assert_eq!(lex_texts("\\,"), vec!["\\,", "EOF"]);
    }

    #[test]
    fn test_control_symbol_semicolon() {
        assert_eq!(lex_texts("\\;"), vec!["\\;", "EOF"]);
    }

    #[test]
    fn test_control_space() {
        // \<space> → control space token "\\ "
        assert_eq!(lex_texts("\\ "), vec!["\\ ", "EOF"]);
    }

    #[test]
    fn test_double_backslash() {
        // \\ → control symbol "\"
        assert_eq!(lex_texts("\\\\"), vec!["\\\\", "EOF"]);
    }

    // === Whitespace ===

    #[test]
    fn test_whitespace_collapsed() {
        assert_eq!(lex_texts("a   b"), vec!["a", " ", "b", "EOF"]);
    }

    #[test]
    fn test_newline_as_space() {
        assert_eq!(lex_texts("a\nb"), vec!["a", " ", "b", "EOF"]);
    }

    #[test]
    fn test_tab_as_space() {
        assert_eq!(lex_texts("a\tb"), vec!["a", " ", "b", "EOF"]);
    }

    #[test]
    fn test_control_word_eats_trailing_space() {
        // In TeX/KaTeX, `\frac  x` → \frac token, then x (spaces consumed by control word)
        let tokens = lex_texts("\\frac  x");
        assert_eq!(tokens, vec!["\\frac", "x", "EOF"]);
    }

    // === Comments ===

    #[test]
    fn test_comment_to_eol() {
        assert_eq!(lex_texts("a%comment\nb"), vec!["a", "b", "EOF"]);
    }

    #[test]
    fn test_comment_at_end() {
        assert_eq!(lex_texts("a%comment"), vec!["a", "EOF"]);
    }

    #[test]
    fn test_comment_with_commands() {
        assert_eq!(
            lex_texts("\\alpha%skip\n\\beta"),
            vec!["\\alpha", "\\beta", "EOF"]
        );
    }

    // === Special characters ===

    #[test]
    fn test_braces() {
        assert_eq!(lex_texts("{x}"), vec!["{", "x", "}", "EOF"]);
    }

    #[test]
    fn test_caret_underscore() {
        assert_eq!(lex_texts("a^2_3"), vec!["a", "^", "2", "_", "3", "EOF"]);
    }

    #[test]
    fn test_ampersand() {
        assert_eq!(lex_texts("a&b"), vec!["a", "&", "b", "EOF"]);
    }

    #[test]
    fn test_tilde_as_active() {
        assert_eq!(lex_texts("a~b"), vec!["a", "~", "b", "EOF"]);
    }

    // === Complex expressions ===

    #[test]
    fn test_frac_expression() {
        let tokens = lex_texts("\\frac{a^2}{b}");
        assert_eq!(
            tokens,
            vec!["\\frac", "{", "a", "^", "2", "}", "{", "b", "}", "EOF"]
        );
    }

    #[test]
    fn test_sqrt_with_optional() {
        let tokens = lex_texts("\\sqrt[3]{x}");
        assert_eq!(tokens, vec!["\\sqrt", "[", "3", "]", "{", "x", "}", "EOF"]);
    }

    #[test]
    fn test_complex_fraction() {
        let tokens = lex_texts("\\frac{a + b}{c - d}");
        assert_eq!(
            tokens,
            vec![
                "\\frac", "{", "a", " ", "+", " ", "b", "}", "{", "c", " ", "-", " ", "d", "}",
                "EOF"
            ]
        );
    }

    #[test]
    fn test_sum_with_limits() {
        let tokens = lex_texts("\\sum_{i=0}^{n}");
        assert_eq!(
            tokens,
            vec![
                "\\sum", "_", "{", "i", "=", "0", "}", "^", "{", "n", "}", "EOF"
            ]
        );
    }

    #[test]
    fn test_matrix_row() {
        let tokens = lex_texts("a & b \\\\ c & d");
        assert_eq!(
            tokens,
            vec![
                "a", " ", "&", " ", "b", " ", "\\\\", " ", "c", " ", "&", " ", "d", "EOF"
            ]
        );
    }

    #[test]
    fn test_nested_frac() {
        let tokens = lex_texts("\\frac{\\sqrt{a^2+b^2}}{c}");
        assert_eq!(
            tokens,
            vec![
                "\\frac", "{", "\\sqrt", "{", "a", "^", "2", "+", "b", "^", "2", "}", "}", "{",
                "c", "}", "EOF"
            ]
        );
    }

    // === Unicode ===

    #[test]
    fn test_unicode_char() {
        let tokens = lex_texts("α");
        assert_eq!(tokens, vec!["α", "EOF"]);
    }

    #[test]
    fn test_unicode_mixed() {
        let tokens = lex_texts("x + α");
        assert_eq!(tokens, vec!["x", " ", "+", " ", "α", "EOF"]);
    }

    // === Edge cases ===

    #[test]
    fn test_backslash_at_end() {
        let tokens = lex_texts("\\");
        assert_eq!(tokens, vec!["\\", "EOF"]);
    }

    #[test]
    fn test_multiple_commands() {
        let tokens = lex_texts("\\alpha\\beta");
        assert_eq!(tokens, vec!["\\alpha", "\\beta", "EOF"]);
    }

    #[test]
    fn test_command_then_digit() {
        // \frac1 → \frac, 1 (digit is not a letter, stops control word)
        let tokens = lex_texts("\\frac1");
        assert_eq!(tokens, vec!["\\frac", "1", "EOF"]);
    }

    #[test]
    fn test_equals_sign() {
        let tokens = lex_texts("x = 1");
        assert_eq!(tokens, vec!["x", " ", "=", " ", "1", "EOF"]);
    }

    // === Source locations ===

    #[test]
    fn test_source_locations() {
        let mut lexer = Lexer::new("\\frac{a}");
        let t1 = lexer.lex(); // \frac
        assert_eq!(t1.text, "\\frac");
        assert_eq!(t1.loc.start, 0);
        assert_eq!(t1.loc.end, 5);

        let t2 = lexer.lex(); // {
        assert_eq!(t2.text, "{");
        assert_eq!(t2.loc.start, 5);

        let t3 = lexer.lex(); // a
        assert_eq!(t3.text, "a");
        assert_eq!(t3.loc.start, 6);

        let t4 = lexer.lex(); // }
        assert_eq!(t4.text, "}");
        assert_eq!(t4.loc.start, 7);
    }

    // === Iterator trait ===

    #[test]
    fn test_iterator() {
        let lexer = Lexer::new("a+b");
        let texts: Vec<String> = lexer.map(|t| t.text).collect();
        assert_eq!(texts, vec!["a", "+", "b"]);
    }

    // =========================================================================
    // KaTeX katex-spec.ts: "A parser" describe block — whitespace behavior
    // =========================================================================

    /// KaTeX: `it("should not fail on an empty string")`
    #[test]
    fn katex_empty_string() {
        assert_eq!(lex_texts(""), vec!["EOF"]);
    }

    /// KaTeX: `it("should ignore whitespace")` — ` x y ` parseLike `xy`
    /// At the lexer level: leading/trailing/middle spaces become space tokens.
    /// The parser ignores them; lexer must produce them correctly.
    #[test]
    fn katex_whitespace_around_and_between() {
        let tokens = lex_texts(" x y ");
        assert_eq!(tokens, vec![" ", "x", " ", "y", " ", "EOF"]);
    }

    /// KaTeX: `it("should ignore whitespace in atom")` — ` x ^ y ` parseLike `x^y`
    #[test]
    fn katex_whitespace_in_atom() {
        let tokens = lex_texts(" x ^ y ");
        assert_eq!(tokens, vec![" ", "x", " ", "^", " ", "y", " ", "EOF"]);
    }

    // =========================================================================
    // KaTeX katex-spec.ts: "A comment parser" describe block
    // All these tests correspond to the lexer's comment-handling behavior.
    // =========================================================================

    /// KaTeX: `it("should parse comments at the end of a line")`
    /// `"a^2 + b^2 = c^2 % Pythagoras' Theorem\n"`
    #[test]
    fn katex_comment_at_end_of_line() {
        let tokens = lex_texts("a^2 + b^2 = c^2 % Pythagoras' Theorem\n");
        // Comment and everything after % until \n is removed; \n consumed by comment
        assert_eq!(
            tokens,
            vec![
                "a", "^", "2", " ", "+", " ", "b", "^", "2", " ", "=", " ", "c", "^", "2", " ",
                "EOF"
            ]
        );
    }

    /// KaTeX: `it("should parse comments at the start of a line")`
    /// `"% comment\n"`
    #[test]
    fn katex_comment_at_start_of_line() {
        let tokens = lex_texts("% comment\n");
        assert_eq!(tokens, vec!["EOF"]);
    }

    /// KaTeX: `it("should parse multiple lines of comments in a row")`
    /// `"% comment 1\n% comment 2\n"`
    #[test]
    fn katex_multiple_comment_lines() {
        let tokens = lex_texts("% comment 1\n% comment 2\n");
        assert_eq!(tokens, vec!["EOF"]);
    }

    #[test]
    fn many_consecutive_comment_lines_are_lexed_iteratively() {
        let input = "%\n".repeat(4_200) + "x";
        assert_eq!(lex_texts(&input), vec!["x", "EOF"]);
    }

    /// KaTeX: `it("should parse comments between subscript and superscript")`
    /// `"x_3 %comment\n^2"` parseLike `"x_3^2"`
    #[test]
    fn katex_comment_between_sub_sup() {
        let with_comment = lex_texts("x_3 %comment\n^2");
        let without_comment = lex_texts("x_3^2");
        // After lexing: "x_3 %comment\n^2" → [x, _, 3, ^, 2]
        // Note: The space before % and the comment are removed.
        // But the space before % is still lexed as a space token!
        // Let's check what actually happens:
        // x → "x"
        // _ → "_"
        // 3 → "3"
        // " " → " " (space before %)
        // % comment\n → skipped
        // ^ → "^"
        // 2 → "2"
        assert_eq!(with_comment, vec!["x", "_", "3", " ", "^", "2", "EOF"]);
        // Without comment: x_3^2
        assert_eq!(without_comment, vec!["x", "_", "3", "^", "2", "EOF"]);
        // The parser would ignore the space; lexer correctly produces it.
    }

    /// KaTeX: `"x^ %comment\n{2}"` parseLike `"x^{2}"`
    #[test]
    fn katex_comment_after_caret() {
        let tokens = lex_texts("x^ %comment\n{2}");
        assert_eq!(tokens, vec!["x", "^", " ", "{", "2", "}", "EOF"]);
    }

    /// KaTeX: `"x^ %comment\n\\frac{1}{2}"` parseLike `"x^\\frac{1}{2}"`
    #[test]
    fn katex_comment_before_frac() {
        let tokens = lex_texts("x^ %comment\n\\frac{1}{2}");
        assert_eq!(
            tokens,
            vec!["x", "^", " ", "\\frac", "{", "1", "}", "{", "2", "}", "EOF"]
        );
    }

    /// KaTeX: `it("should parse comments in size and color groups")`
    /// `"\\kern{1 %kern\nem}"`
    #[test]
    fn katex_comment_in_kern() {
        let tokens = lex_texts("\\kern{1 %kern\nem}");
        assert_eq!(tokens, vec!["\\kern", "{", "1", " ", "e", "m", "}", "EOF"]);
    }

    /// KaTeX: `"\\kern1 %kern\nem"`
    #[test]
    fn katex_comment_in_kern_no_brace() {
        let tokens = lex_texts("\\kern1 %kern\nem");
        assert_eq!(tokens, vec!["\\kern", "1", " ", "e", "m", "EOF"]);
    }

    /// KaTeX: `"\\color{#f00%red\n}"`
    #[test]
    fn katex_comment_in_color() {
        let tokens = lex_texts("\\color{#f00%red\n}");
        assert_eq!(tokens, vec!["\\color", "{", "#", "f", "0", "0", "}", "EOF"]);
    }

    /// KaTeX: `it("should parse comments before an expression")`
    /// `"%comment\n{2}"` parseLike `"{2}"`
    #[test]
    fn katex_comment_before_expression() {
        let with_comment = lex_texts("%comment\n{2}");
        let without_comment = lex_texts("{2}");
        assert_eq!(with_comment, without_comment);
    }

    /// KaTeX: `it("should not produce or consume space")`
    /// `"hello% comment 1\nworld"` — comment consumes newline, no space produced
    #[test]
    fn katex_comment_no_space_produced() {
        let tokens = lex_texts("hello% comment 1\nworld");
        // % eats everything to \n inclusive; result is "hello" directly followed by "world"
        assert_eq!(
            tokens,
            vec!["h", "e", "l", "l", "o", "w", "o", "r", "l", "d", "EOF"]
        );
    }

    /// KaTeX: `"hello% comment\n\nworld"` — comment eats first \n, second \n becomes space
    #[test]
    fn katex_comment_then_blank_line() {
        let tokens = lex_texts("hello% comment\n\nworld");
        // % eats to first \n (inclusive), then second \n is whitespace → space token
        assert_eq!(
            tokens,
            vec!["h", "e", "l", "l", "o", " ", "w", "o", "r", "l", "d", "EOF"]
        );
    }

    /// KaTeX: `it("should not include comments in the output")`
    /// `"5 % comment\n"` parseLike `"5"`
    #[test]
    fn katex_comment_not_in_output() {
        let tokens = lex_texts("5 % comment\n");
        assert_eq!(tokens, vec!["5", " ", "EOF"]);
    }

    /// KaTeX: `it("should not parse a comment without newline in strict mode")`
    /// `"x%y"` — our lexer always skips comments (strict mode is a parser concern)
    #[test]
    fn katex_comment_without_newline() {
        let tokens = lex_texts("x%y");
        // Comment without newline: % eats to EOF
        assert_eq!(tokens, vec!["x", "EOF"]);
    }

    // =========================================================================
    // KaTeX katex-spec.ts: backslash + newline in \text
    // =========================================================================

    /// KaTeX: `it("should handle backslash followed by newline")`
    /// `"\\text{\\ \t\r \n \t\r }"` — \<space> followed by more whitespace
    /// At lexer level: `\\ ` is control space, then remaining whitespace collapses
    #[test]
    fn katex_backslash_whitespace_sequence() {
        let tokens = lex_texts("\\ \t\r \n \t\r ");
        // \<space> is control space (eats trailing whitespace too)
        assert_eq!(tokens, vec!["\\ ", "EOF"]);
    }

    // =========================================================================
    // KaTeX Lexer.ts: additional behaviors from source code analysis
    // =========================================================================

    /// Combining diacritical marks (U+0300-U+036F) are grouped with their base
    /// character into a single token, matching KaTeX's regex behavior.
    #[test]
    fn test_combining_diacritical_mark() {
        // 'a' + combining acute accent (U+0301)
        let tokens = lex_texts("a\u{0301}");
        assert_eq!(tokens, vec!["a\u{0301}", "EOF"]);
    }

    #[test]
    fn test_multiple_combining_marks() {
        // 'a' + combining acute (U+0301) + combining tilde (U+0303)
        let tokens = lex_texts("a\u{0301}\u{0303}");
        assert_eq!(tokens, vec!["a\u{0301}\u{0303}", "EOF"]);
    }

    #[test]
    fn test_combining_mark_between_chars() {
        let tokens = lex_texts("a\u{0301}b");
        assert_eq!(tokens, vec!["a\u{0301}", "b", "EOF"]);
    }

    /// Ord characters from KaTeX's test: `1234|/@.\"\`abcdefgzABCDEFGZ`
    #[test]
    fn katex_ord_characters() {
        let input = "1234";
        let tokens = lex_texts(input);
        assert_eq!(tokens, vec!["1", "2", "3", "4", "EOF"]);
    }

    /// Various special symbols should be individual tokens
    #[test]
    fn katex_special_symbols() {
        let tokens = lex_texts("|/@.\"'");
        assert_eq!(tokens, vec!["|", "/", "@", ".", "\"", "'", "EOF"]);
    }

    /// KaTeX includes `@` in control word characters: `\\[a-zA-Z@]+`.
    /// This is needed for internal macros like `\@ifstar`, `\@firstoftwo`.
    #[test]
    fn test_at_sign_in_control_word() {
        let tokens = lex_texts("\\foo@bar");
        // @ is part of control word chars, so \foo@bar is one token
        assert_eq!(tokens, vec!["\\foo@bar", "EOF"]);
    }

    #[test]
    fn test_at_ifstar() {
        let tokens = lex_texts("\\@ifstar");
        assert_eq!(tokens, vec!["\\@ifstar", "EOF"]);
    }

    // =========================================================================
    // Additional coverage: real-world LaTeX expressions
    // =========================================================================

    // =========================================================================
    // \verb / \verb* support
    // =========================================================================

    #[test]
    fn test_verb_basic() {
        let tokens = lex_texts("\\verb|hello|");
        assert_eq!(tokens, vec!["\\verb|hello|", "EOF"]);
    }

    #[test]
    fn test_verb_star() {
        let tokens = lex_texts("\\verb*|hello world|");
        assert_eq!(tokens, vec!["\\verb*|hello world|", "EOF"]);
    }

    #[test]
    fn test_verb_with_special_chars() {
        let tokens = lex_texts("\\verb!\\frac{a}{b}!");
        assert_eq!(tokens, vec!["\\verb!\\frac{a}{b}!", "EOF"]);
    }

    #[test]
    fn test_verb_in_expression() {
        let tokens = lex_texts("x + \\verb|y| + z");
        assert_eq!(
            tokens,
            vec!["x", " ", "+", " ", "\\verb|y|", " ", "+", " ", "z", "EOF"]
        );
    }

    /// Quadratic formula
    #[test]
    fn katex_quadratic_formula() {
        let tokens = lex_texts("\\frac{-b \\pm \\sqrt{b^2-4ac}}{2a}");
        assert_eq!(
            tokens,
            vec![
                "\\frac", "{", "-", "b", " ", "\\pm", "\\sqrt", "{", "b", "^", "2", "-", "4", "a",
                "c", "}", "}", "{", "2", "a", "}", "EOF"
            ]
        );
    }

    /// Matrix environment
    #[test]
    fn katex_matrix_begin_end() {
        let tokens = lex_texts("\\begin{pmatrix} a & b \\\\ c & d \\end{pmatrix}");
        assert_eq!(
            tokens,
            vec![
                "\\begin", "{", "p", "m", "a", "t", "r", "i", "x", "}", " ", "a", " ", "&", " ",
                "b", " ", "\\\\", " ", "c", " ", "&", " ", "d", " ", "\\end", "{", "p", "m", "a",
                "t", "r", "i", "x", "}", "EOF"
            ]
        );
    }
}

#[cfg(test)]
mod span_tests {
    use super::*;
    #[test]
    fn borrowed_spans_preserve_normalization_locations_and_eof() {
        let input = "  \\alpha  \\ \t\\verb|abc|é\u{0301}% comment\n~";
        let mut spans = Lexer::new(input);
        let expected = [
            (" ", 0, 2),
            ("\\alpha", 2, 8),
            ("\\ ", 10, 13),
            ("\\verb|abc|", 13, 23),
            ("é\u{0301}", 23, 27),
            ("~", 37, 38),
        ];
        for (text, start, end) in expected {
            let span = spans.next_span();
            assert_eq!(span.text, text);
            assert_eq!(span.loc, SourceLocation { start, end });
            assert!(!span.is_eof);
        }
        assert!(spans.next_span().is_eof);
    }
}
