use crate::diagrams::scan::{LineCursor, leading_whitespace_len, starts_with_case_insensitive};
use crate::{
    EditorExpectedSyntax, EditorExpectedSyntaxKind, EditorSemanticFacts, EditorSemanticKind,
    EditorSemanticSymbol, Error, OperationControl, OperationControlResult, ParseMetadata, Result,
    SourceSpan,
};
use serde_json::{Value, json};
#[cfg(test)]
use std::cell::Cell;

#[cfg(test)]
thread_local! {
    static TIMELINE_SYNTAX_CONSTRUCTION_COUNT: Cell<usize> = const { Cell::new(0) };
}

#[cfg(test)]
fn reset_timeline_syntax_construction_count() {
    TIMELINE_SYNTAX_CONSTRUCTION_COUNT.set(0);
}

#[cfg(test)]
fn timeline_syntax_construction_count() -> usize {
    TIMELINE_SYNTAX_CONSTRUCTION_COUNT.get()
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct TimelineRenderTask {
    pub id: i64,
    pub section: String,
    /// Index of the authored section occurrence in `TimelineDiagramRenderModel::sections`.
    ///
    /// `None` represents a task without parser-backed occurrence ownership, including tasks
    /// authored before any section and legacy direct models.
    #[serde(
        default,
        rename = "sectionIndex",
        skip_serializing_if = "Option::is_none"
    )]
    pub section_index: Option<usize>,
    #[serde(rename = "type")]
    pub task_type: String,
    pub task: String,
    pub score: i64,
    #[serde(default)]
    pub events: Vec<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, serde::Serialize, serde::Deserialize)]
pub enum TimelineDirection {
    #[default]
    #[serde(rename = "LR")]
    LeftToRight,
    #[serde(rename = "TD")]
    TopDown,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, Default)]
pub struct TimelineDiagramRenderModel {
    #[serde(default)]
    pub direction: TimelineDirection,
    pub title: Option<String>,
    #[serde(rename = "accTitle")]
    pub acc_title: Option<String>,
    #[serde(rename = "accDescr")]
    pub acc_descr: Option<String>,
    #[serde(default)]
    pub sections: Vec<String>,
    #[serde(default)]
    pub tasks: Vec<TimelineRenderTask>,
    #[serde(skip)]
    compatibility_output: CompatibilityOutputState,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
enum CompatibilityOutputState {
    Empty,
    #[default]
    Model,
}

impl TimelineDiagramRenderModel {
    fn empty_compatibility_output() -> Self {
        Self {
            compatibility_output: CompatibilityOutputState::Empty,
            ..Self::default()
        }
    }

    pub(crate) fn sanitize_common_db_fields(&mut self, config: &crate::MermaidConfig) {
        crate::common_db::sanitize_optional_title(&mut self.title, config);
        crate::common_db::sanitize_optional_acc_title(&mut self.acc_title, config);
        crate::common_db::sanitize_optional_acc_descr(&mut self.acc_descr, config);
    }
}

#[derive(Debug, Default)]
struct TimelineDb {
    direction: TimelineDirection,
    title: String,
    acc_title: String,
    acc_descr: String,

    current_section: String,
    current_section_index: Option<usize>,
    sections: Vec<String>,
    tasks: Vec<TimelineRenderTask>,
    next_id: i64,
}

impl TimelineDb {
    fn add_section(&mut self, txt: &str) {
        self.current_section = txt.to_string();
        self.current_section_index = Some(self.sections.len());
        self.sections.push(txt.to_string());
    }

    fn add_task(&mut self, period: &str) {
        let id = self.next_id;
        self.next_id += 1;
        self.tasks.push(TimelineRenderTask {
            id,
            section: self.current_section.clone(),
            section_index: self.current_section_index,
            task_type: self.current_section.clone(),
            task: period.to_string(),
            score: 0,
            events: Vec::new(),
        });
    }

    fn add_event(&mut self, event: &str) -> Result<()> {
        let Some(last) = self.tasks.last_mut() else {
            return Err(Error::diagram_parse_fallback(
                "timeline".to_string(),
                "event without a preceding task".to_string(),
            ));
        };
        last.events.push(event.to_string());
        Ok(())
    }
}

struct TimelineSemanticSource {
    model: Option<TimelineDiagramRenderModel>,
    editor_facts: EditorSemanticFacts,
}

struct TimelineParseIssue {
    error: Error,
    span: SourceSpan,
}

struct TimelineParseOutcome {
    source: TimelineSemanticSource,
    issues: Vec<TimelineParseIssue>,
}

impl TimelineParseOutcome {
    fn into_strict_source(self) -> Result<TimelineSemanticSource> {
        if let Some(issue) = self.issues.into_iter().next() {
            return Err(issue.error);
        }
        Ok(self.source)
    }

    fn into_combined(mut self, meta: &ParseMetadata) -> crate::family::CombinedSemanticParse {
        let mut first_error = None;
        for issue in self.issues {
            self.source.editor_facts.mark_recovered_from_parse_error(
                format!(
                    "timeline parser recovered after parse error: {}",
                    issue.error
                ),
                Some(issue.span),
            );
            if first_error.is_none() {
                first_error = Some(issue.error);
            }
        }
        let construction = match first_error {
            Some(error) => Err(crate::family::CombinedSemanticFailure::new(
                error,
                self.source.editor_facts,
            )),
            None => Ok(self.source),
        };
        crate::family::CombinedSemanticParse::from_construction(
            construction,
            |source| {
                let model = match source.model {
                    Some(model) => render_model_to_compat_json(&model, meta),
                    None => Ok(json!({})),
                };
                (model, source.editor_facts)
            },
            crate::family::CombinedSemanticFailure::into_parts,
        )
    }
}

#[derive(Debug, Clone, Copy)]
struct TimelineSource<'source> {
    text: &'source str,
    start: usize,
}

impl<'source> TimelineSource<'source> {
    fn new(text: &'source str, start: usize) -> Self {
        Self { text, start }
    }

    fn end(self) -> usize {
        self.start + self.text.len()
    }

    fn span(self) -> SourceSpan {
        SourceSpan::new(self.start, self.end())
    }

