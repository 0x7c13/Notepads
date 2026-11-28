use crate::{
    EditorExpectedSyntax, EditorExpectedSyntaxKind, EditorSemanticFacts, EditorSemanticKind,
    EditorSemanticSymbol, Error, OperationControl, OperationControlResult, ParseDiagnostic,
    ParseDiagnosticSpanKind, ParseMetadata, Result, SourceSpan,
    editor::{
        format_lalrpop_parse_error, has_ascii_separator, lalrpop_parse_diagnostic,
        lalrpop_recovery_span, line_content_end, source_value_span, trailing_ascii_whitespace_slot,
    },
};
use serde_json::Value;
#[cfg(test)]
use std::cell::Cell;
use std::collections::HashSet;

use super::db::StateDb;
use super::{Lexer, StateDiagramRenderModel, StateDocument, StateStatement, Tok};

#[cfg(test)]
thread_local! {
    static STATE_SYNTAX_CONSTRUCTION_COUNT: Cell<usize> = const { Cell::new(0) };
}

#[cfg(test)]
pub(crate) fn reset_state_syntax_construction_count() {
    STATE_SYNTAX_CONSTRUCTION_COUNT.set(0);
}

#[cfg(test)]
pub(crate) fn state_syntax_construction_count() -> usize {
    STATE_SYNTAX_CONSTRUCTION_COUNT.get()
}

type StateLexicalEvent = std::result::Result<(usize, Tok, usize), super::LexError>;
type StateGrammarError = lalrpop_util::ParseError<usize, Tok, super::LexError>;

struct StateSyntax {
    events: Vec<StateLexicalEvent>,
}

impl StateSyntax {
    fn lex(code: &str, control: &OperationControl) -> OperationControlResult<Self> {
        #[cfg(test)]
        STATE_SYNTAX_CONSTRUCTION_COUNT.set(STATE_SYNTAX_CONSTRUCTION_COUNT.get() + 1);

        let events = crate::diagrams::scan::collect_lexical_events(&mut Lexer::new(code), control)?;
        Ok(Self { events })
    }

    fn into_editor_facts_and_document(
        self,
        code: &str,
        control: &OperationControl,
    ) -> OperationControlResult<(
        EditorSemanticFacts,
        std::result::Result<StateDocument, StateGrammarError>,
    )> {
        let editor_facts = collect_state_editor_facts_from_events(&self.events, code, control)?;
        control.checkpoint()?;
        let mut emitted = 0usize;
        let controlled_events = self.events.into_iter().take_while(|_| {
            let active = !emitted.is_multiple_of(128) || control.checkpoint().is_ok();
            emitted = emitted.saturating_add(1);
            active
        });
        let mut document = StateDocument::default();
        let parsed =
            super::state_grammar::RootParser::new().parse(&mut document, controlled_events);
        let document = parsed.map(|root| {
            document.root = Some(root);
            document
        });
        control.checkpoint()?;
        Ok((editor_facts, document))
    }
}

struct StateSemanticSource {
    db: StateDb,
    editor_facts: EditorSemanticFacts,
}

struct StateSemanticFailure {
    error: Box<StateGrammarError>,
    editor_facts: Box<EditorSemanticFacts>,
}

impl StateSemanticFailure {
    fn into_parse_error(self, meta: &ParseMetadata, fallback_offset: usize) -> Error {
        self.into_error_and_editor_facts(meta, fallback_offset).0
    }

    fn into_error_and_editor_facts(
        self,
        meta: &ParseMetadata,
        fallback_offset: usize,
    ) -> (Error, EditorSemanticFacts) {
        self.into_error_and_editor_facts_for_type(&meta.diagram_type, fallback_offset)
    }

    fn into_error_and_editor_facts_for_type(
        mut self,
        diagram_type: &str,
        fallback_offset: usize,
    ) -> (Error, EditorSemanticFacts) {
        let error = Error::diagram_parse_diagnostic(
            diagram_type.to_string(),
            state_parse_diagnostic(&self.error, fallback_offset),
        );
        let span = match self.error.as_ref() {
            lalrpop_util::ParseError::User { error } => error
                .span
                .unwrap_or_else(|| SourceSpan::new(fallback_offset, fallback_offset)),
            _ => lalrpop_recovery_span(&self.error, fallback_offset),
        };
        self.editor_facts.mark_recovered_from_parse_error(
            format!(
                "state parser recovered after parse error: {}",
                format_lalrpop_parse_error(self.error.as_ref())
            ),
            Some(span),
        );
        (error, *self.editor_facts)
    }
}

