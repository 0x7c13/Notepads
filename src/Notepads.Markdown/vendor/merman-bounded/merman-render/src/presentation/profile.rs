use merman_core::{Engine, MermaidConfig};
use serde_json::Value;

use super::{HostTheme, PresentationError, compiler};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum PresentationProfile {
    MermanModern,
}

impl PresentationProfile {
    pub const ALL: [Self; 1] = [Self::MermanModern];

    pub const fn id(self) -> &'static str {
        match self {
            Self::MermanModern => "merman-modern",
        }
    }

    pub fn from_id(id: &str) -> Result<Self, PresentationError> {
        Self::ALL
            .into_iter()
            .find(|profile| profile.id() == id)
            .ok_or_else(|| PresentationError::UnknownPresentationProfile(id.to_string()))
    }

    #[cfg(any(
        feature = "diagram-flowchart",
        feature = "diagram-swimlane",
        feature = "diagram-agentflow"
    ))]
    const fn flowchart_policy(self) -> FlowchartPresentationPolicy {
        match self {
            Self::MermanModern => FlowchartPresentationPolicy {
                edge_corner_radius: None,
                edge_label_padding: 4.0,
                compact_edge_corners: true,
            },
        }
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Presentation {
    profile: Option<PresentationProfile>,
    theme: Option<HostTheme>,
}

impl Presentation {
    pub fn new() -> Self {
        Self::default()
    }

    pub const fn profile(&self) -> Option<PresentationProfile> {
        self.profile
    }

    pub fn theme(&self) -> Option<&HostTheme> {
        self.theme.as_ref()
    }

    pub fn with_profile(mut self, profile: PresentationProfile) -> Self {
        self.profile = Some(profile);
        self
    }

    pub fn with_theme(mut self, theme: HostTheme) -> Self {
        self.theme = Some(theme);
        self
    }

    pub fn resolve(self) -> ResolvedPresentation {
        let mut mermaid_config = self.profile.map(profile_defaults).unwrap_or_default();
        if let Some(theme) = &self.theme {
            let theme = theme.mermaid_config_patch();
            mermaid_config.deep_merge(theme.as_value());
        }
        ResolvedPresentation {
            presentation: self,
            mermaid_config,
        }
    }
}

#[derive(Debug, Clone)]
pub struct ResolvedPresentation {
    presentation: Presentation,
    mermaid_config: MermaidConfig,
}

impl ResolvedPresentation {
    pub fn presentation(&self) -> &Presentation {
        &self.presentation
    }

    pub fn materialize_engine(&self, engine: Engine) -> Engine {
        engine.with_presentation_defaults_internal(
            self.mermaid_config.clone(),
            compiler::non_paint_defaults(&self.mermaid_config),
            self.presentation
                .theme
                .as_ref()
                .is_some_and(HostTheme::has_paint_values),
        )
    }

    /// Returns the small renderer-owned policy that accompanies the materialized Mermaid config.
    pub const fn render_policy(&self) -> PresentationRenderPolicy {
        PresentationRenderPolicy {
            profile: self.presentation.profile,
        }
    }
}

/// Opaque renderer policy derived from a resolved product presentation.
///
/// The policy intentionally excludes Mermaid configuration and presentation theme data. Callers should
/// first materialize the resolved presentation into the parsing engine, then carry this compact
/// value alongside the resulting typed render model.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct PresentationRenderPolicy {
    profile: Option<PresentationProfile>,
}

impl PresentationRenderPolicy {
    pub const fn profile(self) -> Option<PresentationProfile> {
        self.profile
    }

