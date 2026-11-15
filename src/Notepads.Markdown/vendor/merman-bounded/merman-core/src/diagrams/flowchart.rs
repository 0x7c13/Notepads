// Notepads Markdown modifications: Copyright (c) 2026, Jiaqi (0x7c13) Liu.
// Original upstream MIT/Apache-2.0 licenses and notices are preserved.
use crate::SourceSpan;
#[cfg(any(feature = "diagram-flowchart", feature = "diagram-swimlane"))]
use crate::diagram::legacy_warning_messages;
#[cfg(any(feature = "diagram-flowchart", feature = "diagram-swimlane"))]
use crate::native_collections::OrderedMap as IndexMap;
#[cfg(any(feature = "diagram-flowchart", feature = "diagram-swimlane"))]
use crate::native_collections::{
    self as memory, NativeHashMap as HashMap, NativeHashSet as HashSet,
};
#[cfg(any(feature = "diagram-flowchart", feature = "diagram-swimlane"))]
use crate::sanitize::sanitize_text;
#[cfg(any(feature = "diagram-flowchart", feature = "diagram-swimlane"))]
use crate::{
    DiagramWarningFact, EditorExpectedSyntax, EditorExpectedSyntaxKind, EditorRenamePolicy,
    EditorSemanticFacts, EditorSemanticKind, EditorSemanticRole, EditorSemanticSymbol, Error,
    FLOWCHART_EXPLICIT_DIRECTION_WARNING_RULE_ID, MermaidConfig, OperationControl,
    OperationControlResult, ParseMetadata, Result,
    editor::{format_lalrpop_parse_error, lalrpop_parse_diagnostic, lalrpop_recovery_span},
};
#[cfg(any(feature = "diagram-flowchart", feature = "diagram-swimlane"))]
use serde_json::{Value, json};
#[cfg(all(test, any(feature = "diagram-flowchart", feature = "diagram-swimlane")))]
use std::cell::Cell;

#[cfg(all(test, any(feature = "diagram-flowchart", feature = "diagram-swimlane")))]
thread_local! {
    static FLOWCHART_TOKEN_TRACE_CONSTRUCTION_COUNT: Cell<usize> = const { Cell::new(0) };
}

#[cfg(all(test, any(feature = "diagram-flowchart", feature = "diagram-swimlane")))]
pub(crate) fn reset_flowchart_token_trace_construction_count() {
    FLOWCHART_TOKEN_TRACE_CONSTRUCTION_COUNT.set(0);
}

#[cfg(all(test, any(feature = "diagram-flowchart", feature = "diagram-swimlane")))]
pub(crate) fn flowchart_token_trace_construction_count() -> usize {
    FLOWCHART_TOKEN_TRACE_CONSTRUCTION_COUNT.get()
}

#[cfg(all(test, any(feature = "diagram-flowchart", feature = "diagram-swimlane")))]
pub(crate) fn reset_flowchart_accessibility_scan_count() {
    accessibility::reset_flowchart_accessibility_scan_count();
}

#[cfg(all(test, any(feature = "diagram-flowchart", feature = "diagram-swimlane")))]
pub(crate) fn flowchart_accessibility_scan_count() -> usize {
    accessibility::flowchart_accessibility_scan_count()
}

#[cfg(any(feature = "diagram-flowchart", feature = "diagram-swimlane"))]
include_checked_in_lalrpop_parser!(
    #[allow(
        clippy::empty_line_after_outer_attr,
        clippy::large_enum_variant,
        clippy::type_complexity,
        clippy::result_large_err
    )]
    flowchart_grammar,
    "flowchart_grammar.rs"
);

#[cfg(any(feature = "diagram-flowchart", feature = "diagram-swimlane"))]
mod accessibility;
mod ast;
#[cfg(any(feature = "diagram-flowchart", feature = "diagram-swimlane"))]
mod build;
mod lex;
mod lexer;
mod lexer_iter;
mod link;
mod model;
#[cfg(feature = "diagram-flowchart")]
mod native;
#[cfg(feature = "diagram-flowchart")]
pub use native::{NativeFeature, NativeParseError, NativeParsePolicy};
#[cfg(any(feature = "diagram-flowchart", feature = "diagram-swimlane"))]
mod semantic;
#[cfg(any(feature = "diagram-flowchart", feature = "diagram-swimlane"))]
mod shape_data;
#[cfg(any(feature = "diagram-flowchart", feature = "diagram-swimlane"))]
mod subgraph;
mod text;
mod tokens;

use text::{is_ecmascript_trim_char, parse_label_text};
#[cfg(any(feature = "diagram-flowchart", feature = "diagram-swimlane"))]
use text::{title_kind_str, trim_flowdb_label_text};

#[doc(hidden)]
pub use model::FlowchartRenderContext;
pub use model::{
    FlowEdge, FlowEdgeDefaults, FlowEdgeMarker, FlowEdgeStroke, FlowEdgeVisibility, FlowNode,
    FlowNodeProvenance, FlowSubgraph, FlowchartModel,
};

pub(crate) use model::{LabeledText, LinkToken, SubgraphHeader, TitleKind};

pub(crate) use ast::{
    ClassAssignStmt, ClassDefStmt, ClickAction, ClickStmt, LinkStylePos, LinkStyleStmt, StyleStmt,
};
#[cfg(any(feature = "diagram-flowchart", feature = "diagram-swimlane"))]
use ast::{
    FlowchartAst, StatementArena, StatementEvent, StatementId, StatementList, Stmt, SubgraphBlock,
};
#[cfg(any(feature = "diagram-flowchart", feature = "diagram-swimlane"))]
use model::{
    Edge, EdgeDefaults, FlowNodeSyntax, FlowSubgraphVertexStyle, FlowchartRenderLabelSources,
    FlowchartRenderStyleSources, Node,
};

pub(crate) use tokens::{ArrowToken, DirectionStatementToken, LexError, NodeLabelToken, Tok};

use super::shapes::{pinned_shape_names, public_pinned_shape_names};
#[cfg(any(feature = "diagram-flowchart", feature = "diagram-swimlane"))]
use accessibility::{
    FlowchartAccessibilityScan, FlowchartAccessibilityStatement, scan_flowchart_accessibility,
    scan_flowchart_accessibility_controlled,
};
#[cfg(any(feature = "diagram-flowchart", feature = "diagram-swimlane"))]
use build::FlowchartBuildState;
use lexer::Lexer;
use link::{destruct_end_link, destruct_labeled_end_link, destruct_start_link};
#[cfg(any(feature = "diagram-flowchart", feature = "diagram-swimlane"))]
use semantic::{FlowchartSemanticContext, apply_semantic_statements};
#[cfg(any(feature = "diagram-flowchart", feature = "diagram-swimlane"))]
use shape_data::{apply_shape_data_value_to_node, value_to_bool, value_to_string};
#[cfg(any(feature = "diagram-flowchart", feature = "diagram-swimlane"))]
use subgraph::SubgraphBuilder;

#[cfg(feature = "diagram-agentflow")]
/// Lex the presentation directives shared by Flowchart and Agentflow.
/// Family-specific mutation order remains the caller's responsibility.
pub(crate) fn lex_presentation_statement(
    source: &str,
) -> Option<std::result::Result<(usize, Tok, usize), LexError>> {
    let mut lexer = Lexer::new(source);
    lexer
        .lex_style_stmt()
        .or_else(|| lexer.lex_classdef_stmt())
        .or_else(|| lexer.lex_class_assign_stmt())
        .or_else(|| lexer.lex_click_stmt())
        .or_else(|| lexer.lex_link_style_stmt())
}

pub(crate) fn is_valid_editor_node_id(candidate: &str) -> bool {
    let mut lexer = Lexer::new(candidate);
    matches!(
        (lexer.next(), lexer.next()),
        (Some(Ok((0, Tok::Id(_), end))), None) if end == candidate.len()
    )
}

#[derive(Debug, Clone)]
#[cfg(any(feature = "diagram-flowchart", feature = "diagram-swimlane"))]
pub(crate) struct FlowSubGraph {
    pub id: String,
    pub nodes: Vec<String>,
    pub title: String,
    pub classes: Vec<String>,
    pub styles: Vec<String>,
    pub dir: Option<String>,
    pub has_explicit_dir: bool,
    pub label_type: String,
    pub metadata: Option<Value>,
}

#[cfg(any(feature = "diagram-flowchart", feature = "diagram-swimlane"))]
struct FlowchartSemanticSource {
    keyword: String,
    direction: Option<String>,
    acc_title: Option<String>,
    acc_descr: Option<String>,
    class_defs: IndexMap<String, Vec<String>>,
    tooltips: HashMap<String, String>,
    edge_defaults: EdgeDefaults,
    vertex_calls: Vec<String>,
    nodes: Vec<Node>,
    edges: Vec<Edge>,
    subgraphs: Vec<FlowSubGraph>,
    subgraph_vertex_styles: FlowchartRenderStyleSources,
    collapsed_subgraphs: HashSet<String>,
    warning_facts: Vec<DiagramWarningFact>,
}

#[cfg(any(feature = "diagram-flowchart", feature = "diagram-swimlane"))]
pub(crate) fn parse_flowchart(code: &str, meta: &ParseMetadata) -> Result<Value> {
    parse_flowchart_with_warning_facts(code, meta)
        .map(crate::family::WarningSemanticParse::into_model)
}

#[cfg(any(feature = "diagram-flowchart", feature = "diagram-swimlane"))]
pub(crate) fn parse_flowchart_with_warning_facts(
    code: &str,
    meta: &ParseMetadata,
) -> Result<crate::family::WarningSemanticParse> {
    let model = parse_flowchart_semantic_source(code, meta)?.into_render_model(meta)?;
    let compatibility = render_model_to_compat_json(&model, meta)?;
    Ok(crate::family::WarningSemanticParse::new(
        compatibility,
        model.warning_facts,
    ))
}

#[cfg(any(feature = "diagram-flowchart", feature = "diagram-swimlane"))]
pub(crate) fn parse_flowchart_json_and_editor_facts(
    code: &str,
    meta: &ParseMetadata,
    control: &OperationControl,
) -> OperationControlResult<crate::family::CombinedSemanticParse> {
    control.checkpoint()?;
    let FlowchartAccessibilityScan {
        parser_input: code,
        title: acc_title,
        description: acc_descr,
        statements: accessibility_statements,
    } = scan_flowchart_accessibility_controlled(code, control)?;
    control.checkpoint()?;
    let trace = construct_flowchart_token_trace(&code, control)?;
    let construction = match parse_flowchart_ast_from_trace(&trace, control)? {
        Ok(ast) => {
            let mut facts = editor_facts_from_flowchart_ast(&ast, control)?;
            control.checkpoint()?;
            collect_accessibility_directive_prefixes(
                &accessibility_statements,
                &mut facts,
                control,
            )?;
            collect_expected_syntax_from_tokens(&code, trace.editor_tokens(), &mut facts, control)?;
            let (model, warning_facts) = match parse_flowchart_semantic_source_from_ast_controlled(
                ast, acc_title, acc_descr, meta, control,
            )? {
                Ok(source) => match source.into_render_model_controlled(meta, control)? {
                    Ok(model) => {
                        let compatibility = render_model_to_compat_json(&model, meta);
                        (compatibility, model.warning_facts)
                    }
                    Err(error) => (Err(error), Vec::new()),
                },
                Err(error) => (Err(error), Vec::new()),
            };
            control.checkpoint()?;
            Ok((model, facts, warning_facts))
        }
        Err(error) => {
            let facts = flowchart_recovery_facts(
                &code,
                trace,
                &accessibility_statements,
                error.as_ref(),
                control,
            )?;
            let error = Error::diagram_parse_diagnostic(
                meta.diagram_type.clone(),
                flowchart_parse_diagnostic(error.as_ref(), &code, &facts),
            );
            Err(crate::family::CombinedSemanticFailure::new(error, facts))
        }
    };
    let parsed = crate::family::CombinedSemanticParse::from_construction_with_warning_facts(
        construction,
        |parts| parts,
        crate::family::CombinedSemanticFailure::into_parts,
    );
    control.checkpoint()?;
    Ok(parsed)
}

#[cfg(all(test, any(feature = "diagram-flowchart", feature = "diagram-swimlane")))]
pub(crate) fn parse_flowchart_model_for_render(
    code: &str,
    meta: &ParseMetadata,
) -> Result<FlowchartModel> {
    parse_flowchart_semantic_source(code, meta)?.into_render_model(meta)
}

#[cfg(all(test, any(feature = "diagram-flowchart", feature = "diagram-swimlane")))]
pub(crate) fn parse_flowchart_model_with_render_context(
    code: &str,
    meta: &ParseMetadata,
) -> Result<(FlowchartModel, FlowchartRenderContext)> {
    parse_flowchart_semantic_source(code, meta)?.into_render_model_parts(meta)
}

#[cfg(any(feature = "diagram-flowchart", feature = "diagram-swimlane"))]
pub(crate) fn parse_flowchart_model_with_render_context_controlled(
    code: &str,
    meta: &ParseMetadata,
    control: &OperationControl,
) -> OperationControlResult<Result<(FlowchartModel, FlowchartRenderContext)>> {
    let source = parse_flowchart_semantic_source_controlled(code, meta, control)?;
    match source {
        Ok(source) => source.into_render_model_parts_controlled(meta, control),
        Err(error) => Ok(Err(error)),
    }
}

/// Pure, bounded native flowchart producer. No Engine, registry, metadata JSON,
/// DOM or YAML configuration is constructed by this entry.
#[cfg(feature = "diagram-flowchart")]
pub fn parse_flowchart_native_controlled(
    code: &str,
    policy: &NativeParsePolicy,
    control: &OperationControl,
) -> std::result::Result<
    markdown_memory::NativePayload<(FlowchartModel, FlowchartRenderContext)>,
    NativeParseError,
> {
    native::parse(code, policy, control)
}

