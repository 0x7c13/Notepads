#[cfg(feature = "layout-elk")]
use crate::elk_options::layout_options as class_elk_layout_options;
use crate::entities::decode_entities_minimal;
use crate::layout_work::OperationLayoutWorkControl;
use crate::math::MathRenderer;
use crate::model::{
    Bounds, ClassDiagramLayout, ClassNodeLabelPlan, ClassNodeRowMetrics, ClassPreparedHtmlLabel,
    ClassPreparedHtmlNodeLabels, ClassRenderItem, ClassRenderRoot, ClassRenderRootId,
    ClassRenderTree, LayoutCluster, LayoutEdge, LayoutLabel, LayoutNode, LayoutPoint,
};
use crate::text::{
    MERMAID_CREATE_TEXT_DEFAULT_WIDTH_PX, MermaidMarkdownAnalysis, TextMeasurer, TextStyle,
    WrapMode, analyze_mermaid_markdown, measure_mermaid_text_dimensions,
};
use crate::{Error, Result};
use dugong::graphlib::{Graph, GraphOptions};
use dugong::{EdgeLabel, GraphLabel, NodeLabel, RankDir};
use indexmap::IndexMap;
use rustc_hash::FxHashMap;
use serde_json::Value;
use std::collections::{BTreeMap, HashMap, HashSet};
use std::sync::Arc;

pub(crate) mod config;
#[cfg(feature = "layout-elk")]
mod elk_terminals;
mod measured;
use self::config::{ClassConfigView, ClassLayoutSettings};
use self::measured::{MeasuredEdge, MeasuredGraph, MeasuredNode};
#[cfg(feature = "layout-elk")]
use merman_layout_elk as elk;

type ClassDiagramModel = merman_core::models::class_diagram::ClassDiagram;
type ClassNode = merman_core::models::class_diagram::ClassNode;
type ClassNote = merman_core::models::class_diagram::ClassNote;
type ClassLayoutGraph = Graph<NodeLabel, EdgeLabel, GraphLabel>;
type ExtractedClusterGraph = (Box<ClassLayoutGraph>, HashSet<String>);

pub(crate) fn class_node_requires_math(node: &ClassNode) -> bool {
    [
        node.label.as_str(),
        node.text.as_str(),
        node.type_param.as_str(),
    ]
    .into_iter()
    .chain(node.annotations.iter().map(String::as_str))
    .chain(
        node.members
            .iter()
            .chain(node.methods.iter())
            .map(|member| member.display_text.as_str()),
    )
    .any(crate::math::contains_delimited_math)
}

pub(crate) fn class_requires_math(model: &ClassDiagramModel) -> bool {
    model.classes.values().any(class_node_requires_math)
        || model.relations.iter().any(|relation| {
            crate::math::contains_delimited_math(&relation.title)
                || relation
                    .relation_title_1
                    .as_deref()
                    .is_some_and(crate::math::contains_delimited_math)
                || relation
                    .relation_title_2
                    .as_deref()
                    .is_some_and(crate::math::contains_delimited_math)
        })
        || model
            .notes
            .iter()
            .map(|note| note.text.as_str())
            .chain(model.interfaces.iter().map(|iface| iface.label.as_str()))
            .chain(
                model
                    .namespaces
                    .values()
                    .flat_map(|namespace| [namespace.label.as_str(), namespace.id.as_str()]),
            )
            .any(crate::math::contains_delimited_math)
}

/// Estimates the source-backed graph preparation work shared by Class Dagre and ELK layouts.
///
/// Both backends share source measurements. The conservative family allowance also covers Dagre
/// namespace extraction, which scans edges while copying nested descendants, in addition to the
/// linear measurement and projection baseline.
pub(crate) fn class_layout_work_units(
    model: &ClassDiagramModel,
    work_control: &OperationLayoutWorkControl,
) -> Result<usize> {
    let complexity = merman_core::resources::ClassComplexity::from_model(model);
    let baseline = work_control.checked_mul(
        work_control.checked_add(complexity.nodes, complexity.edges)?,
        4,
    )?;
    if complexity.namespaces == 0 || complexity.edges == 0 {
        return Ok(baseline);
    }

    let depth = complexity.namespace_depth.max(1);
    let namespace_edge_scans = work_control.checked_mul(
        work_control.checked_mul(complexity.namespaces, complexity.edges)?,
        depth,
    )?;
    let extraction_edge_scans = work_control.checked_mul(complexity.nodes, complexity.edges)?;
    work_control.checked_add(
        work_control.checked_add(baseline, namespace_edge_scans)?,
        extraction_edge_scans,
    )
}

pub(crate) fn class_member_display_text(
    member: &merman_core::models::class_diagram::ClassMember,
) -> String {
    // Mermaid ClassMember.parseMember preserves internal attribute whitespace in `text`;
    // shapeUtil.addText passes that text directly to createText, including spaces around ':'.
    member.display_text.trim().to_string()
}

pub(crate) fn class_member_create_text_input(
    member: &merman_core::models::class_diagram::ClassMember,
) -> String {
    class_member_display_text(member)
        .replace('<', "&lt;")
        .replace('>', "&gt;")
}

struct MeasuredClassDiagram {
    graph: MeasuredGraph,
    class_label_plans_by_id: FxHashMap<String, Arc<ClassNodeLabelPlan>>,
    node_label_metrics_by_id: HashMap<String, (f64, f64)>,
    namespace_label_metrics_by_id: HashMap<String, (f64, f64)>,
}

fn normalize_dir(direction: &str) -> String {
    match direction.trim().to_uppercase().as_str() {
        "TB" | "TD" => "TB".to_string(),
        "BT" => "BT".to_string(),
        "LR" => "LR".to_string(),
        "RL" => "RL".to_string(),
        other => other.to_string(),
    }
}

fn rank_dir_from(direction: &str) -> RankDir {
    match normalize_dir(direction).as_str() {
        "TB" => RankDir::TB,
        "BT" => RankDir::BT,
        "LR" => RankDir::LR,
        "RL" => RankDir::RL,
        _ => RankDir::TB,
    }
}

fn class_dom_decl_order_index(dom_id: &str) -> usize {
    dom_id
        .rsplit_once('-')
        .and_then(|(_, suffix)| suffix.parse::<usize>().ok())
        .unwrap_or(usize::MAX)
}

pub(crate) fn class_namespace_ids_in_decl_order(model: &ClassDiagramModel) -> Vec<&str> {
    let mut namespaces: Vec<_> = model.namespaces.values().collect();
    namespaces.sort_by(|lhs, rhs| {
        class_dom_decl_order_index(&lhs.dom_id)
            .cmp(&class_dom_decl_order_index(&rhs.dom_id))
            .then_with(|| lhs.id.cmp(&rhs.id))
    });
    namespaces.into_iter().map(|ns| ns.id.as_str()).collect()
}

pub(crate) fn class_namespace_label<'a>(model: &'a ClassDiagramModel, id: &'a str) -> &'a str {
    model
        .namespaces
        .get(id)
        .and_then(|ns| {
            let label = ns.label.trim();
            (!label.is_empty()).then_some(label)
        })
        .unwrap_or(id)
}

type Rect = merman_core::geom::Box2;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct PreparedGraphId(usize);

struct PreparedGraph {
    graph: Box<Graph<NodeLabel, EdgeLabel, GraphLabel>>,
    extracted: BTreeMap<String, PreparedGraphId>,
    injected_cluster_root_id: Option<String>,
}

struct PreparedGraphArena {
    graphs: Vec<PreparedGraph>,
    top: PreparedGraphId,
}

fn extract_cluster_copy_order(
    graph: &Graph<NodeLabel, EdgeLabel, GraphLabel>,
    cluster_id: &str,
    root_id: &str,
    out: &mut Vec<String>,
) {
    // Mirrors Mermaid's `copy(...)`: children are copied before the non-root cluster node itself.
    // That order decides which nested cluster is extracted first in later recursive passes.
    let mut stack: Vec<(String, bool)> = vec![(cluster_id.to_string(), false)];
    while let Some((node, expanded)) = stack.pop() {
        if expanded {
            if node != root_id {
                out.push(node);
            }
            continue;
        }

        let children = graph.children(&node);
        if children.is_empty() {
            if node != root_id {
                out.push(node);
            }
            continue;
        }

        stack.push((node, true));
        for child in children.iter().rev() {
            stack.push((child.to_string(), false));
        }
    }
}

struct ClassClusterHierarchy<'a> {
    ids: Vec<&'a str>,
    index_by_id: FxHashMap<&'a str, usize>,
    parent: Vec<Option<usize>>,
    depth: Vec<usize>,
    root: Vec<usize>,
    ancestor_jumps: Vec<Vec<usize>>,
    postorder: Vec<usize>,
}

impl<'a> ClassClusterHierarchy<'a> {
    fn new(graph: &'a Graph<NodeLabel, EdgeLabel, GraphLabel>) -> Self {
        let ids = graph.nodes().collect::<Vec<_>>();
        let index_by_id = ids
            .iter()
            .copied()
            .enumerate()
            .map(|(index, id)| (id, index))
            .collect::<FxHashMap<_, _>>();
        let parent = ids
            .iter()
            .map(|id| {
                graph
                    .parent(id)
                    .and_then(|parent| index_by_id.get(parent).copied())
            })
            .collect::<Vec<_>>();
        let mut depth = vec![0; ids.len()];
        let mut root = vec![0; ids.len()];
        let mut stack = parent
            .iter()
            .enumerate()
            .filter_map(|(index, parent)| parent.is_none().then_some(index))
            .collect::<Vec<_>>();
        for index in stack.iter().copied() {
            root[index] = index;
        }
        let mut traversal_order = Vec::with_capacity(ids.len());
        while let Some(index) = stack.pop() {
            traversal_order.push(index);
            for child in graph
                .children_iter(ids[index])
                .filter_map(|id| index_by_id.get(id).copied())
            {
                depth[child] = depth[index] + 1;
                root[child] = root[index];
                stack.push(child);
            }
        }

        let ancestor_levels = usize::BITS as usize - ids.len().max(1).leading_zeros() as usize;
        let mut ancestor_jumps = Vec::with_capacity(ancestor_levels);
        ancestor_jumps.push(
            parent
                .iter()
                .enumerate()
                .map(|(index, parent)| parent.unwrap_or(index))
                .collect::<Vec<_>>(),
        );
        for level in 1..ancestor_levels {
            let previous = &ancestor_jumps[level - 1];
            ancestor_jumps.push(
                previous
                    .iter()
                    .map(|ancestor| previous[*ancestor])
                    .collect(),
            );
        }

        traversal_order.reverse();
        Self {
            ids,
            index_by_id,
            parent,
            depth,
            root,
            ancestor_jumps,
            postorder: traversal_order,
        }
    }

    fn lowest_common_ancestor(&self, mut lhs: usize, mut rhs: usize) -> Option<usize> {
        if self.root[lhs] != self.root[rhs] {
            return None;
        }
        if self.depth[lhs] < self.depth[rhs] {
            std::mem::swap(&mut lhs, &mut rhs);
        }

        let depth_delta = self.depth[lhs] - self.depth[rhs];
        for level in 0..self.ancestor_jumps.len() {
            if depth_delta & (1 << level) != 0 {
                lhs = self.ancestor_jumps[level][lhs];
            }
        }
        if lhs == rhs {
            return Some(lhs);
        }
        for level in (0..self.ancestor_jumps.len()).rev() {
            let lhs_ancestor = self.ancestor_jumps[level][lhs];
            let rhs_ancestor = self.ancestor_jumps[level][rhs];
            if lhs_ancestor != rhs_ancestor {
                lhs = lhs_ancestor;
                rhs = rhs_ancestor;
            }
        }
        self.parent[lhs]
    }

    fn boundary_crossings(&self, graph: &Graph<NodeLabel, EdgeLabel, GraphLabel>) -> Vec<i64> {
        // For one edge, the clusters whose strict-descendant boundary it crosses are exactly the
        // two ancestor paths from each endpoint's parent up to, but excluding, their LCA. Tree
        // differences mark both paths in O(log N), including Mermaid's rule that an edge incident
        // on the cluster node itself does not make that cluster ineligible.
        let mut crossings = vec![0_i64; self.ids.len()];
        for edge in graph.edges() {
            let Some(&from) = self.index_by_id.get(edge.v.as_str()) else {
                continue;
            };
            let Some(&to) = self.index_by_id.get(edge.w.as_str()) else {
                continue;
            };
            let common = self.lowest_common_ancestor(from, to);
            for endpoint in [from, to] {
                if Some(endpoint) == common {
                    continue;
                }
                if let Some(parent) = self.parent[endpoint] {
                    crossings[parent] += 1;
                    if let Some(common) = common {
                        crossings[common] -= 1;
                    }
                }
            }
        }
        for index in self.postorder.iter().copied() {
            if let Some(parent) = self.parent[index] {
                crossings[parent] += crossings[index];
            }
        }
        crossings
    }
}

fn class_cluster_candidates(graph: &Graph<NodeLabel, EdgeLabel, GraphLabel>) -> Vec<String> {
    let hierarchy = ClassClusterHierarchy::new(graph);
    let boundary_crossings = hierarchy.boundary_crossings(graph);
    hierarchy
        .ids
        .iter()
        .copied()
        .enumerate()
        .filter(|(index, id)| {
            graph.children_iter(id).next().is_some() && boundary_crossings[*index] == 0
        })
        .map(|(_, id)| id.to_string())
        .collect()
}

fn class_nodes_in_hierarchy_order(graph: &Graph<NodeLabel, EdgeLabel, GraphLabel>) -> Vec<&str> {
    // Mermaid's Dagre renderer inserts cluster elements using a hierarchy preorder. Parent
    // clusters must therefore precede their descendants so an opaque parent fill cannot cover
    // the child cluster's frame and label.
    let mut ordered = Vec::with_capacity(graph.node_count());
    let mut stack = graph.children_root().into_iter().rev().collect::<Vec<_>>();
    while let Some(id) = stack.pop() {
        ordered.push(id);
        stack.extend(graph.children(id).into_iter().rev());
    }
    ordered
}

fn prepare_graph(
    graph: Box<Graph<NodeLabel, EdgeLabel, GraphLabel>>,
) -> Result<PreparedGraphArena> {
    // Mermaid's default Class renderer uses the shared Dagre rendering-util path. Its
    // graphlib pre-pass extracts clusters *without* external connections into their own subgraphs,
    // toggles their rankdir (TB <-> LR), and renders them recursively to obtain concrete cluster
    // geometry before laying out the parent graph.
    //
    // Reference: pinned Mermaid `rendering-util/layout-algorithms/dagre`:
    // - eligible cluster: has children, and no edge crosses its descendant boundary
    // - extracted subgraph gets `rankdir = parent.rankdir === 'TB' ? 'LR' : 'TB'`
    // - `copy(...)` walks child clusters first and copies a non-root cluster node after its
    //   children, so child extractions may later be moved under an extracted parent
    // - recursive render copies `nodesep` and sets child `ranksep = parent.ranksep + 25`
    // - margins are fixed at 8

    struct PendingCluster {
        id: String,
        moved_ids: HashSet<String>,
    }

    struct PrepareFrame {
        graph: Box<Graph<NodeLabel, EdgeLabel, GraphLabel>>,
        candidates: Vec<String>,
        next_candidate: usize,
        extracted: BTreeMap<String, PreparedGraphId>,
        injected_cluster_root_id: Option<String>,
        pending: Option<PendingCluster>,
    }

    fn frame_for(
        graph: Box<Graph<NodeLabel, EdgeLabel, GraphLabel>>,
        injected_cluster_root_id: Option<String>,
    ) -> PrepareFrame {
        let candidates = class_cluster_candidates(&graph);
        PrepareFrame {
            graph,
            candidates,
            next_candidate: 0,
            extracted: BTreeMap::new(),
            injected_cluster_root_id,
            pending: None,
        }
    }

    let mut graphs = Vec::new();
    let mut stack = vec![frame_for(graph, None)];
    let mut completed_child = None;
    loop {
        if let Some(child_id) = completed_child.take() {
            let Some(parent) = stack.last_mut() else {
                return Ok(PreparedGraphArena {
                    graphs,
                    top: child_id,
                });
            };
            let Some(pending) = parent.pending.take() else {
                return Err(Error::InvalidModel {
                    message: format!(
                        "prepared Class child graph {} has no parent extraction",
                        child_id.0
                    ),
                });
            };
            for moved_id in pending.moved_ids {
                if let Some(moved_child_id) = parent.extracted.remove(&moved_id) {
                    graphs[child_id.0]
                        .extracted
                        .insert(moved_id, moved_child_id);
                }
            }
            parent.extracted.insert(pending.id, child_id);
        }

        let Some(frame) = stack.last_mut() else {
            return Err(Error::InvalidModel {
                message: "missing Class prepare frame".to_string(),
            });
        };
        let mut child_frame = None;
        while let Some(cluster_id) = frame.candidates.get(frame.next_candidate).cloned() {
            frame.next_candidate += 1;
            if frame.graph.children(&cluster_id).is_empty() {
                continue;
            }

            let parent_dir = frame.graph.graph().rankdir;
            let nodesep = frame.graph.graph().nodesep;
            let ranksep = frame.graph.graph().ranksep;
            let (mut subgraph, moved_ids) = extract_cluster_graph(&cluster_id, &mut frame.graph)?;
            let child_label = subgraph.graph_mut();
            child_label.rankdir = if parent_dir == RankDir::TB {
                RankDir::LR
            } else {
                RankDir::TB
            };
            child_label.nodesep = nodesep;
            child_label.ranksep = ranksep;
            child_label.marginx = 8.0;
            child_label.marginy = 8.0;
            frame.pending = Some(PendingCluster {
                id: cluster_id.clone(),
                moved_ids,
            });
            child_frame = Some(frame_for(subgraph, Some(cluster_id)));
            break;
        }

        if let Some(child_frame) = child_frame {
            stack.push(child_frame);
            continue;
        }

        let frame = stack.pop().expect("checked Class prepare frame");
        let graph_id = PreparedGraphId(graphs.len());
        graphs.push(PreparedGraph {
            graph: frame.graph,
            extracted: frame.extracted,
            injected_cluster_root_id: frame.injected_cluster_root_id,
        });
        completed_child = Some(graph_id);
    }
}

