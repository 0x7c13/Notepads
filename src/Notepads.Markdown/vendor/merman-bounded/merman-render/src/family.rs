use crate::environment::RenderSession;
#[cfg(feature = "diagram-sequence")]
use crate::environment::TextMeasurementPhase;
use crate::model::*;
#[cfg(any(
    feature = "diagram-flowchart",
    feature = "diagram-swimlane",
    feature = "diagram-agentflow"
))]
use crate::presentation::FlowchartPresentationPolicy;
use crate::presentation::{
    PresentationAspectResolution, PresentationProfile, PresentationRenderPolicy,
};
use crate::resources::ResourceLimitPhase;
use crate::svg::{
    FlowchartEdgeTraceCollector, ResvgCompatibleSvg, SvgDebugOptions, SvgPipeline,
    SvgPostprocessExecution, SvgPostprocessMetadata, SvgRenderOptions,
};
#[cfg(feature = "diagram-wardley")]
use crate::wardley::WardleyDiagramLayout;
use crate::{Error, LayoutExecution, LayoutOptions, RenderCapability, Result};
use merman_core::OperationPhase;
use merman_core::diagrams;
#[cfg(feature = "diagram-class")]
use merman_core::models::class_diagram::ClassDiagram;
use merman_core::{
    BuiltinRenderSemantic, ManagedSemanticJson, ParseMetadata, ParsedDiagramRender,
    RenderSemanticModel,
};
use std::fmt;
use std::sync::OnceLock;

/// Stable identity for a built-in typed render family.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum RenderFamilyKind {
    Error,
    Mindmap,
    State,
    Sequence,
    Zenuml,
    Flowchart,
    Swimlane,
    Architecture,
    Class,
    C4,
    Cynefin,
    Wardley,
    Railroad,
    Kanban,
    Gantt,
    Pie,
    Packet,
    Timeline,
    Journey,
    Requirement,
    Sankey,
    Radar,
    Info,
    Treemap,
    Block,
    Er,
    QuadrantChart,
    XyChart,
    GitGraph,
    TreeView,
    Ishikawa,
    EventModeling,
    Venn,
    Usecase,
    Agentflow,
}

impl RenderFamilyKind {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Error => "error",
            Self::Mindmap => "mindmap",
            Self::State => "state",
            Self::Sequence => "sequence",
            Self::Zenuml => "zenuml",
            Self::Flowchart => "flowchart",
            Self::Swimlane => "swimlane",
            Self::Architecture => "architecture",
            Self::Class => "class",
            Self::C4 => "c4",
            Self::Cynefin => "cynefin",
            Self::Wardley => "wardley",
            Self::Railroad => "railroad",
            Self::Kanban => "kanban",
            Self::Gantt => "gantt",
            Self::Pie => "pie",
            Self::Packet => "packet",
            Self::Timeline => "timeline",
            Self::Journey => "journey",
            Self::Requirement => "requirement",
            Self::Sankey => "sankey",
            Self::Radar => "radar",
            Self::Info => "info",
            Self::Treemap => "treemap",
            Self::Block => "block",
            Self::Er => "er",
            Self::QuadrantChart => "quadrantChart",
            Self::XyChart => "xychart",
            Self::GitGraph => "gitGraph",
            Self::TreeView => "treeView",
            Self::Ishikawa => "ishikawa",
            Self::EventModeling => "eventmodeling",
            Self::Venn => "venn",
            Self::Usecase => "usecase",
            Self::Agentflow => "agentflow",
        }
    }
}

/// Capabilities required by one parsed typed render operation before layout starts.
///
/// Requirements come from the canonically paired semantic model and effective Mermaid config;
/// availability comes from the compiled layout backends and the operation's render session.
#[must_use]
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RenderCapabilityPlan {
    diagram_type: String,
    required: Vec<RenderCapability>,
    missing: Vec<RenderCapability>,
    presentation_profile: Option<PresentationProfile>,
    presentation_aspects: Vec<PresentationAspectResolution>,
}

impl RenderCapabilityPlan {
    /// Returns the detected Mermaid diagram type used by render dispatch.
    pub fn diagram_type(&self) -> &str {
        &self.diagram_type
    }

    /// Returns every optional capability this operation requires.
    pub fn required_capabilities(&self) -> &[RenderCapability] {
        &self.required
    }

    /// Returns the required capabilities unavailable in the planned render session.
    pub fn missing_capabilities(&self) -> &[RenderCapability] {
        &self.missing
    }

    /// Returns the selected first-party presentation profile, if any.
    pub const fn presentation_profile(&self) -> Option<PresentationProfile> {
        self.presentation_profile
    }

    /// Returns the selected presentation profile's stable ID, if any.
    pub const fn presentation_profile_id(&self) -> Option<&'static str> {
        match self.presentation_profile {
            Some(profile) => Some(profile.id()),
            None => None,
        }
    }

    /// Returns per-aspect resolution for the selected presentation profile.
    pub fn presentation_aspects(&self) -> &[PresentationAspectResolution] {
        &self.presentation_aspects
    }

    /// Iterates over stable semantic IDs for every required capability.
    pub fn required_capability_ids(&self) -> impl ExactSizeIterator<Item = &'static str> + '_ {
        self.required.iter().copied().map(RenderCapability::id)
    }

    /// Iterates over stable semantic IDs for every missing capability.
    pub fn missing_capability_ids(&self) -> impl ExactSizeIterator<Item = &'static str> + '_ {
        self.missing.iter().copied().map(RenderCapability::id)
    }

    /// Reports whether the planned render session satisfies every requirement.
    pub fn is_ready(&self) -> bool {
        self.missing.is_empty()
    }

    fn ensure_available(&self) -> Result<()> {
        let Some(capability) = self.missing.first().copied() else {
            return Ok(());
        };
        Err(Error::MissingCapability {
            capability,
            diagram_type: self.diagram_type.clone(),
        })
    }
}

impl fmt::Display for RenderFamilyKind {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.as_str())
    }
}

#[derive(Debug)]
pub(crate) struct FamilyPair<S, L> {
    semantic: S,
    layout: L,
}

impl<S, L> FamilyPair<S, L> {
    fn new(semantic: S, layout: L) -> Self {
        Self { semantic, layout }
    }

    pub(crate) fn semantic(&self) -> &S {
        &self.semantic
    }

    pub(crate) fn layout(&self) -> &L {
        &self.layout
    }
}

impl<S: BuiltinRenderSemantic, L> FamilyPair<S, L> {
    fn compatibility_json(
        &self,
        metadata: &ParseMetadata,
    ) -> merman_core::Result<ManagedSemanticJson> {
        self.semantic.compatibility_json(metadata)
    }
}

#[derive(Debug)]
#[cfg(any(
    feature = "diagram-flowchart",
    feature = "diagram-swimlane",
    feature = "diagram-agentflow"
))]
pub(crate) struct FlowchartFamilyArtifact<L> {
    pair: FamilyPair<diagrams::flowchart::FlowchartModel, L>,
    render_context: diagrams::flowchart::FlowchartRenderContext,
    svg_label_sidecar: crate::flowchart::FlowchartSvgLabelSidecar,
    policy: Option<FlowchartPresentationPolicy>,
}

#[cfg(any(
    feature = "diagram-flowchart",
    feature = "diagram-swimlane",
    feature = "diagram-agentflow"
))]
impl<L> FlowchartFamilyArtifact<L> {
    pub(crate) fn pair(&self) -> &FamilyPair<diagrams::flowchart::FlowchartModel, L> {
        &self.pair
    }

    pub(crate) fn render_context(&self) -> &diagrams::flowchart::FlowchartRenderContext {
        &self.render_context
    }

    pub(crate) fn svg_label_sidecar(&self) -> &crate::flowchart::FlowchartSvgLabelSidecar {
        &self.svg_label_sidecar
    }

    pub(crate) const fn policy(&self) -> Option<FlowchartPresentationPolicy> {
        self.policy
    }
}

#[derive(Debug)]
pub(crate) enum BuiltinFamilyArtifact {
    #[cfg(feature = "diagram-agentflow")]
    Agentflow {
        semantic: Box<diagrams::agentflow::AgentflowDiagramRenderModel>,
        flow: Box<FlowchartFamilyArtifact<FlowchartLayout>>,
    },
    Error(Box<FamilyPair<diagrams::error_diagram::ErrorDiagramRenderModel, ErrorDiagramLayout>>),
    #[cfg(feature = "diagram-mindmap")]
    Mindmap(Box<FamilyPair<diagrams::mindmap::MindmapDiagramRenderModel, MindmapDiagramLayout>>),
    #[cfg(feature = "diagram-state")]
    State(Box<FamilyPair<diagrams::state::StateDiagramRenderModel, StateDiagramLayout>>),
    #[cfg(feature = "diagram-sequence")]
    Sequence(
        Box<
            FamilyPair<
                diagrams::sequence::SequenceDiagramRenderModel,
                crate::sequence::SequencePreparedArtifact,
            >,
        >,
    ),
    #[cfg(feature = "diagram-zenuml")]
    Zenuml(
        Box<
            FamilyPair<
                diagrams::zenuml::ZenumlDiagramRenderModel,
                crate::zenuml::ZenumlDiagramLayout,
            >,
        >,
    ),
    #[cfg(any(feature = "diagram-flowchart", feature = "diagram-swimlane"))]
    Flowchart(Box<FlowchartFamilyArtifact<FlowchartLayout>>),
    #[cfg(any(feature = "diagram-flowchart", feature = "diagram-swimlane"))]
    Swimlane(Box<FlowchartFamilyArtifact<SwimlaneLayout>>),
    #[cfg(feature = "layout-cytoscape")]
    #[cfg(feature = "diagram-architecture")]
    Architecture(
        Box<
            FamilyPair<
                diagrams::architecture::ArchitectureDiagramRenderModel,
                ArchitectureDiagramLayout,
            >,
        >,
    ),
    #[cfg(feature = "diagram-class")]
    Class(Box<FamilyPair<ClassDiagram, ClassDiagramLayout>>),
    #[cfg(feature = "diagram-c4")]
    C4(Box<FamilyPair<diagrams::c4::C4DiagramRenderModel, C4DiagramLayout>>),
    #[cfg(feature = "diagram-cynefin")]
    Cynefin(Box<FamilyPair<diagrams::cynefin::CynefinDiagramRenderModel, CynefinDiagramLayout>>),
    #[cfg(feature = "diagram-wardley")]
    Wardley(Box<FamilyPair<diagrams::wardley::WardleyDiagramRenderModel, WardleyDiagramLayout>>),
    #[cfg(feature = "diagram-railroad")]
    Railroad(
        Box<FamilyPair<diagrams::railroad::RailroadDiagramRenderModel, RailroadDiagramLayout>>,
    ),
    #[cfg(feature = "diagram-kanban")]
    Kanban(
        Box<
            FamilyPair<
                diagrams::kanban::KanbanDiagramRenderModel,
                crate::kanban::KanbanPreparedArtifact,
            >,
        >,
    ),
    #[cfg(feature = "diagram-gantt")]
    Gantt(Box<FamilyPair<diagrams::gantt::GanttDiagramRenderModel, GanttDiagramLayout>>),
    #[cfg(feature = "diagram-pie")]
    Pie(Box<FamilyPair<diagrams::pie::PieDiagramRenderModel, PieDiagramLayout>>),
    #[cfg(feature = "diagram-packet")]
    Packet(Box<FamilyPair<diagrams::packet::PacketDiagramRenderModel, PacketDiagramLayout>>),
    #[cfg(feature = "diagram-timeline")]
    Timeline(
        Box<FamilyPair<diagrams::timeline::TimelineDiagramRenderModel, TimelineDiagramLayout>>,
    ),
    #[cfg(feature = "diagram-journey")]
    Journey(Box<FamilyPair<diagrams::journey::JourneyDiagramRenderModel, JourneyDiagramLayout>>),
    #[cfg(feature = "diagram-requirement")]
    Requirement(
        Box<
            FamilyPair<
                diagrams::requirement::RequirementDiagramRenderModel,
                crate::requirement::RequirementPreparedArtifact,
            >,
        >,
    ),
    #[cfg(feature = "diagram-sankey")]
    Sankey(Box<FamilyPair<diagrams::sankey::SankeyDiagramRenderModel, SankeyDiagramLayout>>),
    #[cfg(feature = "diagram-radar")]
    Radar(Box<FamilyPair<diagrams::radar::RadarDiagramRenderModel, RadarDiagramLayout>>),
    #[cfg(feature = "diagram-info")]
    Info(Box<FamilyPair<diagrams::info::InfoDiagramRenderModel, InfoDiagramLayout>>),
    #[cfg(feature = "diagram-treemap")]
    Treemap(Box<FamilyPair<diagrams::treemap::TreemapDiagramRenderModel, TreemapDiagramLayout>>),
    #[cfg(feature = "diagram-block")]
    Block(Box<FamilyPair<diagrams::block::BlockDiagramRenderModel, BlockDiagramLayout>>),
    #[cfg(feature = "diagram-er")]
    Er(Box<FamilyPair<diagrams::er::ErDiagramRenderModel, ErDiagramLayout>>),
    #[cfg(feature = "diagram-quadrant-chart")]
    QuadrantChart(
        Box<
            FamilyPair<
                diagrams::quadrant_chart::QuadrantChartRenderModel,
                QuadrantChartDiagramLayout,
            >,
        >,
    ),
    #[cfg(feature = "diagram-xychart")]
    XyChart(Box<FamilyPair<diagrams::xychart::XyChartDiagramRenderModel, XyChartDiagramLayout>>),
    #[cfg(feature = "diagram-git-graph")]
    GitGraph(Box<FamilyPair<diagrams::git_graph::GitGraphRenderModel, GitGraphDiagramLayout>>),
    #[cfg(feature = "diagram-tree-view")]
    TreeView(
        Box<FamilyPair<diagrams::tree_view::TreeViewDiagramRenderModel, TreeViewDiagramLayout>>,
    ),
    #[cfg(feature = "diagram-ishikawa")]
    Ishikawa(
        Box<FamilyPair<diagrams::ishikawa::IshikawaDiagramRenderModel, IshikawaDiagramLayout>>,
    ),
    #[cfg(feature = "diagram-event-modeling")]
    EventModeling(
        Box<
            FamilyPair<
                diagrams::eventmodeling::EventModelingDiagramRenderModel,
                EventModelingDiagramLayout,
            >,
        >,
    ),
    #[cfg(feature = "diagram-venn")]
    Venn(Box<FamilyPair<diagrams::venn::VennDiagramRenderModel, VennDiagramLayout>>),
    #[cfg(feature = "diagram-usecase")]
    Usecase(
        Box<
            FamilyPair<
                diagrams::usecase::UsecaseDiagramRenderModel,
                crate::usecase::UsecasePreparedArtifact,
            >,
        >,
    ),
}

