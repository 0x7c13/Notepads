//! Backend-independent Mermaid source and semantic-model resource policy.

use crate::diagram::{ParsedDiagramRender, RenderSemanticModel};
#[cfg(any(
    feature = "diagram-flowchart",
    feature = "diagram-swimlane",
    feature = "diagram-agentflow"
))]
use crate::diagrams::flowchart::FlowchartModel;
#[cfg(feature = "diagram-ishikawa")]
use crate::diagrams::ishikawa::IshikawaDiagramRenderModel;
#[cfg(feature = "diagram-kanban")]
use crate::diagrams::kanban::KanbanDiagramRenderModel;
#[cfg(feature = "diagram-mindmap")]
use crate::diagrams::mindmap::MindmapDiagramRenderModel;
#[cfg(feature = "diagram-radar")]
use crate::diagrams::radar::RadarDiagramRenderModel;
#[cfg(feature = "diagram-requirement")]
use crate::diagrams::requirement::RequirementDiagramRenderModel;
#[cfg(feature = "diagram-sequence")]
use crate::diagrams::sequence::SequenceDiagramRenderModel;
#[cfg(feature = "diagram-treemap")]
use crate::diagrams::treemap::TreemapDiagramRenderModel;
#[cfg(feature = "diagram-zenuml")]
use crate::diagrams::zenuml::{ZenumlDiagramRenderModel, ZenumlStatementKind};
#[cfg(feature = "diagram-class")]
use crate::models::class_diagram::ClassDiagram;
use serde::Serialize;
use serde::ser::{
    SerializeMap, SerializeSeq, SerializeStruct, SerializeStructVariant, SerializeTuple,
    SerializeTupleStruct, SerializeTupleVariant,
};

const KIB: usize = 1024;
const MIB: usize = 1024 * KIB;

pub const RESOURCE_PROFILE_COUNT: usize = 4;
pub const INPUT_RESOURCE_LIMIT_COUNT: usize = 4;

#[repr(usize)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ResourceProfile {
    Interactive,
    Constrained,
    TrustedNative,
    UnboundedForTrustedInput,
}

impl ResourceProfile {
    pub const ALL: [Self; RESOURCE_PROFILE_COUNT] = [
        Self::Interactive,
        Self::Constrained,
        Self::TrustedNative,
        Self::UnboundedForTrustedInput,
    ];

    pub const fn descriptor(self) -> &'static ResourceProfileDescriptor {
        &RESOURCE_PROFILE_DESCRIPTORS[self as usize]
    }

    pub const fn id(self) -> &'static str {
        self.descriptor().id
    }

    pub fn from_id(id: &str) -> Option<Self> {
        RESOURCE_PROFILE_DESCRIPTORS
            .iter()
            .find(|descriptor| descriptor.id == id)
            .map(|descriptor| descriptor.profile)
    }
}

impl std::fmt::Display for ResourceProfile {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.id())
    }
}

impl std::str::FromStr for ResourceProfile {
    type Err = String;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        Self::from_id(value).ok_or_else(|| {
            let supported = RESOURCE_PROFILE_DESCRIPTORS
                .iter()
                .map(|descriptor| descriptor.id)
                .collect::<Vec<_>>()
                .join(", ");
            format!("unsupported resource profile `{value}`; expected one of: {supported}")
        })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub struct ResourceProfileDescriptor {
    pub profile: ResourceProfile,
    pub id: &'static str,
    pub purpose: &'static str,
    pub trust_assumption: &'static str,
    pub recommended_binding_default: bool,
}

pub static RESOURCE_PROFILE_DESCRIPTORS: [ResourceProfileDescriptor; RESOURCE_PROFILE_COUNT] = [
    ResourceProfileDescriptor {
        profile: ResourceProfile::Interactive,
        id: "interactive",
        purpose: "General interactive applications and public binding surfaces",
        trust_assumption: "Cooperative user-authored input; not a hostile or multi-tenant isolation boundary",
        recommended_binding_default: true,
    },
    ResourceProfileDescriptor {
        profile: ResourceProfile::Constrained,
        id: "constrained",
        purpose: "Constrained rendering for untrusted or publicly submitted documents",
        trust_assumption: "The host must provide timeout, memory, concurrency, and preemption controls",
        recommended_binding_default: false,
    },
    ResourceProfileDescriptor {
        profile: ResourceProfile::TrustedNative,
        id: "trusted-native",
        purpose: "Local CLI and controlled native batch rendering",
        trust_assumption: "Input is trusted and the native host controls the workload",
        recommended_binding_default: false,
    },
    ResourceProfileDescriptor {
        profile: ResourceProfile::UnboundedForTrustedInput,
        id: "unbounded-for-trusted-input",
        purpose: "Explicitly disable policy budgets while retaining hard backend capabilities",
        trust_assumption: "Input is fully trusted and the host provides outer isolation",
        recommended_binding_default: false,
    },
];

pub const GENERAL_BINDING_DEFAULT_RESOURCE_PROFILE: ResourceProfile = ResourceProfile::Interactive;
pub const CLI_DEFAULT_RESOURCE_PROFILE: ResourceProfile = ResourceProfile::TrustedNative;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum InputResourceLimitPhase {
    Source,
    Model,
}

impl InputResourceLimitPhase {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Source => "source",
            Self::Model => "layout_model",
        }
    }
}

impl std::fmt::Display for InputResourceLimitPhase {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

#[repr(usize)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum InputResourceLimitId {
    MaxSourceBytes,
    MaxModelItems,
    MaxModelTextBytes,
    MaxModelNestingDepth,
}

impl InputResourceLimitId {
    pub const ALL: [Self; INPUT_RESOURCE_LIMIT_COUNT] = [
        Self::MaxSourceBytes,
        Self::MaxModelItems,
        Self::MaxModelTextBytes,
        Self::MaxModelNestingDepth,
    ];

    pub const fn index(self) -> usize {
        self as usize
    }

    pub fn from_stable_id(id: &str) -> Option<Self> {
        INPUT_RESOURCE_LIMIT_DESCRIPTORS
            .iter()
            .find(|descriptor| descriptor.stable_id == id)
            .map(|descriptor| descriptor.id)
    }

    pub const fn descriptor(self) -> &'static InputResourceLimitDescriptor {
        &INPUT_RESOURCE_LIMIT_DESCRIPTORS[self.index()]
    }

    pub const fn as_str(self) -> &'static str {
        self.descriptor().stable_id
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub struct InputResourceLimitDescriptor {
    pub id: InputResourceLimitId,
    pub stable_id: &'static str,
    pub phase: InputResourceLimitPhase,
    pub description: &'static str,
    pub overridable: bool,
    pub minimum_value: usize,
}

macro_rules! input_limit_descriptors {
    ($($id:ident => ($stable:literal, $phase:ident, $description:literal)),+ $(,)?) => {
        pub static INPUT_RESOURCE_LIMIT_DESCRIPTORS:
            [InputResourceLimitDescriptor; INPUT_RESOURCE_LIMIT_COUNT] = [
                $(InputResourceLimitDescriptor {
                    id: InputResourceLimitId::$id,
                    stable_id: $stable,
                    phase: InputResourceLimitPhase::$phase,
                    description: $description,
                    overridable: true,
                    minimum_value: 1,
                }),+
            ];
    };
}

input_limit_descriptors! {
    MaxSourceBytes => ("max_source_bytes", Source, "Maximum UTF-8 Mermaid source bytes"),
    MaxModelItems => ("max_model_items", Model, "Maximum semantic model entities and relationships across all diagram families"),
    MaxModelTextBytes => ("max_model_text_bytes", Model, "Maximum aggregate UTF-8 text retained by a semantic model"),
    MaxModelNestingDepth => ("max_model_nesting_depth", Model, "Maximum semantic nesting depth accepted before layout"),
}

const PROFILE_VALUES: [[Option<usize>; RESOURCE_PROFILE_COUNT]; INPUT_RESOURCE_LIMIT_COUNT] = [
    [Some(2 * MIB), Some(MIB), Some(16 * MIB), None],
    // Policy defaults, not Mermaid syntax limits. They deliberately preserve the
    // aggregate headroom of the previous per-family cardinality budgets.
    [Some(32_000), Some(16_000), Some(200_000), None],
    [Some(2 * MIB), Some(MIB), Some(16 * MIB), None],
    [Some(256), Some(128), Some(1_024), None],
];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct InputResourcePolicy {
    profile: ResourceProfile,
    base_values: [Option<usize>; INPUT_RESOURCE_LIMIT_COUNT],
    effective_values: [Option<usize>; INPUT_RESOURCE_LIMIT_COUNT],
    explicit_overrides: [Option<usize>; INPUT_RESOURCE_LIMIT_COUNT],
}

impl Default for InputResourcePolicy {
    fn default() -> Self {
        Self::for_profile(GENERAL_BINDING_DEFAULT_RESOURCE_PROFILE)
    }
}

impl InputResourcePolicy {
    pub const fn for_profile(profile: ResourceProfile) -> Self {
        let mut values = [None; INPUT_RESOURCE_LIMIT_COUNT];
        let mut index = 0;
        while index < INPUT_RESOURCE_LIMIT_COUNT {
            values[index] = PROFILE_VALUES[index][profile as usize];
            index += 1;
        }
        Self {
            profile,
            base_values: values,
            effective_values: values,
            explicit_overrides: [None; INPUT_RESOURCE_LIMIT_COUNT],
        }
    }

    pub const fn profile(self) -> ResourceProfile {
        self.profile
    }

    pub const fn value(self, id: InputResourceLimitId) -> Option<usize> {
        self.effective_values[id.index()]
    }

    pub const fn base_value(self, id: InputResourceLimitId) -> Option<usize> {
        self.base_values[id.index()]
    }

    pub const fn explicit_override(self, id: InputResourceLimitId) -> Option<usize> {
        self.explicit_overrides[id.index()]
    }

