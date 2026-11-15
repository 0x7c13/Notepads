// Notepads Markdown bounded modifications: Copyright (c) 2026, Jiaqi (0x7c13) Liu.
use ratex_lexer::token::{SourceLocation, Token};

/// Error type for the parser, modeled after KaTeX's ParseError.
#[derive(Debug, Clone)]
pub struct ParseError {
    pub message: std::borrow::Cow<'static, str>,
    pub loc: Option<SourceLocation>,
}

impl ParseError {
    /// The complete typed failure is retained by the explicit operation ledger. Constructing
    /// its parser sentinel must not allocate another diagnostic after an allocation failure.
    pub(crate) fn storage(_error: ratex_types::storage::StorageError) -> Self {
        Self {
            message: std::borrow::Cow::Borrowed("Resource operation failed"),
            loc: None,
        }
    }
    pub(crate) fn control(_error: ratex_types::execution::ExecutionError) -> Self {
        Self {
            message: std::borrow::Cow::Borrowed("Controlled operation failed"),
            loc: None,
        }
    }
    pub fn new(message: impl Into<std::borrow::Cow<'static, str>>, token: Option<&Token>) -> Self {
        Self {
            message: message.into(),
            loc: token.map(|t| t.loc.clone()),
        }
    }

    pub fn msg(message: impl Into<std::borrow::Cow<'static, str>>) -> Self {
        Self {
            message: message.into(),
            loc: None,
        }
    }

    pub fn at(message: impl Into<std::borrow::Cow<'static, str>>, loc: SourceLocation) -> Self {
        Self {
            message: message.into(),
            loc: Some(loc),
        }
    }

    pub fn recursion_limit_exceeded() -> Self {
        Self::msg("Recursion limit exceeded")
    }
}

impl std::fmt::Display for ParseError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        if let Some(ref loc) = self.loc {
            write!(f, "ParseError at position {}: {}", loc.start, self.message)
        } else {
            write!(f, "ParseError: {}", self.message)
        }
    }
}

impl std::error::Error for ParseError {}

pub type ParseResult<T> = Result<T, ParseError>;
