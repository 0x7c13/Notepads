//! Diagram family facts for the pinned Mermaid baseline.
//!
//! This module owns release-facing Mermaid family facts and projects them into detector,
//! parser, render-model, and metadata surfaces. Identity, detection, headers, and configuration
//! facts remain available when an implementation is absent. Parser and typed-model lists are
//! separate projections of the callbacks compiled into this crate.

use crate::detect::DetectorFn;
use crate::diagram::{
    BuiltInDiagramSemanticParser, BuiltInRenderSemanticParser, RenderSemanticModel,
    RenderSemanticParseOutput,
};
use crate::{
    DiagramWarningFact, EditorFamilySemantics, EditorSemanticFacts, EditorSemanticKind, Error,
    ManagedSemanticJson, OperationControl, OperationControlResult, ParseMetadata, Result,
};
use serde::Serialize;
use serde_json::Value;
use std::sync::OnceLock;

pub(crate) type CombinedSemanticParser = fn(
    code: &str,
    meta: &ParseMetadata,
    control: &OperationControl,
) -> OperationControlResult<CombinedSemanticParse>;

pub(crate) type WarningSemanticParser =
    fn(code: &str, meta: &ParseMetadata) -> Result<WarningSemanticParse>;

/// Built-in compatibility JSON paired with the typed warnings that produced its warning field.
pub(crate) struct WarningSemanticParse {
    model: ManagedSemanticJson,
    warning_facts: Vec<DiagramWarningFact>,
}

#[allow(
    dead_code,
    reason = "Warning construction is used only by warning-producing diagram families."
)]
impl WarningSemanticParse {
    pub(crate) fn new(model: Value, warning_facts: Vec<DiagramWarningFact>) -> Self {
        Self {
            model: model.into(),
            warning_facts,
        }
    }

    pub(crate) fn into_model(self) -> Value {
        self.model.into_unmanaged_value()
    }

    pub(crate) fn into_parts(self) -> (ManagedSemanticJson, Vec<DiagramWarningFact>) {
        (self.model, self.warning_facts)
    }
}

/// Closed result of one family semantic construction.
///
/// A successful construction hands typed warnings directly to the operation snapshot. A failed
/// construction still owns the parser-derived editor facts produced before the error. This
/// prevents callers from invoking a second recovery parser over the same source.
pub(crate) struct CombinedSemanticParse {
    model: Result<ManagedSemanticJson>,
    editor_facts: EditorSemanticFacts,
    warning_facts: Vec<DiagramWarningFact>,
}

/// Closed failure handoff produced after a family has retained its recovery journal.
#[allow(
    dead_code,
    reason = "Shared parser facilities have different consumers in each family selection."
)]
pub(crate) struct CombinedSemanticFailure {
    error: Box<Error>,
    editor_facts: Box<EditorSemanticFacts>,
    recovery_parser: Option<&'static str>,
}

#[allow(
    dead_code,
    reason = "Shared parser facilities have different consumers in each family selection."
)]
impl CombinedSemanticFailure {
    pub(crate) fn new(error: Error, editor_facts: EditorSemanticFacts) -> Self {
        Self {
            error: Box::new(error),
            editor_facts: Box::new(editor_facts),
            recovery_parser: None,
        }
    }

    pub(crate) fn parser_recovery(
        parser: &'static str,
        error: Error,
        editor_facts: EditorSemanticFacts,
    ) -> Self {
        Self {
            error: Box::new(error),
            editor_facts: Box::new(editor_facts),
            recovery_parser: Some(parser),
        }
    }

    pub(crate) fn into_parts(mut self) -> (Error, EditorSemanticFacts) {
        if let Some(parser) = self.recovery_parser.take() {
            let (message, span) = match self.error.as_ref() {
                Error::DiagramParse { diagnostic, .. } => {
                    (diagnostic.message().to_string(), diagnostic.span())
                }
                error => (error.to_string(), None),
            };
            self.editor_facts.mark_recovered_from_parse_error(
                format!("{parser} parser recovered after parse error: {message}"),
                span,
            );
        }
        (*self.error, *self.editor_facts)
    }

    pub(crate) fn into_error(self) -> Error {
        *self.error
    }
}

#[cfg(test)]
mod combined_semantic_failure_tests {
    use super::*;
    use crate::{EditorSemanticDiagnosticKind, ParseDiagnosticSpanKind, SourceSpan};

    #[test]
    fn parser_recovery_preserves_the_strict_error_and_exact_editor_diagnostic() {
        let span = SourceSpan::new(17, 29);
        let failure = CombinedSemanticFailure::parser_recovery(
            "quadrant chart",
            Error::diagram_parse_exact("quadrantChart", "expected point coordinates", span),
            EditorSemanticFacts::new(),
        );

        let (error, facts) = failure.into_parts();
        let Error::DiagramParse {
            diagram_type,
            diagnostic,
        } = error
        else {
            panic!("expected diagram parse error");
        };
        assert_eq!(diagram_type, "quadrantChart");
        assert_eq!(diagnostic.message(), "expected point coordinates");
        assert_eq!(diagnostic.span(), Some(span));
        assert_eq!(diagnostic.span_kind(), ParseDiagnosticSpanKind::Exact);
        assert_eq!(facts.diagnostics.len(), 1);
        assert_eq!(
            facts.diagnostics[0].message,
            "quadrant chart parser recovered after parse error: expected point coordinates"
        );
        assert_eq!(facts.diagnostics[0].span, Some(span));
        assert_eq!(
            facts.diagnostics[0].kind,
            EditorSemanticDiagnosticKind::ParserRecovery
        );
    }
}

impl CombinedSemanticParse {
    #[allow(
        dead_code,
        reason = "Shared parser facilities have different consumers in each family selection."
    )]
    pub(crate) fn from_construction<S, F>(
        construction: std::result::Result<S, F>,
        success: impl FnOnce(S) -> (Result<Value>, EditorSemanticFacts),
        failure: impl FnOnce(F) -> (Error, EditorSemanticFacts),
    ) -> Self {
        match construction {
            Ok(source) => {
                let (model, editor_facts) = success(source);
                Self {
                    model: model.map(ManagedSemanticJson::from),
                    editor_facts,
                    warning_facts: Vec::new(),
                }
            }
            Err(parse_failure) => {
                let (error, editor_facts) = failure(parse_failure);
                Self {
                    model: Err(error),
                    editor_facts,
                    warning_facts: Vec::new(),
                }
            }
        }
    }

    #[allow(
        dead_code,
        reason = "Shared parser facilities have different consumers in each family selection."
    )]
    pub(crate) fn from_construction_with_warning_facts<S, F>(
        construction: std::result::Result<S, F>,
        success: impl FnOnce(S) -> (Result<Value>, EditorSemanticFacts, Vec<DiagramWarningFact>),
        failure: impl FnOnce(F) -> (Error, EditorSemanticFacts),
    ) -> Self {
        match construction {
            Ok(source) => {
                let (model, editor_facts, warning_facts) = success(source);
                Self {
                    model: model.map(ManagedSemanticJson::from),
                    editor_facts,
                    warning_facts,
                }
            }
            Err(parse_failure) => {
                let (error, editor_facts) = failure(parse_failure);
                Self {
                    model: Err(error),
                    editor_facts,
                    warning_facts: Vec::new(),
                }
            }
        }
    }

    pub(crate) fn into_parts(
        self,
    ) -> (
        Result<ManagedSemanticJson>,
        EditorSemanticFacts,
        Vec<DiagramWarningFact>,
    ) {
        (self.model, self.editor_facts, self.warning_facts)
    }
}

#[cfg(test)]
#[allow(
    dead_code,
    reason = "Test helpers serve different optional family suites."
)]
pub(crate) mod test_support {
    use super::{CombinedSemanticParse, CombinedSemanticParser};
    use crate::{
        EditorSemanticFacts, Error, OperationControl, OperationControlResult, ParseMetadata,
    };
    use serde_json::Value;

    pub(crate) fn into_result(
        parsed: OperationControlResult<CombinedSemanticParse>,
    ) -> std::result::Result<(Value, EditorSemanticFacts), Error> {
        let (model, editor_facts, _) = parsed
            .expect("a private parse control cannot be cancelled")
            .into_parts();
        model.map(|model| (model.into_unmanaged_value(), editor_facts))
    }

    pub(crate) fn editor_facts(
        parser: CombinedSemanticParser,
        code: &str,
        meta: &ParseMetadata,
    ) -> EditorSemanticFacts {
        parser(code, meta, &OperationControl::new())
            .expect("a private parse control cannot be cancelled")
            .into_parts()
            .1
    }
}

#[derive(Clone, Copy)]
pub(crate) struct DetectorFact {
    pub(crate) id: &'static str,
    pub(crate) detector: DetectorFn,
}

#[derive(Clone, Copy)]
pub(crate) struct SemanticParserFact {
    pub(crate) id: &'static str,
    pub(crate) parser: BuiltInDiagramSemanticParser,
}

#[derive(Clone, Copy)]
pub(crate) struct RenderParserFact {
    pub(crate) id: &'static str,
    pub(crate) metadata_id: Option<&'static str>,
    pub(crate) model_kind: &'static str,
    pub(crate) parser: BuiltInRenderSemanticParser,
}

