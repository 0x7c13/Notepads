use serde_json::Value;
#[cfg(test)]
use std::cell::Cell;

use crate::{
    EditorExpectedSyntax, EditorExpectedSyntaxKind, EditorSemanticFacts, EditorSemanticKind,
    EditorSemanticSymbol, Error, ParseMetadata, Result, SourceSpan,
    family::CombinedSemanticFailure,
};

use super::db::{MindmapDb, MindmapParseConfig};
use super::render_model::MindmapDiagramRenderModel;
use super::utils::{
    NodeSpec, NodeSpecContinuation, NodeSpecError, parse_node_spec, starts_node_spec,
    strip_inline_comment,
};
use crate::diagrams::scan::{split_indent, starts_with_case_insensitive};

#[cfg(test)]
thread_local! {
    static MINDMAP_SYNTAX_CONSTRUCTION_COUNT: Cell<usize> = const { Cell::new(0) };
}

#[cfg(test)]
pub(crate) fn reset_mindmap_syntax_construction_count() {
    MINDMAP_SYNTAX_CONSTRUCTION_COUNT.set(0);
}

#[cfg(test)]
pub(crate) fn mindmap_syntax_construction_count() -> usize {
    MINDMAP_SYNTAX_CONSTRUCTION_COUNT.get()
}

pub(crate) fn parse_mindmap(code: &str, meta: &ParseMetadata) -> Result<Value> {
    let model = parse_mindmap_semantic_source(code, meta)?.into_render_model(meta)?;
    super::render_model_to_compat_json(&model, meta)
}

pub(crate) fn parse_mindmap_json_and_editor_facts(
    code: &str,
    meta: &ParseMetadata,
    control: &crate::OperationControl,
) -> crate::OperationControlResult<crate::family::CombinedSemanticParse> {
    control.checkpoint()?;
    let construction = match construct_mindmap_semantic_source_controlled(code, meta, control)? {
        Ok(source) => {
            let editor_facts = source.editor_facts.clone();
            let model = match source.into_render_model_controlled(meta, control)? {
                Ok(model) => super::render_model::render_model_to_compat_json_controlled(
                    &model, meta, control,
                )?,
                Err(error) => Err(error),
            };
            Ok((model, editor_facts))
        }
        Err(error) => Err(error),
    };
    let parsed = crate::family::CombinedSemanticParse::from_construction(
        construction,
        |parts| parts,
        CombinedSemanticFailure::into_parts,
    );
    control.checkpoint()?;
    Ok(parsed)
}

#[cfg(test)]
pub(crate) fn parse_mindmap_model_for_render(
    code: &str,
    meta: &ParseMetadata,
) -> Result<MindmapDiagramRenderModel> {
    parse_mindmap_semantic_source(code, meta)?.into_render_model(meta)
}

pub(crate) fn parse_mindmap_model_for_render_controlled(
    code: &str,
    meta: &ParseMetadata,
    control: &crate::OperationControl,
) -> crate::OperationControlResult<Result<MindmapDiagramRenderModel>> {
    let construction = construct_mindmap_semantic_source_controlled(code, meta, control)?;
    match construction {
        Ok(source) => source.into_render_model_controlled(meta, control),
        Err(error) => Ok(Err(error.into_error())),
    }
}

struct MindmapSemanticSource {
    db: MindmapDb,
    editor_facts: EditorSemanticFacts,
}

impl MindmapSemanticSource {
    fn into_render_model(self, meta: &ParseMetadata) -> Result<MindmapDiagramRenderModel> {
        let control = crate::OperationControl::new();
        self.into_render_model_controlled(meta, &control)
            .expect("a private parse control cannot be cancelled")
    }

    fn into_render_model_controlled(
        self,
        meta: &ParseMetadata,
        control: &crate::OperationControl,
    ) -> crate::OperationControlResult<Result<MindmapDiagramRenderModel>> {
        control.checkpoint()?;
        let mut db = self.db;
        let Some(root_id) = db.get_mindmap().map(|n| n.id) else {
            return Ok(Ok(MindmapDiagramRenderModel::default()));
        };

        db.assign_sections_controlled(root_id, None, control)?;

        let nodes =
            db.to_layout_nodes_for_render_controlled(root_id, &meta.effective_config, control)?;
        let edges = db.to_edges_for_render_controlled(root_id, &meta.effective_config, control)?;
        control.checkpoint()?;
        Ok(Ok(MindmapDiagramRenderModel { nodes, edges }))
    }
}

