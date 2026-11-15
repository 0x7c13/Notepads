#[cfg(any(
    feature = "diagram-flowchart",
    feature = "diagram-swimlane",
    feature = "diagram-agentflow"
))]
use merman_core::diagrams::flowchart::FlowchartModel;
#[cfg(feature = "diagram-mindmap")]
use merman_core::diagrams::mindmap::MindmapDiagramRenderModel;
#[cfg(feature = "diagram-zenuml")]
use merman_core::diagrams::zenuml::ZenumlDiagramRenderModel;
#[cfg(feature = "diagram-class")]
use merman_core::models::class_diagram::ClassDiagram;
use merman_core::resources::{
    InputResourceLimitExceeded, InputResourceLimitId, InputResourceLimitPhase, InputResourcePolicy,
};
pub use merman_core::resources::{
    ModelComplexity, RESOURCE_PROFILE_DESCRIPTORS, ResourceProfile as RenderResourceProfile,
    ResourceProfileDescriptor as RenderResourceProfileDescriptor,
};
use merman_core::{
    OperationCancelled, OperationControl, OperationLedgerError, OperationPhase,
    OperationResourceDomain, OperationResourceLimitExceeded, OperationResourceOverride,
    OperationResourceProvenance, ParsedDiagramRender, RenderSemanticModel,
};

const KIB: usize = 1024;
const MIB: usize = 1024 * KIB;

/// Hard recursion cap used by the SVG backend on WebAssembly targets.
///
/// Icon-body admission is intentionally derived from this smallest supported backend cap so a
/// registry accepted on native targets remains portable to WebAssembly.
pub const WASM_RESVG_TREE_DEPTH_HARD_CAP: usize = 64;
/// Maximum XML nesting accepted inside a portable icon body.
///
/// The icon renderer adds at least one wrapping `<g>` before whole-document validation, so this
/// remains strictly below the WebAssembly backend hard cap.
pub const MAX_PORTABLE_ICON_BODY_XML_DEPTH: usize = 32;
const _: () = assert!(MAX_PORTABLE_ICON_BODY_XML_DEPTH < WASM_RESVG_TREE_DEPTH_HARD_CAP);

#[cfg(not(target_arch = "wasm32"))]
pub const MAX_RESVG_TREE_DEPTH: usize = 256;
pub const SVG_BACKEND_TREE_DEPTH_HARD_CAP_ID: &str = "svg_backend_tree_depth";
/// Maximum resolved usvg nodes accepted independently of caller-selected policies.
///
/// This matches usvg's own parser capability while rejecting expansion before it can allocate the
/// upstream parser's full tree.
pub const MAX_RESVG_TREE_NODES: usize = 1_000_000;
pub const SVG_BACKEND_TREE_NODES_HARD_CAP_ID: &str = "svg_backend_tree_nodes";

#[cfg(target_arch = "wasm32")]
pub const MAX_RESVG_TREE_DEPTH: usize = WASM_RESVG_TREE_DEPTH_HARD_CAP;

pub const RESOURCE_PROFILE_COUNT: usize = merman_core::resources::RESOURCE_PROFILE_COUNT;
const RENDER_RESOURCE_LIMIT_COUNT: usize = 5;
pub const RESOURCE_LIMIT_COUNT: usize =
    merman_core::resources::INPUT_RESOURCE_LIMIT_COUNT + RENDER_RESOURCE_LIMIT_COUNT;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ResourceLimitPhase {
    Source,
    LayoutModel,
    SvgOutput,
    SvgPostprocess,
}

/// Stable reason why a resource check failed.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum ResourceLimitCause {
    /// The requested work exceeded the configured policy ceiling.
    Ceiling,
    /// Computing cumulative work overflowed the platform counter.
    ArithmeticOverflow,
}

impl ResourceLimitCause {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Ceiling => "ceiling",
            Self::ArithmeticOverflow => "arithmetic_overflow",
        }
    }
}

impl std::fmt::Display for ResourceLimitCause {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

impl ResourceLimitPhase {
    const ALL: [Self; 4] = [
        Self::Source,
        Self::LayoutModel,
        Self::SvgOutput,
        Self::SvgPostprocess,
    ];

    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Source => "source",
            Self::LayoutModel => "layout_model",
            Self::SvgOutput => "svg_output",
            Self::SvgPostprocess => "svg_postprocess",
        }
    }

    fn from_stable_id(value: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|phase| phase.as_str() == value)
    }
}

impl std::fmt::Display for ResourceLimitPhase {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

#[repr(usize)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum RenderResourceLimitId {
    MaxSvgBytes,
    MaxSvgElements,
    MaxLayoutWorkUnits,
    SvgBackendTreeNodes,
    SvgBackendTreeDepth,
}

impl RenderResourceLimitId {
    pub const ALL: [Self; RENDER_RESOURCE_LIMIT_COUNT] = [
        Self::MaxSvgBytes,
        Self::MaxSvgElements,
        Self::MaxLayoutWorkUnits,
        Self::SvgBackendTreeNodes,
        Self::SvgBackendTreeDepth,
    ];