#[derive(Clone, Copy)]
pub(crate) struct CombinedParserFact {
    pub(crate) id: &'static str,
    pub(crate) parser: CombinedSemanticParser,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DiagramHeaderFact {
    /// Mermaid diagram type id owned by this catalog entry.
    pub diagram_type: &'static str,
    /// Header text suggested to the user.
    pub label: &'static str,
    /// Short description shown in completion details.
    pub detail: &'static str,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[non_exhaustive]
pub struct DiagramFamilyCapability {
    /// Mermaid diagram type id used by the pinned detector and parser registries.
    pub diagram_type: &'static str,
    /// Logical diagram family. This does not change when a family reuses another render model.
    pub logical_family_kind: &'static str,
    /// Public metadata identity, retained even when its implementation is unavailable.
    pub metadata_id: Option<&'static str>,
    /// Known typed render-model kind, independent of whether its parser is compiled.
    pub render_model_kind: Option<&'static str>,
    /// Whether this id participates in automatic detection.
    pub has_detector: bool,
    /// Whether this build includes a semantic parser for this diagram type.
    pub has_semantic_parser: bool,
    /// Whether this build includes parser-backed editor facts.
    pub has_editor_parser: bool,
    /// Whether JSON and editor facts share one combined semantic construction.
    pub has_combined_parser: bool,
    /// Whether this build includes a typed render-model parser for this diagram type.
    pub has_render_parser: bool,
    /// Whether this known id has an authoring header, without promising parser availability.
    pub has_header: bool,
    /// Mermaid configuration namespace associated with this id.
    pub config_namespace: Option<&'static str>,
}

/// Public Cargo selector for one logical family, regardless of this build's availability.
///
/// Aliases share the selector of their owning family. Infrastructure models have no selector.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[non_exhaustive]
pub struct DiagramFamilySelector {
    /// Logical identity shared by all parser aliases in this family.
    pub logical_family_kind: &'static str,
    /// Positive Cargo feature selecting this family's implementation.
    pub feature: &'static str,
}

/// Canonical public identity for one compiled concrete built-in typed render family.
///
/// Parser aliases that share a render model contribute exactly one entry. Error and custom JSON
/// models are infrastructure variants rather than concrete Mermaid families and are excluded.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[non_exhaustive]
pub struct BuiltInTypedRenderFamily {
    pub diagram_type: &'static str,
    pub render_model_kind: &'static str,
}

/// Closed, catalog-owned identity for one logical Mermaid diagram family.
///
/// The inner value is private so editor facts cannot invent family ownership from an arbitrary
/// string. Instances only come from the admitted family catalog and are cheap to copy into
/// lexical provenance.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize)]
#[serde(transparent)]
pub struct DiagramFamilyId(&'static str);

impl DiagramFamilyId {
    /// Logical identity shared by all admitted Flowchart parser variants.
    pub const FLOWCHART: Self = Self("flowchart");

    pub fn as_str(self) -> &'static str {
        self.0
    }
}

struct FamilyCatalogProjection {
    detector_facts: Vec<DetectorFact>,
    semantic_parser_facts: Vec<SemanticParserFact>,
    render_parser_facts: Vec<RenderParserFact>,
    combined_parser_facts: Vec<CombinedParserFact>,
    supported_diagram_metadata_ids: Vec<&'static str>,
    diagram_header_facts: Vec<DiagramHeaderFact>,
    diagram_family_capabilities: Vec<DiagramFamilyCapability>,
    built_in_typed_render_families: Vec<BuiltInTypedRenderFamily>,
}

impl FamilyCatalogProjection {
    fn build<'a>(
        variants: impl IntoIterator<Item = (&'a DiagramFamilyDefinition, &'a FamilyVariantDefinition)>,
    ) -> Self {
        let mut detector_facts = Vec::<(u16, DetectorFact)>::new();
        let mut semantic_parser_facts = Vec::<(u16, SemanticParserFact)>::new();
        let mut render_parser_facts = Vec::<(u16, RenderParserFact)>::new();
        let mut combined_parser_facts = Vec::<(u16, CombinedParserFact)>::new();
        let mut metadata_facts = Vec::<(u16, &'static str)>::new();
        let mut diagram_header_facts = Vec::<(u16, DiagramHeaderFact)>::new();
        let mut diagram_family_capabilities = Vec::<(u16, DiagramFamilyCapability)>::new();

        for (family, variant) in variants {
            let identity = &variant.identity;
            let implementation = &variant.implementation;
            if let Some(ordered) = identity.detector {
                detector_facts.push((
                    ordered.order,
                    DetectorFact {
                        id: identity.id,
                        detector: ordered.value,
                    },
                ));
            }
            if let Some(ordered) = implementation.semantic {
                semantic_parser_facts.push((
                    ordered.order,
                    SemanticParserFact {
                        id: identity.id,
                        parser: ordered.value,
                    },
                ));
            }
            if let Some(ordered) = implementation.typed_render {
                render_parser_facts.push((
                    ordered.order,
                    RenderParserFact {
                        id: identity.id,
                        metadata_id: identity.metadata.map(|metadata| metadata.id),
                        model_kind: identity
                            .render_model_kind
                            .expect("typed render variants declare their model kind"),
                        parser: ordered.value,
                    },
                ));
            }
            if let Some(ordered) = implementation.combined {
                combined_parser_facts.push((
                    ordered.order,
                    CombinedParserFact {
                        id: identity.id,
                        parser: ordered.value,
                    },
                ));
            }
            if let Some((order, id)) = identity
                .metadata
                .and_then(|metadata| metadata.order.map(|order| (order, metadata.id)))
            {
                metadata_facts.push((order, id));
            }
            for header in identity.headers {
                diagram_header_facts.push((
                    header.order,
                    DiagramHeaderFact {
                        diagram_type: identity.id,
                        label: header.label,
                        detail: header.detail,
                    },
                ));
            }
            diagram_family_capabilities.push((
                identity.catalog_order,
                DiagramFamilyCapability {
                    diagram_type: identity.id,
                    logical_family_kind: family.logical_kind,
                    metadata_id: identity.metadata.map(|metadata| metadata.id),
                    render_model_kind: identity.render_model_kind,
                    has_detector: identity.detector.is_some(),
                    has_semantic_parser: implementation.semantic.is_some(),
                    has_editor_parser: implementation.combined.is_some(),
                    has_combined_parser: implementation.combined.is_some(),
                    has_render_parser: implementation.typed_render.is_some(),
                    has_header: !identity.headers.is_empty(),
                    config_namespace: family.config.map(|config| config.namespace),
                },
            ));
        }

        let detector_facts = ordered_values(detector_facts);
        let semantic_parser_facts = ordered_values(semantic_parser_facts);
        let render_parser_facts = ordered_values(render_parser_facts);
        let combined_parser_facts = ordered_values(combined_parser_facts);
        let metadata_facts = ordered_values(metadata_facts);
        let diagram_header_facts = ordered_values(diagram_header_facts);
        let diagram_family_capabilities = ordered_values(diagram_family_capabilities);
        let supported_diagram_metadata_ids = metadata_facts
            .iter()
            .copied()
            .filter(|metadata_id| {
                diagram_family_capabilities
                    .iter()
                    .any(|fact| fact.metadata_id == Some(*metadata_id) && fact.has_semantic_parser)
            })
            .collect::<Vec<&'static str>>();
        let mut built_in_typed_render_families = Vec::<BuiltInTypedRenderFamily>::new();
        for metadata_id in metadata_facts {
            let Some(fact) = render_parser_facts
                .iter()
                .find(|fact| fact.metadata_id == Some(metadata_id))
            else {
                continue;
            };
            if !built_in_typed_render_families
                .iter()
                .any(|family| family.render_model_kind == fact.model_kind)
            {
                built_in_typed_render_families.push(BuiltInTypedRenderFamily {
                    diagram_type: metadata_id,
                    render_model_kind: fact.model_kind,
                });
            }
        }

        Self {
            detector_facts,
            semantic_parser_facts,
            render_parser_facts,
            combined_parser_facts,
            supported_diagram_metadata_ids,
            diagram_header_facts,
            diagram_family_capabilities,
            built_in_typed_render_families,
        }
    }
}

fn ordered_values<T>(mut values: Vec<(u16, T)>) -> Vec<T> {
    values.sort_by_key(|(order, _)| *order);
    values.into_iter().map(|(_, value)| value).collect()
}

fn family_catalog_projection() -> &'static FamilyCatalogProjection {
    static CATALOG: OnceLock<FamilyCatalogProjection> = OnceLock::new();
    CATALOG.get_or_init(|| FamilyCatalogProjection::build(variants()))
}

pub(crate) fn detector_facts() -> &'static [DetectorFact] {
    family_catalog_projection().detector_facts.as_slice()
}

pub(crate) fn semantic_parser_facts() -> &'static [SemanticParserFact] {
    family_catalog_projection().semantic_parser_facts.as_slice()
}

pub(crate) fn render_parser_facts() -> &'static [RenderParserFact] {
    family_catalog_projection().render_parser_facts.as_slice()
}

pub(crate) fn combined_parser_facts() -> &'static [CombinedParserFact] {
    family_catalog_projection().combined_parser_facts.as_slice()
}

pub(crate) fn combined_parser(diagram_type: &str) -> Option<CombinedSemanticParser> {
    combined_parser_facts()
        .iter()
        .find_map(|fact| (fact.id == diagram_type).then_some(fact.parser))
}

pub(crate) fn warning_semantic_parser(diagram_type: &str) -> Option<WarningSemanticParser> {
    find_variant(diagram_type).and_then(|(_, variant)| variant.implementation.warning_semantic)
}

pub(crate) fn supported_diagram_metadata_ids() -> &'static [&'static str] {
    family_catalog_projection()
        .supported_diagram_metadata_ids
        .as_slice()
}

pub(crate) fn diagram_header_facts() -> &'static [DiagramHeaderFact] {
    family_catalog_projection().diagram_header_facts.as_slice()
}

pub(crate) fn diagram_family_capabilities() -> &'static [DiagramFamilyCapability] {
    family_catalog_projection()
        .diagram_family_capabilities
        .as_slice()
}

pub(crate) fn diagram_family_selectors() -> &'static [DiagramFamilySelector] {
    static SELECTORS: OnceLock<Vec<DiagramFamilySelector>> = OnceLock::new();
    SELECTORS.get_or_init(|| {
        FAMILY_CATALOG
            .iter()
            .filter_map(|family| {
                family.selector.map(|feature| DiagramFamilySelector {
                    logical_family_kind: family.logical_kind,
                    feature,
                })
            })
            .collect()
    })
}

pub(crate) fn built_in_typed_render_families() -> &'static [BuiltInTypedRenderFamily] {
    family_catalog_projection()
        .built_in_typed_render_families
        .as_slice()
}

pub(crate) fn is_builtin_diagram_type(diagram_type: &str) -> bool {
    find_variant(diagram_type).is_some()
}

pub(crate) fn render_model_kind_supports_diagram_type(
    model_kind: &'static str,
    diagram_type: &str,
) -> bool {
    render_parser_facts()
        .iter()
        .any(|fact| fact.id == diagram_type && fact.model_kind == model_kind)
}

pub fn diagram_type_family_kind(diagram_type: &str) -> Option<&'static str> {
    diagram_type_family_id(diagram_type).map(DiagramFamilyId::as_str)
}

pub fn diagram_type_metadata_id(diagram_type: &str) -> Option<&'static str> {
    find_variant(diagram_type)
        .and_then(|(_, variant)| variant.identity.metadata.map(|metadata| metadata.id))
}

pub fn diagram_type_family_id(diagram_type: &str) -> Option<DiagramFamilyId> {
    find_variant(diagram_type).map(|(family, _)| DiagramFamilyId(family.logical_kind))
}

pub(crate) fn diagram_type_editor_semantics(diagram_type: &str) -> Option<EditorFamilySemantics> {
    find_variant(diagram_type).map(|(family, _)| family.editor_semantics)
}

pub fn diagram_type_render_model_kind(diagram_type: &str) -> Option<&'static str> {
    find_variant(diagram_type).and_then(|(_, variant)| variant.identity.render_model_kind)
}

macro_rules! render_parser {
    ($fn_name:ident, $parser:path, $variant:path) => {
        fn $fn_name(
            code: &str,
            meta: &ParseMetadata,
            control: &OperationControl,
        ) -> OperationControlResult<Result<RenderSemanticParseOutput>> {
            control.checkpoint()?;
            let result = $parser(code, meta)
                .map($variant)
                .map(RenderSemanticParseOutput::new);
            control.checkpoint()?;
            Ok(result)
        }
    };
}

#[allow(
    unused_macros,
    reason = "No controlled family adapter is emitted in infrastructure-only builds."
)]
macro_rules! render_parser_controlled {
    ($fn_name:ident, $parser:path, $variant:path) => {
        fn $fn_name(
            code: &str,
            meta: &ParseMetadata,
            control: &OperationControl,
        ) -> OperationControlResult<Result<RenderSemanticParseOutput>> {
            control.checkpoint()?;
            let result = $parser(code, meta, control)?
                .map($variant)
                .map(RenderSemanticParseOutput::new);
            control.checkpoint()?;
            Ok(result)
        }
    };
}

