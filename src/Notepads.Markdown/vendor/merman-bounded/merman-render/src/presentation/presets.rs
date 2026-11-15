use super::{HostTheme, HostThemeAppearance, PresentationError};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
#[non_exhaustive]
pub enum HostThemePreset {
    #[default]
    EditorLight,
    EditorDark,
    OneDark,
    GruvboxLight,
    GruvboxDark,
    AyuLight,
    AyuDark,
}

impl HostThemePreset {
    pub const ALL: [Self; 7] = [
        Self::EditorLight,
        Self::EditorDark,
        Self::OneDark,
        Self::GruvboxLight,
        Self::GruvboxDark,
        Self::AyuLight,
        Self::AyuDark,
    ];

    pub const fn id(self) -> &'static str {
        match self {
            Self::EditorLight => "editor-light",
            Self::EditorDark => "editor-dark",
            Self::OneDark => "one-dark",
            Self::GruvboxLight => "gruvbox-light",
            Self::GruvboxDark => "gruvbox-dark",
            Self::AyuLight => "ayu-light",
            Self::AyuDark => "ayu-dark",
        }
    }

    pub fn from_id(id: &str) -> Result<Self, PresentationError> {
        Self::ALL
            .into_iter()
            .find(|preset| preset.id() == id)
            .ok_or_else(|| PresentationError::UnknownThemePreset(id.to_string()))
    }

    pub const fn appearance(self) -> HostThemeAppearance {
        match self {
            Self::EditorLight | Self::GruvboxLight | Self::AyuLight => HostThemeAppearance::Light,
            Self::EditorDark | Self::OneDark | Self::GruvboxDark | Self::AyuDark => {
                HostThemeAppearance::Dark
            }
        }
    }

    pub(crate) fn theme(self) -> HostTheme {
        let (roles, palette) = match self {
            Self::EditorLight => (EDITOR_LIGHT_ROLES, EDITOR_LIGHT_PALETTE),
            Self::EditorDark => (EDITOR_DARK_ROLES, EDITOR_DARK_PALETTE),
            Self::OneDark => (ONE_DARK_ROLES, ONE_DARK_PALETTE),
            Self::GruvboxLight => (GRUVBOX_LIGHT_ROLES, GRUVBOX_LIGHT_PALETTE),
            Self::GruvboxDark => (GRUVBOX_DARK_ROLES, GRUVBOX_DARK_PALETTE),
            Self::AyuLight => (AYU_LIGHT_ROLES, AYU_LIGHT_PALETTE),
            Self::AyuDark => (AYU_DARK_ROLES, AYU_DARK_PALETTE),
        };
        HostTheme::bundled(self.appearance(), roles, palette)
    }
}

const EDITOR_LIGHT_ROLES: &[(super::ThemeRole, &str)] = &[
    (super::ThemeRole::Canvas, "#ffffff"),
    (super::ThemeRole::Surface, "#f8fafc"),
    (super::ThemeRole::SurfaceAlt, "#e2e8f0"),
    (super::ThemeRole::SurfaceMuted, "#f1f5f9"),
    (super::ThemeRole::Text, "#0f172a"),
    (super::ThemeRole::SubtleText, "#475569"),
    (super::ThemeRole::Border, "#94a3b8"),
    (super::ThemeRole::Line, "#64748b"),
    (super::ThemeRole::EdgeLabelBackground, "#ffffff"),
    (super::ThemeRole::ClusterBackground, "#f1f5f9"),
    (super::ThemeRole::ClusterBorder, "#cbd5e1"),
    (super::ThemeRole::NoteBackground, "#fff7ed"),
    (super::ThemeRole::NoteBorder, "#fdba74"),
    (super::ThemeRole::NoteText, "#7c2d12"),
    (super::ThemeRole::ActorBackground, "#f8fafc"),
    (super::ThemeRole::ActorBorder, "#94a3b8"),
    (super::ThemeRole::ActorText, "#0f172a"),
    (super::ThemeRole::ActivationBackground, "#e2e8f0"),
    (super::ThemeRole::ActivationBorder, "#94a3b8"),
    (super::ThemeRole::Error, "#dc2626"),
    (super::ThemeRole::Warning, "#d97706"),
    (super::ThemeRole::Success, "#059669"),
];
const EDITOR_LIGHT_PALETTE: &[&str] = &[
    "#2563eb", "#059669", "#d97706", "#7c3aed", "#0891b2", "#be123c", "#a16207", "#65a30d",
];

