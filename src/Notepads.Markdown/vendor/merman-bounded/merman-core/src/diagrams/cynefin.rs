use crate::diagrams::scan::strip_line_ending;
use crate::{
    EditorExpectedSyntax, EditorExpectedSyntaxKind, EditorSemanticFacts, EditorSemanticKind,
    EditorSemanticSymbol, Error, ParseMetadata, Result, SourceSpan,
};
use crate::{
    common_db::LangiumCommonDbFields,
    diagrams::langium_common::{
        LangiumCommonFacts, parse_langium_common, parse_langium_string,
        push_langium_common_editor_fact, push_langium_common_recovery,
        strip_langium_inline_comment,
    },
};
use serde_json::{Value, json};

const HEADER: &str = "cynefin-beta";
const DOMAINS: &[&str] = &["complex", "complicated", "clear", "chaotic", "confusion"];

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, PartialEq, Eq)]
pub struct CynefinItemModel {
    pub label: String,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, PartialEq, Eq)]
pub struct CynefinDomainModel {
    pub name: String,
    #[serde(default)]
    pub items: Vec<CynefinItemModel>,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, PartialEq, Eq)]
pub struct CynefinTransitionModel {
    pub from: String,
    pub to: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub label: Option<String>,
}

#[derive(Debug, Clone, Default, serde::Serialize, serde::Deserialize, PartialEq, Eq)]
pub struct CynefinDiagramModel {
    #[serde(default, rename = "accTitle")]
    pub acc_title: Option<String>,
    #[serde(default, rename = "accDescr")]
    pub acc_descr: Option<String>,
    #[serde(default)]
    pub title: Option<String>,
    #[serde(default)]
    pub domains: Vec<CynefinDomainModel>,
    #[serde(default)]
    pub transitions: Vec<CynefinTransitionModel>,
}

pub type CynefinDiagramRenderModel = CynefinDiagramModel;

impl CynefinDiagramModel {
    pub(crate) fn sanitize_common_db_fields(&mut self, config: &crate::MermaidConfig) {
        crate::common_db::sanitize_optional_title(&mut self.title, config);
        crate::common_db::sanitize_optional_acc_title(&mut self.acc_title, config);
        crate::common_db::sanitize_optional_acc_descr(&mut self.acc_descr, config);
    }
}

#[derive(Debug, Clone)]
struct SpannedText {
    text: String,
    span: SourceSpan,
    selection: SourceSpan,
}

#[derive(Debug, Clone)]
struct TransitionParts {
    from: SpannedText,
    to: SpannedText,
    label: Option<SpannedText>,
}

#[derive(Debug, Clone)]
enum CynefinLinePart {
    Domain(SpannedText),
    Item(SpannedText),
    Transition(TransitionParts),
}

struct CynefinParsedLine {
    parts: Vec<CynefinLinePart>,
    error: Option<Error>,
}

struct CynefinSemanticSource {
    model: CynefinDiagramModel,
    editor_facts: EditorSemanticFacts,
}

struct CynefinParseOutcome {
    source: CynefinSemanticSource,
    first_error: Option<Error>,
}

impl CynefinParseOutcome {
    fn into_strict_source(self) -> Result<CynefinSemanticSource> {
        match self.first_error {
            Some(error) => Err(error),
            None => Ok(self.source),
        }
    }
}

pub(crate) fn parse_cynefin(code: &str, meta: &ParseMetadata) -> Result<Value> {
    let mut model = parse_cynefin_semantic_source(code, meta)?.model;
    model.sanitize_common_db_fields(&meta.effective_config);

    render_model_to_compat_json(&model, meta)
}