render_parser!(
    render_error,
    crate::diagrams::error_diagram::parse_error_model_for_render,
    RenderSemanticModel::Error
);
#[cfg(feature = "diagram-mindmap")]
render_parser_controlled!(
    render_mindmap,
    crate::diagrams::mindmap::parse_mindmap_model_for_render_controlled,
    RenderSemanticModel::Mindmap
);
#[cfg(feature = "diagram-state")]
render_parser_controlled!(
    render_state,
    crate::diagrams::state::parse_state_model_for_render_controlled,
    RenderSemanticModel::State
);
#[cfg(feature = "diagram-zenuml")]
render_parser_controlled!(
    render_zenuml,
    crate::diagrams::zenuml::parse_zenuml_model_for_render_controlled,
    RenderSemanticModel::Zenuml
);
#[cfg(feature = "diagram-sequence")]
render_parser_controlled!(
    render_sequence,
    crate::diagrams::sequence::parse_sequence_model_for_render_controlled,
    RenderSemanticModel::Sequence
);
#[cfg(any(feature = "diagram-flowchart", feature = "diagram-swimlane"))]
fn render_flowchart(
    code: &str,
    meta: &ParseMetadata,
    control: &OperationControl,
) -> OperationControlResult<Result<RenderSemanticParseOutput>> {
    let result = crate::diagrams::flowchart::parse_flowchart_model_with_render_context_controlled(
        code, meta, control,
    )?;
    Ok(result
        .map(|(model, label_sources)| RenderSemanticParseOutput::flowchart(model, label_sources)))
}
#[cfg(feature = "diagram-class")]
render_parser_controlled!(
    render_class,
    crate::diagrams::class::parse_class_typed_controlled,
    RenderSemanticModel::Class
);
#[cfg(feature = "diagram-c4")]
render_parser_controlled!(
    render_c4,
    crate::diagrams::c4::parse_c4_model_for_render_controlled,
    RenderSemanticModel::C4
);
#[cfg(feature = "diagram-cynefin")]
render_parser_controlled!(
    render_cynefin,
    crate::diagrams::cynefin::parse_cynefin_model_for_render_controlled,
    RenderSemanticModel::Cynefin
);
#[cfg(feature = "diagram-railroad")]
render_parser_controlled!(
    render_railroad,
    crate::diagrams::railroad::parse_railroad_model_for_render_controlled,
    RenderSemanticModel::Railroad
);
#[cfg(feature = "diagram-railroad")]
render_parser_controlled!(
    render_railroad_ebnf,
    crate::diagrams::railroad::parse_railroad_ebnf_model_for_render_controlled,
    RenderSemanticModel::Railroad
);
#[cfg(feature = "diagram-railroad")]
render_parser_controlled!(
    render_railroad_abnf,
    crate::diagrams::railroad::parse_railroad_abnf_model_for_render_controlled,
    RenderSemanticModel::Railroad
);
#[cfg(feature = "diagram-railroad")]
render_parser_controlled!(
    render_railroad_peg,
    crate::diagrams::railroad::parse_railroad_peg_model_for_render_controlled,
    RenderSemanticModel::Railroad
);
#[cfg(feature = "diagram-architecture")]
render_parser_controlled!(
    render_architecture,
    crate::diagrams::architecture::parse_architecture_model_for_render_controlled,
    RenderSemanticModel::Architecture
);
#[cfg(feature = "diagram-kanban")]
render_parser_controlled!(
    render_kanban,
    crate::diagrams::kanban::parse_kanban_model_for_render_controlled,
    RenderSemanticModel::Kanban
);
#[cfg(feature = "diagram-gantt")]
render_parser_controlled!(
    render_gantt,
    crate::diagrams::gantt::parse_gantt_model_for_render_controlled,
    RenderSemanticModel::Gantt
);
#[cfg(feature = "diagram-pie")]
render_parser_controlled!(
    render_pie,
    crate::diagrams::pie::parse_pie_model_for_render_controlled,
    RenderSemanticModel::Pie
);
#[cfg(feature = "diagram-packet")]
render_parser_controlled!(
    render_packet,
    crate::diagrams::packet::parse_packet_model_for_render_controlled,
    RenderSemanticModel::Packet
);
#[cfg(feature = "diagram-timeline")]
render_parser_controlled!(
    render_timeline,
    crate::diagrams::timeline::parse_timeline_model_for_render_controlled,
    RenderSemanticModel::Timeline
);
#[cfg(feature = "diagram-journey")]
render_parser_controlled!(
    render_journey,
    crate::diagrams::journey::parse_journey_model_for_render_controlled,
    RenderSemanticModel::Journey
);
#[cfg(feature = "diagram-requirement")]
render_parser_controlled!(
    render_requirement,
    crate::diagrams::requirement::parse_requirement_model_for_render_controlled,
    RenderSemanticModel::Requirement
);
#[cfg(feature = "diagram-sankey")]
render_parser_controlled!(
    render_sankey,
    crate::diagrams::sankey::parse_sankey_model_for_render_controlled,
    RenderSemanticModel::Sankey
);
#[cfg(feature = "diagram-radar")]
render_parser_controlled!(
    render_radar,
    crate::diagrams::radar::parse_radar_model_for_render_controlled,
    RenderSemanticModel::Radar
);
#[cfg(feature = "diagram-info")]
render_parser_controlled!(
    render_info,
    crate::diagrams::info::parse_info_model_for_render_controlled,
    RenderSemanticModel::Info
);
#[cfg(feature = "diagram-treemap")]
render_parser_controlled!(
    render_treemap,
    crate::diagrams::treemap::parse_treemap_model_for_render_controlled,
    RenderSemanticModel::Treemap
);
#[cfg(feature = "diagram-block")]
render_parser_controlled!(
    render_block,
    crate::diagrams::block::parse_block_model_for_render_controlled,
    RenderSemanticModel::Block
);
#[cfg(feature = "diagram-er")]
render_parser_controlled!(
    render_er,
    crate::diagrams::er::parse_er_model_for_render_controlled,
    RenderSemanticModel::Er
);
#[cfg(feature = "diagram-quadrant-chart")]
render_parser_controlled!(
    render_quadrant_chart,
    crate::diagrams::quadrant_chart::parse_quadrant_chart_model_for_render_controlled,
    RenderSemanticModel::QuadrantChart
);
#[cfg(feature = "diagram-xychart")]
render_parser_controlled!(
    render_xychart,
    crate::diagrams::xychart::parse_xychart_model_for_render_controlled,
    RenderSemanticModel::XyChart
);
#[cfg(feature = "diagram-git-graph")]
render_parser_controlled!(
    render_git_graph,
    crate::diagrams::git_graph::parse_git_graph_model_for_render_controlled,
    RenderSemanticModel::GitGraph
);
#[cfg(feature = "diagram-tree-view")]
render_parser_controlled!(
    render_tree_view,
    crate::diagrams::tree_view::parse_tree_view_model_for_render_controlled,
    RenderSemanticModel::TreeView
);
#[cfg(feature = "diagram-ishikawa")]
render_parser_controlled!(
    render_ishikawa,
    crate::diagrams::ishikawa::parse_ishikawa_model_for_render_controlled,
    RenderSemanticModel::Ishikawa
);
#[cfg(feature = "diagram-event-modeling")]
render_parser_controlled!(
    render_eventmodeling,
    crate::diagrams::eventmodeling::parse_eventmodeling_model_for_render_controlled,
    RenderSemanticModel::EventModeling
);
#[cfg(feature = "diagram-venn")]
render_parser_controlled!(
    render_venn,
    crate::diagrams::venn::parse_venn_model_for_render_controlled,
    RenderSemanticModel::Venn
);
#[cfg(feature = "diagram-wardley")]
render_parser_controlled!(
    render_wardley,
    crate::diagrams::wardley::parse_wardley_model_for_render_controlled,
    RenderSemanticModel::Wardley
);
#[cfg(feature = "diagram-usecase")]
render_parser_controlled!(
    render_usecase,
    crate::diagrams::usecase::parse_usecase_model_for_render_controlled,
    RenderSemanticModel::Usecase
);
#[cfg(feature = "diagram-agentflow")]
render_parser_controlled!(
    render_agentflow,
    crate::diagrams::agentflow::parse_agentflow_model_for_render_controlled,
    RenderSemanticModel::Agentflow
);

#[derive(Clone, Copy)]
struct Ordered<T> {
    order: u16,
    value: T,
}

const fn ordered<T>(order: u16, value: T) -> Ordered<T> {
    Ordered { order, value }
}

#[derive(Clone, Copy)]
struct MetadataDefinition {
    id: &'static str,
    order: Option<u16>,
}

const fn metadata(id: &'static str, order: Option<u16>) -> MetadataDefinition {
    MetadataDefinition { id, order }
}

#[derive(Clone, Copy)]
struct HeaderDefinition {
    order: u16,
    label: &'static str,
    detail: &'static str,
}

const fn header(order: u16, label: &'static str, detail: &'static str) -> HeaderDefinition {
    HeaderDefinition {
        order,
        label,
        detail,
    }
}

#[derive(Clone, Copy)]
struct FamilyVariantDefinition {
    identity: FamilyVariantIdentity,
    implementation: FamilyImplementation,
}

/// Lightweight pinned facts survive omission of their parser and model implementations.
#[derive(Clone, Copy)]
struct FamilyVariantIdentity {
    id: &'static str,
    catalog_order: u16,
    detector: Option<Ordered<DetectorFn>>,
    render_model_kind: Option<&'static str>,
    metadata: Option<MetadataDefinition>,
    headers: &'static [HeaderDefinition],
    frontmatter_alias_order: Option<u16>,
}

/// Compiled entry points, bound beside the identity facts in the same catalog.
///
/// An absent callback makes only its executable projection unavailable; it never removes the
/// known identity or releases its built-in identity reservation.
#[derive(Clone, Copy, Default)]
struct FamilyImplementation {
    semantic: Option<Ordered<BuiltInDiagramSemanticParser>>,
    warning_semantic: Option<WarningSemanticParser>,
    combined: Option<Ordered<CombinedSemanticParser>>,
    typed_render: Option<Ordered<BuiltInRenderSemanticParser>>,
}

#[derive(Clone, Copy)]
struct FamilyConfigDefinition {
    namespace: &'static str,
    frontmatter_order: u16,
}

#[derive(Clone, Copy)]
struct DiagramFamilyDefinition {
    logical_kind: &'static str,
    selector: Option<&'static str>,
    editor_semantics: EditorFamilySemantics,
    config: Option<FamilyConfigDefinition>,
    variants: &'static [FamilyVariantDefinition],
}

const GENERIC_EDITOR_SEMANTICS: EditorFamilySemantics =
    EditorFamilySemantics::new(EditorSemanticKind::Variable);
const FLOWCHART_EDITOR_SEMANTICS: EditorFamilySemantics =
    EditorFamilySemantics::new(EditorSemanticKind::Module);
const SWIMLANE_EDITOR_SEMANTICS: EditorFamilySemantics =
    EditorFamilySemantics::new(EditorSemanticKind::Variable);
const MINDMAP_EDITOR_SEMANTICS: EditorFamilySemantics =
    EditorFamilySemantics::new(EditorSemanticKind::Namespace);
const SEQUENCE_EDITOR_SEMANTICS: EditorFamilySemantics =
    EditorFamilySemantics::new(EditorSemanticKind::Event);
const CLASS_EDITOR_SEMANTICS: EditorFamilySemantics =
    EditorFamilySemantics::new(EditorSemanticKind::Class);
const ER_EDITOR_SEMANTICS: EditorFamilySemantics =
    EditorFamilySemantics::new(EditorSemanticKind::Struct);
const CARDINAL_EDITOR_SEMANTICS: EditorFamilySemantics =
    EditorFamilySemantics::new(EditorSemanticKind::Variable);
const STATE_EDITOR_SEMANTICS: EditorFamilySemantics =
    EditorFamilySemantics::new(EditorSemanticKind::Class);
const BLOCK_EDITOR_SEMANTICS: EditorFamilySemantics =
    EditorFamilySemantics::new(EditorSemanticKind::Object);

