use crate::diagrams::scan::strip_line_ending;
use crate::sanitize::sanitize_text;
use crate::{
    EditorExpectedSyntax, EditorExpectedSyntaxKind, EditorSemanticFacts, EditorSemanticKind,
    EditorSemanticSymbol, Error, ParseMetadata, Result, SourceSpan,
};
use serde_json::{Map, Value, json};
#[cfg(test)]
use std::cell::Cell;

#[cfg(test)]
thread_local! {
    static ISHIKAWA_SYNTAX_CONSTRUCTION_COUNT: Cell<usize> = const { Cell::new(0) };
}

#[cfg(test)]
pub(crate) fn reset_ishikawa_syntax_construction_count() {
    ISHIKAWA_SYNTAX_CONSTRUCTION_COUNT.set(0);
}

#[cfg(test)]
pub(crate) fn ishikawa_syntax_construction_count() -> usize {
    ISHIKAWA_SYNTAX_CONSTRUCTION_COUNT.get()
}

#[derive(Debug, Clone, Default, serde::Serialize, serde::Deserialize)]
pub struct IshikawaNodeRenderModel {
    pub text: String,
    #[serde(default)]
    pub children: Vec<usize>,
}

#[derive(Debug, Clone, Default)]
pub struct IshikawaDiagramRenderModel {
    pub acc_title: Option<String>,
    pub acc_descr: Option<String>,
    pub title: Option<String>,
    pub root: Option<usize>,
    /// Flat records; root and child IDs are indices into this vector.
    pub nodes: Vec<IshikawaNodeRenderModel>,
}

impl IshikawaDiagramRenderModel {
    pub(crate) fn sanitize_common_db_fields(&mut self, config: &crate::MermaidConfig) {
        crate::common_db::sanitize_optional_acc_title(&mut self.acc_title, config);
        crate::common_db::sanitize_optional_acc_descr(&mut self.acc_descr, config);
    }
}

#[derive(Debug, Clone)]
struct FlatNode {
    raw_level: usize,
    text: String,
    span: SourceSpan,
    selection: SourceSpan,
}

struct IshikawaSemanticSource {
    nodes: Vec<FlatNode>,
    editor_facts: EditorSemanticFacts,
}

struct IshikawaParseFailure {
    error: Box<Error>,
    editor_facts: Box<EditorSemanticFacts>,
}

impl IshikawaSemanticSource {
    fn editor_facts(&self) -> EditorSemanticFacts {
        self.editor_facts.clone()
    }

    fn into_render_model_controlled(
        mut self,
        meta: &ParseMetadata,
        control: &crate::OperationControl,
    ) -> crate::OperationControlResult<IshikawaDiagramRenderModel> {
        for node in &mut self.nodes {
            control.checkpoint()?;
            node.text = sanitize_text(&node.text, &meta.effective_config);
        }
        nodes_to_render_model_controlled(self.nodes, control)
    }
}

pub(crate) fn parse_ishikawa(code: &str, meta: &ParseMetadata) -> Result<Value> {
    let model = construct_ishikawa_semantic_source(code, meta)
        .map_err(|failure| *failure.error)?
        .into_render_model_controlled(meta, &crate::OperationControl::new())
        .expect("a private parse control cannot be cancelled");
    render_model_to_compat_json(&model, meta)
}

pub(crate) fn parse_ishikawa_json_and_editor_facts(
    code: &str,
    meta: &ParseMetadata,
    control: &crate::OperationControl,
) -> crate::OperationControlResult<crate::family::CombinedSemanticParse> {
    let construction = construct_ishikawa_semantic_source_controlled(code, meta, control)?;
    let construction = match construction {
        Ok(source) => {
            let editor_facts = source.editor_facts();
            let model = source.into_render_model_controlled(meta, control)?;
            let projected = render_model_to_compat_json_controlled(&model, meta, control)?;
            Ok((projected, editor_facts))
        }
        Err(failure) => Err(failure),
    };
    let parsed = crate::family::CombinedSemanticParse::from_construction(
        construction,
        |projection| projection,
        IshikawaParseFailure::into_error_and_editor_facts,
    );
    control.checkpoint()?;
    Ok(parsed)
}

