mod source_config;
mod source_edit_map;

pub(crate) use source_config::SourceConfigPath;
pub use source_config::{
    FrontmatterSourceEvidence, SourceConfigEvidence, SourceConfigKeyEvidence, SourceConfigOrigin,
    SourceDirectiveEvidence,
};
pub use source_edit_map::PreprocessedSource;

use crate::{
    DetectorRegistry, EditorExpectedSyntaxKind, Error, MermaidConfig, OperationControl,
    OperationControlResult, Result, SourceSpan, diagram::CapturedPanic, editor::line_content_end,
};
use serde_json::{Map, Value};
use source_edit_map::{ReplacementMapping, SourceEdit};
use std::borrow::Cow;
#[cfg(test)]
use std::cell::Cell;

#[cfg(test)]
thread_local! {
    static PUBLIC_PARSE_PREPROCESS_COUNT: Cell<usize> = const { Cell::new(0) };
}

#[cfg(all(test, feature = "all-diagrams"))]
pub(crate) fn reset_public_parse_preprocess_count() {
    PUBLIC_PARSE_PREPROCESS_COUNT.set(0);
}

#[cfg(all(test, feature = "all-diagrams"))]
pub(crate) fn public_parse_preprocess_count() -> usize {
    PUBLIC_PARSE_PREPROCESS_COUNT.get()
}

#[derive(Debug, Clone)]
pub struct PreprocessResult {
    pub source: PreprocessedSource,
    pub title: Option<String>,
    pub config: MermaidConfig,
    pub(crate) with_comments: Option<PreprocessedSource>,
}

#[derive(Debug)]
pub(crate) struct PreprocessCaptureOutcome {
    pub(crate) outcome: PreprocessCaptureResult,
    pub(crate) source_config: SourceConfigEvidence,
}

#[derive(Debug)]
pub(crate) enum PreprocessCaptureResult {
    Ready(Box<PreprocessResult>),
    Failed(Error),
    Panicked(CapturedPanic),
}