    const fn index(self) -> usize {
        self as usize
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ResourceLimitId {
    Input(InputResourceLimitId),
    Render(RenderResourceLimitId),
}

#[allow(non_upper_case_globals)]
impl ResourceLimitId {
    pub const MaxSourceBytes: Self = Self::Input(InputResourceLimitId::MaxSourceBytes);
    pub const MaxModelItems: Self = Self::Input(InputResourceLimitId::MaxModelItems);
    pub const MaxModelTextBytes: Self = Self::Input(InputResourceLimitId::MaxModelTextBytes);
    pub const MaxModelNestingDepth: Self = Self::Input(InputResourceLimitId::MaxModelNestingDepth);
    pub const MaxSvgBytes: Self = Self::Render(RenderResourceLimitId::MaxSvgBytes);
    pub const MaxSvgElements: Self = Self::Render(RenderResourceLimitId::MaxSvgElements);
    pub const MaxLayoutWorkUnits: Self = Self::Render(RenderResourceLimitId::MaxLayoutWorkUnits);
    pub const SvgBackendTreeNodes: Self = Self::Render(RenderResourceLimitId::SvgBackendTreeNodes);
    pub const SvgBackendTreeDepth: Self = Self::Render(RenderResourceLimitId::SvgBackendTreeDepth);

    pub const ALL: [Self; RESOURCE_LIMIT_COUNT] = [
        Self::MaxSourceBytes,
        Self::MaxModelItems,
        Self::MaxModelTextBytes,
        Self::MaxModelNestingDepth,
        Self::MaxLayoutWorkUnits,
        Self::MaxSvgBytes,
        Self::MaxSvgElements,
        Self::SvgBackendTreeNodes,
        Self::SvgBackendTreeDepth,
    ];

    pub fn from_stable_id(id: &str) -> Option<Self> {
        InputResourceLimitId::from_stable_id(id)
            .map(Self::Input)
            .or_else(|| {
                RENDER_RESOURCE_LIMIT_DESCRIPTORS
                    .iter()
                    .find(|descriptor| descriptor.stable_id == id)
                    .map(|descriptor| descriptor.id)
            })
    }

    pub const fn descriptor(self) -> ResourceLimitDescriptor {
        match self {
            Self::Input(id) => input_descriptor(id),
            Self::Render(id) => RENDER_RESOURCE_LIMIT_DESCRIPTORS[id.index()],
        }
    }

    pub const fn as_str(self) -> &'static str {
        self.descriptor().stable_id
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub struct ResourceLimitDescriptor {
    pub id: ResourceLimitId,
    pub stable_id: &'static str,
    pub phase: ResourceLimitPhase,
    pub description: &'static str,
    pub overridable: bool,
    pub hard_cap: bool,
    pub minimum_value: usize,
}

const fn input_descriptor(id: InputResourceLimitId) -> ResourceLimitDescriptor {
    let descriptor = id.descriptor();
    ResourceLimitDescriptor {
        id: ResourceLimitId::Input(id),
        stable_id: descriptor.stable_id,
        phase: match descriptor.phase {
            InputResourceLimitPhase::Source => ResourceLimitPhase::Source,
            InputResourceLimitPhase::Model => ResourceLimitPhase::LayoutModel,
        },
        description: descriptor.description,
        overridable: descriptor.overridable,
        hard_cap: false,
        minimum_value: descriptor.minimum_value,
    }
}

const RENDER_RESOURCE_LIMIT_DESCRIPTORS: [ResourceLimitDescriptor; RENDER_RESOURCE_LIMIT_COUNT] = [
    ResourceLimitDescriptor {
        id: ResourceLimitId::MaxSvgBytes,
        stable_id: "max_svg_bytes",
        phase: ResourceLimitPhase::SvgOutput,
        description: "Maximum serialized SVG bytes",
        overridable: true,
        hard_cap: false,
        minimum_value: 1,
    },
    ResourceLimitDescriptor {
        id: ResourceLimitId::MaxSvgElements,
        stable_id: "max_svg_elements",
        phase: ResourceLimitPhase::SvgPostprocess,
        description: "Maximum SVG element count",
        overridable: true,
        hard_cap: false,
        minimum_value: 1,
    },
    ResourceLimitDescriptor {
        id: ResourceLimitId::MaxLayoutWorkUnits,
        stable_id: "max_layout_work_units",
        phase: ResourceLimitPhase::LayoutModel,
        description: "Maximum family-accounted derived layout and render geometry work units",
        overridable: true,
        hard_cap: false,
        minimum_value: 1,
    },
    ResourceLimitDescriptor {
        id: ResourceLimitId::SvgBackendTreeNodes,
        stable_id: SVG_BACKEND_TREE_NODES_HARD_CAP_ID,
        phase: ResourceLimitPhase::SvgPostprocess,
        description: "Maximum SVG backend tree nodes accepted by rendering backends",
        overridable: false,
        hard_cap: true,
        minimum_value: 1,
    },
    ResourceLimitDescriptor {
        id: ResourceLimitId::SvgBackendTreeDepth,
        stable_id: SVG_BACKEND_TREE_DEPTH_HARD_CAP_ID,
        phase: ResourceLimitPhase::SvgPostprocess,
        description: "Maximum SVG backend tree depth accepted by rendering backends",
        overridable: false,
        hard_cap: true,
        minimum_value: 1,
    },
];

pub static RESOURCE_LIMIT_DESCRIPTORS: [ResourceLimitDescriptor; RESOURCE_LIMIT_COUNT] = [
    input_descriptor(InputResourceLimitId::MaxSourceBytes),
    input_descriptor(InputResourceLimitId::MaxModelItems),
    input_descriptor(InputResourceLimitId::MaxModelTextBytes),
    input_descriptor(InputResourceLimitId::MaxModelNestingDepth),
    RENDER_RESOURCE_LIMIT_DESCRIPTORS[2],
    RENDER_RESOURCE_LIMIT_DESCRIPTORS[0],
    RENDER_RESOURCE_LIMIT_DESCRIPTORS[1],
    RENDER_RESOURCE_LIMIT_DESCRIPTORS[3],
    RENDER_RESOURCE_LIMIT_DESCRIPTORS[4],
];

const RENDER_PROFILE_VALUES: [[Option<usize>; RESOURCE_PROFILE_COUNT];
    RENDER_RESOURCE_LIMIT_COUNT] = [
    [Some(24 * MIB), Some(12 * MIB), Some(128 * MIB), None],
    [Some(250_000), Some(125_000), Some(1_000_000), None],
    // A deterministic work-admission budget, not a Mermaid limit or response-time bound.
    // Interactive and trusted-native admit ordinary diagrams, including nested ELK layouts;
    // constrained remains a tighter host policy. Hosts control latency through cancellation,
    // deadlines, and concurrency. See the dated layout-work calibration receipts in
    // docs/performance for the corpus and the profile-specific rounding rules.
    [Some(14_100_000), Some(125_000), Some(15_000_000), None],
    [Some(MAX_RESVG_TREE_NODES); RESOURCE_PROFILE_COUNT],
    [Some(MAX_RESVG_TREE_DEPTH); RESOURCE_PROFILE_COUNT],
];

pub const GENERAL_BINDING_DEFAULT_RESOURCE_PROFILE: RenderResourceProfile =
    merman_core::resources::GENERAL_BINDING_DEFAULT_RESOURCE_PROFILE;
pub const CLI_DEFAULT_RESOURCE_PROFILE: RenderResourceProfile =
    merman_core::resources::CLI_DEFAULT_RESOURCE_PROFILE;

pub const fn resource_profile_descriptors() -> &'static [RenderResourceProfileDescriptor] {
    &merman_core::resources::RESOURCE_PROFILE_DESCRIPTORS
}

pub const fn resource_limit_descriptors() -> &'static [ResourceLimitDescriptor] {
    &RESOURCE_LIMIT_DESCRIPTORS
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum ResourceLimitOverrideError {
    #[error("resource limit id `{0}` is not part of resource contract schema 1")]
    UnknownLimit(String),
    #[error("resource limit `{0}` is a hard implementation capability and cannot be overridden")]
    HardCap(&'static str),
    #[error("resource limit `{0}` must be a positive integer")]
    NonPositive(&'static str),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RenderResourcePolicy {
    input: InputResourcePolicy,
    render_base_values: [Option<usize>; RENDER_RESOURCE_LIMIT_COUNT],
    render_effective_values: [Option<usize>; RENDER_RESOURCE_LIMIT_COUNT],
    render_explicit_overrides: [Option<usize>; RENDER_RESOURCE_LIMIT_COUNT],
}

impl Default for RenderResourcePolicy {
    fn default() -> Self {
        Self::interactive()
    }
}

impl RenderResourcePolicy {
    pub const fn profile(self) -> RenderResourceProfile {
        self.input.profile()
    }

    pub const fn interactive() -> Self {
        Self::for_profile(RenderResourceProfile::Interactive)
    }

    pub const fn constrained() -> Self {
        Self::for_profile(RenderResourceProfile::Constrained)
    }

    pub const fn trusted_native() -> Self {
        Self::for_profile(RenderResourceProfile::TrustedNative)
    }

    pub const fn unbounded_for_trusted_input() -> Self {
        Self::for_profile(RenderResourceProfile::UnboundedForTrustedInput)
    }

    pub const fn for_profile(profile: RenderResourceProfile) -> Self {
        let mut render_values = [None; RENDER_RESOURCE_LIMIT_COUNT];
        let mut index = 0;
        while index < RENDER_RESOURCE_LIMIT_COUNT {
            render_values[index] = RENDER_PROFILE_VALUES[index][profile as usize];
            index += 1;
        }
        Self {
            input: InputResourcePolicy::for_profile(profile),
            render_base_values: render_values,
            render_effective_values: render_values,
            render_explicit_overrides: [None; RENDER_RESOURCE_LIMIT_COUNT],
        }
    }

    pub const fn input_policy(&self) -> &InputResourcePolicy {
        &self.input
    }

    pub const fn value(self, id: ResourceLimitId) -> Option<usize> {
        match id {
            ResourceLimitId::Input(id) => self.input.value(id),
            ResourceLimitId::Render(id) => self.render_effective_values[id.index()],
        }
    }

    pub const fn base_value(self, id: ResourceLimitId) -> Option<usize> {
        match id {
            ResourceLimitId::Input(id) => self.input.base_value(id),
            ResourceLimitId::Render(id) => self.render_base_values[id.index()],
        }
    }

    pub const fn explicit_override(self, id: ResourceLimitId) -> Option<usize> {
        match id {
            ResourceLimitId::Input(id) => self.input.explicit_override(id),
            ResourceLimitId::Render(id) => self.render_explicit_overrides[id.index()],
        }
    }

    pub fn explicit_overrides(&self) -> impl Iterator<Item = (ResourceLimitId, usize)> + '_ {
        ResourceLimitId::ALL
            .into_iter()
            .filter_map(|id| self.explicit_override(id).map(|value| (id, value)))
    }

    pub fn apply_override(
        &mut self,
        stable_id: &str,
        value: usize,
    ) -> Result<(), ResourceLimitOverrideError> {
        let id = ResourceLimitId::from_stable_id(stable_id)
            .ok_or_else(|| ResourceLimitOverrideError::UnknownLimit(stable_id.to_string()))?;
        self.apply_limit(id, value)
    }

    pub fn apply_limit(
        &mut self,
        id: ResourceLimitId,
        value: usize,
    ) -> Result<(), ResourceLimitOverrideError> {
        match id {
            ResourceLimitId::Input(id) => {
                self.input
                    .apply_limit(id, value)
                    .map_err(|error| match error {
                        merman_core::resources::InputResourceLimitOverrideError::UnknownLimit(
                            id,
                        ) => ResourceLimitOverrideError::UnknownLimit(id),
                        merman_core::resources::InputResourceLimitOverrideError::NonPositive(
                            id,
                        ) => ResourceLimitOverrideError::NonPositive(id),
                    })
            }
            ResourceLimitId::Render(id) => {
                let descriptor = RENDER_RESOURCE_LIMIT_DESCRIPTORS[id.index()];
                if descriptor.hard_cap || !descriptor.overridable {
                    return Err(ResourceLimitOverrideError::HardCap(descriptor.stable_id));
                }
                if value == 0 {
                    return Err(ResourceLimitOverrideError::NonPositive(
                        descriptor.stable_id,
                    ));
                }
                self.render_effective_values[id.index()] = Some(value);
                self.render_explicit_overrides[id.index()] = Some(value);
                Ok(())
            }
        }
    }

    pub fn with_override(
        mut self,
        stable_id: &str,
        value: usize,
    ) -> Result<Self, ResourceLimitOverrideError> {
        self.apply_override(stable_id, value)?;
        Ok(self)
    }

    pub fn with_limit(
        mut self,
        id: ResourceLimitId,
        value: usize,
    ) -> Result<Self, ResourceLimitOverrideError> {
        self.apply_limit(id, value)?;
        Ok(self)
    }

    fn check_render_limit(
        &self,
        phase: ResourceLimitPhase,
        id: RenderResourceLimitId,
        actual: usize,
    ) -> Result<(), ResourceLimitExceeded> {
        let Some(max) = self.render_effective_values[id.index()] else {
            return Ok(());
        };
        if actual <= max {
            return Ok(());
        }
        let limit = ResourceLimitId::Render(id);
        Err(ResourceLimitExceeded {
            cause: ResourceLimitCause::Ceiling,
            phase,
            limit: limit.as_str(),
            actual,
            max,
            profile: self.profile(),
            explicit_overrides: self
                .explicit_overrides()
                .map(|(id, value)| ResourceLimitOverride { id, value })
                .collect(),
        })
    }

    pub fn check_source_bytes(&self, source: &str) -> Result<(), ResourceLimitExceeded> {
        self.input
            .check_source_bytes(source)
            .map_err(|error| ResourceLimitExceeded::from_input(self, error))
    }

    pub fn check_render_model(
        &self,
        model: &RenderSemanticModel,
    ) -> Result<(), ResourceLimitExceeded> {
        let complexity = ModelComplexity::from_render_model(model);
        self.check_model_complexity(complexity)
    }

    pub fn check_parsed_render(
        &self,
        parsed: &ParsedDiagramRender,
    ) -> Result<(), ResourceLimitExceeded> {
        let mut complexity = ModelComplexity::from_render_model(parsed.model());
        complexity.text_bytes = complexity
            .text_bytes
            .saturating_add(parsed.retained_render_context_bytes());
        self.check_model_complexity(complexity)?;
        Ok(())
    }

    pub fn check_model_complexity(
        &self,
        complexity: ModelComplexity,
    ) -> Result<(), ResourceLimitExceeded> {
        self.input
            .check_model_complexity(complexity)
            .map_err(|error| ResourceLimitExceeded::from_input(self, error))
    }

    pub fn check_svg_bytes(
        &self,
        svg: &str,
        phase: ResourceLimitPhase,
    ) -> Result<(), ResourceLimitExceeded> {
        self.check_svg_byte_count(svg.len(), phase)
    }

    pub(crate) fn check_svg_byte_count(
        &self,
        bytes: usize,
        phase: ResourceLimitPhase,
    ) -> Result<(), ResourceLimitExceeded> {
        self.check_render_limit(phase, RenderResourceLimitId::MaxSvgBytes, bytes)
    }

    pub fn check_svg_structure(
        &self,
        elements: usize,
        tree_depth: usize,
    ) -> Result<(), ResourceLimitExceeded> {
        self.check_render_limit(
            ResourceLimitPhase::SvgPostprocess,
            RenderResourceLimitId::MaxSvgElements,
            elements,
        )?;
        self.check_render_limit(
            ResourceLimitPhase::SvgPostprocess,
            RenderResourceLimitId::SvgBackendTreeNodes,
            elements,
        )?;
        self.check_render_limit(
            ResourceLimitPhase::SvgPostprocess,
            RenderResourceLimitId::SvgBackendTreeDepth,
            tree_depth,
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
    ) -> Result<FlowchartComplexity, ResourceLimitExceeded> {
        self.input
            .check_flowchart_complexity(model)
            .map_err(|error| ResourceLimitExceeded::from_input(self, error))
    }

    #[cfg(feature = "diagram-class")]
    pub fn check_class_complexity(
        &self,
        model: &ClassDiagram,
    ) -> Result<ClassComplexity, ResourceLimitExceeded> {
        self.input
            .check_class_complexity(model)
            .map_err(|error| ResourceLimitExceeded::from_input(self, error))
    }

    #[cfg(feature = "diagram-mindmap")]
    pub fn check_mindmap_complexity(
        &self,
        model: &MindmapDiagramRenderModel,
    ) -> Result<MindmapComplexity, ResourceLimitExceeded> {
        self.input
            .check_mindmap_complexity(model)
            .map_err(|error| ResourceLimitExceeded::from_input(self, error))
    }

    #[cfg(feature = "diagram-zenuml")]
    pub fn check_zenuml_complexity(
        &self,
        model: &ZenumlDiagramRenderModel,
    ) -> Result<ZenumlComplexity, ResourceLimitExceeded> {
        self.input
            .check_zenuml_complexity(model)
            .map_err(|error| ResourceLimitExceeded::from_input(self, error))
    }

    #[cfg(feature = "diagram-sequence")]
    pub fn check_sequence_complexity(
        &self,
        model: &merman_core::diagrams::sequence::SequenceDiagramRenderModel,
    ) -> Result<SequenceComplexity, ResourceLimitExceeded> {
        self.input
            .check_sequence_complexity(model)
            .map_err(|error| ResourceLimitExceeded::from_input(self, error))
    }

    pub fn check_layout_work_units(&self, work_units: usize) -> Result<(), ResourceLimitExceeded> {
        self.check_render_limit(
            ResourceLimitPhase::LayoutModel,
            RenderResourceLimitId::MaxLayoutWorkUnits,
            work_units,
        )
    }
}

/// One cumulative derived-geometry budget shared by layout and SVG emission.
/// Failure returned by controlled render work accounting.
///
/// Cancellation is deliberately kept separate from a resource rejection. Adapter kernels may
/// expose only a neutral interruption signal, so the owning work control retains this structured
/// cause until the outer renderer maps it to [`crate::Error`].
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub(crate) enum OperationWorkError {
    #[error(transparent)]
    Cancelled(#[from] OperationCancelled),
    #[error(transparent)]
    ResourceLimitExceeded(#[from] ResourceLimitExceeded),
    #[error(transparent)]
    ForeignResourceTerminal(OperationLedgerError),
}

#[cfg_attr(
    not(feature = "all-diagrams"),
    allow(
        dead_code,
        reason = "Shared operation support has different consumers in each diagram selection."
    )
)]
pub(crate) struct OperationWorkMeter {
    policy: RenderResourcePolicy,
    control: OperationControl,
    used: std::sync::atomic::AtomicUsize,
    projected_svg_bytes: std::sync::atomic::AtomicUsize,
}

#[cfg_attr(
    not(feature = "all-diagrams"),
    allow(
        dead_code,
        reason = "Shared operation support has different consumers in each diagram selection."
    )
)]
pub(crate) struct SvgByteReservation {
    pub(crate) additional_bytes: usize,
    pub(crate) limit_error: Option<ResourceLimitExceeded>,
}

#[cfg_attr(
    not(feature = "all-diagrams"),
    allow(
        dead_code,
        reason = "Shared operation support has different consumers in each diagram selection."
    )
)]
impl OperationWorkMeter {
    #[cfg_attr(not(feature = "layout-elk"), allow(dead_code))]
    pub(crate) fn new(policy: RenderResourcePolicy) -> Self {
        Self::new_with_control(policy, OperationControl::new())
    }