    pub fn explicit_overrides(&self) -> impl Iterator<Item = (InputResourceLimitId, usize)> + '_ {
        InputResourceLimitId::ALL
            .into_iter()
            .filter_map(|id| self.explicit_override(id).map(|value| (id, value)))
    }

    pub fn apply_override(
        &mut self,
        stable_id: &str,
        value: usize,
    ) -> Result<(), InputResourceLimitOverrideError> {
        let id = InputResourceLimitId::from_stable_id(stable_id)
            .ok_or_else(|| InputResourceLimitOverrideError::UnknownLimit(stable_id.to_string()))?;
        self.apply_limit(id, value)
    }

    pub fn apply_limit(
        &mut self,
        id: InputResourceLimitId,
        value: usize,
    ) -> Result<(), InputResourceLimitOverrideError> {
        if value == 0 {
            return Err(InputResourceLimitOverrideError::NonPositive(id.as_str()));
        }
        self.effective_values[id.index()] = Some(value);
        self.explicit_overrides[id.index()] = Some(value);
        Ok(())
    }

    pub fn with_limit(
        mut self,
        id: InputResourceLimitId,
        value: usize,
    ) -> Result<Self, InputResourceLimitOverrideError> {
        self.apply_limit(id, value)?;
        Ok(self)
    }

    fn check_limit(
        &self,
        phase: InputResourceLimitPhase,
        id: InputResourceLimitId,
        actual: usize,
    ) -> Result<(), InputResourceLimitExceeded> {
        let Some(max) = self.value(id) else {
            return Ok(());
        };
        if actual <= max {
            return Ok(());
        }
        Err(InputResourceLimitExceeded {
            phase,
            limit: id.as_str(),
            actual,
            max,
            profile: self.profile,
            explicit_overrides: self
                .explicit_overrides()
                .map(|(id, value)| InputResourceLimitOverride { id, value })
                .collect(),
        })
    }

    pub fn check_source_bytes(&self, source: &str) -> Result<(), InputResourceLimitExceeded> {
        self.check_limit(
            InputResourceLimitPhase::Source,
            InputResourceLimitId::MaxSourceBytes,
            source.len(),
        )
    }

    /// Checks only the public semantic-model projection.
    ///
    /// Use [`Self::check_parsed_render`] for a parsed built-in render operation so parser-owned
    /// render context is included in the text budget.
    pub fn check_render_model(
        &self,
        model: &RenderSemanticModel,
    ) -> Result<(), InputResourceLimitExceeded> {
        self.check_model_complexity(ModelComplexity::from_render_model(model))
    }

    pub fn check_parsed_render(
        &self,
        parsed: &ParsedDiagramRender,
    ) -> Result<(), InputResourceLimitExceeded> {
        let mut complexity = ModelComplexity::from_render_model(parsed.model());
        complexity.text_bytes = complexity
            .text_bytes
            .saturating_add(parsed.retained_render_context_bytes());
        self.check_model_complexity(complexity)
    }

    pub fn check_model_complexity(
        &self,
        complexity: ModelComplexity,
    ) -> Result<(), InputResourceLimitExceeded> {
        self.check_limit(
            InputResourceLimitPhase::Model,
            InputResourceLimitId::MaxModelItems,
            complexity.items,
        )?;
        self.check_limit(
            InputResourceLimitPhase::Model,
            InputResourceLimitId::MaxModelTextBytes,
            complexity.text_bytes,
        )?;
        self.check_limit(
            InputResourceLimitPhase::Model,
            InputResourceLimitId::MaxModelNestingDepth,
            complexity.nesting_depth,
        )
    }

    #[cfg(any(
        feature = "diagram-flowchart",
        feature = "diagram-swimlane",
        feature = "diagram-agentflow"
    ))]
    pub fn check_flowchart_complexity(
        &self,
        model: &FlowchartModel,
    ) -> Result<FlowchartComplexity, InputResourceLimitExceeded> {
        let complexity = FlowchartComplexity::from_model(model);
        self.check_model_complexity(complexity.as_model_complexity())?;
        Ok(complexity)
    }

    #[cfg(feature = "diagram-class")]
    pub fn check_class_complexity(
        &self,
        model: &ClassDiagram,
    ) -> Result<ClassComplexity, InputResourceLimitExceeded> {
        let complexity = ClassComplexity::from_model(model);
        self.check_model_complexity(complexity.as_model_complexity())?;
        Ok(complexity)
    }

    #[cfg(feature = "diagram-mindmap")]
    pub fn check_mindmap_complexity(
        &self,
        model: &MindmapDiagramRenderModel,
    ) -> Result<MindmapComplexity, InputResourceLimitExceeded> {
        let complexity = MindmapComplexity::from_model(model);
        self.check_model_complexity(complexity.as_model_complexity())?;
        Ok(complexity)
    }

    #[cfg(feature = "diagram-zenuml")]
    pub fn check_zenuml_complexity(
        &self,
        model: &ZenumlDiagramRenderModel,
    ) -> Result<ZenumlComplexity, InputResourceLimitExceeded> {
        let complexity = ZenumlComplexity::from_model(model);
        self.check_model_complexity(complexity.as_model_complexity())?;
        Ok(complexity)
    }

    #[cfg(feature = "diagram-sequence")]
    pub fn check_sequence_complexity(
        &self,
        model: &SequenceDiagramRenderModel,
    ) -> Result<SequenceComplexity, InputResourceLimitExceeded> {
        let complexity = SequenceComplexity::from_model(model);
        self.check_model_complexity(complexity.as_model_complexity())?;
        Ok(complexity)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum InputResourceLimitOverrideError {
    #[error("resource limit id `{0}` is not part of input resource contract schema 1")]
    UnknownLimit(String),
    #[error("resource limit `{0}` must be a positive integer")]
    NonPositive(&'static str),
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("resource limit exceeded during {phase}: {limit} actual={actual} max={max}")]
#[non_exhaustive]
pub struct InputResourceLimitExceeded {
    pub phase: InputResourceLimitPhase,
    pub limit: &'static str,
    pub actual: usize,
    pub max: usize,
    pub profile: ResourceProfile,
    pub explicit_overrides: Vec<InputResourceLimitOverride>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct InputResourceLimitOverride {
    pub id: InputResourceLimitId,
    pub value: usize,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct ModelComplexity {
    pub items: usize,
    pub text_bytes: usize,
    pub nesting_depth: usize,
}

impl ModelComplexity {
    pub const fn new(items: usize, text_bytes: usize, nesting_depth: usize) -> Self {
        Self {
            items,
            text_bytes,
            nesting_depth,
        }
    }

    pub fn from_render_model(model: &RenderSemanticModel) -> Self {
        match model {
            RenderSemanticModel::Error(model) => Self::from_serializable(model),
            RenderSemanticModel::CustomJson(model) => {
                let mut complexity = Self::from_json(model.value());
                complexity.text_bytes = complexity
                    .text_bytes
                    .saturating_add(model.model_name().len());
                complexity
            }
            #[cfg(feature = "diagram-mindmap")]
            RenderSemanticModel::Mindmap(model) => {
                MindmapComplexity::from_model(model).as_model_complexity()
            }
            #[cfg(feature = "diagram-state")]
            RenderSemanticModel::State(model) => model.model_complexity(),
            #[cfg(feature = "diagram-sequence")]
            RenderSemanticModel::Sequence(model) => {
                SequenceComplexity::from_model(model).as_model_complexity()
            }
            #[cfg(feature = "diagram-zenuml")]
            RenderSemanticModel::Zenuml(model) => {
                ZenumlComplexity::from_model(model).as_model_complexity()
            }
            #[cfg(any(feature = "diagram-flowchart", feature = "diagram-swimlane"))]
            RenderSemanticModel::Flowchart(model) => {
                FlowchartComplexity::from_model(model).as_model_complexity()
            }
            #[cfg(feature = "diagram-architecture")]
            RenderSemanticModel::Architecture(model) => Self::from_serializable(model),
            #[cfg(feature = "diagram-class")]
            RenderSemanticModel::Class(model) => {
                ClassComplexity::from_model(model).as_model_complexity()
            }
            #[cfg(feature = "diagram-c4")]
            RenderSemanticModel::C4(model) => Self::from_serializable(model),
            #[cfg(feature = "diagram-cynefin")]
            RenderSemanticModel::Cynefin(model) => Self::from_serializable(model),
            #[cfg(feature = "diagram-railroad")]
            RenderSemanticModel::Railroad(model) => Self::from_serializable(model),
            #[cfg(feature = "diagram-kanban")]
            RenderSemanticModel::Kanban(model) => Self::from_kanban(model),
            #[cfg(feature = "diagram-gantt")]
            RenderSemanticModel::Gantt(model) => Self::from_serializable(model),
            #[cfg(feature = "diagram-pie")]
            RenderSemanticModel::Pie(model) => Self::from_serializable(model),
            #[cfg(feature = "diagram-packet")]
            RenderSemanticModel::Packet(model) => Self::from_serializable(model),
            #[cfg(feature = "diagram-timeline")]
            RenderSemanticModel::Timeline(model) => Self::from_serializable(model),
            #[cfg(feature = "diagram-journey")]
            RenderSemanticModel::Journey(model) => Self::from_serializable(model),
            #[cfg(feature = "diagram-requirement")]
            RenderSemanticModel::Requirement(model) => Self::from_requirement(model),
            #[cfg(feature = "diagram-sankey")]
            RenderSemanticModel::Sankey(model) => Self::from_serializable(model),
            #[cfg(feature = "diagram-radar")]
            RenderSemanticModel::Radar(model) => Self::from_radar(model),
            #[cfg(feature = "diagram-info")]
            RenderSemanticModel::Info(model) => Self::from_serializable(model),
            #[cfg(feature = "diagram-treemap")]
            RenderSemanticModel::Treemap(model) => {
                TreemapComplexity::from_model(model).as_model_complexity()
            }
            #[cfg(feature = "diagram-block")]
            RenderSemanticModel::Block(model) => model.model_complexity(),
            #[cfg(feature = "diagram-er")]
            RenderSemanticModel::Er(model) => Self::from_serializable(model),
            #[cfg(feature = "diagram-quadrant-chart")]
            RenderSemanticModel::QuadrantChart(model) => Self::from_serializable(model),
            #[cfg(feature = "diagram-xychart")]
            RenderSemanticModel::XyChart(model) => Self::from_serializable(model),
            #[cfg(feature = "diagram-git-graph")]
            RenderSemanticModel::GitGraph(model) => Self::from_serializable(model),
            #[cfg(feature = "diagram-tree-view")]
            RenderSemanticModel::TreeView(model) => Self::from_serializable(model),
            #[cfg(feature = "diagram-ishikawa")]
            RenderSemanticModel::Ishikawa(model) => {
                IshikawaComplexity::from_model(model).as_model_complexity()
            }
            #[cfg(feature = "diagram-event-modeling")]
            RenderSemanticModel::EventModeling(model) => Self::from_serializable(model),
            #[cfg(feature = "diagram-venn")]
            RenderSemanticModel::Venn(model) => Self::from_serializable(model),
            #[cfg(feature = "diagram-wardley")]
            RenderSemanticModel::Wardley(model) => Self::from_serializable(model),
            #[cfg(feature = "diagram-usecase")]
            RenderSemanticModel::Usecase(model) => Self::from_serializable(model),
            #[cfg(feature = "diagram-agentflow")]
            RenderSemanticModel::Agentflow(model) => Self::from_serializable(model),
        }
    }

    /// Counts the stable serialized model shape used by JSON-returning transport operations.
    ///
    /// A model whose serializer refuses the generic depth boundary receives saturated complexity
    /// so resource policy rejects it without panicking. Use [`Self::from_json`] for an exact count
    /// of a managed JSON value at arbitrary depth.
    pub fn from_serializable<T: Serialize + ?Sized>(model: &T) -> Self {
        let mut counter = ModelComplexitySerializer::default();
        if model.serialize(&mut counter).is_err() {
            return Self::new(usize::MAX, usize::MAX, usize::MAX);
        }
        counter.finish()
    }

    /// Counts JSON's serialized shape without invoking a recursive serializer.
    pub fn from_json(model: &serde_json::Value) -> Self {
        let complexity = json_value_complexity(model, 0);
        Self::new(
            complexity.items.max(1),
            complexity.text_bytes,
            complexity.container_depth,
        )
    }

    #[cfg(feature = "diagram-kanban")]
    pub fn from_kanban(model: &KanbanDiagramRenderModel) -> Self {
        let text_bytes = model.nodes.iter().fold(0usize, |total, node| {
            [
                Some(node.id.as_str()),
                Some(node.label.as_str()),
                node.parent_id.as_deref(),
                node.ticket.as_deref(),
                node.priority.as_deref(),
                node.assigned.as_deref(),
                node.icon.as_deref(),
            ]
            .into_iter()
            .flatten()
            .fold(total, |subtotal, value| {
                subtotal.saturating_add(value.len())
            })
        });
        let nesting_depth = kanban_nesting_depth(model);
        Self::new(model.nodes.len(), text_bytes, nesting_depth)
    }

    #[cfg(feature = "diagram-radar")]
    pub fn from_radar(model: &RadarDiagramRenderModel) -> Self {
        let common_text_bytes = [
            model.title.as_deref(),
            model.acc_title.as_deref(),
            model.acc_descr.as_deref(),
            Some(model.options.graticule.as_str()),
        ]
        .into_iter()
        .flatten()
        .fold(0usize, |total, value| total.saturating_add(value.len()));
        let axis_text_bytes = model.axes.iter().fold(0usize, |total, axis| {
            total
                .saturating_add(axis.name.len())
                .saturating_add(axis.label.len())
        });
        let (curve_items, curve_text_bytes) =
            model
                .curves
                .iter()
                .fold((0usize, 0usize), |(items, text_bytes), curve| {
                    (
                        items.saturating_add(curve.entries.len()),
                        text_bytes
                            .saturating_add(curve.name.len())
                            .saturating_add(curve.label.len()),
                    )
                });
        Self::new(
            model
                .axes
                .len()
                .saturating_add(model.curves.len())
                .saturating_add(curve_items),
            common_text_bytes
                .saturating_add(axis_text_bytes)
                .saturating_add(curve_text_bytes),
            0,
        )
    }

    #[cfg(feature = "diagram-requirement")]
    pub fn from_requirement(model: &RequirementDiagramRenderModel) -> Self {
        let common_text_bytes = [
            model.acc_title.as_deref(),
            model.acc_descr.as_deref(),
            Some(model.direction.as_str()),
        ]
        .into_iter()
        .flatten()
        .fold(0usize, |total, value| total.saturating_add(value.len()));
        let requirement_text_bytes = model.requirements.iter().fold(0usize, |total, node| {
            let total = [
                node.name.as_str(),
                node.node_type.as_str(),
                node.requirement_id.as_str(),
                node.text.as_str(),
                node.risk.as_str(),
                node.verify_method.as_str(),
            ]
            .into_iter()
            .fold(total, |subtotal, value| {
                subtotal.saturating_add(value.len())
            });
            node.css_styles
                .iter()
                .chain(&node.classes)
                .fold(total, |subtotal, value| {
                    subtotal.saturating_add(value.len())
                })
        });
        let element_text_bytes = model.elements.iter().fold(0usize, |total, node| {
            let total = [
                node.name.as_str(),
                node.element_type.as_str(),
                node.doc_ref.as_str(),
            ]
            .into_iter()
            .fold(total, |subtotal, value| {
                subtotal.saturating_add(value.len())
            });
            node.css_styles
                .iter()
                .chain(&node.classes)
                .fold(total, |subtotal, value| {
                    subtotal.saturating_add(value.len())
                })
        });
        let relationship_text_bytes =
            model
                .relationships
                .iter()
                .fold(0usize, |total, relationship| {
                    [
                        relationship.rel_type.as_str(),
                        relationship.src.as_str(),
                        relationship.dst.as_str(),
                    ]
                    .into_iter()
                    .fold(total, |subtotal, value| {
                        subtotal.saturating_add(value.len())
                    })
                });
        let class_text_bytes = model.classes.iter().fold(0usize, |total, (key, class)| {
            class.styles.iter().chain(&class.text_styles).fold(
                total
                    .saturating_add(key.len())
                    .saturating_add(class.id.len()),
                |subtotal, value| subtotal.saturating_add(value.len()),
            )
        });
        Self::new(
            model
                .requirements
                .len()
                .saturating_add(model.elements.len())
                .saturating_add(model.relationships.len())
                .saturating_add(model.classes.len()),
            common_text_bytes
                .saturating_add(requirement_text_bytes)
                .saturating_add(element_text_bytes)
                .saturating_add(relationship_text_bytes)
                .saturating_add(class_text_bytes),
            0,
        )
    }
}

#[derive(Debug, Default)]
struct ModelComplexitySerializer {
    items: usize,
    text_bytes: usize,
    current_depth: usize,
    max_depth: usize,
}

impl ModelComplexitySerializer {
    fn enter(&mut self) {
        self.current_depth = self.current_depth.saturating_add(1);
        self.max_depth = self.max_depth.max(self.current_depth);
    }

    fn leave(&mut self) {
        self.current_depth = self.current_depth.saturating_sub(1);
    }

    fn finish(self) -> ModelComplexity {
        ModelComplexity::new(
            self.items.max(1),
            self.text_bytes,
            self.max_depth.saturating_sub(1),
        )
    }
}

#[derive(Debug)]
struct ModelComplexitySerializationError;

impl std::fmt::Display for ModelComplexitySerializationError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str("model complexity serialization failed")
    }
}

impl std::error::Error for ModelComplexitySerializationError {}

impl serde::ser::Error for ModelComplexitySerializationError {
    fn custom<T: std::fmt::Display>(_message: T) -> Self {
        Self
    }
}

struct ModelCompound<'a> {
    counter: &'a mut ModelComplexitySerializer,
    count_items: bool,
}

impl<'a> ModelCompound<'a> {
    fn new(counter: &'a mut ModelComplexitySerializer, count_items: bool) -> Self {
        counter.enter();
        Self {
            counter,
            count_items,
        }
    }

    fn serialize_value<T: Serialize + ?Sized>(
        &mut self,
        value: &T,
    ) -> Result<(), ModelComplexitySerializationError> {
        if self.count_items {
            self.counter.items = self.counter.items.saturating_add(1);
        }
        value.serialize(&mut *self.counter)
    }
}

impl Drop for ModelCompound<'_> {
    fn drop(&mut self) {
        self.counter.leave();
    }
}

