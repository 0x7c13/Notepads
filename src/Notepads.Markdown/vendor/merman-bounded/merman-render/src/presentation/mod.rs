//! Product presentation inputs that are intentionally separate from Mermaid config and SVG output.

mod compiler;
mod presets;
mod profile;
mod theme;

pub use presets::{HostThemePreset, ThemePresetDescriptor, theme_preset_descriptors};
#[cfg(any(
    feature = "diagram-flowchart",
    feature = "diagram-swimlane",
    feature = "diagram-agentflow"
))]
pub(crate) use profile::FlowchartPresentationPolicy;
pub use profile::{
    Presentation, PresentationAspectApplicability, PresentationAspectDescriptor,
    PresentationAspectResolution, PresentationAspectState, PresentationProfile,
    PresentationProfileDescriptor, PresentationRenderPolicy, ResolvedPresentation,
    presentation_profile_descriptors,
};
pub use theme::{HostTheme, HostThemeAppearance, ThemeRole};

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[non_exhaustive]
pub enum PresentationError {
    #[error("unknown host theme preset `{0}`")]
    UnknownThemePreset(String),
    #[error("unknown presentation profile `{0}`")]
    UnknownPresentationProfile(String),
    #[error("unknown theme role `{0}`")]
    UnknownThemeRole(String),
    #[error("{field} must be a non-empty single CSS declaration value")]
    InvalidCssValue { field: String },
}

fn css_declaration_value(
    value: impl Into<String>,
    field: &str,
) -> Result<String, PresentationError> {
    let value = value.into();
    let trimmed = value.trim();
    let invalid = trimmed.is_empty()
        || trimmed
            .chars()
            .any(|ch| ch.is_control() || matches!(ch, ';' | '"' | '\'' | '<' | '>' | '{' | '}'));
    if invalid {
        return Err(PresentationError::InvalidCssValue {
            field: field.to_string(),
        });
    }
    Ok(trimmed.to_string())
}

fn css_font_family(value: impl Into<String>, field: &str) -> Result<String, PresentationError> {
    let value = value.into();
    let trimmed = value.trim();
    let invalid = trimmed.is_empty()
        || trimmed
            .chars()
            .any(|ch| ch.is_control() || matches!(ch, ';' | '<' | '>' | '{' | '}'));
    if invalid {
        return Err(PresentationError::InvalidCssValue {
            field: field.to_string(),
        });
    }
    Ok(trimmed.to_string())
}
