use crate::diagrams::langium_common::{
    LangiumCommonField, parse_langium_common, push_langium_common_editor_fact,
};
use crate::diagrams::scan::{physical_line_at, split_ascii_indent};
use crate::{
    EditorExpectedSyntax, EditorExpectedSyntaxKind, EditorSemanticFacts, EditorSemanticKind,
    EditorSemanticSymbol, Error, MAX_DIAGRAM_NESTING_DEPTH, ParseMetadata, Result, SourceSpan,
};
use serde_json::{Value, json};
use std::collections::HashMap;

const TREE_VIEW_FILE_NODE_TYPE: &str = "file";
const TREE_VIEW_DIRECTORY_NODE_TYPE: &str = "directory";

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct TreeViewNodeRenderModel {
    pub id: i64,
    pub level: i64,
    pub name: String,
    #[serde(rename = "nodeType")]
    pub node_type: String,
    #[serde(default, rename = "cssClass", skip_serializing_if = "Option::is_none")]
    pub css_class: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub icon: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(default)]
    pub children: Vec<TreeViewNodeRenderModel>,
}

impl Default for TreeViewNodeRenderModel {
    fn default() -> Self {
        Self {
            id: 0,
            level: -1,
            name: "/".to_string(),
            node_type: TREE_VIEW_DIRECTORY_NODE_TYPE.to_string(),
            css_class: None,
            icon: None,
            description: None,
            children: Vec::new(),
        }
    }
}

#[derive(Debug, Clone, Default, serde::Serialize, serde::Deserialize)]
pub struct TreeViewDiagramRenderModel {
    #[serde(default, rename = "accTitle")]
    pub acc_title: Option<String>,
    #[serde(default, rename = "accDescr")]
    pub acc_descr: Option<String>,
    #[serde(default)]
    pub title: Option<String>,
    pub root: TreeViewNodeRenderModel,
}

impl TreeViewDiagramRenderModel {
    pub(crate) fn sanitize_common_db_fields(&mut self, config: &crate::MermaidConfig) {
        crate::common_db::sanitize_optional_title(&mut self.title, config);
        crate::common_db::sanitize_optional_acc_title(&mut self.acc_title, config);
        crate::common_db::sanitize_optional_acc_descr(&mut self.acc_descr, config);
    }
}

#[derive(Debug, Clone)]
struct ParsedTreeViewInput {
    title: Option<String>,
    acc_title: Option<String>,
    acc_descr: Option<String>,
    nodes: Vec<TreeViewNodeLineDetails>,
    editor_facts: EditorSemanticFacts,
}

#[derive(Debug, Clone)]
struct TreeViewParseIssue {
    message: String,
    span: Option<SourceSpan>,
}

struct TreeViewParseOutcome {
    snapshot: ParsedTreeViewInput,
    first_issue: Option<TreeViewParseIssue>,
}

impl TreeViewParseOutcome {
    fn into_strict(self, meta: &ParseMetadata) -> Result<ParsedTreeViewInput> {
        match self.first_issue {
            Some(issue) => Err(issue.into_error(meta)),
            None => Ok(self.snapshot),
        }
    }

    fn into_combined(self, meta: &ParseMetadata) -> crate::family::CombinedSemanticParse {
        let Self {
            snapshot,
            first_issue,
        } = self;
        let construction = match first_issue {
            Some(issue) => Err(crate::family::CombinedSemanticFailure::new(
                issue.into_error(meta),
                snapshot.editor_facts,
            )),
            None => Ok(snapshot),
        };
        crate::family::CombinedSemanticParse::from_construction(
            construction,
            |snapshot| {
                let ParsedTreeViewInput {
                    title,
                    acc_title,
                    acc_descr,
                    nodes,
                    mut editor_facts,
                } = snapshot;
                let model = tree_view_input_to_render_model(
                    title,
                    acc_title,
                    acc_descr,
                    nodes,
                    &meta.effective_config,
                )
                .map_err(|message| {
                    editor_facts.mark_recovered_from_parse_error(message.clone(), None);
                    Error::diagram_parse_fallback(meta.diagram_type.clone(), message)
                })
                .and_then(|model| render_model_to_compat_json(&model, meta));
                (model, editor_facts)
            },
            crate::family::CombinedSemanticFailure::into_parts,
        )
    }
}

impl TreeViewParseIssue {
    fn new(message: impl Into<String>, span: Option<SourceSpan>) -> Self {
        Self {
            message: message.into(),
            span,
        }
    }

    fn into_error(self, meta: &ParseMetadata) -> Error {
        match self.span {
            Some(span) => {
                Error::diagram_parse_exact(meta.diagram_type.clone(), &self.message, span)
            }
            None => Error::diagram_parse_fallback(meta.diagram_type.clone(), &self.message),
        }
    }
}

struct TreeViewSemanticSource {
    render_model: TreeViewDiagramRenderModel,
}

#[derive(Debug, Clone)]
struct TreeViewNodeStatement {
    line: String,
    line_start: usize,
}

struct TreeViewNodeParseFailure {
    error: Box<Error>,
}

#[derive(Debug, Clone)]
struct TreeViewNodeLineDetails {
    indent: usize,
    name: String,
    node_type: String,
    css_class: Option<TreeViewSpannedValue>,
    icon: Option<TreeViewSpannedValue>,
    description: Option<TreeViewSpannedValue>,
    span: SourceSpan,
    selection: SourceSpan,
}

#[derive(Debug, Clone)]
struct TreeViewSpannedValue {
    value: String,
    span: SourceSpan,
}

#[derive(Debug, Clone, Default)]
struct TreeViewAnnotations {
    css_class: Option<TreeViewSpannedValue>,
    icon: Option<TreeViewSpannedValue>,
    description: Option<TreeViewSpannedValue>,
}

#[derive(Debug, Clone, Copy)]
struct TreeViewLineFormat {
    box_drawing: bool,
    segment_width: usize,
}

#[derive(Debug, Clone, Copy)]
struct TreeViewLineView<'a> {
    indent: usize,
    content: &'a str,
    content_offset: usize,
    expand_content_tabs: bool,
}

#[derive(Debug, Clone)]
struct ArenaNode {
    id: i64,
    level: i64,
    name: String,
    node_type: String,
    css_class: Option<String>,
    icon: Option<String>,
    description: Option<String>,
    children: Vec<usize>,
}

pub(crate) fn parse_tree_view(code: &str, meta: &ParseMetadata) -> Result<Value> {
    let source = parse_tree_view_semantic_source(code, meta)?;
    render_model_to_compat_json(&source.render_model, meta)
}

pub(crate) fn parse_tree_view_json_and_editor_facts(
    code: &str,
    meta: &ParseMetadata,
    control: &crate::OperationControl,
) -> crate::OperationControlResult<crate::family::CombinedSemanticParse> {
    #[cfg(test)]
    crate::diagrams::langium_common::record_family_syntax_construction("treeView");
    let parsed = parse_tree_view_input_controlled(code, meta, control)?.into_combined(meta);
    control.checkpoint()?;
    Ok(parsed)
}

#[cfg(test)]
pub(crate) fn parse_tree_view_model_for_render(
    code: &str,
    meta: &ParseMetadata,
) -> Result<TreeViewDiagramRenderModel> {
    Ok(parse_tree_view_semantic_source(code, meta)?.render_model)
}