/// Decodes an already classified label text token once. The caller must first
/// recognize raw markup/hard breaks, and must not tokenize the decoded result.
#[cfg(feature = "diagram-flowchart")]
pub fn decode_flowchart_native_text(
    input: &str,
    control: &OperationControl,
) -> std::result::Result<markdown_memory::NativePayload<String>, NativeParseError> {
    let region = control.memory_region().ok_or(NativeParseError::Memory(
        markdown_memory::MemoryError::Unfunded,
    ))?;
    control.checkpoint().map_err(|_| {
        control
            .memory_scope()
            .and_then(|scope| scope.failure())
            .map(NativeParseError::Memory)
            .unwrap_or(NativeParseError::Cancelled)
    })?;
    let value = native::semantic_text(input, control)?;
    Ok(markdown_memory::NativePayload::new(value, region.clone()))
}

#[cfg(any(feature = "diagram-flowchart", feature = "diagram-swimlane"))]
pub(crate) fn render_model_to_compat_json(
    model: &FlowchartModel,
    meta: &ParseMetadata,
) -> Result<Value> {
    let mut value =
        serde_json::to_value(model).expect("Flowchart typed model must remain JSON-serializable");
    let root = value.as_object_mut().ok_or_else(|| {
        Error::diagram_parse_fallback(
            meta.diagram_type.clone(),
            "flowchart typed model did not serialize to an object".to_string(),
        )
    })?;
    if let Some(edges) = root.get_mut("edges").and_then(Value::as_array_mut) {
        for edge in edges {
            if let Some(edge) = edge.as_object_mut() {
                edge.remove("startMarker");
                edge.remove("endMarker");
                edge.remove("strokeKind");
                edge.remove("visibility");
            }
        }
    }
    if let Some(nodes) = root.get_mut("nodes").and_then(Value::as_array_mut) {
        for node in nodes {
            if let Some(node) = node.as_object_mut() {
                node.remove("provenance");
            }
        }
    }
    root.insert("type".to_string(), Value::String(meta.diagram_type.clone()));
    if model
        .warning_facts
        .iter()
        .any(|fact| fact.rule_id == FLOWCHART_EXPLICIT_DIRECTION_WARNING_RULE_ID)
    {
        root.insert("direction".to_string(), Value::Null);
    }
    if !model.warning_facts.is_empty() {
        root.insert(
            "warnings".to_string(),
            json!(legacy_warning_messages(&model.warning_facts)),
        );
    }
    Ok(value)
}

pub fn flowchart_public_shape_names() -> impl Iterator<Item = &'static str> {
    public_pinned_shape_names()
}

#[doc(hidden)]
pub fn flowchart_pinned_shape_names() -> impl Iterator<Item = &'static str> {
    pinned_shape_names()
}

#[cfg(any(feature = "diagram-flowchart", feature = "diagram-swimlane"))]
fn parse_flowchart_semantic_source(
    code: &str,
    meta: &ParseMetadata,
) -> Result<FlowchartSemanticSource> {
    let FlowchartAccessibilityScan {
        parser_input: code,
        title: acc_title,
        description: acc_descr,
        ..
    } = scan_flowchart_accessibility(code);
    let ast = parse_flowchart_ast(&code, meta)?;
    let control = OperationControl::new();
    parse_flowchart_semantic_source_from_ast_controlled(ast, acc_title, acc_descr, meta, &control)
        .expect("a private parse control cannot be cancelled")
}

#[cfg(any(feature = "diagram-flowchart", feature = "diagram-swimlane"))]
fn parse_flowchart_semantic_source_controlled(
    code: &str,
    meta: &ParseMetadata,
    control: &OperationControl,
) -> OperationControlResult<Result<FlowchartSemanticSource>> {
    control.checkpoint()?;
    if control.memory_scope().is_some() && !has_accessibility_masking_directive(code) {
        // The borrowed parser input remains immutable for this call.
        // With no directive to mask, borrow it instead of making an unfunded
        // full-source conversion solely to obtain the identical parser bytes.
        let ast = match parse_flowchart_ast_controlled(code, meta, control)? {
            Ok(ast) => ast,
            Err(error) => return Ok(Err(error)),
        };
        return parse_flowchart_semantic_source_from_ast_controlled(ast, None, None, meta, control);
    }
    let FlowchartAccessibilityScan {
        parser_input: code,
        title: acc_title,
        description: acc_descr,
        ..
    } = scan_flowchart_accessibility_controlled(code, control)?;
    let ast = match parse_flowchart_ast_controlled(&code, meta, control)? {
        Ok(ast) => ast,
        Err(error) => return Ok(Err(error)),
    };
    parse_flowchart_semantic_source_from_ast_controlled(ast, acc_title, acc_descr, meta, control)
}

#[cfg(any(feature = "diagram-flowchart", feature = "diagram-swimlane"))]
fn has_accessibility_masking_directive(code: &str) -> bool {
    // Match the existing accessibility scanner's line-prefix rules exactly.
    // str::lines retains lone CR in the same line, like the original LF scan.
    code.lines().any(|line| {
        let line = line.trim_start();
        line.strip_prefix("accTitle")
            .is_some_and(|rest| rest.trim_start().starts_with(':'))
            || line.strip_prefix("accDescr").is_some_and(|rest| {
                let rest = rest.trim_start();
                rest.starts_with(':') || rest.starts_with('{')
            })
    })
}

#[cfg(any(feature = "diagram-flowchart", feature = "diagram-swimlane"))]
fn parse_flowchart_ast_controlled(
    code: &str,
    meta: &ParseMetadata,
    control: &OperationControl,
) -> OperationControlResult<Result<FlowchartAst>> {
    parse_flowchart_ast_for_type_controlled(code, &meta.diagram_type, control)
}

#[cfg(any(feature = "diagram-flowchart", feature = "diagram-swimlane"))]
fn parse_flowchart_ast_for_type_controlled(
    code: &str,
    diagram_type: &str,
    control: &OperationControl,
) -> OperationControlResult<Result<FlowchartAst>> {
    if let Some(scope) = control.memory_scope() {
        if control.memory_region().is_none() {
            scope.record_failure(markdown_memory::MemoryError::Unfunded);
            control.cancel();
        }
    }
    control.checkpoint()?;
    let mut statements = StatementArena::default();
    let mut emitted = 0usize;
    let tokens = Lexer::controlled(code, control).take_while(|_| {
        let active = !emitted.is_multiple_of(128) || control.checkpoint().is_ok();
        emitted = emitted.saturating_add(1);
        active
    });
    let parsed =
        flowchart_grammar::FlowchartAstParser::new().parse(&mut statements, control, tokens);
    // Parser EOF and user errors are subordinate to the sticky operation outcome.
    control.checkpoint()?;
    Ok(parsed.map_err(|error| {
        if control.memory_scope().is_some() {
            let span = match &error {
                lalrpop_util::ParseError::InvalidToken { location }
                | lalrpop_util::ParseError::UnrecognizedEof { location, .. } => {
                    SourceSpan::new(*location, *location)
                }
                lalrpop_util::ParseError::UnrecognizedToken { token, .. }
                | lalrpop_util::ParseError::ExtraToken { token } => {
                    SourceSpan::new(token.0, token.2)
                }
                lalrpop_util::ParseError::User { error } => {
                    error.span.unwrap_or(SourceSpan::new(0, 0))
                }
            };
            Error::diagram_parse_diagnostic(
                String::new(),
                crate::ParseDiagnostic::new(String::new())
                    .with_span(span, crate::ParseDiagnosticSpanKind::Exact),
            )
        } else {
            Error::diagram_parse_diagnostic(
                diagram_type.to_owned(),
                lalrpop_parse_diagnostic(&error, code.len()),
            )
        }
    }))
}

#[cfg(any(feature = "diagram-flowchart", feature = "diagram-swimlane"))]
fn parse_flowchart_semantic_source_from_ast_controlled(
    ast: FlowchartAst,
    acc_title: Option<String>,
    acc_descr: Option<String>,
    meta: &ParseMetadata,
    control: &OperationControl,
) -> OperationControlResult<Result<FlowchartSemanticSource>> {
    let shape_data_documents = prepare_flowchart_shape_data(&ast, control)?;
    control.checkpoint()?;
    let inherit_dir = meta
        .effective_config
        .as_value()
        .get("flowchart")
        .and_then(|v| v.get("inheritDir"))
        .and_then(|v| v.as_bool())
        .unwrap_or(false);
    let mut builder = SubgraphBuilder::new_controlled(
        inherit_dir,
        crate::native_collections::clone_owned(&ast.direction, control)?,
        control,
    );
    builder.visit_statements(&ast, control)?;

    let mut subgraph_ids = crate::native_collections::new_set(control);
    for subgraph in &builder.subgraphs {
        let id = crate::native_collections::clone_owned(&subgraph.id, control)?;
        crate::native_collections::insert_set(&mut subgraph_ids, id, control)?;
    }

    let mut build = FlowchartBuildState::new_controlled(subgraph_ids, control);
    build.add_statements(&ast, control)?;
    let FlowchartBuildState { nodes, edges, .. } = build;
    let mut nodes = nodes;
    let mut edges = edges;

    let mut class_defs: IndexMap<String, Vec<String>> = control
        .memory_region()
        .map(|region| IndexMap::new_in(region.clone()))
        .unwrap_or_default();
    let mut subgraph_vertex_styles = FlowchartRenderStyleSources::new_controlled(control);
    let mut collapsed_subgraphs = memory::new_set(control);
    let mut vertex_calls = Vec::new();
    let mut warning_facts = Vec::new();
    let mut tooltips: HashMap<String, String> = memory::new_map(control);
    let mut edge_defaults = EdgeDefaults {
        style: Vec::new(),
        interpolate: None,
    };

    let mut node_index: HashMap<String, usize> = memory::new_map(control);
    for (idx, n) in nodes.iter().enumerate() {
        if idx % 128 == 0 {
            control.checkpoint()?;
        }
        memory::insert(
            &mut node_index,
            memory::clone_owned(&n.id, control)?,
            idx,
            control,
        )?;
    }
    let security_level_loose = meta.effective_config.get_str("securityLevel") == Some("loose");
    {
        let mut semantic_ctx = FlowchartSemanticContext {
            nodes: &mut nodes,
            node_index: &mut node_index,
            edges: &mut edges,
            subgraphs: &mut builder.subgraphs,
            subgraph_declaration_owners: &builder.declaration_owners,
            subgraph_vertex_styles: &mut subgraph_vertex_styles,
            collapsed_subgraphs: &mut collapsed_subgraphs,
            vertex_calls: &mut vertex_calls,
            warning_facts: &mut warning_facts,
            class_defs: &mut class_defs,
            tooltips: &mut tooltips,
            edge_defaults: &mut edge_defaults,
            security_level_loose,
            diagram_type: &meta.diagram_type,
            config: Some(&meta.effective_config),
            shape_data_documents: &shape_data_documents,
            #[cfg(feature = "diagram-flowchart")]
            native_shape_documents: None,
            control,
        };
        if let Err(error) = apply_semantic_statements(&ast, &mut semantic_ctx)? {
            return Ok(Err(error));
        }
    }

    let direction = ast.direction;
    if control.memory_region().is_some() {
        if direction.is_none() {
            let warning = DiagramWarningFact::new(
                memory::owned_string(FLOWCHART_EXPLICIT_DIRECTION_WARNING_RULE_ID, control)?,
                memory::owned_string("flowchart headers should declare an explicit direction such as `TB`, `TD`, `BT`, `LR`, or `RL`", control)?,
            ).with_span(ast.header_span)
                .with_fix_span(SourceSpan::new(ast.header_span.end, ast.header_span.end));
            memory::push(&mut warning_facts, warning, control)?;
        }
    } else {
        warning_facts.extend(flowchart_warning_facts(&direction, ast.header_span));
    }
    control.checkpoint()?;
    Ok(Ok(FlowchartSemanticSource {
        keyword: ast.keyword,
        direction,
        acc_descr,
        acc_title,
        class_defs,
        tooltips,
        edge_defaults,
        vertex_calls,
        nodes,
        edges,
        subgraphs: builder.subgraphs,
        subgraph_vertex_styles,
        collapsed_subgraphs,
        warning_facts,
    }))
}

#[cfg(any(feature = "diagram-flowchart", feature = "diagram-swimlane"))]
fn prepare_flowchart_shape_data(
    ast: &FlowchartAst,
    control: &OperationControl,
) -> OperationControlResult<HashMap<String, std::result::Result<Value, String>>> {
    let mut documents = memory::new_map(control);
    for (visited, event) in ast.walk_controlled(control)?.enumerate() {
        let event = event?;
        if visited.is_multiple_of(128) {
            control.checkpoint()?;
        }
        let StatementEvent::Enter(statement) = event else {
            continue;
        };

        match statement {
            Stmt::Chain { node_groups, .. } => {
                for (index, node) in node_groups.iter().flatten().enumerate() {
                    if index % 128 == 0 {
                        control.checkpoint()?;
                    }
                    if let Some(source) = node.shape_data.as_deref() {
                        prepare_flowchart_shape_data_document(source, control, &mut documents)?;
                    }
                }
            }
            Stmt::Node(node) => {
                if let Some(source) = node.shape_data.as_deref() {
                    prepare_flowchart_shape_data_document(source, control, &mut documents)?;
                }
            }
            Stmt::Subgraph(_) => {}
            Stmt::ShapeData { yaml, .. } => {
                prepare_flowchart_shape_data_document(yaml, control, &mut documents)?;
            }
            Stmt::Direction(_)
            | Stmt::Style(_)
            | Stmt::ClassDef(_)
            | Stmt::ClassAssign(_)
            | Stmt::Click(_)
            | Stmt::LinkStyle(_) => {}
        }
    }

    control.checkpoint()?;
    Ok(documents)
}

#[cfg(any(feature = "diagram-flowchart", feature = "diagram-swimlane"))]
fn prepare_flowchart_shape_data_document(
    source: &str,
    control: &OperationControl,
    documents: &mut HashMap<String, std::result::Result<Value, String>>,
) -> OperationControlResult<()> {
    if documents.contains_key(source) {
        return control.checkpoint();
    }
    let document = crate::inline_config::parse_mermaid_inline_object_controlled(source, control)?;
    memory::insert(
        documents,
        memory::owned_string(source, control)?,
        document,
        control,
    )?;
    Ok(())
}

