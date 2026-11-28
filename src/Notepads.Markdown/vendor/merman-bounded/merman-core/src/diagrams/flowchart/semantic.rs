// Notepads Markdown modifications: Copyright (c) 2026, Jiaqi (0x7c13) Liu.
// Original upstream MIT/Apache-2.0 licenses and notices are preserved.
use crate::diagram::{DiagramWarningFact, FLOWCHART_UNKNOWN_STYLE_TARGET_WARNING_RULE_ID};
use crate::native_collections::OrderedMap as IndexMap;
use crate::native_collections::{
    self as memory, NativeHashMap as HashMap, NativeHashSet as HashSet,
};
use crate::sanitize::sanitize_text;
use crate::utils::format_url;
use crate::{Error, MermaidConfig, OperationControl, OperationControlResult, Result};

use super::{
    ClickAction, Edge, EdgeDefaults, FlowNodeProvenance, FlowNodeSyntax, FlowSubGraph,
    FlowSubgraphVertexStyle, FlowchartAst, FlowchartRenderStyleSources, LinkStylePos, Node,
    StatementEvent, Stmt, TitleKind, apply_shape_data_value_to_node, value_to_bool,
    value_to_string,
};

pub(super) struct FlowchartSemanticContext<'a> {
    pub(super) nodes: &'a mut Vec<Node>,
    pub(super) node_index: &'a mut HashMap<String, usize>,
    pub(super) edges: &'a mut Vec<Edge>,
    pub(super) subgraphs: &'a mut Vec<FlowSubGraph>,
    pub(super) subgraph_declaration_owners: &'a [usize],
    pub(super) subgraph_vertex_styles: &'a mut FlowchartRenderStyleSources,
    pub(super) collapsed_subgraphs: &'a mut HashSet<String>,
    pub(super) vertex_calls: &'a mut Vec<String>,
    pub(super) warning_facts: &'a mut Vec<DiagramWarningFact>,
    pub(super) class_defs: &'a mut IndexMap<String, Vec<String>>,
    pub(super) tooltips: &'a mut HashMap<String, String>,
    pub(super) edge_defaults: &'a mut EdgeDefaults,
    pub(super) security_level_loose: bool,
    pub(super) diagram_type: &'a str,
    pub(super) config: Option<&'a MermaidConfig>,
    pub(super) shape_data_documents:
        &'a HashMap<String, std::result::Result<serde_json::Value, String>>,
    #[cfg(feature = "diagram-flowchart")]
    pub(super) native_shape_documents: Option<&'a HashMap<String, super::native::FlatShapeData>>,
    pub(super) control: &'a OperationControl,
}

pub(super) fn apply_semantic_statements(
    ast: &FlowchartAst,
    ctx: &mut FlowchartSemanticContext<'_>,
) -> OperationControlResult<Result<()>> {
    ctx.apply_statements(ast)
}

impl<'a> FlowchartSemanticContext<'a> {
    fn replay_error(&self, message: std::fmt::Arguments<'_>) -> OperationControlResult<Error> {
        Ok(Error::diagram_parse_fallback(
            memory::owned_string(self.diagram_type, self.control)?,
            memory::format_owned(message, self.control)?,
        ))
    }