pub(crate) fn parse_tree_view_model_for_render_controlled(
    code: &str,
    meta: &ParseMetadata,
    control: &crate::OperationControl,
) -> crate::OperationControlResult<Result<TreeViewDiagramRenderModel>> {
    let parsed = parse_tree_view_input_controlled(code, meta, control)?;
    let parsed = match parsed.into_strict(meta) {
        Ok(parsed) => parsed,
        Err(error) => return Ok(Err(error)),
    };
    let ParsedTreeViewInput {
        title,
        acc_title,
        acc_descr,
        nodes,
        editor_facts: _,
    } = parsed;
    let model =
        tree_view_input_to_render_model(title, acc_title, acc_descr, nodes, &meta.effective_config)
            .map_err(|message| Error::diagram_parse_fallback(meta.diagram_type.clone(), message));
    control.checkpoint()?;
    Ok(model)
}

pub(crate) fn render_model_to_compat_json(
    model: &TreeViewDiagramRenderModel,
    meta: &ParseMetadata,
) -> Result<Value> {
    let mut nodes = Vec::new();
    flatten_nodes(&model.root, &mut nodes);
    Ok(json!({
        "type": meta.diagram_type,
        "title": model.title,
        "accTitle": model.acc_title,
        "accDescr": model.acc_descr,
        "root": tree_view_node_to_value(&model.root),
        "nodes": nodes,
    }))
}

fn parse_tree_view_semantic_source(
    code: &str,
    meta: &ParseMetadata,
) -> Result<TreeViewSemanticSource> {
    construct_tree_view_semantic_source(code, meta)
}

fn construct_tree_view_semantic_source(
    code: &str,
    meta: &ParseMetadata,
) -> Result<TreeViewSemanticSource> {
    #[cfg(test)]
    crate::diagrams::langium_common::record_family_syntax_construction("treeView");

    let parsed = parse_tree_view_input(code, meta).into_strict(meta)?;
    let ParsedTreeViewInput {
        title,
        acc_title,
        acc_descr,
        nodes,
        editor_facts: _,
    } = parsed;
    let render_model =
        tree_view_input_to_render_model(title, acc_title, acc_descr, nodes, &meta.effective_config)
            .map_err(|message| Error::diagram_parse_fallback(meta.diagram_type.clone(), message))?;
    Ok(TreeViewSemanticSource { render_model })
}

fn parse_tree_view_input(code: &str, meta: &ParseMetadata) -> TreeViewParseOutcome {
    parse_tree_view_input_controlled(code, meta, &crate::OperationControl::new())
        .expect("a private parse control cannot be cancelled")
}

fn parse_tree_view_input_controlled(
    code: &str,
    meta: &ParseMetadata,
    control: &crate::OperationControl,
) -> crate::OperationControlResult<TreeViewParseOutcome> {
    control.checkpoint()?;
    let mut editor_facts = EditorSemanticFacts::new();
    let mut first_issue = None;
    let body = match tree_view_body_start_controlled(code, control)? {
        Ok(body) => body,
        Err(issue) => {
            mark_tree_view_recovery(&mut editor_facts, &mut first_issue, issue);
            return Ok(TreeViewParseOutcome {
                snapshot: ParsedTreeViewInput {
                    title: None,
                    acc_title: None,
                    acc_descr: None,
                    nodes: Vec::new(),
                    editor_facts,
                },
                first_issue,
            });
        }
    };
    let mut offset = body.offset;
    let mut title = None;
    let mut acc_title = None;
    let mut acc_descr = None;
    let mut node_statements = Vec::new();
    let mut saw_node = false;

    while offset < code.len() {
        control.checkpoint()?;
        if let Some(parsed) = parse_langium_common(code, offset) {
            if saw_node {
                let span = parsed.fact.raw_span;
                mark_tree_view_recovery(
                    &mut editor_facts,
                    &mut first_issue,
                    TreeViewParseIssue::new(
                        "tree view title and accessibility fields must precede every node",
                        Some(span),
                    ),
                );
                offset += parsed.consumed;
                continue;
            }
            let field = parsed.fact.field;
            let value = parsed.fact.value.clone();
            push_langium_common_editor_fact(&mut editor_facts, &parsed.fact, "tree view");
            match field {
                LangiumCommonField::Title => title = Some(value),
                LangiumCommonField::AccTitle => acc_title = Some(value),
                LangiumCommonField::AccDescr => acc_descr = Some(value),
            }
            if let Some(diagnostic) = parsed.diagnostic {
                mark_tree_view_recovery(
                    &mut editor_facts,
                    &mut first_issue,
                    TreeViewParseIssue::new(diagnostic.message, Some(diagnostic.span)),
                );
            }
            offset += parsed.consumed;
            continue;
        }

        let (line, next_offset) = physical_line_at(code, offset);
        let visible = strip_inline_comment_aware(line);
        if visible.trim().is_empty() {
            offset = next_offset;
            continue;
        }
        saw_node = true;
        node_statements.push(TreeViewNodeStatement {
            line: visible.to_string(),
            line_start: offset,
        });
        offset = next_offset;
    }

    let node_lines = node_statements
        .iter()
        .map(|statement| statement.line.as_str())
        .collect::<Vec<_>>();
    let line_format = TreeViewLineFormat::from_lines(&node_lines);
    let mut nodes = Vec::new();
    for (index, statement) in node_statements.into_iter().enumerate() {
        if index % 128 == 0 {
            control.checkpoint()?;
        }
        match parse_node_line_details(&statement.line, statement.line_start, line_format, meta) {
            Ok(Some(node)) => {
                push_tree_view_node_editor_facts(&mut editor_facts, &node);
                nodes.push(node);
            }
            Ok(None) => {}
            Err(failure) => {
                let trimmed = statement.line.trim();
                let leading = statement
                    .line
                    .len()
                    .saturating_sub(statement.line.trim_start().len());
                let span = SourceSpan::new(
                    statement.line_start + leading,
                    statement.line_start + leading + trimmed.len(),
                );
                mark_tree_view_recovery(
                    &mut editor_facts,
                    &mut first_issue,
                    TreeViewParseIssue::new(tree_view_error_message(&failure.error), Some(span)),
                );
            }
        }
    }

    control.checkpoint()?;
    Ok(TreeViewParseOutcome {
        snapshot: ParsedTreeViewInput {
            title,
            acc_title,
            acc_descr,
            nodes,
            editor_facts,
        },
        first_issue,
    })
}

#[derive(Debug, Clone, Copy)]
struct TreeViewBodyStart {
    offset: usize,
}

fn tree_view_body_start_controlled(
    code: &str,
    control: &crate::OperationControl,
) -> crate::OperationControlResult<std::result::Result<TreeViewBodyStart, TreeViewParseIssue>> {
    let mut offset = 0usize;
    while offset < code.len() {
        control.checkpoint()?;
        let (line, next_offset) = physical_line_at(code, offset);
        let visible = strip_inline_comment_aware(line);
        let trimmed = visible.trim();
        if trimmed.is_empty() {
            offset = next_offset;
            continue;
        }

        let leading = visible.len().saturating_sub(visible.trim_start().len());
        let span = SourceSpan::new(offset + leading, offset + leading + trimmed.len());
        let Some(trailing) = trimmed.strip_prefix("treeView-beta") else {
            return Ok(Err(TreeViewParseIssue::new(
                "expected treeView-beta",
                Some(span),
            )));
        };
        if !trailing.trim().is_empty() {
            return Ok(Err(TreeViewParseIssue::new(
                "unexpected tokens after treeView-beta",
                Some(span),
            )));
        }
        return Ok(Ok(TreeViewBodyStart {
            offset: next_offset,
        }));
    }

    Ok(Err(TreeViewParseIssue::new("expected treeView-beta", None)))
}

