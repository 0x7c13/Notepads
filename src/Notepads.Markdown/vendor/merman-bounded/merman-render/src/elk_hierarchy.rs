//! Mermaid's cross-parent ELK hierarchy policy, shared by registered graph families.
//! Source: Mermaid 12 render.ts configureCrossHierarchyEdges/setIncludeChildrenPolicy.

use crate::Result;
use crate::layout_work::OperationLayoutWorkControl;
use crate::resources::OperationWorkMeter;
use std::collections::{HashMap, HashSet, hash_map::Entry};

pub(crate) fn charge_adapter_work(
    work_control: &mut Option<&mut OperationLayoutWorkControl>,
    units: usize,
) -> Result<()> {
    match work_control.as_deref_mut() {
        Some(work_control) => work_control.charge_adapter(units),
        None => Ok(()),
    }
}

pub(crate) fn checked_adapter_add(
    work_control: &Option<&mut OperationLayoutWorkControl>,
    left: usize,
    right: usize,
) -> Result<usize> {
    left.checked_add(right).ok_or_else(|| {
        work_control
            .as_deref()
            .map(|work_control| work_control.arithmetic_overflow())
            .unwrap_or_else(|| {
                OperationWorkMeter::new(
                    crate::resources::RenderResourcePolicy::unbounded_for_trusted_input(),
                )
                .arithmetic_overflow()
            })
            .into()
    })
}

pub(crate) fn checked_adapter_mul(
    work_control: &Option<&mut OperationLayoutWorkControl>,
    left: usize,
    right: usize,
) -> Result<usize> {
    left.checked_mul(right).ok_or_else(|| {
        work_control
            .as_deref()
            .map(|work_control| work_control.arithmetic_overflow())
            .unwrap_or_else(|| {
                OperationWorkMeter::new(
                    crate::resources::RenderResourcePolicy::unbounded_for_trusted_input(),
                )
                .arithmetic_overflow()
            })
            .into()
    })
}

// Heavy-light decomposition keeps hierarchy preprocessing and retained memory linear while making
// repeated common-ancestor queries logarithmic in branching depth (and constant on a heavy chain).
pub(crate) struct HierarchyIndex<'a> {
    ids: Vec<&'a str>,
    index_by_id: HashMap<&'a str, usize>,
    parent: Vec<Option<usize>>,
    depth: Vec<usize>,
    root: Vec<usize>,
    chain_head: Vec<usize>,
    heavy_child: Vec<Option<usize>>,
    preorder: Vec<usize>,
}

impl<'a> HierarchyIndex<'a> {
    pub(crate) fn build(
        nodes: impl Iterator<Item = (&'a str, Option<&'a str>)> + Clone,
        item_capacity: usize,
        work_control: &mut Option<&mut OperationLayoutWorkControl>,
    ) -> Result<Self> {
        charge_adapter_work(work_control, item_capacity)?;
        let mut ids = Vec::with_capacity(item_capacity);
        let mut index_by_id = HashMap::with_capacity(item_capacity);
        for (id, _) in nodes.clone() {
            if let Entry::Vacant(entry) = index_by_id.entry(id) {
                let index = ids.len();
                ids.push(id);
                entry.insert(index);
            }
        }

        charge_adapter_work(work_control, ids.len())?;
        let mut parent = vec![None; ids.len()];
        for (id, parent_id) in nodes {
            let index = index_by_id[id];
            parent[index] = parent_id.and_then(|parent| index_by_id.get(parent).copied());
        }

        charge_adapter_work(work_control, ids.len())?;
        let mut children = vec![Vec::new(); ids.len()];
        let mut roots = Vec::new();
        for (index, parent) in parent.iter().copied().enumerate() {
            match parent {
                Some(parent) => children[parent].push(index),
                None => roots.push(index),
            }
        }

        let hierarchy_stage_work = checked_adapter_mul(work_control, ids.len(), 2)?;
        charge_adapter_work(work_control, hierarchy_stage_work)?;
        let mut depth = vec![0usize; ids.len()];
        let mut root = vec![0usize; ids.len()];
        let mut preorder = Vec::with_capacity(ids.len());
        let mut stack = roots
            .iter()
            .rev()
            .copied()
            .map(|node| (node, node))
            .collect::<Vec<_>>();
        while let Some((node, root_node)) = stack.pop() {
            root[node] = root_node;
            preorder.push(node);
            for child in children[node].iter().rev().copied() {
                depth[child] = checked_adapter_add(work_control, depth[node], 1)?;
                stack.push((child, root_node));
            }
        }

        if preorder.len() != ids.len() {
            return Err(crate::Error::InvalidModel {
                message: "ELK node parent hierarchy contains a cycle".to_owned(),
            });
        }

        charge_adapter_work(work_control, hierarchy_stage_work)?;
        let mut subtree_size = vec![1usize; ids.len()];
        let mut heavy_child = vec![None; ids.len()];
        for node in preorder.iter().rev().copied() {
            let mut largest_child = 0usize;
            for child in children[node].iter().copied() {
                subtree_size[node] =
                    checked_adapter_add(work_control, subtree_size[node], subtree_size[child])?;
                if subtree_size[child] > largest_child {
                    largest_child = subtree_size[child];
                    heavy_child[node] = Some(child);
                }
            }
        }

        charge_adapter_work(work_control, hierarchy_stage_work)?;
        let mut chain_head = vec![0usize; ids.len()];
        let mut chains = roots
            .iter()
            .rev()
            .copied()
            .map(|root| (root, root))
            .collect::<Vec<_>>();
        while let Some((start, head)) = chains.pop() {
            let mut current = Some(start);
            while let Some(node) = current {
                chain_head[node] = head;
                for child in children[node].iter().rev().copied() {
                    if Some(child) != heavy_child[node] {
                        chains.push((child, child));
                    }
                }
                current = heavy_child[node];
            }
        }

        Ok(Self {
            ids,
            index_by_id,
            parent,
            depth,
            root,
            chain_head,
            heavy_child,
            preorder,
        })
    }

