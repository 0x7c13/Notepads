use crate::operation::OperationPhase;
use crate::preprocess::{
    DirectiveRecoveryMode, PreprocessCaptureOutcome, PreprocessedSource, SourceConfigEvidence,
    preprocess_diagram_with_known_type_and_directive_recovery_controlled,
    preprocess_diagram_with_known_type_and_directive_recovery_evidence_controlled,
    preprocess_mermaid_public_parse_pipeline_with_directive_recovery_controlled,
    preprocess_mermaid_public_parse_pipeline_with_directive_recovery_evidence_controlled,
};
use crate::{
    EditorSemanticFacts, Engine, Error, ManagedSemanticJson, MermaidConfig, OperationControl,
    OperationControlResult, ParseMetadata, ParseOptions, Result, SourceSpan, common_db, diagram,
    diagrams::error_diagram, family, runtime, sanitize, theme,
};
use diagram::{
    CapturedPanic, CustomJsonRenderModel, DiagramParseOutcome, DiagramParseSnapshot,
    DiagramSnapshotCapture, DiagramWarningFact, ParsedDiagram, ParsedDiagramRender,
    ParsedEditorFacts, RegistryOwner, RenderSemanticModel, RenderSemanticParseOutput,
    ResolvedRenderParser, ResolvedSemanticParser,
};
use serde::Deserialize;
use serde_json::Value;

#[derive(Debug, Clone, Copy)]
pub(crate) enum ParseSource<'a> {
    Detect,
    KnownType(&'a str),
}

#[derive(Debug, Clone, Copy, Eq, PartialEq)]
enum PreprocessPath {
    PublicParse,
    Render,
}

struct CompatibilitySemanticParse {
    model: ManagedSemanticJson,
    warnings: CompatibilityWarnings,
}

enum CompatibilityWarnings {
    Typed(Vec<DiagramWarningFact>),
    BuiltInWithoutWarnings,
    CustomJson,
}

impl CompatibilitySemanticParse {
    fn built_in(model: ManagedSemanticJson, warning_facts: Vec<DiagramWarningFact>) -> Self {
        Self {
            model,
            warnings: CompatibilityWarnings::Typed(warning_facts),
        }
    }

    fn built_in_without_warnings(model: Value) -> Self {
        Self {
            model: model.into(),
            warnings: CompatibilityWarnings::BuiltInWithoutWarnings,
        }
    }

    fn custom(model: Value) -> Self {
        Self {
            model: model.into(),
            warnings: CompatibilityWarnings::CustomJson,
        }
    }
}

#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub(crate) enum ParseTiming {
    None,
    Json,
    Render,
}

pub(crate) struct ParsePipeline<'a> {
    engine: &'a Engine,
    text: &'a str,
    options: ParseOptions,
    source: ParseSource<'a>,
}

struct PreparedPreprocessCapture {
    outcome: PreparedPreprocessOutcome,
    source_config: SourceConfigEvidence,
}

// The ready variant is the parse hot path and is moved exactly once into snapshot construction.
// Boxing it would add one allocation to every parse only to shrink this short-lived control value.
#[allow(clippy::large_enum_variant)]
enum PreparedPreprocessOutcome {
    Ready(PreprocessedSource, ParseMetadata),
    Failed(Error),
    Panicked(CapturedPanic),
}

struct EditorParseSourceMap<'a> {
    source: &'a PreprocessedSource,
}

impl<'a> EditorParseSourceMap<'a> {
    fn new(source: &'a PreprocessedSource) -> Self {
        Self { source }
    }

    fn parser_input(&self) -> &'a str {
        self.source.text()
    }

    fn remap_facts(
        &self,
        facts: &mut EditorSemanticFacts,
        control: &OperationControl,
    ) -> OperationControlResult<()> {
        let original_symbol_count = facts.symbols.len();
        let mut remapped_symbols = Vec::with_capacity(original_symbol_count);
        for (index, mut symbol) in std::mem::take(&mut facts.symbols).into_iter().enumerate() {
            if index % 128 == 0 {
                control.checkpoint()?;
            }
            let Some(span) = self.try_remap_source_span(symbol.span) else {
                continue;
            };
            let Some(selection) = self.try_remap_source_span(symbol.selection) else {
                continue;
            };
            symbol.span = span;
            symbol.selection = selection;
            remapped_symbols.push(symbol);
        }
        facts.symbols = remapped_symbols;

        let mut dropped_diagnostic_spans = 0usize;
        for (index, diagnostic) in facts.diagnostics.iter_mut().enumerate() {
            if index % 128 == 0 {
                control.checkpoint()?;
            }
            if let Some(span) = diagnostic.span {
                diagnostic.span = self.try_remap_source_span(span);
                dropped_diagnostic_spans += usize::from(diagnostic.span.is_none());
            }
        }
        let original_expected_count = facts.expected_syntax.len();
        let mut remapped_expected = Vec::with_capacity(original_expected_count);
        for (index, mut expected) in std::mem::take(&mut facts.expected_syntax)
            .into_iter()
            .enumerate()
        {
            if index % 128 == 0 {
                control.checkpoint()?;
            }
            if let Some(span) = self.try_remap_source_span(expected.span) {
                expected.span = span;
                remapped_expected.push(expected);
            }
        }
        facts.expected_syntax = remapped_expected;

        let dropped_symbols = original_symbol_count - facts.symbols.len();
        let dropped_expected = original_expected_count - facts.expected_syntax.len();
        let dropped_spans = dropped_symbols + dropped_expected + dropped_diagnostic_spans;
        if dropped_spans > 0 {
            facts.mark_recovered_with_diagnostic(
                format!(
                    "dropped {dropped_spans} editor fact span(s) that crossed a preprocessing edit"
                ),
                None,
            );
        }
        control.checkpoint()
    }

    fn remap_parse_error(&self, err: Error) -> Error {
        match err {
            Error::DiagramParse {
                diagram_type,
                diagnostic,
            } => Error::DiagramParse {
                diagram_type,
                diagnostic: self.remap_parse_diagnostic(diagnostic),
            },
            err => err,
        }
    }

    fn remap_parse_diagnostic(&self, diagnostic: crate::ParseDiagnostic) -> crate::ParseDiagnostic {
        let Some(span) = diagnostic.span() else {
            return diagnostic;
        };
        match self.try_remap_source_span(span) {
            Some(remapped) => diagnostic.map_span(|_| remapped),
            None => diagnostic.without_span(),
        }
    }

    fn try_remap_source_span(&self, span: SourceSpan) -> Option<SourceSpan> {
        self.source.try_map_span(span)
    }

    fn try_remap_warning_source_span(&self, span: SourceSpan) -> Option<SourceSpan> {
        self.try_remap_source_span(span)
    }
}

impl<'a> ParsePipeline<'a> {
    pub(crate) fn detect(engine: &'a Engine, text: &'a str, options: ParseOptions) -> Self {
        Self {
            engine,
            text,
            options,
            source: ParseSource::Detect,
        }
    }

    pub(crate) fn known_type(
        engine: &'a Engine,
        diagram_type: &'a str,
        text: &'a str,
        options: ParseOptions,
    ) -> Self {
        Self {
            engine,
            text,
            options,
            source: ParseSource::KnownType(diagram_type),
        }
    }

    pub(crate) fn metadata(&self) -> Result<ParseMetadata> {
        self.with_operation_context(|_| self.metadata_in_context())
    }

    fn metadata_in_context(&self) -> Result<ParseMetadata> {
        let (_, metadata) = self.preprocess_strict(PreprocessPath::PublicParse)?;
        Ok(metadata)
    }

    pub(crate) fn parse_json(&self, timing: ParseTiming) -> Result<Option<ParsedDiagram>> {
        self.parse_model(
            timing,
            PreprocessPath::PublicParse,
            Self::parse_compatibility_semantic,
            |parsed, config| {
                common_db::apply_common_db_sanitization(parsed.model.as_value_mut(), config);
            },
            error_diagram::suppressed_error_diagram,
            |meta, parsed| ParsedDiagram {
                meta,
                model: parsed.model,
            },
            Self::remap_compatibility_semantic_warnings,
            |_| None,
        )
    }