macro_rules! variant {
    (
        $(feature: $feature:literal,)?
        id: $id:literal,
        catalog_order: $catalog_order:literal,
        detector: $detector:expr,
        semantic: $semantic:expr,
        $(warning_semantic: $warning_semantic:expr,)?
        combined: $combined:expr,
        typed: $typed:expr,
        render_kind: $render_kind:expr,
        metadata: $metadata:expr,
        headers: $headers:expr,
        config_alias_order: $config_alias_order:expr $(,)?
    ) => {
        FamilyVariantDefinition {
            identity: FamilyVariantIdentity {
                id: $id,
                catalog_order: $catalog_order,
                detector: $detector,
                render_model_kind: $render_kind,
                metadata: $metadata,
                headers: $headers,
                frontmatter_alias_order: $config_alias_order,
            },
            implementation: {
                $(#[cfg(feature = $feature)])?
                {
                    FamilyImplementation {
                        semantic: $semantic,
                        warning_semantic: variant!(@warning_semantic $($warning_semantic)?),
                        combined: $combined,
                        typed_render: $typed,
                    }
                }
                $(#[cfg(not(feature = $feature))]
                {
                    FamilyImplementation {
                        semantic: None,
                        warning_semantic: None,
                        combined: None,
                        typed_render: None,
                    }
                })?
            },
        }
    };
    (@warning_semantic) => {
        None
    };
    (@warning_semantic $warning_semantic:expr) => {
        Some($warning_semantic)
    };
}

fn variants() -> impl Iterator<
    Item = (
        &'static DiagramFamilyDefinition,
        &'static FamilyVariantDefinition,
    ),
> {
    FAMILY_CATALOG
        .iter()
        .flat_map(|family| family.variants.iter().map(move |variant| (family, variant)))
}

fn find_variant(
    diagram_type: &str,
) -> Option<(
    &'static DiagramFamilyDefinition,
    &'static FamilyVariantDefinition,
)> {
    FAMILY_CATALOG.iter().find_map(|family| {
        family
            .variants
            .iter()
            .find(|variant| variant.identity.id == diagram_type)
            .map(|variant| (family, variant))
    })
}

#[derive(Clone, Copy)]
pub(crate) struct FrontmatterConfigAliasFact {
    pub(crate) source: &'static str,
    pub(crate) namespace: &'static str,
}

pub(crate) fn frontmatter_config_aliases() -> &'static [FrontmatterConfigAliasFact] {
    static FACTS: OnceLock<Vec<FrontmatterConfigAliasFact>> = OnceLock::new();
    FACTS
        .get_or_init(|| {
            let mut facts: Vec<_> = FAMILY_CATALOG
                .iter()
                .flat_map(|family| {
                    family.variants.iter().filter_map(move |variant| {
                        variant.identity.frontmatter_alias_order.map(|order| {
                            let namespace = family
                                .config
                                .expect("config aliases require a family namespace")
                                .namespace;
                            (
                                order,
                                FrontmatterConfigAliasFact {
                                    source: variant.identity.id,
                                    namespace,
                                },
                            )
                        })
                    })
                })
                .collect();
            facts.sort_by_key(|(order, _)| *order);
            facts.into_iter().map(|(_, fact)| fact).collect()
        })
        .as_slice()
}

pub(crate) fn frontmatter_config_namespaces() -> &'static [&'static str] {
    static NAMESPACES: OnceLock<Vec<&'static str>> = OnceLock::new();
    NAMESPACES
        .get_or_init(|| {
            let mut namespaces: Vec<_> = FAMILY_CATALOG
                .iter()
                .filter_map(|family| {
                    family
                        .config
                        .map(|config| (config.frontmatter_order, config.namespace))
                })
                .collect();
            namespaces.sort_by_key(|(order, _)| *order);
            namespaces
                .into_iter()
                .map(|(_, namespace)| namespace)
                .collect()
        })
        .as_slice()
}

pub(crate) fn config_namespace_for_diagram_type(diagram_type: &str) -> Option<&'static str> {
    find_variant(diagram_type).and_then(|(family, _)| family.config.map(|config| config.namespace))
}

const FLOWCHART_HEADERS: &[HeaderDefinition] = &[
    header(0, "flowchart TD", "flowchart header"),
    header(1, "graph TD", "flowchart alias"),
];
const SEQUENCE_HEADERS: &[HeaderDefinition] = &[header(2, "sequenceDiagram", "sequence header")];
const SWIMLANE_HEADERS: &[HeaderDefinition] = &[header(3, "swimlane-beta", "swimlane header")];
const CLASS_HEADERS: &[HeaderDefinition] = &[
    header(4, "classDiagram", "class header"),
    header(5, "classDiagram-v2", "class header"),
];
const STATE_HEADERS: &[HeaderDefinition] = &[
    header(6, "stateDiagram-v2", "state header"),
    header(7, "stateDiagram", "legacy state header"),
];
const ER_HEADERS: &[HeaderDefinition] = &[header(8, "erDiagram", "er header")];
const GANTT_HEADERS: &[HeaderDefinition] = &[header(9, "gantt", "gantt header")];
const MINDMAP_HEADERS: &[HeaderDefinition] = &[header(10, "mindmap", "mindmap header")];
const INFO_HEADERS: &[HeaderDefinition] = &[header(11, "info", "info header")];
const JOURNEY_HEADERS: &[HeaderDefinition] = &[header(12, "journey", "journey header")];
const TIMELINE_HEADERS: &[HeaderDefinition] = &[header(13, "timeline", "timeline header")];
const GIT_GRAPH_HEADERS: &[HeaderDefinition] = &[header(14, "gitGraph", "git graph header")];
const PIE_HEADERS: &[HeaderDefinition] = &[header(15, "pie", "pie header")];
const REQUIREMENT_HEADERS: &[HeaderDefinition] =
    &[header(16, "requirementDiagram", "requirement header")];
const SANKEY_HEADERS: &[HeaderDefinition] = &[header(17, "sankey", "sankey header")];
const PACKET_HEADERS: &[HeaderDefinition] = &[
    header(18, "packet", "packet header"),
    header(19, "packet-beta", "packet beta header"),
];
const XYCHART_HEADERS: &[HeaderDefinition] = &[
    header(20, "xychart", "xychart header"),
    header(21, "xychart-beta", "xychart beta header"),
];
const TREE_VIEW_HEADERS: &[HeaderDefinition] = &[header(22, "treeView-beta", "tree view header")];
const ISHIKAWA_HEADERS: &[HeaderDefinition] = &[header(23, "ishikawa-beta", "ishikawa header")];
const EVENTMODELING_HEADERS: &[HeaderDefinition] =
    &[header(24, "eventmodeling", "event modeling header")];
const QUADRANT_HEADERS: &[HeaderDefinition] =
    &[header(25, "quadrantChart", "quadrant chart header")];
const VENN_HEADERS: &[HeaderDefinition] = &[header(26, "venn-beta", "venn header")];
const ZENUML_HEADERS: &[HeaderDefinition] = &[header(27, "zenuml", "zenuml header")];
const C4_HEADERS: &[HeaderDefinition] = &[
    header(28, "C4Context", "c4 context header"),
    header(29, "C4Container", "c4 container header"),
    header(30, "C4Component", "c4 component header"),
    header(31, "C4Dynamic", "c4 dynamic header"),
    header(32, "C4Deployment", "c4 deployment header"),
];
const KANBAN_HEADERS: &[HeaderDefinition] = &[header(33, "kanban", "kanban header")];
const ARCHITECTURE_HEADERS: &[HeaderDefinition] =
    &[header(34, "architecture-beta", "architecture header")];
const BLOCK_HEADERS: &[HeaderDefinition] = &[header(35, "block-beta", "block header")];
const RADAR_HEADERS: &[HeaderDefinition] = &[header(36, "radar-beta", "radar header")];
const TREEMAP_HEADERS: &[HeaderDefinition] = &[header(37, "treemap-beta", "treemap header")];
const RAILROAD_HEADERS: &[HeaderDefinition] = &[header(38, "railroad-beta", "railroad header")];
const RAILROAD_EBNF_HEADERS: &[HeaderDefinition] =
    &[header(39, "railroad-ebnf-beta", "railroad ebnf header")];
const RAILROAD_ABNF_HEADERS: &[HeaderDefinition] =
    &[header(40, "railroad-abnf-beta", "railroad abnf header")];
const RAILROAD_PEG_HEADERS: &[HeaderDefinition] =
    &[header(41, "railroad-peg-beta", "railroad peg header")];
const WARDLEY_HEADERS: &[HeaderDefinition] = &[header(42, "wardley-beta", "wardley header")];
const CYNEFIN_HEADERS: &[HeaderDefinition] = &[header(43, "cynefin-beta", "cynefin header")];
const FLOWCHART_ELK_HEADERS: &[HeaderDefinition] =
    &[header(44, "flowchart-elk TD", "elk flowchart header")];
const USECASE_HEADERS: &[HeaderDefinition] =
    &[header(45, "usecase-beta", "usecase diagram header")];
const AGENTFLOW_HEADERS: &[HeaderDefinition] =
    &[header(46, "agentflow-beta", "agentflow diagram header")];

const ERROR_VARIANTS: &[FamilyVariantDefinition] = &[variant! {
    id: "error",
    catalog_order: 0,
    detector: Some(ordered(0, crate::detect::detector_error)),
    semantic: Some(ordered(0, crate::diagrams::error_diagram::parse_error)),
    combined: None,
    typed: Some(ordered(38, render_error)),
    render_kind: Some("error"),
    metadata: None,
    headers: &[],
    config_alias_order: None,
}];

const FLOWCHART_VARIANTS: &[FamilyVariantDefinition] = &[
    variant! {
        feature: "diagram-flowchart",
        id: "flowchart-elk",
        catalog_order: 2,
        detector: Some(ordered(2, crate::detect::detector_flowchart_elk)),
        semantic: Some(ordered(3, crate::diagrams::flowchart::parse_flowchart)),
        warning_semantic: crate::diagrams::flowchart::parse_flowchart_with_warning_facts,
        combined: Some(ordered(2, crate::diagrams::flowchart::parse_flowchart_json_and_editor_facts)),
        typed: Some(ordered(7, render_flowchart)),
        render_kind: Some("flowchart"),
        metadata: Some(metadata("flowchart", None)),
        headers: FLOWCHART_ELK_HEADERS,
        config_alias_order: Some(3),
    },
    variant! {
        feature: "diagram-flowchart",
        id: "flowchart-v2",
        catalog_order: 17,
        detector: Some(ordered(17, crate::detect::detector_flowchart_v2)),
        semantic: Some(ordered(1, crate::diagrams::flowchart::parse_flowchart)),
        warning_semantic: crate::diagrams::flowchart::parse_flowchart_with_warning_facts,
        combined: Some(ordered(0, crate::diagrams::flowchart::parse_flowchart_json_and_editor_facts)),
        typed: Some(ordered(5, render_flowchart)),
        render_kind: Some("flowchart"),
        metadata: Some(metadata("flowchart", Some(7))),
        headers: FLOWCHART_HEADERS,
        config_alias_order: Some(2),
    },
    variant! {
        feature: "diagram-flowchart",
        id: "flowchart",
        catalog_order: 18,
        detector: None,
        semantic: Some(ordered(2, crate::diagrams::flowchart::parse_flowchart)),
        warning_semantic: crate::diagrams::flowchart::parse_flowchart_with_warning_facts,
        combined: Some(ordered(1, crate::diagrams::flowchart::parse_flowchart_json_and_editor_facts)),
        typed: Some(ordered(6, render_flowchart)),
        render_kind: Some("flowchart"),
        metadata: Some(metadata("flowchart", None)),
        headers: &[],
        config_alias_order: None,
    },
];

const SWIMLANE_VARIANTS: &[FamilyVariantDefinition] = &[variant! {
    feature: "diagram-swimlane",
    id: "swimlane",
    catalog_order: 16,
    detector: Some(ordered(16, crate::detect::detector_swimlane)),
    semantic: Some(ordered(9, crate::diagrams::flowchart::parse_flowchart)),
    warning_semantic: crate::diagrams::flowchart::parse_flowchart_with_warning_facts,
    combined: Some(ordered(3, crate::diagrams::flowchart::parse_flowchart_json_and_editor_facts)),
    typed: Some(ordered(39, render_flowchart)),
    render_kind: Some("flowchart"),
    metadata: Some(metadata("swimlane", Some(27))),
    headers: SWIMLANE_HEADERS,
    config_alias_order: None,
}];

