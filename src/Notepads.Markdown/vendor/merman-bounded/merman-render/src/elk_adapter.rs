//! Shared Mermaid ELK adapter semantics, separate from provider layout geometry.
//! Source: Mermaid 12 elk/render.ts and createGraph.ts at the pinned baseline.

use crate::Result;
use crate::elk_hierarchy::{
    HierarchyIndex, charge_adapter_work, checked_adapter_add, checked_adapter_mul,
};
use crate::layout_work::OperationLayoutWorkControl;
use merman_layout_elk as elk;
use std::collections::HashMap;

/// Pin the true entry of each source-less weak component in its direct parent scope.
/// Callers own the configuration switch and supply final node and edge declaration order.
/// Work rejection leaves all node constraints unchanged.
pub(crate) fn apply_cyclic_entry_constraints<'a>(
    nodes: &mut [elk::Node],
    edges: impl ExactSizeIterator<Item = (&'a str, &'a str)>,
    work: &mut Option<&mut OperationLayoutWorkControl>,
) -> Result<()> {
    if nodes.is_empty() || edges.len() == 0 {
        return Ok(());
    }
    charge_adapter_work(work, checked_adapter_mul(work, nodes.len(), 7)?)?;
    let indices: HashMap<_, _> = nodes
        .iter()
        .enumerate()
        .map(|(index, node)| (node.id.as_str(), index))
        .collect();
    let mut neighbors = vec![Vec::new(); nodes.len()];
    let mut indegree = vec![0usize; nodes.len()];
    charge_adapter_work(work, checked_adapter_mul(work, edges.len(), 3)?)?;
    let mut internal_edges = Vec::with_capacity(edges.len());
    for (source, target) in edges {
        let (Some(&source), Some(&target)) = (indices.get(source), indices.get(target)) else {
            continue;
        };
        if source == target || nodes[source].parent != nodes[target].parent {
            continue;
        }
        indegree[target] = checked_adapter_add(work, indegree[target], 1)?;
        neighbors[source].push(target);
        neighbors[target].push(source);
        internal_edges.push((source, target));
    }

    let mut component = vec![usize::MAX; nodes.len()];
    let mut has_source = Vec::new();
    let mut stack = Vec::with_capacity(nodes.len());
    for start in 0..nodes.len() {
        charge_adapter_work(work, 1)?;
        if component[start] != usize::MAX {
            continue;
        }
        let current_component = has_source.len();
        has_source.push(false);
        component[start] = current_component;
        stack.push(start);
        while let Some(node) = stack.pop() {
            charge_adapter_work(work, checked_adapter_add(work, neighbors[node].len(), 1)?)?;
            has_source[current_component] |= indegree[node] == 0;
            for &next in &neighbors[node] {
                if component[next] == usize::MAX {
                    // Mark on insertion so parallel edges cannot duplicate pending work.
                    component[next] = current_component;
                    stack.push(next);
                }
            }
        }
    }
    if has_source.iter().all(|has_source| *has_source) {
        return Ok(());
    }

    charge_adapter_work(work, checked_adapter_mul(work, nodes.len(), 4)?)?;
    let mut forward = vec![Vec::<usize>::new(); nodes.len()];
    let mut residual_indegree = vec![0usize; nodes.len()];
    let mut seen = vec![0usize; nodes.len()];
    for (edge_index, &(source, target)) in internal_edges.iter().enumerate() {
        charge_adapter_work(work, 1)?;
        if has_source[component[source]] {
            continue;
        }
        let visit = checked_adapter_add(work, edge_index, 1)?;
        seen[target] = visit;
        stack.clear();
        stack.push(target);
        let mut closes_cycle = false;
        while let Some(node) = stack.pop() {
            charge_adapter_work(work, 1)?;
            if node == source {
                closes_cycle = true;
                break;
            }
            charge_adapter_work(work, forward[node].len())?;
            for &next in &forward[node] {
                if seen[next] != visit {
                    seen[next] = visit;
                    stack.push(next);
                }
            }
        }
        if !closes_cycle {
            forward[source].push(target);
            residual_indegree[target] = checked_adapter_add(work, residual_indegree[target], 1)?;
        }
    }

    charge_adapter_work(
        work,
        checked_adapter_add(work, has_source.len(), nodes.len())?,
    )?;
    let mut nominated = vec![false; has_source.len()];
    let mut entries = Vec::new();
    for node in 0..nodes.len() {
        let component = component[node];
        if !has_source[component] && !nominated[component] && residual_indegree[node] == 0 {
            nominated[component] = true;
            entries.push(node);
        }
    }
    charge_adapter_work(work, entries.len())?;
    for entry in entries {
        nodes[entry].layer_constraint = Some(elk::LayerConstraint::First);
    }
    Ok(())
}