fn state_parse_diagnostic(error: &StateGrammarError, fallback_offset: usize) -> ParseDiagnostic {
    let diagnostic = lalrpop_parse_diagnostic(error, fallback_offset);
    match error {
        lalrpop_util::ParseError::UnrecognizedToken {
            token: (start, Tok::Newline, end),
            ..
        } if start == end && *end == fallback_offset => diagnostic.with_span(
            SourceSpan::new(*start, *end),
            ParseDiagnosticSpanKind::InsertionPoint,
        ),
        _ => diagnostic,
    }
}

pub(crate) fn parse_state(code: &str, meta: &ParseMetadata) -> Result<Value> {
    parse_state_semantic_source(code, meta)?.db.into_model(meta)
}

pub(crate) fn parse_state_model_for_render_controlled(
    code: &str,
    meta: &ParseMetadata,
    control: &OperationControl,
) -> OperationControlResult<Result<StateDiagramRenderModel>> {
    let construction = construct_state_semantic_source(code, control)?;
    match construction {
        Ok(source) => source.db.into_model_controlled(meta, control),
        Err(failure) => Ok(Err(failure.into_parse_error(meta, code.len()))),
    }
}

pub(crate) fn parse_state_json_and_editor_facts(
    code: &str,
    meta: &ParseMetadata,
    control: &OperationControl,
) -> OperationControlResult<crate::family::CombinedSemanticParse> {
    let construction = construct_state_semantic_source(code, control)?;
    let construction = match construction {
        Ok(StateSemanticSource { db, editor_facts }) => {
            let model = match db.into_model_controlled(meta, control)? {
                Ok(model) => super::render_model::render_model_to_compat_json_controlled(
                    &model, meta, control,
                )?,
                Err(error) => Err(error),
            };
            Ok((model, editor_facts))
        }
        Err(failure) => Err(failure),
    };
    control.checkpoint()?;
    let parsed = crate::family::CombinedSemanticParse::from_construction(
        construction,
        |(model, editor_facts)| {
            (
                model.map(crate::ManagedSemanticJson::into_unmanaged_value),
                editor_facts,
            )
        },
        |failure| failure.into_error_and_editor_facts(meta, code.len()),
    );
    Ok(parsed)
}

fn parse_state_semantic_source(code: &str, meta: &ParseMetadata) -> Result<StateSemanticSource> {
    construct_state_semantic_source(code, &OperationControl::new())
        .expect("a private parse control cannot be cancelled")
        .map_err(|failure| failure.into_parse_error(meta, code.len()))
}

fn construct_state_semantic_source(
    code: &str,
    control: &OperationControl,
) -> OperationControlResult<std::result::Result<StateSemanticSource, StateSemanticFailure>> {
    let syntax = StateSyntax::lex(code, control)?;
    let (editor_facts, document) = syntax.into_editor_facts_and_document(code, control)?;
    let mut doc = match document {
        Ok(doc) => doc,
        Err(error) => {
            return Ok(Err(StateSemanticFailure {
                error: Box::new(error),
                editor_facts: Box::new(editor_facts),
            }));
        }
    };

    let mut divider_cnt = 0usize;
    assign_divider_ids(&mut doc, &mut divider_cnt, control)?;

    let mut db = StateDb::new();
    db.set_root_doc(doc, control)?;
    control.checkpoint()?;
    Ok(Ok(StateSemanticSource { db, editor_facts }))
}

pub(super) fn assign_divider_ids(
    document: &mut StateDocument,
    cnt: &mut usize,
    control: &OperationControl,
) -> OperationControlResult<()> {
    let Some(root) = document.root else {
        return Ok(());
    };
    let mut stack = vec![(root, 0usize)];
    let mut inspected = 0usize;
    while let Some((doc, index)) = stack.last_mut() {
        if inspected.is_multiple_of(128) {
            control.checkpoint()?;
        }
        let Some(stmt) = document.documents[doc.0].get_mut(*index) else {
            stack.pop();
            continue;
        };
        *index += 1;
        let mut child = None;
        match stmt {
            StateStatement::State(st) => {
                if st.ty == "divider" && st.id == "__divider__" {
                    *cnt += 1;
                    st.id = format!("divider-id-{cnt}");
                }
                child = st.doc;
            }
            StateStatement::Relation(relation) => {
                for st in [&mut relation.state1, &mut relation.state2] {
                    if st.ty == "divider" && st.id == "__divider__" {
                        *cnt += 1;
                        st.id = format!("divider-id-{cnt}");
                    }
                }
            }
            _ => {}
        }
        if let Some(child) = child {
            stack.push((child, 0));
        }
        inspected = inspected.saturating_add(1);
    }
    control.checkpoint()
}

