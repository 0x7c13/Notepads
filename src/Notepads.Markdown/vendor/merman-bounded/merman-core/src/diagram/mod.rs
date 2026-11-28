use crate::{
    EditorSemanticFacts, Error, ManagedSemanticJson, MermaidConfig, OperationControl,
    OperationControlResult, OperationPhase, ParseMetadata, Result, editor::SourceSpan,
    preprocess::SourceConfigEvidence,
};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::any::Any;
use std::collections::HashMap;
use std::fmt::{self, Debug, Formatter};
use std::sync::Arc;

pub const AGENTFLOW_SHAPE_REMOVED_WARNING_RULE_ID: &str = "merman.semantic.agentflow.shape_removed";
pub const AGENTFLOW_SHAPE_UNSUPPORTED_WARNING_RULE_ID: &str =
    "merman.semantic.agentflow.shape_unsupported";
pub const AGENTFLOW_CONTAINMENT_VIOLATION_WARNING_RULE_ID: &str =
    "merman.semantic.agentflow.containment_violation";

pub const BLOCK_WIDTH_WARNING_RULE_ID: &str = "merman.block.width_exceeds_columns";
pub const FLOWCHART_EXPLICIT_DIRECTION_WARNING_RULE_ID: &str =
    "merman.authoring.flowchart.explicit_direction";
pub const FLOWCHART_UNKNOWN_STYLE_TARGET_WARNING_RULE_ID: &str =
    "merman.semantic.flowchart.unknown_style_target";
pub const GIT_GRAPH_DUPLICATE_COMMIT_WARNING_RULE_ID: &str = "merman.git_graph.duplicate_commit_id";

/// Shared warning fact emitted by diagram families for analysis and lint consumers.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DiagramWarningFact {
    pub rule_id: String,
    pub message: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub span: Option<SourceSpan>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub fix_span: Option<SourceSpan>,
}

impl DiagramWarningFact {
    pub fn new(rule_id: impl Into<String>, message: impl Into<String>) -> Self {
        Self {
            rule_id: rule_id.into(),
            message: message.into(),
            span: None,
            fix_span: None,
        }
    }

    pub fn with_span(mut self, span: SourceSpan) -> Self {
        self.span = Some(span);
        self
    }

    pub fn with_fix_span(mut self, span: SourceSpan) -> Self {
        self.fix_span = Some(span);
        self
    }
}

#[cfg(any(
    feature = "diagram-block",
    feature = "diagram-flowchart",
    feature = "diagram-swimlane",
    feature = "diagram-git-graph"
))]
pub(crate) fn legacy_warning_messages(facts: &[DiagramWarningFact]) -> Vec<String> {
    facts.iter().map(|fact| fact.message.clone()).collect()
}

/// Parser used by a custom semantic JSON registry overlay.
///
/// Implementations must call [`OperationControl::checkpoint`] inside potentially long-running loops.
/// The outer [`OperationControlResult`] reports cooperative cancellation; the inner [`Result`]
/// reports Mermaid detection or parse failures. Non-cancellable `Engine` snapshot and model APIs
/// surface an unexpected outer cancellation as [`crate::Error::OperationCancelled`].
pub type DiagramSemanticParser = fn(
    code: &str,
    meta: &ParseMetadata,
    control: &OperationControl,
) -> OperationControlResult<Result<Value>>;

/// Parser used by the pinned built-in semantic JSON path.
pub(crate) type BuiltInDiagramSemanticParser =
    fn(code: &str, meta: &ParseMetadata) -> Result<Value>;

/// Parser used by the built-in typed render-model path for one Mermaid diagram family.
pub(crate) type BuiltInRenderSemanticParser =
    fn(
        code: &str,
        meta: &ParseMetadata,
        control: &OperationControl,
    ) -> OperationControlResult<Result<RenderSemanticParseOutput>>;

/// Parser used by a custom render-model registry overlay.
///
/// Custom adapters intentionally return only a named JSON model. Built-in typed variants are
/// reserved for the pinned family catalog and cannot be manufactured through this interface.
pub type CustomJsonRenderParser = fn(
    code: &str,
    meta: &ParseMetadata,
    control: &OperationControl,
) -> OperationControlResult<Result<CustomJsonRenderModel>>;

/// Ownership of a parser resolved from a built-in registry plus its custom overlay.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum RegistryOwner {
    BuiltIn,
    Custom,
}

#[derive(Debug, Clone, Copy)]
pub(crate) enum ResolvedSemanticParser {
    BuiltIn(BuiltInDiagramSemanticParser),
    Custom(DiagramSemanticParser),
}

impl ResolvedSemanticParser {
    pub(crate) const fn owner(self) -> RegistryOwner {
        match self {
            Self::BuiltIn(_) => RegistryOwner::BuiltIn,
            Self::Custom(_) => RegistryOwner::Custom,
        }
    }
}

/// Registry for semantic JSON parsers keyed by Mermaid diagram type id.
#[derive(Debug, Clone)]
pub struct DiagramRegistry {
    builtins: Arc<HashMap<&'static str, BuiltInDiagramSemanticParser>>,
    overlays: Arc<HashMap<&'static str, DiagramSemanticParser>>,
}

impl Default for DiagramRegistry {
    fn default() -> Self {
        Self::new()
    }
}

impl DiagramRegistry {
    /// Creates an empty registry.
    pub fn new() -> Self {
        Self {
            builtins: Arc::new(HashMap::new()),
            overlays: Arc::new(HashMap::new()),
        }
    }

    /// Registers or replaces the parser for a Mermaid diagram type id.
    ///
    /// The parser participates in the owning operation's cancellation lifecycle. It must preserve
    /// the outer-cancellation/inner-parse-error distinction documented by
    /// [`DiagramSemanticParser`].
    pub fn insert(&mut self, diagram_type: &'static str, parser: DiagramSemanticParser) {
        Arc::make_mut(&mut self.overlays).insert(diagram_type, parser);
    }

    pub(crate) fn resolve(&self, diagram_type: &str) -> Option<ResolvedSemanticParser> {
        if let Some(parser) = self.overlays.get(diagram_type).copied() {
            return Some(ResolvedSemanticParser::Custom(parser));
        }
        self.builtins
            .get(diagram_type)
            .copied()
            .map(ResolvedSemanticParser::BuiltIn)
    }

    /// Builds the semantic parser registry for the repository's pinned Mermaid baseline.
    pub fn pinned_mermaid_baseline() -> Self {
        let mut reg = Self::new();
        for fact in crate::family::semantic_parser_facts() {
            Arc::make_mut(&mut reg.builtins).insert(fact.id, fact.parser);
        }

        reg
    }

    #[cfg(all(test, feature = "all-diagrams"))]
    pub(crate) fn parser_ids(&self) -> impl Iterator<Item = &'static str> + '_ {
        self.builtins
            .keys()
            .chain(
                self.overlays
                    .keys()
                    .filter(|id| !self.builtins.contains_key(**id)),
            )
            .copied()
    }
}

/// Parsed diagram metadata plus the Mermaid-compatible semantic JSON model.
#[derive(Debug, Clone)]
pub struct ParsedDiagram {
    /// Diagram type and effective configuration extracted during preprocessing.
    pub meta: ParseMetadata,
    /// Semantic JSON model matching Mermaid's parser/database output shape where possible.
    pub model: ManagedSemanticJson,
}

/// Parser-backed editor facts produced by a diagram parse operation.
#[derive(Debug)]
pub enum ParsedEditorFacts {
    Available(EditorSemanticFacts),
    Unavailable,
}