pub(crate) fn render_model_to_compat_json(
    model: &IshikawaDiagramRenderModel,
    meta: &ParseMetadata,
) -> Result<Value> {
    render_model_to_compat_json_controlled(model, meta, &crate::OperationControl::new())
        .expect("a private parse control cannot be cancelled")
}

pub(crate) fn render_model_to_compat_json_controlled(
    model: &IshikawaDiagramRenderModel,
    meta: &ParseMetadata,
    control: &crate::OperationControl,
) -> crate::OperationControlResult<Result<Value>> {
    let root = match model.project_root_controlled(control)? {
        Ok(root) => root,
        Err(message) => {
            return Ok(Err(Error::diagram_parse_fallback(
                &meta.diagram_type,
                message,
            )));
        }
    };
    let mut nodes = Vec::with_capacity(model.nodes.len());
    let mut stack = model
        .root
        .map(|root| vec![(root, 0usize)])
        .unwrap_or_default();
    while let Some((id, depth)) = stack.pop() {
        control.checkpoint()?;
        let node = &model.nodes[id];
        nodes.push(json!({"text": node.text, "depth": depth}));
        for &child in node.children.iter().rev() {
            control.checkpoint()?;
            stack.push((child, depth.saturating_add(1)));
        }
    }
    let mut out = Map::new();
    out.insert("type".to_string(), Value::String(meta.diagram_type.clone()));
    out.insert("title".to_string(), json!(&model.title));
    out.insert("accTitle".to_string(), json!(&model.acc_title));
    out.insert("accDescr".to_string(), json!(&model.acc_descr));
    out.insert("root".to_string(), root.into_unmanaged_value());
    out.insert("nodes".to_string(), Value::Array(nodes));
    let out = crate::ManagedSemanticJson::from_value(Value::Object(out));
    control.checkpoint()?;
    Ok(Ok(out.into_unmanaged_value()))
}

impl IshikawaDiagramRenderModel {
    fn project_root_controlled(
        &self,
        control: &crate::OperationControl,
    ) -> crate::OperationControlResult<std::result::Result<crate::ManagedSemanticJson, &'static str>>
    {
        let Some(root) = self.root else {
            return Ok(Ok(crate::ManagedSemanticJson::from_value(Value::Null)));
        };
        if root >= self.nodes.len() {
            return Ok(Err("ishikawa root ID is out of range"));
        }
        let mut completed = vec![None::<crate::ManagedSemanticJson>; self.nodes.len()];
        let mut seen = vec![false; self.nodes.len()];
        let mut stack = vec![(root, false)];
        while let Some((id, expanded)) = stack.pop() {
            control.checkpoint()?;
            let node = &self.nodes[id];
            if expanded {
                let mut children = Vec::with_capacity(node.children.len());
                for &child in &node.children {
                    control.checkpoint()?;
                    let Some(value) = completed[child].take() else {
                        return Ok(Err("invalid ishikawa child relationship"));
                    };
                    children.push(value);
                }
                let mut out = Map::new();
                out.insert("text".to_string(), Value::String(node.text.clone()));
                out.insert(
                    "children".to_string(),
                    Value::Array(
                        children
                            .into_iter()
                            .map(crate::ManagedSemanticJson::into_unmanaged_value)
                            .collect(),
                    ),
                );
                completed[id] = Some(crate::ManagedSemanticJson::from_value(Value::Object(out)));
            } else {
                if std::mem::replace(&mut seen[id], true) {
                    return Ok(Err("cyclic or repeated ishikawa child relationship"));
                }
                stack.push((id, true));
                for &child in node.children.iter().rev() {
                    control.checkpoint()?;
                    if child >= self.nodes.len() {
                        return Ok(Err("ishikawa child ID is out of range"));
                    }
                    stack.push((child, false));
                }
            }
        }
        control.checkpoint()?;
        Ok(Ok(completed[root]
            .take()
            .expect("root projection completed")))
    }
}

#[cfg(test)]
pub(crate) fn parse_ishikawa_model_for_render(
    code: &str,
    meta: &ParseMetadata,
) -> Result<IshikawaDiagramRenderModel> {
    Ok(construct_ishikawa_semantic_source(code, meta)
        .map_err(|failure| *failure.error)?
        .into_render_model_controlled(meta, &crate::OperationControl::new())
        .expect("a private parse control cannot be cancelled"))
}