#[cfg(any(feature = "diagram-flowchart", feature = "diagram-swimlane"))]
fn parse_flowchart_ast(code: &str, meta: &ParseMetadata) -> Result<FlowchartAst> {
    parse_flowchart_ast_controlled(code, meta, &OperationControl::new())?
}

#[cfg(any(feature = "diagram-flowchart", feature = "diagram-swimlane"))]
type FlowchartAstParseError = lalrpop_util::ParseError<usize, Tok, LexError>;
#[cfg(any(feature = "diagram-flowchart", feature = "diagram-swimlane"))]
type FlowchartToken = (usize, Tok, usize);
#[cfg(any(feature = "diagram-flowchart", feature = "diagram-swimlane"))]
type FlowchartLexerItem = std::result::Result<FlowchartToken, LexError>;

#[cfg(any(feature = "diagram-flowchart", feature = "diagram-swimlane"))]
enum FlowchartTracedItem {
    Token(FlowchartToken),
    RecoveredToken {
        token: FlowchartToken,
        strict_error: LexError,
    },
    LexerError(LexError),
}

#[cfg(any(feature = "diagram-flowchart", feature = "diagram-swimlane"))]
struct FlowchartTokenTrace {
    items: Vec<FlowchartTracedItem>,
}

#[cfg(any(feature = "diagram-flowchart", feature = "diagram-swimlane"))]
impl FlowchartTokenTrace {
    fn parser_items<'a>(
        &'a self,
        control: &'a OperationControl,
    ) -> impl Iterator<Item = FlowchartLexerItem> + 'a {
        let mut emitted = 0usize;
        self.items
            .iter()
            .take_while(move |_| {
                let active = !emitted.is_multiple_of(128) || control.checkpoint().is_ok();
                emitted = emitted.saturating_add(1);
                active
            })
            .map(|item| match item {
                FlowchartTracedItem::Token(token) => Ok(token.clone()),
                FlowchartTracedItem::RecoveredToken { strict_error, .. }
                | FlowchartTracedItem::LexerError(strict_error) => Err(strict_error.clone()),
            })
    }

    fn editor_tokens(&self) -> impl Iterator<Item = &FlowchartToken> {
        self.items.iter().filter_map(|item| match item {
            FlowchartTracedItem::Token(token)
            | FlowchartTracedItem::RecoveredToken { token, .. } => Some(token),
            FlowchartTracedItem::LexerError(_) => None,
        })
    }
}

#[cfg(any(feature = "diagram-flowchart", feature = "diagram-swimlane"))]
fn construct_flowchart_token_trace(
    code: &str,
    control: &OperationControl,
) -> OperationControlResult<FlowchartTokenTrace> {
    #[cfg(all(test, any(feature = "diagram-flowchart", feature = "diagram-swimlane")))]
    FLOWCHART_TOKEN_TRACE_CONSTRUCTION_COUNT.set(
        FLOWCHART_TOKEN_TRACE_CONSTRUCTION_COUNT
            .get()
            .saturating_add(1),
    );

    let mut items = Vec::new();
    for item in Lexer::recovering(code) {
        if items.len() % 128 == 0 {
            control.checkpoint()?;
        }
        match item {
            Ok(mut token) => {
                let strict_error = match &mut token.1 {
                    Tok::NodeLabel(label) => label.recovery_error.take(),
                    Tok::DirectionStmt(direction) => direction.recovery_error.take(),
                    Tok::Arrow(arrow) => arrow.recovery_error.take(),
                    Tok::ClickStmt(click) => click.recovery_error.take(),
                    _ => None,
                };
                items.push(match strict_error {
                    Some(strict_error) => FlowchartTracedItem::RecoveredToken {
                        token,
                        strict_error,
                    },
                    None => FlowchartTracedItem::Token(token),
                });
            }
            Err(error) => items.push(FlowchartTracedItem::LexerError(error)),
        }
    }

    control.checkpoint()?;
    Ok(FlowchartTokenTrace { items })
}

#[cfg(any(feature = "diagram-flowchart", feature = "diagram-swimlane"))]
fn parse_flowchart_ast_from_trace(
    trace: &FlowchartTokenTrace,
    control: &OperationControl,
) -> OperationControlResult<std::result::Result<FlowchartAst, Box<FlowchartAstParseError>>> {
    control.checkpoint()?;
    let mut statements = StatementArena::default();
    let parsed = flowchart_grammar::FlowchartAstParser::new()
        .parse(&mut statements, control, trace.parser_items(control))
        .map_err(Box::new);
    control.checkpoint()?;
    Ok(parsed)
}

#[cfg(any(feature = "diagram-flowchart", feature = "diagram-swimlane"))]
fn flowchart_warning_facts(
    direction: &Option<String>,
    header_span: crate::SourceSpan,
) -> Vec<DiagramWarningFact> {
    if direction.is_some() {
        return Vec::new();
    }

    vec![
        DiagramWarningFact::new(
            FLOWCHART_EXPLICIT_DIRECTION_WARNING_RULE_ID,
            "flowchart headers should declare an explicit direction such as `TB`, `TD`, `BT`, `LR`, or `RL`",
        )
        .with_span(header_span)
        .with_fix_span(crate::SourceSpan::new(header_span.end, header_span.end)),
    ]
}

#[cfg(any(feature = "diagram-flowchart", feature = "diagram-swimlane"))]
fn editor_facts_from_flowchart_ast(
    ast: &FlowchartAst,
    control: &OperationControl,
) -> OperationControlResult<EditorSemanticFacts> {
    let mut facts = EditorSemanticFacts::new();
    collect_editor_facts_from_statements(ast, &mut facts, control)?;
    Ok(facts)
}

#[cfg(any(feature = "diagram-flowchart", feature = "diagram-swimlane"))]
fn recover_flowchart_editor_facts_from_tokens(
    code: &str,
    trace: FlowchartTokenTrace,
    control: &OperationControl,
) -> OperationControlResult<EditorSemanticFacts> {
    let mut facts = EditorSemanticFacts::new();
    facts.mark_recovered();
    let mut collector = FlowchartRecoveryFactCollector::default();
    for (index, (start, token, end)) in trace.editor_tokens().enumerate() {
        if index % 128 == 0 {
            control.checkpoint()?;
        }
        collector.accept(code, token, *start, *end, &mut facts);
    }
    collector.finish(code.len(), &mut facts);
    for item in &trace.items {
        let error = match item {
            FlowchartTracedItem::RecoveredToken { strict_error, .. }
            | FlowchartTracedItem::LexerError(strict_error) => Some(strict_error),
            FlowchartTracedItem::Token(_) => None,
        };
        if let Some(error) = error {
            push_flowchart_expected_syntax(&mut facts, error.expected_syntax.iter().copied());
        }
        if let Some(prefix) = error.and_then(|error| error.directive_prefix) {
            facts.push_directive_prefix(prefix);
        }
    }
    control.checkpoint()?;
    Ok(facts)
}

#[cfg(any(feature = "diagram-flowchart", feature = "diagram-swimlane"))]
fn flowchart_recovery_facts(
    parser_code: &str,
    trace: FlowchartTokenTrace,
    accessibility_statements: &[FlowchartAccessibilityStatement],
    error: &FlowchartAstParseError,
    control: &OperationControl,
) -> OperationControlResult<EditorSemanticFacts> {
    let mut facts = recover_flowchart_editor_facts_from_tokens(parser_code, trace, control)?;
    collect_accessibility_directive_prefixes(accessibility_statements, &mut facts, control)?;
    let span = flowchart_eof_recovery_insertion(error, parser_code, &facts)
        .map(|insertion| SourceSpan::new(insertion, insertion))
        .unwrap_or_else(|| lalrpop_recovery_span(error, parser_code.len()));
    facts.mark_recovered_from_parse_error(
        format!(
            "flowchart parser recovered after parse error: {}",
            format_lalrpop_parse_error(error)
        ),
        Some(span),
    );
    control.checkpoint()?;
    Ok(facts)
}

#[cfg(any(feature = "diagram-flowchart", feature = "diagram-swimlane"))]
fn flowchart_parse_diagnostic(
    error: &FlowchartAstParseError,
    code: &str,
    facts: &EditorSemanticFacts,
) -> crate::ParseDiagnostic {
    let diagnostic = lalrpop_parse_diagnostic(error, code.len());
    match flowchart_eof_recovery_insertion(error, code, facts) {
        Some(insertion) => diagnostic.map_span(|_| SourceSpan::new(insertion, insertion)),
        None => diagnostic,
    }
}

#[cfg(any(feature = "diagram-flowchart", feature = "diagram-swimlane"))]
fn flowchart_eof_recovery_insertion(
    error: &FlowchartAstParseError,
    code: &str,
    facts: &EditorSemanticFacts,
) -> Option<usize> {
    if !matches!(error, lalrpop_util::ParseError::UnrecognizedEof { .. }) {
        return None;
    }
    let insertion = code.trim_end_matches(['\r', '\n']).len();
    (insertion < code.len()
        && facts.expected_syntax.iter().any(|expected| {
            matches!(
                expected.kind,
                EditorExpectedSyntaxKind::NodeIdentifier
                    | EditorExpectedSyntaxKind::FlowchartOperator
            ) && expected.span.end == code.len()
        }))
    .then_some(insertion)
}

#[cfg(any(feature = "diagram-flowchart", feature = "diagram-swimlane"))]
fn collect_expected_syntax_from_tokens<'a>(
    code: &str,
    tokens: impl Iterator<Item = &'a FlowchartToken>,
    facts: &mut EditorSemanticFacts,
    control: &OperationControl,
) -> OperationControlResult<()> {
    for (index, (start, token, end)) in tokens.enumerate() {
        if index % 128 == 0 {
            control.checkpoint()?;
        }
        match token {
            Tok::NodeLabel(label) => {
                if let Some(trigger_span) = label.trigger_span {
                    push_flowchart_shape_trigger_expected_syntax(trigger_span, facts);
                }
            }
            Tok::ShapeData(_) => {
                push_flowchart_shape_value_expected_syntax(code, *start, *end, facts)
            }
            Tok::DirectionStmt(stmt) => {
                push_flowchart_direction_value_expected_syntax(stmt.selection, facts)
            }
            _ => {}
        }
    }
    control.checkpoint()
}

#[derive(Debug, Default)]
#[cfg(any(feature = "diagram-flowchart", feature = "diagram-swimlane"))]
struct FlowchartRecoveryFactCollector {
    pending_node_identifier: Option<FlowchartRecoveryTargetState>,
    seen_entities: HashSet<String>,
}

#[derive(Debug, Clone, Copy)]
#[cfg(any(feature = "diagram-flowchart", feature = "diagram-swimlane"))]
enum FlowchartRecoveryTargetState {
    Awaiting(SourceSpan),
    Sealed(SourceSpan),
}

#[cfg(any(feature = "diagram-flowchart", feature = "diagram-swimlane"))]
impl FlowchartRecoveryFactCollector {
    fn accept(
        &mut self,
        code: &str,
        token: &Tok,
        start: usize,
        end: usize,
        facts: &mut EditorSemanticFacts,
    ) {
        enum TokenKind {
            Arrow,
            EdgeLabel,
            Id,
            Sep,
            Other,
        }

        let token_kind = match token {
            Tok::Arrow(_) => TokenKind::Arrow,
            Tok::EdgeLabel(_) => TokenKind::EdgeLabel,
            Tok::Id(_) => TokenKind::Id,
            Tok::Sep => TokenKind::Sep,
            _ => TokenKind::Other,
        };
        collect_editor_fact_from_token(code, token, start, end, facts, &mut self.seen_entities);
        match token_kind {
            TokenKind::Arrow => {
                self.pending_node_identifier = Some(FlowchartRecoveryTargetState::Awaiting(
                    SourceSpan::new(end, end),
                ));
            }
            TokenKind::EdgeLabel => {
                if matches!(
                    self.pending_node_identifier,
                    Some(FlowchartRecoveryTargetState::Awaiting(_))
                ) {
                    self.pending_node_identifier = Some(FlowchartRecoveryTargetState::Awaiting(
                        SourceSpan::new(end, end),
                    ));
                }
            }
            TokenKind::Id => {
                self.pending_node_identifier = None;
            }
            TokenKind::Sep => {
                if let Some(FlowchartRecoveryTargetState::Awaiting(mut span)) =
                    self.pending_node_identifier.take()
                {
                    span.end = start;
                    self.pending_node_identifier = Some(FlowchartRecoveryTargetState::Sealed(span));
                }
            }
            TokenKind::Other => {
                self.pending_node_identifier = None;
            }
        }
    }

    fn finish(self, code_len: usize, facts: &mut EditorSemanticFacts) {
        let Some(state) = self.pending_node_identifier else {
            return;
        };

        let span = match state {
            FlowchartRecoveryTargetState::Awaiting(mut span) => {
                span.end = code_len;
                span
            }
            FlowchartRecoveryTargetState::Sealed(span) => span,
        };

        if span.end >= span.start {
            facts.push_expected_syntax(EditorExpectedSyntax::new(
                EditorExpectedSyntaxKind::NodeIdentifier,
                span,
            ));
        }
    }
}