fn extract_cluster_graph(
    cluster_id: &str,
    graph: &mut ClassLayoutGraph,
) -> Result<ExtractedClusterGraph> {
    if graph.children(cluster_id).is_empty() {
        return Err(Error::InvalidModel {
            message: format!("cluster has no children: {cluster_id}"),
        });
    }

    let mut descendants: Vec<String> = Vec::new();
    extract_cluster_copy_order(graph, cluster_id, cluster_id, &mut descendants);

    let moved_set: HashSet<String> = descendants.iter().cloned().collect();

    let mut sub = Box::new(Graph::<NodeLabel, EdgeLabel, GraphLabel>::new(
        GraphOptions {
            directed: true,
            multigraph: true,
            compound: true,
        },
    ));

    // Preserve parent graph settings as a base.
    sub.set_graph(graph.graph().clone());

    for id in &descendants {
        let Some(label) = graph.node(id).cloned() else {
            continue;
        };
        sub.set_node(id.clone(), label);
    }

    for key in graph.edge_keys() {
        if moved_set.contains(&key.v)
            && moved_set.contains(&key.w)
            && let Some(label) = graph.edge_by_key(&key).cloned()
        {
            sub.set_edge_named(key.v.clone(), key.w.clone(), key.name.clone(), Some(label));
        }
    }

    for id in &descendants {
        let Some(parent) = graph.parent(id) else {
            continue;
        };
        if moved_set.contains(parent) {
            sub.set_parent(id.clone(), parent.to_string());
        }
    }

    for id in &descendants {
        let _ = graph.remove_node(id);
    }

    Ok((sub, moved_set))
}

#[derive(Debug, Clone)]
struct EdgeTerminalMetrics {
    start_left: Option<(f64, f64)>,
    start_right: Option<(f64, f64)>,
    end_left: Option<(f64, f64)>,
    end_right: Option<(f64, f64)>,
    start_marker: f64,
    end_marker: f64,
    #[cfg(feature = "layout-elk")]
    start_arrow_type: Option<&'static str>,
    #[cfg(feature = "layout-elk")]
    end_arrow_type: Option<&'static str>,
}

fn edge_terminal_metrics_from_extras(e: &EdgeLabel) -> EdgeTerminalMetrics {
    let get_pair = |key: &str| -> Option<(f64, f64)> {
        let obj = e.extras.get(key)?;
        let w = obj.get("width").and_then(|v| v.as_f64()).unwrap_or(0.0);
        let h = obj.get("height").and_then(|v| v.as_f64()).unwrap_or(0.0);
        if w > 0.0 && h > 0.0 {
            Some((w, h))
        } else {
            None
        }
    };
    let start_marker = e
        .extras
        .get("startMarker")
        .and_then(|v| v.as_f64())
        .unwrap_or(0.0);
    let end_marker = e
        .extras
        .get("endMarker")
        .and_then(|v| v.as_f64())
        .unwrap_or(0.0);
    EdgeTerminalMetrics {
        start_left: get_pair("startLeft"),
        start_right: get_pair("startRight"),
        end_left: get_pair("endLeft"),
        end_right: get_pair("endRight"),
        start_marker,
        end_marker,
        #[cfg(feature = "layout-elk")]
        start_arrow_type: None,
        #[cfg(feature = "layout-elk")]
        end_arrow_type: None,
    }
}

#[derive(Debug, Clone)]
struct LayoutFragments {
    nodes: IndexMap<String, LayoutNode>,
    edges: Vec<(LayoutEdge, Option<EdgeTerminalMetrics>)>,
    render_root_id: ClassRenderRootId,
}

fn round_number(num: f64, precision: i32) -> f64 {
    if !num.is_finite() {
        return 0.0;
    }
    let factor = 10_f64.powi(precision);
    (num * factor).round() / factor
}

fn distance(a: &LayoutPoint, b: Option<&LayoutPoint>) -> f64 {
    let Some(b) = b else {
        return 0.0;
    };
    let dx = a.x - b.x;
    let dy = a.y - b.y;
    (dx * dx + dy * dy).sqrt()
}

fn calculate_point(points: &[LayoutPoint], distance_to_traverse: f64) -> Option<LayoutPoint> {
    if points.is_empty() {
        return None;
    }
    let mut prev: Option<&LayoutPoint> = None;
    let mut remaining = distance_to_traverse.max(0.0);
    for p in points {
        if let Some(prev_p) = prev {
            let vector_distance = distance(p, Some(prev_p));
            if vector_distance == 0.0 {
                return Some(prev_p.clone());
            }
            if vector_distance < remaining {
                remaining -= vector_distance;
            } else {
                let ratio = remaining / vector_distance;
                if ratio <= 0.0 {
                    return Some(prev_p.clone());
                }
                if ratio >= 1.0 {
                    return Some(p.clone());
                }
                return Some(LayoutPoint {
                    x: round_number((1.0 - ratio) * prev_p.x + ratio * p.x, 5),
                    y: round_number((1.0 - ratio) * prev_p.y + ratio * p.y, 5),
                });
            }
        }
        prev = Some(p);
    }
    None
}

#[derive(Debug, Clone, Copy)]
enum TerminalPos {
    StartLeft,
    StartRight,
    EndLeft,
    EndRight,
}

fn calc_terminal_label_position(
    terminal_marker_size: f64,
    position: TerminalPos,
    points: &[LayoutPoint],
) -> Option<(f64, f64)> {
    if points.len() < 2 {
        return None;
    }

    let mut pts = points.to_vec();
    match position {
        TerminalPos::StartLeft | TerminalPos::StartRight => {}
        TerminalPos::EndLeft | TerminalPos::EndRight => pts.reverse(),
    }

    let distance_to_cardinality_point = 25.0 + terminal_marker_size;
    let center = calculate_point(&pts, distance_to_cardinality_point)?;
    let d = 10.0 + terminal_marker_size * 0.5;
    let angle = (pts[0].y - center.y).atan2(pts[0].x - center.x);

    let (x, y) = match position {
        TerminalPos::StartLeft => {
            let a = angle + std::f64::consts::PI;
            (
                a.sin() * d + (pts[0].x + center.x) / 2.0,
                -a.cos() * d + (pts[0].y + center.y) / 2.0,
            )
        }
        TerminalPos::StartRight => (
            angle.sin() * d + (pts[0].x + center.x) / 2.0,
            -angle.cos() * d + (pts[0].y + center.y) / 2.0,
        ),
        TerminalPos::EndLeft => (
            angle.sin() * d + (pts[0].x + center.x) / 2.0,
            -angle.cos() * d + (pts[0].y + center.y) / 2.0,
        ),
        TerminalPos::EndRight => {
            let a = angle - std::f64::consts::PI;
            (
                a.sin() * d + (pts[0].x + center.x) / 2.0,
                -a.cos() * d + (pts[0].y + center.y) / 2.0,
            )
        }
    };
    Some((x, y))
}

pub(crate) fn class_arrow_type_for_relation_end(ty: i32) -> Option<&'static str> {
    match ty {
        0 => Some("aggregation"),
        1 => Some("extension"),
        2 => Some("composition"),
        3 => Some("dependency"),
        4 => Some("lollipop"),
        _ => None,
    }
}

fn intersect_segment_with_rect(
    p0: &LayoutPoint,
    p1: &LayoutPoint,
    rect: Rect,
) -> Option<LayoutPoint> {
    let dx = p1.x - p0.x;
    let dy = p1.y - p0.y;
    if dx == 0.0 && dy == 0.0 {
        return None;
    }

    let mut candidates: Vec<(f64, LayoutPoint)> = Vec::new();
    let eps = 1e-9;
    let min_x = rect.min_x();
    let max_x = rect.max_x();
    let min_y = rect.min_y();
    let max_y = rect.max_y();

    if dx.abs() > eps {
        for x_edge in [min_x, max_x] {
            let t = (x_edge - p0.x) / dx;
            if t < -eps || t > 1.0 + eps {
                continue;
            }
            let y = p0.y + t * dy;
            if y + eps >= min_y && y <= max_y + eps {
                candidates.push((t, LayoutPoint { x: x_edge, y }));
            }
        }
    }

    if dy.abs() > eps {
        for y_edge in [min_y, max_y] {
            let t = (y_edge - p0.y) / dy;
            if t < -eps || t > 1.0 + eps {
                continue;
            }
            let x = p0.x + t * dx;
            if x + eps >= min_x && x <= max_x + eps {
                candidates.push((t, LayoutPoint { x, y: y_edge }));
            }
        }
    }

    candidates.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap_or(std::cmp::Ordering::Equal));
    candidates
        .into_iter()
        .find(|(t, _)| *t >= 0.0)
        .map(|(_, p)| p)
}

fn terminal_path_for_edge(
    points: &[LayoutPoint],
    from_rect: Rect,
    to_rect: Rect,
) -> Vec<LayoutPoint> {
    if points.len() < 2 {
        return points.to_vec();
    }
    let mut out = points.to_vec();

    if let Some(p) = intersect_segment_with_rect(&out[0], &out[1], from_rect) {
        out[0] = p;
    }
    let last = out.len() - 1;
    if let Some(p) = intersect_segment_with_rect(&out[last], &out[last - 1], to_rect) {
        out[last] = p;
    }

    out
}

fn layout_prepared(
    arena: &mut PreparedGraphArena,
    node_label_metrics_by_id: &HashMap<String, (f64, f64)>,
    render_roots: &mut Vec<ClassRenderRoot>,
    work_control: &mut OperationLayoutWorkControl,
) -> Result<(LayoutFragments, Rect)> {
    if arena.top.0 >= arena.graphs.len() {
        return Err(Error::InvalidModel {
            message: format!(
                "invalid prepared Class graph top {} for {} graphs",
                arena.top.0,
                arena.graphs.len()
            ),
        });
    }

    // Mermaid adds 25px rank separation at each recursive render boundary. Propagate those graph
    // settings top-down before laying nodes out bottom-up.
    let mut settings_stack = vec![arena.top];
    while let Some(parent_id) = settings_stack.pop() {
        let parent_ranksep = arena.graphs[parent_id.0].graph.graph().ranksep;
        let parent_nodesep = arena.graphs[parent_id.0].graph.graph().nodesep;
        let child_ids = arena.graphs[parent_id.0]
            .extracted
            .values()
            .copied()
            .collect::<Vec<_>>();
        for child_id in child_ids.into_iter().rev() {
            let Some(child) = arena.graphs.get_mut(child_id.0) else {
                return Err(Error::InvalidModel {
                    message: format!("missing prepared Class child graph {}", child_id.0),
                });
            };
            child.graph.graph_mut().ranksep = parent_ranksep + 25.0;
            child.graph.graph_mut().nodesep = parent_nodesep;
            settings_stack.push(child_id);
        }
    }

    enum PreparedFrame {
        Enter(PreparedGraphId),
        Exit(PreparedGraphId),
    }
    let mut graph_state = vec![0_u8; arena.graphs.len()];
    let mut postorder = Vec::with_capacity(arena.graphs.len());
    let mut stack = vec![PreparedFrame::Enter(arena.top)];
    while let Some(frame) = stack.pop() {
        match frame {
            PreparedFrame::Enter(graph_id) => {
                let Some(graph) = arena.graphs.get(graph_id.0) else {
                    return Err(Error::InvalidModel {
                        message: format!("missing prepared Class graph {}", graph_id.0),
                    });
                };
                match graph_state[graph_id.0] {
                    1 => {
                        return Err(Error::InvalidModel {
                            message: format!(
                                "cycle in prepared Class graph arena at {}",
                                graph_id.0
                            ),
                        });
                    }
                    2 => {
                        return Err(Error::InvalidModel {
                            message: format!(
                                "prepared Class graph {} has multiple owners",
                                graph_id.0
                            ),
                        });
                    }
                    _ => {}
                }
                graph_state[graph_id.0] = 1;
                stack.push(PreparedFrame::Exit(graph_id));
                for child_id in graph.extracted.values().rev() {
                    stack.push(PreparedFrame::Enter(*child_id));
                }
            }
            PreparedFrame::Exit(graph_id) => {
                graph_state[graph_id.0] = 2;
                postorder.push(graph_id);
            }
        }
    }
    if let Some(unattached) = graph_state.iter().position(|state| *state == 0) {
        return Err(Error::InvalidModel {
            message: format!("unattached prepared Class graph {unattached}"),
        });
    }

    let mut results = (0..arena.graphs.len())
        .map(|_| None)
        .collect::<Vec<Option<(LayoutFragments, Rect)>>>();
    for graph_id in postorder {
        let child_links = arena.graphs[graph_id.0].extracted.clone();
        let mut extracted_fragments = BTreeMap::new();
        for (cluster_id, child_id) in child_links {
            let Some(result) = results.get_mut(child_id.0).and_then(Option::take) else {
                return Err(Error::InvalidModel {
                    message: format!(
                        "missing laid out Class child graph {} for {cluster_id}",
                        child_id.0
                    ),
                });
            };
            extracted_fragments.insert(cluster_id, result);
        }
        let result = layout_prepared_node(
            &mut arena.graphs[graph_id.0],
            node_label_metrics_by_id,
            render_roots,
            extracted_fragments,
            work_control,
        )?;
        results[graph_id.0] = Some(result);
    }

    results
        .get_mut(arena.top.0)
        .and_then(Option::take)
        .ok_or_else(|| Error::InvalidModel {
            message: format!("missing laid out Class top graph {}", arena.top.0),
        })
}

