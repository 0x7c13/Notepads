//! Mermaid's shared Dagre cluster extraction specialized to Usecase's one-level boundaries.
//!
//! Source: `dagre/mermaid-graphlib.js`, `dagre/index.js`, `clusters.js`, and `nodes.ts`.
//! Boundary IDs cannot be relationship endpoints and boundaries cannot nest, so no general
//! cluster-anchor rewriting or recursive arena is needed for this native family.

use super::*;
use crate::Error;
use crate::layout_work::OperationLayoutWorkControl;
use crate::model::{LayoutLabel, LayoutPoint};
use dugong::graphlib::{Graph, GraphOptions};
use dugong::{EdgeLabel, GraphLabel, NodeLabel, RankDir};
use std::collections::{HashMap, HashSet};

type DagreGraph = Graph<NodeLabel, EdgeLabel, GraphLabel>;

/// Usecase retains the three shared-Dagre segments; it is absent from the source merge allowlist.
pub(super) fn expand_self_loops(
    nodes: &mut Vec<UsecaseNodePlan>,
    edges: &mut Vec<UsecaseEdgePlan>,
    config: &Value,
    work: &mut OperationLayoutWorkControl,
) -> Result<()> {
    work.charge_adapter(work.checked_add(nodes.len(), edges.len())?)?;
    if !edges.iter().any(|edge| edge.source == edge.target) {
        return Ok(());
    }
    let parents: HashMap<_, _> = nodes
        .iter()
        .map(|node| (node.id.clone(), node.parent.clone()))
        .collect();
    let mut helpers = HashSet::new();
    let mut projected = indexmap::IndexMap::new();
    for edge in edges.drain(..) {
        work.charge_adapter(1)?;
        if edge.source != edge.target {
            projected.insert(edge.id.clone(), edge);
            continue;
        }
        let helper_ids = [
            format!("{}---{}---1", edge.source, edge.source),
            format!("{}---{}---2", edge.source, edge.source),
        ];
        for id in &helper_ids {
            if helpers.insert(id.clone()) {
                work.charge_adapter(1)?;
                nodes.push(UsecaseNodePlan {
                    id: id.clone(),
                    parent: parents[&edge.source].clone(),
                    source_label: String::new(),
                    label: UsecaseLabelPlan {
                        math_html: None,
                        text: String::new(),
                        label_type: UsecaseLabelType::Text,
                        metrics: TextMetrics {
                            width: 0.0,
                            height: 0.0,
                            line_count: 1,
                        },
                        style: text_style(config, None),
                        max_width: Some(10.0),
                        styles: indexmap::IndexMap::new(),
                    },
                    stereotype: None,
                    folded_stereotype: false,
                    // labelRect replaces the initial 10x10 layout hint with its tiny drawn rect.
                    width: 0.1,
                    height: 0.1,
                    is_boundary: false,
                    package: false,
                    ellipse: false,
                    table: None,
                    dagre_helper: true,
                });
            }
        }
        let mut first = edge.clone();
        first.id = format!("{}-cyclic-special-1", edge.source);
        first.original_id = Some(edge.id.clone());
        first.self_loop_node = Some(edge.source.clone());
        first.target = helper_ids[0].clone();
        first.label = None;
        first.end_marker = None;
        let mut middle = edge.clone();
        middle.id = format!("{}-cyclic-special-mid", edge.source);
        middle.original_id = Some(edge.id.clone());
        middle.self_loop_node = Some(edge.source.clone());
        middle.source = helper_ids[0].clone();
        middle.target = helper_ids[1].clone();
        middle.start_marker = None;
        middle.end_marker = None;
        let mut last = edge;
        last.original_id = Some(last.id.clone());
        last.self_loop_node = Some(last.source.clone());
        last.id = format!("{}-cyclic-special-2", last.source);
        last.source = helper_ids[1].clone();
        last.label = None;
        last.start_marker = None;
        // Source Graphlib keys are node-scoped: a later parallel self-loop replaces this triple.
        for segment in [first, middle, last] {
            projected.insert(segment.id.clone(), segment);
        }
    }
    *edges = projected.into_values().collect();
    Ok(())
}

struct Fragment {
    nodes: Vec<LayoutNode>,
    edges: Vec<LayoutEdge>,
    bounds: Option<Bounds>,
    diff: f64,
}

#[derive(Clone, Copy)]
struct TitleMargins {
    top: f64,
    total: f64,
}

