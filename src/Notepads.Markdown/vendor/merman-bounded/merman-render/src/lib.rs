#![forbid(unsafe_code)]

//! Headless layout + rendering for Mermaid diagrams.
//!
//! This crate consumes `merman-core`'s semantic models and produces:
//! - a layout JSON (geometry + routes)
//! - Mermaid-like SVG output with DOM parity checks against upstream baselines
//!
//! This low-level crate has no default diagram families. Select `diagram-*` features or
//! `all-diagrams` explicitly; optional layout engines and math remain independent selectors.
//! Family-owned typed layout APIs are conditional on those selections. Use
//! [`family::supports_diagram_type`] to inspect this renderer's local handler availability:
//! another dependency may enable additional core parsers without enabling their renderers.

#[cfg(feature = "layout-cytoscape")]
#[cfg(feature = "diagram-architecture")]
pub mod architecture;
#[cfg(all(feature = "layout-cytoscape", feature = "diagram-architecture"))]
pub(crate) mod architecture_metrics;
#[cfg(feature = "diagram-block")]
pub mod block;
#[cfg(feature = "diagram-c4")]
pub mod c4;
#[cfg(feature = "diagram-xychart")]
mod chart_palette;
#[cfg(feature = "diagram-class")]
pub mod class;
#[cfg_attr(
    not(feature = "all-diagrams"),
    allow(
        dead_code,
        unused_imports,
        reason = "Shared rendering utilities have different callers in each diagram selection."
    )
)]
mod config;
#[cfg(feature = "diagram-cynefin")]
pub mod cynefin;
#[cfg(any(
    feature = "diagram-agentflow",
    feature = "diagram-flowchart",
    feature = "diagram-swimlane",
    feature = "diagram-state"
))]
mod dagre;
#[cfg(feature = "layout-elk")]
#[cfg_attr(
    not(feature = "all-diagrams"),
    allow(
        dead_code,
        unused_imports,
        reason = "Shared rendering utilities have different callers in each diagram selection."
    )
)]
mod elk_adapter;
#[cfg_attr(
    not(feature = "all-diagrams"),
    allow(
        dead_code,
        unused_imports,
        reason = "Shared rendering utilities have different callers in each diagram selection."
    )
)]
mod elk_edge_geometry;
#[cfg(feature = "layout-elk")]
#[cfg_attr(
    not(feature = "all-diagrams"),
    allow(
        dead_code,
        unused_imports,
        reason = "Shared rendering utilities have different callers in each diagram selection."
    )
)]
mod elk_feedback_edges;
#[cfg_attr(
    not(feature = "all-diagrams"),
    allow(
        dead_code,
        unused_imports,
        reason = "Shared rendering utilities have different callers in each diagram selection."
    )
)]
mod elk_geometry;
#[cfg(feature = "layout-elk")]
#[cfg_attr(
    not(feature = "all-diagrams"),
    allow(
        dead_code,
        unused_imports,
        reason = "Shared rendering utilities have different callers in each diagram selection."
    )
)]
mod elk_hierarchy;
#[cfg(feature = "layout-elk")]
#[cfg_attr(
    not(feature = "all-diagrams"),
    allow(
        dead_code,
        unused_imports,
        reason = "Shared rendering utilities have different callers in each diagram selection."
    )
)]
mod elk_options;
#[cfg_attr(
    not(feature = "all-diagrams"),
    allow(
        dead_code,
        unused_imports,
        reason = "Shared rendering utilities have different callers in each diagram selection."
    )
)]
mod elk_terminal_jogs;
#[cfg_attr(
    not(feature = "all-diagrams"),
    allow(
        dead_code,
        unused_imports,
        reason = "Shared rendering utilities have different callers in each diagram selection."
    )
)]
mod entities;
pub mod environment;
#[cfg(feature = "diagram-er")]
pub mod er;
pub mod error;
#[cfg(feature = "diagram-event-modeling")]
pub mod eventmodeling;
pub mod family;
#[cfg(any(
    feature = "diagram-flowchart",
    feature = "diagram-swimlane",
    feature = "diagram-agentflow"
))]
pub mod flowchart;
#[cfg(feature = "diagram-gantt")]
pub mod gantt;
mod generated;
#[cfg(feature = "diagram-git-graph")]
pub mod gitgraph;
#[cfg(any(
    feature = "diagram-agentflow",
    feature = "diagram-class",
    feature = "diagram-flowchart",
    feature = "diagram-swimlane",
    feature = "diagram-mindmap"
))]
#[cfg_attr(
    not(feature = "all-diagrams"),
    allow(
        dead_code,
        unused_imports,
        reason = "Shared rendering utilities have different callers in each diagram selection."
    )
)]
mod graph_label;
#[cfg(feature = "diagram-info")]
pub mod info;
#[cfg(feature = "diagram-ishikawa")]
pub mod ishikawa;
#[cfg(feature = "diagram-journey")]
pub mod journey;
#[cfg(feature = "diagram-kanban")]
pub mod kanban;
#[cfg_attr(
    not(feature = "all-diagrams"),
    allow(
        dead_code,
        unused_imports,
        reason = "Shared rendering utilities have different callers in each diagram selection."
    )
)]
mod layout_backend;
#[cfg_attr(
    not(feature = "all-diagrams"),
    allow(
        dead_code,
        unused_imports,
        reason = "Shared rendering utilities have different callers in each diagram selection."
    )
)]
mod layout_work;
#[cfg_attr(
    not(feature = "all-diagrams"),
    allow(
        dead_code,
        unused_imports,
        reason = "Shared rendering utilities have different callers in each diagram selection."
    )
)]
pub mod math;
#[cfg_attr(
    not(feature = "all-diagrams"),
    allow(
        dead_code,
        unused_imports,
        reason = "Shared rendering utilities have different callers in each diagram selection."
    )
)]
mod mermaid_style;
#[cfg(feature = "diagram-mindmap")]
pub mod mindmap;
pub mod model;
#[cfg(feature = "diagram-packet")]
pub mod packet;
#[cfg(feature = "diagram-pie")]
pub mod pie;
pub mod presentation;
#[cfg(feature = "diagram-quadrant-chart")]
pub mod quadrantchart;
#[cfg(feature = "diagram-radar")]
pub mod radar;
#[cfg(feature = "diagram-railroad")]
pub mod railroad;
#[cfg(feature = "diagram-requirement")]
pub mod requirement;
pub mod resources;
#[cfg(feature = "diagram-sankey")]
pub mod sankey;
#[cfg(feature = "diagram-sequence")]
pub mod sequence;
#[cfg(feature = "diagram-state")]
pub mod state;
pub mod svg;
#[cfg(any(
    feature = "diagram-flowchart",
    feature = "diagram-swimlane",
    feature = "diagram-mindmap"
))]
pub mod swimlane;
#[cfg_attr(
    not(feature = "all-diagrams"),
    allow(
        dead_code,
        unused_imports,
        reason = "Shared rendering utilities have different callers in each diagram selection."
    )
)]
pub mod text;
#[cfg_attr(
    not(feature = "all-diagrams"),
    allow(
        dead_code,
        unused_imports,
        reason = "Shared rendering utilities have different callers in each diagram selection."
    )
)]
mod theme;
#[cfg(feature = "diagram-timeline")]
pub mod timeline;
#[cfg(feature = "diagram-tree-view")]
pub mod tree_view;
#[cfg(feature = "diagram-treemap")]
pub mod treemap;

