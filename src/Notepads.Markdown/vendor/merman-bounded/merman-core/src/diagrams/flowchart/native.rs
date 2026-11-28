// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE file in the project root for license information.
// ---------------------------------------------------------------------------------------------

//! NativeFlowchartV1 keeps the upstream typed grammar and declaration replay,
//! with a borrowed scalar policy and a joint, actually funded model owner.

use super::*;
use markdown_memory::{MemoryError, NativePayload};

mod flat;
pub(super) use flat::FlatShapeData;

#[derive(Clone, Copy, Debug, Default)]
pub struct NativeParsePolicy {
    pub inherit_dir: bool,
    pub html_labels: bool,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum NativeFeature {
    Configuration,
    HtmlLabels,
    Click,
    ShapeData,
    Accessibility,
    Family,
    Shape,
    Metadata,
    Curve,
    Assets,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum NativeParseError {
    Memory(MemoryError),
    Cancelled,
    Syntax { span: Option<SourceSpan> },
    Unsupported { feature: NativeFeature },
    Model,
}

type NativeResult<T> = std::result::Result<T, NativeParseError>;

fn stopped(control: &OperationControl) -> NativeParseError {
    control
        .memory_scope()
        .and_then(|scope| scope.failure())
        .map(NativeParseError::Memory)
        .unwrap_or(NativeParseError::Cancelled)
}
fn active<T>(value: OperationControlResult<T>, control: &OperationControl) -> NativeResult<T> {
    value.map_err(|_| stopped(control))
}
fn syntax(error: Error, control: &OperationControl) -> NativeParseError {
    if let Some(failure) = control.memory_scope().and_then(|scope| scope.failure()) {
        return NativeParseError::Memory(failure);
    }
    match error {
        Error::OperationCancelled(_) => NativeParseError::Cancelled,
        Error::DiagramParse { diagnostic, .. } => {
            if diagnostic.code() == Some("merman.native.unsupported") {
                let feature = match diagnostic.message() {
                    "shape" => NativeFeature::Shape,
                    "assets" => NativeFeature::Assets,
                    "click" => NativeFeature::Click,
                    "shape-data" => NativeFeature::ShapeData,
                    _ => NativeFeature::Metadata,
                };
                NativeParseError::Unsupported { feature }
            } else if diagnostic.code() == Some("merman.native.model") {
                NativeParseError::Model
            } else {
                NativeParseError::Syntax {
                    span: diagnostic.span(),
                }
            }
        }
        _ => NativeParseError::Model,
    }
}

pub(super) fn core_result<T>(
    value: NativeResult<T>,
    control: &OperationControl,
) -> OperationControlResult<Result<T>> {
    Ok(match value {
        Ok(value) => Ok(value),
        Err(NativeParseError::Memory(error)) => return Err(memory::failure(control, error)),
        Err(NativeParseError::Cancelled) => {
            return Err(memory::failure(control, MemoryError::Cancelled));
        }
        Err(NativeParseError::Unsupported { feature }) => Err(memory::unsupported_error(
            match feature {
                NativeFeature::Shape => "shape",
                NativeFeature::Assets => "assets",
                NativeFeature::ShapeData => "shape-data",
                NativeFeature::Click => "click",
                _ => "metadata",
            },
            control,
        )?),
        Err(NativeParseError::Syntax { span }) => Err(Error::diagram_parse_diagnostic(
            String::new(),
            if let Some(span) = span {
                crate::ParseDiagnostic::new(String::new())
                    .with_span(span, crate::ParseDiagnosticSpanKind::Fallback)
            } else {
                crate::ParseDiagnostic::new(String::new())
            },
        )),
        Err(NativeParseError::Model) => Err(Error::diagram_parse_diagnostic(
            String::new(),
            crate::ParseDiagnostic::new(String::new())
                .with_code(memory::owned_string("merman.native.model", control)?),
        )),
    })
}
fn limited<T>(control: &OperationControl) -> NativeResult<T> {
    if let Some(scope) = control.memory_scope() {
        scope.record_failure(MemoryError::BudgetLimited);
    }
    Err(NativeParseError::Memory(MemoryError::BudgetLimited))
}

pub(super) fn parse(
    code: &str,
    policy: &NativeParsePolicy,
    control: &OperationControl,
) -> NativeResult<NativePayload<(FlowchartModel, FlowchartRenderContext)>> {
    let Some(region) = control.memory_region() else {
        return Err(NativeParseError::Memory(MemoryError::Unfunded));
    };
    active(control.checkpoint(), control)?;
    if code.len() > 16 * 1024 {
        return limited(control);
    }
    if policy.html_labels {
        return Err(NativeParseError::Unsupported {
            feature: NativeFeature::HtmlLabels,
        });
    }
    let input = preflight(code, control)?;
    if unsupported_family(input.masked.as_deref().unwrap_or(code)) {
        return Err(NativeParseError::Unsupported {
            feature: NativeFeature::Family,
        });
    }
    let ast = active(
        parse_flowchart_ast_for_type_controlled(
            input.masked.as_deref().unwrap_or(code),
            "flowchart-v2",
            control,
        ),
        control,
    )?
    .map_err(|error| syntax(error, control))?;
    if !matches!(ast.keyword.as_str(), "flowchart" | "graph") {
        return Err(NativeParseError::Unsupported {
            feature: NativeFeature::Family,
        });
    }
    validate_ast(&ast, control)?;
    let shape_documents_native = prepare_flat(&ast, control)?;
    let mut builder = SubgraphBuilder::new_controlled(
        policy.inherit_dir,
        active(memory::clone_owned(&ast.direction, control), control)?,
        control,
    );
    active(builder.visit_statements(&ast, control), control)?;
    if builder.subgraphs.len() > 32 {
        return limited(control);
    }
    let mut group_ids = memory::new_set(control);
    for group in &builder.subgraphs {
        if group.id.len() > 128 {
            return limited(control);
        }
        let id = active(memory::clone_owned(&group.id, control), control)?;
        active(memory::insert_set(&mut group_ids, id, control), control)?;
    }
    let mut build = FlowchartBuildState::new_controlled(group_ids, control);
    active(build.add_statements(&ast, control), control)?;
    if build
        .nodes
        .iter()
        .filter(|node| node.provenance == FlowNodeProvenance::Authored)
        .count()
        > 128
        || build.edges.len() > 256
    {
        return limited(control);
    }
    let FlowchartBuildState {
        mut nodes,
        mut node_index,
        mut edges,
        ..
    } = build;
    let mut classes = IndexMap::new_in(region.clone());
    let mut styles = FlowchartRenderStyleSources::new_controlled(control);
    let mut collapsed = memory::new_set(control);
    let mut vertex_calls = Vec::new();
    let mut warnings = Vec::new();
    let mut tooltips = memory::new_map(control);
    let mut edge_defaults = EdgeDefaults {
        style: Vec::new(),
        interpolate: None,
    };
    // Basic classic syntax has no shape document. The dedicated flat reader
    // supplies typed records here, never a legacy Value/YAML conversion.
    let shape_documents = memory::new_map(control);
    let mut context = FlowchartSemanticContext {
        nodes: &mut nodes,
        node_index: &mut node_index,
        edges: &mut edges,
        subgraphs: &mut builder.subgraphs,
        subgraph_declaration_owners: &builder.declaration_owners,
        subgraph_vertex_styles: &mut styles,
        collapsed_subgraphs: &mut collapsed,
        vertex_calls: &mut vertex_calls,
        warning_facts: &mut warnings,
        class_defs: &mut classes,
        tooltips: &mut tooltips,
        edge_defaults: &mut edge_defaults,
        security_level_loose: false,
        diagram_type: "flowchart-v2",
        config: None,
        shape_data_documents: &shape_documents,
        native_shape_documents: Some(&shape_documents_native),
        control,
    };
    active(apply_semantic_statements(&ast, &mut context), control)?
        .map_err(|error| syntax(error, control))?;
    if ast.direction.is_none() {
        let warning = DiagramWarningFact::new(
            active(
                memory::owned_string(FLOWCHART_EXPLICIT_DIRECTION_WARNING_RULE_ID, control),
                control,
            )?,
            active(
                memory::owned_string("flowchart headers should declare an explicit direction such as `TB`, `TD`, `BT`, `LR`, or `RL`", control),
                control,
            )?,
        )
        .with_span(ast.header_span)
        .with_fix_span(SourceSpan::new(ast.header_span.end, ast.header_span.end));
        active(memory::push(&mut warnings, warning, control), control)?;
    }
    if nodes.len() > 160
        || nodes
            .iter()
            .filter(|node| node.provenance == FlowNodeProvenance::Authored)
            .count()
            > 128
    {
        return limited(control);
    }
    if edge_defaults
        .interpolate
        .as_deref()
        .is_some_and(|curve| curve != "linear")
        || edges.iter().any(|edge| {
            edge.interpolate
                .as_deref()
                .is_some_and(|curve| curve != "linear")
        })
    {
        return Err(NativeParseError::Unsupported {
            feature: NativeFeature::Curve,
        });
    }
    let mut labels = FlowchartRenderLabelSources::new_controlled(control);
    let mut render_nodes = Vec::new();
    active(
        memory::reserve(&mut render_nodes, nodes.len(), control),
        control,
    )?;
    for node in nodes {
        let raw = node.label.as_deref().unwrap_or(&node.id);
        let label = semantic_text(raw, control)?;
        let source = active(memory::owned_string(raw, control), control)?;
        let key = active(memory::clone_owned(&node.id, control), control)?;
        active(labels.insert_node(key, source, control), control)?;
        let layout_shape = match node.shape.as_deref() {
            None | Some("square") => "squareRect",
            Some("round") => "roundedRect",
            Some(shape) => shape,
        };
        let model = FlowNode {
            id: node.id,
            provenance: node.provenance,
            label: Some(label),
            label_type: Some(active(
                memory::owned_string(title_kind_str(&node.label_type), control),
                control,
            )?),
            layout_shape: Some(active(
                memory::owned_string(layout_shape, control),
                control,
            )?),
            shape: node.shape,
            icon: node.icon,
            form: node.form,
            pos: node.pos,
            img: node.img,
            constraint: node.constraint,
            asset_width: node.asset_width,
            asset_height: node.asset_height,
            classes: node.classes,
            styles: node.styles,
            link: node.link,
            link_target: node.link_target,
            have_callback: node.have_callback,
        };
        active(memory::push(&mut render_nodes, model, control), control)?;
    }
    let mut render_edges = Vec::new();
    active(
        memory::reserve(&mut render_edges, edges.len(), control),
        control,
    )?;
    for edge in edges {
        let id = edge.id.ok_or(NativeParseError::Model)?;
        let label = edge
            .label
            .as_deref()
            .map(|raw| semantic_text(raw, control))
            .transpose()?;
        if let Some(raw) = edge.label.as_deref() {
            let source = active(memory::owned_string(raw, control), control)?;
            let key = active(memory::clone_owned(&id, control), control)?;
            active(labels.insert_edge(key, source, control), control)?;
        }
        let model = FlowEdge {
            id,
            from: edge.from,
            to: edge.to,
            label,
            label_type: Some(active(
                memory::owned_string(title_kind_str(&edge.label_type), control),
                control,
            )?),
            edge_type: Some(active(
                memory::owned_string(edge.link.compatibility_edge_type(), control),
                control,
            )?),
            stroke: Some(active(
                memory::owned_string(edge.link.compatibility_stroke(), control),
                control,
            )?),
            arrow: edge.link.end,
            start_marker: edge.link.start_marker,
            end_marker: edge.link.end_marker,
            is_user_defined_id: edge.is_user_defined_id,
            stroke_kind: edge.link.stroke_kind,
            visibility: edge.link.visibility,
            interpolate: edge.interpolate,
            classes: edge.classes,
            style: edge.style,
            animate: edge.animate,
            animation: edge.animation,
            length: edge.link.length,
        };
        active(memory::push(&mut render_edges, model, control), control)?;
    }
    let mut groups = Vec::new();
    for (ordinal, group) in builder.subgraphs.into_iter().enumerate() {
        let title = semantic_text(&group.title, control)?;
        let source = active(memory::clone_owned(&group.title, control), control)?;
        let id = active(memory::clone_owned(&group.id, control), control)?;
        active(
            labels.insert_subgraph(id, ordinal, Some(source), control),
            control,
        )?;
        active(
            memory::push(
                &mut groups,
                FlowSubgraph {
                    id: group.id,
                    title,
                    dir: group.dir,
                    has_explicit_dir: group.has_explicit_dir,
                    label_type: Some(group.label_type),
                    classes: group.classes,
                    styles: group.styles,
                    nodes: group.nodes,
                    metadata: None,
                },
                control,
            ),
            control,
        )?;
    }
    let model = FlowchartModel {
        keyword: ast.keyword,
        acc_title: input.title,
        acc_descr: input.description,
        class_defs: classes,
        direction: match ast.direction {
            Some(direction) => Some(direction),
            None => Some(active(memory::owned_string("TB", control), control)?),
        },
        edge_defaults: Some(FlowEdgeDefaults {
            interpolate: edge_defaults.interpolate,
            style: edge_defaults.style,
        }),
        vertex_calls,
        nodes: render_nodes,
        edges: render_edges,
        subgraphs: groups,
        tooltips,
        warning_facts: warnings,
    };
    let context = active(
        FlowchartRenderContext::new_controlled(
            labels,
            styles,
            collapsed,
            &model.subgraphs,
            control,
        ),
        control,
    )?;
    active(control.checkpoint(), control)?;
    Ok(NativePayload::new((model, context), region.clone()))
}

fn build_text(
    control: &OperationControl,
    emit: impl Fn(&mut dyn FnMut(&str) -> NativeResult<()>) -> NativeResult<()>,
) -> NativeResult<String> {
    let region = control
        .memory_region()
        .ok_or(NativeParseError::Memory(MemoryError::Unfunded))?;
    let mut length = 0usize;
    emit(&mut |part| {
        length = length.checked_add(part.len()).ok_or_else(|| {
            region
                .scope()
                .record_failure(MemoryError::ArithmeticOverflow);
            NativeParseError::Memory(MemoryError::ArithmeticOverflow)
        })?;
        Ok(())
    })?;
    let mut output = region
        .try_string_capacity(length, markdown_memory::Family::Model)
        .map_err(NativeParseError::Memory)?;
    emit(&mut |part| {
        if part.len() > output.capacity() - output.len() {
            return Err(NativeParseError::Model);
        }
        output.push_str(part);
        Ok(())
    })?;
    Ok(output)
}

/// A semantic-only Unicode projection. Rendering always starts with the raw
/// context and identifies markup before decoding its text tokens once.
pub(super) fn semantic_text(raw: &str, control: &OperationControl) -> NativeResult<String> {
    let region = control
        .memory_region()
        .ok_or(NativeParseError::Memory(MemoryError::Unfunded))?;
    let mut restored: Option<String> = None;
    for (pattern, replacement) in [("ﬂ°°", "&#"), ("ﬂ°", "&"), ("¶ß", ";")] {
        let current = restored.as_deref().unwrap_or(raw);
        if current.contains(pattern) {
            restored = Some(
                lex::replace_controlled(current, pattern, replacement, region)
                    .map_err(|_| stopped(control))?,
            );
        }
    }
    let current = restored.as_deref().unwrap_or(raw);
    let shorthand = if current.contains('#') {
        Some(build_text(control, |visit| {
            visit_shorthand(current, control, visit)
        })?)
    } else {
        None
    };
    let input = shorthand.as_deref().unwrap_or(current);
    build_text(control, |visit| {
        crate::entities::visit_decoded_html_entity_fragments_with_checkpoints(
            input,
            || {
                region
                    .scope()
                    .charge_work(64)
                    .map_err(NativeParseError::Memory)?;
                active(control.checkpoint(), control)
            },
            |fragment| match fragment {
                crate::entities::DecodedHtmlFragment::Borrowed(value) => visit(value),
                crate::entities::DecodedHtmlFragment::Scalar(value) => {
                    let mut buffer = [0u8; 4];
                    visit(value.encode_utf8(&mut buffer))
                }
            },
        )
    })
}

fn visit_shorthand(
    input: &str,
    control: &OperationControl,
    visit: &mut dyn FnMut(&str) -> NativeResult<()>,
) -> NativeResult<()> {
    if let Some(scope) = control.memory_scope() {
        scope
            .charge_work(input.len())
            .map_err(NativeParseError::Memory)?;
    }
    let mut position = 0usize;
    let mut emitted = 0usize;
    let mut previous = None;
    while position < input.len() {
        let ch = input[position..]
            .chars()
            .next()
            .ok_or(NativeParseError::Model)?;
        if ch != '#' {
            previous = Some(ch);
            position += ch.len_utf8();
            continue;
        }
        if previous == Some('&') {
            previous = Some('#');
            position += 1;
            continue;
        }
        let hash = position;
        position += 1;
        let entity_start = position;
        let mut terminated = false;
        for _ in 0..64 {
            match input.as_bytes().get(position) {
                Some(b';') => {
                    terminated = true;
                    position += 1;
                    break;
                }
                Some(byte) if byte.is_ascii_alphanumeric() || matches!(*byte, b'_' | b'+') => {
                    position += 1
                }
                _ => break,
            }
        }
        if terminated {
            let entity = &input[entity_start..position - 1];
            visit(&input[emitted..hash])?;
            visit("&")?;
            if entity
                .bytes()
                .all(|byte| byte.is_ascii_digit() || byte == b'+')
                && entity.bytes().any(|byte| byte.is_ascii_digit())
            {
                visit("#")?;
            }
            visit(entity)?;
            visit(";")?;
            emitted = position;
            previous = Some(';');
        }
    }
    visit(&input[emitted..])
}

fn validate_ast(ast: &FlowchartAst, control: &OperationControl) -> NativeResult<()> {
    let mut depth = 0usize;
    for event in active(ast.walk_controlled(control), control)? {
        match active(event, control)? {
            StatementEvent::Exit(_) => depth -= 1,
            StatementEvent::Enter(Stmt::Subgraph(_)) => {
                depth += 1;
                if depth > 8 {
                    return limited(control);
                }
            }
            StatementEvent::Enter(Stmt::Click(_)) => {
                return Err(NativeParseError::Unsupported {
                    feature: NativeFeature::Click,
                });
            }
            StatementEvent::Enter(Stmt::Node(node)) => validate_node(node, control)?,
            StatementEvent::Enter(Stmt::Chain { node_groups, .. }) => {
                for node in node_groups.iter().flatten() {
                    validate_node(node, control)?;
                }
            }
            _ => {}
        }
    }
    Ok(())
}

fn validate_node(node: &Node, control: &OperationControl) -> NativeResult<()> {
    if node.id.len() > 128 {
        return limited(control);
    }
    if node
        .shape
        .as_deref()
        .is_some_and(|shape| shape == "ellipse")
    {
        return Err(NativeParseError::Unsupported {
            feature: NativeFeature::Shape,
        });
    }
    Ok(())
}

fn prepare_flat(
    ast: &FlowchartAst,
    control: &OperationControl,
) -> NativeResult<HashMap<String, FlatShapeData>> {
    let mut documents = memory::new_map(control);
    fn insert_document(
        documents: &mut HashMap<String, FlatShapeData>,
        raw: &str,
        control: &OperationControl,
    ) -> NativeResult<()> {
        if documents.contains_key(raw) {
            return Ok(());
        }
        let document = FlatShapeData::parse(raw, control)?;
        let key = active(memory::owned_string(raw, control), control)?;
        active(memory::insert(documents, key, document, control), control)?;
        Ok(())
    }
    for event in active(ast.walk_controlled(control), control)? {
        match active(event, control)? {
            StatementEvent::Enter(Stmt::ShapeData { yaml, .. }) => {
                insert_document(&mut documents, yaml, control)?
            }
            StatementEvent::Enter(Stmt::Node(node)) => {
                if let Some(raw) = &node.shape_data {
                    insert_document(&mut documents, raw, control)?;
                }
            }
            StatementEvent::Enter(Stmt::Chain { node_groups, .. }) => {
                for node in node_groups.iter().flatten() {
                    if let Some(raw) = &node.shape_data {
                        insert_document(&mut documents, raw, control)?;
                    }
                }
            }
            _ => {}
        }
    }
    Ok(documents)
}

struct NativeInput {
    masked: Option<String>,
    title: Option<String>,
    description: Option<String>,
}

fn preflight(code: &str, control: &OperationControl) -> NativeResult<NativeInput> {
    let region = control
        .memory_region()
        .ok_or(NativeParseError::Memory(MemoryError::Unfunded))?;
    region
        .scope()
        .charge_work(code.len())
        .map_err(NativeParseError::Memory)?;
    let mut position = 0usize;
    let mut quote = None;
    let mut scalar_data = false;
    let mut depth = 0usize;
    let mut line_start = true;
    let mut line_begin = 0usize;
    let mut previous = None;
    let mut masked: Option<Vec<u8>> = None;
    let mut title = None;
    let mut description = None;
    while position < code.len() {
        if position.is_multiple_of(128) {
            active(control.checkpoint(), control)?;
        }
        let byte = code.as_bytes()[position];
        if let Some(active_quote) = quote {
            if scalar_data && active_quote == b'"' && byte == b'\\' {
                position += 1;
                if position < code.len() {
                    position += code[position..]
                        .chars()
                        .next()
                        .map(char::len_utf8)
                        .unwrap_or(1);
                }
                continue;
            }
            if scalar_data
                && active_quote == b'\''
                && byte == b'\''
                && code.as_bytes().get(position + 1) == Some(&b'\'')
            {
                position += 2;
                continue;
            }
            if byte == active_quote {
                quote = None;
            }
            position += 1;
            continue;
        }
        if byte == b'"' || scalar_data && byte == b'\'' {
            quote = Some(byte);
            position += 1;
            continue;
        }
        if code[position..].starts_with("@{") {
            scalar_data = true;
        }
        if depth == 0 {
            if code[position..].starts_with("%%") {
                if code[position + 2..].starts_with('{') {
                    return Err(NativeParseError::Unsupported {
                        feature: NativeFeature::Configuration,
                    });
                }
                position += code[position..].find('\n').unwrap_or(code.len() - position);
                continue;
            }
            if line_start {
                let rest = &code[position..];
                if rest
                    .strip_prefix("---")
                    .is_some_and(|after| after.is_empty() || after.starts_with(['\r', '\n']))
                {
                    return Err(NativeParseError::Unsupported {
                        feature: NativeFeature::Configuration,
                    });
                }
                let directive = rest
                    .strip_prefix("accTitle")
                    .map(|after| (true, after))
                    .or_else(|| rest.strip_prefix("accDescr").map(|after| (false, after)));
                if let Some((is_title, after)) = directive {
                    let delimiter = after.trim_start_matches(|ch| {
                        ch != '\r' && ch != '\n' && is_ecmascript_trim_char(ch)
                    });
                    if delimiter.starts_with(':') || !is_title && delimiter.starts_with('{') {
                        let delimiter_start = code.len() - delimiter.len();
                        let content_start = delimiter_start + 1;
                        let (content_end, end) = if delimiter.starts_with(':') {
                            let end = code[content_start..]
                                .find('\n')
                                .map(|offset| content_start + offset + 1)
                                .unwrap_or(code.len());
                            (end, end)
                        } else {
                            let close = code[content_start..]
                                .find('}')
                                .map(|offset| content_start + offset)
                                .ok_or(NativeParseError::Syntax {
                                    span: Some(SourceSpan::new(position, code.len())),
                                })?;
                            (close, close + 1)
                        };
                        let value = active(
                            memory::owned_string(code[content_start..content_end].trim(), control),
                            control,
                        )?;
                        if is_title {
                            title = Some(value);
                        } else {
                            description = Some(value);
                        }
                        if masked.is_none() {
                            masked = Some(
                                region
                                    .try_build_recipe(
                                        markdown_memory::Family::Parse,
                                        |plan| plan.array::<u8>(code.len()),
                                        |group| {
                                            let mut bytes = group.vector::<u8>(code.len())?;
                                            bytes.extend_from_slice(code.as_bytes());
                                            Ok(bytes)
                                        },
                                    )
                                    .map_err(NativeParseError::Memory)?,
                            );
                        }
                        if let Some(bytes) = masked.as_mut() {
                            region
                                .scope()
                                .charge_work(end - line_begin)
                                .map_err(NativeParseError::Memory)?;
                            for byte in &mut bytes[line_begin..end] {
                                if !matches!(*byte, b'\r' | b'\n') {
                                    *byte = b' ';
                                }
                            }
                        }
                        if let Some(last_newline) = code[line_begin..end].rfind('\n') {
                            line_begin += last_newline + 1;
                        }
                        position = end;
                        line_start = true;
                        continue;
                    }
                }
            }
        }
        let ch = code[position..]
            .chars()
            .next()
            .ok_or(NativeParseError::Model)?;
        match byte {
            b'[' | b'(' | b'{' => depth += 1,
            b'>' if depth == 0
                && previous.is_some_and(|previous: char| {
                    previous.is_alphanumeric() || matches!(previous, '_' | '.')
                }) =>
            {
                depth += 1
            }
            b']' | b')' | b'}' => {
                depth = depth.saturating_sub(1);
                if byte == b'}' {
                    scalar_data = false;
                }
            }
            b'\n' => {
                line_begin = position + 1;
                line_start = true;
            }
            b'\r' => line_start = true,
            _ if line_start && is_ecmascript_trim_char(ch) => {}
            _ => line_start = false,
        }
        if !is_ecmascript_trim_char(ch) {
            previous = Some(ch);
        }
        position += ch.len_utf8();
    }
    let masked = masked
        .map(String::from_utf8)
        .transpose()
        .map_err(|_| NativeParseError::Model)?;
    Ok(NativeInput {
        masked,
        title,
        description,
    })
}

fn unsupported_family(mut source: &str) -> bool {
    loop {
        source = source
            .trim_start_matches(is_ecmascript_trim_char)
            .trim_start_matches(';');
        source = source.trim_start_matches(is_ecmascript_trim_char);
        if source.starts_with("%%") {
            let Some(newline) = source.find('\n') else {
                return false;
            };
            source = &source[newline + 1..];
            continue;
        }
        break;
    }
    let end = source
        .find(|ch: char| is_ecmascript_trim_char(ch) || ch == ';')
        .unwrap_or(source.len());
    matches!(
        &source[..end],
        "sequenceDiagram"
            | "classDiagram"
            | "stateDiagram"
            | "stateDiagram-v2"
            | "erDiagram"
            | "journey"
            | "gantt"
            | "pie"
            | "quadrantChart"
            | "requirementDiagram"
            | "gitGraph"
            | "C4Context"
            | "C4Container"
            | "C4Component"
            | "C4Dynamic"
            | "C4Deployment"
            | "mindmap"
            | "timeline"
            | "zenuml"
            | "sankey-beta"
            | "xychart"
            | "xychart-beta"
            | "block"
            | "block-beta"
            | "packet"
            | "packet-beta"
            | "kanban"
            | "architecture-beta"
            | "radar-beta"
            | "radar"
            | "treemap-beta"
            | "treemap"
            | "flowchart-elk"
            | "swimlane-beta"
            | "agentflow-beta"
            | "cynefin"
            | "eventModel"
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use markdown_memory::{AllocationAuthority, Family, MemoryScope, NativeRegion};
    use std::{
        alloc::Layout,
        sync::{
            Arc,
            atomic::{AtomicUsize, Ordering},
        },
    };

    struct Authority {
        used: AtomicUsize,
        allocations: AtomicUsize,
        fail_at: AtomicUsize,
        fault: AtomicUsize,
    }
    impl AllocationAuthority for Authority {
        fn reserve(&self, layout: Layout, _: Family) -> std::result::Result<(), MemoryError> {
            self.used.fetch_add(layout.size(), Ordering::SeqCst);
            Ok(())
        }
        fn release(&self, layout: Layout, _: Family) {
            self.used.fetch_sub(layout.size(), Ordering::SeqCst);
        }
        fn failure(&self) -> Option<MemoryError> {
            match self.fault.load(Ordering::SeqCst) {
                1 => Some(MemoryError::BudgetLimited),
                2 => Some(MemoryError::OutOfMemory),
                _ => None,
            }
        }
        fn record_failure(&self, error: MemoryError) {
            let _ = self.fault.compare_exchange(
                0,
                if error == MemoryError::OutOfMemory {
                    2
                } else {
                    1
                },
                Ordering::SeqCst,
                Ordering::SeqCst,
            );
        }
        fn before_allocate(&self, _: Layout, _: Family) -> std::result::Result<(), MemoryError> {
            let site = self.allocations.fetch_add(1, Ordering::SeqCst) + 1;
            if site == self.fail_at.load(Ordering::SeqCst) {
                self.record_failure(MemoryError::OutOfMemory);
                return Err(MemoryError::OutOfMemory);
            }
            Ok(())
        }
    }
    fn make_control() -> (Arc<Authority>, OperationControl) {
        let authority = Arc::new(Authority {
            used: AtomicUsize::new(0),
            allocations: AtomicUsize::new(0),
            fail_at: AtomicUsize::new(usize::MAX),
            fault: AtomicUsize::new(0),
        });
        let scope = MemoryScope::new(authority.clone());
        let region = NativeRegion::try_new(&scope, 4096).unwrap();
        let control = OperationControl::new().with_memory_region(region);
        authority.allocations.store(0, Ordering::SeqCst);
        (authority, control)
    }
    const BASIC: &str = "flowchart LR\nA[Start]-->B{继续?}\nB-- yes -->C((Done))\nB-->A\nclassDef hot fill:#fff\nclass C hot\n";

    #[test]
    fn pure_policy_basic_source_produces_real_owned_model_and_exact_order_without_metadata_json() {
        let (authority, control) = make_control();
        let payload = parse(BASIC, &NativeParsePolicy::default(), &control).unwrap();
        let (model, context) = &*payload;
        assert_eq!(
            model
                .nodes
                .iter()
                .map(|node| node.id.as_str())
                .collect::<Vec<_>>(),
            ["A", "B", "C"]
        );
        assert_eq!(
            model
                .edges
                .iter()
                .map(|edge| edge.id.as_str())
                .collect::<Vec<_>>(),
            ["L_A_B_0", "L_B_C_0", "L_B_A_0"]
        );
        assert_eq!(
            context.node_label_for_render(&model.nodes[1]),
            Some("继续?")
        );
        assert_eq!(context.edge_label_for_render(&model.edges[1]), Some("yes"));
        assert_eq!(model.nodes[2].classes, ["hot"]);
        drop(control);
        assert!(authority.used.load(Ordering::SeqCst) > 0);
        drop(payload);
        assert_eq!(authority.used.load(Ordering::SeqCst), 0);
    }

    #[test]
    fn real_family_header_is_unsupported_while_broken_flowchart_and_quoted_words_are_not_reclassified()
     {
        for source in [
            "sequenceDiagram\nA->>B:hello",
            "%% comment\nsequenceDiagram\nA->>B:hello",
            "flowchart-elk\nA-->B",
        ] {
            let (_, control) = make_control();
            assert!(matches!(
                parse(source, &NativeParsePolicy::default(), &control),
                Err(NativeParseError::Unsupported {
                    feature: NativeFeature::Family
                })
            ));
        }
        let (_, control) = make_control();
        assert!(matches!(
            parse(
                "flowchart TB\nA-->\n",
                &NativeParsePolicy::default(),
                &control
            ),
            Err(NativeParseError::Syntax { .. })
        ));
        let (_, control) = make_control();
        assert!(
            parse(
                "flowchart TB\nA[\"sequenceDiagram\"]\n",
                &NativeParsePolicy::default(),
                &control
            )
            .is_ok()
        );
    }

    #[test]
    fn raw_context_keeps_literal_backslashes_entities_and_quoted_directive_spelling() {
        let (authority, control) = make_control();
        let code =
            "flowchart TB\nA[\"%%{init:{htmlLabels:true}}%% <br> &lt;br&gt; \\n \\nabla\"]\n";
        let payload = parse(code, &NativeParsePolicy::default(), &control).unwrap();
        let (model, context) = &*payload;
        assert_eq!(
            context.node_label_for_render(&model.nodes[0]),
            Some("%%{init:{htmlLabels:true}}%% <br> &lt;br&gt; \\n \\nabla")
        );
        assert_eq!(model.nodes[0].label_type.as_deref(), Some("string"));
        drop(payload);
        drop(control);
        assert_eq!(authority.used.load(Ordering::SeqCst), 0);
        let (_, control) = make_control();
        assert!(matches!(
            parse(
                "%%{init:{htmlLabels:true}}%%\nflowchart TB\nA\n",
                &NativeParsePolicy::default(),
                &control
            ),
            Err(NativeParseError::Unsupported {
                feature: NativeFeature::Configuration
            })
        ));
    }

    #[test]
    fn native_fanout_limit_is_observed_before_materializing_out_of_profile_edges() {
        let (authority, control) = make_control();
        let code = format!(
            "flowchart TB\n{}-->{}\n",
            (0..17)
                .map(|index| format!("A{index}"))
                .collect::<Vec<_>>()
                .join("&"),
            (0..16)
                .map(|index| format!("B{index}"))
                .collect::<Vec<_>>()
                .join("&")
        );
        assert!(matches!(
            parse(&code, &NativeParsePolicy::default(), &control),
            Err(NativeParseError::Memory(MemoryError::BudgetLimited))
        ));
        drop(control);
        assert_eq!(authority.used.load(Ordering::SeqCst), 0);
    }

    #[test]
    fn each_real_basic_parser_allocation_failure_preserves_fixed_reason_and_releases_every_owner() {
        let (first, first_control) = make_control();
        drop(parse(BASIC, &NativeParsePolicy::default(), &first_control).unwrap());
        let sites = first.allocations.load(Ordering::SeqCst);
        drop(first_control);
        assert_eq!(first.used.load(Ordering::SeqCst), 0);
        for site in 1..=sites {
            let (authority, control) = make_control();
            authority.fail_at.store(site, Ordering::SeqCst);
            assert!(
                matches!(
                    parse(BASIC, &NativeParsePolicy::default(), &control),
                    Err(NativeParseError::Memory(MemoryError::OutOfMemory))
                ),
                "site {site}"
            );
            drop(control);
            assert_eq!(authority.used.load(Ordering::SeqCst), 0, "site {site}");
        }
    }

    #[test]
    fn flat_last_value_null_and_later_declarations_follow_fixed_field_projection() {
        let (authority, control) = make_control();
        let source = "flowchart TB\nA[old]\nA@{shape:bogus,shape:rounded,label:first,label:null}\nA@{labelType:text}\nA\n";
        let payload = parse(source, &NativeParsePolicy::default(), &control).unwrap();
        assert_eq!(payload.0.nodes[0].shape.as_deref(), Some("rounded"));
        assert_eq!(payload.0.nodes[0].label.as_deref(), Some("old"));
        assert_eq!(payload.0.nodes[0].label_type.as_deref(), Some("text"));
        drop(payload);
        drop(control);
        assert_eq!(authority.used.load(Ordering::SeqCst), 0);
        for (scalar, expected) in [
            ("null", "old"),
            ("\"null\"", "null"),
            ("true", "true"),
            ("42", "42"),
            ("0x10", "16"),
            ("1_000", "1000"),
            ("NaN", "NaN"),
        ] {
            let (_, control) = make_control();
            let source = format!("flowchart TB\nA[old]\nA@{{label:{scalar}}}\n");
            let payload = parse(&source, &NativeParsePolicy::default(), &control).unwrap();
            assert_eq!(
                payload.1.node_label_for_render(&payload.0.nodes[0]),
                Some(expected),
                "{scalar}"
            );
        }
    }

    #[test]
    fn flat_quotes_keep_delimiters_decode_unicode_and_join_only_language_escaped_newlines() {
        let (_, control) = make_control();
        let source = "flowchart TB\nA@{label:'a}b, c:d''e',labelType:string}\nB@{label:\"x\\x41\\u4E2D\\U0001F600\\n\",labelType:text}\nC@{label:\"joined\\\n    word\"}\n";
        let payload = parse(source, &NativeParsePolicy::default(), &control).unwrap();
        assert_eq!(
            payload.1.node_label_for_render(&payload.0.nodes[0]),
            Some("a}b, c:d'e")
        );
        assert_eq!(
            payload.1.node_label_for_render(&payload.0.nodes[1]),
            Some("xA中😀\n")
        );
        assert_eq!(
            payload.1.node_label_for_render(&payload.0.nodes[2]),
            Some("joinedword")
        );
    }

    #[test]
    fn flat_target_role_unsupported_yaml_and_unknown_shape_remain_distinct() {
        for source in [
            "flowchart TB\nA@{label:{nested:true}}\n",
            "flowchart TB\nA@{label:*alias}\n",
        ] {
            let (_, control) = make_control();
            assert!(matches!(
                parse(source, &NativeParsePolicy::default(), &control),
                Err(NativeParseError::Unsupported {
                    feature: NativeFeature::ShapeData
                })
            ));
        }
        for source in [
            "flowchart TB\nsubgraph G\nA\nend\nG@{label:group}\n",
            "flowchart TB\nA edge1@--> B\nedge1@{label:edge}\n",
        ] {
            let (_, control) = make_control();
            assert!(matches!(
                parse(source, &NativeParsePolicy::default(), &control),
                Err(NativeParseError::Unsupported {
                    feature: NativeFeature::Metadata
                })
            ));
        }
        let (_, control) = make_control();
        assert!(matches!(
            parse(
                "flowchart TB\nA@{shape:no-such-shape}\n",
                &NativeParsePolicy::default(),
                &control
            ),
            Err(NativeParseError::Syntax { .. })
        ));
        let (_, control) = make_control();
        assert!(matches!(
            parse(
                "flowchart TB\nA@{shape:cloud}\n",
                &NativeParsePolicy::default(),
                &control
            ),
            Err(NativeParseError::Unsupported {
                feature: NativeFeature::Shape
            })
        ));
    }

    #[test]
    fn each_flat_reader_allocation_failure_releases_partial_scalar_and_model_owners() {
        const SOURCE: &str =
            "flowchart TB\nA@{shape:rounded,label:'中文, quote''s'}\nA@{label:\"next\\u4E2D\"}\n";
        let (first, first_control) = make_control();
        drop(parse(SOURCE, &NativeParsePolicy::default(), &first_control).unwrap());
        let count = first.allocations.load(Ordering::SeqCst);
        drop(first_control);
        for site in 1..=count {
            let (authority, control) = make_control();
            authority.fail_at.store(site, Ordering::SeqCst);
            assert!(
                matches!(
                    parse(SOURCE, &NativeParsePolicy::default(), &control),
                    Err(NativeParseError::Memory(MemoryError::OutOfMemory))
                ),
                "site {site}"
            );
            drop(control);
            assert_eq!(authority.used.load(Ordering::SeqCst), 0, "site {site}");
        }
    }

    #[test]
    fn accessibility_keeps_original_text_masks_exact_bytes_and_ignores_label_payload_spelling() {
        let (authority, control) = make_control();
        let source = "flowchart TB\r\naccTitle: 标题\r\naccDescr {\r\n  First line\r\n  第二行\r\n} A-->B\r\nC[\"accTitle: literal\n%%{init:{htmlLabels:true}}%%\"]\r\nD>plain\naccDescr: label text]\r\n";
        let input = preflight(source, &control).unwrap();
        let masked = input.masked.as_ref().unwrap();
        assert_eq!(masked.len(), source.len());
        assert_eq!(
            masked
                .match_indices('\n')
                .map(|(index, _)| index)
                .collect::<Vec<_>>(),
            source
                .match_indices('\n')
                .map(|(index, _)| index)
                .collect::<Vec<_>>()
        );
        assert_eq!(input.title.as_deref(), Some("标题"));
        assert_eq!(input.description.as_deref(), Some("First line\r\n  第二行"));
        let payload = parse(source, &NativeParsePolicy::default(), &control).unwrap();
        assert_eq!(payload.0.acc_title.as_deref(), Some("标题"));
        assert_eq!(
            payload.0.acc_descr.as_deref(),
            Some("First line\r\n  第二行")
        );
        assert_eq!(payload.0.nodes.len(), 4);
        assert_eq!(
            payload.1.node_label_for_render(&payload.0.nodes[2]),
            Some("accTitle: literal\n%%{init:{htmlLabels:true}}%%")
        );
        assert_eq!(
            payload.1.node_label_for_render(&payload.0.nodes[3]),
            Some("plain\naccDescr: label text")
        );
        drop(payload);
        drop(input);
        drop(control);
        assert_eq!(authority.used.load(Ordering::SeqCst), 0);
    }

    #[test]
    fn each_accessibility_mask_allocation_failure_releases_the_real_original_and_masked_values() {
        const SOURCE: &str = "flowchart TB\naccTitle: 标题\naccDescr {\n  one\n  two\n}\nA-->B\n";
        let (first, first_control) = make_control();
        drop(parse(SOURCE, &NativeParsePolicy::default(), &first_control).unwrap());
        let count = first.allocations.load(Ordering::SeqCst);
        drop(first_control);
        for site in 1..=count {
            let (authority, control) = make_control();
            authority.fail_at.store(site, Ordering::SeqCst);
            assert!(
                matches!(
                    parse(SOURCE, &NativeParsePolicy::default(), &control),
                    Err(NativeParseError::Memory(MemoryError::OutOfMemory))
                ),
                "site {site}"
            );
            drop(control);
            assert_eq!(authority.used.load(Ordering::SeqCst), 0, "site {site}");
        }
    }

    #[test]
    fn native_unicode_semantics_keep_raw_render_provenance_and_decode_only_once() {
        let (authority, control) = make_control();
        let source = "flowchart TB\nsubgraph G[\"&lt;br&gt;\"]\nA[\"&amp; #65; #hearts; &#x4E2D; &lt;br&gt; &ast;x&ast; &bogus; \\n &amp;amp;\"]\nend\nA-->|\"&lt;i&gt; #169;\"|B\n";
        let payload = parse(source, &NativeParsePolicy::default(), &control).unwrap();
        let node = &payload.0.nodes[0];
        assert_eq!(
            node.label.as_deref(),
            Some("& A ♥ 中 <br> *x* &bogus; \\n &amp;")
        );
        assert_eq!(
            payload.1.node_label_for_render(node),
            Some("&amp; #65; #hearts; &#x4E2D; &lt;br&gt; &ast;x&ast; &bogus; \\n &amp;amp;")
        );
        assert_eq!(payload.0.edges[0].label.as_deref(), Some("<i> ©"));
        assert_eq!(
            payload.1.edge_label_for_render(&payload.0.edges[0]),
            Some("&lt;i&gt; #169;")
        );
        assert_eq!(payload.0.subgraphs[0].title, "<br>");
        assert_eq!(
            payload
                .1
                .subgraph_title_for_render(0, &payload.0.subgraphs[0]),
            "&lt;br&gt;"
        );
        drop(payload);
        drop(control);
        assert_eq!(authority.used.load(Ordering::SeqCst), 0);
    }

    #[test]
    fn native_limits_are_checked_during_authored_growth_and_preserve_first_failure() {
        let mut nodes = String::from("flowchart TB\n");
        for index in 0..129 {
            nodes.push_str(&format!("N{index}\n"));
        }
        let mut groups = String::from("flowchart TB\n");
        for index in 0..33 {
            groups.push_str(&format!("subgraph G{index}\nend\n"));
        }
        let depth = format!(
            "flowchart TB\n{}A\n{}",
            (0..9)
                .map(|index| format!("subgraph G{index}\n"))
                .collect::<String>(),
            "end\n".repeat(9)
        );
        let id = format!("flowchart TB\n{}\n", "a".repeat(129));
        let edges = format!("flowchart TB\n{}", "A-->B\n".repeat(257));
        for source in [nodes, groups, depth, id, edges] {
            let (authority, control) = make_control();
            assert!(matches!(
                parse(&source, &NativeParsePolicy::default(), &control),
                Err(NativeParseError::Memory(MemoryError::BudgetLimited))
            ));
            assert_eq!(
                control.memory_scope().unwrap().failure(),
                Some(MemoryError::BudgetLimited)
            );
            assert!(matches!(
                parse(
                    "flowchart TB\nA-->B",
                    &NativeParsePolicy::default(),
                    &control
                ),
                Err(NativeParseError::Memory(MemoryError::BudgetLimited))
            ));
            drop(control);
            assert_eq!(authority.used.load(Ordering::SeqCst), 0);
        }
    }

    #[test]
    fn malformed_link_style_diagnostic_is_funded_and_drops_partial_model() {
        const SOURCE: &str = "flowchart TB\nA-->B\nlinkStyle 9 stroke:red\n";
        let (first, first_control) = make_control();
        assert!(matches!(
            parse(SOURCE, &NativeParsePolicy::default(), &first_control),
            Err(NativeParseError::Syntax { .. })
        ));
        let count = first.allocations.load(Ordering::SeqCst);
        drop(first_control);
        assert_eq!(first.used.load(Ordering::SeqCst), 0);
        for site in 1..=count {
            let (authority, control) = make_control();
            authority.fail_at.store(site, Ordering::SeqCst);
            assert!(
                matches!(
                    parse(SOURCE, &NativeParsePolicy::default(), &control),
                    Err(NativeParseError::Memory(MemoryError::OutOfMemory))
                ),
                "site {site}"
            );
            drop(control);
            assert_eq!(authority.used.load(Ordering::SeqCst), 0, "site {site}");
        }
    }
}