fn parse_mindmap_semantic_source(
    code: &str,
    meta: &ParseMetadata,
) -> Result<MindmapSemanticSource> {
    construct_mindmap_semantic_source(code, meta).map_err(CombinedSemanticFailure::into_error)
}

fn construct_mindmap_semantic_source(
    code: &str,
    meta: &ParseMetadata,
) -> std::result::Result<MindmapSemanticSource, CombinedSemanticFailure> {
    construct_mindmap_semantic_source_controlled(code, meta, &crate::OperationControl::new())
        .expect("a private parse control cannot be cancelled")
}

fn construct_mindmap_semantic_source_controlled(
    code: &str,
    meta: &ParseMetadata,
    control: &crate::OperationControl,
) -> crate::OperationControlResult<
    std::result::Result<MindmapSemanticSource, CombinedSemanticFailure>,
> {
    control.checkpoint()?;
    #[cfg(test)]
    MINDMAP_SYNTAX_CONSTRUCTION_COUNT.set(MINDMAP_SYNTAX_CONSTRUCTION_COUNT.get() + 1);

    let MindmapParseOutcome {
        parsed,
        first_error,
    } = parse_mindmap_lines(code, meta, control)?;
    control.checkpoint()?;
    let editor_facts = mindmap_editor_facts_from_parsed(&parsed, control)?;

    if let Some(error) = first_error {
        return Ok(Err(CombinedSemanticFailure::parser_recovery(
            "mindmap",
            error,
            editor_facts,
        )));
    }

    let db = match mindmap_db_from_events(parsed.events, meta, control)? {
        Ok(db) => db,
        Err(error) => {
            return Ok(Err(CombinedSemanticFailure::parser_recovery(
                "mindmap",
                error,
                editor_facts,
            )));
        }
    };
    control.checkpoint()?;
    Ok(Ok(MindmapSemanticSource { db, editor_facts }))
}

fn mindmap_db_from_events(
    events: Vec<MindmapParsedEvent>,
    meta: &ParseMetadata,
    control: &crate::OperationControl,
) -> crate::OperationControlResult<Result<MindmapDb>> {
    let mut db = MindmapDb::default();
    db.clear();
    let parse_config = MindmapParseConfig::from_config(&meta.effective_config);

    for (index, event) in events.into_iter().enumerate() {
        if index % 128 == 0 {
            control.checkpoint()?;
        }
        match event {
            MindmapParsedEvent::Node(node) => {
                let selection = node.selection;
                if let Err(error) = db.add_node_controlled(
                    super::db::MindmapNodeInput {
                        indent_level: node.indent as i32,
                        id_raw: &node.id_raw,
                        descr_raw: &node.descr_raw,
                        descr_is_markdown: node.descr_is_markdown,
                        ty: node.ty,
                        diagram_type: &meta.diagram_type,
                    },
                    &meta.effective_config,
                    parse_config,
                    control,
                )? {
                    return Ok(Err(error.with_exact_span_if_missing(selection)));
                }
            }
            MindmapParsedEvent::Class(class) => {
                db.decorate_last(Some(class.value), None, &meta.effective_config);
            }
            MindmapParsedEvent::Icon(icon) => {
                db.decorate_last(None, Some(icon.value), &meta.effective_config);
            }
        }
    }

    control.checkpoint()?;
    Ok(Ok(db))
}

#[derive(Debug, Clone)]
struct MindmapParsedNodeLine {
    indent: usize,
    id_raw: String,
    descr_raw: String,
    descr_is_markdown: bool,
    ty: i32,
    span: SourceSpan,
    selection: SourceSpan,
    payload_span: Option<SourceSpan>,
}

#[derive(Debug, Clone)]
struct MindmapParsedPayloadLine {
    value: String,
    span: SourceSpan,
    tokens: Vec<MindmapParsedPayloadToken>,
}

#[derive(Debug, Clone)]
struct MindmapParsedPayloadToken {
    value: String,
    selection: SourceSpan,
}

#[derive(Debug, Clone)]
enum MindmapParsedEvent {
    Node(MindmapParsedNodeLine),
    Class(MindmapParsedPayloadLine),
    Icon(MindmapParsedPayloadLine),
}

#[derive(Debug, Default)]
struct MindmapParsedLines {
    events: Vec<MindmapParsedEvent>,
    directive_prefixes: Vec<String>,
}

struct MindmapParseOutcome {
    parsed: MindmapParsedLines,
    first_error: Option<Error>,
}

