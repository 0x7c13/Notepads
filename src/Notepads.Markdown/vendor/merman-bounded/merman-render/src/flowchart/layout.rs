// Modifications Copyright (c) 2026, Jiaqi (0x7c13) Liu. See project LICENSE.
use crate::dagre::self_loop::compact_self_loop_geometry;
use crate::layout_work::DugongOperationWorkControl as DagreOperationWorkControl;
use crate::math::MathRenderer;
use crate::model::{
    FlowchartLayout, LayoutCluster, LayoutEdge, LayoutLabel, LayoutNode, LayoutPoint,
};
use crate::resources::OperationWorkMeter;
#[cfg(test)]
use crate::resources::RenderResourcePolicy;
use crate::text::{TextMeasurer, TextStyle, WrapMode};
use crate::{Error, Result};
use dugong::graphlib::{Graph, GraphOptions, is_javascript_array_index};
use dugong::{EdgeLabel, GraphLabel, LabelPos, NodeLabel, RankDir};
use indexmap::IndexMap;
#[cfg(test)]
use merman_core::diagrams::flowchart::{FlowEdgeMarker, FlowEdgeStroke, FlowEdgeVisibility};
use merman_core::{MermaidConfig, geom::Size};
use serde_json::Value;
use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use super::config::{FlowchartConfigView, FlowchartLayoutSettings};
use super::label::compute_bounds_controlled;
use super::node::{NodeLayoutDimensionsRequest, node_layout_dimensions};
use super::{
    FlowEdge, FlowSubgraph, FlowchartModel, FlowchartRenderContext, FlowchartRenderModelRef,
};
use super::{
    FlowchartLabelMetricsRequest, FlowchartSvgLabelOwner, FlowchartSvgLabelSidecarBuilder,
    FlowchartSvgWidthMode, flowchart_apply_html_node_class_box_metrics,
    flowchart_effective_edge_label_text_style, flowchart_effective_text_style_for_classes,
    flowchart_effective_text_style_for_node_classes, flowchart_node_svg_width_mode,
    measure_flowchart_svg_label_for_layout,
    measure_flowchart_svg_label_for_layout_with_metrics_style,
};

type FlowSubgraphIndex<'a> = HashMap<&'a str, &'a FlowSubgraph>;

fn rank_dir_from_flow(direction: &str) -> RankDir {
    match direction.trim().to_uppercase().as_str() {
        "TB" | "TD" => RankDir::TB,
        "BT" => RankDir::BT,
        "LR" => RankDir::LR,
        "RL" => RankDir::RL,
        _ => RankDir::TB,
    }
}

fn normalize_dir(s: &str) -> String {
    s.trim().to_uppercase()
}

fn toggled_dir(parent: &str) -> String {
    let parent = normalize_dir(parent);
    if parent == "TB" || parent == "TD" {
        "LR".to_string()
    } else {
        "TB".to_string()
    }
}

fn flow_dir_from_rankdir(rankdir: RankDir) -> &'static str {
    match rankdir {
        RankDir::TB => "TB",
        RankDir::BT => "BT",
        RankDir::LR => "LR",
        RankDir::RL => "RL",
    }
}

fn effective_cluster_dir(sg: &FlowSubgraph, parent_dir: &str, inherit_dir: bool) -> String {
    if let Some(dir) = sg.dir.as_deref().map(str::trim).filter(|s| !s.is_empty()) {
        return normalize_dir(dir);
    }
    if inherit_dir {
        return normalize_dir(parent_dir);
    }
    toggled_dir(parent_dir)
}

fn compute_effective_dir_by_id(
    subgraphs_in_order: &[FlowSubgraph],
    subgraphs_by_id: &FlowSubgraphIndex<'_>,
    g: &Graph<NodeLabel, EdgeLabel, GraphLabel>,
    diagram_dir: &str,
    inherit_dir: bool,
    work_control: &mut DagreOperationWorkControl,
) -> Result<HashMap<String, String>> {
    fn compute_one_iterative(
        id: &str,
        subgraphs_by_id: &FlowSubgraphIndex<'_>,
        g: &Graph<NodeLabel, EdgeLabel, GraphLabel>,
        diagram_dir: &str,
        inherit_dir: bool,
        memo: &mut HashMap<String, String>,
        work_control: &mut DagreOperationWorkControl,
    ) -> Result<()> {
        if memo.contains_key(id) {
            return Ok(());
        }

        let mut path: Vec<String> = Vec::new();
        let mut seen: HashSet<String> = HashSet::new();
        let mut cur = id.to_string();
        let mut parent_dir = loop {
            work_control.charge_adapter(1)?;
            if let Some(dir) = memo.get(&cur) {
                break dir.clone();
            }
            if !seen.insert(cur.clone()) {
                let dir = toggled_dir(diagram_dir);
                memo.insert(cur.clone(), dir.clone());
                break dir;
            }

            path.push(cur.clone());
            let Some(parent) = g.parent(&cur).filter(|p| subgraphs_by_id.contains_key(*p)) else {
                break normalize_dir(diagram_dir);
            };
            cur = parent.to_string();
        };

        for node in path.into_iter().rev() {
            work_control.charge_adapter(1)?;
            if let Some(dir) = memo.get(&node) {
                parent_dir = dir.clone();
                continue;
            }

            let dir = subgraphs_by_id
                .get(node.as_str())
                .map(|sg| effective_cluster_dir(sg, &parent_dir, inherit_dir))
                .unwrap_or_else(|| toggled_dir(&parent_dir));
            memo.insert(node, dir.clone());
            parent_dir = dir;
        }
        Ok(())
    }

    let mut memo: HashMap<String, String> = HashMap::new();
    // `std::collections::HashMap` seeds each map independently, so walking its keys makes the
    // amount of memoized parent-path work vary between otherwise identical renders. Mermaid's
    // source order is already available here; use it for deterministic work accounting and error
    // timing while retaining the map for lookups.
    for subgraph in subgraphs_in_order {
        work_control.charge_adapter(1)?;
        compute_one_iterative(
            &subgraph.id,
            subgraphs_by_id,
            g,
            diagram_dir,
            inherit_dir,
            &mut memo,
            work_control,
        )?;
    }
    Ok(memo)
}

fn dir_to_rankdir(dir: &str) -> RankDir {
    match normalize_dir(dir).as_str() {
        "TB" | "TD" => RankDir::TB,
        "BT" => RankDir::BT,
        "LR" => RankDir::LR,
        "RL" => RankDir::RL,
        _ => RankDir::TB,
    }
}

const SELF_LOOP_ID_EXTRA: &str = "selfLoopId";
const SELF_LOOP_NODE_EXTRA: &str = "selfLoopNode";
const SELF_LOOP_ORDER_EXTRA: &str = "selfLoopOrder";

#[derive(Debug, Clone, PartialEq, Eq)]
struct FlowchartSelfLoopSegmentMeta {
    logical_edge_id: String,
    node_id: String,
    order: u8,
}

struct FlowchartLayoutEdgeCandidate {
    edge: LayoutEdge,
    self_loop: Option<FlowchartSelfLoopSegmentMeta>,
}

fn flowchart_layout_edge_key(
    edge: &FlowEdge,
    self_loop: Option<&FlowchartSelfLoopSegmentMeta>,
) -> String {
    match self_loop {
        Some(meta) => format!("{}-cyclic-special-{}", meta.node_id, meta.order),
        None => edge.id.clone(),
    }
}

fn annotate_flowchart_self_loop_segment(
    label: &mut EdgeLabel,
    meta: &FlowchartSelfLoopSegmentMeta,
) {
    label.extras.insert(
        SELF_LOOP_ID_EXTRA.to_string(),
        Value::String(meta.logical_edge_id.clone()),
    );
    label.extras.insert(
        SELF_LOOP_NODE_EXTRA.to_string(),
        Value::String(meta.node_id.clone()),
    );
    label
        .extras
        .insert(SELF_LOOP_ORDER_EXTRA.to_string(), Value::from(meta.order));
}

fn flowchart_self_loop_segment_meta(label: &EdgeLabel) -> Option<FlowchartSelfLoopSegmentMeta> {
    let logical_edge_id = label.extras.get(SELF_LOOP_ID_EXTRA)?.as_str()?.to_string();
    let node_id = label
        .extras
        .get(SELF_LOOP_NODE_EXTRA)?
        .as_str()?
        .to_string();
    let order = u8::try_from(label.extras.get(SELF_LOOP_ORDER_EXTRA)?.as_u64()?).ok()?;
    Some(FlowchartSelfLoopSegmentMeta {
        logical_edge_id,
        node_id,
        order,
    })
}

fn merge_flowchart_self_loop_segments(
    model_edges: &[FlowEdge],
    layout_nodes: &[LayoutNode],
    rankdir: &str,
    layout_edges: Vec<FlowchartLayoutEdgeCandidate>,
) -> Vec<LayoutEdge> {
    let mut output = Vec::with_capacity(layout_edges.len());
    let mut segments_by_id: IndexMap<String, Vec<(LayoutEdge, FlowchartSelfLoopSegmentMeta)>> =
        IndexMap::new();
    for candidate in layout_edges {
        if let Some(meta) = candidate.self_loop {
            segments_by_id
                .entry(meta.logical_edge_id.clone())
                .or_default()
                .push((candidate.edge, meta));
        } else {
            output.push(candidate.edge);
        }
    }

    let nodes_by_id: HashMap<&str, &LayoutNode> = layout_nodes
        .iter()
        .map(|node| (node.id.as_str(), node))
        .collect();
    let mut model_edges_by_id: HashMap<&str, &FlowEdge> = HashMap::with_capacity(model_edges.len());
    for edge in model_edges {
        model_edges_by_id.entry(edge.id.as_str()).or_insert(edge);
    }

    for (logical_edge_id, mut segments) in segments_by_id {
        if segments.len() != 3 {
            output.extend(segments.into_iter().map(|(edge, _)| edge));
            continue;
        }
        segments.sort_by_key(|(_, meta)| meta.order);
        if segments.iter().map(|(_, meta)| meta.order).ne([0_u8, 1, 2]) {
            output.extend(segments.into_iter().map(|(edge, _)| edge));
            continue;
        }

        let (first, first_meta) = &segments[0];
        let (middle, _) = &segments[1];
        let (last, _) = &segments[2];
        let Some(original) = model_edges_by_id.get(logical_edge_id.as_str()).copied() else {
            output.extend(segments.into_iter().map(|(edge, _)| edge));
            continue;
        };
        let node_id = first_meta.node_id.as_str();
        let Some(node) = nodes_by_id.get(node_id).copied() else {
            output.extend(segments.into_iter().map(|(edge, _)| edge));
            continue;
        };

        let helper_ids = [
            format!("{node_id}---{node_id}---1"),
            format!("{node_id}---{node_id}---2"),
        ];
        let mut hints = helper_ids
            .iter()
            .filter_map(|id| nodes_by_id.get(id.as_str()).copied())
            .map(|helper| LayoutPoint {
                x: helper.x,
                y: helper.y,
            })
            .collect::<Vec<_>>();
        if hints.is_empty() {
            hints.extend(
                [first, middle, last]
                    .into_iter()
                    .flat_map(|edge| edge.points.iter().cloned()),
            );
        }

        let label_width = middle.label.as_ref().map_or(0.0, |label| label.width);
        let label_height = middle.label.as_ref().map_or(0.0, |label| label.height);
        let geometry = compact_self_loop_geometry(
            &LayoutPoint {
                x: node.x,
                y: node.y,
            },
            Size::new(node.width, node.height),
            dir_to_rankdir(rankdir),
            &hints,
            0.0,
            Size::new(label_width, label_height),
        );
        let label = middle.label.as_ref().map(|_| LayoutLabel {
            x: geometry.label_center.x,
            y: geometry.label_center.y,
            width: label_width,
            height: label_height,
        });

        output.push(LayoutEdge {
            id: original.id.clone(),
            from: original.from.clone(),
            to: original.to.clone(),
            from_cluster: first
                .from_cluster
                .clone()
                .or_else(|| middle.from_cluster.clone())
                .or_else(|| last.from_cluster.clone()),
            to_cluster: first
                .to_cluster
                .clone()
                .or_else(|| middle.to_cluster.clone())
                .or_else(|| last.to_cluster.clone()),
            points: geometry.points,
            label,
            start_label_left: None,
            start_label_right: None,
            end_label_left: None,
            end_label_right: None,
            start_marker: None,
            end_marker: None,
            stroke_dasharray: None,
        });
    }

    output
}

fn edge_label_is_non_empty(model: &FlowchartRenderModelRef<'_>, edge: &FlowEdge) -> bool {
    model
        .edge_label_for_render(edge)
        .is_some_and(|text| !crate::flowchart::flowchart_label_is_empty_for_render(text))
}

fn lowest_common_parent(
    graph: &Graph<NodeLabel, EdgeLabel, GraphLabel>,
    a: &str,
    b: &str,
    work_control: &mut DagreOperationWorkControl,
) -> Result<Option<String>> {
    if !graph.options().compound {
        return Ok(None);
    }

    let mut ancestors = HashSet::new();
    let mut current = graph.parent(a);
    while let Some(parent) = current {
        work_control.charge_adapter(1)?;
        ancestors.insert(parent.to_string());
        current = graph.parent(parent);
    }

    let mut current = graph.parent(b);
    while let Some(parent) = current {
        work_control.charge_adapter(1)?;
        if ancestors.contains(parent) {
            return Ok(Some(parent.to_string()));
        }
        current = graph.parent(parent);
    }

    Ok(None)
}

fn extract_descendants(
    id: &str,
    graph: &Graph<NodeLabel, EdgeLabel, GraphLabel>,
    work_control: &mut DagreOperationWorkControl,
) -> Result<Vec<String>> {
    let initial_children = child_id_snapshot_work_upper_bound(graph, id, work_control)?;
    work_control.charge_adapter(initial_children)?;
    let mut out = Vec::new();
    let mut visited = HashSet::new();
    let mut stack = graph
        .children_iter(id)
        .map(str::to_string)
        .collect::<Vec<_>>();
    while let Some(node) = stack.pop() {
        work_control.charge_adapter(1)?;
        if !visited.insert(node.clone()) {
            continue;
        }
        let child_count = child_id_snapshot_work_upper_bound(graph, &node, work_control)?;
        work_control.charge_adapter(child_count)?;
        stack.extend(graph.children_iter(&node).map(str::to_string));
        out.push(node);
    }

    Ok(out)
}

fn edge_in_cluster(
    edge: &dugong::graphlib::EdgeKey,
    cluster_id: &str,
    descendants: &HashMap<String, Vec<String>>,
    work_control: &mut DagreOperationWorkControl,
) -> Result<bool> {
    if edge.v == cluster_id || edge.w == cluster_id {
        return Ok(false);
    }
    let Some(cluster_descendants) = descendants.get(cluster_id) else {
        return Ok(false);
    };
    for descendant in cluster_descendants {
        work_control.charge_adapter(1)?;
        if descendant == &edge.v {
            return Ok(true);
        }
    }
    for descendant in cluster_descendants {
        work_control.charge_adapter(1)?;
        if descendant == &edge.w {
            return Ok(true);
        }
    }
    Ok(false)
}

#[derive(Debug, Clone)]
struct FlowchartClusterDbEntry {
    anchor_id: String,
    external_connections: bool,
}

fn flowchart_find_common_edges(
    graph: &Graph<NodeLabel, EdgeLabel, GraphLabel>,
    id1: &str,
    id2: &str,
    work_control: &mut DagreOperationWorkControl,
) -> Result<Vec<(String, String)>> {
    let edge_snapshot_work =
        work_control.checked_add(graph.edge_slot_count(), graph.edge_count())?;
    work_control.charge_adapter(edge_snapshot_work)?;
    let edges1 = graph
        .edges()
        .filter(|edge| edge.v == id1 || edge.w == id1)
        .map(|edge| (edge.v.clone(), edge.w.clone()))
        .collect::<Vec<_>>();
    work_control.charge_adapter(edge_snapshot_work)?;
    let edges2 = graph
        .edges()
        .filter(|edge| edge.v == id2 || edge.w == id2)
        .map(|edge| (edge.v.clone(), edge.w.clone()))
        .collect::<Vec<_>>();

    work_control.charge_adapter(edges1.len())?;
    let edges1_prim = edges1
        .into_iter()
        .map(|(v, w)| {
            (
                if v == id1 { id2.to_string() } else { v },
                // Mermaid's `findCommonEdges(...)` has an asymmetry here: it maps the `w` side
                // back to `id1` rather than `id2`.
                if w == id1 { id1.to_string() } else { w },
            )
        })
        .collect::<Vec<_>>();

    let mut out = Vec::new();
    for edge in edges1_prim {
        let mut found = false;
        for candidate in &edges2 {
            work_control.charge_adapter(1)?;
            if candidate == &edge {
                found = true;
                break;
            }
        }
        if found {
            out.push(edge);
        }
    }
    Ok(out)
}

fn flowchart_find_non_cluster_child(
    id: &str,
    graph: &Graph<NodeLabel, EdgeLabel, GraphLabel>,
    cluster_id: &str,
    work_control: &mut DagreOperationWorkControl,
) -> Result<Option<String>> {
    let child_count = graph.child_count(id);
    if child_count == 0 {
        return Ok(Some(id.to_string()));
    }
    work_control.charge_adapter(child_id_snapshot_work_upper_bound(graph, id, work_control)?)?;
    let mut reserve: Option<String> = None;
    let mut visited: HashSet<String> = HashSet::new();
    let mut stack = graph
        .children(id)
        .into_iter()
        .rev()
        .map(str::to_string)
        .collect::<Vec<_>>();

    while let Some(node) = stack.pop() {
        work_control.charge_adapter(1)?;
        if !visited.insert(node.clone()) {
            continue;
        }
        let child_count = graph.child_count(&node);
        if child_count != 0 {
            work_control.charge_adapter(child_id_snapshot_work_upper_bound(
                graph,
                &node,
                work_control,
            )?)?;
            stack.extend(graph.children(&node).into_iter().rev().map(str::to_string));
            continue;
        }
        if !flowchart_find_common_edges(graph, cluster_id, &node, work_control)?.is_empty() {
            reserve = Some(node);
        } else {
            return Ok(Some(node));
        }
    }

    Ok(reserve)
}

fn flowchart_is_node_in_extractable_cluster(
    graph: &Graph<NodeLabel, EdgeLabel, GraphLabel>,
    node_id: &str,
    root_id: &str,
    cluster_db: &HashMap<String, FlowchartClusterDbEntry>,
    work_control: &mut DagreOperationWorkControl,
) -> Result<bool> {
    let mut parent = graph.parent(node_id);
    while let Some(parent_id) = parent {
        work_control.charge_adapter(1)?;
        if parent_id == root_id {
            break;
        }
        if cluster_db
            .get(parent_id)
            .is_some_and(|entry| !entry.external_connections)
        {
            return Ok(true);
        }
        parent = graph.parent(parent_id);
    }
    Ok(false)
}

fn flowchart_find_safe_anchor_node(
    graph: &Graph<NodeLabel, EdgeLabel, GraphLabel>,
    cluster_id: &str,
    excluded_cluster: &str,
    descendants: &HashMap<String, Vec<String>>,
    cluster_db: &HashMap<String, FlowchartClusterDbEntry>,
    work_control: &mut DagreOperationWorkControl,
) -> Result<Option<String>> {
    work_control.charge_adapter(graph.child_order_slot_count(cluster_id))?;
    for child in graph.children_iter(cluster_id) {
        if child == excluded_cluster {
            continue;
        }
        if let Some(excluded_descendants) = descendants.get(excluded_cluster) {
            let mut excluded = false;
            for descendant in excluded_descendants {
                work_control.charge_adapter(1)?;
                if descendant == child {
                    excluded = true;
                    break;
                }
            }
            if excluded {
                continue;
            }
        }

        let Some(candidate) =
            flowchart_find_non_cluster_child(child, graph, cluster_id, work_control)?
        else {
            continue;
        };
        if !flowchart_is_node_in_extractable_cluster(
            graph,
            &candidate,
            cluster_id,
            cluster_db,
            work_control,
        )? {
            return Ok(Some(candidate));
        }
    }
    Ok(None)
}

fn adjust_flowchart_clusters_and_edges(
    graph: &mut Graph<NodeLabel, EdgeLabel, GraphLabel>,
    work_control: &mut DagreOperationWorkControl,
) -> Result<std::collections::HashMap<String, bool>> {
    use serde_json::Value;

    fn is_descendant(
        node_id: &str,
        cluster_id: &str,
        descendants: &HashMap<String, Vec<String>>,
        work_control: &mut DagreOperationWorkControl,
    ) -> Result<bool> {
        let Some(cluster_descendants) = descendants.get(cluster_id) else {
            return Ok(false);
        };
        for descendant in cluster_descendants {
            work_control.charge_adapter(1)?;
            if descendant == node_id {
                return Ok(true);
            }
        }
        Ok(false)
    }

    let node_snapshot_work = node_id_snapshot_work_upper_bound(graph, work_control)?;
    work_control.charge_adapter(node_snapshot_work)?;
    let node_ids = graph.node_ids();
    let mut descendants: HashMap<String, Vec<String>> = HashMap::new();
    let mut cluster_db: std::collections::HashMap<String, FlowchartClusterDbEntry> =
        std::collections::HashMap::new();
    let mut cluster_ids = Vec::new();
    work_control.charge_adapter(node_ids.len())?;
    for id in node_ids {
        if graph.child_count(&id) == 0 {
            continue;
        }
        descendants.insert(id.clone(), extract_descendants(&id, graph, work_control)?);
        let anchor_id = flowchart_find_non_cluster_child(&id, graph, &id, work_control)?
            .unwrap_or_else(|| id.clone());
        cluster_ids.push(id.clone());
        cluster_db.insert(
            id,
            FlowchartClusterDbEntry {
                anchor_id,
                external_connections: false,
            },
        );
    }

    for id in &cluster_ids {
        let mut has_external = false;
        for edge in graph.edges() {
            work_control.charge_adapter(1)?;
            let v_is_descendant = is_descendant(&edge.v, id, &descendants, work_control)?;
            let w_is_descendant = is_descendant(&edge.w, id, &descendants, work_control)?;
            if v_is_descendant ^ w_is_descendant {
                has_external = true;
                break;
            }
        }
        if let Some(entry) = cluster_db.get_mut(id) {
            entry.external_connections = has_external;
        }
    }

    for id in &cluster_ids {
        let Some(non_cluster_child) = cluster_db.get(id).map(|e| e.anchor_id.clone()) else {
            continue;
        };
        let parent = graph.parent(&non_cluster_child);
        if parent.is_some_and(|p| p != id.as_str())
            && parent.is_some_and(|p| cluster_db.contains_key(p))
            && parent.is_some_and(|p| !cluster_db.get(p).is_some_and(|e| e.external_connections))
            && let Some(p) = parent
            && let Some(entry) = cluster_db.get_mut(id)
        {
            entry.anchor_id = p.to_string();
        }

        work_control.charge_adapter(graph.edge_count())?;
        let mut has_direct_outgoing_edge = false;
        for edge in graph.edges() {
            has_direct_outgoing_edge |= edge.v == *id;
        }
        let needs_safe_anchor = cluster_db
            .get(id)
            .is_some_and(|entry| entry.external_connections)
            && has_direct_outgoing_edge
            && flowchart_is_node_in_extractable_cluster(
                graph,
                &non_cluster_child,
                id,
                &cluster_db,
                work_control,
            )?;
        if needs_safe_anchor
            && let Some(excluded_cluster) = graph.parent(&non_cluster_child)
            && let Some(safe_anchor) = flowchart_find_safe_anchor_node(
                graph,
                id,
                excluded_cluster,
                &descendants,
                &cluster_db,
                work_control,
            )?
            && let Some(entry) = cluster_db.get_mut(id)
        {
            entry.anchor_id = safe_anchor;
        }
    }

    fn get_anchor_id(
        id: &str,
        cluster_db: &std::collections::HashMap<String, FlowchartClusterDbEntry>,
    ) -> String {
        let Some(entry) = cluster_db.get(id) else {
            return id.to_string();
        };
        if !entry.external_connections {
            return id.to_string();
        }
        entry.anchor_id.clone()
    }

    let edge_snapshot_work =
        work_control.checked_add(graph.edge_slot_count(), graph.edge_count())?;
    work_control.charge_adapter(edge_snapshot_work)?;
    let edge_keys = graph.edge_keys();
    for ek in edge_keys {
        if !cluster_db.contains_key(&ek.v) && !cluster_db.contains_key(&ek.w) {
            continue;
        }

        let Some(mut edge_label) = graph.edge_by_key(&ek).cloned() else {
            continue;
        };

        let v = get_anchor_id(&ek.v, &cluster_db);
        let w = get_anchor_id(&ek.w, &cluster_db);

        // Match Mermaid `adjustClustersAndEdges`: edges that touch cluster nodes are removed and
        // re-inserted even when their endpoints do not change. This affects edge iteration order
        // and therefore cycle-breaking determinism in Dagre's acyclic pass.
        let _ = graph.remove_edge_key(&ek);

        if v != ek.v {
            if let Some(parent) = graph.parent(&v)
                && let Some(entry) = cluster_db.get_mut(parent)
            {
                entry.external_connections = true;
            }
            edge_label
                .extras
                .insert("fromCluster".to_string(), Value::String(ek.v.clone()));
        }

        if w != ek.w {
            if let Some(parent) = graph.parent(&w)
                && let Some(entry) = cluster_db.get_mut(parent)
            {
                entry.external_connections = true;
            }
            edge_label
                .extras
                .insert("toCluster".to_string(), Value::String(ek.w.clone()));
        }

        graph.set_edge_named(v, w, ek.name, Some(edge_label));
    }

    Ok(cluster_db
        .into_iter()
        .map(|(id, entry)| (id, entry.external_connections))
        .collect())
}

