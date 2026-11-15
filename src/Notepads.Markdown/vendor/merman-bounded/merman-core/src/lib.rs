#![forbid(unsafe_code)]
//! Mermaid parser + semantic model (headless).
//!
//! Design goals:
//! - 1:1 parity with the repository's pinned upstream Mermaid baseline
//! - deterministic, testable outputs (semantic snapshot goldens)
//! - runtime-agnostic async APIs (no specific executor required)
//!
//! Default features are empty. Select `diagram-*` features for the required logical families or
//! `all-diagrams` for the complete parser surface. Family-exclusive public model types and enum
//! variants are conditional; recognition and custom registries remain available without a family.

pub mod baseline;
pub mod common;
pub mod common_db;
mod compatibility_json;
pub mod config;
pub mod detect;
pub mod diagram;
pub mod diagrams;
pub mod editor;
pub mod entities;
pub mod error;
mod family;
pub mod generated;
pub mod geom;
#[cfg(any(
    test,
    feature = "diagram-flowchart",
    feature = "diagram-swimlane",
    feature = "diagram-agentflow",
    feature = "diagram-kanban",
    feature = "diagram-sequence"
))]
mod inline_config;
pub mod models;
pub mod native_collections;
pub mod operation;
mod parse_pipeline;
pub mod preprocess;
pub mod resources;
pub mod runtime;
pub mod sanitize;
pub mod svg_security;
#[doc(hidden)]
pub mod terminal_text;
mod theme;
pub mod theme_color;
pub mod time;
pub mod utils;
mod yaml_config;

pub use compatibility_json::ManagedSemanticJson;
pub use config::MermaidConfig;

#[doc(hidden)]
pub fn presentation_defaults_active_internal(config: &MermaidConfig) -> bool {
    config.presentation_defaults_active()
}

#[doc(hidden)]
pub fn presentation_font_defaults_active_internal(config: &MermaidConfig) -> bool {
    config.presentation_font_defaults_active()
}

#[doc(hidden)]
pub fn presentation_c4_line_default_active_internal(config: &MermaidConfig) -> bool {
    config.presentation_c4_line_default_active()
}

#[doc(hidden)]
pub fn wardley_axis_text_authored_internal(config: &MermaidConfig) -> bool {
    config.wardley_axis_text_authored()
}
pub use detect::{Detector, DetectorRegistry};
#[cfg(feature = "diagram-flowchart")]
pub use diagrams::flowchart::{NativeFeature, NativeParseError, NativeParsePolicy, parse_flowchart_native_controlled};
pub use diagram::{
    AGENTFLOW_CONTAINMENT_VIOLATION_WARNING_RULE_ID, AGENTFLOW_SHAPE_REMOVED_WARNING_RULE_ID,
    AGENTFLOW_SHAPE_UNSUPPORTED_WARNING_RULE_ID, BLOCK_WIDTH_WARNING_RULE_ID,
    BuiltinRenderSemantic, CapturedPanic, CustomJsonProvenance, CustomJsonRenderModel,
    CustomJsonRenderParser, DiagramParseOutcome, DiagramParseSnapshot, DiagramRegistry,
    DiagramSemanticParser, DiagramSnapshotCapture, DiagramWarningFact,
    FLOWCHART_EXPLICIT_DIRECTION_WARNING_RULE_ID, FLOWCHART_UNKNOWN_STYLE_TARGET_WARNING_RULE_ID,
    GIT_GRAPH_DUPLICATE_COMMIT_WARNING_RULE_ID, ParsedDiagram, ParsedDiagramRender,
    ParsedEditorFacts, RenderDiagramRegistry, RenderSemanticModel,
};
pub use editor::{
    EditorExpectedSyntax, EditorExpectedSyntaxKind, EditorFamilySemantics, EditorRenamePolicy,
    EditorSemanticCompleteness, EditorSemanticDiagnostic, EditorSemanticDiagnosticKind,
    EditorSemanticFacts, EditorSemanticKind, EditorSemanticRole, EditorSemanticSymbol, SourceSpan,
};
pub use error::{Error, ParseDiagnostic, ParseDiagnosticSpanKind, Result};
pub use family::{
    BuiltInTypedRenderFamily, DiagramFamilyCapability, DiagramFamilyId, DiagramFamilySelector,
    DiagramHeaderFact, diagram_type_family_id, diagram_type_family_kind, diagram_type_metadata_id,
    diagram_type_render_model_kind,
};
pub use operation::{
    CancelReason, OperationCancelled, OperationControl, OperationControlResult,
    OperationLedgerError, OperationPhase, OperationResourceDomain, OperationResourceLimitExceeded,
    OperationResourceOverride, OperationResourceProvenance,
};
pub use preprocess::{
    PreprocessResult, PreprocessedSource, preprocess_diagram, preprocess_diagram_with_known_type,
};