    pub(crate) fn new_with_control(
        policy: RenderResourcePolicy,
        control: OperationControl,
    ) -> Self {
        Self {
            policy,
            control,
            used: std::sync::atomic::AtomicUsize::new(0),
            projected_svg_bytes: std::sync::atomic::AtomicUsize::new(0),
        }
    }

    pub(crate) const fn policy(&self) -> RenderResourcePolicy {
        self.policy
    }

    pub(crate) fn control(&self) -> &OperationControl {
        &self.control
    }

    /// Replays the operation's first cancellation, ceiling, or overflow terminal.
    pub(crate) fn checkpoint(&self, phase: OperationPhase) -> Result<(), OperationWorkError> {
        self.control
            .terminal_checkpoint_at(phase)
            .map_err(|error| self.map_terminal_error(error))
    }

    fn resource_checkpoint(&self, phase: OperationPhase) -> Result<(), OperationWorkError> {
        self.checkpoint(phase)
    }

    pub(crate) fn preflight(&self, additional: usize) -> Result<(), OperationWorkError> {
        let phase = OperationPhase::Layout;
        self.resource_checkpoint(phase)?;
        if additional == 0 {
            return Ok(());
        }
        let used = self.used.load(std::sync::atomic::Ordering::Relaxed);
        let Some(next) = used.checked_add(additional) else {
            return Err(self.terminate_resource_error(
                self.layout_work_overflow_details(),
                phase,
                used,
                additional,
            ));
        };
        match self.policy.check_layout_work_units(next) {
            Ok(()) => Ok(()),
            Err(error) => Err(self.terminate_resource_error(error, phase, used, additional)),
        }
    }

