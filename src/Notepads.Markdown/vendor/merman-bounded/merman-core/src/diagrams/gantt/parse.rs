use super::*;
use crate::diagrams::scan::{LineCursor, leading_whitespace_len, starts_with_case_insensitive};
use crate::{
    EditorExpectedSyntax, EditorExpectedSyntaxKind, EditorSemanticFacts, EditorSemanticKind,
    EditorSemanticSymbol, OperationControl, OperationControlResult, SourceSpan,
    family::CombinedSemanticFailure,
};
use serde_json::Map;
#[cfg(test)]
use std::cell::Cell;

#[cfg(test)]
thread_local! {
    static GANTT_SYNTAX_CONSTRUCTION_COUNT: Cell<usize> = const { Cell::new(0) };
}

#[cfg(test)]
pub(crate) fn reset_gantt_syntax_construction_count() {
    GANTT_SYNTAX_CONSTRUCTION_COUNT.set(0);
}

#[cfg(test)]
pub(crate) fn gantt_syntax_construction_count() -> usize {
    GANTT_SYNTAX_CONSTRUCTION_COUNT.get()
}

fn strip_inline_comment(line: &str) -> &str {
    // Mermaid gantt does not treat `%%` as an inline comment delimiter for statements like `title`
    // or task lines (see `fixtures/gantt/task_inline_percent_comment.mmd`). It does, however,
    // accept full-line `%% ...` comments (and directive lines `%%{...}%%`).
    let t = line.trim_start();
    if t.starts_with("%%{") {
        return line;
    }
    if t.starts_with("%%") {
        return "";
    }
    line
}

fn split_statement_suffix(s: &str) -> &str {
    let mut end = s.len();
    for (i, c) in s.char_indices() {
        if c == '#' || c == ';' {
            end = i;
            break;
        }
    }
    &s[..end]
}

fn parse_key_colon_value_spanned<'a>(
    line: &'a str,
    line_start: usize,
    key: &str,
) -> Option<SpannedText<'a>> {
    let trimmed = line.trim_start();
    if !starts_with_case_insensitive(trimmed, key) {
        return None;
    }
    let leading = line.len().saturating_sub(trimmed.len());
    let after_key_start = key.len();
    let after_key = &trimmed[after_key_start..];
    let after_key_ws = leading_whitespace_len(after_key);
    let colon_start = after_key_start + after_key_ws;
    let rest_start = colon_start + ':'.len_utf8();
    if !trimmed[colon_start..].starts_with(':') {
        return None;
    }
    let rest = &trimmed[rest_start..];
    let rest_ws = leading_whitespace_len(rest);
    let value_start = rest_start + rest_ws;
    Some(SpannedText {
        text: &trimmed[value_start..],
        start: line_start + leading + value_start,
        end: line_start + leading + trimmed.len(),
    })
}

fn parse_click_statement(
    line: &str,
    line_start: usize,
) -> std::result::Result<Option<ClickStatementParts<'_>>, ClickStatementError> {
    let trimmed = line.trim_start();
    if !starts_with_click_keyword(trimmed, "click") {
        return Ok(None);
    }
    let leading = line.len().saturating_sub(trimmed.len());
    let after_click = &trimmed["click".len()..];
    let rest_leading = leading_whitespace_len(after_click);
    let rest_start = "click".len() + rest_leading;
    let rest = &trimmed[rest_start..];
    let ids_len = rest
        .char_indices()
        .find_map(|(idx, ch)| ch.is_whitespace().then_some(idx))
        .unwrap_or(rest.len());
    let ids = SpannedText {
        text: &rest[..ids_len],
        start: line_start + leading + rest_start,
        end: line_start + leading + rest_start + ids_len,
    };
    if ids.text.is_empty() {
        return Err(ClickStatementError::new(
            "invalid click statement: missing task id",
        ));
    }

    let mut tail_offset = rest_start + ids_len;
    let action_separator_present = tail_offset < trimmed.len();
    tail_offset += leading_whitespace_len(&trimmed[tail_offset..]);
    let mut href = None;
    let mut href_keyword = None;
    let mut call = None;
    let mut call_keyword = None;

    while tail_offset < trimmed.len() {
        let tail = &trimmed[tail_offset..];
        if starts_with_click_keyword(tail, "href") {
            if href.is_some() {
                return Err(ClickStatementError::new(
                    "invalid click statement: duplicate href",
                ));
            }
            let href_keyword_end = tail_offset + "href".len();
            href_keyword = Some(SourceSpan::new(
                line_start + leading + tail_offset,
                line_start + leading + href_keyword_end,
            ));
            let after_href = &trimmed[href_keyword_end..];
            let href_ws = leading_whitespace_len(after_href);
            let quote_start = href_keyword_end + href_ws;
            let value_start = quote_start + '"'.len_utf8();
            if !trimmed[quote_start..].starts_with('"') {
                return Err(ClickStatementError::new(
                    "invalid click statement: href requires a quoted URL",
                ));
            }
            let value_tail = &trimmed[value_start..];
            let Some(end) = value_tail.find('"') else {
                return Err(ClickStatementError::new(
                    "invalid click statement: unterminated href URL",
                ));
            };
            let value_end = value_start + end;
            href = Some(SpannedText {
                text: &trimmed[value_start..value_end],
                start: line_start + leading + value_start,
                end: line_start + leading + value_end,
            });
            tail_offset = value_end + '"'.len_utf8();
            tail_offset += leading_whitespace_len(&trimmed[tail_offset..]);
            continue;
        }

        if starts_with_click_keyword(tail, "call") {
            if call.is_some() {
                return Err(ClickStatementError::new(
                    "invalid click statement: duplicate callback",
                ));
            }
            let call_keyword_end = tail_offset + "call".len();
            call_keyword = Some(SourceSpan::new(
                line_start + leading + tail_offset,
                line_start + leading + call_keyword_end,
            ));
            let after_call = &trimmed[call_keyword_end..];
            let call_ws = leading_whitespace_len(after_call);
            let name_start = call_keyword_end + call_ws;
            let (parsed_call, next_offset) =
                parse_callback_tail(trimmed, name_start, line_start + leading)
                    .map_err(ClickStatementError::new)?;
            call = Some(parsed_call);
            tail_offset = next_offset;
            tail_offset += leading_whitespace_len(&trimmed[tail_offset..]);
            continue;
        }

        if tail.starts_with('"') {
            tail_offset =
                skip_quoted_click_tail(trimmed, tail_offset).map_err(ClickStatementError::new)?;
            tail_offset += leading_whitespace_len(&trimmed[tail_offset..]);
            continue;
        }

        if ["href", "call"]
            .iter()
            .any(|keyword| starts_with_case_insensitive(keyword, tail))
        {
            return Err(ClickStatementError::with_expected_action(
                "invalid click statement: incomplete action",
                SourceSpan::new(
                    line_start + leading + tail_offset,
                    line_start + leading + trimmed.len(),
                ),
            ));
        }

        if call.is_none() {
            let (parsed_call, next_offset) =
                parse_callback_tail(trimmed, tail_offset, line_start + leading)
                    .map_err(ClickStatementError::new)?;
            call = Some(parsed_call);
            tail_offset = next_offset;
            tail_offset += leading_whitespace_len(&trimmed[tail_offset..]);
            continue;
        }

        return Err(ClickStatementError::new(format!(
            "invalid click statement tail: {tail:?}"
        )));
    }

    if href.is_none() && call.is_none() {
        let expected_action = action_separator_present.then(|| {
            let end = line_start + leading + trimmed.len();
            SourceSpan::new(end, end)
        });
        return Err(ClickStatementError {
            message: "invalid click statement: missing href or callback".to_string(),
            expected_action,
        });
    }

    Ok(Some(ClickStatementParts {
        ids,
        href,
        href_keyword,
        call,
        call_keyword,
    }))
}