fn mindmap_editor_facts_from_parsed(
    parsed: &MindmapParsedLines,
    control: &crate::OperationControl,
) -> crate::OperationControlResult<EditorSemanticFacts> {
    let mut facts = EditorSemanticFacts::new();
    for (index, prefix) in parsed.directive_prefixes.iter().enumerate() {
        if index % 128 == 0 {
            control.checkpoint()?;
        }
        facts.push_directive_prefix(prefix.clone());
    }
    for (index, event) in parsed.events.iter().enumerate() {
        if index % 128 == 0 {
            control.checkpoint()?;
        }
        match event {
            MindmapParsedEvent::Node(node) => {
                facts.push_expected_syntax(EditorExpectedSyntax::new(
                    EditorExpectedSyntaxKind::NodeIdentifier,
                    node.selection,
                ));
                if let Some(payload_span) = node.payload_span {
                    facts.push_expected_syntax(EditorExpectedSyntax::new(
                        EditorExpectedSyntaxKind::Payload,
                        payload_span,
                    ));
                    facts.push_symbol(EditorSemanticSymbol::payload(
                        node.descr_raw.clone(),
                        Some("mindmap node label".to_string()),
                        EditorSemanticKind::String,
                        payload_span,
                        payload_span,
                    ));
                }
                facts.push_symbol(EditorSemanticSymbol::new(
                    node.id_raw.clone(),
                    Some("mindmap node".to_string()),
                    EditorSemanticKind::Namespace,
                    node.span,
                    node.selection,
                ));
            }
            MindmapParsedEvent::Class(class) => {
                for (index, token) in class.tokens.iter().enumerate() {
                    if index % 128 == 0 {
                        control.checkpoint()?;
                    }
                    facts.push_symbol(EditorSemanticSymbol::payload(
                        token.value.clone(),
                        Some("mindmap class".to_string()),
                        EditorSemanticKind::Property,
                        class.span,
                        token.selection,
                    ));
                }
            }
            MindmapParsedEvent::Icon(icon) => {
                for (index, token) in icon.tokens.iter().enumerate() {
                    if index % 128 == 0 {
                        control.checkpoint()?;
                    }
                    facts.push_symbol(EditorSemanticSymbol::payload(
                        token.value.clone(),
                        Some("mindmap icon".to_string()),
                        EditorSemanticKind::String,
                        icon.span,
                        token.selection,
                    ));
                }
            }
        }
    }
    control.checkpoint()?;
    Ok(facts)
}

fn absolute_mindmap_span(base: usize, span: SourceSpan) -> SourceSpan {
    SourceSpan::new(base + span.start, base + span.end)
}

fn mindmap_payload_tokens(raw: &str, raw_start: usize) -> Vec<MindmapParsedPayloadToken> {
    let mut tokens = Vec::new();
    let mut cursor = 0usize;
    while cursor < raw.len() {
        let Some(ch) = raw[cursor..].chars().next() else {
            break;
        };
        if ch.is_whitespace() {
            cursor += ch.len_utf8();
            continue;
        }
        let start = cursor;
        cursor += ch.len_utf8();
        while cursor < raw.len() {
            let Some(ch) = raw[cursor..].chars().next() else {
                break;
            };
            if ch.is_whitespace() {
                break;
            }
            cursor += ch.len_utf8();
        }
        tokens.push(MindmapParsedPayloadToken {
            value: raw[start..cursor].to_string(),
            selection: SourceSpan::new(raw_start + start, raw_start + cursor),
        });
    }
    tokens
}

fn record_mindmap_error(
    first_error: &mut Option<Error>,
    meta: &ParseMetadata,
    message: impl Into<String>,
    span: SourceSpan,
) {
    first_error.get_or_insert_with(|| {
        Error::diagram_parse_exact(meta.diagram_type.clone(), message.into(), span)
    });
}

#[derive(Clone, Copy, PartialEq, Eq)]
struct MindmapContinuationState {
    syntax: NodeSpecContinuation,
    original_indent: usize,
}

struct MindmapPendingSyntax {
    state: MindmapContinuationState,
    statement_span: SourceSpan,
    error: Box<NodeSpecError>,
}

enum MindmapLineOutcome {
    Done,
    NeedMoreInput(MindmapPendingSyntax),
}

struct PendingMindmapLine {
    text: String,
    start: usize,
    syntax: MindmapPendingSyntax,
}