    fn subslice(self, start: usize, end: usize) -> Self {
        debug_assert!(start <= end);
        debug_assert!(end <= self.text.len());
        Self::new(&self.text[start..end], self.start + start)
    }

    fn trim_start(self) -> Self {
        let leading = leading_whitespace_len(self.text);
        self.subslice(leading, self.text.len())
    }

    fn trim_end(self) -> Self {
        let text = self.text.trim_end();
        self.subslice(0, text.len())
    }

    fn trim(self) -> Self {
        self.trim_start().trim_end()
    }
}

#[derive(Debug, Clone, Copy)]
struct ParsedSpaceDirective<'source> {
    value: TimelineSource<'source>,
}

#[derive(Debug, Clone, Copy)]
struct ParsedColonDirective<'source> {
    value: TimelineSource<'source>,
}

#[derive(Debug, Clone, Copy)]
struct ParsedSection<'source> {
    db_value: TimelineSource<'source>,
    value: TimelineSource<'source>,
}

struct ParsedAccDescrBlock {
    keyword: SourceSpan,
    closing: Option<SourceSpan>,
    text: String,
    span: SourceSpan,
}

#[derive(Debug, Clone, Copy)]
struct ParsedTimelineEvent<'source> {
    value: TimelineSource<'source>,
}

struct ParsedTimelineEvents<'source> {
    events: Vec<ParsedTimelineEvent<'source>>,
    issue: Option<TimelineParseIssue>,
}

#[derive(Debug, Clone, Copy)]
enum TimelinePeriodSuffix<'source> {
    None,
    HashComment,
    Events(TimelineSource<'source>),
}

#[derive(Debug, Clone, Copy)]
struct ParsedTimelinePeriod<'source> {
    period: TimelineSource<'source>,
    task: TimelineSource<'source>,
    suffix: TimelinePeriodSuffix<'source>,
}

fn parse_keyword_prefix<'source>(
    source: TimelineSource<'source>,
    keyword: &str,
) -> Option<(TimelineSource<'source>, TimelineSource<'source>)> {
    if !starts_with_case_insensitive(source.text, keyword) {
        return None;
    }
    Some((
        source.subslice(0, keyword.len()),
        source.subslice(keyword.len(), source.text.len()),
    ))
}

fn parse_space_directive<'source>(
    line: TimelineSource<'source>,
    keyword: &str,
) -> Option<ParsedSpaceDirective<'source>> {
    let line = line.trim_start();
    let (_, after_keyword) = parse_keyword_prefix(line, keyword)?;
    let whitespace = after_keyword.text.chars().next()?;
    if !whitespace.is_whitespace() {
        return None;
    }
    Some(ParsedSpaceDirective {
        value: after_keyword.subslice(whitespace.len_utf8(), after_keyword.text.len()),
    })
}

fn parse_colon_directive<'source>(
    line: TimelineSource<'source>,
    keyword: &str,
) -> Option<ParsedColonDirective<'source>> {
    let line = line.trim_start();
    let (_, after_keyword) = parse_keyword_prefix(line, keyword)?;
    let after_keyword = after_keyword.trim_start();
    let after_colon = after_keyword.text.strip_prefix(':')?;
    let after_colon = TimelineSource::new(after_colon, after_keyword.start + 1).trim_start();

    let terminator = after_colon
        .text
        .char_indices()
        .find(|(_, ch)| matches!(ch, '#' | ';'));
    let value_source = match terminator {
        Some((offset, '#')) | Some((offset, ';')) => after_colon.subslice(0, offset).trim(),
        Some(_) => unreachable!("Timeline terminator is closed"),
        None => after_colon.trim(),
    };

    Some(ParsedColonDirective {
        value: value_source,
    })
}

fn parse_section<'source>(line: TimelineSource<'source>) -> Option<ParsedSection<'source>> {
    let parsed = parse_space_directive(line, "section")?;
    let colon = parsed.value.text.find(':');
    let db_value = match colon {
        Some(offset) => parsed.value.subslice(0, offset),
        None => parsed.value,
    };
    Some(ParsedSection {
        db_value,
        value: db_value.trim(),
    })
}

fn parse_acc_descr_block(
    cursor: &mut LineCursor<'_>,
    first_line: TimelineSource<'_>,
    control: &OperationControl,
) -> OperationControlResult<Option<ParsedAccDescrBlock>> {
    control.checkpoint()?;
    let line = first_line.trim_start();
    let Some((keyword, after_keyword)) = parse_keyword_prefix(line, "accDescr") else {
        return Ok(None);
    };
    let after_keyword = after_keyword.trim_start();
    let Some(rest) = after_keyword.text.strip_prefix('{') else {
        return Ok(None);
    };
    let mut rest = TimelineSource::new(rest, after_keyword.start + 1);
    let mut content_spans = Vec::new();
    let mut text = String::new();

    if let Some(close) = rest.text.find('}') {
        let content = rest.subslice(0, close);
        let trimmed = content.trim();
        if !trimmed.text.is_empty() {
            content_spans.push(trimmed.span());
        }
        text.push_str(content.text);
        let closing = SourceSpan::new(rest.start + close, rest.start + close + 1);
        cursor.resume_same_line_at(closing.end);
        let span = content_spans
            .first()
            .zip(content_spans.last())
            .map_or(SourceSpan::new(rest.start, rest.start), |(first, last)| {
                SourceSpan::new(first.start, last.end)
            });
        return Ok(Some(ParsedAccDescrBlock {
            keyword: keyword.span(),
            closing: Some(closing),
            text: text.trim().to_string(),
            span,
        }));
    }

    let first = rest.trim();
    if !first.text.is_empty() {
        content_spans.push(first.span());
    }
    text.push_str(rest.text);
    text.push('\n');
    let mut closing = None;
    let empty_span = SourceSpan::new(rest.start, rest.start);

    while let Some((line, line_start)) = cursor.next_line() {
        control.checkpoint()?;
        rest = TimelineSource::new(line, line_start);
        if let Some(close) = rest.text.find('}') {
            let content = rest.subslice(0, close);
            let trimmed = content.trim();
            if !trimmed.text.is_empty() {
                content_spans.push(trimmed.span());
            }
            text.push_str(content.text);
            closing = Some(SourceSpan::new(rest.start + close, rest.start + close + 1));
            cursor.resume_same_line_at(rest.start + close + 1);
            break;
        }
        let trimmed = rest.trim();
        if !trimmed.text.is_empty() {
            content_spans.push(trimmed.span());
        }
        text.push_str(rest.text);
        text.push('\n');
    }

    let span = content_spans
        .first()
        .zip(content_spans.last())
        .map_or(empty_span, |(first, last)| {
            SourceSpan::new(first.start, last.end)
        });
    Ok(Some(ParsedAccDescrBlock {
        keyword: keyword.span(),
        closing,
        text: text.trim().to_string(),
        span,
    }))
}