const EDITOR_DARK_ROLES: &[(super::ThemeRole, &str)] = &[
    (super::ThemeRole::Canvas, "#0f172a"),
    (super::ThemeRole::Surface, "#111827"),
    (super::ThemeRole::SurfaceAlt, "#1f2937"),
    (super::ThemeRole::SurfaceMuted, "#334155"),
    (super::ThemeRole::Text, "#e5e7eb"),
    (super::ThemeRole::SubtleText, "#cbd5e1"),
    (super::ThemeRole::Border, "#475569"),
    (super::ThemeRole::Line, "#94a3b8"),
    (super::ThemeRole::EdgeLabelBackground, "#0f172a"),
    (super::ThemeRole::ClusterBackground, "#1e293b"),
    (super::ThemeRole::ClusterBorder, "#475569"),
    (super::ThemeRole::NoteBackground, "#422006"),
    (super::ThemeRole::NoteBorder, "#f59e0b"),
    (super::ThemeRole::NoteText, "#fef3c7"),
    (super::ThemeRole::ActorBackground, "#1f2937"),
    (super::ThemeRole::ActorBorder, "#475569"),
    (super::ThemeRole::ActorText, "#e5e7eb"),
    (super::ThemeRole::ActivationBackground, "#334155"),
    (super::ThemeRole::ActivationBorder, "#64748b"),
    (super::ThemeRole::Error, "#f87171"),
    (super::ThemeRole::Warning, "#fbbf24"),
    (super::ThemeRole::Success, "#34d399"),
];
const EDITOR_DARK_PALETTE: &[&str] = &[
    "#60a5fa", "#34d399", "#f59e0b", "#c084fc", "#22d3ee", "#fb7185", "#facc15", "#a3e635",
];

const ONE_DARK_ROLES: &[(super::ThemeRole, &str)] = &[
    (super::ThemeRole::Canvas, "#282c34"),
    (super::ThemeRole::Surface, "#21252b"),
    (super::ThemeRole::SurfaceAlt, "#2c313a"),
    (super::ThemeRole::SurfaceMuted, "#3e4451"),
    (super::ThemeRole::Text, "#abb2bf"),
    (super::ThemeRole::SubtleText, "#abb2bf"),
    (super::ThemeRole::Border, "#3e4451"),
    (super::ThemeRole::Line, "#61afef"),
    (super::ThemeRole::EdgeLabelBackground, "#282c34"),
    (super::ThemeRole::ClusterBackground, "#2c313a"),
    (super::ThemeRole::ClusterBorder, "#3e4451"),
    (super::ThemeRole::NoteBackground, "#3a2f1b"),
    (super::ThemeRole::NoteBorder, "#e5c07b"),
    (super::ThemeRole::NoteText, "#f0dca4"),
    (super::ThemeRole::ActorBackground, "#2c313a"),
    (super::ThemeRole::ActorBorder, "#3e4451"),
    (super::ThemeRole::ActorText, "#abb2bf"),
    (super::ThemeRole::ActivationBackground, "#3e4451"),
    (super::ThemeRole::ActivationBorder, "#5c6370"),
    (super::ThemeRole::Error, "#e06c75"),
    (super::ThemeRole::Warning, "#e5c07b"),
    (super::ThemeRole::Success, "#98c379"),
];
const ONE_DARK_PALETTE: &[&str] = &[
    "#61afef", "#98c379", "#e5c07b", "#c678dd", "#56b6c2", "#e06c75", "#d19a66", "#be5046",
];