fn state_editor_facts_from_events(events: Vec<StateEditorEvent>) -> EditorSemanticFacts {
    let mut facts = EditorSemanticFacts::new();
    for event in events {
        event.emit(&mut facts);
    }
    facts
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum StateTokenContext {
    Default,
    ClickTarget,
    ClickAfterTarget,
    ClickAfterHref,
    ClickTooltip,
}

#[derive(Debug)]
enum StateEditorEvent {
    DirectivePrefix(&'static str),
    ExpectedSyntax {
        kind: EditorExpectedSyntaxKind,
        span: SourceSpan,
    },
    Entity {
        name: String,
        selection: SourceSpan,
        detail: &'static str,
        kind: EditorSemanticKind,
    },
    Reference {
        name: String,
        selection: SourceSpan,
        detail: &'static str,
        kind: EditorSemanticKind,
    },
    ClassDefinition {
        name: String,
        selection: SourceSpan,
        detail: &'static str,
        kind: EditorSemanticKind,
    },
    Payload {
        value: String,
        span: SourceSpan,
        selection: SourceSpan,
        detail: &'static str,
        kind: EditorSemanticKind,
    },
}

impl StateEditorEvent {
    fn emit(self, facts: &mut EditorSemanticFacts) {
        match self {
            Self::DirectivePrefix(prefix) => facts.push_directive_prefix(prefix),
            Self::ExpectedSyntax { kind, span } => {
                facts.push_expected_syntax(EditorExpectedSyntax::new(kind, span))
            }
            Self::Entity {
                name,
                selection,
                detail,
                kind,
            } => facts.push_symbol(EditorSemanticSymbol::new(
                name,
                Some(detail.to_string()),
                kind,
                selection,
                selection,
            )),
            Self::Reference {
                name,
                selection,
                detail,
                kind,
            } => facts.push_symbol(EditorSemanticSymbol::reference(
                name,
                Some(detail.to_string()),
                kind,
                selection,
                selection,
            )),
            Self::ClassDefinition {
                name,
                selection,
                detail,
                kind,
            } => facts.push_symbol(EditorSemanticSymbol::class_definition(
                name,
                Some(detail.to_string()),
                kind,
                selection,
                selection,
            )),
            Self::Payload {
                value,
                span,
                selection,
                detail,
                kind,
            } => facts.push_symbol(EditorSemanticSymbol::payload(
                value,
                Some(detail.to_string()),
                kind,
                span,
                selection,
            )),
        }
    }
}

#[derive(Debug)]
struct StatePendingEntity {
    name: String,
    selection: SourceSpan,
    detail: &'static str,
    kind: EditorSemanticKind,
}

fn collect_state_editor_facts_from_events(
    lexical_events: &[StateLexicalEvent],
    code: &str,
    control: &OperationControl,
) -> OperationControlResult<EditorSemanticFacts> {
    let mut collector = StateTokenFactCollector {
        code,
        context: StateTokenContext::Default,
        pending_entity: None,
        pending_directive_slot: None,
        directive_start: None,
        interaction_target_end: None,
        note_text_pending: false,
        note_position_seen: false,
        note_alias_pending: false,
        relation_label_pending: false,
        relation_target_seen: false,
        declared_entities: HashSet::new(),
    };
    let mut events = Vec::new();
    for (index, event) in lexical_events.iter().enumerate() {
        if index % 128 == 0 {
            control.checkpoint()?;
        }
        match event {
            Ok((start, token, end)) => {
                collector.collect_token(token.clone(), *start, *end, &mut events);
            }
            Err(error) => {
                if let Some(expected) = error.expected_syntax.as_ref() {
                    events.push(StateEditorEvent::ExpectedSyntax {
                        kind: expected.kind,
                        span: expected.span,
                    });
                }
            }
        }
    }
    collector.finish_statement(line_content_end(code, code.len()), &mut events);
    control.checkpoint()?;
    Ok(state_editor_facts_from_events(events))
}

struct StateTokenFactCollector<'a> {
    code: &'a str,
    context: StateTokenContext,
    pending_entity: Option<StatePendingEntity>,
    pending_directive_slot: Option<PendingDirectiveSlot>,
    directive_start: Option<usize>,
    interaction_target_end: Option<usize>,
    note_text_pending: bool,
    note_position_seen: bool,
    note_alias_pending: bool,
    relation_label_pending: bool,
    relation_target_seen: bool,
    declared_entities: HashSet<String>,
}

#[derive(Debug, Clone, Copy)]
struct PendingDirectiveSlot {
    kind: EditorExpectedSyntaxKind,
    anchor_end: usize,
}

impl StateTokenFactCollector<'_> {
    fn collect_token(
        &mut self,
        token: Tok,
        start: usize,
        end: usize,
        events: &mut Vec<StateEditorEvent>,
    ) {
        match token {
            Tok::Newline => self.finish_statement(line_content_end(self.code, start), events),
            Tok::StructStart | Tok::StructStop => self.reset_statement(events),
            Tok::Arrow => {
                self.flush_pending_entity_for_relation(events);
                self.relation_label_pending = true;
                self.relation_target_seen = false;
            }
            Tok::Click => {
                self.flush_pending_entity(events);
                events.push(StateEditorEvent::DirectivePrefix("click"));
                self.directive_start = Some(start);
                self.expect_after_separator(EditorExpectedSyntaxKind::NodeIdentifier, end);
                self.context = StateTokenContext::ClickTarget;
            }
            Tok::Href if self.context == StateTokenContext::ClickAfterTarget => {
                push_state_expected_syntax(
                    events,
                    EditorExpectedSyntaxKind::InteractionAction,
                    SourceSpan::new(start, end),
                );
                self.context = StateTokenContext::ClickAfterHref;
                self.interaction_target_end = None;
            }
            Tok::Id(id) if self.context == StateTokenContext::ClickTarget => {
                self.pending_directive_slot = None;
                self.interaction_target_end = Some(end);
                push_state_reference_event(events, id, start, end, "state click target");
                self.context = StateTokenContext::ClickAfterTarget;
            }
            Tok::StyledId((id, class_id)) if self.context == StateTokenContext::ClickTarget => {
                self.pending_directive_slot = None;
                self.interaction_target_end = Some(end);
                push_state_reference_event(events, id, start, end, "state click target");
                push_state_payload_event_from_token_as(
                    events,
                    self.code,
                    class_id.as_str(),
                    SourceSpan::new(start, end),
                    "state inline class",
                    EditorSemanticKind::Property,
                    EditorExpectedSyntaxKind::ClassName,
                );
                self.context = StateTokenContext::ClickAfterTarget;
            }
            Tok::EdgeState if self.context == StateTokenContext::ClickTarget => {
                self.pending_directive_slot = None;
                self.interaction_target_end = Some(end);
                self.context = StateTokenContext::ClickAfterTarget;
            }
            Tok::Id(_) if self.context == StateTokenContext::ClickAfterTarget => {
                push_state_expected_syntax(
                    events,
                    EditorExpectedSyntaxKind::InteractionAction,
                    SourceSpan::new(start, end),
                );
                self.interaction_target_end = None;
                self.context = StateTokenContext::Default;
            }
            Tok::EdgeState => {
                self.flush_pending_entity(events);
                if self.relation_label_pending {
                    self.relation_target_seen = true;
                }
            }
            Tok::Note => {
                self.flush_pending_entity(events);
                self.note_text_pending = true;
                self.note_position_seen = false;
                self.note_alias_pending = false;
            }
            Tok::LeftOf | Tok::RightOf => {
                self.note_position_seen = true;
            }
            Tok::NoteText(text) if self.note_text_pending => {
                self.flush_pending_entity(events);
                push_state_payload_event_from_token(
                    events,
                    self.code,
                    text.as_str(),
                    start,
                    end,
                    "state note",
                    EditorSemanticKind::String,
                );
                self.note_alias_pending = !self.note_position_seen;
                self.note_text_pending = false;
            }
            Tok::StateDescr(text) => {
                self.flush_pending_entity(events);
                push_state_string_payload_event(
                    events,
                    text.as_str(),
                    start,
                    end,
                    "state display label",
                    EditorSemanticKind::String,
                );
            }
            Tok::Descr(text) => {
                self.flush_pending_entity(events);
                let detail = if self.relation_label_pending && self.relation_target_seen {
                    "state relation label"
                } else {
                    "state description"
                };
                push_state_payload_event_from_token(
                    events,
                    self.code,
                    text.as_str(),
                    start,
                    end,
                    detail,
                    EditorSemanticKind::String,
                );
                self.relation_label_pending = false;
                self.relation_target_seen = false;
            }
            Tok::StringLit(text)
                if matches!(
                    self.context,
                    StateTokenContext::ClickAfterTarget | StateTokenContext::ClickAfterHref
                ) =>
            {
                self.flush_pending_entity(events);
                self.interaction_target_end = None;
                push_state_string_payload_event(
                    events,
                    text.as_str(),
                    start,
                    end,
                    "state click url",
                    EditorSemanticKind::String,
                );
                self.context = if self.context == StateTokenContext::ClickAfterTarget {
                    StateTokenContext::ClickTooltip
                } else {
                    StateTokenContext::Default
                };
            }
            Tok::StringLit(text) if self.context == StateTokenContext::ClickTooltip => {
                push_state_string_payload_event(
                    events,
                    text.as_str(),
                    start,
                    end,
                    "state click tooltip",
                    EditorSemanticKind::String,
                );
                self.context = StateTokenContext::Default;
            }
            Tok::Id(id) | Tok::CompositState(id) => {
                self.handle_state_entity(id, start, end, "state", events);
            }
            Tok::StyledId((id, class_id)) => {
                self.handle_state_entity(id, start, end, "state", events);
                push_state_payload_event_from_token_as(
                    events,
                    self.code,
                    class_id.as_str(),
                    SourceSpan::new(start, end),
                    "state inline class",
                    EditorSemanticKind::Property,
                    EditorExpectedSyntaxKind::ClassName,
                );
            }
            Tok::Fork(id) => {
                self.handle_state_entity(id, start, end, "state fork", events);
            }
            Tok::Join(id) => {
                self.handle_state_entity(id, start, end, "state join", events);
            }
            Tok::Choice(id) => {
                self.handle_state_entity(id, start, end, "state choice", events);
            }
            Tok::ClassDef => {
                self.flush_pending_entity(events);
                events.push(StateEditorEvent::DirectivePrefix("classDef"));
                self.directive_start = Some(start);
                self.expect_after_separator(EditorExpectedSyntaxKind::ClassName, end);
            }
            Tok::ClassDefId(id) => {
                if !id.trim().is_empty() {
                    push_state_expected_syntax(
                        events,
                        EditorExpectedSyntaxKind::ClassName,
                        SourceSpan::new(start, end),
                    );
                    self.expect_after_separator(EditorExpectedSyntaxKind::StyleValue, end);
                }
                push_state_class_definition_event(
                    events,
                    id,
                    start,
                    end,
                    "state class definition",
                    EditorSemanticKind::Property,
                );
            }
            Tok::ClassDefStyleOpts(raw) => {
                if let Some(span) = trailing_ascii_whitespace_slot(self.code, start, end) {
                    push_state_expected_syntax(events, EditorExpectedSyntaxKind::StyleValue, span);
                }
                self.pending_directive_slot = None;
                push_state_payload_event_from_token(
                    events,
                    self.code,
                    raw.as_str(),
                    start,
                    end,
                    "state class definition style",
                    EditorSemanticKind::String,
                );
            }
            Tok::Class => {
                self.flush_pending_entity(events);
                events.push(StateEditorEvent::DirectivePrefix("class"));
                self.directive_start = Some(start);
                self.expect_after_separator(EditorExpectedSyntaxKind::IdList, end);
            }
            Tok::ClassEntityIds(ids) => {
                let has_targets = !ids.trim().is_empty();
                push_state_reference_id_list_events(
                    events,
                    self.code,
                    ids.as_str(),
                    start,
                    end,
                    "state class target",
                    EditorExpectedSyntaxKind::IdList,
                );
                if has_targets {
                    self.expect_after_separator(EditorExpectedSyntaxKind::ClassName, end);
                }
            }
            Tok::StyleClass(class_id) => {
                if !class_id.trim().is_empty() {
                    self.pending_directive_slot = None;
                }
                push_state_payload_event_from_token_as(
                    events,
                    self.code,
                    class_id.as_str(),
                    SourceSpan::new(start, end),
                    "state class reference",
                    EditorSemanticKind::Property,
                    EditorExpectedSyntaxKind::ClassName,
                );
            }
            Tok::Style => {
                self.flush_pending_entity(events);
                events.push(StateEditorEvent::DirectivePrefix("style"));
                self.directive_start = Some(start);
                self.expect_after_separator(EditorExpectedSyntaxKind::IdList, end);
            }
            Tok::StyleIds(ids) => {
                let has_targets = !ids.trim().is_empty();
                push_state_reference_id_list_events(
                    events,
                    self.code,
                    ids.as_str(),
                    start,
                    end,
                    "state style target",
                    EditorExpectedSyntaxKind::IdList,
                );
                if has_targets {
                    self.expect_after_separator(EditorExpectedSyntaxKind::StyleValue, end);
                }
            }
            Tok::StyleDefStyleOpts(raw) => {
                if let Some(span) = trailing_ascii_whitespace_slot(self.code, start, end) {
                    push_state_expected_syntax(events, EditorExpectedSyntaxKind::StyleValue, span);
                }
                self.pending_directive_slot = None;
                push_state_payload_event_from_token(
                    events,
                    self.code,
                    raw.as_str(),
                    start,
                    end,
                    "state style",
                    EditorSemanticKind::String,
                );
            }
            Tok::AccTitle(value) => {
                self.flush_pending_entity(events);
                events.push(StateEditorEvent::DirectivePrefix("accTitle"));
                push_state_payload_event_from_token(
                    events,
                    self.code,
                    value.as_str(),
                    start,
                    end,
                    "state accessibility title",
                    EditorSemanticKind::String,
                );
            }
            Tok::AccDescr(value) | Tok::AccDescrMultiline(value) => {
                self.flush_pending_entity(events);
                events.push(StateEditorEvent::DirectivePrefix("accDescr"));
                push_state_payload_event_from_token(
                    events,
                    self.code,
                    value.as_str(),
                    start,
                    end,
                    "state accessibility description",
                    EditorSemanticKind::String,
                );
            }
            Tok::Sd
            | Tok::As
            | Tok::NoteText(_)
            | Tok::Concurrent
            | Tok::HideEmptyDescription
            | Tok::ScaleWidth(_)
            | Tok::Href
            | Tok::StringLit(_) => {}
            Tok::Direction(direction) => {
                push_state_expected_syntax(
                    events,
                    EditorExpectedSyntaxKind::CardinalDirectionValue,
                    direction.selection,
                );
            }
        }
    }

    fn handle_state_entity(
        &mut self,
        id: String,
        start: usize,
        end: usize,
        default_detail: &'static str,
        events: &mut Vec<StateEditorEvent>,
    ) {
        if self.note_alias_pending {
            self.note_alias_pending = false;
            push_state_expected_syntax(
                events,
                EditorExpectedSyntaxKind::Payload,
                SourceSpan::new(start, end),
            );
            return;
        }

        if self.note_text_pending && self.note_position_seen {
            self.push_state_relation_occurrence(id, start, end, "state note target", events);
            return;
        }

        if self.relation_label_pending {
            self.flush_pending_entity(events);
            self.push_state_relation_occurrence(id, start, end, "state reference", events);
            self.relation_target_seen = true;
            return;
        }

        self.queue_state_entity(id, start, end, default_detail, events);
    }

    fn queue_state_entity(
        &mut self,
        id: String,
        start: usize,
        end: usize,
        detail: &'static str,
        events: &mut Vec<StateEditorEvent>,
    ) {
        self.flush_pending_entity(events);
        let Some(entity) = state_pending_entity(id, start, end, detail) else {
            return;
        };
        push_state_expected_syntax(
            events,
            EditorExpectedSyntaxKind::NodeIdentifier,
            entity.selection,
        );
        self.pending_entity = Some(entity);
    }

    fn flush_pending_entity(&mut self, events: &mut Vec<StateEditorEvent>) {
        if let Some(entity) = self.pending_entity.take() {
            self.declared_entities.insert(entity.name.clone());
            events.push(StateEditorEvent::Entity {
                name: entity.name,
                selection: entity.selection,
                detail: entity.detail,
                kind: entity.kind,
            });
        }
    }

    fn flush_pending_entity_for_relation(&mut self, events: &mut Vec<StateEditorEvent>) {
        if let Some(entity) = self.pending_entity.take() {
            self.push_state_relation_occurrence_with_selection(
                entity.name,
                entity.selection,
                "state reference",
                events,
            );
        }
    }

    fn push_state_relation_occurrence(
        &mut self,
        id: String,
        start: usize,
        end: usize,
        detail: &'static str,
        events: &mut Vec<StateEditorEvent>,
    ) {
        let Some(entity) = state_pending_entity(id, start, end, detail) else {
            return;
        };
        push_state_expected_syntax(
            events,
            EditorExpectedSyntaxKind::NodeIdentifier,
            entity.selection,
        );
        self.push_state_relation_occurrence_with_selection(
            entity.name,
            entity.selection,
            entity.detail,
            events,
        );
    }

    fn push_state_relation_occurrence_with_selection(
        &mut self,
        name: String,
        selection: SourceSpan,
        detail: &'static str,
        events: &mut Vec<StateEditorEvent>,
    ) {
        let kind = EditorSemanticKind::Class;
        if self.declared_entities.insert(name.clone()) {
            events.push(StateEditorEvent::Entity {
                name,
                selection,
                detail,
                kind,
            });
        } else {
            events.push(StateEditorEvent::Reference {
                name,
                selection,
                detail,
                kind,
            });
        }
    }

    fn finish_statement(&mut self, line_end: usize, events: &mut Vec<StateEditorEvent>) {
        self.flush_pending_entity(events);
        if let Some(start) = self.directive_start {
            push_state_expected_syntax(
                events,
                EditorExpectedSyntaxKind::Directive,
                SourceSpan::new(start, line_end),
            );
        }
        if let Some(slot) = self.pending_directive_slot
            && has_ascii_separator(self.code, slot.anchor_end, line_end)
        {
            push_state_expected_syntax(events, slot.kind, SourceSpan::new(line_end, line_end));
        }
        if let Some(target_end) = self.interaction_target_end
            && has_ascii_separator(self.code, target_end, line_end)
        {
            push_state_expected_syntax(
                events,
                EditorExpectedSyntaxKind::InteractionAction,
                SourceSpan::new(line_end, line_end),
            );
        }
        self.reset_statement_state();
    }

    fn reset_statement(&mut self, events: &mut Vec<StateEditorEvent>) {
        self.flush_pending_entity(events);
        self.reset_statement_state();
    }

    fn reset_statement_state(&mut self) {
        self.context = StateTokenContext::Default;
        self.pending_directive_slot = None;
        self.directive_start = None;
        self.interaction_target_end = None;
        self.note_text_pending = false;
        self.note_position_seen = false;
        self.note_alias_pending = false;
        self.relation_label_pending = false;
        self.relation_target_seen = false;
    }

    fn expect_after_separator(&mut self, kind: EditorExpectedSyntaxKind, anchor_end: usize) {
        self.pending_directive_slot = Some(PendingDirectiveSlot { kind, anchor_end });
    }
}

