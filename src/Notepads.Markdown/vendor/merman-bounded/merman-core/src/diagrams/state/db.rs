use crate::sanitize::{sanitize_text, sanitize_text_or_array};
use crate::{
    Error, MermaidConfig, OperationCancelled, OperationControl, OperationControlResult,
    ParseMetadata, Result,
};
use indexmap::IndexMap;
use serde_json::{Map, Value, json};
use std::collections::HashMap;
use std::collections::hash_map::Entry;

use super::{
    StateDiagramRenderEdge, StateDiagramRenderLink, StateDiagramRenderLinks,
    StateDiagramRenderModel, StateDiagramRenderNode, StateDiagramRenderNote,
    StateDiagramRenderRelation, StateDiagramRenderState, StateDiagramRenderStyleClass,
    StateDocument, StateDocumentId, StateStatement, StateStatementNote, StateStatementState,
};

#[derive(Debug, Clone, Default)]
struct StyleClass {
    id: String,
    styles: Vec<String>,
    text_styles: Vec<String>,
}

#[derive(Debug, Clone)]
struct RelationEdge {
    id1: String,
    id2: String,
    relation_title: Option<String>,
}

#[derive(Debug, Clone)]
struct StateRecord {
    id: String,
    ty: String,
    descriptions: Vec<String>,
    note: Option<StateStatementNote>,
    classes: Vec<String>,
    styles: Vec<String>,
    text_styles: Vec<String>,
    start: Option<bool>,
}

#[derive(Debug, Default)]
pub(super) struct StateDb {
    document: StateDocument,
    states: HashMap<String, StateRecord>,
    state_order: Vec<String>,
    relations: Vec<RelationEdge>,
    style_classes: IndexMap<String, StyleClass>,
    direction: Option<String>,
    acc_title: Option<String>,
    acc_descr: Option<String>,
    generated_id_cnt: usize,
    links: HashMap<String, Vec<Link>>,
}

impl StateDb {
    pub(super) fn new() -> Self {
        Self::default()
    }

    fn generate_id(&mut self) -> String {
        self.generated_id_cnt += 1;
        let cnt = self.generated_id_cnt as u64;

        // Mermaid `@11.12.2` uses `Math.random().toString(36).substr(2, 12)` plus a monotonically
        // increasing counter. We keep the same `id-<base36>-<n>` shape, but generate the middle
        // segment deterministically to keep snapshots stable.
        let mut x = cnt ^ 0x9e37_79b9_7f4a_7c15u64;
        x = x.wrapping_mul(0xbf58_476d_1ce4_e5b9u64);
        x ^= x >> 32;
        x = x.wrapping_mul(0x94d0_49bb_1331_11ebu64);
        x ^= x >> 32;

        fn to_base36(mut v: u64) -> String {
            const DIGITS: &[u8] = b"0123456789abcdefghijklmnopqrstuvwxyz";
            if v == 0 {
                return "0".to_string();
            }
            let mut buf = [0u8; 32];
            let mut i = buf.len();
            while v > 0 {
                let rem = (v % 36) as usize;
                v /= 36;
                i -= 1;
                buf[i] = DIGITS[rem];
            }
            String::from_utf8_lossy(&buf[i..]).to_string()
        }

        let mut mid = to_base36(x);
        if mid.len() < 12 {
            let pad = "0".repeat(12 - mid.len());
            mid = format!("{pad}{mid}");
        } else if mid.len() > 12 {
            mid = mid[mid.len() - 12..].to_string();
        }

        format!("id-{mid}-{}", self.generated_id_cnt)
    }

    pub(super) fn set_root_doc(
        &mut self,
        mut document: StateDocument,
        control: &OperationControl,
    ) -> OperationControlResult<()> {
        self.translate_doc(&mut document, control)?;
        self.extract(document.root_statements(), control)?;
        self.document = document;
        Ok(())
    }

    fn translate_state_ref(parent_id: &str, s: &mut StateStatementState, first: bool) {
        if s.id.trim() == "[*]" {
            s.id = format!("{}_{}", parent_id, if first { "start" } else { "end" });
            s.start = Some(first);
        } else {
            s.id = s.id.trim().to_string();
        }
    }

    fn translate_state_concurrency_split(
        &mut self,
        document: &mut StateDocument,
        doc: StateDocumentId,
        control: &OperationControl,
    ) -> OperationControlResult<()> {
        let old = std::mem::take(&mut document.documents[doc.0]);
        let mut out = Vec::new();
        let mut current = Vec::new();
        let mut saw_divider = false;
        for (index, stmt) in old.into_iter().enumerate() {
            if index.is_multiple_of(128) {
                control.checkpoint()?;
            }
            match stmt {
                StateStatement::State(mut state) if state.ty == "divider" => {
                    saw_divider = true;
                    state.doc = Some(document.push(std::mem::take(&mut current)));
                    out.push(StateStatement::State(state));
                }
                stmt => current.push(stmt),
            }
        }
        if saw_divider && !current.is_empty() {
            let mut divider = StateStatementState::new_typed(self.generate_id(), "divider");
            divider.doc = Some(document.push(std::mem::take(&mut current)));
            out.push(StateStatement::State(divider));
        }
        document.documents[doc.0] = if saw_divider { out } else { current };
        Ok(())
    }