fn layout_prepared_node(
    prepared: &mut PreparedGraph,
    node_label_metrics_by_id: &HashMap<String, (f64, f64)>,
    render_roots: &mut Vec<ClassRenderRoot>,
    extracted_fragments: BTreeMap<String, (LayoutFragments, Rect)>,
    work_control: &mut OperationLayoutWorkControl,
) -> Result<(LayoutFragments, Rect)> {
    let root_namespace_id = prepared.injected_cluster_root_id.clone();
    let render_root_id = ClassRenderRootId(render_roots.len());
    render_roots.push(ClassRenderRoot {
        namespace_id: root_namespace_id,
        ..Default::default()
    });
    let mut fragments = LayoutFragments {
        nodes: IndexMap::new(),
        edges: Vec::new(),
        render_root_id,
    };

    if let Some(root_id) = prepared.injected_cluster_root_id.clone() {
        if prepared.graph.node(&root_id).is_none() {
            prepared
                .graph
                .set_node(root_id.clone(), NodeLabel::default());
        }
        let top_level_ids: Vec<String> = prepared
            .graph
            .node_ids()
            .into_iter()
            .filter(|id| id != &root_id && prepared.graph.parent(id).is_none())
            .collect();
        for id in top_level_ids {
            prepared.graph.set_parent(id, root_id.clone());
        }
    }

    for (id, (_sub_frag, bounds)) in &extracted_fragments {
        let Some(n) = prepared.graph.node_mut(id) else {
            return Err(Error::InvalidModel {
                message: format!("missing cluster placeholder node: {id}"),
            });
        };
        n.width = bounds.width().max(1.0);
        n.height = bounds.height().max(1.0);
    }

    // Mermaid's dagre wrapper always sets `compound: true`, and Dagre's ranker expects a connected
    // graph. `dugong::layout` mirrors Dagre's full pipeline (including `nestingGraph`)
    // and should be used for class diagrams even when there are no explicit clusters.
    dugong::layout_controlled(&mut prepared.graph, work_control)
        .map_err(|error| work_control.map_dugong_error(error))?;

    // Mermaid does not render Dagre's internal dummy nodes/edges (border nodes, edge label nodes,
    // nesting artifacts). Filter them out before computing bounds and before merging extracted
    // layouts back into the parent.
    let mut dummy_nodes: HashSet<String> = HashSet::new();
    for id in prepared.graph.node_ids() {
        let Some(n) = prepared.graph.node(&id) else {
            continue;
        };
        if n.dummy.is_some() {
            dummy_nodes.insert(id);
            continue;
        }
        let is_cluster =
            !prepared.graph.children(&id).is_empty() || prepared.extracted.contains_key(&id);
        let (label_width, label_height) = node_label_metrics_by_id
            .get(id.as_str())
            .copied()
            .map(|(w, h)| (Some(w), Some(h)))
            .unwrap_or((None, None));
        fragments.nodes.insert(
            id.clone(),
            LayoutNode {
                id: id.clone(),
                x: n.x.unwrap_or(0.0),
                y: n.y.unwrap_or(0.0),
                width: n.width,
                height: n.height,
                is_cluster,
                label_width,
                label_height,
            },
        );
    }

    for key in prepared.graph.edge_keys() {
        let Some(e) = prepared.graph.edge_by_key(&key) else {
            continue;
        };
        if e.nesting_edge {
            continue;
        }
        if dummy_nodes.contains(&key.v) || dummy_nodes.contains(&key.w) {
            continue;
        }
        if !fragments.nodes.contains_key(&key.v) || !fragments.nodes.contains_key(&key.w) {
            continue;
        }
        let id = key
            .name
            .clone()
            .unwrap_or_else(|| format!("edge:{}:{}", key.v, key.w));

        let label = if e.width > 0.0 && e.height > 0.0 {
            Some(LayoutLabel {
                x: e.x.unwrap_or(0.0),
                y: e.y.unwrap_or(0.0),
                width: e.width,
                height: e.height,
            })
        } else {
            None
        };

        let points = e
            .points
            .iter()
            .map(|p| LayoutPoint { x: p.x, y: p.y })
            .collect::<Vec<_>>();

        let edge = LayoutEdge {
            id,
            from: key.v.clone(),
            to: key.w.clone(),
            from_cluster: None,
            to_cluster: None,
            points,
            label,
            start_label_left: None,
            start_label_right: None,
            end_label_left: None,
            end_label_right: None,
            start_marker: None,
            end_marker: None,
            stroke_dasharray: None,
        };

        let terminals = edge_terminal_metrics_from_extras(e);
        let has_terminals = terminals.start_left.is_some()
            || terminals.start_right.is_some()
            || terminals.end_left.is_some()
            || terminals.end_right.is_some();
        let terminal_meta = if has_terminals { Some(terminals) } else { None };

        fragments.edges.push((edge, terminal_meta));
    }

    let mut child_roots = extracted_fragments
        .iter()
        .map(|(id, (fragments, _))| (id.clone(), fragments.render_root_id))
        .collect::<BTreeMap<_, _>>();
    let current_node_ids = prepared.graph.node_ids();
    let edge_ids = fragments
        .edges
        .iter()
        .map(|(edge, _)| edge.id.clone())
        .collect();
    let cluster_ids = class_nodes_in_hierarchy_order(&prepared.graph)
        .into_iter()
        .filter(|id| {
            !dummy_nodes.contains(*id)
                && prepared.graph.child_count(id) > 0
                && !prepared.extracted.contains_key(*id)
        })
        .map(str::to_owned)
        .collect();
    let mut items = Vec::new();
    for id in current_node_ids {
        if dummy_nodes.contains(id.as_str()) {
            continue;
        }
        if let Some(root_id) = child_roots.remove(id.as_str()) {
            items.push(ClassRenderItem::Subgraph(root_id));
        } else if prepared.graph.children(&id).is_empty() {
            items.push(ClassRenderItem::Node(id));
        }
    }
    if !child_roots.is_empty() {
        return Err(Error::InvalidModel {
            message: format!(
                "class layout did not attach extracted render roots: {}",
                child_roots.keys().cloned().collect::<Vec<_>>().join(", ")
            ),
        });
    }
    let Some(render_root) = render_roots.get_mut(render_root_id.0) else {
        return Err(Error::InvalidModel {
            message: format!("missing class render root arena entry {}", render_root_id.0),
        });
    };
    render_root.edge_ids = edge_ids;
    render_root.cluster_ids = cluster_ids;
    render_root.items = items;

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
        for (e, _t) in &mut sub_frag.edges {
            for p in &mut e.points {
                p.x += dx;
                p.y += dy;
            }
            if let Some(l) = e.label.as_mut() {
                l.x += dx;
                l.y += dy;
            }
        }

        // The extracted subgraph includes its own copy of the cluster root node so bounds match
        // Mermaid's `updateNodeBounds(...)`. Do not merge that node back into the parent layout,
        // otherwise we'd overwrite the placeholder position computed by the parent graph layout.
        let _ = sub_frag.nodes.swap_remove(&cluster_id);

        fragments.nodes.extend(sub_frag.nodes);
        fragments.edges.extend(sub_frag.edges);
    }

    let mut points: Vec<(f64, f64)> = Vec::new();
    for n in fragments.nodes.values() {
        let r = Rect::from_center(n.x, n.y, n.width, n.height);
        points.push((r.min_x(), r.min_y()));
        points.push((r.max_x(), r.max_y()));
    }
    for (e, _t) in &fragments.edges {
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

struct ClassBoxMeasureCtx<'a> {
    measurer: &'a dyn TextMeasurer,
    mermaid_config: &'a merman_core::MermaidConfig,
    math_renderer: Option<&'a (dyn MathRenderer + Send + Sync)>,
    text_style: &'a TextStyle,
    html_calc_text_style: &'a TextStyle,
    wrap_probe_font_size: f64,
    wrap_mode: WrapMode,
    padding: f64,
    hide_empty_members_box: bool,
    capture_row_metrics: bool,
}

pub(crate) fn class_math_label_metrics(
    text: &str,
    measurer: &dyn TextMeasurer,
    style: &TextStyle,
    max_width_px: Option<f64>,
    mermaid_config: &merman_core::MermaidConfig,
    math_renderer: Option<&(dyn MathRenderer + Send + Sync)>,
) -> Option<crate::text::TextMetrics> {
    if !crate::math::contains_delimited_math(text) {
        return None;
    }
    let math_renderer = math_renderer?;
    crate::math::math_label_metrics_for_layout(crate::math::MathLabelMetricsRequest {
        measurer,
        raw_label: text,
        style,
        max_width_px,
        wrap_mode: WrapMode::HtmlLike,
        config: mermaid_config,
        math_renderer: Some(math_renderer),
    })
}

fn class_box_dimensions(
    node: &ClassNode,
    ctx: &ClassBoxMeasureCtx<'_>,
) -> (f64, f64, Option<ClassNodeLabelPlan>) {
    let measurer = ctx.measurer;
    let mermaid_config = ctx.mermaid_config;
    let math_renderer = ctx.math_renderer;
    let text_style = ctx.text_style;
    let html_calc_text_style = ctx.html_calc_text_style;
    let wrap_probe_font_size = ctx.wrap_probe_font_size;
    let wrap_mode = ctx.wrap_mode;
    let padding = ctx.padding;
    let hide_empty_members_box = ctx.hide_empty_members_box;
    let capture_row_metrics = ctx.capture_row_metrics;

    // Mermaid class nodes are sized by rendering the label groups (`textHelper(...)`) and taking
    // the resulting SVG bbox (`getBBox()`), then expanding by class padding (see upstream:
    // `rendering-elements/shapes/classBox.ts` + `diagrams/class/shapeUtil.ts`).
    //
    // Emulate that sizing logic deterministically using the same text measurer.
    let use_html_labels = matches!(wrap_mode, WrapMode::HtmlLike);
    let prepare_html_labels = use_html_labels && !class_node_requires_math(node);
    let padding = padding.max(0.0);
    let gap = padding;
    let text_padding = if use_html_labels { 0.0 } else { 3.0 };

    fn mermaid_class_svg_create_text_width_px(
        measurer: &dyn TextMeasurer,
        text: &str,
        style: &TextStyle,
        wrap_probe_font_size: f64,
    ) -> Option<f64> {
        let wrap_probe_style = TextStyle {
            font_family: style
                .font_family
                .clone()
                .or_else(|| Some("Arial".to_string())),
            font_size: wrap_probe_font_size.max(1.0),
            font_weight: None,
            font_style: None,
        };
        let w = class_html_create_text_width_px(text, measurer, &wrap_probe_style) as f64;
        if w.is_finite() && w > 0.0 {
            Some(w)
        } else {
            None
        }
    }

    fn wrap_class_svg_text_like_mermaid(
        text: &str,
        measurer: &dyn TextMeasurer,
        style: &TextStyle,
        wrap_probe_font_size: f64,
        bold: bool,
    ) -> String {
        let Some(wrap_width_px) =
            mermaid_class_svg_create_text_width_px(measurer, text, style, wrap_probe_font_size)
        else {
            return text.to_string();
        };
        let mut lines: Vec<String> = Vec::new();
        for line in crate::text::DeterministicTextMeasurer::normalized_text_lines(text) {
            let mut tokens = std::collections::VecDeque::from(
                crate::text::DeterministicTextMeasurer::split_line_to_words(&line),
            );
            let mut cur = String::new();

            while let Some(tok) = tokens.pop_front() {
                if cur.is_empty() && tok == " " {
                    continue;
                }

                let candidate = format!("{cur}{tok}");
                let candidate_w = if bold {
                    let bold_style = TextStyle {
                        font_family: style.font_family.clone(),
                        font_size: style.font_size,
                        font_weight: Some("bolder".to_string()),
                        font_style: None,
                    };
                    measurer.measure_svg_text_computed_length_px(candidate.trim_end(), &bold_style)
                } else {
                    measurer.measure_svg_text_computed_length_px(candidate.trim_end(), style)
                };
                if candidate_w <= wrap_width_px {
                    cur = candidate;
                    continue;
                }

                if !cur.trim().is_empty() {
                    lines.push(cur.trim_end().to_string());
                    cur.clear();
                    tokens.push_front(tok);
                    continue;
                }

                if tok == " " {
                    continue;
                }

                // Token itself does not fit on an empty line; split by characters.
                let chars = tok.chars().collect::<Vec<_>>();
                let mut cut = 1usize;
                while cut < chars.len() {
                    let head: String = chars[..cut].iter().collect();
                    let head_w = if bold {
                        let bold_style = TextStyle {
                            font_family: style.font_family.clone(),
                            font_size: style.font_size,
                            font_weight: Some("bolder".to_string()),
                            font_style: None,
                        };
                        measurer.measure_svg_text_computed_length_px(head.as_str(), &bold_style)
                    } else {
                        measurer.measure_svg_text_computed_length_px(head.as_str(), style)
                    };
                    if head_w > wrap_width_px {
                        break;
                    }
                    cut += 1;
                }
                cut = cut.saturating_sub(1).max(1);
                let head: String = chars[..cut].iter().collect();
                let tail: String = chars[cut..].iter().collect();
                lines.push(head);
                if !tail.is_empty() {
                    tokens.push_front(tail);
                }
            }

            if !cur.trim().is_empty() {
                lines.push(cur.trim_end().to_string());
            }
        }

        if lines.len() <= 1 {
            text.to_string()
        } else {
            lines.join("\n")
        }
    }

    let measure_label = |text: &str,
                         css_style: &str|
     -> (crate::text::TextMetrics, Option<ClassPreparedHtmlLabel>) {
        let effective_style = crate::class::class_effective_text_style(text_style, css_style);
        let style = effective_style.as_ref();
        let max_width_px = class_html_create_text_width_px(text, measurer, html_calc_text_style);
        if let Some(metrics) = class_math_label_metrics(
            text,
            measurer,
            style,
            Some(max_width_px.max(1) as f64),
            mermaid_config,
            math_renderer,
        ) {
            return (metrics, None);
        }
        if matches!(wrap_mode, WrapMode::HtmlLike) {
            let prepared = crate::class::class_prepare_html_label(
                measurer,
                style,
                text,
                max_width_px,
                css_style,
            );
            let metrics = prepared.metrics;
            (metrics, prepare_html_labels.then_some(prepared))
        } else if analyze_class_svg_markdown(text).has_styled_runs {
            (
                crate::text::measure_markdown_with_inline_styles(
                    measurer, text, style, None, wrap_mode,
                ),
                None,
            )
        } else {
            let wrapped = if matches!(wrap_mode, WrapMode::SvgLike | WrapMode::SvgLikeSingleRun) {
                wrap_class_svg_text_like_mermaid(text, measurer, style, wrap_probe_font_size, false)
            } else {
                text.to_string()
            };
            if matches!(wrap_mode, WrapMode::SvgLike | WrapMode::SvgLikeSingleRun) {
                // Keep layout sizing aligned with the SVG renderer, which emits labels through
                // Mermaid's Markdown-aware `createText(...)` path even for plain class text.
                (
                    crate::text::measure_markdown_with_inline_styles(
                        measurer, &wrapped, style, None, wrap_mode,
                    ),
                    None,
                )
            } else {
                (
                    measurer.measure_wrapped(&wrapped, style, None, wrap_mode),
                    None,
                )
            }
        }
    };

    fn label_rect(m: crate::text::TextMetrics, y_offset: f64) -> Option<Rect> {
        if !(m.width.is_finite() && m.height.is_finite()) {
            return None;
        }
        let w = m.width.max(0.0);
        let h = m.height.max(0.0);
        if w <= 0.0 || h <= 0.0 {
            return None;
        }
        let lines = m.line_count.max(1) as f64;
        let y = y_offset - (h / (2.0 * lines));
        Some(Rect::from_min_max(0.0, y, w, y + h))
    }

    // Annotation group: Mermaid only renders the first annotation.
    let mut annotation_rect: Option<Rect> = None;
    let mut annotation_group_height = 0.0;
    let mut annotation_prepared = None;
    if let Some(a) = node.annotations.first() {
        let t = format!("\u{00AB}{}\u{00BB}", decode_entities_minimal(a.trim()));
        let (m, prepared) = measure_label(&t, "");
        annotation_prepared = prepared;
        annotation_rect = label_rect(m, 0.0);
        if let Some(r) = annotation_rect {
            annotation_group_height = r.height().max(0.0);
        }
    }

    // Title label group (bold).
    let mut title_text = if use_html_labels {
        node.text.trim().to_string()
    } else {
        decode_entities_minimal(&node.text)
    };
    if !use_html_labels && title_text.starts_with('\\') {
        title_text = title_text.trim_start_matches('\\').to_string();
    }
    // Mermaid 11.16 renders class titles with `font-weight: bolder`; preserve that CSS value for
    // the operation-owned SVG bbox measurement below.
    let title_markdown_analysis =
        (!use_html_labels).then(|| analyze_class_svg_markdown(&title_text));
    let wrapped_title_text = if matches!(wrap_mode, WrapMode::SvgLike | WrapMode::SvgLikeSingleRun)
        && title_markdown_analysis
            .as_ref()
            .is_some_and(MermaidMarkdownAnalysis::all_runs_normal)
    {
        wrap_class_svg_text_like_mermaid(
            &title_text,
            measurer,
            text_style,
            wrap_probe_font_size,
            false,
        )
    } else {
        title_text.clone()
    };
    let title_lines =
        crate::text::DeterministicTextMeasurer::normalized_text_lines(&wrapped_title_text);
    let title_max_width_px = matches!(wrap_mode, WrapMode::HtmlLike).then(|| {
        class_html_create_text_width_px(title_text.as_str(), measurer, html_calc_text_style).max(1)
    });
    let title_max_width = title_max_width_px.map(|width| width as f64);

    let title_has_styled_runs = title_markdown_analysis
        .as_ref()
        .is_some_and(|analysis| analysis.has_styled_runs);
    let bold_title_style = TextStyle {
        font_family: text_style.font_family.clone(),
        font_size: text_style.font_size,
        font_weight: Some("bolder".to_string()),
        font_style: text_style.font_style.clone(),
    };
    let math_title_metrics = crate::math::contains_delimited_math(&title_text).then(|| {
        let max_width = title_max_width.unwrap_or_else(|| {
            class_html_create_text_width_px(title_text.as_str(), measurer, html_calc_text_style)
                .max(1) as f64
        });
        class_math_label_metrics(
            &title_text,
            measurer,
            &bold_title_style,
            Some(max_width),
            mermaid_config,
            math_renderer,
        )
    });
    let math_title_metrics = math_title_metrics.flatten();
    let has_math_title_metrics = math_title_metrics.is_some();
    let mut title_metrics = math_title_metrics.unwrap_or_else(|| {
        if matches!(wrap_mode, WrapMode::HtmlLike) || title_has_styled_runs {
            crate::text::measure_markdown_with_inline_styles(
                measurer,
                &wrapped_title_text,
                &bold_title_style,
                title_max_width,
                wrap_mode,
            )
        } else {
            measurer.measure_wrapped(&wrapped_title_text, &bold_title_style, None, wrap_mode)
        }
    });

    if !has_math_title_metrics
        && matches!(wrap_mode, WrapMode::SvgLike | WrapMode::SvgLikeSingleRun)
        && !title_has_styled_runs
    {
        let bold_title_style = TextStyle {
            font_family: text_style.font_family.clone(),
            font_size: text_style.font_size,
            font_weight: Some("bolder".to_string()),
            font_style: None,
        };
        let width = title_lines.iter().fold(0.0_f64, |width, line| {
            width.max(measurer.measure_svg_tspan_text_bbox_width_px(line, &bold_title_style))
        });
        if width.is_finite() && width > 0.0 {
            title_metrics.width = width;
        }
    }
    let title_rect = label_rect(title_metrics, 0.0);
    let title_group_height = title_rect.map(|r| r.height()).unwrap_or(0.0);

    let capture_fallback_row_metrics = capture_row_metrics && !prepare_html_labels;
    let measure_rows = |rows: &[merman_core::models::class_diagram::ClassMember]| {
        let mut rows_rect: Option<Rect> = None;
        let mut metrics_out: Option<Vec<crate::text::TextMetrics>> =
            capture_fallback_row_metrics.then(|| Vec::with_capacity(rows.len()));
        let mut prepared_out = prepare_html_labels.then(|| Vec::with_capacity(rows.len()));
        let mut y_offset = 0.0;
        for row in rows {
            let mut t = if use_html_labels {
                class_member_create_text_input(row)
            } else {
                decode_entities_minimal(class_member_display_text(row).as_str())
            };
            if !use_html_labels && t.starts_with('\\') {
                t = t.trim_start_matches('\\').to_string();
            }
            let (metrics, prepared) = measure_label(&t, row.css_style.as_str());
            if let Some(out) = metrics_out.as_mut() {
                out.push(metrics);
            }
            if let (Some(out), Some(prepared)) = (prepared_out.as_mut(), prepared) {
                out.push(prepared);
            }
            if let Some(r) = label_rect(metrics, y_offset) {
                if let Some(ref mut cur) = rows_rect {
                    cur.union(r);
                } else {
                    rows_rect = Some(r);
                }
            }
            y_offset += metrics.height.max(0.0) + text_padding;
        }

        (rows_rect, metrics_out, prepared_out)
    };

    // Members group.
    let (members_rect, members_metrics_out, members_prepared_out) = measure_rows(&node.members);
    let mut members_group_height = members_rect.map(|r| r.height()).unwrap_or(0.0);
    if members_group_height <= 0.0 {
        // Mermaid reserves half a gap when the members group is empty.
        members_group_height = (gap / 2.0).max(0.0);
    }

    // Methods group.
    let (methods_rect, methods_metrics_out, methods_prepared_out) = measure_rows(&node.methods);

    // Combine into the bbox returned by `textHelper(...)`.
    let mut bbox_opt: Option<Rect> = None;

    // annotation-group: centered horizontally (`translate(-w/2, 0)`).
    if let Some(mut r) = annotation_rect {
        let w = r.width();
        r.translate(-w / 2.0, 0.0);
        bbox_opt = Some(if let Some(mut cur) = bbox_opt {
            cur.union(r);
            cur
        } else {
            r
        });
    }

    // label-group: centered and shifted down by annotation height.
    if let Some(mut r) = title_rect {
        let w = r.width();
        r.translate(-w / 2.0, annotation_group_height);
        bbox_opt = Some(if let Some(mut cur) = bbox_opt {
            cur.union(r);
            cur
        } else {
            r
        });
    }

    // members-group: left-aligned, shifted down by label height + gap*2.
    if let Some(mut r) = members_rect {
        let dy = annotation_group_height + title_group_height + gap * 2.0;
        r.translate(0.0, dy);
        bbox_opt = Some(if let Some(mut cur) = bbox_opt {
            cur.union(r);
            cur
        } else {
            r
        });
    }

    // methods-group: left-aligned, shifted down by label height + members height + gap*4.
    if let Some(mut r) = methods_rect {
        let dy = annotation_group_height + title_group_height + (members_group_height + gap * 4.0);
        r.translate(0.0, dy);
        bbox_opt = Some(if let Some(mut cur) = bbox_opt {
            cur.union(r);
            cur
        } else {
            r
        });
    }

    let bbox = bbox_opt.unwrap_or_else(|| Rect::from_min_max(0.0, 0.0, 0.0, 0.0));
    let w = bbox.width().max(0.0);
    let mut h = bbox.height().max(0.0);

    // Mermaid adjusts bbox height depending on which compartments exist.
    if node.members.is_empty() && node.methods.is_empty() {
        h += gap;
    } else if !node.members.is_empty() && node.methods.is_empty() {
        h += gap * 2.0;
    }

    let render_extra_box =
        node.members.is_empty() && node.methods.is_empty() && !hide_empty_members_box;

    // The Dagre node bounds come from the rectangle passed to `updateNodeBounds`.
    let mut rect_w = w + 2.0 * padding;
    let mut rect_h = h + 2.0 * padding;
    if render_extra_box {
        rect_h += padding * 2.0;
    } else if node.members.is_empty() && node.methods.is_empty() {
        rect_h -= padding;
    }

    if node.type_param == "group" {
        rect_w = rect_w.max(500.0);
    }

    let label_plan = if prepare_html_labels {
        Some(ClassNodeLabelPlan::PreparedHtml(
            ClassPreparedHtmlNodeLabels {
                title: ClassPreparedHtmlLabel {
                    metrics: title_metrics,
                    max_width_px: title_max_width_px.unwrap_or(1),
                    xhtml: crate::text::mermaid_markdown_to_xhtml_label_fragment(&title_text, true),
                },
                annotation: annotation_prepared,
                members: members_prepared_out.unwrap_or_default(),
                methods: methods_prepared_out.unwrap_or_default(),
            },
        ))
    } else {
        capture_row_metrics.then(|| {
            ClassNodeLabelPlan::RowMetrics(ClassNodeRowMetrics {
                members: members_metrics_out.unwrap_or_default(),
                methods: methods_metrics_out.unwrap_or_default(),
            })
        })
    };

    (rect_w.max(1.0), rect_h.max(1.0), label_plan)
}

