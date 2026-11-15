// Notepads Markdown bounded modifications: Copyright (c) 2026, Jiaqi (0x7c13) Liu.
use serde::{Deserialize, Serialize};

/// Source location of a token in the original LaTeX string.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SourceLocation {
    pub start: usize,
    pub end: usize,
}

impl SourceLocation {
    /// Compute a location spanning from `start_loc` to `end_loc`.
    pub fn range(start_loc: &SourceLocation, end_loc: &SourceLocation) -> Self {
        Self {
            start: start_loc.start,
            end: end_loc.end,
        }
    }
}

/// A lexed token from a LaTeX input string.
///
/// Mirrors KaTeX's Token class: the token's identity is determined by its
/// `text` field. The parser and macro expander interpret meaning.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Token {
    /// True only for a literal emitted from the fixed readonly builtin template.
    /// User lexer tokens and macro arguments never acquire this bit by substitution.
    pub is_builtin_literal: bool,
    /// The text content of the token.
    /// - For control sequences: `"\\frac"`, `"\\alpha"`, `"\\ "` (control space)
    /// - For single chars: `"a"`, `"+"`, `"{"`, `"}"`, `"^"`, `"_"`
    /// - For whitespace: `" "`
    /// - For EOF: `"EOF"`
    pub text: String,
    pub loc: SourceLocation,
    /// When true, the macro expander should not expand this token.
    /// Set by `\noexpand`.
    pub noexpand: bool,
    /// When true, the token should be treated as `\relax` by the parser.
    /// Used in conjunction with `noexpand` for `\noexpand` semantics.
    pub treat_as_relax: bool,
}

impl Token {
    pub fn new(text: impl Into<String>, start: usize, end: usize) -> Self {
        Self {
            is_builtin_literal: false,
            text: text.into(),
            loc: SourceLocation { start, end },
            noexpand: false,
            treat_as_relax: false,
        }
    }

    pub fn eof(pos: usize) -> Self {
        Self::new("EOF", pos, pos)
    }

    pub fn is_eof(&self) -> bool {
        self.text == "EOF"
    }

    /// Check if this token is a control sequence (starts with `\`).
    pub fn is_command(&self) -> bool {
        self.text.starts_with('\\')
    }

    /// Check if this is a whitespace token.
    pub fn is_space(&self) -> bool {
        self.text == " "
    }

    /// For a control sequence, return the name without the leading `\`.
    pub fn command_name(&self) -> Option<&str> {
        if self.is_command() {
            Some(&self.text[1..])
        } else {
            None
        }
    }

    /// Create a new token spanning from `self` to `end_token` with the given text.
    pub fn range(&self, end_token: &Token, text: impl Into<String>) -> Token {
        Token {
            is_builtin_literal: self.is_builtin_literal,
            text: text.into(),
            loc: SourceLocation::range(&self.loc, &end_token.loc),
            noexpand: false,
            treat_as_relax: false,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_token_creation() {
        let tok = Token::new("\\frac", 0, 5);
        assert_eq!(tok.text, "\\frac");
        assert_eq!(tok.loc.start, 0);
        assert_eq!(tok.loc.end, 5);
    }

    #[test]
    fn test_is_command() {
        assert!(Token::new("\\frac", 0, 5).is_command());
        assert!(Token::new("\\ ", 0, 2).is_command());
        assert!(!Token::new("a", 0, 1).is_command());
        assert!(!Token::new("{", 0, 1).is_command());
    }

    #[test]
    fn test_is_eof() {
        assert!(Token::eof(10).is_eof());
        assert!(!Token::new("x", 0, 1).is_eof());
    }

    #[test]
    fn test_command_name() {
        assert_eq!(Token::new("\\frac", 0, 5).command_name(), Some("frac"));
        assert_eq!(Token::new("\\alpha", 0, 6).command_name(), Some("alpha"));
        assert_eq!(Token::new("\\ ", 0, 2).command_name(), Some(" "));
        assert_eq!(Token::new("a", 0, 1).command_name(), None);
    }

    #[test]
    fn test_is_space() {
        assert!(Token::new(" ", 0, 1).is_space());
        assert!(!Token::new("a", 0, 1).is_space());
        assert!(!Token::new("\\ ", 0, 2).is_space());
    }

    #[test]
    fn test_noexpand_default_false() {
        let tok = Token::new("\\foo", 0, 4);
        assert!(!tok.noexpand);
        assert!(!tok.treat_as_relax);
    }

    #[test]
    fn test_range() {
        let t1 = Token::new("a", 0, 1);
        let t2 = Token::new("c", 4, 5);
        let spanned = t1.range(&t2, "abc");
        assert_eq!(spanned.text, "abc");
        assert_eq!(spanned.loc.start, 0);
        assert_eq!(spanned.loc.end, 5);
    }

    #[test]
    fn test_source_location_range() {
        let a = SourceLocation { start: 0, end: 5 };
        let b = SourceLocation { start: 10, end: 15 };
        let r = SourceLocation::range(&a, &b);
        assert_eq!(r.start, 0);
        assert_eq!(r.end, 15);
    }
}
