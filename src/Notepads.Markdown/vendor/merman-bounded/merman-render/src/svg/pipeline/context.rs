use super::builtin::util::{
    SvgTagScanner, checkpoint_loop, next_svg_quoted_attr_with_checkpoints, start_tag_name,
};
use super::preset::SvgPipelinePreset;
use crate::environment::{RenderSession, RoutedTextMeasurer, TextMeasurementPhase};
use crate::family::RenderFamilyKind;
use crate::resources::{
    RenderResourcePolicy, ResourceLimitCause, ResourceLimitExceeded, ResourceLimitOverride,
    ResourceLimitPhase,
};
use merman_core::OperationPhase;

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct SvgPostprocessMetadata {
    family_kind: Option<RenderFamilyKind>,
    diagram_type: Option<String>,
    diagram_title: Option<String>,
    svg_id: Option<String>,
}

impl SvgPostprocessMetadata {
    pub fn new() -> Self {
        Self::default()
    }

    /// Recovers descriptive metadata from the root SVG without granting family capabilities.
    ///
    /// Family-specific passes consume an explicitly supplied [`RenderFamilyKind`], never metadata
    /// inferred from SVG text.
    pub fn from_svg(svg: &str) -> Self {
        let mut checkpoint = || Ok::<(), std::convert::Infallible>(());
        match Self::from_svg_with_checkpoints(svg, &mut checkpoint) {
            Ok(metadata) => metadata,
            Err(error) => match error {},
        }
    }

    pub(crate) fn from_svg_with_execution(
        svg: &str,
        execution: SvgPostprocessExecution<'_>,
    ) -> crate::Result<Self> {
        execution.checkpoint()?;
        execution.preflight_svg_byte_count(svg.len())?;
        Self::from_svg_with_checkpoints(svg, &mut || execution.checkpoint())
    }

    fn from_svg_with_checkpoints<E>(
        svg: &str,
        checkpoint: &mut impl FnMut() -> Result<(), E>,
    ) -> Result<Self, E> {
        checkpoint()?;
        let mut scanner = SvgTagScanner::new(svg);
        let mut tag_iteration = 0usize;
        let root_tag = loop {
            let Some(tag) = scanner.next_with_checkpoints(checkpoint)? else {
                return Ok(Self::default());
            };
            checkpoint_loop(tag_iteration, checkpoint)?;
            tag_iteration = tag_iteration.saturating_add(1);
            let Some(root_name) = start_tag_name(tag.raw()) else {
                continue;
            };
            if root_name != "svg" {
                return Ok(Self::default());
            }
            break tag.raw();
        };

        let mut diagram_type = None;
        let mut svg_id = None;
        let mut cursor = 0usize;
        let mut attribute_iteration = 0usize;
        while let Some(attribute) =
            next_svg_quoted_attr_with_checkpoints(root_tag, cursor, checkpoint)?
        {
            checkpoint_loop(attribute_iteration, checkpoint)?;
            attribute_iteration = attribute_iteration.saturating_add(1);
            let name = &root_tag[attribute.name_start..attribute.name_end];
            if name.eq_ignore_ascii_case("aria-roledescription") && diagram_type.is_none() {
                diagram_type = Some(copy_trimmed_with_checkpoints(
                    &root_tag[attribute.value_start..attribute.value_end],
                    checkpoint,
                )?);
            } else if name.eq_ignore_ascii_case("id") && svg_id.is_none() {
                svg_id = Some(copy_trimmed_with_checkpoints(
                    &root_tag[attribute.value_start..attribute.value_end],
                    checkpoint,
                )?);
            }
            cursor = attribute.full_end;
        }
        checkpoint()?;

        Ok(Self {
            diagram_type,
            svg_id,
            ..Self::default()
        })
    }

    /// Supplies the renderer-owned family identity required by family-specific built-in passes.
    pub(crate) fn with_family_kind(mut self, family_kind: RenderFamilyKind) -> Self {
        self.family_kind = Some(family_kind);
        self
    }

    /// Records descriptive diagram metadata without granting family-specific processing.
    pub fn with_diagram_type(mut self, diagram_type: impl Into<String>) -> Self {
        self.diagram_type = Some(diagram_type.into());
        self
    }

    pub fn with_optional_diagram_type(mut self, diagram_type: Option<impl Into<String>>) -> Self {
        self.diagram_type = diagram_type.map(Into::into);
        self
    }

    pub fn with_diagram_title(mut self, diagram_title: impl Into<String>) -> Self {
        self.diagram_title = Some(diagram_title.into());
        self
    }

