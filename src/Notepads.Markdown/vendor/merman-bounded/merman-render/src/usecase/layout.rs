use super::*;
use crate::Error;
use crate::layout_work::OperationLayoutWorkControl;
use crate::model::{LayoutLabel, LayoutPoint};
use std::collections::HashMap;

pub(super) fn layout(
    model: &UsecaseDiagramRenderModel,
    config: &Value,
    plans: &[UsecaseNodePlan],
    edge_plans: &mut [UsecaseEdgePlan],
    work: &mut OperationLayoutWorkControl,
    #[cfg(feature = "layout-elk")] operation_seed: merman_layout_elk::ElkOperationSeed,
) -> Result<UsecaseDiagramLayout> {
    crate::layout_backend::resolve_graph_layout(config).validate_rootless_graph()?;
    #[cfg(feature = "layout-elk")]
    if crate::layout_backend::resolve_graph_layout(config).backend
        == crate::layout_backend::GraphLayoutBackend::Elk
    {
        return elk_layout(model, config, plans, edge_plans, work, operation_seed);
    }
    super::dagre::layout(model, config, plans, edge_plans, work)
}

pub(super) fn node(plan: &UsecaseNodePlan, x: f64, y: f64, width: f64, height: f64) -> LayoutNode {
    LayoutNode {
        id: plan.id.clone(),
        x,
        y,
        width,
        height,
        is_cluster: plan.is_boundary,
        label_width: Some(plan.label.metrics.width),
        label_height: Some(plan.label.metrics.height),
    }
}

pub(super) fn edge(
    plan: &UsecaseEdgePlan,
    points: Vec<LayoutPoint>,
    label: Option<LayoutLabel>,
) -> LayoutEdge {
    LayoutEdge {
        id: plan.id.clone(),
        from: plan.source.clone(),
        to: plan.target.clone(),
        from_cluster: None,
        to_cluster: None,
        points,
        label,
        start_label_left: None,
        start_label_right: None,
        end_label_left: None,
        end_label_right: None,
        start_marker: plan.start_marker.clone(),
        end_marker: plan.end_marker.clone(),
        stroke_dasharray: plan.dotted.then(|| "3,3".to_owned()),
    }
}