#[cfg(any(feature = "diagram-flowchart", feature = "diagram-swimlane"))]
fn collect_editor_fact_from_token(
    code: &str,
    token: &Tok,
    start: usize,
    end: usize,
    facts: &mut EditorSemanticFacts,
    seen_entities: &mut HashSet<String>,
) {
    match token {
        Tok::Id(id) => push_flowchart_token_symbol(facts, id, start, end, seen_entities),
        Tok::SubgraphHeader(header) => {
            push_flowchart_header_symbol(facts, header);
        }
        Tok::NodeLabel(label) => {
            if let Some(trigger_span) = label.trigger_span {
                push_flowchart_shape_trigger_expected_syntax(trigger_span, facts);
            }
            push_flowchart_labeled_payload_symbol(
                facts,
                &label.text,
                Some(SourceSpan::new(start, end)),
                "flowchart node label",
            )
        }
        Tok::EdgeLabel(label) => push_flowchart_labeled_payload_symbol(
            facts,
            label,
            Some(SourceSpan::new(start, end)),
            "flowchart edge label",
        ),
        Tok::StyleStmt(stmt) => push_flowchart_style_stmt_facts(facts, stmt),
        Tok::ClassDefStmt(stmt) => push_flowchart_classdef_stmt_facts(facts, stmt),
        Tok::ClassAssignStmt(stmt) => push_flowchart_class_assign_stmt_facts(facts, stmt),
        Tok::ClickStmt(stmt) => push_flowchart_click_stmt_facts(facts, stmt),
        Tok::LinkStyleStmt(_) => facts.push_directive_prefix("linkStyle"),
        Tok::KwGraph
        | Tok::KwFlowchart
        | Tok::KwFlowchartElk
        | Tok::KwSwimlane
        | Tok::KwSubgraph
        | Tok::KwEnd
        | Tok::Sep
        | Tok::Amp
        | Tok::StyleSep
        | Tok::Direction(_) => {}
        Tok::DirectionStmt(stmt) => {
            push_flowchart_direction_value_expected_syntax(stmt.selection, facts)
        }
        Tok::Arrow(_) | Tok::EdgeId(_) => {}
        Tok::ShapeData(_) => push_flowchart_shape_value_expected_syntax(code, start, end, facts),
    }
}

#[cfg(any(feature = "diagram-flowchart", feature = "diagram-swimlane"))]
fn collect_editor_facts_from_statements(
    ast: &FlowchartAst,
    facts: &mut EditorSemanticFacts,
    control: &OperationControl,
) -> OperationControlResult<()> {
    let mut emitted_edge_label_spans = HashSet::default();
    let mut seen_edge_ids = HashSet::default();
    let mut seen_entities = HashSet::default();
    collect_editor_facts_from_statements_with_seen_edges(
        ast,
        facts,
        &mut emitted_edge_label_spans,
        &mut seen_edge_ids,
        &mut seen_entities,
        control,
    )
}

#[cfg(any(feature = "diagram-flowchart", feature = "diagram-swimlane"))]
fn collect_editor_facts_from_statements_with_seen_edges(
    ast: &FlowchartAst,
    facts: &mut EditorSemanticFacts,
    emitted_edge_label_spans: &mut HashSet<(usize, usize)>,
    seen_edge_ids: &mut HashSet<String>,
    seen_entities: &mut HashSet<String>,
    control: &OperationControl,
) -> OperationControlResult<()> {
    for (visited, event) in ast.walk().enumerate() {
        if visited.is_multiple_of(128) {
            control.checkpoint()?;
        }
        let StatementEvent::Enter(stmt) = event else {
            continue;
        };

        match stmt {
            Stmt::Chain {
                node_groups,
                edge_groups,
            } => {
                for (index, node) in node_groups.iter().flatten().enumerate() {
                    if index % 128 == 0 {
                        control.checkpoint()?;
                    }
                    let occurrence =
                        if edge_groups.is_empty() || node_defines_flowchart_entity(node) {
                            FlowchartNodeOccurrence::Definition
                        } else {
                            FlowchartNodeOccurrence::RelationEndpoint
                        };
                    push_flowchart_node_symbol(facts, node, seen_entities, occurrence);
                }
                for (index, edge) in edge_groups.iter().flatten().enumerate() {
                    if index % 128 == 0 {
                        control.checkpoint()?;
                    }
                    push_flowchart_edge_label_symbol(facts, edge, emitted_edge_label_spans);
                    if let Some(id) = edge.id.as_deref() {
                        seen_edge_ids.insert(id.to_string());
                    }
                }
            }
            Stmt::Node(node) => push_flowchart_node_symbol(
                facts,
                node,
                seen_entities,
                FlowchartNodeOccurrence::Definition,
            ),
            Stmt::Subgraph(subgraph) => {
                push_flowchart_subgraph_symbol(facts, subgraph);
            }
            Stmt::Style(stmt) => push_flowchart_style_stmt_facts(facts, stmt),
            Stmt::ClassDef(stmt) => push_flowchart_classdef_stmt_facts(facts, stmt),
            Stmt::ClassAssign(stmt) => push_flowchart_class_assign_stmt_facts(facts, stmt),
            Stmt::Click(stmt) => push_flowchart_click_stmt_facts(facts, stmt),
            Stmt::LinkStyle(_) => facts.push_directive_prefix("linkStyle"),
            Stmt::ShapeData {
                target,
                target_span,
                ..
            } => {
                if !seen_edge_ids.contains(target) {
                    seen_entities.insert(target.clone());
                    push_flowchart_span_symbol(
                        facts,
                        target,
                        "flowchart node",
                        EditorSemanticKind::Module,
                        *target_span,
                        EditorSemanticRole::Entity,
                    );
                }
            }
            Stmt::Direction(_) => {}
        }
    }
    control.checkpoint()
}

#[cfg(any(feature = "diagram-flowchart", feature = "diagram-swimlane"))]
fn push_flowchart_node_symbol(
    facts: &mut EditorSemanticFacts,
    node: &Node,
    seen_entities: &mut HashSet<String>,
    occurrence: FlowchartNodeOccurrence,
) {
    if let Some(span) = node.id_span {
        let role = match occurrence {
            FlowchartNodeOccurrence::Definition => {
                seen_entities.insert(node.id.clone());
                EditorSemanticRole::Entity
            }
            FlowchartNodeOccurrence::RelationEndpoint => {
                if seen_entities.insert(node.id.clone()) {
                    EditorSemanticRole::Entity
                } else {
                    EditorSemanticRole::Reference
                }
            }
        };
        push_flowchart_span_symbol(
            facts,
            &node.id,
            "flowchart node",
            EditorSemanticKind::Module,
            Some(span),
            role,
        );
    }

    if let Some(label) = node.label.as_deref() {
        push_flowchart_payload_symbol(
            facts,
            label,
            "flowchart node label",
            node.label_span,
            node.label_selection,
        );
    }
}

#[derive(Debug, Clone, Copy)]
#[cfg(any(feature = "diagram-flowchart", feature = "diagram-swimlane"))]
enum FlowchartNodeOccurrence {
    Definition,
    RelationEndpoint,
}

#[cfg(any(feature = "diagram-flowchart", feature = "diagram-swimlane"))]
fn node_defines_flowchart_entity(node: &Node) -> bool {
    node.label.is_some() || node.shape_data.is_some() || !node.classes.is_empty()
}

#[cfg(any(feature = "diagram-flowchart", feature = "diagram-swimlane"))]
fn push_flowchart_edge_label_symbol(
    facts: &mut EditorSemanticFacts,
    edge: &Edge,
    emitted_spans: &mut HashSet<(usize, usize)>,
) {
    let Some(label) = edge.label.as_deref() else {
        return;
    };
    let Some(span) = edge.label_span else {
        return;
    };
    if !emitted_spans.insert((span.start, span.end)) {
        return;
    }
    push_flowchart_payload_symbol(
        facts,
        label,
        "flowchart edge label",
        Some(span),
        edge.label_selection,
    );
}

#[cfg(any(feature = "diagram-flowchart", feature = "diagram-swimlane"))]
fn push_flowchart_style_stmt_facts(facts: &mut EditorSemanticFacts, stmt: &StyleStmt) {
    facts.push_directive_prefix("style");
    push_flowchart_expected_syntax(facts, stmt.editor_evidence.iter());
    push_flowchart_span_symbol(
        facts,
        &stmt.target,
        "flowchart style target",
        EditorSemanticKind::Module,
        stmt.target_span,
        EditorSemanticRole::Reference,
    );
    if let (Some(text), Some(span)) = (stmt.styles_text.as_deref(), stmt.styles_span) {
        push_flowchart_payload_symbol(facts, text, "flowchart style", Some(span), Some(span));
    }
}

#[cfg(any(feature = "diagram-flowchart", feature = "diagram-swimlane"))]
fn push_flowchart_classdef_stmt_facts(facts: &mut EditorSemanticFacts, stmt: &ClassDefStmt) {
    facts.push_directive_prefix("classDef");
    push_flowchart_expected_syntax(facts, stmt.editor_evidence.iter());
    for (id, span) in stmt.ids.iter().zip(stmt.id_spans.iter().copied()) {
        push_flowchart_span_symbol(
            facts,
            id,
            "flowchart class definition",
            EditorSemanticKind::Property,
            Some(span),
            EditorSemanticRole::ClassDefinition,
        );
    }
    if let (Some(text), Some(span)) = (stmt.styles_text.as_deref(), stmt.styles_span) {
        push_flowchart_payload_symbol(
            facts,
            text,
            "flowchart class definition style",
            Some(span),
            Some(span),
        );
    }
}

#[cfg(any(feature = "diagram-flowchart", feature = "diagram-swimlane"))]
fn push_flowchart_class_assign_stmt_facts(facts: &mut EditorSemanticFacts, stmt: &ClassAssignStmt) {
    facts.push_directive_prefix("class");
    push_flowchart_expected_syntax(facts, stmt.editor_evidence.iter());
    for (target, span) in stmt.targets.iter().zip(stmt.target_spans.iter().copied()) {
        push_flowchart_span_symbol(
            facts,
            target,
            "flowchart class target",
            EditorSemanticKind::Module,
            Some(span),
            EditorSemanticRole::Reference,
        );
    }
    push_flowchart_span_symbol(
        facts,
        &stmt.class_name,
        "flowchart class name",
        EditorSemanticKind::Property,
        stmt.class_name_span,
        EditorSemanticRole::Payload,
    );
}

#[cfg(any(feature = "diagram-flowchart", feature = "diagram-swimlane"))]
fn push_flowchart_click_stmt_facts(facts: &mut EditorSemanticFacts, stmt: &ClickStmt) {
    facts.push_directive_prefix("click");
    push_flowchart_expected_syntax(facts, stmt.editor_evidence.iter());
    push_flowchart_expected_syntax(facts, stmt.interaction_evidence.iter());
    for (id, span) in stmt.ids.iter().zip(stmt.id_spans.iter().copied()) {
        push_flowchart_span_symbol(
            facts,
            id,
            "flowchart interaction target",
            EditorSemanticKind::Module,
            Some(span),
            EditorSemanticRole::Reference,
        );
    }
}

#[cfg(any(feature = "diagram-flowchart", feature = "diagram-swimlane"))]
fn push_flowchart_expected_syntax(
    facts: &mut EditorSemanticFacts,
    expected_syntax: impl IntoIterator<Item = EditorExpectedSyntax>,
) {
    for expected in expected_syntax {
        facts.push_expected_syntax(expected);
    }
}

#[cfg(any(feature = "diagram-flowchart", feature = "diagram-swimlane"))]
fn push_flowchart_span_symbol(
    facts: &mut EditorSemanticFacts,
    name: &str,
    detail: &'static str,
    kind: EditorSemanticKind,
    span: Option<SourceSpan>,
    role: EditorSemanticRole,
) {
    let Some(span) = span else {
        return;
    };
    if name.is_empty() {
        return;
    }
    let symbol = EditorSemanticSymbol::with_role(
        name.to_string(),
        Some(detail.to_string()),
        kind,
        role,
        span,
        span,
    );
    facts.push_symbol(
        if matches!(
            role,
            EditorSemanticRole::Entity | EditorSemanticRole::Reference
        ) {
            symbol.with_rename_policy(EditorRenamePolicy::FlowchartNodeId)
        } else {
            symbol
        },
    );
}

#[cfg(any(feature = "diagram-flowchart", feature = "diagram-swimlane"))]
fn push_flowchart_labeled_payload_symbol(
    facts: &mut EditorSemanticFacts,
    label: &LabeledText,
    fallback_span: Option<SourceSpan>,
    detail: &'static str,
) {
    push_flowchart_payload_symbol(
        facts,
        &label.text,
        detail,
        label.span.or(fallback_span),
        label.selection,
    );
}

#[cfg(any(feature = "diagram-flowchart", feature = "diagram-swimlane"))]
fn push_flowchart_payload_symbol(
    facts: &mut EditorSemanticFacts,
    name: &str,
    detail: &'static str,
    span: Option<SourceSpan>,
    selection: Option<SourceSpan>,
) {
    if name.is_empty() {
        return;
    }
    let Some(span) = span else {
        return;
    };
    facts.push_expected_syntax(EditorExpectedSyntax::new(
        EditorExpectedSyntaxKind::Payload,
        selection.unwrap_or(span),
    ));
    facts.push_symbol(EditorSemanticSymbol::payload(
        name.to_string(),
        Some(detail.to_string()),
        EditorSemanticKind::String,
        span,
        selection.unwrap_or(span),
    ));
}

#[cfg(any(feature = "diagram-flowchart", feature = "diagram-swimlane"))]
fn push_flowchart_shape_value_expected_syntax(
    code: &str,
    start: usize,
    end: usize,
    facts: &mut EditorSemanticFacts,
) {
    let Some(span) = shape_value_expected_span(code, start, end) else {
        return;
    };

    facts.push_expected_syntax(EditorExpectedSyntax::new(
        EditorExpectedSyntaxKind::ShapeValue,
        span,
    ));
}

#[cfg(any(feature = "diagram-flowchart", feature = "diagram-swimlane"))]
fn push_flowchart_direction_value_expected_syntax(
    span: SourceSpan,
    facts: &mut EditorSemanticFacts,
) {
    facts.push_expected_syntax(EditorExpectedSyntax::new(
        EditorExpectedSyntaxKind::FlowchartDirectionValue,
        span,
    ));
}

#[cfg(any(feature = "diagram-flowchart", feature = "diagram-swimlane"))]
fn push_flowchart_shape_trigger_expected_syntax(span: SourceSpan, facts: &mut EditorSemanticFacts) {
    facts.push_expected_syntax(EditorExpectedSyntax::new(
        EditorExpectedSyntaxKind::ShapeTrigger,
        span,
    ));
}

