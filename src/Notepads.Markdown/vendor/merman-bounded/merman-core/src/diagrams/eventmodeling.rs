use crate::common_db::{LangiumCommonDbFields, sanitize_acc_descr, sanitize_acc_title};
use crate::diagrams::langium_common::{
    LangiumCommonFacts, parse_langium_common, push_langium_common_editor_fact,
};
use crate::diagrams::scan::physical_line_at;
use crate::sanitize::sanitize_text;
use crate::{
    EditorExpectedSyntax, EditorExpectedSyntaxKind, EditorRenamePolicy, EditorSemanticFacts,
    EditorSemanticKind, EditorSemanticSymbol, Error, ParseMetadata, Result, SourceSpan,
};
use serde_json::{Value, json};
use std::collections::{HashMap, HashSet};

#[cfg(test)]
pub(crate) fn reset_eventmodeling_syntax_construction_count() {
    crate::diagrams::langium_common::reset_family_syntax_construction_count("eventmodeling");
}

#[cfg(test)]
pub(crate) fn eventmodeling_syntax_construction_count() -> usize {
    crate::diagrams::langium_common::family_syntax_construction_count("eventmodeling")
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct EventModelingFrameRenderModel {
    pub name: String,
    #[serde(rename = "frameKind")]
    pub frame_kind: String,
    #[serde(rename = "modelEntityType")]
    pub model_entity_type: String,
    #[serde(rename = "entityIdentifier")]
    pub entity_identifier: String,
    #[serde(default, rename = "sourceFrames")]
    pub source_frames: Vec<String>,
    #[serde(default, rename = "dataInlineValue")]
    pub data_inline_value: Option<String>,
    #[serde(default, rename = "dataReference")]
    pub data_reference: Option<String>,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct EventModelingDataEntityRenderModel {
    pub name: String,
    #[serde(rename = "dataBlockValue")]
    pub data_block_value: String,
}

#[derive(Debug, Clone, Default, serde::Serialize, serde::Deserialize)]
pub struct EventModelingDiagramRenderModel {
    #[serde(default, rename = "accTitle")]
    pub acc_title: Option<String>,
    #[serde(default, rename = "accDescr")]
    pub acc_descr: Option<String>,
    #[serde(default)]
    pub title: Option<String>,
    #[serde(default)]
    pub frames: Vec<EventModelingFrameRenderModel>,
    #[serde(default, rename = "dataEntities")]
    pub data_entities: Vec<EventModelingDataEntityRenderModel>,
}

impl EventModelingDiagramRenderModel {
    pub(crate) fn sanitize_common_db_fields(&mut self, config: &crate::MermaidConfig) {
        crate::common_db::sanitize_optional_title(&mut self.title, config);
        crate::common_db::sanitize_optional_acc_title(&mut self.acc_title, config);
        crate::common_db::sanitize_optional_acc_descr(&mut self.acc_descr, config);
    }
}

pub(crate) fn parse_eventmodeling(code: &str, meta: &ParseMetadata) -> Result<Value> {
    construct_eventmodeling_semantic_source(code, meta)
        .map_err(|failure| *failure.error)?
        .into_compat_json(meta)
}

pub(crate) fn parse_eventmodeling_json_and_editor_facts(
    code: &str,
    meta: &ParseMetadata,
    control: &crate::OperationControl,
) -> crate::OperationControlResult<crate::family::CombinedSemanticParse> {
    control.checkpoint()?;
    let construction =
        match construct_eventmodeling_semantic_source_controlled(code, meta, control)? {
            Ok(source) => {
                let editor_facts = source.editor_facts();
                let json = source.into_compat_json_controlled(meta, control)?;
                Ok((json, editor_facts))
            }
            Err(failure) => Err(failure),
        };
    let parsed = crate::family::CombinedSemanticParse::from_construction(
        construction,
        |(json, editor_facts)| (json, editor_facts),
        EventModelingParseFailure::into_error_and_editor_facts,
    );
    control.checkpoint()?;
    Ok(parsed)
}

pub(crate) fn render_model_to_compat_json(
    model: &EventModelingDiagramRenderModel,
    meta: &ParseMetadata,
) -> Result<Value> {
    Ok(json!({
        "type": meta.diagram_type,
        "title": &model.title,
        "accTitle": &model.acc_title,
        "accDescr": &model.acc_descr,
        "frames": &model.frames,
        "dataEntities": &model.data_entities,
    }))
}

#[cfg(test)]
pub(crate) fn parse_eventmodeling_model_for_render(
    code: &str,
    meta: &ParseMetadata,
) -> Result<EventModelingDiagramRenderModel> {
    Ok(construct_eventmodeling_semantic_source(code, meta)
        .map_err(|failure| *failure.error)?
        .into_render_model(meta))
}

pub(crate) fn parse_eventmodeling_model_for_render_controlled(
    code: &str,
    meta: &ParseMetadata,
    control: &crate::OperationControl,
) -> crate::OperationControlResult<Result<EventModelingDiagramRenderModel>> {
    let construction = construct_eventmodeling_semantic_source_controlled(code, meta, control)?;
    let source = match construction {
        Ok(source) => source,
        Err(failure) => return Ok(Err(*failure.error)),
    };
    Ok(Ok(source.into_render_model_controlled(meta, control)?))
}

#[derive(Debug, Clone)]
struct EventModelingFieldSpan {
    text: String,
    span: SourceSpan,
}

#[derive(Debug, Clone)]
struct EventModelingFrameFacts {
    frame_kind: String,
    name: String,
    name_span: SourceSpan,
    model_entity_type: String,
    model_entity_type_span: SourceSpan,
    entity_identifier: String,
    entity_identifier_span: SourceSpan,
    source_frames: Vec<EventModelingFieldSpan>,
    data_reference: Option<EventModelingFieldSpan>,
    data_type: Option<EventModelingFieldSpan>,
    data_inline_value: Option<EventModelingFieldSpan>,
}

#[derive(Debug, Clone)]
struct EventModelingDataEntityFacts {
    name: EventModelingFieldSpan,
    data_type: Option<EventModelingFieldSpan>,
    block_text: String,
    block_span: SourceSpan,
}

#[derive(Debug, Clone)]
struct EventModelingNoteFacts {
    source_frame: EventModelingFieldSpan,
    data_type: Option<EventModelingFieldSpan>,
    block_text: String,
    block_span: SourceSpan,
}

#[derive(Debug, Clone)]
struct EventModelingGwtStatementFacts {
    model_entity_type: EventModelingFieldSpan,
    entity_reference: EventModelingFieldSpan,
}

#[derive(Debug, Clone)]
struct EventModelingGwtFacts {
    source_frame: EventModelingFieldSpan,
    given: Vec<EventModelingGwtStatementFacts>,
    when: Vec<EventModelingGwtStatementFacts>,
    then: Vec<EventModelingGwtStatementFacts>,
}

#[derive(Debug, Clone)]
struct EventModelingValidationDiagnostic {
    message: String,
    span: SourceSpan,
}

#[derive(Default)]
struct EventModelingSyntaxFacts {
    header: Option<EventModelingFieldSpan>,
    common: LangiumCommonFacts,
    model_entities: Vec<EventModelingFieldSpan>,
    frames: Vec<EventModelingFrameFacts>,
    data_entities: Vec<EventModelingDataEntityFacts>,
    note_entities: Vec<EventModelingNoteFacts>,
    gwt_entities: Vec<EventModelingGwtFacts>,
    validation_diagnostics: Vec<EventModelingValidationDiagnostic>,
}

struct EventModelingSemanticSource {
    syntax: EventModelingSyntaxFacts,
    editor_facts: EditorSemanticFacts,
}

struct EventModelingParseFailure {
    error: Box<Error>,
    editor_facts: Box<EditorSemanticFacts>,
    span: SourceSpan,
}

impl EventModelingSyntaxFacts {
    fn editor_facts_controlled(
        &self,
        control: &crate::OperationControl,
    ) -> crate::OperationControlResult<EditorSemanticFacts> {
        control.checkpoint()?;
        let mut facts = EditorSemanticFacts::new();
        if let Some(header) = &self.header {
            facts.push_expected_syntax(EditorExpectedSyntax::new(
                EditorExpectedSyntaxKind::Payload,
                header.span,
            ));
            facts.push_symbol(EditorSemanticSymbol::payload(
                header.text.clone(),
                Some("eventmodeling header".to_string()),
                EditorSemanticKind::String,
                header.span,
                header.span,
            ));
        }
        for common in self.common.iter() {
            control.checkpoint()?;
            push_langium_common_editor_fact(&mut facts, common, "eventmodeling");
        }
        for entity in &self.model_entities {
            control.checkpoint()?;
            facts.push_symbol(
                EditorSemanticSymbol::new(
                    entity.text.clone(),
                    Some("eventmodeling model entity".to_string()),
                    EditorSemanticKind::Object,
                    entity.span,
                    entity.span,
                )
                .with_rename_policy(EditorRenamePolicy::QualifiedIdentifier),
            );
        }
        for frame in &self.frames {
            control.checkpoint()?;
            push_eventmodeling_frame_facts(&mut facts, frame, control)?;
        }
        for data_entity in &self.data_entities {
            control.checkpoint()?;
            push_eventmodeling_data_facts(&mut facts, data_entity);
            control.checkpoint()?;
        }
        for note in &self.note_entities {
            control.checkpoint()?;
            push_eventmodeling_note_facts(&mut facts, note);
            control.checkpoint()?;
        }
        for gwt in &self.gwt_entities {
            control.checkpoint()?;
            push_eventmodeling_gwt_facts(&mut facts, gwt, control)?;
        }
        for diagnostic in &self.validation_diagnostics {
            control.checkpoint()?;
            facts.push_diagnostic(&diagnostic.message, Some(diagnostic.span));
        }
        control.checkpoint()?;
        Ok(facts)
    }
}

impl EventModelingSemanticSource {
    fn editor_facts(&self) -> EditorSemanticFacts {
        self.editor_facts.clone()
    }

    fn into_render_model(self, meta: &ParseMetadata) -> EventModelingDiagramRenderModel {
        self.into_render_model_controlled(meta, &crate::OperationControl::new())
            .expect("a private parse control cannot be cancelled")
    }

    fn into_render_model_controlled(
        self,
        meta: &ParseMetadata,
        control: &crate::OperationControl,
    ) -> crate::OperationControlResult<EventModelingDiagramRenderModel> {
        control.checkpoint()?;
        let common = LangiumCommonDbFields::from_facts(&self.syntax.common);
        let mut frames = Vec::with_capacity(self.syntax.frames.len());
        for frame in self.syntax.frames {
            control.checkpoint()?;
            let mut source_frames = Vec::with_capacity(frame.source_frames.len());
            for source in frame.source_frames {
                control.checkpoint()?;
                source_frames.push(source.text);
            }
            let data_inline_value = if let Some(data) = frame.data_inline_value {
                control.checkpoint()?;
                let value = sanitize_text(&data.text, &meta.effective_config);
                control.checkpoint()?;
                Some(value)
            } else {
                None
            };
            frames.push(EventModelingFrameRenderModel {
                name: frame.name,
                frame_kind: frame.frame_kind,
                model_entity_type: frame.model_entity_type,
                entity_identifier: sanitize_text(&frame.entity_identifier, &meta.effective_config),
                source_frames,
                data_inline_value,
                data_reference: frame.data_reference.map(|data| data.text),
            });
            control.checkpoint()?;
        }
        let mut data_entities = Vec::with_capacity(self.syntax.data_entities.len());
        for data in self.syntax.data_entities {
            control.checkpoint()?;
            let data_block_value = sanitize_text(&data.block_text, &meta.effective_config);
            control.checkpoint()?;
            data_entities.push(EventModelingDataEntityRenderModel {
                name: data.name.text,
                data_block_value,
            });
        }
        control.checkpoint()?;
        Ok(EventModelingDiagramRenderModel {
            title: common
                .title
                .map(|value| sanitize_text(&value, &meta.effective_config)),
            acc_title: common
                .acc_title
                .map(|value| sanitize_acc_title(&value, &meta.effective_config)),
            acc_descr: common
                .acc_descr
                .map(|value| sanitize_acc_descr(&value, &meta.effective_config)),
            frames,
            data_entities,
        })
    }

    fn into_compat_json(self, meta: &ParseMetadata) -> Result<Value> {
        let model = self.into_render_model(meta);
        render_model_to_compat_json(&model, meta)
    }

    fn into_compat_json_controlled(
        self,
        meta: &ParseMetadata,
        control: &crate::OperationControl,
    ) -> crate::OperationControlResult<Result<Value>> {
        let model = self.into_render_model_controlled(meta, control)?;
        control.checkpoint()?;
        Ok(render_model_to_compat_json(&model, meta))
    }
}

impl EventModelingParseFailure {
    fn into_error_and_editor_facts(self) -> (Error, EditorSemanticFacts) {
        let mut facts = *self.editor_facts;
        facts.mark_recovered_from_parse_error(
            format!(
                "eventmodeling parser recovered after parse error: {}",
                self.error
            ),
            Some(self.span),
        );
        (*self.error, facts)
    }
}

fn construct_eventmodeling_semantic_source(
    code: &str,
    meta: &ParseMetadata,
) -> std::result::Result<EventModelingSemanticSource, EventModelingParseFailure> {
    construct_eventmodeling_semantic_source_controlled(code, meta, &crate::OperationControl::new())
        .expect("a private parse control cannot be cancelled")
}

fn construct_eventmodeling_semantic_source_controlled(
    code: &str,
    meta: &ParseMetadata,
    control: &crate::OperationControl,
) -> crate::OperationControlResult<
    std::result::Result<EventModelingSemanticSource, EventModelingParseFailure>,
> {
    control.checkpoint()?;
    #[cfg(test)]
    crate::diagrams::langium_common::record_family_syntax_construction("eventmodeling");

    let mut syntax = EventModelingSyntaxFacts::default();
    let mut cursor = EventModelingCursor::new(code, control);
    if let Err(error) = cursor.skip_hidden(meta) {
        control.checkpoint()?;
        let fallback = cursor.insertion_span();
        return Ok(Err(eventmodeling_failure_controlled(
            error, syntax, fallback, control,
        )?));
    }
    control.checkpoint()?;
    let header = parse_eventmodeling_header_cursor(&mut cursor, meta);
    control.checkpoint()?;
    match header {
        Ok(header) => syntax.header = Some(header),
        Err(error) => {
            let fallback = cursor.insertion_span();
            return Ok(Err(eventmodeling_failure_controlled(
                error, syntax, fallback, control,
            )?));
        }
    }

    let mut first_failure = None;
    loop {
        control.checkpoint()?;
        if let Err(error) = cursor.skip_hidden(meta) {
            control.checkpoint()?;
            let span = eventmodeling_error_span(&error, cursor.insertion_span());
            first_failure.get_or_insert((error, span));
            break;
        }
        if cursor.is_eof() {
            break;
        }

        if let Some(parsed) = parse_langium_common(code, cursor.offset()) {
            cursor.set_offset(cursor.offset() + parsed.consumed);
            syntax.common.push(parsed.fact);
            if let Some(diagnostic) = parsed.diagnostic {
                let error = Error::diagram_parse_insertion_point(
                    meta.diagram_type.clone(),
                    diagnostic.message,
                    diagnostic.span.start,
                );
                first_failure.get_or_insert((error, diagnostic.span));
            }
            continue;
        }

        let statement_start = cursor.offset();
        let result = if cursor.starts_keyword("entity") {
            parse_eventmodeling_entity_cursor(&mut cursor, meta)
                .map(|entity| syntax.model_entities.push(entity))
        } else if cursor.starts_keyword("tf") || cursor.starts_keyword("timeframe") {
            parse_eventmodeling_frame_cursor(&mut cursor, "timeframe", meta)
                .map(|frame| syntax.frames.push(frame))
        } else if cursor.starts_keyword("rf") || cursor.starts_keyword("resetframe") {
            parse_eventmodeling_frame_cursor(&mut cursor, "resetframe", meta)
                .map(|frame| syntax.frames.push(frame))
        } else if cursor.starts_keyword("data") {
            match parse_eventmodeling_data_cursor(&mut cursor, meta) {
                Ok(data) => {
                    syntax.data_entities.push(data);
                    Ok(())
                }
                Err(failure) => {
                    syntax.data_entities.push(*failure.partial);
                    Err(*failure.error)
                }
            }
        } else if cursor.starts_keyword("note") {
            match parse_eventmodeling_note_cursor(&mut cursor, meta) {
                Ok(note) => {
                    syntax.note_entities.push(note);
                    Ok(())
                }
                Err(failure) => {
                    syntax.note_entities.push(*failure.partial);
                    Err(*failure.error)
                }
            }
        } else if cursor.starts_keyword("gwt") {
            parse_eventmodeling_gwt_cursor(&mut cursor, meta)
                .map(|gwt| syntax.gwt_entities.push(gwt))
        } else {
            let span = cursor.unknown_statement_span();
            Err(Error::diagram_parse_exact(
                meta.diagram_type.clone(),
                format!(
                    "unexpected eventmodeling statement: {}",
                    &code[span.start..span.end]
                ),
                span,
            ))
        };

        control.checkpoint()?;
        if let Err(error) = result {
            let span = eventmodeling_error_span(&error, cursor.insertion_span());
            first_failure.get_or_insert((error, span));
            if cursor.is_eof() {
                break;
            }
            cursor.recover_to_next_statement(statement_start);
            control.checkpoint()?;
        }
    }

    if let Some(error) = validate_eventmodeling_semantics(&mut syntax, meta, control)? {
        let span = eventmodeling_error_span(&error, cursor.insertion_span());
        first_failure.get_or_insert((error, span));
    }
    if let Some((error, span)) = first_failure {
        return Ok(Err(eventmodeling_failure_controlled(
            error, syntax, span, control,
        )?));
    }
    let editor_facts = syntax.editor_facts_controlled(control)?;
    Ok(Ok(EventModelingSemanticSource {
        syntax,
        editor_facts,
    }))
}

const EVENTMODELING_ENTITY_TYPES: &[&str] = &[
    "rmo",
    "readmodel",
    "ui",
    "cmd",
    "command",
    "evt",
    "event",
    "pcr",
    "processor",
];
const EVENTMODELING_DATA_TYPES: &[&str] = &[
    "json", "jsobj", "figma", "salt", "uri", "md", "html", "text",
];

struct EventModelingCursor<'a> {
    source: &'a str,
    control: &'a crate::OperationControl,
    offset: usize,
}

impl<'a> EventModelingCursor<'a> {
    fn new(source: &'a str, control: &'a crate::OperationControl) -> Self {
        Self {
            source,
            control,
            offset: 0,
        }
    }

    fn offset(&self) -> usize {
        self.offset
    }

    fn set_offset(&mut self, offset: usize) {
        self.offset = offset.min(self.source.len());
    }

    fn is_eof(&self) -> bool {
        self.offset >= self.source.len()
    }

    fn insertion_span(&self) -> SourceSpan {
        SourceSpan::new(self.offset, self.offset)
    }

    fn starts_keyword(&self, keyword: &str) -> bool {
        let Some(rest) = self.source[self.offset..].strip_prefix(keyword) else {
            return false;
        };
        rest.chars()
            .next()
            .is_none_or(|ch| !ch.is_ascii_alphanumeric() && ch != '_')
    }

    fn consume_keyword(&mut self, keyword: &str) -> bool {
        if !self.starts_keyword(keyword) {
            return false;
        }
        self.offset += keyword.len();
        true
    }

    fn consume_literal(&mut self, literal: &str) -> bool {
        if !self.starts_literal(literal) {
            return false;
        }
        self.offset += literal.len();
        true
    }

    fn starts_literal(&self, literal: &str) -> bool {
        self.source[self.offset..].starts_with(literal)
    }

    fn unknown_statement_span(&self) -> SourceSpan {
        let rest = &self.source[self.offset..];
        let end = rest.find(['\r', '\n']).unwrap_or(rest.len());
        let visible = rest[..end].trim_end();
        let len = if visible.is_empty() {
            rest.chars().next().map(char::len_utf8).unwrap_or_default()
        } else {
            visible.len()
        };
        SourceSpan::new(self.offset, self.offset + len)
    }

    fn recover_to_next_statement(&mut self, statement_start: usize) {
        self.offset = statement_start.min(self.source.len());
        let mut next_checkpoint = self.offset.saturating_add(4096);
        while self.offset < self.source.len() {
            let ch = self.source[self.offset..]
                .chars()
                .next()
                .expect("eventmodeling recovery offset must be a character boundary");
            self.offset += ch.len_utf8();
            if self.offset >= next_checkpoint {
                if self.control.checkpoint().is_err() {
                    break;
                }
                next_checkpoint = self.offset.saturating_add(4096);
            }
            if ch == '\n' || ch == '\r' {
                break;
            }
        }
    }

    fn skip_hidden(&mut self, meta: &ParseMetadata) -> Result<()> {
        loop {
            if self.control.checkpoint().is_err() {
                return Ok(());
            }
            let mut next_checkpoint = self.offset.saturating_add(4096);
            while self.offset < self.source.len() {
                let ch = self.source[self.offset..].chars().next().unwrap();
                if !ch.is_whitespace() {
                    break;
                }
                self.offset += ch.len_utf8();
                if self.offset >= next_checkpoint {
                    if self.control.checkpoint().is_err() {
                        return Ok(());
                    }
                    next_checkpoint = self.offset.saturating_add(4096);
                }
            }
            if self.is_eof() {
                return Ok(());
            }

            let rest = &self.source[self.offset..];
            if rest.starts_with("%%{") {
                let Some(end) = rest.find("}%%") else {
                    return Err(Error::diagram_parse_insertion_point(
                        meta.diagram_type.clone(),
                        "expected closing eventmodeling directive",
                        self.source.len(),
                    ));
                };
                self.offset += end + 3;
                continue;
            }
            if rest.starts_with("%%") {
                self.offset += rest.find(['\r', '\n']).unwrap_or(rest.len());
                continue;
            }
            if rest.starts_with("//") {
                self.offset += rest.find(['\r', '\n']).unwrap_or(rest.len());
                continue;
            }
            if rest.starts_with("/*") {
                let Some(end) = rest.find("*/") else {
                    return Err(Error::diagram_parse_insertion_point(
                        meta.diagram_type.clone(),
                        "expected closing eventmodeling block comment",
                        self.source.len(),
                    ));
                };
                self.offset += end + 2;
                continue;
            }
            if rest.starts_with("---")
                && let Some(end) = eventmodeling_yaml_end(self.source, self.offset, self.control)
            {
                self.offset = end;
                continue;
            }
            return Ok(());
        }
    }

    fn take_token(
        &mut self,
        meta: &ParseMetadata,
        expected: &str,
    ) -> Result<EventModelingFieldSpan> {
        self.skip_hidden(meta)?;
        let start = self.offset;
        let mut next_checkpoint = self.offset.saturating_add(4096);
        while self.offset < self.source.len() {
            let rest = &self.source[self.offset..];
            let ch = rest.chars().next().unwrap();
            if ch.is_whitespace()
                || rest.starts_with("->>")
                || rest.starts_with("[[")
                || rest.starts_with("]]")
                || matches!(ch, '`' | '{' | '}' | '"' | '\'')
            {
                break;
            }
            self.offset += ch.len_utf8();
            if self.offset >= next_checkpoint {
                if self.control.checkpoint().is_err() {
                    break;
                }
                next_checkpoint = self.offset.saturating_add(4096);
            }
        }
        if start == self.offset {
            return Err(Error::diagram_parse_insertion_point(
                meta.diagram_type.clone(),
                expected,
                start,
            ));
        }
        Ok(EventModelingFieldSpan {
            text: self.source[start..self.offset].to_string(),
            span: SourceSpan::new(start, self.offset),
        })
    }
}

fn eventmodeling_yaml_end(
    source: &str,
    start: usize,
    control: &crate::OperationControl,
) -> Option<usize> {
    let rest = source.get(start..)?;
    let opening_newline = rest.find('\n')?;
    let opening = rest[..opening_newline]
        .strip_suffix('\r')
        .unwrap_or(&rest[..opening_newline]);
    if !opening
        .strip_prefix("---")?
        .bytes()
        .all(|byte| matches!(byte, b' ' | b'\t'))
    {
        return None;
    }

    let mut cursor = start + opening_newline + 1;
    while cursor <= source.len() {
        if control.checkpoint().is_err() {
            return None;
        }
        let (line, next) = physical_line_at(source, cursor);
        if line.trim_end_matches([' ', '\t']) == "---" {
            return Some(next);
        }
        if next == cursor || next == source.len() {
            break;
        }
        cursor = next;
    }
    None
}

fn eventmodeling_failure_controlled(
    error: Error,
    syntax: EventModelingSyntaxFacts,
    fallback: SourceSpan,
    control: &crate::OperationControl,
) -> crate::OperationControlResult<EventModelingParseFailure> {
    let span = eventmodeling_error_span(&error, fallback);
    let editor_facts = syntax.editor_facts_controlled(control)?;
    Ok(EventModelingParseFailure {
        error: Box::new(error),
        editor_facts: Box::new(editor_facts),
        span,
    })
}

fn eventmodeling_error_span(error: &Error, fallback: SourceSpan) -> SourceSpan {
    match error {
        Error::DiagramParse { diagnostic, .. } => diagnostic.span().unwrap_or(fallback),
        _ => fallback,
    }
}

fn eventmodeling_exact_error(
    meta: &ParseMetadata,
    message: impl Into<String>,
    span: SourceSpan,
) -> Error {
    Error::diagram_parse_exact(meta.diagram_type.clone(), message, span)
}

fn is_eventmodeling_id(value: &str) -> bool {
    let mut chars = value.chars();
    chars
        .next()
        .is_some_and(|ch| ch == '_' || ch.is_ascii_alphabetic())
        && chars.all(|ch| ch == '_' || ch.is_ascii_alphanumeric())
}

fn take_eventmodeling_id_cursor(
    cursor: &mut EventModelingCursor<'_>,
    meta: &ParseMetadata,
    expected: &str,
) -> Result<EventModelingFieldSpan> {
    let field = cursor.take_token(meta, expected)?;
    if !is_eventmodeling_id(&field.text) {
        return Err(eventmodeling_exact_error(meta, expected, field.span));
    }
    Ok(field)
}

fn take_eventmodeling_qualified_name_cursor(
    cursor: &mut EventModelingCursor<'_>,
    meta: &ParseMetadata,
    expected: &str,
) -> Result<EventModelingFieldSpan> {
    cursor.skip_hidden(meta)?;
    let start = cursor.offset;
    let mut text = String::new();
    let mut end = start;

    loop {
        if cursor.control.checkpoint().is_err() {
            break;
        }
        let segment_start = cursor.offset;
        let Some(first) = cursor.source[cursor.offset..].chars().next() else {
            return Err(Error::diagram_parse_insertion_point(
                meta.diagram_type.clone(),
                expected,
                cursor.offset,
            ));
        };
        if first != '_' && !first.is_ascii_alphabetic() {
            let invalid = cursor.take_token(meta, expected)?;
            let span = SourceSpan::new(start, invalid.span.end);
            return Err(eventmodeling_exact_error(meta, expected, span));
        }
        cursor.offset += first.len_utf8();
        let mut next_checkpoint = cursor.offset.saturating_add(4096);
        while let Some(ch) = cursor.source[cursor.offset..].chars().next() {
            if ch != '_' && !ch.is_ascii_alphanumeric() {
                break;
            }
            cursor.offset += ch.len_utf8();
            if cursor.offset >= next_checkpoint {
                if cursor.control.checkpoint().is_err() {
                    break;
                }
                next_checkpoint = cursor.offset.saturating_add(4096);
            }
        }
        let segment_span = SourceSpan::new(segment_start, cursor.offset);
        if !text.is_empty() {
            text.push('.');
        }
        text.push_str(&cursor.source[segment_span.start..segment_span.end]);
        end = segment_span.end;

        cursor.skip_hidden(meta)?;
        if !cursor.source[cursor.offset..].starts_with('.') {
            break;
        }
        cursor.offset += 1;
        cursor.skip_hidden(meta)?;
    }

    Ok(EventModelingFieldSpan {
        text,
        span: SourceSpan::new(start, end),
    })
}

fn take_eventmodeling_frame_id_cursor(
    cursor: &mut EventModelingCursor<'_>,
    meta: &ParseMetadata,
    expected: &str,
) -> Result<EventModelingFieldSpan> {
    let field = cursor.take_token(meta, expected)?;
    if field.text.is_empty()
        || field.text.len() > 3
        || !field.text.bytes().all(|byte| byte.is_ascii_digit())
    {
        return Err(eventmodeling_exact_error(meta, expected, field.span));
    }
    Ok(field)
}

fn take_eventmodeling_entity_type_cursor(
    cursor: &mut EventModelingCursor<'_>,
    meta: &ParseMetadata,
) -> Result<EventModelingFieldSpan> {
    let field = cursor.take_token(meta, "expected eventmodeling entity type")?;
    if !EVENTMODELING_ENTITY_TYPES.contains(&field.text.as_str()) {
        return Err(eventmodeling_exact_error(
            meta,
            "expected eventmodeling entity type",
            field.span,
        ));
    }
    Ok(field)
}

fn parse_eventmodeling_header_cursor(
    cursor: &mut EventModelingCursor<'_>,
    meta: &ParseMetadata,
) -> Result<EventModelingFieldSpan> {
    let start = cursor.offset();
    if !cursor.consume_keyword("eventmodeling") {
        let token =
            cursor
                .take_token(meta, "expected eventmodeling")
                .unwrap_or(EventModelingFieldSpan {
                    text: String::new(),
                    span: SourceSpan::new(start, start),
                });
        return Err(if token.span.start == token.span.end {
            Error::diagram_parse_insertion_point(
                meta.diagram_type.clone(),
                "expected eventmodeling",
                start,
            )
        } else {
            eventmodeling_exact_error(meta, "expected eventmodeling", token.span)
        });
    }
    Ok(EventModelingFieldSpan {
        text: "eventmodeling".to_string(),
        span: SourceSpan::new(start, cursor.offset()),
    })
}

fn parse_eventmodeling_entity_cursor(
    cursor: &mut EventModelingCursor<'_>,
    meta: &ParseMetadata,
) -> Result<EventModelingFieldSpan> {
    cursor.consume_keyword("entity");
    take_eventmodeling_qualified_name_cursor(
        cursor,
        meta,
        "expected eventmodeling model entity name",
    )
}

fn parse_eventmodeling_frame_cursor(
    cursor: &mut EventModelingCursor<'_>,
    frame_kind: &str,
    meta: &ParseMetadata,
) -> Result<EventModelingFrameFacts> {
    if frame_kind == "timeframe" {
        if !cursor.consume_keyword("timeframe") {
            cursor.consume_keyword("tf");
        }
    } else if !cursor.consume_keyword("resetframe") {
        cursor.consume_keyword("rf");
    }

    let name = take_eventmodeling_frame_id_cursor(
        cursor,
        meta,
        "expected eventmodeling frame id with one to three digits",
    )?;
    let model_entity_type = take_eventmodeling_entity_type_cursor(cursor, meta)?;
    let entity_identifier = take_eventmodeling_qualified_name_cursor(
        cursor,
        meta,
        "expected eventmodeling qualified entity identifier",
    )?;

    let mut source_frames = Vec::new();
    loop {
        if cursor.control.checkpoint().is_err() {
            break;
        }
        cursor.skip_hidden(meta)?;
        if !cursor.starts_literal("->>") {
            break;
        }
        cursor.consume_literal("->>");
        source_frames.push(take_eventmodeling_frame_id_cursor(
            cursor,
            meta,
            "expected eventmodeling source frame id",
        )?);
    }

    cursor.skip_hidden(meta)?;
    let data_reference = if cursor.starts_literal("[[") {
        cursor.consume_literal("[[");
        let reference =
            take_eventmodeling_id_cursor(cursor, meta, "expected eventmodeling data reference")?;
        cursor.skip_hidden(meta)?;
        if !cursor.starts_literal("]]") {
            return Err(Error::diagram_parse_insertion_point(
                meta.diagram_type.clone(),
                "expected closing ']]' for eventmodeling data reference",
                cursor.offset(),
            ));
        }
        cursor.consume_literal("]]");
        Some(reference)
    } else {
        None
    };

    let (data_type, data_inline_value) = parse_eventmodeling_optional_inline(cursor, meta)?;
    Ok(EventModelingFrameFacts {
        frame_kind: frame_kind.to_string(),
        name: name.text,
        name_span: name.span,
        model_entity_type: model_entity_type.text,
        model_entity_type_span: model_entity_type.span,
        entity_identifier: entity_identifier.text,
        entity_identifier_span: entity_identifier.span,
        source_frames,
        data_reference,
        data_type,
        data_inline_value,
    })
}

fn parse_eventmodeling_optional_data_type(
    cursor: &mut EventModelingCursor<'_>,
    meta: &ParseMetadata,
) -> Result<Option<EventModelingFieldSpan>> {
    cursor.skip_hidden(meta)?;
    if !cursor.starts_literal("`") {
        return Ok(None);
    }
    cursor.consume_literal("`");
    let field = cursor.take_token(meta, "expected eventmodeling data type")?;
    if !EVENTMODELING_DATA_TYPES.contains(&field.text.as_str()) {
        return Err(eventmodeling_exact_error(
            meta,
            format!("unsupported eventmodeling data type '{}'", field.text),
            field.span,
        ));
    }
    cursor.skip_hidden(meta)?;
    if !cursor.starts_literal("`") {
        return Err(Error::diagram_parse_insertion_point(
            meta.diagram_type.clone(),
            "expected closing backtick for eventmodeling data type",
            cursor.offset(),
        ));
    }
    cursor.consume_literal("`");
    Ok(Some(field))
}

fn parse_eventmodeling_optional_inline(
    cursor: &mut EventModelingCursor<'_>,
    meta: &ParseMetadata,
) -> Result<(
    Option<EventModelingFieldSpan>,
    Option<EventModelingFieldSpan>,
)> {
    let data_type = parse_eventmodeling_optional_data_type(cursor, meta)?;
    cursor.skip_hidden(meta)?;
    let Some(delimiter) = cursor.source[cursor.offset..].chars().next() else {
        if data_type.is_some() {
            return Err(Error::diagram_parse_insertion_point(
                meta.diagram_type.clone(),
                "expected eventmodeling inline data",
                cursor.offset(),
            ));
        }
        return Ok((None, None));
    };
    if !matches!(delimiter, '{' | '"' | '\'') {
        if data_type.is_some() {
            return Err(Error::diagram_parse_insertion_point(
                meta.diagram_type.clone(),
                "expected eventmodeling inline data",
                cursor.offset(),
            ));
        }
        return Ok((None, None));
    }

    let start = cursor.offset;
    let rest = &cursor.source[start..];
    let line_end = rest.find(['\r', '\n']).unwrap_or(rest.len());
    let line = &rest[..line_end];
    let end = if delimiter == '{' {
        line.rfind('}').map(|index| start + index + 1)
    } else {
        line[delimiter.len_utf8()..]
            .rfind(delimiter)
            .map(|index| start + delimiter.len_utf8() + index + delimiter.len_utf8())
    }
    .ok_or_else(|| {
        Error::diagram_parse_insertion_point(
            meta.diagram_type.clone(),
            "expected closing delimiter for eventmodeling inline data",
            start + line_end,
        )
    })?;
    cursor.offset = end;
    Ok((
        data_type,
        Some(EventModelingFieldSpan {
            text: cursor.source[start..end].to_string(),
            span: SourceSpan::new(start, end),
        }),
    ))
}

struct ParsedEventModelingBlock {
    data_type: Option<EventModelingFieldSpan>,
    text: String,
    span: SourceSpan,
}

struct FailedEventModelingBlock {
    error: Box<Error>,
    data_type: Option<EventModelingFieldSpan>,
    text: String,
    span: SourceSpan,
}

fn parse_eventmodeling_block_cursor(
    cursor: &mut EventModelingCursor<'_>,
    meta: &ParseMetadata,
) -> std::result::Result<ParsedEventModelingBlock, FailedEventModelingBlock> {
    let data_type = match parse_eventmodeling_optional_data_type(cursor, meta) {
        Ok(data_type) => data_type,
        Err(error) => {
            return Err(FailedEventModelingBlock {
                error: Box::new(error),
                data_type: None,
                text: String::new(),
                span: cursor.insertion_span(),
            });
        }
    };
    if let Err(error) = cursor.skip_hidden(meta) {
        return Err(FailedEventModelingBlock {
            error: Box::new(error),
            data_type,
            text: String::new(),
            span: cursor.insertion_span(),
        });
    }
    if !cursor.starts_literal("{") {
        let span = cursor.insertion_span();
        return Err(FailedEventModelingBlock {
            error: Box::new(Error::diagram_parse_insertion_point(
                meta.diagram_type.clone(),
                "expected eventmodeling data block",
                span.start,
            )),
            data_type,
            text: String::new(),
            span,
        });
    }

    let block_start = cursor.offset;
    cursor.consume_literal("{");
    let after_open = &cursor.source[cursor.offset..];
    let Some(newline_rel) = after_open.find('\n') else {
        let span = SourceSpan::new(block_start, cursor.source.len());
        return Err(FailedEventModelingBlock {
            error: Box::new(eventmodeling_exact_error(
                meta,
                "eventmodeling data block must start on a new line",
                span,
            )),
            data_type,
            text: cursor.source[block_start..].to_string(),
            span,
        });
    };
    let before_newline = after_open[..newline_rel]
        .strip_suffix('\r')
        .unwrap_or(&after_open[..newline_rel]);
    if !before_newline
        .bytes()
        .all(|byte| matches!(byte, b' ' | b'\t'))
    {
        let leading = before_newline
            .bytes()
            .take_while(|byte| matches!(byte, b' ' | b'\t'))
            .count();
        let span = SourceSpan::new(
            cursor.offset + leading,
            cursor.offset + before_newline.len(),
        );
        return Err(FailedEventModelingBlock {
            error: Box::new(eventmodeling_exact_error(
                meta,
                "eventmodeling data block must start on a new line",
                span,
            )),
            data_type,
            text: cursor.source[block_start..cursor.offset + before_newline.len()].to_string(),
            span,
        });
    }
    cursor.offset += newline_rel + 1;

    let mut line_start = cursor.offset;
    while line_start < cursor.source.len() {
        if cursor.control.checkpoint().is_err() {
            break;
        }
        if cursor.source[line_start..].starts_with('}') {
            let block_end = line_start + 1;
            let after = &cursor.source[block_end..];
            if after.is_empty() || after.chars().next().is_some_and(char::is_whitespace) {
                cursor.offset = block_end;
                return Ok(ParsedEventModelingBlock {
                    data_type,
                    text: cursor.source[block_start..block_end].to_string(),
                    span: SourceSpan::new(block_start, block_end),
                });
            }
        }
        let (_, next) = physical_line_at(cursor.source, line_start);
        if next <= line_start || next == cursor.source.len() {
            break;
        }
        line_start = next;
    }

    let span = SourceSpan::new(block_start, cursor.source.len());
    cursor.offset = cursor.source.len();
    Err(FailedEventModelingBlock {
        error: Box::new(Error::diagram_parse_insertion_point(
            meta.diagram_type.clone(),
            "expected closing brace for eventmodeling data block",
            cursor.source.len(),
        )),
        data_type,
        text: cursor.source[block_start..].to_string(),
        span,
    })
}

struct FailedEventModelingDataCursor {
    error: Box<Error>,
    partial: Box<EventModelingDataEntityFacts>,
}

fn parse_eventmodeling_data_cursor(
    cursor: &mut EventModelingCursor<'_>,
    meta: &ParseMetadata,
) -> std::result::Result<EventModelingDataEntityFacts, FailedEventModelingDataCursor> {
    cursor.consume_keyword("data");
    let name =
        match take_eventmodeling_id_cursor(cursor, meta, "expected eventmodeling data entity name")
        {
            Ok(name) => name,
            Err(error) => {
                return Err(FailedEventModelingDataCursor {
                    error: Box::new(error),
                    partial: Box::new(EventModelingDataEntityFacts {
                        name: EventModelingFieldSpan {
                            text: String::new(),
                            span: cursor.insertion_span(),
                        },
                        data_type: None,
                        block_text: String::new(),
                        block_span: cursor.insertion_span(),
                    }),
                });
            }
        };
    match parse_eventmodeling_block_cursor(cursor, meta) {
        Ok(block) => Ok(EventModelingDataEntityFacts {
            name,
            data_type: block.data_type,
            block_text: block.text,
            block_span: block.span,
        }),
        Err(block) => Err(FailedEventModelingDataCursor {
            error: block.error,
            partial: Box::new(EventModelingDataEntityFacts {
                name,
                data_type: block.data_type,
                block_text: block.text,
                block_span: block.span,
            }),
        }),
    }
}

struct FailedEventModelingNoteCursor {
    error: Box<Error>,
    partial: Box<EventModelingNoteFacts>,
}

fn parse_eventmodeling_note_cursor(
    cursor: &mut EventModelingCursor<'_>,
    meta: &ParseMetadata,
) -> std::result::Result<EventModelingNoteFacts, FailedEventModelingNoteCursor> {
    cursor.consume_keyword("note");
    let source_frame = match take_eventmodeling_frame_id_cursor(
        cursor,
        meta,
        "expected eventmodeling note source frame",
    ) {
        Ok(source) => source,
        Err(error) => {
            return Err(FailedEventModelingNoteCursor {
                error: Box::new(error),
                partial: Box::new(EventModelingNoteFacts {
                    source_frame: EventModelingFieldSpan {
                        text: String::new(),
                        span: cursor.insertion_span(),
                    },
                    data_type: None,
                    block_text: String::new(),
                    block_span: cursor.insertion_span(),
                }),
            });
        }
    };
    match parse_eventmodeling_block_cursor(cursor, meta) {
        Ok(block) => Ok(EventModelingNoteFacts {
            source_frame,
            data_type: block.data_type,
            block_text: block.text,
            block_span: block.span,
        }),
        Err(block) => Err(FailedEventModelingNoteCursor {
            error: block.error,
            partial: Box::new(EventModelingNoteFacts {
                source_frame,
                data_type: block.data_type,
                block_text: block.text,
                block_span: block.span,
            }),
        }),
    }
}

fn is_eventmodeling_top_level_start(cursor: &EventModelingCursor<'_>) -> bool {
    [
        "title",
        "accTitle",
        "accDescr",
        "entity",
        "tf",
        "timeframe",
        "rf",
        "resetframe",
        "data",
        "note",
        "gwt",
    ]
    .into_iter()
    .any(|keyword| cursor.starts_keyword(keyword))
}

fn parse_eventmodeling_gwt_group(
    cursor: &mut EventModelingCursor<'_>,
    meta: &ParseMetadata,
    stop_keywords: &[&str],
) -> Result<Vec<EventModelingGwtStatementFacts>> {
    let mut statements = Vec::new();
    loop {
        if cursor.control.checkpoint().is_err() {
            break;
        }
        cursor.skip_hidden(meta)?;
        if cursor.is_eof()
            || stop_keywords
                .iter()
                .any(|keyword| cursor.starts_keyword(keyword))
            || (!statements.is_empty() && is_eventmodeling_top_level_start(cursor))
        {
            break;
        }
        let model_entity_type = take_eventmodeling_entity_type_cursor(cursor, meta)?;
        let entity_reference = take_eventmodeling_id_cursor(
            cursor,
            meta,
            "expected eventmodeling gwt model entity reference",
        )?;
        statements.push(EventModelingGwtStatementFacts {
            model_entity_type,
            entity_reference,
        });
    }
    if statements.is_empty() {
        return Err(Error::diagram_parse_insertion_point(
            meta.diagram_type.clone(),
            "expected at least one eventmodeling gwt statement",
            cursor.offset(),
        ));
    }
    Ok(statements)
}

fn parse_eventmodeling_gwt_cursor(
    cursor: &mut EventModelingCursor<'_>,
    meta: &ParseMetadata,
) -> Result<EventModelingGwtFacts> {
    cursor.consume_keyword("gwt");
    let source_frame = take_eventmodeling_frame_id_cursor(
        cursor,
        meta,
        "expected eventmodeling gwt source frame",
    )?;
    cursor.skip_hidden(meta)?;
    if !cursor.consume_keyword("given") {
        return Err(Error::diagram_parse_insertion_point(
            meta.diagram_type.clone(),
            "expected 'given' in eventmodeling gwt",
            cursor.offset(),
        ));
    }
    let given = parse_eventmodeling_gwt_group(cursor, meta, &["when", "then"])?;
    cursor.skip_hidden(meta)?;
    let when = if cursor.consume_keyword("when") {
        parse_eventmodeling_gwt_group(cursor, meta, &["then"])?
    } else {
        Vec::new()
    };
    cursor.skip_hidden(meta)?;
    if !cursor.consume_keyword("then") {
        return Err(Error::diagram_parse_insertion_point(
            meta.diagram_type.clone(),
            "expected 'then' in eventmodeling gwt",
            cursor.offset(),
        ));
    }
    let then = parse_eventmodeling_gwt_group(cursor, meta, &[])?;
    Ok(EventModelingGwtFacts {
        source_frame,
        given,
        when,
        then,
    })
}

fn validate_eventmodeling_semantics(
    syntax: &mut EventModelingSyntaxFacts,
    meta: &ParseMetadata,
    control: &crate::OperationControl,
) -> crate::OperationControlResult<Option<Error>> {
    let mut frame_types = HashMap::with_capacity(syntax.frames.len());
    let mut duplicate_frame = None;
    for frame in &syntax.frames {
        control.checkpoint()?;
        match frame_types.entry(frame.name.clone()) {
            std::collections::hash_map::Entry::Vacant(entry) => {
                entry.insert(frame.model_entity_type.clone());
            }
            std::collections::hash_map::Entry::Occupied(_) => {
                duplicate_frame.get_or_insert_with(|| {
                    Error::diagram_parse_exact(
                        meta.diagram_type.clone(),
                        format!(r#"Duplicate event modeling frame ID "{}""#, frame.name),
                        frame.name_span,
                    )
                });
            }
        }
    }
    let mut data_names = HashSet::with_capacity(syntax.data_entities.len());
    for data in &syntax.data_entities {
        control.checkpoint()?;
        data_names.insert(data.name.text.as_str());
    }
    let mut model_entity_names = HashSet::with_capacity(syntax.model_entities.len());
    for entity in &syntax.model_entities {
        control.checkpoint()?;
        model_entity_names.insert(entity.text.as_str());
    }
    let mut diagnostics = Vec::new();

    for frame in &syntax.frames {
        control.checkpoint()?;
        for source in &frame.source_frames {
            control.checkpoint()?;
            let Some(source_type) = frame_types.get(&source.text) else {
                diagnostics.push(EventModelingValidationDiagnostic {
                    message: format!("unknown eventmodeling frame reference '{}'", source.text),
                    span: source.span,
                });
                continue;
            };
            if let Some((target_label, expected_label, allowed)) =
                eventmodeling_allowed_source_types(&frame.model_entity_type)
                && !allowed.contains(&source_type.as_str())
            {
                diagnostics.push(EventModelingValidationDiagnostic {
                    message: format!(
                        "A {target_label} can only receive input from a {expected_label}, not from '{source_type}'."
                    ),
                    span: source.span,
                });
            }
        }
        if let Some(reference) = &frame.data_reference
            && !data_names.contains(reference.text.as_str())
        {
            diagnostics.push(EventModelingValidationDiagnostic {
                message: format!("unknown eventmodeling data reference '{}'", reference.text),
                span: reference.span,
            });
        }
    }
    for note in &syntax.note_entities {
        control.checkpoint()?;
        if !frame_types.contains_key(&note.source_frame.text) {
            diagnostics.push(EventModelingValidationDiagnostic {
                message: format!(
                    "unknown eventmodeling frame reference '{}'",
                    note.source_frame.text
                ),
                span: note.source_frame.span,
            });
        }
    }
    for gwt in &syntax.gwt_entities {
        control.checkpoint()?;
        if !frame_types.contains_key(&gwt.source_frame.text) {
            diagnostics.push(EventModelingValidationDiagnostic {
                message: format!(
                    "unknown eventmodeling frame reference '{}'",
                    gwt.source_frame.text
                ),
                span: gwt.source_frame.span,
            });
        }
        for statement in gwt.given.iter().chain(&gwt.when).chain(&gwt.then) {
            control.checkpoint()?;
            if !model_entity_names.contains(statement.entity_reference.text.as_str()) {
                diagnostics.push(EventModelingValidationDiagnostic {
                    message: format!(
                        "unknown eventmodeling model entity reference '{}'",
                        statement.entity_reference.text
                    ),
                    span: statement.entity_reference.span,
                });
            }
        }
    }
    syntax.validation_diagnostics = diagnostics;
    Ok(duplicate_frame)
}

fn eventmodeling_allowed_source_types(
    target: &str,
) -> Option<(&'static str, &'static str, &'static [&'static str])> {
    match target {
        "cmd" | "command" => Some(("command", "ui or processor", &["ui", "pcr", "processor"])),
        "evt" | "event" => Some(("event", "command", &["cmd", "command"])),
        "rmo" | "readmodel" => Some(("read model", "event", &["evt", "event"])),
        "pcr" | "processor" => Some(("processor", "read model", &["rmo", "readmodel"])),
        "ui" => Some(("ui", "read model", &["rmo", "readmodel"])),
        _ => None,
    }
}

fn push_eventmodeling_frame_facts(
    facts: &mut EditorSemanticFacts,
    frame: &EventModelingFrameFacts,
    control: &crate::OperationControl,
) -> crate::OperationControlResult<()> {
    facts.push_expected_syntax(EditorExpectedSyntax::new(
        EditorExpectedSyntaxKind::Payload,
        frame.name_span,
    ));
    facts.push_symbol(
        EditorSemanticSymbol::new(
            frame.name.clone(),
            Some("eventmodeling frame".to_string()),
            EditorSemanticKind::Namespace,
            frame.name_span,
            frame.name_span,
        )
        .with_rename_policy(EditorRenamePolicy::EventModelingFrameId),
    );
    facts.push_symbol(EditorSemanticSymbol::payload(
        frame.model_entity_type.clone(),
        Some("eventmodeling entity type".to_string()),
        EditorSemanticKind::String,
        frame.model_entity_type_span,
        frame.model_entity_type_span,
    ));
    facts.push_symbol(EditorSemanticSymbol::payload(
        frame.entity_identifier.clone(),
        Some("eventmodeling entity identifier".to_string()),
        EditorSemanticKind::String,
        frame.entity_identifier_span,
        frame.entity_identifier_span,
    ));
    for source in &frame.source_frames {
        control.checkpoint()?;
        facts.push_symbol(
            EditorSemanticSymbol::reference(
                source.text.clone(),
                Some("eventmodeling source frame".to_string()),
                EditorSemanticKind::Namespace,
                source.span,
                source.span,
            )
            .with_rename_policy(EditorRenamePolicy::EventModelingFrameId),
        );
    }
    if let Some(reference) = &frame.data_reference {
        control.checkpoint()?;
        facts.push_symbol(
            EditorSemanticSymbol::reference(
                reference.text.clone(),
                Some("eventmodeling data reference".to_string()),
                EditorSemanticKind::Namespace,
                reference.span,
                reference.span,
            )
            .with_rename_policy(EditorRenamePolicy::EventModelingId),
        );
    }
    for (field, detail) in [
        (&frame.data_type, "eventmodeling data type"),
        (&frame.data_inline_value, "eventmodeling inline data"),
    ] {
        control.checkpoint()?;
        let Some(field) = field else {
            continue;
        };
        facts.push_symbol(EditorSemanticSymbol::payload(
            field.text.clone(),
            Some(detail.to_string()),
            EditorSemanticKind::String,
            field.span,
            field.span,
        ));
    }
    Ok(())
}

fn push_eventmodeling_data_facts(
    facts: &mut EditorSemanticFacts,
    data_entity: &EventModelingDataEntityFacts,
) {
    if !data_entity.name.text.is_empty() {
        facts.push_symbol(
            EditorSemanticSymbol::new(
                data_entity.name.text.clone(),
                Some("eventmodeling data entity".to_string()),
                EditorSemanticKind::Namespace,
                data_entity.name.span,
                data_entity.name.span,
            )
            .with_rename_policy(EditorRenamePolicy::EventModelingId),
        );
    }
    if let Some(data_type) = &data_entity.data_type {
        facts.push_symbol(EditorSemanticSymbol::payload(
            data_type.text.clone(),
            Some("eventmodeling data type".to_string()),
            EditorSemanticKind::String,
            data_type.span,
            data_type.span,
        ));
    }
    facts.push_expected_syntax(EditorExpectedSyntax::new(
        EditorExpectedSyntaxKind::Payload,
        data_entity.block_span,
    ));
    facts.push_symbol(EditorSemanticSymbol::payload(
        data_entity.block_text.clone(),
        Some("eventmodeling data block".to_string()),
        EditorSemanticKind::String,
        data_entity.block_span,
        data_entity.block_span,
    ));
}

fn push_eventmodeling_note_facts(facts: &mut EditorSemanticFacts, note: &EventModelingNoteFacts) {
    if !note.source_frame.text.is_empty() {
        facts.push_symbol(
            EditorSemanticSymbol::reference(
                note.source_frame.text.clone(),
                Some("eventmodeling note source frame".to_string()),
                EditorSemanticKind::Namespace,
                note.source_frame.span,
                note.source_frame.span,
            )
            .with_rename_policy(EditorRenamePolicy::EventModelingFrameId),
        );
    }
    if let Some(data_type) = &note.data_type {
        facts.push_symbol(EditorSemanticSymbol::payload(
            data_type.text.clone(),
            Some("eventmodeling data type".to_string()),
            EditorSemanticKind::String,
            data_type.span,
            data_type.span,
        ));
    }
    facts.push_expected_syntax(EditorExpectedSyntax::new(
        EditorExpectedSyntaxKind::Payload,
        note.block_span,
    ));
    facts.push_symbol(EditorSemanticSymbol::payload(
        note.block_text.clone(),
        Some("eventmodeling note block".to_string()),
        EditorSemanticKind::String,
        note.block_span,
        note.block_span,
    ));
}

fn push_eventmodeling_gwt_facts(
    facts: &mut EditorSemanticFacts,
    gwt: &EventModelingGwtFacts,
    control: &crate::OperationControl,
) -> crate::OperationControlResult<()> {
    facts.push_symbol(
        EditorSemanticSymbol::reference(
            gwt.source_frame.text.clone(),
            Some("eventmodeling gwt source frame".to_string()),
            EditorSemanticKind::Namespace,
            gwt.source_frame.span,
            gwt.source_frame.span,
        )
        .with_rename_policy(EditorRenamePolicy::EventModelingFrameId),
    );
    for statement in gwt.given.iter().chain(&gwt.when).chain(&gwt.then) {
        control.checkpoint()?;
        facts.push_symbol(EditorSemanticSymbol::payload(
            statement.model_entity_type.text.clone(),
            Some("eventmodeling entity type".to_string()),
            EditorSemanticKind::String,
            statement.model_entity_type.span,
            statement.model_entity_type.span,
        ));
        facts.push_symbol(
            EditorSemanticSymbol::reference(
                statement.entity_reference.text.clone(),
                Some("eventmodeling gwt entity reference".to_string()),
                EditorSemanticKind::Object,
                statement.entity_reference.span,
                statement.entity_reference.span,
            )
            .with_rename_policy(EditorRenamePolicy::EventModelingId),
        );
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        EditorSemanticCompleteness, EditorSemanticRole, Engine, MermaidConfig,
        ParseDiagnosticSpanKind, ParseMetadata,
    };

    fn meta() -> ParseMetadata {
        ParseMetadata {
            diagram_type: "eventmodeling".to_string(),
            config: MermaidConfig::empty_object(),
            effective_config: MermaidConfig::empty_object(),
            title: None,
        }
    }

    #[test]
    fn eventmodeling_parser_can_cancel_inside_a_gwt_group() {
        let mut text = String::from("eventmodeling\ntf 001 evt Root\ngwt 001 given ");
        for index in 0..512 {
            text.push_str(&format!("evt Entity{index} "));
        }
        text.push_str("then evt Entity0\n");
        let control = crate::OperationControl::new();
        control.cancel_after_checkpoints(40);

        assert!(matches!(
            construct_eventmodeling_semantic_source_controlled(&text, &meta(), &control),
            Err(crate::OperationCancelled { .. })
        ));
    }

    #[test]
    fn eventmodeling_projection_can_cancel_between_frames() {
        let mut text = String::from("eventmodeling\n");
        for index in 0..512 {
            text.push_str(&format!("tf {:03} evt Entity{index}\n", index % 1000));
        }
        let source = construct_eventmodeling_semantic_source(&text, &meta())
            .unwrap_or_else(|_| panic!("large eventmodeling source"));
        let control = crate::OperationControl::new();
        control.cancel_after_checkpoints(20);

        assert!(matches!(
            source.into_render_model_controlled(&meta(), &control),
            Err(crate::OperationCancelled { .. })
        ));
    }

    #[test]
    fn late_syntax_error_recovery_can_cancel_while_projecting_prefix_facts() {
        let header = "eventmodeling";
        let mut source = format!("{header}\n");
        let mut syntax = EventModelingSyntaxFacts {
            header: Some(EventModelingFieldSpan {
                text: header.to_string(),
                span: SourceSpan::new(0, header.len()),
            }),
            ..EventModelingSyntaxFacts::default()
        };
        for index in 0..512 {
            let name = format!("Entity{index}");
            let name_start = source.len() + "entity ".len();
            source.push_str("entity ");
            source.push_str(&name);
            source.push('\n');
            syntax.model_entities.push(EventModelingFieldSpan {
                text: name,
                span: SourceSpan::new(name_start, source.len() - 1),
            });
        }
        let invalid_start = source.len();
        source.push_str("not-a-statement");
        let invalid_span = SourceSpan::new(invalid_start, source.len());
        let error = Error::diagram_parse_exact(
            "eventmodeling".to_string(),
            "late eventmodeling syntax error",
            invalid_span,
        );
        let control = crate::OperationControl::new();
        control.cancel_after_checkpoints(20);

        assert!(matches!(
            eventmodeling_failure_controlled(error, syntax, invalid_span, &control),
            Err(crate::OperationCancelled { .. })
        ));
    }

    #[test]
    fn parses_simple_model_with_full_syntax() {
        let model = parse_eventmodeling_model_for_render(
            "eventmodeling\ntimeframe 01 event Start\n",
            &meta(),
        )
        .unwrap();

        assert_eq!(model.frames.len(), 1);
        let frame = &model.frames[0];
        assert_eq!(frame.name, "01");
        assert_eq!(frame.model_entity_type, "event");
        assert_eq!(frame.entity_identifier, "Start");
    }

    #[test]
    fn parses_reset_frames_qualified_names_and_sources() {
        let model = parse_eventmodeling_model_for_render(
            r#"eventmodeling
tf 02 ui UI
resetframe 01 evt Product.PriceChanged
tf 03 evt Cart.ItemAdded ->> 01 ->> 02
"#,
            &meta(),
        )
        .unwrap();

        assert_eq!(model.frames.len(), 3);
        assert_eq!(model.frames[1].frame_kind, "resetframe");
        assert_eq!(model.frames[1].entity_identifier, "Product.PriceChanged");
        assert_eq!(model.frames[2].source_frames, ["01", "02"]);
    }

    #[test]
    fn qualified_names_allow_hidden_trivia_around_dots_and_normalize_segments() {
        let text = concat!(
            "eventmodeling\n",
            "entity Sales /* bounded context */ . Order\n",
            "tf 01 evt Sales \t.\n Order\n",
        );
        let model = parse_eventmodeling_model_for_render(text, &meta()).unwrap();

        assert_eq!(model.frames[0].entity_identifier, "Sales.Order");

        let facts = crate::family::test_support::editor_facts(
            parse_eventmodeling_json_and_editor_facts,
            text,
            &meta(),
        );
        assert!(
            facts
                .symbols
                .iter()
                .any(|symbol| symbol.name == "Sales.Order")
        );
    }

    #[test]
    fn yaml_is_hidden_where_the_upstream_terminal_can_start() {
        let text = concat!(
            "eventmodeling ---\n",
            "title: hidden document metadata\n",
            "---\n",
            "entity Cart\n",
            "tf 01 evt Cart\n",
        );
        let model = parse_eventmodeling_model_for_render(text, &meta()).unwrap();

        assert_eq!(model.frames[0].entity_identifier, "Cart");
    }

    #[test]
    fn captures_inline_data_and_data_blocks() {
        let model = parse_eventmodeling_model_for_render(
            r#"eventmodeling
tf 01 cmd AddItem { productId: 7 }
tf 02 evt ItemAdded [[ItemAddedData]]
tf 03 evt QuotedData `json`" { "ok": true } "

data ItemAddedData {
  productId: 7
}
"#,
            &meta(),
        )
        .unwrap();

        assert_eq!(
            model.frames[0].data_inline_value.as_deref(),
            Some("{ productId: 7 }")
        );
        assert_eq!(
            model.frames[1].data_reference.as_deref(),
            Some("ItemAddedData")
        );
        assert_eq!(
            model.frames[2].data_inline_value.as_deref(),
            Some("\" { \"ok\": true } \"")
        );
        assert_eq!(model.data_entities.len(), 1);
        assert!(
            model.data_entities[0]
                .data_block_value
                .contains("productId")
        );
    }

    #[test]
    fn combined_parse_constructs_once_and_preserves_all_projections() {
        let text = concat!(
            "eventmodeling\r\n",
            "tf 01 cmd AddItem { productId: 7 }\r\n",
            "tf 02 evt ItemAdded ->> 01 [[ItemAddedData]]\r\n",
            "data ItemAddedData {\r\n",
            "  productId: 7\r\n",
            "}\r\n",
        );
        let expected_json = parse_eventmodeling(text, &meta()).unwrap();
        let expected_model = parse_eventmodeling_model_for_render(text, &meta()).unwrap();

        reset_eventmodeling_syntax_construction_count();
        let (json, facts) =
            crate::family::test_support::into_result(parse_eventmodeling_json_and_editor_facts(
                text,
                &meta(),
                &crate::OperationControl::new(),
            ))
            .unwrap();

        assert_eq!(eventmodeling_syntax_construction_count(), 1);
        assert_eq!(json, expected_json);
        assert_eq!(
            render_model_to_compat_json(&expected_model, &meta()).unwrap(),
            expected_json
        );
        assert_eq!(
            json["frames"],
            serde_json::to_value(&expected_model.frames).unwrap()
        );
        assert_eq!(
            json["dataEntities"],
            serde_json::to_value(&expected_model.data_entities).unwrap()
        );

        let block_start = text.find("{\r\n").unwrap();
        let block_end = text.rfind('}').unwrap() + 1;
        let block = facts
            .symbols
            .iter()
            .find(|symbol| symbol.detail.as_deref() == Some("eventmodeling data block"))
            .expect("missing multiline data block fact");
        assert_eq!(block.span, SourceSpan::new(block_start, block_end));
        assert_eq!(block.selection, block.span);
        assert_eq!(block.name, &text[block_start..block_end]);
    }

    #[test]
    fn data_block_closing_brace_accepts_pinned_trailing_whitespace() {
        for suffix in ["  \n", "\t"] {
            let text = format!("eventmodeling\ndata Payload {{\nvalue\n}}{suffix}");
            let block_start = text.find('{').expect("opening brace");
            let block_end = text.rfind('}').expect("closing brace") + 1;

            let (json, facts) = crate::family::test_support::into_result(
                parse_eventmodeling_json_and_editor_facts(
                    &text,
                    &meta(),
                    &crate::OperationControl::new(),
                ),
            )
            .expect("EM_DATA_BLOCK accepts whitespace after its closing brace");
            let typed = parse_eventmodeling_model_for_render(&text, &meta())
                .expect("typed projection accepts the same block");
            let block = facts
                .symbols
                .iter()
                .find(|symbol| symbol.detail.as_deref() == Some("eventmodeling data block"))
                .expect("data block semantic fact");

            assert_eq!(facts.completeness, EditorSemanticCompleteness::Complete);
            assert_eq!(json["dataEntities"][0]["name"], "Payload");
            assert_eq!(typed.data_entities[0].name, "Payload");
            assert_eq!(block.span, SourceSpan::new(block_start, block_end));
            assert_eq!(block.name, text[block_start..block_end]);
        }
    }

    #[test]
    fn incomplete_data_block_recovers_from_the_single_construction() {
        let text = concat!(
            "eventmodeling\n",
            "tf 01 cmd AddItem\n",
            "data Broken {\n",
            "  productId: 7\n",
        );
        let Error::DiagramParse { diagnostic, .. } =
            parse_eventmodeling(text, &meta()).unwrap_err()
        else {
            panic!("expected eventmodeling parse error");
        };
        assert_eq!(
            diagnostic.span(),
            Some(SourceSpan::new(text.len(), text.len()))
        );
        assert_eq!(
            diagnostic.span_kind(),
            ParseDiagnosticSpanKind::InsertionPoint
        );

        reset_eventmodeling_syntax_construction_count();
        let facts = crate::family::test_support::editor_facts(
            parse_eventmodeling_json_and_editor_facts,
            text,
            &meta(),
        );
        assert_eq!(eventmodeling_syntax_construction_count(), 1);
        assert_eq!(facts.completeness, EditorSemanticCompleteness::Recovered);
        assert!(facts.symbols.iter().any(|symbol| symbol.name == "01"));
        assert!(facts.symbols.iter().any(|symbol| symbol.name == "Broken"));
        let block_start = text.find('{').unwrap();
        assert!(facts.symbols.iter().any(|symbol| {
            symbol.detail.as_deref() == Some("eventmodeling data block")
                && symbol.span == SourceSpan::new(block_start, text.len())
        }));
    }

    #[test]
    fn parse_eventmodeling_editor_facts_expose_parser_backed_spans() {
        let engine = Engine::new();
        let text = r#"eventmodeling
tf 01 cmd AddItem { productId: 7 }
tf 02 evt ItemAdded ->> 01 [[ItemAddedData]]

data ItemAddedData {
  productId: 7
}
"#;
        let facts = engine
            .parse_editor_semantic_facts_with_type_sync("eventmodeling", text)
            .unwrap()
            .unwrap();

        assert!(facts.symbols.iter().any(|symbol| symbol.name == "01"));
        assert!(
            facts
                .symbols
                .iter()
                .any(|symbol| symbol.name == "ItemAddedData")
        );
        assert!(
            facts
                .symbols
                .iter()
                .any(|symbol| symbol.name == "ItemAdded")
        );
        assert!(facts.symbols.iter().any(|symbol| symbol.name == "AddItem"));
        assert!(facts.symbols.iter().any(|symbol| {
            symbol.name == "01" && symbol.rename_policy == EditorRenamePolicy::EventModelingFrameId
        }));
        assert!(facts.symbols.iter().any(|symbol| {
            symbol.name == "ItemAddedData"
                && symbol.rename_policy == EditorRenamePolicy::EventModelingId
        }));

        let frame_start = text.find("01").unwrap();
        assert!(facts.expected_syntax.iter().any(|expected| {
            expected.kind == EditorExpectedSyntaxKind::Payload
                && expected.span == SourceSpan::new(frame_start, frame_start + "01".len())
        }));
    }

    #[test]
    fn editor_facts_assign_entity_reference_and_payload_roles_explicitly() {
        let text = concat!(
            "eventmodeling\n",
            "entity CartUpdated\n",
            "tf 001 cmd UpdateCart\n",
            "tf 002 evt Cart.Updated ->> 001 [[CartData]] `json`{ \"ok\": true }\n",
            "data CartData `json` {\n",
            "  { \"ok\": true }\n",
            "}\n",
            "note 002 `md` {\n",
            "  Cart changed\n",
            "}\n",
            "gwt 002\n",
            "  given\n",
            "    evt CartUpdated\n",
            "  then\n",
            "    evt CartUpdated\n",
        );
        let facts = crate::family::test_support::editor_facts(
            parse_eventmodeling_json_and_editor_facts,
            text,
            &meta(),
        );

        for (name, detail) in [
            ("CartUpdated", "eventmodeling model entity"),
            ("001", "eventmodeling frame"),
            ("002", "eventmodeling frame"),
            ("CartData", "eventmodeling data entity"),
        ] {
            assert!(facts.symbols.iter().any(|symbol| {
                symbol.name == name
                    && symbol.detail.as_deref() == Some(detail)
                    && symbol.role == EditorSemanticRole::Entity
            }));
        }
        for (name, detail) in [
            ("001", "eventmodeling source frame"),
            ("CartData", "eventmodeling data reference"),
            ("002", "eventmodeling note source frame"),
            ("002", "eventmodeling gwt source frame"),
            ("CartUpdated", "eventmodeling gwt entity reference"),
        ] {
            assert!(facts.symbols.iter().any(|symbol| {
                symbol.name == name
                    && symbol.detail.as_deref() == Some(detail)
                    && symbol.role == EditorSemanticRole::Reference
            }));
        }
        for detail in [
            "eventmodeling entity type",
            "eventmodeling entity identifier",
            "eventmodeling data type",
            "eventmodeling inline data",
            "eventmodeling data block",
            "eventmodeling note block",
        ] {
            assert!(facts.symbols.iter().any(|symbol| {
                symbol.detail.as_deref() == Some(detail)
                    && symbol.role == EditorSemanticRole::Payload
            }));
        }
    }

    #[test]
    fn parses_complete_pinned_langium_grammar_from_one_source() {
        let text = r#"eventmodeling
title Checkout flow
accTitle: Checkout accessibility title
accDescr {
  Checkout accessibility description
}
entity CartUpdated
entity ProductChanged
tf 001 cmd UpdateCart
tf 002 evt Cart.Updated ->> 001 [[CartData]] `json`{ "ok": true }
data CartData `json` {
  { "ok": true }
}
note 002 `md` {
  Cart changed
}
gwt 002
  given
    evt CartUpdated
  when
    cmd ProductChanged
  then
    evt CartUpdated
"#;

        let model = parse_eventmodeling_model_for_render(text, &meta()).unwrap();
        assert_eq!(model.title.as_deref(), Some("Checkout flow"));
        assert_eq!(
            model.acc_title.as_deref(),
            Some("Checkout accessibility title")
        );
        assert_eq!(
            model.acc_descr.as_deref(),
            Some("Checkout accessibility description")
        );
        assert_eq!(model.frames.len(), 2);
        assert_eq!(model.data_entities.len(), 1);

        let facts = crate::family::test_support::editor_facts(
            parse_eventmodeling_json_and_editor_facts,
            text,
            &meta(),
        );
        assert_eq!(facts.completeness, EditorSemanticCompleteness::Complete);
        assert!(facts.diagnostics.is_empty());
        for (name, detail) in [
            ("CartUpdated", "eventmodeling model entity"),
            ("CartData", "eventmodeling data entity"),
            ("002", "eventmodeling note source frame"),
            ("ProductChanged", "eventmodeling gwt entity reference"),
        ] {
            assert!(
                facts.symbols.iter().any(|symbol| {
                    symbol.name == name && symbol.detail.as_deref() == Some(detail)
                })
            );
        }
        for directive in ["title", "accTitle", "accDescr"] {
            assert!(
                facts
                    .directive_prefixes
                    .iter()
                    .any(|item| item == directive)
            );
        }
    }

    #[test]
    fn malformed_middle_statement_keeps_later_semantic_facts() {
        let text = concat!(
            "eventmodeling\r\n",
            "entity Before\r\n",
            "tf 01 invalid Broken\r\n",
            "entity After\r\n",
            "tf 02 evt Done\r\n",
        );
        let invalid_start = text.find("invalid").unwrap();
        let invalid_span = SourceSpan::new(invalid_start, invalid_start + "invalid".len());

        let Error::DiagramParse { diagnostic, .. } =
            parse_eventmodeling(text, &meta()).expect_err("strict parse must keep the first error")
        else {
            panic!("expected eventmodeling parse diagnostic");
        };
        assert_eq!(diagnostic.span(), Some(invalid_span));

        let facts = crate::family::test_support::editor_facts(
            parse_eventmodeling_json_and_editor_facts,
            text,
            &meta(),
        );
        assert_eq!(facts.completeness, EditorSemanticCompleteness::Recovered);
        assert!(facts.symbols.iter().any(|symbol| symbol.name == "After"));
        assert!(facts.symbols.iter().any(|symbol| symbol.name == "Done"));
    }

    #[test]
    fn cursor_skips_c_style_comments_with_crlf_unicode_content() {
        let text = concat!(
            "eventmodeling // 行内 🤓\r\n",
            "%% global preprocess comment\r\n",
            "/* 块注释\r\n",
            "   第二行 🤓 */\r\n",
            "entity After\r\n",
        );
        parse_eventmodeling(text, &meta()).expect("C-style comments are hidden family grammar");
        let facts = crate::family::test_support::editor_facts(
            parse_eventmodeling_json_and_editor_facts,
            text,
            &meta(),
        );

        assert_eq!(facts.completeness, EditorSemanticCompleteness::Complete);
        assert!(facts.symbols.iter().any(|symbol| symbol.name == "After"));
    }

    #[test]
    fn unterminated_block_comment_keeps_the_confirmed_comment_prefix() {
        let text = concat!(
            "eventmodeling\r\n",
            "entity Before\r\n",
            "/* 未闭合 🤓\r\n",
            "仍是注释",
        );
        let Error::DiagramParse { diagnostic, .. } = parse_eventmodeling(text, &meta())
            .expect_err("unterminated block comment must remain a strict parse error")
        else {
            panic!("expected eventmodeling parse diagnostic");
        };
        assert_eq!(
            diagnostic.span(),
            Some(SourceSpan::new(text.len(), text.len()))
        );
        assert_eq!(
            diagnostic.span_kind(),
            ParseDiagnosticSpanKind::InsertionPoint
        );

        let facts = crate::family::test_support::editor_facts(
            parse_eventmodeling_json_and_editor_facts,
            text,
            &meta(),
        );
        assert_eq!(facts.completeness, EditorSemanticCompleteness::Recovered);
    }

    #[test]
    fn rejects_tokens_outside_the_pinned_langium_grammar_with_exact_spans() {
        for (statement, invalid) in [
            ("tf 1000 evt Started", "1000"),
            ("tf xx evt Started", "xx"),
            ("tf 01 invalid Started", "invalid"),
            ("tf 01 evt Product..Started", "Product..Started"),
            ("data Cart `yaml` {\n}\n", "yaml"),
            ("unknown value", "unknown value"),
        ] {
            let text = format!("eventmodeling\n{statement}\n");
            let Error::DiagramParse { diagnostic, .. } =
                parse_eventmodeling(&text, &meta()).unwrap_err()
            else {
                panic!("expected eventmodeling parse diagnostic for {statement:?}");
            };
            let start = text.find(invalid).unwrap();
            assert_eq!(
                diagnostic.span(),
                Some(SourceSpan::new(start, start + invalid.len())),
                "statement: {statement:?}"
            );
            assert_eq!(
                diagnostic.span_kind(),
                ParseDiagnosticSpanKind::Exact,
                "statement: {statement:?}"
            );
        }
    }

    #[test]
    fn reports_link_and_source_type_validation_without_rejecting_render_semantics() {
        let text = r#"eventmodeling
entity KnownEvent
tf 01 rmo ReadModel
tf 02 evt Changed ->> 01 [[MissingData]]
note 99 {
  unresolved frame
}
gwt 02
  given
    evt MissingEntity
  then
    evt KnownEvent
"#;

        parse_eventmodeling(text, &meta()).unwrap();
        let facts = crate::family::test_support::editor_facts(
            parse_eventmodeling_json_and_editor_facts,
            text,
            &meta(),
        );
        assert_eq!(facts.completeness, EditorSemanticCompleteness::Complete);
        for expected in [
            "event can only receive input from a command",
            "unknown eventmodeling data reference 'MissingData'",
            "unknown eventmodeling frame reference '99'",
            "unknown eventmodeling model entity reference 'MissingEntity'",
        ] {
            assert!(
                facts
                    .diagnostics
                    .iter()
                    .any(|diagnostic| diagnostic.message.contains(expected)),
                "missing validation diagnostic containing {expected:?}: {:?}",
                facts.diagnostics
            );
        }
    }
}