/// Resolve a node's own direction, then its nearest ancestor's, then the diagram default.
/// The input is independent of measured ELK nodes because shapes need direction during sizing.
pub(crate) fn resolved_node_directions<'a>(
    nodes: impl Iterator<Item = (&'a str, Option<&'a str>, Option<elk::Direction>)> + Clone,
    item_capacity: usize,
    root: elk::Direction,
    work: &mut Option<&mut OperationLayoutWorkControl>,
) -> Result<HashMap<&'a str, elk::Direction>> {
    let hierarchy = HierarchyIndex::build(
        nodes.clone().map(|(id, parent, _)| (id, parent)),
        item_capacity,
        work,
    )?;
    charge_adapter_work(work, checked_adapter_mul(work, hierarchy.len(), 2)?)?;
    let explicit: HashMap<_, _> = nodes.map(|(id, _, direction)| (id, direction)).collect();
    let mut resolved = HashMap::with_capacity(hierarchy.len());
    for &node in hierarchy.parent_first_indices() {
        let inherited = hierarchy
            .parent_index(node)
            .and_then(|parent| resolved.get(hierarchy.id(parent)).copied())
            .unwrap_or(root);
        resolved.insert(
            hierarchy.id(node),
            explicit[hierarchy.id(node)].unwrap_or(inherited),
        );
    }
    Ok(resolved)
}

/// Stable Mermaid paint order: groups by ascending depth, followed by leaves in model order.
pub(crate) fn parent_first_order(
    nodes: &[elk::Node],
    work: &mut Option<&mut OperationLayoutWorkControl>,
) -> Result<Vec<usize>> {
    let hierarchy = HierarchyIndex::build(
        nodes
            .iter()
            .map(|node| (node.id.as_str(), node.parent.as_deref())),
        nodes.len(),
        work,
    )?;
    // Depth cannot exceed the number of nodes. Counting buckets preserves sibling model order
    // without a comparator that repeatedly walks ancestors or adds a comparison-sort budget.
    charge_adapter_work(work, checked_adapter_mul(work, nodes.len(), 3)?)?;
    let mut groups_by_depth = vec![Vec::new(); nodes.len()];
    let mut leaves = Vec::new();
    for (index, node) in nodes.iter().enumerate() {
        if node.kind == elk::NodeKind::Group {
            if let Some(hierarchy_index) = hierarchy.index_of(&node.id) {
                groups_by_depth[hierarchy.depth(hierarchy_index)].push(index);
            }
        } else {
            leaves.push(index);
        }
    }
    let mut order = Vec::with_capacity(nodes.len());
    order.extend(groups_by_depth.into_iter().flatten());
    order.extend(leaves);
    Ok(order)
}

/// A painted group rectangle, expressed as center coordinates in the provider's absolute space.
/// This is a projection only: provider node origins and routed edge points remain immutable.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct GroupFrame {
    pub(crate) x: f64,
    pub(crate) y: f64,
    pub(crate) width: f64,
    pub(crate) height: f64,
}

impl From<&elk::NodeLayout> for GroupFrame {
    fn from(node: &elk::NodeLayout) -> Self {
        Self {
            x: node.x,
            y: node.y,
            width: node.width,
            height: node.height,
        }
    }
}

#[derive(Clone, Copy)]
struct Bounds {
    left: f64,
    right: f64,
    bottom: f64,
}

impl Bounds {
    fn point(point: &elk::Point) -> Self {
        Self {
            left: point.x,
            right: point.x,
            bottom: point.y,
        }
    }

    fn frame(frame: GroupFrame) -> Self {
        Self {
            left: frame.x - frame.width / 2.0,
            right: frame.x + frame.width / 2.0,
            bottom: frame.y + frame.height / 2.0,
        }
    }

    fn include(&mut self, other: Self) {
        self.left = self.left.min(other.left);
        self.right = self.right.max(other.right);
        self.bottom = self.bottom.max(other.bottom);
    }
}

fn include_bounds(bounds: &mut Option<Bounds>, addition: Bounds) {
    match bounds {
        Some(bounds) => bounds.include(addition),
        None => *bounds = Some(addition),
    }
}