const MINDMAP_VARIANTS: &[FamilyVariantDefinition] = &[variant! {
    feature: "diagram-mindmap",
    id: "mindmap",
    catalog_order: 3,
    detector: Some(ordered(3, crate::detect::detector_mindmap)),
    semantic: Some(ordered(22, crate::diagrams::mindmap::parse_mindmap)),
    combined: Some(ordered(5, crate::diagrams::mindmap::parse_mindmap_json_and_editor_facts)),
    typed: Some(ordered(0, render_mindmap)),
    render_kind: Some("mindmap"),
    metadata: Some(metadata("mindmap", Some(14))),
    headers: MINDMAP_HEADERS,
    config_alias_order: None,
}];

const ARCHITECTURE_VARIANTS: &[FamilyVariantDefinition] = &[variant! {
    feature: "diagram-architecture",
    id: "architecture",
    catalog_order: 4,
    detector: Some(ordered(4, crate::detect::detector_architecture)),
    semantic: Some(ordered(27, crate::diagrams::architecture::parse_architecture)),
    combined: Some(ordered(4, crate::diagrams::architecture::parse_architecture_json_and_editor_facts)),
    typed: Some(ordered(16, render_architecture)),
    render_kind: Some("architecture"),
    metadata: Some(metadata("architecture", Some(0))),
    headers: ARCHITECTURE_HEADERS,
    config_alias_order: None,
}];

const ZENUML_VARIANTS: &[FamilyVariantDefinition] = &[variant! {
    feature: "diagram-zenuml",
    id: "zenuml",
    catalog_order: 5,
    detector: Some(ordered(5, crate::detect::detector_zenuml)),
    semantic: Some(ordered(15, crate::diagrams::zenuml::parse_zenuml)),
    combined: Some(ordered(36, crate::diagrams::zenuml::parse_zenuml_json_and_editor_facts)),
    typed: Some(ordered(3, render_zenuml)),
    render_kind: Some("zenuml"),
    metadata: Some(metadata("zenuml", Some(34))),
    headers: ZENUML_HEADERS,
    config_alias_order: None,
}];

const SEQUENCE_VARIANTS: &[FamilyVariantDefinition] = &[variant! {
    feature: "diagram-sequence",
    id: "sequence",
    catalog_order: 15,
    detector: Some(ordered(15, crate::detect::detector_sequence)),
    semantic: Some(ordered(8, crate::diagrams::sequence::parse_sequence)),
    combined: Some(ordered(19, crate::diagrams::sequence::parse_sequence_json_and_editor_facts)),
    typed: Some(ordered(4, render_sequence)),
    render_kind: Some("sequence"),
    metadata: Some(metadata("sequence", Some(25))),
    headers: SEQUENCE_HEADERS,
    config_alias_order: None,
}];

const C4_VARIANTS: &[FamilyVariantDefinition] = &[variant! {
    feature: "diagram-c4",
    id: "c4",
    catalog_order: 6,
    detector: Some(ordered(6, crate::detect::detector_c4)),
    semantic: Some(ordered(6, crate::diagrams::c4::parse_c4)),
    combined: Some(ordered(28, crate::diagrams::c4::parse_c4_json_and_editor_facts)),
    typed: Some(ordered(10, render_c4)),
    render_kind: Some("c4"),
    metadata: Some(metadata("c4", Some(2))),
    headers: C4_HEADERS,
    config_alias_order: None,
}];

const KANBAN_VARIANTS: &[FamilyVariantDefinition] = &[variant! {
    feature: "diagram-kanban",
    id: "kanban",
    catalog_order: 7,
    detector: Some(ordered(7, crate::detect::detector_kanban)),
    semantic: Some(ordered(26, crate::diagrams::kanban::parse_kanban)),
    combined: Some(ordered(29, crate::diagrams::kanban::parse_kanban_json_and_editor_facts)),
    typed: Some(ordered(17, render_kanban)),
    render_kind: Some("kanban"),
    metadata: Some(metadata("kanban", Some(13))),
    headers: KANBAN_HEADERS,
    config_alias_order: None,
}];

const CLASS_VARIANTS: &[FamilyVariantDefinition] = &[
    variant! {
        feature: "diagram-class",
        id: "classDiagram",
        catalog_order: 8,
        detector: Some(ordered(8, crate::detect::detector_class_v2)),
        semantic: Some(ordered(16, crate::diagrams::class::parse_class)),
        combined: Some(ordered(20, crate::diagrams::class::parse_class_json_and_editor_facts)),
        typed: Some(ordered(8, render_class)),
        render_kind: Some("class"),
        metadata: Some(metadata("class", Some(3))),
        headers: CLASS_HEADERS,
        config_alias_order: Some(0),
    },
    variant! {
        feature: "diagram-class",
        id: "class",
        catalog_order: 9,
        detector: None,
        semantic: Some(ordered(17, crate::diagrams::class::parse_class)),
        combined: Some(ordered(21, crate::diagrams::class::parse_class_json_and_editor_facts)),
        typed: Some(ordered(9, render_class)),
        render_kind: Some("class"),
        metadata: Some(metadata("class", None)),
        headers: &[],
        config_alias_order: None,
    },
];

const ER_VARIANTS: &[FamilyVariantDefinition] = &[
    variant! {
        feature: "diagram-er",
        id: "er",
        catalog_order: 10,
        detector: Some(ordered(10, crate::detect::detector_er)),
        semantic: Some(ordered(18, crate::diagrams::er::parse_er)),
        combined: Some(ordered(17, crate::diagrams::er::parse_er_json_and_editor_facts)),
        typed: Some(ordered(29, render_er)),
        render_kind: Some("er"),
        metadata: Some(metadata("er", Some(5))),
        headers: ER_HEADERS,
        config_alias_order: None,
    },
    variant! {
        feature: "diagram-er",
        id: "erDiagram",
        catalog_order: 41,
        detector: None,
        semantic: Some(ordered(19, crate::diagrams::er::parse_er)),
        combined: Some(ordered(18, crate::diagrams::er::parse_er_json_and_editor_facts)),
        typed: Some(ordered(30, render_er)),
        render_kind: Some("er"),
        metadata: Some(metadata("er", None)),
        headers: &[],
        config_alias_order: Some(1),
    },
];

const GANTT_VARIANTS: &[FamilyVariantDefinition] = &[variant! {
    feature: "diagram-gantt",
    id: "gantt",
    catalog_order: 11,
    detector: Some(ordered(11, crate::detect::detector_gantt)),
    semantic: Some(ordered(23, crate::diagrams::gantt::parse_gantt)),
    combined: Some(ordered(30, crate::diagrams::gantt::parse_gantt_json_and_editor_facts)),
    typed: Some(ordered(18, render_gantt)),
    render_kind: Some("gantt"),
    metadata: Some(metadata("gantt", Some(8))),
    headers: GANTT_HEADERS,
    config_alias_order: None,
}];

const INFO_VARIANTS: &[FamilyVariantDefinition] = &[variant! {
    feature: "diagram-info",
    id: "info",
    catalog_order: 12,
    detector: Some(ordered(12, crate::detect::detector_info)),
    semantic: Some(ordered(4, crate::diagrams::info::parse_info)),
    combined: Some(ordered(10, crate::diagrams::info::parse_info_json_and_editor_facts)),
    typed: Some(ordered(26, render_info)),
    render_kind: Some("info"),
    metadata: Some(metadata("info", Some(10))),
    headers: INFO_HEADERS,
    config_alias_order: None,
}];

const PIE_VARIANTS: &[FamilyVariantDefinition] = &[variant! {
    feature: "diagram-pie",
    id: "pie",
    catalog_order: 13,
    detector: Some(ordered(13, crate::detect::detector_pie)),
    semantic: Some(ordered(5, crate::diagrams::pie::parse_pie)),
    combined: Some(ordered(11, crate::diagrams::pie::parse_pie_json_and_editor_facts)),
    typed: Some(ordered(19, render_pie)),
    render_kind: Some("pie"),
    metadata: Some(metadata("pie", Some(16))),
    headers: PIE_HEADERS,
    config_alias_order: None,
}];

const REQUIREMENT_VARIANTS: &[FamilyVariantDefinition] = &[variant! {
    feature: "diagram-requirement",
    id: "requirement",
    catalog_order: 14,
    detector: Some(ordered(14, crate::detect::detector_requirement)),
    semantic: Some(ordered(7, crate::diagrams::requirement::parse_requirement)),
    combined: Some(ordered(31, crate::diagrams::requirement::parse_requirement_json_and_editor_facts)),
    typed: Some(ordered(23, render_requirement)),
    render_kind: Some("requirement"),
    metadata: Some(metadata("requirement", Some(23))),
    headers: REQUIREMENT_HEADERS,
    config_alias_order: None,
}];

const TIMELINE_VARIANTS: &[FamilyVariantDefinition] = &[variant! {
    feature: "diagram-timeline",
    id: "timeline",
    catalog_order: 19,
    detector: Some(ordered(19, crate::detect::detector_timeline)),
    semantic: Some(ordered(24, crate::diagrams::timeline::parse_timeline)),
    combined: Some(ordered(32, crate::diagrams::timeline::parse_timeline_json_and_editor_facts)),
    typed: Some(ordered(21, render_timeline)),
    render_kind: Some("timeline"),
    metadata: Some(metadata("timeline", Some(28))),
    headers: TIMELINE_HEADERS,
    config_alias_order: None,
}];

const GIT_GRAPH_VARIANTS: &[FamilyVariantDefinition] = &[variant! {
    feature: "diagram-git-graph",
    id: "gitGraph",
    catalog_order: 20,
    detector: Some(ordered(20, crate::detect::detector_git_graph)),
    semantic: Some(ordered(29, crate::diagrams::git_graph::parse_git_graph)),
    warning_semantic: crate::diagrams::git_graph::parse_git_graph_with_warning_facts,
    combined: Some(ordered(15, crate::diagrams::git_graph::parse_git_graph_json_and_editor_facts)),
    typed: Some(ordered(33, render_git_graph)),
    render_kind: Some("gitGraph"),
    metadata: Some(metadata("gitgraph", Some(9))),
    headers: GIT_GRAPH_HEADERS,
    config_alias_order: None,
}];

const STATE_VARIANTS: &[FamilyVariantDefinition] = &[
    variant! {
        feature: "diagram-state",
        id: "stateDiagram",
        catalog_order: 21,
        detector: Some(ordered(21, crate::detect::detector_state_v2)),
        semantic: Some(ordered(20, crate::diagrams::state::parse_state)),
        combined: Some(ordered(26, crate::diagrams::state::parse_state_json_and_editor_facts)),
        typed: Some(ordered(1, render_state)),
        render_kind: Some("state"),
        metadata: Some(metadata("state", Some(26))),
        headers: STATE_HEADERS,
        config_alias_order: Some(4),
    },
    variant! {
        feature: "diagram-state",
        id: "state",
        catalog_order: 22,
        detector: None,
        semantic: Some(ordered(21, crate::diagrams::state::parse_state)),
        combined: Some(ordered(27, crate::diagrams::state::parse_state_json_and_editor_facts)),
        typed: Some(ordered(2, render_state)),
        render_kind: Some("state"),
        metadata: Some(metadata("state", None)),
        headers: &[],
        config_alias_order: None,
    },
];