fn mark_tree_view_recovery(
    editor_facts: &mut EditorSemanticFacts,
    first_issue: &mut Option<TreeViewParseIssue>,
    issue: TreeViewParseIssue,
) {
    if first_issue.is_none() {
        *first_issue = Some(issue.clone());
    }
    editor_facts.mark_recovered_from_parse_error(
        format!(
            "treeView parser recovered after parse error: {}",
            issue.message
        ),
        issue.span,
    );
}

fn tree_view_error_message(error: &Error) -> String {
    match error {
        Error::DiagramParse { diagnostic, .. } => diagnostic.message().to_string(),
        _ => error.to_string(),
    }
}

fn tree_view_input_to_render_model(
    title: Option<String>,
    acc_title: Option<String>,
    acc_descr: Option<String>,
    nodes: Vec<TreeViewNodeLineDetails>,
    config: &crate::MermaidConfig,
) -> std::result::Result<TreeViewDiagramRenderModel, String> {
    let mut arena = vec![ArenaNode {
        id: 0,
        level: -1,
        name: "/".to_string(),
        node_type: TREE_VIEW_DIRECTORY_NODE_TYPE.to_string(),
        css_class: None,
        icon: None,
        description: None,
        children: Vec::new(),
    }];
    let mut stack = vec![0usize];
    for (next_id, flat) in (1i64..).zip(nodes) {
        let level = flat.indent as i64;
        while stack
            .last()
            .and_then(|&idx| arena.get(idx))
            .is_some_and(|node| level <= node.level)
        {
            stack.pop();
        }

        let parent = stack.last().copied().unwrap_or(0);
        let idx = arena.len();
        arena.push(ArenaNode {
            id: next_id,
            level,
            name: flat.name,
            node_type: flat.node_type,
            css_class: flat.css_class.map(|value| value.value),
            icon: flat.icon.map(|value| value.value),
            description: flat
                .description
                .map(|value| crate::sanitize::sanitize_text(&value.value, config)),
            children: Vec::new(),
        });
        arena[parent].children.push(idx);
        stack.push(idx);
        if stack.len().saturating_sub(1) > MAX_DIAGRAM_NESTING_DEPTH {
            return Err(format!(
                "treeView nesting depth exceeds {MAX_DIAGRAM_NESTING_DEPTH}"
            ));
        }
    }

    Ok(TreeViewDiagramRenderModel {
        title,
        acc_title,
        acc_descr,
        root: arena_node_to_render_model(&arena, 0),
    })
}

fn arena_node_to_render_model(arena: &[ArenaNode], idx: usize) -> TreeViewNodeRenderModel {
    let mut models: Vec<Option<TreeViewNodeRenderModel>> = vec![None; arena.len()];
    let mut stack = vec![(idx, false)];

    while let Some((node_idx, visited)) = stack.pop() {
        let Some(node) = arena.get(node_idx) else {
            continue;
        };

        if visited {
            let children = node
                .children
                .iter()
                .filter_map(|&child_idx| models.get_mut(child_idx).and_then(Option::take))
                .collect();
            models[node_idx] = Some(TreeViewNodeRenderModel {
                id: node.id,
                level: node.level,
                name: node.name.clone(),
                node_type: node.node_type.clone(),
                css_class: node.css_class.clone(),
                icon: node.icon.clone(),
                description: node.description.clone(),
                children,
            });
        } else {
            stack.push((node_idx, true));
            for &child_idx in node.children.iter().rev() {
                stack.push((child_idx, false));
            }
        }
    }

    models
        .get_mut(idx)
        .and_then(Option::take)
        .unwrap_or_default()
}

fn flatten_nodes(node: &TreeViewNodeRenderModel, out: &mut Vec<Value>) {
    let mut stack = vec![node];
    while let Some(current) = stack.pop() {
        out.push(tree_view_flat_node_to_value(current));
        for child in current.children.iter().rev() {
            stack.push(child);
        }
    }
}

fn tree_view_node_to_value(root: &TreeViewNodeRenderModel) -> Value {
    let mut values: HashMap<*const TreeViewNodeRenderModel, Value> = HashMap::new();
    let mut stack = vec![(root, false)];

    while let Some((node, visited)) = stack.pop() {
        let node_ptr = std::ptr::from_ref(node);
        if visited {
            let children = node
                .children
                .iter()
                .filter_map(|child| values.remove(&std::ptr::from_ref(child)))
                .collect::<Vec<_>>();
            values.insert(
                node_ptr,
                tree_view_node_with_children_to_value(node, children),
            );
        } else {
            stack.push((node, true));
            for child in node.children.iter().rev() {
                stack.push((child, false));
            }
        }
    }

    values.remove(&std::ptr::from_ref(root)).unwrap_or_else(|| {
        json!({
            "id": 0,
            "level": -1,
            "name": "/",
            "nodeType": TREE_VIEW_DIRECTORY_NODE_TYPE,
            "children": [],
        })
    })
}

fn tree_view_flat_node_to_value(node: &TreeViewNodeRenderModel) -> Value {
    let mut value = serde_json::Map::new();
    value.insert("id".to_string(), json!(node.id));
    value.insert("level".to_string(), json!(node.level));
    value.insert("name".to_string(), json!(node.name));
    value.insert("nodeType".to_string(), json!(node.node_type));
    if let Some(css_class) = &node.css_class {
        value.insert("cssClass".to_string(), json!(css_class));
    }
    if let Some(icon) = &node.icon {
        value.insert("icon".to_string(), json!(icon));
    }
    if let Some(description) = &node.description {
        value.insert("description".to_string(), json!(description));
    }
    Value::Object(value)
}

fn tree_view_node_with_children_to_value(
    node: &TreeViewNodeRenderModel,
    children: Vec<Value>,
) -> Value {
    let mut value = match tree_view_flat_node_to_value(node) {
        Value::Object(value) => value,
        _ => serde_json::Map::new(),
    };
    value.insert("children".to_string(), Value::Array(children));
    Value::Object(value)
}

fn parse_node_line_details(
    line: &str,
    line_start: usize,
    line_format: TreeViewLineFormat,
    meta: &ParseMetadata,
) -> std::result::Result<Option<TreeViewNodeLineDetails>, TreeViewNodeParseFailure> {
    let line_view =
        tree_view_line_view(line, line_format, meta).map_err(|error| TreeViewNodeParseFailure {
            error: Box::new(error),
        })?;
    let Some(line_view) = line_view else {
        return Ok(None);
    };
    parse_node_content(line_view, line_start, meta).map(Some)
}

impl TreeViewLineFormat {
    fn from_lines(lines: &[&str]) -> Self {
        let mut content_lines = Vec::new();
        for line in lines {
            if should_skip_tree_view_box_detection_line(line) {
                continue;
            }
            content_lines.push(line.replace('\t', "    "));
        }
        let box_drawing = content_lines
            .iter()
            .any(|line| line.chars().any(is_tree_view_box_char));
        let segment_width = if box_drawing {
            infer_tree_view_box_segment_width(&content_lines)
        } else {
            4
        };
        Self {
            box_drawing,
            segment_width,
        }
    }
}