fn parse_timeline_events<'source>(
    input: TimelineSource<'source>,
    diagram_type: &str,
) -> ParsedTimelineEvents<'source> {
    let original = input;
    let mut source = input;
    let mut events = Vec::new();

    while !source.text.is_empty() {
        if !source.text.starts_with(':') {
            return ParsedTimelineEvents {
                events,
                issue: Some(timeline_parse_issue(
                    Error::diagram_parse_exact(
                        diagram_type.to_string(),
                        format!("invalid event token: {}", original.text),
                        source.span(),
                    ),
                    source.span(),
                )),
            };
        }
        let after_colon = source.subslice(1, source.text.len());
        let Some(whitespace) = after_colon.text.chars().next() else {
            let insertion = SourceSpan::new(after_colon.start, after_colon.start);
            return ParsedTimelineEvents {
                events,
                issue: Some(timeline_parse_issue(
                    Error::diagram_parse_insertion_point(
                        diagram_type.to_string(),
                        "invalid event token: missing whitespace after ':'",
                        after_colon.start,
                    ),
                    insertion,
                )),
            };
        };
        if !whitespace.is_whitespace() {
            let insertion = SourceSpan::new(after_colon.start, after_colon.start);
            return ParsedTimelineEvents {
                events,
                issue: Some(timeline_parse_issue(
                    Error::diagram_parse_insertion_point(
                        diagram_type.to_string(),
                        "invalid event token: missing whitespace after ':'",
                        after_colon.start,
                    ),
                    insertion,
                )),
            };
        }

        source = after_colon.subslice(whitespace.len_utf8(), after_colon.text.len());
        if source.text.is_empty() {
            let insertion = SourceSpan::new(source.start, source.start);
            return ParsedTimelineEvents {
                events,
                issue: Some(timeline_parse_issue(
                    Error::diagram_parse_insertion_point(
                        diagram_type.to_string(),
                        "invalid event token: expected event text",
                        source.start,
                    ),
                    insertion,
                )),
            };
        }

        let boundary = source.text.char_indices().find_map(|(offset, ch)| {
            if ch != ':' {
                return None;
            }
            source.text[offset + 1..]
                .chars()
                .next()
                .is_some_and(char::is_whitespace)
                .then_some(offset)
        });
        let (value, rest) = match boundary {
            Some(offset) => (
                source.subslice(0, offset),
                source.subslice(offset, source.text.len()),
            ),
            None => (source, TimelineSource::new("", source.end())),
        };
        events.push(ParsedTimelineEvent { value });
        source = rest;
    }

    ParsedTimelineEvents {
        events,
        issue: None,
    }
}

