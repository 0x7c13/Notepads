// Modifications Copyright (c) 2026, Jiaqi (0x7c13) Liu. See project LICENSE.
use crate::config::config_bool;
use crate::elk_options::layout_options as elk_layout_options;
use crate::layout_work::ElkOperationWorkControl;
use crate::math::MathRenderer;
use crate::model::{
    FlowchartLayout, LayoutCluster, LayoutEdge, LayoutLabel, LayoutNode, LayoutPoint,
};
use crate::resources::OperationWorkMeter;
use crate::text::{TextMeasurer, TextStyle, WrapMode};
use crate::{Error, Result};
use merman_core::{MermaidConfig, ParsedDiagramRender, RenderSemanticModel};
use merman_layout_elk as elk;
use std::collections::{HashMap, HashSet, VecDeque};
use std::sync::Arc;

use merman_core::diagrams::flowchart::{
    FlowEdge, FlowNode, FlowSubgraph, FlowchartModel, FlowchartRenderContext,
};
#[cfg(test)]
use merman_core::diagrams::flowchart::{FlowEdgeMarker, FlowEdgeStroke, FlowEdgeVisibility};

use super::config::{FlowchartConfigView, FlowchartLayoutSettings};
use super::label::compute_bounds;
use super::node::{NodeLayoutDimensionsRequest, node_layout_dimensions};
use super::{
    FlowchartLabelMetricsRequest, FlowchartRenderModelRef, FlowchartSvgLabelOwner,
    FlowchartSvgLabelSidecarBuilder, FlowchartSvgWidthMode,
    flowchart_apply_html_node_class_box_metrics, flowchart_effective_edge_label_text_style,
    flowchart_effective_text_style_for_classes, flowchart_effective_text_style_for_node_classes,
    flowchart_node_svg_width_mode, measure_flowchart_svg_label_for_layout,
    measure_flowchart_svg_label_for_layout_with_metrics_style,
};

pub(crate) struct FlowchartElkLayoutExecution<'a> {
    measurer: &'a dyn TextMeasurer,
    math_renderer: Option<&'a (dyn MathRenderer + Send + Sync)>,
    operation_seed: elk::ElkOperationSeed,
    svg_label_sidecar: Option<&'a FlowchartSvgLabelSidecarBuilder>,
    work_meter: Arc<OperationWorkMeter>,
}

impl<'a> FlowchartElkLayoutExecution<'a> {
    pub(crate) fn new(
        measurer: &'a dyn TextMeasurer,
        math_renderer: Option<&'a (dyn MathRenderer + Send + Sync)>,
        operation_seed: elk::ElkOperationSeed,
        svg_label_sidecar: Option<&'a FlowchartSvgLabelSidecarBuilder>,
        work_meter: Arc<OperationWorkMeter>,
    ) -> Self {
        Self {
            measurer,
            math_renderer,
            operation_seed,
            svg_label_sidecar,
            work_meter,
        }
    }
}

#[cfg(test)]
pub(crate) fn layout_flowchart_elk_typed(
    model: &FlowchartModel,
    effective_config: &MermaidConfig,
    measurer: &dyn TextMeasurer,
    math_renderer: Option<&(dyn MathRenderer + Send + Sync)>,
) -> Result<FlowchartLayout> {
    let render_label_sources = FlowchartRenderContext::default();
    let mut graph = build_flowchart_elk_graph_with_render_labels(
        model,
        &render_label_sources,
        effective_config,
        measurer,
        math_renderer,
    )?;
    let mut no_work = None;
    let orientation = crate::elk_feedback_edges::orient_feedback_edges(
        &mut graph,
        effective_config.as_value(),
        &mut no_work,
    )?;
    let mut layout = elk::layout(orientation.graph()).map_err(|err| Error::InvalidModel {
        message: format!("ELK layout failed: {err}"),
    })?;
    orientation.restore(&mut layout, &mut no_work)?;
    flowchart_layout_from_elk_with_render_labels(
        model,
        &render_label_sources,
        effective_config,
        &graph,
        layout,
    )
}

/// Lays out a Flowchart through ELK using the render operation's captured seed.
///
/// This is intentionally crate-private. The public typed function above is a raw diagnostic API
/// and fails closed when source configuration uses ELK's `randomSeed = 0` sentinel.
#[cfg(test)]
pub(crate) fn layout_flowchart_elk_typed_with_operation_seed(
    model: &FlowchartModel,
    effective_config: &MermaidConfig,
    measurer: &dyn TextMeasurer,
    math_renderer: Option<&(dyn MathRenderer + Send + Sync)>,
    operation_seed: elk::ElkOperationSeed,
    work_meter: Arc<OperationWorkMeter>,
) -> Result<FlowchartLayout> {
    layout_flowchart_elk_typed_with_render_labels_and_operation_seed(
        model,
        &FlowchartRenderContext::default(),
        effective_config,
        FlowchartElkLayoutExecution::new(measurer, math_renderer, operation_seed, None, work_meter),
    )
}

pub(crate) fn layout_flowchart_elk_typed_with_render_labels_and_operation_seed(
    model: &FlowchartModel,
    render_label_sources: &FlowchartRenderContext,
    effective_config: &MermaidConfig,
    execution: FlowchartElkLayoutExecution<'_>,
) -> Result<FlowchartLayout> {
    let mut work_control = ElkOperationWorkControl::new(execution.work_meter);
    let mut graph = build_flowchart_elk_graph_with_render_labels_and_work_control(
        model,
        render_label_sources,
        effective_config,
        execution.measurer,
        execution.math_renderer,
        execution.svg_label_sidecar,
        Some(&mut work_control),
    )?;
    let orientation = {
        let mut layout_work = Some(&mut work_control);
        crate::elk_feedback_edges::orient_feedback_edges(
            &mut graph,
            effective_config.as_value(),
            &mut layout_work,
        )?
    };
    let mut layout = match elk::layout_with_operation_seed_and_work_control(
        orientation.graph(),
        execution.operation_seed,
        &mut work_control,
    ) {
        Ok(layout) => layout,
        Err(error) => return Err(work_control.map_elk_error(error)),
    };
    let mut layout_work = Some(&mut work_control);
    orientation.restore(&mut layout, &mut layout_work)?;
    flowchart_layout_from_elk_with_render_labels_and_work_control(
        model,
        render_label_sources,
        effective_config,
        &graph,
        layout,
        Some(&mut work_control),
    )
}

#[cfg(test)]
fn flowchart_layout_from_elk(
    model: &FlowchartModel,
    effective_config: &MermaidConfig,
    graph: &elk::Graph,
    layout: elk::LayoutResult,
) -> Result<FlowchartLayout> {
    flowchart_layout_from_elk_with_render_labels(
        model,
        &FlowchartRenderContext::default(),
        effective_config,
        graph,
        layout,
    )
}

#[cfg(test)]
fn flowchart_layout_from_elk_with_render_labels(
    model: &FlowchartModel,
    render_label_sources: &FlowchartRenderContext,
    effective_config: &MermaidConfig,
    graph: &elk::Graph,
    layout: elk::LayoutResult,
) -> Result<FlowchartLayout> {
    flowchart_layout_from_elk_with_render_labels_and_work_control(
        model,
        render_label_sources,
        effective_config,
        graph,
        layout,
        None,
    )
}

#[cfg(test)]
fn flowchart_layout_from_elk_with_work_control(
    model: &FlowchartModel,
    effective_config: &MermaidConfig,
    graph: &elk::Graph,
    layout: elk::LayoutResult,
    work_control: Option<&mut ElkOperationWorkControl>,
) -> Result<FlowchartLayout> {
    flowchart_layout_from_elk_with_render_labels_and_work_control(
        model,
        &FlowchartRenderContext::default(),
        effective_config,
        graph,
        layout,
        work_control,
    )
}

