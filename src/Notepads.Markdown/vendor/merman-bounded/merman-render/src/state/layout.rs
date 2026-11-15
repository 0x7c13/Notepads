//! State diagram layout implementation (stateDiagram-v2).

use crate::dagre::self_loop::compact_self_loop_geometry;
use crate::layout_work::OperationLayoutWorkControl;
use crate::model::{
    Bounds, LayoutCluster, LayoutEdge, LayoutLabel, LayoutNode, LayoutPoint, StateDiagramLayout,
};
use crate::text::{TextMeasurer, TextStyle, WrapMode};
use crate::{Error, Result};
use dugong::graphlib::{Graph, GraphOptions};
use dugong::{EdgeLabel, GraphLabel, LabelPos, NodeLabel, RankDir};
use merman_core::geom::Size;
use serde_json::Value;
use std::collections::{BTreeMap, HashMap, HashSet};

use super::config::*;
use super::{StateDiagramModel, StateNode};

struct PreparedGraph {
    graph: Graph<NodeLabel, EdgeLabel, GraphLabel>,
    extracted: BTreeMap<String, PreparedGraph>,
    root_cluster_id: Option<String>,
}

trait StatePreparationWorkControl {
    fn charge(&mut self, units: usize) -> Result<()>;
    fn checked_mul(&self, left: usize, right: usize) -> Result<usize>;
}

impl StatePreparationWorkControl for OperationLayoutWorkControl {
    fn charge(&mut self, units: usize) -> Result<()> {
        self.charge_adapter(units)
    }

    fn checked_mul(&self, left: usize, right: usize) -> Result<usize> {
        OperationLayoutWorkControl::checked_mul(self, left, right)
    }
}

#[derive(Default)]
struct NoopStatePreparationWorkControl;

impl StatePreparationWorkControl for NoopStatePreparationWorkControl {
    fn charge(&mut self, _units: usize) -> Result<()> {
        Ok(())
    }

    fn checked_mul(&self, left: usize, right: usize) -> Result<usize> {
        left.checked_mul(right).ok_or_else(|| Error::InvalidModel {
            message: "state preparation work overflowed".to_string(),
        })
    }
}

impl Drop for PreparedGraph {
    fn drop(&mut self) {
        let extracted = std::mem::take(&mut self.extracted);
        let mut stack: Vec<PreparedGraph> = extracted.into_values().collect();
        while let Some(mut graph) = stack.pop() {
            let children = std::mem::take(&mut graph.extracted);
            stack.extend(children.into_values());
        }
    }
}

type Rect = merman_core::geom::Box2;

#[derive(Default)]
struct HiddenPrefixTrieNode {
    children: HashMap<char, usize>,
    terminal: bool,
}

#[derive(Default)]
pub(super) struct HiddenPrefixMatcher {
    nodes: Vec<HiddenPrefixTrieNode>,
}

impl HiddenPrefixMatcher {
    fn from_prefixes(prefixes: impl IntoIterator<Item = String>) -> Self {
        let mut matcher = Self {
            nodes: vec![HiddenPrefixTrieNode::default()],
        };
        for prefix in prefixes {
            let mut node_idx = 0usize;
            for ch in prefix.chars() {
                let next = if let Some(&next) = matcher.nodes[node_idx].children.get(&ch) {
                    next
                } else {
                    let next = matcher.nodes.len();
                    matcher.nodes.push(HiddenPrefixTrieNode::default());
                    matcher.nodes[node_idx].children.insert(ch, next);
                    next
                };
                node_idx = next;
            }
            matcher.nodes[node_idx].terminal = true;
        }
        matcher
    }

    pub(super) fn is_hidden(&self, id: &str) -> bool {
        let mut node_idx = 0usize;
        for (byte_idx, ch) in id.char_indices() {
            let Some(&next) = self.nodes[node_idx].children.get(&ch) else {
                return false;
            };
            node_idx = next;
            let rest = &id[byte_idx + ch.len_utf8()..];
            if self.nodes[node_idx].terminal && (rest.is_empty() || rest.starts_with("----")) {
                return true;
            }
        }
        self.nodes[node_idx].terminal
    }
}

#[derive(Debug, Clone)]
struct EdgeSegment {
    original_id: String,
    logical_self_loop_id: Option<String>,
    segment: i32,
    original_from: String,
    original_to: String,
    from_cluster: Option<String>,
    to_cluster: Option<String>,
    points: Vec<LayoutPoint>,
    label: Option<LayoutLabel>,
}

#[derive(Debug, Clone)]
struct LayoutFragments {
    nodes: HashMap<String, LayoutNode>,
    edge_segments: Vec<EdgeSegment>,
}

struct StateDagreInput {
    graph: Graph<NodeLabel, EdgeLabel, GraphLabel>,
    rankdir: RankDir,
    hidden_prefixes: HiddenPrefixMatcher,
    dagre_id_by_semantic_id: HashMap<String, String>,
    note_group_canonical_by_semantic_id: HashMap<String, String>,
    dir_by_dagre_id: HashMap<String, Option<String>>,
    text_style: TextStyle,
    wrap_mode: WrapMode,
    wrapping_width: f64,
    html_labels: bool,
}

fn get_extras_string(
    extras: &std::collections::BTreeMap<String, Value>,
    key: &str,
) -> Option<String> {
    extras
        .get(key)
        .and_then(|v| v.as_str())
        .map(|s| s.to_string())
}

fn set_extras_string(
    extras: &mut std::collections::BTreeMap<String, Value>,
    key: &str,
    value: &str,
) {
    extras.insert(key.to_string(), Value::String(value.to_string()));
}

fn set_extras_i32(extras: &mut std::collections::BTreeMap<String, Value>, key: &str, value: i32) {
    extras.insert(key.to_string(), Value::Number(value.into()));
}

pub(super) fn edge_label_metrics(
    label: &str,
    measurer: &dyn TextMeasurer,
    text_style: &TextStyle,
    wrap_mode: WrapMode,
) -> (f64, f64) {
    if label.trim().is_empty() {
        return (0.0, 0.0);
    }
    let wrapping_width = crate::text::MERMAID_CREATE_TEXT_DEFAULT_WIDTH_PX;
    let mut metrics = super::measure_state_markdown_label(
        label,
        measurer,
        text_style,
        Some(wrapping_width),
        wrap_mode,
    )
    .metrics;
    // For SVG edge labels, `createText(..., addSvgBackground=true)` adds a background rect with a
    // 2px padding.
    if wrap_mode == WrapMode::SvgLike {
        metrics.width += 4.0;
        metrics.height += 4.0;
    }

    (metrics.width.max(0.0), metrics.height.max(0.0))
}

fn node_label_metrics(
    label: &str,
    wrapping_width: f64,
    node_css_compiled_styles: &[String],
    node_css_styles: &[String],
    measurer: &dyn TextMeasurer,
    text_style: &TextStyle,
    wrap_mode: WrapMode,
) -> (f64, f64) {
    fn parse_css_px_f64(v: &str) -> Option<f64> {
        let t = v.trim();
        let t = t.trim_end_matches(';').trim();
        let t = t.trim_end_matches("!important").trim();
        let t = t.trim_end_matches("px").trim();
        t.parse::<f64>().ok()
    }

    fn parse_text_style_overrides(
        compiled: &[String],
        direct: &[String],
    ) -> (Option<String>, Option<String>, Option<f64>, Option<String>) {
        let mut weight: Option<String> = None;
        let mut font_style: Option<String> = None;
        let mut font_size_px: Option<f64> = None;
        let mut font_family: Option<String> = None;

        for raw in compiled.iter().chain(direct.iter()) {
            let raw = raw.trim().trim_end_matches(';').trim();
            if raw.is_empty() {
                continue;
            }
            let Some((k, v)) = raw.split_once(':') else {
                continue;
            };
            let key = k.trim().to_ascii_lowercase();
            let val = v.trim();
            match key.as_str() {
                "font-weight" => {
                    let val = val.trim_end_matches("!important").trim();
                    if !val.is_empty() {
                        weight = Some(val.to_string());
                    }
                }
                "font-style" => {
                    let val = val.trim_end_matches("!important").trim();
                    if !val.is_empty() {
                        font_style = Some(val.to_string());
                    }
                }
                "font-size" => {
                    if let Some(px) = parse_css_px_f64(val)
                        && px.is_finite()
                        && px > 0.0
                    {
                        font_size_px = Some(px);
                    }
                }
                "font-family" => {
                    let val = val.trim_end_matches("!important").trim();
                    if !val.is_empty() {
                        font_family = Some(val.to_string());
                    }
                }
                _ => {}
            }
        }

        (weight, font_style, font_size_px, font_family)
    }

    let (weight, font_style, font_size_px, font_family) =
        parse_text_style_overrides(node_css_compiled_styles, node_css_styles);
    let mut style = text_style.clone();
    if let Some(px) = font_size_px {
        style.font_size = px;
    }
    if let Some(ff) = font_family {
        style.font_family = Some(ff);
    }
    if let Some(weight) = weight {
        style.font_weight = Some(weight);
    }
    if let Some(font_style) = font_style {
        style.font_style = Some(font_style);
    }

    let metrics = super::measure_state_markdown_label(
        label,
        measurer,
        &style,
        Some(wrapping_width),
        wrap_mode,
    )
    .metrics;

    (metrics.width.max(0.0), metrics.height.max(0.0))
}

pub(super) fn title_label_metrics(
    label: &str,
    measurer: &dyn TextMeasurer,
    text_style: &TextStyle,
    wrap_mode: WrapMode,
) -> (f64, f64) {
    // Mermaid state diagram cluster titles use `createLabel(...)` (nowrap) rather than
    // `createText(...)` (width constrained).
    let decoded = decode_html_entities_once(label);
    let metrics = measurer.measure_wrapped(decoded.as_ref(), text_style, None, wrap_mode);

    (metrics.width.max(0.0), metrics.height.max(0.0))
}