#[derive(Debug)]
struct ClickStatementError {
    message: String,
    expected_action: Option<SourceSpan>,
}

impl ClickStatementError {
    fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
            expected_action: None,
        }
    }

    fn with_expected_action(message: impl Into<String>, span: SourceSpan) -> Self {
        Self {
            message: message.into(),
            expected_action: Some(span),
        }
    }
}

fn skip_quoted_click_tail(trimmed: &str, quote_start: usize) -> std::result::Result<usize, String> {
    let value_start = quote_start + '"'.len_utf8();
    let Some(end) = trimmed[value_start..].find('"') else {
        return Err("invalid click statement: unterminated quoted tail".to_string());
    };
    Ok(value_start + end + '"'.len_utf8())
}

fn starts_with_click_keyword(input: &str, keyword: &str) -> bool {
    if !starts_with_case_insensitive(input, keyword) {
        return false;
    }
    input[keyword.len()..]
        .chars()
        .next()
        .is_some_and(char::is_whitespace)
}

fn parse_callback_tail<'a>(
    trimmed: &'a str,
    name_start: usize,
    absolute_offset: usize,
) -> std::result::Result<(ClickCallParts<'a>, usize), String> {
    let name_tail = &trimmed[name_start..];
    let Some(name_len) = callback_name_len(name_tail) else {
        return Err("invalid click statement: missing callback name".to_string());
    };
    let name_end = name_start + name_len;
    let mut next_offset = name_end;
    let args = if trimmed[next_offset..].starts_with('(') {
        let args_start = next_offset + '('.len_utf8();
        let args_tail = &trimmed[args_start..];
        let Some(end_rel) = args_tail.find(')') else {
            return Err("invalid click statement: unterminated callback args".to_string());
        };
        let args_end = args_start + end_rel;
        next_offset = args_end + ')'.len_utf8();
        let args_text = &trimmed[args_start..args_end];
        if args_text.trim().is_empty() {
            None
        } else {
            Some(SpannedText {
                text: args_text,
                start: absolute_offset + args_start,
                end: absolute_offset + args_end,
            })
        }
    } else {
        None
    };
    Ok((
        ClickCallParts {
            name: SpannedText {
                text: &trimmed[name_start..name_end],
                start: absolute_offset + name_start,
                end: absolute_offset + name_end,
            },
            args,
        },
        next_offset,
    ))
}

fn callback_name_len(input: &str) -> Option<usize> {
    let mut chars = input.char_indices();
    let (_, first) = chars.next()?;
    if !is_callback_name_start(first) {
        return None;
    }
    for (idx, ch) in chars {
        if !is_callback_name_continue(ch) {
            return Some(idx);
        }
    }
    Some(input.len())
}

fn is_callback_name_start(ch: char) -> bool {
    ch == '_' || ch == '$' || ch.is_ascii_alphabetic()
}

fn is_callback_name_continue(ch: char) -> bool {
    is_callback_name_start(ch) || ch.is_ascii_digit() || ch == '.'
}

#[derive(Debug, Clone, Copy)]
struct ClickStatementParts<'a> {
    ids: SpannedText<'a>,
    href: Option<SpannedText<'a>>,
    href_keyword: Option<SourceSpan>,
    call: Option<ClickCallParts<'a>>,
    call_keyword: Option<SourceSpan>,
}

#[derive(Debug, Clone, Copy)]
struct ClickCallParts<'a> {
    name: SpannedText<'a>,
    args: Option<SpannedText<'a>>,
}

fn parse_gantt_keyword_arg_spanned<'a>(
    line: &'a str,
    line_start: usize,
    keyword: &str,
    terminates_at_statement_suffix: bool,
) -> Option<SpannedText<'a>> {
    let trimmed = line.trim_start();
    if !starts_with_case_insensitive(trimmed, keyword) {
        return None;
    }
    let after = &trimmed[keyword.len()..];
    let ws = after.chars().next()?;
    if !ws.is_whitespace() {
        return None;
    }
    // The pinned Jison actions use `substr(keyword.len() + 1)`: one delimiter is consumed and
    // any additional whitespace remains part of the DB value.
    let ws_len = ws.len_utf8();
    let rest_start = keyword.len() + ws_len;
    let rest = &after[ws_len..];
    let text = if terminates_at_statement_suffix {
        split_statement_suffix(rest)
    } else {
        rest
    };
    if text.is_empty() {
        return None;
    }
    let leading = line.len().saturating_sub(trimmed.len());
    let start = line_start + leading + rest_start;
    Some(SpannedText {
        text,
        start,
        end: start + text.len(),
    })
}

#[derive(Debug)]
struct GanttAccDescrBlock {
    statement_start: usize,
    statement_end: usize,
    body: String,
    first_content_start: Option<usize>,
    last_content_end: Option<usize>,
    complete: bool,
}