/// Maximum nested diagram/include depth accepted by recursive parsers.
pub const MAX_DIAGRAM_NESTING_DEPTH: usize = 256;

/// Returns Mermaid theme names supported by the pinned baseline.
pub fn supported_themes() -> &'static [&'static str] {
    theme::SUPPORTED_THEME_NAMES
}

/// Returns metadata names backed by semantic parsers compiled into this build.
///
/// Aliases share their catalog-owned metadata name. This does not promise an output adapter;
/// use the complete [`diagram_family_capabilities`] catalog to distinguish known identities
/// from available implementations.
pub fn supported_diagrams() -> &'static [&'static str] {
    family::supported_diagram_metadata_ids()
}

/// Returns every known Mermaid diagram id in the pinned baseline, including unavailable ones.
///
/// Identity, detector, header, and configuration facts are retained independently of the parser,
/// editor, and typed render-model callbacks compiled into this build.
pub fn diagram_family_capabilities() -> &'static [DiagramFamilyCapability] {
    family::diagram_family_capabilities()
}

/// Returns the complete catalog-owned mapping from logical families to Cargo selectors.
///
/// These declarations are independent of enabled features and exclude infrastructure models.
/// Selecting a family does not enable an output format or optional layout engine.
pub fn diagram_family_selectors() -> &'static [DiagramFamilySelector] {
    family::diagram_family_selectors()
}

/// Returns each compiled concrete built-in typed render-model family exactly once.
///
/// This is independent of output adapters and deduplicates aliases sharing a typed model.
pub fn built_in_typed_render_families() -> &'static [BuiltInTypedRenderFamily] {
    family::built_in_typed_render_families()
}

/// Returns known header facts for Mermaid diagram starters in the pinned baseline.
///
/// A header identifies a known family; actionable suggestions must also check that its semantic
/// parser is available in [`diagram_family_capabilities`].
pub fn diagram_header_facts() -> &'static [DiagramHeaderFact] {
    family::diagram_header_facts()
}

fn build_default_effective_config(
    site_config: &MermaidConfig,
) -> std::result::Result<MermaidConfig, theme_color::ColorError> {
    let mut effective_config = site_config.clone();
    theme::apply_theme_defaults(&mut effective_config)?;
    Ok(effective_config)
}

fn merge_site_config_override(target: &mut MermaidConfig, mut site_config: MermaidConfig) {
    config::mirror_legacy_font_family_into_theme_variables(&mut site_config);
    // initialize() normalizes an unknown global theme before it becomes a user layer.
    // Scoped themes instead fall through during diagram appearance resolution.
    if site_config.as_value().get("theme").is_some_and(|value| {
        !value.is_null()
            && !value
                .as_str()
                .is_some_and(|name| name == "null" || theme::SUPPORTED_THEME_NAMES.contains(&name))
    }) && let Some(theme) = generated::upstream_default_config().as_value().get("theme")
    {
        site_config.set_value("theme", theme.clone());
    }
    let explicit_secure_policy = site_config
        .as_value()
        .get("secure")
        .filter(|value| value.is_array())
        .map(config::clone_value_nonrecursive);
    target.deep_merge(site_config.as_value());

    // Merman adds host-level hardening beyond Mermaid's upstream defaults. An explicit site
    // policy is host authority, so it replaces that added list after the source-compatible array
    // merge instead of making the hardening impossible to opt out of.
    if let Some(secure) = explicit_secure_policy {
        target.set_value("secure", secure);
    }
}