    fn translate_doc(
        &mut self,
        document: &mut StateDocument,
        control: &OperationControl,
    ) -> OperationControlResult<()> {
        let Some(root) = document.root else {
            return Ok(());
        };
        let mut stack = vec![("root".to_string(), root, 0usize)];
        let mut inspected = 0usize;
        while let Some((parent, doc, index)) = stack.last_mut() {
            if inspected.is_multiple_of(128) {
                control.checkpoint()?;
            }
            let Some(stmt) = document.documents[doc.0].get_mut(*index) else {
                stack.pop();
                continue;
            };
            *index += 1;
            let child = match stmt {
                StateStatement::Relation(relation) => {
                    Self::translate_state_ref(parent, &mut relation.state1, true);
                    Self::translate_state_ref(parent, &mut relation.state2, false);
                    None
                }
                StateStatement::State(state) => {
                    Self::translate_state_ref(parent, state, true);
                    state.doc.map(|doc| (state.id.clone(), doc))
                }
                _ => None,
            };
            if let Some((parent, child)) = child {
                self.translate_state_concurrency_split(document, child, control)?;
                stack.push((parent, child, 0));
            }
            inspected = inspected.saturating_add(1);
        }
        control.checkpoint()
    }

    fn extract(
        &mut self,
        root_doc: &[StateStatement],
        control: &OperationControl,
    ) -> OperationControlResult<()> {
        self.states.clear();
        self.state_order.clear();
        self.relations.clear();
        self.style_classes.clear();
        self.direction = None;
        self.acc_title = None;
        self.acc_descr = None;
        self.generated_id_cnt = 0;
        self.links.clear();

        for (index, stmt) in root_doc.iter().enumerate() {
            if index.is_multiple_of(128) {
                control.checkpoint()?;
            }
            match stmt {
                StateStatement::State(s) => self.add_state(s),
                StateStatement::Relation(relation) => self.add_relation(
                    &relation.state1,
                    &relation.state2,
                    relation.description.as_deref(),
                ),
                StateStatement::ClassDef { id, classes } => self.add_style_class(id, classes),
                StateStatement::ApplyClass { ids, class_name } => {
                    self.set_css_class(ids, class_name)
                }
                StateStatement::Style { ids, styles } => self.handle_style_def(ids, styles),
                StateStatement::Direction(dir) => self.direction = Some(dir.clone()),
                StateStatement::AccTitle(t) => self.acc_title = Some(t.clone()),
                StateStatement::AccDescr(d) => self.acc_descr = Some(normalize_multiline_ws(d)),
                StateStatement::Click(c) => self.add_link(&c.id, &c.url, &c.tooltip),
                StateStatement::Noop => {}
            }
        }
        control.checkpoint()
    }

    fn add_link(&mut self, state_id: &str, url: &str, tooltip: &str) {
        // Mermaid's Jison parser creates a fresh idStatement object for every click directive.
        // Group by the normalized state id, but retain declaration order for wrapper replay.
        self.links
            .entry(state_id.to_string())
            .or_default()
            .push(Link {
                url: url.to_string(),
                tooltip: tooltip.to_string(),
            });
    }

    fn ensure_state(&mut self, id: &str) -> &mut StateRecord {
        let id = id.trim().to_string();
        match self.states.entry(id.clone()) {
            Entry::Occupied(entry) => entry.into_mut(),
            Entry::Vacant(entry) => {
                self.state_order.push(id.clone());
                entry.insert(StateRecord {
                    id,
                    ty: "default".to_string(),
                    descriptions: Vec::new(),
                    note: None,
                    classes: Vec::new(),
                    styles: Vec::new(),
                    text_styles: Vec::new(),
                    start: None,
                })
            }
        }
    }

    fn add_description(&mut self, id: &str, descr: &str) {
        let clean = descr.trim().trim_start_matches(':').trim().to_string();
        if clean.is_empty() {
            return;
        }
        self.ensure_state(id).descriptions.push(clean);
    }

    fn add_state(&mut self, state: &StateStatementState) {
        let id = state.id.trim();
        let st = self.ensure_state(id);
        if st.ty == "default" && state.ty != "default" {
            st.ty = state.ty.clone();
        }
        if st.start.is_none() {
            st.start = state.start;
        }
        if state.note.is_some() {
            st.note = state.note.clone();
        }

        if let Some(d) = state.description.as_deref() {
            self.add_description(id, d);
        }
        for d in &state.descriptions {
            self.add_description(id, d);
        }
        for c in &state.classes {
            self.set_css_class(id, c);
        }
        for s in &state.styles {
            self.set_style(id, s);
        }
        for ts in &state.text_styles {
            self.set_text_style(id, ts);
        }
    }

    fn add_relation(
        &mut self,
        s1: &StateStatementState,
        s2: &StateStatementState,
        title: Option<&str>,
    ) {
        let id1 = s1.id.trim();
        let id2 = s2.id.trim();

        self.add_state(s1);
        self.add_state(s2);

        let relation_title = title
            .map(|t| t.trim().to_string())
            .filter(|s| !s.is_empty());

        // Mermaid `@11.12.2` self-loops are special-cased during rendering/layout (fixed
        // `*-cyclic-special-*` ids). Multiple self-loop statements on the same node effectively
        // overwrite; keep the latest label/title.
        if id1 == id2
            && let Some(existing) = self
                .relations
                .iter_mut()
                .find(|r| r.id1 == id1 && r.id2 == id2)
        {
            existing.relation_title = relation_title;
            return;
        }

        self.relations.push(RelationEdge {
            id1: id1.to_string(),
            id2: id2.to_string(),
            relation_title,
        });
    }