impl SerializeSeq for ModelCompound<'_> {
    type Ok = ();
    type Error = ModelComplexitySerializationError;

    fn serialize_element<T: Serialize + ?Sized>(
        &mut self,
        value: &T,
    ) -> Result<Self::Ok, Self::Error> {
        self.serialize_value(value)
    }

    fn end(self) -> Result<Self::Ok, Self::Error> {
        Ok(())
    }
}

impl SerializeTuple for ModelCompound<'_> {
    type Ok = ();
    type Error = ModelComplexitySerializationError;

    fn serialize_element<T: Serialize + ?Sized>(
        &mut self,
        value: &T,
    ) -> Result<Self::Ok, Self::Error> {
        self.serialize_value(value)
    }

    fn end(self) -> Result<Self::Ok, Self::Error> {
        Ok(())
    }
}

impl SerializeTupleStruct for ModelCompound<'_> {
    type Ok = ();
    type Error = ModelComplexitySerializationError;

    fn serialize_field<T: Serialize + ?Sized>(
        &mut self,
        value: &T,
    ) -> Result<Self::Ok, Self::Error> {
        self.serialize_value(value)
    }

    fn end(self) -> Result<Self::Ok, Self::Error> {
        Ok(())
    }
}

impl SerializeTupleVariant for ModelCompound<'_> {
    type Ok = ();
    type Error = ModelComplexitySerializationError;

    fn serialize_field<T: Serialize + ?Sized>(
        &mut self,
        value: &T,
    ) -> Result<Self::Ok, Self::Error> {
        self.serialize_value(value)
    }

    fn end(self) -> Result<Self::Ok, Self::Error> {
        Ok(())
    }
}

impl SerializeMap for ModelCompound<'_> {
    type Ok = ();
    type Error = ModelComplexitySerializationError;

    fn serialize_key<T: Serialize + ?Sized>(&mut self, key: &T) -> Result<Self::Ok, Self::Error> {
        self.serialize_value(key)
    }

    fn serialize_value<T: Serialize + ?Sized>(
        &mut self,
        value: &T,
    ) -> Result<Self::Ok, Self::Error> {
        value.serialize(&mut *self.counter)
    }

    fn end(self) -> Result<Self::Ok, Self::Error> {
        Ok(())
    }
}

