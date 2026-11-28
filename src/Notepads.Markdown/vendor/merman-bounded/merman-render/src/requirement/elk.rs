//! Requirement projection of Mermaid 12's registered ELK renderer.
//!
//! Source: packages/mermaid/src/diagrams/requirement/requirementRenderer.ts and
//! rendering-util/layout-algorithms/elk/render.ts at 98a0945418c76238f15df2afaddbba4272656c3b.

use super::*;
use crate::elk_edge_geometry::{self as geometry, Outline, Shape};
use merman_layout_elk as elk;

type MeasuredGraph = Graph<NodeLabel, EdgeLabel, GraphLabel>;

pub(super) fn layout(
    graph: &mut MeasuredGraph,
    config: &Value,
    operation_seed: elk::ElkOperationSeed,
    work: &mut OperationLayoutWorkControl,
) -> Result<()> {
    let keys = graph.edge_keys();
    let mut input = elk::Graph {
        id: "root".to_owned(),
        direction: match graph.graph().rankdir {
            RankDir::TB => elk::Direction::Down,
            RankDir::BT => elk::Direction::Up,
            RankDir::LR => elk::Direction::Right,
            RankDir::RL => elk::Direction::Left,
        },
        nodes: graph
            .nodes()
            .filter_map(|id| {
                graph.node(id).map(|node| elk::Node {
                    id: id.to_owned(),
                    kind: elk::NodeKind::Leaf,
                    container: Default::default(),
                    label_text: None,
                    width: node.width,
                    height: node.height,
                    parent: None,
                    direction: None,
                    hierarchy_handling: None,
                    layer_constraint: None,
                    port_alignment: None,
                    label: None,
                })
            })
            .collect(),
        edges: keys
            .iter()
            .filter_map(|key| {
                graph.edge_by_key(key).map(|edge| elk::Edge {
                    id: key.name.clone().expect("Requirement edges are named"),
                    source: key.v.clone(),
                    target: key.w.clone(),
                    label: Some(elk::Label {
                        width: edge.width,
                        height: edge.height,
                    }),
                    minlen: 1,
                    terminal_labels: Vec::new(),
                    inside_self_loops_yo: false,
                })
            })
            .collect(),
        // The shared Mermaid ELK renderer sets spacing.baseValue=40, independently of
        // the Requirement renderer's Dagre nodeSpacing/rankSpacing fields.
        spacing: elk::Spacing::default(),
        options: crate::elk_options::layout_options(config),
    };
    if crate::config::config_bool(config, &["elk", "keepEntryNodeOnTop"]).unwrap_or(false) {
        crate::elk_adapter::apply_cyclic_entry_constraints(
            &mut input.nodes,
            input
                .edges
                .iter()
                .map(|edge| (edge.source.as_str(), edge.target.as_str())),
            &mut Some(&mut *work),
        )?;
    }
    work.charge_adapter(input.nodes.len().saturating_add(input.edges.len()))?;
    let oriented = crate::elk_feedback_edges::orient_feedback_edges(
        &mut input,
        config,
        &mut Some(&mut *work),
    )?;
    let mut placed =
        elk::layout_with_operation_seed_and_work_control(oriented.graph(), operation_seed, work)
            .map_err(|error| work.map_elk_error_with_context(error, "Requirement ELK"))?;
    oriented.restore(&mut placed, &mut Some(&mut *work))?;
    for placed in placed.nodes {
        let node = graph
            .node_mut(&placed.id)
            .ok_or_else(|| Error::InvalidModel {
                message: format!("ELK returned unknown Requirement node {}", placed.id),
            })?;
        node.x = Some(placed.x);
        node.y = Some(placed.y);
        node.width = placed.width;
        node.height = placed.height;
    }
    let keys_by_id: HashMap<_, _> = keys
        .iter()
        .filter_map(|key| key.name.as_deref().map(|id| (id, key)))
        .collect();
    for placed in placed.edges {
        let key = keys_by_id
            .get(placed.id.as_str())
            .ok_or_else(|| Error::InvalidModel {
                message: format!("ELK returned unknown Requirement edge {}", placed.id),
            })?;
        let edge = graph
            .edge_mut_by_key(key)
            .ok_or_else(|| Error::InvalidModel {
                message: format!("missing measured Requirement edge {}", placed.id),
            })?;
        edge.points = placed
            .points
            .into_iter()
            .map(|point| dugong::Point {
                x: point.x,
                y: point.y,
            })
            .collect();
        if let Some(label) = placed.labels.first() {
            edge.x = Some(label.x + label.width / 2.0);
            edge.y = Some(label.y + label.height / 2.0);
        }
    }
    Ok(())
}