fn generated_default_effective_config()
-> std::result::Result<MermaidConfig, theme_color::ColorError> {
    static DEFAULT_EFFECTIVE_CONFIG: std::sync::OnceLock<
        std::result::Result<MermaidConfig, theme_color::ColorError>,
    > = std::sync::OnceLock::new();
    DEFAULT_EFFECTIVE_CONFIG
        .get_or_init(|| build_default_effective_config(&generated::default_site_config()))
        .clone()
}

/// Parser behavior switches for model-producing parse facades.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct ParseOptions {
    /// Return an `error` diagram model from JSON/render facades when diagram parsing fails.
    pub suppress_errors: bool,
}

impl ParseOptions {
    /// Strict parsing (errors are returned).
    pub fn strict() -> Self {
        Self {
            suppress_errors: false,
        }
    }

    /// Lenient model parsing: return an `error` diagram from JSON/render facades on failure.
    pub fn lenient() -> Self {
        Self {
            suppress_errors: true,
        }
    }
}

/// Metadata extracted before semantic diagram parsing.
#[derive(Debug, Clone)]
pub struct ParseMetadata {
    /// Mermaid diagram type id selected by detection or supplied by a known-type parse entrypoint.
    pub diagram_type: String,
    /// Parsed config overrides extracted from front-matter and directives.
    /// This mirrors Mermaid's `mermaidAPI.parse()` return shape.
    pub config: MermaidConfig,
    /// The effective config used for detection/parsing after applying site defaults.
    pub effective_config: MermaidConfig,
    /// Sanitized Mermaid title from front-matter/directives, when present.
    pub title: Option<String>,
}

/// Headless Mermaid parser engine.
///
/// An engine owns detector/parser registries and a site-level Mermaid configuration. It is cheap
/// to clone when callers need per-request option variants.
#[derive(Debug, Clone)]
pub struct Engine {
    registry: DetectorRegistry,
    diagram_registry: DiagramRegistry,
    render_diagram_registry: RenderDiagramRegistry,
    site_config: MermaidConfig,
    site_config_delta: MermaidConfig,
    presentation_defaults: MermaidConfig,
    presentation_non_paint_defaults: MermaidConfig,
    default_effective_config: std::result::Result<MermaidConfig, theme_color::ColorError>,
    runtime_policy: runtime::RuntimePolicy,
}

impl Default for Engine {
    fn default() -> Self {
        let site_config = generated::default_site_config();
        let default_effective_config = generated_default_effective_config();

        Self {
            registry: DetectorRegistry::pinned_mermaid_baseline(),
            diagram_registry: DiagramRegistry::pinned_mermaid_baseline(),
            render_diagram_registry: RenderDiagramRegistry::pinned_mermaid_baseline(),
            site_config,
            site_config_delta: MermaidConfig::empty_object(),
            presentation_defaults: MermaidConfig::empty_object(),
            presentation_non_paint_defaults: MermaidConfig::empty_object(),
            default_effective_config,
            runtime_policy: runtime::RuntimePolicy::deterministic(),
        }
    }
}

impl Engine {
    /// Creates an engine using the pinned Mermaid baseline registries and default site config.
    pub fn new() -> Self {
        Self::default()
    }

    /// Creates an engine backed by all native system adapters.
    pub fn try_native() -> std::result::Result<Self, runtime::RuntimePolicyError> {
        Ok(Self::new().with_runtime_policy(runtime::RuntimePolicy::try_native()?))
    }

    pub(crate) fn default_effective_config(&self) -> Result<MermaidConfig> {
        self.default_effective_config.clone().map_err(Error::from)
    }

    /// Overrides the "today" value used by diagrams that depend on local time (e.g. Gantt).
    ///
    /// This exists primarily to make fixture snapshots deterministic. The default runtime policy
    /// uses the Unix epoch; native callers must explicitly select [`Engine::try_native`].
    pub fn with_fixed_today(mut self, today: Option<time::CivilDate>) -> Self {
        self.runtime_policy = self.runtime_policy.with_fixed_today(today);
        self
    }