fn flowchart_layout_from_elk_with_render_labels_and_work_control(
    model: &FlowchartModel,
    render_label_sources: &FlowchartRenderContext,
    effective_config: &MermaidConfig,
    graph: &elk::Graph,
    layout: elk::LayoutResult,
    mut work_control: Option<&mut ElkOperationWorkControl>,
) -> Result<FlowchartLayout> {
    let render_model = FlowchartRenderModelRef::new(model, render_label_sources);
    let model = &render_model;
    let effective_config_value = effective_config.as_value();
    let FlowchartLayoutSettings {
        cluster_padding,
        title_margin_top,
        title_margin_bottom,
        ..
    } = FlowchartConfigView::new(effective_config_value).layout_settings();

    let source_index_work = checked_adapter_add(
        &work_control,
        checked_adapter_add(&work_control, graph.nodes.len(), graph.edges.len())?,
        model.nodes.len(),
    )?;
    charge_adapter_work(&mut work_control, source_index_work)?;
    let source_node_by_id: HashMap<&str, &elk::Node> = graph
        .nodes
        .iter()
        .map(|node| (node.id.as_str(), node))
        .collect();
    let source_edge_by_id: HashMap<&str, &elk::Edge> = graph
        .edges
        .iter()
        .map(|edge| (edge.id.as_str(), edge))
        .collect();

    let source_shape_by_id: HashMap<&str, Option<&str>> = model
        .nodes
        .iter()
        .map(|node| (node.id.as_str(), node.layout_shape.as_deref()))
        .collect();

    let group_frames = crate::elk_adapter::drawing_group_frames(
        graph,
        &layout,
        |node| node.label.map_or(0.0, |label| label.width) + node.container.padding,
        &mut work_control,
    )?;

    charge_adapter_work(&mut work_control, layout.nodes.len())?;
    let mut out_nodes = Vec::with_capacity(layout.nodes.len());
    for node in layout.nodes {
        let Some(source) = source_node_by_id.get(node.id.as_str()).copied() else {
            return Err(Error::InvalidModel {
                message: format!("ELK layout returned unknown node {}", node.id),
            });
        };
        // Frame contraction changes only the painted container. Provider edge points already
        // use the original owner's absolute origin and must not follow this translation.
        let frame = group_frames.get(node.id.as_str());
        out_nodes.push(LayoutNode {
            id: node.id,
            x: frame.map_or(node.x, |frame| frame.x),
            y: frame.map_or(node.y, |frame| frame.y),
            width: frame.map_or(node.width, |frame| frame.width),
            height: frame.map_or(node.height, |frame| frame.height),
            is_cluster: source.kind == elk::NodeKind::Group,
            label_width: source.label.map(|label| label.width),
            label_height: source.label.map(|label| label.height),
        });
    }

    // Mermaid's applyElkEdgeLayout aligns small nodes before constructing any edge points.
    // Keep ELK's routes intact: a clamped port defines the line onto which its node must move.
    let alignment_work = checked_adapter_add(
        &work_control,
        out_nodes.len(),
        checked_adapter_mul(&work_control, layout.edges.len(), 3)?,
    )?;
    charge_adapter_work(&mut work_control, alignment_work)?;
    {
        let mut nodes_to_align = HashMap::with_capacity(out_nodes.len());
        for node in &mut out_nodes {
            if let Some(source) = source_node_by_id.get(node.id.as_str()) {
                nodes_to_align.insert(source.id.as_str(), (node, false));
            }
        }
        for edge in &layout.edges {
            let (Some(start), Some(end)) = (edge.points.first(), edge.points.last()) else {
                continue;
            };
            let Some(source) = source_edge_by_id.get(edge.id.as_str()) else {
                return Err(Error::InvalidModel {
                    message: format!("ELK layout returned unknown edge {}", edge.id),
                });
            };
            if !nodes_to_align.contains_key(source.source.as_str())
                || !nodes_to_align.contains_key(source.target.as_str())
            {
                continue;
            }
            for (id, anchor) in [
                (source.source.as_str(), start),
                (source.target.as_str(), end),
            ] {
                if source_shape_by_id.get(id).copied().flatten() == Some("rect33") {
                    continue;
                }
                if let Some((node, aligned)) = nodes_to_align.get_mut(id) {
                    align_degenerate_node_to_anchor(node, anchor, aligned);
                }
            }
        }
    }

    charge_adapter_work(&mut work_control, out_nodes.len())?;
    let layout_node_by_id: HashMap<&str, &LayoutNode> = out_nodes
        .iter()
        .map(|node| (node.id.as_str(), node))
        .collect();
    let diagram_direction = model.direction.as_deref().unwrap_or("TB");
    charge_adapter_work(&mut work_control, model.subgraphs.len())?;
    let mut clusters = Vec::new();
    for sg in &model.subgraphs {
        if model.collapsed_replacement(sg.id.as_str()).is_some()
            || model.is_subgraph_collapsed(sg.id.as_str())
        {
            continue;
        }
        let Some(node) = layout_node_by_id.get(sg.id.as_str()).copied() else {
            return Err(Error::InvalidModel {
                message: format!("missing ELK layout cluster {}", sg.id),
            });
        };
        let label = source_node_by_id
            .get(sg.id.as_str())
            .and_then(|node| node.label)
            .unwrap_or(elk::Label {
                width: 1.0,
                height: 1.0,
            });
        let title_label = LayoutLabel {
            x: node.x,
            y: node.y - node.height / 2.0 + title_margin_top + label.height / 2.0,
            width: label.width,
            height: label.height,
        };
        let title_w = label.width.max(1.0);
        let diff = if node.width <= title_w {
            (title_w - node.width) / 2.0 - cluster_padding / 2.0
        } else {
            -cluster_padding / 2.0
        };
        clusters.push(LayoutCluster {
            id: sg.id.clone(),
            x: node.x,
            y: node.y,
            width: node.width,
            height: node.height,
            diff,
            offset_y: label.height - cluster_padding / 2.0,
            title: sg.title.clone(),
            title_label,
            requested_dir: sg.dir.as_ref().map(|dir| normalize_flow_direction(dir)),
            effective_dir: sg
                .dir
                .as_deref()
                .map(normalize_flow_direction)
                .unwrap_or_else(|| normalize_flow_direction(diagram_direction)),
            padding: cluster_padding,
            title_margin_top,
            title_margin_bottom,
        });
    }
    let cluster_sort_work = comparison_sort_work_units(clusters.len(), &work_control)?;
    charge_adapter_work(&mut work_control, cluster_sort_work)?;
    clusters.sort_by(|a, b| a.id.cmp(&b.id));

    let edge_projection_work = layout.edges.iter().try_fold(0usize, |total, edge| {
        let point_walks = checked_adapter_mul(&work_control, edge.points.len(), 2)?;
        let edge_work = checked_adapter_add(
            &work_control,
            checked_adapter_add(&work_control, point_walks, edge.labels.len())?,
            3,
        )?;
        checked_adapter_add(&work_control, total, edge_work)
    })?;
    // Reject the complete user-sized projection tranche before reserving its output vector.
    charge_adapter_work(&mut work_control, edge_projection_work)?;
    let mut out_edges = Vec::with_capacity(layout.edges.len());
    for edge in layout.edges {
        let Some(source) = source_edge_by_id.get(edge.id.as_str()).copied() else {
            return Err(Error::InvalidModel {
                message: format!("ELK layout returned unknown edge {}", edge.id),
            });
        };
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
                .first()
                .map(edge_label_layout)
                .or_else(|| edge_label_position(&points, source_label))
        });
        out_edges.push(LayoutEdge {
            id: edge.id,
            from: source.source.clone(),
            to: source.target.clone(),
            from_cluster: endpoint_cluster(source.source.as_str(), &layout_node_by_id),
            to_cluster: endpoint_cluster(source.target.as_str(), &layout_node_by_id),
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

    let bounds_points = out_edges.iter().try_fold(
        checked_adapter_mul(&work_control, out_nodes.len(), 2)?,
        |total, edge| {
            let label_points = usize::from(edge.label.is_some()) * 2;
            checked_adapter_add(
                &work_control,
                total,
                checked_adapter_add(&work_control, edge.points.len(), label_points)?,
            )
        },
    )?;
    let bounds_work = checked_adapter_mul(&work_control, bounds_points, 2)?;
    charge_adapter_work(&mut work_control, bounds_work)?;
    let bounds = compute_bounds(&out_nodes, &out_edges);
    let dom_work = checked_adapter_add(
        &work_control,
        checked_adapter_mul(&work_control, graph.nodes.len(), 2)?,
        1,
    )?;
    charge_adapter_work(&mut work_control, dom_work)?;
    let dom_node_order_by_root = flowchart_elk_dom_node_order_by_root(graph);

    Ok(FlowchartLayout {
        nodes: out_nodes,
        edges: out_edges,
        clusters,
        bounds,
        dom_node_order_by_root,
        uses_elk_adapter_dom: true,
    })
}

// Pinned Mermaid elk/render.ts: alignDegenerateNodeToAnchor. The 12px margin is also
// configured by merman-layout-elk; only sides shorter than twice that margin are affected.
fn align_degenerate_node_to_anchor(node: &mut LayoutNode, anchor: &elk::Point, aligned: &mut bool) {
    let top = node.y - node.height / 2.0;
    let bottom = node.y + node.height / 2.0;
    let along_width = (anchor.y - top).abs() <= 0.5 || (anchor.y - bottom).abs() <= 0.5;
    let side = if along_width { node.width } else { node.height };
    if side >= 24.0 || *aligned {
        return;
    }
    // Even a negligible first adjustment claims the node; later edges must not move it.
    *aligned = true;
    let delta = if along_width {
        anchor.x - node.x
    } else {
        anchor.y - node.y
    };
    if delta.abs() < 0.01 {
        return;
    }
    if along_width {
        node.x += delta;
    } else {
        node.y += delta;
    }
}

fn flowchart_elk_dom_node_order_by_root(graph: &elk::Graph) -> HashMap<String, Vec<String>> {
    let ids = mermaid_elk_adapter_dom_order(graph);
    std::iter::once((String::new(), ids)).collect()
}

fn mermaid_elk_adapter_dom_order(graph: &elk::Graph) -> Vec<String> {
    let mut children_by_parent: HashMap<Option<&str>, Vec<&elk::Node>> = HashMap::new();
    for node in &graph.nodes {
        children_by_parent
            .entry(node.parent.as_deref())
            .or_default()
            .push(node);
    }
    let mut out = Vec::with_capacity(graph.nodes.len());
    let mut stack = children_by_parent
        .get(&None)
        .into_iter()
        .flat_map(|children| children.iter().rev().copied())
        .collect::<Vec<_>>();
    while let Some(node) = stack.pop() {
        out.push(node.id.clone());
        if node.kind == elk::NodeKind::Group
            && let Some(children) = children_by_parent.get(&Some(node.id.as_str()))
        {
            stack.extend(children.iter().rev().copied());
        }
    }
    out
}

fn normalize_flow_direction(dir: &str) -> String {
    let upper = dir.trim().to_uppercase();
    if upper == "TD" {
        "TB".to_string()
    } else {
        upper
    }
}

fn endpoint_cluster(id: &str, layout_node_by_id: &HashMap<&str, &LayoutNode>) -> Option<String> {
    layout_node_by_id
        .get(id)
        .filter(|node| node.is_cluster)
        .map(|node| node.id.clone())
}

fn edge_label_layout(label: &elk::EdgeLabelLayout) -> LayoutLabel {
    LayoutLabel {
        x: label.x + label.width / 2.0,
        y: label.y + label.height / 2.0,
        width: label.width,
        height: label.height,
    }
}

fn edge_label_position(points: &[LayoutPoint], label: elk::Label) -> Option<LayoutLabel> {
    let point = polyline_midpoint(points)?;
    Some(LayoutLabel {
        x: point.x,
        y: point.y,
        width: label.width,
        height: label.height,
    })
}

fn polyline_midpoint(points: &[LayoutPoint]) -> Option<LayoutPoint> {
    match points {
        [] => None,
        [single] => Some(single.clone()),
        _ => {
            let total = points
                .windows(2)
                .map(|pair| (pair[1].x - pair[0].x).hypot(pair[1].y - pair[0].y))
                .sum::<f64>();
            if !total.is_finite() || total <= 0.0 {
                return points.first().cloned();
            }
            let mut remaining = total / 2.0;
            for pair in points.windows(2) {
                let len = (pair[1].x - pair[0].x).hypot(pair[1].y - pair[0].y);
                if len <= 0.0 {
                    continue;
                }
                if remaining > len {
                    remaining -= len;
                    continue;
                }
                let t = remaining / len;
                return Some(LayoutPoint {
                    x: pair[0].x + (pair[1].x - pair[0].x) * t,
                    y: pair[0].y + (pair[1].y - pair[0].y) * t,
                });
            }
            points.last().cloned()
        }
    }
}

/// Builds the ELK input graph from the complete parser-owned Flowchart render artifact.
///
/// Flowchart labels retain source spelling that is not recoverable from the public semantic
/// model alone (for example an authored `&lt;` versus a literal `<`). Accepting the parsed artifact
/// keeps that provenance paired with the effective configuration and prevents diagnostics from
/// silently measuring a different createText payload than the normal render pipeline.
pub fn build_flowchart_elk_graph(
    parsed: &ParsedDiagramRender,
    measurer: &dyn TextMeasurer,
    math_renderer: Option<&(dyn MathRenderer + Send + Sync)>,
) -> Result<elk::Graph> {
    let RenderSemanticModel::Flowchart(model) = parsed.model() else {
        return Err(Error::InvalidModel {
            message: format!(
                "expected Flowchart render model, got {}",
                parsed.model().kind()
            ),
        });
    };
    let empty_sources = FlowchartRenderContext::default();
    let render_label_sources = parsed.flowchart_render_context().unwrap_or(&empty_sources);
    build_flowchart_elk_graph_with_render_labels(
        model,
        render_label_sources,
        &parsed.metadata().effective_config,
        measurer,
        math_renderer,
    )
}

#[cfg(test)]
fn build_flowchart_elk_graph_from_semantic(
    model: &FlowchartModel,
    effective_config: &MermaidConfig,
    measurer: &dyn TextMeasurer,
    math_renderer: Option<&(dyn MathRenderer + Send + Sync)>,
) -> Result<elk::Graph> {
    build_flowchart_elk_graph_with_render_labels(
        model,
        &FlowchartRenderContext::default(),
        effective_config,
        measurer,
        math_renderer,
    )
}

fn build_flowchart_elk_graph_with_render_labels(
    model: &FlowchartModel,
    render_label_sources: &FlowchartRenderContext,
    effective_config: &MermaidConfig,
    measurer: &dyn TextMeasurer,
    math_renderer: Option<&(dyn MathRenderer + Send + Sync)>,
) -> Result<elk::Graph> {
    build_flowchart_elk_graph_with_render_labels_and_work_control(
        model,
        render_label_sources,
        effective_config,
        measurer,
        math_renderer,
        None,
        None,
    )
}

#[cfg(test)]
fn build_flowchart_elk_graph_with_work_control(
    model: &FlowchartModel,
    effective_config: &MermaidConfig,
    measurer: &dyn TextMeasurer,
    math_renderer: Option<&(dyn MathRenderer + Send + Sync)>,
    work_control: Option<&mut ElkOperationWorkControl>,
) -> Result<elk::Graph> {
    build_flowchart_elk_graph_with_render_labels_and_work_control(
        model,
        &FlowchartRenderContext::default(),
        effective_config,
        measurer,
        math_renderer,
        None,
        work_control,
    )
}

fn charge_adapter_work(
    work_control: &mut Option<&mut ElkOperationWorkControl>,
    units: usize,
) -> Result<()> {
    match work_control.as_deref_mut() {
        Some(work_control) => work_control.charge_adapter(units),
        None => Ok(()),
    }
}

fn checked_adapter_add(
    work_control: &Option<&mut ElkOperationWorkControl>,
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

fn checked_adapter_mul(
    work_control: &Option<&mut ElkOperationWorkControl>,
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

fn comparison_sort_work_units(
    items: usize,
    work_control: &Option<&mut ElkOperationWorkControl>,
) -> Result<usize> {
    if items < 2 {
        return Ok(0);
    }
    let levels = usize::BITS as usize - (items - 1).leading_zeros() as usize;
    checked_adapter_mul(work_control, items, levels)
}

fn build_flowchart_elk_graph_with_render_labels_and_work_control(
    model: &FlowchartModel,
    render_label_sources: &FlowchartRenderContext,
    effective_config: &MermaidConfig,
    measurer: &dyn TextMeasurer,
    math_renderer: Option<&(dyn MathRenderer + Send + Sync)>,
    svg_label_sidecar: Option<&FlowchartSvgLabelSidecarBuilder>,
    mut work_control: Option<&mut ElkOperationWorkControl>,
) -> Result<elk::Graph> {
    // Mermaid's FlowDB projects collapsed subgraphs before handing data to any layout backend.
    // Keep the public typed model lossless, but give ELK the same visible graph. The projected
    // model retains subgraph declaration ordinals so source-backed title bindings remain valid.
    if let Some(projected_model) =
        project_collapsed_flowchart_model(model, render_label_sources, &mut work_control)?
    {
        return build_flowchart_elk_graph_with_render_labels_and_work_control_inner(
            &projected_model,
            render_label_sources,
            effective_config,
            measurer,
            math_renderer,
            None,
            work_control,
        );
    }

    build_flowchart_elk_graph_with_render_labels_and_work_control_inner(
        model,
        render_label_sources,
        effective_config,
        measurer,
        math_renderer,
        svg_label_sidecar,
        work_control,
    )
}

fn project_collapsed_flowchart_model(
    model: &FlowchartModel,
    render_label_sources: &FlowchartRenderContext,
    work_control: &mut Option<&mut ElkOperationWorkControl>,
) -> Result<Option<FlowchartModel>> {
    let has_collapsed_projection = model.subgraphs.iter().any(|subgraph| {
        render_label_sources.is_subgraph_collapsed(subgraph.id.as_str())
            || render_label_sources
                .collapsed_replacement(subgraph.id.as_str())
                .is_some()
    });
    if !has_collapsed_projection {
        return Ok(None);
    }

    let projection_work = checked_adapter_add(
        work_control,
        model.nodes.len(),
        checked_adapter_add(
            work_control,
            model.edges.len(),
            checked_adapter_add(
                work_control,
                model.subgraphs.len(),
                model.vertex_calls.len(),
            )?,
        )?,
    )?;
    charge_adapter_work(work_control, projection_work)?;

    let mut projected = model.clone();
    projected.nodes.retain(|node| {
        render_label_sources
            .collapsed_replacement(node.id.as_str())
            .is_none()
    });
    projected.edges = model
        .edges
        .iter()
        .filter_map(|edge| super::project_flowchart_edge(edge, render_label_sources))
        .collect();
    // Keep the original subgraph vector length/order. Render-label sources are keyed by the
    // declaration ordinal, and filtering hidden descendants here would make later titles read
    // the wrong source-backed spelling (especially for nested collapsed subgraphs).
    projected.subgraphs = model
        .subgraphs
        .iter()
        .map(|subgraph| {
            let mut projected = subgraph.clone();
            projected.nodes.retain(|id| {
                render_label_sources
                    .collapsed_replacement(id.as_str())
                    .is_none()
            });
            projected
        })
        .collect();
    projected.vertex_calls.retain(|id| {
        render_label_sources
            .collapsed_replacement(id.as_str())
            .is_none()
    });

    Ok(Some(projected))
}

fn build_flowchart_elk_graph_with_render_labels_and_work_control_inner(
    model: &FlowchartModel,
    render_label_sources: &FlowchartRenderContext,
    effective_config: &MermaidConfig,
    measurer: &dyn TextMeasurer,
    math_renderer: Option<&(dyn MathRenderer + Send + Sync)>,
    svg_label_sidecar: Option<&FlowchartSvgLabelSidecarBuilder>,
    mut work_control: Option<&mut ElkOperationWorkControl>,
) -> Result<elk::Graph> {
    let render_model = FlowchartRenderModelRef::new(model, render_label_sources);
    let model = &render_model;
    // Shape validation only walks nodes. Charge that tranche before the validation scan; the
    // hierarchy and edge tranches are charged by their actual owners below.
    charge_adapter_work(&mut work_control, model.nodes.len())?;
    super::validate_flowchart_model_shapes(model)?;
    let effective_config_value = effective_config.as_value();
    let FlowchartLayoutSettings {
        node_padding,
        state_padding,
        wrapping_width,
        edge_label_wrapping_width,
        edge_html_labels,
        node_wrap_mode,
        edge_wrap_mode,
        cluster_wrap_mode,
        cluster_padding,
        nodesep,
        ranksep,
        text_style,
        html_label_text_style,
        ..
    } = FlowchartConfigView::new(effective_config_value).layout_settings();

    let diagram_direction = model
        .direction
        .as_deref()
        .map(dir_to_elk_direction)
        .unwrap_or_default();

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

    let mut graph = elk::Graph {
        id: "root".to_string(),
        direction: diagram_direction,
        spacing: elk::Spacing {
            node_node: nodesep,
            layer_layer: ranksep,
            group_padding_x: cluster_padding,
            group_padding_y: cluster_padding,
            ..Default::default()
        },
        options: elk_layout_options(effective_config_value),
        ..Default::default()
    };

    charge_adapter_work(&mut work_control, model.subgraphs.len())?;
    let subgraph_ids: HashSet<&str> = model.subgraphs.iter().map(|sg| sg.id.as_str()).collect();
    let parent_by_id = parent_by_id(model, &mut work_control)?;
    let include_children_groups = include_children_groups(model, &parent_by_id, &mut work_control)?;
    let direction_item_count =
        checked_adapter_add(&work_control, model.nodes.len(), model.subgraphs.len())?;
    let node_directions = crate::elk_adapter::resolved_node_directions(
        model
            .nodes
            .iter()
            .map(|node| (node.id.as_str(), None))
            .chain(model.subgraphs.iter().map(|group| {
                (
                    group.id.as_str(),
                    group.dir.as_deref().map(dir_to_elk_direction),
                )
            }))
            .map(|(id, direction)| (id, parent_by_id.get(id).map(String::as_str), direction)),
        direction_item_count,
        diagram_direction,
        &mut work_control,
    )?;

    let cluster_measure_ctx = ElkMeasureContext {
        model: render_model,
        effective_config,
        measurer,
        math_renderer,
        cluster_label_base_style,
        cluster_title_wrapping_width: wrapping_width,
        cluster_wrap_mode,
        node_padding,
    };
    let node_measure_ctx = NodeMeasureContext {
        model: render_model,
        effective_config,
        measurer,
        math_renderer,
        node_label_base_style,
        wrapping_width,
        node_direction: diagram_direction,
        node_padding,
        state_padding,
        node_wrap_mode,
        class_html_labels: edge_html_labels,
        svg_label_sidecar,
    };
    let mut inserted_ids: HashSet<&str> = HashSet::new();
    // FlowDB emits subgraphs in reverse storage order before leaf vertices, and Mermaid derives
    // sibling lists by filtering that canonical array without sorting. Preserve that order here.
    for subgraph_index in (0..model.subgraphs.len()).rev() {
        let sg = &model.subgraphs[subgraph_index];
        charge_adapter_work(&mut work_control, 1)?;
        if render_label_sources
            .collapsed_replacement(sg.id.as_str())
            .is_some()
        {
            continue;
        }
        if !inserted_ids.insert(sg.id.as_str()) {
            continue;
        }
        graph.nodes.push(subgraph_to_elk_node(
            subgraph_index,
            sg,
            parent_by_id.get(&sg.id).cloned(),
            &include_children_groups,
            &cluster_measure_ctx,
        ));
    }

    for (node_index, node) in model.nodes.iter().enumerate() {
        charge_adapter_work(&mut work_control, 1)?;
        if subgraph_ids.contains(node.id.as_str()) || !inserted_ids.insert(node.id.as_str()) {
            continue;
        }
        graph.nodes.push(flow_node_to_elk_node(
            node,
            parent_by_id.get(&node.id).cloned(),
            FlowchartSvgLabelOwner::Node(node_index),
            NodeMeasureContext {
                node_direction: node_directions
                    .get(node.id.as_str())
                    .copied()
                    .unwrap_or(diagram_direction),
                ..node_measure_ctx
            },
        ));
    }

    if config_bool(effective_config_value, &["elk", "keepEntryNodeOnTop"]).unwrap_or(false) {
        crate::elk_adapter::apply_cyclic_entry_constraints(
            &mut graph.nodes,
            model
                .edges
                .iter()
                .map(|edge| (edge.from.as_str(), edge.to.as_str())),
            &mut work_control,
        )?;
    }

    charge_adapter_work(&mut work_control, model.edges.len())?;
    let mut edges = Vec::with_capacity(model.edges.len());
    for (edge_index, edge) in model.edges.iter().enumerate() {
        let label = edge_label(
            edge,
            FlowchartSvgLabelOwner::Edge(edge_index),
            EdgeMeasureContext {
                model: render_model,
                effective_config,
                measurer,
                math_renderer,
                edge_label_base_style,
                edge_label_wrapping_width,
                edge_wrap_mode,
                edge_html_labels,
                svg_label_sidecar,
            },
        );
        edges.push(elk::Edge {
            id: edge.id.clone(),
            source: edge.from.clone(),
            target: edge.to.clone(),
            label,
            minlen: edge.length.max(1),
            inside_self_loops_yo: false,
            terminal_labels: Vec::new(),
        });
    }
    graph.edges = edges;

    Ok(graph)
}

#[derive(Clone, Copy)]
struct ElkMeasureContext<'a> {
    model: FlowchartRenderModelRef<'a>,
    effective_config: &'a MermaidConfig,
    measurer: &'a dyn TextMeasurer,
    math_renderer: Option<&'a (dyn MathRenderer + Send + Sync)>,
    cluster_label_base_style: &'a TextStyle,
    cluster_title_wrapping_width: f64,
    cluster_wrap_mode: WrapMode,
    node_padding: f64,
}

#[derive(Clone, Copy)]
struct NodeMeasureContext<'a> {
    model: FlowchartRenderModelRef<'a>,
    effective_config: &'a MermaidConfig,
    measurer: &'a dyn TextMeasurer,
    math_renderer: Option<&'a (dyn MathRenderer + Send + Sync)>,
    node_label_base_style: &'a TextStyle,
    wrapping_width: f64,
    node_direction: elk::Direction,
    node_padding: f64,
    state_padding: f64,
    node_wrap_mode: WrapMode,
    class_html_labels: bool,
    svg_label_sidecar: Option<&'a FlowchartSvgLabelSidecarBuilder>,
}

#[derive(Clone, Copy)]
struct EdgeMeasureContext<'a> {
    model: FlowchartRenderModelRef<'a>,
    effective_config: &'a MermaidConfig,
    measurer: &'a dyn TextMeasurer,
    math_renderer: Option<&'a (dyn MathRenderer + Send + Sync)>,
    edge_label_base_style: &'a TextStyle,
    edge_label_wrapping_width: f64,
    edge_wrap_mode: WrapMode,
    edge_html_labels: bool,
    svg_label_sidecar: Option<&'a FlowchartSvgLabelSidecarBuilder>,
}