pub(crate) fn class_calculate_text_width_like_mermaid_px(
    text: &str,
    measurer: &dyn TextMeasurer,
    calc_text_style: &TextStyle,
) -> i64 {
    measure_mermaid_text_dimensions(measurer, text, calc_text_style).width
}

pub(crate) fn class_html_create_text_width_px(
    text: &str,
    measurer: &dyn TextMeasurer,
    calc_text_style: &TextStyle,
) -> i64 {
    class_calculate_text_width_like_mermaid_px(text, measurer, calc_text_style) + 50
}

fn class_effective_text_style<'a>(
    base: &'a TextStyle,
    css_style: &str,
) -> std::borrow::Cow<'a, TextStyle> {
    let mut style = std::borrow::Cow::Borrowed(base);
    for declaration in css_style.split(';') {
        let Some((key, value)) = crate::mermaid_style::parse_safe_style_decl(declaration) else {
            continue;
        };
        match key {
            "font-weight" => style.to_mut().font_weight = Some(value.trim().to_string()),
            "font-style" => style.to_mut().font_style = Some(value.trim().to_string()),
            "font-size" => {
                if let Some(font_size) =
                    crate::mermaid_style::parse_css_font_size_px(value, style.font_size)
                {
                    style.to_mut().font_size = font_size;
                }
            }
            "font-family" => {
                style.to_mut().font_family = Some(value.trim().to_string());
            }
            _ => {}
        }
    }
    style
}

pub(crate) fn class_html_measure_label_metrics(
    measurer: &dyn TextMeasurer,
    style: &TextStyle,
    text: &str,
    max_width_px: i64,
    css_style: &str,
) -> crate::text::TextMetrics {
    class_prepare_html_label(measurer, style, text, max_width_px, css_style).metrics
}

pub(crate) fn class_prepare_html_label(
    measurer: &dyn TextMeasurer,
    style: &TextStyle,
    text: &str,
    max_width_px: i64,
    css_style: &str,
) -> ClassPreparedHtmlLabel {
    let max_width = Some(max_width_px.max(1) as f64);
    let effective_style = class_effective_text_style(style, css_style);
    let style = effective_style.as_ref();
    let xhtml = crate::text::mermaid_markdown_to_xhtml_label_fragment(text, true);
    let mut metrics = crate::text::measure_xhtml_label_fragment(
        measurer,
        &xhtml,
        style,
        max_width,
        WrapMode::HtmlLike,
    );

    let rendered_width = metrics.width;
    if metrics.line_count == 1
        && rendered_width > 0.0
        && rendered_width < max_width_px.max(1) as f64 - 0.01
    {
        metrics.height = crate::text::flowchart_html_line_height_px(style.font_size);
        metrics.line_count = 1;
    }

    ClassPreparedHtmlLabel {
        metrics,
        max_width_px,
        xhtml,
    }
}

pub(crate) fn class_normalize_xhtml_br_tags(html: &str) -> String {
    html.replace("<br>", "<br />")
        .replace("<br/>", "<br />")
        .replace("<br >", "<br />")
        .replace("</br>", "<br />")
        .replace("</br/>", "<br />")
        .replace("</br />", "<br />")
        .replace("</br >", "<br />")
}

pub(crate) fn class_note_html_fragment(
    note_src: &str,
    mermaid_config: &merman_core::MermaidConfig,
) -> String {
    let note_html = note_src.replace("\r\n", "\n").replace('\n', "<br />");
    let note_html = merman_core::sanitize::sanitize_text(&note_html, mermaid_config);
    class_normalize_xhtml_br_tags(&note_html)
}

pub(crate) fn class_html_measure_note_metrics(
    measurer: &dyn TextMeasurer,
    style: &TextStyle,
    note_src: &str,
    mermaid_config: &merman_core::MermaidConfig,
) -> crate::text::TextMetrics {
    let html = class_note_html_fragment(note_src, mermaid_config);
    crate::text::measure_html_with_inline_styles(measurer, &html, style, None, WrapMode::HtmlLike)
}

pub(crate) fn analyze_class_svg_markdown(text: &str) -> MermaidMarkdownAnalysis {
    analyze_mermaid_markdown(text, true)
}

pub(crate) fn class_svg_single_line_plain_label_width_px(
    text: &str,
    measurer: &dyn TextMeasurer,
    text_style: &TextStyle,
) -> Option<f64> {
    let trimmed = text.trim();
    let analysis = analyze_class_svg_markdown(trimmed);
    if trimmed.is_empty() || analysis.line_count != 1 || !analysis.all_runs_normal() {
        return None;
    }

    let canonical_line = analysis.lines.into_iter().next()?.into_iter().fold(
        String::new(),
        |mut line, (word, _)| {
            if !line.is_empty() {
                line.push(' ');
            }
            line.push_str(&word);
            line
        },
    );
    let (left, right) = measurer.measure_svg_text_bbox_x(&canonical_line, text_style);
    let width = left + right;
    (width.is_finite() && width > 0.0).then_some(width)
}

fn note_dimensions(
    text: &str,
    measurer: &dyn TextMeasurer,
    text_style: &TextStyle,
    wrap_mode: WrapMode,
    padding: f64,
    mermaid_config: Option<&merman_core::MermaidConfig>,
    math_renderer: Option<&(dyn MathRenderer + Send + Sync)>,
) -> (f64, f64, crate::text::TextMetrics) {
    let p = padding.max(0.0);
    let label = decode_entities_minimal(text);
    let math_metrics = mermaid_config.and_then(|config| {
        class_math_label_metrics(&label, measurer, text_style, None, config, math_renderer)
    });
    let has_math_metrics = math_metrics.is_some();
    let mut m = if let Some(metrics) = math_metrics {
        metrics
    } else if matches!(wrap_mode, WrapMode::HtmlLike) {
        mermaid_config
            .map(|config| class_html_measure_note_metrics(measurer, text_style, text, config))
            .unwrap_or_else(|| measurer.measure_wrapped(&label, text_style, None, wrap_mode))
    } else {
        measurer.measure_wrapped(&label, text_style, None, wrap_mode)
    };
    if !has_math_metrics
        && matches!(wrap_mode, WrapMode::SvgLike | WrapMode::SvgLikeSingleRun)
        && let Some(width) =
            class_svg_single_line_plain_label_width_px(label.as_str(), measurer, text_style)
    {
        m.width = width;
    }
    (m.width + p, m.height + p, m)
}

fn label_metrics(
    text: &str,
    measurer: &dyn TextMeasurer,
    text_style: &TextStyle,
    wrap_mode: WrapMode,
    mermaid_config: &merman_core::MermaidConfig,
    math_renderer: Option<&(dyn MathRenderer + Send + Sync)>,
) -> (f64, f64) {
    if text.trim().is_empty() {
        return (0.0, 0.0);
    }
    let t = decode_entities_minimal(text);
    let m = class_math_label_metrics(
        &t,
        measurer,
        text_style,
        None,
        mermaid_config,
        math_renderer,
    )
    .unwrap_or_else(|| measurer.measure_wrapped(&t, text_style, None, wrap_mode));
    (m.width.max(0.0), m.height.max(0.0))
}

fn edge_title_metrics(
    text: &str,
    measurer: &dyn TextMeasurer,
    text_style: &TextStyle,
    wrap_mode: WrapMode,
    mermaid_config: &merman_core::MermaidConfig,
    math_renderer: Option<&(dyn MathRenderer + Send + Sync)>,
) -> (f64, f64) {
    let trimmed = text.trim();
    if trimmed.is_empty() {
        return (0.0, 0.0);
    }

    let label = decode_entities_minimal(text);
    if let Some(metrics) = class_math_label_metrics(
        &label,
        measurer,
        text_style,
        Some(MERMAID_CREATE_TEXT_DEFAULT_WIDTH_PX),
        mermaid_config,
        math_renderer,
    ) {
        return (metrics.width.max(0.0), metrics.height.max(0.0));
    }
    if matches!(wrap_mode, WrapMode::HtmlLike) {
        let metrics = class_html_measure_label_metrics(
            measurer,
            text_style,
            &label,
            MERMAID_CREATE_TEXT_DEFAULT_WIDTH_PX as i64,
            "",
        );
        return (metrics.width.max(0.0), metrics.height.max(0.0));
    }

    let mut metrics = measurer.measure_wrapped(&label, text_style, None, wrap_mode);
    if let Some(width) =
        class_svg_single_line_plain_label_width_px(label.as_str(), measurer, text_style)
    {
        metrics.width = width;
    }
    (metrics.width.max(0.0) + 4.0, metrics.height.max(0.0) + 4.0)
}

fn set_extras_label_metrics(extras: &mut BTreeMap<String, Value>, key: &str, w: f64, h: f64) {
    let obj = Value::Object(
        [
            ("width".to_string(), Value::from(w)),
            ("height".to_string(), Value::from(h)),
        ]
        .into_iter()
        .collect(),
    );
    extras.insert(key.to_string(), obj);
}

pub(crate) fn layout_class_diagram_typed_with_config(
    model: &ClassDiagramModel,
    effective_config: &merman_core::MermaidConfig,
    measurer: &dyn TextMeasurer,
    math_renderer: Option<&(dyn MathRenderer + Send + Sync)>,
    work_control: &mut OperationLayoutWorkControl,
) -> Result<ClassDiagramLayout> {
    let settings = ClassConfigView::new(effective_config.as_value()).layout_settings();
    let measured = measure_class_diagram(
        model,
        effective_config,
        measurer,
        math_renderer,
        &settings,
        false,
    )?;
    layout_class_diagram_dagre(model, measured, &settings, work_control)
}

