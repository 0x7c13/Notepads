use crate::SourceSpan;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct SpannedText {
    pub(super) value: String,
    pub(super) span: SourceSpan,
}

impl SpannedText {
    pub(super) fn new(value: impl Into<String>, span: SourceSpan) -> Self {
        Self {
            value: value.into(),
            span,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct SyntaxDocument {
    pub(super) title: Option<SpannedText>,
    pub(super) head: Vec<HeadItemSyntax>,
    pub(super) starter: Option<StarterSyntax>,
    pub(super) statements: Vec<StatementSyntax>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) enum HeadItemSyntax {
    Participant(Box<ParticipantSyntax>),
    Group(GroupSyntax),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct StarterSyntax {
    pub(super) name: Option<SpannedText>,
    pub(super) span: SourceSpan,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct ParticipantSyntax {
    pub(super) name: SpannedText,
    pub(super) label: Option<SpannedText>,
    pub(super) participant_type: Option<SpannedText>,
    pub(super) stereotype: Option<SpannedText>,
    pub(super) emoji: Option<SpannedText>,
    pub(super) width: Option<SpannedText>,
    pub(super) color: Option<SpannedText>,
    pub(super) comment: Option<SpannedText>,
    pub(super) span: SourceSpan,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct GroupSyntax {
    pub(super) name: Option<SpannedText>,
    pub(super) participants: Vec<ParticipantSyntax>,
    pub(super) span: SourceSpan,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct StatementSyntax {
    pub(super) kind: StatementKindSyntax,
    pub(super) comment: Option<SpannedText>,
    pub(super) span: SourceSpan,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) enum StatementKindSyntax {
    Message(MessageSyntax),
    Creation(CreationSyntax),
    Return(ReturnSyntax),
    Fragment(FragmentSyntax),
    Reference(ReferenceSyntax),
    Divider(SpannedText),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum MessageStyleSyntax {
    Synchronous,
    Asynchronous,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct MessageSyntax {
    pub(super) from: Option<SpannedText>,
    pub(super) from_emoji: Option<SpannedText>,
    pub(super) to: Option<SpannedText>,
    pub(super) to_emoji: Option<SpannedText>,
    pub(super) signature: SpannedText,
    pub(super) assignment: Option<SpannedText>,
    pub(super) style: MessageStyleSyntax,
    pub(super) body: Vec<StatementSyntax>,
    pub(super) body_comment: Option<SpannedText>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct CreationSyntax {
    pub(super) constructor: SpannedText,
    pub(super) assignment: Option<SpannedText>,
    pub(super) parameters: Option<SpannedText>,
    pub(super) body: Vec<StatementSyntax>,
    pub(super) body_comment: Option<SpannedText>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct ReturnSyntax {
    pub(super) from: Option<SpannedText>,
    pub(super) from_emoji: Option<SpannedText>,
    pub(super) to: Option<SpannedText>,
    pub(super) to_emoji: Option<SpannedText>,
    pub(super) value: Option<SpannedText>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum FragmentKindSyntax {
    Loop,
    Alternative,
    Parallel,
    Optional,
    Critical,
    Section,
    TryCatchFinally,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct FragmentSyntax {
    pub(super) kind: FragmentKindSyntax,
    pub(super) label: Option<SpannedText>,
    pub(super) sections: Vec<FragmentSectionSyntax>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct FragmentSectionSyntax {
    pub(super) label: Option<SpannedText>,
    pub(super) statements: Vec<StatementSyntax>,
    pub(super) body_comment: Option<SpannedText>,
    pub(super) span: SourceSpan,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct ReferenceSyntax {
    pub(super) participants: Vec<SpannedText>,
    pub(super) span: SourceSpan,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct SyntaxDiagnostic {
    pub(super) message: String,
    pub(super) span: SourceSpan,
}

#[derive(Debug, Clone)]
pub(super) struct ParsedSyntax {
    pub(super) document: SyntaxDocument,
    pub(super) diagnostics: Vec<SyntaxDiagnostic>,
}
