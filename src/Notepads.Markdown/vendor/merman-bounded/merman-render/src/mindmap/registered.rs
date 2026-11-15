//! Mindmap projections for Mermaid's registered graph layout loaders.

use super::{MindmapEdgeModel, MindmapModel, compute_bounds};
use crate::layout_work::OperationLayoutWorkControl;
use crate::model::{LayoutEdge, LayoutNode, LayoutPoint, MindmapDiagramLayout};
use crate::{Error, Result};
use serde_json::Value;

fn edge(model: &MindmapEdgeModel, points: Vec<LayoutPoint>) -> LayoutEdge {
    LayoutEdge {
        id: model.id.clone(),
        from: model.start.clone(),
        to: model.end.clone(),
        from_cluster: None,
        to_cluster: None,
        points,
        label: None,
        start_label_left: None,
        start_label_right: None,
        end_label_left: None,
        end_label_right: None,
        start_marker: None,
        end_marker: None,
        stroke_dasharray: None,
    }
}

pub(super) fn dagre(
    model: &MindmapModel,
    mut nodes: Vec<LayoutNode>,
    config: &Value,
    work: &mut OperationLayoutWorkControl,
) -> Result<MindmapDiagramLayout> {
    use dugong::graphlib::{Graph, GraphOptions};
    use dugong::{EdgeLabel, GraphLabel, NodeLabel, RankDir};
    let spacing = |key: &str| {
        config
            .get(key)
            .and_then(Value::as_f64)
            .filter(|value| value.is_finite() && *value != 0.0)
            .unwrap_or(50.0)
    };
    let mut graph = Graph::<NodeLabel, EdgeLabel, GraphLabel>::new(GraphOptions {
        directed: true,
        multigraph: true,
        compound: true,
    });
    // MindmapDB supplies TB and spacing 50 independently of Flowchart configuration.
    graph.set_graph(GraphLabel {
        rankdir: RankDir::TB,
        nodesep: spacing("nodeSpacing"),
        ranksep: spacing("rankSpacing"),
        marginx: 8.0,
        marginy: 8.0,
        ..Default::default()
    });
    for node in &nodes {
        graph.set_node(
            node.id.clone(),
            NodeLabel {
                width: node.width,
                height: node.height,
                ..Default::default()
            },
        );
    }
    for edge in &model.edges {
        graph.set_edge_named(
            edge.start.clone(),
            edge.end.clone(),
            Some(edge.id.clone()),
            Some(EdgeLabel {
                minlen: 1,
                weight: 1.0,
                ..Default::default()
            }),
        );
    }
    dugong::layout_controlled(&mut graph, work).map_err(|error| work.map_dugong_error(error))?;
    for node in &mut nodes {
        let placed = graph.node(&node.id).ok_or_else(|| Error::InvalidModel {
            message: format!("Dagre omitted Mindmap node {}", node.id),
        })?;
        let (Some(x), Some(y)) = (placed.x, placed.y) else {
            return Err(Error::InvalidModel {
                message: format!("Dagre did not position Mindmap node {}", node.id),
            });
        };
        node.x = x;
        node.y = y;
    }
    let edge_by_id: rustc_hash::FxHashMap<_, _> = model
        .edges
        .iter()
        .map(|edge| (edge.id.as_str(), edge))
        .collect();
    let mut edges = Vec::with_capacity(model.edges.len());
    for key in graph.edge_keys() {
        let Some(source) = key.name.as_deref().and_then(|id| edge_by_id.get(id)) else {
            continue;
        };
        let Some(placed) = graph.edge_by_key(&key) else {
            continue;
        };
        work.charge_adapter(placed.points.len())?;
        edges.push(edge(
            source,
            placed
                .points
                .iter()
                .map(|point| LayoutPoint {
                    x: point.x,
                    y: point.y,
                })
                .collect(),
        ));
    }
    let bounds = compute_bounds(&nodes, &edges);
    Ok(MindmapDiagramLayout {
        swimlane_lanes: Vec::new(),
        nodes,
        edges,
        bounds,
    })
}