impl PreprocessCaptureOutcome {
    fn into_result(self) -> Result<PreprocessResult> {
        match self.outcome {
            PreprocessCaptureResult::Ready(result) => Ok(*result),
            PreprocessCaptureResult::Failed(error) => Err(error),
            PreprocessCaptureResult::Panicked(panic) => {
                std::panic::resume_unwind(panic.into_payload())
            }
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum SourceConfigCaptureMode {
    Omit,
    Collect,
}

impl SourceConfigCaptureMode {
    const fn collects(self) -> bool {
        matches!(self, Self::Collect)
    }
}

#[derive(Debug)]
struct LocalConfigKeyEvidence {
    path: SourceConfigPath,
    span: Option<SourceSpan>,
    rewrite_safe: bool,
}

impl PreprocessResult {
    pub fn code(&self) -> &str {
        self.source.text()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FrontmatterByteSpan {
    pub start: usize,
    pub end: usize,
}

/// Source-backed frontmatter bounds found without materializing a dedented body.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FrontmatterBlockLocation<'a> {
    pub full: FrontmatterByteSpan,
    pub body: FrontmatterByteSpan,
    pub indent: &'a str,
    pub stripped: &'a str,
}

#[derive(Debug, Clone)]
pub struct FrontmatterBlock<'a> {
    pub full: FrontmatterByteSpan,
    pub body: FrontmatterByteSpan,
    pub indent: &'a str,
    pub dedented_body: Cow<'a, str>,
    pub stripped: &'a str,
}

const MAX_CONFIG_NESTING_DEPTH: usize = crate::MAX_DIAGRAM_NESTING_DEPTH;
const CONTROLLED_SCAN_CHECKPOINT_BYTES: usize = 4 * 1024;
const MAX_DIRECTIVE_CONFIG_PARSE_BYTES: usize = 1024 * 1024;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum DirectiveRecoveryMode {
    Strict,
    RecoverLine,
}

pub fn preprocess_diagram(input: &str, registry: &DetectorRegistry) -> Result<PreprocessResult> {
    preprocess_diagram_with_known_type(input, registry, None)
}

pub fn preprocess_diagram_with_known_type(
    input: &str,
    registry: &DetectorRegistry,
    diagram_type: Option<&str>,
) -> Result<PreprocessResult> {
    preprocess_diagram_with_known_type_and_directive_recovery(
        input,
        registry,
        diagram_type,
        DirectiveRecoveryMode::Strict,
    )
}

pub(crate) fn preprocess_diagram_with_known_type_and_directive_recovery(
    input: &str,
    registry: &DetectorRegistry,
    diagram_type: Option<&str>,
    directive_recovery: DirectiveRecoveryMode,
) -> Result<PreprocessResult> {
    let control = OperationControl::new();
    preprocess_diagram_with_known_type_and_directive_recovery_controlled(
        input,
        registry,
        diagram_type,
        directive_recovery,
        &control,
    )
    .expect("a private parse control cannot be cancelled")
}

pub(crate) fn preprocess_diagram_with_known_type_and_directive_recovery_controlled(
    input: &str,
    registry: &DetectorRegistry,
    diagram_type: Option<&str>,
    directive_recovery: DirectiveRecoveryMode,
    control: &OperationControl,
) -> OperationControlResult<Result<PreprocessResult>> {
    Ok(
        preprocess_diagram_with_known_type_and_directive_recovery_capture_controlled(
            input,
            registry,
            diagram_type,
            directive_recovery,
            SourceConfigCaptureMode::Omit,
            control,
        )?
        .into_result(),
    )
}

fn preprocess_diagram_with_known_type_and_directive_recovery_capture_controlled(
    input: &str,
    registry: &DetectorRegistry,
    diagram_type: Option<&str>,
    directive_recovery: DirectiveRecoveryMode,
    capture_mode: SourceConfigCaptureMode,
    control: &OperationControl,
) -> OperationControlResult<PreprocessCaptureOutcome> {
    let captured = preprocess_single_pass_controlled(
        PreprocessedSource::new_controlled(input, control)?,
        registry,
        diagram_type,
        directive_recovery,
        capture_mode,
        control,
    )?;
    control.checkpoint()?;
    let outcome = match captured.outcome {
        PreprocessCaptureResult::Ready(preprocessed) => PreprocessCaptureResult::Ready(Box::new(
            prepare_parser_code_controlled(*preprocessed, control)?,
        )),
        PreprocessCaptureResult::Failed(error) => PreprocessCaptureResult::Failed(error),
        PreprocessCaptureResult::Panicked(panic) => PreprocessCaptureResult::Panicked(panic),
    };
    control.checkpoint()?;
    Ok(PreprocessCaptureOutcome {
        outcome,
        source_config: captured.source_config,
    })
}

pub(crate) fn preprocess_diagram_with_known_type_and_directive_recovery_evidence_controlled(
    input: &str,
    registry: &DetectorRegistry,
    diagram_type: Option<&str>,
    directive_recovery: DirectiveRecoveryMode,
    control: &OperationControl,
) -> OperationControlResult<PreprocessCaptureOutcome> {
    preprocess_diagram_with_known_type_and_directive_recovery_capture_controlled(
        input,
        registry,
        diagram_type,
        directive_recovery,
        SourceConfigCaptureMode::Collect,
        control,
    )
}

#[cfg(test)]
pub(crate) fn preprocess_mermaid_public_parse_pipeline(
    input: &str,
    registry: &DetectorRegistry,
    diagram_type: Option<&str>,
) -> Result<PreprocessResult> {
    preprocess_mermaid_public_parse_pipeline_with_directive_recovery(
        input,
        registry,
        diagram_type,
        DirectiveRecoveryMode::Strict,
    )
}

#[cfg(test)]
pub(crate) fn preprocess_mermaid_public_parse_pipeline_with_directive_recovery(
    input: &str,
    registry: &DetectorRegistry,
    diagram_type: Option<&str>,
    directive_recovery: DirectiveRecoveryMode,
) -> Result<PreprocessResult> {
    let control = OperationControl::new();
    preprocess_mermaid_public_parse_pipeline_with_directive_recovery_controlled(
        input,
        registry,
        diagram_type,
        directive_recovery,
        &control,
    )
    .expect("a private parse control cannot be cancelled")
}

pub(crate) fn preprocess_mermaid_public_parse_pipeline_with_directive_recovery_controlled(
    input: &str,
    registry: &DetectorRegistry,
    diagram_type: Option<&str>,
    directive_recovery: DirectiveRecoveryMode,
    control: &OperationControl,
) -> OperationControlResult<Result<PreprocessResult>> {
    Ok(
        preprocess_mermaid_public_parse_pipeline_with_directive_recovery_capture_controlled(
            input,
            registry,
            diagram_type,
            directive_recovery,
            SourceConfigCaptureMode::Omit,
            control,
        )?
        .into_result(),
    )
}

pub(crate) fn preprocess_mermaid_public_parse_pipeline_with_directive_recovery_evidence_controlled(
    input: &str,
    registry: &DetectorRegistry,
    diagram_type: Option<&str>,
    directive_recovery: DirectiveRecoveryMode,
    control: &OperationControl,
) -> OperationControlResult<PreprocessCaptureOutcome> {
    preprocess_mermaid_public_parse_pipeline_with_directive_recovery_capture_controlled(
        input,
        registry,
        diagram_type,
        directive_recovery,
        SourceConfigCaptureMode::Collect,
        control,
    )
}

fn preprocess_mermaid_public_parse_pipeline_with_directive_recovery_capture_controlled(
    input: &str,
    registry: &DetectorRegistry,
    diagram_type: Option<&str>,
    directive_recovery: DirectiveRecoveryMode,
    capture_mode: SourceConfigCaptureMode,
    control: &OperationControl,
) -> OperationControlResult<PreprocessCaptureOutcome> {
    #[cfg(test)]
    PUBLIC_PARSE_PREPROCESS_COUNT.set(PUBLIC_PARSE_PREPROCESS_COUNT.get() + 1);

    // Mermaid 12 passes the complete DiagramCode to Diagram.fromText on both public paths.
    preprocess_diagram_with_known_type_and_directive_recovery_capture_controlled(
        input,
        registry,
        diagram_type,
        directive_recovery,
        capture_mode,
        control,
    )
}

fn preprocess_single_pass_controlled(
    mut source: PreprocessedSource,
    registry: &DetectorRegistry,
    diagram_type: Option<&str>,
    directive_recovery: DirectiveRecoveryMode,
    capture_mode: SourceConfigCaptureMode,
    control: &OperationControl,
) -> OperationControlResult<PreprocessCaptureOutcome> {
    control.checkpoint()?;
    let capture_editor_evidence = capture_mode.collects();
    cleanup_text_controlled(&mut source, control)?;
    let mut source_config = SourceConfigEvidence::empty();
    if capture_mode.collects() {
        if let Some(config_insert_span) = source.try_map_span(SourceSpan::new(0, 0)) {
            source_config.set_config_insert_span(config_insert_span);
        } else {
            source_config.mark_rewrite_unsafe();
        }
    }
    let frontmatter = process_frontmatter_controlled(source.text(), capture_mode, control)?;
    if capture_mode.collects() {
        append_frontmatter_evidence(&source, &frontmatter, &mut source_config, control)?;
    }
    let frontmatter_len = frontmatter.full.map_or(0, |full| full.end);
    let mut first_error = None;
    let (title, mut frontmatter_config) = match frontmatter.result {
        Ok(processed) => (processed.title, processed.config),
        Err(error) => {
            source_config.mark_rewrite_unsafe();
            if !capture_mode.collects() {
                return Ok(PreprocessCaptureOutcome {
                    outcome: PreprocessCaptureResult::Failed(error),
                    source_config,
                });
            }
            first_error = Some(error);
            (None, MermaidConfig::empty_object())
        }
    };
    if frontmatter_len > 0 {
        if capture_editor_evidence {
            let expected_end = editor_evidence_line_end(source.text(), frontmatter_len);
            source.record_global_expected_syntax(
                EditorExpectedSyntaxKind::Frontmatter,
                SourceSpan::new(0, expected_end),
            );
        }
        source.record_frontmatter_removal(frontmatter_len);
        source.apply_edits(vec![SourceEdit::delete(0..frontmatter_len)], control)?;
    }

    control.checkpoint()?;
    let processed_directives = process_directives_controlled(
        source.text(),
        registry,
        diagram_type,
        directive_recovery,
        capture_mode,
        first_error.is_none(),
        control,
    )?;
    if capture_mode.collects() {
        append_directive_evidence(&source, &processed_directives, &mut source_config, control)?;
    }
    if let Some(panic) = processed_directives.panic {
        return Ok(PreprocessCaptureOutcome {
            outcome: PreprocessCaptureResult::Panicked(panic),
            source_config,
        });
    }
    if let Some(error) = processed_directives.error {
        source_config.mark_rewrite_unsafe();
        if first_error.is_none() {
            first_error = Some(error);
        }
    }
    if let Some(error) = first_error {
        control.checkpoint()?;
        return Ok(PreprocessCaptureOutcome {
            outcome: PreprocessCaptureResult::Failed(error),
            source_config,
        });
    }
    if processed_directives.recovered_incomplete_directive {
        source.mark_recovered_incomplete_directive();
    }
    if capture_editor_evidence {
        for prefix in processed_directives.editor_prefixes.iter().cloned() {
            source.record_global_directive_prefix(prefix);
        }
        for removal in &processed_directives.removals {
            source.record_global_expected_syntax(
                EditorExpectedSyntaxKind::Directive,
                SourceSpan::new(removal.start, removal.end),
            );
        }
    }
    source.apply_edits(
        processed_directives
            .removals
            .into_iter()
            .map(SourceEdit::delete)
            .collect(),
        control,
    )?;

    frontmatter_config.deep_merge(processed_directives.config.as_value());

    control.checkpoint()?;
    // These families opt into Mermaid's comment-preserving parser input. Keep the existing
    // edit map with that buffer so editor byte spans and domain lexer positions remain distinct.
    let preserve_comments = match diagram_type {
        Some(diagram_type) => matches!(diagram_type, "agentflow" | "eventmodeling"),
        None => {
            let header = source
                .text()
                .lines()
                .map(str::trim)
                .find(|line| !line.is_empty() && !line.starts_with("%%"))
                .unwrap_or("");
            crate::detect::detector_agentflow(header, &mut MermaidConfig::empty_object())
                || crate::detect::detector_eventmodeling(header, &mut MermaidConfig::empty_object())
        }
    };
    let mut with_comments = preserve_comments.then(|| source.clone());
    remove_mermaid_comments_controlled(&mut source, capture_editor_evidence, control)?;
    if let Some(with_comments) = &mut with_comments {
        with_comments.copy_global_editor_evidence_from(&source);
    }
    Ok(PreprocessCaptureOutcome {
        outcome: PreprocessCaptureResult::Ready(Box::new(PreprocessResult {
            source,
            title,
            config: frontmatter_config,
            with_comments,
        })),
        source_config,
    })
}

fn append_frontmatter_evidence(
    source: &PreprocessedSource,
    captured: &FrontmatterCapture,
    evidence: &mut SourceConfigEvidence,
    control: &OperationControl,
) -> OperationControlResult<()> {
    let frontmatter = captured
        .full
        .zip(captured.body)
        .and_then(|(full, local_body)| {
            let full = source.try_map_span(SourceSpan::new(full.start, full.end))?;
            let body = map_frontmatter_body_span_to_original(source, local_body)?;
            // The mapped body end is the exact edit boundary used by the existing insertion
            // projection. For CRLF it intentionally sits between `\r` and `\n`; the projection
            // replaces that boundary with a complete CRLF-delimited config block.
            let insert = SourceSpan::new(body.end, body.end);
            Some(FrontmatterSourceEvidence::new(
                full,
                body,
                captured.indent.clone(),
                captured.has_config,
                captured.rewrite_safe,
                captured.fields.clone(),
            ))
            .map(|frontmatter| (frontmatter, insert))
        });
    if let Some((frontmatter, insert)) = frontmatter {
        evidence.set_config_insert_span(insert);
        evidence.set_frontmatter(frontmatter);
    } else if captured.full.is_some() {
        evidence.mark_rewrite_unsafe();
    }

    for (index, key) in captured.keys.iter().enumerate() {
        if index.is_multiple_of(128) {
            control.checkpoint()?;
        }
        let Some(local_span) = key.span else {
            continue;
        };
        let exact_span = source.try_map_span(local_span);
        let Some(span) = exact_span.or_else(|| source.try_map_enclosing_span(local_span)) else {
            continue;
        };
        evidence.push_key(SourceConfigKeyEvidence::new(
            SourceConfigOrigin::Frontmatter,
            key.path.clone(),
            span,
            evidence.keys().len(),
            key.rewrite_safe && exact_span.is_some(),
        ));
    }
    control.checkpoint()
}

fn map_frontmatter_body_span_to_original(
    source: &PreprocessedSource,
    body: FrontmatterByteSpan,
) -> Option<SourceSpan> {
    let mut mapped = source.try_map_span(SourceSpan::new(body.start, body.end))?;
    // The frontmatter locator intentionally leaves the `\r` from a CRLF immediately before the
    // closing delimiter inside the body range. Preprocessing normalizes that pair to one `\n`,
    // whose left boundary maps before the original `\r`. Recover the locator's original boundary
    // from the same edit map without rescanning or retaining source text.
    if source.text().as_bytes().get(body.end) == Some(&b'\n') {
        let newline = source.try_map_span(SourceSpan::new(body.end, body.end + 1))?;
        if newline.end.saturating_sub(newline.start) == 2 && mapped.end == newline.start {
            mapped.end = newline.end - 1;
        }
    }
    Some(mapped)
}

fn append_directive_evidence(
    source: &PreprocessedSource,
    captured: &ProcessedDirectives,
    evidence: &mut SourceConfigEvidence,
    control: &OperationControl,
) -> OperationControlResult<()> {
    for (index, directive) in captured.evidence.iter().enumerate() {
        if index.is_multiple_of(128) {
            control.checkpoint()?;
        }
        let Some(full_span) = source.try_map_span(directive.full_span) else {
            evidence.mark_rewrite_unsafe();
            continue;
        };
        let Some(keyword_span) = source.try_map_span(directive.keyword_span) else {
            evidence.mark_rewrite_unsafe();
            continue;
        };
        let directive_index = evidence.push_directive(SourceDirectiveEvidence::new(
            directive.keyword.clone(),
            full_span,
            keyword_span,
            evidence.directives().len(),
            directive.complete,
            directive.rewrite_safe,
        ));
        for key in &directive.keys {
            let Some(local_span) = key.span else {
                continue;
            };
            let exact_span = source.try_map_span(local_span);
            let Some(span) = exact_span.or_else(|| source.try_map_enclosing_span(local_span))
            else {
                continue;
            };
            evidence.push_key(SourceConfigKeyEvidence::new(
                SourceConfigOrigin::Directive { directive_index },
                key.path.clone(),
                span,
                evidence.keys().len(),
                key.rewrite_safe && exact_span.is_some(),
            ));
        }
    }
    if captured.recovered_incomplete_directive {
        evidence.mark_rewrite_unsafe();
    }
    control.checkpoint()
}

fn prepare_parser_code_controlled(
    mut preprocessed: PreprocessResult,
    control: &OperationControl,
) -> OperationControlResult<PreprocessResult> {
    preprocessed.source = prepare_parser_text_controlled(preprocessed.source, control)?;
    Ok(preprocessed)
}

pub(crate) fn prepare_parser_text_controlled(
    mut source: PreprocessedSource,
    control: &OperationControl,
) -> OperationControlResult<PreprocessedSource> {
    encode_mermaid_entities_like_upstream_controlled(&mut source, control)?;
    Ok(source)
}

fn cleanup_text_controlled(
    source: &mut PreprocessedSource,
    control: &OperationControl,
) -> OperationControlResult<()> {
    control.checkpoint()?;
    strip_leading_utf8_bom(source, control)?;
    normalize_crlf_controlled(source, control)?;

    // Mermaid performs this HTML attribute rewrite as part of preprocessing.
    normalize_html_tag_attributes_like_upstream_controlled(source, control)?;
    control.checkpoint()
}

fn strip_leading_utf8_bom(
    source: &mut PreprocessedSource,
    control: &OperationControl,
) -> OperationControlResult<()> {
    if source.text().starts_with('\u{feff}') {
        source.set_first_line_column_offset(1);
        source.apply_edits(vec![SourceEdit::delete(0..'\u{feff}'.len_utf8())], control)?;
    }
    Ok(())
}

fn remove_mermaid_comments_controlled(
    source: &mut PreprocessedSource,
    capture_editor_evidence: bool,
    control: &OperationControl,
) -> OperationControlResult<()> {
    let text = source.text();
    let mut checkpoints = ControlledScanCheckpoints::new(control)?;
    let mut edits = Vec::new();
    let mut comments = Vec::new();
    let mut line_start = 0usize;
    while line_start < text.len() {
        let newline = find_newline_controlled(text, line_start, &mut checkpoints)?;
        let line_end = newline.map_or(text.len(), |newline| newline + 1);
        let line = &text[line_start..line_end];
        let trimmed_start = trim_start_whitespace_controlled(line, &mut checkpoints)?;
        let trimmed = &line[trimmed_start..];
        if let Some(after_marker) = trimmed.strip_prefix("%%") {
            let has_comment_body = after_marker.chars().next().is_some_and(|ch| ch != '\n');
            if !after_marker.starts_with('{') && has_comment_body {
                let range = line_start..line_end;
                if capture_editor_evidence {
                    comments.push(SourceSpan::new(
                        range.start,
                        editor_evidence_line_end(text, range.end),
                    ));
                }
                edits.push(SourceEdit::delete(range));
            }
        }
        let Some(newline) = newline else {
            break;
        };
        line_start = newline + 1;
    }
    checkpoints.finish()?;
    for (index, expected_span) in comments.into_iter().enumerate() {
        if index.is_multiple_of(128) {
            control.checkpoint()?;
        }
        source.record_global_expected_syntax(EditorExpectedSyntaxKind::Directive, expected_span);
    }
    source.apply_edits(edits, control)?;
    source.set_first_line_column_offset(0);

    let mut checkpoints = ControlledScanCheckpoints::new(control)?;
    let leading_whitespace = trim_start_whitespace_controlled(source.text(), &mut checkpoints)?;
    checkpoints.finish()?;
    if leading_whitespace > 0 {
        source.apply_edits(vec![SourceEdit::delete(0..leading_whitespace)], control)?;
    }
    control.checkpoint()
}

fn editor_evidence_line_end(source: &str, end: usize) -> usize {
    let end = end.min(source.len());
    let newline_start = end
        .checked_sub(1)
        .filter(|index| source.as_bytes().get(*index) == Some(&b'\n'))
        .unwrap_or(end);
    line_content_end(source, newline_start)
}

fn normalize_crlf_controlled(
    source: &mut PreprocessedSource,
    control: &OperationControl,
) -> OperationControlResult<()> {
    let bytes = source.text().as_bytes();
    let mut checkpoints = ControlledScanCheckpoints::new(control)?;
    let mut edits = Vec::new();
    let mut cursor = 0usize;
    while cursor < bytes.len() {
        checkpoints.scanned(1)?;
        if bytes[cursor] == b'\r' {
            let end = cursor + usize::from(bytes.get(cursor + 1) == Some(&b'\n')) + 1;
            edits.push(SourceEdit::replace(
                cursor..end,
                "\n",
                if end - cursor == 1 {
                    ReplacementMapping::ExactBytes
                } else {
                    ReplacementMapping::Boundaries
                },
            ));
            cursor = end;
        } else {
            cursor += 1;
        }
    }
    checkpoints.finish()?;
    source.apply_edits(edits, control)?;
    control.checkpoint()
}

#[cfg(test)]
fn normalize_crlf(source: &mut PreprocessedSource) {
    normalize_crlf_controlled(source, &OperationControl::new())
        .expect("a private parse control cannot be cancelled");
}

fn normalize_html_tag_attributes_like_upstream_controlled(
    source: &mut PreprocessedSource,
    control: &OperationControl,
) -> OperationControlResult<()> {
    let text = source.text();
    let bytes = text.as_bytes();
    let mut checkpoints = ControlledScanCheckpoints::new(control)?;
    let mut probe = 0usize;
    let mut edits = Vec::new();

    while let Some(start) = find_ascii_pattern_controlled(text, probe, b"<", &mut checkpoints)? {
        let tag_start = start + 1;
        if tag_start >= bytes.len() || !is_mermaid_js_word_byte(bytes[tag_start]) {
            probe = tag_start;
            continue;
        }

        let mut tag_end = tag_start + 1;
        while tag_end < bytes.len() && is_mermaid_js_word_byte(bytes[tag_end]) {
            checkpoints.scanned(1)?;
            tag_end += 1;
        }

        let Some(end) = find_ascii_pattern_controlled(text, tag_end, b">", &mut checkpoints)?
        else {
            // No later `<` can form a closed tag when the remaining suffix has no `>`.
            // Advancing by one here made malformed input rescan the same suffix O(n^2).
            break;
        };

        html_attribute_quote_edits_controlled(text, tag_end, end, &mut edits, &mut checkpoints)?;

        probe = end + 1;
    }
    checkpoints.finish()?;
    source.apply_edits(edits, control)?;
    control.checkpoint()
}

#[cfg(test)]
fn normalize_html_tag_attributes_like_upstream(source: &mut PreprocessedSource) {
    normalize_html_tag_attributes_like_upstream_controlled(source, &OperationControl::new())
        .expect("a private parse control cannot be cancelled");
}

fn html_attribute_quote_edits_controlled(
    text: &str,
    attributes_start: usize,
    attributes_end: usize,
    edits: &mut Vec<SourceEdit>,
    checkpoints: &mut ControlledScanCheckpoints<'_>,
) -> OperationControlResult<()> {
    let mut probe = attributes_start;
    while let Some(start) =
        find_ascii_pattern_in_range_controlled(text, probe, attributes_end, b"=\"", checkpoints)?
    {
        let value_start = start + 2;
        let Some(end) = find_ascii_pattern_in_range_controlled(
            text,
            value_start,
            attributes_end,
            b"\"",
            checkpoints,
        )?
        else {
            probe = value_start;
            continue;
        };

        let opening_quote = start + 1;
        let closing_quote = end;
        edits.push(SourceEdit::replace(
            opening_quote..opening_quote + 1,
            "'",
            ReplacementMapping::ExactBytes,
        ));
        edits.push(SourceEdit::replace(
            closing_quote..closing_quote + 1,
            "'",
            ReplacementMapping::ExactBytes,
        ));

        probe = end + 1;
    }
    Ok(())
}

fn encode_mermaid_entities_like_upstream_controlled(
    source: &mut PreprocessedSource,
    control: &OperationControl,
) -> OperationControlResult<()> {
    // Mirrors Mermaid `encodeEntities` (Mermaid@11.12.2):
    //
    // 1) Protect `style...:#...;` and `classDef...:#...;` so color hex fragments are not mistaken
    //    as entities by the `/#\\w+;/g` pass.
    // 2) Encode `#<name>;` and `#<number>;` sequences into placeholders that do not contain `#`/`;`.
    strip_hex_style_semicolons_like_upstream_controlled(source, "style", control)?;
    strip_hex_style_semicolons_like_upstream_controlled(source, "classDef", control)?;
    encode_entity_placeholders_like_upstream_controlled(source, control)?;
    control.checkpoint()
}

#[cfg(test)]
fn encode_mermaid_entities_like_upstream(source: &mut PreprocessedSource) {
    encode_mermaid_entities_like_upstream_controlled(source, &OperationControl::new())
        .expect("a private parse control cannot be cancelled");
}

fn encode_entity_placeholders_like_upstream_controlled(
    source: &mut PreprocessedSource,
    control: &OperationControl,
) -> OperationControlResult<()> {
    let text = source.text();
    let bytes = text.as_bytes();
    let mut checkpoints = ControlledScanCheckpoints::new(control)?;
    let mut cursor = 0usize;
    let mut edits = Vec::new();

    while let Some(start) = find_ascii_pattern_controlled(text, cursor, b"#", &mut checkpoints)? {
        let mut end = start + 1;
        while end < bytes.len() && is_mermaid_js_word_byte(bytes[end]) {
            checkpoints.scanned(1)?;
            end += 1;
        }

        if end > start + 1 && bytes.get(end) == Some(&b';') {
            let inner = &text[start + 1..end];
            let mut all_digits = true;
            for byte in inner.bytes() {
                checkpoints.scanned(1)?;
                if !byte.is_ascii_digit() {
                    all_digits = false;
                    break;
                }
            }
            let prefix = if all_digits { "ﬂ°°" } else { "ﬂ°" };
            edits.push(SourceEdit::replace(
                start..start + 1,
                prefix,
                ReplacementMapping::Boundaries,
            ));
            edits.push(SourceEdit::replace(
                end..end + 1,
                "¶ß",
                ReplacementMapping::Boundaries,
            ));
            cursor = end + 1;
        } else {
            cursor = start + 1;
        }
    }
    checkpoints.finish()?;
    source.apply_edits(edits, control)?;
    control.checkpoint()
}

fn is_mermaid_js_word_byte(byte: u8) -> bool {
    byte.is_ascii_alphanumeric() || byte == b'_'
}

fn strip_hex_style_semicolons_like_upstream_controlled(
    source: &mut PreprocessedSource,
    keyword: &str,
    control: &OperationControl,
) -> OperationControlResult<()> {
    let text = source.text();
    let mut checkpoints = ControlledScanCheckpoints::new(control)?;
    let mut edits = Vec::new();
    let mut line_start = 0usize;

    loop {
        let newline = find_newline_controlled(text, line_start, &mut checkpoints)?;
        let line_end = newline.unwrap_or(text.len());
        collect_hex_style_semicolon_edits_controlled(
            text,
            line_start,
            line_end,
            keyword,
            &mut edits,
            &mut checkpoints,
        )?;
        let Some(newline) = newline else {
            break;
        };
        line_start = newline + 1;
    }

    checkpoints.finish()?;
    source.apply_edits(edits, control)?;
    control.checkpoint()
}

fn collect_hex_style_semicolon_edits_controlled(
    text: &str,
    line_start: usize,
    line_end: usize,
    keyword: &str,
    edits: &mut Vec<SourceEdit>,
    checkpoints: &mut ControlledScanCheckpoints<'_>,
) -> OperationControlResult<()> {
    let mut cursor = line_start;
    while let Some(semicolon) =
        find_hex_style_match_controlled(text, line_start, line_end, keyword, cursor, checkpoints)?
    {
        edits.push(SourceEdit::delete(semicolon..semicolon + 1));
        cursor = semicolon + 1;
    }
    Ok(())
}

fn find_hex_style_match_controlled(
    text: &str,
    line_start: usize,
    line_end: usize,
    keyword: &str,
    search_start: usize,
    checkpoints: &mut ControlledScanCheckpoints<'_>,
) -> OperationControlResult<Option<usize>> {
    let Some(start) = find_ascii_pattern_in_range_controlled(
        text,
        search_start.max(line_start),
        line_end,
        keyword.as_bytes(),
        checkpoints,
    )?
    else {
        return Ok(None);
    };
    find_hex_style_match_end_controlled(text, start + keyword.len(), line_end, checkpoints)
}

fn find_hex_style_match_end_controlled(
    text: &str,
    search_start: usize,
    line_end: usize,
    checkpoints: &mut ControlledScanCheckpoints<'_>,
) -> OperationControlResult<Option<usize>> {
    let mut probe = search_start;
    while let Some(colon) =
        find_ascii_pattern_in_range_controlled(text, probe, line_end, b":", checkpoints)?
    {
        let mut hash = None;
        let mut token_end = colon + 1;
        for (rel, ch) in text[colon + 1..line_end].char_indices() {
            checkpoints.scanned(ch.len_utf8())?;
            if ch.is_whitespace() {
                token_end = colon + 1 + rel;
                break;
            }
            token_end = colon + 1 + rel + ch.len_utf8();
            if ch == '#' {
                hash = Some(colon + 1 + rel);
                break;
            }
        }

        if let Some(hash) = hash {
            return rfind_ascii_byte_in_range_controlled(
                text,
                hash + 1,
                line_end,
                b';',
                checkpoints,
            );
        }

        // Skip the complete value token. Re-probing at every byte after a colon makes a
        // long colon-heavy line quadratic when no hexadecimal color is present.
        probe = token_end.max(colon + 1);
    }
    Ok(None)
}

fn rfind_ascii_byte_in_range_controlled(
    text: &str,
    start: usize,
    end: usize,
    needle: u8,
    checkpoints: &mut ControlledScanCheckpoints<'_>,
) -> OperationControlResult<Option<usize>> {
    for offset in (start..end.min(text.len())).rev() {
        checkpoints.scanned(1)?;
        if text.as_bytes()[offset] == needle {
            return Ok(Some(offset));
        }
    }
    Ok(None)
}

struct ProcessedFrontmatter {
    title: Option<String>,
    config: MermaidConfig,
}

struct FrontmatterCapture {
    result: Result<ProcessedFrontmatter>,
    full: Option<FrontmatterByteSpan>,
    body: Option<FrontmatterByteSpan>,
    indent: String,
    keys: Vec<LocalConfigKeyEvidence>,
    has_config: bool,
    rewrite_safe: bool,
    fields: Option<MermaidConfig>,
}

enum FrontmatterYamlInput<'a> {
    Plain(Cow<'a, str>),
    Mapped(Box<PreprocessedSource>),
}

impl FrontmatterYamlInput<'_> {
    fn text(&self) -> &str {
        match self {
            Self::Plain(text) => text.as_ref(),
            Self::Mapped(source) => source.text(),
        }
    }

    fn try_map_span_to_body(&self, span: SourceSpan) -> Option<SourceSpan> {
        match self {
            Self::Plain(_) => Some(span),
            Self::Mapped(source) => source.try_map_span(span),
        }
    }

    fn try_map_enclosing_span_to_body(&self, span: SourceSpan) -> Option<SourceSpan> {
        match self {
            Self::Plain(_) => Some(span),
            Self::Mapped(source) => source.try_map_enclosing_span(span),
        }
    }
}

fn map_frontmatter_yaml_span_to_preprocessed_source(
    yaml_input: &FrontmatterYamlInput<'_>,
    body_start: usize,
    span: std::ops::Range<usize>,
) -> Option<(SourceSpan, bool)> {
    let parser_span = SourceSpan::new(span.start, span.end);
    let exact = yaml_input.try_map_span_to_body(parser_span);
    let body_span = exact.or_else(|| yaml_input.try_map_enclosing_span_to_body(parser_span))?;
    Some((
        SourceSpan::new(
            body_start.checked_add(body_span.start)?,
            body_start.checked_add(body_span.end)?,
        ),
        exact.is_some(),
    ))
}

fn process_frontmatter_controlled(
    input: &str,
    capture_mode: SourceConfigCaptureMode,
    control: &OperationControl,
) -> OperationControlResult<FrontmatterCapture> {
    control.checkpoint()?;
    let Some(location) = locate_frontmatter_block_controlled(input, control)? else {
        return Ok(FrontmatterCapture {
            result: Ok(ProcessedFrontmatter {
                title: None,
                config: MermaidConfig::empty_object(),
            }),
            full: None,
            body: None,
            indent: String::new(),
            keys: Vec::new(),
            has_config: false,
            rewrite_safe: true,
            fields: None,
        });
    };
    let body = &input[location.body.start..location.body.end];
    let yaml_input =
        dedented_frontmatter_yaml_input_controlled(body, location.indent, capture_mode, control)?;
    let yaml_body = yaml_input.text();
    let mut keys = Vec::new();
    let mut rewrite_safe = true;

    if config_nesting_exceeds_limit_controlled(yaml_body, control)? {
        return Ok(FrontmatterCapture {
            result: Err(Error::InvalidFrontMatterYaml {
                message: format!("config nesting exceeds {MAX_CONFIG_NESTING_DEPTH} levels"),
            }),
            full: Some(location.full),
            body: Some(location.body),
            indent: location.indent.to_string(),
            keys,
            has_config: false,
            rewrite_safe: false,
            fields: None,
        });
    }

    control.checkpoint()?;
    let parsed = if capture_mode.collects() {
        let captured = crate::yaml_config::parse_yaml_value_capture_controlled(
            yaml_body,
            MAX_CONFIG_NESTING_DEPTH,
            control,
        )?;
        rewrite_safe = captured.rewrite_safe;
        keys.reserve(captured.keys.len());
        for key in captured.keys {
            // Compose exactly once through each coordinate space: parser span in the dedented
            // YAML input -> body-relative source -> cleaned preprocess source. The outer source
            // edit map is applied later when the operation evidence is appended.
            let mapped = key.span.and_then(|span| {
                map_frontmatter_yaml_span_to_preprocessed_source(
                    &yaml_input,
                    location.body.start,
                    span,
                )
            });
            let (span, exact) = mapped.map_or((None, false), |(span, exact)| (Some(span), exact));
            let rewrite_safe = key.rewrite_safe && exact;
            keys.push(LocalConfigKeyEvidence {
                path: key.path,
                span,
                rewrite_safe,
            });
        }
        captured.value
    } else {
        crate::yaml_config::parse_yaml_value_controlled(
            yaml_body,
            MAX_CONFIG_NESTING_DEPTH,
            control,
        )?
    };
    let parsed_obj = match frontmatter_fields_from_yaml_value(parsed) {
        Ok(parsed) => parsed,
        Err(message) => {
            return Ok(FrontmatterCapture {
                result: Err(Error::InvalidFrontMatterYaml { message }),
                full: Some(location.full),
                body: Some(location.body),
                indent: location.indent.to_string(),
                has_config: keys.iter().any(|key| key.path.first() == Some("config")),
                keys,
                rewrite_safe: false,
                fields: None,
            });
        }
    };
    if let Err(cancelled) = control.checkpoint() {
        crate::config::drop_value_nonrecursive(Value::Object(parsed_obj));
        return Err(cancelled);
    }

    let mut title = None;
    let mut display_mode = None;

    if let Some(t) = parsed_obj
        .get("title")
        .filter(|value| frontmatter_truthy(value))
    {
        title = Some(frontmatter_to_string(t));
    }
    if let Some(dm) = parsed_obj
        .get("displayMode")
        .filter(|value| frontmatter_truthy(value))
    {
        display_mode = Some(frontmatter_to_string(dm));
    }

    let mut config = MermaidConfig::empty_object();
    merge_top_level_frontmatter_diagram_configs(&parsed_obj, &mut config);
    if let Some(v) = parsed_obj
        .get("config")
        .filter(|value| frontmatter_truthy(value))
    {
        config.deep_merge(v);
    }
    crate::config::mirror_legacy_font_family_into_theme_variables(&mut config);
    if let Some(dm) = display_mode {
        config.set_value("gantt.displayMode", Value::String(dm));
    }

    let has_config = parsed_obj.contains_key("config");
    let fields = if capture_mode.collects() && has_config && rewrite_safe {
        Some(MermaidConfig::from_value(Value::Object(parsed_obj)))
    } else {
        crate::config::drop_value_nonrecursive(Value::Object(parsed_obj));
        None
    };
    control.checkpoint()?;
    Ok(FrontmatterCapture {
        result: Ok(ProcessedFrontmatter { title, config }),
        full: Some(location.full),
        body: Some(location.body),
        indent: location.indent.to_string(),
        keys,
        has_config,
        rewrite_safe,
        fields,
    })
}

fn dedented_frontmatter_yaml_input_controlled<'a>(
    body: &'a str,
    indent: &str,
    capture_mode: SourceConfigCaptureMode,
    control: &OperationControl,
) -> OperationControlResult<FrontmatterYamlInput<'a>> {
    if !capture_mode.collects() {
        return dedent_frontmatter_body_controlled(body, indent, control)
            .map(FrontmatterYamlInput::Plain);
    }
    if indent.is_empty() {
        control.checkpoint()?;
        return Ok(FrontmatterYamlInput::Plain(Cow::Borrowed(body)));
    }

