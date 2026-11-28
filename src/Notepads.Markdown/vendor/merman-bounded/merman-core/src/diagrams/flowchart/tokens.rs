// Notepads Markdown modifications: Copyright (c) 2026, Jiaqi (0x7c13) Liu.
// Original upstream MIT/Apache-2.0 licenses and notices are preserved.
use super::{
    ClassAssignStmt, ClassDefStmt, ClickStmt, LabeledText, LinkStyleStmt, LinkToken, StyleStmt,
    SubgraphHeader,
};
use crate::SourceSpan;
#[cfg(any(feature = "diagram-flowchart", feature = "diagram-swimlane"))]
use crate::error::ParseErrorSourceSpan;

#[derive(Debug, Clone)]
#[cfg_attr(
    not(any(feature = "diagram-flowchart", feature = "diagram-swimlane")),
    allow(
        dead_code,
        reason = "Agentflow shares presentation lexing but does not consume Flowchart grammar and recovery payloads."
    )
)]
pub(crate) enum Tok {
    KwGraph,
    KwFlowchart,
    KwFlowchartElk,
    KwSwimlane,
    KwSubgraph,
    KwEnd,

    Sep,
    Amp,
    StyleSep,
    NodeLabel(NodeLabelToken),

    Direction(String),
    DirectionStmt(DirectionStatementToken),
    Id(String),
    Arrow(ArrowToken),
    EdgeLabel(LabeledText),
    SubgraphHeader(SubgraphHeader),

    StyleStmt(StyleStmt),
    ClassDefStmt(ClassDefStmt),
    ClassAssignStmt(ClassAssignStmt),
    ClickStmt(ClickStmt),
    LinkStyleStmt(LinkStyleStmt),

    EdgeId(String),
    ShapeData(String),
}

#[derive(Debug, Clone)]
#[cfg_attr(
    not(any(feature = "diagram-flowchart", feature = "diagram-swimlane")),
    allow(
        dead_code,
        reason = "Agentflow shares presentation lexing but does not consume Flowchart grammar and recovery payloads."
    )
)]
pub(crate) struct ArrowToken {
    pub link: LinkToken,
    pub recovery_error: Option<LexError>,
}

#[derive(Debug, Clone)]
#[cfg_attr(
    not(any(feature = "diagram-flowchart", feature = "diagram-swimlane")),
    allow(
        dead_code,
        reason = "Agentflow shares presentation lexing but does not consume Flowchart grammar and recovery payloads."
    )
)]
pub(crate) struct NodeLabelToken {
    pub shape: String,
    pub text: LabeledText,
    pub trigger_span: Option<SourceSpan>,
    /// Strict parser error represented by this editor-recovery token.
    ///
    /// The combined semantic path lexes once. An incomplete label therefore carries both the
    /// token needed for editor facts and the error projected to the strict LALRPOP parser.
    pub recovery_error: Option<LexError>,
}

#[derive(Debug, Clone)]
#[cfg_attr(
    not(any(feature = "diagram-flowchart", feature = "diagram-swimlane")),
    allow(
        dead_code,
        reason = "Agentflow shares presentation lexing but does not consume Flowchart grammar and recovery payloads."
    )
)]
pub(crate) struct DirectionStatementToken {
    pub direction: String,
    pub selection: SourceSpan,
    pub recovery_error: Option<LexError>,
}

#[derive(Debug, Clone, thiserror::Error)]
#[error("{message}")]
pub(crate) struct LexError {
    pub message: String,
    pub span: Option<SourceSpan>,
    pub expected_syntax: Vec<crate::EditorExpectedSyntax>,
    pub directive_prefix: Option<&'static str>,
    diagnostic_fixed: bool,
}

impl LexError {
    // Native terminal projection has no heap allocation. Its true fixed cause
    // remains in the shared job authority and is consumed before formatting.
    pub(crate) fn terminal() -> Self {
        Self {
            message: String::new(),
            span: None,
            expected_syntax: Vec::new(),
            directive_prefix: None,
            diagnostic_fixed: true,
        }
    }

    pub(crate) fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
            span: None,
            expected_syntax: Vec::new(),
            directive_prefix: None,
            diagnostic_fixed: false,
        }
    }

    pub(crate) fn with_span(message: impl Into<String>, span: SourceSpan) -> Self {
        Self {
            message: message.into(),
            span: Some(span),
            expected_syntax: Vec::new(),
            directive_prefix: None,
            diagnostic_fixed: false,
        }
    }

    pub(crate) fn expecting(
        mut self,
        kind: crate::EditorExpectedSyntaxKind,
        span: SourceSpan,
    ) -> Self {
        if self.diagnostic_fixed {
            return self;
        }
        let expected = crate::EditorExpectedSyntax::new(kind, span);
        if !self.expected_syntax.contains(&expected) {
            self.expected_syntax.push(expected);
        }
        self
    }

    pub(crate) fn at_span(mut self, span: SourceSpan) -> Self {
        self.span = Some(span);
        self
    }

    pub(crate) fn in_directive(mut self, prefix: &'static str, span: SourceSpan) -> Self {
        self.directive_prefix = Some(prefix);
        self.expecting(crate::EditorExpectedSyntaxKind::Directive, span)
    }
}

#[cfg(any(feature = "diagram-flowchart", feature = "diagram-swimlane"))]
impl ParseErrorSourceSpan for LexError {
    fn source_span(&self) -> Option<SourceSpan> {
        self.span
    }
}