const GRUVBOX_LIGHT_ROLES: &[(super::ThemeRole, &str)] = &[
    (super::ThemeRole::Canvas, "#fbf1c7"),
    (super::ThemeRole::Surface, "#f2e5bc"),
    (super::ThemeRole::SurfaceAlt, "#ebdbb2"),
    (super::ThemeRole::SurfaceMuted, "#d5c4a1"),
    (super::ThemeRole::Text, "#3c3836"),
    (super::ThemeRole::SubtleText, "#665c54"),
    (super::ThemeRole::Border, "#d5c4a1"),
    (super::ThemeRole::Line, "#7c6f64"),
    (super::ThemeRole::EdgeLabelBackground, "#fbf1c7"),
    (super::ThemeRole::ClusterBackground, "#ebdbb2"),
    (super::ThemeRole::ClusterBorder, "#d5c4a1"),
    (super::ThemeRole::NoteBackground, "#f2e5bc"),
    (super::ThemeRole::NoteBorder, "#d79921"),
    (super::ThemeRole::NoteText, "#3c3836"),
    (super::ThemeRole::ActorBackground, "#ebdbb2"),
    (super::ThemeRole::ActorBorder, "#d5c4a1"),
    (super::ThemeRole::ActorText, "#3c3836"),
    (super::ThemeRole::ActivationBackground, "#d5c4a1"),
    (super::ThemeRole::ActivationBorder, "#bdae93"),
    (super::ThemeRole::Error, "#cc241d"),
    (super::ThemeRole::Warning, "#d79921"),
    (super::ThemeRole::Success, "#98971a"),
];
const GRUVBOX_LIGHT_PALETTE: &[&str] = &[
    "#458588", "#98971a", "#d79921", "#b16286", "#689d6a", "#cc241d", "#d65d0e", "#427b58",
];

const GRUVBOX_DARK_ROLES: &[(super::ThemeRole, &str)] = &[
    (super::ThemeRole::Canvas, "#282828"),
    (super::ThemeRole::Surface, "#3c3836"),
    (super::ThemeRole::SurfaceAlt, "#504945"),
    (super::ThemeRole::SurfaceMuted, "#665c54"),
    (super::ThemeRole::Text, "#ebdbb2"),
    (super::ThemeRole::SubtleText, "#d5c4a1"),
    (super::ThemeRole::Border, "#665c54"),
    (super::ThemeRole::Line, "#d5c4a1"),
    (super::ThemeRole::EdgeLabelBackground, "#282828"),
    (super::ThemeRole::ClusterBackground, "#3c3836"),
    (super::ThemeRole::ClusterBorder, "#665c54"),
    (super::ThemeRole::NoteBackground, "#3c3836"),
    (super::ThemeRole::NoteBorder, "#fabd2f"),
    (super::ThemeRole::NoteText, "#fbf1c7"),
    (super::ThemeRole::ActorBackground, "#3c3836"),
    (super::ThemeRole::ActorBorder, "#665c54"),
    (super::ThemeRole::ActorText, "#ebdbb2"),
    (super::ThemeRole::ActivationBackground, "#504945"),
    (super::ThemeRole::ActivationBorder, "#7c6f64"),
    (super::ThemeRole::Error, "#fb4934"),
    (super::ThemeRole::Warning, "#fabd2f"),
    (super::ThemeRole::Success, "#b8bb26"),
];
const GRUVBOX_DARK_PALETTE: &[&str] = &[
    "#83a598", "#b8bb26", "#fabd2f", "#d3869b", "#8ec07c", "#fb4934", "#fe8019", "#689d6a",
];