    let mut source = PreprocessedSource::new_controlled(body, control)?;
    let mut checkpoints = ControlledScanCheckpoints::new(control)?;
    let mut edits = Vec::new();
    let mut line_start = 0usize;
    loop {
        let line_ending = find_physical_line_ending_controlled(body, line_start, &mut checkpoints)?;
        let (line_end, next_line_start) = line_ending.unwrap_or((body.len(), body.len()));
        let line = &body[line_start..line_end];
        if frontmatter_has_prefix_controlled(line, indent, &mut checkpoints)? {
            edits.push(SourceEdit::delete(line_start..line_start + indent.len()));
        }
        if line_ending.is_none() || next_line_start == body.len() {
            break;
        }
        line_start = next_line_start;
    }
    checkpoints.finish()?;
    source.apply_edits(edits, control)?;
    Ok(FrontmatterYamlInput::Mapped(Box::new(source)))
}

/// Splits an optional frontmatter block using a private, non-cancellable parse control.
pub fn split_frontmatter_block(input: &str) -> Option<FrontmatterBlock<'_>> {
    let control = OperationControl::new();
    split_frontmatter_block_controlled(input, &control)
        .expect("a private parse control cannot be cancelled")
}

/// Locates an optional frontmatter block without allocating a dedented body.
pub fn locate_frontmatter_block_controlled<'a>(
    input: &'a str,
    control: &OperationControl,
) -> OperationControlResult<Option<FrontmatterBlockLocation<'a>>> {
    let mut checkpoints = ControlledScanCheckpoints::new(control)?;
    let indent_end = frontmatter_indent_end_controlled(input, &mut checkpoints)?;
    let indent = &input[..indent_end];
    if !input[indent_end..].starts_with("---") {
        checkpoints.finish()?;
        return Ok(None);
    }

    // Match Mermaid's greedy opening whitespace and lazy body without regex backtracking.
    let mut opening_ends = Vec::new();
    for (offset, ch) in input[indent_end + 3..].char_indices() {
        checkpoints.scanned(ch.len_utf8())?;
        if !is_frontmatter_whitespace(ch) {
            break;
        }
        if matches!(ch, '\n' | '\r') {
            opening_ends.push(indent_end + 3 + offset);
        }
    }
    let Some(&first_opening_end) = opening_ends.first() else {
        checkpoints.finish()?;
        return Ok(None);
    };
    let earliest_body_start = first_opening_end + 1;
    let mut last_closing_fence = None;
    for (offset, ch) in input[earliest_body_start..].char_indices() {
        checkpoints.scanned(ch.len_utf8())?;
        let position = earliest_body_start + offset;
        if matches!(ch, '\n' | '\r')
            && frontmatter_closing_end_controlled(input, position, indent, &mut checkpoints)?
                .is_some()
        {
            last_closing_fence = Some(position);
        }
    }
    let Some(last_closing_fence) = last_closing_fence else {
        checkpoints.finish()?;
        return Ok(None);
    };
    let body_start = opening_ends
        .into_iter()
        .rfind(|&end| end < last_closing_fence)
        .unwrap_or(first_opening_end)
        + 1;
    for (offset, ch) in input[body_start..=last_closing_fence].char_indices() {
        checkpoints.scanned(ch.len_utf8())?;
        if !matches!(ch, '\n' | '\r') {
            continue;
        }
        let body_end = body_start + offset;
        if let Some(end) =
            frontmatter_closing_end_controlled(input, body_end, indent, &mut checkpoints)?
        {
            checkpoints.finish()?;
            return Ok(Some(FrontmatterBlockLocation {
                full: FrontmatterByteSpan { start: 0, end },
                body: FrontmatterByteSpan {
                    start: body_start,
                    end: body_end,
                },
                indent,
                stripped: &input[end..],
            }));
        }
    }
    checkpoints.finish()?;
    Ok(None)
}

/// Splits an optional frontmatter block while observing cooperative cancellation.
pub fn split_frontmatter_block_controlled<'a>(
    input: &'a str,
    control: &OperationControl,
) -> OperationControlResult<Option<FrontmatterBlock<'a>>> {
    let Some(location) = locate_frontmatter_block_controlled(input, control)? else {
        return Ok(None);
    };
    let body = &input[location.body.start..location.body.end];
    let dedented_body = dedent_frontmatter_body_controlled(body, location.indent, control)?;
    control.checkpoint()?;
    Ok(Some(FrontmatterBlock {
        full: location.full,
        body: location.body,
        indent: location.indent,
        dedented_body,
        stripped: location.stripped,
    }))
}

pub fn parse_frontmatter_yaml_fields(
    input: &str,
) -> std::result::Result<Map<String, Value>, String> {
    let control = OperationControl::new();
    parse_frontmatter_yaml_fields_controlled(input, &control)
        .expect("a private parse control cannot be cancelled")
}

/// Parses frontmatter YAML fields while observing cooperative cancellation.
pub fn parse_frontmatter_yaml_fields_controlled(
    input: &str,
    control: &OperationControl,
) -> OperationControlResult<std::result::Result<Map<String, Value>, String>> {
    let parsed =
        crate::yaml_config::parse_yaml_value_controlled(input, MAX_CONFIG_NESTING_DEPTH, control)?;
    Ok(frontmatter_fields_from_yaml_value(parsed))
}

/// Parses frontmatter fields with caller-owned nesting and materialization budgets.
pub fn parse_frontmatter_yaml_fields_bounded_controlled(
    input: &str,
    max_input_bytes: usize,
    max_nesting_depth: usize,
    max_materialized_bytes: usize,
    control: &OperationControl,
) -> OperationControlResult<std::result::Result<Map<String, Value>, String>> {
    let parsed = crate::yaml_config::parse_yaml_value_with_limits_controlled(
        input,
        max_input_bytes,
        max_nesting_depth,
        max_materialized_bytes,
        control,
    )?;
    Ok(frontmatter_fields_from_yaml_value(parsed))
}

fn frontmatter_fields_from_yaml_value(
    parsed: std::result::Result<Value, String>,
) -> std::result::Result<Map<String, Value>, String> {
    let parsed = parsed?;
    match parsed {
        Value::Object(map) => Ok(map),
        other => {
            crate::config::drop_value_nonrecursive(other);
            Ok(Map::new())
        }
    }
}