impl SerializeStruct for ModelCompound<'_> {
    type Ok = ();
    type Error = ModelComplexitySerializationError;

    fn serialize_field<T: Serialize + ?Sized>(
        &mut self,
        _key: &'static str,
        value: &T,
    ) -> Result<Self::Ok, Self::Error> {
        value.serialize(&mut *self.counter)
    }

    fn end(self) -> Result<Self::Ok, Self::Error> {
        Ok(())
    }
}

impl SerializeStructVariant for ModelCompound<'_> {
    type Ok = ();
    type Error = ModelComplexitySerializationError;

    fn serialize_field<T: Serialize + ?Sized>(
        &mut self,
        _key: &'static str,
        value: &T,
    ) -> Result<Self::Ok, Self::Error> {
        value.serialize(&mut *self.counter)
    }

    fn end(self) -> Result<Self::Ok, Self::Error> {
        Ok(())
    }
}

impl<'a> serde::Serializer for &'a mut ModelComplexitySerializer {
    type Ok = ();
    type Error = ModelComplexitySerializationError;
    type SerializeSeq = ModelCompound<'a>;
    type SerializeTuple = ModelCompound<'a>;
    type SerializeTupleStruct = ModelCompound<'a>;
    type SerializeTupleVariant = ModelCompound<'a>;
    type SerializeMap = ModelCompound<'a>;
    type SerializeStruct = ModelCompound<'a>;
    type SerializeStructVariant = ModelCompound<'a>;

    fn serialize_bool(self, _value: bool) -> Result<Self::Ok, Self::Error> {
        Ok(())
    }

    fn serialize_i8(self, _value: i8) -> Result<Self::Ok, Self::Error> {
        Ok(())
    }

    fn serialize_i16(self, _value: i16) -> Result<Self::Ok, Self::Error> {
        Ok(())
    }

    fn serialize_i32(self, _value: i32) -> Result<Self::Ok, Self::Error> {
        Ok(())
    }

    fn serialize_i64(self, _value: i64) -> Result<Self::Ok, Self::Error> {
        Ok(())
    }

    fn serialize_i128(self, _value: i128) -> Result<Self::Ok, Self::Error> {
        Ok(())
    }

    fn serialize_u8(self, _value: u8) -> Result<Self::Ok, Self::Error> {
        Ok(())
    }

    fn serialize_u16(self, _value: u16) -> Result<Self::Ok, Self::Error> {
        Ok(())
    }

    fn serialize_u32(self, _value: u32) -> Result<Self::Ok, Self::Error> {
        Ok(())
    }

    fn serialize_u64(self, _value: u64) -> Result<Self::Ok, Self::Error> {
        Ok(())
    }

    fn serialize_u128(self, _value: u128) -> Result<Self::Ok, Self::Error> {
        Ok(())
    }

    fn serialize_f32(self, _value: f32) -> Result<Self::Ok, Self::Error> {
        Ok(())
    }

    fn serialize_f64(self, _value: f64) -> Result<Self::Ok, Self::Error> {
        Ok(())
    }

    fn serialize_char(self, value: char) -> Result<Self::Ok, Self::Error> {
        self.text_bytes = self.text_bytes.saturating_add(value.len_utf8());
        Ok(())
    }

    fn serialize_str(self, value: &str) -> Result<Self::Ok, Self::Error> {
        self.text_bytes = self.text_bytes.saturating_add(value.len());
        Ok(())
    }

    fn serialize_bytes(self, value: &[u8]) -> Result<Self::Ok, Self::Error> {
        self.items = self.items.saturating_add(value.len());
        Ok(())
    }

    fn serialize_none(self) -> Result<Self::Ok, Self::Error> {
        Ok(())
    }

    fn serialize_some<T: Serialize + ?Sized>(self, value: &T) -> Result<Self::Ok, Self::Error> {
        value.serialize(self)
    }

    fn serialize_unit(self) -> Result<Self::Ok, Self::Error> {
        Ok(())
    }

    fn serialize_unit_struct(self, _name: &'static str) -> Result<Self::Ok, Self::Error> {
        Ok(())
    }

    fn serialize_unit_variant(
        self,
        _name: &'static str,
        _variant_index: u32,
        _variant: &'static str,
    ) -> Result<Self::Ok, Self::Error> {
        Ok(())
    }

    fn serialize_newtype_struct<T: Serialize + ?Sized>(
        self,
        _name: &'static str,
        value: &T,
    ) -> Result<Self::Ok, Self::Error> {
        value.serialize(self)
    }

    fn serialize_newtype_variant<T: Serialize + ?Sized>(
        self,
        _name: &'static str,
        _variant_index: u32,
        _variant: &'static str,
        value: &T,
    ) -> Result<Self::Ok, Self::Error> {
        value.serialize(self)
    }

    fn serialize_seq(self, _length: Option<usize>) -> Result<Self::SerializeSeq, Self::Error> {
        Ok(ModelCompound::new(self, true))
    }

    fn serialize_tuple(self, _length: usize) -> Result<Self::SerializeTuple, Self::Error> {
        Ok(ModelCompound::new(self, true))
    }

    fn serialize_tuple_struct(
        self,
        _name: &'static str,
        _length: usize,
    ) -> Result<Self::SerializeTupleStruct, Self::Error> {
        Ok(ModelCompound::new(self, true))
    }

    fn serialize_tuple_variant(
        self,
        _name: &'static str,
        _variant_index: u32,
        _variant: &'static str,
        _length: usize,
    ) -> Result<Self::SerializeTupleVariant, Self::Error> {
        Ok(ModelCompound::new(self, true))
    }

    fn serialize_map(self, _length: Option<usize>) -> Result<Self::SerializeMap, Self::Error> {
        Ok(ModelCompound::new(self, true))
    }

    fn serialize_struct(
        self,
        _name: &'static str,
        _length: usize,
    ) -> Result<Self::SerializeStruct, Self::Error> {
        Ok(ModelCompound::new(self, false))
    }

    fn serialize_struct_variant(
        self,
        _name: &'static str,
        _variant_index: u32,
        _variant: &'static str,
        _length: usize,
    ) -> Result<Self::SerializeStructVariant, Self::Error> {
        Ok(ModelCompound::new(self, false))
    }

    fn collect_str<T: std::fmt::Display + ?Sized>(
        self,
        value: &T,
    ) -> Result<Self::Ok, Self::Error> {
        self.text_bytes = self.text_bytes.saturating_add(value.to_string().len());
        Ok(())
    }

    fn is_human_readable(&self) -> bool {
        true
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg(any(
    feature = "diagram-flowchart",
    feature = "diagram-swimlane",
    feature = "diagram-agentflow"
))]
pub struct FlowchartComplexity {
    pub nodes: usize,
    pub edges: usize,
    pub subgraphs: usize,
    pub label_bytes: usize,
    pub subgraph_depth: usize,
}

#[cfg(any(
    feature = "diagram-flowchart",
    feature = "diagram-swimlane",
    feature = "diagram-agentflow"
))]
impl FlowchartComplexity {
    pub fn from_model(model: &FlowchartModel) -> Self {
        Self {
            nodes: model.nodes.len().saturating_add(model.subgraphs.len()),
            edges: model.edges.len(),
            subgraphs: model.subgraphs.len(),
            label_bytes: flowchart_text_bytes(model),
            subgraph_depth: flowchart_subgraph_depth(model),
        }
    }

    pub fn as_model_complexity(self) -> ModelComplexity {
        ModelComplexity::new(
            self.nodes.saturating_add(self.edges),
            self.label_bytes,
            self.subgraph_depth,
        )
    }
}

/// Computes Treemap model complexity iteratively without serializing a user-controlled node tree.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg(feature = "diagram-treemap")]
pub struct TreemapComplexity {
    pub nodes: usize,
    pub classes: usize,
    pub label_bytes: usize,
    pub nesting_depth: usize,
}

#[cfg(feature = "diagram-treemap")]
impl TreemapComplexity {
    pub fn from_model(model: &TreemapDiagramRenderModel) -> Self {
        let mut nodes = 0usize;
        let mut classes = 0usize;
        let mut label_bytes = [
            model.acc_title.as_deref(),
            model.acc_descr.as_deref(),
            model.title.as_deref(),
        ]
        .into_iter()
        .flatten()
        .fold(0usize, |total, value| total.saturating_add(value.len()));
        let mut nesting_depth = 0usize;

        let mut pending = vec![(&model.root, 0usize)];
        let mut visited = vec![false; model.nodes.len()];
        while let Some((node, depth)) = pending.pop() {
            nodes = nodes.saturating_add(1);
            nesting_depth = nesting_depth.max(depth);
            label_bytes = label_bytes
                .saturating_add(node.name.len())
                .saturating_add(node.class_selector.as_deref().map_or(0, str::len))
                .saturating_add(
                    node.css_compiled_styles
                        .as_deref()
                        .unwrap_or(&[])
                        .iter()
                        .map(String::len)
                        .sum::<usize>(),
                );
            if let Some(value) = node.value.as_ref() {
                label_bytes = label_bytes.saturating_add(json_text_bytes(value));
                nodes = nodes.saturating_add(json_item_count(value));
            }
            if let Some(children) = node.children.as_ref() {
                for child in children.iter().rev() {
                    if let Some(record) = model.nodes.get(*child)
                        && !visited[*child]
                    {
                        visited[*child] = true;
                        pending.push((record, depth.saturating_add(1)));
                    }
                }
            }
        }

        for (class_name, class_def) in &model.classes {
            classes = classes.saturating_add(1);
            label_bytes = label_bytes
                .saturating_add(class_name.len())
                .saturating_add(class_def.id.len())
                .saturating_add(class_def.styles.iter().map(String::len).sum::<usize>())
                .saturating_add(class_def.text_styles.iter().map(String::len).sum::<usize>());
        }

        Self {
            nodes,
            classes,
            label_bytes,
            nesting_depth,
        }
    }

    pub fn as_model_complexity(self) -> ModelComplexity {
        ModelComplexity::new(
            self.nodes.saturating_add(self.classes),
            self.label_bytes,
            self.nesting_depth,
        )
    }
}

/// Computes Ishikawa model complexity iteratively without growing the stack with cause depth.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg(feature = "diagram-ishikawa")]
pub struct IshikawaComplexity {
    pub nodes: usize,
    pub label_bytes: usize,
    pub nesting_depth: usize,
}