#[cfg(feature = "layout-elk")]
fn elk_layout(
    model: &UsecaseDiagramRenderModel,
    config: &Value,
    plans: &[UsecaseNodePlan],
    edge_plans: &[UsecaseEdgePlan],
    work: &mut OperationLayoutWorkControl,
    operation_seed: merman_layout_elk::ElkOperationSeed,
) -> Result<UsecaseDiagramLayout> {
    use merman_layout_elk as elk;
    let direction = match model.direction.as_str() {
        "LR" => elk::Direction::Right,
        "RL" => elk::Direction::Left,
        "BT" => elk::Direction::Up,
        _ => elk::Direction::Down,
    };
    let mut graph = elk::Graph {
        id: "usecase".into(),
        direction,
        nodes: plans
            .iter()
            .map(|plan| elk::Node {
                id: plan.id.clone(),
                kind: if plan.is_boundary {
                    elk::NodeKind::Group
                } else {
                    elk::NodeKind::Leaf
                },
                container: elk::ContainerNodeOptions {
                    padding: if plan.is_boundary { 20.0 } else { 0.0 },
                    ..Default::default()
                },
                label_text: plan.is_boundary.then(|| plan.label.text.clone()),
                width: plan.width,
                height: plan.height,
                parent: plan.parent.clone(),
                direction: plan.is_boundary.then_some(direction),
                hierarchy_handling: plan
                    .is_boundary
                    .then_some(elk::HierarchyHandling::IncludeChildren),
                layer_constraint: None,
                port_alignment: plan.ellipse.then_some(elk::PortAlignment::Center),
                label: plan.is_boundary.then_some(elk::Label {
                    width: plan.label.metrics.width,
                    height: plan.label.metrics.height,
                }),
            })
            .collect(),
        edges: edge_plans
            .iter()
            .map(|plan| elk::Edge {
                id: plan.id.clone(),
                source: plan.source.clone(),
                target: plan.target.clone(),
                label: plan.label.as_ref().map(|label| elk::Label {
                    width: label.metrics.width,
                    height: label.metrics.height,
                }),
                minlen: plan.minlen,
                terminal_labels: Vec::new(),
                inside_self_loops_yo: false,
            })
            .collect(),
        spacing: elk::Spacing {
            node_node: measure::number(config, "nodeSpacing", 50.0),
            layer_layer: measure::number(config, "rankSpacing", 50.0),
            ..Default::default()
        },
        options: crate::elk_options::layout_options(config),
    };
    if config
        .pointer("/elk/keepEntryNodeOnTop")
        .and_then(Value::as_bool)
        .unwrap_or(false)
    {
        crate::elk_adapter::apply_cyclic_entry_constraints(
            &mut graph.nodes,
            graph
                .edges
                .iter()
                .map(|edge| (edge.source.as_str(), edge.target.as_str())),
            &mut Some(&mut *work),
        )?;
    }
    let oriented = crate::elk_feedback_edges::orient_feedback_edges(
        &mut graph,
        config,
        &mut Some(&mut *work),
    )?;
    let mut placed =
        elk::layout_with_operation_seed_and_work_control(oriented.graph(), operation_seed, work)
            .map_err(|error| work.map_elk_error_with_context(error, "Usecase ELK"))?;
    oriented.restore(&mut placed, &mut Some(&mut *work))?;
    let frames = crate::elk_adapter::drawing_group_frames(
        &graph,
        &placed,
        |node| node.label.as_ref().map_or(0.0, |label| label.width) + node.container.padding,
        &mut Some(&mut *work),
    )?;
    let plans_by_id: HashMap<_, _> = plans.iter().map(|plan| (plan.id.as_str(), plan)).collect();
    let edges_by_id: HashMap<_, _> = edge_plans
        .iter()
        .map(|plan| (plan.id.as_str(), plan))
        .collect();
    let mut nodes = Vec::with_capacity(placed.nodes.len());
    for placed in placed.nodes {
        let plan = plans_by_id
            .get(placed.id.as_str())
            .ok_or_else(|| Error::InvalidModel {
                message: format!("ELK returned unknown Usecase node {}", placed.id),
            })?;
        // Project only the drawn group frame; provider routes keep their original origins.
        let frame = frames
            .get(placed.id.as_str())
            .copied()
            .unwrap_or_else(|| crate::elk_adapter::GroupFrame::from(&placed));
        nodes.push(node(plan, frame.x, frame.y, frame.width, frame.height));
    }
    let mut edges = Vec::with_capacity(placed.edges.len());
    for placed in placed.edges {
        let plan = edges_by_id
            .get(placed.id.as_str())
            .ok_or_else(|| Error::InvalidModel {
                message: format!("ELK returned unknown Usecase edge {}", placed.id),
            })?;
        let label = placed.labels.first().map(|label| LayoutLabel {
            x: label.x + label.width / 2.0,
            y: label.y + label.height / 2.0,
            width: label.width,
            height: label.height,
        });
        edges.push(edge(
            plan,
            placed
                .points
                .into_iter()
                .map(|point| LayoutPoint {
                    x: point.x,
                    y: point.y,
                })
                .collect(),
            label,
        ));
    }
    finish(nodes, edges, plans, edge_plans, work)
}

fn intersect(node: &LayoutNode, ellipse: bool, target: &LayoutPoint) -> LayoutPoint {
    let dx = target.x - node.x;
    let dy = target.y - node.y;
    let rx = node.width / 2.0;
    let ry = node.height / 2.0;
    if dx == 0.0 && dy == 0.0 {
        return LayoutPoint {
            x: node.x + rx,
            y: node.y,
        };
    }
    let denominator = if ellipse {
        (dx * dx / (rx * rx) + dy * dy / (ry * ry)).sqrt()
    } else {
        (dx.abs() / rx).max(dy.abs() / ry)
    };
    if !denominator.is_finite() || denominator == 0.0 {
        return LayoutPoint {
            x: node.x,
            y: node.y,
        };
    }
    LayoutPoint {
        x: node.x + dx / denominator,
        y: node.y + dy / denominator,
    }
}