    fn add_style_class(&mut self, id: &str, style_attributes: &str) {
        let entry = self
            .style_classes
            .entry(id.trim().to_string())
            .or_insert_with(|| StyleClass {
                id: id.trim().to_string(),
                styles: Vec::new(),
                text_styles: Vec::new(),
            });

        for attrib in style_attributes.split(',') {
            let fixed = attrib
                .split_once(';')
                .map(|(a, _)| a)
                .unwrap_or(attrib)
                .trim()
                .to_string();
            if fixed.is_empty() {
                continue;
            }
            if attrib.contains("color") {
                let t1 = fixed.replace("fill", "bgFill");
                let t2 = t1.replace("color", "fill");
                entry.text_styles.push(t2);
            }
            entry.styles.push(fixed);
        }
    }

    fn set_css_class(&mut self, item_ids: &str, css_class_name: &str) {
        for id in item_ids.split(',') {
            let trimmed = id.trim();
            if trimmed.is_empty() {
                continue;
            }
            self.ensure_state(trimmed)
                .classes
                .push(css_class_name.trim().to_string());
        }
    }

    fn set_style(&mut self, item_id: &str, style_text: &str) {
        self.ensure_state(item_id)
            .styles
            .push(style_text.trim().to_string());
    }

    fn set_text_style(&mut self, item_id: &str, style_text: &str) {
        self.ensure_state(item_id)
            .text_styles
            .push(style_text.trim().to_string());
    }

    fn handle_style_def(&mut self, ids: &str, styles: &str) {
        let styles_vec: Vec<String> = styles
            .split(',')
            .map(|s| s.replace(';', "").trim().to_string())
            .filter(|s| !s.is_empty())
            .collect();
        for id in ids.split(',') {
            let trimmed = id.trim();
            if trimmed.is_empty() {
                continue;
            }
            self.ensure_state(trimmed).styles = styles_vec.clone();
        }
    }

    pub(super) fn into_model(self, meta: &ParseMetadata) -> Result<Value> {
        let model = self
            .into_model_controlled(meta, &OperationControl::new())
            .expect("a private operation control cannot be cancelled")?;
        super::render_model_to_compat_json(&model, meta)
    }

    pub(super) fn into_model_controlled(
        self,
        meta: &ParseMetadata,
        control: &OperationControl,
    ) -> OperationControlResult<Result<StateDiagramRenderModel>> {
        let (nodes, edges) = match build_layout_data_typed(
            &self.document,
            &self.states,
            &self.style_classes,
            &meta.effective_config,
            control,
        ) {
            Ok(data) => data,
            Err(StateBuildError::Cancelled(error)) => return Err(error),
            Err(StateBuildError::Semantic(message)) => {
                return Ok(Err(Error::diagram_parse_fallback(
                    meta.diagram_type.clone(),
                    message,
                )));
            }
        };
        let mut doc_by_state_id = HashMap::new();
        for (index, stmt) in self.document.root_statements().iter().enumerate() {
            if index.is_multiple_of(128) {
                control.checkpoint()?;
            }
            if let StateStatement::State(state) = stmt
                && let Some(doc) = state.doc
            {
                doc_by_state_id.entry(state.id.clone()).or_insert(doc);
            }
        }
        let mut states = HashMap::with_capacity(self.states.len());
        for (index, id) in self.state_order.iter().enumerate() {
            if index.is_multiple_of(128) {
                control.checkpoint()?;
            }
            let Some(s) = self.states.get(id) else {
                continue;
            };
            let note = s.note.as_ref().map(|n| StateDiagramRenderNote {
                position: n.position.clone(),
                text: n.text.clone(),
            });
            states.insert(
                s.id.clone(),
                StateDiagramRenderState {
                    id: s.id.clone(),
                    state_type: s.ty.clone(),
                    descriptions: s.descriptions.clone(),
                    doc: doc_by_state_id.remove(&s.id),
                    note,
                    classes: s.classes.clone(),
                    styles: s.styles.clone(),
                    text_styles: s.text_styles.clone(),
                    start: s.start,
                },
            );
        }
        let mut relations = Vec::with_capacity(self.relations.len());
        for (index, relation) in self.relations.iter().enumerate() {
            if index.is_multiple_of(128) {
                control.checkpoint()?;
            }
            relations.push(StateDiagramRenderRelation {
                id1: relation.id1.clone(),
                id2: relation.id2.clone(),
                relation_title: relation.relation_title.clone(),
            });
        }
        let mut style_classes = IndexMap::with_capacity(self.style_classes.len());
        for (index, (key, class)) in self.style_classes.iter().enumerate() {
            if index.is_multiple_of(128) {
                control.checkpoint()?;
            }
            style_classes.insert(
                key.clone(),
                StateDiagramRenderStyleClass {
                    id: class.id.clone(),
                    styles: class.styles.clone(),
                    text_styles: class.text_styles.clone(),
                },
            );
        }
        let mut links = HashMap::with_capacity(self.links.len());
        for (index, (key, declarations)) in self.links.iter().enumerate() {
            if index.is_multiple_of(128) {
                control.checkpoint()?;
            }
            let declarations = if declarations.len() == 1 {
                let link = &declarations[0];
                StateDiagramRenderLinks::One(StateDiagramRenderLink {
                    url: link.url.clone(),
                    tooltip: link.tooltip.clone(),
                })
            } else {
                let mut out = Vec::with_capacity(declarations.len());
                for (index, link) in declarations.iter().enumerate() {
                    if index.is_multiple_of(128) {
                        control.checkpoint()?;
                    }
                    out.push(StateDiagramRenderLink {
                        url: link.url.clone(),
                        tooltip: link.tooltip.clone(),
                    });
                }
                StateDiagramRenderLinks::Many(out)
            };
            links.insert(key.clone(), declarations);
        }

        control.checkpoint()?;
        Ok(Ok(StateDiagramRenderModel {
            document: self.document,
            direction: self.direction.clone().unwrap_or_else(|| "TB".to_string()),
            acc_title: self.acc_title.clone(),
            acc_descr: self.acc_descr.clone(),
            nodes,
            edges,
            relations,
            links,
            states,
            style_classes,
        }))
    }
}

