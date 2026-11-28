mod bounds;
mod config;
mod direction;
#[cfg(feature = "diagram-mindmap")]
mod flat;
mod geometry;

#[cfg(feature = "diagram-mindmap")]
pub(crate) use flat::layout_flat;
#[cfg(any(feature = "diagram-flowchart", feature = "diagram-swimlane"))]
mod prepare;
mod routing;
mod sugiyama;
mod work_budget;
mod working;

use crate::Result;
#[cfg(any(feature = "diagram-flowchart", feature = "diagram-swimlane"))]
use crate::flowchart::FlowchartConfigView;
#[cfg(any(feature = "diagram-flowchart", feature = "diagram-swimlane"))]
use crate::math::MathRenderer;
use crate::model::{
    Bounds, SwimlaneEdgeLayout, SwimlaneLaneLayout, SwimlaneLayout, SwimlaneNodeLayout,
};
use crate::resources::OperationWorkMeter;
#[cfg(any(feature = "diagram-flowchart", feature = "diagram-swimlane"))]
use crate::text::TextMeasurer;
#[cfg(any(feature = "diagram-flowchart", feature = "diagram-swimlane"))]
use merman_core::MermaidConfig;
#[cfg(any(feature = "diagram-flowchart", feature = "diagram-swimlane"))]
use merman_core::diagrams::flowchart::{FlowchartModel, FlowchartRenderContext};
use std::cmp::Ordering;
use std::sync::Arc;

/// Compare identifiers by their UTF-16 code units.
///
/// This matches JavaScript's relational string ordering while remaining independent of host
/// locale and bundled collation data. Every Swimlane algorithmic tie-break uses this comparator so
/// hash-map iteration order cannot affect cycle removal, layering, or lane ordering.
pub(crate) fn deterministic_identifier_cmp(left: &str, right: &str) -> Ordering {
    left.encode_utf16().cmp(right.encode_utf16())
}

fn output_bounds(layout: &working::WorkingLayout) -> Option<Bounds> {
    let mut points = Vec::new();
    for node in layout
        .nodes
        .values()
        .filter(|node| node.kind != working::WorkingNodeKind::Dummy)
    {
        let width = if node.kind == working::WorkingNodeKind::EdgeLabel {
            node.label_width
        } else {
            node.width
        };
        let height = if node.kind == working::WorkingNodeKind::EdgeLabel {
            node.label_height
        } else {
            node.height
        };
        points.push((node.x - width / 2.0, node.y - height / 2.0));
        points.push((node.x + width / 2.0, node.y + height / 2.0));
    }
    for edge in &layout.original_edges {
        points.extend(edge.points.iter().map(|point| (point.x, point.y)));
    }
    Bounds::from_points(points)
}

#[cfg(any(feature = "diagram-flowchart", feature = "diagram-swimlane"))]
pub(crate) fn layout_swimlane_typed_with_work_meter_and_svg_label_sidecar(
    model: &FlowchartModel,
    render_label_sources: &FlowchartRenderContext,
    effective_config: &MermaidConfig,
    measurer: &dyn TextMeasurer,
    math_renderer: Option<&(dyn MathRenderer + Send + Sync)>,
    svg_label_sidecar: Option<&crate::flowchart::FlowchartSvgLabelSidecarBuilder>,
    work_meter: Arc<OperationWorkMeter>,
) -> Result<SwimlaneLayout> {
    let source_nodes = model.nodes.len().saturating_add(model.subgraphs.len());
    let source_edges = model.edges.len();
    work_meter.preflight(swimlane_layout_preflight_work_units(
        source_nodes,
        source_edges,
    ))?;
    work_meter.charge(swimlane_core_layout_work_units(source_nodes, source_edges))?;
    let config = config::SwimlaneConfig::from_config(effective_config);
    let mut working = prepare::prepare(
        model,
        render_label_sources,
        effective_config,
        measurer,
        math_renderer,
        svg_label_sidecar,
    );
    run_layout_core(&mut working, config, work_meter)?;

    // Mermaid's swimlane core only normalizes the implicit `basis` curve to
    // `rounded`; an explicit edge/default/config curve remains authoritative.
    // Resolve the same precedence here before the layout artifact is consumed
    // by the SVG renderer.
    let config_curve = FlowchartConfigView::new(effective_config.as_value()).render_curve();
    let default_curve = model
        .edge_defaults
        .as_ref()
        .and_then(|defaults| defaults.interpolate.as_deref())
        .filter(|curve| !curve.is_empty())
        .or(config_curve.as_deref())
        .unwrap_or("basis");
    let curve_by_id: std::collections::HashMap<&str, &str> = model
        .edges
        .iter()
        .map(|edge| {
            let curve = edge
                .interpolate
                .as_deref()
                .filter(|curve| !curve.is_empty())
                .unwrap_or(default_curve);
            (edge.id.as_str(), curve)
        })
        .collect();

    Ok(project_layout(working, &curve_by_id))
}

fn run_layout_core(
    working: &mut working::WorkingLayout,
    config: config::SwimlaneConfig,
    work_meter: Arc<OperationWorkMeter>,
) -> Result<()> {
    let reversed = sugiyama::run(working, config);
    for edge in &mut working.original_edges {
        edge.reversed_for_layout = reversed.contains(&edge.id);
    }
    bounds::assign_canonical_group_bounds(working);
    let mut work_budget = work_budget::LayoutWorkBudget::for_operation(work_meter);
    routing::route(working, &mut work_budget)?;
    direction::post_process(working, &mut work_budget)?;

    Ok(())
}