    /// Charges work atomically. A rejected charge leaves the cumulative usage unchanged.
    pub(crate) fn charge(&self, additional: usize) -> Result<(), OperationWorkError> {
        let phase = OperationPhase::Layout;
        self.resource_checkpoint(phase)?;
        if additional == 0 {
            return Ok(());
        }
        let mut used = self.used.load(std::sync::atomic::Ordering::Relaxed);
        loop {
            let Some(next) = used.checked_add(additional) else {
                return Err(self.terminate_resource_error(
                    self.layout_work_overflow_details(),
                    phase,
                    used,
                    additional,
                ));
            };
            if let Err(error) = self.policy.check_layout_work_units(next) {
                return Err(self.terminate_resource_error(error, phase, used, additional));
            }
            match self.used.compare_exchange_weak(
                used,
                next,
                std::sync::atomic::Ordering::Relaxed,
                std::sync::atomic::Ordering::Relaxed,
            ) {
                Ok(_) => return Ok(()),
                Err(actual) => {
                    self.resource_checkpoint(phase)?;
                    used = actual;
                }
            }
        }
    }

    fn layout_work_overflow_details(&self) -> ResourceLimitExceeded {
        accumulation_overflow(
            self.policy,
            ResourceLimitPhase::LayoutModel,
            RenderResourceLimitId::MaxLayoutWorkUnits,
        )
    }

    pub(crate) fn arithmetic_overflow(&self) -> OperationWorkError {
        self.terminate_resource_error(
            self.layout_work_overflow_details(),
            OperationPhase::Layout,
            self.used(),
            1,
        )
    }

    /// Reserves the projected serialized bytes contributed by external icon expansion.
    ///
    /// The final whole-document SVG check remains authoritative. This earlier cumulative charge
    /// prevents repeated maximum-size icons from allocating their complete expanded strings before
    /// the operation-level output policy can reject them.
    pub(crate) fn charge_svg_bytes(&self, additional: usize) -> Result<(), OperationWorkError> {
        let phase = OperationPhase::Emit;
        self.resource_checkpoint(phase)?;
        let mut used = self
            .projected_svg_bytes
            .load(std::sync::atomic::Ordering::Relaxed);
        loop {
            let Some(next) = used.checked_add(additional) else {
                return Err(self.terminate_resource_error(
                    accumulation_overflow(
                        self.policy,
                        ResourceLimitPhase::SvgOutput,
                        RenderResourceLimitId::MaxSvgBytes,
                    ),
                    phase,
                    used,
                    additional,
                ));
            };
            if let Err(error) = self
                .policy
                .check_svg_byte_count(next, ResourceLimitPhase::SvgOutput)
            {
                return Err(self.terminate_resource_error(error, phase, used, additional));
            }
            match self.projected_svg_bytes.compare_exchange_weak(
                used,
                next,
                std::sync::atomic::Ordering::Relaxed,
                std::sync::atomic::Ordering::Relaxed,
            ) {
                Ok(_) => return Ok(()),
                Err(actual) => {
                    self.resource_checkpoint(phase)?;
                    used = actual;
                }
            }
        }
    }

    /// Checks an absolute SVG byte projection without consuming the cumulative icon reservation.
    ///
    /// Whole-document postprocessors use this after a streaming size pass. A rejection is still an
    /// operation terminal, but a successful preflight must not double-count icon bytes already
    /// included in the projected document.
    pub(crate) fn preflight_svg_byte_count(
        &self,
        actual: usize,
        resource_phase: ResourceLimitPhase,
        operation_phase: OperationPhase,
    ) -> Result<(), OperationWorkError> {
        self.resource_checkpoint(operation_phase)?;
        self.policy
            .check_svg_byte_count(actual, resource_phase)
            .map_err(|error| self.terminate_resource_error(error, operation_phase, 0, actual))
    }

    pub(crate) fn preflight_svg_structure(
        &self,
        elements: usize,
        tree_depth: usize,
        operation_phase: OperationPhase,
    ) -> Result<(), OperationWorkError> {
        self.resource_checkpoint(operation_phase)?;
        self.policy
            .check_svg_structure(elements, tree_depth)
            .map_err(|error| self.terminate_absolute_resource_error(error, operation_phase))
    }

    pub(crate) fn preflight_parsed_render(
        &self,
        parsed: &ParsedDiagramRender,
        operation_phase: OperationPhase,
    ) -> Result<(), OperationWorkError> {
        self.resource_checkpoint(operation_phase)?;
        let result = self.policy.check_parsed_render(parsed);
        self.resource_checkpoint(operation_phase)?;
        result.map_err(|error| self.terminate_absolute_resource_error(error, operation_phase))
    }

    #[cfg(feature = "diagram-class")]
    pub(crate) fn preflight_class_complexity(
        &self,
        model: &ClassDiagram,
        operation_phase: OperationPhase,
    ) -> Result<ClassComplexity, OperationWorkError> {
        self.resource_checkpoint(operation_phase)?;
        let result = self.policy.check_class_complexity(model);
        self.resource_checkpoint(operation_phase)?;
        result.map_err(|error| self.terminate_absolute_resource_error(error, operation_phase))
    }

    #[cfg(feature = "diagram-sequence")]
    pub(crate) fn preflight_sequence_complexity(
        &self,
        model: &merman_core::diagrams::sequence::SequenceDiagramRenderModel,
        operation_phase: OperationPhase,
    ) -> Result<SequenceComplexity, OperationWorkError> {
        self.resource_checkpoint(operation_phase)?;
        let result = self.policy.check_sequence_complexity(model);
        self.resource_checkpoint(operation_phase)?;
        result.map_err(|error| self.terminate_absolute_resource_error(error, operation_phase))
    }

    pub(crate) fn terminate_absolute_resource_error(
        &self,
        error: ResourceLimitExceeded,
        operation_phase: OperationPhase,
    ) -> OperationWorkError {
        let actual = error.actual;
        self.terminate_resource_error(error, operation_phase, 0, actual)
    }

    pub(crate) fn terminate_svg_byte_count_overflow(
        &self,
        resource_phase: ResourceLimitPhase,
        operation_phase: OperationPhase,
    ) -> OperationWorkError {
        self.terminate_resource_error(
            accumulation_overflow(
                self.policy,
                resource_phase,
                RenderResourceLimitId::MaxSvgBytes,
            ),
            operation_phase,
            0,
            usize::MAX,
        )
    }

    fn terminate_resource_error(
        &self,
        error: ResourceLimitExceeded,
        phase: OperationPhase,
        consumed: usize,
        requested: usize,
    ) -> OperationWorkError {
        let provenance = self.operation_resource_provenance();
        let terminal = match error.cause {
            ResourceLimitCause::Ceiling => {
                self.control
                    .terminate_resource_limit(OperationResourceLimitExceeded {
                        id: error.limit,
                        phase,
                        resource_phase: error.phase.as_str(),
                        limit: saturating_u64(error.max),
                        consumed: saturating_u64(consumed),
                        requested: saturating_u64(requested),
                        provenance,
                    })
            }
            ResourceLimitCause::ArithmeticOverflow => self.control.terminate_resource_overflow(
                error.limit,
                phase,
                error.phase.as_str(),
                saturating_u64(error.actual),
                saturating_u64(error.max),
                provenance,
            ),
        };
        self.map_terminal_error(terminal)
    }