#[derive(Debug, Clone)]
struct Link {
    url: String,
    tooltip: String,
}

const DEFAULT_NESTED_DOC_DIR: &str = "TB";

const DOMID_STATE: &str = "state";
const DOMID_TYPE_SPACER: &str = "----";

const NOTE: &str = "note";
const PARENT: &str = "parent";
const NOTE_ID: &str = "----note";
const PARENT_ID: &str = "----parent";

const SHAPE_STATE: &str = "rect";
const SHAPE_STATE_WITH_DESC: &str = "rectWithTitle";
const SHAPE_START: &str = "stateStart";
const SHAPE_END: &str = "stateEnd";
const SHAPE_DIVIDER: &str = "divider";
const SHAPE_GROUP: &str = "roundedWithTitle";
const SHAPE_NOTE: &str = "note";
const SHAPE_NOTEGROUP: &str = "noteGroup";

const CSS_EDGE: &str = "transition";
const CSS_EDGE_NOTE_EDGE: &str = "transition note-edge";
const CSS_DIAGRAM_STATE: &str = "statediagram-state";
const CSS_DIAGRAM_NOTE: &str = "statediagram-note";
const CSS_DIAGRAM_CLUSTER: &str = "statediagram-cluster";
const CSS_DIAGRAM_CLUSTER_ALT: &str = "statediagram-cluster-alt";

fn state_dom_id(item_id: &str, counter: usize, ty: Option<&str>) -> String {
    let type_str = ty
        .filter(|t| !t.is_empty())
        .map(|t| format!("{DOMID_TYPE_SPACER}{t}"))
        .unwrap_or_default();
    format!("{DOMID_STATE}-{item_id}{type_str}-{counter}")
}

fn string_array_value(values: &[String]) -> Value {
    Value::Array(values.iter().cloned().map(Value::String).collect())
}

fn option_string_value(value: &Option<String>) -> Value {
    value
        .as_ref()
        .map(|v| Value::String(v.clone()))
        .unwrap_or(Value::Null)
}

#[derive(Debug, Clone)]
struct NodeScratch {
    id: String,
    shape: String,
    label: Value,
    css_classes: String,
    css_styles: Vec<String>,
    node_type: Option<String>,
    dir: Option<String>,
    explicit_dir: Option<bool>,
    is_group: bool,
    parent_id: Option<String>,
    color_index: Option<usize>,
}

fn apply_state_descriptions(
    entry: &mut NodeScratch,
    item_id: &str,
    primary_description: Option<&str>,
    additional_descriptions: &[String],
    config: &MermaidConfig,
) {
    // Mermaid's compact form can produce a primary display label plus an additional description:
    // `state "Some long name" as S1: The description`.
    let mut descriptions = primary_description
        .into_iter()
        .chain(additional_descriptions.iter().map(String::as_str))
        .filter(|description| !description.trim().is_empty())
        .peekable();
    if descriptions.peek().is_none() {
        return;
    }

    let base_label = sanitize_text(item_id, config);
    for description in descriptions {
        match &mut entry.label {
            Value::Array(labels) => {
                entry.shape = SHAPE_STATE_WITH_DESC.to_string();
                labels.push(Value::String(description.to_string()));
            }
            Value::String(label) if !label.is_empty() => {
                entry.shape = SHAPE_STATE_WITH_DESC.to_string();
                if *label == base_label {
                    entry.label = Value::Array(vec![Value::String(description.to_string())]);
                } else {
                    entry.label = Value::Array(vec![
                        Value::String(label.clone()),
                        Value::String(description.to_string()),
                    ]);
                }
            }
            _ => {
                entry.shape = SHAPE_STATE.to_string();
                entry.label = Value::String(description.to_string());
            }
        }
    }

    entry.label = sanitize_text_or_array(&entry.label, config);

    if entry.shape == SHAPE_STATE_WITH_DESC
        && let Some(labels) = entry.label.as_array()
        && labels.len() == 1
    {
        entry.shape = if entry.node_type.as_deref() == Some("group") {
            SHAPE_GROUP.to_string()
        } else {
            SHAPE_STATE.to_string()
        };
    }
}

fn get_dir_for_doc(doc: &[StateStatement], default_dir: &str) -> String {
    let mut dir = default_dir.to_string();
    for stmt in doc {
        if let StateStatement::Direction(d) = stmt {
            dir = d.clone();
        }
    }
    dir
}

fn compiled_styles(css_classes: &str, classes: &IndexMap<String, StyleClass>) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for class_name in css_classes.split_whitespace() {
        if let Some(c) = classes.get(class_name) {
            out.extend(c.styles.iter().cloned());
        }
    }
    out
}

fn upsert_node_typed(
    nodes: &mut Vec<StateDiagramRenderNode>,
    index: &mut HashMap<String, usize>,
    node: StateDiagramRenderNode,
) {
    match index.entry(node.id.clone()) {
        Entry::Occupied(o) => {
            if let Some(dst) = nodes.get_mut(*o.get()) {
                *dst = node;
            }
        }
        Entry::Vacant(v) => {
            v.insert(nodes.len());
            nodes.push(node);
        }
    }
}