    fn apply_statements(&mut self, ast: &FlowchartAst) -> OperationControlResult<Result<()>> {
        let mut active_subgraphs = memory::new_map(self.control);
        let mut seen_vertex_ids = memory::new_set(self.control);
        let mut vertex_css: HashMap<String, FlowSubgraphVertexStyle> =
            memory::new_map(self.control);
        let mut seen_edge_indices: HashMap<String, Vec<usize>> = memory::new_map(self.control);
        let mut next_built_edge_index = 0usize;
        let mut next_built_subgraph_index = 0usize;
        for (visited, event) in ast.walk_controlled(self.control)?.enumerate() {
            let event = event?;
            if visited.is_multiple_of(128) {
                self.control.checkpoint()?;
            }
            let StatementEvent::Enter(stmt) = event else {
                let owner = self
                    .subgraph_declaration_owners
                    .get(next_built_subgraph_index)
                    .copied();
                let Some((owner, subgraph)) = owner
                    .and_then(|owner| self.subgraphs.get(owner).map(|subgraph| (owner, subgraph)))
                else {
                    return Ok(Err(self.replay_error(format_args!(
                        "flowchart subgraph replay diverged from the built model"
                    ))?));
                };
                memory::insert(
                    &mut active_subgraphs,
                    memory::clone_owned(&subgraph.id, self.control)?,
                    owner,
                    self.control,
                )?;
                next_built_subgraph_index = next_built_subgraph_index.saturating_add(1);
                continue;
            };

            match stmt {
                Stmt::Subgraph(_) => {}
                Stmt::Style(s) => {
                    if seen_edge_indices.contains_key(&s.target) {
                        continue;
                    }
                    memory::push(
                        self.vertex_calls,
                        memory::clone_owned(&s.target, self.control)?,
                        self.control,
                    )?;
                    let is_new_vertex = memory::insert_set(
                        &mut seen_vertex_ids,
                        memory::clone_owned(&s.target, self.control)?,
                        self.control,
                    )?;
                    let style = memory::entry_default(
                        &mut vertex_css,
                        memory::clone_owned(&s.target, self.control)?,
                        self.control,
                    )?;
                    memory::extend_cloned(&mut style.styles, &s.styles, self.control)?;
                    if !active_subgraphs.contains_key(&s.target) {
                        if is_new_vertex {
                            let mut warning = DiagramWarningFact::new(
                                memory::owned_string(
                                    FLOWCHART_UNKNOWN_STYLE_TARGET_WARNING_RULE_ID,
                                    self.control,
                                )?,
                                memory::format_owned(
                                    format_args!(
                                        "Style applied to unknown node \"{}\". This may indicate a typo. The node will be created automatically.",
                                        s.target
                                    ),
                                    self.control,
                                )?,
                            );
                            if let Some(span) = s.target_span {
                                warning = warning.with_span(span);
                            }
                            memory::push(self.warning_facts, warning, self.control)?;
                        }
                        let idx = self.ensure_authored_node(&s.target)?;
                        memory::extend_cloned(
                            &mut self.nodes[idx].styles,
                            &s.styles,
                            self.control,
                        )?;
                    }
                }
                Stmt::ClassDef(c) => {
                    for (index, id) in c.ids.iter().enumerate() {
                        if index % 128 == 0 {
                            self.control.checkpoint()?;
                        }
                        let id = memory::clone_owned(id, self.control)?;
                        let styles = memory::result(
                            self.class_defs.try_entry_or_default(id, self.control),
                            self.control,
                        )?;
                        memory::extend_cloned(styles, &c.styles, self.control)?;
                    }
                }
                Stmt::ClassAssign(c) => {
                    for (index, target) in c.targets.iter().enumerate() {
                        if index % 128 == 0 {
                            self.control.checkpoint()?;
                        }
                        if let Some(style) = vertex_css.get_mut(target) {
                            memory::push(
                                &mut style.classes,
                                memory::clone_owned(&c.class_name, self.control)?,
                                self.control,
                            )?;
                        }
                        self.add_class_to_target(
                            target,
                            &c.class_name,
                            &active_subgraphs,
                            &seen_vertex_ids,
                            &seen_edge_indices,
                        )?;
                    }
                }
                Stmt::Click(c) => {
                    let Some(config) = self.config else {
                        return Ok(Err(memory::unsupported_error("click", self.control)?));
                    };
                    for (index, id) in c.ids.iter().enumerate() {
                        if index % 128 == 0 {
                            self.control.checkpoint()?;
                        }
                        if let Some(tt) = &c.tooltip {
                            self.tooltips.insert(id.clone(), sanitize_text(tt, config));
                        }
                        if let Some(style) = vertex_css.get_mut(id) {
                            memory::push(
                                &mut style.classes,
                                memory::owned_string("clickable", self.control)?,
                                self.control,
                            )?;
                        }
                        self.add_class_to_target(
                            id,
                            "clickable",
                            &active_subgraphs,
                            &seen_vertex_ids,
                            &seen_edge_indices,
                        )?;

                        match &c.action {
                            ClickAction::Link { href, target } => {
                                if seen_vertex_ids.contains(id)
                                    && let Some(&idx) = self.node_index.get(id)
                                {
                                    self.nodes[idx].link = format_url(href, config);
                                    self.nodes[idx].link_target =
                                        memory::clone_owned(target, self.control)?;
                                }
                            }
                            ClickAction::Callback => {
                                if self.security_level_loose
                                    && seen_vertex_ids.contains(id)
                                    && let Some(&idx) = self.node_index.get(id)
                                {
                                    self.nodes[idx].have_callback = true;
                                }
                            }
                        }
                    }
                }
                Stmt::LinkStyle(ls) => {
                    if let Some(algo) = &ls.interpolate {
                        for (index, pos) in ls.positions.iter().enumerate() {
                            if index % 128 == 0 {
                                self.control.checkpoint()?;
                            }
                            match pos {
                                LinkStylePos::Default => {
                                    self.edge_defaults.interpolate =
                                        Some(memory::clone_owned(algo, self.control)?)
                                }
                                LinkStylePos::Index(i) => {
                                    if *i >= next_built_edge_index {
                                        return Ok(Err(self.replay_error(format_args!(
                                                "The index {i} for linkStyle is out of bounds. Valid indices for linkStyle are between 0 and {}. (Help: Ensure that the index is within the range of existing edges.)",
                                                next_built_edge_index.saturating_sub(1)
                                            ))?));
                                    }
                                    self.edges[*i].interpolate =
                                        Some(memory::clone_owned(algo, self.control)?);
                                }
                            }
                        }
                    }

                    if !ls.styles.is_empty() {
                        for (index, pos) in ls.positions.iter().enumerate() {
                            if index % 128 == 0 {
                                self.control.checkpoint()?;
                            }
                            match pos {
                                LinkStylePos::Default => {
                                    self.edge_defaults.style =
                                        memory::clone_owned(&ls.styles, self.control)?
                                }
                                LinkStylePos::Index(i) => {
                                    if *i >= next_built_edge_index {
                                        return Ok(Err(self.replay_error(format_args!(
                                                "The index {i} for linkStyle is out of bounds. Valid indices for linkStyle are between 0 and {}. (Help: Ensure that the index is within the range of existing edges.)",
                                                next_built_edge_index.saturating_sub(1)
                                            ))?));
                                    }
                                    self.edges[*i].style =
                                        memory::clone_owned(&ls.styles, self.control)?;
                                    if !self.edges[*i].style.is_empty()
                                        && !self.edges[*i]
                                            .style
                                            .iter()
                                            .any(|s| s.trim_start().starts_with("fill"))
                                    {
                                        memory::push(
                                            &mut self.edges[*i].style,
                                            memory::owned_string("fill:none", self.control)?,
                                            self.control,
                                        )?;
                                    }
                                }
                            }
                        }
                    }
                }
                Stmt::ShapeData { target, yaml, .. } => {
                    #[cfg(feature = "diagram-flowchart")]
                    if let Some(documents) = self.native_shape_documents {
                        if active_subgraphs.contains_key(target)
                            || seen_edge_indices.contains_key(target)
                        {
                            return Ok(Err(memory::unsupported_error("metadata", self.control)?));
                        }
                        let Some(document) = documents.get(yaml) else {
                            return super::native::core_result(
                                Err(super::native::NativeParseError::Model),
                                self.control,
                            );
                        };
                        memory::push(
                            self.vertex_calls,
                            memory::clone_owned(target, self.control)?,
                            self.control,
                        )?;
                        memory::push(
                            self.vertex_calls,
                            memory::clone_owned(target, self.control)?,
                            self.control,
                        )?;
                        memory::insert_set(
                            &mut seen_vertex_ids,
                            memory::clone_owned(target, self.control)?,
                            self.control,
                        )?;
                        memory::entry_default(
                            &mut vertex_css,
                            memory::clone_owned(target, self.control)?,
                            self.control,
                        )?;
                        let index = self.ensure_authored_node(target)?;
                        if let Err(error) = super::native::core_result(
                            document.apply_node(&mut self.nodes[index], self.control),
                            self.control,
                        )? {
                            return Ok(Err(error));
                        }
                        continue;
                    }
                    let value = match Self::shape_data_value(
                        self.shape_data_documents,
                        self.diagram_type,
                        yaml,
                    ) {
                        Ok(value) => value,
                        Err(error) => return Ok(Err(error)),
                    };
                    // Mermaid checks subgraph metadata before edge metadata. This matters when
                    // an id is shared by a subgraph and a user-defined edge.
                    let routed_to_subgraph =
                        active_subgraphs.get(target).is_some_and(|&subgraph_index| {
                            self.apply_shape_data_to_subgraph(subgraph_index, target, value)
                        });
                    if routed_to_subgraph {
                        // The parser first calls addVertex for the bare reference and then
                        // calls it again with shapeData. A subgraph metadata call returns from
                        // the second call, so retain only the preceding vertex call unless the
                        // id already names an edge (where addVertex returns immediately).
                        if !seen_edge_indices.contains_key(target) {
                            memory::push(
                                self.vertex_calls,
                                memory::clone_owned(target, self.control)?,
                                self.control,
                            )?;
                            memory::insert_set(
                                &mut seen_vertex_ids,
                                memory::clone_owned(target, self.control)?,
                                self.control,
                            )?;
                            memory::entry_default(
                                &mut vertex_css,
                                memory::clone_owned(target, self.control)?,
                                self.control,
                            )?;
                        }
                        continue;
                    }

                    if let Some(indices) = seen_edge_indices.get(target) {
                        Self::apply_shape_data_to_edges(self.edges, self.control, indices, value)?;
                        continue;
                    }

                    // The Jison grammar calls addVertex once for the node and once for its
                    // shapeData. Preserve both calls because Mermaid derives DOM ids from this
                    // sequence even though they update the same typed node.
                    memory::push(
                        self.vertex_calls,
                        memory::clone_owned(target, self.control)?,
                        self.control,
                    )?;
                    memory::push(
                        self.vertex_calls,
                        memory::clone_owned(target, self.control)?,
                        self.control,
                    )?;
                    memory::insert_set(
                        &mut seen_vertex_ids,
                        memory::clone_owned(target, self.control)?,
                        self.control,
                    )?;
                    memory::entry_default(
                        &mut vertex_css,
                        memory::clone_owned(target, self.control)?,
                        self.control,
                    )?;
                    let idx = self.ensure_authored_node(target)?;
                    if let Err(error) = apply_shape_data_value_to_node(&mut self.nodes[idx], value)
                    {
                        return Ok(Err(Error::diagram_parse_fallback(
                            self.diagram_type.to_string(),
                            error,
                        )));
                    }
                }
                Stmt::Chain {
                    node_groups,
                    edge_groups,
                } => {
                    if let Some(first_group) = node_groups.first()
                        && let Err(error) = self.observe_node_group(
                            first_group,
                            &active_subgraphs,
                            &mut seen_vertex_ids,
                            &mut vertex_css,
                            &seen_edge_indices,
                        )?
                    {
                        return Ok(Err(error));
                    }
                    for (segment_index, edge_group) in edge_groups.iter().enumerate() {
                        if let Some(next_group) = node_groups.get(segment_index + 1)
                            && let Err(error) = self.observe_node_group(
                                next_group,
                                &active_subgraphs,
                                &mut seen_vertex_ids,
                                &mut vertex_css,
                                &seen_edge_indices,
                            )?
                        {
                            return Ok(Err(error));
                        }
                        if let Err(error) = self.activate_edges(
                            edge_group.len(),
                            &mut next_built_edge_index,
                            &mut seen_edge_indices,
                        )? {
                            return Ok(Err(error));
                        }
                    }
                }
                Stmt::Node(node) => {
                    if let Err(error) = self.observe_node_group(
                        std::slice::from_ref(node.as_ref()),
                        &active_subgraphs,
                        &mut seen_vertex_ids,
                        &mut vertex_css,
                        &seen_edge_indices,
                    )? {
                        return Ok(Err(error));
                    }
                }
                Stmt::Direction(_) => {}
            }
        }

        if next_built_edge_index != self.edges.len()
            || next_built_subgraph_index != self.subgraph_declaration_owners.len()
        {
            return Ok(Err(self.replay_error(format_args!(
                "flowchart semantic replay did not consume the built model"
            ))?));
        }
        for (index, (id, style)) in vertex_css.into_iter().enumerate() {
            if index % 128 == 0 {
                self.control.checkpoint()?;
            }
            if let Some(&declaration_ordinal) = active_subgraphs.get(&id) {
                // Every completed declaration with this id resolves to the same canonical
                // FlowDB group, including CSS authored between repeated declarations.
                self.subgraph_vertex_styles
                    .insert(id, declaration_ordinal, style, self.control)?;
            }
        }
        self.control.checkpoint()?;
        Ok(Ok(()))
    }