#[cfg(feature = "diagram-usecase")]
pub mod usecase;
#[cfg(feature = "diagram-venn")]
pub mod venn;
#[cfg(feature = "diagram-wardley")]
pub mod wardley;
#[cfg_attr(
    not(feature = "all-diagrams"),
    allow(
        dead_code,
        unused_imports,
        reason = "Shared rendering utilities have different callers in each diagram selection."
    )
)]
mod xml;
#[cfg(feature = "diagram-xychart")]
pub mod xychart;
#[cfg(feature = "diagram-zenuml")]
pub mod zenuml;

/// Reports whether the Cytoscape-derived layout backend is present in this compiled renderer.
pub const fn layout_cytoscape_available() -> bool {
    cfg!(feature = "layout-cytoscape")
}

/// Reports whether the ELK layout backend is present in this compiled renderer.
pub const fn layout_elk_available() -> bool {
    cfg!(feature = "layout-elk")
}

/// Reports whether the built-in math renderer is present in this compiled renderer.
pub const fn math_available() -> bool {
    cfg!(feature = "math")
}

/// Optional renderer capabilities that a typed diagram operation may require.
#[non_exhaustive]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum RenderCapability {
    LayoutCytoscape,
    LayoutElk,
    Math,
}

impl RenderCapability {
    const fn bit(self) -> u8 {
        match self {
            Self::LayoutCytoscape => 1 << 0,
            Self::LayoutElk => 1 << 1,
            Self::Math => 1 << 2,
        }
    }

    pub const fn id(self) -> &'static str {
        match self {
            Self::LayoutCytoscape => "layout-cytoscape",
            Self::LayoutElk => "layout-elk",
            Self::Math => "math",
        }
    }
}

const ALL_RENDER_CAPABILITY_BITS: u8 = RenderCapability::LayoutCytoscape.bit()
    | RenderCapability::LayoutElk.bit()
    | RenderCapability::Math.bit();

/// Operation-level permission for optional renderer capabilities.
///
/// The policy does not claim that a backend is compiled or that a service is installed. Effective
/// availability requires both this permission and the concrete backend/service. The unrestricted
/// default preserves the direct Rust renderer API, while artifact facades can project their exact
/// owner-selected capability contract without depending on binding-specific types.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RenderCapabilityPolicy {
    allowed_bits: u8,
}

impl RenderCapabilityPolicy {
    /// Allows every optional renderer capability that is otherwise available.
    pub const fn unrestricted() -> Self {
        Self {
            allowed_bits: ALL_RENDER_CAPABILITY_BITS,
        }
    }

    /// Denies every optional renderer capability until explicitly allowed.
    pub const fn deny_all() -> Self {
        Self { allowed_bits: 0 }
    }

    /// Allows one optional renderer capability.
    #[must_use]
    pub const fn with_allowed(mut self, capability: RenderCapability) -> Self {
        self.allowed_bits |= capability.bit();
        self
    }

    /// Reports whether this policy permits the capability.
    pub const fn allows(self, capability: RenderCapability) -> bool {
        self.allowed_bits & capability.bit() != 0
    }
}

impl Default for RenderCapabilityPolicy {
    fn default() -> Self {
        Self::unrestricted()
    }
}

impl std::fmt::Display for RenderCapability {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.id())
    }
}

use crate::environment::{RenderSession, RoutedTextMeasurer, TextMeasurementPhase};
use merman_core::OperationPhase;
#[cfg(any(
    feature = "diagram-flowchart",
    feature = "diagram-swimlane",
    feature = "diagram-agentflow"
))]
use merman_core::diagrams::flowchart::FlowchartModel;
#[cfg(feature = "diagram-class")]
use merman_core::models::class_diagram::ClassDiagram;

pub use resources::{
    CLI_DEFAULT_RESOURCE_PROFILE, GENERAL_BINDING_DEFAULT_RESOURCE_PROFILE, RenderResourceLimitId,
    RenderResourcePolicy, RenderResourceProfile, RenderResourceProfileDescriptor,
    ResourceLimitCause, ResourceLimitDescriptor, ResourceLimitExceeded, ResourceLimitId,
    ResourceLimitOverride, ResourceLimitOverrideError, ResourceLimitPhase,
    resource_limit_descriptors, resource_profile_descriptors,
};

#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum Error {
    #[error(transparent)]
    Cancelled(#[from] merman_core::OperationCancelled),
    #[error("unsupported diagram type for layout: {diagram_type}")]
    UnsupportedDiagram { diagram_type: String },
    #[error("render session lacks capability `{capability}` required by diagram `{diagram_type}`")]
    MissingCapability {
        capability: RenderCapability,
        diagram_type: String,
    },
    #[error("invalid semantic model: {message}")]
    InvalidModel { message: String },
    #[error(
        "custom JSON model `{model_name}` from {provenance:?} cannot render diagram type `{diagram_type}`"
    )]
    NonRenderableCustomModel {
        diagram_type: String,
        model_name: String,
        provenance: merman_core::CustomJsonProvenance,
    },
    #[error("SVG postprocessor `{pass}` failed: {message}")]
    SvgPostprocess { pass: String, message: String },
    #[error("external icon output is invalid: {message}")]
    InvalidIconOutput { message: String },
    #[error("icon rendering failed internally: {message}")]
    IconProcessing { message: String },
    #[error(transparent)]
    ResourceLimitExceeded(#[from] ResourceLimitExceeded),
    #[error(transparent)]
    OperationResourceTerminal(merman_core::OperationLedgerError),
    #[error(transparent)]
    Color(#[from] merman_core::theme_color::ColorError),
    #[error("semantic model JSON error: {0}")]
    Json(#[from] serde_json::Error),
    #[error(transparent)]
    OperationTimingUnavailable(#[from] merman_core::runtime::OperationTimingUnavailable),
}

pub type Result<T> = std::result::Result<T, Error>;

impl From<dugong::LayoutError> for Error {
    fn from(error: dugong::LayoutError) -> Self {
        Self::InvalidModel {
            message: error.to_string(),
        }
    }
}

impl From<crate::resources::OperationWorkError> for Error {
    fn from(error: crate::resources::OperationWorkError) -> Self {
        match error {
            crate::resources::OperationWorkError::Cancelled(error) => Self::Cancelled(error),
            crate::resources::OperationWorkError::ResourceLimitExceeded(error) => {
                Self::ResourceLimitExceeded(error)
            }
            crate::resources::OperationWorkError::ForeignResourceTerminal(error) => {
                Self::OperationResourceTerminal(error)
            }
        }
    }
}

impl Error {
    pub fn svg_postprocess(pass: impl Into<String>, message: impl Into<String>) -> Self {
        Self::SvgPostprocess {
            pass: pass.into(),
            message: message.into(),
        }
    }

    #[cfg_attr(
        not(feature = "all-diagrams"),
        allow(
            dead_code,
            reason = "Shared operation support has different consumers in each diagram selection."
        )
    )]
    pub(crate) fn invalid_icon_output(message: impl Into<String>) -> Self {
        Self::InvalidIconOutput {
            message: message.into(),
        }
    }

    #[cfg_attr(
        not(feature = "all-diagrams"),
        allow(
            dead_code,
            reason = "Shared operation support has different consumers in each diagram selection."
        )
    )]
    pub(crate) fn icon_processing(message: impl Into<String>) -> Self {
        Self::IconProcessing {
            message: message.into(),
        }
    }

    pub const fn missing_capability(&self) -> Option<RenderCapability> {
        match self {
            Self::MissingCapability { capability, .. } => Some(*capability),
            _ => None,
        }
    }
}