fn state_pending_entity(
    id: String,
    start: usize,
    end: usize,
    detail: &'static str,
) -> Option<StatePendingEntity> {
    if !is_editor_visible_state_name(&id) {
        return None;
    }

    let selection_end = start + id.len();
    let selection = SourceSpan::new(start, selection_end.min(end));
    Some(StatePendingEntity {
        name: id,
        selection,
        detail,
        kind: EditorSemanticKind::Class,
    })
}

fn push_state_reference_event(
    events: &mut Vec<StateEditorEvent>,
    id: String,
    start: usize,
    end: usize,
    detail: &'static str,
) {
    let Some(entity) = state_pending_entity(id, start, end, detail) else {
        return;
    };
    push_state_expected_syntax(
        events,
        EditorExpectedSyntaxKind::NodeIdentifier,
        entity.selection,
    );
    events.push(StateEditorEvent::Reference {
        name: entity.name,
        selection: entity.selection,
        detail: entity.detail,
        kind: entity.kind,
    });
}

fn push_state_reference_event_with_selection(
    events: &mut Vec<StateEditorEvent>,
    id: String,
    selection: SourceSpan,
    detail: &'static str,
) {
    if !is_editor_visible_state_name(&id) {
        return;
    }

    push_state_expected_syntax(events, EditorExpectedSyntaxKind::NodeIdentifier, selection);
    events.push(StateEditorEvent::Reference {
        name: id,
        selection,
        detail,
        kind: EditorSemanticKind::Class,
    });
}