    /// Selects a fixed local UTC offset for diagrams with local-time semantics.
    pub fn try_with_fixed_local_offset_minutes(
        mut self,
        offset_minutes: i32,
    ) -> std::result::Result<Self, runtime::RuntimePolicyError> {
        self.runtime_policy = self
            .runtime_policy
            .try_with_fixed_local_offset_minutes(offset_minutes)?;
        Ok(self)
    }

    /// Installs an already-resolved local timezone without consulting ambient process state.
    pub fn with_local_time_zone(mut self, time_zone: time::LocalTimeZone) -> Self {
        self.runtime_policy = self.runtime_policy.with_local_time_zone(time_zone);
        self
    }

    /// Replaces the complete runtime policy used to begin future operations.
    pub fn with_runtime_policy(mut self, policy: runtime::RuntimePolicy) -> Self {
        self.runtime_policy = policy;
        self
    }

    /// Replays a previously frozen runtime context without consulting system state.
    pub fn with_operation_context(self, context: runtime::OperationContext) -> Self {
        self.with_runtime_policy(runtime::RuntimePolicy::from_operation_context(context))
    }

    pub fn runtime_policy(&self) -> &runtime::RuntimePolicy {
        &self.runtime_policy
    }

    pub fn begin_operation(
        &self,
    ) -> std::result::Result<runtime::OperationContext, runtime::RuntimePolicyError> {
        self.runtime_policy.begin_operation()
    }

    /// Returns the fixed local timezone offset configured for this engine.
    pub fn fixed_local_offset_minutes(&self) -> Option<i32> {
        self.runtime_policy.fixed_local_offset_minutes()
    }

    pub fn local_time_zone(&self) -> &time::LocalTimeZone {
        self.runtime_policy.local_time_zone()
    }

    /// Applies site-level Mermaid config defaults.
    pub fn with_site_config(mut self, site_config: MermaidConfig) -> Self {
        if site_config.is_empty_object() {
            return self;
        }
        // Keep the user's layer separate: an explicit schema-default appearance still
        // outranks a diagram-specific default.
        merge_site_config_override(&mut self.site_config_delta, site_config.clone());
        if self.presentation_defaults.is_empty_object() {
            merge_site_config_override(&mut self.site_config, site_config);
            self.default_effective_config = build_default_effective_config(&self.site_config);
        } else {
            self.rebuild_presentation_site_config();
        }
        self
    }

    /// Installs renderer-owned defaults without treating them as authored initialize inputs.
    #[doc(hidden)]
    pub fn with_presentation_defaults_internal(
        mut self,
        mut defaults: MermaidConfig,
        non_paint_defaults: MermaidConfig,
        paint_defaults_active: bool,
    ) -> Self {
        defaults.set_presentation_defaults_active(paint_defaults_active);
        // Capture the materialized host default before authored site/source values merge.
        defaults.set_presentation_c4_line_default_active(
            defaults.get_str("c4.relationship_line_color").is_some(),
        );
        defaults.set_presentation_font_defaults_active(
            non_paint_defaults.get_str("fontFamily").is_some()
                || non_paint_defaults
                    .get_str("themeVariables.fontSize")
                    .is_some(),
        );
        self.presentation_defaults = defaults;
        self.presentation_non_paint_defaults = non_paint_defaults;
        self.rebuild_presentation_site_config();
        self
    }