    fn operation_resource_provenance(&self) -> OperationResourceProvenance {
        OperationResourceProvenance::new(
            OperationResourceDomain::Render,
            Some(self.policy.profile()),
            self.policy
                .explicit_overrides()
                .map(|(id, value)| OperationResourceOverride {
                    id: id.as_str(),
                    value: saturating_u64(value),
                }),
        )
    }

    fn map_terminal_error(&self, error: OperationLedgerError) -> OperationWorkError {
        match error {
            OperationLedgerError::Cancelled(error) => OperationWorkError::Cancelled(error),
            OperationLedgerError::ResourceLimitExceeded(error) => self
                .project_resource_limit(&error)
                .map(OperationWorkError::ResourceLimitExceeded)
                .unwrap_or_else(|| {
                    OperationWorkError::ForeignResourceTerminal(
                        OperationLedgerError::ResourceLimitExceeded(error),
                    )
                }),
            OperationLedgerError::ArithmeticOverflow { .. } => self
                .project_resource_overflow(&error)
                .map(OperationWorkError::ResourceLimitExceeded)
                .unwrap_or(OperationWorkError::ForeignResourceTerminal(error)),
        }
    }

    fn project_resource_limit(
        &self,
        error: &OperationResourceLimitExceeded,
    ) -> Option<ResourceLimitExceeded> {
        let (profile, explicit_overrides) = project_render_provenance(&error.provenance)?;
        Some(ResourceLimitExceeded {
            cause: ResourceLimitCause::Ceiling,
            phase: ResourceLimitPhase::from_stable_id(error.resource_phase)?,
            limit: error.id,
            actual: saturating_usize(error.consumed.saturating_add(error.requested)),
            max: saturating_usize(error.limit),
            profile,
            explicit_overrides,
        })
    }

    fn project_resource_overflow(
        &self,
        error: &OperationLedgerError,
    ) -> Option<ResourceLimitExceeded> {
        let OperationLedgerError::ArithmeticOverflow {
            id,
            resource_phase,
            actual,
            maximum,
            provenance,
            ..
        } = error
        else {
            return None;
        };
        let (profile, explicit_overrides) = project_render_provenance(provenance)?;
        Some(ResourceLimitExceeded {
            cause: ResourceLimitCause::ArithmeticOverflow,
            phase: ResourceLimitPhase::from_stable_id(resource_phase)?,
            limit: id,
            actual: saturating_usize(*actual),
            max: saturating_usize(*maximum),
            profile,
            explicit_overrides,
        })
    }

    /// Returns the unreserved SVG budget for bounded policies after all successful charges.
    #[cfg(test)]
    pub(crate) fn remaining_svg_bytes(&self) -> Option<usize> {
        let maximum = self.policy.value(ResourceLimitId::MaxSvgBytes)?;
        let used = self
            .projected_svg_bytes
            .load(std::sync::atomic::Ordering::Relaxed);
        Some(maximum.saturating_sub(used))
    }

    /// Atomically reserves up to `requested` additional SVG bytes.
    ///
    /// Bounded policies return a deterministic limit error alongside a partial reservation when
    /// the full amount does not fit. The caller may still succeed if its bounded producer emits no
    /// more than the reserved bytes, then reconcile the conservative reservation to actual output.
    pub(crate) fn reserve_svg_bytes_up_to(
        &self,
        requested: usize,
    ) -> Result<SvgByteReservation, OperationWorkError> {
        let phase = OperationPhase::Emit;
        self.resource_checkpoint(phase)?;
        let Some(maximum) = self.policy.value(ResourceLimitId::MaxSvgBytes) else {
            self.charge_svg_bytes(requested)?;
            return Ok(SvgByteReservation {
                additional_bytes: requested,
                limit_error: None,
            });
        };

        let mut used = self
            .projected_svg_bytes
            .load(std::sync::atomic::Ordering::Relaxed);
        loop {
            let available = maximum.saturating_sub(used);
            let additional_bytes = requested.min(available);
            let Some(next) = used.checked_add(additional_bytes) else {
                return Err(self.terminate_resource_error(
                    accumulation_overflow(
                        self.policy,
                        ResourceLimitPhase::SvgOutput,
                        RenderResourceLimitId::MaxSvgBytes,
                    ),
                    phase,
                    used,
                    additional_bytes,
                ));
            };
            match self.projected_svg_bytes.compare_exchange_weak(
                used,
                next,
                std::sync::atomic::Ordering::Relaxed,
                std::sync::atomic::Ordering::Relaxed,
            ) {
                Ok(_) => {
                    let limit_error = (additional_bytes < requested).then(|| {
                        svg_reservation_limit_error(self.policy, maximum, used, requested)
                    });
                    return Ok(SvgByteReservation {
                        additional_bytes,
                        limit_error,
                    });
                }
                Err(actual) => {
                    self.resource_checkpoint(phase)?;
                    used = actual;
                }
            }
        }
    }

    /// Reconciles an earlier conservative SVG reservation with the bytes actually retained.
    pub(crate) fn reconcile_svg_bytes(
        &self,
        reserved: usize,
        actual: usize,
    ) -> Result<(), OperationWorkError> {
        self.resource_checkpoint(OperationPhase::Emit)?;
        if actual > reserved {
            return self.charge_svg_bytes(actual - reserved);
        }

        let released = reserved - actual;
        if released != 0 {
            let previous = self
                .projected_svg_bytes
                .fetch_sub(released, std::sync::atomic::Ordering::Relaxed);
            debug_assert!(previous >= released);
        }
        Ok(())
    }

    pub(crate) fn used(&self) -> usize {
        self.used.load(std::sync::atomic::Ordering::Relaxed)
    }

    #[cfg(test)]
    pub(crate) fn projected_svg_bytes(&self) -> usize {
        self.projected_svg_bytes
            .load(std::sync::atomic::Ordering::Relaxed)
    }
}

fn project_render_provenance(
    provenance: &OperationResourceProvenance,
) -> Option<(RenderResourceProfile, Vec<ResourceLimitOverride>)> {
    if provenance.domain != OperationResourceDomain::Render {
        return None;
    }
    let explicit_overrides = provenance
        .explicit_overrides
        .iter()
        .map(|override_| {
            Some(ResourceLimitOverride {
                id: ResourceLimitId::from_stable_id(override_.id)?,
                value: saturating_usize(override_.value),
            })
        })
        .collect::<Option<Vec<_>>>()?;
    Some((provenance.profile?, explicit_overrides))
}

fn saturating_usize(value: u64) -> usize {
    usize::try_from(value).unwrap_or(usize::MAX)
}

fn saturating_u64(value: usize) -> u64 {
    u64::try_from(value).unwrap_or(u64::MAX)
}

fn accumulation_overflow(
    policy: RenderResourcePolicy,
    phase: ResourceLimitPhase,
    id: RenderResourceLimitId,
) -> ResourceLimitExceeded {
    let limit = ResourceLimitId::Render(id);
    ResourceLimitExceeded {
        cause: ResourceLimitCause::ArithmeticOverflow,
        phase,
        limit: limit.as_str(),
        actual: usize::MAX,
        max: policy.value(limit).unwrap_or(usize::MAX),
        profile: policy.profile(),
        explicit_overrides: policy
            .explicit_overrides()
            .map(|(id, value)| ResourceLimitOverride { id, value })
            .collect(),
    }
}

#[cfg_attr(
    not(feature = "all-diagrams"),
    allow(
        dead_code,
        reason = "Shared operation support has different consumers in each diagram selection."
    )
)]
fn svg_byte_limit_error(policy: RenderResourcePolicy, maximum: usize) -> ResourceLimitExceeded {
    ResourceLimitExceeded {
        cause: ResourceLimitCause::Ceiling,
        phase: ResourceLimitPhase::SvgOutput,
        limit: ResourceLimitId::MaxSvgBytes.as_str(),
        actual: maximum.saturating_add(1),
        max: maximum,
        profile: policy.profile(),
        explicit_overrides: policy
            .explicit_overrides()
            .map(|(id, value)| ResourceLimitOverride { id, value })
            .collect(),
    }
}