    fn parse_compatibility_semantic(
        &self,
        code: &str,
        meta: &ParseMetadata,
    ) -> Result<CompatibilitySemanticParse> {
        let Some(parser) = self.engine.diagram_registry.resolve(&meta.diagram_type) else {
            return Err(Error::UnsupportedDiagram {
                diagram_type: meta.diagram_type.clone(),
            });
        };
        match parser {
            ResolvedSemanticParser::BuiltIn(parser) => {
                if let Some(parser) = family::warning_semantic_parser(&meta.diagram_type) {
                    let (model, warning_facts) = parser(code, meta)?.into_parts();
                    Ok(CompatibilitySemanticParse::built_in(model, warning_facts))
                } else {
                    parser(code, meta).map(CompatibilitySemanticParse::built_in_without_warnings)
                }
            }
            ResolvedSemanticParser::Custom(parser) => {
                let control = OperationControl::new();
                parser(code, meta, &control)
                    .map_err(Error::from)?
                    .map(CompatibilitySemanticParse::custom)
            }
        }
    }

    pub(crate) fn parse_editor_snapshot(
        &self,
        timing: ParseTiming,
    ) -> Result<Option<DiagramParseSnapshot>> {
        let control = OperationControl::new();
        self.parse_editor_snapshot_controlled(timing, &control)
            .map_err(Error::from)?
    }

    pub(crate) fn parse_editor_snapshot_controlled(
        &self,
        timing: ParseTiming,
        control: &OperationControl,
    ) -> OperationControlResult<Result<Option<DiagramParseSnapshot>>> {
        Ok(self
            .capture_editor_snapshot_controlled(timing, control)?
            .into_result())
    }

    pub(crate) fn capture_editor_snapshot_controlled(
        &self,
        timing: ParseTiming,
        control: &OperationControl,
    ) -> OperationControlResult<DiagramSnapshotCapture> {
        control.checkpoint()?;
        let operation_context = match self.engine.begin_operation() {
            Ok(context) => context,
            Err(error) => {
                return Ok(DiagramSnapshotCapture::Failed {
                    error: error.into(),
                    source_config: SourceConfigEvidence::default(),
                });
            }
        };
        runtime::with_operation_context(&operation_context, || {
            self.parse_editor_snapshot_in_context_controlled(timing, &operation_context, control)
        })
    }

    fn parse_editor_snapshot_in_context_controlled(
        &self,
        timing: ParseTiming,
        operation_context: &runtime::OperationContext,
        control: &OperationControl,
    ) -> OperationControlResult<DiagramSnapshotCapture> {
        control.checkpoint()?;
        let operation_timing = timing.operation_timing(operation_context);
        let total_start = operation_timing.map(runtime::OperationTiming::start);
        let preprocess_start = operation_timing.map(runtime::OperationTiming::start);
        let preprocessed = self.preprocess_with_evidence_controlled(
            PreprocessPath::PublicParse,
            DirectiveRecoveryMode::RecoverLine,
            control,
        )?;
        let PreparedPreprocessCapture {
            outcome,
            source_config,
        } = preprocessed;
        let (code, meta) = match outcome {
            PreparedPreprocessOutcome::Ready(code, meta) => (code, meta),
            PreparedPreprocessOutcome::Failed(Error::DetectType(_))
                if self.options.suppress_errors =>
            {
                return Ok(DiagramSnapshotCapture::Snapshot(None));
            }
            PreparedPreprocessOutcome::Failed(error) => {
                return Ok(DiagramSnapshotCapture::Failed {
                    error,
                    source_config,
                });
            }
            PreparedPreprocessOutcome::Panicked(panic) => {
                return Ok(DiagramSnapshotCapture::Panicked {
                    panic,
                    source_config,
                });
            }
        };
        control.checkpoint()?;
        let source_map = EditorParseSourceMap::new(&code);
        let recovered_incomplete_directive = code.recovered_incomplete_directive();
        let editor_input = source_map.parser_input();
        let preprocess = preprocess_start.map(runtime::OperationTimer::elapsed);

        let resolved = self.engine.diagram_registry.resolve(&meta.diagram_type);
        let combined = matches!(resolved, Some(ResolvedSemanticParser::BuiltIn(_)))
            .then(|| family::combined_parser(&meta.diagram_type))
            .flatten();

        let parse_start = operation_timing.map(runtime::OperationTiming::start);
        let parse_result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(
            || -> OperationControlResult<(
                Result<ManagedSemanticJson>,
                Option<EditorSemanticFacts>,
                Vec<DiagramWarningFact>,
            )> {
                let parsed = match resolved {
                    Some(ResolvedSemanticParser::BuiltIn(parser)) => {
                        if let Some(parser) = combined {
                            control.checkpoint()?;
                            let parsed = parser(editor_input, &meta, control)?;
                            control.checkpoint()?;
                            let (model, editor_facts, warning_facts) = parsed.into_parts();
                            (model, Some(editor_facts), warning_facts)
                        } else {
                            control.checkpoint()?;
                            let model = parser(editor_input, &meta).map(ManagedSemanticJson::from);
                            control.checkpoint()?;
                            (model, None, Vec::new())
                        }
                    }
                    Some(ResolvedSemanticParser::Custom(parser)) => {
                        control.checkpoint()?;
                        let model = parser(editor_input, &meta, control)?.map(ManagedSemanticJson::from);
                        control.checkpoint()?;
                        (model, None, Vec::new())
                    }
                    None => (
                        Err(Error::UnsupportedDiagram {
                            diagram_type: meta.diagram_type.clone(),
                        }),
                        None,
                        Vec::new(),
                    ),
                };
                Ok(parsed)
            },
        ));
        let (model_result, combined_facts, mut warning_facts) = match parse_result {
            Ok(parsed) => parsed?,
            Err(payload) => {
                return Ok(DiagramSnapshotCapture::Snapshot(Some(
                    DiagramParseSnapshot::new(
                        meta,
                        DiagramParseOutcome::Panicked(
                            CapturedPanic::from_payload(payload).message().to_string(),
                        ),
                        ParsedEditorFacts::Unavailable,
                        source_config,
                        recovered_incomplete_directive,
                    ),
                )));
            }
        };
        let parse = parse_start.map(runtime::OperationTimer::elapsed);
        let owner = resolved.map(ResolvedSemanticParser::owner);
        let mut model = match model_result {
            Ok(model) => model,
            Err(err) => {
                let err = source_map.remap_parse_error(err);
                let editor_facts = self.finish_snapshot_editor_facts(
                    owner,
                    combined_facts,
                    &meta,
                    &source_map,
                    control,
                )?;
                return Ok(DiagramSnapshotCapture::Snapshot(Some(
                    DiagramParseSnapshot::new(
                        meta,
                        DiagramParseOutcome::Failed(err),
                        editor_facts,
                        source_config,
                        recovered_incomplete_directive,
                    ),
                )));
            }
        };

        control.checkpoint()?;
        let sanitize_start = operation_timing.map(runtime::OperationTiming::start);
        common_db::apply_common_db_sanitization(model.as_value_mut(), &meta.effective_config);
        control.checkpoint()?;
        let sanitize = sanitize_start.map(runtime::OperationTimer::elapsed);
        let custom_warning_adapter_succeeded = if matches!(owner, Some(RegistryOwner::Custom)) {
            match Self::decode_custom_warning_facts_controlled(&model, control)? {
                Some(custom_warning_facts) => {
                    warning_facts = custom_warning_facts;
                    true
                }
                None => false,
            }
        } else {
            false
        };
        #[cfg(feature = "diagram-agentflow")]
        if matches!(owner, Some(RegistryOwner::BuiltIn)) {
            let (line_offset, column_offset) = code.parser_position_offset();
            crate::diagrams::agentflow::offset_compatibility_diagnostic_positions(
                model.as_value_mut(),
                line_offset,
                column_offset,
            );
        }
        Self::remap_warning_facts_controlled(&mut warning_facts, &source_map, control)?;
        if matches!(owner, Some(RegistryOwner::BuiltIn)) || custom_warning_adapter_succeeded {
            Self::sync_compatibility_warning_facts(model.as_value_mut(), &warning_facts);
        }
        control.checkpoint()?;
        timing.log_success(ParseTimingSuccess {
            total_start,
            meta: &meta,
            model_kind: None,
            preprocess,
            parse,
            sanitize,
            input_bytes: self.text.len(),
        });