impl GanttAccDescrBlock {
    fn start(line: &str, line_start: usize) -> Option<Self> {
        let trimmed = line.trim_start();
        if !starts_with_case_insensitive(trimmed, "accDescr") {
            return None;
        }

        let leading = line.len().saturating_sub(trimmed.len());
        let after_key = &trimmed["accDescr".len()..];
        let after_key_ws = leading_whitespace_len(after_key);
        let open_offset = leading + "accDescr".len() + after_key_ws;
        if !line[open_offset..].starts_with('{') {
            return None;
        }

        let body_start = open_offset + '{'.len_utf8();
        let mut block = Self {
            statement_start: line_start + leading,
            statement_end: line_start + line.len(),
            body: String::new(),
            first_content_start: None,
            last_content_end: None,
            complete: false,
        };
        block.accept_body_slice(&line[body_start..], line_start + body_start);
        Some(block)
    }

    fn accept_continuation_line(&mut self, line: &str, line_start: usize) -> bool {
        self.accept_body_slice(line, line_start);
        self.complete
    }

    fn is_complete(&self) -> bool {
        self.complete
    }

    fn accept_body_slice(&mut self, text: &str, start: usize) {
        let Some(close_offset) = text.find('}') else {
            self.append_text(text, start);
            self.body.push('\n');
            self.statement_end = start + text.len();
            return;
        };

        self.append_text(&text[..close_offset], start);
        self.statement_end = start + close_offset + '}'.len_utf8();
        self.complete = true;
    }

    fn append_text(&mut self, text: &str, start: usize) {
        self.body.push_str(text);

        if self.first_content_start.is_none() {
            let leading = leading_whitespace_len(text);
            if leading < text.len() {
                self.first_content_start = Some(start + leading);
            }
        }

        let trimmed_len = text.trim_end().len();
        if trimmed_len > 0 {
            self.last_content_end = Some(start + trimmed_len);
        }
    }

    fn consume_remaining(
        mut self,
        cursor: &mut LineCursor<'_>,
        control: &OperationControl,
    ) -> OperationControlResult<Self> {
        while !self.complete {
            control.checkpoint()?;
            let Some((line, line_start)) = cursor.next_line() else {
                break;
            };
            self.accept_continuation_line(line, line_start);
        }
        Ok(self)
    }

    fn resume_after_closing_brace(&self, cursor: &mut LineCursor<'_>) {
        if self.complete {
            cursor.resume_same_line_at(self.statement_end);
        }
    }

    fn value(&self) -> &str {
        self.body.trim()
    }

    fn statement_span(&self) -> SourceSpan {
        SourceSpan::new(self.statement_start, self.statement_end)
    }

    fn emit_symbol(&self, facts: &mut EditorSemanticFacts) {
        let text = self.body.trim();
        if text.is_empty() {
            return;
        }

        let Some(selection_start) = self.first_content_start else {
            return;
        };
        let Some(selection_end) = self.last_content_end else {
            return;
        };

        facts.push_symbol(EditorSemanticSymbol::payload(
            text.to_string(),
            Some("gantt accessibility description".to_string()),
            EditorSemanticKind::String,
            SourceSpan::new(self.statement_start, self.statement_end),
            SourceSpan::new(selection_start, selection_end),
        ));
        facts.push_expected_syntax(EditorExpectedSyntax::new(
            EditorExpectedSyntaxKind::Payload,
            SourceSpan::new(selection_start, selection_end),
        ));
    }
}

fn collect_gantt_click_symbols(
    line: &str,
    line_start: usize,
    click: ClickStatementParts<'_>,
    facts: &mut EditorSemanticFacts,
) {
    let statement_span = gantt_statement_span(line, line_start);

    push_gantt_delimited_id_symbols(
        GanttDelimitedIdSymbols {
            text: click.ids.text,
            text_start: click.ids.start,
            delimiter: ',',
            detail: "gantt click target",
            kind: EditorSemanticKind::Variable,
            expected_syntax: Some(EditorExpectedSyntaxKind::NodeIdentifier),
            statement_span,
            role: GanttIdRole::Reference,
        },
        facts,
    );

    for action in [click.href_keyword, click.call_keyword]
        .into_iter()
        .flatten()
    {
        facts.push_expected_syntax(EditorExpectedSyntax::new(
            EditorExpectedSyntaxKind::InteractionAction,
            action,
        ));
    }

    if let Some(href) = click.href {
        push_gantt_payload_symbol(
            line,
            line_start,
            href,
            "gantt click href",
            EditorSemanticKind::String,
            facts,
        );
    }

    if let Some(call) = click.call {
        push_gantt_payload_symbol(
            line,
            line_start,
            call.name,
            "gantt click callback",
            EditorSemanticKind::Function,
            facts,
        );
        if let Some(args) = call.args {
            push_gantt_payload_symbol(
                line,
                line_start,
                args,
                "gantt click callback args",
                EditorSemanticKind::String,
                facts,
            );
        }
    }
}

fn collect_gantt_section_symbol(
    line: &str,
    line_start: usize,
    section: SpannedText<'_>,
    facts: &mut EditorSemanticFacts,
) {
    let Some(section) = section.trim() else {
        return;
    };

    facts.push_symbol(EditorSemanticSymbol::outline(
        section.text,
        Some("gantt section".to_string()),
        EditorSemanticKind::Namespace,
        gantt_statement_span(line, line_start),
        section.span(),
    ));
}

fn push_gantt_payload_symbol(
    line: &str,
    line_start: usize,
    field: SpannedText<'_>,
    detail: &'static str,
    kind: EditorSemanticKind,
    facts: &mut EditorSemanticFacts,
) {
    let Some(field) = field.trim() else {
        return;
    };
    facts.push_expected_syntax(EditorExpectedSyntax::new(
        EditorExpectedSyntaxKind::Payload,
        field.span(),
    ));
    facts.push_symbol(EditorSemanticSymbol::payload(
        field.text,
        Some(detail.to_string()),
        kind,
        gantt_statement_span(line, line_start),
        field.span(),
    ));
}

fn gantt_statement_span(line: &str, line_start: usize) -> SourceSpan {
    let trimmed_line = line.trim_start();
    let leading = line.len().saturating_sub(trimmed_line.len());
    SourceSpan::new(
        line_start + leading,
        line_start + leading + trimmed_line.len(),
    )
}