fn extract_descendants(
    graph: &Graph<NodeLabel, EdgeLabel, GraphLabel>,
    id: &str,
    out: &mut Vec<String>,
) {
    let mut visited: HashSet<String> = HashSet::new();
    let mut stack: Vec<String> = graph
        .children(id)
        .iter()
        .rev()
        .map(|s| s.to_string())
        .collect();
    while let Some(node) = stack.pop() {
        if !visited.insert(node.clone()) {
            continue;
        }
        out.push(node.clone());
        let children = graph.children(&node);
        for child in children.iter().rev() {
            stack.push(child.to_string());
        }
    }
}

fn is_descendant(descendants: &HashMap<String, HashSet<String>>, id: &str, ancestor: &str) -> bool {
    descendants
        .get(ancestor)
        .is_some_and(|set| set.contains(id))
}

fn find_common_edges<W: StatePreparationWorkControl>(
    graph: &Graph<NodeLabel, EdgeLabel, GraphLabel>,
    id1: &str,
    id2: &str,
    work_control: &mut W,
) -> Result<Vec<(String, String)>> {
    let edge_scan_work = work_control.checked_mul(graph.edge_slot_count(), 2)?;
    work_control.charge(edge_scan_work)?;
    let edges1: Vec<(String, String)> = graph
        .edge_keys()
        .into_iter()
        .filter(|e| e.v == id1 || e.w == id1)
        .map(|e| (e.v, e.w))
        .collect();
    let edges2: Vec<(String, String)> = graph
        .edge_keys()
        .into_iter()
        .filter(|e| e.v == id2 || e.w == id2)
        .map(|e| (e.v, e.w))
        .collect();

    let edges1_prim: Vec<(String, String)> = edges1
        .into_iter()
        .map(|(v, w)| {
            (
                if v == id1 { id2.to_string() } else { v },
                // Mermaid's `findCommonEdges(...)` has an asymmetry here: it maps the `w` side
                // back to `id1` rather than `id2` (Mermaid@11.12.2).
                if w == id1 { id1.to_string() } else { w },
            )
        })
        .collect();

    let mut out = Vec::new();
    for e1 in edges1_prim {
        if edges2.contains(&e1) {
            out.push(e1);
        }
    }
    Ok(out)
}

fn find_non_cluster_child<W: StatePreparationWorkControl>(
    graph: &Graph<NodeLabel, EdgeLabel, GraphLabel>,
    id: &str,
    cluster_id: &str,
    work_control: &mut W,
) -> Result<Option<String>> {
    let children = graph.children(id);
    if children.is_empty() {
        return Ok(Some(id.to_string()));
    }
    let mut reserve: Option<String> = None;
    let mut visited: HashSet<String> = HashSet::new();
    let mut stack: Vec<String> = children.iter().rev().map(|s| s.to_string()).collect();
    while let Some(node) = stack.pop() {
        if !visited.insert(node.clone()) {
            continue;
        }
        let children = graph.children(&node);
        if !children.is_empty() {
            for child in children.iter().rev() {
                stack.push(child.to_string());
            }
            continue;
        }
        let common_edges = find_common_edges(graph, cluster_id, &node, work_control)?;
        if !common_edges.is_empty() {
            reserve = Some(node);
        } else {
            return Ok(Some(node));
        }
    }
    Ok(reserve)
}

fn state_is_node_in_extractable_cluster(
    graph: &Graph<NodeLabel, EdgeLabel, GraphLabel>,
    node_id: &str,
    root_id: &str,
    external: &HashMap<String, bool>,
) -> bool {
    let mut parent = graph.parent(node_id);
    while let Some(parent_id) = parent {
        if parent_id == root_id {
            break;
        }
        if external
            .get(parent_id)
            .is_some_and(|has_external| !*has_external)
        {
            return true;
        }
        parent = graph.parent(parent_id);
    }
    false
}

fn state_find_safe_anchor_node<W: StatePreparationWorkControl>(
    graph: &Graph<NodeLabel, EdgeLabel, GraphLabel>,
    cluster_id: &str,
    excluded_cluster: &str,
    descendants: &HashMap<String, HashSet<String>>,
    external: &HashMap<String, bool>,
    work_control: &mut W,
) -> Result<Option<String>> {
    work_control.charge(graph.node_slot_count())?;
    for child in graph.children(cluster_id) {
        if child == excluded_cluster || is_descendant(descendants, child, excluded_cluster) {
            continue;
        }

        let Some(candidate) = find_non_cluster_child(graph, child, cluster_id, work_control)?
        else {
            continue;
        };
        if !state_is_node_in_extractable_cluster(graph, &candidate, cluster_id, external) {
            return Ok(Some(candidate));
        }
    }
    Ok(None)
}

fn prepare_graph<W: StatePreparationWorkControl>(
    graph: Graph<NodeLabel, EdgeLabel, GraphLabel>,
    cluster_dir: &impl Fn(&str) -> Option<String>,
    root_cluster_id: Option<String>,
    work_control: &mut W,
) -> Result<PreparedGraph> {
    let mut root = PreparedGraph {
        graph,
        extracted: BTreeMap::new(),
        root_cluster_id,
    };

    let mut stack: Vec<Vec<String>> = vec![Vec::new()];
    while let Some(path) = stack.pop() {
        let prepared = prepared_graph_at_path_mut(&mut root, &path)?;
        let mut child_ids = prepare_graph_one_level(prepared, cluster_dir, work_control)?;
        child_ids.reverse();
        for child_id in child_ids {
            let mut child_path = path.clone();
            child_path.push(child_id);
            stack.push(child_path);
        }
    }

    Ok(root)
}

fn prepared_graph_at_path_mut<'a>(
    root: &'a mut PreparedGraph,
    path: &[String],
) -> Result<&'a mut PreparedGraph> {
    let mut current = root;
    for id in path {
        current = current
            .extracted
            .get_mut(id)
            .ok_or_else(|| Error::InvalidModel {
                message: format!("missing prepared cluster graph: {id}"),
            })?;
    }
    Ok(current)
}

fn prepare_graph_one_level<W: StatePreparationWorkControl>(
    prepared: &mut PreparedGraph,
    cluster_dir: &impl Fn(&str) -> Option<String>,
    work_control: &mut W,
) -> Result<Vec<String>> {
    let graph = &mut prepared.graph;
    let cluster_ids: Vec<String> = graph
        .node_ids()
        .into_iter()
        .filter(|id| !graph.children(id).is_empty())
        .collect();

    let mut descendants: HashMap<String, HashSet<String>> = HashMap::new();
    let mut external: HashMap<String, bool> =
        cluster_ids.iter().map(|id| (id.clone(), false)).collect();

    work_control.charge(graph.edge_slot_count())?;
    let edge_keys = graph.edge_keys();
    if !edge_keys.is_empty() {
        for id in &cluster_ids {
            let mut vec: Vec<String> = Vec::new();
            work_control.charge(graph.node_slot_count())?;
            extract_descendants(graph, id, &mut vec);
            descendants.insert(id.clone(), vec.into_iter().collect());
        }

        for id in &cluster_ids {
            work_control.charge(edge_keys.len())?;
            for e in &edge_keys {
                let d1 = is_descendant(&descendants, &e.v, id);
                let d2 = is_descendant(&descendants, &e.w, id);
                if d1 ^ d2 {
                    external.insert(id.clone(), true);
                    break;
                }
            }
        }
    }

    let mut anchor: HashMap<String, String> = HashMap::new();
    if !edge_keys.is_empty() {
        for id in &cluster_ids {
            let Some(a) = find_non_cluster_child(graph, id, id, work_control)? else {
                continue;
            };
            anchor.insert(id.clone(), a);
        }
    }

    // Match Mermaid 11.16's anchor stabilization before cluster edges are rebound. The first
    // leaf selected by findNonClusterChild can live inside a sibling cluster that the extractor
    // later replaces with a placeholder. Prefer an ancestor that survives extraction, or a safe
    // sibling leaf for a directly outgoing cluster edge.
    for id in &cluster_ids {
        let Some(non_cluster_child) = anchor.get(id).cloned() else {
            continue;
        };

        if let Some(parent) = graph.parent(&non_cluster_child).map(str::to_string)
            && parent != *id
            && external
                .get(&parent)
                .is_some_and(|has_external| !*has_external)
        {
            anchor.insert(id.clone(), parent);
        }

        work_control.charge(edge_keys.len())?;
        let has_direct_outgoing_edge = edge_keys.iter().any(|edge| edge.v == *id);
        let needs_safe_anchor = external.get(id).copied().unwrap_or(false)
            && has_direct_outgoing_edge
            && state_is_node_in_extractable_cluster(graph, &non_cluster_child, id, &external);
        if needs_safe_anchor
            && let Some(excluded_cluster) = graph.parent(&non_cluster_child).map(str::to_string)
            && let Some(safe_anchor) = state_find_safe_anchor_node(
                graph,
                id,
                &excluded_cluster,
                &descendants,
                &external,
                work_control,
            )?
        {
            anchor.insert(id.clone(), safe_anchor);
        }
    }

    // Adjust edges that touch cluster ids by rewriting them to anchor nodes.
    //
    // Match Mermaid `adjustClustersAndEdges(graph)`: edges incident on cluster nodes are removed
    // and re-inserted even when their endpoints do not change. This affects edge insertion order
    // and can change deterministic tie-breaking in Dagre's acyclic pass.
    for key in edge_keys {
        let mut from_cluster: Option<String> = None;
        let mut to_cluster: Option<String> = None;
        let mut v = key.v.clone();
        let mut w = key.w.clone();

        let touches_cluster =
            cluster_ids.iter().any(|c| c == &v) || cluster_ids.iter().any(|c| c == &w);
        if !touches_cluster {
            continue;
        }

        if cluster_ids.iter().any(|c| c == &v)
            && *external.get(&v).unwrap_or(&false)
            && let Some(a) = anchor.get(&v)
        {
            from_cluster = Some(v.clone());
            v = a.clone();
        }
        if cluster_ids.iter().any(|c| c == &w)
            && *external.get(&w).unwrap_or(&false)
            && let Some(a) = anchor.get(&w)
        {
            to_cluster = Some(w.clone());
            w = a.clone();
        }

        let Some(old_label) = graph.edge_by_key(&key).cloned() else {
            continue;
        };
        let _ = graph.remove_edge_key(&key);

        let mut new_label = old_label;
        if let Some(fc) = from_cluster.as_deref() {
            set_extras_string(&mut new_label.extras, "fromCluster", fc);
        }
        if let Some(tc) = to_cluster.as_deref() {
            set_extras_string(&mut new_label.extras, "toCluster", tc);
        }
        graph.set_edge_named(v, w, key.name.clone(), Some(new_label));
    }

    // Extract clusters without external connections into subgraphs for nested layout. Mermaid
    // 11.17.2 does not let an explicit direction override the external-connection guard.
    //
    // Mermaid@11.12.2 `dagre-wrapper` extractor does not require clusters to be root-level. It
    // extracts any cluster node that has children and no external connections, then relies on the
    // nested render pass to (optionally) inject the cluster root node back into the subgraph
    // for sizing/padding.
    let mut candidate_roots: Vec<String> = Vec::new();
    for id in graph.node_ids() {
        if graph.children(&id).is_empty() {
            continue;
        }
        if *external.get(&id).unwrap_or(&false) {
            continue;
        }
        candidate_roots.push(id);
    }
    fn cluster_depth(g: &Graph<NodeLabel, EdgeLabel, GraphLabel>, id: &str) -> usize {
        let mut depth = 0usize;
        let mut cur = id;
        while let Some(parent) = g.parent(cur) {
            depth += 1;
            cur = parent;
            if depth > 128 {
                break;
            }
        }
        depth
    }
    candidate_roots.sort_by(|a, b| {
        cluster_depth(graph, a)
            .cmp(&cluster_depth(graph, b))
            .then(a.cmp(b))
    });

    let mut child_ids = Vec::new();
    for cluster_id in candidate_roots {
        if !graph.has_node(&cluster_id) || graph.children(&cluster_id).is_empty() {
            continue;
        }
        let parent_dir = graph.graph().rankdir;
        let requested = cluster_dir(&cluster_id).map(|d| rank_dir_from(&d));
        // Mermaid keeps nested state graphs in the same rank direction by default. Only apply
        // a different direction when explicitly requested by the cluster itself.
        let dir = requested.unwrap_or(parent_dir);
        let nodesep = graph.graph().nodesep;
        let ranksep = graph.graph().ranksep + 25.0;
        let marginx = graph.graph().marginx;
        let marginy = graph.graph().marginy;

        let mut subgraph = extract_cluster_graph(&cluster_id, graph, work_control)?;
        subgraph.graph_mut().rankdir = dir;
        subgraph.graph_mut().nodesep = nodesep;
        subgraph.graph_mut().ranksep = ranksep;
        subgraph.graph_mut().marginx = marginx;
        subgraph.graph_mut().marginy = marginy;

        prepared.extracted.insert(
            cluster_id.clone(),
            PreparedGraph {
                graph: subgraph,
                extracted: BTreeMap::new(),
                root_cluster_id: Some(cluster_id.clone()),
            },
        );
        child_ids.push(cluster_id);
    }

    Ok(child_ids)
}