#[cfg(feature = "diagram-ishikawa")]
impl IshikawaComplexity {
    pub fn from_model(model: &IshikawaDiagramRenderModel) -> Self {
        let mut complexity = Self {
            nodes: 0,
            label_bytes: [
                model.acc_title.as_deref(),
                model.acc_descr.as_deref(),
                model.title.as_deref(),
            ]
            .into_iter()
            .flatten()
            .fold(0usize, |total, value| total.saturating_add(value.len())),
            nesting_depth: 0,
        };
        let Some(root) = model.root.and_then(|id| model.nodes.get(id)) else {
            return complexity;
        };

        let mut pending = vec![(root, 0usize)];
        let mut visited = vec![false; model.nodes.len()];
        if let Some(root) = model.root {
            visited[root] = true;
        }
        while let Some((node, depth)) = pending.pop() {
            complexity.nodes = complexity.nodes.saturating_add(1);
            complexity.label_bytes = complexity.label_bytes.saturating_add(node.text.len());
            complexity.nesting_depth = complexity.nesting_depth.max(depth);
            for child in node.children.iter().rev() {
                if let Some(record) = model.nodes.get(*child)
                    && !visited[*child]
                {
                    visited[*child] = true;
                    pending.push((record, depth.saturating_add(1)));
                }
            }
        }
        complexity
    }

    pub fn as_model_complexity(self) -> ModelComplexity {
        ModelComplexity::new(self.nodes, self.label_bytes, self.nesting_depth)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg(feature = "diagram-class")]
pub struct ClassComplexity {
    pub nodes: usize,
    pub edges: usize,
    pub namespaces: usize,
    pub label_bytes: usize,
    pub namespace_depth: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg(feature = "diagram-mindmap")]
pub struct MindmapComplexity {
    pub nodes: usize,
    pub edges: usize,
    pub label_bytes: usize,
    pub nesting_depth: usize,
}

#[cfg(feature = "diagram-mindmap")]
impl MindmapComplexity {
    pub fn from_model(model: &MindmapDiagramRenderModel) -> Self {
        let node_label_bytes = model.nodes.iter().fold(0usize, |total, node| {
            let strings = [
                node.id.as_str(),
                node.dom_id.as_str(),
                node.label.as_str(),
                node.label_type.as_str(),
                node.shape.as_str(),
                node.css_classes.as_str(),
                node.look.as_str(),
                node.node_id.as_str(),
            ];
            let total = strings.into_iter().fold(total, |subtotal, value| {
                subtotal.saturating_add(value.len())
            });
            let total = node.css_styles.iter().fold(total, |subtotal, value| {
                subtotal.saturating_add(value.len())
            });
            total.saturating_add(node.icon.as_deref().map_or(0, str::len))
        });
        let edge_label_bytes = model.edges.iter().fold(0usize, |total, edge| {
            [
                edge.id.as_str(),
                edge.start.as_str(),
                edge.end.as_str(),
                edge.edge_type.as_str(),
                edge.curve.as_str(),
                edge.thickness.as_str(),
                edge.look.as_str(),
                edge.classes.as_str(),
            ]
            .into_iter()
            .fold(total, |subtotal, value| {
                subtotal.saturating_add(value.len())
            })
        });

        Self {
            nodes: model.nodes.len(),
            edges: model.edges.len(),
            label_bytes: node_label_bytes.saturating_add(edge_label_bytes),
            nesting_depth: model
                .nodes
                .iter()
                .filter_map(|node| usize::try_from(node.level).ok())
                .max()
                .unwrap_or(0),
        }
    }

    pub fn as_model_complexity(self) -> ModelComplexity {
        ModelComplexity::new(
            self.nodes.saturating_add(self.edges),
            self.label_bytes,
            self.nesting_depth,
        )
    }
}

#[cfg(feature = "diagram-class")]
impl ClassComplexity {
    pub fn from_model(model: &ClassDiagram) -> Self {
        let class_label_bytes = model
            .classes
            .values()
            .map(|node| {
                node.id
                    .len()
                    .saturating_add(node.label.len())
                    .saturating_add(node.text.len())
                    .saturating_add(node.type_param.len())
                    .saturating_add(node.css_classes.len())
                    .saturating_add(node.tooltip.as_deref().map(str::len).unwrap_or(0))
                    .saturating_add(node.link.as_deref().map(str::len).unwrap_or(0))
                    .saturating_add(
                        node.members
                            .iter()
                            .chain(node.methods.iter())
                            .map(|member| {
                                member
                                    .display_text
                                    .len()
                                    .saturating_add(member.id.len())
                                    .saturating_add(member.parameters.len())
                                    .saturating_add(member.return_type.len())
                            })
                            .sum::<usize>(),
                    )
                    .saturating_add(node.annotations.iter().map(String::len).sum::<usize>())
                    .saturating_add(node.styles.iter().map(String::len).sum::<usize>())
            })
            .sum::<usize>();
        let relation_label_bytes = model
            .relations
            .iter()
            .map(|rel| {
                rel.id
                    .len()
                    .saturating_add(rel.id1.len())
                    .saturating_add(rel.id2.len())
                    .saturating_add(rel.title.len())
                    .saturating_add(rel.relation_title_1.as_deref().map_or(0, str::len))
                    .saturating_add(rel.relation_title_2.as_deref().map_or(0, str::len))
            })
            .sum::<usize>();
        let note_label_bytes = model
            .notes
            .iter()
            .map(|note| {
                note.id
                    .len()
                    .saturating_add(note.text.len())
                    .saturating_add(note.class_id.as_deref().map(str::len).unwrap_or(0))
            })
            .sum::<usize>();
        let interface_label_bytes = model
            .interfaces
            .iter()
            .map(|iface| {
                iface
                    .id
                    .len()
                    .saturating_add(iface.label.len())
                    .saturating_add(iface.class_id.len())
            })
            .sum::<usize>();
        let namespace_label_bytes = model
            .namespaces
            .values()
            .map(|namespace| {
                namespace
                    .id
                    .len()
                    .saturating_add(namespace.label.len())
                    .saturating_add(namespace.parent.as_deref().map(str::len).unwrap_or(0))
            })
            .sum::<usize>();

        Self {
            nodes: model
                .classes
                .len()
                .saturating_add(model.notes.len())
                .saturating_add(model.interfaces.len())
                .saturating_add(model.namespaces.len()),
            edges: model.relations.len().saturating_add(
                model
                    .notes
                    .iter()
                    .filter(|note| note.class_id.is_some())
                    .count(),
            ),
            namespaces: model.namespaces.len(),
            label_bytes: class_label_bytes
                .saturating_add(relation_label_bytes)
                .saturating_add(note_label_bytes)
                .saturating_add(interface_label_bytes)
                .saturating_add(namespace_label_bytes),
            namespace_depth: class_namespace_depth(model),
        }
    }

    pub fn as_model_complexity(self) -> ModelComplexity {
        ModelComplexity::new(
            self.nodes.saturating_add(self.edges),
            self.label_bytes,
            self.namespace_depth,
        )
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg(feature = "diagram-zenuml")]
pub struct ZenumlComplexity {
    pub participants: usize,
    pub groups: usize,
    pub statements: usize,
    pub fragments: usize,
    pub label_bytes: usize,
    pub nesting_depth: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg(feature = "diagram-sequence")]
pub struct SequenceComplexity {
    pub messages: usize,
    pub block_depth: usize,
    pub items: usize,
    pub text_bytes: usize,
    pub nesting_depth: usize,
}

#[cfg(feature = "diagram-sequence")]
impl SequenceComplexity {
    pub fn from_model(model: &SequenceDiagramRenderModel) -> Self {
        const ACTOR_PROPERTY_VALUE_DEPTH: usize = 4;
        const NOTE_ACTOR_VALUE_DEPTH: usize = 3;

        let mut depth = 0usize;
        let mut block_depth = 0usize;
        for message in &model.messages {
            match message.message_type {
                10 | 12 | 15 | 19 | 22 | 27 | 30 | 32 => {
                    depth = depth.saturating_add(1);
                    block_depth = block_depth.max(depth);
                }
                11 | 14 | 16 | 21 | 23 | 29 | 31 => {
                    depth = depth.saturating_sub(1);
                }
                _ => {}
            }
        }
        let common_text_bytes = [
            model.acc_title.as_deref(),
            model.acc_descr.as_deref(),
            model.title.as_deref(),
        ]
        .into_iter()
        .flatten()
        .fold(0usize, |total, value| total.saturating_add(value.len()));
        let actor_order_text_bytes = model
            .actor_order
            .iter()
            .fold(0usize, |total, value| total.saturating_add(value.len()));
        let mut arbitrary_json = JsonValueComplexity::default();
        let actor_text_bytes = model.actors.iter().fold(0usize, |total, (key, actor)| {
            let links = json_map_complexity(&actor.links, ACTOR_PROPERTY_VALUE_DEPTH);
            let properties = json_map_complexity(&actor.properties, ACTOR_PROPERTY_VALUE_DEPTH);
            arbitrary_json.merge(links);
            arbitrary_json.merge(properties);
            total
                .saturating_add(key.len())
                .saturating_add(actor.name.len())
                .saturating_add(actor.description.len())
                .saturating_add(actor.actor_type.len())
                .saturating_add(links.text_bytes)
                .saturating_add(properties.text_bytes)
        });
        let box_text_bytes = model.boxes.iter().fold(0usize, |total, sequence_box| {
            sequence_box.actor_keys.iter().fold(
                total
                    .saturating_add(sequence_box.fill.len())
                    .saturating_add(sequence_box.name.as_deref().map_or(0, str::len)),
                |subtotal, actor| subtotal.saturating_add(actor.len()),
            )
        });
        let message_text_bytes = model.messages.iter().fold(0usize, |total, message| {
            total
                .saturating_add(message.id.len())
                .saturating_add(message.from.as_deref().map_or(0, str::len))
                .saturating_add(message.to.as_deref().map_or(0, str::len))
                .saturating_add(message.message_text().len())
        });
        let note_text_bytes = model.notes.iter().fold(0usize, |total, note| {
            let actor = json_value_complexity(&note.actor, NOTE_ACTOR_VALUE_DEPTH);
            arbitrary_json.merge(actor);
            total
                .saturating_add(actor.text_bytes)
                .saturating_add(note.message.len())
        });
        let lifecycle_text_bytes = model
            .created_actors
            .keys()
            .chain(model.destroyed_actors.keys())
            .fold(0usize, |total, value| total.saturating_add(value.len()));
        Self {
            messages: model.messages.len(),
            block_depth,
            items: model
                .actors
                .len()
                .saturating_add(model.boxes.len())
                .saturating_add(model.messages.len())
                .saturating_add(model.notes.len())
                .saturating_add(arbitrary_json.items),
            text_bytes: common_text_bytes
                .saturating_add(actor_order_text_bytes)
                .saturating_add(actor_text_bytes)
                .saturating_add(box_text_bytes)
                .saturating_add(message_text_bytes)
                .saturating_add(note_text_bytes)
                .saturating_add(lifecycle_text_bytes),
            nesting_depth: block_depth.max(arbitrary_json.nesting_depth),
        }
    }

