use crate::diagrams::scan::strip_line_ending;
use crate::{
    EditorExpectedSyntax, EditorExpectedSyntaxKind, EditorSemanticFacts, EditorSemanticKind,
    EditorSemanticSymbol, Error, OperationControl, OperationControlResult, ParseMetadata, Result,
    SourceSpan,
    family::{CombinedSemanticFailure, CombinedSemanticParse},
};
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value, json};
use std::collections::HashMap;

#[derive(Debug, Clone)]
struct ArchitectureIdentifier {
    text: String,
    span: SourceSpan,
}

#[derive(Debug, Clone)]
struct ArchitectureGroup {
    id: String,
    icon: Option<String>,
    title: Option<String>,
    in_group: Option<String>,
}

#[derive(Debug, Clone)]
struct ArchitectureEdge {
    lhs_id: String,
    lhs_span: SourceSpan,
    lhs_dir: char,
    lhs_into: Option<bool>,
    lhs_group: Option<bool>,
    rhs_id: String,
    rhs_span: SourceSpan,
    rhs_dir: char,
    rhs_into: Option<bool>,
    rhs_group: Option<bool>,
    title: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ArchitectureLayoutDirection {
    Row,
    Column,
}

#[derive(Debug, Clone)]
struct ArchitectureLayoutHint {
    direction: ArchitectureLayoutDirection,
    members: Vec<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ArchitectureNodeType {
    Service,
    Junction,
}

#[derive(Debug, Clone)]
struct ArchitectureNode {
    id: String,
    ty: ArchitectureNodeType,
    edges: Vec<usize>,
    icon: Option<String>,
    icon_text: Option<String>,
    title: Option<String>,
    in_group: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum RegisteredIdType {
    Node,
    Group,
}

impl std::fmt::Display for RegisteredIdType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            RegisteredIdType::Node => write!(f, "node"),
            RegisteredIdType::Group => write!(f, "group"),
        }
    }
}

#[derive(Debug, Default)]
struct ArchitectureDb {
    title: String,
    acc_title: String,
    acc_descr: String,

    nodes: HashMap<String, ArchitectureNode>,
    node_order: Vec<String>,
    groups: HashMap<String, ArchitectureGroup>,
    group_order: Vec<String>,
    edges: Vec<ArchitectureEdge>,
    layout_hints: Vec<ArchitectureLayoutHint>,
    registered_ids: HashMap<String, RegisteredIdType>,
}

mod parse;

impl ArchitectureDb {
    fn editor_kind_for_id(&self, id: &str) -> Option<EditorSemanticKind> {
        if self.groups.contains_key(id) {
            return Some(EditorSemanticKind::Namespace);
        }
        self.nodes.get(id).map(|node| match node.ty {
            ArchitectureNodeType::Service => EditorSemanticKind::Variable,
            ArchitectureNodeType::Junction => EditorSemanticKind::Object,
        })
    }

    fn set_title(&mut self, title: String) {
        self.title = title;
    }

    fn set_acc_title(&mut self, title: String) {
        self.acc_title = title;
    }

    fn set_acc_descr(&mut self, descr: String) {
        self.acc_descr = descr;
    }

    fn render_model(&self) -> ArchitectureDiagramRenderModel {
        let control = OperationControl::new();
        self.render_model_controlled(&control)
            .expect("a private parse control cannot be cancelled")
    }

    fn render_model_controlled(
        &self,
        control: &OperationControl,
    ) -> OperationControlResult<ArchitectureDiagramRenderModel> {
        control.checkpoint()?;
        let title = (!self.title.trim().is_empty()).then(|| self.title.clone());
        let acc_title = (!self.acc_title.trim().is_empty()).then(|| self.acc_title.clone());
        let acc_descr = (!self.acc_descr.trim().is_empty()).then(|| self.acc_descr.clone());

        let mut nodes = Vec::with_capacity(self.node_order.len());
        for (index, id) in self.node_order.iter().enumerate() {
            if index % 128 == 0 {
                control.checkpoint()?;
            }
            let Some(n) = self.nodes.get(id) else {
                continue;
            };
            nodes.push(ArchitectureRenderNode {
                id: n.id.clone(),
                node_type: match n.ty {
                    ArchitectureNodeType::Service => ArchitectureRenderNodeType::Service,
                    ArchitectureNodeType::Junction => ArchitectureRenderNodeType::Junction,
                },
                edge_indices: n.edges.clone(),
                icon: n.icon.clone(),
                icon_text: n.icon_text.clone(),
                title: n.title.clone(),
                in_group: n.in_group.clone(),
            });
        }

        let mut groups = Vec::with_capacity(self.group_order.len());
        for (index, id) in self.group_order.iter().enumerate() {
            if index % 128 == 0 {
                control.checkpoint()?;
            }
            let Some(g) = self.groups.get(id) else {
                continue;
            };
            groups.push(ArchitectureRenderGroup {
                id: g.id.clone(),
                icon: g.icon.clone(),
                title: g.title.clone(),
                in_group: g.in_group.clone(),
            });
        }

        let mut edges = Vec::with_capacity(self.edges.len());
        for (index, e) in self.edges.iter().enumerate() {
            if index % 128 == 0 {
                control.checkpoint()?;
            }
            edges.push(ArchitectureRenderEdge {
                lhs_id: e.lhs_id.clone(),
                lhs_dir: e.lhs_dir,
                lhs_into: e.lhs_into,
                lhs_group: e.lhs_group,
                rhs_id: e.rhs_id.clone(),
                rhs_dir: e.rhs_dir,
                rhs_into: e.rhs_into,
                rhs_group: e.rhs_group,
                title: e.title.clone(),
            });
        }

        let mut layout_hints = Vec::with_capacity(self.layout_hints.len());
        for (index, hint) in self.layout_hints.iter().enumerate() {
            if index % 128 == 0 {
                control.checkpoint()?;
            }
            layout_hints.push(ArchitectureRenderLayoutHint {
                direction: hint.direction,
                members: hint.members.clone(),
            });
        }

        control.checkpoint()?;
        Ok(ArchitectureDiagramRenderModel {
            title,
            acc_title,
            acc_descr,
            nodes,
            groups,
            edges,
            layout_hints,
        })
    }

    fn add_service(
        &mut self,
        id: ArchitectureIdentifier,
        icon: Option<String>,
        icon_text: Option<String>,
        title: Option<String>,
        in_group: Option<ArchitectureIdentifier>,
    ) -> Result<()> {
        let id_text = id.text;
        let id_span = id.span;
        if let Some(existing) = self.registered_ids.get(&id_text) {
            return Err(Error::diagram_parse_exact(
                "architecture",
                format!("The service id [{id_text}] is already in use by another {existing}"),
                id_span,
            ));
        }

        if let Some(parent) = &in_group {
            if id_text == parent.text {
                return Err(Error::diagram_parse_exact(
                    "architecture",
                    format!("The service [{id_text}] cannot be placed within itself"),
                    parent.span,
                ));
            }
            let Some(parent_type) = self.registered_ids.get(&parent.text).copied() else {
                return Err(Error::diagram_parse_exact(
                    "architecture",
                    format!(
                        "The service [{id_text}]'s parent does not exist. Please make sure the parent is created before this service"
                    ),
                    parent.span,
                ));
            };
            if parent_type == RegisteredIdType::Node {
                return Err(Error::diagram_parse_exact(
                    "architecture",
                    format!("The service [{id_text}]'s parent is not a group"),
                    parent.span,
                ));
            }
        }

        let in_group = in_group.map(|parent| parent.text);
        self.registered_ids
            .insert(id_text.clone(), RegisteredIdType::Node);
        if !self.nodes.contains_key(&id_text) {
            self.node_order.push(id_text.clone());
        }
        self.nodes.insert(
            id_text.clone(),
            ArchitectureNode {
                id: id_text,
                ty: ArchitectureNodeType::Service,
                edges: Vec::new(),
                icon,
                icon_text,
                title,
                in_group,
            },
        );
        Ok(())
    }

    fn add_junction(
        &mut self,
        id: ArchitectureIdentifier,
        in_group: Option<ArchitectureIdentifier>,
    ) -> Result<()> {
        let id_text = id.text;
        if let Some(existing) = self.registered_ids.get(&id_text) {
            return Err(Error::diagram_parse_exact(
                "architecture",
                format!("The junction id [{id_text}] is already in use by another {existing}"),
                id.span,
            ));
        }

        if let Some(parent) = &in_group {
            if id_text == parent.text {
                return Err(Error::diagram_parse_exact(
                    "architecture",
                    format!("The junction [{id_text}] cannot be placed within itself"),
                    parent.span,
                ));
            }
            let Some(parent_type) = self.registered_ids.get(&parent.text).copied() else {
                return Err(Error::diagram_parse_exact(
                    "architecture",
                    format!(
                        "The junction [{id_text}]'s parent does not exist. Please make sure the parent is created before this junction"
                    ),
                    parent.span,
                ));
            };
            if parent_type == RegisteredIdType::Node {
                return Err(Error::diagram_parse_exact(
                    "architecture",
                    format!("The junction [{id_text}]'s parent is not a group"),
                    parent.span,
                ));
            }
        }

        let in_group = in_group.map(|parent| parent.text);
        self.registered_ids
            .insert(id_text.clone(), RegisteredIdType::Node);
        if !self.nodes.contains_key(&id_text) {
            self.node_order.push(id_text.clone());
        }
        self.nodes.insert(
            id_text.clone(),
            ArchitectureNode {
                id: id_text,
                ty: ArchitectureNodeType::Junction,
                edges: Vec::new(),
                icon: None,
                icon_text: None,
                title: None,
                in_group,
            },
        );
        Ok(())
    }