    fn observe_node_group(
        &mut self,
        nodes: &[Node],
        active_subgraphs: &HashMap<String, usize>,
        seen_vertex_ids: &mut HashSet<String>,
        vertex_css: &mut HashMap<String, FlowSubgraphVertexStyle>,
        seen_edge_indices: &HashMap<String, Vec<usize>>,
    ) -> OperationControlResult<Result<()>> {
        #[cfg(feature = "diagram-flowchart")]
        if self.native_shape_documents.is_some() {
            return self.observe_native_node_group(
                nodes,
                active_subgraphs,
                seen_vertex_ids,
                vertex_css,
                seen_edge_indices,
            );
        }
        let mut deferred_shape_data_calls = Vec::new();
        for (index, node) in nodes.iter().enumerate() {
            if index % 128 == 0 {
                self.control.checkpoint()?;
            }
            if let Some(yaml) = node.shape_data.as_deref()
                && let Some(&subgraph_index) = active_subgraphs.get(&node.id)
            {
                let value = match Self::shape_data_value(
                    self.shape_data_documents,
                    self.diagram_type,
                    yaml,
                ) {
                    Ok(value) => value,
                    Err(error) => return Ok(Err(error)),
                };
                if self.apply_shape_data_to_subgraph(subgraph_index, &node.id, value) {
                    // The node production has already emitted one bare addVertex call before
                    // its shapeData action. Preserve that call for DOM-id sequencing, while
                    // avoiding it when an existing edge makes addVertex return early.
                    if !seen_edge_indices.contains_key(&node.id) {
                        memory::push(
                            self.vertex_calls,
                            memory::clone_owned(&node.id, self.control)?,
                            self.control,
                        )?;
                        memory::insert_set(
                            seen_vertex_ids,
                            memory::clone_owned(&node.id, self.control)?,
                            self.control,
                        )?;
                        let style = memory::entry_default(
                            vertex_css,
                            memory::clone_owned(&node.id, self.control)?,
                            self.control,
                        )?;
                        memory::extend_cloned(&mut style.classes, &node.classes, self.control)?;
                        memory::extend_cloned(&mut style.styles, &node.styles, self.control)?;
                    }
                    continue;
                }
            }

            if let Some(indices) = seen_edge_indices.get(&node.id) {
                if let Some(yaml) = node.shape_data.as_deref() {
                    let value = match Self::shape_data_value(
                        self.shape_data_documents,
                        self.diagram_type,
                        yaml,
                    ) {
                        Ok(value) => value,
                        Err(error) => return Ok(Err(error)),
                    };
                    Self::apply_shape_data_to_edges(self.edges, self.control, indices, value)?;
                }
                continue;
            }

            memory::push(
                self.vertex_calls,
                memory::clone_owned(&node.id, self.control)?,
                self.control,
            )?;
            memory::insert_set(
                seen_vertex_ids,
                memory::clone_owned(&node.id, self.control)?,
                self.control,
            )?;
            let style = memory::entry_default(
                vertex_css,
                memory::clone_owned(&node.id, self.control)?,
                self.control,
            )?;
            memory::extend_cloned(&mut style.classes, &node.classes, self.control)?;
            memory::extend_cloned(&mut style.styles, &node.styles, self.control)?;
            if let Some(yaml) = node.shape_data.as_deref() {
                memory::push(
                    &mut deferred_shape_data_calls,
                    memory::clone_owned(&node.id, self.control)?,
                    self.control,
                )?;
                let value = match Self::shape_data_value(
                    self.shape_data_documents,
                    self.diagram_type,
                    yaml,
                ) {
                    Ok(value) => value,
                    Err(error) => return Ok(Err(error)),
                };
                let idx = self.ensure_authored_node(&node.id)?;
                if let Err(error) = apply_shape_data_value_to_node(&mut self.nodes[idx], value) {
                    return Ok(Err(Error::diagram_parse_fallback(
                        self.diagram_type.to_string(),
                        error,
                    )));
                }
            }
        }
        for (index, id) in deferred_shape_data_calls.into_iter().enumerate() {
            if index % 128 == 0 {
                self.control.checkpoint()?;
            }
            memory::push(self.vertex_calls, id, self.control)?;
        }
        Ok(Ok(()))
    }