#[cfg(feature = "layout-elk")]
/// Lays out a Class diagram through ELK using the render operation's captured seed.
///
/// This remains crate-private so direct callers cannot accidentally turn ELK's unseeded
/// `randomSeed = 0` sentinel into a process-random layout.
pub(crate) fn layout_class_diagram_elk_typed_with_config_and_operation_seed(
    model: &ClassDiagramModel,
    effective_config: &merman_core::MermaidConfig,
    measurer: &dyn TextMeasurer,
    math_renderer: Option<&(dyn MathRenderer + Send + Sync)>,
    operation_seed: elk::ElkOperationSeed,
    work_control: &mut OperationLayoutWorkControl,
) -> Result<ClassDiagramLayout> {
    let settings = ClassConfigView::new(effective_config.as_value()).layout_settings();
    let measured = measure_class_diagram(
        model,
        effective_config,
        measurer,
        math_renderer,
        &settings,
        true,
    )?;
    layout_class_diagram_elk_from_measured(
        model,
        measured,
        ClassElkLayoutSettings {
            // ClassDB.getData assigns class.padding to namespaces, independently of ELK's
            // fixed content inset and the legacy Dagre cluster settings.
            namespace_padding: settings.class_padding,
            title_margin_top: settings.title_margin_top,
            title_margin_bottom: settings.title_margin_bottom,
            effective_config: effective_config.as_value(),
        },
        operation_seed,
        work_control,
    )
}

fn measure_class_diagram(
    model: &ClassDiagramModel,
    mermaid_config: &merman_core::MermaidConfig,
    measurer: &dyn TextMeasurer,
    math_renderer: Option<&(dyn MathRenderer + Send + Sync)>,
    settings: &ClassLayoutSettings,
    uses_elk_adapter: bool,
) -> Result<MeasuredClassDiagram> {
    validate_class_namespace_hierarchy(model)?;
    let wrap_mode_node = settings.wrap_mode_node;
    let wrap_mode_label = settings.wrap_mode_label;
    let wrap_mode_note = settings.wrap_mode_note;
    let class_padding = settings.class_padding;
    let text_style = &settings.text_style;
    // Class CSS fixes cardinalities at 11px independently of class member text.
    let terminal_text_style = TextStyle {
        font_family: text_style.font_family.clone(),
        font_size: 11.0,
        font_weight: None,
        font_style: None,
    };
    let html_calc_text_style = &settings.html_calc_text_style;
    let wrap_probe_font_size = settings.wrap_probe_font_size;
    let hide_empty_members_box = settings.hide_empty_members_box;
    let contains_math = class_requires_math(model);
    let capture_row_metrics = matches!(wrap_mode_node, WrapMode::HtmlLike) || contains_math;
    let capture_note_label_metrics = matches!(wrap_mode_note, WrapMode::HtmlLike) || contains_math;
    let note_html_config = capture_note_label_metrics.then_some(mermaid_config);
    let mut class_label_plans_by_id: FxHashMap<String, Arc<ClassNodeLabelPlan>> =
        FxHashMap::default();
    let mut node_label_metrics_by_id: HashMap<String, (f64, f64)> = HashMap::new();
    let namespace_ids = class_namespace_ids_in_decl_order(model);

    let mut g = MeasuredGraph {
        direction: normalize_dir(&model.direction),
        nodesep: settings.nodesep,
        ranksep: settings.ranksep,
        nodes: IndexMap::new(),
        parents: IndexMap::new(),
        edges: IndexMap::new(),
    };
    let mut namespace_label_metrics_by_id = HashMap::new();

    let class_box_measure_ctx = ClassBoxMeasureCtx {
        measurer,
        mermaid_config,
        math_renderer,
        text_style,
        html_calc_text_style,
        wrap_probe_font_size,
        wrap_mode: wrap_mode_node,
        padding: class_padding,
        hide_empty_members_box,
        capture_row_metrics,
    };

    let insert_class_node =
        |g: &mut MeasuredGraph,
         c: &ClassNode,
         class_label_plans_by_id: &mut FxHashMap<String, Arc<ClassNodeLabelPlan>>| {
            let (w, h, label_plan) = class_box_dimensions(c, &class_box_measure_ctx);
            if let Some(label_plan) = label_plan {
                class_label_plans_by_id.insert(c.id.clone(), Arc::new(label_plan));
            }
            g.set_node(
                c.id.clone(),
                MeasuredNode {
                    width: w,
                    height: h,
                    ..Default::default()
                },
            );
        };

    let insert_note_node =
        |g: &mut MeasuredGraph,
         n: &ClassNote,
         node_label_metrics_by_id: &mut HashMap<String, (f64, f64)>| {
            let (w, h, metrics) = note_dimensions(
                &n.text,
                measurer,
                text_style,
                wrap_mode_note,
                class_padding,
                note_html_config,
                math_renderer,
            );
            if capture_note_label_metrics {
                node_label_metrics_by_id.insert(
                    n.id.clone(),
                    (metrics.width.max(0.0), metrics.height.max(0.0)),
                );
            }
            g.set_node(
                n.id.clone(),
                MeasuredNode {
                    width: w.max(1.0),
                    height: h.max(1.0),
                    ..Default::default()
                },
            );
        };

    for &id in &namespace_ids {
        // Mermaid's v3 `ClassDB.getData()` emits namespace groups before classes, notes, and
        // interfaces. Graphlib preserves that insertion order for Dagre's `initOrder`.
        g.set_node(
            id.to_string(),
            MeasuredNode {
                is_namespace: true,
                ..Default::default()
            },
        );
        namespace_label_metrics_by_id.insert(
            id.to_string(),
            label_metrics(
                class_namespace_label(model, id),
                measurer,
                text_style,
                if ClassConfigView::new(mermaid_config.as_value()).render_edge_html_labels() {
                    WrapMode::HtmlLike
                } else {
                    WrapMode::SvgLike
                },
                mermaid_config,
                math_renderer,
            ),
        );

        if let Some(parent) = model
            .namespaces
            .get(id)
            .and_then(|ns| ns.parent.as_deref())
            .map(str::trim)
            .filter(|parent| !parent.is_empty())
            && model.namespaces.contains_key(parent)
        {
            g.set_parent(id.to_string(), parent.to_string());
        }
    }

    for c in model.classes.values() {
        insert_class_node(&mut g, c, &mut class_label_plans_by_id);
        if let Some(parent) = c
            .parent
            .as_ref()
            .map(|s| s.trim())
            .filter(|s| !s.is_empty())
            && model.namespaces.contains_key(parent)
        {
            g.set_parent(c.id.clone(), parent.to_string());
        }
    }

    for n in &model.notes {
        insert_note_node(&mut g, n, &mut node_label_metrics_by_id);
        if let Some(parent) = n
            .parent
            .as_ref()
            .map(|s| s.trim())
            .filter(|s| !s.is_empty())
            && model.namespaces.contains_key(parent)
        {
            g.set_parent(n.id.clone(), parent.to_string());
        }
    }

    // Interface nodes follow notes in `ClassDB.getData()` and remain at the root even when
    // their related class belongs to a namespace. `squareRect` adds fixed Neo label padding;
    // classic interfaces have no node padding.
    let (interface_padding_x, interface_padding_y) =
        if ClassConfigView::new(mermaid_config.as_value()).diagram_look() == "neo" {
            (32.0, 24.0)
        } else {
            (0.0, 0.0)
        };
    for iface in &model.interfaces {
        let label = decode_entities_minimal(iface.label.trim());
        let metrics = crate::graph_label::flowchart_label_metrics_for_layout(
            crate::graph_label::FlowchartLabelMetricsRequest {
                measurer,
                raw_label: &label,
                label_type: "text",
                style: text_style,
                max_width_px: Some(
                    ClassConfigView::new(mermaid_config.as_value()).interface_wrapping_width(),
                ),
                wrap_mode: wrap_mode_node,
                config: mermaid_config,
                math_renderer,
            },
        );
        let (tw, th) = (metrics.width.max(0.0), metrics.height.max(0.0));
        node_label_metrics_by_id.insert(iface.id.clone(), (tw, th));
        g.set_node(
            iface.id.clone(),
            MeasuredNode {
                width: (tw + interface_padding_x).max(1.0),
                height: (th + interface_padding_y).max(1.0),
                ..Default::default()
            },
        );
    }

    // Note attachments precede class relations in Mermaid's layout edge array. Their IDs use the
    // note declaration index, including unattached notes, rather than the relation count.
    for (i, note) in model.notes.iter().enumerate() {
        let Some(class_id) = note.class_id.as_ref() else {
            continue;
        };
        if !model.classes.contains_key(class_id) {
            continue;
        }
        g.set_edge(
            note.id.clone(),
            class_id.clone(),
            format!("edgeNote{i}"),
            MeasuredEdge {
                width: 0.0,
                height: 0.0,
                terminals: None,
            },
        );
    }

    for rel in &model.relations {
        let title = if uses_elk_adapter {
            crate::text::mermaid_html_breaks_to_newlines(&rel.title)
        } else {
            std::borrow::Cow::Borrowed(rel.title.as_str())
        };
        let (lw, lh) = edge_title_metrics(
            &title,
            measurer,
            text_style,
            wrap_mode_label,
            mermaid_config,
            math_renderer,
        );
        let start_text = rel.relation_title_1.clone().unwrap_or_default();
        let end_text = rel.relation_title_2.clone().unwrap_or_default();

        let (srw, srh) = label_metrics(
            &start_text,
            measurer,
            &terminal_text_style,
            wrap_mode_label,
            mermaid_config,
            math_renderer,
        );
        let (elw, elh) = label_metrics(
            &end_text,
            measurer,
            &terminal_text_style,
            wrap_mode_label,
            mermaid_config,
            math_renderer,
        );

        // Mermaid passes `edge.arrowTypeStart ? 10 : 0` / `edge.arrowTypeEnd ? 10 : 0`
        // into `calcTerminalLabelPosition(...)`. In class diagrams the arrow type strings are
        // still truthy even for plain `none` association ends, so any rendered terminal label
        // effectively gets the 10px marker offset on its own side.
        let start_marker = if start_text.trim().is_empty() {
            0.0
        } else {
            10.0
        };
        let end_marker = if end_text.trim().is_empty() {
            0.0
        } else {
            10.0
        };

        g.set_edge(
            rel.id1.clone(),
            rel.id2.clone(),
            rel.id.clone(),
            MeasuredEdge {
                width: lw,
                height: lh,
                terminals: Some(EdgeTerminalMetrics {
                    start_left: None,
                    start_right: (srw > 0.0 && srh > 0.0).then_some((srw, srh)),
                    end_left: (elw > 0.0 && elh > 0.0).then_some((elw, elh)),
                    end_right: None,
                    start_marker,
                    end_marker,
                    #[cfg(feature = "layout-elk")]
                    start_arrow_type: class_arrow_type_for_relation_end(rel.relation.type1),
                    #[cfg(feature = "layout-elk")]
                    end_arrow_type: class_arrow_type_for_relation_end(rel.relation.type2),
                }),
            },
        );
    }

    Ok(MeasuredClassDiagram {
        graph: g,
        class_label_plans_by_id,
        node_label_metrics_by_id,
        namespace_label_metrics_by_id,
    })
}

fn layout_class_diagram_dagre(
    model: &ClassDiagramModel,
    measured: MeasuredClassDiagram,
    settings: &ClassLayoutSettings,
    work_control: &mut OperationLayoutWorkControl,
) -> Result<ClassDiagramLayout> {
    let namespace_ids = class_namespace_ids_in_decl_order(model);
    let namespace_padding = settings.namespace_padding;
    let title_margin_top = settings.title_margin_top;
    let title_margin_bottom = settings.title_margin_bottom;
    let MeasuredClassDiagram {
        graph,
        class_label_plans_by_id,
        node_label_metrics_by_id,
        namespace_label_metrics_by_id,
    } = measured;
    let g = Box::new(graph.to_dagre());
    let mut prepared = prepare_graph(g)?;
    let mut render_roots = Vec::new();
    let (mut fragments, _bounds) = layout_prepared(
        &mut prepared,
        &node_label_metrics_by_id,
        &mut render_roots,
        work_control,
    )?;

    let mut node_rect_by_id: HashMap<String, Rect> = HashMap::new();
    for n in fragments.nodes.values() {
        node_rect_by_id.insert(n.id.clone(), Rect::from_center(n.x, n.y, n.width, n.height));
    }

    for (edge, terminal_meta) in fragments.edges.iter_mut() {
        let Some(meta) = terminal_meta.clone() else {
            continue;
        };
        let points = if let (Some(from), Some(to)) = (
            node_rect_by_id.get(edge.from.as_str()).copied(),
            node_rect_by_id.get(edge.to.as_str()).copied(),
        ) {
            terminal_path_for_edge(&edge.points, from, to)
        } else {
            edge.points.clone()
        };
        apply_class_terminal_labels(edge, &meta, &points)?;
    }

    let mut clusters: Vec<LayoutCluster> = Vec::new();
    // Mermaid renders namespaces as Dagre clusters. The cluster geometry comes from the Dagre
    // compound layout (not a post-hoc union of class-node bboxes). Use the computed namespace
    // node x/y/width/height and mirror `clusters.js` sizing tweaks for title width.
    for &id in &namespace_ids {
        let Some(ns_node) = fragments.nodes.get(id) else {
            continue;
        };
        let cx = ns_node.x;
        let cy = ns_node.y;
        let base_w = ns_node.width.max(1.0);
        let base_h = ns_node.height.max(1.0);

        let title = class_namespace_label(model, id).to_string();
        let (tw, th) = namespace_label_metrics_by_id[id];
        let min_title_w = (tw + namespace_padding).max(1.0);
        let width = if base_w <= min_title_w {
            min_title_w
        } else {
            base_w
        };
        let diff = if base_w <= min_title_w {
            (width - base_w) / 2.0 - namespace_padding
        } else {
            -namespace_padding
        };
        let offset_y = th - namespace_padding / 2.0;
        let title_label = LayoutLabel {
            x: cx,
            y: (cy - base_h / 2.0) + title_margin_top + th / 2.0,
            width: tw,
            height: th,
        };

        clusters.push(LayoutCluster {
            id: id.to_string(),
            x: cx,
            y: cy,
            width,
            height: base_h,
            diff,
            offset_y,
            title: title.clone(),
            title_label,
            requested_dir: None,
            effective_dir: normalize_dir(&model.direction),
            padding: namespace_padding,
            title_margin_top,
            title_margin_bottom,
        });
    }

    let render_tree = ClassRenderTree {
        roots: render_roots,
        top: fragments.render_root_id,
    };
    let nodes: Vec<LayoutNode> = fragments.nodes.into_values().collect();
    let edges: Vec<LayoutEdge> = fragments.edges.into_iter().map(|(e, _)| e).collect();

    let namespace_order: std::collections::HashMap<&str, usize> = namespace_ids
        .iter()
        .copied()
        .enumerate()
        .map(|(idx, id)| (id, idx))
        .collect();
    clusters.sort_by(|a, b| {
        namespace_order
            .get(a.id.as_str())
            .copied()
            .unwrap_or(usize::MAX)
            .cmp(
                &namespace_order
                    .get(b.id.as_str())
                    .copied()
                    .unwrap_or(usize::MAX),
            )
            .then_with(|| a.id.cmp(&b.id))
    });

    let bounds = compute_bounds(&nodes, &edges, &clusters);

    Ok(ClassDiagramLayout {
        nodes,
        edges,
        clusters,
        bounds,
        uses_elk_adapter_dom: false,
        class_label_plans_by_id,
        render_tree,
    })
}

/// Debug-only helper: builds the production Class Dagre graph and returns it before layout runs.
///
/// Shares measurement and the Dagre projection with production, including source order,
/// named multiedges, terminal-label metrics, and namespace parents.
#[doc(hidden)]
pub fn debug_build_class_diagram_dagre_graph(
    model: &ClassDiagramModel,
    effective_config: &merman_core::MermaidConfig,
    measurer: &dyn TextMeasurer,
) -> Result<ClassLayoutGraph> {
    let settings = ClassConfigView::new(effective_config.as_value()).layout_settings();
    let measured =
        measure_class_diagram(model, effective_config, measurer, None, &settings, false)?;
    Ok(measured.graph.to_dagre())
}