pub(crate) fn parse_cynefin_json_and_editor_facts(
    code: &str,
    meta: &ParseMetadata,
    control: &crate::OperationControl,
) -> crate::OperationControlResult<crate::family::CombinedSemanticParse> {
    let CynefinParseOutcome {
        source: CynefinSemanticSource {
            mut model,
            editor_facts,
        },
        first_error,
    } = construct_cynefin_parse_outcome_controlled(code, meta, control)?;
    model.sanitize_common_db_fields(&meta.effective_config);
    let construction = match first_error {
        Some(error) => Err(crate::family::CombinedSemanticFailure::new(
            error,
            editor_facts,
        )),
        None => Ok(CynefinSemanticSource {
            model,
            editor_facts,
        }),
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

pub(crate) fn render_model_to_compat_json(
    model: &CynefinDiagramRenderModel,
    meta: &ParseMetadata,
) -> Result<Value> {
    Ok(json!({
        "type": meta.diagram_type,
        "title": &model.title,
        "accTitle": &model.acc_title,
        "accDescr": &model.acc_descr,
        "domains": &model.domains,
        "transitions": &model.transitions,
    }))
}

#[cfg(test)]
pub(crate) fn parse_cynefin_model_for_render(
    code: &str,
    meta: &ParseMetadata,
) -> Result<CynefinDiagramRenderModel> {
    let mut model = parse_cynefin_semantic_source(code, meta)?.model;
    model.sanitize_common_db_fields(&meta.effective_config);
    Ok(model)
}

pub(crate) fn parse_cynefin_model_for_render_controlled(
    code: &str,
    meta: &ParseMetadata,
    control: &crate::OperationControl,
) -> crate::OperationControlResult<Result<CynefinDiagramRenderModel>> {
    let outcome = construct_cynefin_parse_outcome_controlled(code, meta, control)?;
    let source = match outcome.into_strict_source() {
        Ok(source) => source,
        Err(error) => return Ok(Err(error)),
    };
    let mut model = source.model;
    model.sanitize_common_db_fields(&meta.effective_config);
    Ok(Ok(model))
}

fn trimmed_source_span(source: &str, source_start: usize) -> SourceSpan {
    let trimmed = source.trim();
    let start = source_start + source.len().saturating_sub(source.trim_start().len());
    SourceSpan::new(start, start + trimmed.len())
}

fn parse_cynefin_semantic_source(
    code: &str,
    meta: &ParseMetadata,
) -> Result<CynefinSemanticSource> {
    construct_cynefin_parse_outcome(code, meta).into_strict_source()
}

fn construct_cynefin_parse_outcome(code: &str, meta: &ParseMetadata) -> CynefinParseOutcome {
    construct_cynefin_parse_outcome_controlled(code, meta, &crate::OperationControl::new())
        .expect("a private parse control cannot be cancelled")
}

fn construct_cynefin_parse_outcome_controlled(
    code: &str,
    meta: &ParseMetadata,
    control: &crate::OperationControl,
) -> crate::OperationControlResult<CynefinParseOutcome> {
    control.checkpoint()?;
    #[cfg(test)]
    crate::diagrams::langium_common::record_family_syntax_construction("cynefin");

    let mut model = CynefinDiagramModel::default();
    let mut editor_facts = EditorSemanticFacts::new();
    let mut header_decided = false;
    let mut current_domain: Option<usize> = None;
    let mut offset = 0usize;
    let mut common = LangiumCommonFacts::default();
    let mut first_error = None;

    while offset < code.len() {
        control.checkpoint()?;
        let line_start = offset;
        let (segment, next_offset) = physical_line_at(code, offset);
        offset = next_offset;
        let line = strip_line_ending(segment);
        let stripped = strip_inline_comment_aware(line);
        let trimmed = stripped.trim();
        if trimmed.is_empty() {
            continue;
        }

        let (body, body_start) = if !header_decided {
            header_decided = true;
            let Some(header) = split_header(stripped, line_start) else {
                let span = trimmed_source_span(stripped, line_start);
                remember_cynefin_error(
                    &mut first_error,
                    &mut editor_facts,
                    Error::diagram_parse_exact(
                        meta.diagram_type.clone(),
                        "expected cynefin-beta header",
                        span,
                    ),
                    "expected cynefin-beta header",
                    Some(span),
                );
                current_domain = None;
                continue;
            };
            (header.body, header.body_start)
        } else {
            let body_start = line_start;
            (stripped, body_start)
        };

        if body.trim().is_empty() {
            continue;
        }

        if let Some(parsed) = parse_langium_common(code, body_start) {
            offset = body_start + parsed.consumed;
            current_domain = None;
            push_langium_common_editor_fact(&mut editor_facts, &parsed.fact, "cynefin");
            if let Some(diagnostic) = parsed.diagnostic {
                let error = Error::diagram_parse_insertion_point(
                    meta.diagram_type.clone(),
                    diagnostic.message.clone(),
                    diagnostic.span.start,
                );
                first_error.get_or_insert(error);
                push_langium_common_recovery(&mut editor_facts, &diagnostic);
            } else {
                common.push(parsed.fact);
            }
            continue;
        }

        let parsed_line = parse_cynefin_line_parts_controlled(body, body_start, control)?;
        if let Some(error) = parsed_line.error {
            let span = trimmed_source_span(body, body_start);
            let message = cynefin_error_message(&error);
            remember_cynefin_error(
                &mut first_error,
                &mut editor_facts,
                Error::diagram_parse_exact(meta.diagram_type.clone(), &message, span),
                format!("cynefin parser recovered after parse error: {message}"),
                Some(span),
            );
            current_domain = None;
            continue;
        }

        for (index, part) in parsed_line.parts.into_iter().enumerate() {
            if index % 128 == 0 {
                control.checkpoint()?;
            }
            match part {
                CynefinLinePart::Domain(domain) => {
                    current_domain = Some(start_domain(&mut model.domains, domain.text.clone()));
                    push_domain_fact(&mut editor_facts, domain, "cynefin domain");
                }
                CynefinLinePart::Item(item) => {
                    let Some(domain_idx) = current_domain else {
                        let span = item.span;
                        remember_cynefin_error(
                            &mut first_error,
                            &mut editor_facts,
                            Error::diagram_parse_exact(
                                meta.diagram_type.clone(),
                                "cynefin item must follow a domain",
                                span,
                            ),
                            "cynefin item must follow a domain",
                            Some(span),
                        );
                        current_domain = None;
                        continue;
                    };
                    model.domains[domain_idx].items.push(CynefinItemModel {
                        label: item.text.clone(),
                    });
                    push_payload_fact(
                        &mut editor_facts,
                        item,
                        "cynefin domain item",
                        EditorSemanticKind::String,
                    );
                }
                CynefinLinePart::Transition(transition) => {
                    current_domain = None;
                    push_domain_fact(
                        &mut editor_facts,
                        transition.from.clone(),
                        "cynefin transition source",
                    );
                    push_domain_fact(
                        &mut editor_facts,
                        transition.to.clone(),
                        "cynefin transition target",
                    );
                    if let Some(label) = transition.label.clone() {
                        push_payload_fact(
                            &mut editor_facts,
                            label,
                            "cynefin transition label",
                            EditorSemanticKind::String,
                        );
                    }
                    if transition.from.text == transition.to.text {
                        editor_facts.push_diagnostic(
                            format!(
                                "cynefin self-loop transition on domain \"{}\" is skipped",
                                transition.from.text
                            ),
                            Some(transition.from.span),
                        );
                    } else {
                        model.transitions.push(CynefinTransitionModel {
                            from: transition.from.text,
                            to: transition.to.text,
                            label: transition
                                .label
                                .map(|label| label.text)
                                .filter(|label| !label.is_empty()),
                        });
                    }
                }
            }
        }
    }

    if !header_decided {
        let span = SourceSpan::new(0, 0);
        remember_cynefin_error(
            &mut first_error,
            &mut editor_facts,
            Error::diagram_parse_exact(
                meta.diagram_type.clone(),
                "expected cynefin-beta header",
                span,
            ),
            "expected cynefin-beta header",
            Some(span),
        );
    }

    let common = LangiumCommonDbFields::from_facts(&common);
    model.title = common.title;
    model.acc_title = common.acc_title;
    model.acc_descr = common.acc_descr;

    control.checkpoint()?;
    Ok(CynefinParseOutcome {
        source: CynefinSemanticSource {
            model,
            editor_facts,
        },
        first_error,
    })
}

fn remember_cynefin_error(
    first_error: &mut Option<Error>,
    editor_facts: &mut EditorSemanticFacts,
    error: Error,
    message: impl Into<String>,
    span: Option<SourceSpan>,
) {
    if first_error.is_none() {
        *first_error = Some(error);
    }
    editor_facts.mark_recovered_from_parse_error(message, span);
}

fn cynefin_error_message(error: &Error) -> String {
    match error {
        Error::DiagramParse { diagnostic, .. } => diagnostic.message().to_string(),
        _ => error.to_string(),
    }
}

struct CynefinHeader<'a> {
    body: &'a str,
    body_start: usize,
}

fn split_header(line: &str, line_start: usize) -> Option<CynefinHeader<'_>> {
    let leading = line.len() - line.trim_start().len();
    let rest = &line[leading..];
    let after_header = rest.strip_prefix(HEADER)?;
    let next = after_header.chars().next();
    if next.is_some_and(|ch| ch != ':' && !ch.is_whitespace() && !after_header.starts_with("%%")) {
        return None;
    }

    let colon_len = after_header.starts_with(':') as usize;
    let body_offset = leading + HEADER.len() + colon_len;
    Some(CynefinHeader {
        body: &line[body_offset..],
        body_start: line_start + body_offset,
    })
}