fn insert_generated_node_typed(
    nodes: &mut Vec<StateDiagramRenderNode>,
    index: &mut HashMap<String, usize>,
    authored_states: &HashMap<String, StateRecord>,
    node: StateDiagramRenderNode,
    generated_kind: &str,
    owner_id: &str,
) -> std::result::Result<(), String> {
    if authored_states.contains_key(&node.id) {
        return Err(format!(
            "Authored state id `{}` conflicts with generated {generated_kind} id for state `{owner_id}`",
            node.id
        ));
    }

    match index.entry(node.id.clone()) {
        Entry::Occupied(_) => Err(format!(
            "Generated {generated_kind} id `{}` for state `{owner_id}` conflicts with an existing render node",
            node.id
        )),
        Entry::Vacant(entry) => {
            entry.insert(nodes.len());
            nodes.push(node);
            Ok(())
        }
    }
}

fn value_as_f64(v: &Value) -> Option<f64> {
    v.as_f64()
        .or_else(|| v.as_i64().map(|n| n as f64))
        .or_else(|| v.as_u64().map(|n| n as f64))
}

fn arrow_type_end_for_look_name(look: Option<&str>) -> &'static str {
    if look == Some("neo") {
        "arrow_barb_neo"
    } else {
        "arrow_barb"
    }
}

#[derive(Debug)]
enum StateBuildError {
    Cancelled(OperationCancelled),
    Semantic(String),
}

impl From<String> for StateBuildError {
    fn from(message: String) -> Self {
        Self::Semantic(message)
    }
}

impl From<OperationCancelled> for StateBuildError {
    fn from(error: OperationCancelled) -> Self {
        Self::Cancelled(error)
    }
}