fn extract_cluster_graph<W: StatePreparationWorkControl>(
    cluster_id: &str,
    graph: &mut Graph<NodeLabel, EdgeLabel, GraphLabel>,
    work_control: &mut W,
) -> Result<Graph<NodeLabel, EdgeLabel, GraphLabel>> {
    if graph.children(cluster_id).is_empty() {
        return Err(Error::InvalidModel {
            message: format!("cluster has no children: {cluster_id}"),
        });
    }

    // Mermaid's cluster extractor uses a somewhat surprising copy algorithm:
    // - It walks leaf nodes in a deterministic-but-mutation-sensitive order.
    // - For each leaf, it calls `graph.edges(node)` (Graphlib ignores the argument and returns
    //   *all* edges), inserting edges opportunistically while the source graph is being mutated.
    //
    // This affects edge insertion order in the extracted graph and can change Dagre's cycle
    // breaking tie-breakers (notably for cyclic-special self-loop expansions). Mirror that
    // behavior for parity.
    let mut descendants: Vec<String> = Vec::new();
    work_control.charge(graph.node_slot_count())?;
    extract_descendants(graph, cluster_id, &mut descendants);
    let descendants_set: HashSet<String> = descendants.iter().cloned().collect();

    let mut sub = Graph::<NodeLabel, EdgeLabel, GraphLabel>::new(GraphOptions {
        directed: true,
        multigraph: true,
        compound: true,
    });

    struct CopyFrame {
        current_cluster_id: String,
        nodes: Vec<String>,
        next_index: usize,
    }

    let root_nodes: Vec<String> = graph
        .children(cluster_id)
        .iter()
        .map(|s| s.to_string())
        .collect();
    let mut stack = vec![CopyFrame {
        current_cluster_id: cluster_id.to_string(),
        nodes: root_nodes,
        next_index: 0,
    }];

    while !stack.is_empty() {
        let frame_idx = stack.len() - 1;
        let Some((node, current_cluster_id)) = ({
            let frame = &mut stack[frame_idx];
            if frame.next_index >= frame.nodes.len() {
                None
            } else {
                let node = frame.nodes[frame.next_index].clone();
                frame.next_index += 1;
                Some((node, frame.current_cluster_id.clone()))
            }
        }) else {
            stack.pop();
            continue;
        };

        if !graph.has_node(&node) {
            continue;
        }

        if !graph.children(&node).is_empty() {
            let mut child_nodes: Vec<String> = graph
                .children(&node)
                .iter()
                .map(|s| s.to_string())
                .collect();
            if node != cluster_id {
                child_nodes.push(node.clone());
            }
            stack.push(CopyFrame {
                current_cluster_id: node,
                nodes: child_nodes,
                next_index: 0,
            });
            continue;
        }

        let data = graph.node(&node).cloned().unwrap_or_default();
        work_control.charge(1)?;
        sub.set_node(node.clone(), data);

        if let Some(parent) = graph.parent(&node)
            && parent != cluster_id
        {
            sub.set_parent(node.clone(), parent.to_string());
        }
        if current_cluster_id != cluster_id && node != current_cluster_id {
            sub.set_parent(node.clone(), current_cluster_id);
        }

        // NOTE: Mermaid uses `graph.edges(node)` but Graphlib ignores the argument and
        // returns all edges. Mirror that by iterating the full edge set each time.
        work_control.charge(graph.edge_slot_count())?;
        let edge_keys = graph.edge_keys();
        for ek in edge_keys {
            if ek.v == cluster_id || ek.w == cluster_id {
                continue;
            }
            let Some(label) = graph.edge_by_key(&ek).cloned() else {
                continue;
            };
            let v_inside = descendants_set.contains(&ek.v);
            let w_inside = descendants_set.contains(&ek.w);
            if !v_inside && !w_inside {
                continue;
            }
            if v_inside && w_inside {
                sub.set_edge_named(ek.v, ek.w, ek.name, Some(label));
                continue;
            }

            // `edgeInCluster` in Mermaid intentionally admits either endpoint. Since 11.16,
            // cross-boundary edges are kept in the outer graph and rebound to the extracted root
            // instead of auto-creating the external endpoint inside the child graph.
            let outer_v = if v_inside {
                cluster_id.to_string()
            } else {
                ek.v
            };
            let outer_w = if w_inside {
                cluster_id.to_string()
            } else {
                ek.w
            };
            graph.set_edge_named(outer_v, outer_w, ek.name, Some(label));
        }

        let _ = graph.remove_node(&node);
    }

    Ok(sub)
}

/// Debug-only helper: extracts a cluster subgraph the same way `prepare_graph(...)` does.
#[doc(hidden)]
pub fn debug_extract_state_diagram_cluster_graph(
    graph: &mut Graph<NodeLabel, EdgeLabel, GraphLabel>,
    cluster_id: &str,
) -> Result<Graph<NodeLabel, EdgeLabel, GraphLabel>> {
    let mut work_control = NoopStatePreparationWorkControl;
    extract_cluster_graph(cluster_id, graph, &mut work_control)
}

fn inject_root_cluster_node(g: &mut Graph<NodeLabel, EdgeLabel, GraphLabel>, root_id: &str) {
    if !g.has_node(root_id) {
        g.set_node(
            root_id.to_string(),
            NodeLabel {
                width: 1.0,
                height: 1.0,
                ..Default::default()
            },
        );
    }

    let node_ids: Vec<String> = g.node_ids().into_iter().map(|s| s.to_string()).collect();
    for v in node_ids {
        if v == root_id {
            continue;
        }
        if g.parent(&v).is_none() {
            g.set_parent(v, root_id.to_string());
        }
    }
}