/// Host-provided geometry available to family layout algorithms.
///
/// This models the element that owns diagram layout, not a browser page viewport and not the
/// final SVG viewport emitted after layout.
#[derive(Debug, Clone)]
#[non_exhaustive]
pub struct LayoutOptions {
    /// Width of the host layout container in CSS pixels.
    ///
    /// Families whose Mermaid renderer reads DOM-available width use this value.
    pub container_width: f64,
    /// Height of the host layout container in CSS pixels.
    pub container_height: f64,
    /// Browser `screen.availWidth` in CSS pixels when the host has a screen environment.
    ///
    /// Mermaid's C4 renderer uses the available screen width rather than its container width.
    /// `None` keeps headless rendering deterministic by falling back to `container_width`.
    pub screen_available_width: Option<f64>,
}

impl Default for LayoutOptions {
    fn default() -> Self {
        Self {
            container_width: 800.0,
            container_height: 600.0,
            screen_available_width: None,
        }
    }
}

impl LayoutOptions {
    /// Returns geometry defaults suitable for headless SVG rendering.
    pub fn headless_svg_defaults() -> Self {
        Self::default()
    }

    /// Sets the host layout container dimensions in CSS pixels.
    pub fn with_container_size(mut self, width: f64, height: f64) -> Self {
        self.container_width = width;
        self.container_height = height;
        self
    }

    /// Supplies the browser `screen.availWidth` observed by the host.
    pub fn with_screen_available_width(mut self, width: f64) -> Self {
        self.screen_available_width = Some(width);
        self
    }
}

#[cfg_attr(
    not(feature = "all-diagrams"),
    allow(
        dead_code,
        reason = "Shared operation support has different consumers in each diagram selection."
    )
)]
pub(crate) struct LayoutExecution<'a> {
    request: &'a LayoutOptions,
    session: &'a RenderSession,
    text_measurer: RoutedTextMeasurer<'a>,
}

#[cfg_attr(
    not(feature = "all-diagrams"),
    allow(
        dead_code,
        reason = "Shared operation support has different consumers in each diagram selection."
    )
)]
impl<'a> LayoutExecution<'a> {
    pub(crate) fn new(request: &'a LayoutOptions, session: &'a RenderSession) -> Self {
        Self {
            request,
            session,
            text_measurer: session
                .controlled_text_measurer(TextMeasurementPhase::Layout, OperationPhase::Layout),
        }
    }

    pub(crate) fn text_measurer(&self) -> &dyn crate::text::TextMeasurer {
        &self.text_measurer
    }

    pub(crate) fn math_renderer(&self) -> Option<&(dyn crate::math::MathRenderer + Send + Sync)> {
        self.session.math_renderer()
    }

    pub(crate) fn work_meter(&self) -> std::sync::Arc<crate::resources::OperationWorkMeter> {
        std::sync::Arc::clone(self.session.work_meter())
    }

    pub(crate) fn work_meter_ref(&self) -> &crate::resources::OperationWorkMeter {
        self.session.work_meter().as_ref()
    }

    pub(crate) fn local_time_zone(&self) -> &merman_core::time::LocalTimeZone {
        self.session.local_time_zone()
    }

    #[cfg(feature = "layout-cytoscape")]
    pub(crate) fn operation_seed(&self) -> u64 {
        self.session.render_seed().get()
    }

    #[cfg(feature = "layout-elk")]
    pub(crate) fn elk_operation_seed(&self) -> merman_layout_elk::ElkOperationSeed {
        // The ELK source port applies its own stable ELK-specific domain and graph-path
        // derivation. This token merely keeps every ELK random boundary tied to one immutable
        // render operation.
        merman_layout_elk::ElkOperationSeed::from_operation_seed(self.session.render_seed())
    }
}

impl std::ops::Deref for LayoutExecution<'_> {
    type Target = LayoutOptions;

    fn deref(&self) -> &Self::Target {
        self.request
    }
}

#[cfg(feature = "diagram-class")]
pub(crate) fn layout_class_typed_by_engine(
    model: &ClassDiagram,
    effective_config: &merman_core::MermaidConfig,
    options: &LayoutExecution<'_>,
) -> Result<model::ClassDiagramLayout> {
    options
        .work_meter_ref()
        .preflight_class_complexity(model, OperationPhase::Layout)?;
    let mut work_control = layout_work::OperationLayoutWorkControl::new(options.work_meter());
    let preparation_work = class::class_layout_work_units(model, &work_control)?;
    work_control.charge_adapter(preparation_work)?;
    let selection = layout_backend::resolve_graph_layout(effective_config.as_value());
    selection.validate_rootless_graph()?;
    #[cfg(feature = "layout-elk")]
    if selection.backend == layout_backend::GraphLayoutBackend::Elk {
        return class::layout_class_diagram_elk_typed_with_config_and_operation_seed(
            model,
            effective_config,
            options.text_measurer(),
            options.math_renderer(),
            options.elk_operation_seed(),
            &mut work_control,
        );
    }
    class::layout_class_diagram_typed_with_config(
        model,
        effective_config,
        options.text_measurer(),
        options.math_renderer(),
        &mut work_control,
    )
}

#[cfg(all(test, feature = "layout-elk"))]
#[cfg(any(
    feature = "diagram-flowchart",
    feature = "diagram-swimlane",
    feature = "diagram-agentflow"
))]
pub(crate) fn layout_flowchart_typed_by_engine(
    model: &FlowchartModel,
    effective_config: &merman_core::MermaidConfig,
    options: &LayoutExecution<'_>,
) -> Result<model::FlowchartLayout> {
    layout_flowchart_typed_with_render_labels_by_engine(
        model,
        &merman_core::diagrams::flowchart::FlowchartRenderContext::default(),
        effective_config,
        options,
        None,
    )
}