const JOURNEY_VARIANTS: &[FamilyVariantDefinition] = &[variant! {
    feature: "diagram-journey",
    id: "journey",
    catalog_order: 23,
    detector: Some(ordered(23, crate::detect::detector_journey)),
    semantic: Some(ordered(25, crate::diagrams::journey::parse_journey)),
    combined: Some(ordered(33, crate::diagrams::journey::parse_journey_json_and_editor_facts)),
    typed: Some(ordered(22, render_journey)),
    render_kind: Some("journey"),
    metadata: Some(metadata("journey", Some(12))),
    headers: JOURNEY_HEADERS,
    config_alias_order: None,
}];

const QUADRANT_VARIANTS: &[FamilyVariantDefinition] = &[variant! {
    feature: "diagram-quadrant-chart",
    id: "quadrantChart",
    catalog_order: 24,
    detector: Some(ordered(24, crate::detect::detector_quadrant)),
    semantic: Some(ordered(30, crate::diagrams::quadrant_chart::parse_quadrant_chart)),
    combined: Some(ordered(34, crate::diagrams::quadrant_chart::parse_quadrant_chart_json_and_editor_facts)),
    typed: Some(ordered(31, render_quadrant_chart)),
    render_kind: Some("quadrantChart"),
    metadata: Some(metadata("quadrantchart", Some(17))),
    headers: QUADRANT_HEADERS,
    config_alias_order: None,
}];

const SANKEY_VARIANTS: &[FamilyVariantDefinition] = &[variant! {
    feature: "diagram-sankey",
    id: "sankey",
    catalog_order: 25,
    detector: Some(ordered(25, crate::detect::detector_sankey)),
    semantic: Some(ordered(38, crate::diagrams::sankey::parse_sankey)),
    combined: Some(ordered(16, crate::diagrams::sankey::parse_sankey_json_and_editor_facts)),
    typed: Some(ordered(24, render_sankey)),
    render_kind: Some("sankey"),
    metadata: Some(metadata("sankey", Some(24))),
    headers: SANKEY_HEADERS,
    config_alias_order: None,
}];

const PACKET_VARIANTS: &[FamilyVariantDefinition] = &[variant! {
    feature: "diagram-packet",
    id: "packet",
    catalog_order: 26,
    detector: Some(ordered(26, crate::detect::detector_packet)),
    semantic: Some(ordered(31, crate::diagrams::packet::parse_packet)),
    combined: Some(ordered(12, crate::diagrams::packet::parse_packet_json_and_editor_facts)),
    typed: Some(ordered(20, render_packet)),
    render_kind: Some("packet"),
    metadata: Some(metadata("packet", Some(15))),
    headers: PACKET_HEADERS,
    config_alias_order: None,
}];

const XYCHART_VARIANTS: &[FamilyVariantDefinition] = &[variant! {
    feature: "diagram-xychart",
    id: "xychart",
    catalog_order: 27,
    detector: Some(ordered(27, crate::detect::detector_xychart)),
    semantic: Some(ordered(39, crate::diagrams::xychart::parse_xychart)),
    combined: Some(ordered(38, crate::diagrams::xychart::parse_xychart_json_and_editor_facts)),
    typed: Some(ordered(32, render_xychart)),
    render_kind: Some("xychart"),
    metadata: Some(metadata("xychart", Some(33))),
    headers: XYCHART_HEADERS,
    config_alias_order: Some(5),
}];

const BLOCK_VARIANTS: &[FamilyVariantDefinition] = &[variant! {
    feature: "diagram-block",
    id: "block",
    catalog_order: 28,
    detector: Some(ordered(28, crate::detect::detector_block)),
    semantic: Some(ordered(28, crate::diagrams::block::parse_block)),
    warning_semantic: crate::diagrams::block::parse_block_with_warning_facts,
    combined: Some(ordered(37, crate::diagrams::block::parse_block_json_and_editor_facts)),
    typed: Some(ordered(28, render_block)),
    render_kind: Some("block"),
    metadata: Some(metadata("block", Some(1))),
    headers: BLOCK_HEADERS,
    config_alias_order: None,
}];

const EVENTMODELING_VARIANTS: &[FamilyVariantDefinition] = &[variant! {
    feature: "diagram-event-modeling",
    id: "eventmodeling",
    catalog_order: 29,
    detector: Some(ordered(29, crate::detect::detector_eventmodeling)),
    semantic: Some(ordered(35, crate::diagrams::eventmodeling::parse_eventmodeling)),
    combined: Some(ordered(22, crate::diagrams::eventmodeling::parse_eventmodeling_json_and_editor_facts)),
    typed: Some(ordered(36, render_eventmodeling)),
    render_kind: Some("eventmodeling"),
    metadata: Some(metadata("eventmodeling", Some(6))),
    headers: EVENTMODELING_HEADERS,
    config_alias_order: None,
}];

const TREE_VIEW_VARIANTS: &[FamilyVariantDefinition] = &[variant! {
    feature: "diagram-tree-view",
    id: "treeView",
    catalog_order: 30,
    detector: Some(ordered(30, crate::detect::detector_tree_view)),
    semantic: Some(ordered(33, crate::diagrams::tree_view::parse_tree_view)),
    combined: Some(ordered(23, crate::diagrams::tree_view::parse_tree_view_json_and_editor_facts)),
    typed: Some(ordered(34, render_tree_view)),
    render_kind: Some("treeView"),
    metadata: Some(metadata("treeView", Some(29))),
    headers: TREE_VIEW_HEADERS,
    config_alias_order: None,
}];

const RADAR_VARIANTS: &[FamilyVariantDefinition] = &[variant! {
    feature: "diagram-radar",
    id: "radar",
    catalog_order: 31,
    detector: Some(ordered(31, crate::detect::detector_radar)),
    semantic: Some(ordered(32, crate::diagrams::radar::parse_radar)),
    combined: Some(ordered(14, crate::diagrams::radar::parse_radar_json_and_editor_facts)),
    typed: Some(ordered(25, render_radar)),
    render_kind: Some("radar"),
    metadata: Some(metadata("radar", Some(18))),
    headers: RADAR_HEADERS,
    config_alias_order: None,
}];

const ISHIKAWA_VARIANTS: &[FamilyVariantDefinition] = &[variant! {
    feature: "diagram-ishikawa",
    id: "ishikawa",
    catalog_order: 32,
    detector: Some(ordered(32, crate::detect::detector_ishikawa)),
    semantic: Some(ordered(34, crate::diagrams::ishikawa::parse_ishikawa)),
    combined: Some(ordered(24, crate::diagrams::ishikawa::parse_ishikawa_json_and_editor_facts)),
    typed: Some(ordered(35, render_ishikawa)),
    render_kind: Some("ishikawa"),
    metadata: Some(metadata("ishikawa", Some(11))),
    headers: ISHIKAWA_HEADERS,
    config_alias_order: None,
}];

const TREEMAP_VARIANTS: &[FamilyVariantDefinition] = &[variant! {
    feature: "diagram-treemap",
    id: "treemap",
    catalog_order: 33,
    detector: Some(ordered(33, crate::detect::detector_treemap)),
    semantic: Some(ordered(36, crate::diagrams::treemap::parse_treemap)),
    combined: Some(ordered(25, crate::diagrams::treemap::parse_treemap_json_and_editor_facts)),
    typed: Some(ordered(27, render_treemap)),
    render_kind: Some("treemap"),
    metadata: Some(metadata("treemap", Some(30))),
    headers: TREEMAP_HEADERS,
    config_alias_order: None,
}];

const RAILROAD_VARIANTS: &[FamilyVariantDefinition] = &[
    variant! {
        feature: "diagram-railroad",
        id: "railroad",
        catalog_order: 34,
        detector: Some(ordered(34, crate::detect::detector_railroad)),
        semantic: Some(ordered(11, crate::diagrams::railroad::parse_railroad)),
        combined: Some(ordered(6, crate::diagrams::railroad::parse_railroad_json_and_editor_facts)),
        typed: Some(ordered(12, render_railroad)),
        render_kind: Some("railroad"),
        metadata: Some(metadata("railroad", Some(19))),
        headers: RAILROAD_HEADERS,
        config_alias_order: None,
    },
    variant! {
        feature: "diagram-railroad",
        id: "railroadEbnf",
        catalog_order: 35,
        detector: Some(ordered(35, crate::detect::detector_railroad_ebnf)),
        semantic: Some(ordered(12, crate::diagrams::railroad::parse_railroad_ebnf)),
        combined: Some(ordered(7, crate::diagrams::railroad::parse_railroad_ebnf_json_and_editor_facts)),
        typed: Some(ordered(13, render_railroad_ebnf)),
        render_kind: Some("railroad"),
        metadata: Some(metadata("railroadEbnf", Some(21))),
        headers: RAILROAD_EBNF_HEADERS,
        config_alias_order: None,
    },
    variant! {
        feature: "diagram-railroad",
        id: "railroadAbnf",
        catalog_order: 36,
        detector: Some(ordered(36, crate::detect::detector_railroad_abnf)),
        semantic: Some(ordered(13, crate::diagrams::railroad::parse_railroad_abnf)),
        combined: Some(ordered(8, crate::diagrams::railroad::parse_railroad_abnf_json_and_editor_facts)),
        typed: Some(ordered(14, render_railroad_abnf)),
        render_kind: Some("railroad"),
        metadata: Some(metadata("railroadAbnf", Some(20))),
        headers: RAILROAD_ABNF_HEADERS,
        config_alias_order: None,
    },
    variant! {
        feature: "diagram-railroad",
        id: "railroadPeg",
        catalog_order: 37,
        detector: Some(ordered(37, crate::detect::detector_railroad_peg)),
        semantic: Some(ordered(14, crate::diagrams::railroad::parse_railroad_peg)),
        combined: Some(ordered(9, crate::diagrams::railroad::parse_railroad_peg_json_and_editor_facts)),
        typed: Some(ordered(15, render_railroad_peg)),
        render_kind: Some("railroad"),
        metadata: Some(metadata("railroadPeg", Some(22))),
        headers: RAILROAD_PEG_HEADERS,
        config_alias_order: None,
    },
];

const VENN_VARIANTS: &[FamilyVariantDefinition] = &[variant! {
    feature: "diagram-venn",
    id: "venn",
    catalog_order: 38,
    detector: Some(ordered(38, crate::detect::detector_venn)),
    semantic: Some(ordered(37, crate::diagrams::venn::parse_venn)),
    combined: Some(ordered(35, crate::diagrams::venn::parse_venn_json_and_editor_facts)),
    typed: Some(ordered(37, render_venn)),
    render_kind: Some("venn"),
    metadata: Some(metadata("venn", Some(31))),
    headers: VENN_HEADERS,
    config_alias_order: None,
}];

const WARDLEY_VARIANTS: &[FamilyVariantDefinition] = &[variant! {
    feature: "diagram-wardley",
    id: "wardley",
    catalog_order: 39,
    detector: Some(ordered(39, crate::detect::detector_wardley)),
    semantic: Some(ordered(40, crate::diagrams::wardley::parse_wardley)),
    combined: Some(ordered(39, crate::diagrams::wardley::parse_wardley_json_and_editor_facts)),
    typed: Some(ordered(40, render_wardley)),
    render_kind: Some("wardley"),
    metadata: Some(metadata("wardley", Some(32))),
    headers: WARDLEY_HEADERS,
    config_alias_order: None,
}];

const CYNEFIN_VARIANTS: &[FamilyVariantDefinition] = &[variant! {
    feature: "diagram-cynefin",
    id: "cynefin",
    catalog_order: 40,
    detector: Some(ordered(40, crate::detect::detector_cynefin)),
    semantic: Some(ordered(10, crate::diagrams::cynefin::parse_cynefin)),
    combined: Some(ordered(13, crate::diagrams::cynefin::parse_cynefin_json_and_editor_facts)),
    typed: Some(ordered(11, render_cynefin)),
    render_kind: Some("cynefin"),
    metadata: Some(metadata("cynefin", Some(4))),
    headers: CYNEFIN_HEADERS,
    config_alias_order: None,
}];