pub(crate) fn parse_ishikawa_model_for_render_controlled(
    code: &str,
    meta: &ParseMetadata,
    control: &crate::OperationControl,
) -> crate::OperationControlResult<Result<IshikawaDiagramRenderModel>> {
    let construction = construct_ishikawa_semantic_source_controlled(code, meta, control)?;
    let source = match construction {
        Ok(source) => source,
        Err(failure) => return Ok(Err(*failure.error)),
    };
    control.checkpoint()?;
    let model = source.into_render_model_controlled(meta, control)?;
    control.checkpoint()?;
    Ok(Ok(model))
}

impl IshikawaParseFailure {
    fn into_error_and_editor_facts(self) -> (Error, EditorSemanticFacts) {
        (*self.error, *self.editor_facts)
    }
}

struct IshikawaHeader {
    root: Option<FlatNode>,
}

fn construct_ishikawa_semantic_source(
    code: &str,
    meta: &ParseMetadata,
) -> std::result::Result<IshikawaSemanticSource, IshikawaParseFailure> {
    construct_ishikawa_semantic_source_controlled(code, meta, &crate::OperationControl::new())
        .expect("a private parse control cannot be cancelled")
}

fn construct_ishikawa_semantic_source_controlled(
    code: &str,
    meta: &ParseMetadata,
    control: &crate::OperationControl,
) -> crate::OperationControlResult<std::result::Result<IshikawaSemanticSource, IshikawaParseFailure>>
{
    control.checkpoint()?;
    #[cfg(test)]
    ISHIKAWA_SYNTAX_CONSTRUCTION_COUNT.set(ISHIKAWA_SYNTAX_CONSTRUCTION_COUNT.get() + 1);

    let mut nodes = Vec::new();
    let mut offset = 0usize;
    let mut header_seen = false;
    let mut first_error = None;

    for segment in code.split_inclusive('\n') {
        control.checkpoint()?;
        let line_start = offset;
        offset += segment.len();
        let line = strip_line_ending(segment);
        if is_space_or_comment_line(line) {
            continue;
        }

        if !header_seen {
            match parse_ishikawa_header_line(line, line_start, meta) {
                Ok(header) => {
                    header_seen = true;
                    if let Some(root) = header.root {
                        nodes.push(root);
                    }
                }
                Err(error) => {
                    let span = SourceSpan::new(line_start, line_start + line.len());
                    first_error.get_or_insert((error, span));
                }
            }
            continue;
        }

        match parse_ishikawa_node_line(line, line_start, meta) {
            Ok(node) => nodes.push(node),
            Err(error) => {
                let span = SourceSpan::new(line_start, line_start + line.len());
                first_error.get_or_insert((error, span));
            }
        }
    }

    if !header_seen {
        first_error.get_or_insert((
            Error::diagram_parse_insertion_point(meta.diagram_type.clone(), "expected ishikawa", 0),
            SourceSpan::new(0, 0),
        ));
    }

    let mut editor_facts = EditorSemanticFacts::new();
    for (index, node) in nodes.iter().enumerate() {
        if index % 128 == 0 {
            control.checkpoint()?;
        }
        push_ishikawa_node_fact(&mut editor_facts, node, index == 0);
    }
    if let Some((error, span)) = first_error {
        editor_facts.mark_recovered_from_parse_error(
            format!("ishikawa parser recovered after parse error: {error}"),
            Some(span),
        );
        return Ok(Err(IshikawaParseFailure {
            error: Box::new(error),
            editor_facts: Box::new(editor_facts),
        }));
    }
    control.checkpoint()?;
    Ok(Ok(IshikawaSemanticSource {
        nodes,
        editor_facts,
    }))
}