fn tree_view_line_view<'a>(
    line: &'a str,
    line_format: TreeViewLineFormat,
    meta: &ParseMetadata,
) -> Result<Option<TreeViewLineView<'a>>> {
    if !line_format.box_drawing {
        let (indent, rest) = split_ascii_indent(line);
        let content = rest.trim_end();
        if content.is_empty() {
            return Ok(None);
        }
        let content_offset = line.len().saturating_sub(rest.len());
        return Ok(Some(TreeViewLineView {
            indent,
            content,
            content_offset,
            expand_content_tabs: false,
        }));
    }

    if is_tree_view_decoration_only(line) {
        return Ok(None);
    }

    if let Some((branch_byte, branch_col, branch_char)) = find_tree_view_branch_char(line) {
        let depth = ((branch_col as f64 / line_format.segment_width as f64).round() as usize) + 1;
        let mut content_offset = branch_byte + branch_char.len_utf8();
        while let Some(ch) = line[content_offset..].chars().next() {
            if is_tree_view_dash_char(ch) {
                content_offset += ch.len_utf8();
            } else {
                break;
            }
        }
        while let Some(ch) = line[content_offset..].chars().next() {
            if ch == ' ' || ch == '\t' {
                content_offset += ch.len_utf8();
            } else {
                break;
            }
        }
        let content = line[content_offset..].trim_end();
        if content.is_empty() {
            return Err(parse_error(
                meta,
                "empty tree node after box-drawing prefix",
            ));
        }
        return Ok(Some(TreeViewLineView {
            indent: depth * 4,
            content,
            content_offset,
            expand_content_tabs: true,
        }));
    }

    if is_tree_view_box_drawing_only(line) {
        return Ok(None);
    }

    if line.chars().any(is_tree_view_box_char) {
        let content = line.trim_end();
        return Ok(Some(TreeViewLineView {
            indent: 0,
            content,
            content_offset: 0,
            expand_content_tabs: false,
        }));
    }

    if line.chars().next().is_some_and(char::is_whitespace) {
        return Err(parse_error(
            meta,
            "unexpected indentation without box-drawing prefix in treeView box-drawing input",
        ));
    }

    let content = line.trim_end();
    Ok(Some(TreeViewLineView {
        indent: 0,
        content,
        content_offset: 0,
        expand_content_tabs: false,
    }))
}

fn parse_node_content(
    line_view: TreeViewLineView<'_>,
    line_start: usize,
    meta: &ParseMetadata,
) -> std::result::Result<TreeViewNodeLineDetails, TreeViewNodeParseFailure> {
    let content = line_view.content;
    let content_abs = line_start + line_view.content_offset;
    let span = SourceSpan::new(content_abs, content_abs + content.len());

    let (mut raw_name, name_start, name_end, suffix_start) =
        if let Some((_, quote @ ('"' | '\''))) = content.char_indices().next() {
            let mut end = None;
            for (idx, ch) in content[quote.len_utf8()..].char_indices() {
                if ch == quote {
                    end = Some(quote.len_utf8() + idx);
                    break;
                }
            }
            let Some(end_idx) = end else {
                return Err(TreeViewNodeParseFailure {
                    error: Box::new(parse_error(meta, "unterminated quoted tree node name")),
                });
            };
            (
                content[quote.len_utf8()..end_idx].to_string(),
                quote.len_utf8(),
                end_idx,
                end_idx + quote.len_utf8(),
            )
        } else {
            let annotation_start =
                find_next_tree_view_annotation_start(content, 0).unwrap_or(content.len());
            let name_end = trim_end_byte_index(&content[..annotation_start]);
            if name_end == 0 {
                return Err(TreeViewNodeParseFailure {
                    error: Box::new(parse_error(meta, "expected tree node name")),
                });
            }
            (content[..name_end].to_string(), 0, name_end, name_end)
        };
    if line_view.expand_content_tabs {
        raw_name = raw_name.replace('\t', "    ");
    }

    let (name, node_type, selection_end) = normalize_tree_view_node_name(raw_name, name_end);
    let selection = SourceSpan::new(content_abs + name_start, content_abs + selection_end);

    let suffix = &content[suffix_start..];
    let suffix_abs = content_abs + suffix_start;
    let mut annotations = match parse_tree_view_annotations(suffix, suffix_abs, meta) {
        Ok(annotations) => annotations,
        Err(error) => {
            return Err(TreeViewNodeParseFailure {
                error: Box::new(error),
            });
        }
    };
    if line_view.expand_content_tabs
        && let Some(description) = &mut annotations.description
    {
        description.value = description.value.replace('\t', "    ");
    }

    Ok(TreeViewNodeLineDetails {
        indent: line_view.indent,
        name,
        node_type,
        css_class: annotations.css_class,
        icon: annotations.icon,
        description: annotations.description,
        span,
        selection,
    })
}

fn normalize_tree_view_node_name(
    raw_name: String,
    raw_selection_end: usize,
) -> (String, String, usize) {
    if raw_name.ends_with('/') {
        let mut name = raw_name;
        name.pop();
        (
            name,
            TREE_VIEW_DIRECTORY_NODE_TYPE.to_string(),
            raw_selection_end.saturating_sub('/'.len_utf8()),
        )
    } else {
        (
            raw_name,
            TREE_VIEW_FILE_NODE_TYPE.to_string(),
            raw_selection_end,
        )
    }
}

fn parse_tree_view_annotations(
    suffix: &str,
    abs_base: usize,
    meta: &ParseMetadata,
) -> Result<TreeViewAnnotations> {
    let mut annotations = TreeViewAnnotations::default();
    let mut pos = 0usize;
    while pos < suffix.len() {
        pos = skip_ascii_whitespace(suffix, pos);
        if pos >= suffix.len() {
            break;
        }

        if suffix[pos..].starts_with(":::") && is_annotation_token_boundary(suffix, pos) {
            let value_start = skip_ascii_whitespace(suffix, pos + 3);
            let Some(value_end) = tree_view_class_name_end(suffix, value_start) else {
                return Err(parse_error(meta, "expected tree node class after :::"));
            };
            annotations.css_class = Some(TreeViewSpannedValue {
                value: suffix[value_start..value_end].to_string(),
                span: SourceSpan::new(abs_base + value_start, abs_base + value_end),
            });
            pos = value_end;
            continue;
        }

        if suffix[pos..].starts_with("icon(") && is_annotation_token_boundary(suffix, pos) {
            let value_start = pos + "icon(".len();
            let Some(close_rel) = suffix[value_start..].find(')') else {
                return Err(parse_error(meta, "unterminated tree node icon annotation"));
            };
            let value_end = value_start + close_rel;
            let payload = &suffix[value_start..value_end];
            if !is_tree_view_icon_name(payload) {
                return Err(parse_error(meta, "invalid tree node icon annotation"));
            }
            let value = if payload.is_empty() {
                "none".to_string()
            } else {
                payload.to_string()
            };
            annotations.icon = Some(TreeViewSpannedValue {
                value,
                span: SourceSpan::new(abs_base + value_start, abs_base + value_end),
            });
            pos = value_end + ')'.len_utf8();
            continue;
        }

        if suffix[pos..].starts_with("##") && is_annotation_token_boundary(suffix, pos) {
            let value_start = skip_ascii_whitespace(suffix, pos + 2);
            let (trimmed_start, trimmed_end) = trim_ascii_span(suffix, value_start, suffix.len());
            if trimmed_start != trimmed_end {
                annotations.description = Some(TreeViewSpannedValue {
                    value: suffix[trimmed_start..trimmed_end].to_string(),
                    span: SourceSpan::new(abs_base + trimmed_start, abs_base + trimmed_end),
                });
            }
            break;
        }

        return Err(parse_error(meta, "unexpected tokens after tree node name"));
    }
    Ok(annotations)
}