fn parse_cynefin_line_parts_controlled(
    line: &str,
    line_start: usize,
    control: &crate::OperationControl,
) -> crate::OperationControlResult<CynefinParsedLine> {
    let mut cursor = CynefinCursor::new(line, line_start);
    let mut parts = Vec::new();
    let mut error = None;
    let mut iteration = 0usize;

    loop {
        if iteration.is_multiple_of(128) {
            control.checkpoint()?;
        }
        iteration += 1;
        cursor.skip_ws();
        if cursor.is_eof() {
            break;
        }

        match cursor.take_transition() {
            Ok(Some(transition)) => {
                parts.push(CynefinLinePart::Transition(transition));
                continue;
            }
            Ok(None) => {}
            Err(parse_error) => {
                error = Some(parse_error);
                break;
            }
        }

        if let Some(domain) = cursor.take_domain() {
            parts.push(CynefinLinePart::Domain(domain));
            continue;
        }

        if let Some(item) = cursor.take_quoted_string() {
            parts.push(CynefinLinePart::Item(item));
            continue;
        }

        error = Some(Error::diagram_parse_fallback(
            "cynefin",
            "expected cynefin domain, quoted item, transition, or common directive",
        ));
        break;
    }

    Ok(CynefinParsedLine { parts, error })
}