    fn rebuild_presentation_site_config(&mut self) {
        self.site_config = generated::default_site_config();
        self.site_config
            .deep_merge(self.presentation_defaults.as_value());
        merge_site_config_override(&mut self.site_config, self.site_config_delta.clone());
        let mut initialized = self.site_config.clone();
        let external_palette = config::resolve_appearance(
            "",
            &MermaidConfig::default(),
            &self.site_config_delta,
            &self.presentation_defaults,
            &generated::upstream_default_config(),
            &mut initialized,
        );
        self.default_effective_config =
            if external_palette && !self.presentation_defaults.is_empty_object() {
                config::apply_external_presentation_palette(
                    &mut initialized,
                    &self.presentation_defaults,
                    &self.presentation_non_paint_defaults,
                    &self.site_config_delta,
                    &MermaidConfig::default(),
                    None,
                )
                .map(|()| initialized)
            } else {
                build_default_effective_config(&initialized)
            };
        if let Ok(config) = &mut self.default_effective_config {
            config.set_presentation_defaults_active(
                self.presentation_defaults.presentation_defaults_active(),
            );
            config.set_presentation_font_defaults_active(
                self.presentation_defaults
                    .presentation_font_defaults_active(),
            );
            config.set_presentation_c4_line_default_active(
                self.presentation_defaults
                    .presentation_c4_line_default_active(),
            );
            config.set_wardley_axis_text_authored(
                self.site_config_delta
                    .get_str("themeVariables.wardley.axisTextColor")
                    .is_some(),
            );
        }
    }

    /// Replaces the complete site-config environment while preserving custom registries.
    ///
    /// `None` restores the pinned Mermaid defaults. An explicit config is merged onto those
    /// defaults without inheriting values from the engine's previous site config.
    pub fn with_exact_site_config(mut self, site_config: Option<MermaidConfig>) -> Self {
        self.site_config = generated::default_site_config();
        self.site_config_delta = MermaidConfig::empty_object();
        self.presentation_defaults = MermaidConfig::empty_object();
        self.presentation_non_paint_defaults = MermaidConfig::empty_object();
        if let Some(site_config) = site_config {
            merge_site_config_override(&mut self.site_config_delta, site_config.clone());
            merge_site_config_override(&mut self.site_config, site_config);
        }
        self.default_effective_config = build_default_effective_config(&self.site_config);
        self
    }

    /// Returns the detector registry used for automatic diagram type detection.
    pub fn registry(&self) -> &DetectorRegistry {
        &self.registry
    }

    /// Returns a mutable detector registry for custom diagram detection.
    pub fn registry_mut(&mut self) -> &mut DetectorRegistry {
        &mut self.registry
    }

    /// Returns the semantic JSON parser registry.
    pub fn diagram_registry(&self) -> &DiagramRegistry {
        &self.diagram_registry
    }

    /// Returns a mutable semantic JSON parser registry for custom diagram adapters.
    pub fn diagram_registry_mut(&mut self) -> &mut DiagramRegistry {
        &mut self.diagram_registry
    }

    /// Returns the typed render-model parser registry.
    pub fn render_diagram_registry(&self) -> &RenderDiagramRegistry {
        &self.render_diagram_registry
    }

    /// Returns a mutable typed render-model parser registry.
    pub fn render_diagram_registry_mut(&mut self) -> &mut RenderDiagramRegistry {
        &mut self.render_diagram_registry
    }

    /// Synchronous variant of [`Engine::parse_metadata`].
    ///
    /// This is useful for UI render pipelines that are synchronous (e.g. immediate-mode UI),
    /// where introducing an async executor would be awkward. The parsing work is CPU-bound and
    /// does not perform I/O.
    pub fn parse_metadata_sync(&self, text: &str) -> Result<ParseMetadata> {
        parse_pipeline::ParsePipeline::detect(self, text, ParseOptions::strict()).metadata()
    }

    /// Parses metadata for an already-known diagram type (skips type detection).
    ///
    /// This is intended for integrations that already know the diagram type, e.g. Markdown fences
    /// like ````mermaid` / ` ```flowchart` / ` ```sequenceDiagram`.
    ///
    /// ## Example (Markdown fence)
    ///
    /// ```no_run
    /// use merman_core::Engine;
    ///
    /// let engine = Engine::new();
    ///
    /// // Your markdown parser provides the fence info string (e.g. "flowchart", "sequenceDiagram").
    /// let fence = "sequenceDiagram";
    /// let diagram = r#"sequenceDiagram
    ///   Alice->>Bob: Hello
    /// "#;
    ///
    /// // Map fence info strings to merman's internal diagram ids.
    /// let diagram_type = match fence {
    ///     "sequenceDiagram" => "sequence",
    ///     "flowchart" | "graph" => "flowchart-v2",
    ///     "stateDiagram" | "stateDiagram-v2" => "stateDiagram",
    ///     other => other,
    /// };
    ///
    /// let meta = engine
    ///     .parse_metadata_with_type_sync(diagram_type, diagram)?;
    /// # Ok::<(), merman_core::Error>(())
    /// ```
    pub fn parse_metadata_with_type_sync(
        &self,
        diagram_type: &str,
        text: &str,
    ) -> Result<ParseMetadata> {
        parse_pipeline::ParsePipeline::known_type(self, diagram_type, text, ParseOptions::strict())
            .metadata()
    }