fn collect_gantt_task_field_symbols(
    fields: &[SpannedText<'_>],
    statement_span: SourceSpan,
    facts: &mut EditorSemanticFacts,
) {
    let mut field_start = 0usize;
    while fields
        .get(field_start)
        .is_some_and(|field| is_gantt_task_tag(field.text))
    {
        field_start += 1;
    }

    let fields = &fields[field_start..];
    match fields {
        [end_data] => push_gantt_relative_ref_symbols(end_data, statement_span, facts),
        [start_data, end_data] => {
            push_gantt_relative_ref_symbols(start_data, statement_span, facts);
            push_gantt_relative_ref_symbols(end_data, statement_span, facts);
        }
        [id, start_data, end_data] => {
            push_gantt_id_symbol(
                *id,
                "gantt task",
                EditorSemanticKind::Variable,
                Some(EditorExpectedSyntaxKind::NodeIdentifier),
                statement_span,
                GanttIdRole::Definition,
                facts,
            );
            push_gantt_relative_ref_symbols(start_data, statement_span, facts);
            push_gantt_relative_ref_symbols(end_data, statement_span, facts);
        }
        _ => {}
    }
}

fn is_gantt_task_tag(text: &str) -> bool {
    matches!(text, "active" | "done" | "crit" | "milestone" | "vert")
}

fn push_gantt_relative_ref_symbols(
    field: &SpannedText<'_>,
    statement_span: SourceSpan,
    facts: &mut EditorSemanticFacts,
) {
    for keyword in ["after", "until"] {
        let Some(range) = relative_ref_ids_range(field.text, keyword) else {
            continue;
        };
        push_gantt_delimited_id_symbols(
            GanttDelimitedIdSymbols {
                text: &field.text[range.clone()],
                text_start: field.start + range.start,
                delimiter: ' ',
                detail: "gantt dependency",
                kind: EditorSemanticKind::Variable,
                expected_syntax: Some(EditorExpectedSyntaxKind::NodeIdentifier),
                statement_span,
                role: GanttIdRole::Reference,
            },
            facts,
        );
    }
}

struct GanttDelimitedIdSymbols<'a> {
    text: &'a str,
    text_start: usize,
    delimiter: char,
    detail: &'a str,
    kind: EditorSemanticKind,
    expected_syntax: Option<EditorExpectedSyntaxKind>,
    statement_span: SourceSpan,
    role: GanttIdRole,
}

#[derive(Clone, Copy)]
enum GanttIdRole {
    Definition,
    Reference,
}

fn push_gantt_delimited_id_symbols(
    request: GanttDelimitedIdSymbols<'_>,
    facts: &mut EditorSemanticFacts,
) {
    let GanttDelimitedIdSymbols {
        text,
        text_start,
        delimiter,
        detail,
        kind,
        expected_syntax,
        statement_span,
        role,
    } = request;
    let mut segment_start = 0usize;
    for (idx, ch) in text.char_indices() {
        if ch == delimiter {
            push_gantt_id_symbol(
                SpannedText {
                    text: &text[segment_start..idx],
                    start: text_start + segment_start,
                    end: text_start + idx,
                },
                detail,
                kind,
                expected_syntax,
                statement_span,
                role,
                facts,
            );
            segment_start = idx + ch.len_utf8();
        }
    }

    push_gantt_id_symbol(
        SpannedText {
            text: &text[segment_start..],
            start: text_start + segment_start,
            end: text_start + text.len(),
        },
        detail,
        kind,
        expected_syntax,
        statement_span,
        role,
        facts,
    );
}

fn push_gantt_id_symbol(
    field: SpannedText<'_>,
    detail: &str,
    kind: EditorSemanticKind,
    expected_syntax: Option<EditorExpectedSyntaxKind>,
    statement_span: SourceSpan,
    role: GanttIdRole,
    facts: &mut EditorSemanticFacts,
) {
    let Some(field) = field.trim() else {
        return;
    };
    if let Some(expected_syntax) = expected_syntax {
        facts.push_expected_syntax(EditorExpectedSyntax::new(expected_syntax, field.span()));
    }
    let symbol = match role {
        GanttIdRole::Definition => EditorSemanticSymbol::new(
            field.text,
            Some(detail.to_string()),
            kind,
            statement_span,
            field.span(),
        ),
        GanttIdRole::Reference => EditorSemanticSymbol::reference(
            field.text,
            Some(detail.to_string()),
            kind,
            statement_span,
            field.span(),
        ),
    };
    facts.push_symbol(symbol);
}

fn split_gantt_fields(text: &str, text_start: usize) -> Vec<SpannedText<'_>> {
    let mut out = Vec::new();
    let mut field_start = 0usize;
    for (idx, ch) in text.char_indices() {
        if ch == ',' {
            out.push(SpannedText {
                text: &text[field_start..idx],
                start: text_start + field_start,
                end: text_start + idx,
            });
            field_start = idx + ch.len_utf8();
        }
    }

    out.push(SpannedText {
        text: &text[field_start..],
        start: text_start + field_start,
        end: text_start + text.len(),
    });
    out
}

#[derive(Debug, Clone, Copy)]
struct SpannedText<'a> {
    text: &'a str,
    start: usize,
    end: usize,
}

impl<'a> SpannedText<'a> {
    fn trim(self) -> Option<Self> {
        let leading = self.text.len().saturating_sub(self.text.trim_start().len());
        let text = &self.text[leading..];
        let trimmed_len = text.trim_end().len();
        if trimmed_len == 0 {
            return None;
        }

        Some(Self {
            text: &text[..trimmed_len],
            start: self.start + leading,
            end: self.start + leading + trimmed_len,
        })
    }

    fn span(self) -> SourceSpan {
        SourceSpan::new(self.start, self.end)
    }
}

pub(crate) fn parse_gantt(code: &str, meta: &ParseMetadata) -> Result<Value> {
    let Some(db) = parse_gantt_semantic_source(code, meta)?.db else {
        return Ok(json!({}));
    };
    let model = gantt_db_to_render_model(db)?;
    super::render_model_to_compat_json(&model, meta)
}

#[cfg(test)]
pub(crate) fn parse_gantt_model_for_render(
    code: &str,
    meta: &ParseMetadata,
) -> Result<GanttDiagramRenderModel> {
    let Some(db) = parse_gantt_semantic_source(code, meta)?.db else {
        return Ok(GanttDiagramRenderModel::empty_compatibility_output());
    };
    gantt_db_to_render_model(db)
}