#[cfg(feature = "layout-elk")]
fn finish(
    nodes: Vec<LayoutNode>,
    mut edges: Vec<LayoutEdge>,
    plans: &[UsecaseNodePlan],
    edge_plans: &[UsecaseEdgePlan],
    work: &mut OperationLayoutWorkControl,
) -> Result<UsecaseDiagramLayout> {
    let by_id: HashMap<_, _> = nodes.iter().map(|node| (node.id.as_str(), node)).collect();
    let edge_by_id: HashMap<_, _> = edge_plans
        .iter()
        .map(|plan| (plan.id.as_str(), plan))
        .collect();
    for edge in &mut edges {
        work.charge_adapter(edge.points.len().saturating_add(1))?;
        let Some(start) = by_id.get(edge.from.as_str()) else {
            continue;
        };
        let Some(end) = by_id.get(edge.to.as_str()) else {
            continue;
        };
        edge.from_cluster = start.is_cluster.then(|| start.id.clone());
        edge.to_cluster = end.is_cluster.then(|| end.id.clone());
        if edge.label.is_none()
            && let Some(label) = edge_by_id
                .get(edge.id.as_str())
                .and_then(|plan| plan.label.as_ref())
            && let Some((x, y)) = route_midpoint(&edge_points(edge, &nodes, plans))
        {
            edge.label = Some(LayoutLabel {
                x,
                y,
                width: label.metrics.width,
                height: label.metrics.height,
            });
        }
    }
    let bounds = Bounds::from_points(
        nodes
            .iter()
            .flat_map(|node| {
                [
                    (node.x - node.width / 2.0, node.y - node.height / 2.0),
                    (node.x + node.width / 2.0, node.y + node.height / 2.0),
                ]
            })
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
    );
    Ok(UsecaseDiagramLayout {
        nodes,
        edges,
        bounds,
    })
}

#[cfg(feature = "layout-elk")]
fn route_midpoint(points: &[LayoutPoint]) -> Option<(f64, f64)> {
    let first = points.first()?;
    let distance = |pair: &[LayoutPoint]| (pair[1].x - pair[0].x).hypot(pair[1].y - pair[0].y);
    let mut remaining = points.windows(2).map(distance).sum::<f64>() / 2.0;
    for pair in points.windows(2) {
        let length = distance(pair);
        if length > 0.0 && remaining <= length {
            let t = remaining / length;
            return Some((
                pair[0].x + (pair[1].x - pair[0].x) * t,
                pair[0].y + (pair[1].y - pair[0].y) * t,
            ));
        }
        remaining -= length;
    }
    Some((first.x, first.y))
}

pub(super) fn edge_points(
    edge: &LayoutEdge,
    nodes: &[LayoutNode],
    plans: &[UsecaseNodePlan],
) -> Vec<LayoutPoint> {
    let Some(start) = nodes.iter().find(|node| node.id == edge.from) else {
        return edge.points.clone();
    };
    let Some(end) = nodes.iter().find(|node| node.id == edge.to) else {
        return edge.points.clone();
    };
    let start_ellipse = plans
        .iter()
        .any(|plan| plan.id == edge.from && plan.ellipse);
    let end_ellipse = plans.iter().any(|plan| plan.id == edge.to && plan.ellipse);
    let mut points = edge.points.clone();
    if points.is_empty() {
        points = vec![
            LayoutPoint {
                x: start.x,
                y: start.y,
            },
            LayoutPoint { x: end.x, y: end.y },
        ];
        let centers = points.clone();
        if (end.x - start.x).abs() >= start.width / 2.0
            || (end.y - start.y).abs() >= start.height / 2.0
        {
            points[0] = intersect(start, start_ellipse, &points[1]);
        }
        points[1] = intersect(end, end_ellipse, &points[0]);
        if (points[1].x - points[0].x).hypot(points[1].y - points[0].y) < 2.0 {
            points = centers;
        }
        points.dedup_by(|right, left| {
            (right.x - left.x).abs() <= 1e-6 && (right.y - left.y).abs() <= 1e-6
        });
    } else if points.len() >= 2 {
        let last = points.len() - 1;
        points[0] = intersect(start, start_ellipse, &points[1]);
        points[last] = intersect(end, end_ellipse, &points[last - 1]);
    }
    points
}