#[derive(serde::Serialize)]
enum LayoutProjection<'a> {
    #[cfg(feature = "diagram-agentflow")]
    AgentflowDiagram(&'a FlowchartLayout),
    #[cfg(feature = "diagram-block")]
    BlockDiagram(&'a BlockDiagramLayout),
    #[cfg(feature = "diagram-requirement")]
    RequirementDiagram(&'a RequirementDiagramLayout),
    #[cfg(feature = "layout-cytoscape")]
    #[cfg(feature = "diagram-architecture")]
    ArchitectureDiagram(&'a ArchitectureDiagramLayout),
    #[cfg(feature = "diagram-mindmap")]
    MindmapDiagram(&'a MindmapDiagramLayout),
    #[cfg(feature = "diagram-sankey")]
    SankeyDiagram(&'a SankeyDiagramLayout),
    #[cfg(feature = "diagram-radar")]
    RadarDiagram(&'a RadarDiagramLayout),
    #[cfg(feature = "diagram-treemap")]
    TreemapDiagram(&'a TreemapDiagramLayout),
    #[cfg(feature = "diagram-venn")]
    VennDiagram(&'a VennDiagramLayout),
    #[cfg(feature = "diagram-usecase")]
    UsecaseDiagram(&'a crate::usecase::UsecaseDiagramLayout),
    #[cfg(feature = "diagram-xychart")]
    XyChartDiagram(&'a XyChartDiagramLayout),
    #[cfg(feature = "diagram-quadrant-chart")]
    QuadrantChartDiagram(&'a QuadrantChartDiagramLayout),
    #[serde(rename = "FlowchartV2")]
    #[cfg(any(feature = "diagram-flowchart", feature = "diagram-swimlane"))]
    Flowchart(&'a FlowchartLayout),
    #[cfg(any(feature = "diagram-flowchart", feature = "diagram-swimlane"))]
    SwimlaneDiagram(&'a SwimlaneLayout),
    #[serde(rename = "StateDiagramV2")]
    #[cfg(feature = "diagram-state")]
    StateDiagram(&'a StateDiagramLayout),
    #[serde(rename = "ClassDiagramV2")]
    #[cfg(feature = "diagram-class")]
    ClassDiagram(&'a ClassDiagramLayout),
    #[cfg(feature = "diagram-er")]
    ErDiagram(&'a ErDiagramLayout),
    #[cfg(feature = "diagram-sequence")]
    SequenceDiagram(&'a SequenceDiagramLayout),
    #[cfg(feature = "diagram-zenuml")]
    ZenumlDiagram(&'a crate::zenuml::ZenumlDiagramLayout),
    #[cfg(feature = "diagram-info")]
    InfoDiagram(&'a InfoDiagramLayout),
    #[cfg(feature = "diagram-packet")]
    PacketDiagram(&'a PacketDiagramLayout),
    #[cfg(feature = "diagram-timeline")]
    TimelineDiagram(&'a TimelineDiagramLayout),
    #[cfg(feature = "diagram-pie")]
    PieDiagram(&'a PieDiagramLayout),
    #[cfg(feature = "diagram-journey")]
    JourneyDiagram(&'a JourneyDiagramLayout),
    #[cfg(feature = "diagram-kanban")]
    KanbanDiagram(&'a KanbanDiagramLayout),
    #[cfg(feature = "diagram-git-graph")]
    GitGraphDiagram(&'a GitGraphDiagramLayout),
    #[cfg(feature = "diagram-tree-view")]
    TreeViewDiagram(&'a TreeViewDiagramLayout),
    #[cfg(feature = "diagram-ishikawa")]
    IshikawaDiagram(&'a IshikawaDiagramLayout),
    #[cfg(feature = "diagram-event-modeling")]
    EventModelingDiagram(&'a EventModelingDiagramLayout),
    #[cfg(feature = "diagram-cynefin")]
    CynefinDiagram(&'a CynefinDiagramLayout),
    #[cfg(feature = "diagram-wardley")]
    WardleyDiagram(&'a WardleyDiagramLayout),
    #[cfg(feature = "diagram-railroad")]
    RailroadDiagram(&'a RailroadDiagramLayout),
    #[cfg(feature = "diagram-gantt")]
    GanttDiagram(&'a GanttDiagramLayout),
    #[cfg(feature = "diagram-c4")]
    C4Diagram(&'a C4DiagramLayout),
    ErrorDiagram(&'a ErrorDiagramLayout),
}

impl BuiltinFamilyArtifact {
    pub fn kind(&self) -> RenderFamilyKind {
        match self {
            #[cfg(feature = "diagram-agentflow")]
            Self::Agentflow { .. } => RenderFamilyKind::Agentflow,
            Self::Error(_) => RenderFamilyKind::Error,
            #[cfg(feature = "diagram-mindmap")]
            Self::Mindmap(_) => RenderFamilyKind::Mindmap,
            #[cfg(feature = "diagram-state")]
            Self::State(_) => RenderFamilyKind::State,
            #[cfg(feature = "diagram-sequence")]
            Self::Sequence(_) => RenderFamilyKind::Sequence,
            #[cfg(feature = "diagram-zenuml")]
            Self::Zenuml(_) => RenderFamilyKind::Zenuml,
            #[cfg(any(feature = "diagram-flowchart", feature = "diagram-swimlane"))]
            Self::Flowchart(_) => RenderFamilyKind::Flowchart,
            #[cfg(any(feature = "diagram-flowchart", feature = "diagram-swimlane"))]
            Self::Swimlane(_) => RenderFamilyKind::Swimlane,
            #[cfg(feature = "layout-cytoscape")]
            #[cfg(feature = "diagram-architecture")]
            Self::Architecture(_) => RenderFamilyKind::Architecture,
            #[cfg(feature = "diagram-class")]
            Self::Class(_) => RenderFamilyKind::Class,
            #[cfg(feature = "diagram-c4")]
            Self::C4(_) => RenderFamilyKind::C4,
            #[cfg(feature = "diagram-cynefin")]
            Self::Cynefin(_) => RenderFamilyKind::Cynefin,
            #[cfg(feature = "diagram-wardley")]
            Self::Wardley(_) => RenderFamilyKind::Wardley,
            #[cfg(feature = "diagram-railroad")]
            Self::Railroad(_) => RenderFamilyKind::Railroad,
            #[cfg(feature = "diagram-kanban")]
            Self::Kanban(_) => RenderFamilyKind::Kanban,
            #[cfg(feature = "diagram-gantt")]
            Self::Gantt(_) => RenderFamilyKind::Gantt,
            #[cfg(feature = "diagram-pie")]
            Self::Pie(_) => RenderFamilyKind::Pie,
            #[cfg(feature = "diagram-packet")]
            Self::Packet(_) => RenderFamilyKind::Packet,
            #[cfg(feature = "diagram-timeline")]
            Self::Timeline(_) => RenderFamilyKind::Timeline,
            #[cfg(feature = "diagram-journey")]
            Self::Journey(_) => RenderFamilyKind::Journey,
            #[cfg(feature = "diagram-requirement")]
            Self::Requirement(_) => RenderFamilyKind::Requirement,
            #[cfg(feature = "diagram-sankey")]
            Self::Sankey(_) => RenderFamilyKind::Sankey,
            #[cfg(feature = "diagram-radar")]
            Self::Radar(_) => RenderFamilyKind::Radar,
            #[cfg(feature = "diagram-info")]
            Self::Info(_) => RenderFamilyKind::Info,
            #[cfg(feature = "diagram-treemap")]
            Self::Treemap(_) => RenderFamilyKind::Treemap,
            #[cfg(feature = "diagram-block")]
            Self::Block(_) => RenderFamilyKind::Block,
            #[cfg(feature = "diagram-er")]
            Self::Er(_) => RenderFamilyKind::Er,
            #[cfg(feature = "diagram-quadrant-chart")]
            Self::QuadrantChart(_) => RenderFamilyKind::QuadrantChart,
            #[cfg(feature = "diagram-xychart")]
            Self::XyChart(_) => RenderFamilyKind::XyChart,
            #[cfg(feature = "diagram-git-graph")]
            Self::GitGraph(_) => RenderFamilyKind::GitGraph,
            #[cfg(feature = "diagram-tree-view")]
            Self::TreeView(_) => RenderFamilyKind::TreeView,
            #[cfg(feature = "diagram-ishikawa")]
            Self::Ishikawa(_) => RenderFamilyKind::Ishikawa,
            #[cfg(feature = "diagram-event-modeling")]
            Self::EventModeling(_) => RenderFamilyKind::EventModeling,
            #[cfg(feature = "diagram-venn")]
            Self::Venn(_) => RenderFamilyKind::Venn,
            #[cfg(feature = "diagram-usecase")]
            Self::Usecase(_) => RenderFamilyKind::Usecase,
        }
    }

    fn compatibility_json(
        &self,
        metadata: &ParseMetadata,
    ) -> merman_core::Result<ManagedSemanticJson> {
        match self {
            #[cfg(feature = "diagram-agentflow")]
            Self::Agentflow { semantic, .. } => semantic.compatibility_json(metadata),
            Self::Error(pair) => pair.compatibility_json(metadata),
            #[cfg(feature = "diagram-mindmap")]
            Self::Mindmap(pair) => pair.compatibility_json(metadata),
            #[cfg(feature = "diagram-state")]
            Self::State(pair) => pair.compatibility_json(metadata),
            #[cfg(feature = "diagram-sequence")]
            Self::Sequence(pair) => pair.compatibility_json(metadata),
            #[cfg(feature = "diagram-zenuml")]
            Self::Zenuml(pair) => pair.compatibility_json(metadata),
            #[cfg(any(feature = "diagram-flowchart", feature = "diagram-swimlane"))]
            Self::Flowchart(artifact) => artifact.pair.compatibility_json(metadata),
            #[cfg(any(feature = "diagram-flowchart", feature = "diagram-swimlane"))]
            Self::Swimlane(artifact) => artifact.pair.compatibility_json(metadata),
            #[cfg(feature = "layout-cytoscape")]
            #[cfg(feature = "diagram-architecture")]
            Self::Architecture(pair) => pair.compatibility_json(metadata),
            #[cfg(feature = "diagram-class")]
            Self::Class(pair) => pair.compatibility_json(metadata),
            #[cfg(feature = "diagram-c4")]
            Self::C4(pair) => pair.compatibility_json(metadata),
            #[cfg(feature = "diagram-cynefin")]
            Self::Cynefin(pair) => pair.compatibility_json(metadata),
            #[cfg(feature = "diagram-wardley")]
            Self::Wardley(pair) => pair.compatibility_json(metadata),
            #[cfg(feature = "diagram-railroad")]
            Self::Railroad(pair) => pair.compatibility_json(metadata),
            #[cfg(feature = "diagram-kanban")]
            Self::Kanban(pair) => pair.compatibility_json(metadata),
            #[cfg(feature = "diagram-gantt")]
            Self::Gantt(pair) => pair.compatibility_json(metadata),
            #[cfg(feature = "diagram-pie")]
            Self::Pie(pair) => pair.compatibility_json(metadata),
            #[cfg(feature = "diagram-packet")]
            Self::Packet(pair) => pair.compatibility_json(metadata),
            #[cfg(feature = "diagram-timeline")]
            Self::Timeline(pair) => pair.compatibility_json(metadata),
            #[cfg(feature = "diagram-journey")]
            Self::Journey(pair) => pair.compatibility_json(metadata),
            #[cfg(feature = "diagram-requirement")]
            Self::Requirement(pair) => pair.compatibility_json(metadata),
            #[cfg(feature = "diagram-sankey")]
            Self::Sankey(pair) => pair.compatibility_json(metadata),
            #[cfg(feature = "diagram-radar")]
            Self::Radar(pair) => pair.compatibility_json(metadata),
            #[cfg(feature = "diagram-info")]
            Self::Info(pair) => pair.compatibility_json(metadata),
            #[cfg(feature = "diagram-treemap")]
            Self::Treemap(pair) => pair.compatibility_json(metadata),
            #[cfg(feature = "diagram-block")]
            Self::Block(pair) => pair.compatibility_json(metadata),
            #[cfg(feature = "diagram-er")]
            Self::Er(pair) => pair.compatibility_json(metadata),
            #[cfg(feature = "diagram-quadrant-chart")]
            Self::QuadrantChart(pair) => pair.compatibility_json(metadata),
            #[cfg(feature = "diagram-xychart")]
            Self::XyChart(pair) => pair.compatibility_json(metadata),
            #[cfg(feature = "diagram-git-graph")]
            Self::GitGraph(pair) => pair.compatibility_json(metadata),
            #[cfg(feature = "diagram-tree-view")]
            Self::TreeView(pair) => pair.compatibility_json(metadata),
            #[cfg(feature = "diagram-ishikawa")]
            Self::Ishikawa(pair) => pair.compatibility_json(metadata),
            #[cfg(feature = "diagram-event-modeling")]
            Self::EventModeling(pair) => pair.compatibility_json(metadata),
            #[cfg(feature = "diagram-venn")]
            Self::Venn(pair) => pair.compatibility_json(metadata),
            #[cfg(feature = "diagram-usecase")]
            Self::Usecase(pair) => pair.compatibility_json(metadata),
        }
    }

    fn layout_projection(&self) -> LayoutProjection<'_> {
        match self {
            #[cfg(feature = "diagram-agentflow")]
            Self::Agentflow { flow, .. } => LayoutProjection::AgentflowDiagram(flow.pair.layout()),
            Self::Error(pair) => LayoutProjection::ErrorDiagram(pair.layout()),
            #[cfg(feature = "diagram-mindmap")]
            Self::Mindmap(pair) => LayoutProjection::MindmapDiagram(pair.layout()),
            #[cfg(feature = "diagram-state")]
            Self::State(pair) => LayoutProjection::StateDiagram(pair.layout()),
            #[cfg(feature = "diagram-sequence")]
            Self::Sequence(pair) => LayoutProjection::SequenceDiagram(pair.layout().layout()),
            #[cfg(feature = "diagram-zenuml")]
            Self::Zenuml(pair) => LayoutProjection::ZenumlDiagram(pair.layout()),
            #[cfg(any(feature = "diagram-flowchart", feature = "diagram-swimlane"))]
            Self::Flowchart(artifact) => LayoutProjection::Flowchart(artifact.pair.layout()),
            #[cfg(any(feature = "diagram-flowchart", feature = "diagram-swimlane"))]
            Self::Swimlane(artifact) => LayoutProjection::SwimlaneDiagram(artifact.pair.layout()),
            #[cfg(feature = "layout-cytoscape")]
            #[cfg(feature = "diagram-architecture")]
            Self::Architecture(pair) => LayoutProjection::ArchitectureDiagram(pair.layout()),
            #[cfg(feature = "diagram-class")]
            Self::Class(pair) => LayoutProjection::ClassDiagram(pair.layout()),
            #[cfg(feature = "diagram-c4")]
            Self::C4(pair) => LayoutProjection::C4Diagram(pair.layout()),
            #[cfg(feature = "diagram-cynefin")]
            Self::Cynefin(pair) => LayoutProjection::CynefinDiagram(pair.layout()),
            #[cfg(feature = "diagram-wardley")]
            Self::Wardley(pair) => LayoutProjection::WardleyDiagram(pair.layout()),
            #[cfg(feature = "diagram-railroad")]
            Self::Railroad(pair) => LayoutProjection::RailroadDiagram(pair.layout()),
            #[cfg(feature = "diagram-kanban")]
            Self::Kanban(pair) => LayoutProjection::KanbanDiagram(pair.layout().layout()),
            #[cfg(feature = "diagram-gantt")]
            Self::Gantt(pair) => LayoutProjection::GanttDiagram(pair.layout()),
            #[cfg(feature = "diagram-pie")]
            Self::Pie(pair) => LayoutProjection::PieDiagram(pair.layout()),
            #[cfg(feature = "diagram-packet")]
            Self::Packet(pair) => LayoutProjection::PacketDiagram(pair.layout()),
            #[cfg(feature = "diagram-timeline")]
            Self::Timeline(pair) => LayoutProjection::TimelineDiagram(pair.layout()),
            #[cfg(feature = "diagram-journey")]
            Self::Journey(pair) => LayoutProjection::JourneyDiagram(pair.layout()),
            #[cfg(feature = "diagram-requirement")]
            Self::Requirement(pair) => LayoutProjection::RequirementDiagram(pair.layout().layout()),
            #[cfg(feature = "diagram-sankey")]
            Self::Sankey(pair) => LayoutProjection::SankeyDiagram(pair.layout()),
            #[cfg(feature = "diagram-radar")]
            Self::Radar(pair) => LayoutProjection::RadarDiagram(pair.layout()),
            #[cfg(feature = "diagram-info")]
            Self::Info(pair) => LayoutProjection::InfoDiagram(pair.layout()),
            #[cfg(feature = "diagram-treemap")]
            Self::Treemap(pair) => LayoutProjection::TreemapDiagram(pair.layout()),
            #[cfg(feature = "diagram-block")]
            Self::Block(pair) => LayoutProjection::BlockDiagram(pair.layout()),
            #[cfg(feature = "diagram-er")]
            Self::Er(pair) => LayoutProjection::ErDiagram(pair.layout()),
            #[cfg(feature = "diagram-quadrant-chart")]
            Self::QuadrantChart(pair) => LayoutProjection::QuadrantChartDiagram(pair.layout()),
            #[cfg(feature = "diagram-xychart")]
            Self::XyChart(pair) => LayoutProjection::XyChartDiagram(pair.layout()),
            #[cfg(feature = "diagram-git-graph")]
            Self::GitGraph(pair) => LayoutProjection::GitGraphDiagram(pair.layout()),
            #[cfg(feature = "diagram-tree-view")]
            Self::TreeView(pair) => LayoutProjection::TreeViewDiagram(pair.layout()),
            #[cfg(feature = "diagram-ishikawa")]
            Self::Ishikawa(pair) => LayoutProjection::IshikawaDiagram(pair.layout()),
            #[cfg(feature = "diagram-event-modeling")]
            Self::EventModeling(pair) => LayoutProjection::EventModelingDiagram(pair.layout()),
            #[cfg(feature = "diagram-venn")]
            Self::Venn(pair) => LayoutProjection::VennDiagram(pair.layout()),
            #[cfg(feature = "diagram-usecase")]
            Self::Usecase(pair) => LayoutProjection::UsecaseDiagram(pair.layout().layout()),
        }
    }
}

pub struct FamilyRenderArtifact {
    metadata: ParseMetadata,
    compatibility_projection: OnceLock<std::result::Result<ManagedSemanticJson, String>>,
    family: BuiltinFamilyArtifact,
    required_capabilities: Vec<RenderCapability>,
    session: RenderSession,
}

/// Owned projection of the Gantt time scale used by comparison tooling.
///
/// The projection deliberately hides the full family layout. Its inverse uses the same rounded
/// pixel mapping as the renderer, so coordinates that fall between representable task times return
/// `None` instead of inventing a timestamp.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct GanttTimeAxisDiagnostics {
    min_ms: i64,
    max_ms: i64,
    left_x: f64,
    drawable_width: f64,
}

impl GanttTimeAxisDiagnostics {
    #[cfg(feature = "diagram-gantt")]
    fn from_layout(layout: &GanttDiagramLayout) -> Option<Self> {
        let min_ms = layout.tasks.iter().map(|task| task.start_ms).min()?;
        let max_ms = layout.tasks.iter().map(|task| task.end_ms).max()?;
        if max_ms <= min_ms {
            return None;
        }

        let left_x = layout.left_padding;
        let drawable_width = (layout.width - layout.left_padding - layout.right_padding).max(1.0);
        if !left_x.is_finite() || !drawable_width.is_finite() {
            return None;
        }

        Some(Self {
            min_ms,
            max_ms,
            left_x,
            drawable_width,
        })
    }

    /// Resolves an exact rendered x coordinate back to a Unix timestamp in milliseconds.
    pub fn unix_millis_at_rendered_x(&self, target_x: f64) -> Option<i64> {
        if !target_x.is_finite() {
            return None;
        }

        let span_ms = (i128::from(self.max_ms) - i128::from(self.min_ms)) as f64;
        let scaled_x = target_x - self.left_x;
        if !span_ms.is_finite() || !scaled_x.is_finite() {
            return None;
        }

        let estimate = self.min_ms as f64 + span_ms * (scaled_x / self.drawable_width);
        if !estimate.is_finite() {
            return None;
        }

        let mut lo = estimate.round() as i64;
        let mut hi = lo;
        let mut step = 1_i64;
        for _ in 0..80 {
            if self.rendered_x(lo)? <= target_x {
                break;
            }
            hi = lo;
            lo = lo.saturating_sub(step);
            step = step.saturating_mul(2);
        }

        step = 1;
        for _ in 0..80 {
            if self.rendered_x(hi)? >= target_x {
                break;
            }
            lo = hi;
            hi = hi.saturating_add(step);
            step = step.saturating_mul(2);
        }

        let lo_x = self.rendered_x(lo)?;
        let hi_x = self.rendered_x(hi)?;
        if !(lo_x <= target_x && target_x <= hi_x) {
            return None;
        }

        while lo < hi {
            let half_distance = ((i128::from(hi) - i128::from(lo)) / 2) as i64;
            let mid = lo + half_distance;
            if self.rendered_x(mid)? < target_x {
                lo = mid.saturating_add(1);
            } else {
                hi = mid;
            }
        }

        (self.rendered_x(lo)? == target_x).then_some(lo)
    }

    fn rendered_x(&self, unix_millis: i64) -> Option<f64> {
        let offset_ms = (i128::from(unix_millis) - i128::from(self.min_ms)) as f64;
        let span_ms = (i128::from(self.max_ms) - i128::from(self.min_ms)) as f64;
        let x = self.left_x + (offset_ms / span_ms * self.drawable_width).round();
        x.is_finite().then_some(x)
    }
}

/// A completed family SVG produced by the canonical typed render operation.
///
/// Root completion evidence is private to the renderer and cannot be named by callers:
///
/// ```compile_fail
/// use merman_render::svg::RootedSvg;
/// ```
///
/// A raw string cannot be substituted for a completed family SVG:
///
/// ```compile_fail
/// use merman_render::family::RenderedFamilySvg;
///
/// let forged: RenderedFamilySvg = String::from("<svg xmlns=\"http://www.w3.org/2000/svg\"/>");
/// ```
pub struct RenderedFamilySvg {
    svg: String,
    family_kind: RenderFamilyKind,
    metadata: ParseMetadata,
    required_capabilities: Vec<RenderCapability>,
    session: RenderSession,
}

impl RenderedFamilySvg {
    pub fn svg(&self) -> &str {
        &self.svg
    }

    pub fn metadata(&self) -> &ParseMetadata {
        &self.metadata
    }

    pub fn family_kind(&self) -> RenderFamilyKind {
        self.family_kind
    }

    /// Returns the optional capabilities admitted during this artifact's preparation.
    pub fn required_capabilities(&self) -> &[RenderCapability] {
        &self.required_capabilities
    }

    /// Applies an output pipeline while retaining the renderer-owned family capability.
    pub fn apply_pipeline(mut self, pipeline: &SvgPipeline) -> Result<Self> {
        self.session.checkpoint(OperationPhase::Postprocess)?;
        let output_metadata = self.output_metadata()?;
        self.svg = pipeline.process_owned_to_string_with_metadata(
            self.svg,
            &output_metadata,
            &self.session,
        )?;
        self.session.work_meter().preflight_svg_byte_count(
            self.svg.len(),
            ResourceLimitPhase::SvgPostprocess,
            OperationPhase::Postprocess,
        )?;
        self.session.checkpoint(OperationPhase::Postprocess)?;
        Ok(self)
    }

    /// Finalizes the typed family output for resvg/raster consumption.
    pub fn finalize_resvg(self, pipeline: &SvgPipeline) -> Result<RenderedResvgCompatibleSvg> {
        self.session.checkpoint(OperationPhase::Export)?;
        let output_metadata = self.output_metadata()?;
        let svg = pipeline.process_owned_resvg_compatible_with_metadata(
            self.svg,
            &output_metadata,
            &self.session,
        )?;
        self.session.work_meter().preflight_svg_byte_count(
            svg.as_str().len(),
            ResourceLimitPhase::SvgPostprocess,
            OperationPhase::Export,
        )?;
        self.session.checkpoint(OperationPhase::Export)?;
        Ok(RenderedResvgCompatibleSvg {
            svg,
            family_kind: self.family_kind,
            metadata: self.metadata,
            session: self.session,
        })
    }

    fn output_metadata(&self) -> Result<SvgPostprocessMetadata> {
        let metadata = SvgPostprocessMetadata::from_svg_with_execution(
            &self.svg,
            SvgPostprocessExecution::new(&self.session),
        )?;
        Ok(metadata
            .with_family_kind(self.family_kind)
            .with_diagram_type(self.metadata.diagram_type.clone())
            .with_optional_diagram_title(self.metadata.title.clone()))
    }

    pub fn into_parts(self) -> (String, RenderFamilyKind, ParseMetadata, RenderSession) {
        (self.svg, self.family_kind, self.metadata, self.session)
    }
}

/// Renderer-owned family output after the terminal resvg compatibility finalizer.
pub struct RenderedResvgCompatibleSvg {
    svg: ResvgCompatibleSvg,
    family_kind: RenderFamilyKind,
    metadata: ParseMetadata,
    session: RenderSession,
}

impl RenderedResvgCompatibleSvg {
    pub fn svg(&self) -> &ResvgCompatibleSvg {
        &self.svg
    }

    pub fn into_parts(
        self,
    ) -> (
        ResvgCompatibleSvg,
        RenderFamilyKind,
        ParseMetadata,
        RenderSession,
    ) {
        (self.svg, self.family_kind, self.metadata, self.session)
    }
}

impl FamilyRenderArtifact {
    pub fn metadata(&self) -> &ParseMetadata {
        &self.metadata
    }

    pub fn family_kind(&self) -> RenderFamilyKind {
        self.family.kind()
    }

    pub fn gantt_time_axis_diagnostics(&self) -> Option<GanttTimeAxisDiagnostics> {
        match &self.family {
            #[cfg(feature = "diagram-gantt")]
            BuiltinFamilyArtifact::Gantt(pair) => {
                GanttTimeAxisDiagnostics::from_layout(pair.layout())
            }
            _ => None,
        }
    }

    pub fn layout_json(&self) -> Result<ManagedSemanticJson> {
        self.session.checkpoint(OperationPhase::Emit)?;
        let semantic = self
            .compatibility_projection
            .get_or_init(|| {
                self.family
                    .compatibility_json(&self.metadata)
                    .map_err(|error| {
                        format!(
                            "failed to project {} compatibility JSON: {error}",
                            self.family.kind()
                        )
                    })
            })
            .as_ref()
            .map_err(|message| Error::InvalidModel {
                message: message.clone(),
            })?;
        let layout = serde_json::to_value(self.family.layout_projection())?;

        let mut metadata = serde_json::Map::new();
        metadata.insert(
            "diagram_type".to_string(),
            serde_json::Value::String(self.metadata.diagram_type.clone()),
        );
        metadata.insert(
            "title".to_string(),
            self.metadata
                .title
                .as_ref()
                .map_or(serde_json::Value::Null, |title| {
                    serde_json::Value::String(title.clone())
                }),
        );
        metadata.insert(
            "config".to_string(),
            ManagedSemanticJson::from(self.metadata.config.as_value()).into_unmanaged_value(),
        );
        metadata.insert(
            "effective_config".to_string(),
            ManagedSemanticJson::from(self.metadata.effective_config.as_value())
                .into_unmanaged_value(),
        );

        let mut projection = serde_json::Map::new();
        projection.insert("meta".to_string(), serde_json::Value::Object(metadata));
        projection.insert(
            "semantic".to_string(),
            semantic.clone().into_unmanaged_value(),
        );
        projection.insert("layout".to_string(), layout);
        let projection = ManagedSemanticJson::from(serde_json::Value::Object(projection));
        self.session.checkpoint(OperationPhase::Emit)?;
        Ok(projection)
    }

    /// Post-paint edge geometry, recovered while emitting the diagram.
    ///
    /// `layout_json` reports `LayoutEdge::points` before the endpoints are clipped to the endpoint
    /// shapes' ink. This returns the values that end up on screen, in the same layout coordinate
    /// space, so a consumer no longer has to render a whole SVG and recover them from the
    /// `data-points` attribute or edge-label transforms. Label positions are `None` for edges
    /// without a positioned label.
    ///
    /// Consumes the artifact, exactly like [`Self::render_svg`], because it runs the same compute
    /// pass. Returns an empty vector for families that do not route through the flowchart
    /// renderer.
    pub fn edge_geometry_json(self) -> Result<Vec<crate::model::EdgePaintGeometry>> {
        self.session.checkpoint(OperationPhase::Emit)?;
        let options =
            crate::svg::normalize_svg_render_options(&SvgRenderOptions::default(), &self.session)?;
        let debug = SvgDebugOptions::default();
        let mut geometry = Vec::new();
        let svg = render_family_artifact_svg(&self, &options, &debug, Some(&mut geometry))?;
        admit_rendered_svg_output(&self.session, &svg)?;
        self.session.checkpoint(OperationPhase::Emit)?;
        Ok(geometry)
    }

    pub fn render_svg(
        self,
        options: &SvgRenderOptions,
        debug: &SvgDebugOptions,
    ) -> Result<RenderedFamilySvg> {
        self.session.checkpoint(OperationPhase::Emit)?;
        let trace_stage = debug.flowchart_edge_trace().map(|(edge_id, destination)| {
            let staging = FlowchartEdgeTraceCollector::default();
            let staged_debug = debug
                .clone()
                .with_flowchart_edge_trace(edge_id.to_owned(), staging.clone());
            (destination.clone(), staging, staged_debug)
        });
        let render_debug = trace_stage
            .as_ref()
            .map_or(debug, |(_, _, staged_debug)| staged_debug);
        let svg = render_family_artifact_svg(&self, options, render_debug, None)?;
        admit_rendered_svg_output(&self.session, &svg)?;
        self.session.checkpoint(OperationPhase::Emit)?;
        if let Some((destination, staging, _)) = trace_stage {
            for trace in staging.drain() {
                destination.record(trace);
            }
        }
        let family_kind = self.family.kind();
        let Self {
            metadata,
            compatibility_projection: _,
            family: _,
            required_capabilities,
            session,
        } = self;

        Ok(RenderedFamilySvg {
            svg,
            family_kind,
            metadata,
            required_capabilities,
            session,
        })
    }
}

fn admit_rendered_svg_output(session: &RenderSession, svg: &str) -> Result<()> {
    // Termination wins over the final output ceiling when both become observable during emit.
    session.checkpoint(OperationPhase::Emit)?;
    session.work_meter().preflight_svg_byte_count(
        svg.len(),
        ResourceLimitPhase::SvgOutput,
        OperationPhase::Emit,
    )?;
    Ok(())
}

#[inline(never)]
fn render_family_artifact_svg(
    artifact: &FamilyRenderArtifact,
    request: &SvgRenderOptions,
    debug: &SvgDebugOptions,
    edge_paint_geometry: Option<&mut Vec<crate::model::EdgePaintGeometry>>,
) -> Result<String> {
    let options = crate::svg::normalize_svg_render_options(request, &artifact.session)?;
    #[cfg(feature = "diagram-agentflow")]
    // Agentflow is adapted to the Flowchart renderer, while Mermaid's config namespace remains
    // `agentflow`. Keep the public metadata untouched and project only the renderer input.
    let projected_metadata = if matches!(&artifact.family, BuiltinFamilyArtifact::Agentflow { .. })
    {
        let mut metadata = artifact.metadata.clone();
        metadata.effective_config = project_agentflow_flowchart_config(&metadata.effective_config);
        Some(metadata)
    } else {
        None
    };
    #[cfg(feature = "diagram-agentflow")]
    let metadata = projected_metadata.as_ref().unwrap_or(&artifact.metadata);
    #[cfg(not(feature = "diagram-agentflow"))]
    let metadata = &artifact.metadata;
    #[cfg(all(feature = "layout-cytoscape", feature = "diagram-architecture"))]
    if let BuiltinFamilyArtifact::Architecture(pair) = &artifact.family {
        return crate::svg::render_architecture_family_artifact(
            pair,
            &metadata.effective_config,
            &artifact.session,
            &options,
            debug,
        );
    }
    crate::svg::render_builtin_family_artifact(
        &artifact.family,
        metadata,
        &artifact.session,
        &options,
        debug,
        edge_paint_geometry,
    )
}

#[inline(never)]
fn prepare_pair<S, L>(
    semantic: S,
    layout: impl FnOnce(&S) -> Result<L>,
) -> Result<Box<FamilyPair<S, L>>> {
    let layout = layout(&semantic)?;
    Ok(Box::new(FamilyPair::new(semantic, layout)))
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct FlowchartSvgLabelPreparation(bool);

impl FlowchartSvgLabelPreparation {
    #[cfg(any(
        feature = "diagram-flowchart",
        feature = "diagram-swimlane",
        feature = "diagram-agentflow"
    ))]
    const fn enabled(self) -> bool {
        self.0
    }
}

const DEFAULT_FLOWCHART_SVG_LABEL_PREPARATION: FlowchartSvgLabelPreparation =
    FlowchartSvgLabelPreparation(true);

#[cfg(any(
    feature = "diagram-flowchart",
    feature = "diagram-swimlane",
    feature = "diagram-agentflow"
))]
fn prepare_flowchart_artifact<L>(
    semantic: diagrams::flowchart::FlowchartModel,
    render_context: diagrams::flowchart::FlowchartRenderContext,
    policy: Option<FlowchartPresentationPolicy>,
    svg_label_preparation: FlowchartSvgLabelPreparation,
    layout: impl FnOnce(
        &diagrams::flowchart::FlowchartModel,
        &diagrams::flowchart::FlowchartRenderContext,
        Option<&crate::flowchart::FlowchartSvgLabelSidecarBuilder>,
    ) -> Result<L>,
) -> Result<Box<FlowchartFamilyArtifact<L>>> {
    let svg_label_sidecar = svg_label_preparation
        .enabled()
        .then(crate::flowchart::FlowchartSvgLabelSidecarBuilder::default);
    let layout = layout(&semantic, &render_context, svg_label_sidecar.as_ref())?;
    let svg_label_sidecar = svg_label_sidecar
        .map(crate::flowchart::FlowchartSvgLabelSidecarBuilder::finish)
        .unwrap_or_default();
    Ok(Box::new(FlowchartFamilyArtifact {
        pair: FamilyPair::new(semantic, layout),
        render_context,
        svg_label_sidecar,
        policy,
    }))
}

#[cfg(feature = "diagram-agentflow")]
fn project_agentflow_flowchart_config(
    config: &merman_core::MermaidConfig,
) -> merman_core::MermaidConfig {
    let mut projected = config.clone();
    let root = config.as_value();
    let agentflow = root.get("agentflow").unwrap_or(&serde_json::Value::Null);
    // Agentflow's renderer owns the root viewport. Defaults here mirror its `??`
    // expressions; normal Engine configuration already includes schema defaults.
    // Overwrite even absent family values so unrelated Flowchart settings cannot leak in.
    for (key, fallback) in [
        ("diagramPadding", serde_json::json!(8)),
        ("useMaxWidth", serde_json::json!(true)),
        ("titleTopMargin", serde_json::json!(0)),
    ] {
        let value = agentflow.get(key).filter(|value| !value.is_null());
        projected.set_value(
            &format!("flowchart.{key}"),
            value.cloned().unwrap_or(fallback),
        );
    }
    // Dagre takes a truthy root override before the renderer's family spacing.
    // Mermaid's ELK adapter ignores all three configuration namespaces for spacing.
    // Our shared adapter reads Flowchart settings, so keep its default spacing for ELK.
    // Resolve registration first: an unavailable ELK loader executes Dagre instead.
    let uses_dagre = crate::layout_backend::resolve_graph_layout(root).backend
        == crate::layout_backend::GraphLayoutBackend::Dagre;
    for key in ["nodeSpacing", "rankSpacing"] {
        let value = uses_dagre
            .then(|| {
                root.get(key)
                    .filter(|value| crate::config::json_value_is_truthy(value))
                    .or_else(|| {
                        agentflow
                            .get(key)
                            .filter(|value| crate::config::json_value_is_truthy(value))
                    })
            })
            .flatten();
        projected.set_value(
            &format!("flowchart.{key}"),
            value.cloned().unwrap_or_else(|| serde_json::json!(50)),
        );
    }
    // AgentflowDB assigns these values to each node before the shared shape helpers
    // read them. Node-specific values precede Flowchart configuration; only a falsy
    // wrappingWidth falls through via labelHelper's JavaScript `||` expression.
    for key in ["minNodeWidth", "wrappingWidth"] {
        if let Some(value) = agentflow.get(key) {
            if key == "wrappingWidth" && !crate::config::json_value_is_truthy(value) {
                continue;
            }
            projected.set_value(&format!("flowchart.{key}"), value.clone());
        }
    }
    projected
}

#[cfg(any(feature = "diagram-flowchart", feature = "diagram-swimlane"))]
fn semantic_flowchart_requires_math(model: &diagrams::flowchart::FlowchartModel) -> bool {
    model
        .nodes
        .iter()
        .filter_map(|node| node.label.as_deref())
        .chain(model.edges.iter().filter_map(|edge| edge.label.as_deref()))
        .chain(
            model
                .subgraphs
                .iter()
                .map(|subgraph| subgraph.title.as_str()),
        )
        .any(crate::math::contains_delimited_math)
}

#[cfg(feature = "diagram-sequence")]
fn sequence_requires_math(model: &diagrams::sequence::SequenceDiagramRenderModel) -> bool {
    model
        .actors
        .values()
        .map(|actor| actor.description.as_str())
        .chain(model.messages.iter().map(|message| message.message_text()))
        .chain(model.notes.iter().map(|note| note.message.as_str()))
        .any(crate::math::contains_delimited_math)
}

#[cfg(feature = "diagram-mindmap")]
fn mindmap_requires_math(model: &diagrams::mindmap::MindmapDiagramRenderModel) -> bool {
    model
        .nodes
        .iter()
        .map(|node| node.label.as_str())
        .any(crate::math::contains_delimited_math)
}

fn parsed_render_requires_math(parsed: &ParsedDiagramRender) -> bool {
    match parsed.model() {
        #[cfg(feature = "diagram-usecase")]
        RenderSemanticModel::Usecase(model) => {
            crate::usecase::requires_math(model, &parsed.metadata().effective_config)
        }
        #[cfg(feature = "diagram-agentflow")]
        RenderSemanticModel::Agentflow(model) => model
            .vertices
            .iter()
            .filter_map(|node| node.label.as_deref())
            .chain(model.edges.iter().filter_map(|edge| edge.label.as_deref()))
            .chain(
                model
                    .sub_graphs
                    .iter()
                    .filter_map(|graph| graph.title.as_deref()),
            )
            .chain(
                model
                    .connectors
                    .iter()
                    .filter_map(|connector| connector.title.as_deref()),
            )
            .any(crate::math::contains_delimited_math),
        #[cfg(feature = "diagram-class")]
        RenderSemanticModel::Class(model) => crate::class::class_requires_math(model),
        #[cfg(any(feature = "diagram-flowchart", feature = "diagram-swimlane"))]
        RenderSemanticModel::Flowchart(model) => parsed.flowchart_render_context().map_or_else(
            || semantic_flowchart_requires_math(model),
            |render_context| {
                crate::flowchart::FlowchartRenderModelRef::new(model, render_context)
                    .requires_math()
            },
        ),
        #[cfg(feature = "diagram-mindmap")]
        RenderSemanticModel::Mindmap(model) => mindmap_requires_math(model),
        #[cfg(feature = "diagram-sequence")]
        RenderSemanticModel::Sequence(model) => sequence_requires_math(model),
        _ => false,
    }
}

fn capability_is_available(capability: RenderCapability, session: &RenderSession) -> bool {
    session.supports_capability(capability)
}

#[allow(
    unused_variables,
    reason = "Dispatch inputs depend on the selected diagram families."
)]
fn required_capabilities(parsed: &ParsedDiagramRender) -> Vec<RenderCapability> {
    let mut required = Vec::with_capacity(2);
    let meta = parsed.metadata();
    let model = parsed.model();
    let effective_config = &meta.effective_config;
    match model {
        #[cfg(feature = "diagram-architecture")]
        RenderSemanticModel::Architecture(_) => {
            required.push(RenderCapability::LayoutCytoscape);
        }
        #[cfg(feature = "diagram-mindmap")]
        RenderSemanticModel::Mindmap(_) => {
            if let Some(capability) = crate::mindmap::required_layout_capability(effective_config) {
                required.push(capability);
            }
        }
        #[cfg(feature = "diagram-agentflow")]
        RenderSemanticModel::Agentflow(_) => {
            required.extend(
                crate::layout_backend::resolve_graph_layout(effective_config.as_value())
                    .required_capability(),
            );
        }
        #[cfg(feature = "diagram-usecase")]
        RenderSemanticModel::Usecase(_) => {
            required.extend(
                crate::layout_backend::resolve_graph_layout(effective_config.as_value())
                    .required_capability(),
            );
        }
        #[cfg(feature = "diagram-state")]
        RenderSemanticModel::State(_) => {
            required.extend(
                crate::layout_backend::resolve_graph_layout(effective_config.as_value())
                    .required_capability(),
            );
        }
        #[cfg(feature = "diagram-requirement")]
        RenderSemanticModel::Requirement(_) => {
            required.extend(
                crate::layout_backend::resolve_graph_layout(effective_config.as_value())
                    .required_capability(),
            );
        }
        #[cfg(any(feature = "diagram-flowchart", feature = "diagram-swimlane"))]
        RenderSemanticModel::Flowchart(_) => {
            required.extend(
                crate::layout_backend::resolve_graph_layout(effective_config.as_value())
                    .required_capability(),
            );
        }
        #[cfg(feature = "diagram-class")]
        RenderSemanticModel::Class(_) => {
            required.extend(
                crate::layout_backend::resolve_graph_layout(effective_config.as_value())
                    .required_capability(),
            );
        }
        #[cfg(feature = "diagram-er")]
        RenderSemanticModel::Er(_) => {
            required.extend(
                crate::layout_backend::resolve_graph_layout(effective_config.as_value())
                    .required_capability(),
            );
        }
        _ => {}
    }

    if parsed_render_requires_math(parsed) {
        required.push(RenderCapability::Math);
    }
    required
}

/// Reports whether this renderer contains the logical family handler for a known diagram type.
/// Optional layout and math capabilities are admitted separately during planning.
#[allow(
    clippy::match_like_matches_macro,
    reason = "Each arm has a distinct feature condition that only coincides in all-family builds."
)]
pub fn supports_diagram_type(diagram_type: &str) -> bool {
    match merman_core::diagram_type_family_kind(diagram_type) {
        Some("error") => true,
        Some("agentflow") => cfg!(feature = "diagram-agentflow"),
        Some("usecase") => cfg!(feature = "diagram-usecase"),
        Some("flowchart") => cfg!(feature = "diagram-flowchart"),
        Some("swimlane") => cfg!(feature = "diagram-swimlane"),
        Some("mindmap") => cfg!(feature = "diagram-mindmap"),
        Some("architecture") => cfg!(feature = "diagram-architecture"),
        Some("zenuml") => cfg!(feature = "diagram-zenuml"),
        Some("sequence") => cfg!(feature = "diagram-sequence"),
        Some("c4") => cfg!(feature = "diagram-c4"),
        Some("kanban") => cfg!(feature = "diagram-kanban"),
        Some("class") => cfg!(feature = "diagram-class"),
        Some("er") => cfg!(feature = "diagram-er"),
        Some("gantt") => cfg!(feature = "diagram-gantt"),
        Some("info") => cfg!(feature = "diagram-info"),
        Some("pie") => cfg!(feature = "diagram-pie"),
        Some("requirement") => cfg!(feature = "diagram-requirement"),
        Some("timeline") => cfg!(feature = "diagram-timeline"),
        Some("gitGraph") => cfg!(feature = "diagram-git-graph"),
        Some("state") => cfg!(feature = "diagram-state"),
        Some("journey") => cfg!(feature = "diagram-journey"),
        Some("quadrantChart") => cfg!(feature = "diagram-quadrant-chart"),
        Some("sankey") => cfg!(feature = "diagram-sankey"),
        Some("packet") => cfg!(feature = "diagram-packet"),
        Some("xychart") => cfg!(feature = "diagram-xychart"),
        Some("block") => cfg!(feature = "diagram-block"),
        Some("eventmodeling") => cfg!(feature = "diagram-event-modeling"),
        Some("treeView") => cfg!(feature = "diagram-tree-view"),
        Some("radar") => cfg!(feature = "diagram-radar"),
        Some("ishikawa") => cfg!(feature = "diagram-ishikawa"),
        Some("treemap") => cfg!(feature = "diagram-treemap"),
        Some("railroad") => cfg!(feature = "diagram-railroad"),
        Some("venn") => cfg!(feature = "diagram-venn"),
        Some("wardley") => cfg!(feature = "diagram-wardley"),
        Some("cynefin") => cfg!(feature = "diagram-cynefin"),
        _ => false,
    }
}

fn validate_render_input(parsed: &ParsedDiagramRender, session: &RenderSession) -> Result<()> {
    session.checkpoint(OperationPhase::Layout)?;
    let meta = parsed.metadata();
    let model = parsed.model();
    let diagram_type = meta.diagram_type.as_str();
    if let RenderSemanticModel::CustomJson(custom) = model {
        return Err(Error::NonRenderableCustomModel {
            diagram_type: meta.diagram_type.clone(),
            model_name: custom.model_name().to_string(),
            provenance: custom.provenance(),
        });
    }

    if !model.supports_diagram_type(diagram_type) {
        return Err(Error::InvalidModel {
            message: format!(
                "unexpected render model variant {} for diagram type: {diagram_type}",
                model.kind()
            ),
        });
    }

    if !supports_diagram_type(diagram_type) {
        return Err(Error::UnsupportedDiagram {
            diagram_type: diagram_type.to_owned(),
        });
    }
    session
        .work_meter()
        .preflight_parsed_render(parsed, OperationPhase::Layout)?;
    Ok(())
}

/// Plans capability admission for a canonically paired typed render model without running layout.
pub fn plan_render(
    parsed: &ParsedDiagramRender,
    session: &RenderSession,
) -> Result<RenderCapabilityPlan> {
    plan_render_with_policy(parsed, session, PresentationRenderPolicy::default())
}

/// Plans capability admission and selected presentation aspects without running layout.
pub fn plan_render_with_policy(
    parsed: &ParsedDiagramRender,
    session: &RenderSession,
    render_policy: PresentationRenderPolicy,
) -> Result<RenderCapabilityPlan> {
    let meta = parsed.metadata();
    let model = parsed.model();
    validate_render_input(parsed, session)?;
    let required = required_capabilities(parsed);
    let missing = required
        .iter()
        .copied()
        .filter(|capability| !capability_is_available(*capability, session))
        .collect();
    let flowchart_svg_applicable = match model {
        #[cfg(any(feature = "diagram-flowchart", feature = "diagram-swimlane"))]
        RenderSemanticModel::Flowchart(_) => {
            meta.effective_config.get_str("layout") != Some("swimlane")
        }
        #[cfg(feature = "diagram-agentflow")]
        RenderSemanticModel::Agentflow(_) => {
            meta.effective_config.get_str("layout") != Some("swimlane")
        }
        _ => false,
    };
    let presentation_aspects = render_policy.resolve_aspects(
        flowchart_svg_applicable,
        crate::layout_backend::ElkRootAlgorithm::from_name(
            crate::layout_backend::resolve_graph_layout(meta.effective_config.as_value()).requested,
        )
        .is_some(),
        capability_is_available(RenderCapability::LayoutElk, session),
    );

    Ok(RenderCapabilityPlan {
        diagram_type: meta.diagram_type.clone(),
        required,
        missing,
        presentation_profile: render_policy.profile(),
        presentation_aspects,
    })
}

#[inline(never)]
#[cfg(feature = "diagram-class")]
fn prepare_class_family(
    model: ClassDiagram,
    meta: &ParseMetadata,
    execution: &LayoutExecution<'_>,
) -> Result<BuiltinFamilyArtifact> {
    Ok(BuiltinFamilyArtifact::Class(prepare_pair(
        model,
        |model| crate::layout_class_typed_by_engine(model, &meta.effective_config, execution),
    )?))
}

#[inline(never)]
#[cfg(feature = "diagram-class")]
fn prepare_class_render(
    parsed: ParsedDiagramRender,
    options: &LayoutOptions,
    required_capabilities: Vec<RenderCapability>,
    session: RenderSession,
) -> Result<FamilyRenderArtifact> {
    let (meta, model) = parsed.into_parts();
    let RenderSemanticModel::Class(model) = model else {
        unreachable!("Class render dispatch requires a Class semantic model")
    };
    let execution = LayoutExecution::new(options, &session);
    let family = prepare_class_family(model, &meta, &execution)?;

    Ok(FamilyRenderArtifact {
        metadata: meta,
        compatibility_projection: OnceLock::new(),
        family,
        required_capabilities,
        session,
    })
}

/// Prepares one family-owned typed semantic model for layout and SVG rendering.
///
/// Compatibility JSON is deliberately not accepted by this interface:
///
/// ```compile_fail
/// use merman_render::{LayoutOptions, environment::RenderEnvironment};
///
/// let session = RenderEnvironment::deterministic().begin_session().unwrap();
/// let raw_json = serde_json::json!({ "type": "flowchart-v2" });
/// let _ = merman_render::family::prepare(raw_json, &LayoutOptions::default(), session);
/// ```
///
/// Family semantic/layout pairing is private and therefore cannot be assembled independently:
///
/// ```compile_fail
/// use merman_render::family::FamilyPair;
/// ```
pub fn prepare(
    parsed: ParsedDiagramRender,
    options: &LayoutOptions,
    session: RenderSession,
) -> Result<FamilyRenderArtifact> {
    prepare_with_render_policy(
        parsed,
        options,
        session,
        PresentationRenderPolicy::default(),
    )
}

/// Prepares one family artifact with renderer policy derived from a resolved presentation.
pub fn prepare_with_render_policy(
    parsed: ParsedDiagramRender,
    options: &LayoutOptions,
    session: RenderSession,
    render_policy: PresentationRenderPolicy,
) -> Result<FamilyRenderArtifact> {
    prepare_with_render_policy_impl(
        parsed,
        options,
        session,
        render_policy,
        DEFAULT_FLOWCHART_SVG_LABEL_PREPARATION,
    )
}

fn prepare_with_render_policy_impl(
    parsed: ParsedDiagramRender,
    options: &LayoutOptions,
    session: RenderSession,
    render_policy: PresentationRenderPolicy,
    flowchart_svg_label_preparation: FlowchartSvgLabelPreparation,
) -> Result<FamilyRenderArtifact> {
    session.checkpoint(OperationPhase::Layout)?;
    let capability_plan = plan_render_with_policy(&parsed, &session, render_policy)?;
    capability_plan.ensure_available()?;
    let required_capabilities = capability_plan.required_capabilities().to_vec();
    // The heterogeneous router has one generic layout call per family. Keep its debug-build
    // caller slots out of the Class Dagre call chain, whose own phase frames are already deep.
    #[cfg(feature = "diagram-class")]
    if matches!(parsed.model(), RenderSemanticModel::Class(_)) {
        let artifact = prepare_class_render(parsed, options, required_capabilities, session)?;
        artifact.session.checkpoint(OperationPhase::Layout)?;
        return Ok(artifact);
    }
    let artifact = prepare_non_class_render(
        parsed,
        options,
        session,
        render_policy,
        required_capabilities,
        flowchart_svg_label_preparation,
    )?;
    artifact.session.checkpoint(OperationPhase::Layout)?;
    Ok(artifact)
}

#[inline(never)]
#[allow(
    unused_variables,
    reason = "Dispatch inputs depend on the selected diagram families."
)]
fn prepare_non_class_render(
    parsed: ParsedDiagramRender,
    options: &LayoutOptions,
    session: RenderSession,
    render_policy: PresentationRenderPolicy,
    required_capabilities: Vec<RenderCapability>,
    flowchart_svg_label_preparation: FlowchartSvgLabelPreparation,
) -> Result<FamilyRenderArtifact> {
    let (meta, model, render_context) = parsed.into_render_parts();
    #[cfg(any(feature = "diagram-flowchart", feature = "diagram-swimlane"))]
    let flowchart_label_sources = render_context.into_flowchart_render_context();
    let diagram_type = meta.diagram_type.as_str();
    let execution = LayoutExecution::new(options, &session);
    let effective_config = meta.effective_config.as_value();
    let title = meta.title.as_deref();
    let family = match model {
        RenderSemanticModel::Error(model) => {
            BuiltinFamilyArtifact::Error(prepare_pair(model, |model| {
                crate::error::layout_error_diagram_typed(
                    model,
                    effective_config,
                    execution.text_measurer(),
                )
            })?)
        }
        #[cfg(feature = "diagram-mindmap")]
        RenderSemanticModel::Mindmap(model) => {
            BuiltinFamilyArtifact::Mindmap(prepare_pair(model, |model| {
                crate::mindmap::layout_mindmap_diagram_typed_with_work_meter(
                    model,
                    &meta.effective_config,
                    execution.text_measurer(),
                    execution.math_renderer(),
                    execution.work_meter(),
                    #[cfg(feature = "layout-elk")]
                    execution.elk_operation_seed(),
                )
            })?)
        }
        #[cfg(feature = "diagram-state")]
        RenderSemanticModel::State(model) => {
            BuiltinFamilyArtifact::State(prepare_pair(model, |model| {
                crate::state::layout_state_diagram_typed_with_work_meter(
                    model,
                    effective_config,
                    &execution,
                )
            })?)
        }
        #[cfg(feature = "diagram-sequence")]
        RenderSemanticModel::Sequence(model) => {
            let text_measurer = session
                .controlled_text_measurer(TextMeasurementPhase::Layout, OperationPhase::Layout);
            BuiltinFamilyArtifact::Sequence(prepare_pair(model, |model| {
                crate::sequence::prepare_sequence_diagram_typed_with_title_and_work_meter(
                    model,
                    title,
                    effective_config,
                    &text_measurer,
                    execution.math_renderer(),
                    execution.work_meter_ref(),
                )
            })?)
        }
        #[cfg(feature = "diagram-zenuml")]
        RenderSemanticModel::Zenuml(model) => {
            BuiltinFamilyArtifact::Zenuml(prepare_pair(model, |model| {
                crate::zenuml::layout_zenuml_diagram_typed(
                    model,
                    &crate::zenuml::ZenumlTypography::from_config(&meta.effective_config),
                    execution.text_measurer(),
                )
            })?)
        }
        #[cfg(any(feature = "diagram-flowchart", feature = "diagram-swimlane"))]
        RenderSemanticModel::Flowchart(model)
            if meta.effective_config.get_str("layout") == Some("swimlane") =>
        {
            BuiltinFamilyArtifact::Swimlane(prepare_flowchart_artifact(
                model,
                flowchart_label_sources,
                None,
                flowchart_svg_label_preparation,
                |model, label_sources, svg_label_sidecar| {
                    crate::swimlane::layout_swimlane_typed_with_work_meter_and_svg_label_sidecar(
                        model,
                        label_sources,
                        &meta.effective_config,
                        execution.text_measurer(),
                        execution.math_renderer(),
                        svg_label_sidecar,
                        execution.work_meter(),
                    )
                },
            )?)
        }
        #[cfg(any(feature = "diagram-flowchart", feature = "diagram-swimlane"))]
        RenderSemanticModel::Flowchart(model) => {
            BuiltinFamilyArtifact::Flowchart(prepare_flowchart_artifact(
                model,
                flowchart_label_sources,
                render_policy.flowchart(),
                flowchart_svg_label_preparation,
                |model, label_sources, svg_label_sidecar| {
                    crate::layout_flowchart_typed_with_render_labels_by_engine(
                        model,
                        label_sources,
                        &meta.effective_config,
                        &execution,
                        svg_label_sidecar,
                    )
                },
            )?)
        }
        #[cfg(feature = "layout-cytoscape")]
        #[cfg(feature = "diagram-architecture")]
        RenderSemanticModel::Architecture(model) => {
            BuiltinFamilyArtifact::Architecture(prepare_pair(model, |model| {
                crate::architecture::layout_architecture_diagram_typed(
                    model,
                    effective_config,
                    execution.text_measurer(),
                    execution.operation_seed(),
                    execution.work_meter().as_ref(),
                )
            })?)
        }
        #[cfg(not(feature = "layout-cytoscape"))]
        #[cfg(feature = "diagram-architecture")]
        RenderSemanticModel::Architecture(_) => {
            return Err(Error::MissingCapability {
                capability: RenderCapability::LayoutCytoscape,
                diagram_type: diagram_type.to_string(),
            });
        }
        #[cfg(feature = "diagram-class")]
        RenderSemanticModel::Class(_) => {
            unreachable!("Class models use the stack-bounded family dispatch path")
        }
        #[cfg(feature = "diagram-c4")]
        RenderSemanticModel::C4(model) => {
            BuiltinFamilyArtifact::C4(prepare_pair(model, |model| {
                crate::c4::layout_c4_diagram_typed(
                    model,
                    effective_config,
                    execution.text_measurer(),
                    execution.container_width,
                    execution.container_height,
                    execution.screen_available_width,
                )
            })?)
        }
        #[cfg(feature = "diagram-cynefin")]
        RenderSemanticModel::Cynefin(model) => {
            BuiltinFamilyArtifact::Cynefin(prepare_pair(model, |model| {
                crate::cynefin::layout_cynefin_diagram_typed(
                    model,
                    effective_config,
                    execution.text_measurer(),
                )
            })?)
        }
        #[cfg(feature = "diagram-wardley")]
        RenderSemanticModel::Wardley(model) => {
            BuiltinFamilyArtifact::Wardley(prepare_pair(model, |model| {
                crate::wardley::layout_wardley_diagram_typed(
                    model,
                    title,
                    effective_config,
                    execution.text_measurer(),
                )
            })?)
        }
        #[cfg(feature = "diagram-railroad")]
        RenderSemanticModel::Railroad(model) => {
            BuiltinFamilyArtifact::Railroad(prepare_pair(model, |model| {
                crate::railroad::layout_railroad_diagram_typed_for_type(
                    model,
                    diagram_type,
                    effective_config,
                    execution.text_measurer(),
                )
            })?)
        }
        #[cfg(feature = "diagram-kanban")]
        RenderSemanticModel::Kanban(model) => {
            BuiltinFamilyArtifact::Kanban(prepare_pair(model, |model| {
                crate::kanban::prepare_kanban_diagram_typed_with_work_meter(
                    model,
                    &meta.effective_config,
                    execution.text_measurer(),
                    execution.work_meter_ref(),
                )
            })?)
        }
        #[cfg(feature = "diagram-gantt")]
        RenderSemanticModel::Gantt(model) => {
            BuiltinFamilyArtifact::Gantt(prepare_pair(model, |model| {
                crate::gantt::layout_gantt_diagram_typed(
                    model,
                    title,
                    effective_config,
                    execution.text_measurer(),
                    execution.container_width,
                    execution.local_time_zone(),
                )
            })?)
        }
        #[cfg(feature = "diagram-pie")]
        RenderSemanticModel::Pie(model) => {
            BuiltinFamilyArtifact::Pie(prepare_pair(model, |model| {
                crate::pie::layout_pie_diagram_typed(
                    model,
                    title,
                    effective_config,
                    execution.text_measurer(),
                )
            })?)
        }
        #[cfg(feature = "diagram-packet")]
        RenderSemanticModel::Packet(model) => {
            BuiltinFamilyArtifact::Packet(prepare_pair(model, |model| {
                crate::packet::layout_packet_diagram_typed(
                    model,
                    title,
                    effective_config,
                    execution.text_measurer(),
                )
            })?)
        }
        #[cfg(feature = "diagram-timeline")]
        RenderSemanticModel::Timeline(model) => {
            BuiltinFamilyArtifact::Timeline(prepare_pair(model, |model| {
                crate::timeline::layout_timeline_diagram_typed(
                    model,
                    effective_config,
                    execution.text_measurer(),
                )
            })?)
        }
        #[cfg(feature = "diagram-journey")]
        RenderSemanticModel::Journey(model) => {
            BuiltinFamilyArtifact::Journey(prepare_pair(model, |model| {
                crate::journey::layout_journey_diagram_typed(
                    model,
                    effective_config,
                    execution.text_measurer(),
                )
            })?)
        }
        #[cfg(feature = "diagram-requirement")]
        RenderSemanticModel::Requirement(model) => {
            BuiltinFamilyArtifact::Requirement(prepare_pair(model, |model| {
                crate::requirement::layout_requirement_diagram_typed_with_work_meter(
                    model,
                    effective_config,
                    execution.text_measurer(),
                    execution.work_meter(),
                    #[cfg(feature = "layout-elk")]
                    execution.elk_operation_seed(),
                )
            })?)
        }
        #[cfg(feature = "diagram-sankey")]
        RenderSemanticModel::Sankey(model) => {
            BuiltinFamilyArtifact::Sankey(prepare_pair(model, |model| {
                crate::sankey::layout_sankey_diagram_typed_with_work_meter(
                    model,
                    effective_config,
                    execution.text_measurer(),
                    execution.work_meter_ref(),
                )
            })?)
        }
        #[cfg(feature = "diagram-radar")]
        RenderSemanticModel::Radar(model) => {
            BuiltinFamilyArtifact::Radar(prepare_pair(model, |model| {
                crate::radar::layout_radar_diagram_typed_with_work_meter(
                    model,
                    effective_config,
                    execution.text_measurer(),
                    execution.work_meter_ref(),
                )
            })?)
        }
        #[cfg(feature = "diagram-info")]
        RenderSemanticModel::Info(model) => {
            BuiltinFamilyArtifact::Info(prepare_pair(model, |model| {
                crate::info::layout_info_diagram_typed(
                    model,
                    effective_config,
                    execution.text_measurer(),
                )
            })?)
        }
        #[cfg(feature = "diagram-treemap")]
        RenderSemanticModel::Treemap(model) => {
            BuiltinFamilyArtifact::Treemap(prepare_pair(model, |model| {
                crate::treemap::layout_treemap_diagram_typed(
                    model,
                    title,
                    effective_config,
                    execution.text_measurer(),
                )
            })?)
        }
        #[cfg(feature = "diagram-block")]
        RenderSemanticModel::Block(model) => {
            BuiltinFamilyArtifact::Block(prepare_pair(model, |model| {
                crate::block::layout_block_diagram_typed(
                    model,
                    effective_config,
                    execution.text_measurer(),
                )
            })?)
        }
        #[cfg(feature = "diagram-er")]
        RenderSemanticModel::Er(model) => {
            #[cfg(feature = "layout-elk")]
            {
                BuiltinFamilyArtifact::Er(prepare_pair(model, |model| {
                    crate::er::layout_er_diagram_typed_with_elk_operation_seed(
                        model,
                        effective_config,
                        execution.text_measurer(),
                        execution.elk_operation_seed(),
                        execution.work_meter(),
                    )
                })?)
            }
            #[cfg(not(feature = "layout-elk"))]
            BuiltinFamilyArtifact::Er(prepare_pair(model, |model| {
                crate::er::layout_er_diagram_typed(
                    model,
                    effective_config,
                    execution.text_measurer(),
                    execution.work_meter(),
                )
            })?)
        }
        #[cfg(feature = "diagram-quadrant-chart")]
        RenderSemanticModel::QuadrantChart(model) => {
            BuiltinFamilyArtifact::QuadrantChart(prepare_pair(model, |model| {
                crate::quadrantchart::layout_quadrantchart_diagram_typed(
                    model,
                    title,
                    effective_config,
                    execution.text_measurer(),
                )
            })?)
        }
        #[cfg(feature = "diagram-xychart")]
        RenderSemanticModel::XyChart(model) => {
            BuiltinFamilyArtifact::XyChart(prepare_pair(model, |model| {
                crate::xychart::layout_xychart_diagram_typed(
                    model,
                    title,
                    effective_config,
                    execution.text_measurer(),
                )
            })?)
        }
        #[cfg(feature = "diagram-git-graph")]
        RenderSemanticModel::GitGraph(model) => {
            BuiltinFamilyArtifact::GitGraph(prepare_pair(model, |model| {
                crate::gitgraph::layout_gitgraph_diagram_typed(
                    model,
                    effective_config,
                    execution.text_measurer(),
                )
            })?)
        }
        #[cfg(feature = "diagram-tree-view")]
        RenderSemanticModel::TreeView(model) => {
            BuiltinFamilyArtifact::TreeView(prepare_pair(model, |model| {
                crate::tree_view::layout_tree_view_diagram_typed(
                    model,
                    effective_config,
                    execution.text_measurer(),
                )
            })?)
        }
        #[cfg(feature = "diagram-ishikawa")]
        RenderSemanticModel::Ishikawa(model) => {
            BuiltinFamilyArtifact::Ishikawa(prepare_pair(model, |model| {
                crate::ishikawa::layout_ishikawa_diagram_typed(
                    model,
                    effective_config,
                    execution.text_measurer(),
                )
            })?)
        }
        #[cfg(feature = "diagram-event-modeling")]
        RenderSemanticModel::EventModeling(model) => {
            BuiltinFamilyArtifact::EventModeling(prepare_pair(model, |model| {
                crate::eventmodeling::layout_eventmodeling_diagram_typed(
                    model,
                    effective_config,
                    execution.text_measurer(),
                )
            })?)
        }
        #[cfg(feature = "diagram-venn")]
        RenderSemanticModel::Venn(model) => {
            BuiltinFamilyArtifact::Venn(prepare_pair(model, |model| {
                crate::venn::layout_venn_diagram_typed_with_work_meter(
                    model,
                    title,
                    effective_config,
                    execution.work_meter_ref(),
                )
            })?)
        }
        #[cfg(feature = "diagram-usecase")]
        RenderSemanticModel::Usecase(model) => {
            BuiltinFamilyArtifact::Usecase(prepare_pair(model, |model| {
                crate::usecase::prepare_usecase_diagram(
                    model,
                    effective_config,
                    execution.text_measurer(),
                    execution.math_renderer(),
                    execution.work_meter(),
                    #[cfg(feature = "layout-elk")]
                    execution.elk_operation_seed(),
                )
            })?)
        }
        #[cfg(feature = "diagram-agentflow")]
        RenderSemanticModel::Agentflow(model) => {
            let (flowchart, render_context) = model.to_flowchart_model();
            let agentflow_config = project_agentflow_flowchart_config(&meta.effective_config);
            let flow = prepare_flowchart_artifact(
                flowchart,
                render_context,
                render_policy.flowchart(),
                flowchart_svg_label_preparation,
                |model, label_sources, svg_label_sidecar| {
                    crate::layout_flowchart_typed_with_render_labels_by_engine(
                        model,
                        label_sources,
                        &agentflow_config,
                        &execution,
                        svg_label_sidecar,
                    )
                },
            )?;
            BuiltinFamilyArtifact::Agentflow {
                semantic: Box::new(model),
                flow,
            }
        }
        RenderSemanticModel::CustomJson(_) => {
            unreachable!("custom JSON models return before built-in family dispatch")
        }
        // Another dependency can widen core without widening this renderer.
        #[allow(unreachable_patterns)]
        _ => {
            return Err(Error::UnsupportedDiagram {
                diagram_type: diagram_type.to_owned(),
            });
        }
    };
    Ok(FamilyRenderArtifact {
        metadata: meta,
        compatibility_projection: OnceLock::new(),
        family,
        required_capabilities,
        session,
    })
}

#[cfg(all(test, feature = "all-diagrams"))]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::{Arc, Mutex};

    use crate::environment::{
        HostMeasurementResult, HostTextMeasurement, HostTextMeasurementRequest, HostTextMeasurer,
        MeasurementProfileId, TextMeasurementOperation, TextMeasurementPhase,
        TextMeasurementPolicy, TextMeasurementProfileIdentity, TextMeasurementReport,
        TextMeasurementResultKind,
    };
    use crate::text::{TextMetrics, WrapMode};
    use merman_core::{
        CustomJsonProvenance, CustomJsonRenderModel, Engine, OperationControl, ParseOptions,
    };
    use serde_json::{Value, json};

    fn custom_semantic_parser(
        _code: &str,
        meta: &ParseMetadata,
        control: &merman_core::OperationControl,
    ) -> merman_core::OperationControlResult<merman_core::Result<Value>> {
        control.checkpoint()?;
        Ok(Ok(
            json!({ "type": meta.diagram_type, "owner": "semantic" }),
        ))
    }

    fn custom_render_parser(
        _code: &str,
        _meta: &ParseMetadata,
        control: &merman_core::OperationControl,
    ) -> merman_core::OperationControlResult<merman_core::Result<CustomJsonRenderModel>> {
        control.checkpoint()?;
        Ok(Ok(CustomJsonRenderModel::new(
            "custom-flowchart",
            json!({ "owner": "render" }),
        )))
    }

    fn session() -> RenderSession {
        crate::environment::RenderEnvironment::deterministic()
            .begin_session()
            .unwrap()
    }

    /// Decodes the `data-points` attribute the SVG emitter writes on every edge `<path>`.
    ///
    /// Deliberately an independent reader: the equivalence assertion below is only meaningful
    /// if it goes through the serialized form rather than the in-memory value it came from.
    fn decode_data_points(svg: &str) -> Vec<(String, Vec<(f64, f64)>)> {
        let mut out = Vec::new();
        for chunk in svg.split("<path").skip(1) {
            let Some(tag) = chunk.split('>').next() else {
                continue;
            };
            if !tag.contains("data-points=") {
                continue;
            }
            let id = tag
                .split("data-id=\"")
                .nth(1)
                .and_then(|rest| rest.split('"').next())
                .unwrap_or_default()
                .to_owned();
            let encoded = tag
                .split("data-points=\"")
                .nth(1)
                .and_then(|rest| rest.split('"').next())
                .expect("edge path carries data-points");
            use base64::Engine as _;
            let raw = base64::engine::general_purpose::STANDARD
                .decode(encoded)
                .expect("data-points is base64");
            let points: Vec<serde_json::Value> =
                serde_json::from_slice(&raw).expect("data-points is JSON");
            out.push((
                id,
                points
                    .iter()
                    .map(|p| {
                        (
                            p["x"].as_f64().expect("point x"),
                            p["y"].as_f64().expect("point y"),
                        )
                    })
                    .collect(),
            ));
        }
        out
    }

    fn decode_edge_label_positions(svg: &str) -> std::collections::HashMap<String, (f64, f64)> {
        let document = roxmltree::Document::parse(svg).expect("valid flowchart SVG");
        let mut positions = std::collections::HashMap::new();
        for edge_label in document.descendants().filter(|node| {
            node.has_tag_name("g")
                && node.attribute("class").is_some_and(|class| {
                    class
                        .split_ascii_whitespace()
                        .any(|part| part == "edgeLabel")
                })
        }) {
            let Some(transform) = edge_label.attribute("transform") else {
                continue;
            };
            let Some((x, y)) = transform
                .strip_prefix("translate(")
                .and_then(|value| value.strip_suffix(')'))
                .and_then(|value| {
                    let (x, y) = value.split_once(',')?;
                    Some((x.trim().parse().ok()?, y.trim().parse().ok()?))
                })
            else {
                continue;
            };
            let Some(label) = edge_label.descendants().find(|node| {
                node.has_tag_name("g")
                    && node.attribute("class").is_some_and(|class| {
                        class.split_ascii_whitespace().any(|part| part == "label")
                    })
                    && node.attribute("data-id").is_some()
            }) else {
                continue;
            };
            positions.insert(label.attribute("data-id").unwrap().to_owned(), (x, y));
        }
        positions
    }

    fn parse_flowchart(source: &str) -> ParsedDiagramRender {
        Engine::new()
            .parse_diagram_for_render_model_sync(source, ParseOptions::strict())
            .expect("parse flowchart")
            .expect("detect flowchart")
    }

    #[test]
    fn edge_geometry_json_points_match_the_emitted_data_points() {
        // Circle endpoints exercise the difference between `layoutJson.points` and the clipped
        // polyline reported by the renderer. The reverse labelled edge also keeps this aligned
        // with the minimal reproduction from issue #172.
        let source = r#"flowchart TD
    S[Start] --> M((Circle))
    M -->|back| S
"#;

        let geometry = prepare(
            parse_flowchart(source),
            &LayoutOptions::default(),
            session(),
        )
        .expect("prepare flowchart")
        .edge_geometry_json()
        .expect("edge geometry json");
        let svg = prepare(
            parse_flowchart(source),
            &LayoutOptions::default(),
            session(),
        )
        .expect("prepare flowchart")
        .render_svg(&SvgRenderOptions::default(), &SvgDebugOptions::default())
        .expect("render flowchart")
        .svg()
        .to_owned();

        let from_svg = decode_data_points(&svg);
        assert!(
            !from_svg.is_empty(),
            "the emitter must have written data-points for this diagram"
        );
        assert_eq!(
            geometry.len(),
            from_svg.len(),
            "every rendered edge must have a structured counterpart"
        );

        let mut compared = 0;
        for edge in &geometry {
            let Some((_, expected)) = from_svg.iter().find(|(id, _)| id == &edge.id) else {
                continue;
            };
            let actual = edge
                .points
                .as_ref()
                .unwrap_or_else(|| panic!("edge {} must have clipped points", edge.id));
            assert_eq!(actual.len(), expected.len(), "edge {} point count", edge.id);
            for (index, (point, (ex, ey))) in actual.iter().zip(expected).enumerate() {
                let (ax, ay) = (point.x, point.y);
                assert_eq!(ax, *ex, "edge {} point {index} x", edge.id);
                assert_eq!(ay, *ey, "edge {} point {index} y", edge.id);
            }
            compared += 1;
        }
        assert_eq!(compared, from_svg.len(), "every SVG edge was compared");

        // The clipped polyline must differ from the layout polyline here, otherwise this test
        // would be asserting equality of two things that were never different.
        let layout = prepare(
            parse_flowchart(source),
            &LayoutOptions::default(),
            session(),
        )
        .expect("prepare flowchart")
        .layout_json()
        .expect("layout json");
        let layout_edges = layout["layout"]["FlowchartV2"]["edges"]
            .as_array()
            .expect("layout edges");
        let differed = geometry.iter().filter(|edge| {
            layout_edges
                .iter()
                .find(|candidate| candidate["id"] == edge.id.as_str())
                .and_then(|candidate| candidate["points"].as_array())
                .zip(edge.points.as_ref())
                .is_some_and(|(layout_points, clipped)| {
                    layout_points.len() != clipped.len()
                        || layout_points.iter().zip(clipped).any(|(l, c)| {
                            l["x"].as_f64() != Some(c.x) || l["y"].as_f64() != Some(c.y)
                        })
                })
        });
        assert!(
            differed.count() > 0,
            "this diagram must exercise clipping, otherwise it proves nothing"
        );
    }

    #[test]
    fn edge_geometry_json_label_positions_match_emitted_positions() {
        let cases = [
            (
                "issue_176_three_parallel_unequal_labels",
                "flowchart TD\n    A -->|one| B\n    A -->|two| B\n    A -->|three| B\n",
                3,
            ),
            (
                "two_parallel_unequal_labels",
                "flowchart TD\n    A -->|short| B\n    A -->|a substantially wider edge label| B\n",
                2,
            ),
            (
                "three_parallel_equal_labels",
                "flowchart TD\n    A -->|same| B\n    A -->|same| B\n    A -->|same| B\n",
                3,
            ),
        ];

        for (case, source, expected_edges) in cases {
            let geometry = prepare(
                parse_flowchart(source),
                &LayoutOptions::default(),
                session(),
            )
            .expect("prepare flowchart")
            .edge_geometry_json()
            .expect("edge geometry json");
            let svg = prepare(
                parse_flowchart(source),
                &LayoutOptions::default(),
                session(),
            )
            .expect("prepare flowchart")
            .render_svg(&SvgRenderOptions::default(), &SvgDebugOptions::default())
            .expect("render flowchart")
            .svg()
            .to_owned();
            let rendered = decode_edge_label_positions(&svg);
            let positions: Vec<_> = geometry
                .iter()
                .map(|edge| {
                    let actual = edge.label_position.as_ref().unwrap_or_else(|| {
                        panic!("{case}: edge {} must have a label position", edge.id)
                    });
                    let (rendered_x, rendered_y) = rendered
                        .get(&edge.id)
                        .unwrap_or_else(|| panic!("{case}: SVG must contain label {}", edge.id));
                    (
                        edge.id.as_str(),
                        actual.x,
                        actual.y,
                        *rendered_x,
                        *rendered_y,
                    )
                })
                .collect();

            assert_eq!(positions.len(), expected_edges, "{case}: edge count");
            assert_eq!(
                rendered.len(),
                expected_edges,
                "{case}: rendered label count"
            );

            // SVG labels are emitted in the root's translated coordinate space, while this API
            // intentionally reports layout coordinates. Their pairwise deltas must therefore
            // match exactly even when the whole diagram has a non-zero translation.
            for left in 0..positions.len() {
                for right in left + 1..positions.len() {
                    let (left_id, lx, ly, lrx, lry) = positions[left];
                    let (right_id, rx, ry, rrx, rry) = positions[right];
                    assert!(
                        ((rx - lx) - (rrx - lrx)).abs() < 1e-5,
                        "{case}: x delta for {left_id} -> {right_id} differs between geometry and SVG"
                    );
                    assert!(
                        ((ry - ly) - (rry - lry)).abs() < 1e-5,
                        "{case}: y delta for {left_id} -> {right_id} differs between geometry and SVG"
                    );
                }
            }
        }
    }

    #[test]
    fn agentflow_layout_config_prioritizes_family_measurement_keys() {
        let config = merman_core::MermaidConfig::from_value(json!({
            "agentflow": { "minNodeWidth": 180, "wrappingWidth": 240 },
            "flowchart": { "minNodeWidth": 90 }
        }));
        let projected = project_agentflow_flowchart_config(&config);
        let number = |config: &merman_core::MermaidConfig, path: &str| {
            config
                .as_value()
                .pointer(path)
                .and_then(serde_json::Value::as_f64)
        };
        assert_eq!(number(&projected, "/flowchart/minNodeWidth"), Some(180.0));
        assert_eq!(number(&projected, "/flowchart/wrappingWidth"), Some(240.0));
        assert_eq!(number(&config, "/flowchart/wrappingWidth"), None);
        assert_eq!(number(&config, "/agentflow/minNodeWidth"), Some(180.0));
    }

    #[test]
    fn agentflow_layout_config_keeps_flowchart_wrapping_fallback() {
        for wrapping in [json!(0), json!(null), json!(false), json!("")] {
            let config = merman_core::MermaidConfig::from_value(json!({
                "agentflow": { "minNodeWidth": 0, "wrappingWidth": wrapping },
                "flowchart": { "minNodeWidth": 90, "wrappingWidth": 240 }
            }));
            let projected = project_agentflow_flowchart_config(&config);
            assert_eq!(projected.as_value()["flowchart"]["minNodeWidth"], 0);
            assert_eq!(projected.as_value()["flowchart"]["wrappingWidth"], 240);
        }
    }

    #[test]
    fn agentflow_projection_separates_renderer_ownership_from_shared_shape_settings() {
        for family in [json!({}), json!(null)] {
            let config = merman_core::MermaidConfig::from_value(json!({
                "agentflow": family,
                "flowchart": {
                    "diagramPadding": 99, "titleTopMargin": 99, "useMaxWidth": false,
                    "nodeSpacing": 99, "rankSpacing": 99,
                    "padding": 17, "curve": "linear", "htmlLabels": false,
                    "inheritDir": true, "subGraphTitleMargin": {"top": 7}
                }
            }));
            let projected = project_agentflow_flowchart_config(&config);
            let flow = &projected.as_value()["flowchart"];
            assert_eq!(flow["diagramPadding"], 8);
            assert_eq!(flow["titleTopMargin"], 0);
            assert_eq!(flow["useMaxWidth"], true);
            assert_eq!(flow["nodeSpacing"], 50);
            assert_eq!(flow["rankSpacing"], 50);
            for key in [
                "padding",
                "curve",
                "htmlLabels",
                "inheritDir",
                "subGraphTitleMargin",
            ] {
                assert_eq!(flow[key], config.as_value()["flowchart"][key]);
            }
        }
        for value in [json!(null), json!(0)] {
            let config = merman_core::MermaidConfig::from_value(json!({
                "nodeSpacing": 90, "rankSpacing": 0,
                "agentflow": {
                    "diagramPadding": value, "titleTopMargin": value, "useMaxWidth": false,
                    "nodeSpacing": 200, "rankSpacing": value
                }
            }));
            let projected = project_agentflow_flowchart_config(&config);
            let flow = &projected.as_value()["flowchart"];
            assert_eq!(flow["diagramPadding"], if value.is_null() { 8 } else { 0 });
            assert_eq!(flow["titleTopMargin"], 0);
            assert_eq!(flow["useMaxWidth"], false);
            assert_eq!(flow["nodeSpacing"], 90);
            assert_eq!(flow["rankSpacing"], 50);
        }
    }

    #[test]
    fn agentflow_spacing_projection_follows_registered_backend_selection() {
        for layout in [
            "dagre",
            "unknown",
            "elk.layered",
            "elk",
            "elk.stress",
            "elk.force",
            "elk.mrtree",
            "elk.sporeOverlap",
            "elk.box",
            "elk.rectpacking",
        ] {
            let config = merman_core::MermaidConfig::from_value(json!({
                "layout": layout,
                "nodeSpacing": 90, "rankSpacing": 110,
                "agentflow": {"nodeSpacing": 200, "rankSpacing": 210},
                "flowchart": {"nodeSpacing": 300, "rankSpacing": 310}
            }));
            let selected = crate::layout_backend::resolve_graph_layout(config.as_value());
            let projected = project_agentflow_flowchart_config(&config);
            let flow = &projected.as_value()["flowchart"];
            let elk_registered = cfg!(feature = "layout-elk")
                && crate::layout_backend::ElkRootAlgorithm::from_name(layout).is_some();
            assert_eq!(
                selected.backend == crate::layout_backend::GraphLayoutBackend::Elk,
                elk_registered,
                "{layout}"
            );
            assert_eq!(
                flow["nodeSpacing"],
                if elk_registered { 50 } else { 90 },
                "{layout}"
            );
            assert_eq!(
                flow["rankSpacing"],
                if elk_registered { 50 } else { 110 },
                "{layout}"
            );
        }
    }

    #[test]
    fn final_svg_admission_prefers_emit_cancellation_to_byte_limit() {
        let policy = crate::resources::RenderResourcePolicy::unbounded_for_trusted_input()
            .with_limit(crate::resources::ResourceLimitId::MaxSvgBytes, 1)
            .unwrap();
        let control = OperationControl::new();
        let session = crate::environment::RenderEnvironment::deterministic()
            .with_resource_policy(policy)
            .begin_session_with_control(control.clone())
            .unwrap();
        let svg = "<svg/>";

        let limit = session
            .resource_policy()
            .check_svg_bytes(svg, ResourceLimitPhase::SvgOutput)
            .unwrap_err();
        assert_eq!(limit.limit, "max_svg_bytes");

        control.cancel();
        let error = admit_rendered_svg_output(&session, svg).unwrap_err();
        let Error::Cancelled(error) = error else {
            panic!("expected final SVG admission cancellation");
        };
        assert_eq!(error.phase, OperationPhase::Emit);
    }

    #[test]
    fn final_svg_resource_terminal_replays_before_later_cancellation() {
        let policy = crate::resources::RenderResourcePolicy::unbounded_for_trusted_input()
            .with_limit(crate::resources::ResourceLimitId::MaxSvgBytes, 1)
            .unwrap();
        let control = OperationControl::new();
        let session = crate::environment::RenderEnvironment::deterministic()
            .with_resource_policy(policy)
            .begin_session_with_control(control.clone())
            .unwrap();
        let svg = "<svg/>";

        let first = admit_rendered_svg_output(&session, svg)
            .expect_err("the formal SVG output admission must reject");
        let Error::ResourceLimitExceeded(first_limit) = first else {
            panic!("expected a resource rejection");
        };
        assert_eq!(first_limit.limit, "max_svg_bytes");
        assert_eq!(first_limit.actual, svg.len());
        assert_eq!(first_limit.max, 1);

        control.cancel();
        let replayed = admit_rendered_svg_output(&session, svg)
            .expect_err("the first SVG output terminal must remain sticky");
        let Error::ResourceLimitExceeded(replayed_limit) = replayed else {
            panic!("expected the resource terminal to replay");
        };
        assert_eq!(replayed_limit, first_limit);
    }

    #[test]
    fn requested_diagram_id_fanout_is_admitted_per_output_occurrence() {
        let diagram_id = "d".repeat(256);
        let maximum = diagram_id.len() * 4;
        let policy = crate::resources::RenderResourcePolicy::unbounded_for_trusted_input()
            .with_limit(crate::resources::ResourceLimitId::MaxSvgBytes, maximum)
            .unwrap();
        let parsed = Engine::new()
            .parse_diagram_for_render_model_sync("info", ParseOptions::strict())
            .expect("parse info diagram")
            .expect("detect info diagram");
        let session = crate::environment::RenderEnvironment::deterministic()
            .with_resource_policy(policy)
            .begin_session()
            .expect("begin render session");
        let artifact =
            prepare(parsed, &LayoutOptions::default(), session).expect("prepare info diagram");
        let error = match artifact.render_svg(
            &SvgRenderOptions {
                diagram_id: Some(diagram_id.clone()),
                ..SvgRenderOptions::default()
            },
            &SvgDebugOptions::default(),
        ) {
            Ok(_) => panic!("the diagram ID fanout must exceed the SVG byte ceiling"),
            Err(error) => error,
        };

        let Error::ResourceLimitExceeded(details) = error else {
            panic!("expected the family fanout preflight to reject");
        };
        assert_eq!(details.limit, "max_svg_bytes");
        assert_eq!(details.max, maximum);
        assert_eq!(details.actual, maximum + diagram_id.len());
    }

    #[test]
    fn renderer_owned_metadata_scan_observes_the_artifact_session_control() {
        let parsed = Engine::new()
            .parse_diagram_for_render_model_sync("info", ParseOptions::strict())
            .expect("parse info diagram")
            .expect("detect info diagram");
        let control = OperationControl::new();
        let session = crate::environment::RenderEnvironment::deterministic()
            .begin_session_with_control(control.clone())
            .expect("begin render session");
        let rendered = prepare(parsed, &LayoutOptions::default(), session)
            .expect("prepare info diagram")
            .render_svg(&SvgRenderOptions::default(), &SvgDebugOptions::default())
            .expect("render info diagram");

        control.cancel();
        let error = rendered
            .output_metadata()
            .expect_err("metadata extraction must observe the artifact session control");

        let Error::Cancelled(cancelled) = error else {
            panic!("expected structured postprocess cancellation");
        };
        assert_eq!(cancelled.phase, OperationPhase::Postprocess);
    }

    fn text_measurement_call_count(session: &RenderSession) -> u64 {
        session
            .text_measurement_report()
            .entries()
            .iter()
            .map(crate::environment::TextMeasurementSummary::count)
            .sum()
    }

    #[derive(Debug, Clone, Copy)]
    enum SidecarHostOutcome {
        Success,
        Missing,
    }

    #[derive(Debug, Clone, PartialEq, Eq)]
    struct SidecarHostRequest {
        ordinal: usize,
        phase: TextMeasurementPhase,
        operation: TextMeasurementOperation,
        result_kind: TextMeasurementResultKind,
        text: String,
        font_size_bits: u64,
        max_width_bits: Option<u64>,
        wrap_mode: WrapMode,
    }

    struct SidecarRecordingHost {
        outcome: SidecarHostOutcome,
        requests: Mutex<Vec<SidecarHostRequest>>,
    }

    impl SidecarRecordingHost {
        fn new(outcome: SidecarHostOutcome) -> Self {
            Self {
                outcome,
                requests: Mutex::new(Vec::new()),
            }
        }

        fn snapshot(&self) -> Vec<SidecarHostRequest> {
            self.requests.lock().expect("host request trace").clone()
        }
    }

    impl HostTextMeasurer for SidecarRecordingHost {
        fn measure(&self, request: HostTextMeasurementRequest<'_>) -> HostMeasurementResult {
            let ordinal = {
                let mut requests = self.requests.lock().expect("host request trace");
                let ordinal = requests.len();
                requests.push(SidecarHostRequest {
                    ordinal,
                    phase: request.phase,
                    operation: request.operation,
                    result_kind: request.operation.required_result_kind(),
                    text: request.text.to_string(),
                    font_size_bits: request.style.font_size.to_bits(),
                    max_width_bits: request.max_width.map(f64::to_bits),
                    wrap_mode: request.wrap_mode,
                });
                ordinal
            };

            match self.outcome {
                SidecarHostOutcome::Success => Ok(Some(sidecar_host_measurement(request, ordinal))),
                SidecarHostOutcome::Missing => Ok(None),
            }
        }
    }

    struct CancellingSuccessfulHost {
        calls: AtomicUsize,
        control: OperationControl,
    }

    impl HostTextMeasurer for CancellingSuccessfulHost {
        fn measure(&self, request: HostTextMeasurementRequest<'_>) -> HostMeasurementResult {
            self.calls.fetch_add(1, Ordering::Relaxed);
            self.control.cancel();
            Ok(Some(sidecar_host_measurement(request, 0)))
        }
    }

    fn sidecar_host_measurement(
        request: HostTextMeasurementRequest<'_>,
        ordinal: usize,
    ) -> HostTextMeasurement {
        let state_delta = (ordinal % 7) as f64 / 32.0;
        let raw_width = request
            .text
            .lines()
            .map(|line| line.chars().count() as f64 * 8.0)
            .fold(0.0_f64, f64::max)
            + state_delta;
        let max_width = request
            .max_width
            .filter(|width| width.is_finite() && *width > 0.0);
        let line_count = max_width
            .map(|width| (raw_width / width).ceil() as usize)
            .unwrap_or(1)
            .max(request.text.lines().count())
            .max(1)
            .min(request.text.len().saturating_add(1));
        let metrics = TextMetrics {
            width: max_width.map_or(raw_width, |width| raw_width.min(width)),
            height: line_count as f64 * 20.0 + state_delta,
            line_count,
        };

        match request.operation.required_result_kind() {
            TextMeasurementResultKind::Metrics => HostTextMeasurement::Metrics(metrics),
            TextMeasurementResultKind::Length => {
                let length = match request.operation {
                    TextMeasurementOperation::RawBBoxHeight
                    | TextMeasurementOperation::SimpleBBoxHeight
                    | TextMeasurementOperation::TspanBBoxHeight => metrics.height,
                    TextMeasurementOperation::CreateTextBBoxYOffset
                    | TextMeasurementOperation::CreateTextMiddleBBoxYOffset => 0.0,
                    _ => raw_width,
                };
                HostTextMeasurement::Length(length)
            }
            TextMeasurementResultKind::HorizontalExtents => {
                HostTextMeasurement::HorizontalExtents {
                    left: raw_width / 2.0,
                    right: raw_width / 2.0,
                }
            }
            TextMeasurementResultKind::WrappedWithRawWidth => {
                HostTextMeasurement::WrappedWithRawWidth {
                    metrics,
                    raw_width: Some(raw_width),
                }
            }
        }
    }

    #[test]
    fn family_layout_stops_host_measurement_after_callback_cancellation() {
        let parsed = Engine::new()
            .parse_diagram_for_render_model_sync(
                "---\nconfig:\n  layout: tidy-tree\n---\nmindmap\n  Root\n    First child\n    Second child\n",
                ParseOptions::strict(),
            )
            .expect("parse mindmap")
            .expect("detect mindmap");
        let control = OperationControl::new();
        let host = Arc::new(CancellingSuccessfulHost {
            calls: AtomicUsize::new(0),
            control: control.clone(),
        });
        let identity = TextMeasurementProfileIdentity::new(
            MeasurementProfileId::new("test.family-cancelling-host").expect("profile id"),
            "1",
        )
        .expect("profile identity");
        let session = crate::environment::RenderEnvironment::deterministic()
            .with_text_measurement_policy(TextMeasurementPolicy::host_display(
                identity,
                host.clone(),
                TextMeasurementPhase::ALL,
            ))
            .begin_session_with_control(control)
            .expect("begin render session");

        let result = prepare(parsed, &LayoutOptions::default(), session);
        let Err(Error::Cancelled(cancelled)) = result else {
            panic!("family preparation must surface callback cancellation");
        };
        assert_eq!(cancelled.phase, OperationPhase::Layout);
        assert_eq!(host.calls.load(Ordering::Relaxed), 1);
    }

    fn render_with_sidecar_host_control(
        preparation: FlowchartSvgLabelPreparation,
        outcome: SidecarHostOutcome,
    ) -> (bool, Vec<SidecarHostRequest>, TextMeasurementReport, String) {
        let source = r#"---
config:
  htmlLabels: false
  flowchart:
    htmlLabels: false
    wrappingWidth: 96
---
flowchart LR
subgraph S["service words"]
  A["alpha beta<br/>gamma"]
end
A traced-edge@-->|edge label words| B["delta epsilon"]
"#;
        let parsed = Engine::new()
            .parse_diagram_for_render_model_sync(source, ParseOptions::strict())
            .expect("parse flowchart")
            .expect("detect flowchart");
        let identity = TextMeasurementProfileIdentity::new(
            MeasurementProfileId::new("test.flowchart-sidecar-control").expect("profile id"),
            "1",
        )
        .expect("profile identity");
        let host = Arc::new(SidecarRecordingHost::new(outcome));
        let environment = crate::environment::RenderEnvironment::deterministic()
            .with_text_measurement_policy(TextMeasurementPolicy::host_display(
                identity,
                host.clone(),
                TextMeasurementPhase::ALL,
            ));
        let session = environment.begin_session().expect("render session");
        let artifact = prepare_with_render_policy_impl(
            parsed,
            &LayoutOptions::default(),
            session,
            PresentationRenderPolicy::default(),
            preparation,
        )
        .expect("prepare flowchart artifact");
        let indexed_sidecar = match &artifact.family {
            #[cfg(any(feature = "diagram-flowchart", feature = "diagram-swimlane"))]
            BuiltinFamilyArtifact::Flowchart(flowchart) => {
                flowchart
                    .svg_label_sidecar()
                    .node_owner("A", false)
                    .is_some()
                    && flowchart
                        .svg_label_sidecar()
                        .edge_owner("traced-edge", false)
                        .is_some()
            }
            _ => panic!("expected Flowchart family artifact"),
        };
        let rendered = artifact
            .render_svg(&SvgRenderOptions::default(), &SvgDebugOptions::default())
            .expect("render Flowchart SVG");
        let trace = host.snapshot();
        let (svg, _, _, session) = rendered.into_parts();
        (
            indexed_sidecar,
            trace,
            session.text_measurement_report(),
            svg,
        )
    }

    #[cfg(feature = "layout-cytoscape")]
    fn prepare_mindmap_with_host_limit(
        max_layout_work_units: Option<usize>,
    ) -> (Result<FamilyRenderArtifact>, Arc<SidecarRecordingHost>) {
        let source = "mindmap\n  Root\n    First child\n    Second child\n";
        let parsed = Engine::new()
            .parse_diagram_for_render_model_sync(source, ParseOptions::strict())
            .expect("parse mindmap")
            .expect("detect mindmap");
        let identity = TextMeasurementProfileIdentity::new(
            MeasurementProfileId::new("test.mindmap-cose-budget").expect("profile id"),
            "1",
        )
        .expect("profile identity");
        let host = Arc::new(SidecarRecordingHost::new(SidecarHostOutcome::Success));
        let mut environment = crate::environment::RenderEnvironment::deterministic()
            .with_text_measurement_policy(TextMeasurementPolicy::host_display(
                identity,
                host.clone(),
                TextMeasurementPhase::ALL,
            ));
        if let Some(limit) = max_layout_work_units {
            let policy = crate::resources::RenderResourcePolicy::unbounded_for_trusted_input()
                .with_limit(crate::resources::ResourceLimitId::MaxLayoutWorkUnits, limit)
                .expect("layout work limit");
            environment = environment.with_resource_policy(policy);
        }
        let session = environment.begin_session().expect("render session");
        (prepare(parsed, &LayoutOptions::default(), session), host)
    }

    #[test]
    fn public_flowchart_preparation_enables_prepared_svg_label_reuse() {
        let source = r#"---
config:
  htmlLabels: false
  flowchart:
    htmlLabels: false
---
flowchart LR
A -->|control label| B
"#;
        let parsed = Engine::new()
            .parse_diagram_for_render_model_sync(source, ParseOptions::strict())
            .expect("parse flowchart")
            .expect("detect flowchart");
        let artifact = prepare_with_render_policy(
            parsed,
            &LayoutOptions::default(),
            session(),
            PresentationRenderPolicy::default(),
        )
        .expect("prepare public flowchart artifact");
        let BuiltinFamilyArtifact::Flowchart(flowchart) = &artifact.family else {
            panic!("expected Flowchart family artifact");
        };

        assert!(
            flowchart
                .svg_label_sidecar()
                .node_owner("A", false)
                .is_some(),
            "the public Flowchart preparation path must build the label sidecar"
        );
    }

    #[test]
    fn routed_host_and_fallback_traces_match_a_no_sidecar_family_control() {
        for outcome in [SidecarHostOutcome::Success, SidecarHostOutcome::Missing] {
            let (control_indexed, control_trace, control_report, control_svg) =
                render_with_sidecar_host_control(FlowchartSvgLabelPreparation(false), outcome);
            let (sidecar_indexed, sidecar_trace, sidecar_report, sidecar_svg) =
                render_with_sidecar_host_control(FlowchartSvgLabelPreparation(true), outcome);

            assert!(
                !control_indexed,
                "the control must bypass sidecar preparation"
            );
            assert!(
                sidecar_indexed,
                "the candidate must prepare semantic owners"
            );
            assert!(!control_trace.is_empty());
            assert_eq!(sidecar_trace, control_trace);
            assert_eq!(sidecar_report, control_report);
            assert_eq!(sidecar_svg, control_svg);
        }
    }

    #[test]
    fn prepared_self_loop_edge_label_keeps_its_semantic_owner_through_family_dispatch() {
        let source = r#"---
config:
  htmlLabels: false
  flowchart:
    htmlLabels: false
---
flowchart LR
A ordinary-edge@-->|ordinary owner sentinel| B
A self-loop-edge@-->|self loop semantic owner keeps wrapped label rows through the logical render id alpha beta gamma delta epsilon zeta eta theta iota kappa lambda mu nu xi omicron| A
"#;
        let parsed = Engine::new()
            .parse_diagram_for_render_model_sync(source, ParseOptions::strict())
            .expect("parse flowchart")
            .expect("detect flowchart");
        let artifact = prepare_with_render_policy_impl(
            parsed,
            &LayoutOptions::default(),
            session(),
            PresentationRenderPolicy::default(),
            FlowchartSvgLabelPreparation(true),
        )
        .expect("prepare flowchart family artifact");

        let rendered_svg = {
            let BuiltinFamilyArtifact::Flowchart(flowchart) = &artifact.family else {
                panic!("expected Flowchart family artifact");
            };
            let model = crate::flowchart::FlowchartRenderModelRef::new(
                flowchart.pair().semantic(),
                flowchart.render_context(),
            );
            let edge = model.edges.get(1).expect("self-loop edge");
            assert_eq!(edge.id, "self-loop-edge");
            let label = model
                .edge_label_for_render(edge)
                .expect("self-loop edge label");
            let owner = flowchart
                .svg_label_sidecar()
                .edge_owner(edge.id.as_str(), false)
                .expect("semantic self-loop owner");
            assert_eq!(owner, crate::flowchart::FlowchartSvgLabelOwner::Edge(1));
            assert_eq!(
                flowchart
                    .svg_label_sidecar()
                    .edge_owner("A-cyclic-special-mid", false),
                None
            );
            assert!(
                flowchart
                    .pair()
                    .layout()
                    .edges
                    .iter()
                    .any(|edge| edge.id == "self-loop-edge")
            );

            let config = crate::flowchart::FlowchartConfigView::new(
                artifact.metadata.effective_config.as_value(),
            );
            let font_family = config.font_family();
            let render_style = config.render_text_style(&font_family, config.render_font_size());
            let edge_width = config.layout_settings().edge_label_wrapping_width;
            let render_measurer = artifact
                .session
                .text_measurer(crate::environment::TextMeasurementPhase::SvgBBox);
            let calls_before = text_measurement_call_count(&artifact.session);
            let plan = crate::flowchart::FlowchartSvgLabelRenderPlan::new(
                Some(flowchart.svg_label_sidecar()),
                Some(owner),
                label,
                &render_measurer,
                &render_style,
                Some(edge_width),
                true,
                crate::flowchart::FlowchartSvgWidthMode::Bbox,
            );
            assert!(matches!(
                &plan,
                crate::flowchart::FlowchartSvgLabelRenderPlan::Prepared { .. }
            ));
            let wrapped = plan.wrapped_lines();
            assert!(matches!(&wrapped, std::borrow::Cow::Borrowed(_)));
            assert!(wrapped.len() >= 2, "{wrapped:?}");
            assert_eq!(
                text_measurement_call_count(&artifact.session),
                calls_before,
                "a prepared self-loop label must not invoke the SVG measurer again"
            );
            drop(wrapped);
            drop(plan);

            let hits_before_render = flowchart.svg_label_sidecar().prepared_hit_count(owner);
            let svg = render_family_artifact_svg(
                &artifact,
                &SvgRenderOptions::default(),
                &SvgDebugOptions::default(),
                None,
            )
            .expect("render self-loop SVG");
            assert!(
                flowchart.svg_label_sidecar().prepared_hit_count(owner) > hits_before_render,
                "the real Flowchart SVG renderer must consume the prepared self-loop label"
            );
            svg
        };

        assert!(!rendered_svg.contains("cyclic-special"), "{}", rendered_svg);
        let document = roxmltree::Document::parse(&rendered_svg).expect("valid self-loop SVG");
        let logical_path = document.descendants().any(|node| {
            node.has_tag_name("path") && node.attribute("data-id") == Some("self-loop-edge")
        });
        assert!(logical_path, "{rendered_svg}");
        let label_group = document
            .descendants()
            .find(|node| {
                node.has_tag_name("g") && node.attribute("data-id") == Some("self-loop-edge")
            })
            .expect("logical self-loop label group");
        let text = label_group
            .descendants()
            .filter_map(|node| node.text().filter(|_| node.is_text()))
            .collect::<String>();
        assert!(text.contains("self loop semantic owner"), "{text:?}");
        let row_count = label_group
            .descendants()
            .filter(|node| {
                node.has_tag_name("tspan")
                    && node.attribute("class").is_some_and(|class| {
                        class
                            .split_ascii_whitespace()
                            .any(|part| part == "text-outer-tspan")
                    })
            })
            .count();
        assert!(row_count >= 2, "rows={row_count}: {rendered_svg}");
    }

    #[test]
    fn prepared_swimlane_edge_label_is_consumed_by_the_real_svg_renderer() {
        let source = r#"---
config:
  htmlLabels: false
  flowchart:
    htmlLabels: false
    wrappingWidth: 96
---
swimlane-beta LR
A styled@-->|swimlane semantic owner keeps wrapped label rows through the generated labelRect| B
linkStyle default font-size:24px,font-weight:bold
linkStyle 0 font-size:12px,font-style:italic
"#;
        let parsed = Engine::new()
            .parse_diagram_for_render_model_sync(source, ParseOptions::strict())
            .expect("parse Swimlane")
            .expect("detect Swimlane");
        let artifact = prepare_with_render_policy_impl(
            parsed,
            &LayoutOptions::default(),
            session(),
            PresentationRenderPolicy::default(),
            FlowchartSvgLabelPreparation(true),
        )
        .expect("prepare Swimlane family artifact");

        let rendered_svg = {
            let BuiltinFamilyArtifact::Swimlane(swimlane) = &artifact.family else {
                panic!("expected Swimlane family artifact");
            };
            let model = crate::flowchart::FlowchartRenderModelRef::new(
                swimlane.pair().semantic(),
                swimlane.render_context(),
            );
            let edge = model.edges.first().expect("styled Swimlane edge");
            assert_eq!(edge.id, "styled");
            let label = model
                .edge_label_for_render(edge)
                .expect("Swimlane edge label");
            let owner = swimlane
                .svg_label_sidecar()
                .edge_owner(edge.id.as_str(), true)
                .expect("semantic Swimlane edge owner");
            assert_eq!(
                owner,
                crate::flowchart::FlowchartSvgLabelOwner::SwimlaneEdgeLabel(0)
            );
            assert_eq!(
                swimlane.pair().layout().edges[0].label_node_id.as_deref(),
                Some("edge-label-A-B-styled")
            );

            let config = crate::flowchart::FlowchartConfigView::new(
                artifact.metadata.effective_config.as_value(),
            );
            let font_family = config.font_family();
            let base_style = config.render_text_style(&font_family, config.render_font_size());
            let default_edge_styles = model
                .edge_defaults
                .as_ref()
                .map_or(&[][..], |defaults| defaults.style.as_slice());
            let label_style = crate::flowchart::flowchart_swimlane_label_rect_text_style(
                &base_style,
                default_edge_styles,
                &edge.style,
            );
            let render_measurer = artifact
                .session
                .text_measurer(crate::environment::TextMeasurementPhase::SvgBBox);
            let plan = crate::flowchart::FlowchartSvgLabelRenderPlan::new(
                Some(swimlane.svg_label_sidecar()),
                Some(owner),
                label,
                &render_measurer,
                label_style.as_ref(),
                Some(config.render_wrapping_width()),
                true,
                crate::flowchart::FlowchartSvgWidthMode::Bbox,
            );
            assert!(matches!(
                &plan,
                crate::flowchart::FlowchartSvgLabelRenderPlan::Prepared { .. }
            ));
            let wrapped = plan.wrapped_lines();
            assert!(matches!(&wrapped, std::borrow::Cow::Borrowed(_)));
            assert!(wrapped.len() >= 2, "{wrapped:?}");
            drop(wrapped);
            drop(plan);

            let hits_before_render = swimlane.svg_label_sidecar().prepared_hit_count(owner);
            let svg = render_family_artifact_svg(
                &artifact,
                &SvgRenderOptions::default(),
                &SvgDebugOptions::default(),
                None,
            )
            .expect("render Swimlane SVG");
            assert!(
                swimlane.svg_label_sidecar().prepared_hit_count(owner) > hits_before_render,
                "the real Swimlane SVG renderer must consume the prepared labelRect"
            );
            svg
        };

        let document = roxmltree::Document::parse(&rendered_svg).expect("valid Swimlane SVG");
        let label_group = document
            .descendants()
            .find(|node| {
                node.has_tag_name("g") && node.attribute("id") == Some("edge-label-A-B-styled")
            })
            .expect("generated labelRect group");
        let visible = label_group
            .descendants()
            .filter_map(|node| node.text().filter(|_| node.is_text()))
            .flat_map(str::chars)
            .filter(|ch| !ch.is_whitespace())
            .collect::<String>();
        assert_eq!(
            visible,
            "swimlanesemanticownerkeepswrappedlabelrowsthroughthegeneratedlabelRect"
        );
    }

    fn prepare_with_model_item_limit(
        source: &str,
        max_model_items: usize,
    ) -> Result<FamilyRenderArtifact> {
        let parsed = Engine::new()
            .parse_diagram_for_render_model_sync(source, ParseOptions::strict())
            .unwrap()
            .expect("flowchart source should produce a render model");
        let session = crate::environment::RenderEnvironment::deterministic()
            .with_resource_policy(
                crate::resources::RenderResourcePolicy::unbounded_for_trusted_input()
                    .with_limit(
                        crate::resources::ResourceLimitId::MaxModelItems,
                        max_model_items,
                    )
                    .unwrap(),
            )
            .begin_session()
            .unwrap();
        prepare(parsed, &LayoutOptions::default(), session)
    }

    fn prepare_with_layout_work_limit(
        source: &str,
        max_layout_work_units: usize,
    ) -> Result<FamilyRenderArtifact> {
        let parsed = Engine::new()
            .parse_diagram_for_render_model_sync(source, ParseOptions::strict())
            .unwrap()
            .expect("flowchart source should produce a render model");
        let session = crate::environment::RenderEnvironment::deterministic()
            .with_resource_policy(
                crate::resources::RenderResourcePolicy::unbounded_for_trusted_input()
                    .with_limit(
                        crate::resources::ResourceLimitId::MaxLayoutWorkUnits,
                        max_layout_work_units,
                    )
                    .unwrap(),
            )
            .begin_session()
            .unwrap();
        prepare(parsed, &LayoutOptions::default(), session)
    }

    fn prepare_with_unbounded_layout_work(source: &str) -> Result<FamilyRenderArtifact> {
        let parsed = Engine::new()
            .parse_diagram_for_render_model_sync(source, ParseOptions::strict())
            .unwrap()
            .expect("source should produce a render model");
        let session = crate::environment::RenderEnvironment::deterministic()
            .with_resource_policy(
                crate::resources::RenderResourcePolicy::unbounded_for_trusted_input(),
            )
            .begin_session()
            .unwrap();
        prepare(parsed, &LayoutOptions::default(), session)
    }

    fn assert_model_item_limit(error: Error, actual: usize, max: usize) {
        let Error::ResourceLimitExceeded(limit) = error else {
            panic!("expected max_model_items resource limit error")
        };
        assert_eq!(limit.phase, ResourceLimitPhase::LayoutModel);
        assert_eq!(limit.limit, "max_model_items");
        assert_eq!(limit.actual, actual);
        assert_eq!(limit.max, max);
    }

    #[test]
    fn session_report_accounts_for_every_metered_layout_family() {
        let cases = vec![
            (
                "classDiagram\nclass A\nclass B\nA --> B\n",
                RenderFamilyKind::Class,
            ),
            (
                "stateDiagram-v2\n[*] --> Idle\nIdle --> Active\n",
                RenderFamilyKind::State,
            ),
            (
                "erDiagram\nCUSTOMER ||--o{ ORDER : places\n",
                RenderFamilyKind::Er,
            ),
            (
                "---\nconfig:\n  layout: tidy-tree\n---\nmindmap\n  Root\n    First child\n    Second child\n",
                RenderFamilyKind::Mindmap,
            ),
            (
                "sequenceDiagram\nparticipant A\nparticipant B\nA->>B: hello\n",
                RenderFamilyKind::Sequence,
            ),
            (
                "kanban\n  todo[Todo]\n    task[Task]\n",
                RenderFamilyKind::Kanban,
            ),
            (
                "requirementDiagram\nrequirement req1 {\n  id: 1\n  text: Login\n  risk: high\n}\n",
                RenderFamilyKind::Requirement,
            ),
            ("sankey-beta\nA,B,10\n", RenderFamilyKind::Sankey),
            (
                "radar-beta\naxis A,B,C\ncurve score{1,2,3}\n",
                RenderFamilyKind::Radar,
            ),
            (
                "venn-beta\nset A[\"Core\"]:20\nset B[\"Editor\"]:14\nunion A,B[\"Shared\"]:4\n",
                RenderFamilyKind::Venn,
            ),
        ];

        #[cfg(feature = "layout-cytoscape")]
        let cases = {
            let mut cases = cases;
            cases.push((
                "mindmap\n  Root\n    First child\n    Second child\n",
                RenderFamilyKind::Mindmap,
            ));
            cases
        };

        for (source, expected_family) in cases {
            let parsed = Engine::new()
                .parse_diagram_for_render_model_sync(source, ParseOptions::default())
                .unwrap()
                .expect("the layout-work fixture should produce a render model");
            let artifact = prepare(parsed, &LayoutOptions::default(), session()).unwrap();

            assert_eq!(artifact.family_kind(), expected_family);
            assert!(
                artifact.session.report().layout_work_units() > 0,
                "{expected_family} must contribute layout work to the session report"
            );
        }
    }

    #[test]
    fn dagre_family_work_budgets_are_exact_and_preserve_layout_output() {
        let cases = [
            (
                "classDiagram\nnamespace Outer {\n  class A\n  class B\n}\nA --> B\n",
                RenderFamilyKind::Class,
            ),
            (
                "stateDiagram-v2\nstate Parent {\n  [*] --> Idle\n  Idle --> Active\n}\nParent --> Outside\n",
                RenderFamilyKind::State,
            ),
            (
                "erDiagram\nNODE {\n  string id\n}\nNODE ||--o{ NODE : leads\n",
                RenderFamilyKind::Er,
            ),
        ];

        for (source, expected_family) in cases {
            let unbounded = prepare_with_unbounded_layout_work(source).unwrap();
            assert_eq!(unbounded.family_kind(), expected_family);
            let exact = unbounded.session.report().layout_work_units();
            assert!(exact > 0, "{expected_family} must report layout work");
            let expected_layout = unbounded.layout_json().unwrap();

            let bounded = prepare_with_layout_work_limit(source, exact).unwrap();
            assert_eq!(bounded.session.report().layout_work_units(), exact);
            assert_eq!(bounded.layout_json().unwrap(), expected_layout);

            let error = match prepare_with_layout_work_limit(source, exact - 1) {
                Ok(_) => panic!("{expected_family} exact minus one unexpectedly succeeded"),
                Err(error) => error,
            };
            let Error::ResourceLimitExceeded(limit) = error else {
                panic!("expected {expected_family} layout work rejection")
            };
            assert_eq!(limit.limit, "max_layout_work_units");
            assert_eq!(limit.max, exact - 1);
        }
    }

    #[cfg(feature = "layout-cytoscape")]
    #[test]
    fn default_mindmap_cose_reports_kernel_work_and_has_an_exact_resource_boundary() {
        let (unbounded_result, unbounded_host) = prepare_mindmap_with_host_limit(None);
        let unbounded = unbounded_result.expect("unbounded COSE mindmap");
        let exact = unbounded.session.report().layout_work_units();
        assert!(
            exact > 78,
            "kernel work must exceed the 3-node adapter estimate"
        );
        let unbounded_layout = unbounded.layout_json().expect("unbounded layout json");
        let unbounded_trace = unbounded_host.snapshot();
        assert!(!unbounded_trace.is_empty());

        let (exact_result, exact_host) = prepare_mindmap_with_host_limit(Some(exact));
        let exact_artifact = exact_result.expect("exact COSE budget");
        assert_eq!(exact_artifact.session.report().layout_work_units(), exact);
        assert_eq!(
            exact_artifact.layout_json().expect("exact layout json"),
            unbounded_layout
        );
        assert_eq!(exact_host.snapshot(), unbounded_trace);

        let (short_result, _short_host) = prepare_mindmap_with_host_limit(Some(exact - 1));
        let error = match short_result {
            Ok(_) => panic!("exact minus one must reject COSE work"),
            Err(error) => error,
        };
        let Error::ResourceLimitExceeded(limit) = error else {
            panic!("expected max_layout_work_units rejection")
        };
        assert_eq!(limit.limit, "max_layout_work_units");
        assert_eq!(limit.max, exact - 1);

        let (early_result, early_host) = prepare_mindmap_with_host_limit(Some(1));
        assert!(matches!(early_result, Err(Error::ResourceLimitExceeded(_))));
        assert!(
            early_host.snapshot().is_empty(),
            "adapter admission must reject before the first host measurement"
        );
    }

    #[test]
    fn requirement_layout_projection_excludes_operation_prepared_labels() {
        let parsed = Engine::new()
            .parse_diagram_for_render_model_sync(
                r#"requirementDiagram
requirement req1 {
  id: 1
  text: User logs in
  risk: high
}
element system {
  type: service
}
system - satisfies -> req1
"#,
                ParseOptions::strict(),
            )
            .unwrap()
            .expect("Requirement source should produce a render model");
        let artifact = prepare(parsed, &LayoutOptions::default(), session()).unwrap();
        let projection = artifact.layout_json().unwrap();
        let layout = &projection["layout"]["RequirementDiagram"];
        let fields = layout
            .as_object()
            .expect("Requirement layout projection should remain an object");

        assert_eq!(artifact.family_kind(), RenderFamilyKind::Requirement);
        assert!(fields.contains_key("nodes"));
        assert!(fields.contains_key("edges"));
        assert!(fields.contains_key("bounds"));
        assert!(!fields.contains_key("labels"));
        assert!(!layout.to_string().contains("display_text"));
        let serialized_projection = projection.to_string();
        assert!(!serialized_projection.contains("max_width_px"));
        assert!(!serialized_projection.contains("keep_centered"));
        assert!(!serialized_projection.contains("divider_y_offset"));
        assert!(
            serde_json::from_value::<RequirementDiagramLayout>(layout.clone()).is_ok(),
            "prepared labels must not alter the public Requirement layout schema"
        );
    }

    #[test]
    fn dagre_flowchart_node_limit_accepts_boundary_and_rejects_one_beyond() {
        let source = "flowchart TD\nA --> B";
        let artifact = prepare_with_model_item_limit(source, 3).unwrap();
        assert_eq!(artifact.family_kind(), RenderFamilyKind::Flowchart);

        let error = match prepare_with_model_item_limit(source, 2) {
            Err(error) => error,
            Ok(_) => panic!("flowchart above the node limit unexpectedly rendered"),
        };
        assert_model_item_limit(error, 3, 2);
    }

    #[test]
    fn swimlane_node_limit_accepts_boundary_and_rejects_one_beyond() {
        let source = "swimlane-beta LR\nA --> B";
        let artifact = prepare_with_model_item_limit(source, 3).unwrap();
        assert_eq!(artifact.family_kind(), RenderFamilyKind::Swimlane);

        let error = match prepare_with_model_item_limit(source, 2) {
            Err(error) => error,
            Ok(_) => panic!("swimlane above the node limit unexpectedly rendered"),
        };
        assert_model_item_limit(error, 3, 2);
    }

    #[test]
    fn swimlane_rejects_pairwise_routing_work_before_layout() {
        let source = "swimlane-beta LR\nA --> B\nB --> C";
        let artifact = prepare_with_layout_work_limit(source, 1_000).unwrap();
        assert_eq!(artifact.family_kind(), RenderFamilyKind::Swimlane);

        let error = match prepare_with_layout_work_limit(source, 1) {
            Err(error) => error,
            Ok(_) => panic!("swimlane above the layout work limit unexpectedly rendered"),
        };
        let Error::ResourceLimitExceeded(limit) = error else {
            panic!("expected max_layout_work_units resource limit error");
        };
        assert_eq!(limit.phase, ResourceLimitPhase::LayoutModel);
        assert_eq!(limit.limit, "max_layout_work_units");
        assert!(limit.actual > limit.max);
        assert_eq!(limit.max, 1);
    }

    #[test]
    fn mindmap_node_limit_is_checked_before_layout_allocation_or_backend_dispatch() {
        let parsed = Engine::new()
            .parse_diagram_for_render_model_sync(
                "mindmap\n  Root\n    First child\n    Second child\n",
                ParseOptions::strict(),
            )
            .unwrap()
            .expect("mindmap source should produce a render model");
        let session = crate::environment::RenderEnvironment::deterministic()
            .with_resource_policy(
                crate::resources::RenderResourcePolicy::unbounded_for_trusted_input()
                    .with_limit(crate::resources::ResourceLimitId::MaxModelItems, 4)
                    .unwrap(),
            )
            .begin_session()
            .unwrap();

        let error = match prepare(parsed, &LayoutOptions::default(), session) {
            Err(error) => error,
            Ok(_) => panic!("mindmap above the node limit unexpectedly reached layout"),
        };
        let Error::ResourceLimitExceeded(limit) = error else {
            panic!("expected max_model_items resource limit error");
        };
        assert_eq!(limit.phase, ResourceLimitPhase::LayoutModel);
        assert_eq!(limit.limit, "max_model_items");
        assert_eq!(limit.actual, 5);
        assert_eq!(limit.max, 4);
    }

    #[test]
    fn flowchart_math_capability_uses_parser_owned_render_spelling() {
        for source in [
            "flowchart TD\nA[\"#36;#36;node#36;#36;\"]\n",
            "flowchart TD\nA -->|#36;#36;edge#36;#36;| B\n",
            "flowchart TD\nsubgraph S[\"#36;#36;group#36;#36;\"]\nA\nend\n",
        ] {
            let parsed = Engine::new()
                .parse_diagram_for_render_model_sync(source, ParseOptions::strict())
                .unwrap()
                .expect("Flowchart source should produce a render model");
            let session = crate::environment::RenderEnvironment::deterministic()
                .without_math_renderer()
                .begin_session()
                .unwrap();

            let plan = plan_render(&parsed, &session).unwrap();
            assert!(
                !plan
                    .required_capabilities()
                    .contains(&RenderCapability::Math),
                "encoded dollar entities remain ordinary createText input: {source}"
            );
            prepare(parsed, &LayoutOptions::default(), session)
                .expect("encoded dollar entities must not require a Math renderer");
        }

        let parsed = Engine::new()
            .parse_diagram_for_render_model_sync(
                "flowchart TD\nA[\"$$x$$\"]\n",
                ParseOptions::strict(),
            )
            .unwrap()
            .expect("Flowchart source should produce a render model");
        let session = crate::environment::RenderEnvironment::deterministic()
            .without_math_renderer()
            .begin_session()
            .unwrap();
        let plan = plan_render(&parsed, &session).unwrap();
        assert_eq!(
            plan.required_capabilities(),
            if cfg!(feature = "layout-elk") {
                &[RenderCapability::LayoutElk, RenderCapability::Math][..]
            } else {
                &[RenderCapability::Math][..]
            }
        );
        assert_eq!(plan.missing_capabilities(), &[RenderCapability::Math]);
    }

    #[test]
    fn mindmap_math_label_requires_the_math_capability() {
        let source = r#"---
config:
  layout: tidy-tree
---
mindmap
  root[Root]
    formula["$$x^2$$"]
"#;
        let parsed = Engine::new()
            .parse_diagram_for_render_model_sync(source, ParseOptions::strict())
            .unwrap()
            .expect("mindmap source should produce a render model");
        let session = crate::environment::RenderEnvironment::deterministic()
            .without_math_renderer()
            .begin_session()
            .unwrap();

        let plan = plan_render(&parsed, &session).unwrap();
        assert_eq!(plan.required_capabilities(), &[RenderCapability::Math]);
        assert_eq!(plan.missing_capabilities(), &[RenderCapability::Math]);
        assert!(!plan.is_ready());

        let error = match prepare(parsed, &LayoutOptions::default(), session) {
            Err(error) => error,
            Ok(_) => panic!("mindmap math label unexpectedly rendered without a math backend"),
        };
        assert!(matches!(
            error,
            Error::MissingCapability {
                capability: RenderCapability::Math,
                ref diagram_type,
            } if diagram_type == "mindmap"
        ));
    }

    #[derive(Debug)]
    struct MindmapMathRenderer;

    impl crate::math::MathRenderer for MindmapMathRenderer {
        fn render_html_label(
            &self,
            text: &str,
            _config: &merman_core::MermaidConfig,
        ) -> Option<String> {
            text.contains("$$")
                .then(|| "<strong>rendered-mindmap-math</strong>".to_string())
        }

        fn measure_html_label(
            &self,
            text: &str,
            _config: &merman_core::MermaidConfig,
            _style: &crate::text::TextStyle,
            _max_width_px: Option<f64>,
            _wrap_mode: crate::text::WrapMode,
        ) -> Option<crate::text::TextMetrics> {
            text.contains("$$").then_some(crate::text::TextMetrics {
                width: 96.0,
                height: 24.0,
                line_count: 1,
            })
        }
    }

    #[test]
    fn mindmap_math_label_is_consumed_by_the_math_renderer() {
        let source = r#"---
config:
  layout: tidy-tree
---
mindmap
  root[Root]
    formula["$$x^2$$"]
"#;
        let parsed = Engine::new()
            .parse_diagram_for_render_model_sync(source, ParseOptions::strict())
            .unwrap()
            .expect("mindmap source should produce a render model");
        let session = crate::environment::RenderEnvironment::deterministic()
            .with_math_renderer(std::sync::Arc::new(MindmapMathRenderer))
            .begin_session()
            .unwrap();

        let plan = plan_render(&parsed, &session).unwrap();
        assert_eq!(plan.required_capabilities(), &[RenderCapability::Math]);
        assert!(plan.missing_capabilities().is_empty());
        assert!(plan.is_ready());

        let artifact = prepare(parsed, &LayoutOptions::default(), session).unwrap();
        let rendered = artifact
            .render_svg(&SvgRenderOptions::default(), &SvgDebugOptions::default())
            .unwrap();
        assert_eq!(rendered.required_capabilities(), &[RenderCapability::Math]);
        assert!(rendered.svg().contains("rendered-mindmap-math"));
        assert!(!rendered.svg().contains("$$x^2$$"));
    }

    #[test]
    fn class_math_label_requires_the_math_capability() {
        let source = r#"classDiagram
class Formula["$$x^2$$"]
"#;
        let parsed = Engine::new()
            .parse_diagram_for_render_model_sync(source, ParseOptions::strict())
            .unwrap()
            .expect("Class source should produce a render model");
        let session = crate::environment::RenderEnvironment::deterministic()
            .without_math_renderer()
            .begin_session()
            .unwrap();

        let plan = plan_render(&parsed, &session).unwrap();
        assert_eq!(
            plan.required_capabilities(),
            if cfg!(feature = "layout-elk") {
                &[RenderCapability::LayoutElk, RenderCapability::Math][..]
            } else {
                &[RenderCapability::Math][..]
            }
        );
        assert_eq!(plan.missing_capabilities(), &[RenderCapability::Math]);
        assert!(!plan.is_ready());

        let error = match prepare(parsed, &LayoutOptions::default(), session) {
            Err(error) => error,
            Ok(_) => panic!("Class math label unexpectedly rendered without a math backend"),
        };
        assert!(matches!(
            error,
            Error::MissingCapability {
                capability: RenderCapability::Math,
                ref diagram_type,
            } if diagram_type == "classDiagram"
        ));
    }

    #[derive(Debug)]
    struct ClassMathRenderer;

    impl crate::math::MathRenderer for ClassMathRenderer {
        fn render_html_label(
            &self,
            text: &str,
            _config: &merman_core::MermaidConfig,
        ) -> Option<String> {
            text.contains("$$")
                .then(|| "<div>rendered-class-math</div>".to_string())
        }

        fn measure_html_label(
            &self,
            text: &str,
            _config: &merman_core::MermaidConfig,
            _style: &crate::text::TextStyle,
            _max_width_px: Option<f64>,
            _wrap_mode: crate::text::WrapMode,
        ) -> Option<crate::text::TextMetrics> {
            text.contains("$$").then_some(crate::text::TextMetrics {
                width: 96.0,
                height: 24.0,
                line_count: 1,
            })
        }
    }

    #[test]
    fn class_math_label_is_consumed_by_the_math_renderer() {
        let source = r#"classDiagram
class Formula["$$x^2$$"]
"#;
        let parsed = Engine::new()
            .parse_diagram_for_render_model_sync(source, ParseOptions::strict())
            .unwrap()
            .expect("Class source should produce a render model");
        let session = crate::environment::RenderEnvironment::deterministic()
            .with_math_renderer(std::sync::Arc::new(ClassMathRenderer))
            .begin_session()
            .unwrap();

        let plan = plan_render(&parsed, &session).unwrap();
        assert_eq!(
            plan.required_capabilities(),
            if cfg!(feature = "layout-elk") {
                &[RenderCapability::LayoutElk, RenderCapability::Math][..]
            } else {
                &[RenderCapability::Math][..]
            }
        );
        assert!(plan.missing_capabilities().is_empty());
        assert!(plan.is_ready());

        let rendered = prepare(parsed, &LayoutOptions::default(), session)
            .unwrap()
            .render_svg(&SvgRenderOptions::default(), &SvgDebugOptions::default())
            .unwrap();
        assert!(rendered.svg().contains("rendered-class-math"));
        assert!(!rendered.svg().contains("$$x^2$$"));
    }

    fn render_class_math(source: &str) -> String {
        let parsed = Engine::new()
            .parse_diagram_for_render_model_sync(source, ParseOptions::strict())
            .unwrap()
            .expect("Class source should produce a render model");
        let session = crate::environment::RenderEnvironment::deterministic()
            .with_math_renderer(std::sync::Arc::new(ClassMathRenderer))
            .begin_session()
            .unwrap();
        prepare(parsed, &LayoutOptions::default(), session)
            .unwrap()
            .render_svg(&SvgRenderOptions::default(), &SvgDebugOptions::default())
            .unwrap()
            .svg()
            .to_string()
    }

    #[test]
    fn class_math_label_forces_html_rendering_when_html_labels_are_disabled() {
        let svg = render_class_math(
            r#"---
config:
  htmlLabels: false
---
classDiagram
class Formula["$$x^2$$"]
"#,
        );

        assert!(svg.contains("rendered-class-math"));
        assert!(!svg.contains("$$x^2$$"));
    }

    #[test]
    fn class_relation_terminal_and_note_math_labels_use_the_math_renderer() {
        let svg = render_class_math(
            r#"classDiagram
class Formula
class Result
Formula "$$one$$" --> "$$many$$" Result : $$edge$$
note for Formula "$$note$$"
"#,
        );

        assert_eq!(svg.matches("rendered-class-math").count(), 4);
        assert!(!svg.contains("$$"));
        assert!(!svg.contains("<p><div>"));
    }

    #[test]
    fn class_annotation_and_interface_math_labels_require_and_use_math() {
        for source in [
            r#"classDiagram
class Formula <<$$annotation$$>>
"#,
            r#"classDiagram
class Formula
$$interface$$ ()-- Formula
"#,
        ] {
            let parsed = Engine::new()
                .parse_diagram_for_render_model_sync(source, ParseOptions::strict())
                .unwrap()
                .expect("Class source should produce a render model");
            let session = crate::environment::RenderEnvironment::deterministic()
                .without_math_renderer()
                .begin_session()
                .unwrap();

            let plan = plan_render(&parsed, &session).unwrap();
            assert_eq!(
                plan.required_capabilities(),
                if cfg!(feature = "layout-elk") {
                    &[RenderCapability::LayoutElk, RenderCapability::Math][..]
                } else {
                    &[RenderCapability::Math][..]
                }
            );
            assert_eq!(plan.missing_capabilities(), &[RenderCapability::Math]);

            let svg = render_class_math(source);
            assert!(svg.contains("rendered-class-math"));
            assert!(!svg.contains("$$"));
            assert!(!svg.contains("<p><div>"));
        }
    }

    #[cfg(feature = "layout-elk")]
    #[test]
    fn elk_flowchart_node_limit_accepts_boundary_and_rejects_one_beyond() {
        let source = "flowchart-elk TD\nA --> B";
        let artifact = prepare_with_model_item_limit(source, 3).unwrap();
        assert_eq!(artifact.family_kind(), RenderFamilyKind::Flowchart);

        let error = match prepare_with_model_item_limit(source, 2) {
            Err(error) => error,
            Ok(_) => panic!("ELK flowchart above the node limit unexpectedly rendered"),
        };
        assert_model_item_limit(error, 3, 2);
    }

    #[test]
    fn custom_semantic_json_is_explicitly_non_renderable() {
        let mut engine = Engine::new();
        engine
            .diagram_registry_mut()
            .insert("customDiagram", custom_semantic_parser);
        let parsed = engine
            .parse_diagram_for_render_model_with_type_sync(
                "customDiagram",
                "customDiagram\npayload",
                ParseOptions::strict(),
            )
            .unwrap()
            .unwrap();

        let error = match prepare(parsed, &LayoutOptions::default(), session()) {
            Err(error) => error,
            Ok(_) => panic!("custom JSON unexpectedly produced a built-in artifact"),
        };
        let Error::NonRenderableCustomModel {
            diagram_type,
            model_name,
            provenance,
        } = error
        else {
            panic!("expected explicit custom-model capability error")
        };
        assert_eq!(diagram_type, "customDiagram");
        assert_eq!(model_name, "customDiagram");
        assert_eq!(provenance, CustomJsonProvenance::SemanticRegistryOverlay);
    }

    #[test]
    fn custom_render_overlay_cannot_masquerade_as_a_builtin_family() {
        let mut engine = Engine::new();
        engine
            .render_diagram_registry_mut()
            .insert("flowchart-v2", custom_render_parser);
        let parsed = engine
            .parse_diagram_for_render_model_with_type_sync(
                "flowchart-v2",
                "flowchart TD\nA --> B",
                ParseOptions::strict(),
            )
            .unwrap()
            .unwrap();

        let error = match prepare(parsed, &LayoutOptions::default(), session()) {
            Err(error) => error,
            Ok(_) => panic!("custom JSON unexpectedly produced a built-in artifact"),
        };
        let Error::NonRenderableCustomModel {
            diagram_type,
            model_name,
            provenance,
        } = error
        else {
            panic!("expected explicit custom-model capability error")
        };
        assert_eq!(diagram_type, "flowchart-v2");
        assert_eq!(model_name, "custom-flowchart");
        assert_eq!(provenance, CustomJsonProvenance::RenderRegistryOverlay);
    }

    #[test]
    fn gantt_time_axis_diagnostics_invert_rendered_x_without_exposing_layout() {
        let parsed = Engine::new()
            .parse_diagram_for_render_model_sync(
                r#"---
config:
  gantt:
    useWidth: 130
    leftPadding: 10
    rightPadding: 20
---
gantt
dateFormat x
section Delivery
First: first,-1,1ms
Second: second,after first,2ms
"#,
                ParseOptions::strict(),
            )
            .unwrap()
            .unwrap();
        let artifact = prepare(parsed, &LayoutOptions::default(), session()).unwrap();

        assert_eq!(artifact.family_kind(), RenderFamilyKind::Gantt);
        let diagnostics = artifact
            .gantt_time_axis_diagnostics()
            .expect("Gantt tasks should expose time-axis diagnostics");
        assert_eq!(diagnostics.unix_millis_at_rendered_x(10.0), Some(-1));
        assert_eq!(diagnostics.unix_millis_at_rendered_x(43.0), Some(0));
        assert_eq!(diagnostics.unix_millis_at_rendered_x(77.0), Some(1));
        assert_eq!(diagnostics.unix_millis_at_rendered_x(110.0), Some(2));
        assert_eq!(diagnostics.unix_millis_at_rendered_x(44.0), None);
        assert_eq!(diagnostics.unix_millis_at_rendered_x(f64::NAN), None);

        artifact
            .render_svg(&SvgRenderOptions::default(), &SvgDebugOptions::default())
            .unwrap();
        assert_eq!(diagnostics.unix_millis_at_rendered_x(77.0), Some(1));
    }

    #[test]
    fn suppressed_parse_failure_uses_the_typed_error_artifact_and_renderer() {
        let parsed = Engine::new()
            .parse_diagram_for_render_model_sync("flowchart TD\nA -->", ParseOptions::lenient())
            .unwrap()
            .unwrap();
        let artifact = prepare(parsed, &LayoutOptions::default(), session()).unwrap();

        assert_eq!(artifact.family_kind(), RenderFamilyKind::Error);
        let rendered = artifact
            .render_svg(&SvgRenderOptions::default(), &SvgDebugOptions::default())
            .unwrap();
        assert!(rendered.svg().contains("Syntax error in text"));
    }
}