        let editor_facts =
            self.finish_snapshot_editor_facts(owner, combined_facts, &meta, &source_map, control)?;

        Ok(DiagramSnapshotCapture::Snapshot(Some(
            DiagramParseSnapshot::new(
                meta,
                DiagramParseOutcome::Parsed {
                    model,
                    warning_facts,
                },
                editor_facts,
                source_config,
                recovered_incomplete_directive,
            ),
        )))
    }

    fn finish_snapshot_editor_facts(
        &self,
        owner: Option<RegistryOwner>,
        facts: Option<EditorSemanticFacts>,
        meta: &ParseMetadata,
        source_map: &EditorParseSourceMap<'_>,
        control: &OperationControl,
    ) -> OperationControlResult<ParsedEditorFacts> {
        control.checkpoint()?;
        let facts = match (owner, facts) {
            (Some(RegistryOwner::Custom), _) | (None, _) => ParsedEditorFacts::Unavailable,
            (Some(RegistryOwner::BuiltIn), Some(facts)) => ParsedEditorFacts::Available(
                self.finish_editor_semantic_facts(facts, meta, source_map, control)?,
            ),
            (Some(RegistryOwner::BuiltIn), None) => {
                debug_assert!(
                    family::combined_parser(&meta.diagram_type).is_none(),
                    "built-in families with editor capability must provide a combined semantic parser"
                );
                ParsedEditorFacts::Unavailable
            }
        };
        control.checkpoint()?;
        Ok(facts)
    }

    pub(crate) fn parse_render_model(&self) -> Result<Option<ParsedDiagramRender>> {
        self.parse_model(
            ParseTiming::Render,
            PreprocessPath::Render,
            Self::parse_render_semantic_model,
            |output, config| output.model_mut().sanitize_common_db_fields(config),
            error_diagram::suppressed_error_render_diagram,
            ParsedDiagramRender::from_parse_output,
            |output, source_map| {
                let (line_offset, column_offset) = source_map.source.parser_position_offset();
                output
                    .model_mut()
                    .offset_parser_diagnostic_positions(line_offset, column_offset);
                output.model_mut().remap_warning_fact_spans(|fact| {
                    Self::remap_warning_fact_spans(fact, source_map);
                });
            },
            |output| Some(output.model().kind()),
        )
    }

    /// Parses a typed render model while observing one caller-owned operation control.
    ///
    /// The existing render parser implementations remain family-owned. This seam checks the
    /// shared control before and after each parser-owned stage, then returns cancellation through
    /// the outer operation result instead of converting it into a Mermaid parse error.
    pub(crate) fn parse_render_model_controlled(
        &self,
        operation: &OperationControl,
    ) -> OperationControlResult<Result<Option<ParsedDiagramRender>>> {
        let operation_context = match self.engine.begin_operation() {
            Ok(context) => context,
            Err(error) => return Ok(Err(error.into())),
        };
        self.parse_render_model_controlled_in_context(operation, &operation_context)
    }

    /// Parses a render model inside a caller-owned runtime context and operation.
    ///
    /// This is the composition seam for higher-level render facades. It deliberately does not
    /// begin another runtime operation, so deterministic time, randomness, and timezone values
    /// remain shared across parsing and every downstream target adapter.
    pub(crate) fn parse_render_model_controlled_in_context(
        &self,
        operation: &OperationControl,
        operation_context: &runtime::OperationContext,
    ) -> OperationControlResult<Result<Option<ParsedDiagramRender>>> {
        operation.checkpoint_at(OperationPhase::Admission)?;
        let control = operation.for_phase(OperationPhase::Parse);
        control.checkpoint_at(OperationPhase::Parse)?;
        runtime::with_operation_context(operation_context, || {
            let timing = ParseTiming::Render;
            let operation_timing = timing.operation_timing(operation_context);
            let total_start = operation_timing.map(runtime::OperationTiming::start);
            operation.checkpoint_at(OperationPhase::Parse)?;
            let directive_recovery = if self.options.suppress_errors {
                DirectiveRecoveryMode::RecoverLine
            } else {
                DirectiveRecoveryMode::Strict
            };
            let preprocess_start = operation_timing.map(runtime::OperationTiming::start);
            let preprocessed = self.preprocess_for_with_directive_recovery_controlled(
                PreprocessPath::Render,
                directive_recovery,
                &control,
            )?;
            let Some((code, meta)) = (match preprocessed {
                Ok(preprocessed) => preprocessed,
                Err(error) => return Ok(Err(error)),
            }) else {
                return Ok(Ok(None));
            };
            let preprocess = preprocess_start.map(runtime::OperationTimer::elapsed);
            operation.checkpoint_at(OperationPhase::Semantic)?;
            let source_map = EditorParseSourceMap::new(&code);
            let parse_start = operation_timing.map(runtime::OperationTiming::start);
            let parsed = self.parse_render_semantic_model_controlled(
                source_map.parser_input(),
                &meta,
                &control,
            );
            let parse = parse_start.map(runtime::OperationTimer::elapsed);
            operation.checkpoint_at(OperationPhase::Semantic)?;
            let parsed = parsed?;
            let mut output = match parsed {
                Ok(output) => output,
                Err(error) => {
                    if !self.options.suppress_errors {
                        return Ok(Err(source_map.remap_parse_error(error)));
                    }
                    timing.log_suppressed_error(total_start, preprocess, parse, self.text.len());
                    return Ok(Ok(Some(error_diagram::suppressed_error_render_diagram(
                        &meta,
                        &source_map.remap_parse_error(error),
                    ))));
                }
            };
            let sanitize_start = operation_timing.map(runtime::OperationTiming::start);
            output
                .model_mut()
                .sanitize_common_db_fields(&meta.effective_config);
            let sanitize = sanitize_start.map(runtime::OperationTimer::elapsed);
            operation.checkpoint_at(OperationPhase::Semantic)?;
            let (line_offset, column_offset) = code.parser_position_offset();
            output
                .model_mut()
                .offset_parser_diagnostic_positions(line_offset, column_offset);
            output.model_mut().remap_warning_fact_spans(|fact| {
                Self::remap_warning_fact_spans(fact, &source_map);
            });
            operation.checkpoint_at(OperationPhase::Semantic)?;
            timing.log_success(ParseTimingSuccess {
                total_start,
                meta: &meta,
                model_kind: Some(output.model().kind()),
                preprocess,
                parse,
                sanitize,
                input_bytes: self.text.len(),
            });
            Ok(Ok(Some(ParsedDiagramRender::from_parse_output(
                meta, output,
            ))))
        })
    }

    fn finish_editor_semantic_facts(
        &self,
        mut facts: EditorSemanticFacts,
        meta: &ParseMetadata,
        source_map: &EditorParseSourceMap<'_>,
        control: &OperationControl,
    ) -> OperationControlResult<EditorSemanticFacts> {
        let family_directive_prefixes = std::mem::take(&mut facts.directive_prefixes);
        source_map.remap_facts(&mut facts, control)?;
        facts.family_semantics = family::diagram_type_editor_semantics(&meta.diagram_type)
            .expect("built-in combined semantic facts have typed editor family semantics");
        for expected in source_map.source.global_expected_syntax().chunks(128) {
            control.checkpoint()?;
            facts.expected_syntax.extend_from_slice(expected);
        }
        facts.finalize_expected_syntax_controlled(control)?;
        for (index, prefix) in source_map
            .source
            .global_directive_prefixes()
            .iter()
            .enumerate()
        {
            if index % 128 == 0 {
                control.checkpoint()?;
            }
            facts.push_directive_prefix(prefix.clone());
        }
        for (index, prefix) in family_directive_prefixes.into_iter().enumerate() {
            if index % 128 == 0 {
                control.checkpoint()?;
            }
            facts.push_directive_prefix(prefix);
        }
        control.checkpoint()?;
        Ok(facts)
    }

    // These callbacks are the explicit stages of the parse pipeline; bundling them into an
    // untyped options object would obscure their distinct ownership and lifetimes.
    #[allow(clippy::too_many_arguments)]
    fn parse_model<T, O>(
        &self,
        timing: ParseTiming,
        preprocess_path: PreprocessPath,
        parse: impl FnOnce(&Self, &str, &ParseMetadata) -> Result<T>,
        sanitize: impl FnOnce(&mut T, &MermaidConfig),
        suppressed: impl FnOnce(&ParseMetadata, &Error) -> O,
        finish: impl FnOnce(ParseMetadata, T) -> O,
        postprocess: impl FnOnce(&mut T, &EditorParseSourceMap<'_>),
        model_kind: impl FnOnce(&T) -> Option<&'static str>,
    ) -> Result<Option<O>> {
        self.with_operation_context(|operation_context| {
            self.parse_model_in_context(
                timing,
                preprocess_path,
                operation_context,
                parse,
                sanitize,
                suppressed,
                finish,
                postprocess,
                model_kind,
            )
        })
    }

    #[allow(clippy::too_many_arguments)]
    fn parse_model_in_context<T, O>(
        &self,
        timing: ParseTiming,
        preprocess_path: PreprocessPath,
        operation_context: &runtime::OperationContext,
        parse: impl FnOnce(&Self, &str, &ParseMetadata) -> Result<T>,
        sanitize: impl FnOnce(&mut T, &MermaidConfig),
        suppressed: impl FnOnce(&ParseMetadata, &Error) -> O,
        finish: impl FnOnce(ParseMetadata, T) -> O,
        postprocess: impl FnOnce(&mut T, &EditorParseSourceMap<'_>),
        model_kind: impl FnOnce(&T) -> Option<&'static str>,
    ) -> Result<Option<O>> {
        let operation_timing = timing.operation_timing(operation_context);
        let total_start = operation_timing.map(runtime::OperationTiming::start);

        let preprocess_start = operation_timing.map(runtime::OperationTiming::start);
        let Some((code, meta)) = self.preprocess_for(preprocess_path)? else {
            return Ok(None);
        };
        let source_map = EditorParseSourceMap::new(&code);
        let preprocess = preprocess_start.map(runtime::OperationTimer::elapsed);

        let parse_start = operation_timing.map(runtime::OperationTiming::start);
        let parse_res = parse(self, source_map.parser_input(), &meta);
        let parse = parse_start.map(runtime::OperationTimer::elapsed);

        let mut model = match parse_res {
            Ok(model) => model,
            Err(err) => {
                if !self.options.suppress_errors {
                    return Err(source_map.remap_parse_error(err));
                }

                timing.log_suppressed_error(total_start, preprocess, parse, self.text.len());
                return Ok(Some(suppressed(&meta, &source_map.remap_parse_error(err))));
            }
        };

        let sanitize_start = operation_timing.map(runtime::OperationTiming::start);
        sanitize(&mut model, &meta.effective_config);
        let sanitize = sanitize_start.map(runtime::OperationTimer::elapsed);
        postprocess(&mut model, &source_map);

        timing.log_success(ParseTimingSuccess {
            total_start,
            meta: &meta,
            model_kind: model_kind(&model),
            preprocess,
            parse,
            sanitize,
            input_bytes: self.text.len(),
        });

        Ok(Some(finish(meta, model)))
    }

    fn remap_compatibility_semantic_warnings(
        parsed: &mut CompatibilitySemanticParse,
        source_map: &EditorParseSourceMap<'_>,
    ) {
        match &mut parsed.warnings {
            CompatibilityWarnings::Typed(warning_facts) => {
                #[cfg(feature = "diagram-agentflow")]
                {
                    let (line_offset, column_offset) = source_map.source.parser_position_offset();
                    crate::diagrams::agentflow::offset_compatibility_diagnostic_positions(
                        parsed.model.as_value_mut(),
                        line_offset,
                        column_offset,
                    );
                }
                for fact in warning_facts.iter_mut() {
                    Self::remap_warning_fact_spans(fact, source_map);
                }
                Self::sync_compatibility_warning_facts(parsed.model.as_value_mut(), warning_facts);
            }
            CompatibilityWarnings::BuiltInWithoutWarnings => {}
            CompatibilityWarnings::CustomJson => {
                Self::remap_custom_compatibility_json_warning_facts(
                    parsed.model.as_value_mut(),
                    source_map,
                );
            }
        }
    }

    fn remap_custom_compatibility_json_warning_facts(
        model: &mut serde_json::Value,
        source_map: &EditorParseSourceMap<'_>,
    ) {
        let Some(warning_facts_value) = model.get_mut("warningFacts") else {
            return;
        };
        let Ok(mut warning_facts) = Vec::<DiagramWarningFact>::deserialize(&*warning_facts_value)
        else {
            return;
        };

        for fact in &mut warning_facts {
            Self::remap_warning_fact_spans(fact, source_map);
        }

        crate::config::replace_value_nonrecursive(
            warning_facts_value,
            serde_json::json!(warning_facts),
        );
    }

    fn decode_custom_warning_facts_controlled(
        model: &Value,
        control: &OperationControl,
    ) -> OperationControlResult<Option<Vec<DiagramWarningFact>>> {
        let Some(values) = model.get("warningFacts").and_then(Value::as_array) else {
            return Ok(None);
        };

        let mut warning_facts = Vec::with_capacity(values.len());
        for (index, value) in values.iter().enumerate() {
            if index.is_multiple_of(128) {
                control.checkpoint()?;
            }
            let Ok(fact) = DiagramWarningFact::deserialize(value) else {
                return Ok(None);
            };
            warning_facts.push(fact);
        }
        control.checkpoint()?;
        Ok(Some(warning_facts))
    }

    fn remap_warning_facts_controlled(
        warning_facts: &mut [DiagramWarningFact],
        source_map: &EditorParseSourceMap<'_>,
        control: &OperationControl,
    ) -> OperationControlResult<()> {
        for (index, fact) in warning_facts.iter_mut().enumerate() {
            if index.is_multiple_of(128) {
                control.checkpoint()?;
            }
            Self::remap_warning_fact_spans(fact, source_map);
        }
        control.checkpoint()?;
        Ok(())
    }

    fn sync_compatibility_warning_facts(model: &mut Value, warning_facts: &[DiagramWarningFact]) {
        let Some(value) = model.get_mut("warningFacts") else {
            return;
        };
        crate::config::replace_value_nonrecursive(
            value,
            serde_json::to_value(warning_facts)
                .expect("diagram warning facts must remain JSON-serializable"),
        );
    }

    fn remap_warning_fact_spans(
        fact: &mut DiagramWarningFact,
        source_map: &EditorParseSourceMap<'_>,
    ) {
        let source_span = fact.span;
        let remapped_span =
            source_span.and_then(|span| source_map.try_remap_warning_source_span(span));
        fact.span = remapped_span;
        fact.fix_span = match (fact.fix_span, source_span, remapped_span) {
            (Some(fix_span), Some(source_span), Some(remapped_span))
                if fix_span.start == fix_span.end && fix_span.start == source_span.end =>
            {
                Some(SourceSpan::new(remapped_span.end, remapped_span.end))
            }
            (Some(fix_span), _, _) => source_map.try_remap_warning_source_span(fix_span),
            (None, _, _) => None,
        };
    }

    fn parse_render_semantic_model(
        &self,
        code: &str,
        meta: &ParseMetadata,
    ) -> Result<RenderSemanticParseOutput> {
        let control = OperationControl::new();
        self.parse_render_semantic_model_controlled(code, meta, &control)
            .map_err(Error::from)?
    }

    fn parse_render_semantic_model_controlled(
        &self,
        code: &str,
        meta: &ParseMetadata,
        control: &OperationControl,
    ) -> OperationControlResult<Result<RenderSemanticParseOutput>> {
        control.checkpoint()?;
        let semantic = self.engine.diagram_registry.resolve(&meta.diagram_type);
        let render = self
            .engine
            .render_diagram_registry
            .resolve(&meta.diagram_type);

        if let Some(ResolvedRenderParser::Custom(parser)) = render {
            let parsed = parser(code, meta, control);
            control.checkpoint()?;
            return Ok(parsed?
                .map(RenderSemanticModel::CustomJson)
                .map(RenderSemanticParseOutput::new));
        }

        if let Some(ResolvedSemanticParser::Custom(_)) = semantic {
            return Ok(diagram::parse_or_unsupported_controlled(
                &self.engine.diagram_registry,
                &meta.diagram_type,
                code,
                meta,
                control,
            )?
            .map(|value| {
                RenderSemanticModel::CustomJson(CustomJsonRenderModel::from_semantic_registry(
                    meta.diagram_type.clone(),
                    value,
                ))
            })
            .map(RenderSemanticParseOutput::new));
        }

        if let Some(ResolvedRenderParser::BuiltIn(parser)) = render {
            let parsed = parser(code, meta, control);
            control.checkpoint()?;
            return parsed;
        }

        if let Some(ResolvedSemanticParser::BuiltIn(_)) = semantic {
            return Ok(Err(Error::diagram_parse_fallback(
                meta.diagram_type.clone(),
                format!(
                    "built-in diagram type `{}` is missing a typed render parser; the custom JSON boundary is reserved for custom registry adapters",
                    meta.diagram_type
                ),
            )));
        }

        Ok(Err(Error::UnsupportedDiagram {
            diagram_type: meta.diagram_type.clone(),
        }))
    }

    fn preprocess_for(
        &self,
        path: PreprocessPath,
    ) -> Result<Option<(PreprocessedSource, ParseMetadata)>> {
        let directive_recovery = if self.options.suppress_errors {
            DirectiveRecoveryMode::RecoverLine
        } else {
            DirectiveRecoveryMode::Strict
        };
        self.preprocess_for_with_directive_recovery(path, directive_recovery)
    }

    fn preprocess_for_with_directive_recovery(
        &self,
        path: PreprocessPath,
        directive_recovery: DirectiveRecoveryMode,
    ) -> Result<Option<(PreprocessedSource, ParseMetadata)>> {
        let control = OperationControl::new();
        self.preprocess_for_with_directive_recovery_controlled(path, directive_recovery, &control)
            .expect("a private parse control cannot be cancelled")
    }

    fn preprocess_for_with_directive_recovery_controlled(
        &self,
        path: PreprocessPath,
        directive_recovery: DirectiveRecoveryMode,
        control: &OperationControl,
    ) -> OperationControlResult<Result<Option<(PreprocessedSource, ParseMetadata)>>> {
        let preprocessed =
            self.preprocess_with_directive_recovery_controlled(path, directive_recovery, control)?;
        Ok(match preprocessed {
            Err(Error::DetectType(_)) if self.options.suppress_errors => Ok(None),
            result => result.map(Some),
        })
    }

    fn preprocess_strict(
        &self,
        path: PreprocessPath,
    ) -> Result<(PreprocessedSource, ParseMetadata)> {
        self.preprocess_with_directive_recovery(path, DirectiveRecoveryMode::Strict)
    }

    fn preprocess_with_directive_recovery(
        &self,
        path: PreprocessPath,
        directive_recovery: DirectiveRecoveryMode,
    ) -> Result<(PreprocessedSource, ParseMetadata)> {
        let control = OperationControl::new();
        self.preprocess_with_directive_recovery_controlled(path, directive_recovery, &control)
            .expect("a private parse control cannot be cancelled")
    }

    fn preprocess_with_directive_recovery_controlled(
        &self,
        path: PreprocessPath,
        directive_recovery: DirectiveRecoveryMode,
        control: &OperationControl,
    ) -> OperationControlResult<Result<(PreprocessedSource, ParseMetadata)>> {
        control.checkpoint()?;
        match self.source {
            ParseSource::Detect => {
                self.preprocess_and_detect_strict_controlled(path, directive_recovery, control)
            }
            ParseSource::KnownType(diagram_type) => self.preprocess_and_assume_type_controlled(
                diagram_type,
                path,
                directive_recovery,
                control,
            ),
        }
    }

    fn preprocess_with_evidence_controlled(
        &self,
        path: PreprocessPath,
        directive_recovery: DirectiveRecoveryMode,
        control: &OperationControl,
    ) -> OperationControlResult<PreparedPreprocessCapture> {
        control.checkpoint()?;
        let known_type = match self.source {
            ParseSource::Detect => None,
            ParseSource::KnownType(diagram_type) => Some(diagram_type),
        };
        let captured = self.preprocess_input_with_evidence_controlled(
            path,
            known_type,
            directive_recovery,
            control,
        )?;
        let outcome = match captured.outcome {
            crate::preprocess::PreprocessCaptureResult::Ready(preprocessed) => {
                match std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                    self.finish_preprocessed_controlled(*preprocessed, known_type, control)
                })) {
                    Ok(result) => match result? {
                        Ok((source, metadata)) => {
                            PreparedPreprocessOutcome::Ready(source, metadata)
                        }
                        Err(error) => PreparedPreprocessOutcome::Failed(error),
                    },
                    Err(payload) => {
                        PreparedPreprocessOutcome::Panicked(CapturedPanic::from_payload(payload))
                    }
                }
            }
            crate::preprocess::PreprocessCaptureResult::Failed(error) => {
                PreparedPreprocessOutcome::Failed(error)
            }
            crate::preprocess::PreprocessCaptureResult::Panicked(panic) => {
                PreparedPreprocessOutcome::Panicked(panic)
            }
        };
        Ok(PreparedPreprocessCapture {
            outcome,
            source_config: captured.source_config,
        })
    }

    fn preprocess_and_detect_strict_controlled(
        &self,
        path: PreprocessPath,
        directive_recovery: DirectiveRecoveryMode,
        control: &OperationControl,
    ) -> OperationControlResult<Result<(PreprocessedSource, ParseMetadata)>> {
        let pre = match self.preprocess_input_with_directive_recovery_controlled(
            path,
            None,
            directive_recovery,
            control,
        )? {
            Ok(pre) => pre,
            Err(error) => return Ok(Err(error)),
        };
        self.finish_preprocessed_controlled(pre, None, control)
    }

    fn preprocess_and_assume_type_controlled(
        &self,
        diagram_type: &str,
        path: PreprocessPath,
        directive_recovery: DirectiveRecoveryMode,
        control: &OperationControl,
    ) -> OperationControlResult<Result<(PreprocessedSource, ParseMetadata)>> {
        let pre = match self.preprocess_input_with_directive_recovery_controlled(
            path,
            Some(diagram_type),
            directive_recovery,
            control,
        )? {
            Ok(pre) => pre,
            Err(error) => return Ok(Err(error)),
        };
        self.finish_preprocessed_controlled(pre, Some(diagram_type), control)
    }

    fn finish_preprocessed_controlled(
        &self,
        mut pre: crate::PreprocessResult,
        known_type: Option<&str>,
        control: &OperationControl,
    ) -> OperationControlResult<Result<(PreprocessedSource, ParseMetadata)>> {
        control.checkpoint()?;
        if pre.code().trim_start().starts_with("---") {
            return Ok(Err(Error::MalformedFrontMatter));
        }

        let has_config_overrides = !pre.config.is_empty_object();
        let (mut effective_config, source_config) =
            self.effective_config_before_detect(&pre.config, control)?;
        let cached_effective_config = (!has_config_overrides).then(|| effective_config.clone());
        let track_detector = !self.engine.presentation_defaults.is_empty_object();
        let detector_before = track_detector.then(|| effective_config.begin_mutation_tracking());
        let diagram_type = match known_type {
            Some(diagram_type) => diagram_type.to_string(),
            None => match self.engine.registry.detect_type_precleaned_controlled(
                pre.code(),
                &mut effective_config,
                control,
            )? {
                Ok(diagram_type) => diagram_type.to_owned(),
                Err(error) => return Ok(Err(error)),
            },
        };
        let detector_writes = if let Some(before) = detector_before {
            effective_config.take_mutation_paths(&before)
        } else {
            Vec::new()
        };
        control.checkpoint()?;
        let external_palette = crate::config::resolve_appearance(
            &diagram_type,
            &source_config,
            &self.engine.site_config_delta,
            &self.engine.presentation_defaults,
            &crate::generated::upstream_default_config(),
            &mut effective_config,
        );
        // The explicit keyword is the only detector-level layout selection retained in
        // Mermaid 12. Apply it after appearance resolution for known-type entrypoints too.
        if diagram_type == "flowchart-elk" {
            effective_config.set_value("layout", serde_json::Value::String("elk".to_owned()));
        }
        if effective_config.get_str("theme") == Some("null") {
            // The sentinel disables theme selection. Upstream retains the initialized
            // variables and merges source overrides without running another theme program.
            let mut initialized = match self.engine.default_effective_config() {
                Ok(config) => config,
                Err(error) => return Ok(Err(error)),
            };
            initialized.deep_merge(source_config.as_value());
            if let Some(variables) = initialized.as_value().get("themeVariables") {
                effective_config.set_value(
                    "themeVariables",
                    crate::config::clone_value_nonrecursive(variables),
                );
            }
        } else if external_palette && track_detector {
            if let Err(error) = crate::config::apply_external_presentation_palette(
                &mut effective_config,
                &self.engine.presentation_defaults,
                &self.engine.presentation_non_paint_defaults,
                &self.engine.site_config_delta,
                &source_config,
                Some(&detector_writes),
            ) {
                return Ok(Err(error.into()));
            }
        } else if has_config_overrides {
            if let Err(error) = theme::apply_theme_defaults(&mut effective_config) {
                return Ok(Err(error.into()));
            }
        } else if cached_effective_config
            .as_ref()
            .is_some_and(|cached| effective_config.ptr_eq(cached))
        {
            effective_config = match self.engine.default_effective_config() {
                Ok(config) => config,
                Err(error) => return Ok(Err(error)),
            };
        } else if let Err(error) = theme::apply_theme_defaults(&mut effective_config) {
            return Ok(Err(error.into()));
        }

        control.checkpoint()?;
        effective_config.set_presentation_defaults_active(
            self.engine
                .presentation_defaults
                .presentation_defaults_active(),
        );
        effective_config.set_presentation_font_defaults_active(
            self.engine
                .presentation_defaults
                .presentation_font_defaults_active(),
        );
        effective_config.set_presentation_c4_line_default_active(
            self.engine
                .presentation_defaults
                .presentation_c4_line_default_active(),
        );
        let axis_text_path = "themeVariables.wardley.axisTextColor";
        effective_config.set_wardley_axis_text_authored(
            self.engine
                .site_config_delta
                .get_str(axis_text_path)
                .is_some()
                || source_config.get_str(axis_text_path).is_some()
                || detector_writes.iter().any(|path| {
                    path.is_empty()
                        || path == axis_text_path
                        || axis_text_path
                            .strip_prefix(path)
                            .is_some_and(|suffix| suffix.starts_with('.'))
                }),
        );
        let title = sanitized_title(pre.title.as_deref(), &effective_config);
        control.checkpoint()?;

        if matches!(diagram_type.as_str(), "agentflow" | "eventmodeling")
            && let Some(with_comments) = pre.with_comments.take()
        {
            let max_text_size = effective_config
                .as_value()
                .get("maxTextSize")
                .and_then(Value::as_u64)
                .unwrap_or(50_000);
            if with_comments.text().encode_utf16().count() as u64 <= max_text_size {
                pre.source =
                    crate::preprocess::prepare_parser_text_controlled(with_comments, control)?;
            }
        }

        Ok(Ok((
            pre.source,
            ParseMetadata {
                diagram_type,
                config: pre.config,
                effective_config,
                title,
            },
        )))
    }

    #[cfg(test)]
    fn preprocess_input(
        &self,
        path: PreprocessPath,
        diagram_type: Option<&str>,
    ) -> Result<crate::PreprocessResult> {
        self.preprocess_input_with_directive_recovery(
            path,
            diagram_type,
            DirectiveRecoveryMode::Strict,
        )
    }

    #[cfg(test)]
    fn preprocess_input_with_directive_recovery(
        &self,
        path: PreprocessPath,
        diagram_type: Option<&str>,
        directive_recovery: DirectiveRecoveryMode,
    ) -> Result<crate::PreprocessResult> {
        let control = OperationControl::new();
        self.preprocess_input_with_directive_recovery_controlled(
            path,
            diagram_type,
            directive_recovery,
            &control,
        )
        .expect("a private parse control cannot be cancelled")
    }

    fn preprocess_input_with_directive_recovery_controlled(
        &self,
        path: PreprocessPath,
        diagram_type: Option<&str>,
        directive_recovery: DirectiveRecoveryMode,
        control: &OperationControl,
    ) -> OperationControlResult<Result<crate::PreprocessResult>> {
        control.checkpoint()?;
        match path {
            PreprocessPath::PublicParse => {
                preprocess_mermaid_public_parse_pipeline_with_directive_recovery_controlled(
                    self.text,
                    &self.engine.registry,
                    diagram_type,
                    directive_recovery,
                    control,
                )
            }
            PreprocessPath::Render => {
                preprocess_diagram_with_known_type_and_directive_recovery_controlled(
                    self.text,
                    &self.engine.registry,
                    diagram_type,
                    directive_recovery,
                    control,
                )
            }
        }
    }

    fn preprocess_input_with_evidence_controlled(
        &self,
        path: PreprocessPath,
        diagram_type: Option<&str>,
        directive_recovery: DirectiveRecoveryMode,
        control: &OperationControl,
    ) -> OperationControlResult<PreprocessCaptureOutcome> {
        control.checkpoint()?;
        match path {
            PreprocessPath::PublicParse => {
                preprocess_mermaid_public_parse_pipeline_with_directive_recovery_evidence_controlled(
                    self.text,
                    &self.engine.registry,
                    diagram_type,
                    directive_recovery,
                    control,
                )
            }
            PreprocessPath::Render => {
                preprocess_diagram_with_known_type_and_directive_recovery_evidence_controlled(
                    self.text,
                    &self.engine.registry,
                    diagram_type,
                    directive_recovery,
                    control,
                )
            }
        }
    }

    fn with_operation_context<R>(
        &self,
        f: impl FnOnce(&runtime::OperationContext) -> Result<R>,
    ) -> Result<R> {
        let context = self.engine.begin_operation()?;
        runtime::with_operation_context(&context, || f(&context))
    }

    fn effective_config_before_detect(
        &self,
        overrides: &MermaidConfig,
        control: &OperationControl,
    ) -> OperationControlResult<(MermaidConfig, MermaidConfig)> {
        if overrides.is_empty_object() {
            return Ok((self.engine.site_config.clone(), overrides.clone()));
        }

        let mut effective_config = self.engine.site_config.clone();
        let effective_overrides = effective_config.source_filtered_overrides(overrides, control)?;
        effective_config.deep_merge(effective_overrides.as_value());
        Ok((effective_config, effective_overrides))
    }
}