/// Mermaid's evenGroupFrames pass, after provider positions and before edge/shape projection.
/// `title_width` includes the painted title's total horizontal padding, or zero for an untitled
/// frame (for example State noteGroup). Its result must agree with the provider's title minimum.
///
/// Provider routes have already been flattened to absolute coordinates. Keep them untouched and
/// return only drawn rectangles so a shifted frame never changes an edge owner's routing origin.
/// Internal lane bounds accumulate at the lowest strict common ancestor and propagate bottom-up;
/// this avoids rescanning every edge or materializing every descendant set for every container.
pub(crate) fn drawing_group_frames<'a>(
    graph: &'a elk::Graph,
    provider: &elk::LayoutResult,
    title_width: impl Fn(&elk::Node) -> f64,
    work: &mut Option<&mut OperationLayoutWorkControl>,
) -> Result<HashMap<&'a str, GroupFrame>> {
    if !graph
        .nodes
        .iter()
        .any(|node| node.kind == elk::NodeKind::Group)
    {
        return Ok(HashMap::new());
    }
    let hierarchy = HierarchyIndex::build(
        graph
            .nodes
            .iter()
            .map(|node| (node.id.as_str(), node.parent.as_deref())),
        graph.nodes.len(),
        work,
    )?;
    if hierarchy.len() != graph.nodes.len() {
        return Err(crate::Error::InvalidModel {
            message: "ELK frame projection requires unique node IDs".to_owned(),
        });
    }
    charge_adapter_work(work, checked_adapter_mul(work, graph.nodes.len(), 5)?)?;
    charge_adapter_work(
        work,
        checked_adapter_add(work, provider.nodes.len(), provider.edges.len())?,
    )?;
    let provider_nodes: HashMap<_, _> = provider
        .nodes
        .iter()
        .map(|node| (node.id.as_str(), GroupFrame::from(node)))
        .collect();
    let provider_edges: HashMap<_, _> = provider
        .edges
        .iter()
        .map(|edge| (edge.id.as_str(), edge))
        .collect();
    let mut children = vec![Vec::new(); graph.nodes.len()];
    for node in 0..graph.nodes.len() {
        if let Some(parent) = hierarchy.parent_index(node) {
            children[parent].push(node);
        }
    }
    let mut internal_lanes = vec![None; graph.nodes.len()];
    let mut terminals = vec![None; graph.nodes.len()];
    for edge in &graph.edges {
        charge_adapter_work(work, 1)?;
        let Some(route) = provider_edges.get(edge.id.as_str()) else {
            continue;
        };
        let (Some(source), Some(target)) = (
            hierarchy.index_of(&edge.source),
            hierarchy.index_of(&edge.target),
        ) else {
            continue;
        };
        if let Some(point) = route.points.first() {
            include_bounds(&mut terminals[source], Bounds::point(point));
        }
        if let Some(point) = route.points.last() {
            include_bounds(&mut terminals[target], Bounds::point(point));
        }
        let mut owner = hierarchy.common_ancestor_index(&edge.source, &edge.target, work)?;
        // A group is not its own descendant. Full lanes enter its frame only when both
        // endpoints are strictly inside; a group endpoint contributes only its terminal above.
        if owner == Some(source) || owner == Some(target) {
            owner = owner.and_then(|owner| hierarchy.parent_index(owner));
        }
        if let Some(owner) = owner {
            charge_adapter_work(work, route.points.len())?;
            for point in &route.points {
                include_bounds(&mut internal_lanes[owner], Bounds::point(point));
            }
        }
    }

    const SUBGRAPH_PADDING: f64 = 24.0;
    let mut frames: HashMap<&str, GroupFrame> = HashMap::new();
    for &node in hierarchy.parent_first_indices().iter().rev() {
        charge_adapter_work(work, checked_adapter_add(work, children[node].len(), 1)?)?;
        // Propagate provider lane extents, never a drawing frame's shifted origin.
        if let Some(parent) = hierarchy.parent_index(node)
            && let Some(lane) = internal_lanes[node]
        {
            include_bounds(&mut internal_lanes[parent], lane);
        }
        let source = &graph.nodes[node];
        if source.kind != elk::NodeKind::Group {
            continue;
        }
        let Some(&original) = provider_nodes.get(source.id.as_str()) else {
            continue;
        };
        let mut bounds = None;
        for &child in &children[node] {
            let id = hierarchy.id(child);
            if let Some(&frame) = frames.get(id).or_else(|| provider_nodes.get(id))
                && frame.width > 0.0
                && frame.height > 0.0
            {
                include_bounds(&mut bounds, Bounds::frame(frame));
            }
        }
        // Empty groups retain the exact provider rectangle, including their label geometry.
        let Some(mut bounds) = bounds else {
            continue;
        };
        if let Some(lane) = internal_lanes[node] {
            bounds.include(lane);
        }
        if let Some(terminal) = terminals[node] {
            bounds.include(terminal);
        }
        let original_left = original.x - original.width / 2.0;
        let original_right = original.x + original.width / 2.0;
        let top = original.y - original.height / 2.0;
        let left = original_left.max(bounds.left - SUBGRAPH_PADDING);
        let right = original_right.min(bounds.right + SUBGRAPH_PADDING);
        let bottom = (original.y + original.height / 2.0).min(bounds.bottom + SUBGRAPH_PADDING);
        let mut x = left;
        let mut width = right - left;
        let label_floor = title_width(source);
        if width < label_floor {
            x -= (label_floor - width) / 2.0;
            width = label_floor;
            x = original_left.max(x.min(original_right - width));
            width = width.min(original.width);
        }
        let height = bottom - top;
        if height > 0.0 && width > 0.0 {
            frames.insert(
                source.id.as_str(),
                GroupFrame {
                    x: x + width / 2.0,
                    y: top + height / 2.0,
                    width,
                    height,
                },
            );
        }
    }
    Ok(frames)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::resources::{OperationWorkMeter, RenderResourcePolicy};
    use std::sync::Arc;

    fn node(id: &str, parent: Option<&str>, kind: elk::NodeKind) -> elk::Node {
        elk::Node {
            id: id.to_owned(),
            kind,
            parent: parent.map(str::to_owned),
            label_text: None,
            container: Default::default(),
            width: 0.0,
            height: 0.0,
            direction: None,
            hierarchy_handling: None,
            layer_constraint: None,
            port_alignment: None,
            label: None,
        }
    }

    fn leaf(id: &str, parent: Option<&str>) -> elk::Node {
        node(id, parent, elk::NodeKind::Leaf)
    }

    fn group(id: &str, parent: Option<&str>) -> elk::Node {
        node(id, parent, elk::NodeKind::Group)
    }

    fn box_at(id: &str, x: f64, y: f64, width: f64, height: f64) -> elk::NodeLayout {
        elk::NodeLayout {
            id: id.to_owned(),
            x: x + width / 2.0,
            y: y + height / 2.0,
            width,
            height,
        }
    }

    fn work() -> OperationLayoutWorkControl {
        OperationLayoutWorkControl::new(Arc::new(OperationWorkMeter::new(
            RenderResourcePolicy::unbounded_for_trusted_input(),
        )))
    }

    fn limited_work(units: usize) -> OperationLayoutWorkControl {
        OperationLayoutWorkControl::new(Arc::new(OperationWorkMeter::new(
            RenderResourcePolicy::unbounded_for_trusted_input()
                .with_limit(crate::ResourceLimitId::MaxLayoutWorkUnits, units)
                .unwrap(),
        )))
    }

    fn nominees(nodes: &[elk::Node]) -> Vec<&str> {
        nodes
            .iter()
            .filter(|node| node.layer_constraint == Some(elk::LayerConstraint::First))
            .map(|node| node.id.as_str())
            .collect()
    }

    #[test]
    fn cyclic_entry_follows_forward_edge_order_independently_of_node_declaration() {
        for ids in [["a", "b", "c"], ["c", "b", "a"], ["a", "c", "b"]] {
            let mut nodes: Vec<_> = ids.into_iter().map(|id| leaf(id, None)).collect();
            apply_cyclic_entry_constraints(
                &mut nodes,
                [("b", "c"), ("c", "a"), ("a", "b")].into_iter(),
                &mut Some(&mut work()),
            )
            .unwrap();
            assert_eq!(nominees(&nodes), ["b"]);
        }
    }

    #[test]
    fn cyclic_entry_uses_first_declared_residual_source_not_first_edge_source() {
        let mut nodes = vec![leaf("b", None), leaf("a", None), leaf("c", None)];
        apply_cyclic_entry_constraints(
            &mut nodes,
            [("a", "c"), ("b", "c"), ("c", "a"), ("c", "b")].into_iter(),
            &mut Some(&mut work()),
        )
        .unwrap();
        assert_eq!(nominees(&nodes), ["b"]);
    }

    #[test]
    fn cyclic_entry_matches_upstream_world_clock_regression() {
        let mut nodes: Vec<_> = [
            "san_francisco",
            "stockholm",
            "new_york",
            "decide",
            "end_decision",
            "format_json",
        ]
        .into_iter()
        .map(|id| leaf(id, Some("world-clock")))
        .collect();
        apply_cyclic_entry_constraints(
            &mut nodes,
            [
                ("stockholm", "new_york"),
                ("new_york", "san_francisco"),
                ("san_francisco", "decide"),
                ("decide", "end_decision"),
                ("end_decision", "format_json"),
                ("end_decision", "stockholm"),
            ]
            .into_iter(),
            &mut Some(&mut work()),
        )
        .unwrap();
        assert_eq!(nominees(&nodes), ["stockholm"]);
    }

    #[test]
    fn cyclic_entry_scopes_components_and_ignores_self_loops_and_cross_parent_edges() {
        let mut nodes = vec![
            leaf("root_a", None),
            leaf("root_b", None),
            leaf("orphan", None),
            group("outer", None),
            group("inner", Some("outer")),
            leaf("a", Some("outer")),
            leaf("b", Some("outer")),
            leaf("x", Some("inner")),
            leaf("y", Some("inner")),
        ];
        apply_cyclic_entry_constraints(
            &mut nodes,
            [
                ("root_a", "root_b"),
                ("root_b", "root_b"),
                ("orphan", "orphan"),
                ("b", "a"),
                ("a", "b"),
                ("y", "x"),
                ("x", "y"),
                ("root_a", "b"),
                ("b", "y"),
                ("missing", "a"),
            ]
            .into_iter(),
            &mut Some(&mut work()),
        )
        .unwrap();
        assert_eq!(nominees(&nodes), ["b", "y"]);
    }

    #[test]
    fn cyclic_component_with_natural_source_preserves_existing_constraints() {
        let mut nodes = vec![leaf("source", None), leaf("a", None), leaf("b", None)];
        nodes[0].layer_constraint = Some(elk::LayerConstraint::Last);
        let before = nodes.clone();
        apply_cyclic_entry_constraints(
            &mut nodes,
            [
                ("source", "a"),
                ("a", "b"),
                ("b", "a"),
                ("source", "source"),
            ]
            .into_iter(),
            &mut Some(&mut work()),
        )
        .unwrap();
        assert_eq!(nodes, before);
    }

    #[test]
    fn cyclic_entry_charges_reachability_and_rejects_before_mutation() {
        let nodes = vec![leaf("a", None), leaf("b", None), leaf("c", None)];
        let edges = [("b", "c"), ("c", "a"), ("a", "b")];
        let mut full_work = work();
        apply_cyclic_entry_constraints(
            &mut nodes.clone(),
            edges.into_iter(),
            &mut Some(&mut full_work),
        )
        .unwrap();
        let exact = full_work.adapter_work();
        for limit in [1, exact - 1] {
            let mut attempted = nodes.clone();
            assert!(matches!(
                apply_cyclic_entry_constraints(
                    &mut attempted,
                    edges.into_iter(),
                    &mut Some(&mut limited_work(limit)),
                ),
                Err(crate::Error::ResourceLimitExceeded(_)),
            ));
            assert_eq!(attempted, nodes);
        }
        let mut attempted = nodes;
        let mut exact_work = limited_work(exact);
        apply_cyclic_entry_constraints(
            &mut attempted,
            edges.into_iter(),
            &mut Some(&mut exact_work),
        )
        .unwrap();
        assert_eq!(nominees(&attempted), ["b"]);
    }

    #[test]
    fn nearest_direction_supports_late_ancestors_and_explicit_group_aliases() {
        let nodes = [
            ("leaf", Some("inner"), None),
            ("inner", Some("outer"), None),
            ("sibling", Some("outer"), None),
            ("outside", None, None),
            ("outer", None, Some(elk::Direction::Right)),
            ("inner", Some("outer"), Some(elk::Direction::Up)),
        ];
        let resolved = resolved_node_directions(
            nodes.into_iter(),
            nodes.len(),
            elk::Direction::Down,
            &mut Some(&mut work()),
        )
        .unwrap();
        assert_eq!(resolved["leaf"], elk::Direction::Up);
        assert_eq!(resolved["inner"], elk::Direction::Up);
        assert_eq!(resolved["sibling"], elk::Direction::Right);
        assert_eq!(resolved["outside"], elk::Direction::Down);
    }

    #[test]
    fn container_paint_order_is_depth_stable_not_lexicographic() {
        let nodes = vec![
            group("AInner", Some("ZOuter")),
            leaf("leaf", Some("AInner")),
            group("BInner", Some("ZOuter")),
            group("ZOuter", None),
            group("RootSibling", None),
            group("Grandchild", Some("AInner")),
            leaf("root_leaf", None),
        ];
        let order = parent_first_order(&nodes, &mut Some(&mut work())).unwrap();
        let ids: Vec<_> = order
            .into_iter()
            .map(|index| nodes[index].id.as_str())
            .collect();
        assert_eq!(
            ids,
            [
                "ZOuter",
                "RootSibling",
                "AInner",
                "BInner",
                "Grandchild",
                "leaf",
                "root_leaf"
            ]
        );
    }

    fn scene(group_width: f64, child_x: f64, child_width: f64) -> (elk::Graph, elk::LayoutResult) {
        (
            elk::Graph {
                nodes: vec![group("g", None), leaf("n", Some("g"))],
                ..Default::default()
            },
            elk::LayoutResult {
                nodes: vec![
                    box_at("g", 0.0, 0.0, group_width, 148.0),
                    box_at("n", child_x, 48.0, child_width, 76.0),
                ],
                edges: Vec::new(),
            },
        )
    }

    fn frames<'a>(
        graph: &'a elk::Graph,
        provider: &elk::LayoutResult,
    ) -> HashMap<&'a str, GroupFrame> {
        drawing_group_frames(
            graph,
            provider,
            |node| node.label.map_or(0.0, |label| label.width) + node.container.padding,
            &mut Some(&mut work()),
        )
        .unwrap()
    }

    fn edge(
        graph: &mut elk::Graph,
        provider: &mut elk::LayoutResult,
        source: &str,
        target: &str,
        points: &[(f64, f64)],
    ) {
        let id = format!("edge-{}", graph.edges.len());
        graph.edges.push(elk::Edge {
            id: id.clone(),
            source: source.to_owned(),
            target: target.to_owned(),
            label: None,
            terminal_labels: Vec::new(),
            minlen: 1,
            inside_self_loops_yo: false,
        });
        provider.edges.push(elk::EdgeLayout {
            id,
            points: points.iter().map(|&(x, y)| elk::Point { x, y }).collect(),
            labels: vec![elk::EdgeLabelLayout {
                terminal: None,
                x: 10.0,
                y: 20.0,
                width: 30.0,
                height: 40.0,
            }],
        });
    }

    #[test]
    fn frame_contraction_matches_upstream_200_to_148_without_moving_provider_geometry() {
        let (graph, provider) = scene(200.0, 24.0, 100.0);
        let before = provider.clone();
        assert_eq!(
            frames(&graph, &provider)["g"],
            GroupFrame {
                x: 74.0,
                y: 74.0,
                width: 148.0,
                height: 148.0
            }
        );
        assert_eq!(provider, before);
        let (graph, provider) = scene(148.0, 24.0, 100.0);
        assert_eq!(
            frames(&graph, &provider)["g"],
            GroupFrame::from(&provider.nodes[0])
        );
    }

    #[test]
    fn frames_preserve_provider_origins_and_absolute_routes_when_left_edge_moves() {
        let (mut graph, mut provider) = scene(200.0, 40.0, 100.0);
        edge(
            &mut graph,
            &mut provider,
            "n",
            "n",
            &[(40.0, 60.0), (140.0, 70.0)],
        );
        let before = provider.clone();
        let frame = frames(&graph, &provider)["g"];
        assert_eq!(frame.x - frame.width / 2.0, 16.0);
        assert_eq!(frame.width, 148.0);
        assert_eq!(provider, before);
    }

    #[test]
    fn frames_reserve_title_padding_and_never_grow_beyond_provider_bounds() {
        for (provider_width, child_x, label_width, padding, expected_width) in [
            (300.0, 24.0, 200.0, 0.0, 200.0),
            (300.0, 24.0, 200.0, 20.0, 220.0),
            (100.0, 10.0, 200.0, 20.0, 100.0),
            (120.0, 10.0, 0.0, 0.0, 74.0),
        ] {
            let (mut graph, provider) = scene(provider_width, child_x, 40.0);
            graph.nodes[0].label = Some(elk::Label {
                width: label_width,
                height: 20.0,
            });
            graph.nodes[0].container.padding = padding;
            let frame = frames(&graph, &provider)["g"];
            assert_eq!(frame.width, expected_width);
            assert_eq!(frame.x - frame.width / 2.0, 0.0);
            assert_eq!(frame.y - frame.height / 2.0, 0.0);
        }
        let (graph, provider) = scene(120.0, 10.0, 100.0);
        assert_eq!(frames(&graph, &provider)["g"].width, 120.0);
    }

    #[test]
    fn untitled_and_empty_groups_do_not_inherit_a_painted_title_floor() {
        let (mut graph, provider) = scene(300.0, 24.0, 40.0);
        graph.nodes[0].label = Some(elk::Label {
            width: 200.0,
            height: 20.0,
        });
        let projected =
            drawing_group_frames(&graph, &provider, |_| 0.0, &mut Some(&mut work())).unwrap();
        assert_eq!(projected["g"].width, 88.0);
        graph.nodes.truncate(1);
        assert!(frames(&graph, &provider).is_empty());
    }

    #[test]
    fn nested_frames_measure_contracted_children_and_internal_lanes_separately() {
        let mut graph = elk::Graph {
            nodes: vec![
                group("C", Some("P")),
                group("P", None),
                leaf("leaf", Some("C")),
            ],
            ..Default::default()
        };
        let mut provider = elk::LayoutResult {
            nodes: vec![
                box_at("P", 0.0, 0.0, 400.0, 220.0),
                box_at("C", 24.0, 48.0, 200.0, 148.0),
                box_at("leaf", 48.0, 96.0, 100.0, 76.0),
            ],
            edges: Vec::new(),
        };
        let projected = frames(&graph, &provider);
        assert_eq!(projected["C"].width, 148.0);
        assert_eq!(projected["P"].width, 196.0);
        graph.nodes.push(leaf("sib", Some("P")));
        provider.nodes.push(box_at("sib", 260.0, 96.0, 60.0, 76.0));
        edge(
            &mut graph,
            &mut provider,
            "leaf",
            "sib",
            &[
                (148.0, 134.0),
                (350.0, 134.0),
                (350.0, 60.0),
                (260.0, 134.0),
            ],
        );
        let projected = frames(&graph, &provider);
        assert_eq!(projected["C"].width, 148.0);
        assert_eq!(projected["P"].x + projected["P"].width / 2.0, 374.0);
    }

    #[test]
    fn nested_route_bounds_use_provider_absolute_space_without_adding_owner_offsets() {
        let mut graph = elk::Graph {
            nodes: vec![
                group("outer", None),
                group("inner", Some("outer")),
                leaf("a", Some("inner")),
                leaf("b", Some("inner")),
            ],
            ..Default::default()
        };
        let mut provider = elk::LayoutResult {
            nodes: vec![
                box_at("outer", 0.0, 0.0, 300.0, 250.0),
                box_at("inner", 24.0, 48.0, 250.0, 178.0),
                box_at("a", 48.0, 96.0, 40.0, 76.0),
                box_at("b", 140.0, 96.0, 40.0, 76.0),
            ],
            edges: Vec::new(),
        };
        edge(
            &mut graph,
            &mut provider,
            "a",
            "b",
            &[(88.0, 134.0), (220.0, 134.0), (220.0, 80.0), (140.0, 134.0)],
        );
        let original_frames = frames(&graph, &provider);
        assert_eq!(original_frames["inner"].width, 220.0);
        assert_eq!(original_frames["outer"].width, 268.0);
        for node in &mut provider.nodes {
            node.x += 500.0;
            node.y -= 100.0;
        }
        for edge in &mut provider.edges {
            for point in &mut edge.points {
                point.x += 500.0;
                point.y -= 100.0;
            }
        }
        let translated_frames = frames(&graph, &provider);
        for (id, original) in original_frames {
            let translated = translated_frames[id];
            assert_eq!(translated.x, original.x + 500.0);
            assert_eq!(translated.y, original.y - 100.0);
            assert_eq!(translated.width, original.width);
            assert_eq!(translated.height, original.height);
        }
    }

    #[test]
    fn frame_terminal_anchors_preserve_borders_without_including_external_route_lanes() {
        for outgoing in [false, true] {
            let (mut graph, mut provider) = scene(200.0, 60.0, 100.0);
            graph.nodes.push(leaf("ext", None));
            provider.nodes.push(box_at("ext", 5.0, -60.0, 14.0, 14.0));
            if outgoing {
                edge(
                    &mut graph,
                    &mut provider,
                    "g",
                    "ext",
                    &[(12.0, 0.0), (500.0, -10.0), (500.0, -40.0)],
                );
            } else {
                edge(
                    &mut graph,
                    &mut provider,
                    "ext",
                    "g",
                    &[(500.0, -40.0), (500.0, -10.0), (12.0, 0.0)],
                );
            }
            let before = provider.clone();
            let frame = frames(&graph, &provider)["g"];
            assert_eq!(frame.x - frame.width / 2.0, 0.0);
            assert_eq!(frame.width, 184.0);
            assert_eq!(provider, before);
        }
    }

    #[test]
    fn group_endpoint_is_not_its_own_descendant_for_internal_lane_accounting() {
        let (mut graph, mut provider) = scene(200.0, 60.0, 100.0);
        edge(
            &mut graph,
            &mut provider,
            "g",
            "n",
            &[(12.0, 0.0), (500.0, 20.0), (60.0, 60.0)],
        );
        assert_eq!(frames(&graph, &provider)["g"].width, 184.0);
        let (mut graph, mut provider) = scene(200.0, 60.0, 100.0);
        edge(
            &mut graph,
            &mut provider,
            "g",
            "g",
            &[(12.0, 0.0), (500.0, 20.0), (80.0, 0.0)],
        );
        assert_eq!(frames(&graph, &provider)["g"].width, 184.0);
    }

    #[test]
    fn deep_hierarchies_use_linear_metered_storage_without_recursion() {
        let depth = 2048;
        let mut graph = elk::Graph::default();
        let mut provider = elk::LayoutResult::default();
        for index in (0..depth).rev() {
            let id = format!("g{index}");
            let parent = (index > 0).then(|| format!("g{}", index - 1));
            graph.nodes.push(group(&id, parent.as_deref()));
            provider.nodes.push(box_at(&id, 0.0, 0.0, 200.0, 148.0));
        }
        graph
            .nodes
            .push(leaf("leaf", Some(&format!("g{}", depth - 1))));
        provider.nodes.push(box_at("leaf", 24.0, 48.0, 100.0, 76.0));
        let mut measured = work();
        let projected =
            drawing_group_frames(&graph, &provider, |_| 0.0, &mut Some(&mut measured)).unwrap();
        assert_eq!(projected.len(), depth);
        assert!(measured.adapter_work() < 32 * graph.nodes.len());
        let order = parent_first_order(&graph.nodes, &mut Some(&mut work())).unwrap();
        assert_eq!(graph.nodes[order[0]].id, "g0");
        assert_eq!(graph.nodes[*order.last().unwrap()].id, "leaf");
    }

    #[test]
    fn shared_passes_honor_work_rejection_and_cancellation_without_provider_mutation() {
        let (graph, provider) = scene(200.0, 24.0, 100.0);
        let before = provider.clone();
        assert!(matches!(
            drawing_group_frames(&graph, &provider, |_| 0.0, &mut Some(&mut limited_work(1))),
            Err(crate::Error::ResourceLimitExceeded(_))
        ));
        assert_eq!(provider, before);
        let control = merman_core::OperationControl::new();
        control.cancel();
        let mut cancelled =
            OperationLayoutWorkControl::new(Arc::new(OperationWorkMeter::new_with_control(
                RenderResourcePolicy::unbounded_for_trusted_input(),
                control,
            )));
        assert!(matches!(
            drawing_group_frames(&graph, &provider, |_| 0.0, &mut Some(&mut cancelled)),
            Err(crate::Error::Cancelled(_))
        ));
        assert!(matches!(
            parent_first_order(&graph.nodes, &mut Some(&mut cancelled)),
            Err(crate::Error::Cancelled(_))
        ));
        let mut nodes = vec![leaf("a", None), leaf("b", None)];
        assert!(matches!(
            apply_cyclic_entry_constraints(
                &mut nodes,
                [("a", "b"), ("b", "a")].into_iter(),
                &mut Some(&mut cancelled)
            ),
            Err(crate::Error::Cancelled(_))
        ));
        assert!(nominees(&nodes).is_empty());
    }

    #[test]
    fn malformed_parent_cycles_are_rejected_before_hierarchy_queries() {
        let nodes = vec![group("a", Some("b")), group("b", Some("a"))];
        assert!(matches!(
            parent_first_order(&nodes, &mut Some(&mut work())),
            Err(crate::Error::InvalidModel { .. })
        ));
    }
}
