use super::ast::*;
use super::model::*;
use crate::{
    EditorExpectedSyntax, EditorExpectedSyntaxKind, EditorSemanticFacts, EditorSemanticKind,
    EditorSemanticRole, EditorSemanticSymbol, OperationControl, OperationControlResult, SourceSpan,
};
use indexmap::IndexMap;

pub(super) struct SemanticBuild {
    pub(super) model: ZenumlDiagramRenderModel,
    pub(super) editor_facts: EditorSemanticFacts,
    pub(super) diagnostics: Vec<SyntaxDiagnostic>,
}

#[cfg(test)]
pub(super) fn build(parsed: ParsedSyntax) -> SemanticBuild {
    build_controlled(parsed, &OperationControl::new())
        .expect("a private parse control cannot be cancelled")
}

pub(super) fn build_controlled(
    parsed: ParsedSyntax,
    control: &OperationControl,
) -> OperationControlResult<SemanticBuild> {
    SemanticBuilder::new(parsed, control).build()
}

struct ParticipantAccumulator {
    participant: ZenumlParticipant,
}

struct SemanticBuilder {
    syntax: SyntaxDocument,
    diagnostics: Vec<SyntaxDiagnostic>,
    participants: IndexMap<String, ParticipantAccumulator>,
    groups: Vec<ZenumlGroup>,
    facts: EditorSemanticFacts,
    generated_statement_id: usize,
    ownable_statement_count: usize,
    some_statement_misses_from: bool,
    control: OperationControl,
}

#[derive(Clone)]
struct ResolveContext {
    origin: Option<String>,
    owner: Option<String>,
    return_to: Option<String>,
}

impl SemanticBuilder {
    fn new(parsed: ParsedSyntax, control: &OperationControl) -> Self {
        Self {
            syntax: parsed.document,
            diagnostics: parsed.diagnostics,
            participants: IndexMap::new(),
            groups: Vec::new(),
            facts: EditorSemanticFacts::new(),
            generated_statement_id: 0,
            ownable_statement_count: 0,
            some_statement_misses_from: false,
            control: control.clone(),
        }
    }

    fn build(mut self) -> OperationControlResult<SemanticBuild> {
        self.control.checkpoint()?;
        self.facts.push_directive_prefix("title");
        if let Some(title) = self.syntax.title.clone() {
            self.push_payload(&title, "zenuml title", EditorSemanticKind::String);
        }

        for (index, item) in self.syntax.head.clone().into_iter().enumerate() {
            if index % 128 == 0 {
                self.control.checkpoint()?;
            }
            match item {
                HeadItemSyntax::Participant(participant) => {
                    self.declare_participant(&participant, None);
                }
                HeadItemSyntax::Group(group) => {
                    let group_id = group.name.as_ref().map(|name| name.value.clone());
                    if let Some(name) = &group.name {
                        self.push_outline(
                            name,
                            "zenuml participant group",
                            EditorSemanticKind::Namespace,
                        );
                    }
                    let mut participant_names = Vec::new();
                    for (index, participant) in group.participants.iter().enumerate() {
                        if index % 128 == 0 {
                            self.control.checkpoint()?;
                        }
                        participant_names.push(participant.name.value.clone());
                        self.declare_participant(participant, group_id.clone());
                    }
                    self.groups.push(ZenumlGroup {
                        id: group_id,
                        participant_names,
                        span: group.span,
                    });
                }
            }
        }

        let starter_name = self
            .syntax
            .starter
            .as_ref()
            .and_then(|starter| starter.name.clone());
        let starter = starter_name
            .clone()
            .unwrap_or_else(|| SpannedText::new("_STARTER_", SourceSpan::new(0, 0)));
        let has_explicit_starter = starter_name.is_some();
        if has_explicit_starter {
            self.reference_participant(&starter, true, "zenuml starter");
        }

        let context = ResolveContext {
            origin: starter_name.as_ref().map(|starter| starter.value.clone()),
            owner: None,
            return_to: starter_name.as_ref().map(|starter| starter.value.clone()),
        };
        let syntax_statements = std::mem::take(&mut self.syntax.statements);
        let statements = self.resolve_statements(&syntax_statements, &context)?;

        let needs_default_starter = (self.ownable_statement_count == 0
            && self.participants.is_empty())
            || self.some_statement_misses_from;
        if needs_default_starter {
            self.reference_named_participant(
                "_STARTER_",
                None,
                true,
                None,
                "zenuml participant reference",
            );
        }

        if needs_default_starter && self.participants.contains_key("_STARTER_") {
            let starter = self
                .participants
                .shift_remove("_STARTER_")
                .expect("starter was present");
            self.participants
                .shift_insert(0, "_STARTER_".to_string(), starter);
        }

        for (index, diagnostic) in self.diagnostics.iter().enumerate() {
            if index % 128 == 0 {
                self.control.checkpoint()?;
            }
            self.facts.mark_recovered_from_parse_error(
                format!(
                    "zenuml parser recovered after parse error: {}",
                    diagnostic.message
                ),
                Some(diagnostic.span),
            );
            self.facts.push_expected_syntax(EditorExpectedSyntax::new(
                EditorExpectedSyntaxKind::Payload,
                diagnostic.span,
            ));
        }

        let model = ZenumlDiagramRenderModel {
            title: self.syntax.title.as_ref().map(|title| title.value.clone()),
            starter: has_explicit_starter.then_some(starter.value),
            participants: self
                .participants
                .into_values()
                .map(|entry| entry.participant)
                .collect(),
            groups: self.groups,
            statements,
        };
        self.control.checkpoint()?;
        Ok(SemanticBuild {
            model,
            editor_facts: self.facts,
            diagnostics: self.diagnostics,
        })
    }