    /// Parses editor-facing semantic facts when a family has a parser-backed implementation.
    ///
    /// Returned spans are byte offsets in the `text` supplied to this method.
    pub fn parse_editor_semantic_facts_with_type_sync(
        &self,
        diagram_type: &str,
        text: &str,
    ) -> Result<Option<EditorSemanticFacts>> {
        let Some(snapshot) = self.parse_diagram_snapshot_with_type_sync(diagram_type, text)? else {
            return Ok(None);
        };
        let (_, outcome, editor_facts) = snapshot.into_parts();
        match editor_facts {
            ParsedEditorFacts::Available(facts) => Ok(Some(facts)),
            ParsedEditorFacts::Unavailable => match outcome {
                DiagramParseOutcome::Failed(error @ Error::UnsupportedDiagram { .. })
                    if family::is_builtin_diagram_type(diagram_type) =>
                {
                    Err(error)
                }
                _ => Ok(None),
            },
        }
    }

    /// Async facade for [`Engine::parse_metadata_sync`].
    ///
    /// The work is CPU-bound and executes synchronously; this method exists for callers that
    /// prefer an async-shaped API.
    pub async fn parse_metadata(&self, text: &str) -> Result<ParseMetadata> {
        self.parse_metadata_sync(text)
    }

    /// Async facade for [`Engine::parse_metadata_with_type_sync`].
    ///
    /// The work is CPU-bound and executes synchronously.
    pub async fn parse_metadata_with_type(
        &self,
        diagram_type: &str,
        text: &str,
    ) -> Result<ParseMetadata> {
        self.parse_metadata_with_type_sync(diagram_type, text)
    }

    /// Synchronous variant of [`Engine::parse_diagram`].
    ///
    /// Note: callers that want “always returns a diagram” behavior can set
    /// [`ParseOptions::suppress_errors`] to `true` to get an `error` diagram on parse failures.
    pub fn parse_diagram_sync(
        &self,
        text: &str,
        options: ParseOptions,
    ) -> Result<Option<ParsedDiagram>> {
        parse_pipeline::ParsePipeline::detect(self, text, options)
            .parse_json(parse_pipeline::ParseTiming::Json)
    }

    /// Captures semantic JSON or its original error, parser-owned typed warning facts, and
    /// parser-backed editor facts in one operation.
    ///
    /// This is intended for editor integrations that need both diagnostics/facts and the
    /// Mermaid-compatible model. Once preprocessing and detection succeed, family parse errors and
    /// panics are retained inside the snapshot alongside metadata and recovery facts. Successful
    /// snapshots expose warnings through the `warning_facts` field of
    /// [`DiagramParseOutcome::Parsed`]; consumers should not decode the compatibility model's
    /// `warningFacts` field. Consumers must project a retained failure state directly rather than
    /// parsing the source again.
    /// Error suppression is deliberately absent from this API; suppression remains limited to
    /// model-producing JSON and render facades.
    pub fn parse_diagram_snapshot_sync(&self, text: &str) -> Result<Option<DiagramParseSnapshot>> {
        let control = OperationControl::new();
        self.parse_diagram_snapshot_controlled_sync(text, &control)
            .map_err(Error::from)?
    }

    /// Captures an editor-facing parse operation with cooperative cancellation.
    ///
    /// Cancellation is returned through the outer [`OperationControlResult`] and is never converted
    /// into a Mermaid parse error, failed snapshot, or recovery diagnostic.
    pub fn parse_diagram_snapshot_controlled_sync(
        &self,
        text: &str,
        control: &OperationControl,
    ) -> OperationControlResult<Result<Option<DiagramParseSnapshot>>> {
        parse_pipeline::ParsePipeline::detect(self, text, ParseOptions::strict())
            .parse_editor_snapshot_controlled(parse_pipeline::ParseTiming::Json, control)
    }