#[cfg_attr(
    not(feature = "all-diagrams"),
    allow(
        dead_code,
        reason = "Shared operation support has different consumers in each diagram selection."
    )
)]
fn svg_reservation_limit_error(
    policy: RenderResourcePolicy,
    maximum: usize,
    used: usize,
    requested: usize,
) -> ResourceLimitExceeded {
    if used.checked_add(requested).is_none() {
        return accumulation_overflow(
            policy,
            ResourceLimitPhase::SvgOutput,
            RenderResourceLimitId::MaxSvgBytes,
        );
    }
    svg_byte_limit_error(policy, maximum)
}

#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub struct ResourceLimitExceeded {
    pub cause: ResourceLimitCause,
    pub phase: ResourceLimitPhase,
    pub limit: &'static str,
    pub actual: usize,
    pub max: usize,
    pub profile: RenderResourceProfile,
    pub explicit_overrides: Vec<ResourceLimitOverride>,
}

impl std::fmt::Display for ResourceLimitExceeded {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "resource limit exceeded during {}: {}",
            self.phase, self.limit
        )?;
        if self.cause == ResourceLimitCause::ArithmeticOverflow {
            write!(f, " cause={}", self.cause)?;
        }
        write!(f, " actual={} max={}", self.actual, self.max)
    }
}

impl std::error::Error for ResourceLimitExceeded {}