pub fn diagram_config_key_for_type(diagram_type: &str) -> &str {
    crate::family::config_namespace_for_diagram_type(diagram_type).unwrap_or(diagram_type)
}

struct ControlledScanCheckpoints<'a> {
    control: &'a OperationControl,
    bytes_since_checkpoint: usize,
}

impl<'a> ControlledScanCheckpoints<'a> {
    fn new(control: &'a OperationControl) -> OperationControlResult<Self> {
        control.checkpoint()?;
        Ok(Self {
            control,
            bytes_since_checkpoint: 0,
        })
    }

    fn scanned(&mut self, bytes: usize) -> OperationControlResult<()> {
        self.bytes_since_checkpoint = self.bytes_since_checkpoint.saturating_add(bytes);
        while self.bytes_since_checkpoint >= CONTROLLED_SCAN_CHECKPOINT_BYTES {
            self.control.checkpoint()?;
            self.bytes_since_checkpoint -= CONTROLLED_SCAN_CHECKPOINT_BYTES;
        }
        Ok(())
    }

    fn finish(&self) -> OperationControlResult<()> {
        self.control.checkpoint()
    }
}

fn find_newline_controlled(
    input: &str,
    start: usize,
    checkpoints: &mut ControlledScanCheckpoints<'_>,
) -> OperationControlResult<Option<usize>> {
    for (offset, byte) in input.as_bytes()[start..].iter().enumerate() {
        checkpoints.scanned(1)?;
        if *byte == b'\n' {
            return Ok(Some(start + offset));
        }
    }
    Ok(None)
}

fn find_physical_line_ending_controlled(
    input: &str,
    start: usize,
    checkpoints: &mut ControlledScanCheckpoints<'_>,
) -> OperationControlResult<Option<(usize, usize)>> {
    let bytes = input.as_bytes();
    for offset in start..bytes.len() {
        checkpoints.scanned(1)?;
        match bytes[offset] {
            b'\r' => {
                let end = offset + 1 + usize::from(bytes.get(offset + 1) == Some(&b'\n'));
                if end == offset + 2 {
                    checkpoints.scanned(1)?;
                }
                return Ok(Some((offset, end)));
            }
            b'\n' => return Ok(Some((offset, offset + 1))),
            _ => {}
        }
    }
    Ok(None)
}

fn frontmatter_indent_end_controlled(
    line: &str,
    checkpoints: &mut ControlledScanCheckpoints<'_>,
) -> OperationControlResult<usize> {
    let mut end = 0usize;
    for (idx, ch) in line.char_indices() {
        checkpoints.scanned(ch.len_utf8())?;
        if ch == '\n' || ch == '\r' || !is_frontmatter_whitespace(ch) {
            break;
        }
        end = idx + ch.len_utf8();
    }
    Ok(end)
}

fn is_frontmatter_whitespace(ch: char) -> bool {
    (ch.is_whitespace() && ch != '\u{0085}') || ch == '\u{feff}'
}

fn frontmatter_closing_end_controlled(
    input: &str,
    preceding_line_break: usize,
    indent: &str,
    checkpoints: &mut ControlledScanCheckpoints<'_>,
) -> OperationControlResult<Option<usize>> {
    let fence_start = preceding_line_break + 1;
    if !frontmatter_has_prefix_controlled(&input[fence_start..], indent, checkpoints)? {
        return Ok(None);
    }
    let fence_end = fence_start + indent.len();
    if !input[fence_end..].starts_with("---") {
        return Ok(None);
    }
    let whitespace_start = fence_end + 3;
    let mut end = None;
    for (offset, ch) in input[whitespace_start..].char_indices() {
        checkpoints.scanned(ch.len_utf8())?;
        if !is_frontmatter_whitespace(ch) {
            break;
        }
        if matches!(ch, '\n' | '\r') {
            end = Some(whitespace_start + offset + 1);
        }
    }
    Ok(end)
}

fn frontmatter_has_prefix_controlled(
    text: &str,
    prefix: &str,
    checkpoints: &mut ControlledScanCheckpoints<'_>,
) -> OperationControlResult<bool> {
    if text.len() < prefix.len() {
        return Ok(false);
    }
    for (actual, expected) in text.as_bytes()[..prefix.len()]
        .iter()
        .zip(prefix.as_bytes())
    {
        checkpoints.scanned(1)?;
        if actual != expected {
            return Ok(false);
        }
    }
    Ok(true)
}

fn dedent_frontmatter_body_controlled<'a>(
    body: &'a str,
    indent: &str,
    control: &OperationControl,
) -> OperationControlResult<Cow<'a, str>> {
    if indent.is_empty() {
        control.checkpoint()?;
        return Ok(Cow::Borrowed(body));
    }

    let mut checkpoints = ControlledScanCheckpoints::new(control)?;
    let mut out = String::with_capacity(body.len());
    let mut line_start = 0usize;
    loop {
        let line_ending = find_physical_line_ending_controlled(body, line_start, &mut checkpoints)?;
        let (line_end, next_line_start) = line_ending.unwrap_or((body.len(), body.len()));
        let line = &body[line_start..line_end];
        let content = if frontmatter_has_prefix_controlled(line, indent, &mut checkpoints)? {
            &line[indent.len()..]
        } else {
            line
        };
        push_frontmatter_str_controlled(&mut out, content, &mut checkpoints)?;
        if line_end == body.len() {
            break;
        }
        push_frontmatter_str_controlled(
            &mut out,
            &body[line_end..next_line_start],
            &mut checkpoints,
        )?;
        if next_line_start == body.len() {
            break;
        }
        line_start = next_line_start;
    }
    checkpoints.finish()?;
    Ok(Cow::Owned(out))
}

fn push_frontmatter_str_controlled(
    out: &mut String,
    text: &str,
    checkpoints: &mut ControlledScanCheckpoints<'_>,
) -> OperationControlResult<()> {
    let mut chunk_start = 0usize;
    let mut chunk_len = 0usize;
    for (idx, ch) in text.char_indices() {
        let ch_len = ch.len_utf8();
        checkpoints.scanned(ch_len)?;
        chunk_len += ch_len;
        if chunk_len >= CONTROLLED_SCAN_CHECKPOINT_BYTES {
            let chunk_end = idx + ch_len;
            out.push_str(&text[chunk_start..chunk_end]);
            chunk_start = chunk_end;
            chunk_len = 0;
        }
    }
    out.push_str(&text[chunk_start..]);
    Ok(())
}

fn frontmatter_truthy(value: &Value) -> bool {
    match value {
        Value::Null => false,
        Value::Bool(value) => *value,
        Value::Number(value) => value.as_f64().is_some_and(|value| value != 0.0),
        Value::String(value) => !value.is_empty(),
        Value::Array(_) | Value::Object(_) => true,
    }
}

fn frontmatter_to_string(value: &Value) -> String {
    match value {
        Value::String(value) => value.clone(),
        Value::Number(value) => value.to_string(),
        Value::Bool(true) => "true".to_string(),
        Value::Bool(false) => "false".to_string(),
        Value::Null => "null".to_string(),
        Value::Array(_) | Value::Object(_) => value.to_string(),
    }
}

fn merge_top_level_frontmatter_diagram_configs(
    parsed_obj: &serde_json::Map<String, Value>,
    config: &mut MermaidConfig,
) {
    // Mermaid upstream only consumes `config`, but users commonly read docs examples as allowing
    // diagram config namespaces at the YAML root. Keep this compatibility narrow and explicit.
    for fact in crate::family::frontmatter_config_aliases() {
        if let Some(value) = parsed_obj.get(fact.source) {
            config.set_value(
                fact.namespace,
                crate::config::clone_value_nonrecursive(value),
            );
        }
    }

    for &key in crate::family::frontmatter_config_namespaces() {
        if let Some(value) = parsed_obj.get(key) {
            config.set_value(key, crate::config::clone_value_nonrecursive(value));
        }
    }
}

struct ProcessedDirectives {
    config: MermaidConfig,
    removals: Vec<std::ops::Range<usize>>,
    editor_prefixes: Vec<String>,
    recovered_incomplete_directive: bool,
    evidence: Vec<LocalDirectiveEvidence>,
    error: Option<Error>,
    panic: Option<CapturedPanic>,
}

struct LocalDirectiveEvidence {
    keyword: String,
    full_span: SourceSpan,
    keyword_span: SourceSpan,
    complete: bool,
    rewrite_safe: bool,
    keys: Vec<LocalConfigKeyEvidence>,
}

fn process_directives_controlled(
    input: &str,
    registry: &DetectorRegistry,
    diagram_type: Option<&str>,
    directive_recovery: DirectiveRecoveryMode,
    capture_mode: SourceConfigCaptureMode,
    evaluate_config: bool,
    control: &OperationControl,
) -> OperationControlResult<ProcessedDirectives> {
    control.checkpoint()?;
    let blocks = directive_blocks_controlled(input, directive_recovery, control)?;
    if blocks.is_empty() {
        return Ok(ProcessedDirectives {
            config: MermaidConfig::empty_object(),
            removals: Vec::new(),
            editor_prefixes: Vec::new(),
            recovered_incomplete_directive: false,
            evidence: Vec::new(),
            error: None,
            panic: None,
        });
    }
    let recovered_incomplete_directive = blocks.iter().any(|block| !block.complete);
    let mut directives = Vec::new();
    let mut evidence = Vec::new();
    let mut first_error = None;
    for (index, block) in blocks.iter().enumerate() {
        if index % 32 == 0 {
            control.checkpoint()?;
        }
        if !block.complete && !capture_mode.collects() {
            continue;
        }
        let parsed = parse_directive_like_upstream_capture_controlled(
            block.raw,
            capture_mode,
            block.complete,
            control,
        )?;
        if let Some(directive) = parsed.directive {
            if capture_mode.collects() {
                let keyword_span = SourceSpan::new(
                    block.raw_start.saturating_add(directive.keyword_span.start),
                    block.raw_start.saturating_add(directive.keyword_span.end),
                );
                let keys = directive
                    .config_keys
                    .iter()
                    .map(|key| LocalConfigKeyEvidence {
                        path: key.path.clone(),
                        span: key.span.as_ref().map(|span| {
                            SourceSpan::new(
                                block.raw_start.saturating_add(span.start),
                                block.raw_start.saturating_add(span.end),
                            )
                        }),
                        rewrite_safe: key.rewrite_safe,
                    })
                    .collect();
                evidence.push(LocalDirectiveEvidence {
                    keyword: directive.ty.clone(),
                    full_span: SourceSpan::new(block.range.start, block.range.end),
                    keyword_span,
                    complete: block.complete,
                    rewrite_safe: block.complete && directive.rewrite_safe,
                    keys,
                });
            }
            if block.complete && parsed.error.is_none() {
                directives.push(directive);
            }
        }
        if block.complete && first_error.is_none() {
            first_error = parsed.error;
        }
        if first_error.is_some() && !capture_mode.collects() {
            return Ok(ProcessedDirectives {
                config: MermaidConfig::empty_object(),
                removals: Vec::new(),
                editor_prefixes: Vec::new(),
                recovered_incomplete_directive,
                evidence,
                error: first_error,
                panic: None,
            });
        }
    }
    control.checkpoint()?;
    let input_without_directives = remove_directive_blocks_controlled(input, &blocks, control)?;
    let wrap = directives.iter().any(|d| d.ty == "wrap");
    let mut editor_prefixes = Vec::new();
    for (index, directive) in directives.iter().enumerate() {
        if index % 32 == 0 {
            control.checkpoint()?;
        }
        if matches!(directive.ty.as_str(), "init" | "initialize" | "wrap")
            && !editor_prefixes.contains(&directive.ty)
        {
            editor_prefixes.push(directive.ty.clone());
        }
    }

    let detected_init = if !evaluate_config || first_error.is_some() {
        Ok(MermaidConfig::empty_object())
    } else if capture_mode.collects() {
        match std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            detect_init_controlled(
                &directives,
                input_without_directives.as_ref(),
                registry,
                diagram_type,
                control,
            )
        })) {
            Ok(result) => result?,
            Err(payload) => {
                return Ok(ProcessedDirectives {
                    config: MermaidConfig::empty_object(),
                    removals: blocks.into_iter().map(|block| block.range).collect(),
                    editor_prefixes,
                    recovered_incomplete_directive,
                    evidence,
                    error: first_error,
                    panic: Some(CapturedPanic::from_payload(payload)),
                });
            }
        }
    } else {
        detect_init_controlled(
            &directives,
            input_without_directives.as_ref(),
            registry,
            diagram_type,
            control,
        )?
    };
    let init = match detected_init {
        Ok(init) => init,
        Err(error) => {
            if first_error.is_none() {
                first_error = Some(error);
            }
            MermaidConfig::empty_object()
        }
    };
    let mut merged = init;
    if wrap {
        merged.set_value("wrap", Value::Bool(true));
    }

    control.checkpoint()?;
    Ok(ProcessedDirectives {
        config: merged,
        removals: blocks.into_iter().map(|block| block.range).collect(),
        editor_prefixes,
        recovered_incomplete_directive,
        evidence,
        error: first_error,
        panic: None,
    })
}

#[cfg(test)]
fn detect_init(
    directives: &[Directive],
    input: &str,
    registry: &DetectorRegistry,
    diagram_type: Option<&str>,
) -> Result<MermaidConfig> {
    let control = OperationControl::new();
    detect_init_controlled(directives, input, registry, diagram_type, &control)
        .expect("a private parse control cannot be cancelled")
}

fn detect_init_controlled(
    directives: &[Directive],
    input: &str,
    registry: &DetectorRegistry,
    diagram_type: Option<&str>,
    control: &OperationControl,
) -> OperationControlResult<Result<MermaidConfig>> {
    control.checkpoint()?;
    let mut merged = MermaidConfig::empty_object();
    let mut config_for_detect = MermaidConfig::empty_object();
    let mut detected_type = diagram_type.map(str::to_owned);
    let mut detection_attempted = diagram_type.is_some();

    for (index, d) in directives.iter().enumerate() {
        if index % 16 == 0 {
            control.checkpoint()?;
        }
        if d.ty != "init" && d.ty != "initialize" {
            continue;
        }

        let mut args = match &d.args {
            Some(v) => crate::config::clone_value_nonrecursive(v),
            None => Value::Object(Default::default()),
        };
        let mut diagram_specific = args
            .as_object_mut()
            .and_then(|object| object.remove("config"));

        sanitize_directive_controlled(&mut args, control)?;

        // Mermaid moves a top-level `config` directive field into the diagram-type-specific config.
        if let Some(mut diagram_specific_value) = diagram_specific.take() {
            sanitize_directive_controlled(&mut diagram_specific_value, control)?;
            if !detection_attempted {
                detection_attempted = true;
                detected_type = match registry.detect_type_controlled(
                    input,
                    &mut config_for_detect,
                    control,
                )? {
                    Ok(diagram_type) => Some(diagram_type.to_string()),
                    Err(_) => None,
                };
            }

            if let Some(ty) = detected_type.as_deref() {
                let key = diagram_config_key_for_type(ty).to_string();
                if let Value::Object(obj) = &mut args
                    && let Some(old) = obj.insert(key, diagram_specific_value)
                {
                    crate::config::drop_value_nonrecursive(old);
                }
            } else {
                crate::config::drop_value_nonrecursive(diagram_specific_value);
            }
        }
        crate::config::mirror_legacy_font_family_into_theme_variables_value(&mut args);

        merged.deep_merge(&args);
    }

    control.checkpoint()?;
    Ok(Ok(merged))
}