#[cfg(feature = "layout-elk")]
pub(super) fn elk(
    model: &MindmapModel,
    mut nodes: Vec<LayoutNode>,
    config: &Value,
    work: &mut OperationLayoutWorkControl,
    operation_seed: merman_layout_elk::ElkOperationSeed,
) -> Result<MindmapDiagramLayout> {
    use merman_layout_elk as elk;
    let mut graph = elk::Graph {
        id: "mindmap".into(),
        direction: elk::Direction::Down,
        nodes: nodes
            .iter()
            .map(|node| elk::Node {
                id: node.id.clone(),
                width: node.width,
                height: node.height,
                kind: elk::NodeKind::Leaf,
                label_text: None,
                container: Default::default(),
                parent: None,
                direction: None,
                hierarchy_handling: None,
                layer_constraint: None,
                port_alignment: None,
                label: None,
            })
            .collect(),
        edges: model
            .edges
            .iter()
            .map(|edge| elk::Edge {
                id: edge.id.clone(),
                source: edge.start.clone(),
                target: edge.end.clone(),
                label: None,
                minlen: 1,
                terminal_labels: Vec::new(),
                inside_self_loops_yo: false,
            })
            .collect(),
        spacing: elk::Spacing::default(),
        options: crate::elk_options::layout_options(config),
    };
    let oriented = crate::elk_feedback_edges::orient_feedback_edges(
        &mut graph,
        config,
        &mut Some(&mut *work),
    )?;
    let mut placed =
        elk::layout_with_operation_seed_and_work_control(oriented.graph(), operation_seed, work)
            .map_err(|error| work.map_elk_error_with_context(error, "Mindmap ELK"))?;
    oriented.restore(&mut placed, &mut Some(&mut *work))?;
    let indices: rustc_hash::FxHashMap<_, _> = nodes
        .iter()
        .enumerate()
        .map(|(index, node)| (node.id.clone(), index))
        .collect();
    for positioned in placed.nodes {
        let index = indices
            .get(&positioned.id)
            .ok_or_else(|| Error::InvalidModel {
                message: format!("ELK returned unknown Mindmap node {}", positioned.id),
            })?;
        let node = &mut nodes[*index];
        node.x = positioned.x;
        node.y = positioned.y;
        node.width = positioned.width;
        node.height = positioned.height;
    }
    let edge_by_id: rustc_hash::FxHashMap<_, _> = model
        .edges
        .iter()
        .map(|edge| (edge.id.as_str(), edge))
        .collect();
    let mut edges = Vec::with_capacity(placed.edges.len());
    for positioned in placed.edges {
        let source = edge_by_id
            .get(positioned.id.as_str())
            .ok_or_else(|| Error::InvalidModel {
                message: format!("ELK returned unknown Mindmap edge {}", positioned.id),
            })?;
        work.charge_adapter(positioned.points.len())?;
        edges.push(edge(
            source,
            positioned
                .points
                .into_iter()
                .map(|point| LayoutPoint {
                    x: point.x,
                    y: point.y,
                })
                .collect(),
        ));
    }
    let bounds = compute_bounds(&nodes, &edges);
    Ok(MindmapDiagramLayout {
        swimlane_lanes: Vec::new(),
        nodes,
        edges,
        bounds,
    })
}

pub(super) fn swimlane(
    model: &MindmapModel,
    nodes: Vec<LayoutNode>,
    config: &merman_core::MermaidConfig,
    work_meter: std::sync::Arc<crate::resources::OperationWorkMeter>,
) -> Result<MindmapDiagramLayout> {
    use crate::model::{LayoutCluster, LayoutLabel};
    let measured: Vec<_> = nodes
        .iter()
        .zip(&model.nodes)
        .map(|(node, model)| (node, model.shape.as_str()))
        .collect();
    let mut edges: Vec<_> = model
        .edges
        .iter()
        .map(|source| edge(source, Vec::new()))
        .collect();
    let placed = crate::swimlane::layout_flat(&measured, &edges, config, work_meter)?;
    let nodes = placed
        .nodes
        .into_iter()
        .map(|node| LayoutNode {
            id: node.id,
            x: node.x,
            y: node.y,
            width: node.width,
            height: node.height,
            is_cluster: false,
            label_width: Some(node.label_width),
            label_height: Some(node.label_height),
        })
        .collect();
    let routes: rustc_hash::FxHashMap<_, _> = placed
        .edges
        .into_iter()
        .map(|edge| (edge.id, edge.points))
        .collect();
    for edge in &mut edges {
        edge.points = routes.get(&edge.id).cloned().unwrap_or_default();
    }
    let swimlane_lanes = placed
        .lanes
        .into_iter()
        .map(|lane| LayoutCluster {
            id: lane.id,
            x: lane.x,
            y: lane.y,
            width: lane.width,
            height: lane.height,
            diff: 0.0,
            offset_y: 0.0,
            title: lane.title,
            title_label: LayoutLabel {
                x: lane.x,
                y: lane.y - lane.height / 2.0,
                width: 0.0,
                height: 0.0,
            },
            requested_dir: lane.requested_dir,
            effective_dir: "TB".into(),
            padding: lane.padding,
            title_margin_top: 0.0,
            title_margin_bottom: 0.0,
        })
        .collect();
    Ok(MindmapDiagramLayout {
        nodes,
        edges,
        bounds: placed.bounds,
        swimlane_lanes,
    })
}