    fn add_group(
        &mut self,
        id: ArchitectureIdentifier,
        icon: Option<String>,
        title: Option<String>,
        in_group: Option<ArchitectureIdentifier>,
    ) -> Result<()> {
        let id_text = id.text;
        let id_span = id.span;
        if let Some(existing) = self.registered_ids.get(&id_text) {
            return Err(Error::diagram_parse_exact(
                "architecture",
                format!("The group id [{id_text}] is already in use by another {existing}"),
                id_span,
            ));
        }

        if let Some(parent) = &in_group {
            if id_text == parent.text {
                return Err(Error::diagram_parse_exact(
                    "architecture",
                    format!("The group [{id_text}] cannot be placed within itself"),
                    parent.span,
                ));
            }
            let Some(parent_type) = self.registered_ids.get(&parent.text).copied() else {
                return Err(Error::diagram_parse_exact(
                    "architecture",
                    format!(
                        "The group [{id_text}]'s parent does not exist. Please make sure the parent is created before this group"
                    ),
                    parent.span,
                ));
            };
            if parent_type == RegisteredIdType::Node {
                return Err(Error::diagram_parse_exact(
                    "architecture",
                    format!("The group [{id_text}]'s parent is not a group"),
                    parent.span,
                ));
            }
        }

        let in_group = in_group.map(|parent| parent.text);
        self.registered_ids
            .insert(id_text.clone(), RegisteredIdType::Group);
        if !self.groups.contains_key(&id_text) {
            self.group_order.push(id_text.clone());
        }
        self.groups.insert(
            id_text.clone(),
            ArchitectureGroup {
                id: id_text,
                icon,
                title,
                in_group,
            },
        );
        Ok(())
    }

    fn add_edge(&mut self, edge: ArchitectureEdge) -> Result<()> {
        if !is_dir(edge.lhs_dir) {
            return Err(Error::diagram_parse_fallback(
                "architecture".to_string(),
                format!(
                    "Invalid direction given for left hand side of edge {}--{}. Expected (L,R,T,B) got {}",
                    edge.lhs_id, edge.rhs_id, edge.lhs_dir
                ),
            ));
        }
        if !is_dir(edge.rhs_dir) {
            return Err(Error::diagram_parse_fallback(
                "architecture".to_string(),
                format!(
                    "Invalid direction given for right hand side of edge {}--{}. Expected (L,R,T,B) got {}",
                    edge.lhs_id, edge.rhs_id, edge.rhs_dir
                ),
            ));
        }

        if !self.nodes.contains_key(&edge.lhs_id) && !self.groups.contains_key(&edge.lhs_id) {
            return Err(Error::diagram_parse_exact(
                "architecture",
                format!(
                    "The left-hand id [{}] does not yet exist. Please create the service/group before declaring an edge to it.",
                    edge.lhs_id
                ),
                edge.lhs_span,
            ));
        }
        if !self.nodes.contains_key(&edge.rhs_id) && !self.groups.contains_key(&edge.rhs_id) {
            return Err(Error::diagram_parse_exact(
                "architecture",
                format!(
                    "The right-hand id [{}] does not yet exist. Please create the service/group before declaring an edge to it.",
                    edge.rhs_id
                ),
                edge.rhs_span,
            ));
        }
        if self.groups.contains_key(&edge.lhs_id) {
            return Err(Error::diagram_parse_exact(
                "architecture",
                format!(
                    "The left-hand id [{}] is a group; architecture edges require a service or junction endpoint.",
                    edge.lhs_id
                ),
                edge.lhs_span,
            ));
        }
        if self.groups.contains_key(&edge.rhs_id) {
            return Err(Error::diagram_parse_exact(
                "architecture",
                format!(
                    "The right-hand id [{}] is a group; architecture edges require a service or junction endpoint.",
                    edge.rhs_id
                ),
                edge.rhs_span,
            ));
        }

        if edge.lhs_group == Some(true)
            && let (Some(lhs), Some(rhs)) =
                (self.nodes.get(&edge.lhs_id), self.nodes.get(&edge.rhs_id))
            && let (Some(lhs_parent), Some(rhs_parent)) = (&lhs.in_group, &rhs.in_group)
            && lhs_parent == rhs_parent
        {
            return Err(Error::diagram_parse_exact(
                "architecture",
                format!(
                    "The left-hand id [{}] is modified to traverse the group boundary, but the edge does not pass through two groups.",
                    edge.lhs_id
                ),
                edge.lhs_span,
            ));
        }
        if edge.rhs_group == Some(true)
            && let (Some(lhs), Some(rhs)) =
                (self.nodes.get(&edge.lhs_id), self.nodes.get(&edge.rhs_id))
            && let (Some(lhs_parent), Some(rhs_parent)) = (&lhs.in_group, &rhs.in_group)
            && lhs_parent == rhs_parent
        {
            return Err(Error::diagram_parse_exact(
                "architecture",
                format!(
                    "The right-hand id [{}] is modified to traverse the group boundary, but the edge does not pass through two groups.",
                    edge.rhs_id
                ),
                edge.rhs_span,
            ));
        }

        let edge_idx = self.edges.len();
        self.edges.push(edge);
        let lhs_id = self.edges[edge_idx].lhs_id.clone();
        let rhs_id = self.edges[edge_idx].rhs_id.clone();
        if self.nodes.contains_key(&lhs_id) && self.nodes.contains_key(&rhs_id) {
            if let Some(lhs) = self.nodes.get_mut(&lhs_id) {
                lhs.edges.push(edge_idx);
            }
            if let Some(rhs) = self.nodes.get_mut(&rhs_id) {
                rhs.edges.push(edge_idx);
            }
        }
        Ok(())
    }

    fn add_layout_hint_controlled(
        &mut self,
        direction: ArchitectureLayoutDirection,
        members: Vec<ArchitectureIdentifier>,
        control: &OperationControl,
    ) -> OperationControlResult<Result<()>> {
        control.checkpoint()?;
        if members.len() < 2 {
            return Ok(Err(Error::diagram_parse_fallback(
                "architecture".to_string(),
                format!(
                    "An align directive requires at least two members; got {}",
                    members.len()
                ),
            )));
        }

        let mut seen = std::collections::HashSet::new();
        let mut member_texts = Vec::with_capacity(members.len());
        for (index, member) in members.into_iter().enumerate() {
            if index % 128 == 0 {
                control.checkpoint()?;
            }
            if self.registered_ids.get(&member.text).copied() != Some(RegisteredIdType::Node) {
                return Ok(Err(Error::diagram_parse_exact(
                    "architecture",
                    format!(
                        "align {} references [{}], which is not a service or junction",
                        direction.as_str(),
                        member.text
                    ),
                    member.span,
                )));
            }
            if !seen.insert(member.text.clone()) {
                return Ok(Err(Error::diagram_parse_exact(
                    "architecture",
                    format!(
                        "align {} lists [{}] more than once",
                        direction.as_str(),
                        member.text
                    ),
                    member.span,
                )));
            }
            member_texts.push(member.text);
        }

        self.layout_hints.push(ArchitectureLayoutHint {
            direction,
            members: member_texts,
        });
        control.checkpoint()?;
        Ok(Ok(()))
    }
}

fn is_dir(c: char) -> bool {
    matches!(c, 'L' | 'R' | 'T' | 'B')
}

impl ArchitectureLayoutDirection {
    fn as_str(self) -> &'static str {
        match self {
            ArchitectureLayoutDirection::Row => "row",
            ArchitectureLayoutDirection::Column => "column",
        }
    }

    fn parse(value: &str) -> Option<Self> {
        match value {
            "row" => Some(Self::Row),
            "column" => Some(Self::Column),
            _ => None,
        }
    }
}

fn strip_inline_comment(line: &str) -> &str {
    let mut in_quote = false;
    let mut quote_char: Option<char> = None;
    let mut in_title = false;
    let mut it = line.char_indices().peekable();
    while let Some((idx, ch)) = it.next() {
        if in_quote {
            if ch == '\\' {
                it.next();
                continue;
            }
            if Some(ch) == quote_char {
                in_quote = false;
                quote_char = None;
            }
            continue;
        }
        if ch == '"' || ch == '\'' {
            in_quote = true;
            quote_char = Some(ch);
            continue;
        }
        if ch == '[' {
            in_title = true;
            continue;
        }
        if ch == ']' {
            in_title = false;
            continue;
        }
        if !in_title && ch == '%' && it.peek().is_some_and(|(_, next)| *next == '%') {
            return &line[..idx];
        }
    }
    line
}