/// Semantic result retained by one editor-facing diagram parse operation.
#[derive(Debug)]
pub enum DiagramParseOutcome {
    Parsed {
        /// Mermaid-compatible semantic JSON retained for existing projections.
        model: ManagedSemanticJson,
        /// Parser-owned warning facts after preprocessing spans have been remapped once.
        warning_facts: Vec<DiagramWarningFact>,
    },
    Failed(Error),
    /// Family construction panicked after preprocessing produced valid metadata.
    Panicked(String),
}

impl DiagramParseOutcome {
    /// Returns the semantic model when family construction succeeded.
    pub fn parsed_model(&self) -> Option<&ManagedSemanticJson> {
        match self {
            Self::Parsed { model, .. } => Some(model),
            Self::Failed(_) | Self::Panicked(_) => None,
        }
    }
}

/// One preprocessing, detection, and family-construction operation for editor consumers.
///
/// Metadata, semantic output or its original error, typed warnings, and recovery facts are
/// retained together so downstream analysis cannot reconstruct parser state from compatibility
/// JSON or by parsing the source again.
#[derive(Debug)]
pub struct DiagramParseSnapshot {
    meta: ParseMetadata,
    outcome: DiagramParseOutcome,
    editor_facts: ParsedEditorFacts,
    source_config: SourceConfigEvidence,
    recovered_incomplete_directive: bool,
}

impl DiagramParseSnapshot {
    pub(crate) fn new(
        meta: ParseMetadata,
        outcome: DiagramParseOutcome,
        editor_facts: ParsedEditorFacts,
        source_config: SourceConfigEvidence,
        recovered_incomplete_directive: bool,
    ) -> Self {
        Self {
            meta,
            outcome,
            editor_facts,
            source_config,
            recovered_incomplete_directive,
        }
    }

    /// Consumes the closed snapshot into its three operation-owned projections.
    pub fn into_parts(self) -> (ParseMetadata, DiagramParseOutcome, ParsedEditorFacts) {
        (self.meta, self.outcome, self.editor_facts)
    }

    /// Consumes the snapshot while retaining source-configuration evidence from preprocessing.
    pub fn into_parts_with_source_config(
        self,
    ) -> (
        ParseMetadata,
        DiagramParseOutcome,
        ParsedEditorFacts,
        SourceConfigEvidence,
    ) {
        (
            self.meta,
            self.outcome,
            self.editor_facts,
            self.source_config,
        )
    }

    /// Returns metadata produced by this preprocessing and detection operation.
    pub fn metadata(&self) -> &ParseMetadata {
        &self.meta
    }

    /// Returns the semantic success or original family-construction error.
    pub fn outcome(&self) -> &DiagramParseOutcome {
        &self.outcome
    }

    /// Returns parser-backed editor facts retained by this operation.
    pub fn editor_facts(&self) -> &ParsedEditorFacts {
        &self.editor_facts
    }

    /// Source-backed frontmatter and directive evidence from the outer preprocessing pass.
    pub fn source_config(&self) -> &SourceConfigEvidence {
        &self.source_config
    }

    /// Whether editor preprocessing recovered an unterminated directive line.
    pub const fn recovered_incomplete_directive(&self) -> bool {
        self.recovered_incomplete_directive
    }
}

/// Non-cancellation result of one editor snapshot capture operation.
// Keep the successful snapshot inline: the pre-existing parse API already returned this payload
// by value, while boxing it would add an allocation to every editor capture only to balance the
// less common failure variants.
pub struct CapturedPanic {
    message: String,
    payload: Box<dyn Any + Send + 'static>,
}

impl CapturedPanic {
    pub(crate) fn from_payload(payload: Box<dyn Any + Send + 'static>) -> Self {
        let message = payload
            .downcast_ref::<&str>()
            .copied()
            .or_else(|| payload.downcast_ref::<String>().map(String::as_str))
            .unwrap_or("panic while analyzing Mermaid source")
            .to_string();
        Self { message, payload }
    }

    pub fn message(&self) -> &str {
        &self.message
    }

    pub(crate) fn into_payload(self) -> Box<dyn Any + Send + 'static> {
        self.payload
    }
}

impl Debug for CapturedPanic {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("CapturedPanic")
            .field("message", &self.message)
            .finish_non_exhaustive()
    }
}

#[derive(Debug)]
#[allow(clippy::large_enum_variant)]
pub enum DiagramSnapshotCapture {
    Snapshot(Option<DiagramParseSnapshot>),
    Failed {
        error: Error,
        source_config: SourceConfigEvidence,
    },
    /// Detection or metadata finalization panicked after preprocessing evidence was captured.
    ///
    /// No diagram metadata is attached because automatic detection did not complete.
    Panicked {
        panic: CapturedPanic,
        source_config: SourceConfigEvidence,
    },
}

impl DiagramSnapshotCapture {
    pub(crate) fn into_result(self) -> Result<Option<DiagramParseSnapshot>> {
        match self {
            Self::Snapshot(snapshot) => Ok(snapshot),
            Self::Failed { error, .. } => Err(error),
            // The legacy Result facade has no panic outcome. Resume the exact original payload;
            // evidence-aware callers consume this capture before reaching this adapter.
            Self::Panicked { panic, .. } => std::panic::resume_unwind(panic.into_payload()),
        }
    }
}

/// Origin of a custom JSON render model.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CustomJsonProvenance {
    /// Produced by a custom semantic JSON parser because no custom render parser was registered.
    SemanticRegistryOverlay,
    /// Produced by an explicitly registered custom render-model parser.
    RenderRegistryOverlay,
}

/// Explicit non-built-in JSON model boundary for custom parser adapters.
#[derive(Debug, Clone, PartialEq)]
pub struct CustomJsonRenderModel {
    model_name: String,
    value: ManagedSemanticJson,
    provenance: CustomJsonProvenance,
}

impl CustomJsonRenderModel {
    /// Creates the result of a custom render-model registry parser.
    pub fn new(model_name: impl Into<String>, value: impl Into<ManagedSemanticJson>) -> Self {
        Self {
            model_name: model_name.into(),
            value: value.into(),
            provenance: CustomJsonProvenance::RenderRegistryOverlay,
        }
    }

    pub(crate) fn from_semantic_registry(
        model_name: impl Into<String>,
        value: impl Into<ManagedSemanticJson>,
    ) -> Self {
        Self {
            model_name: model_name.into(),
            value: value.into(),
            provenance: CustomJsonProvenance::SemanticRegistryOverlay,
        }
    }

    /// Returns the adapter-defined model name.
    pub fn model_name(&self) -> &str {
        &self.model_name
    }

    /// Returns the custom JSON payload.
    pub fn value(&self) -> &Value {
        &self.value
    }

    /// Borrows the managed payload for iterative cloning and export.
    pub fn json(&self) -> &ManagedSemanticJson {
        &self.value
    }

    /// Consumes the adapter while retaining managed ownership.
    pub fn into_json(self) -> ManagedSemanticJson {
        self.value
    }

    /// Extracts raw JSON; subsequent deep lifecycle operations are the caller's responsibility.
    pub fn into_unmanaged_value(self) -> Value {
        self.value.into_unmanaged_value()
    }

    /// Returns which custom registry path produced the model.
    pub fn provenance(&self) -> CustomJsonProvenance {
        self.provenance
    }
}

