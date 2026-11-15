//! Trace payload structures for debugging flowchart edge geometry.
//!
//! These records are emitted only when tracing is enabled through [`SvgDebugOptions`].

use super::super::*;
use crate::svg::{FlowchartEdgeTrace, FlowchartEdgeTracePoint};

pub(in crate::svg::parity::flowchart) fn tp(
    p: &crate::model::LayoutPoint,
) -> FlowchartEdgeTracePoint {
    FlowchartEdgeTracePoint { x: p.x, y: p.y }
}

pub(in crate::svg::parity::flowchart) struct FlowchartEdgeTraceInput<'a> {
    pub(in crate::svg::parity::flowchart) ctx: &'a FlowchartRenderCtx<'a>,
    pub(in crate::svg::parity::flowchart) edge: &'a crate::flowchart::FlowEdge,
    pub(in crate::svg::parity::flowchart) layout_edge: &'a crate::model::LayoutEdge,
    pub(in crate::svg::parity::flowchart) origin_x: f64,
    pub(in crate::svg::parity::flowchart) origin_y: f64,
    pub(in crate::svg::parity::flowchart) base_points: &'a [crate::model::LayoutPoint],
    pub(in crate::svg::parity::flowchart) points_after_intersect_for_trace:
        Option<&'a [crate::model::LayoutPoint]>,
    pub(in crate::svg::parity::flowchart) points_for_render: &'a [crate::model::LayoutPoint],
    pub(in crate::svg::parity::flowchart) points_for_data_points: &'a [crate::model::LayoutPoint],
}

pub(in crate::svg::parity::flowchart) fn record_flowchart_edge_trace(
    input: FlowchartEdgeTraceInput<'_>,
) {
    let FlowchartEdgeTraceInput {
        ctx,
        edge,
        layout_edge,
        origin_x,
        origin_y,
        base_points,
        points_after_intersect_for_trace,
        points_for_render,
        points_for_data_points,
    } = input;

    let trace = FlowchartEdgeTrace {
        fixture_diagram_id: ctx.diagram_id.semantic_str().to_string(),
        edge_id: edge.id.clone(),
        from: edge.from.clone(),
        to: edge.to.clone(),
        layout_from: layout_edge.from.clone(),
        layout_to: layout_edge.to.clone(),
        from_cluster: layout_edge.from_cluster.clone(),
        to_cluster: layout_edge.to_cluster.clone(),
        origin_x,
        origin_y,
        tx: ctx.tx,
        ty: ctx.ty,
        base_points: base_points.iter().map(tp).collect(),
        points_after_intersect: points_after_intersect_for_trace
            .unwrap_or(points_for_data_points)
            .iter()
            .map(tp)
            .collect(),
        points_for_render: points_for_render.iter().map(tp).collect(),
        points_for_data_points: points_for_data_points.iter().map(tp).collect(),
    };

    if let Some(collector) = ctx.trace_collector {
        collector.record(trace);
    }
}