const USECASE_VARIANTS: &[FamilyVariantDefinition] = &[variant! {
    feature: "diagram-usecase",
    id: "usecase",
    catalog_order: 42,
    detector: Some(ordered(42, crate::detect::detector_usecase)),
    semantic: Some(ordered(42, crate::diagrams::usecase::parse_usecase)),
    combined: Some(ordered(42, crate::diagrams::usecase::parse_usecase_json_and_editor_facts)),
    typed: Some(ordered(42, render_usecase)),
    render_kind: Some("usecase"),
    metadata: Some(metadata("usecase", Some(36))),
    headers: USECASE_HEADERS,
    config_alias_order: None,
}];

const AGENTFLOW_VARIANTS: &[FamilyVariantDefinition] = &[variant! {
    feature: "diagram-agentflow",
    id: "agentflow",
    catalog_order: 43,
    detector: Some(ordered(43, crate::detect::detector_agentflow)),
    semantic: Some(ordered(43, crate::diagrams::agentflow::parse_agentflow)),
    warning_semantic: crate::diagrams::agentflow::parse_agentflow_with_warning_facts,
    combined: Some(ordered(43, crate::diagrams::agentflow::parse_agentflow_json_and_editor_facts)),
    typed: Some(ordered(43, render_agentflow)),
    render_kind: Some("agentflow"),
    metadata: Some(metadata("agentflow", Some(35))),
    headers: AGENTFLOW_HEADERS,
    config_alias_order: Some(36),
}];

const FAMILY_CATALOG: &[DiagramFamilyDefinition] = &[
    DiagramFamilyDefinition {
        logical_kind: "error",
        selector: None,
        editor_semantics: GENERIC_EDITOR_SEMANTICS,
        config: None,
        variants: ERROR_VARIANTS,
    },
    DiagramFamilyDefinition {
        logical_kind: "flowchart",
        selector: Some("diagram-flowchart"),
        editor_semantics: FLOWCHART_EDITOR_SEMANTICS,
        config: Some(FamilyConfigDefinition {
            namespace: "flowchart",
            frontmatter_order: 7,
        }),
        variants: FLOWCHART_VARIANTS,
    },
    DiagramFamilyDefinition {
        logical_kind: "swimlane",
        selector: Some("diagram-swimlane"),
        editor_semantics: SWIMLANE_EDITOR_SEMANTICS,
        config: Some(FamilyConfigDefinition {
            namespace: "swimlane",
            frontmatter_order: 23,
        }),
        variants: SWIMLANE_VARIANTS,
    },
    DiagramFamilyDefinition {
        logical_kind: "mindmap",
        selector: Some("diagram-mindmap"),
        editor_semantics: MINDMAP_EDITOR_SEMANTICS,
        config: Some(FamilyConfigDefinition {
            namespace: "mindmap",
            frontmatter_order: 13,
        }),
        variants: MINDMAP_VARIANTS,
    },
    DiagramFamilyDefinition {
        logical_kind: "architecture",
        selector: Some("diagram-architecture"),
        editor_semantics: GENERIC_EDITOR_SEMANTICS,
        config: Some(FamilyConfigDefinition {
            namespace: "architecture",
            frontmatter_order: 0,
        }),
        variants: ARCHITECTURE_VARIANTS,
    },
    DiagramFamilyDefinition {
        logical_kind: "zenuml",
        selector: Some("diagram-zenuml"),
        editor_semantics: GENERIC_EDITOR_SEMANTICS,
        config: Some(FamilyConfigDefinition {
            namespace: "zenuml",
            frontmatter_order: 30,
        }),
        variants: ZENUML_VARIANTS,
    },
    DiagramFamilyDefinition {
        logical_kind: "sequence",
        selector: Some("diagram-sequence"),
        editor_semantics: SEQUENCE_EDITOR_SEMANTICS,
        config: Some(FamilyConfigDefinition {
            namespace: "sequence",
            frontmatter_order: 21,
        }),
        variants: SEQUENCE_VARIANTS,
    },
    DiagramFamilyDefinition {
        logical_kind: "c4",
        selector: Some("diagram-c4"),
        editor_semantics: CARDINAL_EDITOR_SEMANTICS,
        config: Some(FamilyConfigDefinition {
            namespace: "c4",
            frontmatter_order: 2,
        }),
        variants: C4_VARIANTS,
    },
    DiagramFamilyDefinition {
        logical_kind: "kanban",
        selector: Some("diagram-kanban"),
        editor_semantics: GENERIC_EDITOR_SEMANTICS,
        config: Some(FamilyConfigDefinition {
            namespace: "kanban",
            frontmatter_order: 12,
        }),
        variants: KANBAN_VARIANTS,
    },
    DiagramFamilyDefinition {
        logical_kind: "class",
        selector: Some("diagram-class"),
        editor_semantics: CLASS_EDITOR_SEMANTICS,
        config: Some(FamilyConfigDefinition {
            namespace: "class",
            frontmatter_order: 3,
        }),
        variants: CLASS_VARIANTS,
    },
    DiagramFamilyDefinition {
        logical_kind: "er",
        selector: Some("diagram-er"),
        editor_semantics: ER_EDITOR_SEMANTICS,
        config: Some(FamilyConfigDefinition {
            namespace: "er",
            frontmatter_order: 5,
        }),
        variants: ER_VARIANTS,
    },
    DiagramFamilyDefinition {
        logical_kind: "gantt",
        selector: Some("diagram-gantt"),
        editor_semantics: GENERIC_EDITOR_SEMANTICS,
        config: Some(FamilyConfigDefinition {
            namespace: "gantt",
            frontmatter_order: 8,
        }),
        variants: GANTT_VARIANTS,
    },
    DiagramFamilyDefinition {
        logical_kind: "info",
        selector: Some("diagram-info"),
        editor_semantics: GENERIC_EDITOR_SEMANTICS,
        config: None,
        variants: INFO_VARIANTS,
    },
    DiagramFamilyDefinition {
        logical_kind: "pie",
        selector: Some("diagram-pie"),
        editor_semantics: GENERIC_EDITOR_SEMANTICS,
        config: Some(FamilyConfigDefinition {
            namespace: "pie",
            frontmatter_order: 15,
        }),
        variants: PIE_VARIANTS,
    },
    DiagramFamilyDefinition {
        logical_kind: "requirement",
        selector: Some("diagram-requirement"),
        editor_semantics: CARDINAL_EDITOR_SEMANTICS,
        config: Some(FamilyConfigDefinition {
            namespace: "requirement",
            frontmatter_order: 19,
        }),
        variants: REQUIREMENT_VARIANTS,
    },
    DiagramFamilyDefinition {
        logical_kind: "timeline",
        selector: Some("diagram-timeline"),
        editor_semantics: GENERIC_EDITOR_SEMANTICS,
        config: Some(FamilyConfigDefinition {
            namespace: "timeline",
            frontmatter_order: 24,
        }),
        variants: TIMELINE_VARIANTS,
    },
    DiagramFamilyDefinition {
        logical_kind: "gitGraph",
        selector: Some("diagram-git-graph"),
        editor_semantics: GENERIC_EDITOR_SEMANTICS,
        config: Some(FamilyConfigDefinition {
            namespace: "gitGraph",
            frontmatter_order: 9,
        }),
        variants: GIT_GRAPH_VARIANTS,
    },
    DiagramFamilyDefinition {
        logical_kind: "state",
        selector: Some("diagram-state"),
        editor_semantics: STATE_EDITOR_SEMANTICS,
        config: Some(FamilyConfigDefinition {
            namespace: "state",
            frontmatter_order: 22,
        }),
        variants: STATE_VARIANTS,
    },
    DiagramFamilyDefinition {
        logical_kind: "journey",
        selector: Some("diagram-journey"),
        editor_semantics: GENERIC_EDITOR_SEMANTICS,
        config: Some(FamilyConfigDefinition {
            namespace: "journey",
            frontmatter_order: 11,
        }),
        variants: JOURNEY_VARIANTS,
    },
    DiagramFamilyDefinition {
        logical_kind: "quadrantChart",
        selector: Some("diagram-quadrant-chart"),
        editor_semantics: GENERIC_EDITOR_SEMANTICS,
        config: Some(FamilyConfigDefinition {
            namespace: "quadrantChart",
            frontmatter_order: 16,
        }),
        variants: QUADRANT_VARIANTS,
    },
    DiagramFamilyDefinition {
        logical_kind: "sankey",
        selector: Some("diagram-sankey"),
        editor_semantics: GENERIC_EDITOR_SEMANTICS,
        config: Some(FamilyConfigDefinition {
            namespace: "sankey",
            frontmatter_order: 20,
        }),
        variants: SANKEY_VARIANTS,
    },
    DiagramFamilyDefinition {
        logical_kind: "packet",
        selector: Some("diagram-packet"),
        editor_semantics: GENERIC_EDITOR_SEMANTICS,
        config: Some(FamilyConfigDefinition {
            namespace: "packet",
            frontmatter_order: 14,
        }),
        variants: PACKET_VARIANTS,
    },
    DiagramFamilyDefinition {
        logical_kind: "xychart",
        selector: Some("diagram-xychart"),
        editor_semantics: GENERIC_EDITOR_SEMANTICS,
        config: Some(FamilyConfigDefinition {
            namespace: "xyChart",
            frontmatter_order: 29,
        }),
        variants: XYCHART_VARIANTS,
    },
    DiagramFamilyDefinition {
        logical_kind: "block",
        selector: Some("diagram-block"),
        editor_semantics: BLOCK_EDITOR_SEMANTICS,
        config: Some(FamilyConfigDefinition {
            namespace: "block",
            frontmatter_order: 1,
        }),
        variants: BLOCK_VARIANTS,
    },
    DiagramFamilyDefinition {
        logical_kind: "eventmodeling",
        selector: Some("diagram-event-modeling"),
        editor_semantics: GENERIC_EDITOR_SEMANTICS,
        config: Some(FamilyConfigDefinition {
            namespace: "eventmodeling",
            frontmatter_order: 6,
        }),
        variants: EVENTMODELING_VARIANTS,
    },
    DiagramFamilyDefinition {
        logical_kind: "treeView",
        selector: Some("diagram-tree-view"),
        editor_semantics: GENERIC_EDITOR_SEMANTICS,
        config: Some(FamilyConfigDefinition {
            namespace: "treeView",
            frontmatter_order: 25,
        }),
        variants: TREE_VIEW_VARIANTS,
    },
    DiagramFamilyDefinition {
        logical_kind: "radar",
        selector: Some("diagram-radar"),
        editor_semantics: GENERIC_EDITOR_SEMANTICS,
        config: Some(FamilyConfigDefinition {
            namespace: "radar",
            frontmatter_order: 17,
        }),
        variants: RADAR_VARIANTS,
    },
    DiagramFamilyDefinition {
        logical_kind: "ishikawa",
        selector: Some("diagram-ishikawa"),
        editor_semantics: GENERIC_EDITOR_SEMANTICS,
        config: Some(FamilyConfigDefinition {
            namespace: "ishikawa",
            frontmatter_order: 10,
        }),
        variants: ISHIKAWA_VARIANTS,
    },
    DiagramFamilyDefinition {
        logical_kind: "treemap",
        selector: Some("diagram-treemap"),
        editor_semantics: GENERIC_EDITOR_SEMANTICS,
        config: Some(FamilyConfigDefinition {
            namespace: "treemap",
            frontmatter_order: 26,
        }),
        variants: TREEMAP_VARIANTS,
    },
    DiagramFamilyDefinition {
        logical_kind: "railroad",
        selector: Some("diagram-railroad"),
        editor_semantics: GENERIC_EDITOR_SEMANTICS,
        config: Some(FamilyConfigDefinition {
            namespace: "railroad",
            frontmatter_order: 18,
        }),
        variants: RAILROAD_VARIANTS,
    },
    DiagramFamilyDefinition {
        logical_kind: "venn",
        selector: Some("diagram-venn"),
        editor_semantics: GENERIC_EDITOR_SEMANTICS,
        config: Some(FamilyConfigDefinition {
            namespace: "venn",
            frontmatter_order: 27,
        }),
        variants: VENN_VARIANTS,
    },
    DiagramFamilyDefinition {
        logical_kind: "wardley",
        selector: Some("diagram-wardley"),
        editor_semantics: GENERIC_EDITOR_SEMANTICS,
        config: Some(FamilyConfigDefinition {
            namespace: "wardley-beta",
            frontmatter_order: 28,
        }),
        variants: WARDLEY_VARIANTS,
    },
    DiagramFamilyDefinition {
        logical_kind: "cynefin",
        selector: Some("diagram-cynefin"),
        editor_semantics: GENERIC_EDITOR_SEMANTICS,
        config: Some(FamilyConfigDefinition {
            namespace: "cynefin",
            frontmatter_order: 4,
        }),
        variants: CYNEFIN_VARIANTS,
    },
    DiagramFamilyDefinition {
        logical_kind: "usecase",
        selector: Some("diagram-usecase"),
        editor_semantics: GENERIC_EDITOR_SEMANTICS,
        config: Some(FamilyConfigDefinition {
            namespace: "usecase",
            frontmatter_order: 35,
        }),
        variants: USECASE_VARIANTS,
    },
    DiagramFamilyDefinition {
        logical_kind: "agentflow",
        selector: Some("diagram-agentflow"),
        editor_semantics: GENERIC_EDITOR_SEMANTICS,
        config: Some(FamilyConfigDefinition {
            namespace: "agentflow",
            frontmatter_order: 36,
        }),
        variants: AGENTFLOW_VARIANTS,
    },
];

