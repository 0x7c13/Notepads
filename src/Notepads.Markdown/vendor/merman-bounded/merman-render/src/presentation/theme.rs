use std::collections::BTreeMap;

use merman_core::MermaidConfig;

use super::{HostThemePreset, PresentationError, compiler, css_declaration_value, css_font_family};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
#[non_exhaustive]
pub enum HostThemeAppearance {
    #[default]
    Light,
    Dark,
}

impl HostThemeAppearance {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Light => "light",
            Self::Dark => "dark",
        }
    }

    pub const fn is_dark(self) -> bool {
        matches!(self, Self::Dark)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[non_exhaustive]
pub enum ThemeRole {
    Canvas,
    Surface,
    SurfaceAlt,
    SurfaceMuted,
    Text,
    SubtleText,
    Border,
    Line,
    EdgeLabelBackground,
    ClusterBackground,
    ClusterBorder,
    NoteBackground,
    NoteBorder,
    NoteText,
    ActorBackground,
    ActorBorder,
    ActorText,
    ActivationBackground,
    ActivationBorder,
    Error,
    Warning,
    Success,
}

impl ThemeRole {
    pub const ALL: [Self; 22] = [
        Self::Canvas,
        Self::Surface,
        Self::SurfaceAlt,
        Self::SurfaceMuted,
        Self::Text,
        Self::SubtleText,
        Self::Border,
        Self::Line,
        Self::EdgeLabelBackground,
        Self::ClusterBackground,
        Self::ClusterBorder,
        Self::NoteBackground,
        Self::NoteBorder,
        Self::NoteText,
        Self::ActorBackground,
        Self::ActorBorder,
        Self::ActorText,
        Self::ActivationBackground,
        Self::ActivationBorder,
        Self::Error,
        Self::Warning,
        Self::Success,
    ];

    pub const fn id(self) -> &'static str {
        match self {
            Self::Canvas => "canvas",
            Self::Surface => "surface",
            Self::SurfaceAlt => "surface-alt",
            Self::SurfaceMuted => "surface-muted",
            Self::Text => "text",
            Self::SubtleText => "subtle-text",
            Self::Border => "border",
            Self::Line => "line",
            Self::EdgeLabelBackground => "edge-label-background",
            Self::ClusterBackground => "cluster-background",
            Self::ClusterBorder => "cluster-border",
            Self::NoteBackground => "note-background",
            Self::NoteBorder => "note-border",
            Self::NoteText => "note-text",
            Self::ActorBackground => "actor-background",
            Self::ActorBorder => "actor-border",
            Self::ActorText => "actor-text",
            Self::ActivationBackground => "activation-background",
            Self::ActivationBorder => "activation-border",
            Self::Error => "error",
            Self::Warning => "warning",
            Self::Success => "success",
        }
    }

    pub fn from_id(id: &str) -> Result<Self, PresentationError> {
        Self::ALL
            .into_iter()
            .find(|role| role.id() == id)
            .ok_or_else(|| PresentationError::UnknownThemeRole(id.to_string()))
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct HostTheme {
    appearance: Option<HostThemeAppearance>,
    font_family: Option<String>,
    font_size: Option<String>,
    roles: BTreeMap<ThemeRole, String>,
    series_palette: Vec<String>,
}

impl HostTheme {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn from_preset(preset: HostThemePreset) -> Self {
        preset.theme()
    }

    pub const fn appearance(&self) -> Option<HostThemeAppearance> {
        self.appearance
    }

    pub fn font_family(&self) -> Option<&str> {
        self.font_family.as_deref()
    }

    pub fn font_size(&self) -> Option<&str> {
        self.font_size.as_deref()
    }

    pub fn role(&self, role: ThemeRole) -> Option<&str> {
        self.roles.get(&role).map(String::as_str)
    }

    pub fn roles(&self) -> impl ExactSizeIterator<Item = (ThemeRole, &str)> {
        self.roles
            .iter()
            .map(|(role, value)| (*role, value.as_str()))
    }

    pub fn series_palette(&self) -> &[String] {
        &self.series_palette
    }

    pub fn with_appearance(mut self, appearance: HostThemeAppearance) -> Self {
        self.appearance = Some(appearance);
        self
    }

    pub fn try_with_font_family(
        mut self,
        font_family: impl Into<String>,
    ) -> Result<Self, PresentationError> {
        self.font_family = Some(css_font_family(font_family, "theme.font_family")?);
        Ok(self)
    }

    pub fn try_with_font_size(
        mut self,
        font_size: impl Into<String>,
    ) -> Result<Self, PresentationError> {
        self.font_size = Some(css_declaration_value(font_size, "theme.font_size")?);
        Ok(self)
    }

    pub fn try_with_role(
        mut self,
        role: ThemeRole,
        value: impl Into<String>,
    ) -> Result<Self, PresentationError> {
        self.roles.insert(
            role,
            css_declaration_value(value, &format!("theme.roles.{}", role.id()))?,
        );
        Ok(self)
    }

    pub fn try_with_series_palette(
        mut self,
        palette: impl IntoIterator<Item = impl Into<String>>,
    ) -> Result<Self, PresentationError> {
        self.series_palette = palette
            .into_iter()
            .enumerate()
            .map(|(index, value)| {
                css_declaration_value(value, &format!("theme.series_palette[{index}]"))
            })
            .collect::<Result<Vec<_>, _>>()?;
        Ok(self)
    }

    pub(crate) fn mermaid_config_patch(&self) -> MermaidConfig {
        compiler::compile(self)
    }

    pub(super) fn bundled(
        appearance: HostThemeAppearance,
        roles: &[(ThemeRole, &str)],
        series_palette: &[&str],
    ) -> Self {
        Self {
            appearance: Some(appearance),
            font_family: Some(
                r#"Inter, ui-sans-serif, system-ui, -apple-system, "Segoe UI", sans-serif"#
                    .to_string(),
            ),
            font_size: Some("14px".to_string()),
            roles: roles
                .iter()
                .map(|(role, value)| (*role, (*value).to_string()))
                .collect(),
            series_palette: series_palette
                .iter()
                .map(|value| (*value).to_string())
                .collect(),
        }
    }

    pub(super) fn has_values(&self) -> bool {
        self.appearance.is_some()
            || self.font_family.is_some()
            || self.font_size.is_some()
            || !self.roles.is_empty()
            || !self.series_palette.is_empty()
    }

    pub(super) fn has_paint_values(&self) -> bool {
        self.appearance.is_some() || !self.roles.is_empty() || !self.series_palette.is_empty()
    }
}