fn layout_prepared(
    prepared: &mut PreparedGraph,
    work_control: &mut OperationLayoutWorkControl,
) -> Result<(LayoutFragments, Rect)> {
    let mut stack: Vec<(Vec<String>, bool)> = vec![(Vec::new(), false)];
    let mut completed: HashMap<Vec<String>, (LayoutFragments, Rect)> = HashMap::new();

    while let Some((path, visited)) = stack.pop() {
        let child_ids: Vec<String> = {
            let node = prepared_graph_at_path_mut(prepared, &path)?;
            node.extracted.keys().cloned().collect()
        };

        if visited {
            let mut extracted_fragments = HashMap::new();
            for child_id in child_ids {
                let mut child_path = path.clone();
                child_path.push(child_id.clone());
                let Some(result) = completed.remove(&child_path) else {
                    return Err(Error::InvalidModel {
                        message: format!("missing prepared cluster layout: {child_id}"),
                    });
                };
                extracted_fragments.insert(child_id, result);
            }

            let node = prepared_graph_at_path_mut(prepared, &path)?;
            let result = layout_prepared_node(node, extracted_fragments, work_control)?;
            completed.insert(path, result);
            continue;
        }

        stack.push((path.clone(), true));
        for child_id in child_ids.into_iter().rev() {
            let mut child_path = path.clone();
            child_path.push(child_id);
            stack.push((child_path, false));
        }
    }

    completed
        .remove(&Vec::new())
        .ok_or_else(|| Error::InvalidModel {
            message: "missing prepared root layout".to_string(),
        })
}

fn layout_prepared_node(
    prepared: &mut PreparedGraph,
    extracted_fragments: HashMap<String, (LayoutFragments, Rect)>,
    work_control: &mut OperationLayoutWorkControl,
) -> Result<(LayoutFragments, Rect)> {
    if let Some(root_id) = prepared.root_cluster_id.clone() {
        // Mermaid's dagre-wrapper nested render pass injects the parent cluster node into the
        // extracted graph and parents top-level nodes to it. This is required for Dagre’s
        // compound border nodes to yield the same “outer padding” used by upstream when sizing
        // clusterNode placeholders via `updateNodeBounds(...)`.
        inject_root_cluster_node(&mut prepared.graph, &root_id);
    }

    let mut fragments = LayoutFragments {
        nodes: HashMap::new(),
        edge_segments: Vec::new(),
    };

    for (id, (_sub_frag, bounds)) in &extracted_fragments {
        let Some(n) = prepared.graph.node_mut(id) else {
            return Err(Error::InvalidModel {
                message: format!("missing cluster placeholder node: {id}"),
            });
        };
        n.width = bounds.width().max(1.0);
        n.height = bounds.height().max(1.0);
    }

    // State diagrams use Mermaid's unified Dagre renderer, so use Dugong's canonical pipeline
    // here (edge label proxies, BK positioning, etc.).
    dugong::layout_controlled(&mut prepared.graph, work_control)
        .map_err(|error| work_control.map_dugong_error(error))?;

    for id in prepared.graph.node_ids() {
        let Some(n) = prepared.graph.node(&id) else {
            continue;
        };
        fragments.nodes.insert(
            id.clone(),
            LayoutNode {
                id: id.clone(),
                x: n.x.unwrap_or(0.0),
                y: n.y.unwrap_or(0.0),
                width: n.width,
                height: n.height,
                is_cluster: false,
                label_width: None,
                label_height: None,
            },
        );
    }

    for key in prepared.graph.edge_keys() {
        let Some(e) = prepared.graph.edge_by_key(&key) else {
            continue;
        };
        let original_id = get_extras_string(&e.extras, "originalId").unwrap_or_else(|| {
            key.name
                .clone()
                .unwrap_or_else(|| format!("edge:{}:{}", key.v, key.w))
        });
        let logical_self_loop_id = get_extras_string(&e.extras, "selfLoopId");
        let segment = e
            .extras
            .get("segment")
            .and_then(|v| v.as_i64())
            .unwrap_or(0) as i32;
        let original_from =
            get_extras_string(&e.extras, "originalFrom").unwrap_or_else(|| key.v.clone());
        let original_to =
            get_extras_string(&e.extras, "originalTo").unwrap_or_else(|| key.w.clone());
        let from_cluster = get_extras_string(&e.extras, "fromCluster");
        let to_cluster = get_extras_string(&e.extras, "toCluster");

        // Mermaid's dagre wrapper emits "edgeLabel" placeholder groups even when the visible
        // label is empty. Dagre still assigns an `(x, y)` label position for those edges, and the
        // placeholders can affect the root `svg.getBBox()` (and therefore `viewBox/max-width`).
        //
        // Preserve the label center even when `width/height` are 0 so downstream renderers can
        // place the placeholders like upstream.
        let label = match (e.x, e.y) {
            (Some(x), Some(y)) => Some(LayoutLabel {
                x,
                y,
                width: e.width.max(0.0),
                height: e.height.max(0.0),
            }),
            _ => None,
        };

        let points = e
            .points
            .iter()
            .map(|p| LayoutPoint { x: p.x, y: p.y })
            .collect::<Vec<_>>();

        fragments.edge_segments.push(EdgeSegment {
            original_id,
            logical_self_loop_id,
            segment,
            original_from,
            original_to,
            from_cluster,
            to_cluster,
            points,
            label,
        });
    }

    // Merge extracted fragments into this graph, translating them by the cluster placeholder
    // position.
    for (cluster_id, (mut sub_frag, sub_bounds)) in extracted_fragments {
        let Some(cluster_node) = fragments.nodes.get(&cluster_id).cloned() else {
            return Err(Error::InvalidModel {
                message: format!("missing cluster placeholder layout: {cluster_id}"),
            });
        };
        let (sub_cx, sub_cy) = sub_bounds.center();
        let dx = cluster_node.x - sub_cx;
        let dy = cluster_node.y - sub_cy;

        for n in sub_frag.nodes.values_mut() {
            n.x += dx;
            n.y += dy;
        }
        for seg in &mut sub_frag.edge_segments {
            for p in &mut seg.points {
                p.x += dx;
                p.y += dy;
            }
            if let Some(l) = seg.label.as_mut() {
                l.x += dx;
                l.y += dy;
            }
        }

        fragments.nodes.extend(sub_frag.nodes);
        fragments.edge_segments.extend(sub_frag.edge_segments);
    }

    let mut points: Vec<(f64, f64)> = Vec::new();
    for n in fragments.nodes.values() {
        let r = Rect::from_center(n.x, n.y, n.width, n.height);
        points.push((r.min_x(), r.min_y()));
        points.push((r.max_x(), r.max_y()));
    }
    for e in &fragments.edge_segments {
        for p in &e.points {
            points.push((p.x, p.y));
        }
        if let Some(l) = &e.label {
            let r = Rect::from_center(l.x, l.y, l.width, l.height);
            points.push((r.min_x(), r.min_y()));
            points.push((r.max_x(), r.max_y()));
        }
    }
    let bounds = Bounds::from_points(points)
        .map(|b| Rect::from_min_max(b.min_x, b.min_y, b.max_x, b.max_y))
        .unwrap_or_else(|| Rect::from_min_max(0.0, 0.0, 0.0, 0.0));

    Ok((fragments, bounds))
}