/// Typed semantic model used by the headless renderer.
///
/// Most public callers should use [`ParsedDiagram`] when they need JSON output. This enum is for
/// render paths that benefit from typed data and avoiding a JSON round trip.
///
/// Family variants and their payload modules are compiled only when the owning `diagram-*`
/// feature is enabled. `Flowchart` is shared by `diagram-flowchart` and `diagram-swimlane`;
/// sharing that payload does not enable the other language. `Error` and `CustomJson` remain
/// available in infrastructure-only builds.
#[derive(Debug, Clone)]
#[allow(
    clippy::large_enum_variant,
    reason = "Family selection changes variant size ratios; keep the public typed payloads unboxed."
)]
pub enum RenderSemanticModel {
    Error(crate::diagrams::error_diagram::ErrorDiagramRenderModel),
    CustomJson(CustomJsonRenderModel),
    #[cfg(feature = "diagram-mindmap")]
    Mindmap(crate::diagrams::mindmap::MindmapDiagramRenderModel),
    #[cfg(feature = "diagram-state")]
    State(crate::diagrams::state::StateDiagramRenderModel),
    #[cfg(feature = "diagram-sequence")]
    Sequence(crate::diagrams::sequence::SequenceDiagramRenderModel),
    #[cfg(feature = "diagram-zenuml")]
    Zenuml(crate::diagrams::zenuml::ZenumlDiagramRenderModel),
    #[cfg(any(feature = "diagram-flowchart", feature = "diagram-swimlane"))]
    Flowchart(crate::diagrams::flowchart::FlowchartModel),
    #[cfg(feature = "diagram-architecture")]
    Architecture(crate::diagrams::architecture::ArchitectureDiagramRenderModel),
    #[cfg(feature = "diagram-class")]
    Class(crate::models::class_diagram::ClassDiagram),
    #[cfg(feature = "diagram-c4")]
    C4(crate::diagrams::c4::C4DiagramRenderModel),
    #[cfg(feature = "diagram-cynefin")]
    Cynefin(crate::diagrams::cynefin::CynefinDiagramRenderModel),
    #[cfg(feature = "diagram-railroad")]
    Railroad(crate::diagrams::railroad::RailroadDiagramRenderModel),
    #[cfg(feature = "diagram-kanban")]
    Kanban(crate::diagrams::kanban::KanbanDiagramRenderModel),
    #[cfg(feature = "diagram-gantt")]
    Gantt(crate::diagrams::gantt::GanttDiagramRenderModel),
    #[cfg(feature = "diagram-pie")]
    Pie(crate::diagrams::pie::PieDiagramRenderModel),
    #[cfg(feature = "diagram-packet")]
    Packet(crate::diagrams::packet::PacketDiagramRenderModel),
    #[cfg(feature = "diagram-timeline")]
    Timeline(crate::diagrams::timeline::TimelineDiagramRenderModel),
    #[cfg(feature = "diagram-journey")]
    Journey(crate::diagrams::journey::JourneyDiagramRenderModel),
    #[cfg(feature = "diagram-requirement")]
    Requirement(crate::diagrams::requirement::RequirementDiagramRenderModel),
    #[cfg(feature = "diagram-sankey")]
    Sankey(crate::diagrams::sankey::SankeyDiagramRenderModel),
    #[cfg(feature = "diagram-radar")]
    Radar(crate::diagrams::radar::RadarDiagramRenderModel),
    #[cfg(feature = "diagram-info")]
    Info(crate::diagrams::info::InfoDiagramRenderModel),
    #[cfg(feature = "diagram-treemap")]
    Treemap(crate::diagrams::treemap::TreemapDiagramRenderModel),
    #[cfg(feature = "diagram-block")]
    Block(crate::diagrams::block::BlockDiagramRenderModel),
    #[cfg(feature = "diagram-er")]
    Er(crate::diagrams::er::ErDiagramRenderModel),
    #[cfg(feature = "diagram-quadrant-chart")]
    QuadrantChart(crate::diagrams::quadrant_chart::QuadrantChartRenderModel),
    #[cfg(feature = "diagram-xychart")]
    XyChart(crate::diagrams::xychart::XyChartDiagramRenderModel),
    #[cfg(feature = "diagram-git-graph")]
    GitGraph(crate::diagrams::git_graph::GitGraphRenderModel),
    #[cfg(feature = "diagram-tree-view")]
    TreeView(crate::diagrams::tree_view::TreeViewDiagramRenderModel),
    #[cfg(feature = "diagram-ishikawa")]
    Ishikawa(crate::diagrams::ishikawa::IshikawaDiagramRenderModel),
    #[cfg(feature = "diagram-event-modeling")]
    EventModeling(crate::diagrams::eventmodeling::EventModelingDiagramRenderModel),
    #[cfg(feature = "diagram-venn")]
    Venn(crate::diagrams::venn::VennDiagramRenderModel),
    #[cfg(feature = "diagram-wardley")]
    Wardley(crate::diagrams::wardley::WardleyDiagramRenderModel),
    #[cfg(feature = "diagram-usecase")]
    Usecase(crate::diagrams::usecase::UsecaseDiagramRenderModel),
    #[cfg(feature = "diagram-agentflow")]
    Agentflow(crate::diagrams::agentflow::AgentflowDiagramRenderModel),
}

/// Parser-owned data needed only while rendering a typed semantic model.
///
/// This context is intentionally separate from [`RenderSemanticModel`] so family models retain
/// their public construction and serialization contracts.
#[doc(hidden)]
#[derive(Debug, Clone, Default)]
pub struct RenderSemanticContext {
    #[cfg(any(feature = "diagram-flowchart", feature = "diagram-swimlane"))]
    flowchart: Option<crate::diagrams::flowchart::FlowchartRenderContext>,
}

impl RenderSemanticContext {
    #[cfg(any(feature = "diagram-flowchart", feature = "diagram-swimlane"))]
    fn for_flowchart(context: crate::diagrams::flowchart::FlowchartRenderContext) -> Self {
        Self {
            flowchart: Some(context),
        }
    }

    /// Consumes the context and returns Flowchart's parser-owned render facts.
    #[doc(hidden)]
    #[cfg(any(feature = "diagram-flowchart", feature = "diagram-swimlane"))]
    pub fn into_flowchart_render_context(
        self,
    ) -> crate::diagrams::flowchart::FlowchartRenderContext {
        self.flowchart.unwrap_or_default()
    }

    /// Borrows Flowchart's complete parser-owned render context.
    #[doc(hidden)]
    #[cfg(any(feature = "diagram-flowchart", feature = "diagram-swimlane"))]
    pub fn flowchart_render_context(
        &self,
    ) -> Option<&crate::diagrams::flowchart::FlowchartRenderContext> {
        self.flowchart.as_ref()
    }

    pub(crate) fn retained_text_bytes(&self) -> usize {
        #[cfg(any(feature = "diagram-flowchart", feature = "diagram-swimlane"))]
        {
            self.flowchart
                .as_ref()
                .map_or(0, |context| context.retained_bytes())
        }
        #[cfg(not(any(feature = "diagram-flowchart", feature = "diagram-swimlane")))]
        {
            0
        }
    }
}

#[derive(Debug, Clone)]
pub(crate) struct RenderSemanticParseOutput {
    model: RenderSemanticModel,
    context: RenderSemanticContext,
}

impl RenderSemanticParseOutput {
    pub(crate) fn new(model: RenderSemanticModel) -> Self {
        Self {
            model,
            context: RenderSemanticContext::default(),
        }
    }