impl ParseTiming {
    fn operation_timing(
        self,
        context: &runtime::OperationContext,
    ) -> Option<runtime::OperationTiming> {
        (self != Self::None).then(|| context.timing()).flatten()
    }

    fn log_suppressed_error(
        self,
        total_start: Option<runtime::OperationTimer>,
        preprocess: Option<std::time::Duration>,
        parse: Option<std::time::Duration>,
        input_bytes: usize,
    ) {
        let Some(start) = total_start else {
            return;
        };

        match self {
            Self::None => {}
            Self::Json => {
                eprintln!(
                    "[parse-timing] diagram=error total={:?} preprocess={:?} parse={:?} sanitize={:?} input_bytes={}",
                    start.elapsed(),
                    preprocess.unwrap_or_default(),
                    parse.unwrap_or_default(),
                    std::time::Duration::ZERO,
                    input_bytes,
                );
            }
            Self::Render => {
                eprintln!(
                    "[parse-render-timing] diagram=error model=json total={:?} preprocess={:?} parse={:?} sanitize={:?} input_bytes={}",
                    start.elapsed(),
                    preprocess.unwrap_or_default(),
                    parse.unwrap_or_default(),
                    std::time::Duration::ZERO,
                    input_bytes,
                );
            }
        }
    }

    fn log_success(self, success: ParseTimingSuccess<'_>) {
        let Some(start) = success.total_start else {
            return;
        };

        match self {
            Self::None => {}
            Self::Json => {
                eprintln!(
                    "[parse-timing] diagram={} total={:?} preprocess={:?} parse={:?} sanitize={:?} input_bytes={}",
                    success.meta.diagram_type,
                    start.elapsed(),
                    success.preprocess.unwrap_or_default(),
                    success.parse.unwrap_or_default(),
                    success.sanitize.unwrap_or_default(),
                    success.input_bytes,
                );
            }
            Self::Render => {
                eprintln!(
                    "[parse-render-timing] diagram={} model={} total={:?} preprocess={:?} parse={:?} sanitize={:?} input_bytes={}",
                    success.meta.diagram_type,
                    success.model_kind.unwrap_or("unknown"),
                    start.elapsed(),
                    success.preprocess.unwrap_or_default(),
                    success.parse.unwrap_or_default(),
                    success.sanitize.unwrap_or_default(),
                    success.input_bytes,
                );
            }
        }
    }
}