fn validate_class_namespace_hierarchy(model: &ClassDiagramModel) -> Result<()> {
    const UNVISITED: u8 = 0;
    const VISITING: u8 = 1;
    const COMPLETE: u8 = 2;

    let ids = model
        .namespaces
        .keys()
        .map(String::as_str)
        .collect::<Vec<_>>();
    let index_by_id = ids
        .iter()
        .copied()
        .enumerate()
        .map(|(index, id)| (id, index))
        .collect::<FxHashMap<_, _>>();
    let parents = ids
        .iter()
        .map(|id| {
            model
                .namespaces
                .get(*id)
                .and_then(|namespace| namespace.parent.as_deref())
                .and_then(|parent| index_by_id.get(parent).copied())
        })
        .collect::<Vec<_>>();
    let mut state = vec![UNVISITED; ids.len()];
    let mut depth = vec![0_usize; ids.len()];

    for start in 0..ids.len() {
        if state[start] == COMPLETE {
            continue;
        }

        let mut path = Vec::new();
        let mut current = Some(start);
        let inherited_depth = loop {
            let Some(index) = current else {
                break None;
            };
            match state[index] {
                UNVISITED => {
                    state[index] = VISITING;
                    path.push(index);
                    current = parents[index];
                }
                VISITING => {
                    return Err(Error::InvalidModel {
                        message: format!("class namespace parent cycle involving {}", ids[index]),
                    });
                }
                COMPLETE => break Some(depth[index].saturating_add(1)),
                _ => unreachable!("Class namespace traversal state is internal"),
            }
        };

        let mut next_depth = inherited_depth.unwrap_or(0);
        for index in path.into_iter().rev() {
            if next_depth > merman_core::MAX_DIAGRAM_NESTING_DEPTH {
                return Err(Error::InvalidModel {
                    message: format!(
                        "class namespace nesting depth exceeds {} at {}",
                        merman_core::MAX_DIAGRAM_NESTING_DEPTH,
                        ids[index]
                    ),
                });
            }
            depth[index] = next_depth;
            state[index] = COMPLETE;
            next_depth = next_depth.saturating_add(1);
        }
    }
    Ok(())
}

#[cfg(feature = "layout-elk")]
struct ClassElkLayoutSettings<'a> {
    namespace_padding: f64,
    title_margin_top: f64,
    title_margin_bottom: f64,
    effective_config: &'a Value,
}

#[cfg(feature = "layout-elk")]
fn layout_class_diagram_elk_from_measured(
    model: &ClassDiagramModel,
    measured: MeasuredClassDiagram,
    settings: ClassElkLayoutSettings<'_>,
    operation_seed: elk::ElkOperationSeed,
    work_control: &mut OperationLayoutWorkControl,
) -> Result<ClassDiagramLayout> {
    let mut elk_graph = class_measured_to_elk_graph(&measured, &settings, &mut Some(work_control))?;
    let orientation = crate::elk_feedback_edges::orient_feedback_edges(
        &mut elk_graph,
        settings.effective_config,
        &mut Some(work_control),
    )?;
    let mut layout = elk::layout_with_operation_seed_and_work_control(
        orientation.graph(),
        operation_seed,
        work_control,
    )
    .map_err(|error| work_control.map_elk_error_with_context(error, "Class ELK"))?;
    orientation.restore(&mut layout, &mut Some(work_control))?;
    class_layout_from_elk(
        model,
        measured,
        &elk_graph,
        layout,
        settings,
        &mut Some(work_control),
    )
}

#[cfg(feature = "layout-elk")]
fn class_measured_to_elk_graph(
    measured: &MeasuredClassDiagram,
    settings: &ClassElkLayoutSettings<'_>,
    work: &mut Option<&mut OperationLayoutWorkControl>,
) -> Result<elk::Graph> {
    let graph = &measured.graph;
    let options = class_elk_layout_options(settings.effective_config);
    let direction = match graph.direction.as_str() {
        "LR" => elk::Direction::Right,
        "RL" => elk::Direction::Left,
        "BT" => elk::Direction::Up,
        _ => elk::Direction::Down,
    };
    let parents = graph
        .parents
        .values()
        .map(String::as_str)
        .collect::<HashSet<_>>();
    let mut nodes: Vec<_> = graph
        .nodes
        .iter()
        .map(|(id, node)| {
            let is_group = node.is_namespace || parents.contains(id.as_str());
            let namespace_label =
                measured
                    .namespace_label_metrics_by_id
                    .get(id)
                    .map(|&(width, height)| elk::Label {
                        width: width.max(1.0),
                        height: height.max(1.0),
                    });
            elk::Node {
                id: id.clone(),
                container: elk::ContainerNodeOptions {
                    padding: if is_group {
                        settings.namespace_padding
                    } else {
                        0.0
                    },
                    ..Default::default()
                },
                label_text: is_group.then(|| id.clone()),
                kind: if is_group {
                    elk::NodeKind::Group
                } else {
                    elk::NodeKind::Leaf
                },
                width: node.width.max(if is_group { 0.0 } else { 1.0 }),
                height: node.height.max(if is_group { 0.0 } else { 1.0 }),
                parent: graph.parents.get(id).cloned(),
                direction: is_group.then_some(direction),
                hierarchy_handling: is_group.then_some(elk::HierarchyHandling::IncludeChildren),
                layer_constraint: None,
                port_alignment: None,
                label: namespace_label,
            }
        })
        .collect();
    if let Some(work) = work.as_deref_mut() {
        let terminal_work =
            work.checked_mul(graph.edges.len(), elk::TerminalLabelKey::ALL.len())?;
        work.charge_adapter(terminal_work)?;
    }
    let edges = graph
        .edges
        .iter()
        .map(|((source, target, id), edge)| elk::Edge {
            id: id.clone(),
            source: source.clone(),
            target: target.clone(),
            label: (edge.width > 0.0 && edge.height > 0.0).then_some(elk::Label {
                width: edge.width,
                height: edge.height,
            }),
            terminal_labels: if options.algorithm == elk::Algorithm::Layered {
                edge.terminals
                    .as_ref()
                    .map(|meta| {
                        elk::TerminalLabelKey::ALL
                            .into_iter()
                            .filter_map(|key| {
                                let (width, height) = class_terminal_metrics(meta, key)?;
                                // Mermaid's class markers are 12px wide. Padding reserves half that
                                // width on each side of the measured label beside a real marker.
                                let marker = if key.at_start() {
                                    meta.start_arrow_type
                                } else {
                                    meta.end_arrow_type
                                };
                                let padding = if marker.is_some() { 6.0 } else { 0.0 };
                                Some(elk::TerminalLabel {
                                    key,
                                    label: elk::Label {
                                        width: width + 2.0 * padding,
                                        height: height + 2.0 * padding,
                                    },
                                })
                            })
                            .collect()
                    })
                    .unwrap_or_default()
            } else {
                Vec::new()
            },
            minlen: 1,
            inside_self_loops_yo: false,
        })
        .collect();
    if settings
        .effective_config
        .pointer("/elk/keepEntryNodeOnTop")
        .and_then(Value::as_bool)
        != Some(false)
    {
        crate::elk_adapter::apply_cyclic_entry_constraints(
            &mut nodes,
            graph
                .edges
                .keys()
                .map(|(source, target, _)| (source.as_str(), target.as_str())),
            work,
        )?;
    }
    Ok(elk::Graph {
        id: "classDiagram".to_string(),
        direction,
        nodes,
        edges,
        spacing: elk::Spacing {
            node_node: graph.nodesep,
            layer_layer: graph.ranksep,
            group_padding_x: settings.namespace_padding,
            group_padding_y: settings.namespace_padding,
            ..Default::default()
        },
        options,
    })
}

#[cfg(feature = "layout-elk")]
fn class_layout_from_elk(
    model: &ClassDiagramModel,
    measured: MeasuredClassDiagram,
    elk_graph: &elk::Graph,
    layout: elk::LayoutResult,
    settings: ClassElkLayoutSettings<'_>,
    work: &mut Option<&mut OperationLayoutWorkControl>,
) -> Result<ClassDiagramLayout> {
    let frames = crate::elk_adapter::drawing_group_frames(
        elk_graph,
        &layout,
        |node| node.label.map_or(0.0, |label| label.width) + node.container.padding,
        work,
    )?;
    let namespace_ids = class_namespace_ids_in_decl_order(model);
    let namespace_set: HashSet<&str> = namespace_ids.iter().copied().collect();
    let source_node_by_id: HashMap<&str, &elk::Node> = elk_graph
        .nodes
        .iter()
        .map(|node| (node.id.as_str(), node))
        .collect();
    let source_edge_by_id: HashMap<&str, &elk::Edge> = elk_graph
        .edges
        .iter()
        .map(|edge| (edge.id.as_str(), edge))
        .collect();

    let mut nodes = Vec::with_capacity(layout.nodes.len());
    for node in layout.nodes {
        let Some(source) = source_node_by_id.get(node.id.as_str()).copied() else {
            return Err(Error::InvalidModel {
                message: format!("ELK layout returned unknown class node {}", node.id),
            });
        };
        let frame = frames.get(node.id.as_str());
        let label_metrics = measured
            .node_label_metrics_by_id
            .get(&node.id)
            .copied()
            .or_else(|| source.label.map(|label| (label.width, label.height)));
        nodes.push(LayoutNode {
            id: node.id,
            x: frame.map_or(node.x, |frame| frame.x),
            y: frame.map_or(node.y, |frame| frame.y),
            width: frame.map_or(node.width, |frame| frame.width),
            height: frame.map_or(node.height, |frame| frame.height),
            is_cluster: source.kind == elk::NodeKind::Group,
            label_width: label_metrics.map(|(width, _)| width),
            label_height: label_metrics.map(|(_, height)| height),
        });
    }

    let node_by_id: HashMap<&str, &LayoutNode> =
        nodes.iter().map(|node| (node.id.as_str(), node)).collect();
    let mut node_rect_by_id: HashMap<&str, Rect> = HashMap::new();
    for node in &nodes {
        node_rect_by_id.insert(
            node.id.as_str(),
            Rect::from_center(node.x, node.y, node.width, node.height),
        );
    }

    let edge_terminals_by_id = measured
        .graph
        .edges
        .iter()
        .filter_map(|((_, _, id), edge)| edge.terminals.as_ref().map(|meta| (id.as_str(), meta)))
        .collect::<HashMap<_, _>>();

    let mut edges = Vec::with_capacity(layout.edges.len());
    let mut placed_terminals = Vec::with_capacity(layout.edges.len());
    for edge in layout.edges {
        let Some(source) = source_edge_by_id.get(edge.id.as_str()).copied() else {
            return Err(Error::InvalidModel {
                message: format!("ELK layout returned unknown class edge {}", edge.id),
            });
        };
        let terminal_meta = edge_terminals_by_id.get(edge.id.as_str());
        if let Some(work) = work.as_deref_mut() {
            work.charge_adapter(
                edge.points
                    .len()
                    .saturating_add(edge.labels.len())
                    .saturating_add(2),
            )?;
        }
        let points = edge
            .points
            .into_iter()
            .map(|point| LayoutPoint {
                x: point.x,
                y: point.y,
            })
            .collect::<Vec<_>>();
        let label = source.label.and_then(|source_label| {
            edge.labels
                .iter()
                .find(|label| label.terminal.is_none())
                .map(|label| LayoutLabel {
                    x: label.x + label.width / 2.0,
                    y: label.y + label.height / 2.0,
                    width: label.width,
                    height: label.height,
                })
                .or_else(|| class_elk_edge_label_position(&points, source_label))
        });
        let terminal_points = if points.is_empty()
            && let (Some(from), Some(to)) = (
                node_by_id.get(source.source.as_str()),
                node_by_id.get(source.target.as_str()),
            ) {
            crate::elk_geometry::missing_rect_section_points(from, to)
        } else if let (Some(from), Some(to)) = (
            node_rect_by_id.get(source.source.as_str()).copied(),
            node_rect_by_id.get(source.target.as_str()).copied(),
        ) {
            terminal_path_for_edge(&points, from, to)
        } else {
            points.clone()
        };

        let mut placed = elk_terminals::PlacedTerminals {
            ports: points
                .first()
                .zip(points.last())
                .map(|(start, end)| (start.clone(), end.clone())),
            ..Default::default()
        };
        let routed_points = if !points.is_empty()
            && let (Some(start), Some(end)) = (
                node_by_id.get(source.source.as_str()),
                node_by_id.get(source.target.as_str()),
            ) {
            use crate::elk_edge_geometry::{self as geometry, Outline, Shape};
            let start_shape = Shape {
                node: start,
                outline: Outline::Rect,
                intersection: None,
            };
            let end_shape = Shape {
                node: end,
                outline: Outline::Rect,
                intersection: None,
            };
            let mut input = Vec::with_capacity(points.len() + 2);
            input.push(LayoutPoint {
                x: start.x,
                y: start.y,
            });
            input.extend_from_slice(&points);
            input.push(LayoutPoint { x: end.x, y: end.y });
            let mut clipped = geometry::sanitize(&input, start_shape, end_shape);
            if let Some(meta) = terminal_meta {
                geometry::marker_segment(&mut clipped, end_shape, meta.end_arrow_type, false);
                geometry::marker_segment(&mut clipped, start_shape, meta.start_arrow_type, true);
            }
            clipped
        } else {
            points
        };
        let mut out_edge = LayoutEdge {
            id: edge.id,
            from: source.source.clone(),
            to: source.target.clone(),
            from_cluster: node_by_id
                .get(source.source.as_str())
                .filter(|node| node.is_cluster)
                .map(|node| node.id.clone()),
            to_cluster: node_by_id
                .get(source.target.as_str())
                .filter(|node| node.is_cluster)
                .map(|node| node.id.clone()),
            points: routed_points,
            label,
            start_label_left: None,
            start_label_right: None,
            end_label_left: None,
            end_label_right: None,
            start_marker: None,
            end_marker: None,
            stroke_dasharray: None,
        };
        if let Some(meta) = terminal_meta {
            for terminal in &edge.labels {
                let Some(key) = terminal.terminal else {
                    continue;
                };
                if terminal.x == 0.0 && terminal.y == 0.0 {
                    continue;
                }
                let Some((width, height)) = class_terminal_metrics(meta, key) else {
                    continue;
                };
                *elk_terminals::label_slot(&mut out_edge, key) = Some(LayoutLabel {
                    x: terminal.x + terminal.width / 2.0,
                    y: terminal.y + terminal.height / 2.0,
                    width,
                    height,
                });
                placed.keys.push(key);
            }
            apply_class_terminal_labels(&mut out_edge, meta, &terminal_points)?;
        }
        edges.push(out_edge);
        placed_terminals.push(placed);
    }

    let mut charge = |units| match work.as_deref_mut() {
        Some(work) => work.charge_adapter(units),
        None => Ok(()),
    };
    if settings
        .effective_config
        .pointer("/elk/straightenEdges")
        .and_then(Value::as_bool)
        != Some(false)
    {
        crate::elk_terminal_jogs::straighten_edge_terminals(&mut edges, &mut charge)?;
    }
    for (edge, placed) in edges.iter_mut().zip(&placed_terminals) {
        charge(1)?;
        elk_terminals::follow_moved_endpoints(edge, placed);
        // Only unplaced/non-layered labels retain Mermaid's legacy path fallback.
        if !edge.points.is_empty() {
            for (key, position) in [
                (elk::TerminalLabelKey::StartLeft, TerminalPos::StartLeft),
                (elk::TerminalLabelKey::StartRight, TerminalPos::StartRight),
                (elk::TerminalLabelKey::EndLeft, TerminalPos::EndLeft),
                (elk::TerminalLabelKey::EndRight, TerminalPos::EndRight),
            ] {
                if !placed.keys.contains(&key) && elk_terminals::label(edge, key).is_some() {
                    let (x, y) =
                        required_terminal_label_position(&edge.id, 10.0, position, &edge.points)?;
                    if let Some(label) = elk_terminals::label_slot(edge, key) {
                        label.x = x;
                        label.y = y;
                    }
                }
            }
        }
    }
    elk_terminals::slide_off_frames(&mut edges, &placed_terminals, &nodes, &mut charge)?;
    elk_terminals::put_on_own_side(&mut edges, &placed_terminals, &nodes, &mut charge)?;
    crate::elk_terminal_jogs::separate_opposite_edge_labels(
        edges
            .iter_mut()
            .map(|edge| (edge.from.as_str(), edge.to.as_str(), edge.label.as_mut())),
        &mut charge,
    )?;

    let mut clusters = Vec::new();
    for &id in &namespace_ids {
        if !namespace_set.contains(id) {
            continue;
        }
        let Some(node) = node_by_id.get(id).copied() else {
            continue;
        };
        let title = class_namespace_label(model, id).to_string();
        let (title_width, title_height) = measured.namespace_label_metrics_by_id[id];
        let title_label = LayoutLabel {
            x: node.x,
            y: node.y - node.height / 2.0 + settings.title_margin_top + title_height / 2.0,
            width: title_width,
            height: title_height,
        };
        // The title minimum is reserved before layout and bounded by the provider frame.
        // Reapplying it here would grow a contracted cluster without updating its node bounds.
        let width = node.width;
        let diff = -settings.namespace_padding;
        clusters.push(LayoutCluster {
            id: id.to_string(),
            x: node.x,
            y: node.y,
            width,
            height: node.height,
            diff,
            offset_y: title_height - settings.namespace_padding / 2.0,
            title,
            title_label,
            requested_dir: None,
            effective_dir: normalize_dir(&model.direction),
            padding: settings.namespace_padding,
            title_margin_top: settings.title_margin_top,
            title_margin_bottom: settings.title_margin_bottom,
        });
    }

    let namespace_order: HashMap<&str, usize> = namespace_ids
        .iter()
        .copied()
        .enumerate()
        .map(|(idx, id)| (id, idx))
        .collect();
    clusters.sort_by(|a, b| {
        namespace_order
            .get(a.id.as_str())
            .copied()
            .unwrap_or(usize::MAX)
            .cmp(
                &namespace_order
                    .get(b.id.as_str())
                    .copied()
                    .unwrap_or(usize::MAX),
            )
            .then_with(|| a.id.cmp(&b.id))
    });

    let bounds = compute_bounds(&nodes, &edges, &clusters);
    let render_tree = ClassRenderTree {
        roots: vec![ClassRenderRoot {
            namespace_id: None,
            cluster_ids: clusters.iter().map(|cluster| cluster.id.clone()).collect(),
            edge_ids: edges.iter().map(|edge| edge.id.clone()).collect(),
            items: nodes
                .iter()
                .filter(|node| !node.is_cluster)
                .map(|node| ClassRenderItem::Node(node.id.clone()))
                .collect(),
        }],
        top: ClassRenderRootId(0),
    };
    Ok(ClassDiagramLayout {
        nodes,
        edges,
        clusters,
        bounds,
        uses_elk_adapter_dom: true,
        class_label_plans_by_id: measured.class_label_plans_by_id,
        render_tree,
    })
}