#[cfg(any(
    feature = "diagram-flowchart",
    feature = "diagram-swimlane",
    feature = "diagram-agentflow"
))]
pub(crate) fn layout_flowchart_typed_with_render_labels_by_engine(
    model: &FlowchartModel,
    render_label_sources: &merman_core::diagrams::flowchart::FlowchartRenderContext,
    effective_config: &merman_core::MermaidConfig,
    options: &LayoutExecution<'_>,
    svg_label_sidecar: Option<&flowchart::FlowchartSvgLabelSidecarBuilder>,
) -> Result<model::FlowchartLayout> {
    options.session.checkpoint(OperationPhase::Layout)?;
    let selection = layout_backend::resolve_graph_layout(effective_config.as_value());
    selection.validate_rootless_graph()?;
    #[cfg(feature = "layout-elk")]
    if selection.backend == layout_backend::GraphLayoutBackend::Elk {
        return flowchart::elk::layout_flowchart_elk_typed_with_render_labels_and_operation_seed(
            model,
            render_label_sources,
            effective_config,
            flowchart::elk::FlowchartElkLayoutExecution::new(
                options.text_measurer(),
                options.math_renderer(),
                options.elk_operation_seed(),
                svg_label_sidecar,
                options.work_meter(),
            ),
        );
    }

    flowchart::layout_flowchart_typed_with_render_labels_and_work_meter_and_svg_label_sidecar(
        model,
        render_label_sources,
        effective_config,
        options.text_measurer(),
        options.math_renderer(),
        svg_label_sidecar,
        options.work_meter(),
    )
}

#[cfg(all(test, feature = "all-diagrams"))]
mod tests {
    use super::*;
    #[cfg(feature = "layout-elk")]
    use merman_core::ParsedDiagramRender;
    #[cfg(feature = "layout-elk")]
    use merman_core::RenderSemanticModel;
    use merman_core::{Engine, ParseOptions};

    #[test]
    fn render_capability_ids_and_error_accessor_are_stable() {
        for (capability, stable_id) in [
            (RenderCapability::LayoutCytoscape, "layout-cytoscape"),
            (RenderCapability::LayoutElk, "layout-elk"),
            (RenderCapability::Math, "math"),
        ] {
            assert_eq!(capability.id(), stable_id);
            assert_eq!(capability.to_string(), stable_id);

            let error = Error::MissingCapability {
                capability,
                diagram_type: "contract-test".to_string(),
            };
            assert_eq!(error.missing_capability(), Some(capability));
            assert_eq!(
                error.to_string(),
                format!(
                    "render session lacks capability `{stable_id}` required by diagram `contract-test`"
                )
            );
        }
    }

    #[test]
    fn render_capability_policy_is_an_explicit_allow_mask() {
        let denied = RenderCapabilityPolicy::deny_all();
        for capability in [
            RenderCapability::LayoutCytoscape,
            RenderCapability::LayoutElk,
            RenderCapability::Math,
        ] {
            assert!(!denied.allows(capability));
            assert!(
                RenderCapabilityPolicy::unrestricted().allows(capability),
                "unrestricted policy denied {}",
                capability.id()
            );
        }

        let math_only = denied.with_allowed(RenderCapability::Math);
        assert!(math_only.allows(RenderCapability::Math));
        assert!(!math_only.allows(RenderCapability::LayoutCytoscape));
        assert!(!math_only.allows(RenderCapability::LayoutElk));
    }

    #[cfg(feature = "layout-elk")]
    fn flowchart_layout(
        parsed: &ParsedDiagramRender,
        options: &LayoutOptions,
        session: &RenderSession,
    ) -> model::FlowchartLayout {
        let RenderSemanticModel::Flowchart(model) = parsed.model() else {
            panic!("expected flowchart render model");
        };
        layout_flowchart_typed_by_engine(
            model,
            &parsed.metadata().effective_config,
            &LayoutExecution::new(options, session),
        )
        .expect("flowchart layout")
    }

    #[cfg(feature = "layout-elk")]
    fn class_layout(
        parsed: &ParsedDiagramRender,
        options: &LayoutOptions,
        session: &RenderSession,
    ) -> model::ClassDiagramLayout {
        let RenderSemanticModel::Class(model) = parsed.model() else {
            panic!("expected class render model");
        };
        layout_class_typed_by_engine(
            model,
            &parsed.metadata().effective_config,
            &LayoutExecution::new(options, session),
        )
        .expect("class layout")
    }

    fn render_source(
        source: &str,
        layout_options: &LayoutOptions,
        svg_options: &crate::svg::SvgRenderOptions,
    ) -> String {
        let parsed = Engine::new()
            .parse_diagram_for_render_model_sync(source, ParseOptions::strict())
            .expect("parse")
            .expect("diagram");
        let session = crate::environment::RenderEnvironment::deterministic()
            .begin_session()
            .unwrap();
        crate::family::prepare(parsed, layout_options, session)
            .expect("prepare")
            .render_svg(svg_options, &crate::svg::SvgDebugOptions::default())
            .expect("render")
            .svg()
            .to_owned()
    }

    #[cfg(feature = "layout-elk")]
    #[test]
    fn elk_operation_seed_is_captured_once_per_render_operation() {
        fn capture(seed: u64) -> merman_layout_elk::ElkOperationSeed {
            let session = crate::environment::RenderEnvironment::deterministic()
                .with_runtime_policy(
                    merman_core::runtime::RuntimePolicy::deterministic().with_fixed_seed(seed),
                )
                .begin_session()
                .expect("render session");
            LayoutExecution::new(&LayoutOptions::default(), &session).elk_operation_seed()
        }

        assert_eq!(capture(17), capture(17));
        assert_ne!(capture(17), capture(18));
    }

    #[cfg(feature = "layout-elk")]
    #[test]
    fn render_model_dispatch_accepts_diagram_type_aliases() {
        let session = crate::environment::RenderEnvironment::deterministic()
            .begin_session()
            .unwrap();
        let parsed = Engine::new()
            .parse_diagram_for_render_model_with_type_sync(
                "flowchart-elk",
                "flowchart-elk TD\nA-->B;",
                ParseOptions::strict(),
            )
            .unwrap()
            .unwrap();

        let artifact = crate::family::prepare(parsed, &LayoutOptions::default(), session).unwrap();
        assert_eq!(
            artifact.family_kind(),
            crate::family::RenderFamilyKind::Flowchart
        );
    }

    #[cfg(feature = "layout-elk")]
    #[test]
    fn render_model_dispatch_uses_elk_for_scoped_flowchart_layout() {
        let session = crate::environment::RenderEnvironment::deterministic()
            .begin_session()
            .unwrap();
        let parsed = Engine::new()
            .parse_diagram_for_render_model_sync(
                r#"---
config:
  flowchart:
    layout: elk
---
flowchart TD
A-->B
"#,
                ParseOptions::strict(),
            )
            .unwrap()
            .unwrap();

        assert_eq!(parsed.metadata().diagram_type, "flowchart-v2");
        let layout = flowchart_layout(&parsed, &LayoutOptions::default(), &session);
        assert!(layout.uses_elk_adapter_dom);
        let a = layout.nodes.iter().find(|node| node.id == "A").unwrap();
        let b = layout.nodes.iter().find(|node| node.id == "B").unwrap();
        assert!(b.y > a.y);
    }

    #[cfg(feature = "layout-elk")]
    #[test]
    fn render_model_dispatch_rejects_flowchart_over_node_resource_limit() {
        let parsed = Engine::new()
            .parse_diagram_for_render_model_with_type_sync(
                "flowchart-elk",
                "flowchart-elk TD\nA-->B;",
                ParseOptions::strict(),
            )
            .unwrap()
            .unwrap();
        let options = LayoutOptions::default();
        let session = crate::environment::RenderEnvironment::deterministic()
            .with_resource_policy(
                RenderResourcePolicy::unbounded_for_trusted_input()
                    .with_limit(ResourceLimitId::MaxModelItems, 1)
                    .unwrap(),
            )
            .begin_session()
            .unwrap();

        let err = match crate::family::prepare(parsed, &options, session) {
            Err(error) => error,
            Ok(_) => panic!("expected resource limit error"),
        };

        let Error::ResourceLimitExceeded(limit) = err else {
            panic!("expected resource limit error");
        };
        assert_eq!(limit.phase, ResourceLimitPhase::LayoutModel);
        assert_eq!(limit.limit, "max_model_items");
    }