fn parse_ishikawa_header_line(
    line: &str,
    line_start: usize,
    meta: &ParseMetadata,
) -> Result<IshikawaHeader> {
    let trimmed_start = line.len().saturating_sub(line.trim_start().len());
    let trimmed = &line[trimmed_start..];
    for header in ["ishikawa-beta", "ishikawa"] {
        if !starts_with_ignore_ascii_case(trimmed, header) {
            continue;
        }
        let rest = &trimmed[header.len()..];
        if rest
            .chars()
            .next()
            .is_some_and(|ch| ch.is_ascii_alphanumeric() || ch == '-' || ch == '_')
        {
            continue;
        }
        let text = rest.trim();
        if text.is_empty() {
            return Ok(IshikawaHeader { root: None });
        }
        let rel = rest.len().saturating_sub(rest.trim_start().len());
        let start = line_start + trimmed_start + header.len() + rel;
        let end = start + text.len();
        return Ok(IshikawaHeader {
            root: Some(FlatNode {
                raw_level: 0,
                text: text.to_string(),
                span: SourceSpan::new(line_start, line_start + line.len()),
                selection: SourceSpan::new(start, end),
            }),
        });
    }

    Err(Error::diagram_parse_exact(
        meta.diagram_type.clone(),
        "expected ishikawa",
        SourceSpan::new(line_start, line_start + line.len()),
    ))
}

fn parse_ishikawa_node_line(
    line: &str,
    line_start: usize,
    meta: &ParseMetadata,
) -> Result<FlatNode> {
    let indent = line
        .chars()
        .take_while(|ch| matches!(ch, ' ' | '\t'))
        .count();
    let body = &line[indent..];
    let text = body.trim();
    if text.is_empty() {
        return Err(Error::diagram_parse_exact(
            meta.diagram_type.clone(),
            "expected ishikawa node",
            SourceSpan::new(line_start, line_start + line.len()),
        ));
    }

    let rel = body.len().saturating_sub(body.trim_start().len());
    let start = line_start + indent + rel;
    let end = start + text.len();
    Ok(FlatNode {
        raw_level: indent,
        text: text.to_string(),
        span: SourceSpan::new(line_start, line_start + line.len()),
        selection: SourceSpan::new(start, end),
    })
}

fn push_ishikawa_node_fact(facts: &mut EditorSemanticFacts, node: &FlatNode, is_root: bool) {
    let detail = if is_root {
        "ishikawa effect"
    } else {
        "ishikawa cause"
    };
    facts.push_expected_syntax(EditorExpectedSyntax::new(
        EditorExpectedSyntaxKind::NodeIdentifier,
        node.selection,
    ));
    facts.push_symbol(EditorSemanticSymbol::new(
        node.text.clone(),
        Some(detail.to_string()),
        EditorSemanticKind::Namespace,
        node.span,
        node.selection,
    ));
}

fn nodes_to_render_model_controlled(
    nodes: Vec<FlatNode>,
    control: &crate::OperationControl,
) -> crate::OperationControlResult<IshikawaDiagramRenderModel> {
    let mut iter = nodes.into_iter();
    let Some(root) = iter.next() else {
        return Ok(IshikawaDiagramRenderModel::default());
    };
    let mut arena = vec![IshikawaNodeRenderModel {
        text: root.text,
        children: Vec::new(),
    }];
    let mut stack = vec![(0usize, 0usize)];
    let mut base_level = None;
    for flat in iter {
        control.checkpoint()?;
        let base = *base_level.get_or_insert(flat.raw_level);
        let level = flat.raw_level.saturating_sub(base) + 1;
        while stack.len() > 1
            && stack
                .last()
                .is_some_and(|(_, top_level)| *top_level >= level)
        {
            control.checkpoint()?;
            stack.pop();
        }
        let parent = stack.last().map(|(id, _)| *id).unwrap_or(0);
        let id = arena.len();
        arena.push(IshikawaNodeRenderModel {
            text: flat.text,
            children: Vec::new(),
        });
        arena[parent].children.push(id);
        stack.push((id, level));
    }
    control.checkpoint()?;
    Ok(IshikawaDiagramRenderModel {
        title: Some(arena[0].text.clone()),
        root: Some(0),
        nodes: arena,
        ..Default::default()
    })
}

fn is_space_or_comment_line(line: &str) -> bool {
    let trimmed = line.trim();
    trimmed.is_empty() || trimmed.starts_with("%%")
}