fn checked_n_log_n(value: usize, work_control: &DagreOperationWorkControl) -> Result<usize> {
    if value <= 1 {
        return Ok(0);
    }
    let passes = usize::BITS as usize - (value - 1).leading_zeros() as usize;
    work_control.checked_mul(value, passes)
}

fn ordered_key_update_work_upper_bound(
    entry_bound: usize,
    update_count: usize,
    work_control: &DagreOperationWorkControl,
) -> Result<usize> {
    let height = if entry_bound <= 1 {
        1
    } else {
        let log = usize::BITS as usize - (entry_bound - 1).leading_zeros() as usize;
        work_control.checked_add(log, 1)?
    };
    work_control.checked_mul(update_count, height)
}

fn node_id_snapshot_work_upper_bound(
    graph: &Graph<NodeLabel, EdgeLabel, GraphLabel>,
    work_control: &DagreOperationWorkControl,
) -> Result<usize> {
    work_control.checked_add(graph.node_order_slot_count(), graph.node_count())
}

fn child_id_snapshot_work_upper_bound(
    graph: &Graph<NodeLabel, EdgeLabel, GraphLabel>,
    parent: &str,
    work_control: &DagreOperationWorkControl,
) -> Result<usize> {
    work_control.checked_add(
        graph.child_order_slot_count(parent),
        graph.child_count(parent),
    )
}

fn charge_node_insertion_ordering(
    graph: &Graph<NodeLabel, EdgeLabel, GraphLabel>,
    node_id: &str,
    work_control: &mut DagreOperationWorkControl,
) -> Result<()> {
    if graph.has_node(node_id) || !is_javascript_array_index(node_id) {
        return Ok(());
    }
    let final_node_count = work_control.checked_add(graph.node_count(), 1)?;
    let ordered_maps = 1 + usize::from(graph.options().compound);
    let work = ordered_key_update_work_upper_bound(final_node_count, ordered_maps, work_control)?;
    work_control.charge_adapter(work)
}

fn charge_edge_endpoint_insertions(
    graph: &Graph<NodeLabel, EdgeLabel, GraphLabel>,
    v: &str,
    w: &str,
    work_control: &mut DagreOperationWorkControl,
) -> Result<()> {
    let v_missing = !graph.has_node(v);
    let w_missing = w != v && !graph.has_node(w);
    let missing_count = usize::from(v_missing) + usize::from(w_missing);
    if missing_count == 0 {
        return Ok(());
    }

    let numeric_count = usize::from(v_missing && is_javascript_array_index(v))
        + usize::from(w_missing && is_javascript_array_index(w));
    let final_node_count = work_control.checked_add(graph.node_count(), missing_count)?;
    let ordered_updates =
        work_control.checked_mul(numeric_count, 1 + usize::from(graph.options().compound))?;
    let ordered_work =
        ordered_key_update_work_upper_bound(final_node_count, ordered_updates, work_control)?;
    work_control.charge_adapter(work_control.checked_add(missing_count, ordered_work)?)
}

fn remove_node_work_upper_bound(
    graph: &Graph<NodeLabel, EdgeLabel, GraphLabel>,
    node_id: &str,
    work_control: &DagreOperationWorkControl,
) -> Result<usize> {
    let node_slots = graph.node_slot_count();
    let edge_slots = graph.edge_slot_count();
    let incident_slots = if graph.options().directed {
        work_control.checked_mul(edge_slots, 2)?
    } else {
        edge_slots
    };
    let cache_and_tombstone_work = work_control.checked_add(
        work_control.checked_mul(node_slots, 7)?,
        work_control.checked_mul(edge_slots, 6)?,
    )?;
    let incident_work = work_control.checked_add(
        work_control.checked_mul(incident_slots, 2)?,
        checked_n_log_n(incident_slots, work_control)?,
    )?;
    let compound = graph.options().compound;
    let previous_order_slots = if compound {
        graph.parent(node_id).map_or_else(
            || graph.root_child_order_slot_count(),
            |parent| graph.child_order_slot_count(parent),
        )
    } else {
        0
    };
    let promoted_order_slots = if compound {
        graph.child_order_slot_count(node_id)
    } else {
        0
    };
    let object_order_scan_work = work_control.checked_add(
        graph.node_order_slot_count(),
        work_control.checked_add(previous_order_slots, promoted_order_slots)?,
    )?;
    let numeric_node = usize::from(is_javascript_array_index(node_id));
    let promoted_numeric = if compound {
        graph.array_index_child_count(node_id)
    } else {
        0
    };
    let numeric_updates = work_control.checked_add(
        work_control.checked_mul(numeric_node, 1 + usize::from(compound))?,
        promoted_numeric,
    )?;
    let ordered_map_work = ordered_key_update_work_upper_bound(
        graph.array_index_node_count(),
        numeric_updates,
        work_control,
    )?;
    work_control.checked_add(
        work_control.checked_add(
            work_control.checked_add(cache_and_tombstone_work, incident_work)?,
            object_order_scan_work,
        )?,
        ordered_map_work,
    )
}

fn unparented_parent_batch_work_upper_bound(
    node_slots: usize,
    existing_parent_count: usize,
    assignment_count: usize,
    numeric_assignment_count: usize,
    work_control: &DagreOperationWorkControl,
) -> Result<usize> {
    if assignment_count == 0 {
        return Ok(0);
    }
    let height = if node_slots <= 1 {
        0
    } else {
        usize::BITS as usize - (node_slots - 1).leading_zeros() as usize
    };
    // Union by rank bounds every pre-compression find to `height`. Account for two traversals per
    // find, two finds per forest link, vector initialization/reservation, and one replay pass.
    let find_work = work_control.checked_mul(work_control.checked_add(height, 1)?, 4)?;
    let forest_items = work_control.checked_add(existing_parent_count, assignment_count)?;
    let union_work = work_control.checked_mul(forest_items, find_work)?;
    let linear_work = work_control.checked_add(
        work_control.checked_mul(node_slots, 6)?,
        work_control.checked_mul(assignment_count, 2)?,
    )?;
    // Each array-index child requires one root-bucket removal and one target-bucket insertion.
    // Callers derive this count while already traversing assignments in pinned Graphlib order, so
    // ordinary IDs do not pay a logarithmic ordered-map term that cannot occur.
    let ordered_updates = work_control.checked_mul(numeric_assignment_count, 2)?;
    let ordered_work =
        ordered_key_update_work_upper_bound(node_slots, ordered_updates, work_control)?;
    work_control.checked_add(
        work_control.checked_add(union_work, linear_work)?,
        ordered_work,
    )
}

#[derive(Debug, Clone, Copy, Default)]
struct UnparentedParentBatchShape {
    existing_parent_count: usize,
    numeric_assignment_count: usize,
}

fn apply_unparented_parent_assignments(
    graph: &mut Graph<NodeLabel, EdgeLabel, GraphLabel>,
    assignments: &[(usize, usize)],
    shape: UnparentedParentBatchShape,
    work_control: &mut DagreOperationWorkControl,
) -> Result<()> {
    if assignments.is_empty() {
        return Ok(());
    }
    let work = unparented_parent_batch_work_upper_bound(
        graph.node_slot_count(),
        shape.existing_parent_count,
        assignments.len(),
        shape.numeric_assignment_count,
        work_control,
    )?;
    work_control.charge_adapter(work)?;
    match graph.try_set_unparented_parents_ix(assignments) {
        Ok(_) => Ok(()),
        Err(dugong::graphlib::GraphError::ParentCycle {
            child_ix,
            parent_ix,
        }) => {
            let child = graph.node_id_by_ix(child_ix).unwrap_or("<removed>");
            let parent = graph.node_id_by_ix(parent_ix).unwrap_or("<removed>");
            Err(Error::InvalidModel {
                message: format!("Setting {parent} as parent of {child} would create a cycle"),
            })
        }
        Err(error) => Err(Error::InvalidModel {
            message: error.to_string(),
        }),
    }
}

fn set_parent_controlled(
    graph: &mut Graph<NodeLabel, EdgeLabel, GraphLabel>,
    child: &str,
    parent: &str,
    work_control: &mut DagreOperationWorkControl,
) -> Result<()> {
    let child_exists = graph.has_node(child);
    let parent_exists = graph.has_node(parent);
    let inserted_nodes = if child == parent {
        usize::from(!child_exists)
    } else {
        usize::from(!child_exists) + usize::from(!parent_exists)
    };
    let final_node_slots = work_control.checked_add(graph.node_slot_count(), inserted_nodes)?;
    let child_is_numeric = is_javascript_array_index(child);
    let relation_changes =
        !child_exists || graph.parent(child) != Some(parent) || !child_is_numeric;
    let previous_order_slots = if relation_changes && !child_is_numeric {
        if child_exists {
            graph.parent(child).map_or_else(
                || graph.root_child_order_slot_count(),
                |previous_parent| graph.child_order_slot_count(previous_parent),
            )
        } else {
            work_control.checked_add(graph.root_child_order_slot_count(), 1)?
        }
    } else {
        0
    };
    let inserted_numeric_nodes = if child == parent {
        usize::from(!child_exists && child_is_numeric)
    } else {
        usize::from(!child_exists && child_is_numeric)
            + usize::from(!parent_exists && is_javascript_array_index(parent))
    };
    let final_numeric_nodes =
        work_control.checked_add(graph.array_index_node_count(), inserted_numeric_nodes)?;
    let insertion_updates = work_control.checked_mul(
        inserted_numeric_nodes,
        1 + usize::from(graph.options().compound),
    )?;
    let relation_updates = if relation_changes && child_is_numeric {
        2
    } else {
        0
    };
    let ordered_map_work = ordered_key_update_work_upper_bound(
        final_numeric_nodes,
        work_control.checked_add(insertion_updates, relation_updates)?,
        work_control,
    )?;
    let base_work = work_control.checked_add(
        final_node_slots,
        work_control.checked_add(inserted_nodes, 1)?,
    )?;
    let work = work_control.checked_add(
        work_control.checked_add(base_work, previous_order_slots)?,
        ordered_map_work,
    )?;
    work_control.charge_adapter(work)?;
    graph.set_parent_ref(child, parent);
    Ok(())
}

fn copy_cluster(
    cluster_id: &str,
    graph: &mut Graph<NodeLabel, EdgeLabel, GraphLabel>,
    new_graph: &mut Graph<NodeLabel, EdgeLabel, GraphLabel>,
    root_id: &str,
    descendants: &HashMap<String, Vec<String>>,
    work_control: &mut DagreOperationWorkControl,
) -> Result<()> {
    #[derive(Debug)]
    struct CopyFrame {
        node: String,
        owner_cluster: String,
        expanded: bool,
    }

    let mut nodes: Vec<(String, String)> = Vec::new();
    let mut visited: HashSet<String> = HashSet::new();
    let mut stack: Vec<CopyFrame> = Vec::new();
    if cluster_id != root_id {
        stack.push(CopyFrame {
            node: cluster_id.to_string(),
            owner_cluster: cluster_id.to_string(),
            expanded: true,
        });
    }
    let child_count = graph.child_count(cluster_id);
    let child_snapshot = child_id_snapshot_work_upper_bound(graph, cluster_id, work_control)?;
    work_control.charge_adapter(work_control.checked_add(child_snapshot, child_count)?)?;
    stack.extend(
        graph
            .children(cluster_id)
            .into_iter()
            .rev()
            .map(|child| CopyFrame {
                node: child.to_string(),
                owner_cluster: cluster_id.to_string(),
                expanded: false,
            }),
    );

    while let Some(frame) = stack.pop() {
        work_control.charge_adapter(1)?;
        if frame.expanded {
            nodes.push((frame.node, frame.owner_cluster));
            continue;
        }
        if !visited.insert(frame.node.clone()) {
            continue;
        }

        let child_count = graph.child_count(&frame.node);
        if child_count == 0 {
            nodes.push((frame.node, frame.owner_cluster));
            continue;
        }

        let child_snapshot = child_id_snapshot_work_upper_bound(graph, &frame.node, work_control)?;
        let child_clone_work = work_control.checked_add(child_snapshot, child_count)?;
        work_control.charge_adapter(work_control.checked_add(child_clone_work, 1)?)?;
        stack.push(CopyFrame {
            node: frame.node.clone(),
            owner_cluster: frame.node.clone(),
            expanded: true,
        });
        stack.extend(
            graph
                .children(&frame.node)
                .into_iter()
                .rev()
                .map(|child| CopyFrame {
                    node: child.to_string(),
                    owner_cluster: frame.node.clone(),
                    expanded: false,
                }),
        );
    }

    for (node, owner_cluster) in nodes {
        work_control.charge_adapter(1)?;
        if !graph.has_node(&node) {
            continue;
        }

        let data = graph.node(&node).cloned().unwrap_or_default();
        charge_node_insertion_ordering(new_graph, &node, work_control)?;
        new_graph.set_node(node.clone(), data);

        if let Some(parent) = graph.parent(&node)
            && parent != root_id
        {
            set_parent_controlled(new_graph, &node, parent, work_control)?;
        }
        if owner_cluster != root_id && node != owner_cluster {
            set_parent_controlled(new_graph, &node, &owner_cluster, work_control)?;
        }

        let edge_snapshot_work =
            work_control.checked_add(graph.edge_slot_count(), graph.edge_count())?;
        work_control.charge_adapter(edge_snapshot_work)?;
        for edge_key in graph.edge_keys() {
            if !edge_in_cluster(&edge_key, root_id, descendants, work_control)? {
                continue;
            }
            let Some(label) = graph.edge_by_key(&edge_key).cloned() else {
                continue;
            };
            let root_descendants = descendants.get(root_id).map(Vec::as_slice).unwrap_or(&[]);
            let mut v_in = edge_key.v == root_id;
            if !v_in {
                for descendant in root_descendants {
                    work_control.charge_adapter(1)?;
                    if descendant == &edge_key.v {
                        v_in = true;
                        break;
                    }
                }
            }
            let mut w_in = edge_key.w == root_id;
            if !w_in {
                for descendant in root_descendants {
                    work_control.charge_adapter(1)?;
                    if descendant == &edge_key.w {
                        w_in = true;
                        break;
                    }
                }
            }

            if v_in && w_in {
                if !new_graph.has_edge(&edge_key.v, &edge_key.w, edge_key.name.as_deref()) {
                    charge_edge_endpoint_insertions(
                        new_graph,
                        &edge_key.v,
                        &edge_key.w,
                        work_control,
                    )?;
                    new_graph.set_edge_named(edge_key.v, edge_key.w, edge_key.name, Some(label));
                }
            } else {
                let new_v = if v_in {
                    root_id.to_string()
                } else {
                    edge_key.v
                };
                let new_w = if w_in {
                    root_id.to_string()
                } else {
                    edge_key.w
                };
                charge_edge_endpoint_insertions(graph, &new_v, &new_w, work_control)?;
                graph.set_edge_named(new_v, new_w, edge_key.name, Some(label));
            }
        }

        let removal_work = remove_node_work_upper_bound(graph, &node, work_control)?;
        work_control.charge_adapter(removal_work)?;
        let _ = graph.remove_node(&node);
    }

    Ok(())
}

fn extract_clusters_recursively(
    graph: &mut Graph<NodeLabel, EdgeLabel, GraphLabel>,
    subgraphs_by_id: &FlowSubgraphIndex<'_>,
    external_connections_by_id: &std::collections::HashMap<String, bool>,
    extracted: &mut std::collections::HashMap<String, Graph<NodeLabel, EdgeLabel, GraphLabel>>,
    extracted_order: &mut Vec<String>,
    _depth: usize,
    work_control: &mut DagreOperationWorkControl,
) -> Result<()> {
    // Mermaid's recursive JavaScript extractor stops after depth 10 as a stack-safety bailout.
    // Merman intentionally replaces that fixed cutoff with an explicit stack plus the shared work
    // limit, preserving extraction semantics for deeper valid graphs without unbounded recursion.
    struct ExtractFrame {
        id: String,
        graph: Graph<NodeLabel, EdgeLabel, GraphLabel>,
        expanded: bool,
    }

    type ExtractedCluster = (String, Graph<NodeLabel, EdgeLabel, GraphLabel>);

    fn extract_one_level(
        graph: &mut Graph<NodeLabel, EdgeLabel, GraphLabel>,
        subgraphs_by_id: &FlowSubgraphIndex<'_>,
        external_connections_by_id: &std::collections::HashMap<String, bool>,
        work_control: &mut DagreOperationWorkControl,
    ) -> Result<Vec<ExtractedCluster>> {
        let snapshot_work = node_id_snapshot_work_upper_bound(graph, work_control)?;
        work_control.charge_adapter(snapshot_work)?;
        let node_ids = graph.node_ids();
        let mut descendants = HashMap::new();
        work_control.charge_adapter(node_ids.len())?;
        for id in &node_ids {
            if graph.child_count(id) == 0 {
                continue;
            }
            descendants.insert(id.clone(), extract_descendants(id, graph, work_control)?);
        }

        let mut extracted_here: Vec<ExtractedCluster> = Vec::new();

        work_control.charge_adapter(node_ids.len())?;
        let candidates: Vec<String> = node_ids
            .into_iter()
            .filter(|id| graph.has_node(id))
            .filter(|id| graph.child_count(id) != 0)
            // Mermaid's Dagre extractor only promotes clusters without external connections.
            // An explicit `direction` controls the nested graph's rankdir after extraction; it
            // does not change whether the cluster is extracted. Keeping this predicate tied to
            // the computed `externalConnections` flag also keeps cluster extraction, edge
            // ownership, and the downstream render plan on the same semantic boundary.
            //
            // Reference:
            // - `packages/mermaid/src/rendering-util/layout-algorithms/dagre/mermaid-graphlib.js`
            .filter(|id| {
                external_connections_by_id
                    .get(id.as_str())
                    .is_some_and(|external| !external)
            })
            .collect();

        for id in candidates {
            if !graph.has_node(&id) {
                continue;
            }
            if graph.child_count(&id) == 0 {
                continue;
            }

            let mut cluster_graph: Graph<NodeLabel, EdgeLabel, GraphLabel> =
                Graph::new(GraphOptions {
                    multigraph: true,
                    compound: true,
                    directed: true,
                });

            // Mermaid's `extractor(...)` uses:
            // - `clusterData.dir` when explicitly set for the subgraph
            // - otherwise: toggle relative to the current graph's rankdir (TB<->LR)
            let dir = subgraphs_by_id
                .get(id.as_str())
                .and_then(|sg| sg.dir.as_deref())
                .map(str::trim)
                .filter(|s| !s.is_empty())
                .map(normalize_dir)
                .unwrap_or_else(|| toggled_dir(flow_dir_from_rankdir(graph.graph().rankdir)));

            cluster_graph.set_graph(GraphLabel {
                rankdir: dir_to_rankdir(&dir),
                // Mermaid's cluster extractor initializes subgraphs with a fixed dagre config
                // (nodesep/ranksep=50, marginx/marginy=8). Before each recursive render Mermaid then
                // overrides `nodesep` to the parent graph value and `ranksep` to `parent.ranksep + 25`.
                //
                // We model that in headless mode by keeping the extractor defaults here, then applying
                // the per-depth override inside `layout_graph_with_recursive_clusters(...)` right
                // before laying out each extracted graph.
                //
                // Reference:
                // - `packages/mermaid/src/rendering-util/layout-algorithms/dagre/mermaid-graphlib.js`
                // - `packages/mermaid/src/rendering-util/layout-algorithms/dagre/index.js`
                nodesep: 50.0,
                ranksep: 50.0,
                marginx: 8.0,
                marginy: 8.0,
                acyclicer: None,
                ..Default::default()
            });

            copy_cluster(
                &id,
                graph,
                &mut cluster_graph,
                &id,
                &descendants,
                work_control,
            )?;
            extracted_here.push((id, cluster_graph));
        }

        Ok(extracted_here)
    }

    let extracted_here = extract_one_level(
        graph,
        subgraphs_by_id,
        external_connections_by_id,
        work_control,
    )?;
    let mut stack: Vec<ExtractFrame> = extracted_here
        .into_iter()
        .rev()
        .map(|(id, graph)| ExtractFrame {
            id,
            graph,
            expanded: false,
        })
        .collect();

    while let Some(mut frame) = stack.pop() {
        if frame.expanded {
            extracted_order.push(frame.id.clone());
            extracted.insert(frame.id, frame.graph);
            continue;
        }

        let children = extract_one_level(
            &mut frame.graph,
            subgraphs_by_id,
            external_connections_by_id,
            work_control,
        )?;
        frame.expanded = true;
        stack.push(frame);
        stack.extend(children.into_iter().rev().map(|(id, graph)| ExtractFrame {
            id,
            graph,
            expanded: false,
        }));
    }
    Ok(())
}