fn push_tree_view_spanned_payload_fact(
    facts: &mut EditorSemanticFacts,
    value: &TreeViewSpannedValue,
    detail: &'static str,
) {
    if value.value.is_empty() || value.span.start == value.span.end {
        return;
    }
    facts.push_expected_syntax(EditorExpectedSyntax::new(
        EditorExpectedSyntaxKind::Payload,
        value.span,
    ));
    facts.push_symbol(EditorSemanticSymbol::payload(
        value.value.clone(),
        Some(detail.to_string()),
        EditorSemanticKind::String,
        value.span,
        value.span,
    ));
}

fn push_tree_view_node_editor_facts(
    facts: &mut EditorSemanticFacts,
    node: &TreeViewNodeLineDetails,
) {
    facts.push_expected_syntax(EditorExpectedSyntax::new(
        EditorExpectedSyntaxKind::NodeIdentifier,
        node.selection,
    ));
    facts.push_symbol(EditorSemanticSymbol::new(
        node.name.clone(),
        Some("tree view node".to_string()),
        EditorSemanticKind::Namespace,
        node.span,
        node.selection,
    ));
    for (value, detail) in [
        (node.css_class.as_ref(), "tree view class"),
        (node.icon.as_ref(), "tree view icon"),
        (node.description.as_ref(), "tree view description"),
    ] {
        if let Some(value) = value {
            push_tree_view_spanned_payload_fact(facts, value, detail);
        }
    }
}

fn should_skip_tree_view_box_detection_line(line: &str) -> bool {
    let trimmed = line.trim();
    trimmed.is_empty() || is_tree_view_comment_line(line) || is_tree_view_decoration_only(line)
}

fn is_tree_view_comment_line(line: &str) -> bool {
    line.trim_start().starts_with("%%")
}

fn infer_tree_view_box_segment_width(content_lines: &[String]) -> usize {
    for line in content_lines {
        if let Some((col, _)) = line
            .chars()
            .enumerate()
            .find(|(_, ch)| is_tree_view_branch_char(*ch))
            && col > 0
        {
            return col;
        }
    }
    4
}

fn find_tree_view_branch_char(line: &str) -> Option<(usize, usize, char)> {
    let mut col = 0usize;
    for (idx, ch) in line.char_indices() {
        if is_tree_view_branch_char(ch) {
            return Some((idx, col, ch));
        }
        col += if ch == '\t' { 4 } else { 1 };
    }
    None
}

fn is_tree_view_box_char(ch: char) -> bool {
    matches!(ch, '─' | '━' | '│' | '┃' | '└' | '┗' | '├' | '┣')
}

fn is_tree_view_branch_char(ch: char) -> bool {
    matches!(ch, '└' | '┗' | '├' | '┣')
}

fn is_tree_view_dash_char(ch: char) -> bool {
    matches!(ch, '─' | '━')
}

fn is_tree_view_decoration_only(line: &str) -> bool {
    !line.is_empty()
        && line
            .chars()
            .all(|ch| ch.is_whitespace() || matches!(ch, '│' | '┃'))
}

fn is_tree_view_box_drawing_only(line: &str) -> bool {
    !line.is_empty()
        && line
            .chars()
            .all(|ch| ch.is_whitespace() || is_tree_view_box_char(ch))
}

fn find_next_tree_view_annotation_start(s: &str, from: usize) -> Option<usize> {
    for (idx, _) in s.char_indices().filter(|(idx, _)| *idx >= from) {
        let valid_class = s[idx..].starts_with(":::")
            && tree_view_class_name_end(s, skip_ascii_whitespace(s, idx + 3)).is_some();
        if (valid_class || s[idx..].starts_with("##") || s[idx..].starts_with("icon("))
            && ((from == 0 && idx == 0) || is_annotation_token_boundary(s, idx))
        {
            return Some(idx);
        }
    }
    None
}

fn tree_view_class_name_end(s: &str, start: usize) -> Option<usize> {
    let mut chars = s.get(start..)?.char_indices();
    let (_, first) = chars.next()?;
    if !first.is_ascii_alphabetic() && first != '_' {
        return None;
    }
    let mut end = start + first.len_utf8();
    for (relative, ch) in chars {
        if !ch.is_ascii_alphanumeric() && ch != '_' && ch != '-' {
            break;
        }
        end = start + relative + ch.len_utf8();
    }
    Some(end)
}

fn is_tree_view_icon_name(value: &str) -> bool {
    fn is_component(value: &str, allow_empty: bool) -> bool {
        (allow_empty || !value.is_empty())
            && value
                .chars()
                .all(|ch| ch.is_ascii_alphanumeric() || ch == '_' || ch == '-')
    }

    match value.split_once(':') {
        Some((pack, icon)) => {
            !icon.contains(':') && is_component(pack, true) && is_component(icon, false)
        }
        None => is_component(value, true),
    }
}

fn is_annotation_token_boundary(s: &str, idx: usize) -> bool {
    idx > 0
        && s[..idx]
            .chars()
            .next_back()
            .is_some_and(|ch| ch == ' ' || ch == '\t')
}

fn skip_ascii_whitespace(s: &str, mut idx: usize) -> usize {
    while let Some(ch) = s[idx..].chars().next() {
        if ch == ' ' || ch == '\t' {
            idx += ch.len_utf8();
        } else {
            break;
        }
        if idx >= s.len() {
            break;
        }
    }
    idx
}

fn trim_ascii_span(s: &str, start: usize, end: usize) -> (usize, usize) {
    let mut start = start;
    let mut end = end;
    while start < end {
        let Some(ch) = s[start..end].chars().next() else {
            break;
        };
        if ch == ' ' || ch == '\t' {
            start += ch.len_utf8();
        } else {
            break;
        }
    }
    while start < end {
        let Some(ch) = s[start..end].chars().next_back() else {
            break;
        };
        if ch == ' ' || ch == '\t' {
            end -= ch.len_utf8();
        } else {
            break;
        }
    }
    (start, end)
}

fn trim_end_byte_index(s: &str) -> usize {
    let mut end = s.len();
    while end > 0 {
        let Some(ch) = s[..end].chars().next_back() else {
            break;
        };
        if ch == ' ' || ch == '\t' {
            end -= ch.len_utf8();
        } else {
            break;
        }
    }
    end
}

fn strip_inline_comment_aware(line: &str) -> &str {
    let mut in_single = false;
    let mut in_double = false;
    let mut iter = line.char_indices().peekable();
    while let Some((idx, ch)) = iter.next() {
        match ch {
            '\'' if !in_double => in_single = !in_single,
            '"' if !in_single => in_double = !in_double,
            '%' if !in_single
                && !in_double
                && iter.peek().is_some_and(|(_, next)| *next == '%') =>
            {
                return &line[..idx];
            }
            _ => {}
        }
    }
    line
}