    #[cfg(feature = "diagram-flowchart")]
    fn observe_native_node_group(
        &mut self,
        nodes: &[Node],
        active_subgraphs: &HashMap<String, usize>,
        seen_vertex_ids: &mut HashSet<String>,
        vertex_css: &mut HashMap<String, FlowSubgraphVertexStyle>,
        seen_edge_indices: &HashMap<String, Vec<usize>>,
    ) -> OperationControlResult<Result<()>> {
        let Some(documents) = self.native_shape_documents else {
            return super::native::core_result(
                Err(super::native::NativeParseError::Model),
                self.control,
            );
        };
        let mut deferred = Vec::new();
        for (ordinal, node) in nodes.iter().enumerate() {
            if ordinal.is_multiple_of(128) {
                self.control.checkpoint()?;
            }
            if seen_edge_indices.contains_key(&node.id) {
                if node.shape_data.is_some() {
                    return Ok(Err(memory::unsupported_error("metadata", self.control)?));
                }
                continue;
            }
            if active_subgraphs.contains_key(&node.id) && node.shape_data.is_some() {
                return Ok(Err(memory::unsupported_error("metadata", self.control)?));
            }
            memory::push(
                self.vertex_calls,
                memory::clone_owned(&node.id, self.control)?,
                self.control,
            )?;
            memory::insert_set(
                seen_vertex_ids,
                memory::clone_owned(&node.id, self.control)?,
                self.control,
            )?;
            let style = memory::entry_default(
                vertex_css,
                memory::clone_owned(&node.id, self.control)?,
                self.control,
            )?;
            memory::extend_cloned(&mut style.classes, &node.classes, self.control)?;
            memory::extend_cloned(&mut style.styles, &node.styles, self.control)?;
            let Some(&index) = self.node_index.get(&node.id) else {
                return super::native::core_result(
                    Err(super::native::NativeParseError::Model),
                    self.control,
                );
            };
            // Replay authored values in declaration order. A bare reference
            // does not overwrite a modern label/shape from an earlier call.
            if let Some(label) = &node.label {
                self.nodes[index].label = Some(memory::clone_owned(label, self.control)?);
                self.nodes[index].label_type = node.label_type.clone();
                self.nodes[index].label_span = node.label_span;
                self.nodes[index].label_selection = node.label_selection;
            }
            if let Some(shape) = &node.shape {
                self.nodes[index].shape = Some(memory::clone_owned(shape, self.control)?);
            }
            if let Some(raw) = &node.shape_data {
                let Some(document) = documents.get(raw) else {
                    return super::native::core_result(
                        Err(super::native::NativeParseError::Model),
                        self.control,
                    );
                };
                if let Err(error) = super::native::core_result(
                    document.apply_node(&mut self.nodes[index], self.control),
                    self.control,
                )? {
                    return Ok(Err(error));
                }
                memory::push(
                    &mut deferred,
                    memory::clone_owned(&node.id, self.control)?,
                    self.control,
                )?;
            }
        }
        for id in deferred {
            memory::push(self.vertex_calls, id, self.control)?;
        }
        Ok(Ok(()))
    }

