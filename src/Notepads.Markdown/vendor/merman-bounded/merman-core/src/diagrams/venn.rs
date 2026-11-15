use crate::{
    EditorExpectedSyntax, EditorExpectedSyntaxKind, EditorSemanticFacts, EditorSemanticKind,
    EditorSemanticSymbol, Error, ParseMetadata, Result, SourceSpan,
};
use serde_json::{Value, json};
use std::collections::{BTreeMap, HashSet};

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, PartialEq)]
pub struct VennSubsetRenderModel {
    pub sets: Vec<String>,
    pub size: f64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub label: Option<String>,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, PartialEq, Eq)]
pub struct VennTextNodeRenderModel {
    pub sets: Vec<String>,
    pub id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub label: Option<String>,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, PartialEq, Eq)]
pub struct VennStyleEntryRenderModel {
    pub targets: Vec<String>,
    pub styles: BTreeMap<String, String>,
}

#[derive(Debug, Clone, Default, serde::Serialize, serde::Deserialize, PartialEq)]
pub struct VennDiagramRenderModel {
    #[serde(default, rename = "accTitle")]
    pub acc_title: Option<String>,
    #[serde(default, rename = "accDescr")]
    pub acc_descr: Option<String>,
    #[serde(default)]
    pub title: Option<String>,
    #[serde(default)]
    pub subsets: Vec<VennSubsetRenderModel>,
    #[serde(default, rename = "textNodes")]
    pub text_nodes: Vec<VennTextNodeRenderModel>,
    #[serde(default, rename = "styleEntries")]
    pub style_entries: Vec<VennStyleEntryRenderModel>,
}

impl VennDiagramRenderModel {
    pub(crate) fn sanitize_common_db_fields(&mut self, config: &crate::MermaidConfig) {
        crate::common_db::sanitize_optional_title(&mut self.title, config);
        crate::common_db::sanitize_optional_acc_title(&mut self.acc_title, config);
        crate::common_db::sanitize_optional_acc_descr(&mut self.acc_descr, config);
    }
}

#[derive(Debug)]
struct VennSemanticState {
    model: VennDiagramRenderModel,
    known_sets: HashSet<String>,
    current_sets: Option<Vec<String>>,
    indent_mode: bool,
    editor_facts: EditorSemanticFacts,
}

impl VennSemanticState {
    fn new() -> Self {
        Self {
            model: VennDiagramRenderModel::default(),
            known_sets: HashSet::new(),
            current_sets: None,
            indent_mode: false,
            editor_facts: EditorSemanticFacts::new(),
        }
    }

    fn add_subset(&mut self, identifiers: Vec<String>, label: Option<String>, size: Option<f64>) {
        let mut sets = normalize_identifier_list(identifiers);
        let resolved_size = size.unwrap_or_else(|| 10.0 / (sets.len() as f64).powi(2));
        self.current_sets = Some(sets.clone());

        if sets.len() == 1 {
            self.known_sets.insert(sets[0].clone());
        }

        self.model.subsets.push(VennSubsetRenderModel {
            sets: std::mem::take(&mut sets),
            size: resolved_size,
            label: label
                .map(|value| normalize_text(&value))
                .filter(|value| !value.is_empty()),
        });
    }

    fn validate_union_identifiers(
        &self,
        identifiers: &[String],
        meta: &ParseMetadata,
    ) -> Result<()> {
        let unknown = identifiers
            .iter()
            .map(|identifier| normalize_text(identifier))
            .filter(|identifier| !self.known_sets.contains(identifier))
            .collect::<Vec<_>>();

        if unknown.is_empty() {
            Ok(())
        } else {
            Err(parse_error(
                meta,
                format!("unknown set identifier: {}", unknown.join(", ")),
            ))
        }
    }

    fn add_text(&mut self, identifiers: Vec<String>, id: String, label: Option<String>) {
        self.model.text_nodes.push(VennTextNodeRenderModel {
            sets: normalize_identifier_list(identifiers),
            id: normalize_text(&id),
            label: label
                .map(|value| normalize_text(&value))
                .filter(|value| !value.is_empty()),
        });
    }

    fn add_style(&mut self, targets: Vec<String>, styles: BTreeMap<String, String>) {
        self.model.style_entries.push(VennStyleEntryRenderModel {
            targets: normalize_identifier_list(targets),
            styles,
        });
    }
}

struct VennSemanticSource {
    model: VennDiagramRenderModel,
    editor_facts: EditorSemanticFacts,
}

struct VennParseOutcome {
    source: VennSemanticSource,
    first_error: Option<Error>,
}