struct ParseTimingSuccess<'a> {
    total_start: Option<runtime::OperationTimer>,
    meta: &'a ParseMetadata,
    model_kind: Option<&'static str>,
    preprocess: Option<std::time::Duration>,
    parse: Option<std::time::Duration>,
    sanitize: Option<std::time::Duration>,
    input_bytes: usize,
}

fn sanitized_title(title: Option<&str>, effective_config: &MermaidConfig) -> Option<String> {
    title
        .map(|title| sanitize::sanitize_text(title, effective_config))
        .filter(|title| !title.is_empty())
}

#[cfg(test)]
mod editor_parse_source_map_tests {
    use std::sync::atomic::{AtomicUsize, Ordering};
    #[cfg(not(all(target_arch = "wasm32", target_os = "unknown")))]
    use std::time::Duration;

    use super::{CustomJsonRenderModel, EditorParseSourceMap, ParsePipeline, PreprocessPath};
    use crate::{
        CancelReason, DetectorRegistry, DiagramSnapshotCapture, EditorExpectedSyntax,
        EditorExpectedSyntaxKind, EditorSemanticFacts, EditorSemanticKind, EditorSemanticSymbol,
        Engine, Error, MermaidConfig, OperationCancelled, OperationControl, OperationControlResult,
        OperationPhase, ParseMetadata, ParseOptions, Result, SourceSpan,
    };