    #[cfg(any(feature = "diagram-flowchart", feature = "diagram-swimlane"))]
    pub(crate) fn flowchart(
        model: crate::diagrams::flowchart::FlowchartModel,
        context: crate::diagrams::flowchart::FlowchartRenderContext,
    ) -> Self {
        Self {
            model: RenderSemanticModel::Flowchart(model),
            context: RenderSemanticContext::for_flowchart(context),
        }
    }

    pub(crate) fn model(&self) -> &RenderSemanticModel {
        &self.model
    }

    pub(crate) fn model_mut(&mut self) -> &mut RenderSemanticModel {
        &mut self.model
    }

    fn into_parts(self) -> (RenderSemanticModel, RenderSemanticContext) {
        (self.model, self.context)
    }
}

mod builtin_render_semantic_private {
    pub trait Sealed {}
}

/// Family-owned typed semantic data that can project the public compatibility JSON contract.
///
/// This trait is sealed because built-in family membership is defined by the pinned Mermaid
/// catalog. Consumers can use it generically but cannot manufacture a new built-in family.
pub trait BuiltinRenderSemantic: builtin_render_semantic_private::Sealed {
    fn compatibility_json(&self, meta: &ParseMetadata) -> Result<ManagedSemanticJson>;

    fn compatibility_json_controlled(
        &self,
        meta: &ParseMetadata,
        control: &OperationControl,
    ) -> OperationControlResult<Result<ManagedSemanticJson>> {
        control.checkpoint()?;
        let projected = self.compatibility_json(meta);
        control.checkpoint()?;
        Ok(projected)
    }
}

macro_rules! impl_builtin_render_semantic {
    ($model:path, $project:path) => {
        impl builtin_render_semantic_private::Sealed for $model {}

        impl BuiltinRenderSemantic for $model {
            fn compatibility_json(&self, meta: &ParseMetadata) -> Result<ManagedSemanticJson> {
                $project(self, meta).map(ManagedSemanticJson::from)
            }
        }
    };
}

#[cfg(any(
    feature = "diagram-mindmap",
    feature = "diagram-architecture",
    feature = "diagram-gantt",
    feature = "diagram-block",
    feature = "diagram-state",
    feature = "diagram-treemap",
    feature = "diagram-ishikawa",
    feature = "diagram-zenuml",
    feature = "diagram-wardley"
))]
macro_rules! impl_builtin_render_semantic_controlled {
    ($model:path, $project:path, $controlled_project:path) => {
        impl builtin_render_semantic_private::Sealed for $model {}

        impl BuiltinRenderSemantic for $model {
            fn compatibility_json(&self, meta: &ParseMetadata) -> Result<ManagedSemanticJson> {
                $project(self, meta).map(ManagedSemanticJson::from)
            }

            fn compatibility_json_controlled(
                &self,
                meta: &ParseMetadata,
                control: &OperationControl,
            ) -> OperationControlResult<Result<ManagedSemanticJson>> {
                $controlled_project(self, meta, control)
                    .map(|result| result.map(ManagedSemanticJson::from))
            }
        }
    };
}

impl_builtin_render_semantic!(
    crate::diagrams::error_diagram::ErrorDiagramRenderModel,
    crate::diagrams::error_diagram::render_model_to_compat_json
);
#[cfg(feature = "diagram-mindmap")]
impl_builtin_render_semantic_controlled!(
    crate::diagrams::mindmap::MindmapDiagramRenderModel,
    crate::diagrams::mindmap::render_model_to_compat_json,
    crate::diagrams::mindmap::render_model_to_compat_json_controlled
);
#[cfg(feature = "diagram-state")]
impl_builtin_render_semantic_controlled!(
    crate::diagrams::state::StateDiagramRenderModel,
    crate::diagrams::state::render_model_to_compat_json,
    crate::diagrams::state::render_model_to_compat_json_controlled
);
#[cfg(feature = "diagram-sequence")]
impl_builtin_render_semantic!(
    crate::diagrams::sequence::SequenceDiagramRenderModel,
    crate::diagrams::sequence::render_model_to_compat_json
);
#[cfg(feature = "diagram-zenuml")]
impl_builtin_render_semantic_controlled!(
    crate::diagrams::zenuml::ZenumlDiagramRenderModel,
    crate::diagrams::zenuml::render_model_to_compat_json,
    crate::diagrams::zenuml::render_model_to_compat_json_controlled
);
#[cfg(any(feature = "diagram-flowchart", feature = "diagram-swimlane"))]
impl_builtin_render_semantic!(
    crate::diagrams::flowchart::FlowchartModel,
    crate::diagrams::flowchart::render_model_to_compat_json
);
#[cfg(feature = "diagram-architecture")]
impl_builtin_render_semantic_controlled!(
    crate::diagrams::architecture::ArchitectureDiagramRenderModel,
    crate::diagrams::architecture::render_model_to_compat_json,
    crate::diagrams::architecture::render_model_to_compat_json_controlled
);
#[cfg(feature = "diagram-class")]
impl_builtin_render_semantic!(
    crate::models::class_diagram::ClassDiagram,
    crate::diagrams::class::render_model_to_compat_json
);
#[cfg(feature = "diagram-c4")]
impl_builtin_render_semantic!(
    crate::diagrams::c4::C4DiagramRenderModel,
    crate::diagrams::c4::render_model_to_compat_json
);
#[cfg(feature = "diagram-cynefin")]
impl_builtin_render_semantic!(
    crate::diagrams::cynefin::CynefinDiagramRenderModel,
    crate::diagrams::cynefin::render_model_to_compat_json
);
#[cfg(feature = "diagram-railroad")]
impl_builtin_render_semantic!(
    crate::diagrams::railroad::RailroadDiagramRenderModel,
    crate::diagrams::railroad::render_model_to_compat_json
);
#[cfg(feature = "diagram-kanban")]
impl_builtin_render_semantic!(
    crate::diagrams::kanban::KanbanDiagramRenderModel,
    crate::diagrams::kanban::render_model_to_compat_json
);
#[cfg(feature = "diagram-gantt")]
impl_builtin_render_semantic_controlled!(
    crate::diagrams::gantt::GanttDiagramRenderModel,
    crate::diagrams::gantt::render_model_to_compat_json,
    crate::diagrams::gantt::render_model_to_compat_json_controlled
);
#[cfg(feature = "diagram-pie")]
impl_builtin_render_semantic!(
    crate::diagrams::pie::PieDiagramRenderModel,
    crate::diagrams::pie::render_model_to_compat_json
);
#[cfg(feature = "diagram-packet")]
impl_builtin_render_semantic!(
    crate::diagrams::packet::PacketDiagramRenderModel,
    crate::diagrams::packet::render_model_to_compat_json
);
#[cfg(feature = "diagram-timeline")]
impl_builtin_render_semantic!(
    crate::diagrams::timeline::TimelineDiagramRenderModel,
    crate::diagrams::timeline::render_model_to_compat_json
);
#[cfg(feature = "diagram-journey")]
impl_builtin_render_semantic!(
    crate::diagrams::journey::JourneyDiagramRenderModel,
    crate::diagrams::journey::render_model_to_compat_json
);
#[cfg(feature = "diagram-requirement")]
impl_builtin_render_semantic!(
    crate::diagrams::requirement::RequirementDiagramRenderModel,
    crate::diagrams::requirement::render_model_to_compat_json
);
#[cfg(feature = "diagram-sankey")]
impl_builtin_render_semantic!(
    crate::diagrams::sankey::SankeyDiagramRenderModel,
    crate::diagrams::sankey::render_model_to_compat_json
);
#[cfg(feature = "diagram-radar")]
impl_builtin_render_semantic!(
    crate::diagrams::radar::RadarDiagramRenderModel,
    crate::diagrams::radar::render_model_to_compat_json
);
#[cfg(feature = "diagram-info")]
impl_builtin_render_semantic!(
    crate::diagrams::info::InfoDiagramRenderModel,
    crate::diagrams::info::render_model_to_compat_json
);
#[cfg(feature = "diagram-treemap")]
impl_builtin_render_semantic_controlled!(
    crate::diagrams::treemap::TreemapDiagramRenderModel,
    crate::diagrams::treemap::render_model_to_compat_json,
    crate::diagrams::treemap::render_model_to_compat_json_controlled
);
#[cfg(feature = "diagram-block")]
impl_builtin_render_semantic_controlled!(
    crate::diagrams::block::BlockDiagramRenderModel,
    crate::diagrams::block::render_model_to_compat_json,
    crate::diagrams::block::render_model_to_compat_json_controlled
);
#[cfg(feature = "diagram-er")]
impl_builtin_render_semantic!(
    crate::diagrams::er::ErDiagramRenderModel,
    crate::diagrams::er::render_model_to_compat_json
);
#[cfg(feature = "diagram-quadrant-chart")]
impl_builtin_render_semantic!(
    crate::diagrams::quadrant_chart::QuadrantChartRenderModel,
    crate::diagrams::quadrant_chart::render_model_to_compat_json
);
#[cfg(feature = "diagram-xychart")]
impl_builtin_render_semantic!(
    crate::diagrams::xychart::XyChartDiagramRenderModel,
    crate::diagrams::xychart::render_model_to_compat_json
);
#[cfg(feature = "diagram-git-graph")]
impl_builtin_render_semantic!(
    crate::diagrams::git_graph::GitGraphRenderModel,
    crate::diagrams::git_graph::render_model_to_compat_json
);
#[cfg(feature = "diagram-tree-view")]
impl_builtin_render_semantic!(
    crate::diagrams::tree_view::TreeViewDiagramRenderModel,
    crate::diagrams::tree_view::render_model_to_compat_json
);
#[cfg(feature = "diagram-ishikawa")]
impl_builtin_render_semantic_controlled!(
    crate::diagrams::ishikawa::IshikawaDiagramRenderModel,
    crate::diagrams::ishikawa::render_model_to_compat_json,
    crate::diagrams::ishikawa::render_model_to_compat_json_controlled
);
#[cfg(feature = "diagram-event-modeling")]
impl_builtin_render_semantic!(
    crate::diagrams::eventmodeling::EventModelingDiagramRenderModel,
    crate::diagrams::eventmodeling::render_model_to_compat_json
);
#[cfg(feature = "diagram-venn")]
impl_builtin_render_semantic!(
    crate::diagrams::venn::VennDiagramRenderModel,
    crate::diagrams::venn::render_model_to_compat_json
);
#[cfg(feature = "diagram-wardley")]
impl_builtin_render_semantic_controlled!(
    crate::diagrams::wardley::WardleyDiagramRenderModel,
    crate::diagrams::wardley::render_model_to_compat_json,
    crate::diagrams::wardley::render_model_to_compat_json_controlled
);
#[cfg(feature = "diagram-usecase")]
impl_builtin_render_semantic!(
    crate::diagrams::usecase::UsecaseDiagramRenderModel,
    crate::diagrams::usecase::render_model_to_compat_json
);
#[cfg(feature = "diagram-agentflow")]
impl_builtin_render_semantic!(
    crate::diagrams::agentflow::AgentflowDiagramRenderModel,
    crate::diagrams::agentflow::render_model_to_compat_json
);