fn dir_to_elk_direction(dir: &str) -> elk::Direction {
    match dir.trim().to_uppercase().as_str() {
        "LR" => elk::Direction::Right,
        "RL" => elk::Direction::Left,
        "BT" => elk::Direction::Up,
        "TB" | "TD" => elk::Direction::Down,
        _ => elk::Direction::Down,
    }
}

fn parent_by_id(
    model: &FlowchartModel,
    work_control: &mut Option<&mut ElkOperationWorkControl>,
) -> Result<HashMap<String, String>> {
    let mut parent_by_id = HashMap::new();
    for sg in model.subgraphs.iter().rev() {
        charge_adapter_work(work_control, sg.nodes.len())?;
        for child in &sg.nodes {
            parent_by_id.insert(child.clone(), sg.id.clone());
        }
    }

    if let Some((child, parent)) =
        first_elk_parent_cycle_assignment(model, &parent_by_id, work_control)?
    {
        return Err(Error::InvalidModel {
            message: format!("Setting {parent} as parent of {child} would create a cycle"),
        });
    }

    Ok(parent_by_id)
}

fn first_elk_parent_cycle_assignment(
    model: &FlowchartModel,
    parent_by_id: &HashMap<String, String>,
    work_control: &mut Option<&mut ElkOperationWorkControl>,
) -> Result<Option<(String, String)>> {
    // ELK does not materialize a Graphlib compound graph, so it must validate the final FlowDB
    // parent map itself. Preserve Mermaid's reverse-subgraph assignment order, but keep this check
    // local to the ELK adapter instead of restoring Dagre's obsolete duplicate preflight.
    charge_adapter_work(work_control, model.subgraphs.len())?;
    let mut assigned = HashSet::with_capacity(model.subgraphs.len());
    let mut assignments = Vec::with_capacity(model.subgraphs.len());
    for child in model
        .subgraphs
        .iter()
        .rev()
        .map(|subgraph| subgraph.id.as_str())
    {
        let Some(parent) = parent_by_id.get(child) else {
            continue;
        };
        if assigned.insert(child) {
            assignments.push((child, parent.as_str()));
        }
    }
    if assignments.is_empty() {
        return Ok(None);
    }

    let node_capacity = checked_adapter_mul(work_control, assignments.len(), 2)?;
    // Each retained assignment child has at most one final parent. In that functional graph, an
    // assignment closes a Graphlib parent cycle exactly when it is the latest Mermaid-order edge
    // on one directed cycle. Peel every acyclic node with Kahn's algorithm, then select the
    // earliest such closing edge across the remaining cycles. This preserves Graphlib's
    // observable error order without adding a logarithmic union-find term to otherwise linear ELK
    // hierarchy preparation.
    let node_work = checked_adapter_mul(work_control, node_capacity, 12)?;
    let assignment_work = checked_adapter_mul(work_control, assignments.len(), 4)?;
    charge_adapter_work(
        work_control,
        checked_adapter_add(work_control, node_work, assignment_work)?,
    )?;

    fn intern<'a>(
        id: &'a str,
        index_by_id: &mut HashMap<&'a str, usize>,
        next_by_index: &mut Vec<Option<usize>>,
        assignment_by_index: &mut Vec<Option<usize>>,
        indegree: &mut Vec<usize>,
    ) -> usize {
        if let Some(&index) = index_by_id.get(id) {
            return index;
        }
        let index = next_by_index.len();
        next_by_index.push(None);
        assignment_by_index.push(None);
        indegree.push(0);
        index_by_id.insert(id, index);
        index
    }

    let mut index_by_id = HashMap::with_capacity(node_capacity);
    let mut next_by_index = Vec::with_capacity(node_capacity);
    let mut assignment_by_index = Vec::with_capacity(node_capacity);
    let mut indegree = Vec::with_capacity(node_capacity);
    for (assignment_index, &(child, parent)) in assignments.iter().enumerate() {
        let child_index = intern(
            child,
            &mut index_by_id,
            &mut next_by_index,
            &mut assignment_by_index,
            &mut indegree,
        );
        let parent_index = intern(
            parent,
            &mut index_by_id,
            &mut next_by_index,
            &mut assignment_by_index,
            &mut indegree,
        );
        next_by_index[child_index] = Some(parent_index);
        assignment_by_index[child_index] = Some(assignment_index);
        indegree[parent_index] += 1;
    }

    let mut queue = indegree
        .iter()
        .enumerate()
        .filter_map(|(index, &degree)| (degree == 0).then_some(index))
        .collect::<VecDeque<_>>();
    while let Some(index) = queue.pop_front() {
        let Some(parent_index) = next_by_index[index] else {
            continue;
        };
        indegree[parent_index] -= 1;
        if indegree[parent_index] == 0 {
            queue.push_back(parent_index);
        }
    }

    let mut visited = vec![false; next_by_index.len()];
    let mut first_closing_assignment = None;
    for start in 0..next_by_index.len() {
        if indegree[start] == 0 || visited[start] {
            continue;
        }
        let mut current = start;
        let mut closing_assignment = 0;
        loop {
            if visited[current] {
                break;
            }
            visited[current] = true;
            let assignment_index = assignment_by_index[current]
                .expect("a node retained by functional-graph peeling must own a parent assignment");
            closing_assignment = closing_assignment.max(assignment_index);
            current = next_by_index[current]
                .expect("a node retained by functional-graph peeling must have a parent");
        }
        if first_closing_assignment.is_none_or(|first| closing_assignment < first) {
            first_closing_assignment = Some(closing_assignment);
        }
    }

    Ok(first_closing_assignment.map(|assignment_index| {
        let (child, parent) = assignments[assignment_index];
        (child.to_string(), parent.to_string())
    }))
}

fn include_children_groups<'a>(
    model: &'a FlowchartModel,
    parent_by_id: &'a HashMap<String, String>,
    work_control: &mut Option<&mut ElkOperationWorkControl>,
) -> Result<HashSet<&'a str>> {
    if model.subgraphs.is_empty() || model.edges.is_empty() {
        return Ok(HashSet::new());
    }
    let item_count = checked_adapter_add(work_control, model.nodes.len(), model.subgraphs.len())?;
    crate::elk_hierarchy::include_children(
        model
            .nodes
            .iter()
            .map(|node| node.id.as_str())
            .chain(model.subgraphs.iter().map(|subgraph| subgraph.id.as_str()))
            .map(|id| (id, parent_by_id.get(id).map(String::as_str))),
        item_count,
        model
            .edges
            .iter()
            .map(|edge| (edge.from.as_str(), edge.to.as_str())),
        work_control,
    )
}

fn subgraph_label(
    declaration_ordinal: usize,
    sg: &FlowSubgraph,
    ctx: &ElkMeasureContext<'_>,
) -> Option<elk::Label> {
    let label_type = sg.label_type.as_deref().unwrap_or("text");
    let title = ctx.model.subgraph_title_for_render(declaration_ordinal, sg);
    let (classes, styles) = ctx.model.effective_subgraph_css(declaration_ordinal, sg);
    let text_style = flowchart_effective_text_style_for_classes(
        ctx.cluster_label_base_style,
        &ctx.model.class_defs,
        classes,
        styles,
    );
    // createGraph's unwrapGroupLabels measures ordinary groups at the same unbounded width
    // used by the cluster painter. Markdown and collapsed leaf labels retain their wrapping.
    let max_width_px = (label_type == "markdown"
        || ctx.model.is_subgraph_collapsed(sg.id.as_str()))
    .then_some(ctx.cluster_title_wrapping_width);
    let metrics = measure_flowchart_svg_label_for_layout(
        None,
        None,
        None,
        FlowchartLabelMetricsRequest {
            measurer: ctx.measurer,
            raw_label: title,
            label_type,
            style: text_style.as_ref(),
            max_width_px,
            wrap_mode: ctx.cluster_wrap_mode,
            config: ctx.effective_config,
            math_renderer: ctx.math_renderer,
        },
        FlowchartSvgWidthMode::Bbox,
    );
    Some(elk::Label {
        width: metrics.width.max(1.0),
        height: (metrics.height - 2.0).max(1.0),
    })
}