    pub fn as_model_complexity(self) -> ModelComplexity {
        ModelComplexity::new(self.items, self.text_bytes, self.nesting_depth)
    }
}

#[cfg(feature = "diagram-zenuml")]
impl ZenumlComplexity {
    pub fn from_model(model: &ZenumlDiagramRenderModel) -> Self {
        let common_label_bytes = [model.title.as_deref(), model.starter.as_deref()]
            .into_iter()
            .flatten()
            .fold(0usize, |total, value| total.saturating_add(value.len()));
        let participant_label_bytes =
            model
                .participants
                .iter()
                .fold(0usize, |total, participant| {
                    [
                        Some(participant.name.as_str()),
                        participant.label.as_deref(),
                        participant.participant_type.as_deref(),
                        participant.stereotype.as_deref(),
                        participant.emoji.as_deref(),
                        participant.width_source.as_deref(),
                        participant.color.as_deref(),
                        participant.group_id.as_deref(),
                    ]
                    .into_iter()
                    .flatten()
                    .fold(total, |subtotal, value| {
                        subtotal.saturating_add(value.len())
                    })
                });
        let group_label_bytes = model.groups.iter().fold(0usize, |total, group| {
            group.participant_names.iter().fold(
                total.saturating_add(group.id.as_deref().map_or(0, str::len)),
                |subtotal, name| subtotal.saturating_add(name.len()),
            )
        });
        let mut complexity = Self {
            participants: model.participants.len(),
            groups: model.groups.len(),
            statements: 0,
            fragments: 0,
            label_bytes: common_label_bytes
                .saturating_add(participant_label_bytes)
                .saturating_add(group_label_bytes),
            nesting_depth: 0,
        };
        let mut pending = vec![(model.statements.as_slice(), 0usize)];
        while let Some((statements, nesting_depth)) = pending.pop() {
            complexity.nesting_depth = complexity.nesting_depth.max(nesting_depth);
            for statement in statements {
                complexity.statements = complexity.statements.saturating_add(1);
                complexity.label_bytes = complexity
                    .label_bytes
                    .saturating_add(statement.comment.as_deref().map_or(0, str::len));
                match &statement.kind {
                    ZenumlStatementKind::Message {
                        explicit_from,
                        resolved_from,
                        resolved_to,
                        label,
                        assignment,
                        body,
                        ..
                    } => {
                        complexity.label_bytes = complexity
                            .label_bytes
                            .saturating_add(explicit_from.as_deref().map_or(0, str::len))
                            .saturating_add(resolved_from.as_deref().map_or(0, str::len))
                            .saturating_add(resolved_to.as_deref().map_or(0, str::len))
                            .saturating_add(label.len())
                            .saturating_add(assignment.as_deref().map_or(0, str::len));
                        pending.push((body, nesting_depth.saturating_add(1)));
                    }
                    ZenumlStatementKind::Creation {
                        resolved_from,
                        resolved_to,
                        constructor,
                        parameters,
                        assignment,
                        label,
                        body,
                        ..
                    } => {
                        complexity.label_bytes = complexity
                            .label_bytes
                            .saturating_add(resolved_from.as_deref().map_or(0, str::len))
                            .saturating_add(resolved_to.len())
                            .saturating_add(constructor.len())
                            .saturating_add(parameters.len())
                            .saturating_add(assignment.as_deref().map_or(0, str::len))
                            .saturating_add(label.len());
                        pending.push((body, nesting_depth.saturating_add(1)));
                    }
                    ZenumlStatementKind::Return {
                        explicit_from,
                        resolved_from,
                        explicit_to,
                        resolved_to,
                        label,
                    } => {
                        complexity.label_bytes = complexity
                            .label_bytes
                            .saturating_add(explicit_from.as_deref().map_or(0, str::len))
                            .saturating_add(resolved_from.as_deref().map_or(0, str::len))
                            .saturating_add(explicit_to.as_deref().map_or(0, str::len))
                            .saturating_add(resolved_to.as_deref().map_or(0, str::len))
                            .saturating_add(label.len());
                    }
                    ZenumlStatementKind::Fragment {
                        label, sections, ..
                    } => {
                        complexity.fragments = complexity.fragments.saturating_add(1);
                        complexity.label_bytes = complexity
                            .label_bytes
                            .saturating_add(label.as_deref().map_or(0, str::len));
                        for section in sections {
                            complexity.label_bytes = complexity
                                .label_bytes
                                .saturating_add(section.label.as_deref().map_or(0, str::len));
                            pending.push((&section.statements, nesting_depth.saturating_add(1)));
                        }
                    }
                    ZenumlStatementKind::Reference {
                        participants,
                        label,
                    } => {
                        complexity.label_bytes = participants.iter().fold(
                            complexity.label_bytes.saturating_add(label.len()),
                            |total, participant| total.saturating_add(participant.len()),
                        );
                    }
                    ZenumlStatementKind::Divider { label } => {
                        complexity.label_bytes = complexity.label_bytes.saturating_add(label.len());
                    }
                }
            }
        }
        complexity
    }

    pub fn as_model_complexity(self) -> ModelComplexity {
        ModelComplexity::new(
            self.participants
                .saturating_add(self.groups)
                .saturating_add(self.statements),
            self.label_bytes,
            self.nesting_depth,
        )
    }
}

#[cfg(any(
    feature = "diagram-flowchart",
    feature = "diagram-swimlane",
    feature = "diagram-agentflow"
))]
fn flowchart_text_bytes(model: &FlowchartModel) -> usize {
    let mut total = [
        Some(model.keyword.as_str()),
        model.acc_title.as_deref(),
        model.acc_descr.as_deref(),
        model.direction.as_deref(),
    ]
    .into_iter()
    .flatten()
    .fold(0usize, |subtotal, value| {
        subtotal.saturating_add(value.len())
    });
    for (id, styles) in &model.class_defs {
        total = styles
            .iter()
            .fold(total.saturating_add(id.len()), |subtotal, value| {
                subtotal.saturating_add(value.len())
            });
    }
    total = model.vertex_calls.iter().fold(total, |subtotal, value| {
        subtotal.saturating_add(value.len())
    });
    if let Some(defaults) = &model.edge_defaults {
        total = total.saturating_add(defaults.interpolate.as_deref().map_or(0, str::len));
        total = defaults.style.iter().fold(total, |subtotal, value| {
            subtotal.saturating_add(value.len())
        });
    }
    for node in &model.nodes {
        for value in [
            Some(node.id.as_str()),
            node.label.as_deref(),
            node.label_type.as_deref(),
            node.layout_shape.as_deref(),
            node.shape.as_deref(),
            node.icon.as_deref(),
            node.form.as_deref(),
            node.pos.as_deref(),
            node.img.as_deref(),
            node.constraint.as_deref(),
            node.link.as_deref(),
            node.link_target.as_deref(),
        ]
        .into_iter()
        .flatten()
        {
            total = total.saturating_add(value.len());
        }
        total = node
            .classes
            .iter()
            .chain(&node.styles)
            .fold(total, |subtotal, value| {
                subtotal.saturating_add(value.len())
            });
    }
    for edge in &model.edges {
        for value in [
            Some(edge.id.as_str()),
            Some(edge.from.as_str()),
            Some(edge.to.as_str()),
            edge.label.as_deref(),
            edge.label_type.as_deref(),
            edge.edge_type.as_deref(),
            Some(edge.arrow.as_str()),
            edge.stroke.as_deref(),
            edge.interpolate.as_deref(),
            edge.animation.as_deref(),
        ]
        .into_iter()
        .flatten()
        {
            total = total.saturating_add(value.len());
        }
        total = edge
            .classes
            .iter()
            .chain(&edge.style)
            .fold(total, |subtotal, value| {
                subtotal.saturating_add(value.len())
            });
    }
    for subgraph in &model.subgraphs {
        for value in [
            Some(subgraph.id.as_str()),
            Some(subgraph.title.as_str()),
            subgraph.dir.as_deref(),
            subgraph.label_type.as_deref(),
        ]
        .into_iter()
        .flatten()
        {
            total = total.saturating_add(value.len());
        }
        total = subgraph
            .classes
            .iter()
            .chain(&subgraph.styles)
            .chain(&subgraph.nodes)
            .fold(total, |subtotal, value| {
                subtotal.saturating_add(value.len())
            });
    }
    total = model.tooltips.iter().fold(total, |subtotal, (id, value)| {
        subtotal
            .saturating_add(id.len())
            .saturating_add(value.len())
    });
    total
}

#[cfg(any(
    feature = "diagram-flowchart",
    feature = "diagram-swimlane",
    feature = "diagram-agentflow"
))]
fn flowchart_subgraph_depth(model: &FlowchartModel) -> usize {
    if model.subgraphs.is_empty() {
        return 0;
    }
    let indices = model
        .subgraphs
        .iter()
        .enumerate()
        .map(|(index, subgraph)| (subgraph.id.as_str(), index))
        .collect::<std::collections::HashMap<_, _>>();
    let mut children = vec![Vec::new(); model.subgraphs.len()];
    let mut incoming = vec![0usize; model.subgraphs.len()];
    for (parent, subgraph) in model.subgraphs.iter().enumerate() {
        for member in &subgraph.nodes {
            let Some(&child) = indices.get(member.as_str()) else {
                continue;
            };
            children[parent].push(child);
            incoming[child] = incoming[child].saturating_add(1);
        }
    }
    let mut queue = std::collections::VecDeque::new();
    let mut depths = vec![1usize; model.subgraphs.len()];
    for (index, incoming) in incoming.iter().enumerate() {
        if *incoming == 0 {
            queue.push_back(index);
        }
    }
    while let Some(parent) = queue.pop_front() {
        for &child in &children[parent] {
            depths[child] = depths[child].max(depths[parent].saturating_add(1));
            incoming[child] = incoming[child].saturating_sub(1);
            if incoming[child] == 0 {
                queue.push_back(child);
            }
        }
    }
    depths.into_iter().max().unwrap_or(0)
}