pub(super) fn shape_value_expected_span(
    code: &str,
    start: usize,
    end: usize,
) -> Option<SourceSpan> {
    let raw = code.get(start..end)?;
    let body = raw.strip_prefix("@{")?;
    let body = body.strip_suffix('}').unwrap_or(body);
    let body_base = start + 2;
    let mut pos = 0usize;
    let mut in_string: Option<char> = None;
    let mut depth = 0usize;

    while pos < body.len() {
        let Some(ch) = body[pos..].chars().next() else {
            break;
        };
        if let Some(quote) = in_string {
            if ch == '\\' {
                pos += ch.len_utf8();
                if pos < body.len() {
                    let Some(escaped) = body[pos..].chars().next() else {
                        break;
                    };
                    pos += escaped.len_utf8();
                }
                continue;
            }
            if ch == quote {
                in_string = None;
            }
            pos += ch.len_utf8();
            continue;
        }

        match ch {
            '"' | '\'' => {
                in_string = Some(ch);
                pos += ch.len_utf8();
            }
            '{' | '[' | '(' => {
                depth += 1;
                pos += ch.len_utf8();
            }
            '}' => {
                if depth == 0 {
                    break;
                }
                depth -= 1;
                pos += ch.len_utf8();
            }
            ']' | ')' => {
                depth = depth.saturating_sub(1);
                pos += ch.len_utf8();
            }
            _ => {
                if depth == 0 && body[pos..].starts_with("shape") && shape_key_boundary(body, pos) {
                    let mut key_end = pos + "shape".len();
                    while let Some(ch) = body[key_end..].chars().next() {
                        if ch.is_whitespace() {
                            key_end += ch.len_utf8();
                        } else {
                            break;
                        }
                    }
                    if body[key_end..].starts_with(':') {
                        let mut value_start = key_end + 1;
                        while let Some(ch) = body[value_start..].chars().next() {
                            if ch.is_whitespace() {
                                value_start += ch.len_utf8();
                            } else {
                                break;
                            }
                        }
                        let value_end = shape_value_end(body, value_start);
                        return Some(SourceSpan::new(
                            body_base + value_start,
                            body_base + value_end,
                        ));
                    }
                }
                pos += ch.len_utf8();
            }
        }
    }

    None
}

#[cfg(any(feature = "diagram-flowchart", feature = "diagram-swimlane"))]
fn collect_accessibility_directive_prefixes(
    statements: &[FlowchartAccessibilityStatement],
    facts: &mut EditorSemanticFacts,
    control: &OperationControl,
) -> OperationControlResult<()> {
    for (index, statement) in statements.iter().enumerate() {
        if index % 128 == 0 {
            control.checkpoint()?;
        }
        if statement.complete {
            facts.push_directive_prefix(statement.directive.prefix());
        }
    }
    control.checkpoint()
}

fn shape_key_boundary(body: &str, pos: usize) -> bool {
    let before = if pos == 0 {
        None
    } else {
        body[..pos].chars().next_back()
    };
    let after = body[pos + "shape".len()..].chars().next();

    before.is_none_or(|ch| !ch.is_ascii_alphanumeric() && ch != '_')
        && after.is_none_or(|ch| ch.is_whitespace() || ch == ':')
}

fn shape_value_end(body: &str, start: usize) -> usize {
    if start >= body.len() {
        return start;
    }

    let Some(first) = body[start..].chars().next() else {
        return start;
    };

    match first {
        '"' | '\'' => {
            let quote = first;
            let mut pos = start + 1;
            while pos < body.len() {
                let Some(ch) = body[pos..].chars().next() else {
                    break;
                };
                if ch == '\\' {
                    pos += ch.len_utf8();
                    if pos < body.len() {
                        let Some(escaped) = body[pos..].chars().next() else {
                            break;
                        };
                        pos += escaped.len_utf8();
                    }
                    continue;
                }
                if ch == quote {
                    pos += ch.len_utf8();
                    break;
                }
                pos += ch.len_utf8();
            }
            pos
        }
        _ => {
            let mut pos = start;
            while pos < body.len() {
                let Some(ch) = body[pos..].chars().next() else {
                    break;
                };
                match ch {
                    ',' | '}' | '\n' | '\r' | ' ' | '\t' => break,
                    _ => pos += ch.len_utf8(),
                }
            }
            pos
        }
    }
}

#[cfg(any(feature = "diagram-flowchart", feature = "diagram-swimlane"))]
fn push_flowchart_subgraph_symbol(facts: &mut EditorSemanticFacts, subgraph: &SubgraphBlock) {
    push_flowchart_header_symbol(facts, &subgraph.header);
}

#[cfg(any(feature = "diagram-flowchart", feature = "diagram-swimlane"))]
fn push_flowchart_header_symbol(facts: &mut EditorSemanticFacts, header: &SubgraphHeader) {
    let Some(span) = header.header_span.or(header.raw_id_span) else {
        return;
    };
    let Some((name, selection)) = flowchart_subgraph_symbol_id(header) else {
        return;
    };
    facts.push_symbol(
        EditorSemanticSymbol::new(
            name,
            Some("subgraph".to_string()),
            EditorSemanticKind::Namespace,
            span,
            selection,
        )
        .with_rename_policy(EditorRenamePolicy::FlowchartNodeId),
    );
}

#[cfg(any(feature = "diagram-flowchart", feature = "diagram-swimlane"))]
fn flowchart_subgraph_symbol_id(header: &SubgraphHeader) -> Option<(String, SourceSpan)> {
    if header.id_equals_title && header.raw_title.chars().any(is_ecmascript_trim_char) {
        // FlowDB replaces this authored title/id with `subGraphN`; exposing the raw title as a
        // renameable id would point editor operations at a symbol that does not exist in the model.
        return None;
    }

    let raw_span = header.raw_id_span?;
    let raw = header.raw_id.as_str();
    let mut start = 0usize;
    let mut end = raw.len();

    if raw.starts_with('"') && raw.ends_with('"') && raw.len() >= 2 {
        start += 1;
        end -= 1;
        let unquoted = &raw[start..end];
        if unquoted.starts_with('`') && unquoted.ends_with('`') && unquoted.len() >= 2 {
            start += 1;
            end -= 1;
        }
    }

    let candidate = &raw[start..end];
    let leading = candidate
        .len()
        .saturating_sub(candidate.trim_start_matches(is_ecmascript_trim_char).len());
    start += leading;
    let trimmed = &raw[start..end];
    end = start + trimmed.trim_end_matches(is_ecmascript_trim_char).len();

    let name = raw[start..end].to_string();
    if name.is_empty() {
        return None;
    }
    Some((
        name,
        SourceSpan::new(raw_span.start + start, raw_span.start + end),
    ))
}

#[cfg(any(feature = "diagram-flowchart", feature = "diagram-swimlane"))]
fn push_flowchart_token_symbol(
    facts: &mut EditorSemanticFacts,
    id: &str,
    start: usize,
    end: usize,
    seen_entities: &mut HashSet<String>,
) {
    if id.is_empty() {
        return;
    }
    let span = crate::SourceSpan::new(start, end);
    let role = if seen_entities.insert(id.to_string()) {
        EditorSemanticRole::Entity
    } else {
        EditorSemanticRole::Reference
    };
    push_flowchart_span_symbol(
        facts,
        id,
        "flowchart node",
        EditorSemanticKind::Module,
        Some(span),
        role,
    );
}

#[cfg(any(feature = "diagram-flowchart", feature = "diagram-swimlane"))]
impl FlowchartSemanticSource {
    fn into_render_model(self, meta: &ParseMetadata) -> Result<FlowchartModel> {
        self.into_render_model_parts(meta).map(|(model, _)| model)
    }

    fn into_render_model_parts(
        self,
        meta: &ParseMetadata,
    ) -> Result<(FlowchartModel, FlowchartRenderContext)> {
        let control = OperationControl::new();
        self.into_render_model_parts_controlled(meta, &control)
            .expect("a private parse control cannot be cancelled")
    }

    fn into_render_model_controlled(
        self,
        meta: &ParseMetadata,
        control: &OperationControl,
    ) -> OperationControlResult<Result<FlowchartModel>> {
        Ok(self
            .into_render_model_parts_controlled(meta, control)?
            .map(|(model, _)| model))
    }

    fn into_render_model_parts_controlled(
        self,
        meta: &ParseMetadata,
        control: &OperationControl,
    ) -> OperationControlResult<Result<(FlowchartModel, FlowchartRenderContext)>> {
        control.checkpoint()?;
        let FlowchartSemanticSource {
            acc_descr,
            acc_title,
            class_defs,
            edge_defaults,
            vertex_calls,
            mut nodes,
            edges,
            subgraphs,
            subgraph_vertex_styles,
            collapsed_subgraphs,
            warning_facts,
            tooltips,
            keyword,
            direction,
        } = self;

        if meta.diagram_type == "flowchart-elk" {
            append_missing_subgraph_nodes(&mut nodes, &subgraphs, control)?;
        }

        let mut render_nodes = Vec::new();
        memory::reserve(&mut render_nodes, nodes.len(), control)?;
        let mut render_label_sources = FlowchartRenderLabelSources::new_controlled(control);
        for (index, node) in nodes.into_iter().enumerate() {
            if index % 128 == 0 {
                control.checkpoint()?;
            }
            let (node, render_label_source) = flow_node_to_model(node, &meta.effective_config);
            if let Some(source) = render_label_source {
                render_label_sources.insert_node(
                    memory::clone_owned(&node.id, control)?,
                    source,
                    control,
                )?;
            }
            memory::push(&mut render_nodes, node, control)?;
        }
        let mut render_edges = Vec::new();
        memory::reserve(&mut render_edges, edges.len(), control)?;
        for (index, edge) in edges.into_iter().enumerate() {
            if index % 128 == 0 {
                control.checkpoint()?;
            }
            match flow_edge_to_model(edge, meta) {
                Ok((edge, render_label_source)) => {
                    if let Some(source) = render_label_source {
                        render_label_sources.insert_edge(
                            memory::clone_owned(&edge.id, control)?,
                            source,
                            control,
                        )?;
                    }
                    memory::push(&mut render_edges, edge, control)?;
                }
                Err(error) => return Ok(Err(error)),
            }
        }
        let mut render_subgraphs = Vec::new();
        memory::reserve(&mut render_subgraphs, subgraphs.len(), control)?;
        for (index, subgraph) in subgraphs.into_iter().enumerate() {
            if index % 128 == 0 {
                control.checkpoint()?;
            }
            let (subgraph, render_title_source) =
                flow_subgraph_to_model(subgraph, &meta.effective_config);
            render_label_sources.insert_subgraph(
                memory::clone_owned(&subgraph.id, control)?,
                index,
                render_title_source,
                control,
            )?;
            memory::push(&mut render_subgraphs, subgraph, control)?;
        }
        let mut render_tooltips = memory::new_map(control);
        for (index, (id, tooltip)) in tooltips.into_iter().enumerate() {
            if index % 128 == 0 {
                control.checkpoint()?;
            }
            memory::insert(&mut render_tooltips, id, tooltip, control)?;
        }

        control.checkpoint()?;
        let model = FlowchartModel {
            keyword,
            acc_descr,
            acc_title,
            class_defs,
            direction: match direction {
                Some(direction) => Some(direction),
                None => Some(memory::owned_string("TB", control)?),
            },
            edge_defaults: Some(FlowEdgeDefaults {
                style: edge_defaults.style,
                interpolate: edge_defaults.interpolate,
            }),
            vertex_calls,
            nodes: render_nodes,
            edges: render_edges,
            subgraphs: render_subgraphs,
            tooltips: render_tooltips,
            warning_facts,
        };
        let render_context = FlowchartRenderContext::new_controlled(
            render_label_sources,
            subgraph_vertex_styles,
            collapsed_subgraphs,
            &model.subgraphs,
            control,
        )?;
        Ok(Ok((model, render_context)))
    }
}

#[cfg(any(feature = "diagram-flowchart", feature = "diagram-swimlane"))]
fn append_missing_subgraph_nodes(
    nodes: &mut Vec<Node>,
    subgraphs: &[FlowSubGraph],
    control: &OperationControl,
) -> OperationControlResult<()> {
    let mut existing_ids = memory::new_set(control);
    for (index, node) in nodes.iter().enumerate() {
        if index % 128 == 0 {
            control.checkpoint()?;
        }
        memory::insert_set(
            &mut existing_ids,
            memory::clone_owned(&node.id, control)?,
            control,
        )?;
    }
    for (index, subgraph) in subgraphs.iter().enumerate() {
        if index % 128 == 0 {
            control.checkpoint()?;
        }
        if memory::insert_set(
            &mut existing_ids,
            memory::clone_owned(&subgraph.id, control)?,
            control,
        )? {
            nodes.push(Node {
                id: subgraph.id.clone(),
                provenance: FlowNodeProvenance::SubgraphAnchor,
                syntax: FlowNodeSyntax::BareReference,
                id_span: None,
                label: None,
                label_type: TitleKind::Text,
                label_span: None,
                label_selection: None,
                shape: None,
                shape_data: None,
                icon: None,
                form: None,
                pos: None,
                img: None,
                constraint: None,
                asset_width: None,
                asset_height: None,
                styles: Vec::new(),
                classes: Vec::new(),
                link: None,
                link_target: None,
                have_callback: false,
            });
        }
    }
    control.checkpoint()
}

#[cfg(any(feature = "diagram-flowchart", feature = "diagram-swimlane"))]
fn flow_node_to_model(n: Node, config: &MermaidConfig) -> (FlowNode, Option<String>) {
    let layout_shape = layout_shape_for_node(&n);
    let label = sanitized_node_label(&n, config);
    let label_raw = n.label.as_deref().unwrap_or(&n.id);
    let render_label_source = render_label_source_needs_provenance(label_raw)
        .then(|| sanitized_node_render_label_source(&n, config))
        .filter(|source| *source != label);

    let node = FlowNode {
        id: n.id,
        provenance: n.provenance,
        label: Some(label),
        label_type: Some(title_kind_str(&n.label_type).to_string()),
        layout_shape: Some(layout_shape),
        shape: n.shape,
        icon: n.icon,
        form: n.form,
        pos: n.pos,
        img: n.img,
        constraint: n.constraint,
        asset_width: n.asset_width,
        asset_height: n.asset_height,
        classes: n.classes,
        styles: n.styles,
        link: n.link,
        link_target: n.link_target,
        have_callback: n.have_callback,
    };
    (node, render_label_source)
}