    /// Captures an editor parse operation while retaining preprocessing source-configuration
    /// evidence on non-cancellation failures and on panics after preprocessing completes.
    /// Cooperative cancellation remains the outer error channel and never yields a partial
    /// capture.
    pub fn capture_diagram_snapshot_controlled_sync(
        &self,
        text: &str,
        control: &OperationControl,
    ) -> OperationControlResult<DiagramSnapshotCapture> {
        parse_pipeline::ParsePipeline::detect(self, text, ParseOptions::strict())
            .capture_editor_snapshot_controlled(parse_pipeline::ParseTiming::Json, control)
    }

    /// Captures one editor-facing parse operation when the diagram type is already known.
    ///
    /// This has the same closed snapshot contract, including parser-owned typed warning facts, as
    /// [`Engine::parse_diagram_snapshot_sync`], but skips automatic detection. Family parse
    /// failures and panics remain inside the returned snapshot.
    pub fn parse_diagram_snapshot_with_type_sync(
        &self,
        diagram_type: &str,
        text: &str,
    ) -> Result<Option<DiagramParseSnapshot>> {
        parse_pipeline::ParsePipeline::known_type(self, diagram_type, text, ParseOptions::strict())
            .parse_editor_snapshot(parse_pipeline::ParseTiming::Json)
    }

    /// Async facade for [`Engine::parse_diagram_sync`].
    ///
    /// The work is CPU-bound and executes synchronously.
    pub async fn parse_diagram(
        &self,
        text: &str,
        options: ParseOptions,
    ) -> Result<Option<ParsedDiagram>> {
        self.parse_diagram_sync(text, options)
    }

    /// Parses a diagram into a typed semantic model optimized for headless layout + SVG rendering.
    ///
    /// Unlike [`Engine::parse_diagram_sync`], this avoids constructing large
    /// `serde_json::Value` object trees for high-impact typed-first diagrams and instead returns
    /// typed semantic structs that the renderer can consume directly.
    ///
    /// Callers that need the semantic JSON model should continue using
    /// [`Engine::parse_diagram_sync`].
    pub fn parse_diagram_for_render_model_sync(
        &self,
        text: &str,
        options: ParseOptions,
    ) -> Result<Option<ParsedDiagramRender>> {
        parse_pipeline::ParsePipeline::detect(self, text, options).parse_render_model()
    }

    /// Controlled variant of [`Self::parse_diagram_for_render_model_sync`].
    ///
    /// The caller owns the cloneable operation control and may cancel it from another thread or
    /// task. Cancellation is returned through the outer result and is never folded into a parse
    /// error or a partial render model.
    pub fn parse_diagram_for_render_model_controlled_sync(
        &self,
        text: &str,
        options: ParseOptions,
        operation: &OperationControl,
    ) -> OperationControlResult<Result<Option<ParsedDiagramRender>>> {
        parse_pipeline::ParsePipeline::detect(self, text, options)
            .parse_render_model_controlled(operation)
    }

    /// Parses a typed render model inside a caller-owned runtime context and operation.
    ///
    /// Higher-level render facades use this composition seam to keep runtime values and
    /// cancellation state identical across parsing and target-specific layout/output stages.
    /// Unlike [`Self::parse_diagram_for_render_model_controlled_sync`], this method does not
    /// begin a replacement runtime operation.
    #[doc(hidden)]
    pub fn parse_diagram_for_render_model_controlled_in_context_sync(
        &self,
        text: &str,
        options: ParseOptions,
        operation: &OperationControl,
        operation_context: &runtime::OperationContext,
    ) -> OperationControlResult<Result<Option<ParsedDiagramRender>>> {
        parse_pipeline::ParsePipeline::detect(self, text, options)
            .parse_render_model_controlled_in_context(operation, operation_context)
    }