impl RenderSemanticModel {
    /// Applies Mermaid common DB sanitization to family-owned typed fields.
    pub(crate) fn sanitize_common_db_fields(&mut self, config: &MermaidConfig) {
        match self {
            Self::Error(_) => {}
            Self::CustomJson(v) => {
                crate::common_db::apply_common_db_sanitization(v.value.as_value_mut(), config);
            }
            #[cfg(feature = "diagram-mindmap")]
            Self::Mindmap(_) => {}
            #[cfg(feature = "diagram-state")]
            Self::State(v) => v.sanitize_common_db_fields(config),
            #[cfg(feature = "diagram-sequence")]
            Self::Sequence(v) => v.sanitize_common_db_fields(config),
            #[cfg(feature = "diagram-zenuml")]
            Self::Zenuml(v) => v.sanitize_common_db_fields(config),
            #[cfg(any(feature = "diagram-flowchart", feature = "diagram-swimlane"))]
            Self::Flowchart(v) => v.sanitize_common_db_fields(config),
            #[cfg(feature = "diagram-architecture")]
            Self::Architecture(v) => v.sanitize_common_db_fields(config),
            #[cfg(feature = "diagram-class")]
            Self::Class(v) => v.sanitize_common_db_fields(config),
            #[cfg(feature = "diagram-c4")]
            Self::C4(v) => v.sanitize_common_db_fields(config),
            #[cfg(feature = "diagram-cynefin")]
            Self::Cynefin(v) => v.sanitize_common_db_fields(config),
            #[cfg(feature = "diagram-railroad")]
            Self::Railroad(v) => v.sanitize_common_db_fields(config),
            #[cfg(feature = "diagram-kanban")]
            Self::Kanban(_) => {}
            #[cfg(feature = "diagram-gantt")]
            Self::Gantt(v) => v.sanitize_common_db_fields(config),
            #[cfg(feature = "diagram-pie")]
            Self::Pie(v) => v.sanitize_common_db_fields(config),
            #[cfg(feature = "diagram-packet")]
            Self::Packet(v) => v.sanitize_common_db_fields(config),
            #[cfg(feature = "diagram-timeline")]
            Self::Timeline(v) => v.sanitize_common_db_fields(config),
            #[cfg(feature = "diagram-journey")]
            Self::Journey(v) => v.sanitize_common_db_fields(config),
            #[cfg(feature = "diagram-requirement")]
            Self::Requirement(v) => v.sanitize_common_db_fields(config),
            #[cfg(feature = "diagram-sankey")]
            Self::Sankey(_) => {}
            #[cfg(feature = "diagram-radar")]
            Self::Radar(v) => v.sanitize_common_db_fields(config),
            #[cfg(feature = "diagram-info")]
            Self::Info(_) => {}
            #[cfg(feature = "diagram-treemap")]
            Self::Treemap(v) => v.sanitize_common_db_fields(config),
            #[cfg(feature = "diagram-block")]
            Self::Block(_) => {}
            #[cfg(feature = "diagram-er")]
            Self::Er(v) => v.sanitize_common_db_fields(config),
            #[cfg(feature = "diagram-quadrant-chart")]
            Self::QuadrantChart(v) => v.sanitize_common_db_fields(config),
            #[cfg(feature = "diagram-xychart")]
            Self::XyChart(v) => v.sanitize_common_db_fields(config),
            #[cfg(feature = "diagram-git-graph")]
            Self::GitGraph(v) => v.sanitize_common_db_fields(config),
            #[cfg(feature = "diagram-tree-view")]
            Self::TreeView(v) => v.sanitize_common_db_fields(config),
            #[cfg(feature = "diagram-ishikawa")]
            Self::Ishikawa(v) => v.sanitize_common_db_fields(config),
            #[cfg(feature = "diagram-event-modeling")]
            Self::EventModeling(v) => v.sanitize_common_db_fields(config),
            #[cfg(feature = "diagram-venn")]
            Self::Venn(v) => v.sanitize_common_db_fields(config),
            #[cfg(feature = "diagram-wardley")]
            Self::Wardley(v) => v.sanitize_common_db_fields(config),
            #[cfg(feature = "diagram-usecase")]
            Self::Usecase(v) => v.sanitize_common_db_fields(config),
            #[cfg(feature = "diagram-agentflow")]
            Self::Agentflow(v) => v.sanitize_common_db_fields(config),
        }
    }