fn project_layout(
    working: working::WorkingLayout,
    curve_by_id: &std::collections::HashMap<&str, &str>,
) -> SwimlaneLayout {
    let bounds = output_bounds(&working);
    let nodes = working
        .nodes
        .values()
        .filter(|node| {
            matches!(
                node.kind,
                working::WorkingNodeKind::Content | working::WorkingNodeKind::EdgeLabel
            )
        })
        .map(|node| SwimlaneNodeLayout {
            id: node.id.clone(),
            label: node.label.clone(),
            label_type: node.label_type.clone(),
            shape: node.shape.clone(),
            parent_id: node.parent_id.clone(),
            top_lane_id: node.top_lane_id.clone(),
            x: node.x,
            y: node.y,
            width: node.width,
            height: node.height,
            label_width: node.label_width,
            label_height: node.label_height,
            layer: node.layer,
            order: node.order,
            is_edge_label: node.kind == working::WorkingNodeKind::EdgeLabel,
        })
        .collect();
    let lanes = working
        .nodes
        .values()
        .filter(|node| node.kind == working::WorkingNodeKind::Group)
        .map(|lane| SwimlaneLaneLayout {
            id: lane.id.clone(),
            title: lane.label.clone(),
            parent_id: lane.parent_id.clone(),
            x: lane.x,
            y: lane.y,
            width: lane.width,
            height: lane.height,
            padding: lane.padding,
            title_label_width: lane.label_width,
            title_label_height: lane.label_height,
            content_top: lane.content_top,
            title_rect: lane.title_rect.clone(),
            requested_dir: lane.requested_dir.clone(),
        })
        .collect();
    let edges = working
        .original_edges
        .iter()
        .map(|edge| SwimlaneEdgeLayout {
            id: edge.id.clone(),
            from: edge.from.clone(),
            to: edge.to.clone(),
            points: edge.points.clone(),
            label_node_id: edge.label_node_id.clone(),
            reversed_for_layout: edge.reversed_for_layout,
            curve: match curve_by_id
                .get(edge.id.as_str())
                .copied()
                .unwrap_or("basis")
            {
                "basis" => "rounded",
                curve => curve,
            }
            .to_string(),
        })
        .collect();

    SwimlaneLayout {
        direction: working.direction,
        nodes,
        lanes,
        edges,
        bounds,
    }
}

fn swimlane_core_layout_work_units(nodes: usize, edges: usize) -> usize {
    let baseline = nodes.saturating_add(edges).saturating_mul(4);

    // This is the stable, family-accounted cost for prepare, Sugiyama, and routing. The linear
    // baseline covers their source-item passes. Routing adds edges incrementally and can compare
    // each new route with every earlier route, so charge one conservative unit per unordered
    // source-edge pair. Direction post-processing and SVG line hops charge the shared meter
    // independently and must not be included here.
    baseline.saturating_add(work_budget::unordered_pair_count(edges))
}

fn swimlane_layout_preflight_work_units(nodes: usize, edges: usize) -> usize {
    // Keep preflight as a pure fail-fast estimate. In addition to the core-layout cost, reserve
    // one unordered-pair allowance for the direction post-processing that follows. That phase
    // charges its inspected candidates precisely, so this allowance is intentionally not charged
    // by `swimlane_core_layout_work_units`.
    swimlane_core_layout_work_units(nodes, edges)
        .saturating_add(work_budget::unordered_pair_count(edges))
}

#[cfg(test)]
mod tests {
    use super::{
        deterministic_identifier_cmp, swimlane_core_layout_work_units,
        swimlane_layout_preflight_work_units,
    };

    #[test]
    fn identifier_order_is_stable_utf16_code_unit_order() {
        let mut ids = [
            "B", "a", "A", "b", "a.1", "a-1", "a_1", "Z", "z", "é", "e", "中", "\u{e000}", "😀",
            "🧪",
        ];
        ids.sort_by(|left, right| deterministic_identifier_cmp(left, right));
        assert_eq!(
            ids,
            [
                "A", "B", "Z", "a", "a-1", "a.1", "a_1", "b", "e", "z", "é", "中", "😀", "🧪",
                "\u{e000}",
            ]
        );
    }

    #[test]
    fn core_layout_cost_accounts_routing_pairs_once() {
        // Four nodes plus three edges consume 28 linear units; routing can inspect three
        // unordered edge pairs.
        assert_eq!(swimlane_core_layout_work_units(4, 3), 31);
    }

    #[test]
    fn preflight_reserves_direction_pairs_without_charging_them_to_core() {
        let core = swimlane_core_layout_work_units(4, 3);
        let preflight = swimlane_layout_preflight_work_units(4, 3);

        assert_eq!(core, 31);
        assert_eq!(preflight, 34);
    }

    #[test]
    fn swimlane_layout_work_estimates_saturate() {
        assert_eq!(
            swimlane_core_layout_work_units(usize::MAX, usize::MAX),
            usize::MAX
        );
        assert_eq!(
            swimlane_layout_preflight_work_units(usize::MAX, usize::MAX),
            usize::MAX
        );
    }
}
