// Notepads Markdown modifications: Copyright (c) 2026, Jiaqi (0x7c13) Liu.
// Original upstream MIT/Apache-2.0 licenses and notices are preserved.
use super::{
    Edge, FlowNodeProvenance, FlowNodeSyntax, FlowchartAst, Node, StatementEvent, Stmt, TitleKind,
};
use crate::native_collections::{
    self as memory, NativeHashMap as HashMap, NativeHashSet as HashSet,
};
use crate::{OperationControl, OperationControlResult};

pub(super) struct FlowchartBuildState {
    pub(super) nodes: Vec<Node>,
    pub(super) node_index: HashMap<String, usize>,
    pub(super) edges: Vec<Edge>,
    pub(super) used_edge_ids: HashSet<String>,
    pub(super) subgraph_ids: HashSet<String>,
    pub(super) edge_pair_counts: HashMap<(String, String), usize>,
}

impl FlowchartBuildState {
    #[cfg(test)]
    pub(super) fn new(subgraph_ids: HashSet<String>) -> Self {
        Self {
            nodes: Vec::new(),
            node_index: HashMap::default(),
            edges: Vec::new(),
            used_edge_ids: HashSet::default(),
            subgraph_ids,
            edge_pair_counts: HashMap::default(),
        }
    }

    pub(super) fn new_controlled(
        subgraph_ids: HashSet<String>,
        control: &OperationControl,
    ) -> Self {
        Self {
            nodes: Vec::new(),
            node_index: memory::new_map(control),
            edges: Vec::new(),
            used_edge_ids: memory::new_set(control),
            subgraph_ids,
            edge_pair_counts: memory::new_map(control),
        }
    }