fn merge_edge_segments(mut segments: Vec<EdgeSegment>) -> Vec<LayoutEdge> {
    segments.sort_by(|a, b| {
        a.original_id
            .cmp(&b.original_id)
            .then_with(|| a.segment.cmp(&b.segment))
    });

    let mut out: Vec<LayoutEdge> = Vec::new();
    let mut i = 0usize;
    while i < segments.len() {
        let id = segments[i].original_id.clone();
        let from = segments[i].original_from.clone();
        let to = segments[i].original_to.clone();

        let mut from_cluster = segments[i].from_cluster.clone();
        let mut to_cluster = segments[i].to_cluster.clone();

        let mut points: Vec<LayoutPoint> = Vec::new();
        let mut label: Option<LayoutLabel> = None;

        while i < segments.len() && segments[i].original_id == id {
            let seg = &segments[i];
            if from_cluster.is_none() {
                from_cluster = seg.from_cluster.clone();
            }
            if to_cluster.is_none() {
                to_cluster = seg.to_cluster.clone();
            }
            if label.is_none() {
                label = seg.label.clone();
            }

            for (idx, p) in seg.points.iter().enumerate() {
                if points.is_empty() {
                    points.push(p.clone());
                    continue;
                }
                if idx == 0
                    && points.last().is_some_and(|last| {
                        (last.x - p.x).abs() < 1e-9 && (last.y - p.y).abs() < 1e-9
                    })
                {
                    continue;
                }
                points.push(p.clone());
            }

            i += 1;
        }

        out.push(LayoutEdge {
            id,
            from,
            to,
            from_cluster,
            to_cluster,
            points,
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

    out
}

fn merge_self_loop_segment_fallback(id: String, mut segments: Vec<EdgeSegment>) -> LayoutEdge {
    segments.sort_by_key(|segment| segment.segment);
    let first = &segments[0];
    let from = first.original_from.clone();
    let to = first.original_to.clone();
    let from_cluster = segments
        .iter()
        .find_map(|segment| segment.from_cluster.clone());
    let to_cluster = segments
        .iter()
        .find_map(|segment| segment.to_cluster.clone());
    let label = segments
        .iter()
        .find(|segment| segment.segment == 1)
        .and_then(|segment| segment.label.clone())
        .or_else(|| segments.iter().find_map(|segment| segment.label.clone()));

    let mut points = Vec::new();
    for segment in &segments {
        for point in &segment.points {
            if points.last().is_some_and(|last: &LayoutPoint| {
                (last.x - point.x).abs() < 1e-9 && (last.y - point.y).abs() < 1e-9
            }) {
                continue;
            }
            points.push(point.clone());
        }
    }

    LayoutEdge {
        id,
        from,
        to,
        from_cluster,
        to_cluster,
        points,
        label,
        start_label_left: None,
        start_label_right: None,
        end_label_left: None,
        end_label_right: None,
        start_marker: None,
        end_marker: None,
        stroke_dasharray: None,
    }
}

fn compact_self_loop_edges(
    segments: Vec<EdgeSegment>,
    nodes: &[LayoutNode],
    clusters: &[LayoutCluster],
    rankdir: RankDir,
) -> Vec<LayoutEdge> {
    let mut groups: BTreeMap<String, Vec<EdgeSegment>> = BTreeMap::new();
    let mut passthrough = Vec::new();
    for segment in segments {
        let Some(id) = segment.logical_self_loop_id.clone() else {
            passthrough.push(segment);
            continue;
        };
        groups.entry(id).or_default().push(segment);
    }

    let mut edges = Vec::with_capacity(groups.len());
    for (id, mut segments) in groups {
        segments.sort_by_key(|segment| segment.segment);
        let is_complete_group =
            segments.len() == 3 && segments.iter().map(|segment| segment.segment).eq([0, 1, 2]);
        if !is_complete_group {
            edges.push(merge_self_loop_segment_fallback(id, segments));
            continue;
        }

        let first = &segments[0];
        let from = first.original_from.clone();
        let to = first.original_to.clone();
        if from != to
            || segments
                .iter()
                .any(|segment| segment.original_from != from || segment.original_to != to)
        {
            edges.push(merge_self_loop_segment_fallback(id, segments));
            continue;
        }
        let Some((node_x, node_y, node_width, node_height)) = nodes
            .iter()
            .find(|node| node.id == from)
            .map(|node| (node.x, node.y, node.width, node.height))
            .or_else(|| {
                clusters
                    .iter()
                    .find(|cluster| cluster.id == from)
                    .map(|cluster| (cluster.x, cluster.y, cluster.width, cluster.height))
            })
        else {
            edges.push(merge_self_loop_segment_fallback(id, segments));
            continue;
        };

        let helper_ids = [
            format!("{from}---{from}---1"),
            format!("{from}---{from}---2"),
        ];
        let mut hints: Vec<LayoutPoint> = helper_ids
            .iter()
            .filter_map(|id| {
                nodes
                    .iter()
                    .find(|node| node.id == *id)
                    .map(|node| LayoutPoint {
                        x: node.x,
                        y: node.y,
                    })
            })
            .collect();
        if hints.is_empty() {
            hints.extend(
                segments
                    .iter()
                    .flat_map(|segment| segment.points.iter().cloned()),
            );
        }

        let mut label = segments
            .iter()
            .find(|segment| segment.segment == 1)
            .and_then(|segment| segment.label.clone())
            .or_else(|| segments.iter().find_map(|segment| segment.label.clone()));
        let label_width = label.as_ref().map_or(0.0, |label| label.width);
        let label_height = label.as_ref().map_or(0.0, |label| label.height);
        let geometry = compact_self_loop_geometry(
            &LayoutPoint {
                x: node_x,
                y: node_y,
            },
            Size::new(node_width, node_height),
            rankdir,
            &hints,
            0.0,
            Size::new(label_width, label_height),
        );

        if let Some(label) = label.as_mut() {
            label.x = geometry.label_center.x;
            label.y = geometry.label_center.y;
        }

        edges.push(LayoutEdge {
            id,
            from,
            to,
            from_cluster: segments
                .iter()
                .find_map(|segment| segment.from_cluster.clone()),
            to_cluster: segments
                .iter()
                .find_map(|segment| segment.to_cluster.clone()),
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

    edges.extend(merge_edge_segments(passthrough));
    edges
}

fn state_layout_adapter_work(
    model: &StateDiagramModel,
    work_control: &OperationLayoutWorkControl,
) -> Result<usize> {
    let node_work = work_control.checked_mul(model.nodes.len(), 12)?;
    let edge_work = work_control.checked_mul(model.edges.len(), 8)?;
    let state_work = work_control.checked_mul(model.states.len(), 4)?;
    let relation_work = work_control.checked_mul(model.relations.len(), 2)?;
    let link_work = work_control.checked_mul(model.links.len(), 2)?;
    let style_work = work_control.checked_mul(model.style_classes.len(), 2)?;
    let hidden_prefix_bytes = model.states.iter().try_fold(0usize, |work, (id, state)| {
        if state
            .note
            .as_ref()
            .is_some_and(|note| !note.text.trim().is_empty() && note.position.is_none())
        {
            work_control.checked_add(work, id.len())
        } else {
            Ok(work)
        }
    })?;
    let hidden_candidate_bytes = model.nodes.iter().try_fold(0usize, |work, node| {
        work_control.checked_add(
            work,
            node.id
                .len()
                .checked_add(node.parent_id.as_deref().map_or(0, str::len))
                .ok_or_else(|| work_control.record_arithmetic_overflow())?,
        )
    })?;
    let hidden_edge_bytes = model.edges.iter().try_fold(0usize, |work, edge| {
        let edge_bytes = edge
            .id
            .len()
            .checked_add(edge.start.len())
            .and_then(|units| units.checked_add(edge.end.len()))
            .ok_or_else(|| work_control.record_arithmetic_overflow())?;
        work_control.checked_add(work, edge_bytes)
    })?;
    let hidden_filter_work = work_control.checked_mul(
        work_control.checked_add(
            hidden_prefix_bytes,
            work_control.checked_add(hidden_candidate_bytes, hidden_edge_bytes)?,
        )?,
        4,
    )?;
    work_control.checked_add(
        work_control.checked_add(node_work, edge_work)?,
        work_control.checked_add(
            work_control.checked_add(state_work, relation_work)?,
            work_control.checked_add(
                work_control.checked_add(link_work, style_work)?,
                hidden_filter_work,
            )?,
        )?,
    )
}

pub(crate) fn layout_state_diagram_typed_with_work_meter(
    model: &StateDiagramModel,
    effective_config: &Value,
    execution: &crate::LayoutExecution<'_>,
) -> Result<StateDiagramLayout> {
    let mut work_control = OperationLayoutWorkControl::new(execution.work_meter());
    let adapter_work = state_layout_adapter_work(model, &work_control)?;
    work_control.charge_adapter(adapter_work)?;
    crate::layout_backend::resolve_graph_layout(effective_config).validate_rootless_graph()?;
    #[cfg(feature = "layout-elk")]
    if crate::layout_backend::resolve_graph_layout(effective_config).backend
        == crate::layout_backend::GraphLayoutBackend::Elk
    {
        return super::elk::layout(
            model,
            effective_config,
            execution.text_measurer(),
            execution.elk_operation_seed(),
            &mut work_control,
        );
    }
    layout_state_diagram_inner(
        model,
        effective_config,
        execution.text_measurer(),
        &mut work_control,
    )
}

pub(super) fn state_hidden_prefixes(model: &StateDiagramModel) -> HiddenPrefixMatcher {
    let mut hidden_prefixes: Vec<String> = Vec::new();
    for (id, st) in &model.states {
        let Some(note) = st.note.as_ref() else {
            continue;
        };
        if note.text.trim().is_empty() {
            continue;
        }
        if note.position.is_none() {
            hidden_prefixes.push(id.clone());
        }
    }
    HiddenPrefixMatcher::from_prefixes(hidden_prefixes)
}

fn dagre_id_for_node(n: &StateNode) -> String {
    if n.dom_id.trim().is_empty() {
        n.id.clone()
    } else {
        n.dom_id.clone()
    }
}

pub(super) fn note_group_owner_id(id: &str) -> Option<&str> {
    let (owner, _) = id.rsplit_once("----parent")?;
    (!owner.is_empty()).then_some(owner)
}

pub(super) fn state_fork_join_painted_dimensions(rankdir: RankDir) -> (f64, f64) {
    if matches!(rankdir, RankDir::LR | RankDir::RL) {
        (10.0, 70.0)
    } else {
        (70.0, 10.0)
    }
}

pub(super) fn state_node_dimensions(
    n: &StateNode,
    settings: &StateLayoutSettings,
    measurer: &dyn TextMeasurer,
) -> Result<(f64, f64)> {
    let padding = n.padding.unwrap_or(settings.state_padding).max(0.0);
    let label_text = n
        .label
        .as_ref()
        .map(value_to_label_text)
        .unwrap_or_else(|| n.id.clone());

    let (w, h) = match n.shape.as_str() {
        "stateStart" => (14.0, 14.0),
        "stateEnd" => (14.0, 14.0),
        "choice" => (28.0, 28.0),
        "fork" | "join" => {
            let (mut width, mut height) =
                state_fork_join_painted_dimensions(settings.graph.rankdir);
            width += settings.state_padding / 2.0;
            height += settings.state_padding / 2.0;
            (width, height)
        }
        "note" => {
            let (tw, th) = node_label_metrics(
                &label_text,
                settings.wrapping_width,
                &n.css_compiled_styles,
                &n.css_styles,
                measurer,
                &settings.text_style,
                settings.wrap_mode,
            );
            let tw = crate::text::TextMetrics {
                width: tw,
                height: th,
                line_count: 1,
            }
            .with_label_min_width(&label_text, settings.label_min_width, None)
            .width;
            (tw + padding * 2.0, th + padding * 2.0)
        }
        "rectWithTitle" => {
            let desc = n
                .description
                .as_ref()
                .map(|v| v.join("\n"))
                .unwrap_or_default();
            let (title_w, title_h) = title_label_metrics(
                &label_text,
                measurer,
                &settings.text_style,
                WrapMode::HtmlLike,
            );
            let (desc_w, desc_h) =
                title_label_metrics(&desc, measurer, &settings.text_style, WrapMode::HtmlLike);

            let geometry = super::RectWithTitleGeometry::from_metrics(
                title_w, title_h, desc_w, desc_h, padding,
            );
            (geometry.width, geometry.height)
        }
        "rect" => {
            let (tw, th) = node_label_metrics(
                &label_text,
                settings.wrapping_width,
                &n.css_compiled_styles,
                &n.css_styles,
                measurer,
                &settings.text_style,
                settings.wrap_mode,
            );
            let tw = crate::text::TextMetrics {
                width: tw,
                height: th,
                line_count: 1,
            }
            .with_label_min_width(&label_text, settings.label_min_width, None)
            .width;
            // Mermaid converts `rect` into `roundedRect` when rx/ry is set.
            let has_rounding = n.rx.unwrap_or(0.0) > 0.0 && n.ry.unwrap_or(0.0) > 0.0;
            let pad_x = if has_rounding { padding } else { padding * 2.0 };
            let pad_y = padding;
            (tw + pad_x * 2.0, th + pad_y * 2.0)
        }
        other => {
            return Err(Error::InvalidModel {
                message: format!("unsupported state node shape: {other}"),
            });
        }
    };
    Ok((w.max(1.0), h.max(1.0)))
}

fn build_state_diagram_dagre_input(
    model: &StateDiagramModel,
    effective_config: &Value,
    measurer: &dyn TextMeasurer,
) -> Result<StateDagreInput> {
    // Mermaid accepts some historical "floating note" syntaxes in the parser but does not render them.
    // Keep them in the semantic model/snapshots, but exclude them from layout so they do not shift
    // visible nodes/edges (and therefore do not affect root viewBox/max-width parity).
    let hidden_prefixes = state_hidden_prefixes(model);

    // The semantic model keeps one generated note group per note so side-specific note metadata
    // remains observable. Mermaid's layout graph, however, uses one physical parent cluster per
    // state and lets the last note declaration replace that cluster's metadata. Keep both layers
    // explicit: map all generated groups for an owner to the last declaration's layout identity.
    let mut last_note_group_by_owner: HashMap<String, String> = HashMap::new();
    for n in &model.nodes {
        if n.shape == "noteGroup"
            && let Some(owner) = note_group_owner_id(&n.id)
        {
            last_note_group_by_owner.insert(owner.to_string(), n.id.clone());
        }
    }
    let mut note_group_canonical_by_semantic_id: HashMap<String, String> = HashMap::new();
    for n in &model.nodes {
        if n.shape == "noteGroup"
            && let Some(owner) = note_group_owner_id(&n.id)
            && let Some(canonical) = last_note_group_by_owner.get(owner)
        {
            note_group_canonical_by_semantic_id.insert(n.id.clone(), canonical.clone());
        }
    }

    let mut raw_dagre_id_by_semantic_id: HashMap<String, String> = HashMap::new();
    for n in &model.nodes {
        raw_dagre_id_by_semantic_id.insert(n.id.clone(), dagre_id_for_node(n));
    }
    let mut dagre_id_by_semantic_id: HashMap<String, String> = HashMap::new();
    let mut dir_by_dagre_id: HashMap<String, Option<String>> = HashMap::new();
    for n in &model.nodes {
        let layout_semantic_id = note_group_canonical_by_semantic_id
            .get(&n.id)
            .unwrap_or(&n.id);
        let dagre_id = raw_dagre_id_by_semantic_id
            .get(layout_semantic_id)
            .cloned()
            .unwrap_or_else(|| dagre_id_for_node(n));
        dagre_id_by_semantic_id.insert(n.id.clone(), dagre_id.clone());
        dir_by_dagre_id.insert(dagre_id.clone(), n.dir.as_ref().map(|s| normalize_dir(s)));
    }

    let settings = StateConfigView::new(effective_config).layout_settings(&model.direction);
    let graph_label = settings.graph.clone();
    let diagram_dir = graph_label.rankdir;
    let html_labels = settings.html_labels;
    let wrap_mode = settings.wrap_mode;
    let wrapping_width = settings.wrapping_width;
    let text_style = settings.text_style.clone();

    let mut graph = Graph::<NodeLabel, EdgeLabel, GraphLabel>::new(GraphOptions {
        directed: true,
        multigraph: true,
        compound: true,
    });
    // Mermaid 11.16's Dagre adapter leaves `ranker` unset, so Dagre uses `network-simplex`.
    graph.set_graph(graph_label);

    // Pre-size nodes (leaf nodes only). Cluster nodes start with a tiny placeholder size.
    // Mermaid's renderer interleaves `setNode` and `setParent` for each node. Graphlib inserts a
    // parent that has not been seen yet when `setParent` runs, so preserving this operation order
    // is observable in Dagre's insertion-order tie breaking.
    for n in &model.nodes {
        if hidden_prefixes.is_hidden(n.id.as_str()) {
            continue;
        }
        let dagre_id = dagre_id_by_semantic_id
            .get(&n.id)
            .cloned()
            .unwrap_or_else(|| n.id.clone());
        let node_label = if state_node_is_effective_group(n) {
            NodeLabel {
                width: 1.0,
                height: 1.0,
                ..Default::default()
            }
        } else {
            let (w, h) = state_node_dimensions(n, &settings, measurer)?;

            NodeLabel {
                width: w.max(1.0),
                height: h.max(1.0),
                ..Default::default()
            }
        };

        graph.set_node(dagre_id.clone(), node_label);
        if let Some(parent) = n
            .parent_id
            .as_ref()
            .filter(|parent| !hidden_prefixes.is_hidden(parent))
        {
            let parent_id = dagre_id_by_semantic_id
                .get(parent)
                .cloned()
                .unwrap_or_else(|| parent.clone());
            graph.set_parent(dagre_id, parent_id);
        }
    }

    // Add edges. For self-loops, split into 3 edges with 2 tiny dummy nodes (Mermaid wrapper
    // behavior).
    for e in &model.edges {
        if hidden_prefixes.is_hidden(e.id.as_str())
            || hidden_prefixes.is_hidden(e.start.as_str())
            || hidden_prefixes.is_hidden(e.end.as_str())
        {
            continue;
        }
        let (lw, lh) = edge_label_metrics(&e.label, measurer, &text_style, wrap_mode);
        let mut base = EdgeLabel {
            width: lw,
            height: lh,
            labelpos: LabelPos::C,
            labeloffset: 10.0,
            minlen: 1,
            weight: 1.0,
            ..Default::default()
        };
        set_extras_string(&mut base.extras, "originalId", &e.id);
        set_extras_string(&mut base.extras, "originalFrom", &e.start);
        set_extras_string(&mut base.extras, "originalTo", &e.end);
        set_extras_i32(&mut base.extras, "segment", 0);

        if e.start != e.end {
            let start_id = dagre_id_by_semantic_id
                .get(&e.start)
                .cloned()
                .unwrap_or_else(|| e.start.clone());
            let end_id = dagre_id_by_semantic_id
                .get(&e.end)
                .cloned()
                .unwrap_or_else(|| e.end.clone());
            graph.set_edge_named(start_id, end_id, Some(e.id.clone()), Some(base));
            continue;
        }

        let node_id = e.start.clone();
        let node_dagre_id = dagre_id_by_semantic_id
            .get(&node_id)
            .cloned()
            .unwrap_or_else(|| node_id.clone());
        let id1 = format!("{node_id}-cyclic-special-1");
        let idm = format!("{node_id}-cyclic-special-mid");
        let id2 = format!("{node_id}-cyclic-special-2");
        // Mermaid uses fixed self-loop helper node ids (`${nodeId}---${nodeId}---{1|2}`), not
        // per-edge ids. This means multiple self-loop transitions on the same node collide in the
        // layout graph; match upstream behavior for parity.
        let special1 = format!("{node_id}---{node_id}---1");
        let special2 = format!("{node_id}---{node_id}---2");

        graph.set_node(
            special1.clone(),
            NodeLabel {
                // Mermaid's renderer initially seeds these dummy nodes with `10x10`, but then
                // `labelRect` renders them as `0.1x0.1` and `updateNodeBounds(...)` overwrites
                // `node.width/height` *before* Dagre layout runs.
                //
                // Mirror the effective size seen by Dagre to keep cyclic self-loop layouts and
                // root viewBox parity stable.
                width: 0.1,
                height: 0.1,
                ..Default::default()
            },
        );
        graph.set_node(
            special2.clone(),
            NodeLabel {
                width: 0.1,
                height: 0.1,
                ..Default::default()
            },
        );
        if let Some(parent) = graph.parent(&node_dagre_id).map(|s| s.to_string()) {
            graph.set_parent(special1.clone(), parent.clone());
            graph.set_parent(special2.clone(), parent);
        }

        let mut edge1 = base.clone();
        edge1.width = 0.0;
        edge1.height = 0.0;
        set_extras_i32(&mut edge1.extras, "segment", 0);
        set_extras_string(&mut edge1.extras, "originalId", &id1);
        set_extras_string(&mut edge1.extras, "selfLoopId", &e.id);

        let mut edge_mid = base.clone();
        set_extras_i32(&mut edge_mid.extras, "segment", 1);
        set_extras_string(&mut edge_mid.extras, "originalId", &idm);
        set_extras_string(&mut edge_mid.extras, "selfLoopId", &e.id);

        let mut edge2 = base.clone();
        edge2.width = 0.0;
        edge2.height = 0.0;
        set_extras_i32(&mut edge2.extras, "segment", 2);
        set_extras_string(&mut edge2.extras, "originalId", &id2);
        set_extras_string(&mut edge2.extras, "selfLoopId", &e.id);

        // Mermaid uses different edge *names* (graphlib multigraph keys) from the edge `.id`
        // property for cyclic-special helper edges. This impacts edge iteration order and can
        // affect Dagre's cycle-breaking tie-breakers. Mermaid 11.16 corrected the third helper
        // edge name to use the same `cyclic-special` spelling as the first two segments.
        let name1 = format!("{node_id}-cyclic-special-0");
        let name_mid = format!("{node_id}-cyclic-special-1");
        let name2 = format!("{node_id}-cyclic-special-2");

        graph.set_edge_named(
            node_dagre_id.clone(),
            special1.clone(),
            Some(name1),
            Some(edge1),
        );
        graph.set_edge_named(special1, special2.clone(), Some(name_mid), Some(edge_mid));
        graph.set_edge_named(special2, node_dagre_id, Some(name2), Some(edge2));
    }

    Ok(StateDagreInput {
        graph,
        rankdir: diagram_dir,
        hidden_prefixes,
        dagre_id_by_semantic_id,
        note_group_canonical_by_semantic_id,
        dir_by_dagre_id,
        text_style,
        wrap_mode,
        wrapping_width,
        html_labels,
    })
}

fn layout_state_diagram_inner(
    model: &StateDiagramModel,
    effective_config: &Value,
    measurer: &dyn TextMeasurer,
    work_control: &mut OperationLayoutWorkControl,
) -> Result<StateDiagramLayout> {
    validate_state_parent_cycles(model)?;
    let StateDagreInput {
        graph,
        rankdir,
        hidden_prefixes,
        dagre_id_by_semantic_id,
        note_group_canonical_by_semantic_id,
        dir_by_dagre_id,
        text_style,
        wrap_mode,
        wrapping_width,
        html_labels,
    } = build_state_diagram_dagre_input(model, effective_config, measurer)?;

    let cluster_dir =
        |id: &str| -> Option<String> { dir_by_dagre_id.get(id).and_then(|v| v.clone()) };
    let mut prepared = prepare_graph(graph, &cluster_dir, None, work_control)?;
    let (fragments, _layout_bounds) = layout_prepared(&mut prepared, work_control)?;

    let semantic_ids: HashSet<&str> = model
        .nodes
        .iter()
        .filter(|n| !hidden_prefixes.is_hidden(n.id.as_str()))
        .map(|n| n.id.as_str())
        .collect();

    // Build output nodes from semantic nodes only.
    let mut out_nodes: Vec<LayoutNode> = Vec::new();
    for n in &model.nodes {
        if hidden_prefixes.is_hidden(n.id.as_str()) {
            continue;
        }
        let dagre_id = dagre_id_by_semantic_id
            .get(&n.id)
            .map(|s| s.as_str())
            .unwrap_or(n.id.as_str());
        let Some(pos) = fragments.nodes.get(dagre_id) else {
            return Err(Error::InvalidModel {
                message: format!("missing positioned node: {}", n.id),
            });
        };

        if !state_node_is_effective_group(n) {
            out_nodes.push(LayoutNode {
                id: n.id.clone(),
                x: pos.x,
                y: pos.y,
                width: pos.width,
                height: pos.height,
                is_cluster: false,
                label_width: None,
                label_height: None,
            });
        }
    }

    // Preserve Mermaid's hidden self-loop helper nodes (`${nodeId}---${nodeId}---{1|2}`).
    //
    // These nodes are not part of the semantic model and are not rendered as visible nodes, but
    // Mermaid's SVG output uses their positioned bounding boxes to place `0.1 x 0.1` placeholder
    // rects which can affect `svg.getBBox()` and therefore the root `viewBox/max-width`.
    let mut helper_ids: HashSet<String> = HashSet::new();
    for e in &model.edges {
        if hidden_prefixes.is_hidden(e.id.as_str())
            || hidden_prefixes.is_hidden(e.start.as_str())
            || hidden_prefixes.is_hidden(e.end.as_str())
        {
            continue;
        }
        if e.start != e.end {
            continue;
        }
        let node_id = e.start.as_str();
        helper_ids.insert(format!("{node_id}---{node_id}---1"));
        helper_ids.insert(format!("{node_id}---{node_id}---2"));
    }
    for id in helper_ids {
        let Some(pos) = fragments.nodes.get(&id) else {
            continue;
        };
        out_nodes.push(LayoutNode {
            id,
            x: pos.x,
            y: pos.y,
            width: pos.width,
            height: pos.height,
            is_cluster: false,
            label_width: None,
            label_height: None,
        });
    }

    let mut clusters: Vec<LayoutCluster> = Vec::new();
    for n in &model.nodes {
        if hidden_prefixes.is_hidden(n.id.as_str()) {
            continue;
        }
        if !state_node_is_effective_group(n) {
            continue;
        }
        if note_group_canonical_by_semantic_id
            .get(&n.id)
            .is_some_and(|canonical| canonical != &n.id)
        {
            continue;
        }
        let dagre_id = dagre_id_by_semantic_id
            .get(&n.id)
            .map(|s| s.as_str())
            .unwrap_or(n.id.as_str());
        let Some(pos) = fragments.nodes.get(dagre_id) else {
            return Err(Error::InvalidModel {
                message: format!("missing positioned cluster node: {}", n.id),
            });
        };

        let mut title = n
            .label
            .as_ref()
            .map(value_to_label_text)
            .unwrap_or_default();
        if title.trim().is_empty() {
            title = n.id.clone();
        }
        let pad = n.padding.unwrap_or(8.0).max(0.0);
        let (tw, th) = if title.trim().is_empty() {
            (0.0, 0.0)
        } else if n.shape == "noteGroup" {
            let measurement = super::measure_state_markdown_label(
                &title,
                measurer,
                &text_style,
                Some(wrapping_width),
                wrap_mode,
            );
            (measurement.metrics.width, measurement.metrics.height)
        } else {
            title_label_metrics(&title, measurer, &text_style, wrap_mode)
        };

        // Mermaid expands cluster width to ensure the title fits, but does not re-run Dagre after
        // that adjustment (so child node positions remain unchanged).
        let min_cluster_width = if title.trim().is_empty() {
            0.0
        } else {
            (tw + pad).max(0.0)
        };
        let rect = Rect::from_center(pos.x, pos.y, pos.width.max(min_cluster_width), pos.height);
        let (cx, cy) = rect.center();

        let title_top_adjust = if html_labels { 0.0 } else { 3.0 };
        let title_label = LayoutLabel {
            x: cx,
            y: rect.min_y() + 1.0 - title_top_adjust + th / 2.0,
            width: tw,
            height: th,
        };

        let diff = match n.shape.as_str() {
            "divider" => -pad,
            "noteGroup" => 0.0,
            _ => {
                let padded_label_width = tw + pad;
                if rect.width() <= padded_label_width {
                    (padded_label_width - rect.width()) / 2.0 - pad
                } else {
                    -pad
                }
            }
        };
        let offset_y = if n.shape == "roundedWithTitle" {
            th - pad / 2.0
        } else {
            0.0
        };

        let requested_dir = n.dir.as_ref().map(|s| normalize_dir(s));
        let effective_dir = requested_dir
            .clone()
            .unwrap_or_else(|| normalize_dir(&model.direction));

        clusters.push(LayoutCluster {
            id: n.id.clone(),
            x: cx,
            y: cy,
            width: rect.width(),
            height: rect.height(),
            diff,
            offset_y,
            title,
            title_label,
            requested_dir,
            effective_dir,
            padding: pad,
            title_margin_top: 0.0,
            title_margin_bottom: 0.0,
        });

        out_nodes.push(LayoutNode {
            id: n.id.clone(),
            x: cx,
            y: cy,
            width: rect.width(),
            height: rect.height(),
            is_cluster: true,
            label_width: None,
            label_height: None,
        });
    }

    out_nodes.sort_by(|a, b| a.id.cmp(&b.id));
    clusters.sort_by(|a, b| a.id.cmp(&b.id));

    let (self_loop_segments, regular_segments): (Vec<_>, Vec<_>) = fragments
        .edge_segments
        .into_iter()
        .filter(|segment| {
            semantic_ids.contains(segment.original_from.as_str())
                && semantic_ids.contains(segment.original_to.as_str())
        })
        .partition(|segment| segment.logical_self_loop_id.is_some());
    let mut out_edges = merge_edge_segments(regular_segments);
    out_edges.extend(compact_self_loop_edges(
        self_loop_segments,
        &out_nodes,
        &clusters,
        rankdir,
    ));

    // Mermaid 12's drawRect uses a rectangular intersection even when its visible
    // corners are rounded. Polygon intersection no longer adds the historical half pixel.
    {
        use crate::elk_edge_geometry::{Outline, Shape};
        let layout_nodes: HashMap<&str, &LayoutNode> = out_nodes
            .iter()
            .map(|node| (node.id.as_str(), node))
            .collect();
        let semantic_nodes: HashMap<&str, &StateNode> = model
            .nodes
            .iter()
            .map(|node| (node.id.as_str(), node))
            .collect();
        for edge in &mut out_edges {
            if edge.points.len() < 2 || edge.from == edge.to {
                continue;
            }
            let endpoints = [
                (edge.from.as_str(), 0, 1),
                (
                    edge.to.as_str(),
                    edge.points.len() - 1,
                    edge.points.len() - 2,
                ),
            ];
            // Read both targets before modifying endpoints (two-point paths share them).
            let targets = [
                edge.points[endpoints[0].2].clone(),
                edge.points[endpoints[1].2].clone(),
            ];
            for ((id, endpoint, _), target) in endpoints.into_iter().zip(targets) {
                let (Some(&node), Some(&semantic)) = (layout_nodes.get(id), semantic_nodes.get(id))
                else {
                    continue;
                };
                let outline = match semantic.shape.as_str() {
                    "stateStart" | "stateEnd" => Outline::Ellipse,
                    "choice" => Outline::Diamond,
                    _ => Outline::Rect,
                };
                let hit = Shape {
                    node,
                    outline,
                    intersection: None,
                }
                .intersect(&target);
                if hit.x.is_finite() && hit.y.is_finite() {
                    edge.points[endpoint] = hit;
                }
            }
        }
    }
    out_edges.sort_by(|a, b| a.id.cmp(&b.id));

    let bounds = {
        let mut points: Vec<(f64, f64)> = Vec::new();
        for n in &out_nodes {
            let r = Rect::from_center(n.x, n.y, n.width, n.height);
            points.push((r.min_x(), r.min_y()));
            points.push((r.max_x(), r.max_y()));
        }
        for e in &out_edges {
            for p in &e.points {
                points.push((p.x, p.y));
            }
            if let Some(l) = &e.label {
                let r = Rect::from_center(l.x, l.y, l.width, l.height);
                points.push((r.min_x(), r.min_y()));
                points.push((r.max_x(), r.max_y()));
            }
        }
        Bounds::from_points(points)
    };

    Ok(StateDiagramLayout {
        uses_elk_adapter_dom: false,
        elk_edge_paths: HashMap::new(),
        nodes: out_nodes,
        edges: out_edges,
        clusters,
        bounds,
    })
}

pub(super) fn validate_state_parent_cycles(model: &StateDiagramModel) -> Result<()> {
    let parent_by_id: HashMap<&str, &str> = model
        .nodes
        .iter()
        .filter_map(|node| {
            node.parent_id
                .as_deref()
                .map(|parent| (node.id.as_str(), parent))
        })
        .collect();
    for node in &model.nodes {
        let mut current = Some(node.id.as_str());
        let mut seen: HashSet<&str> = HashSet::new();
        while let Some(id) = current {
            if !seen.insert(id) {
                return Err(Error::InvalidModel {
                    message: format!("state parent cycle involving {id}"),
                });
            }
            current = parent_by_id.get(id).copied();
        }
    }
    Ok(())
}

/// Debug-only helper: builds the Dagre input graph for stateDiagram-v2 *before* layout runs.
///
/// This shares the same graph construction path as `layout_state_diagram_inner`.
/// It is used by `xtask` to compare `dugong` against Mermaid's JS Dagre implementation
/// (`dagre-d3-es`) at the layout output layer (nodes/edges/points) rather than at the SVG layer.
#[doc(hidden)]
pub fn debug_build_state_diagram_dagre_graph(
    model: &StateDiagramModel,
    effective_config: &Value,
    measurer: &dyn TextMeasurer,
) -> Result<Graph<NodeLabel, EdgeLabel, GraphLabel>> {
    Ok(build_state_diagram_dagre_input(model, effective_config, measurer)?.graph)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::text::{DeterministicTextMeasurer, TextMetrics};
    use merman_core::{Engine, ParseOptions, RenderSemanticModel};

    struct NonLatticeMeasurer {
        width: f64,
        height: f64,
    }

    impl TextMeasurer for NonLatticeMeasurer {
        fn measure(&self, _text: &str, _style: &TextStyle) -> TextMetrics {
            TextMetrics {
                width: self.width,
                height: self.height,
                line_count: 1,
            }
        }
    }

    #[test]
    fn state_dagre_input_interleaves_child_parent_insertion_like_mermaid() {
        let source = include_str!(
            "../../../../fixtures/state/stress_state_batch5_concurrency_four_regions_long_titles_061.mmd"
        );
        let parsed = Engine::new()
            .parse_diagram_for_render_model_sync(source, ParseOptions::default())
            .expect("parse state fixture")
            .expect("detect state fixture");
        let RenderSemanticModel::State(model) = parsed.model() else {
            panic!("expected State render model");
        };
        let input = build_state_diagram_dagre_input(
            model,
            parsed.metadata().effective_config.as_value(),
            &DeterministicTextMeasurer::default(),
        )
        .expect("build State Dagre input");

        let r1_id = &input.dagre_id_by_semantic_id["r1"];
        let divider2_id = &input.dagre_id_by_semantic_id["divider-id-2"];
        let region_id = input.graph.parent(r1_id).expect("r1 compound parent");

        let model_index = |dagre_id: &str| {
            model
                .nodes
                .iter()
                .position(|node| dagre_id_for_node(node) == dagre_id)
                .unwrap_or_else(|| panic!("missing model node {dagre_id}"))
        };
        assert!(
            model_index(r1_id) < model_index(divider2_id)
                && model_index(divider2_id) < model_index(region_id),
            "the fixture must keep the compound parent later than its first child and divider2"
        );

        let node_ids = input.graph.node_ids();
        let graph_index = |id: &str| {
            node_ids
                .iter()
                .position(|candidate| candidate == id)
                .unwrap_or_else(|| panic!("missing Dagre node {id}"))
        };
        assert_eq!(
            graph_index(region_id),
            graph_index(r1_id) + 1,
            "setParent must implicitly insert the unseen region immediately after r1"
        );
        assert!(
            graph_index(region_id) < graph_index(divider2_id),
            "the implicitly inserted parent must participate in upstream insertion-order ties"
        );
    }

    #[test]
    fn state_html_metrics_preserve_host_measurement_precision() {
        let measurer = NonLatticeMeasurer {
            width: 73.123_456_789,
            height: 17.25,
        };
        let style = TextStyle::default();

        assert_eq!(
            edge_label_metrics("edge", &measurer, &style, WrapMode::HtmlLike),
            (73.123_456_789, 17.25)
        );
        assert_eq!(
            node_label_metrics(
                "node",
                180.0,
                &[],
                &[],
                &measurer,
                &style,
                WrapMode::HtmlLike,
            ),
            (73.123_456_789, 17.25)
        );
        assert_eq!(
            title_label_metrics("title", &measurer, &style, WrapMode::HtmlLike),
            (73.123_456_789, 17.25)
        );
    }

    #[test]
    fn state_label_metrics_keep_html_min_content_and_svg_background_padding() {
        let measurer = NonLatticeMeasurer {
            width: 250.123_456_789,
            height: 17.25,
        };
        let style = TextStyle::default();

        assert_eq!(
            edge_label_metrics("edge", &measurer, &style, WrapMode::HtmlLike),
            (250.123_456_789, 17.25)
        );
        assert_eq!(
            node_label_metrics(
                "node",
                180.0,
                &[],
                &[],
                &measurer,
                &style,
                WrapMode::HtmlLike,
            ),
            (250.123_456_789, 17.25)
        );
        assert_eq!(
            edge_label_metrics("edge", &measurer, &style, WrapMode::SvgLike),
            (254.123_456_789, 21.25)
        );
    }

    fn self_loop_segment(original_id: &str, segment: i32) -> EdgeSegment {
        EdgeSegment {
            original_id: original_id.to_string(),
            logical_self_loop_id: Some("edge1".to_string()),
            segment,
            original_from: "A".to_string(),
            original_to: "A".to_string(),
            from_cluster: None,
            to_cluster: None,
            points: vec![
                LayoutPoint { x: 0.0, y: 0.0 },
                LayoutPoint {
                    x: f64::from(segment + 1),
                    y: f64::from(segment + 1),
                },
            ],
            label: None,
        }
    }

    fn layout_node(id: &str) -> LayoutNode {
        LayoutNode {
            id: id.to_string(),
            x: 0.0,
            y: 0.0,
            width: 40.0,
            height: 30.0,
            is_cluster: false,
            label_width: None,
            label_height: None,
        }
    }

    #[test]
    fn hidden_prefix_matcher_preserves_note_boundary_semantics() {
        let matcher = HiddenPrefixMatcher::from_prefixes([
            "note-root".to_string(),
            "分组".to_string(),
            "note-root".to_string(),
        ]);

        assert!(matcher.is_hidden("note-root"));
        assert!(matcher.is_hidden("note-root----edge"));
        assert!(matcher.is_hidden("分组----节点"));
        assert!(!matcher.is_hidden("note-root---edge"));
        assert!(!matcher.is_hidden("note-root-child"));
        assert!(!matcher.is_hidden("分组节点"));
        assert!(!matcher.is_hidden("other"));
    }

    #[test]
    fn safe_anchor_avoids_extractable_sibling_cluster() {
        let mut graph: Graph<NodeLabel, EdgeLabel, GraphLabel> = Graph::new(GraphOptions {
            multigraph: true,
            compound: true,
            directed: true,
        });
        for id in ["P", "I", "a", "b", "x", "y"] {
            graph.set_node(id.to_string(), NodeLabel::default());
        }
        graph.set_parent("I", "P");
        graph.set_parent("a", "I");
        graph.set_parent("b", "P");
        graph.set_edge_named("b", "x", Some("edge0".to_string()), None);
        graph.set_edge_named("P", "y", Some("edge1".to_string()), None);

        let descendants = HashMap::from([
            (
                "P".to_string(),
                HashSet::from(["I".to_string(), "a".to_string(), "b".to_string()]),
            ),
            ("I".to_string(), HashSet::from(["a".to_string()])),
        ]);
        let external = HashMap::from([("P".to_string(), true), ("I".to_string(), false)]);
        let mut work_control = NoopStatePreparationWorkControl;

        assert!(state_is_node_in_extractable_cluster(
            &graph, "a", "P", &external
        ));
        assert_eq!(
            state_find_safe_anchor_node(
                &graph,
                "P",
                "I",
                &descendants,
                &external,
                &mut work_control,
            )
            .expect("unbounded anchor search"),
            Some("b".to_string())
        );
    }

    #[test]
    fn compact_self_loop_keeps_incomplete_helpers_out_of_public_layout() {
        let edges = compact_self_loop_edges(
            vec![
                self_loop_segment("A-cyclic-special-1", 0),
                self_loop_segment("A-cyclic-special-mid", 1),
            ],
            &[layout_node("A")],
            &[],
            RankDir::TB,
        );

        assert_eq!(edges.len(), 1);
        assert_eq!(edges[0].id, "edge1");
        assert_eq!(edges[0].from, "A");
        assert_eq!(edges[0].to, "A");
        assert!(edges.iter().all(|edge| !edge.id.contains("cyclic-special")));
    }

    #[test]
    fn compact_self_loop_keeps_helpers_private_when_bounds_are_missing() {
        let edges = compact_self_loop_edges(
            vec![
                self_loop_segment("A-cyclic-special-1", 0),
                self_loop_segment("A-cyclic-special-mid", 1),
                self_loop_segment("A-cyclic-special-2", 2),
            ],
            &[],
            &[],
            RankDir::TB,
        );

        assert_eq!(edges.len(), 1);
        assert_eq!(edges[0].id, "edge1");
        assert_eq!(edges[0].from, "A");
        assert_eq!(edges[0].to, "A");
        assert!(edges.iter().all(|edge| !edge.id.contains("cyclic-special")));
    }
}