#[cfg(feature = "diagram-class")]
fn class_namespace_depth(model: &ClassDiagram) -> usize {
    if model.namespaces.is_empty() {
        return 0;
    }
    let indices = model
        .namespaces
        .keys()
        .enumerate()
        .map(|(index, id)| (id.as_str(), index))
        .collect::<std::collections::HashMap<_, _>>();
    let parents = model
        .namespaces
        .values()
        .map(|namespace| {
            namespace
                .parent
                .as_deref()
                .and_then(|parent| indices.get(parent).copied())
        })
        .collect::<Vec<_>>();
    let mut depths = vec![0usize; parents.len()];
    let mut completed = vec![false; parents.len()];
    for start in 0..parents.len() {
        if completed[start] {
            continue;
        }
        let mut path = Vec::new();
        let mut positions = std::collections::HashMap::new();
        let mut cursor = Some(start);
        while let Some(index) = cursor {
            if completed[index] || positions.contains_key(&index) {
                break;
            }
            positions.insert(index, path.len());
            path.push(index);
            cursor = parents[index];
        }
        let mut depth = cursor
            .filter(|index| completed[*index])
            .map(|index| depths[index])
            .unwrap_or(0);
        for index in path.into_iter().rev() {
            depth = depth.saturating_add(1);
            depths[index] = depth;
            completed[index] = true;
        }
    }
    depths.into_iter().max().unwrap_or(0)
}