#[cfg(any(feature = "diagram-flowchart", feature = "diagram-swimlane"))]
fn flow_edge_to_model(e: Edge, meta: &ParseMetadata) -> Result<(FlowEdge, Option<String>)> {
    let label = sanitized_optional_label(e.label.as_deref(), &meta.effective_config);
    let render_label_source = e
        .label
        .as_deref()
        .filter(|raw| render_label_source_needs_provenance(raw))
        .map(|raw| sanitized_render_label_source(raw, &meta.effective_config));
    let render_label_source = match (&label, render_label_source) {
        (Some(label), Some(source)) if *label != source => Some(source),
        _ => None,
    };
    let id = e.id.ok_or_else(|| {
        Error::diagram_parse_fallback(
            meta.diagram_type.clone(),
            "flowchart edge id missing".to_string(),
        )
    })?;
    let edge_type = e.link.compatibility_edge_type().to_string();
    let stroke = e.link.compatibility_stroke().to_string();

    Ok((
        FlowEdge {
            id,
            from: e.from,
            to: e.to,
            label,
            label_type: Some(title_kind_str(&e.label_type).to_string()),
            edge_type: Some(edge_type),
            arrow: e.link.end,
            start_marker: e.link.start_marker,
            end_marker: e.link.end_marker,
            is_user_defined_id: e.is_user_defined_id,
            stroke: Some(stroke),
            stroke_kind: e.link.stroke_kind,
            visibility: e.link.visibility,
            length: e.link.length,
            style: e.style,
            classes: e.classes,
            interpolate: e.interpolate,
            animate: e.animate,
            animation: e.animation,
        },
        render_label_source,
    ))
}

#[cfg(any(feature = "diagram-flowchart", feature = "diagram-swimlane"))]
fn layout_shape_for_node(n: &Node) -> String {
    // Mirrors Mermaid FlowDB `getTypeFromVertex` logic at the pinned Mermaid baseline.
    if n.img.is_some() {
        return "imageSquare".to_string();
    }
    if n.icon.is_some() {
        match n.form.as_deref() {
            Some("circle") => return "iconCircle".to_string(),
            Some("square") => return "iconSquare".to_string(),
            Some("rounded") => return "iconRounded".to_string(),
            _ => return "icon".to_string(),
        }
    }
    match n.shape.as_deref() {
        Some("square") | None => "squareRect".to_string(),
        Some("round") => "roundedRect".to_string(),
        Some("ellipse") => "ellipse".to_string(),
        Some(other) => other.to_string(),
    }
}

#[cfg(any(feature = "diagram-flowchart", feature = "diagram-swimlane"))]
fn sanitized_node_label(n: &Node, config: &MermaidConfig) -> String {
    let label_raw = n.label.as_ref().unwrap_or(&n.id);
    let mut label = sanitized_label(label_raw, config);
    if label.len() >= 2 && label.starts_with('\"') && label.ends_with('\"') {
        label = label[1..label.len() - 1].to_string();
    }
    label
}

#[cfg(any(feature = "diagram-flowchart", feature = "diagram-swimlane"))]
fn sanitized_node_render_label_source(n: &Node, config: &MermaidConfig) -> String {
    let label_raw = n.label.as_ref().unwrap_or(&n.id);
    let mut label = sanitized_render_label_source(label_raw, config);
    if label.len() >= 2 && label.starts_with('"') && label.ends_with('"') {
        label = label[1..label.len() - 1].to_string();
    }
    label
}

#[cfg(any(feature = "diagram-flowchart", feature = "diagram-swimlane"))]
fn sanitized_optional_label(label: Option<&str>, config: &MermaidConfig) -> Option<String> {
    label.map(|s| sanitized_label(s, config))
}

#[cfg(any(feature = "diagram-flowchart", feature = "diagram-swimlane"))]
fn sanitized_label(raw: &str, config: &MermaidConfig) -> String {
    let decoded = decode_mermaid_hash_entities(raw);
    sanitize_text(&decoded, config)
}

#[cfg(any(feature = "diagram-flowchart", feature = "diagram-swimlane"))]
fn sanitized_render_label_source(raw: &str, config: &MermaidConfig) -> String {
    let flow_db_label = sanitize_text(raw, config);
    let decoded = crate::entities::restore_mermaid_entity_spelling(&flow_db_label);
    crate::sanitize::sanitize_text_as_html_fragment(decoded.as_ref(), config)
}

#[cfg(any(feature = "diagram-flowchart", feature = "diagram-swimlane"))]
fn render_label_source_needs_provenance(raw: &str) -> bool {
    raw.contains(['&', '#', 'ﬂ', '¶', '<', '>'])
}

#[cfg(any(feature = "diagram-flowchart", feature = "diagram-swimlane"))]
fn decode_mermaid_hash_entities(input: &str) -> std::borrow::Cow<'_, str> {
    // Mermaid runs `encodeEntities(...)` before parsing and later decodes with browser
    // `entityDecode(...)`. In our headless pipeline we decode into Unicode during parsing so
    // layout + SVG output match upstream.
    crate::entities::decode_mermaid_entities_to_unicode(input)
}

#[cfg(any(feature = "diagram-flowchart", feature = "diagram-swimlane"))]
fn flow_subgraph_to_model(
    sg: FlowSubGraph,
    config: &MermaidConfig,
) -> (FlowSubgraph, Option<String>) {
    let sanitized_title = sanitize_text(&sg.title, config);
    let title = decode_mermaid_hash_entities(&sanitized_title).into_owned();
    let render_title_source = render_label_source_needs_provenance(&sg.title)
        .then(|| sanitized_render_label_source(&sg.title, config))
        .filter(|source| *source != title);
    let subgraph = FlowSubgraph {
        id: sg.id,
        nodes: sg.nodes,
        title,
        classes: sg.classes,
        styles: sg.styles,
        dir: sg.dir,
        has_explicit_dir: sg.has_explicit_dir,
        label_type: Some(sg.label_type),
        metadata: sg.metadata,
    };
    (subgraph, render_title_source)
}

#[cfg(all(test, any(feature = "diagram-flowchart", feature = "diagram-swimlane")))]
mod tests {
    use super::*;

    fn flowchart_test_meta(diagram_type: &str) -> ParseMetadata {
        ParseMetadata {
            diagram_type: diagram_type.to_string(),
            config: MermaidConfig::empty_object(),
            effective_config: MermaidConfig::empty_object(),
            title: None,
        }
    }

    #[test]
    fn flowchart_subgraph_endpoint_provenance_preserves_authored_nodes() {
        let meta = flowchart_test_meta("flowchart-v2");
        let (endpoint_only, endpoint_context) = parse_flowchart_model_with_render_context(
            "flowchart TD\nsubgraph G\n  A\nend\nG --> A\n",
            &meta,
        )
        .expect("subgraph endpoint model");
        let endpoint = endpoint_only
            .nodes
            .iter()
            .find(|node| node.id == "G")
            .expect("subgraph endpoint node");
        assert_eq!(endpoint.provenance, FlowNodeProvenance::SubgraphAnchor);
        assert!(endpoint_context.contains_subgraph_vertex("G"));

        for source in [
            "flowchart TD\nG\nsubgraph G\n  A\nend\nG --> A\n",
            "flowchart TD\nsubgraph G\n  A\nend\nG --> A\nG\n",
            "flowchart TD\nsubgraph G\n  A\nend\nG:::authored --> A\n",
            concat!(
                "flowchart TD\n",
                "G@{ shape: rect, label: \"Authored G\" }\n",
                "subgraph G\n  A\nend\n",
                "G --> A\n",
            ),
        ] {
            let (model, context) = parse_flowchart_model_with_render_context(source, &meta)
                .expect("authored node and subgraph model");
            let node = model
                .nodes
                .iter()
                .find(|node| node.id == "G")
                .expect("authored G node");
            assert_eq!(node.provenance, FlowNodeProvenance::Authored, "{source}");
            assert!(context.contains_subgraph_vertex("G"), "{source}");
        }
    }

    #[test]
    fn same_id_shape_data_routes_to_subgraph_metadata_without_mutating_node() {
        let meta = flowchart_test_meta("flowchart-v2");
        let source = parse_flowchart_semantic_source(
            concat!(
                "flowchart TD\n",
                "subgraph G\n",
                "  A\n",
                "end\n",
                "G --> A\n",
                "G@{ shape: stadium, label: \"Authored G\" }\n",
            ),
            &meta,
        )
        .expect("same-id shapeData should parse");

        let subgraph = source
            .subgraphs
            .iter()
            .find(|subgraph| subgraph.id == "G")
            .expect("subgraph metadata target");
        assert_eq!(
            subgraph.metadata.as_ref().unwrap()["shape"],
            json!("stadium")
        );
        assert_eq!(
            subgraph.metadata.as_ref().unwrap()["label"],
            json!("Authored G")
        );

        let node = source
            .nodes
            .iter()
            .find(|node| node.id == "G")
            .expect("authored G node");
        assert_eq!(node.provenance, FlowNodeProvenance::Authored);
        assert_eq!(node.shape, None);
        assert_eq!(node.label, None);
    }

    #[test]
    fn flowchart_subgraph_metadata_survives_render_model_projection() {
        // Mermaid 12 FlowDB forwards the shallow-merged metadata unchanged to group nodes.
        let meta = flowchart_test_meta("flowchart-v2");
        let (model, context) = parse_flowchart_model_with_render_context(
            concat!(
                "flowchart TD\n",
                "subgraph G[Group Title]\n A\nend\n",
                "subgraph H\n B\nend\n",
                "G@{\nalgorithm: elk.box\ncustom:\n  first: 1\nview: collapsed\n}\n",
                "G@{\nalgorithm: elk.rectpacking\ncustom:\n  second: 2\nview: expanded\nlabel: Ignored\n}\n",
            ),
            &meta,
        )
        .expect("subgraph metadata should parse");
        let group = model.subgraphs.iter().find(|sg| sg.id == "G").unwrap();
        assert_eq!(group.title, "Group Title");
        assert_eq!(
            group.metadata,
            Some(json!({
                "algorithm": "elk.rectpacking",
                "custom": { "second": 2 },
                "view": "expanded",
                "label": "Ignored",
            }))
        );
        assert!(!context.is_subgraph_collapsed("G"));
        assert!(
            model
                .subgraphs
                .iter()
                .find(|sg| sg.id == "H")
                .unwrap()
                .metadata
                .is_none()
        );

        let serialized = serde_json::to_value(group).unwrap();
        let restored: FlowSubgraph = serde_json::from_value(serialized.clone()).unwrap();
        assert_eq!(restored.metadata, group.metadata);
        assert_eq!(
            serialized["metadata"],
            group.metadata.as_ref().unwrap().clone()
        );
        let mut without_metadata = serialized;
        without_metadata.as_object_mut().unwrap().remove("metadata");
        let restored: FlowSubgraph = serde_json::from_value(without_metadata).unwrap();
        assert!(restored.metadata.is_none());
        assert!(
            serde_json::to_value(restored)
                .unwrap()
                .get("metadata")
                .is_none()
        );
    }

    #[test]
    fn flowchart_subgraph_metadata_updates_the_canonical_group() {
        let meta = flowchart_test_meta("flowchart-v2");
        let model = parse_flowchart_model_for_render(
            concat!(
                "flowchart TD\n",
                "subgraph G[First]\n A\nend\n",
                "G@{ algorithm: elk.box }\n",
                "subgraph G[Second]\n B\nend\n",
                "G@{ algorithm: elk.radial }\n",
            ),
            &meta,
        )
        .expect("duplicate subgraph declarations should parse");
        assert_eq!(model.subgraphs.len(), 1);
        let group = &model.subgraphs[0];
        assert_eq!(group.title, "First");
        assert_eq!(group.nodes, ["A", "B"]);
        assert_eq!(group.metadata.as_ref().unwrap()["algorithm"], "elk.radial");
    }

    #[test]
    fn duplicate_subgraph_metadata_and_classes_follow_the_canonical_owner() {
        let meta = flowchart_test_meta("flowchart-v2");
        for metadata_between in [false, true] {
            let update = "class G hot\nG@{ view: collapsed, algorithm: elk.box }\n";
            let source = format!(
                "flowchart TB\nsubgraph G[First]\ndirection LR\nA\nend\n{}subgraph G[Second]\ndirection RL\nB\nA\nend\n{}",
                if metadata_between { update } else { "" },
                if metadata_between { "" } else { update },
            );
            let (model, context) = parse_flowchart_model_with_render_context(&source, &meta)
                .expect("repeated groups share their metadata target");
            assert_eq!(model.subgraphs.len(), 1, "{source}");
            let group = &model.subgraphs[0];
            assert_eq!(group.title, "First");
            assert_eq!(group.dir.as_deref(), Some("LR"));
            assert_eq!(group.nodes, ["A", "B"]);
            assert_eq!(group.classes, ["hot"]);
            assert_eq!(group.metadata.as_ref().unwrap()["algorithm"], "elk.box");
            assert!(context.is_subgraph_collapsed("G"));
            for member in ["A", "B"] {
                assert_eq!(context.collapsed_replacement(member), Some("G"));
            }
            let json = render_model_to_compat_json(&model, &meta).unwrap();
            assert_eq!(json["subgraphs"].as_array().unwrap().len(), 1);
            assert_eq!(json["subgraphs"][0]["nodes"], json!(["A", "B"]));
        }
    }

    #[test]
    fn duplicate_subgraph_membership_excludes_self_and_preserves_nested_groups() {
        let meta = flowchart_test_meta("flowchart-v2");
        let cases = [
            (
                "flowchart TB\nsubgraph S[Outer]\nx\nsubgraph S[Inner]\ny\nend\nend\n",
                vec![("S", "Inner", vec!["y", "x"])],
            ),
            (
                "flowchart TB\nsubgraph S[First]\nend\nsubgraph S[Second]\nsubgraph T\ny\nend\nend\n",
                vec![("S", "First", vec!["T"]), ("T", "T", vec!["y"])],
            ),
            (
                "flowchart TB\nsubgraph S\nx\nend\nsubgraph S\ny\nend\nsubgraph Anonymous group\na\nend\nsubgraph Anonymous group\nb\nend\n",
                vec![
                    ("S", "S", vec!["x", "y"]),
                    ("subGraph2", "Anonymous group", vec!["a"]),
                    ("subGraph3", "Anonymous group", vec!["b"]),
                ],
            ),
        ];
        for (source, expected) in cases {
            let model = parse_flowchart_model_for_render(source, &meta).unwrap();
            assert_eq!(model.subgraphs.len(), expected.len(), "{source}");
            for (group, (id, title, nodes)) in model.subgraphs.iter().zip(expected) {
                assert_eq!(group.id, id, "{source}");
                assert_eq!(group.title, title, "{source}");
                assert_eq!(group.nodes, nodes, "{source}");
                assert!(!group.nodes.contains(&group.id), "{source}");
            }
        }
    }