pub(super) fn prepare_edge_paths(
    layout: &mut UsecaseDiagramLayout,
    plans: &[UsecaseNodePlan],
    edge_plans: &[UsecaseEdgePlan],
    config: &Value,
    work: &mut OperationLayoutWorkControl,
) -> Result<HashMap<String, Vec<LayoutPoint>>> {
    use super::elk_edge_geometry::{self as geometry, Outline, Shape};
    let elk = crate::layout_backend::resolve_graph_layout(config).backend
        == crate::layout_backend::GraphLayoutBackend::Elk;
    // Dagre's recursive paint measurement includes title labels which may extend beyond
    // a rect boundary's frame. Retain that measured contribution when clipping edge paths.
    let dagre_paint_bounds = (!elk).then(|| layout.bounds.clone()).flatten();
    let node_indexes: HashMap<_, _> = layout
        .nodes
        .iter()
        .enumerate()
        .map(|(index, node)| (node.id.clone(), index))
        .collect();
    let edge_indexes: HashMap<_, _> = layout
        .edges
        .iter()
        .enumerate()
        .map(|(index, edge)| (edge.id.as_str(), index))
        .collect();
    let plan_by_id: HashMap<_, _> = plans.iter().map(|plan| (plan.id.as_str(), plan)).collect();
    if elk {
        // The source resolves every node's first tiny-side anchor before building any route.
        let mut aligned = std::collections::HashSet::new();
        for edge in &layout.edges {
            for (id, anchor) in [
                (&edge.from, edge.points.first()),
                (&edge.to, edge.points.last()),
            ] {
                work.charge_adapter(1)?;
                let (Some(&index), Some(anchor)) = (node_indexes.get(id), anchor) else {
                    continue;
                };
                let node = &mut layout.nodes[index];
                let along_width = (anchor.y - (node.y - node.height / 2.0)).abs() <= 0.5
                    || (anchor.y - (node.y + node.height / 2.0)).abs() <= 0.5;
                if (if along_width { node.width } else { node.height }) >= 24.0
                    || !aligned.insert(index)
                {
                    continue;
                }
                let delta = if along_width {
                    anchor.x - node.x
                } else {
                    anchor.y - node.y
                };
                if delta.abs() < 0.01 {
                    continue;
                }
                if along_width {
                    node.x += delta;
                } else {
                    node.y += delta;
                }
            }
        }
    }
    work.charge_adapter(edge_plans.len())?;
    let mut route_layout_indexes = Vec::with_capacity(edge_plans.len());
    let mut routes = Vec::with_capacity(edge_plans.len());
    for plan in edge_plans {
        let Some(&index) = edge_indexes.get(plan.id.as_str()) else {
            return Err(Error::InvalidModel {
                message: format!("Usecase layout omitted edge {}", plan.id),
            });
        };
        let edge = &layout.edges[index];
        work.charge_adapter(work.checked_add(edge.points.len(), 50)?)?;
        let points = if elk {
            let start = Shape {
                intersection: None,
                node: &layout.nodes[node_indexes[&edge.from]],
                outline: if plan_by_id[edge.from.as_str()].ellipse {
                    Outline::Ellipse
                } else {
                    Outline::Rect
                },
            };
            let end = Shape {
                intersection: None,
                node: &layout.nodes[node_indexes[&edge.to]],
                outline: if plan_by_id[edge.to.as_str()].ellipse {
                    Outline::Ellipse
                } else {
                    Outline::Rect
                },
            };
            let mut input = Vec::with_capacity(edge.points.len() + 2);
            input.push(LayoutPoint {
                x: start.node.x,
                y: start.node.y,
            });
            input.extend_from_slice(&edge.points);
            input.push(LayoutPoint {
                x: end.node.x,
                y: end.node.y,
            });
            let mut points = geometry::sanitize(&input, start, end);
            if !edge.points.is_empty() {
                geometry::marker_segment(&mut points, end, plan.end_marker.as_deref(), false);
                geometry::marker_segment(&mut points, start, plan.start_marker.as_deref(), true);
            }
            points
        } else {
            if let Some(id) = plan
                .self_loop_node
                .as_deref()
                .filter(|_| !plan.dagre_recursive)
            {
                // Top-level normalization retains the original self-edge endpoints. Recursive
                // Dagre paint instead receives the actual helper-node endpoints.
                let mut paint_edge = edge.clone();
                paint_edge.from = id.to_owned();
                paint_edge.to = id.to_owned();
                edge_points(&paint_edge, &layout.nodes, plans)
            } else {
                edge_points(edge, &layout.nodes, plans)
            }
        };
        // Empty diagrams can have one coincident endpoint after source deduplication.
        // Do not synthesize a second terminal, which would change the source fallback.
        route_layout_indexes.push(index);
        routes.push(points);
    }
    drop(edge_indexes);
    if elk
        && config
            .pointer("/elk/straightenEdges")
            .and_then(Value::as_bool)
            != Some(false)
    {
        let changes =
            crate::elk_terminal_jogs::straighten_edge_terminals_with_runs(&mut routes, |units| {
                work.charge_adapter(units)
            })?;
        for change in changes {
            let index = route_layout_indexes[change.route_index];
            if let Some(label) = layout.edges[index].label.as_mut() {
                crate::elk_terminal_jogs::reproject_label(label, &change.runs);
            }
        }
    }
    let paths: HashMap<_, _> = edge_plans
        .iter()
        .zip(routes)
        .map(|(plan, route)| (plan.id.clone(), route))
        .collect();
    // Missing sections use the two terminal midpoint before painting; routed labels
    // retain the provider's own label placement.
    if elk {
        for edge in &mut layout.edges {
            if edge.points.is_empty()
                && let Some(label) = edge.label.as_mut()
                && let Some(points) = paths.get(&edge.id)
                && let (Some(first), Some(last)) = (points.first(), points.last())
            {
                label.x = (first.x + last.x) / 2.0;
                label.y = (first.y + last.y) / 2.0;
            }
        }
    }
    if elk {
        crate::elk_terminal_jogs::separate_opposite_edge_labels(
            layout
                .edges
                .iter_mut()
                .map(|edge| (edge.from.as_str(), edge.to.as_str(), edge.label.as_mut())),
            |units| work.charge_adapter(units),
        )?;
    }
    layout.bounds = Bounds::from_points(
        layout
            .nodes
            .iter()
            .flat_map(|node| {
                [
                    (node.x - node.width / 2.0, node.y - node.height / 2.0),
                    (node.x + node.width / 2.0, node.y + node.height / 2.0),
                ]
            })
            .chain(
                paths
                    .values()
                    .flat_map(|points| points.iter().map(|p| (p.x, p.y))),
            )
            .chain(
                layout
                    .edges
                    .iter()
                    .filter_map(|edge| edge.label.as_ref())
                    .flat_map(|label| {
                        [
                            (label.x - label.width / 2.0, label.y - label.height / 2.0),
                            (label.x + label.width / 2.0, label.y + label.height / 2.0),
                        ]
                    }),
            ),
    );
    if let Some(previous) = dagre_paint_bounds {
        if let Some(bounds) = &mut layout.bounds {
            bounds.min_x = bounds.min_x.min(previous.min_x);
            bounds.min_y = bounds.min_y.min(previous.min_y);
            bounds.max_x = bounds.max_x.max(previous.max_x);
            bounds.max_y = bounds.max_y.max(previous.max_y);
        } else {
            layout.bounds = Some(previous);
        }
    }
    Ok(paths)
}