fn parse_timeline_period(line: TimelineSource<'_>) -> ParsedTimelinePeriod<'_> {
    let boundary = line
        .text
        .char_indices()
        .find(|(_, ch)| matches!(ch, ':' | '#'));
    let (period, suffix) = match boundary {
        Some((offset, ':')) => (
            line.subslice(0, offset),
            TimelinePeriodSuffix::Events(line.subslice(offset, line.text.len())),
        ),
        Some((offset, '#')) => (line.subslice(0, offset), TimelinePeriodSuffix::HashComment),
        Some(_) => unreachable!("Timeline period boundary is closed"),
        None => (line, TimelinePeriodSuffix::None),
    };
    ParsedTimelinePeriod {
        period,
        task: period.trim(),
        suffix,
    }
}

fn push_timeline_payload_fact(
    facts: &mut EditorSemanticFacts,
    text: &str,
    span: SourceSpan,
    detail: &'static str,
    kind: EditorSemanticKind,
) {
    facts.push_expected_syntax(EditorExpectedSyntax::new(
        EditorExpectedSyntaxKind::Payload,
        span,
    ));
    if text.is_empty() {
        return;
    }
    facts.push_symbol(EditorSemanticSymbol::payload(
        text.to_string(),
        Some(detail.to_string()),
        kind,
        span,
        span,
    ));
}

fn push_timeline_outline_fact(
    facts: &mut EditorSemanticFacts,
    text: &str,
    statement_span: SourceSpan,
    selection: SourceSpan,
    detail: &'static str,
    kind: EditorSemanticKind,
) {
    facts.push_expected_syntax(EditorExpectedSyntax::new(
        EditorExpectedSyntaxKind::Payload,
        selection,
    ));
    if text.is_empty() {
        return;
    }
    facts.push_symbol(EditorSemanticSymbol::outline(
        text.to_string(),
        Some(detail.to_string()),
        kind,
        statement_span,
        selection,
    ));
}

fn timeline_parse_issue(error: Error, fallback: SourceSpan) -> TimelineParseIssue {
    let span = match &error {
        Error::DiagramParse { diagnostic, .. } => diagnostic.span().unwrap_or(fallback),
        _ => fallback,
    };
    TimelineParseIssue { error, span }
}

pub(crate) fn parse_timeline(code: &str, meta: &ParseMetadata) -> Result<Value> {
    let source = construct_timeline_semantic_source(code, meta).into_strict_source()?;
    match source.model {
        Some(model) => render_model_to_compat_json(&model, meta),
        None => Ok(json!({})),
    }
}

pub(crate) fn parse_timeline_json_and_editor_facts(
    code: &str,
    meta: &ParseMetadata,
    control: &OperationControl,
) -> OperationControlResult<crate::family::CombinedSemanticParse> {
    Ok(construct_timeline_semantic_source_controlled(code, meta, control)?.into_combined(meta))
}

pub(crate) fn render_model_to_compat_json(
    model: &TimelineDiagramRenderModel,
    meta: &ParseMetadata,
) -> Result<Value> {
    if model.compatibility_output == CompatibilityOutputState::Empty {
        return Ok(json!({}));
    }
    let mut tasks =
        serde_json::to_value(&model.tasks).expect("Timeline tasks must remain JSON-serializable");
    for task in tasks
        .as_array_mut()
        .expect("Timeline tasks must serialize to an array")
    {
        if let Some(task) = task.as_object_mut() {
            task.remove("sectionIndex");
        }
    }
    Ok(json!({
        "type": meta.diagram_type,
        "direction": model.direction,
        "title": &model.title,
        "accTitle": &model.acc_title,
        "accDescr": &model.acc_descr,
        "sections": &model.sections,
        "tasks": tasks,
    }))
}

#[cfg(test)]
pub(crate) fn parse_timeline_model_for_render(
    code: &str,
    meta: &ParseMetadata,
) -> Result<TimelineDiagramRenderModel> {
    construct_timeline_semantic_source(code, meta)
        .into_strict_source()
        .map(|source| {
            source
                .model
                .unwrap_or_else(TimelineDiagramRenderModel::empty_compatibility_output)
        })
}

pub(crate) fn parse_timeline_model_for_render_controlled(
    code: &str,
    meta: &ParseMetadata,
    control: &OperationControl,
) -> OperationControlResult<Result<TimelineDiagramRenderModel>> {
    let source =
        construct_timeline_semantic_source_controlled(code, meta, control)?.into_strict_source();
    let source = match source {
        Ok(source) => source,
        Err(error) => return Ok(Err(error)),
    };
    control.checkpoint()?;
    Ok(Ok(source.model.unwrap_or_else(
        TimelineDiagramRenderModel::empty_compatibility_output,
    )))
}

fn construct_timeline_semantic_source(code: &str, meta: &ParseMetadata) -> TimelineParseOutcome {
    construct_timeline_semantic_source_controlled(code, meta, &OperationControl::new())
        .expect("a private parse control cannot be cancelled")
}

fn construct_timeline_semantic_source_controlled(
    code: &str,
    meta: &ParseMetadata,
    control: &OperationControl,
) -> OperationControlResult<TimelineParseOutcome> {
    control.checkpoint()?;
    #[cfg(test)]
    TIMELINE_SYNTAX_CONSTRUCTION_COUNT.set(TIMELINE_SYNTAX_CONSTRUCTION_COUNT.get() + 1);

    parse_timeline_semantic_source(code, meta, control)
}

fn parse_timeline_semantic_source(
    code: &str,
    meta: &ParseMetadata,
    control: &OperationControl,
) -> OperationControlResult<TimelineParseOutcome> {
    control.checkpoint()?;
    let mut db = TimelineDb::default();
    let mut editor_facts = EditorSemanticFacts::new();
    let mut issues = Vec::new();
    let mut lines = LineCursor::new(code);
    let mut header_seen = false;

    while let Some((line, line_start)) = lines.next_line() {
        control.checkpoint()?;
        let line = TimelineSource::new(line, line_start);
        let significant = line.trim();
        if significant.text.is_empty() {
            continue;
        }

        if significant.text.starts_with("%%") {
            continue;
        }
        if significant.text.starts_with('#') {
            continue;
        }

        if !header_seen {
            let Some((_, after_keyword)) = parse_keyword_prefix(significant, "timeline") else {
                issues.push(timeline_parse_issue(
                    Error::diagram_parse_exact(
                        meta.diagram_type.clone(),
                        "expected timeline header",
                        significant.span(),
                    ),
                    significant.span(),
                ));
                continue;
            };

            header_seen = true;
            if after_keyword.text.is_empty() {
                continue;
            }
            if after_keyword.text.starts_with('#') {
                continue;
            }
            if after_keyword.text.starts_with("%%") {
                continue;
            }

            let had_whitespace = after_keyword
                .text
                .chars()
                .next()
                .is_some_and(char::is_whitespace);
            let rest = after_keyword.trim_start();
            if had_whitespace && rest.text.starts_with('#') {
                continue;
            }
            if had_whitespace && rest.text.starts_with("%%") {
                continue;
            }
            let direction_end = rest
                .text
                .char_indices()
                .find_map(|(offset, ch)| ch.is_whitespace().then_some(offset))
                .unwrap_or(rest.text.len());
            let direction = rest.subslice(0, direction_end);
            let trailing = rest.subslice(direction_end, rest.text.len()).trim_start();
            let valid_direction = had_whitespace
                && (direction.text.eq_ignore_ascii_case("LR")
                    || direction.text.eq_ignore_ascii_case("TD"));
            if valid_direction {
                db.direction = if direction.text.eq_ignore_ascii_case("TD") {
                    TimelineDirection::TopDown
                } else {
                    TimelineDirection::LeftToRight
                };
                if trailing.text.starts_with('#') {
                    continue;
                }
                if trailing.text.is_empty() || trailing.text.starts_with("%%") {
                    continue;
                }
            }

            let invalid = if valid_direction { trailing } else { rest };
            issues.push(timeline_parse_issue(
                Error::diagram_parse_exact(
                    meta.diagram_type.clone(),
                    "unexpected content after timeline header",
                    invalid.span(),
                ),
                invalid.span(),
            ));
            continue;
        }

        let statement = line.trim_start();
        if let Some(parsed) = parse_space_directive(statement, "title") {
            editor_facts.push_directive_prefix("title");
            push_timeline_payload_fact(
                &mut editor_facts,
                parsed.value.text,
                parsed.value.span(),
                "timeline title",
                EditorSemanticKind::String,
            );
            db.title = parsed.value.text.to_string();
            continue;
        }

        if let Some(parsed) = parse_colon_directive(statement, "accTitle") {
            editor_facts.push_directive_prefix("accTitle");
            push_timeline_payload_fact(
                &mut editor_facts,
                parsed.value.text,
                parsed.value.span(),
                "timeline accessibility title",
                EditorSemanticKind::String,
            );
            db.acc_title = parsed.value.text.to_string();
            continue;
        }

        if let Some(parsed) = parse_colon_directive(statement, "accDescr") {
            editor_facts.push_directive_prefix("accDescr");
            push_timeline_payload_fact(
                &mut editor_facts,
                parsed.value.text,
                parsed.value.span(),
                "timeline accessibility description",
                EditorSemanticKind::String,
            );
            db.acc_descr = parsed.value.text.to_string();
            continue;
        }

        if let Some(parsed) = parse_acc_descr_block(&mut lines, line, control)? {
            editor_facts.push_directive_prefix("accDescr");
            push_timeline_payload_fact(
                &mut editor_facts,
                &parsed.text,
                parsed.span,
                "timeline accessibility description",
                EditorSemanticKind::String,
            );
            if parsed.closing.is_none() {
                let span = SourceSpan::new(parsed.keyword.start, code.len());
                issues.push(timeline_parse_issue(
                    Error::diagram_parse_insertion_point(
                        meta.diagram_type.clone(),
                        "unterminated accDescr block",
                        code.len(),
                    ),
                    span,
                ));
            }
            db.acc_descr = parsed.text;
            continue;
        }

        if let Some(parsed) = parse_section(statement) {
            push_timeline_outline_fact(
                &mut editor_facts,
                parsed.value.text,
                statement.span(),
                parsed.value.span(),
                "timeline section",
                EditorSemanticKind::Namespace,
            );
            db.add_section(parsed.db_value.text);
            continue;
        }

        if statement.text.starts_with(':') {
            let parsed = parse_timeline_events(statement, &meta.diagram_type);
            for event in parsed.events {
                match db.add_event(event.value.text) {
                    Ok(()) => push_timeline_payload_fact(
                        &mut editor_facts,
                        event.value.text,
                        event.value.span(),
                        "timeline event",
                        EditorSemanticKind::String,
                    ),
                    Err(error) => issues.push(timeline_parse_issue(error, event.value.span())),
                }
            }
            if let Some(issue) = parsed.issue {
                issues.push(issue);
            }
            continue;
        }

        let parsed = parse_timeline_period(statement);
        if parsed.task.text.is_empty() {
            continue;
        }

        push_timeline_outline_fact(
            &mut editor_facts,
            parsed.task.text,
            statement.span(),
            parsed.task.span(),
            "timeline task",
            EditorSemanticKind::Event,
        );
        db.add_task(parsed.period.text);

        match parsed.suffix {
            TimelinePeriodSuffix::None => {}
            TimelinePeriodSuffix::HashComment => {}
            TimelinePeriodSuffix::Events(events_source) => {
                let events = parse_timeline_events(events_source, &meta.diagram_type);
                for event in events.events {
                    match db.add_event(event.value.text) {
                        Ok(()) => push_timeline_payload_fact(
                            &mut editor_facts,
                            event.value.text,
                            event.value.span(),
                            "timeline event",
                            EditorSemanticKind::String,
                        ),
                        Err(error) => issues.push(timeline_parse_issue(error, event.value.span())),
                    }
                }
                if let Some(issue) = events.issue {
                    issues.push(issue);
                }
            }
        }
    }

    control.checkpoint()?;
    let model = header_seen.then(|| TimelineDiagramRenderModel {
        direction: db.direction,
        title: (!db.title.is_empty()).then_some(db.title),
        acc_title: (!db.acc_title.is_empty()).then_some(db.acc_title),
        acc_descr: (!db.acc_descr.is_empty()).then_some(db.acc_descr),
        sections: db.sections,
        tasks: db.tasks,
        compatibility_output: CompatibilityOutputState::Model,
    });
    Ok(TimelineParseOutcome {
        source: TimelineSemanticSource {
            model,
            editor_facts,
        },
        issues,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        EditorExpectedSyntaxKind, EditorSemanticCompleteness, EditorSemanticDiagnosticKind,
        EditorSemanticKind, EditorSemanticRole, Engine, ParseDiagnosticSpanKind, ParseOptions,
        SourceSpan,
    };
    use futures::executor::block_on;

    fn parse(text: &str) -> crate::ManagedSemanticJson {
        let engine = Engine::new();
        block_on(engine.parse_diagram(text, ParseOptions::default()))
            .unwrap()
            .unwrap()
            .model
    }

    #[test]
    fn timeline_simple_section_definition() {
        let model = parse(
            r#"
timeline
section abc-123
"#,
        );
        assert_eq!(model["sections"][0].as_str().unwrap(), "abc-123");
    }

    #[test]
    fn timeline_section_with_two_tasks() {
        let model = parse(
            r#"
timeline
section abc-123
task1
task2
"#,
        );
        let tasks = model["tasks"].as_array().unwrap();
        assert_eq!(tasks.len(), 2);
        for task in tasks {
            assert_eq!(task["section"].as_str().unwrap(), "abc-123");
            assert!(matches!(task["task"].as_str().unwrap(), "task1" | "task2"));
        }
    }

    #[test]
    fn timeline_two_sections_and_two_tasks_each() {
        let model = parse(
            r#"
timeline
section abc-123
task1
task2
section abc-456
task3
task4
"#,
        );
        assert_eq!(
            model["sections"]
                .as_array()
                .unwrap()
                .iter()
                .map(|v| v.as_str().unwrap().to_string())
                .collect::<Vec<_>>(),
            vec!["abc-123".to_string(), "abc-456".to_string()]
        );

        let tasks = model["tasks"].as_array().unwrap();
        assert_eq!(tasks.len(), 4);
        for t in tasks {
            let section = t["section"].as_str().unwrap();
            let task = t["task"].as_str().unwrap().trim();
            assert!(matches!(section, "abc-123" | "abc-456"));
            assert!(matches!(task, "task1" | "task2" | "task3" | "task4"));
            if section == "abc-123" {
                assert!(matches!(task, "task1" | "task2"));
            } else {
                assert!(matches!(task, "task3" | "task4"));
            }
        }
    }

    #[test]
    fn timeline_typed_tasks_retain_duplicate_section_occurrences() {
        let source = concat!(
            "timeline\n",
            "section Repeated\n",
            "2000: First\n",
            "section Repeated\n",
            "2001: Second\n",
            "2002: Third\n",
        );
        let parsed = Engine::new()
            .parse_diagram_for_render_model_sync(source, ParseOptions::strict())
            .expect("duplicate Timeline sections should parse")
            .expect("Timeline source should be detected");
        let crate::diagram::RenderSemanticModel::Timeline(model) = parsed.model() else {
            panic!("expected a Timeline render model");
        };

        assert_eq!(
            model.sections,
            vec!["Repeated".to_string(), "Repeated".to_string()]
        );
        assert_eq!(
            model
                .tasks
                .iter()
                .map(|task| task.section_index)
                .collect::<Vec<_>>(),
            [Some(0), Some(1), Some(1)]
        );
        assert_eq!(
            serde_json::to_value(&model.tasks[0]).expect("typed task should serialize")["sectionIndex"],
            json!(0)
        );

        let compat = render_model_to_compat_json(model, parsed.metadata())
            .expect("Timeline compatibility projection should serialize");
        assert!(compat["tasks"][0].get("sectionIndex").is_none());
    }

    #[test]
    fn timeline_tasks_and_events() {
        let model = parse(
            r#"
timeline
section abc-123
task1: event1
task2: event2: event3
"#,
        );
        let tasks = model["tasks"].as_array().unwrap();
        assert_eq!(tasks.len(), 2);
        for t in tasks {
            let task = t["task"].as_str().unwrap().trim();
            match task {
                "task1" => {
                    assert_eq!(
                        t["events"]
                            .as_array()
                            .unwrap()
                            .iter()
                            .map(|v| v.as_str().unwrap().to_string())
                            .collect::<Vec<_>>(),
                        vec!["event1".to_string()]
                    );
                }
                "task2" => {
                    assert_eq!(
                        t["events"]
                            .as_array()
                            .unwrap()
                            .iter()
                            .map(|v| v.as_str().unwrap().to_string())
                            .collect::<Vec<_>>(),
                        vec!["event2".to_string(), "event3".to_string()]
                    );
                }
                _ => panic!("unexpected task: {task}"),
            }
        }
    }

    #[test]
    fn timeline_events_support_markdown_link() {
        let model = parse(
            r#"
timeline
section abc-123
task1: [event1](http://example.com)
task2: event2: event3
"#,
        );
        let tasks = model["tasks"].as_array().unwrap();
        assert_eq!(tasks.len(), 2);
        for t in tasks {
            let task = t["task"].as_str().unwrap().trim();
            match task {
                "task1" => {
                    assert_eq!(
                        t["events"]
                            .as_array()
                            .unwrap()
                            .iter()
                            .map(|v| v.as_str().unwrap().to_string())
                            .collect::<Vec<_>>(),
                        vec!["[event1](http://example.com)".to_string()]
                    );
                }
                "task2" => {}
                _ => panic!("unexpected task: {task}"),
            }
        }
    }

    #[test]
    fn timeline_multiline_events_are_attached_to_previous_task() {
        let model = parse(
            r#"
timeline
section abc-123
task1: event1
task2: event2: event3
     : event4: event5
"#,
        );
        let tasks = model["tasks"].as_array().unwrap();
        assert_eq!(tasks.len(), 2);
        for t in tasks {
            let task = t["task"].as_str().unwrap().trim();
            match task {
                "task1" => {
                    assert_eq!(t["events"].as_array().unwrap().len(), 1);
                }
                "task2" => {
                    assert_eq!(
                        t["events"]
                            .as_array()
                            .unwrap()
                            .iter()
                            .map(|v| v.as_str().unwrap().to_string())
                            .collect::<Vec<_>>(),
                        vec![
                            "event2".to_string(),
                            "event3".to_string(),
                            "event4".to_string(),
                            "event5".to_string()
                        ]
                    );
                }
                _ => panic!("unexpected task: {task}"),
            }
        }
    }

    #[test]
    fn timeline_allows_semicolons_in_title_section_and_events() {
        let model = parse(
            r#"
timeline
title ;my;title;
section ;a;bc-123;
;ta;sk1;: ;ev;ent1; : ;ev;ent2; : ;ev;ent3;
"#,
        );
        assert_eq!(model["title"].as_str().unwrap(), ";my;title;");
        assert_eq!(model["sections"][0].as_str().unwrap(), ";a;bc-123;");

        let tasks = model["tasks"].as_array().unwrap();
        assert_eq!(tasks.len(), 1);
        let events = tasks[0]["events"]
            .as_array()
            .unwrap()
            .iter()
            .map(|v| v.as_str().unwrap().to_string())
            .collect::<Vec<_>>();
        assert_eq!(
            events,
            vec![
                ";ev;ent1; ".to_string(),
                ";ev;ent2; ".to_string(),
                ";ev;ent3;".to_string()
            ]
        );
    }

    #[test]
    fn timeline_allows_hashtags_in_title_section_and_events() {
        let model = parse(
            r#"
timeline
title #my#title#
section #a#bc-123#
task1: #ev#ent1# : #ev#ent2# : #ev#ent3#
"#,
        );
        assert_eq!(model["title"].as_str().unwrap(), "#my#title#");
        assert_eq!(model["sections"][0].as_str().unwrap(), "#a#bc-123#");

        let tasks = model["tasks"].as_array().unwrap();
        assert_eq!(tasks.len(), 1);
        assert_eq!(tasks[0]["task"].as_str().unwrap(), "task1");
        let events = tasks[0]["events"]
            .as_array()
            .unwrap()
            .iter()
            .map(|v| v.as_str().unwrap().to_string())
            .collect::<Vec<_>>();
        assert_eq!(
            events,
            vec![
                "#ev#ent1# ".to_string(),
                "#ev#ent2# ".to_string(),
                "#ev#ent3#".to_string()
            ]
        );
    }

    #[test]
    fn timeline_event_missing_space_reports_insertion_point() {
        let engine = Engine::new();
        let text = "timeline\ntask1:event1\n";
        let err = block_on(engine.parse_diagram(text, ParseOptions::default())).unwrap_err();
        let Error::DiagramParse { diagnostic, .. } = err else {
            panic!("expected timeline parse error");
        };

        let colon = text.find(':').unwrap();
        assert_eq!(
            diagnostic.message(),
            "invalid event token: missing whitespace after ':'"
        );
        assert_eq!(
            diagnostic.span(),
            Some(SourceSpan::new(colon + 1, colon + 1))
        );
        assert_eq!(
            diagnostic.span_kind(),
            ParseDiagnosticSpanKind::InsertionPoint
        );
    }

    #[test]
    fn timeline_editor_facts_expose_parser_backed_spans() {
        let engine = Engine::new();
        let text = "timeline\n\
title My timeline\n\
accTitle: My acc title\n\
accDescr: My acc descr\n\
section alpha\n\
task1: event1\n\
task2: event2: event3\n";
        let facts = engine
            .parse_editor_semantic_facts_with_type_sync("timeline", text)
            .unwrap()
            .unwrap();

        assert!(facts.directive_prefixes.iter().any(|p| p == "title"));
        assert!(facts.directive_prefixes.iter().any(|p| p == "accTitle"));
        assert!(facts.directive_prefixes.iter().any(|p| p == "accDescr"));
        assert!(facts.symbols.iter().any(|symbol| {
            symbol.name == "alpha"
                && symbol.kind == EditorSemanticKind::Namespace
                && symbol.role == EditorSemanticRole::Outline
        }));
        assert!(facts.symbols.iter().any(|symbol| {
            symbol.name == "task1"
                && symbol.kind == EditorSemanticKind::Event
                && symbol.role == EditorSemanticRole::Outline
        }));

        let task_start = text.find("task1").unwrap();
        let event_start = text.find("event1").unwrap();

        assert!(facts.expected_syntax.iter().any(|expected| {
            expected.kind == EditorExpectedSyntaxKind::Payload
                && expected.span == SourceSpan::new(task_start, task_start + "task1".len())
        }));
        assert!(!facts.expected_syntax.iter().any(|expected| {
            expected.kind == EditorExpectedSyntaxKind::NodeIdentifier
                && expected.span == SourceSpan::new(task_start, task_start + "task1".len())
        }));
        assert!(facts.expected_syntax.iter().any(|expected| {
            expected.kind == EditorExpectedSyntaxKind::Payload
                && expected.span == SourceSpan::new(event_start, event_start + "event1".len())
        }));
    }

    #[test]
    fn timeline_entrypoints_and_combined_projection_construct_once() {
        let engine = Engine::new();
        let text = concat!(
            "timeline\n",
            "title Delivery\n",
            "accTitle: Delivery timeline\n",
            "section Build\n",
            "Implement parser: API ready: Tests green\n",
        );
        let parsed = engine
            .parse_diagram_sync(text, ParseOptions::strict())
            .expect("standalone Timeline JSON parse succeeds")
            .expect("standalone Timeline JSON parse returns a diagram");
        reset_timeline_syntax_construction_count();
        parse_timeline(text, &parsed.meta).expect("Timeline JSON projection succeeds");
        assert_eq!(timeline_syntax_construction_count(), 1);

        reset_timeline_syntax_construction_count();
        let typed = parse_timeline_model_for_render(text, &parsed.meta)
            .expect("Timeline typed projection succeeds");
        assert_eq!(timeline_syntax_construction_count(), 1);

        reset_timeline_syntax_construction_count();
        let (combined_json, combined_editor) = crate::family::test_support::into_result(
            parse_timeline_json_and_editor_facts(text, &parsed.meta, &OperationControl::new()),
        )
        .expect("Timeline combined projection succeeds");
        assert_eq!(timeline_syntax_construction_count(), 1);
        assert_eq!(combined_json, parsed.model);
        assert!(!combined_editor.symbols.is_empty());

        assert_eq!(
            render_model_to_compat_json(&typed, &parsed.meta).unwrap(),
            combined_json
        );
        assert_eq!(parsed.model["type"], "timeline");
        assert!(parsed.model["accDescr"].is_null());
    }

    #[test]
    fn timeline_typed_projection_preserves_empty_and_header_only_output_states() {
        let meta = ParseMetadata {
            diagram_type: "timeline".to_string(),
            config: crate::MermaidConfig::empty_object(),
            effective_config: crate::MermaidConfig::empty_object(),
            title: None,
        };
        for source in ["", "timeline"] {
            let compat = parse_timeline(source, &meta).unwrap();
            let typed = parse_timeline_model_for_render(source, &meta).unwrap();

            assert_eq!(
                render_model_to_compat_json(&typed, &meta).unwrap(),
                compat,
                "projection drift for {source:?}"
            );
        }
    }

    #[test]
    fn timeline_malformed_event_recovers_prior_parser_facts_once() {
        let engine = Engine::new();
        let text = "timeline\nsection Build\nImplement parser:event\n";
        let colon = text.find(':').expect("malformed event colon");

        reset_timeline_syntax_construction_count();
        let facts = engine
            .parse_editor_semantic_facts_with_type_sync("timeline", text)
            .expect("Timeline editor recovery succeeds")
            .expect("Timeline editor facts are available");

        assert_eq!(timeline_syntax_construction_count(), 1);
        assert_eq!(facts.completeness, EditorSemanticCompleteness::Recovered);
        assert!(facts.symbols.iter().any(|symbol| {
            symbol.name == "Build" && symbol.role == EditorSemanticRole::Outline
        }));
        assert!(facts.symbols.iter().any(|symbol| {
            symbol.name == "Implement parser" && symbol.role == EditorSemanticRole::Outline
        }));
        assert!(facts.diagnostics.iter().any(|diagnostic| {
            diagnostic.kind == EditorSemanticDiagnosticKind::ParserRecovery
                && diagnostic.span == Some(SourceSpan::new(colon + 1, colon + 1))
        }));
    }

    #[test]
    fn timeline_direction_is_preserved_in_semantic_and_compatibility_models() {
        let engine = Engine::new();
        for (source, expected) in [
            ("timeline\n2026", TimelineDirection::LeftToRight),
            ("timeline LR\n2026", TimelineDirection::LeftToRight),
            ("timeline TD\n2026", TimelineDirection::TopDown),
        ] {
            let parsed = engine
                .parse_diagram_for_render_model_sync(source, ParseOptions::strict())
                .expect("Timeline parse succeeds")
                .expect("Timeline is detected");
            let crate::RenderSemanticModel::Timeline(model) = parsed.model() else {
                panic!("expected Timeline render model");
            };

            assert_eq!(model.direction, expected, "direction for {source:?}");
            assert_eq!(
                render_model_to_compat_json(model, parsed.metadata()).expect("compatibility JSON")
                    ["direction"],
                serde_json::to_value(expected).expect("direction serializes")
            );
        }
    }

    #[test]
    fn timeline_semantic_facts_are_exact_for_crlf_unicode_and_repeated_text() {
        let source = concat!(
            "---\r\n",
            "config:\r\n",
            "  timeline:\r\n",
            "    disableMulticolor: false\r\n",
            "---\r\n",
            "%%{init: { 'theme': 'base' }}%%\r\n",
            "%% global comment\r\n",
            "timeline LR # header comment\r\n",
            "# family comment 🤓\r\n",
            "title title\r\n",
            "accTitle: accTitle # accessibility comment\r\n",
            "accDescr: accDescr; trailing\r\n",
            "accDescr {\r\n",
            "  多行说明 🤓\r\n",
            "}\r\n",
            "section section\r\n",
            "section 重复\r\n",
            "重复: 重复\r\n",
            ": 重复\r\n",
        );
        let facts = Engine::new()
            .parse_editor_semantic_facts_with_type_sync("timeline", source)
            .expect("Timeline editor parse")
            .expect("Timeline editor facts");

        assert_eq!(facts.completeness, EditorSemanticCompleteness::Complete);
        let repeated = source
            .match_indices("重复")
            .map(|(start, text)| SourceSpan::new(start, start + text.len()))
            .collect::<Vec<_>>();
        assert_eq!(repeated.len(), 4);
        for (span, kind, role) in [
            (
                repeated[0],
                EditorSemanticKind::Namespace,
                EditorSemanticRole::Outline,
            ),
            (
                repeated[1],
                EditorSemanticKind::Event,
                EditorSemanticRole::Outline,
            ),
            (
                repeated[2],
                EditorSemanticKind::String,
                EditorSemanticRole::Payload,
            ),
            (
                repeated[3],
                EditorSemanticKind::String,
                EditorSemanticRole::Payload,
            ),
        ] {
            assert!(facts.symbols.iter().any(|symbol| {
                symbol.name == "重复"
                    && symbol.kind == kind
                    && symbol.role == role
                    && symbol.selection == span
            }));
        }
    }

    #[test]
    fn timeline_recovery_keeps_error_prefix_and_later_lines_while_strict_returns_first_error() {
        let source = concat!(
            "timeline TD\r\n",
            "section Before\r\n",
            "Before task:event\r\n",
            "After task: 🤓 后续\r\n",
            "Later task:broken\r\n",
        );
        let first_colon = source.find(':').expect("first malformed event colon");
        let meta = ParseMetadata {
            diagram_type: "timeline".to_string(),
            config: crate::MermaidConfig::empty_object(),
            effective_config: crate::MermaidConfig::empty_object(),
            title: None,
        };
        let error =
            parse_timeline(source, &meta).expect_err("strict parse must return first error");
        let Error::DiagramParse { diagnostic, .. } = error else {
            panic!("expected structured Timeline parse error");
        };
        assert_eq!(
            diagnostic.message(),
            "invalid event token: missing whitespace after ':'"
        );
        assert_eq!(
            diagnostic.span(),
            Some(SourceSpan::new(first_colon + 1, first_colon + 1))
        );

        let facts = Engine::new()
            .parse_editor_semantic_facts_with_type_sync("timeline", source)
            .expect("Timeline editor recovery")
            .expect("Timeline recovery facts");
        assert_eq!(facts.completeness, EditorSemanticCompleteness::Recovered);
        assert!(facts.symbols.iter().any(|symbol| {
            symbol.name == "Before task"
                && symbol.kind == EditorSemanticKind::Event
                && symbol.role == EditorSemanticRole::Outline
        }));
        assert!(facts.symbols.iter().any(|symbol| {
            symbol.name == "After task"
                && symbol.kind == EditorSemanticKind::Event
                && symbol.role == EditorSemanticRole::Outline
        }));
        assert!(facts.symbols.iter().any(|symbol| {
            symbol.name == "🤓 后续"
                && symbol.kind == EditorSemanticKind::String
                && symbol.role == EditorSemanticRole::Payload
        }));
        assert!(facts.symbols.iter().any(|symbol| {
            symbol.name == "Later task"
                && symbol.kind == EditorSemanticKind::Event
                && symbol.role == EditorSemanticRole::Outline
        }));
    }

    #[test]
    fn timeline_rejects_unterminated_multiline_acc_descr_like_pinned_jison() {
        let text = "timeline\naccDescr {\n  partial description\n";

        reset_timeline_syntax_construction_count();
        let snapshot = Engine::new()
            .parse_diagram_snapshot_with_type_sync("timeline", text)
            .expect("Timeline snapshot operation")
            .expect("Timeline snapshot");
        assert_eq!(timeline_syntax_construction_count(), 1);
        assert!(matches!(
            snapshot.outcome(),
            crate::DiagramParseOutcome::Failed(_)
        ));
        let crate::ParsedEditorFacts::Available(facts) = snapshot.editor_facts() else {
            panic!("Timeline recovery facts");
        };
        assert_eq!(facts.completeness, EditorSemanticCompleteness::Recovered);
        assert!(!facts.diagnostics.is_empty());
        assert!(facts.symbols.iter().any(|symbol| {
            symbol.name == "partial description" && symbol.role == EditorSemanticRole::Payload
        }));
    }
}