    pub(crate) fn len(&self) -> usize {
        self.ids.len()
    }

    pub(crate) fn index_of(&self, id: &str) -> Option<usize> {
        self.index_by_id.get(id).copied()
    }

    pub(crate) fn parent_index(&self, index: usize) -> Option<usize> {
        self.parent[index]
    }

    pub(crate) fn parent_first_indices(&self) -> &[usize] {
        &self.preorder
    }

    pub(crate) fn depth(&self, index: usize) -> usize {
        self.depth[index]
    }

    pub(crate) fn id(&self, index: usize) -> &'a str {
        self.ids[index]
    }

    /// Resolve the direct child on a descendant-to-ancestor path. None is the synthetic root.
    /// The caller obtains the ancestor from this index, so each branch stays in the same tree.
    pub(crate) fn child_on_path(
        &self,
        mut descendant: usize,
        ancestor: Option<usize>,
        work: &mut Option<&mut OperationLayoutWorkControl>,
    ) -> Result<usize> {
        charge_adapter_work(work, 1)?;
        let Some(ancestor) = ancestor else {
            return Ok(self.root[descendant]);
        };
        if descendant == ancestor {
            return Ok(descendant);
        }
        while self.chain_head[descendant] != self.chain_head[ancestor] {
            charge_adapter_work(work, 1)?;
            let head = self.chain_head[descendant];
            if self.parent[head] == Some(ancestor) {
                return Ok(head);
            }
            descendant = self.parent[head]
                .expect("a strict descendant has a parent above its deeper heavy chain");
        }
        Ok(self.heavy_child[ancestor]
            .expect("a same-chain strict descendant follows its ancestor's heavy child"))
    }

    pub(crate) fn common_ancestor_index(
        &self,
        left: &str,
        right: &str,
        work_control: &mut Option<&mut OperationLayoutWorkControl>,
    ) -> Result<Option<usize>> {
        let (Some(mut left), Some(mut right)) = (
            self.index_by_id.get(left).copied(),
            self.index_by_id.get(right).copied(),
        ) else {
            return Ok(None);
        };

        // Mermaid's findCommonAncestor is endpoint-inclusive, except that a self edge resolves to
        // the endpoint's parent (None here represents Mermaid's synthetic root).
        if left == right {
            return Ok(self.parent[left]);
        }
        if self.root[left] != self.root[right] {
            return Ok(None);
        }

        while self.chain_head[left] != self.chain_head[right] {
            charge_adapter_work(work_control, 1)?;
            let left_head = self.chain_head[left];
            let right_head = self.chain_head[right];
            if self.depth[left_head] > self.depth[right_head] {
                left = self.parent[left_head]
                    .expect("same-root heavy-light query has a parent above the deeper chain");
            } else {
                right = self.parent[right_head]
                    .expect("same-root heavy-light query has a parent above the deeper chain");
            }
        }
        charge_adapter_work(work_control, 1)?;
        Ok(Some(if self.depth[left] <= self.depth[right] {
            left
        } else {
            right
        }))
    }

    #[cfg(test)]
    pub(crate) fn common_ancestor_id(
        &self,
        left: &str,
        right: &str,
        work_control: &mut Option<&mut OperationLayoutWorkControl>,
    ) -> Result<Option<&'a str>> {
        self.common_ancestor_index(left, right, work_control)
            .map(|ancestor| ancestor.map(|ancestor| self.ids[ancestor]))
    }
}

