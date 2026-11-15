//! SVG renderers for Mermaid-parity diagrams.
//!
//! Public API is re-exported from the parity-focused renderer implementation.
//!
//! This module is named `parity` to reflect intent: upstream Mermaid is treated as the spec, and
//! SVG output is gated by DOM parity checks.

#![forbid(unsafe_code)]

mod fallback;
#[cfg_attr(
    not(feature = "all-diagrams"),
    allow(
        dead_code,
        unused_imports,
        reason = "Shared rendering utilities have different callers in each diagram selection."
    )
)]
mod icon_registry;
mod parity;
mod pipeline;
pub(crate) mod scanner;

#[cfg(all(feature = "layout-cytoscape", feature = "diagram-architecture"))]
pub(crate) use parity::render_architecture_family_artifact;
pub(crate) use parity::render_builtin_family_artifact;
pub(crate) use parity::theme as render_theme;
pub(crate) use pipeline::SvgPostprocessExecution;

pub use fallback::foreign_object_label_fallback_svg_text;
pub use icon_registry::{
    IconPack, IconRegistry, IconRegistryBuildError, IconRegistryBuildErrorKind,
    IconRegistryBuilder, IconRegistryResourceLimitDescriptor, IconRegistryResourceLimitId,
    icon_registry_resource_limit_descriptors,
};
pub use parity::*;
pub use pipeline::{
    CssOverridePolicy, CssOverridePostprocessor, ForeignObjectFallbackPostprocessor,
    ResvgCompatibleSvg, RootBackgroundPostprocessor, SanitizeCssPostprocessor,
    SanitizeSvgAttributesPostprocessor, ScopedCssPostprocessor, StripForeignObjectPostprocessor,
    SvgOutputPolicy, SvgPipeline, SvgPipelinePreset, SvgPostprocessContext, SvgPostprocessMetadata,
    SvgPostprocessor, SvgReferencePlan, finalize_resvg_svg, rebase_svg_ids,
    validate_static_inline_svg, validate_static_inline_svg_admission,
};