#[cfg(feature = "layout-elk")]
fn class_terminal_metrics(
    meta: &EdgeTerminalMetrics,
    key: elk::TerminalLabelKey,
) -> Option<(f64, f64)> {
    match key {
        elk::TerminalLabelKey::StartLeft => meta.start_left,
        elk::TerminalLabelKey::StartRight => meta.start_right,
        elk::TerminalLabelKey::EndLeft => meta.end_left,
        elk::TerminalLabelKey::EndRight => meta.end_right,
    }
}

fn required_terminal_label_position(
    edge_id: &str,
    marker: f64,
    position: TerminalPos,
    points: &[LayoutPoint],
) -> Result<(f64, f64)> {
    // Mermaid's calculatePoint throws when the route cannot supply the sample distance.
    calc_terminal_label_position(marker, position, points).ok_or_else(|| Error::InvalidModel {
        message: format!(
            "Class edge {edge_id}: Could not find a suitable point for the given distance"
        ),
    })
}

fn apply_class_terminal_labels(
    edge: &mut LayoutEdge,
    meta: &EdgeTerminalMetrics,
    points: &[LayoutPoint],
) -> Result<()> {
    for (slot, metrics, marker, position) in [
        (
            &mut edge.start_label_left,
            meta.start_left,
            meta.start_marker,
            TerminalPos::StartLeft,
        ),
        (
            &mut edge.start_label_right,
            meta.start_right,
            meta.start_marker,
            TerminalPos::StartRight,
        ),
        (
            &mut edge.end_label_left,
            meta.end_left,
            meta.end_marker,
            TerminalPos::EndLeft,
        ),
        (
            &mut edge.end_label_right,
            meta.end_right,
            meta.end_marker,
            TerminalPos::EndRight,
        ),
    ] {
        if slot.is_none()
            && let Some((width, height)) = metrics
        {
            let (x, y) = required_terminal_label_position(&edge.id, marker, position, points)?;
            *slot = Some(LayoutLabel {
                x,
                y,
                width,
                height,
            });
        }
    }
    Ok(())
}

#[cfg(feature = "layout-elk")]
fn class_elk_edge_label_position(points: &[LayoutPoint], label: elk::Label) -> Option<LayoutLabel> {
    calculate_point(points, class_elk_polyline_len(points) / 2.0).map(|point| LayoutLabel {
        x: point.x,
        y: point.y,
        width: label.width,
        height: label.height,
    })
}

#[cfg(feature = "layout-elk")]
fn class_elk_polyline_len(points: &[LayoutPoint]) -> f64 {
    points
        .windows(2)
        .map(|pair| (pair[1].x - pair[0].x).hypot(pair[1].y - pair[0].y))
        .sum::<f64>()
}

fn compute_bounds(
    nodes: &[LayoutNode],
    edges: &[LayoutEdge],
    clusters: &[LayoutCluster],
) -> Option<Bounds> {
    let mut points: Vec<(f64, f64)> = Vec::new();

    for c in clusters {
        let r = Rect::from_center(c.x, c.y, c.width, c.height);
        points.push((r.min_x(), r.min_y()));
        points.push((r.max_x(), r.max_y()));
        let lr = Rect::from_center(
            c.title_label.x,
            c.title_label.y,
            c.title_label.width,
            c.title_label.height,
        );
        points.push((lr.min_x(), lr.min_y()));
        points.push((lr.max_x(), lr.max_y()));
    }

    for n in nodes {
        let r = Rect::from_center(n.x, n.y, n.width, n.height);
        points.push((r.min_x(), r.min_y()));
        points.push((r.max_x(), r.max_y()));
    }

    for e in edges {
        for p in &e.points {
            points.push((p.x, p.y));
        }
        for l in [
            e.label.as_ref(),
            e.start_label_left.as_ref(),
            e.start_label_right.as_ref(),
            e.end_label_left.as_ref(),
            e.end_label_right.as_ref(),
        ]
        .into_iter()
        .flatten()
        {
            let r = Rect::from_center(l.x, l.y, l.width, l.height);
            points.push((r.min_x(), r.min_y()));
            points.push((r.max_x(), r.max_y()));
        }
    }

    Bounds::from_points(points)
}

#[cfg(test)]
mod tests {
    use dugong::graphlib::{Graph, GraphOptions};
    use dugong::{EdgeLabel, GraphLabel, NodeLabel};
    use merman_core::models::class_diagram::Namespace;
    use merman_core::{Engine, ParseOptions, RenderSemanticModel};

    use crate::text::{DeterministicTextMeasurer, TextMeasurer, TextMetrics, TextStyle, WrapMode};

    #[test]
    fn class_member_display_preserves_authored_colon_spaces_and_classifiers() {
        let parsed = Engine::new()
            .parse_diagram_for_render_model_sync(
                "classDiagram\nclass Box {\n+left :String\n+right: String\n+both : String$\n+tight:String\n+convert(value: String) Result\n}",
                ParseOptions::default(),
            )
            .expect("parse Class member whitespace")
            .expect("detect Class diagram");
        let RenderSemanticModel::Class(model) = parsed.model() else {
            panic!("expected Class model");
        };
        let class = &model.classes["Box"];
        let members = class
            .members
            .iter()
            .map(super::class_member_display_text)
            .collect::<Vec<_>>();
        assert_eq!(
            members,
            [
                "+left :String",
                "+right: String",
                "+both : String",
                "+tight:String"
            ]
        );
        assert_eq!(class.members[2].classifier, "$");
        assert_eq!(class.members[2].css_style, "text-decoration:underline;");
        assert_eq!(
            super::class_member_display_text(&class.methods[0]),
            "+convert(value: String) : Result"
        );
        assert_eq!(
            super::class_member_create_text_input(&class.members[1]),
            "+right: String"
        );
    }

    #[test]
    fn class_dagre_debug_input_uses_the_production_graph_and_source_identity_order() {
        let source =
            include_str!("../../../fixtures/class/stress_class_many_relations_labels_020.mmd");
        let parsed = Engine::new()
            .parse_diagram_for_render_model_sync(source, ParseOptions::default())
            .expect("parse Class fixture")
            .expect("detect Class fixture");
        let RenderSemanticModel::Class(model) = parsed.model() else {
            panic!("expected Class render model");
        };
        let graph = super::debug_build_class_diagram_dagre_graph(
            model,
            &parsed.metadata().effective_config,
            &DeterministicTextMeasurer::default(),
        )
        .expect("build Class Dagre input");

        assert_eq!(graph.node_ids(), ["A", "B", "C", "D", "E"]);
        assert!(graph.node_ids().into_iter().all(|id| {
            graph
                .node(&id)
                .is_some_and(|node| node.x.is_none() && node.y.is_none())
        }));
        assert_eq!(
            graph
                .edge_keys()
                .into_iter()
                .map(|edge| edge.name.expect("named Class edge"))
                .collect::<Vec<_>>(),
            ["0", "1", "2", "3", "4", "5", "6", "7"]
        );
    }

    #[cfg(feature = "layout-elk")]
    fn class_elk_graph_for_test(source: &str) -> super::elk::Graph {
        let parsed = Engine::new()
            .parse_diagram_for_render_model_sync(source, ParseOptions::default())
            .expect("parse Class source")
            .expect("detect Class source");
        let RenderSemanticModel::Class(model) = parsed.model() else {
            panic!("expected Class model");
        };
        let config = &parsed.metadata().effective_config;
        let settings = super::ClassConfigView::new(config.as_value()).layout_settings();
        let measured = super::measure_class_diagram(
            model,
            config,
            &DeterministicTextMeasurer::default(),
            None,
            &settings,
            true,
        )
        .expect("measure Class diagram");
        super::class_measured_to_elk_graph(
            &measured,
            &super::ClassElkLayoutSettings {
                namespace_padding: settings.class_padding,
                title_margin_top: settings.title_margin_top,
                title_margin_bottom: settings.title_margin_bottom,
                effective_config: config.as_value(),
            },
            &mut None,
        )
        .expect("Class ELK graph")
    }

    #[cfg(feature = "layout-elk")]
    #[test]
    fn class_elk_interfaces_remain_root_nodes_outside_nested_namespaces() {
        for relation in ["A --() ProvidedInterface", "ProvidedInterface ()-- A"] {
            let graph = class_elk_graph_for_test(&format!(
                "classDiagram\nnamespace Outer {{\nnamespace Inner {{\nclass A\n}}\n}}\n{relation}\n"
            ));
            let node = |id: &str| graph.nodes.iter().find(|node| node.id == id).expect("node");
            assert_eq!(node("A").parent.as_deref(), Some("Outer.Inner"));
            assert_eq!(node("Outer.Inner").parent.as_deref(), Some("Outer"));
            assert_eq!(node("interface0").parent, None);
            assert!(graph.edges.iter().any(|edge| {
                (edge.source == "A" && edge.target == "interface0")
                    || (edge.source == "interface0" && edge.target == "A")
            }));
        }
    }

    #[cfg(feature = "layout-elk")]
    #[test]
    fn class_elk_cycle_entry_follows_relations_independently_of_node_declarations() {
        for declarations in ["class B\nclass A\nclass C", "class C\nclass B\nclass A"] {
            for enabled in [false, true] {
                let graph = class_elk_graph_for_test(&format!(
                    "---\nconfig:\n  elk:\n    keepEntryNodeOnTop: {enabled}\n    cycleBreakingStrategy: GREEDY_MODEL_ORDER\n---\nclassDiagram\n{declarations}\nA --> B\nB --> C\nC --> A\n"
                ));
                let constrained = graph
                    .nodes
                    .iter()
                    .filter(|node| {
                        node.layer_constraint == Some(super::elk::LayerConstraint::First)
                    })
                    .map(|node| node.id.as_str())
                    .collect::<Vec<_>>();
                assert_eq!(constrained, if enabled { vec!["A"] } else { vec![] });
            }
        }
    }

    #[cfg(feature = "layout-elk")]
    #[test]
    fn class_elk_interface_padding_preserves_measured_label_bounds_in_both_modes() {
        for look in ["classic", "neo"] {
            for html in [false, true] {
                for font_size in [14, 23] {
                    let source = format!(
                        "---\nconfig:\n  look: {look}\n  htmlLabels: {html}\n  themeVariables:\n    fontSize: {font_size}px\n---\nclassDiagram\nA --() CompleteInterface\n"
                    );
                    let parsed = Engine::new()
                        .parse_diagram_for_render_model_sync(&source, ParseOptions::default())
                        .unwrap()
                        .unwrap();
                    let RenderSemanticModel::Class(model) = parsed.model() else {
                        panic!("Class model")
                    };
                    let config = &parsed.metadata().effective_config;
                    let layout_settings =
                        super::ClassConfigView::new(config.as_value()).layout_settings();
                    let measured = super::measure_class_diagram(
                        model,
                        config,
                        &DeterministicTextMeasurer::default(),
                        None,
                        &layout_settings,
                        true,
                    )
                    .unwrap();
                    let (label_width, label_height) =
                        measured.node_label_metrics_by_id["interface0"];
                    let (padding_x, padding_y) = if look == "neo" {
                        (32.0, 24.0)
                    } else {
                        (0.0, 0.0)
                    };
                    let settings = super::ClassElkLayoutSettings {
                        namespace_padding: layout_settings.class_padding,
                        title_margin_top: layout_settings.title_margin_top,
                        title_margin_bottom: layout_settings.title_margin_bottom,
                        effective_config: config.as_value(),
                    };
                    let graph = super::class_measured_to_elk_graph(&measured, &settings, &mut None)
                        .unwrap();
                    let interface = graph
                        .nodes
                        .iter()
                        .find(|node| node.id == "interface0")
                        .unwrap();
                    assert!((interface.width - label_width - padding_x).abs() < 1e-6);
                    assert!((interface.height - label_height - padding_y).abs() < 1e-6);
                    let provider = super::elk::LayoutResult {
                        nodes: graph
                            .nodes
                            .iter()
                            .map(|node| super::elk::NodeLayout {
                                id: node.id.clone(),
                                x: 0.0,
                                y: 0.0,
                                width: node.width,
                                height: node.height,
                            })
                            .collect(),
                        edges: Vec::new(),
                    };
                    let layout = super::class_layout_from_elk(
                        model, measured, &graph, provider, settings, &mut None,
                    )
                    .unwrap();
                    let painted = layout
                        .nodes
                        .iter()
                        .find(|node| node.id == "interface0")
                        .unwrap();
                    assert_eq!(painted.label_width, Some(label_width));
                    assert_eq!(painted.label_height, Some(label_height));
                    assert_eq!(
                        (painted.width, painted.height),
                        (interface.width, interface.height)
                    );
                }
            }
        }
    }

    #[cfg(feature = "layout-elk")]
    #[test]
    fn class_elk_contracts_namespace_frame_without_rebasing_provider_routes() {
        for title_width in [32.0, 300.0] {
            let parsed = Engine::new()
                .parse_diagram_for_render_model_sync(
                    "classDiagram\nnamespace Frame {\nclass A\n}\nA --> A : loop\n",
                    ParseOptions::default(),
                )
                .unwrap()
                .unwrap();
            let RenderSemanticModel::Class(model) = parsed.model() else {
                panic!("Class model")
            };
            let config = &parsed.metadata().effective_config;
            let layout_settings = super::ClassConfigView::new(config.as_value()).layout_settings();
            let mut measured = super::measure_class_diagram(
                model,
                config,
                &DeterministicTextMeasurer::default(),
                None,
                &layout_settings,
                true,
            )
            .unwrap();
            measured
                .namespace_label_metrics_by_id
                .get_mut("Frame")
                .unwrap()
                .0 = title_width;
            let settings = super::ClassElkLayoutSettings {
                namespace_padding: layout_settings.class_padding,
                title_margin_top: layout_settings.title_margin_top,
                title_margin_bottom: layout_settings.title_margin_bottom,
                effective_config: config.as_value(),
            };
            let graph =
                super::class_measured_to_elk_graph(&measured, &settings, &mut None).unwrap();
            let provider = super::elk::LayoutResult {
                nodes: vec![
                    super::elk::NodeLayout {
                        id: "Frame".to_string(),
                        x: 100.0,
                        y: 74.0,
                        width: 200.0,
                        height: 148.0,
                    },
                    super::elk::NodeLayout {
                        id: "A".to_string(),
                        x: 90.0,
                        y: 86.0,
                        width: 100.0,
                        height: 76.0,
                    },
                ],
                edges: vec![super::elk::EdgeLayout {
                    id: graph.edges[0].id.clone(),
                    points: vec![
                        super::elk::Point { x: 40.0, y: 60.0 },
                        super::elk::Point { x: 140.0, y: 70.0 },
                    ],
                    labels: vec![super::elk::EdgeLabelLayout {
                        terminal: None,
                        x: 100.0,
                        y: 20.0,
                        width: 30.0,
                        height: 20.0,
                    }],
                }],
            };
            let layout = super::class_layout_from_elk(
                model, measured, &graph, provider, settings, &mut None,
            )
            .unwrap();
            let frame = &layout.clusters[0];
            let expected = if title_width < 200.0 {
                (90.0, 74.0, 148.0, 148.0)
            } else {
                (100.0, 74.0, 200.0, 148.0)
            };
            assert_eq!((frame.x, frame.y, frame.width, frame.height), expected);
            let node = layout.nodes.iter().find(|node| node.id == "Frame").unwrap();
            assert_eq!((node.x, node.y, node.width, node.height), expected);
            assert_eq!(
                layout.edges[0]
                    .points
                    .iter()
                    .map(|p| (p.x, p.y))
                    .collect::<Vec<_>>(),
                [(40.0, 60.0), (140.0, 70.0)]
            );
            let label = layout.edges[0].label.as_ref().unwrap();
            assert_eq!((label.x, label.y), (115.0, 30.0));
        }
    }