pub(crate) fn parse_gantt_model_for_render_controlled(
    code: &str,
    meta: &ParseMetadata,
    control: &OperationControl,
) -> OperationControlResult<Result<GanttDiagramRenderModel>> {
    let construction = construct_gantt_semantic_source_controlled(code, meta, control)?;
    let source = match construction {
        Ok(source) => source,
        Err(failure) => return Ok(Err(failure.into_error())),
    };
    match source.db {
        Some(db) => gantt_db_to_render_model_controlled(db, control),
        None => Ok(Ok(GanttDiagramRenderModel::empty_compatibility_output())),
    }
}

pub(crate) fn parse_gantt_json_and_editor_facts(
    code: &str,
    meta: &ParseMetadata,
    control: &OperationControl,
) -> OperationControlResult<crate::family::CombinedSemanticParse> {
    let construction = match construct_gantt_semantic_source_controlled(code, meta, control)? {
        Ok(source) => Ok(source.into_combined_parts_controlled(meta, control)?),
        Err(error) => Err(error),
    };
    Ok(crate::family::CombinedSemanticParse::from_construction(
        construction,
        |parts| parts,
        CombinedSemanticFailure::into_parts,
    ))
}

struct GanttSemanticSource {
    db: Option<GanttDb>,
    editor_facts: EditorSemanticFacts,
}

impl GanttSemanticSource {
    fn into_combined_parts_controlled(
        self,
        meta: &ParseMetadata,
        control: &OperationControl,
    ) -> OperationControlResult<(Result<Value>, EditorSemanticFacts)> {
        let Self { db, editor_facts } = self;
        let model = match db {
            Some(db) => match gantt_db_to_render_model_controlled(db, control)? {
                Ok(model) => render_model_to_compat_json_controlled(&model, meta, control)?,
                Err(error) => Err(error),
            },
            None => Ok(json!({})),
        };
        control.checkpoint()?;
        Ok((model, editor_facts))
    }
}

fn parse_gantt_semantic_source(code: &str, meta: &ParseMetadata) -> Result<GanttSemanticSource> {
    construct_gantt_semantic_source(code, meta).map_err(CombinedSemanticFailure::into_error)
}

fn construct_gantt_semantic_source(
    code: &str,
    meta: &ParseMetadata,
) -> std::result::Result<GanttSemanticSource, CombinedSemanticFailure> {
    construct_gantt_semantic_source_controlled(code, meta, &OperationControl::new())
        .expect("a private parse control cannot be cancelled")
}

fn construct_gantt_semantic_source_controlled(
    code: &str,
    meta: &ParseMetadata,
    control: &OperationControl,
) -> OperationControlResult<std::result::Result<GanttSemanticSource, CombinedSemanticFailure>> {
    control.checkpoint()?;
    #[cfg(test)]
    GANTT_SYNTAX_CONSTRUCTION_COUNT.set(GANTT_SYNTAX_CONSTRUCTION_COUNT.get() + 1);

    parse_gantt_semantic_source_once(code, meta, control)
}

fn parse_gantt_semantic_source_once(
    code: &str,
    meta: &ParseMetadata,
    control: &OperationControl,
) -> OperationControlResult<std::result::Result<GanttSemanticSource, CombinedSemanticFailure>> {
    control.checkpoint()?;
    let mut db = GanttDb::default();
    db.clear();
    db.set_security_level(meta.effective_config.get_str("securityLevel"));
    if let Some(dm) = meta.effective_config.get_str("gantt.displayMode") {
        db.set_display_mode(dm);
    }

    let mut cursor = LineCursor::new(code);
    let mut header_seen = false;
    let mut editor_facts = EditorSemanticFacts::new();
    let mut first_error = None;

    while let Some((line, line_start)) = cursor.next_line() {
        control.checkpoint()?;
        let stripped = strip_inline_comment(line);
        let trimmed = stripped.trim();
        if trimmed.is_empty() {
            continue;
        }

        if !header_seen {
            if starts_with_case_insensitive(trimmed, "gantt") {
                header_seen = true;
                let rest = trimmed["gantt".len()..].trim_start();
                if !rest.is_empty() {
                    let rest_offset = trimmed["gantt".len()..].len() - rest.len();
                    let trimmed_start = stripped.find(trimmed).unwrap_or(0);
                    parse_gantt_statement(
                        rest,
                        line_start + trimmed_start + "gantt".len() + rest_offset,
                        &mut db,
                        &mut cursor,
                        &mut editor_facts,
                        control,
                    )?
                    .unwrap_or_else(|error| {
                        first_error.get_or_insert(error);
                    });
                }
                continue;
            }
            first_error.get_or_insert(Error::diagram_parse_exact(
                "gantt".to_string(),
                "expected gantt header".to_string(),
                gantt_statement_span(stripped, line_start),
            ));
            parse_gantt_statement(
                stripped,
                line_start,
                &mut db,
                &mut cursor,
                &mut editor_facts,
                control,
            )?
            .unwrap_or_else(|error| {
                first_error.get_or_insert(error);
            });
            continue;
        }

        parse_gantt_statement(
            stripped,
            line_start,
            &mut db,
            &mut cursor,
            &mut editor_facts,
            control,
        )?
        .unwrap_or_else(|error| {
            first_error.get_or_insert(error);
        });
    }

    if let Some(error) = first_error {
        return Ok(Err(CombinedSemanticFailure::parser_recovery(
            "gantt",
            error,
            editor_facts,
        )));
    }

    if !header_seen {
        return Ok(Ok(GanttSemanticSource {
            db: None,
            editor_facts,
        }));
    }

    control.checkpoint()?;
    if let Err(error) = db.finalize_tasks_controlled(control)? {
        return Ok(Err(CombinedSemanticFailure::parser_recovery(
            "gantt",
            error,
            editor_facts,
        )));
    }
    Ok(Ok(GanttSemanticSource {
        db: Some(db),
        editor_facts,
    }))
}

pub(crate) fn render_model_to_compat_json(
    model: &GanttDiagramRenderModel,
    meta: &ParseMetadata,
) -> Result<Value> {
    let control = OperationControl::new();
    render_model_to_compat_json_controlled(model, meta, &control)
        .expect("a private parse control cannot be cancelled")
}