struct UnmarkedHierarchyPaths {
    next: Vec<usize>,
    sentinel: usize,
}

impl UnmarkedHierarchyPaths {
    fn new(node_count: usize) -> Self {
        Self {
            next: (0..=node_count).collect(),
            sentinel: node_count,
        }
    }

    fn find(&mut self, node: usize) -> usize {
        let mut root = node;
        while self.next[root] != root {
            root = self.next[root];
        }
        let mut current = node;
        while self.next[current] != current {
            let next = self.next[current];
            self.next[current] = root;
            current = next;
        }
        root
    }

    fn remove(&mut self, node: usize, parent: Option<usize>) {
        let parent = parent.unwrap_or(self.sentinel);
        let next = self.find(parent);
        self.next[node] = next;
    }
}

/// Mermaid render.ts configureCrossHierarchyEdges/setIncludeChildrenPolicy. Both endpoints
/// and their common ancestor are included; only containers on those paths lose isolation.
/// Callers retain ownership of parent normalization and explicit algorithm metadata.
pub(crate) fn include_children<'a>(
    nodes: impl Iterator<Item = (&'a str, Option<&'a str>)> + Clone,
    node_count: usize,
    edges: impl ExactSizeIterator<Item = (&'a str, &'a str)>,
    work_control: &mut Option<&mut OperationLayoutWorkControl>,
) -> Result<HashSet<&'a str>> {
    if node_count == 0 || edges.len() == 0 {
        return Ok(HashSet::new());
    }
    charge_adapter_work(work_control, node_count)?;
    let parents = nodes.clone().collect::<HashMap<_, _>>();
    charge_adapter_work(work_control, edges.len())?;
    let cross_parent_edges = edges
        .filter(|(source, target)| {
            parents.contains_key(source)
                && parents.contains_key(target)
                && parents[source] != parents[target]
        })
        .collect::<Vec<_>>();
    if cross_parent_edges.is_empty() {
        return Ok(HashSet::new());
    }
    let hierarchy = HierarchyIndex::build(nodes, node_count, work_control)?;
    // Cross-parent edges override direction-induced SeparateChildren on both endpoint-to-LCA
    // paths. Mermaid's walk includes both endpoints and the common ancestor; path compression may
    // skip only nodes already marked by an earlier edge.
    charge_adapter_work(work_control, hierarchy.len())?;
    let mut unmarked = UnmarkedHierarchyPaths::new(hierarchy.len());
    let mut include_children = HashSet::new();
    for (source, target) in cross_parent_edges {
        let ancestor = hierarchy.common_ancestor_index(source, target, work_control)?;
        mark_include_children_path(
            source,
            ancestor,
            &hierarchy,
            &mut unmarked,
            &mut include_children,
            work_control,
        )?;
        mark_include_children_path(
            target,
            ancestor,
            &hierarchy,
            &mut unmarked,
            &mut include_children,
            work_control,
        )?;
    }
    Ok(include_children)
}

fn mark_include_children_path<'a>(
    node_id: &str,
    ancestor: Option<usize>,
    hierarchy: &HierarchyIndex<'a>,
    unmarked: &mut UnmarkedHierarchyPaths,
    include_children: &mut HashSet<&'a str>,
    work_control: &mut Option<&mut OperationLayoutWorkControl>,
) -> Result<()> {
    let Some(start) = hierarchy.index_by_id.get(node_id).copied() else {
        return Ok(());
    };
    let stop_depth = ancestor.map(|ancestor| hierarchy.depth[ancestor]);
    loop {
        let node = unmarked.find(start);
        if node == unmarked.sentinel
            || stop_depth.is_some_and(|stop_depth| hierarchy.depth[node] < stop_depth)
        {
            break;
        }
        charge_adapter_work(work_control, 1)?;
        include_children.insert(hierarchy.ids[node]);
        let reached_ancestor = Some(node) == ancestor;
        unmarked.remove(node, hierarchy.parent[node]);
        if reached_ancestor {
            break;
        }
    }
    Ok(())
}