    #[cfg(feature = "layout-elk")]
    #[test]
    fn class_elk_terminal_provider_padding_only_reserves_real_markers() {
        use super::elk::TerminalLabelKey;
        let plain = class_elk_graph_for_test("classDiagram\nA \"many\" -- \"many\" B\n");
        let marked = class_elk_graph_for_test("classDiagram\nA \"many\" <|-- \"many\" B\n");
        assert_eq!(plain.edges[0].terminal_labels.len(), 2);
        for key in [TerminalLabelKey::StartRight, TerminalLabelKey::EndLeft] {
            let before = plain.edges[0]
                .terminal_labels
                .iter()
                .find(|label| label.key == key)
                .unwrap()
                .label;
            let after = marked.edges[0]
                .terminal_labels
                .iter()
                .find(|label| label.key == key)
                .unwrap()
                .label;
            let padding = if key.at_start() { 12.0 } else { 0.0 };
            assert!((after.width - before.width - padding).abs() < 1e-9);
            assert!((after.height - before.height - padding).abs() < 1e-9);
            assert_eq!(before.height, 16.5, "measured terminal CSS font is 11px");
        }
        let non_layered = class_elk_graph_for_test(
            "---\nconfig:\n  layout: elk.mrtree\n---\nclassDiagram\nA \"many\" --> \"one\" B\n",
        );
        assert!(non_layered.edges[0].terminal_labels.is_empty());
    }

    #[cfg(feature = "layout-elk")]
    #[test]
    fn class_elk_keeps_the_source_default_nonzero_seed() {
        assert_eq!(
            super::class_elk_layout_options(&serde_json::Value::Null)
                .layered
                .random_seed,
            1
        );
    }

    #[cfg(feature = "layout-elk")]
    #[test]
    fn class_elk_zero_seed_adapter_graph_requires_an_operation_seed() {
        use std::num::NonZeroU64;

        let parsed = Engine::new()
            .parse_diagram_for_render_model_sync("classDiagram\nA --> B", ParseOptions::default())
            .expect("parse Class source")
            .expect("detect Class source");
        let RenderSemanticModel::Class(model) = parsed.model() else {
            panic!("expected Class model");
        };
        let config = &parsed.metadata().effective_config;
        let layout_settings = super::ClassConfigView::new(config.as_value()).layout_settings();
        let measured = super::measure_class_diagram(
            model,
            config,
            &DeterministicTextMeasurer::default(),
            None,
            &layout_settings,
            true,
        )
        .expect("measure Class diagram");
        let settings = super::ClassElkLayoutSettings {
            namespace_padding: 8.0,
            title_margin_top: 0.0,
            title_margin_bottom: 0.0,
            effective_config: &serde_json::Value::Null,
        };
        let mut graph = super::class_measured_to_elk_graph(&measured, &settings, &mut None)
            .expect("Class ELK graph");
        graph.options.layered.random_seed = 0;

        assert!(super::elk::layout(&graph).is_err());

        let operation_seed = super::elk::ElkOperationSeed::from_operation_seed(
            NonZeroU64::new(0x636c_6173_7365_6c6b).expect("nonzero operation seed"),
        );
        let first = super::elk::layout_with_operation_seed(&graph, operation_seed)
            .expect("seeded Class layout");
        let replayed = super::elk::layout_with_operation_seed(&graph, operation_seed)
            .expect("replayed seeded Class layout");

        assert_eq!(first, replayed);
    }

    struct ClassProbeMeasurer;

    struct ClassPrecisionMeasurer;

    struct ClassMathGeometryMeasurer;

    #[derive(Debug)]
    struct ClassMathGeometryRenderer;

    impl TextMeasurer for ClassProbeMeasurer {
        fn measure(&self, _text: &str, _style: &TextStyle) -> TextMetrics {
            TextMetrics {
                width: 0.0,
                height: 0.0,
                line_count: 1,
            }
        }

        fn measure_svg_simple_text_bbox_width_px(&self, _text: &str, style: &TextStyle) -> f64 {
            if style.font_family.as_deref() == Some("sans-serif") {
                120.0
            } else {
                80.0
            }
        }

        fn measure_svg_simple_text_bbox_height_px(&self, _text: &str, _style: &TextStyle) -> f64 {
            17.0
        }
    }

    impl TextMeasurer for ClassPrecisionMeasurer {
        fn measure(&self, _text: &str, _style: &TextStyle) -> TextMetrics {
            TextMetrics {
                width: 73.123_456_789,
                height: 17.25,
                line_count: 1,
            }
        }

        fn measure_svg_text_bbox_x(&self, _text: &str, _style: &TextStyle) -> (f64, f64) {
            (31.0, 42.123_456_789)
        }

        fn measure_svg_tspan_text_bbox_width_px(&self, _text: &str, _style: &TextStyle) -> f64 {
            panic!("formatted Class labels must not use the single-tspan operation")
        }
    }

    impl TextMeasurer for ClassMathGeometryMeasurer {
        fn measure(&self, _text: &str, _style: &TextStyle) -> TextMetrics {
            TextMetrics {
                width: 29.0,
                height: 19.0,
                line_count: 1,
            }
        }

        fn measure_svg_tspan_text_bbox_width_px(&self, _text: &str, _style: &TextStyle) -> f64 {
            17.0
        }
    }

    impl crate::math::MathRenderer for ClassMathGeometryRenderer {
        fn render_html_label(
            &self,
            text: &str,
            _config: &merman_core::MermaidConfig,
        ) -> Option<String> {
            text.contains("$$").then(|| text.to_string())
        }

        fn measure_html_label(
            &self,
            text: &str,
            _config: &merman_core::MermaidConfig,
            _style: &TextStyle,
            _max_width_px: Option<f64>,
            _wrap_mode: WrapMode,
        ) -> Option<TextMetrics> {
            text.contains("$$").then_some(TextMetrics {
                width: 123.0,
                height: 37.0,
                line_count: 1,
            })
        }
    }

    fn default_style() -> TextStyle {
        TextStyle {
            font_family: Some("\"trebuchet ms\", verdana, arial, sans-serif".to_string()),
            font_size: 16.0,
            font_weight: None,
            font_style: None,
        }
    }

    fn class_model_with_namespace_chain(count: usize) -> super::ClassDiagramModel {
        let mut model: super::ClassDiagramModel = serde_json::from_value(serde_json::json!({
            "type": "classDiagram",
            "direction": "TB",
            "classes": {},
            "constants": {
                "lineType": { "line": 0, "dottedLine": 1 },
                "relationType": {
                    "none": 0,
                    "aggregation": 1,
                    "extension": 2,
                    "composition": 3,
                    "dependency": 4,
                    "lollipop": 5
                }
            }
        }))
        .expect("minimal Class diagram model");
        for index in 0..count {
            let id = format!("ns{index}");
            model.namespaces.insert(
                id.clone(),
                Namespace {
                    id,
                    label: String::new(),
                    dom_id: format!("classId-namespace-{index}"),
                    class_ids: Vec::new(),
                    note_ids: Vec::new(),
                    parent: index.checked_sub(1).map(|parent| format!("ns{parent}")),
                    explicit: true,
                },
            );
        }
        model
    }

    fn class_candidate_test_graph(
        edges: &[(&str, &str)],
    ) -> Graph<NodeLabel, EdgeLabel, GraphLabel> {
        let mut graph = Graph::new(GraphOptions {
            directed: true,
            multigraph: true,
            compound: true,
        });
        graph.set_graph(GraphLabel::default());
        for id in [
            "root",
            "a",
            "b",
            "leaf",
            "sibling",
            "outside",
            "outside_child",
        ] {
            graph.set_node(id, NodeLabel::default());
        }
        for (child, parent) in [
            ("a", "root"),
            ("b", "a"),
            ("leaf", "b"),
            ("sibling", "root"),
            ("outside_child", "outside"),
        ] {
            graph.set_parent(child, parent);
        }
        for (index, (from, to)) in edges.iter().copied().enumerate() {
            graph.set_edge_named(
                from,
                to,
                Some(index.to_string()),
                Some(EdgeLabel::default()),
            );
        }
        graph
    }

    fn reference_class_cluster_candidates(
        graph: &Graph<NodeLabel, EdgeLabel, GraphLabel>,
    ) -> Vec<String> {
        let is_strict_descendant = |node: &str, ancestor: &str| {
            let mut parent = graph.parent(node);
            while let Some(id) = parent {
                if id == ancestor {
                    return true;
                }
                parent = graph.parent(id);
            }
            false
        };
        graph
            .nodes()
            .filter(|id| graph.children_iter(id).next().is_some())
            .filter(|id| {
                graph.edges().all(|edge| {
                    edge.v == *id
                        || edge.w == *id
                        || is_strict_descendant(&edge.v, id) == is_strict_descendant(&edge.w, id)
                })
            })
            .map(str::to_string)
            .collect()
    }

    #[test]
    fn class_cluster_boundary_index_matches_mermaid_descendant_semantics() {
        let ids = [
            "root",
            "a",
            "b",
            "leaf",
            "sibling",
            "outside",
            "outside_child",
        ];
        let mut edge_sets = vec![Vec::new()];
        for from in ids {
            for to in ids {
                edge_sets.push(vec![(from, to)]);
            }
        }
        edge_sets.push(vec![
            ("leaf", "sibling"),
            ("a", "leaf"),
            ("outside_child", "b"),
        ]);

        for edges in edge_sets {
            let graph = class_candidate_test_graph(&edges);
            assert_eq!(
                super::class_cluster_candidates(&graph),
                reference_class_cluster_candidates(&graph),
                "edges={edges:?}"
            );
        }
    }

    #[test]
    fn class_cluster_boundary_index_handles_max_size_namespace_chain() {
        const NODE_COUNT: usize = 4_000;
        let mut graph = Graph::new(GraphOptions {
            directed: true,
            multigraph: true,
            compound: true,
        });
        graph.set_graph(GraphLabel::default());
        for index in 0..NODE_COUNT {
            graph.set_node(format!("n{index}"), NodeLabel::default());
        }
        for index in (1..NODE_COUNT).rev() {
            graph.set_parent(format!("n{index}"), format!("n{}", index - 1));
        }

        let candidates = super::class_cluster_candidates(&graph);

        assert_eq!(candidates.len(), NODE_COUNT - 1);
        assert_eq!(candidates.first().map(String::as_str), Some("n0"));
        let expected_last = format!("n{}", NODE_COUNT - 2);
        assert_eq!(
            candidates.last().map(String::as_str),
            Some(expected_last.as_str())
        );
    }

    #[test]
    fn class_namespace_hierarchy_accepts_the_shared_depth_boundary() {
        let model = class_model_with_namespace_chain(merman_core::MAX_DIAGRAM_NESTING_DEPTH + 1);

        super::validate_class_namespace_hierarchy(&model)
            .expect("the shared maximum namespace nesting depth is valid");
    }

    #[test]
    fn class_namespace_hierarchy_rejects_excessive_depth_without_recursion() {
        let model = class_model_with_namespace_chain(merman_core::MAX_DIAGRAM_NESTING_DEPTH + 2);

        let error = super::validate_class_namespace_hierarchy(&model)
            .expect_err("namespace nesting beyond the shared limit must be rejected");

        assert_eq!(
            error.to_string(),
            format!(
                "invalid semantic model: class namespace nesting depth exceeds {} at ns{}",
                merman_core::MAX_DIAGRAM_NESTING_DEPTH,
                merman_core::MAX_DIAGRAM_NESTING_DEPTH + 1
            )
        );
    }

    #[test]
    fn class_namespace_hierarchy_rejects_parent_cycles() {
        let mut model = class_model_with_namespace_chain(3);
        model.namespaces["ns0"].parent = Some("ns2".to_string());

        let error = super::validate_class_namespace_hierarchy(&model)
            .expect_err("namespace parent cycles must be rejected");

        assert!(error.to_string().contains("namespace parent cycle"));
    }

    #[test]
    fn class_create_text_width_uses_shared_mermaid_dimensions() {
        let style = TextStyle {
            font_family: Some("Arial".to_string()),
            font_size: 16.0,
            font_weight: None,
            font_style: None,
        };

        assert_eq!(
            super::class_html_create_text_width_px("unseen label", &ClassProbeMeasurer, &style),
            130
        );
    }

    #[test]
    fn class_raw_code_and_anchor_metrics_measure_the_rendered_dom() {
        let measurer = DeterministicTextMeasurer::default();
        let style = default_style();
        let source = "<a href='https://example.com'><code>Entity</code></a>";
        let fragment = crate::text::mermaid_markdown_to_xhtml_label_fragment(source, true);
        let expected = crate::text::measure_html_with_inline_styles(
            &measurer,
            &fragment,
            &style,
            Some(500.0),
            WrapMode::HtmlLike,
        );

        let actual = super::class_html_measure_label_metrics(&measurer, &style, source, 500, "");
        let literal = measurer.measure_wrapped(source, &style, Some(500.0), WrapMode::HtmlLike);

        assert_eq!(actual.width, expected.width);
        assert_eq!(actual.height, expected.height);
        assert_eq!(actual.line_count, expected.line_count);
        assert!(
            actual.width < literal.width,
            "actual={actual:?}, literal={literal:?}"
        );
    }

    #[test]
    fn class_plain_underscore_label_preserves_host_precision() {
        let metrics = super::class_html_measure_label_metrics(
            &ClassPrecisionMeasurer,
            &default_style(),
            "driver_license",
            200,
            "",
        );

        assert_eq!(metrics.width, 73.123_456_789);
        assert_eq!(metrics.line_count, 1);
    }

    #[test]
    fn class_svg_plain_underscore_uses_formatted_text_bbox_precision() {
        assert_eq!(
            super::class_svg_single_line_plain_label_width_px(
                "driver_license",
                &ClassPrecisionMeasurer,
                &default_style(),
            ),
            Some(73.123_456_789)
        );
    }

    #[test]
    fn class_svg_formatted_bbox_uses_semantic_markdown_analysis() {
        let style = default_style();
        for literal in [
            "driver_license",
            "*unclosed",
            "literal ` backtick",
            "plain title",
        ] {
            assert_eq!(
                super::class_svg_single_line_plain_label_width_px(
                    literal,
                    &ClassPrecisionMeasurer,
                    &style,
                ),
                Some(73.123_456_789),
                "literal={literal:?}"
            );
        }

        for non_plain_single_line in ["*emphasis*", "__strong__", "first<br/>second"] {
            assert_eq!(
                super::class_svg_single_line_plain_label_width_px(
                    non_plain_single_line,
                    &ClassPrecisionMeasurer,
                    &style,
                ),
                None,
                "label={non_plain_single_line:?}"
            );
        }
    }

    #[test]
    fn class_svg_math_title_keeps_math_renderer_width_in_box_geometry() {
        let node: super::ClassNode = serde_json::from_value(serde_json::json!({
            "id": "Formula",
            "label": "Formula",
            "text": "$$x^2$$",
            "domId": "classId-Formula-0"
        }))
        .expect("Class node");
        let config = merman_core::MermaidConfig::default();
        let text_style = default_style();
        let context = super::ClassBoxMeasureCtx {
            measurer: &ClassMathGeometryMeasurer,
            mermaid_config: &config,
            math_renderer: Some(&ClassMathGeometryRenderer),
            text_style: &text_style,
            html_calc_text_style: &text_style,
            wrap_probe_font_size: 10.0,
            wrap_mode: WrapMode::SvgLike,
            padding: 8.0,
            hide_empty_members_box: true,
            capture_row_metrics: false,
        };

        let (width, _, _) = super::class_box_dimensions(&node, &context);

        assert_eq!(width, 139.0);
    }

    #[test]
    fn class_svg_math_note_keeps_math_renderer_width_in_note_geometry() {
        let config = merman_core::MermaidConfig::default();
        let style = default_style();

        let (width, height, metrics) = super::note_dimensions(
            "$$x^2$$",
            &ClassMathGeometryMeasurer,
            &style,
            WrapMode::SvgLike,
            8.0,
            Some(&config),
            Some(&ClassMathGeometryRenderer),
        );

        assert_eq!(metrics.width, 123.0);
        assert_eq!(metrics.height, 37.0);
        assert_eq!(width, 131.0);
        assert_eq!(height, 45.0);
    }
}