fn physical_line_at(code: &str, start: usize) -> (&str, usize) {
    let rest = &code[start..];
    let len = rest.find('\n').map_or(rest.len(), |index| index + 1);
    (&rest[..len], start + len)
}

fn start_domain(domains: &mut Vec<CynefinDomainModel>, name: String) -> usize {
    if let Some(idx) = domains.iter().position(|domain| domain.name == name) {
        domains[idx].items.clear();
        idx
    } else {
        domains.push(CynefinDomainModel {
            name,
            items: Vec::new(),
        });
        domains.len() - 1
    }
}

fn push_domain_fact(facts: &mut EditorSemanticFacts, domain: SpannedText, detail: &'static str) {
    facts.push_expected_syntax(EditorExpectedSyntax::new(
        EditorExpectedSyntaxKind::NodeIdentifier,
        domain.selection,
    ));
    facts.push_symbol(EditorSemanticSymbol::outline(
        domain.text,
        Some(detail.to_string()),
        EditorSemanticKind::Namespace,
        domain.span,
        domain.selection,
    ));
}

fn push_payload_fact(
    facts: &mut EditorSemanticFacts,
    value: SpannedText,
    detail: &'static str,
    kind: EditorSemanticKind,
) {
    facts.push_expected_syntax(EditorExpectedSyntax::new(
        EditorExpectedSyntaxKind::Payload,
        value.selection,
    ));
    facts.push_symbol(EditorSemanticSymbol::payload(
        value.text,
        Some(detail.to_string()),
        kind,
        value.span,
        value.selection,
    ));
}