    fn add_class_to_target(
        &mut self,
        target: &str,
        class_name: &str,
        active_subgraphs: &HashMap<String, usize>,
        seen_vertex_ids: &HashSet<String>,
        seen_edge_indices: &HashMap<String, Vec<usize>>,
    ) -> OperationControlResult<()> {
        if let Some(&idx) = active_subgraphs.get(target) {
            memory::push(
                &mut self.subgraphs[idx].classes,
                memory::owned_string(class_name, self.control)?,
                self.control,
            )?;
        }
        if seen_vertex_ids.contains(target)
            && let Some(&idx) = self.node_index.get(target)
        {
            memory::push(
                &mut self.nodes[idx].classes,
                memory::owned_string(class_name, self.control)?,
                self.control,
            )?;
        }
        if let Some(edge_indices) = seen_edge_indices.get(target) {
            for (index, edge_index) in edge_indices.iter().copied().enumerate() {
                if index % 128 == 0 {
                    self.control.checkpoint()?;
                }
                if let Some(edge) = self.edges.get_mut(edge_index) {
                    memory::push(
                        &mut edge.classes,
                        memory::owned_string(class_name, self.control)?,
                        self.control,
                    )?;
                }
            }
        } else {
            // Preserve the canonical cancellation cadence even when the requested edge id is
            // absent. The previous implementation scanned every built edge in this case.
            for index in 0..self.edges.len() {
                if index % 128 == 0 {
                    self.control.checkpoint()?;
                }
            }
        }
        Ok(())
    }