impl VennParseOutcome {
    fn into_strict_source(self) -> Result<VennSemanticSource> {
        match self.first_error {
            Some(error) => Err(error),
            None => Ok(self.source),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum TextIdKind {
    IdentifierOrString,
    Numeric,
}

#[derive(Debug, Clone)]
struct VennFieldSpan {
    text: String,
    span: SourceSpan,
    selection: SourceSpan,
}

struct VennCursor<'a> {
    input: &'a str,
    line_start: usize,
    pos: usize,
}

impl<'a> VennCursor<'a> {
    fn new(input: &'a str, line_start: usize) -> Self {
        Self {
            input,
            line_start,
            pos: 0,
        }
    }

    fn remaining(&self) -> &'a str {
        &self.input[self.pos..]
    }

    fn skip_ws(&mut self) {
        self.pos += self
            .remaining()
            .chars()
            .take_while(|ch| matches!(ch, ' ' | '\t'))
            .map(char::len_utf8)
            .sum::<usize>();
    }

    fn abs_start(&self) -> usize {
        self.line_start + self.pos
    }

    fn take_identifier_like(&mut self, meta: &ParseMetadata) -> Result<VennFieldSpan> {
        self.skip_ws();
        let token_start = self.abs_start();
        let rest = self.remaining();

        if let Some((value, after)) = parse_string_token(rest) {
            let consumed = rest.len() - after.len();
            self.pos += consumed;
            return Ok(VennFieldSpan {
                text: normalize_text(&value),
                span: SourceSpan::new(token_start, token_start + consumed),
                selection: SourceSpan::new(token_start + 1, token_start + consumed - 1),
            });
        }

        let Some((value, after)) = parse_bare_identifier_token(rest) else {
            return Err(parse_error(meta, "expected identifier"));
        };
        let consumed = rest.len() - after.len();
        self.pos += consumed;
        Ok(VennFieldSpan {
            text: value.to_string(),
            span: SourceSpan::new(token_start, token_start + consumed),
            selection: SourceSpan::new(token_start, token_start + consumed),
        })
    }

    fn take_bare_identifier(&mut self, meta: &ParseMetadata) -> Result<VennFieldSpan> {
        self.skip_ws();
        let token_start = self.abs_start();
        let rest = self.remaining();
        let Some((value, after)) = parse_bare_identifier_token(rest) else {
            return Err(parse_error(meta, "expected identifier"));
        };
        let consumed = rest.len() - after.len();
        self.pos += consumed;
        Ok(VennFieldSpan {
            text: value.to_string(),
            span: SourceSpan::new(token_start, token_start + consumed),
            selection: SourceSpan::new(token_start, token_start + consumed),
        })
    }

    fn take_text_id(&mut self, meta: &ParseMetadata) -> Result<(VennFieldSpan, TextIdKind)> {
        self.skip_ws();
        let token_start = self.abs_start();
        let rest = self.remaining();

        if let Some((value, after)) = parse_string_token(rest) {
            let consumed = rest.len() - after.len();
            self.pos += consumed;
            return Ok((
                VennFieldSpan {
                    text: normalize_text(&value),
                    span: SourceSpan::new(token_start, token_start + consumed),
                    selection: SourceSpan::new(token_start + 1, token_start + consumed - 1),
                },
                TextIdKind::IdentifierOrString,
            ));
        }

        if let Some((value, after)) = parse_numeric_token(rest) {
            let consumed = rest.len() - after.len();
            self.pos += consumed;
            return Ok((
                VennFieldSpan {
                    text: value,
                    span: SourceSpan::new(token_start, token_start + consumed),
                    selection: SourceSpan::new(token_start, token_start + consumed),
                },
                TextIdKind::Numeric,
            ));
        }

        let token = self.take_identifier_like(meta)?;
        Ok((token, TextIdKind::IdentifierOrString))
    }

    fn take_optional_bracket_label(
        &mut self,
        meta: &ParseMetadata,
    ) -> Result<Option<VennFieldSpan>> {
        self.skip_ws();
        let Some(rest) = self.remaining().strip_prefix('[') else {
            return Ok(None);
        };

        let token_start = self.abs_start();
        if let Some(rest) = rest.strip_prefix('"') {
            let Some(end) = rest.find("\"]") else {
                return Err(parse_error(meta, "unterminated bracket label"));
            };
            let text = rest[..end].to_string();
            let consumed = end + 4;
            self.pos += consumed;
            if text.is_empty() {
                return Ok(None);
            }
            return Ok(Some(VennFieldSpan {
                text,
                span: SourceSpan::new(token_start, token_start + consumed),
                selection: SourceSpan::new(token_start + 2, token_start + 2 + end),
            }));
        }

        let Some(end) = rest.find(']') else {
            return Err(parse_error(meta, "unterminated bracket label"));
        };
        if rest[..end].contains('"') {
            return Err(parse_error(meta, "invalid bracket label"));
        }
        let raw = &rest[..end];
        let text = raw.trim().to_string();
        let consumed = end + 2;
        self.pos += consumed;
        if text.is_empty() {
            return Ok(None);
        }
        let leading = raw.len() - raw.trim_start().len();
        let trailing = raw.len() - raw.trim_end().len();
        let selection = SourceSpan::new(
            token_start + 1 + leading,
            token_start + 1 + raw.len() - trailing,
        );
        Ok(Some(VennFieldSpan {
            text,
            span: SourceSpan::new(token_start, token_start + consumed),
            selection,
        }))
    }

    fn take_optional_size(&mut self, meta: &ParseMetadata) -> Result<Option<VennFieldSpan>> {
        self.skip_ws();
        let Some(_) = self.remaining().strip_prefix(':') else {
            return Ok(None);
        };

        self.pos += 1;
        self.skip_ws();
        let token_start = self.abs_start();
        let rest = self.remaining();
        let Some((value, after)) = parse_numeric_token(rest) else {
            return Err(parse_error(meta, "expected numeric"));
        };
        let consumed = rest.len() - after.len();
        self.pos += consumed;
        Ok(Some(VennFieldSpan {
            text: value,
            span: SourceSpan::new(token_start, token_start + consumed),
            selection: SourceSpan::new(token_start, token_start + consumed),
        }))
    }

    fn take_remaining_payload(&mut self) -> Option<VennFieldSpan> {
        self.skip_ws();
        let rest = self.remaining();
        let trimmed = rest.trim_end();
        if trimmed.is_empty() {
            return None;
        }

        let leading = rest.len() - rest.trim_start().len();
        let trailing = rest.len() - trimmed.len();
        let span_start = self.abs_start() + leading;
        let span_end = self.abs_start() + rest.len() - trailing;
        self.pos = self.input.len();

        let selection = if trimmed.len() >= 2 && trimmed.starts_with('"') && trimmed.ends_with('"')
        {
            SourceSpan::new(span_start + 1, span_end - 1)
        } else {
            SourceSpan::new(span_start, span_end)
        };

        Some(VennFieldSpan {
            text: normalize_text(trimmed),
            span: SourceSpan::new(span_start, span_end),
            selection,
        })
    }

    fn take_style_value(&mut self, meta: &ParseMetadata) -> Result<VennFieldSpan> {
        self.skip_ws();
        let token_start = self.abs_start();
        let rest = self.remaining();
        let (raw, _after_raw) = take_style_value_segment(rest);
        let trimmed = raw.trim();
        if trimmed.is_empty() {
            return Err(parse_error(meta, "expected style value"));
        }

        let leading = raw.len() - raw.trim_start().len();
        let span_start = token_start + leading;
        let span_end = span_start + trimmed.len();
        self.pos += raw.len();
        let text = if trimmed.len() >= 2 && trimmed.starts_with('"') && trimmed.ends_with('"') {
            normalize_text(trimmed)
        } else {
            style_value_tokens(trimmed, meta)?.join(" ")
        };

        let selection = if trimmed.len() >= 2 && trimmed.starts_with('"') && trimmed.ends_with('"')
        {
            SourceSpan::new(span_start + 1, span_end - 1)
        } else {
            SourceSpan::new(span_start, span_end)
        };

        Ok(VennFieldSpan {
            text,
            span: SourceSpan::new(span_start, span_end),
            selection,
        })
    }

    fn expect_end(&self, meta: &ParseMetadata) -> Result<()> {
        if self.remaining().trim().is_empty() {
            Ok(())
        } else {
            Err(parse_error(
                meta,
                format!(
                    "unexpected trailing venn tokens: {}",
                    self.remaining().trim()
                ),
            ))
        }
    }

    fn take_delimiter(&mut self, delimiter: char) -> bool {
        if self.remaining().starts_with(delimiter) {
            self.pos += delimiter.len_utf8();
            true
        } else {
            false
        }
    }
}

pub(crate) fn parse_venn(code: &str, meta: &ParseMetadata) -> Result<Value> {
    let source = parse_venn_semantic_source(code, meta)?;
    render_model_to_compat_json(&source.model, meta)
}

pub(crate) fn parse_venn_json_and_editor_facts(
    code: &str,
    meta: &ParseMetadata,
    control: &crate::OperationControl,
) -> crate::OperationControlResult<crate::family::CombinedSemanticParse> {
    let VennParseOutcome {
        source,
        first_error,
    } = construct_venn_parse_outcome_controlled(code, meta, control)?;
    let construction = match first_error {
        Some(error) => Err(crate::family::CombinedSemanticFailure::new(
            error,
            source.editor_facts,
        )),
        None => Ok(source),
    };
    let parsed = crate::family::CombinedSemanticParse::from_construction(
        construction,
        |source| {
            (
                render_model_to_compat_json(&source.model, meta),
                source.editor_facts,
            )
        },
        crate::family::CombinedSemanticFailure::into_parts,
    );
    control.checkpoint()?;
    Ok(parsed)
}

#[cfg(test)]
pub(crate) fn parse_venn_model_for_render(
    code: &str,
    meta: &ParseMetadata,
) -> Result<VennDiagramRenderModel> {
    Ok(parse_venn_semantic_source(code, meta)?.model)
}

pub(crate) fn parse_venn_model_for_render_controlled(
    code: &str,
    meta: &ParseMetadata,
    control: &crate::OperationControl,
) -> crate::OperationControlResult<Result<VennDiagramRenderModel>> {
    let outcome = construct_venn_parse_outcome_controlled(code, meta, control)?;
    match outcome.first_error {
        Some(error) => Ok(Err(error)),
        None => Ok(Ok(outcome.source.model)),
    }
}

pub(crate) fn render_model_to_compat_json(
    model: &VennDiagramRenderModel,
    meta: &ParseMetadata,
) -> Result<Value> {
    Ok(json!({
        "type": meta.diagram_type,
        "title": &model.title,
        "accTitle": &model.acc_title,
        "accDescr": &model.acc_descr,
        "subsets": &model.subsets,
        "textNodes": &model.text_nodes,
        "styleEntries": &model.style_entries,
    }))
}

fn parse_venn_semantic_source(code: &str, meta: &ParseMetadata) -> Result<VennSemanticSource> {
    construct_venn_parse_outcome(code, meta).into_strict_source()
}

fn construct_venn_parse_outcome(code: &str, meta: &ParseMetadata) -> VennParseOutcome {
    construct_venn_parse_outcome_controlled(code, meta, &crate::OperationControl::new())
        .expect("a private parse control cannot be cancelled")
}

fn construct_venn_parse_outcome_controlled(
    code: &str,
    meta: &ParseMetadata,
    control: &crate::OperationControl,
) -> crate::OperationControlResult<VennParseOutcome> {
    control.checkpoint()?;
    #[cfg(test)]
    crate::diagrams::langium_common::record_family_syntax_construction("venn");

    let mut state = VennSemanticState::new();
    let mut header_decided = false;
    let mut offset = 0usize;
    let mut first_error = None;

    for segment in code.split_inclusive('\n') {
        control.checkpoint()?;
        let line_start = offset;
        offset += segment.len();
        let line = segment.trim_end_matches(['\n', '\r']);
        let stripped = strip_inline_comment_aware(line);
        if stripped.trim().is_empty() {
            continue;
        }

        if !header_decided {
            header_decided = true;
            let indent = leading_indent_len(stripped);
            let statement = &stripped[indent..];
            let statement_start = line_start + indent;
            let Some(rest) = strip_keyword_ci(statement, "venn-beta") else {
                let span = SourceSpan::new(
                    statement_start,
                    statement_start + statement.trim_end().len(),
                );
                recover_venn_error(
                    meta,
                    &mut state,
                    &mut first_error,
                    parse_error(meta, "expected venn-beta header"),
                    "expected venn-beta header",
                    span,
                );
                continue;
            };

            let header_span = SourceSpan::new(statement_start, statement_start + "venn-beta".len());
            state
                .editor_facts
                .push_expected_syntax(EditorExpectedSyntax::new(
                    EditorExpectedSyntaxKind::Payload,
                    header_span,
                ));
            state
                .editor_facts
                .push_symbol(EditorSemanticSymbol::payload(
                    "venn-beta".to_string(),
                    Some("venn header".to_string()),
                    EditorSemanticKind::String,
                    header_span,
                    header_span,
                ));

            if !rest.trim().is_empty()
                && let Err(error) = parse_venn_statement_facts(
                    rest,
                    statement_start + "venn-beta".len(),
                    &mut state,
                    meta,
                )
            {
                let message = venn_error_message(&error);
                recover_venn_error(
                    meta,
                    &mut state,
                    &mut first_error,
                    error,
                    format!("venn parser recovered after parse error: {message}"),
                    SourceSpan::new(
                        statement_start,
                        statement_start + statement.trim_end().len(),
                    ),
                );
            }
            continue;
        }

        if let Err(error) = parse_venn_statement_facts(stripped, line_start, &mut state, meta) {
            let indent = leading_indent_len(stripped);
            let message = venn_error_message(&error);
            recover_venn_error(
                meta,
                &mut state,
                &mut first_error,
                error,
                format!("venn parser recovered after parse error: {message}"),
                SourceSpan::new(line_start + indent, line_start + stripped.trim_end().len()),
            );
        }
    }

    if !header_decided {
        recover_venn_error(
            meta,
            &mut state,
            &mut first_error,
            parse_error(meta, "expected venn-beta"),
            "expected venn-beta",
            SourceSpan::new(0, 0),
        );
    }

    control.checkpoint()?;
    Ok(VennParseOutcome {
        source: VennSemanticSource {
            model: state.model,
            editor_facts: state.editor_facts,
        },
        first_error,
    })
}

fn recover_venn_error(
    meta: &ParseMetadata,
    state: &mut VennSemanticState,
    first_error: &mut Option<Error>,
    error: Error,
    recovery_message: impl Into<String>,
    span: SourceSpan,
) {
    if first_error.is_none() {
        *first_error = Some(Error::diagram_parse_exact(
            meta.diagram_type.clone(),
            venn_error_message(&error),
            span,
        ));
    }
    state
        .editor_facts
        .mark_recovered_from_parse_error(recovery_message, Some(span));
    state.current_sets = None;
    state.indent_mode = false;
}

fn venn_error_message(error: &Error) -> String {
    match error {
        Error::DiagramParse { diagnostic, .. } => diagnostic.message().to_string(),
        _ => error.to_string(),
    }
}

fn parse_venn_statement_facts(
    line: &str,
    line_start: usize,
    state: &mut VennSemanticState,
    meta: &ParseMetadata,
) -> Result<()> {
    let indent = leading_indent_len(line);
    let statement = &line[indent..];
    if statement.trim().is_empty() {
        return Ok(());
    }

    let statement_start = line_start + indent;
    if indent > 0 && state.indent_mode && starts_with_keyword_ci(statement, "text") {
        state.editor_facts.push_directive_prefix("text");
        let rest = strip_keyword_ci(statement, "text")
            .expect("starts_with_keyword_ci and strip_keyword_ci agree");
        return parse_venn_text_statement_facts(
            rest,
            statement_start + "text".len(),
            state,
            meta,
            true,
        );
    }

    if indent == 0 {
        state.indent_mode = false;
    }

    if let Some(rest) = strip_keyword_ci(statement, "title") {
        let Some(separator) = rest.chars().next() else {
            return Err(parse_error(meta, "expected title text"));
        };
        if !separator.is_whitespace()
            || rest[separator.len_utf8()..].is_empty()
            || rest.contains(['#', ';'])
        {
            return Err(parse_error(meta, "invalid Venn title syntax"));
        }
        state.editor_facts.push_directive_prefix("title");
        let mut cursor = VennCursor::new(rest, statement_start + "title".len());
        let Some(payload) = cursor.take_remaining_payload() else {
            return Err(parse_error(meta, "expected title text"));
        };
        push_venn_payload_fact(
            &mut state.editor_facts,
            &payload,
            "venn title",
            EditorSemanticKind::String,
        );
        state.model.title = Some(rest.trim().to_string());
        return Ok(());
    }

    if let Some(rest) = strip_keyword_ci(statement, "set") {
        state.editor_facts.push_directive_prefix("set");
        return parse_venn_set_statement_facts(rest, statement_start + "set".len(), state, meta);
    }

    if let Some(rest) = strip_keyword_ci(statement, "union") {
        state.editor_facts.push_directive_prefix("union");
        return parse_venn_union_statement_facts(
            rest,
            statement_start + "union".len(),
            state,
            meta,
        );
    }

    if let Some(rest) = strip_keyword_ci(statement, "text") {
        state.editor_facts.push_directive_prefix("text");
        return parse_venn_text_statement_facts(
            rest,
            statement_start + "text".len(),
            state,
            meta,
            false,
        );
    }

    if let Some(rest) = strip_keyword_ci(statement, "style") {
        state.editor_facts.push_directive_prefix("style");
        return parse_venn_style_statement_facts(
            rest,
            statement_start + "style".len(),
            state,
            meta,
        );
    }

    Err(parse_error(
        meta,
        format!("unexpected venn statement: {}", statement.trim()),
    ))
}

fn parse_venn_set_statement_facts(
    input: &str,
    line_start: usize,
    state: &mut VennSemanticState,
    meta: &ParseMetadata,
) -> Result<()> {
    let mut cursor = VennCursor::new(input, line_start);
    let parsed = (|| {
        let identifier = cursor.take_identifier_like(meta)?;
        cursor.skip_ws();
        if cursor.take_delimiter(',') {
            return Err(parse_error(meta, "set requires single identifier"));
        }

        let label = cursor.take_optional_bracket_label(meta)?;
        let size = cursor.take_optional_size(meta)?;
        cursor.expect_end(meta)?;
        let size_value = size
            .as_ref()
            .map(|field| field.text.parse::<f64>())
            .transpose()
            .map_err(|_| parse_error(meta, "expected numeric"))?;
        Ok((identifier, label, size, size_value))
    })();
    let (identifier, label, size, size_value) = parsed?;

    state
        .editor_facts
        .push_expected_syntax(EditorExpectedSyntax::new(
            EditorExpectedSyntaxKind::NodeIdentifier,
            identifier.span,
        ));
    push_venn_entity_fact(
        &mut state.editor_facts,
        &identifier,
        "venn set",
        EditorSemanticKind::Namespace,
    );
    if let Some(label) = label.as_ref() {
        push_venn_payload_fact(
            &mut state.editor_facts,
            label,
            "venn set label",
            EditorSemanticKind::String,
        );
    }
    if let Some(size) = size.as_ref() {
        push_venn_payload_fact(
            &mut state.editor_facts,
            size,
            "venn size",
            EditorSemanticKind::String,
        );
    }
    state.add_subset(
        vec![identifier.text],
        label.map(|field| field.text),
        size_value,
    );
    state.indent_mode = true;
    Ok(())
}

fn parse_venn_union_statement_facts(
    input: &str,
    line_start: usize,
    state: &mut VennSemanticState,
    meta: &ParseMetadata,
) -> Result<()> {
    let mut cursor = VennCursor::new(input, line_start);
    let parsed = (|| {
        let identifiers = parse_venn_identifier_list(&mut cursor, meta)?;
        if identifiers.len() < 2 {
            return Err(parse_error(meta, "union requires multiple identifiers"));
        }
        let label = cursor.take_optional_bracket_label(meta)?;
        let size = cursor.take_optional_size(meta)?;
        cursor.expect_end(meta)?;
        let size_value = size
            .as_ref()
            .map(|field| field.text.parse::<f64>())
            .transpose()
            .map_err(|_| parse_error(meta, "expected numeric"))?;
        Ok((identifiers, label, size, size_value))
    })();
    let (identifiers, label, size, size_value) = parsed?;

    let identifier_values = identifiers
        .iter()
        .map(|identifier| identifier.text.clone())
        .collect::<Vec<_>>();
    state.validate_union_identifiers(&identifier_values, meta)?;

    let list_span = venn_list_span(&identifiers);
    state
        .editor_facts
        .push_expected_syntax(EditorExpectedSyntax::new(
            EditorExpectedSyntaxKind::IdList,
            list_span,
        ));
    for identifier in &identifiers {
        push_venn_reference_fact(
            &mut state.editor_facts,
            identifier,
            "venn union set",
            EditorSemanticKind::Namespace,
        );
    }

    if let Some(label) = label.as_ref() {
        push_venn_payload_fact(
            &mut state.editor_facts,
            label,
            "venn union label",
            EditorSemanticKind::String,
        );
    }
    if let Some(size) = size.as_ref() {
        push_venn_payload_fact(
            &mut state.editor_facts,
            size,
            "venn size",
            EditorSemanticKind::String,
        );
    }

    state.add_subset(identifier_values, label.map(|field| field.text), size_value);
    state.indent_mode = true;
    Ok(())
}

fn parse_venn_text_statement_facts(
    input: &str,
    line_start: usize,
    state: &mut VennSemanticState,
    meta: &ParseMetadata,
    indented: bool,
) -> Result<()> {
    let mut cursor = VennCursor::new(input, line_start);
    let parsed = (|| {
        let (explicit_sets, set_fields) = if indented {
            (
                state
                    .current_sets
                    .clone()
                    .ok_or_else(|| parse_error(meta, "text requires set"))?,
                None,
            )
        } else {
            let sets = parse_venn_identifier_list(&mut cursor, meta)?;
            let values = sets.iter().map(|set| set.text.clone()).collect::<Vec<_>>();
            (values, Some(sets))
        };

        let (identifier, kind) = cursor.take_text_id(meta)?;
        let label = cursor.take_optional_bracket_label(meta)?;
        if kind == TextIdKind::Numeric && label.is_some() {
            return Err(parse_error(meta, "unexpected label after numeric text id"));
        }
        cursor.expect_end(meta)?;
        if explicit_sets.is_empty() {
            return Err(parse_error(meta, "text requires set"));
        }
        Ok((explicit_sets, set_fields, identifier, label))
    })();
    let (explicit_sets, set_fields, identifier, label) = parsed?;

    if let Some(sets) = set_fields {
        let list_span = venn_list_span(&sets);
        state
            .editor_facts
            .push_expected_syntax(EditorExpectedSyntax::new(
                EditorExpectedSyntaxKind::IdList,
                list_span,
            ));
        for set in &sets {
            push_venn_reference_fact(
                &mut state.editor_facts,
                set,
                "venn text set",
                EditorSemanticKind::Namespace,
            );
        }
    }

    state
        .editor_facts
        .push_expected_syntax(EditorExpectedSyntax::new(
            EditorExpectedSyntaxKind::NodeIdentifier,
            identifier.span,
        ));
    push_venn_entity_fact(
        &mut state.editor_facts,
        &identifier,
        "venn text node",
        EditorSemanticKind::Namespace,
    );
    if let Some(label) = label.as_ref() {
        push_venn_payload_fact(
            &mut state.editor_facts,
            label,
            "venn text label",
            EditorSemanticKind::String,
        );
    }

    state.add_text(
        explicit_sets,
        identifier.text,
        label.map(|field| field.text),
    );
    Ok(())
}

fn parse_venn_style_statement_facts(
    input: &str,
    line_start: usize,
    state: &mut VennSemanticState,
    meta: &ParseMetadata,
) -> Result<()> {
    let mut cursor = VennCursor::new(input, line_start);
    let parsed = (|| {
        let targets = parse_venn_identifier_list(&mut cursor, meta)?;
        let mut styles = Vec::new();
        loop {
            cursor.skip_ws();
            let value = cursor.remaining();
            if value.trim().is_empty() {
                break;
            }

            let key = cursor.take_bare_identifier(meta)?;
            cursor.skip_ws();
            if !cursor.take_delimiter(':') {
                return Err(parse_error(meta, "expected ':' after style field"));
            }
            let value = cursor.take_style_value(meta)?;
            styles.push((key, value));

            cursor.skip_ws();
            if cursor.take_delimiter(',') {
                continue;
            }
            break;
        }

        cursor.expect_end(meta)?;
        if styles.is_empty() {
            return Err(parse_error(meta, "expected style field"));
        }
        Ok((targets, styles))
    })();
    let (targets, styles) = parsed?;

    let list_span = venn_list_span(&targets);
    state
        .editor_facts
        .push_expected_syntax(EditorExpectedSyntax::new(
            EditorExpectedSyntaxKind::IdList,
            list_span,
        ));
    for target in &targets {
        push_venn_reference_fact(
            &mut state.editor_facts,
            target,
            "venn style target",
            EditorSemanticKind::Namespace,
        );
    }
    for (_, value) in &styles {
        push_venn_payload_fact(
            &mut state.editor_facts,
            value,
            "venn style value",
            EditorSemanticKind::String,
        );
    }

    state.add_style(
        targets.into_iter().map(|target| target.text).collect(),
        styles
            .into_iter()
            .map(|(key, value)| (key.text, value.text))
            .collect(),
    );
    Ok(())
}

fn parse_venn_identifier_list(
    cursor: &mut VennCursor<'_>,
    meta: &ParseMetadata,
) -> Result<Vec<VennFieldSpan>> {
    let first = cursor.take_identifier_like(meta)?;
    let mut identifiers = vec![first];

    loop {
        cursor.skip_ws();
        if !cursor.take_delimiter(',') {
            break;
        }
        identifiers.push(cursor.take_identifier_like(meta)?);
    }

    Ok(identifiers)
}

fn venn_list_span(items: &[VennFieldSpan]) -> SourceSpan {
    let start = items.first().map(|item| item.span.start).unwrap_or(0);
    let end = items.last().map(|item| item.span.end).unwrap_or(start);
    SourceSpan::new(start, end)
}

fn push_venn_entity_fact(
    facts: &mut EditorSemanticFacts,
    field: &VennFieldSpan,
    detail: &'static str,
    kind: EditorSemanticKind,
) {
    if field.text.is_empty() {
        return;
    }
    facts.push_symbol(EditorSemanticSymbol::new(
        field.text.clone(),
        Some(detail.to_string()),
        kind,
        field.span,
        field.selection,
    ));
}

fn push_venn_reference_fact(
    facts: &mut EditorSemanticFacts,
    field: &VennFieldSpan,
    detail: &'static str,
    kind: EditorSemanticKind,
) {
    if field.text.is_empty() {
        return;
    }
    facts.push_symbol(EditorSemanticSymbol::reference(
        field.text.clone(),
        Some(detail.to_string()),
        kind,
        field.span,
        field.selection,
    ));
}

fn push_venn_payload_fact(
    facts: &mut EditorSemanticFacts,
    field: &VennFieldSpan,
    detail: &'static str,
    kind: EditorSemanticKind,
) {
    if field.text.is_empty() {
        return;
    }
    facts.push_expected_syntax(EditorExpectedSyntax::new(
        EditorExpectedSyntaxKind::Payload,
        field.span,
    ));
    facts.push_symbol(EditorSemanticSymbol::payload(
        field.text.clone(),
        Some(detail.to_string()),
        kind,
        field.span,
        field.selection,
    ));
}

fn parse_string_token(input: &str) -> Option<(String, &str)> {
    let rest = input.strip_prefix('"')?;
    let end = rest.find('"')?;
    let value = &input[..end + 2];
    Some((value.to_string(), &rest[end + 1..]))
}

fn parse_bare_identifier_token(input: &str) -> Option<(&str, &str)> {
    let bytes = input.as_bytes();
    let first = *bytes.first()?;
    if !(first.is_ascii_alphabetic() || first == b'_') {
        return None;
    }

    let mut end = 1usize;
    while end < bytes.len() {
        let byte = bytes[end];
        if byte.is_ascii_alphanumeric() || byte == b'_' || byte == b'-' {
            end += 1;
        } else {
            break;
        }
    }
    Some((&input[..end], &input[end..]))
}

fn parse_numeric_token(input: &str) -> Option<(String, &str)> {
    let bytes = input.as_bytes();
    let mut end = usize::from(
        bytes
            .first()
            .is_some_and(|byte| matches!(byte, b'+' | b'-')),
    );
    let integer_start = end;
    while bytes.get(end).is_some_and(u8::is_ascii_digit) {
        end += 1;
    }
    let has_integer = end > integer_start;

    let has_fraction =
        bytes.get(end) == Some(&b'.') && bytes.get(end + 1).is_some_and(u8::is_ascii_digit);
    if has_fraction {
        end += 1;
        while bytes.get(end).is_some_and(u8::is_ascii_digit) {
            end += 1;
        }
    }

    if !has_integer && !has_fraction {
        return None;
    }

    Some((input[..end].to_string(), &input[end..]))
}

fn take_style_value_segment(input: &str) -> (&str, &str) {
    let mut in_quote = false;
    let mut paren_depth = 0usize;

    for (idx, ch) in input.char_indices() {
        match ch {
            '"' => in_quote = !in_quote,
            '(' if !in_quote => paren_depth += 1,
            ')' if !in_quote => paren_depth = paren_depth.saturating_sub(1),
            ',' if !in_quote && paren_depth == 0 => return (&input[..idx], &input[idx..]),
            _ => {}
        }
    }

    (input, "")
}

fn style_value_tokens(input: &str, meta: &ParseMetadata) -> Result<Vec<String>> {
    let mut rest = input.trim();
    let mut tokens = Vec::new();
    while !rest.is_empty() {
        if let Some((token, after)) = parse_rgb_like_token(rest) {
            tokens.push(token.to_string());
            rest = skip_ws(after);
            continue;
        }
        if let Some((token, after)) = parse_hex_color_token(rest) {
            tokens.push(token.to_string());
            rest = skip_ws(after);
            continue;
        }
        if let Some((token, after)) = parse_numeric_token(rest) {
            tokens.push(token);
            rest = skip_ws(after);
            continue;
        }
        if let Some((identifier, after)) = parse_bare_identifier_token(rest) {
            tokens.push(identifier.to_string());
            rest = skip_ws(after);
            continue;
        }

        return Err(parse_error(meta, "expected style value"));
    }
    Ok(tokens)
}

fn parse_rgb_like_token(input: &str) -> Option<(&str, &str)> {
    let lower = input.to_ascii_lowercase();
    let (prefix_len, component_count) = if lower.starts_with("rgba(") {
        ("rgba(".len(), 4)
    } else if lower.starts_with("rgb(") {
        ("rgb(".len(), 3)
    } else {
        return None;
    };
    let end = input.find(')')?;
    let components = input[prefix_len..end].split(',').collect::<Vec<_>>();
    if components.len() != component_count
        || components.iter().any(|component| {
            let component = component.trim();
            component.is_empty()
                || !component
                    .bytes()
                    .all(|byte| byte.is_ascii_digit() || byte == b'.')
        })
    {
        return None;
    }
    Some((&input[..end + 1], &input[end + 1..]))
}

fn parse_hex_color_token(input: &str) -> Option<(&str, &str)> {
    let rest = input.strip_prefix('#')?;
    let len = rest
        .bytes()
        .take_while(|byte| byte.is_ascii_hexdigit())
        .count();
    if (3..=8).contains(&len) {
        Some((&input[..len + 1], &input[len + 1..]))
    } else {
        None
    }
}

fn normalize_identifier_list(identifiers: Vec<String>) -> Vec<String> {
    let mut out = identifiers
        .into_iter()
        .map(|identifier| normalize_text(&identifier))
        .collect::<Vec<_>>();
    out.sort();
    out
}

fn normalize_text(text: &str) -> String {
    let trimmed = text.trim();
    if trimmed.len() >= 2 && trimmed.starts_with('"') && trimmed.ends_with('"') {
        trimmed[1..trimmed.len() - 1].to_string()
    } else {
        trimmed.to_string()
    }
}

fn leading_indent_len(line: &str) -> usize {
    line.chars()
        .take_while(|ch| matches!(ch, ' ' | '\t'))
        .map(char::len_utf8)
        .sum()
}

fn skip_ws(input: &str) -> &str {
    input.trim_start_matches([' ', '\t'])
}

fn strip_keyword_ci<'a>(input: &'a str, keyword: &str) -> Option<&'a str> {
    let candidate = input.get(..keyword.len())?;
    if !candidate.eq_ignore_ascii_case(keyword) {
        return None;
    }
    let rest = &input[keyword.len()..];
    if rest
        .chars()
        .next()
        .is_some_and(|ch| ch.is_ascii_alphanumeric() || ch == '_' || ch == '-')
    {
        return None;
    }
    Some(rest)
}