fn build_layout_data_typed(
    document: &StateDocument,
    states: &HashMap<String, StateRecord>,
    classes: &IndexMap<String, StyleClass>,
    config: &MermaidConfig,
    control: &OperationControl,
) -> std::result::Result<(Vec<StateDiagramRenderNode>, Vec<StateDiagramRenderEdge>), StateBuildError>
{
    let mut nodes: Vec<StateDiagramRenderNode> = Vec::new();
    let mut edges: Vec<StateDiagramRenderEdge> = Vec::new();
    let mut node_index: HashMap<String, usize> = HashMap::new();

    let mut node_db: HashMap<String, NodeScratch> = HashMap::new();
    let mut graph_item_count: usize = 0;

    struct TypedLayoutContext<'a> {
        document: &'a StateDocument,
        control: &'a OperationControl,
        states: &'a HashMap<String, StateRecord>,
        classes: &'a IndexMap<String, StyleClass>,
        config: &'a MermaidConfig,
        nodes: &'a mut Vec<StateDiagramRenderNode>,
        node_index: &'a mut HashMap<String, usize>,
        edges: &'a mut Vec<StateDiagramRenderEdge>,
        node_db: &'a mut HashMap<String, NodeScratch>,
        graph_item_count: &'a mut usize,
        next_color_index: usize,
        container_color_index: HashMap<String, Option<usize>>,
    }

    fn setup_doc(
        ctx: &mut TypedLayoutContext<'_>,
        parent: Option<&StateStatementState>,
        doc: StateDocumentId,
        alt_flag: bool,
    ) -> std::result::Result<(), StateBuildError> {
        struct DocFrame<'a> {
            parent: Option<&'a StateStatementState>,
            doc: StateDocumentId,
            index: usize,
            alt_flag: bool,
        }

        let mut stack = vec![DocFrame {
            parent,
            doc,
            index: 0,
            alt_flag,
        }];

        let mut inspected = 0usize;
        while let Some(frame) = stack.last_mut() {
            if inspected.is_multiple_of(128) {
                ctx.control.checkpoint()?;
            }
            inspected = inspected.saturating_add(1);
            let Some(item) = ctx.document.documents[frame.doc.0].get(frame.index) else {
                stack.pop();
                continue;
            };
            frame.index += 1;
            let parent = frame.parent;
            let alt_flag = frame.alt_flag;

            match item {
                StateStatement::State(s) => {
                    data_fetcher(ctx, parent, s, alt_flag)?;
                    if let Some(doc) = s.doc {
                        stack.push(DocFrame {
                            parent: Some(s),
                            doc,
                            index: 0,
                            alt_flag: !alt_flag,
                        });
                    }
                }
                StateStatement::Relation(relation) => {
                    let relation = relation.as_ref();
                    data_fetcher(ctx, parent, &relation.state1, alt_flag)?;
                    data_fetcher(ctx, parent, &relation.state2, alt_flag)?;

                    let edge_label_raw = relation.description.clone().unwrap_or_default();
                    let edge_label = sanitize_text(&edge_label_raw, ctx.config);
                    ctx.edges.push(StateDiagramRenderEdge {
                        id: format!("edge{}", *ctx.graph_item_count),
                        start: relation.state1.id.clone(),
                        end: relation.state2.id.clone(),
                        arrow_type_end: arrow_type_end_for_look_name(ctx.config.get_str("look"))
                            .to_string(),
                        classes: CSS_EDGE.to_string(),
                        label: edge_label,
                    });
                    *ctx.graph_item_count += 1;
                }
                _ => {}
            }
        }
        Ok(())
    }

    fn config_state_number(config: &MermaidConfig, key: &str) -> Option<f64> {
        config
            .as_value()
            .get("state")
            .and_then(Value::as_object)
            .and_then(|state| state.get(key))
            .and_then(value_as_f64)
    }

    fn color_slot_for(
        next_color_index: &mut usize,
        container_color_index: &mut HashMap<String, Option<usize>>,
        shape: &str,
        item_id: &str,
        parent: Option<&StateStatementState>,
        user_styled: bool,
    ) -> Option<usize> {
        if shape == SHAPE_DIVIDER
            && let Some(parent_id) = parent.map(|parent| parent.id.as_str())
            && let Some(inherited) = container_color_index.get(parent_id).copied()
        {
            container_color_index.insert(item_id.to_string(), inherited);
            return inherited;
        }

        let slot = *next_color_index;
        *next_color_index += 1;
        let effective = (!user_styled).then_some(slot);
        container_color_index.insert(item_id.to_string(), effective);
        effective
    }

    fn data_fetcher(
        ctx: &mut TypedLayoutContext<'_>,
        parent: Option<&StateStatementState>,
        parsed_item: &StateStatementState,
        alt_flag: bool,
    ) -> std::result::Result<(), StateBuildError> {
        let item_id = parsed_item.id.clone();
        if item_id == "root" || item_id.is_empty() {
            return Ok(());
        }

        let states = ctx.states;
        let classes = ctx.classes;
        let config = ctx.config;

        let db_state = states.get(&item_id);
        let class_str = db_state.map(|s| s.classes.join(" ")).unwrap_or_default();
        let styles = db_state.map(|s| s.styles.clone()).unwrap_or_default();
        let user_styled = !class_str.trim().is_empty() || !styles.is_empty();
        let wrapping_width = config_state_number(config, "wrappingWidth");
        let min_width = config_state_number(config, "minNodeWidth");
        let needs_color_slot =
            parsed_item.doc.is_some() && !ctx.container_color_index.contains_key(&item_id);

        let mut entry = ctx.node_db.entry(item_id.clone()).or_insert_with(|| {
            let mut css_classes = String::new();
            if !class_str.trim().is_empty() {
                css_classes.push_str(class_str.trim());
                css_classes.push(' ');
            }
            css_classes.push_str(CSS_DIAGRAM_STATE);

            let mut shape = SHAPE_STATE.to_string();
            if parsed_item.start == Some(true) {
                shape = SHAPE_START.to_string();
            } else if parsed_item.start == Some(false) {
                shape = SHAPE_END.to_string();
            }
            if parsed_item.ty != "default" {
                shape = parsed_item.ty.clone();
            }

            NodeScratch {
                id: item_id.clone(),
                shape,
                label: json!(sanitize_text(&item_id, config)),
                css_classes,
                css_styles: styles.clone(),
                node_type: None,
                dir: None,
                explicit_dir: None,
                is_group: false,
                parent_id: None,
                color_index: None,
            }
        });

        apply_state_descriptions(
            entry,
            &item_id,
            parsed_item.description.as_deref(),
            &parsed_item.descriptions,
            config,
        );

        // Group handling (composite states)
        if entry.node_type.is_none()
            && let Some(doc_id) = parsed_item.doc
        {
            let doc = &ctx.document.documents[doc_id.0];
            entry.node_type = Some("group".to_string());
            entry.is_group = true;
            let dir = get_dir_for_doc(doc, DEFAULT_NESTED_DOC_DIR);
            entry.dir = Some(dir);
            entry.explicit_dir = Some(
                doc.iter()
                    .any(|stmt| matches!(stmt, StateStatement::Direction(_))),
            );
            entry.shape = if parsed_item.ty == "divider" {
                SHAPE_DIVIDER.to_string()
            } else {
                SHAPE_GROUP.to_string()
            };
            let mut css = entry.css_classes.clone();
            css.push(' ');
            css.push_str(CSS_DIAGRAM_CLUSTER);
            if alt_flag {
                css.push(' ');
                css.push_str(CSS_DIAGRAM_CLUSTER_ALT);
            }
            entry.css_classes = css;
        }

        if let Some(p) = parent
            && p.id != "root"
        {
            entry.parent_id = Some(p.id.clone());
        }

        if needs_color_slot {
            let shape = entry.shape.clone();
            let _ = entry;
            let color_index = color_slot_for(
                &mut ctx.next_color_index,
                &mut ctx.container_color_index,
                &shape,
                &item_id,
                parent,
                user_styled,
            );
            entry = ctx
                .node_db
                .get_mut(&item_id)
                .expect("state node inserted before color assignment");
            entry.color_index = color_index;
        }

        let dom_id = state_dom_id(&item_id, *ctx.graph_item_count, None);
        let mut label = Some(entry.label.clone());
        if entry.shape == SHAPE_DIVIDER {
            label = Some(Value::String(String::new()));
        }

        let mut node = StateDiagramRenderNode {
            id: entry.id.clone(),
            label_style: String::new(),
            label,
            description: None,
            dom_id,
            is_group: entry.is_group,
            node_type: entry.node_type.clone(),
            parent_id: entry.parent_id.clone(),
            css_classes: entry.css_classes.clone(),
            css_compiled_styles: Vec::new(),
            css_styles: entry.css_styles.clone(),
            dir: entry.dir.clone(),
            explicit_dir: entry.explicit_dir,
            padding: Some(8.0),
            rx: Some(10.0),
            ry: Some(10.0),
            shape: entry.shape.clone(),
            position: None,
            color_index: entry.color_index,
            wrapping_width,
            min_width: (!entry.is_group).then_some(min_width).flatten(),
        };
        node.css_compiled_styles = compiled_styles(&node.css_classes, classes);

        // Notes create a note node + note group + note edge
        if let Some(mut n) = parsed_item.note.clone() {
            let flowchart_padding = config
                .as_value()
                .as_object()
                .and_then(|o| o.get("flowchart"))
                .and_then(|v| v.as_object())
                .and_then(|o| o.get("padding"))
                .and_then(value_as_f64);

            n.text = sanitize_text(&n.text, config);

            let note_id = format!("{item_id}{NOTE_ID}-{}", *ctx.graph_item_count);
            let parent_base_id = format!("{item_id}{PARENT_ID}");
            let parent_node_id = if ctx.node_index.contains_key(&parent_base_id)
                || ctx.states.contains_key(&parent_base_id)
            {
                format!("{parent_base_id}-{}", *ctx.graph_item_count)
            } else {
                parent_base_id
            };
            let note_dom_id = state_dom_id(&item_id, *ctx.graph_item_count, Some(NOTE));
            let group_dom_id = state_dom_id(&item_id, *ctx.graph_item_count, Some(PARENT));

            let mut group_node = StateDiagramRenderNode {
                id: parent_node_id.clone(),
                label_style: String::new(),
                label: Some(Value::String(n.text.clone())),
                description: None,
                dom_id: group_dom_id,
                is_group: true,
                node_type: Some("group".to_string()),
                parent_id: None,
                css_classes: node.css_classes.clone(),
                css_compiled_styles: Vec::new(),
                css_styles: Vec::new(),
                dir: None,
                explicit_dir: None,
                padding: Some(16.0),
                rx: None,
                ry: None,
                shape: SHAPE_NOTEGROUP.to_string(),
                position: n.position.clone(),
                color_index: None,
                wrapping_width,
                min_width: None,
            };
            group_node.css_compiled_styles = compiled_styles(&group_node.css_classes, classes);

            let mut note_node = StateDiagramRenderNode {
                id: note_id.clone(),
                label_style: String::new(),
                label: Some(Value::String(n.text.clone())),
                description: None,
                dom_id: note_dom_id,
                // Mermaid's generated note node is always a leaf, even when the
                // annotated state is a composite.
                is_group: false,
                node_type: Some("node".to_string()),
                parent_id: Some(parent_node_id.clone()),
                css_classes: CSS_DIAGRAM_NOTE.to_string(),
                css_compiled_styles: Vec::new(),
                css_styles: Vec::new(),
                dir: None,
                explicit_dir: None,
                padding: flowchart_padding,
                rx: None,
                ry: None,
                shape: SHAPE_NOTE.to_string(),
                position: n.position.clone(),
                color_index: None,
                wrapping_width,
                min_width,
            };
            note_node.css_compiled_styles = compiled_styles(&note_node.css_classes, classes);

            insert_generated_node_typed(
                ctx.nodes,
                ctx.node_index,
                ctx.states,
                group_node,
                "note group",
                &item_id,
            )?;
            insert_generated_node_typed(
                ctx.nodes,
                ctx.node_index,
                ctx.states,
                note_node,
                "note",
                &item_id,
            )?;
            upsert_node_typed(ctx.nodes, ctx.node_index, node.clone());

            let (mut from, mut to) = (item_id.clone(), note_id);
            if n.position.as_deref() == Some("left of") {
                std::mem::swap(&mut from, &mut to);
            }

            ctx.edges.push(StateDiagramRenderEdge {
                id: format!("{from}-{to}"),
                start: from,
                end: to,
                arrow_type_end: String::new(),
                classes: CSS_EDGE_NOTE_EDGE.to_string(),
                label: String::new(),
            });
            *ctx.graph_item_count += 1;
        } else {
            upsert_node_typed(ctx.nodes, ctx.node_index, node);
        }

        Ok(())
    }

    {
        let mut ctx = TypedLayoutContext {
            document,
            control,
            states,
            classes,
            config,
            nodes: &mut nodes,
            node_index: &mut node_index,
            edges: &mut edges,
            node_db: &mut node_db,
            graph_item_count: &mut graph_item_count,
            next_color_index: 0,
            container_color_index: HashMap::new(),
        };
        if let Some(root) = document.root {
            setup_doc(&mut ctx, None, root, false)?;
        }
    }

    // Post-process label arrays into (label, description) like Mermaid's StateDB.extract().
    for (index, node) in nodes.iter_mut().enumerate() {
        if index.is_multiple_of(128) {
            control.checkpoint()?;
        }
        let Some(label_val) = node.label.clone() else {
            continue;
        };
        let Some(arr) = label_val.as_array() else {
            continue;
        };
        if arr.is_empty() {
            continue;
        }
        let label0 = arr[0].clone();
        let rest: Vec<String> = arr
            .iter()
            .skip(1)
            .filter_map(|v| v.as_str().map(|s| s.to_string()))
            .collect();
        if node.is_group && !rest.is_empty() {
            return Err("Group nodes can only have label".to_string().into());
        }
        node.label = Some(label0);
        node.description = Some(rest);
    }

    Ok((nodes, edges))
}