#[derive(Debug, Clone)]
struct Directive {
    ty: String,
    args: Option<Value>,
    keyword_span: std::ops::Range<usize>,
    config_keys: Vec<source_config::Json5ConfigKeyEvidence>,
    rewrite_safe: bool,
}

#[derive(Debug)]
struct DirectiveBlock<'a> {
    raw: &'a str,
    raw_start: usize,
    complete: bool,
    range: std::ops::Range<usize>,
}

fn remove_directive_blocks_controlled<'a>(
    input: &'a str,
    blocks: &[DirectiveBlock<'_>],
    control: &OperationControl,
) -> OperationControlResult<Cow<'a, str>> {
    if blocks.is_empty() {
        control.checkpoint()?;
        return Ok(Cow::Borrowed(input));
    }

    let mut retained_bytes = input.len();
    for (index, block) in blocks.iter().enumerate() {
        if index.is_multiple_of(32) {
            control.checkpoint()?;
        }
        retained_bytes = retained_bytes.saturating_sub(block.range.len());
    }
    let mut output = String::with_capacity(retained_bytes);
    let mut checkpoints = ControlledScanCheckpoints::new(control)?;
    let mut cursor = 0usize;
    for block in blocks {
        push_frontmatter_str_controlled(
            &mut output,
            &input[cursor..block.range.start],
            &mut checkpoints,
        )?;
        cursor = block.range.end;
    }
    push_frontmatter_str_controlled(&mut output, &input[cursor..], &mut checkpoints)?;
    checkpoints.finish()?;
    Ok(Cow::Owned(output))
}

#[cfg(test)]
fn directive_blocks(
    input: &str,
    directive_recovery: DirectiveRecoveryMode,
) -> Vec<DirectiveBlock<'_>> {
    let control = OperationControl::new();
    directive_blocks_controlled(input, directive_recovery, &control)
        .expect("a private parse control cannot be cancelled")
}

fn directive_blocks_controlled<'a>(
    input: &'a str,
    directive_recovery: DirectiveRecoveryMode,
    control: &OperationControl,
) -> OperationControlResult<Vec<DirectiveBlock<'a>>> {
    let mut checkpoints = ControlledScanCheckpoints::new(control)?;
    let mut blocks = Vec::new();
    let mut pos = 0;
    while let Some(start) = find_ascii_pattern_controlled(input, pos, b"%%{", &mut checkpoints)? {
        let content_start = start + 3;
        let close = find_ascii_pattern_controlled(input, content_start, b"}%%", &mut checkpoints)?;
        let next_open = if directive_recovery == DirectiveRecoveryMode::RecoverLine {
            find_ascii_pattern_controlled(input, content_start, b"%%{", &mut checkpoints)?
        } else {
            None
        };

        let content_end = match directive_recovery {
            DirectiveRecoveryMode::Strict => close,
            DirectiveRecoveryMode::RecoverLine => {
                close.filter(|close| next_open.is_none_or(|open| *close < open))
            }
        };
        if let Some(content_end) = content_end {
            let end = content_end + 3;
            let raw = &input[content_start..content_end];
            let (trimmed_start, trimmed_end) =
                trim_whitespace_bounds_controlled(raw, &mut checkpoints)?;
            blocks.push(DirectiveBlock {
                raw: &raw[trimmed_start..trimmed_end],
                raw_start: content_start + trimmed_start,
                complete: true,
                range: start..end,
            });
            pos = end;
            continue;
        }

        let end = match directive_recovery {
            // Mermaid's optional closing marker makes an unterminated directive consume to EOF.
            DirectiveRecoveryMode::Strict => input.len(),
            DirectiveRecoveryMode::RecoverLine => {
                let line_end = find_line_break_controlled(input, content_start, &mut checkpoints)?
                    .unwrap_or(input.len());
                next_open.map_or(line_end, |open| line_end.min(open))
            }
        };
        let raw = &input[content_start..end];
        let (trimmed_start, trimmed_end) =
            trim_whitespace_bounds_controlled(raw, &mut checkpoints)?;
        blocks.push(DirectiveBlock {
            raw: &raw[trimmed_start..trimmed_end],
            raw_start: content_start + trimmed_start,
            complete: false,
            range: start..end,
        });
        pos = end;
    }

    checkpoints.finish()?;
    Ok(blocks)
}

fn find_ascii_pattern_controlled(
    input: &str,
    start: usize,
    pattern: &[u8],
    checkpoints: &mut ControlledScanCheckpoints<'_>,
) -> OperationControlResult<Option<usize>> {
    find_ascii_pattern_in_range_controlled(input, start, input.len(), pattern, checkpoints)
}

fn find_ascii_pattern_in_range_controlled(
    input: &str,
    start: usize,
    end: usize,
    pattern: &[u8],
    checkpoints: &mut ControlledScanCheckpoints<'_>,
) -> OperationControlResult<Option<usize>> {
    let end = end.min(input.len());
    if pattern.is_empty() || start > end || pattern.len() > end.saturating_sub(start) {
        return Ok(None);
    }
    let bytes = input.as_bytes();
    let last_start = end - pattern.len();
    for cursor in start..=last_start {
        checkpoints.scanned(1)?;
        if &bytes[cursor..cursor + pattern.len()] == pattern {
            return Ok(Some(cursor));
        }
    }
    Ok(None)
}

fn find_line_break_controlled(
    input: &str,
    start: usize,
    checkpoints: &mut ControlledScanCheckpoints<'_>,
) -> OperationControlResult<Option<usize>> {
    for (offset, byte) in input.as_bytes()[start..].iter().copied().enumerate() {
        checkpoints.scanned(1)?;
        if matches!(byte, b'\r' | b'\n') {
            return Ok(Some(start + offset));
        }
    }
    Ok(None)
}

#[cfg(test)]
fn parse_directive_like_upstream(raw: &str) -> Result<Option<Directive>> {
    let control = OperationControl::new();
    parse_directive_like_upstream_controlled(raw, SourceConfigCaptureMode::Omit, &control)
        .expect("a private parse control cannot be cancelled")
}

#[cfg(test)]
fn parse_directive_like_upstream_controlled(
    raw: &str,
    capture_mode: SourceConfigCaptureMode,
    control: &OperationControl,
) -> OperationControlResult<Result<Option<Directive>>> {
    let captured =
        parse_directive_like_upstream_capture_controlled(raw, capture_mode, true, control)?;
    Ok(match captured.error {
        Some(error) => Err(error),
        None => Ok(captured.directive),
    })
}

struct DirectiveParseCapture {
    directive: Option<Directive>,
    error: Option<Error>,
}

fn parse_directive_like_upstream_capture_controlled(
    raw: &str,
    capture_mode: SourceConfigCaptureMode,
    parse_arguments: bool,
    control: &OperationControl,
) -> OperationControlResult<DirectiveParseCapture> {
    // Incomplete authoring evidence only needs the directive keyword. Avoid walking an
    // arbitrarily large unfinished argument merely to normalize quotes that will not be parsed.
    if !parse_arguments {
        return parse_directive_capture_controlled(raw, capture_mode, false, control);
    }
    let normalized = normalize_directive_quotes_controlled(raw, control)?;
    parse_directive_capture_controlled(normalized.as_ref(), capture_mode, true, control)
}

fn normalize_directive_quotes_controlled<'a>(
    raw: &'a str,
    control: &OperationControl,
) -> OperationControlResult<Cow<'a, str>> {
    let mut checkpoints = ControlledScanCheckpoints::new(control)?;
    let mut first_quote = None;
    for (index, byte) in raw.as_bytes().iter().copied().enumerate() {
        checkpoints.scanned(1)?;
        if byte == b'\'' {
            first_quote = Some(index);
            break;
        }
    }
    let Some(first_quote) = first_quote else {
        checkpoints.finish()?;
        return Ok(Cow::Borrowed(raw));
    };

    let mut normalized = String::with_capacity(raw.len());
    let mut cursor = 0usize;
    for (relative, byte) in raw.as_bytes()[first_quote..].iter().copied().enumerate() {
        checkpoints.scanned(1)?;
        if byte != b'\'' {
            continue;
        }
        let quote = first_quote + relative;
        push_frontmatter_str_controlled(&mut normalized, &raw[cursor..quote], &mut checkpoints)?;
        normalized.push('"');
        cursor = quote + 1;
    }
    push_frontmatter_str_controlled(&mut normalized, &raw[cursor..], &mut checkpoints)?;
    checkpoints.finish()?;
    Ok(Cow::Owned(normalized))
}

#[cfg(test)]
fn detect_directives(input: &str) -> Result<Vec<Directive>> {
    let mut directives = Vec::new();
    for block in directive_blocks(input, DirectiveRecoveryMode::Strict) {
        if !block.complete {
            continue;
        }
        if let Some(directive) = parse_directive_like_upstream(block.raw)? {
            directives.push(directive);
        }
    }
    Ok(directives)
}

#[derive(Clone)]
enum DirectiveValuePathSegment {
    Key(String),
    Index(usize),
}

#[derive(Clone, Copy)]
enum DirectiveDictionaryKind {
    NodeColors,
    IconReferences,
}

#[cfg(test)]
fn sanitize_directive(value: &mut Value) {
    let control = OperationControl::new();
    sanitize_directive_controlled(value, &control)
        .expect("a private parse control cannot be cancelled");
}

fn sanitize_directive_controlled(
    value: &mut Value,
    control: &OperationControl,
) -> OperationControlResult<()> {
    control.checkpoint()?;
    let mut stack = vec![Vec::<DirectiveValuePathSegment>::new()];
    let mut visited = 0usize;

    while let Some(path) = stack.pop() {
        if visited.is_multiple_of(64) {
            control.checkpoint()?;
        }
        visited = visited.saturating_add(1);
        let Some(current) = directive_value_at_path_mut(value, &path) else {
            continue;
        };

        match current {
            Value::Object(map) => {
                if let Some(old) = map.remove("secure") {
                    crate::config::drop_value_nonrecursive(old);
                }

                let blocked_keys = map
                    .iter()
                    .filter(|(key, value)| {
                        is_suspicious_directive_key(key)
                            || !crate::generated::is_default_config_key(key)
                            || value.is_null()
                    })
                    .map(|(key, _)| key)
                    .cloned()
                    .collect::<Vec<_>>();
                for key in blocked_keys {
                    if let Some(old) = map.remove(&key) {
                        crate::config::drop_value_nonrecursive(old);
                    }
                }

                let child_keys = map.keys().cloned().collect::<Vec<_>>();
                for key in child_keys.into_iter().rev() {
                    if let Some(kind) = directive_dictionary_kind(&key)
                        && map
                            .get_mut(&key)
                            .is_some_and(|child| sanitize_directive_dictionary(child, kind))
                    {
                        continue;
                    }

                    let mut child_path = path.clone();
                    child_path.push(DirectiveValuePathSegment::Key(key));
                    stack.push(child_path);
                }
            }
            Value::Array(arr) => {
                for idx in (0..arr.len()).rev() {
                    let mut child_path = path.clone();
                    child_path.push(DirectiveValuePathSegment::Index(idx));
                    stack.push(child_path);
                }
            }
            Value::String(s) => {
                // Presentation values from directives and frontmatter share the final source
                // admission boundary. Do not erase rejected values here: deleting the override
                // there lets the trusted host value survive instead of replacing it with "".
                if path.iter().any(|segment| {
                    matches!(segment, DirectiveValuePathSegment::Key(key)
                        if crate::config::is_presentation_field(key))
                }) {
                    continue;
                }
                if directive_path_is_css(&path) && !css_braces_are_balanced(s) {
                    *s = "{ /* ERROR: Unbalanced CSS */ }".to_string();
                }
                let blocked = s.contains('<') || s.contains('>') || s.contains("url(data:");
                if blocked {
                    s.clear();
                }
            }
            _ => {}
        }
    }
    control.checkpoint()
}

fn directive_path_is_css(path: &[DirectiveValuePathSegment]) -> bool {
    matches!(
        path.last(),
        Some(DirectiveValuePathSegment::Key(key))
            if ["themeCSS", "fontFamily", "altFontFamily"]
                .iter()
                .any(|css_key| key.contains(css_key))
    )
}

fn is_suspicious_directive_key(key: &str) -> bool {
    key.starts_with("__") || key.contains("proto") || key.contains("constr")
}

fn css_braces_are_balanced(css: &str) -> bool {
    let mut depth = 0usize;
    for ch in css.chars() {
        match ch {
            '{' => depth += 1,
            '}' => {
                let Some(next) = depth.checked_sub(1) else {
                    return false;
                };
                depth = next;
            }
            _ => {}
        }
    }
    depth == 0
}

fn directive_dictionary_kind(key: &str) -> Option<DirectiveDictionaryKind> {
    // Source: Mermaid 11.16 `sanitizeDirective.ts` DICTIONARY_CONFIG_PATTERNS.
    match key {
        "nodeColors" => Some(DirectiveDictionaryKind::NodeColors),
        "filenameIcons" | "extensionIcons" => Some(DirectiveDictionaryKind::IconReferences),
        _ => None,
    }
}

fn sanitize_directive_dictionary(value: &mut Value, kind: DirectiveDictionaryKind) -> bool {
    let is_valid_value = |value: &Value| {
        value.as_str().is_some_and(|value| match kind {
            DirectiveDictionaryKind::NodeColors => is_valid_node_color(value),
            DirectiveDictionaryKind::IconReferences => is_valid_icon_reference(value),
        })
    };

    match value {
        Value::Object(map) => {
            let blocked_keys = map
                .iter()
                .filter(|(key, value)| is_suspicious_dictionary_key(key) || !is_valid_value(value))
                .map(|(key, _)| key.clone())
                .collect::<Vec<_>>();

            for key in blocked_keys {
                if let Some(old) = map.remove(&key) {
                    crate::config::drop_value_nonrecursive(old);
                }
            }
            true
        }
        Value::Array(values) => {
            for value in values.iter_mut().filter(|value| !is_valid_value(value)) {
                let old = std::mem::replace(value, Value::Null);
                crate::config::drop_value_nonrecursive(old);
            }
            true
        }
        _ => false,
    }
}

fn is_suspicious_dictionary_key(key: &str) -> bool {
    is_suspicious_directive_key(key)
}

fn is_valid_icon_reference(value: &str) -> bool {
    let mut segments = value.split(':');
    let Some(first) = segments.next() else {
        return false;
    };
    let second = segments.next();

    is_valid_icon_reference_segment(first)
        && second.is_none_or(is_valid_icon_reference_segment)
        && segments.next().is_none()
}

fn is_valid_icon_reference_segment(segment: &str) -> bool {
    !segment.is_empty()
        && segment
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'-'))
}

fn is_valid_node_color(value: &str) -> bool {
    if let Some(hex) = value.strip_prefix('#') {
        return (3..=8).contains(&hex.len()) && hex.bytes().all(|byte| byte.is_ascii_hexdigit());
    }

    if is_valid_node_color_function(value, "rgb(") || is_valid_node_color_function(value, "hsl(") {
        return true;
    }

    !value.is_empty() && value.bytes().all(|byte| byte.is_ascii_alphabetic())
}

fn is_valid_node_color_function(value: &str, prefix: &str) -> bool {
    let Some(actual_prefix) = value.get(..prefix.len()) else {
        return false;
    };
    if !actual_prefix.eq_ignore_ascii_case(prefix) || !value.ends_with(')') {
        return false;
    }

    let inner = &value[prefix.len()..value.len() - 1];
    !inner.is_empty()
        && inner.chars().all(|ch| {
            ch.is_ascii_digit() || is_js_regex_whitespace(ch) || matches!(ch, '%' | ',' | '.')
        })
}