    fn panicking_detector(_source: &str, _config: &mut MermaidConfig) -> bool {
        panic!("detector fixture panic")
    }

    static FRONTMATTER_PROBE_CALLS: AtomicUsize = AtomicUsize::new(0);

    fn frontmatter_probe_detector(_source: &str, _config: &mut MermaidConfig) -> bool {
        FRONTMATTER_PROBE_CALLS.fetch_add(1, Ordering::Relaxed);
        panic!("frontmatter probe detector must not run")
    }

    fn cancelling_render_parser(
        _code: &str,
        meta: &ParseMetadata,
        control: &OperationControl,
    ) -> OperationControlResult<Result<CustomJsonRenderModel>> {
        control.cancel();
        Ok(Err(Error::diagram_parse_fallback(
            meta.diagram_type.clone(),
            "custom parser failed after requesting cancellation",
        )))
    }

    #[cfg(not(all(target_arch = "wasm32", target_os = "unknown")))]
    fn expiring_render_parser(
        _code: &str,
        meta: &ParseMetadata,
        control: &OperationControl,
    ) -> OperationControlResult<Result<CustomJsonRenderModel>> {
        assert!(
            control.set_deadline(Duration::ZERO),
            "deadline fixture must install a fresh deadline"
        );
        Ok(Err(Error::diagram_parse_fallback(
            meta.diagram_type.clone(),
            "custom parser failed after expiring its deadline",
        )))
    }