fn state_stmt_ref_to_json(state: &StateStatementState) -> Value {
    let mut obj = Map::new();
    obj.insert("id".to_string(), Value::String(state.id.clone()));
    obj.insert("type".to_string(), Value::String(state.ty.clone()));
    obj.insert("classes".to_string(), string_array_value(&state.classes));
    Value::Object(obj)
}

pub(super) fn stmt_to_json_shallow(
    stmt: &StateStatement,
    doc: Option<crate::ManagedSemanticJson>,
) -> crate::ManagedSemanticJson {
    let value = match stmt {
        StateStatement::Noop => Value::Null,
        StateStatement::State(s) => {
            let mut obj = Map::new();
            obj.insert("stmt".to_string(), Value::String("state".to_string()));
            obj.insert("id".to_string(), Value::String(s.id.clone()));
            obj.insert("type".to_string(), Value::String(s.ty.clone()));
            obj.insert(
                "description".to_string(),
                option_string_value(&s.description),
            );
            obj.insert(
                "doc".to_string(),
                doc.map(crate::ManagedSemanticJson::into_unmanaged_value)
                    .unwrap_or(Value::Null),
            );
            obj.insert("classes".to_string(), string_array_value(&s.classes));
            Value::Object(obj)
        }
        StateStatement::Relation(relation) => {
            let mut obj = Map::new();
            obj.insert("stmt".to_string(), Value::String("relation".to_string()));
            obj.insert(
                "state1".to_string(),
                state_stmt_ref_to_json(&relation.state1),
            );
            obj.insert(
                "state2".to_string(),
                state_stmt_ref_to_json(&relation.state2),
            );
            obj.insert(
                "description".to_string(),
                option_string_value(&relation.description),
            );
            Value::Object(obj)
        }
        StateStatement::ClassDef { id, classes } => {
            json!({ "stmt": "classDef", "id": id, "classes": classes })
        }
        StateStatement::ApplyClass { ids, class_name } => {
            json!({ "stmt": "applyClass", "id": ids, "styleClass": class_name })
        }
        StateStatement::Style { ids, styles } => {
            json!({ "stmt": "style", "id": ids, "styleClass": styles })
        }
        StateStatement::Direction(v) => json!({ "stmt": "dir", "value": v }),
        StateStatement::AccTitle(t) => json!(t),
        StateStatement::AccDescr(d) => json!(d),
        StateStatement::Click(c) => {
            json!({ "stmt": "click", "id": c.id, "url": c.url, "tooltip": c.tooltip })
        }
    };
    value.into()
}