#[cfg(test)]
pub(crate) fn layout_flowchart_typed(
    model: &FlowchartModel,
    effective_config: &MermaidConfig,
    measurer: &dyn TextMeasurer,
    math_renderer: Option<&(dyn MathRenderer + Send + Sync)>,
) -> Result<FlowchartLayout> {
    layout_flowchart_typed_with_work_meter(
        model,
        effective_config,
        measurer,
        math_renderer,
        Arc::new(OperationWorkMeter::new(
            RenderResourcePolicy::unbounded_for_trusted_input(),
        )),
    )
}

#[cfg(test)]
pub(crate) fn layout_flowchart_typed_with_work_meter(
    model: &FlowchartModel,
    effective_config: &MermaidConfig,
    measurer: &dyn TextMeasurer,
    math_renderer: Option<&(dyn MathRenderer + Send + Sync)>,
    work_meter: Arc<OperationWorkMeter>,
) -> Result<FlowchartLayout> {
    layout_flowchart_typed_with_render_labels_and_work_meter_and_svg_label_sidecar(
        model,
        &FlowchartRenderContext::default(),
        effective_config,
        measurer,
        math_renderer,
        None,
        work_meter,
    )
}

pub(crate) fn layout_flowchart_typed_with_render_labels_and_work_meter_and_svg_label_sidecar(
    model: &FlowchartModel,
    render_label_sources: &FlowchartRenderContext,
    effective_config: &MermaidConfig,
    measurer: &dyn TextMeasurer,
    math_renderer: Option<&(dyn MathRenderer + Send + Sync)>,
    svg_label_sidecar: Option<&FlowchartSvgLabelSidecarBuilder>,
    work_meter: Arc<OperationWorkMeter>,
) -> Result<FlowchartLayout> {
    let mut work_control = DagreOperationWorkControl::new(work_meter);
    layout_flowchart_with_model(
        model,
        render_label_sources,
        effective_config,
        measurer,
        math_renderer,
        svg_label_sidecar,
        &mut work_control,
    )
}