    #[test]
    fn controlled_snapshot_stops_before_a_cancelled_operation() {
        let control = OperationControl::new();
        control.cancel();

        let result =
            Engine::new().parse_diagram_snapshot_controlled_sync("flowchart TD\nA-->B\n", &control);

        assert!(matches!(result, Err(OperationCancelled { .. })));
    }

    #[test]
    fn custom_render_parse_error_cannot_mask_callback_cancellation() {
        let mut engine = Engine::new();
        engine
            .render_diagram_registry_mut()
            .insert("flowchart-v2", cancelling_render_parser);
        let control = OperationControl::new();

        let error = engine
            .parse_diagram_for_render_model_with_type_controlled_sync(
                "flowchart-v2",
                "flowchart TD\nA -->\n",
                ParseOptions::lenient(),
                &control,
            )
            .expect_err("callback cancellation must win over a suppressible parse error");

        assert_eq!(error.phase, OperationPhase::Parse);
        assert_eq!(error.reason, CancelReason::Requested);
    }

    #[cfg(not(all(target_arch = "wasm32", target_os = "unknown")))]
    #[test]
    fn custom_render_parse_error_cannot_mask_callback_deadline() {
        let mut engine = Engine::new();
        engine
            .render_diagram_registry_mut()
            .insert("flowchart-v2", expiring_render_parser);
        let control = OperationControl::new();

        let error = engine
            .parse_diagram_for_render_model_with_type_controlled_sync(
                "flowchart-v2",
                "flowchart TD\nA -->\n",
                ParseOptions::lenient(),
                &control,
            )
            .expect_err("callback deadline must win over a suppressible parse error");

        assert_eq!(error.phase, OperationPhase::Parse);
        assert_eq!(error.reason, CancelReason::DeadlineExceeded);
    }

    #[test]
    fn controlled_capture_cancellation_never_returns_partial_evidence() {
        let control = OperationControl::new();
        control.cancel();

        let result = Engine::new().capture_diagram_snapshot_controlled_sync(
            "%%{init: {theme: 'dark'}}%%\nflowchart TD\nA-->B\n",
            &control,
        );

        assert!(matches!(result, Err(OperationCancelled { .. })));
    }

    #[test]
    fn editor_capture_retains_source_config_on_detection_failure() {
        let source = "%%{ initialize: { theme: 'dark' } }%%\nnot a diagram\n";
        let captured = Engine::new()
            .capture_diagram_snapshot_controlled_sync(source, &OperationControl::new())
            .expect("an active parse control must not cancel");

        let DiagramSnapshotCapture::Failed {
            error: Error::DetectType(_),
            source_config,
        } = captured
        else {
            panic!("strict detection failure must retain preprocessing evidence");
        };
        assert_eq!(source_config.directives().len(), 1);
        let keyword = source_config.directives()[0].keyword_span();
        assert_eq!(&source[keyword.start..keyword.end], "initialize");
    }

    #[test]
    fn editor_capture_retains_source_config_when_init_config_detection_panics() {
        let source = concat!(
            "%%{ initialize: { lazyLoadedDiagrams: true, config: { htmlLabels: false } } }%%\n",
            "detector-panic-fixture\n",
        );
        let mut engine = Engine::new();
        *engine.registry_mut() = DetectorRegistry::new();
        engine
            .registry_mut()
            .add_fn("detector-panic-fixture", panicking_detector);

        let captured = engine
            .capture_diagram_snapshot_controlled_sync(source, &OperationControl::new())
            .expect("an active parse control must not cancel");

        let DiagramSnapshotCapture::Panicked {
            panic,
            source_config,
        } = captured
        else {
            panic!("detector panic must remain a capture outcome");
        };
        assert!(panic.message().contains("detector fixture panic"));
        assert_eq!(source_config.directives().len(), 1);
        let directive = &source_config.directives()[0];
        assert_eq!(directive.keyword(), "initialize");
        assert!(
            source_config
                .keys()
                .iter()
                .any(|entry| entry.matches_path(&["lazyLoadedDiagrams"]))
        );
    }

