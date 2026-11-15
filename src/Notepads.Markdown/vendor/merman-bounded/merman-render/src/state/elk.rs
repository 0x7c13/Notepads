//! Mermaid 12 StateDB projection into the registered ELK renderer.
//!
//! Keep semantic IDs, hierarchy and self loops intact: Dagre's recursive extraction and
//! cyclic-special nodes belong only to its own adapter.

use super::StateDiagramModel;
use super::config::*;
use super::layout::{
    edge_label_metrics, note_group_owner_id, state_fork_join_painted_dimensions,
    state_hidden_prefixes, state_node_dimensions, title_label_metrics,
    validate_state_parent_cycles,
};
use crate::layout_work::OperationLayoutWorkControl;
use crate::model::{
    Bounds, LayoutCluster, LayoutEdge, LayoutLabel, LayoutNode, LayoutPoint, StateDiagramLayout,
};
use crate::text::TextMeasurer;
use crate::{Error, Result};
use merman_layout_elk as elk;
use serde_json::Value;
use std::collections::HashMap;

fn is_self_loop_helper(id: &str) -> bool {
    let Some((prefix, suffix)) = id.rsplit_once("---") else {
        return false;
    };
    if suffix != "1" && suffix != "2" {
        return false;
    }
    let Some((left, right)) = prefix.rsplit_once("---") else {
        return false;
    };
    !left.is_empty() && left == right
}

fn direction(value: &str) -> elk::Direction {
    match normalize_dir(value).as_str() {
        "LR" => elk::Direction::Right,
        "RL" => elk::Direction::Left,
        "BT" => elk::Direction::Up,
        _ => elk::Direction::Down,
    }
}