fn handle_mindmap_line(
    line: &str,
    line_start: usize,
    parsed: &mut MindmapParsedLines,
    first_error: &mut Option<Error>,
    meta: &ParseMetadata,
) -> MindmapLineOutcome {
    if line.trim().is_empty() {
        return MindmapLineOutcome::Done;
    }

    let (indent, rest) = split_indent(line);
    let rest_offset = line.len().saturating_sub(rest.len());
    let rest = rest.trim_end();
    if rest.is_empty() {
        return MindmapLineOutcome::Done;
    }
    let statement_start = line_start + rest_offset;
    let statement_span = SourceSpan::new(statement_start, statement_start + rest.len());

    if starts_with_case_insensitive(rest, "::icon(") {
        let after = &rest["::icon(".len()..];
        let Some(end) = after.find(')') else {
            record_mindmap_error(
                first_error,
                meta,
                "unterminated mindmap icon directive",
                statement_span,
            );
            return MindmapLineOutcome::Done;
        };
        let icon_raw = &after[..end];
        let icon = icon_raw.trim();
        if icon.is_empty() {
            record_mindmap_error(first_error, meta, "mindmap icon is empty", statement_span);
            return MindmapLineOutcome::Done;
        }
        let icon_leading = icon_raw.len() - icon_raw.trim_start().len();
        let selection_start = statement_start + "::icon(".len() + icon_leading;
        let token = MindmapParsedPayloadToken {
            value: icon.to_string(),
            selection: SourceSpan::new(selection_start, selection_start + icon.len()),
        };
        parsed.directive_prefixes.push("::icon".to_string());
        parsed
            .events
            .push(MindmapParsedEvent::Icon(MindmapParsedPayloadLine {
                value: icon.to_string(),
                span: statement_span,
                tokens: vec![token],
            }));
        return MindmapLineOutcome::Done;
    }

    if let Some(after) = rest.strip_prefix(":::") {
        let class = after.trim();
        if class.is_empty() {
            record_mindmap_error(first_error, meta, "mindmap class is empty", statement_span);
            return MindmapLineOutcome::Done;
        }
        let class_leading = after.len() - after.trim_start().len();
        let class_start = statement_start + ":::".len() + class_leading;
        let tokens = mindmap_payload_tokens(class, class_start);
        parsed.directive_prefixes.push(":::".to_string());
        parsed
            .events
            .push(MindmapParsedEvent::Class(MindmapParsedPayloadLine {
                value: class.to_string(),
                span: statement_span,
                tokens,
            }));
        return MindmapLineOutcome::Done;
    }

    let rest = strip_inline_comment(rest).trim_end();
    if rest.is_empty() {
        return MindmapLineOutcome::Done;
    }
    let statement_span = SourceSpan::new(statement_start, statement_start + rest.len());
    match parse_node_spec(rest) {
        Ok(NodeSpec {
            id_raw,
            descr_raw,
            ty,
            descr_is_markdown,
            trace,
        }) => {
            let selection = trace
                .id_span
                .map(|span| absolute_mindmap_span(statement_start, span))
                .unwrap_or(statement_span);
            let payload_span = trace
                .description_span
                .map(|span| absolute_mindmap_span(statement_start, span));
            parsed
                .events
                .push(MindmapParsedEvent::Node(MindmapParsedNodeLine {
                    indent,
                    id_raw,
                    descr_raw,
                    descr_is_markdown,
                    ty,
                    span: statement_span,
                    selection,
                    payload_span,
                }));
            MindmapLineOutcome::Done
        }
        Err(error) if error.continuation.is_some() => {
            let continuation = error
                .continuation
                .expect("continuation branch requires typed syntax state");
            MindmapLineOutcome::NeedMoreInput(MindmapPendingSyntax {
                state: MindmapContinuationState {
                    syntax: continuation,
                    original_indent: indent,
                },
                statement_span,
                error,
            })
        }
        Err(error) => {
            record_mindmap_error(first_error, meta, error.message, statement_span);
            MindmapLineOutcome::Done
        }
    }
}

fn finish_pending_mindmap_line(
    pending: PendingMindmapLine,
    first_error: &mut Option<Error>,
    meta: &ParseMetadata,
) {
    let MindmapPendingSyntax {
        statement_span,
        error,
        ..
    } = pending.syntax;
    let NodeSpecError { message, .. } = *error;
    record_mindmap_error(first_error, meta, message, statement_span);
}

fn is_safe_mindmap_statement(line: &str) -> bool {
    let (_, rest) = split_indent(line);
    let rest = strip_inline_comment(rest).trim();
    if rest.is_empty() {
        return false;
    }
    if starts_with_case_insensitive(rest, "::icon(") || rest.starts_with(":::") {
        return true;
    }

    starts_node_spec(rest)
}