    #[test]
    fn legacy_snapshot_facade_resumes_the_original_detector_panic_payload() {
        let source = "detector-panic-fixture\n";
        let mut engine = Engine::new();
        *engine.registry_mut() = DetectorRegistry::new();
        engine
            .registry_mut()
            .add_fn("detector-panic-fixture", panicking_detector);

        let payload = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            let _ = engine.parse_diagram_snapshot_sync(source);
        }))
        .expect_err("the legacy snapshot facade must resume detector panics");

        assert_eq!(
            payload.downcast_ref::<&'static str>(),
            Some(&"detector fixture panic")
        );
    }

    #[test]
    fn editor_capture_retains_later_source_config_on_preprocessing_failure() {
        let source = concat!(
            "---\n",
            "config: [\n",
            "---\n",
            "%%{ initialize: { theme: 'dark' } }%%\n",
            "flowchart TD\nA-->B\n",
        );
        let captured = Engine::new()
            .capture_diagram_snapshot_controlled_sync(source, &OperationControl::new())
            .expect("an active parse control must not cancel");

        let DiagramSnapshotCapture::Failed {
            error: Error::InvalidFrontMatterYaml { .. },
            source_config,
        } = captured
        else {
            panic!("preprocessing failure must be distinct from cancellation");
        };
        assert_eq!(source_config.directives().len(), 1);
        assert_eq!(source_config.directives()[0].keyword(), "initialize");
        assert!(!source_config.rewrite_safe());
    }

    #[test]
    fn editor_capture_preserves_frontmatter_error_before_later_detector_panic() {
        let source = concat!(
            "---\n",
            "config: [\n",
            "---\n",
            "%%{ initialize: { config: { htmlLabels: false } } }%%\n",
            "detector-panic-fixture\n",
        );
        let mut engine = Engine::new();
        *engine.registry_mut() = DetectorRegistry::new();
        FRONTMATTER_PROBE_CALLS.store(0, Ordering::Relaxed);
        engine
            .registry_mut()
            .add_fn("detector-panic-fixture", frontmatter_probe_detector);

        let captured = engine
            .capture_diagram_snapshot_controlled_sync(source, &OperationControl::new())
            .expect("an active parse control must not cancel");

        let DiagramSnapshotCapture::Failed {
            error: Error::InvalidFrontMatterYaml { .. },
            source_config,
        } = captured
        else {
            panic!("the first preprocessing error must win over later detector work");
        };
        assert_eq!(source_config.directives().len(), 1);
        assert_eq!(source_config.directives()[0].keyword(), "initialize");
        assert!(!source_config.rewrite_safe());
        assert_eq!(FRONTMATTER_PROBE_CALLS.load(Ordering::Relaxed), 0);
    }

    #[test]
    fn lenient_editor_capture_preserves_suppressed_detection_semantics() {
        let engine = Engine::new();
        let pipeline = ParsePipeline::detect(&engine, "not a diagram\n", ParseOptions::lenient());
        let captured = pipeline
            .capture_editor_snapshot_controlled(super::ParseTiming::Json, &OperationControl::new())
            .expect("an active parse control must not cancel");

        assert!(matches!(captured, DiagramSnapshotCapture::Snapshot(None)));
    }

    #[test]
    #[cfg(feature = "diagram-flowchart")]
    fn controlled_snapshot_stops_during_family_parser_work() {
        let mut source = String::from("flowchart TD\n");
        for index in 0..4_096 {
            source.push_str(&format!("n{index}-->n{}\n", index + 1));
        }
        let control = OperationControl::new();
        control.cancel_after_checkpoints(128);
        crate::diagrams::flowchart::reset_flowchart_token_trace_construction_count();

        let result = Engine::new().parse_diagram_snapshot_controlled_sync(&source, &control);

        assert!(matches!(result, Err(OperationCancelled { .. })));
        assert_eq!(
            crate::diagrams::flowchart::flowchart_token_trace_construction_count(),
            0,
            "the cancellation schedule must stop in the family-owned accessibility scan"
        );
    }

    #[test]
    #[cfg(feature = "diagram-flowchart")]
    fn active_control_preserves_the_snapshot_model() {
        let source = "flowchart TD\nA-->B\n";
        let engine = Engine::new();
        let regular = engine
            .parse_diagram_snapshot_sync(source)
            .expect("regular parse succeeds")
            .expect("regular snapshot");
        let controlled = engine
            .parse_diagram_snapshot_controlled_sync(source, &OperationControl::new())
            .expect("active control")
            .expect("controlled parse succeeds")
            .expect("controlled snapshot");

        assert_eq!(
            controlled.metadata().diagram_type,
            regular.metadata().diagram_type
        );
        assert_eq!(
            controlled.outcome().parsed_model(),
            regular.outcome().parsed_model()
        );
    }

    #[test]
    fn public_and_render_preprocessing_share_one_frontmatter_pass() {
        let input = concat!(
            "   ---\n",
            "title: only-visible-after-trimming\n",
            "---\n",
            "flowchart TD\n",
            "A-->B\n",
        );
        let engine = Engine::new();
        let pipeline = ParsePipeline::detect(&engine, input, ParseOptions::strict());

        let render = pipeline
            .preprocess_input(PreprocessPath::Render, None)
            .expect("single render preprocess");
        let public_parse = pipeline
            .preprocess_input(PreprocessPath::PublicParse, None)
            .expect("public parse preprocess");

        assert!(render.code().starts_with("---"));
        assert_eq!(public_parse.code(), render.code());
    }

    #[test]
    fn render_rejects_a_second_frontmatter_block() {
        let input = concat!(
            "---\n",
            "title: outer\n",
            "---\n",
            "---\n",
            "title: inner\n",
            "---\n",
            "flowchart TD\n",
            "A-->B\n",
        );
        let engine = Engine::new();

        let error = engine
            .parse_diagram_for_render_model_sync(input, ParseOptions::strict())
            .expect_err("render preprocessing must leave the second block visible");

        assert!(matches!(error, crate::Error::MalformedFrontMatter));
    }

    #[test]
    fn fact_remap_drops_only_the_fact_crossing_a_deleted_span() {
        let original = "flowchart TD\nA%%{wrap}%%B\nC\n";
        let engine = Engine::new();
        let preprocessed = crate::preprocess::preprocess_mermaid_public_parse_pipeline(
            original,
            &engine.registry,
            None,
        )
        .unwrap()
        .source;
        let map = EditorParseSourceMap::new(&preprocessed);
        let mut facts = EditorSemanticFacts::new();
        let joined_start = preprocessed.text().find("AB").unwrap();
        facts.push_symbol(EditorSemanticSymbol::new(
            "AB",
            None,
            EditorSemanticKind::Variable,
            SourceSpan::new(joined_start, joined_start + 2),
            SourceSpan::new(joined_start, joined_start + 2),
        ));
        let c = preprocessed.text().find('C').unwrap();
        facts.push_symbol(EditorSemanticSymbol::new(
            "C",
            None,
            EditorSemanticKind::Variable,
            SourceSpan::new(c, c + 1),
            SourceSpan::new(c, c + 1),
        ));
        facts.push_expected_syntax(EditorExpectedSyntax::new(
            EditorExpectedSyntaxKind::NodeIdentifier,
            SourceSpan::new(c, c + 1),
        ));

        map.remap_facts(&mut facts, &OperationControl::new())
            .expect("a private parse control cannot be cancelled");

        assert_eq!(facts.symbols.len(), 1);
        assert_eq!(facts.symbols[0].name, "C");
        assert_eq!(
            &original[facts.symbols[0].span.start..facts.symbols[0].span.end],
            "C"
        );
        assert_eq!(facts.expected_syntax.len(), 1);
        assert!(
            facts
                .diagnostics
                .iter()
                .any(|diagnostic| diagnostic.message.contains("dropped 1 editor fact span"))
        );
    }

    #[test]
    fn fact_remap_observes_cancellation_during_large_fact_batches() {
        let original = "flowchart TD\nA\n";
        let engine = Engine::new();
        let preprocessed = crate::preprocess::preprocess_mermaid_public_parse_pipeline(
            original,
            &engine.registry,
            None,
        )
        .unwrap()
        .source;
        let map = EditorParseSourceMap::new(&preprocessed);
        let mut facts = EditorSemanticFacts::new();
        let offset = preprocessed.text().find('A').unwrap();
        for index in 0..256 {
            facts.push_symbol(EditorSemanticSymbol::new(
                format!("node-{index}"),
                None,
                EditorSemanticKind::Variable,
                SourceSpan::new(offset, offset + 1),
                SourceSpan::new(offset, offset + 1),
            ));
        }
        let control = OperationControl::new();
        control.cancel_after_checkpoints(1);

        assert!(matches!(
            map.remap_facts(&mut facts, &control),
            Err(crate::OperationCancelled { .. })
        ));
        assert!(control.is_cancelled());
    }
}