    /// Async facade for [`Engine::parse_diagram_for_render_model_sync`].
    ///
    /// The work is CPU-bound and executes synchronously.
    pub async fn parse_diagram_for_render_model(
        &self,
        text: &str,
        options: ParseOptions,
    ) -> Result<Option<ParsedDiagramRender>> {
        self.parse_diagram_for_render_model_sync(text, options)
    }

    /// Parses a diagram into a typed semantic render model when the diagram type is already known
    /// (skips type detection).
    ///
    /// This is the preferred entrypoint for Markdown renderers and editors that already know the
    /// diagram type from the code fence info string. It avoids the detection pass and can reduce a
    /// small fixed overhead in tight render loops.
    pub fn parse_diagram_for_render_model_with_type_sync(
        &self,
        diagram_type: &str,
        text: &str,
        options: ParseOptions,
    ) -> Result<Option<ParsedDiagramRender>> {
        parse_pipeline::ParsePipeline::known_type(self, diagram_type, text, options)
            .parse_render_model()
    }

    /// Controlled known-type variant of [`Self::parse_diagram_for_render_model_with_type_sync`].
    pub fn parse_diagram_for_render_model_with_type_controlled_sync(
        &self,
        diagram_type: &str,
        text: &str,
        options: ParseOptions,
        operation: &OperationControl,
    ) -> OperationControlResult<Result<Option<ParsedDiagramRender>>> {
        parse_pipeline::ParsePipeline::known_type(self, diagram_type, text, options)
            .parse_render_model_controlled(operation)
    }

    /// Known-type composition seam for a caller-owned runtime context and operation.
    #[doc(hidden)]
    pub fn parse_diagram_for_render_model_with_type_controlled_in_context_sync(
        &self,
        diagram_type: &str,
        text: &str,
        options: ParseOptions,
        operation: &OperationControl,
        operation_context: &runtime::OperationContext,
    ) -> OperationControlResult<Result<Option<ParsedDiagramRender>>> {
        parse_pipeline::ParsePipeline::known_type(self, diagram_type, text, options)
            .parse_render_model_controlled_in_context(operation, operation_context)
    }

    /// Async facade for [`Engine::parse_diagram_for_render_model_with_type_sync`].
    ///
    /// The work is CPU-bound and executes synchronously.
    pub async fn parse_diagram_for_render_model_with_type(
        &self,
        diagram_type: &str,
        text: &str,
        options: ParseOptions,
    ) -> Result<Option<ParsedDiagramRender>> {
        self.parse_diagram_for_render_model_with_type_sync(diagram_type, text, options)
    }

    /// Parses a diagram when the diagram type is already known (skips type detection).
    ///
    /// This is the preferred entrypoint for Markdown renderers and editors that already know the
    /// diagram type from the code fence info string. It avoids the detection pass and can reduce a
    /// small fixed overhead in tight render loops.
    ///
    /// ## Example
    ///
    /// ```no_run
    /// use merman_core::{Engine, ParseOptions};
    ///
    /// let engine = Engine::new();
    /// let input = "flowchart TD; A-->B;";
    ///
    /// let parsed = engine
    ///     .parse_diagram_with_type_sync("flowchart-v2", input, ParseOptions::strict())?
    ///     .expect("diagram detected");
    ///
    /// assert_eq!(parsed.meta.diagram_type, "flowchart-v2");
    /// # Ok::<(), merman_core::Error>(())
    /// ```
    pub fn parse_diagram_with_type_sync(
        &self,
        diagram_type: &str,
        text: &str,
        options: ParseOptions,
    ) -> Result<Option<ParsedDiagram>> {
        parse_pipeline::ParsePipeline::known_type(self, diagram_type, text, options)
            .parse_json(parse_pipeline::ParseTiming::None)
    }

    /// Async facade for [`Engine::parse_diagram_with_type_sync`].
    ///
    /// The work is CPU-bound and executes synchronously.
    pub async fn parse_diagram_with_type(
        &self,
        diagram_type: &str,
        text: &str,
        options: ParseOptions,
    ) -> Result<Option<ParsedDiagram>> {
        self.parse_diagram_with_type_sync(diagram_type, text, options)
    }
}

#[cfg(test)]
mod tests;
