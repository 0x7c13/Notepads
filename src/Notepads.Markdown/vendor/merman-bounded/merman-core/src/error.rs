use crate::{SourceSpan, detect::DetectTypeError};

pub type Result<T> = std::result::Result<T, Error>;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ParseDiagnosticSpanKind {
    Exact,
    InsertionPoint,
    Fallback,
}

/// Structured parser diagnostic carried by [`Error::DiagramParse`].
///
/// `message()` is the human-readable text that older callers previously matched through a
/// top-level `Error::DiagramParse { message, .. }` field. Prefer the structured accessors when an
/// integration can use spans or stable diagnostic codes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParseDiagnostic {
    message: String,
    span: Option<SourceSpan>,
    span_kind: ParseDiagnosticSpanKind,
    code: Option<String>,
}

#[cfg(any(
    feature = "diagram-class",
    feature = "diagram-er",
    feature = "diagram-flowchart",
    feature = "diagram-swimlane",
    feature = "diagram-sequence",
    feature = "diagram-state"
))]
pub(crate) trait ParseErrorSourceSpan {
    fn source_span(&self) -> Option<SourceSpan>;
}

#[cfg(any(
    feature = "diagram-class",
    feature = "diagram-er",
    feature = "diagram-flowchart",
    feature = "diagram-swimlane",
    feature = "diagram-sequence",
    feature = "diagram-state"
))]
impl ParseErrorSourceSpan for String {
    fn source_span(&self) -> Option<SourceSpan> {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::{Error, ParseDiagnostic, ParseDiagnosticSpanKind};
    use crate::SourceSpan;

    #[test]
    fn diagram_parse_fallback_builds_structured_fallback_diagnostic() {
        let error = Error::diagram_parse_fallback("flowchart", "bad syntax");
        let display = error.to_string();

        let Error::DiagramParse {
            diagram_type,
            diagnostic,
        } = error
        else {
            panic!("expected diagram parse error");
        };

        assert_eq!(diagram_type, "flowchart");
        assert_eq!(diagnostic.message(), "bad syntax");
        assert_eq!(diagnostic.span(), None);
        assert_eq!(diagnostic.span_kind(), ParseDiagnosticSpanKind::Fallback);
        assert_eq!(diagnostic.code(), None);
        assert_eq!(display, "Diagram parse error (flowchart): bad syntax");
    }

    #[test]
    fn diagram_parse_diagnostic_preserves_structured_metadata() {
        let span = SourceSpan::new(2, 5);
        let error = Error::diagram_parse_diagnostic(
            "state",
            ParseDiagnostic::new("unexpected token")
                .with_span(span, ParseDiagnosticSpanKind::Exact)
                .with_code("merman.test"),
        );

        let Error::DiagramParse {
            diagram_type,
            diagnostic,
        } = error
        else {
            panic!("expected diagram parse error");
        };

        assert_eq!(diagram_type, "state");
        assert_eq!(diagnostic.message(), "unexpected token");
        assert_eq!(diagnostic.span(), Some(span));
        assert_eq!(diagnostic.span_kind(), ParseDiagnosticSpanKind::Exact);
        assert_eq!(diagnostic.code(), Some("merman.test"));
    }
}

impl ParseDiagnostic {
    pub fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
            span: None,
            span_kind: ParseDiagnosticSpanKind::Fallback,
            code: None,
        }
    }

    pub fn with_span(mut self, span: SourceSpan, span_kind: ParseDiagnosticSpanKind) -> Self {
        self.span = Some(span);
        self.span_kind = span_kind;
        self
    }

    pub fn with_code(mut self, code: impl Into<String>) -> Self {
        self.code = Some(code.into());
        self
    }

    pub fn map_span(mut self, map: impl FnOnce(SourceSpan) -> SourceSpan) -> Self {
        self.span = self.span.map(map);
        self
    }

    pub(crate) fn without_span(mut self) -> Self {
        self.span = None;
        self.span_kind = ParseDiagnosticSpanKind::Fallback;
        self
    }

    pub fn message(&self) -> &str {
        &self.message
    }

    pub fn span(&self) -> Option<SourceSpan> {
        self.span
    }

    pub fn span_kind(&self) -> ParseDiagnosticSpanKind {
        self.span_kind
    }

    pub fn code(&self) -> Option<&str> {
        self.code.as_deref()
    }
}

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error(transparent)]
    OperationCancelled(#[from] crate::OperationCancelled),

    #[error(transparent)]
    ThemeColor(#[from] crate::theme_color::ColorError),

    #[error(transparent)]
    RuntimePolicy(#[from] crate::runtime::RuntimePolicyError),

    #[error(transparent)]
    DetectType(#[from] DetectTypeError),

    #[error("Unsupported diagram type: {diagram_type}")]
    UnsupportedDiagram { diagram_type: String },

    /// Parser failure for a detected diagram.
    ///
    /// This API now carries `diagnostic: ParseDiagnostic` instead of a top-level `message`
    /// field. Call `diagnostic.message()` for the display text and `diagnostic.span()` /
    /// `diagnostic.span_kind()` when projecting editor or CLI diagnostics.
    #[error("Diagram parse error ({diagram_type}): {}", diagnostic.message())]
    DiagramParse {
        diagram_type: String,
        diagnostic: ParseDiagnostic,
    },

    #[error(
        "Malformed YAML front-matter. If you were trying to use a YAML front-matter, please ensure that you've correctly opened and closed the YAML front-matter with un-indented `---` blocks"
    )]
    MalformedFrontMatter,

    #[error("Invalid directive JSON: {message}")]
    InvalidDirectiveJson { message: String },

    #[error("Invalid YAML front-matter: {message}")]
    InvalidFrontMatterYaml { message: String },
}

impl Error {
    #[cfg(any(
        feature = "diagram-git-graph",
        feature = "diagram-radar",
        feature = "diagram-mindmap"
    ))]
    pub(crate) fn with_exact_span_if_missing(self, span: SourceSpan) -> Self {
        match self {
            Self::DiagramParse {
                diagram_type,
                diagnostic,
            } if diagnostic.span().is_none() => Self::DiagramParse {
                diagram_type,
                diagnostic: diagnostic.with_span(span, ParseDiagnosticSpanKind::Exact),
            },
            other => other,
        }
    }

    pub fn diagram_parse_fallback(
        diagram_type: impl Into<String>,
        message: impl Into<String>,
    ) -> Self {
        Self::diagram_parse_diagnostic(diagram_type, ParseDiagnostic::new(message))
    }

    pub fn diagram_parse_exact(
        diagram_type: impl Into<String>,
        message: impl Into<String>,
        span: SourceSpan,
    ) -> Self {
        Self::diagram_parse_diagnostic(
            diagram_type,
            ParseDiagnostic::new(message).with_span(span, ParseDiagnosticSpanKind::Exact),
        )
    }

    pub fn diagram_parse_insertion_point(
        diagram_type: impl Into<String>,
        message: impl Into<String>,
        offset: usize,
    ) -> Self {
        Self::diagram_parse_diagnostic(
            diagram_type,
            ParseDiagnostic::new(message).with_span(
                SourceSpan::new(offset, offset),
                ParseDiagnosticSpanKind::InsertionPoint,
            ),
        )
    }

    pub fn diagram_parse_fallback_at(
        diagram_type: impl Into<String>,
        message: impl Into<String>,
        span: SourceSpan,
    ) -> Self {
        Self::diagram_parse_diagnostic(
            diagram_type,
            ParseDiagnostic::new(message).with_span(span, ParseDiagnosticSpanKind::Fallback),
        )
    }

    pub fn diagram_parse_diagnostic(
        diagram_type: impl Into<String>,
        diagnostic: ParseDiagnostic,
    ) -> Self {
        Self::DiagramParse {
            diagram_type: diagram_type.into(),
            diagnostic,
        }
    }
}