    pub(crate) fn offset_parser_diagnostic_positions(
        &mut self,
        line_offset: usize,
        column_offset: usize,
    ) {
        #[cfg(not(feature = "diagram-agentflow"))]
        let _ = (line_offset, column_offset);
        #[cfg(feature = "diagram-agentflow")]
        if let Self::Agentflow(model) = self {
            model.offset_diagnostic_positions(line_offset, column_offset);
        }
    }

    pub(crate) fn remap_warning_fact_spans(
        &mut self,
        mut remap: impl FnMut(&mut DiagramWarningFact),
    ) {
        match self {
            Self::CustomJson(v) => {
                Self::remap_json_warning_fact_spans(v.value.as_value_mut(), &mut remap);
            }
            #[cfg(feature = "diagram-agentflow")]
            Self::Agentflow(v) => Self::remap_warning_fact_slice(&mut v.warning_facts, &mut remap),
            #[cfg(any(feature = "diagram-flowchart", feature = "diagram-swimlane"))]
            Self::Flowchart(v) => Self::remap_warning_fact_slice(&mut v.warning_facts, &mut remap),
            #[cfg(feature = "diagram-block")]
            Self::Block(v) => Self::remap_warning_fact_slice(&mut v.warning_facts, &mut remap),
            #[cfg(feature = "diagram-git-graph")]
            Self::GitGraph(v) => Self::remap_warning_fact_slice(&mut v.warning_facts, &mut remap),
            _ => {}
        }
    }

    fn remap_warning_fact_slice(
        facts: &mut [DiagramWarningFact],
        remap: &mut impl FnMut(&mut DiagramWarningFact),
    ) {
        for fact in facts {
            remap(fact);
        }
    }

    fn remap_json_warning_fact_spans(
        model: &mut Value,
        remap: &mut impl FnMut(&mut DiagramWarningFact),
    ) {
        let Some(warning_facts_value) = model.get_mut("warningFacts") else {
            return;
        };
        let Ok(mut warning_facts) = Vec::<DiagramWarningFact>::deserialize(&*warning_facts_value)
        else {
            return;
        };

        Self::remap_warning_fact_slice(&mut warning_facts, remap);
        crate::config::replace_value_nonrecursive(
            warning_facts_value,
            serde_json::json!(warning_facts),
        );
    }