    pub(super) fn add_statements(
        &mut self,
        ast: &FlowchartAst,
        control: &OperationControl,
    ) -> OperationControlResult<()> {
        for (visited, event) in ast.walk_controlled(control)?.enumerate() {
            let event = event?;
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
                    let has_edges = !edge_groups.is_empty();
                    if let Some(first_group) = node_groups.first() {
                        self.upsert_group(first_group, has_edges, control)?;
                    }
                    for (segment_index, edges) in edge_groups.iter().enumerate() {
                        if let Some(next_group) = node_groups.get(segment_index + 1) {
                            self.upsert_group(next_group, true, control)?;
                        }
                        for (edge_index, edge) in edges.iter().enumerate() {
                            if edge_index % 128 == 0 {
                                control.checkpoint()?;
                            }
                            self.push_edge(memory::clone_owned(edge, control)?, control)?;
                        }
                    }
                }
                Stmt::Node(n) => {
                    let mut n = memory::clone_owned(n.as_ref(), control)?;
                    if self.used_edge_ids.contains(&n.id) {
                        continue;
                    }
                    n.provenance = FlowNodeProvenance::Authored;
                    self.upsert_node(n, control)?;
                }
                Stmt::ShapeData {
                    target,
                    target_span,
                    ..
                } => {
                    // Reserve the structural node slot in source order. The semantic replay is
                    // the sole owner of parsing and applying shapeData values.
                    if !self.used_edge_ids.contains(target) {
                        self.upsert_node(
                            Node {
                                id: memory::clone_owned(target, control)?,
                                provenance: FlowNodeProvenance::Authored,
                                syntax: FlowNodeSyntax::ExplicitDefinition,
                                id_span: *target_span,
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
                            control,
                        )?;
                    }
                }
                Stmt::Style(_) => {}
                Stmt::Subgraph(_) => {}
                Stmt::Direction(_)
                | Stmt::ClassDef(_)
                | Stmt::ClassAssign(_)
                | Stmt::Click(_)
                | Stmt::LinkStyle(_) => {}
            }
        }
        control.checkpoint()?;
        Ok(())
    }

    fn upsert_group(
        &mut self,
        nodes: &[Node],
        has_edges: bool,
        control: &OperationControl,
    ) -> OperationControlResult<()> {
        for (index, node) in nodes.iter().enumerate() {
            if index % 128 == 0 {
                control.checkpoint()?;
            }
            if self.used_edge_ids.contains(&node.id) {
                continue;
            }
            let mut node = memory::clone_owned(node, control)?;
            if has_edges
                && node.syntax == FlowNodeSyntax::BareReference
                && self.subgraph_ids.contains(&node.id)
            {
                node.provenance = FlowNodeProvenance::SubgraphAnchor;
            }
            self.upsert_node(node, control)?;
        }
        Ok(())
    }

    fn upsert_node(
        &mut self,
        mut n: Node,
        control: &OperationControl,
    ) -> OperationControlResult<()> {
        if control.memory_region().is_some() && n.provenance == FlowNodeProvenance::Authored {
            let already_authored = self
                .node_index
                .get(&n.id)
                .is_some_and(|index| self.nodes[*index].provenance == FlowNodeProvenance::Authored);
            if !already_authored {
                if let Some(scope) = control.memory_scope() {
                    memory::result(scope.charge_work(self.nodes.len()), control)?;
                }
                if self
                    .nodes
                    .iter()
                    .filter(|node| node.provenance == FlowNodeProvenance::Authored)
                    .count()
                    >= 128
                {
                    return Err(memory::failure(
                        control,
                        markdown_memory::MemoryError::BudgetLimited,
                    ));
                }
            }
        }
        if let Some(&idx) = self.node_index.get(&n.id) {
            if matches!(n.provenance, FlowNodeProvenance::Authored) {
                self.nodes[idx].provenance = FlowNodeProvenance::Authored;
            }
            if n.label.is_some() {
                self.nodes[idx].label = n.label;
                self.nodes[idx].label_type = n.label_type;
                self.nodes[idx].label_span = n.label_span;
                self.nodes[idx].label_selection = n.label_selection;
            }
            if n.shape.is_some() {
                self.nodes[idx].shape = n.shape;
            }
            if n.icon.is_some() {
                self.nodes[idx].icon = n.icon;
            }
            if n.form.is_some() {
                self.nodes[idx].form = n.form;
            }
            if n.pos.is_some() {
                self.nodes[idx].pos = n.pos;
            }
            if n.img.is_some() {
                self.nodes[idx].img = n.img;
            }
            if n.constraint.is_some() {
                self.nodes[idx].constraint = n.constraint;
            }
            if n.asset_width.is_some() {
                self.nodes[idx].asset_width = n.asset_width;
            }
            if n.asset_height.is_some() {
                self.nodes[idx].asset_height = n.asset_height;
            }
            memory::reserve(&mut self.nodes[idx].styles, n.styles.len(), control)?;
            self.nodes[idx].styles.append(&mut n.styles);
            memory::reserve(&mut self.nodes[idx].classes, n.classes.len(), control)?;
            self.nodes[idx].classes.append(&mut n.classes);
            return Ok(());
        }
        let idx = self.nodes.len();
        memory::insert(
            &mut self.node_index,
            memory::clone_owned(&n.id, control)?,
            idx,
            control,
        )?;
        memory::push(&mut self.nodes, n, control)?;
        Ok(())
    }

    fn push_edge(&mut self, mut e: Edge, control: &OperationControl) -> OperationControlResult<()> {
        let key = (
            memory::clone_owned(&e.from, control)?,
            memory::clone_owned(&e.to, control)?,
        );
        let existing = *self.edge_pair_counts.get(&key).unwrap_or(&0);

        let mut final_id = memory::clone_owned(&e.id, control)?;
        let mut is_user_defined_id = false;
        if let Some(user_id) = memory::clone_owned(&e.id, control)? {
            if !self.used_edge_ids.contains(&user_id) {
                is_user_defined_id = true;
                memory::insert_set(&mut self.used_edge_ids, user_id, control)?;
            } else {
                final_id = None;
            }
        }

        if final_id.is_none() {
            let counter = if existing == 0 { 0 } else { existing + 1 };
            final_id = Some(memory::format_owned(
                format_args!("L_{}_{}_{}", e.from, e.to, counter),
                control,
            )?);
            if let Some(id) = memory::clone_owned(&final_id, control)? {
                memory::insert_set(&mut self.used_edge_ids, id, control)?;
            }
        }

        memory::insert(&mut self.edge_pair_counts, key, existing + 1, control)?;

        e.id = final_id;
        e.is_user_defined_id = is_user_defined_id;
        e.link.length = e.link.length.min(10);
        memory::push(&mut self.edges, e, control)?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::diagrams::flowchart::TitleKind;

    #[test]
    fn single_large_chain_observes_cancellation_inside_node_projection() {
        let nodes = (0..256)
            .map(|index| Node {
                id: format!("n{index}"),
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
            })
            .collect();
        let mut statements = super::super::StatementArena::default();
        let construction_control = OperationControl::new();
        let id = statements
            .push(
                Stmt::Chain {
                    node_groups: vec![nodes],
                    edge_groups: Vec::new(),
                },
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
        let mut build = FlowchartBuildState::new(HashSet::default());
        let control = OperationControl::new();
        control.cancel_after_checkpoints(2);

        assert!(matches!(
            build.add_statements(&ast, &control),
            Err(crate::OperationCancelled { .. })
        ));
        assert!(build.nodes.len() < 256);
    }
}