    pub fn with_optional_diagram_title(mut self, diagram_title: Option<impl Into<String>>) -> Self {
        self.diagram_title = diagram_title.map(Into::into);
        self
    }

    pub fn with_svg_id(mut self, svg_id: impl Into<String>) -> Self {
        self.svg_id = Some(svg_id.into());
        self
    }

    pub fn with_optional_svg_id(mut self, svg_id: Option<impl Into<String>>) -> Self {
        if let Some(svg_id) = svg_id {
            self.svg_id = Some(svg_id.into());
        }
        self
    }

    pub fn diagram_type(&self) -> Option<&str> {
        self.diagram_type.as_deref()
    }

    pub fn family_kind(&self) -> Option<RenderFamilyKind> {
        self.family_kind
    }

    pub fn diagram_title(&self) -> Option<&str> {
        self.diagram_title.as_deref()
    }

    pub fn svg_id(&self) -> Option<&str> {
        self.svg_id.as_deref()
    }
}

fn copy_trimmed_with_checkpoints<E>(
    value: &str,
    checkpoint: &mut impl FnMut() -> Result<(), E>,
) -> Result<String, E> {
    let value = value.trim();
    checkpoint()?;
    let mut copied = String::with_capacity(value.len());
    for (index, character) in value.chars().enumerate() {
        checkpoint_loop(index, checkpoint)?;
        copied.push(character);
    }
    checkpoint()?;
    Ok(copied)
}

/// Operation-owned capabilities available to built-in SVG postprocessing stages.
///
/// Custom [`super::SvgPostprocessor`] implementations retain the documented opaque-callback
/// boundary. This projection is intentionally crate-private so built-ins can cooperate without
/// exposing the render session or its resource ledger through the public extension API.
#[derive(Clone, Copy)]
pub(crate) struct SvgPostprocessExecution<'a> {
    session: &'a RenderSession,
}

impl<'a> SvgPostprocessExecution<'a> {
    pub(crate) const fn new(session: &'a RenderSession) -> Self {
        Self { session }
    }

    pub(crate) fn checkpoint(self) -> crate::Result<()> {
        self.session.checkpoint(OperationPhase::Postprocess)
    }

    pub(crate) const fn resource_policy(self) -> RenderResourcePolicy {
        self.session.resource_policy()
    }

    pub(crate) fn preflight_svg_byte_count(self, actual: usize) -> crate::Result<()> {
        self.session
            .work_meter()
            .preflight_svg_byte_count(
                actual,
                ResourceLimitPhase::SvgPostprocess,
                OperationPhase::Postprocess,
            )
            .map_err(Into::into)
    }

    pub(crate) fn preflight_svg_structure(
        self,
        elements: usize,
        tree_depth: usize,
    ) -> crate::Result<()> {
        self.session
            .work_meter()
            .preflight_svg_structure(elements, tree_depth, OperationPhase::Postprocess)
            .map_err(Into::into)
    }

    pub(crate) fn selector_index_limit(self, actual: usize, maximum: usize) -> crate::Error {
        let policy = self.resource_policy();
        ResourceLimitExceeded {
            cause: ResourceLimitCause::Ceiling,
            phase: ResourceLimitPhase::SvgPostprocess,
            limit: "svg_fallback_selector_index",
            actual,
            max: maximum,
            profile: policy.profile(),
            explicit_overrides: policy
                .explicit_overrides()
                .map(|(id, value)| ResourceLimitOverride { id, value })
                .collect(),
        }
        .into()
    }

    pub(crate) fn svg_byte_count_overflow(self) -> crate::Error {
        self.session
            .work_meter()
            .terminate_svg_byte_count_overflow(
                ResourceLimitPhase::SvgPostprocess,
                OperationPhase::Postprocess,
            )
            .into()
    }

    pub(crate) fn terminate_resource_error(self, error: ResourceLimitExceeded) -> crate::Error {
        self.session
            .work_meter()
            .terminate_absolute_resource_error(error, OperationPhase::Postprocess)
            .into()
    }

    pub(crate) fn controlled_text_measurer(
        self,
        phase: TextMeasurementPhase,
    ) -> RoutedTextMeasurer<'a> {
        self.session
            .controlled_text_measurer(phase, OperationPhase::Postprocess)
    }
}

#[derive(Clone, Copy)]
pub struct SvgPostprocessContext<'a> {
    preset: SvgPipelinePreset,
    pass_index: usize,
    pass_name: &'a str,
    metadata: &'a SvgPostprocessMetadata,
    session: &'a RenderSession,
}