pub(crate) fn apply_to_graph(
    graph: &mut merman_layout_elk::Graph,
    work: &mut OperationLayoutWorkControl,
) -> Result<()> {
    use merman_layout_elk::{HierarchyHandling, NodeKind};
    if graph.edges.is_empty()
        || !graph.nodes.iter().any(|node| {
            node.kind == NodeKind::Group
                && node.hierarchy_handling != Some(HierarchyHandling::IncludeChildren)
        })
    {
        return Ok(());
    }
    let included = include_children(
        graph
            .nodes
            .iter()
            .map(|node| (node.id.as_str(), node.parent.as_deref())),
        graph.nodes.len(),
        graph
            .edges
            .iter()
            .map(|edge| (edge.source.as_str(), edge.target.as_str())),
        &mut Some(&mut *work),
    )?;
    if included.is_empty() {
        return Ok(());
    }
    work.charge_adapter(graph.nodes.len())?;
    let indices = graph
        .nodes
        .iter()
        .enumerate()
        .filter_map(|(index, node)| {
            (node.kind == NodeKind::Group && included.contains(node.id.as_str())).then_some(index)
        })
        .collect::<Vec<_>>();
    for index in indices {
        graph.nodes[index].hierarchy_handling = Some(HierarchyHandling::IncludeChildren);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::resources::RenderResourcePolicy;
    use merman_layout_elk::{Direction, Edge, Graph, HierarchyHandling, Node, NodeKind};
    use std::sync::Arc;

    fn work() -> OperationLayoutWorkControl {
        OperationLayoutWorkControl::new(Arc::new(OperationWorkMeter::new(
            RenderResourcePolicy::unbounded_for_trusted_input(),
        )))
    }

    #[test]
    fn cross_parent_policy_includes_endpoints_and_lca_without_ancestors_above_it() {
        let nodes = [
            ("outer", None),
            ("lca", Some("outer")),
            ("left", Some("lca")),
            ("right", Some("lca")),
            ("a", Some("left")),
            ("b", Some("right")),
            ("untouched", Some("outer")),
        ];
        let mut work = work();
        let included = include_children(
            nodes.into_iter(),
            nodes.len(),
            [("a", "b"), ("a", "b")].into_iter(),
            &mut Some(&mut work),
        )
        .unwrap();
        assert_eq!(included, HashSet::from(["a", "left", "lca", "right", "b"]));
    }

    #[test]
    fn same_parent_policy_skips_hierarchy_index_and_empty_edges_charge_nothing() {
        let nodes = [
            ("container", None),
            ("a", Some("container")),
            ("b", Some("container")),
        ];
        let mut work = work();
        assert!(
            include_children(
                nodes.into_iter(),
                nodes.len(),
                [].into_iter(),
                &mut Some(&mut work)
            )
            .unwrap()
            .is_empty()
        );
        assert_eq!(work.adapter_work(), 0);
        assert!(
            include_children(
                nodes.into_iter(),
                nodes.len(),
                [("a", "b")].into_iter(),
                &mut Some(&mut work)
            )
            .unwrap()
            .is_empty()
        );
        assert_eq!(work.adapter_work(), nodes.len() + 1);
    }

    fn directional_graph() -> Graph {
        Graph {
            nodes: [
                ("container", None, NodeKind::Group),
                ("a", Some("container"), NodeKind::Leaf),
                ("b", Some("container"), NodeKind::Leaf),
                ("outside", None, NodeKind::Leaf),
            ]
            .into_iter()
            .map(|(id, parent, kind)| Node {
                id: id.into(),
                kind,
                parent: parent.map(str::to_owned),
                label_text: None,
                container: Default::default(),
                width: 40.0,
                height: 20.0,
                direction: (kind == NodeKind::Group).then_some(Direction::Right),
                hierarchy_handling: (kind == NodeKind::Group)
                    .then_some(HierarchyHandling::SeparateChildren),
                layer_constraint: None,
                port_alignment: None,
                label: None,
            })
            .collect(),
            edges: vec![Edge {
                id: "edge".into(),
                source: "a".into(),
                target: "b".into(),
                label: None,
                minlen: 1,
                terminal_labels: Vec::new(),
                inside_self_loops_yo: false,
            }],
            ..Graph::default()
        }
    }

    #[test]
    fn graph_policy_preserves_independent_direction_and_rejects_before_mutation() {
        let mut graph = directional_graph();
        let unchanged = graph.clone();
        apply_to_graph(&mut graph, &mut work()).unwrap();
        assert_eq!(graph, unchanged);
        graph.edges[0].target = "outside".into();
        let before = graph.clone();
        let meter = OperationWorkMeter::new(
            RenderResourcePolicy::unbounded_for_trusted_input()
                .with_limit(crate::ResourceLimitId::MaxLayoutWorkUnits, 1)
                .expect("valid work limit"),
        );
        let mut limited = OperationLayoutWorkControl::new(Arc::new(meter));
        assert!(matches!(
            apply_to_graph(&mut graph, &mut limited),
            Err(crate::Error::ResourceLimitExceeded(_))
        ));
        assert_eq!(graph, before);
        apply_to_graph(&mut graph, &mut work()).unwrap();
        assert_eq!(
            graph.nodes[0].hierarchy_handling,
            Some(HierarchyHandling::IncludeChildren)
        );
        assert_eq!(graph.nodes[0].direction, Some(Direction::Right));
        assert_eq!(graph.edges, before.edges);
    }
}