    /// Returns a stable family label for diagnostics and timing output.
    pub fn kind(&self) -> &'static str {
        match self {
            Self::Error(_) => "error",
            Self::CustomJson(_) => "custom-json",
            #[cfg(feature = "diagram-mindmap")]
            Self::Mindmap(_) => "mindmap",
            #[cfg(feature = "diagram-state")]
            Self::State(_) => "state",
            #[cfg(feature = "diagram-sequence")]
            Self::Sequence(_) => "sequence",
            #[cfg(feature = "diagram-zenuml")]
            Self::Zenuml(_) => "zenuml",
            #[cfg(any(feature = "diagram-flowchart", feature = "diagram-swimlane"))]
            Self::Flowchart(_) => "flowchart",
            #[cfg(feature = "diagram-architecture")]
            Self::Architecture(_) => "architecture",
            #[cfg(feature = "diagram-class")]
            Self::Class(_) => "class",
            #[cfg(feature = "diagram-c4")]
            Self::C4(_) => "c4",
            #[cfg(feature = "diagram-cynefin")]
            Self::Cynefin(_) => "cynefin",
            #[cfg(feature = "diagram-railroad")]
            Self::Railroad(_) => "railroad",
            #[cfg(feature = "diagram-kanban")]
            Self::Kanban(_) => "kanban",
            #[cfg(feature = "diagram-gantt")]
            Self::Gantt(_) => "gantt",
            #[cfg(feature = "diagram-pie")]
            Self::Pie(_) => "pie",
            #[cfg(feature = "diagram-packet")]
            Self::Packet(_) => "packet",
            #[cfg(feature = "diagram-timeline")]
            Self::Timeline(_) => "timeline",
            #[cfg(feature = "diagram-journey")]
            Self::Journey(_) => "journey",
            #[cfg(feature = "diagram-requirement")]
            Self::Requirement(_) => "requirement",
            #[cfg(feature = "diagram-sankey")]
            Self::Sankey(_) => "sankey",
            #[cfg(feature = "diagram-radar")]
            Self::Radar(_) => "radar",
            #[cfg(feature = "diagram-info")]
            Self::Info(_) => "info",
            #[cfg(feature = "diagram-treemap")]
            Self::Treemap(_) => "treemap",
            #[cfg(feature = "diagram-block")]
            Self::Block(_) => "block",
            #[cfg(feature = "diagram-er")]
            Self::Er(_) => "er",
            #[cfg(feature = "diagram-quadrant-chart")]
            Self::QuadrantChart(_) => "quadrantChart",
            #[cfg(feature = "diagram-xychart")]
            Self::XyChart(_) => "xychart",
            #[cfg(feature = "diagram-git-graph")]
            Self::GitGraph(_) => "gitGraph",
            #[cfg(feature = "diagram-tree-view")]
            Self::TreeView(_) => "treeView",
            #[cfg(feature = "diagram-ishikawa")]
            Self::Ishikawa(_) => "ishikawa",
            #[cfg(feature = "diagram-event-modeling")]
            Self::EventModeling(_) => "eventmodeling",
            #[cfg(feature = "diagram-venn")]
            Self::Venn(_) => "venn",
            #[cfg(feature = "diagram-wardley")]
            Self::Wardley(_) => "wardley",
            #[cfg(feature = "diagram-usecase")]
            Self::Usecase(_) => "usecase",
            #[cfg(feature = "diagram-agentflow")]
            Self::Agentflow(_) => "agentflow",
        }
    }

    /// Projects this family-owned typed model into the public compatibility JSON shape.
    ///
    /// Built-in families delegate to their own lossless projector. This never reparses source;
    /// custom adapters retain their explicitly named JSON boundary.
    pub fn compatibility_json(&self, meta: &ParseMetadata) -> Result<ManagedSemanticJson> {
        let control = OperationControl::new();
        self.compatibility_json_controlled(meta, &control)
            .expect("a private operation control cannot be cancelled")
    }

    /// Projects the public compatibility JSON while observing the caller's operation control.
    pub fn compatibility_json_controlled(
        &self,
        meta: &ParseMetadata,
        control: &OperationControl,
    ) -> OperationControlResult<Result<ManagedSemanticJson>> {
        let control = control.for_phase(OperationPhase::Semantic);
        control.checkpoint()?;
        let projected = match self {
            Self::Error(model) => model.compatibility_json_controlled(meta, &control),
            Self::CustomJson(model) => model.json().clone_controlled(&control).map(Ok),
            #[cfg(feature = "diagram-mindmap")]
            Self::Mindmap(model) => model.compatibility_json_controlled(meta, &control),
            #[cfg(feature = "diagram-state")]
            Self::State(model) => model.compatibility_json_controlled(meta, &control),
            #[cfg(feature = "diagram-sequence")]
            Self::Sequence(model) => model.compatibility_json_controlled(meta, &control),
            #[cfg(feature = "diagram-zenuml")]
            Self::Zenuml(model) => model.compatibility_json_controlled(meta, &control),
            #[cfg(any(feature = "diagram-flowchart", feature = "diagram-swimlane"))]
            Self::Flowchart(model) => model.compatibility_json_controlled(meta, &control),
            #[cfg(feature = "diagram-architecture")]
            Self::Architecture(model) => model.compatibility_json_controlled(meta, &control),
            #[cfg(feature = "diagram-class")]
            Self::Class(model) => model.compatibility_json_controlled(meta, &control),
            #[cfg(feature = "diagram-c4")]
            Self::C4(model) => model.compatibility_json_controlled(meta, &control),
            #[cfg(feature = "diagram-cynefin")]
            Self::Cynefin(model) => model.compatibility_json_controlled(meta, &control),
            #[cfg(feature = "diagram-railroad")]
            Self::Railroad(model) => model.compatibility_json_controlled(meta, &control),
            #[cfg(feature = "diagram-kanban")]
            Self::Kanban(model) => model.compatibility_json_controlled(meta, &control),
            #[cfg(feature = "diagram-gantt")]
            Self::Gantt(model) => model.compatibility_json_controlled(meta, &control),
            #[cfg(feature = "diagram-pie")]
            Self::Pie(model) => model.compatibility_json_controlled(meta, &control),
            #[cfg(feature = "diagram-packet")]
            Self::Packet(model) => model.compatibility_json_controlled(meta, &control),
            #[cfg(feature = "diagram-timeline")]
            Self::Timeline(model) => model.compatibility_json_controlled(meta, &control),
            #[cfg(feature = "diagram-journey")]
            Self::Journey(model) => model.compatibility_json_controlled(meta, &control),
            #[cfg(feature = "diagram-requirement")]
            Self::Requirement(model) => model.compatibility_json_controlled(meta, &control),
            #[cfg(feature = "diagram-sankey")]
            Self::Sankey(model) => model.compatibility_json_controlled(meta, &control),
            #[cfg(feature = "diagram-radar")]
            Self::Radar(model) => model.compatibility_json_controlled(meta, &control),
            #[cfg(feature = "diagram-info")]
            Self::Info(model) => model.compatibility_json_controlled(meta, &control),
            #[cfg(feature = "diagram-treemap")]
            Self::Treemap(model) => model.compatibility_json_controlled(meta, &control),
            #[cfg(feature = "diagram-block")]
            Self::Block(model) => model.compatibility_json_controlled(meta, &control),
            #[cfg(feature = "diagram-er")]
            Self::Er(model) => model.compatibility_json_controlled(meta, &control),
            #[cfg(feature = "diagram-quadrant-chart")]
            Self::QuadrantChart(model) => model.compatibility_json_controlled(meta, &control),
            #[cfg(feature = "diagram-xychart")]
            Self::XyChart(model) => model.compatibility_json_controlled(meta, &control),
            #[cfg(feature = "diagram-git-graph")]
            Self::GitGraph(model) => model.compatibility_json_controlled(meta, &control),
            #[cfg(feature = "diagram-tree-view")]
            Self::TreeView(model) => model.compatibility_json_controlled(meta, &control),
            #[cfg(feature = "diagram-ishikawa")]
            Self::Ishikawa(model) => model.compatibility_json_controlled(meta, &control),
            #[cfg(feature = "diagram-event-modeling")]
            Self::EventModeling(model) => model.compatibility_json_controlled(meta, &control),
            #[cfg(feature = "diagram-venn")]
            Self::Venn(model) => model.compatibility_json_controlled(meta, &control),
            #[cfg(feature = "diagram-wardley")]
            Self::Wardley(model) => model.compatibility_json_controlled(meta, &control),
            #[cfg(feature = "diagram-usecase")]
            Self::Usecase(model) => model.compatibility_json_controlled(meta, &control),
            #[cfg(feature = "diagram-agentflow")]
            Self::Agentflow(model) => model.compatibility_json_controlled(meta, &control),
        }?;
        control.checkpoint()?;
        Ok(projected)
    }

    /// Returns whether this typed model can represent the given Mermaid diagram type id.
    pub fn supports_diagram_type(&self, diagram_type: &str) -> bool {
        match self {
            Self::CustomJson(model) => {
                !crate::family::is_builtin_diagram_type(diagram_type)
                    && model.model_name() == diagram_type
            }
            other => {
                crate::family::render_model_kind_supports_diagram_type(other.kind(), diagram_type)
            }
        }
    }
}

/// Registry for typed render-model parsers keyed by Mermaid diagram type id.
#[derive(Debug, Clone)]
pub struct RenderDiagramRegistry {
    builtins: Arc<HashMap<&'static str, BuiltInRenderSemanticParser>>,
    overlays: Arc<HashMap<&'static str, CustomJsonRenderParser>>,
}

#[derive(Clone, Copy)]
pub(crate) enum ResolvedRenderParser {
    BuiltIn(BuiltInRenderSemanticParser),
    Custom(CustomJsonRenderParser),
}

impl Default for RenderDiagramRegistry {
    fn default() -> Self {
        Self::new()
    }
}

impl RenderDiagramRegistry {
    /// Creates an empty registry.
    pub fn new() -> Self {
        Self {
            builtins: Arc::new(HashMap::new()),
            overlays: Arc::new(HashMap::new()),
        }
    }

    /// Registers or replaces a custom JSON render-model parser for a diagram type id.
    pub fn insert(&mut self, diagram_type: &'static str, parser: CustomJsonRenderParser) {
        Arc::make_mut(&mut self.overlays).insert(diagram_type, parser);
    }

    /// Returns whether a built-in or custom render-model parser is registered for the id.
    pub fn contains(&self, diagram_type: &str) -> bool {
        self.resolve(diagram_type).is_some()
    }

    pub(crate) fn resolve(&self, diagram_type: &str) -> Option<ResolvedRenderParser> {
        if let Some(parser) = self.overlays.get(diagram_type).copied() {
            return Some(ResolvedRenderParser::Custom(parser));
        }
        self.builtins
            .get(diagram_type)
            .copied()
            .map(ResolvedRenderParser::BuiltIn)
    }

    #[cfg(all(test, feature = "diagram-flowchart"))]
    pub(crate) fn remove(&mut self, diagram_type: &str) -> bool {
        Arc::make_mut(&mut self.overlays)
            .remove(diagram_type)
            .is_some()
            || Arc::make_mut(&mut self.builtins)
                .remove(diagram_type)
                .is_some()
    }

    /// Builds the typed render parser registry for the repository's pinned Mermaid baseline.
    pub fn pinned_mermaid_baseline() -> Self {
        let mut reg = Self::new();
        for fact in crate::family::render_parser_facts() {
            Arc::make_mut(&mut reg.builtins).insert(fact.id, fact.parser);
        }

        reg
    }

    #[cfg(all(test, feature = "all-diagrams"))]
    pub(crate) fn parser_ids(&self) -> impl Iterator<Item = &'static str> + '_ {
        self.builtins
            .keys()
            .chain(
                self.overlays
                    .keys()
                    .filter(|id| !self.builtins.contains_key(**id)),
            )
            .copied()
    }
}