fn push_state_class_definition_event(
    events: &mut Vec<StateEditorEvent>,
    name: String,
    start: usize,
    end: usize,
    detail: &'static str,
    kind: EditorSemanticKind,
) {
    if name.trim().is_empty() {
        return;
    }

    let selection = SourceSpan::new(start, end);
    events.push(StateEditorEvent::ClassDefinition {
        name,
        selection,
        detail,
        kind,
    });
}

fn push_state_expected_syntax(
    events: &mut Vec<StateEditorEvent>,
    kind: EditorExpectedSyntaxKind,
    span: SourceSpan,
) {
    if span.start > span.end {
        return;
    }

    events.push(StateEditorEvent::ExpectedSyntax { kind, span });
}

fn push_state_reference_id_list_events(
    events: &mut Vec<StateEditorEvent>,
    code: &str,
    fallback_ids: &str,
    start: usize,
    end: usize,
    detail: &'static str,
    expected_kind: EditorExpectedSyntaxKind,
) {
    push_state_expected_syntax(events, expected_kind, SourceSpan::new(start, end));

    let Some(slice) = code.get(start..end) else {
        for id in fallback_ids
            .split(',')
            .map(str::trim)
            .filter(|id| !id.is_empty())
        {
            push_state_reference_event_with_selection(
                events,
                id.to_string(),
                SourceSpan::new(start, end),
                detail,
            );
        }
        return;
    };

    let mut cursor = 0usize;
    while cursor <= slice.len() {
        let next_comma = slice[cursor..]
            .find(',')
            .map(|offset| cursor + offset)
            .unwrap_or(slice.len());
        let part = &slice[cursor..next_comma];
        let leading = part.len().saturating_sub(part.trim_start().len());
        let trailing = part.trim_end().len();
        if leading < trailing {
            let id_start = cursor + leading;
            let id_end = cursor + trailing;
            push_state_reference_event_with_selection(
                events,
                slice[id_start..id_end].to_string(),
                SourceSpan::new(start + id_start, start + id_end),
                detail,
            );
        }

        if next_comma == slice.len() {
            break;
        }
        cursor = next_comma + 1;
    }
}