    fn activate_edges(
        &self,
        count: usize,
        next_edge_index: &mut usize,
        seen_edge_indices: &mut HashMap<String, Vec<usize>>,
    ) -> OperationControlResult<Result<()>> {
        let Some(edge_end) = next_edge_index.checked_add(count) else {
            return Ok(Err(self.replay_error(format_args!(
                "flowchart edge replay index overflow"
            ))?));
        };
        if edge_end > self.edges.len() {
            return Ok(Err(self.replay_error(format_args!(
                "flowchart edge replay diverged from the built model"
            ))?));
        }
        for (index, edge_index) in (*next_edge_index..edge_end).enumerate() {
            if index % 128 == 0 {
                self.control.checkpoint()?;
            }
            if let Some(id) = self.edges[edge_index].id.as_deref() {
                let indices = memory::entry_default(
                    seen_edge_indices,
                    memory::owned_string(id, self.control)?,
                    self.control,
                )?;
                memory::push(indices, edge_index, self.control)?;
            }
        }
        *next_edge_index = edge_end;
        Ok(Ok(()))
    }

    fn shape_data_value<'b>(
        documents: &'b HashMap<String, std::result::Result<serde_json::Value, String>>,
        diagram_type: &str,
        yaml: &str,
    ) -> Result<&'b serde_json::Value> {
        match documents
            .get(yaml)
            .expect("flowchart shape data must be prepared before semantic construction")
        {
            Ok(document) => Ok(document),
            Err(error) => Err(Error::diagram_parse_fallback(
                diagram_type.to_string(),
                format!("Invalid shapeData: {error}"),
            )),
        }
    }

    fn apply_shape_data_to_subgraph(
        &mut self,
        subgraph_index: usize,
        target: &str,
        value: &serde_json::Value,
    ) -> bool {
        let Some(map) = value.as_object() else {
            return false;
        };
        let Some(subgraph) = self.subgraphs.get_mut(subgraph_index) else {
            return false;
        };
        if subgraph.id != target {
            return false;
        }

        let metadata = subgraph
            .metadata
            .get_or_insert_with(|| serde_json::Value::Object(serde_json::Map::new()));
        let Some(metadata_map) = metadata.as_object_mut() else {
            *metadata = serde_json::Value::Object(serde_json::Map::new());
            unreachable!("subgraph metadata was replaced with an object");
        };
        for (key, item) in map {
            metadata_map.insert(key.clone(), item.clone());
        }

        if map.contains_key("view") {
            if metadata_map.get("view").and_then(serde_json::Value::as_str) == Some("collapsed") {
                self.collapsed_subgraphs.insert(target.to_string());
            } else {
                self.collapsed_subgraphs.remove(target);
            }
        }
        true
    }

    fn apply_shape_data_to_edges(
        edges: &mut [Edge],
        control: &OperationControl,
        indices: &[usize],
        value: &serde_json::Value,
    ) -> OperationControlResult<()> {
        let Some(map) = value.as_object() else {
            return Ok(());
        };
        for (index, edge_index) in indices.iter().copied().enumerate() {
            if index % 128 == 0 {
                control.checkpoint()?;
            }
            let Some(edge) = edges.get_mut(edge_index) else {
                continue;
            };
            for (key, value) in map {
                match key.as_str() {
                    "animate" => {
                        if let Some(value) = value_to_bool(value) {
                            edge.animate = Some(value);
                        }
                    }
                    "animation" => {
                        if let Some(value) = value_to_string(value) {
                            edge.animation = Some(value);
                        }
                    }
                    "curve" => {
                        if let Some(value) = value_to_string(value) {
                            edge.interpolate = Some(value);
                        }
                    }
                    _ => {}
                }
            }
        }
        Ok(())
    }

    fn ensure_authored_node(&mut self, id: &str) -> OperationControlResult<usize> {
        #[cfg(feature = "diagram-flowchart")]
        if self.native_shape_documents.is_some() {
            if id.len() > 128 {
                return Err(memory::failure(
                    self.control,
                    markdown_memory::MemoryError::BudgetLimited,
                ));
            }
            let already_authored = self
                .node_index
                .get(id)
                .is_some_and(|index| self.nodes[*index].provenance == FlowNodeProvenance::Authored);
            if !already_authored {
                if let Some(scope) = self.control.memory_scope() {
                    memory::result(scope.charge_work(self.nodes.len()), self.control)?;
                }
                if self
                    .nodes
                    .iter()
                    .filter(|node| node.provenance == FlowNodeProvenance::Authored)
                    .count()
                    >= 128
                {
                    return Err(memory::failure(
                        self.control,
                        markdown_memory::MemoryError::BudgetLimited,
                    ));
                }
            }
        }
        if let Some(&idx) = self.node_index.get(id) {
            self.nodes[idx].provenance = FlowNodeProvenance::Authored;
            self.nodes[idx].syntax = FlowNodeSyntax::ExplicitDefinition;
            return Ok(idx);
        }
        let idx = self.nodes.len();
        memory::push(
            self.nodes,
            Node {
                id: memory::owned_string(id, self.control)?,
                provenance: FlowNodeProvenance::Authored,
                syntax: FlowNodeSyntax::ExplicitDefinition,
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
            },
            self.control,
        )?;
        memory::insert(
            self.node_index,
            memory::owned_string(id, self.control)?,
            idx,
            self.control,
        )?;
        Ok(idx)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::diagrams::flowchart::{
        ClassAssignStmt, FlowEdgeMarker, FlowEdgeStroke, FlowEdgeVisibility, LinkToken,
    };

    #[test]
    fn class_assignment_can_cancel_during_edge_scanning() {
        let mut nodes = Vec::new();
        let mut node_index = HashMap::default();
        let mut edges = (0..512)
            .map(|index| Edge {
                from: format!("n{index}"),
                to: format!("n{}", index + 1),
                id: Some(format!("edge-{index}")),
                link: LinkToken {
                    end: "arrow_point".to_string(),
                    start_marker: FlowEdgeMarker::None,
                    end_marker: FlowEdgeMarker::Point,
                    stroke_kind: FlowEdgeStroke::Normal,
                    visibility: FlowEdgeVisibility::Visible,
                    length: 1,
                },
                label: None,
                label_type: TitleKind::Text,
                label_span: None,
                label_selection: None,
                style: Vec::new(),
                classes: Vec::new(),
                interpolate: None,
                is_user_defined_id: true,
                animate: None,
                animation: None,
            })
            .collect::<Vec<_>>();
        let mut subgraphs = Vec::new();
        let mut subgraph_vertex_styles = FlowchartRenderStyleSources::default();
        let mut vertex_calls = Vec::new();
        let mut warning_facts = Vec::new();
        let mut class_defs = IndexMap::new();
        let mut tooltips = HashMap::default();
        let mut edge_defaults = EdgeDefaults {
            style: Vec::new(),
            interpolate: None,
        };
        let config = MermaidConfig::empty_object();
        let shape_data_documents = HashMap::default();
        let control = OperationControl::new();
        control.cancel_after_checkpoints(3);
        let mut context = FlowchartSemanticContext {
            nodes: &mut nodes,
            node_index: &mut node_index,
            edges: &mut edges,
            subgraphs: &mut subgraphs,
            subgraph_declaration_owners: &[],
            subgraph_vertex_styles: &mut subgraph_vertex_styles,
            collapsed_subgraphs: &mut HashSet::default(),
            vertex_calls: &mut vertex_calls,
            warning_facts: &mut warning_facts,
            class_defs: &mut class_defs,
            tooltips: &mut tooltips,
            edge_defaults: &mut edge_defaults,
            security_level_loose: false,
            diagram_type: "flowchart-v2",
            config: Some(&config),
            shape_data_documents: &shape_data_documents,
            #[cfg(feature = "diagram-flowchart")]
            native_shape_documents: None,
            control: &control,
        };
        let mut statements = super::super::StatementArena::default();
        let construction_control = OperationControl::new();
        let id = statements
            .push(
                Stmt::ClassAssign(ClassAssignStmt {
                    targets: vec!["missing-edge".to_string()],
                    target_spans: Vec::new(),
                    class_name: "hot".to_string(),
                    class_name_span: None,
                    editor_evidence: Default::default(),
                }),
                &construction_control,
            )
            .unwrap();
        let root = statements
            .prepend(
                id,
                super::super::StatementList::default(),
                &construction_control,
            )
            .unwrap();
        let ast = FlowchartAst {
            keyword: "flowchart".to_string(),
            direction: Some("TB".to_string()),
            header_span: crate::SourceSpan::new(0, 0),
            statements,
            root,
        };

        assert!(matches!(
            apply_semantic_statements(&ast, &mut context),
            Err(crate::OperationCancelled { .. })
        ));
    }
}