fn parse_error(meta: &ParseMetadata, message: impl Into<String>) -> Error {
    Error::diagram_parse_fallback(meta.diagram_type.clone(), message.into())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{EditorExpectedSyntaxKind, Engine, MermaidConfig, ParseMetadata, SourceSpan};

    fn meta() -> ParseMetadata {
        ParseMetadata {
            diagram_type: "treeView".to_string(),
            config: MermaidConfig::empty_object(),
            effective_config: MermaidConfig::empty_object(),
            title: None,
        }
    }

    #[test]
    fn builds_virtual_root_and_indentation_tree() {
        let model = parse_tree_view_model_for_render(
            r#"treeView-beta
"Root"
    "Child1"
    "Child2"
        "Grandchild"
"Sibling""#,
            &meta(),
        )
        .unwrap();

        assert_eq!(model.root.name, "/");
        assert_eq!(model.root.children.len(), 2);
        assert_eq!(model.root.children[0].name, "Root");
        assert_eq!(model.root.children[0].children.len(), 2);
        assert_eq!(
            model.root.children[0].children[1].children[0].name,
            "Grandchild"
        );
        assert_eq!(model.root.children[1].name, "Sibling");
    }

    #[test]
    fn parses_title_and_accessibility_before_nodes() {
        let model = parse_tree_view_model_for_render(
            r#"treeView-beta
title My Tree
accTitle: Accessible Title
accDescr: Accessible Description
"Root""#,
            &meta(),
        )
        .unwrap();

        assert_eq!(model.title.as_deref(), Some("My Tree"));
        assert_eq!(model.acc_title.as_deref(), Some("Accessible Title"));
        assert_eq!(model.acc_descr.as_deref(), Some("Accessible Description"));
    }

    #[test]
    fn parses_mermaid_11_16_node_annotations_and_bare_names() {
        let semantic = parse_tree_view(
            r#"treeView-beta
src/ :::highlight icon(folder) ## source directory
App.tsx icon(logos:react)
index.js icon()
".gitignore"
'My Documents/' :::important
plain file.ts ## entry point
"#,
            &meta(),
        )
        .unwrap();
        let nodes = semantic["nodes"].as_array().expect("nodes array");

        assert_eq!(nodes[1]["name"], json!("src"));
        assert_eq!(nodes[1]["nodeType"], json!("directory"));
        assert_eq!(nodes[1]["cssClass"], json!("highlight"));
        assert_eq!(nodes[1]["icon"], json!("folder"));
        assert_eq!(nodes[1]["description"], json!("source directory"));
        assert_eq!(nodes[2]["name"], json!("App.tsx"));
        assert_eq!(nodes[2]["nodeType"], json!("file"));
        assert_eq!(nodes[2]["icon"], json!("logos:react"));
        assert_eq!(nodes[3]["icon"], json!("none"));
        assert_eq!(nodes[4]["name"], json!(".gitignore"));
        assert_eq!(nodes[5]["name"], json!("My Documents"));
        assert_eq!(nodes[5]["nodeType"], json!("directory"));
        assert_eq!(nodes[5]["cssClass"], json!("important"));
        assert_eq!(nodes[6]["name"], json!("plain file.ts"));
        assert_eq!(nodes[6]["description"], json!("entry point"));
    }

    #[test]
    fn sanitizes_descriptions_before_projecting_tree_view_semantics() {
        let mut meta = meta();
        meta.effective_config = MermaidConfig::from_value(json!({
            "securityLevel": "strict"
        }));
        let source = "treeView-beta\nfile.txt ## <style>.bad{display:none}</style><b>safe</b>\n";

        let semantic = parse_tree_view(source, &meta).unwrap();
        let typed = parse_tree_view_model_for_render(source, &meta).unwrap();

        assert_eq!(semantic["nodes"][1]["description"], json!("<b>safe</b>"));
        assert_eq!(
            typed.root.children[0].description.as_deref(),
            Some("<b>safe</b>")
        );
    }

    #[test]
    fn annotation_markers_inside_bare_node_names_remain_literal() {
        let semantic = parse_tree_view(
            r#"treeView-beta
foo:::bar
file##notes
"#,
            &meta(),
        )
        .unwrap();
        let nodes = semantic["nodes"].as_array().expect("nodes array");

        assert_eq!(nodes[1]["name"], json!("foo:::bar"));
        assert!(nodes[1].get("cssClass").is_none());
        assert_eq!(nodes[2]["name"], json!("file##notes"));
        assert!(nodes[2].get("description").is_none());
    }

    #[test]
    fn non_breaking_space_does_not_start_an_annotation() {
        let semantic = parse_tree_view(
            "treeView-beta\nclass\u{a0}:::literal\nicon\u{a0}icon(literal)\ndesc\u{a0}## literal\n",
            &meta(),
        )
        .unwrap();
        let nodes = semantic["nodes"].as_array().expect("nodes array");

        assert_eq!(nodes[1]["name"], json!("class\u{a0}:::literal"));
        assert_eq!(nodes[2]["name"], json!("icon\u{a0}icon(literal)"));
        assert_eq!(nodes[3]["name"], json!("desc\u{a0}## literal"));
        for node in &nodes[1..] {
            assert!(node.get("cssClass").is_none());
            assert!(node.get("icon").is_none());
            assert!(node.get("description").is_none());
        }
    }

    #[test]
    fn class_annotation_uses_the_upstream_langium_identifier_terminal() {
        for class_name in ["a", "_", "Z9_-", "_ready-2"] {
            let input = format!("treeView-beta\nfile ::: {class_name}\n");
            let semantic = parse_tree_view(&input, &meta()).expect("valid class must parse");
            assert_eq!(semantic["nodes"][1]["name"], json!("file"));
            assert_eq!(semantic["nodes"][1]["cssClass"], json!(class_name));
        }

        for rejected_class in ["9bad", "-bad", "éclair"] {
            let input = format!("treeView-beta\nfile :::{rejected_class}\n");
            let semantic = parse_tree_view(&input, &meta())
                .expect("an invalid class marker remains part of the bare name upstream");
            assert_eq!(
                semantic["nodes"][1]["name"],
                json!(format!("file :::{rejected_class}"))
            );
            assert!(semantic["nodes"][1].get("cssClass").is_none());
        }
    }

    #[test]
    fn icon_annotation_uses_the_upstream_iconify_reference_terminal() {
        for (payload, expected) in [
            ("", "none"),
            ("foo", "foo"),
            ("-", "-"),
            (":valid-name", ":valid-name"),
            ("pack:icon", "pack:icon"),
            ("_-:9-a", "_-:9-a"),
        ] {
            let input = format!("treeView-beta\nfile icon({payload})\n");
            let semantic = parse_tree_view(&input, &meta()).expect("valid icon must parse");
            assert_eq!(semantic["nodes"][1]["icon"], json!(expected));
        }

        for payload in [
            "foo bar",
            "foo:",
            "foo:bar:baz",
            "foo::bar",
            "foo/bar",
            "emoji:face🙂",
        ] {
            let input = format!("treeView-beta\nfile icon({payload})\n");
            let error = parse_tree_view(&input, &meta()).expect_err("invalid icon must fail");
            assert!(
                error
                    .to_string()
                    .contains("invalid tree node icon annotation"),
                "{payload}: {error}"
            );
        }
    }

    #[test]
    fn common_metadata_after_a_node_is_rejected_and_not_reinterpreted_as_a_node() {
        for late_metadata in [
            "title Late\n",
            "accTitle: Late\n",
            "accDescr: Late\n",
            "accDescr {\nLate\n}\n",
        ] {
            let source = format!("treeView-beta\nroot\n{late_metadata}after\n");
            let error =
                parse_tree_view(&source, &meta()).expect_err("late common metadata must fail");
            assert!(
                error.to_string().contains("must precede every node"),
                "{late_metadata:?}: {error}"
            );

            let facts = crate::family::test_support::editor_facts(
                parse_tree_view_json_and_editor_facts,
                &source,
                &meta(),
            );
            assert!(
                facts
                    .diagnostics
                    .iter()
                    .any(|diagnostic| diagnostic.message.contains("must precede every node")),
                "{late_metadata:?}: {:?}",
                facts.diagnostics
            );
            assert!(facts.symbols.iter().any(|symbol| symbol.name == "root"));
            assert!(facts.symbols.iter().any(|symbol| symbol.name == "after"));
            assert!(
                !facts
                    .symbols
                    .iter()
                    .any(|symbol| symbol.name.contains("Late"))
            );
        }
    }

    #[test]
    fn annotation_tokens_cannot_replace_a_tree_node_name() {
        for annotation in [":::highlight", "icon(file)", "## description"] {
            let input = format!("treeView-beta\n{annotation}\n");
            let err = parse_tree_view(&input, &meta()).expect_err("node name is required");

            assert!(err.to_string().contains("expected tree node name"), "{err}");
        }
    }

    #[test]
    fn parses_mermaid_11_16_box_drawing_as_indented_tree() {
        let indent = parse_tree_view(
            r#"treeView-beta
my-project/
    src/ :::highlight
        App.tsx icon(react) ## main component
        index.ts ## entry point
    package.json
    README.md ## project docs
"#,
            &meta(),
        )
        .unwrap();
        let box_draw = parse_tree_view(
            r#"treeView-beta
my-project/
├── src/ :::highlight
│   ├── App.tsx icon(react) ## main component
│   └── index.ts ## entry point
├── package.json
└── README.md ## project docs
"#,
            &meta(),
        )
        .unwrap();

        assert_eq!(box_draw["root"], indent["root"]);
    }

    #[test]
    fn box_drawing_preprocessing_expands_tabs_across_the_branch_line() {
        let semantic = parse_tree_view(
            "treeView-beta\nroot/\n├── feature\tflag ## alpha\tbeta\n",
            &meta(),
        )
        .unwrap();
        let node = &semantic["nodes"][2];

        assert_eq!(node["name"], json!("feature    flag"));
        assert_eq!(node["description"], json!("alpha    beta"));
    }

    #[test]
    fn rejects_mixed_indentation_inside_box_drawing_tree() {
        let err = parse_tree_view_model_for_render(
            r#"treeView-beta
root/
├── src/
    mixed.txt
"#,
            &meta(),
        )
        .unwrap_err();

        assert!(
            err.to_string()
                .contains("unexpected indentation without box-drawing prefix"),
            "{err}"
        );
    }

    #[test]
    fn rejects_tree_view_input_beyond_nesting_limit() {
        let mut input = String::from("treeView-beta\n");
        for depth in 0..=crate::MAX_DIAGRAM_NESTING_DEPTH {
            input.push_str(&" ".repeat(depth));
            input.push('"');
            input.push_str(&format!("n{depth}"));
            input.push_str("\"\n");
        }

        let err = parse_tree_view_model_for_render(&input, &meta()).unwrap_err();
        assert!(
            err.to_string().contains("treeView nesting depth exceeds"),
            "{err}"
        );
    }

    #[test]
    fn parse_tree_view_projects_max_allowed_chain() {
        let mut input = String::from("treeView-beta\n");
        for depth in 0..crate::MAX_DIAGRAM_NESTING_DEPTH {
            input.push_str(&" ".repeat(depth));
            input.push('"');
            input.push_str(&format!("n{depth}"));
            input.push_str("\"\n");
        }

        let semantic = parse_tree_view(&input, &meta()).unwrap();
        let nodes = semantic
            .get("nodes")
            .and_then(Value::as_array)
            .expect("nodes array");

        assert_eq!(nodes.len(), crate::MAX_DIAGRAM_NESTING_DEPTH + 1);
        assert_eq!(nodes[0].get("name").and_then(Value::as_str), Some("/"));
        assert_eq!(nodes[1].get("name").and_then(Value::as_str), Some("n0"));
        let expected_last = format!("n{}", crate::MAX_DIAGRAM_NESTING_DEPTH - 1);
        assert_eq!(
            nodes
                .last()
                .and_then(|node| node.get("name"))
                .and_then(Value::as_str),
            Some(expected_last.as_str())
        );
    }

    #[test]
    fn parse_tree_view_editor_facts_expose_parser_backed_spans() {
        let engine = Engine::new();
        let text = r#"
treeView-beta
title My Tree
accTitle: Accessible Title
accDescr: Accessible Description
"Root"
  "Child 1"
"#;
        let facts = engine
            .parse_editor_semantic_facts_with_type_sync("treeView", text)
            .unwrap()
            .unwrap();

        assert!(facts.directive_prefixes.iter().any(|p| p == "title"));
        assert!(facts.directive_prefixes.iter().any(|p| p == "accTitle"));
        assert!(facts.directive_prefixes.iter().any(|p| p == "accDescr"));
        assert!(facts.symbols.iter().any(|symbol| symbol.name == "Root"));
        assert!(facts.symbols.iter().any(|symbol| symbol.name == "Child 1"));

        let root_start = text.find("Root").unwrap();
        assert!(facts.expected_syntax.iter().any(|expected| {
            expected.kind == EditorExpectedSyntaxKind::NodeIdentifier
                && expected.span == SourceSpan::new(root_start, root_start + "Root".len())
        }));
    }

    #[test]
    fn parse_tree_view_editor_facts_preserve_box_drawing_annotation_spans() {
        let engine = Engine::new();
        let text = r#"
treeView-beta
├── App.tsx :::highlight icon(react) ## main component
"#;
        let facts = engine
            .parse_editor_semantic_facts_with_type_sync("treeView", text)
            .unwrap()
            .unwrap();

        for (payload, detail) in [
            ("App.tsx", "tree view node"),
            ("highlight", "tree view class"),
            ("react", "tree view icon"),
            ("main component", "tree view description"),
        ] {
            let start = text.find(payload).unwrap();
            assert!(
                facts.symbols.iter().any(|symbol| {
                    symbol.name == payload
                        && symbol.detail.as_deref() == Some(detail)
                        && symbol.selection == SourceSpan::new(start, start + payload.len())
                }),
                "missing {detail} payload {payload:?}"
            );
        }
    }

    #[test]
    fn tree_view_combined_parse_constructs_source_once_and_preserves_projections() {
        let text = concat!(
            "treeView-beta\n",
            "title Project Tree\n",
            "accTitle: Project files\n",
            "accDescr: Source hierarchy\n",
            "src/ :::highlight icon(folder) ## source directory\n",
            "  App.tsx icon(react)\n",
        );
        let meta = meta();

        crate::diagrams::langium_common::reset_family_syntax_construction_count("treeView");
        let (combined_json, combined_editor) = crate::family::test_support::into_result(
            parse_tree_view_json_and_editor_facts(text, &meta, &crate::OperationControl::new()),
        )
        .unwrap();
        assert_eq!(
            crate::diagrams::langium_common::family_syntax_construction_count("treeView"),
            1,
            "one combined request must construct TreeView syntax once"
        );

        let standalone_json = parse_tree_view(text, &meta).unwrap();
        assert_eq!(combined_json, standalone_json);
        assert!(!combined_editor.symbols.is_empty());
    }

    #[test]
    fn tree_view_typed_and_json_projections_share_the_same_semantics() {
        let text = concat!(
            "treeView-beta\n",
            "title Project Tree\n",
            "root/ :::root icon(folder) ## project root\n",
            "  child.ts icon(typescript)\n",
        );
        let meta = meta();

        let compat = parse_tree_view(text, &meta).unwrap();
        let typed = parse_tree_view_model_for_render(text, &meta).unwrap();

        assert_eq!(render_model_to_compat_json(&typed, &meta).unwrap(), compat);
        assert_eq!(compat["title"], json!(typed.title));
        assert_eq!(compat["accTitle"], json!(typed.acc_title));
        assert_eq!(compat["accDescr"], json!(typed.acc_descr));
        assert_eq!(compat["root"], tree_view_node_to_value(&typed.root));
    }

    #[test]
    fn tree_view_incomplete_node_recovers_prior_facts_with_exact_error_span() {
        let text = "treeView-beta\nroot/\n  \"unfinished\n";
        let meta = meta();

        let error = parse_tree_view(text, &meta).expect_err("strict parse must reject the node");
        let facts = crate::family::test_support::editor_facts(
            parse_tree_view_json_and_editor_facts,
            text,
            &meta,
        );

        assert_eq!(
            facts.completeness,
            crate::EditorSemanticCompleteness::Recovered
        );
        assert!(facts.symbols.iter().any(|symbol| symbol.name == "root"));
        let diagnostic = facts
            .diagnostics
            .iter()
            .find(|diagnostic| {
                diagnostic
                    .message
                    .contains("unterminated quoted tree node name")
            })
            .expect("recovery diagnostic");
        assert!(
            error
                .to_string()
                .contains("unterminated quoted tree node name")
        );
        assert!(
            diagnostic
                .message
                .contains("unterminated quoted tree node name")
        );
        let start = text.find("\"unfinished").unwrap();
        assert_eq!(
            diagnostic.span,
            Some(SourceSpan::new(start, start + "\"unfinished".len()))
        );
    }

    #[test]
    fn tree_view_recovery_keeps_partial_and_later_semantic_facts_with_crlf_spans() {
        let text = concat!(
            "treeView-beta\r\n",
            "root/\r\n",
            "  broken.txt icon(unclosed\r\n",
            "  later.txt :::ready icon(file)\r\n",
        );
        let meta = meta();

        let error =
            parse_tree_view(text, &meta).expect_err("strict parse must stop at first error");
        assert!(
            error
                .to_string()
                .contains("unterminated tree node icon annotation"),
            "{error}"
        );

        let facts = crate::family::test_support::editor_facts(
            parse_tree_view_json_and_editor_facts,
            text,
            &meta,
        );
        assert_eq!(
            facts.completeness,
            crate::EditorSemanticCompleteness::Recovered
        );
        assert!(facts.symbols.iter().any(|symbol| symbol.name == "root"));
        assert!(
            facts
                .symbols
                .iter()
                .any(|symbol| symbol.name == "later.txt")
        );
        assert!(
            !facts
                .symbols
                .iter()
                .any(|symbol| symbol.name == "broken.txt")
        );

        let malformed_line = "broken.txt icon(unclosed";
        let malformed_start = text.find(malformed_line).unwrap();
        assert!(facts.diagnostics.iter().any(|diagnostic| {
            diagnostic
                .message
                .contains("unterminated tree node icon annotation")
                && diagnostic.span
                    == Some(SourceSpan::new(
                        malformed_start,
                        malformed_start + malformed_line.len(),
                    ))
        }));
    }

    #[test]
    fn tree_view_strict_projections_report_the_first_recoverable_issue() {
        let text = concat!(
            "treeView-beta\n",
            "first.txt icon(unclosed\n",
            "\"second.txt\n",
            "after.txt\n",
        );
        let meta = meta();

        let errors = [
            parse_tree_view(text, &meta).unwrap_err(),
            crate::family::test_support::into_result(parse_tree_view_json_and_editor_facts(
                text,
                &meta,
                &crate::OperationControl::new(),
            ))
            .unwrap_err(),
            parse_tree_view_model_for_render(text, &meta).unwrap_err(),
        ];
        for error in errors {
            let message = error.to_string();
            assert!(
                message.contains("unterminated tree node icon annotation"),
                "{message}"
            );
            assert!(!message.contains("unterminated quoted tree node name"));
        }

        let facts = crate::family::test_support::editor_facts(
            parse_tree_view_json_and_editor_facts,
            text,
            &meta,
        );
        assert!(facts.diagnostics.iter().any(|diagnostic| {
            diagnostic
                .message
                .contains("unterminated quoted tree node name")
        }));
        assert!(
            facts
                .symbols
                .iter()
                .any(|symbol| symbol.name == "after.txt")
        );
    }

    #[test]
    fn tree_view_rejects_tokens_after_header_with_compatible_diagnostic() {
        let error = parse_tree_view("treeView-beta junk\nroot\n", &meta()).unwrap_err();
        assert!(
            error
                .to_string()
                .contains("unexpected tokens after treeView-beta")
        );
    }

    #[test]
    fn tree_view_multiline_acc_descr_uses_common_syntax_and_recovers_when_unterminated() {
        let complete = "treeView-beta\naccDescr {\nline one\nline two\n}\nroot/\n";
        let meta = meta();
        let (json, facts) = crate::family::test_support::into_result(
            parse_tree_view_json_and_editor_facts(complete, &meta, &crate::OperationControl::new()),
        )
        .unwrap();
        assert_eq!(json["accDescr"], json!("line one\nline two"));
        let payload_start = complete.find("line one").unwrap();
        let payload = facts
            .symbols
            .iter()
            .find(|symbol| symbol.name == "line one\nline two")
            .expect("multiline accessibility payload");
        assert_eq!(
            payload.selection,
            SourceSpan::new(payload_start, payload_start + "line one\nline two".len())
        );

        let incomplete = "treeView-beta\naccDescr {\nline one\n";
        assert!(parse_tree_view(incomplete, &meta).is_err());
        let recovered = crate::family::test_support::editor_facts(
            parse_tree_view_json_and_editor_facts,
            incomplete,
            &meta,
        );
        assert_eq!(
            recovered.completeness,
            crate::EditorSemanticCompleteness::Recovered
        );
        assert!(recovered.diagnostics.iter().any(|diagnostic| {
            diagnostic.message.contains("unterminated accDescr block")
                && diagnostic.span == Some(SourceSpan::new(incomplete.len(), incomplete.len()))
        }));
    }
}