fn layout_flowchart_with_model(
    model: &FlowchartModel,
    render_label_sources: &FlowchartRenderContext,
    effective_config: &MermaidConfig,
    measurer: &dyn TextMeasurer,
    math_renderer: Option<&(dyn MathRenderer + Send + Sync)>,
    svg_label_sidecar: Option<&FlowchartSvgLabelSidecarBuilder>,
    work_control: &mut DagreOperationWorkControl,
) -> Result<FlowchartLayout> {
    let render_model = FlowchartRenderModelRef::new(model, render_label_sources);
    let model = &render_model;
    // Shape validation is an adapter-owned full node scan. Charge it before touching the model so
    // a low operation budget still bounds invalid inputs and preserves the old preflight's
    // resource-error precedence when the scan itself cannot be admitted.
    work_control.charge_adapter(model.nodes.len())?;
    super::validate_flowchart_model_shapes(model)?;
    let effective_config_value = effective_config.as_value();

    // Project the lossless semantic edges through the collapsed-subgraph view before expanding
    // self-loops. Boundary edges use the visible collapsed node as their endpoint, while edges
    // wholly hidden by a collapsed subgraph are omitted.
    work_control.charge_adapter(model.edges.len())?;
    let mut projected_edges: Vec<FlowEdge> = Vec::with_capacity(model.edges.len());
    let mut projected_edge_owner_indices: Vec<usize> = Vec::with_capacity(model.edges.len());
    for (edge_index, edge) in model.edges.iter().enumerate() {
        if let Some(projected) = super::project_flowchart_edge(edge, render_label_sources) {
            projected_edges.push(projected);
            projected_edge_owner_indices.push(edge_index);
        }
    }
    work_control.charge_adapter(projected_edges.len())?;

    // Mermaid's dagre adapter expands self-loop edges into a chain of two special label nodes plus
    // three edges. This avoids `v == w` edges in Dagre and is required for SVG parity (Mermaid
    // uses `*-cyclic-special-*` ids when rendering self-loops).
    let self_loop_count = projected_edges.iter().filter(|e| e.from == e.to).count();
    let derived_render_edges = work_control.checked_add(
        projected_edges.len(),
        work_control.checked_mul(self_loop_count, 2)?,
    )?;
    work_control.charge_adapter(derived_render_edges)?;
    let mut render_edges: Vec<std::borrow::Cow<'_, FlowEdge>> =
        Vec::with_capacity(derived_render_edges);
    let mut render_edge_self_loop_meta: Vec<Option<FlowchartSelfLoopSegmentMeta>> =
        Vec::with_capacity(derived_render_edges);
    let mut render_edge_owner_indices = Vec::with_capacity(derived_render_edges);
    let mut self_loop_label_node_ids: Vec<String> = Vec::new();
    let mut self_loop_label_node_id_set: std::collections::HashSet<String> =
        std::collections::HashSet::new();
    for (projected_index, e) in projected_edges.iter().enumerate() {
        let edge_index = projected_edge_owner_indices[projected_index];
        if e.from != e.to {
            render_edges.push(std::borrow::Cow::Borrowed(e));
            render_edge_self_loop_meta.push(None);
            render_edge_owner_indices.push(edge_index);
            continue;
        }

        let mut helper_edges = super::flowchart_self_loop_helper_edges(e);
        helper_edges.edge_mid.label = model.edge_label_for_render(e).map(str::to_owned);
        if self_loop_label_node_id_set.insert(helper_edges.special_id_1.clone()) {
            self_loop_label_node_ids.push(helper_edges.special_id_1.clone());
        }
        if self_loop_label_node_id_set.insert(helper_edges.special_id_2.clone()) {
            self_loop_label_node_ids.push(helper_edges.special_id_2.clone());
        }

        // Mermaid clears the label text on the end segments, but keeps the label (if any) on the
        // mid edge (`edgeMid` is a structuredClone of the original edge without label changes).
        render_edges.push(std::borrow::Cow::Owned(helper_edges.edge1));
        render_edge_owner_indices.push(edge_index);
        render_edge_self_loop_meta.push(Some(FlowchartSelfLoopSegmentMeta {
            logical_edge_id: e.id.clone(),
            node_id: e.from.clone(),
            order: 0,
        }));
        render_edges.push(std::borrow::Cow::Owned(helper_edges.edge_mid));
        render_edge_owner_indices.push(edge_index);
        render_edge_self_loop_meta.push(Some(FlowchartSelfLoopSegmentMeta {
            logical_edge_id: e.id.clone(),
            node_id: e.from.clone(),
            order: 1,
        }));
        render_edges.push(std::borrow::Cow::Owned(helper_edges.edge2));
        render_edge_owner_indices.push(edge_index);
        render_edge_self_loop_meta.push(Some(FlowchartSelfLoopSegmentMeta {
            logical_edge_id: e.id.clone(),
            node_id: e.from.clone(),
            order: 2,
        }));
    }
    let FlowchartLayoutSettings {
        nodesep,
        ranksep,
        node_padding,
        state_padding,
        wrapping_width,
        edge_label_wrapping_width,
        cluster_title_wrapping_width,
        edge_html_labels,
        node_wrap_mode,
        edge_wrap_mode,
        cluster_wrap_mode,
        cluster_padding,
        title_margin_top,
        title_margin_bottom,
        title_total_margin,
        y_shift,
        inherit_dir,
        text_style,
        html_label_text_style,
    } = FlowchartConfigView::new(effective_config_value).layout_settings();
    let look_is_neo = crate::config::config_diagram_look(effective_config_value).is_neo();
    let node_label_base_style = if node_wrap_mode == WrapMode::HtmlLike {
        &html_label_text_style
    } else {
        &text_style
    };
    let cluster_label_base_style = if cluster_wrap_mode == WrapMode::HtmlLike {
        &html_label_text_style
    } else {
        &text_style
    };
    let edge_label_base_style = if edge_wrap_mode == WrapMode::HtmlLike {
        &html_label_text_style
    } else {
        &text_style
    };

    let diagram_direction = normalize_dir(model.direction.as_deref().unwrap_or("TB"));
    let has_subgraphs = !model.subgraphs.is_empty();
    work_control.charge_adapter(model.subgraphs.len())?;
    // Parsed Mermaid 12.1 models already contain one canonical group per id. Public typed
    // callers can still construct duplicate groups; retain first-owner presentation and merged
    // membership for that supported input without rebuilding parser semantics here.
    let mut subgraphs_by_id: FlowSubgraphIndex<'_> = HashMap::with_capacity(model.subgraphs.len());
    let mut subgraph_index_by_id: HashMap<&str, usize> =
        HashMap::with_capacity(model.subgraphs.len());
    let mut canonical_subgraphs_in_order: Vec<(usize, &FlowSubgraph)> = Vec::new();
    let mut subgraph_members_by_id: HashMap<&str, Vec<&str>> = HashMap::new();
    let mut nonempty_subgraph_ids: HashSet<&str> = HashSet::new();
    for (index, subgraph) in model.subgraphs.iter().enumerate() {
        let id = subgraph.id.as_str();
        if !subgraphs_by_id.contains_key(id) {
            subgraphs_by_id.insert(id, subgraph);
            subgraph_index_by_id.insert(id, index);
            canonical_subgraphs_in_order.push((index, subgraph));
        }
        let members = subgraph_members_by_id.entry(id).or_default();
        members.extend(subgraph.nodes.iter().map(String::as_str));
        if !subgraph.nodes.is_empty() {
            nonempty_subgraph_ids.insert(id);
        }
    }
    work_control.charge_adapter(model.subgraphs.len())?;
    let subgraph_ids: std::collections::HashSet<&str> =
        model.subgraphs.iter().map(|sg| sg.id.as_str()).collect();
    let collapsed_subgraph_ids: HashSet<&str> = model
        .subgraphs
        .iter()
        .filter(|sg| model.is_subgraph_collapsed(sg.id.as_str()))
        .filter(|sg| model.collapsed_replacement(sg.id.as_str()).is_none())
        .map(|sg| sg.id.as_str())
        .collect();
    // Only expanded, non-empty subgraphs participate in Dagre's compound-cluster pipeline.
    // Collapsed subgraphs are ordinary leaf nodes; nested collapsed/expanded descendants replaced
    // by an outer collapsed ancestor are omitted from the graph entirely.
    let cluster_subgraph_ids: HashSet<&str> = model
        .subgraphs
        .iter()
        .filter(|sg| !collapsed_subgraph_ids.contains(sg.id.as_str()))
        .filter(|sg| model.collapsed_replacement(sg.id.as_str()).is_none())
        .filter(|sg| !sg.nodes.is_empty())
        .map(|sg| sg.id.as_str())
        .collect();
    let mut g: Graph<NodeLabel, EdgeLabel, GraphLabel> = Graph::new(GraphOptions {
        multigraph: true,
        // Mermaid's Dagre adapter always enables `compound: true`, even if there are no explicit
        // subgraphs. This also allows `nestingGraph.run` to connect components during ranking.
        compound: true,
        directed: true,
    });
    g.set_graph(GraphLabel {
        rankdir: rank_dir_from_flow(&diagram_direction),
        nodesep,
        ranksep,
        marginx: 8.0,
        marginy: 8.0,
        acyclicer: None,
        ..Default::default()
    });

    let mut empty_subgraph_ids: Vec<String> = Vec::new();
    let mut cluster_node_labels: std::collections::HashMap<String, NodeLabel> =
        std::collections::HashMap::new();
    work_control.charge_adapter(canonical_subgraphs_in_order.len())?;
    for (_, sg) in &canonical_subgraphs_in_order {
        if model.collapsed_replacement(sg.id.as_str()).is_some() {
            continue;
        }
        if collapsed_subgraph_ids.contains(sg.id.as_str()) {
            continue;
        }
        if !nonempty_subgraph_ids.contains(sg.id.as_str()) {
            // Mermaid renders empty subgraphs as regular nodes. Keep the semantic `subgraph`
            // definition around for styling/title, but size + lay it out as a leaf node.
            empty_subgraph_ids.push(sg.id.clone());
            continue;
        }
        // Mermaid does not pre-size compound (subgraph) nodes based on title metrics for Dagre
        // layout. Their dimensions are computed from children (border nodes) and then adjusted at
        // render time for title width and configured margins.
        cluster_node_labels.insert(sg.id.clone(), NodeLabel::default());
    }

    let mut leaf_node_labels: std::collections::HashMap<String, NodeLabel> =
        std::collections::HashMap::new();
    let mut leaf_label_metrics_by_id: HashMap<String, (f64, f64)> = HashMap::new();
    let collapsed_leaf_count = collapsed_subgraph_ids.len();
    let leaf_label_capacity = work_control.checked_add(
        model.nodes.len(),
        work_control.checked_add(empty_subgraph_ids.len(), collapsed_leaf_count)?,
    )?;
    work_control.charge_adapter(leaf_label_capacity)?;
    leaf_label_metrics_by_id.reserve(leaf_label_capacity);
    work_control.charge_adapter(model.nodes.len())?;
    for (node_index, n) in model.nodes.iter().enumerate() {
        // Mermaid treats the subgraph id as the "group node" id (a cluster can be referenced in
        // edges). Avoid introducing a separate leaf node that would collide with the cluster node
        // of the same id.
        if subgraph_ids.contains(n.id.as_str())
            || model.collapsed_replacement(n.id.as_str()).is_some()
        {
            continue;
        }
        let raw_label = model.node_label_for_render(n).unwrap_or(&n.id);
        let label_type = n.label_type.as_deref().unwrap_or("text");
        let node_text_style = flowchart_effective_text_style_for_node_classes(
            node_label_base_style,
            &model.class_defs,
            &n.classes,
            &n.styles,
        );
        let svg_width_mode = flowchart_node_svg_width_mode(
            raw_label,
            label_type,
            node_wrap_mode,
            n.layout_shape.as_deref().unwrap_or("squareRect"),
        );
        let mut metrics = measure_flowchart_svg_label_for_layout(
            svg_label_sidecar,
            Some(FlowchartSvgLabelOwner::Node(node_index)),
            Some(n.id.as_str()),
            FlowchartLabelMetricsRequest {
                measurer,
                raw_label,
                label_type,
                style: node_text_style.as_ref(),
                max_width_px: Some(wrapping_width),
                wrap_mode: node_wrap_mode,
                config: effective_config,
                math_renderer,
            },
            svg_width_mode,
        );
        if node_wrap_mode == WrapMode::HtmlLike && edge_html_labels {
            flowchart_apply_html_node_class_box_metrics(
                &mut metrics,
                raw_label,
                label_type,
                node_text_style.as_ref(),
                &model.class_defs,
                &n.classes,
            );
        }
        metrics = metrics.with_label_min_width(
            raw_label,
            super::flowchart_node_label_min_width(
                raw_label,
                n.layout_shape.as_deref(),
                effective_config,
            ),
            None,
        );
        leaf_label_metrics_by_id.insert(n.id.clone(), (metrics.width, metrics.height));
        let (width, height) = node_layout_dimensions(NodeLayoutDimensionsRequest {
            layout_shape: n.layout_shape.as_deref(),
            layout_direction: &diagram_direction,
            metrics,
            has_label: !raw_label.is_empty(),
            wrapping_width,
            node_constraint: n.constraint.as_deref(),
            padding: node_padding,
            look_is_neo,
            state_padding,
            node_icon: n.icon.as_deref(),
            node_img: n.img.as_deref(),
            node_pos: n.pos.as_deref(),
            node_asset_width: n.asset_width,
            node_asset_height: n.asset_height,
        });
        leaf_node_labels.insert(
            n.id.clone(),
            NodeLabel {
                width,
                height,
                ..Default::default()
            },
        );
    }
    work_control.charge_adapter(canonical_subgraphs_in_order.len())?;
    for &(subgraph_index, sg) in &canonical_subgraphs_in_order {
        if nonempty_subgraph_ids.contains(sg.id.as_str())
            || collapsed_subgraph_ids.contains(sg.id.as_str())
            || model.collapsed_replacement(sg.id.as_str()).is_some()
        {
            continue;
        }
        let label_type = sg.label_type.as_deref().unwrap_or("text");
        let (classes, styles) = model.effective_subgraph_css(subgraph_index, sg);
        let sg_text_style = flowchart_effective_text_style_for_classes(
            cluster_label_base_style,
            &model.class_defs,
            classes,
            styles,
        );
        let title = model.subgraph_title_for_render(subgraph_index, sg);
        // Mermaid renders an empty subgraph through the ordinary node `labelHelper`: wrapping
        // probes use `flowchart.wrappingWidth` and `getComputedTextLength()`, while the final label
        // dimensions come from `getBBox()`. Selecting `ComputedLength` here would add a post-wrap
        // per-line measurement pass that is absent upstream.
        let mut metrics = measure_flowchart_svg_label_for_layout(
            svg_label_sidecar,
            Some(FlowchartSvgLabelOwner::EmptySubgraphNode(subgraph_index)),
            Some(sg.id.as_str()),
            FlowchartLabelMetricsRequest {
                measurer,
                raw_label: title,
                label_type,
                style: sg_text_style.as_ref(),
                max_width_px: Some(wrapping_width),
                wrap_mode: node_wrap_mode,
                config: effective_config,
                math_renderer,
            },
            FlowchartSvgWidthMode::Bbox,
        );
        if node_wrap_mode == WrapMode::HtmlLike && edge_html_labels {
            flowchart_apply_html_node_class_box_metrics(
                &mut metrics,
                title,
                label_type,
                sg_text_style.as_ref(),
                &model.class_defs,
                classes,
            );
        }
        leaf_label_metrics_by_id.insert(sg.id.clone(), (metrics.width, metrics.height));
        let (width, height) = node_layout_dimensions(NodeLayoutDimensionsRequest {
            layout_shape: Some("squareRect"),
            layout_direction: &diagram_direction,
            metrics,
            has_label: !title.is_empty(),
            wrapping_width,
            node_constraint: None,
            padding: cluster_padding,
            look_is_neo: false,
            state_padding,
            node_icon: None,
            node_img: None,
            node_pos: None,
            node_asset_width: None,
            node_asset_height: None,
        });
        leaf_node_labels.insert(
            sg.id.clone(),
            NodeLabel {
                width,
                height,
                ..Default::default()
            },
        );
    }

    // A collapsed subgraph is laid out as a regular leaf node. Its title uses the same label
    // preparation path as a subgraph title, but its outer dimensions reserve the separator and
    // indicator rows rendered by `collapsedGroup`.
    work_control.charge_adapter(collapsed_leaf_count)?;
    for &(subgraph_index, sg) in &canonical_subgraphs_in_order {
        if !collapsed_subgraph_ids.contains(sg.id.as_str()) {
            continue;
        }
        let label_type = sg.label_type.as_deref().unwrap_or("text");
        let (classes, styles) = model.effective_subgraph_css(subgraph_index, sg);
        let base_style = if node_wrap_mode == WrapMode::HtmlLike {
            &html_label_text_style
        } else {
            &text_style
        };
        let text_style_for_label = flowchart_effective_text_style_for_classes(
            base_style,
            &model.class_defs,
            classes,
            styles,
        );
        let title = model.subgraph_title_for_render(subgraph_index, sg);
        let mut metrics = measure_flowchart_svg_label_for_layout(
            svg_label_sidecar,
            Some(FlowchartSvgLabelOwner::SubgraphTitle(subgraph_index)),
            Some(sg.id.as_str()),
            FlowchartLabelMetricsRequest {
                measurer,
                raw_label: title,
                label_type,
                style: text_style_for_label.as_ref(),
                max_width_px: Some(wrapping_width),
                wrap_mode: node_wrap_mode,
                config: effective_config,
                math_renderer,
            },
            FlowchartSvgWidthMode::Bbox,
        );
        if node_wrap_mode == WrapMode::HtmlLike && edge_html_labels {
            flowchart_apply_html_node_class_box_metrics(
                &mut metrics,
                title,
                label_type,
                text_style_for_label.as_ref(),
                &model.class_defs,
                classes,
            );
        }
        leaf_label_metrics_by_id.insert(sg.id.clone(), (metrics.width, metrics.height));
        leaf_node_labels.insert(
            sg.id.clone(),
            NodeLabel {
                width: (metrics.width + 16.0).max(80.0),
                height: metrics.height + 44.0,
                ..Default::default()
            },
        );
    }

    // Mermaid constructs the Dagre graph by:
    // 1) inserting subgraph (cluster) nodes first (in reverse `subgraphs[]` order), then
    // 2) inserting vertex nodes (in FlowDB `Map` insertion order),
    // and setting `parentId` as each node is inserted.
    //
    // Matching property creation matters because Graphlib exposes JavaScript `Object.keys`
    // order: array-index IDs enumerate numerically before ordinary IDs in creation order. That
    // order affects compound children, anchor selection, and deterministic layout tie-breaking.
    let mut inserted: std::collections::HashSet<String> = std::collections::HashSet::new();
    let mut parent_assigned: std::collections::HashSet<String> = std::collections::HashSet::new();
    let insert_one = |id: &str,
                      g: &mut Graph<NodeLabel, EdgeLabel, GraphLabel>,
                      inserted: &mut std::collections::HashSet<String>,
                      work_control: &mut DagreOperationWorkControl|
     -> Result<()> {
        if inserted.contains(id) {
            return Ok(());
        }
        if let Some(lbl) = cluster_node_labels.get(id).cloned() {
            charge_node_insertion_ordering(g, id, work_control)?;
            g.set_node(id.to_string(), lbl);
            inserted.insert(id.to_string());
            return Ok(());
        }
        if let Some(lbl) = leaf_node_labels.get(id).cloned() {
            charge_node_insertion_ordering(g, id, work_control)?;
            g.set_node(id.to_string(), lbl);
            inserted.insert(id.to_string());
        }
        Ok(())
    };

    let mut existing_parent_count = 0usize;
    if has_subgraphs {
        // Match Mermaid's `FlowDB.getData()` parent assignment: build `parentId` by iterating
        // subgraphs in reverse order and recording each membership.
        let mut parent_by_id: std::collections::HashMap<String, String> =
            std::collections::HashMap::new();
        let membership_count = model.subgraphs.iter().try_fold(0usize, |total, subgraph| {
            work_control.checked_add(total, subgraph.nodes.len())
        })?;
        let hierarchy_validation_work =
            work_control.checked_add(membership_count, model.subgraphs.len())?;
        work_control.charge_adapter(hierarchy_validation_work)?;
        for sg in model.subgraphs.iter().rev() {
            for child in &sg.nodes {
                parent_by_id.insert(child.clone(), sg.id.clone());
            }
        }

        let insert_with_parent = |id: &str,
                                  g: &mut Graph<NodeLabel, EdgeLabel, GraphLabel>,
                                  inserted: &mut std::collections::HashSet<String>,
                                  parent_assigned: &mut std::collections::HashSet<String>,
                                  parent_assignments: &mut Vec<(usize, usize)>,
                                  numeric_parent_assignments: &mut usize,
                                  work_control: &mut DagreOperationWorkControl|
         -> Result<()> {
            insert_one(id, g, inserted, work_control)?;
            if !parent_assigned.insert(id.to_string()) {
                return Ok(());
            }
            if let Some(parent) = parent_by_id.get(id) {
                charge_node_insertion_ordering(g, parent, work_control)?;
                g.ensure_node_ref(parent);
                if let (Some(child_ix), Some(parent_ix)) = (g.node_ix(id), g.node_ix(parent)) {
                    parent_assignments.push((child_ix, parent_ix));
                    *numeric_parent_assignments += usize::from(is_javascript_array_index(id));
                }
            }
            Ok(())
        };

        let insertion_visits = work_control.checked_add(
            work_control.checked_add(model.subgraphs.len(), model.nodes.len())?,
            model.vertex_calls.len(),
        )?;
        let insertion_work = work_control.checked_add(insertion_visits, parent_by_id.len())?;
        work_control.charge_adapter(insertion_work)?;
        let mut parent_assignments = Vec::with_capacity(parent_by_id.len());
        let mut numeric_parent_assignments = 0usize;
        for sg in model.subgraphs.iter().rev() {
            if model.collapsed_replacement(sg.id.as_str()).is_some() {
                continue;
            }
            insert_with_parent(
                sg.id.as_str(),
                &mut g,
                &mut inserted,
                &mut parent_assigned,
                &mut parent_assignments,
                &mut numeric_parent_assignments,
                work_control,
            )?;
        }
        for n in &model.nodes {
            if model.collapsed_replacement(n.id.as_str()).is_some() {
                continue;
            }
            insert_with_parent(
                n.id.as_str(),
                &mut g,
                &mut inserted,
                &mut parent_assigned,
                &mut parent_assignments,
                &mut numeric_parent_assignments,
                work_control,
            )?;
        }
        for id in &model.vertex_calls {
            if model.collapsed_replacement(id.as_str()).is_some() {
                continue;
            }
            insert_with_parent(
                id.as_str(),
                &mut g,
                &mut inserted,
                &mut parent_assigned,
                &mut parent_assignments,
                &mut numeric_parent_assignments,
                work_control,
            )?;
        }
        apply_unparented_parent_assignments(
            &mut g,
            &parent_assignments,
            UnparentedParentBatchShape {
                existing_parent_count: 0,
                numeric_assignment_count: numeric_parent_assignments,
            },
            work_control,
        )?;
        existing_parent_count = parent_assignments.len();
    } else {
        // No subgraphs: insertion order still matters for deterministic Dagre tie-breaking.
        let insertion_work =
            work_control.checked_add(model.nodes.len(), model.vertex_calls.len())?;
        work_control.charge_adapter(insertion_work)?;
        for n in &model.nodes {
            insert_one(n.id.as_str(), &mut g, &mut inserted, work_control)?;
        }
        for id in &model.vertex_calls {
            insert_one(id.as_str(), &mut g, &mut inserted, work_control)?;
        }
    }

    // Materialize self-loop helper label nodes and place them in the same parent cluster as the
    // base node (if any), matching Mermaid `@11.12.2` dagre layout adapter behavior.
    work_control.charge_adapter(self_loop_label_node_ids.len())?;
    let mut self_loop_parent_assignments = Vec::new();
    let mut numeric_self_loop_parent_assignments = 0usize;
    for id in &self_loop_label_node_ids {
        if !g.has_node(id) {
            charge_node_insertion_ordering(&g, id, work_control)?;
            g.set_node(
                id.clone(),
                NodeLabel {
                    // Mermaid initializes these labelRect nodes at 10x10, but then immediately
                    // runs `insertNode(...)` + `updateNodeBounds(...)` before Dagre layout. For an
                    // empty `labelRect`, the measured bbox collapses to ~0.1x0.1 and that is what
                    // Dagre actually sees for spacing. Match that here for layout parity.
                    width: 0.1_f32 as f64,
                    height: 0.1_f32 as f64,
                    ..Default::default()
                },
            );
        }
        let Some((base, _)) = id.split_once("---") else {
            continue;
        };
        if let Some(p) = g.parent(base) {
            let child_ix = g
                .node_ix(id)
                .expect("a self-loop helper node is present after insertion");
            let parent_ix = g
                .node_ix(p)
                .expect("the base node parent is present in the layout graph");
            self_loop_parent_assignments.push((child_ix, parent_ix));
            numeric_self_loop_parent_assignments += usize::from(is_javascript_array_index(id));
        }
    }
    apply_unparented_parent_assignments(
        &mut g,
        &self_loop_parent_assignments,
        UnparentedParentBatchShape {
            existing_parent_count,
            numeric_assignment_count: numeric_self_loop_parent_assignments,
        },
        work_control,
    )?;

    let effective_dir_by_id = if has_subgraphs {
        compute_effective_dir_by_id(
            &model.subgraphs,
            &subgraphs_by_id,
            &g,
            &diagram_direction,
            inherit_dir,
            work_control,
        )?
    } else {
        HashMap::new()
    };

    // Map SVG edge ids to the multigraph key used by the Dagre layout graph. Most edges use their
    // `id` as the key, but Mermaid uses distinct keys for the self-loop special edges and we also
    // want deterministic ordering under our BTree-backed graph storage.
    let mut edge_key_by_id: HashMap<String, String> = HashMap::new();
    let mut edge_id_by_key: HashMap<String, String> = HashMap::new();

    let default_edge_styles = model
        .edge_defaults
        .as_ref()
        .map_or(&[][..], |defaults| defaults.style.as_slice());
    work_control.charge_adapter(render_edges.len())?;
    for ((e, self_loop_meta), edge_owner_index) in render_edges
        .iter()
        .zip(&render_edge_self_loop_meta)
        .zip(&render_edge_owner_indices)
    {
        // Mermaid 11.16 stores helper identity as edge metadata. The graph key is intentionally
        // node-scoped, so a later parallel self-loop overwrites the earlier triple in Graphlib.
        let edge_key = flowchart_layout_edge_key(e, self_loop_meta.as_ref());
        edge_key_by_id.insert(e.id.clone(), edge_key.clone());
        edge_id_by_key.insert(edge_key.clone(), e.id.clone());

        let from = e.from.clone();
        let to = e.to.clone();

        if edge_label_is_non_empty(model, e) {
            let label_text = model.edge_label_for_render(e).unwrap_or_default();
            let label_type = e.label_type.as_deref().unwrap_or("text");
            let edge_text_style = flowchart_effective_edge_label_text_style(
                edge_label_base_style,
                &model.class_defs,
                &e.classes,
                default_edge_styles,
                &e.style,
            );
            let metrics = if label_type == "markdown" && edge_wrap_mode != WrapMode::HtmlLike {
                crate::text::measure_wrapped_markdown_with_inline_styles(
                    measurer,
                    label_text,
                    edge_text_style.as_ref(),
                    Some(edge_label_wrapping_width),
                    edge_wrap_mode,
                )
            } else if edge_wrap_mode == WrapMode::SvgLike {
                let render_id = self_loop_meta
                    .as_ref()
                    .map_or(e.id.as_str(), |meta| meta.logical_edge_id.as_str());
                measure_flowchart_svg_label_for_layout_with_metrics_style(
                    svg_label_sidecar,
                    Some(FlowchartSvgLabelOwner::Edge(*edge_owner_index)),
                    Some(render_id),
                    FlowchartLabelMetricsRequest {
                        measurer,
                        raw_label: label_text,
                        label_type,
                        // Mermaid wraps the temporary SVG text before applying `labelStyle`.
                        style: edge_label_base_style,
                        max_width_px: Some(edge_label_wrapping_width),
                        wrap_mode: edge_wrap_mode,
                        config: effective_config,
                        math_renderer,
                    },
                    edge_text_style.as_ref(),
                    FlowchartSvgWidthMode::Bbox,
                )
            } else {
                measure_flowchart_svg_label_for_layout(
                    svg_label_sidecar,
                    Some(FlowchartSvgLabelOwner::Edge(*edge_owner_index)),
                    Some(e.id.as_str()),
                    FlowchartLabelMetricsRequest {
                        measurer,
                        raw_label: label_text,
                        label_type,
                        style: edge_text_style.as_ref(),
                        max_width_px: Some(edge_label_wrapping_width),
                        wrap_mode: edge_wrap_mode,
                        config: effective_config,
                        math_renderer,
                    },
                    FlowchartSvgWidthMode::Bbox,
                )
            };
            let (label_width, label_height) = if edge_html_labels {
                (metrics.width.max(1.0), metrics.height.max(1.0))
            } else {
                // Mermaid's SVG edge-labels include a padded background rect (+2px left/right and
                // +2px top/bottom).
                (
                    (metrics.width + 4.0).max(1.0),
                    (metrics.height + 4.0).max(1.0),
                )
            };

            let minlen = e.length.max(1);
            let mut el = EdgeLabel {
                width: label_width,
                height: label_height,
                labelpos: LabelPos::C,
                // Dagre layout defaults `labeloffset` to 10 when unspecified.
                labeloffset: 10.0,
                minlen,
                weight: 1.0,
                ..Default::default()
            };
            if let Some(meta) = self_loop_meta {
                annotate_flowchart_self_loop_segment(&mut el, meta);
            }

            g.set_edge_named(from, to, Some(edge_key), Some(el));
        } else {
            let mut el = EdgeLabel {
                width: 0.0,
                height: 0.0,
                labelpos: LabelPos::C,
                // Dagre layout defaults `labeloffset` to 10 when unspecified.
                labeloffset: 10.0,
                minlen: e.length.max(1),
                weight: 1.0,
                ..Default::default()
            };
            if let Some(meta) = self_loop_meta {
                annotate_flowchart_self_loop_segment(&mut el, meta);
            }
            g.set_edge_named(from, to, Some(edge_key), Some(el));
        }
    }

    let external_connections_by_id = if has_subgraphs {
        adjust_flowchart_clusters_and_edges(&mut g, work_control)?
    } else {
        std::collections::HashMap::new()
    };

    let mut edge_endpoints_by_id: HashMap<String, (String, String)> = HashMap::new();
    let edge_snapshot_work = work_control.checked_add(g.edge_slot_count(), g.edge_count())?;
    work_control.charge_adapter(edge_snapshot_work)?;
    for ek in g.edge_keys() {
        let Some(edge_key) = ek.name.as_deref() else {
            continue;
        };
        let edge_id = edge_id_by_key
            .get(edge_key)
            .cloned()
            .unwrap_or_else(|| edge_key.to_string());
        edge_endpoints_by_id.insert(edge_id, (ek.v.clone(), ek.w.clone()));
    }

    let mut extracted_graphs: std::collections::HashMap<
        String,
        Graph<NodeLabel, EdgeLabel, GraphLabel>,
    > = std::collections::HashMap::new();
    let mut extracted_order = Vec::new();
    if has_subgraphs {
        extract_clusters_recursively(
            &mut g,
            &subgraphs_by_id,
            &external_connections_by_id,
            &mut extracted_graphs,
            &mut extracted_order,
            0,
            work_control,
        )?;
        // Cluster extraction can rebind a cross-boundary edge to the cluster node. Refresh root
        // endpoints after extraction so output lookup uses the surviving nodes.
        let edge_snapshot_work = work_control.checked_add(g.edge_slot_count(), g.edge_count())?;
        work_control.charge_adapter(edge_snapshot_work)?;
        for ek in g.edge_keys() {
            let Some(edge_key) = ek.name.as_deref() else {
                continue;
            };
            let edge_id = edge_id_by_key
                .get(edge_key)
                .cloned()
                .unwrap_or_else(|| edge_key.to_string());
            edge_endpoints_by_id.insert(edge_id, (ek.v, ek.w));
        }
    }

    // Mermaid's flowchart-v2 renderer inserts node DOM elements in pinned Graphlib
    // `graph.nodes()` order before Dagre layout, including recursively extracted cluster graphs.
    // This per-root map is the render plan consumed by the SVG renderer, hierarchy ownership, and
    // viewBox calculation; capture it from the same extracted graphs used for layout.
    let mut dom_node_order_by_root: std::collections::HashMap<String, Vec<String>> =
        std::collections::HashMap::new();
    let mut dom_order_work = node_id_snapshot_work_upper_bound(&g, work_control)?;
    for id in &extracted_order {
        let graph = extracted_graphs
            .get(id)
            .expect("the extraction order references an extracted graph");
        let graph_work = node_id_snapshot_work_upper_bound(graph, work_control)?;
        dom_order_work = work_control.checked_add(dom_order_work, graph_work)?;
    }
    work_control.charge_adapter(dom_order_work)?;
    dom_node_order_by_root.insert(String::new(), g.node_ids());
    for id in &extracted_order {
        let cg = extracted_graphs
            .get(id)
            .expect("the extraction order references an extracted graph");
        dom_node_order_by_root.insert(id.clone(), cg.node_ids());
    }
    type Rect = merman_core::geom::Box2;

    struct ClusterTitleMetricsContext<'a> {
        model: &'a FlowchartRenderModelRef<'a>,
        subgraphs_by_id: &'a FlowSubgraphIndex<'a>,
        subgraph_index_by_id: &'a HashMap<&'a str, usize>,
        class_defs: &'a merman_core::native_collections::OrderedMap<String, Vec<String>>,
        measurer: &'a dyn TextMeasurer,
        text_style: &'a TextStyle,
        html_label_text_style: &'a TextStyle,
        title_wrapping_width: f64,
        wrap_mode: WrapMode,
        config: &'a MermaidConfig,
        math_renderer: Option<&'a (dyn MathRenderer + Send + Sync)>,
        svg_label_sidecar: Option<&'a FlowchartSvgLabelSidecarBuilder>,
    }

    fn cluster_title_metrics_for_layout(
        id: &str,
        ctx: &ClusterTitleMetricsContext<'_>,
    ) -> Option<(f64, f64)> {
        let sg = ctx.subgraphs_by_id.get(id)?;
        let subgraph_index = ctx.subgraph_index_by_id.get(id).copied()?;
        let title = ctx.model.subgraph_title_for_render(subgraph_index, sg);
        let label_type = sg.label_type.as_deref().unwrap_or("text");
        let title_width_limit = (label_type == "markdown").then_some(ctx.title_wrapping_width);
        let base_style = if ctx.wrap_mode == WrapMode::HtmlLike {
            ctx.html_label_text_style
        } else {
            ctx.text_style
        };
        let (classes, styles) = ctx.model.effective_subgraph_css(subgraph_index, sg);
        let text_style =
            flowchart_effective_text_style_for_classes(base_style, ctx.class_defs, classes, styles);
        let owner = ctx
            .subgraph_index_by_id
            .get(id)
            .copied()
            .map(FlowchartSvgLabelOwner::SubgraphTitle);
        let metrics = measure_flowchart_svg_label_for_layout(
            ctx.svg_label_sidecar,
            owner,
            Some(id),
            FlowchartLabelMetricsRequest {
                measurer: ctx.measurer,
                raw_label: title,
                label_type,
                style: text_style.as_ref(),
                max_width_px: title_width_limit,
                wrap_mode: ctx.wrap_mode,
                config: ctx.config,
                math_renderer: ctx.math_renderer,
            },
            FlowchartSvgWidthMode::Bbox,
        );
        Some((metrics.width.max(1.0), metrics.height.max(1.0)))
    }

    fn extracted_graph_bbox_rect(
        g: &Graph<NodeLabel, EdgeLabel, GraphLabel>,
        title_total_margin: f64,
        extracted: &std::collections::HashMap<String, Graph<NodeLabel, EdgeLabel, GraphLabel>>,
        subgraph_id_set: &std::collections::HashSet<&str>,
        title_metrics_ctx: &ClusterTitleMetricsContext<'_>,
        cluster_padding: f64,
        work_control: &mut DagreOperationWorkControl,
    ) -> Result<Option<Rect>> {
        fn graph_content_rect(
            g: &Graph<NodeLabel, EdgeLabel, GraphLabel>,
            extracted: &std::collections::HashMap<String, Graph<NodeLabel, EdgeLabel, GraphLabel>>,
            subgraph_id_set: &std::collections::HashSet<&str>,
            title_total_margin: f64,
            title_metrics_ctx: &ClusterTitleMetricsContext<'_>,
            cluster_padding: f64,
            work_control: &mut DagreOperationWorkControl,
        ) -> Result<Option<Rect>> {
            let node_snapshot_work = node_id_snapshot_work_upper_bound(g, work_control)?;
            work_control.charge_adapter(node_snapshot_work)?;
            let mut out: Option<Rect> = None;
            for id in g.node_ids() {
                let Some(n) = g.node(&id) else { continue };
                let (Some(x), Some(y)) = (n.x, n.y) else {
                    continue;
                };
                let mut width = n.width;
                let mut height = n.height;
                let is_cluster_node = extracted.contains_key(&id) && g.child_count(&id) == 0;
                let is_non_recursive_cluster =
                    subgraph_id_set.contains(id.as_str()) && g.child_count(&id) != 0;

                // Mermaid increases cluster node height by `subGraphTitleTotalMargin` *after* Dagre
                // layout (just before rendering), and `updateNodeBounds(...)` measures the DOM
                // bbox after that expansion. Mirror that here for non-recursive clusters.
                //
                // For leaf clusterNodes (recursively rendered clusters), the node's width/height
                // comes directly from `updateNodeBounds(...)`, so do not add margins again.
                if !is_cluster_node && is_non_recursive_cluster {
                    if title_total_margin > 0.0 {
                        height = (height + title_total_margin).max(1.0);
                    }
                    if let Some((title_w, title_h)) =
                        cluster_title_metrics_for_layout(&id, title_metrics_ctx)
                    {
                        width = width.max(title_w + cluster_padding);
                        height = height.max(title_h + title_total_margin);
                    }
                }

                let r = Rect::from_center(x, y, width, height);
                if let Some(ref mut cur) = out {
                    cur.union(r);
                } else {
                    out = Some(r);
                }
            }
            let edge_snapshot_work =
                work_control.checked_add(g.edge_slot_count(), g.edge_count())?;
            work_control.charge_adapter(edge_snapshot_work)?;
            for ek in g.edge_keys() {
                let Some(e) = g.edge_by_key(&ek) else {
                    continue;
                };
                let (Some(x), Some(y)) = (e.x, e.y) else {
                    continue;
                };
                if e.width <= 0.0 && e.height <= 0.0 {
                    continue;
                }
                let r = Rect::from_center(x, y, e.width, e.height);
                if let Some(ref mut cur) = out {
                    cur.union(r);
                } else {
                    out = Some(r);
                }
            }
            Ok(out)
        }

        graph_content_rect(
            g,
            extracted,
            subgraph_id_set,
            title_total_margin,
            title_metrics_ctx,
            cluster_padding,
            work_control,
        )
    }

    fn apply_mermaid_subgraph_title_shifts(
        graph: &mut Graph<NodeLabel, EdgeLabel, GraphLabel>,
        extracted: &std::collections::HashMap<String, Graph<NodeLabel, EdgeLabel, GraphLabel>>,
        subgraph_id_set: &std::collections::HashSet<&str>,
        y_shift: f64,
    ) {
        if y_shift.abs() < 1e-9 {
            return;
        }

        // Mermaid v11.12.2 adjusts Y positions after Dagre layout:
        // - regular nodes: +subGraphTitleTotalMargin/2
        // - clusterNode nodes (recursively rendered clusters): +subGraphTitleTotalMargin
        // - pure cluster nodes (non-recursive clusters): no y-shift (but height grows elsewhere)
        for id in graph.node_ids() {
            // A cluster is only a Mermaid "clusterNode" placeholder if it is a leaf in the
            // current graph. Extracted graphs contain an injected parent cluster node with the
            // same id (and children), which must follow the pure-cluster path.
            let is_cluster_node = extracted.contains_key(&id) && graph.child_count(&id) == 0;
            let delta_y = if is_cluster_node {
                y_shift * 2.0
            } else if subgraph_id_set.contains(id.as_str()) && graph.child_count(&id) != 0 {
                0.0
            } else {
                y_shift
            };
            if delta_y.abs() > 1e-9 {
                let Some(y) = graph.node(&id).and_then(|n| n.y) else {
                    continue;
                };
                if let Some(n) = graph.node_mut(&id) {
                    n.y = Some(y + delta_y);
                }
            }
        }

        // Mermaid shifts all edge points and the edge label position by +subGraphTitleTotalMargin/2.
        for ek in graph.edge_keys() {
            let Some(e) = graph.edge_mut_by_key(&ek) else {
                continue;
            };
            if let Some(y) = e.y {
                e.y = Some(y + y_shift);
            }
            for p in &mut e.points {
                p.y += y_shift;
            }
        }
    }

    struct RecursiveLayoutContext<'a> {
        extracted:
            &'a mut std::collections::HashMap<String, Graph<NodeLabel, EdgeLabel, GraphLabel>>,
        subgraph_id_set: &'a std::collections::HashSet<&'a str>,
        y_shift: f64,
        cluster_node_labels: &'a std::collections::HashMap<String, NodeLabel>,
        title_total_margin: f64,
        title_metrics_ctx: &'a ClusterTitleMetricsContext<'a>,
        cluster_padding: f64,
        work_control: &'a mut DagreOperationWorkControl,
    }

    fn layout_graph_with_recursive_clusters(
        graph: &mut Graph<NodeLabel, EdgeLabel, GraphLabel>,
        graph_cluster_id: Option<&str>,
        _depth: usize,
        ctx: &mut RecursiveLayoutContext<'_>,
    ) -> Result<()> {
        struct LayoutFrame {
            cluster_id: Option<String>,
            child_ids: Option<Vec<String>>,
        }

        fn recursive_child_ids(
            graph: &Graph<NodeLabel, EdgeLabel, GraphLabel>,
            extracted: &std::collections::HashMap<String, Graph<NodeLabel, EdgeLabel, GraphLabel>>,
        ) -> Vec<String> {
            graph
                .node_ids()
                .into_iter()
                // Only recurse into extracted graphs for leaf cluster nodes ("clusterNode" in
                // Mermaid). Child graphs also get their parent cluster node injected, with
                // children, and must not recurse back into themselves.
                .filter(|id| graph.child_count(id) == 0 && extracted.contains_key(id))
                .collect()
        }

        fn inject_parent_cluster(
            graph: &mut Graph<NodeLabel, EdgeLabel, GraphLabel>,
            cluster_id: &str,
            ctx: &mut RecursiveLayoutContext<'_>,
        ) -> Result<()> {
            let inserts_cluster = usize::from(!graph.has_node(cluster_id));
            let final_node_order_slots = ctx
                .work_control
                .checked_add(graph.node_order_slot_count(), inserts_cluster)?;
            let final_node_count = ctx
                .work_control
                .checked_add(graph.node_count(), inserts_cluster)?;
            let snapshot_work = ctx
                .work_control
                .checked_add(final_node_order_slots, final_node_count)?;
            ctx.work_control.charge_adapter(snapshot_work)?;

            if inserts_cluster != 0 {
                let lbl = ctx
                    .cluster_node_labels
                    .get(cluster_id)
                    .cloned()
                    .unwrap_or_default();
                charge_node_insertion_ordering(graph, cluster_id, ctx.work_control)?;
                graph.set_node(cluster_id.to_string(), lbl);
            }
            let node_ids = graph.node_ids();
            let cluster_ix = graph
                .node_ix(cluster_id)
                .expect("the injected cluster node is present");
            let mut parent_assignments = Vec::new();
            let mut existing_parent_count = usize::from(graph.parent(cluster_id).is_some());
            let mut numeric_parent_assignments = 0usize;
            for node_id in node_ids {
                if node_id == cluster_id {
                    continue;
                }
                if graph.parent(&node_id).is_some() {
                    existing_parent_count += 1;
                } else {
                    let child_ix = graph
                        .node_ix(&node_id)
                        .expect("a node snapshot contains only live nodes");
                    parent_assignments.push((child_ix, cluster_ix));
                    numeric_parent_assignments += usize::from(is_javascript_array_index(&node_id));
                }
            }
            apply_unparented_parent_assignments(
                graph,
                &parent_assignments,
                UnparentedParentBatchShape {
                    existing_parent_count,
                    numeric_assignment_count: numeric_parent_assignments,
                },
                ctx.work_control,
            )?;
            Ok(())
        }

        fn update_child_cluster_bounds(
            graph: &mut Graph<NodeLabel, EdgeLabel, GraphLabel>,
            ids: &[String],
            ctx: &mut RecursiveLayoutContext<'_>,
        ) -> Result<()> {
            for id in ids {
                let Some(child) = ctx.extracted.get(id) else {
                    continue;
                };
                // In Mermaid, `updateNodeBounds(...)` measures the recursively rendered `<g
                // class="root">` group. In that render path, the child graph contains a node
                // matching the cluster id (inserted via `graph.setNode(parentCluster.id, ...)`),
                // whose computed compound bounds correspond to the cluster box measured in the DOM.
                if let Some(r) = extracted_graph_bbox_rect(
                    child,
                    ctx.title_total_margin,
                    ctx.extracted,
                    ctx.subgraph_id_set,
                    ctx.title_metrics_ctx,
                    ctx.cluster_padding,
                    ctx.work_control,
                )? {
                    if let Some(n) = graph.node_mut(id) {
                        n.width = r.width().max(1.0);
                        n.height = r.height().max(1.0);
                    }
                } else if let Some(n_child) = child.node(id)
                    && let Some(n) = graph.node_mut(id)
                {
                    n.width = n_child.width.max(1.0);
                    n.height = n_child.height.max(1.0);
                }
            }
            Ok(())
        }

        fn layout_one_graph(
            graph: &mut Graph<NodeLabel, EdgeLabel, GraphLabel>,
            ctx: &mut RecursiveLayoutContext<'_>,
        ) -> Result<()> {
            if let Err(error) = dugong::layout_controlled(graph, &mut *ctx.work_control) {
                return Err(ctx.work_control.map_dugong_error(error));
            }
            // The pinned Mermaid title-shift pass is a strict no-op at the default zero margin.
            // Avoid materializing point counts and graph snapshots that the mutation would never
            // consume.
            if ctx.y_shift.abs() < 1e-9 {
                return Ok(());
            }
            ctx.work_control.charge_adapter(graph.edge_slot_count())?;
            let point_work = graph.edges().try_fold(0usize, |total, edge_key| {
                let points = graph
                    .edge_by_key(edge_key)
                    .map_or(0, |edge| edge.points.len());
                ctx.work_control.checked_add(total, points)
            })?;
            let node_snapshot_work = node_id_snapshot_work_upper_bound(graph, ctx.work_control)?;
            let edge_snapshot_work = ctx
                .work_control
                .checked_add(graph.edge_slot_count(), graph.edge_count())?;
            let graph_work = ctx
                .work_control
                .checked_add(node_snapshot_work, edge_snapshot_work)?;
            let shift_work = ctx.work_control.checked_add(graph_work, point_work)?;
            ctx.work_control.charge_adapter(shift_work)?;
            apply_mermaid_subgraph_title_shifts(
                graph,
                ctx.extracted,
                ctx.subgraph_id_set,
                ctx.y_shift,
            );
            Ok(())
        }

        if ctx.extracted.is_empty() {
            return layout_one_graph(graph, ctx);
        }

        let mut stack = vec![LayoutFrame {
            cluster_id: graph_cluster_id.map(str::to_string),
            child_ids: None,
        }];

        while let Some(frame) = stack.pop() {
            let Some(child_ids) = frame.child_ids else {
                let cluster_id = frame.cluster_id;
                let graph_snapshot_work = match &cluster_id {
                    Some(id) => ctx.extracted.get(id).map_or(Ok(0), |graph| {
                        node_id_snapshot_work_upper_bound(graph, ctx.work_control)
                    })?,
                    None => node_id_snapshot_work_upper_bound(graph, ctx.work_control)?,
                };
                ctx.work_control.charge_adapter(graph_snapshot_work)?;
                let Some((child_ids, parent_nodesep, parent_ranksep)) = (match &cluster_id {
                    Some(id) => ctx.extracted.get(id).map(|g| {
                        (
                            recursive_child_ids(g, ctx.extracted),
                            g.graph().nodesep,
                            g.graph().ranksep,
                        )
                    }),
                    None => Some((
                        recursive_child_ids(graph, ctx.extracted),
                        graph.graph().nodesep,
                        graph.graph().ranksep,
                    )),
                }) else {
                    continue;
                };

                let mut child_frames = Vec::with_capacity(child_ids.len());
                for child_id in child_ids.iter().rev() {
                    // Match Mermaid `recursiveRender` behavior: before laying out a recursively
                    // rendered cluster graph, override `nodesep` to the parent graph spacing and
                    // `ranksep` to `parent.ranksep + 25`. This compounds for nested recursive
                    // clusters.
                    if let Some(child) = ctx.extracted.get_mut(child_id) {
                        child.graph_mut().nodesep = parent_nodesep;
                        child.graph_mut().ranksep = parent_ranksep + 25.0;
                    }
                    child_frames.push(LayoutFrame {
                        cluster_id: Some(child_id.clone()),
                        child_ids: None,
                    });
                }
                stack.push(LayoutFrame {
                    cluster_id,
                    child_ids: Some(child_ids),
                });
                stack.extend(child_frames);
                continue;
            };

            if let Some(cluster_id) = frame.cluster_id {
                let Some(mut current) = ctx.extracted.remove(&cluster_id) else {
                    continue;
                };
                update_child_cluster_bounds(&mut current, &child_ids, ctx)?;
                inject_parent_cluster(&mut current, &cluster_id, ctx)?;
                layout_one_graph(&mut current, ctx)?;
                ctx.extracted.insert(cluster_id, current);
            } else {
                update_child_cluster_bounds(graph, &child_ids, ctx)?;
                layout_one_graph(graph, ctx)?;
            }
        }
        Ok(())
    }

    {
        let title_metrics_ctx = ClusterTitleMetricsContext {
            model,
            subgraphs_by_id: &subgraphs_by_id,
            subgraph_index_by_id: &subgraph_index_by_id,
            class_defs: &model.class_defs,
            measurer,
            text_style: &text_style,
            html_label_text_style: &html_label_text_style,
            title_wrapping_width: cluster_title_wrapping_width,
            wrap_mode: cluster_wrap_mode,
            config: effective_config,
            math_renderer,
            svg_label_sidecar,
        };
        let mut recursive_layout_ctx = RecursiveLayoutContext {
            extracted: &mut extracted_graphs,
            subgraph_id_set: &cluster_subgraph_ids,
            y_shift,
            cluster_node_labels: &cluster_node_labels,
            title_total_margin,
            title_metrics_ctx: &title_metrics_ctx,
            cluster_padding,
            work_control,
        };
        layout_graph_with_recursive_clusters(&mut g, None, 0, &mut recursive_layout_ctx)?;
    }

    let mut leaf_rects: std::collections::HashMap<String, Rect> = std::collections::HashMap::new();
    let mut base_pos: std::collections::HashMap<String, (f64, f64)> =
        std::collections::HashMap::new();
    let mut edge_override_points: std::collections::HashMap<String, Vec<LayoutPoint>> =
        std::collections::HashMap::new();
    let mut edge_override_label: std::collections::HashMap<String, Option<LayoutLabel>> =
        std::collections::HashMap::new();
    let mut edge_override_from_cluster: std::collections::HashMap<String, Option<String>> =
        std::collections::HashMap::new();
    let mut edge_override_to_cluster: std::collections::HashMap<String, Option<String>> =
        std::collections::HashMap::new();
    let mut edge_override_endpoints: std::collections::HashMap<String, (String, String)> =
        std::collections::HashMap::new();
    let mut edge_override_self_loop_meta: std::collections::HashMap<
        String,
        FlowchartSelfLoopSegmentMeta,
    > = std::collections::HashMap::new();
    let leaf_node_id_work = work_control.checked_add(
        model.nodes.len(),
        work_control.checked_add(self_loop_label_node_ids.len(), empty_subgraph_ids.len())?,
    )?;
    work_control.charge_adapter(leaf_node_id_work)?;
    let mut leaf_node_ids: std::collections::HashSet<String> = model
        .nodes
        .iter()
        .filter(|n| !subgraph_ids.contains(n.id.as_str()))
        .map(|n| n.id.clone())
        .collect();
    for id in &self_loop_label_node_ids {
        leaf_node_ids.insert(id.clone());
    }
    for id in &empty_subgraph_ids {
        leaf_node_ids.insert(id.clone());
    }
    for id in &collapsed_subgraph_ids {
        leaf_node_ids.insert((*id).to_string());
    }

    struct PlaceGraphInputs<'a> {
        edge_id_by_key: &'a std::collections::HashMap<String, String>,
        extracted_graphs:
            &'a std::collections::HashMap<String, Graph<NodeLabel, EdgeLabel, GraphLabel>>,
        subgraph_ids: &'a std::collections::HashSet<&'a str>,
        leaf_node_ids: &'a std::collections::HashSet<String>,
    }

    struct PlaceGraphOutputs<'a> {
        base_pos: &'a mut std::collections::HashMap<String, (f64, f64)>,
        leaf_rects: &'a mut std::collections::HashMap<String, Rect>,
        cluster_rects_from_graph: &'a mut std::collections::HashMap<String, Rect>,
        extracted_cluster_rects: &'a mut std::collections::HashMap<String, Rect>,
        extracted_cluster_base_widths: &'a mut std::collections::HashMap<String, f64>,
        edge_override_points: &'a mut std::collections::HashMap<String, Vec<LayoutPoint>>,
        edge_override_label: &'a mut std::collections::HashMap<String, Option<LayoutLabel>>,
        edge_override_from_cluster: &'a mut std::collections::HashMap<String, Option<String>>,
        edge_override_to_cluster: &'a mut std::collections::HashMap<String, Option<String>>,
        edge_override_endpoints: &'a mut std::collections::HashMap<String, (String, String)>,
        edge_override_self_loop_meta:
            &'a mut std::collections::HashMap<String, FlowchartSelfLoopSegmentMeta>,
    }

    fn place_graph(
        graph: &Graph<NodeLabel, EdgeLabel, GraphLabel>,
        offset: (f64, f64),
        is_root: bool,
        inputs: &PlaceGraphInputs<'_>,
        out: &mut PlaceGraphOutputs<'_>,
        work_control: &mut DagreOperationWorkControl,
    ) -> Result<()> {
        struct PlaceFrame<'a> {
            graph: &'a Graph<NodeLabel, EdgeLabel, GraphLabel>,
            offset: (f64, f64),
            is_root: bool,
        }

        fn subtree_rect_iterative(
            graph: &Graph<NodeLabel, EdgeLabel, GraphLabel>,
            id: &str,
            work_control: &mut DagreOperationWorkControl,
        ) -> Result<Option<Rect>> {
            let mut visiting: std::collections::HashSet<String> = std::collections::HashSet::new();
            if !visiting.insert(id.to_string()) {
                return Ok(None);
            }
            let mut out: Option<Rect> = None;
            let child_count = child_id_snapshot_work_upper_bound(graph, id, work_control)?;
            work_control.charge_adapter(child_count)?;
            let mut stack: Vec<String> =
                graph.children(id).into_iter().map(str::to_string).collect();
            while let Some(node) = stack.pop() {
                work_control.charge_adapter(1)?;
                if let Some(n) = graph.node(&node)
                    && let (Some(x), Some(y)) = (n.x, n.y)
                {
                    let r = Rect::from_center(x, y, n.width, n.height);
                    if let Some(ref mut cur) = out {
                        cur.union(r);
                    } else {
                        out = Some(r);
                    }
                }
                let child_count = child_id_snapshot_work_upper_bound(graph, &node, work_control)?;
                work_control.charge_adapter(
                    work_control.checked_add(child_count, graph.child_count(&node))?,
                )?;
                for child in graph.children(&node) {
                    if visiting.insert(child.to_string()) {
                        stack.push(child.to_string());
                    }
                }
            }
            Ok(out)
        }

        let mut stack = vec![PlaceFrame {
            graph,
            offset,
            is_root,
        }];
        while let Some(frame) = stack.pop() {
            let node_snapshot_work = node_id_snapshot_work_upper_bound(frame.graph, work_control)?;
            work_control.charge_adapter(node_snapshot_work)?;
            for id in frame.graph.node_ids() {
                let Some(n) = frame.graph.node(&id) else {
                    continue;
                };
                let x = n.x.unwrap_or(0.0) + frame.offset.0;
                let y = n.y.unwrap_or(0.0) + frame.offset.1;
                if inputs.leaf_node_ids.contains(&id) {
                    out.base_pos.insert(id.clone(), (x, y));
                    out.leaf_rects
                        .insert(id.clone(), Rect::from_center(x, y, n.width, n.height));
                    continue;
                }
            }

            // Capture the layout-computed compound bounds for non-extracted clusters.
            //
            // Upstream Dagre computes compound-node geometry from border nodes and then removes the
            // border dummy nodes (`removeBorderNodes`). Our dugong parity pipeline mirrors that, so
            // prefer the compound node's own x/y/width/height when available.
            work_control.charge_adapter(node_snapshot_work)?;
            for id in frame.graph.node_ids() {
                if !inputs.subgraph_ids.contains(id.as_str()) {
                    continue;
                }
                if inputs.extracted_graphs.contains_key(&id) {
                    continue;
                }
                if out.cluster_rects_from_graph.contains_key(&id) {
                    continue;
                }
                if let Some(n) = frame.graph.node(&id)
                    && let (Some(x), Some(y)) = (n.x, n.y)
                    && n.width > 0.0
                    && n.height > 0.0
                {
                    let mut r = Rect::from_center(x, y, n.width, n.height);
                    r.translate(frame.offset.0, frame.offset.1);
                    out.cluster_rects_from_graph.insert(id, r);
                    continue;
                }

                let Some(mut r) = subtree_rect_iterative(frame.graph, &id, work_control)? else {
                    continue;
                };
                r.translate(frame.offset.0, frame.offset.1);
                out.cluster_rects_from_graph.insert(id, r);
            }

            work_control.charge_adapter(frame.graph.edge_slot_count())?;
            let edge_point_work = frame.graph.edges().try_fold(0usize, |total, edge_key| {
                let points = frame
                    .graph
                    .edge_by_key(edge_key)
                    .map_or(0, |edge| edge.points.len());
                work_control.checked_add(total, points)
            })?;
            let edge_snapshot_work = work_control
                .checked_add(frame.graph.edge_slot_count(), frame.graph.edge_count())?;
            let edge_work = work_control.checked_add(edge_snapshot_work, edge_point_work)?;
            work_control.charge_adapter(edge_work)?;
            for ek in frame.graph.edge_keys() {
                let Some(edge_key) = ek.name.as_deref() else {
                    continue;
                };
                let edge_id = inputs
                    .edge_id_by_key
                    .get(edge_key)
                    .map(String::as_str)
                    .unwrap_or(edge_key);
                let Some(lbl) = frame.graph.edge_by_key(&ek) else {
                    continue;
                };

                if let (Some(x), Some(y)) = (lbl.x, lbl.y)
                    && (lbl.width > 0.0 || lbl.height > 0.0)
                {
                    let lx = x + frame.offset.0;
                    let ly = y + frame.offset.1;
                    let leaf_id = format!("edge-label::{edge_id}");
                    out.base_pos.insert(leaf_id.clone(), (lx, ly));
                    out.leaf_rects
                        .insert(leaf_id, Rect::from_center(lx, ly, lbl.width, lbl.height));
                }

                if !frame.is_root {
                    let points = lbl
                        .points
                        .iter()
                        .map(|p| LayoutPoint {
                            x: p.x + frame.offset.0,
                            y: p.y + frame.offset.1,
                        })
                        .collect::<Vec<_>>();
                    let label_pos = match (lbl.x, lbl.y) {
                        (Some(x), Some(y)) if lbl.width > 0.0 || lbl.height > 0.0 => {
                            Some(LayoutLabel {
                                x: x + frame.offset.0,
                                y: y + frame.offset.1,
                                width: lbl.width,
                                height: lbl.height,
                            })
                        }
                        _ => None,
                    };
                    out.edge_override_points.insert(edge_id.to_string(), points);
                    out.edge_override_label
                        .insert(edge_id.to_string(), label_pos);
                    let from_cluster = lbl
                        .extras
                        .get("fromCluster")
                        .and_then(|v| v.as_str().map(|s| s.to_string()));
                    let to_cluster = lbl
                        .extras
                        .get("toCluster")
                        .and_then(|v| v.as_str().map(|s| s.to_string()));
                    out.edge_override_from_cluster
                        .insert(edge_id.to_string(), from_cluster);
                    out.edge_override_to_cluster
                        .insert(edge_id.to_string(), to_cluster);
                    out.edge_override_endpoints
                        .insert(edge_id.to_string(), (ek.v.clone(), ek.w.clone()));
                    if let Some(meta) = flowchart_self_loop_segment_meta(lbl) {
                        out.edge_override_self_loop_meta
                            .insert(edge_id.to_string(), meta);
                    }
                }
            }

            let mut child_frames = Vec::new();
            work_control.charge_adapter(node_snapshot_work)?;
            for id in frame.graph.node_ids() {
                // Only recurse into extracted graphs for leaf cluster nodes ("clusterNode" in Mermaid).
                // The recursively rendered graph itself also contains a node with the same id (the
                // parent cluster node injected before layout), which has children and must not recurse.
                if frame.graph.child_count(&id) != 0 {
                    continue;
                }
                let Some(child) = inputs.extracted_graphs.get(&id) else {
                    continue;
                };
                let Some(n) = frame.graph.node(&id) else {
                    continue;
                };
                let (Some(px), Some(py)) = (n.x, n.y) else {
                    continue;
                };
                let parent_x = px + frame.offset.0;
                let parent_y = py + frame.offset.1;
                let Some(cnode) = child.node(&id) else {
                    continue;
                };
                let (Some(cx), Some(cy)) = (cnode.x, cnode.y) else {
                    continue;
                };
                let child_offset = (parent_x - cx, parent_y - cy);
                // The extracted cluster's footprint in the parent graph is the clusterNode itself.
                // Our recursive layout step updates the parent graph's node `width/height` to match
                // Mermaid's `updateNodeBounds(...)` behavior (including any title margin). Avoid
                // adding `title_total_margin` again here.
                let r = Rect::from_center(parent_x, parent_y, n.width, n.height);
                out.extracted_cluster_rects.insert(id.clone(), r);
                out.extracted_cluster_base_widths
                    .insert(id.clone(), cnode.width.max(1.0));
                child_frames.push(PlaceFrame {
                    graph: child,
                    offset: child_offset,
                    is_root: false,
                });
            }
            for child in child_frames.into_iter().rev() {
                stack.push(child);
            }
        }
        Ok(())
    }

    let mut cluster_rects_from_graph: std::collections::HashMap<String, Rect> =
        std::collections::HashMap::new();
    let mut extracted_cluster_rects: std::collections::HashMap<String, Rect> =
        std::collections::HashMap::new();
    let mut extracted_cluster_base_widths: std::collections::HashMap<String, f64> =
        std::collections::HashMap::new();
    {
        let place_graph_inputs = PlaceGraphInputs {
            edge_id_by_key: &edge_id_by_key,
            extracted_graphs: &extracted_graphs,
            subgraph_ids: &cluster_subgraph_ids,
            leaf_node_ids: &leaf_node_ids,
        };
        let mut place_graph_outputs = PlaceGraphOutputs {
            base_pos: &mut base_pos,
            leaf_rects: &mut leaf_rects,
            cluster_rects_from_graph: &mut cluster_rects_from_graph,
            extracted_cluster_rects: &mut extracted_cluster_rects,
            extracted_cluster_base_widths: &mut extracted_cluster_base_widths,
            edge_override_points: &mut edge_override_points,
            edge_override_label: &mut edge_override_label,
            edge_override_from_cluster: &mut edge_override_from_cluster,
            edge_override_to_cluster: &mut edge_override_to_cluster,
            edge_override_endpoints: &mut edge_override_endpoints,
            edge_override_self_loop_meta: &mut edge_override_self_loop_meta,
        };
        place_graph(
            &g,
            (0.0, 0.0),
            true,
            &place_graph_inputs,
            &mut place_graph_outputs,
            work_control,
        )?;
    }

    let mut extra_children: std::collections::HashMap<String, Vec<String>> =
        std::collections::HashMap::new();
    work_control.charge_adapter(render_edges.len())?;
    let labeled_edges: std::collections::HashSet<&str> = render_edges
        .iter()
        .filter(|e| edge_label_is_non_empty(model, e))
        .map(|e| e.id.as_str())
        .collect();

    fn collect_extra_children(
        graph: &Graph<NodeLabel, EdgeLabel, GraphLabel>,
        edge_id_by_key: &std::collections::HashMap<String, String>,
        labeled_edges: &std::collections::HashSet<&str>,
        implicit_root: Option<&str>,
        out: &mut std::collections::HashMap<String, Vec<String>>,
        work_control: &mut DagreOperationWorkControl,
    ) -> Result<()> {
        let edge_snapshot_work =
            work_control.checked_add(graph.edge_slot_count(), graph.edge_count())?;
        work_control.charge_adapter(edge_snapshot_work)?;
        for ek in graph.edge_keys() {
            let Some(edge_key) = ek.name.as_deref() else {
                continue;
            };
            let edge_id = edge_id_by_key
                .get(edge_key)
                .map(String::as_str)
                .unwrap_or(edge_key);
            if !labeled_edges.contains(edge_id) {
                continue;
            }
            // Mermaid's recursive cluster extractor removes the root cluster node from the
            // extracted graph. In that case, the "lowest common parent" of edges whose endpoints
            // belong to the extracted cluster becomes `None`, even though the label should still
            // participate in the extracted cluster's bounding box. Use `implicit_root` to map
            // those labels back to the extracted cluster id.
            let parent = lowest_common_parent(graph, &ek.v, &ek.w, work_control)?
                .or_else(|| implicit_root.map(|s| s.to_string()));
            let Some(parent) = parent else {
                continue;
            };
            out.entry(parent)
                .or_default()
                .push(format!("edge-label::{edge_id}"));
        }
        Ok(())
    }

    collect_extra_children(
        &g,
        &edge_id_by_key,
        &labeled_edges,
        None,
        &mut extra_children,
        work_control,
    )?;
    for cluster_id in &extracted_order {
        let cg = extracted_graphs
            .get(cluster_id)
            .expect("the extraction order references an extracted graph");
        collect_extra_children(
            cg,
            &edge_id_by_key,
            &labeled_edges,
            Some(cluster_id.as_str()),
            &mut extra_children,
            work_control,
        )?;
    }

    // Ensure Mermaid-style self-loop helper nodes participate in cluster bounding/packing.
    // These nodes are not part of the semantic `subgraph ... end` membership list, but are
    // parented into the same clusters as their base node.
    work_control.charge_adapter(self_loop_label_node_ids.len())?;
    for id in &self_loop_label_node_ids {
        if let Some(p) = g.parent(id) {
            extra_children
                .entry(p.to_string())
                .or_default()
                .push(id.clone());
        }
    }

    let mut out_nodes: Vec<LayoutNode> = Vec::new();
    work_control.charge_adapter(model.nodes.len())?;
    for n in &model.nodes {
        if subgraph_ids.contains(n.id.as_str())
            || model.collapsed_replacement(n.id.as_str()).is_some()
        {
            continue;
        }
        let (x, y) = base_pos
            .get(&n.id)
            .copied()
            .ok_or_else(|| Error::InvalidModel {
                message: format!("missing positioned node {}", n.id),
            })?;
        let (width, height) = leaf_rects
            .get(&n.id)
            .map(|r| (r.width(), r.height()))
            .ok_or_else(|| Error::InvalidModel {
                message: format!("missing sized node {}", n.id),
            })?;
        out_nodes.push(LayoutNode {
            id: n.id.clone(),
            x,
            y,
            width,
            height,
            is_cluster: false,
            label_width: leaf_label_metrics_by_id.get(&n.id).map(|v| v.0),
            label_height: leaf_label_metrics_by_id.get(&n.id).map(|v| v.1),
        });
    }
    work_control.charge_adapter(empty_subgraph_ids.len())?;
    for id in &empty_subgraph_ids {
        let (x, y) = base_pos
            .get(id)
            .copied()
            .ok_or_else(|| Error::InvalidModel {
                message: format!("missing positioned node {id}"),
            })?;
        let (width, height) = leaf_rects
            .get(id)
            .map(|r| (r.width(), r.height()))
            .ok_or_else(|| Error::InvalidModel {
                message: format!("missing sized node {id}"),
            })?;
        out_nodes.push(LayoutNode {
            id: id.clone(),
            x,
            y,
            width,
            height,
            is_cluster: false,
            label_width: leaf_label_metrics_by_id.get(id).map(|v| v.0),
            label_height: leaf_label_metrics_by_id.get(id).map(|v| v.1),
        });
    }
    work_control.charge_adapter(collapsed_subgraph_ids.len())?;
    for id in &collapsed_subgraph_ids {
        let (x, y) = base_pos
            .get(*id)
            .copied()
            .ok_or_else(|| Error::InvalidModel {
                message: format!("missing positioned collapsed subgraph {id}"),
            })?;
        let (width, height) = leaf_rects
            .get(*id)
            .map(|r| (r.width(), r.height()))
            .ok_or_else(|| Error::InvalidModel {
                message: format!("missing sized collapsed subgraph {id}"),
            })?;
        out_nodes.push(LayoutNode {
            id: (*id).to_string(),
            x,
            y,
            width,
            height,
            is_cluster: false,
            label_width: leaf_label_metrics_by_id.get(*id).map(|v| v.0),
            label_height: leaf_label_metrics_by_id.get(*id).map(|v| v.1),
        });
    }
    work_control.charge_adapter(self_loop_label_node_ids.len())?;
    for id in &self_loop_label_node_ids {
        let (x, y) = base_pos
            .get(id)
            .copied()
            .ok_or_else(|| Error::InvalidModel {
                message: format!("missing positioned node {id}"),
            })?;
        let (width, height) = leaf_rects
            .get(id)
            .map(|r| (r.width(), r.height()))
            .ok_or_else(|| Error::InvalidModel {
                message: format!("missing sized node {id}"),
            })?;
        out_nodes.push(LayoutNode {
            id: id.clone(),
            x,
            y,
            width,
            height,
            is_cluster: false,
            label_width: None,
            label_height: None,
        });
    }

    let mut clusters: Vec<LayoutCluster> = Vec::new();

    let mut cluster_rects: std::collections::HashMap<String, Rect> =
        std::collections::HashMap::new();
    let mut cluster_base_widths: std::collections::HashMap<String, f64> =
        std::collections::HashMap::new();
    let mut visiting: std::collections::HashSet<String> = std::collections::HashSet::new();
    let cluster_membership_work = model.subgraphs.iter().try_fold(0usize, |total, subgraph| {
        work_control.checked_add(total, subgraph.nodes.len())
    })?;
    let extra_cluster_work = extra_children
        .values()
        .try_fold(0usize, |total, children| {
            work_control.checked_add(total, children.len())
        })?;
    // `compute_cluster_rect` visits every computed subgraph membership once while expanding the
    // explicit stack and once while folding child rectangles. The outer materialization loop and
    // both stack frames add three subgraph-local visits. Extracted/empty subgraphs may do less,
    // but this checked owner-local upper bound admits every scan and stack allocation before the
    // first cluster rectangle is planned.
    let cluster_work = work_control.checked_add(
        work_control.checked_add(
            work_control.checked_mul(model.subgraphs.len(), 3)?,
            work_control.checked_mul(cluster_membership_work, 2)?,
        )?,
        extra_cluster_work,
    )?;
    work_control.charge_adapter(cluster_work)?;

    struct ClusterRectContext<'a> {
        model: &'a FlowchartRenderModelRef<'a>,
        subgraphs_by_id: &'a FlowSubgraphIndex<'a>,
        subgraph_index_by_id: &'a HashMap<&'a str, usize>,
        subgraph_members_by_id: &'a HashMap<&'a str, Vec<&'a str>>,
        class_defs: &'a merman_core::native_collections::OrderedMap<String, Vec<String>>,
        leaf_rects: &'a std::collections::HashMap<String, Rect>,
        extra_children: &'a std::collections::HashMap<String, Vec<String>>,
        measurer: &'a dyn TextMeasurer,
        text_style: &'a TextStyle,
        html_label_text_style: &'a TextStyle,
        title_wrapping_width: f64,
        wrap_mode: WrapMode,
        config: &'a MermaidConfig,
        math_renderer: Option<&'a (dyn MathRenderer + Send + Sync)>,
        cluster_padding: f64,
        title_total_margin: f64,
        svg_label_sidecar: Option<&'a FlowchartSvgLabelSidecarBuilder>,
    }

    struct ClusterRectState<'a> {
        cluster_rects: &'a mut std::collections::HashMap<String, Rect>,
        cluster_base_widths: &'a mut std::collections::HashMap<String, f64>,
        visiting: &'a mut std::collections::HashSet<String>,
    }

    fn compute_cluster_rect(
        id: &str,
        ctx: &ClusterRectContext<'_>,
        state: &mut ClusterRectState<'_>,
    ) -> Result<(Rect, f64)> {
        if let Some(r) = state.cluster_rects.get(id).copied() {
            let base_width = state
                .cluster_base_widths
                .get(id)
                .copied()
                .unwrap_or_else(|| r.width());
            return Ok((r, base_width));
        }

        struct ClusterRectFrame {
            id: String,
            expanded: bool,
        }

        let mut stack = vec![ClusterRectFrame {
            id: id.to_string(),
            expanded: false,
        }];
        while let Some(frame) = stack.pop() {
            if state.cluster_rects.contains_key(&frame.id) {
                continue;
            }

            if !frame.expanded {
                if !state.visiting.insert(frame.id.clone()) {
                    return Err(Error::InvalidModel {
                        message: format!("cycle in subgraph membership involving {}", frame.id),
                    });
                }

                if !ctx.subgraphs_by_id.contains_key(frame.id.as_str()) {
                    return Err(Error::InvalidModel {
                        message: format!("missing subgraph definition for {}", frame.id),
                    });
                }

                stack.push(ClusterRectFrame {
                    id: frame.id.clone(),
                    expanded: true,
                });
                let members = ctx
                    .subgraph_members_by_id
                    .get(frame.id.as_str())
                    .map(Vec::as_slice)
                    .unwrap_or_default();
                for member in members.iter().rev() {
                    if ctx.subgraphs_by_id.contains_key(*member)
                        && !state.cluster_rects.contains_key(*member)
                    {
                        stack.push(ClusterRectFrame {
                            id: (*member).to_string(),
                            expanded: false,
                        });
                    }
                }
                continue;
            }

            let Some(sg) = ctx.subgraphs_by_id.get(frame.id.as_str()) else {
                return Err(Error::InvalidModel {
                    message: format!("missing subgraph definition for {}", frame.id),
                });
            };

            let mut content: Option<Rect> = None;
            let members = ctx
                .subgraph_members_by_id
                .get(frame.id.as_str())
                .map(Vec::as_slice)
                .unwrap_or_default();
            for member in members {
                let member_rect = if let Some(r) = ctx.leaf_rects.get(*member).copied() {
                    Some(r)
                } else if ctx.subgraphs_by_id.contains_key(*member) {
                    Some(state.cluster_rects.get(*member).copied().ok_or_else(|| {
                        Error::InvalidModel {
                            message: format!("missing computed subgraph rect for {member}"),
                        }
                    })?)
                } else {
                    None
                };

                if let Some(r) = member_rect {
                    if let Some(ref mut cur) = content {
                        cur.union(r);
                    } else {
                        content = Some(r);
                    }
                }
            }

            if let Some(extra) = ctx.extra_children.get(&frame.id) {
                for child in extra {
                    if let Some(r) = ctx.leaf_rects.get(child).copied() {
                        if let Some(ref mut cur) = content {
                            cur.union(r);
                        } else {
                            content = Some(r);
                        }
                    }
                }
            }

            let label_type = sg.label_type.as_deref().unwrap_or("text");
            let subgraph_index = ctx
                .subgraph_index_by_id
                .get(frame.id.as_str())
                .copied()
                .ok_or_else(|| Error::InvalidModel {
                    message: format!("missing canonical subgraph index for {}", frame.id),
                })?;
            let title = ctx.model.subgraph_title_for_render(subgraph_index, sg);
            let title_width_limit = (label_type == "markdown").then_some(ctx.title_wrapping_width);
            let base_style = if ctx.wrap_mode == WrapMode::HtmlLike {
                ctx.html_label_text_style
            } else {
                ctx.text_style
            };
            let (classes, styles) = ctx.model.effective_subgraph_css(subgraph_index, sg);
            let text_style = flowchart_effective_text_style_for_classes(
                base_style,
                ctx.class_defs,
                classes,
                styles,
            );
            let owner = ctx
                .subgraph_index_by_id
                .get(frame.id.as_str())
                .copied()
                .map(FlowchartSvgLabelOwner::SubgraphTitle);
            let title_metrics = measure_flowchart_svg_label_for_layout(
                ctx.svg_label_sidecar,
                owner,
                Some(frame.id.as_str()),
                FlowchartLabelMetricsRequest {
                    measurer: ctx.measurer,
                    raw_label: title,
                    label_type,
                    style: text_style.as_ref(),
                    max_width_px: title_width_limit,
                    wrap_mode: ctx.wrap_mode,
                    config: ctx.config,
                    math_renderer: ctx.math_renderer,
                },
                FlowchartSvgWidthMode::Bbox,
            );
            let mut rect = if let Some(r) = content {
                r
            } else {
                Rect::from_center(
                    0.0,
                    0.0,
                    title_metrics.width.max(1.0),
                    title_metrics.height.max(1.0),
                )
            };

            // Expand to provide the cluster's internal padding.
            rect.pad(ctx.cluster_padding);

            // Mermaid computes `node.diff` using the pre-widened layout node width, then may widen the
            // rect to fit the label bbox during rendering.
            let base_width = rect.width();

            // Mermaid 11.16 `rendering-elements/clusters.js` sets the rect width to
            // `max(node.width, labelBBox.width + node.padding)`.
            let min_width = title_metrics.width.max(1.0) + ctx.cluster_padding;
            if rect.width() < min_width {
                let (cx, cy) = rect.center();
                rect = Rect::from_center(cx, cy, min_width, rect.height());
            }

            // Extend height to reserve space for subgraph title margins (Mermaid does this after layout).
            if ctx.title_total_margin > 0.0 {
                let (cx, cy) = rect.center();
                rect =
                    Rect::from_center(cx, cy, rect.width(), rect.height() + ctx.title_total_margin);
            }

            // Keep the cluster tall enough to accommodate the title bbox if needed.
            let min_height = title_metrics.height.max(1.0) + ctx.title_total_margin;
            if rect.height() < min_height {
                let (cx, cy) = rect.center();
                rect = Rect::from_center(cx, cy, rect.width(), min_height);
            }

            state.visiting.remove(&frame.id);
            state.cluster_rects.insert(frame.id.clone(), rect);
            state.cluster_base_widths.insert(frame.id, base_width);
        }

        let rect = state
            .cluster_rects
            .get(id)
            .copied()
            .ok_or_else(|| Error::InvalidModel {
                message: format!("missing computed subgraph rect for {id}"),
            })?;
        let base_width = state
            .cluster_base_widths
            .get(id)
            .copied()
            .unwrap_or_else(|| rect.width());
        Ok((rect, base_width))
    }

    struct ClusterTitleAdjustContext<'a> {
        model: &'a FlowchartRenderModelRef<'a>,
        class_defs: &'a merman_core::native_collections::OrderedMap<String, Vec<String>>,
        subgraph_index_by_id: &'a HashMap<&'a str, usize>,
        measurer: &'a dyn TextMeasurer,
        text_style: &'a TextStyle,
        html_label_text_style: &'a TextStyle,
        title_wrapping_width: f64,
        wrap_mode: WrapMode,
        config: &'a MermaidConfig,
        math_renderer: Option<&'a (dyn MathRenderer + Send + Sync)>,
        title_total_margin: f64,
        cluster_padding: f64,
        svg_label_sidecar: Option<&'a FlowchartSvgLabelSidecarBuilder>,
    }

    fn adjust_cluster_rect_for_title(
        mut rect: Rect,
        declaration_ordinal: usize,
        sg: &FlowSubgraph,
        title: &str,
        label_type: &str,
        add_title_total_margin: bool,
        ctx: &ClusterTitleAdjustContext<'_>,
    ) -> Rect {
        let title_width_limit = (label_type == "markdown").then_some(ctx.title_wrapping_width);
        let base_style = if ctx.wrap_mode == WrapMode::HtmlLike {
            ctx.html_label_text_style
        } else {
            ctx.text_style
        };
        let (classes, styles) = ctx.model.effective_subgraph_css(declaration_ordinal, sg);
        let text_style =
            flowchart_effective_text_style_for_classes(base_style, ctx.class_defs, classes, styles);
        let owner = ctx
            .subgraph_index_by_id
            .get(sg.id.as_str())
            .copied()
            .map(FlowchartSvgLabelOwner::SubgraphTitle);
        let title_metrics = measure_flowchart_svg_label_for_layout(
            ctx.svg_label_sidecar,
            owner,
            Some(sg.id.as_str()),
            FlowchartLabelMetricsRequest {
                measurer: ctx.measurer,
                raw_label: title,
                label_type,
                style: text_style.as_ref(),
                max_width_px: title_width_limit,
                wrap_mode: ctx.wrap_mode,
                config: ctx.config,
                math_renderer: ctx.math_renderer,
            },
            FlowchartSvgWidthMode::Bbox,
        );
        let title_w = title_metrics.width.max(1.0);
        let title_h = title_metrics.height.max(1.0);

        // Mermaid cluster "rect" widens to fit the raw title bbox (no added padding),
        // even when the cluster bounds come from Dagre border nodes.
        let min_w = title_w + ctx.cluster_padding;
        if rect.width() < min_w {
            let (cx, cy) = rect.center();
            rect = Rect::from_center(cx, cy, min_w, rect.height());
        }

        // Mermaid adds `subGraphTitleTotalMargin` to cluster height after layout.
        if add_title_total_margin && ctx.title_total_margin > 0.0 {
            let (cx, cy) = rect.center();
            rect = Rect::from_center(cx, cy, rect.width(), rect.height() + ctx.title_total_margin);
        }

        // Keep the cluster tall enough for the title bbox (including title margins).
        let min_h = title_h + ctx.title_total_margin;
        if rect.height() < min_h {
            let (cx, cy) = rect.center();
            rect = Rect::from_center(cx, cy, rect.width(), min_h);
        }

        rect
    }

    let cluster_rect_ctx = ClusterRectContext {
        model,
        subgraphs_by_id: &subgraphs_by_id,
        subgraph_index_by_id: &subgraph_index_by_id,
        subgraph_members_by_id: &subgraph_members_by_id,
        class_defs: &model.class_defs,
        leaf_rects: &leaf_rects,
        extra_children: &extra_children,
        measurer,
        text_style: &text_style,
        html_label_text_style: &html_label_text_style,
        title_wrapping_width: cluster_title_wrapping_width,
        wrap_mode: cluster_wrap_mode,
        config: effective_config,
        math_renderer,
        cluster_padding,
        title_total_margin,
        svg_label_sidecar,
    };
    let mut cluster_rect_state = ClusterRectState {
        cluster_rects: &mut cluster_rects,
        cluster_base_widths: &mut cluster_base_widths,
        visiting: &mut visiting,
    };
    let title_adjust_ctx = ClusterTitleAdjustContext {
        model,
        class_defs: &model.class_defs,
        subgraph_index_by_id: &subgraph_index_by_id,
        measurer,
        text_style: &text_style,
        html_label_text_style: &html_label_text_style,
        title_wrapping_width: cluster_title_wrapping_width,
        wrap_mode: cluster_wrap_mode,
        config: effective_config,
        math_renderer,
        title_total_margin,
        cluster_padding,
        svg_label_sidecar,
    };

    for &(subgraph_index, sg) in &canonical_subgraphs_in_order {
        if !cluster_subgraph_ids.contains(sg.id.as_str()) {
            continue;
        }
        let title = model.subgraph_title_for_render(subgraph_index, sg);

        let (rect, base_width) = if extracted_graphs.contains_key(&sg.id) {
            // For extracted (recursive) clusters, match Mermaid's `updateNodeBounds(...)` intent by
            // taking the rendered child-graph content bbox (including border nodes) as the cluster
            // node's bounds.
            let rect = extracted_cluster_rects
                .get(&sg.id)
                .copied()
                .unwrap_or_else(|| {
                    compute_cluster_rect(&sg.id, &cluster_rect_ctx, &mut cluster_rect_state)
                        .map(|v| v.0)
                        .unwrap_or_else(|_| Rect::from_center(0.0, 0.0, 1.0, 1.0))
                });
            let base_width = extracted_cluster_base_widths
                .get(&sg.id)
                .copied()
                .unwrap_or_else(|| rect.width());
            let rect = adjust_cluster_rect_for_title(
                rect,
                subgraph_index,
                sg,
                title,
                sg.label_type.as_deref().unwrap_or("text"),
                false,
                &title_adjust_ctx,
            );
            (rect, base_width)
        } else if let Some(r) = cluster_rects_from_graph.get(&sg.id).copied() {
            let base_width = r.width();
            let rect = adjust_cluster_rect_for_title(
                r,
                subgraph_index,
                sg,
                title,
                sg.label_type.as_deref().unwrap_or("text"),
                true,
                &title_adjust_ctx,
            );
            (rect, base_width)
        } else {
            compute_cluster_rect(&sg.id, &cluster_rect_ctx, &mut cluster_rect_state)?
        };
        let (cx, cy) = rect.center();

        let label_type = sg.label_type.as_deref().unwrap_or("text");
        let title_width_limit = (label_type == "markdown").then_some(cluster_title_wrapping_width);
        let base_style = if cluster_wrap_mode == WrapMode::HtmlLike {
            &html_label_text_style
        } else {
            &text_style
        };
        let (classes, styles) = model.effective_subgraph_css(subgraph_index, sg);
        let title_text_style = flowchart_effective_text_style_for_classes(
            base_style,
            &model.class_defs,
            classes,
            styles,
        );
        let title_metrics = measure_flowchart_svg_label_for_layout(
            svg_label_sidecar,
            Some(FlowchartSvgLabelOwner::SubgraphTitle(subgraph_index)),
            Some(sg.id.as_str()),
            FlowchartLabelMetricsRequest {
                measurer,
                raw_label: title,
                label_type,
                style: title_text_style.as_ref(),
                max_width_px: title_width_limit,
                wrap_mode: cluster_wrap_mode,
                config: effective_config,
                math_renderer,
            },
            FlowchartSvgWidthMode::Bbox,
        );
        let title_label = LayoutLabel {
            x: cx,
            y: cy - rect.height() / 2.0 + title_margin_top + title_metrics.height / 2.0,
            width: title_metrics.width,
            height: title_metrics.height,
        };

        // `dagre-wrapper/clusters.js` (shape `rect`) sets `padding = 0 * node.padding`.
        // The cluster label is positioned at `node.x - bbox.width/2`, and `node.diff` is:
        // - `(bbox.width - node.width)/2 - node.padding/2` when the box widens to fit the title
        // - otherwise `-node.padding/2`.
        let title_w = title_metrics.width.max(1.0);
        let diff = if base_width <= title_w {
            (title_w - base_width) / 2.0 - cluster_padding / 2.0
        } else {
            -cluster_padding / 2.0
        };
        let offset_y = title_metrics.height - cluster_padding / 2.0;

        let effective_dir = effective_dir_by_id
            .get(&sg.id)
            .cloned()
            .unwrap_or_else(|| effective_cluster_dir(sg, &diagram_direction, inherit_dir));

        clusters.push(LayoutCluster {
            id: sg.id.clone(),
            x: cx,
            y: cy,
            width: rect.width(),
            height: rect.height(),
            diff,
            offset_y,
            title: sg.title.clone(),
            title_label,
            requested_dir: sg.dir.as_ref().map(|s| normalize_dir(s)),
            effective_dir,
            padding: cluster_padding,
            title_margin_top,
            title_margin_bottom,
        });

        out_nodes.push(LayoutNode {
            id: sg.id.clone(),
            x: cx,
            // Mermaid does not shift pure cluster nodes by `subGraphTitleTotalMargin / 2`.
            y: cy,
            width: rect.width(),
            height: rect.height(),
            is_cluster: true,
            label_width: None,
            label_height: None,
        });
    }
    let cluster_sort_work = checked_n_log_n(clusters.len(), work_control)?;
    work_control.charge_adapter(cluster_sort_work)?;
    clusters.sort_by(|a, b| a.id.cmp(&b.id));

    let mut out_edge_candidates: Vec<FlowchartLayoutEdgeCandidate> = Vec::new();
    work_control.charge_adapter(g.edge_slot_count())?;
    let root_edge_point_work = g.edges().try_fold(0usize, |total, edge_key| {
        let points = g.edge_by_key(edge_key).map_or(0, |edge| edge.points.len());
        work_control.checked_add(total, points)
    })?;
    let mut extracted_edge_point_work = 0usize;
    for id in &extracted_order {
        let graph = extracted_graphs
            .get(id)
            .expect("the extraction order references an extracted graph");
        work_control.charge_adapter(graph.edge_slot_count())?;
        extracted_edge_point_work =
            graph
                .edges()
                .try_fold(extracted_edge_point_work, |inner_total, edge_key| {
                    let points = graph
                        .edge_by_key(edge_key)
                        .map_or(0, |edge| edge.points.len());
                    work_control.checked_add(inner_total, points)
                })?;
    }
    let edge_projection_work = work_control.checked_add(
        render_edges.len(),
        work_control.checked_add(root_edge_point_work, extracted_edge_point_work)?,
    )?;
    work_control.charge_adapter(edge_projection_work)?;
    for (e, expected_self_loop_meta) in render_edges.iter().zip(&render_edge_self_loop_meta) {
        let (
            points,
            label_pos,
            mut from_cluster,
            mut to_cluster,
            layout_from,
            layout_to,
            actual_self_loop_meta,
        ) = if let Some(points) = edge_override_points.get(&e.id) {
            let from_cluster = edge_override_from_cluster
                .get(&e.id)
                .cloned()
                .unwrap_or(None);
            let to_cluster = edge_override_to_cluster.get(&e.id).cloned().unwrap_or(None);
            (
                points.clone(),
                edge_override_label.get(&e.id).cloned().unwrap_or(None),
                from_cluster,
                to_cluster,
                edge_override_endpoints
                    .get(&e.id)
                    .map(|(from, _)| from.clone())
                    .unwrap_or_else(|| e.from.clone()),
                edge_override_endpoints
                    .get(&e.id)
                    .map(|(_, to)| to.clone())
                    .unwrap_or_else(|| e.to.clone()),
                edge_override_self_loop_meta.get(&e.id).cloned(),
            )
        } else {
            let (v, w) = edge_endpoints_by_id
                .get(&e.id)
                .cloned()
                .unwrap_or_else(|| (e.from.clone(), e.to.clone()));
            let edge_key = edge_key_by_id
                .get(&e.id)
                .map(String::as_str)
                .unwrap_or(e.id.as_str());
            let Some(label) = g.edge(&v, &w, Some(edge_key)) else {
                return Err(Error::InvalidModel {
                    message: format!("missing layout edge {}", e.id),
                });
            };
            let from_cluster = label
                .extras
                .get("fromCluster")
                .and_then(|v| v.as_str().map(|s| s.to_string()));
            let to_cluster = label
                .extras
                .get("toCluster")
                .and_then(|v| v.as_str().map(|s| s.to_string()));
            let points = label
                .points
                .iter()
                .map(|p| LayoutPoint { x: p.x, y: p.y })
                .collect::<Vec<_>>();
            let label_pos = match (label.x, label.y) {
                (Some(x), Some(y)) if label.width > 0.0 || label.height > 0.0 => {
                    Some(LayoutLabel {
                        x,
                        y,
                        width: label.width,
                        height: label.height,
                    })
                }
                _ => None,
            };
            let self_loop_meta = flowchart_self_loop_segment_meta(label);
            (
                points,
                label_pos,
                from_cluster,
                to_cluster,
                v,
                w,
                self_loop_meta,
            )
        };

        // Graphlib identifies a multiedge by (v, w, name). Parallel self-loops reuse all three
        // helper keys, so the later logical edge overwrites the earlier triple. Only materialize
        // the helper segments whose metadata survived that overwrite.
        if expected_self_loop_meta.is_some()
            && actual_self_loop_meta.as_ref() != expected_self_loop_meta.as_ref()
        {
            continue;
        }

        // Match Mermaid's dagre adapter: self-loop special edges on group nodes are annotated with
        // `fromCluster` / `toCluster` so downstream renderers can clip routes to the cluster
        // boundary.
        if cluster_subgraph_ids.contains(e.from.as_str())
            && actual_self_loop_meta
                .as_ref()
                .is_some_and(|meta| meta.order == 0)
        {
            from_cluster = Some(e.from.clone());
        }
        if cluster_subgraph_ids.contains(e.to.as_str())
            && actual_self_loop_meta
                .as_ref()
                .is_some_and(|meta| meta.order == 2)
        {
            to_cluster = Some(e.to.clone());
        }

        out_edge_candidates.push(FlowchartLayoutEdgeCandidate {
            edge: LayoutEdge {
                id: e.id.clone(),
                from: layout_from,
                to: layout_to,
                from_cluster,
                to_cluster,
                points,
                label: label_pos,
                start_label_left: None,
                start_label_right: None,
                end_label_left: None,
                end_label_right: None,
                start_marker: None,
                end_marker: None,
                stroke_dasharray: None,
            },
            self_loop: actual_self_loop_meta,
        });
    }

    let merge_work = work_control.checked_add(
        work_control.checked_add(model.edges.len(), out_edge_candidates.len())?,
        out_nodes.len(),
    )?;
    work_control.charge_adapter(merge_work)?;
    let mut out_edges = merge_flowchart_self_loop_segments(
        &projected_edges,
        &out_nodes,
        &diagram_direction,
        out_edge_candidates,
    );

    // Mermaid's flowchart renderer uses shape-specific intersection functions for edge endpoints
    // (e.g. diamond nodes). Our Dagre-ish layout currently treats all nodes as rectangles, so the
    // first/last points can land on the bounding box rather than the actual polygon boundary.
    //
    // Adjust the first/last edge points to match Mermaid's shape intersection behavior for the
    // shapes that materially differ from rectangles.
    let endpoint_work = work_control.checked_add(
        work_control.checked_add(model.nodes.len(), out_nodes.len())?,
        out_edges.len(),
    )?;
    work_control.charge_adapter(endpoint_work)?;
    let mut node_shape_by_id: HashMap<&str, &str> = HashMap::new();
    for n in &model.nodes {
        if let Some(s) = n.layout_shape.as_deref() {
            node_shape_by_id.insert(n.id.as_str(), s);
        }
    }
    let mut layout_node_by_id: HashMap<&str, &LayoutNode> = HashMap::new();
    for n in &out_nodes {
        layout_node_by_id.insert(n.id.as_str(), n);
    }

    fn diamond_intersection(node: &LayoutNode, toward: &LayoutPoint) -> Option<LayoutPoint> {
        let vx = toward.x - node.x;
        let vy = toward.y - node.y;
        if !(vx.is_finite() && vy.is_finite()) {
            return None;
        }
        if vx.abs() <= 1e-12 && vy.abs() <= 1e-12 {
            return None;
        }
        let hw = (node.width / 2.0).max(1e-9);
        let hh = (node.height / 2.0).max(1e-9);
        let denom = vx.abs() / hw + vy.abs() / hh;
        if !(denom.is_finite() && denom > 0.0) {
            return None;
        }
        let t = 1.0 / denom;
        Some(LayoutPoint {
            x: node.x + vx * t,
            y: node.y + vy * t,
        })
    }

    for e in &mut out_edges {
        if e.points.len() < 2 {
            continue;
        }

        if let Some(node) = layout_node_by_id.get(e.from.as_str())
            && !node.is_cluster
        {
            let shape = node_shape_by_id
                .get(e.from.as_str())
                .copied()
                .unwrap_or("squareRect");
            if matches!(shape, "diamond" | "question" | "diam")
                && let Some(p) = diamond_intersection(node, &e.points[1])
            {
                e.points[0] = p;
            }
        }
        if let Some(node) = layout_node_by_id.get(e.to.as_str())
            && !node.is_cluster
        {
            let shape = node_shape_by_id
                .get(e.to.as_str())
                .copied()
                .unwrap_or("squareRect");
            if matches!(shape, "diamond" | "question" | "diam") {
                let n = e.points.len();
                if let Some(p) = diamond_intersection(node, &e.points[n - 2]) {
                    e.points[n - 1] = p;
                }
            }
        }
    }

    let bounds = compute_bounds_controlled(&out_nodes, &out_edges, |units| {
        work_control.charge_adapter(units)
    })?;

    Ok(FlowchartLayout {
        nodes: out_nodes,
        edges: out_edges,
        clusters,
        bounds,
        dom_node_order_by_root,
        uses_elk_adapter_dom: false,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use merman_core::{
        CancelReason, Engine, OperationControl, OperationPhase, ParseOptions, RenderSemanticModel,
    };
    use std::sync::mpsc;
    use std::time::Duration;

    const DEEP_SUBGRAPH_DEPTH: usize = 10_000;
    const NON_LATTICE_COMPUTED_LENGTH_PX: f64 = 73.123_456_789;

    struct NonLatticeComputedLengthMeasurer;

    impl TextMeasurer for NonLatticeComputedLengthMeasurer {
        fn measure(&self, _text: &str, _style: &TextStyle) -> crate::text::TextMetrics {
            crate::text::TextMetrics {
                width: 40.0,
                height: 18.0,
                line_count: 1,
            }
        }

        fn measure_svg_text_computed_length_px(&self, _text: &str, _style: &TextStyle) -> f64 {
            NON_LATTICE_COMPUTED_LENGTH_PX
        }
    }

    #[test]
    fn dagre_wrapped_labels_fit_rectangle_and_diamond_nodes() {
        let parsed = Engine::new()
            .parse_diagram_for_render_model_sync(
                r#"%%{init: {"flowchart": {"wrappingWidth": 120}}}%%
flowchart TD
    A["Receive a new deployment request from the release pipeline"] --> B{"Did every required validation and security check pass successfully?"}
    B -->|Yes| C["Publish the approved application version to the production environment"]
    B -->|No| D["Return the detailed validation errors to the requesting developer"]
"#,
                ParseOptions::default(),
            )
            .expect("parse ok")
            .expect("diagram detected");
        let RenderSemanticModel::Flowchart(model) = parsed.model() else {
            panic!("expected Flowchart render model");
        };
        let measurer = crate::text::DeterministicTextMeasurer::default()
            .with_width_callback(|text, _| text.chars().count() as f64 * 10.0);
        let layout = layout_flowchart_typed(
            model,
            &parsed.metadata().effective_config,
            measurer.as_ref(),
            None,
        )
        .expect("layout ok");

        for id in ["A", "C", "D"] {
            let node = layout
                .nodes
                .iter()
                .find(|node| node.id == id)
                .unwrap_or_else(|| panic!("node {id}"));
            let label_width = node.label_width.expect("rectangle label width");
            let label_height = node.label_height.expect("rectangle label height");
            assert!(
                label_height > 24.0,
                "node {id} label did not wrap: {node:?}"
            );
            assert!(label_width < node.width, "node {id}: {node:?}");
            assert!(label_height < node.height, "node {id}: {node:?}");
        }

        let diamond = layout
            .nodes
            .iter()
            .find(|node| node.id == "B")
            .expect("diamond node B");
        let label_width = diamond.label_width.expect("diamond label width");
        let label_height = diamond.label_height.expect("diamond label height");
        assert!(
            label_height > 24.0,
            "diamond label did not wrap: {diamond:?}"
        );
        assert_eq!(diamond.width, diamond.height);
        assert!(
            label_width + label_height < diamond.width,
            "diamond label does not fit inside its sloped sides: {diamond:?}"
        );
    }

    #[test]
    fn dagre_preserves_operation_computed_length_precision() {
        // Isolate host precision from Mermaid 12's minimum label width.
        let parsed = Engine::new()
            .parse_diagram_for_render_model_sync(
                "%%{init: {\"htmlLabels\": false, \"flowchart\": {\"htmlLabels\": false, \"minNodeWidth\": 0}}}%%\nflowchart TB\nA[alpha]\n",
                ParseOptions::default(),
            )
            .expect("parse ok")
            .expect("diagram detected");
        let RenderSemanticModel::Flowchart(model) = parsed.model() else {
            panic!("expected Flowchart render model");
        };
        let layout = layout_flowchart_typed(
            model,
            &parsed.metadata().effective_config,
            &NonLatticeComputedLengthMeasurer,
            None,
        )
        .expect("layout ok");
        let node = layout
            .nodes
            .iter()
            .find(|node| node.id == "A")
            .expect("node A");

        assert_eq!(node.label_width, Some(NON_LATTICE_COMPUTED_LENGTH_PX));
    }

    #[test]
    fn dagre_nonempty_subgraph_title_reuses_its_prepared_layout_measurement() {
        let parsed = Engine::new()
            .parse_diagram_for_render_model_sync(
                "---\nconfig:\n  htmlLabels: false\n  flowchart:\n    htmlLabels: false\n---\nflowchart TD\nsubgraph S[service title]\n  A\nend\n",
                ParseOptions::default(),
            )
            .expect("parse ok")
            .expect("diagram detected");
        let RenderSemanticModel::Flowchart(model) = parsed.model() else {
            panic!("expected Flowchart render model");
        };
        let labels = FlowchartRenderContext::default();
        let builder = FlowchartSvgLabelSidecarBuilder::default();
        let session = crate::environment::RenderEnvironment::deterministic()
            .begin_session()
            .expect("render session");
        let measurer = session.text_measurer(crate::environment::TextMeasurementPhase::Layout);
        let meter = Arc::new(OperationWorkMeter::new(
            RenderResourcePolicy::unbounded_for_trusted_input(),
        ));
        let mut work_control = DagreOperationWorkControl::new(meter);

        layout_flowchart_with_model(
            model,
            &labels,
            &parsed.metadata().effective_config,
            &measurer,
            None,
            Some(&builder),
            &mut work_control,
        )
        .expect("layout ok");

        assert!(builder.prepared_count() > 0);
        assert!(
            builder.prepared_hit_count() > 0,
            "the cluster rect/title stages must reuse the same built-in measurement"
        );
    }

    #[test]
    fn dagre_html_nbsp_only_edge_labels_respect_source_provenance() {
        let nbsp = '\u{00A0}';
        let source = format!("flowchart LR\nA -- \"&nbsp;\" --> B\nC -- \"{nbsp}\" --> D\n");
        let parsed = Engine::new()
            .parse_diagram_for_render_model_sync(&source, ParseOptions::default())
            .expect("parse ok")
            .expect("diagram detected");
        let RenderSemanticModel::Flowchart(model) = parsed.model() else {
            panic!("expected Flowchart render model");
        };
        let layout = layout_flowchart_typed(
            model,
            &parsed.metadata().effective_config,
            &crate::text::DeterministicTextMeasurer::default(),
            None,
        )
        .expect("layout ok");

        assert_eq!(layout.edges.len(), 2);
        let entity_label = layout
            .edges
            .iter()
            .find(|edge| edge.from == "A")
            .and_then(|edge| edge.label.as_ref())
            .expect("entity-authored NBSP label must reach Dagre");
        assert!(entity_label.width > 0.0, "{entity_label:?}");
        assert!(entity_label.height > 0.0, "{entity_label:?}");

        let direct_label = layout
            .edges
            .iter()
            .find(|edge| edge.from == "C")
            .and_then(|edge| edge.label.as_ref());
        assert!(direct_label.is_none(), "{direct_label:?}");
    }

    #[test]
    fn dagre_operation_meter_honors_below_equal_and_above_boundaries() {
        let parsed = Engine::new()
            .parse_diagram_for_render_model_sync(
                "flowchart TB\nsubgraph Outer\nsubgraph Inner\nA-->B\nend\nB-->C\nend\nC-->C\n",
                ParseOptions::default(),
            )
            .expect("parse ok")
            .expect("diagram detected");
        let RenderSemanticModel::Flowchart(model) = parsed.model() else {
            panic!("expected Flowchart render model");
        };
        let config = &parsed.metadata().effective_config;
        let measurer = crate::text::DeterministicTextMeasurer::default();

        let unbounded_meter = Arc::new(OperationWorkMeter::new(
            RenderResourcePolicy::unbounded_for_trusted_input(),
        ));
        let expected = layout_flowchart_typed_with_work_meter(
            model,
            config,
            &measurer,
            None,
            Arc::clone(&unbounded_meter),
        )
        .expect("unbounded Dagre layout succeeds");
        let exact = unbounded_meter.used();
        assert!(exact > 1);

        let below_meter = Arc::new(OperationWorkMeter::new(
            RenderResourcePolicy::unbounded_for_trusted_input()
                .with_limit(crate::ResourceLimitId::MaxLayoutWorkUnits, exact - 1)
                .unwrap(),
        ));
        let error = layout_flowchart_typed_with_work_meter(
            model,
            config,
            &measurer,
            None,
            Arc::clone(&below_meter),
        )
        .expect_err("one unit below the exact Dagre work must fail");
        let Error::ResourceLimitExceeded(limit) = error else {
            panic!("expected ResourceLimitExceeded");
        };
        assert_eq!(limit.actual, exact);
        assert_eq!(limit.max, exact - 1);
        assert!(below_meter.used() < exact);

        for limit in [exact, exact + 1] {
            let meter = Arc::new(OperationWorkMeter::new(
                RenderResourcePolicy::unbounded_for_trusted_input()
                    .with_limit(crate::ResourceLimitId::MaxLayoutWorkUnits, limit)
                    .unwrap(),
            ));
            let actual = layout_flowchart_typed_with_work_meter(
                model,
                config,
                &measurer,
                None,
                Arc::clone(&meter),
            )
            .expect("equal and above Dagre work budgets succeed");
            assert_eq!(meter.used(), exact);
            assert_eq!(
                actual.dom_node_order_by_root,
                expected.dom_node_order_by_root
            );
            assert_eq!(actual.uses_elk_adapter_dom, expected.uses_elk_adapter_dom);
            assert_eq!(
                serde_json::to_value(&actual).unwrap(),
                serde_json::to_value(&expected).unwrap()
            );
        }
    }

    #[test]
    fn dagre_adapter_rejects_before_a_tombstoned_edge_snapshot() {
        let mut graph = compound_graph();
        for id in ["a", "b", "c", "d"] {
            graph.set_node(id, NodeLabel::default());
        }
        for (from, to) in [("a", "b"), ("b", "c"), ("c", "d"), ("d", "a")] {
            graph.set_edge(from, to);
        }
        assert!(graph.remove_edge("b", "c", None));

        let node_count = graph.node_count();
        let edge_count = graph.edge_count();
        let before_edge_snapshot = graph.node_order_slot_count() + node_count + node_count;
        let edge_snapshot_work = graph.edge_slot_count() + edge_count;
        assert!(
            graph.edge_slot_count() > edge_count,
            "the removed non-tail edge must leave a tombstone"
        );
        let attempted_after_edge_snapshot = before_edge_snapshot + edge_snapshot_work;
        let meter = Arc::new(OperationWorkMeter::new(
            RenderResourcePolicy::unbounded_for_trusted_input()
                .with_limit(
                    crate::ResourceLimitId::MaxLayoutWorkUnits,
                    attempted_after_edge_snapshot - 1,
                )
                .unwrap(),
        ));
        let mut work_control = DagreOperationWorkControl::new(Arc::clone(&meter));

        let error = adjust_flowchart_clusters_and_edges(&mut graph, &mut work_control)
            .expect_err("the complete edge snapshot must be admitted before it is cloned");
        let Error::ResourceLimitExceeded(limit) = error else {
            panic!("expected ResourceLimitExceeded");
        };
        assert_eq!(meter.used(), before_edge_snapshot);
        assert_eq!(limit.actual, attempted_after_edge_snapshot);
        assert_eq!(limit.max, attempted_after_edge_snapshot - 1);

        let sticky = work_control
            .charge_adapter(1)
            .expect_err("a rejected owner tranche must remain sticky");
        let Error::ResourceLimitExceeded(sticky) = sticky else {
            panic!("expected sticky ResourceLimitExceeded");
        };
        assert_eq!(sticky.actual, attempted_after_edge_snapshot);
        assert_eq!(meter.used(), before_edge_snapshot);
    }

    #[test]
    fn dagre_adapter_checked_work_overflow_maps_to_the_resource_contract() {
        let mut work_control = DagreOperationWorkControl::new(Arc::new(OperationWorkMeter::new(
            RenderResourcePolicy::unbounded_for_trusted_input(),
        )));
        let error = work_control.checked_mul(usize::MAX, 2).unwrap_err();
        let Error::ResourceLimitExceeded(limit) = error else {
            panic!("expected ResourceLimitExceeded");
        };
        assert_eq!(
            limit.cause,
            crate::resources::ResourceLimitCause::ArithmeticOverflow
        );
        assert_eq!(limit.limit, "max_layout_work_units");
        let Error::ResourceLimitExceeded(sticky) = work_control
            .charge_adapter(1)
            .expect_err("arithmetic overflow must remain sticky")
        else {
            panic!("expected sticky ResourceLimitExceeded");
        };
        assert_eq!(sticky, limit);
    }

    #[test]
    fn dagre_kernel_rejection_remains_sticky_after_error_mapping() {
        let meter = Arc::new(OperationWorkMeter::new(
            RenderResourcePolicy::unbounded_for_trusted_input()
                .with_limit(crate::ResourceLimitId::MaxLayoutWorkUnits, 1)
                .unwrap(),
        ));
        let mut work_control = DagreOperationWorkControl::new(Arc::clone(&meter));

        let interrupted = dugong::WorkControl::charge(&mut work_control, 2)
            .expect_err("the kernel charge must exceed the operation budget");
        let Error::ResourceLimitExceeded(mapped) = work_control.map_dugong_error(interrupted)
        else {
            panic!("expected ResourceLimitExceeded");
        };
        let Error::ResourceLimitExceeded(sticky) = work_control
            .charge_adapter(1)
            .expect_err("mapping the kernel error must not consume its sticky rejection")
        else {
            panic!("expected sticky ResourceLimitExceeded");
        };

        assert_eq!(sticky, mapped);
        assert_eq!(meter.used(), 0);
    }

    #[test]
    fn dagre_kernel_interruption_preserves_operation_cancellation() {
        let control = OperationControl::new();
        control.cancel();
        let meter = Arc::new(OperationWorkMeter::new_with_control(
            RenderResourcePolicy::unbounded_for_trusted_input(),
            control,
        ));
        let mut work_control = DagreOperationWorkControl::new(Arc::clone(&meter));

        let interrupted = dugong::WorkControl::charge(&mut work_control, 1)
            .expect_err("the cancelled operation must interrupt the kernel");
        let Error::Cancelled(cancelled) = work_control.map_dugong_error(interrupted) else {
            panic!("expected structured operation cancellation");
        };

        assert_eq!(cancelled.phase, OperationPhase::Layout);
        assert_eq!(cancelled.reason, CancelReason::Requested);
        assert_eq!(meter.used(), 0);
    }

    #[test]
    fn dagre_kernel_invalid_tree_maps_to_invalid_model() {
        let meter = Arc::new(OperationWorkMeter::new(
            RenderResourcePolicy::unbounded_for_trusted_input(),
        ));
        let mut work_control = DagreOperationWorkControl::new(Arc::clone(&meter));

        let Error::InvalidModel { message } =
            work_control.map_dugong_error(dugong::LayoutError::InvalidNetworkSimplexTree)
        else {
            panic!("expected InvalidModel");
        };

        assert_eq!(
            message,
            "network simplex encountered an invalid mutable tree state"
        );
        assert_eq!(meter.used(), 0);
    }

    #[test]
    fn dagre_kernel_arithmetic_overflow_maps_to_the_resource_contract() {
        let parsed = Engine::new()
            .parse_diagram_for_render_model_sync("flowchart TB\nA-->B\n", ParseOptions::default())
            .expect("parse ok")
            .expect("diagram detected");
        let RenderSemanticModel::Flowchart(model) = parsed.model() else {
            panic!("expected Flowchart render model");
        };
        let mut model = model.clone();
        model.edges[0].length = usize::MAX;

        let error = layout_flowchart_typed_with_work_meter(
            &model,
            &parsed.metadata().effective_config,
            &crate::text::DeterministicTextMeasurer::default(),
            None,
            Arc::new(OperationWorkMeter::new(
                RenderResourcePolicy::unbounded_for_trusted_input(),
            )),
        )
        .expect_err("Dugong arithmetic overflow must fail closed");
        let Error::ResourceLimitExceeded(limit) = error else {
            panic!("expected ResourceLimitExceeded");
        };
        assert_eq!(
            limit.cause,
            crate::resources::ResourceLimitCause::ArithmeticOverflow
        );
        assert_eq!(limit.limit, "max_layout_work_units");
    }

    #[test]
    fn unknown_shape_is_rejected_instead_of_becoming_a_rectangle() {
        let parsed = Engine::new()
            .parse_diagram_for_render_model_sync(
                "flowchart TB\nA[known]\n",
                ParseOptions::default(),
            )
            .expect("parse ok")
            .expect("diagram detected");
        let RenderSemanticModel::Flowchart(model) = parsed.model() else {
            panic!("expected Flowchart render model");
        };
        let mut model = model.clone();
        model.nodes[0].layout_shape = Some("definitely-unknown".to_string());
        let error = layout_flowchart_typed(
            &model,
            &parsed.metadata().effective_config,
            &crate::text::DeterministicTextMeasurer::default(),
            None,
        )
        .expect_err("unknown Flowchart shapes must not silently become rectangles");
        let Error::InvalidModel { message } = error else {
            panic!("expected InvalidModel for unknown Flowchart shape");
        };
        assert_eq!(
            message,
            "No such shape: definitely-unknown. Please check your syntax."
        );
    }

    #[test]
    fn shape_validation_scan_is_admitted_before_invalid_model_reporting() {
        let parsed = Engine::new()
            .parse_diagram_for_render_model_sync(
                "flowchart TB\nA[known]\nB[also known]\n",
                ParseOptions::default(),
            )
            .expect("parse ok")
            .expect("diagram detected");
        let RenderSemanticModel::Flowchart(model) = parsed.model() else {
            panic!("expected Flowchart render model");
        };
        let mut model = model.clone();
        model.nodes[1].layout_shape = Some("definitely-unknown".to_string());

        let error = layout_flowchart_typed_with_work_meter(
            &model,
            &parsed.metadata().effective_config,
            &crate::text::DeterministicTextMeasurer::default(),
            None,
            Arc::new(OperationWorkMeter::new(
                RenderResourcePolicy::unbounded_for_trusted_input()
                    .with_limit(crate::ResourceLimitId::MaxLayoutWorkUnits, 1)
                    .unwrap(),
            )),
        )
        .expect_err("the node scan must be rejected before inspecting an invalid shape");
        let Error::ResourceLimitExceeded(limit) = error else {
            panic!("expected ResourceLimitExceeded");
        };
        assert_eq!(limit.actual, 2);
        assert_eq!(limit.max, 1);
    }

    #[derive(Debug)]
    struct DeepTraversalOutcome {
        descendant_count: usize,
        anchor: Option<String>,
        root_dir: Option<String>,
        child_dir: Option<String>,
        copied_node_count: usize,
    }

    fn compound_graph() -> Graph<NodeLabel, EdgeLabel, GraphLabel> {
        Graph::new(GraphOptions {
            multigraph: true,
            compound: true,
            directed: true,
        })
    }

    #[test]
    fn numeric_node_insertion_ordering_is_admitted_before_mutation() {
        let graph = compound_graph();
        let exact = 2usize;
        let below_meter = Arc::new(OperationWorkMeter::new(
            RenderResourcePolicy::unbounded_for_trusted_input()
                .with_limit(crate::ResourceLimitId::MaxLayoutWorkUnits, exact - 1)
                .unwrap(),
        ));
        let mut below = DagreOperationWorkControl::new(Arc::clone(&below_meter));
        let error = charge_node_insertion_ordering(&graph, "1", &mut below)
            .expect_err("the numeric node's two ordered-map insertions require admission");
        assert!(matches!(error, Error::ResourceLimitExceeded(_)));
        assert_eq!(below_meter.used(), 0);
        assert!(!graph.has_node("1"));

        for limit in [exact, exact + 1] {
            let meter = Arc::new(OperationWorkMeter::new(
                RenderResourcePolicy::unbounded_for_trusted_input()
                    .with_limit(crate::ResourceLimitId::MaxLayoutWorkUnits, limit)
                    .unwrap(),
            ));
            let mut work_control = DagreOperationWorkControl::new(Arc::clone(&meter));
            charge_node_insertion_ordering(&graph, "1", &mut work_control)
                .expect("equal and above numeric insertion budgets succeed");
            assert_eq!(meter.used(), exact);
            assert!(!graph.has_node("1"));
        }
    }

    #[test]
    fn implicit_numeric_edge_endpoints_are_admitted_before_mutation() {
        let graph = compound_graph();
        let exact = 10usize;
        let below_meter = Arc::new(OperationWorkMeter::new(
            RenderResourcePolicy::unbounded_for_trusted_input()
                .with_limit(crate::ResourceLimitId::MaxLayoutWorkUnits, exact - 1)
                .unwrap(),
        ));
        let mut below = DagreOperationWorkControl::new(Arc::clone(&below_meter));
        let error = charge_edge_endpoint_insertions(&graph, "1", "2", &mut below)
            .expect_err("both missing numeric endpoints require admission before set_edge");
        assert!(matches!(error, Error::ResourceLimitExceeded(_)));
        assert_eq!(below_meter.used(), 0);
        assert_eq!(graph.node_count(), 0);

        for limit in [exact, exact + 1] {
            let meter = Arc::new(OperationWorkMeter::new(
                RenderResourcePolicy::unbounded_for_trusted_input()
                    .with_limit(crate::ResourceLimitId::MaxLayoutWorkUnits, limit)
                    .unwrap(),
            ));
            let mut work_control = DagreOperationWorkControl::new(Arc::clone(&meter));
            charge_edge_endpoint_insertions(&graph, "1", "2", &mut work_control)
                .expect("equal and above endpoint budgets succeed");
            assert_eq!(meter.used(), exact);
            assert_eq!(graph.node_count(), 0);
        }
    }

    #[test]
    fn child_snapshot_work_includes_ordinary_bucket_tombstones() {
        let mut graph = compound_graph();
        for id in ["parent", "other", "a", "b", "c"] {
            graph.set_node(id, NodeLabel::default());
        }
        for child in ["a", "b", "c"] {
            graph.set_parent_ref(child, "parent");
        }
        graph.set_parent_ref("b", "other");

        let work_control = DagreOperationWorkControl::new(Arc::new(OperationWorkMeter::new(
            RenderResourcePolicy::unbounded_for_trusted_input(),
        )));
        assert_eq!(graph.child_count("parent"), 2);
        assert_eq!(graph.child_order_slot_count("parent"), 3);
        assert_eq!(
            child_id_snapshot_work_upper_bound(&graph, "parent", &work_control).unwrap(),
            5
        );
    }

    #[test]
    fn remove_node_numeric_child_promotion_curve_charges_the_logarithmic_term() {
        for width in (0..=10).map(|shift| 1usize << shift) {
            let mut numeric = compound_graph();
            let mut ordinary = compound_graph();
            numeric.set_node("parent", NodeLabel::default());
            ordinary.set_node("parent", NodeLabel::default());
            for index in 0..width {
                let numeric_id = index.to_string();
                numeric.set_node(&numeric_id, NodeLabel::default());
                numeric.set_parent_ref(&numeric_id, "parent");

                let ordinary_id = format!("child-{index}");
                ordinary.set_node(&ordinary_id, NodeLabel::default());
                ordinary.set_parent_ref(&ordinary_id, "parent");
            }

            let meter = Arc::new(OperationWorkMeter::new(
                RenderResourcePolicy::unbounded_for_trusted_input(),
            ));
            let work_control = DagreOperationWorkControl::new(meter);
            let numeric_bound =
                remove_node_work_upper_bound(&numeric, "parent", &work_control).unwrap();
            let ordinary_bound =
                remove_node_work_upper_bound(&ordinary, "parent", &work_control).unwrap();
            let ordered_term =
                ordered_key_update_work_upper_bound(width, width, &work_control).unwrap();
            // Ordinary-key promotion leaves a root tombstone to scan, while array-index
            // promotion removes from the B-tree without that physical slot. Correct for the
            // representation difference before asserting that the logarithmic update term is
            // present in the numeric bound.
            let root_tombstone_savings = ordinary
                .root_child_order_slot_count()
                .saturating_sub(numeric.root_child_order_slot_count());
            assert_eq!(
                numeric_bound + root_tombstone_savings,
                ordinary_bound + ordered_term
            );
        }
    }

    #[test]
    fn flowchart_third_self_loop_segment_uses_mermaid_11_16_graph_key() {
        let edge = FlowEdge {
            id: "A-cyclic-special-2".to_string(),
            from: "A---A---2".to_string(),
            to: "A".to_string(),
            label: Some(String::new()),
            label_type: None,
            edge_type: None,
            arrow: "-->".to_string(),
            start_marker: FlowEdgeMarker::None,
            end_marker: FlowEdgeMarker::Point,
            is_user_defined_id: false,
            stroke: None,
            stroke_kind: FlowEdgeStroke::Normal,
            visibility: FlowEdgeVisibility::Visible,
            interpolate: None,
            classes: Vec::new(),
            style: Vec::new(),
            animate: None,
            animation: None,
            length: 1,
        };
        let meta = FlowchartSelfLoopSegmentMeta {
            logical_edge_id: "L_A_A_0".to_string(),
            node_id: "A".to_string(),
            order: 2,
        };

        assert_eq!(
            flowchart_layout_edge_key(&edge, Some(&meta)),
            "A-cyclic-special-2"
        );
    }

    fn deep_compound_graph(depth: usize) -> Graph<NodeLabel, EdgeLabel, GraphLabel> {
        let mut graph = compound_graph();
        for i in (0..depth).rev() {
            graph.set_parent(format!("n{}", i + 1), format!("n{i}"));
        }
        graph
    }

    fn deep_subgraphs(depth: usize) -> Vec<FlowSubgraph> {
        let mut subgraphs = Vec::with_capacity(depth);
        for i in 0..depth {
            subgraphs.push(FlowSubgraph {
                metadata: None,
                id: format!("n{i}"),
                title: format!("n{i}"),
                dir: None,
                has_explicit_dir: false,
                label_type: None,
                classes: Vec::new(),
                styles: Vec::new(),
                nodes: vec![format!("n{}", i + 1)],
            });
        }
        subgraphs
    }

    #[test]
    fn parent_batch_handles_a_long_acyclic_chain_without_parent_walks() {
        let count = 8_192usize;
        let mut graph = compound_graph();
        graph.set_node("root", NodeLabel::default());
        for index in 0..count {
            graph.set_node(format!("n{index}"), NodeLabel::default());
        }
        let root_ix = graph.node_ix("root").unwrap();
        let assignments = (0..count)
            .map(|index| {
                let child_ix = graph.node_ix(&format!("n{index}")).unwrap();
                let parent_ix = if index == 0 {
                    root_ix
                } else {
                    graph.node_ix(&format!("n{}", index - 1)).unwrap()
                };
                (child_ix, parent_ix)
            })
            .collect::<Vec<_>>();
        let mut work_control = DagreOperationWorkControl::new(Arc::new(OperationWorkMeter::new(
            RenderResourcePolicy::unbounded_for_trusted_input(),
        )));

        apply_unparented_parent_assignments(
            &mut graph,
            &assignments,
            UnparentedParentBatchShape::default(),
            &mut work_control,
        )
        .expect("the deep functional-parent chain is acyclic");
        assert_eq!(graph.parent("n0"), Some("root"));
        assert_eq!(graph.parent(&format!("n{}", count - 1)), Some("n8190"));
    }

    #[test]
    fn parent_batch_matches_mermaid_graphlib_assignment_order() {
        // Mermaid applies the final FlowDB parents through Graphlib `setParent` in this order.
        // The atomic Graphlib batch must identify the same offending assignment so the
        // source-backed error names the same child and parent without a duplicate preflight.
        fn sequential_parent_walk_reference(
            order: &[&str],
            parent_by_id: &HashMap<String, String>,
        ) -> Option<(String, String)> {
            let mut assigned_parent: HashMap<String, String> = HashMap::new();
            for &child in order {
                let Some(parent) = parent_by_id.get(child) else {
                    continue;
                };
                let mut current = Some(parent.as_str());
                while let Some(id) = current {
                    if id == child {
                        return Some((child.to_string(), parent.clone()));
                    }
                    current = assigned_parent.get(id).map(String::as_str);
                }
                assigned_parent.insert(child.to_string(), parent.clone());
            }
            None
        }

        let cases = [
            (
                [("a", "root"), ("b", "root"), ("c", "a"), ("d", "a")],
                vec!["a", "b", "c", "d"],
            ),
            (
                [("a", "b"), ("b", "c"), ("c", "a"), ("d", "root")],
                vec!["a", "b", "c", "d"],
            ),
            (
                [("a", "b"), ("b", "c"), ("c", "a"), ("d", "root")],
                vec!["b", "c", "a", "d"],
            ),
            (
                [("a", "b"), ("b", "a"), ("c", "b"), ("d", "root")],
                vec!["c", "a", "b", "d"],
            ),
            (
                [("a", "a"), ("b", "root"), ("c", "b"), ("d", "c")],
                vec!["b", "c", "a", "d"],
            ),
            (
                [("a", "b"), ("b", "c"), ("c", "a"), ("d", "root")],
                vec!["a", "a", "b", "c", "d"],
            ),
        ];

        for (assignments, order) in cases {
            let parent_by_id = assignments
                .into_iter()
                .map(|(child, parent)| (child.to_string(), parent.to_string()))
                .collect::<HashMap<_, _>>();
            let expected = sequential_parent_walk_reference(&order, &parent_by_id);
            let mut graph = compound_graph();
            for id in order
                .iter()
                .copied()
                .chain(parent_by_id.values().map(String::as_str))
            {
                graph.set_node(id, NodeLabel::default());
            }
            let mut assigned = HashSet::new();
            let parent_assignments = order
                .iter()
                .copied()
                .filter_map(|child| {
                    let parent = parent_by_id.get(child)?;
                    assigned.insert(child).then(|| {
                        (
                            graph.node_ix(child).unwrap(),
                            graph.node_ix(parent).unwrap(),
                        )
                    })
                })
                .collect::<Vec<_>>();
            let mut work_control = DagreOperationWorkControl::new(Arc::new(
                OperationWorkMeter::new(RenderResourcePolicy::unbounded_for_trusted_input()),
            ));
            let actual = apply_unparented_parent_assignments(
                &mut graph,
                &parent_assignments,
                UnparentedParentBatchShape::default(),
                &mut work_control,
            );

            match expected {
                None => actual.expect("the reference parent sequence is acyclic"),
                Some((child, parent)) => {
                    let Error::InvalidModel { message } = actual.unwrap_err() else {
                        panic!("expected InvalidModel for order {order:?}");
                    };
                    assert_eq!(
                        message,
                        format!("Setting {parent} as parent of {child} would create a cycle"),
                        "cycle assignment mismatch for order {order:?}"
                    );
                }
            }
        }
    }

    #[test]
    fn parent_batch_adapter_charges_the_documented_width_and_depth_bound() {
        for size in (0..=10).map(|shift| 1usize << shift) {
            let mut width_graph = compound_graph();
            width_graph.set_node("root", NodeLabel::default());
            for index in 0..size {
                width_graph.set_node(format!("child-{index}"), NodeLabel::default());
            }
            let root_ix = width_graph.node_ix("root").unwrap();
            let width_assignments = (0..size)
                .map(|index| {
                    (
                        width_graph.node_ix(&format!("child-{index}")).unwrap(),
                        root_ix,
                    )
                })
                .collect::<Vec<_>>();
            let width_meter = Arc::new(OperationWorkMeter::new(
                RenderResourcePolicy::unbounded_for_trusted_input(),
            ));
            let mut width_control = DagreOperationWorkControl::new(Arc::clone(&width_meter));
            let expected_width_work = unparented_parent_batch_work_upper_bound(
                width_graph.node_slot_count(),
                0,
                width_assignments.len(),
                0,
                &width_control,
            )
            .unwrap();

            apply_unparented_parent_assignments(
                &mut width_graph,
                &width_assignments,
                UnparentedParentBatchShape::default(),
                &mut width_control,
            )
            .expect("the unbounded width batch succeeds");

            assert_eq!(width_meter.used(), expected_width_work);
            assert_eq!(width_graph.child_count("root"), size);
            assert_eq!(
                width_graph.children("root").first().copied(),
                Some("child-0")
            );
            let last_child = format!("child-{}", size - 1);
            assert_eq!(
                width_graph.children("root").last().copied(),
                Some(last_child.as_str())
            );

            let mut depth_graph = compound_graph();
            for index in 0..=size {
                depth_graph.set_node(format!("node-{index}"), NodeLabel::default());
            }
            let depth_assignments = (1..=size)
                .rev()
                .map(|index| {
                    (
                        depth_graph.node_ix(&format!("node-{index}")).unwrap(),
                        depth_graph.node_ix(&format!("node-{}", index - 1)).unwrap(),
                    )
                })
                .collect::<Vec<_>>();
            let depth_meter = Arc::new(OperationWorkMeter::new(
                RenderResourcePolicy::unbounded_for_trusted_input(),
            ));
            let mut depth_control = DagreOperationWorkControl::new(Arc::clone(&depth_meter));
            let expected_depth_work = unparented_parent_batch_work_upper_bound(
                depth_graph.node_slot_count(),
                0,
                depth_assignments.len(),
                0,
                &depth_control,
            )
            .unwrap();

            apply_unparented_parent_assignments(
                &mut depth_graph,
                &depth_assignments,
                UnparentedParentBatchShape::default(),
                &mut depth_control,
            )
            .expect("the unbounded depth batch succeeds");

            assert_eq!(depth_meter.used(), expected_depth_work);
            assert_eq!(depth_graph.parent("node-0"), None);
            let deepest = format!("node-{size}");
            let expected_parent = format!("node-{}", size - 1);
            assert_eq!(depth_graph.parent(&deepest), Some(expected_parent.as_str()));
        }
    }

    #[test]
    fn parent_batch_adapter_charges_only_real_existing_and_numeric_work() {
        let control = DagreOperationWorkControl::new(Arc::new(OperationWorkMeter::new(
            RenderResourcePolicy::unbounded_for_trusted_input(),
        )));

        assert_eq!(
            unparented_parent_batch_work_upper_bound(4, 0, 3, 0, &control).unwrap(),
            66
        );
        assert_eq!(
            unparented_parent_batch_work_upper_bound(4, 0, 3, 2, &control).unwrap(),
            78
        );
        assert_eq!(
            unparented_parent_batch_work_upper_bound(4, 3, 3, 0, &control).unwrap(),
            102
        );
        assert_eq!(
            unparented_parent_batch_work_upper_bound(4, 3, 0, 0, &control).unwrap(),
            0
        );
    }

    #[test]
    fn flowchart_cluster_traversals_handle_deep_subgraphs_with_small_stack() {
        let (tx, rx) = mpsc::channel();
        std::thread::Builder::new()
            .name("flowchart-deep-subgraph-traversal".to_string())
            .stack_size(512 * 1024)
            .spawn(move || {
                let mut graph = deep_compound_graph(DEEP_SUBGRAPH_DEPTH);
                let mut work_control = DagreOperationWorkControl::new(Arc::new(
                    OperationWorkMeter::new(RenderResourcePolicy::unbounded_for_trusted_input()),
                ));
                let descendants = extract_descendants("n0", &graph, &mut work_control)
                    .expect("unbounded work control accepts descendant extraction");
                let anchor =
                    flowchart_find_non_cluster_child("n0", &graph, "n0", &mut work_control)
                        .expect("unbounded work control accepts anchor lookup");
                let deep_subgraphs = deep_subgraphs(DEEP_SUBGRAPH_DEPTH);
                let deep_subgraphs_by_id = deep_subgraphs
                    .iter()
                    .map(|subgraph| (subgraph.id.as_str(), subgraph))
                    .collect();
                let dirs = compute_effective_dir_by_id(
                    &deep_subgraphs,
                    &deep_subgraphs_by_id,
                    &graph,
                    "TB",
                    false,
                    &mut work_control,
                )
                .expect("unbounded work control accepts effective-direction planning");

                let mut copied = compound_graph();
                let descendants_by_id = HashMap::from([("n0".to_string(), descendants.clone())]);
                copy_cluster(
                    "n0",
                    &mut graph,
                    &mut copied,
                    "n0",
                    &descendants_by_id,
                    &mut work_control,
                )
                .expect("unbounded work control accepts cluster copying");

                tx.send(DeepTraversalOutcome {
                    descendant_count: descendants.len(),
                    anchor,
                    root_dir: dirs.get("n0").cloned(),
                    child_dir: dirs.get("n1").cloned(),
                    copied_node_count: copied.node_count(),
                })
                .unwrap();
            })
            .expect("spawn deep subgraph traversal test");

        let outcome = rx
            .recv_timeout(Duration::from_secs(15))
            .expect("deep subgraph traversal should finish without stack overflow");

        assert_eq!(outcome.descendant_count, DEEP_SUBGRAPH_DEPTH);
        assert_eq!(outcome.anchor, Some(format!("n{DEEP_SUBGRAPH_DEPTH}")));
        assert_eq!(outcome.root_dir.as_deref(), Some("LR"));
        assert_eq!(outcome.child_dir.as_deref(), Some("TB"));
        assert_eq!(outcome.copied_node_count, DEEP_SUBGRAPH_DEPTH);
    }

    #[test]
    fn extract_descendants_handles_deeply_nested_subgraphs() {
        let (tx, rx) = mpsc::channel();
        std::thread::Builder::new()
            .stack_size(512 * 1024)
            .spawn(move || {
                let mut g = Graph::new(GraphOptions {
                    compound: true,
                    ..Default::default()
                });
                for i in (0..DEEP_SUBGRAPH_DEPTH).rev() {
                    g.set_parent(format!("n{}", i + 1), format!("n{i}"));
                }
                let mut work_control = DagreOperationWorkControl::new(Arc::new(
                    OperationWorkMeter::new(RenderResourcePolicy::unbounded_for_trusted_input()),
                ));
                let descendants = extract_descendants("n0", &g, &mut work_control)
                    .expect("unbounded work control accepts descendant extraction");
                let _ = tx.send(descendants);
            })
            .unwrap();
        let descendants = rx
            .recv_timeout(Duration::from_secs(2))
            .expect("extract_descendants overflowed the stack on deep nesting");
        assert_eq!(descendants.len(), DEEP_SUBGRAPH_DEPTH);
    }
}
