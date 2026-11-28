use super::{
    CssOverridePolicy, CssOverridePostprocessor, GitGraphBranchLabelBaselinePostprocessor,
    RootBackgroundPostprocessor, ScopedCssPostprocessor, SvgPipeline, SvgPipelinePreset,
};

/// Canonical consumer-facing SVG output policy shared by presentation, bindings, and CLI exports.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SvgOutputPolicy {
    pub preset: SvgPipelinePreset,
    pub css_override_policy: CssOverridePolicy,
    /// Explicit host canvas color. `None` preserves Mermaid's unpainted root.
    pub root_background_color: Option<String>,
    pub drop_native_duplicate_fallbacks: bool,
    pub scoped_css: Option<String>,
}

impl Default for SvgOutputPolicy {
    fn default() -> Self {
        Self {
            preset: SvgPipelinePreset::Parity,
            css_override_policy: CssOverridePolicy::Preserve,
            root_background_color: None,
            drop_native_duplicate_fallbacks: false,
            scoped_css: None,
        }
    }
}

impl SvgOutputPolicy {
    pub fn pipeline(&self) -> SvgPipeline {
        let mut pipeline = SvgPipeline::from_preset(self.preset);

        if matches!(
            self.css_override_policy,
            CssOverridePolicy::StripExistingImportant
        ) {
            pipeline.push_postprocessor(CssOverridePostprocessor::strip_existing_important());
        }

        pipeline =
            pipeline.with_drop_native_duplicate_fallbacks(self.drop_native_duplicate_fallbacks);

        if matches!(self.preset, SvgPipelinePreset::ResvgSafe) {
            pipeline.push_postprocessor(GitGraphBranchLabelBaselinePostprocessor);
        }

        if let Some(color) = self
            .root_background_color
            .as_deref()
            .filter(|color| !color.trim().is_empty())
        {
            pipeline.push_postprocessor(RootBackgroundPostprocessor::new(color.trim()));
        }

        if let Some(css) = self
            .scoped_css
            .as_deref()
            .filter(|css| !css.trim().is_empty())
        {
            pipeline.push_postprocessor(
                ScopedCssPostprocessor::new(css.to_string())
                    .with_override_policy(self.css_override_policy),
            );
        }

        pipeline
    }
}