fn margins(config: &Value) -> TitleMargins {
    let value = |key| {
        config
            .pointer(&format!("/flowchart/subGraphTitleMargin/{key}"))
            .and_then(Value::as_f64)
            .filter(|value| value.is_finite())
            .unwrap_or(0.0)
    };
    let top = value("top");
    TitleMargins {
        top,
        total: top + value("bottom"),
    }
}

fn graph(label: GraphLabel) -> DagreGraph {
    let mut graph = Graph::new(GraphOptions {
        directed: true,
        multigraph: true,
        compound: true,
    });
    graph.set_graph(label);
    graph
}

fn spacing(config: &Value, top: &str, local: &str, flowchart: &str) -> f64 {
    // The pinned wrapper uses JS `||`; zero falls through to the following source.
    let local_default = Value::from(50.0);
    [
        config.get(top),
        Some(
            config
                .get("usecase")
                .and_then(|value| value.get(local))
                .unwrap_or(&local_default),
        ),
        config
            .get("flowchart")
            .and_then(|value| value.get(flowchart)),
    ]
    .into_iter()
    .flatten()
    .filter_map(Value::as_f64)
    .find(|value| value.is_finite() && *value != 0.0)
    .unwrap_or(50.0)
}

pub(super) fn layout(
    model: &UsecaseDiagramRenderModel,
    config: &Value,
    plans: &[UsecaseNodePlan],
    edge_plans: &mut [UsecaseEdgePlan],
    work: &mut OperationLayoutWorkControl,
) -> Result<UsecaseDiagramLayout> {
    let mut root = graph(GraphLabel {
        rankdir: match model.direction.as_str() {
            "LR" => RankDir::LR,
            "RL" => RankDir::RL,
            "BT" => RankDir::BT,
            _ => RankDir::TB,
        },
        nodesep: spacing(config, "nodeSpacing", "nodeSpacing", "nodeSpacing"),
        ranksep: spacing(config, "rankSpacing", "rankSpacing", "rankSpacing"),
        marginx: 8.0,
        marginy: 8.0,
        ..Default::default()
    });
    let plan_by_id: HashMap<_, _> = plans.iter().map(|plan| (plan.id.as_str(), plan)).collect();
    let edge_by_id: HashMap<_, _> = edge_plans
        .iter()
        .map(|plan| (plan.id.as_str(), plan))
        .collect();
    let mut external = HashSet::new();
    for plan in plans {
        work.charge_adapter(1)?;
        root.set_node(
            plan.id.clone(),
            NodeLabel {
                width: plan.width,
                height: plan.height,
                ..Default::default()
            },
        );
        // setParent creates the cluster node immediately, as in prepareLayoutForDagre.
        // Its explicit label replaces that placeholder when its own declaration is visited.
        if let Some(parent) = &plan.parent {
            root.set_parent_ref(&plan.id, parent);
        }
    }
    for plan in edge_plans.iter() {
        work.charge_adapter(1)?;
        let source = plan_by_id[plan.source.as_str()];
        let target = plan_by_id[plan.target.as_str()];
        if source.is_boundary || target.is_boundary {
            return Err(Error::InvalidModel {
                message: "Usecase boundaries cannot be relationship endpoints".into(),
            });
        }
        if source.parent != target.parent {
            external.extend(source.parent.as_deref());
            external.extend(target.parent.as_deref());
        }
        let (width, height) = plan.label.as_ref().map_or(
            (
                if config
                    .get("htmlLabels")
                    .and_then(Value::as_bool)
                    .unwrap_or(true)
                {
                    4.0
                } else {
                    0.0
                },
                0.0,
            ),
            |label| (label.metrics.width, label.metrics.height),
        );
        root.set_edge_named(
            plan.source.clone(),
            plan.target.clone(),
            Some(plan.id.clone()),
            Some(EdgeLabel {
                width,
                height,
                minlen: plan.minlen,
                weight: 1.0,
                ..Default::default()
            }),
        );
    }
    let title_margins = margins(config);
    let mut extracted = HashMap::new();
    work.charge_adapter(work.checked_add(root.node_order_slot_count(), root.node_count())?)?;
    for id in root.node_ids() {
        if root.child_count(&id) == 0 || external.contains(id.as_str()) {
            continue;
        }
        let original = root.node(&id).cloned().ok_or_else(|| missing_node(&id))?;
        let mut child = extract(&mut root, &id, work)?;
        child.set_graph(GraphLabel {
            rankdir: if root.graph().rankdir == RankDir::TB {
                RankDir::LR
            } else {
                RankDir::TB
            },
            nodesep: root.graph().nodesep,
            ranksep: root.graph().ranksep + 25.0,
            marginx: 8.0,
            marginy: 8.0,
            ..Default::default()
        });
        // measureDagreGraph injects the original parent only after extraction has established
        // the child graph's node ordering, then parents each otherwise-unparented member.
        let members = child.node_ids();
        child.set_node(id.clone(), original);
        for member in members {
            child.set_parent_ref(&member, &id);
        }
        dugong::layout_controlled(&mut child, work)
            .map_err(|error| work.map_dugong_error(error))?;
        let fragment = project(
            &child,
            &plan_by_id,
            &edge_by_id,
            &HashSet::new(),
            title_margins,
            work,
        )?;
        let bounds = fragment.bounds.as_ref().ok_or_else(|| missing_node(&id))?;
        let placeholder = root.node_mut(&id).ok_or_else(|| missing_node(&id))?;
        placeholder.width = bounds.max_x - bounds.min_x;
        placeholder.height = bounds.max_y - bounds.min_y;
        extracted.insert(id, fragment);
    }
    let extracted_ids = extracted.keys().cloned().collect();
    dugong::layout_controlled(&mut root, work).map_err(|error| work.map_dugong_error(error))?;
    let outer = project(
        &root,
        &plan_by_id,
        &edge_by_id,
        &extracted_ids,
        title_margins,
        work,
    )?;
    let mut nodes = Vec::with_capacity(plans.len());
    let mut edges = outer.edges;
    let mut recursive_edges = HashSet::new();
    for placed in outer.nodes {
        if let Some(mut child) = extracted.remove(&placed.id) {
            // positionNode(clusterNode): the child DOM retains its own graph coordinates.
            // `diff` comes from its boundary painter; centering the child bbox is not equivalent.
            let dx = placed.x + child.diff - placed.width / 2.0;
            let dy = placed.y - placed.height / 2.0 - 8.0;
            for mut node in child.nodes {
                node.x += dx;
                node.y += dy;
                nodes.push(node);
            }
            for mut edge in child.edges.drain(..) {
                recursive_edges.insert(edge.id.clone());
                work.charge_adapter(edge.points.len().saturating_add(1))?;
                for point in &mut edge.points {
                    point.x += dx;
                    point.y += dy;
                }
                if let Some(label) = &mut edge.label {
                    label.x += dx;
                    label.y += dy;
                }
                edges.push(edge);
            }
        } else {
            nodes.push(placed);
        }
    }
    let bounds = painted_bounds(&nodes, &edges, &plan_by_id, title_margins);
    drop(edge_by_id);
    for plan in edge_plans.iter_mut() {
        plan.dagre_recursive = recursive_edges.contains(&plan.id);
    }
    Ok(UsecaseDiagramLayout {
        nodes,
        edges,
        bounds,
    })
}