pub(crate) fn render_model_to_compat_json_controlled(
    model: &GanttDiagramRenderModel,
    meta: &ParseMetadata,
    control: &OperationControl,
) -> OperationControlResult<Result<Value>> {
    control.checkpoint()?;
    if model.compatibility_output == CompatibilityOutputState::Empty {
        return Ok(Ok(json!({})));
    }

    let includes = strings_to_json_controlled(&model.includes, control)?;
    let excludes = strings_to_json_controlled(&model.excludes, control)?;
    let sections = strings_to_json_controlled(&model.sections, control)?;
    let tasks = serialize_gantt_tasks_controlled(&model.tasks, control)?;
    let links = string_map_to_json_controlled(&model.links, control)?;
    let click_events = serialize_map_to_json_controlled(&model.click_events, control)?;

    let mut out = Map::with_capacity(19);
    out.insert("type".to_string(), Value::String(meta.diagram_type.clone()));
    out.insert("title".to_string(), json!(&model.title));
    out.insert("accTitle".to_string(), json!(&model.acc_title));
    out.insert("accDescr".to_string(), json!(&model.acc_descr));
    out.insert(
        "dateFormat".to_string(),
        Value::String(model.date_format.clone()),
    );
    out.insert(
        "axisFormat".to_string(),
        Value::String(model.axis_format.clone()),
    );
    out.insert("tickInterval".to_string(), json!(&model.tick_interval));
    out.insert(
        "todayMarker".to_string(),
        Value::String(model.today_marker.clone()),
    );
    out.insert("includes".to_string(), Value::Array(includes));
    out.insert("excludes".to_string(), Value::Array(excludes));
    out.insert(
        "inclusiveEndDates".to_string(),
        Value::Bool(model.inclusive_end_dates),
    );
    out.insert("topAxis".to_string(), Value::Bool(model.top_axis));
    out.insert("weekday".to_string(), Value::String(model.weekday.clone()));
    out.insert("weekend".to_string(), Value::String(model.weekend.clone()));
    out.insert(
        "displayMode".to_string(),
        Value::String(model.display_mode.clone()),
    );
    out.insert("sections".to_string(), Value::Array(sections));
    out.insert("tasks".to_string(), Value::Array(tasks));
    out.insert("links".to_string(), Value::Object(links));
    out.insert("clickEvents".to_string(), Value::Object(click_events));
    control.checkpoint()?;
    Ok(Ok(Value::Object(out)))
}

fn strings_to_json_controlled(
    strings: &[String],
    control: &OperationControl,
) -> OperationControlResult<Vec<Value>> {
    let mut values = Vec::with_capacity(strings.len());
    for (index, value) in strings.iter().enumerate() {
        if index % 128 == 0 {
            control.checkpoint()?;
        }
        values.push(Value::String(value.clone()));
    }
    Ok(values)
}

fn serialize_gantt_tasks_controlled(
    tasks: &[GanttRenderTask],
    control: &OperationControl,
) -> OperationControlResult<Vec<Value>> {
    let mut values = Vec::with_capacity(tasks.len());
    for (index, task) in tasks.iter().enumerate() {
        if index % 128 == 0 {
            control.checkpoint()?;
        }
        let mut value = json!(task);
        if let Some(object) = value.as_object_mut() {
            object.remove("sectionIndex");
            object.remove("startConstraint");
            object.remove("endConstraint");
        }
        values.push(value);
    }
    Ok(values)
}

fn string_map_to_json_controlled(
    values: &HashMap<String, String>,
    control: &OperationControl,
) -> OperationControlResult<Map<String, Value>> {
    let mut out = Map::with_capacity(values.len());
    for (index, (key, value)) in values.iter().enumerate() {
        if index % 128 == 0 {
            control.checkpoint()?;
        }
        out.insert(key.clone(), Value::String(value.clone()));
    }
    Ok(out)
}

fn serialize_map_to_json_controlled<T: serde::Serialize>(
    values: &HashMap<String, T>,
    control: &OperationControl,
) -> OperationControlResult<Map<String, Value>> {
    let mut out = Map::with_capacity(values.len());
    for (index, (key, value)) in values.iter().enumerate() {
        if index % 128 == 0 {
            control.checkpoint()?;
        }
        out.insert(key.clone(), json!(value));
    }
    Ok(out)
}

fn gantt_db_to_render_model(db: GanttDb) -> Result<GanttDiagramRenderModel> {
    let control = OperationControl::new();
    gantt_db_to_render_model_controlled(db, &control)
        .expect("a private parse control cannot be cancelled")
}

pub(super) fn gantt_db_to_render_model_controlled(
    mut db: GanttDb,
    control: &OperationControl,
) -> OperationControlResult<Result<GanttDiagramRenderModel>> {
    control.checkpoint()?;
    let raw_tasks = db.take_tasks();
    let mut tasks = Vec::with_capacity(raw_tasks.len());
    for (index, task) in raw_tasks.into_iter().enumerate() {
        if index % 128 == 0 {
            control.checkpoint()?;
        }
        match raw_task_to_render_task(task, &db.date_format) {
            Ok(task) => tasks.push(task),
            Err(error) => return Ok(Err(error)),
        }
    }

    control.checkpoint()?;
    Ok(Ok(GanttDiagramRenderModel {
        title: non_empty_opt(std::mem::take(&mut db.diagram_title)),
        acc_title: non_empty_opt(std::mem::take(&mut db.acc_title)),
        acc_descr: non_empty_opt(std::mem::take(&mut db.acc_descr)),
        date_format: std::mem::take(&mut db.date_format),
        axis_format: std::mem::take(&mut db.axis_format),
        tick_interval: db.tick_interval.take(),
        today_marker: std::mem::take(&mut db.today_marker),
        includes: std::mem::take(&mut db.includes),
        excludes: std::mem::take(&mut db.excludes),
        inclusive_end_dates: db.inclusive_end_dates,
        display_mode: std::mem::take(&mut db.display_mode),
        top_axis: db.top_axis,
        weekday: std::mem::take(&mut db.weekday),
        weekend: std::mem::take(&mut db.weekend),
        sections: std::mem::take(&mut db.sections),
        tasks,
        links: std::mem::take(&mut db.links),
        click_events: std::mem::take(&mut db.click_events),
        compatibility_output: CompatibilityOutputState::Model,
    }))
}