impl std::fmt::Debug for SvgPostprocessContext<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SvgPostprocessContext")
            .field("preset", &self.preset)
            .field("pass_index", &self.pass_index)
            .field("pass_name", &self.pass_name)
            .field("metadata", &self.metadata)
            .finish_non_exhaustive()
    }
}

impl<'a> SvgPostprocessContext<'a> {
    pub(crate) fn new(
        preset: SvgPipelinePreset,
        pass_index: usize,
        pass_name: &'a str,
        metadata: &'a SvgPostprocessMetadata,
        session: &'a RenderSession,
    ) -> Self {
        Self {
            preset,
            pass_index,
            pass_name,
            metadata,
            session,
        }
    }

    pub fn preset(&self) -> SvgPipelinePreset {
        self.preset
    }

    pub fn pass_index(&self) -> usize {
        self.pass_index
    }

    pub fn pass_name(&self) -> &'a str {
        self.pass_name
    }

    pub fn diagram_type(&self) -> Option<&'a str> {
        self.metadata.diagram_type()
    }

    pub fn family_kind(&self) -> Option<RenderFamilyKind> {
        self.metadata.family_kind()
    }

    pub fn diagram_title(&self) -> Option<&'a str> {
        self.metadata.diagram_title()
    }

    pub fn svg_id(&self) -> Option<&'a str> {
        self.metadata.svg_id()
    }

    pub fn text_measurer(&self, phase: TextMeasurementPhase) -> RoutedTextMeasurer<'a> {
        self.session.text_measurer(phase)
    }

    pub(crate) fn controlled_text_measurer(
        &self,
        phase: TextMeasurementPhase,
    ) -> RoutedTextMeasurer<'a> {
        self.session
            .controlled_text_measurer(phase, OperationPhase::Postprocess)
    }

    /// Checks the operation-owned postprocess control for work performed by a built-in pass.
    pub(crate) fn checkpoint(&self) -> crate::Result<()> {
        self.execution().checkpoint()
    }

    pub(crate) const fn execution(&self) -> SvgPostprocessExecution<'a> {
        SvgPostprocessExecution::new(self.session)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn metadata_extracts_root_svg_id_and_diagram_type() {
        let metadata = SvgPostprocessMetadata::from_svg(
            r#"<svg xmlns="http://www.w3.org/2000/svg" id="diagram-1" aria-roledescription="quadrantChart"><g/></svg>"#,
        );

        assert_eq!(metadata.svg_id(), Some("diagram-1"));
        assert_eq!(metadata.diagram_type(), Some("quadrantChart"));
        assert_eq!(metadata.family_kind(), None);
    }

    #[test]
    fn metadata_ignores_non_root_ids() {
        let metadata = SvgPostprocessMetadata::from_svg(
            r#"<g id="nested" aria-roledescription="quadrantChart"></g>"#,
        );

        assert_eq!(metadata.svg_id(), None);
        assert_eq!(metadata.diagram_type(), None);
        assert_eq!(metadata.family_kind(), None);
    }

    #[test]
    fn metadata_rejects_comment_spoofs_and_nested_svg_roles() {
        for svg in [
            r#"<!-- <svg id="spoof" aria-roledescription="quadrantChart"> --><g/>"#,
            r#"<g><svg id="nested" aria-roledescription="quadrantChart"/></g>"#,
        ] {
            let metadata = SvgPostprocessMetadata::from_svg(svg);

            assert_eq!(metadata.svg_id(), None, "{svg}");
            assert_eq!(metadata.diagram_type(), None, "{svg}");
            assert_eq!(metadata.family_kind(), None, "{svg}");
        }
    }

    #[test]
    fn metadata_accepts_a_root_svg_after_xml_prolog_and_comment() {
        let metadata = SvgPostprocessMetadata::from_svg(
            r#"<?xml version="1.0"?><!-- generated --><svg id="root" aria-roledescription="quadrantChart"/>"#,
        );

        assert_eq!(metadata.svg_id(), Some("root"));
        assert_eq!(metadata.diagram_type(), Some("quadrantChart"));
        assert_eq!(metadata.family_kind(), None);
    }

    #[test]
    fn diagram_type_string_does_not_forge_typed_family_context() {
        let metadata = SvgPostprocessMetadata::new().with_diagram_type("quadrantChart");

        assert_eq!(metadata.diagram_type(), Some("quadrantChart"));
        assert_eq!(metadata.family_kind(), None);
    }
}