struct ArchitectureSourceLine<'a> {
    text: &'a str,
    start: usize,
}

#[derive(Debug)]
struct ArchitectureLineCursor<'a> {
    source: &'a str,
    offset: usize,
}

impl<'a> ArchitectureLineCursor<'a> {
    fn new(source: &'a str) -> Self {
        Self { source, offset: 0 }
    }

    fn next(&mut self) -> Option<ArchitectureSourceLine<'a>> {
        if self.offset >= self.source.len() {
            return None;
        }

        let start = self.offset;
        let rest = &self.source[start..];
        let end = if let Some(newline) = rest.find('\n') {
            start + newline + 1
        } else {
            self.source.len()
        };
        self.offset = end;

        Some(ArchitectureSourceLine {
            text: strip_line_ending(&self.source[start..end]),
            start,
        })
    }
}

fn trimmed_statement_with_offset(raw: &str, raw_start: usize) -> (&str, usize) {
    let line = strip_inline_comment(raw);
    let leading = line.len() - line.trim_start().len();
    (line.trim(), raw_start + leading)
}

fn is_architecture_reserved_id(id: &str) -> bool {
    matches!(
        id,
        "architecture-beta" | "group" | "service" | "junction" | "in" | "align" | "row" | "column"
    ) || id.starts_with("title")
        || id
            .as_bytes()
            .first()
            .is_some_and(|first| matches!(first, b'L' | b'R' | b'T' | b'B'))
}

pub(crate) fn is_valid_editor_identifier(candidate: &str) -> bool {
    let mut last_was_word = false;
    for (index, ch) in candidate.chars().enumerate() {
        let is_word = ch.is_ascii_alphanumeric() || ch == '_';
        if index == 0 && !is_word {
            return false;
        }
        if !is_word && ch != '-' {
            return false;
        }
        last_was_word = is_word;
    }
    !candidate.is_empty() && last_was_word && !is_architecture_reserved_id(candidate)
}

fn architecture_reserved_id_message(id: &str) -> String {
    format!("reserved architecture keyword [{id}] cannot be used as an id")
}

pub(crate) fn parse_architecture(code: &str, meta: &ParseMetadata) -> Result<Value> {
    Ok(parse::parse_semantic_source(code, meta)?.compat_json(meta))
}