/// Parsed diagram metadata plus its canonically paired typed render model.
///
/// Construction is restricted to `merman-core`; its parse pipeline pairs the metadata and model.
/// External consumers may inspect the pair or consume it, but cannot assemble metadata and a
/// model produced by different parse operations:
///
/// ```compile_fail,E0451
/// use merman_core::{ParseMetadata, ParsedDiagramRender, RenderSemanticModel};
///
/// fn forge(
///     meta: ParseMetadata,
///     model: RenderSemanticModel,
/// ) -> ParsedDiagramRender {
///     ParsedDiagramRender { meta, model }
/// }
/// ```
#[derive(Debug, Clone)]
pub struct ParsedDiagramRender {
    /// Diagram type and effective configuration extracted during preprocessing.
    meta: ParseMetadata,
    /// Typed model consumed by layout and SVG renderers.
    model: RenderSemanticModel,
    /// Parser-owned render-only data paired with the typed model.
    context: RenderSemanticContext,
}

impl ParsedDiagramRender {
    pub(crate) fn new(meta: ParseMetadata, model: RenderSemanticModel) -> Self {
        Self {
            meta,
            model,
            context: RenderSemanticContext::default(),
        }
    }

    pub(crate) fn from_parse_output(
        meta: ParseMetadata,
        output: RenderSemanticParseOutput,
    ) -> Self {
        let (model, context) = output.into_parts();
        Self {
            meta,
            model,
            context,
        }
    }

    /// Returns the metadata paired with this render model by the core parse pipeline.
    pub fn metadata(&self) -> &ParseMetadata {
        &self.meta
    }

    /// Returns the typed render model paired with this metadata by the core parse pipeline.
    pub fn model(&self) -> &RenderSemanticModel {
        &self.model
    }

    /// Consumes the parsed diagram and returns its canonical metadata/model projection.
    ///
    /// This intentionally omits parser-owned render context. Renderer integrations that need the
    /// complete built-in render state should consume [`Self::into_render_parts`] instead.
    pub fn into_parts(self) -> (ParseMetadata, RenderSemanticModel) {
        (self.meta, self.model)
    }

    /// Consumes the parsed diagram and includes parser-owned render context for renderer crates.
    #[doc(hidden)]
    pub fn into_render_parts(self) -> (ParseMetadata, RenderSemanticModel, RenderSemanticContext) {
        (self.meta, self.model, self.context)
    }

    #[doc(hidden)]
    pub fn retained_render_context_bytes(&self) -> usize {
        self.context.retained_text_bytes()
    }

    /// Borrows all parser-owned Flowchart render facts without exposing them in the typed model.
    #[doc(hidden)]
    #[cfg(any(feature = "diagram-flowchart", feature = "diagram-swimlane"))]
    pub fn flowchart_render_context(
        &self,
    ) -> Option<&crate::diagrams::flowchart::FlowchartRenderContext> {
        self.context.flowchart_render_context()
    }
}

/// Parses with a registry entry while preserving the caller-owned parse control.
pub(crate) fn parse_or_unsupported_controlled(
    registry: &DiagramRegistry,
    diagram_type: &str,
    code: &str,
    meta: &ParseMetadata,
    control: &OperationControl,
) -> OperationControlResult<Result<ManagedSemanticJson>> {
    control.checkpoint()?;
    let Some(parser) = registry.resolve(diagram_type) else {
        return Ok(Err(Error::UnsupportedDiagram {
            diagram_type: diagram_type.to_string(),
        }));
    };
    let result = match parser {
        ResolvedSemanticParser::BuiltIn(parser) => parser(code, meta),
        ResolvedSemanticParser::Custom(parser) => parser(code, meta, control)?,
    }
    .map(ManagedSemanticJson::from);
    control.checkpoint()?;
    Ok(result)
}

#[cfg(test)]
mod registry_clone_tests {
    use super::*;
    use std::sync::Arc;

    fn custom_semantic_parser(
        _code: &str,
        _meta: &ParseMetadata,
        control: &OperationControl,
    ) -> OperationControlResult<Result<Value>> {
        control.checkpoint()?;
        Ok(Ok(Value::Null))
    }

    fn custom_render_parser(
        _code: &str,
        _meta: &ParseMetadata,
        control: &OperationControl,
    ) -> OperationControlResult<Result<CustomJsonRenderModel>> {
        control.checkpoint()?;
        Ok(Ok(CustomJsonRenderModel::new(
            "copy-on-write-render-test",
            Value::Null,
        )))
    }

    fn cancelling_semantic_parser(
        _code: &str,
        _meta: &ParseMetadata,
        control: &OperationControl,
    ) -> OperationControlResult<Result<Value>> {
        control.cancel();
        control.checkpoint()?;
        unreachable!("cancelled parser must stop at its checkpoint")
    }

    #[test]
    fn semantic_registry_clone_uses_copy_on_write_storage() {
        let original = DiagramRegistry::pinned_mermaid_baseline();
        let mut cloned = original.clone();

        assert!(Arc::ptr_eq(&original.builtins, &cloned.builtins));
        assert!(Arc::ptr_eq(&original.overlays, &cloned.overlays));

        cloned.insert("copy-on-write-semantic-test", custom_semantic_parser);

        assert!(Arc::ptr_eq(&original.builtins, &cloned.builtins));
        assert!(!Arc::ptr_eq(&original.overlays, &cloned.overlays));
        assert!(original.resolve("copy-on-write-semantic-test").is_none());
        assert!(matches!(
            cloned.resolve("copy-on-write-semantic-test"),
            Some(ResolvedSemanticParser::Custom(_))
        ));
    }

    #[test]
    fn custom_semantic_parsers_share_the_operation_control() {
        let mut engine = crate::Engine::new();
        engine
            .diagram_registry_mut()
            .insert("flowchart-v2", cancelling_semantic_parser);
        let control = OperationControl::new();

        assert!(matches!(
            engine.parse_diagram_snapshot_controlled_sync("flowchart TD\nA-->B\n", &control),
            Err(crate::OperationCancelled { .. })
        ));
        assert!(control.is_cancelled());
    }

    #[test]
    fn custom_parser_cancellation_is_typed_for_non_cancellable_snapshot_apis() {
        let mut engine = crate::Engine::new();
        engine
            .diagram_registry_mut()
            .insert("flowchart-v2", cancelling_semantic_parser);
        let source = "flowchart TD\nA-->B\n";

        assert!(matches!(
            engine.parse_diagram_snapshot_sync(source),
            Err(Error::OperationCancelled(_))
        ));
        assert!(matches!(
            engine.parse_diagram_snapshot_with_type_sync("flowchart-v2", source),
            Err(Error::OperationCancelled(_))
        ));
    }

    #[test]
    fn render_registry_clone_uses_copy_on_write_storage() {
        let original = RenderDiagramRegistry::pinned_mermaid_baseline();
        let mut cloned = original.clone();

        assert!(Arc::ptr_eq(&original.builtins, &cloned.builtins));
        assert!(Arc::ptr_eq(&original.overlays, &cloned.overlays));

        cloned.insert("copy-on-write-render-test", custom_render_parser);

        assert!(Arc::ptr_eq(&original.builtins, &cloned.builtins));
        assert!(!Arc::ptr_eq(&original.overlays, &cloned.overlays));
        assert!(!original.contains("copy-on-write-render-test"));
        assert!(cloned.contains("copy-on-write-render-test"));
    }
}