    #[test]
    fn duplicate_subgraph_editor_facts_keep_each_declaration_span() {
        let source = "flowchart TB\nsubgraph S[First]\nx\nend\nsubgraph S[Second]\ny\nend\nclass S hot\nS --> z\n";
        let control = OperationControl::new();
        let (model, facts, _) = parse_flowchart_json_and_editor_facts(
            source,
            &flowchart_test_meta("flowchart-v2"),
            &control,
        )
        .unwrap()
        .into_parts();
        let model = model.unwrap();
        let symbols: Vec<_> = facts
            .symbols
            .iter()
            .filter(|symbol| symbol.name == "S")
            .collect();
        let declarations: Vec<_> = symbols
            .iter()
            .filter(|symbol| symbol.kind == EditorSemanticKind::Namespace)
            .collect();
        assert_eq!(declarations.len(), 2);
        assert_ne!(declarations[0].selection, declarations[1].selection);
        assert!(
            symbols.len() >= 4,
            "declarations, class target and edge endpoint stay linked"
        );
        for symbol in symbols {
            assert_eq!(symbol.rename_policy, EditorRenamePolicy::FlowchartNodeId);
            assert_eq!(&source[symbol.selection.start..symbol.selection.end], "S");
        }
        assert_eq!(model["subgraphs"].as_array().unwrap().len(), 1);
        assert_eq!(model["subgraphs"][0]["nodes"], json!(["x", "y"]));
    }

    #[test]
    fn flowchart_subgraph_metadata_preserves_algorithm_values_for_layout_resolution() {
        let meta = flowchart_test_meta("flowchart-v2");
        for (authored, expected) in [
            ("null", json!(null)),
            ("42", json!(42)),
            ("unknown-layout", json!("unknown-layout")),
            ("[elk.box]", json!(["elk.box"])),
        ] {
            let input =
                format!("flowchart TD\nsubgraph G\n A\nend\nG@{{ algorithm: {authored} }}\n");
            let model = parse_flowchart_model_for_render(&input, &meta)
                .expect("opaque metadata should parse");
            assert_eq!(
                model.subgraphs[0].metadata.as_ref().unwrap()["algorithm"],
                expected
            );
        }
    }

    #[test]
    fn subgraph_shape_data_preserves_only_the_bare_vertex_call() {
        let meta = flowchart_test_meta("flowchart-v2");
        let (model, context) = parse_flowchart_model_with_render_context(
            "flowchart TD\nsubgraph G\n  A\nend\nG@{ view: collapsed }\nB\n",
            &meta,
        )
        .expect("subgraph shapeData should parse");

        assert_eq!(model.vertex_calls, ["A", "G", "B"]);
        assert!(context.is_subgraph_collapsed("G"));
    }

    #[test]
    fn standalone_shape_data_preserves_upstream_vertex_call_sequence() {
        let meta = flowchart_test_meta("flowchart-v2");
        let (model, _) = parse_flowchart_model_with_render_context(
            "flowchart TD\nA@{ shape: rect }\nB@{ shape: stadium }\n",
            &meta,
        )
        .expect("standalone shapeData should parse");

        assert_eq!(model.vertex_calls, ["A", "A", "B", "B"]);
    }

    #[test]
    fn collapsed_subgraph_shape_data_is_retained_in_render_context() {
        let meta = flowchart_test_meta("flowchart-v2");
        let (model, context) = parse_flowchart_model_with_render_context(
            "flowchart TD\nsubgraph one[My Group]\n  A --> B\nend\nC --> A\none@{ view: collapsed }\n",
            &meta,
        )
        .expect("collapsed subgraph should parse");
        assert!(context.is_subgraph_collapsed("one"));
        assert_eq!(context.collapsed_replacement("A"), Some("one"));
        assert_eq!(context.collapsed_replacement("B"), Some("one"));
        assert_eq!(context.collapsed_replacement("one"), None);
        assert!(model.subgraphs.iter().any(|sg| sg.id == "one"));
    }

    #[test]
    fn collapsed_subgraph_shape_data_in_chains_uses_subgraph_dispatch() {
        let meta = flowchart_test_meta("flowchart-v2");
        for source in [
            "flowchart TD\nsubgraph G\n  A\nend\nG@{ view: collapsed } --> B\n",
            "flowchart TD\nsubgraph G\n  A\nend\nB --> G@{ view: collapsed }\n",
        ] {
            let (_, context) = parse_flowchart_model_with_render_context(source, &meta)
                .expect("chained subgraph metadata should parse");
            assert!(context.is_subgraph_collapsed("G"), "{source}");
        }
    }

    #[test]
    fn subgraph_shape_data_precedes_same_id_edge_metadata() {
        let meta = flowchart_test_meta("flowchart-v2");
        let (model, _) = parse_flowchart_model_with_render_context(
            concat!(
                "flowchart TD\n",
                "subgraph e1\n  A\nend\n",
                "A e1@--> B\n",
                "e1@{ curve: basis }\n",
            ),
            &meta,
        )
        .expect("same-id subgraph and edge metadata should parse");

        let edge = model
            .edges
            .iter()
            .find(|edge| edge.id == "e1")
            .expect("user-defined edge");
        assert_eq!(edge.interpolate, None);
    }

    #[test]
    fn same_id_subgraph_vertex_style_preserves_statement_order() {
        let meta = flowchart_test_meta("flowchart-v2");
        let cases = [
            (
                concat!(
                    "flowchart TD\n",
                    "classDef base stroke:#00f\n",
                    "subgraph G\n  A\nend\n",
                    "class G base\n",
                    "style G fill:#f00\n",
                ),
                Vec::<String>::new(),
            ),
            (
                concat!(
                    "flowchart TD\n",
                    "classDef base stroke:#00f\n",
                    "subgraph G\n  A\nend\n",
                    "style G fill:#f00\n",
                    "class G base\n",
                ),
                vec!["base".to_string()],
            ),
        ];

        for (source, expected_vertex_classes) in cases {
            let (model, context) = parse_flowchart_model_with_render_context(source, &meta)
                .expect("same-ID group model");
            let (subgraph_index, subgraph) = model
                .subgraphs
                .iter()
                .enumerate()
                .find(|(_, subgraph)| subgraph.id == "G")
                .expect("subgraph G");
            let (vertex_classes, vertex_styles) =
                context.effective_subgraph_css(subgraph_index, subgraph);

            assert_eq!(subgraph.classes, ["base"]);
            assert_eq!(vertex_classes, expected_vertex_classes, "{source}");
            assert_eq!(vertex_styles, ["fill:#f00"], "{source}");
        }
    }

    #[test]
    fn same_id_subgraph_vertex_style_stays_out_of_the_public_model() {
        let meta = flowchart_test_meta("flowchart-v2");
        let (model, context) = parse_flowchart_model_with_render_context(
            concat!(
                "flowchart TD\n",
                "subgraph G\n  A\nend\n",
                "style G fill:#f00\n",
                "class G hot\n",
            ),
            &meta,
        )
        .expect("same-ID group model");

        let json = serde_json::to_value(&model).expect("serialize Flowchart model");
        assert!(json["subgraphs"][0].get("sameIdVertexStyle").is_none());
        let subgraph = &model.subgraphs[0];
        assert!(subgraph.styles.is_empty());
        let (classes, styles) = context.effective_subgraph_css(0, subgraph);
        assert_eq!(classes, ["hot"]);
        assert_eq!(styles, ["fill:#f00"]);

        let compatibility =
            render_model_to_compat_json(&model, &meta).expect("legacy flowchart JSON");
        assert!(
            compatibility["subgraphs"][0]
                .get("sameIdVertexStyle")
                .is_none()
        );
    }

    #[test]
    fn duplicate_subgraph_vertex_css_is_owned_by_the_canonical_group() {
        let meta = flowchart_test_meta("flowchart-v2");
        let cases = [
            concat!(
                "flowchart TD\n",
                "style X fill:#f00\n",
                "subgraph X\n  A\nend\n",
                "subgraph X\n  B\nend\n",
            ),
            concat!(
                "flowchart TD\n",
                "subgraph X\n  A\nend\n",
                "style X fill:#f00\n",
                "subgraph X\n  B\nend\n",
            ),
            concat!(
                "flowchart TD\n",
                "subgraph X\n  A\nend\n",
                "subgraph X\n  B\nend\n",
                "style X fill:#f00\n",
            ),
        ];

        for source in cases {
            let (model, context) = parse_flowchart_model_with_render_context(source, &meta)
                .expect("duplicate subgraph declarations should parse");

            assert_eq!(model.subgraphs.len(), 1, "{source}");
            assert!(
                model
                    .subgraphs
                    .iter()
                    .all(|subgraph| subgraph.styles.is_empty()),
                "style statements belong to FlowDB's vertex record, not FlowSubgraph: {source}"
            );
            let (_, styles) = context.effective_subgraph_css(0, &model.subgraphs[0]);
            assert_eq!(styles, ["fill:#f00"], "{source}");
        }
    }

    #[test]
    fn flowchart_elk_missing_subgraph_node_is_an_explicit_anchor_fact() {
        let meta = flowchart_test_meta("flowchart-elk");
        let model =
            parse_flowchart_model_for_render("flowchart-elk TD\nsubgraph Empty\nend\n", &meta)
                .expect("empty ELK subgraph model");
        let node = model
            .nodes
            .iter()
            .find(|node| node.id == "Empty")
            .expect("ELK subgraph anchor");

        assert_eq!(node.provenance, FlowNodeProvenance::SubgraphAnchor);
    }

    #[test]
    fn legacy_flowchart_json_omits_typed_node_provenance() {
        let meta = flowchart_test_meta("flowchart-v2");
        let model = parse_flowchart_model_for_render(
            "flowchart TD\nsubgraph G\n  A\nend\nG --> A\n",
            &meta,
        )
        .expect("subgraph endpoint model");
        assert!(
            model
                .nodes
                .iter()
                .any(|node| node.provenance == FlowNodeProvenance::SubgraphAnchor)
        );

        let compatibility = render_model_to_compat_json(&model, &meta)
            .expect("legacy flowchart compatibility JSON");
        let nodes = compatibility
            .get("nodes")
            .and_then(Value::as_array)
            .expect("legacy nodes array");
        assert!(nodes.iter().all(|node| node.get("provenance").is_none()));
    }