struct CynefinCursor<'input> {
    input: &'input str,
    line_start: usize,
    pos: usize,
}

impl<'input> CynefinCursor<'input> {
    fn new(input: &'input str, line_start: usize) -> Self {
        Self {
            input,
            line_start,
            pos: 0,
        }
    }

    fn remaining(&self) -> &'input str {
        &self.input[self.pos..]
    }

    fn is_eof(&self) -> bool {
        self.pos >= self.input.len()
    }

    fn skip_ws(&mut self) {
        self.pos += self
            .remaining()
            .chars()
            .take_while(|ch| matches!(ch, ' ' | '\t'))
            .map(char::len_utf8)
            .sum::<usize>();
    }

    fn take_transition(&mut self) -> Result<Option<TransitionParts>> {
        let start = self.pos;
        self.skip_ws();
        let Some(from) = self.take_domain() else {
            self.pos = start;
            return Ok(None);
        };
        self.skip_ws();
        if !self.take_operator("-->") {
            self.pos = start;
            return Ok(None);
        }

        self.skip_ws();
        let Some(to) = self.take_domain() else {
            return Err(Error::diagram_parse_fallback(
                "cynefin",
                "expected cynefin transition target",
            ));
        };
        self.skip_ws();

        let label = if self.is_eof() {
            None
        } else {
            if !self.take_delimiter(":") {
                return Err(Error::diagram_parse_fallback(
                    "cynefin",
                    "expected ':' before cynefin transition label",
                ));
            }
            self.skip_ws();
            Some(self.take_quoted_string().ok_or_else(|| {
                Error::diagram_parse_fallback("cynefin", "expected quoted cynefin transition label")
            })?)
        };
        self.skip_ws();
        if !self.is_eof() {
            return Err(Error::diagram_parse_fallback(
                "cynefin",
                "unexpected trailing cynefin transition tokens",
            ));
        }

        Ok(Some(TransitionParts { from, to, label }))
    }

    fn take_operator(&mut self, operator: &str) -> bool {
        if self.remaining().starts_with(operator) {
            self.pos += operator.len();
            true
        } else {
            false
        }
    }

    fn take_delimiter(&mut self, delimiter: &str) -> bool {
        if self.remaining().starts_with(delimiter) {
            self.pos += delimiter.len();
            true
        } else {
            false
        }
    }

    fn take_domain(&mut self) -> Option<SpannedText> {
        let rest = self.remaining();
        for domain in DOMAINS {
            let Some(after) = rest.strip_prefix(domain) else {
                continue;
            };
            if after
                .chars()
                .next()
                .is_some_and(|ch| ch.is_ascii_alphanumeric() || ch == '_')
            {
                continue;
            }
            let start = self.line_start + self.pos;
            self.pos += domain.len();
            return Some(SpannedText {
                text: (*domain).to_string(),
                span: SourceSpan::new(start, start + domain.len()),
                selection: SourceSpan::new(start, start + domain.len()),
            });
        }
        None
    }

    fn take_quoted_string(&mut self) -> Option<SpannedText> {
        let start = self.line_start + self.pos;
        let parsed = parse_langium_string(self.remaining(), start)?;
        self.pos += parsed.consumed;
        Some(SpannedText {
            text: parsed.value,
            span: parsed.raw_span,
            selection: parsed.value_span,
        })
    }
}