fn normalize_multiline_ws(input: &str) -> String {
    let trimmed = input.trim_matches(crate::diagrams::scan::is_ecmascript_whitespace);
    let mut out = String::with_capacity(trimmed.len());
    let mut chars = trimmed.chars().peekable();
    while let Some(ch) = chars.next() {
        out.push(ch);
        if ch == '\n' {
            while chars
                .peek()
                .is_some_and(|c| crate::diagrams::scan::is_ecmascript_whitespace(*c))
            {
                chars.next();
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use crate::diagrams::state::parse_state_model_for_render_controlled;
    use crate::{Engine, MermaidConfig, OperationControl, ParseMetadata, ParseOptions};
    use serde_json::json;

    fn meta(config: serde_json::Value) -> ParseMetadata {
        let config = MermaidConfig::from_value(config);
        ParseMetadata {
            diagram_type: "stateDiagram".to_string(),
            config: config.clone(),
            effective_config: config,
            title: None,
        }
    }

    #[test]
    fn state_render_nodes_carry_mermaid12_width_fields() {
        let metadata = meta(json!({
            "state": { "wrappingWidth": 144, "minNodeWidth": 132 }
        }));
        let model = parse_state_model_for_render_controlled(
            "stateDiagram-v2\nIdle --> Active\nnote right of Idle : state note\n",
            &metadata,
            &OperationControl::new(),
        )
        .expect("state parse control")
        .expect("state render model");
        let idle = model
            .nodes
            .iter()
            .find(|node| node.id == "Idle")
            .expect("Idle node");
        assert_eq!(idle.wrapping_width, Some(144.0));
        assert_eq!(idle.min_width, Some(132.0));
        assert_eq!(idle.color_index, None);
        let note = model
            .nodes
            .iter()
            .find(|node| node.shape == "note")
            .expect("generated note node");
        assert!(!note.is_group);
        assert_eq!(note.node_type.as_deref(), Some("node"));
        assert_eq!(note.min_width, Some(132.0));

        let compatibility = crate::diagrams::state::render_model_to_compat_json(&model, &metadata)
            .expect("state compatibility projection");
        assert_eq!(compatibility["nodes"][0]["wrappingWidth"], json!(144));
        assert_eq!(compatibility["nodes"][0]["minWidth"], json!(132));
        assert!(compatibility["nodes"][0].get("colorIndex").is_some());
    }

    #[test]
    fn state_container_color_slots_follow_mermaid_depth_first_order() {
        let metadata = meta(json!({}));
        let model = parse_state_model_for_render_controlled(
            "stateDiagram-v2\nstate A {\n  [*] --> A1\n}\nstate B {\n  [*] --> B1\n}\n",
            &metadata,
            &OperationControl::new(),
        )
        .expect("state parse control")
        .expect("state render model");
        let a = model
            .nodes
            .iter()
            .find(|node| node.id == "A")
            .expect("A group");
        let b = model
            .nodes
            .iter()
            .find(|node| node.id == "B")
            .expect("B group");
        assert_eq!(a.color_index, Some(0));
        assert_eq!(b.color_index, Some(1));
    }

    #[test]
    fn state_note_generated_id_collision_is_rejected_independent_of_declaration_order() {
        let engine = Engine::new();
        let authored_state = "state \"Authored\" as A----note-0";
        let positioned_note = "note right of A : Generated note";

        for body in [
            format!("{authored_state}\n{positioned_note}"),
            format!("{positioned_note}\n{authored_state}"),
        ] {
            let input = format!("stateDiagram-v2\n{body}\n");
            let error = engine
                .parse_diagram_for_render_model_sync(&input, ParseOptions::strict())
                .expect_err("authored and generated state ids must not silently collide");
            let message = error.to_string();

            assert!(
                message.contains(
                    "Authored state id `A----note-0` conflicts with generated note id for state `A`"
                ),
                "unexpected collision error: {message}"
            );
        }
    }
}