fn pending_mindmap_line_should_synchronize(
    pending: &PendingMindmapLine,
    physical_line: &str,
) -> bool {
    let (indent, rest) = split_indent(physical_line);
    let rest = rest.trim_end();
    if rest.is_empty()
        || pending.syntax.state.syntax.has_open_text()
        || indent > pending.syntax.state.original_indent
    {
        return false;
    }
    if indent == pending.syntax.state.original_indent
        && rest.starts_with(pending.syntax.state.syntax.expected_closing())
    {
        return false;
    }
    indent <= pending.syntax.state.original_indent && is_safe_mindmap_statement(physical_line)
}

fn process_mindmap_physical_line(
    physical_line: &str,
    line_start: usize,
    pending: &mut Option<PendingMindmapLine>,
    parsed: &mut MindmapParsedLines,
    first_error: &mut Option<Error>,
    meta: &ParseMetadata,
) {
    if pending
        .as_ref()
        .is_some_and(|current| pending_mindmap_line_should_synchronize(current, physical_line))
    {
        let current = pending.take().expect("checked pending mindmap line");
        finish_pending_mindmap_line(current, first_error, meta);
    }

    if let Some(mut current) = pending.take() {
        current.text.push('\n');
        current.text.push_str(physical_line);
        match handle_mindmap_line(&current.text, current.start, parsed, first_error, meta) {
            MindmapLineOutcome::Done => {}
            MindmapLineOutcome::NeedMoreInput(syntax) => {
                current.syntax = syntax;
                *pending = Some(current);
            }
        }
        return;
    }

    if let MindmapLineOutcome::NeedMoreInput(syntax) =
        handle_mindmap_line(physical_line, line_start, parsed, first_error, meta)
    {
        *pending = Some(PendingMindmapLine {
            text: physical_line.to_string(),
            start: line_start,
            syntax,
        });
    }
}

fn parse_mindmap_lines(
    code: &str,
    meta: &ParseMetadata,
    control: &crate::OperationControl,
) -> crate::OperationControlResult<MindmapParseOutcome> {
    control.checkpoint()?;
    let mut lines = code.split_inclusive('\n').peekable();
    let mut offset = 0usize;
    let mut parsed = MindmapParsedLines::default();
    let mut first_error = None;
    let mut header_tail: Option<(&str, usize)> = None;
    let mut found_header = false;

    for segment in lines.by_ref() {
        control.checkpoint()?;
        let line_start = offset;
        offset += segment.len();
        let line = segment.strip_suffix('\n').unwrap_or(segment);
        let statement = strip_inline_comment(line);
        let trimmed = statement.trim();
        if trimmed.is_empty() {
            continue;
        }
        let leading = statement.len() - statement.trim_start().len();
        let keyword_start = line_start + leading;
        let is_header = trimmed.eq_ignore_ascii_case("mindmap");
        let has_tail = starts_with_case_insensitive(trimmed, "mindmap")
            && trimmed.len() > "mindmap".len()
            && trimmed["mindmap".len()..]
                .chars()
                .next()
                .is_some_and(char::is_whitespace);
        if is_header || has_tail {
            found_header = true;
            if has_tail {
                let tail_start = leading + "mindmap".len();
                header_tail = Some((&line[tail_start..], line_start + tail_start));
            }
            break;
        }
        record_mindmap_error(
            &mut first_error,
            meta,
            "expected mindmap header",
            SourceSpan::new(keyword_start, keyword_start + trimmed.len()),
        );
        break;
    }

    if !found_header {
        if first_error.is_none() {
            first_error = Some(Error::diagram_parse_insertion_point(
                meta.diagram_type.clone(),
                "expected mindmap header",
                code.len(),
            ));
        }
        return Ok(MindmapParseOutcome {
            parsed,
            first_error,
        });
    }

    let mut pending = None;
    if let Some((tail, tail_start)) = header_tail {
        process_mindmap_physical_line(
            tail,
            tail_start,
            &mut pending,
            &mut parsed,
            &mut first_error,
            meta,
        );
    }
    for segment in lines {
        control.checkpoint()?;
        let line_start = offset;
        offset += segment.len();
        let line = segment.strip_suffix('\n').unwrap_or(segment);
        process_mindmap_physical_line(
            line,
            line_start,
            &mut pending,
            &mut parsed,
            &mut first_error,
            meta,
        );
    }
    if let Some(pending) = pending {
        finish_pending_mindmap_line(pending, &mut first_error, meta);
    }

    control.checkpoint()?;
    Ok(MindmapParseOutcome {
        parsed,
        first_error,
    })
}