    #[test]
    fn render_model_dispatch_rejects_flowchart_dagre_work_during_its_first_owner_phase() {
        let parsed = Engine::new()
            .parse_diagram_for_render_model_sync(
                "flowchart TD\nsubgraph Cluster\nA\nend\nA-->B",
                ParseOptions::strict(),
            )
            .unwrap()
            .unwrap();
        let session = crate::environment::RenderEnvironment::deterministic()
            .with_resource_policy(
                RenderResourcePolicy::unbounded_for_trusted_input()
                    .with_limit(ResourceLimitId::MaxLayoutWorkUnits, 1)
                    .unwrap(),
            )
            .begin_session()
            .unwrap();

        let err = match crate::family::prepare(parsed, &LayoutOptions::default(), session) {
            Err(error) => error,
            Ok(_) => panic!("expected flowchart layout work resource limit"),
        };

        let Error::ResourceLimitExceeded(limit) = err else {
            panic!("expected resource limit error");
        };
        assert_eq!(limit.phase, ResourceLimitPhase::LayoutModel);
        assert_eq!(limit.limit, "max_layout_work_units");
        assert!(limit.actual > 1);
    }

    fn assert_class_layout_work_limit(source: &str) {
        let parsed = Engine::new()
            .parse_diagram_for_render_model_sync(source, ParseOptions::strict())
            .unwrap()
            .expect("class source should produce a render model");
        let session = crate::environment::RenderEnvironment::deterministic()
            .with_resource_policy(
                RenderResourcePolicy::unbounded_for_trusted_input()
                    .with_limit(ResourceLimitId::MaxLayoutWorkUnits, 1)
                    .unwrap(),
            )
            .begin_session()
            .unwrap();

        let error = match crate::family::prepare(parsed, &LayoutOptions::default(), session) {
            Err(error) => error,
            Ok(_) => panic!("Class layout unexpectedly bypassed the work budget"),
        };
        let Error::ResourceLimitExceeded(limit) = error else {
            panic!("expected Class layout work resource limit error");
        };
        assert_eq!(limit.phase, ResourceLimitPhase::LayoutModel);
        assert_eq!(limit.limit, "max_layout_work_units");
        assert!(limit.actual > 1);
        assert_eq!(limit.max, 1);
    }

    #[test]
    fn dagre_class_layout_honors_the_public_work_budget() {
        assert_class_layout_work_limit("classDiagram\nA --> B\nA --> C\nA --> D\nB --> C\nC --> D");
    }

    #[cfg(feature = "layout-elk")]
    #[test]
    fn elk_class_layout_honors_the_public_work_budget() {
        assert_class_layout_work_limit(
            r#"---
config:
  layout: elk
---
classDiagram
A --> B
A --> C
A --> D
B --> C
C --> D"#,
        );
    }

    #[cfg(feature = "layout-elk")]
    #[test]
    fn render_model_dispatch_uses_elk_for_class_layout_config() {
        let session = crate::environment::RenderEnvironment::deterministic()
            .begin_session()
            .unwrap();
        let parsed = Engine::new()
            .parse_diagram_for_render_model_sync(
                r#"---
config:
  layout: elk
---
classDiagram
direction LR
Animal <|-- Duck
"#,
                ParseOptions::strict(),
            )
            .unwrap()
            .unwrap();

        assert_eq!(parsed.metadata().diagram_type, "classDiagram");
        let layout = class_layout(&parsed, &LayoutOptions::default(), &session);
        let animal = layout
            .nodes
            .iter()
            .find(|node| node.id == "Animal")
            .unwrap();
        let duck = layout.nodes.iter().find(|node| node.id == "Duck").unwrap();
        assert!(
            duck.x > animal.x,
            "ELK LR class layout should place Duck to the right of Animal; Animal={}, Duck={}",
            animal.x,
            duck.x
        );
    }

    #[cfg(feature = "layout-elk")]
    #[test]
    fn render_model_dispatch_rejects_class_over_node_resource_limit() {
        let parsed = Engine::new()
            .parse_diagram_for_render_model_sync(
                "classDiagram\nAnimal <|-- Duck",
                ParseOptions::strict(),
            )
            .unwrap()
            .unwrap();
        let options = LayoutOptions::default();
        let session = crate::environment::RenderEnvironment::deterministic()
            .with_resource_policy(
                RenderResourcePolicy::unbounded_for_trusted_input()
                    .with_limit(ResourceLimitId::MaxModelItems, 1)
                    .unwrap(),
            )
            .begin_session()
            .unwrap();

        let err = match crate::family::prepare(parsed, &options, session) {
            Err(error) => error,
            Ok(_) => panic!("expected resource limit error"),
        };

        let Error::ResourceLimitExceeded(limit) = err else {
            panic!("expected resource limit error");
        };
        assert_eq!(limit.phase, ResourceLimitPhase::LayoutModel);
        assert_eq!(limit.limit, "max_model_items");
    }

    #[cfg(feature = "layout-elk")]
    #[test]
    fn typed_dispatch_rejects_flowchart_over_edge_resource_limit() {
        let parsed = Engine::new()
            .parse_diagram_for_render_model_sync(
                "flowchart TD\nA-->B\nB-->C\nC-->D",
                ParseOptions::strict(),
            )
            .unwrap()
            .unwrap();
        let options = LayoutOptions::default();
        let session = crate::environment::RenderEnvironment::deterministic()
            .with_resource_policy(
                RenderResourcePolicy::unbounded_for_trusted_input()
                    .with_limit(ResourceLimitId::MaxModelItems, 2)
                    .unwrap(),
            )
            .begin_session()
            .unwrap();

        let err = match crate::family::prepare(parsed, &options, session) {
            Err(error) => error,
            Ok(_) => panic!("expected resource limit error"),
        };

        let Error::ResourceLimitExceeded(limit) = err else {
            panic!("expected resource limit error");
        };
        assert_eq!(limit.phase, ResourceLimitPhase::LayoutModel);
        assert_eq!(limit.limit, "max_model_items");
    }

    #[cfg(feature = "layout-elk")]
    #[test]
    fn typed_dispatch_rejects_class_over_edge_resource_limit() {
        let parsed = Engine::new()
            .parse_diagram_for_render_model_sync(
                "classDiagram\nAnimal <|-- Duck\nDuck <|-- Mallard",
                ParseOptions::strict(),
            )
            .unwrap()
            .unwrap();
        let options = LayoutOptions::default();
        let session = crate::environment::RenderEnvironment::deterministic()
            .with_resource_policy(
                RenderResourcePolicy::unbounded_for_trusted_input()
                    .with_limit(ResourceLimitId::MaxModelItems, 1)
                    .unwrap(),
            )
            .begin_session()
            .unwrap();

        let err = match crate::family::prepare(parsed, &options, session) {
            Err(error) => error,
            Ok(_) => panic!("expected resource limit error"),
        };

        let Error::ResourceLimitExceeded(limit) = err else {
            panic!("expected resource limit error");
        };
        assert_eq!(limit.phase, ResourceLimitPhase::LayoutModel);
        assert_eq!(limit.limit, "max_model_items");
    }