fn is_js_regex_whitespace(ch: char) -> bool {
    if ('\u{2000}'..='\u{200A}').contains(&ch) {
        return true;
    }

    matches!(
        ch,
        '\u{0009}'
            | '\u{000A}'
            | '\u{000B}'
            | '\u{000C}'
            | '\u{000D}'
            | '\u{0020}'
            | '\u{00A0}'
            | '\u{1680}'
            | '\u{2028}'
            | '\u{2029}'
            | '\u{202F}'
            | '\u{205F}'
            | '\u{3000}'
            | '\u{FEFF}'
    )
}

fn directive_value_at_path_mut<'a>(
    mut value: &'a mut Value,
    path: &[DirectiveValuePathSegment],
) -> Option<&'a mut Value> {
    for segment in path {
        match segment {
            DirectiveValuePathSegment::Key(key) => {
                value = value.as_object_mut()?.get_mut(key)?;
            }
            DirectiveValuePathSegment::Index(idx) => {
                value = value.as_array_mut()?.get_mut(*idx)?;
            }
        }
    }
    Some(value)
}

pub(crate) fn directive_removal_ranges_controlled(
    text: &str,
    control: &OperationControl,
) -> OperationControlResult<Vec<std::ops::Range<usize>>> {
    Ok(
        directive_blocks_controlled(text, DirectiveRecoveryMode::Strict, control)?
            .into_iter()
            .map(|block| block.range)
            .collect(),
    )
}

fn parse_directive_capture_controlled(
    raw: &str,
    capture_mode: SourceConfigCaptureMode,
    parse_arguments: bool,
    control: &OperationControl,
) -> OperationControlResult<DirectiveParseCapture> {
    let mut checkpoints = ControlledScanCheckpoints::new(control)?;
    let (raw_start, raw_end) = trim_whitespace_bounds_controlled(raw, &mut checkpoints)?;
    let raw = &raw[raw_start..raw_end];
    if raw.is_empty() {
        checkpoints.finish()?;
        return Ok(DirectiveParseCapture {
            directive: None,
            error: None,
        });
    }

    let mut type_end = 0usize;
    for (index, ch) in raw.char_indices() {
        checkpoints.scanned(ch.len_utf8())?;
        if !ch.is_ascii_alphanumeric() && ch != '_' {
            break;
        }
        type_end = index + ch.len_utf8();
    }
    if type_end == 0 {
        checkpoints.finish()?;
        return Ok(DirectiveParseCapture {
            directive: None,
            error: None,
        });
    }
    let mut ty = String::with_capacity(type_end);
    push_frontmatter_str_controlled(&mut ty, &raw[..type_end], &mut checkpoints)?;
    let keyword_span = raw_start..raw_start + type_end;

    if !parse_arguments {
        checkpoints.finish()?;
        return Ok(DirectiveParseCapture {
            directive: Some(Directive {
                ty,
                args: None,
                keyword_span,
                config_keys: Vec::new(),
                rewrite_safe: false,
            }),
            error: None,
        });
    }

    let whitespace = trim_start_whitespace_controlled(&raw[type_end..], &mut checkpoints)?;
    let mut position = type_end + whitespace;
    let mut config_keys = Vec::new();
    let mut rewrite_safe = true;

    let args = if raw.as_bytes().get(position) == Some(&b':') {
        checkpoints.scanned(1)?;
        position += 1;
        let whitespace = trim_start_whitespace_controlled(&raw[position..], &mut checkpoints)?;
        position += whitespace;
        let rest = &raw[position..];
        let (rest_start, rest_end) = trim_whitespace_bounds_controlled(rest, &mut checkpoints)?;
        let rest = &rest[rest_start..rest_end];
        let rest_offset = raw_start
            .saturating_add(position)
            .saturating_add(rest_start);
        if rest.is_empty() {
            None
        } else if rest.starts_with('{') || rest.starts_with('[') {
            if rest.len() > MAX_DIRECTIVE_CONFIG_PARSE_BYTES {
                return Ok(DirectiveParseCapture {
                    directive: Some(Directive {
                        ty,
                        args: None,
                        keyword_span,
                        config_keys,
                        rewrite_safe: false,
                    }),
                    error: Some(Error::InvalidDirectiveJson {
                        message: format!(
                            "directive config exceeds the safe parser budget of {MAX_DIRECTIVE_CONFIG_PARSE_BYTES} bytes"
                        ),
                    }),
                });
            }
            if config_nesting_exceeds_limit_controlled(rest, control)? {
                return Ok(DirectiveParseCapture {
                    directive: Some(Directive {
                        ty,
                        args: None,
                        keyword_span,
                        config_keys,
                        rewrite_safe: false,
                    }),
                    error: Some(Error::InvalidDirectiveJson {
                        message: format!(
                            "config nesting exceeds {MAX_CONFIG_NESTING_DEPTH} levels"
                        ),
                    }),
                });
            }
            checkpoints.finish()?;
            let captured = parse_directive_config_value_controlled(rest, capture_mode, control)?;
            rewrite_safe &= captured.rewrite_safe;
            config_keys = captured
                .keys
                .into_iter()
                .map(|mut key| {
                    key.span = key.span.map(|span| {
                        rest_offset.saturating_add(span.start)..rest_offset.saturating_add(span.end)
                    });
                    key
                })
                .collect();
            captured.value
        } else {
            rewrite_safe = false;
            let mut value = String::with_capacity(rest.len());
            push_frontmatter_str_controlled(&mut value, rest, &mut checkpoints)?;
            Some(Value::String(value))
        }
    } else {
        None
    };

    checkpoints.finish()?;
    Ok(DirectiveParseCapture {
        directive: Some(Directive {
            ty,
            args,
            keyword_span,
            config_keys,
            rewrite_safe,
        }),
        error: None,
    })
}

struct DirectiveConfigCapture {
    value: Option<Value>,
    keys: Vec<source_config::Json5ConfigKeyEvidence>,
    rewrite_safe: bool,
}

fn parse_directive_config_value_controlled(
    input: &str,
    capture_mode: SourceConfigCaptureMode,
    control: &OperationControl,
) -> OperationControlResult<DirectiveConfigCapture> {
    control.checkpoint()?;
    // `json5` has no cancellation hook. The caller enforces a hard input and nesting bound, so
    // this is a bounded atomic parser region rather than an unbounded cancellation gap.
    let captured = if capture_mode.collects() {
        let captured = source_config::parse_json5_config(input);
        DirectiveConfigCapture {
            value: captured.value,
            keys: captured.keys,
            rewrite_safe: captured.rewrite_safe,
        }
    } else {
        let value = json5::from_str::<Value>(input).ok();
        DirectiveConfigCapture {
            rewrite_safe: value.is_some(),
            value,
            keys: Vec::new(),
        }
    };
    if let Err(cancelled) = control.checkpoint() {
        if let Some(value) = captured.value {
            crate::config::drop_value_nonrecursive(value);
        }
        return Err(cancelled);
    }
    Ok(captured)
}

#[cfg(test)]
fn config_nesting_exceeds_limit(text: &str) -> bool {
    let control = OperationControl::new();
    config_nesting_exceeds_limit_controlled(text, &control)
        .expect("a private parse control cannot be cancelled")
}

fn config_nesting_exceeds_limit_controlled(
    text: &str,
    control: &OperationControl,
) -> OperationControlResult<bool> {
    let mut checkpoints = ControlledScanCheckpoints::new(control)?;
    if max_flow_collection_depth_controlled(text, &mut checkpoints)? > MAX_CONFIG_NESTING_DEPTH {
        checkpoints.finish()?;
        return Ok(true);
    }
    let exceeds =
        max_yaml_indent_depth_controlled(text, &mut checkpoints)? > MAX_CONFIG_NESTING_DEPTH;
    checkpoints.finish()?;
    Ok(exceeds)
}

fn max_flow_collection_depth_controlled(
    text: &str,
    checkpoints: &mut ControlledScanCheckpoints<'_>,
) -> OperationControlResult<usize> {
    let mut max_depth = 0usize;
    let mut depth = 0usize;
    let mut quote = None;
    let mut escaped = false;

    for ch in text.chars() {
        checkpoints.scanned(ch.len_utf8())?;
        if let Some(q) = quote {
            if escaped {
                escaped = false;
                continue;
            }
            if ch == '\\' {
                escaped = true;
                continue;
            }
            if ch == q {
                quote = None;
            }
            continue;
        }

        match ch {
            '"' | '\'' => quote = Some(ch),
            '{' | '[' => {
                depth = depth.saturating_add(1);
                max_depth = max_depth.max(depth);
            }
            '}' | ']' => {
                depth = depth.saturating_sub(1);
            }
            _ => {}
        }
    }

    Ok(max_depth)
}

fn max_yaml_indent_depth_controlled(
    text: &str,
    checkpoints: &mut ControlledScanCheckpoints<'_>,
) -> OperationControlResult<usize> {
    let mut indents = Vec::<usize>::new();
    let mut max_depth = 0usize;
    let mut line_start = 0usize;

    while line_start < text.len() {
        let newline = find_newline_controlled(text, line_start, checkpoints)?;
        let line_end = newline.unwrap_or(text.len());
        let line_end =
            if newline.is_some() && line_end > line_start && text.as_bytes()[line_end - 1] == b'\r'
            {
                checkpoints.scanned(1)?;
                line_end - 1
            } else {
                line_end
            };
        let line = &text[line_start..line_end];
        let (trimmed_start, trimmed_end) = trim_whitespace_bounds_controlled(line, checkpoints)?;
        let trimmed = &line[trimmed_start..trimmed_end];
        if !trimmed.is_empty() && !trimmed.starts_with('#') {
            let indent = leading_ascii_space_count_controlled(line, checkpoints)?;
            while indents.last().is_some_and(|prev| indent <= *prev) {
                checkpoints.scanned(1)?;
                indents.pop();
            }
            indents.push(indent);
            let inline_sequence_depth =
                yaml_inline_sequence_indicator_count_controlled(trimmed, checkpoints)?;
            max_depth = max_depth.max(indents.len() + inline_sequence_depth.saturating_sub(1));
        }

        let Some(newline) = newline else {
            break;
        };
        line_start = newline + 1;
    }

    Ok(max_depth)
}

fn trim_whitespace_bounds_controlled(
    text: &str,
    checkpoints: &mut ControlledScanCheckpoints<'_>,
) -> OperationControlResult<(usize, usize)> {
    let mut start = text.len();
    for (idx, ch) in text.char_indices() {
        checkpoints.scanned(ch.len_utf8())?;
        if !ch.is_whitespace() {
            start = idx;
            break;
        }
    }
    if start == text.len() {
        return Ok((text.len(), text.len()));
    }

    let mut end = start;
    for (idx, ch) in text.char_indices().rev() {
        checkpoints.scanned(ch.len_utf8())?;
        if !ch.is_whitespace() {
            end = idx + ch.len_utf8();
            break;
        }
    }
    Ok((start, end))
}

fn leading_ascii_space_count_controlled(
    text: &str,
    checkpoints: &mut ControlledScanCheckpoints<'_>,
) -> OperationControlResult<usize> {
    let mut count = 0usize;
    for byte in text.bytes() {
        checkpoints.scanned(1)?;
        if byte != b' ' {
            break;
        }
        count += 1;
    }
    Ok(count)
}

fn trim_start_whitespace_controlled(
    text: &str,
    checkpoints: &mut ControlledScanCheckpoints<'_>,
) -> OperationControlResult<usize> {
    let mut start = text.len();
    for (idx, ch) in text.char_indices() {
        checkpoints.scanned(ch.len_utf8())?;
        if !ch.is_whitespace() {
            start = idx;
            break;
        }
    }
    Ok(start)
}