    #[cfg(any(
        feature = "diagram-flowchart",
        feature = "diagram-swimlane",
        feature = "diagram-agentflow"
    ))]
    pub(crate) const fn flowchart(self) -> Option<FlowchartPresentationPolicy> {
        match self.profile {
            Some(profile) => Some(profile.flowchart_policy()),
            None => None,
        }
    }

    pub(crate) fn resolve_aspects(
        self,
        flowchart_svg_applicable: bool,
        uses_elk: bool,
        elk_available: bool,
    ) -> Vec<PresentationAspectResolution> {
        match self.profile {
            None => Vec::new(),
            Some(PresentationProfile::MermanModern) => {
                let [global_defaults, flowchart_svg, flowchart_elk_default] = MERMAN_MODERN_ASPECTS;
                vec![
                    PresentationAspectResolution::new(
                        global_defaults,
                        PresentationAspectState::Active,
                    ),
                    PresentationAspectResolution::new(
                        flowchart_svg,
                        if flowchart_svg_applicable {
                            PresentationAspectState::Active
                        } else {
                            PresentationAspectState::Inactive
                        },
                    ),
                    PresentationAspectResolution::new(
                        flowchart_elk_default,
                        if !flowchart_svg_applicable || !uses_elk {
                            PresentationAspectState::Inactive
                        } else if elk_available {
                            PresentationAspectState::Active
                        } else {
                            PresentationAspectState::Blocked
                        },
                    ),
                ]
            }
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum PresentationAspectState {
    Active,
    Inactive,
    Blocked,
}

impl PresentationAspectState {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Active => "active",
            Self::Inactive => "inactive",
            Self::Blocked => "blocked",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PresentationAspectResolution {
    descriptor: PresentationAspectDescriptor,
    state: PresentationAspectState,
}

impl PresentationAspectResolution {
    const fn new(descriptor: PresentationAspectDescriptor, state: PresentationAspectState) -> Self {
        Self { descriptor, state }
    }

    pub const fn id(self) -> &'static str {
        self.descriptor.id()
    }

    pub const fn state(self) -> PresentationAspectState {
        self.state
    }

    pub const fn required_capability_id(self) -> Option<&'static str> {
        self.descriptor.required_capability_id()
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq)]
#[cfg(any(
    feature = "diagram-flowchart",
    feature = "diagram-swimlane",
    feature = "diagram-agentflow"
))]
pub(crate) struct FlowchartPresentationPolicy {
    pub(crate) edge_corner_radius: Option<f64>,
    pub(crate) edge_label_padding: f64,
    pub(crate) compact_edge_corners: bool,
}

fn profile_defaults(profile: PresentationProfile) -> MermaidConfig {
    match profile {
        PresentationProfile::MermanModern => {
            let theme_variables = [
                ("mainBkg", "#F8FAFC"),
                ("nodeBorder", "#64748B"),
                ("nodeTextColor", "#1E293B"),
                ("primaryColor", "#F8FAFC"),
                ("primaryBorderColor", "#64748B"),
                ("primaryTextColor", "#1E293B"),
                ("lineColor", "#64748B"),
                ("arrowheadColor", "#64748B"),
                ("edgeLabelBackground", "#FFFFFF"),
                ("clusterBkg", "#F1F5F9"),
                ("clusterBorder", "#CBD5E1"),
            ]
            .into_iter()
            .map(|(key, value)| (key.to_string(), Value::String(value.to_string())))
            .collect();
            MermaidConfig::from_value(serde_json::json!({
                "theme": "redux",
                "look": "neo",
                "flowchart": { "layout": "elk" },
                "themeVariables": Value::Object(theme_variables),
            }))
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum PresentationAspectApplicability {
    AllDiagrams,
    Family(&'static str),
}

impl PresentationAspectApplicability {
    pub const fn kind_id(self) -> &'static str {
        match self {
            Self::AllDiagrams => "all-diagrams",
            Self::Family(_) => "family",
        }
    }

    pub const fn family_id(self) -> Option<&'static str> {
        match self {
            Self::AllDiagrams => None,
            Self::Family(family_id) => Some(family_id),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PresentationAspectDescriptor {
    id: &'static str,
    applicability: PresentationAspectApplicability,
    required_capability_id: Option<&'static str>,
}

impl PresentationAspectDescriptor {
    pub const fn id(&self) -> &'static str {
        self.id
    }

    pub const fn applicability(&self) -> PresentationAspectApplicability {
        self.applicability
    }

    pub const fn required_capability_id(&self) -> Option<&'static str> {
        self.required_capability_id
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PresentationProfileDescriptor {
    profile: PresentationProfile,
    aspects: &'static [PresentationAspectDescriptor],
}

impl PresentationProfileDescriptor {
    pub const fn id(&self) -> &'static str {
        self.profile.id()
    }

    pub const fn aspects(&self) -> &'static [PresentationAspectDescriptor] {
        self.aspects
    }
}

const MERMAN_MODERN_ASPECTS: [PresentationAspectDescriptor; 3] = [
    PresentationAspectDescriptor {
        id: "global-defaults",
        applicability: PresentationAspectApplicability::AllDiagrams,
        required_capability_id: None,
    },
    PresentationAspectDescriptor {
        id: "flowchart-svg",
        applicability: PresentationAspectApplicability::Family("flowchart"),
        required_capability_id: None,
    },
    PresentationAspectDescriptor {
        id: "flowchart-elk-default",
        applicability: PresentationAspectApplicability::Family("flowchart"),
        required_capability_id: Some("layout-elk"),
    },
];

const PRESENTATION_PROFILE_DESCRIPTORS: [PresentationProfileDescriptor; 1] =
    [PresentationProfileDescriptor {
        profile: PresentationProfile::MermanModern,
        aspects: &MERMAN_MODERN_ASPECTS,
    }];

pub const fn presentation_profile_descriptors() -> &'static [PresentationProfileDescriptor] {
    &PRESENTATION_PROFILE_DESCRIPTORS
}