fn starts_with_keyword_ci(input: &str, keyword: &str) -> bool {
    let Some(candidate) = input.get(..keyword.len()) else {
        return false;
    };
    if !candidate.eq_ignore_ascii_case(keyword) {
        return false;
    }
    input[keyword.len()..]
        .chars()
        .next()
        .is_none_or(|ch| ch.is_whitespace())
}

fn strip_inline_comment_aware(line: &str) -> &str {
    let mut in_quote = false;
    let mut bracket_depth = 0usize;
    let mut iter = line.char_indices().peekable();

    while let Some((idx, ch)) = iter.next() {
        match ch {
            '"' => in_quote = !in_quote,
            '[' if !in_quote => bracket_depth += 1,
            ']' if !in_quote => bracket_depth = bracket_depth.saturating_sub(1),
            '%' if !in_quote
                && bracket_depth == 0
                && iter.peek().is_some_and(|(_, next)| *next == '%') =>
            {
                return &line[..idx];
            }
            _ => {}
        }
    }

    line
}

fn parse_error(meta: &ParseMetadata, message: impl Into<String>) -> Error {
    Error::diagram_parse_fallback(meta.diagram_type.clone(), message.into())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        EditorSemanticCompleteness, EditorSemanticRole, Engine, MermaidConfig, ParseMetadata,
        ParseOptions, RenderSemanticModel,
    };

    fn meta() -> ParseMetadata {
        ParseMetadata {
            diagram_type: "venn".to_string(),
            config: MermaidConfig::empty_object(),
            effective_config: MermaidConfig::empty_object(),
            title: None,
        }
    }

    fn parse(input: &str) -> VennDiagramRenderModel {
        parse_venn_model_for_render(input, &meta()).unwrap()
    }

    #[test]
    fn parses_simple_sets_title_and_default_union_size() {
        let model = parse(
            r#"venn-beta
          title foo bar
          set A
          set B
          union A,B
      "#,
        );

        assert_eq!(model.title.as_deref(), Some("foo bar"));
        assert_eq!(
            model.subsets,
            vec![
                VennSubsetRenderModel {
                    sets: vec!["A".to_string()],
                    size: 10.0,
                    label: None,
                },
                VennSubsetRenderModel {
                    sets: vec!["B".to_string()],
                    size: 10.0,
                    label: None,
                },
                VennSubsetRenderModel {
                    sets: vec!["A".to_string(), "B".to_string()],
                    size: 2.5,
                    label: None,
                },
            ]
        );
    }

    #[test]
    fn title_matches_the_upstream_lexer_boundary() {
        assert_eq!(
            parse("venn-beta\ntitle Valid title\n").title.as_deref(),
            Some("Valid title")
        );

        for source in [
            "venn-beta\ntitle: Invalid\n",
            "venn-beta\ntitle Invalid; suffix\n",
            "venn-beta\ntitle Invalid # suffix\n",
        ] {
            let error = parse_venn_model_for_render(source, &meta())
                .expect_err("the pinned Venn lexer rejects this title form");
            assert!(error.to_string().contains("title"), "{error}");
        }
    }

    #[test]
    fn parses_bracket_labels_and_size_suffixes() {
        let model = parse(
            r#"venn-beta
          title foo bar
          set A["Alpha"]:20
          set B[Beta]:12
          set C["Gamma"]:30
          union A,B["AB"]:5.3
          union C,A,B:1
      "#,
        );

        assert_eq!(model.subsets[0].label.as_deref(), Some("Alpha"));
        assert_eq!(model.subsets[0].size, 20.0);
        assert_eq!(model.subsets[1].label.as_deref(), Some("Beta"));
        assert_eq!(model.subsets[1].size, 12.0);
        assert_eq!(model.subsets[3].sets, ["A", "B"]);
        assert_eq!(model.subsets[3].label.as_deref(), Some("AB"));
        assert_eq!(model.subsets[3].size, 5.3);
        assert_eq!(model.subsets[4].sets, ["A", "B", "C"]);
    }

    #[test]
    fn parses_text_nodes_with_explicit_and_indented_forms() {
        let model = parse(
            r#"venn-beta
          set A["Frontend"]
            text A1["React"]
            text A2
          set B["Backend"]
            text B1
          union A,B["APIs"]
            text AB1["OpenAPI"]
      "#,
        );

        assert_eq!(
            model.text_nodes,
            vec![
                VennTextNodeRenderModel {
                    sets: vec!["A".to_string()],
                    id: "A1".to_string(),
                    label: Some("React".to_string()),
                },
                VennTextNodeRenderModel {
                    sets: vec!["A".to_string()],
                    id: "A2".to_string(),
                    label: None,
                },
                VennTextNodeRenderModel {
                    sets: vec!["B".to_string()],
                    id: "B1".to_string(),
                    label: None,
                },
                VennTextNodeRenderModel {
                    sets: vec!["A".to_string(), "B".to_string()],
                    id: "AB1".to_string(),
                    label: Some("OpenAPI".to_string()),
                },
            ]
        );
    }

    #[test]
    fn parses_explicit_text_statement_and_numeric_text_id() {
        let model = parse(
            r#"venn-beta
set A
set B
union A,B
text A alpha["Alpha note"]
text A,B 42
      "#,
        );

        assert_eq!(
            model.text_nodes,
            vec![
                VennTextNodeRenderModel {
                    sets: vec!["A".to_string()],
                    id: "alpha".to_string(),
                    label: Some("Alpha note".to_string()),
                },
                VennTextNodeRenderModel {
                    sets: vec!["A".to_string(), "B".to_string()],
                    id: "42".to_string(),
                    label: None,
                },
            ]
        );
    }

    #[test]
    fn parses_style_entries() {
        let model = parse(
            r#"venn-beta
          set A
          set B
          union A,B
          style A fill:#ff6b6b, color:#333
          style A,B fill:rgb(255, 0, 128)
          style B fill:rgba(255, 0, 128, 0.5)
      "#,
        );

        assert_eq!(model.style_entries[0].targets, ["A"]);
        assert_eq!(
            model.style_entries[0]
                .styles
                .get("fill")
                .map(String::as_str),
            Some("#ff6b6b")
        );
        assert_eq!(
            model.style_entries[0]
                .styles
                .get("color")
                .map(String::as_str),
            Some("#333")
        );
        assert_eq!(model.style_entries[1].targets, ["A", "B"]);
        assert_eq!(
            model.style_entries[1]
                .styles
                .get("fill")
                .map(String::as_str),
            Some("rgb(255, 0, 128)")
        );
        assert_eq!(
            model.style_entries[2]
                .styles
                .get("fill")
                .map(String::as_str),
            Some("rgba(255, 0, 128, 0.5)")
        );
    }

    #[test]
    fn rejects_invalid_set_and_union_shapes() {
        let err = parse_venn_model_for_render("venn-beta\nset A,B\n", &meta()).unwrap_err();
        assert!(err.to_string().contains("set requires single identifier"));

        let err = parse_venn_model_for_render("venn-beta\nunion A\n", &meta()).unwrap_err();
        assert!(
            err.to_string()
                .contains("union requires multiple identifiers")
        );

        let err = parse_venn_model_for_render("venn-beta\nset Foo\nunion Foo,Buz\n", &meta())
            .unwrap_err();
        assert!(err.to_string().contains("unknown set identifier"));
    }

    #[test]
    fn parses_quoted_identifiers_and_sorts_union_sets() {
        let model = parse(
            r#"venn-beta
        set "Foo Bar"
        set Buz
        union "Foo Bar",Buz
    "#,
        );

        assert_eq!(model.subsets[0].sets, ["Foo Bar"]);
        assert_eq!(model.subsets[2].sets, ["Buz", "Foo Bar"]);
    }

    #[test]
    fn parse_venn_editor_facts_expose_parser_backed_spans() {
        let engine = Engine::new();
        let text = r##"venn-beta
title Product Surface
set A["Core"]:20
set B["Editor"]:14
union A,B["Shared"]:4
  text A1["Nested note"]
  text A1["Nested note"]
text A alpha["Alpha note"]
style A fill:#ff6b6b, color:#101010
style A,B fill:#00ffcc, color:#003333
"##;
        let facts = engine
            .parse_editor_semantic_facts_with_type_sync("venn", text)
            .unwrap()
            .unwrap();

        assert_eq!(facts.completeness, EditorSemanticCompleteness::Complete);
        assert!(facts.diagnostics.is_empty());
        for prefix in ["title", "set", "union", "text", "style"] {
            assert!(
                facts.directive_prefixes.iter().any(|value| value == prefix),
                "missing directive prefix {prefix}"
            );
        }

        let symbol_at = |name: &str, detail: &str, start: usize| {
            facts
                .symbols
                .iter()
                .find(|symbol| {
                    symbol.name == name
                        && symbol.detail.as_deref() == Some(detail)
                        && symbol.selection.start == start
                })
                .unwrap_or_else(|| panic!("missing symbol {name} with detail {detail} at {start}"))
        };

        let title_start = text.find("Product Surface").unwrap();
        let title = symbol_at("Product Surface", "venn title", title_start);
        assert_eq!(title.role, EditorSemanticRole::Payload);
        assert_eq!(title.kind, EditorSemanticKind::String);
        assert_eq!(title.selection.end, title_start + "Product Surface".len());

        let set_a_start = text.find("A[\"Core\"]").unwrap();
        let set_a = symbol_at("A", "venn set", set_a_start);
        assert_eq!(set_a.role, EditorSemanticRole::Entity);
        assert_eq!(set_a.kind, EditorSemanticKind::Namespace);
        assert_eq!(set_a.selection.end, set_a_start + "A".len());

        let set_label_start = text.find("Core").unwrap();
        let set_label = symbol_at("Core", "venn set label", set_label_start);
        assert_eq!(set_label.role, EditorSemanticRole::Payload);
        assert_eq!(set_label.kind, EditorSemanticKind::String);
        assert_eq!(set_label.selection.start, set_label_start);
        assert_eq!(set_label.selection.end, set_label_start + "Core".len());

        let union_a_start = text.find("union A,B").unwrap() + "union ".len();
        let union_a = symbol_at("A", "venn union set", union_a_start);
        assert_eq!(union_a.role, EditorSemanticRole::Reference);
        assert_eq!(union_a.kind, EditorSemanticKind::Namespace);

        let text_set_start = text.find("text A alpha").unwrap() + "text ".len();
        let text_set = symbol_at("A", "venn text set", text_set_start);
        assert_eq!(text_set.role, EditorSemanticRole::Reference);
        assert_eq!(text_set.kind, EditorSemanticKind::Namespace);

        let text_id_start = text.find("alpha").unwrap();
        let text_id = symbol_at("alpha", "venn text node", text_id_start);
        assert_eq!(text_id.role, EditorSemanticRole::Entity);
        assert_eq!(text_id.kind, EditorSemanticKind::Namespace);
        assert_eq!(text_id.selection.end, text_id_start + "alpha".len());

        let text_note_start = text.find("Alpha note").unwrap();
        let text_note = symbol_at("Alpha note", "venn text label", text_note_start);
        assert_eq!(text_note.role, EditorSemanticRole::Payload);

        let style_target_start = text.find("style A fill").unwrap() + "style ".len();
        let style_target = symbol_at("A", "venn style target", style_target_start);
        assert_eq!(style_target.role, EditorSemanticRole::Reference);
        assert_eq!(style_target.kind, EditorSemanticKind::Namespace);

        let fill_start = text.find("#ff6b6b").unwrap();
        let fill = symbol_at("#ff6b6b", "venn style value", fill_start);
        assert_eq!(fill.role, EditorSemanticRole::Payload);
        assert_eq!(fill.kind, EditorSemanticKind::String);
        assert_eq!(fill.selection.start, fill_start);

        assert!(facts.expected_syntax.iter().any(|expected| {
            expected.kind == EditorExpectedSyntaxKind::NodeIdentifier
                && expected.span == SourceSpan::new(set_a_start, set_a_start + "A".len())
        }));
        assert!(facts.expected_syntax.iter().any(|expected| {
            expected.kind == EditorExpectedSyntaxKind::IdList
                && expected.span.start <= union_a_start
                && expected.span.end >= union_a_start + "A".len()
        }));
        assert!(facts.expected_syntax.iter().any(|expected| {
            expected.kind == EditorExpectedSyntaxKind::Payload
                && expected.span.start <= title_start
                && expected.span.end >= title_start + "Product Surface".len()
        }));
    }

    #[test]
    fn venn_reference_roles_do_not_promote_occurrences_to_set_definitions() {
        let text = concat!(
            "venn-beta\n",
            "style Future fill:blue\n",
            "set Future\n",
            "set A\n",
            "set B\n",
            "union A,B\n",
            "text A note[\"Note\"]\n",
            "style A fill:red\n",
        );
        let facts = crate::family::test_support::editor_facts(
            parse_venn_json_and_editor_facts,
            text,
            &meta(),
        );

        let a_roles: Vec<_> = facts
            .symbols
            .iter()
            .filter(|symbol| symbol.name == "A")
            .map(|symbol| symbol.role)
            .collect();
        assert_eq!(
            a_roles,
            [
                EditorSemanticRole::Entity,
                EditorSemanticRole::Reference,
                EditorSemanticRole::Reference,
                EditorSemanticRole::Reference,
            ]
        );
        assert!(
            facts.symbols.iter().any(|symbol| {
                symbol.name == "note" && symbol.role == EditorSemanticRole::Entity
            })
        );
        let future_roles: Vec<_> = facts
            .symbols
            .iter()
            .filter(|symbol| symbol.name == "Future")
            .map(|symbol| symbol.role)
            .collect();
        assert_eq!(
            future_roles,
            [EditorSemanticRole::Reference, EditorSemanticRole::Entity]
        );
    }

    #[test]
    fn venn_combined_parse_constructs_once_and_preserves_projections() {
        let text = r##"venn-beta
title "Product overlap"
set "Frontend Team"["Frontend"]:.5
set Backend:12
union "Frontend Team",Backend["Shared"]:3
  text shared_note["API contracts"]
style "Frontend Team",Backend fill:rgba(255, 0, 128, 0.5), color:#101010
"##;
        let meta = meta();

        crate::diagrams::langium_common::reset_family_syntax_construction_count("venn");
        let (combined_json, combined_editor) = crate::family::test_support::into_result(
            parse_venn_json_and_editor_facts(text, &meta, &crate::OperationControl::new()),
        )
        .unwrap();
        assert_eq!(
            crate::diagrams::langium_common::family_syntax_construction_count("venn"),
            1,
            "one combined request must construct Venn syntax once"
        );

        assert_eq!(combined_json, parse_venn(text, &meta).unwrap());
        assert!(!combined_editor.symbols.is_empty());
    }

    #[test]
    fn venn_typed_and_json_projections_share_the_same_semantics() {
        let text = r#"venn-beta
title "Product overlap"
set A["Frontend"]:20
set B["Backend"]:12
union A,B["Shared"]:3
  text note["API contracts"]
style A,B fill:#ff6b6b, color:red
"#;

        let compat = parse_venn(text, &meta()).unwrap();
        let typed = parse_venn_model_for_render(text, &meta()).unwrap();

        assert_eq!(
            render_model_to_compat_json(&typed, &meta()).unwrap(),
            compat
        );
        assert_eq!(typed.title.as_deref(), Some("\"Product overlap\""));
        assert_eq!(compat["title"], json!(typed.title));
        assert_eq!(compat["accTitle"], json!(typed.acc_title));
        assert_eq!(compat["accDescr"], json!(typed.acc_descr));
        assert_eq!(compat["subsets"], json!(typed.subsets));
        assert_eq!(compat["textNodes"], json!(typed.text_nodes));
        assert_eq!(compat["styleEntries"], json!(typed.style_entries));
    }

    #[test]
    fn venn_editor_projection_preserves_quoted_and_numeric_token_spans() {
        let text = "venn-beta\nset \"Frontend Team\"[\"Core\"]:.5\ntext \"Frontend Team\" 42\n";
        let facts = crate::family::test_support::editor_facts(
            parse_venn_json_and_editor_facts,
            text,
            &meta(),
        );

        let set_raw_start = text.find("\"Frontend Team\"").unwrap();
        let set = facts
            .symbols
            .iter()
            .find(|symbol| {
                symbol.name == "Frontend Team" && symbol.detail.as_deref() == Some("venn set")
            })
            .expect("quoted set fact");
        assert_eq!(
            set.span,
            SourceSpan::new(set_raw_start, set_raw_start + "\"Frontend Team\"".len())
        );
        assert_eq!(
            set.selection,
            SourceSpan::new(set_raw_start + 1, set_raw_start + "\"Frontend Team".len())
        );

        for (name, detail) in [(".5", "venn size"), ("42", "venn text node")] {
            let start = text.find(name).unwrap();
            let symbol = facts
                .symbols
                .iter()
                .find(|symbol| symbol.name == name && symbol.detail.as_deref() == Some(detail))
                .unwrap_or_else(|| panic!("missing {detail} fact"));
            assert_eq!(symbol.span, SourceSpan::new(start, start + name.len()));
            assert_eq!(symbol.selection, symbol.span);
        }
    }

    #[test]
    fn venn_malformed_statement_recovers_prior_facts_with_the_strict_error_span() {
        let text = "venn-beta\nset A[Alpha]\nunion A,\n";
        let statement_start = text.find("union A,").unwrap();
        let statement_span = SourceSpan::new(statement_start, statement_start + "union A,".len());

        let error = parse_venn(text, &meta()).expect_err("strict parse must reject the union");
        let Error::DiagramParse { diagnostic, .. } = error else {
            panic!("expected structured Venn parse error");
        };
        assert!(diagnostic.message().contains("expected identifier"));
        assert_eq!(diagnostic.span(), Some(statement_span));
        assert_eq!(
            diagnostic.span_kind(),
            crate::ParseDiagnosticSpanKind::Exact
        );

        let facts = crate::family::test_support::editor_facts(
            parse_venn_json_and_editor_facts,
            text,
            &meta(),
        );
        assert_eq!(facts.completeness, EditorSemanticCompleteness::Recovered);
        assert!(
            facts.symbols.iter().any(|symbol| {
                symbol.name == "A" && symbol.detail.as_deref() == Some("venn set")
            })
        );
        assert!(facts.diagnostics.iter().any(|diagnostic| {
            diagnostic.message.contains("expected identifier")
                && diagnostic.span == Some(statement_span)
        }));
    }

    #[test]
    fn venn_recovery_preserves_later_semantic_facts_with_crlf_spans() {
        let text = concat!(
            "venn-beta\r\n",
            "  union A, %% missing identifier\r\n",
            "set Later[\"ok\"]:12\r\n",
            "title Done\r\n",
        );
        let invalid = "union A,";
        let invalid_start = text.find(invalid).unwrap();
        let facts = crate::family::test_support::editor_facts(
            parse_venn_json_and_editor_facts,
            text,
            &meta(),
        );

        assert!(facts.symbols.iter().any(|symbol| {
            symbol.name == "Later" && symbol.detail.as_deref() == Some("venn set")
        }));
        assert!(facts.diagnostics.iter().any(|diagnostic| {
            diagnostic.kind == crate::EditorSemanticDiagnosticKind::ParserRecovery
                && diagnostic.span
                    == Some(SourceSpan::new(
                        invalid_start,
                        invalid_start + invalid.len(),
                    ))
        }));
    }

    #[test]
    fn venn_numeric_and_color_edges_follow_the_upstream_jison_grammar() {
        let model = parse("venn-beta\nset A:.5\nset B:+2\nunion A,B:-.25\n");
        assert_eq!(model.subsets[0].size, 0.5);
        assert_eq!(model.subsets[1].size, 2.0);
        assert_eq!(model.subsets[2].size, -0.25);

        let invalid_number = parse_venn("venn-beta\nset A:1.\n", &meta()).unwrap_err();
        assert!(
            invalid_number
                .to_string()
                .contains("unexpected trailing venn tokens")
        );

        let invalid_rgb =
            parse_venn("venn-beta\nset A\nstyle A fill:rgb(red, 0, 0)\n", &meta()).unwrap_err();
        assert!(invalid_rgb.to_string().contains("expected style value"));
    }

    #[test]
    fn render_model_entrypoint_returns_typed_venn_model() {
        let parsed = Engine::new()
            .parse_diagram_for_render_model_sync(
                "venn-beta\nset A\nset B\nunion A,B\n",
                ParseOptions::strict(),
            )
            .unwrap()
            .unwrap();

        assert_eq!(parsed.metadata().diagram_type, "venn");
        let RenderSemanticModel::Venn(model) = parsed.model() else {
            panic!("expected Venn render model");
        };
        assert_eq!(model.subsets.len(), 3);
        assert_eq!(model.subsets[2].sets, ["A", "B"]);
    }
}