fn non_empty_opt(value: String) -> Option<String> {
    if value.is_empty() { None } else { Some(value) }
}

fn raw_task_to_render_task(t: RawTask, date_format: &str) -> Result<GanttRenderTask> {
    let start_ms = task_time_ms(&t, "startTime", t.start_time)?;
    let end_ms = task_time_ms(&t, "endTime", t.end_time)?;
    let (raw_start, start_constraint) = match t.raw.start_time {
        StartTimeRaw::PrevTaskEnd => (
            GanttRenderTaskStart::PrevTaskEnd {
                id: t.prev_task_id.clone(),
            },
            GanttTaskStartConstraint::PreviousTaskEnd {
                dependency_id: t.prev_task_id.clone(),
            },
        ),
        StartTimeRaw::GetStartDate { start_data } => {
            let constraint = match relative_dependency_ids(&start_data, "after") {
                Some(dependency_ids) => GanttTaskStartConstraint::After { dependency_ids },
                None => GanttTaskStartConstraint::Fixed {
                    value: start_data.clone(),
                },
            };
            (
                GanttRenderTaskStart::GetStartDate { start_data },
                constraint,
            )
        }
    };
    let end_constraint = gantt_end_constraint(&t.raw.end_data, date_format);

    Ok(GanttRenderTask {
        id: t.id,
        task: t.task,
        section: t.section,
        section_index: t.section_index,
        task_type: t.type_,
        classes: t.classes,
        active: t.active,
        done: t.done,
        crit: t.crit,
        milestone: t.milestone,
        vert: t.vert,
        order: t.order,
        prev_task_id: t.prev_task_id,
        start_constraint,
        end_constraint,
        processed: t.processed,
        manual_end_time: t.manual_end_time,
        raw: GanttRenderTaskRaw {
            data: t.raw.data,
            start_time: raw_start,
            end_time: GanttRenderTaskEnd {
                data: t.raw.end_data,
            },
        },
        start_ms,
        end_ms,
        render_end_ms: t.render_end_time.map(|d| d.timestamp_millis()),
    })
}

fn relative_dependency_ids(value: &str, keyword: &str) -> Option<Vec<String>> {
    relative_ref_ids(value.trim(), keyword).map(|ids| {
        ids.split(' ')
            .filter(|id| !id.is_empty())
            .map(str::to_string)
            .collect()
    })
}

fn gantt_end_constraint(value: &str, date_format: &str) -> GanttTaskEndConstraint {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return GanttTaskEndConstraint::Unspecified;
    }
    if let Some(dependency_ids) = relative_dependency_ids(trimmed, "until") {
        return GanttTaskEndConstraint::Until { dependency_ids };
    }
    if parse_dayjs_like_strict(date_format.trim(), trimmed).is_some() {
        return GanttTaskEndConstraint::Fixed {
            value: value.to_string(),
        };
    }
    GanttTaskEndConstraint::Duration {
        value: value.to_string(),
    }
}

fn task_time_ms(task: &RawTask, field: &str, value: Option<OffsetDateTime>) -> Result<i64> {
    value.map(|d| d.timestamp_millis()).ok_or_else(|| {
        Error::diagram_parse_fallback(
            "gantt".to_string(),
            format!("task `{}` has unresolved {field}", task.id),
        )
    })
}