/// Prepare the source renderer's clipped paint routes without rewriting provider sections.
pub(super) fn render_layout(
    layout: &RequirementDiagramLayout,
    graph: &MeasuredGraph,
    config: &Value,
    work: &mut OperationLayoutWorkControl,
) -> Result<RequirementDiagramLayout> {
    let mut render = layout.clone();
    // Requirement's public geometry uses top-left nodes; the shared shape adapter uses centers.
    let centers: HashMap<_, _> = layout
        .nodes
        .iter()
        .map(|node| {
            let mut center = node.clone();
            center.x += center.width / 2.0;
            center.y += center.height / 2.0;
            (node.id.as_str(), center)
        })
        .collect();
    let mut routes = Vec::with_capacity(layout.edges.len());
    for edge in &layout.edges {
        work.charge_adapter(edge.points.len().saturating_add(2))?;
        let start = centers
            .get(edge.from.as_str())
            .ok_or_else(|| Error::InvalidModel {
                message: format!("missing Requirement edge source {}", edge.from),
            })?;
        let end = centers
            .get(edge.to.as_str())
            .ok_or_else(|| Error::InvalidModel {
                message: format!("missing Requirement edge target {}", edge.to),
            })?;
        let mut points = Vec::with_capacity(edge.points.len() + 2);
        points.push(LayoutPoint {
            x: start.x,
            y: start.y,
        });
        points.extend_from_slice(&edge.points);
        points.push(LayoutPoint { x: end.x, y: end.y });
        routes.push(geometry::sanitize(
            &points,
            Shape {
                intersection: None,
                node: start,
                outline: Outline::Rect,
            },
            Shape {
                intersection: None,
                node: end,
                outline: Outline::Rect,
            },
        ));
    }
    if config
        .pointer("/elk/straightenEdges")
        .and_then(Value::as_bool)
        != Some(false)
    {
        let changes =
            crate::elk_terminal_jogs::straighten_edge_terminals_with_runs(&mut routes, |units| {
                work.charge_adapter(units)
            })?;
        for change in changes {
            if let Some(label) = render.edges[change.route_index].label.as_mut() {
                crate::elk_terminal_jogs::reproject_label(label, &change.runs);
            }
        }
    }
    for ((edge, raw), points) in render.edges.iter_mut().zip(&layout.edges).zip(routes) {
        if raw.points.is_empty() {
            let key = EdgeKey::new(&edge.from, &edge.to, Some(&edge.id));
            if let (Some(measured), Some(first), Some(last)) =
                (graph.edge_by_key(&key), points.first(), points.last())
            {
                edge.label = Some(LayoutLabel {
                    x: (first.x + last.x) / 2.0,
                    y: (first.y + last.y) / 2.0,
                    width: measured.width,
                    height: measured.height,
                });
            }
        }
        edge.points = points;
    }
    crate::elk_terminal_jogs::separate_opposite_edge_labels(
        render
            .edges
            .iter_mut()
            .map(|edge| (edge.from.as_str(), edge.to.as_str(), edge.label.as_mut())),
        |units| work.charge_adapter(units),
    )?;
    // The SVG renderer reconstructs final viewport bounds from these painted paths and labels.
    Ok(render)
}