#[cfg(feature = "diagram-kanban")]
fn kanban_nesting_depth(model: &KanbanDiagramRenderModel) -> usize {
    let indices = model
        .nodes
        .iter()
        .enumerate()
        .map(|(index, node)| (node.id.as_str(), index))
        .collect::<std::collections::HashMap<_, _>>();
    let parents = model
        .nodes
        .iter()
        .map(|node| {
            node.parent_id
                .as_deref()
                .and_then(|parent| indices.get(parent).copied())
        })
        .collect::<Vec<_>>();
    let mut depths = vec![0usize; parents.len()];
    let mut completed = vec![false; parents.len()];
    for start in 0..parents.len() {
        if completed[start] {
            continue;
        }
        let mut path = Vec::new();
        let mut positions = std::collections::HashMap::new();
        let mut cursor = Some(start);
        while let Some(index) = cursor {
            if completed[index] || positions.contains_key(&index) {
                break;
            }
            positions.insert(index, path.len());
            path.push(index);
            cursor = parents[index];
        }
        let mut depth = cursor
            .filter(|index| completed[*index])
            .map(|index| depths[index])
            .unwrap_or(0);
        for index in path.into_iter().rev() {
            depth = depth.saturating_add(1);
            depths[index] = depth;
            completed[index] = true;
        }
    }
    depths.into_iter().max().unwrap_or(0)
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
struct JsonValueComplexity {
    items: usize,
    text_bytes: usize,
    nesting_depth: usize,
    container_depth: usize,
}

#[cfg(feature = "diagram-sequence")]
impl JsonValueComplexity {
    fn merge(&mut self, other: Self) {
        self.items = self.items.saturating_add(other.items);
        self.text_bytes = self.text_bytes.saturating_add(other.text_bytes);
        self.nesting_depth = self.nesting_depth.max(other.nesting_depth);
        self.container_depth = self.container_depth.max(other.container_depth);
    }
}

enum JsonTraversalFrame<'a> {
    Value(&'a serde_json::Value, usize),
    Array(std::slice::Iter<'a, serde_json::Value>, usize),
    Object(serde_json::map::Iter<'a>, usize),
}

#[cfg(feature = "diagram-sequence")]
fn json_map_complexity(
    map: &serde_json::Map<String, serde_json::Value>,
    value_depth: usize,
) -> JsonValueComplexity {
    let mut complexity = JsonValueComplexity {
        items: map.len(),
        text_bytes: map
            .keys()
            .fold(0usize, |total, key| total.saturating_add(key.len())),
        nesting_depth: 0,
        container_depth: 0,
    };
    for value in map.values() {
        complexity.merge(json_value_complexity(value, value_depth));
    }
    complexity
}

/// Counts an arbitrary JSON subtree without recursive calls or width-proportional scratch space.
fn json_value_complexity(value: &serde_json::Value, initial_depth: usize) -> JsonValueComplexity {
    let mut complexity = JsonValueComplexity::default();
    let mut pending = vec![JsonTraversalFrame::Value(value, initial_depth)];
    while let Some(frame) = pending.pop() {
        match frame {
            JsonTraversalFrame::Value(value, depth) => {
                complexity.nesting_depth = complexity.nesting_depth.max(depth);
                match value {
                    serde_json::Value::String(value) => {
                        complexity.text_bytes = complexity.text_bytes.saturating_add(value.len());
                    }
                    serde_json::Value::Array(values) => {
                        complexity.container_depth = complexity.container_depth.max(depth);
                        complexity.items = complexity.items.saturating_add(values.len());
                        pending.push(JsonTraversalFrame::Array(
                            values.iter(),
                            depth.saturating_add(1),
                        ));
                    }
                    serde_json::Value::Object(values) => {
                        complexity.container_depth = complexity.container_depth.max(depth);
                        complexity.items = complexity.items.saturating_add(values.len());
                        pending.push(JsonTraversalFrame::Object(
                            values.iter(),
                            depth.saturating_add(1),
                        ));
                    }
                    serde_json::Value::Null
                    | serde_json::Value::Bool(_)
                    | serde_json::Value::Number(_) => {}
                }
            }
            JsonTraversalFrame::Array(mut values, depth) => {
                if let Some(value) = values.next() {
                    pending.push(JsonTraversalFrame::Array(values, depth));
                    pending.push(JsonTraversalFrame::Value(value, depth));
                }
            }
            JsonTraversalFrame::Object(mut values, depth) => {
                if let Some((key, value)) = values.next() {
                    complexity.text_bytes = complexity.text_bytes.saturating_add(key.len());
                    pending.push(JsonTraversalFrame::Object(values, depth));
                    pending.push(JsonTraversalFrame::Value(value, depth));
                }
            }
        }
    }
    complexity
}

#[cfg(feature = "diagram-treemap")]
fn json_text_bytes(value: &serde_json::Value) -> usize {
    json_value_complexity(value, 0).text_bytes
}

#[cfg(feature = "diagram-treemap")]
fn json_item_count(value: &serde_json::Value) -> usize {
    json_value_complexity(value, 0).items
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn iterative_json_complexity_matches_existing_serialized_shape() {
        for value in [
            serde_json::json!(null),
            serde_json::json!([]),
            serde_json::json!({"empty": [], "nested": {"name": ["value", {}]}}),
            serde_json::json!([1, "text", [[], {}]]),
        ] {
            assert_eq!(
                ModelComplexity::from_json(&value),
                ModelComplexity::from_serializable(&value),
            );
        }
    }

    #[test]
    fn generic_complexity_rejects_managed_json_depth_without_panicking() {
        let mut value = serde_json::Value::Null;
        for _ in 0..129 {
            value = serde_json::Value::Array(vec![value]);
        }
        let managed = crate::ManagedSemanticJson::from_value(value);
        assert_eq!(
            ModelComplexity::from_serializable(&managed),
            ModelComplexity::new(usize::MAX, usize::MAX, usize::MAX)
        );
    }

    #[test]
    fn custom_json_complexity_preserves_deep_payload_and_model_name() {
        let mut value = serde_json::json!("leaf");
        for _ in 0..5_000 {
            value = serde_json::Value::Array(vec![value]);
        }
        let model =
            RenderSemanticModel::CustomJson(crate::CustomJsonRenderModel::new("custom", value));
        assert_eq!(
            ModelComplexity::from_render_model(&model),
            ModelComplexity::new(5_000, 10, 4_999),
        );
        drop(model);
    }

    #[test]
    fn profile_values_are_single_source_for_input_limits() {
        assert_eq!(
            RESOURCE_PROFILE_DESCRIPTORS.len(),
            ResourceProfile::ALL.len()
        );
        assert_eq!(
            INPUT_RESOURCE_LIMIT_DESCRIPTORS.len(),
            INPUT_RESOURCE_LIMIT_COUNT
        );
        for profile in ResourceProfile::ALL {
            let policy = InputResourcePolicy::for_profile(profile);
            for limit in InputResourceLimitId::ALL {
                assert_eq!(policy.base_value(limit), policy.value(limit));
            }
        }

        let interactive = InputResourcePolicy::for_profile(ResourceProfile::Interactive);
        for (limit, expected) in [
            (InputResourceLimitId::MaxModelItems, Some(32_000)),
            (InputResourceLimitId::MaxModelTextBytes, Some(2 * MIB)),
            (InputResourceLimitId::MaxModelNestingDepth, Some(256)),
        ] {
            assert_eq!(interactive.value(limit), expected, "{limit:?}");
        }
    }

    #[test]
    fn constrained_policy_rejects_source_bytes() {
        let source_error = InputResourcePolicy::for_profile(ResourceProfile::Constrained)
            .apply_for_test(InputResourceLimitId::MaxSourceBytes, 4)
            .check_source_bytes("12345")
            .unwrap_err();
        assert_eq!(source_error.limit, "max_source_bytes");
    }

    #[cfg(feature = "diagram-flowchart")]
    #[test]
    fn constrained_policy_rejects_flowchart_cardinality() {
        let parsed = crate::Engine::new()
            .parse_diagram_for_render_model_sync(
                "flowchart TD\nA --> B",
                crate::ParseOptions::strict(),
            )
            .unwrap()
            .unwrap();
        let model_error = InputResourcePolicy::for_profile(ResourceProfile::Constrained)
            .apply_for_test(InputResourceLimitId::MaxModelItems, 2)
            .check_render_model(parsed.model())
            .unwrap_err();
        assert_eq!(model_error.limit, "max_model_items");
    }

    #[cfg(feature = "diagram-flowchart")]
    #[test]
    fn parsed_render_resource_checks_include_parser_owned_flowchart_label_sources() {
        let parsed = crate::Engine::new()
            .parse_diagram_for_render_model_sync(
                "flowchart LR\nA[\"&nbsp;entity&nbsp;\"] --> B\n",
                crate::ParseOptions::strict(),
            )
            .unwrap()
            .unwrap();
        let model_complexity = ModelComplexity::from_render_model(parsed.model());
        assert!(parsed.retained_render_context_bytes() > 0);

        let policy = InputResourcePolicy::for_profile(ResourceProfile::UnboundedForTrustedInput)
            .with_limit(
                InputResourceLimitId::MaxModelTextBytes,
                model_complexity.text_bytes,
            )
            .unwrap();
        policy
            .check_render_model(parsed.model())
            .expect("the public model alone fits its exact text budget");
        let error = policy.check_parsed_render(&parsed).unwrap_err();
        assert_eq!(error.limit, "max_model_text_bytes");
        assert!(error.actual > model_complexity.text_bytes);
    }

    #[cfg(feature = "diagram-sequence")]
    #[test]
    fn sequence_complexity_bounds_messages_and_nested_frames() {
        let parsed = crate::Engine::new()
            .parse_diagram_for_render_model_sync(
                "sequenceDiagram\nloop outer\nrect rgb(240,240,240)\nA->>B: hi\nend\nend",
                crate::ParseOptions::strict(),
            )
            .unwrap()
            .unwrap();
        let crate::RenderSemanticModel::Sequence(model) = parsed.model() else {
            panic!("expected Sequence model");
        };
        let complexity = SequenceComplexity::from_model(model);
        assert_eq!(complexity.block_depth, 2);
        assert_eq!(complexity.messages, model.messages.len());

        let error = InputResourcePolicy::for_profile(ResourceProfile::UnboundedForTrustedInput)
            .with_limit(InputResourceLimitId::MaxModelNestingDepth, 1)
            .unwrap()
            .check_render_model(parsed.model())
            .unwrap_err();
        assert_eq!(error.limit, "max_model_nesting_depth");
        assert_eq!(error.actual, 2);
        assert_eq!(error.max, 1);
    }

    #[cfg(feature = "diagram-sequence")]
    #[test]
    fn sequence_complexity_counts_arbitrary_json_items_and_depth_iteratively() {
        let parsed = crate::Engine::new()
            .parse_diagram_for_render_model_sync(
                "sequenceDiagram\nparticipant A",
                crate::ParseOptions::strict(),
            )
            .unwrap()
            .unwrap();
        let (_, mut render_model) = parsed.into_parts();
        let RenderSemanticModel::Sequence(model) = &mut render_model else {
            panic!("expected Sequence model");
        };
        let mut nested = serde_json::Value::Null;
        for _ in 0..300 {
            nested = serde_json::Value::Array(vec![nested]);
        }
        model
            .actors
            .get_mut("A")
            .unwrap()
            .properties
            .insert("payload".to_string(), nested);

        let complexity = SequenceComplexity::from_model(model);
        assert_eq!(complexity.nesting_depth, 304);
        assert_eq!(complexity.items, 302);

        let exact = InputResourcePolicy::for_profile(ResourceProfile::UnboundedForTrustedInput)
            .with_limit(
                InputResourceLimitId::MaxModelNestingDepth,
                complexity.nesting_depth,
            )
            .unwrap();
        exact
            .check_render_model(&render_model)
            .expect("the exact JSON depth boundary should pass");

        let below = InputResourcePolicy::for_profile(ResourceProfile::UnboundedForTrustedInput)
            .with_limit(
                InputResourceLimitId::MaxModelNestingDepth,
                complexity.nesting_depth - 1,
            )
            .unwrap();
        let error = below
            .check_render_model(&render_model)
            .expect_err("the N-1 JSON depth boundary should fail");
        assert_eq!(error.limit, "max_model_nesting_depth");
        assert_eq!(error.actual, complexity.nesting_depth);
        assert_eq!(error.max, complexity.nesting_depth - 1);
    }

    #[cfg(feature = "diagram-mindmap")]
    #[test]
    fn mindmap_model_limits_cover_pre_layout_cardinality_and_labels() {
        let constrained = InputResourcePolicy::for_profile(ResourceProfile::Constrained);
        assert_eq!(
            constrained.value(InputResourceLimitId::MaxModelItems),
            Some(16_000)
        );

        let parsed = crate::Engine::new()
            .parse_diagram_for_render_model_sync(
                "mindmap\n  Root\n    First child\n    Second child\n",
                crate::ParseOptions::strict(),
            )
            .unwrap()
            .unwrap();
        let RenderSemanticModel::Mindmap(model) = parsed.model() else {
            panic!("expected Mindmap model");
        };
        let complexity = MindmapComplexity::from_model(model);
        assert_eq!(complexity.nodes, 3);
        assert_eq!(complexity.edges, 2);
        assert!(complexity.label_bytes >= "RootFirst childSecond child".len());

        for (id, max, expected_limit) in [
            (InputResourceLimitId::MaxModelItems, 4, "max_model_items"),
            (
                InputResourceLimitId::MaxModelTextBytes,
                1,
                "max_model_text_bytes",
            ),
        ] {
            let error = InputResourcePolicy::for_profile(ResourceProfile::UnboundedForTrustedInput)
                .with_limit(id, max)
                .unwrap()
                .check_render_model(parsed.model())
                .unwrap_err();
            assert_eq!(error.phase, InputResourceLimitPhase::Model);
            assert_eq!(error.limit, expected_limit);
        }
    }

    #[cfg(any(
        feature = "diagram-kanban",
        feature = "diagram-radar",
        feature = "diagram-requirement"
    ))]
    #[test]
    fn generic_model_budget_covers_new_family_models_without_family_knobs() {
        let models = [
            #[cfg(feature = "diagram-kanban")]
            RenderSemanticModel::Kanban(KanbanDiagramRenderModel {
                nodes: vec![crate::diagrams::kanban::KanbanRenderNode::new(
                    "todo", "Todo",
                )],
            }),
            #[cfg(feature = "diagram-radar")]
            RenderSemanticModel::Radar({
                let mut model = RadarDiagramRenderModel::default();
                model.axes = vec![crate::diagrams::radar::RadarRenderAxis {
                    name: "speed".to_string(),
                    label: "Speed".to_string(),
                }];
                model
            }),
            #[cfg(feature = "diagram-requirement")]
            RenderSemanticModel::Requirement(RequirementDiagramRenderModel {
                direction: "TB".to_string(),
                requirements: vec![crate::diagrams::requirement::RequirementRenderNode {
                    name: "R1".to_string(),
                    node_type: "requirement".to_string(),
                    requirement_id: "1".to_string(),
                    text: "Must render".to_string(),
                    risk: "Low".to_string(),
                    verify_method: "Test".to_string(),
                    css_styles: Vec::new(),
                    classes: Vec::new(),
                }],
                acc_title: None,
                acc_descr: None,
                elements: Vec::new(),
                relationships: Vec::new(),
                classes: Default::default(),
            }),
        ];
        for model in models {
            let error = InputResourcePolicy::for_profile(ResourceProfile::UnboundedForTrustedInput)
                .with_limit(InputResourceLimitId::MaxModelTextBytes, 1)
                .unwrap()
                .check_render_model(&model)
                .unwrap_err();
            assert_eq!(error.limit, "max_model_text_bytes");
        }
    }

    #[cfg(feature = "diagram-kanban")]
    #[test]
    fn kanban_complexity_accounts_for_parent_chain_depth_without_quadratic_scans() {
        let root = KanbanDiagramRenderModel {
            nodes: ["section", "lane", "card"]
                .into_iter()
                .enumerate()
                .map(|(index, id)| {
                    let mut node =
                        crate::diagrams::kanban::KanbanRenderNode::new(id, format!("Node {id}"));
                    if index > 0 {
                        node.parent_id = Some(["section", "lane"][index - 1].to_string());
                    }
                    node
                })
                .collect(),
        };

        assert_eq!(ModelComplexity::from_kanban(&root).nesting_depth, 3);
    }

    #[cfg(feature = "diagram-treemap")]
    #[test]
    fn treemap_complexity_handles_deep_typed_trees_without_serde_recursion() {
        let mut nodes = vec![crate::diagrams::treemap::TreemapNodeRenderModel {
            name: "leaf".to_string(),
            ..Default::default()
        }];
        for index in (0..1_500).rev() {
            nodes.push(crate::diagrams::treemap::TreemapNodeRenderModel {
                name: format!("section{index}"),
                children: Some(vec![nodes.len() - 1]),
                ..Default::default()
            });
        }
        let model = TreemapDiagramRenderModel {
            root: nodes.pop().unwrap(),
            nodes,
            ..Default::default()
        };

        let complexity = TreemapComplexity::from_model(&model);

        assert_eq!(complexity.nodes, 1_501);
        assert_eq!(complexity.nesting_depth, 1_500);
        assert!(complexity.label_bytes >= "leaf".len());
    }

    #[cfg(feature = "diagram-ishikawa")]
    #[test]
    fn ishikawa_complexity_handles_deep_typed_trees_without_serde_recursion() {
        let mut nodes = vec![crate::diagrams::ishikawa::IshikawaNodeRenderModel {
            text: "leaf".to_string(),
            children: Vec::new(),
        }];
        for index in (0..1_500).rev() {
            nodes.push(crate::diagrams::ishikawa::IshikawaNodeRenderModel {
                text: format!("cause{index}"),
                children: vec![nodes.len() - 1],
            });
        }
        let model = IshikawaDiagramRenderModel {
            root: Some(nodes.len() - 1),
            nodes,
            ..Default::default()
        };

        let complexity = IshikawaComplexity::from_model(&model);

        assert_eq!(complexity.nodes, 1_501);
        assert_eq!(complexity.nesting_depth, 1_500);
        assert!(complexity.label_bytes >= "leaf".len());
    }

    impl InputResourcePolicy {
        fn apply_for_test(mut self, id: InputResourceLimitId, value: usize) -> Self {
            self.apply_limit(id, value).unwrap();
            self
        }
    }
}