fn parse_gantt_statement(
    line: &str,
    line_start: usize,
    db: &mut GanttDb,
    cursor: &mut LineCursor<'_>,
    facts: &mut EditorSemanticFacts,
    control: &OperationControl,
) -> OperationControlResult<Result<()>> {
    control.checkpoint()?;
    let stripped = strip_inline_comment(line);
    let t = stripped.trim();
    if t.is_empty() {
        return Ok(Ok(()));
    }

    if let Some(v) = parse_gantt_keyword_arg_spanned(stripped, line_start, "dateFormat", true) {
        facts.push_directive_prefix("dateFormat");
        push_gantt_payload_symbol(
            stripped,
            line_start,
            v,
            "gantt date format",
            EditorSemanticKind::String,
            facts,
        );
        db.set_date_format(v.text);
        return Ok(Ok(()));
    }
    if starts_with_case_insensitive(t, "inclusiveEndDates") {
        facts.push_directive_prefix("inclusiveEndDates");
        db.enable_inclusive_end_dates();
        return Ok(Ok(()));
    }
    if starts_with_case_insensitive(t, "topAxis") {
        facts.push_directive_prefix("topAxis");
        db.enable_top_axis();
        return Ok(Ok(()));
    }
    if let Some(v) = parse_gantt_keyword_arg_spanned(stripped, line_start, "axisFormat", true) {
        facts.push_directive_prefix("axisFormat");
        push_gantt_payload_symbol(
            stripped,
            line_start,
            v,
            "gantt axis format",
            EditorSemanticKind::String,
            facts,
        );
        db.set_axis_format(v.text);
        return Ok(Ok(()));
    }
    if let Some(v) = parse_gantt_keyword_arg_spanned(stripped, line_start, "tickInterval", true) {
        facts.push_directive_prefix("tickInterval");
        push_gantt_payload_symbol(
            stripped,
            line_start,
            v,
            "gantt tick interval",
            EditorSemanticKind::String,
            facts,
        );
        db.set_tick_interval(v.text.trim());
        return Ok(Ok(()));
    }
    if let Some(v) = parse_gantt_keyword_arg_spanned(stripped, line_start, "includes", true) {
        facts.push_directive_prefix("includes");
        push_gantt_payload_symbol(
            stripped,
            line_start,
            v,
            "gantt includes",
            EditorSemanticKind::String,
            facts,
        );
        db.set_includes(v.text);
        return Ok(Ok(()));
    }
    if let Some(v) = parse_gantt_keyword_arg_spanned(stripped, line_start, "excludes", true) {
        facts.push_directive_prefix("excludes");
        push_gantt_payload_symbol(
            stripped,
            line_start,
            v,
            "gantt excludes",
            EditorSemanticKind::String,
            facts,
        );
        db.set_excludes(v.text);
        return Ok(Ok(()));
    }
    if let Some(v) = parse_gantt_keyword_arg_spanned(stripped, line_start, "todayMarker", false) {
        facts.push_directive_prefix("todayMarker");
        push_gantt_payload_symbol(
            stripped,
            line_start,
            v,
            "gantt today marker",
            EditorSemanticKind::String,
            facts,
        );
        db.set_today_marker(v.text.trim());
        return Ok(Ok(()));
    }
    if let Some(v) = parse_gantt_keyword_arg_spanned(stripped, line_start, "weekday", false) {
        facts.push_directive_prefix("weekday");
        push_gantt_payload_symbol(
            stripped,
            line_start,
            v,
            "gantt weekday",
            EditorSemanticKind::String,
            facts,
        );
        let trimmed_day = v.trim();
        let day = trimmed_day
            .map(|value| value.text.to_lowercase())
            .unwrap_or_default();
        if !matches!(
            day.as_str(),
            "monday" | "tuesday" | "wednesday" | "thursday" | "friday" | "saturday" | "sunday"
        ) {
            let span = trimmed_day
                .map(SpannedText::span)
                .unwrap_or_else(|| SourceSpan::new(v.start, v.start));
            return Ok(Err(Error::diagram_parse_exact(
                "gantt".to_string(),
                format!("invalid weekday: {day}"),
                span,
            )));
        }
        db.set_weekday(&day);
        return Ok(Ok(()));
    }
    if let Some(v) = parse_gantt_keyword_arg_spanned(stripped, line_start, "weekend", false) {
        facts.push_directive_prefix("weekend");
        push_gantt_payload_symbol(
            stripped,
            line_start,
            v,
            "gantt weekend",
            EditorSemanticKind::String,
            facts,
        );
        let trimmed_day = v.trim();
        let day = trimmed_day
            .map(|value| value.text.to_lowercase())
            .unwrap_or_default();
        if !matches!(day.as_str(), "friday" | "saturday") {
            let span = trimmed_day
                .map(SpannedText::span)
                .unwrap_or_else(|| SourceSpan::new(v.start, v.start));
            return Ok(Err(Error::diagram_parse_exact(
                "gantt".to_string(),
                format!("invalid weekend: {day}"),
                span,
            )));
        }
        db.set_weekend(&day);
        return Ok(Ok(()));
    }
    if let Some(v) = parse_gantt_keyword_arg_spanned(stripped, line_start, "title", false) {
        facts.push_directive_prefix("title");
        push_gantt_payload_symbol(
            stripped,
            line_start,
            v,
            "gantt title",
            EditorSemanticKind::String,
            facts,
        );
        db.set_diagram_title(v.text);
        return Ok(Ok(()));
    }
    if let Some(v) = parse_gantt_keyword_arg_spanned(stripped, line_start, "section", false) {
        facts.push_directive_prefix("section");
        collect_gantt_section_symbol(stripped, line_start, v, facts);
        db.add_section(v.text.trim());
        return Ok(Ok(()));
    }
    if let Some(v) = parse_key_colon_value_spanned(stripped, line_start, "accTitle") {
        facts.push_directive_prefix("accTitle");
        push_gantt_payload_symbol(
            stripped,
            line_start,
            v,
            "gantt accessibility title",
            EditorSemanticKind::String,
            facts,
        );
        db.set_acc_title(v.text.trim());
        return Ok(Ok(()));
    }
    if let Some(v) = parse_key_colon_value_spanned(stripped, line_start, "accDescr") {
        facts.push_directive_prefix("accDescr");
        push_gantt_payload_symbol(
            stripped,
            line_start,
            v,
            "gantt accessibility description",
            EditorSemanticKind::String,
            facts,
        );
        db.set_acc_descr(v.text.trim());
        return Ok(Ok(()));
    }
    if let Some(block) = GanttAccDescrBlock::start(stripped, line_start) {
        facts.push_directive_prefix("accDescr");
        let block = block.consume_remaining(cursor, control)?;
        block.resume_after_closing_brace(cursor);
        db.set_acc_descr(block.value());
        block.emit_symbol(facts);
        if !block.is_complete() {
            return Ok(Err(Error::diagram_parse_insertion_point(
                "gantt".to_string(),
                "unterminated accDescr block",
                block.statement_span().end,
            )));
        }
        return Ok(Ok(()));
    }
    match parse_click_statement(stripped, line_start) {
        Ok(Some(click)) => {
            facts.push_directive_prefix("click");
            collect_gantt_click_symbols(stripped, line_start, click, facts);
            if let Some(call) = click.call {
                db.set_click_event(
                    click.ids.text,
                    call.name.text.trim(),
                    call.args.map(|args| args.text),
                );
            }
            if let Some(href) = click.href {
                db.set_link(click.ids.text, href.text);
            }
            return Ok(Ok(()));
        }
        Ok(None) => {}
        Err(error) => {
            facts.push_directive_prefix("click");
            if let Some(expected_action) = error.expected_action {
                facts.push_expected_syntax(EditorExpectedSyntax::new(
                    EditorExpectedSyntaxKind::InteractionAction,
                    expected_action,
                ));
            }
            return Ok(Err(Error::diagram_parse_exact(
                "gantt".to_string(),
                error.message,
                gantt_statement_span(stripped, line_start),
            )));
        }
    }

    let task_stmt = stripped.trim_start();

    let Some(colon) = task_stmt.find(':') else {
        return Ok(Err(Error::diagram_parse_exact(
            "gantt".to_string(),
            format!("unrecognized statement: {t}"),
            gantt_statement_span(stripped, line_start),
        )));
    };

    // Mermaid passes `taskTxt` through to the DB without trimming. This preserves any trailing
    // whitespace before the `:` delimiter (e.g. `Task1 :id,...` yields `Task1 `).
    let task_txt = &task_stmt[..colon];
    let task_data = split_statement_suffix(&task_stmt[colon + 1..]);
    if task_txt.is_empty() || task_data.trim().is_empty() {
        return Ok(Ok(()));
    }
    let leading = stripped.len().saturating_sub(task_stmt.len());
    let statement_span =
        SourceSpan::new(line_start + leading, line_start + leading + task_stmt.len());
    let fields = split_gantt_fields(task_data, line_start + leading + colon + 1);
    let editor_fields = fields
        .iter()
        .copied()
        .filter_map(SpannedText::trim)
        .collect::<Vec<_>>();
    collect_gantt_task_field_symbols(&editor_fields, statement_span, facts);
    let field_text = fields.iter().map(|field| field.text).collect::<Vec<_>>();
    let task_info = db.parse_task_info(&field_text);
    db.add_task(task_txt, &format!(":{task_data}"), task_info);
    Ok(Ok(()))
}
