use crate::models::class_diagram as class_typed;
use crate::{
    EditorExpectedSyntax, EditorExpectedSyntaxKind, EditorSemanticFacts, EditorSemanticKind,
    EditorSemanticSymbol, Error, OperationControl, OperationControlResult, ParseMetadata, Result,
    SourceSpan,
    editor::{
        editor_keyword_value_span, format_lalrpop_parse_error, has_ascii_separator,
        lalrpop_parse_diagnostic, lalrpop_recovery_span, line_content_end, source_value_span,
        trailing_ascii_whitespace_slot,
    },
};
use serde_json::Value;
#[cfg(test)]
use std::cell::Cell;
use std::collections::HashSet;

use super::ast::{ActionArena, Actions};
use super::class_grammar;
use super::db::ClassDb;
use super::lexer::Lexer;
use super::{MERMAID_DOM_ID_PREFIX, Tok};

#[cfg(test)]
thread_local! {
    static CLASS_SYNTAX_CONSTRUCTION_COUNT: Cell<usize> = const { Cell::new(0) };
}

#[cfg(test)]
pub(crate) fn reset_class_syntax_construction_count() {
    CLASS_SYNTAX_CONSTRUCTION_COUNT.set(0);
}

#[cfg(test)]
pub(crate) fn class_syntax_construction_count() -> usize {
    CLASS_SYNTAX_CONSTRUCTION_COUNT.get()
}

type ClassLexicalEvent = std::result::Result<(usize, Tok, usize), super::LexError>;
type ClassGrammarError = lalrpop_util::ParseError<usize, Tok, super::LexError>;

struct ClassSyntax {
    events: Vec<ClassLexicalEvent>,
}

impl ClassSyntax {
    fn lex(code: &str, control: &OperationControl) -> OperationControlResult<Self> {
        #[cfg(test)]
        CLASS_SYNTAX_CONSTRUCTION_COUNT.set(CLASS_SYNTAX_CONSTRUCTION_COUNT.get() + 1);

        let events = crate::diagrams::scan::collect_lexical_events(&mut Lexer::new(code), control)?;
        Ok(Self { events })
    }

    fn into_editor_facts_and_actions(
        self,
        code: &str,
        control: &OperationControl,
    ) -> OperationControlResult<(
        EditorSemanticFacts,
        std::result::Result<Actions, ClassGrammarError>,
    )> {
        let Self { events } = self;
        let editor_facts = collect_class_editor_facts_from_events(&events, code, control)?;
        control.checkpoint()?;
        let mut emitted = 0usize;
        let controlled_events = events.into_iter().take_while(|_| {
            let active = !emitted.is_multiple_of(128) || control.checkpoint().is_ok();
            emitted = emitted.saturating_add(1);
            active
        });
        let mut arena = ActionArena::default();
        let actions = class_grammar::ActionsParser::new()
            .parse(&mut arena, control, controlled_events)
            .map(|list| arena.into_actions(list));
        control.checkpoint()?;
        Ok((editor_facts, actions))
    }
}

struct ClassSemanticSource<'a> {
    db: ClassDb<'a>,
    editor_facts: EditorSemanticFacts,
}

enum ClassSemanticFailure {
    Grammar {
        error: ClassGrammarError,
        editor_facts: EditorSemanticFacts,
    },
    Db {
        message: String,
        editor_facts: EditorSemanticFacts,
    },
}

impl ClassSemanticFailure {
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
        self,
        diagram_type: &str,
        fallback_offset: usize,
    ) -> (Error, EditorSemanticFacts) {
        match self {
            Self::Grammar {
                error,
                mut editor_facts,
            } => {
                let parse_error = Error::diagram_parse_diagnostic(
                    diagram_type.to_string(),
                    lalrpop_parse_diagnostic(&error, fallback_offset),
                );
                let span = match &error {
                    lalrpop_util::ParseError::User { error } => error.span,
                    _ => lalrpop_recovery_span(&error, fallback_offset),
                };
                editor_facts.mark_recovered_from_parse_error(
                    format!(
                        "class parser recovered after parse error: {}",
                        format_lalrpop_parse_error(&error)
                    ),
                    Some(span),
                );
                (parse_error, editor_facts)
            }
            Self::Db {
                message,
                mut editor_facts,
            } => {
                let parse_error =
                    Error::diagram_parse_fallback(diagram_type.to_string(), message.clone());
                editor_facts.mark_recovered_from_parse_error(
                    format!("class semantic construction failed: {message}"),
                    None,
                );
                (parse_error, editor_facts)
            }
        }
    }
}