    #[cfg(feature = "layout-elk")]
    #[test]
    fn canonical_svg_preserves_flowchart_elk_roledescription() {
        let svg = render_source(
            "flowchart-elk TD\nA-->B;",
            &LayoutOptions::default(),
            &crate::svg::SvgRenderOptions {
                diagram_id: Some("elk-smoke".to_string()),
                ..Default::default()
            },
        );

        assert!(svg.contains(r#"aria-roledescription="flowchart-elk""#));
        assert!(svg.contains("elk-smoke_flowchart-elk-pointEnd"));
        assert!(!svg.contains(r#"aria-roledescription="flowchart-v2""#));
        assert!(svg.contains(r#"<g class="root""#));

        let marker_pos = svg
            .find(r#"<g><marker id="elk-smoke_flowchart-elk-pointEnd""#)
            .expect("ELK marker group");
        let root_pos = svg.find(r#"<g class="root">"#).expect("ELK root group");
        let clusters_pos = svg
            .find(r#"<g class="clusters"/>"#)
            .expect("ELK clusters group");
        let edges_pos = svg
            .find(r#"<g class="edges edgePaths">"#)
            .expect("ELK edge paths group");
        let labels_pos = svg
            .find(r#"<g class="edgeLabels">"#)
            .expect("ELK edge labels group");
        let nodes_pos = svg.find(r#"<g class="nodes">"#).expect("ELK nodes group");
        let defs_pos = svg
            .find(r#"<defs><filter id="elk-smoke-drop-shadow""#)
            .expect("ELK shadow defs");

        assert!(marker_pos < root_pos);
        assert!(root_pos < clusters_pos);
        assert!(clusters_pos < edges_pos);
        assert!(edges_pos < labels_pos);
        assert!(labels_pos < nodes_pos);
        assert!(nodes_pos < defs_pos);
    }

    #[cfg(feature = "layout-elk")]
    #[test]
    fn canonical_svg_uses_elk_adapter_dom_for_flowchart_layout_elk() {
        let svg = render_source(
            r#"---
config:
  layout: elk
---
flowchart LR
A{A} --> B & C
"#,
            &LayoutOptions::default(),
            &crate::svg::SvgRenderOptions {
                diagram_id: Some("layout-elk-smoke".to_string()),
                ..Default::default()
            },
        );

        assert!(svg.contains(r#"aria-roledescription="flowchart-v2""#));
        assert!(svg.contains("layout-elk-smoke_flowchart-v2-pointEnd"));
        assert!(svg.contains(r#"<g class="root""#));

        let marker_pos = svg
            .find(r#"<g><marker id="layout-elk-smoke_flowchart-v2-pointEnd""#)
            .expect("ELK marker group");
        let root_pos = svg.find(r#"<g class="root">"#).expect("ELK root group");
        let clusters_pos = svg
            .find(r#"<g class="clusters"/>"#)
            .expect("ELK clusters group");
        let edges_pos = svg
            .find(r#"<g class="edges edgePaths">"#)
            .expect("ELK edge paths group");
        let labels_pos = svg
            .find(r#"<g class="edgeLabels">"#)
            .expect("ELK edge labels group");
        let nodes_pos = svg.find(r#"<g class="nodes">"#).expect("ELK nodes group");
        let defs_pos = svg
            .find(r#"<defs><filter id="layout-elk-smoke-drop-shadow""#)
            .expect("ELK shadow defs");

        assert!(marker_pos < root_pos);
        assert!(root_pos < clusters_pos);
        assert!(clusters_pos < edges_pos);
        assert!(edges_pos < labels_pos);
        assert!(labels_pos < nodes_pos);
        assert!(nodes_pos < defs_pos);
    }

    #[cfg(feature = "layout-elk")]
    #[test]
    fn canonical_svg_uses_right_angle_edges_for_flowchart_elk() {
        let svg = render_source(
            "flowchart-elk LR\nA --> B\nA --> C",
            &LayoutOptions::default(),
            &crate::svg::SvgRenderOptions::default(),
        );

        let path = edge_path_chunk(&svg, "L_A_B_0");
        let d = edge_path_d(path);
        assert!(
            d.contains('L') && !d.contains('C'),
            "expected ELK edges to use right-angle paths without smooth curves by default: {d}"
        );
    }

    #[cfg(feature = "layout-elk")]
    #[test]
    fn canonical_svg_keeps_source_ported_elk_rect_edge_boundary_points() {
        let svg = render_source(
            r#"---
config:
  htmlLabels: true
  flowchart:
    htmlLabels: true
  securityLevel: loose
---
flowchart-elk LR
id1(Start)-->id2(Stop)
"#,
            &LayoutOptions::default(),
            &crate::svg::SvgRenderOptions::default(),
        );

        let path = edge_path_chunk(&svg, "L_id1_id2_0");
        let d = edge_path_d(path);
        assert!(
            !d.contains('Q'),
            "straight ELK roundedRect edge should not gain a rounded corner: {d}"
        );
        let points = edge_data_points(path);
        assert_eq!(
            points.len(),
            2,
            "unexpected ELK edge data-points: {points:?}"
        );
        assert_eq!(points[0].1, points[1].1);
        assert!(
            points[0].0 < points[1].0,
            "ELK edge must travel left to right: {points:?}"
        );
        assert!(
            (points[1].0 - points[0].0 - 40.0).abs() < 1.0e-9,
            "ELK straight edge must retain the source-backed node span: {points:?}"
        );
    }

    #[cfg(feature = "layout-elk")]
    #[test]
    fn canonical_svg_keeps_source_ported_elk_self_loop_edges() {
        let svg = render_source(
            "flowchart-elk TD\nA --> A",
            &LayoutOptions::default(),
            &crate::svg::SvgRenderOptions::default(),
        );

        let path = edge_path_chunk(&svg, "L_A_A_0");
        let d = edge_path_d(path);
        assert!(
            d.contains('Q'),
            "ELK self-loop path should be rendered from the source-backed edge: {d}"
        );
        let points = edge_data_points(path);
        assert_eq!(
            points.len(),
            4,
            "unexpected ELK self-loop data-points: {points:?}"
        );
        assert!(
            !svg.contains("A---A---1") && !svg.contains("cyclic-special"),
            "ELK renderer must not reuse Dagre self-loop helper nodes: {svg}"
        );
        assert_eq!(edge_attr_value(path, "data-id"), "L_A_A_0");
    }

    #[test]
    fn graph_layout_unregistered_loader_falls_back_to_dagre() {
        let mut requests = vec!["unknown", "ELK", "elk.layered", "elk.radial"];
        if !cfg!(feature = "layout-elk") {
            requests.extend([
                "elk",
                "elk.stress",
                "elk.force",
                "elk.mrtree",
                "elk.sporeOverlap",
                "elk.box",
                "elk.rectpacking",
            ]);
        }
        for body in [
            "flowchart TD\nA[Alpha] --> B[Beta]\nB --> C[Gamma]",
            "classDiagram\nAnimal <|-- Duck",
            "erDiagram\nCUSTOMER ||--o{ ORDER : places",
        ] {
            let render = |layout: &str| {
                let source = format!("---\nconfig:\n  layout: {layout}\n---\n{body}\n");
                let parsed = Engine::new()
                    .parse_diagram_for_render_model_sync(&source, ParseOptions::strict())
                    .unwrap()
                    .unwrap();
                let session = crate::environment::RenderEnvironment::deterministic()
                    .begin_session()
                    .unwrap();
                let plan = crate::family::plan_render(&parsed, &session).unwrap();
                assert!(
                    !plan
                        .required_capabilities()
                        .contains(&RenderCapability::LayoutElk)
                );
                assert!(plan.missing_capabilities().is_empty());
                let artifact = crate::family::prepare(parsed, &LayoutOptions::default(), session)
                    .expect("unregistered loader falls back before capability admission");
                let geometry = artifact.layout_json().unwrap()["layout"].clone();
                let svg = artifact
                    .render_svg(
                        &crate::svg::SvgRenderOptions {
                            diagram_id: Some("fallback-test".into()),
                            ..Default::default()
                        },
                        &crate::svg::SvgDebugOptions::default(),
                    )
                    .unwrap()
                    .svg()
                    .to_owned();
                (geometry, svg)
            };
            let expected = render("dagre");
            for request in &requests {
                assert_eq!(render(request), expected, "{request}: {body}");
            }
        }
    }

    #[cfg(not(feature = "layout-elk"))]
    #[test]
    fn flowchart_elk_alias_uses_dagre_geometry_when_loader_is_absent() {
        let parsed = Engine::new()
            .parse_diagram_for_render_model_with_type_sync(
                "flowchart-elk",
                "flowchart-elk TD\nA-->B;",
                ParseOptions::strict(),
            )
            .unwrap()
            .unwrap();
        let session = crate::environment::RenderEnvironment::deterministic()
            .begin_session()
            .unwrap();
        let artifact = crate::family::prepare(parsed, &LayoutOptions::default(), session).unwrap();
        let geometry = artifact.layout_json().unwrap()["layout"].clone();
        let dagre_parsed = Engine::new()
            .parse_diagram_for_render_model_sync(
                "---\nconfig:\n  layout: dagre\n---\nflowchart TD\nA-->B;",
                ParseOptions::strict(),
            )
            .unwrap()
            .unwrap();
        let dagre_session = crate::environment::RenderEnvironment::deterministic()
            .begin_session()
            .unwrap();
        let dagre_artifact =
            crate::family::prepare(dagre_parsed, &LayoutOptions::default(), dagre_session).unwrap();
        assert_eq!(geometry, dagre_artifact.layout_json().unwrap()["layout"]);
        let svg = artifact
            .render_svg(
                &crate::svg::SvgRenderOptions::default(),
                &crate::svg::SvgDebugOptions::default(),
            )
            .unwrap()
            .svg()
            .to_owned();
        assert!(svg.contains("edgePath"), "{svg}");
    }

    #[test]
    fn registered_cose_graph_failure_is_not_replaced_by_dagre() {
        for body in [
            "flowchart LR\nA-->B\nA-->C",
            "agentflow-beta\nA-->B",
            "usecase-beta\nactor A\nA --> B",
            "classDiagram\nA --> B",
            "stateDiagram-v2\nA --> B",
            "erDiagram\nA ||--o{ B : relates",
            "requirementDiagram\nrequirement r {\nid: 1\ntext: Demo\nrisk: low\nverifymethod: test\n}",
        ] {
            for layout in ["cose-bilkent", "unknown-layout-probe"] {
                let source = format!("---\nconfig:\n  layout: {layout}\n---\n{body}\n");
                let parsed = Engine::new()
                    .parse_diagram_for_render_model_sync(&source, ParseOptions::strict())
                    .unwrap()
                    .unwrap();
                let session = crate::environment::RenderEnvironment::deterministic()
                    .begin_session()
                    .unwrap();
                let result = crate::family::prepare(parsed, &LayoutOptions::default(), session);
                if layout == "cose-bilkent" && cfg!(feature = "layout-cytoscape") {
                    assert!(
                        matches!(result, Err(Error::InvalidModel { message }) if message.contains("Root node is required")),
                        "{body}"
                    );
                } else {
                    result.unwrap_or_else(|error| panic!("{body}: {layout}: {error}"));
                }
            }
        }
    }

    #[cfg(feature = "layout-cytoscape")]
    #[test]
    fn registered_cose_host_denial_precedes_missing_root_failure() {
        let source = "---\nconfig:\n  layout: cose-bilkent\n---\nusecase-beta\nactor A\nA --> B\n";
        let parsed = Engine::new()
            .parse_diagram_for_render_model_sync(source, ParseOptions::strict())
            .unwrap()
            .unwrap();
        let session = crate::environment::RenderEnvironment::deterministic()
            .with_capability_policy(RenderCapabilityPolicy::deny_all())
            .begin_session()
            .unwrap();
        let plan = crate::family::plan_render(&parsed, &session).unwrap();
        assert_eq!(
            plan.missing_capabilities(),
            &[RenderCapability::LayoutCytoscape]
        );
        assert!(matches!(
            crate::family::prepare(parsed, &LayoutOptions::default(), session),
            Err(Error::MissingCapability {
                capability: RenderCapability::LayoutCytoscape,
                ..
            })
        ));
    }

    #[cfg(feature = "layout-elk")]
    #[test]
    fn registered_elk_denied_by_host_policy_does_not_fall_back() {
        for body in [
            "flowchart TD\nA-->B",
            "classDiagram\nAnimal <|-- Duck",
            "erDiagram\nCUSTOMER ||--o{ ORDER : places",
        ] {
            for layout in [
                "elk",
                "elk.stress",
                "elk.force",
                "elk.mrtree",
                "elk.sporeOverlap",
                "elk.box",
                "elk.rectpacking",
            ] {
                let source = format!("---\nconfig:\n  layout: {layout}\n---\n{body}\n");
                let parsed = Engine::new()
                    .parse_diagram_for_render_model_sync(&source, ParseOptions::strict())
                    .unwrap()
                    .unwrap();
                let session = crate::environment::RenderEnvironment::deterministic()
                    .with_capability_policy(RenderCapabilityPolicy::deny_all())
                    .begin_session()
                    .unwrap();
                let plan = crate::family::plan_render(&parsed, &session).unwrap();
                assert_eq!(plan.missing_capabilities(), &[RenderCapability::LayoutElk]);
                assert!(matches!(
                    crate::family::prepare(parsed, &LayoutOptions::default(), session),
                    Err(Error::MissingCapability {
                        capability: RenderCapability::LayoutElk,
                        ..
                    })
                ));
            }
        }
    }

    #[test]
    fn render_model_dispatch_renders_cynefin_svg() {
        let source = r#"cynefin-beta
  title Team Practices
  accTitle: Cynefin map
  accDescr: Practice movement
  complex
    "Pair programming"
  complicated
    "Architecture review"
  complex --> complicated : "Pattern emerges"
"#;
        let svg = render_source(
            source,
            &LayoutOptions::default(),
            &crate::svg::SvgRenderOptions {
                diagram_id: Some("cynefin-test".to_string()),
                ..Default::default()
            },
        );

        assert!(svg.contains(r#"aria-roledescription="cynefin""#));
        assert!(svg.contains(r#"<g class="cynefin-backgrounds">"#));
        assert!(svg.contains(r#"class="cynefinDomain""#));
        assert!(svg.contains(r#"class="cynefinBoundary""#));
        assert!(svg.contains(r#"class="cynefinCliff""#));
        assert!(svg.contains(r#"class="cynefinItem""#));
        assert!(svg.contains("Pair programming"));
        assert!(svg.contains(r#"class="cynefinArrowLine""#));
        assert!(svg.contains("Pattern emerges"));
        assert!(svg.contains(r#"<title id="chart-title-cynefin-test">Cynefin map</title>"#));
        assert!(svg.contains(r#"<desc id="chart-desc-cynefin-test">Practice movement</desc>"#));
        assert!(svg.contains("#cynefin-test .cynefinDomain{stroke:none;}"));
        assert_eq!(svg.matches("<title").count(), 2, "{svg}");
        assert_eq!(svg.matches("<desc").count(), 2, "{svg}");

        let scoped_title = svg
            .find(r#"<title id="chart-title-cynefin-test">"#)
            .expect("scoped accessibility title");
        let scoped_descr = svg
            .find(r#"<desc id="chart-desc-cynefin-test">"#)
            .expect("scoped accessibility description");
        let style = svg.find("<style>").expect("style");
        let framework_group = svg.find("<g/>").expect("Mermaid framework group");
        let renderer_title = svg
            .find("<title>Cynefin map</title>")
            .expect("renderer accessibility title");
        let renderer_descr = svg
            .find("<desc>Practice movement</desc>")
            .expect("renderer accessibility description");
        let root_group = svg
            .find(r#"<g transform="translate("#)
            .expect("cynefin root group");
        let defs = svg.find("<defs>").expect("transition marker defs");

        assert!(scoped_title < scoped_descr, "{svg}");
        assert!(scoped_descr < style, "{svg}");
        assert!(style < framework_group, "{svg}");
        assert!(framework_group < renderer_title, "{svg}");
        assert!(renderer_title < renderer_descr, "{svg}");
        assert!(renderer_descr < root_group, "{svg}");
        assert!(root_group < defs, "{svg}");
    }

    #[test]
    fn render_model_dispatch_keeps_whitespace_cynefin_transition_labels() {
        let source = r#"cynefin-beta
  complex
  complicated
  complex --> complicated : "   "
"#;
        let svg = render_source(
            source,
            &LayoutOptions::default(),
            &crate::svg::SvgRenderOptions {
                diagram_id: Some("cynefin-whitespace".to_string()),
                ..Default::default()
            },
        );

        assert!(
            svg.contains(r#"class="cynefinArrowLabel""#),
            "a whitespace-only label is truthy in JavaScript and must emit a text node: {svg}"
        );
    }

    #[test]
    fn render_model_dispatch_renders_railroad_svg() {
        let source = r#"railroad-beta
accTitle: Railroad grammar
accDescr: Expression grammar
expr = sequence(nonterminal("term"), optional(special("guard")), zeroOrMore(terminal("+"))) ;
"#;
        let svg = render_source(
            source,
            &LayoutOptions::default(),
            &crate::svg::SvgRenderOptions {
                diagram_id: Some("railroad-test".to_string()),
                ..Default::default()
            },
        );

        assert!(svg.contains(r#"aria-roledescription="railroad""#));
        assert!(svg.contains(r#"class="railroad-diagram""#));
        assert!(svg.contains(r#"class="railroad-rule""#));
        assert!(svg.contains(r#"class="railroad-rule-name""#));
        assert!(svg.contains(r#"class="railroad-nonterminal""#));
        assert!(svg.contains(r#"class="railroad-special""#));
        assert!(svg.contains(r#"class="railroad-terminal""#));
        assert!(svg.contains(r#"class="railroad-line""#));
        assert!(svg.contains("term"));
        assert!(svg.contains("? guard ?"));
        assert!(svg.contains("+"));
        assert!(svg.contains(r#"<title id="chart-title-railroad-test">Railroad grammar</title>"#));
        assert!(svg.contains(r#"<desc id="chart-desc-railroad-test">Expression grammar</desc>"#));
        assert!(
            svg.contains("</style><g/><g class=\"railroad-rule\""),
            "{svg}"
        );
    }

    #[cfg(feature = "layout-elk")]
    fn edge_path_chunk<'a>(svg: &'a str, edge_id: &str) -> &'a str {
        let id_attr = format!(r#"id="merman-{edge_id}""#);
        let id_start = svg.find(&id_attr).expect("edge id");
        let path_start = svg[..id_start].rfind("<path ").expect("edge path start");
        let path_end = svg[id_start..].find("/>").expect("edge path end") + id_start;
        &svg[path_start..path_end]
    }

    #[cfg(feature = "layout-elk")]
    fn edge_path_d(path: &str) -> &str {
        let d_start = path.find(r#"d=""#).expect("edge path d") + r#"d=""#.len();
        let d_end = path[d_start..].find('"').expect("edge path d end") + d_start;
        &path[d_start..d_end]
    }

    #[cfg(feature = "layout-elk")]
    fn edge_attr_value<'a>(path: &'a str, attr: &str) -> &'a str {
        let needle = format!(r#"{attr}=""#);
        let start = path.find(&needle).expect("edge attr") + needle.len();
        let end = path[start..].find('"').expect("edge attr end") + start;
        &path[start..end]
    }

    #[cfg(feature = "layout-elk")]
    fn edge_data_points(path: &str) -> Vec<(f64, f64)> {
        use base64::Engine as _;

        let b64 = edge_attr_value(path, "data-points");
        let bytes = base64::engine::general_purpose::STANDARD
            .decode(b64.as_bytes())
            .expect("data-points base64");
        let json: serde_json::Value =
            serde_json::from_slice(&bytes).expect("data-points JSON payload");
        json.as_array()
            .expect("data-points array")
            .iter()
            .map(|point| {
                (
                    point.get("x").and_then(serde_json::Value::as_f64).unwrap(),
                    point.get("y").and_then(serde_json::Value::as_f64).unwrap(),
                )
            })
            .collect()
    }
}

#[cfg(feature = "diagram-class")]
pub use resources::ClassComplexity;

#[cfg(any(
    feature = "diagram-flowchart",
    feature = "diagram-swimlane",
    feature = "diagram-agentflow"
))]
pub use resources::FlowchartComplexity;

#[cfg(feature = "diagram-mindmap")]
pub use resources::MindmapComplexity;

#[cfg(feature = "diagram-sequence")]
pub use resources::SequenceComplexity;

#[cfg(feature = "diagram-zenuml")]
pub use resources::ZenumlComplexity;