fn node_dimensions_and_label(
    node: &FlowNode,
    owner: FlowchartSvgLabelOwner,
    ctx: NodeMeasureContext<'_>,
) -> (f64, f64, elk::Label) {
    let raw_label = ctx.model.node_label_for_render(node).unwrap_or(&node.id);
    let label_type = node.label_type.as_deref().unwrap_or("text");
    let node_text_style = flowchart_effective_text_style_for_node_classes(
        ctx.node_label_base_style,
        &ctx.model.class_defs,
        &node.classes,
        &node.styles,
    );
    let svg_width_mode = flowchart_node_svg_width_mode(
        raw_label,
        label_type,
        ctx.node_wrap_mode,
        node.layout_shape.as_deref().unwrap_or("squareRect"),
    );
    let mut metrics = measure_flowchart_svg_label_for_layout(
        ctx.svg_label_sidecar,
        Some(owner),
        Some(node.id.as_str()),
        FlowchartLabelMetricsRequest {
            measurer: ctx.measurer,
            raw_label,
            label_type,
            style: node_text_style.as_ref(),
            max_width_px: Some(ctx.wrapping_width),
            wrap_mode: ctx.node_wrap_mode,
            config: ctx.effective_config,
            math_renderer: ctx.math_renderer,
        },
        svg_width_mode,
    );
    if ctx.node_wrap_mode == WrapMode::HtmlLike && ctx.class_html_labels {
        flowchart_apply_html_node_class_box_metrics(
            &mut metrics,
            raw_label,
            label_type,
            node_text_style.as_ref(),
            &ctx.model.class_defs,
            &node.classes,
        );
    }

    metrics = metrics.with_label_min_width(
        raw_label,
        super::flowchart_node_label_min_width(
            raw_label,
            node.layout_shape.as_deref(),
            ctx.effective_config,
        ),
        None,
    );
    let label = elk::Label {
        width: metrics.width,
        height: metrics.height,
    };
    let (width, height) = node_layout_dimensions(NodeLayoutDimensionsRequest {
        layout_shape: node.layout_shape.as_deref(),
        layout_direction: match ctx.node_direction {
            elk::Direction::Left => "RL",
            elk::Direction::Right => "LR",
            elk::Direction::Up => "BT",
            elk::Direction::Down => "TB",
        },
        metrics,
        has_label: !raw_label.is_empty(),
        wrapping_width: ctx.wrapping_width,
        node_constraint: node.constraint.as_deref(),
        padding: ctx.node_padding,
        look_is_neo: crate::config::mermaid_config_diagram_look(ctx.effective_config).is_neo(),
        state_padding: ctx.state_padding,
        node_icon: node.icon.as_deref(),
        node_img: node.img.as_deref(),
        node_pos: node.pos.as_deref(),
        node_asset_width: node.asset_width,
        node_asset_height: node.asset_height,
    });

    (width, height, label)
}

fn edge_label(
    edge: &FlowEdge,
    owner: FlowchartSvgLabelOwner,
    ctx: EdgeMeasureContext<'_>,
) -> Option<elk::Label> {
    let label_text = ctx.model.edge_label_for_render(edge).unwrap_or_default();
    let label_type = edge.label_type.as_deref().unwrap_or("text");
    if crate::flowchart::flowchart_label_is_empty_for_render(label_text) {
        return None;
    }

    let default_edge_styles = ctx
        .model
        .edge_defaults
        .as_ref()
        .map_or(&[][..], |defaults| defaults.style.as_slice());
    let edge_text_style = flowchart_effective_edge_label_text_style(
        ctx.edge_label_base_style,
        &ctx.model.class_defs,
        &edge.classes,
        default_edge_styles,
        &edge.style,
    );
    let metrics = if label_type == "markdown" && ctx.edge_wrap_mode != WrapMode::HtmlLike {
        crate::text::measure_wrapped_markdown_with_inline_styles(
            ctx.measurer,
            label_text,
            edge_text_style.as_ref(),
            Some(ctx.edge_label_wrapping_width),
            ctx.edge_wrap_mode,
        )
    } else if ctx.edge_wrap_mode == WrapMode::SvgLike {
        measure_flowchart_svg_label_for_layout_with_metrics_style(
            ctx.svg_label_sidecar,
            Some(owner),
            Some(edge.id.as_str()),
            FlowchartLabelMetricsRequest {
                measurer: ctx.measurer,
                raw_label: label_text,
                label_type,
                // Mermaid wraps the temporary SVG text before applying `labelStyle`.
                style: ctx.edge_label_base_style,
                max_width_px: Some(ctx.edge_label_wrapping_width),
                wrap_mode: ctx.edge_wrap_mode,
                config: ctx.effective_config,
                math_renderer: ctx.math_renderer,
            },
            edge_text_style.as_ref(),
            FlowchartSvgWidthMode::Bbox,
        )
    } else {
        measure_flowchart_svg_label_for_layout(
            ctx.svg_label_sidecar,
            Some(owner),
            Some(edge.id.as_str()),
            FlowchartLabelMetricsRequest {
                measurer: ctx.measurer,
                raw_label: label_text,
                label_type,
                style: edge_text_style.as_ref(),
                max_width_px: Some(ctx.edge_label_wrapping_width),
                wrap_mode: ctx.edge_wrap_mode,
                config: ctx.effective_config,
                math_renderer: ctx.math_renderer,
            },
            FlowchartSvgWidthMode::Bbox,
        )
    };

    let (width, height) = if ctx.edge_html_labels {
        (metrics.width.max(1.0), metrics.height.max(1.0))
    } else {
        (
            (metrics.width + 4.0).max(1.0),
            (metrics.height + 4.0).max(1.0),
        )
    };

    Some(elk::Label { width, height })
}

fn flow_node_to_elk_node(
    node: &FlowNode,
    parent: Option<String>,
    owner: FlowchartSvgLabelOwner,
    ctx: NodeMeasureContext<'_>,
) -> elk::Node {
    let (width, height, label) = node_dimensions_and_label(node, owner, ctx);
    elk::Node {
        id: node.id.clone(),
        kind: elk::NodeKind::Leaf,
        container: Default::default(),
        label_text: None,
        width,
        height,
        parent,
        direction: None,
        hierarchy_handling: None,
        layer_constraint: None,
        port_alignment: None,
        label: Some(label),
    }
}