    fn declare_participant(&mut self, syntax: &ParticipantSyntax, group_id: Option<String>) {
        let name = syntax.name.value.clone();
        let entry =
            self.participants
                .entry(name.clone())
                .or_insert_with(|| ParticipantAccumulator {
                    participant: ZenumlParticipant {
                        name: name.clone(),
                        label: None,
                        participant_type: None,
                        stereotype: None,
                        emoji: None,
                        width_source: None,
                        color: None,
                        comment: None,
                        group_id: None,
                        explicit: false,
                        is_starter: false,
                        declaration_span: None,
                        occurrences: Vec::new(),
                    },
                });
        let participant = &mut entry.participant;
        participant.explicit = true;
        participant.declaration_span = Some(syntax.span);
        participant.occurrences.push(syntax.name.span);
        participant.label = participant
            .label
            .take()
            .or_else(|| syntax.label.as_ref().map(|label| label.value.clone()));
        participant.participant_type = participant.participant_type.take().or_else(|| {
            syntax
                .participant_type
                .as_ref()
                .map(|value| value.value.clone())
        });
        participant.stereotype = participant
            .stereotype
            .take()
            .or_else(|| syntax.stereotype.as_ref().map(|value| value.value.clone()));
        participant.emoji = participant
            .emoji
            .take()
            .or_else(|| syntax.emoji.as_ref().map(|value| value.value.clone()));
        participant.width_source = participant
            .width_source
            .take()
            .or_else(|| syntax.width.as_ref().map(|width| width.value.clone()));
        participant.color = participant
            .color
            .take()
            .or_else(|| syntax.color.as_ref().map(|value| value.value.clone()));
        participant.comment = participant
            .comment
            .take()
            .or_else(|| syntax.comment.as_ref().map(|value| value.value.clone()));
        participant.group_id = participant.group_id.take().or(group_id);

        self.push_entity(&syntax.name, "zenuml participant");
        if let Some(label) = &syntax.label {
            self.push_payload(
                label,
                "zenuml participant label",
                EditorSemanticKind::String,
            );
        }
        if let Some(stereotype) = &syntax.stereotype {
            self.push_payload(
                stereotype,
                "zenuml participant stereotype",
                EditorSemanticKind::Class,
            );
        }
        if let Some(participant_type) = &syntax.participant_type {
            self.push_payload(
                participant_type,
                "zenuml participant type",
                EditorSemanticKind::Class,
            );
        }
        if let Some(emoji) = &syntax.emoji {
            self.push_payload(
                emoji,
                "zenuml participant emoji",
                EditorSemanticKind::String,
            );
        }
        if let Some(color) = &syntax.color {
            self.push_payload(
                color,
                "zenuml participant color",
                EditorSemanticKind::String,
            );
        }
    }