fn missing_node(id: &str) -> Error {
    Error::InvalidModel {
        message: format!("Dagre omitted Usecase node {id}"),
    }
}

fn extract(
    root: &mut DagreGraph,
    id: &str,
    work: &mut OperationLayoutWorkControl,
) -> Result<DagreGraph> {
    let mut child = graph(GraphLabel::default());
    work.charge_adapter(root.child_count(id))?;
    let members: Vec<_> = root.children(id).into_iter().map(str::to_owned).collect();
    let member_ids: HashSet<_> = members.iter().map(String::as_str).collect();
    // Source copy() visits graph.children() in order and copies all surviving intra-cluster
    // edges after each member. setEdge may introduce the next member before its setNode call;
    // preserving that ordering matters to Dagre's tie-breaking.
    for (index, member) in members.iter().enumerate() {
        work.charge_adapter(1)?;
        let label = root
            .node(member)
            .cloned()
            .ok_or_else(|| missing_node(member))?;
        child.set_node(member.clone(), label);
        // Graphlib edges(node) ignores its argument and visits every edge on the first member.
        // Later copies only overwrite unchanged labels. Skip those redundant full-graph scans.
        if index == 0 {
            work.charge_adapter(work.checked_add(root.edge_slot_count(), root.edge_count())?)?;
            for key in root.edge_keys() {
                if !member_ids.contains(key.v.as_str()) || !member_ids.contains(key.w.as_str()) {
                    continue;
                }
                if let Some(label) = root.edge_by_key(&key).cloned() {
                    child.set_edge_named(key.v, key.w, key.name, Some(label));
                }
            }
        }
        root.remove_node(member);
    }
    Ok(child)
}