    #[test]
    fn flowchart_model_trims_parser_labels_before_entity_decode() {
        let nbsp = '\u{00a0}';
        let source = format!(
            r#"flowchart LR
Direct["{nbsp}Direct{nbsp}"]
Entity["&nbsp;Entity&nbsp;"]
Mixed["{nbsp}&nbsp;Mixed&nbsp;{nbsp}"]
Internal["A{nbsp}B"]
DirectOnly["{nbsp}"]
EntityOnly["&nbsp;"]
Shape@{{ label: "{nbsp}&nbsp;Shape&nbsp;{nbsp}", labelType: "string", shape: "rect" }}
A -->|{nbsp}Text{nbsp}| B
B -- "{nbsp}String{nbsp}" --> C
C -- "`{nbsp}Markdown{nbsp}`" --> D
D -- "{nbsp}&nbsp;MixedEdge&nbsp;{nbsp}" --> E
E -- "{nbsp}" --> F
F -- "&nbsp;" --> G
"#
        );
        let meta = ParseMetadata {
            diagram_type: "flowchart-v2".to_string(),
            config: MermaidConfig::empty_object(),
            effective_config: MermaidConfig::empty_object(),
            title: None,
        };
        let (model, render_label_sources) =
            parse_flowchart_model_with_render_context(&source, &meta).expect("flowchart model");
        let node_label = |id: &str| {
            model
                .nodes
                .iter()
                .find(|node| node.id == id)
                .and_then(|node| node.label.as_deref())
                .unwrap_or_else(|| panic!("missing node label for {id}"))
        };
        let node_render_label = |id: &str| {
            let node = model
                .nodes
                .iter()
                .find(|node| node.id == id)
                .unwrap_or_else(|| panic!("missing node for {id}"));
            render_label_sources
                .node_label_for_render(node)
                .unwrap_or_else(|| panic!("missing render label for {id}"))
        };
        let edge_label = |from: &str| {
            model
                .edges
                .iter()
                .find(|edge| edge.from == from)
                .unwrap_or_else(|| panic!("missing edge from {from}"))
                .label
                .as_deref()
        };
        let edge_render_label = |from: &str| {
            let edge = model
                .edges
                .iter()
                .find(|edge| edge.from == from)
                .unwrap_or_else(|| panic!("missing edge from {from}"));
            render_label_sources.edge_label_for_render(edge)
        };

        assert_eq!(node_label("Direct"), "Direct");
        assert_eq!(node_label("Entity"), format!("{nbsp}Entity{nbsp}"));
        assert_eq!(node_label("Mixed"), format!("{nbsp}Mixed{nbsp}"));
        assert_eq!(node_label("Internal"), format!("A{nbsp}B"));
        assert_eq!(node_label("DirectOnly"), "");
        assert_eq!(node_label("EntityOnly"), nbsp.to_string());
        assert_eq!(
            node_label("Shape"),
            format!("{nbsp}{nbsp}Shape{nbsp}{nbsp}")
        );

        assert_eq!(edge_label("A"), Some("Text"));
        assert_eq!(edge_label("B"), Some("String"));
        assert_eq!(edge_label("C"), Some("Markdown"));
        let mixed_edge_label = format!("{nbsp}MixedEdge{nbsp}");
        assert_eq!(edge_label("D"), Some(mixed_edge_label.as_str()));
        assert_eq!(edge_label("E"), Some(""));
        let nbsp_label = nbsp.to_string();
        assert_eq!(edge_label("F"), Some(nbsp_label.as_str()));

        assert_eq!(node_render_label("Direct"), "Direct");
        assert_eq!(node_render_label("Entity"), "&nbsp;Entity&nbsp;");
        assert_eq!(node_render_label("Mixed"), "&nbsp;Mixed&nbsp;");
        assert_eq!(node_render_label("DirectOnly"), "");
        assert_eq!(node_render_label("EntityOnly"), "&nbsp;");
        assert_eq!(
            node_render_label("Shape"),
            format!("{nbsp}&nbsp;Shape&nbsp;{nbsp}")
        );
        assert_eq!(edge_render_label("A"), Some("Text"));
        assert_eq!(edge_render_label("D"), Some("&nbsp;MixedEdge&nbsp;"));
        assert_eq!(edge_render_label("E"), Some(""));
        assert_eq!(edge_render_label("F"), Some("&nbsp;"));

        let subgraph_source =
            format!("flowchart LR\nsubgraph SG[\"{nbsp}&nbsp;Group&nbsp;{nbsp}\"]\n  H\nend\n");
        let (subgraph_model, subgraph_render_label_sources) =
            parse_flowchart_model_with_render_context(&subgraph_source, &meta)
                .expect("subgraph model");
        let subgraph = subgraph_model
            .subgraphs
            .iter()
            .find(|subgraph| subgraph.id == "SG")
            .expect("subgraph SG");
        assert_eq!(subgraph.title, format!("{nbsp}Group{nbsp}"));
        assert_eq!(
            subgraph_render_label_sources.subgraph_title_for_render(0, subgraph),
            "&nbsp;Group&nbsp;"
        );

        let entity_subgraph_source =
            "flowchart LR\nsubgraph Entity[\"A &amp; &lt; >\"]\n  Inside\nend\n";
        let (entity_subgraph_model, entity_subgraph_render_label_sources) =
            parse_flowchart_model_with_render_context(entity_subgraph_source, &meta)
                .expect("entity subgraph model");
        let entity_subgraph = entity_subgraph_model
            .subgraphs
            .iter()
            .find(|subgraph| subgraph.id == "Entity")
            .expect("entity subgraph");
        assert_eq!(entity_subgraph.title, "A & < >");
        assert_eq!(
            entity_subgraph_render_label_sources.subgraph_title_for_render(0, entity_subgraph),
            "A &amp; &lt; &gt;"
        );

        let duplicate_subgraph_source =
            "flowchart LR\nsubgraph X[\"&nbsp;First\"]\n  A\nend\nsubgraph X[Second]\n  B\nend\n";
        let (duplicate_model, duplicate_sources) =
            parse_flowchart_model_with_render_context(duplicate_subgraph_source, &meta)
                .expect("duplicate subgraph model");
        assert_eq!(duplicate_model.subgraphs.len(), 1);
        assert_eq!(
            duplicate_sources.subgraph_title_for_render(0, &duplicate_model.subgraphs[0]),
            "&nbsp;First"
        );
        assert_eq!(duplicate_model.subgraphs[0].nodes, ["A", "B"]);

        let later_entity_source =
            "flowchart LR\nsubgraph X[First]\n  A\nend\nsubgraph X[\"&nbsp;Second\"]\n  B\nend\n";
        let (later_entity_model, later_entity_sources) =
            parse_flowchart_model_with_render_context(later_entity_source, &meta)
                .expect("duplicate subgraph model with later entity label");
        assert_eq!(
            later_entity_sources.subgraph_title_for_render(0, &later_entity_model.subgraphs[0]),
            "First"
        );
        assert_eq!(later_entity_model.subgraphs.len(), 1);
        assert_eq!(later_entity_model.subgraphs[0].nodes, ["A", "B"]);

        let punctuation_source = "flowchart LR\nsubgraph \"A;B\"\n  Bare\nend\nsubgraph \"`M;D`\"\n  Markdown\nend\nsubgraph SG[\"A]B\"]\n  Bracket\nend\n";
        let punctuation_model = parse_flowchart_model_for_render(punctuation_source, &meta)
            .expect("punctuation subgraph model");
        assert!(
            punctuation_model
                .subgraphs
                .iter()
                .any(|subgraph| subgraph.id == "A;B" && subgraph.title == "A;B")
        );
        assert!(punctuation_model.subgraphs.iter().any(|subgraph| {
            subgraph.id == "M;D"
                && subgraph.title == "M;D"
                && subgraph.label_type.as_deref() == Some("markdown")
        }));
        assert!(
            punctuation_model
                .subgraphs
                .iter()
                .any(|subgraph| subgraph.id == "SG" && subgraph.title == "A]B")
        );
    }

    #[cfg(feature = "diagram-flowchart")]
    #[test]
    fn flowchart_render_label_context_stays_out_of_the_public_model_contract() {
        let parsed = crate::Engine::new()
            .parse_diagram_for_render_model_sync(
                "flowchart LR\nA[\"&nbsp;A&nbsp;\"] -->|\"&nbsp;E&nbsp;\"| B\n",
                crate::ParseOptions::strict(),
            )
            .expect("parse flowchart")
            .expect("detect flowchart");
        assert!(parsed.retained_render_context_bytes() > 0);

        let (_, crate::RenderSemanticModel::Flowchart(model)) = parsed.into_parts() else {
            panic!("expected Flowchart model");
        };
        let json = serde_json::to_value(&model).expect("serialize Flowchart model");
        assert!(json.get("renderLabelSources").is_none());
        assert!(json.get("render_label_sources").is_none());

        let roundtrip: FlowchartModel =
            serde_json::from_value(json).expect("deserialize Flowchart model");
        assert_eq!(roundtrip.nodes.len(), model.nodes.len());
        assert_eq!(roundtrip.edges.len(), model.edges.len());
        assert_eq!(roundtrip.nodes[0].label, model.nodes[0].label);
        assert_eq!(roundtrip.edges[0].label, model.edges[0].label);
    }

    #[cfg(feature = "diagram-flowchart")]
    #[test]
    fn flowchart_render_label_context_applies_shape_sanitization_to_angle_text() {
        let parsed = crate::Engine::new()
            .parse_diagram_for_render_model_sync(
                include_str!(concat!(
                    env!("CARGO_MANIFEST_DIR"),
                    "/../fixtures/flowchart/stress_flowchart_svglike_escaped_tags_025.mmd"
                )),
                crate::ParseOptions::strict(),
            )
            .expect("parse flowchart")
            .expect("detect flowchart");
        let render_context = parsed
            .flowchart_render_context()
            .expect("flowchart render context");
        let crate::RenderSemanticModel::Flowchart(model) = parsed.model() else {
            panic!("expected Flowchart model");
        };

        let comparison = model
            .nodes
            .iter()
            .find(|node| node.id == "C")
            .expect("comparison node");
        assert_eq!(comparison.label.as_deref(), Some("x &lt; y and y > z"));
        assert_eq!(
            render_context.node_label_for_render(comparison),
            Some("x &lt; y and y &gt; z")
        );

        let formatted = model
            .nodes
            .iter()
            .find(|node| node.id == "D")
            .expect("formatted node");
        assert_eq!(
            render_context.node_label_for_render(formatted),
            Some("<u>under</u> and <i>italic</i>")
        );
    }

    #[test]
    fn flowchart_parser_families_expose_typed_editor_semantics() {
        let semantics = crate::family::diagram_type_editor_semantics("flowchart-v2")
            .expect("flowchart is an admitted family");

        assert_eq!(semantics.outline_kind(), EditorSemanticKind::Module);
        let swimlane = crate::family::diagram_type_editor_semantics("swimlane")
            .expect("swimlane is an admitted family");
        assert_eq!(swimlane.outline_kind(), EditorSemanticKind::Variable);
    }

    #[test]
    fn combined_flowchart_parse_propagates_preexisting_cancellation() {
        let meta = ParseMetadata {
            diagram_type: "flowchart-v2".to_string(),
            config: MermaidConfig::empty_object(),
            effective_config: MermaidConfig::empty_object(),
            title: None,
        };
        let control = OperationControl::new();
        control.cancel();

        assert!(matches!(
            parse_flowchart_json_and_editor_facts("flowchart TD\nA-->B\n", &meta, &control),
            Err(crate::OperationCancelled { .. })
        ));
    }

    #[test]
    fn flowchart_parse_error_cannot_mask_post_parser_cancellation() {
        let meta = ParseMetadata {
            diagram_type: "flowchart-v2".to_string(),
            config: MermaidConfig::empty_object(),
            effective_config: MermaidConfig::empty_object(),
            title: None,
        };
        let control = OperationControl::new().for_phase(crate::OperationPhase::Parse);
        control.cancel_after_checkpoints(1);

        assert!(matches!(
            parse_flowchart_ast_controlled("flowchart TD\nA-->\n", &meta, &control),
            Err(crate::OperationCancelled {
                phase: crate::OperationPhase::Parse,
                reason: crate::CancelReason::Requested,
            })
        ));
        assert!(control.is_cancelled());
    }

    #[test]
    fn token_trace_stops_after_cancellation_during_parser_work() {
        let mut source = String::from("flowchart TD\n");
        for index in 0..512 {
            source.push_str(&format!("n{index}-->n{}\n", index + 1));
        }
        let control = OperationControl::new();
        control.cancel_after_checkpoints(2);

        assert!(matches!(
            construct_flowchart_token_trace(&source, &control),
            Err(crate::OperationCancelled { .. })
        ));
        assert!(control.is_cancelled());
    }

    #[test]
    fn single_large_chain_observes_cancellation_in_post_parse_projections() {
        let nodes = (0..256)
            .map(|index| format!("n{index}"))
            .collect::<Vec<_>>()
            .join(" & ");
        let source = format!("flowchart TD\n{nodes}\n");
        let meta = ParseMetadata {
            diagram_type: "flowchart-v2".to_string(),
            config: MermaidConfig::empty_object(),
            effective_config: MermaidConfig::empty_object(),
            title: None,
        };
        let ast = parse_flowchart_ast(&source, &meta).expect("large node group should parse");
        assert!(matches!(
            ast.walk().next(),
            Some(StatementEvent::Enter(Stmt::Chain { node_groups, .. }))
                if node_groups.iter().map(Vec::len).sum::<usize>() == 256
        ));

        let shape_data_control = OperationControl::new();
        shape_data_control.cancel_after_checkpoints(2);
        assert!(matches!(
            prepare_flowchart_shape_data(&ast, &shape_data_control),
            Err(crate::OperationCancelled { .. })
        ));

        let editor_facts_control = OperationControl::new();
        editor_facts_control.cancel_after_checkpoints(2);
        assert!(matches!(
            editor_facts_from_flowchart_ast(&ast, &editor_facts_control),
            Err(crate::OperationCancelled { .. })
        ));

        let semantic_source =
            parse_flowchart_semantic_source(&source, &meta).expect("large node group should build");
        let render_control = OperationControl::new();
        render_control.cancel_after_checkpoints(2);
        assert!(matches!(
            semantic_source.into_render_model_controlled(&meta, &render_control),
            Err(crate::OperationCancelled { .. })
        ));
    }

    #[test]
    fn single_large_subgraph_chain_observes_cancellation_in_membership_projection() {
        let nodes = (0..256)
            .map(|index| format!("n{index}"))
            .collect::<Vec<_>>()
            .join(" & ");
        let source = format!("flowchart TD\nsubgraph group\n{nodes}\nend\n");
        let meta = ParseMetadata {
            diagram_type: "flowchart-v2".to_string(),
            config: MermaidConfig::empty_object(),
            effective_config: MermaidConfig::empty_object(),
            title: None,
        };
        let ast =
            parse_flowchart_ast(&source, &meta).expect("large subgraph node group should parse");
        let mut builder = SubgraphBuilder::new(false, ast.direction.clone());
        let control = OperationControl::new();
        control.cancel_after_checkpoints(2);

        assert!(matches!(
            builder.visit_statements(&ast, &control),
            Err(crate::OperationCancelled { .. })
        ));
    }

    #[test]
    fn flowchart_subgraphs_exist_matches_mermaid_flowdb_spec() {
        let subgraphs = vec![
            FlowSubGraph {
                id: "sg0".to_string(),
                nodes: vec![
                    "a".to_string(),
                    "b".to_string(),
                    "c".to_string(),
                    "e".to_string(),
                ],
                title: "".to_string(),
                classes: Vec::new(),
                styles: Vec::new(),
                dir: None,
                has_explicit_dir: false,
                label_type: "text".to_string(),
                metadata: None,
            },
            FlowSubGraph {
                id: "sg1".to_string(),
                nodes: vec!["f".to_string(), "g".to_string(), "h".to_string()],
                title: "".to_string(),
                classes: Vec::new(),
                styles: Vec::new(),
                dir: None,
                has_explicit_dir: false,
                label_type: "text".to_string(),
                metadata: None,
            },
            FlowSubGraph {
                id: "sg2".to_string(),
                nodes: vec!["i".to_string(), "j".to_string()],
                title: "".to_string(),
                classes: Vec::new(),
                styles: Vec::new(),
                dir: None,
                has_explicit_dir: false,
                label_type: "text".to_string(),
                metadata: None,
            },
            FlowSubGraph {
                id: "sg3".to_string(),
                nodes: vec!["k".to_string()],
                title: "".to_string(),
                classes: Vec::new(),
                styles: Vec::new(),
                dir: None,
                has_explicit_dir: false,
                label_type: "text".to_string(),
                metadata: None,
            },
        ];

        assert!(super::subgraph::subgraphs_exist(&subgraphs, "a"));
        assert!(super::subgraph::subgraphs_exist(&subgraphs, "h"));
        assert!(super::subgraph::subgraphs_exist(&subgraphs, "j"));
        assert!(super::subgraph::subgraphs_exist(&subgraphs, "k"));

        assert!(!super::subgraph::subgraphs_exist(&subgraphs, "a2"));
        assert!(!super::subgraph::subgraphs_exist(&subgraphs, "l"));
    }
}