const AYU_LIGHT_ROLES: &[(super::ThemeRole, &str)] = &[
    (super::ThemeRole::Canvas, "#fcfcfc"),
    (super::ThemeRole::Surface, "#f3f4f5"),
    (super::ThemeRole::SurfaceAlt, "#e6e8eb"),
    (super::ThemeRole::SurfaceMuted, "#d9d7ce"),
    (super::ThemeRole::Text, "#5c6166"),
    (super::ThemeRole::SubtleText, "#5c6166"),
    (super::ThemeRole::Border, "#8a9199"),
    (super::ThemeRole::Line, "#5c6166"),
    (super::ThemeRole::EdgeLabelBackground, "#fcfcfc"),
    (super::ThemeRole::ClusterBackground, "#f3f4f5"),
    (super::ThemeRole::ClusterBorder, "#8a9199"),
    (super::ThemeRole::NoteBackground, "#fff3bf"),
    (super::ThemeRole::NoteBorder, "#ffaa33"),
    (super::ThemeRole::NoteText, "#5c6166"),
    (super::ThemeRole::ActorBackground, "#f3f4f5"),
    (super::ThemeRole::ActorBorder, "#8a9199"),
    (super::ThemeRole::ActorText, "#5c6166"),
    (super::ThemeRole::ActivationBackground, "#e6e8eb"),
    (super::ThemeRole::ActivationBorder, "#8a9199"),
    (super::ThemeRole::Error, "#f07171"),
    (super::ThemeRole::Warning, "#ffaa33"),
    (super::ThemeRole::Success, "#86b300"),
];
const AYU_LIGHT_PALETTE: &[&str] = &[
    "#55b4d4", "#86b300", "#ffaa33", "#a37acc", "#4cbf99", "#f07171", "#f2ae49", "#399ee6",
];

const AYU_DARK_ROLES: &[(super::ThemeRole, &str)] = &[
    (super::ThemeRole::Canvas, "#0b0e14"),
    (super::ThemeRole::Surface, "#11151c"),
    (super::ThemeRole::SurfaceAlt, "#1f2430"),
    (super::ThemeRole::SurfaceMuted, "#343b48"),
    (super::ThemeRole::Text, "#bfbdb6"),
    (super::ThemeRole::SubtleText, "#8a9199"),
    (super::ThemeRole::Border, "#343b48"),
    (super::ThemeRole::Line, "#59c2ff"),
    (super::ThemeRole::EdgeLabelBackground, "#0b0e14"),
    (super::ThemeRole::ClusterBackground, "#1f2430"),
    (super::ThemeRole::ClusterBorder, "#343b48"),
    (super::ThemeRole::NoteBackground, "#332a14"),
    (super::ThemeRole::NoteBorder, "#ffb454"),
    (super::ThemeRole::NoteText, "#ffdf99"),
    (super::ThemeRole::ActorBackground, "#1f2430"),
    (super::ThemeRole::ActorBorder, "#343b48"),
    (super::ThemeRole::ActorText, "#bfbdb6"),
    (super::ThemeRole::ActivationBackground, "#343b48"),
    (super::ThemeRole::ActivationBorder, "#4f5866"),
    (super::ThemeRole::Error, "#f07178"),
    (super::ThemeRole::Warning, "#ffb454"),
    (super::ThemeRole::Success, "#aad94c"),
];
const AYU_DARK_PALETTE: &[&str] = &[
    "#59c2ff", "#aad94c", "#ffb454", "#d2a6ff", "#95e6cb", "#f07178", "#f29668", "#39bae6",
];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ThemePresetDescriptor {
    preset: HostThemePreset,
}

impl ThemePresetDescriptor {
    pub const fn id(&self) -> &'static str {
        self.preset.id()
    }

    pub const fn appearance(&self) -> HostThemeAppearance {
        self.preset.appearance()
    }
}

const THEME_PRESET_DESCRIPTORS: [ThemePresetDescriptor; 7] = [
    ThemePresetDescriptor {
        preset: HostThemePreset::EditorLight,
    },
    ThemePresetDescriptor {
        preset: HostThemePreset::EditorDark,
    },
    ThemePresetDescriptor {
        preset: HostThemePreset::OneDark,
    },
    ThemePresetDescriptor {
        preset: HostThemePreset::GruvboxLight,
    },
    ThemePresetDescriptor {
        preset: HostThemePreset::GruvboxDark,
    },
    ThemePresetDescriptor {
        preset: HostThemePreset::AyuLight,
    },
    ThemePresetDescriptor {
        preset: HostThemePreset::AyuDark,
    },
];

pub const fn theme_preset_descriptors() -> &'static [ThemePresetDescriptor] {
    &THEME_PRESET_DESCRIPTORS
}