#[cfg(test)]
mod catalog_tests {
    use super::*;
    use std::collections::BTreeSet;

    #[test]
    fn selectors_cover_logical_families_once_without_infrastructure_or_aliases() {
        let selectors = diagram_family_selectors();
        assert_eq!(selectors.len(), 34);
        let mut features = BTreeSet::new();
        let mut families = BTreeSet::new();
        for selector in selectors {
            assert!(features.insert(selector.feature));
            assert!(families.insert(selector.logical_family_kind));
            assert!(selector.feature.starts_with("diagram-"));
            assert!(
                selector
                    .feature
                    .bytes()
                    .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'-')
            );
        }
        for family in FAMILY_CATALOG {
            assert_eq!(family.selector.is_some(), family.logical_kind != "error");
            assert_eq!(
                families.contains(family.logical_kind),
                family.logical_kind != "error"
            );
        }
        for (id, expected) in [
            ("flowchart", "diagram-flowchart"),
            ("flowchart-v2", "diagram-flowchart"),
            ("flowchart-elk", "diagram-flowchart"),
            ("swimlane", "diagram-swimlane"),
            ("gitGraph", "diagram-git-graph"),
            ("quadrantChart", "diagram-quadrant-chart"),
            ("treeView", "diagram-tree-view"),
            ("eventmodeling", "diagram-event-modeling"),
        ] {
            let (family, _) = find_variant(id).expect("known family alias");
            assert_eq!(family.selector, Some(expected), "{id}");
        }
        assert!(!features.contains("diagram-error"));
        assert!(!features.contains("diagram-flowchart-elk"));
    }

    #[test]
    fn public_metadata_ids_are_catalog_owned_instead_of_derived_from_family_names() {
        assert_eq!(diagram_type_metadata_id("flowchart-v2"), Some("flowchart"));
        assert_eq!(diagram_type_metadata_id("gitGraph"), Some("gitgraph"));
        assert_eq!(
            diagram_type_metadata_id("quadrantChart"),
            Some("quadrantchart")
        );
        assert_eq!(diagram_type_metadata_id("treeView"), Some("treeView"));
    }

    #[test]
    fn editor_semantics_follow_family_identity_across_variant_ids() {
        for variant in ["flowchart-v2", "flowchart-elk", "flowchart"] {
            assert_eq!(
                diagram_type_family_id(variant),
                Some(DiagramFamilyId::FLOWCHART),
                "{variant} must retain typed Flowchart family identity"
            );
        }

        let flowchart = diagram_type_editor_semantics("flowchart-v2")
            .expect("flowchart-v2 has editor family semantics");
        for variant in ["flowchart-elk", "flowchart"] {
            assert_eq!(
                diagram_type_editor_semantics(variant),
                Some(flowchart),
                "{variant} must inherit flowchart family semantics"
            );
        }

        let class = diagram_type_editor_semantics("classDiagram")
            .expect("classDiagram has editor family semantics");
        assert_eq!(diagram_type_editor_semantics("class"), Some(class));
        assert_ne!(flowchart, class);
    }

    #[test]
    fn absent_implementations_preserve_the_complete_identity_projection() {
        let unavailable = variants()
            .map(|(family, variant)| {
                (
                    family,
                    FamilyVariantDefinition {
                        identity: variant.identity,
                        implementation: FamilyImplementation::default(),
                    },
                )
            })
            .collect::<Vec<_>>();
        let projected = FamilyCatalogProjection::build(
            unavailable
                .iter()
                .map(|(family, variant)| (*family, variant)),
        );
        let complete = family_catalog_projection();

        assert!(projected.semantic_parser_facts.is_empty());
        assert!(projected.combined_parser_facts.is_empty());
        assert!(projected.render_parser_facts.is_empty());
        assert!(projected.supported_diagram_metadata_ids.is_empty());
        assert!(projected.built_in_typed_render_families.is_empty());
        assert_eq!(
            projected.diagram_header_facts,
            complete.diagram_header_facts
        );
        assert_eq!(
            projected
                .detector_facts
                .iter()
                .map(|fact| fact.id)
                .collect::<Vec<_>>(),
            complete
                .detector_facts
                .iter()
                .map(|fact| fact.id)
                .collect::<Vec<_>>()
        );
        let expected = complete
            .diagram_family_capabilities
            .iter()
            .map(|fact| DiagramFamilyCapability {
                has_semantic_parser: false,
                has_editor_parser: false,
                has_combined_parser: false,
                has_render_parser: false,
                ..*fact
            })
            .collect::<Vec<_>>();
        assert_eq!(projected.diagram_family_capabilities, expected);
    }

    #[test]
    #[cfg(feature = "diagram-flowchart")]
    fn parser_and_typed_model_enumerations_follow_their_own_callbacks() {
        let (family, variant) = find_variant("flowchart-v2").unwrap();
        let semantic_only = FamilyVariantDefinition {
            identity: variant.identity,
            implementation: FamilyImplementation {
                semantic: variant.implementation.semantic,
                ..FamilyImplementation::default()
            },
        };
        let projected = FamilyCatalogProjection::build([(family, &semantic_only)]);
        assert_eq!(projected.supported_diagram_metadata_ids, ["flowchart"]);
        assert!(projected.built_in_typed_render_families.is_empty());
        assert!(projected.diagram_family_capabilities[0].has_semantic_parser);
        assert!(!projected.diagram_family_capabilities[0].has_editor_parser);
        assert!(!projected.diagram_family_capabilities[0].has_render_parser);

        let typed_only = FamilyVariantDefinition {
            identity: variant.identity,
            implementation: FamilyImplementation {
                typed_render: variant.implementation.typed_render,
                ..FamilyImplementation::default()
            },
        };
        let projected = FamilyCatalogProjection::build([(family, &typed_only)]);
        assert!(projected.supported_diagram_metadata_ids.is_empty());
        assert_eq!(
            projected.built_in_typed_render_families,
            [BuiltInTypedRenderFamily {
                diagram_type: "flowchart",
                render_model_kind: "flowchart",
            }]
        );
        assert!(!projected.diagram_family_capabilities[0].has_semantic_parser);
        assert!(projected.diagram_family_capabilities[0].has_render_parser);
    }

    #[test]
    #[cfg(feature = "diagram-swimlane")]
    fn shared_typed_models_do_not_admit_an_unavailable_logical_family() {
        let projected_variants = variants()
            .map(|(family, variant)| {
                let mut variant = *variant;
                if family.logical_kind != "swimlane" {
                    variant.implementation = FamilyImplementation::default();
                }
                (family, variant)
            })
            .collect::<Vec<_>>();
        let projected = FamilyCatalogProjection::build(
            projected_variants
                .iter()
                .map(|(family, variant)| (*family, variant)),
        );
        assert_eq!(projected.supported_diagram_metadata_ids, ["swimlane"]);
        assert_eq!(
            projected.built_in_typed_render_families,
            [BuiltInTypedRenderFamily {
                diagram_type: "swimlane",
                render_model_kind: "flowchart",
            }]
        );
        assert!(
            projected.diagram_family_capabilities.iter().all(|fact| {
                fact.has_semantic_parser == (fact.logical_family_kind == "swimlane")
            })
        );
    }

    #[test]
    fn catalog_ids_orders_and_family_policy_are_internally_consistent() {
        let mut ids = BTreeSet::new();
        let mut catalog_orders = BTreeSet::new();
        let mut detector_orders = BTreeSet::new();
        let mut semantic_orders = BTreeSet::new();
        let mut combined_orders = BTreeSet::new();
        let mut render_orders = BTreeSet::new();
        let mut metadata_orders = BTreeSet::new();
        let mut header_orders = BTreeSet::new();

        for family in FAMILY_CATALOG {
            for variant in family.variants {
                assert_ne!(
                    variant.identity.id, "---",
                    "frontmatter guard is not a family"
                );
                assert!(
                    ids.insert(variant.identity.id),
                    "duplicate catalog id {}",
                    variant.identity.id
                );
                assert!(
                    catalog_orders.insert(variant.identity.catalog_order),
                    "duplicate catalog order {}",
                    variant.identity.catalog_order
                );
                assert!(
                    variant.implementation.typed_render.is_none()
                        || variant.identity.render_model_kind.is_some(),
                    "{} typed parser requires a known render kind",
                    variant.identity.id
                );
                assert!(
                    variant.identity.metadata.is_none()
                        || variant.identity.render_model_kind.is_some(),
                    "{} metadata requires a known render kind",
                    variant.identity.id
                );
                assert!(
                    variant.implementation.combined.is_none()
                        || variant.implementation.semantic.is_some(),
                    "{} combined parsing requires a semantic adapter",
                    variant.identity.id
                );

                if let Some(fact) = variant.identity.detector {
                    assert!(detector_orders.insert(fact.order));
                }
                if let Some(fact) = variant.implementation.semantic {
                    assert!(semantic_orders.insert(fact.order));
                }
                if let Some(fact) = variant.implementation.combined {
                    assert!(combined_orders.insert(fact.order));
                }
                if let Some(fact) = variant.implementation.typed_render {
                    assert!(render_orders.insert(fact.order));
                }
                if let Some(order) = variant
                    .identity
                    .metadata
                    .and_then(|metadata| metadata.order)
                {
                    assert!(metadata_orders.insert(order));
                }
                for fact in variant.identity.headers {
                    assert!(header_orders.insert(fact.order));
                }
            }
        }

        let config_orders = FAMILY_CATALOG
            .iter()
            .filter_map(|family| family.config.map(|config| config.frontmatter_order))
            .collect::<BTreeSet<_>>();
        let config_count = FAMILY_CATALOG
            .iter()
            .filter(|family| family.config.is_some())
            .count();
        assert_eq!(config_orders.len(), config_count);
        assert_eq!(ids.len(), diagram_family_capabilities().len());
    }
}