impl ResourceLimitExceeded {
    fn from_input(policy: &RenderResourcePolicy, error: InputResourceLimitExceeded) -> Self {
        Self {
            cause: ResourceLimitCause::Ceiling,
            phase: match error.phase {
                InputResourceLimitPhase::Source => ResourceLimitPhase::Source,
                InputResourceLimitPhase::Model => ResourceLimitPhase::LayoutModel,
            },
            limit: error.limit,
            actual: error.actual,
            max: error.max,
            profile: error.profile,
            explicit_overrides: policy
                .explicit_overrides()
                .map(|(id, value)| ResourceLimitOverride { id, value })
                .collect(),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ResourceLimitOverride {
    pub id: ResourceLimitId,
    pub value: usize,
}

#[cfg(all(test, feature = "all-diagrams"))]
mod tests {
    use super::*;
    use merman_core::diagrams::flowchart::{
        FlowEdge, FlowEdgeMarker, FlowEdgeStroke, FlowEdgeVisibility, FlowNode, FlowSubgraph,
    };
    use merman_core::{Engine, ParseOptions, RenderSemanticModel};
    use std::collections::HashSet;

    #[test]
    fn layout_work_defaults_preserve_bounded_general_and_native_profiles() {
        for (profile, expected) in [
            (RenderResourceProfile::Constrained, Some(125_000)),
            (RenderResourceProfile::Interactive, Some(14_100_000)),
            (RenderResourceProfile::TrustedNative, Some(15_000_000)),
            (RenderResourceProfile::UnboundedForTrustedInput, None),
        ] {
            let policy = RenderResourcePolicy::for_profile(profile);
            assert_eq!(policy.value(ResourceLimitId::MaxLayoutWorkUnits), expected);
            assert!(policy.explicit_overrides().next().is_none());
        }
        assert_eq!(
            RenderResourcePolicy::default(),
            RenderResourcePolicy::interactive()
        );
    }

    #[test]
    fn resource_contract_is_complete_unique_and_drives_every_profile() {
        assert_eq!(
            RESOURCE_PROFILE_DESCRIPTORS.len(),
            RenderResourceProfile::ALL.len()
        );
        assert_eq!(RESOURCE_LIMIT_DESCRIPTORS.len(), RESOURCE_LIMIT_COUNT);

        let profile_ids = RESOURCE_PROFILE_DESCRIPTORS
            .iter()
            .map(|descriptor| descriptor.id)
            .collect::<HashSet<_>>();
        assert_eq!(profile_ids.len(), RESOURCE_PROFILE_DESCRIPTORS.len());
        assert_eq!(
            RESOURCE_PROFILE_DESCRIPTORS
                .iter()
                .filter(|descriptor| descriptor.recommended_binding_default)
                .map(|descriptor| descriptor.profile)
                .collect::<Vec<_>>(),
            vec![GENERAL_BINDING_DEFAULT_RESOURCE_PROFILE]
        );
        for profile in RenderResourceProfile::ALL {
            let descriptor = profile.descriptor();
            assert_eq!(RenderResourceProfile::from_id(descriptor.id), Some(profile));
            let policy = RenderResourcePolicy::for_profile(profile);
            for limit in RESOURCE_LIMIT_DESCRIPTORS {
                assert_eq!(policy.profile(), profile);
                if limit.hard_cap {
                    assert!(!limit.overridable);
                    assert!(policy.value(limit.id).is_some());
                }
            }
        }

        let limit_ids = RESOURCE_LIMIT_DESCRIPTORS
            .iter()
            .map(|descriptor| descriptor.stable_id)
            .collect::<HashSet<_>>();
        assert_eq!(limit_ids.len(), RESOURCE_LIMIT_DESCRIPTORS.len());
        for descriptor in RESOURCE_LIMIT_DESCRIPTORS {
            assert_eq!(
                ResourceLimitId::from_stable_id(descriptor.stable_id),
                Some(descriptor.id)
            );
            assert_eq!(descriptor.id.descriptor(), descriptor);
        }
    }

    #[test]
    fn resource_overrides_fail_closed_for_unknown_and_internal_ids() {
        let mut limits = RenderResourcePolicy::interactive();
        assert!(matches!(
            limits.apply_override("future_limit", 1),
            Err(ResourceLimitOverrideError::UnknownLimit(_))
        ));
        for hard_cap in [
            SVG_BACKEND_TREE_NODES_HARD_CAP_ID,
            SVG_BACKEND_TREE_DEPTH_HARD_CAP_ID,
        ] {
            assert_eq!(
                limits.apply_override(hard_cap, 1),
                Err(ResourceLimitOverrideError::HardCap(hard_cap))
            );
        }
        assert_eq!(
            limits.apply_override("max_svg_elements", 0),
            Err(ResourceLimitOverrideError::NonPositive("max_svg_elements"))
        );
        limits.apply_override("max_svg_elements", 7).unwrap();
        assert_eq!(limits.value(ResourceLimitId::MaxSvgElements), Some(7));
    }

    #[test]
    fn resolved_svg_backend_hard_caps_remain_active_for_unbounded_policy() {
        let policy = RenderResourcePolicy::unbounded_for_trusted_input();
        for (elements, tree_depth, limit, actual, max) in [
            (
                MAX_RESVG_TREE_NODES + 1,
                0,
                SVG_BACKEND_TREE_NODES_HARD_CAP_ID,
                MAX_RESVG_TREE_NODES + 1,
                MAX_RESVG_TREE_NODES,
            ),
            (
                0,
                MAX_RESVG_TREE_DEPTH + 1,
                SVG_BACKEND_TREE_DEPTH_HARD_CAP_ID,
                MAX_RESVG_TREE_DEPTH + 1,
                MAX_RESVG_TREE_DEPTH,
            ),
        ] {
            let error = policy
                .check_svg_structure(elements, tree_depth)
                .unwrap_err();

            assert_eq!(error.phase, ResourceLimitPhase::SvgPostprocess);
            assert_eq!(error.limit, limit);
            assert_eq!(error.actual, actual);
            assert_eq!(error.max, max);
        }
    }

    #[test]
    fn source_limit_reports_structured_error() {
        let err = RenderResourcePolicy::unbounded_for_trusted_input()
            .with_limit(ResourceLimitId::MaxSourceBytes, 4)
            .unwrap()
            .with_limit(ResourceLimitId::MaxSvgBytes, 123)
            .unwrap()
            .check_source_bytes("12345")
            .unwrap_err();

        assert_eq!(err.phase, ResourceLimitPhase::Source);
        assert_eq!(err.limit, "max_source_bytes");
        assert_eq!(err.actual, 5);
        assert_eq!(err.max, 4);
        assert_eq!(
            err.explicit_overrides,
            vec![
                ResourceLimitOverride {
                    id: ResourceLimitId::MaxSourceBytes,
                    value: 4,
                },
                ResourceLimitOverride {
                    id: ResourceLimitId::MaxSvgBytes,
                    value: 123,
                },
            ]
        );
    }

    #[test]
    fn derived_layout_work_limits_report_the_owned_phase_and_metric() {
        let limits = RenderResourcePolicy::unbounded_for_trusted_input()
            .with_limit(ResourceLimitId::MaxLayoutWorkUnits, 7)
            .unwrap();

        let error = limits.check_layout_work_units(8).unwrap_err();
        assert_eq!(error.phase, ResourceLimitPhase::LayoutModel);
        assert_eq!(error.limit, "max_layout_work_units");
        assert_eq!(error.cause, ResourceLimitCause::Ceiling);
        assert_eq!(error.actual, 8);
        assert_eq!(error.max, 7);
    }

    #[test]
    fn operation_work_meter_preflight_does_not_consume_budget() {
        let policy = RenderResourcePolicy::unbounded_for_trusted_input()
            .with_limit(ResourceLimitId::MaxLayoutWorkUnits, 7)
            .unwrap();
        let meter = OperationWorkMeter::new(policy);

        meter.preflight(7).unwrap();
        meter.preflight(7).unwrap();
        assert_eq!(meter.used(), 0);
        meter.charge(7).unwrap();
        assert_eq!(meter.used(), 7);
    }

    #[test]
    fn operation_work_meter_checks_shared_control_before_charging() {
        let control = OperationControl::new();
        control.cancel();
        let meter = OperationWorkMeter::new_with_control(
            RenderResourcePolicy::unbounded_for_trusted_input()
                .with_limit(ResourceLimitId::MaxLayoutWorkUnits, 1)
                .unwrap(),
            control,
        );

        let first = meter.charge(2).unwrap_err();
        let OperationWorkError::Cancelled(error) = &first else {
            panic!("expected cancellation to win before resource accounting");
        };
        assert_eq!(error.phase, OperationPhase::Layout);
        assert_eq!(meter.used(), 0);
        assert_eq!(meter.charge(2).unwrap_err(), first);
    }

    #[test]
    fn operation_work_meter_checks_deadline_before_charging() {
        let control = OperationControl::new().with_deadline(std::time::Duration::ZERO);
        let meter = OperationWorkMeter::new_with_control(
            RenderResourcePolicy::unbounded_for_trusted_input(),
            control,
        );

        let OperationWorkError::Cancelled(error) = meter.charge(1).unwrap_err() else {
            panic!("expected deadline cancellation before resource accounting");
        };
        assert_eq!(error.phase, OperationPhase::Layout);
        assert_eq!(error.reason, merman_core::CancelReason::DeadlineExceeded);
        assert_eq!(meter.used(), 0);
    }

    #[test]
    fn model_preflight_observes_cancellation_before_latching_its_limit() {
        let parsed = Engine::new()
            .parse_diagram_for_render_model_sync("flowchart TD\nA --> B\n", ParseOptions::strict())
            .expect("the controlled fixture should parse")
            .expect("the controlled fixture should produce a render model");
        let policy = RenderResourcePolicy::unbounded_for_trusted_input()
            .with_limit(ResourceLimitId::MaxModelItems, 1)
            .expect("the model limit should be valid");
        let control = OperationControl::new();
        control.cancel_after_checkpoints(1);
        let meter = OperationWorkMeter::new_with_control(policy, control);

        let first = meter
            .preflight_parsed_render(&parsed, OperationPhase::Layout)
            .expect_err("cancellation after the scan must beat its pending resource rejection");
        assert!(matches!(
            first,
            OperationWorkError::Cancelled(error)
                if error.phase == OperationPhase::Layout
                    && error.reason == merman_core::CancelReason::Requested
        ));
        assert_eq!(
            meter
                .preflight_parsed_render(&parsed, OperationPhase::Emit)
                .expect_err("the cancellation must remain the sticky terminal"),
            first
        );
    }

    #[test]
    fn operation_svg_meter_checks_shared_control_before_reserving_bytes() {
        let control = OperationControl::new();
        control.cancel();
        let meter = OperationWorkMeter::new_with_control(
            RenderResourcePolicy::unbounded_for_trusted_input(),
            control,
        );

        let OperationWorkError::Cancelled(error) = meter.charge_svg_bytes(1).unwrap_err() else {
            panic!("expected cancellation to win before SVG accounting");
        };
        assert_eq!(error.phase, OperationPhase::Emit);
        assert_eq!(meter.projected_svg_bytes(), 0);
    }

    #[test]
    fn operation_work_meter_replays_first_resource_terminal_after_cancellation() {
        let policy = RenderResourcePolicy::unbounded_for_trusted_input()
            .with_limit(ResourceLimitId::MaxLayoutWorkUnits, 10)
            .unwrap();
        let control = OperationControl::new();
        let meter = OperationWorkMeter::new_with_control(policy, control.clone());
        meter.charge(8).unwrap();

        let first = meter.charge(3).unwrap_err();
        let OperationWorkError::ResourceLimitExceeded(error) = &first else {
            panic!("expected a resource rejection");
        };
        assert_eq!(error.cause, ResourceLimitCause::Ceiling);
        assert_eq!(error.actual, 11);
        assert_eq!(error.max, 10);
        assert_eq!(meter.used(), 8);

        control.cancel();
        assert_eq!(meter.charge(2).unwrap_err(), first);
        assert_eq!(meter.used(), 8);
    }

    #[test]
    fn operation_work_meter_replays_the_originating_policy_provenance() {
        let originating_policy = RenderResourcePolicy::constrained()
            .with_limit(ResourceLimitId::MaxLayoutWorkUnits, 1)
            .unwrap()
            .with_limit(ResourceLimitId::MaxSvgBytes, 17)
            .unwrap();
        let observing_policy = RenderResourcePolicy::interactive()
            .with_limit(ResourceLimitId::MaxLayoutWorkUnits, 99)
            .unwrap();
        let control = OperationControl::new();
        let originating = OperationWorkMeter::new_with_control(originating_policy, control.clone());
        let observing = OperationWorkMeter::new_with_control(observing_policy, control);

        let OperationWorkError::ResourceLimitExceeded(first) = originating.charge(2).unwrap_err()
        else {
            panic!("expected the originating ceiling");
        };
        let OperationWorkError::ResourceLimitExceeded(replayed) =
            observing.checkpoint(OperationPhase::Emit).unwrap_err()
        else {
            panic!("expected the stored render terminal to retain its typed projection");
        };

        assert_eq!(replayed, first);
        assert_eq!(replayed.profile, RenderResourceProfile::Constrained);
        assert_eq!(
            replayed.explicit_overrides,
            vec![
                ResourceLimitOverride {
                    id: ResourceLimitId::MaxLayoutWorkUnits,
                    value: 1,
                },
                ResourceLimitOverride {
                    id: ResourceLimitId::MaxSvgBytes,
                    value: 17,
                },
            ]
        );
    }

    #[test]
    fn operation_work_meter_preserves_a_foreign_ascii_domain() {
        let control = OperationControl::new();
        let terminal = control.terminate_resource_limit(OperationResourceLimitExceeded {
            id: "max_ascii_output_bytes",
            phase: OperationPhase::Emit,
            resource_phase: "ascii_output",
            limit: 7,
            consumed: 7,
            requested: 1,
            provenance: OperationResourceProvenance::new(
                OperationResourceDomain::Ascii,
                Some(RenderResourceProfile::Constrained),
                [OperationResourceOverride {
                    id: "max_ascii_output_bytes",
                    value: 7,
                }],
            ),
        });
        let observing =
            OperationWorkMeter::new_with_control(RenderResourcePolicy::interactive(), control);

        assert_eq!(
            observing
                .checkpoint(OperationPhase::Layout)
                .expect_err("render must not project an ASCII resource terminal"),
            OperationWorkError::ForeignResourceTerminal(terminal)
        );
    }

    #[test]
    fn operation_work_meter_overflow_fails_under_unlimited_policy() {
        let meter = OperationWorkMeter::new(RenderResourcePolicy::unbounded_for_trusted_input());
        meter.charge(usize::MAX).unwrap();

        let preflight = meter.preflight(1).unwrap_err();
        let OperationWorkError::ResourceLimitExceeded(preflight_error) = &preflight else {
            panic!("expected a resource rejection");
        };
        assert_eq!(
            preflight_error.cause,
            ResourceLimitCause::ArithmeticOverflow
        );
        assert_eq!(preflight_error.limit, "max_layout_work_units");
        assert_eq!(preflight_error.max, usize::MAX);
        assert_eq!(meter.used(), usize::MAX);

        assert_eq!(meter.charge(1).unwrap_err(), preflight);
        assert_eq!(meter.used(), usize::MAX);
    }

    #[test]
    fn operation_svg_meter_accepts_exact_limit_and_rejects_one_more() {
        let policy = RenderResourcePolicy::unbounded_for_trusted_input()
            .with_limit(ResourceLimitId::MaxSvgBytes, 10)
            .unwrap();
        let control = OperationControl::new();
        let meter = OperationWorkMeter::new_with_control(policy, control.clone());

        meter.charge_svg_bytes(10).unwrap();
        assert_eq!(meter.projected_svg_bytes(), 10);
        assert_eq!(meter.remaining_svg_bytes(), Some(0));

        let first = meter.charge_svg_bytes(1).unwrap_err();
        let OperationWorkError::ResourceLimitExceeded(error) = &first else {
            panic!("expected a resource rejection");
        };
        assert_eq!(error.actual, 11);
        assert_eq!(error.max, 10);
        assert_eq!(meter.projected_svg_bytes(), 10);

        control.cancel();
        assert_eq!(meter.charge_svg_bytes(1).unwrap_err(), first);
        assert_eq!(meter.projected_svg_bytes(), 10);
    }

    #[test]
    fn operation_svg_meter_reconciles_conservative_reservations() {
        let policy = RenderResourcePolicy::unbounded_for_trusted_input()
            .with_limit(ResourceLimitId::MaxSvgBytes, 12)
            .unwrap();
        let meter = OperationWorkMeter::new(policy);

        meter.charge_svg_bytes(8).unwrap();
        meter.reconcile_svg_bytes(8, 5).unwrap();
        assert_eq!(meter.projected_svg_bytes(), 5);
        assert_eq!(meter.remaining_svg_bytes(), Some(7));

        meter.reconcile_svg_bytes(5, 12).unwrap();
        assert_eq!(meter.projected_svg_bytes(), 12);
        assert_eq!(meter.remaining_svg_bytes(), Some(0));
    }

    #[test]
    fn operation_svg_meter_atomically_reserves_available_growth() {
        let policy = RenderResourcePolicy::unbounded_for_trusted_input()
            .with_limit(ResourceLimitId::MaxSvgBytes, 10)
            .unwrap();
        let meter = OperationWorkMeter::new(policy);

        meter.charge_svg_bytes(7).unwrap();
        let reservation = meter.reserve_svg_bytes_up_to(5).unwrap();
        assert_eq!(reservation.additional_bytes, 3);
        let error = reservation
            .limit_error
            .expect("a partial reservation carries the deterministic limit error");
        assert_eq!(error.actual, 11);
        assert_eq!(error.max, 10);
        assert_eq!(meter.projected_svg_bytes(), 10);

        meter.reconcile_svg_bytes(3, 1).unwrap();
        assert_eq!(meter.projected_svg_bytes(), 8);
        assert_eq!(meter.remaining_svg_bytes(), Some(2));

        meter.charge_svg_bytes(2).unwrap();
        assert_eq!(meter.projected_svg_bytes(), 10);
    }

    #[test]
    fn partial_svg_reservation_preserves_arithmetic_overflow_classification() {
        let policy = RenderResourcePolicy::unbounded_for_trusted_input()
            .with_limit(ResourceLimitId::MaxSvgBytes, usize::MAX)
            .unwrap();
        let control = OperationControl::new();
        let meter = OperationWorkMeter::new_with_control(policy, control.clone());

        meter.charge_svg_bytes(usize::MAX).unwrap();
        let reservation = meter.reserve_svg_bytes_up_to(1).unwrap();
        assert_eq!(reservation.additional_bytes, 0);
        let error = reservation
            .limit_error
            .expect("an overflowing partial reservation must retain its error");
        assert_eq!(error.cause, ResourceLimitCause::ArithmeticOverflow);
        assert_eq!(error.actual, usize::MAX);
        assert_eq!(error.max, usize::MAX);
        assert_eq!(meter.projected_svg_bytes(), usize::MAX);

        let first = meter.terminate_absolute_resource_error(error, OperationPhase::Emit);
        control.cancel();
        assert_eq!(
            meter
                .charge(1)
                .expect_err("the finalized overflow must remain the sticky terminal"),
            first
        );
    }

    #[test]
    fn zenuml_complexity_includes_inline_decorations() {
        let parsed = Engine::new()
            .parse_diagram_for_render_model_sync(
                "zenuml\nA->[rocket]B.call()\n",
                ParseOptions::strict(),
            )
            .unwrap()
            .unwrap();
        let RenderSemanticModel::Zenuml(model) = parsed.model() else {
            panic!("expected ZenUML model");
        };
        let complexity = ZenumlComplexity::from_model(model);

        assert_eq!(complexity.participants, 2);
        assert_eq!(complexity.statements, 1);
        let required = ["rocket", "call()"]
            .into_iter()
            .map(str::len)
            .sum::<usize>();
        assert!(complexity.label_bytes >= required);
    }

    #[test]
    fn zenuml_uses_the_shared_model_budget() {
        let parsed = Engine::new()
            .parse_diagram_for_render_model_sync(
                "zenuml\nA.call() {\n  if(ok) {\n    if(inner) {\n      B.work()\n    }\n  }\n}\n",
                ParseOptions::strict(),
            )
            .unwrap()
            .unwrap();
        let RenderSemanticModel::Zenuml(model) = parsed.model() else {
            panic!("expected ZenUML model");
        };

        let limits = RenderResourcePolicy::unbounded_for_trusted_input()
            .with_limit(ResourceLimitId::MaxModelItems, 1)
            .unwrap();
        let error = limits.check_zenuml_complexity(model).unwrap_err();
        assert_eq!(error.phase, ResourceLimitPhase::LayoutModel);
        assert_eq!(error.limit, "max_model_items");
    }

    #[test]
    fn flowchart_complexity_counts_layout_nodes_and_labels() {
        let model = FlowchartModel {
            keyword: "graph".to_string(),
            acc_descr: None,
            acc_title: None,
            class_defs: Default::default(),
            direction: None,
            edge_defaults: None,
            vertex_calls: Vec::new(),
            nodes: vec![FlowNode {
                id: "A".to_string(),
                provenance: Default::default(),
                label: Some("Alpha".to_string()),
                label_type: None,
                layout_shape: None,
                shape: None,
                icon: None,
                form: None,
                pos: None,
                img: None,
                constraint: None,
                asset_width: None,
                asset_height: None,
                classes: Vec::new(),
                styles: Vec::new(),
                link: None,
                link_target: None,
                have_callback: false,
            }],
            edges: vec![FlowEdge {
                id: "L-A-B".to_string(),
                from: "A".to_string(),
                to: "B".to_string(),
                label: Some("edge".to_string()),
                label_type: None,
                edge_type: None,
                arrow: "-->".to_string(),
                start_marker: FlowEdgeMarker::None,
                end_marker: FlowEdgeMarker::Point,
                is_user_defined_id: false,
                stroke: None,
                stroke_kind: FlowEdgeStroke::Normal,
                visibility: FlowEdgeVisibility::Visible,
                interpolate: None,
                classes: Vec::new(),
                style: Vec::new(),
                animate: None,
                animation: None,
                length: 1,
            }],
            subgraphs: vec![FlowSubgraph {
                metadata: None,
                id: "cluster".to_string(),
                title: "Cluster".to_string(),
                dir: None,
                has_explicit_dir: false,
                label_type: None,
                classes: Vec::new(),
                styles: Vec::new(),
                nodes: vec!["A".to_string()],
            }],
            tooltips: Default::default(),
            warning_facts: Vec::new(),
        };

        let complexity = FlowchartComplexity::from_model(&model);
        assert_eq!(complexity.nodes, 2);
        assert_eq!(complexity.edges, 1);
        assert_eq!(complexity.subgraphs, 1);
        assert!(complexity.label_bytes >= "AlphaedgeCluster".len());
    }
}

#[cfg(feature = "diagram-class")]
pub use merman_core::resources::ClassComplexity;

#[cfg(any(
    feature = "diagram-flowchart",
    feature = "diagram-swimlane",
    feature = "diagram-agentflow"
))]
pub use merman_core::resources::FlowchartComplexity;

#[cfg(feature = "diagram-mindmap")]
pub use merman_core::resources::MindmapComplexity;

#[cfg(feature = "diagram-sequence")]
pub use merman_core::resources::SequenceComplexity;

#[cfg(feature = "diagram-zenuml")]
pub use merman_core::resources::ZenumlComplexity;