pub(crate) fn parse_architecture_json_and_editor_facts(
    code: &str,
    meta: &ParseMetadata,
    control: &crate::OperationControl,
) -> crate::OperationControlResult<CombinedSemanticParse> {
    control.checkpoint()?;
    let construction = match parse::parse_combined_semantic_source_controlled(code, meta, control)?
    {
        Ok(source) => Ok(source.into_combined_parts_controlled(meta, control)?),
        Err(error) => Err(error),
    };
    let parsed = CombinedSemanticParse::from_construction(
        construction,
        |parts| parts,
        CombinedSemanticFailure::into_parts,
    );
    control.checkpoint()?;
    Ok(parsed)
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ArchitectureDiagramRenderModel {
    #[serde(default)]
    pub title: Option<String>,
    #[serde(default, rename = "accTitle")]
    pub acc_title: Option<String>,
    #[serde(default, rename = "accDescr")]
    pub acc_descr: Option<String>,
    #[serde(default)]
    pub nodes: Vec<ArchitectureRenderNode>,
    #[serde(default)]
    pub groups: Vec<ArchitectureRenderGroup>,
    #[serde(default)]
    pub edges: Vec<ArchitectureRenderEdge>,
    #[serde(default, rename = "layoutHints")]
    pub layout_hints: Vec<ArchitectureRenderLayoutHint>,
}

impl ArchitectureDiagramRenderModel {
    pub(crate) fn sanitize_common_db_fields(&mut self, config: &crate::MermaidConfig) {
        crate::common_db::sanitize_optional_title(&mut self.title, config);
        crate::common_db::sanitize_optional_acc_title(&mut self.acc_title, config);
        crate::common_db::sanitize_optional_acc_descr(&mut self.acc_descr, config);
    }
}

pub(crate) fn render_model_to_compat_json(
    model: &ArchitectureDiagramRenderModel,
    meta: &ParseMetadata,
) -> Result<Value> {
    let control = OperationControl::new();
    render_model_to_compat_json_controlled(model, meta, &control)
        .expect("a private parse control cannot be cancelled")
}

pub(crate) fn render_model_to_compat_json_controlled(
    model: &ArchitectureDiagramRenderModel,
    meta: &ParseMetadata,
    control: &OperationControl,
) -> OperationControlResult<Result<Value>> {
    control.checkpoint()?;
    let mut config = crate::config::clone_value_nonrecursive(meta.effective_config.as_value());
    if meta.config.as_value().get("layout").is_none()
        && let Some(obj) = config.as_object_mut()
    {
        obj.insert("layout".to_string(), Value::String("dagre".to_string()));
    }
    control.checkpoint()?;

    let mut edges = Vec::with_capacity(model.edges.len());
    for (index, edge) in model.edges.iter().enumerate() {
        if index % 128 == 0 {
            control.checkpoint()?;
        }
        edges.push(architecture_render_edge_to_compat_json(edge));
    }
    let mut nodes = Vec::with_capacity(model.nodes.len());
    let mut services = Vec::new();
    let mut junctions = Vec::new();
    for (index, node) in model.nodes.iter().enumerate() {
        if index % 128 == 0 {
            control.checkpoint()?;
        }
        let value = architecture_render_node_to_compat_json(node, &edges, control)?;
        match node.node_type {
            ArchitectureRenderNodeType::Service => services.push(value.clone()),
            ArchitectureRenderNodeType::Junction => junctions.push(value.clone()),
        }
        nodes.push(value);
    }
    let mut groups = Vec::with_capacity(model.groups.len());
    for (index, group) in model.groups.iter().enumerate() {
        if index % 128 == 0 {
            control.checkpoint()?;
        }
        groups.push(json!({
            "id": group.id,
            "icon": group.icon,
            "title": group.title,
            "in": group.in_group,
        }));
    }
    let mut layout_hints = Vec::with_capacity(model.layout_hints.len());
    for (hint_index, hint) in model.layout_hints.iter().enumerate() {
        if hint_index % 128 == 0 {
            control.checkpoint()?;
        }
        let mut members = Vec::with_capacity(hint.members.len());
        for (member_index, member) in hint.members.iter().enumerate() {
            if member_index % 128 == 0 {
                control.checkpoint()?;
            }
            members.push(Value::String(member.clone()));
        }
        layout_hints.push(json!({
            "direction": hint.direction.as_str(),
            "members": members,
        }));
    }

    let mut out = Map::with_capacity(11);
    out.insert("type".to_string(), Value::String(meta.diagram_type.clone()));
    out.insert("title".to_string(), json!(&model.title));
    out.insert("accTitle".to_string(), json!(&model.acc_title));
    out.insert("accDescr".to_string(), json!(&model.acc_descr));
    out.insert("groups".to_string(), Value::Array(groups));
    out.insert("nodes".to_string(), Value::Array(nodes));
    out.insert("services".to_string(), Value::Array(services));
    out.insert("junctions".to_string(), Value::Array(junctions));
    out.insert("edges".to_string(), Value::Array(edges));
    out.insert("layoutHints".to_string(), Value::Array(layout_hints));
    out.insert("config".to_string(), config);
    control.checkpoint()?;
    Ok(Ok(Value::Object(out)))
}

fn architecture_render_edge_to_compat_json(edge: &ArchitectureRenderEdge) -> Value {
    json!({
        "lhsId": edge.lhs_id,
        "lhsDir": edge.lhs_dir.to_string(),
        "lhsInto": edge.lhs_into,
        "lhsGroup": edge.lhs_group,
        "rhsId": edge.rhs_id,
        "rhsDir": edge.rhs_dir.to_string(),
        "rhsInto": edge.rhs_into,
        "rhsGroup": edge.rhs_group,
        "title": edge.title,
    })
}

fn architecture_render_node_to_compat_json(
    node: &ArchitectureRenderNode,
    edges: &[Value],
    control: &OperationControl,
) -> OperationControlResult<Value> {
    let mut node_edges = Vec::with_capacity(node.edge_indices.len());
    for (index, edge_index) in node.edge_indices.iter().enumerate() {
        if index % 128 == 0 {
            control.checkpoint()?;
        }
        if let Some(edge) = edges.get(*edge_index) {
            node_edges.push(edge.clone());
        }
    }
    Ok(json!({
        "id": node.id,
        "type": node.node_type,
        "edges": node_edges,
        "icon": node.icon,
        "iconText": node.icon_text,
        "title": node.title,
        "in": node.in_group,
    }))
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ArchitectureRenderNodeType {
    #[serde(rename = "service")]
    Service,
    #[serde(rename = "junction")]
    Junction,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ArchitectureRenderNode {
    pub id: String,
    #[serde(rename = "type")]
    pub node_type: ArchitectureRenderNodeType,
    #[serde(default)]
    pub edge_indices: Vec<usize>,
    #[serde(default)]
    pub icon: Option<String>,
    #[serde(default, rename = "iconText")]
    pub icon_text: Option<String>,
    #[serde(default)]
    pub title: Option<String>,
    #[serde(default, rename = "in")]
    pub in_group: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ArchitectureRenderGroup {
    pub id: String,
    #[serde(default)]
    pub icon: Option<String>,
    #[serde(default)]
    pub title: Option<String>,
    #[serde(default, rename = "in")]
    pub in_group: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ArchitectureRenderEdge {
    #[serde(rename = "lhsId")]
    pub lhs_id: String,
    #[serde(rename = "lhsDir")]
    pub lhs_dir: char,
    #[serde(default, rename = "lhsInto")]
    pub lhs_into: Option<bool>,
    #[serde(default, rename = "lhsGroup")]
    pub lhs_group: Option<bool>,
    #[serde(rename = "rhsId")]
    pub rhs_id: String,
    #[serde(rename = "rhsDir")]
    pub rhs_dir: char,
    #[serde(default, rename = "rhsInto")]
    pub rhs_into: Option<bool>,
    #[serde(default, rename = "rhsGroup")]
    pub rhs_group: Option<bool>,
    #[serde(default)]
    pub title: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ArchitectureRenderLayoutHint {
    pub direction: ArchitectureLayoutDirection,
    #[serde(default)]
    pub members: Vec<String>,
}

#[cfg(test)]
pub(crate) fn parse_architecture_model_for_render(
    code: &str,
    meta: &ParseMetadata,
) -> Result<ArchitectureDiagramRenderModel> {
    Ok(parse::parse_semantic_source(code, meta)?.render_model())
}

pub(crate) fn parse_architecture_model_for_render_controlled(
    code: &str,
    meta: &ParseMetadata,
    control: &OperationControl,
) -> OperationControlResult<Result<ArchitectureDiagramRenderModel>> {
    parse::parse_model_for_render_controlled(code, meta, control)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        EditorSemanticCompleteness, Engine, MermaidConfig, ParseDiagnosticSpanKind, ParseOptions,
        ParsedEditorFacts,
    };
    use futures::executor::block_on;

    fn parse(text: &str) -> crate::ManagedSemanticJson {
        let engine = Engine::new();
        block_on(engine.parse_diagram(text, ParseOptions::default()))
            .unwrap()
            .unwrap()
            .model
    }

    fn parse_err(text: &str) -> crate::ParseDiagnostic {
        let engine = Engine::new();
        match block_on(engine.parse_diagram(text, ParseOptions::default())).unwrap_err() {
            Error::DiagramParse { diagnostic, .. } => diagnostic,
            other => panic!("expected architecture parse error, got {other:?}"),
        }
    }

    fn test_meta() -> ParseMetadata {
        ParseMetadata {
            diagram_type: "architecture".to_string(),
            config: MermaidConfig::default(),
            effective_config: MermaidConfig::default(),
            title: None,
        }
    }

    fn payload_selection(facts: &EditorSemanticFacts, detail: &str, name: &str) -> SourceSpan {
        facts
            .symbols
            .iter()
            .find(|symbol| symbol.detail.as_deref() == Some(detail) && symbol.name == name)
            .unwrap_or_else(|| panic!("missing payload symbol {detail:?} {name:?}"))
            .selection
    }

    #[test]
    fn architecture_accepts_header_only() {
        let _ = parse("architecture-beta");
    }

    #[test]
    fn architecture_accepts_simple_service() {
        let model = parse("architecture-beta\n  service db\n");
        assert_eq!(model["services"].as_array().unwrap().len(), 1);
        assert_eq!(model["services"][0]["id"].as_str().unwrap(), "db");
    }

    #[test]
    fn architecture_canonical_typed_entrypoint_accepts_simple_service() {
        let parsed = Engine::new()
            .parse_diagram_for_render_model_sync(
                "architecture-beta\nservice db\n",
                ParseOptions::strict(),
            )
            .expect("typed architecture parse should succeed")
            .expect("architecture should be detected");

        assert_eq!(parsed.model().kind(), "architecture");
    }

    #[test]
    fn architecture_rejects_reserved_keywords_as_entity_ids_with_exact_spans() {
        for (entity, suffix) in [
            ("service", "(server)[X]"),
            ("group", "(cloud)[X]"),
            ("junction", ""),
        ] {
            for reserved in [
                "align",
                "row",
                "column",
                "architecture-beta",
                "group",
                "service",
                "junction",
                "in",
                "title",
                "titlex",
                "Left",
                "Right",
                "Top",
                "Bottom",
            ] {
                let text = format!("architecture-beta\n  {entity} {reserved}{suffix}\n");
                let diagnostic = parse_err(&text);
                let offset = text.rfind(reserved).unwrap();

                assert_eq!(
                    diagnostic.message(),
                    format!("reserved architecture keyword [{reserved}] cannot be used as an id")
                );
                assert_eq!(
                    diagnostic.span(),
                    Some(SourceSpan::new(offset, offset + reserved.len()))
                );
                assert_eq!(diagnostic.span_kind(), ParseDiagnosticSpanKind::Exact);
            }
        }
    }

    #[test]
    fn architecture_rejects_reserved_keywords_in_id_reference_positions() {
        for (text, reserved) in [
            (
                "architecture-beta\n  group root\n  service child in row\n",
                "row",
            ),
            (
                "architecture-beta\n  service source\n  source:L -- R:column\n",
                "column",
            ),
            (
                "architecture-beta\n  service source\n  align row source align\n",
                "align",
            ),
            (
                "architecture-beta\n  group root\n  service child in service\n",
                "service",
            ),
            (
                "architecture-beta\n  service source\n  source:L -- R:titlex\n",
                "titlex",
            ),
            (
                "architecture-beta\n  service source\n  align row source Left\n",
                "Left",
            ),
        ] {
            let diagnostic = parse_err(text);
            let offset = text.rfind(reserved).unwrap();

            assert_eq!(
                diagnostic.message(),
                format!("reserved architecture keyword [{reserved}] cannot be used as an id")
            );
            assert_eq!(
                diagnostic.span(),
                Some(SourceSpan::new(offset, offset + reserved.len()))
            );
            assert_eq!(diagnostic.span_kind(), ParseDiagnosticSpanKind::Exact);
        }
    }

    #[test]
    fn architecture_accepts_ids_that_only_start_with_reserved_keywords() {
        let model = parse(
            "architecture-beta\n  service rowspan(server)[Rowspan]\n  group columnar(cloud)[Columnar]\n  junction alignment\n  service architecture-betax\n  service grouped\n  service serviceWorker\n  service junctionBox\n  service inside\n  service left\n  service right\n  service top\n  service bottom\n",
        );

        let service_ids = model["services"]
            .as_array()
            .unwrap()
            .iter()
            .map(|service| service["id"].as_str().unwrap())
            .collect::<Vec<_>>();
        assert_eq!(
            service_ids,
            [
                "rowspan",
                "architecture-betax",
                "grouped",
                "serviceWorker",
                "junctionBox",
                "inside",
                "left",
                "right",
                "top",
                "bottom",
            ]
        );
        assert_eq!(model["groups"][0]["id"], "columnar");
        assert_eq!(model["junctions"][0]["id"], "alignment");
    }

    #[test]
    fn architecture_editor_facts_report_reserved_entity_ids() {
        for (entity, suffix) in [
            ("service", "(server)[X]"),
            ("group", "(cloud)[X]"),
            ("junction", ""),
        ] {
            for reserved in [
                "align",
                "row",
                "column",
                "architecture-beta",
                "group",
                "service",
                "junction",
                "in",
                "title",
                "titlex",
                "Left",
                "Right",
                "Top",
                "Bottom",
            ] {
                let text = format!("architecture-beta\n  {entity} {reserved}{suffix}\n");
                let offset = text.rfind(reserved).unwrap();
                let facts = crate::family::test_support::editor_facts(
                    parse_architecture_json_and_editor_facts,
                    &text,
                    &test_meta(),
                );

                assert_eq!(facts.completeness, EditorSemanticCompleteness::Recovered);
                assert_eq!(facts.diagnostics.len(), 1);
                assert_eq!(
                    facts.diagnostics[0].message,
                    format!("reserved architecture keyword [{reserved}] cannot be used as an id")
                );
                assert_eq!(
                    facts.diagnostics[0].span,
                    Some(SourceSpan::new(offset, offset + reserved.len()))
                );
                assert!(!facts.symbols.iter().any(|symbol| symbol.name == reserved));
            }
        }
    }

    #[test]
    fn architecture_editor_facts_report_reserved_id_references() {
        for (text, reserved) in [
            (
                "architecture-beta\n  group root\n  service child in row\n",
                "row",
            ),
            (
                "architecture-beta\n  service source\n  source:L -- R:column\n",
                "column",
            ),
            (
                "architecture-beta\n  service source\n  align row source align\n",
                "align",
            ),
        ] {
            let offset = text.rfind(reserved).unwrap();
            let facts = crate::family::test_support::editor_facts(
                parse_architecture_json_and_editor_facts,
                text,
                &test_meta(),
            );

            assert_eq!(facts.completeness, EditorSemanticCompleteness::Recovered);
            assert_eq!(facts.diagnostics.len(), 1);
            assert_eq!(
                facts.diagnostics[0].message,
                format!("reserved architecture keyword [{reserved}] cannot be used as an id")
            );
            assert_eq!(
                facts.diagnostics[0].span,
                Some(SourceSpan::new(offset, offset + reserved.len()))
            );
        }
    }

    #[test]
    fn architecture_entity_facts_use_the_architecture_rename_policy() {
        let facts = crate::family::test_support::editor_facts(
            parse_architecture_json_and_editor_facts,
            "architecture-beta\nservice api\n",
            &test_meta(),
        );
        let api = facts
            .symbols
            .iter()
            .find(|symbol| {
                symbol.name == "api" && symbol.detail.as_deref() == Some("architecture service")
            })
            .expect("architecture service fact");

        assert_eq!(
            api.rename_policy,
            crate::EditorRenamePolicy::ArchitectureIdentifier
        );
    }

    #[test]
    fn architecture_title_on_first_line() {
        let model = parse("architecture-beta title Simple Architecture Diagram");
        assert_eq!(
            model["title"].as_str().unwrap(),
            "Simple Architecture Diagram"
        );
    }

    #[test]
    fn architecture_projection_preserves_declaration_order() {
        let model = parse(
            "architecture-beta\n\
group 110\n\
group 102\n\
group 001\n\
service 10\n\
service 2\n\
service 01\n\
service 1\n\
service named\n",
        );
        let ids = |field: &str| {
            model[field]
                .as_array()
                .unwrap()
                .iter()
                .map(|entry| entry["id"].as_str().unwrap())
                .collect::<Vec<_>>()
        };

        assert_eq!(ids("groups"), ["110", "102", "001"]);
        assert_eq!(ids("nodes"), ["10", "2", "01", "1", "named"]);
    }

    #[test]
    fn architecture_title_on_another_line() {
        let model = parse("architecture-beta\n  title Simple Architecture Diagram\n");
        assert_eq!(
            model["title"].as_str().unwrap(),
            "Simple Architecture Diagram"
        );
    }

    #[test]
    fn architecture_title_without_whitespace_is_shadowed_by_langium_title_terminal() {
        let text = "architecture-beta\ntitle: Not a title\n";
        let diagnostic = parse_err(text);
        let title = text.find("\ntitle").unwrap() + 1;

        assert_eq!(
            diagnostic.message(),
            "reserved architecture keyword [title] cannot be used as an id"
        );
        assert_eq!(
            diagnostic.span(),
            Some(SourceSpan::new(title, title + "title".len()))
        );
        assert_eq!(diagnostic.span_kind(), ParseDiagnosticSpanKind::Exact);
    }

    #[test]
    fn architecture_quoted_title_unescapes_both_quote_kinds() {
        let model = parse(
            r#"architecture-beta
service api(server)["Double \"quote\" and \'single\'"]
"#,
        );

        assert_eq!(
            model["services"][0]["title"],
            "Double \"quote\" and 'single'"
        );
    }

    #[test]
    fn architecture_string_uses_langium_default_unescape_semantics_in_every_projection() {
        let text = r#"architecture-beta
service api "\b\f\n\r\t\v\0\"quote\"\\tail"
"#;
        let expected = "bfnrtv0\"quote\"\\tail";
        let meta = test_meta();
        let source = parse::parse_semantic_source(text, &meta).unwrap();

        let json = source.compat_json(&meta);
        let render = source.render_model();
        let facts = source.editor_facts();

        assert_eq!(
            render_model_to_compat_json(&render, &meta).unwrap(),
            json,
            "Architecture typed compatibility projection drifted"
        );

        assert_eq!(json["services"][0]["iconText"], expected);
        assert_eq!(render.nodes[0].icon_text.as_deref(), Some(expected));
        assert!(facts.symbols.iter().any(|symbol| {
            symbol.detail.as_deref() == Some("architecture service icon text")
                && symbol.name == expected
        }));
    }

    #[test]
    fn architecture_langium_strings_and_quoted_titles_can_span_lines() {
        let text = "architecture-beta\n\
service icon \"first\nsecond\"\n\
service captioned(server)[\"third\nfourth\"]\n";
        let meta = test_meta();
        let source = parse::parse_semantic_source(text, &meta).unwrap();

        let json = source.compat_json(&meta);
        let render = source.render_model();
        let facts = source.editor_facts();

        assert_eq!(json["services"][0]["iconText"], "first\nsecond");
        assert_eq!(json["services"][1]["title"], "third\nfourth");
        assert_eq!(render.nodes[0].icon_text.as_deref(), Some("first\nsecond"));
        assert_eq!(render.nodes[1].title.as_deref(), Some("third\nfourth"));
        assert!(facts.symbols.iter().any(|symbol| {
            symbol.detail.as_deref() == Some("architecture service icon text")
                && symbol.name == "first\nsecond"
        }));
        assert!(facts.symbols.iter().any(|symbol| {
            symbol.detail.as_deref() == Some("architecture service title")
                && symbol.name == "third\nfourth"
        }));
    }

    #[test]
    fn architecture_quoted_terminals_keep_percent_markers_as_content() {
        let model = parse(
            "architecture-beta\n\
service api \"before %% after\"\n\
service caption(server)[\"title %% kept\"]\n",
        );

        assert_eq!(model["services"][0]["iconText"], "before %% after");
        assert_eq!(model["services"][1]["title"], "title %% kept");
    }

    #[test]
    fn architecture_unquoted_titles_accept_unicode_punctuation_and_percent_markers() {
        let model = parse(
            "architecture-beta\n\
 group root(cloud)[采集器]\n\
 service api(server)[Structure + Src %% v6] in root %% trailing comment\n\
 service db(database)[Café]\n\
 api:R -[TCP/IP: v6 %% path]- L:db %% trailing comment\n",
        );

        assert_eq!(model["groups"][0]["title"], "采集器");
        assert_eq!(model["services"][0]["title"], "Structure + Src %% v6");
        assert_eq!(model["services"][1]["title"], "Café");
        assert_eq!(model["edges"][0]["title"], "TCP/IP: v6 %% path");
    }

    #[test]
    fn architecture_editor_payload_spans_point_to_values_when_values_match_keywords() {
        let text = "architecture-beta\n  title title\n  accTitle: accTitle\n  accDescr: accDescr\n";
        let facts = crate::family::test_support::editor_facts(
            parse_architecture_json_and_editor_facts,
            text,
            &test_meta(),
        );

        for (detail, name, needle) in [
            ("architecture title", "title", "title title"),
            (
                "architecture accessibility title",
                "accTitle",
                "accTitle: accTitle",
            ),
            (
                "architecture accessibility description",
                "accDescr",
                "accDescr: accDescr",
            ),
        ] {
            let value_start = text.find(needle).unwrap() + needle.rfind(name).unwrap();
            assert_eq!(
                payload_selection(&facts, detail, name),
                SourceSpan::new(value_start, value_start + name.len()),
                "wrong span for {detail}"
            );
        }
    }

    #[test]
    fn architecture_accessibility_title_and_descr() {
        let model = parse(
            "architecture-beta\n  accTitle: Accessibility Title\n  accDescr: Accessibility Description\n",
        );
        assert_eq!(model["accTitle"].as_str().unwrap(), "Accessibility Title");
        assert_eq!(
            model["accDescr"].as_str().unwrap(),
            "Accessibility Description"
        );
    }

    #[test]
    fn architecture_common_fields_treat_percent_markers_as_inline_comments_inside_quotes() {
        let text = concat!(
            "architecture-beta\n",
            "title \"Title %% ignored\n",
            "accTitle: \"Accessible %% ignored\n",
            "accDescr: \"Description %% ignored\n",
        );
        let model = parse(text);
        let facts = crate::family::test_support::editor_facts(
            parse_architecture_json_and_editor_facts,
            text,
            &test_meta(),
        );

        assert_eq!(model["title"], "\"Title");
        assert_eq!(model["accTitle"], "\"Accessible");
        assert_eq!(model["accDescr"], "\"Description");
        assert!(
            facts
                .symbols
                .iter()
                .filter(|symbol| symbol.role == crate::EditorSemanticRole::Payload)
                .all(|symbol| !symbol.name.contains("ignored"))
        );
    }

    #[test]
    fn architecture_multiline_acc_descr() {
        let model = parse("architecture-beta\n  accDescr {\n    Accessibility Description\n  }\n");
        assert_eq!(
            model["accDescr"].as_str().unwrap(),
            "Accessibility Description"
        );
    }

    #[test]
    fn architecture_multiline_acc_descr_allows_adjacent_opening_brace() {
        let model = parse("architecture-beta\naccDescr{Accessibility Description}\n");

        assert_eq!(
            model["accDescr"].as_str().unwrap(),
            "Accessibility Description"
        );
    }

    #[test]
    fn architecture_multiline_acc_descr_allows_newline_before_opening_brace() {
        let model = parse("architecture-beta\naccDescr\n{\n  Accessibility Description\n}\n");

        assert_eq!(
            model["accDescr"].as_str().unwrap(),
            "Accessibility Description"
        );
    }

    #[test]
    fn architecture_edge_with_ports_is_parsed() {
        let model =
            parse("architecture-beta\n  service db\n  service server\n  db:L -- R:server\n");
        let edges = model["edges"].as_array().unwrap();
        assert_eq!(edges.len(), 1);
        assert_eq!(edges[0]["lhsId"].as_str().unwrap(), "db");
        assert_eq!(edges[0]["lhsDir"].as_str().unwrap(), "L");
        assert_eq!(edges[0]["rhsId"].as_str().unwrap(), "server");
        assert_eq!(edges[0]["rhsDir"].as_str().unwrap(), "R");
    }

    #[test]
    fn architecture_edge_with_title_is_parsed() {
        let model = parse("architecture-beta\n  service a\n  service b\n  a:L -[Label]- R:b\n");
        let edges = model["edges"].as_array().unwrap();
        assert_eq!(edges.len(), 1);
        assert_eq!(edges[0]["title"].as_str().unwrap(), "Label");
        assert_eq!(edges[0]["lhsDir"].as_str().unwrap(), "L");
        assert_eq!(edges[0]["rhsDir"].as_str().unwrap(), "R");
    }

    #[test]
    fn architecture_align_layout_hints_are_parsed() {
        let model = parse(
            "architecture-beta\n  group api(cloud)[API]\n  service db1(database)[DB1] in api\n  service db2(database)[DB2] in api\n  service db3(database)[DB3] in api\n  junction join\n  align row db1 db2 db3\n  align column db2 join\n",
        );
        assert_eq!(
            model["layoutHints"],
            serde_json::json!([
                {"direction": "row", "members": ["db1", "db2", "db3"]},
                {"direction": "column", "members": ["db2", "join"]}
            ])
        );
    }

    #[test]
    fn architecture_align_requires_two_members_in_the_statement_grammar() {
        let text = "architecture-beta\nservice api\nalign row api\n";
        let diagnostic = parse_err(text);
        let insertion = text.trim_end().len();

        assert_eq!(
            diagnostic.message(),
            "An align directive requires at least two members; got 1"
        );
        assert_eq!(
            diagnostic.span(),
            Some(SourceSpan::new(insertion, insertion))
        );
        assert_eq!(
            diagnostic.span_kind(),
            ParseDiagnosticSpanKind::InsertionPoint
        );
    }

    #[test]
    fn architecture_align_editor_facts_preserve_spans() {
        let text = "architecture-beta\n  service rowspan(server)[Rowspan]\n  service columnar(server)[Columnar]\n  align row rowspan columnar\n";
        let facts = crate::family::test_support::editor_facts(
            parse_architecture_json_and_editor_facts,
            text,
            &test_meta(),
        );

        let row_start = text.find("align row").unwrap() + "align ".len();
        assert_eq!(
            payload_selection(&facts, "architecture alignment direction", "row"),
            SourceSpan::new(row_start, row_start + "row".len())
        );

        for member in ["rowspan", "columnar"] {
            let member_start = text.rfind(member).unwrap();
            assert_eq!(
                facts
                    .symbols
                    .iter()
                    .find(|symbol| {
                        symbol.detail.as_deref() == Some("architecture alignment member")
                            && symbol.name == member
                    })
                    .unwrap_or_else(|| panic!("missing alignment member symbol {member}"))
                    .selection,
                SourceSpan::new(member_start, member_start + member.len())
            );
        }
    }

    #[test]
    fn architecture_align_rejects_unknown_member_with_exact_span() {
        let text = "architecture-beta\n  service a(server)[A]\n  service b(server)[B]\n  align row a b ghost\n";
        let diagnostic = parse_err(text);
        let offset = text.find("ghost").unwrap();

        assert!(diagnostic.message().contains("ghost"));
        assert_eq!(
            diagnostic.span(),
            Some(SourceSpan::new(offset, offset + "ghost".len()))
        );
        assert_eq!(diagnostic.span_kind(), ParseDiagnosticSpanKind::Exact);
    }

    #[test]
    fn architecture_align_rejects_duplicate_member_with_exact_span() {
        let text = "architecture-beta\n  service a(server)[A]\n  align row a a\n";
        let diagnostic = parse_err(text);
        let offset = text.rfind("a").unwrap();

        assert!(diagnostic.message().contains("more than once"));
        assert_eq!(diagnostic.span(), Some(SourceSpan::new(offset, offset + 1)));
        assert_eq!(diagnostic.span_kind(), ParseDiagnosticSpanKind::Exact);
    }

    #[test]
    fn architecture_rejects_legacy_edge_shorthand() {
        let engine = Engine::new();
        let err = block_on(engine.parse_diagram(
            "architecture-beta\n  service a\n  service b\n  a (T--B) b\n",
            ParseOptions::default(),
        ))
        .unwrap_err();
        let msg = format!("{err}");
        assert!(msg.contains("expected ':' for lhs port") || msg.contains("unrecognized"));
    }

    #[test]
    fn architecture_invalid_service_id_reports_insertion_point() {
        let text = "architecture-beta\n  service -bad\n";
        let diagnostic = parse_err(text);
        let offset = text.find("-bad").unwrap();

        assert_eq!(diagnostic.message(), "invalid service id");
        assert_eq!(diagnostic.span(), Some(SourceSpan::new(offset, offset)));
        assert_eq!(
            diagnostic.span_kind(),
            ParseDiagnosticSpanKind::InsertionPoint
        );
    }

    #[test]
    fn architecture_invalid_edge_direction_reports_exact_token_span() {
        let text = "architecture-beta\n  service a\n  service b\n  a:X -- R:b\n";
        let diagnostic = parse_err(text);
        let offset = text.find('X').unwrap();

        assert_eq!(diagnostic.message(), "invalid lhs direction");
        assert_eq!(diagnostic.span(), Some(SourceSpan::new(offset, offset + 1)));
        assert_eq!(diagnostic.span_kind(), ParseDiagnosticSpanKind::Exact);
    }

    #[test]
    fn architecture_trailing_group_input_reports_exact_token_span() {
        let text = "architecture-beta\n  group core extra\n";
        let diagnostic = parse_err(text);
        let offset = text.find("extra").unwrap();

        assert_eq!(diagnostic.message(), "unexpected trailing input");
        assert_eq!(
            diagnostic.span(),
            Some(SourceSpan::new(offset, offset + "extra".len()))
        );
        assert_eq!(diagnostic.span_kind(), ParseDiagnosticSpanKind::Exact);
    }

    #[test]
    fn architecture_duplicate_service_reports_exact_id_span() {
        let text = "architecture-beta\n  service api\n  service api\n";
        let diagnostic = parse_err(text);
        let offset = text.rfind("api").unwrap();

        assert!(diagnostic.message().contains("already in use"));
        assert_eq!(
            diagnostic.span(),
            Some(SourceSpan::new(offset, offset + "api".len()))
        );
        assert_eq!(diagnostic.span_kind(), ParseDiagnosticSpanKind::Exact);
    }

    #[test]
    fn architecture_unknown_parent_reports_exact_reference_span() {
        let text = "architecture-beta\n  service api in missing\n";
        let diagnostic = parse_err(text);
        let offset = text.find("missing").unwrap();

        assert!(diagnostic.message().contains("parent does not exist"));
        assert_eq!(
            diagnostic.span(),
            Some(SourceSpan::new(offset, offset + "missing".len()))
        );
        assert_eq!(diagnostic.span_kind(), ParseDiagnosticSpanKind::Exact);
    }

    #[test]
    fn architecture_unknown_edge_endpoint_reports_exact_reference_span() {
        let text = "architecture-beta\n  service api\n  api:L -- R:missing\n";
        let diagnostic = parse_err(text);
        let offset = text.find("missing").unwrap();

        assert!(diagnostic.message().contains("right-hand id"));
        assert_eq!(
            diagnostic.span(),
            Some(SourceSpan::new(offset, offset + "missing".len()))
        );
        assert_eq!(diagnostic.span_kind(), ParseDiagnosticSpanKind::Exact);
    }

    #[test]
    fn architecture_recovered_facts_preserve_declared_junction_kind() {
        let text = "architecture-beta\n\
junction hub\n\
service api\n\
api:R -- L:hub\n\
missing:R -- L:api\n";
        let facts = crate::family::test_support::editor_facts(
            parse_architecture_json_and_editor_facts,
            text,
            &test_meta(),
        );

        assert_eq!(facts.completeness, EditorSemanticCompleteness::Recovered);
        assert!(!facts.diagnostics.is_empty());
        assert_eq!(
            facts
                .symbols
                .iter()
                .filter(|symbol| symbol.name == "hub")
                .map(|symbol| (symbol.role, symbol.kind))
                .collect::<Vec<_>>(),
            [
                (
                    crate::EditorSemanticRole::Entity,
                    crate::EditorSemanticKind::Object,
                ),
                (
                    crate::EditorSemanticRole::Reference,
                    crate::EditorSemanticKind::Object,
                ),
            ]
        );
    }

    #[test]
    fn architecture_rejects_group_ids_as_edge_endpoints() {
        let cases = [
            (
                "architecture-beta\ngroup cloud\nservice api\ncloud:L -- R:api\n",
                "cloud",
                "The left-hand id [cloud] is a group; architecture edges require a service or junction endpoint.",
            ),
            (
                "architecture-beta\ngroup cloud\nservice api\napi:L -- R:cloud\n",
                "cloud",
                "The right-hand id [cloud] is a group; architecture edges require a service or junction endpoint.",
            ),
        ];

        for (text, endpoint, message) in cases {
            let diagnostic = parse_err(text);
            let offset = text.rfind(endpoint).unwrap();

            assert_eq!(diagnostic.message(), message);
            assert_eq!(
                diagnostic.span(),
                Some(SourceSpan::new(offset, offset + endpoint.len()))
            );
            assert_eq!(diagnostic.span_kind(), ParseDiagnosticSpanKind::Exact);
        }
    }

    #[test]
    fn architecture_semantic_source_projects_json_render_and_editor_facts() {
        let text = "architecture-beta\n\
title Platform\n\
accTitle: Platform overview\n\
accDescr {\n\
  Public API\n\
  and data plane\n\
}\n\
api:R -- L:join\n\
align row api db join\n\
junction join in child\n\
service api(server)[API] in child\n\
service db(database)[DB] in child\n\
group root(cloud)[Root]\n\
group child(cloud)[Child] in root\n";
        let meta = test_meta();
        let source = parse::parse_semantic_source(text, &meta).unwrap();

        let json = source.compat_json(&meta);
        let render = source.render_model();
        let facts = source.editor_facts();

        assert_eq!(json["title"].as_str(), render.title.as_deref());
        assert_eq!(json["accTitle"].as_str(), render.acc_title.as_deref());
        assert_eq!(json["accDescr"].as_str(), render.acc_descr.as_deref());
        assert_eq!(
            json["groups"],
            serde_json::to_value(&render.groups).unwrap()
        );
        assert_eq!(json["edges"], serde_json::to_value(&render.edges).unwrap());
        assert_eq!(
            json["layoutHints"],
            serde_json::to_value(&render.layout_hints).unwrap()
        );
        let json_nodes = json["nodes"].as_array().unwrap();
        assert_eq!(json_nodes.len(), render.nodes.len());
        for (json_node, render_node) in json_nodes.iter().zip(&render.nodes) {
            assert_eq!(json_node["id"], render_node.id);
            assert_eq!(
                json_node["type"],
                serde_json::to_value(render_node.node_type).unwrap()
            );
            assert_eq!(
                json_node["icon"],
                serde_json::to_value(&render_node.icon).unwrap()
            );
            assert_eq!(
                json_node["iconText"],
                serde_json::to_value(&render_node.icon_text).unwrap()
            );
            assert_eq!(
                json_node["title"],
                serde_json::to_value(&render_node.title).unwrap()
            );
            assert_eq!(
                json_node["in"],
                serde_json::to_value(&render_node.in_group).unwrap()
            );

            let edge_indices = json_node["edges"]
                .as_array()
                .unwrap()
                .iter()
                .map(|node_edge| {
                    json["edges"]
                        .as_array()
                        .unwrap()
                        .iter()
                        .position(|edge| edge == node_edge)
                        .expect("node edge should reference the canonical edge array")
                })
                .collect::<Vec<_>>();
            assert_eq!(edge_indices, render_node.edge_indices);
        }
        assert_eq!(facts.completeness, EditorSemanticCompleteness::Complete);
        assert!(facts.diagnostics.is_empty());
        assert!(facts.symbols.iter().any(|symbol| {
            symbol.detail.as_deref() == Some("architecture accessibility description")
                && symbol.name == "Public API\nand data plane"
        }));
    }

    #[test]
    fn architecture_parse_pipeline_returns_combined_json_and_editor_projection() {
        let text = "architecture-beta\nservice api(server)[API]\n";
        let parsed = Engine::new()
            .parse_diagram_snapshot_sync(text)
            .unwrap()
            .unwrap();

        assert_eq!(
            parsed
                .outcome()
                .parsed_model()
                .expect("expected parsed snapshot")["services"][0]["id"],
            "api"
        );
        let ParsedEditorFacts::Available(facts) = parsed.editor_facts() else {
            panic!("Architecture should return parser-backed editor facts");
        };
        assert!(facts.symbols.iter().any(|symbol| {
            symbol.name == "api" && symbol.detail.as_deref() == Some("architecture service")
        }));
    }

    #[test]
    fn architecture_combined_projection_honors_custom_registry_replacement() {
        fn detect_architecture(code: &str, _config: &mut MermaidConfig) -> bool {
            code.starts_with("architecture-beta")
        }

        fn custom_architecture_parser(
            _code: &str,
            _meta: &ParseMetadata,
            control: &crate::OperationControl,
        ) -> crate::OperationControlResult<Result<Value>> {
            control.checkpoint()?;
            Ok(Ok(json!({ "type": "custom-architecture" })))
        }

        let text = "architecture-beta\nservice api(server)[API]\n";
        let mut engine = Engine::new();
        engine
            .registry_mut()
            .add_fn("architecture", detect_architecture);
        engine
            .diagram_registry_mut()
            .insert("architecture", custom_architecture_parser);

        let plain = engine
            .parse_diagram_sync(text, ParseOptions::strict())
            .unwrap()
            .unwrap();
        let combined = engine.parse_diagram_snapshot_sync(text).unwrap().unwrap();

        assert_eq!(
            combined
                .outcome()
                .parsed_model()
                .expect("expected parsed snapshot"),
            &plain.model
        );
        assert_eq!(
            combined
                .outcome()
                .parsed_model()
                .expect("expected parsed snapshot")["type"],
            "custom-architecture"
        );
        assert!(matches!(
            combined.editor_facts(),
            ParsedEditorFacts::Unavailable
        ));
        assert!(
            engine
                .parse_editor_semantic_facts_with_type_sync("architecture", text,)
                .unwrap()
                .is_none()
        );
    }

    #[test]
    fn architecture_combined_projection_remaps_preprocessed_spans() {
        let text = concat!(
            "---\n",
            "config:\n",
            "  theme: dark\n",
            "---\n",
            "%%{init: {\"theme\": \"default\"}}%%\n",
            "architecture-beta\n",
            "service api(server)[API]\n",
        );
        let parsed = Engine::new()
            .parse_diagram_snapshot_sync(text)
            .unwrap()
            .unwrap();
        let ParsedEditorFacts::Available(facts) = parsed.editor_facts() else {
            panic!("Architecture should return parser-backed editor facts");
        };
        let api = facts
            .symbols
            .iter()
            .find(|symbol| {
                symbol.name == "api" && symbol.detail.as_deref() == Some("architecture service")
            })
            .unwrap();
        let api_start = text.rfind("api").unwrap();

        assert_eq!(api.selection, SourceSpan::new(api_start, api_start + 3));
    }

    #[test]
    fn architecture_combined_projection_remaps_crlf_and_unicode_payload_spans() {
        let text = concat!(
            "---\r\n",
            "config:\r\n",
            "  theme: dark\r\n",
            "---\r\n",
            "%%{init: {\"theme\": \"default\"}}%%\r\n",
            "architecture-beta\r\n",
            "service api(server)[\"Gateway \u{7f51}\u{5173}\"]\r\n",
        );
        let parsed = Engine::new()
            .parse_diagram_snapshot_sync(text)
            .unwrap()
            .unwrap();
        let ParsedEditorFacts::Available(facts) = parsed.editor_facts() else {
            panic!("Architecture should return parser-backed editor facts");
        };
        let payload = facts
            .symbols
            .iter()
            .find(|symbol| {
                symbol.name == "Gateway \u{7f51}\u{5173}"
                    && symbol.detail.as_deref() == Some("architecture service title")
            })
            .expect("missing Unicode Architecture title fact");
        let raw_payload = "\"Gateway \u{7f51}\u{5173}\"";
        let payload_start = text.rfind(raw_payload).unwrap();

        assert_eq!(
            payload.selection,
            SourceSpan::new(payload_start, payload_start + raw_payload.len())
        );
        assert_eq!(
            &text[payload.selection.start..payload.selection.end],
            raw_payload
        );
    }

    #[test]
    fn architecture_combined_projection_remaps_spans_across_removed_body_segments() {
        let text = concat!(
            "architecture-beta\n",
            "service before(server)[Before]\n",
            "%%{init: {\"theme\": \"default\"}}%%\n",
            "%% removed body comment\n",
            "service after(database)[After]\n",
        );
        let parsed = Engine::new()
            .parse_diagram_snapshot_sync(text)
            .unwrap()
            .unwrap();
        let ParsedEditorFacts::Available(facts) = parsed.editor_facts() else {
            panic!("Architecture should return parser-backed editor facts");
        };
        let after = facts
            .symbols
            .iter()
            .find(|symbol| {
                symbol.name == "after" && symbol.detail.as_deref() == Some("architecture service")
            })
            .expect("missing service after removed body segments");
        let after_start = text.rfind("after").unwrap();

        assert_eq!(
            after.selection,
            SourceSpan::new(after_start, after_start + "after".len())
        );
    }

    #[test]
    fn architecture_trace_keeps_lexical_editor_order_while_db_uses_category_order() {
        let text = "architecture-beta\n\
api:R -- L:join\n\
align row api join\n\
junction join in root\n\
service api(server)[API] in root\n\
group root(cloud)[Root]\n";
        let meta = test_meta();
        let source = parse::parse_semantic_source(text, &meta).unwrap();
        let json = source.compat_json(&meta);
        let facts = source.editor_facts();

        let node_ids = json["nodes"]
            .as_array()
            .unwrap()
            .iter()
            .map(|node| node["id"].as_str().unwrap())
            .collect::<Vec<_>>();
        assert_eq!(node_ids, ["api", "join"]);

        let lexical_symbols = facts
            .symbols
            .iter()
            .filter(|symbol| {
                matches!(
                    symbol.role,
                    crate::EditorSemanticRole::Entity | crate::EditorSemanticRole::Reference
                )
            })
            .map(|symbol| {
                (
                    symbol.name.as_str(),
                    symbol.detail.as_deref().unwrap(),
                    symbol.kind,
                    symbol.role,
                )
            })
            .collect::<Vec<_>>();
        assert_eq!(
            lexical_symbols,
            [
                (
                    "api",
                    "architecture edge endpoint",
                    crate::EditorSemanticKind::Variable,
                    crate::EditorSemanticRole::Reference,
                ),
                (
                    "join",
                    "architecture edge endpoint",
                    crate::EditorSemanticKind::Object,
                    crate::EditorSemanticRole::Reference,
                ),
                (
                    "api",
                    "architecture alignment member",
                    crate::EditorSemanticKind::Variable,
                    crate::EditorSemanticRole::Reference,
                ),
                (
                    "join",
                    "architecture alignment member",
                    crate::EditorSemanticKind::Object,
                    crate::EditorSemanticRole::Reference,
                ),
                (
                    "join",
                    "architecture junction",
                    crate::EditorSemanticKind::Object,
                    crate::EditorSemanticRole::Entity,
                ),
                (
                    "root",
                    "architecture junction parent",
                    crate::EditorSemanticKind::Namespace,
                    crate::EditorSemanticRole::Reference,
                ),
                (
                    "api",
                    "architecture service",
                    crate::EditorSemanticKind::Variable,
                    crate::EditorSemanticRole::Entity,
                ),
                (
                    "root",
                    "architecture service parent",
                    crate::EditorSemanticKind::Namespace,
                    crate::EditorSemanticRole::Reference,
                ),
                (
                    "root",
                    "architecture group",
                    crate::EditorSemanticKind::Namespace,
                    crate::EditorSemanticRole::Entity,
                ),
            ]
        );
    }

    #[test]
    fn architecture_editor_roles_keep_declarations_and_references_in_separate_projections() {
        let text = "architecture-beta\n\
group platform\n\
service api in platform\n\
junction hub in platform\n\
align row api hub\n\
api:R -- L:hub\n";
        let facts = crate::family::test_support::editor_facts(
            parse_architecture_json_and_editor_facts,
            text,
            &test_meta(),
        );

        let declarations = facts
            .symbols
            .iter()
            .filter(|symbol| symbol.role == crate::EditorSemanticRole::Entity)
            .collect::<Vec<_>>();
        assert_eq!(
            declarations
                .iter()
                .map(|symbol| (symbol.name.as_str(), symbol.kind))
                .collect::<Vec<_>>(),
            [
                ("platform", crate::EditorSemanticKind::Namespace),
                ("api", crate::EditorSemanticKind::Variable),
                ("hub", crate::EditorSemanticKind::Object),
            ]
        );

        let references = facts
            .symbols
            .iter()
            .filter(|symbol| symbol.role == crate::EditorSemanticRole::Reference)
            .collect::<Vec<_>>();
        assert_eq!(
            references
                .iter()
                .map(|symbol| (symbol.name.as_str(), symbol.kind))
                .collect::<Vec<_>>(),
            [
                ("platform", crate::EditorSemanticKind::Namespace),
                ("platform", crate::EditorSemanticKind::Namespace),
                ("api", crate::EditorSemanticKind::Variable),
                ("hub", crate::EditorSemanticKind::Object),
                ("api", crate::EditorSemanticKind::Variable),
                ("hub", crate::EditorSemanticKind::Object),
            ]
        );

        for declaration in declarations {
            assert!(declaration.role.contributes_completion());
            assert!(declaration.role.contributes_outline());
            assert!(declaration.role.contributes_references());
        }
        for reference in references {
            assert!(!reference.role.contributes_completion());
            assert!(!reference.role.contributes_outline());
            assert!(reference.role.contributes_references());
            assert_eq!(
                reference.rename_policy,
                crate::EditorRenamePolicy::ArchitectureIdentifier
            );
            assert!(facts.symbols.iter().any(|declaration| {
                declaration.role == crate::EditorSemanticRole::Entity
                    && declaration.name == reference.name
                    && declaration.kind == reference.kind
            }));
        }
    }

    #[test]
    fn architecture_multiline_acc_descr_projects_complete_payload_span() {
        let text = "architecture-beta\naccDescr {\n  First   line\n\n  Second line\n}\n";
        let facts = crate::family::test_support::editor_facts(
            parse_architecture_json_and_editor_facts,
            text,
            &test_meta(),
        );
        let payload = facts
            .symbols
            .iter()
            .find(|symbol| {
                symbol.detail.as_deref() == Some("architecture accessibility description")
            })
            .unwrap();

        assert_eq!(payload.name, "First line\nSecond line");
        assert_eq!(
            &text[payload.selection.start..payload.selection.end],
            "First   line\n\n  Second line"
        );
        assert_eq!(facts.completeness, EditorSemanticCompleteness::Complete);
        assert!(facts.diagnostics.is_empty());
    }

    #[test]
    fn architecture_recovering_parser_keeps_partial_statement_facts_out_of_db() {
        let cases = [
            (
                "architecture-beta\ngroup root(\n",
                "root",
                "architecture group",
            ),
            (
                "architecture-beta\nservice api(server)[\n",
                "server",
                "architecture service icon",
            ),
            (
                "architecture-beta\napi:L --\n",
                "api",
                "architecture edge endpoint",
            ),
            (
                "architecture-beta\nservice api\nalign row api\n",
                "api",
                "architecture alignment member",
            ),
        ];

        for (text, name, detail) in cases {
            assert!(parse_architecture(text, &test_meta()).is_err());
            let facts = crate::family::test_support::editor_facts(
                parse_architecture_json_and_editor_facts,
                text,
                &test_meta(),
            );
            assert_eq!(facts.completeness, EditorSemanticCompleteness::Recovered);
            assert!(
                facts.symbols.iter().any(|symbol| {
                    symbol.name == name && symbol.detail.as_deref() == Some(detail)
                })
            );
            assert!(!facts.diagnostics.is_empty());
        }
    }

    #[test]
    fn architecture_json_and_typed_errors_share_diagnostic_source() {
        let text = "architecture-beta\nservice api\nservice api\n";
        let json_error = parse_architecture(text, &test_meta()).unwrap_err();
        let typed_error = parse_architecture_model_for_render(text, &test_meta()).unwrap_err();
        let Error::DiagramParse {
            diagnostic: json_diagnostic,
            ..
        } = json_error
        else {
            panic!("expected JSON parse diagnostic");
        };
        let Error::DiagramParse {
            diagnostic: typed_diagnostic,
            ..
        } = typed_error
        else {
            panic!("expected typed parse diagnostic");
        };

        assert_eq!(json_diagnostic, typed_diagnostic);
        let facts = crate::family::test_support::editor_facts(
            parse_architecture_json_and_editor_facts,
            text,
            &test_meta(),
        );
        assert_eq!(facts.completeness, EditorSemanticCompleteness::Recovered);
        assert_eq!(facts.diagnostics[0].message, json_diagnostic.message());
        assert_eq!(facts.diagnostics[0].span, json_diagnostic.span());
        assert_eq!(
            facts
                .symbols
                .iter()
                .filter(|symbol| symbol.detail.as_deref() == Some("architecture service"))
                .count(),
            2
        );
    }
}