fn yaml_inline_sequence_indicator_count_controlled(
    mut text: &str,
    checkpoints: &mut ControlledScanCheckpoints<'_>,
) -> OperationControlResult<usize> {
    let mut count = 0usize;
    loop {
        let Some(after_dash) = text.strip_prefix('-') else {
            return Ok(count);
        };
        checkpoints.scanned(1)?;
        if let Some(ch) = after_dash.chars().next() {
            checkpoints.scanned(ch.len_utf8())?;
            if !ch.is_whitespace() {
                return Ok(count);
            }
        }
        count += 1;
        let trimmed_start = trim_start_whitespace_controlled(after_dash, checkpoints)?;
        text = &after_dash[trimmed_start..];
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::{Map, json};
    use std::sync::atomic::{AtomicUsize, Ordering};

    static INIT_DETECTOR_CALLS: AtomicUsize = AtomicUsize::new(0);

    fn counting_flowchart_detector(_text: &str, _config: &mut MermaidConfig) -> bool {
        INIT_DETECTOR_CALLS.fetch_add(1, Ordering::Relaxed);
        true
    }

    fn transformed(input: &str, transform: fn(&mut PreprocessedSource)) -> String {
        let mut source = PreprocessedSource::new(input);
        transform(&mut source);
        source.into_text()
    }

    #[test]
    fn controlled_frontmatter_apis_preserve_existing_block_semantics() {
        for (source, expected_block) in [
            ("---\ntitle: Demo\n---\nflowchart TD\n", true),
            (" \t--- \r\n \ttitle: Demo\r\n \t---\r\nflowchart TD", true),
            (
                "\u{2003}---\n\u{2003}title: Demo\n\u{2003}---\nflowchart TD",
                true,
            ),
            ("---\r\r\ntitle: Demo\r\n---\r\r\nflowchart TD", true),
            ("---\rtitle: Demo\r---\rflowchart TD", true),
            ("---\ntitle: Demo\n--- \u{2003}\nflowchart TD", true),
            ("---\ntitle: Demo\n---", false),
            ("--- trailing\ntitle: Demo\n---\nflowchart TD", false),
            ("---\ntitle: Demo\n \t---\nflowchart TD", false),
            ("---", false),
        ] {
            let control = OperationControl::new();
            let controlled = split_frontmatter_block_controlled(source, &control)
                .expect("an active parse control must not cancel");
            let wrapped = split_frontmatter_block(source);

            assert_eq!(controlled.is_some(), expected_block, "source: {source:?}");
            assert_eq!(
                controlled.is_some(),
                wrapped.is_some(),
                "source: {source:?}"
            );
            if let (Some(controlled), Some(wrapped)) = (controlled, wrapped) {
                assert_eq!(controlled.full, wrapped.full, "source: {source:?}");
                assert_eq!(controlled.body, wrapped.body, "source: {source:?}");
                assert_eq!(controlled.indent, wrapped.indent, "source: {source:?}");
                assert_eq!(
                    controlled.dedented_body, wrapped.dedented_body,
                    "source: {source:?}"
                );
                assert_eq!(controlled.stripped, wrapped.stripped, "source: {source:?}");

                let location =
                    locate_frontmatter_block_controlled(source, &OperationControl::new())
                        .expect("an active parse control must not cancel")
                        .expect("a split block must have a location");
                assert_eq!(location.full, controlled.full, "source: {source:?}");
                assert_eq!(location.body, controlled.body, "source: {source:?}");
                assert_eq!(location.indent, controlled.indent, "source: {source:?}");
                assert_eq!(location.stripped, controlled.stripped, "source: {source:?}");
                assert_eq!(&source[location.full.end..], controlled.stripped);
            }
        }
    }

    #[test]
    fn frontmatter_matches_upstream_greedy_whitespace_and_lazy_fences() {
        for (source, indent, body, stripped) in [
            ("---\n\n---\n\nMORE\n---\n", "", "---\n\nMORE", ""),
            ("---\n\n\n---\ngraph TD\n", "", "", "graph TD\n"),
            (
                "---\ntitle: x\n---\n \t\n  graph TD\n",
                "",
                "title: x",
                "  graph TD\n",
            ),
            (
                "---\r\r\ntitle: Demo\r\n---\r\r\nflowchart TD",
                "",
                "title: Demo\r",
                "flowchart TD",
            ),
            (
                "\u{feff}---\n\u{feff}title: x\n\u{feff}---\ngraph TD",
                "\u{feff}",
                "\u{feff}title: x",
                "graph TD",
            ),
            (
                "---\ntitle: |\n  ---\n  still the body\n---\ngraph TD",
                "",
                "title: |\n  ---\n  still the body",
                "graph TD",
            ),
        ] {
            let block = locate_frontmatter_block_controlled(source, &OperationControl::new())
                .unwrap()
                .unwrap();
            assert_eq!(block.indent, indent, "{source:?}");
            assert_eq!(
                &source[block.body.start..block.body.end],
                body,
                "{source:?}"
            );
            assert_eq!(block.stripped, stripped, "{source:?}");
            assert_eq!(block.full.end, source.len() - stripped.len());
        }
        for source in [
            "---\n---\n",
            "---\ntitle: x\n---",
            "---\ntitle: x\n---   ",
            "\u{0085}---\ntitle: x\n---\ngraph TD",
            "----\ntitle: x\n----\ngraph TD",
        ] {
            assert!(
                locate_frontmatter_block_controlled(source, &OperationControl::new())
                    .unwrap()
                    .is_none(),
                "{source:?}"
            );
        }
    }

    #[test]
    fn frontmatter_whitespace_only_input_is_scanned_and_cancellable() {
        let source = format!("---\n{}", " \n".repeat(16_000));
        assert!(
            locate_frontmatter_block_controlled(&source, &OperationControl::new())
                .unwrap()
                .is_none()
        );
        let control = OperationControl::new();
        control.cancel_after_checkpoints(3);
        assert!(locate_frontmatter_block_controlled(&source, &control).is_err());
    }

    #[test]
    fn controlled_frontmatter_split_preserves_indented_body_bytes() {
        let source =
            "  ---\r\n  title: Demo\r\n  config:\r\n    theme: dark\r\n  ---\r\nflowchart TD";
        let block = split_frontmatter_block_controlled(source, &OperationControl::new())
            .expect("an active parse control must not cancel")
            .expect("frontmatter block");

        assert_eq!(block.indent, "  ");
        assert_eq!(
            &source[block.body.start..block.body.end],
            "  title: Demo\r\n  config:\r\n    theme: dark\r"
        );
        assert_eq!(
            block.dedented_body,
            "title: Demo\r\nconfig:\r\n  theme: dark\r"
        );
        assert_eq!(block.stripped, "flowchart TD");
    }

    #[test]
    fn controlled_frontmatter_split_preserves_bare_cr_body_bytes() {
        let source = "  ---\r  title: Demo\r  config:\r    theme: dark\r  ---\rflowchart TD";
        let block = split_frontmatter_block_controlled(source, &OperationControl::new())
            .expect("an active parse control must not cancel")
            .expect("frontmatter block");

        assert_eq!(block.indent, "  ");
        assert_eq!(
            &source[block.body.start..block.body.end],
            "  title: Demo\r  config:\r    theme: dark"
        );
        assert_eq!(block.dedented_body, "title: Demo\rconfig:\r  theme: dark");
        assert_eq!(block.stripped, "flowchart TD");
    }

    #[test]
    fn controlled_frontmatter_split_preserves_long_opening_body_and_indent_semantics() {
        let indent = " ".repeat(8 * 1024);
        let value = "x".repeat(8 * 1024);
        let indented_body = format!("{indent}title: {value}");
        let source = format!("{indent}---\n{indented_body}\n{indent}---\nflowchart TD");
        let block = split_frontmatter_block_controlled(&source, &OperationControl::new())
            .expect("an active parse control must not cancel")
            .expect("frontmatter block");

        let dedented_body = format!("title: {value}");
        assert_eq!(block.indent, indent.as_str());
        assert_eq!(
            &source[block.body.start..block.body.end],
            indented_body.as_str()
        );
        assert_eq!(block.dedented_body.as_ref(), dedented_body.as_str());
        assert_eq!(block.stripped, "flowchart TD");
    }

    #[test]
    fn frontmatter_location_cancels_deterministically_on_a_long_opening_line() {
        let indent = " ".repeat(16 * 1024);
        let source = format!("{indent}---\n{indent}---\nflowchart TD");
        let control = OperationControl::new();
        control.cancel_after_checkpoints(2);

        assert!(locate_frontmatter_block_controlled(&source, &control).is_err());
    }

    #[test]
    fn frontmatter_location_cancels_deterministically_on_a_long_body() {
        let source = format!("---\n{}\n---\nflowchart TD", "x".repeat(16 * 1024));
        let control = OperationControl::new();
        control.cancel_after_checkpoints(2);

        assert!(locate_frontmatter_block_controlled(&source, &control).is_err());
    }

    #[test]
    fn frontmatter_location_cancels_deterministically_on_a_long_closing_line() {
        let source = format!(
            "---\ntitle: Demo\n---{}\nflowchart TD",
            " ".repeat(16 * 1024)
        );
        let control = OperationControl::new();
        control.cancel_after_checkpoints(2);

        assert!(locate_frontmatter_block_controlled(&source, &control).is_err());
    }

    #[test]
    fn frontmatter_dedent_cancels_deterministically_on_a_long_indented_body() {
        let body = format!("  {}", "x".repeat(16 * 1024));
        let control = OperationControl::new();
        control.cancel_after_checkpoints(2);

        assert!(dedent_frontmatter_body_controlled(&body, "  ", &control).is_err());
    }

    #[test]
    fn controlled_frontmatter_yaml_fields_match_the_legacy_wrapper() {
        let yaml = "title: Demo\nconfig:\n  theme: dark\n";
        let controlled = parse_frontmatter_yaml_fields_controlled(yaml, &OperationControl::new())
            .expect("an active parse control must not cancel")
            .expect("valid frontmatter YAML");

        assert_eq!(controlled, parse_frontmatter_yaml_fields(yaml).unwrap());

        let cancelled = OperationControl::new();
        cancelled.cancel();
        assert!(parse_frontmatter_yaml_fields_controlled(yaml, &cancelled).is_err());
    }

    #[test]
    fn bounded_frontmatter_yaml_fields_honor_caller_materialization_limits() {
        let result = parse_frontmatter_yaml_fields_bounded_controlled(
            "values: [one, two, three, four]\n",
            1024,
            16,
            8,
            &OperationControl::new(),
        )
        .expect("an active parse control must not cancel");

        let error = result.expect_err("the caller budget must reject materialization");
        assert!(error.contains("safe materialization budget"));
    }

    fn capture_source_config(input: &str) -> PreprocessCaptureOutcome {
        preprocess_mermaid_public_parse_pipeline_with_directive_recovery_evidence_controlled(
            input,
            &DetectorRegistry::default(),
            Some("flowchart-v2"),
            DirectiveRecoveryMode::RecoverLine,
            &OperationControl::new(),
        )
        .expect("an active parse control must not cancel")
    }

    #[test]
    fn source_config_evidence_maps_crlf_unicode_and_indentation_to_original_source() {
        let source = concat!(
            "\u{feff}  ---\r\n",
            "  title: 图\r\n",
            "  config:\r\n",
            "    flowchart:\r\n",
            "      \"htmlLabels\": false\r\n",
            "  ---\r\n",
            "%%{ initialize: { flowchart: { 'htmlLabels': false } } }%%\r\n",
            "flowchart TD\r\n",
            "A-->B\r\n",
        );
        let captured = capture_source_config(source);
        assert!(matches!(
            &captured.outcome,
            PreprocessCaptureResult::Ready(_)
        ));
        let evidence = captured.source_config;

        let frontmatter = evidence.frontmatter().expect("frontmatter evidence");
        assert_eq!(frontmatter.indent(), "  ");
        assert!(
            source[frontmatter.full_span().start..frontmatter.full_span().end]
                .starts_with("  ---\r\n")
        );
        let directives = evidence.directives();
        assert_eq!(directives.len(), 1);
        assert_eq!(directives[0].order(), 0);
        assert_eq!(directives[0].keyword(), "initialize");
        let keyword = directives[0].keyword_span();
        assert_eq!(&source[keyword.start..keyword.end], "initialize");

        let frontmatter_key = evidence
            .keys()
            .iter()
            .find(|key| {
                key.origin() == SourceConfigOrigin::Frontmatter
                    && key.matches_path(&["config", "flowchart", "htmlLabels"])
            })
            .expect("frontmatter htmlLabels evidence");
        assert_eq!(
            frontmatter_key.path_segments().collect::<Vec<_>>(),
            ["config", "flowchart", "htmlLabels"]
        );
        assert_eq!(
            &source[frontmatter_key.span().start..frontmatter_key.span().end],
            "htmlLabels"
        );
        let directive_key = evidence
            .keys()
            .iter()
            .find(|key| {
                key.origin() == SourceConfigOrigin::Directive { directive_index: 0 }
                    && key.matches_path(&["flowchart", "htmlLabels"])
            })
            .expect("directive htmlLabels evidence");
        assert_eq!(
            &source[directive_key.span().start..directive_key.span().end],
            "htmlLabels"
        );
        assert!(frontmatter_key.order() < directive_key.order());
        assert!(evidence.rewrite_safe());
    }

    #[test]
    fn json5_evidence_omits_escaped_and_array_nested_keys_without_disabling_rewrite() {
        let source = concat!(
            r#"%%{init: { flowchart: { "html\u004cabels": false }, values: [{ htmlLabels: false }] }}%%"#,
            "\nflowchart TD\nA-->B\n",
        );
        let captured = capture_source_config(source);
        assert!(matches!(
            &captured.outcome,
            PreprocessCaptureResult::Ready(_)
        ));
        let evidence = captured.source_config;

        assert!(evidence.rewrite_safe());
        assert!(
            evidence
                .keys()
                .iter()
                .any(|key| key.matches_path(&["flowchart"]))
        );
        assert!(!evidence.keys().iter().any(|key| {
            key.matches_path(&["flowchart", "htmlLabels"])
                || key.matches_path(&["values", "htmlLabels"])
        }));
    }

    #[test]
    fn source_config_evidence_retains_first_yaml_error_and_later_directive_facts() {
        let source = concat!(
            "---\n",
            "config:\n",
            "  flowchart:\n",
            "    htmlLabels: false\n",
            "  broken: [\n",
            "---\n",
            "%%{ initialize: { lazyLoadedDiagrams: true } }%%\n",
            "flowchart TD\nA-->B\n",
        );
        let captured = capture_source_config(source);
        assert!(matches!(
            &captured.outcome,
            PreprocessCaptureResult::Failed(Error::InvalidFrontMatterYaml { .. })
        ));
        assert!(!captured.source_config.rewrite_safe());
        assert!(captured.source_config.keys().iter().any(|key| {
            key.origin() == SourceConfigOrigin::Frontmatter
                && key.matches_path(&["config", "flowchart", "htmlLabels"])
        }));
        let directive = captured
            .source_config
            .directives()
            .iter()
            .find(|directive| directive.keyword() == "initialize")
            .expect("later directive evidence survives the YAML error");
        let span = directive.keyword_span();
        assert_eq!(&source[span.start..span.end], "initialize");
        assert!(captured.source_config.keys().iter().any(|key| {
            matches!(key.origin(), SourceConfigOrigin::Directive { .. })
                && key.matches_path(&["lazyLoadedDiagrams"])
        }));
    }

    #[test]
    fn source_config_evidence_retains_later_directives_after_a_directive_budget_error() {
        let nested = format!(
            "{}true{}",
            "{".repeat(MAX_CONFIG_NESTING_DEPTH + 1),
            "}".repeat(MAX_CONFIG_NESTING_DEPTH + 1)
        );
        let source = format!(
            "%%{{init: {nested}}}%%\n%%{{initialize: {{ theme: 'dark' }} }}%%\nflowchart TD\nA-->B\n"
        );
        let captured = capture_source_config(&source);
        assert!(matches!(
            &captured.outcome,
            PreprocessCaptureResult::Failed(Error::InvalidDirectiveJson { .. })
        ));
        assert_eq!(captured.source_config.directives().len(), 2);
        assert_eq!(captured.source_config.directives()[0].keyword(), "init");
        assert_eq!(
            captured.source_config.directives()[1].keyword(),
            "initialize"
        );
        assert!(!captured.source_config.rewrite_safe());
    }

    #[test]
    fn incomplete_directive_capture_stops_after_the_keyword() {
        let nested = format!(
            "{}true{}",
            "{".repeat(MAX_CONFIG_NESTING_DEPTH + 1),
            "}".repeat(MAX_CONFIG_NESTING_DEPTH + 1)
        );
        let source = format!("%%{{ initialize: {nested}\nflowchart TD\nA-->B\n");
        let captured = capture_source_config(&source);

        assert!(matches!(
            &captured.outcome,
            PreprocessCaptureResult::Ready(_)
        ));
        let directive = &captured.source_config.directives()[0];
        assert_eq!(directive.keyword(), "initialize");
        assert!(!directive.complete());
        assert!(captured.source_config.keys().is_empty());
        assert!(!captured.source_config.rewrite_safe());
    }

    #[test]
    fn source_config_evidence_weight_does_not_scale_with_unrelated_diagram_body() {
        let prefix = "%%{init: { theme: 'dark' }}%%\nflowchart TD\n";
        let small = capture_source_config(&format!("{prefix}A-->B\n")).source_config;
        let large_source = format!("{prefix}{}", "A-->B\n".repeat(16_384));
        let large = capture_source_config(&large_source).source_config;

        assert_eq!(small, large);
        assert_eq!(
            small.estimated_owned_heap_bytes(),
            large.estimated_owned_heap_bytes()
        );
        assert!(large.estimated_owned_heap_bytes() < large_source.len() / 100);
    }

    #[test]
    fn source_config_evidence_paths_remain_linear_for_deep_wide_json5() {
        let mut config = String::new();
        for depth in 0..64 {
            config.push_str(&format!("level{depth}:{{"));
        }
        config.push_str(&"repeated:0,".repeat(2_048));
        config.push_str("tail:1");
        config.push_str(&"}".repeat(64));
        let source = format!("%%{{init: {{{config}}}}}%%\nflowchart TD\nA-->B\n");

        let captured = capture_source_config(&source);
        assert!(matches!(
            &captured.outcome,
            PreprocessCaptureResult::Ready(_)
        ));
        assert!(captured.source_config.keys().len() > 2_048);
        assert!(
            captured.source_config.estimated_owned_heap_bytes() < source.len().saturating_mul(64),
            "shared path prefixes must keep retained evidence proportional to source bytes"
        );
    }

    #[test]
    fn source_config_evidence_drops_unneeded_frontmatter_field_values() {
        let small =
            capture_source_config("---\nnotes: small\n---\nflowchart TD\nA-->B\n").source_config;
        let large = capture_source_config(&format!(
            "---\nnotes: \"{}\"\n---\nflowchart TD\nA-->B\n",
            "x".repeat(256 * 1024)
        ))
        .source_config;

        assert!(small.frontmatter().expect("frontmatter").fields().is_none());
        assert!(large.frontmatter().expect("frontmatter").fields().is_none());
        assert_eq!(
            small.estimated_owned_heap_bytes(),
            large.estimated_owned_heap_bytes()
        );
    }

    #[test]
    fn frontmatter_rewrite_safety_distinguishes_insertion_from_materialization() {
        let commented_without_config = capture_source_config(concat!(
            "---\n",
            "# keep this comment\n",
            "title: Demo\n",
            "---\n",
            "%%{init: { theme: 'dark' }}%%\n",
            "flowchart TD\nA-->B\n",
        ))
        .source_config;
        assert!(commented_without_config.rewrite_safe());
        assert!(
            !commented_without_config
                .frontmatter()
                .expect("frontmatter")
                .rewrite_safe()
        );

        let commented_with_config = capture_source_config(concat!(
            "---\n",
            "# keep this comment\n",
            "config:\n",
            "  theme: default\n",
            "---\n",
            "%%{init: { theme: 'dark' }}%%\n",
            "flowchart TD\nA-->B\n",
        ))
        .source_config;
        assert!(!commented_with_config.rewrite_safe());
        assert!(
            commented_with_config
                .frontmatter()
                .expect("frontmatter")
                .fields()
                .is_none()
        );
    }

    #[test]
    fn normalize_crlf_matches_mermaid_line_ending_cleanup() {
        assert_eq!(
            transformed("flowchart TD\r\nA-->B\rC-->D\n", normalize_crlf),
            "flowchart TD\nA-->B\nC-->D\n"
        );
        assert_eq!(transformed("\r\r\n\n", normalize_crlf), "\n\n\n");
    }

    #[test]
    fn normalize_html_tag_attributes_matches_mermaid_cleanup_shape() {
        assert_eq!(
            transformed(
                r#"<span title="A" data-empty="">Label</span><br disabled="yes">"#,
                normalize_html_tag_attributes_like_upstream,
            ),
            r#"<span title='A' data-empty=''>Label</span><br disabled='yes'>"#
        );
        assert_eq!(
            transformed(
                r#"<é title="A"><_x value="B"><1 n="C">"#,
                normalize_html_tag_attributes_like_upstream,
            ),
            r#"<é title="A"><_x value='B'><1 n='C'>"#
        );
        assert_eq!(
            transformed(
                r#"<span a="x" title="A>B">"#,
                normalize_html_tag_attributes_like_upstream,
            ),
            r#"<span a='x' title="A>B">"#
        );
        assert_eq!(
            transformed(
                r#"<<span title="A">"#,
                normalize_html_tag_attributes_like_upstream,
            ),
            r#"<<span title='A'>"#
        );
    }

    #[test]
    fn normalize_html_attribute_quotes_keep_exact_unicode_source_spans() {
        let original = r#"flowchart TD
A["<span title="😀">Label</span>"]
"#;
        let mut source = PreprocessedSource::new(original);
        normalize_html_tag_attributes_like_upstream(&mut source);

        assert!(source.text().contains("title='😀'"));
        let emoji = source.text().find('😀').unwrap();
        let mapped = source
            .try_map_span(crate::SourceSpan::new(emoji, emoji + '😀'.len_utf8()))
            .expect("normalized attribute value span");
        assert_eq!(&original[mapped.start..mapped.end], "😀");
    }

    #[test]
    fn multiple_init_config_directives_detect_the_diagram_only_once() {
        INIT_DETECTOR_CALLS.store(0, Ordering::Relaxed);
        let input = concat!(
            "%%{init: {\"config\": {\"curve\": \"linear\"}}}%%\n",
            "%%{initialize: {\"config\": {\"htmlLabels\": false}}}%%\n",
            "flowchart TD\nA-->B\n",
        );
        let directives = detect_directives(input).expect("directives parse");
        let mut registry = DetectorRegistry::new();
        registry.add_fn("flowchart-v2", counting_flowchart_detector);

        let config = detect_init(&directives, input, &registry, None).expect("init merges");

        assert_eq!(INIT_DETECTOR_CALLS.load(Ordering::Relaxed), 1);
        assert_eq!(
            config
                .as_value()
                .pointer("/flowchart/curve")
                .and_then(Value::as_str),
            Some("linear")
        );
        assert_eq!(
            config
                .as_value()
                .pointer("/flowchart/htmlLabels")
                .and_then(Value::as_bool),
            Some(false)
        );
    }

    #[test]
    fn encode_entity_placeholders_matches_mermaid_ascii_word_shape() {
        assert_eq!(
            transformed(
                "Hello #there; #andHere;#77653;",
                encode_mermaid_entities_like_upstream,
            ),
            "Hello ﬂ°there¶ß ﬂ°andHere¶ßﬂ°°77653¶ß"
        );
        assert_eq!(
            transformed(
                "style this; is ; everything :something#not-nothing; and this too;",
                encode_mermaid_entities_like_upstream,
            ),
            "style this; is ; everything :something#not-nothing; and this too"
        );
        assert_eq!(
            transformed(
                "classDef this; is ; everything :something#not-nothing; and this too;",
                encode_mermaid_entities_like_upstream,
            ),
            "classDef this; is ; everything :something#not-nothing; and this too"
        );
        assert_eq!(
            transformed(
                "style a fill:#fff; style b fill:#000;",
                encode_mermaid_entities_like_upstream,
            ),
            "style a fill:ﬂ°fff¶ß style b fill:#000"
        );
        assert_eq!(
            transformed("style a fill: #fff;", encode_mermaid_entities_like_upstream,),
            "style a fill: ﬂ°fff¶ß"
        );
        assert_eq!(
            transformed(
                "#é; #+123; #has-dash;",
                encode_mermaid_entities_like_upstream,
            ),
            "#é; #+123; #has-dash;"
        );
    }

    #[test]
    fn sanitize_directive_handles_deep_values_with_small_stack() {
        const DEPTH: usize = 2_048;
        let mut value = deep_directive_value(DEPTH, Value::String("<blocked>".to_string()));

        let handle = std::thread::Builder::new()
            .name("preprocess-deep-directive-sanitize".to_string())
            .stack_size(64 * 1024)
            .spawn(move || {
                sanitize_directive(&mut value);
                assert_eq!(
                    deep_directive_leaf(&value, DEPTH).and_then(Value::as_str),
                    Some("")
                );
                crate::config::drop_value_nonrecursive(value);
            })
            .expect("spawn deep directive sanitizer test");
        handle
            .join()
            .expect("deep directive sanitizer should finish without stack overflow");
    }

    #[test]
    fn sanitize_directive_replaces_unbalanced_css_like_mermaid() {
        let mut value = json!({
            "themeCSS": "} * { background: red }",
            "flowchart": {
                "fontFamily": "valid { nested: value; }",
                "altFontFamily": "missing { close"
            }
        });

        sanitize_directive(&mut value);

        assert_eq!(
            value["themeCSS"],
            Value::String("{ /* ERROR: Unbalanced CSS */ }".to_string())
        );
        assert_eq!(value["flowchart"]["fontFamily"], "valid { nested: value; }");
        assert!(value["flowchart"].get("altFontFamily").is_none());
    }

    #[test]
    fn sanitize_directive_leaves_theme_values_for_shared_source_admission() {
        let mut value = json!({
            "themeVariables": {
                "primaryColor": "#123456",
                "secondaryColor": "rgb(1, 2, 3)",
                "tertiaryColor": "url(javascript:alert(1))",
                "noteBkgColor": "hsl(120, 50%, 25.5%)",
                "noteTextColor": "red-blue"
            }
        });

        sanitize_directive(&mut value);

        assert_eq!(value["themeVariables"]["primaryColor"], "#123456");
        assert_eq!(value["themeVariables"]["secondaryColor"], "rgb(1, 2, 3)");
        assert_eq!(
            value["themeVariables"]["tertiaryColor"],
            "url(javascript:alert(1))"
        );
        assert_eq!(
            value["themeVariables"]["noteBkgColor"],
            "hsl(120, 50%, 25.5%)"
        );
        assert_eq!(value["themeVariables"]["noteTextColor"], "red-blue");
    }

    #[test]
    fn sanitize_directive_uses_generated_config_shape_for_all_value_kinds() {
        let mut value = json!({
            "notAConfigKey": "removed",
            "theme": null,
            "prototype": "removed",
            "constructor": "removed",
            "deterministicIDSeed": "accepted undefined key",
            "sequence": {
                "messageFont": "accepted function key",
                "unknownNestedKey": true
            },
            "secure": ["theme"],
            "flowchart": {
                "secure": ["htmlLabels"],
                "htmlLabels": false
            }
        });

        sanitize_directive(&mut value);

        assert_eq!(
            value,
            json!({
                "deterministicIDSeed": "accepted undefined key",
                "sequence": {
                    "messageFont": "accepted function key"
                },
                "flowchart": {
                    "htmlLabels": false
                }
            })
        );
    }

    #[test]
    fn sanitize_directive_preserves_valid_dictionary_entries() {
        let mut value = json!({
            "sankey": {
                "nodeColors": {
                    "shortHex": "#abc",
                    "alphaHex": "#12345678",
                    "rgb": "rgb(0, 10%, 255)",
                    "hsl": "hsl(120, 50%, 25.5%)",
                    "named": "rebeccapurple"
                }
            },
            "treeView": {
                "filenameIcons": {
                    "Makefile": "cmake",
                    "README.md": "fa:bell"
                },
                "extensionIcons": {
                    ".ts": "logos:typescript-icon",
                    ".txt": "none"
                }
            }
        });

        sanitize_directive(&mut value);

        assert_eq!(value["sankey"]["nodeColors"]["shortHex"], "#abc");
        assert_eq!(value["sankey"]["nodeColors"]["alphaHex"], "#12345678");
        assert_eq!(value["sankey"]["nodeColors"]["rgb"], "rgb(0, 10%, 255)");
        assert_eq!(value["sankey"]["nodeColors"]["hsl"], "hsl(120, 50%, 25.5%)");
        assert_eq!(value["sankey"]["nodeColors"]["named"], "rebeccapurple");
        assert_eq!(value["treeView"]["filenameIcons"]["README.md"], "fa:bell");
        assert_eq!(
            value["treeView"]["extensionIcons"][".ts"],
            "logos:typescript-icon"
        );
    }

    #[test]
    fn sanitize_directive_removes_invalid_dictionary_values() {
        let mut value = json!({
            "sankey": {
                "nodeColors": {
                    "valid": "#ff0000",
                    "short": "#12",
                    "function": "url(javascript:alert(1))",
                    "wrongType": 42
                }
            },
            "treeView": {
                "filenameIcons": {
                    "valid": "docker",
                    "markup": "<script>alert(1)</script>",
                    "wrongType": false
                },
                "extensionIcons": {
                    ".ts": "logos:typescript-icon",
                    ".css": "not a valid name",
                    ".json": "one:two:three"
                }
            }
        });

        sanitize_directive(&mut value);

        assert_eq!(value["sankey"]["nodeColors"], json!({ "valid": "#ff0000" }));
        assert_eq!(
            value["treeView"]["filenameIcons"],
            json!({ "valid": "docker" })
        );
        assert_eq!(
            value["treeView"]["extensionIcons"],
            json!({ ".ts": "logos:typescript-icon" })
        );
    }

    #[test]
    fn sanitize_directive_removes_suspicious_dictionary_keys() {
        let mut value = json!({
            "sankey": {
                "nodeColors": {
                    "__proto__hack": "red",
                    "prototype": "green",
                    "constructor.js": "blue",
                    "safe": "black"
                }
            },
            "treeView": {
                "filenameIcons": {
                    "__proto__hack": "docker",
                    "prototype.ts": "docker",
                    "constructor.js": "docker",
                    "a.ts": "docker"
                }
            }
        });

        sanitize_directive(&mut value);

        assert_eq!(value["sankey"]["nodeColors"], json!({ "safe": "black" }));
        assert_eq!(
            value["treeView"]["filenameIcons"],
            json!({ "a.ts": "docker" })
        );
    }

    #[test]
    fn sanitize_directive_validates_dictionary_arrays_like_javascript_objects() {
        let mut value = json!({
            "sankey": {
                "nodeColors": ["#ff0000", "url(javascript:alert(1))", 42]
            },
            "treeView": {
                "extensionIcons": ["logos:typescript-icon", "not a valid name", false]
            }
        });

        sanitize_directive(&mut value);

        assert_eq!(
            value["sankey"]["nodeColors"],
            json!(["#ff0000", null, null])
        );
        assert_eq!(
            value["treeView"]["extensionIcons"],
            json!(["logos:typescript-icon", null, null])
        );
    }

    #[test]
    fn config_nesting_counts_inline_yaml_sequence_indicators() {
        let yaml = format!(
            "config:\n  {}\"leaf\"",
            "- ".repeat(MAX_CONFIG_NESTING_DEPTH + 1)
        );
        assert!(config_nesting_exceeds_limit(&yaml));
    }

    #[test]
    fn controlled_config_nesting_preserves_existing_depth_semantics() {
        let deep_flow = "[".repeat(MAX_CONFIG_NESTING_DEPTH + 1);
        for (yaml, expected) in [
            ("title: '[not, nesting]'\n", false),
            ("config:\r\n  theme: dark\r\n", false),
            ("# [ignored]\nconfig: { theme: dark }\n", false),
            (deep_flow.as_str(), true),
        ] {
            let controlled =
                config_nesting_exceeds_limit_controlled(yaml, &OperationControl::new())
                    .expect("an active parse control must not cancel");

            assert_eq!(controlled, expected, "yaml: {yaml:?}");
            assert_eq!(controlled, config_nesting_exceeds_limit(yaml));
        }
    }

    #[test]
    fn config_nesting_flow_scan_cancels_deterministically_on_a_long_line() {
        let yaml = format!("title: \"{}\"", "x".repeat(16 * 1024));
        let control = OperationControl::new();
        control.cancel_after_checkpoints(2);

        assert!(config_nesting_exceeds_limit_controlled(&yaml, &control).is_err());
    }

    #[test]
    fn config_nesting_indent_scan_cancels_deterministically_on_a_long_line() {
        let yaml = format!("{}key: value", " ".repeat(16 * 1024));
        let control = OperationControl::new();
        control.cancel_after_checkpoints(2);
        let mut checkpoints = ControlledScanCheckpoints::new(&control).unwrap();

        assert!(max_yaml_indent_depth_controlled(&yaml, &mut checkpoints).is_err());
    }

    #[test]
    fn config_nesting_trim_cancels_deterministically_on_long_whitespace() {
        let line = format!("{}value{}", " ".repeat(16 * 1024), " ".repeat(16 * 1024));
        let control = OperationControl::new();
        control.cancel_after_checkpoints(2);
        let mut checkpoints = ControlledScanCheckpoints::new(&control).unwrap();

        assert!(trim_whitespace_bounds_controlled(&line, &mut checkpoints).is_err());
    }

    fn deep_directive_value(depth: usize, leaf: Value) -> Value {
        let mut value = leaf;
        for _ in 0..depth {
            let mut map = Map::new();
            map.insert("flowchart".to_string(), value);
            value = Value::Object(map);
        }
        value
    }

    fn deep_directive_leaf(mut value: &Value, depth: usize) -> Option<&Value> {
        for _ in 0..depth {
            value = value.as_object()?.get("flowchart")?;
        }
        Some(value)
    }
}