fn strip_inline_comment_aware(line: &str) -> &str {
    strip_langium_inline_comment(line)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn metadata() -> crate::ParseMetadata {
        crate::ParseMetadata {
            diagram_type: "cynefin".to_string(),
            config: crate::MermaidConfig::empty_object(),
            effective_config: crate::MermaidConfig::empty_object(),
            title: None,
        }
    }

    #[test]
    fn controlled_parse_can_cancel_between_cynefin_lines() {
        let control = crate::OperationControl::new();
        control.cancel_after_checkpoints(2);

        assert!(matches!(
            construct_cynefin_parse_outcome_controlled(
                "cynefin-beta\ncomplex\n\"Probe\"\n",
                &metadata(),
                &control,
            ),
            Err(crate::OperationCancelled { .. })
        ));
    }

    #[test]
    fn strips_comment_markers_outside_quoted_strings() {
        assert_eq!(
            strip_inline_comment_aware("  complex %% comment"),
            "  complex "
        );
        assert_eq!(
            strip_inline_comment_aware("  \"100%% visible\" %% comment"),
            "  \"100%% visible\" "
        );
    }

    #[test]
    fn parses_escaped_quoted_string_payload() {
        let mut cursor = CynefinCursor::new("  \"Probe \\\"quoted\\\" value\"", 10);
        cursor.skip_ws();
        let value = cursor.take_quoted_string().unwrap();
        cursor.skip_ws();
        assert!(cursor.is_eof());
        assert_eq!(value.text, "Probe \"quoted\" value");
        assert_eq!(value.selection, SourceSpan::new(13, 35));
    }

    #[test]
    fn cynefin_recovery_uses_exact_trimmed_crlf_spans() {
        for (source, invalid) in [
            ("  invalid header %% hidden\r\n", "invalid header"),
            (
                "cynefin-beta\r\n  invalid body %% hidden\r\n",
                "invalid body",
            ),
            ("cynefin-beta\r\n  \"orphan\" %% hidden\r\n", "\"orphan\""),
        ] {
            let facts = crate::Engine::new()
                .parse_editor_semantic_facts_with_type_sync("cynefin", source)
                .unwrap()
                .unwrap();
            let start = source.find(invalid).unwrap();
            assert!(facts.diagnostics.iter().any(|diagnostic| {
                diagnostic.kind == crate::EditorSemanticDiagnosticKind::ParserRecovery
                    && diagnostic.span == Some(SourceSpan::new(start, start + invalid.len()))
            }));
        }
    }

    #[test]
    fn cynefin_recovery_preserves_cursor_prefix_for_crlf_input() {
        let source = concat!(
            "cynefin-beta\r\n",
            "  complex --> ??? %% malformed target\r\n",
            "title Later\r\n",
            "clear\r\n",
        );
        let invalid = "complex --> ???";
        let invalid_start = source.find(invalid).unwrap();

        let facts = crate::family::test_support::editor_facts(
            parse_cynefin_json_and_editor_facts,
            source,
            &crate::ParseMetadata {
                diagram_type: "cynefin".to_string(),
                config: crate::MermaidConfig::empty_object(),
                effective_config: crate::MermaidConfig::empty_object(),
                title: None,
            },
        );

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
    fn typed_render_model_projects_exact_compatibility_json() {
        let source = concat!(
            "cynefin-beta\n",
            "title Cynefin Map\n",
            "accTitle: Accessible Map\n",
            "complex\n",
            "  \"Probe\"\n",
            "complex --> complicated : \"Sense\"\n",
        );
        let engine = crate::Engine::new();
        let compat = engine
            .parse_diagram_sync(source, crate::ParseOptions::strict())
            .unwrap()
            .unwrap();
        let typed = engine
            .parse_diagram_for_render_model_sync(source, crate::ParseOptions::strict())
            .unwrap()
            .unwrap();
        let crate::RenderSemanticModel::Cynefin(model) = typed.model() else {
            panic!("expected Cynefin render model");
        };

        assert_eq!(
            render_model_to_compat_json(model, typed.metadata()).unwrap(),
            compat.model
        );
    }
}