fn construct_class_semantic_source<'a>(
    code: &str,
    meta: &'a ParseMetadata,
    control: &OperationControl,
) -> OperationControlResult<std::result::Result<ClassSemanticSource<'a>, Box<ClassSemanticFailure>>>
{
    let syntax = ClassSyntax::lex(code, control)?;
    let (editor_facts, actions) = syntax.into_editor_facts_and_actions(code, control)?;
    let actions = match actions {
        Ok(actions) => actions,
        Err(error) => {
            return Ok(Err(Box::new(ClassSemanticFailure::Grammar {
                error,
                editor_facts,
            })));
        }
    };

    let mut db = ClassDb::new(&meta.effective_config);
    for (index, action) in actions.into_iter().enumerate() {
        if index % 128 == 0 {
            control.checkpoint()?;
        }
        if let Err(message) = db.apply(action) {
            return Ok(Err(Box::new(ClassSemanticFailure::Db {
                message,
                editor_facts,
            })));
        }
    }

    control.checkpoint()?;
    Ok(Ok(ClassSemanticSource { db, editor_facts }))
}

fn parse_class_semantic_source<'a>(
    code: &str,
    meta: &'a ParseMetadata,
) -> Result<ClassSemanticSource<'a>> {
    construct_class_semantic_source(code, meta, &OperationControl::new())
        .expect("a private parse control cannot be cancelled")
        .map_err(|failure| (*failure).into_parse_error(meta, code.len()))
}

pub(crate) fn parse_class(code: &str, meta: &ParseMetadata) -> Result<Value> {
    Ok(parse_class_semantic_source(code, meta)?.db.into_model(meta))
}

#[cfg(test)]
pub(crate) fn parse_class_typed(
    code: &str,
    meta: &ParseMetadata,
) -> Result<class_typed::ClassDiagram> {
    Ok(parse_class_semantic_source(code, meta)?
        .db
        .into_typed_model(meta))
}

pub(crate) fn parse_class_typed_controlled(
    code: &str,
    meta: &ParseMetadata,
    control: &OperationControl,
) -> OperationControlResult<Result<class_typed::ClassDiagram>> {
    let construction = construct_class_semantic_source(code, meta, control)?;
    match construction {
        Ok(source) => Ok(Ok(source.db.into_typed_model(meta))),
        Err(failure) => Ok(Err((*failure).into_parse_error(meta, code.len()))),
    }
}

pub(crate) fn parse_class_json_and_editor_facts(
    code: &str,
    meta: &ParseMetadata,
    control: &OperationControl,
) -> OperationControlResult<crate::family::CombinedSemanticParse> {
    let construction = construct_class_semantic_source(code, meta, control)?;
    let parsed = crate::family::CombinedSemanticParse::from_construction(
        construction,
        |ClassSemanticSource { db, editor_facts }| (Ok(db.into_model(meta)), editor_facts),
        |failure| (*failure).into_error_and_editor_facts(meta, code.len()),
    );
    control.checkpoint()?;
    Ok(parsed)
}

fn collect_class_editor_facts_from_events(
    events: &[ClassLexicalEvent],
    code: &str,
    control: &OperationControl,
) -> OperationControlResult<EditorSemanticFacts> {
    let mut facts = EditorSemanticFacts::new();
    let mut collector = ClassEditorFactCollector::new(code);

    for (index, event) in events.iter().enumerate() {
        if index % 128 == 0 {
            control.checkpoint()?;
        }
        match event {
            Ok((start, token, end)) => collector.accept(token, *start, *end, &mut facts),
            Err(error) => {
                facts.mark_recovered();
                if let Some(expected) = error.expected_syntax.as_ref() {
                    facts.push_expected_syntax(*expected);
                }
            }
        }
    }
    collector.finish(code.len(), &mut facts);

    control.checkpoint()?;
    Ok(facts)
}

#[derive(Debug)]
struct ClassEditorFactCollector<'a> {
    code: &'a str,
    expected_name: Option<PendingClassExpectation>,
    pending_relation_source: Option<ClassTokenSymbol>,
    after_annotation_start: bool,
    css_class_targets_pending: bool,
    interaction: Option<ClassInteractionKind>,
    interaction_action_seen: bool,
    interaction_action_end: Option<usize>,
    callback_statement_function_seen: bool,
    line_payload: Option<ClassLinePayloadKind>,
    line_payload_anchor_end: Option<usize>,
    directive_start: Option<usize>,
    interaction_target_end: Option<usize>,
    note_text_pending: bool,
    class_label_pending: bool,
    declared_entities: HashSet<String>,
}

#[derive(Debug, Clone)]
struct ClassTokenSymbol {
    name: String,
    span: SourceSpan,
}

#[derive(Debug, Clone, Copy)]
enum ExpectedClassName {
    Class,
    Namespace,
    MemberOwner,
    AnnotationName,
    NoteTarget,
    CssClassReference,
    InlineClassReference,
    StyleTarget,
    ClassDef,
    ClickTarget,
    RelationTarget,
}