fn subgraph_to_elk_node(
    declaration_ordinal: usize,
    sg: &FlowSubgraph,
    parent: Option<String>,
    include_children_groups: &HashSet<&str>,
    ctx: &ElkMeasureContext<'_>,
) -> elk::Node {
    if ctx.model.is_subgraph_collapsed(sg.id.as_str()) {
        let label = subgraph_label(declaration_ordinal, sg, ctx);
        let (width, height) = label
            .map(|label| ((label.width + 16.0).max(80.0), label.height + 46.0))
            .unwrap_or((80.0, 44.0));
        return elk::Node {
            id: sg.id.clone(),
            kind: elk::NodeKind::Leaf,
            container: Default::default(),
            label_text: None,
            width,
            height,
            parent,
            direction: None,
            hierarchy_handling: None,
            layer_constraint: None,
            port_alignment: None,
            label,
        };
    }

    elk::Node {
        id: sg.id.clone(),
        kind: elk::NodeKind::Group,
        container: elk::ContainerNodeOptions {
            algorithm: sg
                .metadata
                .as_ref()
                .and_then(|metadata| metadata.get("algorithm"))
                .and_then(serde_json::Value::as_str)
                .and_then(elk::Algorithm::from_container_name),
            padding: ctx.node_padding,
        },
        label_text: Some(
            ctx.model
                .subgraph_title_for_render(declaration_ordinal, sg)
                .to_owned(),
        ),
        width: 0.0,
        height: 0.0,
        parent,
        // Use the resolved dir, not has_explicit_dir: FlowDB may populate it through inheritDir,
        // and Mermaid applies SeparateChildren whenever that resolved direction exists.
        direction: sg.dir.as_deref().map(dir_to_elk_direction),
        hierarchy_handling: if include_children_groups.contains(sg.id.as_str()) {
            Some(elk::HierarchyHandling::IncludeChildren)
        } else if sg.dir.is_some() {
            Some(elk::HierarchyHandling::SeparateChildren)
        } else {
            None
        },
        layer_constraint: None,
        port_alignment: None,
        label: subgraph_label(declaration_ordinal, sg, ctx),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use indexmap::IndexMap;
    use merman_core::{Engine, ParseOptions};
    use serde_json::json;

    const NON_LATTICE_COMPUTED_LENGTH_PX: f64 = 73.123_456_789;

    #[test]
    fn elk_subgraph_algorithm_metadata_survives_parse_and_measurement() {
        for (requested, expected) in [
            ("elk.box", Some(elk::Algorithm::Box)),
            ("elk.layered", Some(elk::Algorithm::Layered)),
            ("elk.radial", Some(elk::Algorithm::Radial)),
            ("ELK.BOX", None),
            ("box", None),
            ("elk.unknown", None),
        ] {
            let source = format!(
                "flowchart TB\nsubgraph G[Group]\nA --> B\nend\nG@{{algorithm: {requested}}}\n"
            );
            let parsed = Engine::new()
                .parse_diagram_for_render_model_sync(&source, ParseOptions::default())
                .expect("parse")
                .expect("diagram");
            let graph =
                super::build_flowchart_elk_graph(&parsed, &NonLatticeComputedLengthMeasurer, None)
                    .expect("measured graph");
            let group = graph.nodes.iter().find(|node| node.id == "G").unwrap();
            assert_eq!(group.container.algorithm, expected, "{requested}");
            assert_eq!(group.container.padding, 15.0);
        }
    }

    struct NonLatticeComputedLengthMeasurer;

    fn build_flowchart_elk_graph(
        model: &FlowchartModel,
        effective_config: &MermaidConfig,
        measurer: &dyn TextMeasurer,
        math_renderer: Option<&(dyn MathRenderer + Send + Sync)>,
    ) -> Result<elk::Graph> {
        build_flowchart_elk_graph_from_semantic(model, effective_config, measurer, math_renderer)
    }

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
    fn elk_hidden_shape_labels_remain_paint_metadata_without_layout_margins() {
        let shapes = [
            "anchor",
            "small-circle",
            "framed-circle",
            "fork",
            "join",
            "lightning-bolt",
            "filled-circle",
            "crossed-circle",
            "hourglass",
        ];
        let nodes = shapes
            .iter()
            .enumerate()
            .map(|(index, shape)| {
                let mut item = node(
                    &format!("n{index}"),
                    Some("A long authored label ignored by this shape"),
                    None,
                );
                item.layout_shape = Some((*shape).into());
                item
            })
            .collect();
        let edges = (0..shapes.len() - 1)
            .map(|index| {
                edge(
                    &format!("e{index}"),
                    &format!("n{index}"),
                    &format!("n{}", index + 1),
                    None,
                )
            })
            .collect();
        let mut model = model(nodes, edges);
        model.direction = Some("LR".into());
        let config = MermaidConfig::default();
        let measurer = crate::text::DeterministicTextMeasurer::default();
        let graph = build_flowchart_elk_graph(&model, &config, &measurer, None).unwrap();
        assert!(
            graph
                .nodes
                .iter()
                .all(|node| node.label.is_some_and(|label| label.width > node.width))
        );
        let actual = elk::layout(&graph).unwrap();
        let mut without_paint_metrics = graph.clone();
        for node in &mut without_paint_metrics.nodes {
            node.label = None;
        }
        assert_eq!(actual, elk::layout(&without_paint_metrics).unwrap());
        let rendered = flowchart_layout_from_elk(&model, &config, &graph, actual).unwrap();
        for node in rendered.nodes {
            let source = graph
                .nodes
                .iter()
                .find(|source| source.id == node.id)
                .unwrap();
            let label = source.label.unwrap();
            assert_eq!(node.label_width, Some(label.width));
            assert_eq!(node.label_height, Some(label.height));
        }
    }

    #[test]
    fn elk_preserves_operation_computed_length_precision() {
        let parsed = Engine::new()
            .parse_diagram_for_render_model_sync(
                "%%{init: {\"htmlLabels\": false, \"flowchart\": {\"htmlLabels\": false, \"minNodeWidth\": 0}}}%%\nflowchart TB\nA[alpha]\n",
                ParseOptions::default(),
            )
            .expect("parse ok")
            .expect("diagram detected");
        let graph =
            super::build_flowchart_elk_graph(&parsed, &NonLatticeComputedLengthMeasurer, None)
                .expect("ELK graph");
        let label = graph
            .nodes
            .iter()
            .find(|node| node.id == "A")
            .and_then(|node| node.label)
            .expect("node A label");

        assert_eq!(label.width, NON_LATTICE_COMPUTED_LENGTH_PX);
    }

    #[test]
    fn elk_adapter_binds_prepared_labels_to_semantic_node_and_edge_owners() {
        let mut model = model(
            vec![
                node("A", Some("alpha node owner"), None),
                node("B", Some("beta node owner"), None),
            ],
            vec![
                edge("e1", "A", "B", Some("first edge owner")),
                edge("e2", "A", "B", Some("second edge owner")),
            ],
        );
        model.subgraphs.push(subgraph(
            "Group".to_string(),
            vec!["A".to_string(), "B".to_string()],
        ));
        let config = MermaidConfig::from_value(json!({
            "htmlLabels": false,
            "flowchart": {"htmlLabels": false, "wrappingWidth": 96}
        }));
        let environment = crate::environment::RenderEnvironment::deterministic();
        let session = environment.begin_session().expect("deterministic session");
        let measurer = session.text_measurer(crate::environment::TextMeasurementPhase::Layout);
        let builder = FlowchartSvgLabelSidecarBuilder::default();

        let graph = build_flowchart_elk_graph_with_render_labels_and_work_control(
            &model,
            &FlowchartRenderContext::default(),
            &config,
            &measurer,
            None,
            Some(&builder),
            None,
        )
        .expect("build ELK graph");
        assert_eq!(graph.edges.len(), 2);

        let sidecar = builder.finish();
        assert_eq!(
            sidecar.node_owner("A", false),
            Some(FlowchartSvgLabelOwner::Node(0))
        );
        assert_eq!(
            sidecar.node_owner("B", false),
            Some(FlowchartSvgLabelOwner::Node(1))
        );
        assert_eq!(
            sidecar.edge_owner("e1", false),
            Some(FlowchartSvgLabelOwner::Edge(0))
        );
        assert_eq!(
            sidecar.edge_owner("e2", false),
            Some(FlowchartSvgLabelOwner::Edge(1))
        );
        assert_eq!(sidecar.subgraph_title_owner("Group"), None);
    }

    #[test]
    fn elk_html_nbsp_only_edge_labels_respect_source_provenance() {
        let nbsp = '\u{00A0}';
        let source = format!(
            "flowchart LR\nA -- \"&nbsp;\" --> B\nC -- \"{nbsp}\" --> D\nE[\"&lt;Less&lt;\"]\n"
        );
        let parsed = Engine::new()
            .parse_diagram_for_render_model_sync(&source, ParseOptions::default())
            .expect("parse ok")
            .expect("diagram detected");
        let graph = super::build_flowchart_elk_graph(
            &parsed,
            &crate::text::DeterministicTextMeasurer::default(),
            None,
        )
        .expect("ELK graph ok");

        assert_eq!(graph.edges.len(), 2);
        let entity_label = graph
            .edges
            .iter()
            .find(|edge| edge.source == "A")
            .and_then(|edge| edge.label.as_ref())
            .expect("entity-authored NBSP label must reach ELK");
        assert!(entity_label.width > 0.0, "{entity_label:?}");
        assert!(entity_label.height > 0.0, "{entity_label:?}");

        let direct_label = graph
            .edges
            .iter()
            .find(|edge| edge.source == "C")
            .and_then(|edge| edge.label.as_ref());
        assert!(direct_label.is_none(), "{direct_label:?}");

        let escaped_angle_label = graph
            .nodes
            .iter()
            .find(|node| node.id == "E")
            .and_then(|node| node.label)
            .expect("entity-authored angle brackets must retain a visible ELK label");
        assert!(escaped_angle_label.width > 0.0, "{escaped_angle_label:?}");
        assert!(escaped_angle_label.height > 0.0, "{escaped_angle_label:?}");
    }

    fn node(id: &str, label: Option<&str>, label_type: Option<&str>) -> FlowNode {
        FlowNode {
            id: id.to_string(),
            provenance: Default::default(),
            label: label.map(str::to_string),
            label_type: label_type.map(str::to_string),
            layout_shape: Some("squareRect".to_string()),
            shape: None,
            icon: None,
            form: None,
            pos: None,
            img: None,
            constraint: None,
            asset_width: None,
            asset_height: None,
            classes: Vec::new(),
            styles: Vec::new(),
            link: None,
            link_target: None,
            have_callback: false,
        }
    }

    fn edge(id: &str, from: &str, to: &str, label: Option<&str>) -> FlowEdge {
        FlowEdge {
            id: id.to_string(),
            from: from.to_string(),
            to: to.to_string(),
            label: label.map(str::to_string),
            label_type: Some("text".to_string()),
            edge_type: Some("arrow_point".to_string()),
            arrow: "-->".to_string(),
            start_marker: FlowEdgeMarker::None,
            end_marker: FlowEdgeMarker::Point,
            is_user_defined_id: false,
            stroke: Some("normal".to_string()),
            stroke_kind: FlowEdgeStroke::Normal,
            visibility: FlowEdgeVisibility::Visible,
            interpolate: None,
            classes: Vec::new(),
            style: Vec::new(),
            animate: None,
            animation: None,
            length: 1,
        }
    }

    fn model(nodes: Vec<FlowNode>, edges: Vec<FlowEdge>) -> FlowchartModel {
        FlowchartModel {
            keyword: "graph".to_string(),
            acc_descr: None,
            acc_title: None,
            class_defs: merman_core::native_collections::OrderedMap::new(),
            direction: Some("TD".to_string()),
            edge_defaults: None,
            vertex_calls: Vec::new(),
            nodes,
            edges,
            subgraphs: Vec::new(),
            tooltips: Default::default(),
            warning_facts: Vec::new(),
        }
    }

    fn subgraph(id: String, nodes: Vec<String>) -> FlowSubgraph {
        FlowSubgraph {
            metadata: None,
            title: id.clone(),
            id,
            dir: None,
            has_explicit_dir: false,
            label_type: Some("text".to_string()),
            classes: Vec::new(),
            styles: Vec::new(),
            nodes,
        }
    }

    fn nested_model(depth: usize, target: Option<&str>) -> FlowchartModel {
        let mut nested = model(
            vec![
                node("leaf", Some("leaf"), None),
                node("sibling", Some("sibling"), None),
                node("outside", Some("outside"), None),
            ],
            target
                .map(|target| vec![edge("cross", "leaf", target, None)])
                .unwrap_or_default(),
        );
        if depth == 0 {
            return nested;
        }
        nested.subgraphs.push(subgraph(
            "group-0".to_string(),
            vec!["leaf".to_string(), "sibling".to_string()],
        ));
        for level in 1..depth {
            nested.subgraphs.push(subgraph(
                format!("group-{level}"),
                vec![format!("group-{}", level - 1)],
            ));
        }
        nested
    }

    fn nested_model_with_repeated_cross_edges(depth: usize, edge_count: usize) -> FlowchartModel {
        let mut nested = nested_model(depth, None);
        nested.edges = (0..edge_count)
            .map(|index| edge(&format!("cross-{index}"), "leaf", "outside", None))
            .collect();
        nested
    }

    fn projection_fixture() -> (FlowchartModel, elk::Graph, elk::LayoutResult) {
        let model = model(
            vec![
                node("A", Some("Alpha"), None),
                node("B", Some("Beta"), None),
            ],
            vec![edge("L-A-B", "A", "B", None)],
        );
        let graph = build_flowchart_elk_graph(
            &model,
            &MermaidConfig::default(),
            &crate::text::DeterministicTextMeasurer::default(),
            None,
        )
        .unwrap();
        let layout = elk::LayoutResult {
            nodes: vec![
                elk::NodeLayout {
                    id: "A".to_string(),
                    x: 0.0,
                    y: 0.0,
                    width: 10.0,
                    height: 10.0,
                },
                elk::NodeLayout {
                    id: "B".to_string(),
                    x: 20.0,
                    y: 0.0,
                    width: 10.0,
                    height: 10.0,
                },
            ],
            edges: vec![elk::EdgeLayout {
                id: "L-A-B".to_string(),
                points: vec![
                    elk::Point { x: 5.0, y: 0.0 },
                    elk::Point { x: 15.0, y: 0.0 },
                ],
                labels: Vec::new(),
            }],
        };
        (model, graph, layout)
    }

    #[test]
    fn flowchart_elk_contracts_group_frames_without_moving_owned_routes() {
        let mut model = model(
            vec![node("A", Some("Alpha"), None)],
            vec![edge("loop", "A", "A", Some("route"))],
        );
        model
            .subgraphs
            .push(subgraph("G".to_string(), vec!["A".to_string()]));
        let graph = build_flowchart_elk_graph(
            &model,
            &MermaidConfig::default(),
            &crate::text::DeterministicTextMeasurer::default(),
            None,
        )
        .unwrap();
        let points = vec![
            elk::Point { x: 184.0, y: 86.0 },
            elk::Point { x: 184.0, y: 124.0 },
            elk::Point { x: 84.0, y: 124.0 },
            elk::Point { x: 84.0, y: 86.0 },
        ];
        let provider = elk::LayoutResult {
            nodes: vec![
                elk::NodeLayout {
                    id: "G".into(),
                    x: 150.0,
                    y: 74.0,
                    width: 300.0,
                    height: 148.0,
                },
                elk::NodeLayout {
                    id: "A".into(),
                    x: 134.0,
                    y: 86.0,
                    width: 100.0,
                    height: 76.0,
                },
            ],
            edges: vec![elk::EdgeLayout {
                id: "loop".into(),
                points: points.clone(),
                labels: vec![elk::EdgeLabelLayout {
                    terminal: None,
                    x: 110.0,
                    y: 112.0,
                    width: 30.0,
                    height: 12.0,
                }],
            }],
        };
        // internalEdgePoints uses route sections, not edge-label boxes. A distant provider
        // label must preserve its absolute position without preventing frame contraction.
        for label_x in [110.0, 260.0] {
            let mut provider = provider.clone();
            provider.edges[0].labels[0].x = label_x;
            let projected =
                flowchart_layout_from_elk(&model, &MermaidConfig::default(), &graph, provider)
                    .unwrap();
            let group = projected.nodes.iter().find(|node| node.id == "G").unwrap();
            let cluster = projected
                .clusters
                .iter()
                .find(|cluster| cluster.id == "G")
                .unwrap();
            assert_eq!(
                (group.x, group.y, group.width, group.height),
                (134.0, 74.0, 148.0, 148.0)
            );
            assert_eq!(
                (cluster.x, cluster.y, cluster.width, cluster.height),
                (134.0, 74.0, 148.0, 148.0)
            );
            assert_eq!(cluster.title_label.x, 134.0);
            for (actual, original) in projected.edges[0].points.iter().zip(&points) {
                assert_eq!((actual.x, actual.y), (original.x, original.y));
            }
            let label = projected.edges[0].label.as_ref().unwrap();
            assert_eq!(
                (label.x, label.y, label.width, label.height),
                (label_x + 15.0, 118.0, 30.0, 12.0)
            );
        }
    }

    #[test]
    fn flowchart_elk_small_nodes_align_to_routes_before_projection() {
        for vertical in [false, true] {
            let (mut model, graph, mut layout) = projection_fixture();
            model.nodes[1].layout_shape = Some("anchor".to_string());
            let target = &mut layout.nodes[1];
            target.width = 2.0;
            target.height = 2.0;
            target.x = 205.0;
            target.y = 25.5;
            let anchor = if vertical {
                elk::Point { x: 214.0, y: 24.5 }
            } else {
                elk::Point { x: 204.0, y: 34.5 }
            };
            layout.edges[0].points[1] = anchor;
            layout.edges[0].labels.push(elk::EdgeLabelLayout {
                terminal: None,
                x: 80.0,
                y: 15.0,
                width: 20.0,
                height: 10.0,
            });
            let mut graph = graph;
            graph.edges[0].label = Some(elk::Label {
                width: 20.0,
                height: 10.0,
            });
            let original_route = layout.edges[0].points.clone();
            let projected =
                flowchart_layout_from_elk(&model, &MermaidConfig::default(), &graph, layout)
                    .unwrap();
            let target = &projected.nodes[1];
            let expected = if vertical {
                (214.0, 25.5)
            } else {
                (205.0, 34.5)
            };
            assert_eq!((target.x, target.y), expected);
            assert_eq!((target.width, target.height), (2.0, 2.0));
            for (actual, original) in projected.edges[0].points.iter().zip(original_route) {
                assert_eq!((actual.x, actual.y), (original.x, original.y));
            }
            let label = projected.edges[0].label.as_ref().unwrap();
            assert_eq!((label.x, label.y), (90.0, 20.0));
        }
    }

    #[test]
    fn flowchart_elk_small_node_alignment_preserves_first_anchor_even_for_small_delta() {
        for first_delta in [0.005, 3.0] {
            let (model, mut graph, mut layout) = projection_fixture();
            layout.edges[0].points[1].y = first_delta;
            let mut second_source = graph.edges[0].clone();
            second_source.id = "second".to_string();
            graph.edges.push(second_source);
            let mut second = layout.edges[0].clone();
            second.id = "second".to_string();
            second.points[1].y = 8.0;
            layout.edges.push(second);
            for reversed in [false, true] {
                let mut ordered_layout = layout.clone();
                if reversed {
                    ordered_layout.edges.reverse();
                }
                let projected = flowchart_layout_from_elk(
                    &model,
                    &MermaidConfig::default(),
                    &graph,
                    ordered_layout,
                )
                .unwrap();
                let expected = if reversed {
                    8.0
                } else if first_delta < 0.01 {
                    0.0
                } else {
                    first_delta
                };
                assert_eq!(projected.nodes[1].y, expected);
            }
        }
    }

    #[test]
    fn flowchart_elk_small_node_alignment_skips_rect33_and_missing_sections() {
        for missing_section in [false, true] {
            let (mut model, graph, mut layout) = projection_fixture();
            if missing_section {
                layout.edges[0].points.clear();
            } else {
                model.nodes[1].layout_shape = Some("rect33".to_string());
                layout.edges[0].points[1].y = 8.0;
            }
            let projected =
                flowchart_layout_from_elk(&model, &MermaidConfig::default(), &graph, layout)
                    .unwrap();
            assert_eq!((projected.nodes[1].x, projected.nodes[1].y), (20.0, 0.0));
        }
    }

    #[test]
    fn flowchart_elk_small_node_alignment_uses_source_border_and_size_thresholds() {
        for (width, anchor_y, expected_x, expected_y, expected_aligned) in [
            (23.0, -9.5, 7.0, 0.0, true),
            (24.0, -9.5, 0.0, 0.0, false),
            (24.0, -9.499, 0.0, -9.499, true),
            (23.0, 10.5, 7.0, 0.0, true),
        ] {
            let mut node = LayoutNode {
                id: "tiny".to_string(),
                x: 0.0,
                y: 0.0,
                width,
                height: 20.0,
                is_cluster: false,
                label_width: None,
                label_height: None,
            };
            let mut aligned = false;
            align_degenerate_node_to_anchor(
                &mut node,
                &elk::Point {
                    x: 7.0,
                    y: anchor_y,
                },
                &mut aligned,
            );
            assert_eq!(
                (node.x, node.y, aligned),
                (expected_x, expected_y, expected_aligned)
            );
        }
    }

    fn adapter_graph_and_work(
        model: &FlowchartModel,
        config: &MermaidConfig,
    ) -> (elk::Graph, usize) {
        let meter = Arc::new(OperationWorkMeter::new(
            crate::resources::RenderResourcePolicy::unbounded_for_trusted_input(),
        ));
        let mut work_control = ElkOperationWorkControl::new(meter);
        let graph = build_flowchart_elk_graph_with_work_control(
            model,
            config,
            &crate::text::DeterministicTextMeasurer::default(),
            None,
            Some(&mut work_control),
        )
        .unwrap();
        (graph, work_control.adapter_work())
    }

    fn operation_seed() -> elk::ElkOperationSeed {
        elk::ElkOperationSeed::from_operation_seed(
            std::num::NonZeroU64::new(0x656c_6b2d_776f_726b).expect("nonzero operation seed"),
        )
    }

    #[test]
    fn flowchart_elk_feedback_orientation_restores_semantic_routes_and_labels() {
        for direction in ["TB", "LR"] {
            for enabled in [false, true] {
                let mut model = model(
                    ["S", "A", "B", "C", "X"]
                        .into_iter()
                        .map(|id| node(id, Some(id), None))
                        .collect(),
                    vec![
                        edge("entry", "S", "A", None),
                        edge("inner", "A", "B", None),
                        edge("out", "B", "X", None),
                        edge("feedback", "X", "C", Some("return")),
                    ],
                );
                model.direction = Some(direction.to_owned());
                model.subgraphs.push(subgraph(
                    "G".into(),
                    vec!["A".into(), "B".into(), "C".into()],
                ));
                let config = MermaidConfig::from_value(serde_json::json!({
                    "elk": { "orientFeedbackEdges": enabled }
                }));
                let measurer = crate::text::DeterministicTextMeasurer::default();
                let graph = build_flowchart_elk_graph(&model, &config, &measurer, None).unwrap();
                let mut provider_graph = graph.clone();
                // The collapsed root-level G -> X -> G cycle reverses only X -> C.
                if enabled {
                    provider_graph.edges[3].source = "C".into();
                    provider_graph.edges[3].target = "X".into();
                }
                let mut provider = elk::layout(&provider_graph).unwrap();
                if enabled {
                    provider
                        .edges
                        .iter_mut()
                        .find(|edge| edge.id == "feedback")
                        .unwrap()
                        .points
                        .reverse();
                }
                let expected =
                    flowchart_layout_from_elk(&model, &config, &graph, provider).unwrap();
                let actual = layout_flowchart_elk_typed_with_operation_seed(
                    &model,
                    &config,
                    &measurer,
                    None,
                    operation_seed(),
                    Arc::new(OperationWorkMeter::new(
                        crate::RenderResourcePolicy::unbounded_for_trusted_input(),
                    )),
                )
                .unwrap();
                assert_eq!(
                    serde_json::to_value(&actual).unwrap(),
                    serde_json::to_value(&expected).unwrap(),
                    "{direction}, enabled={enabled}"
                );
                let feedback = actual
                    .edges
                    .iter()
                    .find(|edge| edge.id == "feedback")
                    .unwrap();
                assert_eq!((feedback.from.as_str(), feedback.to.as_str()), ("X", "C"));
                assert!(feedback.label.is_some());
                assert_eq!(
                    (model.edges[3].from.as_str(), model.edges[3].to.as_str()),
                    ("X", "C")
                );
            }
        }
    }

    #[test]
    fn flowchart_elk_kernel_interruption_maps_to_layout_work_resource_error() {
        let model = model(
            vec![
                node("A", Some("Alpha"), None),
                node("B", Some("Beta"), None),
            ],
            vec![edge("L-A-B", "A", "B", None)],
        );
        // Keep this threshold at the provider boundary, not the optional feedback pre-pass.
        let config = MermaidConfig::from_value(serde_json::json!({
            "elk": { "orientFeedbackEdges": false }
        }));
        let (_, adapter_work) = adapter_graph_and_work(&model, &config);
        let meter = Arc::new(OperationWorkMeter::new(
            crate::resources::RenderResourcePolicy::unbounded_for_trusted_input()
                .with_limit(crate::ResourceLimitId::MaxLayoutWorkUnits, adapter_work)
                .unwrap(),
        ));

        let error = layout_flowchart_elk_typed_with_operation_seed(
            &model,
            &config,
            &crate::text::DeterministicTextMeasurer::default(),
            None,
            operation_seed(),
            meter,
        )
        .unwrap_err();

        let Error::ResourceLimitExceeded(limit) = error else {
            panic!("expected ResourceLimitExceeded");
        };
        assert_eq!(limit.cause, crate::resources::ResourceLimitCause::Ceiling);
        assert_eq!(limit.limit, "max_layout_work_units");
        assert_eq!(limit.max, adapter_work);
        assert!(limit.actual > limit.max);
    }

    #[test]
    fn flowchart_elk_checked_work_overflow_maps_to_the_resource_contract() {
        let work_control = ElkOperationWorkControl::new(Arc::new(OperationWorkMeter::new(
            crate::resources::RenderResourcePolicy::unbounded_for_trusted_input(),
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
    }

    #[test]
    fn flowchart_elk_adapter_work_is_linear_for_deep_edgeless_hierarchies() {
        let config = MermaidConfig::default();
        let (_, work_16) = adapter_graph_and_work(&nested_model(16, None), &config);
        let (_, work_32) = adapter_graph_and_work(&nested_model(32, None), &config);
        let (_, work_48) = adapter_graph_and_work(&nested_model(48, None), &config);

        assert_eq!(work_32 - work_16, work_48 - work_32);
    }

    #[test]
    fn flowchart_elk_first_cross_scope_edge_pays_linear_index_and_path_work() {
        let config = MermaidConfig::default();
        let (_, local_16) = adapter_graph_and_work(&nested_model(16, Some("sibling")), &config);
        let (_, cross_16) = adapter_graph_and_work(&nested_model(16, Some("outside")), &config);
        let (_, local_32) = adapter_graph_and_work(&nested_model(32, Some("sibling")), &config);
        let (_, cross_32) = adapter_graph_and_work(&nested_model(32, Some("outside")), &config);

        let ancestor_work_16 = cross_16 - local_16;
        let ancestor_work_32 = cross_32 - local_32;
        assert!(ancestor_work_16 > 0);
        assert!(ancestor_work_32 > ancestor_work_16);
        // Each added chain-level scope contributes nine HLD-index units, one DSU slot, and one
        // first-time path mark. The heavy-chain LCA query itself stays constant for this topology.
        assert_eq!(ancestor_work_32 - ancestor_work_16, (32 - 16) * 11);
    }

    #[test]
    fn flowchart_elk_cross_scope_work_has_no_depth_times_repeated_edge_term() {
        let config = MermaidConfig::default();
        let work = |depth, edge_count| {
            adapter_graph_and_work(
                &nested_model_with_repeated_cross_edges(depth, edge_count),
                &config,
            )
            .1
        };

        let work_8_1 = work(8, 1);
        let work_8_16 = work(8, 16);
        let work_32_1 = work(32, 1);
        let work_32_16 = work(32, 16);

        assert_eq!(work_32_16 - work_8_16, work_32_1 - work_8_1);
        assert_eq!(work_32_16 - work_32_1, work_8_16 - work_8_1);
    }

    #[test]
    fn flowchart_elk_projection_work_has_an_independent_exact_budget() {
        // 5 source-index rows + 2 projected nodes + 5 alignment units + 2 layout-index rows
        // + 7 edge units + 12 bounds units + 5 DOM-order units.
        const EXPECTED_PROJECTION_WORK: usize = 38;

        let (model, graph, layout) = projection_fixture();
        let meter = Arc::new(OperationWorkMeter::new(
            crate::resources::RenderResourcePolicy::unbounded_for_trusted_input()
                .with_limit(
                    crate::ResourceLimitId::MaxLayoutWorkUnits,
                    EXPECTED_PROJECTION_WORK,
                )
                .unwrap(),
        ));
        let mut work_control = ElkOperationWorkControl::new(meter);
        let measured = flowchart_layout_from_elk_with_work_control(
            &model,
            &MermaidConfig::default(),
            &graph,
            layout,
            Some(&mut work_control),
        )
        .unwrap();

        assert_eq!(work_control.adapter_work(), EXPECTED_PROJECTION_WORK);

        let (model, graph, layout) = projection_fixture();
        let unmetered =
            flowchart_layout_from_elk(&model, &MermaidConfig::default(), &graph, layout).unwrap();
        assert_eq!(
            serde_json::to_value(&measured).unwrap(),
            serde_json::to_value(&unmetered).unwrap()
        );
        assert_eq!(
            measured.dom_node_order_by_root,
            unmetered.dom_node_order_by_root
        );
    }

    #[test]
    fn flowchart_elk_projection_rejection_does_not_advance_past_completed_work() {
        const WORK_BEFORE_DOM_ORDER: usize = 33;

        let (model, graph, layout) = projection_fixture();
        let meter = Arc::new(OperationWorkMeter::new(
            crate::resources::RenderResourcePolicy::unbounded_for_trusted_input()
                .with_limit(
                    crate::ResourceLimitId::MaxLayoutWorkUnits,
                    WORK_BEFORE_DOM_ORDER,
                )
                .unwrap(),
        ));
        let mut work_control = ElkOperationWorkControl::new(meter);
        let error = flowchart_layout_from_elk_with_work_control(
            &model,
            &MermaidConfig::default(),
            &graph,
            layout,
            Some(&mut work_control),
        )
        .unwrap_err();

        let Error::ResourceLimitExceeded(limit) = error else {
            panic!("expected ResourceLimitExceeded");
        };
        assert_eq!(limit.max, WORK_BEFORE_DOM_ORDER);
        assert_eq!(limit.actual, WORK_BEFORE_DOM_ORDER + 5);
        assert_eq!(work_control.adapter_work(), WORK_BEFORE_DOM_ORDER);
        assert!(work_control.charge_adapter(1).is_err());
        assert_eq!(work_control.adapter_work(), WORK_BEFORE_DOM_ORDER);
    }

    #[test]
    fn flowchart_elk_projection_rejects_the_complete_edge_tranche_atomically() {
        // 6 source-index rows + 2 projected nodes + 8 alignment units + 2 layout-index rows.
        // Two seven-unit edges must be accepted together before the output allocation starts.
        const WORK_BEFORE_EDGES: usize = 18;
        const ONE_EDGE_WORK: usize = 7;

        let (mut model, _, mut layout) = projection_fixture();
        let mut second_source = model.edges[0].clone();
        second_source.id = "L-A-B-2".to_string();
        model.edges.push(second_source);
        let graph = build_flowchart_elk_graph(
            &model,
            &MermaidConfig::default(),
            &crate::text::DeterministicTextMeasurer::default(),
            None,
        )
        .unwrap();
        let mut second_layout = layout.edges[0].clone();
        second_layout.id = "L-A-B-2".to_string();
        layout.edges.push(second_layout);

        let max_work = WORK_BEFORE_EDGES + 2 * ONE_EDGE_WORK - 1;
        let meter = Arc::new(OperationWorkMeter::new(
            crate::resources::RenderResourcePolicy::unbounded_for_trusted_input()
                .with_limit(crate::ResourceLimitId::MaxLayoutWorkUnits, max_work)
                .unwrap(),
        ));
        let mut work_control = ElkOperationWorkControl::new(meter);
        let error = flowchart_layout_from_elk_with_work_control(
            &model,
            &MermaidConfig::default(),
            &graph,
            layout,
            Some(&mut work_control),
        )
        .unwrap_err();

        let Error::ResourceLimitExceeded(limit) = error else {
            panic!("expected ResourceLimitExceeded");
        };
        assert_eq!(limit.actual, WORK_BEFORE_EDGES + 2 * ONE_EDGE_WORK);
        assert_eq!(work_control.adapter_work(), WORK_BEFORE_EDGES);
    }

    #[test]
    fn flowchart_elk_lca_matches_mermaid_endpoint_inclusive_semantics() {
        fn mermaid_common_ancestor(
            left: &str,
            right: &str,
            parent_by_id: &HashMap<String, String>,
        ) -> Option<String> {
            if left == right {
                return parent_by_id.get(left).cloned();
            }
            let mut visited = HashSet::new();
            let mut current = Some(left);
            while let Some(id) = current {
                visited.insert(id);
                if id == right {
                    return Some(id.to_string());
                }
                current = parent_by_id.get(id).map(String::as_str);
            }
            current = Some(right);
            while let Some(id) = current {
                if visited.contains(id) {
                    return Some(id.to_string());
                }
                current = parent_by_id.get(id).map(String::as_str);
            }
            None
        }

        let mut model = nested_model(3, None);
        model.subgraphs.push(subgraph(
            "other-root".to_string(),
            vec!["outside".to_string()],
        ));
        let mut no_work_control = None;
        let parent_by_id = parent_by_id(&model, &mut no_work_control).unwrap();
        let hierarchy = crate::elk_hierarchy::HierarchyIndex::build(
            model
                .nodes
                .iter()
                .map(|node| node.id.as_str())
                .chain(model.subgraphs.iter().map(|subgraph| subgraph.id.as_str()))
                .map(|id| (id, parent_by_id.get(id).map(String::as_str))),
            model.nodes.len() + model.subgraphs.len(),
            &mut no_work_control,
        )
        .unwrap();
        let ids = [
            "leaf",
            "sibling",
            "outside",
            "group-0",
            "group-1",
            "group-2",
            "other-root",
        ];

        for left in ids {
            for right in ids {
                assert_eq!(
                    hierarchy
                        .common_ancestor_id(left, right, &mut no_work_control)
                        .unwrap()
                        .map(str::to_string),
                    mermaid_common_ancestor(left, right, &parent_by_id),
                    "left={left} right={right}"
                );
            }
        }
    }

    #[test]
    fn flowchart_elk_adapter_budget_gates_are_exact_and_rejections_do_not_advance() {
        let model = nested_model(8, Some("outside"));
        let config = MermaidConfig::default();
        let (expected, exact_work) = adapter_graph_and_work(&model, &config);
        let policy = |limit| {
            crate::resources::RenderResourcePolicy::unbounded_for_trusted_input()
                .with_limit(crate::ResourceLimitId::MaxLayoutWorkUnits, limit)
                .unwrap()
        };

        for limit in [exact_work, exact_work + 1] {
            let mut work_control =
                ElkOperationWorkControl::new(Arc::new(OperationWorkMeter::new(policy(limit))));
            let actual = build_flowchart_elk_graph_with_work_control(
                &model,
                &config,
                &crate::text::DeterministicTextMeasurer::default(),
                None,
                Some(&mut work_control),
            )
            .unwrap();
            assert_eq!(work_control.adapter_work(), exact_work);
            assert_eq!(actual.nodes.len(), expected.nodes.len());
            assert_eq!(actual.edges.len(), expected.edges.len());
        }

        let mut below =
            ElkOperationWorkControl::new(Arc::new(OperationWorkMeter::new(policy(exact_work - 1))));
        let error = build_flowchart_elk_graph_with_work_control(
            &model,
            &config,
            &crate::text::DeterministicTextMeasurer::default(),
            None,
            Some(&mut below),
        )
        .unwrap_err();
        let Error::ResourceLimitExceeded(limit) = error else {
            panic!("expected ResourceLimitExceeded");
        };
        assert_eq!(limit.max, exact_work - 1);
        let charged_before_retry = below.adapter_work();
        assert!(charged_before_retry < exact_work);
        assert!(below.charge_adapter(1).is_err());
        assert_eq!(below.adapter_work(), charged_before_retry);
    }

    #[test]
    fn flowchart_elk_adapter_rejects_before_graph_mutation_or_label_measurement() {
        struct CountingMeasurer(std::cell::Cell<usize>);

        impl TextMeasurer for CountingMeasurer {
            fn measure(&self, _text: &str, _style: &TextStyle) -> crate::text::TextMetrics {
                self.0.set(self.0.get() + 1);
                crate::text::TextMetrics {
                    width: 1.0,
                    height: 1.0,
                    line_count: 1,
                }
            }
        }

        let model = nested_model(1, None);
        let config = MermaidConfig::default();
        let measurer = CountingMeasurer(std::cell::Cell::new(0));
        let policy = crate::resources::RenderResourcePolicy::unbounded_for_trusted_input()
            .with_limit(crate::ResourceLimitId::MaxLayoutWorkUnits, 1)
            .unwrap();
        let mut work_control =
            ElkOperationWorkControl::new(Arc::new(OperationWorkMeter::new(policy)));

        let error = build_flowchart_elk_graph_with_work_control(
            &model,
            &config,
            &measurer,
            None,
            Some(&mut work_control),
        )
        .unwrap_err();

        assert!(matches!(error, Error::ResourceLimitExceeded(_)));
        assert_eq!(work_control.adapter_work(), 0);
        assert_eq!(measurer.0.get(), 0);
    }

    #[test]
    fn flowchart_elk_graph_adapter_preserves_basic_nodes_and_edges() {
        let model = model(
            vec![
                node("A", Some("Alpha"), None),
                node("B", Some("Beta"), None),
            ],
            vec![edge("L-A-B", "A", "B", Some("go"))],
        );
        let graph = build_flowchart_elk_graph(
            &model,
            &MermaidConfig::default(),
            &crate::text::DeterministicTextMeasurer::default(),
            None,
        )
        .unwrap();

        assert_eq!(graph.id, "root");
        assert_eq!(graph.direction, elk::Direction::Down);
        assert_eq!(graph.nodes.len(), 2);
        assert_eq!(graph.edges.len(), 1);
        assert!(graph.nodes.iter().all(|n| n.kind == elk::NodeKind::Leaf));
        assert!(graph.nodes.iter().all(|n| n.width > 0.0 && n.height > 0.0));
        assert_eq!(graph.edges[0].source, "A");
        assert_eq!(graph.edges[0].target, "B");
        assert!(graph.edges[0].label.is_some());
    }

    #[test]
    fn flowchart_elk_graph_adapter_preserves_subgraph_parent_mapping() {
        let mut model = model(
            vec![
                node("A", Some("Alpha"), None),
                node("B", Some("Beta"), None),
            ],
            vec![edge("L-A-B", "A", "B", None)],
        );
        model.subgraphs.push(FlowSubgraph {
            metadata: None,
            id: "cluster".to_string(),
            title: "Cluster".to_string(),
            dir: Some("LR".to_string()),
            has_explicit_dir: true,
            label_type: Some("text".to_string()),
            classes: Vec::new(),
            styles: Vec::new(),
            nodes: vec!["A".to_string()],
        });

        let graph = build_flowchart_elk_graph(
            &model,
            &MermaidConfig::default(),
            &crate::text::DeterministicTextMeasurer::default(),
            None,
        )
        .unwrap();

        let cluster = graph.nodes.iter().find(|n| n.id == "cluster").unwrap();
        let child = graph.nodes.iter().find(|n| n.id == "A").unwrap();
        let outside = graph.nodes.iter().find(|n| n.id == "B").unwrap();

        assert_eq!(cluster.kind, elk::NodeKind::Group);
        assert_eq!(cluster.direction, Some(elk::Direction::Right));
        assert_eq!(child.parent.as_deref(), Some("cluster"));
        assert_eq!(outside.parent, None);
    }

    #[test]
    fn flowchart_elk_parent_validation_reports_mermaids_first_assignment_cycle() {
        let mut model = model(Vec::new(), Vec::new());
        model.subgraphs = [("A", "B"), ("B", "A"), ("C", "D"), ("D", "C")]
            .into_iter()
            .map(|(parent, child)| subgraph(parent.to_string(), vec![child.to_string()]))
            .collect();

        let error = build_flowchart_elk_graph(
            &model,
            &MermaidConfig::default(),
            &crate::text::DeterministicTextMeasurer::default(),
            None,
        )
        .unwrap_err();

        let Error::InvalidModel { message } = error else {
            panic!("expected InvalidModel");
        };
        assert_eq!(message, "Setting D as parent of C would create a cycle");
    }

    #[test]
    fn flowchart_elk_adapter_keeps_empty_subgraphs_as_groups() {
        let mut model = model(
            vec![node("a", Some("a"), None), node("b", Some("b"), None)],
            vec![edge("L-a-b", "a", "b", None)],
        );
        model.direction = Some("LR".to_string());
        model.subgraphs.push(FlowSubgraph {
            metadata: None,
            id: "A".to_string(),
            title: "A".to_string(),
            dir: None,
            has_explicit_dir: false,
            label_type: Some("text".to_string()),
            classes: Vec::new(),
            styles: Vec::new(),
            nodes: vec!["a".to_string(), "b".to_string()],
        });
        model.subgraphs.push(FlowSubgraph {
            metadata: None,
            id: "B".to_string(),
            title: "B".to_string(),
            dir: None,
            has_explicit_dir: false,
            label_type: Some("text".to_string()),
            classes: Vec::new(),
            styles: Vec::new(),
            nodes: Vec::new(),
        });

        let graph = build_flowchart_elk_graph(
            &model,
            &MermaidConfig::default(),
            &crate::text::DeterministicTextMeasurer::default(),
            None,
        )
        .unwrap();

        let empty = graph.nodes.iter().find(|node| node.id == "B").unwrap();

        assert_eq!(empty.kind, elk::NodeKind::Group);
        assert_eq!(empty.label.map(|label| label.height), Some(22.0));
    }

    #[test]
    fn flowchart_elk_graph_adapter_matches_mermaid_get_data_node_order() {
        let mut model = model(
            vec![
                node("root-a", Some("root-a"), None),
                node("cluster", Some("cluster"), None),
                node("cluster-a", Some("cluster-a"), None),
                node("cluster-b", Some("cluster-b"), None),
                node("later-cluster", Some("later-cluster"), None),
                node("later-a", Some("later-a"), None),
                node("root-b", Some("root-b"), None),
            ],
            vec![],
        );
        model.subgraphs.push(FlowSubgraph {
            metadata: None,
            id: "cluster".to_string(),
            title: "Cluster".to_string(),
            dir: None,
            has_explicit_dir: false,
            label_type: Some("text".to_string()),
            classes: Vec::new(),
            styles: Vec::new(),
            nodes: vec!["cluster-a".to_string(), "cluster-b".to_string()],
        });
        model.subgraphs.push(FlowSubgraph {
            metadata: None,
            id: "later-cluster".to_string(),
            title: "Later Cluster".to_string(),
            dir: None,
            has_explicit_dir: false,
            label_type: Some("text".to_string()),
            classes: Vec::new(),
            styles: Vec::new(),
            nodes: vec!["later-a".to_string()],
        });

        let graph = build_flowchart_elk_graph(
            &model,
            &MermaidConfig::default(),
            &crate::text::DeterministicTextMeasurer::default(),
            None,
        )
        .unwrap();

        let ids: Vec<&str> = graph.nodes.iter().map(|node| node.id.as_str()).collect();
        assert_eq!(
            ids,
            vec![
                "later-cluster",
                "cluster",
                "root-a",
                "cluster-a",
                "cluster-b",
                "later-a",
                "root-b"
            ]
        );
    }

    #[test]
    fn flowchart_elk_dom_order_matches_mermaid_add_vertices_recursion() {
        let mut model = model(
            vec![
                node("A", Some("A"), None),
                node("B", Some("B"), None),
                node("C", Some("C"), None),
                node("D", Some("D"), None),
                node("E", Some("E"), None),
                node("F", Some("F"), None),
                node("G", Some("G"), None),
            ],
            vec![],
        );
        model.subgraphs.push(FlowSubgraph {
            metadata: None,
            id: "foo".to_string(),
            title: "Foo SubGraph".to_string(),
            dir: None,
            has_explicit_dir: false,
            label_type: Some("text".to_string()),
            classes: Vec::new(),
            styles: Vec::new(),
            nodes: vec!["C".to_string(), "D".to_string()],
        });
        model.subgraphs.push(FlowSubgraph {
            metadata: None,
            id: "bar".to_string(),
            title: "Bar SubGraph".to_string(),
            dir: None,
            has_explicit_dir: false,
            label_type: Some("text".to_string()),
            classes: Vec::new(),
            styles: Vec::new(),
            nodes: vec!["E".to_string(), "F".to_string()],
        });

        let graph = build_flowchart_elk_graph(
            &model,
            &MermaidConfig::default(),
            &crate::text::DeterministicTextMeasurer::default(),
            None,
        )
        .unwrap();

        let ids = mermaid_elk_adapter_dom_order(&graph);
        let actual: Vec<&str> = ids.iter().map(String::as_str).collect();
        assert_eq!(
            actual,
            vec!["bar", "E", "F", "foo", "C", "D", "A", "B", "G"]
        );
    }

    #[test]
    fn flowchart_elk_graph_adapter_maps_elk_layout_options() {
        let model = model(
            vec![
                node("A", Some("Alpha"), None),
                node("B", Some("Beta"), None),
            ],
            vec![edge("L-A-B", "A", "B", None)],
        );
        let config = MermaidConfig::from_value(json!({
            "elk": {
                "mergeEdges": true,
                "layeringStrategy": "COFFMAN_GRAHAM",
                "layeringLayerBound": 2,
                "nodePlacementStrategy": "LINEAR_SEGMENTS",
                "nodePlacementAlignment": "RIGHTDOWN",
                "forceNodeModelOrder": true,
                "considerModelOrder": "PREFER_EDGES",
                "cycleBreakingStrategy": "GREEDY_MODEL_ORDER",
                "layered": {
                    "edgeRouting": {
                        "selfLoopOrdering": "SEQUENCED"
                    }
                },
                "insideSelfLoops": {
                    "activate": true
                }
            }
        }));

        let graph = build_flowchart_elk_graph(
            &model,
            &config,
            &crate::text::DeterministicTextMeasurer::default(),
            None,
        )
        .unwrap();

        assert_eq!(
            graph.options.layered.layering,
            elk::LayeringStrategy::CoffmanGraham
        );
        assert_eq!(graph.options.layered.layering_layer_bound, 2);
        assert!(graph.options.layered.merge_edges);
        assert!(graph.options.layered.force_node_model_order);
        assert!(graph.options.layered.consider_model_order);
        assert_eq!(
            graph.options.layered.model_order,
            elk::ModelOrderStrategy::PreferEdges
        );
        assert_eq!(
            graph.options.layered.cycle_breaking,
            elk::CycleBreakingStrategy::GreedyModelOrder
        );
        assert_eq!(
            graph.options.layered.node_placement,
            elk::NodePlacementStrategy::LinearSegments
        );
        assert_eq!(
            graph.options.layered.node_placement_alignment,
            elk::NodePlacementAlignment::RightDown
        );
        assert!(graph.options.layered.inside_self_loops_activate);
        assert!(graph.options.layered.unnecessary_bendpoints);
        assert!(graph.options.layered.merge_hierarchy_edges);
        assert_eq!(
            graph.options.layered.self_loop_distribution,
            elk::SelfLoopDistributionStrategy::North
        );
        assert_eq!(
            graph.options.layered.self_loop_ordering,
            elk::SelfLoopOrderingStrategy::Sequenced
        );
    }

    #[test]
    fn flowchart_elk_keeps_the_source_default_nonzero_seed() {
        assert_eq!(
            elk_layout_options(&serde_json::Value::Null)
                .layered
                .random_seed,
            1
        );
    }

    #[test]
    fn flowchart_elk_zero_seed_adapter_graph_requires_an_operation_seed() {
        use std::num::NonZeroU64;

        let model = model(
            vec![
                node("A", Some("Alpha"), None),
                node("B", Some("Beta"), None),
            ],
            vec![edge("L-A-B", "A", "B", Some("go"))],
        );
        let mut graph = build_flowchart_elk_graph(
            &model,
            &MermaidConfig::default(),
            &crate::text::DeterministicTextMeasurer::default(),
            None,
        )
        .unwrap();
        graph.options.layered.random_seed = 0;

        assert!(elk::layout(&graph).is_err());

        let operation_seed = elk::ElkOperationSeed::from_operation_seed(
            NonZeroU64::new(0x666c_6f77_6368_6172).expect("nonzero operation seed"),
        );
        let first = elk::layout_with_operation_seed(&graph, operation_seed)
            .expect("seeded Flowchart layout");
        let replayed = elk::layout_with_operation_seed(&graph, operation_seed)
            .expect("replayed seeded Flowchart layout");

        assert_eq!(first, replayed);
    }

    #[test]
    fn flowchart_elk_graph_adapter_defaults_inside_self_loop_edges_to_false() {
        let model = model(
            vec![node("A", Some("Alpha"), None)],
            vec![edge("A-A", "A", "A", None)],
        );

        let graph = build_flowchart_elk_graph(
            &model,
            &MermaidConfig::default(),
            &crate::text::DeterministicTextMeasurer::default(),
            None,
        )
        .unwrap();

        assert!(!graph.edges[0].inside_self_loops_yo);
    }

    #[test]
    fn flowchart_elk_graph_adapter_maps_disabled_model_order() {
        let model = model(
            vec![
                node("A", Some("Alpha"), None),
                node("B", Some("Beta"), None),
            ],
            vec![edge("L-A-B", "A", "B", None)],
        );
        let config = MermaidConfig::from_value(json!({
            "elk": {
                "considerModelOrder": "NONE",
                "cycleBreakingStrategy": "MODEL_ORDER",
                "nodePlacementStrategy": "NETWORK_SIMPLEX"
            }
        }));

        let graph = build_flowchart_elk_graph(
            &model,
            &config,
            &crate::text::DeterministicTextMeasurer::default(),
            None,
        )
        .unwrap();

        assert!(!graph.options.layered.consider_model_order);
        assert_eq!(
            graph.options.layered.model_order,
            elk::ModelOrderStrategy::None
        );
        assert_eq!(
            graph.options.layered.cycle_breaking,
            elk::CycleBreakingStrategy::ModelOrder
        );
        assert_eq!(
            graph.options.layered.node_placement,
            elk::NodePlacementStrategy::NetworkSimplex
        );
    }

    #[test]
    fn flowchart_elk_graph_adapter_marks_cyclic_entry_node_on_top() {
        let model = model(
            vec![
                node("A", Some("Alpha"), None),
                node("B", Some("Beta"), None),
                node("C", Some("Gamma"), None),
            ],
            vec![
                edge("L-A-B", "A", "B", None),
                edge("L-B-C", "B", "C", None),
                edge("L-C-A", "C", "A", None),
            ],
        );
        let config = MermaidConfig::from_value(json!({
            "elk": {
                "keepEntryNodeOnTop": true
            }
        }));

        let graph = build_flowchart_elk_graph(
            &model,
            &config,
            &crate::text::DeterministicTextMeasurer::default(),
            None,
        )
        .unwrap();

        let a = graph.nodes.iter().find(|node| node.id == "A").unwrap();
        let b = graph.nodes.iter().find(|node| node.id == "B").unwrap();
        assert_eq!(a.layer_constraint, Some(elk::LayerConstraint::First));
        assert_eq!(b.layer_constraint, None);
    }

    #[test]
    fn flowchart_elk_cyclic_entry_follows_forward_edges_independently_of_node_order() {
        for declared in [["A", "B", "C"], ["C", "B", "A"]] {
            for first_edge in [0, 1] {
                for nested in [false, true] {
                    for enabled in [false, true] {
                        let mut edges = vec![
                            edge("a-b", "A", "B", None),
                            edge("b-c", "B", "C", None),
                            edge("c-a", "C", "A", None),
                        ];
                        edges.rotate_left(first_edge);
                        let entry = edges[0].from.clone();
                        let mut model = model(
                            declared.iter().map(|id| node(id, Some(id), None)).collect(),
                            edges,
                        );
                        if nested {
                            model.subgraphs.push(subgraph(
                                "G".to_string(),
                                declared.iter().map(|id| (*id).to_owned()).collect(),
                            ));
                        }
                        let config = MermaidConfig::from_value(json!({
                            "elk": { "keepEntryNodeOnTop": enabled }
                        }));
                        let graph = build_flowchart_elk_graph(
                            &model,
                            &config,
                            &crate::text::DeterministicTextMeasurer::default(),
                            None,
                        )
                        .unwrap();
                        let entries = graph
                            .nodes
                            .iter()
                            .filter(|node| {
                                node.layer_constraint == Some(elk::LayerConstraint::First)
                            })
                            .map(|node| node.id.as_str())
                            .collect::<Vec<_>>();
                        let expected = if enabled {
                            vec![entry.as_str()]
                        } else {
                            Vec::new()
                        };
                        assert_eq!(
                            entries, expected,
                            "declared={declared:?}, first_edge={first_edge}, nested={nested}, enabled={enabled}"
                        );
                    }
                }
            }
        }
    }

    #[test]
    fn flowchart_elk_cyclic_group_entry_follows_edge_declaration_order() {
        for group_first in [false, true] {
            let mut model = model(
                vec![node("A", Some("Alpha"), None)],
                vec![
                    edge("group-to-a", "group", "A", None),
                    edge("a-to-group", "A", "group", None),
                ],
            );
            if !group_first {
                model.edges.reverse();
            }
            model
                .subgraphs
                .push(subgraph("group".to_string(), Vec::new()));
            let config = MermaidConfig::from_value(json!({
                "elk": { "keepEntryNodeOnTop": true }
            }));
            let graph = build_flowchart_elk_graph(
                &model,
                &config,
                &crate::text::DeterministicTextMeasurer::default(),
                None,
            )
            .unwrap();
            let entries = graph
                .nodes
                .iter()
                .filter(|node| node.layer_constraint == Some(elk::LayerConstraint::First))
                .map(|node| node.id.as_str())
                .collect::<Vec<_>>();
            assert_eq!(entries, vec![if group_first { "group" } else { "A" }]);
        }
    }

    #[test]
    fn flowchart_elk_missing_endpoint_does_not_override_separate_children() {
        let mut model = model(
            vec![node("A", Some("Alpha"), None)],
            vec![edge("missing-target", "A", "missing", None)],
        );
        let mut group = subgraph("group".to_string(), vec!["A".to_string()]);
        group.dir = Some("LR".to_string());
        group.has_explicit_dir = true;
        model.subgraphs.push(group);

        let graph = build_flowchart_elk_graph(
            &model,
            &MermaidConfig::default(),
            &crate::text::DeterministicTextMeasurer::default(),
            None,
        )
        .unwrap();
        let group = graph.nodes.iter().find(|node| node.id == "group").unwrap();
        assert_eq!(
            group.hierarchy_handling,
            Some(elk::HierarchyHandling::SeparateChildren)
        );
    }

    #[test]
    fn flowchart_elk_graph_adapter_measures_markdown_and_html_labels() {
        let model = model(
            vec![
                node("A", Some("**bold** label"), Some("markdown")),
                node("B", Some("<span>html</span>"), Some("html")),
            ],
            vec![edge("L-A-B", "A", "B", Some("edge"))],
        );
        let graph = build_flowchart_elk_graph(
            &model,
            &MermaidConfig::default(),
            &crate::text::DeterministicTextMeasurer::default(),
            None,
        )
        .unwrap();

        for node in &graph.nodes {
            let label = node.label.expect("node should carry measured label bounds");
            assert!(label.width > 0.0, "label width for {}", node.id);
            assert!(label.height > 0.0, "label height for {}", node.id);
            assert!(node.width >= label.width, "node width for {}", node.id);
            assert!(node.height >= label.height, "node height for {}", node.id);
        }
    }

    #[test]
    fn flowchart_elk_layout_produces_nodes_edges_and_bounds() {
        let model = model(
            vec![
                node("A", Some("Alpha"), None),
                node("B", Some("Beta"), None),
            ],
            vec![edge("L-A-B", "A", "B", Some("go"))],
        );
        let layout = layout_flowchart_elk_typed(
            &model,
            &MermaidConfig::default(),
            &crate::text::DeterministicTextMeasurer::default(),
            None,
        )
        .unwrap();

        let a = layout.nodes.iter().find(|node| node.id == "A").unwrap();
        let b = layout.nodes.iter().find(|node| node.id == "B").unwrap();
        assert!(b.y > a.y);
        assert_eq!(layout.edges.len(), 1);
        assert!(layout.edges[0].points.len() >= 2);
        assert!(layout.edges[0].label.is_some());
        assert!(layout.bounds.is_some());
    }

    #[test]
    fn flowchart_elk_compound_sweep_preserves_upstream_crossing_routes() {
        let source = include_str!(
            "../../../../fixtures/flowchart/stress_flowchart_cluster_dense_children_021.mmd"
        );
        let parsed = Engine::new()
            .parse_diagram_for_render_model_sync(source, ParseOptions::default())
            .expect("parse")
            .expect("diagram");
        let graph = super::build_flowchart_elk_graph(
            &parsed,
            &crate::text::DeterministicTextMeasurer::default(),
            None,
        )
        .expect("graph");
        let layout = elk::layout(&graph).expect("layout");
        // Mermaid 12's committed SVG data-points retain the source router's choice of
        // which crossing edge to split. Both edges share the same endpoint coordinates.
        for (id, expected) in [
            (
                "L_b2_c3_0",
                vec![
                    (415.0, 269.0),
                    (415.0, 311.0),
                    (652.8, 311.0),
                    (652.8, 353.0),
                ],
            ),
            (
                "L_c2_a3_0",
                vec![
                    (652.8, 269.0),
                    (652.8, 299.0),
                    (533.9, 299.0),
                    (533.9, 323.0),
                    (415.0, 323.0),
                    (415.0, 353.0),
                ],
            ),
        ] {
            let actual = &layout
                .edges
                .iter()
                .find(|edge| edge.id == id)
                .unwrap()
                .points;
            assert_eq!(actual.len(), expected.len(), "{id}: {actual:?}");
            for (point, (x, y)) in actual.iter().zip(expected) {
                assert!((point.x - x).abs() < 1e-9, "{id}: {point:?}");
                assert!((point.y - y).abs() < 1e-9, "{id}: {point:?}");
            }
        }
    }

    #[test]
    #[cfg(feature = "layout-elk")]
    fn flowchart_source_backed_elk_uses_exported_edge_label_position() {
        let model = model(
            vec![
                node("A", Some("Alpha"), None),
                node("B", Some("Beta"), None),
                node("C", Some("Gamma"), None),
            ],
            vec![
                edge("L-A-B", "A", "B", None),
                edge("L-B-C", "B", "C", None),
                edge("L-A-C", "A", "C", Some("choice")),
            ],
        );
        let graph = build_flowchart_elk_graph(
            &model,
            &MermaidConfig::default(),
            &crate::text::DeterministicTextMeasurer::default(),
            None,
        )
        .unwrap();
        let raw_layout = elk::layout(&graph).unwrap();
        let raw_edge = raw_layout
            .edges
            .iter()
            .find(|edge| edge.id == "L-A-C")
            .unwrap();
        let raw_label = raw_edge.labels.first().unwrap();

        let layout = layout_flowchart_elk_typed(
            &model,
            &MermaidConfig::default(),
            &crate::text::DeterministicTextMeasurer::default(),
            None,
        )
        .unwrap();

        let edge = layout.edges.iter().find(|edge| edge.id == "L-A-C").unwrap();
        let label = edge.label.as_ref().unwrap();
        assert_eq!(label.x, raw_label.x + raw_label.width / 2.0);
        assert_eq!(label.y, raw_label.y + raw_label.height / 2.0);
        assert_eq!(label.width, raw_label.width);
        assert_eq!(label.height, raw_label.height);
    }

    #[test]
    #[cfg(feature = "layout-elk")]
    fn flowchart_source_backed_elk_recursively_lays_out_directed_subgraphs() {
        let mut model = model(
            vec![
                node("A", Some("A"), None),
                node("B", Some("B"), None),
                node("i1", Some("i1"), None),
                node("f1", Some("f1"), None),
                node("i2", Some("i2"), None),
                node("f2", Some("f2"), None),
            ],
            vec![
                edge("L-i1-f1", "i1", "f1", None),
                edge("L-i2-f2", "i2", "f2", None),
                edge("L-A-TOP", "A", "TOP", None),
                edge("L-TOP-B", "TOP", "B", None),
                edge("L-B1-B2", "B1", "B2", None),
            ],
        );
        model.direction = Some("LR".to_string());
        model.subgraphs.push(FlowSubgraph {
            metadata: None,
            id: "TOP".to_string(),
            title: "TOP".to_string(),
            dir: Some("TB".to_string()),
            has_explicit_dir: true,
            label_type: Some("text".to_string()),
            classes: Vec::new(),
            styles: Vec::new(),
            nodes: vec!["B1".to_string(), "B2".to_string()],
        });
        model.subgraphs.push(FlowSubgraph {
            metadata: None,
            id: "B1".to_string(),
            title: "B1".to_string(),
            dir: Some("RL".to_string()),
            has_explicit_dir: true,
            label_type: Some("text".to_string()),
            classes: Vec::new(),
            styles: Vec::new(),
            nodes: vec!["i1".to_string(), "f1".to_string()],
        });
        model.subgraphs.push(FlowSubgraph {
            metadata: None,
            id: "B2".to_string(),
            title: "B2".to_string(),
            dir: Some("BT".to_string()),
            has_explicit_dir: true,
            label_type: Some("text".to_string()),
            classes: Vec::new(),
            styles: Vec::new(),
            nodes: vec!["i2".to_string(), "f2".to_string()],
        });

        let layout = layout_flowchart_elk_typed(
            &model,
            &MermaidConfig::default(),
            &crate::text::DeterministicTextMeasurer::default(),
            None,
        )
        .unwrap();

        assert_eq!(layout.clusters.len(), 3);
        for id in ["TOP", "B1", "B2"] {
            assert!(layout.clusters.iter().any(|cluster| cluster.id == id));
        }
        let top = layout
            .clusters
            .iter()
            .find(|cluster| cluster.id == "TOP")
            .unwrap();
        let b1 = layout
            .clusters
            .iter()
            .find(|cluster| cluster.id == "B1")
            .unwrap();
        let b2 = layout
            .clusters
            .iter()
            .find(|cluster| cluster.id == "B2")
            .unwrap();
        let i1 = layout.nodes.iter().find(|node| node.id == "i1").unwrap();
        let f1 = layout.nodes.iter().find(|node| node.id == "f1").unwrap();
        let i2 = layout.nodes.iter().find(|node| node.id == "i2").unwrap();
        let f2 = layout.nodes.iter().find(|node| node.id == "f2").unwrap();
        let a = layout.nodes.iter().find(|node| node.id == "A").unwrap();
        let b = layout.nodes.iter().find(|node| node.id == "B").unwrap();

        assert!(a.x < top.x);
        assert!(b.x > top.x);
        assert!(b2.y > b1.y);
        assert!(f1.x < i1.x);
        assert!(f2.y < i2.y);
    }
}