fn push_state_payload_event_from_token(
    events: &mut Vec<StateEditorEvent>,
    code: &str,
    value: &str,
    start: usize,
    end: usize,
    detail: &'static str,
    kind: EditorSemanticKind,
) {
    push_state_payload_event_from_token_as(
        events,
        code,
        value,
        SourceSpan::new(start, end),
        detail,
        kind,
        EditorExpectedSyntaxKind::Payload,
    );
}

fn push_state_payload_event_from_token_as(
    events: &mut Vec<StateEditorEvent>,
    code: &str,
    value: &str,
    span: SourceSpan,
    detail: &'static str,
    kind: EditorSemanticKind,
    expected_kind: EditorExpectedSyntaxKind,
) {
    let value = value.trim();
    if value.is_empty() {
        return;
    }

    let selection = source_value_span(code, span, value).unwrap_or(span);
    push_state_expected_syntax(events, expected_kind, selection);
    events.push(StateEditorEvent::Payload {
        value: value.to_string(),
        span,
        selection,
        detail,
        kind,
    });
}

fn push_state_string_payload_event(
    events: &mut Vec<StateEditorEvent>,
    value: &str,
    start: usize,
    end: usize,
    detail: &'static str,
    kind: EditorSemanticKind,
) {
    if value.is_empty() {
        return;
    }

    let span = SourceSpan::new(start, end);
    let selection = if end >= start + value.len() + 2 {
        SourceSpan::new(start + 1, start + 1 + value.len())
    } else {
        span
    };
    push_state_expected_syntax(events, EditorExpectedSyntaxKind::Payload, selection);
    events.push(StateEditorEvent::Payload {
        value: value.to_string(),
        span,
        selection,
        detail,
        kind,
    });
}

fn is_editor_visible_state_name(id: &str) -> bool {
    !id.trim().is_empty() && id.trim() != "[*]"
}