#[derive(Debug, Clone, Copy)]
struct PendingClassExpectation {
    kind: ExpectedClassName,
    anchor_end: Option<usize>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ClassInteractionKind {
    Click,
    Link,
    CallbackStatement,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ClassLinePayloadKind {
    Style,
    ClassDef,
}

impl<'a> ClassEditorFactCollector<'a> {
    fn new(code: &'a str) -> Self {
        Self {
            code,
            expected_name: None,
            pending_relation_source: None,
            after_annotation_start: false,
            css_class_targets_pending: false,
            interaction: None,
            interaction_action_seen: false,
            interaction_action_end: None,
            callback_statement_function_seen: false,
            line_payload: None,
            line_payload_anchor_end: None,
            directive_start: None,
            interaction_target_end: None,
            note_text_pending: false,
            class_label_pending: false,
            declared_entities: HashSet::new(),
        }
    }

    fn accept(&mut self, token: &Tok, start: usize, end: usize, facts: &mut EditorSemanticFacts) {
        match token {
            Tok::Newline => {
                self.finish_line(line_content_end(self.code, start), facts);
                self.reset_line_state();
            }
            Tok::ClassDiagram | Tok::StructStop => self.reset_line_state(),
            Tok::ClassKw => {
                facts.push_directive_prefix("class");
                self.directive_start = Some(start);
                self.pending_relation_source = None;
                self.expect_name(ExpectedClassName::Class);
            }
            Tok::NamespaceKw => {
                self.pending_relation_source = None;
                self.expect_name(ExpectedClassName::Namespace);
            }
            Tok::Note => {
                self.note_text_pending = true;
            }
            Tok::NoteFor => {
                self.note_text_pending = true;
                self.expect_name(ExpectedClassName::NoteTarget);
            }
            Tok::CssClass => {
                facts.push_directive_prefix("cssClass");
                self.directive_start = Some(start);
                self.css_class_targets_pending = true;
            }
            Tok::StyleKw => {
                facts.push_directive_prefix("style");
                self.directive_start = Some(start);
                self.line_payload = Some(ClassLinePayloadKind::Style);
                self.expect_name(ExpectedClassName::StyleTarget);
            }
            Tok::ClassDefKw => {
                facts.push_directive_prefix("classDef");
                self.directive_start = Some(start);
                self.line_payload = Some(ClassLinePayloadKind::ClassDef);
                self.expect_name_after(ExpectedClassName::ClassDef, end);
            }
            Tok::ClickKw => {
                facts.push_directive_prefix("click");
                self.directive_start = Some(start);
                self.interaction = Some(ClassInteractionKind::Click);
                self.interaction_action_seen = false;
                self.callback_statement_function_seen = false;
                self.expect_name(ExpectedClassName::ClickTarget);
            }
            Tok::LinkKw => {
                facts.push_directive_prefix("link");
                self.directive_start = Some(start);
                self.interaction = Some(ClassInteractionKind::Link);
                self.interaction_action_seen = false;
                self.callback_statement_function_seen = false;
                self.expect_name(ExpectedClassName::ClickTarget);
            }
            Tok::CallbackKw => {
                facts.push_directive_prefix("callback");
                self.directive_start = Some(start);
                self.interaction = Some(ClassInteractionKind::CallbackStatement);
                self.interaction_action_seen = false;
                self.callback_statement_function_seen = false;
                self.expect_name(ExpectedClassName::ClickTarget);
            }
            Tok::AnnotationStart => {
                self.after_annotation_start = true;
                self.pending_relation_source = None;
            }
            Tok::AnnotationStop => self.expect_name(ExpectedClassName::AnnotationName),
            Tok::Line
            | Tok::DottedLine
            | Tok::Ext
            | Tok::Dep
            | Tok::Comp
            | Tok::Agg
            | Tok::Lollipop => {
                self.push_pending_relation_source(facts);
                self.expect_name(ExpectedClassName::RelationTarget);
            }
            Tok::Label(label) => {
                if let Some(symbol) = self.pending_relation_source.take() {
                    self.push_symbol(facts, symbol, ExpectedClassName::MemberOwner);
                    self.push_label_member_symbol(facts, label, SourceSpan::new(start, end));
                } else {
                    self.push_label_payload_symbol(
                        facts,
                        label,
                        SourceSpan::new(start, end),
                        "class relation label",
                    );
                }
            }
            Tok::Name(name) => {
                if self.interaction == Some(ClassInteractionKind::Click)
                    && self.expected_name.is_none()
                    && !self.interaction_action_seen
                    && self
                        .interaction_target_end
                        .is_some_and(|target_end| has_ascii_separator(self.code, target_end, start))
                {
                    if is_class_interaction_action_prefix(name) {
                        self.push_interaction_expected_syntax(facts, start, end);
                    } else {
                        self.push_interaction_payload_expected_syntax(facts, start, end);
                        self.push_payload_symbol(
                            facts,
                            ClassTokenSymbol {
                                name: name.clone(),
                                span: SourceSpan::new(start, end),
                            },
                            "class interaction payload",
                            EditorSemanticKind::Function,
                        );
                    }
                    return;
                }
                let symbol = ClassTokenSymbol {
                    name: name.clone(),
                    span: SourceSpan::new(start, end),
                };
                if self.after_annotation_start {
                    self.after_annotation_start = false;
                    self.push_payload_symbol(
                        facts,
                        symbol,
                        "class annotation",
                        EditorSemanticKind::String,
                    );
                    return;
                }

                if let Some(expected) = self.expected_name.take() {
                    let expected = expected.kind;
                    facts.push_expected_syntax(EditorExpectedSyntax::new(
                        class_expected_syntax_kind(expected),
                        SourceSpan::new(start, end),
                    ));
                    if matches!(
                        expected,
                        ExpectedClassName::StyleTarget | ExpectedClassName::ClassDef
                    ) {
                        self.line_payload_anchor_end = Some(end);
                    }
                    if matches!(expected, ExpectedClassName::ClickTarget) {
                        self.interaction_target_end = Some(end);
                    }
                    self.push_symbol(facts, symbol, expected);
                } else {
                    self.pending_relation_source = Some(symbol);
                }
            }
            Tok::Member(member) => {
                self.push_member_symbol(facts, member, SourceSpan::new(start, end));
            }
            Tok::Str(text) => {
                if self.css_class_targets_pending {
                    self.css_class_targets_pending = false;
                    self.push_class_target_list_symbols(
                        facts,
                        text,
                        SourceSpan::new(start, end),
                        "class css target",
                    );
                    self.expect_name_after(ExpectedClassName::CssClassReference, end);
                    return;
                }

                if self.class_label_pending {
                    self.class_label_pending = false;
                    self.push_string_payload_symbol(
                        facts,
                        text,
                        SourceSpan::new(start, end),
                        "class display label",
                        EditorSemanticKind::String,
                    );
                    return;
                }

                if self.note_text_pending {
                    self.note_text_pending = false;
                    self.push_string_payload_symbol(
                        facts,
                        text,
                        SourceSpan::new(start, end),
                        "class note",
                        EditorSemanticKind::String,
                    );
                    return;
                }

                if self.pending_relation_source.is_some()
                    || matches!(
                        self.expected_name,
                        Some(PendingClassExpectation {
                            kind: ExpectedClassName::RelationTarget,
                            ..
                        })
                    )
                {
                    self.push_string_payload_symbol(
                        facts,
                        text,
                        SourceSpan::new(start, end),
                        "class relation multiplicity",
                        EditorSemanticKind::String,
                    );
                    return;
                }

                if self.interaction == Some(ClassInteractionKind::CallbackStatement)
                    && !self.callback_statement_function_seen
                {
                    self.push_interaction_payload_expected_syntax(facts, start, end);
                    self.callback_statement_function_seen = true;
                    self.push_string_payload_symbol(
                        facts,
                        text,
                        SourceSpan::new(start, end),
                        "class callback",
                        EditorSemanticKind::Function,
                    );
                    return;
                }

                if self.interaction.is_some() {
                    self.push_interaction_payload_expected_syntax(facts, start, end);
                    self.push_string_payload_symbol(
                        facts,
                        text,
                        SourceSpan::new(start, end),
                        "class interaction string",
                        EditorSemanticKind::String,
                    );
                }
            }
            Tok::LinkTarget(target) => {
                if self.interaction.is_some() {
                    self.push_interaction_payload_expected_syntax(facts, start, end);
                    self.push_payload_symbol(
                        facts,
                        ClassTokenSymbol {
                            name: target.clone(),
                            span: SourceSpan::new(start, end),
                        },
                        "class link target",
                        EditorSemanticKind::String,
                    );
                }
            }
            Tok::CallbackName(function) => {
                if self.interaction.is_some() {
                    self.push_interaction_payload_expected_syntax(facts, start, end);
                    self.push_payload_symbol(
                        facts,
                        ClassTokenSymbol {
                            name: function.clone(),
                            span: SourceSpan::new(start, end),
                        },
                        "class callback",
                        EditorSemanticKind::Function,
                    );
                }
            }
            Tok::CallbackArgs(args) => {
                if self.interaction.is_some() {
                    self.push_interaction_payload_expected_syntax(facts, start, end);
                    self.push_payload_symbol_from_token(
                        facts,
                        args,
                        SourceSpan::new(start, end),
                        "class callback args",
                        EditorSemanticKind::String,
                    );
                }
            }
            Tok::RestOfLine(raw) => {
                if let Some(kind) = self.line_payload.take() {
                    let line_end = line_content_end(self.code, end);
                    let trailing_slot = trailing_ascii_whitespace_slot(self.code, start, end)
                        .or_else(|| {
                            (raw.is_empty()
                                && self.line_payload_anchor_end.is_some_and(|anchor_end| {
                                    has_ascii_separator(self.code, anchor_end, line_end)
                                }))
                            .then_some(SourceSpan::new(line_end, line_end))
                        });
                    if let Some(span) = trailing_slot {
                        facts.push_expected_syntax(EditorExpectedSyntax::new(
                            EditorExpectedSyntaxKind::StyleValue,
                            span,
                        ));
                    }
                    let detail = match kind {
                        ClassLinePayloadKind::Style => "class style",
                        ClassLinePayloadKind::ClassDef => "class definition style",
                    };
                    let payload = raw.trim();
                    if !payload.is_empty()
                        && let Some(selection) =
                            source_value_span(self.code, SourceSpan::new(start, end), payload)
                    {
                        facts.push_expected_syntax(EditorExpectedSyntax::new(
                            EditorExpectedSyntaxKind::Payload,
                            selection,
                        ));
                    }
                    self.push_payload_symbol_from_token(
                        facts,
                        raw,
                        SourceSpan::new(start, end),
                        detail,
                        EditorSemanticKind::String,
                    );
                }
            }
            Tok::AccTitle(value) => {
                facts.push_directive_prefix("accTitle");
                self.push_payload_symbol_from_token(
                    facts,
                    value,
                    SourceSpan::new(start, end),
                    "class accessibility title",
                    EditorSemanticKind::String,
                );
            }
            Tok::AccDescr(value) | Tok::AccDescrMultiline(value) => {
                facts.push_directive_prefix("accDescr");
                self.push_payload_symbol_from_token(
                    facts,
                    value,
                    SourceSpan::new(start, end),
                    "class accessibility description",
                    EditorSemanticKind::String,
                );
            }
            Tok::Direction(_) => {
                if let Some(span) = editor_keyword_value_span(self.code, start, end, "direction") {
                    facts.push_expected_syntax(EditorExpectedSyntax::new(
                        EditorExpectedSyntaxKind::CardinalDirectionValue,
                        span,
                    ));
                }
            }
            Tok::HrefKw => {
                if self.interaction == Some(ClassInteractionKind::Click) {
                    self.push_interaction_expected_syntax(facts, start, end);
                }
            }
            Tok::CallKw => {
                if self.interaction == Some(ClassInteractionKind::Click) {
                    self.push_interaction_expected_syntax(facts, start, end);
                }
            }
            Tok::StructStart => {}
            Tok::SquareStart => {
                self.class_label_pending = true;
            }
            Tok::SquareStop => {
                self.class_label_pending = false;
            }
            Tok::StyleSeparator => {
                self.directive_start = Some(start);
                self.expect_name(ExpectedClassName::InlineClassReference);
            }
        }
    }

    fn finish(&mut self, source_len: usize, facts: &mut EditorSemanticFacts) {
        self.finish_line(line_content_end(self.code, source_len), facts);
    }

    fn finish_line(&mut self, line_end: usize, facts: &mut EditorSemanticFacts) {
        if let Some(start) = self.directive_start {
            facts.push_expected_syntax(EditorExpectedSyntax::new(
                EditorExpectedSyntaxKind::Directive,
                SourceSpan::new(start, line_end),
            ));
        }

        if let Some(expected) = self.expected_name {
            let kind = class_expected_syntax_kind(expected.kind);
            let separator_ready = expected
                .anchor_end
                .is_none_or(|anchor_end| has_ascii_separator(self.code, anchor_end, line_end));
            if !matches!(expected.kind, ExpectedClassName::ClassDef)
                && (!matches!(kind, EditorExpectedSyntaxKind::ClassName) || separator_ready)
            {
                facts.push_expected_syntax(EditorExpectedSyntax::new(
                    kind,
                    SourceSpan::new(line_end, line_end),
                ));
            }
        } else if self.line_payload.is_some()
            && self
                .line_payload_anchor_end
                .is_some_and(|anchor_end| has_ascii_separator(self.code, anchor_end, line_end))
        {
            facts.push_expected_syntax(EditorExpectedSyntax::new(
                EditorExpectedSyntaxKind::StyleValue,
                SourceSpan::new(line_end, line_end),
            ));
        }

        let expects_interaction_action = self.interaction == Some(ClassInteractionKind::Click)
            && self.expected_name.is_none()
            && !self.interaction_action_seen
            && self
                .interaction_target_end
                .is_some_and(|target_end| has_ascii_separator(self.code, target_end, line_end));
        if expects_interaction_action {
            facts.push_expected_syntax(EditorExpectedSyntax::new(
                EditorExpectedSyntaxKind::InteractionAction,
                SourceSpan::new(line_end, line_end),
            ));
            return;
        }

        let click_payload_ready = self.interaction == Some(ClassInteractionKind::Click)
            && self.expected_name.is_none()
            && self
                .interaction_action_end
                .is_some_and(|action_end| has_ascii_separator(self.code, action_end, line_end));
        let statement_payload_ready = matches!(
            self.interaction,
            Some(ClassInteractionKind::Link | ClassInteractionKind::CallbackStatement)
        ) && self.expected_name.is_none()
            && self
                .interaction_target_end
                .is_some_and(|target_end| has_ascii_separator(self.code, target_end, line_end));
        if click_payload_ready || statement_payload_ready {
            facts.push_expected_syntax(EditorExpectedSyntax::new(
                EditorExpectedSyntaxKind::Payload,
                SourceSpan::new(line_end, line_end),
            ));
        }
    }

    fn push_interaction_expected_syntax(
        &mut self,
        facts: &mut EditorSemanticFacts,
        start: usize,
        end: usize,
    ) {
        self.interaction_action_seen = true;
        self.interaction_action_end = Some(end);
        facts.push_expected_syntax(EditorExpectedSyntax::new(
            EditorExpectedSyntaxKind::InteractionAction,
            SourceSpan::new(start, end),
        ));
    }

    fn push_interaction_payload_expected_syntax(
        &mut self,
        facts: &mut EditorSemanticFacts,
        start: usize,
        end: usize,
    ) {
        if self.interaction == Some(ClassInteractionKind::Click) {
            self.interaction_action_seen = true;
        }
        facts.push_expected_syntax(EditorExpectedSyntax::new(
            EditorExpectedSyntaxKind::Payload,
            SourceSpan::new(start, end),
        ));
    }

    fn reset_line_state(&mut self) {
        self.expected_name = None;
        self.pending_relation_source = None;
        self.after_annotation_start = false;
        self.css_class_targets_pending = false;
        self.interaction = None;
        self.interaction_action_seen = false;
        self.interaction_action_end = None;
        self.callback_statement_function_seen = false;
        self.line_payload = None;
        self.line_payload_anchor_end = None;
        self.directive_start = None;
        self.interaction_target_end = None;
        self.note_text_pending = false;
        self.class_label_pending = false;
    }

    fn expect_name(&mut self, expected: ExpectedClassName) {
        self.expected_name = Some(PendingClassExpectation {
            kind: expected,
            anchor_end: None,
        });
    }

    fn expect_name_after(&mut self, expected: ExpectedClassName, anchor_end: usize) {
        self.expected_name = Some(PendingClassExpectation {
            kind: expected,
            anchor_end: Some(anchor_end),
        });
    }

    fn push_pending_relation_source(&mut self, facts: &mut EditorSemanticFacts) {
        if let Some(symbol) = self.pending_relation_source.take() {
            self.push_symbol(facts, symbol, ExpectedClassName::RelationTarget);
        }
    }

    fn push_symbol(
        &mut self,
        facts: &mut EditorSemanticFacts,
        symbol: ClassTokenSymbol,
        expected: ExpectedClassName,
    ) {
        if symbol.name.is_empty() {
            return;
        }

        let detail = match expected {
            ExpectedClassName::Class => "class",
            ExpectedClassName::Namespace => "namespace",
            ExpectedClassName::MemberOwner => "class member owner",
            ExpectedClassName::AnnotationName => "class annotation target",
            ExpectedClassName::NoteTarget => "class note target",
            ExpectedClassName::CssClassReference => "class css reference",
            ExpectedClassName::InlineClassReference => "class inline class",
            ExpectedClassName::StyleTarget => "class style target",
            ExpectedClassName::ClassDef => "class definition",
            ExpectedClassName::ClickTarget => "class interaction target",
            ExpectedClassName::RelationTarget => "class relation target",
        };
        let kind = match expected {
            ExpectedClassName::Namespace => EditorSemanticKind::Namespace,
            ExpectedClassName::ClassDef
            | ExpectedClassName::CssClassReference
            | ExpectedClassName::InlineClassReference => EditorSemanticKind::Property,
            _ => EditorSemanticKind::Class,
        };
        let selection = selection_span_for_class_name(self.code, &symbol.name, symbol.span);
        match expected {
            ExpectedClassName::Class => {
                self.declared_entities.insert(symbol.name.clone());
                facts.push_symbol(EditorSemanticSymbol::new(
                    symbol.name,
                    Some(detail.to_string()),
                    kind,
                    symbol.span,
                    selection,
                ));
            }
            ExpectedClassName::Namespace => {
                facts.push_symbol(EditorSemanticSymbol::new(
                    symbol.name,
                    Some(detail.to_string()),
                    kind,
                    symbol.span,
                    selection,
                ));
            }
            ExpectedClassName::ClassDef => {
                facts.push_symbol(EditorSemanticSymbol::class_definition(
                    symbol.name,
                    Some(detail.to_string()),
                    kind,
                    symbol.span,
                    selection,
                ))
            }
            ExpectedClassName::CssClassReference | ExpectedClassName::InlineClassReference => {
                facts.push_symbol(EditorSemanticSymbol::payload(
                    symbol.name,
                    Some(detail.to_string()),
                    kind,
                    symbol.span,
                    selection,
                ));
            }
            ExpectedClassName::MemberOwner
            | ExpectedClassName::AnnotationName
            | ExpectedClassName::RelationTarget => {
                let role = if self.declared_entities.insert(symbol.name.clone()) {
                    crate::EditorSemanticRole::Entity
                } else {
                    crate::EditorSemanticRole::Reference
                };
                facts.push_symbol(EditorSemanticSymbol::with_role(
                    symbol.name,
                    Some(detail.to_string()),
                    kind,
                    role,
                    symbol.span,
                    selection,
                ));
            }
            ExpectedClassName::NoteTarget
            | ExpectedClassName::StyleTarget
            | ExpectedClassName::ClickTarget => {
                facts.push_symbol(EditorSemanticSymbol::reference(
                    symbol.name,
                    Some(detail.to_string()),
                    kind,
                    symbol.span,
                    selection,
                ));
            }
        }
    }

    fn push_payload_symbol(
        &self,
        facts: &mut EditorSemanticFacts,
        symbol: ClassTokenSymbol,
        detail: &'static str,
        kind: EditorSemanticKind,
    ) {
        if symbol.name.is_empty() {
            return;
        }
        facts.push_symbol(EditorSemanticSymbol::payload(
            symbol.name,
            Some(detail.to_string()),
            kind,
            symbol.span,
            symbol.span,
        ));
    }

    fn push_payload_symbol_from_token(
        &self,
        facts: &mut EditorSemanticFacts,
        value: &str,
        span: SourceSpan,
        detail: &'static str,
        kind: EditorSemanticKind,
    ) {
        let value = value.trim();
        if value.is_empty() {
            return;
        }

        let selection = source_value_span(self.code, span, value).unwrap_or(span);
        facts.push_symbol(EditorSemanticSymbol::payload(
            value,
            Some(detail.to_string()),
            kind,
            span,
            selection,
        ));
    }

    fn push_class_target_list_symbols(
        &self,
        facts: &mut EditorSemanticFacts,
        value: &str,
        span: SourceSpan,
        detail: &'static str,
    ) {
        let body_start = if span.end >= span.start + value.len() + 2 {
            span.start + 1
        } else {
            span.start
        };
        let mut cursor = 0usize;
        while cursor <= value.len() {
            let next_comma = value[cursor..]
                .find(',')
                .map(|offset| cursor + offset)
                .unwrap_or(value.len());
            let part = &value[cursor..next_comma];
            let leading = part.len().saturating_sub(part.trim_start().len());
            let trailing = part.trim_end().len();
            if leading < trailing {
                let id_start = cursor + leading;
                let id_end = cursor + trailing;
                let id = &value[id_start..id_end];
                let selection = SourceSpan::new(body_start + id_start, body_start + id_end);
                facts.push_symbol(EditorSemanticSymbol::reference(
                    id,
                    Some(detail.to_string()),
                    EditorSemanticKind::Class,
                    selection,
                    selection,
                ));
            }

            if next_comma == value.len() {
                break;
            }
            cursor = next_comma + 1;
        }
    }

    fn push_member_symbol(&self, facts: &mut EditorSemanticFacts, member: &str, span: SourceSpan) {
        let Some((name, selection)) = class_member_selection(member, span) else {
            return;
        };
        facts.push_symbol(EditorSemanticSymbol::outline(
            name,
            Some("class member".to_string()),
            EditorSemanticKind::Property,
            span,
            selection,
        ));
    }

    fn push_string_payload_symbol(
        &self,
        facts: &mut EditorSemanticFacts,
        text: &str,
        span: SourceSpan,
        detail: &'static str,
        kind: EditorSemanticKind,
    ) {
        if text.is_empty() {
            return;
        }
        let selection = if span.end >= span.start + text.len() + 2 {
            SourceSpan::new(span.start + 1, span.start + 1 + text.len())
        } else {
            span
        };
        facts.push_symbol(EditorSemanticSymbol::payload(
            text,
            Some(detail.to_string()),
            kind,
            span,
            selection,
        ));
    }

    fn push_label_payload_symbol(
        &self,
        facts: &mut EditorSemanticFacts,
        label: &str,
        span: SourceSpan,
        detail: &'static str,
    ) {
        let Some((name, selection)) = class_label_member_selection(label, span) else {
            return;
        };
        facts.push_symbol(EditorSemanticSymbol::payload(
            name,
            Some(detail.to_string()),
            EditorSemanticKind::String,
            span,
            selection,
        ));
    }

    fn push_label_member_symbol(
        &self,
        facts: &mut EditorSemanticFacts,
        label: &str,
        span: SourceSpan,
    ) {
        let Some((name, selection)) = class_label_member_selection(label, span) else {
            return;
        };
        facts.push_symbol(EditorSemanticSymbol::outline(
            name,
            Some("class member".to_string()),
            EditorSemanticKind::Property,
            span,
            selection,
        ));
    }
}

fn class_expected_syntax_kind(expected: ExpectedClassName) -> EditorExpectedSyntaxKind {
    match expected {
        ExpectedClassName::ClassDef
        | ExpectedClassName::CssClassReference
        | ExpectedClassName::InlineClassReference => EditorExpectedSyntaxKind::ClassName,
        ExpectedClassName::Class
        | ExpectedClassName::Namespace
        | ExpectedClassName::MemberOwner
        | ExpectedClassName::AnnotationName
        | ExpectedClassName::NoteTarget
        | ExpectedClassName::StyleTarget
        | ExpectedClassName::ClickTarget
        | ExpectedClassName::RelationTarget => EditorExpectedSyntaxKind::NodeIdentifier,
    }
}

fn is_class_interaction_action_prefix(value: &str) -> bool {
    !value.is_empty()
        && ["href", "call"]
            .iter()
            .any(|action| action.starts_with(value))
}

fn selection_span_for_class_name(code: &str, name: &str, span: SourceSpan) -> SourceSpan {
    if let Some(raw) = name.strip_prefix(MERMAID_DOM_ID_PREFIX) {
        if let Some(source) = code.get(span.start..span.end)
            && source.starts_with('`')
            && source.ends_with('`')
            && span.end >= span.start + raw.len() + 2
        {
            return SourceSpan::new(span.start + 1, span.start + 1 + raw.len());
        }
        return SourceSpan::new(span.start, span.start + raw.len());
    }

    if span.end > span.start + 1 {
        return SourceSpan::new(span.start, span.end);
    }

    span
}

fn class_member_selection(member: &str, span: SourceSpan) -> Option<(String, SourceSpan)> {
    let trimmed_start = member.len().saturating_sub(member.trim_start().len());
    let text = &member[trimmed_start..];
    let trimmed_len = text.trim_end().len();
    if trimmed_len == 0 {
        return None;
    }

    let text = &text[..trimmed_len];
    Some((
        text.to_string(),
        SourceSpan::new(
            span.start + trimmed_start,
            span.start + trimmed_start + text.len(),
        ),
    ))
}

fn class_label_member_selection(label: &str, span: SourceSpan) -> Option<(String, SourceSpan)> {
    let after_colon = label.strip_prefix(':').unwrap_or(label);
    let colon_offset = usize::from(label.starts_with(':'));
    let leading = after_colon
        .len()
        .saturating_sub(after_colon.trim_start().len());
    let text = &after_colon[leading..];
    let trimmed_len = text.trim_end().len();
    if trimmed_len == 0 {
        return None;
    }

    let text = &text[..trimmed_len];
    let start = span.start + colon_offset + leading;
    Some((text.to_string(), SourceSpan::new(start, start + text.len())))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fmt::Write as _;

    fn assert_carrier_work(source: &str, expected: (usize, usize, usize, usize)) {
        let control = OperationControl::new();
        let mut arena = ActionArena::default();
        let root = class_grammar::ActionsParser::new()
            .parse(&mut arena, &control, Lexer::new(source))
            .expect("Class grammar accepts carrier workload");
        assert_eq!(arena.carrier_counts(), expected);
        let materialized = arena.into_actions(root).count();
        assert_eq!(materialized, expected.0);
        println!(
            "source_bytes={} action_records={} action_links={} class_id_records={} \
             class_id_links={} replayed_events={materialized}",
            source.len(),
            expected.0,
            expected.1,
            expected.2,
            expected.3,
        );
    }

    #[test]
    fn grammar_carriers_attach_each_event_once_at_increasing_width_and_depth() {
        for size in [8, 128, 1_024] {
            let mut flat = String::from("classDiagram\n");
            for index in 0..size {
                writeln!(flat, "class C{index}").unwrap();
            }
            assert_carrier_work(&flat, (size, size - 1, 0, 0));

            let mut namespace = String::from("classDiagram\nnamespace N {\n");
            for index in 0..size {
                writeln!(namespace, "class C{index}").unwrap();
            }
            namespace.push_str("}\n");
            assert_carrier_work(&namespace, (size + 3, size + 2, size, size - 1));

            let mut nested = String::from("classDiagram\n");
            for index in 0..size {
                writeln!(nested, "namespace N{index} {{").unwrap();
            }
            nested.push_str("class Leaf\n");
            nested.push_str(&"}\n".repeat(size));
            assert_carrier_work(&nested, (size * 3 + 1, size * 3, 1, 0));
        }
    }

    #[test]
    fn grammar_carrier_reductions_observe_cancellation() {
        let source = format!("classDiagram\n{}", "class C\n".repeat(1_024));
        let control = OperationControl::new();
        control.cancel_after_checkpoints(1);
        let mut arena = ActionArena::default();
        let error = class_grammar::ActionsParser::new()
            .parse(&mut arena, &control, Lexer::new(&source))
            .err()
            .expect("construction checkpoint interrupts reductions");
        assert!(matches!(error, lalrpop_util::ParseError::User { .. }));
        assert!(arena.carrier_counts().0 < 1_024);
        assert_eq!(
            control.checkpoint().unwrap_err().reason,
            crate::CancelReason::Requested
        );
    }

    #[test]
    fn cancelled_token_delivery_returns_terminal_before_grammar_eof() {
        let source = format!("classDiagram\n{}", "class C\n".repeat(256));
        let control = OperationControl::new();
        let syntax = ClassSyntax::lex(&source, &control).unwrap();
        control.cancel_after_checkpoints(syntax.events.len().div_ceil(128) + 2);
        let error = syntax
            .into_editor_facts_and_actions(&source, &control)
            .err()
            .expect("cancelled delivery returns the outer terminal");
        assert_eq!(error.reason, crate::CancelReason::Requested);
    }
}