pub(super) fn layout(
    model: &StateDiagramModel,
    config: &Value,
    measurer: &dyn TextMeasurer,
    operation_seed: elk::ElkOperationSeed,
    work: &mut OperationLayoutWorkControl,
) -> Result<StateDiagramLayout> {
    validate_state_parent_cycles(model)?;
    let hidden = state_hidden_prefixes(model);
    let settings = StateConfigView::new(config).layout_settings(&model.direction);
    // The parser retains each note declaration, while StateDB overwrites the physical
    // note-group metadata. Preserve the final group identity without orphaning earlier notes.
    let mut note_groups = HashMap::new();
    for node in &model.nodes {
        if node.shape == "noteGroup"
            && let Some(owner) = note_group_owner_id(&node.id)
        {
            note_groups.insert(owner, node.id.as_str());
        }
    }
    let canonical_groups: HashMap<_, _> = model
        .nodes
        .iter()
        .filter(|node| node.shape == "noteGroup")
        .filter_map(|node| {
            note_group_owner_id(&node.id)
                .and_then(|owner| note_groups.get(owner).copied())
                .map(|canonical| (node.id.as_str(), canonical))
        })
        .collect();
    let canonical =
        |id: &str| -> String { canonical_groups.get(id).copied().unwrap_or(id).to_owned() };
    let resolved_directions = crate::elk_adapter::resolved_node_directions(
        model.nodes.iter().map(|node| {
            (
                node.id.as_str(),
                node.parent_id
                    .as_deref()
                    .map(|id| canonical_groups.get(id).copied().unwrap_or(id)),
                node.dir.as_deref().map(direction),
            )
        }),
        model.nodes.len(),
        direction(&model.direction),
        &mut Some(&mut *work),
    )?;
    let mut nodes = Vec::with_capacity(model.nodes.len());
    let mut source_nodes = HashMap::new();
    let mut painted_group_labels = HashMap::new();
    for node in &model.nodes {
        if hidden.is_hidden(&node.id) || canonical(&node.id) != node.id {
            continue;
        }
        let group = state_node_is_effective_group(node);
        let title = node
            .label
            .as_ref()
            .map(value_to_label_text)
            .unwrap_or_else(|| node.id.clone());
        let (width, height) = if group {
            (0.0, 0.0)
        } else if matches!(node.shape.as_str(), "fork" | "join") {
            // ELK insertMeasuredNode replaces forkJoin's padded dimensions with the
            // painted element bbox. Dagre retains that shape function's padding.
            let rankdir = match resolved_directions[node.id.as_str()] {
                elk::Direction::Right => dugong::RankDir::LR,
                elk::Direction::Left => dugong::RankDir::RL,
                elk::Direction::Up => dugong::RankDir::BT,
                elk::Direction::Down => dugong::RankDir::TB,
            };
            state_fork_join_painted_dimensions(rankdir)
        } else {
            state_node_dimensions(node, &settings, measurer)?
        };
        let label = if group {
            let (width, height) = if title.is_empty() {
                (0.0, 0.0)
            } else {
                title_label_metrics(&title, measurer, &settings.text_style, settings.wrap_mode)
            };
            painted_group_labels.insert(node.id.as_str(), elk::Label { width, height });
            // Mermaid ELK's getMeasuredLabelData removes the 2px labelBBox adjustment
            // before reserving the title strip; the cluster painter retains the full bbox.
            Some(elk::Label {
                width,
                height: (height - 2.0).max(0.0),
            })
        } else {
            None
        };
        nodes.push(elk::Node {
            id: node.id.clone(),
            kind: if group {
                elk::NodeKind::Group
            } else {
                elk::NodeKind::Leaf
            },
            container: elk::ContainerNodeOptions {
                padding: node.padding.unwrap_or(settings.state_padding).max(0.0),
                ..Default::default()
            },
            label_text: group.then_some(title),
            width,
            height,
            parent: node
                .parent_id
                .as_deref()
                .filter(|id| !hidden.is_hidden(id))
                .map(canonical),
            direction: node.dir.as_deref().filter(|_| group).map(direction),
            hierarchy_handling: group.then_some(if node.dir.is_some() {
                elk::HierarchyHandling::SeparateChildren
            } else {
                elk::HierarchyHandling::IncludeChildren
            }),
            layer_constraint: None,
            port_alignment: None,
            label,
        });
        source_nodes.insert(node.id.as_str(), node);
    }
    let mut source_edges = HashMap::new();
    let mut edges = Vec::with_capacity(model.edges.len());
    for edge in &model.edges {
        if hidden.is_hidden(&edge.id)
            || hidden.is_hidden(&edge.start)
            || hidden.is_hidden(&edge.end)
        {
            continue;
        }
        let label = crate::text::mermaid_html_breaks_to_newlines(&edge.label);
        let (width, height) =
            edge_label_metrics(&label, measurer, &settings.text_style, settings.wrap_mode);
        edges.push(elk::Edge {
            id: edge.id.clone(),
            source: canonical(&edge.start),
            target: canonical(&edge.end),
            label: (!edge.label.trim().is_empty()).then_some(elk::Label { width, height }),
            minlen: 1,
            terminal_labels: Vec::new(),
            inside_self_loops_yo: false,
        });
        source_edges.insert(edge.id.as_str(), edge);
    }
    let mut graph = elk::Graph {
        id: "root".to_owned(),
        direction: direction(&model.direction),
        nodes,
        edges,
        spacing: elk::Spacing {
            node_node: settings.graph.nodesep,
            layer_layer: settings.graph.ranksep,
            ..Default::default()
        },
        options: crate::elk_options::layout_options(config),
    };
    if crate::config::config_bool(config, &["elk", "keepEntryNodeOnTop"]).unwrap_or(false) {
        crate::elk_adapter::apply_cyclic_entry_constraints(
            &mut graph.nodes,
            graph
                .edges
                .iter()
                .map(|edge| (edge.source.as_str(), edge.target.as_str())),
            &mut Some(&mut *work),
        )?;
    }
    crate::elk_hierarchy::apply_to_graph(&mut graph, work)?;
    let oriented = crate::elk_feedback_edges::orient_feedback_edges(
        &mut graph,
        config,
        &mut Some(&mut *work),
    )?;
    let mut placed =
        elk::layout_with_operation_seed_and_work_control(oriented.graph(), operation_seed, work)
            .map_err(|error| work.map_elk_error_with_context(error, "State ELK"))?;
    oriented.restore(&mut placed, &mut Some(&mut *work))?;
    let frames = crate::elk_adapter::drawing_group_frames(
        &graph,
        &placed,
        |node| {
            if source_nodes
                .get(node.id.as_str())
                .is_some_and(|source| source.shape == "noteGroup" || source.shape == "divider")
            {
                0.0
            } else {
                node.label.map_or(0.0, |label| label.width) + node.container.padding
            }
        },
        &mut Some(&mut *work),
    )?;
    let graph_nodes: HashMap<_, _> = graph
        .nodes
        .iter()
        .map(|node| (node.id.as_str(), node))
        .collect();
    let graph_edges: HashMap<_, _> = graph
        .edges
        .iter()
        .map(|edge| (edge.id.as_str(), edge))
        .collect();
    let mut output = StateDiagramLayout {
        nodes: Vec::with_capacity(placed.nodes.len()),
        edges: Vec::with_capacity(placed.edges.len()),
        clusters: Vec::new(),
        bounds: None,
        uses_elk_adapter_dom: true,
        elk_edge_paths: HashMap::new(),
    };
    for mut node in placed.nodes {
        // ELK may materialize helper vertices for native self-loops. Mermaid's public
        // StateDB layout exposes only semantic nodes; retain their route sections in the
        // edge sidecar but never leak helper IDs into compatibility layout JSON.
        if !source_nodes.contains_key(node.id.as_str()) && is_self_loop_helper(&node.id) {
            continue;
        }
        let source = graph_nodes
            .get(node.id.as_str())
            .ok_or_else(|| Error::InvalidModel {
                message: format!("ELK returned unknown state node {}", node.id),
            })?;
        if let Some(frame) = frames.get(node.id.as_str()) {
            node.x = frame.x;
            node.y = frame.y;
            node.width = frame.width;
            node.height = frame.height;
        }
        if source.kind == elk::NodeKind::Group {
            let semantic = source_nodes[node.id.as_str()];
            let label = painted_group_labels[node.id.as_str()];
            output.clusters.push(LayoutCluster {
                id: node.id.clone(),
                x: node.x,
                y: node.y,
                width: node.width,
                height: node.height,
                diff: 0.0,
                offset_y: 0.0,
                title: source.label_text.clone().unwrap_or_default(),
                title_label: LayoutLabel {
                    x: node.x,
                    y: node.y - node.height / 2.0 + label.height / 2.0,
                    width: label.width,
                    height: label.height,
                },
                requested_dir: semantic.dir.clone(),
                effective_dir: normalize_dir(semantic.dir.as_deref().unwrap_or(&model.direction)),
                padding: source.container.padding,
                title_margin_top: 0.0,
                title_margin_bottom: 0.0,
            });
        }
        output.nodes.push(LayoutNode {
            id: node.id,
            x: node.x,
            y: node.y,
            width: node.width,
            height: node.height,
            is_cluster: source.kind == elk::NodeKind::Group,
            label_width: source.label.map(|label| label.width),
            label_height: source.label.map(|label| label.height),
        });
    }
    for edge in placed.edges {
        let source = graph_edges
            .get(edge.id.as_str())
            .ok_or_else(|| Error::InvalidModel {
                message: format!("ELK returned unknown state edge {}", edge.id),
            })?;
        let label = source.label.map(|size| {
            edge.labels
                .first()
                .map(|label| LayoutLabel {
                    x: label.x + label.width / 2.0,
                    y: label.y + label.height / 2.0,
                    width: label.width,
                    height: label.height,
                })
                .unwrap_or(LayoutLabel {
                    x: 0.0,
                    y: 0.0,
                    width: size.width,
                    height: size.height,
                })
        });
        output.edges.push(LayoutEdge {
            id: edge.id,
            from: source.source.clone(),
            to: source.target.clone(),
            from_cluster: (graph_nodes[source.source.as_str()].kind == elk::NodeKind::Group)
                .then(|| source.source.clone()),
            to_cluster: (graph_nodes[source.target.as_str()].kind == elk::NodeKind::Group)
                .then(|| source.target.clone()),
            points: edge
                .points
                .into_iter()
                .map(|p| LayoutPoint { x: p.x, y: p.y })
                .collect(),
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
    prepare_paint_paths(&mut output, &source_nodes, &source_edges, config, work)?;
    output.nodes.sort_by(|a, b| a.id.cmp(&b.id));
    output.edges.sort_by(|a, b| a.id.cmp(&b.id));
    output.clusters.sort_by(|a, b| a.id.cmp(&b.id));
    Ok(output)
}

fn prepare_paint_paths(
    layout: &mut StateDiagramLayout,
    source_nodes: &HashMap<&str, &super::StateNode>,
    source_edges: &HashMap<&str, &merman_core::diagrams::state::StateDiagramRenderEdge>,
    config: &Value,
    work: &mut OperationLayoutWorkControl,
) -> Result<()> {
    use crate::elk_edge_geometry::{self as geometry, Outline, Shape};
    let indices: HashMap<_, _> = layout
        .nodes
        .iter()
        .enumerate()
        .map(|(i, n)| (n.id.clone(), i))
        .collect();
    // The upstream adapter centers small shapes on the first attached side anchor before
    // computing any route. This matters for start/end circles and fork/join bars.
    let mut aligned = std::collections::HashSet::new();
    for edge in &layout.edges {
        for (id, anchor) in [
            (&edge.from, edge.points.first()),
            (&edge.to, edge.points.last()),
        ] {
            work.charge_adapter(1)?;
            let (Some(&index), Some(anchor)) = (indices.get(id), anchor) else {
                continue;
            };
            let node = &mut layout.nodes[index];
            let horizontal_side = (anchor.y - node.y + node.height / 2.0).abs() <= 0.5
                || (anchor.y - node.y - node.height / 2.0).abs() <= 0.5;
            if (if horizontal_side {
                node.width
            } else {
                node.height
            }) >= 24.0
                || !aligned.insert(index)
            {
                continue;
            }
            let delta = if horizontal_side {
                anchor.x - node.x
            } else {
                anchor.y - node.y
            };
            if delta.abs() >= 0.01 {
                if horizontal_side {
                    node.x += delta;
                } else {
                    node.y += delta;
                }
            }
        }
    }
    let mut paths = Vec::with_capacity(layout.edges.len());
    for edge in &layout.edges {
        work.charge_adapter(work.checked_add(edge.points.len(), 50)?)?;
        let (Some(&start_index), Some(&end_index), Some(start_source), Some(end_source)) = (
            indices.get(&edge.from),
            indices.get(&edge.to),
            source_nodes.get(edge.from.as_str()),
            source_nodes.get(edge.to.as_str()),
        ) else {
            paths.push(edge.points.clone());
            continue;
        };
        let start = Shape {
            intersection: None,
            node: &layout.nodes[start_index],
            outline: match start_source.shape.as_str() {
                "stateStart" | "stateEnd" => Outline::Ellipse,
                "choice" => Outline::Diamond,
                _ => Outline::Rect,
            },
        };
        let end = Shape {
            intersection: None,
            node: &layout.nodes[end_index],
            outline: match end_source.shape.as_str() {
                "stateStart" | "stateEnd" => Outline::Ellipse,
                "choice" => Outline::Diamond,
                _ => Outline::Rect,
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
        if !edge.points.is_empty()
            && let Some(source_edge) = source_edges.get(edge.id.as_str())
        {
            geometry::marker_segment(&mut points, end, Some(&source_edge.arrow_type_end), false);
        }
        paths.push(points);
    }
    if config
        .pointer("/elk/straightenEdges")
        .and_then(Value::as_bool)
        != Some(false)
    {
        let changes =
            crate::elk_terminal_jogs::straighten_edge_terminals_with_runs(&mut paths, |units| {
                work.charge_adapter(units)
            })?;
        for change in changes {
            if let Some(label) = layout.edges[change.route_index].label.as_mut() {
                crate::elk_terminal_jogs::reproject_label(label, &change.runs);
            }
        }
    }
    for (edge, points) in layout.edges.iter_mut().zip(paths) {
        if edge.points.is_empty()
            && let (Some(label), Some(first), Some(last)) =
                (edge.label.as_mut(), points.first(), points.last())
        {
            label.x = (first.x + last.x) / 2.0;
            label.y = (first.y + last.y) / 2.0;
        }
        layout.elk_edge_paths.insert(edge.id.clone(), points);
    }
    crate::elk_terminal_jogs::separate_opposite_edge_labels(
        layout
            .edges
            .iter_mut()
            .map(|edge| (edge.from.as_str(), edge.to.as_str(), edge.label.as_mut())),
        |units| work.charge_adapter(units),
    )?;
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
                layout
                    .elk_edge_paths
                    .values()
                    .flatten()
                    .map(|point| (point.x, point.y)),
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
    Ok(())
}