fn project(
    graph: &DagreGraph,
    plans: &HashMap<&str, &UsecaseNodePlan>,
    edge_plans: &HashMap<&str, &UsecaseEdgePlan>,
    extracted: &HashSet<String>,
    margins: TitleMargins,
    work: &mut OperationLayoutWorkControl,
) -> Result<Fragment> {
    let mut nodes = Vec::new();
    let mut diff = 0.0;
    work.charge_adapter(work.checked_add(graph.node_order_slot_count(), graph.node_count())?)?;
    for id in graph.node_ids() {
        let Some(plan) = plans.get(id.as_str()) else {
            continue;
        };
        let placed = graph.node(&id).ok_or_else(|| missing_node(&id))?;
        let (Some(x), Some(mut y)) = (placed.x, placed.y) else {
            return Err(missing_node(&id));
        };
        let (mut width, mut height) = (placed.width, placed.height);
        if extracted.contains(&id) {
            y += margins.total;
        } else if plan.is_boundary && graph.child_count(&id) > 0 {
            height += margins.total;
            let original_width = width;
            width = width.max(plan.label.metrics.width + 20.0);
            if plan.package {
                height = height.max(plan.label.metrics.height + 50.0);
            }
            diff = if original_width <= plan.label.metrics.width + 20.0 {
                (width - original_width) / 2.0 - 20.0
            } else {
                -20.0
            };
        } else {
            y += margins.total / 2.0;
        }
        nodes.push(layout::node(plan, x, y, width, height));
    }
    let mut edges = Vec::new();
    work.charge_adapter(work.checked_add(graph.edge_slot_count(), graph.edge_count())?)?;
    for key in graph.edge_keys() {
        let Some(plan) = key.name.as_deref().and_then(|id| edge_plans.get(id)) else {
            continue;
        };
        let Some(placed) = graph.edge_by_key(&key) else {
            continue;
        };
        work.charge_adapter(placed.points.len())?;
        let label = plan.label.as_ref().and_then(|_| {
            Some(LayoutLabel {
                x: placed.x?,
                y: placed.y? + margins.total / 2.0,
                width: placed.width,
                height: placed.height,
            })
        });
        edges.push(layout::edge(
            plan,
            placed
                .points
                .iter()
                .map(|point| LayoutPoint {
                    x: point.x,
                    y: point.y + margins.total / 2.0,
                })
                .collect(),
            label,
        ));
    }
    let bounds = painted_bounds(&nodes, &edges, plans, margins);
    Ok(Fragment {
        nodes,
        edges,
        bounds,
        diff,
    })
}

fn painted_bounds(
    nodes: &[LayoutNode],
    edges: &[LayoutEdge],
    plans: &HashMap<&str, &UsecaseNodePlan>,
    margins: TitleMargins,
) -> Option<Bounds> {
    Bounds::from_points(
        nodes
            .iter()
            .flat_map(|node| {
                [
                    (node.x - node.width / 2.0, node.y - node.height / 2.0),
                    (node.x + node.width / 2.0, node.y + node.height / 2.0),
                ]
            })
            .chain(
                nodes
                    .iter()
                    .filter_map(|node| {
                        let plan = plans.get(node.id.as_str())?;
                        if !plan.is_boundary {
                            return None;
                        }
                        let top = node.y - node.height / 2.0;
                        let (x, y) = if plan.package {
                            let tab_width = node
                                .width
                                .min(80.0_f64.max(plan.label.metrics.width + 20.0));
                            (
                                node.x - node.width / 2.0 + tab_width / 2.0,
                                top + 5.0 + plan.label.metrics.height / 2.0,
                            )
                        } else {
                            (node.x, top + margins.top + plan.label.metrics.height / 2.0)
                        };
                        Some([
                            (
                                x - plan.label.metrics.width / 2.0,
                                y - plan.label.metrics.height / 2.0,
                            ),
                            (
                                x + plan.label.metrics.width / 2.0,
                                y + plan.label.metrics.height / 2.0,
                            ),
                        ])
                    })
                    .flatten(),
            )
            .chain(
                edges
                    .iter()
                    .flat_map(|edge| edge.points.iter().map(|point| (point.x, point.y))),
            )
            .chain(
                edges
                    .iter()
                    .filter_map(|edge| edge.label.as_ref())
                    .flat_map(|label| {
                        [
                            (label.x - label.width / 2.0, label.y - label.height / 2.0),
                            (label.x + label.width / 2.0, label.y + label.height / 2.0),
                        ]
                    }),
            ),
    )
}