fn starts_with_ignore_ascii_case(value: &str, prefix: &str) -> bool {
    value
        .get(..prefix.len())
        .is_some_and(|actual| actual.eq_ignore_ascii_case(prefix))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        EditorExpectedSyntaxKind, EditorSemanticCompleteness, EditorSemanticKind,
        EditorSemanticRole, Engine, ManagedSemanticJson, MermaidConfig, ParseDiagnosticSpanKind,
        ParseMetadata, SourceSpan,
    };

    const DEEP_ISHIKAWA_DEPTH: usize = 1_500;

    fn meta() -> ParseMetadata {
        ParseMetadata {
            diagram_type: "ishikawa".to_string(),
            config: MermaidConfig::empty_object(),
            effective_config: MermaidConfig::empty_object(),
            title: None,
        }
    }

    fn deep_ishikawa_source(depth: usize) -> String {
        let mut source = String::from("ishikawa-beta\n  Root\n");
        for i in 0..depth {
            source.push_str(&" ".repeat((i + 2) * 2));
            source.push_str(&format!("Node {i}\n"));
        }
        source
    }

    #[cfg(feature = "all-diagrams")]
    #[test]
    fn ishikawa_deep_clone_drop_export_child() {
        crate::diagrams::treemap::tests::run_lifecycle_child(
            "diagrams::ishikawa::tests::ishikawa_deep_clone_drop_export_child",
            || {
                let source = deep_ishikawa_source(5_000);
                let parsed = Engine::new()
                    .parse_diagram_for_render_model_sync(&source, crate::ParseOptions::strict())
                    .unwrap()
                    .unwrap();
                let crate::RenderSemanticModel::Ishikawa(model) = parsed.model() else {
                    panic!("typed ishikawa");
                };
                assert_eq!(model.nodes.len(), 5_001);
                let json =
                    ManagedSemanticJson::from(render_model_to_compat_json(model, &meta()).unwrap());
                let mut output = Vec::new();
                json.write_json(&mut output).unwrap();
                crate::diagrams::treemap::tests::assert_in_progress_export_deadline(&json);
                assert!(output.windows(9).any(|bytes| bytes == b"Node 4999"));
                drop(json.clone());
                drop(json);
                drop(parsed.clone());
                drop(parsed);
                assert!(
                    Engine::new()
                        .parse_diagram_sync(
                            "ishikawa Root\n Cause\n",
                            crate::ParseOptions::strict()
                        )
                        .is_ok()
                );
            },
        );
    }

    #[cfg(feature = "all-diagrams")]
    #[test]
    fn ishikawa_projection_cancels_after_deep_completed_subtree_child() {
        crate::diagrams::treemap::tests::run_lifecycle_child(
            "diagrams::ishikawa::tests::ishikawa_projection_cancels_after_deep_completed_subtree_child",
            || {
                const DEPTH: usize = 3_000;
                let mut source = deep_ishikawa_source(DEPTH);
                source.push_str("    Sibling\n");
                let model = parse_ishikawa_model_for_render(&source, &meta()).unwrap();
                let control = crate::OperationControl::new();
                control.cancel_after_checkpoints(4 * DEPTH + 1);
                assert!(matches!(
                    model.project_root_controlled(&control),
                    Err(crate::OperationCancelled { .. })
                ));
                let projection =
                    render_model_to_compat_json(&model, &meta()).map(ManagedSemanticJson::from);
                assert!(projection.is_ok());
                let mut invalid = model.clone();
                let sibling = invalid.nodes[invalid.root.unwrap()].children[1];
                invalid.nodes[sibling].children = vec![usize::MAX];
                assert!(render_model_to_compat_json(&invalid, &meta()).is_err());
                drop(invalid);
                drop(model);
            },
        );
    }

    #[test]
    fn ishikawa_projection_handles_deep_canonical_records() {
        let boundary = parse_ishikawa_model_for_render(&deep_ishikawa_source(62), &meta()).unwrap();
        let json =
            ManagedSemanticJson::from(render_model_to_compat_json(&boundary, &meta()).unwrap());
        let mut output = Vec::new();
        json.write_json(&mut output).unwrap();
        assert!(!output.is_empty());
        let empty = IshikawaDiagramRenderModel::default();
        assert!(render_model_to_compat_json(&empty, &meta()).is_ok());
    }

    #[test]
    fn ishikawa_projection_rejects_invalid_ids_and_cycles() {
        let mut model = IshikawaDiagramRenderModel {
            root: Some(0),
            ..Default::default()
        };
        assert!(render_model_to_compat_json(&model, &meta()).is_err());
        model.nodes.push(IshikawaNodeRenderModel {
            text: "cycle".to_string(),
            children: vec![0],
        });
        assert!(render_model_to_compat_json(&model, &meta()).is_err());
    }

    #[test]
    fn controlled_parse_can_cancel_between_ishikawa_lines() {
        let control = crate::OperationControl::new();
        control.cancel_after_checkpoints(2);

        assert!(matches!(
            construct_ishikawa_semantic_source_controlled(
                "ishikawa-beta Problem\n  Cause A\n  Cause B\n",
                &meta(),
                &control,
            ),
            Err(crate::OperationCancelled { .. })
        ));
    }

    #[test]
    fn parses_basic_ishikawa_hierarchy() {
        let model = parse_ishikawa_model_for_render(
            r#"ishikawa-beta
    Blurry Photo
        Process
            Out of focus
        User
            Shaky hands
"#,
            &meta(),
        )
        .unwrap();

        let root = &model.nodes[model.root.unwrap()];
        assert_eq!(root.text, "Blurry Photo");
        assert_eq!(model.title.as_deref(), Some("Blurry Photo"));
        assert_eq!(root.children.len(), 2);
        assert_eq!(model.nodes[root.children[0]].text, "Process");
        assert_eq!(
            model.nodes[model.nodes[root.children[0]].children[0]].text,
            "Out of focus"
        );
        assert_eq!(model.nodes[root.children[1]].text, "User");
        assert_eq!(
            model.nodes[model.nodes[root.children[1]].children[0]].text,
            "Shaky hands"
        );
    }

    #[test]
    fn handles_effect_indented_more_than_causes() {
        let model = parse_ishikawa_model_for_render(
            r#"ishikawa-beta
    Problem
Cause A
  Subcause A1
Cause B
"#,
            &meta(),
        )
        .unwrap();

        let root = &model.nodes[model.root.unwrap()];
        assert_eq!(root.text, "Problem");
        assert_eq!(root.children.len(), 2);
        assert_eq!(model.nodes[root.children[0]].text, "Cause A");
        assert_eq!(
            model.nodes[model.nodes[root.children[0]].children[0]].text,
            "Subcause A1"
        );
        assert_eq!(model.nodes[root.children[1]].text, "Cause B");
    }

    #[test]
    fn detects_plain_header_and_inline_root() {
        let model = parse_ishikawa_model_for_render("ishikawa Problem\n  Cause", &meta()).unwrap();

        let root = &model.nodes[model.root.unwrap()];
        assert_eq!(root.text, "Problem");
        assert_eq!(model.nodes[root.children[0]].text, "Cause");
    }

    #[test]
    fn combined_parse_constructs_syntax_once_and_preserves_all_projections() {
        let text = "ishikawa-beta Problem\r\n  Cause A\r\n    Cause A1\r\n  Cause B\r\n";
        let expected_json = parse_ishikawa(text, &meta()).unwrap();
        let expected_model = parse_ishikawa_model_for_render(text, &meta()).unwrap();

        reset_ishikawa_syntax_construction_count();
        let (json, facts) = crate::family::test_support::into_result(
            parse_ishikawa_json_and_editor_facts(text, &meta(), &crate::OperationControl::new()),
        )
        .unwrap();

        assert_eq!(ishikawa_syntax_construction_count(), 1);
        assert_eq!(json, expected_json);
        assert_eq!(
            render_model_to_compat_json(&expected_model, &meta()).unwrap(),
            expected_json
        );
        assert_eq!(json["title"].as_str(), expected_model.title.as_deref());

        for name in ["Problem", "Cause A", "Cause A1", "Cause B"] {
            let start = text.find(name).unwrap();
            assert!(facts.symbols.iter().any(|symbol| {
                symbol.name == name
                    && symbol.selection == SourceSpan::new(start, start + name.len())
            }));
        }
    }

    #[test]
    fn editor_recovery_reports_invalid_or_incomplete_headers() {
        for (text, span, span_kind) in [
            (
                "not-ishikawa\n  Cause\n",
                SourceSpan::new(0, 12),
                ParseDiagnosticSpanKind::Exact,
            ),
            (
                "",
                SourceSpan::new(0, 0),
                ParseDiagnosticSpanKind::InsertionPoint,
            ),
        ] {
            let Error::DiagramParse { diagnostic, .. } = parse_ishikawa(text, &meta()).unwrap_err()
            else {
                panic!("expected ishikawa parse error");
            };
            assert_eq!(diagnostic.span(), Some(span));
            assert_eq!(diagnostic.span_kind(), span_kind);

            reset_ishikawa_syntax_construction_count();
            let facts = crate::family::test_support::editor_facts(
                parse_ishikawa_json_and_editor_facts,
                text,
                &meta(),
            );
            assert_eq!(ishikawa_syntax_construction_count(), 1);
            assert_eq!(facts.completeness, EditorSemanticCompleteness::Recovered);
            assert_eq!(facts.diagnostics.len(), 1);
            assert_eq!(facts.diagnostics[0].span, Some(span));
        }
    }

    #[test]
    fn parses_deep_hierarchy_without_recursive_stack_growth() {
        let source = deep_ishikawa_source(DEEP_ISHIKAWA_DEPTH);
        let model = parse_ishikawa_model_for_render(&source, &meta()).unwrap();
        let root = &model.nodes[model.root.unwrap()];

        assert_eq!(root.text, "Root");
        let mut node = root;
        for i in 0..DEEP_ISHIKAWA_DEPTH {
            node = &model.nodes[node.children[0]];
            assert_eq!(node.text, format!("Node {i}"));
        }
        assert!(node.children.is_empty());

        let semantic =
            crate::ManagedSemanticJson::from_value(parse_ishikawa(&source, &meta()).unwrap());
        assert_eq!(
            semantic["nodes"].as_array().unwrap().len(),
            DEEP_ISHIKAWA_DEPTH + 1
        );
        assert_eq!(
            semantic["nodes"][DEEP_ISHIKAWA_DEPTH]["depth"].as_u64(),
            Some(DEEP_ISHIKAWA_DEPTH as u64)
        );
        assert_eq!(
            semantic["root"]["children"][0]["children"][0]["text"].as_str(),
            Some("Node 1")
        );
    }

    #[test]
    fn parse_ishikawa_editor_facts_expose_parser_backed_spans() {
        let engine = Engine::new();
        let text = r#"ishikawa-beta
    Problem
Cause A
  Subcause A1
"#;
        let facts = engine
            .parse_editor_semantic_facts_with_type_sync("ishikawa", text)
            .unwrap()
            .expect("ishikawa editor facts");

        assert_eq!(facts.completeness, EditorSemanticCompleteness::Complete);

        for name in ["Problem", "Cause A", "Subcause A1"] {
            let start = text.find(name).unwrap();
            assert!(
                facts.expected_syntax.iter().any(|expected| {
                    expected.kind == EditorExpectedSyntaxKind::NodeIdentifier
                        && expected.span == SourceSpan::new(start, start + name.len())
                }),
                "missing expected syntax for {name}"
            );
        }

        let effect = facts
            .symbols
            .iter()
            .find(|symbol| symbol.name == "Problem")
            .expect("missing ishikawa effect");
        assert_eq!(effect.detail.as_deref(), Some("ishikawa effect"));
        assert_eq!(effect.role, EditorSemanticRole::Entity);
        assert_eq!(effect.kind, EditorSemanticKind::Namespace);
        let effect_start = text.find("Problem").unwrap();
        assert_eq!(
            effect.selection,
            SourceSpan::new(effect_start, effect_start + "Problem".len())
        );

        let cause = facts
            .symbols
            .iter()
            .find(|symbol| symbol.name == "Cause A")
            .expect("missing ishikawa cause");
        assert_eq!(cause.detail.as_deref(), Some("ishikawa cause"));
        let cause_start = text.find("Cause A").unwrap();
        assert_eq!(
            cause.selection,
            SourceSpan::new(cause_start, cause_start + "Cause A".len())
        );
    }

    #[test]
    fn parse_ishikawa_editor_facts_support_inline_root() {
        let engine = Engine::new();
        let text = "ishikawa Problem\n  Cause\n";
        let facts = engine
            .parse_editor_semantic_facts_with_type_sync("ishikawa", text)
            .unwrap()
            .expect("ishikawa editor facts");

        let effect = facts
            .symbols
            .iter()
            .find(|symbol| symbol.name == "Problem")
            .expect("missing inline root");
        assert_eq!(effect.detail.as_deref(), Some("ishikawa effect"));
        let effect_start = text.find("Problem").unwrap();
        assert_eq!(
            effect.selection,
            SourceSpan::new(effect_start, effect_start + "Problem".len())
        );
    }
}