    fn resolve_statements(
        &mut self,
        statements: &[StatementSyntax],
        context: &ResolveContext,
    ) -> OperationControlResult<Vec<ZenumlStatement>> {
        enum Task<'a> {
            Statements(&'a [StatementSyntax], ResolveContext),
            Statement(&'a StatementSyntax, ResolveContext, usize),
            Section(&'a FragmentSectionSyntax, ResolveContext),
            FinishStatements(usize),
            FinishStatement(ZenumlStatement, usize),
            FinishSection(&'a FragmentSectionSyntax),
        }
        enum Completed {
            Statements(Vec<ZenumlStatement>),
            Statement(ZenumlStatement),
            Section(ZenumlFragmentSection),
        }

        // Enter statements and sections in source order; finish their owning records after
        // their children. Participant collection and generated IDs therefore retain preorder.
        let mut tasks = vec![Task::Statements(statements, context.clone())];
        let mut completed = Vec::new();
        while let Some(task) = tasks.pop() {
            match task {
                Task::Statements(statements, context) => {
                    tasks.push(Task::FinishStatements(statements.len()));
                    tasks.extend(
                        statements
                            .iter()
                            .enumerate()
                            .rev()
                            .map(|(index, statement)| {
                                Task::Statement(statement, context.clone(), index)
                            }),
                    );
                }
                Task::Statement(statement, context, index) => {
                    if index % 128 == 0 {
                        self.control.checkpoint()?;
                    }
                    let (resolved, nested_context) =
                        self.resolve_statement_head(statement, &context);
                    match &statement.kind {
                        StatementKindSyntax::Message(message) => {
                            tasks.push(Task::FinishStatement(resolved, 1));
                            tasks.push(Task::Statements(
                                &message.body,
                                nested_context.expect("message has its own resolve context"),
                            ));
                        }
                        StatementKindSyntax::Creation(creation) => {
                            tasks.push(Task::FinishStatement(resolved, 1));
                            tasks.push(Task::Statements(
                                &creation.body,
                                nested_context.expect("creation has its own resolve context"),
                            ));
                        }
                        StatementKindSyntax::Fragment(fragment) => {
                            tasks.push(Task::FinishStatement(resolved, fragment.sections.len()));
                            tasks.extend(
                                fragment
                                    .sections
                                    .iter()
                                    .rev()
                                    .map(|section| Task::Section(section, context.clone())),
                            );
                        }
                        _ => tasks.push(Task::FinishStatement(resolved, 0)),
                    }
                }
                Task::Section(section, context) => {
                    self.control.checkpoint()?;
                    if let Some(label) = &section.label {
                        self.push_payload(
                            label,
                            "zenuml fragment section",
                            EditorSemanticKind::String,
                        );
                    }
                    tasks.push(Task::FinishSection(section));
                    tasks.push(Task::Statements(&section.statements, context));
                }
                Task::FinishStatements(count) => {
                    let mut resolved = Vec::with_capacity(count);
                    for _ in 0..count {
                        let Some(Completed::Statement(statement)) = completed.pop() else {
                            unreachable!("statement list finishes after all of its statements")
                        };
                        resolved.push(statement);
                    }
                    resolved.reverse();
                    completed.push(Completed::Statements(resolved));
                }
                Task::FinishStatement(mut statement, count) => {
                    match &mut statement.kind {
                        ZenumlStatementKind::Message { body, .. }
                        | ZenumlStatementKind::Creation { body, .. } => {
                            let Some(Completed::Statements(statements)) = completed.pop() else {
                                unreachable!("message and creation finish after their body")
                            };
                            *body = statements;
                        }
                        ZenumlStatementKind::Fragment { sections, .. } => {
                            for _ in 0..count {
                                let Some(Completed::Section(section)) = completed.pop() else {
                                    unreachable!("fragment finishes after all of its sections")
                                };
                                sections.push(section);
                            }
                            sections.reverse();
                        }
                        _ => {}
                    }
                    self.control.checkpoint()?;
                    completed.push(Completed::Statement(statement));
                }
                Task::FinishSection(section) => {
                    let Some(Completed::Statements(statements)) = completed.pop() else {
                        unreachable!("fragment section finishes after its statements")
                    };
                    completed.push(Completed::Section(ZenumlFragmentSection {
                        label: section.label.as_ref().map(|label| label.value.clone()),
                        statements,
                        body_comment: section
                            .body_comment
                            .as_ref()
                            .map(|comment| comment.value.clone()),
                        span: section.span,
                    }));
                }
            }
        }
        let Some(Completed::Statements(statements)) = completed.pop() else {
            unreachable!("root statement list is completed last")
        };
        Ok(statements)
    }

    fn resolve_statement_head(
        &mut self,
        statement: &StatementSyntax,
        context: &ResolveContext,
    ) -> (ZenumlStatement, Option<ResolveContext>) {
        let id = format!("zenuml-statement-{}", self.generated_statement_id);
        self.generated_statement_id += 1;
        let mut body_context = None;
        let kind = match &statement.kind {
            StatementKindSyntax::Message(message) => {
                let explicit_from = message.from.as_ref().map(|value| value.value.clone());
                let resolved_from = explicit_from.clone().or_else(|| context.origin.clone());
                let resolved_to = message
                    .to
                    .as_ref()
                    .map(|value| value.value.clone())
                    .or_else(|| context.owner.clone());
                self.record_ownable_from(resolved_from.as_deref());
                if let Some(from) = &resolved_from {
                    self.reference_named_participant(
                        from,
                        message.from.as_ref().map(|v| v.span),
                        false,
                        message.from_emoji.as_ref(),
                        "zenuml participant reference",
                    );
                }
                if let Some(to) = &resolved_to {
                    self.reference_named_participant(
                        to,
                        message.to.as_ref().map(|v| v.span),
                        false,
                        message.to_emoji.as_ref(),
                        "zenuml participant reference",
                    );
                }
                self.push_message_signature(&message.signature);
                if let Some(assignment) = &message.assignment {
                    self.push_payload(
                        assignment,
                        "zenuml assignment target",
                        EditorSemanticKind::Variable,
                    );
                }
                let nested_context = ResolveContext {
                    origin: resolved_to.clone().or_else(|| context.origin.clone()),
                    owner: resolved_to.clone().or_else(|| context.owner.clone()),
                    return_to: explicit_from.clone().or_else(|| context.origin.clone()),
                };
                body_context = Some(nested_context);
                ZenumlStatementKind::Message {
                    explicit_from,
                    resolved_from,
                    resolved_to,
                    label: message.signature.value.clone(),
                    assignment: message.assignment.as_ref().map(|value| value.value.clone()),
                    style: match message.style {
                        MessageStyleSyntax::Synchronous => ZenumlMessageStyle::Synchronous,
                        MessageStyleSyntax::Asynchronous => ZenumlMessageStyle::Asynchronous,
                    },
                    body: Vec::new(),
                    body_comment: message
                        .body_comment
                        .as_ref()
                        .map(|comment| comment.value.clone()),
                }
            }
            StatementKindSyntax::Creation(creation) => {
                let resolved_from = context.origin.clone();
                let resolved_to = creation.assignment.as_ref().map_or_else(
                    || creation.constructor.value.clone(),
                    |assignment| format!("{}:{}", assignment.value, creation.constructor.value),
                );
                self.record_ownable_from(resolved_from.as_deref());
                if let Some(from) = &resolved_from {
                    self.reference_named_participant(
                        from,
                        None,
                        false,
                        None,
                        "zenuml participant reference",
                    );
                }
                self.reference_named_participant(
                    &resolved_to,
                    Some(creation.constructor.span),
                    false,
                    None,
                    "zenuml participant reference",
                );
                self.push_payload(
                    &creation.constructor,
                    "zenuml constructor",
                    EditorSemanticKind::Class,
                );
                if let Some(assignment) = &creation.assignment {
                    self.push_payload(
                        assignment,
                        "zenuml assignment target",
                        EditorSemanticKind::Variable,
                    );
                }
                let nested_context = ResolveContext {
                    origin: Some(resolved_to.clone()),
                    owner: Some(resolved_to.clone()),
                    return_to: resolved_from.clone(),
                };
                body_context = Some(nested_context);
                let parameters = creation
                    .parameters
                    .as_ref()
                    .map_or_else(String::new, |parameters| parameters.value.clone());
                ZenumlStatementKind::Creation {
                    resolved_from,
                    resolved_to,
                    constructor: creation.constructor.value.clone(),
                    parameters: parameters.clone(),
                    assignment: creation
                        .assignment
                        .as_ref()
                        .map(|value| value.value.clone()),
                    label: if parameters.is_empty() {
                        "«create»".to_string()
                    } else {
                        format!("«{parameters}»")
                    },
                    body: Vec::new(),
                    body_comment: creation
                        .body_comment
                        .as_ref()
                        .map(|comment| comment.value.clone()),
                }
            }
            StatementKindSyntax::Return(ret) => {
                let explicit_from = ret.from.as_ref().map(|value| value.value.clone());
                let resolved_from = explicit_from.clone().or_else(|| context.origin.clone());
                let explicit_to = ret.to.as_ref().map(|value| value.value.clone());
                let resolved_to = explicit_to.clone().or_else(|| context.return_to.clone());
                self.record_ownable_from(resolved_from.as_deref());
                if let Some(from) = &resolved_from {
                    self.reference_named_participant(
                        from,
                        ret.from.as_ref().map(|value| value.span),
                        false,
                        ret.from_emoji.as_ref(),
                        "zenuml participant reference",
                    );
                }
                if let Some(to) = &resolved_to {
                    self.reference_named_participant(
                        to,
                        ret.to.as_ref().map(|value| value.span),
                        false,
                        ret.to_emoji.as_ref(),
                        "zenuml participant reference",
                    );
                }
                if let Some(value) = &ret.value {
                    self.push_payload(value, "zenuml return value", EditorSemanticKind::String);
                }
                ZenumlStatementKind::Return {
                    explicit_from,
                    resolved_from,
                    explicit_to,
                    resolved_to,
                    label: ret
                        .value
                        .as_ref()
                        .map_or_else(String::new, |value| value.value.clone()),
                }
            }
            StatementKindSyntax::Fragment(fragment) => {
                let fragment_kind = match fragment.kind {
                    FragmentKindSyntax::Loop => ZenumlFragmentKind::Loop,
                    FragmentKindSyntax::Alternative => ZenumlFragmentKind::Alternative,
                    FragmentKindSyntax::Parallel => ZenumlFragmentKind::Parallel,
                    FragmentKindSyntax::Optional => ZenumlFragmentKind::Optional,
                    FragmentKindSyntax::Critical => ZenumlFragmentKind::Critical,
                    FragmentKindSyntax::Section => ZenumlFragmentKind::Section,
                    FragmentKindSyntax::TryCatchFinally => ZenumlFragmentKind::TryCatchFinally,
                };
                if let Some(label) = &fragment.label {
                    self.push_payload(
                        label,
                        "zenuml fragment condition",
                        EditorSemanticKind::String,
                    );
                }
                ZenumlStatementKind::Fragment {
                    fragment_kind,
                    label: fragment.label.as_ref().map(|label| label.value.clone()),
                    sections: Vec::new(),
                }
            }
            StatementKindSyntax::Reference(reference) => {
                let label = reference
                    .participants
                    .first()
                    .map(|name| name.value.clone())
                    .unwrap_or_default();
                let participants = reference.participants.iter().skip(1);
                for participant in participants {
                    self.reference_participant(participant, false, "zenuml participant reference");
                }
                ZenumlStatementKind::Reference {
                    participants: reference
                        .participants
                        .iter()
                        .skip(1)
                        .map(|participant| participant.value.clone())
                        .collect(),
                    label,
                }
            }
            StatementKindSyntax::Divider(label) => {
                self.push_payload(label, "zenuml divider", EditorSemanticKind::String);
                ZenumlStatementKind::Divider {
                    label: label.value.clone(),
                }
            }
        };
        (
            ZenumlStatement {
                id,
                comment: statement
                    .comment
                    .as_ref()
                    .map(|comment| comment.value.clone()),
                span: statement.span,
                kind,
            },
            body_context,
        )
    }

    fn reference_participant(&mut self, value: &SpannedText, starter: bool, detail: &str) {
        self.reference_named_participant(&value.value, Some(value.span), starter, None, detail);
    }

    fn reference_named_participant(
        &mut self,
        name: &str,
        span: Option<SourceSpan>,
        starter: bool,
        emoji: Option<&SpannedText>,
        detail: &str,
    ) {
        let (entry, is_implicit_definition) = match self.participants.entry(name.to_string()) {
            indexmap::map::Entry::Vacant(entry) => (
                entry.insert(ParticipantAccumulator {
                    participant: ZenumlParticipant {
                        name: name.to_string(),
                        label: None,
                        participant_type: None,
                        stereotype: None,
                        emoji: None,
                        width_source: None,
                        color: None,
                        comment: None,
                        group_id: None,
                        explicit: false,
                        is_starter: false,
                        declaration_span: None,
                        occurrences: Vec::new(),
                    },
                }),
                true,
            ),
            indexmap::map::Entry::Occupied(entry) => (entry.into_mut(), false),
        };
        entry.participant.is_starter |= starter;
        if entry.participant.emoji.is_none() {
            entry.participant.emoji = emoji.map(|emoji| emoji.value.clone());
        }
        if let Some(span) = span {
            entry.participant.occurrences.push(span);
            let symbol = if is_implicit_definition {
                EditorSemanticSymbol::new(
                    name,
                    Some("zenuml implicit participant".to_string()),
                    EditorSemanticKind::Event,
                    span,
                    span,
                )
            } else {
                EditorSemanticSymbol::reference(
                    name,
                    Some(detail.to_string()),
                    EditorSemanticKind::Event,
                    span,
                    span,
                )
            };
            self.facts.push_symbol(symbol);
        }
        if let Some(emoji) = emoji {
            self.push_payload(
                emoji,
                "zenuml participant emoji",
                EditorSemanticKind::String,
            );
        }
    }

    fn record_ownable_from(&mut self, from: Option<&str>) {
        self.ownable_statement_count += 1;
        self.some_statement_misses_from |= from.is_none();
    }

    fn push_message_signature(&mut self, signature: &SpannedText) {
        let (method, span) = signature_method(signature);
        self.facts.push_symbol(EditorSemanticSymbol::with_role(
            method,
            Some("zenuml message".to_string()),
            EditorSemanticKind::Function,
            EditorSemanticRole::Payload,
            span,
            span,
        ));
    }

    fn push_entity(&mut self, value: &SpannedText, detail: &str) {
        self.facts.push_symbol(EditorSemanticSymbol::new(
            value.value.clone(),
            Some(detail.to_string()),
            EditorSemanticKind::Event,
            value.span,
            value.span,
        ));
    }

    fn push_outline(&mut self, value: &SpannedText, detail: &str, kind: EditorSemanticKind) {
        self.facts.push_symbol(EditorSemanticSymbol::outline(
            value.value.clone(),
            Some(detail.to_string()),
            kind,
            value.span,
            value.span,
        ));
    }

    fn push_payload(&mut self, value: &SpannedText, detail: &str, kind: EditorSemanticKind) {
        self.facts.push_symbol(EditorSemanticSymbol::payload(
            value.value.clone(),
            Some(detail.to_string()),
            kind,
            value.span,
            value.span,
        ));
    }
}

fn signature_method(signature: &SpannedText) -> (String, SourceSpan) {
    let text = signature.value.as_str();
    let end = text
        .char_indices()
        .find_map(|(index, ch)| matches!(ch, '(' | '.').then_some(index))
        .unwrap_or(text.len());
    let method = text[..end].trim();
    let leading = text[..end]
        .len()
        .saturating_sub(text[..end].trim_start().len());
    (
        method.to_string(),
        SourceSpan::new(
            signature.span.start + leading,
            signature.span.start + leading + method.len(),
        ),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn nested_message_ownership_is_not_lowered_through_sequence() {
        let source = "zenuml\n@Starter(Client)\nA.one() {\n  B.two()\n}\n";
        let tokens = super::super::lexer::lex(source);
        let parsed = super::super::parser::parse(source, &tokens);
        let built = build(parsed);
        let ZenumlStatementKind::Message {
            resolved_from,
            resolved_to,
            body,
            ..
        } = &built.model.statements[0].kind
        else {
            panic!("expected message");
        };
        assert_eq!(
            (resolved_from.as_deref(), resolved_to.as_deref()),
            (Some("Client"), Some("A"))
        );
        let ZenumlStatementKind::Message {
            resolved_from,
            resolved_to,
            ..
        } = &body[0].kind
        else {
            panic!("expected nested message");
        };
        assert_eq!(
            (resolved_from.as_deref(), resolved_to.as_deref()),
            (Some("A"), Some("B"))
        );
    }

    #[test]
    fn iterative_resolution_preserves_section_order_and_nested_contexts() {
        let source = concat!(
            "zenuml\n@Starter(Client)\n",
            "A.outer() {\n",
            "if(ok) { B.first() { return ready } } else { made = new C { local() return done } }\n",
            "A.tail()\n}\nAfter.last()\n",
        );
        let tokens = super::super::lexer::lex(source);
        let parsed = super::super::parser::parse(source, &tokens);
        assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
        let built = build(parsed);
        assert_eq!(
            built
                .model
                .participants
                .iter()
                .map(|participant| participant.name.as_str())
                .collect::<Vec<_>>(),
            ["Client", "A", "B", "made:C", "After"]
        );
        let ZenumlStatementKind::Message { body, .. } = &built.model.statements[0].kind else {
            panic!("outer message")
        };
        assert_eq!(built.model.statements[0].id, "zenuml-statement-0");
        assert_eq!(built.model.statements[1].id, "zenuml-statement-8");
        let ZenumlStatementKind::Fragment {
            label, sections, ..
        } = &body[0].kind
        else {
            panic!("alternative")
        };
        assert_eq!(body[0].id, "zenuml-statement-1");
        assert_eq!(label.as_deref(), Some("ok"));
        assert_eq!(
            sections
                .iter()
                .map(|section| section.label.as_deref())
                .collect::<Vec<_>>(),
            [Some("ok"), Some("else")]
        );
        let ZenumlStatementKind::Message {
            resolved_from,
            resolved_to,
            body: first_body,
            ..
        } = &sections[0].statements[0].kind
        else {
            panic!("first branch message")
        };
        assert_eq!(
            (resolved_from.as_deref(), resolved_to.as_deref()),
            (Some("A"), Some("B"))
        );
        let ZenumlStatementKind::Return {
            resolved_from,
            resolved_to,
            label,
            ..
        } = &first_body[0].kind
        else {
            panic!("nested return")
        };
        assert_eq!(
            (
                resolved_from.as_deref(),
                resolved_to.as_deref(),
                label.as_str()
            ),
            (Some("B"), Some("A"), "ready")
        );
        let ZenumlStatementKind::Creation {
            resolved_from,
            resolved_to,
            body: created_body,
            label,
            ..
        } = &sections[1].statements[0].kind
        else {
            panic!("second branch creation")
        };
        assert_eq!(
            (
                resolved_from.as_deref(),
                resolved_to.as_str(),
                label.as_str()
            ),
            (Some("A"), "made:C", "«create»")
        );
        let ZenumlStatementKind::Message {
            resolved_from,
            resolved_to,
            ..
        } = &created_body[0].kind
        else {
            panic!("created owner local message")
        };
        assert_eq!(
            (resolved_from.as_deref(), resolved_to.as_deref()),
            (Some("made:C"), Some("made:C"))
        );
        let ZenumlStatementKind::Return {
            resolved_from,
            resolved_to,
            ..
        } = &created_body[1].kind
        else {
            panic!("created owner return")
        };
        assert_eq!(
            (resolved_from.as_deref(), resolved_to.as_deref()),
            (Some("made:C"), Some("A"))
        );
        assert_eq!(body[1].id, "zenuml-statement-7");
    }

    #[test]
    fn reference_uses_its_first_name_as_the_label() {
        let source = "zenuml\nref(Order, A, B)";
        let tokens = super::super::lexer::lex(source);
        let parsed = super::super::parser::parse(source, &tokens);
        let built = build(parsed);

        let ZenumlStatementKind::Reference {
            label,
            participants,
        } = &built.model.statements[0].kind
        else {
            panic!("expected reference");
        };
        assert_eq!(label, "Order");
        assert_eq!(
            participants.iter().map(String::as_str).collect::<Vec<_>>(),
            ["A", "B"]
        );
        assert!(built.model.participant("Order").is_none());
        assert!(built.model.participant("A").is_some());
        assert!(built.model.participant("B").is_some());
    }

    #[test]
    fn explicit_and_implicit_participants_have_one_entity_then_references() {
        let source = concat!(
            "zenuml\n",
            "@Actor Declared\n",
            "Declared->Implicit.call()\n",
            "Implicit->Declared.reply()\n",
        );
        let tokens = super::super::lexer::lex(source);
        let parsed = super::super::parser::parse(source, &tokens);
        let built = build(parsed);

        let declared: Vec<_> = built
            .editor_facts
            .symbols
            .iter()
            .filter(|symbol| symbol.name == "Declared")
            .collect();
        let implicit: Vec<_> = built
            .editor_facts
            .symbols
            .iter()
            .filter(|symbol| symbol.name == "Implicit")
            .collect();
        assert_eq!(declared.len(), 3);
        assert_eq!(declared[0].role, EditorSemanticRole::Entity);
        assert!(
            declared[1..]
                .iter()
                .all(|symbol| symbol.role == EditorSemanticRole::Reference)
        );
        assert_eq!(implicit.len(), 2);
        assert_eq!(implicit[0].role, EditorSemanticRole::Entity);
        assert_eq!(implicit[1].role, EditorSemanticRole::Reference);
        assert_eq!(
            implicit[0].detail.as_deref(),
            Some("zenuml implicit participant")
        );
    }

    #[test]
    fn reference_statement_participants_establish_then_reuse_implicit_entities() {
        let source = "zenuml\nref(Order, A, A)";
        let tokens = super::super::lexer::lex(source);
        let parsed = super::super::parser::parse(source, &tokens);
        let built = build(parsed);
        let occurrences: Vec<_> = built
            .editor_facts
            .symbols
            .iter()
            .filter(|symbol| symbol.name == "A")
            .collect();

        assert_eq!(occurrences.len(), 2);
        assert_eq!(occurrences[0].role, EditorSemanticRole::Entity);
        assert_eq!(occurrences[1].role, EditorSemanticRole::Reference);
    }

    #[test]
    fn accessibility_like_async_messages_remain_zenuml_messages() {
        let source = "zenuml\naccTitle: Accessible";
        let tokens = super::super::lexer::lex(source);
        let parsed = super::super::parser::parse(source, &tokens);
        let built = build(parsed);

        let ZenumlStatementKind::Message {
            resolved_to,
            label,
            style,
            ..
        } = &built.model.statements[0].kind
        else {
            panic!("expected async message");
        };
        assert_eq!(resolved_to.as_deref(), Some("accTitle"));
        assert_eq!(label, "Accessible");
        assert_eq!(*style, ZenumlMessageStyle::Asynchronous);
        assert!(built.model.participant("accTitle").is_some());
    }
}
